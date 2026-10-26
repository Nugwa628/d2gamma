//! [`SkillPets`] on the skill view: the summon, pet, revive and path
//! seams a mod game's skill bodies get, on the steps D2RUST's own summon
//! and revive bodies run ([`crate::skills::use_::bodies`]) and the pet
//! lists of [`crate::wiring::action::ActionHooks::pet_lists`].

use super::skill_use::UseView;
use super::UseRest;
use crate::rules::{SkillPets, Summon};
use crate::skills::use_::bodies::{self, b3_lvl30, BodyEffect, BodyWorld, PathOp};
use crate::units::UnitId;
use crate::wiring::action::Pending;

impl<X: Pending + UseRest> SkillPets for UseView<'_, X> {
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
}
