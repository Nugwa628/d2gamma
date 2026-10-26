//! The game server's journal recorder: [`JournalTap`], an observer on the
//! game host's packet recorder ([`d2_server::packets`]), and [`Recorder`],
//! the game server's handle for what the host doesn't see (whose seat a
//! client is, saves, state hashes, the end).
//!
//! The tap only reads: the host copies nothing for it beyond what its
//! packet recorder already reports, and the game runs the same with or
//! without it (frame hashes and determinism baselines don't move).
//!
//! What is written, in the order it happens:
//! - every C→S message the host's senders take, as [`Record::Intent`] (the
//!   game sender's clock reading as [`Record::Clock`] before it when it
//!   changed);
//! - every host frame that read a message or ran a tick, as
//!   [`Record::Frame`] with its clock reading (and the frame before
//!   anything else is written, so a re-run sees the same host tick);
//! - clients joining and leaving ([`Record::Join`], [`Record::Leave`]);
//! - saves, state hashes, item ID blocks and the end, from [`Recorder`].
//!   What the game hands the recorder while a host frame runs (a save at a
//!   leave the frame read, a transfer a handler made) is written when the
//!   frame is over ([`Recorder::after_frame`]), at the frame's tick: a
//!   re-run checks it after running the whole frame.
//!
//! Wiring (the game server, N7): build the [`Start`] record, make the pair
//! with [`Recorder::new`], install the tap with
//! `Host::set_packet_observer`, call [`Recorder::seat`] before the hub
//! connects a seated client, [`Recorder::saved`] from the save sink,
//! [`Recorder::after_frame`] after every hub frame, and [`Recorder::end`]
//! when the game closes.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use d2_server::packets::{PacketEvent, PacketObserver};
use d2_server::seams::ClientId;

use crate::chunk::{compress, JournalChunk, JournalSink, MAX_BODY};
use crate::format::{
    encode_entry, encode_header, leave, Digest, Entry, Record, Start, MAX_MESSAGE, MAX_RECORDS,
};

/// How often the recorder writes a state hash and cuts a chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecorderConfig {
    /// A [`Record::StateHash`] every this many ticks (contracts 11.11:
    /// 600, ten seconds at 60 Hz).
    pub hash_every: u32,
    /// A chunk is cut once it spans this many ticks (`journal_chunk_s` of
    /// `NodeWelcome` times the tick rate: 600).
    pub chunk_ticks: u32,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            hash_every: 600,
            chunk_ticks: 600,
        }
    }
}

/// What a recorder wrote so far (for the cost measurements of N7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecorderStats {
    pub records: u64,
    pub intents: u64,
    pub frames: u64,
    pub state_hashes: u64,
    pub saves: u64,
    /// Chunk bodies before compression.
    pub raw_bytes: u64,
    /// Chunks as gzip members.
    pub gz_bytes: u64,
    pub chunks: u64,
}

struct Inner {
    game: u32,
    cfg: RecorderConfig,
    /// The chunk being written: header and records.
    body: Vec<u8>,
    /// Records in `body`.
    chunk_records: usize,
    seq: u32,
    first_tick: Option<u32>,
    last_tick: u32,
    /// The game frame now.
    tick: u32,
    /// The latest frame's clock reading, while it is not written (a frame
    /// that read no message and ran no tick is dropped by the next).
    pending_frame: Option<u32>,
    frames_written: bool,
    /// A host frame is running (from its `Frame` event to
    /// [`Recorder::after_frame`]).
    in_frame: bool,
    /// The game server's records of the running frame.
    held: Vec<Record>,
    /// The last [`Record::Clock`] written.
    sender_clock: Option<u32>,
    seats: BTreeMap<ClientId, (u64, Digest)>,
    leaving: BTreeMap<ClientId, u8>,
    last_hash: Option<u32>,
    chunk_from: u32,
    ended: bool,
    sink: Option<Box<dyn JournalSink>>,
    out: Vec<JournalChunk>,
    stats: RecorderStats,
}

/// A client id as the journal writes it.
fn client16(c: ClientId) -> u16 {
    u16::try_from(c).unwrap_or(u16::MAX)
}

