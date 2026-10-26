//! Market search (N30): each filter, the sorts, the index kept with the
//! escrow, and the cost on 100,000 listings.

use super::market::{ber, market, DAY};
use super::*;

/// An item with the given facts.
fn made(
    id: u32,
    item_type: &str,
    quality: u8,
    name: &str,
    level_req: u8,
    (sockets, ethereal): (u8, bool),
    stats: &[(u16, i32)],
) -> Item {
    let mut it = item(id);
    it.facts.item_type = item_type.into();
    it.facts.quality = quality;
    it.facts.name = name.into();
    it.facts.name_parts = name.split_whitespace().map(str::to_owned).collect();
    it.facts.level_req = level_req;
    it.facts.sockets = sockets;
    it.facts.ethereal = ethereal;
    it.facts.stats = stats.to_vec();
    it
}

const SKILLS: u16 = 127;
const FCR: u16 = 105;

/// Seller 1 lists, in order (listings 1 to 5):
/// 1. a ring for 1,000 gold (quality 6, level 1, `Havoc Loop 1`, 10
///    FCR);
/// 2. Mara's Kaleidoscope (amulet, quality 7, level 67, +2 skills, 20 to
///    stat 39) for 2 Ber;
/// 3. Harlequin Crest (helm, quality 7, level 62, 1 socket, +2 skills,
///    10 FCR) for 500 gold and 1 Jah;
/// 4. an ethereal Thresher (polearm, 4 sockets), offers only;
/// 5. 2 Ber for 100,000 gold.
fn listings(e: &mut Economy) {
    let items = [
        item(1),
        made(
            2,
            "amul",
            7,
            "Mara's Kaleidoscope Amulet",
            67,
            (0, false),
            &[(SKILLS, 2), (39, 20)],
        ),
        made(
            3,
            "helm",
            7,
            "Harlequin Crest Shako",
            62,
            (1, false),
            &[(SKILLS, 2), (FCR, 10)],
        ),
        made(4, "pole", 2, "Thresher", 0, (4, true), &[]),
    ];
    for (n, it) in items.into_iter().enumerate() {
        put_item(e, 1, n as u64 + 1, it).unwrap();
    }
    put_stack(e, 1, 5, "r30", 2).unwrap();
    let posts = [
        (
            Goods::Item(ItemId(1)),
            ListingMode::Price,
            Some(Price::gold(1000)),
        ),
        (
            Goods::Item(ItemId(2)),
            ListingMode::Price,
            Some(market::price(0, vec![ber(2)])),
        ),
        (
            Goods::Item(ItemId(3)),
            ListingMode::PriceAndOffers,
            Some(market::price(500, vec![("r31".into(), 1)])),
        ),
        (Goods::Item(ItemId(4)), ListingMode::OffersOnly, None),
        (
            Goods::Stack {
                code: "r30".into(),
                count: 2,
            },
            ListingMode::Price,
            Some(Price::gold(100_000)),
        ),
    ];
    for (n, (goods, mode, price)) in posts.into_iter().enumerate() {
        e.post(
            &owner(1),
            &req(1, 10 + n as u64),
            &Post {
                seller: "Seller".into(),
                goods,
                mode,
                price,
                days: 7,
            },
            // One second apart, so the date order is the post order.
            1000 + n as i64 * 1000,
        )
        .unwrap();
    }
}

fn find(e: &Economy, q: &MarketQuery) -> (u32, Vec<i64>) {
    let p = e.search(&EconomyKey::new(MOD), None, q, 9000).unwrap();
    (p.total, p.listings.iter().map(|l| l.id).collect())
}

/// The listing IDs `f` finds, sorted by date ascending.
fn ids(e: &Economy, f: ItemFilter) -> Vec<i64> {
    find(
        e,
        &MarketQuery {
            filter: f,
            ..MarketQuery::default()
        },
    )
    .1
}

fn text(t: &str) -> ItemFilter {
    ItemFilter {
        text: Some(t.into()),
        ..ItemFilter::default()
    }
}

