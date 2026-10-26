//! My shop and the rest of the market window (market contract 3.3 to 8):
//! relists, edits on many listings with an answer per row, claims of many
//! deliveries, the sales history, notices, fee revisions and migration 5.

use super::facets::{shelf_config, thing};
use super::market::{ber, refused, DAY};
use super::offers;
use super::*;

const NOW: i64 = 10_000;
const HOUR: i64 = 3600 * 1000;

fn econ_with(c: EconomyConfig) -> Economy {
    Economy::in_memory(MOD, c).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn post(
    e: &mut Economy,
    account: i64,
    n: u64,
    goods: Goods,
    mode: ListingMode,
    price: Option<Price>,
    days: u8,
    now: i64,
) -> i64 {
    e.post(
        &owner(account),
        &req(account, n),
        &Post {
            seller: if account == 1 { "Seller" } else { "Other" }.into(),
            goods,
            mode,
            price,
            days,
        },
        now,
    )
    .unwrap()
    .id
    .unwrap()
}

fn sell(e: &mut Economy, account: i64, n: u64, id: u32, p: Price, now: i64) -> i64 {
    post(
        e,
        account,
        n,
        Goods::Item(ItemId(id)),
        ListingMode::Price,
        Some(p),
        7,
        now,
    )
}

fn at(e: &Economy, q: &MarketQuery, now: i64) -> Vec<i64> {
    let p = e.search(&EconomyKey::new(MOD), None, q, now).unwrap();
    p.listings.iter().map(|l| l.id).collect()
}

fn ok(id: i64) -> RowDone {
    RowDone {
        id,
        outcome: Ok(None),
    }
}

fn no(id: i64, r: Refusal) -> RowDone {
    RowDone {
        id,
        outcome: Err(r),
    }
}

fn delivery_ids(e: &Economy, account: i64) -> Vec<i64> {
    let mut ids: Vec<i64> = e
        .deliveries(&owner(account))
        .unwrap()
        .iter()
        .map(|d| d.id)
        .collect();
    ids.sort_unstable();
    ids
}

#[test]
fn relist_extends_active() {
    let mut e = econ_with(EconomyConfig {
        market_listing_fee: 100,
        ..shelf_config()
    });
    put_item(&mut e, 1, 1, item(1)).unwrap();
    put_gold(&mut e, 1, 2, 1000).unwrap();
    let l = sell(&mut e, 1, 3, 1, Price::gold(50), 1000);
    assert_eq!(totals(&e).gold, 900, "the listing fee");
    let d = e.relist(&owner(1), &req(1, 4), l, 3, 2 * DAY).unwrap();
    assert_eq!(d.id, Some(l));
    let listing = e.listing(l).unwrap().unwrap();
    assert_eq!(
        (listing.state, listing.expires),
        (ListingState::Active, 5 * DAY)
    );
    assert_eq!(totals(&e).gold, 800, "a relist takes the fee too");
    // The index ends with it: gone after the new expiry, before the old.
    let q = MarketQuery::default();
    assert_eq!(at(&e, &q, 5 * DAY - 1), [l]);
    assert_eq!(at(&e, &q, 5 * DAY + 1), Vec::<i64>::new());
    // A repeat changes nothing.
    let again = e.relist(&owner(1), &req(1, 4), l, 3, 3 * DAY).unwrap();
    assert!(again.repeated);
    assert_eq!(e.listing(l).unwrap().unwrap().expires, 5 * DAY);
    assert_eq!(totals(&e).gold, 800);
    // Shorter is allowed: now + days.
    e.relist(&owner(1), &req(1, 5), l, 1, 3 * DAY).unwrap();
    assert_eq!(e.listing(l).unwrap().unwrap().expires, 4 * DAY);
    // Bad days, someone else's, a sold one.
    for days in [0, 8] {
        assert_eq!(
            refused(e.relist(&owner(1), &req(1, 6 + u64::from(days)), l, days, 3 * DAY)),
            Refusal::BadRequest
        );
    }
    assert_eq!(
        refused(e.relist(&owner(2), &req(2, 1), l, 3, 3 * DAY)),
        Refusal::NotYours
    );
    put_gold(&mut e, 2, 2, 100).unwrap();
    e.buy(&owner(2), &req(2, 3), l, &Price::gold(50).hash(), 3 * DAY)
        .unwrap();
    assert_eq!(
        refused(e.relist(&owner(1), &req(1, 20), l, 3, 3 * DAY)),
        Refusal::ListingGone
    );
    // Past its expiry, before the sweep: gone too (the sweep returns it).
    put_item(&mut e, 1, 21, item(2)).unwrap();
    let l2 = sell(&mut e, 1, 22, 2, Price::gold(50), 3 * DAY);
    assert_eq!(
        refused(e.relist(&owner(1), &req(1, 23), l2, 3, 11 * DAY)),
        Refusal::ListingGone
    );
}

#[test]
fn relist_expired_from_deliveries() {
    let mut e = econ_with(shelf_config());
    put_item(&mut e, 1, 1, item(1)).unwrap();
    let opts = PostOptions {
        note: "fast".into(),
        ..PostOptions::default()
    };
    let l = e
        .post_with(
            &owner(1),
            &req(1, 2),
            &Post {
                seller: "Seller".into(),
                goods: Goods::Item(ItemId(1)),
                mode: ListingMode::PriceAndOffers,
                price: Some(Price::gold(70)),
                days: 1,
            },
            &opts,
            1000,
        )
        .unwrap()
        .id
        .unwrap();
    assert_eq!(e.expire(1000 + DAY).unwrap(), 1);
    let d = e.deliveries(&owner(1)).unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].source.as_str(), d[0].source_code()), ("expired", 4));
    let shop = e
        .shop_page(
            &owner(1),
            ShopTab::Expired,
            0,
            ShopSort::Posted,
            false,
            2 * DAY,
        )
        .unwrap();
    assert_eq!(shop.rows.len(), 1);
    assert!(shop.rows[0].relistable);

    // Relisted from Deliveries: a new listing, same goods, mode, price
    // and note.
    let before = totals(&e);
    let new = e
        .relist(&owner(1), &req(1, 3), l, 7, 2 * DAY)
        .unwrap()
        .id
        .unwrap();
    assert_ne!(new, l);
    assert_eq!(totals(&e), before, "nothing made or lost");
    assert!(e.deliveries(&owner(1)).unwrap().is_empty());
    let n = e.listing(new).unwrap().unwrap();
    assert_eq!(n.state, ListingState::Active);
    assert_eq!(n.goods, Goods::Item(ItemId(1)));
    assert_eq!(n.mode, ListingMode::PriceAndOffers);
    assert_eq!(n.price, Some(Price::gold(70)));
    assert_eq!(n.note, "fast");
    assert_eq!((n.posted, n.expires), (2 * DAY, 9 * DAY));
    assert_eq!(e.listing(l).unwrap().unwrap().state, ListingState::Expired);
    assert_eq!(at(&e, &MarketQuery::default(), 3 * DAY), [new]);
    // Its goods are in the new listing now.
    assert_eq!(
        refused(e.relist(&owner(1), &req(1, 4), l, 7, 2 * DAY)),
        Refusal::ItemNotFound
    );
    let shop = e
        .shop_page(
            &owner(1),
            ShopTab::Expired,
            0,
            ShopSort::Posted,
            false,
            2 * DAY,
        )
        .unwrap();
    assert!(!shop.rows[0].relistable);

    // Cancelled and claimed: relisted from the stash.
    e.cancel(&owner(1), &req(1, 5), new, 3 * DAY).unwrap();
    let d = delivery_ids(&e, 1);
    e.claim(&owner(1), &req(1, 6), d[0], 3 * DAY).unwrap();
    let from_stash = e
        .relist(&owner(1), &req(1, 7), new, 2, 3 * DAY)
        .unwrap()
        .id
        .unwrap();
    assert_eq!(
        e.listing(from_stash).unwrap().unwrap().goods,
        Goods::Item(ItemId(1))
    );

    // A stack, cancelled, relisted from Deliveries; then the stack is
    // short.
    put_stack(&mut e, 1, 8, "r30", 3).unwrap();
    let s = post(
        &mut e,
        1,
        9,
        Goods::Stack {
            code: "r30".into(),
            count: 3,
        },
        ListingMode::Price,
        Some(Price::gold(9)),
        7,
        3 * DAY,
    );
    e.cancel(&owner(1), &req(1, 10), s, 3 * DAY).unwrap();
    let s2 = e.relist(&owner(1), &req(1, 11), s, 7, 3 * DAY).unwrap().id;
    assert!(s2.is_some());
    e.cancel(&owner(1), &req(1, 12), s2.unwrap(), 3 * DAY)
        .unwrap();
    let d = e.deliveries(&owner(1)).unwrap();
    e.take_delivery(&owner(1), &req(1, 13), &chr("Hero", 9), d[0].id, 3 * DAY)
        .unwrap();
    assert_eq!(
        refused(e.relist(&owner(1), &req(1, 14), s2.unwrap(), 7, 3 * DAY)),
        Refusal::ItemNotFound
    );
    // Closed listings leave the shop after 30 days.
    let late = e
        .shop_page(
            &owner(1),
            ShopTab::Expired,
            0,
            ShopSort::Posted,
            false,
            40 * DAY,
        )
        .unwrap();
    assert!(late.rows.is_empty());
    assert_eq!(late.counts, [1, 0, 0, 0]);
}

