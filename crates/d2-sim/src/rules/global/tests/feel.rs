//! Each configurable number changes a combat, path or animation result.

use super::*;
use crate::tick::TickRate;
use crate::units::anim::{schedule_with_feel, Events, Form};
use crate::units::anim_rate::{anim_rate, anim_rate_with, frame_bonus_with, Rate, RateInput};
use crate::units::record::{Anim, Sequence, ANIM_EVENTS};

#[test]
fn knockback_numbers_change_real_paths_for_both_targets() {
    for (name, ty) in [
        ("players", UnitType::Player),
        ("monsters", UnitType::Monster),
    ] {
        for (key, value) in [("knockback_velocity", "2048"), ("knockback_distance", "9")] {
            let (mut fx, a, d) = wired(
                parsed(&format!("{name}.control.{key}"), value).into_ruleset(),
                ty,
            );
            react(&mut fx, a, d, combat::result::KNOCKBACK);
            let path = fx.sim.hooks().paths.as_ref().unwrap().dynamic(d).unwrap();
            if key == "knockback_velocity" {
                assert_eq!(path.velocity, 2048);
            } else {
                assert_eq!(path.dist_budget, 9);
            }
        }
    }
    let (mut fx, a, d) = wired(
        parsed("monsters.control.sandleaper_knockback_distance", "15").into_ruleset(),
        UnitType::Monster,
    );
    std::sync::Arc::make_mut(&mut fx.sim.hooks().tables)
        .combat
        .monstats[0]
        .baseid = 78;
    react(&mut fx, a, d, combat::result::KNOCKBACK);
    assert_eq!(
        fx.sim
            .hooks()
            .paths
            .as_ref()
            .unwrap()
            .dynamic(d)
            .unwrap()
            .dist_budget,
        15
    );
}

#[test]
fn walk_and_run_animation_numbers_reach_the_wired_unit() {
    use crate::units::hooks::{Sim, UnitHooks};
    for (key, mode) in [("player_walk_speed", 2), ("player_run_speed", 3)] {
        let (mut fx, _, d) = wired(
            parsed(&format!("animation.{key}"), "80").into_ruleset(),
            UnitType::Player,
        );
        fx.sim.sys.units.get_mut(d).unwrap().mode = mode;
        fx.stats(d, &[(67, 100)]);
        let system = &mut fx.sim.sys;
        let mut sim = Sim {
            game: &mut fx.game,
            units: &mut system.units,
            stats: &mut system.stats,
            data: &system.data,
        };
        assert_eq!(system.hooks.anim_rate(&mut sim, d), 80, "{key}");
    }
}

#[test]
fn control_caps_and_freeze_difficulty_for_both_targets() {
    for (name, ty, frozen) in [
        ("players", UnitType::Player, 11),
        ("monsters", UnitType::Monster, 1),
    ] {
        for (key, value, effect, expected) in [
            ("stun_max_frames", "20", "stun", 20),
            ("freeze_max_frames", "20", "freeze", 20),
        ] {
            let (mut f, a, d, c) = combat_world(
                parsed(&format!("{name}.control.{key}"), value).into_ruleset(),
                ty,
            );
            if effect == "stun" {
                damage::stun(&mut f, &c, a, d, 400);
            } else {
                damage::freeze(&mut f, &c, d, a, 400);
            }
            assert_eq!(
                f.units[d].state_expiry[&if effect == "stun" { 21 } else { frozen }],
                expected
            );
        }
    }
    let (mut f, a, d, c) = combat_world(
        parsed("monsters.control.hireling_stun_max_frames", "6").into_ruleset(),
        UnitType::Monster,
    );
    f.units[d].hireling = true;
    damage::stun(&mut f, &c, a, d, 400);
    assert_eq!(f.units[d].state_expiry[&21], 6);
    for (setting, expected) in [(0, true), (100, false)] {
        let (mut f, a, d, c) = combat_world(
            parsed(
                "monsters.control.unique_stun_resist_percent",
                &setting.to_string(),
            )
            .into_ruleset(),
            UnitType::Monster,
        );
        f.units[d].flags = 8;
        damage::stun(&mut f, &c, a, d, 400);
        assert_eq!(f.units[d].state_expiry.contains_key(&21), expected);
    }
    for difficulty in 0..3 {
        let (mut f, a, d, c) = combat_world(
            parsed("monsters.control.freeze_divisors", "[2, 4, 8]").into_ruleset(),
            UnitType::Monster,
        );
        f.difficulty = difficulty;
        let mut c = c;
        c.monstats[0].coldeffect_n = (-50i8) as u8;
        c.monstats[0].coldeffect_h = (-50i8) as u8;
        damage::freeze(&mut f, &c, d, a, 80);
        assert_eq!(f.units[d].state_expiry[&1], [40, 20, 10][difficulty]);
    }
}

