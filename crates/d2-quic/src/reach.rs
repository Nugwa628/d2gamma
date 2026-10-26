//! What reachability needs from the stream layer (multiplayer v2, N9 and
//! N10; `d2-reach` builds on it). All of it is QUIC.
//!
//! - **Pings.** A ping is a QUIC connection on [`PING_ALPN`] (contracts
//!   11.4): the handshake completes and both ends close at once; the
//!   handshake is the answer. Every [`QuicServer`] answers pings beside
//!   its players. The main server pings a game server's public addresses
//!   from fresh endpoints ([`probe`]) to learn whether strangers get in.
//! - **The observer endpoint.** The main server runs a second, ping-only
//!   endpoint ([`PingServer`]) on its own port and reports the address
//!   each ping came from. A game server pings it from its own endpoint
//!   ([`QuicServer::ping`]); comparing that address with the link's tells
//!   a NAT that keeps one public address per socket (cone) from one that
//!   picks a new one per destination (symmetric).
//! - **A socket shared with punches.** Hole punching sends a few datagrams
//!   from the game server's own UDP socket, so the opening it makes in the
//!   NAT is the one the joiner's QUIC connection uses.
//!   [`QuicServer::bind_socket`] serves on a socket the caller owns
//!   (`d2_reach::SharedSocket` sends punches through it and takes them off
//!   before QUIC sees them).
//!
//! Certificates are pinned by [`fingerprint`](crate::fingerprint), with
//! the same check as [`ServerTrust::Pinned`].

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::{HandshakeData, QuicClientConfig, QuicServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use d2_net::wire::{ConnectError, Timing};

use crate::ping::PING_ALPN;
use crate::{io_err, provider, server, Identity, QuicServer, ServerTrust, TrustCheck, SERVER_NAME};

/// How long an accepted ping may take to finish its handshake.
const PING_HANDSHAKE: Duration = Duration::from_secs(5);

/// How long a [`probe`] waits for its endpoint's goodbye to go out.
const PROBE_CLOSE: Duration = Duration::from_millis(200);

/// Whether `conn` agreed on [`PING_ALPN`].
pub(crate) fn is_ping(conn: &quinn::Connection) -> bool {
    conn.handshake_data()
        .and_then(|d| d.downcast::<HandshakeData>().ok())
        .and_then(|d| d.protocol)
        .is_some_and(|p| p == PING_ALPN)
}

/// Ends an answered ping: the handshake was the answer.
pub(crate) fn close_ping(conn: &quinn::Connection) {
    conn.close(0u32.into(), b"pong");
}

/// The client config of a ping that accepts only the certificate `pin`.
pub fn ping_client_config(pin: [u8; 32]) -> Result<quinn::ClientConfig, ConnectError> {
    let mut crypto = rustls::ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| io_err(e.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(TrustCheck {
            trust: ServerTrust::Pinned(pin),
            provider: provider(),
        }))
        .with_no_client_auth();
    crypto.alpn_protocols = vec![PING_ALPN.to_vec()];
    let quic = QuicClientConfig::try_from(crypto).map_err(|e| io_err(e.to_string()))?;
    Ok(quinn::ClientConfig::new(Arc::new(quic)))
}

/// Pings `to` from `endpoint`: the handshake with the certificate `pin`
/// completes within `timeout`, then the connection closes.
async fn ping_from(
    endpoint: &quinn::Endpoint,
    to: SocketAddr,
    pin: [u8; 32],
    timeout: Duration,
) -> Result<(), ConnectError> {
    let connecting = endpoint
        .connect_with(ping_client_config(pin)?, to, SERVER_NAME)
        .map_err(|e| io_err(format!("ping: {e}")))?;
    let conn = tokio::time::timeout(timeout, connecting)
        .await
        .map_err(|_| io_err("no answer to the ping in time"))?
        .map_err(|e| io_err(format!("ping: {e}")))?;
    conn.close(0u32.into(), b"ping");
    Ok(())
}

/// The main server's attempt: pings `to` from a fresh endpoint, one the
/// game server has never talked to, so only a port open to strangers or a
/// router mapping lets it in. True when the handshake with the certificate
/// `pin` completed within `timeout`. Call on a tokio runtime.
pub async fn probe(to: SocketAddr, pin: [u8; 32], timeout: Duration) -> bool {
    let local: SocketAddr = if to.is_ipv6() {
        (Ipv6Addr::UNSPECIFIED, 0).into()
    } else {
        (Ipv4Addr::UNSPECIFIED, 0).into()
    };
    let Ok(endpoint) = quinn::Endpoint::client(local) else {
        return false;
    };
    let ok = ping_from(&endpoint, to, pin, timeout).await.is_ok();
    endpoint.close(0u32.into(), b"probe");
    let _ = tokio::time::timeout(PROBE_CLOSE, endpoint.wait_idle()).await;
    ok
}

impl QuicServer {
    /// [`QuicServer::bind`] on a socket the caller owns, so it can send
    /// other datagrams (hole punches) from the endpoint's own address. The
    /// endpoint starts on a loopback socket and moves to `socket` before it
    /// takes any connection. Call where `socket` was registered: on `rt`.
    pub fn bind_socket(
        socket: Arc<dyn quinn::AsyncUdpSocket>,
        identity: Identity,
        timing: Timing,
        rt: Handle,
    ) -> Result<Self, ConnectError> {
        let start: SocketAddr = if socket.local_addr()?.is_ipv6() {
            (Ipv6Addr::LOCALHOST, 0).into()
        } else {
            (Ipv4Addr::LOCALHOST, 0).into()
        };
        let server = Self::bind(start, identity, timing, rt)?;
        server.endpoint.rebind_abstract(socket)?;
        Ok(server)
    }

    /// Pings `to` from this endpoint, so the other end sees the address
    /// players reach: the game server's ping to the main server's observer
    /// endpoint. Ok when the handshake with the certificate `pin`
    /// completed within `timeout`. Call on the runtime the server was
    /// bound on.
    pub async fn ping(
        &self,
        to: SocketAddr,
        pin: [u8; 32],
        timeout: Duration,
    ) -> Result<(), ConnectError> {
        ping_from(&self.endpoint, to, pin, timeout).await
    }
}

/// The main server's observer endpoint: answers pings only, and reports
/// the address each came from. Players and game-server links are refused
/// by their ALPN. Dropping it closes the endpoint.
pub struct PingServer {
    endpoint: quinn::Endpoint,
}

impl std::fmt::Debug for PingServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PingServer")
            .field("addr", &self.endpoint.local_addr().ok())
            .finish()
    }
}

