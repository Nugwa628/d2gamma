//! The realm reader: bodies to their owners by kind, never waiting for the
//! session, the stash's answers left in the session, a failed session
//! lost and found again.

use super::*;
use d2_net::msg::lobby::LobbyEvent;
use d2_net::msg::{ErrorCode, Writer};

/// A realm connection in memory: what arrives, what was sent, and whether
/// it is broken.
#[derive(Default)]
struct FakeWire {
    incoming: VecDeque<Vec<u8>>,
    sent: Vec<Vec<u8>>,
    broken: bool,
}

impl RealmWire for FakeWire {
    type Error = String;

    fn send(&mut self, body: Vec<u8>) -> Result<(), String> {
        if self.broken {
            return Err("closed".into());
        }
        self.sent.push(body);
        Ok(())
    }

    fn poll(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.broken {
            return Err("closed".into());
        }
        Ok(self.incoming.pop_front())
    }
}

type Slot = Arc<Mutex<Option<FakeWire>>>;

fn rig() -> (Slot, RealmPump<FakeWire>) {
    let slot: Slot = Arc::new(Mutex::new(Some(FakeWire::default())));
    let pump = RealmPump::new(slot.clone());
    (slot, pump)
}

fn arrive(slot: &Slot, bodies: &[Vec<u8>]) {
    let mut s = slot.lock().unwrap();
    s.as_mut().unwrap().incoming.extend(bodies.iter().cloned());
}

fn sent(slot: &Slot) -> Vec<Vec<u8>> {
    std::mem::take(&mut slot.lock().unwrap().as_mut().unwrap().sent)
}

/// A body of `kind` with one marker byte.
fn body(kind: u8, n: u8) -> Vec<u8> {
    vec![kind, n]
}

fn error(request_kind: u8) -> Vec<u8> {
    ErrorMsg::new(request_kind, ErrorCode::NOT_SUPPORTED).body()
}

/// A game list row (the router reads the kind only).
fn game(id: u8) -> Vec<u8> {
    body(d2_net::msg::lobby::kind::GAME_ENTRY, id)
}

#[test]
fn pump_routes_by_kind() {
    use d2_net::msg::lobby::kind as lobby;
    let (slot, mut pump) = rig();
    let chat_line = LobbyEvent::ChatLine {
        from: "Ann".into(),
        text: "hi".into(),
        at: 5,
    }
    .body();
    let incoming = vec![
        body(0xB1, 1),     // market state
        body(0x74, 2),     // a room line
        body(0x53, 3),     // a ladder head
        body(0xF5, 4),     // a guild notice
        body(0xFB, 5),     // a friend notice
        game(7),           // the lobby's list
        chat_line.clone(), // the old lobby chat
        error(0xB4),       // a market request refused
        error(0x52),       // a ladder query refused
        error(0x72),       // a say refused
        error(lobby::JOIN_GAME),
        body(0x21, 6), // an account answer: no reader in a game
        body(0x60, 7), // a mod's own: no reader in a game
        body(0x55, 8), // a ladder end
        body(0xCE, 9), // a market list end
    ];
    arrive(&slot, &incoming);
    assert_eq!(
        pump.step(Instant::now()),
        Pumped::Done { sent: 0, read: 15 }
    );
    let inbox = pump.inbox();
    assert_eq!(
        inbox.take(Owner::Market),
        [body(0xB1, 1), error(0xB4), body(0xCE, 9)],
        "the market's, in order, its refusal among them"
    );
    assert_eq!(
        inbox.take(Owner::Social),
        [
            body(0x74, 2),
            body(0xF5, 4),
            body(0xFB, 5),
            chat_line,
            error(0x72)
        ]
    );
    assert_eq!(
        inbox.take(Owner::Ladder),
        [body(0x53, 3), error(0x52), body(0x55, 8)]
    );
    assert_eq!(inbox.take(Owner::Lobby), [game(7), error(lobby::JOIN_GAME)]);
    assert_eq!(inbox.link().dropped, 2, "the account's and the mod's");
    for o in Owner::ALL {
        assert_eq!(inbox.waiting(o), 0, "{o:?} taken");
    }
}

