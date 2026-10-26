//! The market on a session against a scripted server over loopback QUIC:
//! `MarketClient` sends without waiting, and its poll puts a list back
//! together while the lobby's bodies go back to the caller.

use std::time::{Duration, Instant};

use d2_net::msg::lobby::LobbyEvent;
use d2_net::msg::market::{
    Ask, Fees, FromRealm, Goods, Group, ListingMode, ListingRow, MarketCounts, MarketLimits,
    MarketQuery, MarketState, OffReason, Price, ResultsHead, ToRealm,
};
use d2_net::wire::{ClientHello, ResumeToken, ServerInfo, SessionWelcome};
use d2_net::Timing;
use d2_online::{ClientInfo, MarketAnswer, MarketClient, ServerSpec, ServerTrust, Session};
use d2_quic::{fingerprint, FramedStream, Identity, QuicConn, QuicServer};

fn me() -> ClientInfo {
    ClientInfo {
        protocol: 2,
        game: "d2rust".into(),
        build: "0.1.2.0".into(),
        tick_rate: 60,
    }
}

fn info() -> ServerInfo {
    ServerInfo {
        protocol: 2,
        game: "d2rust".into(),
        build: "0.1.2.0".into(),
        tick_rate: 60,
    }
}

fn row(listing: u64) -> ListingRow {
    ListingRow {
        listing,
        group: Group::Set,
        goods: Goods::Item {
            item_id: 9,
            record: vec![0x5A; 600],
        },
        mode: ListingMode::Price,
        price: Some(Price::gold(1_000)),
        price_hash: [3; 32],
        seller: "Werwer".into(),
        seller_sales: 2,
        posted_ms: 1,
        expires_ms: 2,
        note: String::new(),
        offers: 0,
        mine: false,
    }
}

fn chat() -> Vec<u8> {
    LobbyEvent::ChatLine {
        from: "Ann".into(),
        text: "wts shako".into(),
        at: 5,
    }
    .body()
}

/// A market server: the state (with a stored request number) for a hello,
/// a page of two rows with a chat line in between for a search.
async fn serve_market(_conn: QuicConn, mut control: FramedStream) {
    let Ok(body) = control.recv_async().await else {
        return;
    };
    if ClientHello::parse(&body)
        .and_then(|h| h.check(&info()))
        .is_err()
    {
        return;
    }
    let welcome = SessionWelcome {
        tick_rate: 60,
        max_players: 8,
        session: ResumeToken([5; 16]),
        server_name: "Test realm".into(),
        registration_open: true,
        resume: None,
    };
    if control.send(welcome.body()).is_err() {
        return;
    }
    while let Ok(b) = control.recv_async().await {
        let Ok(m) = ToRealm::parse(&b) else { return };
        let req = m.req;
        let out = match m.ask {
            Ask::Hello => vec![FromRealm::State(MarketState {
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
                last_req: 41,
                stackables: Vec::new(),
            })
            .body()],
            Ask::Search(_) => vec![
                FromRealm::ResultsBegin {
                    req,
                    head: ResultsHead {
                        total: 2,
                        page: 0,
                        page_size: 50,
                        rows: 2,
                        now_ms: 7,
                        facets: false,
                    },
                }
                .body(),
                FromRealm::Row { req, row: row(1) }.body(),
                chat(),
                FromRealm::Row { req, row: row(2) }.body(),
                FromRealm::ListEnd { req }.body(),
            ],
            _ => return,
        };
        for body in out {
            if control.send(body).is_err() {
                return;
            }
        }
    }
}

/// Polls until `n` answers are in, keeping the other bodies.
fn answers(c: &mut MarketClient, s: &mut Session, n: usize) -> (Vec<MarketAnswer>, Vec<Vec<u8>>) {
    let until = Instant::now() + Duration::from_secs(5);
    let (mut got, mut others) = (Vec::new(), Vec::new());
    while got.len() < n {
        others.extend(c.poll(s).unwrap());
        got.extend(c.take());
        assert!(Instant::now() < until, "no answer: {got:?}");
        std::thread::sleep(Duration::from_millis(2));
    }
    (got, others)
}

#[test]
fn market_client_over_a_session() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let id = Identity::self_signed().unwrap();
    let cert = id.cert.clone();
    let quic = std::sync::Arc::new(
        QuicServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            id,
            Timing::default(),
            rt.handle().clone(),
        )
        .unwrap(),
    );
    let q = quic.clone();
    rt.spawn(async move {
        while let Some((conn, control)) = q.accept().await {
            tokio::spawn(serve_market(conn, control));
        }
    });
    let spec = ServerSpec {
        id: "t1".into(),
        name: "Test".into(),
        game: "d2rust".into(),
        address: quic.local_addr().unwrap().to_string(),
        trust: ServerTrust::Pinned(fingerprint(&cert)),
        version: "0.1.2.0".into(),
    };
    let mut s = Session::connect(&spec, &me(), Timing::default()).unwrap();
    let mut c = MarketClient::new();

    // A changing request doesn't go out before the state.
    let buy = Ask::Buy {
        listing: 1,
        price_hash: [3; 32],
        fee_rev: 0,
    };
    assert_eq!(c.send(&mut s, buy.clone()).unwrap(), None);
    let t = Instant::now();
    let hello = c.send(&mut s, Ask::Hello).unwrap().unwrap();
    assert!(
        t.elapsed() < Duration::from_millis(100),
        "send returns at once"
    );
    let (got, _) = answers(&mut c, &mut s, 1);
    assert!(
        matches!(&got[..], [MarketAnswer::State(st)] if st.req == hello && st.last_req == 41),
        "{got:?}"
    );

    let search = c
        .send(&mut s, Ask::Search(MarketQuery::default()))
        .unwrap()
        .unwrap();
    assert_eq!(search, 42, "numbered above the realm's stored requests");
    let (got, others) = answers(&mut c, &mut s, 1);
    assert_eq!(others, vec![chat()]);
    match &got[..] {
        [MarketAnswer::Results { req, rows, .. }] => {
            assert_eq!(*req, search);
            assert_eq!(rows, &vec![row(1), row(2)]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(c.in_flight(), 0);
    quic.close();
}
