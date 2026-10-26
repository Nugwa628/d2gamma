#![no_main]
//! The item registry's save ID reader (d2-item-registry `save_ids`) over
//! arbitrary item bytes, with the synthetic item tables of the unit tests
//! (no game files). The input is cut into item records at each `JM`; the
//! records go to the player, a corpse and the hireling, and the whole
//! input is also read as a save. Every record either gives its IDs or is
//! one reported error.
use std::sync::OnceLock;

use d2_formats::d2s::{self, status, Body, Corpse, D2s, ItemEntry, SaveTables, StatSave};
use d2_item_registry::save_ids;
use d2_sim::bench_fixtures::{item_rec, item_tables, push_item};
use d2_sim::items::bitstream::read::read_save_entry;
use d2_sim::items::bitstream::Isc;
use d2_sim::items::tables::ItemTables;
use libfuzzer_sys::fuzz_target;

/// The fixture's ring item type.
const RING: u16 = 10;

fn tables() -> &'static ItemTables {
    static T: OnceLock<ItemTables> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = item_tables();
        let mut isc = vec![Isc::default(); 360];
        // The socket count's width.
        isc[194].save_bits = 4;
        t.isc = isc;
        push_item(&mut t, item_rec(RING, b"rin "));
        push_item(&mut t, item_rec(RING, b"r01 "));
        t
    })
}

/// Stats of 16 bits; item records read with the fixture tables.
struct Tables;

impl SaveTables for Tables {
    fn stat_save(&self, id: u16) -> Option<StatSave> {
        (id < 16).then_some(StatSave {
            bits: 16,
            param: 0,
            signed: false,
        })
    }
    fn item_entry_len(&self, buf: &[u8]) -> Result<usize, String> {
        read_save_entry(buf, tables())
            .map(|e| e.len)
            .map_err(|e| e.to_string())
    }
}

/// `data` cut before each `JM` after the first byte.
fn records(data: &[u8]) -> Vec<ItemEntry> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..data.len().saturating_sub(1) {
        if &data[i..i + 2] == b"JM" {
            out.push(ItemEntry {
                bytes: data[start..i].to_vec(),
            });
            start = i;
        }
    }
    out.push(ItemEntry {
        bytes: data[start..].to_vec(),
    });
    out
}

fuzz_target!(|data: &[u8]| {
    d2_fuzz::arm("item_ids", data);
    let t = tables();
    let mut entries = records(data);
    let n = entries.len();
    let hireling = entries.split_off(n - n / 3);
    let corpse = entries.split_off(entries.len() / 2);
    let mut s = D2s::new_stub(b"Fuzz", 0, status::EXPANSION, 0).expect("name fits");
    s.body = Some(Body {
        items: entries,
        corpses: vec![Corpse {
            items: corpse,
            ..Corpse::default()
        }],
        hireling_items: Some(Some(hireling)),
        ..Body::default()
    });
    let found = save_ids(&s, t);
    assert!(found.errors.len() <= n, "at most one error per record");
    assert!(!found.ids.contains(&0), "0 means no ID and is never listed");

    // The whole input as a save, with and without its checksum fixed.
    for expansion in [false, true] {
        let options = d2s::ReadOptions {
            expansion,
            game: None,
        };
        if let Ok(save) = d2s::read(data, &options, &Tables) {
            let _ = save_ids(&save, t);
        }
        if data.len() >= d2s::HEADER_SIZE {
            let mut checked = data.to_vec();
            d2s::finish(&mut checked);
            if let Ok(save) = d2s::read(&checked, &options, &Tables) {
                let _ = save_ids(&save, t);
            }
        }
    }
});
