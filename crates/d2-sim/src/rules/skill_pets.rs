//! Summons, pets, revives and paths for a mod game's skill bodies: part of
//! [`super::SkillHost`].
//!
//! A mod's summon body spawns a monster for its owner, sets it up (skill,
//! level and stats from the owner's skill, the source link), records it
//! in the owner's pet lists and makes it follow; a revive body tests a
//! corpse, stands it up and binds it to its new owner. These are the
//! steps D2RUST's own summon and revive bodies run, one seam each, so a
//! mod can run them in its own order. Every method has a default that
//! does nothing.

use crate::units::UnitId;

pub use crate::skills::use_::bodies::Summon;

/// Summons, pets, revives and paths (a supertrait of
/// [`super::SkillHost`]).
#[allow(unused_variables)]
pub trait SkillPets {
    /// Spawns a summon: a monster of `req.class` in `req.mode` at the
    /// owner's target point (or at `req.x`, `req.y` with flag 1), added to
    /// the owner's pet list of `req.pet_type` (at most `req.pet_max`, at
    /// least 1), linked to its owner, with AI state `req.ai`; flag 2
    /// replaces the owner's linked unit, 4 refuses a second placement
    /// try, 8 keeps the revived flag off. `None` when nothing could be
    /// placed.
    fn summon(&mut self, req: Summon<UnitId>) -> Option<UnitId> {
        None
    }
    /// Gives the pet skill `skill` at base level `level`.
    fn set_pet_skill(&mut self, pet: UnitId, skill: i32, level: i32) {}
    /// Sets the pet's level: `monster_level` when positive, else the
    /// owner's level × 3 / 4 + `skill_level` (at least 1, at most the
    /// owner's level); then its base stats for that level. Returns the
    /// level.
    fn pet_base_stats(
        &mut self,
        owner: UnitId,
        pet: UnitId,
        monster_level: i32,
        skill_level: i32,
    ) -> i32 {
        0
    }
    /// Adds the summon skill's passive and aura stats to the pet, the
    /// formulas computed on the owner.
    fn pet_skill_stats(&mut self, owner: UnitId, pet: UnitId, skill: i32, level: i32) {}
    /// Links the unit to its source unit (its owner), or drops the link
    /// with `None`.
    fn link_source(&mut self, pet: UnitId, owner: Option<UnitId>) {}
    /// Adds the pet to the owner's pet list of `pet_type`; beyond `pet_max`
    /// the oldest pet of that list goes.
    fn add_pet(&mut self, owner: UnitId, pet: UnitId, pet_type: i32, pet_max: i32) {}
    /// The number of pets in the owner's pet list of `pet_type`; 0 without
    /// lists.
    fn pet_count(&self, owner: UnitId, pet_type: i32) -> i32 {
        0
    }
    /// Re-derives the owner's pet list maxima from its skills' `petmax`
    /// (the pet part of a skill resync).
    fn refresh_pets(&mut self, owner: UnitId) {}
    /// Makes the pet follow its owner (the pet's path target) and tells the
    /// owner's client about its new ally.
    fn follow_owner(&mut self, owner: UnitId, pet: UnitId) {}
    /// The corpse can be revived: a dead monster whose class can be
    /// revived and whose corpse can still be raised.
    fn revivable(&mut self, caster: UnitId, corpse: UnitId) -> bool {
        false
    }
    /// The revive's first step: the caster's raise penalty, then the
    /// corpse stands up at a free point near it.
    fn revive_prepare(&mut self, caster: UnitId, corpse: UnitId) {}
    /// The revive's last step: the raised unit becomes `owner`'s (owner
    /// data, leash, the owner's target list), thinks soon, turns good,
    /// gets the revived flag and state, and its path is reset.
    fn revive_finish(&mut self, owner: UnitId, pet: UnitId) {}
    /// Resets the unit's path.
    fn reset_path(&mut self, u: UnitId) {}
    /// Starts a dash along the unit's path to `to`: path type `path_type`,
    /// `velocity` (1/256 sub-tiles per frame), computed. False when the
    /// unit has no path.
    fn start_dash(&mut self, u: UnitId, to: (i32, i32), path_type: u8, velocity: i32) -> bool {
        false
    }
}
