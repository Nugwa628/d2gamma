//! The game-server link's messages (kinds 0x80-0x9D, contracts 11.3, and
//! the item transfers 0xD0-0xD4, [`super::transfer`]):
//! what a game server (`d2rust-node`) and the main server
//! (`d2rust-realm`) say on the link's control stream (ALPN
//! `d2rust-node/1`). They never appear on a player's connection.
//!
//! - [`NodeToRealm`]: the game server's messages (N→R).
//! - [`RealmToNode`]: the main server's messages (R→N).
//!
//! Records and journal chunks travel on streams of their own (`d2-link`);
//! the messages here only name them. Each enum's `Debug` hides the host
//! token and the join ticket, so a message can be logged as it is.
//!
//! Upgrades (`docs/plans/server/upgrades.md`): the main server's
//! [`RealmToNode::Update`] runs a countdown to a restart on a new version
//! (its lines for the games, the restart, a cancel, or a catch-up before a
//! version refusal), and the game server answers with
//! [`NodeToRealm::UpdateState`] (downloading, ready, failed, restarting).

use super::account::{CharSummary, CLASSES, MAX_CHAR_NAME_WIRE};
use super::lobby::{DIFFICULTIES, MAX_GAME_NAME};
use super::transfer::{ItemIn, ItemInDone, ItemOut, ItemWanted};
use super::{ErrorCode, Reader, Writer, MAX_ERROR_TEXT};
use crate::wire::session::MAX_GAME_ID;
use crate::wire::MAX_BUILD;
use crate::NetError;

/// The link's kinds.
pub mod kind {
    use std::ops::RangeInclusive;

    pub const NODE_HELLO: u8 = 0x80;
    pub const NODE_WELCOME: u8 = 0x81;
    pub const NODE_REFUSED: u8 = 0x82;
    pub const LOAD: u8 = 0x83;
    pub const START_GAME: u8 = 0x84;
    pub const GAME_STARTED: u8 = 0x85;
    pub const START_REFUSED: u8 = 0x86;
    pub const SEAT_OFFER: u8 = 0x87;
    pub const SEAT_TAKEN: u8 = 0x88;
    pub const SEAT_REFUSED: u8 = 0x89;
    pub const SAVE: u8 = 0x8A;
    pub const SAVE_ACK: u8 = 0x8B;
    pub const PLAYERS: u8 = 0x8C;
    pub const LEFT: u8 = 0x8D;
    pub const CLOSE_GAME: u8 = 0x8E;
    pub const GAME_CLOSED: u8 = 0x8F;
    pub const KICK: u8 = 0x90;
    pub const DRAIN: u8 = 0x91;
    pub const DEV_COMMAND: u8 = 0x92;
    pub const DEV_ANSWER: u8 = 0x93;
    pub const CRASH_REPORT: u8 = 0x94;
    pub const JOURNAL_CHUNK: u8 = 0x95;
    pub const ITEM_IDS_WANTED: u8 = 0x96;
    pub const ITEM_IDS: u8 = 0x97;
    pub const REACH_INFO: u8 = 0x98;
    pub const REACH_RESULT: u8 = 0x99;
    pub const PUNCH: u8 = 0x9A;
    pub const PUNCH_DONE: u8 = 0x9B;
    pub const UPDATE: u8 = 0x9C;
    pub const UPDATE_STATE: u8 = 0x9D;
    pub const ITEM_WANTED: u8 = 0xD0;
    pub const ITEM_OUT: u8 = 0xD1;
    pub const ITEM_OUT_ACK: u8 = 0xD2;
    pub const ITEM_IN: u8 = 0xD3;
    pub const ITEM_IN_DONE: u8 = 0xD4;
    pub const ITEM_OUT_REFUSED: u8 = 0xD5;

    /// The link's range (0x9E-0x9F are reserved).
    pub const NODE: RangeInclusive<u8> = 0x80..=0x9F;
    /// The link's item transfers (0xD6-0xDF are reserved).
    pub const TRANSFER: RangeInclusive<u8> = 0xD0..=0xDF;
}

/// An official game server's id: 1-15 of `a-z`, `0-9` and `-`. A player
/// host's (`h-` and 12 hex digits) is given by the main server.
pub const MAX_NODE_ID: usize = 15;
/// A realm's name in `NodeWelcome`.
pub const MAX_REALM_NAME: usize = 64;
/// An address written as text (`ip:port`, IPv6 in brackets).
pub const MAX_ADDR: usize = 64;
/// A dev command or its answer.
pub const MAX_DEV_TEXT: usize = 200;
/// An `Update`'s line for the games, or an `UpdateState`'s detail.
pub const MAX_UPDATE_TEXT: usize = 200;

