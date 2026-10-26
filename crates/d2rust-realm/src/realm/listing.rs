//! The game servers as the lobby shows them, and where a new game runs
//! (lobby contract 11.4, `d2_net::msg::nodelist`).
//!
//! - [`listing`]: the `NodeList` entries of the linked official game
//!   servers, in id order: the name players see ([`display_name`]: the
//!   one the admin set, else the short region it was added with,
//!   "Finland", "US East"; never a machine's name), the location, the address clients ping (the
//!   one players are sent to), the certificate pings and games pin, the
//!   load and whether it takes new games. A server that is lost leaves the
//!   list; player-hosted servers are never in it (a host's address never
//!   reaches the list).
//! - [`realm_pick`]: the main server's own choice, the open server with
//!   the lowest load (then the lower id); the list marks it `suggested`.
//! - [`choose`]: where `CreateGame`'s `Placement` puts the game. A named
//!   server must be linked and open (else `NodeUnavailable`, `NodeFull` or
//!   `NodeDraining`); Automatic with the client's pick (by ping) takes that
//!   server while it is open, else the main server's choice, as plain
//!   Automatic does; Host on my PC is `HostingOff` (the main server takes
//!   no player-hosted games yet).
//! - [`Pushes`]: when the list goes out again to the lobby's sessions: on
//!   a change, at most every [`PUSH_EVERY`].

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use d2_net::msg::node::Load;
use d2_net::msg::nodelist::{
    code, NodeInfo, NodeList, NodeState, Placement, MAX_ADDR, MAX_LOCATION, MAX_NODE_NAME,
};
use d2_net::msg::ErrorCode;
use d2_quic::parse_fingerprint;

use super::nodes::{Linked, NodeEntry};
use super::Realm;

/// The list goes out again at most this often (a change in between waits).
pub const PUSH_EVERY: Duration = Duration::from_secs(5);

/// The address players reach a game server at: its listed public address,
/// or the address its link comes from.
pub fn player_addr(entry: &NodeEntry, addr: SocketAddr) -> String {
    if entry.public_addr.is_empty() {
        addr.to_string()
    } else {
        entry.public_addr.clone()
    }
}

/// How full a game server is, 0-100: the larger of its games and its
/// players against its limits.
pub fn load_pct(entry: &NodeEntry, load: &Load) -> u8 {
    let pct = |n: u16, max: u16| u32::from(n) * 100 / u32::from(max.max(1));
    let p = pct(load.games, entry.max_games).max(pct(load.players, entry.max_players_total));
    u8::try_from(p.min(100)).unwrap_or(100)
}

/// Whether a linked game server takes new games: not while it drains, nor
/// at its game or player limit.
pub fn state(entry: &NodeEntry, load: &Load) -> NodeState {
    if load.draining {
        NodeState::Draining
    } else if load.games >= entry.max_games || load.players >= entry.max_players_total {
        NodeState::Full
    } else {
        NodeState::Open
    }
}

/// The name players see for a game server, in the picker and the game
/// list: the one the admin set (`--node-name`), else the short country or
/// region it was added with (`location`), its `name` only when it has
/// neither, else its id.
pub fn display_name(entry: &NodeEntry) -> String {
    let shown = [&entry.display_name, &entry.location, &entry.name]
        .into_iter()
        .map(|s| fit(s, MAX_NODE_NAME))
        .find(|s| !s.trim().is_empty());
    shown.unwrap_or_else(|| entry.id.clone())
}

/// `s` without control characters, cut to `max` bytes at a character
/// boundary (the client refuses a longer string, and with it the list).
fn fit(s: &str, max: usize) -> String {
    let mut out = String::new();
    for c in s.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > max {
            break;
        }
        out.push(c);
    }
    out
}

/// One linked game server's entry, not yet `suggested`. `None` for a
/// server whose listed fingerprint doesn't parse (it could not have
/// linked).
fn info(entry: &NodeEntry, addr: SocketAddr, load: &Load) -> Option<NodeInfo> {
    let ping_addr = player_addr(entry, addr);
    Some(NodeInfo {
        node: entry.id.clone(),
        name: display_name(entry),
        location: fit(&entry.location, MAX_LOCATION),
        // An address too long for the message can't be pinged.
        ping_addr: if ping_addr.len() <= MAX_ADDR {
            ping_addr
        } else {
            String::new()
        },
        cert_sha256: parse_fingerprint(&entry.cert_sha256)?,
        load_pct: load_pct(entry, load),
        state: state(entry, load),
        suggested: false,
    })
}