#[test]
fn hit_thresholds_and_each_seeded_band_change_outcomes() {
    for (name, ty) in [
        ("players", UnitType::Player),
        ("monsters", UnitType::Monster),
    ] {
        let run = |rules: Ruleset, total, class, seed| {
            let (mut f, _, d, _) = combat_world(rules, ty);
            f.units[d].seed = Seed::new(seed, 0);
            let out = damage::no_get_hit(
                &mut f,
                &[*b"1hss", *b"none", *b"club", *b"2hsl"],
                d,
                &DamageRecord {
                    total,
                    ..Default::default()
                },
                class,
            );
            (out, f.units[d].seed)
        };
        assert_ne!(
            run(Ruleset::Original, 12800, 0, 0).0,
            run(
                parsed(&format!("{name}.hit_recovery.minimum_damage"), "20000").into_ruleset(),
                12800,
                0,
                0
            )
            .0
        );
        for (class, total) in [(0, 12800), (1, 6400), (2, 3200), (3, 1600)] {
            assert_ne!(
                run(Ruleset::Original, total, class, 0).0,
                run(
                    parsed(&format!("{name}.hit_recovery.divisors"), "[4, 4, 4, 4]").into_ruleset(),
                    total,
                    class,
                    0
                )
                .0,
                "{name} class {class}"
            );
        }
        for (key, value) in [
            ("band_denominators", "[1, 1]"),
            ("band_denominators", "[2, 2]"),
            ("band_roll_sides", "[1, 4]"),
            ("band_roll_sides", "[2, 1]"),
        ] {
            let rules = parsed(&format!("{name}.hit_recovery.{key}"), value).into_ruleset();
            assert!(
                (0..64).any(|seed| [2000, 4000].into_iter().any(|total| run(
                    Ruleset::Original,
                    total,
                    1,
                    seed
                ) != run(
                    rules.clone(),
                    total,
                    1,
                    seed
                ))),
                "{name}.{key} {value}"
            );
        }
    }
}

fn input(mode: u32) -> RateInput {
    RateInput {
        applies: true,
        t: 0,
        m: mode,
        s: 256,
        w: 213,
        attackrate: 100,
        velocitypercent: 100,
        other_animrate: 100,
        has_path: true,
        ..Default::default()
    }
}

#[test]
fn every_breakpoint_number_changes_rate() {
    for (key, value, mode, raw, holy, seq, velocity) in [
        ("attack_percent", "[15, 80]", 7, 0, false, false, false),
        ("attack_percent", "[120, 175]", 7, 0, false, false, false),
        ("cast_base_percent", "80", 10, 0, false, false, false),
        ("cast_max_percent", "80", 10, 0, false, false, false),
        ("recovery_base_percent", "80", 4, 0, false, false, false),
        ("block_base_percent", "80", 9, 0, false, false, false),
        ("holy_shield_block_percent", "80", 9, 0, true, false, false),
        ("block_min_speed", "200", 9, 0, false, false, false),
        ("movement_min_percent", "120", 2, 0, false, false, true),
        ("movement_max_percent", "80", 2, 0, false, false, true),
        ("other_percent", "[15, 80]", 1, 0, false, false, false),
        ("other_percent", "[120, 175]", 1, 0, false, false, false),
        ("speed_cap", "80", 7, 0, false, false, false),
        ("sequence_attack_penalty", "60", 7, 0, false, true, false),
    ] {
        let mut i = input(mode);
        i.item = [raw; 5];
        i.holyshield = holy;
        i.player_mode_18 = seq;
        i.velocity_mode = velocity;
        let rules = parsed(&format!("animation.{key}"), value);
        assert_ne!(anim_rate(&i), anim_rate_with(&i, &rules.animation), "{key}");
    }
    for (row, mode) in [(0, 7), (1, 4), (2, 10), (3, 9), (4, 2)] {
        let mut i = input(mode);
        i.item[row] = 60;
        i.velocity_mode = row == 4;
        let mut coefficients = [120, 120, 120, 120, 150];
        coefficients[row] = 60;
        let rules = parsed("animation.diminishing", &format!("{coefficients:?}"));
        assert_ne!(
            anim_rate(&i),
            anim_rate_with(&i, &rules.animation),
            "diminishing row {row}"
        );
    }
    let rules = parsed("animation.frame_bonus", &format!("{:?}", [[0; 7]; 12]));
    assert_eq!(frame_bonus_with(0, 0, 7, false, None, &rules.animation), 0);
    assert_eq!(
        frame_bonus_with(0, 0, 7, false, None, &AnimationRules::default()),
        1
    );
}

