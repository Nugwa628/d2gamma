//! A bot: one headless client. It connects over QUIC through its own
//! relay ([`crate::net::Relay`]), sends what a player's client sends, and
//! keeps a small view of the game from what the server delivers (the
//! players it was told about and where they stand). It never draws and
//! never runs the game's rules: what it knows, it knows from the server.
//!
//! It speaks wire version 4 (contracts 3.1): the session's handshake on
//! the control stream ([`ClientHello`], [`SessionWelcome`], [`Refused`]),
//! then the account and lobby messages on that stream and the game stream
//! a game is entered on, which starts with an [`Attach`] (the lobby's
//! ticket, or the game's resume token) and carries the game's packets
//! after the server's welcome.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use d2_net::msg::account::{AccountEvent, AccountRequest, CharSummary};
use d2_net::msg::lobby::{self, GameInfo, JoinNode, LobbyEvent, LobbyRequest};
use d2_net::msg::stash::{self as stash_msg, goods, StashEvent, StashRequest};
use d2_net::msg::{kind as msg_kind, ErrorCode, ErrorMsg};
use d2_net::wire::{
    parse_attach_answer, parse_session_answer, Attach, ClientHello, Refused, SessionWelcome,
    Timing, Welcome,
};
use d2_net::{ClientId, Lane, NetError, Packet, ResumeToken, Transport};
use d2_proto::s2c::{self, Message, ParseError};
use d2_quic::{fingerprint, FramedStream, PacketStream, QuicClient, ServerTrust};

use crate::net::Relay;
use crate::play::Play;

mod chat;
mod friends;
mod guild;
mod ladder;
mod market;

/// Keepalive and timeout of every bot connection: short, so a cut link
/// is noticed within a scenario's wait.
pub const BOT_TIMING: Timing = Timing {
    keepalive: Duration::from_millis(500),
    timeout: Duration::from_secs(3),
};

/// How long a game stream may take to answer its attach: the game loads
/// the character (and its save) before it welcomes the player.
const ATTACH_WAIT: Duration = Duration::from_secs(10);

/// How often a driver reads a stream while it waits.
const POLL: Duration = Duration::from_millis(2);

/// How a step went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// It worked.
    Ok,
    /// The server said no: a stable code and its words.
    Refused {
        /// The contracts' `ErrorCode` name (`Build`, `WrongLogin`, ...),
        /// or `no-player` when a game gave no player and said nothing.
        code: String,
        /// What the server said.
        text: String,
    },
    /// It failed for another reason.
    Error(String),
    /// The bot's connection is closed.
    Closed(String),
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Ok => write!(f, "ok"),
            Status::Refused { code, text } => write!(f, "refused={code} ({text})"),
            Status::Error(e) => write!(f, "error ({e})"),
            Status::Closed(why) => write!(f, "closed ({why})"),
        }
    }
}

/// A step's result: its status and the facts it reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// How it went.
    pub status: Status,
    /// What it found (`players=2`, `client=3`, ...).
    pub facts: BTreeMap<String, String>,
}

impl Outcome {
    /// A plain success.
    pub fn ok() -> Self {
        Self {
            status: Status::Ok,
            facts: BTreeMap::new(),
        }
    }

    /// A failure that isn't a refusal.
    pub fn error(e: impl Into<String>) -> Self {
        Self {
            status: Status::Error(e.into()),
            facts: BTreeMap::new(),
        }
    }

    /// Adds a fact.
    pub fn fact(mut self, k: impl Into<String>, v: impl ToString) -> Self {
        self.facts.insert(k.into(), v.to_string());
        self
    }
}

/// A server's refusal as the step's status: its `ErrorCode` name in the
/// multiplayer contracts (section 3.8), so scenarios read the same for
/// every message.
fn refused(r: &Refused) -> Status {
    Status::Refused {
        code: r.code.name().into(),
        text: r.text.clone(),
    }
}

/// A server's answer to a request that failed.
fn error_status(e: &ErrorMsg) -> Status {
    Status::Refused {
        code: e.code.name().into(),
        text: e.text.clone(),
    }
}

/// Why a request failed.
enum Failed {
    /// The server answered `ERROR`.
    Server(ErrorMsg),
    /// The request couldn't be made or answered.
    Local(Status),
}

impl Failed {
    /// The step's outcome, with how long the server took.
    fn outcome(self, ms: u128) -> Outcome {
        match self {
            Failed::Server(e) => Outcome {
                status: error_status(&e),
                facts: BTreeMap::from([
                    ("ms".into(), ms.to_string()),
                    ("wait_s".into(), e.wait_s.to_string()),
                ]),
            },
            Failed::Local(status) => Outcome {
                status,
                facts: BTreeMap::new(),
            },
        }
    }
}

impl From<Status> for Failed {
    fn from(s: Status) -> Self {
        Failed::Local(s)
    }
}

/// The certificate a server named in its ready line, for pinning.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerCert {
    /// The DER certificate itself, when the server wrote it to a file.
    pub der: Option<Vec<u8>>,
    /// Its SHA-256, when the server printed `cert_sha256=`.
    pub sha256: Option<[u8; 32]>,
}

impl ServerCert {
    /// The fingerprint a bot pins.
    pub fn pin(&self) -> Option<[u8; 32]> {
        self.sha256.or_else(|| self.der.as_deref().map(fingerprint))
    }
}

/// A player a bot was told about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seen {
    /// The character name.
    pub name: String,
    /// The class.
    pub class: u8,
    /// Where it stands (or walks to), as last told.
    pub x: u16,
    /// Where it stands (or walks to), as last told.
    pub y: u16,
    /// Where it was first seen.
    pub first: (u16, u16),
}

/// What a bot knows about the game.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// The players it was told about, by unit id.
    pub players: BTreeMap<u32, Seen>,
    /// Its own player, once assigned.
    pub own: Option<u32>,
    /// Frame marks received, and how many had a tick.
    pub frames: u64,
    /// Frame marks with a tick.
    pub ticks: u64,
    /// Messages delivered.
    pub delivered: u64,
    /// Delivered messages that broke their size rule or layout.
    pub bad: Vec<String>,
    /// Delivered messages by id.
    pub ids: BTreeMap<u8, u64>,
    /// Mod extension messages received.
    pub ext: u64,
    /// The game answered the join (`0x02`): its 0x6B may go now.
    pub loaded: bool,
    /// The game ended the session's play (`0x06`), as a leave does.
    pub exiting: bool,
    /// Monsters, items, objects, act and area.
    pub play: Play,
    /// The server's own lines in the game (chat `0x26` with no speaker).
    pub server_lines: Vec<String>,
}

impl View {
    /// Takes in one delivered message.
    fn apply(&mut self, msg: &[u8], own_name: &str) {
        self.delivered += 1;
        if let Some(&id) = msg.first() {
            *self.ids.entry(id).or_default() += 1;
        }
        if let Some(text) = server_line(msg) {
            self.server_lines.push(text);
        }
        let parsed = s2c::parse(msg);
        self.play.apply(msg, parsed.as_ref().ok(), self.own);
        match parsed {
            Ok(Message::AssignPlayer(a)) => {
                let name = cstr(&a.name);
                if name == own_name {
                    self.own = Some(a.guid);
                }
                self.players.insert(
                    a.guid,
                    Seen {
                        name,
                        class: a.class,
                        x: a.x,
                        y: a.y,
                        first: (a.x, a.y),
                    },
                );
            }
            Ok(Message::PlayerMove(m)) if m.type_ == 0 => {
                if let Some(p) = self.players.get_mut(&m.guid) {
                    (p.x, p.y) = (m.target_x, m.target_y);
                }
            }
            Ok(Message::ReassignPlayer(m)) if m.type_ == 0 => {
                if let Some(p) = self.players.get_mut(&m.guid) {
                    (p.x, p.y) = (m.x, m.y);
                }
            }
            Ok(Message::PlayerLeft(m)) => {
                self.players.remove(&m.guid);
            }
            Ok(Message::RemoveUnit(m)) if m.type_ == 0 => {
                self.players.remove(&m.guid);
            }
            Ok(Message::LoadSuccessful(_)) => self.loaded = true,
            Ok(Message::GameExit(_)) => self.exiting = true,
            Ok(_) | Err(ParseError::Unbuilt { .. }) => {}
            Err(e) => self.bad.push(e.to_string()),
        }
    }

    /// The player named `name`, if known.
    pub fn named(&self, name: &str) -> Option<&Seen> {
        self.players.values().find(|p| p.name == name)
    }
}

/// The text of a chat line (`0x26`) with no speaker: the server's own,
/// shown as a system line in the game.
fn server_line(msg: &[u8]) -> Option<String> {
    // 0x26, type, language, unknown, u32, colour, level, name NUL, text NUL.
    let rest = msg.strip_prefix(&[0x26])?.get(9..)?;
    let (&first, text) = rest.split_first()?;
    if first != 0 {
        return None;
    }
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    Some(String::from_utf8_lossy(&text[..end]).into_owned())
}

/// A NUL-padded name as text.
fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

/// What a bot says in its hello unless a step says otherwise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelloDefaults {
    /// `d2-proto` protocol version.
    pub protocol: u32,
    /// The game the server runs (`d2rust`).
    pub game: String,
    /// Game build.
    pub build: String,
    /// Tick rate.
    pub tick_rate: u32,
}

