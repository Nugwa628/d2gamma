//! The front end and the games it starts, as one window session: the
//! main menu, character select and create and the other front-end
//! screens first, then the game the player chose; Save and Exit returns to
//! the main menu, the window closing ends the session.
//!
//! [`run_front_end`] is the entry point every game binary calls: D2Gamma's
//! exe (`play` without a shortcut) and a mod game's binary alike. A mod
//! game hands it its rules, its save folder and format and its client
//! hooks ([`FrontEndConfig::new`]); its name, title and Documents folder
//! are the process's [`crate::identity`], its own screens and front canvas
//! come in through its [`ClientMods`] (`mods::ui_hooks`, E-UI2), so it
//! shows its own front end without forking the client
//! (`docs/plans/mods/seams.md` F1).
//!
//! One process, one window and one event loop for the whole session: the
//! front end and each game are states of one app (`app::session`); the
//! game data is read once. [`install_front_end`] builds the same session
//! on an app the caller made, with or without the window's rendering
//! ([`FrontOutput`]), so tests drive it headless.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use bevy::prelude::*;
use d2_sim::rules::Ruleset;

use super::front_host::{add_front_session, FrontArt, FrontStarter};
use super::front_start::{front_host_with, Entry, StartChoice, StartHandles};
use super::play::{self, PlayConfig, PlayHandles};
use super::play_start::{self, CliStart, GameStart};
use super::session::{self, GameEnder, GameStarter};
use super::single_player::{self, CharacterCodec, GameData, LiveData, Saves};
use crate::mods::ClientMods;
use crate::ui::front_end::character_files::{CharacterFiles, NoTables};
use crate::ui::front_end::Outcome;

/// Dev tools: a `load` in the session: the next game is the slot's
/// character (already the save file), seed and difficulty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DevLoad {
    /// The slot the game starts again from.
    pub slot: PathBuf,
    /// The character's save file (the slot's copy is already in it).
    pub save: PathBuf,
    pub seed: u32,
    pub difficulty: u8,
}

impl DevLoad {
    /// The slot at `slot` for the save `save`.
    pub fn read(slot: &Path, save: PathBuf) -> anyhow::Result<Self> {
        let s = super::dev::slots::read_slot(slot).map_err(anyhow::Error::msg)?;
        println!("play: dev load: starting again from {}", slot.display());
        crate::launch::note(format!("dev load: {}", slot.display()));
        Ok(Self {
            slot: slot.to_path_buf(),
            save,
            seed: s.seed,
            difficulty: s.difficulty,
        })
    }
}

/// The next game of the session: what the front end chose, or a dev
/// tools' `load`. [`FrontEndConfig`]'s game maker turns it into the
/// game's [`PlayConfig`].
#[derive(Debug)]
pub struct FrontGame {
    /// The session's game data (read once).
    pub data: GameData,
    /// The install folder the data came from, if any (save folders inside
    /// it are refused, `save::save_path`).
    pub game_dir: Option<PathBuf>,
    /// The character the front end chose; `None` with a dev `load`.
    pub choice: Option<StartChoice>,
    /// The difficulty popup's pick.
    pub menu_difficulty: Option<u8>,
    /// A dev tools' `load`: the slot's character, seed and difficulty.
    pub load: Option<DevLoad>,
}

impl FrontGame {
    /// The game start: the front end's choice (or the dev load's save),
    /// read with the game's character format and written to its file in
    /// the game's save folder. Run before the game installs, so a bad
    /// folder stops there.
    pub fn resolve(&self, saves: &Saves) -> anyhow::Result<GameStart> {
        let difficulty = self.load.as_ref().map_or(0, |l| l.difficulty);
        play_start::resolve_with(
            &CliStart {
                save: self.load.as_ref().map(|l| l.save.clone()),
                new: None,
                save_dir: Some(saves.folder.clone()),
                difficulty,
                hardcore: false,
            },
            &self.data,
            self.game_dir.as_deref(),
            self.menu_difficulty,
            self.choice.as_ref(),
            saves.codec.as_ref(),
        )
    }

