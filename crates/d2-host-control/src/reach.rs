//! Reachability as the hosting panel shows it: the game server opens its
//! port, the main server checks it from outside, and the game server
//! reports the answer on its `reach` line (contracts 11.12; the port
//! opening, the check and hole punching are `d2-reach`'s, in the game
//! server). This side only reads that line and picks the panel's words.

use std::fmt::Write as _;
use std::net::SocketAddr;

use crate::lines::{field, LineError, Ready};

/// `reach state=`: what the outside check found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReachState {
    /// Reachable without a mapping.
    Open,
    /// Reachable through the router mapping the game server made.
    Mapped,
    /// Not reachable from outside, but hole punching should get most
    /// joiners in.
    Punch,
    /// Not reachable, and punching can't work (symmetric or blocked NAT).
    Unreachable,
}

impl ReachState {
    fn from_word(w: &str) -> Option<Self> {
        Some(match w {
            "open" => ReachState::Open,
            "mapped" => ReachState::Mapped,
            "punch" => ReachState::Punch,
            "unreachable" => ReachState::Unreachable,
            _ => return None,
        })
    }
}

/// `reach method=`: how the port was opened on the router.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortMethod {
    None,
    /// UPnP Internet Gateway Device.
    Upnp,
    NatPmp,
    Pcp,
}

impl PortMethod {
    fn from_word(w: &str) -> Self {
        match w {
            "upnp" => PortMethod::Upnp,
            "natpmp" => PortMethod::NatPmp,
            "pcp" => PortMethod::Pcp,
            _ => PortMethod::None,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            PortMethod::None => "none",
            PortMethod::Upnp => "upnp",
            PortMethod::NatPmp => "natpmp",
            PortMethod::Pcp => "pcp",
        }
    }

    /// The name support reads.
    pub fn name(self) -> &'static str {
        match self {
            PortMethod::None => "none",
            PortMethod::Upnp => "UPnP-IGD",
            PortMethod::NatPmp => "NAT-PMP",
            PortMethod::Pcp => "PCP",
        }
    }
}

/// `reach nat=`: the NAT type the main server found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NatKind {
    Unknown,
    Open,
    Cone,
    Symmetric,
}

impl NatKind {
    fn from_word(w: &str) -> Self {
        match w {
            "open" => NatKind::Open,
            "cone" => NatKind::Cone,
            "symmetric" => NatKind::Symmetric,
            _ => NatKind::Unknown,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            NatKind::Unknown => "unknown",
            NatKind::Open => "open",
            NatKind::Cone => "cone",
            NatKind::Symmetric => "symmetric",
        }
    }
}

/// One `reach` line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReachReport {
    pub state: ReachState,
    pub method: PortMethod,
    /// The address the check reached, or the one the main server saw.
    pub addr: Option<SocketAddr>,
    pub nat: NatKind,
}

impl ReachReport {
    /// The fields after `reach`. An unknown state refuses the line (the
    /// panel can't guess); an unknown method or NAT word reads as none or
    /// unknown.
    pub(crate) fn parse(rest: &str) -> Result<Self, LineError> {
        let state = field(rest, "state").ok_or_else(|| LineError::Bad("reach: no state".into()))?;
        Ok(ReachReport {
            state: ReachState::from_word(state)
                .ok_or_else(|| LineError::Bad(format!("reach: unknown state {state}")))?,
            method: PortMethod::from_word(field(rest, "method").unwrap_or("none")),
            addr: field(rest, "addr").and_then(|a| a.parse().ok()),
            nat: NatKind::from_word(field(rest, "nat").unwrap_or("unknown")),
        })
    }
}

/// How joiners get in, when they do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReachHow {
    /// The port answers from outside as it is.
    Direct,
    /// The game server opened the port on the router.
    PortOpened(PortMethod),
    /// The main server coordinates hole punching for each joiner.
    HolePunching,
}

/// What the hosting panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReachView {
    /// The game server hasn't linked yet.
    Starting,
    /// Linked; the port is being opened and checked (also after "Check
    /// again").
    Opening,
    Reachable(ReachHow),
    /// The manual help panel (plan N11): port-forward instructions and
    /// "Copy details for support".
    NotReachable,
    /// The game server isn't running.
    Stopped,
}

impl ReachView {
    pub fn of(report: &ReachReport) -> Self {
        match report.state {
            ReachState::Open => ReachView::Reachable(ReachHow::Direct),
            ReachState::Mapped => ReachView::Reachable(ReachHow::PortOpened(report.method)),
            ReachState::Punch => ReachView::Reachable(ReachHow::HolePunching),
            ReachState::Unreachable => ReachView::NotReachable,
        }
    }

