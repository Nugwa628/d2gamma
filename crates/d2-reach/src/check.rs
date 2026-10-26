//! The outside check: the main server finds out whether players can reach
//! a game server, and what NAT stands in front of it (plan N9).
//!
//! Every step is QUIC, like all multiplayer traffic. After the game server
//! has linked and tried its port mapping:
//!
//! 1. The game server sends [`ReachInfo`] on the link, and opens a QUIC
//!    ping connection (ALPN `d2rust-ping/1`, [`crate::quic`]) from its own
//!    game endpoint to the main server's observer endpoint: a second QUIC
//!    endpoint of the main server, on its own port
//!    ([`DEFAULT_OBSERVER_PORT`]).
//! 2. The main server tries a QUIC ping connection to each candidate
//!    address of the game server (the mapped address, then the address it
//!    sees the link come from), pinned to the game server's certificate,
//!    each from a fresh endpoint the game server has never talked to. A
//!    NAT that filters strangers drops the handshake; an open port or a
//!    working mapping completes it.
//! 3. The observer endpoint reports the address that QUIC connection came
//!    from ([`OutsideCheck::on_observed`]). The same public address as the
//!    link means the NAT keeps one address per socket (cone); another means
//!    it picks one per destination (symmetric), and hole punching can't
//!    work. This needs a second destination: the link's own connection
//!    can't tell, as a symmetric NAT keeps one mapping per destination.
//! 4. When every attempt has an answer and the observation arrived, or after
//!    [`CHECK_MS`], the check ends with [`ReachResult`].
//!
//! **Limit:** the main server connects from its own IP address, which the
//! link already talks to, so a NAT that filters by address only (not by
//! port) lets the attempt in and the check says "reachable" while strangers
//! elsewhere are filtered. Most home routers filter by address and port.
//! A second vantage (an official game server on another machine making the
//! attempts) would close the gap: attempts are plain actions, so who makes
//! them is the caller's choice.
//!
//! The state machine does no I/O and reads no clock: the caller passes
//! milliseconds and runs what [`OutsideCheck::poll`] returns
//! ([`crate::quic::drive_check`] does it with real QUIC connections).

use std::net::SocketAddr;

use crate::{NatType, ReachInfo, ReachResult};

/// The port of the main server's observer endpoint, a second QUIC
/// endpoint (ALPN `d2rust-ping/1`) next to the link's. Proposed default:
/// away from the main server's 6112, the game servers' 6113 and the next
/// ports a player host may take on a busy machine. One config value on
/// each side until the link names it.
pub const DEFAULT_OBSERVER_PORT: u16 = 6120;

/// How long one connection attempt may take. QUIC resends its first
/// packet within this, so a lost datagram doesn't fail the check.
pub const ATTEMPT_MS: u64 = 2_500;

/// How long a check waits for every answer.
pub const CHECK_MS: u64 = 3_000;

/// What the check asks its caller to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckAction {
    /// Try a QUIC ping connection to `to`, pinned to the game server's
    /// certificate, from an endpoint the game server has never talked to;
    /// give up after `timeout_ms`; report with [`OutsideCheck::on_attempt`].
    Connect { to: SocketAddr, timeout_ms: u64 },
    /// The check is over: send this to the game server (`ReachResult`).
    Done(ReachResult),
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    addr: SocketAddr,
    /// `None` until the attempt has its answer.
    connected: Option<bool>,
}

/// One outside check of one game server, on the main server.
#[derive(Clone, Debug)]
pub struct OutsideCheck {
    link_seen: SocketAddr,
    info: ReachInfo,
    candidates: Vec<Candidate>,
    observed: Option<SocketAddr>,
    started: bool,
    deadline_ms: u64,
    done: bool,
}

impl OutsideCheck {
    /// Starts a check of a game server whose link the main server sees
    /// come from `link_seen`, after its `ReachInfo`.
    pub fn new(link_seen: SocketAddr, info: ReachInfo, now_ms: u64) -> Self {
        let mut candidates = Vec::with_capacity(2);
        if let Some(mapped) = info.mapped_addr {
            candidates.push(mapped);
        }
        if !candidates.contains(&link_seen) {
            candidates.push(link_seen);
        }
        Self {
            link_seen,
            info,
            candidates: candidates
                .into_iter()
                .map(|addr| Candidate {
                    addr,
                    connected: None,
                })
                .collect(),
            observed: None,
            started: false,
            deadline_ms: now_ms + CHECK_MS,
            done: false,
        }
    }

