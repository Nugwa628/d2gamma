//! Recording a game and re-running it from its journal, in CI: the
//! one-room town of the synthetic install behind a host with the journal
//! recorder on its packet observer, clients that join, walk, repeat
//! themselves, leave and are lost, on an uneven clock.
//!
//! What it pins:
//! - the recorder only reads: the game runs the same with and without it;
//! - the re-run from the journal (read back from its chunks) reaches the
//!   same state hashes and saves to the end;
//! - a changed message is found at the first checkpoint after it, and a
//!   changed save at that save;
//! - transfers (gold to and from the stash, run between frames as the game
//!   server runs them) are re-run at their records: a changed one is found
//!   at it or at its save.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use d2_journal::format::{kind, leave, GameSpec, Goods, Record, Start};
use d2_journal::recorder::{Recorder, RecorderConfig};
use d2_journal::replay::{replay, Client, Mismatch, ReplayGame, TransferIn, TransferOut};
use d2_journal::{read_journal, Digest, Journal};
use d2_server::adapters::handlers::world::ActionWorld;
use d2_server::adapters::session::Entry;
use d2_server::adapters::session_flow::{
    CharacterLoader, CreateGame, Loaded, SessionFlow, DEFAULT_MAX_PLAYERS,
};
use d2_server::adapters::{PlayerData, PlayerFields, ProtoSizes, SimGame};
use d2_server::host::Host;
use d2_server::seams::{ClientId, Clock, Intents, MessageSink, PlayerGate, SessionHandler};
use d2_sim::debug::state;
use d2_sim::game::Game;
use d2_sim::units::lifecycle::AllocRequest;
use d2_sim::units::{UnitId, UnitType};
use d2_sim::wiring::worldgen::WorldSim;
use sha2::{Digest as _, Sha256};
use test_fixtures::game::{ActCreation, GameData, Seams, Sim};
use test_fixtures::{install, synth};

/// The town level of act 1.
const TOWN: u32 = 1;
const INIT: u32 = 0x1234_5678;
const GAME_SEED: u32 = 1234;
/// Expansion (bit 20) and bit 2.
const EXPANSION_FLAGS: u32 = (1 << 20) | 0x4;
/// The clock's steps, frame by frame: around 40 ms, with frames that come
/// early (no tick), late (a tick to catch up) and at the same reading.
const STEPS: [u32; 12] = [40, 40, 0, 13, 67, 40, 25, 55, 40, 80, 40, 1];
/// A state hash every this many ticks, and a chunk every this many.
const CFG: RecorderConfig = RecorderConfig {
    hash_every: 25,
    chunk_ticks: 60,
};
/// The characters are saved every this many ticks.
const SAVE_EVERY: i32 = 40;

fn data() -> &'static GameData {
    static D: OnceLock<GameData> = OnceLock::new();
    D.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("journal-synthetic-{}", std::process::id()));
        let i = install::build(&dir, &synth::synthetic()).unwrap_or_else(|e| panic!("{e}"));
        GameData::from_install(&i).unwrap_or_else(|e| panic!("{e}"))
    })
}

struct NoSession;
impl SessionHandler for NoSession {
    fn system_message(&mut self, _: ClientId, _: &[u8], _: usize, _: &mut dyn MessageSink) {}
}

struct Ms(u32);
impl Clock for Ms {
    fn now_ms(&mut self) -> u32 {
        self.0
    }
}

type TestHost = Host<Sim, ProtoSizes, NoSession, Ms>;

fn alloc_player(s: &mut Sim, class: u32) -> UnitId {
    let req = AllocRequest {
        ty: UnitType::Player,
        class,
        room: None,
        add: true,
        fixed_guid: None,
        mode: 1,
        allied: true,
    };
    let p = s
        .events
        .action
        .with(&mut s.game, |g, v| v.allocate(g, &req, 0, 0))
        .expect("allocated");
    s.events.action.sys.units.get_mut(p).unwrap().mode = 1;
    p
}

