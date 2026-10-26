//! Realm item IDs: the 32-bit item ID a character save carries for every
//! extended item (the field after the socket count,
//! [`StreamItem::unit28`](crate::items::bitstream::StreamItem::unit28)).
//!
//! Where an item's ID comes from is the game's [`ItemIdSource`]:
//!
//! - [`ItemIdSource::InitSeed`] (the default, single player and every game
//!   that is not a realm game): the ID is the item unit's init seed, as the
//!   save has always written it. Nothing is stored and nothing changes.
//! - [`ItemIdSource::Blocks`] (a realm game): IDs come from blocks of IDs the
//!   main server granted the game ([`BlockIds`]). An item takes its ID the
//!   first time it is written into a character save, so the countless items
//!   that drop, sit in vendor stock and vanish never use one. A loaded item
//!   keeps the ID its save carried; a copy of an item (vendor sale, hireling
//!   take, corpse, cube) takes over its source's ID; an item that leaves an
//!   inactive room and comes back keeps its ID.
//!
//! The ID is kept apart from the item's seeds: the unit seed and the item
//! seed drive the item's rolls and stay exactly what they are without realm
//! IDs, so a realm game runs the same simulation as the same game with the
//! default source. The one place where the two meet is the save field
//! itself: an item loaded from a save takes the field as its init seed (as it
//! always has), and in a realm game that field holds the ID. Its rolls are
//! already fixed in the save's other fields, so nothing is re-rolled.
//!
//! Every ID handed out is deterministic: the draws follow the game's save
//! order, and the blocks arrive as host inputs (the journal's `ItemIds`
//! records), so a replay of the game hands out the same IDs.

use std::collections::{BTreeMap, VecDeque};

use crate::units::UnitId;

/// IDs per block the main server grants (contracts 11.3, `ItemIdBlock`).
pub const BLOCK_SIZE: u32 = 4096;

/// When this many IDs or fewer are left, the game asks for the next block,
/// so a block arrives long before the last ID is used.
pub const LOW_WATER: u32 = BLOCK_SIZE / 4;

/// A block of realm item IDs: `first`, `first + 1`, …, `first + count − 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IdBlock {
    pub first: u32,
    pub count: u32,
}

impl IdBlock {
    /// One past the last ID (as `u64`: a block may end at `u32::MAX`).
    fn end(&self) -> u64 {
        u64::from(self.first) + u64::from(self.count)
    }

    fn overlaps(&self, other: &IdBlock) -> bool {
        u64::from(self.first) < other.end() && u64::from(other.first) < self.end()
    }
}

/// Why a granted block was not taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GrantError {
    /// The game's IDs are its items' init seeds; it takes no blocks.
    #[error("the game takes no item ID blocks")]
    NotBlocks,
    /// A block of no IDs.
    #[error("empty item ID block")]
    Empty,
    /// The block holds ID 0 (no ID) or runs past `u32::MAX`.
    #[error("item ID block {first}+{count} out of range")]
    OutOfRange { first: u32, count: u32 },
    /// The block shares IDs with a block granted before: an ID is never
    /// handed out twice.
    #[error("item ID block {first}+{count} overlaps a block granted before")]
    Overlap { first: u32, count: u32 },
}

/// IDs taken from granted blocks, in grant order, each once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BlockIds {
    /// The unused part of each granted block, oldest first.
    left: VecDeque<IdBlock>,
    /// Every block ever granted (the reuse check).
    granted: Vec<IdBlock>,
    /// A request for a block is out and not answered yet.
    asking: bool,
    /// Requests raised and not yet taken by the host.
    wanted: u32,
    /// IDs handed out.
    drawn: u64,
    /// Draws that found no ID left (the item was saved without an ID).
    starved: u64,
}

impl BlockIds {
    /// No IDs yet: the source asks for its first block at once (a realm
    /// game normally starts with one, see [`Self::with_block`]).
    pub fn new() -> Self {
        let mut s = Self::default();
        s.check_low();
        s
    }

