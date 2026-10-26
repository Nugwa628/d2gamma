//! One game on its thread: the built game behind a hub, the seats, and
//! the frame loop's bookkeeping (leaves, player counts, saves, close).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use d2_formats::d2s::appearance::AppearanceTables;
use d2_formats::d2s::{self, D2s};
use d2_journal::format::{leave, GameSpec as JournalSpec, Start};
use d2_journal::server::state_hash;
use d2_net::msg::ErrorCode;
use d2_net::wire::{Refused, Welcome};
use d2_net::{Packet, ResumeToken, Transport};
use d2_proto::dev::DevCommand;
use d2_server::adapters::session_flow::SessionFlow;
use d2_server::adapters::ProtoSizes;
use d2_server::dev::{DevCatalog, DevServer};
use d2_server::game::realm::{IdBlock, RealmStart};
use d2_server::game::{character_save, loader, CharacterCodec, GameBuilder, GameData, Sim};
use d2_server::game_thread::Flow;
use d2_server::host::{Host, SystemClock};
use d2_server::hub::Hub;
use d2_server::seams::{ClientId, Intents, MessageSink, Tick};
use tokio::sync::mpsc;

use super::journal::{join_digest, JournalSetup, Recorder};
use super::seat::{
    self, JoinSnapshot, SeatRefusal, SeatStream, Seats, ServerSession, ServerStore, Taken,
};
use super::transfer::{self, Outcome, Settled, Tag};
use super::{AttachedStream, GameEvent, GameRules, GameSpec, GamesConfig, SaveSink, Seat};
use d2_service::log::ServerLog;

/// Making the saves on the game thread for longer than this is logged: it
/// stalls the game's tick. Storing them happens in the sink, off the game
/// thread.
const SLOW_SAVE: Duration = Duration::from_millis(20);

/// A frame slower than this missed its tick.
const TICK: Duration = Duration::from_micros(16_667);

/// How long a seated player may take to send its join.
const JOIN_WAIT: Duration = Duration::from_secs(30);

type GameHub = Hub<Sim, ProtoSizes, ServerSession, SystemClock, Box<dyn Transport + Send>>;

/// What a game thread is built from.
pub(super) struct Parts {
    pub spec: GameSpec,
    pub cfg: GamesConfig,
    pub data: GameData,
    /// The ruleset the game is built with and its save format.
    pub rules: GameRules,
    pub appearance: Option<Arc<AppearanceTables>>,
    pub log: Arc<ServerLog>,
    pub events: mpsc::Sender<GameEvent>,
    pub players: Arc<AtomicU8>,
    pub seated: Arc<Mutex<BTreeSet<String>>>,
    pub overruns: Arc<AtomicU64>,
    pub shutdown: Arc<AtomicBool>,
    pub closed: Arc<AtomicBool>,
    /// The dev command names, when the server runs `dev = test`.
    pub dev: Option<Arc<DevCatalog>>,
    /// Where the players' saves go.
    pub sink: Arc<dyn SaveSink>,
    /// The seed the game's map is built with.
    pub seed: u32,
    /// The first block of realm item IDs; `None`: the items' init seeds.
    pub item_ids: Option<IdBlock>,
    /// Where the game's journal goes; `None`: not recorded.
    pub journal: Option<Arc<JournalSetup>>,
}

