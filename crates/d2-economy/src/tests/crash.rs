//! Crash injection: every move is run in a child process that aborts at
//! write point 0, 1, 2, ... (no unwinding, no rollback: the process just
//! dies, as on a power cut of the program). After each crash the parent
//! reopens the database and finds it exactly as before the move; once the
//! move gets through every point, it finds the whole move.

use std::path::Path;
use std::process::{Command, Stdio};

use super::*;
use crate::tests::{market, offers};

/// The child's orders: `op|point|path`.
const ENV: &str = "D2_ECONOMY_CRASH";

/// The state every move starts from: account 1 with item 1, 1,000 gold and
/// 5 El in its stash, and item 2 in flight to a character under lease 50
/// (transfer 1). On the market: account 2's item 3 listed for 100 gold
/// and 1 El (listing 1); account 2's item 4 sold to account 3 for 300 gold
/// (listing 2), so account 2 has the gold as delivery 1 and account 3 the
/// item as delivery 2. Offers: account 2's items 5 and 7 listed for offers
/// (listings 3 and 4); account 3 offers 50 gold and item 6 on listing 3
/// (offer 1, open) and 50 gold and item 8 on listing 4 (offer 2, which
/// account 2 countered with 80 gold). Account 2's item 9 was listed and
/// cancelled (listing 5): it waits as delivery 3.
fn setup(e: &mut Economy) {
    put_item(e, 1, 1, item(1)).unwrap();
    put_item(e, 1, 2, item(2)).unwrap();
    put_gold(e, 1, 3, 1000).unwrap();
    put_stack(e, 1, 4, "r01", 5).unwrap();
    let t = e
        .take(
            &owner(1),
            &req(1, 5),
            &chr("Hero", 50),
            Goods::Item(ItemId(2)),
            1000,
        )
        .unwrap();
    assert_eq!(t.id, Some(1));
    put_item(e, 2, 1, item(3)).unwrap();
    put_item(e, 2, 2, item(4)).unwrap();
    let l = market::post_item(e, 2, 3, 3, listed_price()).unwrap();
    assert_eq!(l.id, Some(1));
    let l = market::post_item(e, 2, 4, 4, Price::gold(300)).unwrap();
    assert_eq!(l.id, Some(2));
    put_gold(e, 3, 1, 300).unwrap();
    e.buy(&owner(3), &req(3, 2), 2, &Price::gold(300).hash(), 1000)
        .unwrap();
    assert_eq!((ids(e, 2), ids(e, 3)), (vec![1], vec![2]));
    for (n, id, listing) in [(5, 5, 3), (6, 7, 4)] {
        put_item(e, 2, n, item(id)).unwrap();
        let l = e
            .post(
                &owner(2),
                &req(2, n + 10),
                &Post {
                    seller: "Seller".into(),
                    goods: Goods::Item(ItemId(id)),
                    mode: ListingMode::OffersOnly,
                    price: None,
                    days: 7,
                },
                1000,
            )
            .unwrap();
        assert_eq!(l.id, Some(listing));
    }
    put_gold(e, 3, 3, 1000).unwrap();
    for (n, listing, id) in [(4, 3, 6), (5, 4, 8)] {
        put_item(e, 3, n, item(id)).unwrap();
        let p = offers::bundle(50, vec![], &[id]);
        e.offer(&owner(3), &req(3, n + 10), listing, "Hero", &p, 1000)
            .unwrap();
    }
    let counter = OfferAnswer::Counter(Price::gold(80));
    e.answer(&owner(2), &req(2, 20), 2, &counter, 1000).unwrap();
    put_item(e, 2, 7, item(9)).unwrap();
    let l = market::post_item(e, 2, 8, 9, Price::gold(40)).unwrap();
    assert_eq!(l.id, Some(5));
    e.cancel(&owner(2), &req(2, 9), 5, 1000).unwrap();
    assert_eq!(ids(e, 2), [1, 3]);
}

fn ids(e: &Economy, account: i64) -> Vec<i64> {
    let mut ids: Vec<i64> = e
        .deliveries(&owner(account))
        .unwrap()
        .iter()
        .map(|d| d.id)
        .collect();
    ids.sort_unstable();
    ids
}

/// A many-row answer as a plain one.
fn rows(r: Result<RowsDone, Error>) -> Result<Done, Error> {
    r.map(|r| Done {
        id: None,
        repeated: r.repeated,
    })
}

fn listed_price() -> Price {
    market::price(100, vec![("r01".into(), 1)])
}

