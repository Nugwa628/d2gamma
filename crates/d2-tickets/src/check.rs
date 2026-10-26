//! The game server's ticket check.

use std::collections::HashMap;
use std::time::SystemTime;

use d2_net::msg::ErrorCode;

use crate::body::{self, TicketBody};
use crate::{
    ms, unix_ms, Lease, NodeId, PlayerId, ReplayCache, TicketCheck, TicketClaims, TicketKeys,
    CLOCK_SLACK, OFFER_TTL, TICKET_TTL,
};

/// Why a ticket was refused. The client hears
/// [`ErrorCode::TICKET_INVALID`] for each; the game server logs which.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TicketError {
    #[error("not a ticket (length, version or layout)")]
    Malformed,
    #[error("signed with a key this server doesn't hold")]
    UnknownKey,
    #[error("the signature doesn't match")]
    BadSignature,
    #[error("for another realm")]
    WrongRealm,
    #[error("for another game server")]
    WrongNode,
    #[error("its lifetime is not the ticket lifetime")]
    BadLifetime,
    #[error("expired")]
    Expired,
    #[error("issued in this server's future")]
    NotYetValid,
    #[error("used before")]
    Reused,
    #[error("no seat offer for its lease (yet)")]
    NoOffer,
    #[error("its seat offer waited too long")]
    OfferExpired,
    #[error("its seat offer is for another game, player or character")]
    OfferMismatch,
}

impl TicketError {
    /// Whether the same ticket may pass later: its seat offer may still
    /// be on its way, so the game server tries again for up to
    /// [`crate::OFFER_WAIT`].
    pub fn retry(self) -> bool {
        self == Self::NoOffer
    }

    /// What the client is told.
    pub fn code(self) -> ErrorCode {
        ErrorCode::TICKET_INVALID
    }
}

/// A seat the main server offered this game server (from its `SeatOffer`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfferedSeat {
    pub game: u32,
    pub lease: Lease,
    pub player: PlayerId,
    pub character: String,
}

/// Checks B1 tickets on one game server: against the main server's ticket
/// keys, this server's realm and id, the seat offers it holds, and the
/// tickets used before. A good ticket takes its offer.
#[derive(Debug)]
pub struct SignedChecker {
    realm: String,
    node: NodeId,
    keys: TicketKeys,
    offers: HashMap<Lease, (OfferedSeat, u64)>,
    used: ReplayCache,
}

impl SignedChecker {
    /// A checker for game server `node` of realm `realm`, taking tickets
    /// signed with `keys` (from `NodeWelcome`).
    pub fn new(realm: impl Into<String>, node: NodeId, keys: TicketKeys) -> Self {
        Self::with_cache(realm, node, keys, ReplayCache::default())
    }

    /// As [`new`](Self::new), remembering used tickets in `used`.
    pub fn with_cache(
        realm: impl Into<String>,
        node: NodeId,
        keys: TicketKeys,
        used: ReplayCache,
    ) -> Self {
        Self {
            realm: realm.into(),
            node,
            keys,
            offers: HashMap::new(),
            used,
        }
    }

    /// Takes tickets signed with `keys` from now on (a key rotation).
    pub fn set_keys(&mut self, keys: TicketKeys) {
        self.keys = keys;
    }

    /// Holds a seat offer from `now` on; an offer for the same lease is
    /// replaced.
    pub fn offer(&mut self, seat: OfferedSeat, now: SystemTime) {
        self.offers.insert(seat.lease, (seat, unix_ms(now)));
    }

    /// Drops the offer for `lease` (the main server withdrew it).
    pub fn withdraw(&mut self, lease: Lease) -> Option<OfferedSeat> {
        self.offers.remove(&lease).map(|(s, _)| s)
    }

    /// Drops every offer of `game` (it closed).
    pub fn game_closed(&mut self, game: u32) {
        self.offers.retain(|_, (s, _)| s.game != game);
    }

    /// Gives up the offers that waited longer than [`OFFER_TTL`] at `now`
    /// and returns them (the main server hears `SeatGone` for each).
    pub fn expire_offers(&mut self, now: SystemTime) -> Vec<OfferedSeat> {
        let now = unix_ms(now);
        let mut gone = Vec::new();
        self.offers.retain(|_, (s, at)| {
            let keep = !offer_expired(*at, now);
            if !keep {
                gone.push(s.clone());
            }
            keep
        });
        gone.sort_by_key(|s| s.lease);
        gone
    }

    /// How many offers are waiting for their clients.
    pub fn offers_waiting(&self) -> usize {
        self.offers.len()
    }

    /// Checks `ticket` at `now`: the seat it is for, or why it is refused.
    /// A good ticket takes its offer and is remembered as used; a refused
    /// one changes nothing.
    pub fn verify(&mut self, ticket: &[u8], now: SystemTime) -> Result<TicketClaims, TicketError> {
        let (signed, sig) = body::split(ticket).ok_or(TicketError::Malformed)?;
        let key = self.keys.get(signed[1]).ok_or(TicketError::UnknownKey)?;
        if !key.verifies(signed, sig) {
            return Err(TicketError::BadSignature);
        }
        let b = TicketBody::decode(signed).map_err(|_| TicketError::Malformed)?;
        if b.realm != self.realm {
            return Err(TicketError::WrongRealm);
        }
        if b.node != self.node {
            return Err(TicketError::WrongNode);
        }
        if b.expires_ms.checked_sub(b.issued_ms) != Some(ms(TICKET_TTL)) {
            return Err(TicketError::BadLifetime);
        }
        let now_ms = unix_ms(now);
        let slack = ms(CLOCK_SLACK);
        let until = b.expires_ms.saturating_add(slack);
        if now_ms > until {
            return Err(TicketError::Expired);
        }
        if b.issued_ms > now_ms.saturating_add(slack) {
            return Err(TicketError::NotYetValid);
        }
        if self.used.seen(&b.nonce) {
            return Err(TicketError::Reused);
        }
        let (seat, at) = self.offers.get(&b.lease).ok_or(TicketError::NoOffer)?;
        if seat.game != b.game || seat.player != b.player || seat.character != b.character {
            return Err(TicketError::OfferMismatch);
        }
        if offer_expired(*at, now_ms) {
            return Err(TicketError::OfferExpired);
        }
        self.offers.remove(&b.lease);
        self.used.insert(b.nonce, until, now_ms);
        Ok(TicketClaims {
            game: b.game,
            lease: b.lease,
            character: b.character,
        })
    }
}

impl TicketCheck for SignedChecker {
    fn check(&mut self, ticket: &[u8], now: SystemTime) -> Result<TicketClaims, ErrorCode> {
        self.verify(ticket, now).map_err(TicketError::code)
    }
}

/// Whether an offer made at `at_ms` has waited past [`OFFER_TTL`] at
/// `now_ms`.
fn offer_expired(at_ms: u64, now_ms: u64) -> bool {
    now_ms > at_ms.saturating_add(ms(OFFER_TTL))
}
