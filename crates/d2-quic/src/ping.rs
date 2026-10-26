//! Pings over QUIC: how a client measures its round trip to a game server
//! before choosing where a game runs (contracts section 11.4).
//!
//! A ping is a QUIC handshake on the application protocol [`PING_ALPN`]
//! against the server's pinned certificate: the server completes the
//! handshake and closes the connection at once, without a stream. The
//! round trip is QUIC's own estimate from the handshake's acknowledgements
//! (the connection's RTT), so it is the same path and protocol a game
//! uses, and the server's time making keys doesn't count. There is no other
//! wire protocol for pings.
//!
//! - [`PingClient`]: one client endpoint for every ping of a session.
//! - [`PingServer`]: answers pings on a port of its own, at most
//!   [`PING_LIMIT`] per [`PING_WINDOW`] for each address ([`PingLimit`]);
//!   attempts from unvalidated addresses are retried first, as
//!   [`crate::QuicServer`] does.
//! - Every [`crate::QuicServer`] answers pings beside its players too
//!   ([`crate::reach`]), so a game server's own port takes them.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::runtime::{Handle, Runtime};

use crate::{client_runtime, io_err, provider, server, ConnectError, Identity, ServerTrust};
use crate::{TrustCheck, SERVER_NAME};

/// The application protocol of a ping.
pub const PING_ALPN: &[u8] = b"d2rust-ping/1";
/// The most pings a server answers for one address in [`PING_WINDOW`].
pub const PING_LIMIT: usize = 10;
pub const PING_WINDOW: Duration = Duration::from_secs(10);
/// The application close code of an answered ping.
const PONG: u32 = 0;

/// Pings from one client: an endpoint per address family, made when first
/// needed, on a small runtime of its own.
pub struct PingClient {
    v4: Option<quinn::Endpoint>,
    v6: Option<quinn::Endpoint>,
    /// Dropped last: the endpoints run on it.
    rt: Runtime,
}

impl std::fmt::Debug for PingClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PingClient").finish_non_exhaustive()
    }
}

impl PingClient {
    pub fn new() -> Result<Self, ConnectError> {
        Ok(Self {
            v4: None,
            v6: None,
            rt: client_runtime()?,
        })
    }

    fn endpoint(&mut self, v6: bool) -> Result<quinn::Endpoint, ConnectError> {
        let slot = if v6 { &mut self.v6 } else { &mut self.v4 };
        if let Some(e) = slot {
            return Ok(e.clone());
        }
        let local: SocketAddr = if v6 { "[::]:0" } else { "0.0.0.0:0" }
            .parse()
            .expect("an address");
        let e = {
            let _guard = self.rt.enter();
            quinn::Endpoint::client(local)?
        };
        *slot = Some(e.clone());
        Ok(e)
    }

    /// One ping: the round trip to the server at `addr`, whose certificate
    /// must match `trust`. Fails when it doesn't answer within `wait`, is
    /// not that server, or doesn't take pings.
    pub fn ping(
        &mut self,
        addr: SocketAddr,
        trust: ServerTrust,
        wait: Duration,
    ) -> Result<Duration, ConnectError> {
        let mut crypto = rustls::ClientConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| io_err(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TrustCheck {
                trust,
                provider: provider(),
            }))
            .with_no_client_auth();
        crypto.alpn_protocols = vec![PING_ALPN.to_vec()];
        let quic = QuicClientConfig::try_from(crypto).map_err(|e| io_err(e.to_string()))?;
        let mut config = quinn::ClientConfig::new(Arc::new(quic));
        let mut t = quinn::TransportConfig::default();
        t.max_idle_timeout(quinn::IdleTimeout::try_from(wait).ok());
        config.transport_config(Arc::new(t));
        let endpoint = self.endpoint(addr.is_ipv6())?;
        let start = Instant::now();
        self.rt.block_on(async {
            let connecting = endpoint
                .connect_with(config, addr, SERVER_NAME)
                .map_err(|e| io_err(format!("ping: {e}")))?;
            let conn = match tokio::time::timeout(wait, connecting).await {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => return Err(io_err(format!("ping: {e}"))),
                Err(_) => return Err(io_err("ping: no answer in time")),
            };
            let rtt = conn.rtt();
            conn.close(PONG.into(), b"");
            // QUIC's estimate; the handshake's own time if it has none.
            Ok(if rtt.is_zero() { start.elapsed() } else { rtt })
        })
    }
}

