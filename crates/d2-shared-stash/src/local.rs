//! The single-player shared stash: the economy library's store in the
//! game's save folder, next to the characters.
//!
//! - **Where:** one SQLite file per split of the game's economy, in the
//!   game's save folder (each mod has its own folder, so its own stash):
//!   `shared-stash-sc.sqlite`, `shared-stash-hc.sqlite`, and
//!   `shared-stash-sc-classic.sqlite` / `-hc-classic` for classic
//!   characters ([`file_name`]). Softcore and hardcore never share goods,
//!   nor do two mods; the file also carries its mod's name and refuses to
//!   open as another mod's.
//! - **How a move goes:** the game takes the goods off the character
//!   ([`LocalGame::lift`]) and gives the character's record without them;
//!   one economy transaction stores the goods and that record (pending);
//!   then the record is written to the character's file and the pending
//!   row dropped ([`crate::pending`]). A take is the economy's two steps:
//!   the goods leave the stash into a transfer in flight, the game places
//!   them, and a second transaction closes the transfer with the
//!   character's record holding them. While a move runs, the game's own
//!   periodic saves wait ([`LocalGame::hold_saves`]), so the character file
//!   never shows a half-done move.
//! - **After a crash:** [`LocalStash::open`] puts every take still in
//!   flight back in the stash, writes every pending record to its
//!   character file, and repairs the item pages' cells: before any
//!   character loads. An item is then in exactly one place.
//! - **Backups:** each open keeps the file as it was in
//!   `<file>.bak` (a consistent copy made by SQLite), as character saves
//!   keep their `.bak`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use d2_economy::rusqlite::Connection;
use d2_economy::{
    CharRef, Economy, EconomyConfig, EconomyKey, Goods, Item, ItemFacts, ItemFilter, ItemId, Owner,
    Placed, PutGoods, Refusal, Req, Save, StashQuery,
};

use crate::catalog::Catalog;
use crate::cells::{Cell, Cells, Grid, Placement, StashShape, PAGE_NAME_MAX};
use crate::pending::{self, PendingRecords};
use crate::{PageItem, Put, PutDone, StashStore, StoreError, Take, TakeTo};

/// The one account of a single-player stash.
const ACCOUNT: i64 = 1;
/// The one seat lease of single player: every take still in flight when
/// the stash opens belongs to a game that ended without finishing it.
const LEASE: u64 = 1;

/// A game's refusal or failure, as the game half reports it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GameError {
    /// The goods don't fit on the character.
    #[error("no room")]
    NoRoom,
    /// No such unit on the character, or it can't leave (held, equipped).
    #[error("{0}")]
    Refused(String),
}

impl From<GameError> for StoreError {
    fn from(e: GameError) -> Self {
        match e {
            GameError::NoRoom => StoreError::NoRoom,
            GameError::Refused(s) => StoreError::Game(s),
        }
    }
}

/// What to take off the character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lift {
    /// An item by its unit ID (on the cursor or in the inventory; never
    /// equipped).
    Unit(u32),
    Gold(u64),
}

/// Goods taken off the character, held by the game until the move ends.
// One per move: an item's facts can stay inline.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lifted {
    Item {
        unit: u32,
        /// The item's bytes in the save format.
        record: Vec<u8>,
        facts: ItemFacts,
        /// Width and height in cells.
        size: (u8, u8),
        /// How many of its kind this item is, when it is a plain stackable
        /// item (a rune, a gem, a potion: 1; keys: their quantity); `None`
        /// for an item that must stay an item (sockets filled, a set or
        /// unique, ...).
        count: Option<u32>,
        /// The item as the game client shows it (its item message
        /// stream), kept for the panel when given.
        look: Option<Vec<u8>>,
    },
    Gold(u64),
}

/// Goods arriving on the character from the stash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arrive {
    Item {
        record: Vec<u8>,
    },
    /// `count` of a stackable kind, made as normal items.
    Stack {
        code: String,
        count: u32,
    },
    Gold(u64),
}

