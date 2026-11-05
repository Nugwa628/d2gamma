//! Playing a mod-kit mod: `D2Gamma-Game --mod <id>` or `--mod-dir <folder>`.
//!
//! The mod is stacked over the base data (the layer stack opens its folder,
//! and the folders of the mods it builds on, after `base/`), and the run
//! takes the mod's identity ([`crate::identity::GameIdentity::for_mod`]): its
//! name in the window title and version line, its own folder
//! `Documents\D2Gamma\mods\<id>` for saves, settings, logs and baked
//! animations ([`crate::user_dir`]). D2Gamma's own play has no `--mod` and
//! is unchanged: it opens `base/` alone.
//!
//! The choice is fixed for the process (the identity, the user folder and
//! the data are process-wide), so picking another mod from inside the game
//! starts D2Gamma again with `--mod` and waits for it ([`relaunch`]).

use std::path::Path;
use std::sync::OnceLock;

use d2_modkit::{ModError, ModSpec, ModStack};
use d2_sim::rules::{DataMod, DataModFiles, Ruleset};

use crate::identity::{self, GameIdentity};
use crate::user_dir;

/// The `--mod` flag: an installed mod's id, or a folder.
pub const MOD_FLAG: &str = "--mod";
/// The `--mod-dir` flag: a mod folder anywhere.
pub const MOD_DIR_FLAG: &str = "--mod-dir";
/// The `--no-mod` flag: plain D2Gamma this once, whatever mod was played last
/// (the safety way in when a mod will not start).
pub const NO_MOD_FLAG: &str = "--no-mod";
/// The mod id that means "no mod, plain D2Gamma" (`--mod none`). It is a
/// reserved id, so no mod can take it.
pub const PLAIN_ID: &str = "none";

/// How plain D2Gamma was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plain {
    /// `--no-mod`: this run only; the remembered mod stays.
    Once,
    /// `--mod none`: a choice, which is remembered as the last played.
    Chosen,
}

/// Removes `--no-mod` and `--mod none` / `--mod=none` from `args` and says
/// which was given. Call before [`take_args`], which would take `none` for a
/// folder. Both together are an error.
pub fn take_plain(args: &mut Vec<String>) -> Result<Option<Plain>, String> {
    let mut found = None;
    let mut i = 0;
    while i < args.len() {
        let which = if args[i] == NO_MOD_FLAG {
            args.remove(i);
            Some(Plain::Once)
        } else if args[i] == format!("{MOD_FLAG}={PLAIN_ID}") {
            args.remove(i);
            Some(Plain::Chosen)
        } else if args[i] == MOD_FLAG && args.get(i + 1).map(String::as_str) == Some(PLAIN_ID) {
            args.drain(i..=i + 1);
            Some(Plain::Chosen)
        } else {
            i += 1;
            None
        };
        if let Some(w) = which {
            if found.replace(w).is_some_and(|before| before != w) {
                return Err(format!(
                    "{NO_MOD_FLAG} and {MOD_FLAG} {PLAIN_ID} both ask for plain D2Gamma; give one"
                ));
            }
        }
    }
    Ok(found)
}

/// Whether `args` start the front-end session (the menus, then a game
/// picked in them): the only start that opens the last played mod. The
/// shortcuts straight into a game (`--new`, `--save`, `--frames`, the dumps
/// and `--offscreen`) never do, so a tool run is the same on every machine.
pub fn is_front_session(args: &[String]) -> bool {
    const SHORTCUTS: [&str; 6] = [
        "--new",
        "--save",
        "--frames",
        "--dump-draws",
        "--dump-loading",
        "--offscreen",
    ];
    !args.iter().any(|a| {
        let flag = a.split_once('=').map_or(a.as_str(), |(f, _)| f);
        SHORTCUTS.contains(&flag)
    })
}

static ACTIVE: OnceLock<ModStack> = OnceLock::new();
/// The rules the mod plays under, built once from its files in [`activate`]
/// so a bad file stops the start, not a game started later from the menu.
static RULES: OnceLock<Ruleset> = OnceLock::new();

