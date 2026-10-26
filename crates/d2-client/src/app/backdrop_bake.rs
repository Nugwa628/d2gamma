//! The lobby's backdrop stills: one picture per [`Backdrop`], drawn once
//! from the player's own game files by `bake-backdrops`, a process of its
//! own, and shown by the lobby behind its panels (`super::lobby_picture`).
//! Nothing of a game runs in the client for them.
//!
//! - **Bake** ([`run_cli`]): a headless app starts a local game on the CPU
//!   world view ([`start`]), shown as a scene ([`SceneView`]: no UI, no
//!   players or monsters). For each background still missing, the
//!   [`Director`] on the server thread warps the hidden player to its
//!   level, holds its time of day and moves the camera; once settled, the
//!   world view's picture (the play frame, 1280 × 720) is written as
//!   `<key>.png` ([`Backdrop::key`]), through a temp file, so a picture is
//!   whole or absent. A crash or error ends only the bake's process.
//! - **Cache** ([`stills_dir`]): `Documents\D2Gamma\cache\backdrops\<id>\`,
//!   `<id>` hashing the game build, [`BAKE_VERSION`] and the game files'
//!   stamp (archive names, sizes and times, as the animation bake's
//!   `data_stamp`): a new build or other game files bake again. A bake
//!   removes the other ids' folders.
//! - **Started** by the client ([`spawn`]) when a still is missing, in the
//!   background, once per launch; its lines go to `logs\backdrop-bake.log`.
//! - **Brightness**: not baked in; the lobby lays it on
//!   ([`super::lobby_picture`], [`exponent`]).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use d2_server::seams::Clock;
use d2_sim::poke::PokeOp;
use d2_sim::units::lists::client_state;
use sha2::{Digest, Sha256};

use super::single_player::{self, GameData, Link, LiveData};
use crate::bridge::link::LOCAL_CLIENT;
use crate::ui::front_end::screens::online::backdrop::{Backdrop, Place};
use crate::world_view::present::WorldViewTarget;
use crate::world_view::scene_view::SceneView;

/// The stills' format: a change bakes again.
pub const BAKE_VERSION: u32 = 1;

/// The scene's game seed: the same pictures every time.
const SEED: u32 = 7;

/// The director's steps, in server frames from the local player's entry
/// or from a new background's choice.
const WARP_AT: i32 = 10;
const TIME_AT: i32 = 12;
const MOVE_AT: i32 = 25;
/// Between two camera moves (the rooms on the way load).
const MOVE_STEP: i32 = 40;
/// After the last move, until the picture is taken (the client loads the
/// rooms around it).
const SETTLE: i32 = 45;
/// The hidden player's life (stats 6 and 7, in 1/256): the monsters of a
/// burning Tristram never end it.
const LIFE: i32 = 1 << 28;
/// Frames drawn in place after the director's settle, before the shot.
const DRAWN: u32 = 8;
/// Most updates a background may take until its picture is in place.
const MOST_UPDATES: u32 = 3000;

/// The cache folder's id for game build `version` and game files
/// `stamp`.
pub fn cache_id(version: &str, stamp: &str) -> String {
    let mut h = Sha256::new();
    h.update(format!("backdrops {BAKE_VERSION}\n{version}\n{stamp}\n").as_bytes());
    let hex = format!("{:x}", h.finalize());
    hex[..16].to_string()
}

/// The stills' parent folder under the user's D2Gamma folder `root`.
pub fn cache_root(root: &Path) -> PathBuf {
    root.join("cache").join("backdrops")
}

/// The stills' folder for the archives in `game_dir` (module docs).
pub fn stills_dir(root: &Path, game_dir: &Path) -> Result<PathBuf, String> {
    use crate::world_view::unit_rules::animation_bake::{data_stamp, BakeData};
    let stamp = data_stamp(&BakeData::Archives(game_dir.to_path_buf()))?;
    Ok(cache_root(root).join(cache_id(crate::launch::VERSION, &stamp)))
}

/// Background `b`'s still in `dir`.
pub fn still_path(dir: &Path, b: Backdrop) -> PathBuf {
    dir.join(format!("{}.png", b.key()))
}

/// The backgrounds without a still in `dir`.
pub fn missing(dir: &Path) -> Vec<Backdrop> {
    Backdrop::ALL
        .into_iter()
        .filter(|b| !still_path(dir, *b).is_file())
        .collect()
}

