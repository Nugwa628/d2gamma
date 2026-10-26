//! The economy's limits and lists, per mod realm (contracts 11.8). The
//! defaults are the plan's proposals; each is one value the realm's config
//! may change.

/// A simple item the stash stores as a count (plan E1, open question 19):
/// runes, gems, potions, keys, essences, tokens and a mod's currencies.
/// Stackables are also the market's currencies besides gold (open
/// question 21: every stackable).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stackable {
    /// The item code, as in the game data.
    pub code: String,
    /// The name players see and search for.
    pub name: String,
    /// Its item type and that type's ancestors (`rune`, `misc`, ...), for
    /// the market's type tree; empty leaves it out of the tree.
    pub types: Vec<String>,
}

impl Stackable {
    pub fn new(code: impl Into<String>, name: impl Into<String>) -> Self {
        Stackable {
            code: code.into(),
            name: name.into(),
            types: Vec::new(),
        }
    }

    /// The same stackable under the item types `types` (its type first).
    pub fn with_types(mut self, types: &[&str]) -> Self {
        self.types = types.iter().map(|t| (*t).to_owned()).collect();
        self
    }
}

/// One mod realm's economy settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EconomyConfig {
    /// The realm's `stackables` list.
    pub stackables: Vec<Stackable>,
    /// Items in one account's stash per economy (`stash_items`).
    pub stash_items: u32,
    /// Gold in one account's stash per economy (`stash_gold_max`). Stack
    /// counts have no limit but their `u32`.
    pub stash_gold_max: u64,
    /// How long a request's answer is kept for repeats, in milliseconds.
    pub request_keep_ms: i64,
    /// The `market` switch: off until the owner turns it on.
    pub market: bool,
    /// `market_fee_pct`: the share of a sale's gold the realm keeps (a gold
    /// sink; open question 22).
    pub market_fee_pct: u8,
    /// `market_listing_fee`: gold taken from the stash per listing and per
    /// relist.
    pub market_listing_fee: u64,
    /// `market_buyer_fee`: gold a buyer pays on top of the price. Reserved:
    /// 0, and the main server refuses another value until fees are built.
    pub market_buyer_fee: u64,
    /// `market_max_listings`: active listings per account and economy
    /// (open question 23).
    pub market_max_listings: u32,
    /// `market_max_offers`: open offers per account and economy.
    pub market_max_offers: u32,
    /// `market_offer_items`: stash items one offer may name.
    pub market_offer_items: u32,
    /// `market_days`: the longest a listing stays up.
    pub market_days: u8,
    /// `market_posts_per_min`: listings an account may post per minute.
    pub market_posts_per_min: u32,
    /// `market_searches_per_min`: searches an account may run per minute.
    pub market_searches_per_min: u32,
}

/// `stash_items`: 2,000 items (open question 18).
pub const STASH_ITEMS: u32 = 2_000;
/// `stash_gold_max`: 2,500,000 gold (open question 18).
pub const STASH_GOLD_MAX: u64 = 2_500_000;
/// Answers are repeated for 24 hours (contracts 11.13).
pub const REQUEST_KEEP_MS: i64 = 24 * 3600 * 1000;
/// `market_max_listings`: 50 (open question 23).
pub const MARKET_MAX_LISTINGS: u32 = 50;
/// `market_max_offers`: 20 (open question 23).
pub const MARKET_MAX_OFFERS: u32 = 20;
/// `market_offer_items`: 10 (never above 16, the wire's bound).
pub const MARKET_OFFER_ITEMS: u32 = 10;
/// `market_days`: 7 (open question 23).
pub const MARKET_DAYS: u8 = 7;
/// `market_posts_per_min`: 10. The plan names the limit, not its value.
pub const MARKET_POSTS_PER_MIN: u32 = 10;
/// `market_searches_per_min`: 60. The plan names the limit, not its value.
pub const MARKET_SEARCHES_PER_MIN: u32 = 60;

impl Default for EconomyConfig {
    fn default() -> Self {
        EconomyConfig {
            stackables: Vec::new(),
            stash_items: STASH_ITEMS,
            stash_gold_max: STASH_GOLD_MAX,
            request_keep_ms: REQUEST_KEEP_MS,
            market: false,
            market_fee_pct: 0,
            market_listing_fee: 0,
            market_buyer_fee: 0,
            market_max_listings: MARKET_MAX_LISTINGS,
            market_max_offers: MARKET_MAX_OFFERS,
            market_offer_items: MARKET_OFFER_ITEMS,
            market_days: MARKET_DAYS,
            market_posts_per_min: MARKET_POSTS_PER_MIN,
            market_searches_per_min: MARKET_SEARCHES_PER_MIN,
        }
    }
}

impl EconomyConfig {
    /// The stackable with `code`, if the realm lists it.
    pub fn stackable(&self, code: &str) -> Option<&Stackable> {
        self.stackables.iter().find(|s| s.code == code)
    }

    /// `fee_rev`: names the fees in force. A request that pays or earns
    /// carries the revision the player saw; another one is refused with
    /// [`crate::Refusal::FeeChanged`], so a confirm always matches the
    /// fees taken. The first 4 bytes (little-endian) of SHA-256 over
    /// `fees-1`, the sale fee percent, the listing fee and the buyer fee.
    pub fn fee_rev(&self) -> u32 {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"fees-1");
        h.update([self.market_fee_pct]);
        h.update(self.market_listing_fee.to_le_bytes());
        h.update(self.market_buyer_fee.to_le_bytes());
        let d = h.finalize();
        u32::from_le_bytes([d[0], d[1], d[2], d[3]])
    }
}
