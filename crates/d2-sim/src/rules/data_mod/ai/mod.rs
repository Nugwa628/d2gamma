//! Monster AI as data: a mod's `files/ai.toml`.
//!
//! A mod gives a monster a behaviour graph instead of one of D2Gamma's
//! built-in AIs. The graph is a handful of named *states*; in each state a
//! monster has *transitions* (leave for another state when conditions hold)
//! and *actions* (what it does on its turn). Monsters pick a graph through
//! the `AI` column of their `monstats` row: a graph with `index = 200` is
//! the AI of every monster whose row says 200. A graph with the index of one
//! of D2Gamma's own AIs replaces that AI for the mod; every other monster
//! keeps running the AI it has today.
//!
//! ```toml
//! version = 1
//!
//! [[ai]]
//! index = 200                    # the monstats AI value this graph answers
//! name = "Stalker"               # shown by tools; the game ignores it
//! target = "enemy"               # "enemy" (default), "any" or "none"
//! start = "stalk"                # default: the first state
//!
//! [[ai.cooldown]]                # a timer an action can start and a condition can read
//! name = "leap"
//! seconds = 6
//!
//! [[ai.state]]
//! name = "stalk"
//!
//! [[ai.state.transition]]        # checked in order, first that holds wins
//! to = "strike"
//! when = [{ kind = "distance_below", value = 3 }]
//!
//! [[ai.state.action]]            # tried in order; the first one that acts ends the turn
//! kind = "use_skill"
//! slot = 2                       # the monster's Skill2 column
//! when = [{ kind = "cooldown_ready", name = "leap" }, { kind = "distance_below", value = 14 }]
//! starts_cooldown = "leap"
//!
//! [[ai.state.action]]
//! kind = "move_to"
//! run = true
//! ```
//!
//! # What a turn is
//!
//! A monster *thinks* when its think timer runs out (the same timer the
//! built-in AIs use). On a think the target search has already run, so the
//! graph knows the target, its distance and whether it is in melee range.
//! The graph then:
//!
//! 1. follows the first transition whose conditions all hold (at most
//!    [`MAX_HOPS`] in one think, so a ring of states cannot spin forever),
//! 2. goes through the new state's actions in order. An action whose `when`
//!    conditions do not hold, or that cannot be done right now (no target, no
//!    such skill), is passed over. *Instant* actions (`call_allies`,
//!    `summon`, `set_state`) happen and the walk through the list goes on.
//!    The first other action that acts is the monster's turn and ends the
//!    think,
//! 3. if no action acted, the monster idles for its normal think delay.
//!
//! # Wind-ups
//!
//! `wind_up` is a turn that takes `seconds`: the monster stands still, the
//! graph reports a [`crate::monsters::ai::graph::GraphEventKind::WindUp`]
//! event (what a telegraph effect or sound will hang on) and the actions
//! after it are held back until the time is over. Put the strike after it:
//! `wind_up` then `use_skill`. A state change cancels a running wind-up.
//!
//! # Bounds
//!
//! A file has at most [`MAX_AIS`] graphs; a graph at most [`MAX_STATES`]
//! states, [`MAX_COOLDOWNS`] cooldowns and [`MAX_EVENTS`] wind-up event
//! names; a state at most [`MAX_TRANSITIONS`] transitions and
//! [`MAX_ACTIONS`] actions; and a transition or action at most
//! [`MAX_CONDITIONS`] conditions. One think looks at no more than
//! [`THINK_BUDGET`] conditions and actions, which a test checks against the
//! largest graph the limits allow. All draws come from the monster's own
//! seed, so a replay plays the same game.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};

use crate::monsters::ai::graph::MAX_COOLDOWNS as RUN_COOLDOWNS;

mod preset;
mod run;
pub mod trace;
mod vocab;

pub use preset::{find as find_preset, presets, Knob, Preset};
pub use vocab::{
    action_kind, condition_kind, Field, FieldKind, KindInfo, ACTIONS, CONDITIONS, TARGETS,
};

/// The one version of `ai.toml` this build reads.
pub const VERSION: u32 = 1;
/// Most graphs in a file.
pub const MAX_AIS: usize = 32;
/// Most states in a graph.
pub const MAX_STATES: usize = 24;
/// Most transitions in a state.
pub const MAX_TRANSITIONS: usize = 8;
/// Most actions in a state.
pub const MAX_ACTIONS: usize = 8;
/// Most conditions on a transition or an action.
pub const MAX_CONDITIONS: usize = 6;
/// Most cooldowns a graph declares.
pub const MAX_COOLDOWNS: usize = RUN_COOLDOWNS;
/// Most wind-up event names in a graph.
pub const MAX_EVENTS: usize = 8;
/// Most transitions one think follows, and most phases it fires.
pub const MAX_HOPS: usize = 4;
/// Most phases in a graph.
pub const MAX_PHASES: usize = 8;
/// The most conditions and actions one think may look at: the phase checks,
/// the hops' transition checks, plus one walk through the actions of the
/// state it ends in.
pub const THINK_BUDGET: u32 = (MAX_HOPS * MAX_CONDITIONS
    + MAX_HOPS * MAX_TRANSITIONS * MAX_CONDITIONS
    + MAX_ACTIONS * (MAX_CONDITIONS + 1)) as u32;