/// The exponent that draws a still at `gamma` (the settings' scale, 155:
/// as drawn).
pub fn exponent(gamma: u16) -> f32 {
    155.0 / f32::from(gamma.max(1))
}

/// The poke lines of a background's set-up, each with its frame from the
/// start ([`WARP_AT`], ...), and the frame its picture is ready.
pub fn script(place: Place) -> (Vec<(i32, String)>, i32) {
    let mut v = keep(place).into_iter().map(|l| (0, l)).collect::<Vec<_>>();
    v.push((WARP_AT, format!("warp {}", place.level)));
    v.push((TIME_AT, format!("time {} {}", place.time.0, place.time.1)));
    let mut at = MOVE_AT;
    for (i, (dx, dy)) in place.moves.iter().enumerate() {
        at = MOVE_AT + MOVE_STEP * i as i32;
        v.push((at, format!("pos @player @x{dx:+} @y{dy:+} free")));
    }
    (v, at + SETTLE)
}

/// The player's life and the time of day, set first.
fn keep(place: Place) -> Vec<String> {
    vec![
        format!("stat @player 7 0 {LIFE}"),
        format!("stat @player 6 0 {LIFE}"),
        format!("time {} {}", place.time.0, place.time.1),
    ]
}

fn op(line: &str) -> Option<PokeOp> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    match d2_sim::poke::parse_op(&toks) {
        Ok(op) => Some(op),
        Err(e) => {
            warn!("backdrop bake: {line:?}: {e}");
            None
        }
    }
}

/// The scene's director on the server thread: it sets up the background
/// asked for (`plan`) and says when its picture is in place (`ready`).
pub struct Director {
    plan: Arc<Mutex<Backdrop>>,
    ready: Arc<AtomicBool>,
    current: Option<Backdrop>,
    start: i32,
    steps: Vec<(i32, String)>,
    next: usize,
    ready_at: i32,
}

impl Director {
    pub fn new(plan: Arc<Mutex<Backdrop>>, ready: Arc<AtomicBool>) -> Self {
        Director {
            plan,
            ready,
            current: None,
            start: 0,
            steps: Vec::new(),
            next: 0,
            ready_at: 0,
        }
    }

    /// The poke lines due at server frame `frame` once the local player is
    /// in the game: a new background starts its set-up over.
    pub fn due(&mut self, frame: i32) -> Vec<String> {
        let want = *self.plan.lock().unwrap_or_else(|e| e.into_inner());
        if self.current != Some(want) {
            self.current = Some(want);
            let (steps, ready_at) = script(want.place());
            self.steps = steps;
            self.next = 0;
            self.start = frame;
            self.ready_at = frame + ready_at;
            self.ready.store(false, Ordering::Release);
        }
        let mut out = Vec::new();
        while let Some((at, line)) = self.steps.get(self.next) {
            if frame < self.start + at {
                break;
            }
            out.push(line.clone());
            self.next += 1;
        }
        if self.next == self.steps.len() && frame >= self.ready_at {
            self.ready.store(true, Ordering::Release);
        }
        out
    }

    /// Runs the due lines on the server's link.
    pub fn run<C: Clock>(&mut self, l: &mut Link<C>) {
        let s = &l.host().game;
        let in_game = s
            .game
            .lists
            .client(d2_sim::units::ClientId(LOCAL_CLIENT))
            .is_some_and(|c| c.state == client_state::IN_GAME);
        if !in_game || single_player::local_player(s).is_none() {
            return;
        }
        let frame = s.game.frame;
        for line in self.due(frame) {
            if let Some(op) = op(&line) {
                let r = super::poke::apply_on_link(l, &op);
                if !matches!(r, d2_sim::poke::PokeResult::Ok(_)) {
                    warn!("backdrop bake: {line:?}: {r:?}");
                }
            }
        }
    }
}

/// The running scene: the background asked for and whether its picture
/// is in place.
pub struct Scene {
    plan: Arc<Mutex<Backdrop>>,
    ready: Arc<AtomicBool>,
}

