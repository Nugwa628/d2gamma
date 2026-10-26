//! The shared stash (plan E1, modules N28a-c): per account and economy, an
//! items tab, a stackables tab of counts and a gold balance, across all the
//! account's characters.
//!
//! - **Put** (character → stash): the game server took the goods off the
//!   character and sent them with the character's record without them
//!   (`ItemOut`). [`Economy::put`] adds them to the stash, stores that
//!   record and logs the move in one transaction; a refusal leaves the
//!   stash untouched and the game server gives the goods back.
//! - **Take** (stash → character) is two steps, because the game server
//!   must place the goods and save: [`Economy::take`] moves them from the
//!   stash to a transfer **in flight** to the character (`ItemIn`);
//!   [`Economy::take_done`] closes it with the save that holds them, or
//!   puts them back when there was no room. If the game ends first,
//!   [`Economy::undo_lease`] puts back every transfer of that seat lease.
//! - Undoing a take never fails on the stash's limits: the goods were in
//!   the stash a moment ago.

pub(crate) mod amounts;
pub mod page;

use rusqlite::{params, OptionalExtension};

use crate::db::{Act, Economy};
use crate::error::{Done, Error, Refusal};
use crate::key::{CharRef, ItemId, Owner, Req};
use crate::log::Transfer;
use crate::registry::Place;

/// Migration 1: requests, the transfer log, the stash and transfers in
/// flight.
pub(crate) const SCHEMA: &str = "
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE requests(
    scope TEXT NOT NULL,
    req INTEGER NOT NULL,
    code INTEGER NOT NULL,
    id INTEGER,
    at INTEGER NOT NULL,
    PRIMARY KEY(scope, req)
) WITHOUT ROWID;
CREATE INDEX requests_at ON requests(at);
CREATE TABLE transfers(
    id INTEGER PRIMARY KEY,
    at INTEGER NOT NULL,
    kind TEXT NOT NULL,
    economy TEXT NOT NULL,
    item_id INTEGER,
    code TEXT,
    count INTEGER,
    gold INTEGER,
    from_kind TEXT NOT NULL,
    from_id INTEGER,
    to_kind TEXT NOT NULL,
    to_id INTEGER,
    character TEXT,
    game INTEGER,
    lease INTEGER,
    listing INTEGER,
    offer INTEGER,
    req INTEGER NOT NULL
);
CREATE INDEX transfers_item ON transfers(item_id) WHERE item_id IS NOT NULL;
CREATE TABLE econ_items(
    item_id INTEGER PRIMARY KEY,
    economy TEXT NOT NULL,
    account INTEGER NOT NULL,
    place TEXT NOT NULL,
    place_id INTEGER,
    added INTEGER NOT NULL,
    record BLOB NOT NULL,
    code TEXT NOT NULL,
    item_type TEXT NOT NULL,
    quality INTEGER NOT NULL,
    name TEXT NOT NULL,
    names TEXT NOT NULL,
    level_req INTEGER NOT NULL,
    ilvl INTEGER NOT NULL,
    sockets INTEGER NOT NULL,
    ethereal INTEGER NOT NULL,
    class INTEGER
);
CREATE INDEX econ_items_owner ON econ_items(account, economy, place);
CREATE TABLE item_stats(
    item_id INTEGER NOT NULL REFERENCES econ_items(item_id) ON DELETE CASCADE,
    stat INTEGER NOT NULL,
    value INTEGER NOT NULL,
    PRIMARY KEY(item_id, stat)
) WITHOUT ROWID;
CREATE TABLE stash_stacks(
    account INTEGER NOT NULL,
    economy TEXT NOT NULL,
    code TEXT NOT NULL,
    count INTEGER NOT NULL CHECK(count > 0),
    PRIMARY KEY(account, economy, code)
) WITHOUT ROWID;
CREATE TABLE stash_gold(
    account INTEGER NOT NULL,
    economy TEXT NOT NULL,
    gold INTEGER NOT NULL CHECK(gold >= 0),
    PRIMARY KEY(account, economy)
) WITHOUT ROWID;
CREATE TABLE in_flight(
    id INTEGER PRIMARY KEY,
    account INTEGER NOT NULL,
    economy TEXT NOT NULL,
    character TEXT NOT NULL,
    game INTEGER NOT NULL,
    lease INTEGER NOT NULL,
    from_kind TEXT NOT NULL,
    from_id INTEGER,
    item_id INTEGER,
    code TEXT,
    count INTEGER,
    gold INTEGER,
    at INTEGER NOT NULL
);
CREATE INDEX in_flight_lease ON in_flight(lease);
CREATE VIEW stash_items AS
    SELECT account, economy, item_id, record, added FROM econ_items WHERE place = 'stash';
