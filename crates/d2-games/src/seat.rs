//! The seats of one game, and the parts of the game that act on them:
//!
//! - [`SeatStream`]: a seat's game stream as the hub reads it. It checks
//!   the player's 0x67 join against the seat and writes the game's own
//!   settings into it, and drops character uploads.
//! - [`characters`]: the game's character loader input: each client plays
//!   its seat's character.
//! - [`ServerStore`]: the game's character writer: each client's character
//!   is handed to the game's [`SaveSink`].
//! - [`ServerSession`]: the system messages the game's session flow
//!   doesn't take (the ping).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex};

use d2_formats::d2s::appearance::AppearanceTables;
use d2_formats::d2s::{self, D2s, SaveTables};
use d2_journal::server::save_digest;
use d2_net::msg::account::CharSummary;
use d2_net::msg::ErrorCode;
use d2_net::wire::Refused;
use d2_net::{Lane, NetError, Packet, Transport};
use d2_proto::FixedMessage;
use d2_server::adapters::character::LoadContext;
use d2_server::adapters::session_flow::{create_flags, CreateGame};
use d2_server::adapters::storage::CharacterStore;
use d2_server::game::{
    character_save, Character, CharacterCodec, Characters, NewCharacter, RefusedLoad, Sim, World,
    LOAD_FAILED,
};
use d2_server::local_seams::LocalSeams;
use d2_server::seams::{ClientId, MessageSink, SessionHandler};
use d2_sim::wiring::worldgen::WorldSim;
use tokio::sync::mpsc;

use super::journal::Recorder;
use super::transfer::{self, Pending, Settled, Tag};
use super::{AttachedStream, GameEvent, GameSpec, SaveRecord, SaveSink, Seat};
use d2_service::log::ServerLog;

/// C→S 0x67, the join that creates or enters the game.
const CREATE_GAME: u8 = 0x67;
/// C→S 0x6C, a character upload: online characters live on the server.
const SAVE_UPLOAD: u8 = 0x6C;
/// C→S 0x6D, the ping.
const PING: u8 = 0x6D;

/// One seated client.
#[derive(Debug)]
pub(super) struct SeatState {
    pub seat: Seat,
    /// The save the character's live state is laid over at each save: its
    /// record as loaded, or a new character's stub.
    pub base: D2s,
    /// When the seat was taken.
    pub admitted: std::time::Instant,
    /// The client has had a record in the game (its join was taken).
    pub entered: bool,
    /// The seat's saves so far: the last [`SaveRecord::seq`] handed off.
    pub saves: u32,
    /// What the game sent it while it joined: the character state a
    /// re-entry sends again (contracts v12 3.7).
    pub snapshot: Arc<Mutex<JoinSnapshot>>,
    /// Set by its [`SeatStream`] when the game refused the client's join.
    pub refused: SeatRefusal,
    /// A stash transfer whose save waits for its answer: until then the
    /// seat makes no other save (`crate::transfer`).
    pub transfer: Option<Pending>,
}

/// The code a [`SeatStream`] refused its client's join with (0: none),
/// shared with the seat's state so the game can report it.
#[derive(Clone, Debug, Default)]
pub(super) struct SeatRefusal(Arc<AtomicU16>);

impl SeatRefusal {
    fn set(&self, code: ErrorCode) {
        self.0.store(code.0, Ordering::SeqCst);
    }

    /// The refusal's code, if the join was refused.
    pub fn code(&self) -> Option<ErrorCode> {
        match self.0.load(Ordering::SeqCst) {
            0 => None,
            c => Some(ErrorCode(c)),
        }
    }
}

/// Every S->C message a client was sent since it joined, kept so a re-entry
/// can rebuild its world and character state. Contracts v12 3.7: the client
/// clears its world on a resume; replaying this complete server-side journal
/// (not the hub parked backlog, which misses what a dying connection
/// swallowed) rebuilds the state as it is now: items, stats, quests, skills,
/// waypoints, mercenary, party and the world, in order. Bounded like the
/// hub backlog; past it the client keeps what it had.
#[derive(Debug, Default)]
pub(super) struct JoinSnapshot {
    messages: Vec<Vec<u8>>,
}

/// Most messages kept per client (the hub backlog limit).
const MAX_JOURNAL: usize = 1 << 16;

impl JoinSnapshot {
    fn record(&mut self, m: &[u8]) {
        if self.messages.len() < MAX_JOURNAL {
            self.messages.push(m.to_vec());
        }
    }

    /// Every message the client missed, in send order, without the ones
    /// that open a window: a re-entry closes any open window first, so
    /// replaying a stash or cube open would leave a window the server no
    /// longer has.
    pub fn messages(&self) -> impl Iterator<Item = &Vec<u8>> {
        self.messages.iter().filter(|m| !opens_window(m))
    }
}

