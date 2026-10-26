//! D2RUST's own game on both sides of the journal: how the game server
//! builds a game, hashes its state and digests its saves, and the
//! [`ServerReplay`] the audit re-runs a journal on. Both sides use these
//! same functions, so a re-run builds, hashes and saves exactly as the
//! game server did.
//!
//! - [`build_game`]: the game of a [`GameSpec`] on a seed, with a loader
//!   that gives each client its seat's character ([`characters`]).
//! - [`state_line`] / [`state_hash`]: the `state-1` line of the game
//!   (`d2_sim::debug::state`, with the players' quest records) and its
//!   SHA-256.
//! - [`save_digest`]: SHA-256 of a save's record with its save time zeroed.
//! - [`DigestStore`]: the character writer that makes each save as the
//!   game server's store does and hands on its digest.
//! - [`take_out`] / [`bring_in`]: the game half of a stash or market
//!   transfer, run between host frames.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use d2_formats::d2s::appearance::AppearanceTables;
use d2_formats::d2s::{self, D2s, SaveTables};
use d2_server::adapters::character::LoadContext;
use d2_server::adapters::session_flow::{CreateGame, SessionFlow};
use d2_server::adapters::storage::CharacterStore;
use d2_server::adapters::ProtoSizes;
use d2_server::game::{
    character_save, loader, Character, Characters, GameBuilder, GameData, NewCharacter,
    RefusedLoad, Sim, World, LOAD_FAILED,
};
use d2_server::host::Host;
use d2_server::local_seams::LocalSeams;
use d2_server::seams::{ClientId, Clock, Intents, MessageSink, SessionHandler, Tick};
use d2_server::stash_items::{self, LiftedItem};
use d2_sim::debug::state;
use d2_sim::items::bitstream::read::read_save_entry;
use d2_sim::items::inventory::mode;
use d2_sim::items::moves::handlers::load_to_cursor;
use d2_sim::stats::stat;
use d2_sim::units::lifecycle::LifecycleHooks;
use d2_sim::units::UnitId;
use d2_sim::wiring::inventory::{InvDesk, InvRest};
use d2_sim::wiring::worldgen::WorldSim;
use sha2::{Digest as _, Sha256};

use crate::format::{Digest, GameSpec, Goods, Start};
use crate::replay::{Client, ReplayGame, TransferIn, TransferOut};

/// SHA-256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> Digest {
    Sha256::digest(bytes).into()
}

/// A character that has never been played: the game makes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NewChar {
    pub class: u8,
    pub hardcore: bool,
    /// The progression bits set before the first game (0-31).
    pub progression: u8,
}

/// What a seat's character joins with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Joined {
    /// Its stored record (the `.d2s` bytes).
    Record(Vec<u8>),
    /// A new character.
    New(NewChar),
}

/// A seat's character: its name and what it joins with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatCharacter {
    pub name: String,
    pub joined: Joined,
}

impl SeatCharacter {
    /// The digest a [`crate::format::Record::Join`] names: SHA-256 of the
    /// record, or of [`new_character_bytes`] for a new character.
    pub fn digest(&self) -> Digest {
        match &self.joined {
            Joined::Record(b) => sha256(b),
            Joined::New(n) => sha256(&new_character_bytes(&self.name, n)),
        }
    }
}

/// What a new character's join digest hashes: a fixed tag, the name, the
/// class, hardcore and the progression bits.
pub fn new_character_bytes(name: &str, n: &NewChar) -> Vec<u8> {
    let mut b = b"d2-journal new character\0".to_vec();
    b.extend_from_slice(name.as_bytes());
    b.push(0);
    b.extend_from_slice(&[n.class, u8::from(n.hardcore), n.progression]);
    b
}

/// The save a seat's character is laid over: its record read with the
/// game's tables, or a new character's stub (as the game server loads it).
pub fn base_save(data: &GameData, c: &SeatCharacter, difficulty: u8) -> Result<D2s, String> {
    match &c.joined {
        Joined::New(new) => {
            let mut base = character_save::fresh(new.class, c.name.as_bytes(), difficulty);
            if new.hardcore {
                base.header.status |= d2s::status::HARDCORE;
            }
            base.header.status |= u16::from(new.progression & 0x1F) << 8;
            Ok(base)
        }
        Joined::Record(bytes) => {
            let GameData::Live(live) = data;
            live.read_save(bytes, difficulty)
                .map_err(|e| format!("{}: the character doesn't load: {e}", c.name))
        }
    }
}

