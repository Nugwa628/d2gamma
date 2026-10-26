//! [`crate::rules::HostStates`] through a mod body.

use std::sync::Arc;

use crate::rules::{SkillHost, StateRemove, StateSpec, StateStat};
use crate::skills::use_::bodies::{self, BodyTables, Handler};
use crate::units::{UnitId, UnitType};

use super::super::N_STATES;
use super::{fx, install, probe, run, uninstall, Fx};

/// Two states of one `states` group, and one of no group.
const STATE_A: u16 = 30;
const STATE_B: u16 = 31;
const STATE_C: u16 = 40;
const GROUP: i32 = 7;
const ARMORCLASS: u16 = 31;
const MOD_STAT: u16 = 300;
const SKILL: i32 = 0;

/// The fixture with the states' groups, and a player.
fn fixture() -> (Fx, UnitId) {
    let mut fx = fx();
    let mut g = vec![0; N_STATES];
    g[usize::from(STATE_A)] = GROUP;
    g[usize::from(STATE_B)] = GROUP;
    fx.sim.sys.hooks.bodies = Some(Arc::new(BodyTables {
        stats: vec![bodies::BodyStat::default(); 359],
        state_group: g,
        state_aura: vec![false; N_STATES],
        ..BodyTables::default()
    }));
    let p = fx.spawn(UnitType::Player, 3, 4);
    (fx, p)
}

/// `state` with an armor class stat, a layered stat, the markers and an
/// expiry at frame `expire`, removed through the mod.
fn spec(u: UnitId, state: u16, expire: Option<i32>) -> StateSpec {
    StateSpec {
        source: Some(u),
        skill: SKILL,
        level: 3,
        expire,
        stats: vec![
            StateStat {
                stat: ARMORCLASS,
                layer: 0,
                value: 25,
            },
            StateStat {
                stat: MOD_STAT,
                layer: 2,
                value: 4,
            },
        ],
        markers: true,
        on_remove: StateRemove::Mod,
        ..StateSpec::new(state)
    }
}

fn timers_12(fx: &Fx, u: UnitId) -> Vec<i32> {
    let t = &fx.game.timers;
    t.unit_timers(u)
        .into_iter()
        .filter(|&id| t.event(id).is_some_and(|e| e.0 == 12))
        .filter_map(|id| t.expire(id))
        .collect()
}

#[test]
fn a_mod_body_sets_layered_stats_and_lists_them() {
    let (mut fx, p) = fixture();
    let got = probe(&mut fx, p, SKILL, 1, |w, u, _, _| {
        w.set_stat_layer(u, MOD_STAT, 5, 11);
        let mine = |e: &Vec<StateStat>| {
            e.iter()
                .any(|s| (s.stat, s.layer, s.value) == (MOD_STAT, 5, 11))
        };
        format!(
            "{:?}",
            (
                w.base_stat(u, MOD_STAT, 5),
                w.base_stat(u, MOD_STAT, 0),
                mine(&w.base_stat_entries(u)),
                mine(&w.stat_entries(u)),
            )
        )
    });
    assert_eq!(got.as_deref(), Some("(11, 0, true, true)"));
    let v = fx
        .sim
        .skill_use(&mut fx.game, |w| SkillHost::stat(w, p, MOD_STAT, 5));
    assert_eq!(v, 11);
    fx.assert_clean();
}

#[test]
fn a_mod_body_applies_a_state_with_its_stat_list() {
    let (mut fx, p) = fixture();
    let frame = fx.game.frame;
    let got = probe(&mut fx, p, SKILL, 3, move |w, u, _, _| {
        let ok = w.apply_state(u, &spec(u, STATE_A, Some(frame + 50)));
        format!(
            "{:?}",
            (
                ok,
                w.has_state(u, STATE_A),
                w.has_state_list(u, STATE_A),
                w.state_stat(u, STATE_A, ARMORCLASS),
                w.state_stat(u, STATE_A, 350),
                w.state_stat(u, STATE_A, 351),
                w.stat(u, ARMORCLASS, 0),
                w.stat(u, MOD_STAT, 2),
                w.state_stat(u, STATE_C, ARMORCLASS),
            )
        )
    });
    assert_eq!(
        got.as_deref(),
        Some("(true, true, true, Some(25), Some(0), Some(3), 25, 4, None)")
    );
    // The list expires, with its type-12 check at that frame.
    let (expire, flags) = fx.sim.with(&mut fx.game, |_, v| {
        let l = v.state_list(p, STATE_A).unwrap();
        (v.stats.expire(l), v.stats.flags(l))
    });
    assert_eq!(expire, frame + 50);
    assert_ne!(flags & 2, 0);
    assert!(timers_12(&fx, p).contains(&(frame + 50)));
    // Out of range: nothing.
    let got = probe(&mut fx, p, SKILL, 3, |w, u, _, _| {
        format!("{}", w.apply_state(u, &StateSpec::new(N_STATES as u16 + 5)))
    });
    assert_eq!(got.as_deref(), Some("false"));
    fx.assert_clean();
}

