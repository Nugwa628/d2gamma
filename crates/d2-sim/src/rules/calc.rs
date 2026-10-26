//! The unit a formula is evaluated for, as a mod game's formula functions
//! and parameters ([`super::CalcProvider`]) read it: [`CalcUnit`], from
//! [`crate::skills::calc::CalcContext::unit`].

use crate::units::UnitType;

/// The unit of a formula evaluation: a skills formula's unit, a missiles
/// formula's owner. Every read is of that one unit.
pub trait CalcUnit {
    /// The unit's type.
    fn unit_type(&self) -> UnitType;
    /// The unit's class (monstats or player class row).
    fn class_id(&self) -> i32;
    /// The unit's total value of `stat` at `layer`.
    fn stat(&self, stat: u16, layer: u16) -> i32;
    /// The unit's base value of `stat` at `layer`.
    fn base_stat(&self, stat: u16, layer: u16) -> i32;
    /// The unit has `state`.
    fn has_state(&self, state: u16) -> bool;
    /// The skills in the unit's skill list, in list order (a skill granted
    /// twice is listed twice).
    fn skills(&self) -> Vec<i32>;
    /// The unit's level in `skill`: its hard points, with the bonuses when
    /// `total`; 0 when it doesn't have the skill.
    fn skill_level(&mut self, skill: i32, total: bool) -> i32;
    /// The number of the unit's pets of monster class `class`.
    fn pet_count(&self, class: i32) -> i32;
    /// The total value of `stat` at `layer` on the item the unit wears at
    /// body location `loc`; 0 without one.
    fn item_stat(&self, loc: u8, stat: u16, layer: u16) -> i32;
    /// The level id of the unit's room; 0 without a room.
    fn level_id(&self) -> i32;
    /// The unit's current mode.
    fn mode(&self) -> i32;
    /// Skills formula `field` evaluated for this unit with `skill` at
    /// `lvl` (any skill and level, not only the context's).
    fn eval(&mut self, field: u32, skill: i32, lvl: i32) -> i32;
    /// Skills parameter `code` (a `skillcalc` row) for this unit with
    /// `skill` at `lvl`.
    fn special(&mut self, code: u8, skill: i32, lvl: i32) -> i32;
}
