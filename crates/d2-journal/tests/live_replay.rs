//! A game on the user's game data recorded and re-run from its journal
//! (`#[ignore]`, `D2_GAME_DIR`): the game server's own build
//! ([`d2_journal::server::build_game`]) with the seats' characters and the
//! digest store, two new characters that join, walk, ping, repeat
//! themselves, save and leave, on an uneven clock; then
//! [`ServerReplay`] re-runs the journal.
//!
//! What it pins:
//! - the re-run reaches every state hash and save of the journal, and the
//!   final state of the game;
//! - stash and market transfers (gold in and out, a stored item out, an
//!   item in from save bytes, refusals) re-run at their records, each with
//!   its one-character save;
//! - a changed walk is found at the next checkpoint, a changed save at
//!   that save, a transfer of an item the character doesn't hold at that
//!   transfer, and a stored record that differs is refused;
//! - what recording costs: journal bytes per player-minute and the time
//!   the recorder takes per tick, printed (`--no-capture`).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use d2_formats::d2s::appearance::AppearanceTables;
use d2_formats::d2s::SaveTables;
use d2_journal::format::{kind, leave, GameSpec, Goods, Record, Start};
use d2_journal::recorder::{Recorder, RecorderConfig, RecorderStats};
use d2_journal::replay::{replay, Mismatch, TransferIn, TransferOut};
use d2_journal::server::{
    bring_in, build_game, characters, save_after_transfer, state_hash, stored_items, take_out,
    DigestStore, Joined, NewChar, PongSession, Seat, SeatCharacter, Seats, ServerReplay,
};
use d2_journal::{read_journal, Digest, Journal};
use d2_server::adapters::ProtoSizes;
use d2_server::game::{create_request_for, new_character, GameData, Sim};
use d2_server::host::Host;
use d2_server::packets::{PacketEvent, PacketObserver};
use d2_server::seams::{ClientId, Clock, Intents, Tick};

fn data() -> GameData {
    static DATA: OnceLock<GameData> = OnceLock::new();
    DATA.get_or_init(|| {
        let dir = std::env::var_os("D2_GAME_DIR").expect("D2_GAME_DIR");
        GameData::select(Some(Path::new(&dir))).expect("the game data loads")
    })
    .clone()
}

fn tables() -> Arc<dyn SaveTables + Send + Sync> {
    let GameData::Live(live) = data();
    Arc::new(live.save.clone())
}

fn appearance() -> Option<Arc<AppearanceTables>> {
    let GameData::Live(live) = data();
    d2_server::game::character_save::appearance_tables(&live.tables.fixed)
        .ok()
        .map(Arc::new)
}

/// The realm's seed for the game (any fixed value).
const SEED: u32 = 0x00C0_FFEE;
/// The clock's steps, frame by frame (see `synthetic_replay.rs`).
const STEPS: [u32; 12] = [40, 40, 0, 13, 67, 40, 25, 55, 40, 80, 40, 1];
/// The characters are saved every this many ticks.
const SAVE_EVERY: i32 = 300;
/// Host frames the script runs (about a minute of game time).
const FRAMES: usize = 1500;

/// The seats: client, lease, class and name.
const SEATS: [(ClientId, u64, &str, &str); 2] = [
    (0, 7001, "barbarian", "Journal-Bar"),
    (1, 7002, "sorceress", "Journal-Sor"),
];

fn spec() -> GameSpec {
    GameSpec {
        id: 99,
        name: "journal".into(),
        difficulty: 0,
        hardcore: false,
        expansion: true,
        max_players: 8,
    }
}

fn seat_character(class: &str, name: &str) -> SeatCharacter {
    SeatCharacter {
        name: name.into(),
        joined: Joined::New(NewChar {
            class: d2_server::game::parse_class(class).unwrap(),
            hardcore: false,
            progression: 0,
        }),
    }
}

/// The main server's records of the seats.
fn records() -> BTreeMap<u64, SeatCharacter> {
    SEATS
        .iter()
        .map(|&(_, lease, class, name)| (lease, seat_character(class, name)))
        .collect()
}

#[derive(Clone, Default)]
struct Ms(Arc<std::sync::atomic::AtomicU32>);
impl Clock for Ms {
    fn now_ms(&mut self) -> u32 {
        self.0.load(std::sync::atomic::Ordering::Relaxed)
    }
}
impl Ms {
    fn add(&self, ms: u32) {
        self.0.fetch_add(ms, std::sync::atomic::Ordering::Relaxed);
    }
}

type LiveHost = Host<Sim, ProtoSizes, PongSession, Ms>;