#[test]
fn a_mod_removed_list_reaches_the_mod_when_cleared_and_when_it_expires() {
    let (mut fx, p) = fixture();
    let frame = fx.game.frame;
    let pr = install(&mut fx, move |w, u, _, _| {
        if w.has_state(u, STATE_A) {
            w.clear_state(u, STATE_A);
        } else {
            w.apply_state(u, &spec(u, STATE_A, None));
            w.apply_state(u, &spec(u, STATE_B, Some(frame + 3)));
        }
        String::new()
    });
    run(&mut fx, &pr, p, SKILL, 3);
    // Cleared by the mod body: the mod's callback ran (and switched it off).
    run(&mut fx, &pr, p, SKILL, 3);
    assert_eq!(*pr.removed.lock().unwrap(), [(p, STATE_A)]);
    assert!(!fx.sim.sys.stats.has_state(p, u32::from(STATE_A)));
    // B expires on its own.
    for _ in 0..4 {
        fx.frame();
    }
    assert_eq!(*pr.removed.lock().unwrap(), [(p, STATE_A), (p, STATE_B)]);
    assert!(!fx.sim.sys.stats.has_state(p, u32::from(STATE_B)));
    uninstall(&mut fx);
    fx.assert_clean();
}

#[test]
fn a_default_removed_list_keeps_d2rusts_callback_under_a_mod() {
    let (mut fx, p) = fixture();
    let pr = install(&mut fx, |w, u, _, _| {
        let s = StateSpec {
            on_remove: StateRemove::Default,
            ..spec(u, STATE_A, None)
        };
        w.apply_state(u, &s);
        w.clear_state(u, STATE_A);
        format!(
            "{:?}",
            (w.has_state(u, STATE_A), w.has_state_list(u, STATE_A))
        )
    });
    assert_eq!(
        run(&mut fx, &pr, p, SKILL, 3).as_deref(),
        Some("(false, false)")
    );
    assert!(pr.removed.lock().unwrap().is_empty());
    uninstall(&mut fx);
    fx.assert_clean();
}

#[test]
fn a_mod_body_clears_a_state_group_and_toggles_a_state() {
    let (mut fx, p) = fixture();
    let got = probe(&mut fx, p, SKILL, 3, |w, u, _, _| {
        for s in [STATE_A, STATE_B, STATE_C] {
            let spec = StateSpec {
                on_remove: StateRemove::StateOff,
                ..spec(u, s, None)
            };
            w.apply_state(u, &spec);
        }
        let none = w.clear_state_group(u, STATE_C, true);
        let cleared = w.clear_state_group(u, STATE_A, false);
        let after = [STATE_A, STATE_B, STATE_C].map(|s| w.has_state(u, s));
        w.state_on(u, STATE_B, true);
        let lists = w.has_state_list(u, STATE_B);
        format!(
            "{:?}",
            (none, cleared, after, w.has_state(u, STATE_B), lists)
        )
    });
    // C has no group; A's group without A takes B (state and list).
    assert_eq!(
        got.as_deref(),
        Some("(false, true, [true, false, true], true, false)")
    );
    fx.assert_clean();
}

#[test]
fn a_mod_body_registers_and_clears_state_events() {
    let (mut fx, p) = fixture();
    let got = probe(&mut fx, p, SKILL, 3, |w, u, skill, lvl| {
        let ok = w.add_state_event(u, 6, 5, skill, lvl, STATE_A);
        let bad = w.add_state_event(u, 6, 40, skill, lvl, STATE_A);
        format!("{:?}", (ok, bad))
    });
    assert_eq!(got.as_deref(), Some("(true, false)"));
    assert_eq!(
        fx.sim.sys.hooks.handlers[&p],
        [Handler {
            event: 6,
            key_type: 1,
            key: i32::from(STATE_A),
            skill: SKILL,
            level: 3,
            func: 5,
        }]
    );
    probe(&mut fx, p, SKILL, 3, |w, u, _, _| {
        w.clear_state_events(u, STATE_A);
        String::new()
    });
    assert!(fx.sim.sys.hooks.handlers.get(&p).is_none_or(Vec::is_empty));
    fx.assert_clean();
}