/// The farthest distance, in subtiles, a setting may name.
const MAX_DISTANCE: i32 = 200;
/// The most subtiles a move or a spread may cover.
const MAX_STEPS: i32 = 40;
/// The most a `move_to` may ask to speed up or slow down, in percent.
const MAX_SPEED: i32 = 100;
/// The `skill1`..`skill8` columns of a monster row.
pub const SKILL_SLOTS: u8 = 8;
/// Most monsters one `summon` creates.
pub const MAX_SUMMON: u8 = 4;
/// The longest time the file may state: ten minutes.
const MAX_SECONDS: f64 = 600.0;
/// Base ticks per second, the unit durations are kept in.
const TICKS_PER_SECOND: f64 = 25.0;

/// Think ids of the graphs: the id of graph `i` is `THINK_BASE + i`. They
/// sit far above anything the built-in AIs use.
pub const THINK_BASE: u32 = 0x4000_0000;
/// Init ids of the graphs (`INIT_BASE + i`).
pub const INIT_BASE: u32 = 0x4100_0000;

/// An `ai.toml` that cannot be used. A setting error names the entry and the
/// key (`ai[0].state[1].action[2].slot`).
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("invalid ai.toml: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("invalid ai.toml setting: {0}")]
    Setting(String),
}

/// Whom the target search looks for before the graph thinks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Targeting {
    /// The monster has no target and the graph always thinks.
    None,
    /// The nearest enemy it can see; when there is none the monster waits
    /// and the graph does not think.
    #[default]
    Enemy,
    /// As `enemy`, but the graph also thinks when there is no target (to
    /// wander, call for help or guard a spot).
    Any,
}

impl Targeting {
    /// The target mode of the AI table row this stands for.
    pub fn mode(self) -> u32 {
        match self {
            Targeting::None => 0,
            Targeting::Enemy => 1,
            Targeting::Any => 2,
        }
    }
}

/// Whose side a condition or action looks at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Whose {
    /// The monster itself.
    #[default]
    #[serde(rename = "self")]
    Itself,
    /// The monster's target.
    Target,
    /// The monster's second target: the nearest enemy it sees, which the
    /// built-in AIs often act on instead of the main target.
    Second,
    /// The last fallen undead found by the undead scan (see
    /// [`Condition::CorpseWithin`]), which a skill can raise.
    Corpse,
    /// The last hurt undead found by the undead scan.
    Hurt,
    /// The monster's owner: the unit it is a minion of (a pet's master).
    Owner,
    /// The enemy the owner is fighting: what the owner's own target search
    /// finds, as the built-in pet AIs read it.
    OwnerTarget,
}

/// A thing that must hold.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    HasTarget,
    NoTarget,
    /// The target is closer than `value` subtiles (the distance the built-in
    /// AIs use, body sizes taken off).
    DistanceBelow {
        value: i32,
    },
    /// The target is `value` subtiles or farther.
    DistanceAbove {
        value: i32,
    },
    /// The target is within the monster's melee reach (the "combat range"
    /// the built-in AIs test: there is no separate condition for it).
    InMelee,
    /// The target is not within the monster's melee reach (the opposite of
    /// [`Condition::InMelee`]; it holds with no target too), the "not in
    /// combat" the built-in AIs test first.
    OutOfMelee,
    /// The monster has an owner (see [`Whose::Owner`]).
    HasOwner,
    /// The monster has no owner.
    NoOwner,
    /// The owner is closer than `value` subtiles.
    OwnerDistanceBelow {
        value: i32,
    },
    /// The owner is `value` subtiles or farther.
    OwnerDistanceAbove {
        value: i32,
    },
    /// The owner has an enemy in sight (see [`Whose::OwnerTarget`]).
    OwnerTarget,
    /// The monster has a second target (see [`Whose::Second`]).
    SecondTarget,
    /// The second target is closer than `value` subtiles.
    SecondDistanceBelow {
        value: i32,
    },
    /// The second target is `value` subtiles or farther.
    SecondDistanceAbove {
        value: i32,
    },
    /// At least `at_least` undead monsters of the monster's side (not
    /// itself) are within `radius` subtiles.
    UndeadWithin {
        radius: i32,
        #[serde(default = "one_u8")]
        at_least: u8,
    },
    /// An undead monster of the monster's side lies fallen (and can be
    /// raised) within `radius` subtiles.
    CorpseWithin {
        radius: i32,
    },
    /// An undead monster of the monster's side is hurt (below its full
    /// life) within `radius` subtiles.
    HurtWithin {
        radius: i32,
    },
    /// Fewer than `count` living monsters of the family of `class` (and of
    /// `also`, if given; a family is the monstats `BaseId`) are in the
    /// rooms near the monster.
    ClassCountBelow {
        class: u16,
        #[serde(default)]
        also: Option<u16>,
        count: u8,
    },
    /// At least `count` living monsters of the family of `class` (and of
    /// `also`) are in the rooms near the monster.
    ClassCountAtLeast {
        class: u16,
        #[serde(default)]
        also: Option<u16>,
        count: u8,
    },
    /// The monster has thought at least `count` times since it entered this
    /// state (this think included).
    ThinksAtLeast {
        count: u16,
    },
    LineOfSight,
    NoLineOfSight,
    /// Life is below `percent` of its maximum.
    LifeBelow {
        percent: u8,
    },
    /// Life is at or above `percent` of its maximum.
    LifeAbove {
        percent: u8,
    },
    HasState {
        state: u16,
        #[serde(default)]
        who: Whose,
    },
    LacksState {
        state: u16,
        #[serde(default)]
        who: Whose,
    },
    /// The state is on with at least `count` stacks.
    StacksAtLeast {
        state: u16,
        count: u16,
        #[serde(default)]
        who: Whose,
    },
    /// At least `at_least` living members of the monster's pack (its leader
    /// and the leader's minions, not itself) are within `radius` subtiles.
    AlliesWithin {
        radius: i32,
        #[serde(default = "one_u8")]
        at_least: u8,
    },
    /// At least `at_least` players (or their allies) are within `radius`
    /// subtiles.
    EnemiesWithin {
        radius: i32,
        #[serde(default = "one_u8")]
        at_least: u8,
    },
    /// The cooldown has run out (or was never started).
    CooldownReady {
        name: String,
        #[serde(skip)]
        index: u8,
    },
    /// The monster has had this graph for at least `seconds` (since it
    /// spawned, or since a phase switched to this graph).
    AliveFor {
        #[serde(rename = "seconds", deserialize_with = "ticks")]
        ticks: i32,
    },
    /// The monster has been in this state for at least `seconds`.
    InStateFor {
        #[serde(rename = "seconds", deserialize_with = "ticks")]
        ticks: i32,
    },
    /// A roll on the monster's own seed comes up under `percent`.
    Random {
        percent: u8,
    },
}

