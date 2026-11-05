//! Observe the hit-gated charge list, not a substitute stack.
use super::*;
use crate::rules::{BuffPulse, SkillActions};
use crate::skills::use_::bodies::{helpers2, BodyWorld};

#[test]
fn martial_charge_report_observes_damage_and_real_charge_values() {
    let (_, all) = report();
    let (_, native, data) = all.iter().find(|(n, _, _)| *n == "martial_charge").unwrap();
    assert_eq!(native, data);
    assert!(native.damage > 0);
    assert!(native.martial_count > 0);
    assert_eq!(native.martial_bonus, native.martial_count * 12);
}

#[test]
fn martial_charge_caps_incremental_bonus_and_expires_on_actual_timer() {
    let case = CASES.iter().find(|c| c.name == "martial_charge").unwrap();
    let t = tables(case, None);
    for data in [false, true] {
        let mut fx = fx_with(t.clone());
        let p = fx.spawn(UnitType::Player, 10, 10);
        for n in 1..=5 {
            fx.sim.skill_use(&mut fx.game, |w| {
                if data {
                    SkillActions::martial_charge(
                        w,
                        p,
                        204,
                        &BuffPulse {
                            skill: 0,
                            level: 5,
                            state: STATE,
                            frames: 250,
                            stats: &[(31, 12)],
                            events: &[],
                        },
                    );
                } else {
                    helpers2::charge_add(w, &t, p, 0, 5, (i32::from(STATE), 204));
                }
                let l = w.state_list(p, i32::from(STATE)).unwrap();
                assert_eq!(w.list_get(l, 204), n.min(3));
                assert_eq!(w.list_get(l, 31), n.min(3) * 12);
            });
        }
        for _ in 0..251 {
            fx.frame();
        }
        assert!(!fx.sim.sys.stats.has_state(p, u32::from(STATE)));
        fx.sim.skill_use(&mut fx.game, |w| {
            assert!(w.state_list(p, i32::from(STATE)).is_none())
        });
        fx.assert_clean();
    }
}

#[test]
fn martial_charge_without_a_target_never_adds_a_charge() {
    let case = CASES.iter().find(|c| c.name == "martial_charge").unwrap();
    // An absent target leaves all charge lists absent in both paths.
    for data in [false, true] {
        let mut t = tables(case, None);
        let text = lift(0, &Cells(case.cells.iter().copied().collect()))
            .unwrap()
            .toml;
        if data {
            t.skills[0].srvdofunc = BLOCK_SLOT;
        }
        let mut fx = fx_with(t.clone());
        let p = fx.spawn(UnitType::Player, 10, 10);
        if data {
            install(&mut fx, &text, "");
        }
        fx.sim.skill_use(&mut fx.game, |w| {
            assert_eq!(do_skill(w, &t, p, 0, 5), 0);
            assert!(w.state_list(p, i32::from(STATE)).is_none());
        });
        fx.assert_clean();
    }
}

#[test]
fn martial_charge_expiry_scales_base_ticks_at_sixty_hz() {
    let case = CASES.iter().find(|c| c.name == "martial_charge").unwrap();
    for data in [false, true] {
        let t = tables(case, None);
        let mut fx = fx_with(t.clone());
        fx.game.tick_rate = crate::tick::TickRate::new(60).unwrap();
        let p = fx.spawn(UnitType::Player, 10, 10);
        let frame = fx.game.frame;
        fx.sim.skill_use(&mut fx.game, |w| {
            if data {
                SkillActions::martial_charge(
                    w,
                    p,
                    204,
                    &BuffPulse {
                        skill: 0,
                        level: 5,
                        state: STATE,
                        frames: 250,
                        stats: &[(31, 12)],
                        events: &[],
                    },
                );
            } else {
                helpers2::charge_add(w, &t, p, 0, 5, (i32::from(STATE), 204));
            }
        });
        let timers = fx.game.timers.unit_timers(p);
        let timers: Vec<_> = timers
            .into_iter()
            .filter(|&id| fx.game.timers.event(id).is_some_and(|e| e.0 == 12))
            .collect();
        assert_eq!(timers.len(), 1);
        assert_eq!(fx.game.timers.expire(timers[0]), Some(frame + 600));
        fx.assert_clean();
    }
}
