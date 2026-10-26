// D2RUST's own: the online shared stash in the running game.
//! The account's shared stash on the main server, for a game on a game
//! server (plan N28a): the panel ([`crate::ui::shared_stash`]) and the
//! inventory's "Send stackables to stash" button queue their asks as in
//! single player, and they go to the main server as stash requests
//! ([`d2_shared_stash::OnlineStash`] over the session). The main server
//! moves the goods with the game server; the client only asks and shows
//! the answer.
//!
//! A move waits for the game server, so the requests run on a worker
//! thread (contracts 10.1: no blocking call on the game's thread): the
//! frame system hands it the queued asks and shows its answer when it
//! comes, one batch at a time. The worker holds the session only while it
//! reads ([`SessionLink`]): a move waiting for its game server never holds
//! up a reconnect or the way back to the lobby.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bevy::log::warn;
use bevy::prelude::{
    resource_exists, App, IntoScheduleConfigs, Local, NonSendMut, Res, ResMut, Resource, Update,
};
use d2_net::msg::stash::{StashCell, StashItem, StashRequest};
use d2_online::{OnlineError, Session, StashAnswer, StashWait, STASH_WAIT};
use d2_shared_stash::{
    Catalog, Cell, FromRealm, ItemId, OnlineStash, PageItem, Put, RealmItem, RealmStash,
    StashShape, StashStore, StoreError, Take, TakeTo, ToRealm,
};
use d2_sim::items::bitstream::{self, read::read_save_entry};
use d2_sim::items::ItemTables;

use super::online::OnlineSession;
use super::shared_stash::{left_line, refusal_line, NO_SLOT};
use crate::bridge::items;
use crate::bridge::mirror::BridgeResource;
use crate::bridge::world::ClientWorld;
use crate::market::holdings::SharedBook;
use crate::ui::original::OriginalUi;
use crate::ui::shared_stash::{StashAsk, StashView};
use crate::world_view::WorldViewUi;

/// How long the worker reads the session at a time, and rests between
/// two reads (the session is free then).
const READ_SLICE: Duration = Duration::from_millis(2);
const READ_REST: Duration = Duration::from_millis(8);

/// The main-server session as the online store's link. A request's answer
/// is read in short slices, the session's lock taken for each: whatever
/// else needs the session (the connection watch, a reconnect, the lobby)
/// gets it between two. `stop`: the game ended, the request is given up.
pub struct SessionLink {
    pub session: OnlineSession,
    pub stop: Arc<AtomicBool>,
}

impl SessionLink {
    pub fn new(session: OnlineSession) -> Self {
        SessionLink {
            session,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Runs `f` on the session while holding it.
    fn with<T>(
        &self,
        f: impl FnOnce(&mut Session) -> Result<T, OnlineError>,
    ) -> Result<T, StoreError> {
        let lost = || StoreError::Link("no connection to the main server".into());
        let mut slot = self.session.0.lock().map_err(|_| lost())?;
        let session = slot.as_mut().ok_or_else(lost)?;
        f(session).map_err(|e| StoreError::Link(e.message()))
    }
}

impl RealmStash for SessionLink {
    fn call(&mut self, msg: ToRealm) -> Result<FromRealm, StoreError> {
        let req = to_wire(&msg);
        let mut wait: StashWait = self.with(|s| s.stash_start(&req))?;
        let deadline = Instant::now() + STASH_WAIT;
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return Err(StoreError::Link("the game ended".into()));
            }
            if let Some(answer) = self.with(|s| wait.step(s, READ_SLICE))? {
                return Ok(from_wire(req.req(), answer));
            }
            if Instant::now() >= deadline {
                return Err(StoreError::Link(OnlineError::TimedOut.message()));
            }
            std::thread::sleep(READ_REST);
        }
    }
}

fn cell_to_wire(c: Cell) -> StashCell {
    StashCell {
        page: c.page,
        x: c.x,
        y: c.y,
    }
}

fn cell_of_wire(c: StashCell) -> Cell {
    Cell::new(c.page, c.x, c.y)
}