impl PingServer {
    /// Listens on `addr` (port 0 picks a free one), presenting `identity`,
    /// on the runtime `rt`. Returns the server and the addresses the pings
    /// it answers come from, as it sees them.
    pub fn bind(
        addr: SocketAddr,
        identity: &Identity,
        rt: Handle,
    ) -> Result<(Self, mpsc::UnboundedReceiver<SocketAddr>), ConnectError> {
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key.clone()));
        let mut crypto = rustls::ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| io_err(e.to_string()))?
            .with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(identity.cert.clone())], key)
            .map_err(|e| io_err(format!("certificate: {e}")))?;
        crypto.alpn_protocols = vec![PING_ALPN.to_vec()];
        let quic = QuicServerConfig::try_from(crypto).map_err(|e| io_err(e.to_string()))?;
        let config = quinn::ServerConfig::with_crypto(Arc::new(quic));
        let endpoint = {
            let _guard = rt.enter();
            quinn::Endpoint::server(config, addr)?
        };
        let (tx, rx) = mpsc::unbounded_channel();
        let ep = endpoint.clone();
        rt.spawn(async move {
            while let Some(incoming) = ep.accept().await {
                // Address validation first, like the game endpoint: the
                // address reported is one the client proved.
                let validated = incoming.remote_address_validated();
                if server::answer(validated, incoming.may_retry()) == server::Answer::Retry {
                    let _ = incoming.retry();
                    continue;
                }
                let tx = tx.clone();
                tokio::spawn(async move {
                    if let Ok(Ok(conn)) = tokio::time::timeout(PING_HANDSHAKE, incoming).await {
                        let _ = tx.send(conn.remote_address());
                        close_ping(&conn);
                    }
                });
            }
        });
        Ok((Self { endpoint }, rx))
    }

    /// The address it listens on.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.endpoint.local_addr()
    }
}

