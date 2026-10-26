//! A mod game's quest drops on the host's quest world ([`HostQuests`]): a
//! treasure class dropped at a unit ([`QuestWorld::drop_treasure_class`])
//! and a unique item given to a player ([`QuestWorld::give_unique`]).

use std::sync::Arc;

use super::*;
use crate::items::moves::Spot;
use crate::items::tables::UniqueRec;
use crate::items::{q, ItemTables};
use crate::wiring::economy::{
    DeathDrops, Economy, EconomyError, EconomyQuests, GameFields, HostQuests, ItemStore,
    QuestInventory,
};
use crate::wiring::interaction::tests::Rest;
use crate::wiring::inventory::tests::item_tables;
use crate::world::quests::QuestWorld;

/// An inventory model that takes every item.
struct Takes;

impl<H> QuestInventory<H> for Takes {
    fn place(&mut self, _: &mut Economy<'_, H>, _: UnitId, _: UnitId) -> bool {
        true
    }
    fn drop_at(&mut self, _: &mut Economy<'_, H>, _: UnitId, _: Spot) {}
    fn inventory_pass(&mut self, _: &mut Economy<'_, H>, _: UnitId) {}
    fn fault(&mut self, error: EconomyError) {
        panic!("{error:?}");
    }
    fn items_of(&self, _: UnitId) -> Vec<UnitId> {
        Vec::new()
    }
    fn update_guids(&self, _: UnitId) -> Vec<u32> {
        Vec::new()
    }
    fn cursor_of(&self, _: UnitId) -> Option<UnitId> {
        None
    }
    fn weapon_in_use(&self, _: &Economy<'_, H>, _: UnitId) -> Option<UnitId> {
        None
    }
    fn delete(&mut self, _: &mut Economy<'_, H>, _: UnitId, _: UnitId) {}
}

type Host<'e, 'a> = HostQuests<'e, 'a, TestPending, Rest>;

/// Runs `f` on [`HostQuests`] over the fixture's economy with item tables
/// `tables` (the game's item store lent for the call and put back).
fn host<T>(
    fx: &mut Fx,
    tables: &ItemTables,
    inv: Option<&mut Takes>,
    f: impl FnOnce(&mut Host<'_, '_>) -> T,
) -> T {
    let mut rest = Rest::new();
    let s = &mut fx.sim.sys;
    let mut fields = GameFields::new(s.hooks.game_seed, true);
    let mut items = std::mem::replace(&mut s.hooks.items, ItemStore::new());
    let out = {
        let mut econ = Economy {
            game: &mut fx.game,
            units: &mut s.units,
            stats: &mut s.stats,
            data: &s.data,
            hooks: &mut s.hooks,
            fields: &mut fields,
            tables,
            items: &mut items,
        };
        let mut w = HostQuests::new(EconomyQuests::new(&mut econ, &mut rest));
        w.inventory = inv.map(|i| i as &mut dyn QuestInventory<ActionHooks<TestPending>>);
        f(&mut w)
    };
    s.hooks.items = items;
    out
}

/// The game's drop state: treasure class 1 is one pick of gold.
fn with_drops(fx: &mut Fx) {
    fx.sim.hooks().object_drops = Some(Box::new(DeathDrops::new(
        Arc::new(super::death::drop_tables()),
        GameFields::new(Seed::init(), false),
    )));
}

/// Treasure class 1 at the player: one gold pile in its room, recorded in
/// the drop state.
#[test]
fn a_treasure_class_drops_at_the_unit() {
    let mut fx = Fx::new();
    with_drops(&mut fx);
    let a = fx.a;
    let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
    let t = item_tables();
    let made = host(&mut fx, &t, None, |w| {
        w.drop_treasure_class(p, Some(p), 1, 5, 0)
    });
    assert_eq!(made.len(), 1);
    assert_eq!(fx.game.lists.unit(made[0]).and_then(|e| e.room()), Some(a));
    let d = fx.sim.hooks().object_drops.as_ref().unwrap();
    assert!(d.errors.is_empty() && d.failures.is_empty());
    assert_eq!(d.placed.len(), 1);
    fx.assert_clean();
}

/// Without the game's drop state nothing drops.
#[test]
fn no_drop_state_drops_nothing() {
    let mut fx = Fx::new();
    let a = fx.a;
    let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
    let t = item_tables();
    let made = host(&mut fx, &t, None, |w| {
        w.drop_treasure_class(p, Some(p), 1, 5, 0)
    });
    assert!(made.is_empty());
    assert!(fx.sim.hooks().items.is_empty());
}

/// Unique row 1 (a cap) is made and placed; an unknown row gives nothing.
#[test]
fn a_unique_is_given() {
    let mut fx = Fx::new();
    let a = fx.a;
    let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
    let mut t = item_tables();
    let row = |lvl| UniqueRec {
        code: *b"cap ",
        enabled: true,
        rarity: 1,
        lvl,
        ..UniqueRec::default()
    };
    t.uniques = vec![row(1), row(3)];
    let mut inv = Takes;
    let item = host(&mut fx, &t, Some(&mut inv), |w| {
        w.give_unique(p, 1, 0, true)
    })
    .expect("given");
    let it = fx.sim.hooks().items.get(item).expect("in the game's store");
    assert_eq!((it.quality, it.file_index), (q::UNIQUE, 1));
    // The item level is raised to the row's.
    assert!(it.ilvl >= 3);
    let mut inv = Takes;
    let none = host(&mut fx, &t, Some(&mut inv), |w| {
        w.give_unique(p, 9, 0, true)
    });
    assert_eq!(none, None);
    // Without an inventory model nothing is made.
    let none = host(&mut fx, &t, None, |w| w.give_unique(p, 0, 0, true));
    assert_eq!(none, None);
    assert_eq!(fx.sim.hooks().items.len(), 1);
}
