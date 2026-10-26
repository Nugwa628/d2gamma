//! The online stash's own tables in the realm database, beside the
//! economy's and the item registry's, and the economy's two hooks.
//!
//! - **Cells** (`realm_stash_cells`): where each stash item lies on the
//!   item pages, per account and economy. Only the layout: the economy
//!   says which items are in the stash, and items without a valid cell
//!   get the first free one when a page is read ([`Store::layout`]), so a
//!   crash between a move and its cell write moves an item but never
//!   loses it.
//! - **Records** (`realm_stash_records`): a character record a move
//!   stored, written in the move's own transaction ([`Records`], the
//!   economy's `CharacterRecords`). The character file is written from it
//!   once the move committed, and the row dropped; a row still there when
//!   the main server starts is written then ([`Store::records`]). So a
//!   crash between the economy's commit and the file write neither
//!   duplicates the goods (the record without a put item is written) nor
//!   loses them (the record holding a taken item is).
//! - **Item IDs** ([`RealmItems`], the economy's `ItemRegistry`): every
//!   move of a stash item moves its realm item ID in the registry, and the
//!   IDs of the items socketed in it with it.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use d2_accounts::rules::name_key;
use d2_economy::{CharRef, CharacterRecords, EconomyKey, ItemId, Moved, Place, Refusal};
use d2_item_registry::{Place as IdPlace, Registry, Split};
use d2_net::msg::stash::StashCell;
use d2_save_checks::CheckTables;
use d2_sim::items::bitstream::hflag;
use d2_sim::items::bitstream::read::{read_save_entry, ReadEntry};
use d2_sim::items::tables::ItemTables;
use rusqlite::{params, Connection, OptionalExtension};

use super::cells::{Layout, Placement};
use d2_shared_stash::Grid;

/// The stash's tables (created when the store opens, before the economy's
/// first move: [`Records`] writes into them in its transactions).
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS realm_stash_cells(
    item_id INTEGER PRIMARY KEY,
    account INTEGER NOT NULL,
    economy TEXT NOT NULL,
    page INTEGER NOT NULL,
    x INTEGER NOT NULL,
    y INTEGER NOT NULL,
    w INTEGER NOT NULL,
    h INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS realm_stash_cells_owner ON realm_stash_cells(account, economy);
CREATE TABLE IF NOT EXISTS realm_stash_page_names(
    account INTEGER NOT NULL,
    economy TEXT NOT NULL,
    page INTEGER NOT NULL,
    name TEXT NOT NULL,
    PRIMARY KEY(account, economy, page)
);
CREATE TABLE IF NOT EXISTS realm_stash_records(
    character TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    account INTEGER NOT NULL,
    lease INTEGER NOT NULL,
    seq INTEGER NOT NULL,
    record BLOB NOT NULL
);
";

/// A character record a move stored, to be written to its file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredRecord {
    /// The character's name as the move named it.
    pub name: String,
    pub account: i64,
    pub lease: u64,
    pub seq: u32,
    pub record: Vec<u8>,
}

/// An item of a stash page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageItem {
    pub id: u32,
    pub at: Placement,
    pub record: Vec<u8>,
}

