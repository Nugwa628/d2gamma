//! The shared stash: puts, takes, counts, limits, repeats, pages.

use std::sync::{Arc, Mutex};

use super::*;
use crate::rusqlite::Connection;

fn refused(r: Result<Done, Error>) -> Refusal {
    r.unwrap_err().refusal().expect("a refusal")
}

#[test]
fn put_is_one_transaction() {
    let mut e = econ();
    put_item(&mut e, 1, 1, item(100)).unwrap();
    put_gold(&mut e, 1, 2, 5000).unwrap();
    put_stack(&mut e, 1, 3, "r30", 2).unwrap();
    let page = e.stash_items(&owner(1), &StashQuery::default()).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].item, item(100));
    assert_eq!(page.gold, 5000);
    assert_eq!(e.stash_stacks(&owner(1)).unwrap(), vec![("r30".into(), 2)]);
    // Each put wrote its log row with the move.
    let kinds: i64 = e
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM transfers WHERE kind = 'put'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kinds, 3);
    // A refused put (a duplicate ID) leaves nothing but its answer.
    let before = snapshot_except(&e, "requests");
    assert_eq!(
        refused(put_item(&mut e, 1, 4, item(100))),
        Refusal::TransferInvalid
    );
    assert_eq!(snapshot_except(&e, "requests"), before);
}

#[test]
fn repeated_req_answers_same() {
    let mut e = econ();
    let first = put_gold(&mut e, 1, 9, 100).unwrap();
    assert!(!first.repeated);
    let again = put_gold(&mut e, 1, 9, 100).unwrap();
    assert!(again.repeated);
    assert_eq!(e.stash_gold(&owner(1)).unwrap(), 100, "gold moved once");
    // A refusal repeats too, even once it would succeed.
    put_item(&mut e, 1, 10, item(5)).unwrap();
    let t = e
        .take(
            &owner(1),
            &req(1, 11),
            &chr("Hero", 1),
            Goods::Item(ItemId(6)),
            1000,
        )
        .unwrap_err();
    assert_eq!(t.refusal(), Some(Refusal::ItemNotFound));
    put_item(&mut e, 1, 12, item(6)).unwrap();
    let t = e
        .take(
            &owner(1),
            &req(1, 11),
            &chr("Hero", 1),
            Goods::Item(ItemId(6)),
            1000,
        )
        .unwrap_err();
    assert_eq!(
        t.refusal(),
        Some(Refusal::ItemNotFound),
        "the stored answer"
    );
    // The same number from another scope is another request.
    assert!(!put_gold(&mut e, 2, 9, 100).unwrap().repeated);
    // Answers are kept 24 hours, then forgotten.
    assert_eq!(e.prune_requests(1000 + REQUEST_KEEP_MS).unwrap(), 0);
    assert!(e.prune_requests(1001 + REQUEST_KEEP_MS).unwrap() > 0);
    assert!(!put_gold(&mut e, 1, 9, 100).unwrap().repeated);
}

use crate::config::REQUEST_KEEP_MS;

#[test]
fn take_then_done_moves_item_to_character() {
    let mut e = econ();
    put_item(&mut e, 1, 1, item(42)).unwrap();
    let hero = chr("Hero", 77);
    let t = e
        .take(&owner(1), &req(1, 2), &hero, Goods::Item(ItemId(42)), 2000)
        .unwrap()
        .id
        .unwrap();
    let flight = e.in_flight(t).unwrap().unwrap();
    assert_eq!(flight.character, hero);
    assert_eq!(flight.item.as_ref().unwrap(), &item(42));
    // In flight: not in the stash, can't be taken again.
    assert_eq!(
        e.stash_items(&owner(1), &StashQuery::default())
            .unwrap()
            .total,
        0
    );
    let again = e
        .take(&owner(1), &req(1, 3), &hero, Goods::Item(ItemId(42)), 2000)
        .unwrap_err();
    assert_eq!(again.refusal(), Some(Refusal::ItemHeld));
    // Only the lease it went to can close it.
    let other = e
        .take_done(
            &Req::lease(78, 1),
            78,
            t,
            Placed::Placed,
            Some(&save(2)),
            2100,
        )
        .unwrap_err();
    assert_eq!(other.refusal(), Some(Refusal::TransferUnknown));
    e.take_done(
        &Req::lease(77, 1),
        77,
        t,
        Placed::Placed,
        Some(&save(2)),
        2100,
    )
    .unwrap();
    assert!(e.in_flight(t).unwrap().is_none());
    assert!(totals(&e).items.is_empty(), "the item left the economy");
    let history: Vec<String> = e
        .item_history(ItemId(42))
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect();
    assert_eq!(history, ["put", "take", "take_done"]);
}

