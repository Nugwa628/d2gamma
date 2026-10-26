//! Putting the market's answers together from interleaved bodies.

use super::*;
use d2_net::msg::lobby::{GameInfo, LobbyEvent};
use d2_net::msg::market::{
    source, Fees, Goods, Group, ListingMode, ListingState, MarketCounts, MarketLimits, MarketQuery,
    OffReason, OfferState, OffersWhich, Price, PriceShape, ShopSort, ShopTab,
};
use d2_net::msg::ErrorCode;

fn state(req: u64, last_req: u64) -> FromRealm {
    FromRealm::State(MarketState {
        req,
        on: true,
        off_reason: OffReason::On,
        mod_id: "d2rust".into(),
        hardcore: false,
        ladder: 0,
        expansion: true,
        limits: MarketLimits::default(),
        fees: Fees::default(),
        counts: MarketCounts::default(),
        last_req,
        stackables: vec!["r24".into()],
    })
}

fn row(listing: u64) -> ListingRow {
    ListingRow {
        listing,
        group: Group::Unique,
        goods: Goods::Item {
            item_id: listing as u32,
            record: vec![1, 2, 3],
        },
        mode: ListingMode::Price,
        price: Some(Price::gold(100)),
        price_hash: [1; 32],
        seller: "Werwer".into(),
        seller_sales: 0,
        posted_ms: 1,
        expires_ms: 2,
        note: String::new(),
        offers: 0,
        mine: false,
    }
}

fn shop_row(listing: u64) -> ShopRow {
    ShopRow {
        row: row(listing),
        state: ListingState::Active,
        hidden: false,
        live_offers: 0,
        newest_offer_ms: 0,
        sold: None,
        relistable: false,
    }
}

fn offer_row(offer: u64, items: Vec<u32>) -> OfferRow {
    OfferRow {
        offer,
        listing: 41,
        goods: row(41).goods,
        price: None,
        buyer: "Ann".into(),
        seller: "Werwer".into(),
        offered: Price {
            gold: 5,
            stacks: Vec::new(),
            items,
        },
        counter: None,
        state: OfferState::Open,
        at_ms: 1,
        answered_ms: 0,
    }
}

fn record(item_id: u32) -> FromRealm {
    FromRealm::Item {
        req: 0,
        item: ItemRecord {
            item_id,
            record: vec![item_id as u8; 4],
        },
    }
}

/// Sets the `req` of a message built with 0.
fn with_req(m: FromRealm, r: u64) -> FromRealm {
    let mut body = m.body();
    body[1..9].copy_from_slice(&r.to_le_bytes());
    FromRealm::parse(&body).unwrap()
}

fn lobby_body() -> Vec<u8> {
    LobbyEvent::GameEntry(GameInfo {
        game_id: 1,
        name: "cows".into(),
        difficulty: 0,
        players: 1,
        max_players: 8,
        locked: false,
        hardcore: false,
        expansion: true,
        joinable: 0,
        ..GameInfo::default()
    })
    .body()
}

/// A client that said hello and got the realm's state.
fn ready_client(last_req: u64) -> MarketClient {
    let mut c = MarketClient::new();
    let hello = c.prepare(Ask::Hello).unwrap();
    assert!(c.feed(&state(hello.req, last_req).body()).unwrap());
    assert!(matches!(c.take()[..], [MarketAnswer::State(_)]));
    c
}

