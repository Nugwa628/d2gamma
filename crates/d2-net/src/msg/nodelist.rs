//! The game-server choice of the lobby (multiplayer v2, contracts section
//! 11.4): the main server's list of official game servers ([`NodeList`],
//! kind 0x4C), where a new game runs ([`Placement`], a field of
//! `CreateGame` since wire version 6), the client asking after a game
//! whose server connection broke ([`GameStatus`], kind 0x4D), the error
//! codes of these, and the automatic choice ([`pick_auto`]). Which server
//! a listed game runs on is in `GameInfo`'s tail (`node_id`, `server`,
//! `player_hosted`, `host_rtt_ms`; `super::lobby`).

use super::{ErrorCode, Reader, Writer};
use crate::NetError;

/// The kinds of this module (inside the lobby's range).
pub mod kind {
    /// S→C: the official game servers ([`super::NodeList`]).
    pub const NODE_LIST: u8 = 0x4C;
    /// C→S: after a lost game-server connection ([`super::GameStatus`]).
    pub const GAME_STATUS: u8 = 0x4D;
}

/// The error codes of game servers and hosting a client can be answered
/// with (contracts 11.7).
pub mod code {
    use super::ErrorCode;

    /// No open official server, or the chosen one is unknown or down.
    pub const NODE_UNAVAILABLE: ErrorCode = ErrorCode(72);
    /// The chosen server is at capacity.
    pub const NODE_FULL: ErrorCode = ErrorCode(73);
    /// The chosen server takes no new games.
    pub const NODE_DRAINING: ErrorCode = ErrorCode(74);
    /// The game's server was lost (an official server down, a host's PC
    /// gone): back to the lobby with the last checkpoint.
    pub const NODE_LOST: ErrorCode = ErrorCode(75);
    /// The main server is stopping.
    pub const REALM_UNAVAILABLE: ErrorCode = ErrorCode(77);
    /// The joiner couldn't reach the host's PC.
    pub const HOST_UNREACHABLE: ErrorCode = ErrorCode(78);
    /// Hosting isn't available (the main server's `hosting = off`, or a
    /// build without it).
    pub const HOSTING_OFF: ErrorCode = ErrorCode(79);
}

/// A server's id: official ones 1-15 of `a-z 0-9 -`, a player host's
/// `h-` and 12 hex digits.
pub const MAX_NODE_ID: usize = 15;
/// A server's name as players see it.
pub const MAX_NODE_NAME: usize = 32;
/// Where it is, as players see it ("Helsinki, Finland").
pub const MAX_LOCATION: usize = 32;
/// `host:port`.
pub const MAX_ADDR: usize = 64;
/// The longest list the message carries (`u8 count`).
pub const MAX_NODES: usize = u8::MAX as usize;
/// The automatic choice: a server within this many milliseconds of the
/// best ping counts as as close, and the less loaded of those wins.
pub const AUTO_TIE_MS: u32 = 15;

/// Whether an official server takes new games.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NodeState {
    #[default]
    Open,
    Full,
    Draining,
    Down,
}

impl NodeState {
    fn byte(self) -> u8 {
        match self {
            Self::Open => 0,
            Self::Full => 1,
            Self::Draining => 2,
            Self::Down => 3,
        }
    }

    fn from_byte(b: u8) -> Result<Self, NetError> {
        Ok(match b {
            0 => Self::Open,
            1 => Self::Full,
            2 => Self::Draining,
            3 => Self::Down,
            _ => return Err(NetError::Malformed),
        })
    }
}

/// One official game server of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeInfo {
    pub node: String,
    pub name: String,
    pub location: String,
    /// `host:port` the client pings (a QUIC handshake on its port).
    pub ping_addr: String,
    /// The server's certificate fingerprint: pings and games pin it.
    pub cert_sha256: [u8; 32],
    /// 0-100.
    pub load_pct: u8,
    pub state: NodeState,
    /// The main server's hint, for a client that has no ping.
    pub suggested: bool,
}

impl NodeInfo {
    pub fn encode(&self, w: &mut Writer) {
        w.str8(&self.node)
            .str8(&self.name)
            .str8(&self.location)
            .str8(&self.ping_addr)
            .array(&self.cert_sha256)
            .u8(self.load_pct)
            .u8(self.state.byte())
            .bool(self.suggested);
    }

    pub fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let n = Self {
            node: r.str8(MAX_NODE_ID)?,
            name: r.str8(MAX_NODE_NAME)?,
            location: r.str8(MAX_LOCATION)?,
            ping_addr: r.str8(MAX_ADDR)?,
            cert_sha256: r.array()?,
            load_pct: r.u8()?,
            state: NodeState::from_byte(r.u8()?)?,
            suggested: r.bool()?,
        };
        if n.node.is_empty() || n.load_pct > 100 {
            return Err(NetError::Malformed);
        }
        Ok(n)
    }

    /// The server takes new games.
    pub fn open(&self) -> bool {
        self.state == NodeState::Open
    }
}