/// The stash's own connection to the realm database (reads, and the cell
/// writes; the economy's moves run on the economy worker's).
#[derive(Debug)]
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = d2_item_registry::open_file(path).map_err(|e| e.to_string())?;
        conn.execute_batch(SCHEMA)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[cfg(test)]
    pub fn in_connection(conn: Connection) -> Result<Self, String> {
        conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The layout of an account's stash in economy `split` on the realm's
    /// pages (`grid`), made to agree with the economy first: cells of
    /// items no longer in the stash (nor in flight from it) are dropped,
    /// and stash items without a valid, free cell get the first free one.
    pub fn layout(&self, account: i64, split: &str, grid: &Grid) -> Result<Layout, String> {
        let conn = self.conn();
        let ids = |sql: &str| -> rusqlite::Result<BTreeSet<u32>> {
            let mut st = conn.prepare(sql)?;
            let rows = st.query_map(params![account, split], |r| r.get(0))?;
            rows.collect()
        };
        let stash = ids("SELECT item_id FROM stash_items WHERE account = ?1 AND economy = ?2")
            .map_err(db)?;
        let flying = ids("SELECT item_id FROM in_flight \
             WHERE account = ?1 AND economy = ?2 AND item_id IS NOT NULL")
        .map_err(db)?;
        let old = read_layout(&conn, account, split, grid).map_err(db)?;
        let (layout, changed) = old.repaired(&stash, &flying);
        if changed {
            let tx = conn.unchecked_transaction().map_err(db)?;
            tx.execute(
                "DELETE FROM realm_stash_cells WHERE account = ?1 AND economy = ?2",
                params![account, split],
            )
            .map_err(db)?;
            for (id, p) in &layout.by_item {
                write_cell(&tx, *id, account, split, p).map_err(db)?;
            }
            tx.commit().map_err(db)?;
        }
        Ok(layout)
    }

    /// The names the account gave its item pages in economy `split`, by
    /// page.
    pub fn page_names(&self, account: i64, split: &str) -> Result<BTreeMap<u8, String>, String> {
        let conn = self.conn();
        let mut st = conn
            .prepare(
                "SELECT page, name FROM realm_stash_page_names WHERE account = ?1 AND economy = ?2",
            )
            .map_err(db)?;
        let rows = st
            .query_map(params![account, split], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db)?;
        rows.collect::<Result<_, _>>().map_err(db)
    }

    /// Names page `page`; an empty name gives it back its default.
    pub fn set_page_name(
        &self,
        account: i64,
        split: &str,
        page: u8,
        name: &str,
    ) -> Result<(), String> {
        let conn = self.conn();
        if name.is_empty() {
            conn.execute(
                "DELETE FROM realm_stash_page_names WHERE account = ?1 AND economy = ?2 AND page = ?3",
                params![account, split, page],
            )
        } else {
            conn.execute(
                "INSERT INTO realm_stash_page_names(account, economy, page, name)                  VALUES (?1, ?2, ?3, ?4) ON CONFLICT(account, economy, page)                  DO UPDATE SET name = excluded.name",
                params![account, split, page, name],
            )
        }
        .map(|_| ())
        .map_err(db)
    }

    /// Puts item `id` on `at`.
    pub fn set_cell(
        &self,
        id: u32,
        account: i64,
        split: &str,
        at: &Placement,
    ) -> Result<(), String> {
        write_cell(&self.conn(), id, account, split, at).map_err(db)
    }

    /// Writes several cells at once.
    pub fn set_cells(
        &self,
        account: i64,
        split: &str,
        cells: &[(u32, Placement)],
    ) -> Result<(), String> {
        let conn = self.conn();
        let tx = conn.unchecked_transaction().map_err(db)?;
        for (id, p) in cells {
            write_cell(&tx, *id, account, split, p).map_err(db)?;
        }
        tx.commit().map_err(db)
    }

    pub fn drop_cell(&self, id: u32) -> Result<(), String> {
        self.conn()
            .execute("DELETE FROM realm_stash_cells WHERE item_id = ?1", [id])
            .map(|_| ())
            .map_err(db)
    }

    /// The items of page `page`, with their bytes.
    pub fn page(&self, layout: &Layout, page: u8) -> Result<Vec<PageItem>, String> {
        let conn = self.conn();
        let mut st = conn
            .prepare("SELECT record FROM stash_items WHERE item_id = ?1")
            .map_err(db)?;
        let mut out = Vec::new();
        for (id, at) in layout.by_item.iter().filter(|(_, p)| p.cell.page == page) {
            let record: Option<Vec<u8>> =
                st.query_row([*id], |r| r.get(0)).optional().map_err(db)?;
            if let Some(record) = record {
                out.push(PageItem {
                    id: *id,
                    at: *at,
                    record,
                });
            }
        }
        Ok(out)
    }

    /// Whether item `id` is in the account's stash of `split`.
    pub fn in_stash(&self, id: u32, account: i64, split: &str) -> Result<bool, String> {
        let n: i64 = self
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM stash_items WHERE item_id = ?1 AND account = ?2 \
                 AND economy = ?3",
                params![id, account, split],
                |r| r.get(0),
            )
            .map_err(db)?;
        Ok(n > 0)
    }

    /// The account's gold, item count and stackables in economy `split`.
    pub fn amounts(&self, account: i64, split: &str) -> Result<Amounts, String> {
        let conn = self.conn();
        let gold: Option<i64> = conn
            .query_row(
                "SELECT gold FROM stash_gold WHERE account = ?1 AND economy = ?2",
                params![account, split],
                |r| r.get(0),
            )
            .optional()
            .map_err(db)?;
        let items: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM stash_items WHERE account = ?1 AND economy = ?2",
                params![account, split],
                |r| r.get(0),
            )
            .map_err(db)?;
        let mut st = conn
            .prepare(
                "SELECT code, count FROM stash_stacks WHERE account = ?1 AND economy = ?2 \
                 ORDER BY code",
            )
            .map_err(db)?;
        let stacks = st
            .query_map(params![account, split], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(db)?
            .collect::<Result<Vec<(String, u32)>, _>>()
            .map_err(db)?;
        Ok(Amounts {
            gold: gold.unwrap_or(0).max(0) as u64,
            items: items.max(0) as u64,
            stacks,
        })
    }

    /// The account's stash items of `split` whose names contain `text`
    /// (any case), at most `max`.
    pub fn search(
        &self,
        account: i64,
        split: &str,
        text: &str,
        max: usize,
    ) -> Result<Vec<u32>, String> {
        let pattern = format!(
            "%{}%",
            text.to_lowercase()
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let conn = self.conn();
        let mut st = conn
            .prepare(
                "SELECT item_id FROM econ_items WHERE account = ?1 AND economy = ?2 \
                 AND place = 'stash' AND (lower(name) LIKE ?3 ESCAPE '\\' \
                 OR lower(names) LIKE ?3 ESCAPE '\\' OR lower(code) LIKE ?3 ESCAPE '\\') \
                 ORDER BY item_id LIMIT ?4",
            )
            .map_err(db)?;
        let rows = st
            .query_map(params![account, split, pattern, max as i64], |r| r.get(0))
            .map_err(db)?;
        rows.collect::<Result<_, _>>().map_err(db)
    }

    /// The record a move stored for `character` (its name key), if it
    /// isn't written to its file yet.
    pub fn record(&self, character: &str) -> Result<Option<StoredRecord>, String> {
        self.conn()
            .query_row(
                "SELECT name, account, lease, seq, record FROM realm_stash_records \
                 WHERE character = ?1",
                [character],
                stored_record,
            )
            .optional()
            .map_err(db)
    }

    /// Every record not written to its file yet (the main server's start).
    pub fn records(&self) -> Result<Vec<StoredRecord>, String> {
        let conn = self.conn();
        let mut st = conn
            .prepare(
                "SELECT name, account, lease, seq, record FROM realm_stash_records \
                 ORDER BY character",
            )
            .map_err(db)?;
        let rows = st.query_map([], stored_record).map_err(db)?;
        rows.collect::<Result<_, _>>().map_err(db)
    }

    /// The record is in its file: its row goes, unless a later move
    /// stored another one meanwhile.
    pub fn record_written(&self, r: &StoredRecord) -> Result<(), String> {
        self.conn()
            .execute(
                "DELETE FROM realm_stash_records WHERE character = ?1 AND lease = ?2 AND seq = ?3",
                params![name_key(&r.name), r.lease as i64, r.seq],
            )
            .map(|_| ())
            .map_err(db)
    }

    /// Drops `character`'s row whatever it holds: a later save of the
    /// character is in its file.
    pub fn drop_record(&self, character: &str) -> Result<(), String> {
        self.conn()
            .execute(
                "DELETE FROM realm_stash_records WHERE character = ?1",
                [character],
            )
            .map(|_| ())
            .map_err(db)
    }

    /// The seat leases with transfers still in flight.
    pub fn leases_in_flight(&self) -> Result<Vec<u64>, String> {
        let conn = self.conn();
        let mut st = conn
            .prepare("SELECT DISTINCT lease FROM in_flight ORDER BY lease")
            .map_err(db)?;
        let rows = st
            .query_map([], |r| r.get::<_, i64>(0).map(|l| l as u64))
            .map_err(db)?;
        rows.collect::<Result<_, _>>().map_err(db)
    }
}

