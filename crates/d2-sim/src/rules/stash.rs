//! Stash seams: a mod game's own stash on the server.
//!
//! A mod game may keep a stash of its own (more pages, a larger grid,
//! counters for currencies) as item records in its own state and files.
//! D2Gamma's stash and shared stash are unchanged; these seams only give
//! the mod's stash what it needs from the game:
//! - [`ItemHost`]: the player's items as the engine holds them (list,
//!   read a record, destroy, place a record, make a fresh item by code,
//!   room and cursor queries, the town test). The engine implements it.
//! - [`StashHooks`]: the mod's side. Its client's stash requests arrive
//!   with item access ([`StashHooks::message`]); it decides where an item
//!   sent to the stash goes ([`StashHooks::put`]); it adds stock to NPC
//!   costs and requirements and takes its share of a payment
//!   ([`StashHooks::stock`], [`StashHooks::pay`]), and says what an item
//!   handed to an online stash is worth in counts ([`StashHooks::credits`]).
//! - [`put`] and [`send`]: the engine applies a move into the mod's stash:
//!   the town test, the item read, the mod's answer, then the item leaves
//!   the game. A refused move changes nothing.
//!
//! The mod reads and patches records with [`crate::items::record`].

use std::fmt::Debug;

use super::ModOutbox;
use crate::items::record::{self, RecordFacts};
use crate::units::UnitId;

/// Where an item of the player is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemPlace {
    Cursor,
    Inventory,
    Cube,
    /// The character's own stash page.
    Stash,
    Equipped,
    Belt,
}

/// One item the player holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeldItem {
    pub guid: u32,
    /// Space padded (`*b"r01 "`).
    pub code: [u8; 4],
    /// Width and height in cells.
    pub size: (u8, u8),
    pub place: ItemPlace,
    /// The grid cell (inventory, cube, stash), else the slot.
    pub cell: (i32, i32),
}

/// Where a placed or made item goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceAt {
    /// The cursor; it must be free.
    Cursor,
    /// The inventory: at the cell when free, else the first free place.
    Inventory(Option<(i32, i32)>),
}

/// Why an item call did nothing.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ItemError {
    #[error("no such item on the player")]
    NoItem,
    #[error("no room")]
    NoRoom,
    #[error("the cursor holds an item")]
    CursorBusy,
    #[error("the game has no item {0:?}")]
    UnknownCode([u8; 4]),
    #[error("{0}")]
    Refused(String),
}

/// The player's items as the engine holds them, for a mod's stash. Every
/// change is sent to the clients as the game's own item moves are.
pub trait ItemHost {
    /// Every item of the player: cursor, inventory, cube, stash page,
    /// equipped and belt (socketed items are part of their holder).
    fn items(&mut self, player: UnitId) -> Vec<HeldItem>;
    /// The item's value of `stat` (layer 0), e.g. a quantity or charges.
    fn item_stat(&mut self, player: UnitId, guid: u32, stat: u16) -> Option<i32>;
    /// The player stands in a town.
    fn in_town(&mut self, player: UnitId) -> bool;
    /// The player's cursor holds no item.
    fn cursor_free(&mut self, player: UnitId) -> bool;
    /// The size of item `code` in the game's item table.
    fn size_of(&self, code: [u8; 4]) -> Option<(u8, u8)>;
    /// How many fresh `code` items fit in the inventory now, one per item.
    fn room(&mut self, player: UnitId, code: [u8; 4]) -> u32;
    /// The item's save record (with its socketed items), without moving
    /// it.
    fn record(&mut self, player: UnitId, guid: u32) -> Option<Vec<u8>>;
    /// Removes the item (and its socketed items) from the game. False when
    /// the player holds no such item.
    fn destroy(&mut self, player: UnitId, guid: u32) -> bool;
    /// Makes the item of save record `record` and puts it there; returns
    /// its GUID. On failure nothing is left in the game.
    fn place(&mut self, player: UnitId, record: &[u8], at: PlaceAt) -> Result<u32, ItemError>;
    /// Makes a fresh item of `code` (normal quality, identified), sets
    /// `stats` (layer 0; e.g. a container's count) and puts it there;
    /// returns its GUID. On failure nothing is left in the game.
    fn create(
        &mut self,
        player: UnitId,
        code: [u8; 4],
        stats: &[(u16, i32)],
        at: PlaceAt,
    ) -> Result<u32, ItemError>;
    /// What save record `record` says (code, size, position).
    fn facts(&self, record: &[u8]) -> Option<RecordFacts> {
        record::facts(record, |c| self.size_of(c))
    }
}

/// How an item is sent to the mod's stash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StashDrop {
    /// A quick move (Ctrl+click): the mod picks the place.
    Quick,
    /// The send-to-stash button's pass over the inventory and cube.
    Send,
    /// A drop on a cell of a page the mod shows (`page` is the mod's own
    /// page number).
    At { page: u16, x: u8, y: u8 },
}

/// Where the mod put an item sent to its stash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StashDest {
    /// Credited to a counter: the item itself is gone.
    Deposit,
    /// Kept whole on a page at a cell.
    Page { page: u16, x: u8, y: u8 },
}

/// Why an item stays where it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StashRefusal {
    #[error("only in town")]
    NotInTown,
    #[error("no such item")]
    NoItem,
    /// Worn, in the belt, or a kind the stash does not take.
    #[error("not for the stash")]
    NotStashable,
    #[error("the stash is full")]
    Full,
}