#[test]
fn defaults_preserve_all_rate_modes_and_speed_caps() {
    for t in 0..=1 {
        for m in 0..=20 {
            for raw in [-50, 0, 60, 1000] {
                let mut i = input(m);
                i.t = t;
                i.item = [raw; 5];
                assert_eq!(
                    anim_rate(&i),
                    anim_rate_with(&i, &parsed("version", "1").animation)
                );
            }
        }
    }
    // Fixed expected outputs also pin the pre-K5c arithmetic.
    for (mode, expected) in [(7, 256), (10, 256), (9, 128), (4, 128), (1, 256)] {
        assert!(
            matches!(anim_rate_with(&input(mode), &AnimationRules::default()), Ok(Rate::Set {speed, ..}) if speed == expected)
        );
    }
}

#[test]
fn classic_steps_pin_events_and_sequences_to_the_same_clock() {
    let rate = TickRate::GAME;
    let events = [0; ANIM_EVENTS];
    let classic = parsed("animation.classic_25hz", "true")
        .animation
        .classic_25hz;
    let schedule = |classic| {
        schedule_with_feel(
            (rate, classic),
            Form::Main { bonus: 0 },
            0,
            350,
            10 << 8,
            0,
            Events::Record {
                byte_0f: 0,
                events: &events,
            },
        )
        .unwrap()
        .unwrap()
    };
    assert_ne!(schedule(false).events, schedule(classic).events);
    for e in schedule(classic).events {
        assert_ne!(
            rate.animation_share(300, e.expire, classic),
            0,
            "event must land on a 40 ms boundary"
        );
    }
    for classic in [false, true] {
        let mut a = Anim {
            frame_count: 1000 << 8,
            sequence: Some(Sequence {
                frame_count: 1000 << 8,
                speed: 300,
                pos: 0,
                events: vec![],
                drawn: vec![],
                event_before: None,
            }),
            ..Default::default()
        };
        for t in 0..60 {
            let before = a.sequence.as_ref().unwrap().pos;
            crate::units::anim::advance_sequence_with_feel(&mut a, rate, t, classic);
            assert_eq!(
                a.sequence.as_ref().unwrap().pos - before,
                rate.animation_share(300, t, classic)
            );
        }
        assert_eq!(a.sequence.unwrap().pos, 300 * 25);
    }
    assert_eq!(
        (0..60)
            .filter(|&t| rate.animation_share(300, t, true) != 0)
            .count(),
        25
    );
    for t in [-10, 0, 10, i32::MAX] {
        assert_eq!(rate.animation_share(300, t, false), rate.share(300, t));
    }
}

#[test]
fn invalid_feel_settings_are_rejected_by_key() {
    for (key, value) in [
        ("animation.diminishing", "[0, 120, 120, 120, 150]"),
        ("animation.attack_percent", "[175, 15]"),
        ("animation.speed_cap", "32768"),
        ("animation.other_percent", "[0, 10]"),
        ("animation.block_min_speed", "0"),
        ("animation.movement_max_percent", "10"),
        ("animation.player_walk_speed", "0"),
        ("players.control.stun_max_frames", "0"),
        ("monsters.control.hireling_stun_max_frames", "0"),
        ("monsters.control.unique_stun_resist_percent", "101"),
        ("monsters.control.freeze_divisors", "[1, 0, 1]"),
        ("players.control.freeze_max_frames", "0"),
        ("players.control.knockback_velocity", "0"),
        ("monsters.control.knockback_distance", "0"),
        ("players.hit_recovery.divisors", "[1, 16, 32, 64]"),
        ("players.hit_recovery.band_denominators", "[2, 10]"),
        ("players.hit_recovery.band_roll_sides", "[3, 4]"),
        ("players.hit_recovery.minimum_damage", "-1"),
    ] {
        let error = GlobalRules::from_toml(&format!("{key} = {value}"))
            .unwrap_err()
            .to_string();
        assert!(error.contains(key), "{error}");
    }
}

