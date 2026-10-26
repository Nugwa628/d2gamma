//! The game server's games (plan N4; contracts 11.3, 11.6). The main
//! server starts each game (`StartGame`, with its seed and its first block
//! of item ids) and offers each seat (`SeatOffer`, the character's record
//! on its own stream); the client then shows its ticket here and takes the
//! seat. The games are [`d2_games`]'s, as on the dedicated server; what
//! differs is where their characters come from and where their saves go:
//!
//! - **Seats**: an offer and its record are kept until the ticket for them
//!   arrives ([`NodeGames::take_seat`]); then the game is handed the seat
//!   and the game stream, and the main server hears `SeatTaken` (or
//!   `SeatRefused` with the game's code).
//! - **Saves**: each save the game makes goes up the link: the record on a
//!   stream of its own, then `Save`. The game waits for the main server's
//!   `SaveAck` before it counts the save as stored, so `Left` (sent once a
//!   player's last save is stored) comes after its save was taken. A save
//!   for a seat whose lease ended, or while unlinked, is not stored.
//! - **Events**: `Players`, `Left`, `SeatRefused` (a 0x67 join of another
//!   character: 45) and `GameClosed` go up the link as the games report
//!   them.
//! - `Kick` takes a character out of its game (it is saved, the main
//!   server refuses that save: the lease ended with the takeover), or
//!   withdraws its offer. `CloseGame` closes a game: `how` 0 saves its
//!   players first; 1 drops their leases here first, so nothing is sent.
//!
//! The game server keeps no character data: records live in memory from
//! their offer to their seat, and saves go to the main server only.
//!
//! - **Journals**: every game records its journal (plan N7); its chunks go
//!   up the link as they are cut ([`crate::journal`]), every
//!   `journal_chunk_s` of the welcome and at the end.
//! - **Item IDs**: a game numbers its items from the main server's blocks
//!   of realm item IDs: the first comes with `StartGame`; when a game runs
//!   low it asks (`ItemIdsWanted`), and the block of each `ItemIds` is
//!   granted to it between two frames.
//! - **Stash transfers** (contracts 11.13; plan N28): `ItemWanted` takes
//!   goods off a character between two frames and `ItemIn` places goods on
//!   it ([`d2_games::transfer`]); the character's save goes up with the
//!   transfer from the save sink, in order with its other saves: the
//!   record on its stream, then `ItemOut` or `ItemInDone` instead of
//!   `Save`, and the sink waits for `ItemOutAck`. A refusal goes back to
//!   the game, which undoes the transfer.
//!
//! A new link starts with no seats: the games of an earlier link are closed
//! without saves (their leases ended with it; plan N13 keeps them through a
//! short loss).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use d2_games::journal::{Digest, JournalChunk, JournalSetup, JournalSink, RecorderConfig};
use d2_games::transfer::Arriving;
use d2_games::{
    AccountId, AttachedStream, GameEvent, GameHandle, GameRules, GameSpec, Games, GamesConfig,
    IdBlock, NewChar, QueuedSink, SaveRecord, Seat, SeedSource, TransferGoods,
};
use d2_link::streams::{self, What};
use d2_link::LinkTx;
use d2_net::msg::node::{
    ItemIdBlock, Load, NewCharInfo, NodeGameSpec, NodeToRealm, NodeWelcome, RealmToNode, SaveInfo,
    SAVE_REASON_TRANSFER,
};
use d2_net::msg::stash;
use d2_net::msg::transfer::{placed, ItemIn, ItemInDone, ItemOut, ItemWanted};
use d2_net::msg::ErrorCode;
use d2_net::wire::Refused;
use d2_net::ResumeToken;
use d2_proto::dev::DevCommand;
use d2_quic::{FramedStream, PacketStream, QuicConn};
use d2_server::game::GameData;
use d2_service::log::ServerLog;
use d2_sim::tick::TickRate;
use d2_tickets::{TicketClaims, OFFER_TTL, OFFER_WAIT};
use tokio::sync::mpsc;

