//! Single player's shared stash on the real game (D2RUST's own): the
//! store's moves lift and place the character's real items and rewrite its
//! file; a stopped move loses and duplicates nothing; a reload of both
//! sides gives back the same goods; the softcore / hardcore and mod stores
//! are apart.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use d2_client::app::save::{self, FileStore};
use d2_client::app::server_thread::ThreadLink;
use d2_client::app::shared_stash::{self as stash, SharedStash};
use d2_client::app::single_player::{self, Character, DEFAULT_SEED};
use d2_client::bridge::link::{SendQueue, ServerLink};
use d2_client::ui::shared_stash::StashAsk;
use d2_server::stash_items;
use d2_shared_stash::local::Crash;
use d2_shared_stash::{Catalog, EconomyKey};

use crate::app_support;

struct StepClock(Arc<AtomicU32>);

impl d2_server::seams::Clock for StepClock {
    fn now_ms(&mut self) -> u32 {
        self.0.load(Ordering::SeqCst)
    }
}

type Server = Arc<Mutex<ThreadLink<single_player::Link<StepClock>>>>;

/// One running game of the character saved at `file`, with its store.
struct Run {
    server: Server,
    save: save::SaveHandle,
    stash: SharedStash,
}

/// Runs the join of `character` on the user's game.
fn joined(character: &Character) -> ThreadLink<single_player::Link<StepClock>> {
    let ms = Arc::new(AtomicU32::new(1000));
    let (mut link, _) = single_player::start_with(
        app_support::game_data(),
        DEFAULT_SEED,
        character.clone(),
        StepClock(ms.clone()),
    )
    .unwrap();
    link.send(
        SendQueue::System,
        &single_player::create_request_for(character).encode(),
    )
    .unwrap();
    link.pump().unwrap();
    ms.fetch_add(40, Ordering::SeqCst);
    link.pump().unwrap();
    link.receive();
    link.send(SendQueue::System, &[0x6B]).unwrap();
    ms.fetch_add(40, Ordering::SeqCst);
    assert!(link.pump().unwrap().ticked);
    link.receive();
    link
}

fn key(mod_id: &str, hardcore: bool) -> EconomyKey {
    EconomyKey {
        hardcore,
        ..EconomyKey::new(mod_id)
    }
}

/// Starts the game of `character` saved at `file`, after the folder's
/// recovery (as `play_start`), with the store of `key`.
fn start(character: &Character, file: &Path, key: EconomyKey) -> Run {
    let faults = stash::recover_folder(file.parent().unwrap(), stash::D2RUST);
    assert!(faults.is_empty(), "{faults:?}");
    let live = app_support::live();
    let base = save::base_save(character);
    let tables: Arc<dyn d2_formats::d2s::SaveTables + Send + Sync> = Arc::new(live.save.clone());
    let recipe = FileStore {
        path: file.to_path_buf(),
        base: base.clone(),
        tables: tables.clone(),
        appearance: None,
        codec: Arc::new(single_player::D2sCodec),
    };
    let (shared, save) = save::share(joined(character), base, tables, None, file.into()).unwrap();
    let server = shared.server();
    let catalog = Catalog::base(|c| Some(c.to_owned()));
    let stash = SharedStash::open(key, catalog, recipe, server.clone()).unwrap();
    Run {
        server,
        save,
        stash,
    }
}

/// Starts the game again from the character's file (the store's
/// recovery first: the file is read after it).
fn restart(file: &Path, key: EconomyKey) -> Run {
    let faults = stash::recover_folder(file.parent().unwrap(), stash::D2RUST);
    assert!(faults.is_empty(), "{faults:?}");
    let c = single_player::load_character(&app_support::game_data(), file, 0).unwrap();
    start(&c, file, key)
}