    /// Starts with `first` (`StartGame`'s block).
    pub fn with_block(first: IdBlock) -> Result<Self, GrantError> {
        let mut s = Self::default();
        s.grant(first)?;
        Ok(s)
    }

    /// Adds a granted block (an answer to a request, or one the main server
    /// sends unasked).
    pub fn grant(&mut self, block: IdBlock) -> Result<(), GrantError> {
        if block.count == 0 {
            return Err(GrantError::Empty);
        }
        if block.first == 0 || block.end() > u64::from(u32::MAX) + 1 {
            return Err(GrantError::OutOfRange {
                first: block.first,
                count: block.count,
            });
        }
        if self.granted.iter().any(|g| g.overlaps(&block)) {
            return Err(GrantError::Overlap {
                first: block.first,
                count: block.count,
            });
        }
        self.granted.push(block);
        self.left.push_back(block);
        self.asking = false;
        self.check_low();
        Ok(())
    }

    /// The next unused ID, or `None` when every granted ID is used.
    pub fn next_id(&mut self) -> Option<u32> {
        let id = loop {
            let b = self.left.front_mut()?;
            if b.count == 0 {
                self.left.pop_front();
                continue;
            }
            let id = b.first;
            b.count -= 1;
            b.first = b.first.wrapping_add(1);
            break id;
        };
        self.drawn += 1;
        self.check_low();
        Some(id)
    }

    /// The IDs not handed out yet.
    pub fn remaining(&self) -> u64 {
        self.left.iter().map(|b| u64::from(b.count)).sum()
    }

    /// The requests for a block raised since the last call (each one
    /// `ItemIdsWanted` on the link).
    pub fn take_wanted(&mut self) -> u32 {
        std::mem::take(&mut self.wanted)
    }

    /// A request is out and not answered.
    pub fn asking(&self) -> bool {
        self.asking
    }

    /// IDs handed out so far.
    pub fn drawn(&self) -> u64 {
        self.drawn
    }

    /// Draws that found no ID left.
    pub fn starved(&self) -> u64 {
        self.starved
    }

    /// Every block granted, in grant order.
    pub fn granted(&self) -> &[IdBlock] {
        &self.granted
    }

    /// Asks for a block when few IDs are left and no request is out.
    fn check_low(&mut self) {
        if !self.asking && self.remaining() <= u64::from(LOW_WATER) {
            self.asking = true;
            self.wanted += 1;
        }
    }
}

/// Where a game's item IDs come from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ItemIdSource {
    /// The item's init seed is its ID (no realm IDs).
    #[default]
    InitSeed,
    /// Realm IDs from granted blocks.
    Blocks(BlockIds),
}

/// The game's item IDs: the source and the ID of each item that has one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemIds {
    source: ItemIdSource,
    ids: BTreeMap<UnitId, u32>,
}

impl ItemIds {
    /// IDs from `source`.
    pub fn new(source: ItemIdSource) -> Self {
        Self {
            source,
            ids: BTreeMap::new(),
        }
    }

    pub fn source(&self) -> &ItemIdSource {
        &self.source
    }

    /// The block source, when the game takes realm IDs.
    pub fn blocks(&self) -> Option<&BlockIds> {
        match &self.source {
            ItemIdSource::Blocks(b) => Some(b),
            ItemIdSource::InitSeed => None,
        }
    }

    pub fn blocks_mut(&mut self) -> Option<&mut BlockIds> {
        match &mut self.source {
            ItemIdSource::Blocks(b) => Some(b),
            ItemIdSource::InitSeed => None,
        }
    }

    /// Whether IDs come from blocks.
    pub fn realm(&self) -> bool {
        self.blocks().is_some()
    }

    /// Adds a granted block ([`BlockIds::grant`]).
    pub fn grant(&mut self, block: IdBlock) -> Result<(), GrantError> {
        self.blocks_mut().ok_or(GrantError::NotBlocks)?.grant(block)
    }

    /// The block requests raised since the last call ([`BlockIds::take_wanted`]).
    pub fn take_wanted(&mut self) -> u32 {
        self.blocks_mut().map_or(0, BlockIds::take_wanted)
    }

