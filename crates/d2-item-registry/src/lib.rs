//! The item registry of one mod realm (multiplayer v2, plan C3 and N19;
//! contracts 11.3, 11.11 and 11.13).
//!
//! - **Item IDs.** Every extended item carries a realm item ID: the 32-bit
//!   field a full save record holds after its socketed-child count
//!   (`d2_sim::items::bitstream::StreamItem::unit28`, the item unit's
//!   init seed; [`save_ids`] reads them). Compact records (runes, gems,
//!   potions, keys, ears, gold) and alt-code records carry none, and 0
//!   means none: those items are left to the replay audit, and the shared
//!   stash keeps them as counts.
//! - **Blocks.** The main server grants each game blocks of IDs
//!   ([`Registry::grant`]: `StartGame` and `ItemIds`, 4096 at a time) from
//!   the mod's one rising counter. A block never holds an ID that is
//!   registered already, and no ID is granted twice.
//! - **Saves.** [`Registry::accept_save`] judges every ID of a returned
//!   save: an ID that is on that character, or that an item of the same
//!   game had (a trade, a pick-up), or that comes from a block of that
//!   game passes; an ID held elsewhere (another character outside the
//!   game, a stash, escrow, a delivery) or never granted to the game is
//!   refused, and nothing is written. Accepted saves move the IDs to the
//!   character and keep their history.
//! - **First sight.** Items of a record the registry never saw (v1
//!   characters, the A2b migration of N25) keep their saved ID:
//!   [`Registry::adopt`] registers it on that character, and an ID found
//!   on a second character is kept on the first and flagged.
//! - **Duplicates.** Within a running game items move between characters
//!   before every save is in, so an ID in the latest saves of two
//!   characters is only *contested* then; when the game closes, an ID
//!   still in two characters' last saves is a duplicate
//!   ([`Registry::close_game`]).
//! - **History.** Every place an ID has been (game, character, stash,
//!   escrow, delivery, in flight, gone) is one row ([`Registry::history`])
//!   for the audit tools.
//!
//! The registry keeps its tables in the mod realm's own database
//! (`data/realms/<mod>/realm.sqlite`) beside the economy's, and every
//! call takes the caller's connection, so a registry change commits in
//! the same SQLite transaction as the record or stash change it goes
//! with. [`write`] runs a standalone change in an immediate transaction.
//! The hooks of the save checks (`d2-save-checks`) and the economy
//! (`d2-economy`) are [`Registry::other_holder`],
//! [`Registry::arriving`] and [`Registry::moved`]. No network code, no
//! platform code.

mod blocks;
mod db;
mod economy;
mod error;
mod ids;
mod saves;
#[cfg(test)]
mod tests;

use std::fmt;

use rusqlite::Connection;

pub use db::{open_file, write};
pub use error::{Error, Refusal};
pub use ids::{save_ids, SaveIds};
pub use saves::{Adopted, Dupe, DupeHow, SaveItems, SaveVerdict};

/// The ID count of one block (plan C3: 4096 at a time).
pub const BLOCK_SIZE: u32 = 4096;

/// A block of realm item IDs granted to one game: `first .. first + count`
/// (the link's and the journal's `ItemIdBlock`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    pub first: u32,
    pub count: u32,
}

impl Block {
    /// The block holds `id`.
    pub fn contains(&self, id: u32) -> bool {
        id >= self.first && u64::from(id) < u64::from(self.first) + u64::from(self.count)
    }
}

/// The split of a mod's economy an item belongs to (contracts 11.13): an
/// item never moves between splits. The mod itself is the database.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Split {
    pub hardcore: bool,
    /// The ladder season, 0 for none.
    pub ladder: u16,
    pub expansion: bool,
}

impl Split {
    /// Softcore, non-ladder, expansion.
    pub const DEFAULT: Split = Split {
        hardcore: false,
        ladder: 0,
        expansion: true,
    };

    /// The split as stored, for example `sc-l0-exp` or `hc-l3-cls` (the
    /// same text as `d2_economy::EconomyKey::split`).
    pub fn key(&self) -> String {
        format!(
            "{}-l{}-{}",
            if self.hardcore { "hc" } else { "sc" },
            self.ladder,
            if self.expansion { "exp" } else { "cls" }
        )
    }
}

