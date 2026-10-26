//! Reachability for game servers: how players on the internet get to a
//! game server running on a player's PC (`d2rust-node --host`).
//!
//! The order is fixed by the multiplayer v2 plan (section D, modules N9
//! and N10): first ask the home router to open the game server's UDP port,
//! then let the main server check from outside whether the port answers,
//! and when it doesn't, let the main server coordinate UDP hole punching
//! for each joiner. There is no relay.
//!
//! All multiplayer traffic is QUIC (`d2-quic`), and so is all of this:
//!
//! - [`portmap`]: automatic port opening with UPnP-IGD, NAT-PMP and PCP
//!   (whichever answers first) for the UDP port the game server's QUIC
//!   endpoint listens on, renewed while hosting and removed on exit
//!   ([`PortMapper`]).
//! - [`check`]: the outside check. The game server reports its mapping
//!   ([`ReachInfo`]); the main server makes real QUIC connections to the
//!   host's public addresses from endpoints the host never talked to, and
//!   takes the address of the host's own QUIC ping to the observer endpoint as a
//!   second observation. It answers [`ReachResult`]: reachable or not, and
//!   the NAT type it found.
//! - [`punch`]: hole punching. The main server swaps the host's and the
//!   joiner's observed addresses ([`punch::Punch`] to the host, `reach =
//!   punch` to the joiner), both sides send at once, and the joiner's QUIC
//!   connection goes through the openings.
//! - [`socket`]: the one UDP socket a QUIC endpoint and its punches share
//!   ([`SharedSocket`], served by `d2_quic::QuicServer::bind_socket`): a
//!   punch must leave from the address the QUIC connection then uses.
//! - [`quic`]: the outside check driven over `d2-quic`'s ping connections
//!   (ALPN `d2rust-ping/1`, `d2_quic::reach`).
//! - [`wire`]: the punch datagram, which never looks like QUIC.
//! - [`gateway`]: the default gateway and the local address facing it.
//!
//! The check and the punch logic are sans-I/O state machines driven with
//! a millisecond clock, so the same code runs on real sockets and in the
//! simulated NAT of the tests (`testkit`, feature `testkit`). The link
//! messages that carry [`ReachInfo`], [`ReachResult`], `Punch` and
//! `PunchDone` between the two programs are the link's (contracts v14
//! section 11.3, kinds 0x98-0x9B); this crate holds their contents and
//! codes, not their framing.
//!
//! The library builds on Windows and Linux: the game server runs on both.

use std::fmt;
use std::net::SocketAddr;

pub mod check;
pub mod gateway;
pub mod portmap;
pub mod punch;
pub mod quic;
pub mod socket;
pub mod wire;

#[cfg(any(test, feature = "testkit"))]
pub mod testkit;

pub use check::{CheckAction, OutsideCheck};
pub use portmap::{Attempt, MapEvent, Mapping, PortMapConfig, PortMapper};
pub use punch::{JoinReach, PunchCoordinator, PunchJob};
pub use socket::{PunchSender, SharedSocket};

/// How a game server's port was opened on its router (`ReachInfo.method`,
/// contracts 11.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReachMethod {
    /// No mapping: no router answered, or every answer was a refusal.
    None,
    /// UPnP Internet Gateway Device (`AddPortMapping`).
    UpnpIgd,
    /// NAT Port Mapping Protocol (RFC 6886).
    NatPmp,
    /// Port Control Protocol (RFC 6887).
    Pcp,
}

impl ReachMethod {
    /// The code on the link.
    pub fn code(self) -> u8 {
        match self {
            ReachMethod::None => 0,
            ReachMethod::UpnpIgd => 1,
            ReachMethod::NatPmp => 2,
            ReachMethod::Pcp => 3,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => ReachMethod::None,
            1 => ReachMethod::UpnpIgd,
            2 => ReachMethod::NatPmp,
            3 => ReachMethod::Pcp,
            _ => return None,
        })
    }

    /// The word the hosting process's `reach` line uses (contracts 11.12).
    pub fn word(self) -> &'static str {
        match self {
            ReachMethod::None => "none",
            ReachMethod::UpnpIgd => "upnp",
            ReachMethod::NatPmp => "natpmp",
            ReachMethod::Pcp => "pcp",
        }
    }
}

/// What the main server found out about the NAT in front of a game server
/// (`ReachResult.nat`, contracts 11.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NatType {
    /// Not enough answers to tell.
    Unknown,
    /// Strangers reach the port without a mapping: no NAT, or one that
    /// lets anything in once the port has sent out.
    Open,
    /// The port keeps one public address whatever it sends to (endpoint-
    /// independent mapping) but filters strangers: hole punching works.
    Cone,
    /// The public address changes with the destination: hole punching
    /// can't work, because the joiner is told an address the host's NAT
    /// doesn't use toward it.
    Symmetric,
}

