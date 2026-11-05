//! Deterministic network traffic budgets, measured in simulation frames.
use d2_sim::units::UnitId;
use std::collections::BTreeMap;
// Fixed engine ceilings, independent of untrusted contracts.
pub const MAX_PAYLOAD: usize = 505;
pub const CLIENT_RATE: u32 = 64;
pub const BYTE_RATE: u32 = 4096;
pub const MESSAGE_RATE: u32 = 8;
#[derive(Default)]
pub struct Budgets {
    incoming: BTreeMap<UnitId, Client>,
    outgoing: BTreeMap<UnitId, Client>,
}
#[derive(Default)]
struct Client {
    messages: Option<Bucket>,
    bytes: Option<Bucket>,
    ids: BTreeMap<u16, Bucket>,
}
struct Bucket {
    credit: u64,
    frame: i32,
}
impl Bucket {
    fn available(&mut self, frame: i32, hz: u32, rate: u32, burst: u32, cost: u32) -> bool {
        let elapsed = frame.wrapping_sub(self.frame);
        if elapsed > 0 {
            self.credit = self
                .credit
                .saturating_add(elapsed as u64 * u64::from(rate))
                .min(u64::from(burst) * u64::from(hz));
            self.frame = frame;
        }
        self.credit >= u64::from(cost) * u64::from(hz)
    }
    fn charge(&mut self, hz: u32, cost: u32) {
        self.credit -= u64::from(cost) * u64::from(hz);
    }
}
impl Client {
    fn allow(&mut self, frame: i32, hz: u32, len: usize, id: Option<u16>, outgoing: bool) -> bool {
        let (rate, bytes) = if outgoing {
            (128, 8192)
        } else {
            (CLIENT_RATE, BYTE_RATE)
        };
        let messages = self.messages.get_or_insert(Bucket {
            credit: u64::from(rate) * u64::from(hz),
            frame,
        });
        let byte = self.bytes.get_or_insert(Bucket {
            credit: u64::from(bytes) * u64::from(hz),
            frame,
        });
        let item = id.map(|id| {
            self.ids.entry(id).or_insert(Bucket {
                credit: u64::from(MESSAGE_RATE) * u64::from(hz),
                frame,
            })
        });
        if !messages.available(frame, hz, rate, rate, 1)
            || !byte.available(frame, hz, bytes, bytes, len as u32)
        {
            return false;
        }
        if let Some(item) = item {
            if !item.available(frame, hz, MESSAGE_RATE, MESSAGE_RATE, 1) {
                return false;
            }
            item.charge(hz, 1);
        }
        messages.charge(hz, 1);
        byte.charge(hz, len as u32);
        true
    }
}
impl Budgets {
    pub fn incoming(
        &mut self,
        p: UnitId,
        frame: i32,
        hz: u32,
        len: usize,
        id: Option<u16>,
    ) -> bool {
        len <= MAX_PAYLOAD
            && self
                .incoming
                .entry(p)
                .or_default()
                .allow(frame, hz, len + 3, id, false)
    }
    pub fn outgoing(&mut self, p: UnitId, frame: i32, hz: u32, len: usize, id: u16) -> bool {
        len <= MAX_PAYLOAD
            && self
                .outgoing
                .entry(p)
                .or_default()
                .allow(frame, hz, len, Some(id), true)
    }
    pub fn forget(&mut self, p: UnitId) {
        self.incoming.remove(&p);
        self.outgoing.remove(&p);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn network_limits_are_per_message_per_player_and_frame_rate_independent() {
        for hz in [25, 60] {
            let mut b = Budgets::default();
            let p = UnitId(1);
            for _ in 0..MESSAGE_RATE {
                assert!(b.incoming(p, 0, hz, 8, Some(1)));
            }
            assert!(!b.incoming(p, 0, hz, 8, Some(1)));
            assert!(b.incoming(p, 0, hz, 8, Some(2)));
            assert!(b.incoming(UnitId(2), 0, hz, 8, Some(1)));
            assert!(b.incoming(p, hz as i32, hz, 8, Some(1)));
            assert!(!b.incoming(p, hz as i32, hz, MAX_PAYLOAD + 1, None));
            b.forget(p);
            assert!(b.incoming(p, 0, hz, 8, Some(1)));
        }
    }
    #[test]
    fn network_client_message_and_byte_limits_apply_to_unknown_ids() {
        let mut b = Budgets::default();
        for _ in 0..CLIENT_RATE {
            assert!(b.incoming(UnitId(1), 0, 25, 8, None));
        }
        assert!(!b.incoming(UnitId(1), 0, 25, 8, None));
        assert!(b.incoming(UnitId(1), 25, 25, 8, None));
        let mut b = Budgets::default();
        for _ in 0..8 {
            assert!(b.incoming(UnitId(1), 0, 25, 505, None));
        }
        assert!(!b.incoming(UnitId(1), 0, 25, 505, None));
    }
    #[test]
    fn network_outbound_limits_are_independent_and_handle_frame_wrap() {
        let mut b = Budgets::default();
        for id in 1..=128 {
            assert!(b.outgoing(UnitId(1), i32::MAX, 25, 12, id));
        }
        assert!(!b.outgoing(UnitId(1), i32::MAX, 25, 12, 129));
        assert!(b.outgoing(UnitId(1), i32::MIN + 25, 25, 12, 129));
    }
}
