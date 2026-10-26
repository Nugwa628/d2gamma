//! Market search (plan E3, module N30; market contract 3.1): index tables
//! filled when goods enter a listing's escrow and cleared when they leave,
//! in the same transaction, and the search over them.
//!
//! - `market_items`: one row per active, shown listing with the item's
//!   facts (or the stackable's code and count), its group, name, seller,
//!   price flags, price key and the price's gold or single currency.
//! - `market_prices`: the price's stackable counts, to search and sort by
//!   a given currency.
//! - `market_stats`: the item's stats, indexed on (stat, value) for stat
//!   ranges.
//! - `market_type_tree`: each item type listed and the types above it
//!   (itself included), so a node of the type tree matches everything
//!   under it. Rows are only added: the tree is the game data's.
//! - `market_names`: an FTS5 table of the names (the item's name and name
//!   parts, base code and type; a stackable's name and code), searched by
//!   word prefixes.
//!
//! Hidden listings are not indexed; listings past their expiry stay until
//! the sweep but never match.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter};

use crate::db::{Act, Economy};
use crate::error::{Error, Refusal};
use crate::key::{AccountId, EconomyKey};
use crate::market::facets::Group;
use crate::market::history::price_key;
use crate::market::price::Price;
use crate::market::{fill_item, listing_of, ListingMode, ListingsPage, LISTING_COLUMNS};
use crate::stash::page::{load_item, push_filter, Cols, ItemFilter, Sql, PAGE};
use crate::stash::Goods;

/// Migration 3: the search index.
pub(crate) const SCHEMA: &str = "
CREATE TABLE market_items(
    listing INTEGER PRIMARY KEY,
    economy TEXT NOT NULL,
    item_id INTEGER,
    code TEXT NOT NULL,
    item_type TEXT NOT NULL,
    quality INTEGER NOT NULL,
    level_req INTEGER NOT NULL,
    ilvl INTEGER NOT NULL,
    sockets INTEGER NOT NULL,
    ethereal INTEGER NOT NULL,
    class INTEGER,
    count INTEGER,
    mode INTEGER NOT NULL,
    price_gold INTEGER,
    posted INTEGER NOT NULL,
    expires INTEGER NOT NULL
);
CREATE INDEX market_items_posted ON market_items(economy, posted);
CREATE INDEX market_items_type ON market_items(economy, item_type, posted);
CREATE INDEX market_items_gold ON market_items(economy, price_gold);
CREATE TABLE market_prices(
    listing INTEGER NOT NULL,
    code TEXT NOT NULL,
    count INTEGER NOT NULL,
    PRIMARY KEY(listing, code)
) WITHOUT ROWID;
CREATE INDEX market_prices_code ON market_prices(code, count);
CREATE TABLE market_stats(
    listing INTEGER NOT NULL,
    stat INTEGER NOT NULL,
    value INTEGER NOT NULL,
    PRIMARY KEY(listing, stat)
) WITHOUT ROWID;
CREATE INDEX market_stats_value ON market_stats(stat, value);
CREATE VIRTUAL TABLE market_names USING fts5(names, tokenize = 'unicode61 remove_diacritics 2');
";

/// `market_items.price_flags`: the listing has a price.
pub(crate) const HAS_PRICE: i64 = 1;
/// The listing takes offers.
pub(crate) const TAKES_OFFERS: i64 = 2;
/// Its price has gold.
pub(crate) const HAS_GOLD: i64 = 4;
/// Its price has stackables.
pub(crate) const HAS_STACKS: i64 = 8;

/// What search results are sorted by. Ties are broken by the listing ID,
/// so pages never overlap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MarketSort {
    /// Date posted (the default, newest first with `desc`).
    #[default]
    Date,
    /// The price in [`MarketQuery::sort_currency`] (else the price
    /// filter's currency, else gold). Lists only the listings priced in
    /// that currency.
    Price,
    /// A stat's value. Lists only the listings whose item has the stat.
    Stat(u16),
    /// The level requirement.
    Level,
    /// The name shown, any case.
    Name,
    /// The seller's character name, any case.
    Seller,
}

