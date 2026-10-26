//! The line protocol between the game client and its hosting process
//! (contracts 11.12): one UTF-8 line per message, `key=value` fields
//! separated by spaces, at most [`MAX_LINE`] bytes, unknown keys ignored.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::reach::ReachReport;

/// The longest line either side sends, without its newline.
pub const MAX_LINE: usize = 1024;

/// The UDP port a player host asks for first (contracts 11.8); the game
/// server takes the next free one when it is taken.
pub const DEFAULT_PORT: u16 = 6113;

/// The host token from `HostGrant` (contracts 11.4): 16 random bytes, valid
/// while the session that asked for it lives. It goes to the hosting
/// process on its stdin only, never on a command line or into a log, so
/// `Debug` doesn't show it and its bytes are wiped when it is dropped.
#[derive(Clone, PartialEq, Eq)]
pub struct HostToken([u8; 16]);

impl HostToken {
    pub fn new(bytes: [u8; 16]) -> Self {
        HostToken(bytes)
    }

    /// Writes the token as the start line does: 32 lowercase hex digits.
    fn push_hex(&self, s: &mut String) {
        push_hex(s, &self.0);
    }
}

impl fmt::Debug for HostToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HostToken(..)")
    }
}

impl Drop for HostToken {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// Overwrites secret bytes before their memory is given back.
pub(crate) fn wipe(bytes: &mut [u8]) {
    bytes.fill(0);
    std::hint::black_box(bytes);
}

/// Why a line couldn't be made or read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineError {
    /// An empty line.
    Empty,
    /// Over [`MAX_LINE`] bytes.
    TooLong,
    /// A line kind this side doesn't know (ignored, so a newer game server
    /// can add lines).
    Unknown(String),
    /// A known line with a missing or broken field.
    Bad(String),
}

impl fmt::Display for LineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LineError::Empty => f.write_str("an empty line"),
            LineError::TooLong => write!(f, "a line over {MAX_LINE} bytes"),
            LineError::Unknown(kind) => write!(f, "an unknown line `{kind}`"),
            LineError::Bad(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for LineError {}

/// The first line the client writes: where the main server is, how to
/// pin it, the host token, the client's version and the port to try.
#[derive(Clone)]
pub struct StartLine {
    /// The main server's address for the link, `HOST:PORT` (`HostGrant.realm_addr`).
    pub realm: String,
    /// The SHA-256 of the main server's certificate, as the client pins it.
    pub realm_cert: [u8; 32],
    pub token: HostToken,
    /// The game client's version; the game server must be the same build.
    pub version: String,
    /// The UDP port to try first ([`DEFAULT_PORT`]; 0 = any free port, for tests).
    pub port: u16,
}

impl StartLine {
    /// The start line for a `HostGrant`: its token and link address, the
    /// main server's pinned certificate and this client's version.
    pub fn from_grant(
        token: [u8; 16],
        realm_addr: &str,
        realm_cert: [u8; 32],
        version: &str,
    ) -> Result<Self, LineError> {
        let s = StartLine {
            realm: realm_addr.to_string(),
            realm_cert,
            token: HostToken::new(token),
            version: version.to_string(),
            port: DEFAULT_PORT,
        };
        s.validate()?;
        Ok(s)
    }

    /// The address and version must be one word each.
    pub fn validate(&self) -> Result<(), LineError> {
        for (key, v) in [("realm", &self.realm), ("version", &self.version)] {
            if v.is_empty() || v.len() > 255 || v.chars().any(|c| c.is_whitespace() || c == '=') {
                return Err(LineError::Bad(format!("{key}: not one word")));
            }
        }
        Ok(())
    }

    /// The line with its newline, in one buffer and no copies (it holds
    /// the token): write it to the child's stdin, then wipe it.
    pub(crate) fn render(&self) -> Vec<u8> {
        let mut s = String::with_capacity(self.realm.len() + self.version.len() + 160);
        s.push_str("start realm=");
        s.push_str(&self.realm);
        s.push_str(" realm_cert=");
        push_hex(&mut s, &self.realm_cert);
        s.push_str(" token=");
        self.token.push_hex(&mut s);
        s.push_str(" version=");
        s.push_str(&self.version);
        s.push_str(" port=");
        s.push_str(&self.port.to_string());
        s.push('\n');
        s.into_bytes()
    }
}

impl fmt::Debug for StartLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StartLine")
            .field("realm", &self.realm)
            .field("realm_cert", &hex(&self.realm_cert))
            .field("token", &self.token)
            .field("version", &self.version)
            .field("port", &self.port)
            .finish()
    }
}

/// A line the client sends after the start line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostCommand {
    /// The PC is going to sleep (Windows `PBT_APMSUSPEND`): the game server
    /// saves everyone (`Save.reason` 4) and closes the game.
    Sleep,
    /// The host quits or stops hosting: save, close, exit.
    Stop,
    /// "Check again" in the reachability panel.
    Recheck,
}

