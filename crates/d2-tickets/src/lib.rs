//! Join tickets: how a client proves to a game server that the main
//! server sent it to a seat (multiplayer v2, plan section B; contracts
//! section 11.5).
//!
//! When a player joins a game, the main server first pushes the seat to
//! the game server the game runs on (a seat offer: game, lease, player,
//! character, record), then hands the client a ticket for it. The client
//! opens its own connection to the game server and shows the ticket
//! there. The game server never asks the main server at join time: it
//! checks the ticket against the main server's public keys and the offers
//! it holds.
//!
//! This crate is the B1 design: a ticket is a short body naming the seat
//! (realm, game, game server, lease, player, character, difficulty,
//! build, issue and expiry times, a random nonce) signed with the main
//! server's Ed25519 ticket key ([`body`] has the layout).
//!
//! - **The main server** issues tickets through [`TicketIssue`]
//!   ([`SignedIssuer`]). It holds the ticket keys ([`TicketKey`]) and
//!   gives their public halves ([`TicketKeys`]) to every game server when
//!   it links.
//! - **A game server** checks them through [`TicketCheck`]
//!   ([`SignedChecker`]): a known key, a good signature, its own realm and
//!   id, not expired, a waiting offer for that lease, game and character,
//!   and never used before. A used ticket's nonce is kept in a bounded
//!   [`ReplayCache`] until the ticket could no longer pass anyway.
//! - **Key rotation:** every ticket names its key's id. A game server
//!   accepts up to [`MAX_KEYS`] keys at once, so the main server can
//!   publish a new key, switch to signing with it, then drop the old one.
//!
//! A ticket is good once, for [`TICKET_TTL`] (plus [`CLOCK_SLACK`] for the
//! two machines' clocks), for one character, game and game server. It
//! only travels inside TLS and is not bound to the client's connection or
//! key. It holds no secret beyond itself; it is still never logged beyond
//! its first two bytes ([`ticket_tag`]), and no type here prints key
//! bytes.
//!
//! Every way a ticket fails answers the client with the one code
//! [`ErrorCode::TICKET_INVALID`]; [`TicketError`] says which for the game
//! server's own log and for [`TicketError::retry`] (an offer that hasn't
//! arrived yet).

use std::time::{Duration, SystemTime};

use d2_net::msg::ErrorCode;

pub mod body;
mod check;
mod ids;
mod issue;
mod keys;
mod replay;

pub use body::TicketBody;
pub use check::{OfferedSeat, SignedChecker, TicketError};
pub use ids::{Lease, NodeId, PlayerId};
pub use issue::{IssueError, SignedIssuer};
pub use keys::{KeyError, TicketKey, TicketKeys, TicketPublicKey};
pub use replay::ReplayCache;

#[cfg(test)]
mod abuse;
#[cfg(test)]
mod tests;

/// How long a ticket is good for after the main server issues it.
pub const TICKET_TTL: Duration = Duration::from_secs(30);

/// How far apart the main server's and a game server's clocks may be: a
/// ticket passes until its expiry plus this, and is refused when issued
/// more than this in the game server's future.
pub const CLOCK_SLACK: Duration = Duration::from_secs(30);

/// How long a game server waits for a ticket's seat offer to arrive
/// ([`TicketError::NoOffer`]) before refusing the ticket.
pub const OFFER_WAIT: Duration = Duration::from_secs(5);

/// How long a seat offer waits for its client before the game server
/// gives it up ([`SignedChecker::expire_offers`]; the main server hears
/// `SeatGone`).
pub const OFFER_TTL: Duration = Duration::from_secs(30);

/// How many ticket keys a game server accepts at once: the current one
/// and the next or previous one during a rotation.
pub const MAX_KEYS: usize = 2;

/// What the main server knows about a seat when it issues its ticket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatGrant {
    /// The mod realm (`d2rust`, a mod's id).
    pub realm: String,
    pub game: u32,
    /// The game server the game runs on.
    pub node: NodeId,
    pub lease: Lease,
    pub player: PlayerId,
    pub character: String,
    pub difficulty: u8,
    /// The game's build (version string).
    pub build: String,
}

/// The main server's side: makes the ticket for a seat it has offered.
pub trait TicketIssue: Send + Sync {
    /// The ticket, at most 255 bytes. A grant this issuer can't make a
    /// ticket for gives an empty ticket, which every game server refuses.
    fn issue(&self, g: &SeatGrant) -> Vec<u8>;
}

/// What a game server learns from a good ticket: the seat it is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TicketClaims {
    pub game: u32,
    pub lease: Lease,
    pub character: String,
}

/// The game server's side: checks a ticket a client showed, once.
pub trait TicketCheck: Send {
    /// The seat a good ticket is for; [`ErrorCode::TICKET_INVALID`] for
    /// every ticket that isn't (unknown, forged, used, expired or another
    /// server's).
    fn check(&mut self, ticket: &[u8], now: SystemTime) -> Result<TicketClaims, ErrorCode>;
}

/// A ticket's first two bytes (its version and key id) for logs: the only
/// part of a ticket that may be logged.
pub fn ticket_tag(ticket: &[u8]) -> TicketTag {
    TicketTag([ticket.first().copied(), ticket.get(1).copied()])
}

/// A ticket's loggable part ([`ticket_tag`]); shows as `ticket 01:02..`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TicketTag([Option<u8>; 2]);

impl std::fmt::Display for TicketTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ticket ")?;
        for (i, b) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(":")?;
            }
            match b {
                Some(b) => write!(f, "{b:02x}")?,
                None => f.write_str("--")?,
            }
        }
        f.write_str("..")
    }
}

impl std::fmt::Debug for TicketTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

/// Milliseconds since the Unix epoch; 0 for a clock set before it.
fn unix_ms(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// A duration in whole milliseconds.
fn ms(d: Duration) -> u64 {
    u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
}