/// Two lists in flight, their bodies interleaved with each other, with a
/// third list's offered items and with lobby messages: each comes out
/// whole under its own `req`, and the lobby bodies go back to the caller.
#[test]
fn client_assembles_interleaved_lists() {
    let mut c = ready_client(0);
    let search = c
        .prepare(Ask::Search(MarketQuery {
            facets: true,
            ..MarketQuery::default()
        }))
        .unwrap()
        .req;
    let shop = c
        .prepare(Ask::ShopOpen {
            tab: ShopTab::Shown,
            page: 0,
            sort: ShopSort::Posted,
            desc: false,
        })
        .unwrap()
        .req;
    let offers = c
        .prepare(Ask::OffersOpen {
            which: OffersWhich::OnMyListings,
            live_only: true,
            page: 0,
        })
        .unwrap()
        .req;
    assert_eq!(c.in_flight(), 3);

    let results_head = ResultsHead {
        total: 2,
        page: 0,
        page_size: 50,
        rows: 2,
        now_ms: 9,
        facets: true,
    };
    let shop_head = ShopHead {
        tab: ShopTab::Shown,
        page: 0,
        rows: 1,
        counts: [1, 0, 0, 0],
        slots_used: 1,
        slots_max: 50,
        now_ms: 9,
    };
    let offers_head = OffersHead {
        which: OffersWhich::OnMyListings,
        page: 0,
        rows: 2,
        total: 2,
        now_ms: 9,
    };
    let notice = Notice {
        what: d2_net::msg::market::notice::OFFER_RECEIVED,
        listing: 41,
        offer: Some(7),
        about: "Grim Crown".into(),
        who: "Ann".into(),
        price: None,
        deliveries_waiting: 0,
        at_ms: 9,
    };
    let bodies: Vec<Vec<u8>> = vec![
        FromRealm::ResultsBegin {
            req: search,
            head: results_head,
        }
        .body(),
        FromRealm::OffersBegin {
            req: offers,
            head: offers_head,
        }
        .body(),
        FromRealm::ShopBegin {
            req: shop,
            head: shop_head,
        }
        .body(),
        with_req(record(5), offers).body(),
        FromRealm::Row {
            req: search,
            row: row(1),
        }
        .body(),
        lobby_body(),
        with_req(record(6), offers).body(),
        FromRealm::ShopRow {
            req: shop,
            row: shop_row(3),
        }
        .body(),
        FromRealm::OfferRow {
            req: offers,
            row: offer_row(7, vec![5, 6]),
        }
        .body(),
        FromRealm::Notice(notice.clone()).body(),
        FromRealm::Facets {
            req: search,
            facets: Facets::default(),
        }
        .body(),
        FromRealm::ListEnd { req: shop }.body(),
        FromRealm::Row {
            req: search,
            row: row(2),
        }
        .body(),
        FromRealm::OfferRow {
            req: offers,
            row: offer_row(8, Vec::new()),
        }
        .body(),
        LobbyEvent::GameListEnd.body(),
        FromRealm::ListEnd { req: search }.body(),
        FromRealm::ListEnd { req: offers }.body(),
    ];
    let mut others = Vec::new();
    for b in &bodies {
        if !c.feed(b).unwrap() {
            others.push(b.clone());
        }
    }
    assert_eq!(others, vec![lobby_body(), LobbyEvent::GameListEnd.body()]);
    assert_eq!(
        c.take(),
        vec![
            MarketAnswer::Notice(notice),
            MarketAnswer::Shop {
                req: shop,
                head: shop_head,
                rows: vec![shop_row(3)],
            },
            MarketAnswer::Results {
                req: search,
                head: results_head,
                facets: Some(Facets::default()),
                rows: vec![row(1), row(2)],
            },
            MarketAnswer::Offers {
                req: offers,
                head: offers_head,
                rows: vec![offer_row(7, vec![5, 6]), offer_row(8, Vec::new())],
                items: vec![
                    ItemRecord {
                        item_id: 5,
                        record: vec![5; 4]
                    },
                    ItemRecord {
                        item_id: 6,
                        record: vec![6; 4]
                    },
                ],
            },
        ]
    );
    assert_eq!(c.in_flight(), 0);
    assert!(c.take().is_empty());
}