/// The game half of single player's moves: the in-process game server's
/// end, which owns the character.
pub trait LocalGame {
    /// The character's name.
    fn character(&self) -> String;
    /// The character's save file.
    fn save_path(&self) -> PathBuf;
    /// While true, the game's periodic character saves wait.
    fn hold_saves(&mut self, hold: bool) {
        let _ = hold;
    }
    /// What item `unit` is, without moving it: its code, and its count
    /// when it is a plain stackable item (as [`Lifted::Item::count`]).
    /// `None`: no such item in the inventory or on the cursor.
    fn kind(&mut self, unit: u32) -> Option<(String, Option<u32>)>;
    /// Takes goods off the character and holds them.
    fn lift(&mut self, what: Lift) -> Result<Lifted, GameError>;
    /// Gives lifted goods back where they were (the move was refused).
    fn restore(&mut self, lifted: &Lifted) -> Result<(), GameError>;
    /// How many of `arrive` fit on the character now (an item: 0 or 1; a
    /// stack: items made; gold: up to the character's gold limit).
    fn room(&mut self, arrive: &Arrive, to: TakeTo) -> u64;
    /// Puts goods on the character.
    fn place(&mut self, arrive: &Arrive, to: TakeTo) -> Result<(), GameError>;
    /// Takes back what [`LocalGame::place`] just put there (the move could
    /// not be stored).
    fn unplace(&mut self, arrive: &Arrive, to: TakeTo);
    /// The character's file bytes as the game would save them now.
    fn record(&mut self) -> Result<Vec<u8>, GameError>;
}

/// What [`LocalStash::open`] found from a game that stopped mid-move.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recovered {
    /// Takes in flight put back in the stash.
    pub undone: usize,
    /// Character files written from their pending records.
    pub rewritten: Vec<PathBuf>,
    /// Items with no free cell left on the item pages (still in the stash,
    /// shown once room is made).
    pub homeless: Vec<ItemId>,
}

/// The stash file's name for a split of the economy.
pub fn file_name(key: &EconomyKey) -> String {
    format!(
        "shared-stash-{}{}.sqlite",
        if key.hardcore { "hc" } else { "sc" },
        if key.expansion { "" } else { "-classic" }
    )
}

/// Single player's shared stash for one split of one game.
pub struct LocalStash {
    econ: Economy,
    /// The same file, for the cells, pending records and counters.
    aux: Connection,
    owner: Owner,
    catalog: Catalog,
    /// The item pages and the category slots.
    grid: Grid,
    hook: PendingRecords,
    path: PathBuf,
    /// Whether the last open found work left by a stopped game.
    recovered: Recovered,
    #[cfg(any(test, feature = "crash-points"))]
    pub(crate) crash: Option<Crash>,
}

impl std::fmt::Debug for LocalStash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalStash")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

/// Test crash points between the steps of a move (the tests and the
/// `crash-points` feature, for the game's own tests of a stopped move).
#[cfg(any(test, feature = "crash-points"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crash {
    /// The goods are off the character (in memory), nothing stored.
    Lifted,
    /// Inside the move's transaction.
    InTransaction,
    /// The `n`-th move of this call committed; the character file not
    /// written.
    Stored(u32),
    /// The character file written, its pending record not dropped.
    Written,
    /// A take's goods in flight, not placed.
    InFlight,
    /// A take's goods placed on the character, the transfer still open.
    Placed,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

impl LocalStash {
    /// Opens (creating if needed) the stash of `mod_id`'s split `key` in
    /// `folder` (the game's save folder), with the game's `catalog`.
    /// Finishes what a stopped game left, then keeps a backup. Call it
    /// before the character loads.
    pub fn open(
        folder: &Path,
        key: EconomyKey,
        catalog: Catalog,
    ) -> Result<LocalStash, StoreError> {
        LocalStash::open_with(folder, key, catalog, StashShape::DEFAULT)
    }

