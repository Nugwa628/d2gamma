//! Audits on the main server (plan `multiplayer-v2` sections C4-C6, modules
//! N18 and N20): which games are re-run from their journals, what a re-run
//! found, and the rollbacks that follow.
//!
//! - [`config`]: the audit settings of the main server's config, with the
//!   plan's proposed defaults as constants.
//! - [`sample`]: the spot checks (a share of official games, a share of the
//!   other players in player-hosted games), keyed by a realm secret so a game
//!   server can't tell which games will be audited.
//! - [`engine`]: the [`engine::ReplayEngine`] seam. The journal crate's
//!   replay engine (N17) re-runs a game; this crate only reads its outcome.
//! - [`Audits`]: the audit queue in SQLite (spot checks, on demand, the
//!   host's own character, reports, saves flagged by the save checks), the
//!   runs and their findings, journal retention and holds, rollback to the
//!   record a character joined the failed game with, its undo, and the
//!   determinism watch (a failed audit of an official game is a determinism
//!   bug, never a cheat).
//! - [`report`]: the audit report, `data/audits/<game>.md`.
//!
//! Every call takes the time as Unix milliseconds, so tests and the main
//! server's worker choose it. The characters themselves are not this
//! crate's: a rollback writes them through [`CharacterStore`], which the main
//! server implements over its character database.

pub mod config;
pub mod engine;
pub mod report;
pub mod sample;

mod db;
mod rollback;
mod run;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub use config::{AuditConfig, RollbackMode};
pub use db::{
    AuditId, AuditRow, AuditState, FindingId, FindingKind, FindingRow, FindingState, GameId,
    GameRow, ReportRow, RollbackId, RollbackRow, RollbackState, SeatRow, Watch,
};
pub use engine::{Joined, ReplayEngine};
pub use rollback::CharacterStore;
pub use run::RunResult;

use rusqlite::Connection;

/// An audit failure.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AuditError {
    /// The database failed.
    #[error("audit database: {0}")]
    Db(String),
    /// Writing a report or deleting a journal failed.
    #[error("{0}")]
    Io(String),
    /// The game, audit, finding or rollback isn't known.
    #[error("{0} not found")]
    NotFound(String),
    /// The request doesn't fit the state it names (for example undoing a
    /// rollback that was never applied).
    #[error("{0}")]
    State(String),
    /// The character store refused (the character is in a game, or gone).
    #[error("character store: {0}")]
    Store(String),
}

impl From<rusqlite::Error> for AuditError {
    fn from(e: rusqlite::Error) -> Self {
        AuditError::Db(e.to_string())
    }
}

/// Where a game ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameKind {
    /// On an official game server (the owner's).
    Official,
    /// On a player's PC.
    PlayerHosted,
}

/// Why an audit was queued. One audit may have several.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Reason {
    /// The owner asked (`d2rust-audit queue`, `--audit-game`).
    OnDemand = 1,
    /// A player reported the game (`ReportGame`).
    Report = 2,
    /// A save failed or was flagged by a save check.
    SaveFlagged = 3,
    /// The host's own character in a player-hosted game: every game.
    HostCharacter = 4,
    /// The spot check of an official game.
    SpotOfficial = 5,
    /// The spot check of another player's character in a player-hosted game.
    SpotHosted = 6,
}

impl Reason {
    /// All reasons, in the order of their numbers.
    pub const ALL: [Reason; 6] = [
        Reason::OnDemand,
        Reason::Report,
        Reason::SaveFlagged,
        Reason::HostCharacter,
        Reason::SpotOfficial,
        Reason::SpotHosted,
    ];

    /// The reason of number `n`.
    pub fn from_u8(n: u8) -> Option<Reason> {
        Reason::ALL.into_iter().find(|r| *r as u8 == n)
    }

    /// The reason's name, as reports and the tool print it.
    pub fn name(self) -> &'static str {
        match self {
            Reason::OnDemand => "on demand",
            Reason::Report => "player report",
            Reason::SaveFlagged => "save check",
            Reason::HostCharacter => "host's own character",
            Reason::SpotOfficial => "spot check (official)",
            Reason::SpotHosted => "spot check (player-hosted)",
        }
    }

    /// Audits asked for by someone run before the standing checks.
    fn priority(self) -> u8 {
        match self {
            Reason::OnDemand | Reason::Report | Reason::SaveFlagged => 0,
            Reason::HostCharacter | Reason::SpotOfficial | Reason::SpotHosted => 1,
        }
    }
}

/// A game as the main server starts it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameStart {
    pub game: GameId,
    /// The mod realm (`d2rust`, `pd2`, ...).
    pub realm: String,
    /// The game server's id.
    pub node: String,
    pub kind: GameKind,
}

/// A seat as the main server offered it: the character and the record it
/// joins the game with (kept for the replay and for a rollback).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatJoin {
    pub lease: u64,
    pub character: String,
    /// The host's own character in a player-hosted game.
    pub host: bool,
    pub joined: Joined,
}

/// The audit service: the queue, runs, findings and rollbacks of one main
/// server, in one SQLite file.
pub struct Audits {
    conn: Mutex<Connection>,
    cfg: AuditConfig,
    reports: PathBuf,
}

impl Audits {
    /// Opens (creating if needed) the audit database at `db` and writes
    /// reports into `reports` (`data/audits`).
    pub fn open(db: &Path, reports: &Path, cfg: AuditConfig) -> Result<Self, AuditError> {
        let conn = db::open(Connection::open(db)?)?;
        Ok(Self {
            conn: Mutex::new(conn),
            cfg,
            reports: reports.to_owned(),
        })
    }

    /// A database in memory (tests), with reports into `reports`.
    pub fn in_memory(reports: &Path, cfg: AuditConfig) -> Result<Self, AuditError> {
        let conn = db::open(Connection::open_in_memory()?)?;
        Ok(Self {
            conn: Mutex::new(conn),
            cfg,
            reports: reports.to_owned(),
        })
    }

    /// The settings in use.
    pub fn config(&self) -> &AuditConfig {
        &self.cfg
    }

    /// Where reports are written.
    pub fn reports_dir(&self) -> &Path {
        &self.reports
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A panic while the lock was held leaves no half-done transaction
        // (each one commits or rolls back), so the connection is still good.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }
}
