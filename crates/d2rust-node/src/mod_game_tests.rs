//! A mod game's game server ([`crate::NodeGame`]): its hello names its
//! game and version, a main server of another game refuses it
//! (`NodeGame`), and the games it starts for its main server play the
//! mod game's rules.

use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use d2_games::{AccountId, AttachedStream, GameRules, GamesConfig, NewChar, Seat};
use d2_link::{Bundle, Versions};
use d2_net::msg::node::{ItemIdBlock, NodeGameSpec, NodeToRealm};
use d2_net::msg::ErrorCode;
use d2_net::{pair, Lane, MemoryEnd, NetError, Packet, Transport};
use d2_server::game::{create_request_for, new_character};
use d2_service::log::ServerLog;
use d2_sim::rules::{EventHooks, EventHost, ModRules, Ruleset};
use d2_sim::units::UnitId;

use super::NodeGames;
use crate::tickets::Tickets;
use crate::{NodeGame, GAME, VERSION};

/// A tiny mod game: its rules count the players that joined its games.
#[derive(Debug, Default)]
struct TinyMod {
    joined: AtomicU32,
}

impl ModRules for TinyMod {
    fn name(&self) -> &str {
        "tiny"
    }
    fn events(&self) -> Option<&dyn EventHooks> {
        Some(self)
    }
}

impl EventHooks for TinyMod {
    fn player_joined(&self, _: &mut dyn EventHost, _: UnitId) {
        self.joined.fetch_add(1, Ordering::SeqCst);
    }
}

/// The tiny mod game's game server.
fn tiny_game(tiny: Arc<TinyMod>) -> NodeGame {
    NodeGame {
        program: "tiny-node".into(),
        game: "tiny".into(),
        version: "3.1.4.1".into(),
        rules: GameRules {
            rules: Ruleset::Mod(tiny),
            ..GameRules::default()
        },
        ..NodeGame::own()
    }
}

fn bundle(game: &str) -> Bundle {
    Bundle {
        id: "eu-1".into(),
        realm_addr: "203.0.113.10:6112".parse().unwrap(),
        realm_cert: [0xab; 32],
        game: game.into(),
        ticket_keys: vec![],
    }
}

#[test]
fn a_mod_games_hello_names_its_game_and_only_its_main_server_takes_it() {
    let tiny = tiny_game(Arc::default());
    let hello = tiny
        .versions(tiny.version.clone(), [5; 32])
        .hello("eu-1", 6113, 4, 32);
    assert_eq!(hello.game, "tiny");
    assert_eq!(hello.version, "3.1.4.1");
    assert_eq!(tiny.log_file(), "tiny-node.log");

    // The mod game's main server takes it; D2RUST's refuses it.
    let tiny_realm = tiny.versions("3.1.4.1".into(), [5; 32]);
    assert_eq!(tiny_realm.check(&hello), Ok(()));
    let own_realm = Versions {
        game: GAME.into(),
        ..tiny_realm.clone()
    };
    assert_eq!(
        own_realm.check(&hello).unwrap_err().code,
        ErrorCode::NODE_GAME
    );

    // D2RUST's own game server is unchanged: game `d2rust` at the
    // build's version, which the mod game's main server refuses.
    let own = NodeGame::own();
    assert_eq!(
        (
            own.program.as_str(),
            own.game.as_str(),
            own.version.as_str()
        ),
        ("d2rust-node", "d2rust", VERSION)
    );
    assert!(own.rules.rules.is_original());
    assert_eq!(own.log_file(), "d2rust-node.log");
    let own_hello = own
        .versions("3.1.4.1".into(), [5; 32])
        .hello("eu-1", 6113, 4, 32);
    assert_eq!(own_realm.check(&own_hello), Ok(()));
    assert_eq!(
        tiny_realm.check(&own_hello).unwrap_err().code,
        ErrorCode::NODE_GAME
    );

    // A bundle another game's main server wrote is flagged.
    assert_eq!(own.check_bundle(&bundle("d2rust")), Ok(()));
    assert_eq!(tiny.check_bundle(&bundle("tiny")), Ok(()));
    let e = tiny.check_bundle(&bundle("d2rust")).unwrap_err();
    assert!(e.contains("d2rust") && e.contains("tiny"), "{e}");
}

#[test]
fn config_game_confirms_the_binary_identity_and_cannot_override_it() {
    let tiny = tiny_game(Arc::default());
    for game in [NodeGame::own(), tiny] {
        for configured in [None, Some(game.game.as_str()), Some("other")] {
            let text = format!(
                "bundle = node.bundle\ngame_data = game\n{}",
                configured.map_or(String::new(), |id| format!("game = {id}\n"))
            );
            let cfg = crate::config::NodeConfig::parse(&text, Path::new("node")).unwrap();
            let result = cfg.check_game(&game.game);
            if configured == Some("other") {
                let e = result.unwrap_err();
                assert!(e.contains("other") && e.contains(&game.game), "{e}");
            } else {
                assert_eq!(result, Ok(()));
                assert_eq!(cfg.check_bundle(&bundle(&game.game)), Ok(()));
            }
            let hello = game
                .versions(game.version.clone(), [5; 32])
                .hello("eu-1", 6113, 4, 32);
            assert_eq!(hello.game, game.game);
        }
    }
}