    /// The mod game that stash file `path` belongs to (the `mod_id` its
    /// first open wrote); `None` for a file without one.
    pub fn file_mod(path: &Path) -> Result<Option<String>, StoreError> {
        use d2_economy::rusqlite::{OpenFlags, OptionalExtension};
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(conn
            .query_row("SELECT value FROM meta WHERE key = 'mod_id'", [], |r| {
                r.get(0)
            })
            .optional()?)
    }

    /// [`LocalStash::open`] with the game's own item pages: a mod game's
    /// `shape`. Items whose cells lie off the shape (a shape that shrank)
    /// move to the first free cells.
    pub fn open_with(
        folder: &Path,
        key: EconomyKey,
        catalog: Catalog,
        shape: StashShape,
    ) -> Result<LocalStash, StoreError> {
        LocalStash::opened(folder, key, catalog, Some(shape))
    }

    /// Finishes what a stopped game left in the stash of split `key` (as
    /// [`LocalStash::open`]) without the game's item pages: the cells are
    /// left for the game's own open to check against its shape. For a
    /// folder's start before the game's settings are known.
    pub fn recover_only(
        folder: &Path,
        key: EconomyKey,
        catalog: Catalog,
    ) -> Result<Recovered, StoreError> {
        Ok(LocalStash::opened(folder, key, catalog, None)?.recovered)
    }

    /// Opens the file; `shape` `None`: no cell repair.
    fn opened(
        folder: &Path,
        key: EconomyKey,
        catalog: Catalog,
        shape: Option<StashShape>,
    ) -> Result<LocalStash, StoreError> {
        let path = folder.join(file_name(&key));
        let config = EconomyConfig {
            stackables: catalog.stackables(),
            // The item pages are the limit (`Cells`), not a count.
            stash_items: u32::MAX,
            ..EconomyConfig::default()
        };
        let hook = PendingRecords::default();
        let econ = Economy::open(&path, &key.mod_id, config)?.with_records(Box::new(hook.clone()));
        let aux = Connection::open(&path)?;
        aux.busy_timeout(std::time::Duration::from_secs(5))?;
        aux.execute_batch(pending::SCHEMA)?;
        aux.execute_batch(crate::cells::SCHEMA)?;
        let mut stash = LocalStash {
            econ,
            aux,
            owner: Owner::new(ACCOUNT, key),
            grid: Grid::new(shape.unwrap_or_default(), &catalog),
            catalog,
            hook,
            path,
            recovered: Recovered::default(),
            #[cfg(any(test, feature = "crash-points"))]
            crash: None,
        };
        stash.recovered = stash.recover(shape.is_some())?;
        stash.backup()?;
        Ok(stash)
    }

    /// The stash file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The catalog of the stackable pages.
    pub fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    /// The item pages' shape.
    pub fn shape(&self) -> StashShape {
        self.grid.shape
    }

    /// The item pages' names, page I first: the player's, else the
    /// default roman numeral.
    pub fn page_names(&self) -> Result<Vec<String>, StoreError> {
        let mut named = Cells::page_names(&self.aux)?;
        Ok((0..self.grid.shape.pages)
            .map(|p| {
                named
                    .remove(&p)
                    .unwrap_or_else(|| StashShape::default_name(p))
            })
            .collect())
    }

    /// Names item page `page` (at most [`PAGE_NAME_MAX`] characters,
    /// trimmed; empty: back to the default). Refused
    /// ([`Refusal::BadRequest`]) unless the shape is renamable, or for a
    /// page past the shape or a name too long.
    pub fn rename_page(&mut self, page: u8, name: &str) -> Result<(), StoreError> {
        let name = name.trim();
        if !self.grid.shape.renamable
            || page >= self.grid.shape.pages
            || name.chars().count() > PAGE_NAME_MAX
            || name.chars().any(char::is_control)
        {
            return Err(Refusal::BadRequest.into());
        }
        Cells::set_page_name(&self.aux, page, name)
    }

    /// What the open finished for a stopped game.
    pub fn recovered(&self) -> &Recovered {
        &self.recovered
    }

    /// The game wrote `character`'s file on its own (a periodic save, or
    /// save and exit): its state holds every move, so a pending record
    /// left by a failed file write is no longer needed.
    pub fn character_saved(&mut self, character: &str) -> Result<(), StoreError> {
        pending::forget(&self.aux, character)
    }

