//! The game server's updater (`docs/plans/server/upgrades.md`, U2): what
//! it does with the main server's `Update` messages.
//!
//! - `Update(countdown)`: its line goes to every game ([`NodeGames::announce`]);
//!   the first one of a version starts the fetch: `update_command fetch V`
//!   (the `server-ctl` of its folder, or `node-ctl.ps1` on Windows), one
//!   at a time, with `UpdateState` `downloading`, then `ready` or `failed`.
//! - `Update(restart)`: the last line; every game saves and closes, and it
//!   says `restarting`. With the new version staged it writes the swap
//!   marker (`run/swap`, the version) and the program exits 75: its
//!   supervisor swaps to the staged version and starts it. Not staged yet:
//!   it keeps fetching and restarts once it has it.
//! - `Update(cancel)`: the line; the fetched version is dropped (`idle`).
//! - `Update(catch-up)` (before a `NodeBuild` refusal): fetch the main
//!   server's version and restart on it at once.
//!
//! Without `update_command` it only shows the lines and saves its games at
//! zero; a refusal for another version ends the program, as before.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use d2_net::msg::node::{update_phase, update_state, NodeToRealm, MAX_UPDATE_TEXT};
use d2_service::log::ServerLog;
use tokio::sync::watch;

use crate::seats::NodeGames;

/// The exit code that asks the supervisor to swap to the staged version.
pub const SWAP_EXIT: u8 = 75;

/// The swap marker in the game server's folder.
pub const SWAP_FILE: &str = "run/swap";

/// The longest fetch.
const FETCH_WAIT: Duration = Duration::from_secs(600);

/// How long the games run on after the restart's line before they save and
/// close: they send on their ticks, so the line reaches every player first.
const LAST_LINE_WAIT: Duration = Duration::from_millis(500);

/// How the game server fetches a version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetcher {
    /// No updater: updates are only shown.
    Off,
    /// A test setup's: every version is staged at once.
    Test,
    /// `update_command`'s words, then `fetch VERSION`; run in `folder`.
    Run { words: Vec<String>, folder: PathBuf },
}

/// What the updater does now, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Do {
    /// A line in every game.
    Announce(String),
    /// Fetch and check this version.
    Fetch(String),
    /// `UpdateState` to the main server.
    Report {
        version: String,
        state: u8,
        detail: String,
    },
    /// Every game saves and closes.
    CloseGames,
    /// Write the swap marker for this version and exit [`SWAP_EXIT`].
    Swap(String),
}

fn report(version: &str, state: u8, detail: &str) -> Do {
    let mut detail = detail.to_owned();
    if detail.len() > MAX_UPDATE_TEXT {
        let mut cut = MAX_UPDATE_TEXT;
        while !detail.is_char_boundary(cut) {
            cut -= 1;
        }
        detail.truncate(cut);
    }
    Do::Report {
        version: version.to_owned(),
        state,
        detail,
    }
}

#[derive(Debug, Default)]
struct St {
    /// The version wanted, and where its fetch is.
    target: Option<String>,
    state: u8,
    /// The version being fetched now.
    fetching: Option<String>,
    /// Restart once the target is staged.
    restart: bool,
    /// The games were closed for a restart.
    closed: bool,
    swapping: bool,
}

/// The game server's updater.
pub struct Updater {
    fetcher: Fetcher,
    /// The version this game server runs.
    own: String,
    /// The game server's folder (its `run/swap`).
    folder: PathBuf,
    st: Mutex<St>,
    swap: watch::Sender<Option<String>>,
    log: Arc<ServerLog>,
}

impl Updater {
    pub fn new(fetcher: Fetcher, own: &str, folder: &Path, log: Arc<ServerLog>) -> Arc<Updater> {
        Arc::new(Updater {
            fetcher,
            own: own.to_owned(),
            folder: folder.to_owned(),
            st: Mutex::new(St::default()),
            swap: watch::channel(None).0,
            log,
        })
    }

    fn lock(&self) -> MutexGuard<'_, St> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Whether it fetches versions: then a `NodeBuild` refusal means "link
    /// again later", never the end.
    pub fn managed(&self) -> bool {
        self.fetcher != Fetcher::Off
    }

    /// What an `Update` asks for.
    pub fn on_update(&self, phase: u8, version: &str, text: &str) -> Vec<Do> {
        let mut out = Vec::new();
        if !text.is_empty() && phase != update_phase::CATCH_UP {
            out.push(Do::Announce(text.to_owned()));
        }
        let wanted = self.managed() && version != self.own;
        let mut s = self.lock();
        match phase {
            update_phase::COUNTDOWN if wanted => self.want(&mut s, version, &mut out),
            update_phase::RESTART => {
                if !s.closed {
                    s.closed = true;
                    out.push(Do::CloseGames);
                    out.push(report(version, update_state::RESTARTING, ""));
                }
                if wanted {
                    s.restart = true;
                    self.want(&mut s, version, &mut out);
                    swap_if_staged(&mut s, &mut out);
                }
            }
            update_phase::CANCEL => {
                if let Some(v) = s.target.take() {
                    s.state = update_state::IDLE;
                    s.restart = false;
                    out.push(report(&v, update_state::IDLE, "called off"));
                }
            }
            update_phase::CATCH_UP if wanted => {
                s.restart = true;
                self.want(&mut s, version, &mut out);
                swap_if_staged(&mut s, &mut out);
            }
            _ => {}
        }
        out
    }

