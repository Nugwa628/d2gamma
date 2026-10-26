//! The link's control stream as two typed halves. [`start`] hands the
//! stream to a task on the current runtime; [`LinkTx`] queues messages
//! (it never blocks) and [`LinkRx`] reads the other end's, decoded. A
//! message that doesn't decode ends the link ([`NetError::Malformed`]).
//! Dropping every [`LinkTx`] (or [`LinkTx::close`]) says goodbye on the
//! stream.

use std::marker::PhantomData;

use d2_net::msg::node::{NodeToRealm, RealmToNode};
use d2_net::NetError;
use d2_quic::FramedStream;
use tokio::sync::mpsc;

use crate::{NodeLink, RealmLink};

/// A link message: a body on the control stream.
pub trait Message: Sized + Send + 'static {
    fn body(&self) -> Vec<u8>;
    fn parse(body: &[u8]) -> Result<Self, NetError>;
}

impl Message for NodeToRealm {
    fn body(&self) -> Vec<u8> {
        NodeToRealm::body(self)
    }

    fn parse(body: &[u8]) -> Result<Self, NetError> {
        NodeToRealm::parse(body)
    }
}

impl Message for RealmToNode {
    fn body(&self) -> Vec<u8> {
        RealmToNode::body(self)
    }

    fn parse(body: &[u8]) -> Result<Self, NetError> {
        RealmToNode::parse(body)
    }
}

enum Cmd {
    Body(Vec<u8>),
    Close,
}

/// The sending half: messages of type `M`. Cloning shares it.
pub struct LinkTx<M> {
    tx: mpsc::UnboundedSender<Cmd>,
    _m: PhantomData<fn(M)>,
}

impl<M> Clone for LinkTx<M> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            _m: PhantomData,
        }
    }
}

impl<M> std::fmt::Debug for LinkTx<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkTx")
            .field("closed", &self.tx.is_closed())
            .finish()
    }
}

impl<M: Message> LinkTx<M> {
    /// Queues `m`; [`NetError::Closed`] once the link is over.
    pub fn send_msg(&self, m: &M) -> Result<(), NetError> {
        self.tx
            .send(Cmd::Body(m.body()))
            .map_err(|_| NetError::Closed)
    }

    /// Says goodbye after what is queued.
    pub fn close(&self) {
        let _ = self.tx.send(Cmd::Close);
    }
}

impl NodeLink for LinkTx<RealmToNode> {
    fn send(&self, m: RealmToNode) -> Result<(), NetError> {
        self.send_msg(&m)
    }
}

impl RealmLink for LinkTx<NodeToRealm> {
    fn send(&self, m: NodeToRealm) -> Result<(), NetError> {
        self.send_msg(&m)
    }
}

/// The receiving half: the other end's messages of type `M`.
#[derive(Debug)]
pub struct LinkRx<M> {
    rx: mpsc::UnboundedReceiver<Result<M, NetError>>,
    ended: Option<NetError>,
}

impl<M> LinkRx<M> {
    /// The next message. Once the link is over, its error, every time:
    /// [`NetError::Closed`] (goodbye or gone), [`NetError::TimedOut`] or
    /// [`NetError::Malformed`].
    pub async fn recv(&mut self) -> Result<M, NetError> {
        if let Some(e) = &self.ended {
            return Err(e.clone());
        }
        let got = self.rx.recv().await.unwrap_or(Err(NetError::Closed));
        if let Err(e) = &got {
            self.ended = Some(e.clone());
        }
        got
    }
}

/// Splits `control` into its halves: `Out` goes to the other end, `In`
/// comes from it. Call on the runtime the stream's connection lives on.
pub fn start<Out: Message, In: Message>(control: FramedStream) -> (LinkTx<Out>, LinkRx<In>) {
    let (out_tx, out_rx) = mpsc::unbounded_channel();
    let (in_tx, in_rx) = mpsc::unbounded_channel();
    tokio::spawn(pump::<In>(control, out_rx, in_tx));
    (
        LinkTx {
            tx: out_tx,
            _m: PhantomData,
        },
        LinkRx {
            rx: in_rx,
            ended: None,
        },
    )
}

async fn pump<In: Message>(
    mut control: FramedStream,
    mut out: mpsc::UnboundedReceiver<Cmd>,
    to: mpsc::UnboundedSender<Result<In, NetError>>,
) {
    loop {
        tokio::select! {
            got = control.recv_async() => {
                let m = got.and_then(|b| In::parse(&b));
                let over = m.is_err();
                if to.send(m).is_err() || over {
                    break;
                }
            }
            cmd = out.recv() => match cmd {
                Some(Cmd::Body(b)) => {
                    if let Err(e) = control.send(b) {
                        let _ = to.send(Err(e));
                        break;
                    }
                }
                Some(Cmd::Close) | None => break,
            },
        }
    }
    control.close();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use d2_net::msg::node::Load;
    use d2_net::wire::Timing;
    use d2_quic::{fingerprint, Identity, Listen, QuicServer, ServerTrust, NODE_ALPN};

    use super::*;
    use crate::streams::{self, What};

    fn timing() -> Timing {
        Timing {
            keepalive: Duration::from_millis(200),
            timeout: Duration::from_secs(5),
        }
    }

    /// A game server and a main server over a real loopback link: typed
    /// messages both ways, a record on its own stream, and the goodbye.
    #[test]
    fn both_ends_talk_over_a_loopback_link() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let realm_id = Identity::self_signed().unwrap();
        let node_id = Identity::self_signed().unwrap();
        let pin = fingerprint(&realm_id.cert);
        let any = "127.0.0.1:0".parse().unwrap();
        let realm = QuicServer::bind_with(
            any,
            realm_id,
            timing(),
            rt.handle().clone(),
            Listen::PlayersAndNodes,
        )
        .unwrap();
        let node = QuicServer::bind_with(
            any,
            node_id.clone(),
            timing(),
            rt.handle().clone(),
            Listen::Players,
        )
        .unwrap();
        let addr = realm.local_addr().unwrap();
        rt.block_on(async {
            let (out_conn, out) = node
                .connect(
                    addr,
                    ServerTrust::Pinned(pin),
                    NODE_ALPN,
                    &node_id,
                    timing(),
                )
                .await
                .unwrap();
            let (to_realm, mut from_realm) = start::<NodeToRealm, RealmToNode>(out);
            let load = NodeToRealm::Load(Load {
                games: 1,
                players: 2,
                cpu_permille: 3,
                tick_overruns: 4,
                draining: false,
            });
            RealmLink::send(&to_realm, load.clone()).unwrap();
            let (in_conn, control) = realm.accept().await.unwrap();
            let (to_node, mut from_node) = start::<RealmToNode, NodeToRealm>(control);
            assert_eq!(from_node.recv().await.unwrap(), load);
            NodeLink::send(&to_node, RealmToNode::Drain { on: true }).unwrap();
            assert_eq!(
                from_realm.recv().await.unwrap(),
                RealmToNode::Drain { on: true }
            );
            streams::send(&out_conn, What::Record { lease: 5 }, 1, b"rec")
                .await
                .unwrap();
            let got = streams::recv(in_conn.accept_uni().await.unwrap())
                .await
                .unwrap();
            assert_eq!(got.what, What::Record { lease: 5 });
            assert_eq!(got.bytes, b"rec");
            // Goodbye: the other end reads Closed, and keeps reading it.
            to_realm.close();
            assert_eq!(from_node.recv().await, Err(NetError::Closed));
            assert_eq!(from_node.recv().await, Err(NetError::Closed));
        });
    }
}