/// A running game. Lives on its game thread only.
pub(crate) struct Instance {
    pub(super) hub: GameHub,
    seats: Rc<RefCell<Seats>>,
    spec: GameSpec,
    cfg: GamesConfig,
    data: GameData,
    /// The game's character format: the seats' records are read with it.
    codec: Arc<dyn CharacterCodec>,
    log: Arc<ServerLog>,
    events: mpsc::Sender<GameEvent>,
    /// Where the saves go; `Left` and `Closed` go out through it, after the
    /// saves before them are stored.
    sink: Arc<dyn SaveSink>,
    players: Arc<AtomicU8>,
    players_sent: u8,
    seated_names: Arc<Mutex<BTreeSet<String>>>,
    overruns: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
    closed: Arc<AtomicBool>,
    next_client: ClientId,
    last_save: Instant,
    /// A seat was taken: from then on, no seats means the game is over.
    seated: bool,
    /// The dev command handlers (`dev = test` only).
    dev: Option<DevServer>,
    /// The game's journal, until it ends.
    pub(super) journal: Option<Recorder>,
    /// The stash transfer the next save of its client goes with.
    tag: Tag,
    /// The outcomes of the transfers' saves, from the sink.
    settled: std::sync::mpsc::Receiver<Settled>,
    /// Test-only: the next resume panics after it resolves its session,
    /// to check that a resume panic drops only that session.
    #[cfg(test)]
    pub(super) panic_resume: bool,
}

impl Instance {
    /// Builds the game of `parts.spec` (on the game thread).
    pub(super) fn build(parts: Parts) -> Result<Instance, String> {
        let Parts {
            spec,
            cfg,
            data,
            rules,
            appearance,
            log,
            events,
            players,
            seated,
            overruns,
            shutdown,
            closed,
            dev,
            sink,
            seed,
            item_ids,
            journal,
        } = parts;
        // The game's difficulty comes from its builder's character; the
        // players' characters come from the seats (the loader below).
        let character = d2_server::game::Character::New.with_difficulty(spec.difficulty);
        let builder = GameBuilder::new(data.clone())
            .seed(seed)
            .character(character)
            .rules(rules.rules);
        let builder = match item_ids {
            Some(block) => builder
                .realm(RealmStart {
                    seed,
                    item_ids: block,
                })
                .map_err(|e| format!("item ids {}..+{}: {e}", block.first, block.count))?,
            None => builder,
        };
        let mut sim = builder.build().map_err(|e| e.to_string())?;
        if spec.hardcore {
            sim.events.action.hooks().x.hardcore = true;
        }
        let journal = journal.map(|setup| {
            let start = Start {
                version: setup.version.clone(),
                data_hash: setup.data_hash,
                seed,
                spec: JournalSpec {
                    id: spec.id,
                    name: spec.name.clone(),
                    difficulty: spec.difficulty,
                    hardcore: spec.hardcore,
                    expansion: spec.expansion,
                    max_players: spec.max_players,
                },
                tick_rate: sim.tick_rate().hz(),
                player_hosted: false,
                host_client: None,
            };
            let (rec, tap) = Recorder::new(start, sim.frame(), setup.cfg);
            rec.set_sink((setup.sinks)(spec.id));
            if let Some(b) = item_ids {
                rec.item_ids(b.first, b.count);
            }
            (rec, tap)
        });
        let seats = Rc::new(RefCell::new(Seats::default()));
        let tag = Tag::default();
        let (settled_tx, settled) = std::sync::mpsc::channel();
        sim.set_session(
            SessionFlow::new(loader(seat::characters(seats.clone(), spec.clone())))
                .with_max_players(usize::from(spec.max_players)),
        );
        let GameData::Live(live) = &data;
        sim.set_storage(Box::new(ServerStore {
            seats: seats.clone(),
            tables: Arc::new(live.save.clone()),
            codec: rules.codec.clone(),
            appearance,
            events: events.clone(),
            sink: sink.clone(),
            log: log.clone(),
            game: spec.id,
            journal: journal.as_ref().map(|(rec, _)| rec.clone()),
            tag: tag.clone(),
            settled: settled_tx,
        }));
        let mut host = Host::new(sim, ProtoSizes, ServerSession, SystemClock::default());
        let journal = journal.map(|(rec, tap)| {
            host.set_packet_observer(Some(Box::new(tap)));
            rec
        });
        let mut hub: GameHub = Hub::new(host);
        hub.set_grace(cfg.grace.as_millis().min(u128::from(u32::MAX)) as u32);
        Ok(Instance {
            hub,
            seats,
            spec,
            cfg,
            data,
            codec: rules.codec,
            log,
            events,
            sink,
            players,
            players_sent: 0,
            seated_names: seated,
            overruns,
            shutdown,
            closed,
            next_client: 0,
            last_save: Instant::now(),
            seated: false,
            dev: dev.map(|c| DevServer::new((*c).clone())),
            journal,
            tag,
            settled,
            #[cfg(test)]
            panic_resume: false,
        })
    }