/// A mod game's own stash on the server. Every method defaults to "not
/// the mod's".
pub trait StashHooks: Send + Sync + Debug {
    /// A mod message from `player`'s client (as [`super::ModRules::message`])
    /// with the player's items at hand: the stash panel's requests
    /// (withdraws, quick moves, the send-to-stash button). `true`: handled
    /// here, [`super::ModRules::message`] is not called.
    fn message(
        &self,
        items: &mut dyn ItemHost,
        player: UnitId,
        payload: &[u8],
        out: &mut dyn ModOutbox,
    ) -> bool {
        let _ = (items, player, payload, out);
        false
    }

    /// Moves into the stash are refused outside towns (default `true`).
    fn town_only(&self) -> bool {
        true
    }

    /// Takes `item` (its save record `record`) into the mod's stash: the
    /// mod stores it or credits it and says where. Called by [`put`] and
    /// [`send`]; after `Ok` the engine removes the item from the game.
    fn put(
        &self,
        player: UnitId,
        item: &HeldItem,
        record: &[u8],
        drop: StashDrop,
    ) -> Result<StashDest, StashRefusal> {
        let _ = (player, item, record, drop);
        Err(StashRefusal::NotStashable)
    }

    /// Undoes an accepted [`StashHooks::put`] whose item could not be
    /// removed from the game after all.
    fn put_undone(&self, player: UnitId, item: &HeldItem, dest: StashDest) {
        let _ = (player, item, dest);
    }

    /// How many of `code` the stash adds to what `player` carries for NPC
    /// costs and requirements (`in_town`: the player stands in a town).
    fn stock(&self, player: UnitId, code: [u8; 4], in_town: bool) -> u32 {
        let _ = (player, code, in_town);
        0
    }

    /// What item `guid` of `player` is worth in counts when it goes into
    /// an online stash: a container that holds currencies, say, credits
    /// what it holds, and the game server then sends the counts with the
    /// item instead of the item as an item. Read the item through `items`
    /// (its code, its stats); nothing may change. Empty (the default): the
    /// item goes up as itself.
    fn credits(&self, items: &mut dyn ItemHost, player: UnitId, guid: u32) -> Vec<Credit> {
        let _ = (items, player, guid);
        Vec::new()
    }

    /// Takes up to `need` of `code` from the stash for a payment the
    /// carried items did not cover; returns how many it took.
    fn pay(&self, player: UnitId, code: [u8; 4], need: u32, in_town: bool) -> u32 {
        let _ = (player, code, need, in_town);
        0
    }
}

/// One count an item folds into: the stackable's code as the stash names
/// it (`r01`, no padding) and how many.
pub type Credit = (String, u32);

/// What the mod's stash says item `guid` of `player` is worth in counts
/// when it is put into an online stash, cleaned up: empty codes and zero
/// counts dropped, repeated codes added together, in the order each code
/// first appears. Empty: the item is not folded (it goes up as itself).
pub fn credits(
    items: &mut dyn ItemHost,
    hooks: &dyn StashHooks,
    player: UnitId,
    guid: u32,
) -> Vec<Credit> {
    let mut out: Vec<Credit> = Vec::new();
    for (code, count) in hooks.credits(items, player, guid) {
        if code.is_empty() || count == 0 {
            continue;
        }
        match out.iter_mut().find(|(c, _)| *c == code) {
            Some((_, n)) => *n = n.saturating_add(count),
            None => out.push((code, count)),
        }
    }
    out
}

/// Sends `player`'s item `guid` into the mod's stash: refused outside a
/// town (when the mod asks), for an item the player does not hold, or one
/// worn, in the belt or on the character's stash page; else the mod
/// decides, and an accepted item leaves the game.
pub fn put(
    items: &mut dyn ItemHost,
    hooks: &dyn StashHooks,
    player: UnitId,
    guid: u32,
    drop: StashDrop,
) -> Result<StashDest, StashRefusal> {
    if hooks.town_only() && !items.in_town(player) {
        return Err(StashRefusal::NotInTown);
    }
    let item = items
        .items(player)
        .into_iter()
        .find(|i| i.guid == guid)
        .ok_or(StashRefusal::NoItem)?;
    put_held(items, hooks, player, &item, drop)
}

fn put_held(
    items: &mut dyn ItemHost,
    hooks: &dyn StashHooks,
    player: UnitId,
    item: &HeldItem,
    drop: StashDrop,
) -> Result<StashDest, StashRefusal> {
    if !matches!(
        item.place,
        ItemPlace::Cursor | ItemPlace::Inventory | ItemPlace::Cube
    ) {
        return Err(StashRefusal::NotStashable);
    }
    let record = items
        .record(player, item.guid)
        .ok_or(StashRefusal::NoItem)?;
    let dest = hooks.put(player, item, &record, drop)?;
    if !items.destroy(player, item.guid) {
        hooks.put_undone(player, item, dest);
        return Err(StashRefusal::NoItem);
    }
    Ok(dest)
}

/// What [`send`] did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SendReport {
    /// The items now in the stash, and where.
    pub sent: Vec<(u32, StashDest)>,
    /// The items the mod refused, and why.
    pub left: Vec<(u32, StashRefusal)>,
}

/// The send-to-stash button: offers every inventory and cube item of
/// `player` to the mod's stash, in the engine's item order. Equipped
/// items, the belt and the cursor are never touched. Outside a town
/// (when the mod asks) nothing moves.
pub fn send(
    items: &mut dyn ItemHost,
    hooks: &dyn StashHooks,
    player: UnitId,
) -> Result<SendReport, StashRefusal> {
    if hooks.town_only() && !items.in_town(player) {
        return Err(StashRefusal::NotInTown);
    }
    let mut report = SendReport::default();
    for item in items.items(player) {
        if !matches!(item.place, ItemPlace::Inventory | ItemPlace::Cube) {
            continue;
        }
        match put_held(items, hooks, player, &item, StashDrop::Send) {
            Ok(dest) => report.sent.push((item.guid, dest)),
            Err(why) => report.left.push((item.guid, why)),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