/// The store's request as the wire's (contracts 11.13).
fn to_wire(msg: &ToRealm) -> StashRequest {
    match msg.clone() {
        ToRealm::Open { tab, page } => StashRequest::Open { tab, page },
        ToRealm::Put {
            req,
            unit,
            to,
            gold,
            cell,
        } => StashRequest::Put {
            req,
            unit,
            to,
            gold,
            cell: cell.map(cell_to_wire),
        },
        ToRealm::PutStacks { req, units } => StashRequest::PutStacks { req, units },
        ToRealm::Take {
            req,
            from,
            item,
            stack,
            gold,
            place,
        } => StashRequest::Take {
            req,
            from,
            item: item.map(|i| i.0),
            stack,
            gold,
            place,
        },
        ToRealm::Move { req, item, cell } => StashRequest::Move {
            req,
            item: item.0,
            cell: cell_to_wire(cell),
        },
        ToRealm::Arrange { req, page } => StashRequest::Arrange { req, page },
        ToRealm::Search { text } => StashRequest::Search { text },
        ToRealm::Rename { req, page, name } => StashRequest::Rename { req, page, name },
        ToRealm::Layout => StashRequest::Layout,
    }
}

/// The wire's answer to request number `req` as the store's.
fn from_wire(req: Option<u64>, a: StashAnswer) -> FromRealm {
    match a {
        StashAnswer::Page {
            tab,
            page,
            gold,
            stacks,
            items,
        } => FromRealm::Page {
            tab,
            page,
            items: items
                .into_iter()
                .map(|i: StashItem| RealmItem {
                    id: ItemId(i.id),
                    cell: cell_of_wire(i.cell),
                    size: i.size,
                    record: i.record,
                })
                .collect(),
            stacks,
            gold,
        },
        StashAnswer::Done { code, count, left } => FromRealm::Done {
            req: req.unwrap_or_default(),
            code,
            count,
            left,
        },
        StashAnswer::Found { hits } => FromRealm::Found {
            hits: hits
                .into_iter()
                .map(|(id, c)| (ItemId(id), cell_of_wire(c)))
                .collect(),
        },
        StashAnswer::Shape(s) => FromRealm::Shape {
            shape: StashShape {
                pages: s.pages,
                cols: s.cols,
                rows: s.rows,
                renamable: s.renamable,
            },
            cap: s.cap,
            names: s.names,
        },
    }
}

/// How the game client shows a stash item: its save record as the
/// network stream the panel's picture and tip read. `None`: the record
/// doesn't read (the panel then shows the code).
pub fn look_of(record: &[u8], tables: &ItemTables) -> Option<Vec<u8>> {
    let entry = read_save_entry(record, tables).ok()?;
    if entry.item.failed {
        return None;
    }
    bitstream::write(&entry.item.item, &tables.isc)
        .ok()
        .map(|(b, _)| b)
}

/// One ask of the panel with the gold it needs, read from the game the
/// frame it was made: the character's gold for a gold put, the room for
/// gold for a gold take. `room`: for a slot's take, how many fit in the
/// inventory as the client counts it ([`stack_room`]); `None` when it
/// can't tell (the game server alone decides then).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OnlineAsk {
    pub ask: StashAsk,
    pub gold: u64,
    pub room: Option<u64>,
}

/// What the worker does: the asks in order, then reads the view.
struct Job {
    asks: Vec<OnlineAsk>,
    page: Option<u8>,
    search: String,
    tables: Option<Arc<ItemTables>>,
}

/// The worker's answer: a line per ask that has one, and the view.
struct Answer {
    notes: Vec<String>,
    view: Result<StashView, StoreError>,
    /// The item page the view read.
    page: Option<u8>,
}

