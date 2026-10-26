//! The market messages' round trips, their sizes against a frame, and
//! their decode rules.

use super::*;
use crate::wire::MAX_FRAME;

fn price() -> Price {
    Price {
        gold: 400_000,
        stacks: vec![("r24".into(), 2), ("r23".into(), 1)],
        items: Vec::new(),
    }
}

/// The largest price the wire allows: every currency with the longest
/// code, every item.
fn max_price() -> Price {
    Price {
        gold: u64::MAX,
        stacks: (0..MAX_CURRENCIES)
            .map(|i| (format!("{i:0>8}"), u32::MAX))
            .collect(),
        items: (0..MAX_PRICE_ITEMS as u32).collect(),
    }
}

fn item(record: usize) -> Goods {
    Goods::Item {
        item_id: 77,
        record: (0..record).map(|i| i as u8).collect(),
    }
}

fn row() -> ListingRow {
    ListingRow {
        listing: 41,
        group: Group::Unique,
        goods: item(40),
        mode: ListingMode::PriceAndOffers,
        price: Some(price()),
        price_hash: [0x5A; 32],
        seller: "Werwer".into(),
        seller_sales: 41,
        posted_ms: 1_800_000_000_000,
        expires_ms: 1_800_604_800_000,
        note: "perfect roll".into(),
        offers: 3,
        mine: false,
    }
}

/// The largest listing row: a 2,048-byte record, the longest names and
/// note, the largest price.
fn max_row() -> ListingRow {
    ListingRow {
        listing: u64::MAX,
        goods: item(MAX_RECORD),
        price: Some(max_price()),
        seller: "s".repeat(MAX_NAME),
        note: "n".repeat(MAX_NOTE),
        ..row()
    }
}

fn offer_row() -> OfferRow {
    OfferRow {
        offer: 9,
        listing: 41,
        goods: Goods::Stack {
            code: "r30".into(),
            count: 2,
        },
        price: None,
        buyer: "Ann".into(),
        seller: "Werwer".into(),
        offered: Price {
            gold: 0,
            stacks: vec![("r24".into(), 1)],
            items: vec![5, 6],
        },
        counter: Some(Price::gold(10_000)),
        state: OfferState::Countered,
        at_ms: 5,
        answered_ms: 6,
    }
}

fn max_offer_row() -> OfferRow {
    OfferRow {
        goods: item(MAX_RECORD),
        price: Some(max_price()),
        buyer: "b".repeat(MAX_NAME),
        seller: "s".repeat(MAX_NAME),
        offered: max_price(),
        counter: Some(max_price()),
        ..offer_row()
    }
}

fn listing_offer(p: Price) -> ListingOffer {
    ListingOffer {
        offer: 9,
        offered: p.clone(),
        counter: Some(p),
        state: OfferState::Open,
        at_ms: 1,
        answered_ms: 0,
    }
}

fn shop_row(row: ListingRow, paid: Price) -> ShopRow {
    ShopRow {
        row,
        state: ListingState::Sold,
        hidden: true,
        live_offers: 2,
        newest_offer_ms: 77,
        sold: Some(Sold {
            buyer: "b".repeat(MAX_NAME),
            paid,
            sale_fee: 0,
            closed_ms: 99,
        }),
        relistable: false,
    }
}

fn delivery(goods: Goods, about: &str) -> DeliveryRow {
    DeliveryRow {
        delivery: 3,
        source: source::SOLD,
        reason: reason::NONE,
        listing: Some(41),
        offer: None,
        about: about.into(),
        goods,
        at_ms: 12,
    }
}

fn query() -> MarketQuery {
    MarketQuery {
        text: "harle".into(),
        groups: vec![Group::Unique, Group::Set],
        types: vec!["helm".into(), "circ".into()],
        ethereal: TriState::No,
        socketed: TriState::Yes,
        identified: TriState::Any,
        stats: vec![StatRange {
            stat: 7,
            min: -5,
            max: 30,
        }],
        level: (10, 62),
        sockets: (1, 3),
        class: Some(2),
        price_mode: PriceMode::CurrenciesOnly,
        caps: vec![("r24".into(), 2), (String::new(), 400_000)],
        sort: Sort {
            by: SortBy::Price,
            stat: 0,
            currency: "r24".into(),
            desc: false,
        },
        group_first: true,
        page: 3,
        page_size: 100,
        facets: true,
    }
}

