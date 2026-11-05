//! A mod game's NPC talk and hirelings ([`super::NpcProvider`]): the lines an
//! NPC says at a chat open, the hire list and the hire offer, and a
//! hireling's experience and stat growth.
//!
//! Each hook answers first and D2RUST's rule runs when it doesn't. The
//! offer hook is asked both at the hire (the price, the row and the level)
//! and at the mercenary's init (its level and experience), with the same
//! request, so the two always agree.

use super::hire::HireSlot;
use crate::rules::Ruleset;
use crate::world::hirelings::rows::{threshold, HirelingRow, Offer};

/// The hire offer of a slot seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HireRequest {
    /// An expansion game (the `hireling` rows of version 100).
    pub expansion: bool,
    /// The slot's name string id.
    pub name: u16,
    /// The slot's seed.
    pub seed: u32,
    /// The act of the slot's rows (0-based).
    pub act: u32,
    /// The game's difficulty (0-based).
    pub difficulty: u8,
    /// The player's level (stat 12).
    pub player_level: i32,
}

/// A hireling's stats and skills at a level, set as base values.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MercGrowth {
    /// `(stat, value)`, set in order.
    pub stats: Vec<(u16, i32)>,
    /// `(skill, level)`, set in order.
    pub skills: Vec<(u32, i32)>,
}

/// The experience of hireling level `level` on row `r`: the mod's, else
/// D2RUST's threshold.
pub fn merc_exp(rules: &Ruleset, r: &HirelingRow, level: i32) -> i32 {
    let value = rules
        .npcs()
        .and_then(|p| p.merc_exp(r, level))
        .unwrap_or_else(|| threshold(r.exp_lvl, level));
    scale_merc_exp(rules, value)
}

/// The mod's hire offer for `req`; `None`: D2RUST's.
pub fn mod_offer(rules: &Ruleset, req: &HireRequest) -> Option<Option<Offer>> {
    rules.npcs()?.hire_offer(req)
}

/// The mod's hire list, at most `max` slots.
pub(crate) fn mod_hire_list(mut slots: Vec<HireSlot>, max: usize) -> Vec<HireSlot> {
    slots.truncate(max);
    slots
}

/// Shared by hire offers and threshold consumers, including kill-share caps.
pub(crate) fn scale_merc_exp(rules: &Ruleset, value: i32) -> i32 {
    let percent = rules.global_rules().hireling_experience_curve_percent;
    if percent == 100 {
        return value;
    }
    // Experience stats are signed words but are compared as unsigned.
    crate::rules::global::percent(value as u32, percent) as i32
}
