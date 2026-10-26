//! The audit database: one SQLite file in WAL mode beside the accounts
//! database. It holds the games the main server ran (with the records their
//! characters joined with, for the journal window), the stored saves'
//! digests, the audit queue and its reasons, findings, rollbacks and player
//! reports. Journals themselves are files (plan C2).

use std::path::PathBuf;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::engine::{Digest, Joined};
use crate::sample::{self, SampleKey};
use crate::{AuditError, Audits, GameKind, GameStart, Reason, SeatJoin};

/// The schema version in `PRAGMA user_version`.
const SCHEMA: i64 = 1;

const CREATE: &str = "
CREATE TABLE IF NOT EXISTS meta(
    key TEXT PRIMARY KEY,
    value BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS games(
    id INTEGER PRIMARY KEY,
    realm TEXT NOT NULL,
    node TEXT NOT NULL,
    kind INTEGER NOT NULL,
    journal TEXT NOT NULL,
    started INTEGER NOT NULL,
    ended INTEGER,
    held INTEGER NOT NULL DEFAULT 0,
    pruned INTEGER
);
CREATE TABLE IF NOT EXISTS seats(
    game INTEGER NOT NULL REFERENCES games(id),
    lease INTEGER NOT NULL,
    character TEXT NOT NULL,
    host INTEGER NOT NULL,
    record BLOB,
    new_char BLOB,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY(game, lease)
);
CREATE INDEX IF NOT EXISTS seats_character ON seats(character);
CREATE TABLE IF NOT EXISTS saves(
    game INTEGER NOT NULL REFERENCES games(id),
    lease INTEGER NOT NULL,
    seq INTEGER NOT NULL,
    tick INTEGER NOT NULL,
    digest BLOB NOT NULL,
    PRIMARY KEY(game, lease, seq)
);
CREATE TABLE IF NOT EXISTS audits(
    id INTEGER PRIMARY KEY,
    game INTEGER NOT NULL REFERENCES games(id),
    state INTEGER NOT NULL,
    priority INTEGER NOT NULL,
    queued INTEGER NOT NULL,
    finished INTEGER,
    detail TEXT NOT NULL DEFAULT ''
);
CREATE UNIQUE INDEX IF NOT EXISTS audits_open ON audits(game) WHERE state IN (0, 1);
CREATE TABLE IF NOT EXISTS audit_reasons(
    audit INTEGER NOT NULL REFERENCES audits(id),
    reason INTEGER NOT NULL,
    lease INTEGER,
    detail TEXT NOT NULL,
    at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS findings(
    id INTEGER PRIMARY KEY,
    audit INTEGER NOT NULL REFERENCES audits(id),
    game INTEGER NOT NULL,
    kind INTEGER NOT NULL,
    tick INTEGER,
    last_good INTEGER,
    lease INTEGER,
    detail TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS rollbacks(
    id INTEGER PRIMARY KEY,
    audit INTEGER NOT NULL REFERENCES audits(id),
    game INTEGER NOT NULL,
    lease INTEGER NOT NULL,
    character TEXT NOT NULL,
    state INTEGER NOT NULL,
    created INTEGER NOT NULL,
    before BLOB,
    after_digest BLOB,
    applied INTEGER,
    undone INTEGER
);
CREATE TABLE IF NOT EXISTS reports(
    id INTEGER PRIMARY KEY,
    game INTEGER NOT NULL REFERENCES games(id),
    reporter TEXT NOT NULL,
    reason INTEGER NOT NULL,
    text TEXT NOT NULL,
    at INTEGER NOT NULL,
    UNIQUE(game, reporter)
);
";

pub(crate) fn open(conn: Connection) -> Result<Connection, AuditError> {
    conn.busy_timeout(Duration::from_secs(5))?;
    // An in-memory database answers "memory"; a file one "wal".
    let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    conn.execute_batch("PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > SCHEMA {
        return Err(AuditError::Db(format!(
            "schema version {version} is newer than this tool's {SCHEMA}"
        )));
    }
    conn.execute_batch(CREATE)?;
    conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA}"))?;
    let key: Option<Vec<u8>> = conn
        .query_row("SELECT value FROM meta WHERE key = 'sample_key'", [], |r| {
            r.get(0)
        })
        .optional()?;
    if key.is_none() {
        conn.execute(
            "INSERT INTO meta(key, value) VALUES ('sample_key', ?1)",
            [sample::new_key().to_vec()],
        )?;
    }
    Ok(conn)
}

/// A game id (`u32 game` of the link messages).
pub type GameId = u32;
/// An audit's number.
pub type AuditId = i64;
/// A finding's number.
pub type FindingId = i64;
/// A rollback's number.
pub type RollbackId = i64;

/// Where an audit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditState {
    /// Waiting (for its turn, or for the game to end).
    Queued = 0,
    Running = 1,
    /// The re-run matched everywhere.
    Passed = 2,
    /// The re-run found a difference: see the findings.
    Failed = 3,
    /// The audit could not run (see its detail); the journal is kept.
    Error = 4,
}

impl AuditState {
    fn from_i64(n: i64) -> AuditState {
        match n {
            0 => AuditState::Queued,
            1 => AuditState::Running,
            2 => AuditState::Passed,
            3 => AuditState::Failed,
            _ => AuditState::Error,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            AuditState::Queued => "queued",
            AuditState::Running => "running",
            AuditState::Passed => "passed",
            AuditState::Failed => "failed",
            AuditState::Error => "error",
        }
    }

    /// The state of `name`, as [`AuditState::name`] prints it.
    pub fn parse(name: &str) -> Option<AuditState> {
        [
            AuditState::Queued,
            AuditState::Running,
            AuditState::Passed,
            AuditState::Failed,
            AuditState::Error,
        ]
        .into_iter()
        .find(|s| s.name() == name)
    }
}

/// What an audit found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingKind {
    /// A player-hosted game's re-run differs from its journal.
    Divergence = 1,
    /// An official game's re-run differs: a determinism bug, not a cheat.
    Determinism = 2,
    /// A save the main server stored differs from the one the re-run made.
    StoredSave = 3,
    /// A player's journal intents differ from their client's digests.
    Intents = 4,
}