fn max_query() -> MarketQuery {
    MarketQuery {
        text: "t".repeat(MAX_QUERY_TEXT),
        groups: Group::ALL.to_vec(),
        types: (0..MAX_QUERY_TYPES).map(|i| format!("{i:0>4}")).collect(),
        stats: vec![
            StatRange {
                stat: u16::MAX,
                min: i32::MIN,
                max: i32::MAX,
            };
            MAX_QUERY_STATS
        ],
        caps: (0..MAX_CAPS)
            .map(|i| (format!("{i:0>8}"), u64::MAX))
            .collect(),
        sort: Sort {
            by: SortBy::Stat,
            stat: 9,
            currency: "c".repeat(MAX_CODE),
            desc: true,
        },
        ..query()
    }
}

fn facets() -> Facets {
    Facets {
        groups: vec![(Group::Unique, 12), (Group::Stackable, 400)],
        types: vec![("helm".into(), 9), ("armo".into(), 30)],
        ethereal: [1, 2],
        socketed: [3, 4],
        identified: [5, 6],
        price_modes: [7, 8, 9, 10, 11],
    }
}

fn max_facets() -> Facets {
    Facets {
        groups: Group::ALL.iter().map(|g| (*g, u32::MAX)).collect(),
        types: (0..MAX_FACET_TYPES)
            .map(|i| (format!("{i:0>4}"), u32::MAX))
            .collect(),
        ..facets()
    }
}

fn state() -> MarketState {
    MarketState {
        req: 4,
        on: true,
        off_reason: OffReason::On,
        mod_id: "d2rust".into(),
        hardcore: false,
        ladder: 3,
        expansion: true,
        limits: MarketLimits {
            max_listings: 50,
            max_offers: 20,
            max_days: 7,
            max_offer_items: 10,
            searches_per_min: 60,
            posts_per_min: 10,
        },
        fees: Fees {
            sale_fee_pct: 0,
            listing_fee: 0,
            buyer_fee: 0,
            fee_rev: 0xA1B2_C3D4,
        },
        counts: MarketCounts {
            active_listings: 4,
            deliveries_waiting: 2,
            live_offers_to_me: 1,
            my_live_offers: 0,
        },
        last_req: 1_000,
        stackables: vec!["r01".into(), "gpw".into()],
    }
}

fn done() -> Done {
    Done {
        req: 8,
        code: ErrorCode(0),
        wait_s: 0,
        repeated: true,
        id: Some(41),
        rows: vec![(41, ErrorCode(0)), (42, ErrorCode::NOT_YOURS)],
        text: String::new(),
    }
}

fn notice() -> Notice {
    Notice {
        what: notice::SOLD,
        listing: 41,
        offer: Some(9),
        about: "Grim Crown".into(),
        who: "Ann".into(),
        price: Some(price()),
        deliveries_waiting: 3,
        at_ms: 50,
    }
}

fn series() -> PriceSeries {
    PriceSeries {
        index: 1,
        shape: PriceShape::Bundle,
        currencies: vec!["r23".into(), "r24".into()],
        sold: 7,
        listed_now: 2,
        lowest_now: vec![1, 2],
        median_now: vec![3, 4],
        buckets: vec![
            Bucket {
                index: 0,
                sales: 2,
                values: vec![
                    Spread {
                        min: 1,
                        median: 1,
                        max: 2,
                    },
                    Spread {
                        min: 2,
                        median: 2,
                        max: 3,
                    },
                ],
            },
            Bucket {
                index: 5,
                sales: 1,
                values: vec![
                    Spread {
                        min: 1,
                        median: 1,
                        max: 1,
                    };
                    2
                ],
            },
        ],
    }
}

