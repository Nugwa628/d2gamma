//! NAT modes: a small simulated internet between the programs of a v2
//! setup (plan N1), so a game server can sit behind a home router in a
//! test.
//!
//! Every program of the setup that isn't the main server is a **party**
//! of the [`Fabric`]. A party talks to the main server at its own
//! **ingress** address (the fabric's socket that stands for the main
//! server's public address, for that party only), and the fabric
//! forwards from a **mapping** socket: the party's outside address, the
//! one the main server sees and tells others. Whoever sends to a mapping
//! reaches the party behind it, if its NAT lets them in. Replies come
//! from the address the party sent to, as through a real router.
//!
//! The modes (`nat NAME=MODE` in a scenario):
//!
//! - `open`: no NAT (a public address, or a forwarded port): one outside
//!   address, anyone may send to it.
//! - `cone`: endpoint-independent mapping (one outside address for every
//!   destination) with address-and-port-dependent filtering (only those
//!   the party has sent to may send in). Hole punching works: after the
//!   party sends to a peer, that peer gets through.
//! - `symmetric`: a new outside address per destination, with the same
//!   filtering. The address the main server saw is useless to a peer, so
//!   punching fails.
//! - `blocked`: only the main-server link: nothing in or out to anyone
//!   else.
//!
//! Each party's traffic also takes its [`Conditions`] (latency, jitter,
//! loss, cut), and one party's link to the main server can be cut on its
//! own (`cut-link`). A sender the fabric doesn't know (a plain socket of
//! a test) is an `open` party of its own. Everything stays on
//! 127.0.0.0/8.

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::{Conditions, Rng, POLL};

/// How a party's router treats its traffic.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NatMode {
    /// No NAT: a public address.
    #[default]
    Open,
    /// Endpoint-independent mapping, address-and-port-dependent filtering.
    Cone,
    /// A mapping per destination, address-and-port-dependent filtering.
    Symmetric,
    /// Only the main-server link gets through.
    Blocked,
}

impl NatMode {
    /// Every mode, by its word in a scenario.
    pub const ALL: [NatMode; 4] = [
        NatMode::Open,
        NatMode::Cone,
        NatMode::Symmetric,
        NatMode::Blocked,
    ];

    /// Its word in a scenario.
    pub fn name(self) -> &'static str {
        match self {
            NatMode::Open => "open",
            NatMode::Cone => "cone",
            NatMode::Symmetric => "symmetric",
            NatMode::Blocked => "blocked",
        }
    }
}

impl std::str::FromStr for NatMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        NatMode::ALL
            .into_iter()
            .find(|m| m.name() == s)
            .ok_or_else(|| format!("nat mode {s}: unknown (open, cone, symmetric, blocked)"))
    }
}

/// One end of a datagram's way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum End {
    /// The main server.
    Realm,
    /// A party, by index.
    Party(usize),
}

/// What a fabric socket stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// The main server's address for this party.
    Ingress(usize),
    /// An outside address of this party: toward one destination
    /// (symmetric), or toward all (`None`).
    Mapping { party: usize, toward: Option<End> },
}

#[derive(Debug)]
struct Sock {
    sock: Arc<UdpSocket>,
    addr: SocketAddr,
    role: Role,
    /// The fabric sockets this mapping has sent to: who may answer
    /// through it.
    permits: BTreeSet<usize>,
}

#[derive(Debug)]
struct Party {
    name: String,
    mode: NatMode,
    /// The loopback address its outside sockets use.
    ip: IpAddr,
    /// Where the party's own socket is, once it has sent.
    real: Option<SocketAddr>,
    cond: Conditions,
    ingress: Option<usize>,
}