    fn recover(&mut self, cells: bool) -> Result<Recovered, StoreError> {
        let undone = self.econ.undo_lease(LEASE, now_ms())?;
        let mut rewritten = Vec::new();
        for p in pending::all(&self.aux)? {
            pending::write_file(&p.path, &p.record)?;
            pending::clear(&self.aux, &p.character, &p.record)?;
            rewritten.push(p.path);
        }
        let homeless = if cells {
            Cells::repair(&self.aux, &self.grid, &self.item_ids()?)?
        } else {
            Vec::new()
        };
        Ok(Recovered {
            undone,
            rewritten,
            homeless,
        })
    }

    /// Copies the file to `<file>.bak` through SQLite (a consistent copy,
    /// even with the write-ahead log in use).
    fn backup(&self) -> Result<(), StoreError> {
        let with = |ext: &str| {
            let mut p = self.path.as_os_str().to_owned();
            p.push(ext);
            PathBuf::from(p)
        };
        let tmp = with(".bak.tmp");
        let _ = std::fs::remove_file(&tmp);
        self.aux
            .execute("VACUUM INTO ?1", [tmp.to_string_lossy()])?;
        std::fs::rename(&tmp, with(".bak"))
            .map_err(|e| StoreError::Io(format!("{}: {e}", self.path.display())))
    }

    /// Stops the next move at `at` with a panic, as a stopped game.
    #[cfg(any(test, feature = "crash-points"))]
    pub fn set_crash(&mut self, at: Option<Crash>) {
        self.crash = at;
    }

    #[cfg(any(test, feature = "crash-points"))]
    fn crash_at(&self, at: Crash) {
        if self.crash == Some(at) {
            panic!("injected crash at {at:?}");
        }
    }

    /// Every item in the stash, with what the economy knows of it.
    fn all_items(&self) -> Result<BTreeMap<u32, Item>, StoreError> {
        let mut out = BTreeMap::new();
        let mut q = StashQuery::default();
        loop {
            let page = self.econ.stash_items(&self.owner, &q)?;
            let n = page.items.len();
            for it in page.items {
                out.insert(it.item.id.0, it.item);
            }
            if n < d2_economy::PAGE as usize {
                return Ok(out);
            }
            q.page += 1;
        }
    }

    fn item_ids(&self) -> Result<BTreeSet<u32>, StoreError> {
        Ok(self.all_items()?.into_keys().collect())
    }

    fn chr(&self, game: &dyn LocalGame) -> CharRef {
        CharRef {
            name: game.character(),
            game: 0,
            lease: LEASE,
        }
    }

    fn req(&self) -> Result<Req, StoreError> {
        Ok(Req::account(
            self.owner.account,
            pending::next(&self.aux, "req")?,
        ))
    }

    fn lease_req(&self) -> Result<Req, StoreError> {
        Ok(Req::lease(LEASE, pending::next(&self.aux, "req")?))
    }

    /// Writes `character`'s pending record to its file and drops it.
    fn write_pending(&self, character: &str) -> Result<(), StoreError> {
        if let Some(p) = pending::get(&self.aux, character)? {
            pending::write_file(&p.path, &p.record)?;
            #[cfg(any(test, feature = "crash-points"))]
            self.crash_at(Crash::Written);
            pending::clear(&self.aux, character, &p.record)?;
        }
        Ok(())
    }

    /// One put of lifted goods: the economy transaction with the
    /// character's record. Refused or failed: nothing changed.
    fn store_put(&mut self, game: &mut dyn LocalGame, goods: PutGoods) -> Result<(), StoreError> {
        let record = game.record()?;
        let chr = self.chr(game);
        let req = self.req()?;
        let seq = req.n as u32;
        self.hook.aim(&game.save_path());
        #[cfg(any(test, feature = "crash-points"))]
        {
            self.hook.lock().crash = self.crash == Some(Crash::InTransaction);
        }
        self.econ.put(
            &self.owner,
            &req,
            &chr,
            goods,
            &Save { record, seq },
            now_ms(),
        )?;
        Ok(())
    }