fn asks() -> Vec<Ask> {
    vec![
        Ask::Hello,
        Ask::Search(query()),
        Ask::Search(MarketQuery::default()),
        Ask::ListingGet { listing: 41 },
        Ask::Buy {
            listing: 41,
            price_hash: [7; 32],
            fee_rev: 3,
        },
        Ask::Offer {
            listing: 41,
            price: offer_row().offered,
            fee_rev: 3,
        },
        Ask::Answer {
            offer: 9,
            how: AnswerHow::Counter,
            counter: Some(price()),
            fee_rev: 3,
        },
        Ask::Answer {
            offer: 9,
            how: AnswerHow::Withdraw,
            counter: None,
            fee_rev: 3,
        },
        Ask::Post(Post {
            goods: PostGoods::Item { item_id: 77 },
            mode: ListingMode::Price,
            price: Some(price()),
            days: 7,
            note: "fast trade".into(),
            hidden: false,
            fee_rev: 3,
        }),
        Ask::Post(Post {
            goods: PostGoods::Stack {
                code: "r30".into(),
                count: 2,
            },
            mode: ListingMode::OffersOnly,
            price: None,
            days: 1,
            note: String::new(),
            hidden: true,
            fee_rev: 0,
        }),
        Ask::ShopEdit {
            op: ShopOp::Cancel,
            listings: vec![1, 2, 3],
            fee_rev: 3,
        },
        Ask::ShopEdit {
            op: ShopOp::Relist { days: 7 },
            listings: vec![1],
            fee_rev: 3,
        },
        Ask::ShopEdit {
            op: ShopOp::Hide,
            listings: vec![1],
            fee_rev: 3,
        },
        Ask::ShopEdit {
            op: ShopOp::Show,
            listings: vec![1],
            fee_rev: 3,
        },
        Ask::ShopEdit {
            op: ShopOp::Note("new note".into()),
            listings: vec![1, 2],
            fee_rev: 3,
        },
        Ask::ShopEdit {
            op: ShopOp::Reprice {
                mode: ListingMode::PriceAndOffers,
                price: Some(Price::gold(5)),
            },
            listings: vec![1],
            fee_rev: 3,
        },
        Ask::ShopOpen {
            tab: ShopTab::Expired,
            page: 2,
            sort: ShopSort::Ends,
            desc: true,
        },
        Ask::OffersOpen {
            which: OffersWhich::MadeByMe,
            live_only: true,
            page: 1,
        },
        Ask::DeliveriesOpen { page: 4 },
        Ask::Claim {
            all: false,
            deliveries: vec![3, 4],
        },
        Ask::Claim {
            all: true,
            deliveries: Vec::new(),
        },
        Ask::Prices {
            of: PriceOf::Listing(41),
            days: 30,
        },
        Ask::Prices {
            of: PriceOf::StashItem(77),
            days: 2,
        },
        Ask::Prices {
            of: PriceOf::Stackable("r24".into()),
            days: 90,
        },
        Ask::Prices {
            of: PriceOf::Key("u:Harlequin Crest".into()),
            days: 7,
        },
    ]
}

fn replies() -> Vec<FromRealm> {
    vec![
        FromRealm::State(state()),
        FromRealm::State(MarketState {
            req: 0,
            on: false,
            off_reason: OffReason::Switch,
            stackables: Vec::new(),
            ..state()
        }),
        FromRealm::ResultsBegin {
            req: 2,
            head: ResultsHead {
                total: 1_234,
                page: 1,
                page_size: 50,
                rows: 50,
                now_ms: 99,
                facets: true,
            },
        },
        FromRealm::Facets {
            req: 2,
            facets: facets(),
        },
        FromRealm::Row { req: 2, row: row() },
        FromRealm::Row {
            req: 2,
            row: ListingRow {
                goods: Goods::Stack {
                    code: "r30".into(),
                    count: 1,
                },
                group: Group::Stackable,
                mode: ListingMode::OffersOnly,
                price: None,
                note: String::new(),
                mine: true,
                ..row()
            },
        },
        FromRealm::Listing {
            req: 3,
            detail: ListingDetail {
                state: ListingState::Active,
                row: row(),
                buyer_fee: 0,
                fee_rev: 3,
                my_offer: Some(listing_offer(price())),
            },
        },
        FromRealm::Listing {
            req: 3,
            detail: ListingDetail {
                state: ListingState::Expired,
                row: row(),
                buyer_fee: 100,
                fee_rev: 3,
                my_offer: None,
            },
        },
        FromRealm::ShopBegin {
            req: 4,
            head: ShopHead {
                tab: ShopTab::Hidden,
                page: 0,
                rows: 2,
                counts: [4, 2, 30, 1],
                slots_used: 6,
                slots_max: 50,
                now_ms: 9,
            },
        },
        FromRealm::ShopRow {
            req: 4,
            row: shop_row(row(), price()),
        },
        FromRealm::ShopRow {
            req: 4,
            row: ShopRow {
                state: ListingState::Active,
                hidden: false,
                sold: None,
                relistable: true,
                ..shop_row(row(), price())
            },
        },
        FromRealm::OffersBegin {
            req: 5,
            head: OffersHead {
                which: OffersWhich::OnMyListings,
                page: 0,
                rows: 1,
                total: 1,
                now_ms: 9,
            },
        },
        FromRealm::OfferRow {
            req: 5,
            row: offer_row(),
        },
        FromRealm::DeliveriesBegin {
            req: 6,
            head: DeliveriesHead {
                page: 0,
                rows: 3,
                total: 3,
                now_ms: 9,
            },
        },
        FromRealm::DeliveryRow {
            req: 6,
            row: delivery(Goods::Gold(400_000), "Grim Crown"),
        },
        FromRealm::DeliveryRow {
            req: 6,
            row: DeliveryRow {
                source: source::OTHER,
                reason: reason::DECLINED,
                listing: None,
                offer: Some(9),
                ..delivery(item(30), "")
            },
        },
        FromRealm::Item {
            req: 5,
            item: ItemRecord {
                item_id: 5,
                record: vec![1, 2, 3],
            },
        },
        FromRealm::PricesBegin {
            req: 7,
            head: PricesHead {
                key: "u:Harlequin Crest".into(),
                label: "Harlequin Crest".into(),
                now_ms: 9,
                bucket_ms: 86_400_000,
                listed_now: 4,
                sold: 19,
                series: 2,
            },
        },
        FromRealm::PriceSeries {
            req: 7,
            series: series(),
        },
        FromRealm::PriceSeries {
            req: 7,
            series: PriceSeries {
                index: 2,
                shape: PriceShape::WithItems,
                lowest_now: Vec::new(),
                median_now: Vec::new(),
                buckets: vec![Bucket {
                    index: 3,
                    sales: 4,
                    values: Vec::new(),
                }],
                ..series()
            },
        },
        FromRealm::PriceRecent {
            req: 7,
            sales: vec![
                Sale {
                    at_ms: 9,
                    price: price(),
                },
                Sale {
                    at_ms: 8,
                    price: offer_row().offered,
                },
            ],
        },
        FromRealm::Done(done()),
        FromRealm::Done(Done {
            wait_s: 20,
            ..Done::refused(9, ErrorCode::RATE_LIMITED)
        }),
        FromRealm::Notice(notice()),
        FromRealm::Notice(Notice {
            what: notice::LISTING_EXPIRED,
            offer: None,
            price: None,
            who: String::new(),
            ..notice()
        }),
        FromRealm::ListEnd { req: 2 },
    ]
}

