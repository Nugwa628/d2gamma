//! The market window's realm source on a real `d2rust-realm` with the
//! market on (tools/mp-test/online-market.sh starts it on loopback and
//! passes its address, certificate, console and log): two accounts, each
//! with the window's [`RealmSource`] over its own main-server session,
//! send the asks the window sends and read the events it reads. The
//! seller posts, the buyer searches, looks a listing up and buys it,
//! offers on another (accepted) and on a third (declined), both claim
//! their deliveries; the shop's edits and the price check go the same
//! way. The realm's goods come from its `econ-seed` test line.

#![cfg(feature = "online")]

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use d2_client::app::online::{client_info, OnlineSession};
use d2_client::market::prices::PriceOf;
use d2_client::market::realm::RealmSource;
use d2_client::market::source::{
    Group, ListingMode, MarketAsk, MarketCode, MarketDone, MarketEvent, MarketQuery, MarketSource,
    MarketState, Post, PostGoods, Price, Req, ShopOp, ShopSort, ShopTab,
};
use d2_client::market::trade::{
    AnswerHow, DeliveriesPage, DeliveryReason, DeliverySource, OfferState, OffersWhich,
};
use d2_client::ui::market::{deliveries, MarketUi};
use d2_net::wire::Timing;
use d2_online::{parse_fingerprint, ServerSpec, ServerTrust, Session};

/// How long one answer may take.
const WAIT: Duration = Duration::from_secs(20);

fn env(k: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| panic!("{k} is not set"))
}

/// The main server's console: its test lines in, its answers in its log.
struct Console {
    fifo: String,
    log: String,
}

