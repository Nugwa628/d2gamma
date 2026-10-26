//! Offline backups of the server character folder. Only validated character
//! envelopes are copied; game data and unfinished saves are never included.
//!
//! The caller must stop character writers for the whole operation. A backup is
//! usable only after [`COMPLETE`] appears. A crash may leave an incomplete
//! directory: keep the previous completed backup until the new one completes.
//!
//! Live backups: `server-ctl backup` (and the nightly timer) leaves
//! `run/backup.request` in the server folder; the running server makes the
//! backup ([`backup_now`]) and writes the result to `run/backup.done`
//! (`ok <folder>` or `failed <why>`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use d2_service::status::dated;

use crate::characters::{decode, CharacterError, CharacterStore, EXTENSION};
use crate::Accounts;

/// Written and flushed last, after every character file has been flushed.
pub const COMPLETE: &str = "BACKUP-COMPLETE";

/// A backup could not be completed.
#[derive(Debug, thiserror::Error)]
pub enum BackupError {
    /// A character could not be listed or validated.
    #[error(transparent)]
    Character(#[from] CharacterError),
    /// A file operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The backup would live inside the active character folder.
    #[error("backup destination must be outside the character folder")]
    InsideStore,
    /// Multiple names refer to the same character, or an entry is not a file.
    #[error("ambiguous or non-file character entry: {0}")]
    InvalidEntry(String),
}

/// Copies a quiescent store into a new directory, returning its character count.
/// The destination's parent must exist. Existing destinations are never reused.
/// Each copied envelope is checked before writing, preserving its exact bytes.
/// On a reported failure no completion marker is left; partial files are kept
/// for diagnosis. Restore with the server stopped by copying the character
/// files from a completed backup into a new character folder.
pub fn snapshot(store: &CharacterStore, destination: &Path) -> Result<usize, BackupError> {
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let source = store.dir().canonicalize()?;
    if parent.canonicalize()?.starts_with(&source) {
        return Err(BackupError::InsideStore);
    }
    // create_dir is exclusive: neither an old snapshot nor another invocation
    // can have any of its files replaced by this operation.
    std::fs::create_dir(destination)?;
    let names = store.list()?;
    let mut previous: Option<String> = None;
    for name in &names {
        let key = name.to_ascii_lowercase();
        if previous.as_ref() == Some(&key) {
            return Err(BackupError::InvalidEntry(name.clone()));
        }
        previous = Some(key);
        let file_name = format!("{name}.{EXTENSION}");
        let path = source.join(&file_name);
        if !std::fs::symlink_metadata(&path)?.file_type().is_file() {
            return Err(BackupError::InvalidEntry(name.clone()));
        }
        let bytes = std::fs::read(path)?;
        decode(name, &bytes)?;
        write_new(&destination.join(file_name), &bytes)?;
    }
    let marker = destination.join("BACKUP-COMPLETE.tmp");
    write_new(&marker, b"D2RUST character backup v1\n")?;
    std::fs::rename(marker, destination.join(COMPLETE))?;
    Ok(names.len())
}

fn write_new(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Makes a backup now: a dated folder in `backups` with the accounts
/// database and the characters ([`Accounts::backup_into`]), then removes
/// the oldest beyond `keep`. Returns the folder.
pub fn backup_now(accounts: &Accounts, backups: &Path, keep: usize) -> Result<PathBuf, String> {
    std::fs::create_dir_all(backups).map_err(|e| format!("{}: {e}", backups.display()))?;
    let mut dest = backups.join(dated(SystemTime::now()));
    let mut n = 1;
    while dest.exists() {
        n += 1;
        dest = backups.join(format!("{}-{n}", dated(SystemTime::now())));
    }
    accounts.backup_into(&dest)?;
    prune(backups, keep);
    Ok(dest)
}

/// Removes the oldest dated backups beyond `keep`.
fn prune(backups: &Path, keep: usize) {
    let Ok(entries) = std::fs::read_dir(backups) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    // The names sort by date.
    dirs.sort();
    let over = dirs.len().saturating_sub(keep);
    for d in dirs.into_iter().take(over) {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// Answers `run/backup.request` in the server folder `folder`, if one is
/// there.
pub fn answer_backup_request(folder: &Path, accounts: &Accounts, backups: &Path, keep: usize) {
    let run = folder.join("run");
    let request = run.join("backup.request");
    if !request.exists() {
        return;
    }
    let _ = std::fs::remove_file(&request);
    let answer = match backup_now(accounts, backups, keep) {
        Ok(dir) => format!("ok {}\n", dir.display()),
        Err(e) => format!("failed {e}\n"),
    };
    let _ = std::fs::write(run.join("backup.done"), answer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_oldest_backups_go_beyond_keep() {
        let dir = std::env::temp_dir().join(format!("d2rust-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for n in ["2026-10-01_0430", "2026-10-02_0430", "2026-10-03_0430"] {
            std::fs::create_dir_all(dir.join(n)).unwrap();
        }
        prune(&dir, 2);
        let mut left: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["2026-10-02_0430", "2026-10-03_0430"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