/// The synthetic town behind a host; the loader makes each client a new
/// character.
fn host() -> TestHost {
    let d = data();
    let (mut sim, _) = d
        .world_sim(
            ActCreation::TownOnly,
            INIT,
            TOWN,
            GAME_SEED,
            Seams::default(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
    let mut game = Game::new();
    game.lists.ensure_act(0).unwrap();
    sim.action
        .hooks()
        .drlg
        .with_act(0, &mut game.lists, |dr, svc| {
            let lv = dr.get_or_alloc_level(svc.data, svc.types, TOWN)?;
            let r = dr.level_rooms(lv)[0];
            dr.stream_room(svc, r)
        })
        .expect("act 0 has a DRLG")
        .unwrap_or_else(|e| panic!("town: {e:?}"))
        .expect("the town room is active");
    let world = ActionWorld {
        waypoints: Some(d.waypoints().unwrap()),
        ..ActionWorld::default()
    };
    let mut s: Sim = SimGame::with_world(game, sim, world);
    let loader: CharacterLoader<WorldSim<Seams>, ActionWorld> =
        Box::new(|s: &mut Sim, _: ClientId, r: &CreateGame| {
            let player = alloc_player(s, u32::from(r.class));
            // Staged as the game's save loader stages it, so the
            // dispatcher finds the player.
            s.set_player(
                player,
                PlayerFields {
                    gate: PlayerGate {
                        mode: 1,
                        uninterruptable: false,
                    },
                    data: Some(PlayerData { last_accept: 0 }),
                },
            );
            Ok(Loaded {
                player,
                entry: Entry::new(0, r.char_name),
            })
        });
    s.set_session(SessionFlow::new(loader).with_max_players(DEFAULT_MAX_PLAYERS));
    Host::new(s, ProtoSizes, NoSession, Ms(1000))
}

fn create_request(c: ClientId) -> Vec<u8> {
    let mut game_name = [0u8; 16];
    game_name[..4].copy_from_slice(b"game");
    let mut char_name = [0u8; 16];
    let s = format!("client{c}");
    char_name[..s.len()].copy_from_slice(s.as_bytes());
    CreateGame {
        game_name,
        class: (c % 7) as u8,
        char_name,
        flags: EXPANSION_FLAGS,
        ..CreateGame::default()
    }
    .encode()
    .to_vec()
}

fn sha256(b: &[u8]) -> Digest {
    Sha256::digest(b).into()
}

fn state_hash(s: &Sim) -> Digest {
    sha256(
        state::snapshot_world(&s.game, &s.events)
            .to_json_line()
            .as_bytes(),
    )
}

fn pos(s: &mut Sim, p: UnitId) -> (i32, i32) {
    s.events.action.hooks().path_position(p)
}

/// Carried gold (stat 14): this test's transfers move it.
const GOLD: u16 = 14;
/// This test's carried-gold limit.
const GOLD_LIMIT: i32 = 10_000;

fn gold(s: &Sim, p: UnitId) -> i32 {
    s.events.action.sys.stats.unit_base(p, GOLD, 0)
}

fn set_gold(s: &mut Sim, p: UnitId, g: i32) {
    let sys = &mut s.events.action.sys;
    sys.stats.unit_set(&mut sys.hooks, p, GOLD, g, 0);
}

/// This test's transfer out, as both sides run it: gold only (items are
/// the game server's, `live_replay.rs`).
fn take_gold(s: &mut Sim, c: ClientId, out: &TransferOut<'_>) -> Result<(), String> {
    let p = s.player_of(c).ok_or("no player")?;
    let (have, g) = (gold(s, p), out.gold as i32);
    if out.unit != 0 || out.stack.is_some() || g > have {
        return Err(format!("{g} gold taken of {have}"));
    }
    set_gold(s, p, have - g);
    Ok(())
}

/// This test's transfer in: 0 placed, 2 refused (past the limit).
fn bring_gold(s: &mut Sim, c: ClientId, t: &TransferIn<'_>) -> u8 {
    let Some(p) = s.player_of(c) else {
        return 2;
    };
    let (have, g) = (gold(s, p), t.gold as i32);
    if have + g > GOLD_LIMIT {
        return 2;
    }
    set_gold(s, p, have + g);
    0
}

/// No goods but gold.
const NO_GOODS: Goods = Goods::Stack {
    code: String::new(),
    count: 0,
};

/// This test's "save" of a client's character: its player's position,
/// mode and gold, as both sides make it.
fn save_of(s: &mut Sim, c: ClientId) -> Option<Digest> {
    let p = s.player_of(c)?;
    let (x, y) = pos(s, p);
    let mode = s.events.action.sys.units.get(p)?.mode;
    let g = gold(s, p);
    Some(sha256(format!("{c} {x} {y} {mode} {g}").as_bytes()))
}

/// Every client's save, in client order.
fn save_all(s: &mut Sim, clients: &BTreeSet<ClientId>) -> Vec<(ClientId, Digest)> {
    clients
        .iter()
        .filter_map(|&c| save_of(s, c).map(|d| (c, d)))
        .collect()
}

fn lease(c: ClientId) -> u64 {
    1000 + u64::from(c)
}

fn seat_digest(c: ClientId) -> Digest {
    sha256(format!("character of client {c}").as_bytes())
}

fn walk(x: i32, y: i32) -> Vec<u8> {
    let mut m = vec![0x03];
    m.extend_from_slice(&(x as u16).to_le_bytes());
    m.extend_from_slice(&(y as u16).to_le_bytes());
    m
}

fn start() -> Start {
    Start {
        version: "test".into(),
        data_hash: [0; 32],
        seed: GAME_SEED,
        spec: GameSpec {
            id: 7,
            name: "game".into(),
            difficulty: 0,
            hardcore: false,
            expansion: true,
            max_players: DEFAULT_MAX_PLAYERS as u8,
        },
        tick_rate: 25,
        player_hosted: false,
        host_client: None,
    }
}

/// One scripted game, with or without a recorder.
struct Run {
    host: TestHost,
    rec: Option<Recorder>,
    clients: BTreeSet<ClientId>,
    got: BTreeMap<ClientId, Vec<Vec<u8>>>,
    step: usize,
    frames: u64,
    seq: BTreeMap<ClientId, u32>,
    last_save: i32,
    dropped: u32,
    /// Where each client stood at each walk.
    trail: BTreeSet<(ClientId, i32, i32)>,
    /// The last transfer request number.
    req: u64,
}

impl Run {
    fn new(record: bool) -> Self {
        let mut host = host();
        let rec = record.then(|| {
            let (rec, tap) = Recorder::new(start(), host.game.frame(), CFG);
            host.set_packet_observer(Some(Box::new(tap)));
            rec
        });
        Self {
            host,
            rec,
            clients: BTreeSet::new(),
            got: BTreeMap::new(),
            step: 0,
            frames: 0,
            seq: BTreeMap::new(),
            last_save: 0,
            dropped: 0,
            trail: BTreeSet::new(),
            req: 0,
        }
    }

    fn connect(&mut self, c: ClientId) {
        if let Some(r) = &self.rec {
            r.seat(c, lease(c), seat_digest(c));
        }
        self.clients.insert(c);
        self.host.connect(c);
    }

    fn disconnect(&mut self, c: ClientId, how: Option<u8>) {
        if let (Some(r), Some(how)) = (&self.rec, how) {
            r.leaving(c, how);
        }
        self.clients.remove(&c);
        self.host.disconnect(c);
    }

    fn system(&mut self, c: ClientId, m: &[u8]) {
        self.host.send_system(c, m).expect("queued");
    }

    fn game(&mut self, c: ClientId, m: &[u8]) {
        if self.host.send_game(c, m).expect("queued").is_none() {
            self.dropped += 1;
        }
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.host.clock.0 += STEPS[self.step % STEPS.len()];
            self.step += 1;
            self.host.frame().expect("frame");
            self.frames += 1;
            for &c in &self.clients {
                self.got.entry(c).or_default().extend(self.host.receive(c));
            }
            let frame = self.host.game.frame();
            if let Some(r) = &self.rec {
                let s = &self.host.game;
                r.after_frame(frame, || state_hash(s));
            }
            if frame > 0 && frame % SAVE_EVERY == 0 && frame != self.last_save {
                self.last_save = frame;
                for (c, d) in save_all(&mut self.host.game, &self.clients) {
                    let seq = self.seq.entry(c).or_default();
                    *seq += 1;
                    if let Some(r) = &self.rec {
                        r.saved(lease(c), *seq, d);
                    }
                }
            }
        }
    }

    /// The save that goes with a transfer: this client's alone.
    fn save_one(&mut self, c: ClientId) {
        if let Some(d) = save_of(&mut self.host.game, c) {
            let seq = self.seq.entry(c).or_default();
            *seq += 1;
            if let Some(r) = &self.rec {
                r.saved(lease(c), *seq, d);
            }
        }
    }

    /// Gold from client `c`'s character to the stash, between frames as
    /// the game server runs it: the transfer, its record, its save.
    fn transfer_out(&mut self, c: ClientId, gold: u64) {
        self.req += 1;
        let out = TransferOut {
            req: self.req,
            unit: 0,
            item_id: None,
            stack: None,
            gold,
        };
        take_gold(&mut self.host.game, c, &out).expect("the gold is there");
        if let Some(r) = &self.rec {
            r.record(Record::ItemOut {
                client: c as u16,
                req: self.req,
                unit: 0,
                item_id: None,
                stack: None,
                gold,
            });
        }
        self.save_one(c);
    }

    /// Gold from the stash to client `c`'s character; its result.
    fn transfer_in(&mut self, c: ClientId, gold: u64) -> u8 {
        self.req += 1;
        let t = TransferIn {
            req: self.req,
            goods: &NO_GOODS,
            gold,
            cursor: false,
        };
        let result = bring_gold(&mut self.host.game, c, &t);
        if let Some(r) = &self.rec {
            r.record(Record::ItemIn {
                client: c as u16,
                req: self.req,
                goods: NO_GOODS,
                gold,
                cursor: false,
                result,
            });
        }
        if result == 0 {
            self.save_one(c);
        }
        result
    }

    fn join(&mut self, cs: &[ClientId]) {
        for &c in cs {
            self.connect(c);
            self.system(c, &create_request(c));
        }
        self.frames(2);
        for &c in cs {
            self.system(c, &[0x6B]);
        }
        self.frames(4);
    }

    /// Every client in the game walks a little way, client 0 twice in a
    /// row (the second is the duplicate filter's).
    fn walk_round(&mut self, round: i32) {
        let cs: Vec<ClientId> = self.clients.iter().copied().collect();
        for c in cs {
            let Some(p) = self.host.game.player_of(c) else {
                continue;
            };
            let (x, y) = pos(&mut self.host.game, p);
            self.trail.insert((c, x, y));
            let d = 2 + (round + c as i32) % 4;
            let m = walk(x + d, y + if round % 2 == 0 { d } else { -d });
            self.game(c, &m);
            if c == 0 {
                self.game(c, &m);
            }
        }
    }

    /// The whole script; the game's state hash at its end.
    fn play(&mut self) -> Digest {
        self.join(&[0, 1, 2]);
        for round in 0..6 {
            self.walk_round(round);
            self.frames(9 + round as usize);
        }
        // Transfers between frames: gold to client 0, part of it back to
        // the stash, and gold past client 1's limit refused.
        assert_eq!(self.transfer_in(0, 900), 0);
        self.frames(3);
        self.transfer_out(0, 300);
        assert_eq!(self.transfer_in(1, 20_000), 2);
        self.frames(3);
        self.join(&[3]);
        for round in 6..10 {
            self.walk_round(round);
            self.frames(11);
        }
        // Client 2 leaves the game, then its connection goes; client 1's
        // connection is lost.
        self.system(2, &[0x69]);
        self.frames(5);
        self.disconnect(2, Some(leave::LEFT));
        self.disconnect(1, None);
        for round in 10..16 {
            self.walk_round(round);
            self.frames(13);
        }
        let end = state_hash(&self.host.game);
        if let Some(r) = &self.rec {
            r.end(self.host.game.frame(), 0, end);
        }
        end
    }
}

/// The journal of one scripted game, read back from its chunk file, and
/// the game.
fn recorded() -> (Journal, Run) {
    let mut run = Run::new(true);
    run.play();
    let chunks = run.rec.as_ref().unwrap().take_chunks();
    assert!(
        chunks.len() > 2,
        "the journal is cut in chunks: {} chunks, {} frames, game frame {}",
        chunks.len(),
        run.frames,
        run.host.game.frame()
    );
    assert!(chunks.last().unwrap().last);
    let file: Vec<u8> = chunks.iter().flat_map(|c| c.gz.iter().copied()).collect();
    (read_journal(&file).expect("the journal reads"), run)
}

/// The re-run: the same game built again, fed the journal.
struct Rerun {
    host: TestHost,
    clients: BTreeSet<ClientId>,
    saves: Vec<(Client, Digest)>,
}

impl Rerun {
    fn new() -> Self {
        Self {
            host: host(),
            clients: BTreeSet::new(),
            saves: Vec::new(),
        }
    }
}

impl ReplayGame for Rerun {
    fn join(&mut self, client: Client, lease_: u64, record: &Digest) -> Result<(), String> {
        if lease_ != lease(client) || *record != seat_digest(client) {
            return Err(format!("client {client} joined with another seat"));
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
        self.host.clock.0 = now;
        self.host.frame().map_err(|e| e.to_string())?;
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
        let s = save_all(&mut self.host.game, &self.clients);
        self.saves.extend(s);
    }

    fn take_saves(&mut self) -> Vec<(Client, Digest)> {
        std::mem::take(&mut self.saves)
    }

    fn item_out(&mut self, client: Client, out: &TransferOut<'_>) -> Result<(), String> {
        take_gold(&mut self.host.game, client, out)?;
        let save = save_of(&mut self.host.game, client);
        self.saves.extend(save.map(|d| (client, d)));
        Ok(())
    }

    fn item_in(&mut self, client: Client, item_in: &TransferIn<'_>) -> Result<u8, String> {
        let result = bring_gold(&mut self.host.game, client, item_in);
        if result == 0 {
            let save = save_of(&mut self.host.game, client);
            self.saves.extend(save.map(|d| (client, d)));
        }
        Ok(result)
    }
}

#[test]
fn the_recorder_only_reads() {
    let (_, recorded) = recorded();
    let mut plain = Run::new(false);
    let end = plain.play();
    assert_eq!(end, state_hash(&recorded.host.game));
    assert_eq!(plain.got, recorded.got, "every client got the same bytes");
    assert_eq!(plain.host.game.frame(), recorded.host.game.frame());
}

#[test]
fn the_journal_replays_to_the_same_hashes_and_saves() {
    let (ref j, ref run) = recorded();
    assert!(j.ended);
    assert!(run.dropped > 0, "the duplicate filter dropped a repeat");
    assert!(run.trail.len() > 5, "the players walked: {:?}", run.trail);
    let stats = run.rec.as_ref().unwrap().stats();
    assert!(
        stats.frames < run.frames,
        "frames that changed nothing are left out ({} of {})",
        stats.frames,
        run.frames
    );
    let count = |k: u8| j.entries.iter().filter(|e| e.record.kind() == k).count();
    assert_eq!(count(2), 4, "four joins");
    assert_eq!(count(4), 2, "two leaves");
    let leaves: Vec<_> = j
        .entries
        .iter()
        .filter_map(|e| match e.record {
            Record::Leave { client, how } => Some((client, how)),
            _ => None,
        })
        .collect();
    assert_eq!(leaves, [(2, leave::LEFT), (1, leave::LOST)]);
    assert_eq!(count(kind::ITEM_OUT), 1);
    assert_eq!(count(kind::ITEM_IN), 2);

    let mut game = Rerun::new();
    let report = replay(j, &mut game);
    assert_eq!(report.divergence, None);
    assert!(report.ended);
    assert_eq!(report.transfers, 3);
    assert_eq!(report.records, j.entries.len());
    assert_eq!(u64::from(report.state_hashes), stats.state_hashes);
    assert!(report.state_hashes >= 5);
    assert_eq!(report.saves.len() as u64, stats.saves);
    assert!(report.saves.len() >= 8);
    assert_eq!(
        i64::from(report.last_tick),
        i64::from(run.host.game.frame())
    );
    assert_eq!(state_hash(&game.host.game), state_hash(&run.host.game));
    let s = &report.saves[0];
    assert_eq!(report.check_stored(s.lease, s.seq, &s.record), Ok(()));
    assert!(report.check_stored(s.lease, s.seq, &[1; 32]).is_err());
}

/// The first record from `i` on that checks `client`'s state: a state hash,
/// or a save other than one saving another client after its transfer.
fn next_checkpoint(j: &Journal, i: usize, client: u16) -> usize {
    (i..j.entries.len())
        .find(|&k| match &j.entries[k].record {
            Record::StateHash(_) => true,
            Record::Save { .. } => match &j.entries[k - 1].record {
                Record::ItemOut { client: c, .. } | Record::ItemIn { client: c, .. } => {
                    *c == client
                }
                _ => true,
            },
            _ => false,
        })
        .expect("a checkpoint after it")
}

#[test]
fn a_changed_message_is_found_at_the_next_checkpoint() {
    let (ref j, _) = recorded();
    // A walk after the first state hash, well before the next checkpoint
    // (so the player has moved by then); not client 0's, whose walks come
    // in pairs (a changed first one would let the duplicate through).
    let i = (0..j.entries.len())
        .find(|&i| match &j.entries[i].record {
            Record::Intent {
                client,
                system: false,
                msg,
            } if *client != 0 && msg[0] == 0x03 && j.entries[i].tick > 30 => {
                j.entries[next_checkpoint(j, i, *client)].tick >= j.entries[i].tick + 6
            }
            _ => false,
        })
        .expect("a walk to change");
    let mut bad = j.clone();
    let Record::Intent { msg, .. } = &mut bad.entries[i].record else {
        unreachable!()
    };
    // The walk goes the other way: its target was a few sub-tiles to the
    // player's right, now it is to the left.
    msg[1] -= 10;
    let Record::Intent { client, .. } = bad.entries[i].record else {
        unreachable!()
    };
    let at = next_checkpoint(&bad, i, client);
    let report = replay(&bad, &mut Rerun::new());
    let d = report.divergence.expect("the changed walk is found");
    assert_eq!(d.record, at, "{d:?}");
    assert_eq!(d.tick, bad.entries[at].tick);
    assert!(d.last_good <= bad.entries[i].tick && d.last_good > 0);
    assert!(matches!(
        d.what,
        Mismatch::StateHash { .. } | Mismatch::Save { .. }
    ));
}

#[test]
fn a_changed_save_is_found_at_that_save() {
    let (ref j, _) = recorded();
    let i = j
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.record, Record::Save { .. }))
        .nth(3)
        .expect("a fourth save")
        .0;
    let mut bad = j.clone();
    let Record::Save { record, lease, seq } = &mut bad.entries[i].record else {
        unreachable!()
    };
    record[0] ^= 1;
    let (lease, seq) = (*lease, *seq);
    let report = replay(&bad, &mut Rerun::new());
    let d = report.divergence.expect("the changed save is found");
    assert_eq!(d.record, i);
    assert_eq!(d.tick, bad.entries[i].tick);
    assert!(matches!(d.what, Mismatch::Save { lease: l, seq: s, .. } if l == lease && s == seq));
    assert_eq!(report.saves.len(), 3, "the saves before it matched");
}

