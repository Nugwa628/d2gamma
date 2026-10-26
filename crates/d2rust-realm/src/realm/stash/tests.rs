//! The online stash's store, hooks and moves on a realm database in a
//! temp folder: the records a move stores, the item IDs it moves, the
//! cells, a lease that ends under a move, and the pairing of a transfer
//! with its record.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use d2_economy::{
    CharRef, Economy, EconomyConfig, EconomyKey, Goods, Item, ItemFacts, ItemId, Owner, Placed,
    PutGoods, Refusal, Req, Save,
};
use d2_item_registry::{Config, Place as IdPlace, Registry};
use d2_net::msg::account::CharSummary;
use d2_net::msg::node::{SaveInfo, SAVE_REASON_TRANSFER};
use d2_net::msg::transfer::{placed, ItemInDone};

use super::cells::Placement;
use super::store::{Owners, RealmItems, Records, Store, StoredRecord};
use super::*;
use crate::realm::guard::GuardConfig;
use crate::realm::saves::{SavePairs, TransferHalf, PAIR_WAIT};
use crate::session::ControlOut;
use d2_shared_stash::{StashShape, STACK_CAP};

const NOW: i64 = 1_700_000_000_000;
const ACCOUNT: i64 = 5;
const LEASE: u64 = 7;
const GAME: u32 = 1;

fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2rust-realm-stash")
        .join(format!("{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn owner() -> Owner {
    Owner::new(ACCOUNT, EconomyKey::new("d2rust"))
}

fn amy() -> CharRef {
    CharRef {
        name: "Amy".into(),
        game: GAME,
        lease: LEASE,
    }
}

fn item(id: u32) -> Item {
    Item {
        id: ItemId(id),
        record: vec![0x4A, 0x4D, id as u8],
        facts: ItemFacts {
            code: "rin".into(),
            item_type: "ring".into(),
            name: "rin".into(),
            name_parts: vec!["rin".into()],
            ..ItemFacts::default()
        },
    }
}

fn save(seq: u32) -> Save {
    Save {
        record: vec![seq as u8; 4],
        seq,
    }
}

/// The realm database with the registry (a block granted to the game),
/// the economy with the stash's hooks, and the stash's store.
fn realm(dir: &Path) -> (Economy, Store, Owners, rusqlite::Connection, Registry, u32) {
    let path = dir.join("realm.sqlite");
    let conn = d2_item_registry::open_file(&path).unwrap();
    let registry = Registry::open(&conn, "d2rust", Config::default()).unwrap();
    let block = d2_item_registry::write(&mut d2_item_registry::open_file(&path).unwrap(), |c| {
        registry.grant(c, GAME, NOW)
    })
    .unwrap();
    let store = Store::open(&path).unwrap();
    let owners = Owners::default();
    let economy = Economy::open(&path, "d2rust", EconomyConfig::default())
        .unwrap()
        .with_records(Box::new(Records {
            owners: owners.clone(),
        }))
        .with_registry(Box::new(RealmItems::new(registry.clone(), None)));
    (economy, store, owners, conn, registry, block.first)
}

fn place(reg: &Registry, conn: &rusqlite::Connection, id: u32) -> (IdPlace, String) {
    let w = reg.place_of(conn, id).unwrap().unwrap();
    (w.place, w.place_id)
}

#[test]
fn a_put_stores_the_record_and_moves_the_id_to_the_stash() {
    let dir = temp("put");
    let (mut e, store, owners, conn, reg, id) = realm(&dir);
    // A lease the stash didn't start a move under: its record can't be
    // stored, so nothing moves.
    let r = e.put(
        &owner(),
        &Req::lease(LEASE, 1),
        &amy(),
        PutGoods::Item(item(id)),
        &save(1),
        NOW,
    );
    assert_eq!(r.unwrap_err().refusal(), Some(Refusal::TransferInvalid));
    assert_eq!(store.record("amy").unwrap(), None);
    assert!(reg.place_of(&conn, id).unwrap().is_none());

    owners.set(LEASE, ACCOUNT);
    e.put(
        &owner(),
        &Req::lease(LEASE, 2),
        &amy(),
        PutGoods::Item(item(id)),
        &save(2),
        NOW,
    )
    .unwrap();
    let r = store.record("amy").unwrap().unwrap();
    assert_eq!(
        r,
        StoredRecord {
            name: "Amy".into(),
            account: ACCOUNT,
            lease: LEASE,
            seq: 2,
            record: save(2).record,
        }
    );
    assert_eq!(
        place(&reg, &conn, id),
        (IdPlace::Stash, format!("account {ACCOUNT}"))
    );
    // The item is on the first free cell of the stash.
    let split = owner().economy.split();
    let layout = store.layout(ACCOUNT, &split, &Grid::default()).unwrap();
    assert_eq!(
        layout.by_item.get(&id).map(|p| p.cell),
        Some(StashCell {
            page: 0,
            x: 0,
            y: 0
        })
    );
    let page = store.page(&layout, 0).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].record, item(id).record);
    assert_eq!(store.amounts(ACCOUNT, &split).unwrap().items, 1);
    assert_eq!(store.search(ACCOUNT, &split, "RI", 10).unwrap(), vec![id]);
    assert_eq!(
        store.search(ACCOUNT, &split, "%", 10).unwrap(),
        Vec::<u32>::new()
    );
    // Written to its file: the row goes.
    store.record_written(&r).unwrap();
    assert_eq!(store.record("amy").unwrap(), None);
}