/// S→C 0x77 actions that open a window (the stash view and the cube).
fn opens_window(m: &[u8]) -> bool {
    matches!(m, [0x77, 0x10, ..] | [0x77, 0x15, ..])
}

/// The game's seats by client.
#[derive(Debug, Default)]
pub(super) struct Seats {
    by_client: BTreeMap<ClientId, SeatState>,
}

/// Why a seat can't be taken.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Taken {
    Free,
    Full,
    /// The seat's character is in this game already.
    Character,
}

impl Seats {
    pub fn taken(&self, seat: &Seat, max: usize) -> Taken {
        if self
            .by_client
            .values()
            .any(|s| s.seat.character.eq_ignore_ascii_case(&seat.character))
        {
            return Taken::Character;
        }
        if self.by_client.len() >= max {
            return Taken::Full;
        }
        Taken::Free
    }

    pub fn insert(
        &mut self,
        client: ClientId,
        seat: Seat,
        base: D2s,
        snapshot: Arc<Mutex<JoinSnapshot>>,
        refused: SeatRefusal,
    ) {
        self.by_client.insert(
            client,
            SeatState {
                seat,
                base,
                admitted: std::time::Instant::now(),
                entered: false,
                saves: 0,
                snapshot,
                refused,
                transfer: None,
            },
        );
    }

    pub fn get(&self, client: ClientId) -> Option<&SeatState> {
        self.by_client.get(&client)
    }

    pub fn get_mut(&mut self, client: ClientId) -> Option<&mut SeatState> {
        self.by_client.get_mut(&client)
    }

    /// The client seated with `lease`.
    pub fn client_of_lease(&self, lease: u64) -> Option<ClientId> {
        self.by_client
            .iter()
            .find(|(_, s)| s.seat.lease == lease)
            .map(|(&c, _)| c)
    }

    /// The seat of the character `name` (any case).
    pub fn client_of(&self, name: &str) -> Option<ClientId> {
        self.by_client
            .iter()
            .find(|(_, s)| s.seat.character.eq_ignore_ascii_case(name))
            .map(|(&c, _)| c)
    }

    /// The seated characters' lowercase names.
    pub fn name_keys(&self) -> std::collections::BTreeSet<String> {
        self.by_client
            .values()
            .map(|s| s.seat.character.to_ascii_lowercase())
            .collect()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&ClientId, &mut SeatState)> {
        self.by_client.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.by_client.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_client.is_empty()
    }

    /// Takes out the seats whose client is not in `held`.
    pub fn remove_missing(&mut self, held: &[ClientId]) -> Vec<SeatState> {
        let gone: Vec<ClientId> = self
            .by_client
            .keys()
            .copied()
            .filter(|c| !held.contains(c))
            .collect();
        gone.into_iter()
            .filter_map(|c| self.by_client.remove(&c))
            .collect()
    }
}