";

/// What the main server knows about an item from its bytes and the
/// realm's game data: everything the stash list sorts and filters by, and
/// the market searches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemFacts {
    pub code: String,
    /// The item type (`weap`, `armo`, `ring`, ...).
    pub item_type: String,
    /// The quality (normal, magic, rare, set, unique, ...) as the save
    /// format numbers it.
    pub quality: u8,
    /// The name shown in the list.
    pub name: String,
    /// Every name the item is searched by: base, unique, set or runeword
    /// name, affix names.
    pub name_parts: Vec<String>,
    pub level_req: u8,
    pub ilvl: u8,
    pub sockets: u8,
    pub ethereal: bool,
    /// The class an item is restricted to, if any.
    pub class: Option<u8>,
    /// `(stat ID, value)`, one per stat.
    pub stats: Vec<(u16, i32)>,
    /// A runeword: the market groups it as one, whatever its quality.
    pub runeword: bool,
    pub identified: bool,
    /// The item type and its ancestors in the game data's type tree
    /// (`helm`, `armo`, ...), for the market's type filter. Empty: the
    /// item type alone.
    pub types: Vec<String>,
}

impl Default for ItemFacts {
    /// No facts; an item is identified unless said otherwise.
    fn default() -> Self {
        ItemFacts {
            code: String::new(),
            item_type: String::new(),
            quality: 0,
            name: String::new(),
            name_parts: Vec::new(),
            level_req: 0,
            ilvl: 0,
            sockets: 0,
            ethereal: false,
            class: None,
            stats: Vec::new(),
            runeword: false,
            identified: true,
            types: Vec::new(),
        }
    }
}

impl ItemFacts {
    /// The item's types for the type filter: [`ItemFacts::types`], or the
    /// item type when none are given.
    pub fn type_codes(&self) -> Vec<String> {
        if self.types.is_empty() {
            vec![self.item_type.clone()]
        } else {
            self.types.clone()
        }
    }
}

/// An item with a realm ID: its bytes (the save format's item) and facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: ItemId,
    pub record: Vec<u8>,
    pub facts: ItemFacts,
}

/// Goods arriving from a character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PutGoods {
    Item(Item),
    Stack { code: String, count: u32 },
    Gold(u64),
}

/// Goods named by a request: an item by its ID, a count of a stackable, or
/// gold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Goods {
    Item(ItemId),
    Stack { code: String, count: u32 },
    Gold(u64),
}

impl PutGoods {
    fn goods(&self) -> Goods {
        match self {
            PutGoods::Item(i) => Goods::Item(i.id),
            PutGoods::Stack { code, count } => Goods::Stack {
                code: code.clone(),
                count: *count,
            },
            PutGoods::Gold(g) => Goods::Gold(*g),
        }
    }
}

/// A character's record as a game server saved it, with its save number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Save {
    pub record: Vec<u8>,
    pub seq: u32,
}

/// How a game server answered an `ItemIn` (`ItemInDone.result`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placed {
    Placed,
    NoRoom,
    Refused,
}

/// A transfer in flight to a character: what the main server sends in
/// `ItemIn`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InFlight {
    pub id: i64,
    pub account: i64,
    pub character: CharRef,
    pub goods: Goods,
    /// The item's bytes and facts, for an item.
    pub item: Option<Item>,
}