#[test]
fn classic_variants_action_events_and_completion_share_boundaries() {
    use crate::wiring::action::unit_update::anim_complete_with_feel;
    let mut events = [0; ANIM_EVENTS];
    events[0] = 1;
    events[3] = 2;
    events[6] = 4;
    let rate = TickRate::GAME;
    for start in 0..6 {
        for form in [
            Form::Main { bonus: 0 },
            Form::StartFrame(1),
            Form::Percent(50),
            Form::Frames(3),
        ] {
            let scheduled = schedule_with_feel(
                (rate, true),
                form,
                start,
                350,
                12 << 8,
                (start - 4) << 8,
                Events::Record {
                    byte_0f: 0,
                    events: &events,
                },
            )
            .unwrap()
            .unwrap();
            for e in &scheduled.events {
                assert!(
                    rate.animation_share(350, e.expire, true) > 0,
                    "{form:?} {e:?}"
                );
            }
        }
        // A pending final animation frame cannot complete during a zero-step tick.
        let a = Anim {
            frame: 11 << 8,
            frame_count: 12 << 8,
            speed: 350,
            ..Default::default()
        };
        assert_eq!(
            anim_complete_with_feel(&a, rate, start, true),
            rate.animation_share(350, start, true) > 0
        );
    }
}

#[test]
fn configured_speed_cap_survives_recovery_duration_scaling() {
    use crate::units::hooks::{Sim, UnitHooks};
    let rules = GlobalRules::from_toml(
        "animation.speed_cap = 100\nplayers.hit_recovery.duration_percent = 50",
    )
    .unwrap();
    let (mut fx, _, d) = wired(rules.into_ruleset(), UnitType::Player);
    fx.sim.sys.units.get_mut(d).unwrap().mode = 4;
    let system = &mut fx.sim.sys;
    let mut sim = Sim {
        game: &mut fx.game,
        units: &mut system.units,
        stats: &mut system.stats,
        data: &system.data,
    };
    assert_eq!(system.hooks.anim_rate(&mut sim, d), 100);
}

#[test]
fn classic_zero_steps_clear_action_markers_without_replaying_them() {
    use crate::units::hooks::Sim;
    for sequence in [false, true] {
        let (mut fx, _, d) = wired(
            parsed("animation.classic_25hz", "true").into_ruleset(),
            UnitType::Player,
        );
        fx.game.tick_rate = TickRate::GAME;
        fx.game.frame = 0; // No 40 ms animation boundary on this host tick.
        let mut events = [0; ANIM_EVENTS];
        events[0] = 2;
        let a = &mut fx.sim.sys.units.get_mut(d).unwrap().anim;
        a.speed = 350;
        a.frame = 0;
        a.frame_count = 12 << 8;
        a.action_frame = 2;
        a.record = Some(crate::units::record::AnimRecord {
            frames: 12,
            byte_0f: 0,
            events,
        });
        if sequence {
            a.sequence = Some(Sequence {
                frame_count: 12 << 8,
                speed: 350,
                pos: 0,
                events: events.to_vec(),
                event_before: None,
                drawn: vec![],
            });
        }
        let system = &mut fx.sim.sys;
        let mut sim = Sim {
            game: &mut fx.game,
            units: &mut system.units,
            stats: &mut system.stats,
            data: &system.data,
        };
        system.hooks.refresh_unit_animation(&mut sim, d);
        let a = &sim.units.get(d).unwrap().anim;
        assert_eq!((a.frame, a.frame_count, a.action_frame), (0, 12 << 8, 0));
        if sequence {
            assert_eq!(a.sequence.as_ref().unwrap().pos, 0);
        }
    }
}
