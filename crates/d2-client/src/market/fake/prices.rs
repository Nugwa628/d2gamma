// D2RUST's own: the fake market's sales and price checks.
//! The sales [`super::FakeSource`] remembers and its answer to a price
//! check, with the contract's rules (3.4): the price key of an item (the
//! unique, set or runeword name; else the base code and group; a
//! stackable's code), one series per set of currencies a sale was paid in,
//! sales with items counted apart, buckets of an hour for two days or less
//! and of a day otherwise (index 0 the newest), what is listed now per
//! series, and the last ten sales. Nothing is converted between
//! currencies.
//!
//! The made-up history gives each quality group its own kind of trade, so
//! tests and shots see every case: uniques sell for gold, one rune and
//! bundles (and now and then with items), set items for one rune only,
//! runewords and stackables for gold only, and the rest so seldom that the
//! check has too little to draw. A buy on the fake adds its sale. A stash
//! item the sell form checks gets made-up sales of its own when its key
//! has none (at least [`STASH_SALES`], so the form's check has a graph).

use std::collections::BTreeMap;

use crate::market::prices::{Bucket, PriceHistory, PriceSeries, PriceShape, Sale, Spread, DAY_MS};
use crate::market::source::{Goods, Group, ItemView, ListingRow, Price};

use super::browse::NOW_MS;

/// The most recent sales an answer lists.
pub const RECENT: usize = 10;

/// The longest span a check takes.
pub const MAX_DAYS: u16 = 90;

/// One sale the fake remembers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeSale {
    pub key: String,
    /// The name the check shows.
    pub label: String,
    pub at_ms: u64,
    /// The price paid.
    pub price: Price,
}

/// The price key of a listing (contract 3.4).
pub fn price_key(l: &ListingRow) -> String {
    match &l.goods {
        Goods::Stack { code, .. } => format!("s:{code}"),
        _ => item_key(l.group, &l.item),
    }
}

/// The price key of an item of group `g` (a stash item's too).
pub fn item_key(g: Group, item: &ItemView) -> String {
    match g {
        Group::Stackable => format!("s:{}", item.code),
        Group::Unique | Group::Set | Group::Runeword => format!("u:{}", item.name),
        g => format!("b:{}:{}", item.code, g.code()),
    }
}

/// The name a check of a listing's key shows.
pub fn label_of(l: &ListingRow) -> String {
    item_label(l.group, &l.item)
}

/// The name a check of an item's key shows: the unique, set or runeword
/// name, else the base (a stash item that names only its code: its name).
pub fn item_label(g: Group, item: &ItemView) -> String {
    match g {
        Group::Unique | Group::Set | Group::Runeword => item.name.clone(),
        _ if item.base.is_empty() || item.base == item.code => item.name.clone(),
        _ => item.base.clone(),
    }
}

/// A price's currencies, sorted ("" gold first), and the amount in each.
pub fn currencies(p: &Price) -> Vec<(String, u64)> {
    let mut v = Vec::new();
    if p.gold > 0 {
        v.push((String::new(), p.gold));
    }
    for (c, n) in &p.stacks {
        if *n > 0 {
            v.push((c.clone(), u64::from(*n)));
        }
    }
    v.sort();
    v
}

/// A small fixed random stream (the same history every run).
struct Dice(u64);

impl Dice {
    fn of(key: &str) -> Dice {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in key.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        Dice(h)
    }

    /// A number below `n`.
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n.max(1)
    }
}

/// How many sales a key of group `g` made in 90 days.
pub fn sales_of(g: Group) -> usize {
    match g {
        Group::Unique => 40,
        Group::Set => 18,
        Group::Runeword => 24,
        Group::Stackable => 30,
        _ => 3,
    }
}

/// The made-up price of a sale of group `g`.
fn sale_price(g: Group, d: &mut Dice) -> Price {
    let stacks = |v: &[(&str, u64)]| Price {
        stacks: v
            .iter()
            .map(|(c, n)| ((*c).to_owned(), *n as u32))
            .collect(),
        ..Price::default()
    };
    match g {
        Group::Unique => match d.below(10) {
            0..=3 => stacks(&[("r24", 1 + d.below(3))]),
            4..=6 => Price::gold(150_000 + 10_000 * d.below(30)),
            7 | 8 => stacks(&[("r23", 1 + d.below(2)), ("r24", 1)]),
            _ => Price {
                gold: 100_000,
                items: vec![9000 + d.below(100) as u32],
                ..Price::default()
            },
        },
        Group::Set => stacks(&[("r23", 1 + d.below(3))]),
        Group::Runeword => Price::gold(400_000 + 25_000 * d.below(16)),
        Group::Stackable => Price::gold(20_000 + 1_000 * d.below(20)),
        _ => Price::gold(5_000 + 1_000 * d.below(40)),
    }
}

/// The fake's sales: for every key its listings have, made-up sales over
/// the last 90 days, more of them lately.
pub fn sales(listings: &[ListingRow]) -> Vec<FakeSale> {
    let mut keys: BTreeMap<String, (String, Group)> = BTreeMap::new();
    for l in listings {
        keys.entry(price_key(l))
            .or_insert_with(|| (label_of(l), l.group));
    }
    keys.into_iter()
        .flat_map(|(key, (label, g))| made_up(&key, &label, g, sales_of(g)))
        .collect()
}

/// The fewest made-up sales a stash item's check gets over 90 days: enough
/// for a graph over 30.
pub const STASH_SALES: usize = 20;

