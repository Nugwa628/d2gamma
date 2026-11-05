//! The run-time state a data-driven monster AI keeps for each monster, and
//! the events it reports.
//!
//! A mod's behaviour graph (`files/ai.toml`, read by
//! `rules::data_mod::ai`) is plain data and shared by every monster that
//! uses it. What differs from monster to monster is small: which graph state
//! it is in, since when, which cooldowns are running, and whether it is in
//! the middle of a wind-up. That is [`GraphRun`], and it lives in the
//! monster's [`super::MonsterAi`] next to the rest of its think state. A
//! monster that runs one of D2Gamma's own AIs never has one.
//!
//! The graph tells the rest of the game what it is doing through
//! [`GraphEvent`]s: a state change, the start of a wind-up (the telegraph a
//! screen effect or sound will hang on later), a phase change. They are
//! collected in [`GraphEvents`], bounded so a long game cannot grow it
//! without end; whoever drains them (the server, the live arena, a test)
//! takes them with [`GraphEvents::take`].

use crate::units::UnitId;

/// Most cooldowns one graph declares.
pub const MAX_COOLDOWNS: usize = 8;

/// Most events kept before the oldest are dropped.
pub const MAX_EVENTS: usize = 256;

/// A wind-up in progress: the action of the state that started it and the
/// frame it is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindUp {
    pub action: u8,
    pub until: i32,
}

/// One monster's place in its behaviour graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphRun {
    /// The graph (an `index` of `ai.toml`) this was made for. A monster
    /// whose AI changes starts afresh.
    pub ai: u16,
    /// The state, as an index into the graph's states.
    pub state: u8,
    /// The frame the state was entered.
    pub entered: i32,
    /// How many times the monster has thought since it entered the state
    /// (the think under way included).
    pub thinks: u16,
    /// The frame the monster got this graph.
    pub born: i32,
    /// For each declared cooldown, the frame it is ready again (all ready
    /// when the graph starts).
    pub ready_at: [i32; MAX_COOLDOWNS],
    pub windup: Option<WindUp>,
    /// How many of the graph's phases have started.
    pub phase: u8,
}

impl GraphRun {
    /// A monster just given the graph `ai`, in graph state `state`.
    pub fn new(ai: u16, state: u8, frame: i32) -> GraphRun {
        GraphRun {
            ai,
            state,
            entered: frame,
            thinks: 0,
            born: frame,
            ready_at: [frame; MAX_COOLDOWNS],
            windup: None,
            phase: 0,
        }
    }
}

/// What happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphEventKind {
    /// The monster moved from graph state `from` to `to`.
    StateChanged { from: u8, to: u8 },
    /// A wind-up began: its `event` (an index into the graph's event
    /// names) lasts `ticks` base ticks.
    WindUp { event: u8, ticks: i32 },
    /// A boss phase began (an index into the graph's phases).
    PhaseChanged { phase: u8 },
}

/// An event of one monster's graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphEvent {
    pub frame: i32,
    pub unit: UnitId,
    /// The graph, as an `index` of `ai.toml`.
    pub ai: u16,
    pub kind: GraphEventKind,
}

/// The events not yet taken, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphEvents {
    list: Vec<GraphEvent>,
}

impl GraphEvents {
    /// Adds an event, dropping the oldest when [`MAX_EVENTS`] are waiting.
    pub fn push(&mut self, event: GraphEvent) {
        if self.list.len() >= MAX_EVENTS {
            self.list.remove(0);
        }
        self.list.push(event);
    }

    /// The events waiting, oldest first.
    pub fn peek(&self) -> &[GraphEvent] {
        &self.list
    }

    /// Takes every waiting event.
    pub fn take(&mut self) -> Vec<GraphEvent> {
        std::mem::take(&mut self.list)
    }
}
