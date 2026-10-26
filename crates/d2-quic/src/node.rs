//! The game-server link's part of the stream layer (multiplayer v2,
//! contracts 11.2): a game server connects out to the main server with
//! ALPN [`NODE_ALPN`], from the same endpoint it serves its players on,
//! and presents its own certificate as a client certificate.
//!
//! - **Both ways pinned.** The game server pins the main server's
//!   certificate ([`ServerTrust::Pinned`]). The main server asks every
//!   connection for a client certificate but doesn't require one: players
//!   have none. TLS checks that the client holds the key of the
//!   certificate it shows; which certificates may link is the main
//!   server's decision, by [`QuicConn::peer_fingerprint`].
//! - **One port.** The main server takes players ([`ALPN`]) and game
//!   servers ([`NODE_ALPN`]) on one UDP port ([`Listen::PlayersAndNodes`]);
//!   [`QuicConn::alpn`] says which a connection is. A game-server
//!   connection without a client certificate is the main server's to
//!   refuse.
//! - **Records on their own streams.** Beside the control stream, each
//!   character record and journal chunk travels on a unidirectional stream
//!   of its own ([`QuicConn::send_uni`], [`QuicConn::accept_uni`]); the
//!   link (`d2-link`) puts its header in front.

use std::net::SocketAddr;
use std::sync::Arc;

use quinn::crypto::rustls::{HandshakeData, QuicClientConfig};
use rustls::client::danger::HandshakeSignatureValid;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};

use d2_net::wire::{ConnectError, Timing};
use d2_net::NetError;

use crate::{
    fingerprint, io_err, provider, start_stream, transport_config, FramedStream, Identity,
    QuicConn, QuicServer, ServerTrust, TrustCheck, ALPN, SERVER_NAME,
};

/// The application protocol of the game-server link.
pub const NODE_ALPN: &[u8] = b"d2rust-node/1";

/// Which connections a [`QuicServer`] takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Listen {
    /// Players only ([`ALPN`]); no client certificates.
    Players,
    /// Players and game servers ([`NODE_ALPN`]) on one port; client
    /// certificates are asked for, not required (the main server).
    PlayersAndNodes,
}

/// Takes any client certificate whose key the client proves it holds, and
/// none: the main server decides by fingerprint.
#[derive(Debug)]
pub(crate) struct AnyClientCert {
    provider: Arc<CryptoProvider>,
}

impl AnyClientCert {
    pub(crate) fn new() -> Self {
        Self {
            provider: provider(),
        }
    }
}

impl ClientCertVerifier for AnyClientCert {
    fn offer_client_auth(&self) -> bool {
        true
    }

    fn client_auth_mandatory(&self) -> bool {
        false
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

impl QuicServer {
    /// Connects out from this endpoint (its UDP socket: the other end sees
    /// the address players reach) to `addr` with ALPN `alpn`, presenting
    /// `identity` as the client certificate and checking the other end
    /// against `trust`; opens the control stream. Waits at most
    /// `timing.timeout`. Call on the runtime the server was bound on.
    pub async fn connect(
        &self,
        addr: SocketAddr,
        trust: ServerTrust,
        alpn: &[u8],
        identity: &Identity,
        timing: Timing,
    ) -> Result<(QuicConn, FramedStream), ConnectError> {
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(identity.key.clone()));
        let mut crypto = rustls::ClientConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|e| io_err(e.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(TrustCheck {
                trust,
                provider: provider(),
            }))
            .with_client_auth_cert(vec![CertificateDer::from(identity.cert.clone())], key)
            .map_err(|e| io_err(format!("client certificate: {e}")))?;
        crypto.alpn_protocols = vec![alpn.to_vec()];
        let quic = QuicClientConfig::try_from(crypto).map_err(|e| io_err(e.to_string()))?;
        let mut config = quinn::ClientConfig::new(Arc::new(quic));
        config.transport_config(transport_config(timing)?);
        let opened = tokio::time::timeout(timing.timeout, async {
            let conn = self
                .endpoint
                .connect_with(config, addr, SERVER_NAME)
                .map_err(|e| io_err(format!("connect: {e}")))?
                .await
                .map_err(|e| io_err(format!("connect: {e}")))?;
            let (send, recv) = conn
                .open_bi()
                .await
                .map_err(|e| io_err(format!("open stream: {e}")))?;
            Ok::<_, ConnectError>((conn, send, recv))
        })
        .await;
        let (conn, send, recv) = match opened {
            Ok(r) => r?,
            Err(_) => return Err(io_err("the other end did not answer in time")),
        };
        let rt = tokio::runtime::Handle::current();
        let control = start_stream(&rt, conn.clone(), send, recv, false);
        Ok((QuicConn { conn, rt }, control))
    }
}

impl QuicConn {
    /// The application protocol the connection agreed on ([`ALPN`] or
    /// [`NODE_ALPN`]).
    pub fn alpn(&self) -> Option<Vec<u8>> {
        self.conn
            .handshake_data()?
            .downcast::<HandshakeData>()
            .ok()?
            .protocol
    }

    /// Whether this is a player's connection ([`ALPN`]).
    pub fn is_player(&self) -> bool {
        self.alpn().as_deref() == Some(ALPN)
    }

    /// The fingerprint of the certificate the other end presented: the
    /// server's, or a client's when it showed one.
    pub fn peer_fingerprint(&self) -> Option<[u8; 32]> {
        let certs = self
            .conn
            .peer_identity()?
            .downcast::<Vec<CertificateDer<'static>>>()
            .ok()?;
        certs.first().map(|c| fingerprint(c.as_ref()))
    }

    /// Waits until the connection is closed, by either end or by a
    /// timeout.
    pub async fn closed(&self) {
        self.conn.closed().await;
    }

