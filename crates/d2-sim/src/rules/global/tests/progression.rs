//! Tests of resistance damage outcomes, player level thresholds and death XP loss.

use super::*;
use crate::combat::vitals::{experience::death_experience, VitalsTables};
use d2_data::tables::{Experience, Record};

fn damage_result(
    rules: Ruleset,
    stat: u16,
    bonus: i32,
    field: fn(&mut DamageRecord) -> &mut i32,
) -> i32 {
    let (mut f, a, d, c) = combat_world(rules, UnitType::Player);
    f.units[a].kind = UnitType::Monster;
    f.set(d, stat, 100);
    f.set(d, 40, bonus);
    let mut rec = DamageRecord::default();
    *field(&mut rec) = 1000;
    damage::totals(&mut f, &c, Some(a), d, &mut rec);
    *field(&mut rec)
}

#[test]
fn resistance_caps_change_damage_and_effect_lengths() {
    type Field = fn(&mut DamageRecord) -> &mut i32;
    let fire: Field = |r| &mut r.fire;
    let physical: Field = |r| &mut r.physical;
    let poison_length: Field = |r| &mut r.poison_len;
    for (key, value, stat, bonus, field, baseline, changed) in [
        ("elemental_cap", "50", 39, 0, fire, 250, 500),
        ("elemental_max_cap", "80", 39, 30, fire, 50, 200),
        ("physical_cap", "25", 36, 0, physical, 500, 750),
        ("other_cap", "50", 110, 0, poison_length, 250, 500),
    ] {
        assert_eq!(
            damage_result(Ruleset::Original, stat, bonus, field),
            baseline
        );
        assert_eq!(
            damage_result(
                parsed(&format!("resistances.{key}"), value).into_ruleset(),
                stat,
                bonus,
                field
            ),
            changed,
            "{key}"
        );
        assert_eq!(
            damage_result(GlobalRules::default().into_ruleset(), stat, bonus, field),
            baseline
        );
    }
}

#[test]
fn difficulty_penalties_change_each_difficulty_and_keep_monsters_uncapped() {
    for expansion in [false, true] {
        let key = if expansion {
            "expansion_penalties"
        } else {
            "classic_penalties"
        };
        for difficulty in 0..3 {
            let run = |rules, ty| {
                let (mut f, a, d, mut c) = combat_world(rules, ty);
                f.expansion = expansion;
                f.difficulty = difficulty;
                f.units[a].kind = UnitType::Monster;
                f.set(d, 39, 0);
                c.difficultylevels[difficulty].resistpenalty = [0, -40i32, -100][difficulty] as u32;
                let mut rec = DamageRecord {
                    fire: 1000,
                    ..Default::default()
                };
                damage::totals(&mut f, &c, Some(a), d, &mut rec);
                rec.fire
            };
            let rules = parsed(&format!("resistances.{key}"), "[-10, -30, -60]");
            assert_eq!(
                run(rules.clone().into_ruleset(), UnitType::Player),
                [1100, 1300, 1600][difficulty]
            );
            assert_eq!(
                run(Ruleset::Original, UnitType::Player),
                run(GlobalRules::default().into_ruleset(), UnitType::Player)
            );
            assert_eq!(run(rules.into_ruleset(), UnitType::Monster), 1000);
        }
    }
    let rules = parsed("resistances.elemental_cap", "10").into_ruleset();
    let (mut f, a, d, c) = combat_world(rules, UnitType::Monster);
    f.set(d, 39, 100);
    let mut rec = DamageRecord {
        fire: 1000,
        ..Default::default()
    };
    damage::totals(&mut f, &c, Some(a), d, &mut rec);
    assert_eq!(rec.fire, 0, "monster immunity is unchanged");
}

fn xp_tables() -> VitalsTables {
    VitalsTables {
        charstats: Vec::new(),
        experience: [4, 0, 1000, 3000, 6000]
            .into_iter()
            .map(|value| {
                let mut row = Experience::decode(&[0; Experience::SIZE]);
                row.amazon = value;
                row
            })
            .collect(),
    }
}

#[test]
fn experience_curve_changes_level_from_xp_and_both_threshold_lookups() {
    let tables = xp_tables();
    let rules = parsed("experience_curve_percent", "200").into_ruleset();
    assert_eq!(tables.threshold_in(&rules, 0, 2), 6000);
    assert_eq!(tables.try_threshold_in(&rules, 0, 2), Some(6000));
    assert_eq!(tables.try_threshold_in(&rules, 0, 99), None);
    assert_eq!(tables.level_from_exp_in(&Ruleset::Original, 0, 1500), 2);
    assert_eq!(tables.level_from_exp_in(&rules, 0, 1500), 1);
    for level in 0..4 {
        assert_eq!(
            tables.threshold_in(&GlobalRules::default().into_ruleset(), 0, level),
            tables.threshold(0, level)
        );
    }
}

#[test]
fn death_experience_multiplier_changes_loss_and_preserves_level_floor() {
    use crate::combat::vitals::VitalsUnits;
    let tables = xp_tables();
    for (multiplier, expected, remaining) in [
        (100, Some(200), 1800),
        (200, Some(400), 1600),
        (0, None, 2000),
        (10_000, Some(1000), 1001),
    ] {
        let rules = parsed("death_experience_percent", &multiplier.to_string()).into_ruleset();
        let (mut fx, _, d) = wired(rules, UnitType::Player);
        fx.stats(d, &[(12, 2), (13, 2000)]);
        fx.sim.combat(&mut fx.game, |w, _| {
            assert_eq!(death_experience(w, &tables, d, 10), expected);
            assert_eq!(w.stat(d, 13), remaining);
        });
    }
    for rules in [Ruleset::Original, GlobalRules::default().into_ruleset()] {
        let (mut fx, _, d) = wired(rules, UnitType::Player);
        fx.stats(d, &[(12, 2), (13, 2000)]);
        fx.sim.combat(&mut fx.game, |w, _| {
            assert_eq!(death_experience(w, &tables, d, 10), Some(200))
        });
    }
}

#[test]
fn progression_parser_ranges() {
    for text in [
        "resistances.elemental_cap = 101",
        "resistances.elemental_max_cap = 50",
        "resistances.physical_cap = -1",
        "resistances.other_cap = 101",
        "resistances.expansion_penalties = [0, -20]",
        "resistances.classic_penalties = [0, -20, -1001]",
        "experience_curve_percent = 0",
        "experience_curve_percent = 10001",
        "death_experience_percent = 10001",
    ] {
        assert!(GlobalRules::from_toml(text).is_err(), "{text}");
    }
    assert_eq!(percent(u32::MAX, 10_000), u32::MAX);
}