impl Run {
    fn on_game<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut single_player::Sim, d2_sim::units::UnitId) -> R + Send + 'static,
    ) -> R {
        let mut link = self.server.lock().unwrap();
        link.with(move |l| {
            let sim = &mut l.host_mut().game;
            let (p, _) = single_player::local_player(sim).unwrap();
            f(sim, p)
        })
        .unwrap()
    }

    /// The inventory's items by code: their count (a stack its quantity).
    fn inventory(&self) -> BTreeMap<String, u32> {
        let items = self.on_game(stash_items::inventory_items);
        let mut out = BTreeMap::new();
        for h in items {
            *out.entry(h.code).or_default() += h.plain_count.unwrap_or(1);
        }
        out
    }

    fn give(&self, code: &'static str, count: u32) {
        self.on_game(move |s, p| stash_items::give_plain(s, p, code, count))
            .unwrap();
    }

    fn guids(&self) -> Vec<u32> {
        self.on_game(stash_items::inventory_items)
            .into_iter()
            .map(|h| h.guid)
            .collect()
    }

    fn stacks(&self) -> BTreeMap<String, u32> {
        self.stash.view(None, "").unwrap().stacks
    }

    fn ask(&self, a: StashAsk) -> Option<String> {
        self.stash.run(&a, BTreeMap::new()).unwrap()
    }
}

fn temp(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("app-shared-stash-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn count(m: &BTreeMap<String, u32>, code: &str) -> u32 {
    m.get(code).copied().unwrap_or(0)
}

/// A new character with three El runes, two chipped amethysts, a key
/// stack of 5 and an identify scroll (no slot), saved at `file`.
fn stocked(file: &Path, key: EconomyKey) -> Run {
    let c = single_player::new_character("barbarian", "Stasher").unwrap();
    let run = start(&c, file, key);
    run.give("r01", 3);
    run.give("gcv", 2);
    run.give("key", 5);
    run.give("isc", 1);
    run.save.save().unwrap();
    run
}

/// "Send stackables to stash" moves every stackable with a slot from the
/// inventory and leaves the rest; taking gives single items back; the
/// character file and the store agree after a reload.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn send_take_and_reload_keep_every_item() {
    let dir = temp("send");
    let file = dir.join("Stasher.d2s");
    let run = stocked(&file, key(stash::D2RUST, false));
    let before = run.inventory();
    assert_eq!(count(&before, "r01"), 3);
    assert_eq!(count(&before, "key"), 5);
    let isc = count(&before, "isc");
    assert!(isc >= 1);

    assert_eq!(
        run.ask(StashAsk::SendStackables { guids: run.guids() }),
        None
    );
    let inv = run.inventory();
    assert_eq!(
        count(&inv, "r01") + count(&inv, "gcv") + count(&inv, "key"),
        0
    );
    // A kind with no slot is never moved.
    assert_eq!(count(&inv, "isc"), isc);
    let st = run.stacks();
    assert_eq!(
        (count(&st, "r01"), count(&st, "gcv"), count(&st, "key")),
        (3, 2, 5)
    );

    // Take 2 runes (single items) and the keys (one stack).
    assert_eq!(
        run.ask(StashAsk::TakeStack {
            code: "r01".into(),
            count: 2
        }),
        None
    );
    run.ask(StashAsk::TakeStack {
        code: "key".into(),
        count: u32::MAX,
    });
    let inv = run.inventory();
    assert_eq!((count(&inv, "r01"), count(&inv, "key")), (2, 5));
    let st = run.stacks();
    assert_eq!((count(&st, "r01"), count(&st, "key")), (1, 0));
    drop(run);

    // Reload: the file has what the inventory had, the store the rest.
    let run = restart(&file, key(stash::D2RUST, false));
    let inv = run.inventory();
    let st = run.stacks();
    assert_eq!((count(&inv, "r01"), count(&st, "r01")), (2, 1));
    assert_eq!((count(&inv, "gcv"), count(&st, "gcv")), (0, 2));
    assert_eq!((count(&inv, "key"), count(&st, "key")), (5, 0));
    assert_eq!(count(&inv, "isc"), isc);
}

/// A full inventory: a take gives what fits and says so; nothing is lost.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_take_into_a_full_inventory_gives_what_fits() {
    let dir = temp("full");
    let file = dir.join("Stasher.d2s");
    let run = stocked(&file, key(stash::D2RUST, false));
    run.ask(StashAsk::SendStackables { guids: run.guids() });
    // Fill the inventory but one cell with scrolls.
    let room = run.on_game(|s, p| stash_items::plain_room(s, p, "isc"));
    assert!(room > 1);
    let fill = (room - 1) as u32;
    run.on_game(move |s, p| stash_items::give_plain(s, p, "isc", fill))
        .unwrap();
    let line = run.ask(StashAsk::TakeStack {
        code: "r01".into(),
        count: 3,
    });
    assert_eq!(line.as_deref(), Some("Only 1 fit in the inventory"));
    let (inv, st) = (run.inventory(), run.stacks());
    assert_eq!((count(&inv, "r01"), count(&st, "r01")), (1, 2));
    // No room at all: refused, nothing moved.
    let e = run
        .stash
        .run(
            &StashAsk::TakeStack {
                code: "gcv".into(),
                count: 1,
            },
            BTreeMap::new(),
        )
        .unwrap_err();
    assert_eq!(stash::refusal_line(&e), "Not enough room");
    assert_eq!(count(&run.stacks(), "gcv"), 2);
}