/// How many pings each address got lately: at most [`PING_LIMIT`] in any
/// [`PING_WINDOW`].
#[derive(Debug, Default)]
pub struct PingLimit {
    seen: HashMap<IpAddr, Vec<Instant>>,
}

impl PingLimit {
    /// Whether to answer a ping from `ip` at `now` (and count it if so).
    pub fn allow(&mut self, ip: IpAddr, now: Instant) -> bool {
        let fresh = |t: &Instant| now.saturating_duration_since(*t) < PING_WINDOW;
        // Forget quiet addresses now and then, so the table stays small.
        if self.seen.len() > 4096 {
            self.seen.retain(|_, ts| ts.iter().any(fresh));
        }
        let ts = self.seen.entry(ip).or_default();
        ts.retain(fresh);
        if ts.len() >= PING_LIMIT {
            return false;
        }
        ts.push(now);
        true
    }
}

/// A server endpoint that only answers pings.
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
    /// Answers pings on `addr` (port 0 picks a free port), presenting
    /// `identity`, on the runtime `rt`.
    pub fn bind(addr: SocketAddr, identity: &Identity, rt: Handle) -> Result<Self, ConnectError> {
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key.clone()));
        let mut crypto = rustls::ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| io_err(e.to_string()))?
            .with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(identity.cert.clone())], key)
            .map_err(|e| io_err(format!("certificate: {e}")))?;
        crypto.alpn_protocols = vec![PING_ALPN.to_vec()];
        let quic = QuicServerConfig::try_from(crypto).map_err(|e| io_err(e.to_string()))?;
        let mut config = quinn::ServerConfig::with_crypto(Arc::new(quic));
        let mut t = quinn::TransportConfig::default();
        t.max_concurrent_bidi_streams(0u32.into())
            .max_concurrent_uni_streams(0u32.into());
        config.transport_config(Arc::new(t));
        let endpoint = {
            let _guard = rt.enter();
            quinn::Endpoint::server(config, addr)?
        };
        let ep = endpoint.clone();
        rt.spawn(async move {
            let mut limit = PingLimit::default();
            while let Some(incoming) = ep.accept().await {
                if server::answer(incoming.remote_address_validated(), incoming.may_retry())
                    == server::Answer::Retry
                {
                    let _ = incoming.retry();
                    continue;
                }
                if !limit.allow(incoming.remote_address().ip(), Instant::now()) {
                    incoming.refuse();
                    continue;
                }
                tokio::spawn(async move {
                    let done = tokio::time::timeout(PING_WINDOW, async { incoming.await }).await;
                    if let Ok(Ok(conn)) = done {
                        conn.close(PONG.into(), b"");
                    }
                });
            }
        });
        Ok(Self { endpoint })
    }

    /// The address it listens on.
    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.endpoint.local_addr()
    }

    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"");
    }
}

impl Drop for PingServer {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fingerprint;