    /// The realm ID `unit` holds, if any.
    pub fn id_of(&self, unit: UnitId) -> Option<u32> {
        self.ids.get(&unit).copied()
    }

    /// The value the save writes in `unit`'s ID field. The default source:
    /// `init_seed`. Realm IDs: the item's ID, drawn now when it has none
    /// yet; 0 (no ID) when no ID is left. `extended`: the record has the
    /// field at all (compact and alternate-code records do not, and draw
    /// nothing).
    pub fn save_id(&mut self, unit: UnitId, init_seed: u32, extended: bool) -> u32 {
        let ItemIdSource::Blocks(b) = &mut self.source else {
            return init_seed;
        };
        if !extended {
            return 0;
        }
        if let Some(&id) = self.ids.get(&unit) {
            return id;
        }
        match b.next_id() {
            Some(id) => {
                self.ids.insert(unit, id);
                id
            }
            None => {
                b.starved += 1;
                0
            }
        }
    }

    /// A new item unit: whatever a freed unit in the same slot held is
    /// gone.
    pub fn created(&mut self, unit: UnitId) {
        self.ids.remove(&unit);
    }

    /// An item unit made from a save record whose ID field holds `field`:
    /// with realm IDs, a non-zero field is the item's ID.
    pub fn loaded(&mut self, unit: UnitId, field: u32) {
        self.ids.remove(&unit);
        if self.realm() && field != 0 {
            self.ids.insert(unit, field);
        }
    }

    /// `to` was made as a copy of `from` (which the copy replaces): the ID
    /// moves to the copy, so the two never share it.
    pub fn moved(&mut self, from: UnitId, to: UnitId) {
        self.ids.remove(&to);
        if let Some(id) = self.ids.remove(&from) {
            self.ids.insert(to, id);
        }
    }

