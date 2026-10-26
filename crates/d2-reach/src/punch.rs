//! Hole punching, coordinated by the main server (plan N10).
//!
//! When the outside check found a host unreachable, each joiner gets in
//! like this:
//!
//! 1. The main server knows both public addresses: the host's from its
//!    link, the joiner's from the joiner's main-server connection. Both are
//!    the addresses of the sockets the two QUIC endpoints use (one socket
//!    per program, plan section D), so they are the ones to aim at.
//! 2. It sends the host [`Punch`] (the joiner's address) and the joiner
//!    `JoinNode` with `reach = punch` and the host's address, at the same
//!    moment ([`PunchCoordinator::order`]).
//! 3. The host sends a few punch datagrams ([`crate::wire::punch`]) from its
//!    QUIC endpoint's own socket to the joiner ([`PunchJob`], sent through
//!    [`crate::socket::PunchSender`]). They open the host's NAT toward the
//!    joiner; the joiner's endpoint drops them.
//! 4. The joiner connects with QUIC to the host's address and keeps
//!    resending its first packet for up to [`JOIN_WINDOW_MS`]. Its own
//!    packets open its NAT toward the host; once the host's punches have
//!    opened the host's side, the handshake goes through on the punched
//!    path and the game runs on that connection.
//! 5. The host reports [`PunchDone`]; a joiner that got nowhere gets
//!    `HostUnreachable` (78) from the main server within
//!    [`PUNCH_TIMEOUT_MS`].
//!
//! It works when both NATs keep one public address per socket (cone); a
//! symmetric NAT on either side uses another address toward the peer than
//! the one the main server saw, and the punch fails cleanly.

use std::collections::HashMap;
use std::net::SocketAddr;

use crate::wire;
use crate::ReachResult;

/// Punch datagrams the host sends per joiner.
pub const PUNCH_TRIES: u8 = 10;

/// Time between them.
pub const PUNCH_INTERVAL_MS: u64 = 200;

/// How long a joiner keeps resending its first QUIC packet toward a host
/// it was told to punch through to.
pub const JOIN_WINDOW_MS: u64 = 5_000;

/// How long the host waits for the joiner's connection after the punch
/// started, before it reports failure.
pub const HOST_WAIT_MS: u64 = JOIN_WINDOW_MS + 1_000;

/// How long the main server waits for `PunchDone` before it answers the
/// joiner `HostUnreachable` (the plan wants that within 10 s).
pub const PUNCH_TIMEOUT_MS: u64 = 8_000;

/// R→N: punch toward a joiner (link message `Punch`, contracts 11.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Punch {
    pub game: u32,
    /// The joiner's public address as the main server sees it.
    pub peer_addr: SocketAddr,
    /// The main server's clock when it sent the order; the host starts at
    /// once on receipt (within 50 ms).
    pub at_ms: u64,
    pub tries: u8,
}

/// N→R: how a punch ended (link message `PunchDone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PunchDone {
    pub game: u32,
    pub peer_addr: SocketAddr,
    pub connected: bool,
}

/// How a joiner reaches a host (`JoinNode.addr` and `JoinNode.reach`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinReach {
    /// Connect to this address as to any server.
    Direct(SocketAddr),
    /// Connect to this address and keep resending for [`JOIN_WINDOW_MS`]:
    /// the host punches toward the joiner meanwhile.
    Punch(SocketAddr),
}

impl JoinReach {
    /// How a joiner reaches a host whose link the main server sees from
    /// `link_seen`, given its outside check's result if there is one yet.
    pub fn for_host(link_seen: SocketAddr, check: Option<&ReachResult>) -> Self {
        match check {
            Some(r) if r.reachable => JoinReach::Direct(r.checked_addr),
            _ => JoinReach::Punch(link_seen),
        }
    }

    pub fn addr(self) -> SocketAddr {
        match self {
            JoinReach::Direct(a) | JoinReach::Punch(a) => a,
        }
    }

    /// `JoinNode.reach` (contracts 11.4): 0 direct, 1 punch.
    pub fn code(self) -> u8 {
        match self {
            JoinReach::Direct(_) => 0,
            JoinReach::Punch(_) => 1,
        }
    }
}

/// What the main server sends for one punched join.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PunchOrder {
    /// To the host, on its link.
    pub to_host: Punch,
    /// To the joiner, in its `JoinNode`.
    pub to_joiner: JoinReach,
}