fn one_u8() -> u8 {
    1
}

/// What an action does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    /// Walk (or `run = true`) toward the target. `steps` stops that many
    /// steps short of it (default 1: all the way), `speed` asks for a
    /// faster (or, below 0, slower) walk, in percent.
    MoveTo,
    /// Walk one hop toward the target, of at most `steps` subtiles (default
    /// 12), keeping `keep` subtiles from it (default 0), and think again
    /// after it: the way the built-in AIs close in. Backs off when nearer
    /// than `keep`.
    HopTo,
    /// Stay with the owner the way the built-in pets do: keep up, give way
    /// to its other pets, and catch up when left behind. Does nothing (and
    /// lets the actions after it go on) when the pet is close to its owner
    /// and has an enemy to deal with, or, with `quiet`, when it is already
    /// where it should be. `steps` is how far from the owner it may stray
    /// (default 7); `at` is the enemy it has in mind (default `target`).
    FollowOwner,
    /// Walk or run `steps` subtiles straight away from the target.
    MoveAway,
    /// As `move_away`, running, with a long default distance.
    Flee,
    /// Circle the target, `steps` at a time.
    MoveAround,
    /// A melee swing: `attack = 1` (default) or `2`.
    Attack,
    /// Use the monster's skill in column `slot` (1 to 8) on the target (or
    /// on itself with `at = "self"`).
    UseSkill,
    /// Walk to a random spot within `radius` subtiles.
    Wander,
    /// Stand still for `seconds` (default: the monster's normal think delay).
    Idle,
    /// Stand still for `seconds` and report the `event`, holding back the
    /// actions after it until the time is over.
    WindUp,
    /// Wake the monster's pack mates within `radius` so they join in. Instant.
    CallAllies,
    /// Create `count` monsters of class `class` near the monster, as its
    /// minions. Instant.
    Summon,
    /// Turn a state on or off (`on`, default `true`) on the monster. Instant.
    SetState,
}

impl ActionKind {
    /// An instant action happens and lets the walk through the list go on.
    pub fn is_instant(self) -> bool {
        matches!(
            self,
            ActionKind::CallAllies | ActionKind::Summon | ActionKind::SetState
        )
    }

    /// The keys besides `kind`, `when` and `starts_cooldown` this kind reads.
    fn keys(self) -> &'static [&'static str] {
        match self {
            ActionKind::MoveTo => &["run", "steps", "speed", "at", "radius"],
            ActionKind::HopTo => &["steps", "keep", "speed", "at", "radius"],
            ActionKind::FollowOwner => &["steps", "at", "quiet"],
            ActionKind::MoveAway => &["run", "steps"],
            ActionKind::Flee => &["steps"],
            ActionKind::MoveAround => &["steps"],
            ActionKind::Attack => &["attack", "at", "radius"],
            ActionKind::UseSkill => &["slot", "at", "radius"],
            ActionKind::Wander => &["radius"],
            ActionKind::Idle => &["seconds"],
            ActionKind::WindUp => &["seconds", "event"],
            ActionKind::CallAllies => &["radius"],
            ActionKind::Summon => &["class", "count", "spread"],
            ActionKind::SetState => &["state", "on"],
        }
    }

    /// The keys this kind needs.
    fn needs(self) -> &'static [&'static str] {
        match self {
            ActionKind::UseSkill => &["slot"],
            ActionKind::WindUp => &["seconds"],
            ActionKind::Summon => &["class"],
            ActionKind::SetState => &["state"],
            _ => &[],
        }
    }
}

