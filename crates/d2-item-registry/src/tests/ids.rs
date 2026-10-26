//! Item IDs read from save bytes made by the game's own save writer.

use d2_formats::d2s::{status, Body, Corpse, D2s, ItemEntry};
use d2_sim::bench_fixtures::{item_rec, item_tables, push_item};
use d2_sim::items::bitstream::{write_save, Isc, StreamItem};
use d2_sim::items::tables::ItemTables;
use d2_sim::items::{flag, q};

use crate::save_ids;

/// The fixture's ring item type.
const RING: u16 = 10;

fn tables() -> ItemTables {
    let mut t = item_tables();
    let mut isc = vec![Isc::default(); 360];
    // The socket count's width.
    isc[194].save_bits = 4;
    t.isc = isc;
    push_item(&mut t, item_rec(RING, b"rin "));
    push_item(&mut t, item_rec(RING, b"r01 "));
    t
}

/// A normal ring with ID `id`.
fn ring(id: u32) -> StreamItem {
    StreamItem {
        flags: flag::IDENTIFIED,
        version: 101,
        code: *b"rin ",
        unit28: id,
        ilvl: 5,
        quality: q::NORMAL,
        file_index: -1,
        runeword: 0xFFFF,
        main: Some(Vec::new()),
        ..StreamItem::default()
    }
}

/// A compact record (no ID field), as runes and gems are.
fn compact() -> StreamItem {
    StreamItem {
        flags: flag::IDENTIFIED | d2_sim::items::bitstream::hflag::COMPACT,
        compact: true,
        version: 101,
        code: *b"r01 ",
        ..StreamItem::default()
    }
}

fn entry(t: &ItemTables, it: &StreamItem) -> ItemEntry {
    ItemEntry {
        bytes: write_save(it, &t.isc).expect("item fits").0,
    }
}

#[test]
fn ids_are_the_full_records_seed_field() {
    let t = tables();
    let mut s = D2s::new_stub(b"Tester", 0, status::EXPANSION, 0).expect("name fits");
    s.body = Some(Body {
        items: vec![
            entry(&t, &ring(0x1111)),
            entry(&t, &compact()),
            entry(&t, &ring(0)),
        ],
        corpses: vec![Corpse {
            items: vec![entry(&t, &ring(0x2222))],
            ..Corpse::default()
        }],
        hireling_items: Some(Some(vec![entry(&t, &ring(0x3333))])),
        ..Body::default()
    });
    let found = save_ids(&s, &t);
    assert!(found.errors.is_empty(), "{:?}", found.errors);
    assert_eq!(found.ids, vec![0x1111, 0x2222, 0x3333]);
    assert_eq!(found.without_id, 1);
}

#[test]
fn an_entry_that_does_not_decode_is_reported() {
    let t = tables();
    let mut s = D2s::new_stub(b"Tester", 0, status::EXPANSION, 0).expect("name fits");
    s.body = Some(Body {
        items: vec![ItemEntry {
            bytes: vec![0x4A, 0x4D, 0xFF],
        }],
        ..Body::default()
    });
    let found = save_ids(&s, &t);
    assert!(found.ids.is_empty());
    assert_eq!(found.errors.len(), 1);
}