impl Console {
    fn answers(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with("dev "))
            .map(String::from)
            .collect()
    }

    /// Sends `line` and waits for its answer, which must be `dev ok`.
    fn line(&self, line: &str) {
        let before = self.answers().len();
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.fifo)
            .expect("the console");
        writeln!(f, "{line}").expect("the console line");
        drop(f);
        let end = Instant::now() + WAIT;
        loop {
            let a = self.answers();
            if a.len() > before {
                assert!(a[before].starts_with("dev ok"), "{line}: {}", a[before]);
                eprintln!("market: {line} -> {}", a[before]);
                return;
            }
            assert!(Instant::now() < end, "{line}: no answer");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// One player: an account with a character, the window's source and
/// what it answered that no wait took yet.
struct Side {
    name: String,
    src: RealmSource,
    seen: Vec<MarketEvent>,
    state: Option<MarketState>,
}

impl Side {
    fn new(spec: &ServerSpec, account: &str, character: &str, class: u8) -> Side {
        let mut s = Session::connect(spec, &client_info(), Timing::default()).expect("connect");
        s.register(account, &format!("{account}-pass-1"))
            .expect("register");
        s.create_character(character, class, false, true)
            .expect("create a character");
        s.select_character(character).expect("select it");
        let session = OnlineSession(Arc::new(Mutex::new(Some(s))));
        let mut side = Side {
            name: account.into(),
            src: RealmSource::new(session, None, None),
            seen: Vec::new(),
            state: None,
        };
        side.src.send(MarketAsk::Hello);
        let state = side.wait("the market's state", |e| match e {
            MarketEvent::State { state, .. } => Some(state.clone()),
            _ => None,
        });
        assert!(state.on, "{account}: the market is on: {state:?}");
        side.state = Some(state);
        side
    }

    fn fee_rev(&self) -> u32 {
        self.state.as_ref().map_or(0, |s| s.fees.fee_rev)
    }

    /// Polls until an event `pick` takes (the others stay seen).
    fn wait<T>(&mut self, what: &str, mut pick: impl FnMut(&MarketEvent) -> Option<T>) -> T {
        let end = Instant::now() + WAIT;
        loop {
            self.seen.extend(self.src.poll());
            for (i, e) in self.seen.iter().enumerate() {
                if let Some(t) = pick(e) {
                    self.seen.remove(i);
                    return t;
                }
            }
            assert!(
                Instant::now() < end,
                "{}: timed out waiting for {what}: {:?}",
                self.name,
                self.seen
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Sends `ask` and waits for its result, which must be done.
    fn call(&mut self, ask: MarketAsk) -> MarketDone {
        let what = format!("{ask:?}");
        let req = self.src.send(ask);
        let d = self.done(req, &what);
        assert_eq!(d.code, MarketCode::Done, "{}: {what}: {d:?}", self.name);
        d
    }

    fn done(&mut self, req: Req, what: &str) -> MarketDone {
        self.wait(what, |e| match e {
            MarketEvent::Done(d) if d.req == req => Some(d.clone()),
            _ => None,
        })
    }

    fn deliveries(&mut self) -> DeliveriesPage {
        let req = self.src.send(MarketAsk::DeliveriesOpen { page: 0 });
        self.wait("deliveries", |e| match e {
            MarketEvent::Deliveries { req: r, page } if *r == req => Some(page.clone()),
            _ => None,
        })
    }

    fn offers(&mut self, which: OffersWhich) -> d2_client::market::trade::OffersPage {
        let req = self.src.send(MarketAsk::OffersOpen {
            which,
            live_only: false,
            page: 0,
        });
        self.wait("offers", |e| match e {
            MarketEvent::Offers { req: r, page } if *r == req => Some(page.clone()),
            _ => None,
        })
    }

    fn post(&mut self, count: u32, mode: ListingMode, gold: Option<u64>) -> u64 {
        let d = self.call(MarketAsk::Post(Post {
            goods: PostGoods::Stack {
                code: "r24".into(),
                count,
            },
            mode,
            price: gold.map(Price::gold),
            days: 3,
            note: String::new(),
            hidden: false,
            fee_rev: self.fee_rev(),
        }));
        d.id.expect("the new listing")
    }

    fn offer(&mut self, listing: u64, gold: u64) -> u64 {
        let d = self.call(MarketAsk::Offer {
            listing,
            price: Price::gold(gold),
            fee_rev: self.fee_rev(),
        });
        d.id.expect("the new offer")
    }

    fn answer(&mut self, offer: u64, how: AnswerHow) {
        self.call(MarketAsk::Answer {
            offer,
            how,
            counter: None,
            fee_rev: self.fee_rev(),
        });
    }

    fn claim_all(&mut self) {
        let d = self.call(MarketAsk::Claim {
            all: true,
            deliveries: Vec::new(),
        });
        assert!(
            d.rows.iter().all(|(_, c)| *c == MarketCode::Done),
            "{}: every row claimed: {d:?}",
            self.name
        );
        assert_eq!(self.deliveries().total, 0, "{}: nothing left", self.name);
    }
}

#[test]
#[ignore = "needs a running d2rust-realm with the market on: tools/mp-test/online-market.sh"]
fn online_market_open_search_post_buy_offer_claim() {
    let t0 = Instant::now();
    let console = Console {
        fifo: env("D2RUST_SMOKE_CONSOLE"),
        log: env("D2RUST_SMOKE_LOG"),
    };
    let spec = ServerSpec {
        id: "market".into(),
        name: "Market".into(),
        game: "d2rust".into(),
        address: env("D2RUST_SMOKE_ADDR"),
        trust: ServerTrust::Pinned(
            parse_fingerprint(&env("D2RUST_SMOKE_CERT")).expect("the certificate"),
        ),
        version: d2_client::launch::VERSION.into(),
    };
    // Open: each account's window gets the realm's state, on.
    let mut seller = Side::new(&spec, "mkts", "Mktseller", 3);
    let mut buyer = Side::new(&spec, "mktb", "Mktbuyer", 1);
    console.line("econ-seed mkts r24:5");
    console.line("econ-seed mktb gold:5000");
    let rev = seller.fee_rev();

    // Post: a fixed price, offers only (accepted), offers only (declined).
    let fixed = seller.post(2, ListingMode::Price, Some(1000));
    let taken = seller.post(1, ListingMode::OffersOnly, None);
    let declined = seller.post(1, ListingMode::OffersOnly, None);
    eprintln!("market: posted {fixed} {taken} {declined}");

    // Search: the buyer finds the three, as stackables named by code.
    let req = buyer.src.send(MarketAsk::Search(MarketQuery {
        groups: vec![Group::Stackable],
        facets: true,
        ..MarketQuery::default()
    }));
    let page = buyer.wait("the results", |e| match e {
        MarketEvent::Results { req: r, page } if *r == req => Some(page.clone()),
        _ => None,
    });
    let ids: Vec<u64> = page.rows.iter().map(|r| r.listing).collect();
    for id in [fixed, taken, declined] {
        assert!(ids.contains(&id), "listing {id} in {ids:?}");
    }
    let row = page.rows.iter().find(|r| r.listing == fixed).unwrap();
    assert_eq!(
        (row.group, row.item.code.as_str()),
        (Group::Stackable, "r24")
    );
    assert_eq!(row.price, Some(Price::gold(1000)));
    assert!(!row.mine);
    let facets = page.facets.expect("the facets");
    assert!(facets
        .groups
        .get(&Group::Stackable)
        .is_some_and(|n| *n >= 3));

    // Buy: looked up, then bought at the price the row showed.
    let req = buyer.src.send(MarketAsk::ListingGet(fixed));
    let detail = buyer.wait("the listing", |e| match e {
        MarketEvent::Listing { req: r, detail } if *r == req => Some(detail.clone()),
        _ => None,
    });
    assert_eq!((detail.state, detail.row.listing), (0, fixed));
    buyer.call(MarketAsk::Buy {
        listing: fixed,
        price_hash: detail.row.price_hash,
        fee_rev: detail.fee_rev,
    });

    // Offer: one accepted, one declined; the seller sees both.
    let o1 = buyer.offer(taken, 600);
    let o2 = buyer.offer(declined, 300);
    let mine = seller.offers(OffersWhich::OnMyListings);
    let on: Vec<(u64, OfferState)> = mine.rows.iter().map(|r| (r.offer, r.state)).collect();
    assert!(on.contains(&(o1, OfferState::Open)), "{on:?}");
    assert!(on.contains(&(o2, OfferState::Open)), "{on:?}");
    let r1 = mine.rows.iter().find(|r| r.offer == o1).unwrap();
    assert_eq!((r1.offered.gold, r1.group), (600, Group::Stackable));
    seller.answer(o1, AnswerHow::Accept);
    seller.answer(o2, AnswerHow::Decline);
    let made = buyer.offers(OffersWhich::MadeByMe);
    let states: Vec<(u64, OfferState)> = made.rows.iter().map(|r| (r.offer, r.state)).collect();
    assert!(states.contains(&(o1, OfferState::Accepted)), "{states:?}");
    assert!(states.contains(&(o2, OfferState::Declined)), "{states:?}");

    // Shop: the declined listing gets a note, then is cancelled; its
    // goods come back to the seller.
    let d = seller.call(MarketAsk::ShopEdit {
        op: ShopOp::Note("one left".into()),
        listings: vec![declined],
        fee_rev: rev,
    });
    assert_eq!(d.rows, [(declined, MarketCode::Done)]);
    let req = seller.src.send(MarketAsk::ShopOpen {
        tab: ShopTab::Shown,
        page: 0,
        sort: ShopSort::Posted,
        desc: true,
    });
    let shop = seller.wait("the shop", |e| match e {
        MarketEvent::Shop { req: r, page } if *r == req => Some(page.clone()),
        _ => None,
    });
    let noted = shop.rows.iter().find(|r| r.row.listing == declined);
    assert_eq!(noted.map(|r| r.row.note.as_str()), Some("one left"));
    seller.call(MarketAsk::ShopEdit {
        op: ShopOp::Cancel,
        listings: vec![declined],
        fee_rev: rev,
    });

    // Claim: the seller's two sales and the cancelled listing; the
    // buyer's two buys and the declined offer's refund, which says why.
    let sp = seller.deliveries();
    let sources: Vec<DeliverySource> = sp.rows.iter().map(|d| d.source).collect();
    assert_eq!(
        sources
            .iter()
            .filter(|s| **s == DeliverySource::Sold)
            .count(),
        2,
        "{sources:?}"
    );
    assert!(sources.contains(&DeliverySource::Cancelled), "{sources:?}");
    seller.claim_all();
    let bp = buyer.deliveries();
    let refund = bp
        .rows
        .iter()
        .find(|d| d.source == DeliverySource::Returned)
        .expect("the declined offer's refund");
    assert_eq!(refund.reason, DeliveryReason::Declined);
    assert_eq!(refund.goods, d2_client::market::source::Goods::Gold(300));
    let ui = MarketUi::new();
    let lines: Vec<String> = deliveries::lines(&bp)
        .iter()
        .map(|l| {
            deliveries::line_parts(&ui, &bp, l)
                .into_iter()
                .map(|(s, _)| s)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("Refund:") && l.ends_with("(declined)")),
        "{lines:?}"
    );
    let bought = bp
        .rows
        .iter()
        .filter(|d| d.source == DeliverySource::Bought)
        .count();
    assert_eq!(bought, 2, "{:?}", bp.rows);
    buyer.claim_all();

    // The price check: the realm's sales of r24 over 30 days.
    let req = buyer.src.send(MarketAsk::Prices {
        of: PriceOf::Stackable("r24".into()),
        days: 30,
    });
    let history = buyer.wait("the price check", |e| match e {
        MarketEvent::Prices { req: r, history } if *r == req => Some(history.clone()),
        _ => None,
    });
    assert_eq!(history.days, 30);
    assert!(history.sold >= 2, "{history:?}");
    eprintln!(
        "market: open, search, post, buy, offer, claim in {:.1} s",
        t0.elapsed().as_secs_f32()
    );
}
