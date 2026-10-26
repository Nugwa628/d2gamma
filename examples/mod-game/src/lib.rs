//! An example mod game: the shape every mod game built on D2Gamma has
//! (`docs/mods/mod-game.md`, `docs/plans/mods/seams.md` MS16).
//!
//! A mod game is its own binary. It depends on the D2Gamma crates, installs
//! its overrides as a `ModRules` in the `Ruleset` of every game it builds,
//! chooses the layers its data is opened with and keeps its characters in
//! its own save folder. The D2Gamma exe never loads it: players start the
//! mod game directly.
//!
//! This example is synthetic: it carries no mod data and no real mod's
//! names. Its one rule is a skill body in a "do" slot past D2Gamma's own
//! table, so D2Gamma's own skills are unchanged. Its layers are the folders
//! under `mods/` of the native folder it is pointed at, or archives laid
//! over an install (`--archive`).
//!
//! With `--window` it plays in a window as D2Gamma's exe does, branded as
//! its own game ([`identity`]): its name and title, its own folder under
//! Documents and its own credits. The window opens at the front end (main
//! menu, character select and create, [`front_end_config`]); `--name`
//! skips it and starts that character.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use d2_client::app::front_session::FrontEndConfig;
use d2_client::app::play::PlayConfig;
use d2_client::app::server_thread::ThreadLink;
use d2_client::app::single_player::{
    self, Character, D2sCodec, GameBuilder, GameData, GameSource, Layers, Link, Saves, Started,
};
use d2_client::bridge::link::{SendQueue, ServerLink};
use d2_client::identity::GameIdentity;
use d2_server::seams::Clock;
use d2_sim::rules::{ModRules, Ruleset, SkillFn, SkillHost, SkillProvider};
use d2_sim::units::UnitId;

/// The example's name, in logs and the dev console.
pub const NAME: &str = "example-mod-game";

/// The name players see, in the window title and the version lines.
pub const TITLE: &str = "Example Mod Game";

/// The example's folder under Documents: its saves, settings and logs.
pub const FOLDER: &str = "ExampleModGame";

/// The example's credits text (`Credits.txt` layout: one row per line).
pub const CREDITS: &str = "Example Mod Game\r\n\r\nA synthetic mod game on the D2Gamma engine\r\n";

/// The example's identity: its name and version, its window title, its
/// Documents folder and its credits. Its binary installs it first thing
/// (`d2_client::identity::install`).
pub fn identity() -> GameIdentity {
    GameIdentity::new(TITLE, env!("CARGO_PKG_VERSION"), FOLDER)
        .with_credits(Arc::new(|_expansion| Some(CREDITS.as_bytes().to_vec())))
}

/// The "do" slot of the example's skill body: past D2RUST's own table, so
/// it adds a body instead of replacing one.
pub const EXAMPLE_DO_SLOT: u16 = 240;

/// The example's skill bodies: one added "do" body that reports that it
/// ran (returns 1) and changes nothing else.
#[derive(Debug, Default)]
pub struct ExampleSkills;

impl SkillProvider for ExampleSkills {
    fn has(&self, kind: SkillFn, index: u16) -> bool {
        kind == SkillFn::Do && index == EXAMPLE_DO_SLOT
    }

    fn srvdo(
        &self,
        _w: &mut dyn SkillHost,
        index: u16,
        _u: UnitId,
        _skill: i32,
        _lvl: i32,
        _charge: bool,
        _item: bool,
        _aim: bool,
    ) -> Option<i32> {
        (index == EXAMPLE_DO_SLOT).then_some(1)
    }
}

/// The example's overrides: its name and its skill bodies.
#[derive(Debug, Default)]
pub struct ExampleRules {
    skills: ExampleSkills,
}

impl ModRules for ExampleRules {
    fn name(&self) -> &str {
        NAME
    }

    fn skills(&self) -> Option<&dyn SkillProvider> {
        Some(&self.skills)
    }
}