#[test]
fn search_by_each_filter() {
    let mut e = market();
    listings(&mut e);
    assert_eq!(ids(&e, ItemFilter::default()), [1, 2, 3, 4, 5]);
    // Names, by word prefixes, any case; FTS5 syntax is taken as words.
    assert_eq!(ids(&e, text("harle")), [3]);
    assert_eq!(ids(&e, text("KALEIDO mara")), [2]);
    assert_eq!(ids(&e, text("havoc loop")), [1]);
    assert_eq!(ids(&e, text("ber")), [5]);
    assert_eq!(ids(&e, text("rune")), [5]);
    assert_eq!(ids(&e, text("shako")), [3]);
    assert_eq!(ids(&e, text("crest NOT")), Vec::<i64>::new());
    assert_eq!(ids(&e, text("\"* ( -")), [1, 2, 3, 4, 5], "no words");
    // Facts.
    let f = |f: ItemFilter| ids(&e, f);
    assert_eq!(
        f(ItemFilter {
            item_type: Some("helm".into()),
            ..ItemFilter::default()
        }),
        [3]
    );
    assert_eq!(
        f(ItemFilter {
            quality: Some(7),
            ..ItemFilter::default()
        }),
        [2, 3]
    );
    assert_eq!(
        f(ItemFilter {
            level: Some((60, 70)),
            ..ItemFilter::default()
        }),
        [2, 3]
    );
    assert_eq!(
        f(ItemFilter {
            ethereal: Some(true),
            ..ItemFilter::default()
        }),
        [4]
    );
    assert_eq!(
        f(ItemFilter {
            sockets: Some((1, 6)),
            ..ItemFilter::default()
        }),
        [3, 4]
    );
    // Stat ranges, each one must match.
    let skills = StatRange {
        stat: SKILLS,
        min: 2,
        max: 99,
    };
    assert_eq!(
        f(ItemFilter {
            stats: vec![skills],
            ..ItemFilter::default()
        }),
        [2, 3]
    );
    assert_eq!(
        f(ItemFilter {
            stats: vec![
                skills,
                StatRange {
                    stat: FCR,
                    min: 10,
                    max: 20
                }
            ],
            ..ItemFilter::default()
        }),
        [3]
    );
    assert_eq!(
        f(ItemFilter {
            stats: vec![StatRange {
                stat: SKILLS,
                min: 3,
                max: 99
            }],
            ..ItemFilter::default()
        }),
        Vec::<i64>::new()
    );
    // Prices: gold, or a given stackable.
    let priced = |code: Option<&str>, min, max| {
        find(
            &e,
            &MarketQuery {
                price: Some(PriceFilter {
                    code: code.map(str::to_owned),
                    min,
                    max,
                }),
                ..MarketQuery::default()
            },
        )
        .1
    };
    assert_eq!(priced(None, 400, 1000), [1, 3]);
    assert_eq!(priced(None, 0, u64::MAX), [1, 3, 5]);
    assert_eq!(priced(Some("r30"), 1, 2), [2]);
    assert_eq!(priced(Some("r31"), 1, 1), [3]);
    assert_eq!(priced(Some("r30"), 3, 9), Vec::<i64>::new());
}

#[test]
fn search_sorts_and_pages() {
    let mut e = market();
    listings(&mut e);
    let sorted = |sort, desc, price: Option<&str>| {
        find(
            &e,
            &MarketQuery {
                sort,
                desc,
                price: price.map(|c| PriceFilter {
                    code: Some(c.into()),
                    min: 0,
                    max: u64::MAX,
                }),
                ..MarketQuery::default()
            },
        )
    };
    assert_eq!(
        sorted(MarketSort::Date, true, None),
        (5, vec![5, 4, 3, 2, 1])
    );
    // By price in gold: only listings with gold in their price.
    assert_eq!(sorted(MarketSort::Price, false, None), (3, vec![3, 1, 5]));
    assert_eq!(sorted(MarketSort::Price, true, None), (3, vec![5, 1, 3]));
    assert_eq!(sorted(MarketSort::Price, false, Some("r30")), (1, vec![2]));
    // By a stat: ties by listing, the same way.
    assert_eq!(
        sorted(MarketSort::Stat(SKILLS), true, None),
        (2, vec![3, 2])
    );
    assert_eq!(sorted(MarketSort::Stat(FCR), true, None), (2, vec![3, 1]));

    // Pages of 50, never overlapping (40 listings each from accounts 2 to
    // 4, under `market_max_listings`).
    for n in 0..120u32 {
        let (id, account) = (100 + n, 2 + i64::from(n / 40));
        put_item(&mut e, account, u64::from(id), item(id)).unwrap();
        e.post(
            &owner(account),
            &req(account, u64::from(id) + 1000),
            &Post {
                seller: "Other".into(),
                goods: Goods::Item(ItemId(id)),
                mode: ListingMode::Price,
                price: Some(Price::gold(u64::from(n % 7) + 1)),
                days: 7,
            },
            // Under the posts-per-minute limit.
            10_000 + i64::from(n) * 6_000,
        )
        .unwrap();
    }
    let mut seen = std::collections::BTreeSet::new();
    for page in 0..3 {
        let p = e
            .search(
                &EconomyKey::new(MOD),
                None,
                &MarketQuery {
                    sort: MarketSort::Price,
                    page,
                    ..MarketQuery::default()
                },
                900_000,
            )
            .unwrap();
        assert_eq!(p.total, 123);
        assert_eq!(p.listings.len(), [50, 50, 23][usize::from(page)]);
        for l in &p.listings {
            assert!(seen.insert(l.id), "listing {} on two pages", l.id);
        }
    }
    assert_eq!(seen.len(), 123);
}