use crate::journal::{self, ChunkSink};
use crate::tickets::Tickets;

/// `SaveAck`'s code for a stored save.
const STORED: ErrorCode = ErrorCode(0);

/// How long a save waits for the main server's `SaveAck`.
pub const SAVE_ACK_WAIT: Duration = Duration::from_secs(15);

/// How often a seat's record is looked for while its ticket waits.
const RECORD_RETRY: Duration = Duration::from_millis(100);

/// How long the games have to save everyone when the game server stops.
pub const SHUTDOWN_WAIT: Duration = Duration::from_secs(20);

/// Game events waiting to go up the link.
const EVENTS_QUEUE: usize = 1024;

/// The link the games' messages and saves go up.
#[derive(Clone)]
struct Link {
    tx: LinkTx<NodeToRealm>,
    conn: QuicConn,
}

/// A seat the main server offered that nobody took yet.
#[derive(Clone, Debug)]
struct Offer {
    game: u32,
    player: u64,
    character: String,
    new: Option<NewCharInfo>,
    has_record: bool,
}

/// The seats and games of the current link.
#[derive(Default)]
struct Book {
    offers: HashMap<u64, Offer>,
    /// Seat records by lease, from their streams: an offer's record may
    /// come before or after the offer.
    records: HashMap<u64, (Vec<u8>, Instant)>,
    /// Taken seats: the lease of each (game, character name key).
    taken: HashMap<(u32, String), u64>,
    /// The main server's seed of each game.
    seeds: HashMap<u32, u32>,
    /// The first item-id block of each game, from `StartGame`.
    item_ids: HashMap<u32, ItemIdBlock>,
}

impl Book {
    /// Forgets a taken seat; its lease.
    fn untake(&mut self, game: u32, character: &str) -> Option<u64> {
        self.taken.remove(&(game, key(character)))
    }

    /// Forgets everything of game `game`.
    fn forget_game(&mut self, game: u32) {
        let leases: Vec<u64> = self
            .offers
            .iter()
            .filter(|(_, o)| o.game == game)
            .map(|(l, _)| *l)
            .collect();
        for l in leases {
            self.offers.remove(&l);
            self.records.remove(&l);
        }
        self.taken.retain(|(g, _), _| *g != game);
        self.seeds.remove(&game);
        self.item_ids.remove(&game);
    }
}

/// A character's name key: its name in lower case.
fn key(character: &str) -> String {
    character.to_ascii_lowercase()
}

/// What the games, their save sink and their event pump share.
struct Shared {
    link: Mutex<Option<Link>>,
    book: Mutex<Book>,
    /// Saves waiting for their `SaveAck`, by (lease, seq).
    acks: Mutex<HashMap<(u64, u32), std_mpsc::Sender<ErrorCode>>>,
    /// Transfers' saves waiting for their `ItemOutAck`, by (lease, req).
    transfer_acks: Mutex<HashMap<(u64, u64), std_mpsc::Sender<ErrorCode>>>,
    rt: tokio::runtime::Handle,
    log: Arc<ServerLog>,
}