/// A price range in one currency, both ends included.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PriceFilter {
    /// A stackable code, or `None` for gold.
    pub code: Option<String>,
    pub min: u64,
    pub max: u64,
}

/// What a listing's price must be (`MarketQuery.price_mode`, the price
/// chips).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PriceMode {
    #[default]
    Any = 0,
    /// It has a price (it can be bought).
    HasPrice = 1,
    /// It takes offers.
    TakesOffers = 2,
    /// Its price has no gold: currencies only ("runes only").
    CurrenciesOnly = 3,
    /// Its price is gold only.
    GoldOnly = 4,
}

impl PriceMode {
    pub const ALL: [PriceMode; 5] = [
        PriceMode::Any,
        PriceMode::HasPrice,
        PriceMode::TakesOffers,
        PriceMode::CurrenciesOnly,
        PriceMode::GoldOnly,
    ];

    pub fn from_u8(m: u8) -> Option<PriceMode> {
        PriceMode::ALL.get(usize::from(m)).copied()
    }

    /// Whether a listing with these `price_flags` matches.
    pub(crate) fn matches(self, flags: i64) -> bool {
        match self {
            PriceMode::Any => true,
            PriceMode::HasPrice => flags & HAS_PRICE != 0,
            PriceMode::TakesOffers => flags & TAKES_OFFERS != 0,
            PriceMode::CurrenciesOnly => flags & (HAS_PRICE | HAS_GOLD) == HAS_PRICE,
            PriceMode::GoldOnly => {
                flags & (HAS_PRICE | HAS_GOLD | HAS_STACKS) == HAS_PRICE | HAS_GOLD
            }
        }
    }

    /// The same test in SQL over `m.price_flags`.
    fn sql(self) -> Option<String> {
        let (mask, want) = match self {
            PriceMode::Any => return None,
            PriceMode::HasPrice => (HAS_PRICE, HAS_PRICE),
            PriceMode::TakesOffers => (TAKES_OFFERS, TAKES_OFFERS),
            PriceMode::CurrenciesOnly => (HAS_PRICE | HAS_GOLD, HAS_PRICE),
            PriceMode::GoldOnly => (HAS_PRICE | HAS_GOLD | HAS_STACKS, HAS_PRICE | HAS_GOLD),
        };
        Some(format!("(m.price_flags & {mask}) = {want}"))
    }
}

/// One search (`MarketQuery`, market contract 3.1); every set field must
/// match. The item filter's `text` is matched as word prefixes against
/// every name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct MarketQuery {
    pub filter: ItemFilter,
    /// The listing is priced in this currency, within the range (other
    /// currencies may be in the price too).
    pub price: Option<PriceFilter>,
    pub sort: MarketSort,
    pub desc: bool,
    /// From 0.
    pub page: u16,
    /// Any of these groups; empty for all.
    pub groups: Vec<Group>,
    /// Any of these item types, each matching the types under it; empty
    /// for all.
    pub types: Vec<String>,
    /// Has sockets (yes or no), or any.
    pub socketed: Option<bool>,
    /// Identified (yes or no), or any.
    pub identified: Option<bool>,
    pub price_mode: PriceMode,
    /// "2 Ist or 400,000 gold": a listing matches a cap if its price is
    /// only that currency and within the range; caps are OR'd; empty for
    /// no cap.
    pub caps: Vec<PriceFilter>,
    /// The currency of [`MarketSort::Price`]: `""` for gold, a stackable
    /// code, or `None` for the price filter's.
    pub sort_currency: Option<String>,
    /// Groups in grid order first, then the sort (the grid view).
    pub group_first: bool,
    /// 25, 50 or 100 a page; 0 for 50.
    pub page_size: u8,
}

