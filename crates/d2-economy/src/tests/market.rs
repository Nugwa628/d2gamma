//! The market core: posting, buying, cancelling, expiry, fees, limits and
//! the Deliveries tab.

use super::*;

pub(crate) const DAY: i64 = 24 * 3600 * 1000;

pub(crate) fn market_config() -> EconomyConfig {
    config()
}

pub(crate) fn market() -> Economy {
    econ()
}

pub(crate) fn ber(count: u32) -> (String, u32) {
    ("r30".into(), count)
}

pub(crate) fn price(gold: u64, stacks: Vec<(String, u32)>) -> Price {
    Price {
        gold,
        stacks,
        items: vec![],
    }
}

/// Seller 1 with item 1 in the stash, buyer 2 with 200,000 gold and 5 Ber.
pub(crate) fn seller_and_buyer(e: &mut Economy) {
    put_item(e, 1, 1, item(1)).unwrap();
    put_gold(e, 2, 1, 200_000).unwrap();
    put_stack(e, 2, 2, "r30", 5).unwrap();
}

pub(crate) fn post_item(
    e: &mut Economy,
    account: i64,
    n: u64,
    id: u32,
    p: Price,
) -> Result<Done, Error> {
    e.post(
        &owner(account),
        &req(account, n),
        &Post {
            seller: "Seller".into(),
            goods: Goods::Item(ItemId(id)),
            mode: ListingMode::Price,
            price: Some(p),
            days: 7,
        },
        1000,
    )
}

pub(crate) fn refused(r: Result<Done, Error>) -> Refusal {
    r.unwrap_err().refusal().expect("a refusal")
}

pub(crate) fn delivered(e: &Economy, account: i64) -> Vec<(String, Goods)> {
    let mut d: Vec<(String, Goods)> = e
        .deliveries(&owner(account))
        .unwrap()
        .into_iter()
        .map(|d| (d.source, d.goods))
        .collect();
    d.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    d
}

pub(crate) fn gold(g: u64) -> Goods {
    Goods::Gold(g)
}

pub(crate) fn stack(code: &str, count: u32) -> Goods {
    Goods::Stack {
        code: code.into(),
        count,
    }
}

#[test]
fn buy_moves_item_and_price_once() {
    let mut e = market();
    seller_and_buyer(&mut e);
    let p = price(150_000, vec![ber(2)]);
    let listing = post_item(&mut e, 1, 2, 1, p.clone()).unwrap().id.unwrap();
    let l = e.listing(listing).unwrap().unwrap();
    assert_eq!(l.state, ListingState::Active);
    assert_eq!(l.item.as_ref().unwrap(), &item(1));
    assert_eq!(l.expires, 1000 + 7 * DAY);
    assert_eq!(
        e.stash_items(&owner(1), &StashQuery::default())
            .unwrap()
            .total,
        0
    );
    let before = totals(&e);
    let bought = e
        .buy(&owner(2), &req(2, 3), listing, &p.hash(), 2000)
        .unwrap();
    assert!(!bought.repeated);
    // The buyer paid once, the seller got the price, the buyer the item.
    assert_eq!(e.stash_gold(&owner(2)).unwrap(), 50_000);
    assert_eq!(e.stash_stacks(&owner(2)).unwrap(), vec![("r30".into(), 3)]);
    assert_eq!(
        delivered(&e, 1),
        [
            ("sold".into(), gold(150_000)),
            ("sold".into(), stack("r30", 2))
        ]
    );
    assert_eq!(
        delivered(&e, 2),
        [("bought".into(), Goods::Item(ItemId(1)))]
    );
    assert_eq!(
        e.listing(listing).unwrap().unwrap().state,
        ListingState::Sold
    );
    assert_eq!(totals(&e), before, "nothing made or lost");
    // A repeat changes nothing; another buy of it is refused.
    assert!(
        e.buy(&owner(2), &req(2, 3), listing, &p.hash(), 2000)
            .unwrap()
            .repeated
    );
    assert_eq!(e.stash_gold(&owner(2)).unwrap(), 50_000);
    put_gold(&mut e, 3, 1, 200_000).unwrap();
    put_stack(&mut e, 3, 2, "r30", 2).unwrap();
    assert_eq!(
        refused(e.buy(&owner(3), &req(3, 3), listing, &p.hash(), 2000)),
        Refusal::ListingGone
    );
    let kinds: Vec<String> = e
        .item_history(ItemId(1))
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect();
    assert_eq!(kinds, ["put", "list", "bought"]);
}