/// A game stopped at each step of a send and of a take: after the
/// restart (recovery, then the character file) every rune is either on
/// the character or in the store, once.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_stopped_move_loses_and_duplicates_nothing() {
    let puts = [
        Crash::Lifted,
        Crash::InTransaction,
        Crash::Stored(1),
        Crash::Written,
    ];
    let takes = [
        Crash::InFlight,
        Crash::Placed,
        Crash::InTransaction,
        Crash::Stored(1),
        Crash::Written,
    ];
    let k = || key(stash::D2RUST, false);
    for (i, (at, put)) in puts
        .iter()
        .map(|c| (*c, true))
        .chain(takes.iter().map(|c| (*c, false)))
        .enumerate()
    {
        let dir = temp(&format!("crash{i}"));
        let file = dir.join("Stasher.d2s");
        let run = stocked(&file, k());
        if !put {
            run.ask(StashAsk::SendStackables { guids: run.guids() });
        }
        run.stash.with_store(|s| s.set_crash(Some(at)));
        let a = if put {
            StashAsk::SendStackables { guids: run.guids() }
        } else {
            StashAsk::TakeStack {
                code: "r01".into(),
                count: 2,
            }
        };
        // The game thread stops with the move.
        assert!(run.stash.run(&a, BTreeMap::new()).is_err(), "{at:?}");
        drop(run);
        let run = restart(&file, k());
        let (inv, st) = (run.inventory(), run.stacks());
        for (code, n) in [("r01", 3), ("gcv", 2), ("key", 5)] {
            assert_eq!(
                count(&inv, code) + count(&st, code),
                n,
                "{code} after a stop at {at:?} ({} put): inventory {inv:?}, stash {st:?}",
                if put { "a" } else { "no" }
            );
        }
    }
}

/// The softcore and hardcore stores are apart; a mod's store (in the
/// mod's own save folder) never opens on another mod's file.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn hardcore_softcore_and_mods_are_apart() {
    let dir = temp("apart");
    let file = dir.join("Stasher.d2s");
    let run = stocked(&file, key(stash::D2RUST, false));
    run.ask(StashAsk::SendStackables { guids: run.guids() });
    assert_eq!(count(&run.stacks(), "r01"), 3);
    drop(run);
    let run = restart(&file, key(stash::D2RUST, true));
    assert!(run.stacks().is_empty());
    drop(run);
    // Another mod's store on this folder's softcore file is refused.
    let c = single_player::load_character(&app_support::game_data(), &file, 0).unwrap();
    let recipe = FileStore {
        path: file.clone(),
        base: save::base_save(&c),
        tables: Arc::new(app_support::live().save.clone()),
        appearance: None,
        codec: Arc::new(single_player::D2sCodec),
    };
    let server = Arc::new(Mutex::new(joined(&c)));
    let other = SharedStash::open(
        key("othermod", false),
        Catalog::base(|c| Some(c.to_owned())),
        recipe,
        server,
    );
    assert!(other.is_err());
    // A mod in its own folder starts empty.
    let mod_dir = temp("apart-mod");
    let mod_file = mod_dir.join("Stasher.d2s");
    std::fs::copy(&file, &mod_file).unwrap();
    let run = restart(&mod_file, key("othermod", false));
    assert!(run.stacks().is_empty());
    drop(run);
    let run = restart(&file, key(stash::D2RUST, false));
    assert_eq!(count(&run.stacks(), "r01"), 3);
}

