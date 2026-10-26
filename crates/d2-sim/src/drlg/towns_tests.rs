//! The town levels of a mod game ([`crate::rules::ModRules::town_levels`])
//! on the rules and the DRLG, against D2RUST's act towns.

use std::sync::Arc;

use super::{Drlg, DrlgData, Dungeon, NoLevelTypes, TownLevels, TOWN_LEVELS};
use crate::rules::{ModRules, Ruleset};

/// A mod game whose towns are its set.
#[derive(Debug)]
pub(crate) struct Towns(pub(crate) TownLevels);

impl ModRules for Towns {
    fn name(&self) -> &str {
        "town-levels"
    }
    fn town_levels(&self) -> Option<&TownLevels> {
        Some(&self.0)
    }
}

/// The rules of a mod game whose towns are `levels`.
pub(crate) fn rules(levels: impl IntoIterator<Item = u32>) -> Ruleset {
    Ruleset::Mod(Arc::new(Towns(TownLevels::new(levels))))
}

/// An act's DRLG with no levels, created in a game with `towns`.
fn drlg(towns: &TownLevels) -> Drlg {
    let data = DrlgData::default();
    Drlg::create_with_towns(0, 1, 0, 0, false, towns, &data, &mut NoLevelTypes).unwrap()
}

#[test]
fn the_rules_hand_out_the_mod_s_towns() {
    let r = rules([300, 1]);
    assert_eq!(r.town_levels(), TownLevels::new([1, 300]));
    assert!(r.is_town(300) && r.is_town(1) && !r.is_town(40));
}

/// D2RUST's rules (and a mod without towns) keep the five act towns.
#[test]
fn original_rules_keep_the_act_towns() {
    #[derive(Debug)]
    struct NoTowns;
    impl ModRules for NoTowns {
        fn name(&self) -> &str {
            "no-towns"
        }
    }
    for r in [Ruleset::Original, Ruleset::Mod(Arc::new(NoTowns))] {
        assert_eq!(r.town_levels(), TownLevels::default());
        for l in 0..200 {
            assert_eq!(r.is_town(l), TOWN_LEVELS.contains(&l), "level {l}");
        }
    }
}

/// A DRLG keeps its game's towns: the rooms of a town level stay while in
/// use.
#[test]
fn the_drlg_keeps_its_game_s_towns() {
    let towns = TownLevels::new([300]);
    let d = drlg(&towns);
    assert_eq!(d.towns, towns);
    assert!(d.no_removal_level(300) && d.no_removal_level(120));
    assert!(!d.no_removal_level(1));
    let data = DrlgData::default();
    let o = Drlg::create(0, 1, 0, 0, false, &data, &mut NoLevelTypes).unwrap();
    assert_eq!(o.towns, TownLevels::default());
    assert!(o.no_removal_level(1) && !o.no_removal_level(300));
}

/// The dungeon's towns reach the acts that exist and the acts it creates.
#[test]
fn the_dungeon_s_towns_reach_its_acts() {
    let mut g = Dungeon::default();
    assert_eq!(g.towns, TownLevels::default());
    g.acts[0] = Some(drlg(&TownLevels::default()));
    let towns = TownLevels::new([300]);
    g.set_towns(towns.clone());
    assert_eq!(g.acts[0].as_ref().unwrap().towns, towns);
    let data = DrlgData::default();
    // Arena level 0: no town generated up front.
    let d = g.get_or_create(1, 2, 0, Some(0), &data, &mut NoLevelTypes);
    assert_eq!(d.unwrap().towns, towns);
}
