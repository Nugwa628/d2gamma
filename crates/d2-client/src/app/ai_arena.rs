//! `d2-client ai-arena`: one monster against one test character, headless,
//! for a number of ticks, with a record of what its AI did (`ai-trace-1`,
//! `d2_sim::rules::data_mod::ai::trace`).
//!
//! The run is [`super::state_dump`]'s: the game `play` builds (a new
//! character, `--seed`), on a stepping clock, with the same bridge and
//! client. The character is warped to `--level` (Blood Moor by default; a
//! town's monsters do not fight), the monster is added by a `spawn` poke
//! [`DISTANCE`] sub-tiles east of it, and after every tick a recorder on
//! the server thread reads the character, the monster and the AI's event
//! list (`AiStore`): the monster's mode, its
//! place in its behaviour graph, the distance and both hit points, and the
//! graph's state changes, wind-ups and phases. With `--mod` / `--mod-dir`
//! the game plays the mod's rules, so the graphs of its `ai.toml` run; with
//! neither the monster runs D2Gamma's own AI, which is the baseline a
//! graph is compared with.
//!
//! The character does nothing: it stands where it arrived. The level's own
//! monsters are in the game too and may come; the record is of the one that
//! was spawned. What a monster does to a character that fights back is
//! outside this tool. Two runs with the same arguments write the same bytes.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context, Result};
use d2_sim::debug::state::{StateSnapshot, UnitState};
use d2_sim::monsters::ai::graph::{GraphEvent, GraphEventKind};
use d2_sim::rules::data_mod::ai::trace::{Event, EventKind, Sample, Trace};
use d2_sim::rules::data_mod::{AiDef, AiRules};
use d2_sim::rules::Ruleset;
use d2_sim::units::UnitType;

use super::single_player::{self, GameData};
use super::state_dump::{self, DumpGame, RunInfo, TickObserver};

/// The tick at whose end the character is warped (a poke for frame n runs at
/// the end of tick n - 1).
pub const WARP_TICK: i32 = 2;

/// The tick at whose end the monster is spawned when the character stays
/// where it joined, and when it was warped (the new level has been built by
/// then). The monster is in the game from the next tick, tick 1 of the
/// trace.
pub const SPAWN_TICK: i32 = 2;
pub const SPAWN_TICK_AFTER_WARP: i32 = 11;

/// The level the character is warped to by default: the Blood Moor, the
/// first field of act 1.
pub const LEVEL: u32 = 2;

/// The tick whose end spawns the monster, for a run on `level`.
pub fn spawn_tick(level: u32) -> i32 {
    if level == 0 {
        SPAWN_TICK
    } else {
        SPAWN_TICK_AFTER_WARP
    }
}

/// The life stat.
const STAT_LIFE: u16 = 6;

/// The default distance between the monster and the character, in
/// sub-tiles (a tile is 5): far enough to be outside most sight ranges.
pub const DISTANCE: u32 = 24;

/// The default number of ticks to record after the spawn.
pub const TICKS: u32 = 500;

/// `ai-arena`'s options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaArgs {
    /// `--class N`: the monstats row to spawn.
    pub class: u32,
    /// `--ai N`: the graph the monster is expected to run (an `index` of
    /// `ai.toml`), for the names in the trace.
    pub ai: Option<u16>,
    /// `--name TEXT`: what the trace calls the monster.
    pub name: Option<String>,
    /// `--seed N`.
    pub seed: u32,
    /// `--ticks N`: ticks to record after the spawn.
    pub ticks: u32,
    /// `--distance N`: sub-tiles between the two at the spawn.
    pub distance: u32,
    /// `--hurt TICK:PERCENT,...`: from the given tick after the spawn the
    /// monster's life is set to that percent of its maximum (once, in tick
    /// order), so hurt phases, retreats and boss phase changes happen.
    pub hurt: Vec<(u32, u32)>,
    /// `--level N`: the level the character is warped to; 0: it stays in
    /// the town it joined.
    pub level: u32,
    /// `--difficulty normal|nightmare|hell|0-2`.
    pub difficulty: u8,
    /// `--out FILE`: where the trace is written; none: the caller prints it.
    pub out: Option<PathBuf>,
    /// `--native DIR`: a converted game folder (a mod needs one).
    pub native: Option<PathBuf>,
    /// `--game-dir DIR`, else `$D2_GAME_DIR`.
    pub game_dir: Option<PathBuf>,
}

