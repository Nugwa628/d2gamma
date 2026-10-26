//! Judging the item IDs of a returned save, first sight of a record, and
//! the duplicates found when a game closes.

use std::collections::{BTreeMap, HashSet};

use rusqlite::{params, Connection};

use crate::db::{insert, relocate, row};
use crate::{Error, Place, Refusal, Registry, Split, UnknownIds, Where};

/// The item IDs of one character save (or of a character record on first
/// sight).
#[derive(Clone, Copy, Debug)]
pub struct SaveItems<'a> {
    /// The game the save comes from.
    pub game: u32,
    /// The character's name key.
    pub character: &'a str,
    pub split: Split,
    /// Every item ID in the record ([`crate::save_ids`]); 0 is skipped.
    pub ids: &'a [u32],
    /// Unix milliseconds.
    pub now: i64,
}

/// The answer for one save. When [`SaveVerdict::accepted`] is false,
/// nothing was written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveVerdict {
    /// IDs refused, with why: the save is refused (`SaveInvalid`).
    pub refused: Vec<(u32, Refusal)>,
    /// IDs new from the game's blocks, now on the character.
    pub new: Vec<u32>,
    /// IDs neither on the character nor granted, registered under
    /// [`UnknownIds::Adopt`]: an audit of the game is due.
    pub adopted: Vec<u32>,
    /// IDs that came from another character of the same game (a trade or
    /// a pick-up), with that character.
    pub moved: Vec<(u32, String)>,
    /// IDs that were on the character and are not in the save: they left
    /// it in this game.
    pub left: Vec<u32>,
    /// IDs also in the latest save of another character of the game, with
    /// that character: fine while items change hands, a duplicate if still
    /// so when the game closes.
    pub contested: Vec<(u32, String)>,
}

impl SaveVerdict {
    /// The save may be stored.
    pub fn accepted(&self) -> bool {
        self.refused.is_empty()
    }

    /// An audit of the game is due (IDs adopted).
    pub fn flagged(&self) -> bool {
        !self.adopted.is_empty()
    }
}

/// What first sight of a record did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adopted {
    /// IDs registered on the character now.
    pub registered: Vec<u32>,
    /// IDs that were on the character already.
    pub known: Vec<u32>,
    /// IDs held elsewhere: kept there, flagged ([`Dupe`] rows written).
    pub dupes: Vec<(u32, Where)>,
}

/// How a duplicate was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DupeHow {
    /// A record seen first had an ID registered elsewhere already.
    FirstSight,
    /// When its game closed, the ID was in the last saves of two
    /// characters of that game.
    InGame,
}

impl DupeHow {
    fn as_str(self) -> &'static str {
        match self {
            DupeHow::FirstSight => "first-sight",
            DupeHow::InGame => "in-game",
        }
    }

    fn parse(s: &str) -> DupeHow {
        match s {
            "first-sight" => DupeHow::FirstSight,
            _ => DupeHow::InGame,
        }
    }
}

/// A duplicated ID: where the registry keeps it, and who else has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dupe {
    pub id: u32,
    pub game: Option<u32>,
    /// The holder the registry keeps it on (a character's name key, or the
    /// place it is in).
    pub kept: String,
    /// The other characters with the same ID.
    pub also: Vec<String>,
    pub how: DupeHow,
    /// Unix milliseconds.
    pub at: i64,
}

/// What one ID of a save is.
enum Judged {
    /// On the character already.
    Kept,
    /// Was on, or left, a character of the same game.
    FromGame(String),
    /// In flight to the character (a take from the stash or a delivery).
    Arrived,
    /// Unregistered, from a block of the game.
    New,
    /// Unregistered and not granted; registered under `UnknownIds::Adopt`.
    Adopt,
    Refused(Refusal),
}

impl Registry {
    fn judge(&self, conn: &Connection, s: &SaveItems, id: u32) -> Result<Judged, Error> {
        let Some(at) = row(conn, id)? else {
            return Ok(if Self::granted(conn, s.game, id)? {
                Judged::New
            } else if self.config.unknown == UnknownIds::Adopt {
                Judged::Adopt
            } else {
                Judged::Refused(Refusal::NotGranted)
            });
        };
        if at.split != s.split.key() {
            return Ok(Judged::Refused(Refusal::EconomyMismatch {
                split: at.split,
            }));
        }
        let mine = at.place_id == s.character;
        Ok(match at.place {
            Place::Character if mine => Judged::Kept,
            Place::Character | Place::Gone if at.game == Some(s.game) => {
                Judged::FromGame(at.place_id)
            }
            Place::InFlight if mine => Judged::Arrived,
            _ => Judged::Refused(Refusal::HeldElsewhere(at)),
        })
    }