#[test]
fn a_dropped_frame_or_join_is_found() {
    let (ref j, _) = recorded();
    // A frame that ran a tick taken out: the re-run's game falls a tick
    // behind at the next record.
    let f = (1..j.entries.len() - 1)
        .find(|&i| {
            matches!(j.entries[i].record, Record::Frame { .. })
                && j.entries[i].tick > 60
                && j.entries[i + 1].tick > j.entries[i].tick
        })
        .expect("a frame that ticked");
    let mut bad = j.clone();
    bad.entries.remove(f);
    let d = replay(&bad, &mut Rerun::new())
        .divergence
        .expect("the dropped frame is found");
    assert_eq!(d.record, f, "{d:?}");
    assert_eq!(
        d.what,
        Mismatch::Tick {
            want: j.entries[f].tick + 1,
            got: j.entries[f].tick
        }
    );

    // A join taken out: that client is not in the re-run, so the game
    // refuses its frames' messages or its save names a lease no client
    // joined with.
    let mut bad = j.clone();
    let k = bad
        .entries
        .iter()
        .position(|e| matches!(e.record, Record::Join { client: 3, .. }))
        .unwrap();
    bad.entries.remove(k);
    let d = replay(&bad, &mut Rerun::new())
        .divergence
        .expect("the dropped join is found");
    assert!(d.record >= k);
    assert!(
        matches!(
            d.what,
            Mismatch::UnknownLease { lease: 1003 }
                | Mismatch::StateHash { .. }
                | Mismatch::Frame(_)
        ),
        "{d:?}"
    );
}

