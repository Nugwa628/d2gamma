//! The link to the main server (contracts 11.2): out from the players'
//! endpoint, pinned to the bundle's certificate, `NodeHello` first. Once
//! welcomed the game server prints its ready line and sends `Load` every
//! heartbeat. A lost link is retried with a growing pause (it prints
//! `link lost reason=TEXT` once, and its ready line again when it links
//! again); a refusal (another version, game data or mod realm, an unknown
//! certificate) ends the program, but for a game server with an updater
//! refused for its version (`NodeBuild`): it links again later, while the
//! updater fetches the main server's version.
//!
//! The welcome's ticket keys go to [`Tickets`], and so do the main
//! server's seat offers and game closes; the seat offers nobody claimed in
//! time are answered `SeatRefused` (`SeatGone`). The games' messages
//! (`StartGame`, `SeatOffer`, `SaveAck`, `Kick`, `CloseGame`, `ItemIds`)
//! and the seat records on the link's streams go to [`NodeGames`], which
//! sends its own up the link while it is up; `Load` is theirs. `Update`
//! (also before the welcome, ahead of a refusal) goes to the [`Updater`].

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use d2_link::streams::{self, What};
use d2_link::{LinkRx, LinkTx, Refusal, HELLO_WAIT};
use d2_net::msg::node::{NodeHello, NodeToRealm, RealmToNode};
use d2_net::msg::ErrorCode;
use d2_net::wire::Timing;
use d2_net::NetError;
use d2_quic::{Identity, QuicConn, QuicServer, ServerTrust, NODE_ALPN};
use d2_service::log::ServerLog;
use d2_service::ready::ReadyLine;

use crate::seats::NodeGames;
use crate::tickets::Tickets;
use crate::update::Updater;

/// How often unclaimed seat offers are looked for.
const OFFER_CHECK: Duration = Duration::from_secs(1);

/// The first pause before linking again, and the longest.
const RETRY_FIRST: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(10);

/// What the link needs.
pub struct LinkSetup {
    pub id: String,
    pub realm_addr: SocketAddr,
    pub realm_cert: [u8; 32],
    pub identity: Identity,
    /// The certificate's fingerprint, for the ready line.
    pub cert_sha256: String,
    /// The players' endpoint's address.
    pub listen: SocketAddr,
    pub hello: NodeHello,
    pub caps: Vec<&'static str>,
    /// The join tickets' check, shared with the players' endpoint.
    pub tickets: Arc<Tickets>,
    /// The games, shared with the players' endpoint.
    pub games: Arc<NodeGames>,
    pub updater: Arc<Updater>,
    pub log: Arc<ServerLog>,
}

/// How one link ended.
enum Ended {
    /// For good.
    Refused(Refusal),
    /// For now: `reason` is one word for the `link lost` line.
    Lost {
        reason: &'static str,
        detail: String,
        linked: bool,
    },
}

fn lost(reason: &'static str, detail: impl Into<String>, linked: bool) -> Ended {
    Ended::Lost {
        reason,
        detail: detail.into(),
        linked,
    }
}

/// Links, and links again, until the main server refuses: then returns
/// its refusal.
pub async fn run(quic: Arc<QuicServer>, s: LinkSetup) -> Refusal {
    let mut pause = RETRY_FIRST;
    loop {
        match once(&quic, &s).await {
            // Its updater fetches the main server's version meanwhile.
            Ended::Refused(r) if r.code == ErrorCode::NODE_BUILD && s.updater.managed() => {
                s.log.line(&format!(
                    "the main server at {} refused the link: {}: {}; again in {} s",
                    s.realm_addr,
                    r.code,
                    r.text,
                    RETRY_MAX.as_secs()
                ));
                tokio::time::sleep(RETRY_MAX).await;
            }
            Ended::Refused(r) => {
                s.log.line(&format!(
                    "the main server at {} refused the link: {}: {}",
                    s.realm_addr, r.code, r.text
                ));
                return r;
            }
            Ended::Lost {
                reason,
                detail,
                linked,
            } => {
                if linked {
                    d2_service::ready::print_line(format_args!("link lost reason={reason}"));
                    pause = RETRY_FIRST;
                }
                s.log.line(&format!(
                    "link to {}: {detail}; again in {} s",
                    s.realm_addr,
                    pause.as_secs()
                ));
                tokio::time::sleep(pause).await;
                pause = (pause * 2).min(RETRY_MAX);
            }
        }
    }
}

/// One link, from the connect to its end.
async fn once(quic: &QuicServer, s: &LinkSetup) -> Ended {
    let opened = quic
        .connect(
            s.realm_addr,
            ServerTrust::Pinned(s.realm_cert),
            NODE_ALPN,
            &s.identity,
            Timing::default(),
        )
        .await;
    let (conn, control) = match opened {
        Ok(x) => x,
        Err(e) => return lost("connect", format!("connecting: {e}"), false),
    };
    let (tx, mut rx) = d2_link::start::<NodeToRealm, RealmToNode>(control);
    let reader = tokio::spawn(read_records(conn.clone(), s.games.clone()));
    let ended = linked(&conn, &tx, &mut rx, s).await;
    reader.abort();
    s.tickets.unlinked();
    s.games.unlinked();
    tx.close();
    conn.close(0, "bye");
    ended
}

