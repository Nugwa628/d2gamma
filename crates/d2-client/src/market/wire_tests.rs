//! The window's asks go out as the contract's messages, and the realm's
//! answers come in with every field where the window reads it.

use super::*;
use crate::market::source::Post;

/// Reads a record `[q, ...]` as an item of group `q` named "Item q"; any
/// other record does not read.
fn read_one(record: &[u8]) -> Option<(Group, ItemView)> {
    let (&q, _) = record.split_first()?;
    Some((
        Group::of_code(q)?,
        ItemView {
            name: format!("Item {q}"),
            identified: true,
            ..ItemView::default()
        },
    ))
}

fn stack_name(code: &str) -> Option<String> {
    (code == "r24").then(|| "Rune 24".to_owned())
}

const READ: Reading<'static> = Reading {
    item: &read_one,
    name: &stack_name,
};

fn gold(g: u64) -> Price {
    Price::gold(g)
}

/// Every ask encodes and decodes as the wire's message for it.
#[test]
fn asks_go_out_as_the_contract_s_messages() {
    let q = MarketQuery {
        text: "crown".into(),
        groups: vec![Group::Unique, Group::Stackable],
        ethereal: Tri::No,
        stats: vec![(7, 10, 99)],
        price_mode: PriceMode::RunesOnly,
        sort: MarketSort::Stat,
        sort_stat: 7,
        desc: true,
        page: 2,
        page_size: 50,
        facets: true,
        ..MarketQuery::default()
    };
    let asks = vec![
        MarketAsk::Hello,
        MarketAsk::Search(q),
        MarketAsk::ListingGet(9),
        MarketAsk::Buy {
            listing: 9,
            price_hash: [3; 32],
            fee_rev: 2,
        },
        MarketAsk::Offer {
            listing: 9,
            price: Price {
                gold: 5,
                stacks: vec![("r24".into(), 2)],
                items: vec![41],
            },
            fee_rev: 2,
        },
        MarketAsk::Answer {
            offer: 4,
            how: AnswerHow::Counter,
            counter: Some(gold(900)),
            fee_rev: 2,
        },
        MarketAsk::OffersOpen {
            which: OffersWhich::MadeByMe,
            live_only: true,
            page: 1,
        },
        MarketAsk::DeliveriesOpen { page: 3 },
        MarketAsk::Claim {
            all: false,
            deliveries: vec![1, 2],
        },
        MarketAsk::Post(Post {
            goods: PostGoods::Stack {
                code: "r24".into(),
                count: 3,
            },
            mode: ListingMode::PriceAndOffers,
            price: Some(gold(1000)),
            days: 7,
            note: "fair offers".into(),
            hidden: true,
            fee_rev: 2,
        }),
        MarketAsk::ShopEdit {
            op: ShopOp::Reprice {
                mode: ListingMode::Price,
                price: Some(gold(70)),
            },
            listings: vec![5, 6],
            fee_rev: 2,
        },
        MarketAsk::ShopEdit {
            op: ShopOp::Relist { days: 3 },
            listings: vec![5],
            fee_rev: 2,
        },
        MarketAsk::ShopEdit {
            op: ShopOp::Note("gone soon".into()),
            listings: vec![5],
            fee_rev: 2,
        },
        MarketAsk::ShopOpen {
            tab: ShopTab::Sold,
            page: 1,
            sort: ShopSort::Gold,
            desc: true,
        },
        MarketAsk::Prices {
            of: PriceOf::Stackable("r24".into()),
            days: 30,
        },
    ];
    for (n, a) in asks.iter().enumerate() {
        let m = w::ToRealm {
            req: n as u64 + 1,
            ask: ask(a),
        };
        assert_eq!(w::ToRealm::parse(&m.body()), Ok(m.clone()), "{a:?}");
    }
    // The names that differ between the two sides.
    let w::Ask::Search(wq) = ask(&asks[1]) else {
        panic!("a search");
    };
    assert_eq!(wq.price_mode, w::PriceMode::CurrenciesOnly);
    assert_eq!(wq.groups, [w::Group::Unique, w::Group::Stackable]);
    assert_eq!(wq.ethereal, w::TriState::No);
    assert_eq!(
        wq.stats,
        [w::StatRange {
            stat: 7,
            min: 10,
            max: 99
        }]
    );
    assert_eq!(
        (wq.sort.by, wq.sort.stat, wq.sort.desc),
        (w::SortBy::Stat, 7, true)
    );
    let w::Ask::ShopOpen { sort, tab, .. } = ask(&asks[13]) else {
        panic!("a shop page");
    };
    assert_eq!((sort, tab), (w::ShopSort::GoldPrice, w::ShopTab::Sold));
}

