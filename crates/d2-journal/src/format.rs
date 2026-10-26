//! The records of a game journal, format version 1, and their byte layout
//! (contracts 11.11).
//!
//! A chunk body is `u8 version = 1`, `u32 game`, `u32 seq`, then records
//! until its end. A record is `u32 tick` (the game frame it was written
//! at), `u8 kind`, then the kind's fields. Fields are little-endian; `str8`
//! is a byte length then UTF-8, `bytes16` a `u16` length then the bytes, a
//! `bool` one byte 0 or 1, an optional field a byte 0, or 1 then the field.
//! A chunk with bytes left over after a record, a record kind this version
//! doesn't know, or a field out of range does not decode.
//!
//! Beyond the kinds 1-11 of contracts 11.11, version 1 has kind 12,
//! [`Record::Frame`]: re-running a game needs to know where each host frame
//! ran, so the record of a frame's clock is its own kind, and kind 5
//! ([`Record::Clock`]) is the clock the host's game-message sender read for
//! the [`Record::Intent`]s after it.
//!
//! Kind 13 ([`Record::ItemCredits`]) follows the kind 10 of a mod game's
//! item that was put into an online stash as counts (a container folded
//! into what it holds); games without such puts never write it, so their
//! journals are unchanged.
//!
//! Kind 11 ([`Record::ItemIn`]) starts its goods with a tag: 0 an item, 1
//! a stack, and 2 an item asked for on the cursor (a stash take there).

use thiserror::Error;

/// The journal format version this crate writes and reads.
pub const VERSION: u8 = 1;

/// A SHA-256 digest.
pub type Digest = [u8; 32];

/// The largest message an [`Record::Intent`] holds (`bytes16`).
pub const MAX_MESSAGE: usize = u16::MAX as usize;

/// The most records one chunk body holds. A chunk spans 600 ticks: a
/// frame record per tick and every player's messages are a few thousand.
/// Without a cap a small gzip member of repeated 9-byte records would
/// decode to millions of entries (a decoded entry is several times its
/// bytes); with it a decoded chunk stays under about 40 MB.
pub const MAX_RECORDS: usize = 1 << 18;

/// Record kind numbers.
pub mod kind {
    pub const START: u8 = 1;
    pub const JOIN: u8 = 2;
    pub const INTENT: u8 = 3;
    pub const LEAVE: u8 = 4;
    pub const CLOCK: u8 = 5;
    pub const STATE_HASH: u8 = 6;
    pub const SAVE: u8 = 7;
    pub const ITEM_IDS: u8 = 8;
    pub const END: u8 = 9;
    pub const ITEM_OUT: u8 = 10;
    pub const ITEM_IN: u8 = 11;
    pub const FRAME: u8 = 12;
    pub const ITEM_CREDITS: u8 = 13;
}

/// How a client left ([`Record::Leave`]).
pub mod leave {
    /// It left the game (C→S 0x69, or its seat was given up).
    pub const LEFT: u8 = 0;
    /// Its connection was lost.
    pub const LOST: u8 = 1;
    /// The game took it out (a session takeover, a kick).
    pub const KICKED: u8 = 2;
    /// The game closed.
    pub const CLOSED: u8 = 3;
}

/// The game's settings, as the game server built it (contracts section 6
/// `GameSpec`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameSpec {
    pub id: u32,
    pub name: String,
    /// 0 Normal, 1 Nightmare, 2 Hell.
    pub difficulty: u8,
    pub hardcore: bool,
    pub expansion: bool,
    /// The most players (as the game server gave it to the game).
    pub max_players: u8,
}

/// The first record of a journal: what the game was built from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Start {
    /// The game server's program version: a replay needs the same build.
    pub version: String,
    /// The game data's hash: a replay needs the same data.
    pub data_hash: Digest,
    /// The game's seed (chosen by the main server).
    pub seed: u32,
    pub spec: GameSpec,
    /// The game's tick rate in Hz.
    pub tick_rate: u32,
    pub player_hosted: bool,
    /// The host's own client in a player-hosted game.
    pub host_client: Option<u16>,
}

/// What a stash or market item transfer moved ([`Record::ItemIn`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Goods {
    /// An item's bytes.
    Item(Vec<u8>),
    /// A count of a stackable.
    Stack { code: String, count: u32 },
}