#[test]
fn a_take_moves_the_id_through_the_flight_to_the_character() {
    let dir = temp("take");
    let (mut e, store, owners, conn, reg, id) = realm(&dir);
    owners.set(LEASE, ACCOUNT);
    e.put(
        &owner(),
        &Req::lease(LEASE, 1),
        &amy(),
        PutGoods::Item(item(id)),
        &save(1),
        NOW,
    )
    .unwrap();
    let split = owner().economy.split();
    let cell = store
        .layout(ACCOUNT, &split, &Grid::default())
        .unwrap()
        .by_item[&id];
    let first = store.record("amy").unwrap().unwrap();

    let t = e
        .take(
            &owner(),
            &Req::lease(LEASE, 2),
            &amy(),
            Goods::Item(ItemId(id)),
            NOW,
        )
        .unwrap()
        .id
        .unwrap();
    assert_eq!(place(&reg, &conn, id), (IdPlace::InFlight, "amy".into()));
    assert_eq!(store.leases_in_flight().unwrap(), vec![LEASE]);
    // In flight, the item keeps its cell for a return.
    assert_eq!(
        store
            .layout(ACCOUNT, &split, &Grid::default())
            .unwrap()
            .by_item[&id],
        cell
    );

    e.take_done(
        &Req::lease(LEASE, 3),
        LEASE,
        t,
        Placed::Placed,
        Some(&save(3)),
        NOW,
    )
    .unwrap();
    assert_eq!(place(&reg, &conn, id), (IdPlace::Character, "amy".into()));
    assert!(store.leases_in_flight().unwrap().is_empty());
    assert!(store
        .layout(ACCOUNT, &split, &Grid::default())
        .unwrap()
        .by_item
        .is_empty());
    // The older record's write doesn't drop the newer one.
    store.record_written(&first).unwrap();
    assert_eq!(store.record("amy").unwrap().unwrap().seq, 3);
}

#[test]
fn an_undone_take_comes_back_to_its_cell() {
    let dir = temp("undo");
    let (mut e, store, owners, conn, reg, id) = realm(&dir);
    owners.set(LEASE, ACCOUNT);
    e.put(
        &owner(),
        &Req::lease(LEASE, 1),
        &amy(),
        PutGoods::Item(item(id)),
        &save(1),
        NOW,
    )
    .unwrap();
    let split = owner().economy.split();
    let at = Placement {
        cell: StashCell {
            page: 3,
            x: 2,
            y: 5,
        },
        size: (1, 1),
    };
    store.set_cell(id, ACCOUNT, &split, &at).unwrap();
    e.take(
        &owner(),
        &Req::lease(LEASE, 2),
        &amy(),
        Goods::Item(ItemId(id)),
        NOW,
    )
    .unwrap();
    assert_eq!(e.undo_lease(LEASE, NOW).unwrap(), 1);
    assert_eq!(
        place(&reg, &conn, id),
        (IdPlace::Stash, format!("account {ACCOUNT}"))
    );
    assert_eq!(
        store
            .layout(ACCOUNT, &split, &Grid::default())
            .unwrap()
            .by_item[&id],
        at
    );
}