fn wire_row(listing: u64, group: w::Group, goods: w::Goods) -> w::ListingRow {
    w::ListingRow {
        listing,
        group,
        goods,
        mode: w::ListingMode::PriceAndOffers,
        price: Some(w::Price {
            gold: 100,
            ..w::Price::default()
        }),
        price_hash: [1; 32],
        seller: "seller".into(),
        seller_sales: 3,
        posted_ms: 10,
        expires_ms: 20,
        note: "note".into(),
        offers: 1,
        mine: true,
    }
}

/// The realm's state flattened as the window keeps it; its stackables
/// are the window's currencies.
#[test]
fn the_state_reads_flat() {
    let s = w::MarketState {
        req: 4,
        on: false,
        off_reason: w::OffReason::NoCharacter,
        mod_id: "m".into(),
        hardcore: true,
        ladder: 2,
        expansion: true,
        limits: w::MarketLimits {
            max_listings: 50,
            max_offers: 20,
            max_days: 14,
            max_offer_items: 4,
            searches_per_min: 60,
            posts_per_min: 10,
        },
        fees: w::Fees {
            sale_fee_pct: 5,
            listing_fee: 1,
            buyer_fee: 2,
            fee_rev: 9,
        },
        counts: w::MarketCounts {
            active_listings: 3,
            deliveries_waiting: 4,
            live_offers_to_me: 5,
            my_live_offers: 6,
        },
        last_req: 77,
        stackables: vec!["r24".into(), "gpw".into()],
    };
    let MarketEvent::State { req, state } = state(s) else {
        panic!("a state");
    };
    assert_eq!(req, 4);
    assert!(!state.on);
    assert_eq!(state.off_reason, OffReason::NoCharSelected);
    assert_eq!(
        (state.max_listings, state.max_offers, state.max_days),
        (50, 20, 14)
    );
    assert_eq!(state.max_offer_items, 4);
    assert_eq!((state.fees.sale_fee_pct, state.fees.fee_rev), (5, 9));
    assert_eq!(
        (
            state.active_listings,
            state.deliveries_waiting,
            state.live_offers_to_me,
            state.my_live_offers
        ),
        (3, 4, 5, 6)
    );
    assert_eq!(state.last_req, 77);
    assert_eq!(state.currencies, ["r24", "gpw"]);
}

/// A row keeps the realm's group and gets the client's reading of its
/// record; a stackable is named by its code.
#[test]
fn results_read_their_records() {
    let rows = vec![
        wire_row(
            1,
            w::Group::Unique,
            w::Goods::Item {
                item_id: 8,
                record: vec![1, 0xAA],
            },
        ),
        wire_row(
            2,
            w::Group::Stackable,
            w::Goods::Stack {
                code: "r24".into(),
                count: 3,
            },
        ),
        wire_row(
            3,
            w::Group::Rare,
            w::Goods::Item {
                item_id: 9,
                record: vec![],
            },
        ),
    ];
    let head = w::ResultsHead {
        total: 3,
        page: 0,
        page_size: 25,
        rows: 3,
        now_ms: 99,
        facets: true,
    };
    let facets = w::Facets {
        groups: vec![(w::Group::Unique, 1), (w::Group::Stackable, 1)],
        types: vec![("helm".into(), 2)],
        ethereal: [1, 2],
        ..w::Facets::default()
    };
    let MarketEvent::Results { req, page } = results(5, head, Some(facets), rows, &READ) else {
        panic!("results");
    };
    assert_eq!(
        (req, page.total, page.page_size, page.now_ms),
        (5, 3, 25, 99)
    );
    let r = &page.rows[0];
    // The realm's group wins over the record's reading.
    assert_eq!((r.group, r.item.name.as_str()), (Group::Unique, "Item 1"));
    assert_eq!(r.mode, ListingMode::PriceAndOffers);
    assert_eq!(r.price, Some(gold(100)));
    assert_eq!((r.seller_sales, r.offers, r.mine), (3, 1, true));
    let s = &page.rows[1];
    assert_eq!(
        (s.item.name.as_str(), s.item.code.as_str()),
        ("Rune 24", "r24")
    );
    assert_eq!(page.rows[2].item.name, UNKNOWN);
    let f = page.facets.expect("facets");
    assert_eq!(f.groups.get(&Group::Stackable), Some(&1));
    assert_eq!(f.types.get("helm"), Some(&2));
    assert_eq!(f.ethereal, [1, 2]);
}