/// One journal record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Record {
    /// Kind 1: the game's build, data, seed and settings (first record).
    Start(Start),
    /// Kind 2: a client joins, with the SHA-256 of the record its character
    /// joined with (the main server keeps the record).
    Join {
        client: u16,
        lease: u64,
        record: Digest,
    },
    /// Kind 3: a C→S message as the host's sender took it from the client's
    /// stream (after the seat filter), in arrival order: `system` for the
    /// system sender, otherwise the game sender (which may still drop it as
    /// a duplicate, at the last [`Record::Clock`]).
    Intent {
        client: u16,
        system: bool,
        msg: Vec<u8>,
    },
    /// Kind 4: a client leaves (`how`: [`leave`]).
    Leave { client: u16, how: u8 },
    /// Kind 5: the host clock reading the game sender reads for the
    /// [`Record::Intent`]s that follow (written when it changes).
    Clock { host_ms: u32 },
    /// Kind 6: SHA-256 of the game's `state-1` line at this tick.
    StateHash(Digest),
    /// Kind 7: a character save, SHA-256 of its record with its save time
    /// zeroed. A save made while a host frame ran is written after the
    /// frame, at the frame's tick (so are kinds 8, 10 and 11).
    Save {
        lease: u64,
        seq: u32,
        record: Digest,
    },
    /// Kind 8: a block of realm item IDs granted to the game.
    ItemIds { first: u32, count: u32 },
    /// Kind 9: the game ended (last record).
    End { reason: u16 },
    /// Kind 10: an item, a stack or gold left a character for the stash.
    ItemOut {
        client: u16,
        req: u64,
        unit: u32,
        item_id: Option<u32>,
        stack: Option<(String, u32)>,
        gold: u64,
    },
    /// Kind 11: an item, a stack or gold from the stash or the market was
    /// placed on a character (`result` 0) or refused. `cursor`: the item
    /// was asked for on the cursor (goods tag 2), else the inventory.
    ItemIn {
        client: u16,
        req: u64,
        goods: Goods,
        gold: u64,
        cursor: bool,
        result: u8,
    },
    /// Kind 12: a host frame ran with this clock reading. Frames that read
    /// no message and ran no tick are left out (they change nothing).
    Frame { host_ms: u32 },
    /// Kind 13: the item of the [`Record::ItemOut`] just before it (same
    /// `client` and `req`) became these `(code, count)` credits for the
    /// stash instead of an item. A re-run takes the item out as kind 10
    /// says; the credits are what the game server told the main server.
    ItemCredits {
        client: u16,
        req: u64,
        credits: Vec<(String, u32)>,
    },
}

/// The most credits one [`Record::ItemCredits`] holds (longer is cut).
pub const MAX_CREDITS: usize = 32;

impl Record {
    /// The record's kind number.
    pub fn kind(&self) -> u8 {
        match self {
            Record::Start(_) => kind::START,
            Record::Join { .. } => kind::JOIN,
            Record::Intent { .. } => kind::INTENT,
            Record::Leave { .. } => kind::LEAVE,
            Record::Clock { .. } => kind::CLOCK,
            Record::StateHash(_) => kind::STATE_HASH,
            Record::Save { .. } => kind::SAVE,
            Record::ItemIds { .. } => kind::ITEM_IDS,
            Record::End { .. } => kind::END,
            Record::ItemOut { .. } => kind::ITEM_OUT,
            Record::ItemIn { .. } => kind::ITEM_IN,
            Record::Frame { .. } => kind::FRAME,
            Record::ItemCredits { .. } => kind::ITEM_CREDITS,
        }
    }
}

/// A record and the game frame it was written at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub tick: u32,
    pub record: Record,
}

/// Why journal bytes don't decode.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum FormatError {
    #[error("the data ends inside a record")]
    Truncated,
    #[error("journal format version {0} (this build reads version {VERSION})")]
    Version(u8),
    #[error("unknown record kind {0}")]
    Kind(u8),
    #[error("a field is out of range: {0}")]
    Field(&'static str),
    #[error("more than {MAX_RECORDS} records in one chunk")]
    TooManyRecords,
}

/// Appends fields to a byte buffer.
#[derive(Debug)]
pub(crate) struct Put<'a>(pub &'a mut Vec<u8>);

impl Put<'_> {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }
    /// At most 255 bytes, cut at a character boundary.
    pub fn str8(&mut self, s: &str) {
        let mut n = s.len().min(usize::from(u8::MAX));
        while !s.is_char_boundary(n) {
            n -= 1;
        }
        self.u8(n as u8);
        self.0.extend_from_slice(&s.as_bytes()[..n]);
    }
    /// At most [`MAX_MESSAGE`] bytes (longer is cut).
    pub fn bytes16(&mut self, b: &[u8]) {
        let n = b.len().min(MAX_MESSAGE);
        self.u16(n as u16);
        self.0.extend_from_slice(&b[..n]);
    }
    pub fn digest(&mut self, d: &Digest) {
        self.0.extend_from_slice(d);
    }
}