    /// Test-only: makes the next resume panic after it resolves its
    /// session (the panic guard's regression check).
    #[cfg(test)]
    pub(super) fn panic_next_resume(&mut self) {
        self.panic_resume = true;
    }

    fn line(&self, msg: &str) {
        self.log.line(&format!("game {}: {msg}", self.spec.id));
    }

    fn send(&self, ev: GameEvent) {
        // The lobby may be gone (server shutdown): nothing to tell then.
        let _ = self.events.blocking_send(ev);
    }

    /// Sends `ev` once every save handed off before it is stored: the lobby
    /// hears of a leave or a close only after the characters' last saves.
    fn send_after_saves(&self, ev: GameEvent) {
        let events = self.events.clone();
        self.sink.then(Box::new(move || {
            let _ = events.blocking_send(ev);
        }));
    }

    /// Takes a seat: checks it, loads its character, answers the attach and
    /// connects the stream. A panic here closes the game.
    pub(super) fn admit(
        &mut self,
        seat: Seat,
        stream: Box<dyn AttachedStream>,
    ) -> Result<(), ErrorCode> {
        match catch_unwind(AssertUnwindSafe(|| self.admit_inner(seat, stream))) {
            Ok(r) => r,
            Err(_) => {
                self.line("panicked while admitting a player; closing");
                self.close_after_failure();
                Err(ErrorCode::GAME_CLOSED)
            }
        }
    }

    fn admit_inner(
        &mut self,
        seat: Seat,
        mut stream: Box<dyn AttachedStream>,
    ) -> Result<(), ErrorCode> {
        let checked = self.check_seat(&seat).and_then(|()| self.load_base(&seat));
        let base = match checked {
            Ok(base) => base,
            Err(code) => {
                let _ = stream.send_body(Refused::new(code).body());
                return Err(code);
            }
        };
        let client = self.next_client;
        self.next_client += 1;
        let token = ResumeToken::random();
        let welcome = Welcome {
            client,
            tick_rate: self.hub.host.game.tick_rate().hz(),
            max_players: u32::from(self.spec.max_players),
            resume: token,
        };
        stream.send_body(welcome.body()).map_err(|e| {
            self.line(&format!(
                "{}: answering the attach failed: {e}",
                seat.character
            ));
            ErrorCode::OTHER
        })?;
        let snapshot = Arc::new(Mutex::new(JoinSnapshot::default()));
        let refused = SeatRefusal::default();
        let filtered =
            SeatStream::capturing(stream, &seat, &self.spec, snapshot.clone(), refused.clone());
        if let Some(r) = &self.journal {
            r.seat(client, seat.lease, join_digest(&seat));
        }
        self.hub
            .connect_with_token(client, Box::new(filtered), token);
        self.line(&format!(
            "{} ({}) joins as client {client}{}",
            seat.character,
            seat.account,
            if seat.new.is_some() { ", new" } else { "" }
        ));
        self.seats
            .borrow_mut()
            .insert(client, seat, base, snapshot, refused);
        self.seated = true;
        self.update_players();
        Ok(())
    }