#[test]
fn run_check_and_probe_refuse_config_game_mismatch_before_opening_the_bundle() {
    let path = std::env::temp_dir().join(format!(
        "d2rust-node-config-game-mismatch-{}.cfg",
        std::process::id()
    ));
    std::fs::write(
        &path,
        "bundle = missing\ngame_data = missing\ngame = other\n",
    )
    .unwrap();
    for game in [NodeGame::own(), tiny_game(Arc::default())] {
        for command in [
            crate::Command::Run,
            crate::Command::Check,
            crate::Command::Probe,
        ] {
            let e = crate::run(&game, &path, command).unwrap_err();
            assert!(e.contains("config game = other"), "{e}");
            assert!(e.contains(&format!("runs {}", game.game)), "{e}");
        }
    }
    std::fs::remove_file(path).unwrap();
}

/// The server end of an in-memory game stream; the welcome is kept.
struct TestStream {
    end: MemoryEnd,
    welcome: Arc<Mutex<Option<Vec<u8>>>>,
}

impl Transport for TestStream {
    fn send(&mut self, p: Packet) -> Result<(), NetError> {
        self.end.send(p)
    }
    fn recv(&mut self) -> Result<Option<Packet>, NetError> {
        self.end.recv()
    }
}

impl AttachedStream for TestStream {
    fn send_body(&mut self, body: Vec<u8>) -> Result<(), NetError> {
        *self.welcome.lock().unwrap() = Some(body);
        Ok(())
    }
}

#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_game_started_on_a_mod_games_server_plays_its_rules() {
    let tiny = Arc::new(TinyMod::default());
    let game = tiny_game(tiny.clone());
    let dir = std::env::temp_dir().join(format!("d2rust-node-mod-game-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = Arc::new(ServerLog::open_named(&dir, "test.log").unwrap());
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();
    let data_dir = std::env::var_os("D2_GAME_DIR").expect("D2_GAME_DIR");
    let data = crate::game_data(Path::new(&data_dir), &game.layers);
    let cfg = GamesConfig {
        max_games: 1,
        max_players: 8,
        grace: Duration::from_secs(2),
        frame_pause: Duration::from_millis(1),
        save_interval: Duration::from_secs(3600),
        unused_close: Duration::from_secs(60),
        dev_test: false,
        invent_stash_items: false,
    };
    let games = NodeGames::new(
        cfg,
        data,
        game.rules.clone(),
        (game.version.clone(), [0; 32]),
        Arc::new(Tickets::default()),
        log,
        rt.handle().clone(),
    );

    // The main server starts a game here, and a player takes its seat.
    let spec = NodeGameSpec {
        game: 1,
        name: "tiny1".into(),
        difficulty: 0,
        hardcore: false,
        expansion: true,
        max_players: 0,
    };
    let started = games.start(
        &spec,
        7,
        ItemIdBlock {
            first: 1000,
            count: 1000,
        },
    );
    assert!(
        matches!(started, NodeToRealm::GameStarted { game: 1 }),
        "{started:?}"
    );
    let g = games.game(1).expect("the game is open");
    let (mut end, server_end) = pair();
    let welcome = Arc::new(Mutex::new(None));
    g.admit(
        Seat {
            account: AccountId(1),
            character: "Tinker".into(),
            lease: 1,
            record: None,
            new: Some(NewChar {
                name: "Tinker".into(),
                class: 2,
                hardcore: false,
                expansion: true,
                progression: 0,
            }),
        },
        TestStream {
            end: server_end,
            welcome: welcome.clone(),
        },
    )
    .unwrap();
    assert!(welcome.lock().unwrap().is_some(), "a welcome");
    let join = create_request_for(&new_character("necromancer", "Tinker").unwrap(), 800, 600);
    let _ = end.send(Packet::Send {
        lane: Lane::System,
        msg: join.encode().to_vec(),
    });

    // The game client's start: after the server's 0x02, the 0x6B. The
    // mod's hook runs once the player joined.
    let until = Instant::now() + Duration::from_secs(30);
    let mut entered = false;
    while tiny.joined.load(Ordering::SeqCst) == 0 {
        while let Some(p) = end.recv().unwrap() {
            if let Packet::Deliver(m) = p {
                if m.first() == Some(&0x02) && !entered {
                    entered = true;
                    let _ = end.send(Packet::Send {
                        lane: Lane::System,
                        msg: vec![0x6B],
                    });
                }
            }
        }
        assert!(Instant::now() < until, "the mod's join hook never ran");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(tiny.joined.load(Ordering::SeqCst), 1);
    g.shutdown();
    let _ = std::fs::remove_dir_all(&dir);
}
