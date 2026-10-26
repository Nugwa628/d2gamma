//! The running server's health and housekeeping, without a port:
//!
//! - [`Status`]: written to `logs/status.json` every
//!   [`STATUS_INTERVAL`]: version, uptime, sessions, games, players and
//!   tick overruns. A file older than three intervals means the server is
//!   stuck or down (`server-ctl status` shows its age).
//! - [`ServerLock`]: `data/server.lock` while the server runs, holding its
//!   process id, so offline admin commands ([`ServerLock::running`]) refuse
//!   to change files the server holds, and a second server on the same
//!   folder refuses to start.
//! - [`dated`]: the name of a dated backup folder (the live backups
//!   themselves are the accounts' part, `d2_accounts::backup`).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How often `status.json` is written and the backup request looked for.
pub const STATUS_INTERVAL: Duration = Duration::from_secs(10);

/// The status file's name in the log folder.
pub const STATUS_FILE: &str = "status.json";

/// The lock file's name in the data folder.
pub const LOCK_FILE: &str = "server.lock";

/// What `status.json` says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub version: String,
    pub uptime_s: u64,
    pub sessions: usize,
    pub games: usize,
    pub players: usize,
    pub tick_overruns: u64,
}

impl Status {
    /// The file's text (one JSON object).
    pub fn json(&self) -> String {
        format!(
            "{{\"version\": \"{}\", \"uptime_s\": {}, \"sessions\": {}, \"games\": {}, \"players\": {}, \"tick_overruns\": {}}}\n",
            self.version, self.uptime_s, self.sessions, self.games, self.players, self.tick_overruns
        )
    }

    /// Writes it to `logs/status.json` (a temporary file renamed over the
    /// old one, so a reader never sees half a file).
    pub fn write(&self, logs: &Path) -> std::io::Result<()> {
        let path = logs.join(STATUS_FILE);
        let tmp = logs.join(format!("{STATUS_FILE}.tmp"));
        std::fs::write(&tmp, self.json())?;
        std::fs::rename(tmp, path)
    }
}

/// `data/server.lock` while the server runs.
#[derive(Debug)]
pub struct ServerLock {
    path: PathBuf,
}

impl ServerLock {
    /// Takes the lock of the data folder `data`. Refused while another
    /// server process holds it; a lock left by a process that is gone is
    /// taken over.
    pub fn take(data: &Path) -> Result<ServerLock, String> {
        std::fs::create_dir_all(data).map_err(|e| format!("{}: {e}", data.display()))?;
        let path = data.join(LOCK_FILE);
        if let Some(pid) = Self::running(data) {
            return Err(format!(
                "another server (process {pid}) runs on {}",
                data.display()
            ));
        }
        let mut f = std::fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        writeln!(f, "{}", std::process::id()).map_err(|e| e.to_string())?;
        Ok(ServerLock { path })
    }

    /// The process id of the server running on the data folder `data`, if
    /// one runs.
    pub fn running(data: &Path) -> Option<u32> {
        let text = std::fs::read_to_string(data.join(LOCK_FILE)).ok()?;
        let pid: u32 = text.trim().parse().ok()?;
        (pid != std::process::id() && process_alive(pid)).then_some(pid)
    }
}

impl Drop for ServerLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(target_os = "linux")]
fn process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_alive(_pid: u32) -> bool {
    // Without a process table to look at, a lock file means a server runs.
    true
}

/// A dated backup folder name, UTC: `2026-10-19_0430`.
pub fn dated(now: SystemTime) -> String {
    let secs = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02}_{:02}{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// The calendar date of a day number since 1970-01-01.
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_is_one_json_object() {
        let s = Status {
            version: "0.1.1.0".into(),
            uptime_s: 12,
            sessions: 3,
            games: 1,
            players: 2,
            tick_overruns: 0,
        };
        assert_eq!(
            s.json(),
            "{\"version\": \"0.1.1.0\", \"uptime_s\": 12, \"sessions\": 3, \"games\": 1, \"players\": 2, \"tick_overruns\": 0}\n"
        );
    }

    #[test]
    fn backup_folders_are_dated_in_utc() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1_760_848_200);
        assert_eq!(dated(t), "2025-10-19_0430");
        assert_eq!(dated(SystemTime::UNIX_EPOCH), "1970-01-01_0000");
        let leap = SystemTime::UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(dated(leap), "2000-02-29_0000");
    }

    #[test]
    fn a_second_server_on_the_same_data_is_refused() {
        let dir = std::env::temp_dir().join(format!("d2rust-lock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let lock = ServerLock::take(&dir).unwrap();
        // This process's own lock doesn't count as "another server"; a lock
        // of a live other process does.
        assert_eq!(ServerLock::running(&dir), None);
        std::fs::write(dir.join(LOCK_FILE), "1\n").unwrap();
        #[cfg(target_os = "linux")]
        {
            assert_eq!(ServerLock::running(&dir), Some(1));
            assert!(ServerLock::take(&dir).is_err());
            // A lock of a process that is gone is taken over.
            std::fs::write(dir.join(LOCK_FILE), "4194303\n").unwrap();
            assert!(ServerLock::take(&dir).is_ok());
        }
        drop(lock);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
