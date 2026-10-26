//! [`HostProcess`]: the game server program started by the game client as
//! its own process, driven over its stdin and stdout lines and watched
//! until it exits.

use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::lines::{
    parse_node_line, wipe, GameInfo, HostCommand, LineError, NodeError, NodeLine, Ready, StartLine,
    MAX_LINE,
};
use crate::power::is_sleep_notice;
use crate::reach::{support_details, ReachReport, ReachView};

/// The game server program as the game package ships it, beside the game
/// client's own executable.
pub const NODE_PROGRAM: &str = if cfg!(windows) {
    "D2RUST-Node.exe"
} else {
    "d2rust-node"
};

/// How long a game server may take to save, close and exit after `stop`
/// (or after its stdin closed) before it is killed (contracts 11.12).
pub const STOP_GRACE: Duration = Duration::from_secs(10);

/// Whether the hosted game ends when the host leaves it (plan, open
/// question 9; proposed: it ends). When it does, leaving stops the game
/// server: it saves everyone and closes the game.
pub const GAME_ENDS_WHEN_HOST_LEAVES: bool = true;

/// Restarts of a game server that crashed (proposed default): one, only
/// while it hasn't linked yet. Once linked it holds the game, which a new
/// process can't bring back: the main server ends that game and its
/// players return to the lobby with their last checkpoint (plan N13), and
/// the host may host again.
pub const DEFAULT_RESTART: RestartPolicy = RestartPolicy {
    before_ready: 1,
    after_ready: 0,
};

/// How many of the game server's last stderr lines are kept (for a crash
/// report or support).
pub const STDERR_TAIL: usize = 20;

/// How often the supervisor looks at the process between messages.
const TICK: Duration = Duration::from_millis(50);

/// How long after the process exited its last output may still arrive.
const OUTPUT_DRAIN: Duration = Duration::from_secs(1);

/// The game server program in a folder.
pub fn node_program_in(dir: &Path) -> PathBuf {
    dir.join(NODE_PROGRAM)
}

/// The game server program beside the running executable.
pub fn node_program_beside_exe() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let dir = exe
        .parent()
        .ok_or_else(|| io::Error::other("the executable has no folder"))?;
    Ok(node_program_in(dir))
}

/// When a crashed game server is started again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestartPolicy {
    /// Restarts while it hasn't printed its ready line.
    pub before_ready: u32,
    /// Restarts after it was ready.
    pub after_ready: u32,
}

/// How to start the hosting process.
#[derive(Clone, Debug)]
pub struct HostConfig {
    pub program: PathBuf,
    /// Its arguments: `--host`. Never the token (that is only on stdin).
    pub args: Vec<String>,
    /// Its working folder (the client's own when `None`).
    pub dir: Option<PathBuf>,
    pub start: StartLine,
    pub restart: RestartPolicy,
    /// [`STOP_GRACE`].
    pub stop_grace: Duration,
}

impl HostConfig {
    pub fn new(program: impl Into<PathBuf>, start: StartLine) -> Self {
        HostConfig {
            program: program.into(),
            args: vec!["--host".into()],
            dir: None,
            start,
            restart: DEFAULT_RESTART,
            stop_grace: STOP_GRACE,
        }
    }

    /// The command that starts it: pipes for stdin, stdout and stderr, no
    /// console window on Windows, and on Unix its own process group, so a
    /// Ctrl-C meant for the game client doesn't kill it before it saves
    /// (it stops when its stdin closes).
    pub fn command(&self) -> Command {
        let mut c = Command::new(&self.program);
        c.args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(d) = &self.dir {
            c.current_dir(d);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            c.creation_flags(CREATE_NO_WINDOW);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            c.process_group(0);
        }
        c
    }
}

/// Why hosting couldn't start.
#[derive(Debug)]
pub enum HostError {
    /// A start line field is wrong.
    Start(LineError),
    /// The program couldn't be started (missing from the package, for one).
    Spawn { program: PathBuf, source: io::Error },
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::Start(e) => write!(f, "the start line: {e}"),
            HostError::Spawn { program, source } => {
                write!(f, "starting {}: {source}", program.display())
            }
        }
    }
}

