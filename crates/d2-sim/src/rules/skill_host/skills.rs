//! A unit's skills and the skill formulas, for a mod game's skill bodies.

use crate::skills::SkillEntry;
use crate::units::UnitId;

/// What a skill body reads and changes of a unit's skill list, and the
/// skill formulas D2RUST's bodies evaluate (on the game's `skills` rows
/// and `skillscode`, which are the mod game's own under its rules).
pub trait HostSkills {
    /// The unit's skill list, in list order.
    fn skill_list(&self, u: UnitId) -> Vec<SkillEntry> {
        let _ = u;
        Vec::new()
    }
    /// The unit has `skill` in its skill list.
    fn owns_skill(&self, u: UnitId, skill: i32) -> bool {
        let _ = (u, skill);
        false
    }
    /// The unit's entry of `skill`.
    fn skill_entry(&self, u: UnitId, skill: i32) -> Option<SkillEntry> {
        let _ = (u, skill);
        None
    }
    /// The unit's level in `skill` with its bonuses (items, `+skills`
    /// stats), cut to the level cap; 0 without the skill.
    fn skill_level(&self, u: UnitId, skill: i32) -> i32 {
        let _ = (u, skill);
        0
    }
    /// The entry of the skill the unit is using.
    fn used_skill(&self, u: UnitId) -> Option<SkillEntry> {
        let _ = u;
        None
    }
    /// The flags word of the unit's entry of `skill`; 0 without one.
    fn entry_flags(&self, u: UnitId, skill: i32) -> u32 {
        let _ = (u, skill);
        0
    }
    /// Sets the flags word of the unit's entry of `skill`.
    fn set_entry_flags(&mut self, u: UnitId, skill: i32, flags: u32) {
        let _ = (u, skill, flags);
    }
    /// Param `i` (1 to 4) of the entry of the skill in use; 0 without
    /// one. A start body leaves param 1 for its do body to read.
    fn used_entry_param(&self, u: UnitId, i: u8) -> i32 {
        let _ = (u, i);
        0
    }
    /// Sets param `i` (1 to 4) of the entry of the skill in use.
    fn set_used_entry_param(&mut self, u: UnitId, i: u8, v: i32) {
        let _ = (u, i, v);
    }
    /// Rebuilds the stat list of `skill`'s passive state from its
    /// `passivestat` columns at the unit's current level (none at level
    /// 0).
    fn refresh_passive(&mut self, u: UnitId, skill: i32) {
        let _ = (u, skill);
    }
    /// The `skillscode` formula at offset `calc` evaluated for the unit
    /// using `skill` at `lvl` (`u32::MAX` or an offset past the code: 0).
    fn eval_skill_calc(&mut self, u: UnitId, calc: u32, skill: i32, lvl: i32) -> i32 {
        let _ = (u, calc, skill, lvl);
        0
    }
    /// A progressive skill's count formula: `prgcalc1`, or `prgcalc<n>`
    /// for the `n` charges on its aura state.
    fn prog_count(&mut self, u: UnitId, skill: i32, lvl: i32) -> i32 {
        let _ = (u, skill, lvl);
        0
    }
    /// The skill's attack rating bonus at `lvl`.
    fn skill_to_hit(&mut self, u: UnitId, skill: i32, lvl: i32) -> i32 {
        let _ = (u, skill, lvl);
        0
    }
    /// The skill's physical damage (min, max) at `lvl`, in 1/256 points;
    /// with `src_dam` scaled by its `SrcDam` share.
    fn skill_phys_damage(&mut self, u: UnitId, skill: i32, lvl: i32, src_dam: bool) -> (i32, i32) {
        let _ = (u, skill, lvl, src_dam);
        (0, 0)
    }
    /// The skill's elemental damage (min, max) at `lvl`, in 1/256
    /// points; with `mastery` raised by the unit's mastery of the element.
    fn skill_elem_damage(&mut self, u: UnitId, skill: i32, lvl: i32, mastery: bool) -> (i32, i32) {
        let _ = (u, skill, lvl, mastery);
        (0, 0)
    }
    /// The skill's elemental duration at `lvl`, in frames.
    fn skill_elem_len(&mut self, u: UnitId, skill: i32, lvl: i32) -> i32 {
        let _ = (u, skill, lvl);
        0
    }
    /// The unit's melee reach: a player's weapon `rangeadder`.
    fn melee_range(&self, u: UnitId) -> i32 {
        let _ = u;
        0
    }
    /// The monster class a monster's AI spawns (its minions, a summon
    /// skill without a `summon` row); `None` without one.
    fn minion_spawn_class(&self, u: UnitId) -> Option<i32> {
        let _ = u;
        None
    }
}