/// What an [`RealmToNode::Update`] asks.
pub mod update_phase {
    /// The restart for the version is `seconds` away. The first one of a
    /// version starts its download; its text (when not empty) is the line
    /// every game shows now.
    pub const COUNTDOWN: u8 = 0;
    /// Zero: every game saves, then the game server restarts on the
    /// version. The text is the last line for the games.
    pub const RESTART: u8 = 1;
    /// Called off: the staged package is dropped. The text says so.
    pub const CANCEL: u8 = 2;
    /// Before a version refusal (no welcome follows): fetch the main
    /// server's version and restart on it now.
    pub const CATCH_UP: u8 = 3;
}

/// Where a game server is with an update ([`NodeToRealm::UpdateState`]).
pub mod update_state {
    /// No update (after a cancel).
    pub const IDLE: u8 = 0;
    pub const DOWNLOADING: u8 = 1;
    /// Fetched and checked: it can restart on the version.
    pub const READY: u8 = 2;
    /// The fetch or the check failed; the detail says why.
    pub const FAILED: u8 = 3;
    /// Every game saved; it restarts now.
    pub const RESTARTING: u8 = 4;
}

/// Whether `v` is a version an update can name: the game's form, digits
/// and dots (`0.1.3.0`), at most [`MAX_BUILD`] bytes.
pub fn valid_update_version(v: &str) -> bool {
    (1..=MAX_BUILD).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_digit() || b == b'.')
}

/// Whether `id` is a valid official game-server id.
pub fn valid_node_id(id: &str) -> bool {
    (1..=MAX_NODE_ID).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Whether `id` is a valid mod-realm id (`game` in the main server's
/// config and a game server's bundle): the same form as a game-server id.
/// It names the realm's folder under `data/realms/`.
pub fn valid_game_id(id: &str) -> bool {
    valid_node_id(id)
}

/// What kind of game server a link is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// Run by the owner, added with `d2rust-realm --add-node`.
    Official = 0,
    /// Started by a player's game client (`d2rust-node --host`).
    PlayerHosted = 1,
}

impl NodeKind {
    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        match r.u8()? {
            0 => Ok(Self::Official),
            1 => Ok(Self::PlayerHosted),
            _ => Err(NetError::Malformed),
        }
    }
}

/// A game as the main server asks a game server to run it (contracts
/// section 6's `GameSpec`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeGameSpec {
    pub game: u32,
    pub name: String,
    /// 0 Normal, 1 Nightmare, 2 Hell.
    pub difficulty: u8,
    pub hardcore: bool,
    pub expansion: bool,
    /// 0: the server's limit.
    pub max_players: u8,
}

impl NodeGameSpec {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.game)
            .str8(&self.name)
            .u8(self.difficulty)
            .bool(self.hardcore)
            .bool(self.expansion)
            .u8(self.max_players);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let s = Self {
            game: r.u32()?,
            name: r.str8(MAX_GAME_NAME)?,
            difficulty: r.u8()?,
            hardcore: r.bool()?,
            expansion: r.bool()?,
            max_players: r.u8()?,
        };
        if s.name.is_empty() || s.difficulty >= DIFFICULTIES {
            return Err(NetError::Malformed);
        }
        Ok(s)
    }
}

/// A character that has never been played: the game server makes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewCharInfo {
    pub name: String,
    /// 0-6.
    pub class: u8,
    pub hardcore: bool,
    pub expansion: bool,
    /// The progression set before its first game.
    pub progression: u8,
}

impl NewCharInfo {
    fn encode(&self, w: &mut Writer) {
        w.str8(&self.name)
            .u8(self.class)
            .bool(self.hardcore)
            .bool(self.expansion)
            .u8(self.progression);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let c = Self {
            name: r.str8(MAX_CHAR_NAME_WIRE)?,
            class: r.u8()?,
            hardcore: r.bool()?,
            expansion: r.bool()?,
            progression: r.u8()?,
        };
        if c.name.is_empty() || c.class >= CLASSES {
            return Err(NetError::Malformed);
        }
        Ok(c)
    }
}

/// Realm item IDs granted to a game: `first`, `first + 1`, ...
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemIdBlock {
    pub first: u32,
    pub count: u32,
}