/// The recorder's tap, timed.
struct Timed<O> {
    inner: O,
    spent: Arc<Mutex<Duration>>,
}

impl<O: PacketObserver> PacketObserver for Timed<O> {
    fn packet(&mut self, ev: PacketEvent<'_>) {
        let t = Instant::now();
        self.inner.packet(ev);
        *self.spent.lock().unwrap() += t.elapsed();
    }
}

/// One scripted game on the game server's build.
struct Run {
    host: LiveHost,
    clock: Ms,
    seats: Seats,
    rec: Option<Recorder>,
    clients: BTreeSet<ClientId>,
    step: usize,
    last_save: i32,
    /// Time in host frames and in the recorder.
    frame_time: Duration,
    rec_time: Arc<Mutex<Duration>>,
    ticks: u64,
    /// Ticks times the players in the game.
    player_ticks: u64,
    trail: BTreeSet<(ClientId, i32, i32)>,
    /// The last transfer request number.
    req: u64,
    /// The item entries of each client's last save.
    items: Rc<RefCell<BTreeMap<ClientId, Vec<Vec<u8>>>>>,
    /// Transfers that moved goods.
    transfers: u32,
}

impl Run {
    fn new(record: bool) -> Self {
        let seats: Seats = Rc::default();
        let rec_cell: Rc<RefCell<Option<Recorder>>> = Rc::default();
        let seq: Rc<RefCell<BTreeMap<ClientId, u32>>> = Rc::default();
        let items: Rc<RefCell<BTreeMap<ClientId, Vec<Vec<u8>>>>> = Rc::default();
        let on_save = {
            let rec = rec_cell.clone();
            let items = items.clone();
            Box::new(move |c: ClientId, save: &d2_formats::d2s::D2s, d: Digest| {
                let entries = save.body.as_ref().map_or_else(Vec::new, |b| {
                    b.items.iter().map(|e| e.bytes.clone()).collect()
                });
                items.borrow_mut().insert(c, entries);
                let lease = SEATS.iter().find(|s| s.0 == c).unwrap().1;
                let mut seq = seq.borrow_mut();
                let n = seq.entry(c).or_default();
                *n += 1;
                if let Some(r) = &*rec.borrow() {
                    r.saved(lease, *n, d);
                }
            })
        };
        let store = DigestStore {
            seats: seats.clone(),
            tables: tables(),
            appearance: appearance(),
            on_save,
        };
        let sim = build_game(
            &spec(),
            data(),
            SEED,
            characters(seats.clone(), spec()),
            Box::new(store),
        )
        .expect("the game builds");
        let start = Start {
            version: env!("CARGO_PKG_VERSION").into(),
            data_hash: [0; 32],
            seed: SEED,
            spec: spec(),
            tick_rate: sim.tick_rate().hz(),
            player_hosted: false,
            host_client: None,
        };
        let clock = Ms::default();
        clock.add(1000);
        let mut host = Host::new(sim, ProtoSizes, PongSession, clock.clone());
        let rec_time = Arc::new(Mutex::new(Duration::ZERO));
        if record {
            let (rec, tap) = Recorder::new(start, host.game.frame(), RecorderConfig::default());
            host.set_packet_observer(Some(Box::new(Timed {
                inner: tap,
                spent: rec_time.clone(),
            })));
            *rec_cell.borrow_mut() = Some(rec);
        }
        let rec = rec_cell.borrow().clone();
        Self {
            host,
            clock,
            seats,
            rec,
            clients: BTreeSet::new(),
            step: 0,
            last_save: 0,
            frame_time: Duration::ZERO,
            rec_time,
            ticks: 0,
            player_ticks: 0,
            trail: BTreeSet::new(),
            req: 0,
            items,
            transfers: 0,
        }
    }

    /// A stash or market transfer that takes the stored item `unit` (0:
    /// none) and `gold` from client `c`'s character, run between frames as
    /// the game server runs it: the transfer, its record, its save.
    fn transfer_out(&mut self, c: ClientId, unit: u32, gold: u64) {
        self.req += 1;
        let out = TransferOut {
            req: self.req,
            unit,
            item_id: None,
            stack: None,
            gold,
        };
        take_out(&mut self.host.game, c, &out).expect("the goods are there");
        if let Some(r) = &self.rec {
            r.record(Record::ItemOut {
                client: c as u16,
                req: self.req,
                unit,
                item_id: None,
                stack: None,
                gold,
            });
        }
        save_after_transfer(&mut self.host.game, c).expect("the transfer saves");
        self.transfers += 1;
    }

