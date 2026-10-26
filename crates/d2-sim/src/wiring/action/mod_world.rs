//! The action wiring's side of a mod game's world rules
//! ([`crate::rules::WorldRules`]): the level-entry check of warps, waypoints
//! and portals, and a town portal's destination, asked with the
//! [`super::combat::CombatView`] as their [`crate::rules::EventHost`]. Without world rules
//! (every engine game) nothing is asked: one check of the ruleset.

use crate::game::Game;
use crate::rules::world::{self, Entry, LevelEntry};
use crate::units::UnitId;

use super::{Pending, View};

impl<X: Pending> View<'_, X> {
    /// Whether `player` may enter level `to` from `from` by `via`
    /// ([`world::may_enter`]); always without world rules.
    pub fn may_enter(
        &mut self,
        game: &mut Game,
        player: UnitId,
        from: Option<u32>,
        to: u32,
        via: Entry,
    ) -> bool {
        if self.h.rules.world().is_none() {
            return true;
        }
        let rules = self.h.rules.clone();
        let entry = LevelEntry {
            player,
            from,
            to,
            via,
            difficulty: self.data.difficulty,
        };
        let mut cv = self.combat(game);
        world::may_enter(&rules, &mut cv, &entry)
    }

    /// The level a town portal of `player` from `from` opens to
    /// ([`world::town_portal_level`]); `town` without world rules.
    pub fn town_portal_level(
        &mut self,
        game: &mut Game,
        player: UnitId,
        from: u32,
        town: u32,
    ) -> u32 {
        if self.h.rules.world().is_none() {
            return town;
        }
        let rules = self.h.rules.clone();
        let mut cv = self.combat(game);
        world::town_portal_level(&rules, &mut cv, player, from, town)
    }
}