#[test]
fn owners_of_the_contract_ranges() {
    for k in 0x70..=0x7F {
        assert_eq!(owner_of_kind(k), Some(Owner::Social), "{k:#x}");
    }
    for k in 0xF0..=0xFF {
        assert_eq!(owner_of_kind(k), Some(Owner::Social), "{k:#x}");
    }
    for k in 0x50..=0x57 {
        assert_eq!(owner_of_kind(k), Some(Owner::Ladder), "{k:#x}");
    }
    for k in (0x40..=0x48).chain(0x4B..=0x4F).chain(0x58..=0x5F) {
        assert_eq!(owner_of_kind(k), Some(Owner::Lobby), "{k:#x}");
    }
    // The old lobby chat is the chat's.
    assert_eq!(owner_of_kind(0x49), Some(Owner::Social));
    assert_eq!(owner_of_kind(0x4A), Some(Owner::Social));
    for k in 0xB0..=0xCF {
        assert_eq!(owner_of_kind(k), Some(Owner::Market), "{k:#x}");
    }
    // Stash, game-server link, bounties, mods, account, session: no
    // reader here.
    for k in [
        0x10, 0x20, 0x3F, 0x60, 0x6F, 0x80, 0xA0, 0xAF, 0xD0, 0xE0, 0xEF,
    ] {
        assert_eq!(owner_of_kind(k), None, "{k:#x}");
    }
    assert_eq!(owner_of(&[]), None);
    assert_eq!(
        owner_of(&[msg::kind::ERROR, 1]),
        None,
        "an unreadable error"
    );
}

#[test]
fn pump_keeps_the_newest_bodies_nobody_reads() {
    let (slot, mut pump) = rig();
    let many: Vec<Vec<u8>> = (0..QUEUE_MAX + 10)
        .map(|i| Writer::new(0x74).u32(i as u32).finish())
        .collect();
    arrive(&slot, &many);
    pump.step(Instant::now());
    let kept = pump.inbox().take(Owner::Social);
    assert_eq!(kept.len(), QUEUE_MAX);
    assert_eq!(kept[0], many[10], "the oldest go");
    assert_eq!(kept.last(), many.last());
    assert_eq!(pump.inbox().link().dropped, 10);
}

#[test]
fn a_new_game_list_replaces_the_old_one() {
    use d2_net::msg::lobby::kind as lobby;
    let (slot, mut pump) = rig();
    let begin = |n| body(lobby::GAME_LIST_BEGIN, n);
    let end = body(lobby::GAME_LIST_END, 0);
    let gone = body(lobby::GAME_GONE, 1);
    arrive(&slot, &[begin(1), game(1), end.clone(), gone, begin(2)]);
    pump.step(Instant::now());
    arrive(&slot, &[game(2), game(3), end.clone()]);
    pump.step(Instant::now());
    assert_eq!(
        pump.inbox().take(Owner::Lobby),
        [begin(2), game(2), game(3), end],
        "the realm's latest list only"
    );
}

#[test]
fn pump_never_blocks() {
    let (slot, mut pump) = rig();
    pump.outbox().send(body(0x72, 1));
    arrive(&slot, &[body(0x74, 1)]);
    {
        // The stash's worker (or a reconnect) holds the session.
        let _held = slot.lock().unwrap();
        let t = Instant::now();
        assert_eq!(pump.step(Instant::now()), Pumped::Busy);
        assert!(t.elapsed() < Duration::from_millis(100), "no wait");
        assert_eq!(pump.outbox().waiting(), 1, "kept for the next frame");
        assert_eq!(pump.inbox().waiting(Owner::Social), 0);
    }
    pump.outbox().send(body(0x76, 2));
    assert_eq!(pump.step(Instant::now()), Pumped::Done { sent: 2, read: 1 });
    assert_eq!(sent(&slot), [body(0x72, 1), body(0x76, 2)], "in order");
    assert_eq!(pump.inbox().take(Owner::Social), [body(0x74, 1)]);
}

