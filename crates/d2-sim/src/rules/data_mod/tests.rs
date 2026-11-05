//! The data mod host: an empty mod is D2Gamma, a mod folder's `rules.toml`
//! changes the outcome, and a bad file says which key is wrong.

use std::path::PathBuf;

use super::*;
use crate::combat::{self, DamageRecord};
use crate::units::{UnitId, UnitType};
use crate::wiring::action::tests::Fx;

/// A mod folder in the temp dir with `rules` as its `files/rules.toml`
/// (none when `None`), like one Mod Studio would pass to `--mod-dir`.
fn mod_folder(name: &str, rules: Option<&str>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("d2-sim-data-mod-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("files")).unwrap();
    std::fs::write(
        dir.join("mod.toml"),
        format!(
            "id = \"test-mod\"\nname = \"Test Mod\"\nversion = \"1.0.0\"\nauthor = \"T\"\n\
             description = \"\"\napi = {}\nkind = \"full\"\n",
            d2_modkit::CURRENT_API
        ),
    )
    .unwrap();
    if let Some(text) = rules {
        std::fs::write(dir.join("files/rules.toml"), text).unwrap();
    }
    dir
}

/// What `--mod-dir <dir>` does with the folder, up to the ruleset: resolve
/// the mod, read its rules file, build the host.
fn load(dir: &std::path::Path) -> Result<Ruleset, String> {
    let spec = d2_modkit::ModSpec::from_dir_arg(&dir.display().to_string());
    let stack =
        d2_modkit::resolve(&spec, &dir.join("no-installed-mods")).map_err(|e| e.to_string())?;
    let file = stack.rules_file().map_err(|e| e.to_string())?;
    let path = file.as_ref().map(|f| f.path.display().to_string());
    let files = DataModFiles {
        rules: file
            .as_ref()
            .zip(path.as_deref())
            .map(|(f, p)| (p, f.text.as_str())),
        ..DataModFiles::default()
    };
    DataMod::from_files(&stack.picked().manifest.name, &files)
        .map(DataMod::into_ruleset)
        .map_err(|e| e.to_string())
}

/// A defender and an attacker with the animation data a get-hit reaction
/// needs, under `rules`.
pub(super) fn wired(rules: Ruleset, ty: UnitType) -> (Fx, UnitId, UnitId) {
    use d2_formats::animdata::{self, AnimData, AnimRecord};
    let mut fx = Fx::new();
    fx.sim.hooks().enable_paths().unwrap();
    fx.sim.hooks().rules = rules;
    let a = fx.spawn(UnitType::Player, 0, fx.a, 10, 10);
    let d = fx.spawn(ty, 0, fx.a, 13, 10);
    fx.stats(d, &[(7, 25_600), (6, 25_600)]);
    fx.game.frame = 100;
    let modes = if ty == UnitType::Player {
        [4, 9, 19]
    } else {
        [3, 6, 13]
    };
    let mut data = AnimData {
        buckets: vec![Vec::new(); animdata::BUCKETS],
    };
    for mode in modes {
        let s = format!("X{mode:02}HTH");
        let mut name = [0; 8];
        name[..6].copy_from_slice(s.as_bytes());
        data.buckets[animdata::hash(&name[..6])].push(AnimRecord {
            name,
            frames: 8,
            speed: 256,
            events: [0; animdata::EVENTS],
        });
        fx.sim.hooks().x.names.insert((ty, mode), name);
    }
    fx.sim.hooks().anim_data = Some(std::sync::Arc::new(data));
    (fx, a, d)
}

/// The defender's mode (attacking) after a stunned get-hit reaction, and the
/// reaction's record.
fn stunned_hit(rules: Ruleset, ty: UnitType, attack: u32, flags: u16) -> (DamageRecord, u32, i16) {
    let (mut fx, a, d) = wired(rules, ty);
    fx.sim.sys.units.get_mut(d).unwrap().mode = attack;
    fx.sim.combat(&mut fx.game, |w, _| {
        use crate::combat::CombatWorld;
        w.create_state_list(d, 21, a, 200);
    });
    let mut rec = DamageRecord {
        result: combat::result::HIT | flags,
        total: 20_000,
        ..Default::default()
    };
    fx.sim.combat(&mut fx.game, |w, _| {
        crate::wiring::action::reaction::reaction(w, a, d, &mut rec)
    });
    let unit = fx.sim.sys.units.get(d).unwrap();
    (rec, unit.mode, unit.anim.speed)
}

const TARGETS: [(UnitType, u32); 2] = [(UnitType::Player, 7), (UnitType::Monster, 4)];

#[test]
fn a_mod_folder_with_hit_recovery_off_changes_the_outcome() {
    let dir = mod_folder(
        "hit-recovery-off",
        Some("version = 1\n\n[players.hit_recovery]\nenabled = false\n\n[monsters.hit_recovery]\nenabled = false\n"),
    );
    let modded = load(&dir).unwrap();
    assert_eq!(modded.mod_rules().unwrap().name(), "Test Mod");
    assert_eq!(modded.data_rules(), d2_data::bin::DataRules::Mod);
    assert!(!modded.global_rules().players.hit_recovery.enabled);
    for (ty, attack) in TARGETS {
        let flags = combat::result::GET_HIT;
        let (_, own_mode, _) = stunned_hit(Ruleset::Original, ty, attack, flags);
        let (_, mod_mode, _) = stunned_hit(modded.clone(), ty, attack, flags);
        // D2Gamma interrupts the attack; the mod's defender carries on.
        assert_ne!(own_mode, attack, "{ty:?}");
        assert_eq!(mod_mode, attack, "{ty:?}");
    }
}

#[test]
fn no_mod_rules_file_or_an_empty_one_is_d2gamma() {
    let none = load(&mod_folder("no-file", None)).unwrap();
    let empty = load(&mod_folder("empty-file", Some("# nothing changed\n"))).unwrap();
    for host in [none, empty] {
        assert_eq!(host.global_rules(), &GlobalRules::default());
        for (ty, attack) in TARGETS {
            for flags in [
                combat::result::GET_HIT,
                combat::result::BLOCK,
                combat::result::KNOCKBACK,
            ] {
                assert_eq!(
                    stunned_hit(Ruleset::Original, ty, attack, flags),
                    stunned_hit(host.clone(), ty, attack, flags),
                    "{ty:?} {flags}"
                );
            }
        }
    }
}

#[test]
fn a_bad_rules_file_names_the_file_and_the_key() {
    for (text, key) in [
        ("max_block_chance = 101\n", "max_block_chance"),
        ("unknown_switch = true\n", "unknown_switch"),
        (
            "[players.hit_recovery]\nenabled = \"yes\"\n",
            "players.hit_recovery.enabled",
        ),
        (
            "[monsters.stun]\nduration_percent = 20000\n",
            "monsters.stun.duration_percent",
        ),
        ("version = 2\n", "version"),
        (
            "[drops]\nmagic_find_percent = -1\n",
            "drops.magic_find_percent",
        ),
    ] {
        let message = load(&mod_folder("bad-rules", Some(text))).unwrap_err();
        assert!(message.contains("rules.toml"), "{message}");
        assert!(message.contains(key), "{key}: {message}");
    }
}

#[test]
fn a_mod_that_cannot_start_is_not_a_rules_error() {
    let dir = mod_folder("no-manifest", None);
    std::fs::remove_file(dir.join("mod.toml")).unwrap();
    assert!(load(&dir).unwrap_err().contains("mod.toml"));
}

#[test]
fn the_key_is_found_from_the_line() {
    let text = "version = 1\n[players.stun]\nenabled = 3\n\n[[x]]\nz = 1\n";
    assert_eq!(key_at(text, 3).as_deref(), Some("version"));
    assert_eq!(
        key_at(text, text.find("= 3").unwrap()).as_deref(),
        Some("players.stun.enabled")
    );
    // A header line is not a key.
    assert_eq!(key_at(text, 14), None);
    assert_eq!(key_at("", 0), None);
}

#[test]
fn the_host_is_the_rules_the_server_installs() {
    let host = DataMod::new("Plain");
    assert_eq!(host.global(), &GlobalRules::default());
    let rules = host.into_ruleset();
    assert!(!rules.is_original());
    assert_eq!(rules.data_rules(), d2_data::bin::DataRules::Mod);
}

#[test]
fn no_states_file_or_an_empty_one_is_d2gamma() {
    use super::super::ModRules;
    for text in [None, Some(""), Some("version = 1\n")] {
        let files = DataModFiles {
            states: text.map(|t| ("states.toml", t)),
            ..DataModFiles::default()
        };
        let host = DataMod::from_files("t", &files).unwrap();
        assert!(host.state_rules().is_empty());
        // The seam answers "no rules", so the apply path stays D2Gamma's.
        assert!(ModRules::state_rules(&host).is_none());
        assert!(host.into_ruleset().state_rules().is_none());
    }
    assert!(Ruleset::Original.state_rules().is_none());
}

#[test]
fn a_states_file_with_entries_reaches_the_seam() {
    let files = DataModFiles {
        states: Some(("states.toml", "[[state]]\nid = 12\nmax_stacks = 4\n")),
        ..DataModFiles::default()
    };
    let rules = DataMod::from_files("t", &files).unwrap().into_ruleset();
    assert_eq!(rules.state_rules().unwrap().get(12).unwrap().max_stacks, 4);
    assert!(rules.state_rules().unwrap().get(13).is_none());
}

#[test]
fn a_bad_states_file_names_the_file_and_the_key() {
    let files = DataModFiles {
        states: Some((
            r"C:\mods\ember\files\states.toml",
            "[[state]]\nid = 12\nmax_stacks = 0\n",
        )),
        ..DataModFiles::default()
    };
    let message = DataMod::from_files("t", &files).unwrap_err().to_string();
    assert!(message.contains("states.toml"), "{message}");
    assert!(message.contains("state[0].max_stacks"), "{message}");
}

#[test]
fn no_triggers_file_or_an_empty_one_wants_no_events() {
    use super::super::ModRules;
    for text in [None, Some(""), Some("version = 1\n")] {
        let files = DataModFiles {
            triggers: text.map(|t| ("triggers.toml", t)),
            ..DataModFiles::default()
        };
        let host = DataMod::from_files("t", &files).unwrap();
        assert!(host.trigger_rules().is_empty());
        // Nothing to answer: the skill seam is not even offered.
        assert!(ModRules::skills(&host).is_none());
    }
}

#[test]
fn triggers_reach_the_skill_seam_and_mark_the_states_they_watch() {
    use super::super::ModRules;
    let files = DataModFiles {
        states: Some(("states.toml", "[[state]]\nid = 12\nduration_seconds = 2\n")),
        triggers: Some((
            "triggers.toml",
            "[[trigger]]\non = \"state_end\"\nstate = 12\nwhile_skill = 1\n\
             [[trigger.action]]\nkind = \"remove_state\"\nstate = 12\n",
        )),
        ..DataModFiles::default()
    };
    let host = DataMod::from_files("t", &files).unwrap();
    assert!(ModRules::skills(&host).is_some_and(|s| s.wants_unit_events()));
    assert!(host.state_rules().get(12).unwrap().watched);
}

#[test]
fn a_bad_triggers_file_names_the_file_and_the_key() {
    let files = DataModFiles {
        triggers: Some((
            r"C:\mods\ember\files\triggers.toml",
            "[[trigger]]\non = \"kill\"\nwhile_skill = 1\n",
        )),
        ..DataModFiles::default()
    };
    let message = DataMod::from_files("t", &files).unwrap_err().to_string();
    assert!(message.contains("triggers.toml"), "{message}");
    assert!(message.contains("trigger[0].action"), "{message}");
    // A trigger naming a state without rules is refused with its key too.
    let files = DataModFiles {
        triggers: Some((
            "triggers.toml",
            "[[trigger]]\non = \"state_end\"\nstate = 5\nwhile_skill = 1\n\
             [[trigger.action]]\nkind = \"remove_state\"\nstate = 5\n",
        )),
        ..DataModFiles::default()
    };
    let message = DataMod::from_files("t", &files).unwrap_err().to_string();
    assert!(message.contains("trigger[0].state"), "{message}");
}