/// Runs one ask on the online store. Returns the line to show, if any.
pub fn run_ask<L: RealmStash>(
    store: &mut OnlineStash<L>,
    a: &OnlineAsk,
) -> Result<Option<String>, StoreError> {
    match a.ask.clone() {
        StashAsk::Refresh | StashAsk::Search { .. } => Ok(None),
        StashAsk::PutItem { guid, page, at } => {
            let (x, y) = at.unwrap_or((0, 0));
            store
                .put(Put::Item {
                    unit: guid,
                    at: Some(Cell::new(page, x, y)),
                })
                .map(|_| None)
        }
        StashAsk::PutStack { guid } => store
            .put(Put::Stacks { units: vec![guid] })
            .map(|d| left_line(&d, true)),
        StashAsk::SendStackables { guids } => {
            if guids.is_empty() {
                return Ok(None);
            }
            store
                .put(Put::Stacks { units: guids })
                .map(|d| left_line(&d, false))
        }
        StashAsk::TakeItem { id, to_cursor } => {
            let to = if to_cursor {
                TakeTo::Cursor
            } else {
                TakeTo::Inventory
            };
            store.take(Take::Item { id, to }).map(|_| None)
        }
        StashAsk::TakeStack { code, count } => {
            // The game server places a take whole or not at all: ask for
            // no more than fits, so "take all" fills the inventory.
            let want = match a.room {
                Some(0) => return Err(StoreError::NoRoom),
                Some(r) => count.min(u32::try_from(r).unwrap_or(u32::MAX)),
                None => count,
            };
            let n = store.take(Take::Stack { code, count: want })?;
            Ok((u64::from(count) > n && count != u32::MAX)
                .then(|| format!("Only {n} fit in the inventory")))
        }
        StashAsk::Arrange { page } => store.arrange(page).map(|_| None),
        StashAsk::Gold { put: true } => {
            if a.gold == 0 {
                return Ok(None);
            }
            store.put(Put::Gold(a.gold)).map(|_| None)
        }
        StashAsk::Gold { put: false } => {
            let have = store.gold()?;
            if have == 0 {
                return Ok(None);
            }
            let n = have.min(a.gold);
            if n == 0 {
                return Err(StoreError::NoRoom);
            }
            store.take(Take::Gold(n)).map(|_| None)
        }
    }
}

/// The view of item page `page` (or none), the slots and the gold, and
/// the search's hits; each item's look made from its record.
pub fn read_view<L: RealmStash>(
    store: &mut OnlineStash<L>,
    page: Option<u8>,
    search: &str,
    tables: Option<&ItemTables>,
) -> Result<StashView, StoreError> {
    let items = match page {
        Some(p) => store.page(p)?,
        None => Vec::new(),
    };
    let (stacks, gold) = store.stacks_and_gold()?;
    Ok(StashView {
        items: items
            .into_iter()
            .map(|i| PageItem {
                look: tables.and_then(|t| look_of(&i.record, t)),
                ..i
            })
            .collect(),
        stacks,
        gold,
        hits: if search.trim().is_empty() {
            Vec::new()
        } else {
            store.search(search)?
        },
    })
}

/// The online shared stash of the running game: the worker's channels
/// and the asks waiting for it.
#[derive(Resource)]
pub struct OnlineSharedStash {
    jobs: mpsc::Sender<Job>,
    answers: Mutex<mpsc::Receiver<Answer>>,
    catalog: Catalog,
    queued: Vec<OnlineAsk>,
    busy: bool,
    /// Tells the worker's link to give up a request (the game ended).
    stop: Arc<AtomicBool>,
}