#[test]
fn buy_refusals() {
    let mut e = market();
    seller_and_buyer(&mut e);
    let p = price(150_000, vec![ber(2)]);
    let listing = post_item(&mut e, 1, 2, 1, p.clone()).unwrap().id.unwrap();
    let before = snapshot_except(&e, "requests");
    let buy = |e: &mut Economy, account, n, hash: [u8; 32], now| {
        refused(e.buy(&owner(account), &req(account, n), listing, &hash, now))
    };
    assert_eq!(
        buy(&mut e, 2, 10, price(150_000, vec![]).hash(), 2000),
        Refusal::PriceChanged
    );
    assert_eq!(
        buy(&mut e, 1, 10, p.hash(), 2000),
        Refusal::BadRequest,
        "own listing"
    );
    put_gold(&mut e, 3, 1, 149_999).unwrap();
    put_stack(&mut e, 3, 2, "r30", 9).unwrap();
    let before3 = snapshot_except(&e, "requests");
    assert_eq!(buy(&mut e, 3, 3, p.hash(), 2000), Refusal::NotEnough);
    assert_eq!(snapshot_except(&e, "requests"), before3, "nothing paid");
    assert_eq!(
        buy(&mut e, 2, 11, p.hash(), 1000 + 7 * DAY),
        Refusal::ListingGone,
        "expired"
    );
    let hc = Owner::new(
        2,
        EconomyKey {
            hardcore: true,
            ..EconomyKey::new(MOD)
        },
    );
    assert_eq!(
        refused(e.buy(&hc, &req(2, 12), listing, &p.hash(), 2000)),
        Refusal::EconomyMismatch
    );
    let pd2 = Owner::new(2, EconomyKey::new("pd2"));
    assert_eq!(
        refused(e.buy(&pd2, &req(2, 13), listing, &p.hash(), 2000)),
        Refusal::EconomyMismatch
    );
    assert_eq!(
        refused(e.buy(&owner(2), &req(2, 14), 999, &p.hash(), 2000)),
        Refusal::ListingGone
    );
    let _ = before;
}

#[test]
fn expiry_returns_item() {
    let mut e = market();
    seller_and_buyer(&mut e);
    let listing = post_item(&mut e, 1, 2, 1, price(10, vec![]))
        .unwrap()
        .id
        .unwrap();
    put_stack(&mut e, 1, 3, "r01", 4).unwrap();
    let stacked = e
        .post(
            &owner(1),
            &req(1, 4),
            &Post {
                seller: "Seller".into(),
                goods: stack("r01", 3),
                mode: ListingMode::Price,
                price: Some(price(5, vec![])),
                days: 1,
            },
            1000,
        )
        .unwrap()
        .id
        .unwrap();
    assert_eq!(e.stash_stacks(&owner(1)).unwrap(), vec![("r01".into(), 1)]);
    let before = totals(&e);
    assert_eq!(e.expire(1000 + DAY - 1).unwrap(), 0);
    assert_eq!(e.expire(1000 + DAY).unwrap(), 1);
    assert_eq!(delivered(&e, 1), [("expired".into(), stack("r01", 3))]);
    assert_eq!(e.expire(1000 + 7 * DAY).unwrap(), 1);
    assert_eq!(
        e.listing(listing).unwrap().unwrap().state,
        ListingState::Expired
    );
    assert_eq!(
        e.listing(stacked).unwrap().unwrap().state,
        ListingState::Expired
    );
    assert_eq!(delivered(&e, 1).len(), 2);
    assert_eq!(totals(&e), before);
    // Claimed back into the stash.
    for d in e.deliveries(&owner(1)).unwrap() {
        e.claim(&owner(1), &req(1, 10 + d.id as u64), d.id, 9 * DAY)
            .unwrap();
    }
    assert!(e.deliveries(&owner(1)).unwrap().is_empty());
    assert_eq!(e.stash_stacks(&owner(1)).unwrap(), vec![("r01".into(), 4)]);
    assert_eq!(
        e.stash_items(&owner(1), &StashQuery::default())
            .unwrap()
            .total,
        1
    );
    assert_eq!(totals(&e), before);
}

