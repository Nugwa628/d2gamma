//! Fake-event execution uses generated Luau, with bounded in-memory capabilities.
use d2_modkit::graph::{self, Graph, Type};
use mlua::{Lua, LuaOptions, MultiValue, StdLib, Table, Value, VmState};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
};
#[derive(Default, Clone, Debug)]
pub struct Trace {
    pub nodes: Vec<String>,
    pub calls: Vec<String>,
    pub stats: BTreeMap<(u32, i32, i32), i32>,
    pub error: Option<String>,
}
struct World {
    trace: Trace,
    seed: d2_sim::rng::Seed,
    states: BTreeMap<(u32, i32), i32>,
    spawned: u32,
}
fn failure(s: &str) -> mlua::Error {
    mlua::Error::runtime(s)
}
fn int(v: &Value) -> mlua::Result<i32> {
    match v {
        Value::Integer(n) => i32::try_from(*n).map_err(mlua::Error::external),
        Value::Number(n)
            if n.is_finite()
                && n.fract() == 0.0
                && *n >= i32::MIN as f64
                && *n <= i32::MAX as f64 =>
        {
            Ok(*n as i32)
        }
        _ => Err(failure("expected i32")),
    }
}
/// A fresh VM/world for every fire; no game, filesystem or ambient RNG is available.
pub fn fire(graph: &Graph, event: &str, id: i32, seed: u32) -> Result<Trace, String> {
    let source = graph.compile().map_err(|e| e.to_string())?;
    if !graph::EVENTS.contains(&event) {
        return Err("Select an M2 event".into());
    }
    let run = || -> mlua::Result<Trace> {
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH,
            LuaOptions::default(),
        )?;
        lua.set_memory_limit(4 * 1024 * 1024)?;
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
            lua.globals().set(key, Value::Nil)?;
        }
        let math: Table = lua.globals().get("math")?;
        math.set("random", Value::Nil)?;
        math.set("randomseed", Value::Nil)?;
        lua.sandbox(true)?;
        let budget = Arc::new(AtomicU32::new(10_000));
        let remaining = budget.clone();
        lua.set_interrupt(move |_| {
            remaining
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
                .map_err(|_| failure("fake-event VM budget exceeded"))?;
            Ok(VmState::Continue)
        });
        let exports: Table = lua
            .load(&source)
            .set_name("visual graph fake event")
            .eval()?;
        let function: mlua::Function = exports.get(format!("on_{event}"))?;
        let world = RefCell::new(World {
            trace: Trace::default(),
            seed: d2_sim::rng::Seed::new(seed, 666),
            states: BTreeMap::new(),
            spawned: 2,
        });
        let failure = lua
            .scope(|scope| {
                let api = lua.create_table()?;
                for spec in graph::APIS {
                    let world = &world;
                    api.set(
                        spec.name,
                        scope.create_function(move |_, args: MultiValue| {
                            let mut w = world.borrow_mut();
                            if w.trace.calls.len() >= 128 {
                                return Err(failure("fake-event API call budget exceeded"));
                            }
                            if args.len() != spec.inputs.len() {
                                return Err(failure("wrong API argument count"));
                            }
                            for ((_, ty), arg) in spec.inputs.iter().zip(&args) {
                                match ty {
                                    Type::Integer => {
                                        int(arg)?;
                                    }
                                    Type::Boolean if matches!(arg, Value::Boolean(_)) => {}
                                    Type::Text if matches!(arg, Value::String(_)) => {}
                                    Type::Unit => {
                                        let u = int(arg)?;
                                        if u < 1 || u as u32 > w.spawned {
                                            return Err(failure("unknown fake unit"));
                                        }
                                    }
                                    _ => return Err(failure("incompatible API argument")),
                                }
                            }
                            for ((name, ty), arg) in spec.inputs.iter().zip(&args) {
                                if *ty == Type::Integer {
                                    let value = int(arg)?;
                                    let range = match *name {
                                        "stat" | "state" | "layer" | "skill" | "class"
                                        | "missile" | "count" | "x" | "y" => Some((0, 65535)),
                                        "ticks" => Some((0, 2_160_000)),
                                        "frames" => Some((1, 2_160_000)),
                                        "level" => Some((1, 255)),
                                        "lob" => Some((0, 1)),
                                        "pet_type" => Some((0, 18)),
                                        "maximum" => Some((1, 32)),
                                        "upper" => Some((1, i32::MAX)),
                                        "amount" => Some((0, i32::MAX)),
                                        "element" => Some((0, 5)),
                                        _ => None,
                                    };
                                    if range.is_some_and(|(lo, hi)| value < lo || value > hi) {
                                        return Err(failure(
                                            "API integer argument outside M2 range",
                                        ));
                                    }
                                }
                                if *name == "text" {
                                    let Value::String(text) = arg else {
                                        unreachable!()
                                    };
                                    let text = text.to_str()?;
                                    if text.len() > 200
                                        || !text.bytes().all(|b| (32..=126).contains(&b))
                                    {
                                        return Err(failure("message must be printable ASCII"));
                                    }
                                }
                                if *name == "name" {
                                    let Value::String(text) = arg else {
                                        unreachable!()
                                    };
                                    if !d2_proto::mod_ui::valid_name(&text.to_str()?) {
                                        return Err(failure("invalid UI variable name"));
                                    }
                                }
                            }
                            w.trace.calls.push(format!(
                                "{}({})",
                                spec.name,
                                args.iter()
                                    .map(|v| format!("{v:?}"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                            let n = |i: usize| int(&args[i]);
                            let u = n(0)? as u32;
                            if matches!(spec.name, "message" | "ui_variable") && u != 1 {
                                return Err(failure("API requires the fake player (unit 1)"));
                            }
                            let integer = |v: i32| Value::Integer(v.into());
                            Ok(match spec.name {
                                "stat" | "base_stat" => integer(
                                    w.trace.stats.get(&(u, n(1)?, n(2)?)).copied().unwrap_or(0),
                                ),
                                "set_stat" => {
                                    w.trace.stats.insert((u, n(1)?, n(2)?), n(3)?);
                                    Value::Nil
                                }
                                "random" => {
                                    let upper = n(1)?;
                                    if upper < 1 {
                                        return Err(failure("random upper must be positive"));
                                    }
                                    integer(w.seed.roll(upper) as i32)
                                }
                                "has_state" => Value::Boolean(
                                    w.states.get(&(u, n(1)?)).copied().unwrap_or(0) > 0,
                                ),
                                "state_stacks" => {
                                    integer(w.states.get(&(u, n(1)?)).copied().unwrap_or(0))
                                }
                                "apply_state" | "add_stacks" => {
                                    let target = n(1)? as u32;
                                    let count = if spec.name == "add_stacks" { n(5)? } else { 1 };
                                    let value = if spec.name == "add_stacks" {
                                        w.states
                                            .get(&(target, n(2)?))
                                            .copied()
                                            .unwrap_or(0)
                                            .saturating_add(count)
                                            .min(65535)
                                    } else {
                                        1
                                    };
                                    w.states.insert((target, n(2)?), value);
                                    Value::Nil
                                }
                                "clear_state" => {
                                    w.states.remove(&(u, n(1)?));
                                    Value::Nil
                                }
                                "remove_stacks" => {
                                    let s = w.states.entry((u, n(1)?)).or_default();
                                    *s = (*s - n(2)?).max(0);
                                    Value::Nil
                                }
                                "spawn_monster" | "spawn_missile" => {
                                    w.spawned += 1;
                                    Value::Integer(w.spawned.into())
                                }
                                _ => Value::Nil,
                            })
                        })?,
                    )?;
                }
                api.set_readonly(true);
                let context = lua.create_table()?;
                context.set("event", event)?;
                context.set("version", 1)?;
                context.set("unit", 1)?;
                context.set("other", 2)?;
                context.set("id", id)?;
                context.set("frame", 0)?;
                context.set("unit_type", 0)?;
                context.set_readonly(true);
                let record = scope.create_function(|_, node: String| {
                    let mut w = world.borrow_mut();
                    if w.trace.nodes.len() >= 4096 {
                        return Err(failure("trace budget exceeded"));
                    }
                    w.trace.nodes.push(node);
                    Ok(())
                })?;
                function.call::<Value>((context, api, record))?;
                Ok(())
            })
            .err()
            .map(|e| e.to_string());
        let mut trace = world.into_inner().trace;
        trace.error = failure;
        Ok(trace)
    };
    run().map_err(|e| e.to_string())
}
