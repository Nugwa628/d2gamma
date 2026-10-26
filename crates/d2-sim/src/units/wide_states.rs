//! A unit's state changes as messages, for every state id a `states`
//! table may have.
//!
//! The state messages (S→C 0xA7 DelayedState, 0xA8 SetState, 0xA9
//! EndState, and 0xAA's state stream) name a state in one byte. D2RUST's
//! own `states` table has at most 255 rows, so its states always fit. A mod
//! game's table may have more (`d2_data::bin::DataRules::Mod`): a state
//! from [`FIRST_WIDE_STATE`] up changes in the wide state message
//! [`WIDE_STATE`] instead, which only a mod game's wire carries
//! (`d2_proto::wide_state`).

use super::messages::{set_state, state_ref, write_entries, SendStat, SentState};
use crate::monsters::init::BitWriter;

/// S→C id of the wide state message: type u8@1, GUID u32@2, size u8@6
/// (the whole message's length), op u8@7 ([`StateChange`]), state u16@8,
/// for [`StateChange::Set`] the entries stream from @10 (0xA8's).
pub const WIDE_STATE: u8 = 0xB7;
/// Bytes of a wide state message before its entries stream.
pub const WIDE_HEADER: usize = 10;
/// The first state id past the state messages' byte (0xFF also ends
/// 0xAA's state stream).
pub const FIRST_WIDE_STATE: u16 = 0xFF;

/// The form of a state change: the op byte of [`WIDE_STATE`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateChange {
    /// 0xA7: the state on, without a stat list.
    Delayed = 0,
    /// 0xA8: the state on, with its list's entries.
    Set = 1,
    /// 0xA9: the state off.
    End = 2,
}

/// `true` when `state` fits the state messages' byte.
pub fn fits_byte(state: u16) -> bool {
    state < FIRST_WIDE_STATE
}

/// The message of `change` to `state` on the unit (`unit_type`, `guid`):
/// 0xA7 / 0xA8 (with `entries`) / 0xA9 for a state that [`fits_byte`],
/// else [`WIDE_STATE`]. `None` for a wide set whose entries stream is past
/// the size byte.
pub fn state_change(
    change: StateChange,
    unit_type: u8,
    guid: u32,
    state: u16,
    entries: &[(u16, u16, i32)],
    stat: impl Fn(u16) -> Option<SendStat>,
) -> Option<Vec<u8>> {
    if fits_byte(state) {
        let s = state as u8;
        return Some(match change {
            StateChange::Delayed => state_ref(0xA7, unit_type, guid, s).to_vec(),
            StateChange::Set => set_state(unit_type, guid, s, entries, stat),
            StateChange::End => state_ref(0xA9, unit_type, guid, s).to_vec(),
        });
    }
    let mut w = BitWriter::new();
    if change == StateChange::Set {
        write_entries(&mut w, entries, &stat);
    }
    let size = u8::try_from(WIDE_HEADER + w.bytes.len()).ok()?;
    let g = guid.to_le_bytes();
    let s = state.to_le_bytes();
    let mut b = vec![
        WIDE_STATE,
        unit_type,
        g[0],
        g[1],
        g[2],
        g[3],
        size,
        change as u8,
        s[0],
        s[1],
    ];
    b.extend(w.bytes);
    Some(b)
}

/// The wide state messages that follow a unit's 0xAA for its states past
/// the stream's byte (0xAA leaves them out): each state on, with its
/// list's entries ([`StateChange::Set`]) or without
/// ([`StateChange::Delayed`]). `states` as for 0xAA.
pub fn wide_unit_states(
    unit_type: u8,
    guid: u32,
    states: &[SentState],
    stat: impl Fn(u16) -> Option<SendStat>,
) -> Vec<Vec<u8>> {
    states
        .iter()
        .filter(|s| !fits_byte(s.state))
        .filter_map(|s| {
            let entries = s.entries.as_deref().unwrap_or_default();
            let change = if entries.is_empty() {
                StateChange::Delayed
            } else {
                StateChange::Set
            };
            state_change(change, unit_type, guid, s.state, entries, &stat)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::messages::unit_states;
    use super::*;

    fn row(_: u16) -> Option<SendStat> {
        Some(SendStat {
            bits: 8,
            param_bits: 0,
            signed: false,
        })
    }

    #[test]
    fn byte_states_keep_their_messages() {
        let e = [(0, 0, 25)];
        for s in [0, 7, 254] {
            let b = s as u8;
            let got = |c| state_change(c, 1, 0x0A0B_0C0D, s, &e, row).unwrap();
            assert_eq!(
                got(StateChange::Delayed),
                state_ref(0xA7, 1, 0x0A0B_0C0D, b)
            );
            assert_eq!(got(StateChange::End), state_ref(0xA9, 1, 0x0A0B_0C0D, b));
            assert_eq!(got(StateChange::Set), set_state(1, 0x0A0B_0C0D, b, &e, row));
        }
    }

    #[test]
    fn wide_states_change_in_the_wide_message() {
        let e = [(0, 0, 25)];
        let end = state_change(StateChange::End, 0, 7, 966, &e, row).unwrap();
        assert_eq!(end, [WIDE_STATE, 0, 7, 0, 0, 0, 10, 2, 0xC6, 0x03]);
        let delayed = state_change(StateChange::Delayed, 1, 7, 255, &[], row).unwrap();
        assert_eq!(delayed, [WIDE_STATE, 1, 7, 0, 0, 0, 10, 0, 0xFF, 0x00]);
        // A set carries 0xA8's stream: stat 0 (9 bits), 25 (8 bits), 0x1FF.
        let set = state_change(StateChange::Set, 1, 7, 300, &e, row).unwrap();
        let byte_set = set_state(1, 7, 44, &e, row);
        assert_eq!(&set[..10], [WIDE_STATE, 1, 7, 0, 0, 0, 14, 1, 0x2C, 0x01]);
        assert_eq!(set[10..], byte_set[8..]);
    }

    #[test]
    fn add_unit_leaves_wide_states_to_the_wide_message() {
        let states = [
            SentState {
                state: 3,
                entries: None,
            },
            SentState {
                state: 400,
                entries: Some(vec![(0, 0, 5)]),
            },
            SentState {
                state: 600,
                entries: None,
            },
        ];
        // 0xAA names state 3 only (as without the wide states).
        assert_eq!(
            unit_states(1, 9, &states, row),
            unit_states(1, 9, &states[..1], row)
        );
        let wide = wide_unit_states(1, 9, &states, row);
        let ops: Vec<(u8, u16)> = wide
            .iter()
            .map(|m| (m[7], u16::from_le_bytes([m[8], m[9]])))
            .collect();
        assert_eq!(ops, [(1, 400), (0, 600)]);
        assert!(wide_unit_states(1, 9, &states[..1], row).is_empty());
    }
}