/// The character each joining client plays: its seat's. A client with no
/// seat (never the case: the hub only reads seated clients) is refused.
pub(super) fn characters(seats: Rc<RefCell<Seats>>, spec: GameSpec) -> Characters {
    Box::new(move |client, r: &CreateGame| {
        let seats = seats.borrow();
        let Some(s) = seats.get(client) else {
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
        match &s.seat.new {
            Some(new) => Character::Named(NewCharacter {
                class: new.class,
                name: name16(&s.seat.character),
                difficulty: spec.difficulty,
            }),
            None => Character::Save(
                Box::new(s.base.clone()),
                LoadContext {
                    difficulty: spec.difficulty,
                    map_seed_applies: false,
                },
            ),
        }
    })
}

/// A name as the 16-byte NUL-padded field of the game's messages.
fn name16(name: &str) -> [u8; 16] {
    let mut b = [0u8; 16];
    let n = name.len().min(15);
    b[..n].copy_from_slice(&name.as_bytes()[..n]);
    b
}

/// A seat's game stream as the hub reads it.
///
/// The player's 0x67 join must name the seat's character (any case).
/// Another name is refused: the client is sent `Refused` with
/// [`ErrorCode::CHAR_MISMATCH`] (45), the seat is marked refused for the
/// game's report, and the hub drops the stream. The join is then
/// rewritten with the game's settings (its name, difficulty and kind), the
/// character's stored name and, for a new character, its class (the game
/// makes a new character's player of the join's class), so the client
/// decides none of them. A character upload (0x6C) is dropped: the client
/// never sends a character.
pub struct SeatStream {
    inner: Box<dyn AttachedStream>,
    character: String,
    /// A new character's class; `None` for a stored one (the game takes
    /// its class from the save).
    new_class: Option<u8>,
    game_name: [u8; 16],
    difficulty: u8,
    expansion: bool,
    /// Where a joining client's S→C messages are kept for a re-entry.
    snapshot: Option<Arc<Mutex<JoinSnapshot>>>,
    /// Where a refused join is noted for the game.
    refused: SeatRefusal,
}

impl SeatStream {
    pub(super) fn new(
        inner: Box<dyn AttachedStream>,
        seat: &Seat,
        spec: &GameSpec,
        refused: SeatRefusal,
    ) -> Self {
        Self::with_snapshot(inner, seat, spec, None, refused)
    }

    pub(super) fn capturing(
        inner: Box<dyn AttachedStream>,
        seat: &Seat,
        spec: &GameSpec,
        snapshot: Arc<Mutex<JoinSnapshot>>,
        refused: SeatRefusal,
    ) -> Self {
        Self::with_snapshot(inner, seat, spec, Some(snapshot), refused)
    }

    fn with_snapshot(
        inner: Box<dyn AttachedStream>,
        seat: &Seat,
        spec: &GameSpec,
        snapshot: Option<Arc<Mutex<JoinSnapshot>>>,
        refused: SeatRefusal,
    ) -> Self {
        Self {
            inner,
            character: seat.character.clone(),
            new_class: seat.new.as_ref().map(|n| n.class),
            game_name: name16(&spec.name),
            difficulty: spec.difficulty,
            expansion: spec.expansion,
            snapshot,
            refused,
        }
    }

    /// Refuses the client's join with `code`: tells the client, notes it
    /// for the game, and ends the stream.
    fn refuse(&mut self, code: ErrorCode) -> NetError {
        self.refused.set(code);
        let _ = self.inner.send_body(Refused::new(code).body());
        NetError::Refused(code)
    }

    /// The seat's version of a 0x67 join; [`ErrorCode::CHAR_MISMATCH`] if
    /// it names another character, `MALFORMED` if it doesn't decode.
    fn seat_join(&self, msg: &[u8]) -> Result<Vec<u8>, ErrorCode> {
        let mut r = CreateGame::decode(msg).map_err(|_| ErrorCode::MALFORMED)?;
        let n = r.char_name.iter().position(|&b| b == 0).unwrap_or(16);
        let named = std::str::from_utf8(&r.char_name[..n]).map_err(|_| ErrorCode::MALFORMED)?;
        if !named.eq_ignore_ascii_case(&self.character) {
            return Err(ErrorCode::CHAR_MISMATCH);
        }
        r.char_name = name16(&self.character);
        if let Some(class) = self.new_class {
            r.class = class;
        }
        r.game_name = self.game_name;
        r.difficulty = self.difficulty;
        r.flags &= !(create_flags::EXPANSION | create_flags::LADDER);
        if self.expansion {
            r.flags |= create_flags::EXPANSION;
        }
        Ok(r.encode().to_vec())
    }
}

impl Transport for SeatStream {
    fn send(&mut self, p: Packet) -> Result<(), NetError> {
        if let (Some(snapshot), Packet::Deliver(m)) = (&self.snapshot, &p) {
            snapshot.lock().unwrap_or_else(|e| e.into_inner()).record(m);
        }
        self.inner.send(p)
    }

    fn recv(&mut self) -> Result<Option<Packet>, NetError> {
        loop {
            match self.inner.recv()? {
                Some(Packet::Send {
                    lane: Lane::System,
                    msg,
                }) if msg.first() == Some(&CREATE_GAME) => {
                    let msg = match self.seat_join(&msg) {
                        Ok(m) => m,
                        Err(ErrorCode::CHAR_MISMATCH) => {
                            return Err(self.refuse(ErrorCode::CHAR_MISMATCH))
                        }
                        Err(_) => return Err(NetError::Malformed),
                    };
                    return Ok(Some(Packet::Send {
                        lane: Lane::System,
                        msg,
                    }));
                }
                Some(Packet::Send { msg, .. }) if msg.first() == Some(&SAVE_UPLOAD) => continue,
                other => return Ok(other),
            }
        }
    }

    fn flush(&mut self) -> Result<(), NetError> {
        self.inner.flush()
    }
}

/// The system messages the session flow leaves to the host: the ping is
/// answered with the pong; the rest decide nothing on a server.
#[derive(Debug, Default)]
pub struct ServerSession;

/// S→C 0x8F, the pong: the id and 32 zero bytes.
const PONG: [u8; 33] = {
    let mut b = [0; 33];
    b[0] = 0x8F;
    b
};

impl SessionHandler for ServerSession {
    fn system_message(
        &mut self,
        client: ClientId,
        msg: &[u8],
        _size: usize,
        out: &mut dyn MessageSink,
    ) {
        if msg.first() == Some(&PING) {
            let _ = out.queue(client, &PONG);
        }
    }
}

/// The game's character writer: a client's character is read from the
/// running game, laid over its seat's base and written in the game's
/// character format ([`ServerStore::codec`]); the record is handed to the
/// sink, and the game goes
/// on. [`GameEvent::Saved`] follows once the sink has stored it.
pub(super) struct ServerStore {
    pub seats: Rc<RefCell<Seats>>,
    pub tables: Arc<dyn SaveTables + Send + Sync>,
    /// The game's character format.
    pub codec: Arc<dyn CharacterCodec>,
    pub appearance: Option<Arc<AppearanceTables>>,
    pub events: mpsc::Sender<GameEvent>,
    pub sink: Arc<dyn SaveSink>,
    pub log: Arc<ServerLog>,
    pub game: u32,
    /// The game's journal: each save's digest goes in.
    pub journal: Option<Recorder>,
    /// The stash transfer the next save of its client goes with.
    pub tag: Tag,
    /// Where a transfer's outcome goes back to the game thread.
    pub settled: std::sync::mpsc::Sender<Settled>,
}

impl CharacterStore<WorldSim<LocalSeams>, World> for ServerStore {
    fn save(&mut self, sim: &mut Sim, client: ClientId) -> Result<(), String> {
        let Some((seat, base, waiting)) = self
            .seats
            .borrow()
            .get(client)
            .map(|s| (s.seat.clone(), s.base.clone(), s.transfer.is_some()))
        else {
            return Ok(());
        };
        let tagged = {
            let mut tag = self.tag.borrow_mut();
            match tag.take() {
                Some((c, t)) if c == client => Some(t),
                other => {
                    *tag = other;
                    None
                }
            }
        };
        if waiting && tagged.is_none() {
            // A transfer's save waits for its answer: no save until then.
            return Ok(());
        }
        let Some(player) = sim.player_of(client) else {
            return Ok(());
        };
        let live = character_save::read_live(sim, player);
        if base.header.version < d2s::VERSION
            && character_save::old_items_pass_through(&base, &live)
        {
            return Err(format!(
                "{}: a version {:#x} character whose items the game cannot rewrite is not overwritten",
                seat.character, base.header.version
            ));
        }
        let now = character_save::now_secs();
        let mut save = character_save::apply_live(&base, &live, now);
        if let Some(t) = &self.appearance {
            character_save::rebuild_appearance(sim, player, t, &mut save);
        }
        let record = self.codec.write(&save, &*self.tables)?;
        let summary = summary(&seat.character, &save, now);
        // The character has a record now: it is no longer new.
        let mut seq = 1;
        if let Some(s) = self.seats.borrow_mut().by_client.get_mut(&client) {
            s.seat.new = None;
            s.saves = s.saves.saturating_add(1);
            seq = s.saves;
        }
        if let Some(r) = &self.journal {
            match save_digest(&save, &*self.tables) {
                Ok(d) => r.saved(seat.lease, seq, d),
                Err(e) => self.log.line(&format!(
                    "game {}: {}: save {seq} has no journal digest: {e}",
                    self.game, seat.character
                )),
            }
        }
        let (events, log, game) = (self.events.clone(), self.log.clone(), self.game);
        let (account, saved) = (seat.account, summary.clone());
        let settle = tagged
            .as_ref()
            .map(|t| (self.settled.clone(), t.req, t.answer.clone()));
        self.sink.save(
            SaveRecord {
                game,
                account,
                character: seat.character,
                seq,
                record,
                summary,
                transfer: tagged.map(Box::new),
            },
            Box::new(move |stored| {
                if let Some((tx, req, answer)) = settle {
                    transfer::settle(&tx, client, req, &stored, &answer);
                }
                match stored {
                    Ok(()) => {
                        let _ = events.blocking_send(GameEvent::Saved {
                            game,
                            account,
                            summary: saved,
                        });
                    }
                    Err(e) => log.line(&format!("game {game}: {}: save failed: {e}", saved.name)),
                }
            }),
        );
        Ok(())
    }
}

/// What character select and the lobby read of a saved character.
pub fn summary(name: &str, save: &D2s, now: u32) -> CharSummary {
    let h = &save.header;
    let mut look = h.components.to_vec();
    look.extend_from_slice(&h.colours);
    CharSummary {
        name: name.to_owned(),
        class: h.class,
        level: h.level,
        status: h.status,
        difficulties: d2s::status::difficulties(h.status),
        look,
        last_played: u64::from(now),
        season: 0,
    }
}