/// One action of a state. Which of the optional keys matter depends on the
/// `kind` (see [`ActionKind`]); a key the kind does not read is an error.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDef {
    pub kind: ActionKind,
    /// All must hold for the action to be tried.
    #[serde(default)]
    pub when: Vec<Condition>,
    /// The cooldown this action starts when it acts.
    #[serde(default)]
    pub starts_cooldown: Option<String>,
    #[serde(default, skip_deserializing)]
    pub cooldown: Option<u8>,
    #[serde(default)]
    pub run: Option<bool>,
    #[serde(default)]
    pub steps: Option<i32>,
    #[serde(default)]
    pub speed: Option<i32>,
    #[serde(default)]
    pub keep: Option<i32>,
    #[serde(default)]
    pub quiet: Option<bool>,
    #[serde(default)]
    pub attack: Option<u8>,
    #[serde(default)]
    pub slot: Option<u8>,
    #[serde(default)]
    pub at: Option<Whose>,
    #[serde(default)]
    pub radius: Option<i32>,
    #[serde(default, rename = "seconds", deserialize_with = "opt_ticks")]
    pub ticks: Option<i32>,
    #[serde(default)]
    pub event: Option<String>,
    #[serde(default, skip_deserializing)]
    pub event_index: Option<u8>,
    #[serde(default)]
    pub class: Option<u16>,
    #[serde(default)]
    pub count: Option<u8>,
    #[serde(default)]
    pub spread: Option<i32>,
    #[serde(default)]
    pub state: Option<u16>,
    #[serde(default)]
    pub on: Option<bool>,
}

/// A way out of a state.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionDef {
    /// The state to go to.
    pub to: String,
    #[serde(default, skip_deserializing)]
    pub target: u8,
    /// All must hold for the transition to be followed.
    #[serde(default)]
    pub when: Vec<Condition>,
}

/// A cooldown a graph declares.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CooldownDef {
    pub name: String,
    #[serde(rename = "seconds", deserialize_with = "ticks")]
    pub ticks: i32,
}

/// One state of a graph.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateDef {
    pub name: String,
    #[serde(default, rename = "transition")]
    pub transitions: Vec<TransitionDef>,
    #[serde(default, rename = "action")]
    pub actions: Vec<ActionDef>,
}

/// A boss phase: when its conditions first hold, the graph changes state or
/// the monster switches to another graph, and the monster can be immune to
/// crowd control for a while. Phases fire in order, each at most once.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseDef {
    pub name: String,
    /// All must hold for the phase to start. Only the next phase is looked
    /// at, so a later phase waits for the earlier ones.
    #[serde(default)]
    pub when: Vec<Condition>,
    /// The state of this graph to go to.
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default, skip_deserializing)]
    pub state_index: Option<u8>,
    /// Or the `index` of another graph of the file to switch to.
    #[serde(default)]
    pub graph: Option<u16>,
    /// Where `graph` is in the file.
    #[serde(default, skip_deserializing)]
    pub graph_position: Option<usize>,
    /// For this long the monster takes no stun, freeze, interrupt or
    /// knockback (the mod's `immune_state` in `crowd_control.toml`).
    #[serde(default, rename = "immune_seconds", deserialize_with = "opt_ticks")]
    pub immune_ticks: Option<i32>,
}

/// A preset parameter's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Param {
    /// The same for every difficulty.
    All(i64),
    /// Normal, Nightmare and Hell.
    Each([i64; 3]),
}

/// One entry of the file: a behaviour graph, or a preset with its parameters
/// changed.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiDef {
    /// The `AI` value of the monstats rows this graph answers.
    pub index: u16,
    /// A label for tools and messages; the game ignores it.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub target: Option<Targeting>,
    /// One of D2Gamma's AIs (see [`presets`]) the monsters run instead of a
    /// graph, with `params` changed. A preset entry has no states.
    #[serde(default)]
    pub preset: Option<String>,
    /// The preset's parameters to change, by knob name or `aip1` to `aip8`:
    /// one number for all difficulties or `[Normal, Nightmare, Hell]`.
    #[serde(default)]
    pub params: BTreeMap<String, Param>,
    /// The AI table row of `preset`.
    #[serde(default, skip_deserializing)]
    pub preset_row: Option<u16>,
    /// `params` by parameter (`aip1` is entry 0): the value for each
    /// difficulty, or `None` to leave the monstats column.
    #[serde(default, skip_deserializing)]
    pub knobs: [Option<[i32; 3]>; 8],
    /// The state a monster starts in; default: the first.
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default, skip_deserializing)]
    pub start_state: u8,
    #[serde(default, rename = "cooldown")]
    pub cooldowns: Vec<CooldownDef>,
    #[serde(default, rename = "state")]
    pub states: Vec<StateDef>,
    /// The boss phases, in the order they happen.
    #[serde(default, rename = "phase")]
    pub phases: Vec<PhaseDef>,
    /// The wind-up event names, in the order they first appear.
    #[serde(default, skip_deserializing)]
    pub events: Vec<String>,
}