impl ItemIdBlock {
    pub(super) fn encode(&self, w: &mut Writer) {
        w.u32(self.first).u32(self.count);
    }

    pub(super) fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            first: r.u32()?,
            count: r.u32()?,
        })
    }
}

/// A ticket-signing public key (join tickets, plan B1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TicketKey {
    pub key_id: u8,
    /// An ed25519 public key.
    pub public: [u8; 32],
}

/// The game server's hello: who it is, what it runs and how much it takes.
#[derive(Clone, PartialEq, Eq)]
pub struct NodeHello {
    /// The official id; empty for a player host.
    pub node: String,
    /// A player host's token (`HostGrant`); `None` for an official server.
    pub host_token: Option<[u8; 16]>,
    pub version: String,
    pub protocol: u32,
    pub tick_rate: u32,
    /// The mod realm it serves (`Hello.game`).
    pub game: String,
    /// The SHA-256 of the game data it runs on.
    pub data_hash: [u8; 32],
    /// The UDP port players reach it on.
    pub listen_port: u16,
    pub max_games: u16,
    pub max_players_total: u16,
}

impl std::fmt::Debug for NodeHello {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeHello")
            .field("node", &self.node)
            .field("host_token", &self.host_token.map(|_| "<hidden>"))
            .field("version", &self.version)
            .field("protocol", &self.protocol)
            .field("tick_rate", &self.tick_rate)
            .field("game", &self.game)
            .field("listen_port", &self.listen_port)
            .field("max_games", &self.max_games)
            .field("max_players_total", &self.max_players_total)
            .finish_non_exhaustive()
    }
}

/// The main server's welcome: the link is up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeWelcome {
    pub realm: String,
    /// The game server's id (assigned for a player host).
    pub node: String,
    pub kind: NodeKind,
    /// The game server's address as the main server sees it.
    pub seen_addr: String,
    /// How often the game server sends [`Load`].
    pub heartbeat_ms: u32,
    pub orphan_s: u32,
    pub journal_chunk_s: u32,
    pub save_interval_s: u32,
    /// The keys join tickets are signed with.
    pub ticket_keys: Vec<TicketKey>,
}

/// The game server's load, every heartbeat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Load {
    pub games: u16,
    pub players: u16,
    /// CPU use, 0-1000.
    pub cpu_permille: u16,
    pub tick_overruns: u32,
    /// It takes no new games (N21).
    pub draining: bool,
}

/// A character's save (the record on its own stream).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveInfo {
    pub game: u32,
    pub lease: u64,
    pub seq: u32,
    /// 0 periodic, 1 leave, 2 close, 3 shutdown, 4 sleep, 5 transfer.
    pub reason: u8,
    pub journal_tick: u64,
    pub summary: CharSummary,
}

/// The highest save reason (5, transfer).
pub const SAVE_REASON_MAX: u8 = 5;
/// The save reason of a transfer's save (`ItemOut`, `ItemInDone`).
pub const SAVE_REASON_TRANSFER: u8 = 5;

impl SaveInfo {
    pub(super) fn encode(&self, w: &mut Writer) {
        w.u32(self.game)
            .u64(self.lease)
            .u32(self.seq)
            .u8(self.reason)
            .u64(self.journal_tick);
        self.summary.encode(w);
    }

    pub(super) fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            game: r.u32()?,
            lease: r.u64()?,
            seq: r.u32()?,
            reason: small(r, SAVE_REASON_MAX)?,
            journal_tick: r.u64()?,
            summary: CharSummary::decode(r)?,
        })
    }
}