impl Shared {
    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn link(&self) -> Option<Link> {
        self.link.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn acks(&self) -> MutexGuard<'_, HashMap<(u64, u32), std_mpsc::Sender<ErrorCode>>> {
        self.acks.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn transfer_acks(&self) -> MutexGuard<'_, HashMap<(u64, u64), std_mpsc::Sender<ErrorCode>>> {
        self.transfer_acks.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Sends `m` up the link, if there is one.
    fn send(&self, m: &NodeToRealm) {
        if let Some(l) = self.link() {
            let _ = l.tx.send_msg(m);
        }
    }

    /// Stores one save with the main server: the record, then `Save`, then
    /// waits for its `SaveAck`. Runs on the save sink's thread.
    fn store(&self, s: &SaveRecord) -> Result<(), String> {
        let lease = self
            .book()
            .taken
            .get(&(s.game, key(&s.character)))
            .copied()
            .ok_or_else(|| format!("{}: no seat lease in game {}", s.character, s.game))?;
        let link = self
            .link()
            .ok_or_else(|| format!("{}: no link to the main server", s.character))?;
        if s.transfer.is_some() {
            return self.store_transfer(s, lease, &link);
        }
        let (tx, rx) = std_mpsc::channel();
        self.acks().insert((lease, s.seq), tx);
        let sent = self
            .rt
            .block_on(streams::send(
                &link.conn,
                What::Record { lease },
                s.seq,
                &s.record,
            ))
            .and_then(|()| {
                link.tx.send_msg(&NodeToRealm::Save(SaveInfo {
                    game: s.game,
                    lease,
                    seq: s.seq,
                    // The sink isn't told why the game saves; every save
                    // goes up as a periodic one.
                    reason: 0,
                    journal_tick: 0,
                    summary: s.summary.clone(),
                }))
            });
        if let Err(e) = sent {
            self.acks().remove(&(lease, s.seq));
            return Err(format!("{}: sending save {}: {e}", s.character, s.seq));
        }
        let answer = rx.recv_timeout(SAVE_ACK_WAIT);
        self.acks().remove(&(lease, s.seq));
        match answer {
            Ok(STORED) => Ok(()),
            Ok(code) => Err(format!(
                "{}: the main server refused save {}: {code}",
                s.character, s.seq
            )),
            Err(_) => Err(format!(
                "{}: no answer to save {} from the main server",
                s.character, s.seq
            )),
        }
    }

    /// Stores a transfer's save: the record, then `ItemOut` or
    /// `ItemInDone` with it, then waits for its `ItemOutAck`. A refusal is
    /// told to the game through the transfer's answer.
    fn store_transfer(&self, s: &SaveRecord, lease: u64, link: &Link) -> Result<(), String> {
        let Some(t) = s.transfer.as_deref() else {
            return Err("not a transfer".into());
        };
        let save = SaveInfo {
            game: s.game,
            lease,
            seq: s.seq,
            reason: SAVE_REASON_TRANSFER,
            journal_tick: 0,
            summary: s.summary.clone(),
        };
        let m = match &t.goods {
            TransferGoods::Out {
                to,
                unit,
                item_id,
                stack,
                gold,
                sketch,
                item,
                credits,
            } => NodeToRealm::ItemOut(ItemOut {
                save,
                req: t.req,
                to: *to,
                unit: *unit,
                item_id: *item_id,
                stack: stack.clone(),
                gold: *gold,
                sketch: sketch.clone(),
                item: item.clone(),
                credits: credits.clone(),
            }),
            TransferGoods::In => NodeToRealm::ItemInDone(ItemInDone {
                lease,
                req: t.req,
                result: placed::PLACED,
                save: Some(save),
            }),
        };
        let (tx, rx) = std_mpsc::channel();
        self.transfer_acks().insert((lease, t.req), tx);
        let sent = self
            .rt
            .block_on(streams::send(
                &link.conn,
                What::Record { lease },
                s.seq,
                &s.record,
            ))
            .and_then(|()| link.tx.send_msg(&m));
        if let Err(e) = sent {
            self.transfer_acks().remove(&(lease, t.req));
            return Err(format!("{}: sending transfer {}: {e}", s.character, t.req));
        }
        let answer = rx.recv_timeout(SAVE_ACK_WAIT);
        self.transfer_acks().remove(&(lease, t.req));
        match answer {
            Ok(STORED) => Ok(()),
            Ok(code) => {
                t.answer.refuse(code);
                Err(format!(
                    "{}: the main server refused transfer {}: {code}",
                    s.character, t.req
                ))
            }
            Err(_) => Err(format!(
                "{}: no answer to transfer {} from the main server",
                s.character, t.req
            )),
        }
    }
}

/// The games' seeds and first item-id blocks: the main server's, from
/// `StartGame`.
struct RealmSeeds(Arc<Shared>);

impl SeedSource for RealmSeeds {
    fn seed(&self, spec: &GameSpec) -> u32 {
        let seed = self.0.book().seeds.get(&spec.id).copied();
        seed.unwrap_or_else(|| d2_games::ClockSeed.seed(spec))
    }