/// Every market message reads back as it was written, carries its kind in
/// the market range and its `req`.
#[test]
fn market_messages_round_trip() {
    let asks = asks();
    let mut kinds = std::collections::BTreeSet::new();
    for (i, ask) in asks.into_iter().enumerate() {
        let m = ToRealm {
            req: 1_000 + i as u64,
            ask,
        };
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(super::super::kind::MARKET.contains(&body[0]));
        assert_eq!(body[1..9], m.req.to_le_bytes());
        assert_eq!(ToRealm::parse(&body).as_ref(), Ok(&m), "{m:?}");
        // A request is never read as an answer.
        assert!(FromRealm::parse(&body).is_err(), "{m:?}");
        kinds.insert(body[0]);
    }
    for m in replies() {
        let body = m.body();
        assert_eq!(body[0], m.kind());
        assert!(super::super::kind::MARKET.contains(&body[0]));
        assert_eq!(FromRealm::parse(&body).as_ref(), Ok(&m), "{m:?}");
        assert!(ToRealm::parse(&body).is_err(), "{m:?}");
        if let Some(req) = m.req() {
            assert_eq!(body[1..9], req.to_le_bytes(), "{m:?}");
        }
        kinds.insert(body[0]);
    }
    // Every kind of the contract's table, 0xB0-0xCE, is covered.
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        (kind::HELLO..=kind::LIST_END).collect::<Vec<_>>()
    );
}

