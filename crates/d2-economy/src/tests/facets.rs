//! The market window's search (market contract 3.1, 3.2): groups, the
//! type tree, the tri-state flags, price modes and caps, the facet counts
//! that leave out their own filter, the grid order and page sizes, hidden
//! listings, and the cost on 100,000 listings.

use super::market::{ber, refused, DAY};
use super::*;

/// The test realm with runes and gems in the type tree, and room for many
/// posts.
pub(crate) fn shelf_config() -> EconomyConfig {
    let mut c = config();
    c.stackables = c
        .stackables
        .into_iter()
        .map(|s| {
            let t = if s.code.starts_with('r') {
                "rune"
            } else {
                "gem"
            };
            s.with_types(&[t, "misc"])
        })
        .collect();
    c.market_posts_per_min = 1000;
    c.market_max_listings = 100;
    c
}

/// An item: `types` is its type and the ones above it.
pub(crate) fn thing(id: u32, name: &str, types: &[&str], quality: u8) -> Item {
    let mut it = item(id);
    it.facts.name = name.into();
    it.facts.name_parts = name.split_whitespace().map(str::to_owned).collect();
    it.facts.code = format!("c{id:02}");
    it.facts.item_type = types[0].into();
    it.facts.types = types.iter().map(|t| (*t).to_owned()).collect();
    it.facts.quality = quality;
    it
}

/// Seller 1 lists, in order (listings 1 to 6):
/// 1. Harlequin Crest (unique helm, 1 socket) for 1,000 gold;
/// 2. Raven Frost (unique ring) for 2 Ber;
/// 3. Grim Circlet (rare circlet under helm, ethereal, unidentified),
///    offers only;
/// 4. Lucky Boots (magic) for 500 gold and 1 Jah, or offers;
/// 5. Spirit (a runeword sword, 4 sockets) for 300,000 gold;
/// 6. 2 Ber for 100,000 gold.
pub(crate) fn shelf(e: &mut Economy) {
    let mut circlet = thing(3, "Grim Circlet", &["circ", "helm", "armo"], 6);
    circlet.facts.ethereal = true;
    circlet.facts.identified = false;
    let mut crest = thing(1, "Harlequin Crest", &["helm", "armo"], 7);
    crest.facts.sockets = 1;
    let mut spirit = thing(5, "Spirit", &["swor", "weap"], 2);
    spirit.facts.runeword = true;
    spirit.facts.sockets = 4;
    let items = [
        crest,
        thing(2, "Raven Frost", &["ring", "misc"], 7),
        circlet,
        thing(4, "Lucky Boots", &["boot", "armo"], 4),
        spirit,
    ];
    for (n, it) in items.into_iter().enumerate() {
        put_item(e, 1, n as u64 + 1, it).unwrap();
    }
    put_stack(e, 1, 6, "r30", 2).unwrap();
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
        (Goods::Item(ItemId(3)), ListingMode::OffersOnly, None),
        (
            Goods::Item(ItemId(4)),
            ListingMode::PriceAndOffers,
            Some(market::price(500, vec![("r31".into(), 1)])),
        ),
        (
            Goods::Item(ItemId(5)),
            ListingMode::Price,
            Some(Price::gold(300_000)),
        ),
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
        let l = e
            .post(
                &owner(1),
                &req(1, 10 + n as u64),
                &Post {
                    seller: "Seller".into(),
                    goods,
                    mode,
                    price,
                    days: 7,
                },
                1000 + n as i64 * 1000,
            )
            .unwrap();
        assert_eq!(l.id, Some(n as i64 + 1));
    }
}

pub(crate) fn shelf_econ() -> Economy {
    let mut e = Economy::in_memory(MOD, shelf_config()).unwrap();
    shelf(&mut e);
    e
}

const NOW: i64 = 10_000;

/// The listing IDs `q` finds (all pages of 100), in its order.
pub(crate) fn found(e: &Economy, q: &MarketQuery) -> Vec<i64> {
    let p = e
        .search(
            &EconomyKey::new(MOD),
            None,
            &MarketQuery {
                page_size: 100,
                ..q.clone()
            },
            NOW,
        )
        .unwrap();
    p.listings.iter().map(|l| l.id).collect()
}