/// The moves under test.
fn run(op: &str, e: &mut Economy) -> Result<Done, Error> {
    let hero = chr("Hero", 60);
    match op {
        "put_item" => put_item(e, 1, 10, item(10)),
        "put_gold" => put_gold(e, 1, 10, 250),
        "put_stack" => put_stack(e, 1, 10, "r30", 2),
        "take_item" => e.take(&owner(1), &req(1, 10), &hero, Goods::Item(ItemId(1)), 2000),
        "take_gold" => e.take(&owner(1), &req(1, 10), &hero, Goods::Gold(400), 2000),
        "take_stack" => e.take(
            &owner(1),
            &req(1, 10),
            &hero,
            Goods::Stack {
                code: "r01".into(),
                count: 5,
            },
            2000,
        ),
        "take_done" => e.take_done(
            &Req::lease(50, 1),
            50,
            1,
            Placed::Placed,
            Some(&save(9)),
            2000,
        ),
        "take_no_room" => e.take_done(&Req::lease(50, 1), 50, 1, Placed::NoRoom, None, 2000),
        "undo_lease" => e.undo_lease(50, 2000).map(|_| Done {
            id: None,
            repeated: false,
        }),
        "post" => market::post_item(e, 1, 10, 1, Price::gold(5)),
        "buy" => e.buy(&owner(1), &req(1, 10), 1, &listed_price().hash(), 2000),
        "cancel" => e.cancel(&owner(2), &req(2, 10), 1, 2000),
        "expire" => e.expire(1000 + 7 * market::DAY).map(|_| Done {
            id: None,
            repeated: false,
        }),
        "claim" => e.claim(&owner(2), &req(2, 10), 1, 2000),
        "take_delivery" => e.take_delivery(&owner(3), &req(3, 10), &hero, 2, 2000),
        "offer" => e.offer(
            &owner(1),
            &req(1, 10),
            3,
            "Hero",
            &offers::bundle(10, vec![("r01".into(), 1)], &[1]),
            2000,
        ),
        "accept" => e.answer(&owner(2), &req(2, 30), 1, &OfferAnswer::Accept, 2000),
        "accept_counter" => e.answer(&owner(3), &req(3, 30), 2, &OfferAnswer::Accept, 2000),
        "counter" => e.answer(
            &owner(2),
            &req(2, 30),
            1,
            &OfferAnswer::Counter(Price::gold(60)),
            2000,
        ),
        "decline" => e.answer(&owner(2), &req(2, 30), 2, &OfferAnswer::Decline, 2000),
        "withdraw" => e.answer(&owner(3), &req(3, 30), 1, &OfferAnswer::Withdraw, 2000),
        "hide" => e.hide(&owner(2), &req(2, 10), 1, 2000),
        "reprice" => e.reprice(
            &owner(2),
            &req(2, 10),
            1,
            ListingMode::PriceAndOffers,
            Some(&Price::gold(90)),
            2000,
        ),
        "relist" => e.relist(&owner(2), &req(2, 10), 1, 3, 2000),
        "relist_cancelled" => e.relist(&owner(2), &req(2, 10), 5, 7, 2000),
        "shop_edit" => {
            let fee = e.config().fee_rev();
            rows(e.shop_edit(
                &owner(2),
                &req(2, 10),
                &ShopOp::Cancel,
                &[1, 3, 2, 99],
                fee,
                2000,
            ))
        }
        "claim_many" => rows(e.claim_many(&owner(2), &req(2, 10), &Claim::All, 2000)),
        other => panic!("unknown op {other}"),
    }
}

/// Runs in the child only (with `D2_ECONOMY_CRASH` set); a no-op in a
/// normal test run.
#[test]
fn crash_child() {
    let Ok(orders) = std::env::var(ENV) else {
        return;
    };
    let mut parts = orders.splitn(3, '|');
    let (op, point, path) = (
        parts.next().unwrap(),
        parts.next().unwrap().parse::<u32>().unwrap(),
        Path::new(parts.next().unwrap()),
    );
    let mut e = open(path);
    e.faults().abort.set(true);
    e.faults().crash_at.set(Some(point));
    run(op, &mut e).unwrap();
    std::fs::write(path.with_extension("done"), op).unwrap();
}

/// Crashes `op` at each of its write points in turn; returns the totals
/// before and after the finished move.
fn crash_everywhere(op: &str) -> (Totals, Totals) {
    let exe = std::env::current_exe().unwrap();
    for point in 0..200 {
        let path = temp_db(&format!("crash-{op}-{point}"));
        setup(&mut open(&path));
        let (before, before_totals) = {
            let e = open(&path);
            (snapshot(&e), totals(&e))
        };
        let status = Command::new(&exe)
            .args(["--exact", "tests::crash::crash_child", "--nocapture"])
            .args(["--test-threads", "1"])
            .env(ENV, format!("{op}|{point}|{}", path.display()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        let e = open(&path);
        let finished = path.with_extension("done").exists();
        assert_eq!(
            status.success(),
            finished,
            "{op} at point {point}: the child ran nothing ({status})"
        );
        if finished {
            assert!(point > 0, "{op} wrote nothing");
            assert_ne!(snapshot(&e), before, "{op} changed nothing");
            println!("{op}: {point} crash points, each left the database as before");
            return (before_totals, totals(&e));
        }
        assert_eq!(
            snapshot(&e),
            before,
            "{op}: a crash at point {point} left part of the move"
        );
    }
    panic!("{op} never finished");
}

#[test]
fn crash_during_put() {
    let (before, after) = crash_everywhere("put_item");
    assert_eq!(after.items, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    assert_eq!(before.items, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let (before, after) = crash_everywhere("put_gold");
    assert_eq!(after.gold, before.gold + 250);
    let (before, after) = crash_everywhere("put_stack");
    assert_eq!(after.stacks["r30"], 2);
    assert_eq!(before.stacks.get("r30"), None);
}

#[test]
fn crash_during_market() {
    // Market moves never make or lose goods (no fees here).
    for op in [
        "post",
        "buy",
        "cancel",
        "expire",
        "claim",
        "take_delivery",
        "offer",
        "accept",
        "accept_counter",
        "counter",
        "decline",
        "withdraw",
        "hide",
        "reprice",
        "relist",
        "relist_cancelled",
        "shop_edit",
        "claim_many",
    ] {
        let (before, after) = crash_everywhere(op);
        assert_eq!(before, after, "{op}");
    }
}

#[test]
fn crash_during_take() {
    // Taking moves goods inside the economy: totals never change.
    for op in [
        "take_item",
        "take_gold",
        "take_stack",
        "take_no_room",
        "undo_lease",
    ] {
        let (before, after) = crash_everywhere(op);
        assert_eq!(before, after, "{op}");
    }
    // A finished take leaves the economy with the item.
    let (before, after) = crash_everywhere("take_done");
    assert_eq!(before.items, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(after.items, [1, 3, 4, 5, 6, 7, 8, 9]);
}
