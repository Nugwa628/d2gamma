//! Folder mod development: check and compile on a worker, then offer a
//! process restart from a dev slot. The running world's data never changes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use bevy::prelude::*;

use super::overlay::DevOverlay;

const QUIET: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(200);
pub const SLOT: &str = "mod-reload";

#[derive(Clone)]
struct Config {
    folder: PathBuf,
    native: PathBuf,
}

static CONFIG: OnceLock<Config> = OnceLock::new();
static WINDOW: OnceLock<IVec2> = OnceLock::new();
static RESTORE_ERROR: OnceLock<String> = OnceLock::new();
struct Restart {
    args: Vec<String>,
    save: PathBuf,
    slot: PathBuf,
}

static RESTART: Mutex<Option<Restart>> = Mutex::new(None);

pub fn restart_pending() -> bool {
    RESTART.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

/// Only an explicit folder development launch enables the watcher. Installed
/// mods, including folders selected with `--mod`, retain normal behavior.
pub fn configure(args: &[String], native: &Path, online: bool) {
    if let Some(folder) = folder_arg(args, online) {
        let _ = CONFIG.set(Config {
            folder,
            native: native.to_owned(),
        });
    }
}

fn folder_arg(args: &[String], online: bool) -> Option<PathBuf> {
    if online {
        return None;
    }
    args.iter().enumerate().find_map(|(i, a)| {
        a.strip_prefix("--mod-dir=")
            .or_else(|| {
                (a == "--mod-dir")
                    .then(|| args.get(i + 1).map(String::as_str))
                    .flatten()
            })
            .map(PathBuf::from)
    })
}

/// Private restart metadata, consumed before the regular play parser.
pub fn take_window_arg(args: &mut Vec<String>) -> Result<(), String> {
    if let Some(i) = args.iter().position(|a| a == "--dev-window") {
        if i + 1 >= args.len() {
            return Err("--dev-window needs X,Y".into());
        }
        let value = args.remove(i + 1);
        args.remove(i);
        let (x, y) = value.split_once(',').ok_or("--dev-window needs X,Y")?;
        let pos = IVec2::new(
            x.parse().map_err(|_| "bad window X")?,
            y.parse().map_err(|_| "bad window Y")?,
        );
        let _ = WINDOW.set(pos);
    }
    Ok(())
}

pub fn window_position() -> bevy::window::WindowPosition {
    WINDOW
        .get()
        .copied()
        .map(bevy::window::WindowPosition::At)
        .unwrap_or_default()
}

/// A changed table can make a snapshot character unreadable. The caller
/// starts a fresh, unsaved character and shows this explanation in game.
pub fn restore_failed(error: String) {
    let _ = RESTORE_ERROR.set(error);
}

pub fn restore_error() -> Option<&'static str> {
    RESTORE_ERROR.get().map(String::as_str)
}

pub fn is_reload_slot(path: Option<&Path>) -> bool {
    CONFIG.get().is_some() && path.and_then(Path::file_name).is_some_and(|n| n == SLOT)
}

/// Called only after Bevy has left/saved the old game and dropped its window.
pub fn finish_restart() -> std::io::Result<Option<u8>> {
    RESTART
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .map(|r| {
            // The server's final leave has finished writing the regular
            // save. Restore the explicit snapshot just as dev `load` does.
            std::fs::copy(r.slot.join("char.d2s"), r.save)?;
            if let Some(input) = super::super::control::transport::take_restart_input() {
                let mut child = std::process::Command::new(std::env::current_exe()?)
                    .args(&r.args)
                    .stdin(std::process::Stdio::piped())
                    .spawn()?;
                if let Some(stdin) = child.stdin.take() {
                    std::thread::spawn(move || {
                        super::super::control::transport::forward_input(input, stdin)
                    });
                }
                Ok(child
                    .wait()?
                    .code()
                    .and_then(|c| u8::try_from(c).ok())
                    .unwrap_or(crate::launch::exit::ERROR))
            } else {
                crate::kit_mod::relaunch_self(&r.args)
            }
        })
        .transpose()
}

#[derive(Default)]
struct Debounce {
    observed: Option<u64>,
    changed: Option<Instant>,
}

impl Debounce {
    fn observe(&mut self, fingerprint: u64, now: Instant) -> bool {
        if self.observed != Some(fingerprint) {
            let first = self.observed.is_none();
            self.observed = Some(fingerprint);
            self.changed = (!first).then_some(now);
            return false;
        }
        if self.changed.is_some_and(|t| now.duration_since(t) >= QUIET) {
            self.changed = None;
            return true;
        }
        false
    }
}