/// All the graphs of a mod, in file order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiRules {
    defs: Vec<AiDef>,
}

impl AiDef {
    /// Whether the entry is a preset with parameters rather than a graph.
    pub fn is_preset(&self) -> bool {
        self.preset_row.is_some()
    }

    /// The target mode of the entry's graph (`enemy` unless it says).
    pub fn target_or_default(&self) -> Targeting {
        self.target.unwrap_or_default()
    }

    /// The value of parameter `n` (1 to 8) on `difficulty` (0 to 2) when the
    /// entry sets it.
    pub fn param(&self, n: usize, difficulty: u8) -> Option<i32> {
        let each = (*self.knobs.get(n.checked_sub(1)?)?)?;
        each.get(usize::from(difficulty).min(2)).copied()
    }
}

impl AiRules {
    /// No graphs.
    pub fn none() -> AiRules {
        AiRules::default()
    }

    /// `true` when the mod has no graph.
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// The graphs, in file order.
    pub fn defs(&self) -> &[AiDef] {
        &self.defs
    }

    /// The graph for `AI` value `index`.
    pub fn get(&self, index: u16) -> Option<&AiDef> {
        self.position(index).map(|i| &self.defs[i])
    }

    fn position(&self, index: u16) -> Option<usize> {
        self.defs.iter().position(|d| d.index == index)
    }

    /// Reads an `ai.toml`. A wrong value is an error naming the entry and
    /// the key (`ai[1].state[0].action[2].slot`).
    pub fn from_toml(text: &str) -> Result<AiRules, AiError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct File {
            #[serde(default = "file_version")]
            version: u32,
            #[serde(default, rename = "ai")]
            ais: Vec<toml::Value>,
        }
        let file: File = toml::from_str(text)?;
        if file.version != VERSION {
            return Err(AiError::Setting(format!(
                "`version`: {} is not supported (this build reads version {VERSION})",
                file.version
            )));
        }
        if file.ais.len() > MAX_AIS {
            return Err(AiError::Setting(format!(
                "`ai`: {} graphs; a mod has at most {MAX_AIS}",
                file.ais.len()
            )));
        }
        let mut defs: Vec<AiDef> = Vec::with_capacity(file.ais.len());
        for (i, value) in file.ais.into_iter().enumerate() {
            let mut d = AiDef::deserialize(value)
                .map_err(|e| AiError::Setting(format!("`ai[{i}]`: {}", e.message())))?;
            if defs.iter().any(|o| o.index == d.index) {
                return Err(bad(
                    i,
                    "index",
                    format!("{} is used by another graph", d.index),
                ));
            }
            resolve(i, &mut d)?;
            defs.push(d);
        }
        for i in 0..defs.len() {
            for j in 0..defs[i].phases.len() {
                let Some(target) = defs[i].phases[j].graph else {
                    continue;
                };
                let position = defs
                    .iter()
                    .position(|o| o.index == target && !o.is_preset());
                defs[i].phases[j].graph_position = Some(position.ok_or_else(|| {
                    bad(
                        i,
                        &format!("phase[{j}].graph"),
                        format!("there is no graph with `index = {target}` in this file"),
                    )
                })?);
            }
        }
        Ok(AiRules { defs })
    }

    /// The name of state `state` of graph `ai`, for tools and logs.
    pub fn state_name(&self, ai: u16, state: u8) -> Option<&str> {
        Some(&self.get(ai)?.states.get(usize::from(state))?.name)
    }

    /// The name of phase `phase` of graph `ai`.
    pub fn phase_name(&self, ai: u16, phase: u8) -> Option<&str> {
        Some(&self.get(ai)?.phases.get(usize::from(phase))?.name)
    }

    /// Whether any phase asks for an immunity window.
    pub fn wants_immunity(&self) -> Option<(usize, usize)> {
        self.defs.iter().enumerate().find_map(|(i, d)| {
            let j = d.phases.iter().position(|p| p.immune_ticks.is_some())?;
            Some((i, j))
        })
    }

    /// The name of wind-up event `event` of graph `ai`.
    pub fn event_name(&self, ai: u16, event: u8) -> Option<&str> {
        self.get(ai)?
            .events
            .get(usize::from(event))
            .map(String::as_str)
    }
}

fn file_version() -> u32 {
    VERSION
}

fn bad(i: usize, key: &str, msg: impl std::fmt::Display) -> AiError {
    AiError::Setting(format!("`ai[{i}].{key}`: {msg}"))
}

/// Seconds (a whole or a decimal number) as base ticks. The one float step is
/// at load time, IEEE-exact, and leaves an integer: nothing in the sim runs on
/// the float.
#[allow(clippy::float_arithmetic)]
fn to_ticks(seconds: f64) -> Option<i32> {
    if !(0.0..=MAX_SECONDS).contains(&seconds) {
        return None;
    }
    Some((seconds * TICKS_PER_SECOND).round() as i32)
}

