//! Sales history and the price check (market contract 3.4): every sale
//! writes one row with the price actually paid, under the item's price
//! key; the price check groups an item's recent sales by the set of
//! currencies they were paid in, one series per set, with no conversion
//! between currencies.
//!
//! - **Price key:** `u:<name>` for uniques, sets and runewords (the name
//!   shown), `b:<code>:<group>` for any other item (base code and group),
//!   `s:<code>` for a stackable.
//! - **Series:** gold alone, one stackable alone, a bundle of several
//!   (shape 1); sales that included items are one series of their own
//!   (shape 2), counted, their values left out.
//! - **Buckets:** one hour wide for up to 2 days, else one day; bucket 0
//!   ends now. Only buckets with sales are listed, each with the lowest,
//!   median and highest value per currency.

use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension};

use crate::db::{Act, Economy};
use crate::error::{Error, Refusal};
use crate::key::{EconomyKey, ItemId, Owner};
use crate::market::facets::Group;
use crate::market::price::Price;
use crate::market::Listing;
use crate::stash::page::load_item;
use crate::stash::{Goods, ItemFacts};

/// How many sales [`History::recent`] lists.
pub const RECENT: usize = 10;
/// The longest a price check looks back, in days.
pub const MAX_DAYS: u16 = 90;
const HOUR_MS: i64 = 3600 * 1000;
const DAY_MS: i64 = 24 * HOUR_MS;

/// The price key of an item (its facts) or of a stackable (`code`).
pub fn price_key(facts: Option<&ItemFacts>, code: &str) -> String {
    match facts {
        None => format!("s:{code}"),
        Some(f) => match Group::of(f) {
            Group::Unique | Group::Set | Group::Runeword => format!("u:{}", f.name),
            g => format!("b:{}:{}", f.code, g as u8),
        },
    }
}

/// A series' shape (`MarketPriceSeries.shape`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Shape {
    /// Paid in one currency (gold, or one stackable).
    One = 0,
    /// Paid in a bundle of currencies.
    Bundle = 1,
    /// Paid with items too: counted only.
    WithItems = 2,
}

impl Shape {
    fn of(p: &Price) -> Shape {
        if !p.items.is_empty() {
            Shape::WithItems
        } else if usize::from(p.gold > 0) + p.stacks.len() > 1 {
            Shape::Bundle
        } else {
            Shape::One
        }
    }
}

/// The currencies of a price, sorted: `""` for gold, then the stackable
/// codes. Empty for a price with items (shape 2).
fn currencies(p: &Price) -> Vec<String> {
    if !p.items.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if p.gold > 0 {
        out.push(String::new());
    }
    out.extend(p.stacks.iter().map(|(c, _)| c.clone()));
    out.sort();
    out
}

/// A price's amount in currency `c` (`""` gold).
fn amount(p: &Price, c: &str) -> u64 {
    if c.is_empty() {
        p.gold
    } else {
        p.stacks
            .iter()
            .find(|(code, _)| code == c)
            .map_or(0, |(_, n)| u64::from(*n))
    }
}

/// One bucket of a series: its sales and, per currency of the series, the
/// lowest, median (the lower middle) and highest value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bucket {
    /// 0 is the newest, ending now.
    pub index: u32,
    pub sales: u32,
    /// `(min, median, max)` per currency, in the series' order; empty for
    /// shape 2.
    pub values: Vec<(u64, u64, u64)>,
}

/// The sales paid in one set of currencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Series {
    pub shape: Shape,
    /// `""` for gold, then stackable codes, sorted.
    pub currencies: Vec<String>,
    pub sold: u32,
    /// Active listings with the same key priced in this set.
    pub listed_now: u32,
    /// Per currency, the lowest of those listings; empty when none is
    /// listed.
    pub lowest_now: Vec<u64>,
    /// Per currency, the median (the lower middle) of those listings;
    /// empty when none is listed.
    pub median_now: Vec<u64>,
    /// Oldest first.
    pub buckets: Vec<Bucket>,
}

/// A price check (`MarketPricesBegin` and what follows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct History {
    pub key: String,
    /// The name the key stands for: the unique, set or runeword name, the
    /// stackable's name, or the base code.
    pub label: String,
    pub now: i64,
    pub bucket_ms: i64,
    /// Active, shown listings with the key.
    pub listed_now: u32,
    /// Sales in the window.
    pub sold: u32,
    /// By shape, then currencies.
    pub series: Vec<Series>,
    /// The last [`RECENT`] sales in the window, newest first.
    pub recent: Vec<(i64, Price)>,
}