    /// A transfer that brings `goods` and `gold` to client `c`'s character,
    /// run as [`Run::transfer_out`] is; the save only when they were placed.
    fn transfer_in(&mut self, c: ClientId, goods: Goods, gold: u64) -> u8 {
        self.req += 1;
        let t = TransferIn {
            req: self.req,
            goods: &goods,
            gold,
            cursor: false,
        };
        let result = bring_in(&mut self.host.game, c, &t).expect("goods the game takes");
        if let Some(r) = &self.rec {
            r.record(Record::ItemIn {
                client: c as u16,
                req: self.req,
                goods,
                gold,
                cursor: false,
                result,
            });
        }
        if result == 0 {
            save_after_transfer(&mut self.host.game, c).expect("the transfer saves");
        }
        self.transfers += 1;
        result
    }

    /// The transfers of the script: gold in, a stored item and some gold
    /// out, an item from save bytes in, then two refused (unreadable bytes,
    /// gold past the carried limit).
    fn transfers(&mut self) {
        let gold_only = || Goods::Stack {
            code: String::new(),
            count: 0,
        };
        assert_eq!(self.transfer_in(0, gold_only(), 900), 0);
        self.frames(3);
        let stored = stored_items(&mut self.host.game, 0);
        let &unit = stored.first().expect("a stored item to take");
        self.transfer_out(0, unit, 300);
        assert!(!stored_items(&mut self.host.game, 0).contains(&unit));
        self.frames(2);
        let bytes = self.items.borrow()[&0]
            .first()
            .cloned()
            .expect("an item in the save");
        let before = stored_items(&mut self.host.game, 1).len();
        assert_eq!(self.transfer_in(1, Goods::Item(bytes), 0), 0);
        assert_eq!(stored_items(&mut self.host.game, 1).len(), before + 1);
        self.frames(1);
        assert_eq!(self.transfer_in(1, Goods::Item(vec![0; 4]), 0), 2);
        assert_eq!(self.transfer_in(0, gold_only(), 20_000), 2);
        self.frames(4);
    }

    fn join(&mut self, i: usize) {
        let (c, lease, class, name) = SEATS[i];
        let character = seat_character(class, name);
        let base = d2_journal::server::base_save(&data(), &character, 0).unwrap();
        if let Some(r) = &self.rec {
            r.seat(c, lease, character.digest());
        }
        self.seats.borrow_mut().insert(c, Seat { character, base });
        self.clients.insert(c);
        self.host.connect(c);
        let req = create_request_for(&new_character(class, name).unwrap(), 800, 600);
        self.host.send_system(c, &req.encode()).expect("queued");
        self.frames(2);
        self.host.send_system(c, &[0x6B]).expect("queued");
        self.frames(4);
    }

    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.clock.add(STEPS[self.step % STEPS.len()]);
            self.step += 1;
            let before = self.host.game.frame();
            let t = Instant::now();
            self.host.frame().expect("frame");
            self.frame_time += t.elapsed();
            let frame = self.host.game.frame();
            let ticked = u64::try_from(frame - before).unwrap_or(0);
            self.ticks += ticked;
            self.player_ticks += ticked * self.clients.len() as u64;
            for &c in &self.clients {
                self.host.receive(c);
            }
            if let Some(r) = &self.rec {
                let t = Instant::now();
                let s = &self.host.game;
                r.after_frame(frame, || state_hash(s));
                *self.rec_time.lock().unwrap() += t.elapsed();
            }
            if frame > 0 && frame % SAVE_EVERY == 0 && frame != self.last_save {
                self.last_save = frame;
                for (c, r) in self.host.game.save_characters() {
                    r.unwrap_or_else(|e| panic!("save of client {c}: {e:?}"));
                }
            }
        }
    }

    fn walk_round(&mut self, round: i32) {
        let cs: Vec<ClientId> = self.clients.iter().copied().collect();
        for c in cs {
            let Some(p) = self.host.game.player_of(c) else {
                continue;
            };
            let (x, y) = self.host.game.events.action.hooks().path_position(p);
            self.trail.insert((c, x, y));
            let d = 3 + (round + c as i32) % 5;
            let (tx, ty) = match (round + c as i32) % 4 {
                0 => (x + d, y),
                1 => (x, y + d),
                2 => (x - d, y),
                _ => (x, y - d),
            };
            let mut m = vec![0x03];
            m.extend_from_slice(&(tx as u16).to_le_bytes());
            m.extend_from_slice(&(ty as u16).to_le_bytes());
            let _ = self.host.send_game(c, &m).expect("queued");
            if round % 5 == 0 {
                // A repeat the duplicate filter drops.
                let _ = self.host.send_game(c, &m).expect("queued");
            }
        }
    }

    fn play(&mut self) -> Digest {
        self.join(0);
        self.join(1);
        let mut round = 0;
        while self.step < FRAMES {
            self.walk_round(round);
            if round % 7 == 3 {
                let mut ping = vec![0x6D];
                ping.extend_from_slice(&(round as u32).to_le_bytes());
                ping.extend_from_slice(&[0; 8]);
                self.host.send_system(0, &ping).expect("queued");
            }
            self.frames(12);
            round += 1;
            if round == 20 {
                self.transfers();
            }
            if self.step >= FRAMES * 3 / 4 && self.clients.contains(&1) {
                // Client 1 leaves the game, then its connection goes.
                self.host.send_system(1, &[0x69]).expect("queued");
                self.frames(5);
                if let Some(r) = &self.rec {
                    r.leaving(1, leave::LEFT);
                }
                self.clients.remove(&1);
                self.host.disconnect(1);
            }
        }
        let end = state_hash(&self.host.game);
        if let Some(r) = &self.rec {
            r.end(self.host.game.frame(), 0, end);
        }
        end
    }
}