fn ticks<'de, D: Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number {
        Int(i64),
        Float(f64),
    }
    let seconds = match Number::deserialize(d)? {
        Number::Int(n) => n as f64,
        Number::Float(f) => f,
    };
    to_ticks(seconds).ok_or_else(|| {
        serde::de::Error::custom(format!(
            "{seconds} seconds is out of range (0 to {MAX_SECONDS} seconds)"
        ))
    })
}

fn opt_ticks<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i32>, D::Error> {
    ticks(d).map(Some)
}

/// Looks every name up, fills in the indices the run time uses and checks
/// the limits.
fn resolve(i: usize, d: &mut AiDef) -> Result<(), AiError> {
    if d.preset.is_some() {
        return resolve_preset(i, d);
    }
    if let Some(key) = d.params.keys().next() {
        return Err(bad(
            i,
            &format!("params.{key}"),
            "parameters belong to a `preset`; a graph has none",
        ));
    }
    if d.states.is_empty() {
        return Err(bad(i, "state", "a graph needs at least one state"));
    }
    if d.states.len() > MAX_STATES {
        return Err(bad(
            i,
            "state",
            format!(
                "{} states; a graph has at most {MAX_STATES}",
                d.states.len()
            ),
        ));
    }
    if d.cooldowns.len() > MAX_COOLDOWNS {
        return Err(bad(
            i,
            "cooldown",
            format!(
                "{} cooldowns; a graph has at most {MAX_COOLDOWNS}",
                d.cooldowns.len()
            ),
        ));
    }
    let names: Vec<String> = d.states.iter().map(|s| s.name.clone()).collect();
    for (j, n) in names.iter().enumerate() {
        if n.is_empty() || names[..j].contains(n) {
            return Err(bad(
                i,
                &format!("state[{j}].name"),
                "needs a name no other state has",
            ));
        }
    }
    let cooldowns: Vec<String> = d.cooldowns.iter().map(|c| c.name.clone()).collect();
    for (j, n) in cooldowns.iter().enumerate() {
        if n.is_empty() || cooldowns[..j].contains(n) {
            return Err(bad(
                i,
                &format!("cooldown[{j}].name"),
                "needs a name no other cooldown has",
            ));
        }
    }
    d.start_state = match &d.start {
        None => 0,
        Some(s) => index_of(&names, s).ok_or_else(|| bad(i, "start", no_state(s)))?,
    };
    let mut events: Vec<String> = Vec::new();
    for (j, s) in d.states.iter_mut().enumerate() {
        let at = |key: &str| format!("state[{j}].{key}");
        if s.transitions.len() > MAX_TRANSITIONS {
            return Err(bad(
                i,
                &at("transition"),
                format!(
                    "{} transitions; a state has at most {MAX_TRANSITIONS}",
                    s.transitions.len()
                ),
            ));
        }
        if s.actions.len() > MAX_ACTIONS {
            return Err(bad(
                i,
                &at("action"),
                format!(
                    "{} actions; a state has at most {MAX_ACTIONS}",
                    s.actions.len()
                ),
            ));
        }
        for (k, t) in s.transitions.iter_mut().enumerate() {
            let key = |what: &str| at(&format!("transition[{k}].{what}"));
            t.target =
                index_of(&names, &t.to).ok_or_else(|| bad(i, &key("to"), no_state(&t.to)))?;
            if usize::from(t.target) == j {
                return Err(bad(
                    i,
                    &key("to"),
                    "a transition cannot lead to its own state",
                ));
            }
            check_conditions(i, &key("when"), &mut t.when, &cooldowns)?;
        }
        for (k, a) in s.actions.iter_mut().enumerate() {
            let key = |what: &str| at(&format!("action[{k}].{what}"));
            check_conditions(i, &key("when"), &mut a.when, &cooldowns)?;
            check_action(i, &key, a)?;
            if let Some(name) = &a.starts_cooldown {
                a.cooldown = Some(index_of(&cooldowns, name).ok_or_else(|| {
                    bad(
                        i,
                        &key("starts_cooldown"),
                        format!("there is no cooldown `{name}`"),
                    )
                })?);
            }
            if let Some(name) = &a.event {
                let found = match events.iter().position(|e| e == name) {
                    Some(p) => p,
                    None => {
                        events.push(name.clone());
                        events.len() - 1
                    }
                };
                if events.len() > MAX_EVENTS {
                    return Err(bad(
                        i,
                        &key("event"),
                        format!("more than {MAX_EVENTS} different event names in one graph"),
                    ));
                }
                a.event_index = Some(found as u8);
            }
        }
    }
    d.events = events;
    resolve_phases(i, d, &names, &cooldowns)
}