impl FindingKind {
    fn from_i64(n: i64) -> FindingKind {
        match n {
            2 => FindingKind::Determinism,
            3 => FindingKind::StoredSave,
            4 => FindingKind::Intents,
            _ => FindingKind::Divergence,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FindingKind::Divergence => "divergence",
            FindingKind::Determinism => "determinism bug",
            FindingKind::StoredSave => "stored save differs",
            FindingKind::Intents => "intents differ from digests",
        }
    }
}

/// Where a finding stands: derived from its audit's rollbacks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingState {
    /// Rollbacks wait for the owner, or none were made yet.
    Open,
    /// Its rollbacks were applied.
    RolledBack,
    /// The owner declined its rollbacks, or undid them.
    Dismissed,
    /// In an official game: a bug to fix (determinism, saves or digests),
    /// never a rollback.
    Bug,
}

/// Where a rollback is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollbackState {
    /// Waiting for the owner.
    Proposed = 0,
    Applied = 1,
    Undone = 2,
    Declined = 3,
}

impl RollbackState {
    fn from_i64(n: i64) -> RollbackState {
        match n {
            1 => RollbackState::Applied,
            2 => RollbackState::Undone,
            3 => RollbackState::Declined,
            _ => RollbackState::Proposed,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RollbackState::Proposed => "proposed",
            RollbackState::Applied => "applied",
            RollbackState::Undone => "undone",
            RollbackState::Declined => "declined",
        }
    }
}

/// A game's row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameRow {
    pub id: GameId,
    pub realm: String,
    pub node: String,
    pub kind: GameKind,
    pub journal: PathBuf,
    pub started: u64,
    pub ended: Option<u64>,
    /// The owner, or a failed or errored audit, holds the journal.
    pub held: bool,
    /// When the journal and join records were deleted.
    pub pruned: Option<u64>,
}

/// A seat's row: the character and what it joined with (`None` once the
/// journal window is over).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatRow {
    pub game: GameId,
    pub lease: u64,
    pub character: String,
    pub host: bool,
    pub joined: Option<Joined>,
    pub joined_at: u64,
}

/// An audit's row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditRow {
    pub id: AuditId,
    pub game: GameId,
    pub kind: GameKind,
    pub state: AuditState,
    pub queued: u64,
    pub finished: Option<u64>,
    /// Why it was queued: (reason, lease it names, words).
    pub reasons: Vec<(Reason, Option<u64>, String)>,
    /// Why it errored, or a one-line result.
    pub detail: String,
}