#[test]
fn no_room_returns_to_stash() {
    let mut e = econ();
    put_item(&mut e, 1, 1, item(42)).unwrap();
    put_gold(&mut e, 1, 2, 900).unwrap();
    put_stack(&mut e, 1, 3, "r01", 5).unwrap();
    let before = totals(&e);
    let hero = chr("Hero", 5);
    for (n, goods) in [
        Goods::Item(ItemId(42)),
        Goods::Gold(400),
        Goods::Stack {
            code: "r01".into(),
            count: 3,
        },
    ]
    .into_iter()
    .enumerate()
    {
        let t = e
            .take(&owner(1), &req(1, 10 + n as u64), &hero, goods, 2000)
            .unwrap()
            .id
            .unwrap();
        assert_eq!(totals(&e), before, "in flight is still in the economy");
        e.take_done(&Req::lease(5, n as u64), 5, t, Placed::NoRoom, None, 2100)
            .unwrap();
    }
    assert_eq!(totals(&e), before);
    assert_eq!(e.stash_gold(&owner(1)).unwrap(), 900);
    assert_eq!(e.stash_stacks(&owner(1)).unwrap(), vec![("r01".into(), 5)]);
    assert_eq!(
        e.stash_items(&owner(1), &StashQuery::default())
            .unwrap()
            .total,
        1
    );
}

#[test]
fn take_undone_when_game_lost() {
    let mut e = econ();
    put_item(&mut e, 1, 1, item(1)).unwrap();
    put_item(&mut e, 1, 2, item(2)).unwrap();
    put_gold(&mut e, 1, 3, 1000).unwrap();
    let before = totals(&e);
    let a = chr("Hero", 10);
    let b = chr("Other", 11);
    e.take(&owner(1), &req(1, 4), &a, Goods::Item(ItemId(1)), 2000)
        .unwrap();
    e.take(&owner(1), &req(1, 5), &a, Goods::Gold(300), 2000)
        .unwrap();
    let kept = e
        .take(&owner(1), &req(1, 6), &b, Goods::Item(ItemId(2)), 2000)
        .unwrap()
        .id
        .unwrap();
    // Lease 10 ends (its game closed): both of its transfers come back.
    assert_eq!(e.undo_lease(10, 3000).unwrap(), 2);
    assert_eq!(e.stash_gold(&owner(1)).unwrap(), 1000);
    assert_eq!(
        e.stash_items(&owner(1), &StashQuery::default())
            .unwrap()
            .total,
        1
    );
    assert!(
        e.in_flight(kept).unwrap().is_some(),
        "other leases untouched"
    );
    assert_eq!(totals(&e), before);
    // A late answer for an undone transfer is unknown.
    let late = e
        .take_done(
            &Req::lease(10, 1),
            10,
            1,
            Placed::Placed,
            Some(&save(3)),
            3100,
        )
        .unwrap_err();
    assert_eq!(late.refusal(), Some(Refusal::TransferUnknown));
}

