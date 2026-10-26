//! Unit search and AI for a mod game's skill bodies: part of
//! [`super::SkillHost`].
//!
//! With these a mod's skill body finds the units around a point, tests
//! them against a skill's unit filter and a line of sight, asks whether a
//! curse or a skill's state may land on a unit, and tells a monster's AI
//! it was cursed, the same way D2RUST's own skill bodies do. Every method
//! has a default that finds and allows nothing, so a host without the
//! seam keeps compiling.

use crate::units::UnitId;

/// Unit search and AI (a supertrait of [`super::SkillHost`]).
#[allow(unused_variables)]
pub trait SkillSearch {
    /// The units within `radius` of `at` (squared distance from their
    /// position at most `radius²`) in the rooms around the room holding
    /// `at`, that room looked up from `near`'s room: the rooms a skill's
    /// area scan visits, in their order, `near` included when in range.
    /// Empty when `near` has no room or no room holds `at`.
    fn units_near(&self, near: UnitId, at: (i32, i32), radius: i32) -> Vec<UnitId> {
        Vec::new()
    }
    /// `target` passes the unit filter `filter` (the bits of a skill's
    /// `aurafilter`) for `source`: unit kinds, modes, town, ally and enemy
    /// tests; a missile source tests for its owner.
    fn passes_filter(&mut self, source: UnitId, target: UnitId, filter: u32) -> bool {
        false
    }
    /// The line from `a` to `b`'s position is blocked for the collision
    /// `mask`.
    fn los_blocked(&self, a: UnitId, b: UnitId, mask: u32) -> bool {
        true
    }
    /// A curse may land on the unit: it is not neutral, alive, has all of
    /// unit flags 0xE (targetable, attackable, a valid target), and a
    /// monster has its walk mode and no monster type flag 0x20 (the test
    /// the curse bodies run on each unit, without the hostility test).
    fn curse_allowed(&mut self, u: UnitId) -> bool {
        false
    }
    /// The unit can take a skill's state at all: not a monster, or a
    /// monster whose class is no NPC and can be attacked (the gate of
    /// the state apply the skill bodies use).
    fn accepts_aura(&self, u: UnitId) -> bool {
        false
    }
    /// Sets the monster's AI state `state` through its AI control, as the
    /// fear and confusion curses do (the state goes back when the curse
    /// state's list is removed).
    fn notify_ai(&mut self, u: UnitId, state: i32) {}
}