    /// [`Registry::judge`] for one ID: `Ok(None)` when it is registered
    /// and may be on the character, `Ok(Some(kind))` when it must be
    /// registered as new (`realm`) or adopted (`adopted`), `Err` refused.
    pub(crate) fn judge_one(
        &self,
        conn: &Connection,
        s: &SaveItems,
        id: u32,
    ) -> Result<Result<Option<&'static str>, Refusal>, Error> {
        Ok(match self.judge(conn, s, id)? {
            Judged::Kept | Judged::FromGame(_) | Judged::Arrived => Ok(None),
            Judged::New => Ok(Some("realm")),
            Judged::Adopt => Ok(Some("adopted")),
            Judged::Refused(why) => Err(why),
        })
    }

    /// The holder that would make `id` refused in a save of `character`
    /// from `game`, if any: the hook of the save checks' duplicate check
    /// (`d2_save_checks::ItemRegistry::other_holder`, with the save's game
    /// bound by the caller). An ID that is merely unknown is not held by
    /// anyone: `None`.
    pub fn other_holder(
        &self,
        conn: &Connection,
        game: u32,
        character: &str,
        split: Split,
        id: u32,
    ) -> Result<Option<Where>, Error> {
        let s = SaveItems {
            game,
            character,
            split,
            ids: &[],
            now: 0,
        };
        Ok(match self.judge(conn, &s, id)? {
            Judged::Refused(Refusal::HeldElsewhere(w)) => Some(w),
            _ => None,
        })
    }

    /// Judges every ID of a save from game `s.game` and, when none is
    /// refused, records it: IDs on the character, the game set on them,
    /// IDs no longer in the save marked as having left the character in
    /// this game, and the character's claims in the game replaced. Run it
    /// in the transaction that stores the record.
    pub fn accept_save(&self, conn: &Connection, s: &SaveItems) -> Result<SaveVerdict, Error> {
        let mut v = SaveVerdict::default();
        let mut seen = HashSet::new();
        let mut judged = Vec::with_capacity(s.ids.len());
        for &id in s.ids.iter().filter(|&&id| id != 0) {
            if !seen.insert(id) {
                v.refused.push((id, Refusal::Twice));
                continue;
            }
            match self.judge(conn, s, id)? {
                Judged::Refused(why) => v.refused.push((id, why)),
                j => judged.push((id, j)),
            }
        }
        if !v.accepted() {
            return Ok(v);
        }
        let here = Where {
            place: Place::Character,
            place_id: s.character.to_owned(),
            game: Some(s.game),
            split: s.split.key(),
        };
        for (id, j) in judged {
            match j {
                Judged::Kept => {
                    conn.execute(
                        "UPDATE items SET game = ?2 WHERE id = ?1",
                        params![id, s.game],
                    )?;
                }
                Judged::FromGame(from) => {
                    // From another character: a trade or a pick-up; its own
                    // dropped item: picked up again.
                    let why = if from == s.character { "back" } else { "trade" };
                    relocate(
                        conn,
                        id,
                        Place::Character,
                        s.character,
                        here.game,
                        why,
                        s.now,
                    )?;
                    if from != s.character {
                        v.moved.push((id, from));
                    }
                }
                Judged::Arrived => {
                    relocate(
                        conn,
                        id,
                        Place::Character,
                        s.character,
                        here.game,
                        "arrived",
                        s.now,
                    )?;
                }
                Judged::New => {
                    insert(conn, id, "realm", &here, "new", s.now)?;
                    v.new.push(id);
                }
                Judged::Adopt => {
                    insert(conn, id, "adopted", &here, "adopted", s.now)?;
                    v.adopted.push(id);
                }
                Judged::Refused(_) => unreachable!("refusals are not judged on"),
            }
        }
        // Items of the character missing from the save left it in this game.
        let on: Vec<u32> = {
            let mut q =
                conn.prepare("SELECT id FROM items WHERE place = 'character' AND place_id = ?1")?;
            let rows = q.query_map([s.character], |r| r.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        for id in on.into_iter().filter(|id| !seen.contains(id)) {
            relocate(conn, id, Place::Gone, s.character, here.game, "left", s.now)?;
            v.left.push(id);
        }
        v.left.sort_unstable();
        // The character's claims in the game are this save's IDs.
        conn.execute(
            "DELETE FROM item_claims WHERE game = ?1 AND character = ?2",
            params![s.game, s.character],
        )?;
        {
            let mut ins = conn.prepare(
                "INSERT OR IGNORE INTO item_claims(game, id, character) VALUES (?1, ?2, ?3)",
            )?;
            let mut other = conn.prepare(
                "SELECT character FROM item_claims WHERE game = ?1 AND id = ?2 AND character != ?3
                 ORDER BY character",
            )?;
            let mut ids: Vec<u32> = seen.into_iter().collect();
            ids.sort_unstable();
            for id in ids {
                ins.execute(params![s.game, id, s.character])?;
                let rows = other.query_map(params![s.game, id, s.character], |r| r.get(0))?;
                for c in rows {
                    v.contested.push((id, c?));
                }
            }
        }
        Ok(v)
    }

    /// First sight of a character's record (a v1 character, the N25
    /// migration, or a stored record before its first v2 game): every ID
    /// not registered yet is registered on the character as it is saved;
    /// an ID registered elsewhere stays there and is flagged as a
    /// duplicate. `s.game` is not used.
    pub fn adopt(&self, conn: &Connection, s: &SaveItems) -> Result<Adopted, Error> {
        let mut a = Adopted::default();
        let here = Where {
            place: Place::Character,
            place_id: s.character.to_owned(),
            game: None,
            split: s.split.key(),
        };
        let mut seen = HashSet::new();
        for &id in s.ids.iter().filter(|&&id| id != 0) {
            if !seen.insert(id) {
                continue;
            }
            match row(conn, id)? {
                None => {
                    insert(conn, id, "first-sight", &here, "first-sight", s.now)?;
                    if Self::in_any_block(conn, id)? {
                        // An old item's ID inside a granted block: a game may
                        // make an item with it too, so the owner sees it.
                        dupe(
                            conn,
                            id,
                            None,
                            s.character,
                            "(granted block)",
                            DupeHow::FirstSight,
                            s.now,
                        )?;
                    }
                    a.registered.push(id);
                }
                Some(w) if w.place == Place::Character && w.place_id == s.character => {
                    a.known.push(id)
                }
                Some(w) => {
                    dupe(
                        conn,
                        id,
                        None,
                        &w.place_id,
                        s.character,
                        DupeHow::FirstSight,
                        s.now,
                    )?;
                    a.dupes.push((id, w));
                }
            }
        }
        Ok(a)
    }

    /// The character's last save in `game` is in (it left the game): its
    /// items are no longer in that game, so another character of the game
    /// can't take them on. Its claims stay for the duplicate check of
    /// [`Registry::close_game`].
    pub fn left(&self, conn: &Connection, game: u32, character: &str) -> Result<(), Error> {
        conn.execute(
            "UPDATE items SET game = NULL
             WHERE game = ?1 AND place = 'character' AND place_id = ?2",
            params![game, character],
        )?;
        Ok(())
    }

    /// The game closed: every ID still in the last saves of two of its
    /// characters is a duplicate (kept where the registry has it, the
    /// others flagged), the game's claims are dropped, its items leave
    /// the game and its blocks close (their unused IDs are never granted
    /// again).
    pub fn close_game(&self, conn: &Connection, game: u32, now: i64) -> Result<Vec<Dupe>, Error> {
        let mut claims: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        {
            let mut q = conn.prepare(
                "SELECT id, character FROM item_claims WHERE game = ?1 AND id IN
                 (SELECT id FROM item_claims WHERE game = ?1 GROUP BY id HAVING COUNT(*) > 1)
                 ORDER BY id, character",
            )?;
            let rows = q.query_map([game], |r| {
                Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?))
            })?;
            for r in rows {
                let (id, c) = r?;
                claims.entry(id).or_default().push(c);
            }
        }
        let mut out = Vec::new();
        for (id, holders) in claims {
            let kept = match row(conn, id)? {
                Some(w) if holders.contains(&w.place_id) => w.place_id,
                Some(w) => format!("{} {}", w.place, w.place_id),
                None => holders[0].clone(),
            };
            let also: Vec<String> = holders.into_iter().filter(|h| *h != kept).collect();
            for a in &also {
                dupe(conn, id, Some(game), &kept, a, DupeHow::InGame, now)?;
            }
            out.push(Dupe {
                id,
                game: Some(game),
                kept,
                also,
                how: DupeHow::InGame,
                at: now,
            });
        }
        conn.execute("DELETE FROM item_claims WHERE game = ?1", [game])?;
        conn.execute("UPDATE items SET game = NULL WHERE game = ?1", [game])?;
        conn.execute("UPDATE item_blocks SET open = 0 WHERE game = ?1", [game])?;
        Ok(out)
    }

    /// Every duplicate found, oldest first, one row per extra holder.
    pub fn dupes(&self, conn: &Connection) -> Result<Vec<Dupe>, Error> {
        let mut q =
            conn.prepare("SELECT id, game, kept, also, how, at FROM item_dupes ORDER BY seq")?;
        let rows = q.query_map([], |r| {
            Ok(Dupe {
                id: r.get(0)?,
                game: r.get(1)?,
                kept: r.get(2)?,
                also: vec![r.get(3)?],
                how: DupeHow::parse(&r.get::<_, String>(4)?),
                at: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

fn dupe(
    conn: &Connection,
    id: u32,
    game: Option<u32>,
    kept: &str,
    also: &str,
    how: DupeHow,
    now: i64,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO item_dupes(id, at, game, kept, also, how) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, now, game, kept, also, how.as_str()],
    )?;
    Ok(())
}
