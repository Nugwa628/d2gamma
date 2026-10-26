//! Where a game's saves and seeds come from and go to: the two seams that
//! keep the games free of any character folder (contracts 11.1).
//!
//! - [`SaveSink`]: the game thread makes a character's record and hands it
//!   to the sink, then goes on with its frame. The sink stores it on its
//!   own thread: v1's server writes the character file, a game server
//!   sends it to the main server.
//! - [`SeedSource`]: the seed a game's map is built with, and its first
//!   block of realm item IDs. v1's server takes the seed from the clock
//!   ([`ClockSeed`]) and has no blocks; a game server takes both from the
//!   main server's `StartGame`.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use d2_net::msg::account::CharSummary;
use d2_service::character::AccountId;

use crate::transfer::Transfer;
use crate::{GameSpec, IdBlock};

/// One character's save, as the game hands it off.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveRecord {
    pub game: u32,
    pub account: AccountId,
    /// The character's name as stored.
    pub character: String,
    /// The seat's save number: 1 for its first save in this game, then
    /// one more at each save. A main server stores a save only when its
    /// number rises (contracts 11.6), so an older save that arrives late
    /// can't overwrite a newer one.
    pub seq: u32,
    /// The record, in D2RUST's character format (without any envelope).
    pub record: Vec<u8>,
    /// What character select and the lobby read of it.
    pub summary: CharSummary,
    /// The stash transfer this save goes with: it is sent with the
    /// transfer (contracts 11.13) instead of as a plain save.
    pub transfer: Option<Box<Transfer>>,
}

/// What runs once a save is stored (`Ok`) or failed (`Err`, why).
pub type SaveDone = Box<dyn FnOnce(Result<(), String>) + Send>;

/// Where saves go. Every method returns at once except [`SaveSink::flush`];
/// the work happens on the sink's own thread, in the order it was handed
/// off.
pub trait SaveSink: Send + Sync + 'static {
    /// Takes one save; `done` runs once it is stored or failed.
    fn save(&self, save: SaveRecord, done: SaveDone);
    /// Runs `then` once every save handed off before it is done: the game
    /// tells the lobby a player left only after its last save is stored.
    fn then(&self, then: Box<dyn FnOnce() + Send>);
    /// Waits up to `wait` until everything handed off so far is done.
    /// False if it timed out.
    fn flush(&self, wait: Duration) -> bool;
}

/// The seed a game is built with, and the first block of realm item IDs
/// it numbers its items from.
pub trait SeedSource: Send + Sync + 'static {
    fn seed(&self, spec: &GameSpec) -> u32;

    /// The game's first block of realm item IDs (`StartGame`); more come
    /// through [`crate::GameHandle::grant_item_ids`]. `None` (the default):
    /// the game's items keep their init seeds as their IDs.
    fn item_ids(&self, _spec: &GameSpec) -> Option<IdBlock> {
        None
    }
}

/// v1's seeds: from the clock, like a game the client makes.
#[derive(Clone, Copy, Debug, Default)]
pub struct ClockSeed;

impl SeedSource for ClockSeed {
    fn seed(&self, _spec: &GameSpec) -> u32 {
        d2_server::game::clock_map_seed()
    }
}

enum Job {
    Save(SaveRecord, SaveDone),
    Then(Box<dyn FnOnce() + Send>),
}

/// Jobs handed off and not done yet, and the condition flushes wait on.
#[derive(Default)]
struct Pending {
    count: Mutex<u64>,
    done: Condvar,
}

impl Pending {
    fn add(&self) {
        *self.count.lock().unwrap_or_else(|e| e.into_inner()) += 1;
    }

    fn finish(&self) {
        let mut n = self.count.lock().unwrap_or_else(|e| e.into_inner());
        *n = n.saturating_sub(1);
        if *n == 0 {
            self.done.notify_all();
        }
    }
}

/// A [`SaveSink`] that stores each save with `store` on one thread of its
/// own, in hand-off order. The thread ends when the sink is dropped.
pub struct QueuedSink {
    tx: Mutex<Sender<Job>>,
    pending: Arc<Pending>,
}

impl QueuedSink {
    /// A sink whose thread (named `name`) stores each save with `store`.
    pub fn new(
        name: &str,
        store: impl FnMut(&SaveRecord) -> Result<(), String> + Send + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let pending = Arc::new(Pending::default());
        let worker = pending.clone();
        std::thread::Builder::new()
            .name(name.into())
            .spawn(move || run(rx, store, &worker))
            .expect("starting the save thread");
        Self {
            tx: Mutex::new(tx),
            pending,
        }
    }

    fn push(&self, job: Job) {
        self.pending.add();
        let sent = self.tx.lock().unwrap_or_else(|e| e.into_inner()).send(job);
        if let Err(mpsc::SendError(job)) = sent {
            // The thread is gone (it panicked): nothing is stored any more.
            self.pending.finish();
            match job {
                Job::Save(_, done) => done(Err("the save thread stopped".into())),
                Job::Then(then) => then(),
            }
        }
    }
}

