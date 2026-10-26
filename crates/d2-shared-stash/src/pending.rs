//! Character records waiting to be written to their files (single
//! player).
//!
//! A move changes two things: the stash (in the stash file) and the
//! character (in its own save file). They can't share one transaction,
//! so the stash file holds the character's new record too: the economy
//! calls [`PendingRecords::store`] inside the move's transaction, which
//! keeps the record as **pending** for that character. After the commit
//! the record is written to the character's file (write-then-rename) and
//! the pending row is dropped. If the game stops in between, the next
//! open writes every pending record to its file before any character
//! loads (`LocalStash::open`). So the move counts once it commits: before that
//! the stash and the character file are as they were, after it both
//! show the move.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use d2_economy::rusqlite::{params, Connection};
use d2_economy::{CharRef, CharacterRecords, Refusal};

use crate::StoreError;

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS local_pending(
    character TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    record BLOB NOT NULL,
    seq INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS local_meta(key TEXT PRIMARY KEY, value INTEGER NOT NULL);
";

/// What the next stored record is for: the character file it belongs in.
#[derive(Debug, Default)]
pub(crate) struct Target {
    pub path: Option<PathBuf>,
    /// Test crash point: panic inside the move's transaction.
    #[cfg(any(test, feature = "crash-points"))]
    pub crash: bool,
}

/// The economy's record hook for single player.
#[derive(Clone, Debug, Default)]
pub(crate) struct PendingRecords(pub Arc<Mutex<Target>>);

impl PendingRecords {
    pub fn aim(&self, path: &Path) {
        self.lock().path = Some(path.to_path_buf());
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, Target> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl CharacterRecords for PendingRecords {
    fn store(
        &self,
        conn: &Connection,
        character: &CharRef,
        record: &[u8],
        seq: u32,
    ) -> Result<(), Refusal> {
        let target = self.lock();
        #[cfg(any(test, feature = "crash-points"))]
        if target.crash {
            panic!("injected crash inside the move's transaction");
        }
        let path = target.path.as_ref().ok_or(Refusal::BadRequest)?;
        conn.execute(
            "INSERT INTO local_pending(character, path, record, seq) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(character) DO UPDATE SET path = excluded.path, \
             record = excluded.record, seq = excluded.seq",
            params![character.name, path.to_string_lossy(), record, seq],
        )
        .map_err(|_| Refusal::TransferInvalid)?;
        Ok(())
    }
}

/// A pending record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pending {
    pub character: String,
    pub path: PathBuf,
    pub record: Vec<u8>,
}

pub(crate) fn get(conn: &Connection, character: &str) -> Result<Option<Pending>, StoreError> {
    Ok(all(conn)?.into_iter().find(|p| p.character == character))
}

pub(crate) fn all(conn: &Connection) -> Result<Vec<Pending>, StoreError> {
    let mut st = conn.prepare("SELECT character, path, record FROM local_pending")?;
    let rows = st.query_map([], |r| {
        Ok(Pending {
            character: r.get(0)?,
            path: PathBuf::from(r.get::<_, String>(1)?),
            record: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Drops `character`'s pending record if it is still `record` (a newer
/// move may have replaced it meanwhile).
pub(crate) fn clear(conn: &Connection, character: &str, record: &[u8]) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM local_pending WHERE character = ?1 AND record = ?2",
        params![character, record],
    )?;
    Ok(())
}

/// Drops `character`'s pending record, whatever it is: the game wrote the
/// character's file itself, from a state that already holds every move.
pub(crate) fn forget(conn: &Connection, character: &str) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM local_pending WHERE character = ?1",
        [character],
    )?;
    Ok(())
}

/// Writes `bytes` to `path` as the game writes a character: the old file
/// kept as `<path>.bak`, the new bytes written to `<path>.tmp`, flushed
/// to disk, then renamed over the file.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    use std::io::Write;
    let io = |e: std::io::Error| StoreError::Io(format!("{}: {e}", path.display()));
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let with = |ext: &str| {
        let mut p = path.as_os_str().to_owned();
        p.push(ext);
        PathBuf::from(p)
    };
    if path.exists() {
        std::fs::copy(path, with(".bak")).map_err(io)?;
    }
    let tmp = with(".tmp");
    let mut f = std::fs::File::create(&tmp).map_err(io)?;
    f.write_all(bytes).map_err(io)?;
    f.sync_all().map_err(io)?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(io)
}

/// A counter in `local_meta`, raised by one and returned (request numbers
/// and item IDs, never reused).
pub(crate) fn next(conn: &Connection, key: &str) -> Result<u64, StoreError> {
    let v: i64 = conn.query_row(
        "INSERT INTO local_meta(key, value) VALUES (?1, 1) \
         ON CONFLICT(key) DO UPDATE SET value = value + 1 RETURNING value",
        [key],
        |r| r.get(0),
    )?;
    Ok(v as u64)
}