impl Drop for PingServer {
    fn drop(&mut self) {
        self.endpoint.close(0u32.into(), b"server closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{fingerprint, QuicClient};
    use tokio::runtime::Runtime;

    fn rt() -> Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
    }

    fn loopback() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    const WAIT: Duration = Duration::from_secs(5);

    /// A game endpoint answers pings pinned to its certificate, refuses a
    /// wrong pin, and a ping never shows up as a player.
    #[test]
    fn game_endpoint_answers_pings_beside_players() {
        let rt = rt();
        let identity = Identity::self_signed().unwrap();
        let pin = fingerprint(&identity.cert);
        let server =
            QuicServer::bind(loopback(), identity, Timing::default(), rt.handle().clone()).unwrap();
        let addr = server.local_addr().unwrap();
        rt.block_on(async {
            assert!(probe(addr, pin, WAIT).await, "the ping is answered");
            assert!(!probe(addr, [7; 32], WAIT).await, "another certificate");
        });

        let (client, mut control) =
            QuicClient::connect(addr, ServerTrust::Pinned(pin), Timing::default()).unwrap();
        control.send(b"hello".to_vec()).unwrap();
        let (_conn, mut arrived) = rt
            .block_on(async { tokio::time::timeout(WAIT, server.accept()).await })
            .unwrap()
            .unwrap();
        let deadline = std::time::Instant::now() + WAIT;
        let body = loop {
            if let Some(b) = arrived.recv().unwrap() {
                break b;
            }
            assert!(std::time::Instant::now() < deadline, "no body arrived");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(
            body, b"hello",
            "the first arrival is the player, not a ping"
        );
        drop(client);
    }

    /// A server on a socket the caller owns listens there and serves both
    /// players and pings.
    #[test]
    fn bind_socket_serves_on_the_given_socket() {
        let rt = rt();
        let identity = Identity::self_signed().unwrap();
        let pin = fingerprint(&identity.cert);
        let std_socket = std::net::UdpSocket::bind(loopback()).unwrap();
        let own = std_socket.local_addr().unwrap();
        std_socket.set_nonblocking(true).unwrap();
        let socket = {
            let _g = rt.enter();
            quinn::Runtime::wrap_udp_socket(&quinn::TokioRuntime, std_socket).unwrap()
        };
        let server =
            QuicServer::bind_socket(socket, identity, Timing::default(), rt.handle().clone())
                .unwrap();
        assert_eq!(server.local_addr().unwrap(), own);
        rt.block_on(async { assert!(probe(own, pin, WAIT).await) });
        let (_client, mut control) =
            QuicClient::connect(own, ServerTrust::Pinned(pin), Timing::default()).unwrap();
        control.send(b"hi".to_vec()).unwrap();
        let arrived = rt.block_on(async { tokio::time::timeout(WAIT, server.accept()).await });
        assert!(
            matches!(arrived, Ok(Some(_))),
            "a player arrives on the socket"
        );
    }

    /// A game server's ping to the observer endpoint comes from its game
    /// socket, and the observer reports that address. The observer takes
    /// no players.
    #[test]
    fn observer_sees_the_game_socket() {
        let rt = rt();
        let realm = Identity::self_signed().unwrap();
        let realm_pin = fingerprint(&realm.cert);
        let (observer, mut seen) =
            PingServer::bind(loopback(), &realm, rt.handle().clone()).unwrap();
        let observer_addr = observer.local_addr().unwrap();

        let node = QuicServer::bind(
            loopback(),
            Identity::self_signed().unwrap(),
            Timing::default(),
            rt.handle().clone(),
        )
        .unwrap();
        let node_addr = node.local_addr().unwrap();
        rt.block_on(async {
            node.ping(observer_addr, realm_pin, WAIT).await.unwrap();
            let from = tokio::time::timeout(WAIT, seen.recv())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(from, node_addr);
            assert!(node.ping(observer_addr, [1; 32], WAIT).await.is_err());
        });

        let player = QuicClient::connect(
            observer_addr,
            ServerTrust::Pinned(realm_pin),
            Timing {
                keepalive: Duration::from_millis(200),
                timeout: Duration::from_secs(2),
            },
        );
        assert!(player.is_err(), "the observer refuses the player ALPN");
    }
}