impl Act<'_> {
    /// The price key and group of a listing's goods.
    pub fn listing_key(&self, l: &Listing) -> Result<String, Error> {
        Ok(match &l.goods {
            Goods::Item(id) => {
                let item = load_item(self.conn, *id)?;
                let code = item
                    .as_ref()
                    .map(|i| i.facts.code.clone())
                    .unwrap_or_default();
                price_key(item.as_ref().map(|i| &i.facts), &code)
            }
            Goods::Stack { code, .. } => price_key(None, code),
            Goods::Gold(_) => String::new(),
        })
    }

    /// Writes a sale of listing `l` for `paid` to the history.
    pub fn record_sale(&self, l: &Listing, paid: &Price) -> Result<(), Error> {
        let key = self.listing_key(l)?;
        self.exec(
            "INSERT INTO market_sales(economy, price_key, listing, closed, price, shape) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                l.economy,
                key,
                l.id,
                self.now,
                paid.encode(),
                Shape::of(paid) as u8
            ],
        )?;
        Ok(())
    }
}

impl Economy {
    /// The price key of a listing (any state), if it exists.
    pub fn listing_price_key(&self, listing: i64) -> Result<Option<String>, Error> {
        let Some(l) = self.listing(listing)? else {
            return Ok(None);
        };
        Ok(Some(match (&l.goods, &l.item) {
            (Goods::Stack { code, .. }, _) => price_key(None, code),
            (_, Some(item)) => price_key(Some(&item.facts), &item.facts.code),
            _ => return Ok(None),
        }))
    }

    /// The price key of an item in the asker's stash.
    pub fn item_price_key(&self, owner: &Owner, id: ItemId) -> Result<Option<String>, Error> {
        self.check_key(&owner.economy)?;
        let mine: Option<i64> = self
            .conn()
            .query_row(
                "SELECT 1 FROM econ_items WHERE item_id = ?1 AND account = ?2 AND economy = ?3 \
                 AND place = 'stash'",
                params![id.0, owner.account.0, owner.economy.split()],
                |r| r.get(0),
            )
            .optional()?;
        if mine.is_none() {
            return Ok(None);
        }
        Ok(load_item(self.conn(), id)?.map(|i| price_key(Some(&i.facts), &i.facts.code)))
    }

