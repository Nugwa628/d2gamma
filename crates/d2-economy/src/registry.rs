//! The two seams to the rest of the main server's realm database: the item
//! registry (N19: where each realm item ID is, across characters too) and
//! the character records (N25). Both run inside the economy's transaction,
//! on the same connection, so a move and the registry or record change it
//! implies commit together or not at all.

use rusqlite::Connection;

use crate::db::Act;
use crate::error::Refusal;
use crate::key::{CharRef, EconomyKey, ItemId};

/// Where an item with a realm ID is (contracts 11.13, the registry's
/// `place`). The registry's `gone` is its own: the economy never moves an
/// item there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// On a character: it left the economy into a game.
    Character,
    Stash,
    /// In flight to a character, waiting for the save that holds it.
    InFlight,
    /// In a listing's or an offer's escrow.
    Escrow,
    /// In an account's Deliveries.
    Delivery,
}

impl Place {
    /// The registry's name for the place.
    pub fn as_str(self) -> &'static str {
        match self {
            Place::Character => "character",
            Place::Stash => "stash",
            Place::InFlight => "in_flight",
            Place::Escrow => "escrow",
            Place::Delivery => "delivery",
        }
    }
}

/// One move of an item with an ID, as the registry hears it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moved<'a> {
    pub id: ItemId,
    /// Where the item is now.
    pub place: Place,
    /// The account, for every place but [`Place::Character`].
    pub owner: Option<i64>,
    /// The character it went to, for [`Place::Character`] and
    /// [`Place::InFlight`].
    pub character: Option<&'a str>,
    /// That character's game.
    pub game: Option<u32>,
    /// The transfer kind, as in the log: `put`, `take`, `take_done`,
    /// `take_undo`, `list`, `sold`, `bought`, `claim`, ...
    pub why: &'a str,
}

/// The realm's item registry. The economy calls it on every move of an
/// item with an ID; a refusal undoes the move.
pub trait ItemRegistry {
    /// An item arrives from a character (a put). Refuse an ID that is
    /// registered elsewhere (a duplicate), or that wasn't granted to that
    /// game: [`Refusal::TransferInvalid`].
    fn arriving(
        &self,
        conn: &Connection,
        economy: &EconomyKey,
        id: ItemId,
        from: &CharRef,
    ) -> Result<(), Refusal> {
        let _ = (conn, economy, id, from);
        Ok(())
    }

    /// The item moved; see [`Moved`].
    fn moved(&self, conn: &Connection, moved: &Moved<'_>) -> Result<(), Refusal> {
        let _ = (conn, moved);
        Ok(())
    }

    /// May the item be listed or offered now? An item held for an audit
    /// (open question 25: items from a player-hosted game until its audit)
    /// is refused with [`Refusal::ItemHeld`].
    fn listable(&self, conn: &Connection, id: ItemId, now_ms: i64) -> Result<(), Refusal> {
        let _ = (conn, id, now_ms);
        Ok(())
    }
}

/// No registry yet: every item is accepted (before N19).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRegistry;

impl ItemRegistry for NoRegistry {}

/// The character records the moves change: a put stores the character's
/// record without the item, a finished take the record with it, in the
/// move's transaction (`Save.reason` 5, the immediate save).
pub trait CharacterRecords {
    fn store(
        &self,
        conn: &Connection,
        character: &CharRef,
        record: &[u8],
        seq: u32,
    ) -> Result<(), Refusal>;
}

/// Records are stored elsewhere (before N25).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRecords;

impl CharacterRecords for NoRecords {
    fn store(&self, _: &Connection, _: &CharRef, _: &[u8], _: u32) -> Result<(), Refusal> {
        Ok(())
    }
}

impl Act<'_> {
    /// Tells the registry an item moved.
    pub fn moved(
        &self,
        id: ItemId,
        place: Place,
        owner: Option<i64>,
        to: Option<&CharRef>,
        why: &str,
    ) -> Result<(), Refusal> {
        self.registry.moved(
            self.conn,
            &Moved {
                id,
                place,
                owner,
                character: to.map(|c| c.name.as_str()),
                game: to.map(|c| c.game),
                why,
            },
        )
    }
}
