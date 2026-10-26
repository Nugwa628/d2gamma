//! A simulated network with NAT routers, in memory and deterministic.
//!
//! Endpoints are addresses: public ones are reached directly, private ones
//! sit behind a [`NatKind`] router with one public IP. A datagram sent from
//! a private endpoint leaves through its router (which picks or reuses a
//! public port by its kind), travels for the network's latency, and is
//! delivered when [`SimNet::step`] reaches its time; a router lets an
//! arriving datagram in by its kind's filter, or drops it. A static
//! mapping ([`SimNet::map_port`]) stands for a UPnP, NAT-PMP or PCP
//! mapping and lets everything in on its port. [`SimQuic`] runs simulated
//! QUIC handshakes over it.
//!
//! The kinds follow RFC 4787's mapping and filtering behaviours:
//!
//! | Kind | Mapping | Filtering |
//! |---|---|---|
//! | `FullCone` | endpoint-independent | none |
//! | `Restricted` | endpoint-independent | address-dependent |
//! | `PortRestricted` | endpoint-independent | address-and-port-dependent |
//! | `Symmetric` | address-and-port-dependent | address-and-port-dependent |

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::{IpAddr, SocketAddr};

/// How a simulated router maps and filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NatKind {
    FullCone,
    Restricted,
    PortRestricted,
    Symmetric,
}

/// A router's index in its [`SimNet`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NatId(usize);

#[derive(Debug)]
struct Port {
    internal: SocketAddr,
    /// Remote endpoints the internal one has sent to through this port.
    sent_to: HashSet<SocketAddr>,
    /// A static mapping: everything gets in.
    open: bool,
}

#[derive(Debug)]
struct Nat {
    kind: NatKind,
    ip: IpAddr,
    next_port: u16,
    /// (internal, destination for symmetric routers) → public port.
    out: HashMap<(SocketAddr, Option<SocketAddr>), u16>,
    ports: HashMap<u16, Port>,
}

impl Nat {
    fn alloc(&mut self, wanted: u16) -> u16 {
        if wanted != 0 && !self.ports.contains_key(&wanted) {
            return wanted;
        }
        while self.ports.contains_key(&self.next_port) {
            self.next_port = self.next_port.wrapping_add(1).max(1024);
        }
        let p = self.next_port;
        self.next_port = self.next_port.wrapping_add(1).max(1024);
        p
    }

    /// The public port a datagram from `from` to `to` leaves on.
    fn outbound(&mut self, from: SocketAddr, to: SocketAddr) -> u16 {
        let key = match self.kind {
            NatKind::Symmetric => (from, Some(to)),
            _ => (from, None),
        };
        let port = match self.out.get(&key) {
            Some(&p) => p,
            None => {
                // Cone routers keep the internal port when it is free; a
                // symmetric one takes a fresh port per destination.
                let wanted = match self.kind {
                    NatKind::Symmetric => 0,
                    _ => from.port(),
                };
                let p = self.alloc(wanted);
                self.out.insert(key, p);
                self.ports.insert(
                    p,
                    Port {
                        internal: from,
                        sent_to: HashSet::new(),
                        open: false,
                    },
                );
                p
            }
        };
        if let Some(entry) = self.ports.get_mut(&port) {
            entry.sent_to.insert(to);
        }
        port
    }

    /// Where a datagram from `from` to public port `port` goes, if in.
    fn inbound(&self, port: u16, from: SocketAddr) -> Option<SocketAddr> {
        let entry = self.ports.get(&port)?;
        let pass = entry.open
            || match self.kind {
                NatKind::FullCone => true,
                NatKind::Restricted => entry.sent_to.iter().any(|a| a.ip() == from.ip()),
                NatKind::PortRestricted | NatKind::Symmetric => entry.sent_to.contains(&from),
            };
        pass.then_some(entry.internal)
    }
}

struct InFlight {
    at_ms: u64,
    to: SocketAddr,
    from: SocketAddr,
    bytes: Vec<u8>,
}

