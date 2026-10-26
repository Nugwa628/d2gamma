//! Upgrades with a countdown (`docs/plans/server/upgrades.md`, U1): the
//! main server tells its players and its game servers that it restarts for
//! a new version, waits for every game to save, then lets `server-ctl` swap
//! it.
//!
//! - A request starts it: `run/upgrade.request` from `server-ctl upgrade`
//!   (`version=V`, `minutes=N`), or the dev console's `upgrade V [MINUTES
//!   [MINUTE_MS]]` on a test setup. `MINUTE_MS` (test setups only) makes a
//!   minute shorter, so a test hears the whole countdown in seconds; the
//!   lines still say minutes.
//! - **Countdown:** "Restarting for an update in 10 minutes" at the start,
//!   then at 10, 5, 4, 3, 2 and 1 minutes and at 30 seconds (those inside
//!   the asked length, [`marks`]). Each line goes to Global as a realm
//!   notice and to every linked game server as `Update(countdown)`, which
//!   shows it in every game and starts the game server's download.
//! - **Zero:** "Restarting for an update now" and `Update(restart)`: every
//!   game saves and its game server restarts on the new version. The main
//!   server waits until each game server linked at zero answered
//!   `restarting` or is gone (at most [`RESTART_WAIT`]), then its phase is
//!   `swap`: `server-ctl` stops it and installs the new version (health
//!   check, rollback). Zero waits for the main server's own package
//!   (`run/upgrade.ready`, written by `server-ctl` once it is fetched and
//!   checked) up to [`READY_WAIT`]; without it the upgrade is called off.
//! - **Cancel** (`run/upgrade.cancel`, or `upgrade-cancel`): "The update was
//!   called off" in Global and in the games, `Update(cancel)`.
//! - **Catch-up:** a game server refused for another version is first sent
//!   `Update(catch-up)` with this main server's version, so it fetches it
//!   and restarts on it; not a game server already on the version this
//!   main server restarts for ([`Upgrade::catch_up`]).
//!
//! `logs/upgrade.json` says where it is (`phase`: `idle`, `countdown`,
//! `restart`, `swap` or `cancelled`; `version`, `seconds_left`, `ready`,
//! `waiting`), for `server-ctl`; it is written on every change.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use d2_net::msg::node::{update_phase, valid_update_version, RealmToNode};

use crate::log::ServerLog;

/// The countdown's length when the request names none.
pub const DEFAULT_MINUTES: u32 = 10;
/// The longest countdown a request may ask for.
pub const MAX_MINUTES: u32 = 24 * 60;
/// A real minute.
pub const MINUTE_MS: u64 = 60_000;
/// The shortest minute a test may ask for.
pub const MIN_MINUTE_MS: u64 = 200;
/// How long zero waits for the game servers' `restarting`.
pub const RESTART_WAIT: Duration = Duration::from_secs(60);
/// How long zero waits for the main server's own package.
pub const READY_WAIT: Duration = Duration::from_secs(300);
/// How often the request files are looked for and the countdown moves.
pub const STEP: Duration = Duration::from_millis(250);

/// The minutes the countdown speaks at, when they are inside its length.
const MARK_MINUTES: [u64; 6] = [10, 5, 4, 3, 2, 1];

/// The line at zero.
pub const NOW_TEXT: &str = "Restarting for an update now";
/// The line of a cancel.
pub const CANCEL_TEXT: &str = "The update was called off";

/// The request files in the server folder's `run/`, and the state file in
/// the log folder.
pub const REQUEST_FILE: &str = "upgrade.request";
pub const CANCEL_FILE: &str = "upgrade.cancel";
pub const READY_FILE: &str = "upgrade.ready";
pub const STATE_FILE: &str = "upgrade.json";

/// One upgrade request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub version: String,
    /// 0: restart at once.
    pub minutes: u32,
    /// How long a minute of the countdown is ([`MINUTE_MS`] but in tests).
    pub minute_ms: u64,
}

