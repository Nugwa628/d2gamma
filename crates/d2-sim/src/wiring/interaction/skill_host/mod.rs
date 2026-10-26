//! [`crate::rules::SkillHost`] on the skill use view: the world a mod
//! game's skill bodies are handed (MS1). The base reads here; each part
//! of [`crate::rules::skill_host`] in its own file, over the same real
//! units, stat lists, rooms, tables and combat code D2RUST's own skill
//! bodies use.

use crate::rng::Seed;
use crate::rules::SkillHost;
use crate::skills::use_::UseWorld;
use crate::skills::{ManaUnits, SkillUnits};
use crate::units::UnitId;
use crate::wiring::action::Pending;
use crate::wiring::interaction::{UseRest, UseView};

mod damage;
mod missiles;
mod skills;
pub(super) mod states;
mod timers;
mod units;

impl<X: Pending + UseRest> SkillHost for UseView<'_, X> {
    fn frame(&self) -> i32 {
        UseWorld::frame(self)
    }
    fn class_id(&self, u: UnitId) -> i32 {
        SkillUnits::class_id(self, u)
    }
    fn stat(&self, u: UnitId, stat: u16, layer: u16) -> i32 {
        SkillUnits::stat(self, u, stat, layer)
    }
    fn base_stat(&self, u: UnitId, stat: u16, layer: u16) -> i32 {
        SkillUnits::base_stat(self, u, stat, layer)
    }
    fn has_state(&self, u: UnitId, state: u16) -> bool {
        SkillUnits::has_state(self, u, state)
    }
    fn set_stat(&mut self, u: UnitId, stat: u16, value: i32) {
        ManaUnits::set_stat(self, u, stat, value);
    }
    fn seed(&mut self, u: UnitId) -> &mut Seed {
        SkillUnits::seed(self, u)
    }
}
