//! A mod game's item rules past the property, event, use and quality
//! functions of [`super::provider`]: the runeword match, use and equip
//! requirements and the item-move veto. The hooks are default-bodied
//! methods of [`ItemProvider`](super::provider::ItemProvider); this file holds what they see and the
//! points the engine asks them at ([`runeword_for`],
//! [`requirements_met`](super::inventory::requirements_met),
//! [`handle`](super::moves::handle)). The ethereal and socket rolls of a
//! new item are asked in [`super::quality::dispatch`].
//!
//! Every default is the engine's answer, and under
//! [`crate::rules::Ruleset::Original`] no hook is asked.

use super::inventory::UnitKind;
use super::moves::{Guid, MovePending, MoveUnits, MoveWorld, Owner};
use super::props::runeword_row;
use super::tables::ItemTables;
use crate::units::UnitId;

/// A socketed item as the runeword match sees it.
pub struct RunewordItem<'a> {
    /// The item's row in the combined items array.
    pub record: usize,
    /// Its quality (`items::q`).
    pub quality: u8,
    /// Its socket count (stat 194, as a byte).
    pub sockets: u8,
    /// The items rows of its socket fillers, in insertion order.
    pub fillers: &'a [usize],
    /// The item's stat total on layer 0.
    pub stat: &'a dyn Fn(u16) -> i32,
}

/// The runes row the item forms: the mod's answer
/// ([`ItemProvider::runeword`](super::provider::ItemProvider::runeword)), else the engine's match
/// ([`runeword_row`]).
pub fn runeword_for(t: &ItemTables, item: &RunewordItem<'_>) -> Option<usize> {
    if let Some(answer) = t.rules.items().and_then(|p| p.runeword(t, item)) {
        return answer;
    }
    runeword_row(t, item.record, item.quality, item.sockets, item.fillers)
}

/// One requirement check: may `unit` use (or wear) `item`?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Requirement {
    /// The item.
    pub item: UnitId,
    /// Its row in the combined items array.
    pub record: usize,
    /// The unit that would use or wear it.
    pub unit: UnitId,
    /// What that unit is (`None`: not known).
    pub kind: Option<UnitKind>,
    /// The check is for equipping (the item's own bonuses count).
    pub equipping: bool,
    /// The item is ethereal.
    pub ethereal: bool,
    /// The item is identified.
    pub identified: bool,
    /// The engine's strength and dexterity requirements (the items row's,
    /// with the requirement percentage and the ethereal reduction).
    pub strength: i32,
    pub dexterity: i32,
    /// The engine's level requirement.
    pub level: i32,
    /// The engine's answer.
    pub engine: bool,
}

/// What a requirement hook reads.
pub trait RequirementView {
    /// A unit's stat total (layer 0).
    fn unit_stat(&self, unit: UnitId, stat: u16) -> i32;
    /// An item's stat total (layer 0).
    fn item_stat(&self, item: UnitId, stat: u16) -> i32;
}

/// One C→S item message a player sends (pick up, drop, move, equip,
/// stack, belt, socket, hireling items, ...).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemMove<'a> {
    /// The player.
    pub player: Owner,
    /// The message id (the first byte).
    pub id: u8,
    /// The whole message, its size already checked.
    pub msg: &'a [u8],
}

/// What the item-move veto reads, and its one way to answer the client.
pub trait MoveView {
    /// The player stands in a town.
    fn in_town(&self, player: Owner) -> bool;
    /// A unit's stat total (layer 0).
    fn stat(&self, unit: Owner, stat: u16) -> i32;
    /// The item's code (its items row's `code`).
    fn code(&self, item: Guid) -> [u8; 4];
    /// The item is of item type `ty` (or one of its equivalents).
    fn is_type(&self, item: Guid, ty: u16) -> bool;
    /// The item's page (`items::moves::page`).
    fn page(&self, item: Guid) -> u8;
    /// The unit holding the item, if any.
    fn item_owner(&self, item: Guid) -> Option<Owner>;
    /// The player's cursor item.
    fn cursor(&self, player: Owner) -> Option<Guid>;
    /// Sends `bytes` to the player's client.
    fn send(&mut self, player: Owner, bytes: &[u8]);
}

/// The item-move veto's view of the item-move world.
pub(crate) struct MoveViewOf<'a, W>(pub &'a mut W);

impl<W: MoveWorld> MoveView for MoveViewOf<'_, W> {
    fn in_town(&self, player: Owner) -> bool {
        self.0.in_town(player)
    }
    fn stat(&self, unit: Owner, stat: u16) -> i32 {
        MoveUnits::stat(&*self.0, unit, stat)
    }
    fn code(&self, item: Guid) -> [u8; 4] {
        self.0.code(item)
    }
    fn is_type(&self, item: Guid, ty: u16) -> bool {
        self.0.is_type(item, ty)
    }
    fn page(&self, item: Guid) -> u8 {
        self.0.page(item)
    }
    fn item_owner(&self, item: Guid) -> Option<Owner> {
        self.0.item_owner(item)
    }
    fn cursor(&self, player: Owner) -> Option<Guid> {
        self.0.cursor(player)
    }
    fn send(&mut self, player: Owner, bytes: &[u8]) {
        MovePending::send(self.0, player, bytes.to_vec());
    }
}