impl Request {
    /// A request for `version` in `minutes`, with a test's `minute_ms`.
    pub fn new(version: &str, minutes: u32, minute_ms: u64) -> Result<Request, String> {
        if !valid_update_version(version) {
            return Err(format!(
                "{version:?} is not a version (digits and dots, as 0.1.3.0)"
            ));
        }
        if minutes > MAX_MINUTES {
            return Err(format!("{minutes} minutes is longer than a day"));
        }
        if minute_ms < MIN_MINUTE_MS {
            return Err(format!("a minute of {minute_ms} ms is too short"));
        }
        Ok(Request {
            version: version.to_owned(),
            minutes,
            minute_ms,
        })
    }

    /// `run/upgrade.request`'s text: `version=V`, `minutes=N` and (a test
    /// setup's) `minute_ms=N` lines. `minute_ms` counts only when
    /// `dev_test`.
    pub fn parse_file(text: &str, dev_test: bool) -> Result<Request, String> {
        let (mut version, mut minutes, mut minute_ms) = (None, DEFAULT_MINUTES, MINUTE_MS);
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("{line:?} is not key=value"))?;
            let v = v.trim();
            match k.trim() {
                "version" => version = Some(v.to_owned()),
                "minutes" => {
                    minutes = v
                        .parse()
                        .map_err(|_| format!("minutes={v:?} is not a number"))?
                }
                "minute_ms" if dev_test => {
                    minute_ms = v
                        .parse()
                        .map_err(|_| format!("minute_ms={v:?} is not a number"))?
                }
                "minute_ms" => {}
                other => return Err(format!("unknown key {other:?}")),
            }
        }
        let version = version.ok_or("no version=")?;
        Request::new(&version, minutes, minute_ms)
    }
}

/// How long before the restart the countdown speaks, longest first: the
/// whole length, then 10, 5, 4, 3, 2, 1 minutes and 30 seconds when they
/// are shorter. Empty for a restart at once.
pub fn marks(minutes: u32, minute_ms: u64) -> Vec<u64> {
    let total = u64::from(minutes) * minute_ms;
    if total == 0 {
        return Vec::new();
    }
    let mut out = vec![total];
    let rest = MARK_MINUTES
        .iter()
        .map(|m| m * minute_ms)
        .chain([minute_ms / 2]);
    out.extend(rest.filter(|&left| left < total));
    out
}

/// The countdown's line `left_ms` before the restart, in minutes of
/// `minute_ms` (seconds under a minute).
pub fn line(left_ms: u64, minute_ms: u64) -> String {
    if left_ms >= minute_ms {
        let n = (left_ms + minute_ms / 2) / minute_ms;
        let unit = if n == 1 { "minute" } else { "minutes" };
        format!("Restarting for an update in {n} {unit}")
    } else {
        let s = (left_ms * 60).div_ceil(minute_ms);
        let unit = if s == 1 { "second" } else { "seconds" };
        format!("Restarting for an update in {s} {unit}")
    }
}

/// Where an upgrade is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Countdown,
    /// Zero: the game servers save and restart.
    Restart,
    /// Every game server saved: `server-ctl` swaps the main server.
    Swap,
}

impl Phase {
    fn word(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Countdown => "countdown",
            Phase::Restart => "restart",
            Phase::Swap => "swap",
        }
    }
}

/// What the countdown does now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// A countdown line: in Global, and `Update(countdown)` to every game
    /// server.
    Line {
        version: String,
        seconds: u32,
        text: String,
    },
    /// Zero: the last line, and `Update(restart)` to every game server.
    Restart { version: String, text: String },
    /// Called off: the line, and `Update(cancel)` to every game server.
    Cancel {
        version: String,
        text: String,
        why: String,
    },
    /// Every game server saved (or is gone): the main server may be
    /// swapped.
    Swap { version: String },
}

impl Action {
    /// The line for Global and the games, if the action has one.
    pub fn text(&self) -> Option<&str> {
        match self {
            Action::Line { text, .. }
            | Action::Restart { text, .. }
            | Action::Cancel { text, .. } => Some(text),
            Action::Swap { .. } => None,
        }
    }

