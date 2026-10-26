//! Several connections on one database file, as the main server's threads
//! and later the web interface use it: SQLite serialises the moves, and no
//! move is lost or done twice.

use std::sync::{Arc, Barrier};

use super::*;
use crate::tests::{market, offers};

const THREADS: usize = 8;

/// Runs `f(thread)` on `THREADS` threads at once, each with its own
/// connection to `path`.
fn race<T: Send + 'static>(
    path: &std::path::Path,
    f: impl Fn(usize, &mut Economy) -> T + Send + Sync + 'static,
) -> Vec<T> {
    let f = Arc::new(f);
    let start = Arc::new(Barrier::new(THREADS));
    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let (f, start, path) = (f.clone(), start.clone(), path.to_owned());
            std::thread::spawn(move || {
                // Every thread reaches the barrier, even if its open fails.
                let opened = Economy::open(&path, MOD, config());
                start.wait();
                f(t, &mut opened.unwrap())
            })
        })
        .collect();
    handles.into_iter().map(|h| h.join().unwrap()).collect()
}

#[test]
fn one_item_taken_once() {
    let path = temp_db("one_item_taken_once");
    put_item(&mut open(&path), 1, 1, item(1)).unwrap();
    let results = race(&path, |t, e| {
        e.take(
            &owner(1),
            &req(1, 100 + t as u64),
            &chr("Hero", t as u64),
            Goods::Item(ItemId(1)),
            2000,
        )
        .map_err(|e| e.refusal())
    });
    let won = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(won, 1, "{results:?}");
    assert!(results
        .iter()
        .all(|r| matches!(r, Ok(_) | Err(Some(Refusal::ItemHeld)))));
    let e = open(&path);
    let flights: i64 = e
        .conn()
        .query_row("SELECT COUNT(*) FROM in_flight", [], |r| r.get(0))
        .unwrap();
    assert_eq!(flights, 1);
}

#[test]
fn no_lost_gold() {
    let path = temp_db("no_lost_gold");
    put_gold(&mut open(&path), 1, 1, 800).unwrap();
    // Half the threads put 100 gold, half take 100: the balance ends where
    // it began, with every move logged.
    let results = race(&path, |t, e| {
        let n = 100 + t as u64;
        if t % 2 == 0 {
            put_gold(e, 1, n, 100).map(|_| ())
        } else {
            e.take(
                &owner(1),
                &req(1, n),
                &chr("Hero", 9),
                Goods::Gold(100),
                2000,
            )
            .map(|_| ())
        }
    });
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    let mut e = open(&path);
    assert_eq!(e.stash_gold(&owner(1)).unwrap(), 800);
    e.undo_lease(9, 3000).unwrap();
    assert_eq!(e.stash_gold(&owner(1)).unwrap(), 1200);
}

/// Also opens a new file from every thread at once: the schema is made
/// once.
#[test]
fn repeated_req_from_two_threads_moves_once() {
    let path = temp_db("repeated_req_from_two_threads");
    let results = race(&path, |_, e| put_gold(e, 1, 7, 100).unwrap().repeated);
    assert_eq!(results.iter().filter(|r| !**r).count(), 1);
    assert_eq!(open(&path).stash_gold(&owner(1)).unwrap(), 100);
}

#[test]
fn two_buyers_one_wins() {
    let path = temp_db("two_buyers_one_wins");
    {
        let mut e = open(&path);
        put_item(&mut e, 1, 1, item(1)).unwrap();
        market::post_item(&mut e, 1, 2, 1, Price::gold(1000)).unwrap();
        for t in 0..THREADS {
            put_gold(&mut e, 10 + t as i64, 1, 1000).unwrap();
        }
    }
    let hash = Price::gold(1000).hash();
    let results = race(&path, move |t, e| {
        let buyer = 10 + t as i64;
        e.buy(&owner(buyer), &req(buyer, 2), 1, &hash, 2000)
            .map_err(|e| e.refusal())
    });
    let won: Vec<usize> = (0..THREADS).filter(|t| results[*t].is_ok()).collect();
    assert_eq!(won.len(), 1, "{results:?}");
    assert!(results
        .iter()
        .all(|r| matches!(r, Ok(_) | Err(Some(Refusal::ListingGone)))));
    // The winner paid once and has the item; the seller has the gold once;
    // every other buyer kept theirs.
    let e = open(&path);
    let winner = 10 + won[0] as i64;
    let goods = |a| -> Vec<Goods> {
        e.deliveries(&owner(a))
            .unwrap()
            .into_iter()
            .map(|d| d.goods)
            .collect()
    };
    assert_eq!(goods(winner), [Goods::Item(ItemId(1))]);
    assert_eq!(goods(1), [Goods::Gold(1000)]);
    for t in 0..THREADS {
        let buyer = 10 + t as i64;
        let left = if buyer == winner { 0 } else { 1000 };
        assert_eq!(e.stash_gold(&owner(buyer)).unwrap(), left);
    }
    assert_eq!(totals(&e).gold, THREADS as i64 * 1000);
}

/// The seller accepts while the buyer withdraws, from many threads: one
/// answer wins, and the goods end up on one side only.
#[test]
fn accept_or_withdraw_one_wins() {
    let path = temp_db("accept_or_withdraw_one_wins");
    {
        let mut e = open(&path);
        offers::offers_listing(&mut e, ListingMode::OffersOnly);
        let p = offers::bundle(100, vec![], &[5]);
        e.offer(&owner(2), &req(2, 10), 1, "Buyer", &p, 2000)
            .unwrap();
    }
    let before = totals(&open(&path));
    let results = race(&path, |t, e| {
        let (account, how) = if t % 2 == 0 {
            (1, OfferAnswer::Accept)
        } else {
            (2, OfferAnswer::Withdraw)
        };
        e.answer(
            &owner(account),
            &req(account, 100 + t as u64),
            1,
            &how,
            3000,
        )
        .map_err(|e| e.refusal())
        .map(|_| t % 2 == 0)
    });
    let won: Vec<bool> = results.iter().filter_map(|r| (*r).ok()).collect();
    assert_eq!(won.len(), 1, "{results:?}");
    assert!(results
        .iter()
        .all(|r| matches!(r, Ok(_) | Err(Some(Refusal::OfferGone)))));
    let e = open(&path);
    let goods = |a| -> Vec<Goods> {
        let mut g: Vec<Goods> = e
            .deliveries(&owner(a))
            .unwrap()
            .into_iter()
            .map(|d| d.goods)
            .collect();
        g.sort_by_key(|g| format!("{g:?}"));
        g
    };
    if won[0] {
        assert_eq!(goods(1), [Goods::Gold(100), Goods::Item(ItemId(5))]);
        assert_eq!(goods(2), [Goods::Item(ItemId(1))]);
    } else {
        assert!(goods(1).is_empty());
        assert_eq!(goods(2), [Goods::Gold(100), Goods::Item(ItemId(5))]);
    }
    assert_eq!(totals(&e), before);
}