    fn item_ids(&self, spec: &GameSpec) -> Option<IdBlock> {
        let b = self.0.book().item_ids.get(&spec.id).copied()?;
        Some(IdBlock {
            first: b.first,
            count: b.count,
        })
    }
}

/// The game server's games.
pub struct NodeGames {
    /// `None` when the game data didn't load: every start is refused.
    games: Option<Games>,
    shared: Arc<Shared>,
    tickets: Arc<Tickets>,
    events: mpsc::Sender<GameEvent>,
    /// The program's version and game-data hash, for the journals' `Start`.
    version: String,
    data_hash: Digest,
    /// The journals' chunks, on their way up the link.
    chunks: mpsc::UnboundedSender<JournalChunk>,
    /// Every game was closed for good (the program stops or restarts): no
    /// game starts any more.
    stopping: AtomicBool,
}

impl NodeGames {
    /// The games of a game server on `data`, playing `rules` (the program
    /// `version`, data of `data_hash`); their events and journals go up
    /// the link from tasks on `rt`.
    pub fn new(
        cfg: GamesConfig,
        data: Result<GameData, String>,
        rules: GameRules,
        (version, data_hash): (String, Digest),
        tickets: Arc<Tickets>,
        log: Arc<ServerLog>,
        rt: tokio::runtime::Handle,
    ) -> Arc<Self> {
        let shared = Arc::new(Shared {
            link: Mutex::new(None),
            book: Mutex::new(Book::default()),
            acks: Mutex::new(HashMap::new()),
            transfer_acks: Mutex::new(HashMap::new()),
            rt: rt.clone(),
            log: log.clone(),
        });
        let store = shared.clone();
        let sink = Arc::new(QueuedSink::new("saves", move |s| store.store(s)));
        let (events, rx) = mpsc::channel(EVENTS_QUEUE);
        let games = match data {
            Ok(data) => {
                let seeds = Arc::new(RealmSeeds(shared.clone()));
                let games = Games::with_rules(cfg, data, rules, log.clone(), sink, seeds);
                games.set_events(events.clone());
                Some(games)
            }
            Err(e) => {
                log.line(&format!("{e}; every game start is refused"));
                None
            }
        };
        rt.spawn(pump(shared.clone(), tickets.clone(), rx));
        let (chunks, queued) = mpsc::unbounded_channel();
        let up = shared.clone();
        rt.spawn(journal::send(
            queued,
            move || up.link().map(|l| (l.tx, l.conn)),
            log,
        ));
        Arc::new(Self {
            games,
            shared,
            tickets,
            events,
            version,
            data_hash,
            chunks,
            stopping: AtomicBool::new(false),
        })
    }

    /// The link is up: messages, saves and journals go up `tx` and `conn`.
    /// The games of an earlier link are closed without saving (their
    /// leases ended when the main server lost it); new games save every
    /// `save_interval_s` of the welcome and cut a journal chunk every
    /// `journal_chunk_s`.
    pub fn linked(&self, tx: LinkTx<NodeToRealm>, conn: QuicConn, w: &NodeWelcome) {
        let games = self.games.as_ref();
        if let Some(games) = games.filter(|_| w.save_interval_s > 0) {
            games.set_save_interval(Duration::from_secs(w.save_interval_s.into()));
        }
        if let Some(games) = games {
            games.set_journals(Some(self.journal_setup(w.journal_chunk_s)));
        }
        *self.shared.book() = Book::default();
        for g in games.map(Games::list).unwrap_or_default() {
            self.shared.log.line(&format!(
                "game {}: closed: its main server link was lost",
                g.id()
            ));
            g.shutdown();
        }
        *self.shared.link.lock().unwrap_or_else(|e| e.into_inner()) = Some(Link { tx, conn });
    }