/// Where the example reads the player's files and keeps its saves, and
/// how it runs.
#[derive(Debug, Clone)]
pub struct Options {
    /// The player's files: an install folder (with the example's archives
    /// on top) or a converted native folder with the example's layers.
    pub source: GameSource,
    /// The example's own save folder (never D2RUST's); `None`: `saves` in
    /// its Documents folder ([`Options::saves_dir`]).
    pub saves: Option<PathBuf>,
    /// `--window`: play in a window instead of the headless frames.
    pub window: bool,
    /// `--frames N`: the window closes after N frames.
    pub frames: Option<u32>,
    /// `--name NAME`: the window starts this character (its save, else a
    /// new one) instead of the front end.
    pub name: Option<String>,
}

impl Options {
    /// Reads `--game-dir DIR [--archive FILE]...`, or `--native DIR
    /// [--layer NAME]...`, then `--saves DIR`, `--window`, `--frames N` and
    /// `--name NAME`.
    pub fn parse(args: impl IntoIterator<Item = String>) -> anyhow::Result<Options> {
        let mut game_dir = None;
        let mut native = None;
        let mut layers = Vec::new();
        let mut archives = Vec::new();
        let mut saves = None;
        let mut window = false;
        let mut frames = None;
        let mut name = None;
        let mut args = args.into_iter();
        while let Some(a) = args.next() {
            let mut value = || {
                args.next()
                    .ok_or_else(|| anyhow::anyhow!("{a} needs a value"))
            };
            match a.as_str() {
                "--game-dir" => game_dir = Some(PathBuf::from(value()?)),
                "--native" => native = Some(PathBuf::from(value()?)),
                "--layer" => layers.push(value()?),
                "--archive" => archives.push(PathBuf::from(value()?)),
                "--saves" => saves = Some(PathBuf::from(value()?)),
                "--window" => window = true,
                "--frames" => frames = Some(value()?.parse()?),
                "--name" => name = Some(value()?),
                other => anyhow::bail!("unknown argument {other}"),
            }
        }
        let source = match (native, game_dir) {
            (Some(_), _) if !archives.is_empty() => {
                anyhow::bail!("--archive needs --game-dir: a native folder has layers")
            }
            (Some(dir), _) => GameSource::Native(dir, Layers::List(layers)),
            (None, Some(_)) if !layers.is_empty() => {
                anyhow::bail!("--layer needs --native: an install has no layers")
            }
            (None, Some(dir)) if archives.is_empty() => GameSource::Archives(dir),
            (None, Some(dir)) => GameSource::LayeredArchives(dir, archives),
            (None, None) => anyhow::bail!("pass --game-dir DIR or --native DIR"),
        };
        Ok(Options {
            source,
            saves,
            window,
            frames,
            name,
        })
    }

    /// The save folder: `--saves`, else `saves` in the game's Documents
    /// folder (the example's own once its identity is installed).
    pub fn saves_dir(&self) -> PathBuf {
        self.saves
            .clone()
            .unwrap_or_else(d2_client::user_dir::saves)
    }

    /// The install folder the data is read from (`--game-dir`); `None`
    /// for a native folder.
    pub fn game_dir(&self) -> Option<PathBuf> {
        match &self.source {
            GameSource::Archives(dir) | GameSource::LayeredArchives(dir, _) => Some(dir.clone()),
            GameSource::Native(..) => None,
        }
    }
}

/// The example's rules.
pub fn rules() -> Ruleset {
    Ruleset::Mod(Arc::new(ExampleRules::default()))
}

/// The example's saves in `folder`: D2RUST's `.d2s` format.
pub fn saves(folder: PathBuf) -> Saves {
    Saves::new(folder, Arc::new(D2sCodec))
}

/// The example's game builder on `data`: its rules and its saves.
pub fn builder(data: GameData, saves_dir: PathBuf) -> GameBuilder {
    GameBuilder::new(data)
        .rules(rules())
        .saves(saves(saves_dir))
}