    fn check_seat(&self, seat: &Seat) -> Result<(), ErrorCode> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ErrorCode::GAME_CLOSED);
        }
        match self
            .seats
            .borrow()
            .taken(seat, usize::from(self.spec.max_players))
        {
            Taken::Free => Ok(()),
            Taken::Full => Err(ErrorCode::GAME_FULL),
            Taken::Character => Err(ErrorCode::CHAR_IN_GAME),
        }
    }

    /// Takes `character` out of the game: saved, then gone as on a loss.
    pub(super) fn kick(&mut self, character: &str) -> bool {
        let Some(client) = self.seats.borrow().client_of(character) else {
            return false;
        };
        self.leaving(client, leave::KICKED);
        drop(self.hub.disconnect(client));
        self.note_leavers();
        self.update_players();
        true
    }

    /// A server line in every player's chat window (the Game tab), with
    /// the next frame: how many players it went to.
    pub(super) fn announce(&mut self, text: &str) -> usize {
        let msg = system_line(text);
        let clients: Vec<ClientId> = self.hub.clients().collect();
        clients
            .into_iter()
            .filter(|&c| self.hub.host.buffers.queue(c, &msg).is_ok())
            .count()
    }

    /// Runs one dev command for `character` now, between two frames: the
    /// text of a success, or why it failed (contracts 10.1).
    pub(super) fn dev_command(
        &mut self,
        character: &str,
        cmd: &DevCommand,
    ) -> Result<String, String> {
        let Some(dev) = self.dev.as_mut() else {
            return Err("disabled (the server doesn't run dev = test)".into());
        };
        let client =
            self.seats.borrow().client_of(character).ok_or_else(|| {
                format!("no character named {character:?} in game {}", self.spec.id)
            })?;
        let player = self
            .hub
            .host
            .game
            .player_of(client)
            .ok_or_else(|| format!("{character} hasn't joined the game yet"))?;
        let r = dev.apply(&mut self.hub.host.game, Some(player), cmd);
        if r.ok {
            Ok(r.text)
        } else {
            Err(r.text)
        }
    }

    /// The save the seat's character is laid over: its stored record, read
    /// with the game's tables, or a new character's stub.
    fn load_base(&self, seat: &Seat) -> Result<D2s, ErrorCode> {
        if let Some(new) = &seat.new {
            let mut base =
                character_save::fresh(new.class, seat.character.as_bytes(), self.spec.difficulty);
            if new.hardcore {
                base.header.status |= d2s::status::HARDCORE;
            }
            // The progression set before the first game (--set-progression)
            // is part of the character from its first record: the game never
            // lowers it, so its saves keep it.
            base.header.status |= u16::from(new.progression & 0x1F) << 8;
            return Ok(base);
        }
        let Some(record) = &seat.record else {
            self.line(&format!("{}: no stored record", seat.character));
            return Err(ErrorCode::UNKNOWN_CHAR);
        };
        let GameData::Live(live) = &self.data;
        self.codec
            .read(live, record, self.spec.difficulty)
            .map_err(|e| {
                self.line(&format!(
                    "{}: the character doesn't load: {e}",
                    seat.character
                ));
                ErrorCode::OTHER
            })
    }

    /// Gives a held slot back to a returning stream as a re-entry (contracts
    /// 3.7): the same client, the player unit, its position and the game
    /// state stay, a new resume token is issued, any open window is closed,
    /// the parked backlog is dropped, and the client is sent the game's
    /// state again.
    ///
    /// A panic drops only this session (its stream and slot); the game and
    /// the other players keep running. The resume itself changes nothing
    /// the game trusts, so unlike a frame panic it does not close the game.
    pub(super) fn resume(
        &mut self,
        token: ResumeToken,
        stream: Box<dyn AttachedStream>,
    ) -> Result<(), ErrorCode> {
        let client = self.hub.resumable(&token);
        match catch_unwind(AssertUnwindSafe(|| self.resume_inner(token, stream))) {
            Ok(r) => r,
            Err(_) => {
                self.line("panicked in a resume; dropping that session");
                if let Some(client) = client {
                    self.drop_session(client);
                }
                Err(ErrorCode::OTHER)
            }
        }
    }

    /// Whether a slot is held for the game resume `token`.
    pub(super) fn holds_resume(&self, token: &ResumeToken) -> bool {
        self.hub.resumable(token).is_some()
    }

    /// Takes a client out of the game without closing it: the leave saves
    /// the character and the others keep playing.
    fn drop_session(&mut self, client: ClientId) {
        drop(self.hub.disconnect(client));
        self.note_leavers();
        self.update_players();
    }

    fn resume_inner(
        &mut self,
        token: ResumeToken,
        mut stream: Box<dyn AttachedStream>,
    ) -> Result<(), ErrorCode> {
        let Some(client) = self.hub.resumable(&token) else {
            let _ = stream.send_body(Refused::new(ErrorCode::UNKNOWN_RESUME).body());
            return Err(ErrorCode::UNKNOWN_RESUME);
        };
        let (seat, snapshot, refused) = match self
            .seats
            .borrow()
            .get(client)
            .map(|s| (s.seat.clone(), s.snapshot.clone(), s.refused.clone()))
        {
            Some(x) => x,
            None => {
                let _ = stream.send_body(Refused::new(ErrorCode::UNKNOWN_RESUME).body());
                return Err(ErrorCode::UNKNOWN_RESUME);
            }
        };
        #[cfg(test)]
        if std::mem::take(&mut self.panic_resume) {
            panic!("a resume fault (test)");
        }
        let new_token = ResumeToken::random();
        let welcome = Welcome {
            client,
            tick_rate: self.hub.host.game.tick_rate().hz(),
            max_players: u32::from(self.spec.max_players),
            resume: new_token,
        };
        if stream.send_body(welcome.body()).is_err() {
            return Err(ErrorCode::OTHER);
        }
        // A window the player left open must not survive the loss, and its
        // close must be told to the other player of a trade (contracts v12
        // 3.7). The close is done in the game before the replay; the
        // messages the resuming client expects follow the replay so they
        // end any window the replay opened.
        let closes = self.hub.host.game.close_client_interactions(client);
        // The client cleared its world: replay everything it was sent since
        // it joined, in order (the server-side journal, complete: the hub
        // backlog missed what a dying connection swallowed). That rebuilds
        // the character state (items, stats, quests, skills, waypoints,
        // mercenary, party) and the world as they are now.
        {
            let snapshot = snapshot.lock().unwrap_or_else(|e| e.into_inner());
            for m in snapshot.messages() {
                if stream.send(Packet::Deliver(m.clone())).is_err() {
                    return Err(ErrorCode::OTHER);
                }
            }
        }
        for m in closes {
            if stream.send(Packet::Deliver(m)).is_err() {
                return Err(ErrorCode::OTHER);
            }
        }
        let filtered = SeatStream::new(stream, &seat, &self.spec, refused);
        match self.hub.reattach(&token, new_token, Box::new(filtered)) {
            Some(_) => {
                // What the journal cannot have: a character created in this
                // game never got the loaded character's 0x94, so its live
                // base skill levels go out now (contracts v12 3.7).
                self.hub.host.game.reenter_character_state(client);
                self.line(&format!(
                    "{} is back (client {client}), re-entered",
                    seat.character
                ));
                Ok(())
            }
            None => Err(ErrorCode::UNKNOWN_RESUME),
        }
    }

    /// One loop turn: a hub frame, then the bookkeeping. A panic closes the
    /// game.
    pub(super) fn frame(&mut self) -> Flow {
        match catch_unwind(AssertUnwindSafe(|| self.frame_inner())) {
            Ok(flow) => flow,
            Err(_) => {
                self.line("panicked in a frame; closing (characters keep their last save)");
                self.close_after_failure();
                Flow::Stop
            }
        }
    }

    fn frame_inner(&mut self) -> Flow {
        if self.shutdown.load(Ordering::SeqCst) {
            self.close(ErrorCode::GAME_CLOSED, "the server is stopping");
            return Flow::Stop;
        }
        // The dev switches act before the tick, as the client's do: god
        // heals, free mana refills, no-cooldown clears the skill delay.
        if self.dev.is_some() {
            let clients: Vec<ClientId> = self.hub.clients().collect();
            for c in clients {
                if let Some(p) = self.hub.host.game.player_of(c) {
                    d2_server::dev::upkeep(&mut self.hub.host.game, p);
                }
            }
        }
        let t = Instant::now();
        let served = self.hub.serve_frame();
        if t.elapsed() > TICK {
            self.overruns.fetch_add(1, Ordering::Relaxed);
        }
        if let Err(e) = served {
            self.line(&format!("frame failed: {e}"));
            self.close(ErrorCode::GAME_CLOSED, "the game failed");
            return Flow::Stop;
        }
        if let Some(r) = &self.journal {
            let sim = &self.hub.host.game;
            r.after_frame(sim.frame(), || state_hash(sim));
        }
        // A player that left cleanly (C→S 0x69) ends its stream; the hub
        // parks it like a lost one. Its record is gone, so it can't come
        // back: free the slot now.
        let gone: Vec<ClientId> = self
            .hub
            .parked()
            .filter(|&c| self.hub.host.game.sim_client(c).is_none())
            .collect();
        for c in gone {
            self.leaving(c, leave::LEFT);
            self.hub.disconnect(c);
        }
        // A connected player whose record went (its join was refused, or it
        // left and kept the stream open), or that never joined in time, has
        // no place in the game any more.
        let mut out = Vec::new();
        for (&c, s) in self.seats.borrow_mut().iter_mut() {
            if self.hub.host.game.sim_client(c).is_some() {
                s.entered = true;
            } else if s.entered || s.admitted.elapsed() >= JOIN_WAIT {
                out.push(c);
            }
        }
        for c in out {
            if self.hub.clients().any(|x| x == c) {
                self.leaving(c, leave::LEFT);
                self.hub.disconnect(c);
            }
        }
        for (client, e) in self.hub.take_dropped() {
            self.line(&format!("client {client} dropped: {e}"));
        }
        self.settle_transfers();
        for _ in 0..self.hub.host.game.take_item_ids_wanted() {
            self.send(GameEvent::ItemIdsWanted { game: self.spec.id });
        }
        self.note_leavers();
        for (client, fault) in std::mem::take(&mut self.hub.host.game.save_faults) {
            self.line(&format!("client {client}: save failed: {fault}"));
        }
        self.update_players();
        if self.last_save.elapsed() >= self.cfg.save_interval {
            self.save_all();
        }
        if self.seated && self.seats.borrow().is_empty() {
            self.close(ErrorCode::GAME_CLOSED, "the last player left");
            return Flow::Stop;
        }
        Flow::Go
    }

    /// Seats whose client the hub no longer holds (neither connected nor
    /// parked) have left: their characters were saved by the leave.
    fn note_leavers(&mut self) {
        let held: Vec<ClientId> = self.hub.clients().chain(self.hub.parked()).collect();
        let left = self.seats.borrow_mut().remove_missing(&held);
        for s in left {
            if let Some(code) = s.refused.code() {
                self.line(&format!(
                    "{}: join refused ({code}), the seat ends",
                    s.seat.character
                ));
                self.send(GameEvent::SeatRefused {
                    game: self.spec.id,
                    account: s.seat.account,
                    character: s.seat.character,
                    code,
                });
                continue;
            }
            self.line(&format!("{} left", s.seat.character));
            self.send_after_saves(GameEvent::Left {
                game: self.spec.id,
                account: s.seat.account,
                character: s.seat.character,
            });
        }
    }

    fn update_players(&mut self) {
        let names = self.seats.borrow().name_keys();
        *self.seated_names.lock().unwrap_or_else(|e| e.into_inner()) = names;
        let n = self.seats.borrow().len().min(usize::from(u8::MAX)) as u8;
        self.players.store(n, Ordering::SeqCst);
        if n != self.players_sent {
            self.players_sent = n;
            self.send(GameEvent::Players {
                game: self.spec.id,
                players: n,
            });
        }
    }

    /// Adds a granted block of realm item IDs (between frames).
    pub(super) fn grant_item_ids(&mut self, block: IdBlock) -> Result<(), String> {
        self.hub
            .host
            .game
            .grant_item_ids(block)
            .map_err(|e| format!("item ids {}..+{}: {e}", block.first, block.count))?;
        if let Some(r) = &self.journal {
            r.item_ids(block.first, block.count);
        }
        self.line(&format!(
            "item ids {}..+{} granted",
            block.first, block.count
        ));
        Ok(())
    }

    /// A stash transfer takes goods out of the character of `lease` (see
    /// [`crate::transfer`]); between frames. `Err`: nothing changed.
    pub(super) fn item_out(
        &mut self,
        lease: u64,
        req: u64,
        unit: u32,
        to: u8,
        gold: u64,
    ) -> Result<(), ErrorCode> {
        let client = self.transfer_client(lease)?;
        let sim = &mut self.hub.host.game;
        let pending = transfer::out(
            sim,
            client,
            self.journal.as_ref(),
            &self.tag,
            transfer::OutAsk {
                req,
                unit,
                to,
                gold,
                invent: self.cfg.invent_stash_items,
            },
        )?;
        self.after_transfer(client, pending);
        Ok(())
    }

    /// A stash transfer brings goods to the character of `lease`; between
    /// frames. The result: 0 placed (the save goes up with the transfer),
    /// 1 no room, 2 refused.
    pub(super) fn item_in(
        &mut self,
        lease: u64,
        req: u64,
        arriving: transfer::Arriving,
    ) -> Result<u8, ErrorCode> {
        let client = self.transfer_client(lease)?;
        let sim = &mut self.hub.host.game;
        let (result, pending) =
            transfer::place(sim, client, self.journal.as_ref(), &self.tag, req, arriving)?;
        if let Some(pending) = pending {
            self.after_transfer(client, pending);
        }
        Ok(result)
    }

    /// The client of `lease`, when it may start a transfer.
    fn transfer_client(&self, lease: u64) -> Result<ClientId, ErrorCode> {
        let seats = self.seats.borrow();
        let client = seats
            .client_of_lease(lease)
            .ok_or(transfer::code::NOT_IN_GAME)?;
        match seats.get(client) {
            Some(s) if s.transfer.is_some() => Err(transfer::code::ITEM_HELD),
            Some(_) => Ok(client),
            None => Err(transfer::code::NOT_IN_GAME),
        }
    }

    /// The transfer's goods moved: its save goes up with it, and the seat
    /// waits for the answer.
    fn after_transfer(&mut self, client: ClientId, pending: transfer::Pending) {
        let req = pending.req;
        if let Some(s) = self.seats.borrow_mut().get_mut(client) {
            s.transfer = Some(pending);
        }
        if let Err(e) = transfer::save(&mut self.hub.host.game, client, &self.tag) {
            // No save went up: the transfer is undone at once.
            self.line(&format!("client {client}: transfer {req}: no save: {e}"));
            self.settle(Settled {
                client,
                req,
                outcome: Outcome::Refused(transfer::code::NOT_IN_GAME),
            });
        }
    }

    /// Takes the outcomes of the transfers' saves (between frames).
    fn settle_transfers(&mut self) {
        let settled: Vec<Settled> = self.settled.try_iter().collect();
        for s in settled {
            self.settle(s);
        }
    }

    fn settle(&mut self, s: Settled) {
        let pending = {
            let mut seats = self.seats.borrow_mut();
            let Some(seat) = seats.get_mut(s.client) else {
                return;
            };
            match seat.transfer.take() {
                Some(p) if p.req == s.req => p,
                other => {
                    seat.transfer = other;
                    return;
                }
            }
        };
        match s.outcome {
            Outcome::Stored => {}
            Outcome::Refused(code) => {
                self.line(&format!(
                    "client {}: transfer {} refused ({code}): undone",
                    s.client, s.req
                ));
                let sim = &mut self.hub.host.game;
                if let Some(lost) = transfer::undo(sim, s.client, self.journal.as_ref(), &pending) {
                    self.line(&format!("client {}: transfer {}: {lost}", s.client, s.req));
                }
                let sim = &mut self.hub.host.game;
                if let Err(e) = transfer::save(sim, s.client, &self.tag) {
                    self.line(&format!("client {}: save failed: {e}", s.client));
                }
            }
            Outcome::Unanswered(e) => {
                // Unknown whether the main server stored it: the seat saves
                // nothing more (`crate::transfer`).
                self.line(&format!(
                    "client {}: transfer {} unanswered ({e}): no more saves",
                    s.client, s.req
                ));
                if let Some(seat) = self.seats.borrow_mut().get_mut(s.client) {
                    seat.transfer = Some(pending);
                }
            }
        }
    }

    /// The next leave of `client` is for this reason (its journal `Leave`
    /// record; without one a leave is a loss).
    fn leaving(&self, client: ClientId, how: u8) {
        if let Some(r) = &self.journal {
            r.leaving(client, how);
        }
    }

    /// Saves every player's character now (between frames).
    fn save_all(&mut self) {
        self.last_save = Instant::now();
        if self.seats.borrow().is_empty() {
            return;
        }
        let t = Instant::now();
        let results = self.hub.host.game.save_characters();
        let took = t.elapsed();
        for (client, r) in results {
            if let Err(e) = r {
                self.line(&format!("client {client}: save failed: {e}"));
            }
        }
        if took >= SLOW_SAVE {
            self.line(&format!("saving took {} ms", took.as_millis()));
        }
    }

    /// Ends the game: every player is saved and taken out (the game's own
    /// leave path), then [`GameEvent::Closed`].
    fn close(&mut self, reason: ErrorCode, why: &str) {
        let clients: Vec<ClientId> = self.hub.clients().chain(self.hub.parked()).collect();
        for c in clients {
            self.leaving(c, leave::CLOSED);
            // The leave saves every character still in the game.
            drop(self.hub.disconnect(c));
        }
        self.note_leavers();
        self.update_players();
        if let Some(r) = self.journal.take() {
            let sim = &self.hub.host.game;
            r.end(sim.frame(), reason.0, state_hash(sim));
        }
        self.line(&format!("closed: {why}"));
        self.send_closed(reason);
    }

    /// A failure: nothing more runs on this game's state, so no save; the
    /// characters keep their last one.
    fn close_after_failure(&mut self) {
        self.send_closed(ErrorCode::GAME_CLOSED);
    }

    fn send_closed(&mut self, reason: ErrorCode) {
        // A game that ends without closing (a failure) sends what its
        // journal has, without an end: the journal is cut off there.
        if let Some(r) = self.journal.take() {
            r.cut();
        }
        self.players.store(0, Ordering::SeqCst);
        self.seated_names
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        if !self.closed.swap(true, Ordering::SeqCst) {
            self.send_after_saves(GameEvent::Closed {
                game: self.spec.id,
                reason,
            });
        }
    }
}

impl Drop for Instance {
    /// A game thread that ends any other way (a panic in a job, the handle
    /// dropped) still tells the lobby.
    fn drop(&mut self) {
        self.send_closed(ErrorCode::GAME_CLOSED);
    }
}

/// The colour of a server line: gold.
const SYSTEM_LINE_COLOUR: u8 = 4;

/// The longest text of a server line.
const SYSTEM_LINE_MAX: usize = 255;

/// A server line in the chat window: S→C 0x26 type 4 (a plain line in the
/// colour byte at 8), no name, `text` (cut at [`SYSTEM_LINE_MAX`] bytes,
/// without NULs) and its NUL.
pub(super) fn system_line(text: &str) -> Vec<u8> {
    let mut m = vec![0x26, 4, 0, 2, 0, 0, 0, 0, SYSTEM_LINE_COLOUR, 0, 0];
    m.extend(text.bytes().filter(|&b| b != 0).take(SYSTEM_LINE_MAX));
    m.push(0);
    m
}