    fn stack_count(&self, code: &str) -> Result<u32, StoreError> {
        Ok(self
            .econ
            .stash_stacks(&self.owner)?
            .into_iter()
            .find(|(c, _)| c == code)
            .map_or(0, |(_, n)| n))
    }

    /// Puts one lifted stackable to its slot; `Err` leaves it lifted.
    fn put_stack(
        &mut self,
        game: &mut dyn LocalGame,
        code: &str,
        count: u32,
    ) -> Result<(), StoreError> {
        if self.catalog.slot(code).is_none() {
            return Err(Refusal::BadRequest.into());
        }
        if u64::from(self.stack_count(code)?) + u64::from(count) > u64::from(self.catalog.cap()) {
            return Err(Refusal::StashFull.into());
        }
        self.store_put(
            game,
            PutGoods::Stack {
                code: code.to_owned(),
                count,
            },
        )
    }

    /// Puts one lifted item on the item pages, at `at` or the first free
    /// cells; `Err` leaves it lifted. An `at` on a category page puts it
    /// on that slot when free, else the category's first free slot.
    fn put_item(
        &mut self,
        game: &mut dyn LocalGame,
        record: Vec<u8>,
        facts: ItemFacts,
        size: (u8, u8),
        at: Option<Cell>,
        look: Option<&[u8]>,
    ) -> Result<(), StoreError> {
        let cells = Cells::load(&self.aux)?;
        let g = &self.grid;
        let cell = match at {
            Some(c) if cells.is_free(g, &Placement { cell: c, size }, None) => c,
            Some(c) if c.category().is_some() => cells
                .first_free_on(g, size, c.page)
                .ok_or(Refusal::StashFull)?,
            Some(c) => cells
                .first_free(g, size, c.page)
                .ok_or(Refusal::StashFull)?,
            None => cells.first_free(g, size, 0).ok_or(Refusal::StashFull)?,
        };
        let id = ItemId(pending::next(&self.aux, "item")? as u32);
        // The cell first: a move that doesn't happen leaves a cell that
        // the next repair drops; one that does always has its cell.
        Cells::set(&self.aux, id, Placement { cell, size })?;
        let put = self.store_put(game, PutGoods::Item(Item { id, record, facts }));
        match (&put, look) {
            (Err(_), _) => Cells::remove(&self.aux, id)?,
            // Only for the panel: a crash before this line shows the
            // item from its bytes alone.
            (Ok(()), Some(look)) => Cells::set_look(&self.aux, id, look)?,
            (Ok(()), None) => {}
        }
        put
    }

    /// Runs `f` with the game's periodic saves held.
    fn held<T>(
        game: &mut dyn LocalGame,
        f: impl FnOnce(&mut dyn LocalGame) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        game.hold_saves(true);
        let out = f(game);
        game.hold_saves(false);
        out
    }