impl Scene {
    pub fn ask(&self, b: Backdrop) {
        *self.plan.lock().unwrap_or_else(|e| e.into_inner()) = b;
        self.ready.store(false, Ordering::Release);
    }

    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

/// Starts the scene's game at `first` on `world`, as a played game is
/// started ([`super::session::begin_game`]), drawn on the CPU (`gpu`
/// false) or the GPU.
pub fn start(
    world: &mut World,
    live: &Arc<LiveData>,
    gpu: bool,
    first: Backdrop,
) -> anyhow::Result<Scene> {
    use super::play::{install_live_client, LiveClient, ServerClock};
    super::session::begin_game(world);
    // The one player frame: the stills are the game's own view.
    let frame = super::config::set_play_frame()?;
    anyhow::ensure!(
        frame.width * 9 >= frame.height * 15,
        "the play frame {} x {} is not wide",
        frame.width,
        frame.height
    );
    let character = single_player::new_character("sorceress", "Scene")
        .map_err(|e| anyhow::anyhow!("the scene's character: {e:?}"))?;
    let game = GameData::Live(live.clone());
    let speeds = single_player::walk_speeds(&game, &character)?;
    let clock = match world.get_resource::<crate::bridge::mirror::ScriptedClock>() {
        Some(shared) => ServerClock::Shared(super::state_dump::StepClock(shared.0.clone())),
        None => ServerClock::Wall(d2_server::host::SystemClock::default()),
    };
    let (mut link, started) = single_player::start_with(game, SEED, character.clone(), clock)?;
    let plan = Arc::new(Mutex::new(first));
    let ready = Arc::new(AtomicBool::new(false));
    let mut director = Director::new(plan.clone(), ready.clone());
    link.set_before_pump(move |l: &mut Link<ServerClock>| director.run(l))?;
    install_live_client(
        world,
        Box::new(super::save::SharedLink::new(link)),
        LiveClient {
            data: live,
            request: &character,
            start_flags: None,
            prices: started.prices,
            speeds,
            hardcore: false,
            automap_files: None,
            gpu,
        },
    )?;
    super::loading_overlay::remove_loading(world);
    if let Some(d) = world
        .get_resource::<super::sound::GameAudio>()
        .and_then(|a| a.driver.as_ref())
    {
        d.lock().unwrap_or_else(|e| e.into_inner()).set_silent();
    }
    world.insert_resource(SceneView {
        hold: true,
        ..default()
    });
    Ok(Scene { plan, ready })
}

/// `bake-backdrops`' options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakeOptions {
    pub game_dir: PathBuf,
    /// The stills' folder; default: [`stills_dir`] under the user folder.
    pub out: Option<PathBuf>,
    /// Bake these even when present (`--only KEY`, repeatable); default:
    /// the missing ones.
    pub only: Vec<Backdrop>,
}

pub fn parse_args(args: &[String]) -> Result<BakeOptions, String> {
    let mut game_dir = None;
    let mut out = None;
    let mut only = Vec::new();
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("missing value for {flag}"));
        match flag.as_str() {
            "--game-dir" => game_dir = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--only" => {
                let key = value()?;
                only.push(Backdrop::from_key(key).ok_or(format!("no background {key:?}"))?);
            }
            // The game's global flags (`main`) are allowed and ignored.
            "--mute" | "--sound" => {}
            _ => return Err(format!("unknown bake-backdrops option {flag}")),
        }
    }
    let game_dir = game_dir
        .or_else(|| std::env::var_os("D2_GAME_DIR").map(PathBuf::from))
        .ok_or("--game-dir is required")?;
    Ok(BakeOptions {
        game_dir,
        out,
        only,
    })
}

/// `bake-backdrops`: prints `bake-backdrops: <event>` lines (`todo N`,
/// `still KEY MS BYTES`, `done N`, `error MESSAGE`).
pub fn run_cli(args: &[String]) -> Result<(), String> {
    let r = parse_args(args).and_then(|o| bake(&o).map(|_| ()));
    if let Err(e) = &r {
        line(&format!("error {}", e.replace('\n', " ")));
    }
    r
}

fn line(event: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "bake-backdrops: {event}");
    let _ = out.flush();
}

/// What a bake wrote: each still with its time and size.
#[derive(Debug, Clone, Default)]
pub struct Baked {
    pub dir: PathBuf,
    pub stills: Vec<(Backdrop, Duration, u64)>,
}

