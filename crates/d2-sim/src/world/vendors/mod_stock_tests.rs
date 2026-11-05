//! A mod game's store stock ([`StoreProvider::stock`]), with a synthetic
//! provider: Charsi sells a magic cap at the player's level + 2 and, from
//! player level 5 on (a tier requirement it reads through the host), a
//! rare skull cap; it draws once from the NPC-control seed. A provider
//! without a stock builder leaves D2RUST's store as it is.

use std::sync::Arc;

use super::stock::{StockHost, StockItem, StockRequest};
use super::store::{generate, StoreCtx};
use super::tests::{index, p, record, tables, Fake};
use super::*;
use crate::items::q;
use crate::rng::Seed;
use crate::rules::{ModRules, Ruleset, StoreProvider};

#[derive(Debug)]
struct Tiers;

impl StoreProvider for Tiers {
    fn stock(
        &self,
        t: &VendorTables,
        host: &dyn StockHost,
        req: &StockRequest,
        seed: &mut Seed,
    ) -> Option<Vec<StockItem>> {
        if req.class != class::CHARSI {
            return None;
        }
        seed.step();
        let mut items = vec![StockItem {
            record: index(t, "cap"),
            quality: q::MAGIC,
            ilvl: req.player_level + 2,
        }];
        if host.stat(req.player, stat::LEVEL, 0) >= 5 {
            items.push(StockItem {
                record: index(t, "skp"),
                quality: q::RARE,
                ilvl: 9,
            });
        }
        // A record past the tables is skipped.
        items.push(StockItem {
            record: t.items.len(),
            quality: q::NORMAL,
            ilvl: 1,
        });
        Some(items)
    }
}

#[derive(Debug)]
struct Bare;

impl StoreProvider for Bare {}

#[derive(Debug)]
struct Game<P>(P);

impl<P: StoreProvider + 'static> ModRules for Game<P> {
    fn name(&self) -> &str {
        "store stock test mod"
    }
    fn stores(&self) -> Option<&dyn StoreProvider> {
        Some(&self.0)
    }
}

/// Charsi's store made for a player of `level` under `rules`: the
/// requests (record, quality, item level), the store size and the seed.
fn charsi(rules: Ruleset, level: i32) -> (Vec<(usize, u8, i32)>, usize, Seed) {
    let t = tables();
    let mut rec = record(&t, class::CHARSI, 0);
    let mut w = Fake::new(class::CHARSI);
    w.rules = rules;
    w.set(p().0, stat::LEVEL, level);
    let mut seed = Seed::new(1, 666);
    let mut c = StoreCtx {
        tables: &t,
        seed: &mut seed,
    };
    generate(&mut c, &mut rec, &mut w, p(), 77).unwrap();
    assert_eq!(rec.store_time, 77);
    (w.created.clone(), rec.store.len(), seed)
}

#[test]
fn a_mod_builds_the_store_stock() {
    let t = tables();
    let (cap, skp) = (index(&t, "cap"), index(&t, "skp"));
    let rules = || Ruleset::Mod(Arc::new(Game(Tiers)));
    let mut one = Seed::new(1, 666);
    one.step();
    let (made, n, seed) = charsi(rules(), 3);
    assert_eq!(made, [(cap, q::MAGIC, 5)]);
    assert_eq!((n, seed), (1, one));
    // The rare tier's requirement is met from level 5.
    let (made, n, _) = charsi(rules(), 6);
    assert_eq!(made, [(cap, q::MAGIC, 8), (skp, q::RARE, 9)]);
    assert_eq!(n, 2);
}

#[test]
fn the_engines_store_stock_is_unchanged() {
    for level in [1, 6] {
        let own = charsi(Ruleset::Original, level);
        assert!(own.1 > 2, "D2RUST's lists");
        assert_eq!(charsi(Ruleset::Mod(Arc::new(Game(Bare))), level), own);
    }
}