/// An account's amounts in one economy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Amounts {
    pub gold: u64,
    pub items: u64,
    pub stacks: Vec<(String, u32)>,
}

fn db(e: rusqlite::Error) -> String {
    format!("realm database: {e}")
}

fn stored_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRecord> {
    Ok(StoredRecord {
        name: r.get(0)?,
        account: r.get(1)?,
        lease: r.get::<_, i64>(2)? as u64,
        seq: r.get(3)?,
        record: r.get(4)?,
    })
}

fn read_layout(
    conn: &Connection,
    account: i64,
    split: &str,
    grid: &Grid,
) -> rusqlite::Result<Layout> {
    let mut st = conn.prepare(
        "SELECT item_id, page, x, y, w, h FROM realm_stash_cells \
         WHERE account = ?1 AND economy = ?2",
    )?;
    let rows = st.query_map(params![account, split], |r| {
        Ok((
            r.get::<_, u32>(0)?,
            Placement {
                cell: StashCell {
                    page: r.get(1)?,
                    x: r.get(2)?,
                    y: r.get(3)?,
                },
                size: (r.get(4)?, r.get(5)?),
            },
        ))
    })?;
    Ok(Layout {
        by_item: rows.collect::<Result<BTreeMap<_, _>, _>>()?,
        grid: grid.clone(),
    })
}