/// The asker's offer comes with the listing.
#[test]
fn a_listing_brings_the_asker_s_offer() {
    let d = w::ListingDetail {
        state: w::ListingState::Sold,
        row: wire_row(
            1,
            w::Group::Set,
            w::Goods::Item {
                item_id: 8,
                record: vec![4],
            },
        ),
        buyer_fee: 7,
        fee_rev: 3,
        my_offer: Some(w::ListingOffer {
            offer: 12,
            offered: w::Price {
                gold: 50,
                ..w::Price::default()
            },
            counter: None,
            state: w::OfferState::Countered,
            at_ms: 5,
            answered_ms: 6,
        }),
    };
    let MarketEvent::Listing { req, detail } = listing(2, d, &READ) else {
        panic!("a listing");
    };
    assert_eq!(
        (req, detail.state, detail.buyer_fee, detail.fee_rev),
        (2, 1, 7, 3)
    );
    let o = detail.offer.expect("the offer");
    assert_eq!((o.offer, o.price.gold, o.state), (12, 50, 1));
    assert_eq!((o.made_ms, o.answered_ms), (5, 6));
}

/// My shop's rows (K3): state, hidden, the sale.
#[test]
fn shop_rows_keep_their_state_and_sale() {
    let head = w::ShopHead {
        tab: w::ShopTab::Sold,
        page: 1,
        rows: 1,
        counts: [1, 2, 3, 4],
        slots_used: 5,
        slots_max: 50,
        now_ms: 9,
    };
    let rows = vec![w::ShopRow {
        row: wire_row(
            1,
            w::Group::Stackable,
            w::Goods::Stack {
                code: "r24".into(),
                count: 1,
            },
        ),
        state: w::ListingState::Sold,
        hidden: true,
        live_offers: 2,
        newest_offer_ms: 8,
        sold: Some(w::Sold {
            buyer: "buyer".into(),
            paid: w::Price {
                gold: 90,
                ..w::Price::default()
            },
            sale_fee: 4,
            closed_ms: 7,
        }),
        relistable: false,
    }];
    let MarketEvent::Shop { req, page } = shop(3, head, rows, &READ) else {
        panic!("a shop page");
    };
    assert_eq!(req, 3);
    assert_eq!(
        (page.tab, page.page, page.counts),
        (ShopTab::Sold, 1, [1, 2, 3, 4])
    );
    assert_eq!((page.slots_used, page.slots_max, page.now_ms), (5, 50, 9));
    let r = &page.rows[0];
    assert_eq!(
        (r.state, r.hidden, r.live_offers),
        (ListingState::Sold, true, 2)
    );
    let s = r.sold.as_ref().expect("the sale");
    assert_eq!(
        (s.buyer.as_str(), s.paid.gold, s.sale_fee),
        ("buyer", 90, 4)
    );
}

/// An offer's goods get the group their record reads as; the offered
/// and countered items are named from the page's item records.
#[test]
fn offers_name_their_items() {
    let head = w::OffersHead {
        which: w::OffersWhich::OnMyListings,
        page: 0,
        rows: 1,
        total: 1,
        now_ms: 3,
    };
    let rows = vec![w::OfferRow {
        offer: 4,
        listing: 1,
        goods: w::Goods::Item {
            item_id: 8,
            record: vec![5],
        },
        price: None,
        buyer: "b".into(),
        seller: "s".into(),
        offered: w::Price {
            gold: 10,
            stacks: vec![],
            items: vec![41],
        },
        counter: Some(w::Price {
            gold: 20,
            stacks: vec![],
            items: vec![41, 42],
        }),
        state: w::OfferState::Countered,
        at_ms: 1,
        answered_ms: 2,
    }];
    let items = vec![
        w::ItemRecord {
            item_id: 41,
            record: vec![2],
        },
        w::ItemRecord {
            item_id: 42,
            record: vec![1],
        },
    ];
    let MarketEvent::Offers { page, .. } = offers(6, head, rows, items, &READ) else {
        panic!("offers");
    };
    assert_eq!(page.which, OffersWhich::OnMyListings);
    let r = &page.rows[0];
    assert_eq!((r.group, r.item.name.as_str()), (Group::Unique, "Item 5"));
    assert_eq!(r.state, OfferState::Countered);
    let named: Vec<(u32, Group)> = r.items.iter().map(|i| (i.item_id, i.group)).collect();
    assert_eq!(named, [(41, Group::Rare), (42, Group::Magic)]);
    assert_eq!(r.named(42).map(|i| i.item.name.as_str()), Some("Item 1"));
}

