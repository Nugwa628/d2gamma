//! World actions for a mod game's skill bodies: part of
//! [`super::SkillHost`].
//!
//! A mod's skill body moves a unit, picks items up, opens portals, plays
//! a unit's sound or the "can't do that" message, pulses an aura's stats
//! onto the units around its caster, or runs one of D2RUST's own skill
//! bodies by slot (the mod keeps the map from its own ids to slots).
//! Every method has a default that does nothing.

use crate::rules::SkillFn;
use crate::units::UnitId;

/// One pulse of an aura ([`SkillActions::aura_pulse`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuraPulse<'a> {
    /// The skill and level the state's list records.
    pub skill: i32,
    pub level: i32,
    /// The state each unit gets.
    pub state: u16,
    /// The radius around the caster.
    pub range: i32,
    /// How long the state stays, in base ticks.
    pub frames: i32,
    /// The unit filter (`aurafilter` bits; 0 = the default filter).
    pub filter: u32,
    /// Up to six `(stat, value)` the state's list holds.
    pub stats: &'a [(u16, i32)],
}

/// World actions (a supertrait of [`super::SkillHost`]).
#[allow(unused_variables)]
pub trait SkillActions {
    /// Places the unit at `to` (in the room holding it); false when it
    /// could not be placed there.
    fn teleport(&mut self, u: UnitId, to: (i32, i32)) -> bool {
        false
    }
    /// The unit picks the item up (gold, potions and the other items the
    /// auto pickup takes).
    fn auto_pickup(&mut self, u: UnitId, item: UnitId) {}
    /// Opens a portal pair from the unit's position to level `level`,
    /// object class `class`; false when none was made (no room, a town,
    /// another act, no free spot).
    fn open_portal(&mut self, u: UnitId, level: u32, class: u32) -> bool {
        false
    }
    /// The player casts a town portal; false when none was made.
    fn open_town_portal(&mut self, u: UnitId) -> bool {
        false
    }
    /// The unit plays its event sound `id` (0x13: the "I can't" line).
    fn unit_sound(&mut self, u: UnitId, id: i32) {}
    /// Sends the unit's client the "can't do that" message.
    fn cant_do(&mut self, u: UnitId) {}
    /// Puts `pulse.state` with `pulse.stats` on every unit within
    /// `pulse.range` of the caster that passes `pulse.filter` (the caster
    /// included), as an aura does each period; the number of units whose
    /// stats changed.
    fn aura_pulse(&mut self, u: UnitId, pulse: &AuraPulse<'_>) -> i32 {
        0
    }
    /// Runs D2RUST's own body of slot `index` of the `kind` table (never
    /// the mod's): its result, `None` when the slot is past the table.
    fn engine_body(
        &mut self,
        kind: SkillFn,
        index: u16,
        u: UnitId,
        skill: i32,
        level: i32,
    ) -> Option<i32> {
        None
    }
}