/// N→R: the game server's messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeToRealm {
    Hello(NodeHello),
    Load(Load),
    GameStarted {
        game: u32,
    },
    StartRefused {
        game: u32,
        code: ErrorCode,
    },
    SeatTaken {
        game: u32,
        lease: u64,
    },
    SeatRefused {
        game: u32,
        lease: u64,
        code: ErrorCode,
    },
    Save(SaveInfo),
    Players {
        game: u32,
        players: u8,
    },
    /// After the player's last save was acknowledged.
    Left {
        game: u32,
        lease: u64,
    },
    GameClosed {
        game: u32,
        reason: ErrorCode,
    },
    DevAnswer {
        ok: bool,
        text: String,
    },
    /// The report body travels on a stream.
    CrashReport {
        game: u32,
        text: String,
    },
    /// The chunk travels on a stream.
    JournalChunk {
        game: u32,
        seq: u32,
        first_tick: u64,
        ticks: u32,
        last: bool,
    },
    ItemIdsWanted {
        game: u32,
    },
    ReachInfo {
        /// 0 none, 1 UPnP-IGD, 2 NAT-PMP, 3 PCP.
        method: u8,
        mapped_addr: Option<String>,
    },
    PunchDone {
        game: u32,
        peer_addr: String,
        connected: bool,
    },
    /// Goods left a character for the stash, with its save (the record
    /// on its stream).
    ItemOut(ItemOut),
    /// The answer to an `ItemIn`, with the save that holds the goods when
    /// they were placed (the record on its stream).
    ItemInDone(ItemInDone),
    /// The game couldn't take the goods an `ItemWanted` asked for
    /// (nothing changed on the character): why, as an error code.
    ItemOutRefused {
        lease: u64,
        req: u64,
        code: ErrorCode,
    },
    /// Where the game server is with the update to `version`
    /// ([`update_state`]); `detail` says why it failed.
    UpdateState {
        version: String,
        state: u8,
        detail: String,
    },
}

/// R→N: the main server's messages.
#[derive(Clone, PartialEq, Eq)]
pub enum RealmToNode {
    Welcome(NodeWelcome),
    /// The link is refused (codes 100-108); the main server closes it.
    Refused {
        code: ErrorCode,
        text: String,
    },
    StartGame {
        spec: NodeGameSpec,
        seed: u32,
        item_ids: ItemIdBlock,
    },
    /// The record (if any) travels on a stream.
    SeatOffer {
        game: u32,
        lease: u64,
        player: u64,
        character: String,
        new: Option<NewCharInfo>,
        has_record: bool,
        ticket: Option<[u8; 16]>,
    },
    SaveAck {
        lease: u64,
        seq: u32,
        code: ErrorCode,
    },
    CloseGame {
        game: u32,
        /// 0 save then close, 1 discard (no saves).
        how: u8,
    },
    Kick {
        game: u32,
        lease: u64,
    },
    Drain {
        on: bool,
    },
    /// Test servers only (`dev = test`).
    DevCommand {
        character: String,
        command: String,
    },
    ItemIds {
        game: u32,
        ids: ItemIdBlock,
    },
    ReachResult {
        reachable: bool,
        checked_addr: String,
        /// 0 unknown, 1 open, 2 cone, 3 symmetric.
        nat: u8,
    },
    Punch {
        game: u32,
        peer_addr: String,
        at_ms: u64,
        tries: u8,
    },
    /// Goods of a character are wanted for the stash.
    ItemWanted(ItemWanted),
    /// The answer to an `ItemOut` or an `ItemInDone` (`req` is the
    /// transfer's): 0 stored; else an `ItemOut`'s goods go back to the
    /// character.
    ItemOutAck {
        lease: u64,
        req: u64,
        code: ErrorCode,
    },
    /// Goods from the stash for a character.
    ItemIn(ItemIn),
    /// The upgrade countdown ([`update_phase`]): `seconds` to the restart
    /// on `version`, and the line the games show now (may be empty).
    Update {
        phase: u8,
        version: String,
        seconds: u32,
        text: String,
    },
}

