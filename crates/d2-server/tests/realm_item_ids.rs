//! Realm games on the synthetic playable install (plan C3): a game started
//! with the main server's seed and item ID blocks ([`GameBuilder::realm`])
//! writes realm IDs into its saves, keeps them across games, never hands
//! one out twice, and runs exactly the simulation of the same game with the
//! default source.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use d2_formats::d2s::{D2s, ItemEntry};
use d2_proto::dev::{DevCommand, Quality, Target};
use d2_server::adapters::character::LoadContext;
use d2_server::adapters::ProtoSizes;
use d2_server::dev::{DevCatalog, DevServer};
use d2_server::game::character_save::{apply_live, base_save, read_live};
use d2_server::game::realm::{IdBlock, RealmStart};
use d2_server::game::{create_request_for, new_character, Character, GameBuilder, GameData, Sim};
use d2_server::host::Host;
use d2_server::seams::{ClientId, Clock, MessageSink, SessionHandler};
use d2_sim::items::bitstream::read::read_save_entry;
use d2_sim::items::bitstream::StreamItem;
use d2_sim::items::ItemTables;

/// The realm's seed for the games (any fixed value).
const SEED: u32 = 0x5EED_0C3D;

fn data() -> GameData {
    static DATA: OnceLock<GameData> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("realm-item-ids-{}", std::process::id()));
        test_fixtures::install::build(&dir, &test_fixtures::play::playable())
            .unwrap_or_else(|e| panic!("{e}"));
        GameData::select(Some(&dir)).expect("the playable install loads")
    })
    .clone()
}

fn item_tables() -> &'static ItemTables {
    static T: OnceLock<ItemTables> = OnceLock::new();
    T.get_or_init(|| {
        let GameData::Live(d) = data();
        d.tables.item_tables().expect("item tables")
    })
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

/// One game with one player (client 0), driven frame by frame.
struct Game {
    host: Host<Sim, ProtoSizes, NoSession, Steps>,
    character: Character,
    dev: DevServer,
    /// The realm's side: the next block's first ID and its size; `None`:
    /// requests go unanswered.
    realm: Option<(u32, u32)>,
    /// Every block granted, the first included.
    granted: Vec<IdBlock>,
}

impl Game {
    fn start(character: Character, realm: Option<(IdBlock, u32)>) -> Self {
        let mut b = GameBuilder::new(data())
            .seed(SEED)
            .character(character.clone());
        let mut granted = Vec::new();
        if let Some((first, _)) = realm {
            b = b
                .realm(RealmStart {
                    seed: SEED,
                    item_ids: first,
                })
                .expect("a valid first block");
            granted.push(first);
        }
        let sim = b.build().expect("the game builds");
        let mut host = Host::new(sim, ProtoSizes, NoSession, Steps(1000));
        host.connect(0);
        let GameData::Live(d) = data();
        let dev = DevServer::new(DevCatalog::from_live(&d, |_| String::new()));
        let mut g = Game {
            host,
            character,
            dev,
            realm: realm.map(|(f, size)| (f.first + f.count, size)),
            granted,
        };
        // The client's join: C→S 0x67, then 0x6B once the server's 0x02
        // arrived.
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
                g.frames(4);
                return g;
            }
        }
        panic!("the player did not join");
    }

    /// One host frame, then the realm answers the game's block requests;
    /// the messages client 0 received.
    fn frame(&mut self) -> Vec<Vec<u8>> {
        self.host.clock.0 += 40;
        self.host.frame().expect("frame");
        let got = self.host.receive(0);
        let wanted = self.host.game.take_item_ids_wanted();
        if let Some((next, size)) = &mut self.realm {
            for _ in 0..wanted {
                let block = IdBlock {
                    first: *next,
                    count: *size,
                };
                *next += *size;
                self.host.game.grant_item_ids(block).expect("granted");
                self.granted.push(block);
            }
        }
        got
    }

    /// `n` host frames.
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame();
        }
    }

    /// A normal `code` into the player's inventory; false when it did not
    /// fit there.
    fn give(&mut self, code: &str) -> bool {
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
        r.text.contains("inventory")
    }

    fn save(&mut self) -> D2s {
        let p = self.host.game.player_of(0).expect("player");
        let live = read_live(&mut self.host.game, p);
        apply_live(&base_save(&self.character), &live, 0)
    }

    fn drawn(&mut self) -> u64 {
        self.host.game.item_ids().blocks().map_or(0, |b| b.drawn())
    }

    fn state(&self) -> String {
        let s = &self.host.game;
        d2_sim::debug::state::snapshot_world(&s.game, &s.events).to_json_line()
    }
}

fn new_amazon(name: &str) -> Character {
    new_character("amazon", name).expect("a valid new character")
}

fn loaded(save: D2s) -> Character {
    Character::Save(
        Box::new(save),
        LoadContext {
            difficulty: 0,
            map_seed_applies: false,
        },
    )
}

/// Every item record of the save's lists (children included), in order.
fn records(save: &D2s) -> Vec<StreamItem> {
    let body = save.body.as_ref().expect("a body");
    let mut lists: Vec<&[ItemEntry]> = vec![&body.items];
    for c in &body.corpses {
        lists.push(&c.items);
    }
    if let Some(Some(h)) = &body.hireling_items {
        lists.push(h);
    }
    let golem = body
        .golem
        .iter()
        .filter_map(|g| g.item.clone())
        .collect::<Vec<_>>();
    lists.push(&golem);
    let mut out = Vec::new();
    for e in lists.into_iter().flatten() {
        let r = read_save_entry(&e.bytes, item_tables()).expect("the entry reads");
        out.push(r.item.item);
        out.extend(r.children.into_iter().map(|c| c.item.item));
    }
    out
}