#[test]
fn bulk_partial_rows_stored() {
    let mut e = econ_with(EconomyConfig {
        market_listing_fee: 100,
        market_max_listings: 3,
        ..shelf_config()
    });
    for id in [1, 2, 3, 5] {
        put_item(&mut e, 1, u64::from(id), item(id)).unwrap();
    }
    put_gold(&mut e, 1, 6, 1000).unwrap();
    for id in [1, 2, 3] {
        sell(&mut e, 1, 10 + u64::from(id), id, Price::gold(10), 1000);
    }
    put_item(&mut e, 2, 1, item(4)).unwrap();
    put_gold(&mut e, 2, 2, 100).unwrap();
    assert_eq!(sell(&mut e, 2, 3, 4, Price::gold(10), 1000), 4);
    let fee = e.config().fee_rev();

    // One row each: done, someone else's, done, not there.
    let r = e
        .shop_edit(
            &owner(1),
            &req(1, 20),
            &ShopOp::Hide,
            &[1, 4, 2, 99],
            fee,
            NOW,
        )
        .unwrap();
    assert!(!r.repeated);
    assert_eq!(
        r.rows,
        [
            ok(1),
            no(4, Refusal::NotYours),
            ok(2),
            no(99, Refusal::ListingGone)
        ]
    );
    let hidden = |e: &Economy, id| e.listing(id).unwrap().unwrap().hidden;
    assert!(hidden(&e, 1) && hidden(&e, 2) && !hidden(&e, 3) && !hidden(&e, 4));
    // The rows are stored with the answer: a repeat gets them, changes
    // nothing.
    let snap = snapshot(&e);
    let again = e
        .shop_edit(&owner(1), &req(1, 20), &ShopOp::Show, &[3], fee, NOW)
        .unwrap();
    assert!(again.repeated);
    assert_eq!(again.rows, r.rows);
    assert_eq!(snapshot(&e), snap);

    // A row that fails half way leaves nothing of it: listing 3 is
    // cancelled, a new post takes the last slot, and relisting 3 takes its
    // delivery back out and then finds no slot.
    e.cancel(&owner(1), &req(1, 21), 3, NOW).unwrap();
    let five = sell(&mut e, 1, 22, 5, Price::gold(10), NOW);
    let waiting = delivery_ids(&e, 1);
    assert_eq!(waiting.len(), 1);
    let gold_before = totals(&e).gold;
    let r = e
        .shop_edit(
            &owner(1),
            &req(1, 23),
            &ShopOp::Relist { days: 7 },
            &[1, 3, five],
            fee,
            NOW,
        )
        .unwrap();
    assert_eq!(r.rows, [ok(1), no(3, Refusal::MarketLimit), ok(five)]);
    assert_eq!(delivery_ids(&e, 1), waiting, "the delivery is back");
    assert_eq!(totals(&e).gold, gold_before - 200, "two fees");
    assert_eq!(e.listing(1).unwrap().unwrap().expires, NOW + 7 * DAY);

    // The whole edit refused: bad rows, stale fees, a bad note or days.
    let edit = |e: &mut Economy, n, op: &ShopOp, ids: &[i64], rev| {
        e.shop_edit(&owner(1), &req(1, n), op, ids, rev, NOW)
            .unwrap_err()
            .refusal()
    };
    let many: Vec<i64> = (1..=51).collect();
    let long = ShopOp::Note("x".repeat(NOTE_MAX + 1));
    for (n, op, ids, rev, want) in [
        (30, &ShopOp::Hide, &[][..], fee, Refusal::BadRequest),
        (31, &ShopOp::Hide, &[1, 2, 1][..], fee, Refusal::BadRequest),
        (32, &ShopOp::Hide, &many[..], fee, Refusal::BadRequest),
        (33, &ShopOp::Hide, &[1][..], fee ^ 1, Refusal::FeeChanged),
        (34, &long, &[1][..], fee, Refusal::BadRequest),
        (
            35,
            &ShopOp::Note("a\nb".into()),
            &[1][..],
            fee,
            Refusal::BadRequest,
        ),
        (
            36,
            &ShopOp::Relist { days: 0 },
            &[1][..],
            fee,
            Refusal::BadRequest,
        ),
        (
            37,
            &ShopOp::Reprice {
                mode: ListingMode::Price,
                price: None,
            },
            &[1][..],
            fee,
            Refusal::BadRequest,
        ),
    ] {
        assert_eq!(edit(&mut e, n, op, ids, rev), Some(want), "request {n}");
    }
    // Refusals are stored like any answer.
    assert_eq!(
        edit(&mut e, 33, &ShopOp::Hide, &[1], fee),
        Some(Refusal::FeeChanged)
    );
    assert_eq!(e.last_req(&Scope::Account(AccountId(1))).unwrap(), 37);

    // Notes and prices on many; cancel on many.
    let r = e
        .shop_edit(
            &owner(1),
            &req(1, 40),
            &ShopOp::Note("bundle deal".into()),
            &[1, five],
            fee,
            NOW,
        )
        .unwrap();
    assert_eq!(r.rows, [ok(1), ok(five)]);
    assert_eq!(e.listing(five).unwrap().unwrap().note, "bundle deal");
    let p = Price::gold(25);
    let r = e
        .shop_edit(
            &owner(1),
            &req(1, 41),
            &ShopOp::Reprice {
                mode: ListingMode::PriceAndOffers,
                price: Some(p.clone()),
            },
            &[1, 2],
            fee,
            NOW,
        )
        .unwrap();
    assert_eq!(r.rows, [ok(1), ok(2)]);
    assert_eq!(e.listing(2).unwrap().unwrap().price, Some(p));
    let before = totals(&e);
    let r = e
        .shop_edit(
            &owner(1),
            &req(1, 42),
            &ShopOp::Cancel,
            &[1, 2, five, 3],
            fee,
            NOW,
        )
        .unwrap();
    assert_eq!(
        r.rows,
        [ok(1), ok(2), ok(five), no(3, Refusal::ListingGone)]
    );
    assert_eq!(totals(&e), before);
    assert_eq!(delivery_ids(&e, 1).len(), 4);
    let off = e.counts(&owner(1)).unwrap();
    assert_eq!((off.active_listings, off.deliveries_waiting), (0, 4));
}