    /// Character → stash.
    pub fn put(&mut self, game: &mut dyn LocalGame, put: Put) -> Result<PutDone, StoreError> {
        Self::held(game, |game| {
            let character = game.character();
            let mut done = PutDone::default();
            let units: Vec<(u32, bool)> = match &put {
                Put::Item { unit, .. } => vec![(*unit, false)],
                Put::Stacks { units } => units.iter().map(|u| (*u, true)).collect(),
                Put::Gold(gold) => {
                    let lifted = game.lift(Lift::Gold(*gold))?;
                    #[cfg(any(test, feature = "crash-points"))]
                    self.crash_at(Crash::Lifted);
                    if let Err(e) = self.store_put(game, PutGoods::Gold(*gold)) {
                        game.restore(&lifted)?;
                        return Err(e);
                    }
                    #[cfg(any(test, feature = "crash-points"))]
                    self.crash_at(Crash::Stored(1));
                    self.write_pending(&character)?;
                    return Ok(done);
                }
            };
            for (unit, stacks_only) in units {
                if stacks_only {
                    // Only stackables of a listed kind are lifted at all;
                    // everything else never leaves its place.
                    let why = match game.kind(unit) {
                        None => Some(Refusal::ItemNotFound),
                        Some((code, Some(n))) if self.catalog.slot(&code).is_some() => {
                            let have = u64::from(self.stack_count(&code)?);
                            (have + u64::from(n) > u64::from(self.catalog.cap()))
                                .then_some(Refusal::StashFull)
                        }
                        Some(_) => Some(Refusal::BadRequest),
                    };
                    if let Some(why) = why {
                        done.left.push((unit, why));
                        continue;
                    }
                }
                let lifted = game.lift(Lift::Unit(unit))?;
                #[cfg(any(test, feature = "crash-points"))]
                self.crash_at(Crash::Lifted);
                let Lifted::Item {
                    record,
                    facts,
                    size,
                    count,
                    look,
                    ..
                } = &lifted
                else {
                    game.restore(&lifted)?;
                    return Err(Refusal::BadRequest.into());
                };
                let stack = count.filter(|_| self.catalog.slot(&facts.code).is_some());
                let r = match (stack, stacks_only, &put) {
                    (Some(n), _, _) => self.put_stack(game, &facts.code.clone(), n),
                    (None, true, _) => Err(Refusal::BadRequest.into()),
                    (None, false, Put::Item { at, .. }) => {
                        let (record, facts, size, at) = (record.clone(), facts.clone(), *size, *at);
                        self.put_item(game, record, facts, size, at, look.as_deref())
                    }
                    (None, false, _) => Err(Refusal::BadRequest.into()),
                };
                match r {
                    Ok(()) => {
                        done.moved.push(unit);
                        #[cfg(any(test, feature = "crash-points"))]
                        self.crash_at(Crash::Stored(done.moved.len() as u32));
                    }
                    Err(StoreError::Refused(why)) => {
                        game.restore(&lifted)?;
                        if !stacks_only {
                            return Err(why.into());
                        }
                        done.left.push((unit, why));
                    }
                    Err(e) => {
                        game.restore(&lifted)?;
                        // What is stored so far is kept; its record is
                        // written below on the next call or the next open.
                        let _ = self.write_pending(&character);
                        return Err(e);
                    }
                }
            }
            self.write_pending(&character)?;
            Ok(done)
        })
    }