fn recorded() -> (Journal, Run) {
    let mut run = Run::new(true);
    run.play();
    let chunks = run.rec.as_ref().unwrap().take_chunks();
    let file: Vec<u8> = chunks.iter().flat_map(|c| c.gz.iter().copied()).collect();
    (read_journal(&file).expect("the journal reads"), run)
}

fn rerun(j: &Journal) -> d2_journal::replay::ReplayReport {
    let records = records();
    let mut game = ServerReplay::new(j.start(), data(), tables(), appearance(), &records)
        .expect("the game builds again");
    replay(j, &mut game)
}

/// The next state hash after record `i`: where a changed position shows
/// (a character save holds no position).
fn next_hash(j: &Journal, i: usize) -> usize {
    i + j.entries[i..]
        .iter()
        .position(|e| matches!(e.record, Record::StateHash(_)))
        .expect("a state hash after it")
}

fn per_tick(d: Duration, ticks: u64) -> f64 {
    d.as_secs_f64() * 1e6 / ticks.max(1) as f64
}

#[test]
#[ignore = "needs the game data (D2_GAME_DIR)"]
fn a_live_game_replays_to_the_same_hashes_and_saves() {
    let (j, run) = recorded();
    assert!(j.ended);
    assert!(run.trail.len() > 10, "the players walked: {:?}", run.trail);
    let stats: RecorderStats = run.rec.as_ref().unwrap().stats();
    assert!(stats.saves >= 6, "{stats:?}");

    let t = Instant::now();
    let report = rerun(&j);
    let replay_time = t.elapsed();
    assert_eq!(report.divergence, None);
    assert!(report.ended);
    assert_eq!(u64::from(report.state_hashes), stats.state_hashes);
    assert_eq!(report.saves.len() as u64, stats.saves);
    assert_eq!(run.transfers, 5);
    assert_eq!(report.transfers, run.transfers);
    assert_eq!(
        i64::from(report.last_tick),
        i64::from(run.host.game.frame())
    );

    // The cost of recording: the same script without the recorder.
    let mut plain = Run::new(false);
    let end = plain.play();
    assert_eq!(end, state_hash(&run.host.game), "the recorder only reads");
    let rec_time = *run.rec_time.lock().unwrap();
    let minutes = run.ticks as f64 / f64::from(j.start().tick_rate) / 60.0;
    let player_minutes = run.player_ticks as f64 / f64::from(j.start().tick_rate) / 60.0;
    eprintln!(
        "journal: {} ticks ({minutes:.2} min), {} records ({} intents, {} frames, {} state hashes, {} saves), {} chunks",
        run.ticks, stats.records, stats.intents, stats.frames, stats.state_hashes, stats.saves, stats.chunks
    );
    eprintln!(
        "journal: {} bytes raw, {} gz ({:.1}x); {:.0} gz bytes per game-minute, {:.0} per player-minute",
        stats.raw_bytes,
        stats.gz_bytes,
        stats.raw_bytes as f64 / stats.gz_bytes.max(1) as f64,
        stats.gz_bytes as f64 / minutes,
        stats.gz_bytes as f64 / player_minutes
    );
    eprintln!(
        "journal: host frames {:.1} us/tick recorded, {:.1} us/tick plain; recorder {:.2} us/tick (tap and checkpoints)",
        per_tick(run.frame_time, run.ticks),
        per_tick(plain.frame_time, plain.ticks),
        per_tick(rec_time, run.ticks),
    );
    eprintln!(
        "journal: re-run {:.0} ms for {} ticks",
        replay_time.as_secs_f64() * 1e3,
        report.last_tick
    );
}