#[test]
fn claim_many_stops_at_full_stash() {
    let mut e = econ_with(EconomyConfig {
        stash_items: 3,
        ..shelf_config()
    });
    for id in 1..=3 {
        put_item(&mut e, 1, u64::from(id), item(id)).unwrap();
        sell(&mut e, 1, 10 + u64::from(id), id, Price::gold(100), 1000);
    }
    put_gold(&mut e, 2, 1, 10_000).unwrap();
    put_item(&mut e, 2, 2, item(9)).unwrap();
    for l in 1..=3 {
        e.buy(
            &owner(2),
            &req(2, 2 + l as u64),
            l,
            &Price::gold(100).hash(),
            2000,
        )
        .unwrap();
    }
    let mine = delivery_ids(&e, 2);
    assert_eq!(mine.len(), 3);
    let before = totals(&e);
    // Room for two more items: the oldest two go in, the third stays.
    let r = e
        .claim_many(&owner(2), &req(2, 10), &Claim::All, NOW)
        .unwrap();
    assert_eq!(
        r.rows,
        [ok(mine[0]), ok(mine[1]), no(mine[2], Refusal::StashFull)]
    );
    assert_eq!(delivery_ids(&e, 2), [mine[2]]);
    assert_eq!(totals(&e), before);
    let snap = snapshot(&e);
    let again = e
        .claim_many(&owner(2), &req(2, 10), &Claim::All, NOW)
        .unwrap();
    assert!(again.repeated);
    assert_eq!(again.rows, r.rows);
    assert_eq!(snapshot(&e), snap);

    // Named deliveries: someone else's is refused on its row.
    let sold = delivery_ids(&e, 1);
    assert_eq!(sold.len(), 3);
    let ids = vec![sold[2], mine[2], sold[0]];
    let r = e
        .claim_many(&owner(1), &req(1, 20), &Claim::Ids(ids), NOW)
        .unwrap();
    assert_eq!(
        r.rows,
        [ok(sold[2]), no(mine[2], Refusal::ItemNotFound), ok(sold[0])]
    );
    assert_eq!(delivery_ids(&e, 1), [sold[1]]);
    // Bad lists; nothing to claim.
    for (n, ids) in [
        (21, vec![]),
        (22, vec![sold[1], sold[1]]),
        (23, (1..=51).collect()),
    ] {
        assert_eq!(
            e.claim_many(&owner(1), &req(1, n), &Claim::Ids(ids), NOW)
                .unwrap_err()
                .refusal(),
            Some(Refusal::BadRequest)
        );
    }
    let r = e
        .claim_many(&owner(3), &req(3, 1), &Claim::All, NOW)
        .unwrap();
    assert!(r.rows.is_empty());
}