    /// The game's seed: the dev load's, else one drawn from the clock (the
    /// front end's games), the character's own when it carries one.
    pub fn seed(&self, character: &single_player::Character) -> u32 {
        single_player::game_seed_or(
            character,
            self.load.as_ref().map(|l| l.seed),
            single_player::clock_map_seed(),
        )
    }

    /// The game as a player starts it from the front end: `rules`, `saves`
    /// and `mods`, the chosen character in its file of the save folder,
    /// with every check, dump and test input off and the dev tools as
    /// `dev` says.
    pub fn play_config(
        self,
        rules: Ruleset,
        saves: Saves,
        mods: ClientMods,
        dev: bool,
    ) -> anyhow::Result<PlayConfig> {
        let start = self.resolve(&saves)?;
        let seed = self.seed(&start.character);
        crate::launch::note(format!(
            "game start: {}, difficulty {}, seed {seed}, save {:?}",
            start.origin, start.difficulty, start.save_path
        ));
        Ok(PlayConfig {
            data: self.data,
            seed,
            rules,
            saves,
            mods,
            character: start.character,
            exit_after: None,
            save_path: start.save_path,
            start_flags: start.start_flags,
            hardcore: start.hardcore,
            dump: None,
            input: None,
            pokes: Vec::new(),
            sends: Vec::new(),
            sound_log: None,
            no_sound: false,
            registry: None,
            frame_schedule: None,
            audio_dump: None,
            device_ends: None,
            async_collects: None,
            loading_dump: None,
            dev,
            difficulty: start.difficulty,
            dev_start: self.load.map(|l| l.slot),
            offscreen: None,
            control: None,
        })
    }
}

/// A game's character files read and written with its character codec
/// (a mod game's own save format): the select list shows each file's
/// header as the codec reads it, the create screen's stub is the codec's
/// write of the new character, Convert reads, sets the expansion bit and
/// writes it again with the game's tables. A file the codec refuses is
/// not listed.
#[derive(Debug)]
pub struct CodecFiles {
    codec: Arc<dyn CharacterCodec>,
    live: Arc<LiveData>,
}

impl CodecFiles {
    /// `codec`'s files, read with the tables of `live`.
    pub fn new(codec: Arc<dyn CharacterCodec>, live: Arc<LiveData>) -> CodecFiles {
        CodecFiles { codec, live }
    }
}

impl CharacterFiles for CodecFiles {
    fn extension(&self) -> &str {
        self.codec.extension()
    }

    fn header(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        let save = self.codec.read(&self.live, bytes, 0).ok()?;
        let header = d2_formats::d2s::D2s {
            header: save.header,
            body: None,
        };
        d2_formats::d2s::write(&header, &NoTables).ok()
    }

    fn stub(&self, stub: &d2_formats::d2s::D2s) -> Result<Vec<u8>, String> {
        self.codec.write(stub, &NoTables)
    }

    fn to_expansion(&self, bytes: &[u8]) -> Option<Vec<u8>> {
        let mut save = self.codec.read(&self.live, bytes, 0).ok()?;
        save.header.status |= d2_formats::d2s::status::EXPANSION;
        self.codec.write(&save, &self.live.save).ok()
    }
}

/// Makes each game's config from the front end's choice.
pub type MakeGame = Box<dyn FnMut(FrontGame) -> anyhow::Result<PlayConfig>>;