/// S→C: the official game servers, after `CharSelected` and on change (at
/// most every 5 s). Player-hosted servers are never listed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NodeList(pub Vec<NodeInfo>);

impl NodeList {
    /// The body; a list longer than [`MAX_NODES`] is cut.
    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(kind::NODE_LIST);
        let n = self.0.len().min(MAX_NODES);
        w.u8(n as u8);
        for node in &self.0[..n] {
            node.encode(&mut w);
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut r) = Reader::new(body)?;
        if k != kind::NODE_LIST {
            return Err(NetError::Malformed);
        }
        let n = r.u8()?;
        let mut list = Vec::with_capacity(n.into());
        for _ in 0..n {
            list.push(NodeInfo::decode(&mut r)?);
        }
        r.finish()?;
        Ok(Self(list))
    }
}

/// Where a new game runs: the tail of `CreateGame` (`u8 where`, then
/// `opt<str8 node>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    /// The best server for the creator: the client's own pick
    /// ([`pick_auto`]) when it has one, else the main server's choice.
    #[default]
    Automatic,
    /// [`Placement::Automatic`] with the client's pick.
    AutomaticPick(String),
    /// The creator chose this official server.
    Official(String),
    /// The creator's own PC (`d2rust-node --host`).
    HostOnMyPc,
}

impl Placement {
    pub fn encode(&self, w: &mut Writer) {
        let put = |w: &mut Writer, s: &String| {
            w.str8(s);
        };
        match self {
            Self::Automatic => w.u8(0).opt(None, put),
            Self::AutomaticPick(n) => w.u8(0).opt(Some(n), put),
            Self::Official(n) => w.u8(1).opt(Some(n), put),
            Self::HostOnMyPc => w.u8(2).opt(None, put),
        };
    }

    pub fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let place = r.u8()?;
        let node = r.opt(|r| r.str8(MAX_NODE_ID))?;
        Ok(match (place, node) {
            (0, None) => Self::Automatic,
            (0, Some(n)) => Self::AutomaticPick(n),
            (1, Some(n)) => Self::Official(n),
            (2, None) => Self::HostOnMyPc,
            _ => return Err(NetError::Malformed),
        })
    }

    /// The official server it names, if any.
    pub fn node(&self) -> Option<&str> {
        match self {
            Self::AutomaticPick(n) | Self::Official(n) => Some(n),
            Self::Automatic | Self::HostOnMyPc => None,
        }
    }
}

/// What the game list says of a player-hosted game (owner rule 9: no other
/// warning).
pub const PLAYER_HOSTED_NOTE: &str = "Player hosted (may close without warning)";

/// C→S: the game-server connection of `game_id` broke; is the game still
/// there? Answered with `JoinNode` (alive, a new ticket) or `ERROR`
/// `GameClosed` / [`code::NODE_LOST`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameStatus {
    pub game_id: u32,
}

impl GameStatus {
    pub fn body(&self) -> Vec<u8> {
        Writer::new(kind::GAME_STATUS).u32(self.game_id).finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut r) = Reader::new(body)?;
        if k != kind::GAME_STATUS {
            return Err(NetError::Malformed);
        }
        let game_id = r.u32()?;
        r.finish()?;
        Ok(Self { game_id })
    }
}

/// The automatic choice for a new game: among the open servers, the one
/// with the lowest ping; of those within [`AUTO_TIE_MS`] of it, the lower
/// `load_pct` (then the lower ping, then the id). With no ping answered,
/// the open server the main server suggests; `None` when there is none
/// (the main server then places the game). `ping` is the client's
/// measured round trip in milliseconds per server id.
pub fn pick_auto<'a>(
    nodes: &'a [NodeInfo],
    ping: &dyn Fn(&str) -> Option<u32>,
) -> Option<&'a NodeInfo> {
    let pinged: Vec<(&NodeInfo, u32)> = nodes
        .iter()
        .filter(|n| n.open())
        .filter_map(|n| ping(&n.node).map(|p| (n, p)))
        .collect();
    let Some(best) = pinged.iter().map(|&(_, p)| p).min() else {
        return nodes.iter().find(|n| n.open() && n.suggested);
    };
    pinged
        .into_iter()
        .filter(|&(_, p)| p <= best.saturating_add(AUTO_TIE_MS))
        .min_by(|(a, pa), (b, pb)| (a.load_pct, pa, &a.node).cmp(&(b.load_pct, pb, &b.node)))
        .map(|(n, _)| n)
}

#[cfg(test)]
#[path = "nodelist_tests.rs"]
mod tests;
