//! Declared crowd control: stun, interrupt (get-hit), knockback and freeze that a
//! source states for itself.
//!
//! D2Gamma's own rule ("classic") is emergent: any hit may interrupt by damage size,
//! a missile row's `gethit`/`knockback` columns interrupt and shove, stun and freeze
//! lengths come from skills and items. A mod can switch a defender class to
//! `crowd_control = "declared"` (see `GlobalRules`): the engine then keeps a hit's
//! stun, freeze, get-hit and knockback only when the source *declared* it on the
//! record ([`Declared`]). Sources declare with [`declare`]: a skill part, a
//! `crowd_control.toml` source (monster attack, missile row, skill), an item's
//! stun-length property, or a trigger action.
//!
//! A mod with no declarations and `classic` rules never reaches this module's
//! effects, so D2Gamma's play, RNG order and frame hashes are unchanged.

use super::{result, CombatWorld, DamageRecord};
use crate::rules::global::{CrowdControl, GlobalRules};
use crate::units::UnitType;

/// Most effects one source declares.
pub const MAX_EFFECTS: usize = 8;
/// Longest declared stun or freeze, in base ticks (the engine's own stun cap, 10 s).
pub const MAX_TICKS: i32 = 250;

/// What a source does to the unit it hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CcKind {
    /// The get-hit reaction: the hit interrupts what the unit is doing, whatever
    /// its size.
    Interrupt,
    /// The knockback reaction (and an interrupt for a unit with no knockback mode).
    Knockback,
    /// The stun state for [`CcEffect::ticks`]; a stun also interrupts.
    Stun,
    /// Freeze for [`CcEffect::ticks`] (the engine's freeze rules for the unit apply).
    Freeze,
}

/// One declared effect: kind, chance in percent (100: always, no random draw) and,
/// for stun and freeze, the length in base ticks (25 to the second).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CcEffect {
    pub kind: CcKind,
    pub chance: u8,
    pub ticks: i32,
}

/// Where a hit comes from, for the sources a mod declares by table row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CcSource {
    /// A melee hit by a monster of `monstats` row `class` in monster mode `mode`
    /// (4, 5: attack 1, 2; 8 to 11: skill 1 to 4).
    Monster { class: u32, mode: u32 },
    /// A hit by a missile of `missiles` row `row`, made by `skill` (-1: none).
    Missile { row: u16, skill: i32 },
}

/// What the hit's source declared. Carried on the [`DamageRecord`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Declared {
    /// The hit may play the get-hit reaction regardless of its size.
    pub interrupt: bool,
    /// The hit may knock back.
    pub knockback: bool,
    /// The declared stun length in base ticks (0: none).
    pub stun_len: i32,
    /// The declared freeze length in base ticks (0: none).
    pub freeze_len: i32,
}

impl Declared {
    /// Whether anything is declared.
    pub fn any(&self) -> bool {
        self.interrupt || self.knockback || self.stun_len > 0 || self.freeze_len > 0
    }
}

/// Whether crowd control on `ty` units is declared-only under `rules`.
pub fn is_declared(rules: &GlobalRules, ty: UnitType) -> bool {
    rules.crowd_control_for(ty) == CrowdControl::Declared
}

/// Declares `effects` on a record of attacker `a`. Each effect with a chance below
/// 100 draws once from the attacker's seed (`step() % 100 < chance`); nothing is
/// drawn for a source that declares nothing. Does nothing on a record that is not
/// a hit.
pub fn declare<W: CombatWorld>(
    w: &mut W,
    a: W::Unit,
    rec: &mut DamageRecord,
    effects: &[CcEffect],
) {
    if rec.result & result::HIT == 0 {
        return;
    }
    for e in effects {
        if e.chance < 100 && ((w.seed(a).step() % 100) as u8) >= e.chance {
            continue;
        }
        add(rec, e);
    }
}

/// Adds one effect to the record, with no chance roll.
pub fn add(rec: &mut DamageRecord, e: &CcEffect) {
    let ticks = e.ticks.clamp(0, MAX_TICKS);
    match e.kind {
        CcKind::Interrupt => interrupt(rec),
        CcKind::Knockback => {
            rec.declared.knockback = true;
            rec.result |= result::KNOCKBACK;
        }
        CcKind::Stun => {
            interrupt(rec);
            rec.declared.stun_len = rec.declared.stun_len.max(ticks);
            rec.stun_len = rec.stun_len.max(ticks);
        }
        CcKind::Freeze => {
            rec.declared.freeze_len = rec.declared.freeze_len.max(ticks);
            rec.freeze_len = rec.freeze_len.max(ticks);
        }
    }
}

fn interrupt(rec: &mut DamageRecord) {
    rec.declared.interrupt = true;
    rec.result |= result::GET_HIT;
}

/// An item property's stun of `ticks`: declared by the attacker's gear, so it keeps
/// working under declared-only rules (and interrupts like a declared stun).
pub fn declare_stun(rec: &mut DamageRecord, ticks: i32) {
    let ticks = ticks.clamp(0, MAX_TICKS);
    if ticks > 0 {
        rec.declared.stun_len = rec.declared.stun_len.max(ticks);
        rec.declared.interrupt = true;
    }
}

/// An item property's freeze of `ticks` (see [`declare_stun`]).
pub fn declare_freeze(rec: &mut DamageRecord, ticks: i32) {
    rec.declared.freeze_len = rec.declared.freeze_len.max(ticks.clamp(0, MAX_TICKS));
}

/// Drops the timed effects an undeclared source put on the record, for a defender
/// class that is declared-only. Run before the timed effects are applied.
pub fn strip_timed(rec: &mut DamageRecord) {
    rec.stun_len = rec.declared.stun_len;
    rec.freeze_len = rec.declared.freeze_len;
}

/// Drops every stun and freeze on the record, declared or not, for a defender that is
/// immune to crowd control (the mod's `immune_state`).
pub fn strip_all_timed(rec: &mut DamageRecord) {
    rec.stun_len = 0;
    rec.freeze_len = 0;
}

/// Drops every reaction flag on the record, declared or not, for a defender that is
/// immune to crowd control.
pub fn strip_all_reactions(rec: &mut DamageRecord) {
    rec.result &= !(result::GET_HIT | result::KNOCKBACK);
}

/// Drops the reaction flags an undeclared source put on the record, for a defender
/// class that is declared-only. Run before the defender reacts.
pub fn strip_reaction(rec: &mut DamageRecord) {
    if !rec.declared.interrupt {
        rec.result &= !result::GET_HIT;
    }
    if !rec.declared.knockback {
        rec.result &= !result::KNOCKBACK;
    }
}