/// Parses the options after `ai-arena`.
pub fn parse_args(args: &[String]) -> Result<ArenaArgs> {
    let mut a = ArenaArgs {
        class: 0,
        ai: None,
        name: None,
        seed: single_player::DEFAULT_SEED,
        ticks: TICKS,
        distance: DISTANCE,
        hurt: Vec::new(),
        level: LEVEL,
        difficulty: 0,
        out: None,
        native: None,
        game_dir: None,
    };
    let mut class = None;
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().with_context(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--class" => class = Some(value()?.parse::<u32>().context("--class")?),
            "--ai" => a.ai = Some(value()?.parse::<u16>().context("--ai")?),
            "--name" => a.name = Some(value()?.clone()),
            "--seed" => a.seed = value()?.parse().context("--seed")?,
            "--ticks" => a.ticks = value()?.parse().context("--ticks")?,
            "--distance" => a.distance = value()?.parse().context("--distance")?,
            "--hurt" => a.hurt = parse_hurt(value()?)?,
            "--level" => a.level = value()?.parse().context("--level")?,
            "--difficulty" => {
                let v = value()?;
                a.difficulty = single_player::parse_difficulty(v).with_context(|| {
                    format!("--difficulty {v}: use normal, nightmare, hell or 0-2")
                })?;
            }
            "--out" => a.out = Some(PathBuf::from(value()?)),
            "--native" => a.native = Some(PathBuf::from(value()?)),
            "--game-dir" => a.game_dir = Some(PathBuf::from(value()?)),
            other => bail!("ai-arena: unknown option {other}"),
        }
    }
    a.class = class.context("ai-arena needs --class N (the monstats row to spawn)")?;
    if a.ticks == 0 {
        bail!("--ticks must be at least 1");
    }
    if !(1..=2000).contains(&a.distance) {
        bail!("--distance must be 1 to 2000 sub-tiles");
    }
    Ok(a)
}

/// `TICK:PERCENT,...`: when the monster is hurt, in tick order.
fn parse_hurt(text: &str) -> Result<Vec<(u32, u32)>> {
    let mut hurt = Vec::new();
    for part in text.split(',') {
        let (tick, percent) = part
            .split_once(':')
            .with_context(|| format!("--hurt {part}: use TICK:PERCENT"))?;
        let tick: u32 = tick
            .parse()
            .with_context(|| format!("--hurt tick {tick}"))?;
        let percent: u32 = percent
            .parse()
            .with_context(|| format!("--hurt percent {percent}"))?;
        if tick == 0 || !(1..=100).contains(&percent) {
            bail!("--hurt {part}: the tick is 1 or more and the percent 1 to 100");
        }
        hurt.push((tick, percent));
    }
    hurt.sort_by_key(|&(tick, _)| tick);
    Ok(hurt)
}

/// The text `ai-arena` prints for `--help`.
pub const USAGE: &str = "\
d2-client ai-arena --class N [--ai N] [--name TEXT] [--seed N] [--ticks N]
                   [--distance SUBTILES] [--hurt TICK:PERCENT,...] [--level N] [--difficulty D] [--out FILE]
                   [--native DIR] [--game-dir DIR] [--mod ID | --mod-dir DIR]

