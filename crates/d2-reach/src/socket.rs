//! One UDP socket for a QUIC endpoint and its punches.
//!
//! Hole punching only works when the punch leaves from the socket the QUIC
//! endpoint uses: the opening it makes in the NAT belongs to that socket's
//! public address, and the joiner's QUIC connection must arrive there.
//! [`SharedSocket`] wraps the socket quinn polls (a [`quinn::AsyncUdpSocket`])
//! and adds two things:
//!
//! - [`PunchSender`] sends a datagram through the same socket, beside the
//!   endpoint's own packets;
//! - punch datagrams that arrive ([`wire::is_reach`]) are taken off before
//!   quinn sees them and handed to [`SharedSocket::bind`]'s receiver.
//!
//! A game server serves on it with `d2_quic::QuicServer::bind_socket`; a
//! plain quinn endpoint over it is [`endpoint`]. A punch is shorter than
//! any QUIC packet can be, so the two never mix up ([`crate::wire`]).

use std::fmt;
use std::io::{self, IoSliceMut};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::mpsc;
use std::sync::Arc;
use std::task::{Context, Poll};

use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, EndpointConfig, Runtime, TokioRuntime, UdpPoller};

use crate::wire;

/// Reach datagrams kept for their reader at most; later ones are dropped
/// until it catches up.
pub const TAP_QUEUE: usize = 256;

/// A reach datagram that arrived: where from, and its bytes.
pub type Tapped = (SocketAddr, Vec<u8>);

/// The socket a QUIC endpoint and its punches share.
pub struct SharedSocket {
    inner: Arc<dyn AsyncUdpSocket>,
    tap: mpsc::SyncSender<Tapped>,
}

impl fmt::Debug for SharedSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedSocket")
            .field("addr", &self.inner.local_addr().ok())
            .finish()
    }
}

/// Sends datagrams through a [`SharedSocket`], from the endpoint's own
/// address.
#[derive(Clone, Debug)]
pub struct PunchSender {
    socket: Arc<SharedSocket>,
}

impl SharedSocket {
    /// Binds `addr` (port 0 picks a free one). Must run inside a tokio
    /// runtime: the socket is registered with it. Returns the socket, its
    /// sender and the receiver of the reach datagrams that arrive on it.
    pub fn bind(addr: SocketAddr) -> io::Result<(Arc<Self>, PunchSender, mpsc::Receiver<Tapped>)> {
        Self::wrap(std::net::UdpSocket::bind(addr)?)
    }

    /// As [`SharedSocket::bind`], for a socket already bound.
    pub fn wrap(
        socket: std::net::UdpSocket,
    ) -> io::Result<(Arc<Self>, PunchSender, mpsc::Receiver<Tapped>)> {
        socket.set_nonblocking(true)?;
        let inner = TokioRuntime.wrap_udp_socket(socket)?;
        let (tap, taps) = mpsc::sync_channel(TAP_QUEUE);
        let shared = Arc::new(Self { inner, tap });
        let sender = PunchSender {
            socket: shared.clone(),
        };
        Ok((shared, sender, taps))
    }
}

impl PunchSender {
    /// Sends `bytes` to `to` from the shared socket. Best effort, like any
    /// datagram: a full send buffer returns `WouldBlock` and the caller's
    /// next try goes out later.
    pub fn send(&self, to: SocketAddr, bytes: &[u8]) -> io::Result<()> {
        self.socket.inner.try_send(&Transmit {
            destination: to,
            ecn: None,
            contents: bytes,
            segment_size: None,
            src_ip: None,
        })
    }

    /// The shared socket's address.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.socket.inner.local_addr()
    }
}

impl AsyncUdpSocket for SharedSocket {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        self.inner.clone().create_io_poller()
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        self.inner.try_send(transmit)
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        loop {
            let n = match self.inner.poll_recv(cx, bufs, meta) {
                Poll::Ready(Ok(n)) => n,
                other => return other,
            };
            // Keep QUIC's datagrams, moved to the front; take the reach
            // datagrams out. A coalesced buffer (several datagrams, GRO) is
            // left to quinn, which drops what isn't QUIC.
            let mut kept = 0;
            for i in 0..n {
                let m = meta[i];
                if m.stride == m.len && wire::is_reach(&bufs[i][..m.len]) {
                    let _ = self.tap.try_send((m.addr, bufs[i][..m.len].to_vec()));
                    continue;
                }
                if kept != i {
                    let (front, back) = bufs.split_at_mut(i);
                    front[kept][..m.len].copy_from_slice(&back[0][..m.len]);
                    meta[kept] = m;
                }
                kept += 1;
            }
            if kept > 0 {
                return Poll::Ready(Ok(kept));
            }
            // Only reach datagrams: poll again, which registers the waker
            // when nothing more is there.
        }
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn max_transmit_segments(&self) -> usize {
        self.inner.max_transmit_segments()
    }

    fn max_receive_segments(&self) -> usize {
        self.inner.max_receive_segments()
    }

    fn may_fragment(&self) -> bool {
        self.inner.may_fragment()
    }
}

/// A QUIC endpoint over `socket`, accepting connections with `server` if
/// given. Must run inside a tokio runtime.
pub fn endpoint(
    socket: Arc<SharedSocket>,
    server: Option<quinn::ServerConfig>,
) -> io::Result<quinn::Endpoint> {
    quinn::Endpoint::new_with_abstract_socket(
        EndpointConfig::default(),
        server,
        socket,
        Arc::new(TokioRuntime),
    )
}
