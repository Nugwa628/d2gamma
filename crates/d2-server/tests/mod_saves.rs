//! Save seams on the synthetic playable install: a mod game's own sections
//! in a character save (`SaveHooks`), the stats the stat table marks saved
//! (with their params), the item flags a save and a reload leave, and a
//! new character's waypoint records.
//! The install gets one stat past the fixture's own, marked saved and keyed
//! by a param.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use d2_formats::d2s::{self, D2s, ItemEntry, ModSection, StatEntry};
use d2_proto::dev::{DevCommand, Quality, Target};
use d2_server::adapters::character::LoadContext;
use d2_server::adapters::ProtoSizes;
use d2_server::dev::{DevCatalog, DevServer};
use d2_server::game::character_save::{apply_live, base_save, read_live, Live};
use d2_server::game::{create_request_for, new_character, Character, GameBuilder, GameData, Sim};
use d2_server::host::Host;
use d2_server::seams::{ClientId, Clock, MessageSink, SessionHandler};
use d2_sim::rules::{EventHost, ModRules, Ruleset, SaveHooks};
use d2_sim::units::UnitId;
use d2_sim::world::waypoints::WaypointRecord;

/// The install, and the id of its extra saved stat.
fn data() -> &'static (GameData, u16) {
    static DATA: OnceLock<(GameData, u16)> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("mod-saves-{}", std::process::id()));
        let mut synth = test_fixtures::play::playable();
        let id = synth.tables.files["itemstatcost.txt"].rows.len() as u16;
        synth.tables.row(
            "itemstatcost",
            &[
                ("stat", "counters"),
                ("send bits", "32"),
                ("save bits", "32"),
                ("csvbits", "32"),
                ("csvparam", "16"),
                ("saved", "1"),
            ],
        );
        test_fixtures::install::build(&dir, &synth).unwrap_or_else(|e| panic!("{e}"));
        let data = GameData::select(Some(&dir)).expect("the playable install loads");
        (data, id)
    })
}

