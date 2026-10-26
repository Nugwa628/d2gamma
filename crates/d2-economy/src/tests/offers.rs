//! Offers, counter-offers and barter (N31).

use super::market::{ber, delivered, gold, market, refused, seller_and_buyer, stack, DAY};
use super::*;

/// Seller 1's item 1 listed in `mode` (listing 1, priced 1,000 gold when
/// it has a price); buyer 2 holds 200,000 gold, 5 Ber and items 5 and 6.
pub(crate) fn offers_listing(e: &mut Economy, mode: ListingMode) {
    seller_and_buyer(e);
    put_item(e, 2, 3, item(5)).unwrap();
    put_item(e, 2, 4, item(6)).unwrap();
    let l = e
        .post(
            &owner(1),
            &req(1, 2),
            &Post {
                seller: "Seller".into(),
                goods: Goods::Item(ItemId(1)),
                mode,
                price: mode.has_price().then(|| Price::gold(1000)),
                days: 7,
            },
            1000,
        )
        .unwrap();
    assert_eq!(l.id, Some(1));
}

pub(crate) fn bundle(gold: u64, stacks: Vec<(String, u32)>, items: &[u32]) -> Price {
    Price {
        gold,
        stacks,
        items: items.iter().map(|i| ItemId(*i)).collect(),
    }
}

fn make_offer(e: &mut Economy, account: i64, n: u64, p: &Price) -> Result<Done, Error> {
    e.offer(&owner(account), &req(account, n), 1, "Buyer", p, 2000)
}

fn answer(
    e: &mut Economy,
    account: i64,
    n: u64,
    offer: i64,
    how: OfferAnswer,
) -> Result<Done, Error> {
    e.answer(&owner(account), &req(account, n), offer, &how, 3000)
}

fn item_goods(id: u32) -> Goods {
    Goods::Item(ItemId(id))
}

/// Deliveries as [`delivered`] lists them.
fn sorted(v: Vec<(&str, Goods)>) -> Vec<(String, Goods)> {
    let mut d: Vec<(String, Goods)> = v.into_iter().map(|(s, g)| (s.to_owned(), g)).collect();
    d.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    d
}

fn state(e: &Economy, offer: i64) -> OfferState {
    e.find_offer(offer).unwrap().unwrap().state
}

/// Rows still held in any escrow.
fn in_escrow(e: &Economy) -> i64 {
    e.conn()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM escrow) + (SELECT COUNT(*) FROM escrow_stacks) \
             + (SELECT COUNT(*) FROM econ_items WHERE place IN ('offer', 'listing'))",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn accept_offer_is_one_transaction() {
    let mut e = market();
    offers_listing(&mut e, ListingMode::OffersOnly);
    put_gold(&mut e, 3, 1, 500).unwrap();
    let before = totals(&e);
    let o1 = make_offer(&mut e, 2, 10, &bundle(100, vec![ber(1)], &[]))
        .unwrap()
        .id
        .unwrap();
    let o2 = make_offer(&mut e, 3, 10, &bundle(300, vec![], &[]))
        .unwrap()
        .id
        .unwrap();
    // The bundles left the stashes for escrow.
    assert_eq!(e.stash_gold(&owner(2)).unwrap(), 200_000 - 100);
    assert_eq!(e.stash_gold(&owner(3)).unwrap(), 200);
    assert_eq!(totals(&e), before);
    assert_eq!(
        e.offers_to_me(&owner(1))
            .unwrap()
            .iter()
            .map(|o| o.id)
            .collect::<Vec<_>>(),
        [o2, o1]
    );

    let d = answer(&mut e, 1, 10, o1, OfferAnswer::Accept).unwrap();
    assert_eq!(d.id, Some(o1));
    // In the same transaction: the bundle to the seller, the item to the
    // buyer, the listing sold and the other offer returned.
    assert_eq!(
        delivered(&e, 1),
        sorted(vec![("sold", gold(100)), ("sold", stack("r30", 1))])
    );
    assert_eq!(delivered(&e, 2), sorted(vec![("bought", item_goods(1))]));
    assert_eq!(delivered(&e, 3), sorted(vec![("returned", gold(300))]));
    assert_eq!(state(&e, o1), OfferState::Accepted);
    assert_eq!(state(&e, o2), OfferState::Returned);
    assert_eq!(e.listing(1).unwrap().unwrap().state, ListingState::Sold);
    assert_eq!(in_escrow(&e), 0);
    assert_eq!(totals(&e), before);
    assert!(e.offers_to_me(&owner(1)).unwrap().is_empty());
    // Answers to an offer that is no longer live.
    assert_eq!(
        refused(answer(&mut e, 1, 11, o2, OfferAnswer::Accept)),
        Refusal::OfferGone
    );
    assert_eq!(
        refused(answer(&mut e, 2, 11, o1, OfferAnswer::Withdraw)),
        Refusal::OfferGone
    );
    // The repeated request gets the first answer.
    assert!(
        answer(&mut e, 1, 10, o1, OfferAnswer::Accept)
            .unwrap()
            .repeated
    );
}

