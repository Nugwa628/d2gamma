//! The handshake of an online session (wire version [`NET_VERSION`] 6)
//! and the first frame of a game stream.
//!
//! An online connection carries two streams (contracts section 3.1):
//!
//! - **Control stream**, opened first by the client. The client says
//!   [`ClientHello`]; the server answers [`SessionWelcome`] or
//!   [`Refused`] and then closes. After the welcome the stream carries the
//!   account, character and lobby messages of [`crate::msg`].
//! - **Game stream**, opened by the client after the lobby's `JoinNode`,
//!   on its own connection to the game server (or after the dedicated
//!   server's `JoinOk`, on the same connection). Its first frame is
//!   [`Attach`] (the join ticket, or the game's resume token after a lost
//!   connection); the server answers with the game's [`Welcome`] or
//!   [`Refused`]. After the welcome it carries game packets
//!   ([`super::packet_body`]); a [`Refused`] there ends the game for the
//!   client with its code (`NetError::Refused`), for example
//!   [`ErrorCode::CHAR_MISMATCH`] for a join with another character.
//!
//! The server checks a hello in a fixed order (protocol, game, build, tick
//! rate) and refuses the first mismatch with its [`ErrorCode`] and a text
//! the player can act on. A body that doesn't decode is refused with
//! [`ErrorCode::WIRE_VERSION`] for a hello (an older or foreign client),
//! and [`ErrorCode::MALFORMED`] anywhere else.

use super::{kind, ResumeToken, Welcome, MAGIC, MAX_BUILD, NET_VERSION};
use crate::msg::{ErrorCode, Reader, Writer, MAX_ERROR_TEXT};
use crate::NetError;

/// Longest game id (`d2rust`, or a mod game's id).
pub const MAX_GAME_ID: usize = 32;

/// Longest server name in a [`SessionWelcome`].
pub const MAX_SERVER_NAME: usize = 64;

/// Longest account or character name in a [`ResumeState`]. The names'
/// own rules are shorter; this only bounds the decode.
pub const MAX_NAME: usize = 32;

/// C→S, the first frame of the control stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientHello {
    /// `d2_proto::PROTOCOL_VERSION` of the client.
    pub protocol: u32,
    /// The game the client plays: `d2rust`, or a mod game's id.
    pub game: String,
    /// The client's four-part game version (for example "0.1.2.0").
    pub build: String,
    /// The tick rate the client was built for, in ticks per second.
    pub tick_rate: u32,
    /// The session token of a lost connection, to take that session back.
    pub resume: Option<ResumeToken>,
}

/// What the server runs: what a [`ClientHello`] must match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    /// `d2_proto::PROTOCOL_VERSION` of the server.
    pub protocol: u32,
    /// The game the server runs.
    pub game: String,
    /// The server's game version.
    pub build: String,
    /// The server's tick rate.
    pub tick_rate: u32,
}

/// S→C on the control stream: the session is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionWelcome {
    /// The server's tick rate, in ticks per second.
    pub tick_rate: u32,
    /// The most players one game takes.
    pub max_players: u32,
    /// The session's token: what the client presents to take the session
    /// back after a lost connection. A secret.
    pub session: ResumeToken,
    /// The server's name, for the login screen.
    pub server_name: String,
    /// Whether the server takes new accounts.
    pub registration_open: bool,
    /// Present only when the hello's `resume` named a live session.
    pub resume: Option<ResumeState>,
}

/// The state of a session taken back after a lost connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResumeState {
    /// The logged-in account, as stored.
    pub account: String,
    /// The selected character, if any.
    pub character: Option<String>,
    /// A game slot still held for the session: the client attaches to it
    /// again with [`Attach::Resume`].
    pub game_id: Option<u32>,
}

/// S→C on either stream: why the server turned the client (or its
/// attach) away. The server closes the stream after it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{text} [{code}]")]
pub struct Refused {
    pub code: ErrorCode,
    /// English words for the player; the client shows its own for a code
    /// it knows.
    pub text: String,
}

impl Refused {
    /// A refusal with the code's own words.
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            text: code.text().to_owned(),
        }
    }

    /// A refusal with words of its own.
    pub fn with_text(code: ErrorCode, text: impl Into<String>) -> Self {
        Self {
            code,
            text: text.into(),
        }
    }

    /// The frame body (kind [`kind::REFUSED`]).
    pub fn body(&self) -> Vec<u8> {
        Writer::new(kind::REFUSED)
            .u16(self.code.0)
            .str8(&self.text)
            .finish()
    }

    /// Reads a refusal body (kind byte included).
    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut r) = Reader::new(body)?;
        if k != kind::REFUSED {
            return Err(NetError::Malformed);
        }
        let code = ErrorCode(r.u16()?);
        let text = r.str8(MAX_ERROR_TEXT)?;
        r.finish()?;
        Ok(Self { code, text })
    }
}