impl MarketQuery {
    /// Rows a page: 25, 50 or 100.
    pub fn rows_a_page(&self) -> Result<u32, Refusal> {
        match self.page_size {
            0 => Ok(PAGE),
            n @ (25 | 50 | 100) => Ok(u32::from(n)),
            _ => Err(Refusal::BadRequest),
        }
    }

    pub(crate) fn check(&self) -> Result<(), Refusal> {
        let text = self.filter.text.as_deref().map_or(0, str::len);
        if text > 60
            || self.groups.len() > 8
            || self.types.len() > 32
            || self.types.iter().any(|t| t.len() > 16)
            || self.filter.stats.len() > 8
            || self.caps.len() > 4
        {
            return Err(Refusal::BadRequest);
        }
        self.rows_a_page().map(|_| ())
    }
}

/// The window of the searches-per-minute limit.
const MINUTE_MS: i64 = 60 * 1000;

/// When each account searched in the last minute. Searches only read, so
/// the limit is kept in memory, per [`Economy`].
#[derive(Debug, Default)]
pub(crate) struct SearchLimit(RefCell<HashMap<i64, VecDeque<i64>>>);

impl SearchLimit {
    fn admit(&self, account: i64, now: i64, per_min: u32) -> Result<(), Refusal> {
        let mut all = self.0.borrow_mut();
        // Drop accounts that went quiet, so the map stays small.
        if all.len() > 4096 {
            all.retain(|_, at| at.back().is_some_and(|t| *t > now - MINUTE_MS));
        }
        let seen = all.entry(account).or_default();
        while seen.front().is_some_and(|t| *t <= now - MINUTE_MS) {
            seen.pop_front();
        }
        if seen.len() >= per_min as usize {
            return Err(Refusal::RateLimited);
        }
        seen.push_back(now);
        Ok(())
    }
}

/// An FTS5 query matching every word of `text` as a prefix, or `None` if
/// it has no words. Words are quoted, so no FTS5 syntax gets through.
fn fts_query(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(8)
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!words.is_empty()).then(|| words.join(" "))
}

fn to_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

fn tri(sql: &mut Sql, column: &str, v: Option<bool>) {
    if let Some(v) = v {
        sql.push(format!("{column} = ?"), [Value::Integer(v.into())]);
    }
}

/// The `WHERE` of a search over `market_items m` in `split` at `now`;
/// without the faceted filters (groups, types, ethereal, socketed,
/// identified, price mode) unless `faceted`.
pub(crate) fn query_sql(q: &MarketQuery, split: &str, now: i64, faceted: bool) -> Sql {
    let mut sql = Sql::default();
    sql.push(
        "m.economy = ? AND m.expires > ?",
        [Value::Text(split.to_owned()), Value::Integer(now)],
    );
    if let Some(fts) = q.filter.text.as_deref().and_then(fts_query) {
        sql.push(
            "m.listing IN (SELECT rowid FROM market_names WHERE market_names MATCH ?)",
            [Value::Text(fts)],
        );
    }
    let mut filter = q.filter.clone();
    if !faceted {
        filter.ethereal = None;
    }
    push_filter(
        &mut sql,
        &filter,
        &Cols {
            alias: "m",
            stats: "market_stats",
            stats_key: "listing",
            key: "m.listing",
        },
    );
    if faceted {
        push_faceted(&mut sql, q);
    }
    if let Some(p) = &q.price {
        let (lo, hi) = (to_i64(p.min), to_i64(p.max));
        match &p.code {
            None => sql.push(
                "m.price_gold BETWEEN ? AND ?",
                [Value::Integer(lo), Value::Integer(hi)],
            ),
            Some(code) => sql.push(
                "m.listing IN (SELECT listing FROM market_prices \
                 WHERE code = ? AND count BETWEEN ? AND ?)",
                [
                    Value::Text(code.clone()),
                    Value::Integer(lo),
                    Value::Integer(hi),
                ],
            ),
        }
    }
    if !q.caps.is_empty() {
        let one = "(m.price_cur = ? AND m.price_amt BETWEEN ? AND ?)";
        let clause = vec![one; q.caps.len()].join(" OR ");
        let values = q.caps.iter().flat_map(|c| {
            [
                Value::Text(c.code.clone().unwrap_or_default()),
                Value::Integer(to_i64(c.min)),
                Value::Integer(to_i64(c.max)),
            ]
        });
        sql.push(format!("({clause})"), values);
    }
    sql
}