    /// Where the games' journals go: up the link, a chunk every
    /// `chunk_s` seconds (the default when 0).
    fn journal_setup(&self, chunk_s: u32) -> JournalSetup {
        let mut cfg = RecorderConfig::default();
        if chunk_s > 0 {
            cfg.chunk_ticks = chunk_s.saturating_mul(TickRate::GAME.hz());
        }
        let chunks = self.chunks.clone();
        JournalSetup {
            version: self.version.clone(),
            data_hash: self.data_hash,
            cfg,
            sinks: Arc::new(move |_game| {
                Box::new(ChunkSink(chunks.clone())) as Box<dyn JournalSink>
            }),
        }
    }

    /// The link is gone: nothing goes up until it is back; saves waiting
    /// for their answer fail.
    pub fn unlinked(&self) {
        *self.shared.link.lock().unwrap_or_else(|e| e.into_inner()) = None;
        self.shared.acks().clear();
        self.shared.transfer_acks().clear();
    }

    /// Takes the games' part of a main server's message (the ticket part
    /// is [`Tickets::on_message`]'s). `true` if `m` was one.
    pub fn on_message(&self, m: &RealmToNode) -> bool {
        match m {
            RealmToNode::StartGame {
                spec,
                seed,
                item_ids,
            } => {
                let answer = self.start(spec, *seed, *item_ids);
                self.shared.send(&answer);
            }
            RealmToNode::SeatOffer {
                game,
                lease,
                player,
                character,
                new,
                has_record,
                ..
            } => {
                self.shared.book().offers.insert(
                    *lease,
                    Offer {
                        game: *game,
                        player: *player,
                        character: character.clone(),
                        new: new.clone(),
                        has_record: *has_record,
                    },
                );
            }
            RealmToNode::SaveAck { lease, seq, code } => {
                if let Some(tx) = self.shared.acks().remove(&(*lease, *seq)) {
                    let _ = tx.send(*code);
                }
            }
            RealmToNode::Kick { game, lease } => self.kick(*game, *lease),
            RealmToNode::CloseGame { game, how } => self.close(*game, *how),
            RealmToNode::ItemIds { game, ids } => self.grant_item_ids(*game, *ids),
            RealmToNode::ItemWanted(w) => self.item_wanted(w.clone()),
            RealmToNode::ItemIn(m) => self.item_in(m.clone()),
            RealmToNode::ItemOutAck { lease, req, code } => {
                if let Some(tx) = self.shared.transfer_acks().remove(&(*lease, *req)) {
                    let _ = tx.send(*code);
                }
            }
            _ => return false,
        }
        true
    }

    /// A record from the link's streams: a seat's.
    pub fn record(&self, lease: u64, record: Vec<u8>) {
        self.shared
            .book()
            .records
            .insert(lease, (record, Instant::now()));
    }

    /// Forgets the offer of `lease` (nobody claimed it in time).
    pub fn forget_offer(&self, lease: u64) {
        let mut b = self.shared.book();
        b.offers.remove(&lease);
        b.records.remove(&lease);
    }

    /// Every second: drops records no offer took in time, and closes the
    /// games nobody entered.
    pub fn tick(&self) {
        let now = Instant::now();
        self.shared
            .book()
            .records
            .retain(|_, (_, at)| now.saturating_duration_since(*at) < OFFER_TTL);
        if let Some(games) = &self.games {
            games.reap();
        }
    }

    /// Game `id`, while it runs here.
    fn game(&self, id: u32) -> Option<GameHandle> {
        self.games.as_ref().and_then(|g| g.get(id))
    }

    /// Runs one dev command in `character`'s game here (`dev = test`).
    pub fn dev_command(&self, character: &str, cmd: &DevCommand) -> Result<String, String> {
        self.games
            .as_ref()
            .ok_or_else(|| "this game server's game data didn't load".to_string())?
            .dev_command(character, cmd)
    }