#[derive(Debug)]
struct State {
    realm: SocketAddr,
    parties: Vec<Party>,
    socks: Vec<Sock>,
    /// Party links to the main server that are cut.
    cut: BTreeSet<usize>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Due {
    at: Instant,
    seq: u64,
    from: usize,
    to: SocketAddr,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    rng: Mutex<Rng>,
    queue: Mutex<BinaryHeap<Reverse<Due>>>,
    wake: Condvar,
    stop: AtomicBool,
    seq: AtomicU64,
    relayed: AtomicU64,
    dropped: AtomicU64,
    readers: Mutex<Vec<JoinHandle<()>>>,
}

/// The simulated internet of one setup. Dropping it stops its threads.
#[derive(Debug)]
pub struct Fabric {
    shared: Arc<Shared>,
    sender: Option<JoinHandle<()>>,
}

impl Fabric {
    /// A fabric in front of the main server at `realm`, with loss and
    /// jitter from `seed`.
    pub fn start(realm: SocketAddr, seed: u64) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                realm: loopback(realm),
                parties: Vec::new(),
                socks: Vec::new(),
                cut: BTreeSet::new(),
            }),
            rng: Mutex::new(Rng(seed)),
            queue: Mutex::new(BinaryHeap::new()),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
            seq: AtomicU64::new(0),
            relayed: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            readers: Mutex::new(Vec::new()),
        });
        let s = shared.clone();
        let sender = std::thread::spawn(move || send_loop(&s));
        Ok(Self {
            shared,
            sender: Some(sender),
        })
    }

    /// Adds a party behind a router of `mode`, its outside addresses on
    /// `ip`; returns its index.
    pub fn add_party(&self, name: &str, mode: NatMode, ip: IpAddr) -> std::io::Result<usize> {
        let mut st = self.shared.state.lock().expect("fabric");
        let p = st.parties.len();
        st.parties.push(Party {
            name: name.to_string(),
            mode,
            ip,
            real: None,
            cond: Conditions::default(),
            ingress: None,
        });
        let i = open_sock(
            &self.shared,
            &mut st,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            Role::Ingress(p),
        )?;
        st.parties[p].ingress = Some(i);
        Ok(p)
    }

    /// The party with this name.
    pub fn party(&self, name: &str) -> Option<usize> {
        let st = self.shared.state.lock().expect("fabric");
        st.parties.iter().position(|p| p.name == name)
    }

    /// The main server's address for `party`: what it is configured to
    /// link to.
    pub fn ingress(&self, party: usize) -> Option<SocketAddr> {
        let st = self.shared.state.lock().expect("fabric");
        let i = st.parties.get(party)?.ingress?;
        Some(st.socks[i].addr)
    }

    /// The party's outside address as the main server sees it, once it
    /// has sent there.
    pub fn public(&self, party: usize) -> Option<SocketAddr> {
        let st = self.shared.state.lock().expect("fabric");
        let mode = st.parties.get(party)?.mode;
        let toward = (mode == NatMode::Symmetric).then_some(End::Realm);
        st.socks
            .iter()
            .find(|s| s.role == Role::Mapping { party, toward })
            .map(|s| s.addr)
    }

    /// How many outside addresses the party's router has opened.
    pub fn mappings(&self, party: usize) -> usize {
        let st = self.shared.state.lock().expect("fabric");
        st.socks
            .iter()
            .filter(|s| matches!(s.role, Role::Mapping { party: p, .. } if p == party))
            .count()
    }

    /// The party's NAT mode.
    pub fn mode(&self, party: usize) -> Option<NatMode> {
        let st = self.shared.state.lock().expect("fabric");
        st.parties.get(party).map(|p| p.mode)
    }

    /// The party's network conditions.
    pub fn conditions(&self, party: usize) -> Conditions {
        let st = self.shared.state.lock().expect("fabric");
        st.parties.get(party).map(|p| p.cond).unwrap_or_default()
    }

    /// Changes the party's network conditions (both directions, every
    /// destination).
    pub fn set_conditions(&self, party: usize, c: Conditions) {
        let mut st = self.shared.state.lock().expect("fabric");
        if let Some(p) = st.parties.get_mut(party) {
            p.cond = c;
        }
    }

    /// Cuts (or restores) only the party's link to the main server.
    pub fn cut_link(&self, party: usize, cut: bool) {
        let mut st = self.shared.state.lock().expect("fabric");
        if cut {
            st.cut.insert(party);
        } else {
            st.cut.remove(&party);
        }
    }

    /// Whether the party's main-server link is cut.
    pub fn link_cut(&self, party: usize) -> bool {
        self.shared
            .state
            .lock()
            .expect("fabric")
            .cut
            .contains(&party)
    }

    /// Points the fabric at a main server that moved (a restart on a new
    /// port).
    pub fn retarget_realm(&self, realm: SocketAddr) {
        self.shared.state.lock().expect("fabric").realm = loopback(realm);
    }

    /// Every party's name and mode, in order.
    pub fn parties(&self) -> Vec<(String, NatMode)> {
        let st = self.shared.state.lock().expect("fabric");
        st.parties
            .iter()
            .map(|p| (p.name.clone(), p.mode))
            .collect()
    }

    /// Datagrams relayed and dropped so far.
    pub fn counts(&self) -> (u64, u64) {
        (
            self.shared.relayed.load(Ordering::Relaxed),
            self.shared.dropped.load(Ordering::Relaxed),
        )
    }
}