fn resolve_phases(
    i: usize,
    d: &mut AiDef,
    states: &[String],
    cooldowns: &[String],
) -> Result<(), AiError> {
    if d.phases.len() > MAX_PHASES {
        return Err(bad(
            i,
            "phase",
            format!(
                "{} phases; a graph has at most {MAX_PHASES}",
                d.phases.len()
            ),
        ));
    }
    let names: Vec<String> = d.phases.iter().map(|p| p.name.clone()).collect();
    for (j, p) in d.phases.iter_mut().enumerate() {
        let key = |what: &str| format!("phase[{j}].{what}");
        if p.name.is_empty() || names[..j].contains(&p.name) {
            return Err(bad(i, &key("name"), "needs a name no other phase has"));
        }
        check_conditions(i, &key("when"), &mut p.when, cooldowns)?;
        if p.state.is_some() && p.graph.is_some() {
            return Err(bad(
                i,
                &key("graph"),
                "a phase goes to a `state` or a `graph`, not both",
            ));
        }
        if let Some(name) = &p.state {
            p.state_index =
                Some(index_of(states, name).ok_or_else(|| bad(i, &key("state"), no_state(name)))?);
        }
        if p.graph == Some(d.index) {
            return Err(bad(
                i,
                &key("graph"),
                "this is the graph itself; use `state` to change state",
            ));
        }
    }
    Ok(())
}

/// The lowest and highest value a preset parameter can take: the monstats
/// columns are read as 16-bit signed numbers.
const PARAM_RANGE: std::ops::RangeInclusive<i64> = -32768..=32767;

/// Looks the preset and its parameters up.
fn resolve_preset(i: usize, d: &mut AiDef) -> Result<(), AiError> {
    let name = d.preset.as_deref().unwrap_or_default();
    let preset = find_preset(name).ok_or_else(|| {
        bad(
            i,
            "preset",
            format!("there is no AI preset called `{name}` (docs/modkit/ai.md lists them)"),
        )
    })?;
    let graph_key = if !d.states.is_empty() {
        Some("state")
    } else if !d.cooldowns.is_empty() {
        Some("cooldown")
    } else if d.start.is_some() {
        Some("start")
    } else if !d.phases.is_empty() {
        Some("phase")
    } else if d.target.is_some() {
        Some("target")
    } else {
        None
    };
    if let Some(key) = graph_key {
        return Err(bad(
            i,
            key,
            "a preset entry has only `index`, `name`, `preset` and `params`; drop `preset` to write a graph",
        ));
    }
    let mut knobs = [None; 8];
    for (field, value) in &d.params {
        let aip = preset.aip_of(field).ok_or_else(|| {
            bad(
                i,
                &format!("params.{field}"),
                format!(
                    "`{}` has no parameter `{field}` (it has {})",
                    preset.name,
                    preset.field_list()
                ),
            )
        })?;
        let each = match *value {
            Param::All(v) => [v; 3],
            Param::Each(v) => v,
        };
        let mut out = [0; 3];
        for (slot, v) in out.iter_mut().zip(each) {
            if !PARAM_RANGE.contains(&v) {
                return Err(bad(
                    i,
                    &format!("params.{field}"),
                    format!("{v} is out of range (-32768 to 32767)"),
                ));
            }
            *slot = v as i32;
        }
        let slot = &mut knobs[usize::from(aip) - 1];
        if slot.is_some() {
            return Err(bad(
                i,
                &format!("params.{field}"),
                format!("`aip{aip}` is set twice (by its name and by `aip{aip}`)"),
            ));
        }
        *slot = Some(out);
    }
    d.preset_row = Some(preset.index);
    d.knobs = knobs;
    Ok(())
}

fn no_state(name: &str) -> String {
    format!("there is no state `{name}` in this graph")
}

fn index_of(names: &[String], name: &str) -> Option<u8> {
    names.iter().position(|n| n == name).map(|p| p as u8)
}