fn run(
    rx: Receiver<Job>,
    mut store: impl FnMut(&SaveRecord) -> Result<(), String>,
    pending: &Pending,
) {
    for job in rx {
        match job {
            Job::Save(save, done) => done(store(&save)),
            Job::Then(then) => then(),
        }
        pending.finish();
    }
}

impl SaveSink for QueuedSink {
    fn save(&self, save: SaveRecord, done: SaveDone) {
        self.push(Job::Save(save, done));
    }

    fn then(&self, then: Box<dyn FnOnce() + Send>) {
        self.push(Job::Then(then));
    }

    fn flush(&self, wait: Duration) -> bool {
        let until = Instant::now() + wait;
        let mut n = self.pending.count.lock().unwrap_or_else(|e| e.into_inner());
        while *n > 0 {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            n = self
                .pending
                .done
                .wait_timeout(n, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }
}

/// The latest record of each (account, character).
type Records = Arc<Mutex<BTreeMap<(AccountId, String), Vec<u8>>>>;

/// A [`SaveSink`] that keeps the latest record of every character in
/// memory: the probe's, and the tests'.
#[derive(Clone)]
pub struct MemorySink {
    queue: Arc<QueuedSink>,
    records: Records,
}

impl Default for MemorySink {
    fn default() -> Self {
        let records = Records::default();
        let kept = records.clone();
        let queue = QueuedSink::new("saves-memory", move |s| {
            kept.lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert((s.account, s.character.clone()), s.record.clone());
            Ok(())
        });
        Self {
            queue: Arc::new(queue),
            records,
        }
    }
}

impl MemorySink {
    /// The latest stored record of `character` (the name as stored).
    pub fn record(&self, account: AccountId, character: &str) -> Option<Vec<u8>> {
        self.records
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(account, character.to_owned()))
            .cloned()
    }

    /// How many characters have a stored record.
    pub fn len(&self) -> usize {
        self.records.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SaveSink for MemorySink {
    fn save(&self, save: SaveRecord, done: SaveDone) {
        self.queue.save(save, done);
    }

    fn then(&self, then: Box<dyn FnOnce() + Send>) {
        self.queue.then(then);
    }

    fn flush(&self, wait: Duration) -> bool {
        self.queue.flush(wait)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(n: u8) -> SaveRecord {
        SaveRecord {
            game: 1,
            account: AccountId(1),
            character: "Kashya".into(),
            seq: n.into(),
            record: vec![n],
            summary: CharSummary {
                name: "Kashya".into(),
                class: 0,
                level: 1,
                status: 0,
                difficulties: 1,
                look: Vec::new(),
                last_played: 0,
                season: 0,
            },
            transfer: None,
        }
    }

    /// The hand-off returns at once however slow the store is, and the
    /// saves and the `then`s run in hand-off order.
    #[test]
    fn hand_off_returns_at_once_and_keeps_the_order() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let stored = order.clone();
        let sink = QueuedSink::new("saves-test", move |s| {
            std::thread::sleep(Duration::from_millis(100));
            stored
                .lock()
                .unwrap()
                .push(format!("store {}", s.record[0]));
            Ok(())
        });
        let t = Instant::now();
        for n in 0..3 {
            let done_order = order.clone();
            sink.save(
                record(n),
                Box::new(move |r| done_order.lock().unwrap().push(format!("done {n} {r:?}"))),
            );
        }
        let then_order = order.clone();
        sink.then(Box::new(move || {
            then_order.lock().unwrap().push("then".into())
        }));
        let handed = t.elapsed();
        assert!(
            handed < Duration::from_millis(50),
            "hand-off took {handed:?}"
        );
        assert!(!sink.flush(Duration::ZERO), "still storing");
        assert!(sink.flush(Duration::from_secs(10)));
        assert_eq!(
            *order.lock().unwrap(),
            [
                "store 0",
                "done 0 Ok(())",
                "store 1",
                "done 1 Ok(())",
                "store 2",
                "done 2 Ok(())",
                "then"
            ]
        );
    }

    #[test]
    fn a_failed_store_is_told_to_done() {
        let sink = QueuedSink::new("saves-test", |_| Err("disk full".into()));
        let (tx, rx) = mpsc::channel();
        sink.save(record(0), Box::new(move |r| tx.send(r).unwrap()));
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(10)).unwrap(),
            Err("disk full".to_owned())
        );
    }

    #[test]
    fn the_memory_sink_keeps_the_latest_record() {
        let sink = MemorySink::default();
        sink.save(record(1), Box::new(|_| {}));
        sink.save(record(2), Box::new(|_| {}));
        assert!(sink.flush(Duration::from_secs(10)));
        assert_eq!(sink.record(AccountId(1), "Kashya"), Some(vec![2]));
        assert_eq!(sink.len(), 1);
    }
}
