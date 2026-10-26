//! The realm's ids a ticket names (contracts section 11.1). The
//! game-server link (`d2-link`) re-exports them, so both server programs
//! and the tickets use the same types.

use std::fmt;

/// A game server's id: an official server's 1-15 characters
/// `[a-z0-9-]`, or `h-` and 12 hex digits for a player host's session.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub String);

/// A seat lease: unique per realm, never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Lease(pub u64);

/// A player's id inside one game: opaque, so game servers never see
/// account names or ids.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PlayerId(pub u64);

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for Lease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "lease {}", self.0)
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "player {}", self.0)
    }
}