/// Hash file contents as well as names: same-size edits and atomic editor
/// renames count. Do not follow symlinks or walk the project's git database.
fn fingerprint(root: &Path) -> Result<u64, String> {
    fn walk(path: &Path, root: bool, hash: &mut DefaultHasher) -> std::io::Result<()> {
        let mut entries = std::fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            // A source folder may also be the mod's user folder. Saves,
            // logs and dev slots must never trigger their own reload check.
            if root && e.file_name() != "mod.toml" && e.file_name() != "files" {
                continue;
            }
            if e.file_name() == ".git" {
                continue;
            }
            let kind = e.file_type()?;
            e.file_name().hash(hash);
            if kind.is_dir() {
                walk(&e.path(), false, hash)?;
            } else if kind.is_file() {
                std::fs::read(e.path())?.hash(hash);
            }
        }
        Ok(())
    }
    let mut hash = DefaultHasher::new();
    walk(root, true, &mut hash).map_err(|e| format!("{}: {e}", root.display()))?;
    Ok(hash.finish())
}

/// Re-resolve manifests and rules, compile patches and check references,
/// then decode every overridden native asset before offering the restart.
fn check(config: &Config) -> Result<(), String> {
    use super::super::single_player::{GameData, GameSource, Layers};
    use d2_native::source::{AssetKind, NativeSource};
    let stack = d2_modkit::resolve(
        &d2_modkit::ModSpec::Dir(config.folder.clone()),
        &crate::user_dir::mods_dir(),
    )
    .map_err(|e| e.to_string())?;
    same_mod(
        crate::kit_mod::active().map(|s| s.picked().manifest.id.as_str()),
        &stack.picked().manifest.id,
    )?;
    let rules = crate::kit_mod::host(&stack)
        .map_err(|e| e.to_string())?
        .into_ruleset();
    let entries = stack.layer_entries();
    GameData::open_with(
        &GameSource::Native(config.native.clone(), Layers::List(entries.clone())),
        rules.data_rules(),
    )
    .map_err(|e| e.to_string())?;
    let source =
        NativeSource::open_with(&config.native, Some(&entries)).map_err(|e| e.to_string())?;
    d2_server::scripts::Sources::from_layers(&source.layers)?.instantiate_for(&rules)?;
    for layer in source.layers.layers.iter().skip(1) {
        for file in layer.files_under("").map_err(|e| e.to_string())? {
            let canonical = [".toml", ".tsv", ".pal"]
                .iter()
                .find_map(|suffix| file.strip_suffix(*suffix))
                .unwrap_or(&file);
            if AssetKind::of(canonical).is_some() {
                if let Some(result) = source.read_native(canonical) {
                    result?;
                }
            }
        }
    }
    Ok(())
}

fn same_mod(previous: Option<&str>, picked: &str) -> Result<(), String> {
    if previous.is_some_and(|id| id != picked) {
        return Err("mod.toml: the mod id changed; start a fresh Studio Play session instead of restoring another mod's character".into());
    }
    Ok(())
}

struct Checked {
    restart: bool,
    result: Result<(), String>,
}