impl std::error::Error for HostError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HostError::Start(e) => Some(e),
            HostError::Spawn { source, .. } => Some(source),
        }
    }
}

/// How the game server process ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitKind {
    /// Exit code 0: it saved, closed and exited.
    Clean,
    /// Another exit code: it gave up on its own (after an `error` line).
    Failed(i32),
    /// A panic (exit code 101), a signal on Unix, or an exception code on
    /// Windows.
    Crashed(Option<i32>),
    /// It didn't exit within the grace time after `stop` and was killed.
    Killed,
}

impl ExitKind {
    pub fn of(status: ExitStatus) -> Self {
        match status.code() {
            Some(0) => ExitKind::Clean,
            // Rust's exit code after a panic.
            Some(101) => ExitKind::Crashed(Some(101)),
            // Windows exception codes (NTSTATUS errors: 0xC0000005 and kin).
            Some(c) if cfg!(windows) && (c as u32) >= 0xC000_0000 => ExitKind::Crashed(Some(c)),
            Some(c) => ExitKind::Failed(c),
            None => ExitKind::Crashed(None),
        }
    }

    pub fn is_crash(self) -> bool {
        matches!(self, ExitKind::Crashed(_))
    }
}

/// Where the hosting process is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Started, not linked yet.
    Starting,
    /// Linked to the main server (its ready line came).
    Ready,
    /// Asked to stop: saving and closing.
    Stopping,
    /// Not running.
    Exited,
}

/// What is known about the hosting process now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostStatus {
    pub phase: Phase,
    pub pid: Option<u32>,
    /// How many times it was started.
    pub starts: u32,
    pub ready: Option<Ready>,
    /// The last `reach` line (cleared by "Check again").
    pub reach: Option<ReachReport>,
    pub game: Option<GameInfo>,
    /// The reason of the last `closed` line.
    pub closed: Option<String>,
    pub error: Option<NodeError>,
    pub exit: Option<ExitKind>,
    /// Its last [`STDERR_TAIL`] stderr lines.
    pub stderr_tail: Vec<String>,
}

impl HostStatus {
    fn new() -> Self {
        HostStatus {
            phase: Phase::Starting,
            pid: None,
            starts: 0,
            ready: None,
            reach: None,
            game: None,
            closed: None,
            error: None,
            exit: None,
            stderr_tail: Vec::new(),
        }
    }

    /// What the hosting panel shows about reachability.
    pub fn reach_view(&self) -> ReachView {
        match (self.phase, &self.ready, &self.reach) {
            (Phase::Exited, _, _) => ReachView::Stopped,
            (_, None, _) => ReachView::Starting,
            (_, Some(_), None) => ReachView::Opening,
            (_, Some(_), Some(r)) => ReachView::of(r),
        }
    }

    /// "Copy details for support".
    pub fn support_details(&self) -> String {
        support_details(self.ready.as_ref(), self.reach.as_ref())
    }
}

/// Something that happened to the hosting process, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostEvent {
    /// A process was started (`attempt` 1, then 2.. for restarts).
    Started {
        pid: u32,
        attempt: u32,
    },
    Ready(Ready),
    Reach(ReachReport),
    Game(GameInfo),
    Closed {
        reason: String,
    },
    Error(NodeError),
    /// The process exited; `restart`: it is being started again.
    Exited {
        how: ExitKind,
        restart: bool,
    },
}

enum Inbox {
    Command(HostCommand),
    /// The handle was dropped: close stdin, reap in the background.
    Detach,
    Out(u32, String),
    Err(u32, String),
    Eof(u32),
}

/// The hosting process of "Host on my PC" (plan N8): `d2rust-node --host`
/// as its own process. Commands go to its stdin; its lines come back as
/// [`HostEvent`]s and in [`HostStatus`]. Dropping the handle closes its
/// stdin, so the game server saves everyone, closes the game and exits
/// (and if it doesn't, it is killed after the grace time). If the game
/// client crashes, the system closes the pipe, with the same effect.
pub struct HostProcess {
    inbox: Sender<Inbox>,
    events: Receiver<HostEvent>,
    status: Arc<Mutex<HostStatus>>,
    supervisor: Option<JoinHandle<()>>,
}