/// The exact layout of a few messages (contract sections 1-3).
#[test]
fn market_layouts() {
    let buy = ToRealm {
        req: 2,
        ask: Ask::Buy {
            listing: 3,
            price_hash: [9; 32],
            fee_rev: 0x0102_0304,
        },
    }
    .body();
    let mut want = vec![0xB8, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0];
    want.extend_from_slice(&[9; 32]);
    want.extend_from_slice(&[4, 3, 2, 1]);
    assert_eq!(buy, want);

    // An item record is `bytes16`: a u16 length, then the bytes.
    let item = FromRealm::Item {
        req: 1,
        item: ItemRecord {
            item_id: 5,
            record: vec![0xAA; 300],
        },
    }
    .body();
    assert_eq!(
        item[..15],
        [0xC7, 1, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0x2C, 0x01]
    );
    assert_eq!(item.len(), 15 + 300);

    // A price: gold, the currencies, the items.
    let offer = ToRealm {
        req: 1,
        ask: Ask::Offer {
            listing: 2,
            price: Price {
                gold: 7,
                stacks: vec![("r24".into(), 2)],
                items: vec![9],
            },
            fee_rev: 0,
        },
    }
    .body();
    assert_eq!(
        offer[17..],
        [
            7, 0, 0, 0, 0, 0, 0, 0, // gold
            1, 3, b'r', b'2', b'4', 2, 0, 0, 0, // one currency
            1, 9, 0, 0, 0, // one item
            0, 0, 0, 0, // fee_rev
        ]
    );

    let done = FromRealm::Done(Done::refused(4, ErrorCode::PRICE_CHANGED)).body();
    let text = ErrorCode::PRICE_CHANGED.text();
    let mut want = vec![0xCC, 4, 0, 0, 0, 0, 0, 0, 0, 84, 0, 0, 0, 0, 0, 0, 0, 0];
    want.push(text.len() as u8);
    want.extend_from_slice(text.as_bytes());
    assert_eq!(done, want);
}

fn assert_fits(m: &FromRealm) {
    let body = m.body();
    assert!(
        body.len() <= MAX_FRAME,
        "kind {:#x}: {} bytes",
        body[0],
        body.len()
    );
    assert_eq!(FromRealm::parse(&body).as_ref(), Ok(m));
}

/// The largest legal message of every kind fits one frame: a 2,048-byte
/// record, the longest names and note, the largest price (contract 1-2).
#[test]
fn market_rows_fit_a_frame() {
    let max_listing_offer = listing_offer(max_price());
    for m in [
        FromRealm::Row {
            req: u64::MAX,
            row: max_row(),
        },
        FromRealm::Listing {
            req: u64::MAX,
            detail: ListingDetail {
                state: ListingState::Active,
                row: max_row(),
                buyer_fee: u64::MAX,
                fee_rev: u32::MAX,
                my_offer: Some(max_listing_offer),
            },
        },
        FromRealm::ShopRow {
            req: u64::MAX,
            row: shop_row(max_row(), max_price()),
        },
        FromRealm::OfferRow {
            req: u64::MAX,
            row: max_offer_row(),
        },
        FromRealm::DeliveryRow {
            req: u64::MAX,
            row: delivery(item(MAX_RECORD), &"a".repeat(MAX_ABOUT)),
        },
        FromRealm::Item {
            req: u64::MAX,
            item: ItemRecord {
                item_id: u32::MAX,
                record: vec![0xFF; MAX_RECORD],
            },
        },
        FromRealm::Facets {
            req: u64::MAX,
            facets: max_facets(),
        },
        FromRealm::State(MarketState {
            mod_id: "m".repeat(MAX_MOD_ID),
            stackables: (0..255).map(|i| format!("{i:0>8}")).collect(),
            ..state()
        }),
        FromRealm::Done(Done {
            rows: vec![(u64::MAX, ErrorCode(u16::MAX)); 255],
            text: "t".repeat(MAX_ERROR_TEXT),
            ..done()
        }),
        FromRealm::Notice(Notice {
            about: "a".repeat(MAX_ABOUT),
            who: "w".repeat(MAX_NAME),
            price: Some(max_price()),
            ..notice()
        }),
        FromRealm::PricesBegin {
            req: u64::MAX,
            head: PricesHead {
                key: "k".repeat(MAX_PRICE_KEY),
                label: "l".repeat(MAX_ABOUT),
                now_ms: 0,
                bucket_ms: 0,
                listed_now: 0,
                sold: 0,
                series: 255,
            },
        },
        FromRealm::PriceRecent {
            req: u64::MAX,
            sales: vec![
                Sale {
                    at_ms: u64::MAX,
                    price: max_price(),
                };
                MAX_RECENT
            ],
        },
    ] {
        assert_fits(&m);
    }

    // The largest requests fit too.
    for ask in [
        Ask::Search(max_query()),
        Ask::Offer {
            listing: 1,
            price: max_price(),
            fee_rev: 0,
        },
        Ask::Answer {
            offer: 1,
            how: AnswerHow::Counter,
            counter: Some(max_price()),
            fee_rev: 0,
        },
        Ask::ShopEdit {
            op: ShopOp::Reprice {
                mode: ListingMode::Price,
                price: Some(max_price()),
            },
            listings: vec![u64::MAX; 255],
            fee_rev: 0,
        },
        Ask::Claim {
            all: false,
            deliveries: vec![u64::MAX; 255],
        },
    ] {
        let m = ToRealm { req: u64::MAX, ask };
        let body = m.body();
        assert!(body.len() <= MAX_FRAME, "{:#x}: {}", body[0], body.len());
        assert_eq!(ToRealm::parse(&body), Ok(m));
    }
}

