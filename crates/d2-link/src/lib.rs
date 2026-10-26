//! `d2-link`: the game-server link (multiplayer v2, contracts 11.2), the
//! part both server programs share. A game server (`d2rust-node`) connects
//! out to its main server (`d2rust-realm`) over QUIC with ALPN
//! [`d2_quic::NODE_ALPN`]; both ends pin the other's certificate.
//!
//! - [`control`]: the control stream as typed halves: [`LinkTx`] sends
//!   (the main server's [`NodeLink`], the game server's [`RealmLink`]),
//!   [`LinkRx`] receives.
//! - [`streams`]: the record and journal streams' header and limits.
//! - [`Versions`]: what must match for a game server to link: version,
//!   protocol, tick rate, game and game-data hash.
//! - [`bundle`]: the small file an official game server is given when it
//!   is added (`d2rust-realm --add-node`).
//! - [`Lease`], [`NodeId`], [`PlayerId`]: the realm's ids, from
//!   `d2-tickets` (a seat's saves and its join ticket name its lease).
//! - [`welcome_keys`], [`ticket_keys`]: the join tickets' public keys as
//!   the welcome carries them.
//!
//! Timing: a game server sends `Load` every [`HEARTBEAT`]; the main
//! server counts it lost after [`LOST_AFTER`] without one.

use std::time::Duration;

use d2_net::msg::node::{NodeHello, NodeToRealm, RealmToNode};
use d2_net::msg::ErrorCode;
use d2_net::NetError;

pub mod bundle;
pub mod control;
pub mod streams;

pub use bundle::Bundle;
pub use control::{start, LinkRx, LinkTx, Message};

/// How often a game server sends `Load`.
pub const HEARTBEAT: Duration = Duration::from_secs(2);

/// How long the main server waits for a `Load` before the game server is
/// lost.
pub const LOST_AFTER: Duration = Duration::from_secs(10);

/// How long the main server waits for a game server's `NodeHello`, and a
/// game server for the answer.
pub const HELLO_WAIT: Duration = Duration::from_secs(10);

/// The realm's ids (contracts 11.1), shared with the join tickets:
///
/// - [`Lease`]: a character's seat in one game on one game server
///   (contracts 11.6). The main server grants one when it offers the seat
///   and stores that seat's saves only while it lasts. Never reused.
/// - [`NodeId`]: a game server's id.
/// - [`PlayerId`]: a player's opaque id inside one game.
pub use d2_tickets::{Lease, NodeId, PlayerId};

/// The ticket keys as `NodeWelcome` (and a bundle) carry them: the public
/// halves only.
pub fn welcome_keys(keys: &d2_tickets::TicketKeys) -> Vec<d2_net::msg::node::TicketKey> {
    keys.iter()
        .map(|k| d2_net::msg::node::TicketKey {
            key_id: k.id,
            public: k.bytes,
        })
        .collect()
}

/// The ticket keys of a `NodeWelcome`, for the game server's check: at
/// most [`d2_tickets::MAX_KEYS`], each id once.
pub fn ticket_keys(
    keys: &[d2_net::msg::node::TicketKey],
) -> Result<d2_tickets::TicketKeys, d2_tickets::KeyError> {
    d2_tickets::TicketKeys::new(
        keys.iter()
            .map(|k| d2_tickets::TicketPublicKey {
                id: k.key_id,
                bytes: k.public,
            })
            .collect(),
    )
}

/// The main server's handle on one game server.
pub trait NodeLink: Send + Sync + 'static {
    fn send(&self, m: RealmToNode) -> Result<(), NetError>;
}

/// The game server's handle on its main server.
pub trait RealmLink: Send + Sync + 'static {
    fn send(&self, m: NodeToRealm) -> Result<(), NetError>;
}

/// What a game server and its main server must agree on (compatibility
/// only: games and their audits need the same program and data).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Versions {
    pub version: String,
    pub protocol: u32,
    pub tick_rate: u32,
    /// The mod realm (`d2rust` for the plain game).
    pub game: String,
    pub data_hash: [u8; 32],
}

/// A refused link: its code (contracts 11.7) and words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: ErrorCode,
    pub text: String,
}

impl Refusal {
    pub fn new(code: ErrorCode, text: impl Into<String>) -> Self {
        Self {
            code,
            text: text.into(),
        }
    }
}