/// Removes `--mod X` / `--mod=X` / `--mod-dir X` / `--mod-dir=X` from `args`
/// and returns what they asked for. A mod named twice is an error.
pub fn take_args(args: &mut Vec<String>) -> Result<Option<ModSpec>, String> {
    let mut spec = None;
    let mut i = 0;
    while i < args.len() {
        let (flag, inline) = match args[i].split_once('=') {
            Some((f, v)) => (f.to_owned(), Some(v.to_owned())),
            None => (args[i].clone(), None),
        };
        if flag != MOD_FLAG && flag != MOD_DIR_FLAG {
            i += 1;
            continue;
        }
        let value = match inline {
            Some(v) => {
                args.remove(i);
                v
            }
            None if i + 1 < args.len() => {
                let v = args.remove(i + 1);
                args.remove(i);
                v
            }
            None => return Err(format!("{flag} needs a mod id or a folder after it")),
        };
        if value.is_empty() {
            return Err(format!("{flag} needs a mod id or a folder after it"));
        }
        let this = if flag == MOD_DIR_FLAG {
            ModSpec::from_dir_arg(&value)
        } else {
            ModSpec::from_mod_arg(&value)
        };
        if spec.replace(this).is_some() {
            return Err("only one mod can be started at a time; give --mod once".to_owned());
        }
    }
    Ok(spec)
}

/// Finds the mod (in D2Gamma's mods folder for an id), checks it, and makes
/// this run its game: reads its rules file, installs its identity and
/// remembers the mod stack for [`layers`] and the rules for [`ruleset`]. Call
/// once at start, before [`user_dir::init`].
pub fn activate(spec: &ModSpec) -> Result<&'static ModStack, ModError> {
    let stack = d2_modkit::resolve::resolve_with(
        spec,
        &user_dir::mods_dir(),
        d2_modkit::api::Date::today(),
        d2_native::package::open_mod,
    )?;
    let rules = host(&stack)?.into_ruleset();
    let identity = GameIdentity::for_mod(&stack.picked().manifest);
    // An identity already installed means the caller broke the order; keep
    // the one that is there rather than run half one game and half another.
    if identity::install(identity).is_err() {
        return Err(ModError::Load(
            "a game identity was already set before the mod was started".to_owned(),
        ));
    }
    let _ = RULES.set(rules);
    Ok(ACTIVE.get_or_init(|| stack))
}

/// Read a declaration from either authoring files or a verified installed pack.
/// Keeping this shared makes play and check-mod consume the same text.
pub(crate) fn read_file(
    m: &d2_modkit::resolve::LoadedMod,
    relative: &str,
) -> Result<Option<String>, ModError> {
    let path = m.dir.join(relative);
    let bytes = if m.dir.is_dir() {
        match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(ModError::Load(format!("{}: {e}", path.display()))),
        }
    } else {
        d2_native::pack::Pack::open(&m.dir)
            .map_err(|e| ModError::Load(e.to_string()))?
            .read(relative)
            .transpose()
            .map_err(|e| ModError::Load(e.to_string()))?
    };
    bytes
        .map(|b| {
            String::from_utf8(b).map_err(|e| ModError::Load(format!("{}: {e}", path.display())))
        })
        .transpose()
}

fn last_file(
    stack: &ModStack,
    relative: &str,
) -> Result<Option<d2_modkit::resolve::RulesFile>, ModError> {
    for m in stack.mods.iter().rev() {
        if let Some(text) = read_file(m, relative)? {
            return Ok(Some(d2_modkit::resolve::RulesFile {
                path: m.dir.join(relative),
                text,
            }));
        }
    }
    Ok(None)
}