Warps a new character to level --level (default 2, the Blood Moor; 0 stays in
the first town, where monsters do not fight), spawns monstats row N beside it
and records what its AI does for --ticks ticks (default 500) as an ai-trace-1
file.
With --mod / --mod-dir the mod's ai.toml graphs run (--ai N names the graph
in the trace); without, the monster runs D2Gamma's own AI. Same arguments,
same bytes.";

/// The recorder: reads one tick's snapshot and the monster's graph events
/// and appends them to the [`Trace`].
pub struct Recorder {
    trace: Trace,
    class: u32,
    spawn_frame: i32,
    /// The monsters in the game up to the spawn: the level's own.
    known: BTreeSet<u32>,
    /// The monster's GUID once it is in the game.
    subject: Option<u32>,
}

impl Recorder {
    /// A recorder for a monster of `class`, spawned at the end of the tick
    /// `spawn_frame`.
    pub fn new(trace: Trace, class: u32, spawn_frame: i32) -> Recorder {
        Recorder {
            trace,
            class,
            spawn_frame,
            known: BTreeSet::new(),
            subject: None,
        }
    }

    /// The monster's GUID: the first monster of the class that was not in
    /// the game when it was spawned, kept from then on. `frame` is the
    /// tick `snap` follows.
    pub fn subject(&mut self, frame: i32, snap: &StateSnapshot) -> Option<u32> {
        if self.subject.is_none() {
            let monsters = snap
                .units
                .iter()
                .filter(|u| u.ut == UnitType::Monster as u8);
            if frame <= self.spawn_frame {
                self.known.extend(monsters.map(|u| u.g));
            } else {
                self.subject = monsters
                    .filter(|u| u.cl == Some(self.class) && !self.known.contains(&u.g))
                    .map(|u| u.g)
                    .next();
            }
        }
        self.subject
    }

    /// One tick (the game's frame `frame`): the sample of the monster and the
    /// character, and the graph's events. Ticks before the monster is there
    /// leave no mark. `state` is the monster's graph state, if it has one;
    /// `events` its events (any other monster's are not passed).
    pub fn record(
        &mut self,
        frame: i32,
        snap: &StateSnapshot,
        state: Option<u8>,
        events: &[GraphEvent],
    ) {
        let (Some(guid), Some(player)) = (
            self.subject(frame, snap),
            snap.units.iter().find(|u| u.ut == UnitType::Player as u8),
        ) else {
            return;
        };
        let Some(mon) = snap
            .units
            .iter()
            .find(|u| u.ut == UnitType::Monster as u8 && u.g == guid)
        else {
            return;
        };
        let tick = self.tick(frame);
        self.trace.ticks = tick;
        self.trace.samples.push(Sample {
            tick,
            state,
            mode: mon.m.unwrap_or(0),
            dist: distance(mon, player),
            life: whole(mon.hp),
            max_life: whole(mon.hpx),
            target: whole(player.hp),
        });
        for e in events {
            let kind = match e.kind {
                GraphEventKind::StateChanged { from, to } => EventKind::State { from, to },
                GraphEventKind::WindUp { event, ticks } => EventKind::WindUp { event, ticks },
                GraphEventKind::PhaseChanged { phase } => EventKind::Phase { phase },
            };
            self.trace.events.push(Event {
                tick: self.tick(e.frame),
                kind,
            });
        }
    }

    /// The tick of game frame `frame`: 1 for the first tick after the spawn.
    fn tick(&self, frame: i32) -> u32 {
        u32::try_from(frame - self.spawn_frame).unwrap_or(0)
    }

    /// The trace so far.
    pub fn trace(&self) -> &Trace {
        &self.trace
    }
}

/// Whole points of a raw 8.8 stat.
fn whole(raw: Option<i32>) -> i32 {
    raw.unwrap_or(0) >> 8
}

/// The larger of the two axes between `a` and `b`, in sub-tiles.
fn distance(a: &UnitState, b: &UnitState) -> u32 {
    let d = |p: Option<u32>, q: Option<u32>| p.unwrap_or(0).abs_diff(q.unwrap_or(0));
    d(a.x, b.x).max(d(a.y, b.y))
}