    /// The address the link comes from: the observer endpoint routes a ping from
    /// this IP to this check.
    pub fn link_seen(&self) -> SocketAddr {
        self.link_seen
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    /// The observer endpoint accepted a ping connection from `from`. Only one from
    /// the link's own public IP counts: only the host's NAT sends from it.
    pub fn on_observed(&mut self, from: SocketAddr) {
        if from.ip() == self.link_seen.ip() && self.observed.is_none() {
            self.observed = Some(from);
        }
    }

    /// The attempt to `to` completed its handshake, or failed.
    pub fn on_attempt(&mut self, to: SocketAddr, connected: bool) {
        if let Some(c) = self.candidates.iter_mut().find(|c| c.addr == to) {
            c.connected = Some(connected);
        }
    }

    /// What to do now: the attempts (once), then the result (once).
    pub fn poll(&mut self, now_ms: u64) -> Vec<CheckAction> {
        if self.done {
            return Vec::new();
        }
        if !self.started {
            self.started = true;
            return self
                .candidates
                .iter()
                .map(|c| CheckAction::Connect {
                    to: c.addr,
                    timeout_ms: ATTEMPT_MS,
                })
                .collect();
        }
        let answered = self.candidates.iter().all(|c| c.connected.is_some());
        if now_ms >= self.deadline_ms || (answered && self.observed.is_some()) {
            self.done = true;
            return vec![CheckAction::Done(self.result())];
        }
        Vec::new()
    }

    /// The result from what has been heard so far.
    pub fn result(&self) -> ReachResult {
        let first = self.candidates.iter().find(|c| c.connected == Some(true));
        let link_connected = self
            .candidates
            .iter()
            .any(|c| c.connected == Some(true) && c.addr == self.link_seen);
        // When the link comes from the mapped address, a connection there
        // says the mapping works, not that the NAT lets strangers in.
        let open = link_connected && self.info.mapped_addr != Some(self.link_seen);
        ReachResult {
            reachable: first.is_some(),
            checked_addr: first.map_or(self.link_seen, |c| c.addr),
            nat: classify(self.link_seen, self.observed, open),
        }
    }
}

/// The NAT type from the link's and the observer's public addresses and
/// whether a stranger got in without a mapping.
fn classify(link_seen: SocketAddr, observed: Option<SocketAddr>, open: bool) -> NatType {
    match observed {
        Some(e) if e == link_seen => {
            if open {
                NatType::Open
            } else {
                NatType::Cone
            }
        }
        Some(_) => NatType::Symmetric,
        None if open => NatType::Open,
        None => NatType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::sim::{NatKind, SimNet, SimQuic};
    use crate::ReachMethod;

    struct Setup {
        net: SimNet,
        node: SocketAddr,
        realm: SocketAddr,
        observer: SocketAddr,
    }

    fn setup(kind: Option<NatKind>) -> Setup {
        let mut net = SimNet::new(10);
        let realm: SocketAddr = "203.0.113.1:6112".parse().unwrap();
        let observer = SocketAddr::new(realm.ip(), DEFAULT_OBSERVER_PORT);
        net.add_public(realm);
        net.add_public(observer);
        let node = match kind {
            Some(kind) => {
                let nat = net.add_nat(kind, "198.51.100.20".parse().unwrap());
                let node: SocketAddr = "192.168.1.10:6113".parse().unwrap();
                net.add_private(node, nat);
                node
            }
            None => {
                let node: SocketAddr = "198.51.100.30:6113".parse().unwrap();
                net.add_public(node);
                node
            }
        };
        Setup {
            net,
            node,
            realm,
            observer,
        }
    }

    /// Runs the whole check over the simulated network, every connection a
    /// simulated QUIC handshake: the link, the host's ping to the observer, and the
    /// main server's attempts from fresh endpoints.
    fn run(s: &mut Setup, mapped: Option<u16>) -> ReachResult {
        let info = match mapped {
            Some(port) => ReachInfo {
                method: ReachMethod::UpnpIgd,
                mapped_addr: Some(s.net.map_port(s.node, port).expect("behind a NAT")),
            },
            None => ReachInfo {
                method: ReachMethod::None,
                mapped_addr: None,
            },
        };
        let mut quic = SimQuic::new();
        quic.listen(s.realm);
        quic.listen(s.observer);
        quic.listen(s.node);
        let link = quic.connect(&mut s.net, s.node, s.realm, 0, 2_000);
        let observe = quic.connect(&mut s.net, s.node, s.observer, 0, 2_000);
        let mut check = None;
        let mut attempts: Vec<(usize, SocketAddr)> = Vec::new();
        let mut fresh_port = 50_000;
        for t in (0..=6_000).step_by(10) {
            s.net.step(t);
            quic.step(&mut s.net, t);
            if check.is_none() {
                if let Some(true) = quic.result(link) {
                    let seen = quic.seen_by_server(link).unwrap();
                    check = Some(OutsideCheck::new(seen, info, t));
                }
            }
            let Some(check) = check.as_mut() else {
                continue;
            };
            if let Some(true) = quic.result(observe) {
                check.on_observed(quic.seen_by_server(observe).unwrap());
            }
            attempts.retain(|&(id, to)| match quic.result(id) {
                Some(ok) => {
                    check.on_attempt(to, ok);
                    false
                }
                None => true,
            });
            for a in check.poll(t) {
                match a {
                    CheckAction::Connect { to, timeout_ms } => {
                        // A fresh endpoint the host never talked to.
                        let from = SocketAddr::new(s.realm.ip(), fresh_port);
                        fresh_port += 1;
                        s.net.add_public(from);
                        attempts.push((quic.connect(&mut s.net, from, to, t, timeout_ms), to));
                    }
                    CheckAction::Done(r) => return r,
                }
            }
        }
        panic!("the check never ended");
    }

    #[test]
    fn no_nat_is_open_and_reachable() {
        let mut s = setup(None);
        let r = run(&mut s, None);
        assert!(r.reachable);
        assert_eq!(r.nat, NatType::Open);
        assert_eq!(r.checked_addr, s.node);
    }

    #[test]
    fn full_cone_is_open() {
        let r = run(&mut setup(Some(NatKind::FullCone)), None);
        assert!(r.reachable);
        assert_eq!(r.nat, NatType::Open);
    }

    #[test]
    fn port_restricted_cone_is_unreachable_but_punchable() {
        let r = run(&mut setup(Some(NatKind::PortRestricted)), None);
        assert!(!r.reachable);
        assert_eq!(r.nat, NatType::Cone);
        assert_eq!(r.checked_addr.ip().to_string(), "198.51.100.20");
    }

    #[test]
    fn symmetric_is_found() {
        let r = run(&mut setup(Some(NatKind::Symmetric)), None);
        assert!(!r.reachable);
        assert_eq!(r.nat, NatType::Symmetric);
    }

    /// The documented limit: one vantage address can't see an
    /// address-only filter.
    #[test]
    fn address_restricted_looks_open_from_one_vantage() {
        let r = run(&mut setup(Some(NatKind::Restricted)), None);
        assert!(r.reachable);
    }

    #[test]
    fn a_mapping_makes_a_filtering_nat_reachable() {
        for kind in [NatKind::PortRestricted, NatKind::Symmetric] {
            let mut s = setup(Some(kind));
            let r = run(&mut s, Some(6113));
            assert!(r.reachable, "{kind:?}");
            assert_eq!(r.checked_addr, "198.51.100.20:6113".parse().unwrap());
        }
    }

    #[test]
    fn observation_from_another_ip_is_ignored() {
        let link: SocketAddr = "198.51.100.20:40000".parse().unwrap();
        let info = ReachInfo {
            method: ReachMethod::None,
            mapped_addr: None,
        };
        let mut check = OutsideCheck::new(link, info, 0);
        check.on_observed("192.0.2.66:40000".parse().unwrap());
        assert_eq!(check.result().nat, NatType::Unknown);
        check.on_observed("198.51.100.20:40001".parse().unwrap());
        assert_eq!(check.result().nat, NatType::Symmetric);
    }

    #[test]
    fn attempts_once_then_ends_at_the_deadline_only_once() {
        let link: SocketAddr = "198.51.100.20:40000".parse().unwrap();
        let mapped: SocketAddr = "198.51.100.20:6113".parse().unwrap();
        let info = ReachInfo {
            method: ReachMethod::NatPmp,
            mapped_addr: Some(mapped),
        };
        let mut check = OutsideCheck::new(link, info, 1_000);
        assert_eq!(
            check.poll(1_000),
            vec![
                CheckAction::Connect {
                    to: mapped,
                    timeout_ms: ATTEMPT_MS
                },
                CheckAction::Connect {
                    to: link,
                    timeout_ms: ATTEMPT_MS
                },
            ]
        );
        assert!(check.poll(1_100).is_empty(), "attempts are made once");
        check.on_attempt(mapped, false);
        check.on_attempt(link, false);
        assert!(check.poll(1_200).is_empty(), "waits for the observation");
        let end = check.poll(1_000 + CHECK_MS);
        assert!(matches!(
            end.as_slice(),
            [CheckAction::Done(ReachResult {
                reachable: false,
                nat: NatType::Unknown,
                ..
            })]
        ));
        assert!(check.is_done());
        assert!(check.poll(1_000 + CHECK_MS + 10).is_empty());
    }

    #[test]
    fn ends_early_with_every_answer() {
        let link: SocketAddr = "198.51.100.20:40000".parse().unwrap();
        let info = ReachInfo {
            method: ReachMethod::None,
            mapped_addr: None,
        };
        let mut check = OutsideCheck::new(link, info, 0);
        check.poll(0);
        check.on_attempt(link, false);
        check.on_observed(link);
        assert!(matches!(
            check.poll(20).as_slice(),
            [CheckAction::Done(ReachResult {
                reachable: false,
                nat: NatType::Cone,
                ..
            })]
        ));
    }
}