impl NatType {
    /// The code on the link.
    pub fn code(self) -> u8 {
        match self {
            NatType::Unknown => 0,
            NatType::Open => 1,
            NatType::Cone => 2,
            NatType::Symmetric => 3,
        }
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            0 => NatType::Unknown,
            1 => NatType::Open,
            2 => NatType::Cone,
            3 => NatType::Symmetric,
            _ => return None,
        })
    }

    /// The word the hosting process's `reach` line uses (contracts 11.12).
    pub fn word(self) -> &'static str {
        match self {
            NatType::Unknown => "unknown",
            NatType::Open => "open",
            NatType::Cone => "cone",
            NatType::Symmetric => "symmetric",
        }
    }
}

/// N→R: how the game server opened its port (link message `ReachInfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReachInfo {
    pub method: ReachMethod,
    /// The public address the router mapped, when a mapping worked.
    pub mapped_addr: Option<SocketAddr>,
}

/// R→N: the outside check's answer (link message `ReachResult`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReachResult {
    /// A probe from a socket the host never talked to got an answer.
    pub reachable: bool,
    /// The address that answered, or when none did, the one the main
    /// server sees the game server's link come from.
    pub checked_addr: SocketAddr,
    pub nat: NatType,
}

/// The reachability state the hosting process reports to its game client
/// (`reach state=...`, contracts 11.12), from which the panel picks its
/// words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReachState {
    /// Reachable without a mapping.
    Open,
    /// Reachable through the router mapping.
    Mapped,
    /// Not reachable from outside, but hole punching may get joiners in.
    Punch,
    /// Not reachable and punching can't work: the manual help panel (N11).
    Unreachable,
}

impl ReachState {
    pub fn of(info: &ReachInfo, result: &ReachResult) -> Self {
        if result.reachable {
            if info.method != ReachMethod::None && info.mapped_addr == Some(result.checked_addr) {
                ReachState::Mapped
            } else {
                ReachState::Open
            }
        } else if result.nat == NatType::Symmetric {
            ReachState::Unreachable
        } else {
            ReachState::Punch
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            ReachState::Open => "open",
            ReachState::Mapped => "mapped",
            ReachState::Punch => "punch",
            ReachState::Unreachable => "unreachable",
        }
    }
}

/// The hosting process's `reach` line for its game client (contracts
/// 11.12), without the newline.
pub fn reach_line(info: &ReachInfo, result: &ReachResult) -> String {
    format!(
        "reach state={} method={} addr={} nat={}",
        ReachState::of(info, result).word(),
        info.method.word(),
        result.checked_addr,
        result.nat.word(),
    )
}

impl fmt::Display for ReachMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

impl fmt::Display for NatType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The codes are the link's (contracts v14, 11.3): a change here is a
    /// protocol change.
    #[test]
    fn codes_match_the_contracts() {
        let methods = [
            (ReachMethod::None, 0),
            (ReachMethod::UpnpIgd, 1),
            (ReachMethod::NatPmp, 2),
            (ReachMethod::Pcp, 3),
        ];
        for (m, c) in methods {
            assert_eq!(m.code(), c);
            assert_eq!(ReachMethod::from_code(c), Some(m));
        }
        assert_eq!(ReachMethod::from_code(4), None);
        let nats = [
            (NatType::Unknown, 0),
            (NatType::Open, 1),
            (NatType::Cone, 2),
            (NatType::Symmetric, 3),
        ];
        for (n, c) in nats {
            assert_eq!(n.code(), c);
            assert_eq!(NatType::from_code(c), Some(n));
        }
        assert_eq!(NatType::from_code(4), None);
    }

    #[test]
    fn reach_line_names_each_state() {
        let mapped: SocketAddr = "203.0.113.7:6113".parse().unwrap();
        let seen: SocketAddr = "203.0.113.7:40001".parse().unwrap();
        let info = ReachInfo {
            method: ReachMethod::UpnpIgd,
            mapped_addr: Some(mapped),
        };
        let ok = ReachResult {
            reachable: true,
            checked_addr: mapped,
            nat: NatType::Cone,
        };
        assert_eq!(
            reach_line(&info, &ok),
            "reach state=mapped method=upnp addr=203.0.113.7:6113 nat=cone"
        );
        let none = ReachInfo {
            method: ReachMethod::None,
            mapped_addr: None,
        };
        let open = ReachResult {
            reachable: true,
            checked_addr: seen,
            nat: NatType::Open,
        };
        assert_eq!(ReachState::of(&none, &open), ReachState::Open);
        let cone = ReachResult {
            reachable: false,
            checked_addr: seen,
            nat: NatType::Cone,
        };
        assert_eq!(ReachState::of(&none, &cone), ReachState::Punch);
        let sym = ReachResult {
            nat: NatType::Symmetric,
            ..cone
        };
        assert_eq!(ReachState::of(&none, &sym), ReachState::Unreachable);
        let unknown = ReachResult {
            nat: NatType::Unknown,
            ..cone
        };
        assert_eq!(ReachState::of(&none, &unknown), ReachState::Punch);
    }
}