    /// `version` is wanted: its fetch starts (or starts again after a
    /// failure).
    fn want(&self, s: &mut St, version: &str, out: &mut Vec<Do>) {
        let again = s.target.as_deref() == Some(version) && s.state == update_state::FAILED;
        if s.target.as_deref() == Some(version) && !again {
            return;
        }
        s.target = Some(version.to_owned());
        s.state = update_state::DOWNLOADING;
        out.push(report(version, update_state::DOWNLOADING, ""));
        if s.fetching.is_none() {
            s.fetching = Some(version.to_owned());
            out.push(Do::Fetch(version.to_owned()));
        }
    }

    /// The fetch of `version` ended.
    pub fn fetched(&self, version: &str, got: Result<(), String>) -> Vec<Do> {
        let mut out = Vec::new();
        let mut s = self.lock();
        s.fetching = None;
        if s.target.as_deref() != Some(version) {
            // Another version is wanted now (or none).
            if let Some(t) = s.target.clone() {
                s.fetching = Some(t.clone());
                out.push(Do::Fetch(t));
            }
            return out;
        }
        match got {
            Ok(()) => {
                s.state = update_state::READY;
                out.push(report(version, update_state::READY, ""));
                swap_if_staged(&mut s, &mut out);
            }
            Err(e) => {
                s.state = update_state::FAILED;
                out.push(report(version, update_state::FAILED, &e));
            }
        }
        out
    }

    /// Does `actions` in order, on a task of the runtime.
    pub fn run(self: &Arc<Self>, actions: Vec<Do>, games: &Arc<NodeGames>) {
        if actions.is_empty() {
            return;
        }
        let (me, games) = (self.clone(), games.clone());
        tokio::spawn(async move {
            for a in actions {
                me.exec(a, &games).await;
            }
        });
    }

    async fn exec(self: &Arc<Self>, a: Do, games: &Arc<NodeGames>) {
        match a {
            Do::Announce(text) => {
                let g = games.clone();
                let _ = tokio::task::spawn_blocking(move || g.announce(&text)).await;
            }
            Do::CloseGames => {
                tokio::time::sleep(LAST_LINE_WAIT).await;
                self.log
                    .line("update: the restart; every game saves and closes");
                let g = games.clone();
                let _ = tokio::task::spawn_blocking(move || g.shutdown_all()).await;
            }
            Do::Report {
                version,
                state,
                detail,
            } => games.send_up(&NodeToRealm::UpdateState {
                version,
                state,
                detail,
            }),
            Do::Fetch(version) => {
                let (me, g) = (self.clone(), games.clone());
                tokio::spawn(async move {
                    let fetcher = me.fetcher.clone();
                    let v = version.clone();
                    let log = me.log.clone();
                    let got = tokio::task::spawn_blocking(move || fetch(&fetcher, &v, &log))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                    let next = me.fetched(&version, got);
                    me.run(next, &g);
                });
            }
            Do::Swap(version) => {
                let marker = self.folder.join(SWAP_FILE);
                let written = marker
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&marker, format!("{version}\n")));
                match written {
                    Ok(()) => {
                        self.log.line(&format!(
                            "update: {version} is staged; exiting {SWAP_EXIT} for the swap"
                        ));
                        self.swap.send_replace(Some(version));
                    }
                    Err(e) => {
                        self.log
                            .line(&format!("update: writing {}: {e}", marker.display()));
                        games.send_up(&NodeToRealm::UpdateState {
                            version,
                            state: update_state::FAILED,
                            detail: "the swap marker can't be written".into(),
                        });
                    }
                }
            }
        }
    }

    /// Waits until a swap is asked for: its version.
    pub async fn swapping(&self) -> String {
        let mut rx = self.swap.subscribe();
        loop {
            if let Some(v) = rx.borrow_and_update().clone() {
                return v;
            }
            if rx.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }
}

/// The target is staged and a restart is asked for: the swap, once.
fn swap_if_staged(s: &mut St, out: &mut Vec<Do>) {
    if s.restart && s.state == update_state::READY && !s.swapping {
        if let Some(v) = s.target.clone() {
            s.swapping = true;
            out.push(Do::Swap(v));
        }
    }
}

/// Runs the fetch of `version`: `Err` with the command's last words.
fn fetch(fetcher: &Fetcher, version: &str, log: &ServerLog) -> Result<(), String> {
    let (words, folder) = match fetcher {
        Fetcher::Off => return Err("no update_command".into()),
        Fetcher::Test => return Ok(()),
        Fetcher::Run { words, folder } => (words, folder),
    };
    let Some((program, args)) = words.split_first() else {
        return Err("update_command is empty".into());
    };
    // A program in the game server's folder (`server-ctl`), else on the
    // path (`powershell.exe`).
    let local = folder.join(program);
    let program = if local.is_file() {
        local
    } else {
        PathBuf::from(program)
    };
    log.line(&format!(
        "update: fetching {version} ({})",
        program.display()
    ));
    let mut cmd = Command::new(&program);
    cmd.args(args)
        .arg("fetch")
        .arg(version)
        .current_dir(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    no_window(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("starting {}: {e}", program.display()))?;
    let mut stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(e) = stderr.as_mut() {
            let _ = std::io::Read::read_to_string(e, &mut text);
        }
        text
    });
    let until = Instant::now() + FETCH_WAIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(200)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("no answer in {} s", FETCH_WAIT.as_secs()));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let text = reader.join().unwrap_or_default();
    if status.success() {
        log.line(&format!("update: {version} fetched and checked"));
        return Ok(());
    }
    let last = text
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or("")
        .to_owned();
    log.line(&format!(
        "update: fetching {version} failed ({status}): {last}"
    ));
    Err(if last.is_empty() {
        format!("the fetch ended with {status}")
    } else {
        last
    })
}

/// A Windows game server starts its fetch with no window.
#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_window(_: &mut Command) {}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