/// One seated client of a game.
#[derive(Clone, Debug)]
pub struct Seat {
    pub character: SeatCharacter,
    /// The save its live state is laid over at each save.
    pub base: D2s,
}

/// The seats of a game by client, shared by its loader and its store.
pub type Seats = Rc<RefCell<BTreeMap<ClientId, Seat>>>;

/// A name as the 16-byte NUL-padded field of the game's messages.
fn name16(name: &str) -> [u8; 16] {
    let mut b = [0u8; 16];
    let n = name.len().min(15);
    b[..n].copy_from_slice(&name.as_bytes()[..n]);
    b
}

/// The loader's characters: each client's seat's character; a client with
/// no seat is refused.
pub fn characters(seats: Seats, spec: GameSpec) -> Characters {
    Box::new(move |client, r: &CreateGame| {
        let seats = seats.borrow();
        let Some(s) = seats.get(&client) else {
            return Character::Refused(RefusedLoad {
                result: LOAD_FAILED,
                name: r.char_name,
                expansion: spec.expansion,
                difficulty: spec.difficulty,
                class: r.class,
                after_player: false,
                skills: None,
            });
        };
        match &s.character.joined {
            Joined::New(new) => Character::Named(NewCharacter {
                class: new.class,
                name: name16(&s.character.name),
                difficulty: spec.difficulty,
            }),
            Joined::Record(_) => Character::Save(
                Box::new(s.base.clone()),
                LoadContext {
                    difficulty: spec.difficulty,
                    map_seed_applies: false,
                },
            ),
        }
    })
}

/// The character writer a game server's game gets.
pub type Store = Box<dyn CharacterStore<WorldSim<LocalSeams>, World>>;

/// Builds the game of `spec` on `seed` (on the game's own thread): the
/// players' characters come from `characters`, their saves go to `store`.
pub fn build_game(
    spec: &GameSpec,
    data: GameData,
    seed: u32,
    characters: Characters,
    store: Store,
) -> Result<Sim, String> {
    let character = Character::New.with_difficulty(spec.difficulty);
    let mut sim = GameBuilder::new(data)
        .seed(seed)
        .character(character)
        .build()
        .map_err(|e| e.to_string())?;
    if spec.hardcore {
        sim.events.action.hooks().x.hardcore = true;
    }
    sim.set_session(
        SessionFlow::new(loader(characters)).with_max_players(usize::from(spec.max_players)),
    );
    sim.set_storage(store);
    Ok(sim)
}

/// The game's `state-1` line now: every unit, and the players' quest
/// records of the game's difficulty. Reads only.
pub fn state_line(sim: &Sim) -> String {
    let mut s = state::snapshot_world(&sim.game, &sim.events);
    let d = usize::from(state::difficulty_world(&sim.events)).min(2);
    for (id, q) in &sim.world.rest.quests {
        if let Some(e) = sim.game.lists.unit(*id) {
            s.set_quests(e.guid, state::quest_words(&q.flags[d]));
        }
    }
    s.to_json_line()
}

/// SHA-256 of [`state_line`].
pub fn state_hash(sim: &Sim) -> Digest {
    sha256(state_line(sim).as_bytes())
}

/// SHA-256 of `save`'s record with its save time zeroed (the time is the
/// only part of a save that doesn't come from the game).
pub fn save_digest(save: &D2s, tables: &dyn SaveTables) -> Result<Digest, String> {
    let mut s = save.clone();
    s.header.save_time = 0;
    let bytes = d2s::write(&s, tables).map_err(|e| e.to_string())?;
    Ok(sha256(&bytes))
}

/// The save of `client`'s character now, as the game server's store makes
/// it (at save time `now`): `None` when the client has no seat or no
/// player.
pub fn make_save(
    sim: &mut Sim,
    seats: &Seats,
    client: ClientId,
    appearance: Option<&AppearanceTables>,
    now: u32,
) -> Result<Option<D2s>, String> {
    let Some(seat) = seats.borrow().get(&client).cloned() else {
        return Ok(None);
    };
    let Some(player) = sim.player_of(client) else {
        return Ok(None);
    };
    let live = character_save::read_live(sim, player);
    if seat.base.header.version < d2s::VERSION
        && character_save::old_items_pass_through(&seat.base, &live)
    {
        return Err(format!(
            "{}: a version {:#x} character whose items the game cannot rewrite is not overwritten",
            seat.character.name, seat.base.header.version
        ));
    }
    let mut save = character_save::apply_live(&seat.base, &live, now);
    if let Some(t) = appearance {
        character_save::rebuild_appearance(sim, player, t, &mut save);
    }
    Ok(Some(save))
}