impl Drop for Fabric {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.wake.notify_all();
        if let Some(t) = self.sender.take() {
            let _ = t.join();
        }
        let readers = std::mem::take(&mut *self.shared.readers.lock().expect("readers"));
        for t in readers {
            let _ = t.join();
        }
    }
}

/// A socket bound to `0.0.0.0` answers from `127.0.0.1`.
fn loopback(a: SocketAddr) -> SocketAddr {
    if a.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), a.port())
    } else {
        a
    }
}

/// Opens a fabric socket and its reader thread.
fn open_sock(
    shared: &Arc<Shared>,
    st: &mut State,
    ip: IpAddr,
    role: Role,
) -> std::io::Result<usize> {
    let sock = UdpSocket::bind(SocketAddr::new(ip, 0))?;
    sock.set_read_timeout(Some(POLL))?;
    let addr = sock.local_addr()?;
    let sock = Arc::new(sock);
    let index = st.socks.len();
    st.socks.push(Sock {
        sock: sock.clone(),
        addr,
        role,
        permits: BTreeSet::new(),
    });
    let s = shared.clone();
    let t = std::thread::spawn(move || read_loop(&s, &sock, index));
    shared.readers.lock().expect("readers").push(t);
    Ok(index)
}

/// Reads one fabric socket and routes what arrives.
fn read_loop(s: &Arc<Shared>, sock: &UdpSocket, index: usize) {
    let mut buf = vec![0u8; 65536];
    while !s.stop.load(Ordering::Relaxed) {
        let (n, from) = match sock.recv_from(&mut buf) {
            Ok(x) => x,
            // Timeouts, and on Windows the "port unreachable" a closed
            // peer leaves behind.
            Err(_) => continue,
        };
        route(s, index, from, &buf[..n]);
    }
}