/// Deliveries carry the realm's source and reason (contract v2), and an
/// item delivery its reading.
#[test]
fn deliveries_carry_source_and_reason() {
    let head = w::DeliveriesHead {
        page: 0,
        rows: 3,
        total: 3,
        now_ms: 4,
    };
    let row = |delivery, source, reason, goods| w::DeliveryRow {
        delivery,
        source,
        reason,
        listing: Some(7),
        offer: Some(70),
        about: "Stormcall".into(),
        goods,
        at_ms: 1,
    };
    let rows = vec![
        row(
            1,
            w::source::RETURNED,
            w::reason::OUTBID,
            w::Goods::Gold(500),
        ),
        row(
            2,
            w::source::RETURNED,
            w::reason::WITHDRAWN,
            w::Goods::Item {
                item_id: 41,
                record: vec![6],
            },
        ),
        row(
            3,
            w::source::SOLD,
            w::reason::NONE,
            w::Goods::Stack {
                code: "r24".into(),
                count: 2,
            },
        ),
    ];
    let MarketEvent::Deliveries { page, .. } = deliveries(1, head, rows, &READ) else {
        panic!("deliveries");
    };
    let got: Vec<(DeliverySource, DeliveryReason)> =
        page.rows.iter().map(|d| (d.source, d.reason)).collect();
    assert_eq!(
        got,
        [
            (DeliverySource::Returned, DeliveryReason::Outbid),
            (DeliverySource::Returned, DeliveryReason::Withdrawn),
            (DeliverySource::Sold, DeliveryReason::None),
        ]
    );
    assert_eq!(page.rows[0].item, None);
    assert_eq!(
        page.rows[1]
            .item
            .as_ref()
            .map(|(g, v)| (*g, v.name.as_str())),
        Some((Group::Runeword, "Item 6"))
    );
    assert_eq!(page.rows[2].item, None);
}

/// A price check keeps the realm's medians now (K5) and the asked span.
#[test]
fn prices_keep_the_medians_and_the_span() {
    let head = w::PricesHead {
        key: "u:shako".into(),
        label: "Harlequin Crest".into(),
        now_ms: 10,
        bucket_ms: prices::DAY_MS,
        listed_now: 2,
        sold: 6,
        series: 1,
    };
    let series = vec![w::PriceSeries {
        index: 0,
        shape: w::PriceShape::Bundle,
        currencies: vec!["r24".into(), "r23".into()],
        sold: 6,
        listed_now: 2,
        lowest_now: vec![1, 2],
        median_now: vec![3, 4],
        buckets: vec![w::Bucket {
            index: 1,
            sales: 2,
            values: vec![w::Spread {
                min: 1,
                median: 2,
                max: 3,
            }],
        }],
    }];
    let recent = vec![w::Sale {
        at_ms: 9,
        price: w::Price {
            gold: 0,
            stacks: vec![("r24".into(), 1)],
            items: vec![],
        },
    }];
    let MarketEvent::Prices { req, history } = prices(8, head, series, recent, 30) else {
        panic!("prices");
    };
    assert_eq!(req, 8);
    assert_eq!((history.days, history.sold, history.listed_now), (30, 6, 2));
    assert!(history.enough());
    let s = &history.series[0];
    assert_eq!(s.shape, prices::PriceShape::Bundle);
    assert_eq!(
        (s.lowest_now.clone(), s.median_now.clone()),
        (vec![1, 2], vec![3, 4])
    );
    assert_eq!(s.buckets[0].values[0].median, 2);
    assert_eq!(history.recent[0].price.stacks, [("r24".to_owned(), 1)]);
}

/// A result's per-row codes, and a refusal before the market, read as
/// the request's result.
#[test]
fn results_and_refusals_read_as_done() {
    let d = w::Done {
        req: 3,
        code: ErrorCode(0),
        wait_s: 0,
        repeated: true,
        id: Some(11),
        rows: vec![(1, ErrorCode(0)), (2, ErrorCode(84))],
        text: String::new(),
    };
    let MarketEvent::Done(d) = done(d) else {
        panic!("done");
    };
    assert_eq!(
        (d.req, d.code.clone(), d.repeated, d.id),
        (3, MarketCode::Done, true, Some(11))
    );
    assert_eq!(d.rows, [(1, MarketCode::Done), (2, MarketCode::of(84))]);
    let e = ErrorMsg {
        request_kind: w::kind::SEARCH,
        code: ErrorCode::RATE_LIMITED,
        wait_s: 3,
        text: "slow down".into(),
    };
    let MarketEvent::Done(d) = error(4, e) else {
        panic!("done");
    };
    assert_eq!(
        (d.req, d.code.clone(), d.wait_s),
        (4, MarketCode::RateLimited, 3)
    );
}

/// A notice's code reads as what it tells.
#[test]
fn notices_read_their_code() {
    let n = w::Notice {
        what: w::notice::COUNTERED,
        listing: 1,
        offer: Some(2),
        about: "Stormcall".into(),
        who: "s".into(),
        price: None,
        deliveries_waiting: 3,
        at_ms: 4,
    };
    let MarketEvent::Notice(n) = notice(n) else {
        panic!("a notice");
    };
    assert_eq!(n.what, NoticeWhat::Countered);
    assert_eq!((n.offer, n.deliveries_waiting), (Some(2), 3));
}
