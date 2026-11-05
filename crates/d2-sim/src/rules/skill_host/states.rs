//! Stats and states for a mod game's skill bodies.

use crate::units::UnitId;

/// One stat on a stat list: `stat` at `layer` (the stat's param).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateStat {
    pub stat: u16,
    pub layer: u16,
    pub value: i32,
}

/// What runs when a state's stat list is removed (it expires, is
/// cleared or replaced).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StateRemove {
    /// D2RUST's default: the state's events go, the state goes off and
    /// the unit's passives and (a player's) skills are refreshed.
    #[default]
    Default,
    /// A buff's: the state goes off and the unit is refreshed.
    Buff,
    /// The state goes off, nothing else.
    StateOff,
    /// The mod game's [`crate::rules::SkillProvider::state_removed`].
    Mod,
}

/// A state with its own stat list on a unit.
///
/// Applying it finds the unit's list of `state` or makes one (owned by
/// `source`, carrying `skill` and `level`, attached, the state switched
/// on), sets its expiry, writes `stats` and, with `markers`, the skill and
/// level as stats 350 and 351, and queues the unit for its update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSpec {
    pub state: u16,
    /// The unit the list belongs to (the caster); `None`: no owner.
    pub source: Option<UnitId>,
    pub skill: i32,
    pub level: i32,
    /// The frame the list expires at; `None`: a new list never expires,
    /// an existing one keeps its expiry.
    pub expire: Option<i32>,
    pub stats: Vec<StateStat>,
    /// Writes stats 350 (skill) and 351 (level) on the list.
    pub markers: bool,
    pub on_remove: StateRemove,
}

impl StateSpec {
    /// `state` with no owner, skill, expiry or stats, removed the
    /// default way.
    pub fn new(state: u16) -> Self {
        Self {
            state,
            source: None,
            skill: 0,
            level: 0,
            expire: None,
            stats: Vec::new(),
            markers: false,
            on_remove: StateRemove::Default,
        }
    }
}

/// A state put on a unit the engine's standard way
/// ([`HostStates::apply_ruled_state`]): the state's stack rules from the mod's
/// `states.toml` apply, and a state with none behaves as the skill bodies' own
/// state apply does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuledState {
    pub state: u16,
    /// The skill and level the state's list records.
    pub skill: i32,
    pub level: i32,
    /// How long it lasts in base ticks (25 a second) when the state's own
    /// rules give no duration; 0 never expires.
    pub base_ticks: i32,
}

/// What a skill body reads and changes of a unit's stats and states
/// beyond the base reads of [`crate::rules::SkillHost`].
pub trait HostStates {
    /// Sets the unit's base `stat` at `layer`.
    fn set_stat_layer(&mut self, u: UnitId, stat: u16, layer: u16, value: i32) {
        let _ = (u, stat, layer, value);
    }
    /// The unit's base stats (its own list), in list order.
    fn base_stat_entries(&self, u: UnitId) -> Vec<StateStat> {
        let _ = u;
        Vec::new()
    }
    /// The unit's total stats (base, items and states), in list order.
    fn stat_entries(&self, u: UnitId) -> Vec<StateStat> {
        let _ = u;
        Vec::new()
    }
    /// The value of `stat` (layer 0) on the unit's list of `state`;
    /// `None` without the list.
    fn state_stat(&self, u: UnitId, state: u16, stat: u16) -> Option<i32> {
        let _ = (u, state, stat);
        None
    }
    /// The unit has a stat list of `state`.
    fn has_state_list(&self, u: UnitId, state: u16) -> bool {
        let _ = (u, state);
        false
    }
    /// Applies `spec` (see [`StateSpec`]); `false` when the state is out
    /// of range or no list could be made.
    fn apply_state(&mut self, u: UnitId, spec: &StateSpec) -> bool {
        let _ = (u, spec);
        false
    }
    /// Puts `spec.state` on `target` for `source` the engine's standard way:
    /// the state's stack rules (`states.toml`) decide what a second
    /// application does, and the target checks of a skill's state apply run
    /// (a monster that is an NPC or cannot be attacked takes none). `false`
    /// when the state was not put on.
    fn apply_ruled_state(&mut self, source: UnitId, target: UnitId, spec: &RuledState) -> bool {
        let _ = (source, target, spec);
        false
    }
    /// Enters one level of a mod trigger's actions: `false` at the depth
    /// limit (the trigger does not run). Pair every `true` with
    /// [`Self::leave_trigger`].
    fn enter_trigger(&mut self) -> bool {
        true
    }
    /// Leaves the level [`Self::enter_trigger`] entered.
    fn leave_trigger(&mut self) {}
    /// The stack count of `state` on the unit: 0 when it is off, 1 for a
    /// state without stack rules that is on.
    fn state_stacks(&self, u: UnitId, state: u16) -> u16 {
        let _ = (u, state);
        0
    }
    /// Adds `count` stacks to the unit's `state` up to its `max_stacks`
    /// (the state is put on first when it is off, with `spec`); the count
    /// after. A state without stack rules is just put on.
    fn add_state_stacks(
        &mut self,
        source: UnitId,
        target: UnitId,
        spec: &RuledState,
        count: u16,
    ) -> u16 {
        let _ = (source, target, spec, count);
        0
    }
    /// Takes `count` stacks off the unit's `state`; the state ends when
    /// none is left. The count after.
    fn remove_state_stacks(&mut self, u: UnitId, state: u16, count: u16) -> u16 {
        let _ = (u, state, count);
        0
    }
    /// Frees the unit's list of `state` (its remove callback runs) and
    /// switches the state off.
    fn clear_state(&mut self, u: UnitId, state: u16) {
        let _ = (u, state);
    }
    /// Switches off, with their lists, the unit's states in the `states`
    /// group of `state` (`state` itself with `include_self`); `true` when
    /// one was on.
    fn clear_state_group(&mut self, u: UnitId, state: u16, include_self: bool) -> bool {
        let _ = (u, state, include_self);
        false
    }
    /// Switches `state` on or off without touching its stat list.
    fn state_on(&mut self, u: UnitId, state: u16, on: bool) {
        let _ = (u, state, on);
    }
    /// Registers event handler `func` (the skills `auraeventfunc` table)
    /// for unit event `event`, with the skill and level, owned by `state`
    /// (removed with its list); `false` for a function the table lacks.
    fn add_state_event(
        &mut self,
        u: UnitId,
        event: u8,
        func: i32,
        skill: i32,
        lvl: i32,
        state: u16,
    ) -> bool {
        let _ = (u, event, func, skill, lvl, state);
        false
    }
    /// Removes the event handlers `state` registered.
    fn clear_state_events(&mut self, u: UnitId, state: u16) {
        let _ = (u, state);
    }
}