impl Economy {
    /// Test servers only (the main server's `econ-seed` line with
    /// `dev = test`): adds gold or a stackable count to an account's stash
    /// out of nothing, within the stash's limits, logged as `seed`.
    pub fn seed(
        &mut self,
        owner: &Owner,
        req: &Req,
        goods: &Goods,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_key(&owner.economy)?;
            let split = owner.economy.split();
            match goods {
                Goods::Item(_) | Goods::Gold(0) => return Err(Refusal::BadRequest.into()),
                Goods::Stack { code, count } => a.check_stackable(code, *count)?,
                Goods::Gold(_) => {}
            }
            a.give_to_stash(owner.account.0, &split, goods, true, "seed")?;
            a.log(Transfer {
                kind: "seed",
                economy: &split,
                goods,
                from: ("seed", None),
                to: ("stash", Some(owner.account.0)),
                character: None,
                game: None,
                lease: None,
                listing: None,
                offer: None,
            })?;
            Ok(None)
        })
    }

    /// Character → stash: stores `goods` the game server took off `from`,
    /// with `save`, the character's record without them, in one
    /// transaction.
    pub fn put(
        &mut self,
        owner: &Owner,
        req: &Req,
        from: &CharRef,
        goods: PutGoods,
        save: &Save,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_key(&owner.economy)?;
            let split = owner.economy.split();
            let account = owner.account.0;
            match &goods {
                PutGoods::Item(item) => {
                    a.registry.arriving(a.conn, &owner.economy, item.id, from)?;
                    if a.stash_count(account, &split)? >= i64::from(a.config.stash_items) {
                        return Err(Refusal::StashFull.into());
                    }
                    a.insert_item(item, account, &split, Place::Stash, None)?;
                    a.moved(item.id, Place::Stash, Some(account), None, "put")?;
                }
                PutGoods::Stack { code, count } => {
                    a.check_stackable(code, *count)?;
                    a.add_stack(account, &split, code, *count, true)?;
                }
                PutGoods::Gold(gold) => {
                    if *gold == 0 {
                        return Err(Refusal::BadRequest.into());
                    }
                    a.add_gold(account, &split, *gold, true)?;
                }
            }
            a.records.store(a.conn, from, &save.record, save.seq)?;
            a.step()?;
            a.log(Transfer {
                kind: "put",
                economy: &split,
                goods: &goods.goods(),
                from: ("character", None),
                to: ("stash", Some(account)),
                character: Some(&from.name),
                game: Some(from.game),
                lease: Some(from.lease),
                listing: None,
                offer: None,
            })?;
            Ok(None)
        })
    }

    /// Character → stash: one thing the game server took off `from` that
    /// becomes several stackable counts (a mod game's container folded
    /// into what it holds), stored with `save` in one transaction: every
    /// `(code, count)` is credited, or none. A code given twice counts
    /// once with the sum. A slot that would pass `cap` refuses the whole
    /// put ([`Refusal::StashFull`]); an empty list or a code that is no
    /// stackable is [`Refusal::BadRequest`].
    #[allow(clippy::too_many_arguments)]
    pub fn put_stacks(
        &mut self,
        owner: &Owner,
        req: &Req,
        from: &CharRef,
        credits: &[(String, u32)],
        cap: u32,
        save: &Save,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_key(&owner.economy)?;
            if credits.is_empty() {
                return Err(Refusal::BadRequest.into());
            }
            let split = owner.economy.split();
            let account = owner.account.0;
            let mut sums: std::collections::BTreeMap<&str, u64> = Default::default();
            for (code, count) in credits {
                a.check_stackable(code, *count)?;
                *sums.entry(code.as_str()).or_default() += u64::from(*count);
            }
            for (code, n) in &sums {
                let have = a.stack_count(account, &split, code)?.max(0) as u64;
                if have + n > u64::from(cap) {
                    return Err(Refusal::StashFull.into());
                }
            }
            for (code, n) in &sums {
                let n = u32::try_from(*n).map_err(|_| Refusal::StashFull)?;
                a.add_stack(account, &split, code, n, true)?;
            }
            a.records.store(a.conn, from, &save.record, save.seq)?;
            a.step()?;
            for (code, n) in &sums {
                a.log(Transfer {
                    kind: "put",
                    economy: &split,
                    goods: &Goods::Stack {
                        code: (*code).to_owned(),
                        count: *n as u32,
                    },
                    from: ("character", None),
                    to: ("stash", Some(account)),
                    character: Some(&from.name),
                    game: Some(from.game),
                    lease: Some(from.lease),
                    listing: None,
                    offer: None,
                })?;
            }
            Ok(None)
        })
    }

    /// Stash → character, step 1: moves `what` out of the stash into a
    /// transfer in flight to `to`. The answer's `id` is the transfer;
    /// [`Economy::in_flight`] gives what to send.
    pub fn take(
        &mut self,
        owner: &Owner,
        req: &Req,
        to: &CharRef,
        what: Goods,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            a.check_key(&owner.economy)?;
            let split = owner.economy.split();
            let account = owner.account.0;
            match &what {
                Goods::Item(id) => a.check_in_stash(*id, account, &split)?,
                Goods::Stack { code, count } => {
                    a.check_stackable(code, *count)?;
                    a.remove_stack(account, &split, code, *count, Refusal::ItemNotFound)?;
                }
                Goods::Gold(gold) => {
                    if *gold == 0 {
                        return Err(Refusal::BadRequest.into());
                    }
                    a.remove_gold(account, &split, *gold, Refusal::ItemNotFound)?;
                }
            }
            let transfer = a.open_flight(account, &split, to, ("stash", None), &what)?;
            a.log(Transfer {
                kind: "take",
                economy: &split,
                goods: &what,
                from: ("stash", Some(account)),
                to: ("in_flight", Some(transfer)),
                character: Some(&to.name),
                game: Some(to.game),
                lease: Some(to.lease),
                listing: None,
                offer: None,
            })?;
            Ok(Some(transfer))
        })
    }

    /// Stash → character, step 2: the game server's answer for transfer
    /// `transfer` under `lease`. Placed: the transfer closes and `save`
    /// (the record holding the goods) is stored with it. No room or
    /// refused: the goods go back where they came from.
    pub fn take_done(
        &mut self,
        req: &Req,
        lease: u64,
        transfer: i64,
        placed: Placed,
        save: Option<&Save>,
        now: i64,
    ) -> Result<Done, Error> {
        self.act(req, now, |a| {
            let flight = a
                .flight(transfer)?
                .filter(|f| f.character.lease == lease)
                .ok_or(Refusal::TransferUnknown)?;
            if placed != Placed::Placed {
                a.undo_flight(&flight)?;
                return Ok(Some(transfer));
            }
            let save = save.ok_or(Refusal::BadRequest)?;
            a.records
                .store(a.conn, &flight.character, &save.record, save.seq)?;
            a.step()?;
            if let Goods::Item(id) = &flight.goods {
                // The registry hears of the move while the item's row is
                // still there (it reads the item's socketed items from it).
                a.moved(
                    *id,
                    Place::Character,
                    None,
                    Some(&flight.character),
                    "take_done",
                )?;
                a.exec("DELETE FROM econ_items WHERE item_id = ?1", [id.0])?;
            }
            a.close_flight(&flight, "take_done", ("character", None))?;
            Ok(Some(transfer))
        })
    }

    /// The seat lease ended (the game closed, or its game server was lost)
    /// before its transfers finished: every transfer in flight under it
    /// goes back where it came from. Returns how many.
    pub fn undo_lease(&mut self, lease: u64, now: i64) -> Result<usize, Error> {
        self.sweep(now, |a| {
            let ids: Vec<i64> = {
                let mut st = a
                    .conn
                    .prepare("SELECT id FROM in_flight WHERE lease = ?1 ORDER BY id")?;
                let rows = st.query_map([lease as i64], |r| r.get(0))?;
                rows.collect::<Result<_, _>>()?
            };
            for id in &ids {
                if let Some(flight) = a.flight(*id)? {
                    a.undo_flight(&flight)?;
                }
            }
            Ok(ids.len())
        })
    }

    /// A transfer in flight, with the bytes to send.
    pub fn in_flight(&self, transfer: i64) -> Result<Option<InFlight>, Error> {
        let Some(mut flight) = read_flight(self.conn(), transfer)? else {
            return Ok(None);
        };
        if let Goods::Item(id) = flight.goods {
            flight.item = page::load_item(self.conn(), id)?;
        }
        Ok(Some(flight))
    }
}