impl Versions {
    /// Whether a game server's hello matches this main server:
    /// `NodeBuild` for another version, protocol or tick rate, `NodeGame`
    /// for another mod realm, `NodeData` for other game data.
    pub fn check(&self, hello: &NodeHello) -> Result<(), Refusal> {
        let build = |what: &str, theirs: String, ours: String| {
            Refusal::new(
                ErrorCode::NODE_BUILD,
                format!("{what} {theirs}; the main server runs {ours}"),
            )
        };
        // A game server on another version takes no players until it runs
        // the main server's: one with an updater fetches it by itself
        // (`docs/plans/server/upgrades.md`), else its admin upgrades it.
        if hello.version != self.version {
            return Err(Refusal::new(
                ErrorCode::NODE_BUILD,
                format!(
                    "game server version {}, main server {}: it links once it runs {} \
                     (its updater fetches it, or: server-ctl upgrade {} on its box)",
                    hello.version, self.version, self.version, self.version
                ),
            ));
        }
        if hello.protocol != self.protocol {
            return Err(build(
                "protocol",
                hello.protocol.to_string(),
                self.protocol.to_string(),
            ));
        }
        if hello.tick_rate != self.tick_rate {
            return Err(build(
                "tick rate",
                hello.tick_rate.to_string(),
                self.tick_rate.to_string(),
            ));
        }
        if hello.game != self.game {
            return Err(Refusal::new(
                ErrorCode::NODE_GAME,
                format!("game {}; the main server runs {}", hello.game, self.game),
            ));
        }
        if hello.data_hash != self.data_hash {
            return Err(Refusal::new(
                ErrorCode::NODE_DATA,
                "other game data than the main server's",
            ));
        }
        Ok(())
    }

    /// An official game server's hello with these versions.
    pub fn hello(
        &self,
        node: &str,
        listen_port: u16,
        max_games: u16,
        max_players_total: u16,
    ) -> NodeHello {
        NodeHello {
            node: node.to_owned(),
            host_token: None,
            version: self.version.clone(),
            protocol: self.protocol,
            tick_rate: self.tick_rate,
            game: self.game.clone(),
            data_hash: self.data_hash,
            listen_port,
            max_games,
            max_players_total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ticket keys survive the welcome's form; a bad set is refused.
    #[test]
    fn ticket_keys_travel_in_the_welcome() {
        let a =
            d2_tickets::TicketKey::from_pkcs8(1, &d2_tickets::TicketKey::generate_pkcs8().unwrap())
                .unwrap();
        let keys = d2_tickets::TicketKeys::new(vec![a.public()]).unwrap();
        let w = welcome_keys(&keys);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].key_id, 1);
        assert_eq!(ticket_keys(&w).unwrap(), keys);
        let twice = [w[0], w[0]];
        assert!(ticket_keys(&twice).is_err());
    }

    fn ours() -> Versions {
        Versions {
            version: "0.1.2.0".into(),
            protocol: 4,
            tick_rate: 25,
            game: "d2rust".into(),
            data_hash: [1; 32],
        }
    }

    #[test]
    fn each_mismatch_has_its_code() {
        let v = ours();
        let hello = v.hello("eu-1", 6113, 16, 128);
        assert_eq!(v.check(&hello), Ok(()));
        let code = |h: NodeHello| v.check(&h).unwrap_err().code;
        let other = |f: &dyn Fn(&mut NodeHello)| {
            let mut h = hello.clone();
            f(&mut h);
            h
        };
        assert_eq!(
            code(other(&|h| h.version = "0.1.3.0".into())),
            ErrorCode::NODE_BUILD
        );
        assert_eq!(code(other(&|h| h.protocol = 5)), ErrorCode::NODE_BUILD);
        assert_eq!(code(other(&|h| h.tick_rate = 30)), ErrorCode::NODE_BUILD);
        assert_eq!(
            code(other(&|h| h.game = "mod-x".into())),
            ErrorCode::NODE_GAME
        );
        assert_eq!(
            code(other(&|h| h.data_hash = [2; 32])),
            ErrorCode::NODE_DATA
        );
        let text = v
            .check(&other(&|h| h.version = "0.1.3.0".into()))
            .unwrap_err()
            .text;
        assert!(
            text.contains("0.1.3.0") && text.contains("0.1.2.0"),
            "{text}"
        );
        // The refusal says what to do, and fits the refusal's text even
        // with long versions.
        assert!(text.contains("server-ctl upgrade 0.1.2.0"), "{text}");
        let long = Versions {
            version: "0.1.2.0.12000".into(),
            ..ours()
        };
        let text = long
            .check(&other(&|h| h.version = "0.1.3.0.12001".into()))
            .unwrap_err()
            .text;
        assert!(text.len() <= d2_net::msg::MAX_ERROR_TEXT, "{text}");
    }
}