/// Where an ID is (contracts 11.13, the registry's `place`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Place {
    /// On a character (`place_id`: its name key).
    Character,
    /// In a shared stash.
    Stash,
    /// In a listing's or an offer's escrow.
    Escrow,
    /// In a Deliveries tab.
    Delivery,
    /// In flight to a character (`place_id`: its name key), waiting for
    /// the save that holds it.
    InFlight,
    /// Left a character in a game (dropped, sold, used, traded on before
    /// a save) or destroyed (`place_id`: the character it left).
    Gone,
}

impl Place {
    /// The name stored in the tables.
    pub fn as_str(self) -> &'static str {
        match self {
            Place::Character => "character",
            Place::Stash => "stash",
            Place::Escrow => "escrow",
            Place::Delivery => "delivery",
            Place::InFlight => "in_flight",
            Place::Gone => "gone",
        }
    }

    /// The place of a stored name.
    pub fn parse(s: &str) -> Option<Place> {
        Some(match s {
            "character" => Place::Character,
            "stash" => Place::Stash,
            "escrow" => Place::Escrow,
            "delivery" => Place::Delivery,
            "in_flight" => Place::InFlight,
            "gone" => Place::Gone,
            _ => return None,
        })
    }
}

impl fmt::Display for Place {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where an ID is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Where {
    pub place: Place,
    /// The character's name key, the stash owner, the listing, ... as the
    /// caller named it.
    pub place_id: String,
    /// The running game it is in, if any.
    pub game: Option<u32>,
    /// Its [`Split::key`].
    pub split: String,
}

impl fmt::Display for Where {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.place, self.place_id)?;
        if let Some(g) = self.game {
            write!(f, " (game {g})")?;
        }
        Ok(())
    }
}

/// One step of an ID's history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// Unix milliseconds.
    pub at: i64,
    pub place: Place,
    pub place_id: String,
    pub game: Option<u32>,
    /// Why it moved (`new`, `first-sight`, `trade`, `left`, `stash`, ...).
    pub why: String,
}

/// What a save may carry that is not from the game's blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownIds {
    /// An ID that is not on the character, not from the game and not
    /// granted to it is refused (plan C3).
    Refuse,
    /// Such an ID is registered on the character and the save flagged:
    /// for games whose servers don't place granted IDs into new items yet.
    Adopt,
}

/// The registry's settings (one per mod realm).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// IDs per granted block.
    pub block_size: u32,
    /// The first ID the counter grants (0 is "no ID").
    pub first_id: u32,
    pub unknown: UnknownIds,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            block_size: BLOCK_SIZE,
            first_id: 1,
            unknown: UnknownIds::Refuse,
        }
    }
}

/// The item registry of one mod realm. It holds no connection: every call
/// takes the realm database's connection (or the caller's transaction on
/// it).
#[derive(Clone, Debug)]
pub struct Registry {
    mod_id: String,
    config: Config,
}

impl Registry {
    /// Creates the registry's tables in `conn` if needed and checks that
    /// the database belongs to `mod_id` (a database made for another mod
    /// is refused: [`Error::OtherMod`]).
    pub fn open(conn: &Connection, mod_id: &str, config: Config) -> Result<Registry, Error> {
        if config.block_size == 0 || config.first_id == 0 {
            return Err(Error::Config("block_size and first_id must be above 0"));
        }
        db::setup(conn, mod_id, config.first_id)?;
        Ok(Registry {
            mod_id: mod_id.to_owned(),
            config,
        })
    }

    /// The mod realm.
    pub fn mod_id(&self) -> &str {
        &self.mod_id
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Where `id` is, if registered.
    pub fn place_of(&self, conn: &Connection, id: u32) -> Result<Option<Where>, Error> {
        db::row(conn, id)
    }

    /// Every place `id` has been, oldest first.
    pub fn history(&self, conn: &Connection, id: u32) -> Result<Vec<Step>, Error> {
        db::history(conn, id)
    }
}