/// The filters the facets count: groups, types, socketed, identified and
/// price mode (ethereal is in the item filter).
fn push_faceted(sql: &mut Sql, q: &MarketQuery) {
    if !q.groups.is_empty() {
        let marks = vec!["?"; q.groups.len()].join(", ");
        sql.push(
            format!("m.grp IN ({marks})"),
            q.groups.iter().map(|g| Value::Integer(*g as i64)),
        );
    }
    if !q.types.is_empty() {
        let marks = vec!["?"; q.types.len()].join(", ");
        sql.push(
            format!(
                "m.item_type IN (SELECT item_type FROM market_type_tree WHERE type IN ({marks}))"
            ),
            q.types.iter().map(|t| Value::Text(t.clone())),
        );
    }
    if let Some(s) = q.socketed {
        sql.push(
            if s { "m.sockets > 0" } else { "m.sockets = 0" },
            std::iter::empty(),
        );
    }
    tri(sql, "m.identified", q.identified);
    if let Some(clause) = q.price_mode.sql() {
        sql.push(clause, std::iter::empty());
    }
}

impl Economy {
    /// A page of active listings in `economy` matching `q`. `who` is the
    /// searching account, held to the realm's `market_searches_per_min`
    /// ([`Refusal::RateLimited`]); `None` for callers that limit
    /// otherwise (the public web pages, per address). Listings past their
    /// expiry leave the index with the expiry sweep ([`Economy::expire`])
    /// and never match before it.
    pub fn search(
        &self,
        economy: &EconomyKey,
        who: Option<AccountId>,
        q: &MarketQuery,
        now: i64,
    ) -> Result<ListingsPage, Error> {
        if !self.config().market {
            return Err(Refusal::MarketOff.into());
        }
        self.check_key(economy)?;
        q.check()?;
        if let Some(a) = who {
            self.search_limit()
                .admit(a.0, now, self.config().market_searches_per_min)?;
        }
        let rows = q.rows_a_page()?;
        let conn = self.conn();
        let mut sql = query_sql(q, &economy.split(), now, true);
        let dir = if q.desc { "DESC" } else { "ASC" };
        let mut all = Vec::new();
        let (join, order) = match q.sort {
            MarketSort::Date => (String::new(), format!("m.posted {dir}, m.listing {dir}")),
            MarketSort::Price => {
                let code = match &q.sort_currency {
                    Some(c) => Some(c.clone()).filter(|c| !c.is_empty()),
                    None => q.price.as_ref().and_then(|p| p.code.clone()),
                };
                match code {
                    None => {
                        sql.push("m.price_gold IS NOT NULL", std::iter::empty());
                        (
                            String::new(),
                            format!("m.price_gold {dir}, m.listing {dir}"),
                        )
                    }
                    Some(code) => {
                        all.push(Value::Text(code));
                        (
                            " JOIN market_prices p ON p.listing = m.listing AND p.code = ?".into(),
                            format!("p.count {dir}, p.listing {dir}"),
                        )
                    }
                }
            }
            MarketSort::Stat(stat) => {
                all.push(Value::Integer(stat.into()));
                (
                    " JOIN market_stats s ON s.listing = m.listing AND s.stat = ?".into(),
                    format!("s.value {dir}, s.listing {dir}"),
                )
            }
            MarketSort::Level => (String::new(), format!("m.level_req {dir}, m.listing {dir}")),
            MarketSort::Name => (
                String::new(),
                format!("m.name COLLATE NOCASE {dir}, m.listing {dir}"),
            ),
            MarketSort::Seller => (
                String::new(),
                format!("m.seller COLLATE NOCASE {dir}, m.listing {dir}"),
            ),
        };
        let order = if q.group_first {
            format!("m.gorder, {order}")
        } else {
            order
        };
        let wh = sql.where_clause();
        all.extend(sql.params);
        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM market_items m{join}{wh}"),
            params_from_iter(all.iter()),
            |r| r.get(0),
        )?;
        all.push(Value::Integer(rows.into()));
        all.push(Value::Integer(i64::from(q.page) * i64::from(rows)));
        let mut st = conn.prepare(&format!(
            "SELECT {LISTING_COLUMNS} FROM market_items m{join} JOIN listings l ON l.id = m.listing\
             {wh} ORDER BY {order} LIMIT ? OFFSET ?"
        ))?;
        let found = st.query_map(params_from_iter(all.iter()), listing_of)?;
        let mut listings: Vec<_> = found.collect::<Result<_, _>>()?;
        for l in &mut listings {
            fill_item(conn, l)?;
        }
        Ok(ListingsPage {
            total: u32::try_from(total).unwrap_or(u32::MAX),
            page: q.page,
            listings,
        })
    }
}

