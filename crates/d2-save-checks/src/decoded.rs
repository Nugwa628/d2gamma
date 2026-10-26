//! A save's items decoded once for every check, and the character's stats
//! read from the stats section.

use d2_formats::d2s::{D2s, ItemEntry};
use d2_sim::items::bitstream::read::{read_save_entry, ReadEntry};
use d2_sim::items::bitstream::StreamItem;

use crate::tables::CheckTables;

/// Where an item of a save is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// The player's own items (inventory, belt, equipment, stash, cube).
    Player,
    /// The items on the corpse.
    Corpse,
    /// The hireling's items.
    Hireling,
    /// The Iron Golem's item.
    Golem,
}

/// One item of a save: a top-level entry or a socketed child.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundItem {
    /// Its list.
    pub place: Place,
    /// The entry's index in its list.
    pub entry: usize,
    /// The index in [`Decoded::items`] of the item it is socketed in.
    pub parent: Option<usize>,
    /// Its items row (combined index).
    pub record: usize,
    /// The record as read; [`StreamItem::children`] is empty (children are
    /// items of their own).
    pub item: StreamItem,
    /// The records of its socketed children, in insertion order.
    pub fillers: Vec<usize>,
    /// The reader read the record to its end but it is not valid (a bad
    /// quality, set row or stat list).
    pub failed: bool,
}

impl FoundItem {
    /// Where the item is, for a failure message.
    pub fn at(&self) -> String {
        let code = String::from_utf8_lossy(&self.item.code)
            .trim_end()
            .to_owned();
        let place = match self.place {
            Place::Player => "item",
            Place::Corpse => "corpse item",
            Place::Hireling => "hireling item",
            Place::Golem => "golem item",
        };
        match self.parent {
            Some(_) => format!("{place} {} (socketed `{code}`)", self.entry),
            None => format!("{place} {} (`{code}`)", self.entry),
        }
    }

    /// The item's ID as the save carries it: the 32-bit seed of a full
    /// record (compact records carry none). The realm item IDs of the item
    /// registry (plan C3, N19) are checked through this one accessor.
    pub fn id(&self) -> Option<u32> {
        (!self.item.compact && !self.item.alt && self.item.unit28 != 0).then_some(self.item.unit28)
    }
}

/// A save's items, decoded.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Decoded {
    /// Every item, parents before their children.
    pub items: Vec<FoundItem>,
    /// Entries that did not decode with the realm's tables, with where
    /// they are and why.
    pub errors: Vec<String>,
}

impl Decoded {
    /// Decodes every item list of `save`.
    pub fn of(save: &D2s, t: &CheckTables) -> Self {
        let mut d = Decoded::default();
        let Some(body) = &save.body else {
            return d;
        };
        d.list(Place::Player, &body.items, t);
        for c in &body.corpses {
            d.list(Place::Corpse, &c.items, t);
        }
        if let Some(Some(h)) = &body.hireling_items {
            d.list(Place::Hireling, h, t);
        }
        if let Some(item) = body.golem.as_ref().and_then(|g| g.item.as_ref()) {
            d.list(Place::Golem, std::slice::from_ref(item), t);
        }
        d
    }

    fn list(&mut self, place: Place, entries: &[ItemEntry], t: &CheckTables) {
        for (i, e) in entries.iter().enumerate() {
            match read_save_entry(&e.bytes, &t.items) {
                Ok(read) => self.push(place, i, None, read),
                Err(err) => self.errors.push(format!("{place:?} entry {i}: {err}")),
            }
        }
    }

    fn push(&mut self, place: Place, entry: usize, parent: Option<usize>, read: ReadEntry) {
        let at = self.items.len();
        let mut item = read.item.item;
        item.children.clear();
        self.items.push(FoundItem {
            place,
            entry,
            parent,
            record: read.item.record,
            item,
            fillers: read.children.iter().map(|c| c.item.record).collect(),
            failed: read.item.failed,
        });
        for c in read.children {
            self.push(place, entry, Some(at), c);
        }
    }
}

/// Character stat ids of the stats section.
pub mod stat {
    pub const STRENGTH: u16 = 0;
    pub const ENERGY: u16 = 1;
    pub const DEXTERITY: u16 = 2;
    pub const VITALITY: u16 = 3;
    pub const STATPTS: u16 = 4;
    pub const NEWSKILLS: u16 = 5;
    pub const LEVEL: u16 = 12;
    pub const EXPERIENCE: u16 = 13;
    pub const GOLD: u16 = 14;
    pub const GOLDBANK: u16 = 15;
}

/// A character stat's base value (layer 0); 0 when the save has no body or
/// no such entry. Values read as unsigned 32-bit where the column is.
pub fn char_stat(save: &D2s, id: u16) -> i64 {
    save.body
        .as_ref()
        .and_then(|b| {
            b.stats
                .entries()
                .into_iter()
                .find(|e| e.id == id && e.layer == 0)
        })
        .map_or(0, |e| {
            if id == stat::EXPERIENCE {
                i64::from(e.value as u32)
            } else {
                i64::from(e.value)
            }
        })
}

/// Whether the save carries a stats entry for `id`.
pub fn has_char_stat(save: &D2s, id: u16) -> bool {
    save.body
        .as_ref()
        .is_some_and(|b| b.stats.entries().iter().any(|e| e.id == id && e.layer == 0))
}