#[test]
fn no_session_keeps_the_outbox() {
    let slot: Slot = Arc::new(Mutex::new(None));
    let mut pump = RealmPump::new(slot.clone());
    pump.outbox().send(body(0x72, 1));
    assert_eq!(pump.step(Instant::now()), Pumped::NoSession);
    assert_eq!(pump.outbox().waiting(), 1);
}

#[test]
fn a_failed_session_is_lost_then_found_again() {
    let (slot, mut pump) = rig();
    let t0 = Instant::now();
    slot.lock().unwrap().as_mut().unwrap().broken = true;
    pump.outbox().send(body(0x72, 1));
    assert_eq!(pump.step(t0), Pumped::Failed);
    let link = pump.inbox().link();
    assert!(link.down);
    assert_eq!(link.losses, 1);
    assert_eq!(pump.outbox().waiting(), 0, "lost with the session");

    // Sent while it is down: lost too, and nothing is tried before RETRY.
    pump.outbox().send(body(0x72, 2));
    assert_eq!(pump.step(t0 + RETRY / 2), Pumped::Waiting);
    assert_eq!(pump.outbox().waiting(), 0);
    // Still broken at the retry: another wait, the same loss.
    assert_eq!(pump.step(t0 + RETRY), Pumped::Waiting);
    assert_eq!(pump.inbox().link().losses, 1);

    // A reconnect put a working session in place.
    *slot.lock().unwrap() = Some(FakeWire::default());
    arrive(&slot, &[body(0xB1, 1), body(0x74, 2)]);
    let back = pump.step(t0 + RETRY * 3);
    assert_eq!(back, Pumped::Done { sent: 0, read: 2 });
    let link = pump.inbox().link();
    assert!(!link.down);
    assert_eq!(link.losses, 1);
    pump.outbox().send(body(0x72, 3));
    pump.step(t0 + RETRY * 3);
    assert_eq!(sent(&slot), [body(0x72, 3)]);
}

#[cfg(feature = "online")]
mod online {
    use super::*;
    use crate::market::realm::RealmSource;
    use crate::market::source::{MarketAsk, MarketEvent, MarketSource};
    use d2_net::msg::market::{
        Fees, FromRealm, MarketCounts, MarketLimits, MarketState, OffReason, ToRealm,
    };