/// A long price series goes in parts that each fit a frame, and the
/// parts' buckets put together are the series.
#[test]
fn long_price_series_split_to_fit() {
    let currencies: Vec<String> = (0..MAX_CURRENCIES).map(|i| format!("{i:0>8}")).collect();
    let long = PriceSeries {
        index: 4,
        shape: PriceShape::Bundle,
        lowest_now: vec![u64::MAX; currencies.len()],
        median_now: vec![u64::MAX; currencies.len()],
        buckets: (0..2_160)
            .map(|i| Bucket {
                index: i,
                sales: 1,
                values: vec![
                    Spread {
                        min: u64::MAX,
                        median: u64::MAX,
                        max: u64::MAX,
                    };
                    currencies.len()
                ],
            })
            .collect(),
        currencies,
        sold: 2_160,
        listed_now: 1,
    };
    let parts = long.split_to_fit();
    assert!(parts.len() > 1);
    let mut joined = Vec::new();
    for p in &parts {
        assert_eq!((p.index, p.shape), (long.index, long.shape));
        assert_eq!(p.currencies, long.currencies);
        assert_fits(&FromRealm::PriceSeries {
            req: u64::MAX,
            series: p.clone(),
        });
        joined.extend(p.buckets.iter().cloned());
    }
    assert_eq!(joined, long.buckets);
    // A short series is one part; an empty one too.
    assert_eq!(series().split_to_fit(), vec![series()]);
    let empty = PriceSeries {
        buckets: Vec::new(),
        ..series()
    };
    assert_eq!(empty.split_to_fit(), vec![empty]);
}

fn refused_reply(m: FromRealm) {
    let body = m.body();
    assert_eq!(FromRealm::parse(&body), Err(NetError::Malformed), "{m:?}");
}

fn refused_ask(ask: Ask) {
    let body = ToRealm { req: 1, ask }.body();
    assert_eq!(ToRealm::parse(&body), Err(NetError::Malformed));
}

