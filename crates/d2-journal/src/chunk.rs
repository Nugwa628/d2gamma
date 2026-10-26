//! Journal chunks on the wire and on disk.
//!
//! A game server cuts its journal into chunks (every `journal_chunk_s`, and
//! at the end); each chunk's body is one gzip member. The main server checks
//! a chunk decodes and follows the one before it, then appends the member
//! unchanged to the game's file (`data/journals/<day>/<game>.d2j.gz`), so a
//! journal file is its chunks' members one after another, which any gzip
//! reader also reads as one stream. [`read_journal`] reads such a file back
//! chunk by chunk and checks their order.

use std::io::{Read, Write};

use flate2::bufread::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use thiserror::Error;

use crate::format::{decode_body, ChunkBody, Entry, FormatError, Record, Start};

/// The largest chunk body a reader inflates (contracts 11.2: a chunk is at
/// most 1 MiB on the link; its body may be larger before compression).
pub const MAX_BODY: usize = 64 << 20;

/// One chunk as the game server sends it (link message `JournalChunk`,
/// contracts 11.3) and the main server stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalChunk {
    pub game: u32,
    /// The chunk's number in its game, from 0.
    pub seq: u32,
    /// The tick of its first record.
    pub first_tick: u64,
    /// Ticks from the first record's to the last's.
    pub ticks: u32,
    /// The game's last chunk (it ends with [`Record::End`]).
    pub last: bool,
    /// The body as one gzip member.
    pub gz: Vec<u8>,
}

/// Where a game server's chunks go (the link to the main server; a file in
/// tests). Contracts 11.1 names it for `d2-games`.
pub trait JournalSink: Send + 'static {
    fn chunk(&self, c: JournalChunk);
}

/// Compresses a chunk body into one gzip member.
pub fn compress(body: &[u8]) -> Vec<u8> {
    let mut e = GzEncoder::new(Vec::with_capacity(body.len() / 4 + 64), Compression::new(6));
    e.write_all(body).expect("writing to a Vec");
    e.finish().expect("writing to a Vec")
}

/// Why a journal doesn't read.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum JournalError {
    #[error("chunk {chunk}: not gzip, or cut short: {why}")]
    Gzip { chunk: usize, why: String },
    #[error("chunk {chunk}: its body is over {MAX_BODY} bytes")]
    TooLarge { chunk: usize },
    #[error("chunk {chunk}: {source}")]
    Format { chunk: usize, source: FormatError },
    #[error("chunk {chunk} is of game {got}, the journal's is {want}")]
    OtherGame { chunk: usize, want: u32, got: u32 },
    #[error("chunk {chunk} has number {got}, expected {want}")]
    Order { chunk: usize, want: u32, got: u32 },
    #[error("the journal doesn't start with a Start record")]
    NoStart,
    #[error("a second Start record at tick {tick}")]
    SecondStart { tick: u32 },
    #[error("record {index} goes back in time: tick {tick} after {before}")]
    TimeBack {
        index: usize,
        tick: u32,
        before: u32,
    },
    #[error("records after the End record (tick {tick})")]
    AfterEnd { tick: u32 },
    #[error("the journal is empty")]
    Empty,
}

/// Inflates one gzip member from the front of `input`, leaving the rest.
fn inflate_member(input: &mut &[u8], chunk: usize) -> Result<Vec<u8>, JournalError> {
    let mut d = GzDecoder::new(&mut *input);
    let mut body = Vec::new();
    let read = (&mut d)
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut body)
        .map_err(|e| JournalError::Gzip {
            chunk,
            why: e.to_string(),
        })?;
    if read > MAX_BODY {
        return Err(JournalError::TooLarge { chunk });
    }
    // The decoder stops at the member's end, so the slice it read from is
    // left at the next member.
    Ok(body)
}

/// Decodes one chunk as the main server gets it: one gzip member holding
/// one chunk body.
pub fn decode_chunk(gz: &[u8]) -> Result<ChunkBody, JournalError> {
    let mut input = gz;
    let body = inflate_member(&mut input, 0)?;
    if !input.is_empty() {
        return Err(JournalError::Gzip {
            chunk: 0,
            why: "bytes after the member".into(),
        });
    }
    decode_body(&body).map_err(|source| JournalError::Format { chunk: 0, source })
}

/// A whole journal, read and checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Journal {
    pub game: u32,
    /// The chunks read.
    pub chunks: u32,
    /// Every record in order; the first is [`Record::Start`].
    pub entries: Vec<Entry>,
    /// The journal ends with [`Record::End`] (the game closed); a journal
    /// without one was cut off (the game server was lost).
    pub ended: bool,
}

impl Journal {
    /// The game's [`Start`] record.
    pub fn start(&self) -> &Start {
        match &self.entries[0].record {
            Record::Start(s) => s,
            _ => unreachable!("checked when read"),
        }
    }

    /// Checks a sequence of chunk bodies: one game, numbered from 0 in
    /// order, a Start record first and only there, ticks never going back,
    /// nothing after an End record.
    pub fn from_bodies(bodies: Vec<ChunkBody>) -> Result<Journal, JournalError> {
        let game = bodies.first().ok_or(JournalError::Empty)?.game;
        let mut entries = Vec::new();
        let chunks = bodies.len() as u32;
        for (i, b) in bodies.into_iter().enumerate() {
            if b.game != game {
                return Err(JournalError::OtherGame {
                    chunk: i,
                    want: game,
                    got: b.game,
                });
            }
            if b.seq != i as u32 {
                return Err(JournalError::Order {
                    chunk: i,
                    want: i as u32,
                    got: b.seq,
                });
            }
            entries.extend(b.entries);
        }
        if !matches!(entries.first().map(|e| &e.record), Some(Record::Start(_))) {
            return Err(JournalError::NoStart);
        }
        let mut ended = false;
        let mut before = 0;
        for (i, e) in entries.iter().enumerate() {
            if ended {
                return Err(JournalError::AfterEnd { tick: e.tick });
            }
            if i > 0 && matches!(e.record, Record::Start(_)) {
                return Err(JournalError::SecondStart { tick: e.tick });
            }
            if e.tick < before {
                return Err(JournalError::TimeBack {
                    index: i,
                    tick: e.tick,
                    before,
                });
            }
            before = e.tick;
            ended = matches!(e.record, Record::End { .. });
        }
        Ok(Journal {
            game,
            chunks,
            entries,
            ended,
        })
    }
}

/// Reads a journal file: its chunks' gzip members one after another.
pub fn read_journal(mut bytes: &[u8]) -> Result<Journal, JournalError> {
    let mut bodies = Vec::new();
    while !bytes.is_empty() {
        let chunk = bodies.len();
        let body = inflate_member(&mut bytes, chunk)?;
        bodies.push(decode_body(&body).map_err(|source| JournalError::Format { chunk, source })?);
    }
    Journal::from_bodies(bodies)
}