impl std::fmt::Debug for RealmToNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SeatOffer {
                game,
                lease,
                player,
                character,
                new,
                has_record,
                ticket,
            } => f
                .debug_struct("SeatOffer")
                .field("game", game)
                .field("lease", lease)
                .field("player", player)
                .field("character", character)
                .field("new", new)
                .field("has_record", has_record)
                .field("ticket", &ticket.map(|_| "<hidden>"))
                .finish(),
            Self::Welcome(w) => f.debug_tuple("Welcome").field(w).finish(),
            Self::Refused { code, text } => f
                .debug_struct("Refused")
                .field("code", code)
                .field("text", text)
                .finish(),
            Self::StartGame {
                spec,
                seed,
                item_ids,
            } => f
                .debug_struct("StartGame")
                .field("spec", spec)
                .field("seed", seed)
                .field("item_ids", item_ids)
                .finish(),
            Self::SaveAck { lease, seq, code } => f
                .debug_struct("SaveAck")
                .field("lease", lease)
                .field("seq", seq)
                .field("code", code)
                .finish(),
            Self::CloseGame { game, how } => f
                .debug_struct("CloseGame")
                .field("game", game)
                .field("how", how)
                .finish(),
            Self::Kick { game, lease } => f
                .debug_struct("Kick")
                .field("game", game)
                .field("lease", lease)
                .finish(),
            Self::Drain { on } => f.debug_struct("Drain").field("on", on).finish(),
            Self::DevCommand { character, command } => f
                .debug_struct("DevCommand")
                .field("character", character)
                .field("command", command)
                .finish(),
            Self::ItemIds { game, ids } => f
                .debug_struct("ItemIds")
                .field("game", game)
                .field("ids", ids)
                .finish(),
            Self::ReachResult {
                reachable,
                checked_addr,
                nat,
            } => f
                .debug_struct("ReachResult")
                .field("reachable", reachable)
                .field("checked_addr", checked_addr)
                .field("nat", nat)
                .finish(),
            Self::Punch {
                game,
                peer_addr,
                at_ms,
                tries,
            } => f
                .debug_struct("Punch")
                .field("game", game)
                .field("peer_addr", peer_addr)
                .field("at_ms", at_ms)
                .field("tries", tries)
                .finish(),
            Self::ItemWanted(m) => f.debug_tuple("ItemWanted").field(m).finish(),
            Self::ItemOutAck { lease, req, code } => f
                .debug_struct("ItemOutAck")
                .field("lease", lease)
                .field("req", req)
                .field("code", code)
                .finish(),
            Self::ItemIn(m) => f.debug_tuple("ItemIn").field(m).finish(),
            Self::Update {
                phase,
                version,
                seconds,
                text,
            } => f
                .debug_struct("Update")
                .field("phase", phase)
                .field("version", version)
                .field("seconds", seconds)
                .field("text", text)
                .finish(),
        }
    }
}

fn code(r: &mut Reader) -> Result<ErrorCode, NetError> {
    r.u16().map(ErrorCode)
}

/// An update's version ([`valid_update_version`]).
fn update_version(r: &mut Reader) -> Result<String, NetError> {
    let v = r.str8(MAX_BUILD)?;
    if !valid_update_version(&v) {
        return Err(NetError::Malformed);
    }
    Ok(v)
}

fn small(r: &mut Reader, max: u8) -> Result<u8, NetError> {
    let v = r.u8()?;
    if v > max {
        return Err(NetError::Malformed);
    }
    Ok(v)
}

