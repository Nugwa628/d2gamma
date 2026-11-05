//! A mod's AI presets (`preset = "Zombie"` in `files/ai.toml`): the monsters
//! run D2Gamma's own body with the mod's parameters, and everything else is
//! left as it was.

use super::*;
use crate::rules::data_mod::ai::THINK_BASE;
use crate::rules::{DataMod, DataModFiles, MonsterProvider};

const PRESET_AI: u16 = 201;

fn rules(toml: &str) -> DataMod {
    let files = DataModFiles {
        ai: Some(("ai.toml", toml)),
        ..DataModFiles::default()
    };
    DataMod::from_files("presets", &files).unwrap()
}

/// A world whose monster uses `row`, under a mod with `toml` when given.
fn world(row: Monstats, toml: Option<&str>) -> World {
    let mut w = World::new(row);
    if let Some(toml) = toml {
        w.fake.rules = rules(toml).into_ruleset();
        let mon = w.mon;
        w.with(|g, cx| install(g, cx, mon, 0));
    }
    w
}

/// What the monster does over a spread of situations, from one seed.
fn play(row: Monstats, toml: Option<&str>, seed: u32) -> Vec<String> {
    let mut seen = Vec::new();
    for (combat, distance) in [(true, 1), (false, 5), (false, 12), (true, 2), (false, 7)] {
        let mut w = world(row.clone(), toml);
        w.seed(seed);
        w.run(combat, distance);
        seen.push(format!(
            "{:?} {:?} {:?} {:?}",
            w.fake.log,
            w.thinks(),
            w.fake.seeds.get(&w.mon),
            w.store.get(w.mon).map(|m| m.velocity.speed)
        ));
    }
    seen
}

const ZOMBIE_AIPS: [i16; 5] = [30, 10, 0, 20, 0];

fn zombie_preset(params: &str) -> String {
    format!("[[ai]]\nindex = {PRESET_AI}\npreset = \"Zombie\"\n[ai.params]\n{params}\n")
}

#[test]
fn a_preset_with_parameters_plays_like_the_ai_with_those_columns() {
    let toml = zombie_preset("approach = 30\naware_dist = 10\natt1_att2 = 20");
    for seed in SEEDS {
        // The monstats columns are all zero; the mod supplies the numbers.
        let preset = play(monstats(PRESET_AI, [0; 5], 15), Some(&toml), seed);
        let plain = play(monstats(3, ZOMBIE_AIPS, 15), None, seed);
        assert_eq!(preset, plain, "seed {seed}");
    }
}

#[test]
fn a_preset_without_parameters_is_that_ai_on_the_rows_own_columns() {
    let toml = format!("[[ai]]\nindex = {PRESET_AI}\npreset = \"Zombie\"\n");
    for seed in SEEDS {
        let preset = play(monstats(PRESET_AI, ZOMBIE_AIPS, 15), Some(&toml), seed);
        let plain = play(monstats(3, ZOMBIE_AIPS, 15), None, seed);
        assert_eq!(preset, plain, "seed {seed}");
    }
}

#[test]
fn a_parameter_beats_the_monstats_column() {
    let toml = zombie_preset("approach = 30\naware_dist = 10\natt1_att2 = 20");
    for seed in SEEDS {
        let preset = play(
            monstats(PRESET_AI, [99, 99, 0, 99, 0], 15),
            Some(&toml),
            seed,
        );
        let plain = play(monstats(3, ZOMBIE_AIPS, 15), None, seed);
        assert_eq!(preset, plain, "seed {seed}");
    }
}

#[test]
fn a_parameter_really_changes_what_the_monster_does() {
    // Never attack with the first attack (att1_att2 = 0): every combat think
    // uses the second.
    let toml = zombie_preset("att1_att2 = 0");
    for seed in SEEDS {
        let mut w = world(monstats(PRESET_AI, ZOMBIE_AIPS, 15), Some(&toml));
        w.seed(seed);
        w.run(true, 1);
        assert_eq!(
            last_mode(&w),
            format!("mode 5 Unit({:?})", w.player),
            "seed {seed}"
        );
    }
    let toml = zombie_preset("att1_att2 = 100");
    for seed in SEEDS {
        let mut w = world(monstats(PRESET_AI, ZOMBIE_AIPS, 15), Some(&toml));
        w.seed(seed);
        w.run(true, 1);
        assert_eq!(
            last_mode(&w),
            format!("mode 4 Unit({:?})", w.player),
            "seed {seed}"
        );
    }
}

#[test]
fn a_preset_on_the_ais_own_index_changes_every_monster_that_uses_it() {
    let toml = "[[ai]]\nindex = 3\npreset = \"Zombie\"\n[ai.params]\natt1_att2 = 100\n";
    let mut w = world(monstats(3, ZOMBIE_AIPS, 15), Some(toml));
    w.seed(12345);
    w.run(true, 1);
    assert_eq!(last_mode(&w), format!("mode 4 Unit({:?})", w.player));
}

#[test]
fn other_ais_play_as_they_do_without_the_mod() {
    let toml = zombie_preset("approach = 1\naware_dist = 2\natt1_att2 = 3");
    for seed in SEEDS {
        for ai in [2, 3, 7, 9] {
            let row = || monstats(ai, [30, 10, 50, 20, 40], 15);
            assert_eq!(
                play(row(), Some(&toml), seed),
                play(row(), None, seed),
                "AI {ai} seed {seed}"
            );
        }
    }
}

#[test]
fn a_preset_runs_no_graph() {
    let toml = zombie_preset("approach = 1");
    let host = rules(&toml);
    let w = world(monstats(PRESET_AI, [0; 5], 15), Some(&toml));
    let c = w.store.control(w.mon).unwrap();
    assert_eq!(c.function, AI_TABLE[3].think);
    assert!(w.store.get(w.mon).unwrap().graph.is_none());
    // The ids a graph would have are not answered for a preset.
    let rules = host.ai_rules();
    assert_eq!(rules.ai_record(PRESET_AI), Some(AI_TABLE[3]));
    assert_eq!(rules.aip(PRESET_AI, 1, 0), Some(1));
    assert_eq!(rules.aip(PRESET_AI, 2, 0), None);
    assert_eq!(rules.aip(PRESET_AI + 1, 1, 0), None);
    let mut w = w;
    let mon = w.mon;
    let p = TickParam {
        target: Some(w.player),
        distance: 1,
        combat: true,
        class: 0,
        class2: 0,
    };
    let asked = w.with(|g, cx| rules.ai_think(g, &mut cx.as_dyn(), THINK_BASE, mon, &p));
    assert!(!asked);
}

#[test]
fn parameters_follow_the_difficulty() {
    let toml = zombie_preset("approach = [10, 20, 30]");
    let host = rules(&toml);
    let rules = host.ai_rules();
    assert_eq!(
        [0u8, 1, 2].map(|d| rules.aip(PRESET_AI, 1, d)),
        [Some(10), Some(20), Some(30)]
    );
}
