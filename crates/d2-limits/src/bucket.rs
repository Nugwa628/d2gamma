//! One token bucket per key.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::{Rate, Refused};

/// One bucket of [`Rate`] per key, for at most a fixed number of keys.
///
/// Each key keeps the instant its bucket is full again; a key whose
/// bucket is full is the same as a key never seen, so it is forgotten at
/// the next check. When the limiter holds its most keys and a new one
/// comes, the key closest to a full bucket is forgotten first: a flood of
/// new keys costs a bounded table, and at worst gives that one key a full
/// bucket a little early.
#[derive(Debug)]
pub struct Limiter<K> {
    rate: Rate,
    cap: usize,
    full_at: BTreeMap<K, Instant>,
    order: BTreeSet<(Instant, K)>,
}

impl<K: Ord + Clone> Limiter<K> {
    /// Keys held by default: far above the accounts, addresses or games
    /// active within one window.
    pub const DEFAULT_KEYS: usize = 65_536;

    /// An empty limiter of `rate` holding [`DEFAULT_KEYS`](Self::DEFAULT_KEYS).
    pub fn new(rate: Rate) -> Self {
        Self::with_keys(rate, Self::DEFAULT_KEYS)
    }

    /// An empty limiter of `rate` holding at most `keys` keys (at least 1).
    pub fn with_keys(rate: Rate, keys: usize) -> Self {
        Self {
            rate,
            cap: keys.max(1),
            full_at: BTreeMap::new(),
            order: BTreeSet::new(),
        }
    }

    pub fn rate(&self) -> Rate {
        self.rate
    }

    /// Takes one token of `key`'s bucket at `now`.
    pub fn check(&mut self, key: &K, now: Instant) -> Result<(), Refused> {
        self.check_n(key, 1, now)
    }

    /// Takes `n` tokens of `key`'s bucket at `now` (bytes, for a byte
    /// limit). A refused request takes nothing. More than the burst never
    /// passes; its `wait` is then a whole window.
    pub fn check_n(&mut self, key: &K, n: u32, now: Instant) -> Result<(), Refused> {
        self.prune(now);
        if n > self.rate.burst.max(1) {
            return Err(Refused {
                wait: self.rate.per,
            });
        }
        let start = self.full_at.get(key).map_or(now, |&t| t.max(now));
        let Some(cost) = self.rate.interval().checked_mul(n) else {
            return Err(Refused {
                wait: self.rate.per,
            });
        };
        let after = start + cost;
        let limit = now + self.rate.per;
        if after > limit {
            return Err(Refused {
                wait: after - limit,
            });
        }
        self.set(key, after);
        Ok(())
    }

    /// How long until `key` may take one token at `now` (zero when it may
    /// now), without taking it.
    pub fn wait(&self, key: &K, now: Instant) -> Duration {
        let Some(&t) = self.full_at.get(key) else {
            return Duration::ZERO;
        };
        (t + self.rate.interval()).saturating_duration_since(now + self.rate.per)
    }

    /// Forgets `key`: its bucket is full again.
    pub fn forget(&mut self, key: &K) {
        if let Some(t) = self.full_at.remove(key) {
            self.order.remove(&(t, key.clone()));
        }
    }

    /// Forgets every key whose bucket is full at `now`.
    pub fn prune(&mut self, now: Instant) {
        while let Some((t, _)) = self.order.first() {
            if *t > now {
                break;
            }
            if let Some((_, k)) = self.order.pop_first() {
                self.full_at.remove(&k);
            }
        }
    }

    /// How many keys are held (those with a bucket not yet full).
    pub fn len(&self) -> usize {
        self.full_at.len()
    }

    pub fn is_empty(&self) -> bool {
        self.full_at.is_empty()
    }

    fn set(&mut self, key: &K, full_at: Instant) {
        match self.full_at.insert(key.clone(), full_at) {
            Some(old) => {
                self.order.remove(&(old, key.clone()));
            }
            None => {
                while self.full_at.len() > self.cap {
                    let Some((_, k)) = self.order.pop_first() else {
                        break;
                    };
                    self.full_at.remove(&k);
                }
            }
        }
        self.order.insert((full_at, key.clone()));
    }
}