fn facets(e: &Economy, q: &MarketQuery) -> Facets {
    e.facets(&EconomyKey::new(MOD), q, NOW).unwrap()
}

fn types(t: &[&str]) -> MarketQuery {
    MarketQuery {
        types: t.iter().map(|s| (*s).to_owned()).collect(),
        ..MarketQuery::default()
    }
}

fn counts(list: &[(&str, u32)]) -> Vec<(String, u32)> {
    list.iter().map(|(t, n)| ((*t).to_owned(), *n)).collect()
}

#[test]
fn facets_leave_out_their_own_filter() {
    let mut e = shelf_econ();
    let all = facets(&e, &MarketQuery::default());
    assert_eq!(
        all.groups,
        vec![
            (Group::Magic, 1),
            (Group::Rare, 1),
            (Group::Unique, 2),
            (Group::Runeword, 1),
            (Group::Stackable, 1),
        ]
    );
    assert_eq!(all.price_modes[0], 6, "Any counts every listing");

    // Unique only: the group counts still show every group; the others
    // count the uniques.
    let uniques = MarketQuery {
        groups: vec![Group::Unique],
        ..MarketQuery::default()
    };
    assert_eq!(found(&e, &uniques), [1, 2]);
    let f = facets(&e, &uniques);
    assert_eq!(f.groups, all.groups);
    assert_eq!(f.ethereal, [0, 2]);
    assert_eq!(f.socketed, [1, 1]);
    assert_eq!(f.identified, [2, 0]);
    assert_eq!(f.price_modes, [2, 2, 0, 1, 1]);
    assert_eq!(
        f.types,
        counts(&[("armo", 1), ("helm", 1), ("misc", 1), ("ring", 1)])
    );

    // Unique and ethereal: nothing found, but each facet shows what its
    // own choice would give.
    let both = MarketQuery {
        filter: ItemFilter {
            ethereal: Some(true),
            ..ItemFilter::default()
        },
        ..uniques.clone()
    };
    assert_eq!(found(&e, &both), Vec::<i64>::new());
    let f = facets(&e, &both);
    assert_eq!(f.groups, vec![(Group::Rare, 1)], "ethereal ones by group");
    assert_eq!(f.ethereal, [0, 2], "uniques by ethereal");
    assert_eq!(f.socketed, [0, 0]);
    assert_eq!(f.types, vec![]);

    // A sort or a page doesn't change the counts; a commit clears the
    // kept answers.
    let sorted = MarketQuery {
        sort: MarketSort::Name,
        desc: true,
        page: 3,
        ..uniques.clone()
    };
    assert_eq!(facets(&e, &sorted), facets(&e, &uniques));
    put_item(
        &mut e,
        1,
        30,
        thing(9, "Kira's Guardian", &["helm", "armo"], 7),
    )
    .unwrap();
    super::market::post_item(&mut e, 1, 31, 9, Price::gold(5)).unwrap();
    assert_eq!(facets(&e, &uniques).groups[2], (Group::Unique, 3));

    // Off, another mod, a bad page size.
    let key = EconomyKey::new(MOD);
    let bad = MarketQuery {
        page_size: 30,
        ..MarketQuery::default()
    };
    assert_eq!(
        e.facets(&key, &bad, NOW).unwrap_err().refusal(),
        Some(Refusal::BadRequest)
    );
    assert_eq!(
        e.facets(&EconomyKey::new("pd2"), &uniques, NOW)
            .unwrap_err()
            .refusal(),
        Some(Refusal::EconomyMismatch)
    );
    let off = Economy::in_memory(MOD, config_off()).unwrap();
    assert_eq!(
        off.facets(&key, &uniques, NOW).unwrap_err().refusal(),
        Some(Refusal::MarketOff)
    );
}

fn config_off() -> EconomyConfig {
    EconomyConfig {
        market: false,
        ..shelf_config()
    }
}