/// Why opening the session's handshake failed.
enum Open {
    /// The server refused the hello.
    Refused(Refused),
    /// The connection or its answer failed.
    Error(String),
}

/// A bot's open connection: the QUIC connection, its control stream
/// (the session handshake, then account and lobby messages) and the game
/// stream once a game is entered. A game on a game server (`JoinNode`)
/// has a connection of its own there, from the same socket.
#[derive(Debug)]
struct Conn {
    /// Dropped after the streams: closing it ends the connection.
    client: QuicClient,
    /// The session's control stream.
    control: FramedStream,
    /// Bodies that arrived on the control stream and no driver has read.
    inbox: VecDeque<Vec<u8>>,
    /// The game stream, once attached.
    game: Option<PacketStream>,
    /// The connection to the game server the game stream is on, if any.
    node: Option<QuicClient>,
}

/// The game server a game joined through `JoinNode` runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NodeAt {
    addr: std::net::SocketAddr,
    cert: [u8; 32],
}

impl Conn {
    /// Connects, sends `hello` on the control stream and waits (at most
    /// the timing's timeout) for the session's answer.
    fn open(
        addr: std::net::SocketAddr,
        hello: &ClientHello,
        trust: ServerTrust,
        timing: Timing,
    ) -> Result<(Self, SessionWelcome), Open> {
        let (client, mut control) =
            QuicClient::connect(addr, trust, timing).map_err(|e| Open::Error(e.to_string()))?;
        let answer = control
            .send(hello.body())
            .and_then(|()| control.recv_timeout(timing.timeout))
            .map_err(|e| Open::Error(format!("handshake: {e}")))?;
        let Some(body) = answer else {
            return Err(Open::Error("the server did not answer in time".into()));
        };
        let welcome = parse_session_answer(&body).map_err(Open::Refused)?;
        Ok((
            Self {
                client,
                control,
                inbox: VecDeque::new(),
                game: None,
                node: None,
            },
            welcome,
        ))
    }

    /// Ends the connection with a goodbye on both streams.
    fn close(mut self) {
        if let Some(game) = self.game.as_mut() {
            game.close();
        }
        self.leave_node();
        self.control.close();
        self.client.close();
    }

    /// Closes the connection to the game server, if there is one.
    fn leave_node(&mut self) {
        if let Some(mut node) = self.node.take() {
            node.close();
        }
    }
}

/// One bot client.
#[derive(Debug)]
pub struct Bot {
    /// Its index in the scenario (`b0`, `b1`, ...).
    pub index: usize,
    /// Its relay to the server.
    pub relay: Relay,
    conn: Option<Conn>,
    /// Why its last connection ended, once it has.
    pub closed: Option<String>,
    /// What it knows.
    pub view: View,
    /// It was in a game (sent 0x67) since it entered.
    pub entered: bool,
    /// The session token to come back with after a lost connection.
    session: Option<ResumeToken>,
    /// The game's resume token, while it holds a game slot.
    game_token: Option<ResumeToken>,
    /// The game's client id, as last welcomed.
    game_client: Option<ClientId>,
    /// The account it is logged in to, as stored.
    logged_in: Option<String>,
    /// The character the lobby plays with.
    selected: Option<CharSummary>,
    /// Character names this bot has seen or asked about.
    known_chars: BTreeSet<String>,
    /// The lobby's games, as last listed or updated.
    games: BTreeMap<u32, GameInfo>,
    /// Game names this bot has seen or asked about.
    known_games: BTreeSet<String>,
    /// The main server's last game-server list (`NodeList`), and how many
    /// lists came.
    nodes: Vec<d2_net::msg::nodelist::NodeInfo>,
    node_lists: u32,
    /// Every game server a list named (`nodes` says `has.ID=no` for those
    /// gone since).
    known_nodes: BTreeSet<String>,
    /// Complete game lists received.
    lists: u64,
    /// The lobby is sending this session the game list.
    listening: bool,
    /// The game the session is in, as the lobby said.
    game_id: Option<u32>,
    /// Its difficulty, as the lobby gave it.
    game_difficulty: Option<u8>,
    /// The game server its game runs on, for a game joined through
    /// `JoinNode` (a resume goes there).
    game_node: Option<NodeAt>,
    /// The last `JoinNode` the main server sent it (a test may present
    /// its ticket again).
    last_join: Option<JoinNode>,
    /// The next create or join stops at the main server's answer: no
    /// attach, no enter (`enter=no`).
    hold_join: bool,
    /// The next enter names this character in its 0x67 join instead of
    /// its own (`as=NAME`).
    enter_as: Option<String>,
    /// The code of the last `Refused` that ended its game stream.
    game_refused: Option<ErrorCode>,
    /// The last chat line heard: who and what.
    last_chat: Option<(String, String)>,
    /// The last session-level error the server sent.
    session_error: Option<ErrorMsg>,
    /// The server took this session over: its close is the contract's.
    taken: bool,
    /// A close a driver caused that `pump` should report.
    close_event: Option<String>,
    /// The last stash request number it sent (rising per account; a new
    /// session may start again from 1).
    stash_req: u64,
    /// The market's request numbers, picked listing and pushes.
    market: market::Memo,
    /// The last ladder request number it sent.
    ladder_req: u32,
    /// The chat's request numbers and what it heard.
    chat: chat::Memo,
    /// The guild notices it got.
    guild: guild::Memo,
    /// The friends' notices it heard.
    friends: friends::Memo,
}

impl Bot {
    /// A bot with no connection.
    pub fn new(index: usize, relay: Relay) -> Self {
        Self {
            index,
            relay,
            conn: None,
            closed: None,
            view: View::default(),
            entered: false,
            session: None,
            game_token: None,
            game_client: None,
            logged_in: None,
            selected: None,
            known_chars: BTreeSet::new(),
            games: BTreeMap::new(),
            known_games: BTreeSet::new(),
            nodes: Vec::new(),
            node_lists: 0,
            known_nodes: BTreeSet::new(),
            lists: 0,
            listening: false,
            game_id: None,
            game_difficulty: None,
            game_node: None,
            last_join: None,
            hold_join: false,
            enter_as: None,
            game_refused: None,
            last_chat: None,
            session_error: None,
            taken: false,
            close_event: None,
            stash_req: 0,
            market: market::Memo::default(),
            ladder_req: 0,
            chat: chat::Memo::default(),
            guild: guild::Memo::default(),
            friends: friends::Memo::default(),
        }
    }

    /// Its character name: `bot<index>`.
    pub fn char_name(&self) -> String {
        format!("bot{}", self.index)
    }

    /// The name it plays with: its selected character's, or its own.
    pub fn play_name(&self) -> String {
        self.selected
            .as_ref()
            .map_or_else(|| self.char_name(), |c| c.name.clone())
    }

    /// It has an open connection.
    pub fn connected(&self) -> bool {
        self.conn.is_some()
    }

    /// It holds a game stream.
    pub fn in_game(&self) -> bool {
        self.conn.as_ref().is_some_and(|conn| conn.game.is_some())
    }

    /// The server took its session over (a new login): its close is
    /// expected, not a broken connection.
    pub fn taken(&self) -> bool {
        self.taken
    }

    /// The account it is logged in to, if it is.
    pub fn account(&self) -> &str {
        self.logged_in.as_deref().unwrap_or("")
    }

    /// The last session-level error's code, if the server sent one.
    pub fn session_error(&self) -> &str {
        self.session_error.as_ref().map_or("", |e| e.code.name())
    }

