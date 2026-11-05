//! [`HostSkills`] on the skill use view.

use crate::combat::CombatWorld;
use crate::rules::HostSkills;
use crate::skills::levels::{self, highest_entry};
use crate::skills::use_::bodies::effects::PathOp;
use crate::skills::use_::bodies::{self, BodyWorld};
use crate::skills::use_::{do_core, UseWorld};
use crate::skills::{SkillEntry, SkillUnits};
use crate::units::UnitId;
use crate::wiring::action::Pending;
use crate::wiring::interaction::{UseRest, UseView};

impl<X: Pending + UseRest> HostSkills for UseView<'_, X> {
    fn skill_list(&self, u: UnitId) -> Vec<SkillEntry> {
        SkillUnits::skill_list(self, u)
    }
    fn owns_skill(&self, u: UnitId, skill: i32) -> bool {
        UseWorld::owns_skill(self, u, skill)
    }
    fn skill_entry(&self, u: UnitId, skill: i32) -> Option<SkillEntry> {
        UseWorld::find_entry(self, u, skill)
    }
    /// The level of the skill's highest entry, as the formulas read it.
    fn skill_level(&self, u: UnitId, skill: i32) -> i32 {
        let e = highest_entry(&SkillUnits::skill_list(self, u), skill);
        levels::skill_level(self, &self.cv.v.h.tables.skills, Some(u), e.as_ref(), true)
    }
    /// The do core of an item's skill (no mana, no ownership), aimed at the
    /// target unit or at the caster; the caster's own target and unit flag
    /// 0x40 are restored after. Unlike an item's skill-on-event this has no
    /// `itemeffect` gate: any skills row can be cast, and the client is not
    /// sent a cast message (the effects reach it as the units they make).
    fn cast_skill(&mut self, u: UnitId, skill: i32, lvl: i32, target: Option<UnitId>) -> bool {
        let t = self.cv.v.h.tables.clone();
        let saved_unit = UseWorld::target(self, u);
        let saved_point = BodyWorld::path_target_point(self, u);
        let saved_40 = UseWorld::unit_flags(self, u) & 0x40;
        BodyWorld::path_op(self, u, PathOp::TargetUnit(Some(target.unwrap_or(u))));
        let res = do_core(self, &t.skills, u, skill, lvl, false, true, false);
        let op = match saved_unit {
            Some(t) => PathOp::TargetUnit(Some(t)),
            None => PathOp::TargetPoint(saved_point.0, saved_point.1),
        };
        BodyWorld::path_op(self, u, op);
        let f = UseWorld::unit_flags(self, u);
        UseWorld::set_unit_flags(self, u, (f & !0x40) | saved_40);
        res != 0
    }
    fn used_skill(&self, u: UnitId) -> Option<SkillEntry> {
        SkillUnits::used_skill(self, u)
    }
    fn entry_flags(&self, u: UnitId, skill: i32) -> u32 {
        UseWorld::find_entry(self, u, skill).map_or(0, |e| BodyWorld::entry_flags(self, u, &e))
    }
    fn set_entry_flags(&mut self, u: UnitId, skill: i32, flags: u32) {
        if let Some(e) = UseWorld::find_entry(self, u, skill) {
            BodyWorld::set_entry_flags(self, u, &e, flags);
        }
    }
    fn used_entry_param(&self, u: UnitId, i: u8) -> i32 {
        SkillUnits::used_skill(self, u).map_or(0, |e| BodyWorld::entry_param(self, u, &e, i))
    }
    fn set_used_entry_param(&mut self, u: UnitId, i: u8, v: i32) {
        if let Some(e) = SkillUnits::used_skill(self, u) {
            BodyWorld::set_entry_param_of(self, u, &e, i, v);
        }
    }
    fn refresh_passive(&mut self, u: UnitId, skill: i32) {
        let t = self.cv.v.h.tables.clone();
        bodies::passive::refresh(self, &t.skills, u, skill);
    }
    fn eval_skill_calc(&mut self, u: UnitId, calc: u32, skill: i32, lvl: i32) -> i32 {
        let t = self.cv.v.h.tables.clone();
        levels::eval_skill(self, &t.skills, Some(u), calc, skill, lvl)
    }
    fn eval_formula(&mut self, u: UnitId, code: &[u8], skill: i32, lvl: i32) -> i32 {
        let t = self.cv.v.h.tables.clone();
        levels::eval_desc(self, &t.skills, code, Some(u), 0, skill, lvl)
    }
    fn prog_count(&mut self, u: UnitId, skill: i32, lvl: i32) -> i32 {
        let t = self.cv.v.h.tables.clone();
        bodies::helpers3::prog_count(self, &t.skills, u, skill, lvl)
    }
    fn skill_to_hit(&mut self, u: UnitId, skill: i32, lvl: i32) -> i32 {
        let t = self.cv.v.h.tables.clone();
        levels::to_hit(self, &t.skills, Some(u), skill, lvl)
    }
    fn skill_phys_damage(&mut self, u: UnitId, skill: i32, lvl: i32, src_dam: bool) -> (i32, i32) {
        let t = self.cv.v.h.tables.clone();
        (
            levels::phys_min(self, &t.skills, Some(u), skill, lvl, src_dam),
            levels::phys_max(self, &t.skills, Some(u), skill, lvl, src_dam),
        )
    }
    fn skill_elem_damage(&mut self, u: UnitId, skill: i32, lvl: i32, mastery: bool) -> (i32, i32) {
        let t = self.cv.v.h.tables.clone();
        (
            levels::elem_min(self, &t.skills, Some(u), skill, lvl, mastery),
            levels::elem_max(self, &t.skills, Some(u), skill, lvl, mastery),
        )
    }
    fn skill_elem_len(&mut self, u: UnitId, skill: i32, lvl: i32) -> i32 {
        let t = self.cv.v.h.tables.clone();
        levels::elem_len(self, &t.skills, Some(u), skill, lvl)
    }
    fn melee_range(&self, u: UnitId) -> i32 {
        CombatWorld::melee_range(&self.cv, u)
    }
    fn minion_spawn_class(&self, u: UnitId) -> Option<i32> {
        BodyWorld::minion_spawn_class(self, u)
    }
}