#[test]
fn index_follows_escrow() {
    let mut e = market();
    listings(&mut e);
    let rows = |e: &Economy| -> i64 {
        e.conn()
            .query_row(
                "SELECT (SELECT COUNT(*) FROM market_items) + (SELECT COUNT(*) FROM market_prices) \
                 + (SELECT COUNT(*) FROM market_stats) + (SELECT COUNT(*) FROM market_names)",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    // 5 items, 2 price stacks, 6 stats, 5 names.
    assert_eq!(rows(&e), 18);
    // Sold, cancelled and expired listings leave the index in the same
    // transaction.
    put_gold(&mut e, 2, 1, 1000).unwrap();
    e.buy(&owner(2), &req(2, 2), 1, &Price::gold(1000).hash(), 9000)
        .unwrap();
    e.cancel(&owner(1), &req(1, 30), 3, 9000).unwrap();
    assert_eq!(ids(&e, ItemFilter::default()), [2, 4, 5]);
    assert_eq!(ids(&e, text("harlequin")), Vec::<i64>::new());
    assert_eq!(rows(&e), 18 - 4 - 5);
    assert_eq!(e.expire(1000 + 8 * DAY).unwrap(), 3);
    assert_eq!(rows(&e), 0);
    // A refused post leaves no index rows.
    let r = e.post(
        &owner(1),
        &req(1, 31),
        &Post {
            seller: "Seller".into(),
            goods: Goods::Item(ItemId(9)),
            mode: ListingMode::Price,
            price: Some(Price::gold(1)),
            days: 7,
        },
        9 * DAY,
    );
    assert!(r.is_err());
    assert_eq!(rows(&e), 0);
}

#[test]
fn search_keys_switch_and_limit() {
    let mut e = Economy::in_memory(
        MOD,
        EconomyConfig {
            market_searches_per_min: 3,
            ..config()
        },
    )
    .unwrap();
    listings(&mut e);
    // Economies stay apart, and another mod is refused.
    let hc = EconomyKey {
        hardcore: true,
        ..EconomyKey::new(MOD)
    };
    let q = MarketQuery::default();
    assert_eq!(e.search(&hc, None, &q, 9000).unwrap().total, 0);
    assert_eq!(
        e.search(&EconomyKey::new("pd2"), None, &q, 9000)
            .unwrap_err()
            .refusal(),
        Some(Refusal::EconomyMismatch)
    );
    // Searches per minute, per account.
    let key = EconomyKey::new(MOD);
    for t in 0..3 {
        e.search(&key, Some(AccountId(7)), &q, 10_000 + t).unwrap();
    }
    assert_eq!(
        e.search(&key, Some(AccountId(7)), &q, 10_003)
            .unwrap_err()
            .refusal(),
        Some(Refusal::RateLimited)
    );
    e.search(&key, Some(AccountId(8)), &q, 10_003).unwrap();
    e.search(&key, None, &q, 10_003).unwrap();
    e.search(&key, Some(AccountId(7)), &q, 70_000).unwrap();
    // The market switch.
    let off = Economy::in_memory(
        MOD,
        EconomyConfig {
            market: false,
            ..config()
        },
    )
    .unwrap();
    assert_eq!(
        off.search(&key, None, &q, 0).unwrap_err().refusal(),
        Some(Refusal::MarketOff)
    );
}

/// The plan's cost check: searches over 100,000 listings, each under
/// 50 ms in a release build on the CI server. Debug builds index fewer
/// listings and only check the results.
#[test]
fn search_100k_under_50ms() {
    let n: u32 = if cfg!(debug_assertions) {
        20_000
    } else {
        100_000
    };
    let mut e = market();
    let split = EconomyKey::new(MOD).split();
    let types = [
        "ring", "amul", "helm", "armo", "weap", "boot", "glov", "belt",
    ];
    let words = [
        "Havoc", "Loop", "Bramble", "Crest", "Shako", "Storm", "Eagle", "Wraith", "Grim", "Dread",
        "Doom", "Spirit", "Raven", "Frost", "Blood", "Coil", "Viper", "Horn", "Gale", "Rune",
    ];
    let build = std::time::Instant::now();
    e.sweep(0, |a| {
        // A fixed pseudo-random sequence, the same on every run.
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = |m: u64| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x % m
        };
        for i in 1..=n {
            let r = |v: u64| v as usize;
            let name = format!(
                "{} {} {}",
                words[r(next(20))],
                words[r(next(20))],
                types[r(next(8))]
            );
            let stats: Vec<(u16, i32)> = (0..5)
                .map(|k| (k * 7 + next(7) as u16, next(100) as i32))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .collect();
            let mut it = made(
                i,
                types[r(next(8))],
                next(8) as u8 + 1,
                &name,
                next(90) as u8,
                (next(7) as u8, next(10) == 0),
                &stats,
            );
            it.record = vec![0; 32];
            a.insert_item(&it, 1, &split, Place::Stash, None)?;
            let price = if next(4) == 0 {
                market::price(0, vec![ber(next(5) as u32 + 1)])
            } else {
                Price::gold(next(1_000_000) + 1)
            };
            a.exec(
                "INSERT INTO listings(account, economy, seller, item_id, mode, price, price_gold, \
                 state, posted, expires) \
                 VALUES (1, ?1, 'Seller', ?2, 0, ?3, ?4, 'active', ?5, ?6)",
                rusqlite::params![
                    split,
                    i,
                    price.encode(),
                    price.gold as i64,
                    i64::from(i),
                    100 * DAY
                ],
            )?;
            let listing = a.conn.last_insert_rowid();
            a.exec(
                "UPDATE econ_items SET place = 'listing', place_id = ?2 WHERE item_id = ?1",
                rusqlite::params![i, listing],
            )?;
            a.index_listing(listing)?;
        }
        Ok(())
    })
    .unwrap();
    let build = build.elapsed();

    let key = EconomyKey::new(MOD);
    let stat = |stat, min| StatRange {
        stat,
        min,
        max: 1000,
    };
    let queries: Vec<(&str, MarketQuery)> = vec![
        (
            "newest",
            MarketQuery {
                desc: true,
                ..MarketQuery::default()
            },
        ),
        (
            "deep page",
            MarketQuery {
                desc: true,
                page: 199,
                ..MarketQuery::default()
            },
        ),
        (
            "name",
            MarketQuery {
                filter: text("storm eag"),
                ..MarketQuery::default()
            },
        ),
        (
            "type and quality",
            MarketQuery {
                filter: ItemFilter {
                    item_type: Some("helm".into()),
                    quality: Some(7),
                    ..ItemFilter::default()
                },
                ..MarketQuery::default()
            },
        ),
        (
            "two stats by price",
            MarketQuery {
                filter: ItemFilter {
                    stats: vec![stat(1, 50), stat(8, 50)],
                    ..ItemFilter::default()
                },
                sort: MarketSort::Price,
                ..MarketQuery::default()
            },
        ),
        (
            "gold range by price",
            MarketQuery {
                price: Some(PriceFilter {
                    code: None,
                    min: 1000,
                    max: 50_000,
                }),
                sort: MarketSort::Price,
                ..MarketQuery::default()
            },
        ),
        (
            "cheapest in Ber",
            MarketQuery {
                price: Some(PriceFilter {
                    code: Some("r30".into()),
                    min: 0,
                    max: 99,
                }),
                sort: MarketSort::Price,
                ..MarketQuery::default()
            },
        ),
        (
            "best of a stat",
            MarketQuery {
                sort: MarketSort::Stat(15),
                desc: true,
                ..MarketQuery::default()
            },
        ),
        (
            "level, sockets, stat, name",
            MarketQuery {
                filter: ItemFilter {
                    text: Some("grim".into()),
                    level: Some((30, 80)),
                    sockets: Some((2, 6)),
                    stats: vec![stat(22, 20)],
                    ..ItemFilter::default()
                },
                ..MarketQuery::default()
            },
        ),
    ];
    let mut report = Vec::new();
    let mut slowest = std::time::Duration::ZERO;
    for (what, q) in &queries {
        let mut best = std::time::Duration::MAX;
        let mut page = None;
        // The best of three: the first run also warms the page cache.
        for _ in 0..3 {
            let t = std::time::Instant::now();
            let p = e.search(&key, None, q, 1000).unwrap();
            best = best.min(t.elapsed());
            page = Some(p);
        }
        let p = page.unwrap();
        assert!(p.total > 0, "{what}: nothing found");
        assert!(p.listings.len() <= PAGE as usize);
        if *what == "newest" {
            assert_eq!(p.total, n);
            assert_eq!(p.listings[0].id, i64::from(n));
        }
        slowest = slowest.max(best);
        report.push(format!("{what}: {} results, {best:?}", p.total));
    }
    println!(
        "{n} listings indexed in {build:?}; slowest search {slowest:?}\n  {}",
        report.join("\n  ")
    );
    if !cfg!(debug_assertions) {
        assert!(
            slowest < std::time::Duration::from_millis(50),
            "slowest search {slowest:?}: {report:?}"
        );
    }
}
