//! Synthetic combat and wired reaction tests for each curated setting.

use super::*;
use crate::combat::{self, damage, hit, DamageRecord};
use crate::rng::Seed;
use crate::skills::fake::{combat_tables, monster_rec, FUnit, Fake};
use crate::units::UnitId;
use crate::wiring::action::tests::Fx;

mod progression;

fn parsed(key: &str, value: &str) -> GlobalRules {
    GlobalRules::from_toml(&format!("{key} = {value}")).unwrap()
}

#[test]
fn parser_defaults_and_validation() {
    assert_eq!(GlobalRules::from_toml("").unwrap(), GlobalRules::default());
    assert_eq!(
        parsed("players.stun.enabled", "false").players.stun,
        TimedRule {
            enabled: false,
            duration_percent: 100
        }
    );
    for text in [
        "version = 2",
        "max_block_chance = 101",
        "unknown = true",
        "players.stnu.enabled = false",
        "players.stun.enabled = 1",
        "players.stun.duration_percent = -1",
        "monsters.freeze.duration_percent = 10001",
        "players.hit_recovery.threshold_denominator = 0",
    ] {
        assert!(GlobalRules::from_toml(text).is_err(), "{text}");
    }
}

fn combat_world(rules: Ruleset, ty: UnitType) -> (Fake, usize, usize, combat::CombatTables) {
    let mut f = Fake {
        rules,
        ..Default::default()
    };
    let a = f.add(FUnit::new(UnitType::Player, 0));
    let d = f.add(FUnit::new(ty, 0));
    f.set(d, 7, 25_600);
    f.set(d, 20, 1000);
    f.set(d, 2, 100);
    f.set(d, 12, 1);
    f.units[d].shield = true;
    let mut m = monster_rec();
    m.velocity = 6;
    m.coldeffect = (-50i8) as u8;
    m.noshldblock = true;
    (f, a, d, combat_tables(vec![m]))
}

#[test]
fn stun_switch_and_duration_for_each_target() {
    for (name, ty) in [
        ("players", UnitType::Player),
        ("monsters", UnitType::Monster),
    ] {
        for (setting, value, expected) in [
            ("enabled", "false", None),
            ("duration_percent", "200", Some(80)),
            ("duration_percent", "0", None),
        ] {
            let rules = parsed(&format!("{name}.stun.{setting}"), value);
            let (mut f, a, d, c) = combat_world(rules.into_ruleset(), ty);
            damage::stun(&mut f, &c, a, d, 40);
            assert_eq!(
                f.units[d].state_expiry.get(&21).copied(),
                expected,
                "{name}.{setting}"
            );
        }
    }
}

#[test]
fn freeze_switch_and_duration_for_each_target() {
    for (name, ty, state) in [
        ("players", UnitType::Player, 11),
        ("monsters", UnitType::Monster, 1),
    ] {
        for (setting, value, expected) in [
            ("enabled", "false", None),
            ("duration_percent", "200", Some(80)),
            ("duration_percent", "0", None),
        ] {
            let rules = parsed(&format!("{name}.freeze.{setting}"), value);
            let (mut f, a, d, c) = combat_world(rules.into_ruleset(), ty);
            damage::freeze(&mut f, &c, d, a, 40);
            assert_eq!(
                f.units[d].state_expiry.get(&state).copied(),
                expected,
                "{name}.{setting}"
            );
        }
    }
}

#[test]
fn hit_recovery_threshold_for_each_target() {
    for (name, ty) in [
        ("players", UnitType::Player),
        ("monsters", UnitType::Monster),
    ] {
        let rules = parsed(&format!("{name}.hit_recovery.threshold_denominator"), "2");
        let (mut f, _, d, c) = combat_world(rules.into_ruleset(), ty);
        let rec = DamageRecord {
            total: 12_799,
            ..Default::default()
        };
        assert!(damage::no_get_hit(&mut f, &c.hitclass, d, &rec, 0));
        assert!(!damage::no_get_hit(
            &mut f,
            &c.hitclass,
            d,
            &DamageRecord {
                total: 12_800,
                ..rec
            },
            0
        ));
        f.rules = Ruleset::Original;
        assert!(!damage::no_get_hit(&mut f, &c.hitclass, d, &rec, 0));
    }
}

