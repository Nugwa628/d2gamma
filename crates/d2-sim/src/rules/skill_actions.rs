//! World actions for a mod game's skill bodies: part of
//! [`super::SkillHost`].
//!
//! A mod's skill body moves a unit, picks items up, opens portals, plays
//! a unit's sound or the "can't do that" message, pulses an aura's stats
//! onto the units around its caster, curses the units around its aim point, or runs one of D2RUST's own skill
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
    /// The state the caster gets.
    pub state: u16,
    /// The state the units around the caster get.
    pub target_state: u16,
    /// The radius around the caster.
    pub range: i32,
    /// How long the state stays, in base ticks.
    pub frames: i32,
    /// The unit filter (`aurafilter` bits; 0 = the default filter).
    pub filter: u32,
    /// Up to six `(stat, value)` the state's list holds.
    pub stats: &'a [(u16, i32)],
}

/// One curse ([`SkillActions::curse_pulse`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursePulse<'a> {
    /// The skill and level the state's list records.
    pub skill: i32,
    pub level: i32,
    /// The state each unit gets.
    pub state: u16,
    /// The radius around the aim point.
    pub range: i32,
    /// How long the state stays, in base ticks.
    pub frames: i32,
    /// The unit filter (`aurafilter` bits; 0 = the default filter).
    pub filter: u32,
    /// Up to six `(stat, value)` the state's list holds.
    pub stats: &'a [(u16, i32)],
}

/// World actions (a supertrait of [`super::SkillHost`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuffPulse<'a> {
    pub skill: i32,
    pub level: i32,
    pub state: u16,
    pub frames: i32,
    pub stats: &'a [(u16, i32)],
    pub events: &'a [(u16, u16)],
}

