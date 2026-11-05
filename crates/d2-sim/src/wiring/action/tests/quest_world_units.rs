//! The units the quests ask the host's quest world ([`HostQuests`]) about:
//! a unit the rest has not synced yet (a boss inside its creation's
//! quest hook, the Golden Bird's boss choice) is answered from its room.

use super::*;
use crate::wiring::economy::{Economy, EconomyQuests, GameFields, HostQuests, ItemStore};
use crate::wiring::interaction::tests::Rest;
use crate::wiring::inventory::tests::item_tables;
use crate::world::quests::QuestWorld;

/// The Spider Forest (level 76), in Act III (act 2).
const SPIDER_FOREST: u32 = 76;

/// Runs `f` on [`HostQuests`] over the fixture's economy and `rest`.
fn host<T>(
    fx: &mut Fx,
    rest: &mut Rest,
    f: impl FnOnce(&mut HostQuests<'_, '_, TestPending, Rest>) -> T,
) -> T {
    let tables = item_tables();
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
            tables: &tables,
            items: &mut items,
        };
        f(&mut HostQuests::new(EconomyQuests::new(&mut econ, rest)))
    };
    s.hooks.items = items;
    out
}

#[test]
fn a_unit_created_since_the_sync_is_in_its_rooms_act() {
    let mut fx = Fx::with_rooms(&[(SPIDER_FOREST, TileRect::new(0, 0, 8, 8))]);
    let a = fx.a;
    let synced = fx.spawn(UnitType::Monster, 0, a, 10, 10);
    let boss = fx.spawn(UnitType::Monster, 0, a, 12, 10);
    let mut rest = Rest::new();
    rest.unsynced.insert(boss);
    let (act, level, known) = host(&mut fx, &mut rest, |w| {
        (w.unit_act(boss), w.unit_level(boss), w.unit_act(synced))
    });
    assert_eq!(level, Some(SPIDER_FOREST), "its room's level");
    assert_eq!(act, Some(crate::drlg::act_of_level(SPIDER_FOREST)));
    assert_eq!(act, Some(2), "Act III");
    // A unit the rest knows keeps the rest's act.
    assert_eq!(known, Some(0));
}