/// The data mod host of `stack`: its rules from the `rules.toml`, its
/// states from the `states.toml` and its building-block skills from the
/// `skills.toml` and its triggers from the `triggers.toml` and its declared
/// crowd control from the `crowd_control.toml` and its monster behaviour
/// graphs from the `ai.toml` of the last mod
/// that has one. A file that
/// cannot be used stops the start, naming the file and the setting.
pub fn host(stack: &ModStack) -> Result<DataMod, ModError> {
    let file = last_file(stack, "files/rules.toml")?;
    let path = file.as_ref().map(|f| f.path.display().to_string());
    let rules = file.as_ref().zip(path.as_deref());
    let states_file = last_file(stack, "files/states.toml")?;
    let states_path = states_file.as_ref().map(|f| f.path.display().to_string());
    let states = states_file.as_ref().zip(states_path.as_deref());
    let skills_file = last_file(stack, "files/skills.toml")?;
    let skills_path = skills_file.as_ref().map(|f| f.path.display().to_string());
    let skills = skills_file.as_ref().zip(skills_path.as_deref());
    let triggers_file = last_file(stack, "files/triggers.toml")?;
    let triggers_path = triggers_file.as_ref().map(|f| f.path.display().to_string());
    let triggers = triggers_file.as_ref().zip(triggers_path.as_deref());
    let cc_file = last_file(stack, "files/crowd_control.toml")?;
    let cc_path = cc_file.as_ref().map(|f| f.path.display().to_string());
    let crowd_control = cc_file.as_ref().zip(cc_path.as_deref());
    let ai_file = last_file(stack, "files/ai.toml")?;
    let ai_path = ai_file.as_ref().map(|f| f.path.display().to_string());
    let ai = ai_file.as_ref().zip(ai_path.as_deref());
    let quests_file = last_file(stack, "files/quests.toml")?;
    let quests_path = quests_file.as_ref().map(|f| f.path.display().to_string());
    let stats_file = last_file(stack, "files/stats.toml")?;
    let stats_path = stats_file.as_ref().map(|f| f.path.display().to_string());
    let act_file = last_file(stack, "files/acts.toml")?;
    let act_path = act_file.as_ref().map(|f| f.path.display().to_string());
    let diff_file = last_file(stack, "files/difficulties.toml")?;
    let diff_path = diff_file.as_ref().map(|f| f.path.display().to_string());
    let files = DataModFiles {
        ai: ai.map(|(f, path)| (path, f.text.as_str())),
        acts: act_file
            .as_ref()
            .zip(act_path.as_deref())
            .map(|(f, path)| (path, f.text.as_str())),
        difficulties: diff_file
            .as_ref()
            .zip(diff_path.as_deref())
            .map(|(f, path)| (path, f.text.as_str())),
        stats: stats_file
            .as_ref()
            .zip(stats_path.as_deref())
            .map(|(f, path)| (path, f.text.as_str())),
        crowd_control: crowd_control.map(|(f, path)| (path, f.text.as_str())),
        states: states.map(|(f, path)| (path, f.text.as_str())),
        skills: skills.map(|(f, path)| (path, f.text.as_str())),
        triggers: triggers.map(|(f, path)| (path, f.text.as_str())),
        quests: quests_file
            .as_ref()
            .zip(quests_path.as_deref())
            .map(|(f, path)| (path, f.text.as_str())),
        ..DataModFiles::with_rules(rules.map(|(f, path)| (path, f.text.as_str())))
    };
    DataMod::from_files(&stack.picked().manifest.name, &files)
        .map_err(|e| ModError::Load(e.to_string()))
}

/// The rules a game of this run plays under: the mod's data mod host when a
/// mod was asked for, D2Gamma's own rules otherwise. The authoritative side
/// installs it; an online client's copy is never sent or applied.
/// Save codec of the active data mod; base games keep their byte format.
pub fn character_codec() -> std::sync::Arc<dyn d2_server::game::CharacterCodec> {
    let rules = ruleset();
    if rules.wide_stat_ids() {
        std::sync::Arc::new(d2_server::game::ModCodec(rules))
    } else {
        std::sync::Arc::new(d2_server::game::D2sCodec)
    }
}

pub fn ruleset() -> Ruleset {
    RULES.get().cloned().unwrap_or_default()
}

/// The mod this run plays, when `--mod` / `--mod-dir` was given.
pub fn active() -> Option<&'static ModStack> {
    ACTIVE.get()
}

/// The layer entries to open over `base/`: the mod and the mods it builds
/// on, each as its folder; none when no mod was asked for (`base/` alone).
pub fn layers() -> Vec<String> {
    active().map(ModStack::layer_entries).unwrap_or_default()
}

