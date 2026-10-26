//! The shared stash side of a session against a scripted server over
//! loopback QUIC: a page comes with its items, a move gets its own
//! `StashDone`, an earlier request's late answers are dropped (never left
//! for `poll`), and an `ERROR` for the request's kind is the server's
//! error. A slow move is read a slice at a time, the session free between
//! (and its answer never `poll`'s).

use d2_net::msg::stash::{kind, StashCell, StashEvent, StashItem, StashRequest};
use d2_net::msg::{ErrorCode, ErrorMsg};
use d2_net::wire::{ClientHello, ResumeToken, ServerInfo, SessionWelcome};
use d2_net::Timing;
use d2_online::{
    ClientInfo, OnlineError, ServerSpec, ServerTrust, Session, StashAnswer, STASH_WAIT,
};
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

fn session_welcome() -> SessionWelcome {
    SessionWelcome {
        tick_rate: 60,
        max_players: 8,
        session: ResumeToken([5; 16]),
        server_name: "Test realm".into(),
        registration_open: true,
        resume: None,
    }
}

/// A scripted server: accepts connections and runs `script` on each.
struct Server {
    quic: std::sync::Arc<QuicServer>,
    cert: Vec<u8>,
    _rt: tokio::runtime::Runtime,
}

impl Server {
    fn start<F, Fut>(script: F) -> Self
    where
        F: Fn(QuicConn, FramedStream) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
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
        let script = std::sync::Arc::new(script);
        rt.spawn(async move {
            while let Some((conn, control)) = q.accept().await {
                tokio::spawn(script(conn, control));
            }
        });
        Self {
            quic,
            cert,
            _rt: rt,
        }
    }

    fn spec(&self) -> ServerSpec {
        ServerSpec {
            id: "t1".into(),
            name: "Test".into(),
            game: "d2rust".into(),
            address: self.quic.local_addr().unwrap().to_string(),
            trust: ServerTrust::Pinned(fingerprint(&self.cert)),
            version: "0.1.2.0".into(),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.quic.close();
    }
}

async fn handshake(control: &mut FramedStream) -> bool {
    let Ok(body) = control.recv_async().await else {
        return false;
    };
    match ClientHello::parse(&body).and_then(|h| h.check(&info())) {
        Ok(()) => control.send(session_welcome().body()).is_ok(),
        Err(_) => false,
    }
}

fn item(id: u32, page: u8) -> StashItem {
    StashItem {
        id,
        cell: StashCell { page, x: 1, y: 2 },
        size: (1, 1),
        record: vec![0x4A, 0x4D, id as u8],
    }
}

fn page(page: u16, items: u16) -> StashEvent {
    StashEvent::Page {
        tab: 0,
        page,
        gold: 500,
        stacks: Vec::new(),
        items,
    }
}

fn done(req: u64, code: u16, count: u64) -> StashEvent {
    StashEvent::Done {
        req,
        code,
        count,
        left: Vec::new(),
    }
}

/// The stash server: every answer comes after the late answers of
/// earlier requests and a lobby body.
async fn serve_stash(_conn: QuicConn, mut control: FramedStream) {
    if !handshake(&mut control).await {
        return;
    }
    while let Ok(b) = control.recv_async().await {
        let Ok(req) = StashRequest::parse(&b) else {
            continue;
        };
        let out: Vec<Vec<u8>> = match req {
            StashRequest::Open { page: p, .. } => vec![
                // A page asked before, with its item.
                page(p - 1, 1).body(),
                StashEvent::Item(item(1, (p - 1) as u8)).body(),
                d2_net::msg::Writer::new(0x44).u32(7).finish(),
                done(5, 0, 1).body(),
                page(p, 2).body(),
                StashEvent::Item(item(2, p as u8)).body(),
                done(6, 0, 1).body(),
                StashEvent::Item(item(3, p as u8)).body(),
            ],
            StashRequest::Put { req, .. } => {
                vec![done(req - 1, 0, 1).body(), done(req, 87, 0).body()]
            }
            StashRequest::Take { req, .. } => vec![done(req, 0, 1).body()],
            StashRequest::Search { .. } => vec![
                done(4, 0, 1).body(),
                StashEvent::Found {
                    hits: vec![(
                        2,
                        StashCell {
                            page: 3,
                            x: 1,
                            y: 2,
                        },
                    )],
                }
                .body(),
            ],
            StashRequest::Arrange { .. } => {
                vec![ErrorMsg::new(kind::STASH_ARRANGE, ErrorCode::NOT_SUPPORTED).body()]
            }
            _ => continue,
        };
        for b in out {
            let _ = control.send(b);
        }
    }
}

