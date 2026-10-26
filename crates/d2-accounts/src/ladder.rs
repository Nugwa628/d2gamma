//! The ladder's store (lobby contract sections 6 and 10, migration 3):
//! seasons per game, each character's season, and one ladder row per
//! character and season with what ranks it. The rank tables themselves are
//! the realm's, rebuilt from [`Ladder::split`].
//!
//! Season 0 is the non-ladder one: it has no `seasons` row and is never
//! ended. Times are Unix milliseconds.

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::{Db, DbError};
use crate::rules::name_key;

/// Migration 3.
pub(crate) const SCHEMA: &str = "
ALTER TABLE characters ADD COLUMN season INTEGER NOT NULL DEFAULT 0;
CREATE TABLE seasons(game TEXT NOT NULL, id INTEGER NOT NULL, name TEXT NOT NULL, started INTEGER NOT NULL,
                     ended INTEGER, test INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(game, id));
CREATE TABLE ladder(game TEXT NOT NULL, season INTEGER NOT NULL, name_key TEXT NOT NULL,
                    account_id INTEGER NOT NULL, name TEXT NOT NULL, class INTEGER NOT NULL,
                    level INTEGER NOT NULL, experience INTEGER NOT NULL, hardcore INTEGER NOT NULL,
                    expansion INTEGER NOT NULL, dead INTEGER NOT NULL, reached INTEGER NOT NULL,
                    PRIMARY KEY(game, season, name_key));
CREATE INDEX ladder_rank ON ladder(game, season, hardcore, expansion, experience DESC, reached);
";

/// The non-ladder season.
pub const NON_LADDER: u16 = 0;

/// A ladder season of one game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Season {
    pub id: u16,
    pub name: String,
    pub started_ms: i64,
    /// `None` while open.
    pub ended_ms: Option<i64>,
    /// A test realm's season.
    pub test: bool,
}

/// One ladder of a season: hardcore or softcore, expansion or classic
/// (the wire's `split`: bit 0 hardcore, bit 1 expansion).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Split {
    pub hardcore: bool,
    pub expansion: bool,
}

impl Split {
    pub fn from_bits(bits: u8) -> Self {
        Self {
            hardcore: bits & 1 != 0,
            expansion: bits & 2 != 0,
        }
    }

    pub fn bits(self) -> u8 {
        u8::from(self.hardcore) | (u8::from(self.expansion) << 1)
    }
}

/// A character's ladder row in one season.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LadderEntry {
    pub season: u16,
    /// As stored, with its case.
    pub name: String,
    pub account: i64,
    pub class: u8,
    pub level: u8,
    pub experience: u64,
    pub split: Split,
    /// A hardcore character that died: it stays, marked.
    pub dead: bool,
    /// When it reached its experience; earlier ranks first on a tie.
    pub reached_ms: i64,
}

/// The ladder of the database ([`Db::ladder`]).
#[derive(Clone, Copy, Debug)]
pub struct Ladder<'a> {
    conn: &'a Connection,
}

impl Db {
    pub fn ladder(&self) -> Ladder<'_> {
        Ladder { conn: &self.conn }
    }
}

const SEASON_COLUMNS: &str = "id, name, started, ended, test";

fn season_of_row(r: &rusqlite::Row) -> rusqlite::Result<Season> {
    Ok(Season {
        id: r.get(0)?,
        name: r.get(1)?,
        started_ms: r.get(2)?,
        ended_ms: r.get(3)?,
        test: r.get(4)?,
    })
}

const ENTRY_COLUMNS: &str =
    "season, name, account_id, class, level, experience, hardcore, expansion, dead, reached";

fn entry_of_row(r: &rusqlite::Row) -> rusqlite::Result<LadderEntry> {
    let experience: i64 = r.get(5)?;
    Ok(LadderEntry {
        season: r.get(0)?,
        name: r.get(1)?,
        account: r.get(2)?,
        class: r.get(3)?,
        level: r.get(4)?,
        experience: u64::try_from(experience).unwrap_or(0),
        split: Split {
            hardcore: r.get(6)?,
            expansion: r.get(7)?,
        },
        dead: r.get(8)?,
        reached_ms: r.get(9)?,
    })
}

