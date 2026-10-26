//! The online shared stash (plan E1, modules N28a-c; contracts 11.13).
//!
//! The main server serves each account's stash (item pages, stackables,
//! gold; one per economy split) from the mod realm's economy
//! (`d2-economy`, on the [`Bank`] worker), and makes every move between a
//! character in a game and the stash with the character's game server:
//!
//! - **Put** (`StashPut`; `StashPutStacks` is one put per unit):
//!   `ItemWanted` asks the game to take the goods off the character; it sends them back with the character's save
//!   without them (`ItemOut`, the record on its stream). The economy
//!   stores the goods and that record in one transaction
//!   ([`d2_economy::Economy::put`]), and `ItemOutAck` answers. A refusal
//!   is answered too, and the game gives the goods back.
//! - **Take** (`StashTake`): the economy moves the goods into a transfer
//!   in flight to the character ([`d2_economy::Economy::take`]), and
//!   `ItemIn` hands them to the game. The game places them and sends the
//!   save holding them (`ItemInDone`, the record on its stream); the
//!   economy closes the transfer with that record
//!   ([`d2_economy::Economy::take_done`]), and `ItemOutAck` answers. No
//!   room, a refusal, or a seat lease that ends first: the goods go back
//!   to the stash.
//!
//! Every save that goes with a move passes the seat lease, the save
//! checks and the item registry as any other save does
//! ([`super::saves::SaveDesk`]). An account makes one move at a time (a
//! second one while one runs is `ItemHeld`). The records a move stores are
//! written to the character files once the economy committed, and at the
//! main server's start if it stopped before ([`store`]); a transfer still
//! in flight at the start is undone (seat leases never outlive the main
//! server).

pub mod cells;
pub mod settings;
pub mod store;
mod transfers;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use d2_accounts::AccountId;
use d2_economy::{
    CharRef, Economy, EconomyConfig, EconomyKey, Goods, Item, ItemFacts, ItemId, Owner, Req,
};
use d2_link::Lease;
use d2_net::msg::account::CharSummary;
use d2_net::msg::node::RealmToNode;
use d2_net::msg::stash::{goods, tab, StashCell, StashEvent, StashItem, StashRequest};
use d2_net::msg::stash::{StashShapeMsg, MAX_HITS, MAX_STACKS, MAX_UNITS};
use d2_net::msg::transfer::{ItemIn, ItemOut, ItemWanted};
use d2_net::msg::ErrorCode;
use d2_save_checks::CheckTables;
use d2_shared_stash::{Grid, StashShape, PAGE_NAME_MAX};
use tokio::sync::oneshot;

use super::guard::{RegistryMode, SaveGuard};
use super::saves::{LeaseEnded, LeaseGrant};
use super::Realm;
use crate::economy::Bank;
use crate::log::ServerLog;
use crate::session::ControlTx;
use cells::{Layout, Placement};
pub use settings::StashSettings;
use store::{Owners, ReadFacts, RealmItems, Records, Store};

/// How long a client's move waits for its game server before it is
/// answered; the move itself goes on and ends with the game's answer or
/// the seat lease.
pub const MOVE_WAIT: Duration = Duration::from_secs(30);

/// The stash's refusals (contracts 3.8; the economy's own are
/// [`d2_economy::Refusal::code`]).
pub mod code {
    /// `StashFull`.
    pub const STASH_FULL: u16 = 80;
    /// `NotInGame`: a move needs the character in a game.
    pub const NOT_IN_GAME: u16 = d2_net::msg::ErrorCode::NOT_IN_GAME.0;
    /// `ItemNotFound`.
    pub const ITEM_NOT_FOUND: u16 = 82;
    /// `ItemHeld`: another move of the account runs.
    pub const ITEM_HELD: u16 = 87;
    /// `NoRoom`: no room on the character for goods taken from the
    /// stash; they stay in the stash.
    pub const NO_ROOM: u16 = d2_net::msg::ErrorCode::NO_ROOM.0;
    /// `Other`: the server failed, or the game server didn't answer in
    /// [`super::MOVE_WAIT`] (the move may still end; the stash shows how).
    pub const OTHER: u16 = 99;
    /// `TransferInvalid`.
    pub const TRANSFER_INVALID: u16 = 109;
    /// `TransferUnknown`.
    pub const TRANSFER_UNKNOWN: u16 = 110;
}

/// A move's answer to the client (`StashDone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Answer {
    pub code: u16,
    /// What moved: 1 for an item, a stackable's count, the gold.
    pub count: u64,
}

impl Answer {
    pub fn refused(code: u16) -> Self {
        Self { code, count: 0 }
    }

    fn of(code: ErrorCode) -> Self {
        Self::refused(code.0)
    }
}

/// The session a stash request comes from.
#[derive(Clone, Debug)]
pub struct StashUser {
    pub account: AccountId,
    /// The selected character.
    pub character: Option<CharSummary>,
    pub out: ControlTx,
}

