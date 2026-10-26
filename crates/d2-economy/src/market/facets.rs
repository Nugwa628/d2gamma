//! Filter counts (market contract 3.2): for the browser's filters, how
//! many listings each choice would show. Each facet counts the listings
//! that match the query **with that facet's own filter left out**, so a
//! list shows where items are before the player picks.
//!
//! One grouped pass over the listings that match the query's other
//! filters (by group, ethereal, socketed, identified, price flags and
//! item type) gives every facet: each row adds to a facet when it matches
//! all the faceted filters but that one, and an item type adds to every
//! type above it in `market_type_tree`. Answers are kept for 10 seconds
//! per query, and dropped whenever this connection changes the economy.

use std::cell::RefCell;
use std::collections::HashMap;

use rusqlite::params_from_iter;

use crate::db::Economy;
use crate::error::{Error, Refusal};
use crate::key::EconomyKey;
use crate::market::search::{query_sql, MarketQuery, MarketSort, PriceMode};
use crate::stash::ItemFacts;

/// The quality group the browser lists and groups by (`Group`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    /// Low, normal or superior without a runeword.
    #[default]
    Other = 0,
    Magic = 1,
    Rare = 2,
    Crafted = 3,
    Set = 4,
    Unique = 5,
    Runeword = 6,
    /// Runes, gems and the realm's other stackables.
    Stackable = 7,
}

impl Group {
    pub const ALL: [Group; 8] = [
        Group::Other,
        Group::Magic,
        Group::Rare,
        Group::Crafted,
        Group::Set,
        Group::Unique,
        Group::Runeword,
        Group::Stackable,
    ];

    /// The grid's order.
    pub const GRID: [Group; 8] = [
        Group::Unique,
        Group::Set,
        Group::Runeword,
        Group::Rare,
        Group::Crafted,
        Group::Magic,
        Group::Other,
        Group::Stackable,
    ];

    pub fn from_u8(g: u8) -> Option<Group> {
        Group::ALL.get(usize::from(g)).copied()
    }

    /// The group of an item: a runeword whatever its quality, else by the
    /// save format's quality (4 magic, 5 set, 6 rare, 7 unique, 8 crafted;
    /// the rest other).
    pub fn of(facts: &ItemFacts) -> Group {
        if facts.runeword {
            return Group::Runeword;
        }
        match facts.quality {
            4 => Group::Magic,
            5 => Group::Set,
            6 => Group::Rare,
            7 => Group::Unique,
            8 => Group::Crafted,
            _ => Group::Other,
        }
    }

    /// Its place in [`Group::GRID`].
    pub fn grid_order(self) -> u8 {
        Group::GRID.iter().position(|g| *g == self).unwrap_or(7) as u8
    }
}

/// Most type codes a facet answer lists (the wire's bound).
pub const MAX_TYPES: usize = 400;

/// The counts of each filter (`MarketFacets`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facets {
    /// Listings per group, every group with at least one, in group order.
    pub groups: Vec<(Group, u32)>,
    /// Listings per item type, counted at each ancestor too: every type
    /// with at least one listing (the most-used [`MAX_TYPES`]), by code.
    pub types: Vec<(String, u32)>,
    /// (yes, no).
    pub ethereal: [u32; 2],
    pub socketed: [u32; 2],
    pub identified: [u32; 2],
    /// Per [`PriceMode`], in its order (`Any` counts every listing).
    pub price_modes: [u32; 5],
}

/// How long a facet answer is kept.
const KEEP_MS: i64 = 10_000;

/// Facet answers per (economy, query without its page and sort), for
/// [`KEEP_MS`].
#[derive(Debug, Default)]
pub(crate) struct FacetCache(RefCell<HashMap<(String, MarketQuery), (i64, Facets)>>);

impl FacetCache {
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    fn get(&self, key: &(String, MarketQuery), now: i64) -> Option<Facets> {
        let all = self.0.borrow();
        let (at, f) = all.get(key)?;
        (now >= *at && now - at < KEEP_MS).then(|| f.clone())
    }

    fn put(&self, key: (String, MarketQuery), now: i64, f: &Facets) {
        let mut all = self.0.borrow_mut();
        if all.len() >= 256 {
            all.retain(|_, (at, _)| now >= *at && now - *at < KEEP_MS);
        }
        all.insert(key, (now, f.clone()));
    }
}

/// One row of the grouped pass: group, ethereal, socketed, identified,
/// price flags, item type, and how many listings have them.
type Combo = (u8, bool, bool, bool, i64, String, u32);

/// The faceted filters, in the order [`Economy::count_facets`] checks them.
const GROUPS: usize = 0;
const TYPES: usize = 1;
const ETHEREAL: usize = 2;
const SOCKETED: usize = 3;
const IDENTIFIED: usize = 4;
const PRICE_MODE: usize = 5;