#[test]
fn counter_then_accept() {
    let mut e = market();
    offers_listing(&mut e, ListingMode::OffersOnly);
    let o = make_offer(&mut e, 2, 10, &bundle(100, vec![ber(2)], &[5, 6]))
        .unwrap()
        .id
        .unwrap();
    // The buyer can't accept their own open offer; a counter can only
    // keep offered items.
    assert_eq!(
        refused(answer(&mut e, 2, 11, o, OfferAnswer::Accept)),
        Refusal::OfferGone
    );
    assert_eq!(
        refused(answer(
            &mut e,
            1,
            10,
            o,
            OfferAnswer::Counter(bundle(150, vec![], &[1]))
        )),
        Refusal::BadRequest
    );
    answer(
        &mut e,
        1,
        11,
        o,
        OfferAnswer::Counter(bundle(150, vec![ber(1)], &[5])),
    )
    .unwrap();
    let got = e.find_offer(o).unwrap().unwrap();
    assert_eq!(got.state, OfferState::Countered);
    assert_eq!(got.counter, Some(bundle(150, vec![ber(1)], &[5])));
    // A counter is final: no second counter, and the seller can't accept
    // the original any more.
    assert_eq!(
        refused(answer(
            &mut e,
            1,
            12,
            o,
            OfferAnswer::Counter(bundle(1, vec![], &[]))
        )),
        Refusal::OfferGone
    );
    assert_eq!(
        refused(answer(&mut e, 1, 13, o, OfferAnswer::Accept)),
        Refusal::OfferGone
    );

    let before = totals(&e);
    answer(&mut e, 2, 12, o, OfferAnswer::Accept).unwrap();
    // The seller gets the counter: 50 more gold came from the buyer's
    // stash; the Ber and the item not kept went back to the buyer.
    assert_eq!(
        delivered(&e, 1),
        sorted(vec![
            ("sold", gold(150)),
            ("sold", stack("r30", 1)),
            ("sold", item_goods(5)),
        ])
    );
    assert_eq!(
        delivered(&e, 2),
        sorted(vec![
            ("bought", item_goods(1)),
            ("returned", stack("r30", 1)),
            ("returned", item_goods(6)),
        ])
    );
    assert_eq!(e.stash_gold(&owner(2)).unwrap(), 200_000 - 150);
    assert_eq!(e.stash_stacks(&owner(2)).unwrap(), [("r30".into(), 3)]);
    assert_eq!(state(&e, o), OfferState::Accepted);
    assert_eq!(in_escrow(&e), 0);
    assert_eq!(totals(&e), before);
}

#[test]
fn counter_for_less_returns_the_rest() {
    let mut e = market();
    offers_listing(&mut e, ListingMode::OffersOnly);
    let o = make_offer(&mut e, 2, 10, &bundle(1000, vec![], &[]))
        .unwrap()
        .id
        .unwrap();
    answer(
        &mut e,
        1,
        10,
        o,
        OfferAnswer::Counter(bundle(600, vec![ber(1)], &[])),
    )
    .unwrap();
    answer(&mut e, 2, 11, o, OfferAnswer::Accept).unwrap();
    assert_eq!(
        delivered(&e, 1),
        sorted(vec![("sold", gold(600)), ("sold", stack("r30", 1))])
    );
    assert_eq!(
        delivered(&e, 2),
        sorted(vec![("bought", item_goods(1)), ("returned", gold(400))])
    );
    assert_eq!(e.stash_stacks(&owner(2)).unwrap(), [("r30".into(), 4)]);
}