#[test]
fn cancel_returns_item() {
    let mut e = market();
    seller_and_buyer(&mut e);
    let listing = post_item(&mut e, 1, 2, 1, price(10, vec![]))
        .unwrap()
        .id
        .unwrap();
    assert_eq!(
        refused(e.cancel(&owner(2), &req(2, 9), listing, 2000)),
        Refusal::ListingGone,
        "not the seller's"
    );
    e.cancel(&owner(1), &req(1, 3), listing, 2000).unwrap();
    assert_eq!(
        delivered(&e, 1),
        [("cancelled".into(), Goods::Item(ItemId(1)))]
    );
    assert_eq!(
        refused(e.cancel(&owner(1), &req(1, 4), listing, 2000)),
        Refusal::ListingGone
    );
    assert_eq!(
        refused(e.buy(
            &owner(2),
            &req(2, 5),
            listing,
            &price(10, vec![]).hash(),
            2000
        )),
        Refusal::ListingGone
    );
    let kinds: Vec<String> = e
        .item_history(ItemId(1))
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect();
    assert_eq!(kinds, ["put", "list", "cancelled"]);
}

#[test]
fn fee_taken() {
    let mut e = Economy::in_memory(
        MOD,
        EconomyConfig {
            market_fee_pct: 10,
            market_listing_fee: 100,
            ..market_config()
        },
    )
    .unwrap();
    seller_and_buyer(&mut e);
    let p = price(1005, vec![ber(1)]);
    // No gold for the listing fee.
    assert_eq!(
        refused(post_item(&mut e, 1, 2, 1, p.clone())),
        Refusal::NotEnough
    );
    put_gold(&mut e, 1, 3, 150).unwrap();
    let listing = post_item(&mut e, 1, 4, 1, p.clone()).unwrap().id.unwrap();
    assert_eq!(e.stash_gold(&owner(1)).unwrap(), 50);
    let before = totals(&e);
    e.buy(&owner(2), &req(2, 3), listing, &p.hash(), 2000)
        .unwrap();
    // 10% of 1,005 is 100 (rounded down); counts carry no fee.
    assert_eq!(
        delivered(&e, 1),
        [("sold".into(), gold(905)), ("sold".into(), stack("r30", 1))]
    );
    assert_eq!(totals(&e).gold, before.gold - 100);
    let fees: i64 = e
        .conn()
        .query_row(
            "SELECT SUM(gold) FROM transfers WHERE to_kind = 'fees'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(fees, 200);
    let unnamed: i64 = e
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM transfers WHERE to_kind = 'fees'              AND listing IS NOT ?1",
            [listing],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unnamed, 0, "a fee's log row names its listing");
}

#[test]
fn post_rules_and_limits() {
    let mut e = Economy::in_memory(
        MOD,
        EconomyConfig {
            market_max_listings: 3,
            market_posts_per_min: 2,
            ..market_config()
        },
    )
    .unwrap();
    for id in 1..=6 {
        put_item(&mut e, 1, u64::from(id), item(id)).unwrap();
    }
    let post = |e: &mut Economy, n: u64, goods: Goods, mode, p: Option<Price>, days, now| {
        e.post(
            &owner(1),
            &req(1, n),
            &Post {
                seller: "Seller".into(),
                goods,
                mode,
                price: p,
                days,
            },
            now,
        )
    };
    let ok_price = || Some(price(10, vec![]));
    let one = || Goods::Item(ItemId(1));
    use ListingMode::{OffersOnly, Price as Fixed, PriceAndOffers};
    assert_eq!(
        refused(post(&mut e, 10, one(), Fixed, None, 7, 0)),
        Refusal::BadRequest
    );
    assert_eq!(
        refused(post(&mut e, 11, one(), OffersOnly, ok_price(), 7, 0)),
        Refusal::BadRequest
    );
    assert_eq!(
        refused(post(&mut e, 12, one(), Fixed, ok_price(), 0, 0)),
        Refusal::BadRequest
    );
    assert_eq!(
        refused(post(&mut e, 13, one(), Fixed, ok_price(), 8, 0)),
        Refusal::BadRequest
    );
    assert_eq!(
        refused(post(&mut e, 14, one(), Fixed, Some(price(0, vec![])), 7, 0)),
        Refusal::BadRequest,
        "an empty price"
    );
    assert_eq!(
        refused(post(
            &mut e,
            15,
            one(),
            Fixed,
            Some(price(1, vec![("xyz".into(), 1)])),
            7,
            0
        )),
        Refusal::BadRequest,
        "not a currency"
    );
    let with_item = Price {
        gold: 1,
        stacks: vec![],
        items: vec![ItemId(2)],
    };
    assert_eq!(
        refused(post(&mut e, 16, one(), Fixed, Some(with_item), 7, 0)),
        Refusal::BadRequest,
        "items only in offers"
    );
    assert_eq!(
        refused(post(&mut e, 17, gold(5), Fixed, ok_price(), 7, 0)),
        Refusal::BadRequest
    );
    assert_eq!(
        refused(post(
            &mut e,
            18,
            Goods::Item(ItemId(99)),
            Fixed,
            ok_price(),
            7,
            0
        )),
        Refusal::ItemNotFound
    );
    // Posts per minute, then active listings.
    post(&mut e, 20, one(), Fixed, ok_price(), 7, 0).unwrap();
    post(&mut e, 21, Goods::Item(ItemId(2)), OffersOnly, None, 7, 0).unwrap();
    assert_eq!(
        refused(post(
            &mut e,
            22,
            Goods::Item(ItemId(3)),
            Fixed,
            ok_price(),
            7,
            1
        )),
        Refusal::RateLimited
    );
    post(
        &mut e,
        23,
        Goods::Item(ItemId(3)),
        PriceAndOffers,
        ok_price(),
        7,
        60_001,
    )
    .unwrap();
    assert_eq!(
        refused(post(
            &mut e,
            24,
            Goods::Item(ItemId(4)),
            Fixed,
            ok_price(),
            7,
            200_000
        )),
        Refusal::MarketLimit
    );
    // A listed item is held: it can't be taken or listed again.
    assert_eq!(
        refused(e.take(&owner(1), &req(1, 25), &chr("Hero", 1), one(), 0)),
        Refusal::ItemHeld
    );
    // Offers-only listings can't be bought.
    put_gold(&mut e, 2, 1, 100).unwrap();
    let l = e.my_listings(&owner(1), 10).unwrap();
    assert_eq!(l.len(), 3);
    let offers_only = l.iter().find(|l| l.mode == OffersOnly).unwrap();
    assert_eq!(
        refused(e.buy(&owner(2), &req(2, 2), offers_only.id, &[0; 32], 0)),
        Refusal::BadRequest
    );
}