    /// Stash → character.
    pub fn take(&mut self, game: &mut dyn LocalGame, take: Take) -> Result<u64, StoreError> {
        Self::held(game, |game| {
            let (what, to) = match take {
                Take::Item { id, to } => (Goods::Item(id), to),
                Take::Stack { code, count } => {
                    let have = self.stack_count(&code)?;
                    let want = count.min(have);
                    if want == 0 {
                        return Err(Refusal::ItemNotFound.into());
                    }
                    let probe = Arrive::Stack {
                        code: code.clone(),
                        count: want,
                    };
                    let fit = game.room(&probe, TakeTo::Inventory).min(u64::from(want)) as u32;
                    if fit == 0 {
                        return Err(StoreError::NoRoom);
                    }
                    (Goods::Stack { code, count: fit }, TakeTo::Inventory)
                }
                Take::Gold(gold) => {
                    let have = self.econ.stash_gold(&self.owner)?;
                    let want = gold.min(have);
                    if want == 0 {
                        return Err(Refusal::ItemNotFound.into());
                    }
                    let fit = game.room(&Arrive::Gold(want), TakeTo::Inventory).min(want);
                    if fit == 0 {
                        return Err(StoreError::NoRoom);
                    }
                    (Goods::Gold(fit), TakeTo::Inventory)
                }
            };
            let taken = match &what {
                Goods::Item(_) => 1,
                Goods::Stack { count, .. } => u64::from(*count),
                Goods::Gold(g) => *g,
            };
            let chr = self.chr(game);
            let character = chr.name.clone();
            let req = self.req()?;
            let transfer = self
                .econ
                .take(&self.owner, &req, &chr, what.clone(), now_ms())?
                .id
                .ok_or_else(|| StoreError::Db("take made no transfer".into()))?;
            #[cfg(any(test, feature = "crash-points"))]
            self.crash_at(Crash::InFlight);
            let flight = self
                .econ
                .in_flight(transfer)?
                .ok_or_else(|| StoreError::Db("transfer vanished".into()))?;
            let arrive = match (&what, flight.item) {
                (Goods::Item(_), Some(item)) => Arrive::Item {
                    record: item.record,
                },
                (Goods::Item(_), None) => return Err(StoreError::Db("item without bytes".into())),
                (Goods::Stack { code, count }, _) => Arrive::Stack {
                    code: code.clone(),
                    count: *count,
                },
                (Goods::Gold(g), _) => Arrive::Gold(*g),
            };
            if let Err(e) = game.place(&arrive, to) {
                let why = if e == GameError::NoRoom {
                    Placed::NoRoom
                } else {
                    Placed::Refused
                };
                let req = self.lease_req()?;
                self.econ
                    .take_done(&req, LEASE, transfer, why, None, now_ms())?;
                return Err(e.into());
            }
            #[cfg(any(test, feature = "crash-points"))]
            self.crash_at(Crash::Placed);
            let closed = (|| {
                let record = game.record()?;
                let req = self.lease_req()?;
                self.hook.aim(&game.save_path());
                #[cfg(any(test, feature = "crash-points"))]
                {
                    self.hook.lock().crash = self.crash == Some(Crash::InTransaction);
                }
                let save = Save {
                    record,
                    seq: req.n as u32,
                };
                self.econ.take_done(
                    &req,
                    LEASE,
                    transfer,
                    Placed::Placed,
                    Some(&save),
                    now_ms(),
                )?;
                Ok::<_, StoreError>(())
            })();
            if let Err(e) = closed {
                // The transfer stays open and goes back to the stash at the
                // next open: the goods must not stay on the character too.
                game.unplace(&arrive, to);
                return Err(e);
            }
            #[cfg(any(test, feature = "crash-points"))]
            self.crash_at(Crash::Stored(1));
            if let Goods::Item(id) = what {
                Cells::remove(&self.aux, id)?;
            }
            self.write_pending(&character)?;
            Ok(taken)
        })
    }

    pub fn move_item(&mut self, id: ItemId, to: Cell) -> Result<(), StoreError> {
        let cells = Cells::load(&self.aux)?;
        let p = *cells.by_item.get(&id.0).ok_or(Refusal::ItemNotFound)?;
        let to = Placement {
            cell: to,
            size: p.size,
        };
        if !cells.is_free(&self.grid, &to, Some(id)) {
            return Err(Refusal::StashFull.into());
        }
        Cells::set(&self.aux, id, to)
    }