/// How a punched join ended, for the main server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PunchOutcome {
    Connected {
        game: u32,
        joiner: SocketAddr,
    },
    /// Answer the joiner `HostUnreachable`.
    Failed {
        game: u32,
        joiner: SocketAddr,
        timed_out: bool,
    },
}

/// Counts the main server logs (plan N10: success and failure counts).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PunchStats {
    pub ordered: u64,
    pub connected: u64,
    pub failed: u64,
    pub timed_out: u64,
}

/// The main server's side: orders punches and follows them to their end.
#[derive(Debug, Default)]
pub struct PunchCoordinator {
    /// (game, joiner) → when the order gives up.
    pending: HashMap<(u32, SocketAddr), u64>,
    stats: PunchStats,
}

impl PunchCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    /// A joiner whose main-server connection comes from `joiner_seen` joins
    /// `game` on a host whose link comes from `host_seen`: what to send
    /// both. Asking again for the same pair starts over.
    pub fn order(
        &mut self,
        game: u32,
        host_seen: SocketAddr,
        joiner_seen: SocketAddr,
        now_ms: u64,
    ) -> PunchOrder {
        self.pending
            .insert((game, joiner_seen), now_ms + PUNCH_TIMEOUT_MS);
        self.stats.ordered += 1;
        PunchOrder {
            to_host: Punch {
                game,
                peer_addr: joiner_seen,
                at_ms: now_ms,
                tries: PUNCH_TRIES,
            },
            to_joiner: JoinReach::Punch(host_seen),
        }
    }

    /// The host's `PunchDone`. `None` for one the coordinator doesn't
    /// have open (late, repeated or forged).
    pub fn done(&mut self, d: &PunchDone) -> Option<PunchOutcome> {
        self.pending.remove(&(d.game, d.peer_addr))?;
        Some(if d.connected {
            self.stats.connected += 1;
            PunchOutcome::Connected {
                game: d.game,
                joiner: d.peer_addr,
            }
        } else {
            self.stats.failed += 1;
            PunchOutcome::Failed {
                game: d.game,
                joiner: d.peer_addr,
                timed_out: false,
            }
        })
    }

    /// Orders past their time: each joiner gets `HostUnreachable`.
    pub fn poll(&mut self, now_ms: u64) -> Vec<PunchOutcome> {
        let mut out: Vec<PunchOutcome> = Vec::new();
        self.pending.retain(|&(game, joiner), &mut give_up| {
            if now_ms < give_up {
                return true;
            }
            out.push(PunchOutcome::Failed {
                game,
                joiner,
                timed_out: true,
            });
            false
        });
        self.stats.timed_out += out.len() as u64;
        out.sort_by_key(|o| match o {
            PunchOutcome::Connected { game, joiner }
            | PunchOutcome::Failed { game, joiner, .. } => (*game, *joiner),
        });
        out
    }

    /// Punches the host forgets about when a game ends.
    pub fn game_ended(&mut self, game: u32) {
        self.pending.retain(|&(g, _), _| g != game);
    }

    pub fn stats(&self) -> PunchStats {
        self.stats
    }
}

/// What a [`PunchJob`] asks its game server to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PunchStep {
    /// Send these bytes to `to` through the QUIC endpoint's socket.
    Send { to: SocketAddr, bytes: Vec<u8> },
    /// The punch is over: send this on the link.
    Done(PunchDone),
}

/// The host's side of one punch.
#[derive(Clone, Debug)]
pub struct PunchJob {
    punch: Punch,
    sent: u8,
    next_ms: u64,
    give_up_ms: u64,
    done: bool,
}

impl PunchJob {
    /// Starts on receipt of `punch`.
    pub fn start(punch: Punch, now_ms: u64) -> Self {
        Self {
            punch,
            sent: 0,
            next_ms: now_ms,
            give_up_ms: now_ms + HOST_WAIT_MS,
            done: false,
        }
    }

    pub fn game(&self) -> u32 {
        self.punch.game
    }