/// A finding's row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingRow {
    pub id: FindingId,
    pub audit: AuditId,
    pub game: GameId,
    pub kind: FindingKind,
    pub tick: Option<u32>,
    pub last_good: Option<u32>,
    pub lease: Option<u64>,
    pub detail: String,
    pub state: FindingState,
}

/// A rollback's row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RollbackRow {
    pub id: RollbackId,
    pub audit: AuditId,
    pub game: GameId,
    pub lease: u64,
    pub character: String,
    pub state: RollbackState,
    pub created: u64,
    pub applied: Option<u64>,
    pub undone: Option<u64>,
}

/// A player's report of a game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportRow {
    pub id: i64,
    pub game: GameId,
    pub reporter: String,
    /// `ReportGame.reason`: 1 cheating, 2 abuse, 3 broken, 4 other.
    pub reason: u8,
    pub text: String,
    pub at: u64,
}

/// The determinism watch: official games re-run, and how many matched
/// (should stay at all of them).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Watch {
    /// Finished audits of official games (passed or failed).
    pub official_audited: u64,
    pub official_matched: u64,
    /// Of those, the spot checks.
    pub spot_audited: u64,
    pub spot_matched: u64,
    /// The official games whose re-run differed, newest first.
    pub determinism_bugs: Vec<GameId>,
}

impl Watch {
    /// The share of official audits that matched, in percent (100 when none
    /// ran).
    pub fn matched_pct(&self) -> f64 {
        if self.official_audited == 0 {
            100.0
        } else {
            self.official_matched as f64 * 100.0 / self.official_audited as f64
        }
    }
}

fn to_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

fn to_u64(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

fn kind_of(n: i64) -> GameKind {
    if n == 1 {
        GameKind::PlayerHosted
    } else {
        GameKind::Official
    }
}

fn kind_num(k: GameKind) -> i64 {
    match k {
        GameKind::Official => 0,
        GameKind::PlayerHosted => 1,
    }
}

fn joined_cols(j: &Joined) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    match j {
        Joined::Record(r) => (Some(r.clone()), None),
        Joined::New {
            class,
            hardcore,
            progression,
        } => (None, Some(vec![*class, u8::from(*hardcore), *progression])),
    }
}

fn joined_of(record: Option<Vec<u8>>, new_char: Option<Vec<u8>>) -> Option<Joined> {
    match (record, new_char) {
        (Some(r), _) => Some(Joined::Record(r)),
        (None, Some(n)) if n.len() == 3 => Some(Joined::New {
            class: n[0],
            hardcore: n[1] != 0,
            progression: n[2],
        }),
        _ => None,
    }
}

pub(crate) fn game_row(r: &Row) -> rusqlite::Result<GameRow> {
    let journal: String = r.get(4)?;
    Ok(GameRow {
        id: r.get(0)?,
        realm: r.get(1)?,
        node: r.get(2)?,
        kind: kind_of(r.get(3)?),
        journal: PathBuf::from(journal),
        started: to_u64(r.get(5)?),
        ended: r.get::<_, Option<i64>>(6)?.map(to_u64),
        held: r.get::<_, i64>(7)? != 0,
        pruned: r.get::<_, Option<i64>>(8)?.map(to_u64),
    })
}

pub(crate) const GAME_COLUMNS: &str =
    "id, realm, node, kind, journal, started, ended, held, pruned";

fn seat_row(r: &Row) -> rusqlite::Result<SeatRow> {
    Ok(SeatRow {
        game: r.get(0)?,
        lease: to_u64(r.get(1)?),
        character: r.get(2)?,
        host: r.get::<_, i64>(3)? != 0,
        joined: joined_of(r.get(4)?, r.get(5)?),
        joined_at: to_u64(r.get(6)?),
    })
}

const SEAT_COLUMNS: &str = "game, lease, character, host, record, new_char, joined_at";

fn rollback_row(r: &Row) -> rusqlite::Result<RollbackRow> {
    Ok(RollbackRow {
        id: r.get(0)?,
        audit: r.get(1)?,
        game: r.get(2)?,
        lease: to_u64(r.get(3)?),
        character: r.get(4)?,
        state: RollbackState::from_i64(r.get(5)?),
        created: to_u64(r.get(6)?),
        applied: r.get::<_, Option<i64>>(7)?.map(to_u64),
        undone: r.get::<_, Option<i64>>(8)?.map(to_u64),
    })
}

pub(crate) const ROLLBACK_COLUMNS: &str =
    "id, audit, game, lease, character, state, created, applied, undone";

