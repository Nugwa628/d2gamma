//! States with stack rules (the mod kit's `states.toml`) on the real
//! wiring: applied through the skill bodies' `apply_state`, the stacks and
//! their stat effects follow the rules, one stack decays at a time, and
//! the update tells the client the count.

use crate::rules::{DataMod, DataModFiles, Ruleset};
use crate::skills::use_::bodies::helpers::{apply_state, StateRequest};
use crate::units::{UnitId, UnitType};

use super::{fx, Fx};

const STATE: u16 = 30;
const ARMORCLASS: u16 = 31;

fn states_toml(extra: &str) -> String {
    format!(
        "[[state]]\nid = {STATE}\nname = \"Smolder\"\nduration_seconds = 2\nmax_stacks = 3\n\
         reapply = \"add_stack\"\n{extra}\n[[state.effect]]\nstat = {ARMORCLASS}\nper_stack = 5\n"
    )
}

/// The fixture under a data mod with `states.toml` = `text`, and a player.
fn fixture(text: &str) -> (Fx, UnitId) {
    let mut fx = fx();
    let files = DataModFiles {
        states: Some(("states.toml", text)),
        ..DataModFiles::default()
    };
    fx.sim.sys.hooks.rules = DataMod::from_files("stacks", &files)
        .unwrap()
        .into_ruleset();
    let p = fx.spawn(UnitType::Player, 3, 4);
    (fx, p)
}

fn apply(fx: &mut Fx, p: UnitId) {
    let ct = fx.sim.sys.hooks.tables.combat.clone();
    fx.sim.skill_use(&mut fx.game, |w| {
        let q = StateRequest {
            source: p,
            target: p,
            skill: 0,
            level: 1,
            duration: 0,
            stat: -1,
            value: 0,
            state: i32::from(STATE),
            callback: 0,
        };
        assert!(apply_state(w, &ct, q).is_some());
    });
}

fn armor(fx: &Fx, p: UnitId) -> i32 {
    fx.sim.sys.stats.unit_total(p, ARMORCLASS, 0)
}

fn stacks(fx: &Fx, p: UnitId) -> u16 {
    let r = fx.sim.sys.stats.unit_list(p).unwrap();
    fx.sim
        .sys
        .stats
        .list_of_state(r, u32::from(STATE))
        .map_or(0, |l| fx.sim.sys.stats.stacks(l))
}

fn on(fx: &Fx, p: UnitId) -> bool {
    fx.sim.sys.stats.has_state(p, u32::from(STATE))
}

#[test]
fn stacks_scale_the_stat_effects_up_to_the_maximum() {
    let (mut fx, p) = fixture(&states_toml(""));
    assert_eq!((armor(&fx, p), stacks(&fx, p), on(&fx, p)), (0, 0, false));
    let mut seen = Vec::new();
    for _ in 0..4 {
        apply(&mut fx, p);
        seen.push((stacks(&fx, p), armor(&fx, p)));
    }
    assert_eq!(seen, [(1, 5), (2, 10), (3, 15), (3, 15)]);
    assert!(on(&fx, p));
    fx.assert_clean();
}

#[test]
fn with_lose_all_the_whole_state_ends_when_the_time_runs_out() {
    let (mut fx, p) = fixture(&states_toml("decay = \"lose_all\""));
    apply(&mut fx, p);
    apply(&mut fx, p);
    assert_eq!((stacks(&fx, p), armor(&fx, p)), (2, 10));
    // 2 seconds are 50 ticks.
    for _ in 0..49 {
        fx.frame();
    }
    assert!(on(&fx, p) && armor(&fx, p) == 10);
    fx.frame();
    assert!(!on(&fx, p));
    assert_eq!((stacks(&fx, p), armor(&fx, p)), (0, 0));
    fx.assert_clean();
}

#[test]
fn with_lose_one_the_stacks_decay_one_per_period() {
    let (mut fx, p) = fixture(&states_toml("decay = \"lose_one\""));
    for _ in 0..3 {
        apply(&mut fx, p);
    }
    assert_eq!((stacks(&fx, p), armor(&fx, p)), (3, 15));
    let mut seen = Vec::new();
    for _ in 0..3 {
        for _ in 0..50 {
            fx.frame();
        }
        seen.push((on(&fx, p), stacks(&fx, p), armor(&fx, p)));
    }
    assert_eq!(seen, [(true, 2, 10), (true, 1, 5), (false, 0, 0)]);
    fx.assert_clean();
}

#[test]
fn applying_again_restarts_the_period_of_all_the_stacks() {
    let (mut fx, p) = fixture(&states_toml("decay = \"lose_one\""));
    apply(&mut fx, p);
    for _ in 0..40 {
        fx.frame();
    }
    apply(&mut fx, p);
    assert_eq!(stacks(&fx, p), 2);
    // The first period would have ended 10 ticks from now; it was restarted.
    for _ in 0..40 {
        fx.frame();
    }
    assert_eq!(stacks(&fx, p), 2);
    for _ in 0..10 {
        fx.frame();
    }
    assert_eq!(stacks(&fx, p), 1);
    fx.assert_clean();
}

#[test]
fn a_state_without_rules_is_the_skills() {
    let (mut fx, p) = fixture("");
    fx.sim.sys.hooks.rules = Ruleset::Original;
    let ct = fx.sim.sys.hooks.tables.combat.clone();
    fx.sim.skill_use(&mut fx.game, |w| {
        let q = StateRequest {
            source: p,
            target: p,
            skill: 0,
            level: 1,
            duration: 10,
            stat: i32::from(ARMORCLASS),
            value: 7,
            state: i32::from(STATE),
            callback: 0,
        };
        assert!(apply_state(w, &ct, q).is_some());
    });
    // The skill's stat once, no stack count, whatever the application count.
    assert_eq!((armor(&fx, p), stacks(&fx, p)), (7, 0));
    fx.assert_clean();
}

#[test]
fn named_custom_state_stat_drives_behavior_through_application_and_expiry() {
    let files = DataModFiles {
        stats: Some(("stats.toml", "[[stat]]\nname='focus'\n[[behavior]]\nkind='add'\nsource='focus'\ntarget='armorclass'\npercent=200")),
        states: Some(("states.toml", "[[state]]\nid=30\nduration_seconds=1\nmax_stacks=3\nreapply='add_stack'\n[[state.effect]]\nstat='focus'\nper_stack=5")),
        ..DataModFiles::default()
    };
    let rules = DataMod::from_files("named", &files).unwrap().into_ruleset();
    let mut data = (*super::stat_data()).clone();
    rules
        .stat_rules()
        .unwrap()
        .install(&mut data.stats)
        .unwrap();
    let mut fx = Fx::with(std::sync::Arc::new(data), super::skills());
    fx.sim.sys.hooks.rules = rules;
    let p = fx.spawn(UnitType::Player, 3, 4);
    apply(&mut fx, p);
    assert_eq!(armor(&fx, p), 10);
    apply(&mut fx, p);
    assert_eq!(armor(&fx, p), 20);
    for _ in 0..25 {
        fx.frame();
    }
    assert_eq!(armor(&fx, p), 0);
    assert!(!on(&fx, p));
    fx.assert_clean();
}