/// What the session runs: the game data, the save folder the screens list
/// and create characters in, the client hooks (a mod game's own screens
/// among them) and how each game is made.
pub struct FrontEndConfig {
    /// The game's tables, level data and art, loaded with its layers.
    pub data: GameData,
    /// The install folder the data came from, if any.
    pub game_dir: Option<PathBuf>,
    /// The folder character select lists and character create writes to;
    /// the settings and controls sit next to it (`config::config_dir`).
    pub save_dir: PathBuf,
    /// The game's character files in `save_dir` (`None`: D2Gamma's
    /// `.d2s`; [`FrontEndConfig::new`]: the game's codec, [`CodecFiles`]).
    pub files: Option<Arc<dyn CharacterFiles>>,
    /// The game's client hooks: its front-end screens and canvas, its
    /// panels and HUD (empty for D2Gamma).
    pub mods: ClientMods,
    /// The session opens at the online login (`play --online`).
    pub online: bool,
    /// The server the online screens pick first (`play --server`).
    pub server: Option<String>,
    /// The dev tools are on (the log ring the bug report reads).
    pub dev: bool,
    /// The game may open a sound device (`--no-sound` takes it away).
    pub sound_device: bool,
    /// Makes each game's config ([`FrontGame::play_config`] by default).
    pub make_game: MakeGame,
}

impl FrontEndConfig {
    /// A game's session: `data` from `game_dir`, characters in `saves`'
    /// folder, every game under `rules` with `mods` as its client hooks,
    /// the dev tools on, the sound device allowed.
    pub fn new(
        data: GameData,
        game_dir: Option<PathBuf>,
        rules: Ruleset,
        saves: Saves,
        mods: ClientMods,
    ) -> FrontEndConfig {
        let save_dir = saves.folder.clone();
        let game_mods = mods.clone();
        let GameData::Live(live) = &data;
        let files: Arc<dyn CharacterFiles> =
            Arc::new(CodecFiles::new(saves.codec.clone(), live.clone()));
        FrontEndConfig {
            data,
            game_dir,
            save_dir,
            files: Some(files),
            mods,
            online: false,
            server: None,
            dev: true,
            sound_device: true,
            make_game: Box::new(move |g: FrontGame| {
                g.play_config(rules.clone(), saves.clone(), game_mods.clone(), true)
            }),
        }
    }
}

/// How [`install_front_end`] builds the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontOutput {
    /// A window: the GPU client, sound output and the player's window
    /// choice (what [`run_front_end`] runs).
    Window,
    /// No window and no GPU (tests): the front end's screens and the
    /// session's game start, on an app with minimal plugins.
    Headless,
}

/// The settings folder of a session whose saves are in `save_dir`, and
/// the player's settings in it. A mod game's process-wide UI settings
/// apply first, so its menu side fill is the default the settings read.
pub fn front_settings(
    save_dir: &Path,
    mods: &ClientMods,
) -> anyhow::Result<(PathBuf, super::config::Settings)> {
    if !mods.is_empty() {
        mods.apply_ui_settings();
    }
    let cfg_dir = super::config::config_dir(save_dir);
    let settings = super::config::load_settings(&cfg_dir)?;
    Ok((cfg_dir, settings))
}

/// Plays the session of `config` in a window until it closes: the front
/// end first, then each game the player starts.
pub fn run_front_end(config: FrontEndConfig) -> anyhow::Result<()> {
    let (cfg_dir, settings) = front_settings(&config.save_dir, &config.mods)?;
    // The frame of every game of the session: the one player view, or a
    // test tool's `play --res`.
    let frame = super::config::set_play_frame()?;
    println!("play: frame {} x {}", frame.width, frame.height);
    let mut app = App::new();
    // The window the player chose; the front end and the game present their
    // pictures in it.
    let window = super::config::window_for(&settings);
    let plugins = PluginGroup::set(
        crate::mute::plugins(DefaultPlugins),
        bevy::window::WindowPlugin {
            primary_window: Some(window),
            // The system pointer starts hidden: the front end draws the
            // game's cursor into its picture, and the game shows its own
            // (`app::session` keeps the pointer hidden on every screen).
            primary_cursor_options: Some(session::hidden_cursor()),
            ..Default::default()
        },
    );
    // game.log, and with the dev tools the log ring the bug report reads.
    app.add_plugins(plugins.set(crate::launch::log_plugin(config.dev)));
    super::perf::add(&mut app);
    install_front_end(&mut app, config, FrontOutput::Window, cfg_dir, settings)?;
    match app.run() {
        AppExit::Success => Ok(()),
        AppExit::Error(code) => anyhow::bail!("play exited with code {code}"),
    }
}

