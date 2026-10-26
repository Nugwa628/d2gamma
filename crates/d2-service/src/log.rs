//! A server program's log file in the log folder: `d2rust-realm.log` or
//! `d2rust-node.log` ([`ServerLog::open_named`]).
//!
//! Lines are appended and flushed one by one, so the file is complete up
//! to the last line even after a crash. Each line starts with the seconds
//! since the server started.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

/// The log file's name in the log folder when a test or tool names none.
pub const FILE_NAME: &str = "server.log";

/// An open server log. Shareable between threads.
#[derive(Debug)]
pub struct ServerLog {
    path: PathBuf,
    file: Mutex<File>,
    start: Instant,
}

impl ServerLog {
    /// Opens (or creates) the log in the folder `dir`, creating the folder
    /// if needed, and appends to it.
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        Self::open_named(dir, FILE_NAME)
    }

    /// [`ServerLog::open`] for the file `name` (each program has its own:
    /// `d2rust-realm.log`, `d2rust-node.log`).
    pub fn open_named(dir: &Path, name: &str) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(name);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(ServerLog {
            path,
            file: Mutex::new(file),
            start: Instant::now(),
        })
    }

    /// The log file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Appends one line. A failed write is ignored: the log never stops
    /// the server.
    pub fn line(&self, msg: &str) {
        let secs = self.start.elapsed().as_secs_f64();
        if let Ok(mut f) = self.file.lock() {
            let _ = writeln!(f, "[{secs:10.3}] {msg}");
            let _ = f.flush();
        }
    }
}