    /// Why the connection closed, once it has.
    pub fn close_reason(&self) -> Option<String> {
        self.conn.close_reason().map(|e| e.to_string())
    }

    /// Sends `bytes` on a new unidirectional stream and finishes it.
    pub async fn send_uni(&self, bytes: &[u8]) -> Result<(), NetError> {
        let mut s = self.conn.open_uni().await.map_err(|_| NetError::Closed)?;
        s.write_all(bytes).await.map_err(|_| NetError::Closed)?;
        s.finish().map_err(|_| NetError::Closed)
    }

    /// The next unidirectional stream the other end opens. `None` once the
    /// connection is gone.
    pub async fn accept_uni(&self) -> Option<UniRecv> {
        self.conn.accept_uni().await.ok().map(UniRecv)
    }
}

/// A unidirectional stream the other end opened.
#[derive(Debug)]
pub struct UniRecv(quinn::RecvStream);

impl UniRecv {
    /// Reads the stream to its end: at most `max` bytes, else
    /// [`NetError::Malformed`] (and the stream is stopped).
    pub async fn read_all(mut self, max: usize) -> Result<Vec<u8>, NetError> {
        match self.0.read_to_end(max).await {
            Ok(b) => Ok(b),
            Err(quinn::ReadToEndError::TooLong) => {
                let _ = self.0.stop(1u32.into());
                Err(NetError::Malformed)
            }
            Err(_) => Err(NetError::Closed),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
    }

    fn timing() -> Timing {
        Timing {
            keepalive: Duration::from_millis(200),
            timeout: Duration::from_secs(5),
        }
    }

    fn bind(rt: &tokio::runtime::Runtime, id: Identity, listen: Listen) -> QuicServer {
        QuicServer::bind_with(
            "127.0.0.1:0".parse().unwrap(),
            id,
            timing(),
            rt.handle().clone(),
            listen,
        )
        .unwrap()
    }

    /// A game server's endpoint links to the main server's player port:
    /// each end sees the other's certificate and the node ALPN, and the
    /// main server sees the endpoint's own address.
    #[test]
    fn a_node_links_out_from_its_player_endpoint_with_its_certificate() {
        let rt = rt();
        let (realm_id, node_id) = (
            Identity::self_signed().unwrap(),
            Identity::self_signed().unwrap(),
        );
        let realm_fp = fingerprint(&realm_id.cert);
        let node_fp = fingerprint(&node_id.cert);
        let realm = bind(&rt, realm_id, Listen::PlayersAndNodes);
        let node = bind(&rt, node_id.clone(), Listen::Players);
        let (node_addr, realm_addr) = (node.local_addr().unwrap(), realm.local_addr().unwrap());
        rt.block_on(async {
            let (out, mut control) = node
                .connect(
                    realm_addr,
                    ServerTrust::Pinned(realm_fp),
                    NODE_ALPN,
                    &node_id,
                    timing(),
                )
                .await
                .unwrap();
            control.send(b"hello".to_vec()).unwrap();
            let (conn, mut stream) = realm.accept().await.unwrap();
            assert_eq!(stream.recv_async().await.unwrap(), b"hello");
            assert_eq!(conn.alpn().as_deref(), Some(NODE_ALPN));
            assert!(!conn.is_player());
            assert_eq!(conn.peer_fingerprint(), Some(node_fp));
            assert_eq!(
                conn.peer().port(),
                node_addr.port(),
                "the player endpoint's socket"
            );
            assert_eq!(out.peer_fingerprint(), Some(realm_fp));
            // Unidirectional streams both ways, with a size limit.
            out.send_uni(b"record").await.unwrap();
            let got = conn.accept_uni().await.unwrap();
            assert_eq!(got.read_all(64).await.unwrap(), b"record");
            conn.send_uni(&[7; 100]).await.unwrap();
            let got = out.accept_uni().await.unwrap();
            assert_eq!(got.read_all(99).await, Err(NetError::Malformed));
        });
    }

    /// A player (no certificate, [`ALPN`]) still connects to a main server
    /// that asks for client certificates; it shows none.
    #[test]
    fn a_player_connects_without_a_certificate() {
        let rt = rt();
        let realm = bind(
            &rt,
            Identity::self_signed().unwrap(),
            Listen::PlayersAndNodes,
        );
        let addr = realm.local_addr().unwrap();
        let (_client, mut c) =
            crate::QuicClient::connect(addr, ServerTrust::Any, timing()).unwrap();
        c.send(b"hi".to_vec()).unwrap();
        rt.block_on(async {
            let (conn, mut s) = realm.accept().await.unwrap();
            assert_eq!(s.recv_async().await.unwrap(), b"hi");
            assert!(conn.is_player());
            assert_eq!(conn.peer_fingerprint(), None);
        });
    }

    /// A players-only server doesn't speak the node ALPN, and a node pins
    /// the main server: another certificate fails the handshake.
    #[test]
    fn the_node_alpn_and_the_pin_are_enforced() {
        let rt = rt();
        let node_id = Identity::self_signed().unwrap();
        let players_only = bind(&rt, Identity::self_signed().unwrap(), Listen::Players);
        let realm = bind(
            &rt,
            Identity::self_signed().unwrap(),
            Listen::PlayersAndNodes,
        );
        let node = bind(&rt, node_id.clone(), Listen::Players);
        rt.block_on(async {
            let a = players_only.local_addr().unwrap();
            assert!(node
                .connect(a, ServerTrust::Any, NODE_ALPN, &node_id, timing())
                .await
                .is_err());
            let r = realm.local_addr().unwrap();
            assert!(node
                .connect(
                    r,
                    ServerTrust::Pinned([7; 32]),
                    NODE_ALPN,
                    &node_id,
                    timing()
                )
                .await
                .is_err());
        });
    }
}