impl NodeToRealm {
    pub fn kind(&self) -> u8 {
        match self {
            Self::Hello(_) => kind::NODE_HELLO,
            Self::Load(_) => kind::LOAD,
            Self::GameStarted { .. } => kind::GAME_STARTED,
            Self::StartRefused { .. } => kind::START_REFUSED,
            Self::SeatTaken { .. } => kind::SEAT_TAKEN,
            Self::SeatRefused { .. } => kind::SEAT_REFUSED,
            Self::Save(_) => kind::SAVE,
            Self::Players { .. } => kind::PLAYERS,
            Self::Left { .. } => kind::LEFT,
            Self::GameClosed { .. } => kind::GAME_CLOSED,
            Self::DevAnswer { .. } => kind::DEV_ANSWER,
            Self::CrashReport { .. } => kind::CRASH_REPORT,
            Self::JournalChunk { .. } => kind::JOURNAL_CHUNK,
            Self::ItemIdsWanted { .. } => kind::ITEM_IDS_WANTED,
            Self::ReachInfo { .. } => kind::REACH_INFO,
            Self::PunchDone { .. } => kind::PUNCH_DONE,
            Self::ItemOut(_) => kind::ITEM_OUT,
            Self::ItemInDone(_) => kind::ITEM_IN_DONE,
            Self::ItemOutRefused { .. } => kind::ITEM_OUT_REFUSED,
            Self::UpdateState { .. } => kind::UPDATE_STATE,
        }
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        match self {
            Self::Hello(h) => {
                w.str8(&h.node)
                    .opt(h.host_token.as_ref(), |w, t| {
                        w.array(t);
                    })
                    .str8(&h.version)
                    .u32(h.protocol)
                    .u32(h.tick_rate)
                    .str8(&h.game)
                    .array(&h.data_hash)
                    .u16(h.listen_port)
                    .u16(h.max_games)
                    .u16(h.max_players_total)
                    // The running games (N22): none until then.
                    .u8(0);
            }
            Self::Load(l) => {
                w.u16(l.games)
                    .u16(l.players)
                    .u16(l.cpu_permille)
                    .u32(l.tick_overruns)
                    .u8(u8::from(l.draining));
            }
            Self::GameStarted { game } | Self::ItemIdsWanted { game } => {
                w.u32(*game);
            }
            Self::StartRefused { game, code } => {
                w.u32(*game).u16(code.0);
            }
            Self::SeatTaken { game, lease } | Self::Left { game, lease } => {
                w.u32(*game).u64(*lease);
            }
            Self::SeatRefused { game, lease, code } => {
                w.u32(*game).u64(*lease).u16(code.0);
            }
            Self::Save(s) => s.encode(&mut w),
            Self::Players { game, players } => {
                w.u32(*game).u8(*players);
            }
            Self::GameClosed { game, reason } => {
                w.u32(*game).u16(reason.0);
            }
            Self::DevAnswer { ok, text } => {
                w.bool(*ok).str8(text);
            }
            Self::CrashReport { game, text } => {
                w.u32(*game).str8(text);
            }
            Self::JournalChunk {
                game,
                seq,
                first_tick,
                ticks,
                last,
            } => {
                w.u32(*game)
                    .u32(*seq)
                    .u64(*first_tick)
                    .u32(*ticks)
                    .bool(*last);
            }
            Self::ReachInfo {
                method,
                mapped_addr,
            } => {
                w.u8(*method).opt(mapped_addr.as_ref(), |w, a| {
                    w.str8(a);
                });
            }
            Self::PunchDone {
                game,
                peer_addr,
                connected,
            } => {
                w.u32(*game).str8(peer_addr).bool(*connected);
            }
            Self::ItemOut(m) => m.encode(&mut w),
            Self::ItemInDone(m) => m.encode(&mut w),
            Self::ItemOutRefused { lease, req, code } => {
                w.u64(*lease).u64(*req).u16(code.0);
            }
            Self::UpdateState {
                version,
                state,
                detail,
            } => {
                w.str8(version).u8(*state).str8(detail);
            }
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut r) = Reader::new(body)?;
        let m = match k {
            kind::NODE_HELLO => {
                let h = NodeHello {
                    node: r.str8(MAX_NODE_ID)?,
                    host_token: r.opt(|r| r.array())?,
                    version: r.str8(MAX_BUILD)?,
                    protocol: r.u32()?,
                    tick_rate: r.u32()?,
                    game: r.str8(MAX_GAME_ID)?,
                    data_hash: r.array()?,
                    listen_port: r.u16()?,
                    max_games: r.u16()?,
                    max_players_total: r.u16()?,
                };
                // Running games come with N22.
                if r.u8()? != 0 {
                    return Err(NetError::Malformed);
                }
                // An official server names itself; a host shows its token.
                let official = !h.node.is_empty() && valid_node_id(&h.node);
                if official == h.host_token.is_some() || (!official && !h.node.is_empty()) {
                    return Err(NetError::Malformed);
                }
                Self::Hello(h)
            }
            kind::LOAD => Self::Load(Load {
                games: r.u16()?,
                players: r.u16()?,
                cpu_permille: r.u16()?,
                tick_overruns: r.u32()?,
                draining: small(&mut r, 1)? == 1,
            }),
            kind::GAME_STARTED => Self::GameStarted { game: r.u32()? },
            kind::START_REFUSED => Self::StartRefused {
                game: r.u32()?,
                code: code(&mut r)?,
            },
            kind::SEAT_TAKEN => Self::SeatTaken {
                game: r.u32()?,
                lease: r.u64()?,
            },
            kind::SEAT_REFUSED => Self::SeatRefused {
                game: r.u32()?,
                lease: r.u64()?,
                code: code(&mut r)?,
            },
            kind::SAVE => Self::Save(SaveInfo::decode(&mut r)?),
            kind::PLAYERS => Self::Players {
                game: r.u32()?,
                players: r.u8()?,
            },
            kind::LEFT => Self::Left {
                game: r.u32()?,
                lease: r.u64()?,
            },
            kind::GAME_CLOSED => Self::GameClosed {
                game: r.u32()?,
                reason: code(&mut r)?,
            },
            kind::DEV_ANSWER => Self::DevAnswer {
                ok: r.bool()?,
                text: r.str8(MAX_DEV_TEXT)?,
            },
            kind::CRASH_REPORT => Self::CrashReport {
                game: r.u32()?,
                text: r.str8(MAX_ERROR_TEXT)?,
            },
            kind::JOURNAL_CHUNK => Self::JournalChunk {
                game: r.u32()?,
                seq: r.u32()?,
                first_tick: r.u64()?,
                ticks: r.u32()?,
                last: r.bool()?,
            },
            kind::ITEM_IDS_WANTED => Self::ItemIdsWanted { game: r.u32()? },
            kind::REACH_INFO => Self::ReachInfo {
                method: small(&mut r, 3)?,
                mapped_addr: r.opt(|r| r.str8(MAX_ADDR))?,
            },
            kind::PUNCH_DONE => Self::PunchDone {
                game: r.u32()?,
                peer_addr: r.str8(MAX_ADDR)?,
                connected: r.bool()?,
            },
            kind::ITEM_OUT => Self::ItemOut(ItemOut::decode(&mut r)?),
            kind::ITEM_IN_DONE => Self::ItemInDone(ItemInDone::decode(&mut r)?),
            kind::ITEM_OUT_REFUSED => Self::ItemOutRefused {
                lease: r.u64()?,
                req: r.u64()?,
                code: code(&mut r)?,
            },
            kind::UPDATE_STATE => Self::UpdateState {
                version: update_version(&mut r)?,
                state: small(&mut r, update_state::RESTARTING)?,
                detail: r.str8(MAX_UPDATE_TEXT)?,
            },
            _ => return Err(NetError::Malformed),
        };
        r.finish()?;
        Ok(m)
    }
}

