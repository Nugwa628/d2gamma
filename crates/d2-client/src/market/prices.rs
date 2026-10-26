// D2RUST's own: a price check's answer, as the window reads it.
//! What the realm answers a price check with (contract 3.4, messages
//! 0xC8-0xCB): the sales of items like the checked one, as one series per
//! set of currencies they were paid in. Prices are never converted from
//! one currency to another: a series paid in gold is drawn in gold, one
//! paid in one rune in that rune, and a bundle ("1 Rune 24 + 1 Rune 23") as
//! points with its full price.
//!
//! The values stay as the realm sent them (min, median and max per time
//! bucket and currency), so the drawing can change without a wire change.

use super::source::{ListingId, Price};

/// What a price check is of (contract `MarketPrices.of`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PriceOf {
    /// A listing: the realm finds its price key.
    Listing(ListingId),
    /// One of the asker's shared stash items (the sell form).
    StashItem(u32),
    /// A stackable by code.
    Stackable(String),
    /// A key from an earlier answer (another number of days).
    Key(String),
}

/// How a series was paid (contract `shape`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceShape {
    /// One currency: drawn as a line.
    One,
    /// A bundle of currencies: drawn as points.
    Bundle,
    /// Sales that included items: counted, values left out.
    WithItems,
}

/// The min, median and max price in one currency over a bucket's sales.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spread {
    pub min: u64,
    pub median: u64,
    pub max: u64,
}

/// One time bucket with sales.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bucket {
    /// 0 is the newest, ending at the answer's `now_ms`.
    pub index: u32,
    pub sales: u16,
    /// One per currency of the series; none for [`PriceShape::WithItems`].
    pub values: Vec<Spread>,
}

/// Sales paid in one set of currencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceSeries {
    pub index: u8,
    pub shape: PriceShape,
    /// Sorted; "" gold, else a stackable code.
    pub currencies: Vec<String>,
    pub sold: u32,
    /// Active listings of the same key priced in these currencies.
    pub listed_now: u32,
    /// Per currency, the cheapest of those listings; empty when none is
    /// listed.
    pub lowest_now: Vec<u64>,
    /// Per currency, the median of those listings; empty when none is
    /// listed. Not in contract v1: proposed for 0xCA (`u8 n` × `u64`
    /// after `lowest_now`) for the check's "listed now: lowest, median".
    pub median_now: Vec<u64>,
    pub buckets: Vec<Bucket>,
}

/// One recent sale.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sale {
    pub at_ms: u64,
    pub price: Price,
}

/// The fewest sales a graph is drawn from; under it the check says "not
/// enough data".
pub const MIN_SALES: u32 = 5;

/// The day spans a check offers, and the one it starts with.
pub const DAYS: [u16; 4] = [2, 7, 30, 90];
pub const DEFAULT_DAYS: u16 = 30;

/// An hour and a day in milliseconds.
pub const HOUR_MS: u64 = 3_600_000;
pub const DAY_MS: u64 = 24 * HOUR_MS;

/// A whole price check answer (0xC9, its series, 0xCB).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceHistory {
    /// The price key, for a later [`PriceOf::Key`].
    pub key: String,
    /// "Harlequin Crest".
    pub label: String,
    pub now_ms: u64,
    /// One hour for 2 days or less, else one day.
    pub bucket_ms: u64,
    /// The span asked for.
    pub days: u16,
    /// Active listings with the key, in any currency or offers only.
    pub listed_now: u32,
    /// Sales in the span.
    pub sold: u32,
    pub series: Vec<PriceSeries>,
    /// The last sales (at most 10), newest first.
    pub recent: Vec<Sale>,
}

impl PriceHistory {
    /// Enough sales for a graph.
    pub fn enough(&self) -> bool {
        self.sold >= MIN_SALES
    }

    /// The bucket length the realm uses for a span of `days`.
    pub fn bucket_ms_for(days: u16) -> u64 {
        if days <= 2 {
            HOUR_MS
        } else {
            DAY_MS
        }
    }

    /// The span in milliseconds.
    pub fn span_ms(&self) -> u64 {
        u64::from(self.days) * DAY_MS
    }

    /// How long before `now_ms` the middle of bucket `index` lies.
    pub fn bucket_age_ms(&self, index: u32) -> u64 {
        u64::from(index) * self.bucket_ms + self.bucket_ms / 2
    }
}