#[test]
fn type_node_matches_descendants() {
    let e = shelf_econ();
    assert_eq!(found(&e, &types(&["armo"])), [1, 3, 4]);
    assert_eq!(found(&e, &types(&["helm"])), [1, 3]);
    assert_eq!(found(&e, &types(&["circ"])), [3]);
    assert_eq!(found(&e, &types(&["rune"])), [6], "a stackable's types");
    assert_eq!(found(&e, &types(&["misc"])), [2, 6]);
    assert_eq!(found(&e, &types(&["helm", "weap"])), [1, 3, 5]);
    assert_eq!(found(&e, &types(&["nope"])), Vec::<i64>::new());
    // The tree's counts leave out the type filter; the others don't.
    let f = facets(&e, &types(&["helm"]));
    assert_eq!(
        f.types,
        counts(&[
            ("armo", 3),
            ("boot", 1),
            ("circ", 1),
            ("helm", 2),
            ("misc", 2),
            ("ring", 1),
            ("rune", 1),
            ("swor", 1),
            ("weap", 1),
        ])
    );
    assert_eq!(f.groups, vec![(Group::Rare, 1), (Group::Unique, 1)]);
    // Type and quality from the item filter still apply as before.
    let q = MarketQuery {
        filter: ItemFilter {
            item_type: Some("circ".into()),
            ..ItemFilter::default()
        },
        ..types(&["armo"])
    };
    assert_eq!(found(&e, &q), [3]);
    // Too many types, or too long a code.
    let many: Vec<String> = (0..33).map(|i| format!("t{i}")).collect();
    for bad in [
        MarketQuery {
            types: many,
            ..MarketQuery::default()
        },
        types(&["a-type-code-too-long"]),
    ] {
        assert_eq!(
            e.search(&EconomyKey::new(MOD), None, &bad, NOW)
                .unwrap_err()
                .refusal(),
            Some(Refusal::BadRequest)
        );
    }
}

#[test]
fn tri_state_flags() {
    let e = shelf_econ();
    let q = |socketed, identified, ethereal| MarketQuery {
        socketed,
        identified,
        filter: ItemFilter {
            ethereal,
            ..ItemFilter::default()
        },
        ..MarketQuery::default()
    };
    assert_eq!(found(&e, &q(None, None, None)), [1, 2, 3, 4, 5, 6]);
    assert_eq!(found(&e, &q(Some(true), None, None)), [1, 5]);
    assert_eq!(found(&e, &q(Some(false), None, None)), [2, 3, 4, 6]);
    assert_eq!(found(&e, &q(None, Some(false), None)), [3]);
    assert_eq!(found(&e, &q(None, Some(true), None)), [1, 2, 4, 5, 6]);
    assert_eq!(found(&e, &q(None, None, Some(true))), [3]);
    assert_eq!(found(&e, &q(None, None, Some(false))), [1, 2, 4, 5, 6]);
    assert_eq!(found(&e, &q(Some(false), Some(true), None)), [2, 4, 6]);
    let f = facets(&e, &MarketQuery::default());
    assert_eq!(f.ethereal, [1, 5]);
    assert_eq!(f.socketed, [2, 4]);
    assert_eq!(f.identified, [5, 1]);
    // Each flag's counts leave out its own choice, and keep the others.
    let f = facets(&e, &q(Some(false), Some(true), None));
    assert_eq!(f.socketed, [2, 3], "identified ones by sockets");
    assert_eq!(f.identified, [3, 1], "socketless ones by identified");
    assert_eq!(f.ethereal, [0, 3]);
}