/// Carried gold (the character's inventory gold).
const GOLD: u16 = 14;
/// Carried gold allowed per character level.
const GOLD_PER_LEVEL: i32 = 10_000;
/// Item flags of the removal message: the item leaves the client.
const ITEM_GONE: u32 = 0x20;

/// The game half of a stash or market transfer that takes goods out of
/// `client`'s character (journal [`crate::format::Record::ItemOut`],
/// contracts 11.13): the item `out.unit` (on the cursor, or stored in the
/// inventory, the stash or the cube) leaves the game with what is
/// socketed in it, and `out.gold` leaves the carried gold. The game server
/// runs it between host frames and saves the character at once; a re-run
/// runs it at the record. A stackable (`out.stack`, plan N28b) is the
/// item `out.unit` itself, a plain item of that code holding that count.
/// Nothing changes on `Err`: the goods aren't on the character.
pub fn take_out(sim: &mut Sim, client: ClientId, out: &TransferOut<'_>) -> Result<(), String> {
    take_out_item(sim, client, out).map(|_| ())
}

/// [`take_out`], with the item that left as its save entry when it was
/// on the cursor or in the inventory (what a stash put takes: the game
/// server sends those bytes to the main server).
pub fn take_out_item(
    sim: &mut Sim,
    client: ClientId,
    out: &TransferOut<'_>,
) -> Result<Option<LiftedItem>, String> {
    let p = sim
        .player_of(client)
        .ok_or_else(|| format!("client {client} has no player"))?;
    if let Some((code, count)) = out.stack {
        let held = stash_items::held_item(sim, p, out.unit)
            .ok_or_else(|| format!("stackable {code:?}: item {} is not held", out.unit))?;
        if held.code != code || held.plain_count != Some(count) {
            return Err(format!(
                "stackable {code:?} x{count}: item {} is {:?} x{:?}",
                out.unit, held.code, held.plain_count
            ));
        }
    }
    let gold = i32::try_from(out.gold).map_err(|_| format!("{} gold", out.gold))?;
    let have = carried_gold(sim, p);
    if gold > have {
        return Err(format!("{gold} gold taken, the character carries {have}"));
    }
    let mut lifted = None;
    if out.unit != 0 {
        if stash_items::held_item(sim, p, out.unit).is_some() {
            lifted = Some(stash_items::lift_item(sim, p, out.unit)?);
        } else {
            remove_stored(sim, p, out.unit)?;
        }
    }
    if gold > 0 {
        set_carried_gold(sim, p, have - gold);
    }
    Ok(lifted)
}

/// The game half of a stash or market transfer that brings goods to
/// `client`'s character (journal [`crate::format::Record::ItemIn`]): the
/// item goes to the first free place of the inventory (or the cursor,
/// `item_in.cursor`), a stackable as that many fresh plain items in the
/// inventory (all or none), the gold to the carried gold. Returns 0
/// placed, 1 no room (for the item or the stack; a busy cursor), 2
/// refused (no player, unreadable item bytes, an unknown stackable, or the
/// gold would pass the carried limit); on 1 and 2 nothing changes. Run as
/// [`take_out`] is.
pub fn bring_in(sim: &mut Sim, client: ClientId, item_in: &TransferIn<'_>) -> Result<u8, String> {
    bring_in_item(sim, client, item_in).map(|(result, _)| result)
}

/// [`bring_in`], with the unit ids of the items it placed.
pub fn bring_in_item(
    sim: &mut Sim,
    client: ClientId,
    item_in: &TransferIn<'_>,
) -> Result<(u8, Vec<u32>), String> {
    let Some(p) = sim.player_of(client) else {
        return Ok((2, Vec::new()));
    };
    let Ok(gold) = i32::try_from(item_in.gold) else {
        return Ok((2, Vec::new()));
    };
    let have = carried_gold(sim, p);
    let level = sim
        .events
        .action
        .sys
        .stats
        .unit_base(p, stat::LEVEL, 0)
        .max(1);
    if have.saturating_add(gold) > level.saturating_mul(GOLD_PER_LEVEL) {
        return Ok((2, Vec::new()));
    }
    let mut units = Vec::new();
    match item_in.goods {
        Goods::Item(bytes) => match place_new(sim, p, bytes, item_in.cursor) {
            Placed::Yes(guid) => units.push(guid),
            Placed::NoRoom => return Ok((1, Vec::new())),
            Placed::Unreadable => return Ok((2, Vec::new())),
        },
        Goods::Stack { count: 0, .. } => {}
        Goods::Stack { code, count } => {
            let room = stash_items::plain_room(sim, p, code);
            if room == 0 && !stash_items::plain_kind_known(sim, code) {
                return Ok((2, Vec::new()));
            }
            if room < u64::from(*count) {
                return Ok((1, Vec::new()));
            }
            match stash_items::give_plain(sim, p, code, *count) {
                Ok(made) => units = made,
                Err(_) => return Ok((1, Vec::new())),
            }
        }
    }
    if gold > 0 {
        set_carried_gold(sim, p, have + gold);
    }
    Ok((0, units))
}