fn read_flight(conn: &rusqlite::Connection, id: i64) -> Result<Option<InFlight>, Error> {
    Ok(conn
        .query_row(
            "SELECT account, character, game, lease, item_id, code, count, gold \
             FROM in_flight WHERE id = ?1",
            [id],
            |r| {
                let item: Option<u32> = r.get(4)?;
                let code: Option<String> = r.get(5)?;
                let count: Option<u32> = r.get(6)?;
                let gold: Option<i64> = r.get(7)?;
                let goods = match (item, code, count, gold) {
                    (Some(i), _, _, _) => Goods::Item(ItemId(i)),
                    (None, Some(code), Some(count), _) => Goods::Stack { code, count },
                    (_, _, _, g) => Goods::Gold(g.unwrap_or(0) as u64),
                };
                Ok(InFlight {
                    id,
                    account: r.get(0)?,
                    character: CharRef {
                        name: r.get(1)?,
                        game: r.get(2)?,
                        lease: r.get::<_, i64>(3)? as u64,
                    },
                    goods,
                    item: None,
                })
            },
        )
        .optional()?)
}

impl Act<'_> {
    pub fn check_stackable(&self, code: &str, count: u32) -> Result<(), Error> {
        if count == 0 || self.config.stackable(code).is_none() {
            return Err(Refusal::BadRequest.into());
        }
        Ok(())
    }

    /// Items in an account's stash.
    pub fn stash_count(&self, account: i64, split: &str) -> Result<i64, Error> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM econ_items WHERE account = ?1 AND economy = ?2 AND place = 'stash'",
            params![account, split],
            |r| r.get(0),
        )?)
    }

    /// Adds a new item to the economy at `place`. An ID already in the
    /// economy is a duplicate: refused.
    pub fn insert_item(
        &self,
        item: &Item,
        account: i64,
        split: &str,
        place: Place,
        place_id: Option<i64>,
    ) -> Result<(), Error> {
        let there: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM econ_items WHERE item_id = ?1",
                [item.id.0],
                |r| r.get(0),
            )
            .optional()?;
        if there.is_some() {
            return Err(Refusal::TransferInvalid.into());
        }
        let f = &item.facts;
        self.exec(
            "INSERT INTO econ_items(item_id, economy, account, place, place_id, added, record, \
             code, item_type, quality, name, names, level_req, ilvl, sockets, ethereal, class, \
             runeword, identified, types) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
             ?18, ?19, ?20)",
            params![
                item.id.0,
                split,
                account,
                place.as_str(),
                place_id,
                self.now,
                item.record,
                f.code,
                f.item_type,
                f.quality,
                f.name,
                f.name_parts.join(" "),
                f.level_req,
                f.ilvl,
                f.sockets,
                f.ethereal,
                f.class,
                f.runeword,
                f.identified,
                f.types.join(" "),
            ],
        )?;
        for (stat, value) in &f.stats {
            self.exec(
                "INSERT OR REPLACE INTO item_stats(item_id, stat, value) VALUES (?1, ?2, ?3)",
                params![item.id.0, stat, value],
            )?;
        }
        Ok(())
    }

    /// Replaces the facts stored with item `id` (and its stats).
    pub fn set_facts(&self, id: ItemId, f: &ItemFacts) -> Result<(), Error> {
        self.exec(
            "UPDATE econ_items SET code = ?2, item_type = ?3, quality = ?4, name = ?5,              names = ?6, level_req = ?7, ilvl = ?8, sockets = ?9, ethereal = ?10, class = ?11,              runeword = ?12, identified = ?13, types = ?14 WHERE item_id = ?1",
            params![
                id.0,
                f.code,
                f.item_type,
                f.quality,
                f.name,
                f.name_parts.join(" "),
                f.level_req,
                f.ilvl,
                f.sockets,
                f.ethereal,
                f.class,
                f.runeword,
                f.identified,
                f.types.join(" "),
            ],
        )?;
        self.exec("DELETE FROM item_stats WHERE item_id = ?1", [id.0])?;
        for (stat, value) in &f.stats {
            self.exec(
                "INSERT OR REPLACE INTO item_stats(item_id, stat, value) VALUES (?1, ?2, ?3)",
                params![id.0, stat, value],
            )?;
        }
        Ok(())
    }

    /// The item is in this account's stash of this split; refused if not
    /// (another split of the same account: [`Refusal::EconomyMismatch`]).
    pub fn check_in_stash(&self, id: ItemId, account: i64, split: &str) -> Result<(), Error> {
        let row: Option<(i64, String, String)> = self
            .conn
            .query_row(
                "SELECT account, economy, place FROM econ_items WHERE item_id = ?1",
                [id.0],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        match row {
            Some((acc, econ, place)) if acc == account && place == "stash" => {
                if econ == split {
                    Ok(())
                } else {
                    Err(Refusal::EconomyMismatch.into())
                }
            }
            Some((acc, _, _)) if acc == account => Err(Refusal::ItemHeld.into()),
            _ => Err(Refusal::ItemNotFound.into()),
        }
    }

    /// Opens a transfer in flight to `to` carrying `goods` (an item moves
    /// to it; counts and gold were already taken from their source).
    pub fn open_flight(
        &self,
        account: i64,
        split: &str,
        to: &CharRef,
        from: (&str, Option<i64>),
        goods: &Goods,
    ) -> Result<i64, Error> {
        let (item, code, count, gold) = goods_columns(goods);
        self.exec(
            "INSERT INTO in_flight(account, economy, character, game, lease, from_kind, from_id, \
             item_id, code, count, gold, at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                account,
                split,
                to.name,
                to.game,
                to.lease as i64,
                from.0,
                from.1,
                item,
                code,
                count,
                gold,
                self.now,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        if let Goods::Item(item) = goods {
            self.exec(
                "UPDATE econ_items SET place = 'in_flight', place_id = ?2 WHERE item_id = ?1",
                params![item.0, id],
            )?;
            self.moved(*item, Place::InFlight, Some(account), Some(to), "take")?;
        }
        Ok(id)
    }

    pub fn flight(&self, id: i64) -> Result<Option<InFlight>, Error> {
        read_flight(self.conn, id)
    }

    fn flight_source(&self, id: i64) -> Result<(String, String, Option<i64>), Error> {
        Ok(self.conn.query_row(
            "SELECT economy, from_kind, from_id FROM in_flight WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?)
    }

    /// Puts a transfer's goods back where they came from and closes it.
    pub fn undo_flight(&self, flight: &InFlight) -> Result<(), Error> {
        let (split, from_kind, from_id) = self.flight_source(flight.id)?;
        match from_kind.as_str() {
            "stash" => {
                self.give_to_stash(flight.account, &split, &flight.goods, false, "take_undo")?
            }
            "delivery" => self.undo_delivery_flight(flight, &split)?,
            other => return Err(Error::Db(format!("transfer from unknown place {other:?}"))),
        }
        // Back to the stash of the account, or to the delivery it came from.
        let back = if from_kind == "delivery" {
            from_id
        } else {
            Some(flight.account)
        };
        self.close_flight(flight, "take_undo", (from_kind.as_str(), back))
    }

    /// Ends a transfer and logs where its goods went.
    pub fn close_flight(
        &self,
        flight: &InFlight,
        kind: &str,
        to: (&str, Option<i64>),
    ) -> Result<(), Error> {
        let (split, _, _) = self.flight_source(flight.id)?;
        self.exec("DELETE FROM in_flight WHERE id = ?1", [flight.id])?;
        self.log(Transfer {
            kind,
            economy: &split,
            goods: &flight.goods,
            from: ("in_flight", Some(flight.id)),
            to,
            character: Some(&flight.character.name),
            game: Some(flight.character.game),
            lease: Some(flight.character.lease),
            listing: None,
            offer: None,
        })
    }

    /// Puts goods already in the economy (an item in flight or held
    /// elsewhere, counts, gold) into an account's stash. `capped`: refuse
    /// past the stash's limits; returns never refuse.
    pub fn give_to_stash(
        &self,
        account: i64,
        split: &str,
        goods: &Goods,
        capped: bool,
        why: &str,
    ) -> Result<(), Error> {
        match goods {
            Goods::Item(id) => {
                if capped && self.stash_count(account, split)? >= i64::from(self.config.stash_items)
                {
                    return Err(Refusal::StashFull.into());
                }
                self.exec(
                    "UPDATE econ_items SET place = 'stash', place_id = NULL, account = ?2 \
                     WHERE item_id = ?1",
                    params![id.0, account],
                )?;
                self.moved(*id, Place::Stash, Some(account), None, why)?;
                Ok(())
            }
            Goods::Stack { code, count } => self.add_stack(account, split, code, *count, capped),
            Goods::Gold(gold) => self.add_gold(account, split, *gold, capped),
        }
    }
}

/// The `item_id, code, count, gold` columns of goods.
pub(crate) fn goods_columns(
    goods: &Goods,
) -> (Option<u32>, Option<&str>, Option<u32>, Option<i64>) {
    match goods {
        Goods::Item(id) => (Some(id.0), None, None, None),
        Goods::Stack { code, count } => (None, Some(code.as_str()), Some(*count), None),
        Goods::Gold(g) => (None, None, None, Some(*g as i64)),
    }
}