    fn runtime() -> Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap()
    }

    const WAIT: Duration = Duration::from_secs(2);

    #[test]
    fn a_ping_measures_the_round_trip_to_the_pinned_server() {
        let rt = runtime();
        let id = Identity::self_signed().unwrap();
        let srv =
            PingServer::bind("127.0.0.1:0".parse().unwrap(), &id, rt.handle().clone()).unwrap();
        let addr = srv.local_addr().unwrap();
        let mut c = PingClient::new().unwrap();
        let pin = ServerTrust::Pinned(fingerprint(&id.cert));
        for _ in 0..3 {
            let rtt = c.ping(addr, pin.clone(), WAIT).unwrap();
            assert!(
                rtt > Duration::ZERO && rtt < Duration::from_millis(500),
                "{rtt:?}"
            );
        }
        // Another server's pin: no round trip.
        let other = Identity::self_signed().unwrap();
        assert!(c
            .ping(addr, ServerTrust::Pinned(fingerprint(&other.cert)), WAIT)
            .is_err());
    }

    /// A game port answers pings beside its players (contracts 11.4).
    #[test]
    fn a_game_port_answers_pings() {
        let rt = runtime();
        let id = Identity::self_signed().unwrap();
        let game = crate::QuicServer::bind(
            "127.0.0.1:0".parse().unwrap(),
            id.clone(),
            d2_net::Timing::default(),
            rt.handle().clone(),
        )
        .unwrap();
        let mut c = PingClient::new().unwrap();
        let pin = ServerTrust::Pinned(fingerprint(&id.cert));
        let rtt = c.ping(game.local_addr().unwrap(), pin, WAIT).unwrap();
        assert!(rtt < Duration::from_millis(500), "{rtt:?}");
    }

    /// A server offering only the player protocol: no ping.
    fn players_only(id: &Identity, rt: &Runtime) -> quinn::Endpoint {
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(id.key.clone()));
        let mut crypto = rustls::ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![CertificateDer::from(id.cert.clone())], key)
            .unwrap();
        crypto.alpn_protocols = vec![crate::ALPN.to_vec()];
        let quic = QuicServerConfig::try_from(crypto).unwrap();
        let config = quinn::ServerConfig::with_crypto(Arc::new(quic));
        let _guard = rt.enter();
        quinn::Endpoint::server(config, "127.0.0.1:0".parse().unwrap()).unwrap()
    }

    #[test]
    fn a_port_without_pings_and_a_silent_port_fail() {
        let rt = runtime();
        let id = Identity::self_signed().unwrap();
        let other = players_only(&id, &rt);
        let mut c = PingClient::new().unwrap();
        let pin = ServerTrust::Pinned(fingerprint(&id.cert));
        // Another application protocol: the handshake fails.
        assert!(c
            .ping(other.local_addr().unwrap(), pin.clone(), WAIT)
            .is_err());
        // Nobody there.
        let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let t = Instant::now();
        assert!(c
            .ping(
                silent.local_addr().unwrap(),
                pin,
                Duration::from_millis(300)
            )
            .is_err());
        assert!(t.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn the_server_answers_ten_pings_per_address_in_ten_seconds() {
        let rt = runtime();
        let id = Identity::self_signed().unwrap();
        let srv =
            PingServer::bind("127.0.0.1:0".parse().unwrap(), &id, rt.handle().clone()).unwrap();
        let addr = srv.local_addr().unwrap();
        let mut c = PingClient::new().unwrap();
        let pin = ServerTrust::Pinned(fingerprint(&id.cert));
        for i in 0..PING_LIMIT {
            assert!(c.ping(addr, pin.clone(), WAIT).is_ok(), "ping {i}");
        }
        assert!(c.ping(addr, pin, WAIT).is_err());
    }

    #[test]
    fn the_limit_counts_each_address_over_a_sliding_window() {
        let mut l = PingLimit::default();
        let a: IpAddr = "10.0.0.1".parse().unwrap();
        let b: IpAddr = "10.0.0.2".parse().unwrap();
        let t0 = Instant::now();
        for i in 0..PING_LIMIT {
            assert!(l.allow(a, t0 + Duration::from_millis(i as u64)));
        }
        assert!(!l.allow(a, t0 + Duration::from_secs(1)));
        assert!(l.allow(b, t0 + Duration::from_secs(1)));
        // The first one leaves the window.
        assert!(l.allow(a, t0 + PING_WINDOW));
        assert!(!l.allow(a, t0 + PING_WINDOW));
    }
}