/// The save that goes with a transfer: `client`'s character alone, at
/// once, through the game's store. The game server runs a transfer as
/// [`take_out`] or [`bring_in`], hands its record to the recorder
/// (`Recorder::record`), then runs this (after a [`bring_in`] only when
/// it placed the goods), so the journal has the transfer, then its save.
pub fn save_after_transfer(sim: &mut Sim, client: ClientId) -> Result<(), String> {
    match sim.save_character(client) {
        Some(Ok(())) => Ok(()),
        Some(Err(e)) => Err(format!("save of client {client}: {e:?}")),
        None => Err(format!("client {client} has no player")),
    }
}

fn carried_gold(sim: &Sim, p: UnitId) -> i32 {
    sim.events.action.sys.stats.unit_base(p, GOLD, 0)
}

fn set_carried_gold(sim: &mut Sim, p: UnitId, gold: i32) {
    let sys = &mut sim.events.action.sys;
    sys.stats.unit_set(&mut sys.hooks, p, GOLD, gold, 0);
}

/// Hands the inventory model's messages to the next tick's sends.
fn queue_inventory_sent(sim: &mut Sim) {
    let sent = sim
        .world
        .take_inventory_sent(&mut sim.game, &mut sim.events);
    sim.world.rest.sent.extend(sent);
}

/// Frees `item` and, first, every item socketed into it.
fn free_tree<H, R>(d: &mut InvDesk<'_, '_, H, R>, item: UnitId)
where
    H: LifecycleHooks,
    R: InvRest + ?Sized,
{
    for filler in d.state.fillers(item) {
        free_tree(d, filler);
    }
    d.state.inventories.remove(&item);
    d.free(item);
}

/// The stored item `guid` of player `p` out of the game, as a sale takes
/// it: S→C 0x9D action 5 with the item's page, then unlinked and freed.
fn remove_stored(sim: &mut Sim, p: UnitId, guid: u32) -> Result<(), String> {
    let r = sim
        .world
        .with_economy(&mut sim.game, &mut sim.events, |econ, parts| {
            let inv = parts
                .inventory
                .as_deref_mut()
                .ok_or_else(|| "no inventory model in this game".to_string())?;
            let mut d = InvDesk::new(econ, &inv.tables, &mut inv.state, inv.rest.as_mut());
            if d.check_stored(p, guid) != 0 {
                return Err(format!("item {guid} is not a stored item of the character"));
            }
            let item = d.item_unit(guid).ok_or_else(|| format!("no item {guid}"))?;
            let page = d.state.items.get(&item).map_or(0, |i| i.page);
            d.send_item_page(p, item, ITEM_GONE, page)
                .map_err(|e| format!("item {guid}: {e:?}"))?;
            d.remove(p, item);
            free_tree(&mut d, item);
            Ok(())
        });
    queue_inventory_sent(sim);
    r
}

/// The unit ids of `client`'s stored items (what a [`take_out`] can take:
/// the inventory, stash and cube, not what the character wears or carries
/// in its belt), in the inventory's link order.
pub fn stored_items(sim: &mut Sim, client: ClientId) -> Vec<u32> {
    let Some(p) = sim.player_of(client) else {
        return Vec::new();
    };
    sim.world
        .with_economy(&mut sim.game, &mut sim.events, |econ, parts| {
            let Some(inv) = parts.inventory.as_deref_mut() else {
                return Vec::new();
            };
            let d = InvDesk::new(econ, &inv.tables, &mut inv.state, inv.rest.as_mut());
            d.state
                .items_of(p)
                .into_iter()
                .map(|item| d.guid_of(item))
                .filter(|&guid| d.check_stored(p, guid) == 0)
                .collect()
        })
}

