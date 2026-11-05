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
mod triggers;
mod units;

impl<X: Pending + UseRest> SkillHost for UseView<'_, X> {
    fn call_script(
        &mut self,
        spec: &crate::rules::scripts::CallSpec,
        context: crate::rules::scripts::CallContext,
    ) -> Result<i32, String> {
        let result = match self.cv.v.h.scripts.clone() {
            Some(runtime) => runtime.call(self, spec, context),
            None => Err("script runtime is not installed".into()),
        };
        if let Err(e) = &result {
            self.cv.v.h.x.script_error(e);
        }
        result
    }
    fn script_error(&mut self, message: &str) {
        self.cv.v.h.x.script_error(message);
    }
    fn script_message(&mut self, unit: UnitId, message: &str) {
        self.cv.v.h.x.script_message(unit, message);
    }
    fn script_ui_variable(&mut self, unit: UnitId, name: &str, value: i32) {
        self.cv.v.h.x.script_ui_variable(unit, name, value);
    }
    fn script_id_valid(&self, kind: &str, id: u16) -> bool {
        let t = &self.cv.v.h.tables;
        match kind {
            "stat" => self.cv.v.stats.data().stats.get(id).is_some(),
            "state" => usize::from(id) < self.cv.v.stats.data().states.count(),
            "skill" => usize::from(id) < t.skills.skills.len(),
            "missile" => usize::from(id) < t.missiles.len(),
            "monster" => usize::from(id) < t.combat.monstats.len(),
            _ => false,
        }
    }
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
