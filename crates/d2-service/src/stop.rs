//! The stop signals a server program ends on: every game saves and
//! closes, then the program exits 0.
//!
//! - Linux: SIGTERM (systemd, `server-ctl stop`) or Ctrl+C (SIGINT).
//! - Windows: Ctrl+C, Ctrl+Break, or the console window being closed.
//! - Either: a stop marker file ([`wait_for_stop_file`]), for a supervisor
//!   that has no signal to send (Windows' `node-ctl.ps1` starts its game
//!   server with no console to send Ctrl+Break to).

use std::path::PathBuf;
use std::time::Duration;

/// How often [`wait_for_stop_file`] looks for its marker.
pub const STOP_FILE_POLL: Duration = Duration::from_secs(1);

/// Waits for a stop signal; says which. Call on a tokio runtime with
/// signals enabled.
pub async fn wait_for_stop() -> &'static str {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut term = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
                return "Ctrl+C";
            }
        };
        tokio::select! {
            _ = term.recv() => "SIGTERM",
            _ = tokio::signal::ctrl_c() => "Ctrl+C",
        }
    }
    #[cfg(windows)]
    {
        use tokio::signal::windows::{ctrl_break, ctrl_close};
        // A handler that can't be set (no console) never fires.
        let brk = async {
            match ctrl_break() {
                Ok(mut s) => s.recv().await,
                Err(_) => std::future::pending().await,
            }
        };
        let close = async {
            match ctrl_close() {
                Ok(mut s) => s.recv().await,
                Err(_) => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => "Ctrl+C",
            _ = brk => "Ctrl+Break",
            _ = close => "console closed",
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = tokio::signal::ctrl_c().await;
        "Ctrl+C"
    }
}

/// Waits until the marker file `path` exists, removes it, and says so.
/// A marker already there when it is called counts: the supervisor
/// removes it before each start.
pub async fn wait_for_stop_file(path: PathBuf) -> &'static str {
    let mut every = tokio::time::interval(STOP_FILE_POLL);
    loop {
        every.tick().await;
        if path.exists() {
            let _ = std::fs::remove_file(&path);
            return "stop marker";
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stop_marker_stops_and_is_removed() {
        let dir = std::env::temp_dir().join(format!("d2-service-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let marker = dir.join("stop");
        let _ = std::fs::remove_file(&marker);
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        rt.block_on(async {
            let wait = tokio::spawn(wait_for_stop_file(marker.clone()));
            tokio::time::sleep(STOP_FILE_POLL * 2).await;
            assert!(!wait.is_finished(), "no marker yet");
            std::fs::write(&marker, b"").unwrap();
            let why = tokio::time::timeout(STOP_FILE_POLL * 3, wait)
                .await
                .expect("it stops within a poll or two")
                .unwrap();
            assert_eq!(why, "stop marker");
        });
        assert!(!marker.exists(), "the marker is removed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