/// A move under way, by (lease, req).
#[derive(Debug)]
struct Move {
    account: i64,
    /// The character's stored name.
    character: String,
    owner: Owner,
    what: What,
    done: Option<oneshot::Sender<Answer>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum What {
    Put {
        cell: Option<StashCell>,
    },
    /// `count`: what the take moves (1 for an item, a stackable's count,
    /// the gold).
    Take {
        transfer: i64,
        count: u64,
    },
}

/// A move the game server's answer took over (see [`Stash::claim`]).
#[derive(Debug)]
struct Claimed {
    lease: u64,
    req: u64,
    mv: Move,
}

#[derive(Debug, Default)]
struct Moves {
    open: HashMap<(u64, u64), Move>,
    /// Accounts with a move under way.
    busy: HashSet<i64>,
}

/// The online stash of one main server.
#[derive(Debug)]
pub struct Stash {
    bank: Bank,
    store: Store,
    tables: Option<Arc<CheckTables>>,
    config: EconomyConfig,
    mod_id: String,
    /// The realm's game settings: its pages, slot cap and categories.
    settings: StashSettings,
    /// The pages of [`Stash::settings`].
    grid: Grid,
    owners: Owners,
    moves: Mutex<Moves>,
    next: AtomicU64,
    /// Characters whose stored record couldn't be written to their file.
    unwritten: Mutex<HashSet<String>>,
    /// Each account's last changing request (see [`Stash::request`]).
    last: Mutex<HashMap<i64, Last>>,
    log: Arc<ServerLog>,
}

/// An account's last changing request: its number, its answer once it
/// ended, and the sessions that asked it again meanwhile (a client that
/// came back on a new connection), which get the answer too.
#[derive(Debug)]
struct Last {
    req: u64,
    answer: Option<Answer>,
    again: Vec<ControlTx>,
}

/// The economy's request numbers of move `n`: the take or put, the take's
/// end, and its undo.
fn econ_req(lease: u64, n: u64, step: u64) -> Req {
    Req::lease(lease, n.wrapping_mul(4).wrapping_add(step))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn now_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Stash {
    /// Opens the economy of `mod_id` in the realm database `path`, with the
    /// guard's item registry (unless it is off) and the stash's records,
    /// on D2Gamma's pages ([`Stash::open_with`] for a mod realm's).
    pub fn open(
        path: &Path,
        mod_id: &str,
        guard: &SaveGuard,
        config: EconomyConfig,
        log: Arc<ServerLog>,
    ) -> Result<Arc<Self>, String> {
        Self::open_with(path, mod_id, guard, config, StashSettings::default(), log)
    }

    /// [`Stash::open`] with a mod realm's pages, slot cap and category
    /// pages.
    pub fn open_with(
        path: &Path,
        mod_id: &str,
        guard: &SaveGuard,
        config: EconomyConfig,
        settings: StashSettings,
        log: Arc<ServerLog>,
    ) -> Result<Arc<Self>, String> {
        let store = Store::open(path)?;
        let owners = Owners::default();
        let mut economy = Economy::open(path, mod_id, config.clone())
            .map_err(|e| e.to_string())?
            .with_records(Box::new(Records {
                owners: owners.clone(),
            }));
        if guard.config().registry != RegistryMode::Off {
            economy = economy.with_registry(Box::new(RealmItems::new(
                guard.registry().clone(),
                guard.tables().cloned(),
            )));
        }
        Ok(Arc::new(Self {
            bank: Bank::start(economy)?,
            store,
            tables: guard.tables().cloned(),
            config,
            mod_id: mod_id.to_owned(),
            grid: settings.grid(),
            settings,
            owners,
            moves: Mutex::new(Moves::default()),
            // Above every number an earlier run used under a lease that
            // could still be answered: none can (leases end with the run).
            next: AtomicU64::new(1),
            unwritten: Mutex::new(HashSet::new()),
            last: Mutex::new(HashMap::new()),
            log,
        }))
    }

    /// A changing request `req` of `account` from the session of `out`:
    /// `Ok` to run it; a repeat of the account's last one is `Err` with its
    /// answer, or `Err(None)` while it still runs: `out` then gets the
    /// answer when it ends, so a session that came back never waits on a
    /// move that answers only the one it left. Only the last is kept: a
    /// new session may number from 1 again.
    fn request(&self, account: i64, req: u64, out: &ControlTx) -> Result<(), Option<Answer>> {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        match last.get_mut(&account) {
            Some(l) if l.req == req => {
                if l.answer.is_none() {
                    l.again.push(out.clone());
                }
                Err(l.answer)
            }
            _ => {
                last.insert(
                    account,
                    Last {
                        req,
                        answer: None,
                        again: Vec::new(),
                    },
                );
                Ok(())
            }
        }
    }

    /// Request `req` of `account` ended with `a`: the sessions that asked
    /// it again hear it.
    fn answered(&self, account: i64, req: u64, a: Answer) {
        let again = {
            let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
            match last.get_mut(&account).filter(|l| l.req == req) {
                Some(l) => {
                    l.answer = Some(a);
                    std::mem::take(&mut l.again)
                }
                None => Vec::new(),
            }
        };
        for out in again {
            out.send(
                StashEvent::Done {
                    req,
                    code: a.code,
                    count: a.count,
                    left: Vec::new(),
                }
                .body(),
            );
        }
    }

    /// The economy worker (the market's moves go on it too).
    pub fn bank(&self) -> &Bank {
        &self.bank
    }

    fn moves(&self) -> MutexGuard<'_, Moves> {
        self.moves.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The account's goods in the economy of a character with `status`.
    pub fn owner(&self, account: AccountId, status: u16) -> Owner {
        use d2_formats::d2s::status as st;
        Owner::new(
            account.0,
            EconomyKey {
                mod_id: self.mod_id.clone(),
                hardcore: status & st::HARDCORE != 0,
                // Ladder seasons don't run yet.
                ladder: 0,
                expansion: status & st::EXPANSION != 0,
            },
        )
    }

    /// Starts a move of `account` under `lease`: its number and where its
    /// answer comes. `None`: the account has a move under way.
    fn begin(
        &self,
        lease: u64,
        account: i64,
        character: &str,
        owner: &Owner,
        what: What,
    ) -> Option<(u64, oneshot::Receiver<Answer>)> {
        let mut m = self.moves();
        if !m.busy.insert(account) {
            return None;
        }
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        m.open.insert(
            (lease, n),
            Move {
                account,
                character: character.to_owned(),
                owner: owner.clone(),
                what,
                done: Some(tx),
            },
        );
        drop(m);
        self.owners.set(lease, account);
        Some((n, rx))
    }

    fn set_transfer(&self, lease: u64, n: u64, transfer: i64) {
        if let Some(What::Take { transfer: t, .. }) =
            self.moves().open.get_mut(&(lease, n)).map(|m| &mut m.what)
        {
            *t = transfer;
        }
    }

    /// The game server answered move (lease, req): the answer's handling
    /// owns it now (a lease that ends meanwhile doesn't end it).
    fn claim(&self, lease: u64, req: u64) -> Option<Claimed> {
        let mv = self.moves().open.remove(&(lease, req))?;
        Some(Claimed { lease, req, mv })
    }

    /// A claimed move ended.
    fn finish(&self, mut c: Claimed, a: Answer) {
        self.moves().busy.remove(&c.mv.account);
        if let Some(tx) = c.mv.done.take() {
            let _ = tx.send(a);
        }
        if a.code != 0 {
            self.log.line(&format!(
                "stash: move {} of lease {} (`{}`) refused: {}",
                c.req, c.lease, c.mv.character, a.code
            ));
        }
    }

    /// Ends move (lease, req) now, if it is still open.
    fn end(&self, lease: u64, req: u64, a: Answer) {
        if let Some(c) = self.claim(lease, req) {
            self.finish(c, a);
        }
    }

    /// A seat lease ended: its open moves end (`LeaseRevoked`), and every
    /// transfer in flight under it goes back to the stash.
    pub fn lease_ended(&self, lease: Lease) {
        let ended: Vec<u64> = self
            .moves()
            .open
            .keys()
            .filter(|(l, _)| *l == lease.0)
            .map(|(_, n)| *n)
            .collect();
        for n in ended {
            self.end(lease.0, n, Answer::of(ErrorCode::LEASE_REVOKED));
        }
        self.owners.ended(lease.0);
        let log = self.log.clone();
        self.bank
            .spawn(move |e| match e.undo_lease(lease.0, now_ms()) {
                Ok(0) => {}
                Ok(n) => log.line(&format!("stash: {lease} ended: {n} transfers undone")),
                Err(e) => log.line(&format!("stash: {lease} ended: undoing its transfers: {e}")),
            });
    }

    /// The main server starts: the records a former run's moves stored
    /// are written to their files, and its transfers still in flight go
    /// back to the stash.
    pub async fn recover(&self, accounts: &d2_accounts::Accounts) -> Result<(), String> {
        for r in self.store.records()? {
            match accounts
                .store_record(AccountId(r.account), &r.name, r.record.clone(), now_s())
                .await
            {
                Ok(_) => {
                    self.store.record_written(&r)?;
                    self.log.line(&format!(
                        "stash: `{}`'s record of a former run's move written",
                        r.name
                    ));
                }
                Err(d) => self.log.line(&format!(
                    "stash: `{}`'s record of a former run's move: {}",
                    r.name, d.text
                )),
            }
        }
        for lease in self.store.leases_in_flight()? {
            let n = self
                .bank
                .run(move |e| e.undo_lease(lease, now_ms()))
                .await?
                .map_err(|e| e.to_string())?;
            self.log.line(&format!(
                "stash: lease {lease} of a former run: {n} transfers undone"
            ));
        }
        Ok(())
    }

    /// A save of `character` is in its file: a record a move couldn't
    /// write is out of date now.
    pub fn saved(&self, character: &str) {
        let key = d2_accounts::rules::name_key(character);
        let was = self
            .unwritten
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
        if was {
            if let Err(e) = self.store.drop_record(&key) {
                self.log.line(&format!("stash: {e}"));
            }
        }
    }

    /// The item an `ItemOut` carries: its ID, bytes and facts, and its
    /// size. Read with the realm's item tables when it has them (the ID
    /// must be the item's own); the game server's sketch fills what they
    /// don't say.
    fn item_of(&self, o: &ItemOut) -> Result<(Item, (u8, u8)), String> {
        let id = o.item_id.ok_or("an item without a realm item ID")?;
        let read = match &self.tables {
            Some(t) => {
                let r = store::read_item(&t.items, &o.item)?;
                if r.id != id {
                    return Err(format!("the item's ID is {:#010x}, not {id:#010x}", r.id));
                }
                Some(r)
            }
            None => None,
        };
        let sketch = o.sketch.as_ref();
        let (facts, size) = match (read, sketch) {
            (Some(r), s) => {
                let ReadFacts {
                    code,
                    item_type,
                    quality,
                    ilvl,
                    sockets,
                    ethereal,
                    size,
                    stats,
                    ..
                } = r;
                let size = if size.0 > 0 && size.1 > 0 {
                    size
                } else {
                    s.map_or((0, 0), |s| s.size)
                };
                let facts = ItemFacts {
                    name: code.clone(),
                    name_parts: vec![code.clone()],
                    code,
                    item_type: if item_type.is_empty() {
                        s.map(|s| s.item_type.clone()).unwrap_or_default()
                    } else {
                        item_type
                    },
                    quality,
                    level_req: s.map_or(0, |s| s.level_req),
                    ilvl,
                    sockets: s.map_or(sockets, |s| s.sockets),
                    ethereal,
                    class: s.and_then(|s| s.class),
                    stats,
                    // The market's group and type-tree facts: the market's realm wiring (K1c) reads them.
                    runeword: false,
                    identified: true,
                    types: Vec::new(),
                };
                (facts, size)
            }
            (None, Some(s)) => (
                ItemFacts {
                    name: s.code.clone(),
                    name_parts: vec![s.code.clone()],
                    code: s.code.clone(),
                    item_type: s.item_type.clone(),
                    quality: s.quality,
                    level_req: s.level_req,
                    ilvl: s.ilvl,
                    sockets: s.sockets,
                    ethereal: s.ethereal,
                    class: s.class,
                    stats: Vec::new(),
                    runeword: false,
                    identified: true,
                    types: Vec::new(),
                },
                s.size,
            ),
            (None, None) => return Err("an item without its sketch".into()),
        };
        if !(1..=4).contains(&size.0) || !(1..=4).contains(&size.1) {
            return Err(format!("an item of size {size:?}"));
        }
        Ok((
            Item {
                id: ItemId(id),
                record: o.item.clone(),
                facts,
            },
            size,
        ))
    }

    /// The plain item an `ItemOut` carries for the stash to keep as a
    /// count: its code, count and realm item ID (0: none). A listed
    /// stackable, within the slot's cap; read with the realm's item tables
    /// when it has them (the bytes must be that plain item).
    fn stack_of(&self, o: &ItemOut) -> Result<(String, u32, u32), String> {
        let (code, count) = o.stack.clone().ok_or("no stackable named")?;
        if self.config.stackable(&code).is_none() {
            return Err(format!("`{code}` is no stackable of this realm"));
        }
        if count == 0 || count > self.settings.cap {
            return Err(format!("a stack of {count}"));
        }
        let id = match &self.tables {
            Some(t) => {
                let r = store::read_item(&t.items, &o.item)?;
                if r.code != code || r.count != count || !r.plain() {
                    return Err(format!(
                        "the item is {} `{}` x{}, not plain `{code}` x{count}",
                        if r.plain() { "plain" } else { "not plain" },
                        r.code,
                        r.count
                    ));
                }
                if o.item_id.is_some_and(|i| i != r.id) {
                    return Err(format!("the item's ID is {:#010x}", r.id));
                }
                r.id
            }
            None => o.item_id.unwrap_or(0),
        };
        Ok((code, count, id))
    }

    /// `StashOpen`: the page's header, then its items.
    fn open_page(&self, owner: &Owner, tab_: u8, page: u16) -> Result<Vec<StashEvent>, String> {
        let (account, split) = (owner.account.0, owner.economy.split());
        let amounts = self.store.amounts(account, &split)?;
        let mut stacks = amounts.stacks;
        stacks.truncate(MAX_STACKS);
        let items = if tab_ == tab::ITEMS && self.settings.holds_page(page) {
            let layout = self.store.layout(account, &split, &self.grid)?;
            self.store.page(&layout, page as u8)?
        } else {
            Vec::new()
        };
        let mut out = vec![StashEvent::Page {
            tab: tab_,
            page,
            gold: amounts.gold,
            stacks,
            items: items.len() as u16,
        }];
        out.extend(items.into_iter().map(|i| {
            StashEvent::Item(StashItem {
                id: i.id,
                cell: i.at.cell,
                size: i.at.size,
                record: i.record,
            })
        }));
        Ok(out)
    }

    /// `StashSearch`: the items whose names hold `text`, with their cells.
    fn search(&self, owner: &Owner, text: &str) -> Result<StashEvent, String> {
        let (account, split) = (owner.account.0, owner.economy.split());
        let layout = self.store.layout(account, &split, &self.grid)?;
        let ids = self.store.search(account, &split, text.trim(), MAX_HITS)?;
        let hits = ids
            .into_iter()
            .filter_map(|id| layout.by_item.get(&id).map(|p| (id, p.cell)))
            .collect();
        Ok(StashEvent::Found { hits })
    }

    /// `StashMove`: an item to another free cell.
    fn move_item(&self, owner: &Owner, item: u32, cell: StashCell) -> Result<Answer, String> {
        let (account, split) = (owner.account.0, owner.economy.split());
        let layout = self.store.layout(account, &split, &self.grid)?;
        let Some(p) = layout.by_item.get(&item).copied() else {
            return Ok(Answer::refused(code::ITEM_NOT_FOUND));
        };
        if !self.store.in_stash(item, account, &split)? {
            return Ok(Answer::refused(code::ITEM_NOT_FOUND));
        }
        let to = Placement { cell, size: p.size };
        if !layout.is_free(&to, Some(item)) {
            return Ok(Answer::refused(code::STASH_FULL));
        }
        self.store.set_cell(item, account, &split, &to)?;
        Ok(Answer { code: 0, count: 1 })
    }

    /// `StashArrange`: packs one item page.
    fn arrange(&self, owner: &Owner, page: u8) -> Result<Answer, String> {
        let (account, split) = (owner.account.0, owner.economy.split());
        if !self.settings.holds_page(page.into()) {
            return Ok(Answer::refused(ErrorCode::OTHER.0));
        }
        let layout: Layout = self.store.layout(account, &split, &self.grid)?;
        let packed = layout.arranged(page, |id| id);
        self.store.set_cells(account, &split, &packed)?;
        Ok(Answer { code: 0, count: 0 })
    }

    /// The realm's game settings.
    pub fn settings(&self) -> &StashSettings {
        &self.settings
    }

    /// `StashLayout`: the item pages' shape, the slot cap and the
    /// account's page names (the default roman numeral where it gave
    /// none).
    fn shape(&self, owner: &Owner) -> Result<StashEvent, String> {
        let (account, split) = (owner.account.0, owner.economy.split());
        let mut named = self.store.page_names(account, &split)?;
        let shape = self.settings.shape;
        Ok(StashEvent::Shape(StashShapeMsg {
            pages: shape.pages,
            cols: shape.cols,
            rows: shape.rows,
            renamable: shape.renamable,
            cap: self.settings.cap,
            names: (0..shape.pages)
                .map(|p| {
                    named
                        .remove(&p)
                        .unwrap_or_else(|| StashShape::default_name(p))
                })
                .collect(),
        }))
    }

    /// `StashRename`: names item page `page` (trimmed, at most
    /// [`PAGE_NAME_MAX`] characters; empty: back to the default). Refused
    /// unless the realm's pages are renamable.
    fn rename(&self, owner: &Owner, page: u8, name: &str) -> Result<Answer, String> {
        let name = name.trim();
        let shape = self.settings.shape;
        if !shape.renamable
            || page >= shape.pages
            || name.chars().count() > PAGE_NAME_MAX
            || name.chars().any(char::is_control)
        {
            return Ok(Answer::refused(d2_economy::Refusal::BadRequest.code()));
        }
        let (account, split) = (owner.account.0, owner.economy.split());
        self.store.set_page_name(account, &split, page, name)?;
        Ok(Answer { code: 0, count: 0 })
    }

    /// Runs a store call off the network threads.
    async fn blocking<T, F>(self: &Arc<Self>, f: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&Stash) -> Result<T, String> + Send + 'static,
    {
        let me = self.clone();
        tokio::task::spawn_blocking(move || f(&me))
            .await
            .unwrap_or_else(|e| Err(format!("task failed: {e}")))
    }
}

/// A character's seat: its lease and who holds it.
struct Seat {
    lease: Lease,
    grant: LeaseGrant,
}

impl Realm {
    /// Serves the online stash (`Realm::stash_request`); its moves end
    /// with every seat lease that ends.
    pub fn with_stash(mut self, stash: Arc<Stash>) -> Self {
        let s = stash.clone();
        self.saves
            .on_lease_end(LeaseEnded(Arc::new(move |lease| s.lease_ended(lease))));
        self.stash = Some(stash);
        self
    }

    /// The online stash, when served.
    pub fn stash(&self) -> Option<&Arc<Stash>> {
        self.stash.as_ref()
    }

    /// The live seat of `account`'s character `name`.
    fn seat_of(&self, account: AccountId, name: &str) -> Option<Seat> {
        let t = self.saves.leases();
        let lease = t.of_character(name)?;
        let grant = t.get(lease).filter(|g| g.account == account)?.clone();
        Some(Seat { lease, grant })
    }

    /// One stash request of a session (kinds 0xA0-0xAF). Every changing
    /// request is answered with `StashDone`.
    pub async fn stash_request(self: Arc<Self>, who: StashUser, req: StashRequest) {
        let k = req.kind();
        let Some(stash) = self.stash.clone() else {
            who.out.error(k, ErrorCode::NOT_SUPPORTED, 0, "");
            return;
        };
        let Some(ch) = who.character.clone() else {
            who.out.error(k, ErrorCode::NO_CHAR_SELECTED, 0, "");
            return;
        };
        let owner = stash.owner(who.account, ch.status);
        let send = |req: u64, a: Answer, left: Vec<(u32, u16)>| {
            who.out.send(
                StashEvent::Done {
                    req,
                    code: a.code,
                    count: a.count,
                    left,
                }
                .body(),
            );
        };
        if let Some(r) = req.req() {
            match stash.request(who.account.0, r, &who.out) {
                Ok(()) => {}
                Err(Some(a)) => return send(r, a, Vec::new()),
                Err(None) => return,
            }
        }
        let done = |req: u64, a: Answer| {
            stash.answered(who.account.0, req, a);
            send(req, a, Vec::new());
        };
        let failed = |e: String| {
            self.log.line(&format!("stash: {e}"));
            Answer::refused(code::OTHER)
        };
        match req {
            StashRequest::Open { tab: t, page } => {
                let o = owner.clone();
                match stash.blocking(move |s| s.open_page(&o, t, page)).await {
                    Ok(events) => {
                        for e in events {
                            who.out.send(e.body());
                        }
                    }
                    Err(e) => {
                        failed(e);
                        who.out.error(k, ErrorCode::OTHER, 0, "");
                    }
                }
            }
            StashRequest::Layout => {
                let o = owner.clone();
                match stash.blocking(move |s| s.shape(&o)).await {
                    Ok(shape) => who.out.send(shape.body()),
                    Err(e) => {
                        failed(e);
                        who.out.error(k, ErrorCode::OTHER, 0, "");
                    }
                }
            }
            StashRequest::Rename { req, page, name } => {
                let o = owner.clone();
                let a = stash
                    .blocking(move |s| s.rename(&o, page, &name))
                    .await
                    .unwrap_or_else(failed);
                done(req, a);
            }
            StashRequest::Search { text } => {
                let o = owner.clone();
                match stash.blocking(move |s| s.search(&o, &text)).await {
                    Ok(found) => who.out.send(found.body()),
                    Err(e) => {
                        failed(e);
                        who.out.error(k, ErrorCode::OTHER, 0, "");
                    }
                }
            }
            StashRequest::Move { req, item, cell } => {
                let a = self
                    .layout_change(&stash, &owner, move |s, o| s.move_item(o, item, cell))
                    .await
                    .unwrap_or_else(failed);
                done(req, a);
            }
            StashRequest::Arrange { req, page } => {
                let a = self
                    .layout_change(&stash, &owner, move |s, o| s.arrange(o, page))
                    .await
                    .unwrap_or_else(failed);
                done(req, a);
            }
            StashRequest::PutStacks { req, units } => {
                let (a, left) = self
                    .stash_put_stacks(&stash, who.account, &ch, owner, units)
                    .await;
                stash.answered(who.account.0, req, a);
                send(req, a, left);
            }
            StashRequest::Put {
                req,
                unit,
                to,
                gold,
                cell,
            } => {
                let a = self
                    .stash_put(&stash, who.account, &ch, owner, unit, to, gold, cell)
                    .await;
                done(req, a);
            }
            StashRequest::Take {
                req,
                from,
                item,
                stack,
                gold,
                place,
            } => {
                let what = match (from, item, stack) {
                    (goods::ITEM, Some(id), None) => Some(Goods::Item(ItemId(id))),
                    (goods::STACK, None, Some((code, count))) if count > 0 => {
                        Some(Goods::Stack { code, count })
                    }
                    (goods::GOLD, None, None) if gold > 0 => Some(Goods::Gold(gold)),
                    _ => None,
                };
                let a = match what {
                    Some(what) => {
                        self.stash_take(&stash, who.account, &ch, owner, what, place)
                            .await
                    }
                    None => Answer::of(ErrorCode::OTHER),
                };
                done(req, a);
            }
        }
    }

    /// A layout change (`StashMove`, `StashArrange`): not while a move of
    /// the account runs.
    async fn layout_change<F>(
        &self,
        stash: &Arc<Stash>,
        owner: &Owner,
        f: F,
    ) -> Result<Answer, String>
    where
        F: FnOnce(&Stash, &Owner) -> Result<Answer, String> + Send + 'static,
    {
        let account = owner.account.0;
        if !stash.moves().busy.insert(account) {
            return Ok(Answer::refused(code::ITEM_HELD));
        }
        let o = owner.clone();
        let r = stash.blocking(move |s| f(s, &o)).await;
        stash.moves().busy.remove(&account);
        r
    }

    /// `StashPut`: asks the character's game for the goods; the move ends
    /// with its `ItemOut` ([`Realm::item_out`]) or `ItemOutRefused`.
    #[allow(clippy::too_many_arguments)]
    async fn stash_put(
        &self,
        stash: &Arc<Stash>,
        account: AccountId,
        ch: &CharSummary,
        owner: Owner,
        unit: u32,
        to: u8,
        gold: u64,
        cell: Option<StashCell>,
    ) -> Answer {
        match to {
            goods::ITEM | goods::STACK if unit != 0 => {}
            goods::GOLD if gold > 0 => {}
            _ => return Answer::of(ErrorCode::OTHER),
        }
        let Some(seat) = self.seat_of(account, &ch.name) else {
            return Answer::refused(code::NOT_IN_GAME);
        };
        // What the economy would refuse is refused before the goods leave
        // the character.
        let (acc, split) = (owner.account.0, owner.economy.split());
        let amounts = match stash.blocking(move |s| s.store.amounts(acc, &split)).await {
            Ok(a) => a,
            Err(e) => {
                self.log.line(&format!("stash: {e}"));
                return Answer::refused(code::OTHER);
            }
        };
        let full = match to {
            goods::ITEM => amounts.items >= u64::from(stash.config.stash_items),
            // A stack's cap is checked once its code and count come.
            goods::STACK => false,
            _ => amounts.gold.saturating_add(gold) > stash.config.stash_gold_max,
        };
        if full {
            return Answer::refused(code::STASH_FULL);
        }
        let Some(node) = self.nodes.get(&seat.grant.node) else {
            return Answer::of(ErrorCode::NODE_UNAVAILABLE);
        };
        let Some((n, rx)) = stash.begin(
            seat.lease.0,
            acc,
            &seat.grant.character,
            &owner,
            What::Put { cell },
        ) else {
            return Answer::refused(code::ITEM_HELD);
        };
        let sent = node.tx.send_msg(&RealmToNode::ItemWanted(ItemWanted {
            game: seat.grant.game,
            lease: seat.lease.0,
            req: n,
            unit,
            to,
            count: 0,
            gold: if to == goods::GOLD { gold } else { 0 },
        }));
        if sent.is_err() {
            stash.end(seat.lease.0, n, Answer::of(ErrorCode::NODE_UNAVAILABLE));
        }
        wait(rx).await
    }

    /// `StashPutStacks`: each unit put as a stackable, one move after the
    /// other; the answer counts the units stored, and `left` names the
    /// ones that stay on the character with why. A unit that isn't a plain
    /// item of a listed kind never leaves its place.
    async fn stash_put_stacks(
        &self,
        stash: &Arc<Stash>,
        account: AccountId,
        ch: &CharSummary,
        owner: Owner,
        units: Vec<u32>,
    ) -> (Answer, Vec<(u32, u16)>) {
        let mut seen = HashSet::new();
        let mut left = Vec::new();
        let mut moved = 0;
        let mut gone = None;
        for unit in units.into_iter().take(MAX_UNITS) {
            if let Some(code) = gone {
                left.push((unit, code));
                continue;
            }
            if unit == 0 || !seen.insert(unit) {
                left.push((unit, ErrorCode::OTHER.0));
                continue;
            }
            let a = self
                .stash_put(
                    stash,
                    account,
                    ch,
                    owner.clone(),
                    unit,
                    goods::STACK,
                    0,
                    None,
                )
                .await;
            if a.code == 0 {
                moved += 1;
                continue;
            }
            left.push((unit, a.code));
            // No seat or no game server: the rest can't move either.
            if a.code == code::NOT_IN_GAME || a.code == ErrorCode::NODE_UNAVAILABLE.0 {
                gone = Some(a.code);
            }
        }
        (
            Answer {
                code: 0,
                count: moved,
            },
            left,
        )
    }

    /// `StashTake`: moves the goods into a transfer in flight to the
    /// character and hands them to its game; the move ends with its
    /// `ItemInDone` ([`Realm::item_in_done`]).
    async fn stash_take(
        &self,
        stash: &Arc<Stash>,
        account: AccountId,
        ch: &CharSummary,
        owner: Owner,
        what: Goods,
        place: u8,
    ) -> Answer {
        let Some(seat) = self.seat_of(account, &ch.name) else {
            return Answer::refused(code::NOT_IN_GAME);
        };
        let Some(node) = self.nodes.get(&seat.grant.node) else {
            return Answer::of(ErrorCode::NODE_UNAVAILABLE);
        };
        let lease = seat.lease.0;
        // A stackable: no more than the stash holds.
        let what = match what {
            Goods::Stack { code, count } => {
                let (acc, split) = (owner.account.0, owner.economy.split());
                let held = match stash.blocking(move |s| s.store.amounts(acc, &split)).await {
                    Ok(a) => a
                        .stacks
                        .into_iter()
                        .find(|(c, _)| *c == code)
                        .map_or(0, |(_, n)| n),
                    Err(e) => {
                        self.log.line(&format!("stash: {e}"));
                        return Answer::refused(code::OTHER);
                    }
                };
                if held == 0 {
                    return Answer::refused(code::ITEM_NOT_FOUND);
                }
                Goods::Stack {
                    code,
                    count: count.min(held),
                }
            }
            w => w,
        };
        let count = match &what {
            Goods::Item(_) => 1,
            Goods::Stack { count, .. } => u64::from(*count),
            Goods::Gold(g) => *g,
        };
        let Some((n, rx)) = stash.begin(
            lease,
            owner.account.0,
            &seat.grant.character,
            &owner,
            What::Take { transfer: 0, count },
        ) else {
            return Answer::refused(code::ITEM_HELD);
        };
        let to = CharRef {
            name: seat.grant.character.clone(),
            game: seat.grant.game,
            lease,
        };
        let req = econ_req(lease, n, 0);
        let taken = stash
            .bank
            .run(move |e| {
                let d = e.take(&owner, &req, &to, what, now_ms())?;
                let t = d.id.ok_or(d2_economy::Refusal::TransferUnknown)?;
                Ok::<_, d2_economy::Error>(e.in_flight(t)?.map(|f| (t, f)))
            })
            .await;
        let flight = match taken {
            Ok(Ok(Some(f))) => f,
            Ok(Ok(None)) => {
                stash.end(lease, n, Answer::refused(code::TRANSFER_UNKNOWN));
                return wait(rx).await;
            }
            Ok(Err(e)) => {
                let a = match e.refusal() {
                    Some(r) => Answer::refused(r.code()),
                    None => {
                        self.log.line(&format!("stash: take: {e}"));
                        Answer::refused(code::OTHER)
                    }
                };
                stash.end(lease, n, a);
                return wait(rx).await;
            }
            Err(e) => {
                self.log.line(&format!("stash: take: {e}"));
                stash.end(lease, n, Answer::refused(code::OTHER));
                return wait(rx).await;
            }
        };
        let (transfer, flight) = flight;
        stash.set_transfer(lease, n, transfer);
        let (from, item_id, stack, gold, item) = match (&flight.goods, flight.item) {
            (Goods::Item(id), Some(item)) => (goods::ITEM, Some(id.0), None, 0, item.record),
            (Goods::Stack { code, count }, _) => (
                goods::STACK,
                None,
                Some((code.clone(), *count)),
                0,
                Vec::new(),
            ),
            (Goods::Gold(g), _) => (goods::GOLD, None, None, *g, Vec::new()),
            _ => (0, None, None, 0, Vec::new()),
        };
        let sent = from != 0
            && node
                .tx
                .send_msg(&RealmToNode::ItemIn(ItemIn {
                    game: seat.grant.game,
                    lease,
                    req: n,
                    from,
                    item_id,
                    stack,
                    gold,
                    ids: None,
                    place,
                    item,
                }))
                .is_ok();
        if !sent {
            // Never handed over: the goods go back at once.
            if let Some(c) = stash.claim(lease, n) {
                self.take_back(stash, &c, transfer).await;
                stash.finish(c, Answer::of(ErrorCode::NODE_UNAVAILABLE));
            }
        }
        wait(rx).await
    }
}

/// A move's answer, or `Other` when the game server takes longer than
/// [`MOVE_WAIT`].
async fn wait(rx: oneshot::Receiver<Answer>) -> Answer {
    match tokio::time::timeout(MOVE_WAIT, rx).await {
        Ok(Ok(a)) => a,
        _ => Answer::refused(code::OTHER),
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
