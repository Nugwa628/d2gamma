//! Realm item IDs through the inactive item record (`inactive_items.rs`): an
//! item that leaves with its room and comes back keeps its ID, and its seeds
//! are those it gets without realm IDs.

use super::objects::{create, drop_fx, CHEST};
use super::*;
use crate::items::bitstream::read;
use crate::rng::Seed;
use crate::wiring::economy::{BlockIds, IdBlock, ItemIdSource, ItemIds};
use crate::world::objects::Operate;

/// Chest drops until one is an extended item (one with an ID field).
fn extended_item(fx: &mut Fx) -> UnitId {
    for x in (20..60).step_by(4) {
        let o = create(fx, CHEST, x);
        let a = fx.a;
        let p = fx.spawn(UnitType::Player, 0, a, 10, 10);
        let op = Operate {
            object: o,
            operator: Some(p),
            class: CHEST as u16,
            operate_fn: 4,
        };
        let Some(item) = fx
            .sim
            .objects(&mut fx.game, |_, _, w| {
                crate::world::objects::ChestWorld::chest_drop(w, &op, 0)
            })
            .flatten()
        else {
            continue;
        };
        let s = &mut fx.sim.sys;
        let rec = View::of(&mut s.units, &mut s.stats, &s.data, &mut s.hooks)
            .item_record_of(item)
            .expect("a record");
        let t = s.hooks.object_drops.as_ref().unwrap().tables.clone();
        let e = read::read_save_entry(&rec.bytes, &t.items).expect("reads");
        if !(e.item.item.compact || e.item.item.alt) {
            return item;
        }
    }
    panic!("no extended item from the chests");
}

/// Stores `item` as an inactive record and makes it again; the new unit.
fn round_trip(fx: &mut Fx, item: UnitId) -> UnitId {
    let s = &mut fx.sim.sys;
    let rec = View::of(&mut s.units, &mut s.stats, &s.data, &mut s.hooks)
        .item_record_of(item)
        .expect("a record");
    let room = fx.a;
    let before = fx.game.lists.units_of_type(UnitType::Item);
    let s = &mut fx.sim.sys;
    View::of(&mut s.units, &mut s.stats, &s.data, &mut s.hooks).restore_item_record(
        &mut fx.game,
        room,
        &rec,
        0,
    );
    let after = fx.game.lists.units_of_type(UnitType::Item);
    let new: Vec<UnitId> = after.into_iter().filter(|u| !before.contains(u)).collect();
    assert_eq!(new.len(), 1);
    new[0]
}

#[test]
fn an_item_keeps_its_realm_id_through_an_inactive_room() {
    let mut fx = drop_fx();
    let ids = BlockIds::with_block(IdBlock {
        first: 70_000,
        count: 4096,
    })
    .unwrap();
    *fx.sim.sys.hooks.items.ids_mut() = ItemIds::new(ItemIdSource::Blocks(ids));
    let item = extended_item(&mut fx);
    let id = fx.sim.sys.hooks.items.save_id(item, 0, true);
    assert_eq!(id, 70_000);
    let back = round_trip(&mut fx, item);
    assert_eq!(fx.sim.sys.hooks.items.ids().id_of(back), Some(id));
    // The seeds of a record whose field is 0, as in a game without IDs.
    let r = fx.sim.sys.units.get(back).unwrap();
    assert_eq!((r.init_seed, r.seed), (0, Seed::init_low(0)));
}

#[test]
fn without_realm_ids_the_record_carries_no_id() {
    let mut fx = drop_fx();
    let item = extended_item(&mut fx);
    let back = round_trip(&mut fx, item);
    assert_eq!(fx.sim.sys.hooks.items.ids().iter().count(), 0);
    let r = fx.sim.sys.units.get(back).unwrap();
    assert_eq!((r.init_seed, r.seed), (0, Seed::init_low(0)));
}
