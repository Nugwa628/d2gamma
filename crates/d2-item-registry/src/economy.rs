//! The economy's hooks (`d2_economy::ItemRegistry`): an item leaving a
//! character for the shared stash, and every later move of it. Both run in
//! the economy's transaction.

use rusqlite::{params, Connection};

use crate::db::{insert, relocate};
use crate::saves::SaveItems;
use crate::{Error, Place, Registry, Split, Where};

impl Registry {
    /// An item with ID `id` leaves `character`, seated in `game`, for the
    /// economy (a stash put, `ItemOut`). It must be one the character's
    /// next save could carry: on the character, from another character of
    /// the game, in flight to it, or new from the game's blocks (an item
    /// not yet saved). A new one is registered on the character first.
    /// Refused: [`Error::Refused`] (`TransferInvalid`, or
    /// `EconomyMismatch`).
    pub fn arriving(
        &self,
        conn: &Connection,
        split: Split,
        id: u32,
        character: &str,
        game: u32,
        now: i64,
    ) -> Result<(), Error> {
        let s = SaveItems {
            game,
            character,
            split,
            ids: &[],
            now,
        };
        let here = Where {
            place: Place::Character,
            place_id: character.to_owned(),
            game: Some(game),
            split: split.key(),
        };
        match self.judge_one(conn, &s, id)? {
            Ok(None) => Ok(()),
            Ok(Some(kind)) => insert(conn, id, kind, &here, kind_why(kind), now),
            Err(why) => Err(Error::Refused { id, why }),
        }
    }

    /// `id` is now at `place` (`place_id`: the character's name key, the
    /// stash owner, the listing, ...; `game`: the game of a character or
    /// an item in flight to one). Leaving the characters, it leaves every
    /// game's claims. [`Error::Unregistered`] for an ID never registered.
    #[allow(clippy::too_many_arguments)]
    pub fn moved(
        &self,
        conn: &Connection,
        id: u32,
        place: Place,
        place_id: &str,
        game: Option<u32>,
        why: &str,
        now: i64,
    ) -> Result<(), Error> {
        if !relocate(conn, id, place, place_id, game, why, now)? {
            return Err(Error::Unregistered(id));
        }
        if place != Place::Character {
            conn.execute("DELETE FROM item_claims WHERE id = ?1", params![id])?;
        }
        Ok(())
    }
}

fn kind_why(kind: &str) -> &'static str {
    match kind {
        "realm" => "new",
        _ => "adopted",
    }
}