/// `n` made-up sales of `key` (group `g`) over the last 90 days, more of
/// them lately; the same every run.
pub fn made_up(key: &str, label: &str, g: Group, n: usize) -> Vec<FakeSale> {
    let span = u64::from(MAX_DAYS) * DAY_MS;
    let mut d = Dice::of(key);
    (0..n)
        .map(|_| {
            // The square of an even draw: recent days hold more sales
            // (none in the last minute: a buy now is the newest).
            let u = d.below(1000);
            let age = span * u * u / 1_000_000 + 60_000;
            FakeSale {
                key: key.to_owned(),
                label: label.to_owned(),
                at_ms: NOW_MS - age,
                price: sale_price(g, &mut d),
            }
        })
        .collect()
}

/// The lower median of `v` (sorted here).
fn median(v: &mut [u64]) -> u64 {
    v.sort_unstable();
    v.get(v.len().saturating_sub(1) / 2).copied().unwrap_or(0)
}

fn spread(v: &mut [u64]) -> Spread {
    let m = median(v);
    Spread {
        min: v.first().copied().unwrap_or(0),
        median: m,
        max: v.last().copied().unwrap_or(0),
    }
}

/// What series a price belongs to: its shape and currencies.
fn series_of(p: &Price) -> (PriceShape, Vec<String>) {
    if !p.items.is_empty() {
        return (PriceShape::WithItems, Vec::new());
    }
    let cur: Vec<String> = currencies(p).into_iter().map(|(c, _)| c).collect();
    let shape = if cur.len() == 1 {
        PriceShape::One
    } else {
        PriceShape::Bundle
    };
    (shape, cur)
}

/// The fake's answer to a check of `key` over `days` at `now_ms`, from its
/// `sales` and the `listings` up now.
pub fn history(
    sales: &[FakeSale],
    listings: &[ListingRow],
    key: &str,
    label: &str,
    days: u16,
    now_ms: u64,
) -> PriceHistory {
    let bucket_ms = PriceHistory::bucket_ms_for(days);
    let from = now_ms.saturating_sub(u64::from(days) * DAY_MS);
    let mut sold: Vec<&FakeSale> = sales
        .iter()
        .filter(|s| s.key == key && s.at_ms > from && s.at_ms <= now_ms)
        .collect();
    sold.sort_by_key(|s| std::cmp::Reverse(s.at_ms));
    let listed: Vec<&ListingRow> = listings.iter().filter(|l| price_key(l) == key).collect();

    // Sales by series, then by bucket: per currency the amounts paid.
    type Amounts = BTreeMap<u32, (u16, Vec<Vec<u64>>)>;
    let mut by: BTreeMap<(u8, Vec<String>), Amounts> = BTreeMap::new();
    let shape_rank = |s: PriceShape| match s {
        PriceShape::One => 0,
        PriceShape::Bundle => 1,
        PriceShape::WithItems => 2,
    };
    for s in &sold {
        let (shape, cur) = series_of(&s.price);
        let b = ((now_ms - s.at_ms) / bucket_ms) as u32;
        let slot = by
            .entry((shape_rank(shape), cur.clone()))
            .or_default()
            .entry(b)
            .or_insert_with(|| (0, vec![Vec::new(); cur.len()]));
        slot.0 += 1;
        if shape != PriceShape::WithItems {
            for (k, (_, n)) in currencies(&s.price).into_iter().enumerate() {
                slot.1[k].push(n);
            }
        }
    }
    // Currency sets listed now but not sold in the span still show their
    // listings.
    for l in &listed {
        if let Some(p) = &l.price {
            let (shape, cur) = series_of(p);
            by.entry((shape_rank(shape), cur)).or_default();
        }
    }
    let mut series: Vec<PriceSeries> = by
        .into_iter()
        .map(|((rank, cur), buckets)| {
            let shape =
                [PriceShape::One, PriceShape::Bundle, PriceShape::WithItems][usize::from(rank)];
            let mut now: Vec<Vec<u64>> = vec![Vec::new(); cur.len()];
            let mut listed_now = 0;
            for p in listed.iter().filter_map(|l| l.price.as_ref()) {
                if series_of(p) == (shape, cur.clone()) {
                    listed_now += 1;
                    for (k, (_, n)) in currencies(p).into_iter().enumerate() {
                        now[k].push(n);
                    }
                }
            }
            let lowest_now = now
                .iter_mut()
                .filter(|v| !v.is_empty())
                .map(|v| *v.iter().min().unwrap_or(&0))
                .collect();
            let median_now = now
                .iter_mut()
                .filter(|v| !v.is_empty())
                .map(|v| median(v))
                .collect();
            let sold = buckets.values().map(|(n, _)| u32::from(*n)).sum();
            let buckets = buckets
                .into_iter()
                .map(|(index, (sales, mut vals))| Bucket {
                    index,
                    sales,
                    values: vals.iter_mut().map(|v| spread(v)).collect(),
                })
                .collect();
            PriceSeries {
                index: 0,
                shape,
                currencies: cur,
                sold,
                listed_now,
                lowest_now,
                median_now,
                buckets,
            }
        })
        .collect();
    // Most sold first; sales with items last.
    series.sort_by(|a, b| {
        (a.shape == PriceShape::WithItems)
            .cmp(&(b.shape == PriceShape::WithItems))
            .then(b.sold.cmp(&a.sold))
            .then(b.listed_now.cmp(&a.listed_now))
            .then(a.currencies.cmp(&b.currencies))
    });
    for (i, s) in series.iter_mut().enumerate() {
        s.index = i as u8;
    }
    PriceHistory {
        key: key.to_owned(),
        label: label.to_owned(),
        now_ms,
        bucket_ms,
        days,
        listed_now: listed.len() as u32,
        sold: sold.len() as u32,
        series,
        recent: sold
            .iter()
            .take(RECENT)
            .map(|s| Sale {
                at_ms: s.at_ms,
                price: s.price.clone(),
            })
            .collect(),
    }
}