fn guard(dir: &Path) -> SaveGuard {
    let log = Arc::new(ServerLog::open(&dir.join("logs")).unwrap());
    SaveGuard::open(
        &dir.join("realm.sqlite"),
        "d2rust",
        None,
        GuardConfig {
            checks: super::super::guard::ChecksMode::Off,
            check_modes: Vec::new(),
            registry: RegistryMode::Off,
        },
        log,
    )
    .unwrap()
}

fn stash(dir: &Path) -> Arc<Stash> {
    let log = Arc::new(ServerLog::open(&dir.join("logs")).unwrap());
    Stash::open(
        &dir.join("realm.sqlite"),
        "d2rust",
        &guard(dir),
        EconomyConfig::default(),
        log,
    )
    .unwrap()
}

#[test]
fn an_account_makes_one_move_at_a_time() {
    let dir = temp("one-move");
    let s = stash(&dir);
    let o = owner();
    let (n, rx) = s
        .begin(LEASE, ACCOUNT, "Amy", &o, What::Put { cell: None })
        .unwrap();
    // A second move of the account, from another character too: held.
    assert!(s
        .begin(LEASE + 1, ACCOUNT, "Bob", &o, What::Put { cell: None })
        .is_none());
    // Another account's goes on.
    let (m, _rx2) = s
        .begin(LEASE + 2, ACCOUNT + 1, "Cid", &o, What::Put { cell: None })
        .unwrap();
    assert_ne!(n, m);
    s.end(LEASE, n, Answer { code: 0, count: 1 });
    assert_eq!(rx.blocking_recv().unwrap(), Answer { code: 0, count: 1 });
    // Ended: the account moves again.
    assert!(s
        .begin(LEASE, ACCOUNT, "Amy", &o, What::Put { cell: None })
        .is_some());
}

#[test]
fn a_repeated_request_gets_its_first_answer() {
    let dir = temp("repeat");
    let s = stash(&dir);
    let (first, mut first_rx) = ControlTx::new(1);
    let (again, mut again_rx) = ControlTx::new(2);
    assert_eq!(s.request(ACCOUNT, 1, &first), Ok(()));
    // Still running: the repeat (a client back on a new session) waits
    // for the first's answer.
    assert_eq!(s.request(ACCOUNT, 1, &again), Err(None));
    // Another account's numbers are its own.
    assert_eq!(s.request(ACCOUNT + 1, 1, &first), Ok(()));
    s.answered(ACCOUNT, 1, Answer { code: 0, count: 1 });
    // The session that asked again hears the answer; the first gets it
    // from its own request.
    match again_rx.try_recv() {
        Ok(ControlOut::Body(b)) => assert_eq!(
            StashEvent::parse(&b).unwrap(),
            StashEvent::Done {
                req: 1,
                code: 0,
                count: 1,
                left: Vec::new(),
            }
        ),
        other => panic!("no answer for the repeat: {other:?}"),
    }
    assert!(first_rx.try_recv().is_err());
    assert_eq!(
        s.request(ACCOUNT, 1, &again),
        Err(Some(Answer { code: 0, count: 1 }))
    );
    // Answered: a repeat now is answered at once, not queued.
    s.answered(ACCOUNT, 1, Answer { code: 0, count: 1 });
    assert!(again_rx.try_recv().is_err());
    // A new number runs, and the old one is forgotten.
    assert_eq!(s.request(ACCOUNT, 2, &first), Ok(()));
    s.answered(ACCOUNT, 1, Answer::refused(code::OTHER));
    assert_eq!(s.request(ACCOUNT, 2, &first), Err(None));
    assert_eq!(s.request(ACCOUNT, 1, &first), Ok(()));
}