impl HostCommand {
    pub fn line(self) -> &'static str {
        match self {
            HostCommand::Sleep => "sleep",
            HostCommand::Stop => "stop",
            HostCommand::Recheck => "recheck",
        }
    }
}

/// `ready`: the game server is linked to the main server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ready {
    /// Its id for this hosting session (`h-` and 12 hex digits).
    pub node: String,
    /// The address its UDP socket is bound to.
    pub listen: SocketAddr,
    /// The SHA-256 of its certificate (fresh for each hosting session).
    pub cert_sha256: [u8; 32],
}

impl Ready {
    /// Where the host's own client connects to play in its game: the
    /// listen address, on loopback when it is bound to every address.
    pub fn connect_addr(&self) -> SocketAddr {
        let ip = match self.listen.ip() {
            IpAddr::V4(ip) if ip.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(ip) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
            ip => ip,
        };
        SocketAddr::new(ip, self.listen.port())
    }
}

/// `game`: the hosted game and its player count, on change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameInfo {
    pub id: u32,
    pub players: u32,
}

/// `error`: the link was refused or lost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeError {
    /// An error code of contracts 11.7 (kept as the word sent).
    pub code: String,
    pub text: String,
}

impl NodeError {
    /// The code as a number, when it is one.
    pub fn number(&self) -> Option<u16> {
        self.code.parse().ok()
    }
}

/// A line from the hosting process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeLine {
    Ready(Ready),
    Reach(ReachReport),
    Game(GameInfo),
    /// The game closed (every save acknowledged or given up).
    Closed {
        reason: String,
    },
    Error(NodeError),
}

/// The `key=value` fields of a line after its kind; later keys win.
pub(crate) fn field<'a>(rest: &'a str, key: &str) -> Option<&'a str> {
    rest.split_whitespace()
        .filter_map(|w| w.split_once('='))
        .filter(|(k, _)| *k == key)
        .map(|(_, v)| v)
        .next_back()
}

fn need<'a>(rest: &'a str, kind: &str, key: &str) -> Result<&'a str, LineError> {
    field(rest, key).ok_or_else(|| LineError::Bad(format!("{kind}: no {key}")))
}

fn number(rest: &str, kind: &str, key: &str) -> Result<u32, LineError> {
    need(rest, kind, key)?
        .parse()
        .map_err(|_| LineError::Bad(format!("{kind}: {key} is not a number")))
}

/// Reads one line from the hosting process (without its newline).
pub fn parse_node_line(line: &str) -> Result<NodeLine, LineError> {
    if line.len() > MAX_LINE {
        return Err(LineError::TooLong);
    }
    let line = line.trim();
    let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
    match kind {
        "" => Err(LineError::Empty),
        "ready" => Ok(NodeLine::Ready(Ready {
            node: need(rest, kind, "node")?.to_string(),
            listen: need(rest, kind, "listen")?
                .parse()
                .map_err(|_| LineError::Bad("ready: listen is not an address".into()))?,
            cert_sha256: parse_hex32(need(rest, kind, "cert_sha256")?)
                .ok_or_else(|| LineError::Bad("ready: cert_sha256 is not 64 hex digits".into()))?,
        })),
        "reach" => ReachReport::parse(rest).map(NodeLine::Reach),
        "game" => Ok(NodeLine::Game(GameInfo {
            id: number(rest, kind, "id")?,
            players: number(rest, kind, "players")?,
        })),
        "closed" => Ok(NodeLine::Closed {
            reason: need(rest, kind, "reason")?.to_string(),
        }),
        "error" => Ok(NodeLine::Error(NodeError {
            code: need(rest, kind, "code")?.to_string(),
            // The text is the last field and may hold spaces.
            text: rest
                .find("text=")
                .filter(|&i| i == 0 || rest[..i].ends_with(' '))
                .map(|i| rest[i + 5..].trim().to_string())
                .unwrap_or_default(),
        })),
        other => Err(LineError::Unknown(other.to_string())),
    }
}

fn push_hex(s: &mut String, bytes: &[u8]) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    for b in bytes {
        s.push(DIGITS[usize::from(b >> 4)] as char);
        s.push(DIGITS[usize::from(b & 15)] as char);
    }
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    push_hex(&mut s, bytes);
    s
}