impl Drop for OnlineSharedStash {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl OnlineSharedStash {
    /// Starts the worker on `session`. Its request numbers start at the
    /// time in milliseconds times 1024, above every number the account
    /// used before ([`OnlineStash::new`]). It ends with the resource (the
    /// game's end).
    pub fn start(session: OnlineSession, catalog: Catalog) -> std::io::Result<Self> {
        let (jobs, rx) = mpsc::channel::<Job>();
        let (tx, answers) = mpsc::channel();
        let first_req = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |d| d.as_millis() as u64)
            .saturating_mul(1024);
        let link = SessionLink::new(session);
        let stop = link.stop.clone();
        let mut store = OnlineStash::new(link, catalog.clone(), first_req);
        std::thread::Builder::new()
            .name("d2-online-stash".into())
            .spawn(move || {
                for job in rx {
                    let mut notes = Vec::new();
                    for a in &job.asks {
                        match run_ask(&mut store, a) {
                            Ok(Some(line)) => notes.push(line),
                            Ok(None) => {}
                            Err(e) => notes.push(refusal_line(&e)),
                        }
                    }
                    let view = read_view(&mut store, job.page, &job.search, job.tables.as_deref());
                    let page = job.page;
                    if tx.send(Answer { notes, view, page }).is_err() {
                        return;
                    }
                }
            })?;
        Ok(OnlineSharedStash {
            jobs,
            answers: Mutex::new(answers),
            catalog,
            queued: Vec::new(),
            busy: false,
            stop,
        })
    }

    /// The worker's answer once it came.
    fn poll(&mut self) -> Option<Answer> {
        let rx = self.answers.get_mut().unwrap_or_else(|e| e.into_inner());
        match rx.try_recv() {
            Ok(a) => Some(a),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.busy = false;
                None
            }
        }
    }
}

/// What the stash's answers read, for the market's offer form
/// ([`crate::market::holdings`]): written here, read by the realm's
/// market source.
#[derive(Resource, Clone)]
pub struct MarketBook(pub SharedBook);

/// Starts the online shared stash for a game on a game server, whose main
/// server keeps the account's stash. `names`: the slots' names by code.
pub fn install(
    world: &mut bevy::prelude::World,
    session: &OnlineSession,
    names: impl Fn(&str) -> Option<String>,
) {
    match OnlineSharedStash::start(session.clone(), Catalog::base(names)) {
        Ok(s) => world.insert_resource(s),
        Err(e) => warn!("online shared stash not started: {e}"),
    }
}

/// The online shared stash's frame system, once per app.
pub fn register(app: &mut App) {
    app.add_systems(
        Update,
        run_online_asks
            .run_if(resource_exists::<OnlineSharedStash>)
            .run_if(resource_exists::<BridgeResource>),
    );
}

/// The local character's gold (stat 14) and its room for gold (level
/// times 10000, less what it carries).
fn gold_and_room(world: &ClientWorld) -> (u64, u64) {
    let Some(u) = world.local() else {
        return (0, 0);
    };
    let have = u64::try_from(world.base(u.key, 14, 0)).unwrap_or(0);
    let level = u64::try_from(world.base(u.key, 12, 0)).unwrap_or(1).max(1);
    (have, (level * 10_000).saturating_sub(have))
}

/// The units of `guids` of a kind with a slot: only those go to the
/// slots (any other stays where it is, not lifted to be sent back).
fn with_slot(world: &ClientWorld, catalog: &Catalog, guids: &[u32]) -> Vec<u32> {
    let mine = items::local_items(world);
    guids
        .iter()
        .copied()
        .filter(|g| {
            mine.iter().any(|i| {
                i.key.guid == *g
                    && i.code
                        .as_ref()
                        .and_then(|c| std::str::from_utf8(c).ok())
                        .is_some_and(|c| catalog.slot(c.trim_end()).is_some())
            })
        })
        .collect()
}

/// How many plain `code` items fit in the local player's inventory as the
/// client sees it, counted as the game server does: its free cells times
/// one item's count (a stackable kind's full stack; a kind bigger than a
/// cell gets none). `None` without the tables or the inventory's layout.
fn stack_room(
    o: &OriginalUi,
    world: &ClientWorld,
    tables: Option<&ItemTables>,
    code: &str,
) -> Option<u64> {
    let b = code.as_bytes();
    if b.is_empty() || b.len() > 4 {
        return None;
    }
    let mut key = [b' '; 4];
    key[..b.len()].copy_from_slice(b);
    let r = tables?.items.iter().find(|r| r.code == key)?;
    if (r.invwidth, r.invheight) != (1, 1) {
        return Some(0);
    }
    let per = if r.stackable != 0 {
        r.maxstack.max(1)
    } else {
        1
    };
    Some(o.free_inventory_cells(world)? * u64::from(per))
}