impl Ladder<'_> {
    /// The seasons of `game`, newest first (season 0 isn't one).
    pub fn seasons(&self, game: &str) -> Result<Vec<Season>, DbError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {SEASON_COLUMNS} FROM seasons WHERE game = ?1 ORDER BY id DESC"
        ))?;
        let rows = st.query_map([game], season_of_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn season(&self, game: &str, id: u16) -> Result<Option<Season>, DbError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SEASON_COLUMNS} FROM seasons WHERE game = ?1 AND id = ?2"),
                params![game, id],
                season_of_row,
            )
            .optional()?)
    }

    /// The open season of `game`, if one is.
    pub fn current(&self, game: &str) -> Result<Option<Season>, DbError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SEASON_COLUMNS} FROM seasons WHERE game = ?1 AND ended IS NULL"),
                [game],
                season_of_row,
            )
            .optional()?)
    }

    /// Opens the next season of `game` (numbered from 1), ending the open
    /// one at `now_ms`.
    pub fn start_season(
        &self,
        game: &str,
        name: &str,
        test: bool,
        now_ms: i64,
    ) -> Result<Season, DbError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE seasons SET ended = ?2 WHERE game = ?1 AND ended IS NULL",
            params![game, now_ms],
        )?;
        let last: i64 = tx.query_row(
            "SELECT COALESCE(MAX(id), 0) FROM seasons WHERE game = ?1",
            [game],
            |r| r.get(0),
        )?;
        let id = u16::try_from(last + 1)
            .map_err(|_| DbError(format!("game {game:?} has no season numbers left")))?;
        tx.execute(
            "INSERT INTO seasons(game, id, name, started, test) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![game, id, name, now_ms, test],
        )?;
        tx.commit()?;
        Ok(Season {
            id,
            name: name.to_owned(),
            started_ms: now_ms,
            ended_ms: None,
            test,
        })
    }

    /// Ends the open season of `game` at `now_ms`. Returns it, `None` if no
    /// season was open.
    pub fn end_season(&self, game: &str, now_ms: i64) -> Result<Option<Season>, DbError> {
        let Some(mut s) = self.current(game)? else {
            return Ok(None);
        };
        self.conn.execute(
            "UPDATE seasons SET ended = ?3 WHERE game = ?1 AND id = ?2",
            params![game, s.id, now_ms],
        )?;
        s.ended_ms = Some(now_ms);
        Ok(Some(s))
    }

    /// The season of the character `name` (any case); `None` if no such
    /// row.
    pub fn character_season(&self, name: &str) -> Result<Option<u16>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT season FROM characters WHERE name_key = ?1",
                [name_key(name)],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Sets the season of the character `name`; `false` if no such live
    /// character.
    pub fn set_character_season(&self, name: &str, season: u16) -> Result<bool, DbError> {
        Ok(self.conn.execute(
            "UPDATE characters SET season = ?2 WHERE name_key = ?1 AND deleted IS NULL",
            params![name_key(name), season],
        )? == 1)
    }

    /// Writes a character's row of its season in `game`. The time it
    /// reached its experience stays while the experience is unchanged.
    pub fn upsert(&self, game: &str, e: &LadderEntry) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO ladder(game, season, name_key, account_id, name, class, level, experience,
                                hardcore, expansion, dead, reached)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(game, season, name_key) DO UPDATE SET
                account_id = excluded.account_id, name = excluded.name, class = excluded.class,
                level = excluded.level, hardcore = excluded.hardcore,
                expansion = excluded.expansion, dead = excluded.dead,
                reached = CASE WHEN ladder.experience = excluded.experience
                               THEN ladder.reached ELSE excluded.reached END,
                experience = excluded.experience",
            params![
                game,
                e.season,
                name_key(&e.name),
                e.account,
                e.name,
                e.class,
                e.level,
                i64::try_from(e.experience).unwrap_or(i64::MAX),
                e.split.hardcore,
                e.split.expansion,
                e.dead,
                e.reached_ms
            ],
        )?;
        Ok(())
    }

    /// The row of the character `name` (any case) in `season` of `game`.
    pub fn entry(
        &self,
        game: &str,
        season: u16,
        name: &str,
    ) -> Result<Option<LadderEntry>, DbError> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {ENTRY_COLUMNS} FROM ladder WHERE game = ?1 AND season = ?2 AND name_key = ?3"
                ),
                params![game, season, name_key(name)],
                entry_of_row,
            )
            .optional()?)
    }

    /// A deleted character leaves the ladders of `game`, every season.
    /// Returns how many rows went.
    pub fn remove(&self, game: &str, name: &str) -> Result<usize, DbError> {
        Ok(self.conn.execute(
            "DELETE FROM ladder WHERE game = ?1 AND name_key = ?2",
            params![game, name_key(name)],
        )?)
    }

    /// Every row of one ladder of `season` in `game`, in rank order:
    /// experience (most first), then the time it was reached (earlier
    /// first), then the name in any case.
    pub fn split(
        &self,
        game: &str,
        season: u16,
        split: Split,
    ) -> Result<Vec<LadderEntry>, DbError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {ENTRY_COLUMNS} FROM ladder
             WHERE game = ?1 AND season = ?2 AND hardcore = ?3 AND expansion = ?4
             ORDER BY experience DESC, reached, name_key"
        ))?;
        let rows = st.query_map(
            params![game, season, split.hardcore, split.expansion],
            entry_of_row,
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