fn crest(id: u32) -> Item {
    thing(id, "Harlequin Crest", &["helm", "armo"], 7)
}

const CREST: &str = "u:Harlequin Crest";

#[test]
fn sale_writes_history_with_paid_price() {
    let mut e = econ_with(EconomyConfig {
        market_fee_pct: 10,
        ..shelf_config()
    });
    let fee = e.config().fee_rev();
    put_item(&mut e, 1, 1, crest(1)).unwrap();
    put_item(&mut e, 1, 2, crest(2)).unwrap();
    put_item(&mut e, 1, 3, crest(3)).unwrap();
    let a = sell(&mut e, 1, 4, 1, Price::gold(1000), 1000);
    let b = post(
        &mut e,
        1,
        5,
        Goods::Item(ItemId(2)),
        ListingMode::PriceAndOffers,
        Some(Price::gold(2000)),
        7,
        1000,
    );
    assert_eq!(e.listing_price_key(a).unwrap().as_deref(), Some(CREST));
    assert_eq!(e.listing_price_key(99).unwrap(), None);
    assert_eq!(
        e.item_price_key(&owner(1), ItemId(3)).unwrap().as_deref(),
        Some(CREST)
    );
    assert_eq!(
        e.item_price_key(&owner(1), ItemId(1)).unwrap(),
        None,
        "listed"
    );
    assert_eq!(e.item_price_key(&owner(2), ItemId(3)).unwrap(), None);
    put_gold(&mut e, 2, 1, 10_000).unwrap();
    e.buy_with(
        &owner(2),
        &req(2, 2),
        a,
        "Buyer",
        &Price::gold(1000).hash(),
        fee,
        2000,
    )
    .unwrap();
    // Bought below the list price: the history has what was paid.
    let o = e
        .offer_with(
            &owner(2),
            &req(2, 3),
            b,
            "Buyer",
            &Price::gold(1500),
            fee,
            3000,
        )
        .unwrap()
        .id
        .unwrap();
    e.answer_with(&owner(1), &req(1, 6), o, &OfferAnswer::Accept, fee, 4000)
        .unwrap();

    let key = EconomyKey::new(MOD);
    let h = e.price_history(&key, CREST, 7, 5000).unwrap();
    assert_eq!(
        (h.label.as_str(), h.bucket_ms, h.sold),
        ("Harlequin Crest", DAY, 2)
    );
    assert_eq!(h.listed_now, 0);
    assert_eq!(
        h.series,
        [Series {
            shape: Shape::One,
            currencies: vec![String::new()],
            sold: 2,
            listed_now: 0,
            lowest_now: vec![],
            median_now: vec![],
            buckets: vec![Bucket {
                index: 0,
                sales: 2,
                values: vec![(1000, 1000, 1500)],
            }],
        }]
    );
    assert_eq!(
        h.recent,
        [(4000, Price::gold(1500)), (2000, Price::gold(1000))]
    );
    // The Sold tab: buyer, price paid and the fee.
    let sold = e
        .shop_page(&owner(1), ShopTab::Sold, 0, ShopSort::Posted, false, 5000)
        .unwrap();
    let rows: Vec<(i64, Option<Sold>)> = sold
        .rows
        .iter()
        .map(|r| (r.listing.id, r.sold.clone()))
        .collect();
    assert_eq!(
        rows,
        [
            (
                a,
                Some(Sold {
                    buyer: "Buyer".into(),
                    paid: Price::gold(1000),
                    sale_fee: 100,
                    closed: 2000,
                })
            ),
            (
                b,
                Some(Sold {
                    buyer: "Buyer".into(),
                    paid: Price::gold(1500),
                    sale_fee: 150,
                    closed: 4000,
                })
            ),
        ]
    );
    assert_eq!(sold.counts, [0, 0, 2, 0]);
    // An hour bucket for a day or two; a bad window; the market off.
    let h = e.price_history(&key, CREST, 1, 5000).unwrap();
    assert_eq!((h.bucket_ms, h.sold), (HOUR, 2));
    for days in [0, 91] {
        assert_eq!(
            e.price_history(&key, CREST, days, 5000)
                .unwrap_err()
                .refusal(),
            Some(Refusal::BadRequest)
        );
    }
    let off = econ_with(EconomyConfig {
        market: false,
        ..shelf_config()
    });
    assert_eq!(
        off.price_history(&key, CREST, 7, 5000)
            .unwrap_err()
            .refusal(),
        Some(Refusal::MarketOff)
    );
    // Other keys and other splits are apart.
    assert_eq!(
        e.price_history(&key, "u:Raven Frost", 7, 5000)
            .unwrap()
            .sold,
        0
    );
    let hc = EconomyKey {
        hardcore: true,
        ..EconomyKey::new(MOD)
    };
    assert_eq!(e.price_history(&hc, CREST, 7, 5000).unwrap().sold, 0);
}

