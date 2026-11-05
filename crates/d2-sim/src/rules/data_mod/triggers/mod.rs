//! Triggers: a mod's `files/triggers.toml`.
//!
//! A trigger says "when this happens to a unit, maybe do this". It lives on
//! a source (a skill the unit has, an item property the unit's gear gives,
//! or a state the unit is in), reacts to one event, rolls a chance, checks
//! conditions and runs actions:
//!
//! ```toml
//! version = 1
//!
//! [[trigger]]
//! name = "Reaction window"
//! on = "attack"                 # attack | hit | being_hit | skill_used | kill | state_start | state_end
//! while_skill = 391             # the source: while the unit has this skill (its level is `lvl`)
//! chance = "4 + lvl"            # percent, a formula; default 100
//!
//! [[trigger.condition]]         # all must hold
//! kind = "lacks_state"
//! state = 381
//!
//! [[trigger.action]]            # run in order
//! kind = "apply_state"
//! state = 381
//! seconds = 1
//! ```
//!
//! The event is the unit's own: `attack` is a melee swing it makes, `hit`
//! damage it deals, `being_hit` damage it takes, `skill_used` a skill it has
//! used (`skills = [..]` filters; a skill a trigger casts does not count),
//! `kill` a kill it makes, and `state_start` / `state_end` a state of
//! `states.toml` turning on or running out on it (`state = N`; a state the
//! mod itself removes with a trigger action or a skill part does not end).
//!
//! Every trigger is bounded: at most [`MAX_TRIGGERS`] of them, each with at
//! most [`MAX_CONDITIONS`] conditions and [`MAX_ACTIONS`] actions; triggers
//! that fire triggers stop [`MAX_DEPTH`] levels down. Rolls draw only from
//! the unit's own seed, so a replay gives the same game.

use serde::Deserialize;

use super::{Formula, StateRules};
use crate::combat::cc::CcEffect;

mod run;

/// The one version of `triggers.toml` this build reads.
pub const VERSION: u32 = 1;
/// Most triggers in a file.
pub const MAX_TRIGGERS: usize = 64;
/// Most conditions on one trigger.
pub const MAX_CONDITIONS: usize = 8;
/// Most actions of one trigger.
pub const MAX_ACTIONS: usize = 8;
/// How many triggers deep a trigger's own actions may set off more triggers.
pub const MAX_DEPTH: u8 = 3;

/// A `triggers.toml` that cannot be used. A setting error names the entry
/// and the key (`trigger[1].action[0].seconds`).
#[derive(Debug, thiserror::Error)]
pub enum TriggersError {
    #[error("invalid triggers.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid triggers.toml setting: {0}")]
    Setting(String),
}

/// What a trigger reacts to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum On {
    /// The unit swings at something in melee (hit or miss).
    Attack,
    /// The unit deals damage.
    Hit,
    /// The unit takes damage.
    BeingHit,
    /// The unit used a skill.
    SkillUsed,
    /// The unit killed something.
    Kill,
    /// A state with rules turned on for the unit.
    StateStart,
    /// A state with rules ran out (or was cleared by the game) on the unit.
    StateEnd,
}

/// Whose side of the event an action or condition looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Who {
    /// The unit holding the trigger.
    #[default]
    #[serde(rename = "self")]
    Holder,
    /// The other unit of the event (the one hit, the attacker, the victim).
    Other,
}

/// A thing that must hold for the trigger to run.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    HasState {
        state: u16,
        #[serde(default)]
        who: Who,
    },
    LacksState {
        state: u16,
        #[serde(default)]
        who: Who,
    },
    /// The state is on with at least `count` stacks.
    StacksAtLeast {
        state: u16,
        count: u16,
        #[serde(default)]
        who: Who,
    },
    /// Life is below `percent` of its maximum.
    LifeBelow {
        percent: u8,
        #[serde(default)]
        who: Who,
    },
    /// Life is at or above `percent` of its maximum.
    LifeAbove {
        percent: u8,
        #[serde(default)]
        who: Who,
    },
}

fn one() -> Formula {
    Formula::constant(1)
}

fn lvl() -> Formula {
    Formula::compile("lvl").expect("`lvl` compiles")
}

fn hundred() -> Formula {
    Formula::constant(100)
}