/// Shared between the front end's visits and the game start: the last
/// visit's screen cells and the outcome its host left.
type FrontShared = Rc<RefCell<Option<(StartHandles, Arc<Mutex<Option<Outcome>>>)>>>;

/// Builds the session of `config` on `app` (its plugins already added):
/// the front end as the session's first screen, each game made by
/// `config.make_game` and installed on the running app, back to the main
/// menu after it. `cfg_dir` and `settings`: [`front_settings`].
pub fn install_front_end(
    app: &mut App,
    config: FrontEndConfig,
    output: FrontOutput,
    cfg_dir: PathBuf,
    settings: super::config::Settings,
) -> anyhow::Result<()> {
    let FrontEndConfig {
        data,
        game_dir,
        save_dir: saves,
        files,
        mods,
        online,
        server,
        dev: _,
        sound_device,
        make_game,
    } = config;
    let GameData::Live(live) = &data;
    let live = live.clone();
    let stills = super::backdrop_bake::StillsBake::for_game(game_dir.clone());
    let front: FrontShared = Rc::new(RefCell::new(None));
    // The screens' cells, kept across games: online play's session and
    // screens outlive each game (back to the lobby on the same session).
    #[cfg_attr(not(feature = "online"), allow(unused_mut))]
    let mut start = StartHandles {
        files,
        ..StartHandles::default()
    };
    #[cfg(feature = "online")]
    {
        start.server = server;
    }
    #[cfg(not(feature = "online"))]
    let _ = server;
    // The last game was online: the front end opens at the lobby or the
    // login, with why it ended (`OnlineEnd`).
    let last_online = Rc::new(Cell::new(false));
    #[cfg(feature = "online")]
    let online_end = super::online::OnlineEnd::default();
    let starter = {
        let (front, saves, live) = (front.clone(), saves.clone(), live.clone());
        let (start, last_online) = (start.clone(), last_online.clone());
        #[cfg(feature = "online")]
        let online_end = online_end.clone();
        FrontStarter(Box::new(move |after_game| {
            use d2_data::bin::TableFiles;
            crate::launch::note("front end (main menu)");
            // Music on: the jukebox plays in the menus.
            let music = crate::audio::environment::frontend_stream::FrontMusic::new(
                live.archives.source(),
                live.archives.lod(),
                true,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(1, |t| t.subsec_nanos()),
            );
            let mut art = FrontArt::new(live.archives.source());
            if let Ok(t) =
                super::strings::TableStrings::load(live.archives.as_ref(), super::strings::LANG)
            {
                art = art.with_strings(move |id| {
                    u16::try_from(id).map(|i| t.by_id(i)).unwrap_or_default()
                });
            }
            if let Ok(app) = super::save::appearance_tables(&live.tables.fixed) {
                art = art.with_dolls(crate::ui::front_end::doll::DollTables::new(app));
            }
            // After a game: the main menu, or after an online game the
            // lobby (the login when the session is gone). `play --online`
            // opens at the login.
            let entry = match (after_game, last_online.replace(false)) {
                #[cfg(feature = "online")]
                (true, true) => super::front_start::after_online_game(&start, online_end.take()),
                (true, _) => Entry::MainMenu,
                (false, _) if online => Entry::Online,
                (false, _) => Entry::First,
            };
            let handles = start.next_visit();
            let host = front_host_with(&saves, Some(art), live.archives.lod(), entry, &handles);
            let host = host.with_music(music);
            // The lobby's market window reads the palette, fonts and item
            // tips from the game's files (once).
            #[cfg(feature = "online")]
            if let Some(o) = handles.online.borrow().as_ref() {
                let mut o = o.borrow_mut();
                if o.market_parts.is_none() {
                    o.market_parts = Some(
                        crate::ui::front_end::screens::online::market_host::loader(live.clone()),
                    );
                }
            }
            *front.borrow_mut() = Some((handles, host.outcome_handle()));
            host
        }))
    };
    // The game's leave handles (its save, the dev tools' `load`).
    let handles: Rc<RefCell<Option<PlayHandles>>> = Rc::default();
    let load: Rc<RefCell<Option<DevLoad>>> = Rc::default();
    let game = {
        let (front, saves, handles, load) =
            (front.clone(), saves.clone(), handles.clone(), load.clone());
        let (last_online, live) = (last_online.clone(), live.clone());
        let mut make_game = make_game;
        GameStarter(Box::new(move |world| {
            let next = load.borrow_mut().take();
            // The lobby joined a server game: it starts over the network,
            // without blocking the frame (the join's answer installs it).
            let online_load = front.borrow().as_ref().and_then(|(h, out)| {
                match out.lock().ok().and_then(|o| *o) {
                    Some(Outcome::GameLoad(g)) if g.online && next.is_none() => Some(h.clone()),
                    _ => None,
                }
            });
            if let Some(h) = online_load {
                last_online.set(true);
                return start_online_game(world, &live, &h, &handles);
            }
            let (choice, menu_difficulty) = match &next {
                Some(_) => (None, None),
                None => {
                    let picked = front.borrow().as_ref().and_then(|(h, out)| {
                        match out.lock().ok().and_then(|o| *o) {
                            Some(Outcome::GameLoad(g)) => {
                                StartChoice::resolve(g, h, &saves).map(|c| (c, g.difficulty))
                            }
                            _ => None,
                        }
                    });
                    let (c, d) = picked.context("the front end chose no character")?;
                    (Some(c), d)
                }
            };
            let config = make_game(FrontGame {
                data: data.clone(),
                game_dir: game_dir.clone(),
                choice,
                menu_difficulty,
                load: next,
            })?;
            let ps = play::play_settings(&config)?;
            world.insert_resource(super::ui::LoadBakedAnimations);
            let h = play::install_play(world, config, ps)?;
            *handles.borrow_mut() = Some(h);
            Ok(())
        }))
    };
    let ender = {
        let (handles, load) = (handles.clone(), load.clone());
        GameEnder(Box::new(move |world| {
            let Some(h) = handles.borrow_mut().take() else {
                return Ok(false);
            };
            let save = h.saver.as_ref().map(|s| s.path().to_path_buf());
            let end = play::play_end(world, &h, AppExit::Success)?;
            match (&end.relaunch, save) {
                (Some(slot), Some(save)) => {
                    *load.borrow_mut() = Some(DevLoad::read(slot, save)?);
                    Ok(true)
                }
                (Some(_), None) => {
                    eprintln!("play: dev load: this game has no save file");
                    Ok(false)
                }
                _ => Ok(false),
            }
        }))
    };
    match output {
        FrontOutput::Window => play::register_play(app, true),
        FrontOutput::Headless => play::register_live_client(app, false),
    }
    session::add_session(app);
    if output == FrontOutput::Window {
        super::config::add_window_choice(app, &settings, cfg_dir.clone());
    }
    // The settings in force from the first screen on: the main menu's
    // Options screen shows, saves and applies them (`config::front_settings`),
    // and the menus' frame cap reads them. A game start reloads them.
    let bindings = super::config::load_controls(&super::config::controls_dir(
        crate::ui::front_end::screens::controls::config_path().as_deref(),
        &cfg_dir,
    ))?;
    app.insert_resource(super::config::ConfigRes {
        dir: cfg_dir,
        settings,
        bindings,
        sound_device,
    });
    #[cfg(feature = "online")]
    app.insert_resource(online_end);
    // The realm lobby's stills, baked from the same files
    // (`app::backdrop_bake`).
    app.insert_resource(stills);
    // A mod game's client hooks, for the front end's screens and every
    // game of the session; D2Gamma's empty registry is not inserted.
    if !mods.is_empty() {
        app.insert_resource(mods);
    }
    app.insert_non_send(game).insert_non_send(ender);
    add_front_session(app, starter);
    Ok(())
}

