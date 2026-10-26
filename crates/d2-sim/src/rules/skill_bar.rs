//! Charge-up skills for a mod game's skill bodies: part of
//! [`super::SkillHost`].
//!
//! A charge-up skill builds a charge over several uses and spends it when
//! released. The game keeps one [`SkillCharge`] per unit and skill for
//! the mod; the mod's skill bodies decide what readying, advancing,
//! holding and releasing a charge mean. Charges are not saved and go with
//! their unit. Every method has a default that keeps nothing.

use crate::units::UnitId;

/// A unit's charge in one skill.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkillCharge {
    /// The charge built so far.
    pub charge: i32,
    /// The charge at which the skill is ready.
    pub max: i32,
    /// The charge is being held.
    pub held: bool,
}

/// Charge-up skills (a supertrait of [`super::SkillHost`]).
#[allow(unused_variables)]
pub trait SkillBar {
    /// The unit's charge in `skill`; `None` without one.
    fn skill_charge(&self, u: UnitId, skill: i32) -> Option<SkillCharge> {
        None
    }
    /// Sets the unit's charge in `skill`; `None` drops it.
    fn set_skill_charge(&mut self, u: UnitId, skill: i32, charge: Option<SkillCharge>) {}
}
