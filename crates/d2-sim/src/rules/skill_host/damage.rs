//! Hits and damage for a mod game's skill bodies.

use crate::combat::hit::BlockResult;
use crate::combat::DamageRecord;
pub use crate::skills::levels::ElementAdded;
use crate::units::UnitId;

/// What a skill body does to land a hit: the rolls, the damage record's
/// fill from the skill and the weapon, and the record queued or applied,
/// all on D2RUST's combat code.
pub trait HostDamage {
    /// The hit test of `a` against `d` with `bonus` attack rating (one
    /// draw from the attacker's seed; `missile` uses the missile terms).
    fn hit_roll(&mut self, a: UnitId, d: UnitId, bonus: i32, missile: bool) -> bool {
        let _ = (a, d, bonus, missile);
        false
    }
    /// The defender's block (with `block`), else its weapon block, avoid
    /// (with `avoid`, for missiles) or dodge, or evade while moving.
    fn defend_roll(&mut self, a: UnitId, d: UnitId, avoid: bool, block: bool) -> BlockResult {
        let _ = (a, d, avoid, block);
        BlockResult::None
    }
    /// The result flags of a melee attack (range, hit test with `bonus`,
    /// block and dodge, get-hit); `range_offset` ≠ 0 widens the reach by
    /// it and the attacker's melee range. 0: no hit.
    fn melee_result(&mut self, a: UnitId, d: UnitId, bonus: i32, range_offset: i32) -> u16 {
        let _ = (a, d, bonus, range_offset);
        0
    }
    /// Adds the skill's rolled physical damage at `lvl` to `rec`.
    fn roll_skill_physical(&mut self, a: UnitId, skill: i32, lvl: i32, rec: &mut DamageRecord) {
        let _ = (a, skill, lvl, rec);
    }
    /// Adds the skill's rolled elemental damage (its `EType`, with the
    /// mastery) and length at `lvl` to `rec`; the value added.
    fn roll_skill_element(
        &mut self,
        a: UnitId,
        skill: i32,
        lvl: i32,
        rec: &mut DamageRecord,
    ) -> i32 {
        let _ = (a, skill, lvl, rec);
        0
    }
    /// Adds `value` of element `element` (and its length `len`) to `rec`
    /// (10: one random element).
    fn add_element(
        &mut self,
        a: UnitId,
        rec: &mut DamageRecord,
        element: i32,
        value: i32,
        len: i32,
    ) -> ElementAdded {
        let _ = (a, rec, value, len);
        ElementAdded {
            element,
            resist: -1,
            hit_class: 0,
        }
    }
    /// Rolls the attacker's weapon and elemental damage against `d` into
    /// `rec` (bonuses, critical strike, leech, conversion), scaled by
    /// `src_dam` / 128 (0: all), with the `offhand` weapon.
    fn fill_damage(
        &mut self,
        a: UnitId,
        d: UnitId,
        rec: &mut DamageRecord,
        offhand: bool,
        src_dam: i32,
    ) {
        let _ = (a, d, rec, offhand, src_dam);
    }
    /// Starts a combat: on a hit not avoided, rolls `rec` (unless it is
    /// flagged rolled-skip) with `src_dam`, totals it, flags `WILL_DIE`
    /// when it kills, and queues a copy on the attacker's combat list.
    fn start_combat(&mut self, a: UnitId, d: UnitId, rec: &mut DamageRecord, src_dam: i32) {
        let _ = (a, d, rec, src_dam);
    }
    /// Applies the attacker's queued record against `d` (in melee range,
    /// with durability loss, thorns, the attack events and the
    /// defender's reaction) and frees it.
    fn apply_queued(&mut self, a: UnitId, d: UnitId) {
        let _ = (a, d);
    }
    /// Applies `rec` to `d` now (hostility and room gates, damage events,
    /// leech, life loss, timed effects, kill); `missile` totals it first.
    fn apply_damage(&mut self, a: UnitId, d: UnitId, missile: bool, rec: &mut DamageRecord) {
        let _ = (a, d, missile, rec);
    }
    /// Runs the handlers of unit event `event` (`events` row) on `u`,
    /// with the other unit and the record; the last handler's result.
    fn unit_event(
        &mut self,
        event: u8,
        u: UnitId,
        other: Option<UnitId>,
        rec: Option<&mut DamageRecord>,
    ) -> i32 {
        let _ = (event, u, other, rec);
        0
    }
}
