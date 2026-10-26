//! Client intent digests (plan C2, contracts 11.4 `IntentDigest`): each
//! client hashes the game messages it sends, in spans (every 10 s and on
//! leave), and sends the main server each span's count and digest. The
//! audit checks a client's [`Record::Intent`]s in the journal against
//! them, so a game server can't put actions a player never sent into its
//! journal.
//!
//! A span's digest is SHA-256 over its messages in send order, each as a
//! `u16` little-endian length, then the bytes. The journal's messages are
//! split into spans by the clients' counts, in order.
//!
//! This part of the crate has no game code: the online client library
//! uses it without the game host.

use sha2::{Digest as _, Sha256};

use crate::chunk::Journal;
use crate::format::{Digest, Record};

/// One span's digest, as the client sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentDigest {
    /// The client's game frame at the span's first and last message.
    pub first_tick: u64,
    pub last_tick: u64,
    /// Messages in the span.
    pub count: u32,
    pub digest: Digest,
}

/// The client's running span.
#[derive(Clone, Debug, Default)]
pub struct IntentDigester {
    hasher: Sha256,
    count: u32,
    first_tick: u64,
    last_tick: u64,
}

fn feed(h: &mut Sha256, msg: &[u8]) {
    let n = msg.len().min(usize::from(u16::MAX));
    h.update((n as u16).to_le_bytes());
    h.update(&msg[..n]);
}

impl IntentDigester {
    pub fn new() -> Self {
        Self::default()
    }

    /// A game message the client sent at its game frame `tick`.
    pub fn sent(&mut self, tick: u64, msg: &[u8]) {
        if self.count == 0 {
            self.first_tick = tick;
        }
        self.last_tick = tick;
        self.count += 1;
        feed(&mut self.hasher, msg);
    }

    /// Ends the span: its digest, or `None` when it has no message.
    pub fn take(&mut self) -> Option<IntentDigest> {
        if self.count == 0 {
            return None;
        }
        let d = IntentDigest {
            first_tick: self.first_tick,
            last_tick: self.last_tick,
            count: self.count,
            digest: std::mem::take(&mut self.hasher).finalize().into(),
        };
        self.count = 0;
        Some(d)
    }
}

/// The digest of `msgs` as one span.
pub fn span_digest<'a>(msgs: impl IntoIterator<Item = &'a [u8]>) -> Digest {
    let mut h = Sha256::new();
    for m in msgs {
        feed(&mut h, m);
    }
    h.finalize().into()
}

/// A client's journal messages differ from its digests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntentMismatch {
    /// The first span that differs, from 0.
    pub span: usize,
    /// The client's frames of that span.
    pub first_tick: u64,
    pub last_tick: u64,
    /// The journal tick of the span's first message, if the journal has
    /// one there.
    pub journal_tick: Option<u32>,
}

/// What the check covered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntentsChecked {
    pub spans: usize,
    pub messages: usize,
    /// Journal messages after the last span (the client sent no digest
    /// for them yet, for example a lost connection).
    pub uncovered: usize,
}

/// The client that joined with `lease`, if one did.
pub fn client_of_lease(journal: &Journal, lease: u64) -> Option<u16> {
    journal.entries.iter().find_map(|e| match e.record {
        Record::Join {
            client, lease: l, ..
        } if l == lease => Some(client),
        _ => None,
    })
}

/// Checks `client`'s game-lane messages in `journal` against its digests,
/// span by span in order.
pub fn check_intents(
    journal: &Journal,
    client: u16,
    digests: &[IntentDigest],
) -> Result<IntentsChecked, IntentMismatch> {
    let msgs: Vec<(u32, &[u8])> = journal
        .entries
        .iter()
        .filter_map(|e| match &e.record {
            Record::Intent {
                client: c,
                system: false,
                msg,
            } if *c == client => Some((e.tick, msg.as_slice())),
            _ => None,
        })
        .collect();
    let mut at = 0usize;
    for (span, d) in digests.iter().enumerate() {
        let n = d.count as usize;
        let mismatch = IntentMismatch {
            span,
            first_tick: d.first_tick,
            last_tick: d.last_tick,
            journal_tick: msgs.get(at).map(|m| m.0),
        };
        if at + n > msgs.len() {
            return Err(mismatch);
        }
        if span_digest(msgs[at..at + n].iter().map(|m| m.1)) != d.digest {
            return Err(mismatch);
        }
        at += n;
    }
    Ok(IntentsChecked {
        spans: digests.len(),
        messages: at,
        uncovered: msgs.len() - at,
    })
}