#[test]
fn history_series_per_currency_set() {
    let mut e = econ_with(shelf_config());
    let h = 10 * DAY;
    let posted = h - 4 * DAY;
    for id in 1..=7 {
        put_item(&mut e, 1, u64::from(id), crest(id)).unwrap();
    }
    let jah = |n| ("r31".to_owned(), n);
    let prices = [
        Some(Price::gold(1000)),
        Some(Price::gold(3000)),
        Some(market::price(0, vec![ber(2)])),
        Some(market::price(500, vec![jah(1)])),
        None,
        Some(Price::gold(800)),
        Some(Price::gold(900)),
    ];
    for (i, p) in prices.iter().enumerate() {
        let mode = if p.is_some() {
            ListingMode::Price
        } else {
            ListingMode::OffersOnly
        };
        let id = i as u32 + 1;
        let l = post(
            &mut e,
            1,
            10 + u64::from(id),
            Goods::Item(ItemId(id)),
            mode,
            p.clone(),
            7,
            posted,
        );
        assert_eq!(l, i64::from(id));
    }
    put_gold(&mut e, 2, 1, 1_000_000).unwrap();
    put_stack(&mut e, 2, 2, "r30", 10).unwrap();
    put_stack(&mut e, 2, 3, "r31", 10).unwrap();
    put_item(&mut e, 2, 4, item(20)).unwrap();
    let buy = |e: &mut Economy, n, l: i64, at| {
        let p = prices[l as usize - 1].clone().unwrap();
        e.buy(&owner(2), &req(2, n), l, &p.hash(), at).unwrap();
    };
    buy(&mut e, 10, 1, h - 3 * DAY + 500);
    buy(&mut e, 11, 2, h - 3 * DAY + 1000);
    buy(&mut e, 12, 3, h - DAY / 2);
    buy(&mut e, 13, 4, h - DAY / 2);
    let with_item = offers::bundle(10, vec![], &[20]);
    let o = e
        .offer(&owner(2), &req(2, 14), 5, "Buyer", &with_item, h - 200)
        .unwrap()
        .id
        .unwrap();
    e.answer(&owner(1), &req(1, 30), o, &OfferAnswer::Accept, h - 100)
        .unwrap();

    let key = EconomyKey::new(MOD);
    let hist = e.price_history(&key, CREST, 7, h).unwrap();
    assert_eq!((hist.sold, hist.listed_now), (5, 2));
    let series = |shape, cur: &[&str], sold, listed, now: (Vec<u64>, Vec<u64>), buckets| Series {
        shape,
        currencies: cur.iter().map(|c| (*c).to_owned()).collect(),
        sold,
        listed_now: listed,
        lowest_now: now.0,
        median_now: now.1,
        buckets,
    };
    let bucket = |index, sales, values| Bucket {
        index,
        sales,
        values,
    };
    assert_eq!(
        hist.series,
        [
            series(
                Shape::One,
                &[""],
                2,
                2,
                (vec![800], vec![800]),
                vec![bucket(2, 2, vec![(1000, 1000, 3000)])]
            ),
            series(
                Shape::One,
                &["r30"],
                1,
                0,
                (vec![], vec![]),
                vec![bucket(0, 1, vec![(2, 2, 2)])]
            ),
            series(
                Shape::Bundle,
                &["", "r31"],
                1,
                0,
                (vec![], vec![]),
                vec![bucket(0, 1, vec![(500, 500, 500), (1, 1, 1)])]
            ),
            series(
                Shape::WithItems,
                &[],
                1,
                0,
                (vec![], vec![]),
                vec![bucket(0, 1, vec![])]
            ),
        ]
    );
    assert_eq!(hist.recent.len(), 5);
    assert_eq!(hist.recent[0].0, h - 100, "newest first");
    // The last two days, by the hour: the older gold sales are out.
    let recent = e.price_history(&key, CREST, 2, h).unwrap();
    assert_eq!(recent.sold, 3);
    let sets: Vec<(Shape, u32, Vec<u32>)> = recent
        .series
        .iter()
        .map(|s| (s.shape, s.sold, s.buckets.iter().map(|b| b.index).collect()))
        .collect();
    assert_eq!(
        sets,
        [
            (Shape::One, 0, vec![]),
            (Shape::One, 1, vec![12]),
            (Shape::Bundle, 1, vec![12]),
            (Shape::WithItems, 1, vec![0]),
        ],
        "gold is listed now, not sold in the window"
    );
    // Listed now, unsold in the span: a series with 0 sold, no buckets.
    let gold = &recent.series[0];
    assert_eq!(
        (gold.listed_now, &gold.lowest_now, &gold.median_now),
        (2, &vec![800], &vec![800])
    );
    assert!(gold.buckets.is_empty());
}

/// The notices taken now, as (account, what), in order.
fn told(e: &mut Economy) -> Vec<(i64, NoticeKind)> {
    e.take_notices()
        .unwrap()
        .iter()
        .map(|n| (n.account.0, n.what))
        .collect()
}

fn told_sorted(e: &mut Economy) -> Vec<(i64, NoticeKind)> {
    let mut t = told(e);
    t.sort_by_key(|(a, k)| (*a, *k as u8));
    t
}