    /// The game server's load, for the heartbeat.
    pub fn load(&self) -> Load {
        let (games, players, overruns) = self.games.as_ref().map_or((0, 0, 0), |g| {
            (g.list().len(), g.players(), g.tick_overruns())
        });
        Load {
            games: games.min(u16::MAX.into()) as u16,
            players: players.min(u16::MAX.into()) as u16,
            cpu_permille: 0,
            tick_overruns: overruns.min(u32::MAX.into()) as u32,
            draining: false,
        }
    }

    /// A server line in every game (an upgrade's countdown). Blocks until
    /// each game took it: call it off the async threads.
    pub fn announce(&self, text: &str) {
        if let Some(games) = &self.games {
            let n = games.announce(text);
            self.shared
                .log
                .line(&format!("told {n} players in every game: {text}"));
        }
    }

    /// Sends `m` up the link, if there is one.
    pub fn send_up(&self, m: &NodeToRealm) {
        self.shared.send(m);
    }

    /// Saves every player and closes every game, for good: no game starts
    /// after it. Waits up to [`SHUTDOWN_WAIT`]. Blocks: call it off the
    /// async threads.
    pub fn shutdown_all(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(games) = &self.games {
            games.shutdown_all(SHUTDOWN_WAIT);
        }
    }

    fn start(&self, spec: &NodeGameSpec, seed: u32, ids: ItemIdBlock) -> NodeToRealm {
        let game = spec.game;
        let games = self
            .games
            .as_ref()
            .filter(|_| !self.stopping.load(Ordering::SeqCst));
        let Some(games) = games else {
            self.shared.log.line(&format!(
                "game {game}: start refused: no game data, or stopping"
            ));
            return NodeToRealm::StartRefused {
                game,
                code: ErrorCode::NODE_UNAVAILABLE,
            };
        };
        {
            let mut b = self.shared.book();
            b.seeds.insert(game, seed);
            b.item_ids.insert(game, ids);
        }
        let started = games.start(
            GameSpec {
                id: game,
                name: spec.name.clone(),
                difficulty: spec.difficulty,
                hardcore: spec.hardcore,
                expansion: spec.expansion,
                max_players: spec.max_players,
            },
            self.events.clone(),
        );
        match started {
            Ok(_) => {
                self.shared.log.line(&format!(
                    "game {game}: started for the main server, item ids {}..+{}",
                    ids.first, ids.count
                ));
                NodeToRealm::GameStarted { game }
            }
            Err(code) => {
                self.shared.book().forget_game(game);
                self.shared
                    .log
                    .line(&format!("game {game}: start refused: {code}"));
                NodeToRealm::StartRefused { game, code }
            }
        }
    }

    fn kick(&self, game: u32, lease: u64) {
        let character = {
            let mut b = self.shared.book();
            if b.offers.remove(&lease).is_some() {
                b.records.remove(&lease);
                None
            } else {
                b.taken
                    .iter()
                    .find(|(_, l)| **l == lease)
                    .map(|((_, c), _)| c.clone())
            }
        };
        self.tickets.withdraw(lease);
        let (Some(character), Some(g)) = (character, self.game(game)) else {
            return;
        };
        self.shared.log.line(&format!(
            "game {game}: kicking `{character}` (lease {lease})"
        ));
        // A kick waits for the kicked character's save.
        self.shared.rt.spawn_blocking(move || g.kick(&character));
    }

    /// Grants the main server's block `ids` to game `game` (`ItemIds`).
    fn grant_item_ids(&self, game: u32, ids: ItemIdBlock) {
        let Some(g) = self.game(game) else {
            self.shared.log.line(&format!(
                "game {game}: item ids {}..+{} came after it closed",
                ids.first, ids.count
            ));
            return;
        };
        let block = IdBlock {
            first: ids.first,
            count: ids.count,
        };
        let log = self.shared.log.clone();
        // The grant waits for the game thread's next turn.
        self.shared.rt.spawn_blocking(move || {
            if let Err(e) = g.grant_item_ids(block) {
                log.line(&format!("game {game}: {e}"));
            }
        });
    }