#[test]
fn price_modes_and_caps() {
    let e = shelf_econ();
    let mode = |m| MarketQuery {
        price_mode: m,
        ..MarketQuery::default()
    };
    assert_eq!(found(&e, &mode(PriceMode::HasPrice)), [1, 2, 4, 5, 6]);
    assert_eq!(found(&e, &mode(PriceMode::TakesOffers)), [3, 4]);
    assert_eq!(found(&e, &mode(PriceMode::CurrenciesOnly)), [2]);
    assert_eq!(found(&e, &mode(PriceMode::GoldOnly)), [1, 5, 6]);
    let f = facets(&e, &mode(PriceMode::GoldOnly));
    assert_eq!(f.price_modes, [6, 5, 2, 1, 3], "left out of its own count");
    assert_eq!(
        f.groups,
        vec![
            (Group::Unique, 1),
            (Group::Runeword, 1),
            (Group::Stackable, 1)
        ]
    );
    for m in PriceMode::ALL {
        assert_eq!(PriceMode::from_u8(m as u8), Some(m));
    }
    assert_eq!(PriceMode::from_u8(5), None);

    // "2 Ber or 1,000 gold": a price only in that currency, within the cap.
    let cap = |code: Option<&str>, max| PriceFilter {
        code: code.map(str::to_owned),
        min: 0,
        max,
    };
    let caps = |c: Vec<PriceFilter>| MarketQuery {
        caps: c,
        ..MarketQuery::default()
    };
    assert_eq!(
        found(&e, &caps(vec![cap(Some("r30"), 2), cap(None, 1000)])),
        [1, 2]
    );
    assert_eq!(
        found(&e, &caps(vec![cap(Some("r30"), 1)])),
        Vec::<i64>::new()
    );
    assert_eq!(
        found(&e, &caps(vec![cap(None, 200_000)])),
        [1, 6],
        "a bundle is never under a cap"
    );
    assert_eq!(
        found(&e, &caps(vec![cap(Some("r31"), 9)])),
        Vec::<i64>::new()
    );
    let f = facets(&e, &caps(vec![cap(None, 200_000)]));
    assert_eq!(f.price_modes, [2, 2, 0, 0, 2], "caps apply to the modes");
    assert_eq!(
        found(
            &e,
            &MarketQuery {
                price_mode: PriceMode::CurrenciesOnly,
                ..caps(vec![cap(None, 200_000)])
            }
        ),
        Vec::<i64>::new()
    );
    // Sorted by a currency.
    let by = |c: &str| MarketQuery {
        sort: MarketSort::Price,
        sort_currency: Some(c.into()),
        ..MarketQuery::default()
    };
    assert_eq!(found(&e, &by("")), [4, 1, 6, 5]);
    assert_eq!(found(&e, &by("r30")), [2]);
    // Five caps are too many.
    assert_eq!(
        e.search(
            &EconomyKey::new(MOD),
            None,
            &caps(vec![cap(None, 1); 5]),
            NOW
        )
        .unwrap_err()
        .refusal(),
        Some(Refusal::BadRequest)
    );
}

#[test]
fn group_first_order_and_pages() {
    let mut e = shelf_econ();
    let grid = MarketQuery {
        group_first: true,
        ..MarketQuery::default()
    };
    // Unique, set, runeword, rare, crafted, magic, other, stackable; by
    // date inside a group.
    assert_eq!(found(&e, &grid), [1, 2, 5, 3, 4, 6]);
    let newest = MarketQuery {
        desc: true,
        ..grid.clone()
    };
    assert_eq!(
        found(&e, &newest),
        [2, 1, 5, 3, 4, 6],
        "groups stay in order"
    );
    let names = MarketQuery {
        sort: MarketSort::Name,
        ..types(&["armo"])
    };
    assert_eq!(found(&e, &names), [3, 1, 4]);
    let level = MarketQuery {
        sort: MarketSort::Level,
        ..types(&["armo"])
    };
    assert_eq!(found(&e, &level), [1, 3, 4], "level 1, 3, 4");

    // 30 more magic rings: pages of 25, 50 and 100.
    for i in 0..30u32 {
        put_item(
            &mut e,
            2,
            u64::from(i) + 1,
            thing(100 + i, "Ring", &["ring"], 4),
        )
        .unwrap();
        e.post(
            &owner(2),
            &req(2, 100 + u64::from(i)),
            &Post {
                seller: "Other".into(),
                goods: Goods::Item(ItemId(100 + i)),
                mode: ListingMode::Price,
                price: Some(Price::gold(10)),
                days: 7,
            },
            20_000,
        )
        .unwrap();
    }
    let key = EconomyKey::new(MOD);
    let page = |size, page| {
        let q = MarketQuery {
            page_size: size,
            page,
            ..grid.clone()
        };
        e.search(&key, None, &q, NOW)
            .map(|p| (p.total, p.listings.iter().map(|l| l.id).collect::<Vec<_>>()))
    };
    let (total, first) = page(25, 0).unwrap();
    assert_eq!((total, first.len()), (36, 25));
    assert_eq!(first[..5], [1, 2, 5, 3, 4]);
    let (_, second) = page(25, 1).unwrap();
    assert_eq!(second.len(), 11);
    assert_eq!(second.last(), Some(&6), "the stack is last");
    assert_eq!(page(0, 0).unwrap().1.len(), 36);
    assert_eq!(page(50, 0).unwrap().1, page(100, 0).unwrap().1);
    assert_eq!(
        page(30, 0).unwrap_err().refusal(),
        Some(Refusal::BadRequest)
    );
    // By seller.
    let sellers = MarketQuery {
        sort: MarketSort::Seller,
        desc: true,
        page_size: 100,
        ..MarketQuery::default()
    };
    let p = e.search(&key, None, &sellers, NOW).unwrap();
    assert_eq!(p.listings[0].seller, "Seller");
    assert_eq!(p.listings[35].seller, "Other");
}