    fn state(req: u64) -> Vec<u8> {
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
            last_req: 0,
            stackables: Vec::new(),
        })
        .body()
    }

    fn hellos(bodies: &[Vec<u8>]) -> Vec<u64> {
        bodies
            .iter()
            .filter_map(|b| ToRealm::parse(b).ok())
            .filter(|m| matches!(m.ask, d2_net::msg::market::Ask::Hello))
            .map(|m| m.req)
            .collect()
    }

    fn states(events: &[MarketEvent]) -> usize {
        events
            .iter()
            .filter(|e| matches!(e, MarketEvent::State { .. }))
            .count()
    }

    /// The market of a game reads its queue and sends through the
    /// outbox; after a lost session it refuses what was in flight and,
    /// once the reader has a session again, asks the realm's state anew.
    #[test]
    fn the_market_reads_its_inbox_and_starts_over_after_a_loss() {
        let (slot, mut pump) = rig();
        let mut m = RealmSource::on_reader(pump.inbox().clone(), pump.outbox().clone(), None, None);
        let hello = m.send(MarketAsk::Hello);
        assert!(m.poll().is_empty());
        pump.step(Instant::now());
        assert_eq!(hellos(&sent(&slot)), [hello]);
        arrive(&slot, &[body(0x74, 1), state(hello)]);
        pump.step(Instant::now());
        let ev = m.poll();
        assert_eq!(states(&ev), 1, "{ev:?}");
        assert_eq!(pump.inbox().waiting(Owner::Social), 1, "the chat's stays");

        // An ask in flight, then the session fails.
        let ask = m.send(MarketAsk::DeliveriesOpen { page: 0 });
        m.poll();
        pump.step(Instant::now());
        slot.lock().unwrap().as_mut().unwrap().broken = true;
        pump.outbox().send(body(0x72, 1));
        assert_eq!(pump.step(Instant::now()), Pumped::Failed);
        let ev = m.poll();
        assert!(
            ev.iter()
                .any(|e| matches!(e, MarketEvent::Done(d) if d.req == ask)),
            "the ask in flight is answered here: {ev:?}"
        );

        // A reconnect: the reader finds a session, the market says hello.
        *slot.lock().unwrap() = Some(FakeWire::default());
        let later = Instant::now() + RETRY * 2;
        pump.step(later);
        assert!(!pump.inbox().link().down);
        let mut again = Vec::new();
        let end = Instant::now() + Duration::from_secs(5);
        while again.is_empty() {
            assert!(Instant::now() < end, "no new hello");
            m.poll();
            pump.step(Instant::now() + RETRY * 2);
            again = hellos(&sent(&slot));
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(again[0] > hello, "numbers go on: {again:?}");
    }

    mod loopback {
        use super::*;
        use d2_net::msg::stash::{StashEvent, StashRequest};
        use d2_net::wire::{ClientHello, ResumeToken, ServerInfo, SessionWelcome};
        use d2_net::Timing;
        use d2_online::{ClientInfo, ServerSpec, ServerTrust, Session, StashAnswer};
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

        /// A realm whose answer to a stash take comes between a chat
        /// line, a market body and a room line.
        async fn serve(_conn: QuicConn, mut control: FramedStream) {
            let Ok(b) = control.recv_async().await else {
                return;
            };
            if ClientHello::parse(&b)
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
                let Ok(StashRequest::Take { req, .. }) = StashRequest::parse(&b) else {
                    continue;
                };
                let done = StashEvent::Done {
                    req,
                    code: 0,
                    count: 1,
                    left: Vec::new(),
                };
                for b in [body(0x74, 1), state(0), done.body(), body(0x75, 2)] {
                    let _ = control.send(b);
                }
            }
        }

        #[test]
        fn pump_keeps_stash_answers_queued() {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap();
            let id = Identity::self_signed().unwrap();
            let cert = id.cert.clone();
            let quic = Arc::new(
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
                    tokio::spawn(serve(conn, control));
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
            let s = Session::connect(&spec, &me(), Timing::default()).unwrap();
            let slot = Arc::new(Mutex::new(Some(s)));
            let mut pump = RealmPump::new(slot.clone());

            let take = StashRequest::Take {
                req: 4,
                from: 1,
                item: Some(2),
                stack: None,
                gold: 0,
                place: 0,
            };
            let mut wait = slot
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .stash_start(&take)
                .unwrap();
            // The reader runs until the room lines are in: the stash's
            // answer came between them.
            let end = Instant::now() + Duration::from_secs(10);
            while pump.inbox().waiting(Owner::Social) < 2 {
                assert!(Instant::now() < end, "the realm's bodies didn't come");
                pump.step(Instant::now());
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(
                pump.inbox().take(Owner::Social),
                [body(0x74, 1), body(0x75, 2)]
            );
            assert_eq!(pump.inbox().take(Owner::Market), [state(0)]);
            assert_eq!(pump.inbox().link().dropped, 0, "nothing else came out");

            let answer = {
                let mut g = slot.lock().unwrap();
                wait.step(g.as_mut().unwrap(), Duration::from_secs(5))
                    .unwrap()
            };
            assert_eq!(
                answer,
                Some(StashAnswer::Done {
                    code: 0,
                    count: 1,
                    left: Vec::new()
                }),
                "the stash's answer waited in the session"
            );
            quic.close();
        }
    }
}
