//! The save load's `nextexp` step (`ActionCharacter::next_exp`) on the
//! game's rules: a mod game's experience hooks answer levels past the
//! experience table's rows; D2Gamma's own read the table, and a level
//! past it is reported unapplied instead of stopping the load.

use std::sync::Arc;

use d2_data::tables::{Experience, Record};
use d2_sim::combat::vitals::VitalsTables;
use d2_sim::rules::{FormulaHooks, ModRules, Ruleset};

use super::trade_quests::{Fx, Sim};
use crate::adapters::character::{ActionCharacter, CharacterWorld};

/// Max level 3; levels 1-3 at 0, 100, 1500 experience (every class).
fn vitals() -> VitalsTables {
    let row = |v: u32| {
        let mut e = Experience::decode(&[0u8; Experience::SIZE]);
        e.amazon = v;
        e.sorceress = v;
        e.necromancer = v;
        e.paladin = v;
        e.barbarian = v;
        e.druid = v;
        e.assassin = v;
        e
    };
    VitalsTables {
        charstats: Vec::new(),
        experience: vec![row(3), row(0), row(100), row(1500)],
    }
}

/// A mod game whose classes reach level 120, at `level * 1000`.
#[derive(Debug)]
struct LongExperience;

impl FormulaHooks for LongExperience {
    fn max_level(&self, _: i32) -> Option<u32> {
        Some(120)
    }
    fn exp_threshold(&self, _: i32, level: u32) -> Option<u32> {
        Some(level * 1000)
    }
}

impl ModRules for LongExperience {
    fn name(&self) -> &str {
        "long experience"
    }
    fn formulas(&self) -> Option<&dyn FormulaHooks> {
        Some(self)
    }
}

#[test]
fn a_loaded_level_past_the_table_takes_the_mod_games_threshold() {
    let mut fx = Fx::new(|_| {});
    let p = fx.player;
    let s = &mut fx.h.game;
    s.events.hooks().vitals = Some(Arc::new(vitals()));
    let next = |s: &mut Sim, level: i32| {
        s.events.with(&mut s.game, |_, v| {
            ActionCharacter { v, player: p }.next_exp(level)
        })
    };
    // D2Gamma's own: the table's row; past the table, unapplied (no panic).
    assert_eq!(next(s, 2), Ok(1500));
    let past = next(s, 99).unwrap_err();
    assert_eq!(past.step, "nextexp");
    // A mod game: its threshold, inside the table and past it.
    s.events.hooks().rules = Ruleset::Mod(Arc::new(LongExperience));
    assert_eq!(next(s, 2), Ok(2000));
    assert_eq!(next(s, 99), Ok(99_000));
    assert_eq!(next(s, 119), Ok(119_000));
}