impl From<NetError> for Refused {
    /// A body that didn't decode.
    fn from(_: NetError) -> Self {
        Self::new(ErrorCode::MALFORMED)
    }
}

impl ClientHello {
    /// The frame body (kind [`kind::HELLO`]).
    pub fn body(&self) -> Vec<u8> {
        Writer::new(kind::HELLO)
            .array(&MAGIC)
            .u16(NET_VERSION)
            .u32(self.protocol)
            .str8(&self.game)
            .str8(&self.build)
            .u32(self.tick_rate)
            .opt(self.resume.as_ref(), |w, t| {
                w.array(&t.0);
            })
            .finish()
    }

    /// Reads a hello. Anything that isn't a whole hello of this wire
    /// version is [`ErrorCode::WIRE_VERSION`]: an older client, or not a
    /// D2RUST client at all.
    pub fn parse(body: &[u8]) -> Result<Self, Refused> {
        let wire = |_| Refused::new(ErrorCode::WIRE_VERSION);
        let (k, mut r) = Reader::new(body).map_err(wire)?;
        if k != kind::HELLO
            || r.array::<4>().map_err(wire)? != MAGIC
            || r.u16().map_err(wire)? != NET_VERSION
        {
            return Err(Refused::new(ErrorCode::WIRE_VERSION));
        }
        let read = |r: &mut Reader| -> Result<Self, NetError> {
            Ok(Self {
                protocol: r.u32()?,
                game: r.str8(MAX_GAME_ID)?,
                build: r.str8(MAX_BUILD)?,
                tick_rate: r.u32()?,
                resume: r.opt(|r| r.array().map(ResumeToken))?,
            })
        };
        let hello = read(&mut r).map_err(wire)?;
        r.finish().map_err(wire)?;
        Ok(hello)
    }

    /// The server's check, in the contract's order: protocol, game, build,
    /// tick rate. The resume token is not compared here.
    pub fn check(&self, server: &ServerInfo) -> Result<(), Refused> {
        if self.protocol != server.protocol {
            return Err(Refused::new(ErrorCode::PROTOCOL));
        }
        if self.game != server.game {
            return Err(Refused::with_text(
                ErrorCode::WRONG_GAME,
                format!(
                    "This server runs {}, not {}.",
                    server.game,
                    if self.game.is_empty() {
                        "this game"
                    } else {
                        &self.game
                    }
                ),
            ));
        }
        if self.build != server.build {
            return Err(Refused::with_text(
                ErrorCode::BUILD,
                format!(
                    "The server runs version {}; you have {}. Update through the launcher.",
                    server.build, self.build
                ),
            ));
        }
        if self.tick_rate != server.tick_rate {
            return Err(Refused::new(ErrorCode::TICK_RATE));
        }
        Ok(())
    }
}

impl SessionWelcome {
    /// The frame body (kind [`kind::WELCOME`]).
    pub fn body(&self) -> Vec<u8> {
        Writer::new(kind::WELCOME)
            .u32(self.tick_rate)
            .u32(self.max_players)
            .array(&self.session.0)
            .str8(&self.server_name)
            .bool(self.registration_open)
            .opt(self.resume.as_ref(), |w, s| {
                w.str8(&s.account)
                    .opt(s.character.as_ref(), |w, c| {
                        w.str8(c);
                    })
                    .opt(s.game_id.as_ref(), |w, g| {
                        w.u32(*g);
                    });
            })
            .finish()
    }

    /// Reads a welcome body (kind byte included).
    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut r) = Reader::new(body)?;
        if k != kind::WELCOME {
            return Err(NetError::Malformed);
        }
        let w = Self {
            tick_rate: r.u32()?,
            max_players: r.u32()?,
            session: ResumeToken(r.array()?),
            server_name: r.str8(MAX_SERVER_NAME)?,
            registration_open: r.bool()?,
            resume: r.opt(|r| {
                Ok(ResumeState {
                    account: r.str8(MAX_NAME)?,
                    character: r.opt(|r| r.str8(MAX_NAME))?,
                    game_id: r.opt(Reader::u32)?,
                })
            })?,
        };
        r.finish()?;
        Ok(w)
    }
}

/// The client's reading of the server's answer to its [`ClientHello`].
pub fn parse_session_answer(body: &[u8]) -> Result<SessionWelcome, Refused> {
    match body.first() {
        Some(&kind::WELCOME) => Ok(SessionWelcome::parse(body)?),
        Some(&kind::REFUSED) => Err(Refused::parse(body)?),
        _ => Err(Refused::new(ErrorCode::MALFORMED)),
    }
}