enum Placed {
    /// Placed: the item's unit id.
    Yes(u32),
    NoRoom,
    Unreadable,
}

/// The item of save `bytes` made (with what is socketed in it) and put at
/// the first free place of player `p`'s inventory, or on its cursor when
/// that is free, sent to the client.
fn place_new(sim: &mut Sim, p: UnitId, bytes: &[u8], cursor: bool) -> Placed {
    let placed = sim
        .world
        .with_economy(&mut sim.game, &mut sim.events, |econ, parts| {
            let Some(inv) = parts.inventory.as_deref_mut() else {
                return Placed::Unreadable;
            };
            let mut d = InvDesk::new(econ, &inv.tables, &mut inv.state, inv.rest.as_mut());
            let Ok(entry) = read_save_entry(bytes, d.econ.tables) else {
                return Placed::Unreadable;
            };
            let owner = d.owner_of(p);
            if cursor && (owner.is_none() || d.state.cursor_of(p).is_some()) {
                return Placed::NoRoom;
            }
            let Ok(item) = d.make_entry(&entry) else {
                return Placed::Unreadable;
            };
            if let Some(i) = d.state.items.get_mut(&item) {
                i.mode = mode::CURSOR;
                i.page = 0;
            }
            d.sync_out();
            let guid = d.guid_of(item);
            if let (true, Some(owner)) = (cursor, owner) {
                load_to_cursor(&mut d, owner, guid);
                Placed::Yes(guid)
            } else if d.place(p, item, (0, 0), true, true) {
                Placed::Yes(guid)
            } else {
                free_tree(&mut d, item);
                Placed::NoRoom
            }
        });
    queue_inventory_sent(sim);
    placed
}

/// What [`DigestStore`] hands each save to: the client, the save and its
/// digest.
pub type OnSave = Box<dyn FnMut(ClientId, &D2s, Digest)>;

/// A character writer that makes each save as the game server's store
/// does and hands `(client, save, digest)` on.
pub struct DigestStore {
    pub seats: Seats,
    pub tables: Arc<dyn SaveTables + Send + Sync>,
    pub appearance: Option<Arc<AppearanceTables>>,
    pub on_save: OnSave,
}

impl CharacterStore<WorldSim<LocalSeams>, World> for DigestStore {
    fn save(&mut self, sim: &mut Sim, client: ClientId) -> Result<(), String> {
        let Some(save) = make_save(sim, &self.seats, client, self.appearance.as_deref(), 0)? else {
            return Ok(());
        };
        let digest = save_digest(&save, &*self.tables)?;
        (self.on_save)(client, &save, digest);
        Ok(())
    }
}

/// The system messages the game's session flow leaves to the host: the
/// ping (C→S 0x6D) is answered with the pong (S→C 0x8F).
#[derive(Debug, Default)]
pub struct PongSession;

const PING: u8 = 0x6D;
const PONG: [u8; 33] = {
    let mut b = [0; 33];
    b[0] = 0x8F;
    b
};

impl SessionHandler for PongSession {
    fn system_message(
        &mut self,
        client: ClientId,
        msg: &[u8],
        _: usize,
        out: &mut dyn MessageSink,
    ) {
        if msg.first() == Some(&PING) {
            let _ = out.queue(client, &PONG);
        }
    }
}

/// A clock a re-run sets: each reading is the value last set.
#[derive(Clone, Debug, Default)]
pub struct ReplayClock(pub Arc<AtomicU32>);

impl ReplayClock {
    pub fn set(&self, ms: u32) {
        self.0.store(ms, Ordering::Relaxed);
    }
}