/// The simulated network.
pub struct SimNet {
    now_ms: u64,
    latency_ms: u64,
    nats: Vec<Nat>,
    /// Every endpoint, with its router for private ones.
    endpoints: HashMap<SocketAddr, Option<usize>>,
    in_flight: Vec<InFlight>,
    inbox: HashMap<SocketAddr, VecDeque<(SocketAddr, Vec<u8>)>>,
    dropped: usize,
}

impl SimNet {
    /// A network where every datagram takes `latency_ms`.
    pub fn new(latency_ms: u64) -> Self {
        Self {
            now_ms: 0,
            latency_ms,
            nats: Vec::new(),
            endpoints: HashMap::new(),
            in_flight: Vec::new(),
            inbox: HashMap::new(),
            dropped: 0,
        }
    }

    pub fn add_nat(&mut self, kind: NatKind, public_ip: IpAddr) -> NatId {
        self.nats.push(Nat {
            kind,
            ip: public_ip,
            next_port: 40_000,
            out: HashMap::new(),
            ports: HashMap::new(),
        });
        NatId(self.nats.len() - 1)
    }

    pub fn add_public(&mut self, addr: SocketAddr) {
        self.endpoints.insert(addr, None);
    }

    pub fn add_private(&mut self, addr: SocketAddr, nat: NatId) {
        self.endpoints.insert(addr, Some(nat.0));
    }

    /// A static mapping of public port `port` to private endpoint
    /// `internal`; its outbound traffic leaves on that port too. The
    /// mapped public address, or `None` for a public endpoint or a port
    /// already taken.
    pub fn map_port(&mut self, internal: SocketAddr, port: u16) -> Option<SocketAddr> {
        let nat = &mut self.nats[(*self.endpoints.get(&internal)?)?];
        if let Some(entry) = nat.ports.get_mut(&port) {
            if entry.internal != internal {
                return None;
            }
            entry.open = true;
        } else {
            nat.ports.insert(
                port,
                Port {
                    internal,
                    sent_to: HashSet::new(),
                    open: true,
                },
            );
        }
        if nat.kind != NatKind::Symmetric {
            nat.out.insert((internal, None), port);
        }
        Some(SocketAddr::new(nat.ip, port))
    }

    /// Sends a datagram; it is delivered `latency_ms` after the current
    /// time (the last [`SimNet::step`]).
    pub fn send(&mut self, from: SocketAddr, to: SocketAddr, bytes: Vec<u8>) {
        let seen = match self.endpoints.get(&from) {
            Some(Some(n)) => {
                let nat = &mut self.nats[*n];
                SocketAddr::new(nat.ip, nat.outbound(from, to))
            }
            Some(None) => from,
            None => panic!("{from} is not on the network"),
        };
        self.in_flight.push(InFlight {
            at_ms: self.now_ms + self.latency_ms,
            to,
            from: seen,
            bytes,
        });
    }

    /// Moves the clock to `now_ms` and delivers what is due.
    pub fn step(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
        let (due, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.in_flight)
            .into_iter()
            .partition(|p| p.at_ms <= now_ms);
        self.in_flight = later;
        for p in due {
            match self.route(p.to, p.from) {
                Some(dest) => self
                    .inbox
                    .entry(dest)
                    .or_default()
                    .push_back((p.from, p.bytes)),
                None => self.dropped += 1,
            }
        }
    }

    fn route(&self, to: SocketAddr, from: SocketAddr) -> Option<SocketAddr> {
        if let Some(nat) = self.nats.iter().find(|n| n.ip == to.ip()) {
            return nat.inbound(to.port(), from);
        }
        matches!(self.endpoints.get(&to), Some(None)).then_some(to)
    }

    /// The next datagram delivered to `at`: the address it came from (as
    /// seen after the sender's router) and its bytes.
    pub fn recv(&mut self, at: SocketAddr) -> Option<(SocketAddr, Vec<u8>)> {
        self.inbox.get_mut(&at)?.pop_front()
    }

    /// Datagrams a router or a missing endpoint dropped so far.
    pub fn dropped(&self) -> usize {
        self.dropped
    }
}