fn counters() -> u16 {
    data().1
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

/// One game with one player (client 0).
struct Game {
    host: Host<Sim, ProtoSizes, NoSession, Steps>,
    character: Character,
    dev: DevServer,
}

impl Game {
    fn start(character: Character, rules: Ruleset) -> Game {
        let sim = GameBuilder::new(data().0.clone())
            .character(character.clone())
            .rules(rules)
            .build()
            .expect("the game builds");
        let mut host = Host::new(sim, ProtoSizes, NoSession, Steps(1000));
        host.connect(0);
        let GameData::Live(d) = &data().0;
        let dev = DevServer::new(DevCatalog::from_live(d, |_| String::new()));
        let mut g = Game {
            host,
            character,
            dev,
        };
        let req = create_request_for(&g.character, 800, 600);
        g.host.send_system(0, &req.encode()).expect("queued");
        let mut asked = false;
        for _ in 0..200 {
            let got = g.frame();
            if !asked && got.iter().any(|m| m.first() == Some(&0x02)) {
                g.host.send_system(0, &[0x6B]).expect("queued");
                asked = true;
            }
            if asked && g.host.game.player_of(0).is_some() {
                for _ in 0..4 {
                    g.frame();
                }
                return g;
            }
        }
        panic!("the player did not join");
    }

    fn frame(&mut self) -> Vec<Vec<u8>> {
        self.host.clock.0 += 40;
        self.host.frame().expect("frame");
        self.host.receive(0)
    }

    fn player(&self) -> UnitId {
        self.host.game.player_of(0).expect("player")
    }

    fn give(&mut self, code: &str) {
        let p = self.host.game.player_of(0);
        let r = self.dev.apply(
            &mut self.host.game,
            p,
            &DevCommand::Item {
                item: Target::Name(code.into()),
                quality: Quality::Normal,
                ilvl: Some(5),
                ground: false,
            },
        );
        assert!(r.ok, "{}", r.text);
    }

    fn set_stat(&mut self, id: u16, layer: u16, value: i32) {
        let p = self.player();
        let sim = &mut self.host.game;
        sim.events.action.with(&mut sim.game, |_, v| {
            v.stats.unit_set(&mut *v.h, p, id, value, layer)
        });
    }

    fn base_stat(&mut self, id: u16, layer: u16) -> i32 {
        let p = self.player();
        let sim = &mut self.host.game;
        sim.events
            .action
            .with(&mut sim.game, |_, v| v.stats.unit_base(p, id, layer))
    }

    fn live(&mut self) -> Live {
        let p = self.player();
        read_live(&mut self.host.game, p)
    }

    fn save(&mut self) -> D2s {
        let live = self.live();
        apply_live(&base_save(&self.character), &live, 1_790_000_000)
    }
}

fn loaded(save: &D2s) -> Character {
    Character::Save(
        Box::new(save.clone()),
        LoadContext {
            difficulty: 0,
            map_seed_applies: false,
        },
    )
}

fn sections(save: &D2s) -> &[ModSection] {
    &save.body.as_ref().expect("a body").mod_sections
}

/// Test save hooks: the mod's own per-player value (behind its lock) is
/// saved as section `CNTR`; a load hands it back, and every load is
/// logged.
#[derive(Debug, Default)]
struct Hooks {
    value: Mutex<u32>,
    loads: Mutex<Vec<Vec<ModSection>>>,
}

impl SaveHooks for Hooks {
    fn save_sections(&self, _: &mut dyn EventHost, _: UnitId) -> Vec<ModSection> {
        let v = *self.value.lock().unwrap();
        vec![ModSection::new(*b"CNTR", v.to_le_bytes().to_vec())]
    }
    fn load_sections(&self, _: &mut dyn EventHost, _: UnitId, sections: &[ModSection]) {
        if let Some(s) = d2s::mod_sections::find(sections, b"CNTR") {
            *self.value.lock().unwrap() = u32::from_le_bytes(s.data[..4].try_into().unwrap());
        }
        self.loads.lock().unwrap().push(sections.to_vec());
    }
}

#[derive(Debug)]
struct TestGame(Arc<Hooks>);

impl ModRules for TestGame {
    fn name(&self) -> &str {
        "test-saves"
    }
    fn saves(&self) -> Option<&dyn SaveHooks> {
        Some(self.0.as_ref())
    }
}

fn modded() -> (Arc<Hooks>, Ruleset) {
    let hooks = Arc::new(Hooks::default());
    (hooks.clone(), Ruleset::Mod(Arc::new(TestGame(hooks))))
}

/// A mod game's sections go into the save it makes, come back to its hooks
/// when the character joins the next game, and replace the loaded ones at
/// the next save. A `.d2s` file has no place for them.
#[test]
fn a_mod_games_sections_are_saved_and_loaded_back() {
    let (hooks, rules) = modded();
    let mut g = Game::start(new_character("amazon", "Keeper").expect("new"), rules);
    // A new character's join hands the hooks no sections.
    assert_eq!(*hooks.loads.lock().unwrap(), [Vec::<ModSection>::new()]);
    *hooks.value.lock().unwrap() = 7;
    let first = g.save();
    let want = [ModSection::new(*b"CNTR", 7u32.to_le_bytes().to_vec())];
    assert_eq!(sections(&first), want);
    let GameData::Live(d) = &data().0;
    assert!(d2s::write(&first, &d.save).is_err());

    // The next game's hooks start empty and get the saved value back.
    let (hooks, rules) = modded();
    let mut g = Game::start(loaded(&first), rules);
    assert_eq!(*hooks.loads.lock().unwrap(), [want.to_vec()]);
    assert_eq!(*hooks.value.lock().unwrap(), 7);
    *hooks.value.lock().unwrap() = 9;
    let second = g.save();
    assert_eq!(
        sections(&second),
        [ModSection::new(*b"CNTR", 9u32.to_le_bytes().to_vec())]
    );
}

/// D2RUST's own rules ask nothing: a new character's save has no sections
/// and is a `.d2s` file, and a loaded save's sections pass through.
#[test]
fn without_save_hooks_a_save_keeps_its_sections() {
    let mut g = Game::start(
        new_character("amazon", "Plain").expect("new"),
        Ruleset::Original,
    );
    assert_eq!(g.live().mod_sections, None);
    let mut first = g.save();
    assert!(sections(&first).is_empty());
    let GameData::Live(d) = &data().0;
    assert!(d2s::write(&first, &d.save).is_ok());
    let kept = vec![ModSection::new(*b"KEEP", vec![1, 2, 3])];
    first.body.as_mut().unwrap().mod_sections = kept.clone();
    let mut g = Game::start(loaded(&first), Ruleset::Original);
    assert_eq!(sections(&g.save()), kept);
}

/// A new character joins with its waypoint records, each knowing only the
/// town's waypoint (bit 0), and saves them before any waypoint is used.
#[test]
fn a_new_character_joins_knowing_the_town_waypoint() {
    let mut g = Game::start(
        new_character("amazon", "Walker").expect("new"),
        Ruleset::Original,
    );
    let town = WaypointRecord::allocate().0;
    assert_eq!(g.live().extra.waypoints, Some([town; 3]));
}

fn entries(save: &D2s, id: u16) -> Vec<StatEntry> {
    save.body
        .as_ref()
        .expect("a body")
        .stats
        .entries()
        .into_iter()
        .filter(|e| e.id == id)
        .collect()
}

/// A stat the table marks saved is saved at every param the player holds,
/// loads back into the next game, and a param the player no longer holds
/// leaves the save.
#[test]
fn every_saved_stat_is_saved_with_its_params() {
    let id = counters();
    let mut g = Game::start(
        new_character("amazon", "Counter").expect("new"),
        Ruleset::Original,
    );
    g.set_stat(id, 0, 5);
    g.set_stat(id, 3, 300);
    g.set_stat(id, 0x2004, -1);
    let first = g.save();
    let e = |layer, value| StatEntry { id, layer, value };
    assert_eq!(entries(&first, id), [e(0, 5), e(3, 300), e(0x2004, -1)]);
    // The 16 base stats are saved as before.
    let level = StatEntry {
        id: 12,
        layer: 0,
        value: 1,
    };
    assert_eq!(entries(&first, 12), [level]);

    let mut g = Game::start(loaded(&first), Ruleset::Original);
    assert_eq!(g.base_stat(id, 3), 300);
    assert_eq!(g.base_stat(id, 0x2004), -1);
    g.set_stat(id, 3, 0);
    g.set_stat(id, 9, 90);
    let second = apply_live(&first, &g.live(), 1);
    assert_eq!(entries(&second, id), [e(0, 5), e(9, 90), e(0x2004, -1)]);
}

/// The JM flags of every record of the player's items.
fn flags(save: &D2s) -> Vec<u32> {
    let items: &[ItemEntry] = &save.body.as_ref().expect("a body").items;
    items
        .iter()
        .map(|i| i.record_flags(0).expect("a JM record"))
        .collect()
}

/// An item made straight into the inventory keeps "in a store" (0x2000)
/// until a load: the first save of a new item has it, and a load clears
/// it on every item ([`d2s::item_flags_on_load`]), so the saves after the
/// reload hold the same records without it and stay equal.
#[test]
fn the_in_store_flag_goes_at_the_first_reload() {
    let mut g = Game::start(
        new_character("amazon", "Flags").expect("new"),
        Ruleset::Original,
    );
    g.give("sb1");
    g.give("sb1");
    let first = g.save();
    assert!(flags(&first).len() >= 2);
    let mut g = Game::start(loaded(&first), Ruleset::Original);
    let second = g.save();
    assert_eq!(
        flags(&second),
        flags(&first)
            .iter()
            .map(|f| f & !d2s::ITEM_FLAG_INSTORE)
            .collect::<Vec<_>>()
    );
    let mut g = Game::start(loaded(&second), Ruleset::Original);
    let third = g.save();
    assert_eq!(flags(&third), flags(&second));
    // Given items had it at their first save.
    assert!(flags(&first)
        .iter()
        .rev()
        .take(2)
        .all(|f| f & d2s::ITEM_FLAG_INSTORE != 0));
}
