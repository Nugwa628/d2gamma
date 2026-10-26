//! The games' journals (plan N7, contracts 11.11). Each chunk a game server
//! sends is checked (it decodes, it is that game's, it follows the one
//! before it) and its gzip member appended unchanged to the game's file,
//! `<journals>/<day>/<game>.d2j.gz`. A journal file is its chunks one
//! after another, which `d2_journal::read_journal` reads back.
//!
//! - A game's chunks may arrive out of order (each travels on its own
//!   stream): one that comes early waits for those before it, up to
//!   [`MAX_WAITING`].
//! - A chunk that doesn't decode, isn't that game's, or leaves a gap no
//!   later chunk fills, ends that game's journal here (`JournalInvalid`,
//!   logged); its file keeps the chunks before. The audit queue it should
//!   feed comes with N18.
//! - The day is the UTC day of the game's first chunk. Game ids start
//!   again at 1 when the main server restarts, so a game whose file of the
//!   day exists already gets `<game>-2.d2j.gz` (then `-3`, ...).
//! - A journal is complete when its `End` record arrives. Those of a lost
//!   game server end where its last chunk did.

use std::collections::{BTreeMap, HashMap};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use d2_journal::chunk::decode_chunk;
use d2_journal::Record;

use crate::log::ServerLog;

/// Chunks of one game kept while they wait for an earlier one.
pub const MAX_WAITING: usize = 32;

/// One game's journal while chunks come.
#[derive(Debug)]
struct Open {
    path: PathBuf,
    /// The chunk the file takes next.
    next: u32,
    /// Chunks that came early, by number.
    waiting: BTreeMap<u32, Vec<u8>>,
    bytes: u64,
    /// A chunk broke the journal: the game's later chunks are dropped.
    broken: bool,
}

/// The journals the main server keeps.
#[derive(Debug)]
pub struct Journals {
    dir: PathBuf,
    /// By game server and game.
    open: Mutex<HashMap<(String, u32), Open>>,
    log: Arc<ServerLog>,
}

/// What became of a chunk.
#[derive(Debug, PartialEq, Eq)]
pub enum Kept {
    /// Appended (with the early ones it let follow).
    Stored,
    /// It came early: kept until the ones before it come.
    Waiting,
    /// The journal is complete: its `End` record was stored.
    Complete,
    /// Dropped: the game's journal broke before.
    Dropped,
}

impl Journals {
    /// Journals under `dir`.
    pub fn new(dir: PathBuf, log: Arc<ServerLog>) -> Self {
        Self {
            dir,
            open: Mutex::new(HashMap::new()),
            log,
        }
    }

    fn open(&self) -> MutexGuard<'_, HashMap<(String, u32), Open>> {
        self.open.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Takes chunk `seq` of game `game` from game server `node`, as it
    /// came on its stream. Blocking: it writes the game's file. `Err` is
    /// why the game's journal broke (`JournalInvalid`).
    pub fn chunk(&self, node: &str, game: u32, seq: u32, gz: Vec<u8>) -> Result<Kept, String> {
        let mut open = self.open();
        let key = (node.to_owned(), game);
        if !open.contains_key(&key) {
            if seq != 0 {
                // The start of this journal never came (or broke and was
                // forgotten): nothing to append to.
                return Ok(Kept::Dropped);
            }
            let path = self.new_path(game, SystemTime::now());
            open.insert(
                key.clone(),
                Open {
                    path,
                    next: 0,
                    waiting: BTreeMap::new(),
                    bytes: 0,
                    broken: false,
                },
            );
        }
        let j = open.get_mut(&key).expect("inserted above");
        if j.broken {
            return Ok(Kept::Dropped);
        }
        let kept = Self::take(j, game, seq, gz);
        match &kept {
            Ok(Kept::Complete) => {
                self.log.line(&format!(
                    "node {node}: game {game}: journal complete, {} chunks, {} bytes: {}",
                    j.next,
                    j.bytes,
                    j.path.display()
                ));
                open.remove(&key);
            }
            Err(why) => {
                j.broken = true;
                j.waiting.clear();
                self.log.line(&format!(
                    "node {node}: game {game}: journal invalid at chunk {seq} (kept {} chunks in {}): {why}",
                    j.next,
                    j.path.display()
                ));
            }
            Ok(_) => {}
        }
        kept
    }

    /// Checks chunk `seq` and appends it, and the early ones it lets follow.
    fn take(j: &mut Open, game: u32, seq: u32, gz: Vec<u8>) -> Result<Kept, String> {
        if seq < j.next || j.waiting.contains_key(&seq) {
            return Err(format!("chunk {seq} came twice"));
        }
        if seq > j.next {
            if j.waiting.len() >= MAX_WAITING {
                return Err(format!(
                    "chunk {} never came ({} later ones waiting)",
                    j.next,
                    j.waiting.len()
                ));
            }
            j.waiting.insert(seq, gz);
            return Ok(Kept::Waiting);
        }
        let mut gz = gz;
        loop {
            let body = decode_chunk(&gz).map_err(|e| e.to_string())?;
            if body.game != game || body.seq != j.next {
                return Err(format!(
                    "chunk {} of game {game} holds chunk {} of game {}",
                    j.next, body.seq, body.game
                ));
            }
            append(&j.path, &gz).map_err(|e| format!("{}: {e}", j.path.display()))?;
            j.bytes += gz.len() as u64;
            j.next += 1;
            if matches!(
                body.entries.last().map(|e| &e.record),
                Some(Record::End { .. })
            ) {
                return Ok(Kept::Complete);
            }
            match j.waiting.remove(&j.next) {
                Some(next) => gz = next,
                None => return Ok(Kept::Stored),
            }
        }
    }

    /// Game server `node` was lost: its open journals end where they are.
    pub fn node_lost(&self, node: &str) {
        let mut open = self.open();
        let lost: Vec<(String, u32)> = open.keys().filter(|(n, _)| n == node).cloned().collect();
        for key in lost {
            if let Some(j) = open.remove(&key) {
                if !j.broken {
                    self.log.line(&format!(
                        "node {node}: game {}: journal cut off after {} chunks: {}",
                        key.1,
                        j.next,
                        j.path.display()
                    ));
                }
            }
        }
    }

    /// The file of a game whose first chunk came at `now`.
    fn new_path(&self, game: u32, now: SystemTime) -> PathBuf {
        let day = d2_service::status::dated(now);
        let day = self.dir.join(&day[..10]);
        let mut path = day.join(format!("{game}.d2j.gz"));
        let mut n = 2;
        while path.exists() {
            path = day.join(format!("{game}-{n}.d2j.gz"));
            n += 1;
        }
        path
    }
}

/// Appends `gz` to the file at `path`, making its folder.
fn append(path: &Path, gz: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(gz)?;
    f.flush()
}

#[cfg(test)]
#[path = "journals_tests.rs"]
mod tests;
