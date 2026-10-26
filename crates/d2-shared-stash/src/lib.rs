//! The shared stash as the game client sees it (multiplayer v2 plan E1,
//! modules N28a-c; single player too).
//!
//! Beside the character's own stash (tab P, unchanged), the stash has
//! shared pages for every character of the account (single player: every
//! character of the save folder) in one economy:
//! - **Item pages** I..X ([`PAGES`] grids of [`COLS`] x [`ROWS`] cells),
//!   items placed on cells as in the game's own stash.
//! - **Stackable pages** (Runes, Gems, Misc): one fixed slot per stackable
//!   kind of the [`Catalog`], each holding a count (`code → u32`), never
//!   item copies. A stackable dropped anywhere on these pages goes to its
//!   own slot; taking from a slot gives normal single items.
//! - **Gold**, apart from the character's gold.
//!
//! [`StashStore`] is the one seam the panel talks to. Two stores fill it:
//! - [`LocalStash`] (single player): the economy library's SQLite store
//!   in the game's save folder, next to the characters, one file per
//!   split (softcore or hardcore, expansion or classic). Every move is a
//!   transaction together with the character's record, and the character
//!   file is rewritten from that record, so a crash at any point loses or
//!   duplicates nothing ([`local`]).
//! - [`OnlineStash`]: the same calls as requests to the main server,
//!   which does the moves with the game server (contracts 11.13).
//!
//! Single player and online are separate economies: the local file is
//! never sent anywhere, and the online stash never touches it.

mod catalog;
mod cells;
pub mod local;
pub mod online;
mod pending;

#[cfg(test)]
mod tests;

pub use catalog::{Catalog, CategoryInfo, CategoryPage, Slot, StackPage, STACK_CAP};
pub use cells::{Cell, Grid, StashShape, CATEGORY_PAGE, COLS, PAGES, PAGE_NAME_MAX, ROWS};
pub use d2_economy::{EconomyKey, ItemFacts, ItemId, Refusal, Stackable};
pub use local::{GameError, Lift, Lifted, LocalGame, LocalStash, Recovered};
pub use online::{FromRealm, OnlineStash, RealmItem, RealmStash, ToRealm};

use std::collections::BTreeMap;

/// Where goods taken from the stash go on the character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakeTo {
    /// The cursor, as picking an item up from the game's stash does.
    Cursor,
    /// The first free inventory cells, as right-click moves do.
    Inventory,
}

/// An item on an item page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageItem {
    pub id: ItemId,
    pub cell: Cell,
    /// Width and height in cells.
    pub size: (u8, u8),
    /// The item's bytes, in the save format (the panel draws and
    /// describes the item from them).
    pub record: Vec<u8>,
    /// What the store knows about the item (single player; online the
    /// panel reads the bytes).
    pub facts: Option<ItemFacts>,
    /// The item as the game client last showed it (its item message
    /// stream), when kept: the panel's picture and tooltip read it.
    pub look: Option<Vec<u8>>,
}

/// What a put moves from the character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Put {
    /// One item (`unit`: its unit ID in the game), to `at` on an item page
    /// or the first free cells. A stackable goes to its slot instead.
    Item { unit: u32, at: Option<Cell> },
    /// Stackables only, each to its slot: the inventory's "Send stackables
    /// to stash" button and drops on a stackable page. Anything else
    /// stays on the character.
    Stacks { units: Vec<u32> },
    /// Gold from the character.
    Gold(u64),
}

/// What a take moves to the character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Take {
    Item {
        id: ItemId,
        to: TakeTo,
    },
    /// `count` of a stackable, as normal single items (or the game's own
    /// quantity stacks, such as keys) into the inventory; fewer when fewer
    /// fit.
    Stack {
        code: String,
        count: u32,
    },
    Gold(u64),
}

/// How a put went.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PutDone {
    /// Units now in the stash.
    pub moved: Vec<u32>,
    /// Units that stayed on the character, and why: not a stackable kind
    /// ([`Refusal::BadRequest`]), the slot's count at the catalog's cap
    /// ([`Catalog::cap`]) or the pages full ([`Refusal::StashFull`]).
    pub left: Vec<(u32, Refusal)>,
}

/// Why a stash call did nothing (or, for [`Put::Stacks`], stopped).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("refused: {0}")]
    Refused(Refusal),
    #[error("no room on the character")]
    NoRoom,
    #[error("game: {0}")]
    Game(String),
    #[error("stash storage: {0}")]
    Db(String),
    #[error("{0}")]
    Io(String),
    #[error("main server: {0}")]
    Link(String),
}

impl From<d2_economy::Error> for StoreError {
    fn from(e: d2_economy::Error) -> Self {
        match e {
            d2_economy::Error::Refused(r) => StoreError::Refused(r),
            d2_economy::Error::Db(s) => StoreError::Db(s),
        }
    }
}

impl From<d2_economy::rusqlite::Error> for StoreError {
    fn from(e: d2_economy::rusqlite::Error) -> Self {
        StoreError::Db(e.to_string())
    }
}

impl From<Refusal> for StoreError {
    fn from(r: Refusal) -> Self {
        StoreError::Refused(r)
    }
}

/// The shared stash the panel shows and changes.
pub trait StashStore {
    /// True for the main server's stash, false for single player's.
    fn online(&self) -> bool;
    /// The fixed stackable slots.
    fn catalog(&self) -> &Catalog;
    /// The items on item page `page` (0 is page I).
    fn page(&mut self, page: u8) -> Result<Vec<PageItem>, StoreError>;
    /// Every stackable count above 0, by code.
    fn stacks(&mut self) -> Result<BTreeMap<String, u32>, StoreError>;
    fn gold(&mut self) -> Result<u64, StoreError>;
    /// The items on any item page whose names contain `text` (any case).
    fn search(&mut self, text: &str) -> Result<Vec<(ItemId, Cell)>, StoreError>;
    fn put(&mut self, put: Put) -> Result<PutDone, StoreError>;
    /// Returns how many were taken (1 for an item; gold taken for gold).
    fn take(&mut self, take: Take) -> Result<u64, StoreError>;
    /// Moves an item to another free place on the item pages.
    fn move_item(&mut self, id: ItemId, to: Cell) -> Result<(), StoreError>;
    /// Packs page `page`: largest items first, then by kind and name.
    fn arrange(&mut self, page: u8) -> Result<(), StoreError>;
    /// The item pages' shape (D2Gamma's by default).
    fn shape(&self) -> StashShape {
        StashShape::DEFAULT
    }
    /// The item pages' names, page I first: the player's, else the
    /// default roman numeral.
    fn page_names(&mut self) -> Result<Vec<String>, StoreError> {
        Ok((0..self.shape().pages)
            .map(StashShape::default_name)
            .collect())
    }
    /// Names item page `page` (an empty name: back to the default).
    /// Refused unless the shape is [`StashShape::renamable`].
    fn rename_page(&mut self, page: u8, name: &str) -> Result<(), StoreError> {
        let _ = (page, name);
        Err(Refusal::BadRequest.into())
    }
}