    pub fn peer(&self) -> SocketAddr {
        self.punch.peer_addr
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    /// The endpoint accepted a QUIC connection from `from`. Counts when it
    /// comes from the joiner's IP (its port may differ behind some NATs).
    pub fn connected(&mut self, from: SocketAddr) -> Option<PunchDone> {
        if self.done || from.ip() != self.punch.peer_addr.ip() {
            return None;
        }
        self.done = true;
        Some(self.finish(true))
    }

    /// What to do now.
    pub fn poll(&mut self, now_ms: u64) -> Vec<PunchStep> {
        if self.done {
            return Vec::new();
        }
        if now_ms >= self.give_up_ms {
            self.done = true;
            return vec![PunchStep::Done(self.finish(false))];
        }
        let mut out = Vec::new();
        if self.sent < self.punch.tries && now_ms >= self.next_ms {
            self.sent += 1;
            self.next_ms = now_ms + PUNCH_INTERVAL_MS;
            out.push(PunchStep::Send {
                to: self.punch.peer_addr,
                bytes: wire::punch(self.punch.game),
            });
        }
        out
    }

    fn finish(&self, connected: bool) -> PunchDone {
        PunchDone {
            game: self.punch.game,
            peer_addr: self.punch.peer_addr,
            connected,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::sim::{NatKind, SimNet, SimQuic};
    use crate::NatType;

    const GAME: u32 = 42;

    /// The whole punched join over the simulated network: both links, the
    /// coordinator's order, the host's punches from its endpoint's socket and
    /// the joiner's QUIC connection. The outcome and when it arrived.
    fn punched_join(host_kind: NatKind, joiner_kind: NatKind) -> (PunchOutcome, u64) {
        let mut net = SimNet::new(15);
        let realm: SocketAddr = "203.0.113.1:6112".parse().unwrap();
        net.add_public(realm);
        let host_nat = net.add_nat(host_kind, "198.51.100.1".parse().unwrap());
        let joiner_nat = net.add_nat(joiner_kind, "192.0.2.1".parse().unwrap());
        let host: SocketAddr = "192.168.1.10:6113".parse().unwrap();
        let joiner: SocketAddr = "10.0.0.5:51000".parse().unwrap();
        net.add_private(host, host_nat);
        net.add_private(joiner, joiner_nat);

        let mut quic = SimQuic::new();
        quic.listen(realm);
        quic.listen(host);
        let host_link = quic.connect(&mut net, host, realm, 0, 2_000);
        let joiner_link = quic.connect(&mut net, joiner, realm, 0, 2_000);
        let mut t = 0;
        while quic.result(host_link).is_none() || quic.result(joiner_link).is_none() {
            t += 5;
            net.step(t);
            quic.step(&mut net, t);
        }
        let host_seen = quic.seen_by_server(host_link).unwrap();
        let joiner_seen = quic.seen_by_server(joiner_link).unwrap();

        let mut coord = PunchCoordinator::new();
        let order = coord.order(GAME, host_seen, joiner_seen, t);
        assert_eq!(order.to_joiner, JoinReach::Punch(host_seen));
        let start = t;
        // Both orders travel one link latency.
        let mut job: Option<PunchJob> = None;
        loop {
            t += 5;
            assert!(t < start + 20_000, "no outcome");
            net.step(t);
            quic.step(&mut net, t);
            if t >= start + 15 && job.is_none() {
                job = Some(PunchJob::start(order.to_host, t));
                quic.connect(&mut net, joiner, order.to_joiner.addr(), t, JOIN_WINDOW_MS);
            }
            let Some(job) = job.as_mut() else { continue };
            for from in quic.accepted(host) {
                if let Some(d) = job.connected(from) {
                    if let Some(o) = coord.done(&d) {
                        return (o, t - start);
                    }
                }
            }
            while let Some((from, bytes)) = quic.recv_other(joiner) {
                assert_eq!(
                    from, host_seen,
                    "the punch leaves from the host's QUIC socket"
                );
                assert_eq!(wire::read_punch(&bytes), Some(GAME));
            }
            for step in job.poll(t) {
                match step {
                    PunchStep::Send { to, bytes } => net.send(host, to, bytes),
                    PunchStep::Done(d) => {
                        if let Some(o) = coord.done(&d) {
                            return (o, t - start);
                        }
                    }
                }
            }
            if let Some(o) = coord.poll(t).into_iter().next() {
                return (o, t - start);
            }
        }
    }

    #[test]
    fn cone_to_cone_connects() {
        for (h, j) in [
            (NatKind::PortRestricted, NatKind::PortRestricted),
            (NatKind::PortRestricted, NatKind::Restricted),
            (NatKind::Restricted, NatKind::FullCone),
        ] {
            let (outcome, ms) = punched_join(h, j);
            assert!(
                matches!(outcome, PunchOutcome::Connected { game: GAME, .. }),
                "{h:?} to {j:?}: {outcome:?}"
            );
            assert!(ms < JOIN_WINDOW_MS, "{h:?} to {j:?} took {ms} ms");
        }
    }

    #[test]
    fn symmetric_fails_cleanly_within_ten_seconds() {
        for (h, j) in [
            (NatKind::Symmetric, NatKind::PortRestricted),
            (NatKind::PortRestricted, NatKind::Symmetric),
            (NatKind::Symmetric, NatKind::Symmetric),
        ] {
            let (outcome, ms) = punched_join(h, j);
            assert!(
                matches!(outcome, PunchOutcome::Failed { game: GAME, .. }),
                "{h:?} to {j:?}: {outcome:?}"
            );
            assert!(ms <= 10_000, "{h:?} to {j:?}: told after {ms} ms");
        }
    }

    #[test]
    fn coordinator_counts_and_times_out() {
        let host: SocketAddr = "198.51.100.1:6113".parse().unwrap();
        let a: SocketAddr = "192.0.2.1:50000".parse().unwrap();
        let b: SocketAddr = "192.0.2.2:50000".parse().unwrap();
        let mut c = PunchCoordinator::new();
        let o = c.order(1, host, a, 1_000);
        assert_eq!(o.to_host.peer_addr, a);
        assert_eq!(o.to_host.tries, PUNCH_TRIES);
        c.order(1, host, b, 1_000);
        let ok = PunchDone {
            game: 1,
            peer_addr: a,
            connected: true,
        };
        assert_eq!(
            c.done(&ok),
            Some(PunchOutcome::Connected { game: 1, joiner: a })
        );
        assert_eq!(c.done(&ok), None, "a repeated answer is ignored");
        assert!(c.poll(1_000 + PUNCH_TIMEOUT_MS - 1).is_empty());
        assert_eq!(
            c.poll(1_000 + PUNCH_TIMEOUT_MS),
            vec![PunchOutcome::Failed {
                game: 1,
                joiner: b,
                timed_out: true
            }]
        );
        assert_eq!(
            c.stats(),
            PunchStats {
                ordered: 2,
                connected: 1,
                failed: 0,
                timed_out: 1
            }
        );
        c.order(2, host, a, 0);
        c.game_ended(2);
        assert!(c.poll(u64::MAX).is_empty());
    }

    #[test]
    fn job_sends_its_tries_then_gives_up() {
        let peer: SocketAddr = "192.0.2.1:50000".parse().unwrap();
        let punch = Punch {
            game: 3,
            peer_addr: peer,
            at_ms: 0,
            tries: 3,
        };
        let mut job = PunchJob::start(punch, 100);
        let mut sent = 0;
        for t in (100..100 + HOST_WAIT_MS).step_by(10) {
            for s in job.poll(t) {
                match s {
                    PunchStep::Send { to, bytes } => {
                        assert_eq!(to, peer);
                        assert_eq!(wire::read_punch(&bytes), Some(3));
                        sent += 1;
                    }
                    PunchStep::Done(_) => panic!("too early"),
                }
            }
        }
        assert_eq!(sent, 3);
        assert_eq!(
            job.poll(100 + HOST_WAIT_MS),
            vec![PunchStep::Done(PunchDone {
                game: 3,
                peer_addr: peer,
                connected: false
            })]
        );
        assert!(job.connected(peer).is_none(), "over is over");

        let mut job = PunchJob::start(punch, 0);
        assert!(job.connected("192.0.2.9:50000".parse().unwrap()).is_none());
        assert!(job
            .connected("192.0.2.1:50123".parse().unwrap())
            .is_some_and(|d| d.connected));
    }

    #[test]
    fn reachable_hosts_are_joined_directly() {
        let seen: SocketAddr = "198.51.100.1:40000".parse().unwrap();
        let mapped: SocketAddr = "198.51.100.1:6113".parse().unwrap();
        let ok = ReachResult {
            reachable: true,
            checked_addr: mapped,
            nat: NatType::Cone,
        };
        assert_eq!(
            JoinReach::for_host(seen, Some(&ok)),
            JoinReach::Direct(mapped)
        );
        assert_eq!(JoinReach::Direct(mapped).code(), 0);
        let no = ReachResult {
            reachable: false,
            checked_addr: seen,
            nat: NatType::Symmetric,
        };
        assert_eq!(JoinReach::for_host(seen, Some(&no)), JoinReach::Punch(seen));
        assert_eq!(JoinReach::for_host(seen, None).code(), 1);
    }
}