impl Economy {
    /// The filter counts for `q` in `economy` at `now` (market contract
    /// 3.2). Its page, page size and sort don't change them.
    pub fn facets(&self, economy: &EconomyKey, q: &MarketQuery, now: i64) -> Result<Facets, Error> {
        if !self.config().market {
            return Err(Refusal::MarketOff.into());
        }
        self.check_key(economy)?;
        q.check()?;
        let split = economy.split();
        let key = (
            split.clone(),
            MarketQuery {
                page: 0,
                page_size: 0,
                sort: MarketSort::Date,
                desc: false,
                sort_currency: None,
                group_first: false,
                ..q.clone()
            },
        );
        if let Some(f) = self.facet_cache().get(&key, now) {
            return Ok(f);
        }
        let f = self.count_facets(&split, q, now)?;
        self.facet_cache().put(key, now, &f);
        Ok(f)
    }

    fn count_facets(&self, split: &str, q: &MarketQuery, now: i64) -> Result<Facets, Error> {
        let tree = self.type_tree()?;
        let mut out = Facets::default();
        let mut groups = [0u32; 8];
        let mut types: HashMap<&str, u32> = HashMap::new();
        let combos = self.combos(q, split, now)?;
        for (grp, eth, sock, ident, flags, item_type, n) in &combos {
            let (n, flags) = (*n, *flags);
            let above = tree.get(item_type.as_str()).map_or(&[][..], Vec::as_slice);
            let matches = [
                q.groups.is_empty() || q.groups.iter().any(|g| *g as u8 == *grp),
                q.types.is_empty() || above.iter().any(|t| q.types.contains(t)),
                q.filter.ethereal.is_none_or(|e| e == *eth),
                q.socketed.is_none_or(|s| s == *sock),
                q.identified.is_none_or(|i| i == *ident),
                q.price_mode.matches(flags),
            ];
            // Counted for facet `f` when every other faceted filter matches.
            let counts_for = |f: usize| matches.iter().enumerate().all(|(i, m)| i == f || *m);
            if counts_for(GROUPS) {
                groups[usize::from((*grp).min(7))] += n;
            }
            if counts_for(TYPES) {
                for t in above {
                    *types.entry(t.as_str()).or_default() += n;
                }
            }
            if counts_for(ETHEREAL) {
                out.ethereal[usize::from(!eth)] += n;
            }
            if counts_for(SOCKETED) {
                out.socketed[usize::from(!sock)] += n;
            }
            if counts_for(IDENTIFIED) {
                out.identified[usize::from(!ident)] += n;
            }
            if counts_for(PRICE_MODE) {
                for (i, m) in PriceMode::ALL.iter().enumerate() {
                    if m.matches(flags) {
                        out.price_modes[i] += n;
                    }
                }
            }
        }
        out.groups = Group::ALL
            .into_iter()
            .zip(groups)
            .filter(|(_, n)| *n > 0)
            .collect();
        // The most-used types, by code.
        let mut types: Vec<(String, u32)> =
            types.into_iter().map(|(t, n)| (t.to_owned(), n)).collect();
        types.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        types.truncate(MAX_TYPES);
        types.sort();
        out.types = types;
        Ok(out)
    }

    /// The grouped pass over the listings matching `q`'s filters other
    /// than the faceted ones.
    fn combos(&self, q: &MarketQuery, split: &str, now: i64) -> Result<Vec<Combo>, Error> {
        let sql = query_sql(q, split, now, false);
        let mut st = self.conn().prepare(&format!(
            "SELECT m.grp, m.ethereal, m.sockets > 0, m.identified, m.price_flags, m.item_type, \
             COUNT(*) FROM market_items m{} GROUP BY 1, 2, 3, 4, 5, 6",
            sql.where_clause()
        ))?;
        let rows = st.query_map(params_from_iter(sql.params.iter()), |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Each item type listed so far and the types it is under (itself
    /// included).
    fn type_tree(&self) -> Result<HashMap<String, Vec<String>>, Error> {
        let mut st = self
            .conn()
            .prepare_cached("SELECT item_type, type FROM market_type_tree")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut tree: HashMap<String, Vec<String>> = HashMap::new();
        for row in rows {
            let (item_type, t) = row?;
            tree.entry(item_type).or_default().push(t);
        }
        Ok(tree)
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn groups_of_facts_and_grid_order() {
        let facts = |quality, runeword| ItemFacts {
            quality,
            runeword,
            ..ItemFacts::default()
        };
        let got: Vec<Group> = (1..=8).map(|q| Group::of(&facts(q, false))).collect();
        use Group::*;
        assert_eq!(
            got,
            [Other, Other, Other, Magic, Set, Rare, Unique, Crafted]
        );
        assert_eq!(Group::of(&facts(2, true)), Runeword);
        let order: Vec<u8> = Group::GRID.iter().map(|g| *g as u8).collect();
        assert_eq!(order, [5, 4, 6, 2, 3, 1, 0, 7]);
        for (i, g) in Group::GRID.iter().enumerate() {
            assert_eq!(usize::from(g.grid_order()), i);
            assert_eq!(Group::from_u8(*g as u8), Some(*g));
        }
        assert_eq!(Group::from_u8(8), None);
    }
}