    /// The item units holding an ID, with their IDs.
    pub fn iter(&self) -> impl Iterator<Item = (UnitId, u32)> + '_ {
        self.ids.iter().map(|(&u, &id)| (u, id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(first: u32, count: u32) -> IdBlock {
        IdBlock { first, count }
    }

    #[test]
    fn hands_out_a_block_in_order_then_none() {
        let mut b = BlockIds::with_block(block(100, 3)).unwrap();
        assert_eq!(
            (b.next_id(), b.next_id(), b.next_id(), b.next_id()),
            (Some(100), Some(101), Some(102), None)
        );
        assert_eq!(b.drawn(), 3);
    }

    #[test]
    fn running_low_asks_once_until_answered() {
        let mut b = BlockIds::with_block(block(1, BLOCK_SIZE)).unwrap();
        assert_eq!(b.take_wanted(), 0, "a full block is not low");
        let first_ask = BLOCK_SIZE - LOW_WATER;
        for _ in 0..first_ask - 1 {
            b.next_id().unwrap();
        }
        assert_eq!(b.take_wanted(), 0, "one more than the low-water mark left");
        b.next_id().unwrap();
        assert_eq!(b.take_wanted(), 1, "at the mark: one request");
        assert!(b.asking());
        for _ in 0..LOW_WATER {
            b.next_id().unwrap();
        }
        assert_eq!(b.take_wanted(), 0, "no second request while one is out");
        assert_eq!(b.next_id(), None);
        b.grant(block(1 + BLOCK_SIZE, BLOCK_SIZE)).unwrap();
        assert!(!b.asking());
        assert_eq!(b.next_id(), Some(1 + BLOCK_SIZE));
    }

    #[test]
    fn a_small_grant_asks_again_at_once() {
        let mut b = BlockIds::new();
        assert_eq!(b.take_wanted(), 1, "no block: ask at once");
        b.grant(block(10, 5)).unwrap();
        assert_eq!(b.take_wanted(), 1, "five IDs are below the mark");
    }

    #[test]
    fn blocks_are_used_in_grant_order() {
        let mut b = BlockIds::with_block(block(50, 2)).unwrap();
        b.grant(block(7, 1)).unwrap();
        let ids: Vec<_> = std::iter::from_fn(|| b.next_id()).collect();
        assert_eq!(ids, [50, 51, 7]);
    }

    #[test]
    fn bad_grants_are_refused() {
        let mut b = BlockIds::with_block(block(100, 10)).unwrap();
        assert_eq!(b.grant(block(5, 0)), Err(GrantError::Empty));
        assert_eq!(
            b.grant(block(0, 4)),
            Err(GrantError::OutOfRange { first: 0, count: 4 })
        );
        assert_eq!(
            b.grant(block(u32::MAX, 2)),
            Err(GrantError::OutOfRange {
                first: u32::MAX,
                count: 2
            })
        );
        assert!(b.grant(block(u32::MAX, 1)).is_ok(), "the last ID is valid");
        assert_eq!(
            b.grant(block(109, 5)),
            Err(GrantError::Overlap {
                first: 109,
                count: 5
            })
        );
        // A used-up block still counts: its IDs were handed out.
        for _ in 0..10 {
            b.next_id().unwrap();
        }
        assert_eq!(
            b.grant(block(95, 6)),
            Err(GrantError::Overlap {
                first: 95,
                count: 6
            })
        );
        assert!(b.grant(block(110, 1)).is_ok(), "adjacent is not overlap");
    }

    #[test]
    fn default_source_writes_the_init_seed_and_stores_nothing() {
        let mut ids = ItemIds::default();
        assert_eq!(ids.save_id(UnitId(3), 0xABCD, true), 0xABCD);
        assert_eq!(ids.save_id(UnitId(3), 0x1234, false), 0x1234);
        ids.loaded(UnitId(4), 77);
        assert_eq!(ids.id_of(UnitId(4)), None);
        assert_eq!(ids.grant(block(1, 1)), Err(GrantError::NotBlocks));
        assert_eq!(ids.take_wanted(), 0);
        assert_eq!(ids.iter().count(), 0);
    }

    #[test]
    fn an_item_draws_once_and_keeps_its_id() {
        let mut ids = ItemIds::new(ItemIdSource::Blocks(
            BlockIds::with_block(block(500, 10)).unwrap(),
        ));
        assert_eq!(ids.save_id(UnitId(1), 9, true), 500);
        assert_eq!(ids.save_id(UnitId(2), 9, true), 501);
        assert_eq!(ids.save_id(UnitId(1), 9, true), 500, "kept");
        assert_eq!(ids.save_id(UnitId(3), 9, false), 0, "compact: no ID");
        assert_eq!(ids.blocks().unwrap().drawn(), 2);
    }

    #[test]
    fn loads_copies_and_new_units() {
        let mut ids = ItemIds::new(ItemIdSource::Blocks(
            BlockIds::with_block(block(500, 10)).unwrap(),
        ));
        ids.loaded(UnitId(1), 42);
        ids.loaded(UnitId(2), 0);
        assert_eq!(
            (ids.id_of(UnitId(1)), ids.id_of(UnitId(2))),
            (Some(42), None)
        );
        ids.moved(UnitId(1), UnitId(5));
        assert_eq!(
            (ids.id_of(UnitId(1)), ids.id_of(UnitId(5))),
            (None, Some(42))
        );
        // The source draws afresh if it is saved after all.
        assert_eq!(ids.save_id(UnitId(1), 0, true), 500);
        // A slot reused by a new item holds no old ID.
        ids.created(UnitId(5));
        assert_eq!(ids.save_id(UnitId(5), 0, true), 501);
    }

    #[test]
    fn starved_draws_write_no_id_and_count() {
        let mut ids = ItemIds::new(ItemIdSource::Blocks(
            BlockIds::with_block(block(500, 1)).unwrap(),
        ));
        assert_eq!(ids.save_id(UnitId(1), 0, true), 500);
        assert_eq!(ids.save_id(UnitId(2), 0, true), 0);
        assert_eq!(ids.blocks().unwrap().starved(), 1);
        ids.grant(block(900, 4)).unwrap();
        assert_eq!(
            ids.save_id(UnitId(2), 0, true),
            900,
            "drawn at the next save"
        );
    }
}