/// The ID fields of the save's extended records.
fn ids(save: &D2s) -> Vec<u32> {
    records(save)
        .into_iter()
        .filter(|r| !r.compact && !r.alt)
        .map(|r| r.unit28)
        .collect()
}

fn in_blocks(id: u32, blocks: &[IdBlock]) -> bool {
    blocks
        .iter()
        .any(|b| id >= b.first && u64::from(id) < u64::from(b.first) + u64::from(b.count))
}

fn sorted(mut v: Vec<u32>) -> Vec<u32> {
    v.sort_unstable();
    v
}

const FIRST: IdBlock = IdBlock {
    first: 100_000,
    count: 4096,
};

#[test]
fn a_realm_save_carries_distinct_ids_from_its_blocks() {
    let mut g = Game::start(new_amazon("IdsOne"), Some((FIRST, 4096)));
    for code in ["sb1", "sb1", "ar1"] {
        assert!(g.give(code), "{code} fits");
    }
    g.frames(30);
    let s1 = g.save();
    let got = ids(&s1);
    assert!(got.len() >= 3, "the given items are extended: {got:?}");
    assert!(got.iter().all(|&id| in_blocks(id, &[FIRST])), "{got:?}");
    assert_eq!(got.iter().collect::<BTreeSet<_>>().len(), got.len());
    assert_eq!(g.drawn(), got.len() as u64, "one draw per saved item");
    // A later save writes the same IDs and draws none.
    g.frames(30);
    assert_eq!(sorted(ids(&g.save())), sorted(got));
    assert_eq!(g.drawn(), ids(&s1).len() as u64);
}

#[test]
fn a_reloaded_character_keeps_its_ids_and_draws_none() {
    let mut g = Game::start(new_amazon("IdsTwo"), Some((FIRST, 4096)));
    assert!(g.give("sb1"));
    g.frames(10);
    let first = g.save();
    let before = sorted(ids(&first));
    assert!(!before.is_empty());
    let second = IdBlock {
        first: 500_000,
        count: 4096,
    };
    let mut g = Game::start(loaded(first), Some((second, 4096)));
    g.frames(10);
    assert_eq!(sorted(ids(&g.save())), before, "kept");
    assert_eq!(g.drawn(), 0, "a loaded item draws none");
    assert!(g.give("sb1"));
    let after = ids(&g.save());
    let new: Vec<u32> = after
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert_eq!(
        new,
        [second.first],
        "the new item's ID is from this game's block"
    );
}

/// Five games one after another on the same character, each loading the
/// last one's save, with small blocks so the games keep running low: every
/// item's ID stays the same from save to save, and no ID is ever handed to
/// two items.
#[test]
fn a_long_run_of_games_never_reuses_an_id() {
    const BLOCK: u32 = 3;
    let mut character = new_amazon("IdsLong");
    let mut next = 1_000;
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut last: Vec<u32> = Vec::new();
    let mut grants = 0;
    let mut given = 0;
    for game in 0..5 {
        let first = IdBlock {
            first: next,
            count: BLOCK,
        };
        let mut g = Game::start(character.clone(), Some((first, BLOCK)));
        for round in 0..4 {
            // A blade a round while it fits (one that does not lies on
            // the ground and is not saved).
            if g.give("sb1") {
                given += 1;
            }
            g.frames(150);
            let save = g.save();
            let now = ids(&save);
            assert_eq!(
                now.iter().collect::<BTreeSet<_>>().len(),
                now.len(),
                "game {game} round {round}: distinct IDs {now:?}"
            );
            for &id in &now {
                assert_ne!(id, 0, "game {game} round {round}: every item has an ID");
                if !last.contains(&id) {
                    assert!(seen.insert(id), "ID {id} handed out twice");
                    assert!(in_blocks(id, &g.granted), "ID {id} from this game's blocks");
                }
            }
            for id in &last {
                assert!(now.contains(id), "game {game} round {round}: {id} kept");
            }
            last = now;
            character = loaded(save);
        }
        grants += g.granted.len();
        next = g.realm.unwrap().0;
    }
    assert!(given >= 5, "{given} blades given");
    assert!(seen.len() >= given, "{seen:?}");
    assert!(
        grants > 5,
        "the games asked for more blocks: {grants} grants"
    );
}

/// The same game with the default source and with realm IDs, on the same
/// inputs: the state is the same every frame, and the saves differ only in
/// the ID fields (the default writes the init seeds, as before).
#[test]
fn realm_ids_leave_the_simulation_unchanged() {
    let mut a = Game::start(new_amazon("IdsSame"), None);
    let mut b = Game::start(new_amazon("IdsSame"), Some((FIRST, 4096)));
    assert_eq!(a.state(), b.state());
    for code in ["sb1", "ar1"] {
        assert_eq!(a.give(code), b.give(code));
    }
    let mut saves: BTreeMap<usize, (D2s, D2s)> = BTreeMap::new();
    for f in 0..300 {
        a.frames(1);
        b.frames(1);
        assert_eq!(a.state(), b.state(), "frame {f}");
        if f % 100 == 99 {
            saves.insert(f, (a.save(), b.save()));
        }
    }
    for (f, (sa, sb)) in saves {
        let (ra, rb) = (records(&sa), records(&sb));
        assert_eq!(ra.len(), rb.len());
        for (mut x, mut y) in ra.into_iter().zip(rb) {
            if !x.compact && !x.alt {
                assert!(in_blocks(y.unit28, &[FIRST]), "frame {f}");
            }
            (x.unit28, y.unit28) = (0, 0);
            assert_eq!(x, y, "frame {f}");
        }
        assert_eq!(sa.header, sb.header, "frame {f}");
    }
    assert_eq!(a.drawn(), 0);
}