#[test]
fn notices_for_each_event() {
    use NoticeKind::*;
    let mut e = econ_with(shelf_config());
    for id in 1..=5 {
        put_item(&mut e, 1, u64::from(id), item(id)).unwrap();
    }
    put_gold(&mut e, 2, 1, 1_000_000).unwrap();
    put_gold(&mut e, 3, 1, 1_000_000).unwrap();
    let l1 = post(
        &mut e,
        1,
        10,
        Goods::Item(ItemId(1)),
        ListingMode::PriceAndOffers,
        Some(Price::gold(1000)),
        7,
        1000,
    );
    let l2 = post(
        &mut e,
        1,
        11,
        Goods::Item(ItemId(2)),
        ListingMode::OffersOnly,
        None,
        7,
        1000,
    );
    let l3 = sell(&mut e, 1, 12, 3, Price::gold(50), 1000);
    let l4 = post(
        &mut e,
        1,
        13,
        Goods::Item(ItemId(4)),
        ListingMode::Price,
        Some(Price::gold(10)),
        1,
        1000,
    );
    assert_eq!(told(&mut e), [], "puts and posts tell no one");

    let offer = |e: &mut Economy, account: i64, n, l, g| {
        let who = if account == 2 { "Buyer" } else { "Third" };
        e.offer(
            &owner(account),
            &req(account, n),
            l,
            who,
            &Price::gold(g),
            2000,
        )
        .unwrap()
        .id
        .unwrap()
    };
    let a = offer(&mut e, 2, 2, l1, 500);
    let n = e.take_notices().unwrap();
    assert_eq!(n.len(), 1);
    let split = EconomyKey::new(MOD).split();
    assert_eq!(
        n[0],
        Notice {
            account: AccountId(1),
            economy: split.clone(),
            what: OfferReceived,
            listing: l1,
            offer: Some(a),
            about: "Havoc Loop 1".into(),
            who: "Buyer".into(),
            price: Some(Price::gold(500)),
            deliveries_waiting: 0,
            at: 2000,
        }
    );
    e.answer(
        &owner(1),
        &req(1, 20),
        a,
        &OfferAnswer::Counter(Price::gold(800)),
        2000,
    )
    .unwrap();
    let n = e.take_notices().unwrap();
    assert_eq!(
        (
            n[0].account.0,
            n[0].what,
            n[0].who.as_str(),
            n[0].price.clone()
        ),
        (2, Countered, "Seller", Some(Price::gold(800)))
    );
    let b = offer(&mut e, 3, 2, l1, 600);
    assert_eq!(told(&mut e), [(1, OfferReceived)]);
    let c = e.counts(&owner(1)).unwrap();
    assert_eq!(
        (c.active_listings, c.live_offers_to_me, c.my_live_offers),
        (4, 2, 0)
    );
    assert_eq!(e.counts(&owner(2)).unwrap().my_live_offers, 1);

    // The buyer takes the counter: the seller is told of the sale, the
    // other buyer that their offer is back.
    e.answer(&owner(2), &req(2, 3), a, &OfferAnswer::Accept, 2000)
        .unwrap();
    assert_eq!(told_sorted(&mut e), [(1, Sold), (3, OfferReturned)]);
    let _ = b;

    // Declined; withdrawn; accepted by the seller.
    let c1 = offer(&mut e, 2, 4, l2, 100);
    e.answer(&owner(1), &req(1, 21), c1, &OfferAnswer::Decline, 2000)
        .unwrap();
    assert_eq!(told(&mut e), [(1, OfferReceived), (2, OfferDeclined)]);
    let d = offer(&mut e, 2, 5, l2, 200);
    e.answer(&owner(2), &req(2, 6), d, &OfferAnswer::Withdraw, 2000)
        .unwrap();
    assert_eq!(told(&mut e), [(1, OfferReceived), (1, OfferWithdrawn)]);
    let f = offer(&mut e, 3, 3, l2, 300);
    let _ = told(&mut e);
    e.answer(&owner(1), &req(1, 22), f, &OfferAnswer::Accept, 2000)
        .unwrap();
    let n = e.take_notices().unwrap();
    let mut kinds: Vec<(i64, NoticeKind, String)> = n
        .iter()
        .map(|n| (n.account.0, n.what, n.who.clone()))
        .collect();
    kinds.sort_by_key(|(a, k, _)| (*a, *k as u8));
    assert_eq!(
        kinds,
        [
            (1, Sold, "Third".into()),
            (3, OfferAccepted, "Seller".into())
        ]
    );

    // Bought at its price; a refused call tells no one.
    let fee = e.config().fee_rev();
    e.buy_with(
        &owner(3),
        &req(3, 4),
        l3,
        "Third",
        &Price::gold(50).hash(),
        fee,
        2000,
    )
    .unwrap();
    let n = e.take_notices().unwrap();
    assert_eq!(
        (
            n[0].account.0,
            n[0].what,
            n[0].who.as_str(),
            n[0].price.clone()
        ),
        (1, Sold, "Third", Some(Price::gold(50)))
    );
    assert_eq!(n[0].deliveries_waiting, 3, "three sales' gold");
    assert!(e
        .buy(&owner(3), &req(3, 5), l3, &Price::gold(50).hash(), 2000)
        .is_err());
    assert_eq!(told(&mut e), []);

    // Expired.
    e.expire(1000 + DAY).unwrap();
    let n = e.take_notices().unwrap();
    assert_eq!(
        (n[0].account.0, n[0].what, n[0].listing, n[0].at),
        (1, ListingExpired, l4, 1000 + DAY)
    );
    assert_eq!(n[0].deliveries_waiting, 4);

    // Cancelled in a shop edit with an offer on it; the refused row adds
    // nothing.
    let l5 = post(
        &mut e,
        1,
        14,
        Goods::Item(ItemId(5)),
        ListingMode::OffersOnly,
        None,
        7,
        2 * DAY,
    );
    e.offer(&owner(2), &req(2, 7), l5, "Buyer", &Price::gold(5), 2 * DAY)
        .unwrap();
    let _ = told(&mut e);
    let r = e
        .shop_edit(
            &owner(1),
            &req(1, 23),
            &ShopOp::Cancel,
            &[l5, l3],
            fee,
            2 * DAY,
        )
        .unwrap();
    assert_eq!(r.rows[1].outcome, Err(Refusal::ListingGone));
    assert_eq!(told(&mut e), [(2, OfferReturned)]);
    // Taking them empties them.
    assert_eq!(told(&mut e), []);
}

