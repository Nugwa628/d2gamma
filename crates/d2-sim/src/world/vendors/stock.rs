//! A mod game's store stock ([`super::provider::StoreProvider::stock`]): the
//! items a store generation makes, in place of D2RUST's column lists.
//!
//! A mod's stores may rotate their stock by tier (base, magic, set, rare,
//! unique), each tier with its own item levels and a requirement the
//! player must meet. The provider reads the player through [`StockHost`]
//! (stats and quest slots, for its requirement checks) and draws only from
//! the NPC-control seed it is handed. Each [`StockItem`] then goes through
//! D2RUST's store item making (the upgrade roll, the cracked-item retries,
//! the store page and placing, the vendor marks), so a mod's stock looks
//! like D2RUST's in the store.

use super::{VendorTables, VendorWorld};
use crate::units::UnitId;

/// The store generation the provider answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockRequest {
    /// The NPC class whose store is made.
    pub class: u16,
    /// The NPC's act (0-based).
    pub act: u8,
    /// The player the store is made for.
    pub player: UnitId,
    /// The player's level (stat 12).
    pub player_level: i32,
    /// The game's difficulty (0-based).
    pub difficulty: u8,
    /// D2RUST's store item level for this player, act and difficulty.
    pub ilvl: i32,
}

/// One item of a mod's stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockItem {
    /// The item record ([`VendorTables::items`] index).
    pub record: usize,
    /// The quality asked for.
    pub quality: u8,
    /// The item level.
    pub ilvl: i32,
}

/// What a stock builder may read of the game: the player's stats and quest
/// slots (its tier requirements), the item format.
pub trait StockHost {
    /// Stat `id` (layer `layer`) of `unit`.
    fn stat(&self, unit: UnitId, id: u16, layer: u16) -> i32;
    /// Quest slot `slot` of `player` in `difficulty`.
    fn quest_slot(&self, player: UnitId, difficulty: u8, slot: u32) -> u16;
    /// The game's item format (100 in expansion games).
    fn item_format(&self) -> u16;
}

/// A vendor world as a [`StockHost`].
pub(crate) struct StockView<'a, W>(pub &'a W);

impl<W: VendorWorld> StockHost for StockView<'_, W> {
    fn stat(&self, unit: UnitId, id: u16, layer: u16) -> i32 {
        self.0.stat(unit, id, layer)
    }
    fn quest_slot(&self, player: UnitId, difficulty: u8, slot: u32) -> u16 {
        self.0.quest_slot(player, difficulty, slot)
    }
    fn item_format(&self) -> u16 {
        self.0.item_format()
    }
}

/// A record index of `t` the provider may name: in the item tables.
pub(crate) fn valid(t: &VendorTables, item: &StockItem) -> bool {
    item.record < t.items.len()
}
