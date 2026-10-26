//! [`SkillBar`] on the skill view: the charges of a mod game's charge-up
//! skills, kept in [`crate::wiring::action::ActionHooks::skill_charges`].

use super::skill_use::UseView;
use super::UseRest;
use crate::rules::{SkillBar, SkillCharge};
use crate::units::UnitId;
use crate::wiring::action::Pending;

impl<X: Pending + UseRest> SkillBar for UseView<'_, X> {
    fn skill_charge(&self, u: UnitId, skill: i32) -> Option<SkillCharge> {
        self.cv.v.h.skill_charges.get(&(u, skill)).copied()
    }

    fn set_skill_charge(&mut self, u: UnitId, skill: i32, charge: Option<SkillCharge>) {
        let charges = &mut self.cv.v.h.skill_charges;
        match charge {
            Some(c) => {
                charges.insert((u, skill), c);
            }
            None => {
                charges.remove(&(u, skill));
            }
        }
    }
}