/// The online game the lobby joined, on the session the screens opened
/// (`app::online::start_online`).
#[cfg(feature = "online")]
fn start_online_game(
    world: &mut World,
    live: &Arc<LiveData>,
    h: &StartHandles,
    handles: &Rc<RefCell<Option<PlayHandles>>>,
) -> anyhow::Result<()> {
    use super::online::{start_online, OnlineJoin};
    let join = h
        .online
        .borrow()
        .as_ref()
        .and_then(|o| OnlineJoin::take(&mut o.borrow_mut()))
        .context("the lobby made no join")?;
    world.insert_resource(super::ui::LoadBakedAnimations);
    start_online(world, live, &h.session, join)?;
    *handles.borrow_mut() = Some(PlayHandles {
        saver: None,
        dev_relaunch: Default::default(),
    });
    Ok(())
}

#[cfg(not(feature = "online"))]
fn start_online_game(
    _: &mut World,
    _: &Arc<LiveData>,
    _: &StartHandles,
    _: &Rc<RefCell<Option<PlayHandles>>>,
) -> anyhow::Result<()> {
    anyhow::bail!("this build has no online play")
}

/// The session of a [`FrontEndConfig`] without a window (tests, and a mod
/// game's checks of its own screens): the front end's screens and the
/// session's game start on an app with minimal plugins, stepped 40 ms per
/// frame. Opening one mutes the process (no sound device).
pub struct HeadlessFrontEnd {
    app: App,
}

