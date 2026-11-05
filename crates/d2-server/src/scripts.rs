//! Server-owned Luau modules. Only explicit mod layers supply scripts; base
//! games allocate no VM. Modules return a table of functions, isolated from
//! each other and from the filesystem, clock, network and ambient randomness.

use d2_native::layers::LayerStack;
use d2_sim::rules::scripts::{Event, EventKind, Runtime};
use d2_sim::rules::SkillHost;
use d2_sim::units::UnitId;
use mlua::{ChunkMode, Function, Lua, LuaOptions, StdLib, Table, VmState};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

mod api;
#[cfg(test)]
mod events_tests;

/// Hard host limits, independent of mod-controlled data.
const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_MODULES: usize = 64;
const MEMORY_BYTES: usize = 4 * 1024 * 1024;
const CHECKPOINTS: u32 = 10_000;

/// Effective source set; later layers replace a module at the same path.
#[derive(Clone, Debug, Default)]
pub struct Sources(BTreeMap<String, Vec<u8>>);

impl Sources {
    /// Load and check all statically declared skill/trigger call targets.
    pub fn instantiate_for(
        &self,
        rules: &d2_sim::rules::Ruleset,
    ) -> Result<Option<ScriptHost>, String> {
        let host = self.instantiate()?;
        if let d2_sim::rules::Ruleset::Mod(rules) = rules {
            for call in rules.script_calls() {
                let Some(host) = &host else {
                    return Err(format!(
                        "files/scripts/{}::{}: script runtime is not installed",
                        call.module, call.function
                    ));
                };
                host.export(&call.module, &call.function)?;
            }
        }
        Ok(host)
    }

    /// Read only mod layers. Folder and pack paths use the same layer reader.
    pub fn from_layers(layers: &LayerStack) -> Result<Self, String> {
        let mut sources = BTreeMap::new();
        for layer in layers.layers.iter().skip(1) {
            for name in layer.files_under("scripts").map_err(|e| e.to_string())? {
                if !name.ends_with(".luau") && !name.ends_with(".graph.toml") {
                    continue;
                }
                let path = format!("scripts/{name}");
                let bytes = layer
                    .read(&path)
                    .ok_or_else(|| format!("{path}: missing"))?
                    .map_err(|e| e.to_string())?;
                if bytes.len() > MAX_SOURCE_BYTES {
                    return Err(format!("files/{path}: script source exceeds 1 MiB"));
                }
                sources.insert(name, bytes);
                if sources.len() > MAX_MODULES
                    || sources.values().map(Vec::len).sum::<usize>() > MAX_SOURCE_BYTES
                {
                    return Err(
                        "files/scripts: at most 64 modules and 1 MiB total source allowed".into(),
                    );
                }
            }
        }
        for (name, bytes) in &mut sources {
            if name.ends_with(".graph.toml") {
                let compile = || {
                    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
                    d2_modkit::graph::Graph::parse(text)
                        .and_then(|g| g.compile())
                        .map(String::into_bytes)
                        .map_err(|e| e.to_string())
                };
                *bytes = compile().map_err(|e| format!("files/scripts/{name}: {e}"))?;
            }
        }
        if sources.values().map(Vec::len).sum::<usize>() > MAX_SOURCE_BYTES {
            return Err("files/scripts: generated modules exceed 1 MiB total source".into());
        }
        Ok(Self(sources))
    }

    /// Empty sources never initialize Luau, including the no-mod path.
    pub fn instantiate(&self) -> Result<Option<ScriptHost>, String> {
        if self.0.is_empty() {
            return Ok(None);
        }
        let mut modules = BTreeMap::new();
        for (name, bytes) in &self.0 {
            let module =
                Module::load(bytes, name).map_err(|e| format!("files/scripts/{name}: {e}"))?;
            modules.insert(name.clone(), module);
        }
        Ok(Some(ScriptHost {
            modules,
            busy: AtomicBool::new(false),
            timers: Mutex::new(Vec::new()),
            callbacks: Mutex::new((i32::MIN, 0)),
        }))
    }
}

struct Module {
    lua: Lua,
    exports: Table,
    remaining: Arc<AtomicU32>,
}