/// Longest ticket an [`Attach::Node`] carries.
pub const MAX_ATTACH_TICKET: usize = 255;

/// C→S, the first frame of a game stream.
///
/// `Debug` never prints a ticket or a token.
#[derive(Clone, PartialEq, Eq)]
pub enum Attach {
    /// Mode 0: enter the game a `JoinOk` gave this ticket for (the
    /// one-process server's join, which no server sends since plan N5). A
    /// game server refuses it.
    Ticket([u8; 16]),
    /// Mode 1: come back to a game slot held after a lost connection, with
    /// the game's resume token from its [`Welcome`].
    Resume(ResumeToken),
    /// Mode 2: take the seat the main server's `JoinNode` gave this ticket
    /// for, on that game server.
    Node(Vec<u8>),
}

impl std::fmt::Debug for Attach {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Attach::Ticket(_) => "Ticket",
            Attach::Resume(_) => "Resume",
            Attach::Node(_) => "Node",
        };
        f.debug_tuple(name).field(&"<redacted>").finish()
    }
}

impl Attach {
    /// The frame body (kind [`kind::ATTACH`]).
    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(kind::ATTACH);
        match self {
            Attach::Ticket(t) => w.u8(0).array(t),
            Attach::Resume(t) => w.u8(1).array(&t.0),
            Attach::Node(t) => w.u8(2).bytes8(t),
        };
        w.finish()
    }

    /// Reads an attach body (kind byte included).
    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut r) = Reader::new(body)?;
        if k != kind::ATTACH {
            return Err(NetError::Malformed);
        }
        let a = match r.u8()? {
            0 => Attach::Ticket(r.array()?),
            1 => Attach::Resume(ResumeToken(r.array()?)),
            2 => Attach::Node(r.bytes8(MAX_ATTACH_TICKET)?),
            _ => return Err(NetError::Malformed),
        };
        r.finish()?;
        Ok(a)
    }
}