#[test]
fn counter_buyer_cannot_pay_is_refused() {
    let mut e = market();
    offers_listing(&mut e, ListingMode::OffersOnly);
    let o = make_offer(&mut e, 2, 10, &bundle(100, vec![], &[]))
        .unwrap()
        .id
        .unwrap();
    answer(
        &mut e,
        1,
        10,
        o,
        OfferAnswer::Counter(bundle(100, vec![ber(9)], &[])),
    )
    .unwrap();
    let before = totals(&e);
    assert_eq!(
        refused(answer(&mut e, 2, 11, o, OfferAnswer::Accept)),
        Refusal::NotEnough
    );
    assert_eq!(state(&e, o), OfferState::Countered);
    assert_eq!(e.listing(1).unwrap().unwrap().state, ListingState::Active);
    assert_eq!(totals(&e), before);
}

#[test]
fn withdrawn_offer_returns_goods() {
    let mut e = market();
    offers_listing(&mut e, ListingMode::PriceAndOffers);
    let o = make_offer(&mut e, 2, 10, &bundle(500, vec![ber(2)], &[5]))
        .unwrap()
        .id
        .unwrap();
    let before = totals(&e);
    // Only the buyer withdraws.
    for account in [1, 9] {
        assert_eq!(
            refused(answer(&mut e, account, 10, o, OfferAnswer::Withdraw)),
            Refusal::OfferGone
        );
    }
    answer(&mut e, 2, 11, o, OfferAnswer::Withdraw).unwrap();
    assert_eq!(
        delivered(&e, 2),
        sorted(vec![
            ("withdrawn", gold(500)),
            ("withdrawn", stack("r30", 2)),
            ("withdrawn", item_goods(5)),
        ])
    );
    assert_eq!(state(&e, o), OfferState::Withdrawn);
    assert_eq!(e.listing(1).unwrap().unwrap().state, ListingState::Active);
    assert_eq!(in_escrow(&e), 1, "the listing's item");
    assert_eq!(totals(&e), before);

    // A new offer on the same listing, declined by the seller.
    let o2 = make_offer(&mut e, 2, 12, &bundle(10, vec![], &[]))
        .unwrap()
        .id
        .unwrap();
    assert_eq!(
        refused(answer(&mut e, 2, 13, o2, OfferAnswer::Decline)),
        Refusal::OfferGone
    );
    answer(&mut e, 1, 11, o2, OfferAnswer::Decline).unwrap();
    assert_eq!(state(&e, o2), OfferState::Declined);
    assert!(delivered(&e, 2).contains(&("declined".into(), gold(10))));
    assert_eq!(totals(&e), before);
}

#[test]
fn offer_with_items_moves_both_ways() {
    let mut e = market();
    offers_listing(&mut e, ListingMode::OffersOnly);
    let o = make_offer(&mut e, 2, 10, &bundle(0, vec![], &[5, 6]))
        .unwrap()
        .id
        .unwrap();
    // The offered items left the buyer's stash.
    let q = StashQuery::default();
    assert_eq!(e.stash_items(&owner(2), &q).unwrap().total, 0);
    answer(&mut e, 1, 10, o, OfferAnswer::Accept).unwrap();
    assert_eq!(
        delivered(&e, 1),
        sorted(vec![("sold", item_goods(5)), ("sold", item_goods(6))])
    );
    assert_eq!(delivered(&e, 2), sorted(vec![("bought", item_goods(1))]));
    let kinds = |id| -> Vec<String> {
        e.item_history(ItemId(id))
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    };
    assert_eq!(kinds(5), ["put", "offer", "sold"]);
    assert_eq!(kinds(1), ["put", "list", "bought"]);
    // Both sides claim what they got.
    let d1: Vec<i64> = e
        .deliveries(&owner(1))
        .unwrap()
        .iter()
        .map(|d| d.id)
        .collect();
    for (n, d) in d1.into_iter().enumerate() {
        e.claim(&owner(1), &req(1, 20 + n as u64), d, 4000).unwrap();
    }
    let d2 = e.deliveries(&owner(2)).unwrap()[0].id;
    e.claim(&owner(2), &req(2, 20), d2, 4000).unwrap();
    let ids = |a| -> Vec<u32> {
        e.stash_items(&owner(a), &StashQuery::default())
            .unwrap()
            .items
            .iter()
            .map(|s| s.item.id.0)
            .collect()
    };
    assert_eq!((ids(1), ids(2)), (vec![5, 6], vec![1]));
    // Every move after the puts names its listing, and the offer's moves
    // name the offer too: the audit tools follow a trade through them.
    let mut st = e
        .conn()
        .prepare("SELECT kind, listing, offer FROM transfers WHERE kind <> 'put' ORDER BY id")
        .unwrap();
    let rows: Vec<(String, Option<i64>, Option<i64>)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let kinds: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(
        kinds,
        ["list", "offer", "offer", "sold", "sold", "bought", "claim", "claim", "claim"]
    );
    for (kind, listing, offer) in &rows {
        assert_eq!(*listing, Some(1), "{kind}");
        if kind != "list" {
            assert_eq!(*offer, Some(o), "{kind}");
        }
    }
}