#[test]
fn seed_adds_gold_and_counts_only() {
    let mut e = Economy::in_memory(
        MOD,
        EconomyConfig {
            stash_gold_max: 1000,
            ..config()
        },
    )
    .unwrap();
    e.seed(&owner(1), &req(1, 1), &Goods::Gold(600), 10)
        .unwrap();
    let stack = Goods::Stack {
        code: "r30".into(),
        count: 3,
    };
    e.seed(&owner(1), &req(1, 2), &stack, 10).unwrap();
    assert!(e.seed(&owner(1), &req(1, 2), &stack, 10).unwrap().repeated);
    assert_eq!(e.stash_gold(&owner(1)).unwrap(), 600);
    assert_eq!(e.stash_stacks(&owner(1)).unwrap(), vec![("r30".into(), 3)]);
    // The stash's limits hold; items and unknown stackables are refused.
    assert_eq!(
        refused(e.seed(&owner(1), &req(1, 3), &Goods::Gold(500), 10)),
        Refusal::StashFull
    );
    assert_eq!(
        refused(e.seed(&owner(1), &req(1, 4), &Goods::Item(ItemId(1)), 10)),
        Refusal::BadRequest
    );
    let odd = Goods::Stack {
        code: "zzz".into(),
        count: 1,
    };
    assert!(e.seed(&owner(1), &req(1, 5), &odd, 10).is_err());
    let seeds: i64 = e
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM transfers WHERE kind = 'seed' AND from_kind = 'seed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(seeds, 2);
}

#[test]
fn stash_limits() {
    let mut e = Economy::in_memory(
        MOD,
        EconomyConfig {
            stash_items: 2,
            stash_gold_max: 1000,
            ..config()
        },
    )
    .unwrap();
    put_item(&mut e, 1, 1, item(1)).unwrap();
    put_item(&mut e, 1, 2, item(2)).unwrap();
    assert_eq!(refused(put_item(&mut e, 1, 3, item(3))), Refusal::StashFull);
    // Another account has its own stash.
    put_item(&mut e, 2, 1, item(3)).unwrap();
    put_gold(&mut e, 1, 4, 1000).unwrap();
    assert_eq!(refused(put_gold(&mut e, 1, 5, 1)), Refusal::StashFull);
    // Counts are unlimited up to u32.
    put_stack(&mut e, 1, 6, "r01", u32::MAX - 1).unwrap();
    put_stack(&mut e, 1, 7, "r01", 1).unwrap();
    assert_eq!(
        refused(put_stack(&mut e, 1, 8, "r01", 1)),
        Refusal::StashFull
    );
    // A take undone after the stash filled up again still comes back.
    let t = e
        .take(
            &owner(1),
            &req(1, 9),
            &chr("Hero", 3),
            Goods::Gold(600),
            2000,
        )
        .unwrap()
        .id
        .unwrap();
    put_gold(&mut e, 1, 10, 600).unwrap();
    e.undo_lease(3, 2100).unwrap();
    assert_eq!(e.stash_gold(&owner(1)).unwrap(), 1600);
    let _ = t;
}

#[test]
fn counts_are_counts() {
    let mut e = econ();
    put_stack(&mut e, 1, 1, "r30", 2).unwrap();
    put_stack(&mut e, 1, 2, "r30", 3).unwrap();
    put_stack(&mut e, 1, 3, "r01", 1).unwrap();
    assert_eq!(
        e.stash_stacks(&owner(1)).unwrap(),
        vec![("r01".into(), 1), ("r30".into(), 5)]
    );
    // Only the realm's stackables, and never zero.
    assert_eq!(
        refused(put_stack(&mut e, 1, 4, "xyz", 1)),
        Refusal::BadRequest
    );
    assert_eq!(
        refused(put_stack(&mut e, 1, 5, "r30", 0)),
        Refusal::BadRequest
    );
    let take = |e: &mut Economy, n, count| {
        e.take(
            &owner(1),
            &req(1, n),
            &chr("Hero", 1),
            Goods::Stack {
                code: "r30".into(),
                count,
            },
            2000,
        )
    };
    assert_eq!(refused(take(&mut e, 6, 6)), Refusal::ItemNotFound);
    let t = take(&mut e, 7, 5).unwrap().id.unwrap();
    assert_eq!(e.stash_stacks(&owner(1)).unwrap(), vec![("r01".into(), 1)]);
    let flight = e.in_flight(t).unwrap().unwrap();
    assert_eq!(
        flight.goods,
        Goods::Stack {
            code: "r30".into(),
            count: 5
        }
    );
    e.take_done(
        &Req::lease(1, 1),
        1,
        t,
        Placed::Placed,
        Some(&save(2)),
        2100,
    )
    .unwrap();
    assert_eq!(totals(&e).stacks.get("r30"), None);
}