/// Request numbers start above the highest one the realm stored, and a
/// changing request waits for the realm's state.
#[test]
fn requests_are_numbered_above_the_stored_ones() {
    let mut c = MarketClient::new();
    let buy = Ask::Buy {
        listing: 1,
        price_hash: [0; 32],
        fee_rev: 0,
    };
    assert!(!c.is_ready());
    assert_eq!(c.prepare(buy.clone()), None);
    let hello = c.prepare(Ask::Hello).unwrap();
    assert_eq!(hello.req, 1);
    assert_eq!(hello.body()[0], d2_net::msg::market::kind::HELLO);
    c.feed(&state(hello.req, 500).body()).unwrap();
    assert!(c.is_ready());
    let m = c.prepare(buy.clone()).unwrap();
    assert_eq!(m.req, 501);
    assert_eq!(ToRealm::parse(&m.body()).unwrap(), m);
    // A pushed state with a higher number moves the counter on; a lower
    // one doesn't move it back.
    c.feed(&state(0, 900).body()).unwrap();
    assert_eq!(c.prepare(buy.clone()).unwrap().req, 901);
    c.feed(&state(0, 3).body()).unwrap();
    assert_eq!(c.prepare(buy.clone()).unwrap().req, 902);
    // A new connection: in flight is forgotten, numbers keep rising.
    c.reset();
    assert!(!c.is_ready());
    assert_eq!(c.in_flight(), 0);
    assert_eq!(c.prepare(buy), None);
    assert_eq!(c.prepare(Ask::Hello).unwrap().req, 903);
}

/// Single answers: a done ends its request (and any list it started), a
/// listing takes the items sent before it, and a realm without the
/// market answers the hello with `ERROR`.
#[test]
fn single_answers_and_errors() {
    let mut c = ready_client(10);
    let get = c.prepare(Ask::ListingGet { listing: 41 }).unwrap().req;
    let detail = ListingDetail {
        state: ListingState::Active,
        row: row(41),
        buyer_fee: 0,
        fee_rev: 0,
        my_offer: None,
    };
    c.feed(&with_req(record(5), get).body()).unwrap();
    c.feed(
        &FromRealm::Listing {
            req: get,
            detail: detail.clone(),
        }
        .body(),
    )
    .unwrap();
    let search = c.prepare(Ask::Search(MarketQuery::default())).unwrap().req;
    c.feed(
        &FromRealm::ResultsBegin {
            req: search,
            head: ResultsHead {
                total: 0,
                page: 0,
                page_size: 50,
                rows: 5,
                now_ms: 0,
                facets: false,
            },
        }
        .body(),
    )
    .unwrap();
    // Refused part way (the realm stopped the list).
    let refused = Done::refused(search, ErrorCode::RATE_LIMITED);
    c.feed(&FromRealm::Done(refused.clone()).body()).unwrap();
    assert_eq!(
        c.take(),
        vec![
            MarketAnswer::Listing {
                req: get,
                detail: Box::new(detail),
                items: vec![ItemRecord {
                    item_id: 5,
                    record: vec![5; 4]
                }],
            },
            MarketAnswer::Done(refused),
        ]
    );
    assert_eq!(c.in_flight(), 0);

    let mut c = MarketClient::new();
    let hello = c.prepare(Ask::Hello).unwrap().req;
    let e = ErrorMsg::new(d2_net::msg::market::kind::HELLO, ErrorCode::NOT_SUPPORTED);
    assert!(c.feed(&e.body()).unwrap());
    assert_eq!(
        c.take(),
        vec![MarketAnswer::Error {
            req: Some(hello),
            error: e
        }]
    );
    // An error for a lobby request is not the market's.
    let lobby = ErrorMsg::new(0x45, ErrorCode::GAME_FULL);
    assert!(!c.feed(&lobby.body()).unwrap());
}

/// A price check's series sent in parts is one series; its recent sales
/// come with it.
#[test]
fn price_series_parts_are_joined() {
    let mut c = ready_client(0);
    let req = c
        .prepare(Ask::Prices {
            of: d2_net::msg::market::PriceOf::Stackable("r24".into()),
            days: 30,
        })
        .unwrap()
        .req;
    let head = PricesHead {
        key: "s:r24".into(),
        label: "Ist Rune".into(),
        now_ms: 9,
        bucket_ms: 86_400_000,
        listed_now: 0,
        sold: 3,
        series: 1,
    };
    let bucket = |index| d2_net::msg::market::Bucket {
        index,
        sales: 1,
        values: vec![d2_net::msg::market::Spread {
            min: 1,
            median: 2,
            max: 3,
        }],
    };
    let whole = PriceSeries {
        index: 0,
        shape: PriceShape::One,
        currencies: vec![String::new()],
        sold: 3,
        listed_now: 0,
        lowest_now: Vec::new(),
        median_now: Vec::new(),
        buckets: (0..3).map(bucket).collect(),
    };
    let part = |b: Vec<_>| FromRealm::PriceSeries {
        req,
        series: PriceSeries {
            buckets: b,
            ..whole.clone()
        },
    };
    let recent = vec![Sale {
        at_ms: 8,
        price: Price::gold(2),
    }];
    for m in [
        FromRealm::PricesBegin {
            req,
            head: head.clone(),
        },
        part(vec![bucket(0), bucket(1)]),
        part(vec![bucket(2)]),
        FromRealm::PriceRecent {
            req,
            sales: recent.clone(),
        },
        FromRealm::ListEnd { req },
    ] {
        assert!(c.feed(&m.body()).unwrap());
    }
    assert_eq!(
        c.take(),
        vec![MarketAnswer::Prices {
            req,
            head,
            series: vec![whole],
            recent,
        }]
    );
}