impl Module {
    fn load(bytes: &[u8], name: &str) -> mlua::Result<Self> {
        let text = std::str::from_utf8(bytes).map_err(mlua::Error::external)?;
        // TEXT_ONLY is enforced on the chunk too: untrusted bytecode must
        // never be handed to Luau's VM.
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH,
            LuaOptions::default(),
        )?;
        lua.set_memory_limit(MEMORY_BYTES)?;
        let globals = lua.globals();
        for key in [
            "collectgarbage",
            "loadstring",
            "getfenv",
            "setfenv",
            "require",
            "print",
            "warn",
            "pcall",
            "xpcall",
            "newproxy",
            "gcinfo",
        ] {
            globals.set(key, mlua::Value::Nil)?;
        }
        // Randomness is supplied by the simulation API in S2. No hidden
        // random seed or real-time clock may affect a game.
        let math: Table = globals.get("math")?;
        math.set("random", mlua::Value::Nil)?;
        math.set("randomseed", mlua::Value::Nil)?;
        globals.set("os", mlua::Value::Nil)?;
        globals.set("io", mlua::Value::Nil)?;
        globals.set("SCRIPT_API_VERSION", 1)?;
        lua.sandbox(true)?;
        let remaining = Arc::new(AtomicU32::new(CHECKPOINTS));
        let budget = remaining.clone();
        lua.set_interrupt(move |_| {
            if budget
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
                .is_err()
            {
                return Err(mlua::Error::runtime("script execution budget exceeded"));
            }
            Ok(VmState::Continue)
        });
        let exports = lua
            .load(text)
            .set_name(format!("files/scripts/{name}"))
            .set_mode(ChunkMode::Text)
            .eval::<Table>()?;
        for event in EventKind::ALL {
            let name = format!("on_{}", event.name());
            match exports.raw_get::<mlua::Value>(name.as_str())? {
                mlua::Value::Nil | mlua::Value::Function(_) => {}
                _ => return Err(mlua::Error::runtime(format!("{name} must be a function"))),
            }
        }
        Ok(Self {
            lua,
            exports,
            remaining,
        })
    }
}

/// One host per authoritative game. Never installed into a client ruleset.
pub struct ScriptHost {
    modules: BTreeMap<String, Module>,
    busy: AtomicBool,
    timers: Mutex<Vec<Timer>>,
    callbacks: Mutex<(i32, u16)>,
}

struct Timer {
    module: String,
    unit: UnitId,
    at: i32,
    cookie: i32,
}

struct Busy<'a>(&'a AtomicBool);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl ScriptHost {
    pub(crate) fn export(&self, module: &str, function: &str) -> Result<(), String> {
        let result = self
            .modules
            .get(module)
            .ok_or_else(|| "unknown script module".to_owned())
            .and_then(|m| {
                m.exports
                    .raw_get::<Function>(function)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            });
        result.map_err(|e| format!("files/scripts/{module}::{function}: {e}"))
    }
    /// Integer-only foundation for event/block callers. Errors name the
    /// module and function so callers can put them in a check report or log.
    pub fn call(&self, module: &str, function: &str, argument: i32) -> Result<i32, String> {
        let run = || -> mlua::Result<i32> {
            let m = self
                .modules
                .get(module)
                .ok_or_else(|| mlua::Error::runtime("unknown script module"))?;
            m.remaining.store(CHECKPOINTS, Ordering::Relaxed);
            m.lua.gc_collect()?;
            let f: Function = m.exports.raw_get(function)?;
            let result = f.call::<mlua::Value>(argument)?;
            match result {
                mlua::Value::Integer(value) => i32::try_from(value).map_err(mlua::Error::external),
                _ => Err(mlua::Error::runtime("script must return an i32 integer")),
            }
        };
        run().map_err(|e| format!("files/scripts/{module}::{function}: {e}"))
    }
}

impl ScriptHost {
    fn dispatch(&self, host: &mut dyn SkillHost, event: Event, only: Option<&str>) {
        // Engine operations may synchronously raise events. Do not recursively
        // enter a VM or borrow the same world capability twice.
        if self.busy.swap(true, Ordering::Acquire) {
            return;
        }
        let _busy = Busy(&self.busy);
        for (name, module) in &self.modules {
            if only.is_some_and(|only| name != only) {
                continue;
            }
            let function = format!("on_{}", event.kind.name());
            let mut run = || -> mlua::Result<()> {
                let Some(f) = module
                    .exports
                    .raw_get::<Option<Function>>(function.as_str())?
                else {
                    return Ok(());
                };
                {
                    let mut count = self
                        .callbacks
                        .lock()
                        .map_err(|_| mlua::Error::runtime("callback store unavailable"))?;
                    if count.0 != host.frame() {
                        *count = (host.frame(), 0);
                    }
                    if count.1 >= 256 {
                        return Ok(());
                    }
                    count.1 += 1;
                }
                module.remaining.store(CHECKPOINTS, Ordering::Relaxed);
                module.lua.gc_collect()?;
                api::run(self, name, module, &f, host, event, None).map(|_| ())
            };
            if let Err(error) = run() {
                host.script_error(&format!("files/scripts/{name}::{function}: {error}"));
            }
        }
    }
}