#[test]
fn economies_never_mix() {
    let mut e = econ();
    let hc = Owner::new(
        1,
        EconomyKey {
            hardcore: true,
            ..EconomyKey::new(MOD)
        },
    );
    put_item(&mut e, 1, 1, item(1)).unwrap();
    put_gold(&mut e, 1, 2, 50).unwrap();
    // The hardcore stash of the same account is another stash.
    assert_eq!(e.stash_gold(&hc).unwrap(), 0);
    assert_eq!(e.stash_items(&hc, &StashQuery::default()).unwrap().total, 0);
    let t = e.take(&hc, &req(1, 3), &chr("Hc", 1), Goods::Item(ItemId(1)), 9);
    assert_eq!(refused(t), Refusal::EconomyMismatch);
    let t = e.take(&hc, &req(1, 4), &chr("Hc", 1), Goods::Gold(50), 9);
    assert_eq!(refused(t), Refusal::ItemNotFound);
    // Another mod is refused outright.
    let pd2 = Owner::new(1, EconomyKey::new("pd2"));
    let t = e.put(
        &pd2,
        &req(1, 5),
        &chr("Hero", 1),
        PutGoods::Gold(1),
        &save(1),
        9,
    );
    assert_eq!(refused(t), Refusal::EconomyMismatch);
    assert_eq!(
        e.stash_gold(&pd2).unwrap_err().refusal(),
        Some(Refusal::EconomyMismatch)
    );
}

#[test]
fn one_database_per_mod() {
    let path = temp_db("one_database_per_mod");
    drop(open(&path));
    let other = Economy::open(&path, "pd2", config()).unwrap_err();
    assert!(matches!(other, Error::Db(_)), "{other:?}");
    let data = std::path::Path::new("data");
    assert_eq!(
        realm_db_path(data, "pd2").unwrap(),
        data.join("realms").join("pd2").join("realm.sqlite")
    );
    assert!(realm_db_path(data, "../x").is_err());
    assert!(realm_db_path(data, "").is_err());
    assert!(realm_db_path(data, "PD2").is_err());
}

#[test]
fn reopen_keeps_everything() {
    let path = temp_db("reopen_keeps_everything");
    {
        let mut e = open(&path);
        put_item(&mut e, 1, 1, item(9)).unwrap();
        put_gold(&mut e, 1, 2, 77).unwrap();
    }
    let mut e = open(&path);
    assert_eq!(e.stash_gold(&owner(1)).unwrap(), 77);
    assert!(put_gold(&mut e, 1, 2, 77).unwrap().repeated);
    assert_eq!(e.stash_gold(&owner(1)).unwrap(), 77);
}

/// A registry that remembers every call, and refuses one ID.
#[derive(Clone, Default)]
struct Recorder {
    calls: Arc<Mutex<Vec<String>>>,
    refuse: Option<u32>,
}

impl ItemRegistry for Recorder {
    fn arriving(
        &self,
        _: &Connection,
        _: &EconomyKey,
        id: ItemId,
        from: &CharRef,
    ) -> Result<(), Refusal> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("arriving {} from {}", id.0, from.name));
        if self.refuse == Some(id.0) {
            return Err(Refusal::TransferInvalid);
        }
        Ok(())
    }

    fn moved(&self, _: &Connection, m: &Moved<'_>) -> Result<(), Refusal> {
        self.calls.lock().unwrap().push(format!(
            "{} {} {} {} {}",
            m.id.0,
            m.place.as_str(),
            m.character.unwrap_or("-"),
            m.game.map_or("-".into(), |g| g.to_string()),
            m.why
        ));
        Ok(())
    }
}