#[test]
fn market_off_refuses() {
    let mut e = Economy::in_memory(
        MOD,
        EconomyConfig {
            market: false,
            ..config()
        },
    )
    .unwrap();
    put_item(&mut e, 1, 1, item(1)).unwrap();
    assert_eq!(
        refused(post_item(&mut e, 1, 2, 1, price(1, vec![]))),
        Refusal::MarketOff
    );
    assert_eq!(
        refused(e.buy(&owner(2), &req(2, 1), 1, &[0; 32], 0)),
        Refusal::MarketOff
    );
}

#[test]
fn claim_respects_stash_limits() {
    let mut e = Economy::in_memory(
        MOD,
        EconomyConfig {
            stash_items: 1,
            stash_gold_max: 100,
            ..market_config()
        },
    )
    .unwrap();
    put_item(&mut e, 1, 1, item(1)).unwrap();
    put_gold(&mut e, 2, 1, 100).unwrap();
    let p = price(100, vec![]);
    let listing = post_item(&mut e, 1, 2, 1, p.clone()).unwrap().id.unwrap();
    put_item(&mut e, 1, 3, item(2)).unwrap();
    put_item(&mut e, 2, 2, item(3)).unwrap();
    e.buy(&owner(2), &req(2, 3), listing, &p.hash(), 2000)
        .unwrap();
    put_gold(&mut e, 1, 4, 100).unwrap();
    // The seller's stash is at its gold cap, the buyer's at its item cap.
    let d1 = e.deliveries(&owner(1)).unwrap()[0].id;
    let d2 = e.deliveries(&owner(2)).unwrap()[0].id;
    assert_eq!(
        refused(e.claim(&owner(1), &req(1, 5), d1, 3000)),
        Refusal::StashFull
    );
    assert_eq!(
        refused(e.claim(&owner(2), &req(2, 4), d2, 3000)),
        Refusal::StashFull
    );
    assert_eq!(
        refused(e.claim(&owner(2), &req(2, 5), d1, 3000)),
        Refusal::ItemNotFound
    );
    assert_eq!(e.deliveries(&owner(1)).unwrap().len(), 1, "still there");
}

