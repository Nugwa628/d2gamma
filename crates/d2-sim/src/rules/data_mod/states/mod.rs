//! States and stacks as data: a mod's `states.toml`.
//!
//! A mod file defines the rules of states that already have a row in the
//! `states` table (the row says how the state looks and behaves; this file
//! says how long it lasts, how it stacks and what the stacks do). Each
//! `[[state]]` entry is one state:
//!
//! ```toml
//! version = 1
//!
//! [[state]]
//! id = 380                  # the row in the states table
//! name = "Smolder"          # shown by tools; the game ignores it
//! duration_seconds = 8      # how long a stack lasts (else the applying skill's)
//! max_stacks = 5            # 1 (default) .. 255
//! reapply = "add_stack"     # "refresh" (default), "add_stack" or "ignore"
//! decay = "lose_one"        # "lose_all" (default) or "lose_one"
//! icon = "smolder"          # a name the client resolves to the HUD icon
//! overlay = 17              # an existing overlay row, shown while the state is on
//!
//! [[state.effect]]          # the effects of ONE stack; they multiply by the stack count
//! stat = 25                 # the row in itemstatcost
//! per_stack = 5
//! ```
//!
//! The stack count of a state on a unit is tracked by the simulation
//! (`StatLists`), changes by the rules here and is sent to the clients of
//! the game (`d2_proto::wide_state::Op::Stacks`). Everything here is
//! integers: the host reads the text once and hands the definitions in.

use serde::Deserialize;

/// The one version of `states.toml` this build reads.
pub const VERSION: u32 = 1;
/// Most stacks one state may have (a stack count has to stay small to show on a HUD icon).
pub const MAX_STACKS: u32 = 255;
/// Longest duration the file may state: a day.
const MAX_SECONDS: u32 = 86_400;

/// A `states.toml` that cannot be used. A setting error names the entry and
/// the key (`state[1].max_stacks`).
#[derive(Debug, thiserror::Error)]
pub enum StatesError {
    #[error("invalid states.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid states.toml setting: {0}")]
    Setting(String),
}

/// What a second application of a state that is already on does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Reapply {
    /// Start the duration over; the stack count stays.
    #[default]
    Refresh,
    /// One more stack (up to the maximum) and the duration starts over.
    AddStack,
    /// Nothing changes: the running state keeps its stacks and its clock.
    Ignore,
}

/// What happens to the stacks when the duration runs out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Decay {
    /// The whole state ends.
    #[default]
    LoseAll,
    /// One stack is lost and the duration starts over, until none is left.
    LoseOne,
}

/// One stat a stack of the state gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StackEffect {
    /// The `itemstatcost` row of the stat.
    pub stat: u16,
    /// What ONE stack adds; the state gives this times its stack count
    /// (or just this, with `flat`).
    pub per_stack: i32,
    /// The stat is given only from this many stacks on (default 1: from the
    /// first). Below it the state gives 0.
    #[serde(default = "one_stack")]
    pub min_stacks: u16,
    /// Give `per_stack` once while `min_stacks` is met, whatever the stack
    /// count above it (default false: `per_stack` times the count).
    #[serde(default)]
    pub flat: bool,
}

fn one_stack() -> u16 {
    1
}

/// The rules of one state.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateDef {
    /// The row in the `states` table.
    pub id: u16,
    /// A label for tools and messages; the game ignores it.
    #[serde(default)]
    pub name: String,
    /// How long the state lasts, in seconds; `None`: the applying skill's.
    #[serde(default)]
    pub duration_seconds: Option<u32>,
    /// Most stacks (1..=255).
    #[serde(default = "one")]
    pub max_stacks: u32,
    #[serde(default)]
    pub reapply: Reapply,
    #[serde(default)]
    pub decay: Decay,
    /// The name of the HUD icon (the client resolves it).
    #[serde(default)]
    pub icon: Option<String>,
    /// An existing `overlay` row the state shows.
    #[serde(default)]
    pub overlay: Option<u16>,
    /// What one stack gives.
    #[serde(default, rename = "effect")]
    pub effects: Vec<StackEffect>,
    /// A trigger watches the state's end (set by the host from
    /// `triggers.toml`, not a key of the file): the game tells the mod when
    /// the state runs out.
    #[serde(skip)]
    pub watched: bool,
}

fn one() -> u32 {
    1
}

impl StateDef {
    /// The duration in ticks of the base rate (25 per second), `None` when
    /// the file leaves it to the applying skill.
    pub fn duration_base_ticks(&self) -> Option<i32> {
        self.duration_seconds
            .map(|s| (s * crate::TICKS_PER_SECOND) as i32)
    }

