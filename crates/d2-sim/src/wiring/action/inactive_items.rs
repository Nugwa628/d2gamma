//! The item records of the inactive store on the action wiring: the
//! save-form bit stream of a ground item ([`bitstream::write_save`]) and
//! its re-creation on the room's first population
//! ([`Economy::item_from_record`]: a new item unit, a new GUID). The item
//! tables are the drop state's (`ActionHooks::object_drops`); without it
//! the host's [`Pending`] seams answer (no record).
//!
//! Socketed children (the item's own inventory, which the inventory state
//! holds: [`Pending::socketed_items`]) follow the parent's stream, each a
//! byte-aligned save stream of its own in list order, and the parent's
//! filled count names how many; the restore re-creates them after the
//! parent and hands each to [`Pending::socket_restore`].

use std::sync::Arc;

use crate::game::Game;
use crate::items::bitstream::{self, read};
use crate::items::ItemTables;
use crate::rng::Seed;
use crate::units::inactive::ItemRecord;
use crate::units::lifecycle::LifecycleHooks;
use crate::units::{RoomId, UnitId};
use crate::wiring::economy::{DropTables, Economy, EconomyError, GameFields};
use crate::wiring::inventory::bits::{stream_item_of, ItemPlace};

use super::{Pending, View};

/// The owner value of an item with no owner player (−1).
const NO_OWNER: u32 = u32::MAX;

impl<X: Pending> View<'_, X> {
    fn drop_tables(&self) -> Option<Arc<DropTables>> {
        self.h.object_drops.as_ref().map(|d| d.tables.clone())
    }

    /// The record of the ground item `u`: its save stream,
    /// then each socketed child's (filled count = the children), or none
    /// when a stream view is missing or a stream does not fit.
    pub(super) fn item_record_of(&self, u: UnitId) -> Option<ItemRecord> {
        let t = self.drop_tables()?;
        let children = self.h.x.socketed_items(u);
        let place = ItemPlace {
            x: 0,
            y: 0,
            body_loc: 0,
            filled: children.len() as u32,
            runeword: 0xFFFF,
            static_pos: Some(self.h.path_position(u)),
        };
        let mut bytes = self.save_stream(&t.items, u, &place)?;
        for (i, &c) in children.iter().enumerate() {
            let place = ItemPlace {
                x: i as i32,
                y: 0,
                body_loc: 0,
                filled: 0,
                runeword: 0xFFFF,
                static_pos: None,
            };
            bytes.extend(self.save_stream(&t.items, c, &place)?);
        }
        Some(ItemRecord {
            expiry: self.h.items.get(u)?.ground_expiry,
            value_629f20: NO_OWNER,
            bytes,
        })
    }

    /// The save-form stream of item `u` at `place`. Its ID field is 0, or
    /// in a realm game the item's realm ID when it has one, so the item
    /// keeps it through the restore (`economy::item_ids`).
    fn save_stream(&self, t: &ItemTables, u: UnitId, place: &ItemPlace) -> Option<Vec<u8>> {
        let mut view =
            stream_item_of(&*self.units, &self.h.items, &*self.stats, t, u, place, 0, 0)?;
        if let Some(id) = self.h.items.ids().id_of(u) {
            view.unit28 = id;
        }
        bitstream::write_save(&view, &t.isc).ok().map(|(b, _)| b)
    }

    /// The item of `rec` re-created in `room` with ground
    /// expiry `expiry` (a new unit and GUID).
    pub(super) fn restore_item_record(
        &mut self,
        game: &mut Game,
        room: RoomId,
        rec: &ItemRecord,
        expiry: i32,
    ) {
        let Some(t) = self.drop_tables() else {
            return;
        };
        // The parent, then its filled count of child streams; a child that
        // does not read is fatal (error 0x10C): nothing is made.
        let Ok(entry) = read::read_save_entry(&rec.bytes, &t.items) else {
            return;
        };
        let first = &entry.item;
        if first.failed {
            return;
        }
        let mut fields = GameFields::from_action(
            self.h.game_seed,
            &self.h.ai_info,
            self.data.expansion,
            std::mem::take(&mut self.h.uniques),
        );
        let mut items = std::mem::take(&mut self.h.items);
        let made = {
            let mut econ = Economy {
                game: &mut *game,
                units: &mut *self.units,
                stats: &mut *self.stats,
                data: self.data,
                hooks: &mut *self.h,
                fields: &mut fields,
                tables: &t.items,
                items: &mut items,
            };
            restored_item(&mut econ, first, Some(room)).map(|u| {
                let kids: Vec<UnitId> = entry
                    .children
                    .iter()
                    .filter(|c| !c.item.failed)
                    .filter_map(|c| restored_item(&mut econ, &c.item, None).ok())
                    .collect();
                (u, kids)
            })
        };
        self.h.game_seed = fields.seed;
        self.h.uniques = std::mem::take(&mut fields.uniques);
        let Ok((u, kids)) = made else {
            self.h.items = items;
            return;
        };
        if let Some(it) = items.get_mut(u) {
            it.ground_expiry = expiry;
        }
        self.h.items = items;
        self.path_place(game, u, first.item.x, first.item.y);
        for c in kids {
            self.h.x.socket_restore(game, u, c);
        }
    }
}

/// One item of an inactive record made again. The record's ID field is 0
/// unless it carries a realm ID ([`View::item_record_of`]); the item takes
/// the ID, and its seeds are those of a field of 0, as without realm IDs.
fn restored_item<H: LifecycleHooks>(
    econ: &mut Economy<'_, H>,
    rec: &read::ReadItem,
    room: Option<RoomId>,
) -> Result<UnitId, EconomyError> {
    let u = econ.item_from_record(rec, room)?;
    if rec.item.unit28 != 0 {
        if let Some(r) = econ.units.get_mut(u) {
            r.init_seed = 0;
            r.seed = Seed::init_low(0);
        }
    }
    Ok(u)
}