/// Bakes the stills `o` asks for (module docs).
pub fn bake(o: &BakeOptions) -> Result<Baked, String> {
    let dir = match &o.out {
        Some(d) => d.clone(),
        None => stills_dir(&crate::user_dir::root(), &o.game_dir)?,
    };
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if o.out.is_none() {
        remove_others(&dir);
    }
    let todo = if o.only.is_empty() {
        missing(&dir)
    } else {
        o.only.clone()
    };
    line(&format!("todo {}", todo.len()));
    let mut baked = Baked {
        dir: dir.clone(),
        stills: Vec::new(),
    };
    let Some(&first) = todo.first() else {
        line("done 0");
        return Ok(baked);
    };
    let data = GameData::select(Some(&o.game_dir)).map_err(|e| format!("{e:#}"))?;
    let GameData::Live(live) = &data;
    let mut rig = Rig::new(&dir);
    let scene = start(rig.app.world_mut(), live, false, first).map_err(|e| format!("{e:#}"))?;
    for b in todo {
        let t = Instant::now();
        scene.ask(b);
        let (rgba, size) = rig.picture(&scene, b)?;
        let bytes = write_png(&still_path(&dir, b), &rgba, size)?;
        let took = t.elapsed();
        line(&format!("still {} {} {bytes}", b.key(), took.as_millis()));
        baked.stills.push((b, took, bytes));
    }
    line(&format!("done {}", baked.stills.len()));
    Ok(baked)
}

/// Removes the stills' folders of other ids beside `dir`.
fn remove_others(dir: &Path) {
    let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) else {
        return;
    };
    for e in std::fs::read_dir(parent).into_iter().flatten().flatten() {
        if e.file_name() != name && e.path().is_dir() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// The headless app the scene runs in, on a scripted clock (40 ms per
/// update).
struct Rig {
    app: App,
    ms: Arc<AtomicU32>,
}

impl Rig {
    fn new(dir: &Path) -> Self {
        use bevy::state::app::StatesPlugin;
        use bevy::time::TimeUpdateStrategy;
        let ms = Arc::new(AtomicU32::new(1000));
        let mut app = App::new();
        app.insert_resource(crate::bridge::mirror::ScriptedClock(ms.clone()));
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            StatesPlugin,
            bevy::input::InputPlugin,
        ))
        .init_asset::<Image>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            40,
        )));
        super::play::register_live_client(&mut app, false);
        super::session::add_session(&mut app);
        // The default settings, kept out of the user's own.
        app.insert_resource(super::config::ConfigRes {
            dir: dir.join(".settings"),
            settings: super::config::Settings::default(),
            bindings: crate::controls::Bindings::empty(),
            sound_device: false,
        });
        let mut rig = Rig { app, ms };
        rig.ticks(5);
        rig
    }

    fn ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.app.update();
            self.ms.fetch_add(40, Ordering::SeqCst);
        }
    }

    /// `b`'s picture once in place: RGBA rows and size.
    fn picture(&mut self, scene: &Scene, b: Backdrop) -> Result<(Vec<u8>, (u32, u32)), String> {
        let mut n = 0;
        loop {
            let ready = scene.ready();
            if let Some(mut v) = self.app.world_mut().get_resource_mut::<SceneView>() {
                if v.hold == ready || v.paused {
                    v.hold = !ready;
                    v.paused = false;
                }
            }
            if ready {
                break;
            }
            self.ticks(1);
            n += 1;
            if n >= MOST_UPDATES {
                return Err(format!("{} not in place after {n} updates", b.key()));
            }
        }
        self.ticks(DRAWN);
        let w = self.app.world();
        let target = w
            .get_resource::<WorldViewTarget>()
            .ok_or("no world view picture")?;
        let image = w
            .resource::<Assets<Image>>()
            .get(&target.image)
            .ok_or("no world view image")?;
        let size = (image.width(), image.height());
        let data = image.data.clone().ok_or("the picture has no pixels")?;
        if data.len() != (size.0 * size.1 * 4) as usize {
            return Err(format!("the picture is {} bytes for {size:?}", data.len()));
        }
        Ok((data, size))
    }
}

/// Writes `rgba` (`size`) as an RGB PNG at `path` through a temp file;
/// returns its size in bytes.
pub fn write_png(path: &Path, rgba: &[u8], size: (u32, u32)) -> Result<u64, String> {
    let tmp = path.with_extension("png.tmp");
    let rgb: Vec<u8> = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let write = || -> Result<(), png::EncodingError> {
        let file = std::fs::File::create(&tmp)?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size.0, size.1);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&rgb)?;
        Ok(())
    };
    write().map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(std::fs::metadata(path).map_or(0, |m| m.len()))
}

/// Reads a still written by [`write_png`]: RGBA rows and size.
pub fn read_png(path: &Path) -> Result<(Vec<u8>, (u32, u32)), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("too large")?];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let (w, h) = (info.width, info.height);
    let px = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => px
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return Err(format!("{}: indexed", path.display())),
    };
    Ok((rgba, (w, h)))
}

