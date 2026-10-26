//! The signed ticket's layout (contracts section 11.5, B1).
//!
//! A ticket is a body, then the 64-byte Ed25519 signature of the main
//! server's ticket key over the body:
//!
//! ```text
//! u8 version = 1, u8 key_id, str8 realm, u32 game, str8 node, u64 lease,
//! u64 player, str8 character, u8 difficulty, str8 build, u64 issued_ms,
//! u64 expires_ms, [u8;16] nonce, [u8;64] signature
//! ```
//!
//! Fields use [`d2_net::msg`]'s codec (little-endian, `str8` = a byte
//! length then UTF-8 with no control characters). The string limits keep
//! every ticket at most [`MAX_TICKET`] bytes.

use d2_net::msg::{Reader, Writer};
use d2_net::NetError;

use crate::{Lease, NodeId, PlayerId};

/// The layout version, the ticket's first byte.
pub const VERSION: u8 = 1;
/// Longest realm name.
pub const MAX_REALM: usize = 16;
/// Longest game server id.
pub const MAX_NODE: usize = 15;
/// Longest character name.
pub const MAX_CHARACTER: usize = 15;
/// Longest build string.
pub const MAX_BUILD: usize = 21;
/// Bytes of the random nonce.
pub const NONCE_LEN: usize = 16;
/// Bytes of the signature at the end.
pub const SIGNATURE_LEN: usize = 64;
/// Bytes of a body with every string empty.
const FIXED_BODY: usize = 1 + 1 + 1 + 4 + 1 + 8 + 8 + 1 + 1 + 1 + 8 + 8 + NONCE_LEN;
/// Longest ticket: every string at its limit.
pub const MAX_TICKET: usize =
    FIXED_BODY + MAX_REALM + MAX_NODE + MAX_CHARACTER + MAX_BUILD + SIGNATURE_LEN;
/// Shortest ticket: every string empty.
pub const MIN_TICKET: usize = FIXED_BODY + SIGNATURE_LEN;

/// A ticket's body: everything the signature covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TicketBody {
    pub key_id: u8,
    pub realm: String,
    pub game: u32,
    pub node: NodeId,
    pub lease: Lease,
    pub player: PlayerId,
    pub character: String,
    pub difficulty: u8,
    pub build: String,
    pub issued_ms: u64,
    pub expires_ms: u64,
    pub nonce: [u8; NONCE_LEN],
}

impl TicketBody {
    /// Whether every string fits its limit (a body that doesn't can't be
    /// encoded without cutting a field).
    pub fn fits(&self) -> bool {
        self.realm.len() <= MAX_REALM
            && self.node.0.len() <= MAX_NODE
            && self.character.len() <= MAX_CHARACTER
            && self.build.len() <= MAX_BUILD
    }

    /// The body's bytes, the part the signature covers. The strings must
    /// [`fit`](Self::fits); a longer one is cut and the ticket refused.
    pub fn encode(&self) -> Vec<u8> {
        Writer::new(VERSION)
            .u8(self.key_id)
            .str8(&self.realm)
            .u32(self.game)
            .str8(&self.node.0)
            .u64(self.lease.0)
            .u64(self.player.0)
            .str8(&self.character)
            .u8(self.difficulty)
            .str8(&self.build)
            .u64(self.issued_ms)
            .u64(self.expires_ms)
            .array(&self.nonce)
            .finish()
    }

    /// Reads a body (without its signature).
    pub fn decode(body: &[u8]) -> Result<Self, NetError> {
        let (version, mut r) = Reader::new(body)?;
        if version != VERSION {
            return Err(NetError::Malformed);
        }
        let b = TicketBody {
            key_id: r.u8()?,
            realm: r.str8(MAX_REALM)?,
            game: r.u32()?,
            node: NodeId(r.str8(MAX_NODE)?),
            lease: Lease(r.u64()?),
            player: PlayerId(r.u64()?),
            character: r.str8(MAX_CHARACTER)?,
            difficulty: r.u8()?,
            build: r.str8(MAX_BUILD)?,
            issued_ms: r.u64()?,
            expires_ms: r.u64()?,
            nonce: r.array()?,
        };
        r.finish()?;
        Ok(b)
    }
}

/// Splits a ticket into its body and signature, after checking its length
/// and version; the body is not decoded yet.
pub(crate) fn split(ticket: &[u8]) -> Option<(&[u8], &[u8])> {
    if !(MIN_TICKET..=MAX_TICKET).contains(&ticket.len()) || ticket[0] != VERSION {
        return None;
    }
    Some(ticket.split_at(ticket.len() - SIGNATURE_LEN))
}