    /// The game of `game` while seat `lease` is taken in it here.
    fn seat_game(&self, game: u32, lease: u64) -> Option<GameHandle> {
        let seated = self
            .shared
            .book()
            .taken
            .iter()
            .any(|((g, _), l)| *g == game && *l == lease);
        self.game(game).filter(|_| seated)
    }

    /// `ItemWanted`: takes the goods off the character between two frames;
    /// the save sink sends them with its save (`ItemOut`). A game that
    /// can't take them answers `ItemOutRefused`.
    fn item_wanted(&self, w: ItemWanted) {
        let g = self.seat_game(w.game, w.lease);
        let shared = self.shared.clone();
        self.shared.rt.spawn_blocking(move || {
            let done = match g {
                Some(g) => g.item_out(w.lease, w.req, w.unit, w.to, w.gold),
                None => Err(d2_games::transfer::code::NOT_IN_GAME),
            };
            if let Err(code) = done {
                shared.log.line(&format!(
                    "game {}: lease {}: transfer {} refused: {code}",
                    w.game, w.lease, w.req
                ));
                shared.send(&NodeToRealm::ItemOutRefused {
                    lease: w.lease,
                    req: w.req,
                    code,
                });
            }
        });
    }

    /// `ItemIn`: places the goods on the character between two frames.
    /// Placed: the save sink answers with its save (`ItemInDone`);
    /// otherwise the answer goes now.
    fn item_in(&self, m: ItemIn) {
        let g = self.seat_game(m.game, m.lease);
        let shared = self.shared.clone();
        self.shared.rt.spawn_blocking(move || {
            let arriving = Arriving {
                item: m.item,
                stack: m.stack,
                gold: m.gold,
                cursor: m.place == stash::place::CURSOR,
            };
            let result = match g {
                Some(g) => g
                    .item_in(m.lease, m.req, arriving)
                    .unwrap_or(placed::REFUSED),
                None => placed::REFUSED,
            };
            if result != placed::PLACED {
                shared.send(&NodeToRealm::ItemInDone(ItemInDone {
                    lease: m.lease,
                    req: m.req,
                    result,
                    save: None,
                }));
            }
        });
    }

    fn close(&self, game: u32, how: u8) {
        let Some(g) = self.game(game) else {
            self.shared.send(&NodeToRealm::GameClosed {
                game,
                reason: ErrorCode::GAME_CLOSED,
            });
            return;
        };
        if how != 0 {
            // Discard: no lease here, so the game's last saves aren't sent.
            self.shared.book().taken.retain(|(g, _), _| *g != game);
        }
        self.shared.log.line(&format!(
            "game {game}: closing for the main server{}",
            if how == 0 { "" } else { " without saving" }
        ));
        g.shutdown();
    }

    /// Seats the player whose ticket passed with `claims` on its game
    /// stream: waits (up to [`OFFER_WAIT`]) for the offer's record, then
    /// hands the seat to the game. The main server hears `SeatTaken`, or
    /// `SeatRefused` with why; the client is told on `stream`.
    pub async fn take_seat(
        &self,
        claims: &TicketClaims,
        mut stream: FramedStream,
        peer: SocketAddr,
    ) -> Result<(), ErrorCode> {
        let (game, lease) = (claims.game, claims.lease.0);
        let refuse = |stream: &mut FramedStream, code: ErrorCode| {
            let _ = stream.send(Refused::new(code).body());
            stream.close();
            self.shared
                .send(&NodeToRealm::SeatRefused { game, lease, code });
            Err(code)
        };
        let Some((offer, record)) = self.offer(lease).await else {
            self.shared.log.line(&format!(
                "player from {peer}: lease {lease}: its record didn't come"
            ));
            return refuse(&mut stream, ErrorCode::SEAT_GONE);
        };
        let Some(g) = self.game(game).filter(|_| offer.game == game) else {
            return refuse(&mut stream, ErrorCode::GAME_CLOSED);
        };
        self.shared
            .book()
            .taken
            .insert((game, key(&offer.character)), lease);
        let seat = Seat {
            account: AccountId(offer.player as i64),
            character: offer.character.clone(),
            lease,
            record,
            new: offer.new.map(|n| NewChar {
                name: n.name,
                class: n.class,
                hardcore: n.hardcore,
                expansion: n.expansion,
                progression: n.progression,
            }),
        };
        let stream = PacketStream::server(stream);
        let admitted = tokio::task::spawn_blocking(move || g.admit(seat, stream))
            .await
            .unwrap_or(Err(ErrorCode::OTHER));
        match admitted {
            Ok(()) => {
                self.shared.log.line(&format!(
                    "game {game}: `{}` seated (lease {lease}) from {peer}",
                    offer.character
                ));
                self.shared.send(&NodeToRealm::SeatTaken { game, lease });
                Ok(())
            }
            Err(code) => {
                self.shared.book().untake(game, &offer.character);
                self.shared.log.line(&format!(
                    "game {game}: `{}` (lease {lease}) refused: {code}",
                    offer.character
                ));
                self.shared
                    .send(&NodeToRealm::SeatRefused { game, lease, code });
                Err(code)
            }
        }
    }

