//! The one-app session, headless: three games in one app (front end ->
//! game -> front end), each started on the running app's world and ended
//! by Save and Exit. Every game is the same as the first; nothing a game
//! installed is left after it; the process memory does not grow per game.
//! A game the main menu starts (`play::install_play` on the running app)
//! has the shared stash, as a `play --save` game does.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use d2_client::app::front_start::StartChoice;
use d2_client::app::play::{self, install_live_client, register_live_client, LiveClient};
use d2_client::app::play_start;
use d2_client::app::session::{self, GameEnder, GameEntity, GameStarter, Screen};
use d2_client::app::shared_stash::SharedStash;
use d2_client::app::single_player::{self, GameData};
use d2_client::bridge::BridgeResource;
use d2_client::world_view::ui_bind::ui_elements;
use d2_client::world_view::WorldViewUi;
use d2_server::seams::Clock;

use crate::app_support;

struct StepClock(Arc<AtomicU32>);

impl Clock for StepClock {
    fn now_ms(&mut self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

/// The allocator settings the memory check runs under: one arena that
/// hands freed pages back at once. With the default per-thread arenas
/// the resident set keeps what each game's threads freed: it grew 25 to
/// 30 MiB a game over eight games, by the allocator, not by what a game
/// left behind.
const COMPACT_HEAP: &str = "glibc.malloc.arena_max=1:glibc.malloc.trim_threshold=0";

/// The resident set (`VmRSS`, KiB) of this process.
fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("VmRSS:"))
                .and_then(|v| v.trim().trim_end_matches("kB").trim().parse().ok())
        })
        .unwrap_or(0)
}

fn own_resources(world: &World) -> std::collections::BTreeSet<bevy::ecs::component::ComponentId> {
    world.iter_resources().map(|(i, _)| i.id()).collect()
}

fn live_entities(app: &mut App) -> usize {
    let w = app.world_mut();
    w.query::<Entity>().iter(w).count()
}

fn game_entities(app: &mut App) -> usize {
    let w = app.world_mut();
    w.query_filtered::<(), With<GameEntity>>().iter(w).count()
}