impl Inner {
    fn write(&mut self, record: Record) {
        if self.ended {
            return;
        }
        if !matches!(record, Record::Frame { .. }) {
            self.flush_frame();
        }
        self.write_now(record);
    }

    fn write_now(&mut self, record: Record) {
        let tick = self.tick;
        match &record {
            Record::Intent { .. } => self.stats.intents += 1,
            Record::Frame { .. } => self.stats.frames += 1,
            Record::StateHash(_) => self.stats.state_hashes += 1,
            Record::Save { .. } => self.stats.saves += 1,
            _ => {}
        }
        self.stats.records += 1;
        encode_entry(&mut self.body, &Entry { tick, record });
        self.first_tick.get_or_insert(tick);
        self.last_tick = tick;
        self.chunk_records += 1;
        // A chunk never holds more than a reader takes, however many
        // messages one frame brings: cut it early (a chunk may end
        // anywhere; the reader joins them).
        if self.chunk_records >= MAX_RECORDS || self.body.len() > MAX_BODY - 2 * MAX_MESSAGE {
            self.cut(false);
        }
    }

    /// Writes the frame waiting to be written, if any.
    fn flush_frame(&mut self) {
        if let Some(host_ms) = self.pending_frame.take() {
            self.write_now(Record::Frame { host_ms });
        }
    }

    /// A record of the game server's: held while a frame runs.
    fn put(&mut self, record: Record) {
        if self.in_frame && !self.ended {
            self.held.push(record);
        } else {
            self.write(record);
        }
    }

    /// The frame is over: writes what was held.
    fn frame_over(&mut self) {
        self.in_frame = false;
        for r in std::mem::take(&mut self.held) {
            self.write(r);
        }
    }

    fn frame(&mut self, now: u32) {
        if self.ended {
            return;
        }
        // Without an `after_frame` call since the last frame, that frame
        // ended here.
        self.frame_over();
        self.in_frame = true;
        // The first frame starts the host's tick schedule: always written.
        if !self.frames_written {
            self.frames_written = true;
            self.write_now(Record::Frame { host_ms: now });
        } else {
            self.pending_frame = Some(now);
        }
    }

    fn cut(&mut self, last: bool) {
        if self.first_tick.is_none() && !last {
            return;
        }
        let first = self.first_tick.unwrap_or(self.tick);
        let body = std::mem::take(&mut self.body);
        let gz = compress(&body);
        self.stats.raw_bytes += body.len() as u64;
        self.stats.gz_bytes += gz.len() as u64;
        self.stats.chunks += 1;
        let chunk = JournalChunk {
            game: self.game,
            seq: self.seq,
            first_tick: u64::from(first),
            ticks: self.last_tick.saturating_sub(first),
            last,
            gz,
        };
        self.seq += 1;
        self.chunk_records = 0;
        self.first_tick = None;
        self.chunk_from = self.tick;
        encode_header(&mut self.body, self.game, self.seq);
        match &self.sink {
            Some(s) => s.chunk(chunk),
            None => self.out.push(chunk),
        }
    }
}

/// The game server's handle on a game's journal.
#[derive(Clone)]
pub struct Recorder(Arc<Mutex<Inner>>);

/// The host's side of a recorder: install it with
/// `Host::set_packet_observer`.
pub struct JournalTap(Arc<Mutex<Inner>>);