#[test]
fn stash_requests_get_their_own_answers() {
    let srv = Server::start(serve_stash);
    let mut s = Session::connect(&srv.spec(), &me(), Timing::default()).unwrap();

    let open = StashRequest::Open { tab: 0, page: 3 };
    match s.stash(&open, STASH_WAIT).unwrap() {
        StashAnswer::Page {
            tab,
            page,
            gold,
            items,
            ..
        } => {
            assert_eq!((tab, page, gold), (0, 3, 500));
            let ids: Vec<u32> = items.iter().map(|i| i.id).collect();
            assert_eq!(ids, [2, 3], "the page asked, with its own items");
            assert_eq!(items[0].record, [0x4A, 0x4D, 2]);
        }
        other => panic!("expected the page, got {other:?}"),
    }

    let put = StashRequest::Put {
        req: 9,
        unit: 40,
        to: 1,
        gold: 0,
        cell: None,
    };
    assert_eq!(
        s.stash(&put, STASH_WAIT).unwrap(),
        StashAnswer::Done {
            code: 87,
            count: 0,
            left: Vec::new()
        },
        "request 9's own answer, not request 8's"
    );

    let take = StashRequest::Take {
        req: 10,
        from: 1,
        item: Some(2),
        stack: None,
        gold: 0,
        place: 0,
    };
    assert!(matches!(
        s.stash(&take, STASH_WAIT).unwrap(),
        StashAnswer::Done {
            code: 0,
            count: 1,
            ..
        }
    ));

    let search = StashRequest::Search { text: "cap".into() };
    assert_eq!(
        s.stash(&search, STASH_WAIT).unwrap(),
        StashAnswer::Found {
            hits: vec![(
                2,
                StashCell {
                    page: 3,
                    x: 1,
                    y: 2
                }
            )]
        }
    );

    let arrange = StashRequest::Arrange { req: 11, page: 0 };
    match s.stash(&arrange, STASH_WAIT).unwrap_err() {
        OnlineError::Server {
            request_kind, code, ..
        } => {
            assert_eq!(request_kind, kind::STASH_ARRANGE);
            assert_eq!(code, ErrorCode::NOT_SUPPORTED);
        }
        other => panic!("expected the server's error, got {other:?}"),
    }

    // The lobby body waits for `poll`; no stash answer was left there (the
    // stream is in order: all of them came before the last answer).
    let left = s.poll().unwrap().unwrap();
    assert_eq!(left[0], 0x44);
    assert_eq!(s.poll().unwrap(), None);
}

/// A stash server whose game server is slow: a put's answer comes a while
/// after a lobby body; a page is announced, then its item comes late.
async fn serve_slow(_conn: QuicConn, mut control: FramedStream) {
    if !handshake(&mut control).await {
        return;
    }
    while let Ok(b) = control.recv_async().await {
        let Ok(req) = StashRequest::parse(&b) else {
            continue;
        };
        let (first, then): (Vec<Vec<u8>>, Vec<Vec<u8>>) = match req {
            StashRequest::Put { req, .. } => (
                vec![d2_net::msg::Writer::new(0x44).u32(7).finish()],
                vec![done(req, 0, 1).body()],
            ),
            StashRequest::Open { page: p, .. } => (
                vec![page(p, 1).body()],
                vec![StashEvent::Item(item(4, p as u8)).body()],
            ),
            _ => continue,
        };
        for b in first {
            let _ = control.send(b);
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        for b in then {
            let _ = control.send(b);
        }
    }
}

#[test]
fn a_slow_move_leaves_the_session_free_between_reads() {
    let srv = Server::start(serve_slow);
    let mut s = Session::connect(&srv.spec(), &me(), Timing::default()).unwrap();
    let slice = std::time::Duration::from_millis(5);

    let put = StashRequest::Put {
        req: 20,
        unit: 40,
        to: 1,
        gold: 0,
        cell: None,
    };
    let mut w = s.stash_start(&put).unwrap();
    // While the game server works, each read gives the session back at
    // once: the rest of the game (here the lobby's poll) uses it between.
    let mut lobby = None;
    let mut reads = 0;
    let answer = loop {
        let started = std::time::Instant::now();
        let a = w.step(&mut s, slice).unwrap();
        assert!(
            started.elapsed() < std::time::Duration::from_millis(200),
            "a read held the session for the whole move"
        );
        reads += 1;
        if let Some(a) = a {
            break a;
        }
        // The other reader never gets the move's answer.
        if let Some(b) = s.poll().unwrap() {
            assert_eq!(b[0], 0x44, "only the lobby body is the poll's");
            lobby = Some(b);
        }
        assert!(reads < 2000, "no answer");
    };
    assert_eq!(
        answer,
        StashAnswer::Done {
            code: 0,
            count: 1,
            left: Vec::new()
        }
    );
    assert!(reads > 1, "the answer came after several reads");
    assert_eq!(
        lobby.expect("the lobby body came between the reads")[0],
        0x44
    );

    // A page whose item comes late is read across the slices too.
    let open = StashRequest::Open { tab: 0, page: 2 };
    let mut w = s.stash_start(&open).unwrap();
    let page = loop {
        if let Some(a) = w.step(&mut s, slice).unwrap() {
            break a;
        }
    };
    match page {
        StashAnswer::Page { page, items, .. } => {
            assert_eq!(page, 2);
            assert_eq!(items.iter().map(|i| i.id).collect::<Vec<_>>(), [4]);
        }
        other => panic!("expected the page, got {other:?}"),
    }
}
