//! Who hears the market's pushes (contract sections 4 and 5): every
//! session of an account whose selected character is in the economy the
//! push is about. A session takes a seat when its character is selected
//! (or it speaks to the market) and leaves it when the session ends or the
//! character goes.

use std::collections::BTreeMap;
use std::sync::Mutex;

use d2_economy::EconomyKey;

use crate::session::{ControlTx, SessionId};

/// One session's seat: its account, its character's economy and its
/// control stream.
#[derive(Clone, Debug)]
pub struct Seat {
    pub account: i64,
    pub key: EconomyKey,
    pub out: ControlTx,
}

/// Every seated session.
#[derive(Debug, Default)]
pub struct Seats {
    inner: Mutex<BTreeMap<SessionId, Seat>>,
}

impl Seats {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<SessionId, Seat>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Seats (or moves) a session.
    pub fn seat(&self, session: SessionId, seat: Seat) {
        self.lock().insert(session, seat);
    }

    /// The session leaves its seat.
    pub fn leave(&self, session: SessionId) {
        self.lock().remove(&session);
    }

    /// The seats of `account` in the split `split` (`sc-l0-exp`).
    pub fn of(&self, account: i64, split: &str) -> Vec<Seat> {
        self.lock()
            .values()
            .filter(|s| s.account == account && s.key.split() == split && !s.out.is_closed())
            .cloned()
            .collect()
    }

    /// Sends `body` to every seat of `account` in `split`; how many got it.
    pub fn send(&self, account: i64, split: &str, body: &[u8]) -> usize {
        let seats = self.of(account, split);
        for s in &seats {
            s.out.send(body.to_vec());
        }
        seats.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::ControlOut;

    fn key(hardcore: bool) -> EconomyKey {
        EconomyKey {
            hardcore,
            ..EconomyKey::new("d2rust")
        }
    }

    #[test]
    fn notice_reaches_same_economy_only() {
        let seats = Seats::default();
        let (a_sc, mut rx_sc) = ControlTx::new(1);
        let (a_hc, mut rx_hc) = ControlTx::new(2);
        let (b_sc, mut rx_other) = ControlTx::new(3);
        seats.seat(
            1,
            Seat {
                account: 7,
                key: key(false),
                out: a_sc,
            },
        );
        seats.seat(
            2,
            Seat {
                account: 7,
                key: key(true),
                out: a_hc,
            },
        );
        seats.seat(
            3,
            Seat {
                account: 8,
                key: key(false),
                out: b_sc,
            },
        );
        assert_eq!(seats.send(7, "sc-l0-exp", b"notice"), 1);
        assert!(matches!(rx_sc.try_recv(), Ok(ControlOut::Body(b)) if b == b"notice"));
        assert!(rx_hc.try_recv().is_err(), "the hardcore session heard it");
        assert!(rx_other.try_recv().is_err(), "another account heard it");
        seats.leave(1);
        assert_eq!(seats.send(7, "sc-l0-exp", b"again"), 0);
    }
}
