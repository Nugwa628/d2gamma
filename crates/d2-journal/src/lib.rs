//! The game journal (plan `multiplayer-v2` C2 and C4, contracts 11.11):
//! everything a game server's game took in, so the main server can re-run
//! the game and check it.
//!
//! - [`format`]: the records and their bytes (format version 1).
//! - [`chunk`]: chunks as gzip members, the [`chunk::JournalSink`] they go
//!   to, and the reader of a journal file ([`chunk::read_journal`]).
//! - [`digest`]: the clients' intent digests and their check.
//! - [`recorder`] (feature `host`): the recorder on the game host.
//! - [`replay`]: the replay audit engine; [`server`] (feature `host`):
//!   D2RUST's own game for it, built as the game server builds it.

pub mod chunk;
pub mod digest;
pub mod format;
pub mod replay;

#[cfg(feature = "host")]
pub mod recorder;
#[cfg(feature = "host")]
pub mod server;

pub use chunk::{read_journal, Journal, JournalChunk, JournalError, JournalSink};
pub use format::{Digest, Entry, GameSpec, Record, Start};
