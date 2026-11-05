//! [`SkillPets`] on the skill view: the summon, pet, revive and path
//! seams a mod game's skill bodies get, on the steps D2Gamma's own summon
//! and revive bodies run ([`crate::skills::use_::bodies`]) and the pet
//! lists of [`crate::wiring::action::ActionHooks::pet_lists`].

use super::skill_use::UseView;
use super::UseRest;
use crate::rules::{SkillPets, Summon};
use crate::skills::use_::bodies::{self, b3_lvl30, BodyEffect, BodyWorld, PathOp};
use crate::skills::use_::UseWorld;
use crate::units::UnitId;
use crate::wiring::action::Pending;

impl<X: Pending + UseRest> SkillPets for UseView<'_, X> {
    fn pet_timer(&mut self, pet: UnitId, kind: u8, delay: i32) {
        let due = bodies::after(self, delay);
        self.schedule(pet, kind, due, 0, 0);
    }
    fn shadow_stats(
        &mut self,
        pet: UnitId,
        level: i32,
        percent: i32,
        stats: &[crate::rules::StateStat],
        umod: Option<u8>,
    ) {
        super::skill_host_shadow::stats(self, pet, level, percent, stats, umod);
    }
    fn equip_pet(&mut self, owner: UnitId, pet: UnitId, skill: i32, level: i32, item_level: i32) {
        let item_level = item_level.max(1).min(self.cv.v.h.tables.skills.level_cap);
        self.effect(BodyEffect::Equipment {
            owner,
            m: pet,
            skill,
            lvl: level,
            ilvl: item_level,
        });
    }
    fn pet_umod(&mut self, pet: UnitId, umod: u8, arg: i32) {
        self.effect(BodyEffect::Umod {
            m: pet,
            umod: i32::from(umod),
            arg,
        });
    }
    fn summon_resistance(&mut self, owner: UnitId, pet: UnitId) {
        bodies::summon_resist(self, owner, pet);
    }
    fn announce_pet(&mut self, owner: UnitId, pet: UnitId) {
        self.effect(BodyEffect::AllyInfo { u: owner, m: pet });
    }
    fn place_sentry(
        &mut self,
        caster: UnitId,
        setup: &crate::rules::SentrySetup,
    ) -> Option<UnitId> {
        super::skill_host_sentry::place(self, caster, setup)
    }
    fn join_owner_targets(&mut self, owner: UnitId, pet: UnitId) {
        bodies::node_insert_owner(self, pet, owner);
    }
    fn summon(&mut self, req: Summon<UnitId>) -> Option<UnitId> {
        bodies::spawn(self, req)
    }

    fn set_pet_skill(&mut self, pet: UnitId, skill: i32, level: i32) {
        self.effect(BodyEffect::SetSkill {
            m: pet,
            skill,
            lvl: level,
        });
    }

    fn pet_base_stats(
        &mut self,
        owner: UnitId,
        pet: UnitId,
        monster_level: i32,
        skill_level: i32,
    ) -> i32 {
        bodies::base_stats(self, owner, pet, monster_level, skill_level)
    }

    fn pet_skill_stats(&mut self, owner: UnitId, pet: UnitId, skill: i32, level: i32) {
        let t = self.cv.v.h.tables.clone();
        bodies::skill_stats(self, &t.skills, owner, pet, skill, level, 0);
    }

    fn link_source(&mut self, pet: UnitId, owner: Option<UnitId>) {
        bodies::link_source(self, pet, owner);
    }

    fn add_pet(&mut self, owner: UnitId, pet: UnitId, pet_type: i32, pet_max: i32) {
        self.effect(BodyEffect::PetAdd {
            owner,
            pet,
            t: pet_type,
            max: pet_max,
        });
    }

    fn pet_count(&self, owner: UnitId, pet_type: i32) -> i32 {
        let Ok(t) = usize::try_from(pet_type) else {
            return 0;
        };
        self.cv
            .v
            .h
            .pet_lists
            .get(&owner)
            .and_then(|l| l.entries.get(t))
            .map_or(0, |e| e.count)
    }

    fn refresh_pets(&mut self, owner: UnitId) {
        self.resync_pet_maxima(owner);
    }

    fn follow_owner(&mut self, owner: UnitId, pet: UnitId) {
        self.path_op(pet, PathOp::TargetUnit(Some(owner)));
        self.effect(BodyEffect::AllyInfo { u: owner, m: pet });
    }

    fn revivable(&mut self, _caster: UnitId, corpse: UnitId) -> bool {
        let t = self.cv.v.h.tables.clone();
        b3_lvl30::revive_test(self, &t.combat, corpse)
    }

    fn revive_prepare(&mut self, caster: UnitId, corpse: UnitId) {
        let t = self.cv.v.h.tables.clone();
        b3_lvl30::revive_stand(self, &t.combat, caster, corpse);
    }

    fn revive_finish(&mut self, owner: UnitId, pet: UnitId) {
        b3_lvl30::revive_bind(self, owner, pet);
        bodies::node_insert_owner(self, pet, owner);
        self.path_op(pet, PathOp::Reset);
    }

    fn reset_path(&mut self, u: UnitId) {
        self.path_op(u, PathOp::Reset);
    }

    fn start_dash(&mut self, u: UnitId, to: (i32, i32), path_type: u8, velocity: i32) -> bool {
        if !self.has_path(u) {
            return false;
        }
        self.path_op(u, PathOp::TargetPoint(to.0, to.1));
        self.path_op(u, PathOp::Type(path_type));
        self.path_op(u, PathOp::Velocity(velocity));
        self.path_op(u, PathOp::Compute);
        true
    }

    fn set_path_masks(
        &mut self,
        u: UnitId,
        move_mask: Option<u16>,
        foot_mask: Option<u16>,
    ) -> bool {
        if !self.has_path(u) {
            return false;
        }
        if let Some(m) = move_mask {
            self.path_op(u, PathOp::MoveMask(u32::from(m)));
        }
        if let Some(m) = foot_mask {
            self.path_op(u, PathOp::FootprintMask(u32::from(m)));
        }
        true
    }

    fn leap_land(&mut self, u: UnitId, skill: i32) -> bool {
        let Some(e) = UseWorld::find_entry(self, u, skill) else {
            return false;
        };
        let t = self.cv.v.h.tables.clone();
        bodies::leap_land(self, &t.combat, u, &e) != 0
    }

    fn leap_next_target(&mut self, u: UnitId, skill: i32) -> bool {
        let Some(e) = UseWorld::find_entry(self, u, skill) else {
            return false;
        };
        let t = self.cv.v.h.tables.clone();
        bodies::leap_pick(self, &t.skills, &t.combat, u, Some(&e)).is_some()
    }

    fn leap_strike(&mut self, u: UnitId, skill: i32, level: i32) -> bool {
        let t = self.cv.v.h.tables.clone();
        bodies::leap_strike(self, &t.skills, &t.combat, u, skill, level) != 0
    }

    fn anim_from(&mut self, u: UnitId, frame: i32) {
        BodyWorld::anim_from(self, u, frame);
    }
}
