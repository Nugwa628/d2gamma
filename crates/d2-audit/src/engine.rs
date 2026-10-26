//! The replay seam: an engine re-runs one game from its journal (N17, the
//! journal crate's replay engine on D2RUST's own game build) and tells the
//! audit what it found. The audit decides what that means: which saves
//! the main server stored differ, which characters were affected, a cheat
//! or a determinism bug.

use std::path::Path;

/// A save's digest: SHA-256 of the record with its timestamps zeroed, the
/// same digest the journal's `Save` records carry.
pub type Digest = [u8; 32];

/// What a character joined a game with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Joined {
    /// Its stored record.
    Record(Vec<u8>),
    /// A character never played before: the game made it.
    New {
        class: u8,
        hardcore: bool,
        progression: u8,
    },
}

/// One seat of the game to re-run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobSeat {
    pub lease: u64,
    pub character: String,
    pub joined: Joined,
}

/// What an engine is asked to re-run.
#[derive(Clone, Copy, Debug)]
pub struct ReplayJob<'a> {
    pub game: u32,
    /// The game's journal, `data/journals/<day>/<game>.d2j.gz`.
    pub journal: &'a Path,
    /// The characters the game's `Join` records name, by lease.
    pub seats: &'a [JobSeat],
}

/// When a seat was in the game, from the journal's `Join` and `Leave`
/// records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeatSpan {
    pub lease: u64,
    pub joined: u32,
    /// `None`: still in the game at its end.
    pub left: Option<u32>,
}

/// A save the re-run game made again and that matched the journal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemadeSave {
    pub tick: u32,
    pub lease: u64,
    pub seq: u32,
    pub digest: Digest,
}

/// Where the re-run first differed from the journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// The game differs at or before this tick.
    pub tick: u32,
    /// The last tick where the state still matched.
    pub last_good: u32,
    /// What differed, in words.
    pub what: String,
}

/// A player's journal intents differ from the digests their client sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentFault {
    /// The first differing digest span, from 0.
    pub span: u32,
    /// The journal tick of the span's first intent, if there is one.
    pub tick: Option<u32>,
}

/// The intent check of one player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentResult {
    pub lease: u64,
    /// `None`: every digest matched.
    pub fault: Option<IntentFault>,
}

/// What a re-run found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayOutcome {
    /// Journal records run.
    pub records: u64,
    /// Host frames run.
    pub frames: u64,
    /// The game tick at the end.
    pub last_tick: u32,
    /// State hashes compared and equal.
    pub state_hashes: u32,
    /// The journal ended with its `End` record.
    pub ended: bool,
    pub seats: Vec<SeatSpan>,
    pub saves: Vec<RemadeSave>,
    pub divergence: Option<Divergence>,
    pub intents: Vec<IntentResult>,
}

/// Re-runs a game. `Err` means the audit could not run (the journal is
/// missing or doesn't read, another build or other game data): the audit is
/// marked as an error and the journal kept.
pub trait ReplayEngine {
    fn replay(&self, job: &ReplayJob<'_>) -> Result<ReplayOutcome, String>;
}

/// The engine of a build without one: every run is an error that says so.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoEngine;

impl ReplayEngine for NoEngine {
    fn replay(&self, _: &ReplayJob<'_>) -> Result<ReplayOutcome, String> {
        Err("this build has no replay engine (the journal replay, N17)".into())
    }
}

/// An engine that answers with a set outcome per game (tests, and the MP
/// test tool until the journal replay is wired in).
#[derive(Clone, Debug, Default)]
pub struct ScriptedEngine {
    pub outcomes: std::collections::BTreeMap<u32, Result<ReplayOutcome, String>>,
}

impl ReplayEngine for ScriptedEngine {
    fn replay(&self, job: &ReplayJob<'_>) -> Result<ReplayOutcome, String> {
        self.outcomes
            .get(&job.game)
            .cloned()
            .unwrap_or_else(|| Err(format!("no outcome set for game {}", job.game)))
    }
}
