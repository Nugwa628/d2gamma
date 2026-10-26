//! Realm item IDs through the item paths that make a unit from a save
//! record (`economy::item_ids`): a load keeps the record's ID, a copy takes
//! over its source's ID (and each filler its child's), a new item holds none,
//! and with the default source nothing is stored and the seeds are the same.

use super::*;
use crate::items::bitstream::{self, read, BitWriter, Isc};
use crate::items::stat;
use crate::wiring::economy::{BlockIds, IdBlock, ItemIdSource, ItemIds};

fn isc() -> Vec<Isc> {
    let mut t = vec![Isc::default(); N_STATS];
    t[usize::from(stat::NUMSOCKETS)] = Isc {
        valshift: 0,
        save_bits: 4,
        save_add: 0,
        save_param_bits: 0,
    };
    t
}

fn world(realm: bool) -> World {
    let mut w = World::new();
    w.tables.isc = isc();
    if realm {
        let ids = BlockIds::with_block(IdBlock {
            first: 9000,
            count: 100,
        })
        .unwrap();
        *w.items.ids_mut() = ItemIds::new(ItemIdSource::Blocks(ids));
    }
    w
}

/// The save entry of `u` with its ID field set to `field`.
fn entry_with(w: &mut World, u: UnitId, field: u32) -> read::ReadEntry {
    let bytes = w.desk(|d| {
        let mut v = d.save_view(u).expect("view");
        v.unit28 = field;
        let mut bw = BitWriter::new(bitstream::SAVE_BUFFER);
        bitstream::write_save_into(&mut bw, &v, &d.econ.tables.isc);
        bw.finish().expect("fits")
    });
    read::read_save_entry(&bytes, &w.tables).expect("reads back")
}

/// A socketed sword holding one key, both with realm IDs.
fn sword_with_child(w: &mut World) -> (UnitId, UnitId) {
    super::load::allow_sockets(&mut w.tables, SWORD, 2);
    w.tables.items[SWORD].hasinv = 1;
    let s = w.cursor_item(SWORD);
    let su = w.unit(s).unwrap();
    w.items.get_mut(su).unwrap().flags |= 0x800;
    w.set_stat(su, stat::NUMSOCKETS, 2);
    w.state.add_inventory(su, UnitKind::Item, s);
    let k = w.ground_item(KEY, 22, 21);
    let ku = w.unit(k).unwrap();
    w.desk(|d| {
        let mut inv = d.state.inventories.remove(&su).unwrap();
        inv.link(d, ku, None);
        d.state.inventories.insert(su, inv);
    });
    (su, ku)
}

#[test]
fn a_loaded_item_keeps_its_saved_id() {
    let mut w = world(true);
    let k = w.cursor_item(CAP);
    let u = w.unit(k).unwrap();
    let e = entry_with(&mut w, u, 4242);
    let made = w.desk(|d| d.make_entry(&e)).expect("made");
    assert_eq!(w.items.ids().id_of(made), Some(4242));
    // The field is still the loaded item's init seed, as without IDs.
    assert_eq!(w.units.get(made).unwrap().init_seed, 4242);
    assert_eq!(
        w.items.ids().blocks().unwrap().drawn(),
        0,
        "a load draws none"
    );
}

#[test]
fn a_copy_takes_over_the_ids_of_its_source_and_children() {
    let mut w = world(true);
    let (su, ku) = sword_with_child(&mut w);
    let sid = w.items.save_id(su, 0, true);
    let kid = w.items.save_id(ku, 0, true);
    assert_eq!((sid, kid), (9000, 9001));
    let init = w.units.get(su).unwrap().init_seed;
    let c = w.desk(|d| d.copy_of(su, true)).expect("copied");
    let kids = w
        .state
        .inventories
        .get(&c)
        .expect("fillers")
        .items()
        .to_vec();
    assert_eq!(kids.len(), 1);
    let ids = w.items.ids();
    assert_eq!((ids.id_of(c), ids.id_of(kids[0])), (Some(sid), Some(kid)));
    assert_eq!((ids.id_of(su), ids.id_of(ku)), (None, None), "never shared");
    drop(ids);
    // The copy's seeds are the source's, as without IDs.
    assert_eq!(w.units.get(c).unwrap().init_seed, init);
}

#[test]
fn a_copy_of_an_item_without_an_id_draws_at_its_first_save() {
    let mut w = world(true);
    let k = w.cursor_item(CAP);
    let u = w.unit(k).unwrap();
    let c = w.desk(|d| d.copy_of(u, true)).expect("copied");
    assert_eq!(w.items.ids().id_of(c), None, "not the source's init seed");
    assert_eq!(w.items.save_id(c, 1, true), 9000);
}

#[test]
fn a_new_item_in_a_reused_slot_holds_no_old_id() {
    let mut w = world(true);
    let k = w.cursor_item(CAP);
    let u = w.unit(k).unwrap();
    assert_eq!(w.items.save_id(u, 0, true), 9000);
    w.econ().free_item(u).unwrap();
    let k2 = w.cursor_item(CAP);
    let u2 = w.unit(k2).unwrap();
    assert_eq!(u2, u, "the freed slot is reused");
    assert_eq!(w.items.ids().id_of(u2), None);
    assert_eq!(w.items.save_id(u2, 0, true), 9001);
}

#[test]
fn the_default_source_stores_nothing_on_any_path() {
    let mut w = world(false);
    let (su, _) = sword_with_child(&mut w);
    let init = w.units.get(su).unwrap().init_seed;
    assert_eq!(w.items.save_id(su, init, true), init);
    let c = w.desk(|d| d.copy_of(su, true)).expect("copied");
    let e = entry_with(&mut w, c, 4242);
    let made = w.desk(|d| d.make_entry(&e)).expect("made");
    assert_eq!(w.units.get(made).unwrap().init_seed, 4242);
    assert_eq!(w.items.ids().iter().count(), 0);
    assert_eq!(*w.items.ids().source(), ItemIdSource::InitSeed);
}
