//! The main server's ticket issuer.

use std::time::SystemTime;

use ring::rand::{SecureRandom, SystemRandom};

use crate::body::{TicketBody, NONCE_LEN};
use crate::{
    ms, unix_ms, KeyError, SeatGrant, TicketIssue, TicketKey, TicketKeys, TicketPublicKey,
    TICKET_TTL,
};

/// Why a ticket couldn't be made.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum IssueError {
    #[error("a name in the seat is longer than a ticket holds")]
    TooLong,
    #[error("no random numbers for the ticket's nonce")]
    Random,
}

/// Issues B1 tickets: signs each with the current ticket key.
///
/// Rotation: publish the next key beside the current one
/// ([`with_other`](Self::with_other)) until every game server has it,
/// then sign with it ([`rotate`](Self::rotate), the old key stays
/// published until its last tickets have expired), then drop the old one
/// ([`drop_other`](Self::drop_other)).
#[derive(Debug)]
pub struct SignedIssuer {
    key: TicketKey,
    other: Option<TicketPublicKey>,
    rng: SystemRandom,
}

impl SignedIssuer {
    /// An issuer signing with `key`, publishing only it.
    pub fn new(key: TicketKey) -> Self {
        Self {
            key,
            other: None,
            rng: SystemRandom::new(),
        }
    }

    /// Also publishes `other` (the next key, or the previous one). Its id
    /// must differ from the signing key's.
    pub fn with_other(mut self, other: TicketPublicKey) -> Result<Self, KeyError> {
        if other.id == self.key.id() {
            return Err(KeyError::BadSet);
        }
        self.other = Some(other);
        Ok(self)
    }

    /// Signs with `key` from now on; the old key stays published.
    pub fn rotate(&mut self, key: TicketKey) -> Result<(), KeyError> {
        if key.id() == self.key.id() {
            return Err(KeyError::BadSet);
        }
        self.other = Some(self.key.public());
        self.key = key;
        Ok(())
    }

    /// Stops publishing the other key.
    pub fn drop_other(&mut self) {
        self.other = None;
    }

    /// The id of the key tickets are signed with now.
    pub fn key_id(&self) -> u8 {
        self.key.id()
    }

    /// The keys game servers must accept (for `NodeWelcome`): the signing
    /// key first.
    pub fn public_keys(&self) -> TicketKeys {
        let keys = std::iter::once(self.key.public())
            .chain(self.other)
            .collect();
        TicketKeys::new(keys).expect("two keys with different ids")
    }

    /// The ticket for `g`, issued at `now`.
    pub fn try_issue_at(&self, g: &SeatGrant, now: SystemTime) -> Result<Vec<u8>, IssueError> {
        let mut nonce = [0; NONCE_LEN];
        self.rng.fill(&mut nonce).map_err(|_| IssueError::Random)?;
        let issued_ms = unix_ms(now);
        let body = TicketBody {
            key_id: self.key.id(),
            realm: g.realm.clone(),
            game: g.game,
            node: g.node.clone(),
            lease: g.lease,
            player: g.player,
            character: g.character.clone(),
            difficulty: g.difficulty,
            build: g.build.clone(),
            issued_ms,
            expires_ms: issued_ms.saturating_add(ms(TICKET_TTL)),
            nonce,
        };
        if !body.fits() {
            return Err(IssueError::TooLong);
        }
        let mut ticket = body.encode();
        let sig = self.key.sign(&ticket);
        ticket.extend_from_slice(sig.as_ref());
        Ok(ticket)
    }
}

impl TicketIssue for SignedIssuer {
    fn issue(&self, g: &SeatGrant) -> Vec<u8> {
        self.try_issue_at(g, SystemTime::now()).unwrap_or_default()
    }
}