impl HostProcess {
    /// Starts the game server and writes its start line.
    pub fn spawn(cfg: HostConfig) -> Result<Self, HostError> {
        cfg.start.validate().map_err(HostError::Start)?;
        let (inbox, inbox_rx) = mpsc::channel();
        let (events_tx, events) = mpsc::channel();
        let status = Arc::new(Mutex::new(HostStatus::new()));
        let run = Run::start(&cfg, 1, &inbox)?;
        {
            let mut st = lock(&status);
            st.pid = Some(run.child.id());
            st.starts = 1;
        }
        let _ = events_tx.send(HostEvent::Started {
            pid: run.child.id(),
            attempt: 1,
        });
        let sup = Supervisor {
            cfg,
            inbox: inbox_rx,
            inbox_tx: inbox.clone(),
            events: events_tx,
            status: status.clone(),
            run,
            ending: false,
            deadline: None,
            restarts_before: 0,
            restarts_after: 0,
        };
        let supervisor = std::thread::Builder::new()
            .name("host-control".into())
            .spawn(move || sup.run())
            .map_err(|source| HostError::Spawn {
                program: PathBuf::from("host-control thread"),
                source,
            })?;
        Ok(HostProcess {
            inbox,
            events,
            status,
            supervisor: Some(supervisor),
        })
    }

    pub fn send(&self, c: HostCommand) {
        let _ = self.inbox.send(Inbox::Command(c));
    }

    /// The PC is about to sleep: save everyone and close the game.
    pub fn sleep(&self) {
        self.send(HostCommand::Sleep);
    }

    /// Stop hosting: save, close, exit (killed after the grace time).
    pub fn stop(&self) {
        self.send(HostCommand::Stop);
    }

    /// "Check again" in the reachability panel.
    pub fn recheck(&self) {
        self.send(HostCommand::Recheck);
    }

    /// A window message from the game client's window: passes the Windows
    /// sleep notice on as `sleep`. True when it was one.
    pub fn power_message(&self, msg: u32, wparam: usize) -> bool {
        let sleep = is_sleep_notice(msg, wparam);
        if sleep {
            self.sleep();
        }
        sleep
    }

    /// The host left its own game. With [`GAME_ENDS_WHEN_HOST_LEAVES`]
    /// the game server is stopped; true when it was.
    pub fn host_left_game(&self) -> bool {
        if GAME_ENDS_WHEN_HOST_LEAVES {
            self.stop();
        }
        GAME_ENDS_WHEN_HOST_LEAVES
    }

    /// The next event, if one is waiting (for the frame loop).
    pub fn poll(&self) -> Option<HostEvent> {
        self.events.try_recv().ok()
    }

    /// The next event, waiting up to `timeout`.
    pub fn next_event(&self, timeout: Duration) -> Option<HostEvent> {
        self.events.recv_timeout(timeout).ok()
    }

    pub fn status(&self) -> HostStatus {
        lock(&self.status).clone()
    }

    pub fn is_running(&self) -> bool {
        lock(&self.status).phase != Phase::Exited
    }

    /// Stops hosting and waits until the process is gone (at most the
    /// grace time, then it is killed). Returns how it ended.
    pub fn quit(mut self) -> Option<ExitKind> {
        self.stop();
        if let Some(s) = self.supervisor.take() {
            let _ = s.join();
        }
        lock(&self.status).exit
    }
}

impl Drop for HostProcess {
    fn drop(&mut self) {
        let _ = self.inbox.send(Inbox::Detach);
    }
}

fn lock(m: &Mutex<HostStatus>) -> MutexGuard<'_, HostStatus> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// One started process.
struct Run {
    child: Child,
    stdin: Option<ChildStdin>,
    /// Which start this is: output of an earlier process is ignored.
    gen: u32,
    exit: Option<(ExitKind, Instant)>,
    eof: bool,
    killed: bool,
    ready: bool,
}