/// The list's entries for the linked game servers (`entry`, the address
/// its link comes from, its last load), in the order given; the main
/// server's own choice is marked `suggested`.
pub fn listing<'a>(
    linked: impl IntoIterator<Item = (&'a NodeEntry, SocketAddr, &'a Load)>,
) -> Vec<NodeInfo> {
    let mut list: Vec<NodeInfo> = linked
        .into_iter()
        .filter_map(|(e, addr, load)| info(e, addr, load))
        .collect();
    if let Some(id) = realm_pick(&list).map(|n| n.node.clone()) {
        for n in &mut list {
            n.suggested = n.node == id;
        }
    }
    list
}

/// The main server's choice for a new game: the open server with the
/// lowest load, then the lower id.
pub fn realm_pick(list: &[NodeInfo]) -> Option<&NodeInfo> {
    list.iter()
        .filter(|n| n.open())
        .min_by(|a, b| (a.load_pct, &a.node).cmp(&(b.load_pct, &b.node)))
}

/// The game server `place` puts a new game on, by id, with the list as it
/// is now.
pub fn choose(list: &[NodeInfo], place: &Placement) -> Result<String, ErrorCode> {
    let auto = || {
        realm_pick(list)
            .map(|n| n.node.clone())
            .ok_or(code::NODE_UNAVAILABLE)
    };
    match place {
        Placement::Automatic => auto(),
        Placement::AutomaticPick(id) => match list.iter().find(|n| n.node == *id) {
            Some(n) if n.open() => Ok(n.node.clone()),
            _ => auto(),
        },
        Placement::Official(id) => {
            let n = list
                .iter()
                .find(|n| n.node == *id)
                .ok_or(code::NODE_UNAVAILABLE)?;
            match n.state {
                NodeState::Open => Ok(n.node.clone()),
                NodeState::Full => Err(code::NODE_FULL),
                NodeState::Draining => Err(code::NODE_DRAINING),
                NodeState::Down => Err(code::NODE_UNAVAILABLE),
            }
        }
        Placement::HostOnMyPc => Err(code::HOSTING_OFF),
    }
}

/// When the list goes out to every lobby session again.
#[derive(Debug, Default)]
pub struct Pushes {
    /// What the last push sent (`None`: a session may have another list,
    /// so the next check pushes).
    last: Option<NodeList>,
    /// When it went out.
    at: Option<Instant>,
}

impl Pushes {
    /// Whether `list` goes out now: it differs from the last push, and
    /// [`PUSH_EVERY`] has passed since that one. True marks it sent.
    pub fn due(&mut self, list: &NodeList, now: Instant) -> bool {
        if self.last.as_ref() == Some(list) {
            return false;
        }
        if self
            .at
            .is_some_and(|at| now.saturating_duration_since(at) < PUSH_EVERY)
        {
            return false;
        }
        self.last = Some(list.clone());
        self.at = Some(now);
        true
    }

    /// One session got `list` on its own (entering the lobby): when it
    /// isn't what the last push sent, the next check pushes again, so no
    /// session keeps a list the others no longer have.
    pub fn sent_one(&mut self, list: &NodeList) {
        if self.last.as_ref() != Some(list) {
            self.last = None;
        }
    }
}

impl Realm {
    /// The linked official game servers as the lobby lists them.
    pub fn node_list(&self) -> NodeList {
        let linked = self.nodes.linked_now();
        NodeList(listing(linked.iter().map(|l| (&l.entry, l.addr, &l.load))))
    }

    /// The linked game server a new game with `place` runs on.
    pub fn place(&self, place: &Placement) -> Result<Linked, ErrorCode> {
        let id = choose(&self.node_list().0, place)?;
        self.nodes.get(&id).ok_or(code::NODE_UNAVAILABLE)
    }

    /// The main server's own choice for a new game (see [`realm_pick`]).
    pub fn pick_node(&self) -> Option<Linked> {
        self.place(&Placement::Automatic).ok()
    }
}

#[cfg(test)]
#[path = "listing_tests.rs"]
mod tests;