/// The client's bake process: started once per launch while a still is
/// missing ([`spawn`]).
#[derive(Resource, Default)]
pub struct StillsBake {
    /// The stills' folder; `None`: no stills (no game folder to key them).
    pub dir: Option<PathBuf>,
    pub game_dir: Option<PathBuf>,
    child: Option<std::process::Child>,
    started: bool,
    /// Bumped when a bake ends: the stills present may have changed.
    pub generation: u32,
    /// The last [`Self::still`] answer, for its background and generation.
    seen: Option<(Backdrop, u32, Option<PathBuf>)>,
    /// Stills that could not be read on this launch.
    unreadable: Vec<Backdrop>,
}

impl StillsBake {
    /// The stills for the archives in `game_dir`, under the user folder.
    /// No folder (logged) when the files can't be stamped.
    pub fn for_game(game_dir: Option<PathBuf>) -> Self {
        let dir = game_dir.as_deref().and_then(|g| {
            stills_dir(&crate::user_dir::root(), g)
                .map_err(|e| warn!("backdrop stills: {e}"))
                .ok()
        });
        Self::at(dir, game_dir)
    }

    /// The stills in `dir`, baked from `game_dir` when missing (`None`:
    /// never baked by this client).
    pub fn at(dir: Option<PathBuf>, game_dir: Option<PathBuf>) -> Self {
        StillsBake {
            dir,
            game_dir,
            ..default()
        }
    }

    /// Background `b`'s still, if present and readable (looked up again
    /// only when `b` or the generation changes).
    pub fn still(&mut self, b: Backdrop) -> Option<PathBuf> {
        if let Some((sb, generation, path)) = &self.seen {
            if *sb == b && *generation == self.generation {
                return path.clone();
            }
        }
        let path = self
            .dir
            .as_ref()
            .map(|d| still_path(d, b))
            .filter(|p| p.is_file() && !self.unreadable.contains(&b));
        self.seen = Some((b, self.generation, path.clone()));
        path
    }

    /// `b`'s still could not be read: the lobby keeps its fallback for it
    /// on this launch.
    pub fn unreadable(&mut self, b: Backdrop) {
        self.unreadable.push(b);
        self.seen = None;
    }

    /// Starts the bake if a still is missing and it did not run on this
    /// launch; notes its end. A failure is logged; the lobby keeps its
    /// fallback.
    pub fn poll(&mut self) {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        warn!("backdrop stills: the bake ended with {status}");
                    }
                    self.child = None;
                    self.generation += 1;
                }
                Ok(None) => {}
                Err(e) => {
                    warn!("backdrop stills: {e}");
                    self.child = None;
                    self.generation += 1;
                }
            }
            return;
        }
        if self.started {
            return;
        }
        self.started = true;
        let (Some(dir), Some(game_dir)) = (&self.dir, &self.game_dir) else {
            return;
        };
        if missing(dir).is_empty() {
            return;
        }
        match spawn(game_dir) {
            Ok(c) => self.child = Some(c),
            Err(e) => warn!("backdrop stills: the bake did not start: {e}"),
        }
    }
}

