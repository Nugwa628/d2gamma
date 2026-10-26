//! The game server's memory of used tickets.

use std::collections::{BTreeSet, HashMap};

use crate::body::NONCE_LEN;

/// The nonces of tickets already used, each kept until the time its
/// ticket would be refused anyway (its expiry plus the clock slack).
///
/// It holds at most its capacity. When full, it forgets the nonces that
/// are past their time, then the ones closest to it. A forgotten ticket
/// still can't be used twice: its seat offer was taken by its first use,
/// and leases are never reused.
#[derive(Debug)]
pub struct ReplayCache {
    cap: usize,
    until: HashMap<[u8; NONCE_LEN], u64>,
    order: BTreeSet<(u64, [u8; NONCE_LEN])>,
}

impl ReplayCache {
    /// A game server's default capacity: far above the joins that fit in a
    /// ticket's life.
    pub const DEFAULT_CAPACITY: usize = 4096;

    /// An empty cache holding at most `cap` nonces (at least one).
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            until: HashMap::new(),
            order: BTreeSet::new(),
        }
    }

    /// Whether `nonce` was used and is still remembered.
    pub fn seen(&self, nonce: &[u8; NONCE_LEN]) -> bool {
        self.until.contains_key(nonce)
    }

    /// Remembers `nonce` until `until_ms`, forgetting what is past
    /// `now_ms` first.
    pub fn insert(&mut self, nonce: [u8; NONCE_LEN], until_ms: u64, now_ms: u64) {
        self.prune(now_ms);
        while self.until.len() >= self.cap {
            let Some(first) = self.order.pop_first() else {
                break;
            };
            self.until.remove(&first.1);
        }
        if let Some(old) = self.until.insert(nonce, until_ms) {
            self.order.remove(&(old, nonce));
        }
        self.order.insert((until_ms, nonce));
    }

    /// Forgets every nonce whose time is before `now_ms`.
    pub fn prune(&mut self, now_ms: u64) {
        while let Some(&(until, nonce)) = self.order.first() {
            if until >= now_ms {
                break;
            }
            self.order.pop_first();
            self.until.remove(&nonce);
        }
    }

    pub fn len(&self) -> usize {
        self.until.len()
    }

    pub fn is_empty(&self) -> bool {
        self.until.is_empty()
    }
}

impl Default for ReplayCache {
    fn default() -> Self {
        Self::new(Self::DEFAULT_CAPACITY)
    }
}