/// The client's reading of the server's answer to its [`Attach`]: the
/// game's [`Welcome`] (wire v2 layout) or a refusal.
pub fn parse_attach_answer(body: &[u8]) -> Result<Welcome, Refused> {
    match body.split_first() {
        Some((&kind::WELCOME, rest)) => {
            Welcome::parse_fields(rest).ok_or_else(|| Refused::new(ErrorCode::MALFORMED))
        }
        Some((&kind::REFUSED, _)) => Err(Refused::parse(body)?),
        _ => Err(Refused::new(ErrorCode::MALFORMED)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello() -> ClientHello {
        ClientHello {
            protocol: 2,
            game: "d2rust".into(),
            build: "0.1.2.0".into(),
            tick_rate: 60,
            resume: None,
        }
    }

    fn server() -> ServerInfo {
        ServerInfo {
            protocol: 2,
            game: "d2rust".into(),
            build: "0.1.2.0".into(),
            tick_rate: 60,
        }
    }

    #[test]
    fn hello_round_trips() {
        let h = hello();
        assert_eq!(ClientHello::parse(&h.body()), Ok(h.clone()));
        let back = ClientHello {
            resume: Some(ResumeToken([7; 16])),
            ..hello()
        };
        assert_eq!(ClientHello::parse(&back.body()), Ok(back));
    }

    #[test]
    fn a_hello_of_another_wire_version_or_shape_is_wire_version() {
        let wire = Err(Refused::new(ErrorCode::WIRE_VERSION));
        let mut old = hello().body();
        old[5..7].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(ClientHello::parse(&old), wire);
        let mut magic = hello().body();
        magic[1] ^= 0xFF;
        assert_eq!(ClientHello::parse(&magic), wire);
        let mut cut = hello().body();
        cut.pop();
        assert_eq!(ClientHello::parse(&cut), wire);
        let mut long = hello().body();
        long.push(0);
        assert_eq!(ClientHello::parse(&long), wire);
        assert_eq!(ClientHello::parse(&[]), wire);
        // A wire-v2 hello (the refreshed line's) is refused the same way.
        let v2 = super::super::Hello {
            protocol: 2,
            build: "0.1.2.0".into(),
            tick_rate: 60,
            resume: None,
        };
        let mut v2 = v2.body();
        v2[5..7].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(ClientHello::parse(&v2), wire);
        // Control characters in a string are not a hello.
        let bad = ClientHello {
            game: "d2\nrust".into(),
            ..hello()
        };
        assert_eq!(ClientHello::parse(&bad.body()), wire);
    }

    #[test]
    fn check_refuses_the_first_mismatch_in_order() {
        assert_eq!(hello().check(&server()), Ok(()));
        let all_wrong = ClientHello {
            protocol: 1,
            game: "mod-x".into(),
            build: "0.0.9.0".into(),
            tick_rate: 25,
            resume: None,
        };
        assert_eq!(
            all_wrong.check(&server()).unwrap_err().code,
            ErrorCode::PROTOCOL
        );
        let game = ClientHello {
            protocol: 2,
            ..all_wrong.clone()
        };
        let r = game.check(&server()).unwrap_err();
        assert_eq!(r.code, ErrorCode::WRONG_GAME);
        assert_eq!(r.text, "This server runs d2rust, not mod-x.");
        let build = ClientHello {
            game: "d2rust".into(),
            ..game
        };
        let r = build.check(&server()).unwrap_err();
        assert_eq!(r.code, ErrorCode::BUILD);
        assert_eq!(
            r.text,
            "The server runs version 0.1.2.0; you have 0.0.9.0. Update through the launcher."
        );
        let tick = ClientHello {
            build: "0.1.2.0".into(),
            ..build
        };
        assert_eq!(
            tick.check(&server()).unwrap_err().code,
            ErrorCode::TICK_RATE
        );
    }

    #[test]
    fn welcome_and_refusal_round_trip() {
        let mut w = SessionWelcome {
            tick_rate: 60,
            max_players: 8,
            session: ResumeToken([0xA5; 16]),
            server_name: "D2RUST Europe".into(),
            registration_open: true,
            resume: None,
        };
        assert_eq!(parse_session_answer(&w.body()), Ok(w.clone()));
        for resume in [
            ResumeState {
                account: "Owner".into(),
                character: None,
                game_id: None,
            },
            ResumeState {
                account: "Owner".into(),
                character: Some("Kashya".into()),
                game_id: Some(3),
            },
        ] {
            w.resume = Some(resume);
            assert_eq!(parse_session_answer(&w.body()), Ok(w.clone()));
        }
        let mut cut = w.body();
        cut.pop();
        assert_eq!(
            parse_session_answer(&cut).unwrap_err().code,
            ErrorCode::MALFORMED
        );
        let r = Refused::with_text(ErrorCode::SERVER_FULL, "full");
        assert_eq!(parse_session_answer(&r.body()), Err(r.clone()));
        assert_eq!(parse_attach_answer(&r.body()), Err(r));
        assert_eq!(
            parse_session_answer(&[kind::PING]).unwrap_err().code,
            ErrorCode::MALFORMED
        );
    }

    #[test]
    fn attach_round_trips_and_its_answer_is_the_game_welcome() {
        for a in [
            Attach::Ticket([3; 16]),
            Attach::Resume(ResumeToken([4; 16])),
        ] {
            assert_eq!(Attach::parse(&a.body()), Ok(a));
        }
        let mut bad = Attach::Ticket([3; 16]).body();
        bad[1] = 3;
        assert_eq!(Attach::parse(&bad), Err(NetError::Malformed));
        let mut short = Attach::Ticket([3; 16]).body();
        short.pop();
        assert_eq!(Attach::parse(&short), Err(NetError::Malformed));
        let w = Welcome {
            client: 2,
            tick_rate: 60,
            max_players: 8,
            resume: ResumeToken([9; 16]),
        };
        assert_eq!(parse_attach_answer(&w.body()), Ok(w));
    }

    /// Mode 2 (contracts 11.4): `u8 mode = 2`, `bytes8 ticket`.
    #[test]
    fn attach_mode_2_carries_a_game_servers_ticket() {
        let a = Attach::Node(vec![7, 8, 9]);
        assert_eq!(a.body(), [kind::ATTACH, 2, 3, 7, 8, 9]);
        assert_eq!(Attach::parse(&a.body()), Ok(a));
        let big = Attach::Node(vec![1; MAX_ATTACH_TICKET]);
        assert_eq!(Attach::parse(&big.body()), Ok(big));
        let mut cut = Attach::Node(vec![7, 8, 9]).body();
        cut.pop();
        assert_eq!(Attach::parse(&cut), Err(NetError::Malformed));
        let text = format!("{:?}", Attach::Node(vec![0xAB; 20]));
        assert!(!text.contains("171"), "{text}");
        assert_eq!(text, "Node(\"<redacted>\")");
        let text = format!("{:?}", Attach::Resume(ResumeToken([0xAB; 16])));
        assert!(!text.contains("171"), "{text}");
    }

    /// A client of wire version 4 (before the game servers) or 5 (before
    /// the realm lobby) is told it is too old: `WireVersion`.
    #[test]
    fn an_older_hello_is_wire_version() {
        assert_eq!(NET_VERSION, 6);
        for old in [4u16, 5] {
            let mut body = hello().body();
            body[5..7].copy_from_slice(&old.to_le_bytes());
            assert_eq!(
                ClientHello::parse(&body),
                Err(Refused::new(ErrorCode::WIRE_VERSION)),
                "v{old}"
            );
        }
    }
}
