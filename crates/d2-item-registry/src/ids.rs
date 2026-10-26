//! The item IDs of a character record, as the save carries them.

use d2_formats::d2s::{D2s, ItemEntry};
use d2_sim::items::bitstream::read::{read_save_entry, ReadEntry};
use d2_sim::items::tables::ItemTables;

/// The IDs found in a record.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveIds {
    /// Every item ID, in record order: the player's items, the corpses',
    /// the hireling's and the golem's, each parent before its socketed
    /// children. Duplicates are kept (the registry refuses them).
    pub ids: Vec<u32>,
    /// Full records whose ID field is 0: items the registry can't follow.
    pub without_id: usize,
    /// Entries that didn't decode with the realm's item tables, with where
    /// they are and why (the save checks refuse such a record).
    pub errors: Vec<String>,
}

/// Reads every item ID of `save` with the realm's item tables. The ID is
/// the 32-bit field a full record carries after its socketed-child count
/// (`StreamItem::unit28`); compact and alt-code records carry none.
pub fn save_ids(save: &D2s, t: &ItemTables) -> SaveIds {
    let mut out = SaveIds::default();
    let Some(body) = &save.body else {
        return out;
    };
    list(&mut out, "item", &body.items, t);
    for c in &body.corpses {
        list(&mut out, "corpse item", &c.items, t);
    }
    if let Some(Some(h)) = &body.hireling_items {
        list(&mut out, "hireling item", h, t);
    }
    if let Some(item) = body.golem.as_ref().and_then(|g| g.item.as_ref()) {
        list(&mut out, "golem item", std::slice::from_ref(item), t);
    }
    out
}

fn list(out: &mut SaveIds, what: &str, entries: &[ItemEntry], t: &ItemTables) {
    for (i, e) in entries.iter().enumerate() {
        match read_save_entry(&e.bytes, t) {
            Ok(read) => entry(out, &read),
            Err(err) => out.errors.push(format!("{what} {i}: {err}")),
        }
    }
}

fn entry(out: &mut SaveIds, e: &ReadEntry) {
    let it = &e.item.item;
    if !it.compact && !it.alt {
        match it.unit28 {
            0 => out.without_id += 1,
            id => out.ids.push(id),
        }
    }
    for c in &e.children {
        entry(out, c);
    }
}