impl Recorder {
    /// A recorder for game `start.spec.id`, whose game frame is `frame`
    /// now, and its tap. The first chunk starts with `start`.
    pub fn new(start: Start, frame: i32, cfg: RecorderConfig) -> (Recorder, JournalTap) {
        let game = start.spec.id;
        let tick = u32::try_from(frame).unwrap_or(0);
        let mut body = Vec::new();
        encode_header(&mut body, game, 0);
        let mut inner = Inner {
            game,
            cfg,
            body,
            chunk_records: 0,
            seq: 0,
            first_tick: None,
            last_tick: tick,
            tick,
            pending_frame: None,
            frames_written: false,
            in_frame: false,
            held: Vec::new(),
            sender_clock: None,
            seats: BTreeMap::new(),
            leaving: BTreeMap::new(),
            last_hash: None,
            chunk_from: tick,
            ended: false,
            sink: None,
            out: Vec::new(),
            stats: RecorderStats::default(),
        };
        inner.write(Record::Start(start));
        let shared = Arc::new(Mutex::new(inner));
        (Recorder(shared.clone()), JournalTap(shared))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Sends every chunk cut from now on to `sink` instead of keeping it
    /// for [`Recorder::take_chunks`].
    pub fn set_sink(&self, sink: Box<dyn JournalSink>) {
        let mut g = self.lock();
        for c in std::mem::take(&mut g.out) {
            sink.chunk(c);
        }
        g.sink = Some(sink);
    }

    /// Client `client` plays the seat of `lease`, whose character joined
    /// with the record of SHA-256 `record`. Call it before the host
    /// connects the client: its [`Record::Join`] is written then.
    pub fn seat(&self, client: ClientId, lease: u64, record: Digest) {
        self.lock().seats.insert(client, (lease, record));
    }

    /// The next leave of `client` is for this reason ([`leave`]); without
    /// one a leave is written as lost.
    pub fn leaving(&self, client: ClientId, how: u8) {
        self.lock().leaving.insert(client, how);
    }

    /// A character save the game made: its lease, sequence number and the
    /// record's digest ([`crate::server::save_digest`]).
    pub fn saved(&self, lease: u64, seq: u32, record: Digest) {
        self.lock().put(Record::Save { lease, seq, record });
    }

    /// A block of realm item IDs granted to the game.
    pub fn item_ids(&self, first: u32, count: u32) {
        self.lock().put(Record::ItemIds { first, count });
    }

    /// Writes a record the game server makes itself (the stash and market
    /// transfers, [`Record::ItemOut`] and [`Record::ItemIn`]).
    pub fn record(&self, record: Record) {
        self.lock().put(record);
    }

    /// After every hub frame: at the game frame `frame`, writes what the
    /// game server handed over during the frame, then the state hash when
    /// one is due (`state_hash` runs only then), and cuts a chunk when the
    /// current one spans [`RecorderConfig::chunk_ticks`].
    pub fn after_frame(&self, frame: i32, state_hash: impl FnOnce() -> Digest) {
        let mut g = self.lock();
        if g.ended {
            return;
        }
        let tick = u32::try_from(frame).unwrap_or(0);
        g.tick = tick;
        g.frame_over();
        if tick > 0 && tick % g.cfg.hash_every.max(1) == 0 && g.last_hash != Some(tick) {
            g.last_hash = Some(tick);
            g.write(Record::StateHash(state_hash()));
        }
        if tick.saturating_sub(g.chunk_from) >= g.cfg.chunk_ticks {
            g.flush_frame();
            g.cut(false);
        }
    }

    /// The game closed at game frame `frame`: writes the closing state hash
    /// and [`Record::End`], and cuts the last chunk.
    pub fn end(&self, frame: i32, reason: u16, state_hash: Digest) {
        let mut g = self.lock();
        if g.ended {
            return;
        }
        g.tick = u32::try_from(frame).unwrap_or(g.tick);
        g.frame_over();
        // A last frame that read nothing and ran no tick changed nothing.
        g.pending_frame = None;
        if g.last_hash != Some(g.tick) {
            g.last_hash = Some(g.tick);
            g.write(Record::StateHash(state_hash));
        }
        g.write(Record::End { reason });
        g.ended = true;
        g.cut(true);
    }

    /// Cuts the current chunk now (a link that asks for what there is).
    pub fn cut(&self) {
        let mut g = self.lock();
        if !g.ended {
            g.flush_frame();
            g.cut(false);
        }
    }

    /// The chunks cut so far that no sink took, in order.
    pub fn take_chunks(&self) -> Vec<JournalChunk> {
        std::mem::take(&mut self.lock().out)
    }

    /// What was written so far.
    pub fn stats(&self) -> RecorderStats {
        self.lock().stats
    }
}

impl PacketObserver for JournalTap {
    fn packet(&mut self, ev: PacketEvent<'_>) {
        let mut g = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match ev {
            PacketEvent::Frame { now } => g.frame(now),
            PacketEvent::C2s { .. } => g.flush_frame(),
            PacketEvent::Tick { frame } => {
                g.flush_frame();
                g.tick = u32::try_from(frame).unwrap_or(0);
            }
            PacketEvent::TickEnd { frame } => g.tick = u32::try_from(frame).unwrap_or(0),
            PacketEvent::ClientSend { client, msg, now } => {
                if g.ended {
                    return;
                }
                if g.sender_clock != Some(now) {
                    g.sender_clock = Some(now);
                    g.write(Record::Clock { host_ms: now });
                }
                g.write(Record::Intent {
                    client: client16(client),
                    system: false,
                    msg: msg.to_vec(),
                });
            }
            PacketEvent::ClientOut {
                client,
                msg,
                system: true,
            } => g.write(Record::Intent {
                client: client16(client),
                system: true,
                msg: msg.to_vec(),
            }),
            PacketEvent::Connect { client } => {
                let (lease, record) = g.seats.get(&client).copied().unwrap_or((0, [0; 32]));
                g.write(Record::Join {
                    client: client16(client),
                    lease,
                    record,
                });
            }
            PacketEvent::Disconnect { client } => {
                let how = g.leaving.remove(&client).unwrap_or(leave::LOST);
                g.write(Record::Leave {
                    client: client16(client),
                    how,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::decode_chunk;

    fn entries(rec: &Recorder) -> Vec<Entry> {
        rec.take_chunks()
            .iter()
            .flat_map(|c| decode_chunk(&c.gz).unwrap().entries)
            .collect()
    }

    /// A save made inside a host frame (a leave the frame's drain read) is
    /// written after the frame at the frame's tick, where a re-run checks
    /// it; a save between frames is written where it is made.
    #[test]
    fn a_save_inside_a_frame_is_written_after_it() {
        let (rec, mut tap) = Recorder::new(Start::default(), 10, RecorderConfig::default());
        tap.packet(PacketEvent::Frame { now: 1000 });
        rec.saved(5, 1, [1; 32]);
        tap.packet(PacketEvent::Tick { frame: 11 });
        tap.packet(PacketEvent::TickEnd { frame: 11 });
        rec.after_frame(11, || [0; 32]);
        rec.saved(5, 2, [2; 32]);
        // A frame with no `after_frame` call: the next frame ends it.
        tap.packet(PacketEvent::Frame { now: 1040 });
        tap.packet(PacketEvent::Tick { frame: 12 });
        rec.saved(5, 3, [3; 32]);
        tap.packet(PacketEvent::TickEnd { frame: 12 });
        tap.packet(PacketEvent::Frame { now: 1080 });
        rec.end(12, 0, [9; 32]);
        let got: Vec<(u32, u8)> = entries(&rec)
            .iter()
            .map(|e| (e.tick, e.record.kind()))
            .collect();
        use crate::format::kind::*;
        assert_eq!(
            got,
            [
                (10, START),
                (10, FRAME),
                (11, SAVE),
                (11, SAVE),
                (11, FRAME),
                (12, SAVE),
                (12, STATE_HASH),
                (12, END),
            ]
        );
    }

    /// A frame bringing more messages than a chunk holds is cut into
    /// several chunks, each of which the main server's reader takes.
    #[test]
    fn a_flood_of_messages_in_one_frame_is_cut_into_readable_chunks() {
        let (rec, mut tap) = Recorder::new(Start::default(), 10, RecorderConfig::default());
        tap.packet(PacketEvent::Frame { now: 1000 });
        let n = MAX_RECORDS + 100;
        for _ in 0..n {
            rec.record(Record::Intent {
                client: 1,
                system: false,
                msg: vec![0x10],
            });
        }
        rec.after_frame(11, || [0; 32]);
        rec.end(11, 0, [9; 32]);
        let chunks = rec.take_chunks();
        assert!(chunks.len() >= 2, "{} chunks", chunks.len());
        let total: usize = chunks
            .iter()
            .map(|c| {
                decode_chunk(&c.gz)
                    .expect("every chunk reads")
                    .entries
                    .len()
            })
            .sum();
        // Start, the first frame, the messages, the state hash and End.
        assert_eq!(total, n + 4);
    }
}