#[test]
fn hidden_not_searchable_not_buyable() {
    let mut e = shelf_econ();
    put_gold(&mut e, 2, 1, 1_000_000).unwrap();
    put_item(&mut e, 2, 2, item(20)).unwrap();
    let text = |t: &str| MarketQuery {
        filter: ItemFilter {
            text: Some(t.into()),
            ..ItemFilter::default()
        },
        ..MarketQuery::default()
    };
    assert_eq!(found(&e, &text("harlequin")), [1]);
    e.hide(&owner(1), &req(1, 40), 1, NOW).unwrap();
    e.hide(&owner(1), &req(1, 41), 3, NOW).unwrap();
    assert!(e.listing(1).unwrap().unwrap().hidden);
    assert_eq!(found(&e, &text("harlequin")), Vec::<i64>::new());
    assert_eq!(found(&e, &MarketQuery::default()), [2, 4, 5, 6]);
    assert_eq!(facets(&e, &MarketQuery::default()).ethereal, [0, 4]);
    let browse = e.browse(&EconomyKey::new(MOD), None, 0).unwrap();
    assert!(browse.listings.iter().all(|l| l.id != 1 && l.id != 3));
    // Others can't buy it or offer on it; the seller still sees it.
    let hash = Price::gold(1000).hash();
    assert_eq!(
        refused(e.buy(&owner(2), &req(2, 3), 1, &hash, NOW)),
        Refusal::ListingGone
    );
    let p = Price {
        gold: 10,
        stacks: vec![],
        items: vec![ItemId(20)],
    };
    assert_eq!(
        refused(e.offer(&owner(2), &req(2, 4), 3, "Buyer", &p, NOW)),
        Refusal::ListingGone
    );
    // Only the seller hides and shows.
    assert_eq!(
        refused(e.show(&owner(2), &req(2, 5), 1, NOW)),
        Refusal::NotYours
    );
    assert_eq!(
        refused(e.hide(&owner(1), &req(1, 42), 99, NOW)),
        Refusal::ListingGone
    );
    // Posted hidden: in the shop's Hidden tab, not in the search.
    put_item(&mut e, 1, 43, item(21)).unwrap();
    let l = e
        .post_with(
            &owner(1),
            &req(1, 44),
            &Post {
                seller: "Seller".into(),
                goods: Goods::Item(ItemId(21)),
                mode: ListingMode::Price,
                price: Some(Price::gold(7)),
                days: 7,
            },
            &PostOptions {
                hidden: true,
                note: "ask me".into(),
                fee_rev: None,
                facts: None,
            },
            NOW,
        )
        .unwrap()
        .id
        .unwrap();
    assert_eq!(found(&e, &MarketQuery::default()), [2, 4, 5, 6]);
    let shop = e
        .shop_page(&owner(1), ShopTab::Hidden, 0, ShopSort::Posted, false, NOW)
        .unwrap();
    assert_eq!(
        shop.rows.iter().map(|r| r.listing.id).collect::<Vec<_>>(),
        [1, 3, l]
    );
    assert_eq!(shop.rows[2].listing.note, "ask me");
    assert_eq!(shop.counts, [4, 3, 0, 0]);
    assert_eq!((shop.slots_used, shop.slots_max), (7, 100));
    // Shown again: found and bought.
    e.show(&owner(1), &req(1, 45), 1, NOW).unwrap();
    assert_eq!(found(&e, &text("harlequin")), [1]);
    e.buy(&owner(2), &req(2, 6), 1, &hash, NOW).unwrap();
    // Hidden listings keep their expiry: they expire like the others.
    assert_eq!(e.expire(7 * DAY + 20_000).unwrap(), 6);
}