/// Queues the panel's asks for the worker, hands them over when it is
/// free and shows its answer. The game's first frame turns the panel on;
/// opening the stash reads the store.
fn run_online_asks(
    mut stash: ResMut<OnlineSharedStash>,
    bridge: Res<BridgeResource>,
    parts: Option<Res<super::items::ItemParts>>,
    book: Option<Res<MarketBook>>,
    ui: Option<NonSendMut<WorldViewUi>>,
    mut was_open: Local<bool>,
) {
    let Some(mut ui) = ui else {
        return;
    };
    let Some(o) = ui.original.as_mut() else {
        return;
    };
    {
        let mut st = o.shared_stash();
        if !st.on {
            st.on = true;
            st.catalog = Some(stash.catalog.clone());
        }
    }
    let open = o.stash_open();
    let mut asks = o.shared_stash().take_asks();
    if open && !*was_open {
        asks.insert(0, StashAsk::Refresh);
    }
    // A market change moved stash goods: the market's holdings read again.
    let stale = |b: &MarketBook| b.0.lock().unwrap_or_else(|e| e.into_inner()).take_stale();
    if book.as_deref().is_some_and(stale) && !asks.contains(&StashAsk::Refresh) {
        asks.push(StashAsk::Refresh);
    }
    if !open && *was_open {
        let mut st = o.shared_stash();
        st.search_focus = false;
        st.count_box = None;
    }
    *was_open = open;
    let world = bridge.0.world();
    let tables = parts
        .as_ref()
        .and_then(|p| p.tips.as_ref().map(|t| t.tables()));
    let (have, room) = gold_and_room(world);
    for ask in asks {
        let ask = match ask {
            StashAsk::PutStack { guid } if with_slot(world, &stash.catalog, &[guid]).is_empty() => {
                o.add_note(NO_SLOT);
                continue;
            }
            StashAsk::SendStackables { guids } => StashAsk::SendStackables {
                guids: with_slot(world, &stash.catalog, &guids),
            },
            a => a,
        };
        let gold = match ask {
            StashAsk::Gold { put: true } => have,
            StashAsk::Gold { put: false } => room,
            _ => 0,
        };
        let fits = match &ask {
            StashAsk::TakeStack { code, .. } => stack_room(o, world, tables.as_deref(), code),
            _ => None,
        };
        stash.queued.push(OnlineAsk {
            ask,
            gold,
            room: fits,
        });
    }
    if let Some(a) = stash.poll() {
        stash.busy = false;
        for line in &a.notes {
            o.add_note(line);
        }
        match a.view {
            Ok(v) => {
                if let Some(b) = &book {
                    let items = v.items.iter().map(|i| (i.id.0, i.record.clone())).collect();
                    let mut b = b.0.lock().unwrap_or_else(|e| e.into_inner());
                    b.saw(a.page, v.gold, v.stacks.clone(), items);
                }
                o.show_shared_stash(v)
            }
            Err(e) => o.add_note(&refusal_line(&e)),
        }
    }
    if stash.busy || stash.queued.is_empty() {
        return;
    }
    let (page, search) = {
        let st = o.shared_stash();
        (st.view_page(), st.search.clone())
    };
    let job = Job {
        asks: std::mem::take(&mut stash.queued),
        page,
        search,
        tables,
    };
    stash.busy = stash.jobs.send(job).is_ok();
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use d2_shared_stash::online::{code, goods, tab};

    use super::*;

    /// A fake main-server connection: keeps what was sent, answers from a
    /// queue.
    #[derive(Default)]
    struct FakeRealm {
        sent: Vec<ToRealm>,
        answers: VecDeque<FromRealm>,
    }

    impl RealmStash for FakeRealm {
        fn call(&mut self, msg: ToRealm) -> Result<FromRealm, StoreError> {
            self.sent.push(msg);
            self.answers
                .pop_front()
                .ok_or_else(|| StoreError::Link("connection closed".into()))
        }
    }

    fn done(req: u64, code: u16, count: u64) -> FromRealm {
        FromRealm::Done {
            req,
            code,
            count,
            left: Vec::new(),
        }
    }

    fn slots(gold: u64) -> FromRealm {
        FromRealm::Page {
            tab: tab::STACKS,
            page: 0,
            items: Vec::new(),
            stacks: vec![("r01".into(), 2)],
            gold,
        }
    }

    fn store(answers: Vec<FromRealm>) -> OnlineStash<FakeRealm> {
        OnlineStash::new(
            FakeRealm {
                sent: Vec::new(),
                answers: answers.into(),
            },
            Catalog::base(|_| None),
            7000,
        )
    }

    fn ask(ask: StashAsk, gold: u64) -> OnlineAsk {
        OnlineAsk {
            ask,
            gold,
            room: None,
        }
    }

    /// The panel's asks reach the main server as the contract's goods: an
    /// item, the character's gold, the stash's gold up to the room.
    #[test]
    fn asks_go_out_as_the_contract_s_goods() {
        let mut s = store(vec![
            done(7000, 0, 1),
            done(7001, 0, 1),
            done(7002, 0, 300),
            slots(5000),
            done(7003, 0, 1200),
        ]);
        let put = ask(
            StashAsk::PutItem {
                guid: 41,
                page: 2,
                at: Some((3, 4)),
            },
            0,
        );
        assert_eq!(run_ask(&mut s, &put), Ok(None));
        let take = ask(
            StashAsk::TakeItem {
                id: ItemId(9),
                to_cursor: false,
            },
            0,
        );
        assert_eq!(run_ask(&mut s, &take), Ok(None));
        assert_eq!(
            run_ask(&mut s, &ask(StashAsk::Gold { put: true }, 300)),
            Ok(None)
        );
        // 5000 in the stash, room for 1200 on the character.
        assert_eq!(
            run_ask(&mut s, &ask(StashAsk::Gold { put: false }, 1200)),
            Ok(None)
        );
        let sent = &s.link().sent;
        assert_eq!(
            sent[0],
            ToRealm::Put {
                req: 7000,
                unit: 41,
                to: goods::ITEM,
                gold: 0,
                cell: Some(Cell::new(2, 3, 4))
            }
        );
        assert!(matches!(
            sent[1],
            ToRealm::Take {
                from: goods::ITEM,
                item: Some(ItemId(9)),
                ..
            }
        ));
        assert!(matches!(
            sent[2],
            ToRealm::Put {
                to: goods::GOLD,
                gold: 300,
                unit: 0,
                ..
            }
        ));
        assert!(matches!(
            sent[4],
            ToRealm::Take {
                from: goods::GOLD,
                gold: 1200,
                ..
            }
        ));
        // The wire carries the same numbers.
        assert!(matches!(
            to_wire(&sent[1]),
            StashRequest::Take {
                from: 1,
                item: Some(9),
                ..
            }
        ));
    }

    /// Nothing is asked for no gold, and a take with no room says so.
    #[test]
    fn gold_moves_need_gold_and_room() {
        let mut s = store(vec![slots(5000)]);
        assert_eq!(
            run_ask(&mut s, &ask(StashAsk::Gold { put: true }, 0)),
            Ok(None)
        );
        assert_eq!(
            run_ask(&mut s, &ask(StashAsk::Gold { put: false }, 0)),
            Err(StoreError::NoRoom)
        );
        assert_eq!(s.link().sent.len(), 1, "only the stash's gold was read");
    }

    /// A take with no room, and stackables the main server doesn't keep
    /// yet: each a line, nothing else.
    #[test]
    fn refused_moves_read_as_lines() {
        let mut s = store(vec![
            done(7000, code::NO_ROOM, 0),
            done(7001, code::NOT_SUPPORTED, 0),
        ]);
        let take = ask(
            StashAsk::TakeItem {
                id: ItemId(9),
                to_cursor: true,
            },
            0,
        );
        let e = run_ask(&mut s, &take).unwrap_err();
        assert_eq!(refusal_line(&e), "Not enough room");
        let send = ask(StashAsk::SendStackables { guids: vec![1, 2] }, 0);
        let e = run_ask(&mut s, &send).unwrap_err();
        assert!(refusal_line(&e).contains("not kept online yet"), "{e}");
    }

    /// A slot's take asks for no more than fits in the inventory: "take
    /// all" fills it, and with no room nothing is asked.
    #[test]
    fn a_slot_take_asks_no_more_than_fits() {
        let mut s = store(vec![done(7000, 0, 5), done(7001, 0, 2)]);
        let take = |count, room| OnlineAsk {
            ask: StashAsk::TakeStack {
                code: "r01".into(),
                count,
            },
            gold: 0,
            room,
        };
        assert_eq!(run_ask(&mut s, &take(u32::MAX, Some(5))), Ok(None));
        assert_eq!(run_ask(&mut s, &take(2, Some(9))), Ok(None));
        assert_eq!(run_ask(&mut s, &take(3, Some(0))), Err(StoreError::NoRoom));
        let sent = &s.link().sent;
        assert_eq!(sent.len(), 2, "no request without room");
        let counts: Vec<u32> = sent
            .iter()
            .filter_map(|m| match m {
                ToRealm::Take {
                    from: goods::STACK,
                    stack: Some((c, n)),
                    ..
                } if c == "r01" => Some(*n),
                _ => None,
            })
            .collect();
        assert_eq!(counts, [5, 2]);
    }

    /// A stash item's record shows as the item it is: the look is the
    /// stream the game server would send for it.
    #[test]
    fn a_stash_record_shows_as_its_item() {
        let tables = crate::ui::item_tip::tests::tips().tables();
        let cap = d2_sim::items::bitstream::StreamItem {
            flags: 0x10,
            compact: true,
            version: 101,
            code: *b"cap ",
            ..Default::default()
        };
        let (record, _) = bitstream::write_save(&cap, &tables.isc).unwrap();
        let look = look_of(&record, &tables).expect("the record reads");
        assert_eq!(look, bitstream::write(&cap, &tables.isc).unwrap().0);
        let head = crate::bridge::items::peek(&look).unwrap();
        assert_eq!(head.6, Some(*b"cap "));
        assert_eq!(look_of(&[0x4A, 0x4D, 1], &tables), None, "a short record");
    }

    /// The view: the page's items, the slots and gold in one request, and
    /// the search's hits; a request number answers a `Done` as its own.
    #[test]
    fn the_view_reads_the_page_slots_gold_and_hits() {
        let item = RealmItem {
            id: ItemId(3),
            cell: Cell::new(1, 0, 0),
            size: (2, 2),
            record: vec![1, 2, 3],
        };
        let mut s = store(vec![
            FromRealm::Page {
                tab: tab::ITEMS,
                page: 1,
                items: vec![item.clone()],
                stacks: Vec::new(),
                gold: 0,
            },
            slots(77),
            FromRealm::Found {
                hits: vec![(ItemId(3), item.cell)],
            },
        ]);
        let v = read_view(&mut s, Some(1), "cap", None).unwrap();
        assert_eq!(v.items.len(), 1);
        assert_eq!(v.items[0].id, ItemId(3));
        assert_eq!(v.items[0].look, None, "no tables: the code is shown");
        assert_eq!(v.gold, 77);
        assert_eq!(v.stacks.get("r01"), Some(&2));
        assert_eq!(v.hits, vec![(ItemId(3), item.cell)]);
        assert_eq!(s.link().sent.len(), 3);
        assert_eq!(
            from_wire(
                Some(12),
                StashAnswer::Done {
                    code: 0,
                    count: 1,
                    left: Vec::new()
                }
            ),
            done(12, 0, 1)
        );
    }
}