impl HeadlessFrontEnd {
    /// The session of `config`, its first frames run.
    pub fn open(config: FrontEndConfig) -> anyhow::Result<HeadlessFrontEnd> {
        crate::mute::set(true);
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            AssetPlugin::default(),
            bevy::input::InputPlugin,
            bevy::state::app::StatesPlugin,
        ))
        .init_asset::<Image>()
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_millis(40),
        ));
        let (cfg_dir, settings) = front_settings(&config.save_dir, &config.mods)?;
        install_front_end(&mut app, config, FrontOutput::Headless, cfg_dir, settings)?;
        let mut s = HeadlessFrontEnd { app };
        s.frames(3);
        Ok(s)
    }

    /// Runs `n` frames.
    pub fn frames(&mut self, n: u32) {
        for _ in 0..n {
            self.app.update();
        }
    }

    /// The front end is showing (no game runs).
    pub fn in_front_end(&self) -> bool {
        self.app
            .world()
            .get_resource::<State<session::Screen>>()
            .is_some_and(|s| *s.get() == session::Screen::Front)
    }

    /// The front end's host, while it shows.
    pub fn host(&mut self) -> Option<Mut<'_, super::front_host::FrontHost>> {
        self.app
            .world_mut()
            .get_non_send_mut::<super::front_host::FrontHost>()
    }

    /// The screen the front end shows.
    pub fn screen(&mut self) -> Option<crate::ui::front_end::ScreenId> {
        self.host().map(|h| h.front.current())
    }

    /// The texts of the screen's controls (the version lines, names).
    pub fn texts(&mut self) -> Vec<String> {
        self.host().map_or_else(Vec::new, |h| {
            h.front
                .controls()
                .iter()
                .filter_map(|c| c.text.clone())
                .collect()
        })
    }

    /// Fires `t` on the screen shown, then runs two frames.
    pub fn trigger(&mut self, t: crate::ui::front_end::flow::Trigger) {
        if let Some(mut h) = self.host() {
            h.front.trigger(t);
        }
        self.frames(2);
    }

    /// Presses and releases the key `vk` (a virtual-key code), then runs
    /// two frames.
    pub fn key(&mut self, vk: u16) {
        use crate::ui::front_end::FrontInput;
        if let Some(mut h) = self.host() {
            h.front.input(FrontInput::Key(vk));
            h.front.input(FrontInput::KeyUp(vk));
        }
        self.frames(2);
    }

    /// The session's app.
    pub fn app(&mut self) -> &mut App {
        &mut self.app
    }
}
