//! The punch datagram: the one non-QUIC datagram reachability sends.
//!
//! Everything else in multiplayer is QUIC, the outside check included. A
//! punch only has to leave the game server's socket toward a joiner, so its
//! NAT lets the joiner's QUIC packets in; its contents don't matter to the
//! joiner, whose endpoint drops it. It goes out through the same socket as
//! the game server's QUIC endpoint ([`crate::socket`]).
//!
//! - **Never QUIC.** A punch is 10 bytes; the smallest QUIC packet is 21
//!   (a one-byte header, a 4-byte packet number and the 16-byte sample
//!   header protection needs, RFC 9001 5.4.2), and it starts with `0x00`,
//!   which no QUIC packet does unless its peer greases the fixed bit. So an
//!   endpoint takes a punch off its socket before QUIC by length and magic
//!   alone ([`is_reach`]), whatever QUIC is configured with.
//! - **Layout:** the four magic bytes, the version ([`VERSION`]), the kind
//!   ([`PUNCH`]), then the game id (u32, little-endian): 10 bytes.

/// The first bytes of every reach datagram.
pub const MAGIC: [u8; 4] = [0x00, b'D', b'2', b'R'];

/// Version of the reach datagrams. A change to the layout gets a new one.
pub const VERSION: u8 = 1;

/// The punch's kind byte.
pub const PUNCH: u8 = 5;

/// A punch's length.
pub const PUNCH_LEN: usize = MAGIC.len() + 2 + 4;

/// Whether a datagram that arrived on a shared socket is a reach datagram
/// (and not QUIC): exactly a punch's length, with the magic.
pub fn is_reach(bytes: &[u8]) -> bool {
    bytes.len() == PUNCH_LEN && bytes[..MAGIC.len()] == MAGIC
}

/// A punch for `game`.
pub fn punch(game: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(PUNCH_LEN);
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(PUNCH);
    out.extend_from_slice(&game.to_le_bytes());
    out
}

/// The game a punch names; `None` for anything else.
pub fn read_punch(bytes: &[u8]) -> Option<u32> {
    if !is_reach(bytes) || bytes[4] != VERSION || bytes[5] != PUNCH {
        return None;
    }
    Some(u32::from_le_bytes(bytes[6..].try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for game in [0, 7, u32::MAX] {
            let b = punch(game);
            assert_eq!(b.len(), PUNCH_LEN);
            assert!(is_reach(&b));
            assert_eq!(read_punch(&b), Some(game));
        }
    }

    #[test]
    fn never_looks_like_quic() {
        assert_eq!(punch(1)[0] & 0x40, 0);
        // A QUIC long-header Initial and a short-header packet.
        assert!(!is_reach(&[0xC3, 0, 0, 0, 1, 8, 1, 2, 3]));
        assert!(!is_reach(&[0x41, b'D', b'2', b'R', 1, 5]));
        // Even a greased QUIC packet that starts like a punch is longer.
        let mut greased = punch(1);
        greased.resize(21, 0);
        assert!(!is_reach(&greased));
    }

    #[test]
    fn malformed_is_ignored() {
        let p = punch(3);
        assert_eq!(read_punch(&p[..PUNCH_LEN - 1]), None);
        let mut long = p.clone();
        long.push(0);
        assert_eq!(read_punch(&long), None);
        let mut other_version = p.clone();
        other_version[4] = VERSION + 1;
        assert_eq!(read_punch(&other_version), None);
        let mut other_kind = p;
        other_kind[5] = 1;
        assert_eq!(read_punch(&other_kind), None);
    }
}