/// World actions (a supertrait of [`super::SkillHost`]).
#[allow(unused_variables)]
pub trait SkillActions {
    /// The sight pulse's loaded state and stat rows exist.
    fn sight_rows_valid(&self, state: u16, stat: u16) -> bool {
        false
    }
    /// Caster-centered scan excluding monsters that disallow aura effects.
    fn sight_targets(&mut self, source: UnitId, range: i32, filter: u32) -> Vec<UnitId> {
        Vec::new()
    }
    /// One ordinary timed state application, without AI or curse-specific scaling.
    fn sight_state(&mut self, source: UnitId, target: UnitId, pulse: &CursePulse<'_>) {}
    /// Shared AI-control eligibility for a hostile monster.
    fn ai_curse_eligible(&mut self, source: UnitId, target: UnitId, attract: bool) -> bool {
        false
    }
    /// Scale AI curse duration for the active difficulty.
    fn ai_curse_duration(&mut self, frames: i32) -> i32 {
        frames
    }
    /// Aim-point scan, or None when no room/point can be scanned.
    fn ai_curse_targets(&mut self, source: UnitId, range: i32, filter: u32) -> Option<Vec<UnitId>> {
        None
    }
    /// Attach an explicit AI curse state and expiry callback.
    fn ai_curse_state(
        &mut self,
        source: UnitId,
        target: UnitId,
        pulse: &CursePulse<'_>,
        attract: bool,
    ) -> bool {
        false
    }
    /// Set temporary alignment and the AI node.
    fn ai_curse_align(&mut self, target: UnitId, attract: bool) {}
    /// Override a monster's target until an ordinary AI reset timer.
    fn ai_curse_redirect(&mut self, unit: UnitId, target: Option<UnitId>, frames: i32) {}
    /// The loaded state and counter rows exist for a martial charge.
    fn martial_charge_valid(&self, state: u16, counter: u16) -> bool {
        false
    }
    /// Select the do-frame flag for a dual-claw charge-up.
    fn martial_claw_frame(&mut self, u: UnitId) {}
    /// Increment the explicit charge counter and refresh its ordinary charge-list lifetime.
    fn martial_charge(&mut self, u: UnitId, counter: u16, pulse: &BuffPulse<'_>) {}
    /// Consume a selectable dead corpse; soft requires the potion eligibility flag.
    fn consume_find_corpse(&mut self, corpse: UnitId, soft: bool) -> bool {
        let _ = (corpse, soft);
        false
    }
    /// The corpse's ordinary treasure drop, including the native quality hint.
    fn corpse_item_drop(&mut self, source: UnitId, corpse: UnitId, quality: i32) {
        let _ = (source, corpse, quality);
    }
    /// The act/difficulty potion selector with explicit category bands.
    fn corpse_potion_drop(&mut self, source: UnitId, corpse: UnitId, bands: [i32; 2]) {
        let _ = (source, corpse, bands);
    }
    /// Write or detach a passive list using the engine's refresh markers.
    fn passive_values(
        &mut self,
        u: UnitId,
        state: u16,
        skill: i32,
        level: i32,
        stats: &[super::StateStat],
    ) {
        let _ = (u, state, skill, level, stats);
    }
    /// A table state may be used for an aura.
    fn aura_state_valid(&self, state: u16) -> bool {
        false
    }
    /// Duration at the current frame through the next aligned aura period.
    fn aura_period_frames(&self, base: i32) -> i32 {
        base.max(5)
    }
    /// Apply one aura state with already scaled duration; freeze selects its own callback.
    fn aura_state(&mut self, source: UnitId, target: UnitId, pulse: &AuraPulse<'_>, freeze: bool) {}
    /// Units selected by the aura scan, excluding its caster, in scan order.
    fn aura_targets(&mut self, source: UnitId, radius: i32, filter: u32) -> Vec<UnitId> {
        Vec::new()
    }
    /// A nonmonster, or a monster whose cold-effect entry allows Holy Freeze.
    fn freeze_aura_target(&self, target: UnitId) -> bool {
        false
    }
    /// Enter or toggle off an explicit form, preserving restrictive-state gating.
    fn shift_form(&mut self, u: UnitId, pulse: &BuffPulse<'_>, delay: i32) -> bool {
        false
    }
    /// Restore a form skill entry's animation mode after its state ends.
    fn clear_form_entry(&mut self, u: UnitId, skill: i32) {}
    /// Apply a warcry effect supplied by data to a caster or a missile-hit target.
    fn warcry_state(&mut self, source: UnitId, target: UnitId, pulse: &BuffPulse<'_>) -> bool {
        false
    }
    /// Apply a self buff with explicit stats and combat-event callbacks.
    fn timed_buff(&mut self, u: UnitId, pulse: &BuffPulse<'_>) -> bool {
        false
    }
    /// Plays overlay `id` on the unit (a hit's flash or glow).
    fn unit_overlay(&mut self, u: UnitId, id: i32) {}
    /// The unit's running speed as the engine states it (the player class's
    /// `RunVelocity`, the monster's `Run`; five of it are a sub-tile per base
    /// tick); 0 when it has none.
    fn run_speed(&self, u: UnitId) -> i32 {
        0
    }
    /// The unit's target is within the unit's melee reach.
    fn in_reach(&mut self, u: UnitId, target: UnitId) -> bool {
        false
    }
    /// Puts the unit in (or takes it out of) the engine's skill-move state:
    /// it cannot be interrupted while it moves under a skill.
    fn skill_move(&mut self, u: UnitId, on: bool) {}
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
    /// Puts `pulse.state` with `pulse.stats` on the caster and
    /// `pulse.target_state` with the same stats on every other unit within
    /// `pulse.range` of the caster that passes `pulse.filter`, as an aura does
    /// each period; the number of units whose stats changed.
    fn aura_pulse(&mut self, u: UnitId, pulse: &AuraPulse<'_>) -> i32 {
        0
    }
    /// Curses the units within `pulse.range` of the aim point that pass
    /// `pulse.filter` and the curse tests (hostile, alive, awake): the state
    /// `pulse.state` with `pulse.stats`, as the curse skills do. 1 when the
    /// cast had a point to scan, else 0.
    fn curse_pulse(&mut self, u: UnitId, pulse: &CursePulse<'_>) -> i32 {
        0
    }
    /// Sets a monster's damage and to-hit to those of attack mode `mode` of
    /// its `monstats` row, as the engine's monster skills do before a swing
    /// (1: the first attack, 5: the second, 8: the first skill). A unit that
    /// is not a monster is left as it is.
    fn mode_damage(&mut self, u: UnitId, mode: i32) {
        let _ = (u, mode);
    }
    /// Lays a wall of missile row `missile` across the skill's target as the
    /// engine's Fire Wall does: two missiles running along the wall, and one
    /// of row `centre` (when it is a missile row) at its middle. 1 when the
    /// wall was laid, 0 when the point is blocked, in town or missing.
    fn lay_wall(&mut self, u: UnitId, missile: i32, centre: i32, skill: i32, lvl: i32) -> i32 {
        let _ = (u, missile, centre, skill, lvl);
        0
    }
    /// Raises the dead monster that is the unit's target as the engine's
    /// Resurrect does (the monster's own revive and its rise animation
    /// mode), with overlay `overlay` on it (-1 for none). 1 when it rose.
    fn resurrect(&mut self, u: UnitId, overlay: i32) -> i32 {
        let _ = (u, overlay);
        0
    }
    /// Hatches the nest the unit's start function prepared, as the engine's
    /// Nest does (the monster of the used skill entry's parameters, spawned
    /// at its place), with overlay `overlay` on it (-1 for none). 1 when a
    /// monster was spawned.
    fn nest(&mut self, u: UnitId, overlay: i32) -> i32 {
        let _ = (u, overlay);
        0
    }
    /// Gives `target` back `percent` of its maximum life (it keeps at least
    /// 1 and stays at or below the maximum), as the engine's Zakarum heal
    /// does; false when the host cannot.
    fn heal(&mut self, u: UnitId, target: UnitId, percent: i32) -> bool {
        let _ = (u, target, percent);
        false
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