#[test]
fn offers_returned_when_listing_closes() {
    // Sold at its price to another buyer.
    let mut e = market();
    offers_listing(&mut e, ListingMode::PriceAndOffers);
    put_gold(&mut e, 3, 1, 1000).unwrap();
    let o = make_offer(&mut e, 2, 10, &bundle(700, vec![], &[5]))
        .unwrap()
        .id
        .unwrap();
    e.buy(&owner(3), &req(3, 2), 1, &Price::gold(1000).hash(), 2500)
        .unwrap();
    assert_eq!(state(&e, o), OfferState::Returned);
    assert_eq!(
        delivered(&e, 2),
        sorted(vec![("returned", gold(700)), ("returned", item_goods(5))])
    );

    // Cancelled, and expired.
    for close in ["cancel", "expire"] {
        let mut e = market();
        offers_listing(&mut e, ListingMode::OffersOnly);
        let o = make_offer(&mut e, 2, 10, &bundle(5, vec![], &[]))
            .unwrap()
            .id
            .unwrap();
        let before = totals(&e);
        if close == "cancel" {
            e.cancel(&owner(1), &req(1, 10), 1, 2500).unwrap();
        } else {
            assert_eq!(e.expire(1000 + 7 * DAY).unwrap(), 1);
        }
        assert_eq!(state(&e, o), OfferState::Returned, "{close}");
        assert_eq!(delivered(&e, 2), sorted(vec![("returned", gold(5))]));
        assert_eq!(in_escrow(&e), 0);
        assert_eq!(totals(&e), before);
    }
}

