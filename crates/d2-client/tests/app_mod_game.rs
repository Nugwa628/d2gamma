//! A mod game's inputs reach the windowed game: `play::install_play`, what
//! `play::run` installs on its window, builds the server's game under
//! `PlayConfig::rules`, the bridge speaks the mod wire, and the character is
//! saved in the game's own folder and format (`PlayConfig::saves`).

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use bevy::prelude::*;
use d2_client::app::play;
use d2_client::app::single_player::{self, CharacterCodec, D2sCodec, LiveData, Saves};
use d2_client::bridge::BridgeResource;
use d2_formats::d2s::{D2s, SaveTables};
use d2_proto::envelope::Wire;
use d2_sim::rules::{EventHooks, EventHost, ModRules, Ruleset};
use d2_sim::units::UnitId;

use crate::app_support;

/// How often the test rules' events ran.
#[derive(Debug, Default)]
struct Seen {
    created: AtomicU32,
    joined: AtomicU32,
    handlers: AtomicU32,
}

#[derive(Debug)]
struct CountingEvents(Arc<Seen>);

impl EventHooks for CountingEvents {
    fn game_created(&self, _: &mut dyn EventHost) {
        self.0.created.fetch_add(1, Ordering::SeqCst);
    }
    fn player_joined(&self, _: &mut dyn EventHost, _: UnitId) {
        self.0.joined.fetch_add(1, Ordering::SeqCst);
    }
}

/// A mod game's rules that only count the game's events.
#[derive(Debug)]
struct CountingRules(CountingEvents);

impl ModRules for CountingRules {
    fn name(&self) -> &str {
        "windowed-test-mod"
    }
    fn events(&self) -> Option<&dyn EventHooks> {
        Some(&self.0)
    }
}

/// The test game's own format: a tag in front of the `.d2s` bytes.
#[derive(Debug)]
struct Tagged;

const TAG: &[u8] = b"MODG";

impl CharacterCodec for Tagged {
    fn extension(&self) -> &str {
        "modsave"
    }
    fn read(&self, data: &LiveData, bytes: &[u8], difficulty: u8) -> Result<D2s, String> {
        let body = bytes.strip_prefix(TAG).ok_or("no tag")?;
        D2sCodec.read(data, body, difficulty)
    }
    fn write(&self, save: &D2s, tables: &dyn SaveTables) -> Result<Vec<u8>, String> {
        let mut out = TAG.to_vec();
        out.extend(D2sCodec.write(save, tables)?);
        Ok(out)
    }
}

fn in_game(app: &App) -> bool {
    app.world()
        .get_resource::<BridgeResource>()
        .is_some_and(|b| b.0.world().in_game && b.0.world().local().is_some())
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn the_windowed_game_runs_the_mod_games_rules_and_saves() {
    run_mod_game(false);
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn the_windowed_game_makes_one_mod_message_handler() {
    run_mod_game(true);
}

struct IgnoreModMessages;

impl d2_client::bridge::ModMessages for IgnoreModMessages {
    fn message(&mut self, _: &mut d2_client::bridge::world::ClientWorld, _: &[u8]) {}
}

fn run_mod_game(with_messages: bool) {
    let data = app_support::game_data();
    let dir = std::env::temp_dir().join(format!(
        "d2rust-windowed-mod-{}-{with_messages}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let saves = Saves::new(dir.join("saves"), Arc::new(Tagged));
    let save_path = saves.file("ModHero");
    let seen = Arc::new(Seen::default());
    let mut mods = d2_client::mods::ClientMods::default();
    if with_messages {
        let seen = seen.clone();
        mods = mods.with_mod_messages(move || {
            seen.handlers.fetch_add(1, Ordering::SeqCst);
            Box::new(IgnoreModMessages)
        });
    }
    let config = play::PlayConfig {
        data,
        seed: single_player::DEFAULT_SEED,
        rules: Ruleset::Mod(Arc::new(CountingRules(CountingEvents(seen.clone())))),
        saves,
        mods,
        character: single_player::new_character("sorceress", "ModHero").unwrap(),
        exit_after: None,
        save_path: Some(save_path.clone()),
        hardcore: false,
        start_flags: None,
        dump: None,
        input: None,
        sound_log: None,
        frame_schedule: None,
        no_sound: true,
        device_ends: None,
        async_collects: None,
        registry: None,
        audio_dump: None,
        loading_dump: None,
        pokes: Vec::new(),
        sends: Vec::new(),
        dev: false,
        difficulty: 0,
        dev_start: None,
        offscreen: None,
        control: None,
    };
    let ms = Arc::new(AtomicU32::new(1000));
    let mut app = App::new();
    app.insert_resource(d2_client::bridge::mirror::ScriptedClock(ms.clone()));
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        bevy::state::app::StatesPlugin,
    ))
    .init_asset::<Image>()
    .init_resource::<ButtonInput<MouseButton>>()
    .add_message::<bevy::input::keyboard::KeyboardInput>()
    .add_message::<bevy::window::WindowScaleFactorChanged>();
    play::register_play(&mut app, false);
    // Headless: the systems that need a window skip with a warning.
    app.insert_resource(bevy::ecs::error::FallbackErrorHandler(
        bevy::ecs::error::warn,
    ));
    let ps = play::play_settings(&config).unwrap();
    let _handles = play::install_play(&mut app, config, ps).unwrap();
    // The mod game's host speaks the mod wire, and so does the window's
    // bridge.
    assert_eq!(app.world().resource::<BridgeResource>().0.wire(), Wire::Mod);
    assert_eq!(
        seen.handlers.load(Ordering::SeqCst),
        u32::from(with_messages),
        "one handler per game when registered"
    );
    let step = |app: &mut App| {
        app.update();
        ms.fetch_add(40, Ordering::SeqCst);
    };
    let mut frames = 0;
    while !in_game(&app) {
        step(&mut app);
        frames += 1;
        assert!(frames < 2000, "the join finishes");
    }
    for _ in 0..10 {
        step(&mut app);
    }
    // The game was built under the mod game's rules: its events ran.
    assert_eq!(seen.created.load(Ordering::SeqCst), 1, "game created once");
    assert_eq!(seen.joined.load(Ordering::SeqCst), 1, "one player joined");
    // Save and Exit writes the character in the game's own format, into
    // the game's own folder.
    app.world_mut()
        .resource_mut::<BridgeResource>()
        .0
        .save_and_exit()
        .unwrap();
    let mut frames = 0;
    while in_game(&app) {
        step(&mut app);
        frames += 1;
        assert!(frames < 500, "the leave finishes");
    }
    for _ in 0..5 {
        step(&mut app);
    }
    let bytes = std::fs::read(&save_path).expect("the character was saved");
    assert!(bytes.starts_with(TAG), "saved in the game's own format");
    assert!(!dir.join("saves").join("ModHero.d2s").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
