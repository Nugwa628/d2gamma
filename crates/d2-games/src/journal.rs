//! The games' journals (plan N7, contracts 11.11): once
//! [`crate::Games::set_journals`] is set, every game built from then on
//! records its journal with `d2-journal`'s recorder and hands each chunk to
//! its game's sink. The recorder only reads: a game runs the same with or
//! without it.
//!
//! What the game thread tells the recorder: the `Start` record when the
//! game is built (and its first block of realm item IDs), each seat's lease
//! and the digest of what it joins with before the seat is connected, each
//! save's digest, why a player leaves, each block granted later, every
//! frame (a state hash every [`RecorderConfig::hash_every`] ticks, a chunk
//! every [`RecorderConfig::chunk_ticks`]) and the end.

use std::sync::Arc;

pub use d2_journal::recorder::{Recorder, RecorderConfig};
pub use d2_journal::{Digest, JournalChunk, JournalSink};

use d2_journal::server::{new_character_bytes, sha256, NewChar};

use crate::Seat;

/// Where the games' journals go, and what their `Start` records name.
#[derive(Clone)]
pub struct JournalSetup {
    /// The program's version (`Start.version`): a re-run needs the same
    /// build.
    pub version: String,
    /// The game data's hash (`Start.data_hash`): a re-run needs the same
    /// data.
    pub data_hash: Digest,
    pub cfg: RecorderConfig,
    /// The sink of game `id`'s chunks.
    pub sinks: Arc<dyn Fn(u32) -> Box<dyn JournalSink> + Send + Sync>,
}

/// What a seat's `Join` record names (as `d2_journal::server::SeatCharacter`
/// does): SHA-256 of the record the character joins with, or of a new
/// character's description.
pub fn join_digest(seat: &Seat) -> Digest {
    match (&seat.new, &seat.record) {
        (Some(n), _) => sha256(&new_character_bytes(
            &seat.character,
            &NewChar {
                class: n.class,
                hardcore: n.hardcore,
                progression: n.progression,
            },
        )),
        (None, Some(record)) => sha256(record),
        (None, None) => [0; 32],
    }
}