/// The index of the `n`th record of kind `k`.
fn nth_of(j: &Journal, k: u8, n: usize) -> usize {
    (0..j.entries.len())
        .filter(|&i| j.entries[i].record.kind() == k)
        .nth(n)
        .expect("such a record")
}

#[test]
fn a_changed_transfer_is_found_at_it_or_at_its_save() {
    let (ref j, _) = recorded();
    let given = nth_of(j, kind::ITEM_IN, 0);
    let taken = nth_of(j, kind::ITEM_OUT, 0);
    let refused = nth_of(j, kind::ITEM_IN, 1);
    assert!(matches!(j.entries[given + 1].record, Record::Save { .. }));
    let run = |bad: &Journal| {
        replay(bad, &mut Rerun::new())
            .divergence
            .expect("the change is found")
    };

    // More gold given: the game takes it, its save differs.
    let mut bad = j.clone();
    if let Record::ItemIn { gold, .. } = &mut bad.entries[given].record {
        *gold += 1;
    }
    let d = run(&bad);
    assert_eq!(d.record, given + 1, "{d:?}");
    assert!(matches!(d.what, Mismatch::Save { .. }), "{d:?}");

    // A placed transfer said to have found no room.
    let mut bad = j.clone();
    if let Record::ItemIn { result, .. } = &mut bad.entries[given].record {
        *result = 1;
    }
    let d = run(&bad);
    assert_eq!(d.record, given);
    assert_eq!(d.what, Mismatch::TransferResult { want: 1, got: 0 });

    // Gold taken that the character doesn't carry.
    let mut bad = j.clone();
    if let Record::ItemOut { gold, .. } = &mut bad.entries[taken].record {
        *gold = 5_000;
    }
    let d = run(&bad);
    assert_eq!(d.record, taken);
    assert!(
        matches!(&d.what, Mismatch::Transfer { kind: k, .. } if *k == kind::ITEM_OUT),
        "{d:?}"
    );

    // A refused transfer that would have fit.
    let mut bad = j.clone();
    if let Record::ItemIn { gold, .. } = &mut bad.entries[refused].record {
        *gold = 2_000;
    }
    let d = run(&bad);
    assert_eq!(d.record, refused);
    assert_eq!(d.what, Mismatch::TransferResult { want: 2, got: 0 });

    // A transfer moved past the next frame that ticks, its tick kept: the
    // re-run is a tick on when it gets there.
    let mut bad = j.clone();
    let moved = bad.entries.remove(taken);
    let after = (taken..bad.entries.len())
        .find(|&i| {
            matches!(bad.entries[i].record, Record::Frame { .. })
                && bad.entries[i + 1].tick > moved.tick
        })
        .expect("a frame that ticks");
    bad.entries.insert(after + 1, moved.clone());
    let d = run(&bad);
    assert!(d.record <= after + 1, "{d:?}");
    assert!(d.record >= taken, "{d:?}");
}