#[test]
fn offer_rules_and_limits() {
    let mut e = Economy::in_memory(
        MOD,
        EconomyConfig {
            market_max_offers: 2,
            ..config()
        },
    )
    .unwrap();
    offers_listing(&mut e, ListingMode::OffersOnly);
    for (n, id) in [(5, 2), (6, 3), (7, 4)] {
        put_item(&mut e, 1, n, item(id)).unwrap();
        e.post(
            &owner(1),
            &req(1, n + 10),
            &Post {
                seller: "Seller".into(),
                goods: Goods::Item(ItemId(id)),
                mode: if id == 4 {
                    ListingMode::Price
                } else {
                    ListingMode::OffersOnly
                },
                price: (id == 4).then(|| Price::gold(5)),
                days: 7,
            },
            1000,
        )
        .unwrap();
    }
    let before = totals(&e);
    let offer = |e: &mut Economy, account: i64, n: u64, listing: i64, p: Price| {
        e.offer(
            &owner(account),
            &req(account, n),
            listing,
            "Buyer",
            &p,
            2000,
        )
    };
    let cases: Vec<(i64, i64, Price, Refusal, &str)> = vec![
        (
            2,
            4,
            Price::gold(1),
            Refusal::BadRequest,
            "a listing without offers",
        ),
        (
            1,
            1,
            Price::gold(1),
            Refusal::BadRequest,
            "the seller's own listing",
        ),
        (
            2,
            9,
            Price::gold(1),
            Refusal::ListingGone,
            "no such listing",
        ),
        (
            2,
            1,
            Price::default(),
            Refusal::BadRequest,
            "an empty bundle",
        ),
        (
            2,
            1,
            Price::gold(300_000),
            Refusal::NotEnough,
            "more gold than held",
        ),
        (
            2,
            1,
            bundle(0, vec![ber(6)], &[]),
            Refusal::NotEnough,
            "more Ber",
        ),
        (
            2,
            1,
            bundle(0, vec![("xyz".into(), 1)], &[]),
            Refusal::BadRequest,
            "not a currency",
        ),
        (
            2,
            1,
            bundle(1, vec![], &[1]),
            Refusal::ItemNotFound,
            "the seller's item",
        ),
    ];
    for (n, (account, listing, p, why, what)) in cases.into_iter().enumerate() {
        let r = offer(&mut e, account, 100 + n as u64, listing, p);
        assert_eq!(refused(r), why, "{what}");
    }
    assert_eq!(totals(&e), before);
    assert_eq!(in_escrow(&e), 4, "the listings' items only");

    offer(&mut e, 2, 10, 1, Price::gold(1)).unwrap();
    assert_eq!(
        refused(offer(&mut e, 2, 11, 1, Price::gold(2))),
        Refusal::MarketLimit,
        "one live offer per listing"
    );
    offer(&mut e, 2, 12, 2, Price::gold(1)).unwrap();
    assert_eq!(
        refused(offer(&mut e, 2, 13, 3, Price::gold(1))),
        Refusal::MarketLimit,
        "market_max_offers"
    );
    assert_eq!(
        e.my_offers(&owner(2), 10)
            .unwrap()
            .iter()
            .map(|o| o.listing)
            .collect::<Vec<_>>(),
        [2, 1]
    );
}

/// The reasons of an account's deliveries, oldest first.
fn reasons(e: &Economy, account: i64) -> Vec<Reason> {
    let mut rows = e.deliveries_page(&owner(account), 0).unwrap().rows;
    rows.reverse();
    rows.into_iter().map(|r| r.reason).collect()
}

#[test]
fn refunds_say_why() {
    // Declined, withdrawn, and outbid when someone buys the listing.
    let mut e = market();
    offers_listing(&mut e, ListingMode::PriceAndOffers);
    for a in [3, 4, 5] {
        put_gold(&mut e, a, 1, 5000).unwrap();
    }
    let o1 = make_offer(&mut e, 2, 10, &bundle(100, vec![], &[]))
        .unwrap()
        .id
        .unwrap();
    let o2 = make_offer(&mut e, 3, 10, &bundle(200, vec![], &[]))
        .unwrap()
        .id
        .unwrap();
    make_offer(&mut e, 4, 10, &bundle(300, vec![], &[])).unwrap();
    answer(&mut e, 1, 20, o1, OfferAnswer::Decline).unwrap();
    answer(&mut e, 3, 20, o2, OfferAnswer::Withdraw).unwrap();
    e.buy(&owner(5), &req(5, 10), 1, &Price::gold(1000).hash(), 4000)
        .unwrap();
    assert_eq!(reasons(&e, 2), [Reason::Declined]);
    assert_eq!(reasons(&e, 3), [Reason::Withdrawn]);
    assert_eq!(reasons(&e, 4), [Reason::Outbid]);
    assert_eq!(reasons(&e, 5), [Reason::None], "bought");
    assert_eq!(reasons(&e, 1), [Reason::None], "proceeds");

    // Cancelled: the seller's goods and the open offer's.
    let mut e = market();
    offers_listing(&mut e, ListingMode::OffersOnly);
    make_offer(&mut e, 2, 10, &bundle(100, vec![], &[])).unwrap();
    e.cancel(&owner(1), &req(1, 20), 1, 3000).unwrap();
    assert_eq!(reasons(&e, 1), [Reason::Cancelled]);
    assert_eq!(reasons(&e, 2), [Reason::Cancelled]);

    // Expired, both sides.
    let mut e = market();
    offers_listing(&mut e, ListingMode::OffersOnly);
    make_offer(&mut e, 2, 10, &bundle(100, vec![], &[])).unwrap();
    e.expire(8 * DAY).unwrap();
    assert_eq!(reasons(&e, 1), [Reason::Expired]);
    assert_eq!(reasons(&e, 2), [Reason::Expired]);
}
