//! The dev console's `level n` on the synthetic playable install: a level
//! past the experience table raises the character to the table's max
//! level instead of reading past its end.

use std::path::PathBuf;
use std::sync::OnceLock;

use d2_proto::dev::DevCommand;
use d2_server::adapters::ProtoSizes;
use d2_server::dev::{DevCatalog, DevServer};
use d2_server::game::{create_request_for, new_character, GameBuilder, GameData, Sim};
use d2_server::host::Host;
use d2_server::seams::{ClientId, Clock, MessageSink, SessionHandler};
use d2_sim::stats::stat;

fn data() -> GameData {
    static DATA: OnceLock<GameData> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("dev-level-{}", std::process::id()));
        test_fixtures::install::build(&dir, &test_fixtures::play::playable())
            .unwrap_or_else(|e| panic!("{e}"));
        GameData::select(Some(&dir)).expect("the playable install loads")
    })
    .clone()
}

struct NoSession;

impl SessionHandler for NoSession {
    fn system_message(&mut self, _: ClientId, _: &[u8], _: usize, _: &mut dyn MessageSink) {}
}

#[derive(Default)]
struct Steps(u32);

impl Clock for Steps {
    fn now_ms(&mut self) -> u32 {
        self.0
    }
}

/// A game with one joined amazon (client 0).
fn joined() -> Host<Sim, ProtoSizes, NoSession, Steps> {
    let character = new_character("amazon", "DevLevel").expect("a valid new character");
    let sim = GameBuilder::new(data())
        .character(character.clone())
        .build()
        .expect("the game builds");
    let mut host = Host::new(sim, ProtoSizes, NoSession, Steps(1000));
    host.connect(0);
    let req = create_request_for(&character, 800, 600);
    host.send_system(0, &req.encode()).expect("queued");
    let mut asked = false;
    for _ in 0..200 {
        host.clock.0 += 40;
        host.frame().expect("frame");
        let got = host.receive(0);
        if !asked && got.iter().any(|m| m.first() == Some(&0x02)) {
            host.send_system(0, &[0x6B]).expect("queued");
            asked = true;
        }
        if asked && host.game.player_of(0).is_some() {
            return host;
        }
    }
    panic!("the player did not join");
}

fn level(host: &Host<Sim, ProtoSizes, NoSession, Steps>) -> i32 {
    let p = host.game.player_of(0).expect("player");
    host.game
        .events
        .action
        .sys
        .stats
        .unit_base(p, stat::LEVEL, 0)
}

#[test]
fn level_past_the_table_is_the_max_level() {
    let mut host = joined();
    let GameData::Live(d) = data();
    let mut dev = DevServer::new(DevCatalog::from_live(&d, |_| String::new()));
    let p = host.game.player_of(0);
    let r = dev.apply(&mut host.game, p, &DevCommand::Level(120));
    assert!(r.ok, "{}", r.text);
    let max = level(&host);
    assert!(max > 1 && max < 120, "level {max}");
    assert!(r.text.contains("the max level"), "{}", r.text);
    // At the max level: `level +n` stays there.
    let r = dev.apply(&mut host.game, p, &DevCommand::LevelAdd(5));
    assert!(r.ok, "{}", r.text);
    assert_eq!(level(&host), max);
}
