//! Summons, pets, revives and paths for a mod game's skill bodies: part of
//! [`super::SkillHost`].
//!
//! A mod's summon body spawns a monster for its owner, sets it up (skill,
//! level and stats from the owner's skill, the source link), records it
//! in the owner's pet lists and makes it follow; a revive body tests a
//! corpse, stands it up and binds it to its new owner. These are the
//! steps D2Gamma's own summon and revive bodies run, one seam each, so a
//! mod can run them in its own order. Every method has a default that
//! does nothing.

use crate::units::UnitId;

pub use crate::skills::use_::bodies::Summon;

/// Explicit placement and setup values for a class sentry.
#[derive(Clone, Copy, Debug)]
pub struct SentrySetup {
    pub class: u16,
    pub pet_type: u16,
    pub mode: u8,
    pub cap: i32,
    pub skill: i32,
    pub level: i32,
    pub in_town: bool,
    pub at: (i32, i32),
}

/// Summons, pets, revives and paths (a supertrait of
/// [`super::SkillHost`]).
#[allow(unused_variables)]
pub trait SkillPets {
    /// Schedule a pet event after a delay in base ticks at the game's tick rate.
    fn pet_timer(&mut self, pet: UnitId, kind: u8, delay: i32) {}
    /// Apply explicit shadow life/stat values; the level-one setup is skipped.
    fn shadow_stats(
        &mut self,
        pet: UnitId,
        level: i32,
        life_percent: i32,
        stats: &[super::StateStat],
        umod: Option<u8>,
    ) {
    }
    /// Generate the pet's equipment using the normal monster equipment rows.
    fn equip_pet(&mut self, owner: UnitId, pet: UnitId, skill: i32, level: i32, item_level: i32) {}
    /// Apply one monster modifier through the shared modifier path.
    fn pet_umod(&mut self, pet: UnitId, umod: u8, arg: i32) {}
    /// Add the owner's summon resistance, respecting the pet's elemental absorbs.
    fn summon_resistance(&mut self, owner: UnitId, pet: UnitId) {
        let _ = (owner, pet);
    }
    /// Announce an ally without changing its path target.
    fn announce_pet(&mut self, owner: UnitId, pet: UnitId) {
        let _ = (owner, pet);
    }
    /// Place a sentry, apply pet stats/alignment and request its initial mode.
    fn place_sentry(&mut self, caster: UnitId, setup: &SentrySetup) -> Option<UnitId> {
        None
    }
    /// Join the pet to its owner's player target list.
    fn join_owner_targets(&mut self, owner: UnitId, pet: UnitId) {}
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
    /// Sets the collision masks of the unit's path: `move_mask`, what
    /// blocks its steps, and `foot_mask`, what its footprint marks on the
    /// rooms. `None` keeps a mask as it is. A leap clears both before it
    /// starts, so the unit flies over what would stop a walk. False when
    /// the unit has no path.
    fn set_path_masks(
        &mut self,
        u: UnitId,
        move_mask: Option<u16>,
        foot_mask: Option<u16>,
    ) -> bool {
        false
    }
    /// The landing step of a leap, on each do frame of its flight: the
    /// engine's own Leap landing on the unit's entry of `skill`. A unit on
    /// the entry's point (params 1 and 2) lands: its path takes back its
    /// walk masks and type, the landing message goes out, and a player's
    /// animation ends on the next frame. A player still in flight holds
    /// its animation at frame 10. True on the frame the unit lands; false
    /// in flight, or without the entry or a path.
    fn leap_land(&mut self, u: UnitId, skill: i32) -> bool {
        false
    }
    /// The next target of a leap attack, on the unit's entry of `skill`:
    /// its target when within melee range, else the target the entry
    /// keeps (params 3 and 4), else the next unit around it the leap
    /// attack may hit. The target found is kept on the entry; false
    /// (the entry keeps none) when there is none, or without the entry.
    fn leap_next_target(&mut self, u: UnitId, skill: i32) -> bool {
        false
    }
    /// The engine's own Leap Attack strike of `skill` at `level`: a melee
    /// hit on the entry's next target ([`SkillPets::leap_next_target`])
    /// with the skill's damage bonus, element and overlay, which also
    /// ends a stun on it. True when there was a target (hit or miss).
    fn leap_strike(&mut self, u: UnitId, skill: i32, level: i32) -> bool {
        false
    }
    /// Plays the unit's current animation again from `frame` (its timers
    /// of the mode are posted anew), as a leap attack does after each
    /// strike.
    fn anim_from(&mut self, u: UnitId, frame: i32) {}
}