impl Run {
    fn start(cfg: &HostConfig, gen: u32, inbox: &Sender<Inbox>) -> Result<Self, HostError> {
        let mut child = cfg.command().spawn().map_err(|source| HostError::Spawn {
            program: cfg.program.clone(),
            source,
        })?;
        let mut stdin = child.stdin.take();
        if let Some(out) = child.stdout.take() {
            read_lines(out, gen, inbox.clone(), false);
        }
        if let Some(err) = child.stderr.take() {
            read_lines(err, gen, inbox.clone(), true);
        }
        if let Some(pipe) = &mut stdin {
            let mut line = cfg.start.render();
            let wrote = pipe.write_all(&line).and_then(|()| pipe.flush());
            wipe(&mut line);
            // A process that is already gone shows up as its exit.
            if wrote.is_err() {
                stdin = None;
            }
        }
        Ok(Run {
            child,
            stdin,
            gen,
            exit: None,
            eof: false,
            killed: false,
            ready: false,
        })
    }
}

/// Reads a pipe line by line on its own thread. Lines over [`MAX_LINE`]
/// are skipped whole.
fn read_lines(src: impl Read + Send + 'static, gen: u32, tx: Sender<Inbox>, stderr: bool) {
    std::thread::spawn(move || {
        let mut r = BufReader::new(src);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match r
                .by_ref()
                .take(MAX_LINE as u64 + 1)
                .read_until(b'\n', &mut buf)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            if buf.last() != Some(&b'\n') && buf.len() > MAX_LINE {
                if !skip_line(&mut r) {
                    break;
                }
                continue;
            }
            let line = String::from_utf8_lossy(&buf)
                .trim_end_matches(['\r', '\n'])
                .to_string();
            let msg = if stderr {
                Inbox::Err(gen, line)
            } else {
                Inbox::Out(gen, line)
            };
            if tx.send(msg).is_err() {
                return;
            }
        }
        if !stderr {
            let _ = tx.send(Inbox::Eof(gen));
        }
    });
}

/// Reads up to the end of the current line; false at the end of the pipe.
fn skip_line(r: &mut impl BufRead) -> bool {
    let mut scratch = Vec::new();
    loop {
        scratch.clear();
        match r.by_ref().take(4096).read_until(b'\n', &mut scratch) {
            Ok(0) | Err(_) => return false,
            Ok(_) if scratch.last() == Some(&b'\n') => return true,
            Ok(_) => {}
        }
    }
}

struct Supervisor {
    cfg: HostConfig,
    inbox: Receiver<Inbox>,
    inbox_tx: Sender<Inbox>,
    events: Sender<HostEvent>,
    status: Arc<Mutex<HostStatus>>,
    run: Run,
    /// Stop asked for or the handle dropped: no restart.
    ending: bool,
    /// When the process gets killed if it is still running.
    deadline: Option<Instant>,
    restarts_before: u32,
    restarts_after: u32,
}

impl Supervisor {
    fn emit(&self, e: HostEvent) {
        let _ = self.events.send(e);
    }