    /// The game the lobby put it in, as text (`-` when none).
    pub fn game(&self) -> std::borrow::Cow<'_, str> {
        match self.game_id {
            Some(id) => std::borrow::Cow::Owned(id.to_string()),
            None => std::borrow::Cow::Borrowed("-"),
        }
    }

    /// Its game's difficulty, if the lobby gave it one.
    pub fn difficulty(&self) -> &'static str {
        self.game_difficulty.map_or("", difficulty_name)
    }

    /// Reads everything that has arrived. Returns why the connection
    /// ended, when it ended now.
    pub fn pump(&mut self) -> Option<String> {
        if let Some(why) = self.close_event.take() {
            return Some(why);
        }
        let name = self.play_name();
        let mut control_end = None;
        if let Some(conn) = self.conn.as_mut() {
            loop {
                match conn.control.recv() {
                    Ok(Some(b)) => conn.inbox.push_back(b),
                    Ok(None) => break,
                    Err(e) => {
                        control_end = Some(e.to_string());
                        break;
                    }
                }
            }
        }
        self.note_async_errors();
        if let Some(why) = control_end {
            let why = format!("the session's control stream ended: {why}");
            self.closed = Some(why.clone());
            self.conn = None;
            self.entered = false;
            return Some(why);
        }
        let mut game_end = None;
        if let Some(conn) = self.conn.as_mut() {
            if let Some(game) = conn.game.as_mut() {
                loop {
                    match game.recv() {
                        Ok(Some(Packet::Deliver(m))) => self.view.apply(&m, &name),
                        Ok(Some(Packet::Frame { ticked })) => {
                            self.view.frames += 1;
                            self.view.ticks += u64::from(ticked);
                        }
                        // Mod extension messages: D2RUST's own game sends none,
                        // and bots of a mod game don't read them (yet).
                        Ok(Some(Packet::Ext { .. })) => self.view.ext += 1,
                        Ok(Some(Packet::Send { .. })) => {
                            self.view
                                .bad
                                .push("the server sent a client-only packet".into());
                        }
                        Ok(None) => break,
                        Err(e) => {
                            game_end = Some(e);
                            break;
                        }
                    }
                }
            }
        }
        let e = game_end?;
        if let NetError::Refused(code) = e {
            self.game_refused = Some(code);
        }
        let mut on_node = false;
        if let Some(conn) = self.conn.as_mut() {
            conn.game = None;
            on_node = conn.node.is_some();
            conn.leave_node();
        }
        self.entered = false;
        // The game stream ends when a player leaves and the server closes
        // it; the session lives on. A lost connection ends both streams.
        // A game on a game server has a connection of its own: losing it
        // leaves the session to the main server.
        let realm_closed = self
            .conn
            .as_ref()
            .is_some_and(|conn| conn.client.closed_reason().is_some());
        let lost = match e {
            _ if on_node => realm_closed,
            NetError::TimedOut | NetError::Malformed => true,
            NetError::Refused(_) => false,
            NetError::Closed => realm_closed,
        };
        if lost {
            let why = e.to_string();
            self.conn = None;
            self.closed = Some(why.clone());
            return Some(why);
        }
        None
    }

    /// Reads the session-level errors already queued: a takeover is the
    /// server ending this session on purpose.
    fn note_async_errors(&mut self) {
        let Some(mut inbox) = self.conn.as_mut().map(|c| std::mem::take(&mut c.inbox)) else {
            return;
        };
        let mut kept = VecDeque::new();
        let mut errors = Vec::new();
        while let Some(b) = inbox.pop_front() {
            if b.first() == Some(&msg_kind::ERROR) {
                if let Ok(e) = ErrorMsg::parse(&b) {
                    errors.push(e);
                    continue;
                }
            }
            kept.push_back(b);
        }
        if let Some(conn) = self.conn.as_mut() {
            conn.inbox = kept;
        }
        for e in errors {
            self.note_session_error(e);
        }
    }

    fn note_session_error(&mut self, e: ErrorMsg) {
        if e.code == ErrorCode::SESSION_TAKEN {
            self.taken = true;
        }
        self.session_error = Some(e);
    }

    /// Marks the connection closed for `pump` to report once.
    fn close_now(&mut self, why: String) -> Status {
        self.closed = Some(why.clone());
        self.conn = None;
        self.entered = false;
        self.close_event = Some(why.clone());
        Status::Closed(why)
    }

    /// Connects (or comes back with its session token when `resume`).
    /// `args`: `build=`, `protocol=`, `tick=`, `game=` override the hello;
    /// `trust=pinned|any|wrong` picks the certificate check.
    pub fn connect(
        &mut self,
        args: &[(String, String)],
        defaults: &HelloDefaults,
        cert: &ServerCert,
        resume: bool,
    ) -> Outcome {
        if self.conn.is_some() {
            if !resume {
                return Outcome::error("already connected");
            }
            // The old connection is the one this token comes back for; a
            // server that restarted no longer holds it. Let it go and
            // present the token on a new one.
            let _ = self.pump();
            if let Some(conn) = self.conn.take() {
                conn.close();
            }
            self.closed = None;
            self.close_event = None;
        }
        let mut hello = ClientHello {
            protocol: defaults.protocol,
            game: defaults.game.clone(),
            build: defaults.build.clone(),
            tick_rate: defaults.tick_rate,
            resume: None,
        };
        let mut trust = match cert.pin() {
            Some(f) => ServerTrust::Pinned(f),
            None => ServerTrust::Any,
        };
        for (k, v) in args {
            let bad = || Outcome::error(format!("connect {k}={v}: not a valid value"));
            match k.as_str() {
                "build" => hello.build = v.clone(),
                "game" => hello.game = v.clone(),
                "protocol" => match v.parse() {
                    Ok(p) => hello.protocol = p,
                    Err(_) => return bad(),
                },
                "tick" => match v.parse() {
                    Ok(t) => hello.tick_rate = t,
                    Err(_) => return bad(),
                },
                "trust" => {
                    trust = match v.as_str() {
                        "any" => ServerTrust::Any,
                        "pinned" => match cert.pin() {
                            Some(f) => ServerTrust::Pinned(f),
                            None => return Outcome::error("the server named no certificate"),
                        },
                        // A certificate the server doesn't hold.
                        "wrong" => {
                            ServerTrust::Pinned(fingerprint(b"not the server's certificate"))
                        }
                        _ => return bad(),
                    }
                }
                _ => return Outcome::error(format!("connect: unknown setting {k}")),
            }
        }
        if resume {
            match self.session {
                Some(t) => hello.resume = Some(t),
                None => return Outcome::error("no earlier session to come back to"),
            }
        }
        let previous = self.game_client;
        match Conn::open(self.relay.addr(), &hello, trust, BOT_TIMING) {
            Ok((conn, welcome)) => {
                self.conn = Some(conn);
                self.closed = None;
                self.session_error = None;
                self.taken = false;
                self.close_event = None;
                self.session = Some(welcome.session);
                let mut o = Outcome::ok()
                    .fact("max_players", welcome.max_players)
                    .fact("tick", welcome.tick_rate)
                    .fact("session", "yes")
                    .fact("server", &welcome.server_name);
                if !resume {
                    self.view = View::default();
                    self.entered = false;
                    self.game_token = None;
                    self.game_client = None;
                    self.logged_in = None;
                    self.selected = None;
                    self.known_chars.clear();
                    self.games.clear();
                    self.known_games.clear();
                    self.lists = 0;
                    self.listening = false;
                    self.game_id = None;
                    self.game_difficulty = None;
                    self.game_node = None;
                    self.last_join = None;
                    self.last_chat = None;
                    self.social_reset();
                    return o;
                }
                // A held session comes back with its game; a token the
                // server doesn't know still tries the old game token, so
                // the server's refusal names the rule.
                let kept = welcome.resume.is_some();
                let held = welcome.resume.as_ref().is_some_and(|s| s.game_id.is_some());
                if let Some(token) = self.game_token.filter(|_| held || !kept) {
                    let back = match self.game_node {
                        Some(at) => self.attach_node(at, Attach::Resume(token)),
                        None => self.attach(Attach::Resume(token)),
                    };
                    match back {
                        Ok(game) => {
                            let same = previous == Some(game.client);
                            o = o
                                .fact("client", game.client)
                                .fact("same", if same { "yes" } else { "no" });
                        }
                        Err(status) => {
                            // The old game is gone: start over cleanly.
                            if let Some(conn) = self.conn.take() {
                                conn.close();
                            }
                            self.session = None;
                            self.game_token = None;
                            self.game_client = None;
                            self.game_node = None;
                            return Outcome {
                                status,
                                facts: BTreeMap::new(),
                            };
                        }
                    }
                }
                if !kept {
                    // Nothing was kept: a fresh session, with no account,
                    // character or game.
                    self.view = View::default();
                    self.entered = false;
                    self.game_token = None;
                    self.game_client = None;
                    self.logged_in = None;
                    self.selected = None;
                    self.known_chars.clear();
                    self.games.clear();
                    self.known_games.clear();
                    self.lists = 0;
                    self.listening = false;
                    self.game_id = None;
                    self.game_node = None;
                    self.last_chat = None;
                    self.social_reset();
                    o = o.fact("same", "no");
                }
                o
            }
            Err(Open::Refused(r)) => Outcome {
                status: refused(&r),
                facts: BTreeMap::new(),
            },
            Err(Open::Error(e)) => Outcome::error(e),
        }
    }

    /// Opens the game stream and attaches it with `mode`, then keeps it
    /// for the game's packets. `Err` is the refusal or failure.
    fn attach(&mut self, mode: Attach) -> Result<Welcome, Status> {
        let Some(conn) = self.conn.as_mut() else {
            return Err(Status::Closed(
                self.closed
                    .clone()
                    .unwrap_or_else(|| "not connected".into()),
            ));
        };
        if conn.game.is_some() {
            return Err(Status::Error("already in a game".into()));
        }
        let mut stream = conn
            .node
            .as_ref()
            .unwrap_or(&conn.client)
            .open_stream()
            .map_err(|e| Status::Error(format!("opening the game stream: {e}")))?;
        stream
            .send(mode.body())
            .map_err(|e| Status::Error(format!("attach: {e}")))?;
        let answer = stream
            .recv_timeout(ATTACH_WAIT)
            .map_err(|e| Status::Error(format!("attach: {e}")))?;
        let Some(body) = answer else {
            return Err(Status::Error(
                "the server did not answer the attach in time".into(),
            ));
        };
        match parse_attach_answer(&body) {
            Ok(welcome) => {
                conn.game = Some(PacketStream::client(stream));
                Ok(welcome)
            }
            Err(r) => {
                stream.close();
                Err(refused(&r))
            }
        }
    }

    /// Connects to the game server `at` from the session's socket (its
    /// certificate pinned) and attaches with `mode` on that connection's
    /// first stream (contracts 11.4). `Err` is the refusal or failure; the
    /// connection to the game server is closed then.
    fn attach_node(&mut self, at: NodeAt, mode: Attach) -> Result<Welcome, Status> {
        let Some(conn) = self.conn.as_mut() else {
            return Err(Status::Closed(
                self.closed
                    .clone()
                    .unwrap_or_else(|| "not connected".into()),
            ));
        };
        if conn.game.is_some() {
            return Err(Status::Error("already in a game".into()));
        }
        conn.leave_node();
        let (node, mut stream) = conn
            .client
            .connect_more(at.addr, ServerTrust::Pinned(at.cert))
            .map_err(|e| Status::Error(format!("connecting to the game server: {e}")))?;
        conn.node = Some(node);
        let answer = stream
            .send(mode.body())
            .and_then(|()| stream.recv_timeout(ATTACH_WAIT));
        let refusal = match answer {
            Ok(Some(body)) => match parse_attach_answer(&body) {
                Ok(welcome) => {
                    conn.game = Some(PacketStream::client(stream));
                    return Ok(welcome);
                }
                Err(r) => refused(&r),
            },
            Ok(None) => Status::Error("the game server did not answer the attach in time".into()),
            // A refusal may come as the connection's close.
            Err(e) => Status::Error(format!("attach: {e}")),
        };
        stream.close();
        conn.leave_node();
        Err(refusal)
    }

    /// Sets how the next create or join goes: `hold` stops at the main
    /// server's answer; `enter_as` names another character in the 0x67
    /// join.
    pub fn set_join(&mut self, hold: bool, enter_as: Option<&str>) {
        self.hold_join = hold;
        self.enter_as = enter_as.map(str::to_owned);
    }

    /// `ticket NODE`: shows the last `JoinNode`'s ticket to the game server
    /// at `addr` (`Attach` mode 2 on a connection of its own from the
    /// session's socket, its certificate not checked) and reports the
    /// answer; the connection is closed after it. Facts `client` when it
    /// was taken.
    pub fn present_ticket(&mut self, addr: std::net::SocketAddr) -> Outcome {
        let Some(ticket) = self.last_join.as_ref().map(|j| j.ticket.clone()) else {
            return Outcome::error("no JoinNode yet (create or join a game first)");
        };
        let Some(conn) = self.conn.as_ref() else {
            return Outcome::error("not connected");
        };
        let (mut node, mut stream) = match conn.client.connect_more(addr, ServerTrust::Any) {
            Ok(x) => x,
            Err(e) => return Outcome::error(format!("connecting to {addr}: {e}")),
        };
        let answer = stream
            .send(Attach::Node(ticket).body())
            .and_then(|()| stream.recv_timeout(ATTACH_WAIT));
        let o = match answer {
            Ok(Some(body)) => match parse_attach_answer(&body) {
                Ok(w) => Outcome::ok().fact("client", w.client),
                Err(r) => Outcome {
                    status: refused(&r),
                    facts: BTreeMap::new(),
                },
            },
            Ok(None) => Outcome::error("the game server did not answer in time"),
            Err(e) => Outcome::error(format!("attach: {e}")),
        };
        stream.close();
        node.close();
        o
    }

    /// Enters a game directly: opens the game stream and attaches it with
    /// the stand-in's ticket unless one is already open (the stand-in
    /// keeps the stream for the next enter after a leave). The real
    /// server's games are entered through the lobby instead.
    pub fn open_game(&mut self) -> Outcome {
        if self.in_game() {
            return Outcome::ok();
        }
        match self.attach(Attach::Ticket([0; 16])) {
            Ok(welcome) => {
                self.view = View::default();
                self.game_token = Some(welcome.resume);
                self.game_client = Some(welcome.client);
                Outcome::ok()
                    .fact("client", welcome.client)
                    .fact("max_players", welcome.max_players)
                    .fact("tick", welcome.tick_rate)
            }
            Err(status) => Outcome {
                status,
                facts: BTreeMap::new(),
            },
        }
    }

    /// Ends the connection with a goodbye.
    pub fn disconnect(&mut self) -> Outcome {
        match self.conn.take() {
            Some(c) => {
                c.close();
                self.closed = Some("disconnected".into());
                self.session = None;
                self.game_token = None;
                self.game_client = None;
                self.selected = None;
                self.game_id = None;
                Outcome::ok()
            }
            None => Outcome::error("not connected"),
        }
    }

    /// Sends one C→S message on the game stream.
    pub fn send(&mut self, lane: Lane, msg: &[u8]) -> Outcome {
        let failed = {
            let Some(conn) = self.conn.as_mut() else {
                return self.closed_outcome();
            };
            let Some(game) = conn.game.as_mut() else {
                return Outcome::error("not in a game (enter first)");
            };
            game.send(Packet::Send {
                lane,
                msg: msg.to_vec(),
            })
            .err()
        };
        let Some(e) = failed else {
            return Outcome::ok();
        };
        Outcome {
            status: self.close_now(e.to_string()),
            facts: BTreeMap::new(),
        }
    }

    /// Sends one body on the session's control stream.
    fn send_control(&mut self, body: Vec<u8>) -> Result<(), Status> {
        if self.conn.is_none() {
            return Err(self.closed_outcome().status);
        }
        let failed = {
            let conn = self.conn.as_mut().expect("checked");
            conn.control.send(body).err()
        };
        match failed {
            None => Ok(()),
            Some(e) => Err(self.close_now(format!("the control stream: {e}"))),
        }
    }

    /// The outcome of a step on a bot without a connection.
    pub fn closed_outcome(&self) -> Outcome {
        match &self.closed {
            Some(why) => Outcome {
                status: Status::Closed(why.clone()),
                facts: BTreeMap::new(),
            },
            None => Outcome::error("not connected"),
        }
    }

    /// The next body from the control stream, waiting at most `wait`.
    fn read_body(&mut self, wait: Duration) -> Result<Vec<u8>, Status> {
        enum Got {
            Body(Vec<u8>),
            Empty,
            End(String),
        }
        let deadline = Instant::now() + wait;
        loop {
            if self.conn.is_none() {
                return Err(self.closed_outcome().status);
            }
            let got = {
                let conn = self.conn.as_mut().expect("checked");
                match conn.inbox.pop_front() {
                    Some(b) => Got::Body(b),
                    None => match conn.control.recv() {
                        Ok(Some(b)) => Got::Body(b),
                        Ok(None) => Got::Empty,
                        Err(e) => Got::End(e.to_string()),
                    },
                }
            };
            match got {
                Got::Body(b) => return Ok(b),
                Got::End(e) => {
                    return Err(self.close_now(format!("the session's control stream ended: {e}")))
                }
                Got::Empty => {}
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Status::Error("the server did not answer in time".into()));
            }
            std::thread::sleep(left.min(POLL));
        }
    }

    /// Takes in a body a driver doesn't answer: a game list update, a
    /// chat line, or a session-level error. True when it was one.
    fn handle_event(&mut self, body: &[u8]) -> bool {
        let Some(&kind) = body.first() else {
            return false;
        };
        match kind {
            msg_kind::ERROR => match ErrorMsg::parse(body) {
                Ok(e) => {
                    self.note_session_error(e);
                    true
                }
                Err(_) => false,
            },
            k if k == lobby::kind::GAME_LIST_BEGIN => {
                if matches!(
                    LobbyEvent::parse(body),
                    Ok(LobbyEvent::GameListBegin { .. })
                ) {
                    self.games.clear();
                    return true;
                }
                false
            }
            k if k == lobby::kind::GAME_ENTRY => match LobbyEvent::parse(body) {
                Ok(LobbyEvent::GameEntry(g)) => {
                    self.known_games.insert(g.name.clone());
                    self.games.insert(g.game_id, g);
                    true
                }
                _ => false,
            },
            k if k == lobby::kind::GAME_LIST_END => {
                if matches!(LobbyEvent::parse(body), Ok(LobbyEvent::GameListEnd)) {
                    self.lists += 1;
                    self.listening = true;
                    return true;
                }
                false
            }
            // The game-server list, on entering the lobby and on change.
            d2_net::msg::nodelist::kind::NODE_LIST => {
                match d2_net::msg::nodelist::NodeList::parse(body) {
                    Ok(l) => {
                        self.known_nodes.extend(l.0.iter().map(|n| n.node.clone()));
                        self.nodes = l.0;
                        self.node_lists += 1;
                        true
                    }
                    Err(_) => false,
                }
            }
            k if k == lobby::kind::GAME_GONE => match LobbyEvent::parse(body) {
                Ok(LobbyEvent::GameGone { game_id }) => {
                    self.games.remove(&game_id);
                    true
                }
                _ => false,
            },
            // The market's pushes (and late answers) between its steps.
            k if msg_kind::MARKET.contains(&k) => match d2_net::msg::market::FromRealm::parse(body)
            {
                Ok(msg) => {
                    self.market_pushed(msg);
                    true
                }
                Err(_) => false,
            },
            // A ladder answer that came after its step gave up.
            k if msg_kind::LADDER.contains(&k) => {
                d2_net::msg::ladder::FromRealm::parse(body).is_ok()
            }
            // The chat's pushes (and late answers) between its steps.
            k if msg_kind::SOCIAL.contains(&k) => match d2_net::msg::social::FromRealm::parse(body)
            {
                Ok(msg) => {
                    self.social_pushed(msg);
                    true
                }
                Err(_) => false,
            },
            // The guilds' notices (and late answers) between their steps.
            k if msg_kind::GUILD.contains(&k) => match d2_net::msg::guild::FromRealm::parse(body) {
                Ok(msg) => {
                    self.guild_pushed(msg);
                    true
                }
                Err(_) => false,
            },
            // The friends' notices (and late answers) between their steps.
            k if msg_kind::FRIENDS.contains(&k) => match d2_net::msg::guild::FromRealm::parse(body)
            {
                Ok(msg) => {
                    self.friends_pushed(msg);
                    true
                }
                Err(_) => false,
            },
            // A stash move's answer no driver waits for (`answer=no`).
            stash_msg::kind::STASH_DONE => {
                matches!(StashEvent::parse(body), Ok(StashEvent::Done { .. }))
            }
            k if k == lobby::kind::CHAT_LINE => match LobbyEvent::parse(body) {
                Ok(LobbyEvent::ChatLine { from, text, .. }) => {
                    self.last_chat = Some((from, text));
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Reads bodies until one of the `answers` kinds arrives (an `ERROR`
    /// for this request's kind is the failure); events in between are
    /// taken in.
    fn request(
        &mut self,
        req: &dyn RequestBody,
        answers: &[u8],
        wait: Duration,
    ) -> Result<Vec<u8>, Failed> {
        let kind = req.kind();
        let body = req.body();
        self.send_control(body).map_err(Failed::Local)?;
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let b = self.read_body(left).map_err(Failed::Local)?;
            match b.first() {
                Some(&msg_kind::ERROR) => {
                    let e = ErrorMsg::parse(&b)
                        .map_err(|_| Failed::Local(Status::Error("a malformed error".into())))?;
                    if e.request_kind == kind {
                        return Err(Failed::Server(e));
                    }
                    self.note_session_error(e);
                }
                Some(k) if answers.contains(k) => return Ok(b),
                Some(_) => {
                    if !self.handle_event(&b) {
                        return Err(Failed::Local(Status::Error(format!(
                            "unexpected answer 0x{:02X} to 0x{kind:02X}",
                            b[0]
                        ))));
                    }
                }
                None => {
                    return Err(Failed::Local(Status::Error(
                        "the server sent an empty answer".into(),
                    )))
                }
            }
        }
    }

    /// `register ACCOUNT PASSWORD`: a new account, logged in at once.
    pub fn register(&mut self, account: &str, password: &str, wait: Duration) -> Outcome {
        let req = AccountRequest::Register {
            account: account.into(),
            password: password.into(),
        };
        self.login_step(&req, wait)
    }

    /// `login ACCOUNT PASSWORD`.
    pub fn login(&mut self, account: &str, password: &str, wait: Duration) -> Outcome {
        let req = AccountRequest::Login {
            account: account.into(),
            password: password.into(),
        };
        self.login_step(&req, wait)
    }

    /// A register or login, with the answer's time and a refusal's wait.
    fn login_step(&mut self, req: &AccountRequest, wait: Duration) -> Outcome {
        let started = Instant::now();
        let answer = self.request(req, &[lobby_answer::LOGGED_IN], wait);
        let ms = started.elapsed().as_millis();
        match answer {
            Ok(body) => match AccountEvent::parse(&body) {
                Ok(AccountEvent::LoggedIn {
                    account,
                    max_characters,
                }) => {
                    self.logged_in = Some(account.clone());
                    Outcome::ok()
                        .fact("account", account)
                        .fact("max_characters", max_characters)
                        .fact("ms", ms)
                        .fact("wait_s", 0)
                }
                Ok(_) => Outcome::error("the server answered something else"),
                Err(e) => Outcome::error(format!("a malformed login answer: {e}")),
            },
            Err(f) => f.outcome(ms),
        }
    }

    /// `logout`: the server ends the session and closes the connection.
    pub fn logout(&mut self, wait: Duration) -> Outcome {
        if let Err(status) = self.send_control(AccountRequest::Logout.body()) {
            return Outcome {
                status,
                facts: BTreeMap::new(),
            };
        }
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.read_body(left) {
                Ok(_) => {}
                Err(Status::Closed(_)) => {
                    if let Some(conn) = self.conn.take() {
                        conn.close();
                    }
                    return Outcome::ok();
                }
                Err(e) => {
                    return Outcome {
                        status: e,
                        facts: BTreeMap::new(),
                    }
                }
            }
            if Instant::now() >= deadline {
                return Outcome::error("the server did not close the session");
            }
        }
    }

    /// `char-create NAME CLASS [hardcore] [classic] [ladder]`.
    pub fn char_create(
        &mut self,
        name: &str,
        class: u8,
        hardcore: bool,
        classic: bool,
        ladder: bool,
        wait: Duration,
    ) -> Outcome {
        self.known_chars.insert(name.to_string());
        let req = AccountRequest::CharCreate {
            name: name.into(),
            class,
            hardcore,
            expansion: !classic,
            ladder,
        };
        let started = Instant::now();
        match self.request(&req, &[lobby_answer::CHAR_CREATED], wait) {
            Ok(body) => match AccountEvent::parse(&body) {
                Ok(AccountEvent::CharCreated(c)) => {
                    self.known_chars.insert(c.name.clone());
                    Outcome::ok()
                        .fact("name", c.name)
                        .fact("class", c.class)
                        .fact("difficulties", c.difficulties)
                }
                Ok(_) => Outcome::error("the server answered something else"),
                Err(e) => Outcome::error(format!("a malformed character answer: {e}")),
            },
            Err(f) => f.outcome(started.elapsed().as_millis()),
        }
    }

    /// `char-list`: the account's characters.
    pub fn char_list(&mut self, wait: Duration) -> Outcome {
        let started = Instant::now();
        match self.request(&AccountRequest::CharList, &[lobby_answer::CHAR_LIST], wait) {
            Ok(body) => match AccountEvent::parse(&body) {
                Ok(AccountEvent::CharList(chars)) => {
                    for c in &chars {
                        self.known_chars.insert(c.name.clone());
                    }
                    let mut o = Outcome::ok().fact("count", chars.len());
                    for name in &self.known_chars {
                        let there = chars.iter().any(|c| c.name.eq_ignore_ascii_case(name));
                        o = o.fact(format!("has.{name}"), if there { "yes" } else { "no" });
                    }
                    for c in &chars {
                        o = o
                            .fact(format!("{}.level", c.name), c.level)
                            .fact(format!("{}.class", c.name), c.class)
                            .fact(format!("{}.difficulties", c.name), c.difficulties)
                            .fact(format!("{}.season", c.name), c.season)
                            // A stored save sets when it was last played.
                            .fact(
                                format!("{}.played", c.name),
                                if c.last_played != 0 { "yes" } else { "no" },
                            );
                    }
                    o
                }
                Ok(_) => Outcome::error("the server answered something else"),
                Err(e) => Outcome::error(format!("a malformed character list: {e}")),
            },
            Err(f) => f.outcome(started.elapsed().as_millis()),
        }
    }

    /// `char-select NAME`: the lobby plays with this character.
    pub fn char_select(&mut self, name: &str, wait: Duration) -> Outcome {
        self.known_chars.insert(name.to_string());
        let req = AccountRequest::CharSelect { name: name.into() };
        let started = Instant::now();
        match self.request(&req, &[lobby_answer::CHAR_SELECTED], wait) {
            Ok(body) => match AccountEvent::parse(&body) {
                Ok(AccountEvent::CharSelected(c)) => {
                    let o = Outcome::ok()
                        .fact("name", &c.name)
                        .fact("difficulties", c.difficulties);
                    self.selected = Some(c);
                    // The lobby screen lists the games: subscribing is what
                    // lets this player hear lobby chat.
                    let left = wait.saturating_sub(started.elapsed());
                    let _ = self.list_games(left);
                    o
                }
                Ok(_) => Outcome::error("the server answered something else"),
                Err(e) => Outcome::error(format!("a malformed character answer: {e}")),
            },
            Err(f) => f.outcome(started.elapsed().as_millis()),
        }
    }

    /// `char-delete NAME PASSWORD`: the account's password confirms it.
    pub fn char_delete(&mut self, name: &str, password: &str, wait: Duration) -> Outcome {
        self.known_chars.insert(name.to_string());
        let req = AccountRequest::CharDelete {
            name: name.into(),
            password: password.into(),
        };
        let started = Instant::now();
        match self.request(&req, &[lobby_answer::CHAR_DELETED], wait) {
            Ok(body) => match AccountEvent::parse(&body) {
                Ok(AccountEvent::CharDeleted { name }) => {
                    if self
                        .selected
                        .as_ref()
                        .is_some_and(|c| c.name.eq_ignore_ascii_case(&name))
                    {
                        self.selected = None;
                    }
                    Outcome::ok().fact("name", name)
                }
                Ok(_) => Outcome::error("the server answered something else"),
                Err(e) => Outcome::error(format!("a malformed character answer: {e}")),
            },
            Err(f) => f.outcome(started.elapsed().as_millis()),
        }
    }

    /// Listens from now on and replaces the game list with a snapshot.
    fn list_games(&mut self, wait: Duration) -> Result<(), Failed> {
        let before = self.lists;
        let body = LobbyRequest::ListGames.body();
        self.send_control(body).map_err(Failed::Local)?;
        let deadline = Instant::now() + wait;
        while self.lists == before {
            let left = deadline.saturating_duration_since(Instant::now());
            let b = self.read_body(left).map_err(Failed::Local)?;
            if b.first() == Some(&msg_kind::ERROR) {
                let e = ErrorMsg::parse(&b)
                    .map_err(|_| Failed::Local(Status::Error("a malformed error".into())))?;
                return Err(Failed::Server(e));
            }
            self.handle_event(&b);
        }
        Ok(())
    }

    /// `games`: the open games with their counts, kind, lock and whether
    /// the selected character may join.
    pub fn games(&mut self, wait: Duration) -> Outcome {
        let started = Instant::now();
        match self.list_games(wait) {
            Ok(()) => {
                let mut o = Outcome::ok().fact("count", self.games.len());
                for name in &self.known_games {
                    let there = self
                        .games
                        .values()
                        .any(|g| g.name.eq_ignore_ascii_case(name));
                    o = o.fact(format!("has.{name}"), if there { "yes" } else { "no" });
                }
                for g in self.games.values() {
                    o = o
                        .fact(format!("{}.players", g.name), g.players)
                        .fact(format!("{}.max", g.name), g.max_players)
                        .fact(
                            format!("{}.difficulty", g.name),
                            difficulty_name(g.difficulty),
                        )
                        .fact(
                            format!("{}.locked", g.name),
                            if g.locked { "yes" } else { "no" },
                        )
                        .fact(format!("{}.joinable", g.name), g.joinable)
                        .fact(format!("{}.hardcore", g.name), yes_no(g.hardcore))
                        .fact(format!("{}.expansion", g.name), yes_no(g.expansion))
                        .fact(format!("{}.desc", g.name), &g.description)
                        .fact(
                            format!("{}.levels", g.name),
                            levels_text(g.level_min, g.level_max),
                        )
                        .fact(format!("{}.server", g.name), &g.server)
                        .fact(format!("{}.node", g.name), &g.node_id)
                        .fact(format!("{}.hosted", g.name), yes_no(g.player_hosted))
                        .fact(format!("{}.created", g.name), yes_no(g.created_ms > 0));
                }
                o
            }
            Err(f) => f.outcome(started.elapsed().as_millis()),
        }
    }

    /// `nodes`: the main server's last game-server list (it comes on
    /// entering the lobby and on every change).
    pub fn node_list(&mut self) -> Outcome {
        use d2_net::msg::nodelist::NodeState;
        self.drain_events();
        let mut o = Outcome::ok()
            .fact("count", self.nodes.len())
            .fact("lists", self.node_lists);
        let suggested = self.nodes.iter().find(|n| n.suggested);
        o = o.fact("suggested", suggested.map_or("none", |n| n.node.as_str()));
        for id in &self.known_nodes {
            o = o.fact(format!("has.{id}"), "no");
        }
        for n in &self.nodes {
            let id = &n.node;
            let state = match n.state {
                NodeState::Open => "open",
                NodeState::Full => "full",
                NodeState::Draining => "draining",
                NodeState::Down => "down",
            };
            o = o
                .fact(format!("has.{id}"), "yes")
                .fact(format!("{id}.name"), &n.name)
                .fact(format!("{id}.location"), &n.location)
                .fact(format!("{id}.addr"), &n.ping_addr)
                .fact(format!("{id}.load"), n.load_pct)
                .fact(format!("{id}.state"), state);
        }
        o
    }

    /// `create-game NAME [password=P] [difficulty=...] [max=N] [desc=TEXT]
    /// [levels=MIN-MAX] [on-node ID|auto|pick:ID|host]`: makes the game
    /// where `place` says, takes its seat and enters it.
    pub fn create_game(
        &mut self,
        name: &str,
        password: &str,
        opts: &CreateOpts,
        wait: Duration,
        until: Instant,
    ) -> Outcome {
        self.known_games.insert(name.to_string());
        let req = LobbyRequest::CreateGame {
            name: name.into(),
            password: password.into(),
            difficulty: opts.difficulty,
            max_players: opts.max_players,
            place: opts.place.clone(),
            description: opts.description.clone(),
            level_min: opts.level_min,
            level_max: opts.level_max,
        };
        let started = Instant::now();
        match self.request(
            &req,
            &[lobby_answer::JOIN_OK, lobby_answer::JOIN_NODE],
            wait,
        ) {
            Ok(body) => self.joined(&body, name, until, started),
            Err(f) => f.outcome(started.elapsed().as_millis()),
        }
    }

    /// `join-game NAME [password=P]`: finds the game in the list, takes a
    /// seat and enters it.
    pub fn join_game(
        &mut self,
        name: &str,
        password: &str,
        wait: Duration,
        until: Instant,
    ) -> Outcome {
        self.known_games.insert(name.to_string());
        let started = Instant::now();
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if let Err(f) = self.list_games(left.max(POLL)) {
                return f.outcome(started.elapsed().as_millis());
            }
            let found = self
                .games
                .values()
                .find(|g| g.name.eq_ignore_ascii_case(name))
                .map(|g| (g.game_id, g.difficulty));
            let Some((game_id, difficulty)) = found else {
                if Instant::now() >= deadline {
                    return Outcome::error(format!("no game {name} in the list"));
                }
                std::thread::sleep(Duration::from_millis(50));
                continue;
            };
            let req = LobbyRequest::JoinGame {
                game_id,
                password: password.into(),
            };
            let left = deadline.saturating_duration_since(Instant::now());
            let answers = [lobby_answer::JOIN_OK, lobby_answer::JOIN_NODE];
            match self.request(&req, &answers, left.max(POLL)) {
                Ok(body) => {
                    let o = self.joined(&body, name, until, started);
                    return o.fact("difficulty", difficulty_name(difficulty));
                }
                Err(Failed::Server(e))
                    if e.code == ErrorCode::CHAR_IN_GAME
                        || e.code == ErrorCode::ALREADY_IN_GAME =>
                {
                    // The character leaves a game a moment before the
                    // lobby hears of it: joining again right after a leave
                    // waits for that.
                    if Instant::now() >= until {
                        return Failed::Server(e).outcome(started.elapsed().as_millis());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(f) => return f.outcome(started.elapsed().as_millis()),
            }
        }
    }

    /// The seat is held: attach the game stream and enter. `JoinOk` is a
    /// dedicated server's game, entered on this connection with its
    /// ticket; `JoinNode` is a game server's, entered on a connection of
    /// its own there.
    fn joined(&mut self, body: &[u8], name: &str, until: Instant, started: Instant) -> Outcome {
        let (game_id, difficulty, node) = match LobbyEvent::parse(body) {
            Ok(LobbyEvent::JoinOk {
                game_id,
                ticket,
                difficulty,
            }) => (game_id, difficulty, Err(ticket)),
            Ok(LobbyEvent::JoinNode(j)) => {
                let at = match node_at(&j) {
                    Ok(at) => at,
                    Err(e) => return Outcome::error(e),
                };
                (j.game_id, j.difficulty, Ok((at, j)))
            }
            _ => return Outcome::error("the server answered something else to the join"),
        };
        self.game_id = Some(game_id);
        self.game_difficulty = Some(difficulty);
        self.listening = false;
        let mut o = Outcome::ok();
        if std::mem::take(&mut self.hold_join) {
            self.enter_as = None;
            if let Ok((_, j)) = node {
                o = o.fact("node", &j.node);
                self.last_join = Some(j);
            }
            return o
                .fact("game_id", game_id)
                .fact("difficulty", difficulty_name(difficulty))
                .fact("entered", "no");
        }
        let attached = match node {
            Err(ticket) => {
                self.game_node = None;
                self.attach(Attach::Ticket(ticket))
            }
            Ok((at, j)) => {
                o = o.fact("node", &j.node);
                let ticket = j.ticket.clone();
                self.last_join = Some(j);
                self.game_node = Some(at);
                self.attach_node(at, Attach::Node(ticket))
            }
        };
        let welcome = match attached {
            Ok(w) => w,
            Err(status) => {
                o.status = status;
                return o.fact("game_id", game_id);
            }
        };
        self.view = View::default();
        self.game_token = Some(welcome.resume);
        self.game_client = Some(welcome.client);
        let class = self.selected.as_ref().map_or(0, |c| c.class);
        let node = o.facts.remove("node");
        let mut o = self.enter_current(name, class, difficulty, until);
        if let Some(node) = node {
            o = o.fact("node", node);
        }
        o = o
            .fact("game_id", game_id)
            .fact("difficulty", difficulty_name(difficulty))
            .fact("ms", started.elapsed().as_millis());
        o
    }

    /// Sends the 0x67 join for the game just attached, waits for the
    /// game's `0x02`, then sends 0x6B and waits for its own player.
    pub fn enter_current(
        &mut self,
        game: &str,
        class: u8,
        difficulty: u8,
        until: Instant,
    ) -> Outcome {
        if !self.in_game() {
            return Outcome::error("not in a game (create or join one first)");
        }
        let mut game_name = [0u8; 16];
        let n = game.len().min(15);
        game_name[..n].copy_from_slice(&game.as_bytes()[..n]);
        let mut char_name = [0u8; 16];
        let name = self.enter_as.take().unwrap_or_else(|| self.play_name());
        let n = name.len().min(15);
        char_name[..n].copy_from_slice(&name.as_bytes()[..n]);
        let req = d2_proto::client::CreateGame {
            game_name,
            class,
            difficulty,
            char_name,
            flags: (1 << 20) | 0x4,
            ..d2_proto::client::CreateGame::default()
        };
        self.view.loaded = false;
        self.view.exiting = false;
        self.game_refused = None;
        let o = self.send(Lane::System, &req.encode());
        if o.status != Status::Ok {
            return o;
        }
        self.entered = true;
        if let Err(e) = self.wait_view(until, |v| v.loaded) {
            return e;
        }
        let o = self.send(Lane::System, &[0x6B]);
        if o.status != Status::Ok {
            return o;
        }
        if let Err(e) = self.wait_view(until, |v| v.own.is_some()) {
            return e;
        }
        match self
            .view
            .own
            .and_then(|g| self.view.players.get(&g).map(|p| (g, p)))
        {
            Some((g, p)) => Outcome::ok().fact("guid", g).fact("x", p.x).fact("y", p.y),
            None => Outcome {
                status: Status::Refused {
                    code: "no-player".into(),
                    text: "no player assigned in time".into(),
                },
                facts: BTreeMap::new(),
            },
        }
    }

    /// Pumps until the view holds `done`; a closed connection or game is
    /// the failure.
    fn wait_view(&mut self, until: Instant, done: impl Fn(&View) -> bool) -> Result<(), Outcome> {
        loop {
            if let Some(why) = self.pump() {
                return Err(Outcome {
                    status: Status::Closed(why),
                    facts: BTreeMap::new(),
                });
            }
            if done(&self.view) {
                return Ok(());
            }
            if !self.in_game() {
                if let Some(code) = self.game_refused.take() {
                    return Err(Outcome {
                        status: Status::Refused {
                            code: code.name().into(),
                            text: code.text().into(),
                        },
                        facts: BTreeMap::new(),
                    });
                }
                return Err(Outcome::error("the game stream ended"));
            }
            if Instant::now() >= until {
                return Err(Outcome::error("the game did not answer in time"));
            }
            std::thread::sleep(POLL);
        }
    }

    /// `leave-game`: leaves the game (0x69), ends its stream and is back
    /// in the lobby.
    pub fn leave_game(&mut self, wait: Duration) -> Outcome {
        if !self.in_game() {
            return Outcome::error("not in a game");
        }
        self.view.exiting = false;
        let o = self.send(Lane::System, &[0x69]);
        if o.status != Status::Ok {
            return o;
        }
        let deadline = Instant::now() + wait;
        loop {
            self.pump();
            if self.view.exiting || !self.in_game() || !self.connected() {
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(POLL);
        }
        if let Some(conn) = self.conn.as_mut() {
            if let Some(mut game) = conn.game.take() {
                game.close();
            }
            conn.leave_node();
        }
        self.entered = false;
        self.game_token = None;
        self.game_id = None;
        self.game_difficulty = None;
        self.game_node = None;
        self.view = View::default();
        Outcome::ok()
    }

    /// `chat TEXT`: says a line in the lobby, heard by every listener.
    pub fn chat(&mut self, text: &str, wait: Duration) -> Outcome {
        if !self.listening {
            if let Err(f) = self.list_games(wait) {
                return f.outcome(0);
            }
        }
        let body = LobbyRequest::Chat { text: text.into() }.body();
        if let Err(status) = self.send_control(body) {
            return Outcome {
                status,
                facts: BTreeMap::new(),
            };
        }
        let me = self.play_name();
        let deadline = Instant::now() + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let b = match self.read_body(left) {
                Ok(b) => b,
                Err(e) => {
                    return Outcome {
                        status: e,
                        facts: BTreeMap::new(),
                    }
                }
            };
            if b.first() == Some(&msg_kind::ERROR) {
                if let Ok(e) = ErrorMsg::parse(&b) {
                    if e.request_kind == lobby::kind::CHAT {
                        return Failed::Server(e).outcome(0);
                    }
                    self.note_session_error(e);
                }
                continue;
            }
            let Ok(LobbyEvent::ChatLine { from, text, .. }) = LobbyEvent::parse(&b) else {
                self.handle_event(&b);
                continue;
            };
            self.last_chat = Some((from.clone(), text.clone()));
            if from.eq_ignore_ascii_case(&me) {
                return Outcome::ok();
            }
        }
    }

    /// `heard-game [TEXT..]`: the server's lines in its game; with TEXT,
    /// whether (and how often) a line of exactly that text came.
    pub fn heard_game(&mut self, text: &[String]) -> Outcome {
        self.drain_events();
        let lines = &self.view.server_lines;
        let mut o = Outcome::ok().fact("lines", lines.len());
        if let Some(l) = lines.last() {
            o = o.fact("last", l);
        }
        if !text.is_empty() {
            let want = text.join(" ");
            let times = lines.iter().filter(|l| **l == want).count();
            o = o
                .fact("has", if times > 0 { "yes" } else { "no" })
                .fact("times", times);
        }
        o
    }

    /// `heard`: the last chat line and who said it.
    pub fn heard(&mut self) -> Outcome {
        self.drain_events();
        match &self.last_chat {
            Some((from, text)) => Outcome::ok().fact("last", text).fact("from", from),
            None => Outcome::ok(),
        }
    }

    /// Takes in every queued event, leaving request answers alone.
    /// `stash-put ITEM`: the unit `unit` of its character in game goes to
    /// the shared stash (0xA2), as an item or to its slot (`to`
    /// [`goods::STACK`]); ok once the main server says it is there
    /// (`answer` false: once sent).
    pub fn stash_put(&mut self, unit: u32, to: u8, answer: bool, wait: Duration) -> Outcome {
        self.stash_req += 1;
        let req = StashRequest::Put {
            req: self.stash_req,
            unit,
            to,
            gold: 0,
            cell: None,
        };
        self.stash_move(&req, answer, wait).fact("unit", unit)
    }

    /// `stash-send ITEM...`: the units it holds go to their slots in one
    /// request (0xA5, the inventory's send button); ok once the main
    /// server answers, with `count` the units moved and `left` the rest.
    pub fn stash_send(&mut self, units: Vec<u32>, answer: bool, wait: Duration) -> Outcome {
        self.stash_req += 1;
        let n = units.len();
        let req = StashRequest::PutStacks {
            req: self.stash_req,
            units,
        };
        self.stash_move(&req, answer, wait).fact("units", n)
    }

    /// `stash-take-stack CODE N|all`: `count` of the slot `code` come to
    /// its character's inventory as plain items (0xA3); ok once there,
    /// with `count` what came.
    pub fn stash_take_stack(
        &mut self,
        code: &str,
        count: u32,
        answer: bool,
        wait: Duration,
    ) -> Outcome {
        self.stash_req += 1;
        let req = StashRequest::Take {
            req: self.stash_req,
            from: goods::STACK,
            item: None,
            stack: Some((code.to_owned(), count)),
            gold: 0,
            place: stash_msg::place::INVENTORY,
        };
        self.stash_move(&req, answer, wait)
    }

    /// `stash-take ITEM`: the first item of the stash whose name or code
    /// holds `code` comes to its character's inventory, or its cursor
    /// (`place`) (0xA3); ok once the main server says it arrived there
    /// (`answer` false: once sent).
    pub fn stash_take(&mut self, code: &str, place: u8, answer: bool, wait: Duration) -> Outcome {
        let started = Instant::now();
        let id = match self.stash_find(code, wait) {
            Ok(hits) => match hits.first() {
                Some(&(id, _)) => id,
                None => return Outcome::error(format!("no {code} in the stash")),
            },
            Err(f) => return f.outcome(started.elapsed().as_millis()),
        };
        self.stash_req += 1;
        let req = StashRequest::Take {
            req: self.stash_req,
            from: goods::ITEM,
            item: Some(id),
            stack: None,
            gold: 0,
            place,
        };
        let left = wait.saturating_sub(started.elapsed());
        self.stash_move(&req, answer, left).fact("item", id)
    }

    /// Sends a changing stash request and waits for its `StashDone`: ok
    /// with `count` (what moved), or refused with the code's name (its
    /// number when the contracts give it none). Without `answer` it
    /// returns once sent; the answer is taken in when it comes.
    fn stash_move(&mut self, req: &StashRequest, answer: bool, wait: Duration) -> Outcome {
        let started = Instant::now();
        let n = req.req();
        let kind = req.kind();
        if let Err(s) = self.send_control(req.body()) {
            return Failed::Local(s).outcome(0);
        }
        if !answer {
            return Outcome::ok();
        }
        let deadline = started + wait;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let b = match self.read_body(left) {
                Ok(b) => b,
                Err(s) => return Failed::Local(s).outcome(started.elapsed().as_millis()),
            };
            let ms = started.elapsed().as_millis();
            match b.first() {
                Some(&msg_kind::ERROR) => match ErrorMsg::parse(&b) {
                    Ok(e) if e.request_kind == kind => return Failed::Server(e).outcome(ms),
                    Ok(e) => self.note_session_error(e),
                    Err(_) => return Outcome::error("a malformed error"),
                },
                Some(&stash_msg::kind::STASH_DONE) => match StashEvent::parse(&b) {
                    Ok(StashEvent::Done {
                        req,
                        code,
                        count,
                        left,
                    }) if Some(req) == n => {
                        let o = if code == 0 {
                            Outcome::ok()
                        } else {
                            let c = ErrorCode(code);
                            let name = match c.name() {
                                "Unknown" => code.to_string(),
                                name => name.to_owned(),
                            };
                            Outcome {
                                status: Status::Refused {
                                    code: name,
                                    text: c.text().into(),
                                },
                                facts: BTreeMap::new(),
                            }
                        };
                        return o
                            .fact("count", count)
                            .fact("left", left.len())
                            .fact("ms", ms);
                    }
                    // An earlier request's late answer.
                    Ok(StashEvent::Done { .. }) => {}
                    _ => return Outcome::error("a malformed StashDone"),
                },
                Some(_) => {
                    if !self.handle_event(&b) {
                        return Outcome::error(format!(
                            "unexpected answer 0x{:02X} to 0x{kind:02X}",
                            b[0]
                        ));
                    }
                }
                None => return Outcome::error("the server sent an empty answer"),
            }
        }
    }

    /// The stash's items whose name or code holds `text` (0xA8).
    fn stash_find(
        &mut self,
        text: &str,
        wait: Duration,
    ) -> Result<Vec<(u32, stash_msg::StashCell)>, Failed> {
        let req = StashRequest::Search { text: text.into() };
        let body = self.request(&req, &[stash_msg::kind::STASH_FOUND], wait)?;
        match StashEvent::parse(&body) {
            Ok(StashEvent::Found { hits }) => Ok(hits),
            _ => Err(Failed::Local(Status::Error(
                "a malformed StashFound".into(),
            ))),
        }
    }

    /// `stash`: the shared stash as the main server shows it: facts gold,
    /// items (on the first page), and for every item code in `codes`
    /// has.CODE=yes|no and CODE.count (items of the whole stash) and
    /// slot.CODE (the count its slot keeps).
    pub fn stash_view(&mut self, codes: &BTreeSet<String>, wait: Duration) -> Outcome {
        let started = Instant::now();
        let req = StashRequest::Open {
            tab: stash_msg::tab::ITEMS,
            page: 0,
        };
        let (gold, items, stacks) = match self.request(&req, &[stash_msg::kind::STASH_PAGE], wait) {
            Ok(body) => match StashEvent::parse(&body) {
                Ok(StashEvent::Page {
                    gold,
                    items,
                    stacks,
                    ..
                }) => (gold, items, stacks),
                _ => return Outcome::error("a malformed StashPage"),
            },
            Err(f) => return f.outcome(started.elapsed().as_millis()),
        };
        // The page's items follow it.
        let deadline = started + wait;
        let mut got = 0;
        while got < items {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.read_body(left) {
                Ok(b) if b.first() == Some(&stash_msg::kind::STASH_ITEM) => got += 1,
                Ok(b) => {
                    if !self.handle_event(&b) {
                        return Outcome::error(format!("unexpected 0x{:02X} in a page", b[0]));
                    }
                }
                Err(s) => return Failed::Local(s).outcome(started.elapsed().as_millis()),
            }
        }
        let mut o = Outcome::ok().fact("gold", gold).fact("items", items);
        for code in codes {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.stash_find(code, left) {
                Ok(hits) => {
                    let slot = stacks
                        .iter()
                        .find(|(c, _)| c.eq_ignore_ascii_case(code))
                        .map_or(0, |(_, n)| *n);
                    o = o
                        .fact(
                            format!("has.{code}"),
                            if hits.is_empty() { "no" } else { "yes" },
                        )
                        .fact(format!("{code}.count"), hits.len())
                        .fact(format!("slot.{code}"), slot);
                }
                Err(f) => return f.outcome(started.elapsed().as_millis()),
            }
        }
        o
    }

    fn drain_events(&mut self) {
        let Some(mut inbox) = self.conn.as_mut().map(|c| std::mem::take(&mut c.inbox)) else {
            return;
        };
        let mut kept = VecDeque::new();
        while let Some(b) = inbox.pop_front() {
            if !self.handle_event(&b) {
                kept.push_back(b);
            }
        }
        if let Some(conn) = self.conn.as_mut() {
            conn.inbox = kept;
        }
    }
}

/// A request a [`Bot::request`] can send: its kind and its body.
trait RequestBody {
    fn kind(&self) -> u8;
    fn body(&self) -> Vec<u8>;
}

impl RequestBody for AccountRequest {
    fn kind(&self) -> u8 {
        AccountRequest::kind(self)
    }

    fn body(&self) -> Vec<u8> {
        AccountRequest::body(self)
    }
}

impl RequestBody for StashRequest {
    fn kind(&self) -> u8 {
        StashRequest::kind(self)
    }

    fn body(&self) -> Vec<u8> {
        StashRequest::body(self)
    }
}

impl RequestBody for LobbyRequest {
    fn kind(&self) -> u8 {
        LobbyRequest::kind(self)
    }

    fn body(&self) -> Vec<u8> {
        LobbyRequest::body(self)
    }
}

/// The S→C kinds the account and lobby drivers wait for.
mod lobby_answer {
    use d2_net::msg::account::kind as account;
    use d2_net::msg::lobby::kind as lobby;

    pub const LOGGED_IN: u8 = account::LOGGED_IN;
    pub const CHAR_LIST: u8 = account::CHAR_LIST_ANSWER;
    pub const CHAR_CREATED: u8 = account::CHAR_CREATED;
    pub const CHAR_DELETED: u8 = account::CHAR_DELETED;
    pub const CHAR_SELECTED: u8 = account::CHAR_SELECTED;
    pub const JOIN_OK: u8 = lobby::JOIN_OK;
    pub const JOIN_NODE: u8 = lobby::JOIN_NODE;
}

/// Where the game server of `j` takes players.
fn node_at(j: &JoinNode) -> Result<NodeAt, String> {
    use std::net::ToSocketAddrs;
    let addr = j
        .addr
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .ok_or_else(|| format!("JoinNode: no address in `{}`", j.addr))?;
    Ok(NodeAt {
        addr,
        cert: j.node_cert_sha256,
    })
}

/// A `create-game`'s settings besides its name and password.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreateOpts {
    pub difficulty: u8,
    /// 0: the server's default.
    pub max_players: u8,
    /// The list's description (`desc=`).
    pub description: String,
    /// The level range (`levels=MIN-MAX`), 0 no bound.
    pub level_min: u8,
    pub level_max: u8,
    /// Where it runs (`on-node`): Automatic without it.
    pub place: d2_net::msg::nodelist::Placement,
}

/// A step's `on-node` word as a placement: `auto` (the main server
/// chooses), `pick:ID` (Automatic with the client's pick), `host` (Host on
/// my PC), else the official game server with that id.
pub fn placement(word: Option<&str>) -> d2_net::msg::nodelist::Placement {
    use d2_net::msg::nodelist::Placement;
    match word {
        None | Some("auto") => Placement::Automatic,
        Some("host") => Placement::HostOnMyPc,
        Some(w) => match w.strip_prefix("pick:") {
            Some(id) => Placement::AutomaticPick(id.into()),
            None => Placement::Official(w.into()),
        },
    }
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// A level range as scenarios say it: `40-70`, `40-` (no upper bound),
/// `-70`, or `any`.
pub fn levels_text(min: u8, max: u8) -> String {
    let side = |n: u8| if n == 0 { String::new() } else { n.to_string() };
    match (min, max) {
        (0, 0) => "any".into(),
        _ => format!("{}-{}", side(min), side(max)),
    }
}

/// `levels=MIN-MAX` (either side may be empty or 0: no bound; `any` is
/// none) as the two bounds.
pub fn levels_number(text: &str) -> Option<(u8, u8)> {
    if text == "any" {
        return Some((0, 0));
    }
    let (min, max) = text.split_once('-')?;
    let side = |s: &str| {
        if s.is_empty() {
            Some(0)
        } else {
            s.parse::<u8>().ok()
        }
    };
    Some((side(min)?, side(max)?))
}

/// A difficulty's word, as scenarios say it.
pub fn difficulty_name(difficulty: u8) -> &'static str {
    match difficulty {
        1 => "nightmare",
        2 => "hell",
        _ => "normal",
    }
}

/// A difficulty as its number: `normal` (0), `nightmare` (1), `hell` (2).
pub fn difficulty_number(word: &str) -> Option<u8> {
    match word {
        "normal" => Some(0),
        "nightmare" => Some(1),
        "hell" => Some(2),
        _ => word.parse().ok().filter(|d| *d < 3),
    }
}