#[test]
fn fee_rev_changes_with_fees() {
    let base = EconomyConfig::default();
    assert_eq!(
        (
            base.market_fee_pct,
            base.market_listing_fee,
            base.market_buyer_fee
        ),
        (0, 0, 0),
        "no fees by default"
    );
    let revs = [
        base.fee_rev(),
        EconomyConfig {
            market_fee_pct: 5,
            ..base.clone()
        }
        .fee_rev(),
        EconomyConfig {
            market_listing_fee: 5,
            ..base.clone()
        }
        .fee_rev(),
        EconomyConfig {
            market_buyer_fee: 5,
            ..base.clone()
        }
        .fee_rev(),
    ];
    for i in 0..revs.len() {
        for j in 0..i {
            assert_ne!(revs[i], revs[j], "{i} and {j}");
        }
    }
    assert_eq!(base.fee_rev(), EconomyConfig::default().fee_rev());
    assert_eq!(
        EconomyConfig {
            market: true,
            market_days: 3,
            ..base.clone()
        }
        .fee_rev(),
        revs[0],
        "only the fees count"
    );

    // Calls that pay or earn under stale fees are refused; under the
    // current ones they go through.
    let mut e = econ_with(EconomyConfig {
        market_fee_pct: 5,
        ..shelf_config()
    });
    let stale = shelf_config().fee_rev();
    let fee = e.config().fee_rev();
    assert_ne!(stale, fee);
    put_item(&mut e, 1, 1, item(1)).unwrap();
    put_item(&mut e, 1, 2, item(2)).unwrap();
    put_gold(&mut e, 2, 1, 10_000).unwrap();
    let post = |rev| {
        let p = Post {
            seller: "Seller".into(),
            goods: Goods::Item(ItemId(1)),
            mode: ListingMode::PriceAndOffers,
            price: Some(Price::gold(100)),
            days: 7,
        };
        let opts = PostOptions {
            fee_rev: Some(rev),
            ..PostOptions::default()
        };
        (p, opts)
    };
    let (p, opts) = post(stale);
    assert_eq!(
        refused(e.post_with(&owner(1), &req(1, 3), &p, &opts, NOW)),
        Refusal::FeeChanged
    );
    let (p, opts) = post(fee);
    let l = e
        .post_with(&owner(1), &req(1, 4), &p, &opts, NOW)
        .unwrap()
        .id
        .unwrap();
    let hash = Price::gold(100).hash();
    assert_eq!(
        refused(e.buy_with(&owner(2), &req(2, 2), l, "Buyer", &hash, stale, NOW)),
        Refusal::FeeChanged
    );
    assert_eq!(
        refused(e.offer_with(
            &owner(2),
            &req(2, 3),
            l,
            "Buyer",
            &Price::gold(50),
            stale,
            NOW
        )),
        Refusal::FeeChanged
    );
    let o = e
        .offer_with(
            &owner(2),
            &req(2, 4),
            l,
            "Buyer",
            &Price::gold(50),
            fee,
            NOW,
        )
        .unwrap()
        .id
        .unwrap();
    assert_eq!(
        refused(e.answer_with(&owner(1), &req(1, 5), o, &OfferAnswer::Accept, stale, NOW)),
        Refusal::FeeChanged
    );
    assert_eq!(
        e.shop_edit(&owner(1), &req(1, 6), &ShopOp::Hide, &[l], stale, NOW)
            .unwrap_err()
            .refusal(),
        Some(Refusal::FeeChanged)
    );
    e.buy_with(&owner(2), &req(2, 5), l, "Buyer", &hash, fee, NOW)
        .unwrap();
    assert_eq!(delivered_gold(&e, 1), 95, "the sale fee: 5 of 100 gold");
    assert_eq!(Refusal::FeeChanged.code(), 91);
    assert_eq!(Refusal::NotYours.code(), 92);
    assert_eq!(Refusal::RateLimited.code(), 24);
    assert_eq!(Refusal::MarketOff.code(), 90);
}

fn delivered_gold(e: &Economy, account: i64) -> u64 {
    e.deliveries(&owner(account))
        .unwrap()
        .iter()
        .map(|d| match d.goods {
            Goods::Gold(g) => g,
            _ => 0,
        })
        .sum()
}

/// The index tables, row by row (not the name index: its text is the
/// older build's).
fn index_rows(e: &Economy) -> String {
    let all = snapshot(e);
    all.split("\n[")
        .filter(|t| {
            [
                "market_items]",
                "market_prices]",
                "market_stats]",
                "market_type_tree]",
            ]
            .iter()
            .any(|name| t.starts_with(name))
        })
        .collect::<Vec<_>>()
        .join("\n[")
}