impl Runtime for ScriptHost {
    fn call(
        &self,
        host: &mut dyn SkillHost,
        spec: &d2_sim::rules::scripts::CallSpec,
        context: d2_sim::rules::scripts::CallContext,
    ) -> Result<i32, String> {
        let prefix = || format!("files/scripts/{}::{}", spec.module, spec.function);
        if self.busy.swap(true, Ordering::Acquire) {
            return Err(format!("{}: recursive script call refused", prefix()));
        }
        let _busy = Busy(&self.busy);
        let mut run = || -> mlua::Result<i32> {
            let m = self
                .modules
                .get(&spec.module)
                .ok_or_else(|| mlua::Error::runtime("unknown script module"))?;
            let f: Function = m.exports.raw_get(spec.function.as_str())?;
            {
                let mut count = self
                    .callbacks
                    .lock()
                    .map_err(|_| mlua::Error::runtime("callback store unavailable"))?;
                if count.0 != host.frame() {
                    *count = (host.frame(), 0);
                }
                if count.1 >= 256 {
                    return Err(mlua::Error::runtime("script callback limit exceeded"));
                }
                count.1 += 1;
            }
            m.remaining.store(CHECKPOINTS, Ordering::Relaxed);
            m.lua.gc_collect()?;
            let event = Event {
                kind: EventKind::SkillUsed,
                unit: context.unit,
                other: context.other,
                id: context.argument,
            };
            let result = api::run(self, &spec.module, m, &f, host, event, Some(context))?;
            api::integer(&result)
        };
        run().map_err(|e| format!("{}: {e}", prefix()))
    }