fn in_game(app: &App) -> bool {
    app.world()
        .get_resource::<BridgeResource>()
        .is_some_and(|b| b.0.world().in_game && b.0.world().local().is_some())
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn three_games_in_one_app_leave_nothing_behind() {
    // The allocator reads its settings at start, so the games run in a
    // copy of this test process started with them.
    if std::env::var("GLIBC_TUNABLES").as_deref() != Ok(COMPACT_HEAP) {
        let name = concat!(
            module_path!(),
            "::three_games_in_one_app_leave_nothing_behind"
        );
        let name = name.split_once("::").map_or(name, |(_, n)| n);
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                name,
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GLIBC_TUNABLES", COMPACT_HEAP)
            .status()
            .unwrap();
        assert!(status.success(), "the three games: {status}");
        return;
    }
    let data = app_support::game_data();
    let GameData::Live(live) = data.clone();
    let ms = Arc::new(AtomicU32::new(1000));
    let mut app = App::new();
    app.insert_resource(d2_client::bridge::mirror::ScriptedClock(ms.clone()));
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
        .init_asset::<Image>()
        .init_resource::<ButtonInput<MouseButton>>();
    register_live_client(&mut app, false);
    session::add_session(&mut app);
    let starts = Arc::new(AtomicU32::new(0));
    let game = {
        let (ms, starts) = (ms.clone(), starts.clone());
        GameStarter(Box::new(move |world: &mut World| {
            let character = single_player::Character::New;
            let seed = single_player::game_seed(&character, None);
            let (link, started) = single_player::start_with(
                data.clone(),
                seed,
                character.clone(),
                StepClock(ms.clone()),
            )?;
            let speeds = single_player::walk_speeds(&data, &character)?;
            install_live_client(
                world,
                Box::new(link),
                LiveClient {
                    data: &live,
                    request: &character,
                    start_flags: None,
                    prices: started.prices,
                    speeds,
                    hardcore: false,
                    automap_files: None,
                    gpu: false,
                },
            )?;
            starts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }))
    };
    app.insert_non_send(game)
        .insert_non_send(GameEnder(Box::new(|_| Ok(false))));
    let step = |app: &mut App| {
        app.update();
        ms.fetch_add(40, Ordering::SeqCst);
    };
    // The front end's frames before the first game (the engine adds its
    // own lazy resources in the first passes).
    for _ in 0..5 {
        step(&mut app);
    }
    let mut front = own_resources(app.world());
    let mut entities = live_entities(&mut app);
    let mut rss = Vec::new();
    for round in 1..=3u32 {
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(Screen::Game);
        let mut frames = 0;
        while !in_game(&app) {
            step(&mut app);
            frames += 1;
            assert!(frames < 2000, "game {round}: the join finishes");
        }
        for _ in 0..50 {
            step(&mut app);
        }
        assert_eq!(starts.load(Ordering::SeqCst), round);
        assert!(game_entities(&mut app) > 0);
        // Save and Exit: the server's leave takes the client out; the
        // session goes back to the front end.
        app.world_mut()
            .resource_mut::<BridgeResource>()
            .0
            .save_and_exit()
            .unwrap();
        let mut frames = 0;
        while *app.world().resource::<State<Screen>>().get() != Screen::Front {
            step(&mut app);
            frames += 1;
            assert!(frames < 500, "game {round}: the leave finishes");
        }
        step(&mut app);
        // After the first game: the engine's record of the first state
        // change and the session's game counter; nothing else.
        let now = own_resources(app.world());
        if round == 1 {
            use std::any::TypeId;
            let allowed = [
                TypeId::of::<bevy::state::state::PreviousState<Screen>>(),
                TypeId::of::<session::GameGeneration>(),
            ];
            for id in now.difference(&front) {
                let t = app
                    .world()
                    .components()
                    .get_info(*id)
                    .and_then(|i| i.type_id());
                assert!(
                    t.is_some_and(|t| allowed.contains(&t)),
                    "game 1 left resource {id:?}"
                );
            }
            front = now;
        } else {
            assert_eq!(now, front, "game {round}: resources left");
        }
        assert_eq!(game_entities(&mut app), 0);
        // The entities: a resource is an entity, kept empty when the
        // resource goes and reused when it comes back, so the first game
        // sets the count every later game ends with.
        let count = live_entities(&mut app);
        if round == 1 {
            entities = count;
        } else {
            assert_eq!(count, entities, "game {round}: entities left");
        }
        rss.push(rss_kib());
        println!("after game {round}: rss {} KiB", rss_kib());
    }
    // Measured: the second and third games end within 9 MiB of the first
    // (under 1 MiB a game from the third on); a game server left running
    // each game grows it 62 MiB over the two.
    let growth = rss[2].saturating_sub(rss[0]);
    assert!(
        growth < 16 * 1024,
        "rss grew {growth} KiB over two games: {rss:?}"
    );
}

/// The front end's choice of `name`: a new character of `status`, or
/// (`load`) its file in `dir`.
fn menu_choice(dir: &std::path::Path, name: &str, status: u16, load: bool) -> StartChoice {
    let file = dir.join(format!("{name}.d2s"));
    StartChoice {
        name: name.into(),
        class: 1,
        status,
        difficulty: 0,
        save: load.then(|| file.clone()),
        file,
    }
}

/// `play`'s config for the front end's `choice`, as `main` builds it
/// (`play_start::resolve` with the menu's choice, no command-line start).
fn menu_config(data: &GameData, dir: &std::path::Path, choice: &StartChoice) -> play::PlayConfig {
    let start = play_start::resolve(
        &play_start::CliStart::default(),
        data,
        None,
        Some(choice.difficulty),
        Some(choice),
    )
    .unwrap();
    assert_eq!(
        start.save_path.as_deref().and_then(|p| p.parent()),
        Some(dir)
    );
    play::PlayConfig {
        data: data.clone(),
        seed: single_player::DEFAULT_SEED,
        rules: d2_sim::rules::Ruleset::Original,
        saves: single_player::Saves::d2rust(),
        mods: d2_client::mods::ClientMods::default(),
        character: start.character,
        exit_after: None,
        save_path: start.save_path,
        hardcore: start.hardcore,
        start_flags: start.start_flags,
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
        difficulty: start.difficulty,
        dev_start: None,
        offscreen: None,
        control: None,
    }
}

/// The owner's report: a game started from the main menu (single player, a
/// character) is installed on the running app's `World`, where the shared
/// stash could not add its system, so the stash showed the game's plain
/// panel and the inventory no send button. Every kind of menu start, in one
/// session as a player plays them: a new character, the same one loaded,
/// a new hardcore one and a new classic one.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn main_menu_games_open_the_shared_stash() {
    use d2_client::ui::front_end::screens::difficulty::{STATUS_EXPANSION, STATUS_HARDCORE};
    use d2_client::ui::states::id;
    let data = app_support::game_data();
    let dir = std::env::temp_dir().join(format!("d2rust-menu-stash-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ms = Arc::new(AtomicU32::new(1000));
    let mut app = App::new();
    app.insert_resource(d2_client::bridge::mirror::ScriptedClock(ms.clone()));
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), StatesPlugin))
        .init_asset::<Image>()
        .init_resource::<ButtonInput<MouseButton>>()
        // What the window and input plugins give `main`'s app.
        .add_message::<bevy::input::keyboard::KeyboardInput>()
        .add_message::<bevy::window::WindowScaleFactorChanged>();
    // As `main` builds the session's app.
    play::register_play(&mut app, false);
    session::add_session(&mut app);
    // Headless: the systems that need a window or a keyboard skip with a
    // warning instead of ending the process.
    app.insert_resource(bevy::ecs::error::FallbackErrorHandler(
        bevy::ecs::error::warn,
    ));
    let next: Rc<RefCell<Option<StartChoice>>> = Rc::default();
    let handles: Rc<RefCell<Option<play::PlayHandles>>> = Rc::default();
    let game = {
        let (next, handles, data, dir) = (next.clone(), handles.clone(), data.clone(), dir.clone());
        GameStarter(Box::new(move |world: &mut World| {
            let choice = next.borrow_mut().take().expect("a menu choice");
            let config = menu_config(&data, &dir, &choice);
            let ps = play::play_settings(&config)?;
            *handles.borrow_mut() = Some(play::install_play(world, config, ps)?);
            Ok(())
        }))
    };
    let ender = {
        let handles = handles.clone();
        GameEnder(Box::new(move |world: &mut World| {
            if let Some(h) = handles.borrow_mut().take() {
                play::play_end(world, &h, AppExit::Success)?;
            }
            Ok(false)
        }))
    };
    app.insert_non_send(game).insert_non_send(ender);
    let step = |app: &mut App| {
        app.update();
        ms.fetch_add(40, Ordering::SeqCst);
    };
    for _ in 0..5 {
        step(&mut app);
    }
    let games = [
        ("new", menu_choice(&dir, "Stasher", STATUS_EXPANSION, false)),
        (
            "loaded",
            menu_choice(&dir, "Stasher", STATUS_EXPANSION, true),
        ),
        (
            "new hardcore",
            menu_choice(&dir, "HardStash", STATUS_EXPANSION | STATUS_HARDCORE, false),
        ),
        ("new classic", menu_choice(&dir, "OldStash", 0, false)),
    ];
    for (what, choice) in games {
        *next.borrow_mut() = Some(choice);
        app.world_mut()
            .resource_mut::<NextState<Screen>>()
            .set(Screen::Game);
        let mut frames = 0;
        while !in_game(&app) {
            step(&mut app);
            frames += 1;
            assert!(frames < 2000, "{what}: the join finishes");
        }
        for _ in 0..20 {
            step(&mut app);
        }
        assert!(
            app.world().contains_resource::<SharedStash>(),
            "{what}: the store opened"
        );
        // The inventory and the stash, as the player opens them.
        {
            let mut ui = app.world_mut().non_send_mut::<WorldViewUi>();
            let o = ui.original.as_mut().expect("the game's UI");
            for s in [id::INVENTORY, id::STASH] {
                o.set_ui(u32::from(s), 0, false).unwrap();
            }
        }
        for _ in 0..3 {
            step(&mut app);
        }
        let world = app.world();
        let ui = world.non_send::<WorldViewUi>();
        let o = ui.original.as_ref().unwrap();
        assert!(o.stash_open(), "{what}: the stash panel is open");
        assert!(o.shared_stash().on, "{what}: the shared stash panel is on");
        let ids: Vec<String> = ui_elements(
            &ui.root,
            world.resource::<BridgeResource>().0.world(),
            &*ui.strings,
        )
        .into_iter()
        .map(|e| e.id)
        .collect();
        for want in ["stash.tab(I)", "inventory.send_stackables"] {
            assert!(
                ids.iter().any(|i| i == want),
                "{what}: no {want} among {ids:?}"
            );
        }
        app.world_mut()
            .resource_mut::<BridgeResource>()
            .0
            .save_and_exit()
            .unwrap();
        let mut frames = 0;
        while *app.world().resource::<State<Screen>>().get() != Screen::Front {
            step(&mut app);
            frames += 1;
            assert!(frames < 500, "{what}: the leave finishes");
        }
        step(&mut app);
        assert!(!app.world().contains_resource::<SharedStash>());
    }
    assert!(dir.join("Stasher.d2s").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}
