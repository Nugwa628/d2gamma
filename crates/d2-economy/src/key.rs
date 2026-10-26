//! Who and what the economy's calls name: the economy key, accounts, item
//! IDs, characters in games, and request numbers.

use std::fmt;

/// Which economy goods belong to (contracts 11.13): the mod realm and the
/// split inside it. Goods never move between two different keys.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EconomyKey {
    /// The mod realm (`d2rust` for vanilla, `pd2`, ...); one database per
    /// mod.
    pub mod_id: String,
    pub hardcore: bool,
    /// The ladder season, 0 for none.
    pub ladder: u16,
    pub expansion: bool,
}

impl EconomyKey {
    /// Softcore, non-ladder, expansion: the usual economy of `mod_id`.
    pub fn new(mod_id: impl Into<String>) -> Self {
        EconomyKey {
            mod_id: mod_id.into(),
            hardcore: false,
            ladder: 0,
            expansion: true,
        }
    }

    /// The split inside the mod as stored in every row, for example
    /// `sc-l0-exp` or `hc-l3-cls`.
    pub fn split(&self) -> String {
        format!(
            "{}-l{}-{}",
            if self.hardcore { "hc" } else { "sc" },
            self.ladder,
            if self.expansion { "exp" } else { "cls" }
        )
    }
}

/// An account of the main server (the accounts database's `accounts.id`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(pub i64);

/// A realm item ID: the ID an extended item carries, unique within its mod
/// realm and never reused. Simple items have none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u32);

/// One account's goods in one economy: whose stash, listings and
/// deliveries a call is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    pub account: AccountId,
    pub economy: EconomyKey,
}

impl Owner {
    pub fn new(account: i64, economy: EconomyKey) -> Self {
        Owner {
            account: AccountId(account),
            economy,
        }
    }
}

/// A character seated in a game: where goods leave from or arrive to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharRef {
    pub name: String,
    pub game: u32,
    /// The seat lease the game server holds for this character.
    pub lease: u64,
}

/// Who numbers a request (contracts 11.13): a client per account, the game
/// server per lease, the main server per character.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    Account(AccountId),
    Lease(u64),
    Character(String),
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scope::Account(a) => write!(f, "a:{}", a.0),
            Scope::Lease(l) => write!(f, "l:{l}"),
            Scope::Character(c) => write!(f, "c:{c}"),
        }
    }
}

/// A request number in its scope. A repeated `Req` gets the first answer
/// again and changes nothing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Req {
    pub scope: Scope,
    pub n: u64,
}

impl Req {
    /// A client's request for its account.
    pub fn account(account: AccountId, n: u64) -> Self {
        Req {
            scope: Scope::Account(account),
            n,
        }
    }

    /// A game server's request under a seat lease.
    pub fn lease(lease: u64, n: u64) -> Self {
        Req {
            scope: Scope::Lease(lease),
            n,
        }
    }
}
