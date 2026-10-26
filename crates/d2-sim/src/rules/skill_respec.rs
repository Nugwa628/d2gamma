//! What a mod game's skill reset reads and changes: part of
//! [`super::SkillHost`].
//!
//! A mod that answers [`super::SkillProvider::respec`] resets a player's
//! skills itself: it reads the hard points of each skill, removes the
//! skills, ends the states its skills made and refunds the points, then
//! sends the player the new skill list. Every method has a default that
//! does nothing.

use crate::units::UnitId;

/// The skill reset's world (a supertrait of [`super::SkillHost`]).
#[allow(unused_variables)]
pub trait SkillRespec {
    /// The unit's hard points in `skill` (the base level of its own
    /// entry, no item or bonus levels); 0 without one.
    fn hard_points(&self, u: UnitId, skill: i32) -> i32 {
        0
    }
    /// Removes the unit's own entry of `skill` from its skill list.
    fn remove_skill(&mut self, u: UnitId, skill: i32) {}
    /// Sends the player its skills and stats again after they changed
    /// (the skill resync, the unit queued for update).
    fn skills_changed(&mut self, u: UnitId) {}
    /// The unit's state lists that a skill made, as `(state, skill)`, in
    /// list order.
    fn skill_states(&self, u: UnitId) -> Vec<(u16, i32)> {
        Vec::new()
    }
}