    /// The message for every linked game server, if the action has one.
    pub fn to_nodes(&self) -> Option<RealmToNode> {
        let (phase, version, seconds, text) = match self {
            Action::Line {
                version,
                seconds,
                text,
            } => (update_phase::COUNTDOWN, version, *seconds, text.as_str()),
            Action::Restart { version, text } => (update_phase::RESTART, version, 0, text.as_str()),
            Action::Cancel { version, text, .. } => {
                (update_phase::CANCEL, version, 0, text.as_str())
            }
            Action::Swap { .. } => return None,
        };
        Some(RealmToNode::Update {
            phase,
            version: version.clone(),
            seconds,
            text: text.to_owned(),
        })
    }
}

#[derive(Debug)]
struct State {
    phase: Phase,
    version: String,
    minute_ms: u64,
    started: Instant,
    total_ms: u64,
    /// [`marks`], and the next one to speak.
    marks: Vec<u64>,
    next: usize,
    /// The main server's own package is staged.
    ready: bool,
    /// When the countdown reached zero (waiting for `ready`).
    zero_at: Option<Instant>,
    /// When the restart began, and the game servers not yet restarting.
    restart_at: Option<Instant>,
    waiting: BTreeSet<String>,
    /// Idle after a cancel (of `version`).
    cancelled: bool,
}

impl State {
    fn idle() -> State {
        State {
            phase: Phase::Idle,
            version: String::new(),
            minute_ms: MINUTE_MS,
            started: Instant::now(),
            total_ms: 0,
            marks: Vec::new(),
            next: 0,
            ready: false,
            zero_at: None,
            restart_at: None,
            waiting: BTreeSet::new(),
            cancelled: false,
        }
    }

    /// Idle after the countdown to `version` was called off.
    fn cancelled(version: &str) -> State {
        State {
            version: version.to_owned(),
            cancelled: true,
            ..State::idle()
        }
    }

    fn left_ms(&self, now: Instant) -> u64 {
        let gone = u64::try_from(now.saturating_duration_since(self.started).as_millis())
            .unwrap_or(u64::MAX);
        self.total_ms.saturating_sub(gone)
    }
}

/// Seconds as an update message carries them (rounded up).
fn seconds(ms: u64) -> u32 {
    u32::try_from(ms.div_ceil(1000)).unwrap_or(u32::MAX)
}

/// The main server's upgrade: its phase and countdown. Shared by the
/// request driver ([`drive`]), the dev console and the game-server links.
#[derive(Debug)]
pub struct Upgrade {
    /// The version this main server runs.
    own: String,
    state: Mutex<State>,
}