/// Starts `bake-backdrops` for `game_dir` in the background, its output to
/// `logs\backdrop-bake.log`.
pub fn spawn(game_dir: &Path) -> std::io::Result<std::process::Child> {
    let exe = std::env::current_exe()?;
    let logs = crate::user_dir::logs();
    std::fs::create_dir_all(&logs)?;
    let log = std::fs::File::create(logs.join("backdrop-bake.log"))?;
    let mut c = std::process::Command::new(exe);
    c.arg("--mute")
        .arg("bake-backdrops")
        .arg("--game-dir")
        .arg(game_dir)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window and below the game's own priority.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
        c.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    c.spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_background_is_set_up_in_order_and_then_taken() {
        for b in Backdrop::ALL {
            let place = b.place();
            let (steps, ready) = script(place);
            let frames: Vec<i32> = steps.iter().map(|s| s.0).collect();
            assert!(frames.windows(2).all(|w| w[0] <= w[1]), "{b:?}: in order");
            assert!(
                steps.iter().all(|(_, l)| op(l).is_some()),
                "{b:?}: every line parses"
            );
            let moves = steps.iter().filter(|(_, l)| l.starts_with("pos ")).count();
            assert_eq!(moves, place.moves.len());
            assert!(steps.contains(&(WARP_AT, format!("warp {}", place.level))));
            assert!(
                ready > *frames.last().unwrap(),
                "{b:?}: after the last step"
            );
        }
    }

    #[test]
    fn the_director_follows_the_background_asked_for() {
        let plan = Arc::new(Mutex::new(Backdrop::RogueEncampment));
        let ready = Arc::new(AtomicBool::new(false));
        let mut d = Director::new(plan.clone(), ready.clone());
        let (steps, ready_at) = script(Backdrop::RogueEncampment.place());
        let mut lines = Vec::new();
        for f in 100..100 + ready_at {
            lines.extend(d.due(f));
            assert!(!ready.load(Ordering::Acquire), "frame {f}: not yet");
        }
        assert_eq!(lines, steps.iter().map(|s| s.1.clone()).collect::<Vec<_>>());
        assert!(d.due(100 + ready_at).is_empty());
        assert!(ready.load(Ordering::Acquire));
        *plan.lock().unwrap() = Backdrop::ArreatSummit;
        let mut lines = d.due(400);
        assert!(!ready.load(Ordering::Acquire));
        let (_, ready_at) = script(Backdrop::ArreatSummit.place());
        for f in 401..=400 + ready_at {
            lines.extend(d.due(f));
        }
        assert!(lines.contains(&"warp 120".to_string()));
        assert!(ready.load(Ordering::Acquire));
    }

    #[test]
    fn the_cache_id_follows_the_build_and_the_files() {
        let a = cache_id("0.1.1.1703", "stamp-a");
        assert_eq!(a.len(), 16);
        assert!(a.bytes().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(a, cache_id("0.1.1.1703", "stamp-a"), "stable");
        assert_ne!(a, cache_id("0.1.1.1704", "stamp-a"), "a new build");
        assert_ne!(a, cache_id("0.1.1.1703", "stamp-b"), "other files");
        let root = Path::new("/u/D2Gamma");
        assert_eq!(cache_root(root), root.join("cache").join("backdrops"));
        assert_eq!(
            still_path(Path::new("/c"), Backdrop::LutGholein),
            Path::new("/c").join("lut-gholein.png")
        );
    }

    #[test]
    fn the_stills_folder_changes_with_the_game_files() {
        let tmp = std::env::temp_dir().join(format!("backdrop-stamp-{}", std::process::id()));
        let game = tmp.join("game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("d2data.mpq"), b"one").unwrap();
        let a = stills_dir(&tmp, &game).unwrap();
        assert!(a.starts_with(cache_root(&tmp)));
        assert_eq!(a, stills_dir(&tmp, &game).unwrap());
        std::fs::write(game.join("d2data.mpq"), b"other size").unwrap();
        assert_ne!(a, stills_dir(&tmp, &game).unwrap());
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn missing_stills_are_listed_and_a_still_round_trips() {
        let dir = std::env::temp_dir().join(format!("backdrop-stills-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(missing(&dir), Backdrop::ALL.to_vec());
        let rgba: Vec<u8> = (0..4 * 3).flat_map(|i| [i as u8, 7, 9, 255]).collect();
        let bytes = write_png(&still_path(&dir, Backdrop::Tristram), &rgba, (4, 3)).unwrap();
        assert!(bytes > 0);
        assert!(!missing(&dir).contains(&Backdrop::Tristram));
        assert_eq!(missing(&dir).len(), Backdrop::ALL.len() - 1);
        assert_eq!(
            read_png(&still_path(&dir, Backdrop::Tristram)).unwrap(),
            (rgba, (4, 3))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_options_take_the_game_folder_and_backgrounds() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let o = parse_args(&args(&["--game-dir", "/g", "--only", "tristram"])).unwrap();
        assert_eq!(o.game_dir, PathBuf::from("/g"));
        assert_eq!(o.only, vec![Backdrop::Tristram]);
        assert_eq!(o.out, None);
        assert!(parse_args(&args(&["--game-dir", "/g", "--only", "x"])).is_err());
        assert!(parse_args(&args(&["--game-dir", "/g", "--frames", "3"])).is_err());
    }

    #[test]
    fn a_brighter_background_has_a_smaller_exponent() {
        assert_eq!(exponent(155), 1.0);
        assert!(exponent(215) < exponent(175));
        assert!(exponent(0).is_finite());
    }
}