impl Clock for ReplayClock {
    fn now_ms(&mut self) -> u32 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Where a re-run gets the characters the clients joined with (the main
/// server's records, by lease and digest).
pub trait RecordSource {
    fn character(&self, lease: u64, record: &Digest) -> Option<SeatCharacter>;
}

impl RecordSource for BTreeMap<u64, SeatCharacter> {
    fn character(&self, lease: u64, _: &Digest) -> Option<SeatCharacter> {
        self.get(&lease).cloned()
    }
}

/// The build a journal needs: the same program version and game data.
pub fn check_build(start: &Start, version: &str, data_hash: &Digest) -> Result<(), String> {
    if start.version != version {
        return Err(format!(
            "the game ran on version {}, this is {version}",
            start.version
        ));
    }
    if start.data_hash != *data_hash {
        return Err("the game ran on other game data".into());
    }
    Ok(())
}

type ReplayHost = Host<Sim, ProtoSizes, PongSession, ReplayClock>;

/// A journal's game built again, for [`crate::replay::replay`].
pub struct ServerReplay<'a> {
    pub host: ReplayHost,
    clock: ReplayClock,
    seats: Seats,
    saves: Rc<RefCell<Vec<(Client, Digest)>>>,
    data: GameData,
    spec: GameSpec,
    records: &'a dyn RecordSource,
    clients: BTreeSet<ClientId>,
}

impl<'a> ServerReplay<'a> {
    /// The game of `start`, built on `data` (the build and data the journal
    /// names: [`check_build`]), with the characters of `records`.
    /// `appearance` is what the game server's store had.
    pub fn new(
        start: &Start,
        data: GameData,
        tables: Arc<dyn SaveTables + Send + Sync>,
        appearance: Option<Arc<AppearanceTables>>,
        records: &'a dyn RecordSource,
    ) -> Result<Self, String> {
        let seats: Seats = Rc::default();
        let saves: Rc<RefCell<Vec<(Client, Digest)>>> = Rc::default();
        let out = saves.clone();
        let store = DigestStore {
            seats: seats.clone(),
            tables,
            appearance,
            on_save: Box::new(move |c, _, d| out.borrow_mut().push((c, d))),
        };
        let sim = build_game(
            &start.spec,
            data.clone(),
            start.seed,
            characters(seats.clone(), start.spec.clone()),
            Box::new(store),
        )?;
        if sim.tick_rate().hz() != start.tick_rate {
            return Err(format!(
                "the game ran at {} Hz, this build runs it at {} Hz",
                start.tick_rate,
                sim.tick_rate().hz()
            ));
        }
        let clock = ReplayClock::default();
        let host = Host::new(sim, ProtoSizes, PongSession, clock.clone());
        Ok(Self {
            host,
            clock,
            seats,
            saves,
            data,
            spec: start.spec.clone(),
            records,
            clients: BTreeSet::new(),
        })
    }
}

impl ReplayGame for ServerReplay<'_> {
    fn join(&mut self, client: Client, lease: u64, record: &Digest) -> Result<(), String> {
        if lease != 0 || *record != [0; 32] {
            let c = self
                .records
                .character(lease, record)
                .ok_or_else(|| format!("no character record for lease {lease}"))?;
            if c.digest() != *record {
                return Err(format!(
                    "the record of lease {lease} isn't the one the game was joined with"
                ));
            }
            let base = base_save(&self.data, &c, self.spec.difficulty)?;
            self.seats
                .borrow_mut()
                .insert(client, Seat { character: c, base });
        }
        self.clients.insert(client);
        self.host.connect(client);
        Ok(())
    }

    fn leave(&mut self, client: Client) {
        self.clients.remove(&client);
        self.host.disconnect(client);
    }

    fn send(&mut self, client: Client, system: bool, now: u32, msg: &[u8]) {
        if system {
            let _ = self.host.send_system(client, msg);
        } else {
            self.host.set_send_now(Some(now));
            let _ = self.host.send_game(client, msg);
        }
    }

    fn frame(&mut self, now: u32) -> Result<(), String> {
        self.clock.set(now);
        self.host.frame().map_err(|e| e.to_string())?;
        // What the game sent goes nowhere.
        for &c in &self.clients {
            self.host.receive(c);
        }
        Ok(())
    }

    fn frame_no(&self) -> u32 {
        u32::try_from(self.host.game.frame()).unwrap_or(0)
    }

    fn state_hash(&mut self) -> Digest {
        state_hash(&self.host.game)
    }

    fn save_all(&mut self) {
        let _ = self.host.game.save_characters();
    }

    fn take_saves(&mut self) -> Vec<(Client, Digest)> {
        std::mem::take(&mut *self.saves.borrow_mut())
    }

    fn item_out(&mut self, client: Client, out: &TransferOut<'_>) -> Result<(), String> {
        take_out(&mut self.host.game, client, out)?;
        save_after_transfer(&mut self.host.game, client)
    }

    fn item_in(&mut self, client: Client, item_in: &TransferIn<'_>) -> Result<u8, String> {
        let result = bring_in(&mut self.host.game, client, item_in)?;
        if result == 0 {
            save_after_transfer(&mut self.host.game, client)?;
        }
        Ok(result)
    }
}