/// 64 hex digits (either case) as 32 bytes.
pub fn parse_hex32(s: &str) -> Option<[u8; 32]> {
    let s = s.as_bytes();
    if s.len() != 64 {
        return None;
    }
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = [0u8; 32];
    for (i, pair) in s.chunks(2).enumerate() {
        out[i] = (digit(pair[0])? << 4) | digit(pair[1])?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reach::{NatKind, PortMethod, ReachState};

    const CERT: &str = "00112233445566778899aabbccddeeff00112233445566778899AABBCCDDEEFF";

    #[test]
    fn the_start_line_is_the_contracts_line() {
        let s = StartLine::from_grant(
            [0xab; 16],
            "realm.example:6112",
            parse_hex32(CERT).unwrap(),
            "0.1.3.0",
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(s.render()).unwrap(),
            format!(
                "start realm=realm.example:6112 realm_cert={} token={} version=0.1.3.0 port=6113\n",
                CERT.to_lowercase(),
                "ab".repeat(16)
            )
        );
    }

    #[test]
    fn start_fields_must_be_one_word() {
        for (realm, version) in [("", "1"), ("a b", "1"), ("a:1", "1 2"), ("a:1", "x=y")] {
            assert!(
                StartLine::from_grant([0; 16], realm, [0; 32], version).is_err(),
                "{realm:?} {version:?}"
            );
        }
    }

    #[test]
    fn debug_never_shows_the_token() {
        let s = StartLine::from_grant([0xcd; 16], "127.0.0.1:6112", [0; 32], "1").unwrap();
        let text = format!("{s:?} {:?}", s.token);
        assert!(!text.contains(&"cd".repeat(16)), "{text}");
        assert!(text.contains("HostToken(..)"), "{text}");
    }

    #[test]
    fn commands_are_the_contracts_words() {
        assert_eq!(HostCommand::Sleep.line(), "sleep");
        assert_eq!(HostCommand::Stop.line(), "stop");
        assert_eq!(HostCommand::Recheck.line(), "recheck");
    }

    #[test]
    fn every_node_line_is_read() {
        let r = parse_node_line(&format!(
            "ready node=h-0123456789ab listen=0.0.0.0:6113 cert_sha256={CERT} extra=1"
        ))
        .unwrap();
        let NodeLine::Ready(r) = r else {
            panic!("{r:?}")
        };
        assert_eq!(r.node, "h-0123456789ab");
        assert_eq!(r.listen, "0.0.0.0:6113".parse().unwrap());
        assert_eq!(r.cert_sha256, parse_hex32(CERT).unwrap());
        assert_eq!(r.connect_addr(), "127.0.0.1:6113".parse().unwrap());

        assert_eq!(
            parse_node_line("reach state=mapped method=upnp addr=203.0.113.7:6113 nat=cone")
                .unwrap(),
            NodeLine::Reach(ReachReport {
                state: ReachState::Mapped,
                method: PortMethod::Upnp,
                addr: Some("203.0.113.7:6113".parse().unwrap()),
                nat: NatKind::Cone,
            })
        );
        assert_eq!(
            parse_node_line("game id=7 players=3").unwrap(),
            NodeLine::Game(GameInfo { id: 7, players: 3 })
        );
        assert_eq!(
            parse_node_line("closed reason=sleep").unwrap(),
            NodeLine::Closed {
                reason: "sleep".into()
            }
        );
        let e = parse_node_line("error code=100 text=host token unknown or ended").unwrap();
        assert_eq!(
            e,
            NodeLine::Error(NodeError {
                code: "100".into(),
                text: "host token unknown or ended".into()
            })
        );
        let NodeLine::Error(e) = e else {
            unreachable!()
        };
        assert_eq!(e.number(), Some(100));
        let NodeLine::Error(e) = parse_node_line("error code=link text=closed_by_peer").unwrap()
        else {
            panic!()
        };
        assert_eq!((e.number(), e.text.as_str()), (None, "closed_by_peer"));
    }

    #[test]
    fn broken_and_unknown_lines_are_refused() {
        assert_eq!(parse_node_line("  "), Err(LineError::Empty));
        assert_eq!(
            parse_node_line("hello x=1"),
            Err(LineError::Unknown("hello".into()))
        );
        assert!(matches!(
            parse_node_line("ready node=h-1 listen=nowhere"),
            Err(LineError::Bad(_))
        ));
        assert!(matches!(
            parse_node_line("ready node=h-1 listen=127.0.0.1:1 cert_sha256=abc"),
            Err(LineError::Bad(_))
        ));
        assert!(matches!(
            parse_node_line("game id=x players=1"),
            Err(LineError::Bad(_))
        ));
        assert!(matches!(parse_node_line("closed"), Err(LineError::Bad(_))));
        let long = format!("closed reason={}", "x".repeat(MAX_LINE));
        assert_eq!(parse_node_line(&long), Err(LineError::TooLong));
    }

    #[test]
    fn hex_round_trips() {
        let b = parse_hex32(CERT).unwrap();
        assert_eq!(hex(&b), CERT.to_lowercase());
        assert_eq!(parse_hex32(&CERT[1..]), None);
        assert_eq!(parse_hex32(&format!("{}g", &CERT[1..])), None);
    }
}