impl RealmToNode {
    pub fn kind(&self) -> u8 {
        match self {
            Self::Welcome(_) => kind::NODE_WELCOME,
            Self::Refused { .. } => kind::NODE_REFUSED,
            Self::StartGame { .. } => kind::START_GAME,
            Self::SeatOffer { .. } => kind::SEAT_OFFER,
            Self::SaveAck { .. } => kind::SAVE_ACK,
            Self::CloseGame { .. } => kind::CLOSE_GAME,
            Self::Kick { .. } => kind::KICK,
            Self::Drain { .. } => kind::DRAIN,
            Self::DevCommand { .. } => kind::DEV_COMMAND,
            Self::ItemIds { .. } => kind::ITEM_IDS,
            Self::ReachResult { .. } => kind::REACH_RESULT,
            Self::Punch { .. } => kind::PUNCH,
            Self::ItemWanted(_) => kind::ITEM_WANTED,
            Self::ItemOutAck { .. } => kind::ITEM_OUT_ACK,
            Self::ItemIn(_) => kind::ITEM_IN,
            Self::Update { .. } => kind::UPDATE,
        }
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        match self {
            Self::Welcome(v) => {
                w.str8(&v.realm)
                    .str8(&v.node)
                    .u8(v.kind as u8)
                    .str8(&v.seen_addr)
                    .u32(v.heartbeat_ms)
                    .u32(v.orphan_s)
                    .u32(v.journal_chunk_s)
                    .u32(v.save_interval_s)
                    .u8(v.ticket_keys.len().min(usize::from(u8::MAX)) as u8);
                for k in v.ticket_keys.iter().take(usize::from(u8::MAX)) {
                    w.u8(k.key_id).array(&k.public);
                }
            }
            Self::Refused { code, text } => {
                w.u16(code.0).str8(text);
            }
            Self::StartGame {
                spec,
                seed,
                item_ids,
            } => {
                spec.encode(&mut w);
                w.u32(*seed);
                item_ids.encode(&mut w);
            }
            Self::SeatOffer {
                game,
                lease,
                player,
                character,
                new,
                has_record,
                ticket,
            } => {
                w.u32(*game)
                    .u64(*lease)
                    .u64(*player)
                    .str8(character)
                    .opt(new.as_ref(), |w, n| n.encode(w))
                    .bool(*has_record)
                    .opt(ticket.as_ref(), |w, t| {
                        w.array(t);
                    });
            }
            Self::SaveAck { lease, seq, code } => {
                w.u64(*lease).u32(*seq).u16(code.0);
            }
            Self::CloseGame { game, how } => {
                w.u32(*game).u8(*how);
            }
            Self::Kick { game, lease } => {
                w.u32(*game).u64(*lease);
            }
            Self::Drain { on } => {
                w.bool(*on);
            }
            Self::DevCommand { character, command } => {
                w.str8(character).str8(command);
            }
            Self::ItemIds { game, ids } => {
                w.u32(*game);
                ids.encode(&mut w);
            }
            Self::ReachResult {
                reachable,
                checked_addr,
                nat,
            } => {
                w.bool(*reachable).str8(checked_addr).u8(*nat);
            }
            Self::Punch {
                game,
                peer_addr,
                at_ms,
                tries,
            } => {
                w.u32(*game).str8(peer_addr).u64(*at_ms).u8(*tries);
            }
            Self::ItemWanted(m) => m.encode(&mut w),
            Self::ItemOutAck { lease, req, code } => {
                w.u64(*lease).u64(*req).u16(code.0);
            }
            Self::ItemIn(m) => m.encode(&mut w),
            Self::Update {
                phase,
                version,
                seconds,
                text,
            } => {
                w.u8(*phase).str8(version).u32(*seconds).str8(text);
            }
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut r) = Reader::new(body)?;
        let m = match k {
            kind::NODE_WELCOME => {
                let mut v = NodeWelcome {
                    realm: r.str8(MAX_REALM_NAME)?,
                    node: r.str8(MAX_NODE_ID)?,
                    kind: NodeKind::decode(&mut r)?,
                    seen_addr: r.str8(MAX_ADDR)?,
                    heartbeat_ms: r.u32()?,
                    orphan_s: r.u32()?,
                    journal_chunk_s: r.u32()?,
                    save_interval_s: r.u32()?,
                    ticket_keys: Vec::new(),
                };
                for _ in 0..r.u8()? {
                    v.ticket_keys.push(TicketKey {
                        key_id: r.u8()?,
                        public: r.array()?,
                    });
                }
                if v.node.is_empty() || v.heartbeat_ms == 0 {
                    return Err(NetError::Malformed);
                }
                Self::Welcome(v)
            }
            kind::NODE_REFUSED => Self::Refused {
                code: code(&mut r)?,
                text: r.str8(MAX_ERROR_TEXT)?,
            },
            kind::START_GAME => Self::StartGame {
                spec: NodeGameSpec::decode(&mut r)?,
                seed: r.u32()?,
                item_ids: ItemIdBlock::decode(&mut r)?,
            },
            kind::SEAT_OFFER => {
                let m = Self::SeatOffer {
                    game: r.u32()?,
                    lease: r.u64()?,
                    player: r.u64()?,
                    character: r.str8(MAX_CHAR_NAME_WIRE)?,
                    new: r.opt(NewCharInfo::decode)?,
                    has_record: r.bool()?,
                    ticket: r.opt(|r| r.array())?,
                };
                // A seat is a stored character (its record) or a new one.
                if let Self::SeatOffer {
                    character,
                    new,
                    has_record,
                    ..
                } = &m
                {
                    if character.is_empty() || new.is_some() == *has_record {
                        return Err(NetError::Malformed);
                    }
                }
                m
            }
            kind::SAVE_ACK => Self::SaveAck {
                lease: r.u64()?,
                seq: r.u32()?,
                code: code(&mut r)?,
            },
            kind::CLOSE_GAME => Self::CloseGame {
                game: r.u32()?,
                how: small(&mut r, 1)?,
            },
            kind::KICK => Self::Kick {
                game: r.u32()?,
                lease: r.u64()?,
            },
            kind::DRAIN => Self::Drain { on: r.bool()? },
            kind::DEV_COMMAND => Self::DevCommand {
                character: r.str8(MAX_CHAR_NAME_WIRE)?,
                command: r.str8(MAX_DEV_TEXT)?,
            },
            kind::ITEM_IDS => Self::ItemIds {
                game: r.u32()?,
                ids: ItemIdBlock::decode(&mut r)?,
            },
            kind::REACH_RESULT => Self::ReachResult {
                reachable: r.bool()?,
                checked_addr: r.str8(MAX_ADDR)?,
                nat: small(&mut r, 3)?,
            },
            kind::PUNCH => Self::Punch {
                game: r.u32()?,
                peer_addr: r.str8(MAX_ADDR)?,
                at_ms: r.u64()?,
                tries: r.u8()?,
            },
            kind::ITEM_WANTED => Self::ItemWanted(ItemWanted::decode(&mut r)?),
            kind::ITEM_OUT_ACK => Self::ItemOutAck {
                lease: r.u64()?,
                req: r.u64()?,
                code: code(&mut r)?,
            },
            kind::ITEM_IN => Self::ItemIn(ItemIn::decode(&mut r)?),
            kind::UPDATE => Self::Update {
                phase: small(&mut r, update_phase::CATCH_UP)?,
                version: update_version(&mut r)?,
                seconds: r.u32()?,
                text: r.str8(MAX_UPDATE_TEXT)?,
            },
            _ => return Err(NetError::Malformed),
        };
        r.finish()?;
        Ok(m)
    }
}

#[cfg(test)]
#[path = "node_tests.rs"]
mod tests;
