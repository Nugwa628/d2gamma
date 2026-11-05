//! The test arena from Studio: the game's `ai-arena` command in a child
//! process (Studio does not link the game), and the trace it writes read
//! back. The trace and its sums are `d2_sim`'s; this is the run and the
//! small readings the screen draws.
use crate::actions::hide_console;
use crate::ToolCommand;
use anyhow::{bail, Context, Result};
use d2_sim::rules::data_mod::ai::trace::{EventKind, Trace};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    time::{Duration, Instant},
};

/// What Studio asks of the arena (the game's own defaults are the same).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaOptions {
    /// The monstats row to spawn.
    pub class: u32,
    pub ticks: u32,
    pub seed: u32,
    /// Sub-tiles between the monster and the character.
    pub distance: u32,
    /// The level the character is warped to; 0: it stays in town.
    pub level: u32,
}

impl Default for ArenaOptions {
    fn default() -> Self {
        Self {
            class: 0,
            ticks: 500,
            seed: 1234,
            distance: 24,
            level: 2,
        }
    }
}

/// A run in a child process.
pub struct ArenaRun {
    rx: Receiver<Result<Trace, String>>,
    cancel: Arc<AtomicBool>,
    pub started: Instant,
}

impl Drop for ArenaRun {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl ArenaRun {
    /// Starts the arena for the graph `ai` of the project in `root`.
    pub fn start(
        game: PathBuf,
        root: PathBuf,
        native: PathBuf,
        options: ArenaOptions,
        ai: u16,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::spawn(move || {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "studio-arena-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let command =
                ToolCommand::arena(&game, &root, &native, &options, ai, &dir.join("trace.txt"));
            let result = run(&command, &dir, &flag).map_err(|e| format!("{e:#}"));
            let _ = tx.send(result);
            let _ = fs::remove_dir_all(&dir);
        });
        Self {
            rx,
            cancel,
            started: Instant::now(),
        }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// The result, once there is one.
    pub fn poll(&self) -> Option<Result<Trace, String>> {
        self.rx.try_recv().ok()
    }
}

fn run(command: &ToolCommand, dir: &Path, cancel: &AtomicBool) -> Result<Trace> {
    fs::create_dir(dir)?;
    let log_path = dir.join("arena.log");
    let log = fs::File::create(&log_path)?;
    let mut c = command.command();
    c.stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    hide_console(&mut c);
    let mut child = c
        .spawn()
        .context("Could not start the configured game executable")?;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Cancelled");
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let log = fs::read_to_string(&log_path).unwrap_or_default();
    read_result(status.success(), &log, &command_out(command))
}

/// The `--out` of an arena command.
fn command_out(command: &ToolCommand) -> PathBuf {
    command
        .args
        .iter()
        .position(|a| a == "--out")
        .and_then(|i| command.args.get(i + 1))
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// What a finished run gives: the trace, or the end of the game's output.
pub fn read_result(success: bool, log: &str, out: &Path) -> Result<Trace> {
    if !success {
        bail!("The arena failed:\n{}", tail(log, 12));
    }
    let text = fs::read_to_string(out)
        .with_context(|| format!("The arena wrote no trace:\n{}", tail(log, 12)))?;
    Trace::parse(&text).map_err(anyhow::Error::msg)
}

fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// A stretch of ticks in one state (`None`: outside a graph).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stretch {
    pub from: u32,
    pub to: u32,
    pub state: Option<u8>,
}

/// The trace as runs of ticks in the same state, in order.
pub fn stretches(t: &Trace) -> Vec<Stretch> {
    let mut out: Vec<Stretch> = Vec::new();
    for s in &t.samples {
        match out.last_mut() {
            Some(last) if last.state == s.state && last.to + 1 == s.tick => last.to = s.tick,
            _ => out.push(Stretch {
                from: s.tick,
                to: s.tick,
                state: s.state,
            }),
        }
    }
    out
}

/// Ticks spent in each state of the graph, by state number; `None` when the
/// trace is not of a graph with these state names (the file changed since).
pub fn occupancy(t: &Trace, ai: u16, names: &[String]) -> Option<Vec<u32>> {
    if t.ai != ai || t.states != names {
        return None;
    }
    let mut ticks = vec![0; names.len()];
    for s in &t.samples {
        if let Some(n) = s.state {
            *ticks.get_mut(usize::from(n))? += 1;
        }
    }
    Some(ticks)
}

/// The tick of each wind-up and phase, for marks on the strip.
pub fn marks(t: &Trace) -> Vec<(u32, bool)> {
    t.events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::WindUp { .. } => Some((e.tick, false)),
            EventKind::Phase { .. } => Some((e.tick, true)),
            EventKind::State { .. } => None,
        })
        .collect()
}