    fn event(&self, host: &mut dyn SkillHost, event: Event) {
        self.dispatch(host, event, None);
    }
    fn tick(&self, host: &mut dyn SkillHost) {
        let mut due = Vec::new();
        if let Ok(mut timers) = self.timers.lock() {
            let mut future = Vec::new();
            for timer in timers.drain(..) {
                if timer.at <= host.frame() {
                    due.push(timer);
                } else {
                    future.push(timer);
                }
            }
            *timers = future;
        }
        // Stable ordering preserves insertion order for equal deadlines.
        due.sort_by_key(|t| t.at);
        for timer in due {
            if host.unit_type(timer.unit).is_some() {
                self.dispatch(
                    host,
                    Event {
                        kind: EventKind::Timer,
                        unit: timer.unit,
                        other: None,
                        id: timer.cookie,
                    },
                    Some(&timer.module),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn source(text: &str) -> Sources {
        Sources(BTreeMap::from([(
            "test.luau".into(),
            text.as_bytes().to_vec(),
        )]))
    }

    #[test]
    fn base_game_has_no_vm() {
        assert!(Sources::default().instantiate().unwrap().is_none());
    }

    #[test]
    fn sandbox_and_integer_boundary() {
        let host = source(
            r#"
            assert(io == nil and os == nil and require == nil and debug == nil)
            assert(loadstring == nil and getfenv == nil and setfenv == nil)
            assert(pcall == nil and xpcall == nil and coroutine == nil)
            assert(math.random == nil and math.randomseed == nil)
            assert(SCRIPT_API_VERSION == 1)
            return { twice = function(n) return n * 2 end,
                     fraction = function() return 0.5 end }
        "#,
        )
        .instantiate()
        .unwrap()
        .unwrap();
        assert_eq!(host.call("test.luau", "twice", 12).unwrap(), 24);
        assert!(host
            .call("test.luau", "fraction", 0)
            .unwrap_err()
            .contains("integer"));
        assert!(host
            .call("test.luau", "missing", 0)
            .unwrap_err()
            .contains("test.luau::missing"));
    }

    #[test]
    fn budget_applies_to_load_and_each_call() {
        assert!(source("while true do end")
            .instantiate()
            .err()
            .unwrap()
            .contains("budget"));
        let host = source(
            "return { spin = function() while true do end end, ok = function(n) return n end }",
        )
        .instantiate()
        .unwrap()
        .unwrap();
        for _ in 0..2 {
            assert!(host
                .call("test.luau", "spin", 0)
                .unwrap_err()
                .contains("budget"));
            assert_eq!(host.call("test.luau", "ok", 7).unwrap(), 7);
        }
    }

    #[test]
    fn memory_errors_and_syntax_errors_name_source() {
        for text in [
            "return {",
            "local t = {}; for i=1,100 do t[i]=string.rep(tostring(i), 100000) end; return t",
        ] {
            assert!(source(text)
                .instantiate()
                .err()
                .unwrap()
                .contains("files/scripts/test.luau"));
        }
    }

    #[test]
    fn reload_is_a_fresh_checked_host() {
        let old = source("return { value = function() return 1 end }")
            .instantiate()
            .unwrap()
            .unwrap();
        let new = source("return { value = function() return 2 end }")
            .instantiate()
            .unwrap()
            .unwrap();
        assert_eq!(old.call("test.luau", "value", 0).unwrap(), 1);
        assert_eq!(new.call("test.luau", "value", 0).unwrap(), 2);
    }

    #[test]
    fn builtin_mutation_and_runtime_memory_exhaustion_fail() {
        assert!(source("math.floor = nil; return {}").instantiate().is_err());
        let host = source("return { grow = function() local t = {}; for i=1,100 do t[i]=string.rep(tostring(i),100000) end; return 0 end }").instantiate().unwrap().unwrap();
        assert!(host
            .call("test.luau", "grow", 0)
            .unwrap_err()
            .contains("memory"));
    }

    #[test]
    fn folder_pack_layers_replace_and_never_read_base() {
        use d2_native::layers::{Layer, LayerFiles};
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "script-layers-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let first = root.join("first");
        let second = root.join("second");
        for (dir, value) in [(&first, 1), (&second, 2)] {
            std::fs::create_dir_all(dir.join("files/scripts/nested")).unwrap();
            std::fs::write(dir.join("mod.toml"), "synthetic").unwrap();
            std::fs::write(
                dir.join("files/scripts/nested/test.luau"),
                format!("return {{ value = function() return {value} end }}"),
            )
            .unwrap();
        }
        let pack = root.join("second.d2pack");
        d2_native::pack::build(&second, &pack, 1).unwrap();
        let base = Layer::dir("base", first.join("files"), None);
        assert!(Sources::from_layers(&LayerStack {
            layers: vec![base.clone()]
        })
        .unwrap()
        .instantiate()
        .unwrap()
        .is_none());
        let folder = Layer::dir("first", first.join("files"), None);
        let packed = Layer {
            name: "second".into(),
            files: LayerFiles::Pack(Arc::new(d2_native::pack::Pack::open(&pack).unwrap())),
            info: None,
        };
        for last in [Layer::dir("second", second.join("files"), None), packed] {
            let sources = Sources::from_layers(&LayerStack {
                layers: vec![base.clone(), folder.clone(), last],
            })
            .unwrap();
            assert_eq!(
                sources
                    .instantiate()
                    .unwrap()
                    .unwrap()
                    .call("nested/test.luau", "value", 0)
                    .unwrap(),
                2
            );
        }
        let layers = LayerStack {
            layers: vec![base, folder],
        };
        let scripts = first.join("files/scripts");
        std::fs::write(scripts.join("large.luau"), vec![b' '; MAX_SOURCE_BYTES + 1]).unwrap();
        assert!(Sources::from_layers(&layers)
            .unwrap_err()
            .contains("exceeds 1 MiB"));
        std::fs::write(scripts.join("large.luau"), vec![b' '; MAX_SOURCE_BYTES]).unwrap();
        assert!(Sources::from_layers(&layers)
            .unwrap_err()
            .contains("total source"));
        std::fs::remove_file(scripts.join("large.luau")).unwrap();
        for n in 0..MAX_MODULES {
            std::fs::write(scripts.join(format!("extra{n}.luau")), "return {}").unwrap();
        }
        assert!(Sources::from_layers(&layers)
            .unwrap_err()
            .contains("64 modules"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod graph_tests;