/// What a trigger does.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    CallScript(crate::rules::scripts::CallSpec),
    /// Put the state on (a second application does what its `reapply` says).
    ApplyState {
        state: u16,
        #[serde(default)]
        seconds: Option<Formula>,
        #[serde(default)]
        who: Who,
    },
    /// Add `count` stacks (the state comes on first when it is off).
    AddStack {
        state: u16,
        #[serde(default = "one")]
        count: Formula,
        #[serde(default)]
        seconds: Option<Formula>,
        #[serde(default)]
        who: Who,
    },
    /// Take `count` stacks off; the state ends with the last.
    RemoveStack {
        state: u16,
        #[serde(default = "one")]
        count: Formula,
        #[serde(default)]
        who: Who,
    },
    /// End the state.
    RemoveState {
        state: u16,
        #[serde(default)]
        who: Who,
    },
    /// Stun, interrupt, knock back or freeze the unit, as a hit of the holder
    /// that does no damage: `effect = { kind = "stun", seconds = 0.5 }`
    /// (`chance` in percent is rolled on the holder's seed).
    CrowdControl {
        #[serde(deserialize_with = "super::cc::effect")]
        effect: CcEffect,
        #[serde(default = "other")]
        who: Who,
    },
    /// Cast a skill as the holder at the other unit (or at itself).
    CastSkill {
        skill: u16,
        #[serde(default = "lvl")]
        level: Formula,
        #[serde(default = "other")]
        at: Who,
    },
}

fn other() -> Who {
    Who::Other
}

/// One trigger.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerDef {
    /// A label for tools and messages; the game ignores it.
    #[serde(default)]
    pub name: String,
    pub on: On,
    /// Source: the unit has this skill (its level is the formulas' `lvl`).
    #[serde(default)]
    pub while_skill: Option<u16>,
    /// Source: the unit's stat (an `itemstatcost` row; an item property
    /// gives it) is above 0 (its value is the formulas' `lvl`).
    #[serde(default)]
    pub while_stat: Option<u16>,
    /// Source: the unit is in this state (its stack count is `lvl`).
    #[serde(default)]
    pub while_state: Option<u16>,
    /// Percent chance, 0 to 100.
    #[serde(default = "hundred")]
    pub chance: Formula,
    /// For `skill_used`: only these skills; empty means any.
    #[serde(default)]
    pub skills: Vec<u16>,
    /// For `attack`, `hit` and `kill`: only while the holder is using this
    /// skill. A hit counts when it lands during the cast (a melee skill's
    /// swing, an area or chain part); a missile's later impact does not.
    #[serde(default)]
    pub by_skill: Option<u16>,
    /// For `state_start` and `state_end`: the state.
    #[serde(default)]
    pub state: Option<u16>,
    #[serde(default, rename = "condition")]
    pub conditions: Vec<Condition>,
    #[serde(default, rename = "action")]
    pub actions: Vec<Action>,
}

/// All the triggers of a mod, in file order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TriggerRules {
    defs: Vec<TriggerDef>,
}

impl TriggerRules {
    /// No triggers.
    pub fn none() -> TriggerRules {
        TriggerRules::default()
    }

    /// `true` when the mod has no trigger.
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// The triggers, in file order.
    pub fn defs(&self) -> &[TriggerDef] {
        &self.defs
    }