async fn linked(
    conn: &QuicConn,
    tx: &LinkTx<NodeToRealm>,
    rx: &mut LinkRx<RealmToNode>,
    s: &LinkSetup,
) -> Ended {
    if tx.send_msg(&NodeToRealm::Hello(s.hello.clone())).is_err() {
        return lost("closed", "the link closed before the hello", false);
    }
    let welcome = loop {
        match tokio::time::timeout(HELLO_WAIT, rx.recv()).await {
            Ok(Ok(RealmToNode::Welcome(w))) => break w,
            Ok(Ok(RealmToNode::Refused { code, text })) => {
                return Ended::Refused(Refusal { code, text });
            }
            // A catch-up comes ahead of a refusal.
            Ok(Ok(m @ RealmToNode::Update { .. })) => take(&m, s),
            Ok(Ok(other)) => {
                return lost(
                    "protocol",
                    format!("kind {:#04x} before the welcome", other.kind()),
                    false,
                );
            }
            Ok(Err(e)) => return lost(word(&e), format!("waiting for the welcome: {e}"), false),
            Err(_) => return lost("timeout", "no answer to the hello", false),
        }
    };
    s.log.line(&format!(
        "linked to {} ({}) as {}, seen from {}; heartbeat {} ms",
        s.realm_addr, welcome.realm, welcome.node, welcome.seen_addr, welcome.heartbeat_ms
    ));
    match s.tickets.linked(&welcome) {
        Ok(ids) => s.log.line(&format!("join tickets: key ids {ids:?}")),
        Err(e) => return lost("protocol", format!("the welcome's ticket keys: {e}"), false),
    }
    s.games.linked(tx.clone(), conn.clone(), &welcome);
    ReadyLine::new()
        .field("program", "node")
        .field("node", &s.id)
        .field("listen", s.listen)
        .field("cert_sha256", &s.cert_sha256)
        .field("realm", s.realm_addr)
        .field("linked", 1)
        .field("caps", s.caps.join(","))
        .print();
    let mut beat = tokio::time::interval(Duration::from_millis(welcome.heartbeat_ms.into()));
    let mut offers = tokio::time::interval(OFFER_CHECK);
    loop {
        tokio::select! {
            _ = beat.tick() => {
                if tx.send_msg(&NodeToRealm::Load(s.games.load())).is_err() {
                    return lost("closed", "the link closed", true);
                }
            }
            _ = offers.tick() => {
                for m in s.tickets.expire(SystemTime::now()) {
                    if let NodeToRealm::SeatRefused { lease, .. } = m {
                        s.games.forget_offer(lease);
                    }
                    let _ = tx.send_msg(&m);
                }
                s.games.tick();
            }
            got = rx.recv() => match got {
                Ok(m) => take(&m, s),
                Err(e) => {
                    let why = conn_reason(conn).unwrap_or_else(|| e.to_string());
                    return lost(word(&e), format!("the link ended: {why}"), true);
                }
            },
        }
    }
}

/// Takes one message of the main server: its ticket part, then its games
/// part (a seat offer and a game's close are both's).
fn take(m: &RealmToNode, s: &LinkSetup) {
    if let RealmToNode::Update {
        phase,
        version,
        text,
        ..
    } = m
    {
        let actions = s.updater.on_update(*phase, version, text);
        s.updater.run(actions, &s.games);
        return;
    }
    let tickets = s.tickets.on_message(m, SystemTime::now());
    let games = s.games.on_message(m);
    if !tickets && !games {
        s.log
            .line(&format!("link: kind {:#04x} not handled yet", m.kind()));
    }
}

/// Reads the seat records the main server sends on the link's streams and
/// hands them to the games, each stream on its own task; a broken stream
/// is dropped. Ends with the connection.
async fn read_records(conn: QuicConn, games: Arc<NodeGames>) {
    while let Some(stream) = conn.accept_uni().await {
        let games = games.clone();
        tokio::spawn(async move {
            if let Ok(c) = streams::recv(stream).await {
                if let What::Record { lease } = c.what {
                    games.record(lease, c.bytes);
                }
            }
        });
    }
}

/// One word for the `link lost` line.
fn word(e: &NetError) -> &'static str {
    match e {
        NetError::TimedOut => "timeout",
        NetError::Malformed => "protocol",
        NetError::Closed => "closed",
        NetError::Refused(_) => "refused",
    }
}

/// Why the connection closed, when the other end said.
fn conn_reason(conn: &QuicConn) -> Option<String> {
    conn.close_reason()
}