fn check_conditions(
    i: usize,
    key: &str,
    list: &mut [Condition],
    cooldowns: &[String],
) -> Result<(), AiError> {
    if list.len() > MAX_CONDITIONS {
        return Err(bad(
            i,
            key,
            format!("{} conditions; at most {MAX_CONDITIONS}", list.len()),
        ));
    }
    for (n, c) in list.iter_mut().enumerate() {
        let here = format!("{key}[{n}]");
        match c {
            Condition::CooldownReady { name, index } => {
                *index = index_of(cooldowns, name).ok_or_else(|| {
                    bad(
                        i,
                        &format!("{here}.name"),
                        format!("there is no cooldown `{name}`"),
                    )
                })?;
            }
            Condition::LifeBelow { percent }
            | Condition::LifeAbove { percent }
            | Condition::Random { percent } => {
                if *percent > 100 {
                    return Err(bad(
                        i,
                        &format!("{here}.percent"),
                        "is a percentage, 0 to 100",
                    ));
                }
            }
            Condition::DistanceBelow { value }
            | Condition::DistanceAbove { value }
            | Condition::SecondDistanceBelow { value }
            | Condition::SecondDistanceAbove { value }
            | Condition::OwnerDistanceBelow { value }
            | Condition::OwnerDistanceAbove { value } => {
                if !(0..=MAX_DISTANCE).contains(value) {
                    return Err(bad(
                        i,
                        &format!("{here}.value"),
                        format!("0 to {MAX_DISTANCE} subtiles"),
                    ));
                }
            }
            Condition::CorpseWithin { radius } | Condition::HurtWithin { radius } => {
                if !(1..=MAX_DISTANCE).contains(radius) {
                    return Err(bad(
                        i,
                        &format!("{here}.radius"),
                        format!("1 to {MAX_DISTANCE} subtiles"),
                    ));
                }
            }
            Condition::ClassCountBelow { count, .. }
            | Condition::ClassCountAtLeast { count, .. } => {
                if *count == 0 {
                    return Err(bad(i, &format!("{here}.count"), "is 1 or more"));
                }
            }
            Condition::ThinksAtLeast { count: 0 } => {
                return Err(bad(i, &format!("{here}.count"), "is 1 or more"));
            }
            Condition::AlliesWithin { radius, at_least }
            | Condition::EnemiesWithin { radius, at_least }
            | Condition::UndeadWithin { radius, at_least } => {
                if !(1..=MAX_DISTANCE).contains(radius) {
                    return Err(bad(
                        i,
                        &format!("{here}.radius"),
                        format!("1 to {MAX_DISTANCE} subtiles"),
                    ));
                }
                if *at_least == 0 {
                    return Err(bad(i, &format!("{here}.at_least"), "is 1 or more"));
                }
            }
            Condition::StacksAtLeast { count: 0, .. } => {
                return Err(bad(i, &format!("{here}.count"), "is 1 or more"));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Checks the keys of one action against its kind and bounds the values.
fn check_action(i: usize, key: &dyn Fn(&str) -> String, a: &ActionDef) -> Result<(), AiError> {
    let set: [(&str, bool); 14] = [
        ("run", a.run.is_some()),
        ("steps", a.steps.is_some()),
        ("speed", a.speed.is_some()),
        ("keep", a.keep.is_some()),
        ("attack", a.attack.is_some()),
        ("slot", a.slot.is_some()),
        ("at", a.at.is_some()),
        ("radius", a.radius.is_some()),
        ("seconds", a.ticks.is_some()),
        ("event", a.event.is_some()),
        ("class", a.class.is_some()),
        ("count", a.count.is_some()),
        ("spread", a.spread.is_some()),
        ("state", a.state.is_some()),
    ];
    let kind = format!("{:?}", a.kind);
    for (name, present) in set {
        if present && !a.kind.keys().contains(&name) {
            return Err(bad(
                i,
                &key(name),
                format!("`{name}` does not apply to {kind}"),
            ));
        }
    }
    if a.on.is_some() && !a.kind.keys().contains(&"on") {
        return Err(bad(i, &key("on"), format!("`on` does not apply to {kind}")));
    }
    if a.quiet.is_some() && !a.kind.keys().contains(&"quiet") {
        return Err(bad(
            i,
            &key("quiet"),
            format!("`quiet` does not apply to {kind}"),
        ));
    }
    for name in a.kind.needs() {
        if !set.iter().any(|(n, present)| n == name && *present) {
            return Err(bad(i, &key(name), format!("{kind} needs `{name}`")));
        }
    }
    if let Some(n) = a.steps {
        if !(1..=MAX_STEPS).contains(&n) {
            return Err(bad(i, &key("steps"), format!("1 to {MAX_STEPS}")));
        }
    }
    if a.speed
        .is_some_and(|n| n == 0 || !(-MAX_SPEED..=MAX_SPEED).contains(&n))
    {
        return Err(bad(
            i,
            &key("speed"),
            format!("-{MAX_SPEED} to {MAX_SPEED} percent, not 0"),
        ));
    }
    if a.keep.is_some_and(|n| !(0..=MAX_DISTANCE).contains(&n)) {
        return Err(bad(
            i,
            &key("keep"),
            format!("0 to {MAX_DISTANCE} subtiles"),
        ));
    }
    if a.at == Some(Whose::Itself) && a.kind != ActionKind::UseSkill {
        return Err(bad(
            i,
            &key("at"),
            format!("`self` does not apply to {kind}"),
        ));
    }
    if a.attack.is_some_and(|n| !(1..=2).contains(&n)) {
        return Err(bad(i, &key("attack"), "is 1 or 2"));
    }
    if a.slot.is_some_and(|n| !(1..=SKILL_SLOTS).contains(&n)) {
        return Err(bad(i, &key("slot"), format!("1 to {SKILL_SLOTS}")));
    }
    if let Some(n) = a.radius {
        if !(1..=MAX_DISTANCE).contains(&n) {
            return Err(bad(
                i,
                &key("radius"),
                format!("1 to {MAX_DISTANCE} subtiles"),
            ));
        }
    }
    if a.count.is_some_and(|n| !(1..=MAX_SUMMON).contains(&n)) {
        return Err(bad(i, &key("count"), format!("1 to {MAX_SUMMON}")));
    }
    if a.spread.is_some_and(|n| !(0..=MAX_STEPS).contains(&n)) {
        return Err(bad(i, &key("spread"), format!("0 to {MAX_STEPS}")));
    }
    if a.kind == ActionKind::WindUp && a.event.as_deref().is_some_and(str::is_empty) {
        return Err(bad(i, &key("event"), "needs a name"));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