    /// Reads a `triggers.toml`. A wrong value is an error naming the entry
    /// and the key (`trigger[1].on`).
    pub fn from_toml(text: &str) -> Result<TriggerRules, TriggersError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            #[serde(default = "file_version")]
            version: u32,
            #[serde(default, rename = "trigger")]
            triggers: Vec<toml::Value>,
        }
        let file: File = toml::from_str(text)?;
        if file.version != VERSION {
            return Err(TriggersError::Setting(format!(
                "`version`: {} is not supported (this build reads version {VERSION})",
                file.version
            )));
        }
        if file.triggers.len() > MAX_TRIGGERS {
            return Err(TriggersError::Setting(format!(
                "`trigger`: {} triggers; a mod has at most {MAX_TRIGGERS}",
                file.triggers.len()
            )));
        }
        let mut defs = Vec::with_capacity(file.triggers.len());
        for (i, value) in file.triggers.into_iter().enumerate() {
            let d = TriggerDef::deserialize(value)
                .map_err(|e| TriggersError::Setting(format!("`trigger[{i}]`: {}", e.message())))?;
            check(i, &d)?;
            defs.push(d);
        }
        Ok(TriggerRules { defs })
    }

    /// Runs the triggers `ev` sets off for `u` (see [`UnitEvent`]).
    pub fn run(
        &self,
        w: &mut dyn crate::rules::SkillHost,
        ev: crate::rules::UnitEvent,
        u: crate::units::UnitId,
        other: Option<crate::units::UnitId>,
    ) {
        run::run(self, w, ev, u, other);
    }

    /// Checks the triggers against the mod's states: a state a trigger
    /// watches, counts or stacks needs a `states.toml` entry (only those
    /// states have starts, ends and stacks to follow), and an action that
    /// turns a state on without `seconds` needs a state with a
    /// `duration_seconds` (else it would never end). Marks the watched
    /// states in `states` so the game tells the mod when they end.
    pub fn check_states(&self, states: &mut StateRules) -> Result<(), TriggersError> {
        let ruled = |s: u16| states.get(i32::from(s)).is_some();
        let timed = |s: u16| {
            states
                .get(i32::from(s))
                .is_some_and(|d| d.duration_seconds.is_some())
        };
        let mut watched = Vec::new();
        for (i, d) in self.defs.iter().enumerate() {
            if let Some(s) = d.state {
                if !ruled(s) {
                    return Err(bad(i, "state", no_entry(s)));
                }
                watched.push(s);
            }
            for (j, c) in d.conditions.iter().enumerate() {
                if let Condition::StacksAtLeast { state, .. } = c {
                    if !ruled(*state) {
                        return Err(bad(i, &format!("condition[{j}].state"), no_entry(*state)));
                    }
                }
            }
            for (j, a) in d.actions.iter().enumerate() {
                let key = |k: &str| format!("action[{j}].{k}");
                match a {
                    Action::AddStack { state, seconds, .. }
                    | Action::ApplyState { state, seconds, .. } => {
                        if matches!(a, Action::AddStack { .. }) && !ruled(*state) {
                            return Err(bad(i, &key("state"), no_entry(*state)));
                        }
                        if seconds.is_none() && !timed(*state) {
                            return Err(bad(
                                i,
                                &key("seconds"),
                                format!(
                                    "state {state} has no `duration_seconds` in states.toml, so \
                                     this action needs `seconds` (it would never end)"
                                ),
                            ));
                        }
                    }
                    Action::RemoveStack { state, .. } if !ruled(*state) => {
                        return Err(bad(i, &key("state"), no_entry(*state)));
                    }
                    _ => {}
                }
            }
        }
        states.watch(&watched);
        Ok(())
    }
}

fn no_entry(state: u16) -> String {
    format!(
        "state {state} has no entry in states.toml (stacks, starts and ends are the rules' states)"
    )
}

fn file_version() -> u32 {
    VERSION
}

fn bad(i: usize, key: &str, msg: impl std::fmt::Display) -> TriggersError {
    TriggersError::Setting(format!("`trigger[{i}].{key}`: {msg}"))
}

fn check(i: usize, d: &TriggerDef) -> Result<(), TriggersError> {
    let sources = [
        d.while_skill.is_some(),
        d.while_stat.is_some(),
        d.while_state.is_some(),
    ];
    if sources.iter().filter(|s| **s).count() != 1 {
        return Err(bad(
            i,
            "while_skill",
            "give exactly one source: `while_skill`, `while_stat` or `while_state`",
        ));
    }
    match d.on {
        On::StateStart | On::StateEnd if d.state.is_none() => {
            return Err(bad(
                i,
                "state",
                "a state_start / state_end trigger names its `state`",
            ));
        }
        On::StateStart | On::StateEnd => {}
        _ if d.state.is_some() => {
            return Err(bad(
                i,
                "state",
                "only state_start / state_end triggers have a `state`",
            ));
        }
        _ => {}
    }
    if !d.skills.is_empty() && d.on != On::SkillUsed {
        return Err(bad(
            i,
            "skills",
            "only skill_used triggers filter by `skills`",
        ));
    }
    if d.by_skill.is_some() && !matches!(d.on, On::Attack | On::Hit | On::Kill) {
        return Err(bad(
            i,
            "by_skill",
            "only attack, hit and kill triggers filter by `by_skill`",
        ));
    }
    if d.actions.is_empty() || d.actions.len() > MAX_ACTIONS {
        return Err(bad(
            i,
            "action",
            format!(
                "{} actions; a trigger has 1 to {MAX_ACTIONS}",
                d.actions.len()
            ),
        ));
    }
    if d.conditions.len() > MAX_CONDITIONS {
        return Err(bad(
            i,
            "condition",
            format!(
                "{} conditions; a trigger has at most {MAX_CONDITIONS}",
                d.conditions.len()
            ),
        ));
    }
    for (j, a) in d.actions.iter().enumerate() {
        if let Action::CallScript(call) = a {
            call.check()
                .map_err(|e| bad(i, &format!("action[{j}].call_script"), e))?;
        }
    }
    for (j, c) in d.conditions.iter().enumerate() {
        if let Condition::LifeBelow { percent, .. } | Condition::LifeAbove { percent, .. } = c {
            if *percent > 100 {
                return Err(bad(i, &format!("condition[{j}].percent"), "more than 100"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