    /// The panel's line.
    pub fn text(self) -> &'static str {
        match self {
            ReachView::Starting => "Starting the game server...",
            ReachView::Opening => "Opening port...",
            ReachView::Reachable(ReachHow::Direct) => "Reachable",
            ReachView::Reachable(ReachHow::PortOpened(_)) => {
                "Reachable (port opened automatically)"
            }
            ReachView::Reachable(ReachHow::HolePunching) => {
                "Reachable for most players (through hole punching)"
            }
            ReachView::NotReachable => "Not reachable: see help",
            ReachView::Stopped => "Not hosting",
        }
    }

    /// Shows the help panel (port forwarding, details for support).
    pub fn needs_help(self) -> bool {
        self == ReachView::NotReachable
    }
}

/// The text "Copy details for support" puts on the clipboard: what the
/// client knows about the host's reachability. It holds no account
/// secret: no token, no password, no account name.
pub fn support_details(ready: Option<&Ready>, report: Option<&ReachReport>) -> String {
    let mut s = String::from("D2RUST hosting details\n");
    match ready {
        Some(r) => {
            let _ = writeln!(s, "game server: {}", r.node);
            let _ = writeln!(s, "UDP port: {}", r.listen.port());
        }
        None => s.push_str("game server: not started\n"),
    }
    match report {
        Some(r) => {
            let view = ReachView::of(r);
            let _ = writeln!(s, "result: {}", view.text());
            let _ = writeln!(s, "port opening: {}", r.method.name());
            let _ = writeln!(s, "NAT type: {}", r.nat.word());
            match r.addr {
                Some(a) => {
                    let _ = writeln!(s, "address seen by the main server: {a}");
                }
                None => s.push_str("address seen by the main server: none\n"),
            }
        }
        None => s.push_str("result: not checked yet\n"),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lines::{parse_node_line, NodeLine};

    fn view(line: &str) -> ReachView {
        match parse_node_line(line).unwrap() {
            NodeLine::Reach(r) => ReachView::of(&r),
            l => panic!("{l:?}"),
        }
    }

    /// The lines `d2_reach::reach_line` writes, as the game server prints
    /// them, give the panel's three states.
    #[test]
    fn each_reach_line_has_its_panel_words() {
        let mapped = view("reach state=mapped method=upnp addr=203.0.113.7:6113 nat=cone");
        assert_eq!(
            mapped,
            ReachView::Reachable(ReachHow::PortOpened(PortMethod::Upnp))
        );
        assert_eq!(mapped.text(), "Reachable (port opened automatically)");
        let open = view("reach state=open method=none addr=203.0.113.7:40001 nat=open");
        assert_eq!(open.text(), "Reachable");
        let punch = view("reach state=punch method=none addr=203.0.113.7:40001 nat=cone");
        assert_eq!(punch, ReachView::Reachable(ReachHow::HolePunching));
        assert!(!punch.needs_help());
        let none = view("reach state=unreachable method=none addr=203.0.113.7:40001 nat=symmetric");
        assert_eq!(none.text(), "Not reachable: see help");
        assert!(none.needs_help());
        assert_eq!(ReachView::Opening.text(), "Opening port...");
    }

    #[test]
    fn unknown_words_are_safe() {
        assert!(parse_node_line("reach state=maybe").is_err());
        assert!(parse_node_line("reach method=upnp").is_err());
        let NodeLine::Reach(r) =
            parse_node_line("reach state=open method=teleport nat=odd").unwrap()
        else {
            panic!()
        };
        assert_eq!(
            (r.method, r.nat, r.addr),
            (PortMethod::None, NatKind::Unknown, None)
        );
    }

    #[test]
    fn support_details_say_what_was_found() {
        let ready = Ready {
            node: "h-0123456789ab".into(),
            listen: "0.0.0.0:6114".parse().unwrap(),
            cert_sha256: [7; 32],
        };
        let r = ReachReport {
            state: ReachState::Unreachable,
            method: PortMethod::None,
            addr: Some("198.51.100.4:51000".parse().unwrap()),
            nat: NatKind::Symmetric,
        };
        let d = support_details(Some(&ready), Some(&r));
        for want in [
            "h-0123456789ab",
            "UDP port: 6114",
            "NAT type: symmetric",
            "198.51.100.4:51000",
            "Not reachable",
        ] {
            assert!(d.contains(want), "{want}: {d}");
        }
        assert!(support_details(None, None).contains("not checked yet"));
    }
}