static SKIP_ADDONS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The safety start bypasses all optional add-ons as well as full mods.
pub fn skip_addons() {
    SKIP_ADDONS.store(true, std::sync::atomic::Ordering::Relaxed);
}
/// Plain-game add-ons have no rules host or game identity. Native loading applies
/// the owner's policy and screen-only capability checks before any assets are used.
pub fn layers_for(native: &Path) -> Result<Vec<String>, ModError> {
    if active().is_some() {
        return Ok(layers());
    }
    if SKIP_ADDONS.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(Vec::new());
    }
    plain_layers(native, &user_dir::mods_dir())
}
pub fn plain_layers(native: &Path, mods: &Path) -> Result<Vec<String>, ModError> {
    let mut loaded = Vec::new();
    for id in d2_modkit::addons::enabled(mods, "d2gamma").map_err(ModError::Load)? {
        let addon = d2_native::package::open_mod(
            &d2_modkit::resolve::installed_path(mods, &id),
            d2_modkit::api::Date::today(),
        )?;
        if addon.manifest.id != id {
            return Err(ModError::Load(format!(
                "installed add-on {id} declares {}",
                addon.manifest.id
            )));
        }
        addon
            .manifest
            .check_addon_for(None)
            .map_err(ModError::Load)?;
        loaded.push(addon);
    }
    d2_modkit::addons::check_stack(&loaded.iter().map(|m| &m.manifest).collect::<Vec<_>>())
        .map_err(ModError::Load)?;
    let entries = loaded
        .iter()
        .map(|m| m.dir.display().to_string())
        .collect::<Vec<_>>();
    d2_native::layers::LayerStack::with_mods(native, &entries)
        .map_err(|e| ModError::Load(e.to_string()))?;
    Ok(entries)
}

/// The argument list that starts `mod_id` instead of whatever this run was
/// started with: the old `--mod` / `--mod-dir` is dropped and `--mod <id>`
/// added after the subcommand. Everything else (the game folder, the
/// resolution) is passed on as it was.
pub fn child_args(original: &[String], mod_id: &str) -> Vec<String> {
    let mut rest: Vec<String> = original.to_vec();
    // The earlier choice is not an error here: it is replaced.
    let _ = take_plain(&mut rest);
    let _ = take_args(&mut rest);
    let mut out = Vec::with_capacity(rest.len() + 2);
    if rest.first().map(String::as_str) == Some("play") {
        out.push(rest.remove(0));
    }
    out.push(MOD_FLAG.to_owned());
    out.push(mod_id.to_owned());
    out.extend(rest);
    out
}

/// Runs `exe` with `args`, waits for it, and returns its exit code (a
/// process that ended without one, such as a signal, counts as an error).
/// The running D2Gamma calls this to start the picked mod and exits with the
/// same code, so a launcher sees one game.
pub fn relaunch(exe: &Path, args: &[String]) -> std::io::Result<u8> {
    let status = std::process::Command::new(exe).args(args).status()?;
    let error = crate::launch::exit::ERROR;
    Ok(status
        .code()
        .map_or(error, |c| u8::try_from(c).unwrap_or(error)))
}