#[test]
#[ignore = "needs the game data (D2_GAME_DIR)"]
fn a_changed_live_journal_is_found_where_it_changed() {
    let (j, _) = recorded();
    // A walk well before the next state hash, not the first of a repeated
    // pair (changing it would let the repeat through).
    let i = (0..j.entries.len())
        .find(|&i| match &j.entries[i].record {
            Record::Intent {
                system: false, msg, ..
            } if msg[0] == 0x03 && j.entries[i].tick > 200 => {
                let next = &j.entries[i + 1].record;
                !matches!(next, Record::Intent { msg: m, .. } if m == msg)
                    && j.entries[next_hash(&j, i)].tick >= j.entries[i].tick + 30
            }
            _ => false,
        })
        .expect("a walk to change");
    let at = next_hash(&j, i);
    // The walk turned four ways: a turn into a wall the player already
    // stands against changes nothing (and can't be told from the
    // original); any other is found at the next state hash.
    let mut found = 0;
    for (dx, dy) in [(16i16, 0i16), (0, 16), (-16, 0), (0, -16)] {
        let mut bad = j.clone();
        let Record::Intent { msg, .. } = &mut bad.entries[i].record else {
            unreachable!()
        };
        let x = u16::from_le_bytes([msg[1], msg[2]]).wrapping_add_signed(dx);
        let y = u16::from_le_bytes([msg[3], msg[4]]).wrapping_add_signed(dy);
        msg[1..3].copy_from_slice(&x.to_le_bytes());
        msg[3..5].copy_from_slice(&y.to_le_bytes());
        let Some(d) = rerun(&bad).divergence else {
            continue;
        };
        assert_eq!(d.record, at, "turned ({dx}, {dy}): {d:?}");
        assert!(d.last_good <= bad.entries[i].tick);
        assert!(matches!(
            d.what,
            Mismatch::StateHash { .. } | Mismatch::Save { .. }
        ));
        found += 1;
    }
    assert!(found >= 2, "the turned walks found: {found} of 4");

    // A save's digest changed.
    let s = j
        .entries
        .iter()
        .position(|e| matches!(e.record, Record::Save { .. }))
        .unwrap();
    let mut bad = j.clone();
    let Record::Save { record, .. } = &mut bad.entries[s].record else {
        unreachable!()
    };
    record[5] ^= 0x40;
    let d = rerun(&bad).divergence.expect("the changed save is found");
    assert_eq!((d.record, d.tick), (s, bad.entries[s].tick));
    assert!(matches!(d.what, Mismatch::Save { .. }));

    // A transfer of an item the character doesn't hold.
    let t = j
        .entries
        .iter()
        .position(|e| matches!(e.record, Record::ItemOut { unit, .. } if unit != 0))
        .expect("an item taken out");
    let mut bad = j.clone();
    let Record::ItemOut { unit, .. } = &mut bad.entries[t].record else {
        unreachable!()
    };
    *unit += 1000;
    let d = rerun(&bad)
        .divergence
        .expect("the changed transfer is found");
    assert_eq!(d.record, t);
    assert!(
        matches!(&d.what, Mismatch::Transfer { kind: k, .. } if *k == kind::ITEM_OUT),
        "{d:?}"
    );

    // More gold brought: found at the save that follows.
    let t = j
        .entries
        .iter()
        .position(|e| matches!(e.record, Record::ItemIn { result: 0, gold, .. } if gold > 0))
        .expect("gold brought");
    let mut bad = j.clone();
    let Record::ItemIn { gold, .. } = &mut bad.entries[t].record else {
        unreachable!()
    };
    *gold += 1;
    let d = rerun(&bad)
        .divergence
        .expect("the changed transfer is found");
    assert_eq!(d.record, t + 1, "{d:?}");
    assert!(matches!(d.what, Mismatch::Save { .. }), "{d:?}");

    // A stored record that isn't the one the game made.
    let report = rerun(&j);
    let m = &report.saves[0];
    assert_eq!(report.check_stored(m.lease, m.seq, &m.record), Ok(()));
    let mut other = m.record;
    other[0] ^= 1;
    assert!(report.check_stored(m.lease, m.seq, &other).is_err());
}
