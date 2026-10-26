//! Rate limits for the multiplayer servers (multiplayer v2, plan N26 and
//! C6; contracts sections 5 and 11.4).
//!
//! - [`Rate`]: how many of something one key may do: a bucket of
//!   [`Rate::burst`] that refills all of it over [`Rate::per`].
//! - [`Limiter`]: one bucket per key (an account, an address, a game
//!   server, a game), holding at most a fixed number of keys so a flood
//!   of new keys can't grow it without bound.
//! - [`AddrKey`]: the key an address is limited under: an IPv4 address,
//!   or the /64 network of an IPv6 one (contracts section 5, v9).
//! - [`AddrGate`]: a shared per-address limiter for the QUIC accept loop,
//!   checked on the incoming connection *before* the handshake (the ping
//!   ALPN is only known after it, and a handshake costs the server a
//!   signature).
//! - [`rates`]: every limit's default, with where it comes from.
//!
//! The bucket is the generic cell rate algorithm: each key keeps one
//! instant, the time its bucket will be full again, so a check is a few
//! additions and no floats. The caller passes the time; nothing here
//! sleeps, spawns or reads a clock except [`AddrGate::admit`]. Plain `std`,
//! so the game server builds on Windows and Linux alike.

use std::time::Duration;

mod addr;
mod bucket;
pub mod rates;

pub use addr::{AddrGate, AddrKey};
pub use bucket::Limiter;

#[cfg(test)]
mod tests;

/// A limit: a bucket of `burst` that refills all of it over `per`, one
/// token every `per / burst`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rate {
    /// How many may be done at once, from a full bucket (at least 1).
    pub burst: u32,
    /// How long an empty bucket takes to fill again.
    pub per: Duration,
}

impl Rate {
    /// `burst` every `per`.
    pub const fn new(burst: u32, per: Duration) -> Self {
        Self { burst, per }
    }

    /// `burst` every `secs` seconds.
    pub const fn per_secs(burst: u32, secs: u64) -> Self {
        Self::new(burst, Duration::from_secs(secs))
    }

    /// The same count with the window divided by 60 (an hour becomes a
    /// minute, a minute a second): the servers' `limits = test` mode.
    pub fn test(self) -> Self {
        Self::new(self.burst, self.per / 60)
    }

    /// The time one token takes to come back (at least 1 ns).
    fn interval(self) -> Duration {
        (self.per / self.burst.max(1)).max(Duration::from_nanos(1))
    }
}

/// A refused request: how long until it would pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refused {
    pub wait: Duration,
}

impl Refused {
    /// Whole seconds to wait, at least 1: an answer's `wait_s`.
    pub fn wait_s(self) -> u32 {
        let s = self.wait.as_secs() + u64::from(self.wait.subsec_nanos() > 0);
        u32::try_from(s.max(1)).unwrap_or(u32::MAX)
    }
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rate limited for {} s", self.wait_s())
    }
}

impl std::error::Error for Refused {}