#[test]
fn migration_5_on_a_v4_database() {
    let path = temp_db("migration-5");
    let split = EconomyKey::new(MOD).split();
    let scope = Scope::Account(AccountId(1)).to_string();
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        for (i, sql) in crate::db::MIGRATIONS[..4].iter().enumerate() {
            conn.execute_batch(sql).unwrap();
            conn.execute_batch(&format!("PRAGMA user_version = {}", i + 1))
                .unwrap();
        }
        let price = Price::gold(1000).encode();
        let jah = market::price(0, vec![("r31".into(), 2)]).encode();
        conn.execute_batch(&format!(
            "INSERT INTO meta(key, value) VALUES ('mod_id', '{MOD}');
             INSERT INTO econ_items(item_id, economy, account, place, place_id, added, record, \
                 code, item_type, quality, name, names, level_req, ilvl, sockets, ethereal, class) \
             VALUES (1, '{split}', 1, 'listing', 1, 1000, x'4a4d01', 'uap', 'helm', 7, \
                 'Harlequin Crest', 'Harlequin Crest Shako', 62, 80, 1, 0, NULL),
                    (2, '{split}', 1, 'stash', NULL, 1000, x'4a4d02', 'rin', 'ring', 6, \
                 'Havoc Loop', 'Ring Havoc Loop', 30, 80, 0, 0, NULL);
             INSERT INTO item_stats(item_id, stat, value) VALUES (1, 127, 2), (1, 105, 10);
             INSERT INTO listings(id, account, economy, seller, item_id, code, count, mode, price, \
                 price_gold, state, posted, expires) \
             VALUES (1, 1, '{split}', 'Seller', 1, NULL, NULL, 0, x'{}', 1000, 'active', 1000, \
                 {exp}),
                    (2, 1, '{split}', 'Seller', NULL, 'r30', 2, 2, x'{}', 0, 'active', 2000, \
                 {exp});
             INSERT INTO escrow_stacks(holder, holder_id, code, count) VALUES ('listing', 2, 'r30', 2);
             INSERT INTO market_items(listing, economy, item_id, code, item_type, quality, \
                 level_req, ilvl, sockets, ethereal, class, count, mode, price_gold, posted, expires) \
             VALUES (1, '{split}', 1, 'uap', 'helm', 7, 62, 80, 1, 0, NULL, NULL, 0, 1000, 1000, \
                 {exp}),
                    (2, '{split}', NULL, 'r30', 'r30', 0, 0, 0, 0, 0, NULL, 2, 2, NULL, 2000, {exp});
             INSERT INTO market_prices(listing, code, count) VALUES (2, 'r31', 2);
             INSERT INTO market_stats(listing, stat, value) VALUES (1, 127, 2), (1, 105, 10);
             INSERT INTO market_names(rowid, names) VALUES (1, 'Harlequin Crest Shako uap helm'), \
                 (2, 'Ber Rune r30');
             INSERT INTO requests(scope, req, code, id, at) VALUES ('{scope}', 7, 0, 1, 1000);",
            hex(&price),
            hex(&jah),
            exp = 7 * DAY + 1000,
        ))
        .unwrap();
    }
    let mut e = Economy::open(&path, MOD, shelf_config()).unwrap();
    let version: i64 = e
        .conn()
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(version, 5);

    // Old rows read as before, with the new fields' defaults.
    let ring = crate::stash::page::load_item(e.conn(), ItemId(2))
        .unwrap()
        .unwrap();
    assert!(ring.facts.identified && !ring.facts.runeword);
    assert_eq!(ring.facts.type_codes(), ["ring"]);
    let one = e.listing(1).unwrap().unwrap();
    assert_eq!(
        (one.hidden, one.note.as_str(), one.group),
        (false, "", Group::Unique)
    );
    assert_eq!(e.listing(2).unwrap().unwrap().group, Group::Stackable);
    // The old index answers the new queries.
    let cap = |code: Option<&str>, max| PriceFilter {
        code: code.map(str::to_owned),
        min: 0,
        max,
    };
    assert_eq!(query(&e, |q| q.types = vec!["helm".into()]), [1]);
    assert_eq!(
        query(&e, |q| q.types = vec!["rune".into()]),
        [2],
        "from the realm's stackables"
    );
    assert_eq!(query(&e, |q| q.groups = vec![Group::Unique]), [1]);
    assert_eq!(query(&e, |q| q.groups = vec![Group::Stackable]), [2]);
    assert_eq!(query(&e, |q| q.price_mode = PriceMode::CurrenciesOnly), [2]);
    assert_eq!(query(&e, |q| q.price_mode = PriceMode::GoldOnly), [1]);
    assert_eq!(query(&e, |q| q.price_mode = PriceMode::TakesOffers), [2]);
    assert_eq!(query(&e, |q| q.caps = vec![cap(Some("r31"), 2)]), [2]);
    assert_eq!(query(&e, |q| q.caps = vec![cap(None, 1000)]), [1]);
    assert_eq!(query(&e, |q| q.filter.text = Some("harlequin".into())), [1]);
    assert_eq!(
        query(&e, |q| {
            q.group_first = true;
            q.desc = true;
        }),
        [1, 2]
    );
    assert_eq!(
        query(&e, |q| {
            q.sort = MarketSort::Name;
            q.desc = true;
        }),
        [1, 2],
        "Harlequin Crest, Ber Rune"
    );
    let f = e
        .facets(&EconomyKey::new(MOD), &MarketQuery::default(), NOW)
        .unwrap();
    assert_eq!(f.groups, [(Group::Unique, 1), (Group::Stackable, 1)]);
    assert_eq!(f.socketed, [1, 1]);
    // ... and matches what this build would index.
    let migrated = index_rows(&e);
    e.sweep(0, |a| {
        for id in [1, 2] {
            a.unindex_listing(id)?;
            a.index_listing(id)?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(index_rows(&e), migrated);
    let h = e
        .price_history(&EconomyKey::new(MOD), CREST, 7, NOW)
        .unwrap();
    assert_eq!((h.listed_now, h.sold), (1, 0));

    // A stored answer still answers.
    let post = Post {
        seller: "Seller".into(),
        goods: Goods::Item(ItemId(2)),
        mode: ListingMode::Price,
        price: Some(Price::gold(5)),
        days: 7,
    };
    let d = e.post(&owner(1), &req(1, 7), &post, NOW).unwrap();
    assert_eq!((d.id, d.repeated), (Some(1), true));
    // The new calls work on the old rows.
    e.hide(&owner(1), &req(1, 8), 1, NOW).unwrap();
    assert_eq!(query(&e, |_| ()), [2]);
    put_stack(&mut e, 2, 1, "r31", 2).unwrap();
    let jah = market::price(0, vec![("r31".into(), 2)]);
    e.buy(&owner(2), &req(2, 2), 2, &jah.hash(), NOW).unwrap();
    let h = e
        .price_history(&EconomyKey::new(MOD), "s:r30", 7, NOW)
        .unwrap();
    assert_eq!((h.sold, h.label.as_str()), (1, "Ber Rune"));
    // Opening it again changes nothing.
    let snap = snapshot(&e);
    drop(e);
    let e = Economy::open(&path, MOD, shelf_config()).unwrap();
    assert_eq!(snapshot(&e), snap);
}

/// The listings found by the default query as `f` changes it.
fn query(e: &Economy, f: impl Fn(&mut MarketQuery)) -> Vec<i64> {
    let mut q = MarketQuery::default();
    f(&mut q);
    at(e, &q, NOW)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