/// The example's front end and the games it starts
/// (`d2_client::app::play::run_front_end`): its rules and saves in
/// `saves_dir` for every game, the data read from `game_dir` when it is an
/// install, no client hooks.
pub fn front_end_config(
    data: GameData,
    game_dir: Option<PathBuf>,
    saves_dir: PathBuf,
) -> FrontEndConfig {
    FrontEndConfig::new(
        data,
        game_dir,
        rules(),
        saves(saves_dir),
        d2_client::mods::ClientMods::default(),
    )
}

/// The example's windowed game (`d2_client::app::play::run`): its rules,
/// its saves, `character` written to `save_path` in its folder, every
/// check and dump off, as a player starts it; the window closes after
/// `frames` when given.
pub fn window_config(
    data: GameData,
    saves: Saves,
    character: Character,
    save_path: Option<PathBuf>,
    frames: Option<u32>,
) -> PlayConfig {
    PlayConfig {
        data,
        seed: single_player::clock_map_seed(),
        rules: rules(),
        saves,
        mods: d2_client::mods::ClientMods::default(),
        character,
        exit_after: frames,
        save_path,
        hardcore: false,
        start_flags: None,
        dump: None,
        input: None,
        sound_log: None,
        frame_schedule: None,
        no_sound: false,
        device_ends: None,
        async_collects: None,
        registry: None,
        audio_dump: None,
        loading_dump: None,
        pokes: Vec::new(),
        sends: Vec::new(),
        dev: true,
        difficulty: 0,
        dev_start: None,
        offscreen: None,
        control: None,
    }
}

/// The server's milliseconds per frame step of [`StepClock`].
pub const STEP_MS: u32 = 40;

/// A host clock that moves only when stepped, so a headless run is the
/// same on every machine.
#[derive(Debug, Clone)]
pub struct StepClock(Arc<AtomicU32>);

impl Default for StepClock {
    fn default() -> StepClock {
        StepClock(Arc::new(AtomicU32::new(1000)))
    }
}

impl StepClock {
    /// Moves the clock on by `ms`.
    pub fn step(&self, ms: u32) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for StepClock {
    fn now_ms(&mut self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Starts the example's game on its own server thread behind `clock` and
/// joins its character: the client's create request (C→S 0x67), then its
/// join (C→S 0x6B), one stepped server frame each.
pub fn start_and_join(
    builder: GameBuilder,
    clock: &StepClock,
) -> anyhow::Result<(ThreadLink<Link<StepClock>>, Started)> {
    let request = single_player::create_request_for(&builder.character);
    let (mut link, started) = builder.start(clock.clone())?;
    link.send(SendQueue::System, &request.encode())?;
    link.pump()?;
    clock.step(STEP_MS);
    link.pump()?;
    link.receive();
    link.send(SendQueue::System, &[0x6B])?;
    clock.step(STEP_MS);
    link.pump()?;
    link.receive();
    Ok((link, started))
}

/// Runs `frames` stepped server frames; the number of game ticks run.
pub fn run_frames(
    link: &mut ThreadLink<Link<StepClock>>,
    clock: &StepClock,
    frames: u32,
) -> anyhow::Result<u32> {
    let mut ticks = 0;
    for _ in 0..frames {
        clock.step(STEP_MS);
        if link.pump()?.ticked {
            ticks += 1;
        }
        link.receive();
    }
    Ok(ticks)
}

/// The name of the rules the game behind `link` runs under (`None` for
/// D2RUST's own).
pub fn rules_name(link: &mut ThreadLink<Link<StepClock>>) -> anyhow::Result<Option<String>> {
    Ok(link.with(|l| {
        let hooks = l.host_mut().game.events.action.hooks();
        hooks.rules.mod_rules().map(|r| r.name().to_owned())
    })?)
}