    /// The offer of `lease` and its record, once both are here (an offer
    /// without a record needs none); taken out of the book. `None` if they
    /// don't come within [`OFFER_WAIT`].
    async fn offer(&self, lease: u64) -> Option<(Offer, Option<Vec<u8>>)> {
        let until = Instant::now() + OFFER_WAIT;
        loop {
            {
                let mut b = self.shared.book();
                let ready = b
                    .offers
                    .get(&lease)
                    .is_some_and(|o| !o.has_record || b.records.contains_key(&lease));
                if ready {
                    let offer = b.offers.remove(&lease)?;
                    let record = b.records.remove(&lease).map(|(r, _)| r);
                    let record = record.filter(|_| offer.has_record);
                    return Some((offer, record));
                }
            }
            if Instant::now() + RECORD_RETRY > until {
                return None;
            }
            tokio::time::sleep(RECORD_RETRY).await;
        }
    }

    /// Hands a game stream that attached with a game resume token back to
    /// the game holding its slot; `UnknownResume` when none does.
    pub async fn resume(&self, token: ResumeToken, stream: FramedStream) -> Result<(), ErrorCode> {
        let games = self.games.clone();
        tokio::task::spawn_blocking(move || {
            let mut stream = PacketStream::server(stream);
            let held = games.and_then(|gs| gs.list().into_iter().find(|g| g.holds_resume(&token)));
            match held {
                Some(g) => g.resume(token, stream),
                None => {
                    let _ = stream.send_body(Refused::new(ErrorCode::UNKNOWN_RESUME).body());
                    Err(ErrorCode::UNKNOWN_RESUME)
                }
            }
        })
        .await
        .unwrap_or(Err(ErrorCode::OTHER))
    }
}

/// Sends the games' events up the link.
async fn pump(shared: Arc<Shared>, tickets: Arc<Tickets>, mut rx: mpsc::Receiver<GameEvent>) {
    while let Some(e) = rx.recv().await {
        let m = match e {
            GameEvent::Players { game, players } => Some(NodeToRealm::Players { game, players }),
            GameEvent::Saved { .. } => None,
            GameEvent::Left {
                game, character, ..
            } => shared
                .book()
                .untake(game, &character)
                .map(|lease| NodeToRealm::Left { game, lease }),
            GameEvent::SeatRefused {
                game,
                character,
                code,
                ..
            } => shared
                .book()
                .untake(game, &character)
                .map(|lease| NodeToRealm::SeatRefused { game, lease, code }),
            GameEvent::ItemIdsWanted { game } => Some(NodeToRealm::ItemIdsWanted { game }),
            GameEvent::Closed { game, reason } => {
                shared.book().forget_game(game);
                tickets.game_closed(game);
                Some(NodeToRealm::GameClosed { game, reason })
            }
        };
        if let Some(m) = m {
            shared.send(&m);
        }
    }
}

#[cfg(test)]
#[path = "mod_game_tests.rs"]
mod mod_game_tests;
