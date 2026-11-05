//! Amount and curve outcomes, and strict validation of the economy controls.
use super::*;
use crate::combat::vitals::experience::{gold_penalty, gold_penalty_in};
use crate::world::hirelings::rows::{threshold, HirelingRow};
use crate::world::npc::npc_rules::merc_exp;

#[test]
fn economy_rules_validation() {
    for text in [
        "drops.other_player_divisor = 0",
        "drops.other_player_divisor = 9",
        "drops.magic_find_percent = 10001",
        "drops.magic_find_cap = 10001",
        "drops.nodrop_percent = 10001",
        "drops.players_setting = 1",
        "drops.unknown = true",
        "socket_level_bands = [40, 25]",
        "socket_level_bands = [0, 40]",
        "socket_level_bands = [25]",
        "runeword_max_runes = 7",
        "death_gold_percent = 10001",
        "hireling_experience_curve_percent = 0",
        "hireling_experience_curve_percent = 10001",
    ] {
        assert!(GlobalRules::from_toml(text).is_err(), "{text}");
    }
}

#[test]
fn economy_rules_death_gold_all_modes() {
    let defaults = GlobalRules::default().into_ruleset();
    let doubled = parsed("death_gold_percent", "200").into_ruleset();
    let off = parsed("death_gold_percent", "0").into_ruleset();
    for pvp in [false, true] {
        for game in 0..=3 {
            for (carried, banked) in [(10000, 90000), (30000, 70000), (100, 100), (0, 100000)] {
                let base = gold_penalty(20, carried, banked, pvp, game, i32::MAX, i32::MAX);
                assert_eq!(
                    gold_penalty_in(
                        &defaults,
                        20,
                        carried,
                        banked,
                        pvp,
                        game,
                        i32::MAX,
                        i32::MAX
                    ),
                    base
                );
                let changed =
                    gold_penalty_in(&doubled, 20, carried, banked, pvp, game, i32::MAX, i32::MAX);
                assert!(changed.goldlost >= base.goldlost);
                let zero =
                    gold_penalty_in(&off, 20, carried, banked, pvp, game, i32::MAX, i32::MAX);
                assert_eq!(zero.goldlost, 0);
                if !pvp {
                    assert_eq!(zero.gold, Some(0));
                    assert_eq!(zero.drop, Some(carried));
                }
            }
        }
    }
    let loss = gold_penalty_in(&doubled, 20, 30000, 70000, false, 0, i32::MAX, i32::MAX);
    assert_eq!(
        (loss.gold, loss.goldbank, loss.drop, loss.goldlost),
        (Some(0), Some(60000), None, 40000)
    );
    // Single-player protection is still applied after the multiplier.
    assert_eq!(
        gold_penalty_in(&doubled, 20, 100, 100, false, 3, i32::MAX, i32::MAX).goldlost,
        0
    );
}

#[test]
fn economy_rules_hireling_thresholds() {
    let row = HirelingRow {
        exp_lvl: 105,
        ..Default::default()
    };
    for level in 1..=98 {
        let base = threshold(105, level);
        assert_eq!(merc_exp(&Ruleset::Original, &row, level), base);
        assert_eq!(
            merc_exp(&GlobalRules::default().into_ruleset(), &row, level),
            base
        );
        assert_eq!(
            merc_exp(
                &parsed("hireling_experience_curve_percent", "50").into_ruleset(),
                &row,
                level
            ),
            (base as u32 / 2) as i32
        );
    }
    assert_eq!(
        merc_exp(
            &parsed("hireling_experience_curve_percent", "200").into_ruleset(),
            &row,
            6
        ),
        52920
    );
}