/// Bytes left over, cut bodies, over-long fields and unknown values are
/// malformed.
#[test]
fn market_malformed_refused() {
    // Trailing bytes and a cut body, for every message.
    for ask in asks() {
        let mut body = ToRealm { req: 1, ask }.body();
        body.push(0);
        assert_eq!(ToRealm::parse(&body), Err(NetError::Malformed));
        body.truncate(body.len() - 2);
        assert_eq!(ToRealm::parse(&body), Err(NetError::Malformed));
    }
    for m in replies() {
        let mut body = m.body();
        body.push(0);
        assert_eq!(FromRealm::parse(&body), Err(NetError::Malformed), "{m:?}");
        body.truncate(body.len() - 2);
        assert_eq!(FromRealm::parse(&body), Err(NetError::Malformed), "{m:?}");
    }
    // Unknown kinds: the reserved 0xCF, and kinds of the other direction.
    assert!(FromRealm::parse(&[0xCF, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    assert!(ToRealm::parse(&[0xCF, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    assert!(ToRealm::parse(&[kind::LIST_END, 0, 0, 0, 0, 0, 0, 0, 0]).is_err());
    assert!(ToRealm::parse(&[]).is_err());

    // Over-long fields of the rows.
    let r = |row| FromRealm::Row { req: 1, row };
    refused_reply(r(ListingRow {
        goods: item(MAX_RECORD + 1),
        ..row()
    }));
    refused_reply(r(ListingRow {
        seller: "s".repeat(MAX_NAME + 1),
        ..row()
    }));
    refused_reply(r(ListingRow {
        note: "n".repeat(MAX_NOTE + 1),
        ..row()
    }));
    let mut p = price();
    p.stacks = vec![("r".repeat(MAX_CODE + 1), 1)];
    refused_reply(r(ListingRow {
        price: Some(p),
        ..row()
    }));
    let mut p = price();
    p.stacks = vec![("r24".into(), 1); MAX_CURRENCIES + 1];
    refused_reply(r(ListingRow {
        price: Some(p),
        ..row()
    }));
    let mut p = price();
    p.items = vec![1; MAX_PRICE_ITEMS + 1];
    refused_ask(Ask::Offer {
        listing: 1,
        price: p,
        fee_rev: 0,
    });
    // A listing never holds gold.
    refused_reply(r(ListingRow {
        goods: Goods::Gold(5),
        ..row()
    }));
    refused_reply(FromRealm::OfferRow {
        req: 1,
        row: OfferRow {
            buyer: "b".repeat(MAX_NAME + 1),
            ..offer_row()
        },
    });
    refused_reply(FromRealm::DeliveryRow {
        req: 1,
        row: delivery(Goods::Gold(1), &"a".repeat(MAX_ABOUT + 1)),
    });
    refused_reply(FromRealm::Item {
        req: 1,
        item: ItemRecord {
            item_id: 1,
            record: vec![0; MAX_RECORD + 1],
        },
    });
    let mut f = facets();
    f.types = vec![("helm".into(), 1); MAX_FACET_TYPES + 1];
    refused_reply(FromRealm::Facets { req: 1, facets: f });
    let mut f = facets();
    f.types = vec![("helms".into(), 1)];
    refused_reply(FromRealm::Facets { req: 1, facets: f });
    refused_reply(FromRealm::State(MarketState {
        mod_id: "m".repeat(MAX_MOD_ID + 1),
        ..state()
    }));
    refused_reply(FromRealm::Done(Done {
        text: "t".repeat(MAX_ERROR_TEXT + 1),
        ..done()
    }));
    refused_reply(FromRealm::PriceRecent {
        req: 1,
        sales: vec![
            Sale {
                at_ms: 0,
                price: price(),
            };
            MAX_RECENT + 1
        ],
    });
    // `lowest_now` and `median_now` are empty or one per currency.
    refused_reply(FromRealm::PriceSeries {
        req: 1,
        series: PriceSeries {
            lowest_now: vec![1],
            ..series()
        },
    });
    refused_reply(FromRealm::PriceSeries {
        req: 1,
        series: PriceSeries {
            median_now: vec![1],
            ..series()
        },
    });

    // Over-long query fields.
    for q in [
        MarketQuery {
            text: "t".repeat(MAX_QUERY_TEXT + 1),
            ..query()
        },
        MarketQuery {
            groups: vec![Group::Magic; Group::ALL.len() + 1],
            ..query()
        },
        MarketQuery {
            types: vec!["helm".into(); MAX_QUERY_TYPES + 1],
            ..query()
        },
        MarketQuery {
            types: vec!["helms".into()],
            ..query()
        },
        MarketQuery {
            stats: vec![
                StatRange {
                    stat: 1,
                    min: 0,
                    max: 1,
                };
                MAX_QUERY_STATS + 1
            ],
            ..query()
        },
        MarketQuery {
            caps: vec![("r24".into(), 1); MAX_CAPS + 1],
            ..query()
        },
    ] {
        refused_ask(Ask::Search(q));
    }
    refused_ask(Ask::Prices {
        of: PriceOf::Key("k".repeat(MAX_PRICE_KEY + 1)),
        days: 1,
    });

    // Values outside their field's set: patch one byte of a valid body.
    let patch = |body: Vec<u8>, at: usize, v: u8| {
        let mut b = body;
        b[at] = v;
        b
    };
    let row_body = r(row()).body();
    // kind, req (8), listing (8), then the group.
    assert!(FromRealm::parse(&patch(row_body.clone(), 17, 8)).is_err());
    assert!(FromRealm::parse(&patch(row_body.clone(), 17, 7)).is_ok());
    // The goods kind.
    assert!(FromRealm::parse(&patch(row_body, 18, 3)).is_err());
    let answer = ToRealm {
        req: 1,
        ask: Ask::Answer {
            offer: 1,
            how: AnswerHow::Accept,
            counter: None,
            fee_rev: 0,
        },
    }
    .body();
    assert!(ToRealm::parse(&patch(answer, 17, 4)).is_err());
    let edit = ToRealm {
        req: 1,
        ask: Ask::ShopEdit {
            op: ShopOp::Hide,
            listings: vec![1],
            fee_rev: 0,
        },
    }
    .body();
    assert!(ToRealm::parse(&patch(edit, 9, 6)).is_err());
    let prices = ToRealm {
        req: 1,
        ask: Ask::Prices {
            of: PriceOf::StashItem(1),
            days: 1,
        },
    }
    .body();
    assert!(ToRealm::parse(&patch(prices, 9, 4)).is_err());
    let open = ToRealm {
        req: 1,
        ask: Ask::ShopOpen {
            tab: ShopTab::Shown,
            page: 0,
            sort: ShopSort::Posted,
            desc: false,
        },
    }
    .body();
    assert!(ToRealm::parse(&patch(open.clone(), 9, 4)).is_err());
    assert!(ToRealm::parse(&patch(open, 12, 4)).is_err());
    let st = FromRealm::State(state()).body();
    // kind, req, on, then the off reason.
    assert!(FromRealm::parse(&patch(st, 10, 3)).is_err());
}

/// What the market's rules decide is the realm's to refuse, not the
/// decoder's: a long note, a page size, a day count still read.
#[test]
fn market_rules_are_the_realms() {
    for ask in [
        Ask::Post(Post {
            goods: PostGoods::Item { item_id: 1 },
            mode: ListingMode::Price,
            price: Some(Price {
                items: vec![1],
                ..price()
            }),
            days: 200,
            note: "n".repeat(255),
            hidden: false,
            fee_rev: 0,
        }),
        Ask::ShopEdit {
            op: ShopOp::Note("n".repeat(255)),
            listings: vec![1; 51],
            fee_rev: 0,
        },
        Ask::Search(MarketQuery {
            page_size: 7,
            ..query()
        }),
        Ask::Prices {
            of: PriceOf::Listing(1),
            days: 365,
        },
    ] {
        let m = ToRealm { req: 1, ask };
        assert_eq!(ToRealm::parse(&m.body()), Ok(m));
    }
}

/// Changing requests are the ones the realm stores under their `req`.
#[test]
fn changing_requests() {
    let changing: Vec<u8> = asks()
        .iter()
        .filter(|a| a.changes())
        .map(Ask::kind)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(
        changing,
        [
            kind::BUY,
            kind::OFFER,
            kind::ANSWER,
            kind::POST,
            kind::SHOP_EDIT,
            kind::CLAIM
        ]
    );
}

/// The refusal codes of contract section 6, with the market's words.
#[test]
fn market_refusal_codes() {
    for (code, name, n) in [
        (ErrorCode::STASH_FULL, "StashFull", 80),
        (ErrorCode::ITEM_NOT_FOUND, "ItemNotFound", 82),
        (ErrorCode::LISTING_GONE, "ListingGone", 83),
        (ErrorCode::PRICE_CHANGED, "PriceChanged", 84),
        (ErrorCode::NOT_ENOUGH, "NotEnough", 85),
        (ErrorCode::MARKET_LIMIT, "MarketLimit", 86),
        (ErrorCode::ITEM_HELD, "ItemHeld", 87),
        (ErrorCode::OFFER_GONE, "OfferGone", 88),
        (ErrorCode::ECONOMY_MISMATCH, "EconomyMismatch", 89),
        (ErrorCode::MARKET_OFF, "MarketOff", 90),
        (ErrorCode::FEE_CHANGED, "FeeChanged", 91),
        (ErrorCode::NOT_YOURS, "NotYours", 92),
    ] {
        assert_eq!(code.0, n);
        assert_eq!(code.name(), name);
        assert!(ErrorCode::ALL.contains(&code));
        assert_eq!(refusal_text(code, 0, "realm words"), code.text());
    }
    assert_eq!(
        ErrorCode::MARKET_OFF.text(),
        "The market is closed on this realm."
    );
    assert_eq!(
        refusal_text(ErrorCode::RATE_LIMITED, 12, ""),
        "Too many market requests; wait 12 s."
    );
    // `Other` and unknown codes show the realm's words.
    assert_eq!(refusal_text(ErrorCode::OTHER, 0, "bad note"), "bad note");
    assert_eq!(refusal_text(ErrorCode(98), 0, "new rule"), "new rule");
    assert_eq!(refusal_text(ErrorCode(0), 0, "x"), "");
    let d = Done {
        wait_s: 3,
        ..Done::refused(1, ErrorCode::RATE_LIMITED)
    };
    assert_eq!(d.message(), "Too many market requests; wait 3 s.");
    assert!(!d.is_ok());
    assert!(Done::ok(1).is_ok());
}

#[test]
fn grid_order_has_every_group_once() {
    let mut g: Vec<u8> = Group::GRID_ORDER.iter().map(|g| *g as u8).collect();
    assert_eq!(g, [5, 4, 6, 2, 3, 1, 0, 7]);
    g.sort_unstable();
    assert_eq!(g, (0..8).collect::<Vec<_>>());
}