/// A list that breaks its framing is the realm breaking the protocol;
/// answers to requests not in flight are dropped.
#[test]
fn broken_lists_are_malformed() {
    let begin = |req, rows| FromRealm::DeliveriesBegin {
        req,
        head: DeliveriesHead {
            page: 0,
            rows,
            total: 1,
            now_ms: 0,
        },
    };
    let delivery = |req| FromRealm::DeliveryRow {
        req,
        row: DeliveryRow {
            delivery: 1,
            source: source::BOUGHT,
            reason: 0,
            listing: None,
            offer: None,
            about: String::new(),
            goods: Goods::Gold(1),
            at_ms: 0,
        },
    };
    let open = |c: &mut MarketClient| c.prepare(Ask::DeliveriesOpen { page: 0 }).unwrap().req;
    let feed_all = |c: &mut MarketClient, ms: Vec<FromRealm>| {
        ms.into_iter().try_for_each(|m| c.feed(&m.body()).map(drop))
    };

    // Fewer rows than announced.
    let mut c = ready_client(0);
    let r = open(&mut c);
    let got = feed_all(
        &mut c,
        vec![begin(r, 2), delivery(r), FromRealm::ListEnd { req: r }],
    );
    assert_eq!(got, Err(OnlineError::Malformed));
    // More rows than announced.
    let mut c = ready_client(0);
    let r = open(&mut c);
    let got = feed_all(&mut c, vec![begin(r, 1), delivery(r), delivery(r)]);
    assert_eq!(got, Err(OnlineError::Malformed));
    // A row with no Begin, a second Begin, a row of another list's kind.
    let mut c = ready_client(0);
    let r = open(&mut c);
    assert_eq!(
        feed_all(&mut c, vec![delivery(r)]),
        Err(OnlineError::Malformed)
    );
    let mut c = ready_client(0);
    let r = open(&mut c);
    assert_eq!(
        feed_all(&mut c, vec![begin(r, 1), begin(r, 1)]),
        Err(OnlineError::Malformed)
    );
    let mut c = ready_client(0);
    let r = open(&mut c);
    assert_eq!(
        feed_all(
            &mut c,
            vec![
                begin(r, 1),
                FromRealm::Row {
                    req: r,
                    row: row(1)
                }
            ]
        ),
        Err(OnlineError::Malformed)
    );
    // Facets announced but missing.
    let mut c = ready_client(0);
    let r = c.prepare(Ask::Search(MarketQuery::default())).unwrap().req;
    let got = feed_all(
        &mut c,
        vec![
            FromRealm::ResultsBegin {
                req: r,
                head: ResultsHead {
                    total: 0,
                    page: 0,
                    page_size: 50,
                    rows: 0,
                    now_ms: 0,
                    facets: true,
                },
            },
            FromRealm::ListEnd { req: r },
        ],
    );
    assert_eq!(got, Err(OnlineError::Malformed));
    // A market body that doesn't read.
    let mut c = ready_client(0);
    assert_eq!(c.feed(&[0xCE, 1]), Err(OnlineError::Malformed));

    // Not in flight: dropped, nothing assembled.
    let mut c = ready_client(0);
    feed_all(
        &mut c,
        vec![begin(77, 1), delivery(77), FromRealm::ListEnd { req: 77 }],
    )
    .unwrap();
    c.feed(&FromRealm::Done(Done::ok(78)).body()).unwrap();
    assert!(c.take().is_empty());
}
