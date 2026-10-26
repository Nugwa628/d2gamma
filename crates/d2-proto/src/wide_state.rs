//! The wide state message: a unit's state change for a state id past the
//! byte the state messages carry.
//!
//! D2RUST's state messages (S→C 0xA7 DelayedState, 0xA8 SetState, 0xA9
//! EndState, 0xAA AddUnit) name a state in one byte, and its own tables
//! have at most 255 states. A mod game's `states` table may have more
//! (`d2_data::bin::DataRules::Mod`): a state whose id doesn't fit a byte
//! (0xFF and up) changes in one [`ID`] message instead, with the state id
//! as a u16:
//!
//! - layout `[ID][type u8][GUID u32][size u8][op u8][state u16][stats]`,
//!   `size` the whole message's length ([`HEADER`] + the stats stream);
//! - `op` is [`Op`]: the 0xA7, 0xA8 or 0xA9 form of the change. Only
//!   [`Op::Set`] has a stats stream (0xA8's: the state list's entries, then
//!   0x1FF).
//!
//! The id is valid only on a mod game's wire ([`Wire::Mod`]). On D2RUST's
//! own wire ([`Wire::Original`]) it is refused like an unknown id, and
//! D2RUST never sends it: its states all fit a byte.

use crate::envelope::Wire;

/// S→C id of the wide state message: one past ViewReady (0xB6).
pub const ID: u8 = 0xB7;
/// Bytes before the stats stream.
pub const HEADER: usize = 10;
/// The first state id that doesn't fit the state messages' byte (0xFF
/// also ends 0xAA's state stream).
pub const FIRST_WIDE: u16 = 0xFF;

/// The state change a wide state message makes: the forms of the byte
/// state messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Op {
    /// 0xA7 DelayedState: the state on, without a stat list.
    Delayed = 0,
    /// 0xA8 SetState: the state on with its stats stream.
    Set = 1,
    /// 0xA9 EndState: the state off.
    End = 2,
}

impl Op {
    /// The op of byte `b`, `None` past [`Op::End`].
    pub fn from_byte(b: u8) -> Option<Op> {
        match b {
            0 => Some(Op::Delayed),
            1 => Some(Op::Set),
            2 => Some(Op::End),
            _ => None,
        }
    }
}

/// `true` when `wire` carries wide state messages.
pub fn valid_on(wire: Wire) -> bool {
    wire == Wire::Mod
}

/// The wide state message of `state` on the unit (`unit_type`, `guid`):
/// [`HEADER`] then `stats` (the stats stream; empty unless [`Op::Set`]).
/// `None` when the whole message is past the size byte (255).
pub fn encode(op: Op, unit_type: u8, guid: u32, state: u16, stats: &[u8]) -> Option<Vec<u8>> {
    let size = u8::try_from(HEADER + stats.len()).ok()?;
    let g = guid.to_le_bytes();
    let s = state.to_le_bytes();
    let mut b = vec![
        ID, unit_type, g[0], g[1], g[2], g[3], size, op as u8, s[0], s[1],
    ];
    b.extend_from_slice(stats);
    Some(b)
}

/// One decoded wide state message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WideState<'a> {
    pub op: Op,
    pub unit_type: u8,
    pub guid: u32,
    pub state: u16,
    /// The stats stream ([`Op::Set`]; empty otherwise).
    pub stats: &'a [u8],
}

/// The wide state message `msg` (one whole message): `None` unless it
/// starts with [`ID`], its size byte is its length, and its op is known.
pub fn decode(msg: &[u8]) -> Option<WideState<'_>> {
    if msg.len() < HEADER || msg[0] != ID || usize::from(msg[6]) != msg.len() {
        return None;
    }
    Some(WideState {
        op: Op::from_byte(msg[7])?,
        unit_type: msg[1],
        guid: u32::from_le_bytes([msg[2], msg[3], msg[4], msg[5]]),
        state: u16::from_le_bytes([msg[8], msg[9]]),
        stats: &msg[HEADER..],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Size;
    use crate::transport::{server_list, server_size, server_size_on, ServerList};

    #[test]
    fn round_trip() {
        let m = encode(Op::Set, 1, 0x0102_0304, 600, &[0xAB, 0xFF, 0x01]).unwrap();
        assert_eq!(m, [ID, 1, 4, 3, 2, 1, 13, 1, 0x58, 0x02, 0xAB, 0xFF, 0x01]);
        let d = decode(&m).unwrap();
        assert_eq!(
            (d.op, d.unit_type, d.guid, d.state),
            (Op::Set, 1, 0x0102_0304, 600)
        );
        assert_eq!(d.stats, [0xAB, 0xFF, 0x01]);
        let end = encode(Op::End, 0, 7, 966, &[]).unwrap();
        assert_eq!(decode(&end).unwrap().op, Op::End);
        // Past the size byte; a bad size byte or op.
        assert!(encode(Op::Set, 0, 0, 300, &[0; 246]).is_none());
        assert!(encode(Op::Set, 0, 0, 300, &[0; 245]).is_some());
        let mut bad = end.clone();
        bad[6] = 11;
        assert!(decode(&bad).is_none());
        bad = end;
        bad[7] = 3;
        assert!(decode(&bad).is_none());
    }

    #[test]
    fn only_a_mod_wire_carries_it() {
        let m = encode(Op::Delayed, 1, 9, 400, &[]).unwrap();
        assert_eq!(server_size(&m), Size::Invalid);
        assert_eq!(server_size_on(Wire::Original, &m), Size::Invalid);
        assert_eq!(server_size_on(Wire::Mod, &m), Size::Bytes(HEADER));
        assert_eq!(server_size_on(Wire::Mod, &m[..6]), Size::Incomplete);
        // A unit message: the game list, in order with the byte forms.
        assert_eq!(server_list(ID), ServerList::Game);
        assert!(valid_on(Wire::Mod) && !valid_on(Wire::Original));
    }
}