/// Where a datagram that arrived on fabric socket `at` from `from` goes,
/// through which socket, and when; or why it is dropped.
fn route(s: &Arc<Shared>, at: usize, from: SocketAddr, bytes: &[u8]) {
    let mut st = s.state.lock().expect("fabric");
    let sender = if from == st.realm {
        End::Realm
    } else if let Some(p) = st.parties.iter().position(|p| p.real == Some(from)) {
        End::Party(p)
    } else if let Role::Ingress(p) = st.socks[at].role {
        // A party's first datagram (or one from a new local socket):
        // whoever sends to a party's ingress is that party.
        st.parties[p].real = Some(from);
        End::Party(p)
    } else {
        // A plain socket the fabric doesn't know: a public host.
        let p = st.parties.len();
        st.parties.push(Party {
            name: format!("outside-{from}"),
            mode: NatMode::Open,
            ip: from.ip(),
            real: Some(from),
            cond: Conditions::default(),
            ingress: None,
        });
        End::Party(p)
    };
    let dest = match (st.socks[at].role, sender) {
        (Role::Ingress(p), End::Party(q)) if p == q => End::Realm,
        (Role::Mapping { party, .. }, x) if x != End::Party(party) => End::Party(party),
        _ => return drop_one(s),
    };
    // The sender's outside socket toward `dest`, and the permit it opens.
    let source = match sender {
        End::Realm => {
            let End::Party(d) = dest else {
                return drop_one(s);
            };
            match st.parties[d].ingress {
                Some(i) => i,
                None => return drop_one(s),
            }
        }
        End::Party(x) => {
            let mode = st.parties[x].mode;
            if mode == NatMode::Blocked && dest != End::Realm {
                return drop_one(s);
            }
            let toward = (mode == NatMode::Symmetric).then_some(dest);
            let m = match st
                .socks
                .iter()
                .position(|k| k.role == Role::Mapping { party: x, toward })
            {
                Some(m) => m,
                None => {
                    let ip = st.parties[x].ip;
                    match open_sock(s, &mut st, ip, Role::Mapping { party: x, toward }) {
                        Ok(m) => m,
                        Err(_) => return drop_one(s),
                    }
                }
            };
            st.socks[m].permits.insert(at);
            m
        }
    };
    // The receiver's router lets it in?
    let to = match dest {
        End::Realm => st.realm,
        End::Party(d) => {
            let p = &st.parties[d];
            let allowed = match p.mode {
                NatMode::Open => true,
                NatMode::Blocked => sender == End::Realm,
                NatMode::Cone | NatMode::Symmetric => st.socks[at].permits.contains(&source),
            };
            match (allowed, p.real) {
                (true, Some(real)) => real,
                _ => return drop_one(s),
            }
        }
    };
    let party_of = |e: End| match e {
        End::Party(p) => Some(p),
        End::Realm => None,
    };
    let conds: Vec<Conditions> = [party_of(sender), party_of(dest)]
        .into_iter()
        .flatten()
        .map(|p| st.parties[p].cond)
        .collect();
    let link_cut = [party_of(sender), party_of(dest)]
        .into_iter()
        .flatten()
        .any(|p| st.cut.contains(&p))
        && (sender == End::Realm || dest == End::Realm);
    drop(st);
    if link_cut {
        return drop_one(s);
    }
    let mut delay = 0u64;
    {
        let mut rng = s.rng.lock().expect("rng");
        for c in &conds {
            if c.cut || (c.loss > 0.0 && rng.unit() < c.loss) {
                return drop_one(s);
            }
            delay += u64::from(c.latency_ms);
            if c.jitter_ms > 0 {
                delay += (rng.unit() * f64::from(c.jitter_ms)) as u64;
            }
        }
    }
    let seq = s.seq.fetch_add(1, Ordering::Relaxed);
    s.queue.lock().expect("queue").push(Reverse(Due {
        at: Instant::now() + Duration::from_millis(delay),
        seq,
        from: source,
        to,
        bytes: bytes.to_vec(),
    }));
    s.wake.notify_all();
}

fn drop_one(s: &Shared) {
    s.dropped.fetch_add(1, Ordering::Relaxed);
}

/// Sends each queued datagram when its time comes.
fn send_loop(s: &Shared) {
    let mut q = s.queue.lock().expect("queue");
    while !s.stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        match q.peek() {
            Some(Reverse(d)) if d.at <= now => {
                let Reverse(d) = q.pop().expect("peeked");
                drop(q);
                let sock = {
                    let st = s.state.lock().expect("fabric");
                    st.socks.get(d.from).map(|k| k.sock.clone())
                };
                if let Some(sock) = sock {
                    if sock.send_to(&d.bytes, d.to).is_ok() {
                        s.relayed.fetch_add(1, Ordering::Relaxed);
                    }
                }
                q = s.queue.lock().expect("queue");
            }
            Some(Reverse(d)) => {
                let wait = d.at - now;
                q = s.wake.wait_timeout(q, wait).expect("queue").0;
            }
            None => q = s.wake.wait_timeout(q, POLL).expect("queue").0,
        }
    }
}