/// How often a simulated QUIC client resends its first packet.
pub const SIM_RESEND_MS: u64 = 250;

const INITIAL: &[u8; 2] = b"QI";
const HANDSHAKE: &[u8; 2] = b"QH";

struct SimConn {
    from: SocketAddr,
    to: SocketAddr,
    give_up_ms: u64,
    next_send_ms: u64,
    result: Option<bool>,
    seen_by_server: Option<SocketAddr>,
}

/// Simulated QUIC handshakes over a [`SimNet`]: a client resends its first
/// packet every [`SIM_RESEND_MS`] until a listener's answer arrives or it
/// gives up, as a real QUIC client does. Datagrams that aren't handshakes
/// (punches) are kept for [`SimQuic::recv_other`].
#[derive(Default)]
pub struct SimQuic {
    listeners: HashSet<SocketAddr>,
    conns: Vec<SimConn>,
    accepted: HashMap<SocketAddr, Vec<SocketAddr>>,
    other: HashMap<SocketAddr, VecDeque<(SocketAddr, Vec<u8>)>>,
}

impl SimQuic {
    pub fn new() -> Self {
        Self::default()
    }

    /// `addr` accepts connections (a QUIC server endpoint on that socket).
    pub fn listen(&mut self, addr: SocketAddr) {
        self.listeners.insert(addr);
    }

    /// Starts a connection from `from` to `to`; its id.
    pub fn connect(
        &mut self,
        net: &mut SimNet,
        from: SocketAddr,
        to: SocketAddr,
        now_ms: u64,
        timeout_ms: u64,
    ) -> usize {
        let id = self.conns.len();
        self.conns.push(SimConn {
            from,
            to,
            give_up_ms: now_ms + timeout_ms,
            next_send_ms: now_ms + SIM_RESEND_MS,
            result: None,
            seen_by_server: None,
        });
        net.send(from, to, packet(INITIAL, id));
        id
    }

    /// Handles what [`SimNet::step`] delivered, resends and gives up.
    pub fn step(&mut self, net: &mut SimNet, now_ms: u64) {
        let mut addrs: Vec<SocketAddr> = self.listeners.iter().copied().collect();
        addrs.extend(self.conns.iter().map(|c| c.from));
        addrs.sort();
        addrs.dedup();
        for at in addrs {
            while let Some((from, bytes)) = net.recv(at) {
                match read_packet(&bytes) {
                    Some((INITIAL, id)) if self.listeners.contains(&at) => {
                        if let Some(c) = self.conns.get_mut(id) {
                            if c.seen_by_server.is_none() {
                                c.seen_by_server = Some(from);
                                self.accepted.entry(at).or_default().push(from);
                            }
                        }
                        net.send(at, from, packet(HANDSHAKE, id));
                    }
                    Some((HANDSHAKE, id)) => {
                        if let Some(c) = self.conns.get_mut(id) {
                            if c.from == at && c.result.is_none() {
                                c.result = Some(true);
                            }
                        }
                    }
                    _ => self.other.entry(at).or_default().push_back((from, bytes)),
                }
            }
        }
        for (id, c) in self.conns.iter_mut().enumerate() {
            if c.result.is_some() {
                continue;
            }
            if now_ms >= c.give_up_ms {
                c.result = Some(false);
            } else if now_ms >= c.next_send_ms {
                c.next_send_ms = now_ms + SIM_RESEND_MS;
                net.send(c.from, c.to, packet(INITIAL, id));
            }
        }
    }

    /// `Some(true)` once connected, `Some(false)` once given up.
    pub fn result(&self, id: usize) -> Option<bool> {
        self.conns.get(id)?.result
    }

    /// The address the server saw the connection come from.
    pub fn seen_by_server(&self, id: usize) -> Option<SocketAddr> {
        self.conns.get(id)?.seen_by_server
    }

    /// Connections listener `at` accepted since the last call: where each
    /// came from.
    pub fn accepted(&mut self, at: SocketAddr) -> Vec<SocketAddr> {
        self.accepted.remove(&at).unwrap_or_default()
    }