#[test]
fn registry_sees_every_move() {
    let rec = Recorder {
        refuse: Some(13),
        ..Recorder::default()
    };
    let mut e = econ().with_registry(Box::new(rec.clone()));
    put_item(&mut e, 1, 1, item(12)).unwrap();
    assert_eq!(
        refused(put_item(&mut e, 1, 2, item(13))),
        Refusal::TransferInvalid
    );
    let t = e
        .take(
            &owner(1),
            &req(1, 3),
            &chr("Hero", 4),
            Goods::Item(ItemId(12)),
            9,
        )
        .unwrap()
        .id
        .unwrap();
    e.take_done(&Req::lease(4, 1), 4, t, Placed::Placed, Some(&save(2)), 10)
        .unwrap();
    assert_eq!(
        *rec.calls.lock().unwrap(),
        [
            "arriving 12 from Hero",
            "12 stash - - put",
            "arriving 13 from Hero",
            "12 in_flight Hero 7 take",
            "12 character Hero 7 take_done",
        ]
    );
    assert!(totals(&e).items.is_empty());
}

/// Records stored by the moves, in their transaction; a save numbered 99
/// is refused.
#[derive(Clone, Default)]
struct Records(Arc<Mutex<Vec<(String, u32)>>>);

impl CharacterRecords for Records {
    fn store(
        &self,
        conn: &Connection,
        c: &CharRef,
        record: &[u8],
        seq: u32,
    ) -> Result<(), Refusal> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS test_records(name TEXT, seq INTEGER, record BLOB)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO test_records VALUES (?1, ?2, ?3)",
            rusqlite::params![c.name, seq, record],
        )
        .unwrap();
        self.0.lock().unwrap().push((c.name.clone(), seq));
        if seq == 99 {
            return Err(Refusal::TransferInvalid);
        }
        Ok(())
    }
}