#[test]
fn a_lease_that_ends_ends_its_moves_and_undoes_its_transfers() {
    let dir = temp("lease-end");
    let s = stash(&dir);
    let o = owner();
    // An item and gold in the stash.
    s.owners.set(LEASE, ACCOUNT);
    s.bank
        .run_blocking(move |e| {
            e.put(
                &owner(),
                &Req::lease(LEASE, 1),
                &amy(),
                PutGoods::Gold(500),
                &save(1),
                NOW,
            )
        })
        .unwrap()
        .unwrap();
    // A take under way: the gold is in flight.
    let (n, rx) = s
        .begin(
            LEASE,
            ACCOUNT,
            "Amy",
            &o,
            What::Take {
                transfer: 0,
                count: 1,
            },
        )
        .unwrap();
    let t = s
        .bank
        .run_blocking(move |e| {
            e.take(
                &owner(),
                &Req::lease(LEASE, 2),
                &amy(),
                Goods::Gold(200),
                NOW,
            )
        })
        .unwrap()
        .unwrap()
        .id
        .unwrap();
    s.set_transfer(LEASE, n, t);
    let split = o.economy.split();
    assert_eq!(s.store.amounts(ACCOUNT, &split).unwrap().gold, 300);
    // A claimed move is the game server's answer's to end.
    let (m, rx2) = s
        .begin(LEASE + 1, ACCOUNT + 1, "Bob", &o, What::Put { cell: None })
        .unwrap();
    let claimed = s.claim(LEASE + 1, m).unwrap();

    s.lease_ended(Lease(LEASE));
    s.lease_ended(Lease(LEASE + 1));
    assert_eq!(
        rx.blocking_recv().unwrap(),
        Answer::of(ErrorCode::LEASE_REVOKED)
    );
    // The undo runs on the worker after the moves end.
    let gold = s
        .bank
        .run_blocking(move |e| e.stash_gold(&owner()))
        .unwrap()
        .unwrap();
    assert_eq!(gold, 500);
    assert!(s.store.leases_in_flight().unwrap().is_empty());
    s.finish(claimed, Answer { code: 0, count: 1 });
    assert_eq!(rx2.blocking_recv().unwrap(), Answer { code: 0, count: 1 });
}

#[test]
fn the_start_writes_left_records_and_undoes_left_transfers() {
    let dir = temp("recover");
    let s = stash(&dir);
    s.owners.set(LEASE, ACCOUNT);
    s.bank
        .run_blocking(move |e| {
            e.put(
                &owner(),
                &Req::lease(LEASE, 1),
                &amy(),
                PutGoods::Gold(500),
                &save(1),
                NOW,
            )?;
            e.take(
                &owner(),
                &Req::lease(LEASE, 2),
                &amy(),
                Goods::Gold(100),
                NOW,
            )
        })
        .unwrap()
        .unwrap();
    drop(s);
    // A new run: no account has the character, so its record stays and
    // is logged; the transfer goes back.
    let s = stash(&dir);
    let accounts =
        d2_accounts::Accounts::open_with(d2_accounts::AccountsConfig::new(&dir)).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(s.recover(&accounts)).unwrap();
    assert!(s.store.leases_in_flight().unwrap().is_empty());
    assert_eq!(
        s.store
            .amounts(ACCOUNT, &owner().economy.split())
            .unwrap()
            .gold,
        500
    );
    assert!(s.store.record("amy").unwrap().is_some());
    // A later save of the character in its file drops it only when its
    // write failed in this run.
    s.saved("Amy");
    assert!(s.store.record("amy").unwrap().is_some());
}

fn info(lease: u64, seq: u32) -> SaveInfo {
    SaveInfo {
        game: GAME,
        lease,
        seq,
        reason: SAVE_REASON_TRANSFER,
        journal_tick: 0,
        summary: CharSummary {
            name: "Amy".into(),
            class: 0,
            level: 1,
            status: 0x20,
            difficulties: 1,
            look: vec![0; 16],
            last_played: 0,
            season: 0,
        },
    }
}

