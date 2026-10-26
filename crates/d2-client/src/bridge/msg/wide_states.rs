//! S→C 0xB7, the wide state message of a mod game's wire
//! (`d2_proto::wide_state`): the change of a state whose id is past the
//! byte of 0xA7 / 0xA8 / 0xA9, run as those run theirs ([`super::states`]).
//! D2RUST's own states all fit the byte; only a mod game with more than
//! 255 states sends it.

use d2_proto::wide_state::{self, Op};

use super::super::bits::BitReader;
use super::super::disguise;
use super::super::dispatch::{HandlerError, Message};
use super::super::world::{ClientWorld, UnitKey};
use super::states::{read_stats, state_off, state_on, StatsEnd};

/// 0xB7: type u8@1, GUID u32@2, size u8@6, op u8@7, state u16@8, the stat
/// stream from @10 (a set's). A unit in S → the delayed state on (0xA7's),
/// the state on with its stats (0xA8's) or the state off (0xA9's).
pub fn wide_state(w: &mut ClientWorld, msg: &Message<'_>) -> Result<(), HandlerError> {
    let m = wide_state::decode(msg.bytes).ok_or(HandlerError::Invalid("0xB7 size byte or op"))?;
    let key = UnitKey::new(m.unit_type, m.guid);
    if !w.units.contains_key(&key) {
        return Ok(());
    }
    match m.op {
        Op::Delayed => {
            state_on(w, msg.inputs, key, m.state)?;
            disguise::hooks(w, msg, key, m.state)
        }
        Op::Set => {
            state_on(w, msg.inputs, key, m.state)?;
            let mut r = BitReader::new(m.stats);
            if let StatsEnd::Fatal(e) = read_stats(w, msg.inputs, key, m.state, &mut r) {
                return Err(e);
            }
            disguise::hooks(w, msg, key, m.state)
        }
        Op::End => state_off(w, msg.inputs, key, m.state),
    }
}