pub(crate) fn get_game(c: &Connection, game: GameId) -> Result<GameRow, AuditError> {
    c.query_row(
        &format!("SELECT {GAME_COLUMNS} FROM games WHERE id = ?1"),
        [game],
        game_row,
    )
    .optional()?
    .ok_or_else(|| AuditError::NotFound(format!("game {game}")))
}

pub(crate) fn seats_of(c: &Connection, game: GameId) -> Result<Vec<SeatRow>, AuditError> {
    let mut st = c.prepare(&format!(
        "SELECT {SEAT_COLUMNS} FROM seats WHERE game = ?1 ORDER BY joined_at, lease"
    ))?;
    let rows = st.query_map([game], seat_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub(crate) fn stored_saves(
    c: &Connection,
    game: GameId,
) -> Result<Vec<(u64, u32, u32, Digest)>, AuditError> {
    let mut st = c.prepare(
        "SELECT lease, seq, tick, digest FROM saves WHERE game = ?1 ORDER BY tick, lease, seq",
    )?;
    let rows = st.query_map([game], |r| {
        let d: Vec<u8> = r.get(3)?;
        let mut digest = [0u8; 32];
        if d.len() == 32 {
            digest.copy_from_slice(&d);
        }
        Ok((to_u64(r.get(0)?), r.get(1)?, r.get(2)?, digest))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn sample_key(c: &Connection) -> Result<SampleKey, AuditError> {
    let v: Vec<u8> = c.query_row("SELECT value FROM meta WHERE key = 'sample_key'", [], |r| {
        r.get(0)
    })?;
    v.try_into()
        .map_err(|_| AuditError::Db("the sampling key is damaged".into()))
}

/// Adds `reason` to the game's open audit, queueing one if there is none.
pub(crate) fn queue(
    c: &Connection,
    game: &GameRow,
    reason: Reason,
    lease: Option<u64>,
    detail: &str,
    now: u64,
) -> Result<AuditId, AuditError> {
    if game.pruned.is_some() {
        return Err(AuditError::State(format!(
            "game {}: its journal was deleted after the journal window",
            game.id
        )));
    }
    let open: Option<(AuditId, i64)> = c
        .query_row(
            "SELECT id, priority FROM audits WHERE game = ?1 AND state IN (0, 1)",
            [game.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let prio = i64::from(reason.priority());
    let id = match open {
        Some((id, p)) => {
            if prio < p {
                c.execute(
                    "UPDATE audits SET priority = ?2 WHERE id = ?1",
                    params![id, prio],
                )?;
            }
            id
        }
        None => {
            c.execute(
                "INSERT INTO audits(game, state, priority, queued) VALUES (?1, 0, ?2, ?3)",
                params![game.id, prio, to_i64(now)],
            )?;
            c.last_insert_rowid()
        }
    };
    c.execute(
        "INSERT INTO audit_reasons(audit, reason, lease, detail, at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, reason as i64, lease.map(to_i64), detail, to_i64(now)],
    )?;
    Ok(id)
}

pub(crate) fn audit_row(c: &Connection, id: AuditId) -> Result<AuditRow, AuditError> {
    let (game, state, queued, finished, detail): (GameId, i64, i64, Option<i64>, String) = c
        .query_row(
            "SELECT game, state, queued, finished, detail FROM audits WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?
        .ok_or_else(|| AuditError::NotFound(format!("audit {id}")))?;
    let kind = get_game(c, game)?.kind;
    let mut st = c.prepare(
        "SELECT reason, lease, detail FROM audit_reasons WHERE audit = ?1 ORDER BY rowid",
    )?;
    let reasons = st
        .query_map([id], |r| {
            Ok((
                r.get::<_, u8>(0)?,
                r.get::<_, Option<i64>>(1)?.map(to_u64),
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|(n, l, d)| Reason::from_u8(n).map(|r| (r, l, d)))
        .collect();
    Ok(AuditRow {
        id,
        game,
        kind,
        state: AuditState::from_i64(state),
        queued: to_u64(queued),
        finished: finished.map(to_u64),
        reasons,
        detail,
    })
}

pub(crate) fn findings_where(
    c: &Connection,
    clause: &str,
    arg: i64,
) -> Result<Vec<FindingRow>, AuditError> {
    let mut st = c.prepare(&format!(
        "SELECT id, audit, game, kind, tick, last_good, lease, detail FROM findings {clause} \
         ORDER BY id DESC"
    ))?;
    let rows = st
        .query_map([arg], |r| {
            Ok((
                r.get::<_, FindingId>(0)?,
                r.get::<_, AuditId>(1)?,
                r.get::<_, GameId>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<u32>>(4)?,
                r.get::<_, Option<u32>>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out = Vec::with_capacity(rows.len());
    for (id, audit, game, kind, tick, last_good, lease, detail) in rows {
        let kind = FindingKind::from_i64(kind);
        let official = get_game(c, game)?.kind == GameKind::Official;
        let state = if official || kind == FindingKind::Determinism {
            FindingState::Bug
        } else {
            let states: Vec<RollbackState> = rollbacks_where(c, "WHERE audit = ?1", audit)?
                .into_iter()
                .map(|r| r.state)
                .collect();
            if states.contains(&RollbackState::Applied) {
                FindingState::RolledBack
            } else if !states.is_empty() && !states.contains(&RollbackState::Proposed) {
                FindingState::Dismissed
            } else {
                FindingState::Open
            }
        };
        out.push(FindingRow {
            id,
            audit,
            game,
            kind,
            tick,
            last_good,
            lease: lease.map(to_u64),
            detail,
            state,
        });
    }
    Ok(out)
}

pub(crate) fn rollbacks_where(
    c: &Connection,
    clause: &str,
    arg: i64,
) -> Result<Vec<RollbackRow>, AuditError> {
    let mut st = c.prepare(&format!(
        "SELECT {ROLLBACK_COLUMNS} FROM rollbacks {clause} ORDER BY id"
    ))?;
    let rows = st.query_map([arg], rollback_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Text cut to at most `max` bytes on a character boundary.
fn cut(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

impl Audits {
    /// The main server started a game (`StartGame`). An official game is
    /// spot-checked here; its audit waits for the game's end.
    pub fn game_started(
        &self,
        g: &GameStart,
        journal: &std::path::Path,
        now: u64,
    ) -> Result<(), AuditError> {
        let c = self.conn();
        c.execute(
            "INSERT INTO games(id, realm, node, kind, journal, started) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                g.game,
                g.realm,
                g.node,
                kind_num(g.kind),
                journal.to_string_lossy(),
                to_i64(now)
            ],
        )?;
        if self.cfg.on
            && g.kind == GameKind::Official
            && sample::official_game(&sample_key(&c)?, g.game, self.cfg.spot_official_pct)
        {
            let row = get_game(&c, g.game)?;
            queue(&c, &row, Reason::SpotOfficial, None, "", now)?;
        }
        Ok(())
    }

    /// A character took a seat (`SeatOffer`): its join record is kept for
    /// the replay and a rollback. In a player-hosted game the host's own
    /// character queues an audit, and the others are spot-checked.
    pub fn seat_joined(&self, game: GameId, s: &SeatJoin, now: u64) -> Result<(), AuditError> {
        let c = self.conn();
        let row = get_game(&c, game)?;
        let (record, new_char) = joined_cols(&s.joined);
        c.execute(
            "INSERT OR REPLACE INTO seats(game, lease, character, host, record, new_char, joined_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                game,
                to_i64(s.lease),
                s.character,
                i64::from(s.host),
                record,
                new_char,
                to_i64(now)
            ],
        )?;
        if self.cfg.on && row.kind == GameKind::PlayerHosted {
            if s.host {
                queue(
                    &c,
                    &row,
                    Reason::HostCharacter,
                    Some(s.lease),
                    &s.character,
                    now,
                )?;
            } else if sample::hosted_seat(&sample_key(&c)?, game, s.lease, self.cfg.spot_hosted_pct)
            {
                queue(
                    &c,
                    &row,
                    Reason::SpotHosted,
                    Some(s.lease),
                    &s.character,
                    now,
                )?;
            }
        }
        Ok(())
    }

    /// The main server stored a save of the game (`Save`, accepted): its
    /// digest with timestamps zeroed and the journal tick it names.
    pub fn save_stored(
        &self,
        game: GameId,
        lease: u64,
        seq: u32,
        tick: u32,
        digest: &Digest,
    ) -> Result<(), AuditError> {
        self.conn().execute(
            "INSERT OR REPLACE INTO saves(game, lease, seq, tick, digest) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![game, to_i64(lease), seq, tick, digest.to_vec()],
        )?;
        Ok(())
    }

    /// A save of the game failed or was flagged by save checks (their names).
    pub fn save_flagged(
        &self,
        game: GameId,
        lease: u64,
        checks: &[&str],
        now: u64,
    ) -> Result<AuditId, AuditError> {
        let c = self.conn();
        let row = get_game(&c, game)?;
        queue(
            &c,
            &row,
            Reason::SaveFlagged,
            Some(lease),
            &checks.join(", "),
            now,
        )
    }

    /// The owner asks for an audit of `game` (`by`: who, for the report).
    pub fn request(&self, game: GameId, by: &str, now: u64) -> Result<AuditId, AuditError> {
        let c = self.conn();
        let row = get_game(&c, game)?;
        queue(&c, &row, Reason::OnDemand, None, by, now)
    }

    /// A player reports a game (`ReportGame`): kept with the game, it holds
    /// the journal and queues an audit. A second report of the same game by
    /// the same player replaces the first.
    pub fn report_game(
        &self,
        game: GameId,
        reporter: &str,
        reason: u8,
        text: &str,
        now: u64,
    ) -> Result<AuditId, AuditError> {
        let c = self.conn();
        let row = get_game(&c, game)?;
        let text = cut(text, 200);
        c.execute(
            "INSERT INTO reports(game, reporter, reason, text, at) VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(game, reporter) DO UPDATE SET reason = ?3, text = ?4, at = ?5",
            params![game, reporter, reason, text, to_i64(now)],
        )?;
        queue(&c, &row, Reason::Report, None, reporter, now)
    }

    /// The game ended (`GameClosed`, or its server was lost): its audits
    /// can run, and its journal window starts.
    pub fn game_ended(&self, game: GameId, now: u64) -> Result<(), AuditError> {
        let n = self.conn().execute(
            "UPDATE games SET ended = ?2 WHERE id = ?1 AND ended IS NULL",
            params![game, to_i64(now)],
        )?;
        if n == 0 {
            // Unknown, or ended already: tell an unknown one apart.
            get_game(&self.conn(), game)?;
        }
        Ok(())
    }

    /// Holds the game's journal past its window (`true`), or releases a
    /// hold (`false`; open audits and proposed rollbacks still hold it).
    pub fn hold(&self, game: GameId, on: bool) -> Result<(), AuditError> {
        let c = self.conn();
        get_game(&c, game)?;
        c.execute(
            "UPDATE games SET held = ?2 WHERE id = ?1",
            params![game, i64::from(on)],
        )?;
        Ok(())
    }

    /// Puts audits a stopped process left running back in the queue. The
    /// main server calls it once at start, before its audit worker runs
    /// (not the owner's tool, which may run beside a live worker).
    pub fn recover_interrupted(&self) -> Result<usize, AuditError> {
        Ok(self
            .conn()
            .execute("UPDATE audits SET state = 0 WHERE state = 1", [])?)
    }

    /// A game's row.
    pub fn game(&self, game: GameId) -> Result<GameRow, AuditError> {
        get_game(&self.conn(), game)
    }

    /// A game's seats.
    pub fn seats(&self, game: GameId) -> Result<Vec<SeatRow>, AuditError> {
        seats_of(&self.conn(), game)
    }

    /// One audit.
    pub fn audit(&self, id: AuditId) -> Result<AuditRow, AuditError> {
        audit_row(&self.conn(), id)
    }

    /// Audits, newest first; only those in `state` when given.
    pub fn audits(&self, state: Option<AuditState>) -> Result<Vec<AuditRow>, AuditError> {
        let c = self.conn();
        let ids: Vec<AuditId> = {
            let mut st =
                c.prepare("SELECT id FROM audits WHERE ?1 < 0 OR state = ?1 ORDER BY id DESC")?;
            let rows = st.query_map([state.map_or(-1, |s| s as i64)], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        ids.into_iter().map(|id| audit_row(&c, id)).collect()
    }

    /// The latest audit of a game.
    pub fn audit_of_game(&self, game: GameId) -> Result<AuditRow, AuditError> {
        let c = self.conn();
        let id: AuditId = c
            .query_row(
                "SELECT id FROM audits WHERE game = ?1 ORDER BY id DESC LIMIT 1",
                [game],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| AuditError::NotFound(format!("an audit of game {game}")))?;
        audit_row(&c, id)
    }

    /// All findings, newest first.
    pub fn findings(&self) -> Result<Vec<FindingRow>, AuditError> {
        findings_where(&self.conn(), "WHERE ?1 = ?1", 0)
    }

    /// The findings of one audit.
    pub fn findings_of(&self, audit: AuditId) -> Result<Vec<FindingRow>, AuditError> {
        findings_where(&self.conn(), "WHERE audit = ?1", audit)
    }

    /// All rollbacks, oldest first.
    pub fn rollbacks(&self) -> Result<Vec<RollbackRow>, AuditError> {
        rollbacks_where(&self.conn(), "WHERE ?1 = ?1", 0)
    }

    /// The rollbacks of one audit.
    pub fn rollbacks_of(&self, audit: AuditId) -> Result<Vec<RollbackRow>, AuditError> {
        rollbacks_where(&self.conn(), "WHERE audit = ?1", audit)
    }

    /// The player reports, newest first.
    pub fn reports(&self) -> Result<Vec<ReportRow>, AuditError> {
        let c = self.conn();
        let mut st =
            c.prepare("SELECT id, game, reporter, reason, text, at FROM reports ORDER BY id DESC")?;
        let rows = st.query_map([], |r| {
            Ok(ReportRow {
                id: r.get(0)?,
                game: r.get(1)?,
                reporter: r.get(2)?,
                reason: r.get(3)?,
                text: r.get(4)?,
                at: to_u64(r.get(5)?),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The reports of one game.
    pub fn reports_of(&self, game: GameId) -> Result<Vec<ReportRow>, AuditError> {
        Ok(self
            .reports()?
            .into_iter()
            .filter(|r| r.game == game)
            .collect())
    }

    /// The determinism watch.
    pub fn watch(&self) -> Result<Watch, AuditError> {
        let c = self.conn();
        let count = |sql: &str| -> Result<u64, AuditError> {
            Ok(to_u64(c.query_row(sql, [], |r| r.get::<_, i64>(0))?))
        };
        let official =
            "SELECT COUNT(*) FROM audits a JOIN games g ON g.id = a.game WHERE g.kind = 0";
        let spot =
            "AND EXISTS (SELECT 1 FROM audit_reasons r WHERE r.audit = a.id AND r.reason = 5)";
        let mut st = c.prepare(
            "SELECT a.game FROM audits a JOIN games g ON g.id = a.game \
             WHERE g.kind = 0 AND a.state = 3 ORDER BY a.id DESC",
        )?;
        let bugs = st
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<GameId>, _>>()?;
        Ok(Watch {
            official_audited: count(&format!("{official} AND a.state IN (2, 3)"))?,
            official_matched: count(&format!("{official} AND a.state = 2"))?,
            spot_audited: count(&format!("{official} AND a.state IN (2, 3) {spot}"))?,
            spot_matched: count(&format!("{official} AND a.state = 2 {spot}"))?,
            determinism_bugs: bugs,
        })
    }

    /// Deletes the journals (and join records) of games ended more than the
    /// journal window ago, unless something holds them: the owner's hold, a
    /// failed or errored audit, an open audit, or a proposed rollback.
    /// Returns the games pruned.
    pub fn prune(&self, now: u64) -> Result<Vec<GameId>, AuditError> {
        let c = self.conn();
        let before = to_i64(now.saturating_sub(self.cfg.keep_ms()));
        let games: Vec<GameRow> = {
            let mut st = c.prepare(&format!(
                "SELECT {GAME_COLUMNS} FROM games g WHERE ended IS NOT NULL AND ended <= ?1 \
                 AND pruned IS NULL AND held = 0 \
                 AND NOT EXISTS (SELECT 1 FROM audits a WHERE a.game = g.id AND a.state IN (0, 1)) \
                 AND NOT EXISTS (SELECT 1 FROM rollbacks b WHERE b.game = g.id AND b.state = 0) \
                 ORDER BY id"
            ))?;
            let rows = st.query_map([before], game_row)?;
            rows.collect::<Result<_, _>>()?
        };
        let mut done = Vec::new();
        for g in games {
            match std::fs::remove_file(&g.journal) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(AuditError::Io(format!(
                        "deleting {}: {e}",
                        g.journal.display()
                    )))
                }
            }
            c.execute("UPDATE seats SET record = NULL WHERE game = ?1", [g.id])?;
            c.execute(
                "UPDATE games SET pruned = ?2 WHERE id = ?1",
                params![g.id, to_i64(now)],
            )?;
            done.push(g.id);
        }
        Ok(done)
    }
}