#[test]
fn reprice_changes_hash() {
    let mut e = shelf_econ();
    put_gold(&mut e, 2, 1, 1_000_000).unwrap();
    let seen = e.listing(1).unwrap().unwrap().price_hash();
    assert_eq!(seen, Price::gold(1000).hash());
    let new = Price::gold(1200);
    e.reprice(
        &owner(1),
        &req(1, 40),
        1,
        ListingMode::Price,
        Some(&new),
        NOW,
    )
    .unwrap();
    let l = e.listing(1).unwrap().unwrap();
    assert_eq!(l.price, Some(new.clone()));
    assert_ne!(l.price_hash(), seen);
    assert_eq!(
        refused(e.buy(&owner(2), &req(2, 2), 1, &seen, NOW)),
        Refusal::PriceChanged
    );
    // The index follows: the new gold, the new caps.
    let gold = |min, max| MarketQuery {
        price: Some(PriceFilter {
            code: None,
            min,
            max,
        }),
        ..MarketQuery::default()
    };
    assert_eq!(found(&e, &gold(1100, 1300)), [1]);
    assert_eq!(found(&e, &gold(900, 1099)), Vec::<i64>::new());
    // Offers only, then a price in Ber; bad prices are refused.
    e.reprice(
        &owner(1),
        &req(1, 41),
        1,
        ListingMode::OffersOnly,
        None,
        NOW,
    )
    .unwrap();
    let only = MarketQuery {
        price_mode: PriceMode::TakesOffers,
        ..MarketQuery::default()
    };
    assert_eq!(found(&e, &only), [1, 3, 4]);
    assert_eq!(
        refused(e.reprice(&owner(1), &req(1, 42), 1, ListingMode::Price, None, NOW)),
        Refusal::BadRequest
    );
    assert_eq!(
        refused(e.reprice(
            &owner(2),
            &req(2, 3),
            1,
            ListingMode::Price,
            Some(&new),
            NOW
        )),
        Refusal::NotYours
    );
    let bers = market::price(0, vec![ber(3)]);
    e.reprice(
        &owner(1),
        &req(1, 43),
        1,
        ListingMode::Price,
        Some(&bers),
        NOW,
    )
    .unwrap();
    assert_eq!(
        found(
            &e,
            &MarketQuery {
                price_mode: PriceMode::CurrenciesOnly,
                ..MarketQuery::default()
            }
        ),
        [1, 2]
    );
    // Hidden, then repriced: still out of the search until shown.
    e.hide(&owner(1), &req(1, 44), 1, NOW).unwrap();
    e.reprice(
        &owner(1),
        &req(1, 45),
        1,
        ListingMode::Price,
        Some(&new),
        NOW,
    )
    .unwrap();
    assert_eq!(found(&e, &gold(1100, 1300)), Vec::<i64>::new());
    e.show(&owner(1), &req(1, 46), 1, NOW).unwrap();
    assert_eq!(found(&e, &gold(1100, 1300)), [1]);
    e.buy(&owner(2), &req(2, 4), 1, &new.hash(), NOW).unwrap();
}

