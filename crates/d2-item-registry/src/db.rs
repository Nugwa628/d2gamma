//! The registry's tables in the mod realm's database, and the row reads
//! every part shares.

use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};

use crate::{Error, Place, Step, Where};

/// The registry's schema version (`item_meta.schema`; the database's
/// `user_version` belongs to whoever else shares it).
pub(crate) const SCHEMA: i64 = 1;

const CREATE: &str = "
CREATE TABLE IF NOT EXISTS item_meta(
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS item_blocks(
    first INTEGER PRIMARY KEY,
    count INTEGER NOT NULL,
    game INTEGER NOT NULL,
    open INTEGER NOT NULL DEFAULT 1,
    granted INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS item_blocks_game ON item_blocks(game, open);
CREATE TABLE IF NOT EXISTS items(
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    place TEXT NOT NULL,
    place_id TEXT NOT NULL,
    economy TEXT NOT NULL,
    game INTEGER,
    since INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS items_place ON items(place, place_id);
CREATE INDEX IF NOT EXISTS items_game ON items(game);
CREATE TABLE IF NOT EXISTS item_history(
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    id INTEGER NOT NULL,
    at INTEGER NOT NULL,
    place TEXT NOT NULL,
    place_id TEXT NOT NULL,
    game INTEGER,
    why TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS item_history_id ON item_history(id, seq);
CREATE TABLE IF NOT EXISTS item_claims(
    game INTEGER NOT NULL,
    id INTEGER NOT NULL,
    character TEXT NOT NULL,
    PRIMARY KEY(game, id, character)
);
CREATE INDEX IF NOT EXISTS item_claims_character ON item_claims(game, character);
CREATE TABLE IF NOT EXISTS item_dupes(
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    id INTEGER NOT NULL,
    at INTEGER NOT NULL,
    game INTEGER,
    kept TEXT NOT NULL,
    also TEXT NOT NULL,
    how TEXT NOT NULL
);
";

/// Opens (creating if needed) a realm database file in WAL mode, as the
/// accounts database is.
pub fn open_file(path: &Path) -> Result<Connection, Error> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    // An in-memory database answers "memory"; a file one "wal".
    let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
    Ok(conn)
}

/// Runs `f` in an immediate transaction: committed when it returns `Ok`,
/// rolled back otherwise. Two games' saves on two connections run one
/// after the other.
pub fn write<T>(
    conn: &mut Connection,
    f: impl FnOnce(&Connection) -> Result<T, Error>,
) -> Result<T, Error> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let out = f(&tx)?;
    tx.commit()?;
    Ok(out)
}

pub(crate) fn setup(conn: &Connection, mod_id: &str, first_id: u32) -> Result<(), Error> {
    conn.execute_batch(CREATE)?;
    if let Some(v) = meta(conn, "schema")? {
        let v: i64 = v.parse().unwrap_or(i64::MAX);
        if v > SCHEMA {
            return Err(Error::NewerSchema(v));
        }
    }
    match meta(conn, "mod")? {
        Some(theirs) if theirs != mod_id => {
            return Err(Error::OtherMod {
                ours: mod_id.to_owned(),
                theirs,
            })
        }
        Some(_) => {}
        None => set_meta(conn, "mod", mod_id)?,
    }
    set_meta(conn, "schema", &SCHEMA.to_string())?;
    if meta(conn, "next_id")?.is_none() {
        set_meta(conn, "next_id", &first_id.to_string())?;
    }
    Ok(())
}

pub(crate) fn meta(conn: &Connection, key: &str) -> Result<Option<String>, Error> {
    Ok(conn
        .query_row("SELECT value FROM item_meta WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub(crate) fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO item_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn place_col(s: String) -> rusqlite::Result<Place> {
    Place::parse(&s).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown place `{s}`").into(),
        )
    })
}

/// The registry row of `id`.
pub(crate) fn row(conn: &Connection, id: u32) -> Result<Option<Where>, Error> {
    Ok(conn
        .query_row(
            "SELECT place, place_id, game, economy FROM items WHERE id = ?1",
            [id],
            |r| {
                Ok(Where {
                    place: place_col(r.get(0)?)?,
                    place_id: r.get(1)?,
                    game: r.get(2)?,
                    split: r.get(3)?,
                })
            },
        )
        .optional()?)
}

/// Adds a new row for `id` and its first history step.
pub(crate) fn insert(
    conn: &Connection,
    id: u32,
    kind: &str,
    at: &Where,
    why: &str,
    now: i64,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO items(id, kind, place, place_id, economy, game, since)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            kind,
            at.place.as_str(),
            at.place_id,
            at.split,
            at.game,
            now
        ],
    )?;
    step(conn, id, at.place, &at.place_id, at.game, why, now)
}

/// Moves `id` to (`place`, `place_id`, `game`), with a history step.
/// `Ok(false)`: no such row.
pub(crate) fn relocate(
    conn: &Connection,
    id: u32,
    place: Place,
    place_id: &str,
    game: Option<u32>,
    why: &str,
    now: i64,
) -> Result<bool, Error> {
    let n = conn.execute(
        "UPDATE items SET place = ?2, place_id = ?3, game = ?4, since = ?5 WHERE id = ?1",
        params![id, place.as_str(), place_id, game, now],
    )?;
    if n == 0 {
        return Ok(false);
    }
    step(conn, id, place, place_id, game, why, now)?;
    Ok(true)
}

/// Appends one history step.
pub(crate) fn step(
    conn: &Connection,
    id: u32,
    place: Place,
    place_id: &str,
    game: Option<u32>,
    why: &str,
    now: i64,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO item_history(id, at, place, place_id, game, why)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, now, place.as_str(), place_id, game, why],
    )?;
    Ok(())
}

pub(crate) fn history(conn: &Connection, id: u32) -> Result<Vec<Step>, Error> {
    let mut q = conn.prepare(
        "SELECT at, place, place_id, game, why FROM item_history WHERE id = ?1 ORDER BY seq",
    )?;
    let rows = q.query_map([id], |r| {
        Ok(Step {
            at: r.get(0)?,
            place: place_col(r.get(1)?)?,
            place_id: r.get(2)?,
            game: r.get(3)?,
            why: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}