    pub fn arrange(&mut self, page: u8) -> Result<(), StoreError> {
        let items = self.all_items()?;
        let cells = Cells::load(&self.aux)?;
        let mut on: Vec<(u32, Placement)> = cells
            .by_item
            .iter()
            .filter(|(_, p)| p.cell.page == page)
            .map(|(id, p)| (*id, *p))
            .collect();
        let key = |id: &u32| {
            items
                .get(id)
                .map(|i| (i.facts.item_type.clone(), i.facts.name.clone()))
                .unwrap_or_default()
        };
        on.sort_by(|a, b| {
            let area = |p: &Placement| u16::from(p.size.0) * u16::from(p.size.1);
            area(&b.1)
                .cmp(&area(&a.1))
                .then_with(|| key(&a.0).cmp(&key(&b.0)))
                .then(a.0.cmp(&b.0))
        });
        let mut packed = Cells::default();
        for (id, p) in &cells.by_item {
            if p.cell.page != page {
                packed.by_item.insert(*id, *p);
            }
        }
        let mut out = Vec::new();
        for (id, p) in on {
            // Every item fitted before, so a packing from the top-left
            // finds room; if not, the item keeps its cell.
            let cell = packed
                .first_free_on(&self.grid, p.size, page)
                .unwrap_or(p.cell);
            let np = Placement { cell, size: p.size };
            packed.by_item.insert(id, np);
            out.push((id, np));
        }
        let tx = self.aux.unchecked_transaction()?;
        for (id, p) in out {
            Cells::set(&tx, ItemId(id), p)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn page(&mut self, page: u8) -> Result<Vec<PageItem>, StoreError> {
        let items = self.all_items()?;
        let cells = Cells::load(&self.aux)?;
        let mut looks = Cells::looks(&self.aux)?;
        Ok(cells
            .by_item
            .iter()
            .filter(|(_, p)| p.cell.page == page)
            .filter_map(|(id, p)| {
                items.get(id).map(|it| PageItem {
                    id: it.id,
                    cell: p.cell,
                    size: p.size,
                    record: it.record.clone(),
                    facts: Some(it.facts.clone()),
                    look: looks.remove(id),
                })
            })
            .collect())
    }

    pub fn stacks(&mut self) -> Result<BTreeMap<String, u32>, StoreError> {
        Ok(self.econ.stash_stacks(&self.owner)?.into_iter().collect())
    }

    pub fn gold(&mut self) -> Result<u64, StoreError> {
        Ok(self.econ.stash_gold(&self.owner)?)
    }

    pub fn search(&mut self, text: &str) -> Result<Vec<(ItemId, Cell)>, StoreError> {
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        let cells = Cells::load(&self.aux)?;
        let mut out = Vec::new();
        let mut q = StashQuery {
            filter: ItemFilter {
                text: Some(text.to_owned()),
                ..ItemFilter::default()
            },
            ..StashQuery::default()
        };
        loop {
            let page = self.econ.stash_items(&self.owner, &q)?;
            let n = page.items.len();
            for it in page.items {
                if let Some(p) = cells.by_item.get(&it.item.id.0) {
                    out.push((it.item.id, p.cell));
                }
            }
            if n < d2_economy::PAGE as usize {
                break;
            }
            q.page += 1;
        }
        out.sort_by_key(|(_, c)| *c);
        Ok(out)
    }

    /// This stash bound to the running game, as the panel's store.
    pub fn with_game<'a>(&'a mut self, game: &'a mut dyn LocalGame) -> Bound<'a> {
        Bound { stash: self, game }
    }
}

/// A [`LocalStash`] with the game it moves goods to and from: the
/// panel's [`StashStore`] in single player.
pub struct Bound<'a> {
    pub stash: &'a mut LocalStash,
    pub game: &'a mut dyn LocalGame,
}

impl StashStore for Bound<'_> {
    fn online(&self) -> bool {
        false
    }
    fn catalog(&self) -> &Catalog {
        &self.stash.catalog
    }
    fn page(&mut self, page: u8) -> Result<Vec<PageItem>, StoreError> {
        self.stash.page(page)
    }
    fn stacks(&mut self) -> Result<BTreeMap<String, u32>, StoreError> {
        self.stash.stacks()
    }
    fn gold(&mut self) -> Result<u64, StoreError> {
        self.stash.gold()
    }
    fn search(&mut self, text: &str) -> Result<Vec<(ItemId, Cell)>, StoreError> {
        self.stash.search(text)
    }
    fn put(&mut self, put: Put) -> Result<PutDone, StoreError> {
        self.stash.put(&mut *self.game, put)
    }
    fn take(&mut self, take: Take) -> Result<u64, StoreError> {
        self.stash.take(&mut *self.game, take)
    }
    fn move_item(&mut self, id: ItemId, to: Cell) -> Result<(), StoreError> {
        self.stash.move_item(id, to)
    }
    fn arrange(&mut self, page: u8) -> Result<(), StoreError> {
        self.stash.arrange(page)
    }
    fn shape(&self) -> StashShape {
        self.stash.shape()
    }
    fn page_names(&mut self) -> Result<Vec<String>, StoreError> {
        self.stash.page_names()
    }
    fn rename_page(&mut self, page: u8, name: &str) -> Result<(), StoreError> {
        self.stash.rename_page(page, name)
    }
}