#[derive(Resource)]
pub struct Watcher {
    results: Mutex<mpsc::Receiver<Checked>>,
    request: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub fn install(app: &mut impl crate::app::session::Setup) {
    let Some(config) = CONFIG.get().cloned() else {
        return;
    };
    let (tx, rx) = mpsc::channel();
    let request = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let (ask, done) = (request.clone(), stop.clone());
    std::thread::spawn(move || {
        let mut debounce = Debounce::default();
        let mut last_error = None;
        let mut requested = false;
        while !done.load(Ordering::Relaxed) {
            requested |= ask.swap(false, Ordering::Relaxed);
            let now = Instant::now();
            match fingerprint(&config.folder) {
                Ok(before) => {
                    let recovered = last_error.take().is_some();
                    let changed = debounce.observe(before, now);
                    if recovered {
                        debounce.changed = Some(now);
                    }
                    if changed || (requested && debounce.changed.is_none()) {
                        let result = check(&config);
                        if fingerprint(&config.folder) == Ok(before) {
                            if tx
                                .send(Checked {
                                    restart: requested,
                                    result,
                                })
                                .is_err()
                            {
                                break;
                            }
                            requested = false;
                        } else {
                            debounce.changed = Some(Instant::now());
                        }
                    }
                }
                Err(e) => {
                    if last_error.as_ref() != Some(&e) {
                        let _ = tx.send(Checked {
                            restart: false,
                            result: Err(e.clone()),
                        });
                        last_error = Some(e);
                    }
                }
            }
            std::thread::sleep(POLL);
        }
    });
    app.register("mod-dev-reload", register);
    app.world_mut().insert_resource(Watcher {
        results: Mutex::new(rx),
        request,
        stop,
    });
}

pub fn register(app: &mut App) {
    app.add_systems(Update, poll.run_if(resource_exists::<Watcher>));
}

pub fn request(watcher: Option<&Watcher>) -> Result<(), String> {
    let watcher = watcher.ok_or("reload needs an offline --mod-dir game with dev tools enabled")?;
    watcher.request.store(true, Ordering::Relaxed);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn poll(
    watcher: Res<Watcher>,
    mut overlay: ResMut<DevOverlay>,
    ctx: Option<Res<super::info::DevContext>>,
    saver: Option<Res<crate::app::save::SaveHandle>>,
    bridge: Option<Res<crate::bridge::BridgeResource>>,
    window: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    let results: Vec<_> = watcher
        .results
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .try_iter()
        .collect();
    for checked in results {
        let result = checked.result.and_then(|()| {
            if !checked.restart {
                return Ok("Mod check passed. Type reload to rebuild and restart here.".to_owned());
            }
            let ctx = ctx.as_deref().ok_or("reload: no offline game context")?;
            super::overlay::save_slot(
                ctx,
                saver.as_deref(),
                bridge.as_deref(),
                &overlay.console,
                SLOT,
            )?;
            let save = saver.as_deref().ok_or("reload: no character save")?.path();
            let slot = super::slots::slot_dir(&ctx.dir, SLOT);
            let position = window.iter().next().and_then(|w| match w.position {
                bevy::window::WindowPosition::At(p) => Some(p),
                _ => None,
            });
            let args = restart_args(
                &std::env::args().skip(1).collect::<Vec<_>>(),
                save,
                &slot,
                ctx.seed,
                ctx.difficulty,
                position,
            );
            *RESTART.lock().unwrap_or_else(|e| e.into_inner()) = Some(Restart {
                args,
                save: save.to_owned(),
                slot,
            });
            exit.write(AppExit::Success);
            Ok("Mod checked; restarting from the mod-reload slot.".into())
        });
        report(&mut overlay, result);
    }
}

fn report(overlay: &mut DevOverlay, result: Result<String, String>) {
    overlay.open = true;
    match result {
        Ok(text) => overlay.console.say(true, text),
        Err(error) => overlay.console.say(false, format!("Mod reload: {error}")),
    }
}

/// Preserve the install, folder mod and display/control options, replacing
/// only the old character start and dev-slot metadata.
fn restart_args(
    original: &[String],
    save: &Path,
    slot: &Path,
    seed: u32,
    difficulty: u8,
    window: Option<IVec2>,
) -> Vec<String> {
    let mut args = Vec::new();
    let mut i = 0;
    while i < original.len() {
        let flag = original[i].split('=').next().unwrap_or("");
        let count = match flag {
            "--new" => 3,
            "--save" | "--seed" | "--difficulty" | "--dev-start" | "--dev-window" => 2,
            _ => 0,
        };
        if count == 0 {
            args.push(original[i].clone());
            i += 1;
        } else {
            i += if original[i].contains('=') { 1 } else { count };
        }
    }
    args.extend([
        "--save".into(),
        save.display().to_string(),
        "--seed".into(),
        seed.to_string(),
        "--difficulty".into(),
        difficulty.to_string(),
        "--dev-start".into(),
        slot.display().to_string(),
    ]);
    if let Some(p) = window {
        args.extend(["--dev-window".into(), format!("{},{}", p.x, p.y)]);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_check_reports_in_overlay_without_exiting_running_app() {
        let (tx, rx) = mpsc::channel();
        let mut app = App::new();
        app.add_message::<AppExit>();
        app.insert_resource(Watcher {
            results: Mutex::new(rx),
            request: Arc::new(AtomicBool::new(false)),
            stop: Arc::new(AtomicBool::new(false)),
        });
        app.insert_resource(DevOverlay {
            open: false,
            console: Default::default(),
            chan: Default::default(),
            panel: Default::default(),
            catalog: Arc::new(Default::default()),
            local: Vec::new(),
        });
        register(&mut app);
        tx.send(Checked {
            restart: true,
            result: Err("patches/items.patch: unknown item reference".into()),
        })
        .unwrap();
        app.update();
        let overlay = app.world().resource::<DevOverlay>();
        assert!(overlay.open);
        assert!(overlay
            .console
            .lines
            .back()
            .unwrap()
            .1
            .contains("items.patch"));
        assert!(!overlay.console.lines.back().unwrap().0);
        assert!(app.world().resource::<Messages<AppExit>>().is_empty());
        assert!(!restart_pending());
        app.update();
        assert!(app.world().resource::<Messages<AppExit>>().is_empty());
    }

    #[test]
    fn content_edits_adds_and_deletes_are_detected_and_bad_manifest_rejected() {
        let dir = std::env::temp_dir().join(format!("d2-mod-reload-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("files")).unwrap();
        std::fs::write(dir.join("mod.toml"), "bad!").unwrap();
        let first = fingerprint(&dir).unwrap();
        std::fs::create_dir_all(dir.join("logs")).unwrap();
        std::fs::write(dir.join("logs/game.log"), "game is running").unwrap();
        assert_eq!(
            first,
            fingerprint(&dir).unwrap(),
            "runtime files never retrigger the check"
        );
        std::fs::write(dir.join("mod.toml"), "oops").unwrap();
        let edited = fingerprint(&dir).unwrap();
        assert_ne!(first, edited, "same length edits count");
        std::fs::write(dir.join("files/rules.toml"), "bad").unwrap();
        assert_ne!(edited, fingerprint(&dir).unwrap());
        std::fs::remove_file(dir.join("files/rules.toml")).unwrap();
        assert_eq!(edited, fingerprint(&dir).unwrap());
        std::fs::create_dir_all(dir.join("files/scripts")).unwrap();
        std::fs::write(dir.join("files/scripts/test.luau"), "return {}").unwrap();
        let script = fingerprint(&dir).unwrap();
        assert_ne!(edited, script);
        std::fs::write(dir.join("files/scripts/test.luau"), "return []").unwrap();
        assert_ne!(script, fingerprint(&dir).unwrap());
        let error = check(&Config {
            folder: dir.clone(),
            native: dir.join("absent-base"),
        })
        .unwrap_err();
        assert!(error.contains("mod.toml"), "{error}");
        std::fs::write(dir.join("mod.toml"), format!(
            "id = \"reload-test\"\nname = \"Reload Test\"\nversion = \"1.0.0\"\nauthor = \"T\"\ndescription = \"\"\napi = {}\nkind = \"full\"\n", d2_modkit::CURRENT_API
        )).unwrap();
        std::fs::write(dir.join("files/rules.toml"), "not_a_rule = true\n").unwrap();
        let error = check(&Config {
            folder: dir.clone(),
            native: dir.join("absent-base"),
        })
        .unwrap_err();
        assert!(
            error.contains("rules.toml") && error.contains("not_a_rule"),
            "{error}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bursts_wait_for_quiet_and_emit_once() {
        let mut d = Debounce::default();
        let t = Instant::now();
        assert!(!d.observe(1, t));
        assert!(!d.observe(2, t + POLL));
        assert!(!d.observe(3, t + POLL * 2));
        assert!(!d.observe(3, t + POLL * 3));
        assert!(d.observe(3, t + POLL * 5));
        assert!(!d.observe(3, t + POLL * 6));
    }

    #[test]
    fn installed_and_online_games_never_watch() {
        let a = |s: &[&str]| s.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(folder_arg(&a(&["play"]), false), None);
        assert_eq!(folder_arg(&a(&["--mod", "a-folder"]), false), None);
        assert_eq!(folder_arg(&a(&["--mod-dir=x"]), true), None);
        assert_eq!(folder_arg(&a(&["--mod-dir", "x"]), false), Some("x".into()));
        assert!(request(None).is_err());
        assert_eq!(window_position(), bevy::window::WindowPosition::default());
    }

    #[test]
    fn changing_mod_identity_cannot_import_the_previous_mods_character() {
        assert!(same_mod(None, "test-mod").is_ok());
        assert!(same_mod(Some("test-mod"), "test-mod").is_ok());
        assert!(same_mod(Some("test-mod"), "different-mod")
            .unwrap_err()
            .contains("mod id changed"));
    }

    #[test]
    fn restart_carries_character_area_slot_and_window() {
        let original = [
            "play",
            "--mod-dir",
            "my mod",
            "--new",
            "sorceress",
            "Tester",
            "--seed",
            "1",
            "--native",
            "base",
        ]
        .map(str::to_owned);
        let args = restart_args(
            &original,
            Path::new("Tester.d2s"),
            Path::new("dev-slots/mod-reload"),
            42,
            2,
            Some(IVec2::new(-10, 70)),
        );
        for pair in [
            ["--mod-dir", "my mod"],
            ["--save", "Tester.d2s"],
            ["--seed", "42"],
            ["--difficulty", "2"],
            ["--dev-start", "dev-slots/mod-reload"],
            ["--dev-window", "-10,70"],
        ] {
            assert!(args.windows(2).any(|w| w == pair));
        }
        assert!(!args.iter().any(|a| a == "--new"));
        // Area and cell position are carried in the slot, not inferred from
        // the character's town/waypoint location.
        let slot = super::super::slots::Slot {
            seed: 42,
            difficulty: 2,
            start: super::super::slots::DevStart {
                level: 40,
                x: 123,
                y: 456,
                ..Default::default()
            },
        };
        assert_eq!(
            super::super::slots::Slot::parse(&slot.to_text()).unwrap(),
            slot
        );
    }
}
