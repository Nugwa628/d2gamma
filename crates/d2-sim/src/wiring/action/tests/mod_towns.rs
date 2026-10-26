//! A mod game's town levels on the action wiring: the DRLG's town test
//! ([`DrlgWorld::in_town`]) and the town portal cast follow the game's
//! towns ([`crate::drlg::Dungeon::set_towns`]).

use super::travel::{cast, field_and_town, player_in};
use super::*;
use crate::drlg::TownLevels;

/// The field level is a town of the mod game: its rooms are in town and a
/// cast there is refused.
#[test]
fn a_cast_in_a_mod_town_is_refused() {
    let mut fx = field_and_town();
    let a = fx.a;
    assert!(!fx.sim.sys.hooks.drlg.in_town(&fx.game, a));
    fx.sim
        .hooks()
        .drlg
        .dungeon
        .set_towns(TownLevels::new([LEVEL]));
    assert!(fx.sim.sys.hooks.drlg.in_town(&fx.game, a));
    let p = player_in(&mut fx, a, 20, 20);
    assert_eq!(cast(&mut fx, p), (0, false));
    fx.assert_clean();
}