/// The trace's header: the names of the graph `--ai` in `graphs`.
pub fn header(args: &ArenaArgs, seed: u32, graphs: &AiRules) -> Trace {
    let def: Option<&AiDef> = args
        .ai
        .and_then(|i| graphs.defs().iter().find(|d| d.index == i));
    let monster = args
        .name
        .clone()
        .or_else(|| def.map(|d| d.name.clone()).filter(|n| !n.is_empty()))
        .unwrap_or_else(|| format!("class {}", args.class));
    Trace {
        seed,
        ticks: 0,
        class: args.class,
        ai: def.map_or(0, |d| d.index),
        monster,
        states: def
            .map(|d| d.states.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default(),
        phases: def
            .map(|d| d.phases.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default(),
        windups: def.map(|d| d.events.clone()).unwrap_or_default(),
        samples: Vec::new(),
        events: Vec::new(),
    }
}

/// The game data of the options: `--native` or the configured native
/// folder, opened under the mod's layers and data rules; the archives of
/// the install when no native folder is set and no mod is asked for.
pub fn open_data(args: &ArenaArgs, dir: Option<&std::path::Path>) -> Result<GameData> {
    let choice = crate::assets::choose_source(
        args.native.as_deref(),
        None,
        |k| std::env::var(k).ok(),
        crate::assets::default_native_dir(),
    )
    .map_err(anyhow::Error::msg)?;
    match choice {
        crate::assets::SourceChoice::Native(native) => Ok(GameData::open_with(
            &single_player::GameSource::Native(
                native,
                single_player::Layers::List(crate::kit_mod::layers()),
            ),
            crate::kit_mod::ruleset().data_rules(),
        )?),
        crate::assets::SourceChoice::Mpq if crate::kit_mod::active().is_some() => bail!(
            "a mod needs the converted native game folder (--native DIR or the default native folder)"
        ),
        crate::assets::SourceChoice::Mpq => Ok(GameData::select(dir)?),
    }
}

/// Runs the arena on `data` under `rules`, whose graphs are `graphs`.
pub fn run_on(
    args: &ArenaArgs,
    data: GameData,
    rules: Ruleset,
    graphs: &AiRules,
    dir: Option<&std::path::Path>,
) -> Result<Trace> {
    run_with(args, data, rules, graphs, dir, None)
}

/// [`run_on`], with `also` looking at the server after each tick's record. It
/// may take the graph events (the record has seen them by then); when it is
/// not given, the arena takes them.
pub fn run_with(
    args: &ArenaArgs,
    data: GameData,
    rules: Ruleset,
    graphs: &AiRules,
    dir: Option<&std::path::Path>,
    mut also: Option<TickObserver>,
) -> Result<Trace> {
    let mut pokes = Vec::new();
    if args.level != 0 {
        pokes.push(format!("{} warp {}", WARP_TICK + 1, args.level));
    }
    pokes.push(format!(
        "{} spawn {} @x+{} @y normal free",
        spawn_tick(args.level) + 1,
        args.class,
        args.distance
    ));
    let mut argv: Vec<String> = [
        "--seed",
        &args.seed.to_string(),
        "--difficulty",
        &args.difficulty.to_string(),
        "--ticks",
        "1",
        "--out",
        "-",
    ]
    .map(str::to_owned)
    .to_vec();
    for p in pokes {
        argv.push("--poke".into());
        argv.push(p);
    }
    let dump_args = state_dump::parse_args(&argv)?;
    let mut game = DumpGame::resolve(&dump_args, data, dir)?;
    game.rules = rules.clone();
    // The synthetic or the real client may stop on a town object it cannot
    // draw; the monster's AI is the server's.
    game.ignore_fatal = true;
    let recorder = Arc::new(Mutex::new(Recorder::new(
        header(args, game.seed, graphs),
        args.class,
        spawn_tick(args.level),
    )));
    let shared = recorder.clone();
    let mut hurt = args.hurt.clone();
    let observer: TickObserver = Box::new(move |h| {
        let sim = &mut h.game;
        let snap = d2_sim::debug::state::snapshot_world(&sim.game, &sim.events);
        let mut r = shared.lock().unwrap_or_else(|e| e.into_inner());
        let frame = sim.game.frame;
        let guid = r.subject(frame, &snap);
        let unit = guid.and_then(|g| sim.game.lists.find_unit(UnitType::Monster, g));
        let (state, events) = match sim.events.action.hooks().ai.as_mut() {
            Some(ai) => {
                let state = unit
                    .and_then(|u| ai.get(u))
                    .and_then(|m| m.graph)
                    .map(|g| g.state);
                let events: Vec<GraphEvent> = ai
                    .events
                    .peek()
                    .iter()
                    .filter(|e| Some(e.unit) == unit)
                    .copied()
                    .collect();
                (state, events)
            }
            None => (None, Vec::new()),
        };
        r.record(frame, &snap, state, &events);
        let tick = r.tick(frame);
        drop(r);
        // The schedule: life to a percent of the maximum, from the tick on.
        let max = snap
            .units
            .iter()
            .find(|u| u.ut == UnitType::Monster as u8 && Some(u.g) == guid)
            .and_then(|u| u.hpx);
        hurt.retain(|&(at, percent)| {
            let (Some(u), Some(max)) = (unit, max) else {
                return true;
            };
            if tick < at {
                return true;
            }
            let life = i64::from(max) * i64::from(percent) / 100;
            let sys = &mut sim.events.action.sys;
            sys.stats
                .unit_set(&mut sys.hooks, u, STAT_LIFE, life as i32, 0);
            false
        });
        match also.as_mut() {
            Some(f) => f(h),
            None => {
                if let Some(ai) = h.game.events.action.hooks().ai.as_mut() {
                    ai.events.take();
                }
            }
        }
    });
    game.observer = Some(observer);
    let info = RunInfo {
        tool: format!("d2-client ai-arena {}", env!("CARGO_PKG_VERSION")),
        date: "-".into(),
        command: format!("ai-arena --class {}", args.class),
        save: None,
    };
    let total = u32::try_from(spawn_tick(args.level)).unwrap_or(0) + args.ticks;
    let mut log = Vec::new();
    state_dump::dump(game, total, u32::MAX, &info, &mut log)?;
    let trace = recorder
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .trace()
        .clone();
    if trace.samples.len() < args.ticks as usize {
        // The footer of the run says why it stopped (a fatal exit of the
        // client model, the pause, a refused load).
        let footer = String::from_utf8_lossy(&log);
        let footer = footer.lines().last().unwrap_or_default();
        bail!(
            "class {} was in the game for {} of {} ticks (is it a monstats row, and \
             is there room {} sub-tiles east of the character?); the run: {footer}",
            args.class,
            trace.samples.len(),
            args.ticks,
            args.distance
        );
    }
    Ok(trace)
}

/// `d2-client ai-arena...`: the trace, written to `--out` when given.
pub fn run(args: &ArenaArgs) -> Result<Trace> {
    let dir = args
        .game_dir
        .clone()
        .or_else(|| std::env::var_os("D2_GAME_DIR").map(PathBuf::from));
    let data = open_data(args, dir.as_deref())?;
    let graphs = match crate::kit_mod::active() {
        Some(stack) => crate::kit_mod::host(stack)?.ai_rules().clone(),
        None => AiRules::none(),
    };
    let trace = run_on(
        args,
        data,
        crate::kit_mod::ruleset(),
        &graphs,
        dir.as_deref(),
    )?;
    if let Some(out) = &args.out {
        if let Some(parent) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(out, trace.write()).with_context(|| format!("writing {}", out.display()))?;
    }
    Ok(trace)
}

#[cfg(test)]
mod tests;