/// What the index keeps of a listing's price: its flags, gold, and its one
/// currency and amount when it is priced in exactly one.
fn price_columns(
    mode: ListingMode,
    price: Option<&Price>,
) -> (i64, Option<i64>, Option<(String, i64)>) {
    let mut flags = 0;
    if mode.has_price() {
        flags |= HAS_PRICE;
    }
    if mode.takes_offers() {
        flags |= TAKES_OFFERS;
    }
    let Some(p) = price else {
        return (flags, None, None);
    };
    let gold = (p.gold > 0).then(|| to_i64(p.gold));
    if gold.is_some() {
        flags |= HAS_GOLD;
    }
    if !p.stacks.is_empty() {
        flags |= HAS_STACKS;
    }
    let single = match (gold, p.stacks.as_slice()) {
        (Some(g), []) => Some((String::new(), g)),
        (None, [(code, count)]) => Some((code.clone(), i64::from(*count))),
        _ => None,
    };
    (flags, gold, single)
}

impl Act<'_> {
    /// Adds an active listing to the search index; a hidden one stays out.
    pub fn index_listing(&self, listing: i64) -> Result<(), Error> {
        let l = self.conn.query_row(
            &format!("SELECT {LISTING_COLUMNS} FROM listings l WHERE l.id = ?1"),
            [listing],
            listing_of,
        )?;
        if l.hidden {
            return Ok(());
        }
        let (flags, gold, single) = price_columns(l.mode, l.price.as_ref());
        let (cur, amt) = single.map_or((None, None), |(c, a)| (Some(c), Some(a)));
        let (item_type, names, types) = match &l.goods {
            Goods::Item(id) => {
                let item = load_item(self.conn, *id)?.ok_or_else(|| {
                    Error::Db(format!("listing {listing}: item {} not found", id.0))
                })?;
                let f = &item.facts;
                let group = Group::of(f);
                self.exec(
                    "INSERT INTO market_items(listing, economy, item_id, code, item_type, quality, \
                     level_req, ilvl, sockets, ethereal, class, count, mode, price_gold, posted, \
                     expires, grp, gorder, name, seller, price_flags, identified, price_key, \
                     price_cur, price_amt) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, NULL, ?12, ?13, ?14, \
                     ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
                    params![
                        listing,
                        l.economy,
                        id.0,
                        f.code,
                        f.item_type,
                        f.quality,
                        f.level_req,
                        f.ilvl,
                        f.sockets,
                        f.ethereal,
                        f.class,
                        l.mode as u8,
                        gold,
                        l.posted,
                        l.expires,
                        group as u8,
                        group.grid_order(),
                        f.name,
                        l.seller,
                        flags,
                        f.identified,
                        price_key(Some(f), &f.code),
                        cur,
                        amt,
                    ],
                )?;
                self.exec(
                    "INSERT INTO market_stats(listing, stat, value) \
                     SELECT ?1, stat, value FROM item_stats WHERE item_id = ?2",
                    params![listing, id.0],
                )?;
                let names = format!(
                    "{} {} {} {}",
                    f.name,
                    f.name_parts.join(" "),
                    f.code,
                    f.item_type
                );
                (f.item_type.clone(), names, f.type_codes())
            }
            Goods::Stack { code, count } => {
                let group = Group::Stackable;
                let stackable = self.config.stackable(code);
                let name = stackable.map(|s| s.name.as_str()).unwrap_or_default();
                self.exec(
                    "INSERT INTO market_items(listing, economy, item_id, code, item_type, quality, \
                     level_req, ilvl, sockets, ethereal, class, count, mode, price_gold, posted, \
                     expires, grp, gorder, name, seller, price_flags, identified, price_key, \
                     price_cur, price_amt) \
                     VALUES (?1, ?2, NULL, ?3, ?3, 0, 0, 0, 0, 0, NULL, ?4, ?5, ?6, ?7, ?8, ?9, \
                     ?10, ?11, ?12, ?13, 1, ?14, ?15, ?16)",
                    params![
                        listing,
                        l.economy,
                        code,
                        count,
                        l.mode as u8,
                        gold,
                        l.posted,
                        l.expires,
                        group as u8,
                        group.grid_order(),
                        if name.is_empty() { code.as_str() } else { name },
                        l.seller,
                        flags,
                        price_key(None, code),
                        cur,
                        amt,
                    ],
                )?;
                let types = stackable.map(|s| s.types.clone()).unwrap_or_default();
                (code.clone(), format!("{name} {code}"), types)
            }
            Goods::Gold(_) => return Err(Error::Db(format!("listing {listing} sells gold"))),
        };
        for (code, count) in l.price.iter().flat_map(|p| p.stacks.iter()) {
            self.exec(
                "INSERT INTO market_prices(listing, code, count) VALUES (?1, ?2, ?3)",
                params![listing, code, count],
            )?;
        }
        for t in &types {
            self.exec(
                "INSERT OR IGNORE INTO market_type_tree(item_type, type) VALUES (?1, ?2)",
                params![item_type, t],
            )?;
        }
        self.exec(
            "INSERT INTO market_names(rowid, names) VALUES (?1, ?2)",
            params![listing, names],
        )?;
        Ok(())
    }

    /// Indexes the listed stacks again (after migration 5).
    pub fn reindex_stacks(&self) -> Result<(), Error> {
        let ids: Vec<i64> = {
            let mut st = self
                .conn
                .prepare("SELECT listing FROM market_items WHERE item_id IS NULL")?;
            let rows = st.query_map([], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        for id in ids {
            self.unindex_listing(id)?;
            self.index_listing(id)?;
        }
        Ok(())
    }

    /// Takes a listing out of the search index.
    pub fn unindex_listing(&self, listing: i64) -> Result<(), Error> {
        for table in ["market_items", "market_prices", "market_stats"] {
            self.exec(
                &format!("DELETE FROM {table} WHERE listing = ?1"),
                [listing],
            )?;
        }
        self.exec("DELETE FROM market_names WHERE rowid = ?1", [listing])?;
        Ok(())
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn fts_words_are_quoted_prefixes() {
        assert_eq!(fts_query("  "), None);
        assert_eq!(
            fts_query("Harle* \"quin OR"),
            Some("\"Harle\"* \"quin\"* \"OR\"*".into())
        );
    }

    #[test]
    fn search_limit_window() {
        let l = SearchLimit::default();
        for t in 0..3 {
            l.admit(1, t, 3).unwrap();
        }
        assert_eq!(l.admit(1, 10, 3), Err(Refusal::RateLimited));
        l.admit(2, 10, 3).unwrap();
        l.admit(1, MINUTE_MS, 3).unwrap();
    }
}