fn write_cell(
    conn: &Connection,
    id: u32,
    account: i64,
    split: &str,
    p: &Placement,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO realm_stash_cells(item_id, account, economy, page, x, y, w, h) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(item_id) DO UPDATE SET account = excluded.account, \
         economy = excluded.economy, page = excluded.page, x = excluded.x, y = excluded.y, \
         w = excluded.w, h = excluded.h",
        params![
            id,
            account,
            split,
            p.cell.page,
            p.cell.x,
            p.cell.y,
            p.size.0,
            p.size.1
        ],
    )?;
    Ok(())
}

/// Which account each seat lease's character is on, for the records the
/// moves store. The stash fills it when it starts a move.
#[derive(Clone, Debug, Default)]
pub struct Owners(Arc<Mutex<HashMap<u64, i64>>>);

impl Owners {
    fn lock(&self) -> MutexGuard<'_, HashMap<u64, i64>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set(&self, lease: u64, account: i64) {
        self.lock().insert(lease, account);
    }

    pub fn get(&self, lease: u64) -> Option<i64> {
        self.lock().get(&lease).copied()
    }

    pub fn ended(&self, lease: u64) {
        self.lock().remove(&lease);
    }
}

/// The economy's `CharacterRecords`: the record goes into
/// `realm_stash_records` in the move's transaction.
#[derive(Debug)]
pub struct Records {
    pub owners: Owners,
}

impl CharacterRecords for Records {
    fn store(
        &self,
        conn: &Connection,
        character: &CharRef,
        record: &[u8],
        seq: u32,
    ) -> Result<(), Refusal> {
        let account = self
            .owners
            .get(character.lease)
            .ok_or(Refusal::TransferInvalid)?;
        conn.execute(
            "INSERT INTO realm_stash_records(character, name, account, lease, seq, record) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(character) DO UPDATE SET name = excluded.name, \
             account = excluded.account, lease = excluded.lease, seq = excluded.seq, \
             record = excluded.record",
            params![
                name_key(&character.name),
                character.name,
                account,
                character.lease as i64,
                seq,
                record
            ],
        )
        .map(|_| ())
        .map_err(|_| Refusal::TransferInvalid)
    }
}