#[test]
fn record_is_stored_with_the_move() {
    let records = Records::default();
    let mut e = econ().with_records(Box::new(records.clone()));
    put_item(&mut e, 1, 1, item(1)).unwrap();
    let stored = |e: &Economy| -> i64 {
        e.conn()
            .query_row("SELECT COUNT(*) FROM test_records", [], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(stored(&e), 1);
    // A record the realm refuses undoes the put: no item, no record row.
    let r = e.put(
        &owner(1),
        &req(1, 2),
        &chr("Hero", 1),
        PutGoods::Item(item(2)),
        &save(99),
        1000,
    );
    assert_eq!(refused(r), Refusal::TransferInvalid);
    assert_eq!(stored(&e), 1);
    assert_eq!(totals(&e).items, [1]);
    // A finished take stores the record holding the item.
    let t = e
        .take(
            &owner(1),
            &req(1, 3),
            &chr("Hero", 2),
            Goods::Item(ItemId(1)),
            9,
        )
        .unwrap()
        .id
        .unwrap();
    e.take_done(&Req::lease(2, 1), 2, t, Placed::Placed, Some(&save(5)), 10)
        .unwrap();
    assert_eq!(stored(&e), 2);
    assert_eq!(
        records.0.lock().unwrap().last(),
        Some(&("Hero".to_string(), 5))
    );
    // Placed needs the save that holds it.
    put_item(&mut e, 1, 4, item(3)).unwrap();
    let t = e
        .take(
            &owner(1),
            &req(1, 5),
            &chr("Hero", 2),
            Goods::Item(ItemId(3)),
            9,
        )
        .unwrap()
        .id
        .unwrap();
    let r = e.take_done(&Req::lease(2, 2), 2, t, Placed::Placed, None, 10);
    assert_eq!(refused(r), Refusal::BadRequest);
}

fn ids(page: &ItemsPage) -> Vec<u32> {
    page.items.iter().map(|i| i.item.id.0).collect()
}

#[test]
fn pages_sort_and_filter() {
    let mut e = econ();
    let mut a = item(1);
    a.facts.name = "Zephyr".into();
    a.facts.quality = 7;
    a.facts.stats = vec![(39, 30)];
    let mut b = item(2);
    b.facts.name = "apple 100%".into();
    b.facts.item_type = "amul".into();
    b.facts.ethereal = true;
    b.facts.stats = vec![(39, 5), (127, 2)];
    let mut c = item(3);
    c.facts.name = "Middle".into();
    c.facts.level_req = 60;
    c.facts.sockets = 4;
    c.facts.class = Some(2);
    c.facts.stats = vec![];
    for (n, it) in [a, b, c].into_iter().enumerate() {
        e.put(
            &owner(1),
            &req(1, n as u64),
            &chr("Hero", 1),
            PutGoods::Item(it),
            &save(1),
            1000 + n as i64,
        )
        .unwrap();
    }
    let page = |e: &Economy, sort, desc, filter| {
        e.stash_items(
            &owner(1),
            &StashQuery {
                sort,
                desc,
                filter,
                page: 0,
            },
        )
        .unwrap()
    };
    let none = ItemFilter::default;
    assert_eq!(ids(&page(&e, StashSort::Date, true, none())), [3, 2, 1]);
    assert_eq!(ids(&page(&e, StashSort::Name, false, none())), [2, 3, 1]);
    assert_eq!(ids(&page(&e, StashSort::Quality, true, none())), [1, 3, 2]);
    assert_eq!(ids(&page(&e, StashSort::Level, true, none())), [3, 2, 1]);
    assert_eq!(ids(&page(&e, StashSort::Type, false, none())), [2, 3, 1]);
    // By a stat: items without it last, either way.
    assert_eq!(ids(&page(&e, StashSort::Stat(39), true, none())), [1, 2, 3]);
    assert_eq!(
        ids(&page(&e, StashSort::Stat(39), false, none())),
        [2, 1, 3]
    );
    let f = |f: ItemFilter| ids(&page(&e, StashSort::Date, false, f));
    assert_eq!(
        f(ItemFilter {
            text: Some("PPLE".into()),
            ..none()
        }),
        [2]
    );
    // LIKE's wildcards are plain text.
    assert_eq!(
        f(ItemFilter {
            text: Some("0%".into()),
            ..none()
        }),
        [2]
    );
    assert_eq!(
        f(ItemFilter {
            text: Some("_".into()),
            ..none()
        }),
        Vec::<u32>::new()
    );
    // Any of the item's names.
    assert_eq!(
        f(ItemFilter {
            text: Some("havoc".into()),
            ..none()
        }),
        [1, 2, 3]
    );
    assert_eq!(
        f(ItemFilter {
            item_type: Some("ring".into()),
            ..none()
        }),
        [1, 3]
    );
    assert_eq!(
        f(ItemFilter {
            quality: Some(7),
            ..none()
        }),
        [1]
    );
    assert_eq!(
        f(ItemFilter {
            level: Some((50, 99)),
            ..none()
        }),
        [3]
    );
    assert_eq!(
        f(ItemFilter {
            ethereal: Some(true),
            ..none()
        }),
        [2]
    );
    assert_eq!(
        f(ItemFilter {
            sockets: Some((3, 6)),
            ..none()
        }),
        [3]
    );
    assert_eq!(
        f(ItemFilter {
            class: Some(2),
            ..none()
        }),
        [3]
    );
    let stats = |stats| f(ItemFilter { stats, ..none() });
    assert_eq!(
        stats(vec![StatRange {
            stat: 39,
            min: 5,
            max: 30
        }]),
        [1, 2]
    );
    assert_eq!(
        stats(vec![
            StatRange {
                stat: 39,
                min: 5,
                max: 30
            },
            StatRange {
                stat: 127,
                min: 1,
                max: 3
            }
        ]),
        [2]
    );
    assert_eq!(
        stats(vec![StatRange {
            stat: 39,
            min: 6,
            max: 29
        }]),
        Vec::<u32>::new()
    );
}

#[test]
fn stash_2000_pages_fast() {
    let mut e = econ();
    for id in 0..STASH_ITEMS {
        put_item(&mut e, 1, u64::from(id), item(id + 1)).unwrap();
    }
    assert_eq!(
        refused(put_item(&mut e, 1, 99_999, item(99_999))),
        Refusal::StashFull
    );
    let mut seen = std::collections::BTreeSet::new();
    let mut slowest = std::time::Duration::ZERO;
    for sort in [
        StashSort::Type,
        StashSort::Quality,
        StashSort::Name,
        StashSort::Level,
        StashSort::Date,
        StashSort::Stat(39),
    ] {
        for page in [0u16, 20, 39] {
            let q = StashQuery {
                sort,
                desc: page == 20,
                filter: ItemFilter::default(),
                page,
            };
            let t = std::time::Instant::now();
            let p = e.stash_items(&owner(1), &q).unwrap();
            slowest = slowest.max(t.elapsed());
            assert_eq!(p.total, STASH_ITEMS);
            assert_eq!(p.items.len(), PAGE as usize);
        }
    }
    // Pages of one sort cover every item once.
    for page in 0..(STASH_ITEMS / PAGE) as u16 {
        let q = StashQuery {
            sort: StashSort::Level,
            page,
            ..StashQuery::default()
        };
        for id in ids(&e.stash_items(&owner(1), &q).unwrap()) {
            assert!(seen.insert(id), "item {id} on two pages");
        }
    }
    assert_eq!(seen.len(), STASH_ITEMS as usize);
    println!("stash of {STASH_ITEMS}: slowest page {slowest:?}");
    if !cfg!(debug_assertions) {
        assert!(slowest.as_millis() < 50, "slowest page {slowest:?}");
    }
}

use crate::config::STASH_ITEMS;

fn credit(e: &mut Economy, n: u64, credits: &[(&str, u32)], cap: u32) -> Result<Done, Error> {
    let credits: Vec<(String, u32)> = credits.iter().map(|(c, k)| ((*c).into(), *k)).collect();
    e.put_stacks(
        &owner(1),
        &req(1, n),
        &chr("Hero", 1),
        &credits,
        cap,
        &save(n as u32),
        1000,
    )
}

fn count(e: &Economy, code: &str) -> u32 {
    e.stash_stacks(&owner(1))
        .unwrap()
        .into_iter()
        .find(|(c, _)| c == code)
        .map_or(0, |(_, n)| n)
}

/// A mod game's container folded into what it holds: every code is
/// credited in one move, a code twice counts once with the sum, and a
/// repeat of the request changes nothing.
#[test]
fn a_multi_code_credit_lands_whole() {
    let mut e = econ();
    credit(&mut e, 1, &[("r01", 40), ("r30", 1), ("r01", 2)], 9_999_999).unwrap();
    assert_eq!((count(&e, "r01"), count(&e, "r30")), (42, 1));
    credit(&mut e, 1, &[("r01", 40), ("r30", 1), ("r01", 2)], 9_999_999).unwrap();
    assert_eq!(count(&e, "r01"), 42, "a repeat is answered, not done again");
    // Past D2Gamma's 9,999 a mod's cap holds.
    credit(&mut e, 2, &[("r01", 9_999_000)], 9_999_999).unwrap();
    assert_eq!(count(&e, "r01"), 9_999_042);
}

/// One slot past the cap, or one code that is no stackable, refuses the
/// whole credit: nothing of it lands, and no record is stored.
#[test]
fn a_multi_code_credit_is_all_or_none() {
    let mut e = econ();
    put_stack(&mut e, 1, 1, "r30", 9).unwrap();
    let before = snapshot_except(&e, "requests");
    assert_eq!(
        refused(credit(&mut e, 2, &[("r01", 5), ("r30", 2)], 10)),
        Refusal::StashFull
    );
    assert_eq!(
        refused(credit(&mut e, 3, &[("r01", 5), ("zzz", 1)], 100)),
        Refusal::BadRequest
    );
    assert_eq!(
        refused(credit(&mut e, 4, &[("r01", 0)], 100)),
        Refusal::BadRequest
    );
    assert_eq!(refused(credit(&mut e, 5, &[], 100)), Refusal::BadRequest);
    assert_eq!(snapshot_except(&e, "requests"), before);
    assert_eq!((count(&e, "r01"), count(&e, "r30")), (0, 9));
    credit(&mut e, 6, &[("r01", 5), ("r30", 1)], 10).unwrap();
    assert_eq!((count(&e, "r01"), count(&e, "r30")), (5, 10));
}
