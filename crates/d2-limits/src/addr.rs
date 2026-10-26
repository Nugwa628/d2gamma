//! Limits per address, and the gate the QUIC accept loop checks before a
//! handshake.

use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::sync::Mutex;
use std::time::Instant;

use crate::{Limiter, Rate, Refused};

/// The key an address is limited under: an IPv4 address (an IPv4-mapped
/// IPv6 address counts as its IPv4 one), or the /64 network of an IPv6
/// address, since one home connection usually holds a whole /64. The
/// port never counts (contracts section 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AddrKey(IpAddr);

impl AddrKey {
    pub fn of(ip: IpAddr) -> Self {
        match ip {
            IpAddr::V4(_) => Self(ip),
            IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
                Some(v4) => Self(IpAddr::V4(v4)),
                None => {
                    let s = v6.segments();
                    Self(IpAddr::V6(Ipv6Addr::new(
                        s[0], s[1], s[2], s[3], 0, 0, 0, 0,
                    )))
                }
            },
        }
    }

    /// The address (or network) the key stands for.
    pub fn ip(self) -> IpAddr {
        self.0
    }
}

impl From<IpAddr> for AddrKey {
    fn from(ip: IpAddr) -> Self {
        Self::of(ip)
    }
}

impl From<SocketAddr> for AddrKey {
    fn from(a: SocketAddr) -> Self {
        Self::of(a.ip())
    }
}

/// A per-address limit shared by the tasks of one endpoint: the QUIC
/// accept loop checks every incoming connection with [`admit`](Self::admit)
/// before it starts the handshake, and refuses the ones over the limit
/// (`quinn::Incoming::refuse`), so a flood of pings or connection attempts
/// from one address costs no handshake.
///
/// Check it **after** address validation (the retry): an unvalidated
/// source address may be forged, and counting it would let anyone use up
/// another address's bucket.
#[derive(Debug)]
pub struct AddrGate {
    limiter: Mutex<Limiter<AddrKey>>,
}

impl AddrGate {
    /// A gate of `rate` per address.
    pub fn new(rate: Rate) -> Self {
        Self::with_keys(rate, Limiter::<AddrKey>::DEFAULT_KEYS)
    }

    /// A gate of `rate` per address, holding at most `keys` addresses.
    pub fn with_keys(rate: Rate, keys: usize) -> Self {
        Self {
            limiter: Mutex::new(Limiter::with_keys(rate, keys)),
        }
    }

    /// Whether a connection from `from` may start its handshake now.
    pub fn admit(&self, from: SocketAddr) -> bool {
        self.admit_at(from, Instant::now()).is_ok()
    }

    /// [`admit`](Self::admit) at `now`, with the wait when refused.
    pub fn admit_at(&self, from: SocketAddr, now: Instant) -> Result<(), Refused> {
        self.lock().check(&AddrKey::from(from), now)
    }

    /// How many addresses the gate holds.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Limiter<AddrKey>> {
        // A panic while holding the lock leaves only bucket times behind:
        // still a valid limiter.
        self.limiter
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
