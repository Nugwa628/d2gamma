//! The outside check run with real QUIC, on `d2-quic`'s pings.
//!
//! A ping is a QUIC connection on ALPN [`d2_quic::PING_ALPN`] (contracts
//! 11.4): the handshake completes and both ends close at once. The main
//! server pings a game server's candidate addresses from fresh endpoints
//! ([`d2_quic::reach::probe`], pinned to the game server's certificate)
//! and runs the observer endpoint, a second ping-only QUIC endpoint that
//! reports where each ping came from ([`d2_quic::PingServer`]). A game
//! server answers pings on its own endpoint (every `QuicServer` does) and
//! pings the observer endpoint from it (`QuicServer::ping`). Nothing here
//! is plain UDP.
//!
//! [`drive_check`] runs an [`OutsideCheck`] over those.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::check::{CheckAction, OutsideCheck};
use crate::ReachResult;

/// How often [`drive_check`] looks at its inputs.
const TICK: Duration = Duration::from_millis(20);

/// Runs an outside check with real QUIC: each attempt is a
/// [`d2_quic::reach::probe`] of the game server (certificate `pin`), and
/// `observed` brings the addresses the observer endpoint
/// ([`d2_quic::PingServer`]) saw this game server's pings come from. Call
/// on a tokio runtime.
pub async fn drive_check(
    mut check: OutsideCheck,
    pin: [u8; 32],
    observed: &mut mpsc::UnboundedReceiver<SocketAddr>,
) -> ReachResult {
    let start = Instant::now();
    let (done_tx, mut done_rx) = mpsc::unbounded_channel();
    loop {
        let now = start.elapsed().as_millis() as u64;
        while let Ok(from) = observed.try_recv() {
            check.on_observed(from);
        }
        while let Ok((to, ok)) = done_rx.try_recv() {
            check.on_attempt(to, ok);
        }
        for action in check.poll(now) {
            match action {
                CheckAction::Connect { to, timeout_ms } => {
                    let tx = done_tx.clone();
                    tokio::spawn(async move {
                        let timeout = Duration::from_millis(timeout_ms);
                        let ok = d2_quic::reach::probe(to, pin, timeout).await;
                        let _ = tx.send((to, ok));
                    });
                }
                CheckAction::Done(r) => return r,
            }
        }
        tokio::time::sleep(TICK).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::punch::{Punch, PunchJob, PunchStep};
    use crate::socket::{endpoint, SharedSocket};
    use crate::{wire, NatType, ReachInfo, ReachMethod};
    use d2_net::wire::Timing;
    use d2_quic::{fingerprint, Identity, PingServer, QuicServer, SERVER_NAME};

    const WAIT: Duration = Duration::from_secs(5);

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
    }

    fn loopback() -> SocketAddr {
        "127.0.0.1:0".parse().unwrap()
    }

    /// The host's game endpoint is a `d2-quic` server on a shared socket:
    /// punches leave from that very socket, are taken off the joiner's
    /// socket before its QUIC sees them, and the joiner's QUIC connection
    /// then completes on that path.
    #[test]
    fn punch_and_quic_share_one_socket() {
        let rt = rt();
        let host_id = Identity::self_signed().unwrap();
        let host_pin = fingerprint(&host_id.cert);
        let (host_sock, punches, _host_taps) = {
            let _g = rt.enter();
            SharedSocket::bind(loopback()).unwrap()
        };
        let host_addr = punches.local_addr().unwrap();
        let host =
            QuicServer::bind_socket(host_sock, host_id, Timing::default(), rt.handle().clone())
                .unwrap();
        assert_eq!(host.local_addr().unwrap(), host_addr);

        rt.block_on(async {
            let (joiner_sock, joiner_sender, joiner_taps) = SharedSocket::bind(loopback()).unwrap();
            let joiner_addr = joiner_sender.local_addr().unwrap();
            let joiner = endpoint(joiner_sock, None).unwrap();

            let punch = Punch {
                game: 9,
                peer_addr: joiner_addr,
                at_ms: 0,
                tries: 3,
            };
            let mut job = PunchJob::start(punch, 0);
            for t in [0, 200, 400] {
                for step in job.poll(t) {
                    if let PunchStep::Send { to, bytes } = step {
                        punches.send(to, &bytes).unwrap();
                    }
                }
            }
            // The joiner's endpoint keeps running: connect while the
            // punches sit in its socket's tap.
            let config = d2_quic::reach::ping_client_config(host_pin).unwrap();
            let conn = joiner.connect_with(config, host_addr, SERVER_NAME).unwrap();
            let conn = tokio::time::timeout(WAIT, conn).await.unwrap();
            assert!(conn.is_ok(), "QUIC after the punches: {conn:?}");
            assert!(job.connected(joiner_addr).is_some_and(|d| d.connected));

            let mut got = 0;
            while let Ok((from, bytes)) = joiner_taps.try_recv() {
                assert_eq!(from, host_addr, "punches leave from the host's QUIC socket");
                assert_eq!(wire::read_punch(&bytes), Some(9));
                got += 1;
            }
            assert_eq!(got, 3);
        });
    }

    /// The outside check with real QUIC on loopback: the probes connect,
    /// the host's ping to the observer endpoint arrives from its game
    /// socket, so the port is open; a wrong certificate or a closed port
    /// is not reachable.
    #[test]
    fn outside_check_over_quic() {
        let rt = rt();
        let realm = Identity::self_signed().unwrap();
        let (observer, mut observed) =
            PingServer::bind(loopback(), &realm, rt.handle().clone()).unwrap();
        let observer_addr = observer.local_addr().unwrap();

        let node_id = Identity::self_signed().unwrap();
        let pin = fingerprint(&node_id.cert);
        let node =
            QuicServer::bind(loopback(), node_id, Timing::default(), rt.handle().clone()).unwrap();
        let node_addr = node.local_addr().unwrap();

        rt.block_on(async {
            node.ping(observer_addr, fingerprint(&realm.cert), WAIT)
                .await
                .unwrap();
            let info = ReachInfo {
                method: ReachMethod::None,
                mapped_addr: None,
            };
            let r = drive_check(OutsideCheck::new(node_addr, info, 0), pin, &mut observed).await;
            assert!(r.reachable);
            assert_eq!(r.checked_addr, node_addr);
            assert_eq!(r.nat, NatType::Open);

            let mut none = mpsc::unbounded_channel().1;
            let wrong =
                drive_check(OutsideCheck::new(node_addr, info, 0), [7; 32], &mut none).await;
            assert!(
                !wrong.reachable,
                "another certificate is not this game server"
            );

            let closed = std::net::UdpSocket::bind(loopback()).unwrap();
            let closed_addr = closed.local_addr().unwrap();
            drop(closed);
            let started = Instant::now();
            let r = drive_check(OutsideCheck::new(closed_addr, info, 0), pin, &mut none).await;
            assert!(!r.reachable);
            assert_eq!(r.nat, NatType::Unknown);
            assert!(started.elapsed() < WAIT);
        });
    }
}