    /// The recent sales of `key` in `economy` over the last `days` (1 to
    /// 90), and what is listed now.
    pub fn price_history(
        &self,
        economy: &EconomyKey,
        key: &str,
        days: u16,
        now: i64,
    ) -> Result<History, Error> {
        if !self.config().market {
            return Err(Refusal::MarketOff.into());
        }
        self.check_key(economy)?;
        if days == 0 || days > MAX_DAYS || key.len() > 255 {
            return Err(Refusal::BadRequest.into());
        }
        let split = economy.split();
        let conn = self.conn();
        let bucket_ms = if days <= 2 { HOUR_MS } else { DAY_MS };
        let since = now - i64::from(days) * DAY_MS;
        let sales: Vec<(i64, Price)> = {
            let mut st = conn.prepare(
                "SELECT closed, price FROM market_sales \
                 WHERE economy = ?1 AND price_key = ?2 AND closed > ?3 AND closed <= ?4 \
                 ORDER BY closed DESC, id DESC",
            )?;
            let rows = st.query_map(params![split, key, since, now], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
            })?;
            let mut out = Vec::new();
            for row in rows {
                let (at, bytes) = row?;
                if let Some(p) = Price::decode(&bytes) {
                    out.push((at, p));
                }
            }
            out
        };
        let listed: Vec<Option<Price>> = {
            let mut st = conn.prepare(
                "SELECT l.price FROM market_items m JOIN listings l ON l.id = m.listing \
                 WHERE m.economy = ?1 AND m.price_key = ?2 AND m.expires > ?3",
            )?;
            let rows =
                st.query_map(params![split, key, now], |r| r.get::<_, Option<Vec<u8>>>(0))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?.as_deref().and_then(Price::decode));
            }
            out
        };

        // Series by (shape, currencies): sales, then listings.
        type Set = (Shape, Vec<String>);
        let mut sold: BTreeMap<Set, Vec<(i64, &Price)>> = BTreeMap::new();
        for (at, p) in &sales {
            sold.entry((Shape::of(p), currencies(p)))
                .or_default()
                .push((*at, p));
        }
        let mut now_listed: BTreeMap<Set, Vec<&Price>> = BTreeMap::new();
        for p in listed.iter().flatten() {
            now_listed
                .entry((Shape::of(p), currencies(p)))
                .or_default()
                .push(p);
        }
        let mut sets: Vec<Set> = sold.keys().chain(now_listed.keys()).cloned().collect();
        sets.sort();
        sets.dedup();
        let series = sets
            .into_iter()
            .map(|set| {
                let (shape, cur) = set.clone();
                let these = sold.get(&set).map(Vec::as_slice).unwrap_or_default();
                let listings = now_listed.get(&set).map(Vec::as_slice).unwrap_or_default();
                let now_values: Vec<Vec<u64>> = if listings.is_empty() {
                    Vec::new()
                } else {
                    cur.iter()
                        .map(|c| {
                            let mut v: Vec<u64> = listings.iter().map(|p| amount(p, c)).collect();
                            v.sort_unstable();
                            v
                        })
                        .collect()
                };
                let lowest_now = now_values.iter().map(|v| v[0]).collect();
                let median_now = now_values.iter().map(|v| v[(v.len() - 1) / 2]).collect();
                let mut by_bucket: BTreeMap<u32, Vec<&Price>> = BTreeMap::new();
                for (at, p) in these {
                    let index = u32::try_from((now - at) / bucket_ms).unwrap_or(u32::MAX);
                    by_bucket.entry(index).or_default().push(p);
                }
                let buckets = by_bucket
                    .into_iter()
                    .rev()
                    .map(|(index, ps)| Bucket {
                        index,
                        sales: ps.len() as u32,
                        values: cur
                            .iter()
                            .map(|c| {
                                let mut v: Vec<u64> = ps.iter().map(|p| amount(p, c)).collect();
                                v.sort_unstable();
                                (v[0], v[(v.len() - 1) / 2], v[v.len() - 1])
                            })
                            .collect(),
                    })
                    .collect();
                Series {
                    shape,
                    currencies: cur,
                    sold: these.len() as u32,
                    listed_now: listings.len() as u32,
                    lowest_now,
                    median_now,
                    buckets,
                }
            })
            .collect();
        Ok(History {
            key: key.to_owned(),
            label: self.key_label(key),
            now,
            bucket_ms,
            listed_now: listed.len() as u32,
            sold: sales.len() as u32,
            series,
            recent: sales.iter().take(RECENT).cloned().collect(),
        })
    }

    fn key_label(&self, key: &str) -> String {
        match key.split_once(':') {
            Some(("u", name)) => name.to_owned(),
            Some(("s", code)) => self
                .config()
                .stackable(code)
                .map_or_else(|| code.to_owned(), |s| s.name.clone()),
            Some(("b", rest)) => rest.split(':').next().unwrap_or_default().to_owned(),
            _ => key.to_owned(),
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn keys_shapes_and_currencies() {
        let f = |quality, runeword, name: &str| ItemFacts {
            code: "uap".into(),
            quality,
            runeword,
            name: name.into(),
            ..ItemFacts::default()
        };
        assert_eq!(price_key(Some(&f(7, false, "Shako")), "uap"), "u:Shako");
        assert_eq!(price_key(Some(&f(5, false, "Set Cap")), "uap"), "u:Set Cap");
        assert_eq!(price_key(Some(&f(2, true, "Spirit")), "uap"), "u:Spirit");
        assert_eq!(price_key(Some(&f(6, false, "Grim Cap")), "uap"), "b:uap:2");
        assert_eq!(price_key(None, "r24"), "s:r24");
        let p = |gold, stacks: &[(&str, u32)], items: &[u32]| Price {
            gold,
            stacks: stacks.iter().map(|(c, n)| ((*c).to_owned(), *n)).collect(),
            items: items.iter().map(|i| ItemId(*i)).collect(),
        };
        assert_eq!(Shape::of(&p(5, &[], &[])), Shape::One);
        assert_eq!(Shape::of(&p(0, &[("r24", 1)], &[])), Shape::One);
        assert_eq!(Shape::of(&p(5, &[("r24", 1)], &[])), Shape::Bundle);
        assert_eq!(
            Shape::of(&p(0, &[("r23", 1), ("r24", 1)], &[])),
            Shape::Bundle
        );
        assert_eq!(Shape::of(&p(0, &[], &[3])), Shape::WithItems);
        assert_eq!(currencies(&p(5, &[("r24", 1)], &[])), ["", "r24"]);
        assert_eq!(currencies(&p(5, &[], &[3])), Vec::<String>::new());
        assert_eq!(amount(&p(5, &[("r24", 2)], &[]), "r24"), 2);
        assert_eq!(amount(&p(5, &[("r24", 2)], &[]), ""), 5);
    }
}