#[test]
fn block_cap_for_each_target() {
    for ty in [UnitType::Player, UnitType::Monster] {
        let (mut f, _, d, c) = combat_world(Ruleset::Original, ty);
        assert_eq!(hit::block_chance(&f, &c, d, true), 75);
        f.rules = parsed("max_block_chance", "30").into_ruleset();
        assert_eq!(hit::block_chance(&f, &c, d, true), 30);
        f.rules = parsed("max_block_chance", "100").into_ruleset();
        assert_eq!(hit::block_chance(&f, &c, d, true), 100);
    }
}

fn wired(rules: Ruleset, ty: UnitType) -> (Fx, UnitId, UnitId) {
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

fn react(fx: &mut Fx, a: UnitId, d: UnitId, flags: u16) -> DamageRecord {
    let mut rec = DamageRecord {
        result: combat::result::HIT | flags,
        total: 20_000,
        ..Default::default()
    };
    fx.sim.combat(&mut fx.game, |w, _| {
        crate::wiring::action::reaction::reaction(w, a, d, &mut rec)
    });
    rec
}

#[test]
fn hit_recovery_off_preserves_attack_for_each_target() {
    for (name, ty, attack) in [
        ("players", UnitType::Player, 7),
        ("monsters", UnitType::Monster, 4),
    ] {
        let (mut fx, a, d) = wired(
            parsed(&format!("{name}.hit_recovery.enabled"), "false").into_ruleset(),
            ty,
        );
        fx.sim.sys.units.get_mut(d).unwrap().mode = attack;
        // A stunned defender must not bypass the explicit off switch either.
        fx.sim.combat(&mut fx.game, |w, _| {
            use crate::combat::CombatWorld;
            w.create_state_list(d, 21, a, 200);
        });
        react(&mut fx, a, d, combat::result::GET_HIT);
        assert_eq!(fx.sim.sys.units.get(d).unwrap().mode, attack, "{name}");
    }
}

#[test]
fn hit_recovery_duration_changes_animation_for_each_target() {
    for (name, ty) in [
        ("players", UnitType::Player),
        ("monsters", UnitType::Monster),
    ] {
        let speed = |rules| {
            let (mut fx, a, d) = wired(rules, ty);
            react(&mut fx, a, d, combat::result::GET_HIT);
            fx.sim.sys.units.get(d).unwrap().anim.speed
        };
        let normal = speed(Ruleset::Original);
        assert!(normal > 0);
        assert_eq!(
            speed(parsed(&format!("{name}.hit_recovery.duration_percent"), "200").into_ruleset()),
            normal / 2
        );
    }
}

#[test]
fn knockback_off_preserves_mode_for_each_target() {
    for (name, ty) in [
        ("players", UnitType::Player),
        ("monsters", UnitType::Monster),
    ] {
        let (mut fx, a, d) = wired(
            parsed(&format!("{name}.knockback.enabled"), "false").into_ruleset(),
            ty,
        );
        react(&mut fx, a, d, combat::result::KNOCKBACK);
        assert_eq!(fx.sim.sys.units.get(d).unwrap().mode, 1, "{name}");
    }
}

#[test]
fn knockback_duration_changes_travel_speed_for_each_target() {
    for (name, ty) in [
        ("players", UnitType::Player),
        ("monsters", UnitType::Monster),
    ] {
        let velocity = |rules| {
            let (mut fx, a, d) = wired(rules, ty);
            react(&mut fx, a, d, combat::result::KNOCKBACK);
            fx.sim.hooks().path_velocity(d)
        };
        assert_eq!(velocity(Ruleset::Original), 4096);
        assert_eq!(
            velocity(parsed(&format!("{name}.knockback.duration_percent"), "200").into_ruleset()),
            2048
        );
    }
}

#[test]
fn block_animation_off_keeps_block_result_for_each_target() {
    for ty in [UnitType::Player, UnitType::Monster] {
        let (mut fx, a, d) = wired(parsed("block_animation", "false").into_ruleset(), ty);
        let rec = react(&mut fx, a, d, combat::result::BLOCK);
        assert_ne!(rec.result & combat::result::BLOCK, 0);
        assert_eq!(fx.sim.sys.units.get(d).unwrap().mode, 1);
        let (mut fx, a, d) = wired(Ruleset::Original, ty);
        react(&mut fx, a, d, combat::result::BLOCK);
        assert_eq!(
            fx.sim.sys.units.get(d).unwrap().mode,
            if ty == UnitType::Player { 9 } else { 6 }
        );
    }
}

#[test]
fn default_rules_reproduce_combat_and_seed_streams() {
    for ty in [UnitType::Player, UnitType::Monster] {
        for seed in 0..64 {
            let run = |rules| {
                let (mut f, a, d, c) = combat_world(rules, ty);
                f.units[d].seed = Seed::new(seed, 0);
                let hits: Vec<_> = [0, 255, 256, 500, 1600, 3200, 6400, 25_600]
                    .into_iter()
                    .map(|total| {
                        damage::no_get_hit(
                            &mut f,
                            &c.hitclass,
                            d,
                            &DamageRecord {
                                total,
                                ..Default::default()
                            },
                            0,
                        )
                    })
                    .collect();
                damage::stun(&mut f, &c, a, d, 400);
                damage::freeze(&mut f, &c, d, a, 40);
                (
                    hits,
                    hit::block_chance(&f, &c, d, true),
                    f.units[d].seed,
                    f.log,
                )
            };
            assert_eq!(
                run(Ruleset::Original),
                run(GlobalRules::default().into_ruleset())
            );
        }
        for flag in [
            combat::result::GET_HIT,
            combat::result::BLOCK,
            combat::result::KNOCKBACK,
        ] {
            let run = |rules| {
                let (mut fx, a, d) = wired(rules, ty);
                let rec = react(&mut fx, a, d, flag);
                let r = fx.sim.sys.units.get(d).unwrap();
                (rec, r.mode, r.anim.speed, fx.sim.hooks().path_velocity(d))
            };
            assert_eq!(
                run(Ruleset::Original),
                run(GlobalRules::default().into_ruleset())
            );
        }
    }
}

mod economy;

mod feel;

#[test]
fn block_animation_off_blocks_without_locking_the_defender() {
    // "Block without the animation": the block still avoids the damage (the
    // result keeps its BLOCK flag), but the defender plays nothing, keeps doing
    // what it was doing, and is not held in any state or lockout.
    for (ty, attack) in [(UnitType::Player, 7), (UnitType::Monster, 4)] {
        let (mut fx, a, d) = wired(parsed("block_animation", "false").into_ruleset(), ty);
        fx.sim.sys.units.get_mut(d).unwrap().mode = attack;
        for frame in [100, 101, 200] {
            fx.game.frame = frame;
            let rec = react(&mut fx, a, d, combat::result::BLOCK);
            assert_ne!(rec.result & combat::result::BLOCK, 0, "{ty:?}");
            assert_eq!(fx.sim.sys.units.get(d).unwrap().mode, attack, "{ty:?}");
        }
        // No lockout stat (stat 95, the player's last-block frame) and no hold state.
        assert_eq!(fx.sim.sys.stats.unit_total(d, 95, 0), 0);
        for state in [21u16, 54, 1] {
            assert!(fx.sim.sys.stats.unit_list(d).is_none_or(|l| fx
                .sim
                .sys
                .stats
                .list_of_state(l, u32::from(state))
                .is_none()));
        }
    }
}

const TELEPORT: i32 = 54;

/// `d` is `ty` in its attack/cast `mode`, `frame` 8.8 frames into a 2048-frame cast
/// of `TELEPORT`, and takes a hit big enough to interrupt; the mode it ends in.
fn mode_after_a_hit_at(rules: &str, ty: UnitType, mode: u32, frame: i32) -> u32 {
    let (mut fx, a, d) = wired(GlobalRules::from_toml(rules).unwrap().into_ruleset(), ty);
    fx.sim.hooks().x.used.insert(
        d,
        crate::skills::SkillEntry {
            skill: TELEPORT,
            base: 1,
            owner_guid: -1,
            ..Default::default()
        },
    );
    let r = fx.sim.sys.units.get_mut(d).unwrap();
    r.mode = mode;
    r.anim.frame = frame;
    r.anim.frame_count = 2048;
    react(&mut fx, a, d, combat::result::GET_HIT);
    fx.sim.sys.units.get(d).unwrap().mode
}

#[test]
fn a_cast_past_its_point_is_not_interrupted_by_a_hit() {
    for (ty, cast, get_hit) in [(UnitType::Player, 10, 4), (UnitType::Monster, 8, 3)] {
        let at = |rules, frame| mode_after_a_hit_at(rules, ty, cast, frame);
        let half = "[cast]\nuninterruptible_after_percent = 50\n";
        // Before the point a hit interrupts, as it does without the rule...
        assert_eq!(at(half, 1023), get_hit, "{ty:?}");
        assert_eq!(at("", 2000), get_hit, "{ty:?}: D2Gamma's rule");
        // ...from the point on it does not.
        assert_eq!(at(half, 1024), cast, "{ty:?}");
        assert_eq!(at(half, 2047), cast, "{ty:?}");
        // A skill's own entry wins over the default, both ways.
        let own =
            format!("{half}[[cast.skill]]\nid = {TELEPORT}\nuninterruptible_after_percent = 0\n");
        assert_eq!(at(&own, 0), cast, "{ty:?}: 0% is from the first frame");
        let never =
            format!("{half}[[cast.skill]]\nid = {TELEPORT}\nuninterruptible_after_percent = 100\n");
        assert_eq!(at(&never, 2047), get_hit, "{ty:?}: 100% is never");
        let other = "[[cast.skill]]\nid = 7\nuninterruptible_after_percent = 0\n";
        assert_eq!(at(other, 2047), get_hit, "{ty:?}: another skill's point");
    }
}

#[test]
fn a_unit_not_in_a_skill_mode_is_interrupted_whatever_the_point() {
    // Neutral with a stale used skill: nothing is being cast.
    for (ty, neutral, get_hit) in [(UnitType::Player, 1, 4), (UnitType::Monster, 1, 3)] {
        let rules = "[cast]\nuninterruptible_after_percent = 0\n";
        assert_eq!(
            mode_after_a_hit_at(rules, ty, neutral, 2047),
            get_hit,
            "{ty:?}"
        );
    }
}

#[test]
fn the_cast_rules_are_checked() {
    for text in [
        "[cast]\nuninterruptible_after_percent = 101\n",
        "[[cast.skill]]\nid = 1\nuninterruptible_after_percent = 101\n",
        "[[cast.skill]]\nid = 1\nuninterruptible_after_percent = 5\n[[cast.skill]]\nid = 1\nuninterruptible_after_percent = 6\n",
        "[[cast.skill]]\nid = 1\n",
        "[cast]\nafter = 5\n",
    ] {
        assert!(GlobalRules::from_toml(text).is_err(), "{text}");
    }
    assert!(GlobalRules::from_toml("[cast]\nuninterruptible_after_percent = 100\n").is_ok());
    assert!(GlobalRules::from_toml("").unwrap().cast.is_off());
}