/// The split of an economy key.
pub fn split_of_key(k: &EconomyKey) -> Split {
    Split {
        hardcore: k.hardcore,
        ladder: k.ladder,
        expansion: k.expansion,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// The economy's `ItemRegistry`: the realm's item registry, with the IDs
/// of socketed items moved along with their item's.
pub struct RealmItems {
    pub registry: Registry,
    /// The realm's item tables, to read an item's socketed items.
    pub tables: Option<Arc<CheckTables>>,
    /// The last item that arrived from a character, and from whom: its
    /// socketed items arrive with it when it is put in the stash.
    arrived: Mutex<Option<(u32, CharRef, Split)>>,
}

impl std::fmt::Debug for RealmItems {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RealmItems")
            .field("registry", &self.registry)
            .finish_non_exhaustive()
    }
}

impl RealmItems {
    pub fn new(registry: Registry, tables: Option<Arc<CheckTables>>) -> Self {
        Self {
            registry,
            tables,
            arrived: Mutex::new(None),
        }
    }

    /// The IDs of the items socketed in stored item `id`.
    fn children(&self, conn: &Connection, id: u32) -> Vec<u32> {
        let Some(t) = &self.tables else {
            return Vec::new();
        };
        let record: Option<Vec<u8>> = conn
            .query_row(
                "SELECT record FROM econ_items WHERE item_id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        record
            .and_then(|r| read_item(&t.items, &r).ok())
            .map(|i| i.children)
            .unwrap_or_default()
    }
}

fn refused(e: d2_item_registry::Error) -> Refusal {
    match e {
        d2_item_registry::Error::Refused { why, .. } => {
            Refusal::from_code(why.transfer_code()).unwrap_or(Refusal::TransferInvalid)
        }
        _ => Refusal::TransferInvalid,
    }
}

impl d2_economy::ItemRegistry for RealmItems {
    fn arriving(
        &self,
        conn: &Connection,
        economy: &EconomyKey,
        id: ItemId,
        from: &CharRef,
    ) -> Result<(), Refusal> {
        let split = split_of_key(economy);
        self.registry
            .arriving(
                conn,
                split,
                id.0,
                &name_key(&from.name),
                from.game,
                now_ms(),
            )
            .map_err(refused)?;
        *self.arrived.lock().unwrap_or_else(|e| e.into_inner()) = Some((id.0, from.clone(), split));
        Ok(())
    }

    fn moved(&self, conn: &Connection, m: &Moved<'_>) -> Result<(), Refusal> {
        let (place, place_id) = match m.place {
            Place::Character => (IdPlace::Character, name_key(m.character.unwrap_or(""))),
            Place::InFlight => (IdPlace::InFlight, name_key(m.character.unwrap_or(""))),
            Place::Stash => (IdPlace::Stash, owner_id(m.owner)),
            Place::Escrow => (IdPlace::Escrow, owner_id(m.owner)),
            Place::Delivery => (IdPlace::Delivery, owner_id(m.owner)),
        };
        let now = now_ms();
        let arrived = self
            .arrived
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .filter(|(id, _, _)| *id == m.id.0);
        self.registry
            .moved(conn, m.id.0, place, &place_id, m.game, m.why, now)
            .map_err(refused)?;
        for child in self.children(conn, m.id.0) {
            // A socketed item put with its item arrives as it does;
            // afterwards it only follows its item.
            if let Some((_, from, split)) = &arrived {
                self.registry
                    .arriving(conn, *split, child, &name_key(&from.name), from.game, now)
                    .map_err(refused)?;
            }
            match self
                .registry
                .moved(conn, child, place, &place_id, m.game, m.why, now)
            {
                Ok(()) | Err(d2_item_registry::Error::Unregistered(_)) => {}
                Err(e) => return Err(refused(e)),
            }
        }
        Ok(())
    }
}

fn owner_id(owner: Option<i64>) -> String {
    format!("account {}", owner.unwrap_or(0))
}

/// What the main server reads from an item's bytes with its item tables.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReadFacts {
    /// The realm item ID (0: none).
    pub id: u32,
    /// The IDs of its socketed items.
    pub children: Vec<u32>,
    pub code: String,
    pub item_type: String,
    pub quality: u8,
    pub ilvl: u8,
    pub sockets: u8,
    pub ethereal: bool,
    /// Width and height in cells.
    pub size: (u8, u8),
    /// `(stat, value)` of its own stats.
    pub stats: Vec<(u16, i32)>,
    /// How many of its kind it is: its quantity for a kind the game
    /// stacks (keys), else 1.
    pub count: u32,
}

impl ReadFacts {
    /// A plain item: normal quality, nothing socketed, not ethereal (what
    /// the stash keeps as a count).
    pub fn plain(&self) -> bool {
        self.quality == NORMAL && self.children.is_empty() && !self.ethereal
    }
}

/// The save format's quality number of a normal item.
const NORMAL: u8 = 2;

fn code_text(code: [u8; 4]) -> String {
    String::from_utf8_lossy(&code).trim().to_owned()
}

/// Reads one item entry (an item and its socketed items, as a save holds
/// it) with the realm's item tables.
pub fn read_item(t: &ItemTables, bytes: &[u8]) -> Result<ReadFacts, String> {
    let e: ReadEntry = read_save_entry(bytes, t).map_err(|e| e.to_string())?;
    if e.item.failed {
        return Err("the item doesn't read".into());
    }
    let it = &e.item.item;
    let rec = t.item(e.item.record);
    let mut children = Vec::new();
    collect_ids(&e.children, &mut children);
    Ok(ReadFacts {
        id: if it.compact || it.alt { 0 } else { it.unit28 },
        children,
        code: code_text(it.code),
        item_type: t
            .itype_of(e.item.record)
            .map(|r| code_text(r.code))
            .unwrap_or_default(),
        quality: it.quality,
        ilvl: it.ilvl.clamp(0, 255) as u8,
        sockets: e.item.item.filled.min(255) as u8,
        ethereal: it.flags & hflag::ETHEREAL != 0,
        size: rec.map_or((0, 0), |r| (r.invwidth, r.invheight)),
        stats: it
            .main
            .as_ref()
            .map(|s| s.iter().map(|s| (s.stat, s.value)).collect())
            .unwrap_or_default(),
        count: if it.stackable {
            it.total_quantity.max(1) as u32
        } else {
            1
        },
    })
}

fn collect_ids(entries: &[ReadEntry], out: &mut Vec<u32>) {
    for c in entries {
        let it = &c.item.item;
        if !it.compact && !it.alt && it.unit28 != 0 {
            out.push(it.unit28);
        }
        collect_ids(&c.children, out);
    }
}