/// Reads fields from a byte slice.
#[derive(Debug)]
pub(crate) struct Take<'a>(pub &'a [u8]);

impl<'a> Take<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], FormatError> {
        if self.0.len() < n {
            return Err(FormatError::Truncated);
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    pub fn u8(&mut self) -> Result<u8, FormatError> {
        Ok(self.bytes(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, FormatError> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().expect("2")))
    }
    pub fn u32(&mut self) -> Result<u32, FormatError> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().expect("4")))
    }
    pub fn u64(&mut self) -> Result<u64, FormatError> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().expect("8")))
    }
    pub fn bool(&mut self, what: &'static str) -> Result<bool, FormatError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(FormatError::Field(what)),
        }
    }
    fn opt(&mut self, what: &'static str) -> Result<bool, FormatError> {
        self.bool(what)
    }
    pub fn str8(&mut self, what: &'static str) -> Result<String, FormatError> {
        let n = usize::from(self.u8()?);
        let b = self.bytes(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| FormatError::Field(what))
    }
    pub fn bytes16(&mut self) -> Result<Vec<u8>, FormatError> {
        let n = usize::from(self.u16()?);
        Ok(self.bytes(n)?.to_vec())
    }
    pub fn digest(&mut self) -> Result<Digest, FormatError> {
        Ok(self.bytes(32)?.try_into().expect("32"))
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn put_spec(p: &mut Put<'_>, s: &GameSpec) {
    p.u32(s.id);
    p.str8(&s.name);
    p.u8(s.difficulty);
    p.bool(s.hardcore);
    p.bool(s.expansion);
    p.u8(s.max_players);
}

fn take_spec(t: &mut Take<'_>) -> Result<GameSpec, FormatError> {
    let spec = GameSpec {
        id: t.u32()?,
        name: t.str8("game name")?,
        difficulty: t.u8()?,
        hardcore: t.bool("hardcore")?,
        expansion: t.bool("expansion")?,
        max_players: t.u8()?,
    };
    if spec.difficulty > 2 {
        return Err(FormatError::Field("difficulty"));
    }
    Ok(spec)
}

/// Appends `e` to `out`.
pub fn encode_entry(out: &mut Vec<u8>, e: &Entry) {
    let mut p = Put(out);
    p.u32(e.tick);
    p.u8(e.record.kind());
    match &e.record {
        Record::Start(s) => {
            p.str8(&s.version);
            p.digest(&s.data_hash);
            p.u32(s.seed);
            put_spec(&mut p, &s.spec);
            p.u32(s.tick_rate);
            p.bool(s.player_hosted);
            match s.host_client {
                Some(c) => {
                    p.u8(1);
                    p.u16(c);
                }
                None => p.u8(0),
            }
        }
        Record::Join {
            client,
            lease,
            record,
        } => {
            p.u16(*client);
            p.u64(*lease);
            p.digest(record);
        }
        Record::Intent {
            client,
            system,
            msg,
        } => {
            p.u16(*client);
            p.bool(*system);
            p.bytes16(msg);
        }
        Record::Leave { client, how } => {
            p.u16(*client);
            p.u8(*how);
        }
        Record::Clock { host_ms } | Record::Frame { host_ms } => p.u32(*host_ms),
        Record::ItemCredits {
            client,
            req,
            credits,
        } => {
            p.u16(*client);
            p.u64(*req);
            let n = credits.len().min(MAX_CREDITS);
            p.u8(n as u8);
            for (code, count) in &credits[..n] {
                p.str8(code);
                p.u32(*count);
            }
        }
        Record::StateHash(d) => p.digest(d),
        Record::Save { lease, seq, record } => {
            p.u64(*lease);
            p.u32(*seq);
            p.digest(record);
        }
        Record::ItemIds { first, count } => {
            p.u32(*first);
            p.u32(*count);
        }
        Record::End { reason } => p.u16(*reason),
        Record::ItemOut {
            client,
            req,
            unit,
            item_id,
            stack,
            gold,
        } => {
            p.u16(*client);
            p.u64(*req);
            p.u32(*unit);
            match item_id {
                Some(id) => {
                    p.u8(1);
                    p.u32(*id);
                }
                None => p.u8(0),
            }
            match stack {
                Some((code, count)) => {
                    p.u8(1);
                    p.str8(code);
                    p.u32(*count);
                }
                None => p.u8(0),
            }
            p.u64(*gold);
        }
        Record::ItemIn {
            client,
            req,
            goods,
            gold,
            cursor,
            result,
        } => {
            p.u16(*client);
            p.u64(*req);
            match goods {
                Goods::Item(b) => {
                    p.u8(if *cursor { 2 } else { 0 });
                    p.bytes16(b);
                }
                Goods::Stack { code, count } => {
                    p.u8(1);
                    p.str8(code);
                    p.u32(*count);
                }
            }
            p.u64(*gold);
            p.u8(*result);
        }
    }
}

/// Reads one record from `t`.
pub(crate) fn decode_entry(t: &mut Take<'_>) -> Result<Entry, FormatError> {
    let tick = t.u32()?;
    let k = t.u8()?;
    let record = match k {
        kind::START => Record::Start(Start {
            version: t.str8("version")?,
            data_hash: t.digest()?,
            seed: t.u32()?,
            spec: take_spec(t)?,
            tick_rate: t.u32()?,
            player_hosted: t.bool("player_hosted")?,
            host_client: if t.opt("host_client")? {
                Some(t.u16()?)
            } else {
                None
            },
        }),
        kind::JOIN => Record::Join {
            client: t.u16()?,
            lease: t.u64()?,
            record: t.digest()?,
        },
        kind::INTENT => Record::Intent {
            client: t.u16()?,
            system: t.bool("system")?,
            msg: t.bytes16()?,
        },
        kind::LEAVE => {
            let client = t.u16()?;
            let how = t.u8()?;
            if how > leave::CLOSED {
                return Err(FormatError::Field("leave how"));
            }
            Record::Leave { client, how }
        }
        kind::CLOCK => Record::Clock { host_ms: t.u32()? },
        kind::STATE_HASH => Record::StateHash(t.digest()?),
        kind::SAVE => Record::Save {
            lease: t.u64()?,
            seq: t.u32()?,
            record: t.digest()?,
        },
        kind::ITEM_IDS => Record::ItemIds {
            first: t.u32()?,
            count: t.u32()?,
        },
        kind::END => Record::End { reason: t.u16()? },
        kind::ITEM_OUT => Record::ItemOut {
            client: t.u16()?,
            req: t.u64()?,
            unit: t.u32()?,
            item_id: if t.opt("item_id")? {
                Some(t.u32()?)
            } else {
                None
            },
            stack: if t.opt("stack")? {
                Some((t.str8("stack code")?, t.u32()?))
            } else {
                None
            },
            gold: t.u64()?,
        },
        kind::ITEM_IN => {
            let client = t.u16()?;
            let req = t.u64()?;
            let tag = t.u8()?;
            let goods = match tag {
                0 | 2 => Goods::Item(t.bytes16()?),
                1 => Goods::Stack {
                    code: t.str8("stack code")?,
                    count: t.u32()?,
                },
                _ => return Err(FormatError::Field("goods")),
            };
            Record::ItemIn {
                client,
                req,
                goods,
                gold: t.u64()?,
                cursor: tag == 2,
                result: t.u8()?,
            }
        }
        kind::FRAME => Record::Frame { host_ms: t.u32()? },
        kind::ITEM_CREDITS => {
            let client = t.u16()?;
            let req = t.u64()?;
            let n = usize::from(t.u8()?);
            if n > MAX_CREDITS {
                return Err(FormatError::Field("credits"));
            }
            let mut credits = Vec::with_capacity(n);
            for _ in 0..n {
                credits.push((t.str8("credit code")?, t.u32()?));
            }
            Record::ItemCredits {
                client,
                req,
                credits,
            }
        }
        other => return Err(FormatError::Kind(other)),
    };
    Ok(Entry { tick, record })
}

/// The header of a chunk body: format version, game, chunk number.
pub(crate) fn encode_header(out: &mut Vec<u8>, game: u32, seq: u32) {
    let mut p = Put(out);
    p.u8(VERSION);
    p.u32(game);
    p.u32(seq);
}

/// A decompressed chunk body: its header and records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkBody {
    pub game: u32,
    pub seq: u32,
    pub entries: Vec<Entry>,
}

/// Decodes a decompressed chunk body.
pub fn decode_body(body: &[u8]) -> Result<ChunkBody, FormatError> {
    let mut t = Take(body);
    let version = t.u8()?;
    if version != VERSION {
        return Err(FormatError::Version(version));
    }
    let game = t.u32()?;
    let seq = t.u32()?;
    let mut entries = Vec::new();
    while !t.is_empty() {
        if entries.len() == MAX_RECORDS {
            return Err(FormatError::TooManyRecords);
        }
        entries.push(decode_entry(&mut t)?);
    }
    Ok(ChunkBody { game, seq, entries })
}

/// Encodes a chunk body (uncompressed).
pub fn encode_body(game: u32, seq: u32, entries: &[Entry]) -> Vec<u8> {
    let mut out = Vec::new();
    encode_header(&mut out, game, seq);
    for e in entries {
        encode_entry(&mut out, e);
    }
    out
}