#[test]
fn take_delivery_to_character() {
    let mut e = market();
    seller_and_buyer(&mut e);
    let p = price(100, vec![]);
    let listing = post_item(&mut e, 1, 2, 1, p.clone()).unwrap().id.unwrap();
    e.buy(&owner(2), &req(2, 3), listing, &p.hash(), 2000)
        .unwrap();
    let d = e.deliveries(&owner(2)).unwrap()[0].clone();
    let before = totals(&e);
    let hero = chr("Buyer", 40);
    let t = e
        .take_delivery(&owner(2), &req(2, 4), &hero, d.id, 3000)
        .unwrap()
        .id
        .unwrap();
    assert!(e.deliveries(&owner(2)).unwrap().is_empty());
    assert_eq!(e.in_flight(t).unwrap().unwrap().item.unwrap(), item(1));
    assert_eq!(totals(&e), before);
    // The game is lost: the delivery comes back, same ID and source.
    e.undo_lease(40, 3100).unwrap();
    let back = e.deliveries(&owner(2)).unwrap();
    assert_eq!(back.len(), 1);
    assert_eq!((back[0].id, back[0].source.as_str()), (d.id, "bought"));
    assert_eq!(totals(&e), before);
    // Taken again and placed: the item leaves the economy.
    let t = e
        .take_delivery(&owner(2), &req(2, 5), &hero, d.id, 3200)
        .unwrap()
        .id
        .unwrap();
    e.take_done(
        &Req::lease(40, 1),
        40,
        t,
        Placed::Placed,
        Some(&save(3)),
        3300,
    )
    .unwrap();
    assert!(!totals(&e).items.contains(&1));
    let kinds: Vec<String> = e
        .item_history(ItemId(1))
        .unwrap()
        .into_iter()
        .map(|t| t.kind)
        .collect();
    assert_eq!(
        kinds,
        [
            "put",
            "list",
            "bought",
            "take",
            "take_undo",
            "take",
            "take_done"
        ]
    );
}

#[test]
fn price_bytes_and_hash() {
    let p = Price {
        gold: 7,
        stacks: vec![
            ("r31".into(), 1),
            ("r01".into(), 2),
            ("r31".into(), 3),
            ("gpw".into(), 0),
        ],
        items: vec![ItemId(9), ItemId(3)],
    };
    let n = p.normalized().unwrap();
    assert_eq!(n.stacks, vec![("r01".into(), 2), ("r31".into(), 4)]);
    assert_eq!(n.items, vec![ItemId(3), ItemId(9)]);
    assert_eq!(p.hash(), n.hash(), "one price, one hash");
    assert_eq!(Price::decode(&p.encode()).unwrap(), n);
    assert_ne!(n.hash(), Price::gold(7).hash());
    let bytes = n.encode();
    for cut in 0..bytes.len() {
        assert_eq!(Price::decode(&bytes[..cut]), None);
    }
    let mut long = bytes.clone();
    long.push(0);
    assert_eq!(Price::decode(&long), None);
    let twice = Price {
        items: vec![ItemId(1), ItemId(1)],
        ..Price::default()
    };
    assert_eq!(twice.normalized(), Err(Refusal::BadRequest));
}

#[test]
fn browse_newest_first_by_type() {
    let mut e = market();
    seller_and_buyer(&mut e);
    put_item(&mut e, 1, 3, item(2)).unwrap();
    let mut amulet = item(3);
    amulet.facts.item_type = "amul".into();
    put_item(&mut e, 1, 4, amulet).unwrap();
    put_stack(&mut e, 1, 5, "r30", 5).unwrap();
    for (n, id) in [(10, 1), (11, 2), (12, 3)] {
        post_item(&mut e, 1, n, id, Price::gold(100)).unwrap();
    }
    e.post(
        &owner(1),
        &req(1, 13),
        &Post {
            seller: "Seller".into(),
            goods: stack("r30", 2),
            mode: ListingMode::Price,
            price: Some(Price::gold(100)),
            days: 7,
        },
        1000,
    )
    .unwrap();
    let ids = |e: &Economy, kind: Option<&str>, page: u16| -> (u32, Vec<i64>) {
        let p = e.browse(&EconomyKey::new(MOD), kind, page).unwrap();
        (p.total, p.listings.iter().map(|l| l.id).collect())
    };
    assert_eq!(ids(&e, None, 0), (4, vec![4, 3, 2, 1]));
    assert_eq!(ids(&e, Some("ring"), 0), (2, vec![2, 1]));
    assert_eq!(ids(&e, Some("amul"), 0), (1, vec![3]));
    assert_eq!(ids(&e, Some("r30"), 0), (1, vec![4]));
    assert_eq!(ids(&e, None, 1), (4, vec![]));
    let l = e.browse(&EconomyKey::new(MOD), Some("ring"), 0).unwrap();
    assert_eq!(l.listings[0].item.as_ref().unwrap().id, ItemId(2));
    // Sold listings leave the list; other economies have their own.
    e.buy(&owner(2), &req(2, 10), 2, &Price::gold(100).hash(), 2000)
        .unwrap();
    assert_eq!(ids(&e, None, 0), (3, vec![4, 3, 1]));
    let hc = EconomyKey {
        hardcore: true,
        ..EconomyKey::new(MOD)
    };
    assert_eq!(e.browse(&hc, None, 0).unwrap().total, 0);
}