    /// The stack count after one more application of a state that has
    /// `current` stacks (0: not on yet), `None` when nothing changes.
    pub fn stacks_after_apply(&self, current: u16) -> Option<u16> {
        let max = self.max_stacks as u16;
        match (current, self.reapply) {
            (0, _) => Some(1),
            (_, Reapply::Ignore) => None,
            (c, Reapply::AddStack) if c < max => Some(c + 1),
            _ => None,
        }
    }

    /// Whether another application of a running state starts its duration
    /// over (every policy but `ignore`).
    pub fn reapply_restarts_clock(&self) -> bool {
        self.reapply != Reapply::Ignore
    }

    /// The stacks left after a duration ran out with `current` stacks;
    /// 0 ends the state.
    pub fn stacks_after_expiry(&self, current: u16) -> u16 {
        match self.decay {
            Decay::LoseAll => 0,
            Decay::LoseOne => current.saturating_sub(1),
        }
    }

    /// The value of `effect` with `stacks` stacks, saturating: 0 below its
    /// `min_stacks`.
    pub fn effect_value(effect: &StackEffect, stacks: u16) -> i32 {
        if stacks < effect.min_stacks {
            return 0;
        }
        if effect.flat {
            return effect.per_stack;
        }
        effect.per_stack.saturating_mul(i32::from(stacks))
    }
}

/// All the state definitions of a mod, by state id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StateRules {
    defs: Vec<StateDef>,
}

impl StateRules {
    /// No state has rules: every state plays as the tables say.
    pub fn none() -> StateRules {
        StateRules::default()
    }

    /// `true` when no state has rules.
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// The definitions, in file order.
    pub fn defs(&self) -> &[StateDef] {
        &self.defs
    }

    /// Marks the states in `ids` as watched ([`StateDef::watched`]); ids
    /// without rules are skipped.
    pub fn watch(&mut self, ids: &[u16]) {
        for d in self.defs.iter_mut().filter(|d| ids.contains(&d.id)) {
            d.watched = true;
        }
    }

    /// The rules of the state row `id`.
    pub fn get(&self, id: i32) -> Option<&StateDef> {
        let id = u16::try_from(id).ok()?;
        self.defs.iter().find(|d| d.id == id)
    }

    /// Reads a `states.toml`. A wrong value is an error naming the setting
    /// (`state[1].max_stacks`).
    pub fn from_toml(text: &str) -> Result<StateRules, StatesError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            #[serde(default = "one")]
            version: u32,
            #[serde(default, rename = "state")]
            states: Vec<StateDef>,
        }
        let file: File = toml::from_str(text)?;
        if file.version != VERSION {
            return Err(StatesError::Setting(format!(
                "`version`: {} is not supported (this build reads version {VERSION})",
                file.version
            )));
        }
        for (i, d) in file.states.iter().enumerate() {
            check(i, d)?;
            if file.states[..i].iter().any(|o| o.id == d.id) {
                return Err(bad(i, "id", format!("state {} is defined twice", d.id)));
            }
        }
        Ok(StateRules { defs: file.states })
    }
}

fn bad(i: usize, key: &str, msg: impl std::fmt::Display) -> StatesError {
    StatesError::Setting(format!("`state[{i}].{key}`: {msg}"))
}

fn check(i: usize, d: &StateDef) -> Result<(), StatesError> {
    if d.max_stacks == 0 || d.max_stacks > MAX_STACKS {
        return Err(bad(
            i,
            "max_stacks",
            format!("{} is not between 1 and {MAX_STACKS}", d.max_stacks),
        ));
    }
    if let Some(s) = d.duration_seconds {
        if s == 0 || s > MAX_SECONDS {
            return Err(bad(
                i,
                "duration_seconds",
                format!("{s} is not between 1 and {MAX_SECONDS}"),
            ));
        }
    }
    if d.decay == Decay::LoseOne && d.duration_seconds.is_none() {
        // Without a duration of its own the state would be the skill's, and
        // a stack that loses one at a time needs a clock the file controls.
        return Err(bad(
            i,
            "decay",
            "`lose_one` needs `duration_seconds` (the time each stack lasts)",
        ));
    }
    for (j, e) in d.effects.iter().enumerate() {
        if e.min_stacks == 0 || u32::from(e.min_stacks) > d.max_stacks {
            return Err(bad(
                i,
                &format!("effect[{j}].min_stacks"),
                format!(
                    "{} is not between 1 and the state's max_stacks ({})",
                    e.min_stacks, d.max_stacks
                ),
            ));
        }
    }
    if d.icon.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(bad(i, "icon", "the name is empty"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