fn done(lease: u64, req: u64, seq: u32) -> TransferHalf {
    TransferHalf::In(ItemInDone {
        lease,
        req,
        result: placed::PLACED,
        save: Some(info(lease, seq)),
    })
}

#[test]
fn a_transfer_pairs_with_its_record_in_either_order() {
    let mut p = SavePairs::default();
    let t0 = Instant::now();
    // The message first.
    assert_eq!(p.transfer(done(LEASE, 1, 4), t0), None);
    let (h, r) = p.transfer_record(LEASE, 4, vec![1]).unwrap();
    assert_eq!((h, r), (done(LEASE, 1, 4), vec![1]));
    // The record first: a save's until its message comes.
    assert_eq!(p.transfer_record(LEASE, 5, vec![2]), Err(vec![2]));
    assert_eq!(p.record(LEASE, 5, vec![2], t0), None);
    assert_eq!(
        p.transfer(done(LEASE, 2, 5), t0),
        Some((done(LEASE, 2, 5), Some(vec![2])))
    );
    // Not placed: no save to wait for.
    let not = TransferHalf::In(ItemInDone {
        lease: LEASE,
        req: 3,
        result: placed::NO_ROOM,
        save: None,
    });
    assert_eq!(p.transfer(not.clone(), t0), Some((not, None)));
    // A message whose record never comes expires.
    assert_eq!(p.transfer(done(LEASE, 4, 6), t0), None);
    assert!(p.expire_transfers(t0 + Duration::from_secs(1)).is_empty());
    assert_eq!(p.expire_transfers(t0 + PAIR_WAIT), vec![done(LEASE, 4, 6)]);
    assert_eq!(p.waiting(), 0);
}

fn stack_out(stack: Option<(&str, u32)>) -> ItemOut {
    ItemOut {
        save: info(LEASE, 1),
        req: 1,
        to: goods::STACK,
        unit: 9,
        item_id: None,
        stack: stack.map(|(c, n)| (c.to_owned(), n)),
        gold: 0,
        sketch: None,
        item: Vec::new(),
        credits: Vec::new(),
    }
}

#[test]
fn a_stack_put_names_a_listed_kind_and_a_count_in_range() {
    let dir = temp("stack-of");
    let log = Arc::new(ServerLog::open(&dir.join("logs")).unwrap());
    let config = EconomyConfig {
        stackables: d2_shared_stash::Catalog::base(|_| None).stackables(),
        ..EconomyConfig::default()
    };
    let s = Stash::open(
        &dir.join("realm.sqlite"),
        "d2rust",
        &guard(&dir),
        config,
        log,
    )
    .unwrap();
    assert_eq!(
        s.stack_of(&stack_out(Some(("r01", 1)))),
        Ok(("r01".into(), 1, 0))
    );
    assert_eq!(
        s.stack_of(&stack_out(Some(("gcv", STACK_CAP)))),
        Ok(("gcv".into(), STACK_CAP, 0))
    );
    // Not a kind with a slot, no count, too many, nothing named.
    assert!(s.stack_of(&stack_out(Some(("cap", 1)))).is_err());
    assert!(s.stack_of(&stack_out(Some(("r01", 0)))).is_err());
    assert!(s
        .stack_of(&stack_out(Some(("r01", STACK_CAP + 1))))
        .is_err());
    assert!(s.stack_of(&stack_out(None)).is_err());
}

/// A mod realm's stash: 14 x 14 renamable pages, a slot cap past
/// D2Gamma's, and a category page with whole-item slots.
fn mod_stash(dir: &Path) -> Arc<Stash> {
    let log = Arc::new(ServerLog::open(&dir.join("logs")).unwrap());
    let mut settings = StashSettings::default();
    for (k, v) in [
        ("stash_cols", "14"),
        ("stash_rows", "14"),
        ("stash_renamable", "on"),
        ("stash_cap", "9999999"),
        ("stash_category", "Currency;14;140"),
    ] {
        settings.set(k, v).unwrap();
    }
    let config = EconomyConfig {
        stackables: d2_shared_stash::Catalog::base(|_| None).stackables(),
        ..EconomyConfig::default()
    };
    Stash::open_with(
        &dir.join("realm.sqlite"),
        "mymod",
        &guard(dir),
        config,
        settings,
        log,
    )
    .unwrap()
}