    /// The next non-handshake datagram delivered to `at`.
    pub fn recv_other(&mut self, at: SocketAddr) -> Option<(SocketAddr, Vec<u8>)> {
        self.other.get_mut(&at)?.pop_front()
    }
}

fn packet(kind: &[u8; 2], id: usize) -> Vec<u8> {
    let mut p = kind.to_vec();
    p.extend_from_slice(&(id as u32).to_le_bytes());
    p
}

fn read_packet(bytes: &[u8]) -> Option<(&[u8; 2], usize)> {
    let kind: &[u8; 2] = bytes.get(..2)?.try_into().ok()?;
    let id: [u8; 4] = bytes.get(2..)?.try_into().ok()?;
    let kind = [INITIAL, HANDSHAKE].into_iter().find(|k| *k == kind)?;
    Some((kind, u32::from_le_bytes(id) as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(s: &str) -> SocketAddr {
        s.parse().unwrap()
    }

    /// A private endpoint behind `kind` sends to one server, then a second
    /// server (another port on the same host) and a third host try to get
    /// in on the public address the first one saw.
    fn reach(kind: NatKind) -> (bool, bool, bool) {
        let mut net = SimNet::new(1);
        let nat = net.add_nat(kind, "198.51.100.1".parse().unwrap());
        let inside = a("10.0.0.2:5000");
        net.add_private(inside, nat);
        let s1 = a("203.0.113.1:1000");
        let s1b = a("203.0.113.1:1001");
        let s2 = a("192.0.2.9:1000");
        for s in [s1, s1b, s2] {
            net.add_public(s);
        }
        net.send(inside, s1, vec![1]);
        net.step(1);
        let (seen, _) = net.recv(s1).unwrap();
        net.send(inside, s1b, vec![2]);
        net.step(2);
        let (seen_b, _) = net.recv(s1b).unwrap();
        let same_mapping = seen == seen_b;
        // Strangers try the first mapping: the same host from a new port,
        // and another host.
        let other = a("203.0.113.1:1002");
        net.add_public(other);
        net.send(other, seen, vec![3]);
        net.send(s2, seen, vec![4]);
        net.step(3);
        let mut got = Vec::new();
        while let Some((_, b)) = net.recv(inside) {
            got.push(b[0]);
        }
        (same_mapping, got.contains(&3), got.contains(&4))
    }

    #[test]
    fn kinds_map_and_filter_as_documented() {
        assert_eq!(reach(NatKind::FullCone), (true, true, true));
        assert_eq!(reach(NatKind::Restricted), (true, true, false));
        assert_eq!(reach(NatKind::PortRestricted), (true, false, false));
        assert_eq!(reach(NatKind::Symmetric), (false, false, false));
    }

    #[test]
    fn replies_come_back_and_static_mappings_let_anyone_in() {
        let mut net = SimNet::new(5);
        let nat = net.add_nat(NatKind::Symmetric, "198.51.100.1".parse().unwrap());
        let inside = a("10.0.0.2:6113");
        net.add_private(inside, nat);
        let server = a("203.0.113.1:6112");
        let stranger = a("192.0.2.9:7000");
        net.add_public(server);
        net.add_public(stranger);
        net.send(inside, server, b"hi".to_vec());
        net.step(4);
        assert!(net.recv(server).is_none(), "not before the latency");
        net.step(5);
        let (seen, bytes) = net.recv(server).unwrap();
        assert_eq!(bytes, b"hi");
        net.send(server, seen, b"back".to_vec());
        net.step(10);
        assert_eq!(net.recv(inside).unwrap(), (server, b"back".to_vec()));

        let mapped = net.map_port(inside, 6113).unwrap();
        assert_eq!(mapped, a("198.51.100.1:6113"));
        net.send(stranger, mapped, b"in".to_vec());
        net.step(20);
        assert_eq!(net.recv(inside).unwrap(), (stranger, b"in".to_vec()));
        assert_eq!(
            net.map_port(server, 1),
            None,
            "public endpoints have no router"
        );
        assert_eq!(net.dropped(), 0);
    }
}