impl Upgrade {
    pub fn new(own: &str) -> Upgrade {
        Upgrade {
            own: own.to_owned(),
            state: Mutex::new(State::idle()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn phase(&self) -> Phase {
        self.lock().phase
    }

    /// Starts the countdown of `req`. `ready`: the main server's own
    /// package is staged already (a test setup has none to fetch).
    pub fn start(&self, req: &Request, now: Instant, ready: bool) -> Result<String, String> {
        if req.version == self.own {
            return Err(format!("the main server runs {} already", self.own));
        }
        let mut s = self.lock();
        if s.phase != Phase::Idle {
            return Err(format!(
                "an upgrade to {} is under way ({}); cancel it first",
                s.version,
                s.phase.word()
            ));
        }
        *s = State {
            phase: Phase::Countdown,
            version: req.version.clone(),
            minute_ms: req.minute_ms,
            started: now,
            total_ms: u64::from(req.minutes) * req.minute_ms,
            marks: marks(req.minutes, req.minute_ms),
            next: 0,
            ready,
            ..State::idle()
        };
        Ok(format!(
            "upgrade to {} in {} min{}",
            req.version,
            req.minutes,
            if req.minute_ms == MINUTE_MS {
                String::new()
            } else {
                format!(" of {} ms", req.minute_ms)
            }
        ))
    }

    /// Calls the countdown off (not once the restart began): the cancel's
    /// action.
    pub fn cancel(&self, why: &str) -> Result<Action, String> {
        let mut s = self.lock();
        match s.phase {
            Phase::Countdown => {}
            Phase::Idle => return Err("no upgrade is under way".into()),
            Phase::Restart | Phase::Swap => {
                return Err(format!(
                    "the restart for {} began; it can't be called off",
                    s.version
                ))
            }
        }
        let version = s.version.clone();
        *s = State::cancelled(&version);
        Ok(Action::Cancel {
            version,
            text: CANCEL_TEXT.into(),
            why: why.to_owned(),
        })
    }

    /// The main server's own package is staged.
    pub fn set_ready(&self) {
        let mut s = self.lock();
        if s.phase == Phase::Countdown {
            s.ready = true;
        }
    }

    /// Game server `id` saved its games and restarts.
    pub fn node_restarting(&self, id: &str) {
        self.lock().waiting.remove(id);
    }

    /// Moves the countdown to `now`; `linked`: the game servers linked now.
    pub fn tick(&self, now: Instant, linked: &[String]) -> Vec<Action> {
        let mut out = Vec::new();
        let mut s = self.lock();
        match s.phase {
            Phase::Idle | Phase::Swap => {}
            Phase::Countdown => {
                let left = s.left_ms(now);
                while s.next < s.marks.len() && left <= s.marks[s.next] {
                    let mark = s.marks[s.next];
                    s.next += 1;
                    // A mark passed while another was due is skipped: only
                    // the latest one is spoken.
                    if s.next < s.marks.len() && left <= s.marks[s.next] {
                        continue;
                    }
                    out.push(Action::Line {
                        version: s.version.clone(),
                        seconds: seconds(left),
                        text: line(mark, s.minute_ms),
                    });
                }
                if left == 0 {
                    let zero = *s.zero_at.get_or_insert(now);
                    if s.ready {
                        s.phase = Phase::Restart;
                        s.restart_at = Some(now);
                        s.waiting = linked.iter().cloned().collect();
                        out.push(Action::Restart {
                            version: s.version.clone(),
                            text: NOW_TEXT.into(),
                        });
                    } else if now.saturating_duration_since(zero) >= READY_WAIT {
                        let version = s.version.clone();
                        *s = State::cancelled(&version);
                        out.push(Action::Cancel {
                            version,
                            text: CANCEL_TEXT.into(),
                            why: "the main server's package wasn't ready".into(),
                        });
                    }
                }
            }
            Phase::Restart => {}
        }
        if s.phase == Phase::Restart {
            s.waiting.retain(|id| linked.contains(id));
            let since = s.restart_at.unwrap_or(now);
            if s.waiting.is_empty() || now.saturating_duration_since(since) >= RESTART_WAIT {
                s.phase = Phase::Swap;
                out.push(Action::Swap {
                    version: s.version.clone(),
                });
            }
        }
        out
    }

    /// What a game server that links now is told after its welcome: the
    /// countdown so far (it fetches the version), or the restart.
    pub fn for_new_link(&self, now: Instant) -> Option<RealmToNode> {
        let s = self.lock();
        let (phase, seconds) = match s.phase {
            Phase::Countdown => (update_phase::COUNTDOWN, seconds(s.left_ms(now))),
            Phase::Restart | Phase::Swap => (update_phase::RESTART, 0),
            Phase::Idle => return None,
        };
        Some(RealmToNode::Update {
            phase,
            version: s.version.clone(),
            seconds,
            text: String::new(),
        })
    }

    /// Whether a game server refused for running `theirs` gets
    /// `Update(catch-up)` to this main server's version: not when it runs
    /// the version an upgrade under way restarts for (it waits for the new
    /// main server instead). `Err`: the refusal's text for that case.
    pub fn catch_up(&self, theirs: &str) -> Result<RealmToNode, String> {
        let s = self.lock();
        if s.phase != Phase::Idle && s.version == theirs {
            return Err(format!(
                "the main server is restarting for {theirs}; link again in a moment"
            ));
        }
        Ok(RealmToNode::Update {
            phase: update_phase::CATCH_UP,
            version: self.own.clone(),
            seconds: 0,
            text: String::new(),
        })
    }

    /// `logs/upgrade.json`'s text.
    pub fn json(&self, now: Instant) -> String {
        let s = self.lock();
        let left = match s.phase {
            Phase::Countdown => s.left_ms(now).div_ceil(1000),
            _ => 0,
        };
        let phase = match s.phase {
            Phase::Idle if s.cancelled => "cancelled",
            p => p.word(),
        };
        format!(
            "{{\"phase\": \"{}\", \"version\": \"{}\", \"seconds_left\": {}, \"ready\": {}, \"waiting\": {}}}\n",
            phase,
            s.version,
            left,
            s.ready,
            s.waiting.len()
        )
    }
}

/// Where the countdown's lines and messages go: Global, and the linked
/// game servers.
pub trait Outlet: Send + Sync {
    /// A realm notice in Global.
    fn global(&self, text: &str);
    /// A message to every linked game server.
    fn nodes(&self, m: &RealmToNode);
    /// The ids of the game servers linked now.
    fn linked(&self) -> Vec<String>;
}

/// Does what the countdown says now.
pub fn act(actions: &[Action], out: &dyn Outlet, log: &ServerLog) {
    for a in actions {
        match a {
            Action::Line { version, text, .. } => {
                log.line(&format!("upgrade to {version}: {text}"))
            }
            Action::Restart { version, .. } => log.line(&format!(
                "upgrade to {version}: zero; the game servers save and restart"
            )),
            Action::Cancel { version, why, .. } => {
                log.line(&format!("upgrade to {version} called off: {why}"))
            }
            Action::Swap { version } => log.line(&format!(
                "upgrade to {version}: the game servers saved; ready to be swapped"
            )),
        }
        if let Some(text) = a.text() {
            out.global(text);
        }
        if let Some(m) = a.to_nodes() {
            out.nodes(&m);
        }
    }
}

/// Runs the upgrade for the server folder `folder` (its `run/` request
/// files) and the log folder `logs` (`upgrade.json`): every [`STEP`],
/// until the server stops.
pub async fn drive(
    up: Arc<Upgrade>,
    out: Arc<dyn Outlet>,
    folder: PathBuf,
    logs: PathBuf,
    dev_test: bool,
    log: Arc<ServerLog>,
) {
    let run = folder.join("run");
    let mut every = tokio::time::interval(STEP);
    let mut written = String::new();
    loop {
        every.tick().await;
        let now = Instant::now();
        let mut actions = Vec::new();
        if let Some(text) = take(&run.join(REQUEST_FILE)) {
            let started =
                Request::parse_file(&text, dev_test).and_then(|r| up.start(&r, now, false));
            match started {
                Ok(t) => log.line(&format!("{t} (asked by server-ctl)")),
                Err(e) => log.line(&format!("upgrade request refused: {e}")),
            }
        }
        if let Some(why) = take(&run.join(CANCEL_FILE)) {
            let why = match why.trim() {
                "" => "server-ctl upgrade --cancel".to_owned(),
                w => w.to_owned(),
            };
            match up.cancel(&why) {
                Ok(a) => actions.push(a),
                Err(e) => log.line(&format!("upgrade cancel: {e}")),
            }
        }
        if run.join(READY_FILE).exists() {
            up.set_ready();
        }
        actions.extend(up.tick(now, &out.linked()));
        act(&actions, out.as_ref(), &log);
        if actions
            .iter()
            .any(|a| matches!(a, Action::Cancel { .. } | Action::Swap { .. }))
        {
            let _ = std::fs::remove_file(run.join(READY_FILE));
        }
        let json = up.json(now);
        if json != written {
            match write_state(&logs, &json) {
                Ok(()) => written = json,
                Err(e) => log.line(&format!("{STATE_FILE}: {e}")),
            }
        }
    }
}

/// A request file's text, removed as it is read.
fn take(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let _ = std::fs::remove_file(path);
    Some(text)
}

/// Writes `logs/upgrade.json` (a temporary file renamed over it).
fn write_state(logs: &Path, json: &str) -> std::io::Result<()> {
    let tmp = logs.join(format!("{STATE_FILE}.tmp"));
    std::fs::write(&tmp, json)?;
    std::fs::rename(tmp, logs.join(STATE_FILE))
}

#[cfg(test)]
#[path = "upgrade_tests.rs"]
mod tests;