/// A send that reaches a slot's cap moves what fits; the rest stays in
/// the inventory with a short line, and no key is lost.
#[test]
#[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
fn a_send_past_the_cap_leaves_the_rest() {
    let dir = temp("cap");
    let file = dir.join("Stasher.d2s");
    let run = stocked(&file, key(stash::D2RUST, false));
    let mut given = 5;
    loop {
        let room = run.on_game(|s, p| stash_items::plain_room(s, p, "key")) as u32;
        let free = room - count(&run.inventory(), "key") % 12;
        run.on_game(move |s, p| stash_items::give_plain(s, p, "key", free))
            .unwrap();
        given += free;
        let line = run.ask(StashAsk::SendStackables { guids: run.guids() });
        let (inv, st) = (run.inventory(), run.stacks());
        assert_eq!(count(&inv, "key") + count(&st, "key"), given);
        if let Some(line) = line {
            assert!(line.ends_with("stayed: the stash slot is full"), "{line}");
            assert!(count(&st, "key") <= d2_shared_stash::STACK_CAP);
            assert!(count(&inv, "key") > 0);
            break;
        }
        assert_eq!(count(&inv, "key"), 0);
    }
}

/// Writes a character with a filled shared stash to `D2_STASH_SEED_DIR`
/// (the offscreen shots of every page start from it); skipped without it.
#[test]
#[ignore = "real data: needs D2_GAME_DIR and D2_STASH_SEED_DIR (the shots' save)"]
fn seed_a_stash_for_the_shots() {
    let Some(dir) = std::env::var_os("D2_STASH_SEED_DIR").map(PathBuf::from) else {
        return;
    };
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("Stasher.d2s");
    let c = single_player::new_character("barbarian", "Stasher").unwrap();
    let run = start(&c, &file, key(stash::D2RUST, false));
    // The stackable pages: every rune but a few, gems, potions, keys and
    // the misc kinds, sent in batches that fit the inventory.
    let mut kinds: Vec<(String, u32)> = (1..=33)
        .filter(|i| i % 7 != 0)
        .map(|i| (format!("r{i:02}"), (40 - i) as u32))
        .collect();
    for (code, n) in [
        ("gcv", 6),
        ("gfv", 3),
        ("gsv", 1),
        ("gcy", 4),
        ("gly", 2),
        ("gcb", 7),
        ("gsb", 2),
        ("gpb", 1),
        ("gcg", 3),
        ("gcr", 5),
        ("gfr", 2),
        ("gcw", 2),
        ("skc", 8),
        ("skl", 1),
        ("hp1", 12),
        ("hp4", 9),
        ("hp5", 3),
        ("mp2", 6),
        ("mp5", 2),
        ("rvs", 11),
        ("rvl", 4),
        ("yps", 5),
        ("key", 30),
        ("tes", 2),
        ("ceh", 1),
        ("pk1", 3),
    ] {
        kinds.push((code.to_owned(), n));
    }
    for (code, mut n) in kinds {
        while n > 0 {
            let (code, k) = (code.clone(), n.min(20));
            run.on_game(move |s, p| stash_items::give_plain(s, p, &code, k))
                .unwrap();
            run.ask(StashAsk::SendStackables { guids: run.guids() });
            n -= k;
        }
    }
    // Items on the item pages I and II.
    for (page, codes) in [
        (
            0u8,
            &[
                "lsd", "cap", "buc", "rin", "amu", "jew", "hax", "lbt", "sbw", "lgl",
            ][..],
        ),
        (1, &["bsd", "skp", "lrg", "vgl", "mbt", "sst"][..]),
    ] {
        for code in codes {
            let code = (*code).to_owned();
            let g = run
                .on_game(move |s, p| stash_items::give_plain(s, p, &code, 1))
                .unwrap()[0];
            run.ask(StashAsk::PutItem {
                guid: g,
                page,
                at: None,
            });
        }
    }
    // Shared gold, and stackables left in the inventory for the send
    // button.
    run.on_game(|s, p| stash_items::set_gold(s, p, 9_000));
    run.ask(StashAsk::Gold { put: true });
    for (code, n) in [("r08", 2), ("gcv", 1), ("hp2", 3), ("key", 4)] {
        run.on_game(move |s, p| stash_items::give_plain(s, p, code, n))
            .unwrap();
    }
    run.save.save().unwrap();
    let st = run.stacks();
    assert!(st.len() > 40, "{st:?}");
    assert_eq!(run.stash.view(Some(0), "").unwrap().items.len(), 10);
}