/// The plan's cost check: searches and facet counts over 100,000
/// listings, searches under 50 ms and facets under 150 ms in a release
/// build on the CI server. Debug builds index fewer listings and only
/// check the results.
#[test]
fn search_and_facets_100k() {
    let n: u32 = if cfg!(debug_assertions) {
        20_000
    } else {
        100_000
    };
    let mut e = Economy::in_memory(MOD, shelf_config()).unwrap();
    let split = EconomyKey::new(MOD).split();
    let trees: [&[&str]; 8] = [
        &["ring", "misc"],
        &["amul", "misc"],
        &["helm", "armo"],
        &["circ", "helm", "armo"],
        &["tors", "armo"],
        &["swor", "mele", "weap"],
        &["axe", "mele", "weap"],
        &["bow", "miss", "weap"],
    ];
    let words = [
        "Havoc", "Loop", "Bramble", "Crest", "Shako", "Storm", "Eagle", "Wraith", "Grim", "Dread",
        "Doom", "Spirit", "Raven", "Frost", "Blood", "Coil", "Viper", "Horn", "Gale", "Rune",
    ];
    let build = std::time::Instant::now();
    e.sweep(0, |a| {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = |m: u64| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x % m
        };
        for i in 1..=n {
            let r = |v: u64| v as usize;
            let tree = trees[r(next(8))];
            let name = format!("{} {} {}", words[r(next(20))], words[r(next(20))], tree[0]);
            let mut it = thing(i, &name, tree, next(8) as u8 + 1);
            it.facts.level_req = next(90) as u8;
            it.facts.sockets = next(7) as u8;
            it.facts.ethereal = next(10) == 0;
            it.facts.identified = next(20) != 0;
            it.facts.runeword = it.facts.sockets > 0 && next(10) == 0;
            it.facts.stats = (0..5)
                .map(|k| (k * 7 + next(7) as u16, next(100) as i32))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .collect();
            it.record = vec![0; 32];
            a.insert_item(&it, 1, &split, Place::Stash, None)?;
            let (mode, price) = match next(8) {
                0 => (1, None),
                1 => (0, Some(market::price(0, vec![ber(next(5) as u32 + 1)]))),
                2 => (
                    2,
                    Some(market::price(next(10_000) + 1, vec![("r31".into(), 1)])),
                ),
                _ => (0, Some(Price::gold(next(1_000_000) + 1))),
            };
            a.exec(
                "INSERT INTO listings(account, economy, seller, item_id, mode, price, price_gold, \
                 state, posted, expires) \
                 VALUES (1, ?1, 'Seller', ?2, ?3, ?4, ?5, 'active', ?6, ?7)",
                rusqlite::params![
                    split,
                    i,
                    mode,
                    price.as_ref().map(Price::encode),
                    price.as_ref().map(|p| p.gold as i64),
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
    let cap = |code: Option<&str>, max| PriceFilter {
        code: code.map(str::to_owned),
        min: 0,
        max,
    };
    let queries: Vec<(&str, MarketQuery)> = vec![
        ("all", MarketQuery::default()),
        (
            "grid, newest",
            MarketQuery {
                group_first: true,
                desc: true,
                ..MarketQuery::default()
            },
        ),
        (
            "uniques",
            MarketQuery {
                groups: vec![Group::Unique],
                ..MarketQuery::default()
            },
        ),
        ("armor tree", types(&["armo"])),
        (
            "weapons, socketed, by name",
            MarketQuery {
                socketed: Some(true),
                sort: MarketSort::Name,
                ..types(&["weap"])
            },
        ),
        (
            "unidentified rares and uniques",
            MarketQuery {
                identified: Some(false),
                groups: vec![Group::Rare, Group::Unique],
                ..MarketQuery::default()
            },
        ),
        (
            "runes only",
            MarketQuery {
                price_mode: PriceMode::CurrenciesOnly,
                ..MarketQuery::default()
            },
        ),
        (
            "2 Ber or 50,000 gold, by gold",
            MarketQuery {
                caps: vec![cap(Some("r30"), 2), cap(None, 50_000)],
                sort: MarketSort::Price,
                sort_currency: Some(String::new()),
                ..MarketQuery::default()
            },
        ),
        (
            "name, ethereal, offers",
            MarketQuery {
                filter: ItemFilter {
                    text: Some("storm".into()),
                    ethereal: Some(true),
                    ..ItemFilter::default()
                },
                price_mode: PriceMode::TakesOffers,
                ..MarketQuery::default()
            },
        ),
        (
            "everything set",
            MarketQuery {
                filter: ItemFilter {
                    level: Some((20, 70)),
                    ethereal: Some(false),
                    ..ItemFilter::default()
                },
                groups: vec![Group::Rare, Group::Magic, Group::Unique],
                socketed: Some(true),
                identified: Some(true),
                price_mode: PriceMode::GoldOnly,
                ..types(&["armo", "mele"])
            },
        ),
    ];
    let mut report = Vec::new();
    let (mut slowest_search, mut slowest_facets) =
        (std::time::Duration::ZERO, std::time::Duration::ZERO);
    for (round, (what, q)) in queries.iter().enumerate() {
        let (mut search, mut facet) = (std::time::Duration::MAX, std::time::Duration::MAX);
        let mut total = 0;
        let mut counted = Facets::default();
        // The best of three; each facet count at a new time and after a
        // commit, so none is a kept answer.
        for k in 0..3 {
            let t = std::time::Instant::now();
            let p = e.search(&key, None, q, 1000).unwrap();
            search = search.min(t.elapsed());
            total = p.total;
            e.sweep(0, |_| Ok(())).unwrap();
            let t = std::time::Instant::now();
            counted = e
                .facets(&key, q, 1000 + (round * 3 + k) as i64 * 20_000)
                .unwrap();
            facet = facet.min(t.elapsed());
        }
        assert!(total > 0, "{what}: nothing found");
        // A facet without its own filter set counts what the search finds
        // (a price or stat sort also leaves out listings without that
        // currency or stat; the facets don't sort).
        if q.groups.is_empty() && !matches!(q.sort, MarketSort::Price | MarketSort::Stat(_)) {
            let by_group: u32 = counted.groups.iter().map(|(_, c)| c).sum();
            assert_eq!(by_group, total, "{what}");
        }
        if *what == "all" {
            assert_eq!((total, counted.price_modes[0]), (n, n));
        }
        slowest_search = slowest_search.max(search);
        slowest_facets = slowest_facets.max(facet);
        report.push(format!(
            "{what}: {total} results, search {search:?}, facets {facet:?}"
        ));
    }
    println!(
        "{n} listings indexed in {build:?}; slowest search {slowest_search:?}, slowest facets \
         {slowest_facets:?}\n  {}",
        report.join("\n  ")
    );
    if !cfg!(debug_assertions) {
        assert!(
            slowest_search < std::time::Duration::from_millis(50),
            "slowest search {slowest_search:?}: {report:?}"
        );
        assert!(
            slowest_facets < std::time::Duration::from_millis(150),
            "slowest facets {slowest_facets:?}: {report:?}"
        );
    }
}

#[test]
fn posted_facts_replace_the_stored_ones() {
    // The stash stored what the client said; the main server reads the
    // record on post and its facts are the ones the market indexes.
    let mut e = shelf_econ();
    let mut it = item(30);
    it.facts.runeword = false;
    it.facts.types = vec![];
    put_item(&mut e, 1, 50, it.clone()).unwrap();
    assert_eq!(
        e.stash_record(&owner(1), ItemId(30)).unwrap(),
        Some(it.record.clone())
    );
    assert_eq!(e.stash_record(&owner(2), ItemId(30)).unwrap(), None);
    let read = ItemFacts {
        code: "7wc".into(),
        item_type: "pole".into(),
        quality: 2,
        name: "Insight".into(),
        name_parts: vec!["Insight".into(), "Thresher".into()],
        level_req: 53,
        ilvl: 85,
        sockets: 4,
        ethereal: true,
        class: None,
        stats: vec![(7, 35)],
        runeword: true,
        identified: false,
        types: vec!["pole".into(), "weap".into()],
    };
    let l = e
        .post_with(
            &owner(1),
            &req(1, 51),
            &Post {
                seller: "Seller".into(),
                goods: Goods::Item(ItemId(30)),
                mode: ListingMode::Price,
                price: Some(Price::gold(9)),
                days: 7,
            },
            &PostOptions {
                facts: Some(read),
                ..PostOptions::default()
            },
            NOW,
        )
        .unwrap()
        .id
        .unwrap();
    let text = |t: &str| MarketQuery {
        filter: ItemFilter {
            text: Some(t.into()),
            ..ItemFilter::default()
        },
        ..MarketQuery::default()
    };
    assert_eq!(found(&e, &text("insight")), [l]);
    assert_eq!(found(&e, &text("havoc")), Vec::<i64>::new());
    assert_eq!(found(&e, &types(&["pole"])), [l]);
    let unid = MarketQuery {
        identified: Some(false),
        ..types(&["weap"])
    };
    assert_eq!(found(&e, &unid), [l]);
    let rw = MarketQuery {
        groups: vec![Group::Runeword],
        ..types(&["weap"])
    };
    assert!(found(&e, &rw).contains(&l));
    // Posted: no longer in the stash.
    assert_eq!(e.stash_record(&owner(1), ItemId(30)).unwrap(), None);
}