fn mod_owner() -> Owner {
    Owner::new(ACCOUNT, EconomyKey::new("mymod"))
}

#[test]
fn d2gammas_realm_keeps_its_shape_and_refuses_renames() {
    let dir = temp("d2gamma-shape");
    let s = stash(&dir);
    let StashEvent::Shape(shape) = s.shape(&owner()).unwrap() else {
        panic!("a shape")
    };
    assert_eq!(
        (
            shape.pages,
            shape.cols,
            shape.rows,
            shape.renamable,
            shape.cap
        ),
        (10, 8, 11, false, STACK_CAP)
    );
    assert_eq!(shape.names[0], "I");
    assert_eq!(shape.names[9], "X");
    assert_eq!(
        s.rename(&owner(), 0, "Gear").unwrap().code,
        Refusal::BadRequest.code()
    );
    // A category page or a page past X lists no items.
    assert!(!s.settings().holds_page(100));
    let page = s.open_page(&owner(), tab::ITEMS, 10).unwrap();
    assert_eq!(page.len(), 1);
}

#[test]
fn a_mod_realm_sends_its_shape_and_keeps_page_names() {
    let dir = temp("mod-shape");
    let s = mod_stash(&dir);
    let o = mod_owner();
    assert_eq!(s.rename(&o, 3, "  Charms ").unwrap().code, 0);
    assert_eq!(s.rename(&o, 4, "Runes").unwrap().code, 0);
    assert_eq!(s.rename(&o, 4, "").unwrap().code, 0, "back to the default");
    for (page, name) in [(10, "Past"), (0, "Thirteen long"), (0, "tab	here")] {
        assert_ne!(s.rename(&o, page, name).unwrap().code, 0, "{page} {name}");
    }
    let StashEvent::Shape(shape) = s.shape(&o).unwrap() else {
        panic!("a shape")
    };
    assert_eq!(
        (
            shape.pages,
            shape.cols,
            shape.rows,
            shape.renamable,
            shape.cap
        ),
        (10, 14, 14, true, 9_999_999)
    );
    assert_eq!(shape.names[3], "Charms");
    assert_eq!(shape.names[4], "V");
    // Another account's names are its own.
    let other = Owner::new(ACCOUNT + 1, EconomyKey::new("mymod"));
    let StashEvent::Shape(theirs) = s.shape(&other).unwrap() else {
        panic!("a shape")
    };
    assert_eq!(theirs.names[3], "IV");
}

#[test]
fn a_mod_realms_cells_slots_and_cap_are_its_own() {
    let dir = temp("mod-cells");
    let s = mod_stash(&dir);
    let o = mod_owner();
    let split = o.economy.split();
    // Cells past D2Gamma's 8 x 11 and the category's slots are pages of
    // this realm; the layout reads on its grid.
    let layout = s.store.layout(ACCOUNT, &split, &s.grid).unwrap();
    let big = Placement {
        cell: StashCell {
            page: 9,
            x: 12,
            y: 12,
        },
        size: (2, 2),
    };
    assert!(layout.is_free(&big, None));
    let slot = Placement {
        cell: StashCell {
            page: 100,
            x: 13,
            y: 9,
        },
        size: (2, 4),
    };
    assert!(layout.is_free(&slot, None), "slot 139 of 140");
    assert!(s.settings().holds_page(100));
    assert_eq!(s.open_page(&o, tab::ITEMS, 100).unwrap().len(), 1);
    // A count past D2Gamma's cap, within the realm's.
    assert_eq!(
        s.stack_of(&stack_out(Some(("r01", 9_999_999)))),
        Ok(("r01".into(), 9_999_999, 0))
    );
    assert!(s.stack_of(&stack_out(Some(("r01", 10_000_000)))).is_err());
    assert_eq!(StashShape::DEFAULT, StashSettings::default().shape);
}