/// [`relaunch`] of the running exe.
pub fn relaunch_self(args: &[String]) -> std::io::Result<u8> {
    relaunch(&std::env::current_exe()?, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn no_mod_flag_leaves_the_arguments_and_asks_for_nothing() {
        let mut a = v(&["play", "--new", "Amy"]);
        assert_eq!(take_args(&mut a).unwrap(), None);
        assert_eq!(a, v(&["play", "--new", "Amy"]));
    }

    #[test]
    fn an_id_and_a_folder_are_told_apart() {
        let mut a = v(&["play", "--mod", "ember-age", "--frames", "3"]);
        assert_eq!(
            take_args(&mut a).unwrap(),
            Some(ModSpec::Id("ember-age".into()))
        );
        assert_eq!(a, v(&["play", "--frames", "3"]));

        let mut b = v(&["--mod=C:/mods/ember"]);
        assert!(matches!(take_args(&mut b).unwrap(), Some(ModSpec::Dir(_))));
        assert!(b.is_empty());

        let mut c = v(&["play", "--mod-dir", "ember-age"]);
        assert!(matches!(take_args(&mut c).unwrap(), Some(ModSpec::Dir(_))));
    }

    #[test]
    fn a_missing_value_or_two_mods_are_refused_in_words() {
        assert!(take_args(&mut v(&["play", "--mod"]))
            .unwrap_err()
            .contains("--mod needs"));
        assert!(take_args(&mut v(&["--mod="])).is_err());
        let e = take_args(&mut v(&["--mod", "aaa", "--mod-dir", "x"])).unwrap_err();
        assert!(e.contains("one mod"), "{e}");
    }

    #[test]
    fn the_child_of_a_mod_pick_replaces_the_old_choice() {
        let before = v(&["play", "--mod", "old-mod", "--res", "800x600"]);
        assert_eq!(
            child_args(&before, "new-mod"),
            v(&["play", "--mod", "new-mod", "--res", "800x600"])
        );
        // Started with no subcommand: the flag leads.
        assert_eq!(
            child_args(&v(&["--res", "800x600"]), "new-mod"),
            v(&["--mod", "new-mod", "--res", "800x600"])
        );
    }

    #[test]
    fn the_parent_passes_the_child_exit_code_on() {
        // The test binary itself, asked for something it does not know: it
        // exits with a code of its own, which `relaunch` returns unchanged.
        let exe = std::env::current_exe().unwrap();
        let direct = std::process::Command::new(&exe)
            .arg("--no-such-flag")
            .status()
            .unwrap()
            .code()
            .unwrap();
        assert_ne!(direct, 0);
        let passed = relaunch(&exe, &v(&["--no-such-flag"])).unwrap();
        assert_eq!(i32::from(passed), direct);
    }

    /// A mod folder as `--mod-dir` takes it, with `rules` as its rules file.
    fn mod_dir(name: &str, rules: Option<&str>) -> ModStack {
        let dir = std::env::temp_dir().join(format!("d2-kit-mod-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("files")).unwrap();
        std::fs::write(
            dir.join("mod.toml"),
            format!(
                "id = \"rules-mod\"\nname = \"Rules Mod\"\nversion = \"1.0.0\"\nauthor = \"T\"\ndescription = \"\"\napi = {}\nkind = \"full\"\n",
                d2_modkit::CURRENT_API
            ),
        )
        .unwrap();
        if let Some(text) = rules {
            std::fs::write(dir.join("files/rules.toml"), text).unwrap();
        }
        let spec = take_args(&mut v(&["--mod-dir", &dir.display().to_string()]))
            .unwrap()
            .unwrap();
        d2_modkit::resolve(&spec, &dir.join("installed")).unwrap()
    }

    #[test]
    fn the_host_of_a_mod_folder_carries_its_rules_file() {
        let none = host(&mod_dir("rules-none", None)).unwrap();
        assert_eq!(
            none.global(),
            &d2_sim::rules::global::GlobalRules::default()
        );

        let off = host(&mod_dir(
            "rules-off",
            Some("[monsters.hit_recovery]\nenabled = false\n"),
        ))
        .unwrap();
        assert!(!off.global().monsters.hit_recovery.enabled);
        assert!(off.global().players.hit_recovery.enabled);
        let rules = off.into_ruleset();
        assert!(!rules.is_original());
        assert_eq!(rules.mod_rules().unwrap().name(), "Rules Mod");
    }

    #[test]
    fn the_host_of_a_mod_folder_carries_its_states_file() {
        let stack = mod_dir("states-ok", None);
        let none = host(&stack).unwrap();
        assert!(none.state_rules().is_empty());

        let file = stack.picked().dir.join("files/states.toml");
        std::fs::write(
            &file,
            "[[state]]
id = 380
max_stacks = 5
icon = \"smolder\"
",
        )
        .unwrap();
        let with = host(&stack).unwrap();
        let def = with.state_rules().get(380).unwrap();
        assert_eq!((def.max_stacks, def.icon.as_deref()), (5, Some("smolder")));
        assert!(with.into_ruleset().state_rules().is_some());

        // A bad file stops the start and names the file and the key.
        std::fs::write(
            &file,
            "[[state]]
id = 380
max_stacks = 0
",
        )
        .unwrap();
        let e = host(&stack).unwrap_err().to_string();
        assert!(
            e.contains("states.toml") && e.contains("state[0].max_stacks"),
            "{e}"
        );
    }

    #[test]
    fn folder_stats_resolve_named_state_effects_and_errors_name_the_file() {
        let stack = mod_dir("named-stats", None);
        assert!(host(&stack).unwrap().into_ruleset().stat_rules().is_none());
        let dir = stack.picked().dir.join("files");
        std::fs::write(dir.join("stats.toml"), "[[stat]]\nname='focus'\nid=40000").unwrap();
        std::fs::write(
            dir.join("states.toml"),
            "[[state]]\nid=380\n[[state.effect]]\nstat='focus'\nper_stack=7",
        )
        .unwrap();
        let with = host(&stack).unwrap();
        assert_eq!(with.state_rules().get(380).unwrap().effects[0].stat, 40000);
        assert_eq!(
            with.into_ruleset().stat_rules().unwrap().id("focus"),
            Some(40000)
        );
        std::fs::write(dir.join("stats.toml"), "[[stat]]\nname='strength'").unwrap();
        assert!(host(&stack).unwrap_err().to_string().contains("stats.toml"));
    }

    #[test]
    fn a_bad_rules_file_stops_the_start_naming_the_file_and_key() {
        let e = host(&mod_dir(
            "rules-bad",
            Some("[players.knockback]\nduration_percent = 99999\n"),
        ))
        .unwrap_err()
        .to_string();
        assert!(e.contains("rules.toml"), "{e}");
        assert!(e.contains("players.knockback.duration_percent"), "{e}");
    }

    #[test]
    fn plain_d2gamma_is_asked_for_by_no_mod_or_mod_none() {
        let mut a = v(&["play", "--no-mod", "--res", "800x600"]);
        assert_eq!(take_plain(&mut a), Ok(Some(Plain::Once)));
        assert_eq!(a, v(&["play", "--res", "800x600"]));

        let mut b = v(&["--mod", "none", "--frames", "3"]);
        assert_eq!(take_plain(&mut b), Ok(Some(Plain::Chosen)));
        assert_eq!(b, v(&["--frames", "3"]));

        let mut c = v(&["play", "--mod=none"]);
        assert_eq!(take_plain(&mut c), Ok(Some(Plain::Chosen)));
        assert_eq!(c, v(&["play"]));

        // Neither: nothing is taken, and a real mod is left for `take_args`.
        let mut d = v(&["play", "--mod", "ember-age"]);
        assert_eq!(take_plain(&mut d), Ok(None));
        assert_eq!(d, v(&["play", "--mod", "ember-age"]));
        // A folder that happens to end in none is not the reserved id.
        let mut e = v(&["--mod-dir", "none"]);
        assert_eq!(take_plain(&mut e), Ok(None));
        assert_eq!(e, v(&["--mod-dir", "none"]));
        // Saying the same thing twice is fine; two different things are not.
        assert_eq!(
            take_plain(&mut v(&["--no-mod", "--no-mod"])),
            Ok(Some(Plain::Once))
        );
        assert!(take_plain(&mut v(&["--no-mod", "--mod", "none"])).is_err());
    }

    #[test]
    fn none_is_a_reserved_id_so_it_never_names_a_mod() {
        assert!(d2_modkit::manifest::check_id(PLAIN_ID).is_err());
        // `take_args` alone would take it for a folder; `take_plain` runs first.
        assert!(matches!(
            take_args(&mut v(&["--mod", "none"])).unwrap(),
            Some(ModSpec::Dir(_))
        ));
    }

    #[test]
    fn the_child_of_a_pick_drops_the_safety_flags_too() {
        assert_eq!(
            child_args(&v(&["play", "--no-mod", "--res", "800x600"]), "ember-age"),
            v(&["play", "--mod", "ember-age", "--res", "800x600"])
        );
        assert_eq!(
            child_args(&v(&["play", "--mod", "none"]), "ember-age"),
            v(&["play", "--mod", "ember-age"])
        );
        assert_eq!(
            child_args(&v(&["--mod-dir", "C:/x"]), PLAIN_ID),
            v(&["--mod", "none"])
        );
    }

    #[test]
    fn the_shortcuts_into_a_game_are_not_a_front_end_session() {
        assert!(is_front_session(&v(&[])));
        assert!(is_front_session(&v(&[
            "play",
            "--res",
            "800x600",
            "--save-dir",
            "s"
        ])));
        for shortcut in [
            "--new",
            "--save",
            "--frames",
            "--dump-draws",
            "--dump-loading",
            "--offscreen",
        ] {
            assert!(
                !is_front_session(&v(&["play", shortcut, "x"])),
                "{shortcut}"
            );
        }
        assert!(!is_front_session(&v(&["--save=x.d2s"])));
    }

    #[test]
    fn no_mod_means_the_base_layer_alone() {
        // Nothing in the test process activates a mod.
        assert!(active().is_none());
        assert!(layers().is_empty());
        assert!(ruleset().is_original());
    }
}