    fn run(mut self) {
        loop {
            match self.inbox.recv_timeout(TICK) {
                Ok(Inbox::Out(g, l)) if g == self.run.gen => self.line(&l),
                Ok(Inbox::Err(g, l)) if g == self.run.gen => {
                    let mut st = lock(&self.status);
                    if st.stderr_tail.len() == STDERR_TAIL {
                        st.stderr_tail.remove(0);
                    }
                    st.stderr_tail.push(l);
                }
                Ok(Inbox::Eof(g)) if g == self.run.gen => self.run.eof = true,
                Ok(Inbox::Command(c)) => self.command(c),
                Ok(Inbox::Detach) => {
                    // Its stdin closes: the game server saves, closes and exits.
                    self.run.stdin = None;
                    self.end();
                }
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if self.run.exit.is_none() {
                self.reap();
            }
            // Its last lines first, then the exit.
            if let Some((how, at)) = self.run.exit {
                if (self.run.eof || at.elapsed() >= OUTPUT_DRAIN) && !self.exited(how) {
                    break;
                }
            }
        }
    }

    fn end(&mut self) {
        self.ending = true;
        let grace = self.cfg.stop_grace;
        self.deadline.get_or_insert_with(|| Instant::now() + grace);
        let mut st = lock(&self.status);
        if st.phase != Phase::Exited {
            st.phase = Phase::Stopping;
        }
    }

    fn command(&mut self, c: HostCommand) {
        if let Some(pipe) = &mut self.run.stdin {
            let wrote = pipe
                .write_all(c.line().as_bytes())
                .and_then(|()| pipe.write_all(b"\n"))
                .and_then(|()| pipe.flush());
            if wrote.is_err() {
                self.run.stdin = None;
            }
        }
        match c {
            HostCommand::Stop => self.end(),
            // The panel shows "Opening port..." until the new answer.
            HostCommand::Recheck => lock(&self.status).reach = None,
            HostCommand::Sleep => {}
        }
    }

    fn line(&mut self, l: &str) {
        // Unknown and broken lines are ignored (contracts 11.12).
        let Ok(line) = parse_node_line(l) else {
            return;
        };
        {
            let mut st = lock(&self.status);
            match &line {
                NodeLine::Ready(r) => {
                    self.run.ready = true;
                    st.ready = Some(r.clone());
                    if st.phase == Phase::Starting {
                        st.phase = Phase::Ready;
                    }
                }
                NodeLine::Reach(r) => st.reach = Some(*r),
                NodeLine::Game(g) => st.game = Some(*g),
                NodeLine::Closed { reason } => {
                    st.closed = Some(reason.clone());
                    st.game = None;
                }
                NodeLine::Error(e) => st.error = Some(e.clone()),
            }
        }
        self.emit(match line {
            NodeLine::Ready(r) => HostEvent::Ready(r),
            NodeLine::Reach(r) => HostEvent::Reach(r),
            NodeLine::Game(g) => HostEvent::Game(g),
            NodeLine::Closed { reason } => HostEvent::Closed { reason },
            NodeLine::Error(e) => HostEvent::Error(e),
        });
    }

    fn reap(&mut self) {
        match self.run.child.try_wait() {
            Ok(Some(status)) => {
                let how = if self.run.killed {
                    ExitKind::Killed
                } else {
                    ExitKind::of(status)
                };
                self.run.stdin = None;
                self.run.exit = Some((how, Instant::now()));
            }
            Ok(None) => {
                if !self.run.killed && self.deadline.is_some_and(|d| Instant::now() >= d) {
                    let _ = self.run.child.kill();
                    self.run.killed = true;
                }
            }
            Err(_) => {
                let _ = self.run.child.kill();
                self.run.killed = true;
            }
        }
    }

    /// The process is gone. Starts it again when the policy allows; false
    /// when supervising is over.
    fn exited(&mut self, how: ExitKind) -> bool {
        let restart = !self.ending && how.is_crash() && self.may_restart();
        {
            let mut st = lock(&self.status);
            st.exit = Some(how);
            st.pid = None;
            st.phase = if restart {
                Phase::Starting
            } else {
                Phase::Exited
            };
        }
        self.emit(HostEvent::Exited { how, restart });
        if !restart {
            return false;
        }
        let gen = self.run.gen + 1;
        match Run::start(&self.cfg, gen, &self.inbox_tx) {
            Ok(run) => {
                let pid = run.child.id();
                self.run = run;
                {
                    let mut st = lock(&self.status);
                    st.pid = Some(pid);
                    st.starts += 1;
                    st.ready = None;
                    st.reach = None;
                    st.game = None;
                    st.closed = None;
                    st.error = None;
                    st.exit = None;
                }
                self.emit(HostEvent::Started { pid, attempt: gen });
                true
            }
            Err(e) => {
                let err = NodeError {
                    code: "start".into(),
                    text: e.to_string(),
                };
                {
                    let mut st = lock(&self.status);
                    st.phase = Phase::Exited;
                    st.error = Some(err.clone());
                }
                self.emit(HostEvent::Error(err));
                false
            }
        }
    }

    fn may_restart(&mut self) -> bool {
        let p = self.cfg.restart;
        if self.run.ready {
            if self.restarts_after < p.after_ready {
                self.restarts_after += 1;
                return true;
            }
        } else if self.restarts_before < p.before_ready {
            self.restarts_before += 1;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> StartLine {
        StartLine::from_grant([0x5a; 16], "127.0.0.1:6112", [1; 32], "0.0.0").unwrap()
    }

    #[test]
    fn the_token_is_never_on_the_command_line() {
        let cfg = HostConfig::new(node_program_in(Path::new("bin")), start());
        let c = cfg.command();
        let all: Vec<String> = std::iter::once(c.get_program())
            .chain(c.get_args())
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(all[1..], ["--host"]);
        let token = "5a".repeat(16);
        assert!(all.iter().all(|a| !a.contains(&token)), "{all:?}");
        assert!(!format!("{cfg:?}").contains(&token));
        assert!(c.get_envs().all(|(_, v)| v
            .map(|v| !v.to_string_lossy().contains(&token))
            .unwrap_or(true)));
    }

    #[test]
    fn the_program_is_the_packaged_name() {
        let p = node_program_in(Path::new("game"));
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        if cfg!(windows) {
            assert_eq!(name, "D2RUST-Node.exe");
        } else {
            assert_eq!(name, "d2rust-node");
        }
    }

    #[cfg(unix)]
    #[test]
    fn exit_kinds_are_told_apart() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(ExitKind::of(ExitStatus::from_raw(0)), ExitKind::Clean);
        assert_eq!(
            ExitKind::of(ExitStatus::from_raw(2 << 8)),
            ExitKind::Failed(2)
        );
        assert_eq!(
            ExitKind::of(ExitStatus::from_raw(101 << 8)),
            ExitKind::Crashed(Some(101))
        );
        // Killed by SIGKILL (9).
        assert_eq!(
            ExitKind::of(ExitStatus::from_raw(9)),
            ExitKind::Crashed(None)
        );
        assert!(ExitKind::Crashed(None).is_crash());
        assert!(!ExitKind::Killed.is_crash());
        assert!(!ExitKind::Failed(2).is_crash());
    }

    #[cfg(windows)]
    #[test]
    fn exit_kinds_are_told_apart() {
        use std::os::windows::process::ExitStatusExt;
        assert_eq!(ExitKind::of(ExitStatus::from_raw(0)), ExitKind::Clean);
        assert_eq!(ExitKind::of(ExitStatus::from_raw(2)), ExitKind::Failed(2));
        let access_violation = 0xC000_0005u32;
        assert_eq!(
            ExitKind::of(ExitStatus::from_raw(access_violation)),
            ExitKind::Crashed(Some(access_violation as i32))
        );
    }

    #[test]
    fn the_reach_view_follows_the_phase() {
        let mut st = HostStatus::new();
        assert_eq!(st.reach_view(), ReachView::Starting);
        st.ready = Some(Ready {
            node: "h-1".into(),
            listen: "127.0.0.1:6113".parse().unwrap(),
            cert_sha256: [0; 32],
        });
        st.phase = Phase::Ready;
        assert_eq!(st.reach_view().text(), "Opening port...");
        st.phase = Phase::Exited;
        assert_eq!(st.reach_view(), ReachView::Stopped);
    }

    #[test]
    fn a_missing_program_is_a_spawn_error() {
        let cfg = HostConfig::new(node_program_in(Path::new("no-such-folder-d2hc")), start());
        match HostProcess::spawn(cfg) {
            Err(HostError::Spawn { program, .. }) => {
                assert!(program.ends_with(NODE_PROGRAM))
            }
            Err(e) => panic!("{e}"),
            Ok(_) => panic!("started a missing program"),
        }
    }
}
