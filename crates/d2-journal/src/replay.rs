//! The replay audit engine (N17, plan section C4): re-runs a game from its
//! journal and compares what it gets with what the journal says the game
//! server got.
//!
//! [`replay`] feeds the journal's records to a [`ReplayGame`] (the game
//! built again from the journal's start, [`crate::server::ServerReplay`]
//! for D2RUST's own game) in order: joins and leaves, every message through
//! the host's senders at the recorded clock, every frame at its recorded
//! clock. At each record it checks the game is at the record's tick; at
//! each [`Record::StateHash`] it compares the game's state hash, at each
//! [`Record::Save`] the save the game makes. The stash and market transfers
//! ([`Record::ItemOut`], [`Record::ItemIn`]) are inputs from outside the
//! game: the game server applies them between host frames and saves the
//! character at once, so the re-run applies them at their record, between
//! the same frames, and checks the save that follows. The first difference
//! ends the run and is the report's [`Divergence`]: the record, its tick,
//! and the last tick where the state still matched, so the change lies
//! after it.

use std::collections::{BTreeMap, VecDeque};

use crate::chunk::Journal;
use crate::format::{kind, Digest, Goods, Record};

/// A client id of the game being re-run.
pub type Client = u32;

/// The game a journal is re-run on.
pub trait ReplayGame {
    /// A client joins (the host connects it), playing the seat of `lease`
    /// with the character record of digest `record`.
    fn join(&mut self, client: Client, lease: u64, record: &Digest) -> Result<(), String>;
    /// A client leaves (the host disconnects it).
    fn leave(&mut self, client: Client);
    /// A C→S message through the host's system or game sender; the game
    /// sender's duplicate filter reads `now`.
    fn send(&mut self, client: Client, system: bool, now: u32, msg: &[u8]);
    /// One host frame at the clock reading `now`.
    fn frame(&mut self, now: u32) -> Result<(), String>;
    /// The game frame now.
    fn frame_no(&self) -> u32;
    /// The state hash now (the same function the game server hashes with).
    fn state_hash(&mut self) -> Digest;
    /// Saves every character, as the game server's periodic save does.
    fn save_all(&mut self);
    /// The saves the game made since the last call, in order, as (client,
    /// record digest).
    fn take_saves(&mut self) -> Vec<(Client, Digest)>;
    /// A stash or market transfer takes goods out of `client`'s character
    /// ([`Record::ItemOut`]); the game makes the character's save at once
    /// (the journal's next [`Record::Save`] of it). `Err` when the goods
    /// aren't there to take.
    fn item_out(&mut self, client: Client, out: &TransferOut<'_>) -> Result<(), String>;
    /// A stash or market transfer brings goods to `client`'s character
    /// ([`Record::ItemIn`]) and returns the result: 0 placed (the game
    /// makes the character's save at once), 1 no room or 2 refused
    /// (nothing changes and nothing is saved). `Err` when the game can't
    /// take such goods at all.
    fn item_in(&mut self, client: Client, item_in: &TransferIn<'_>) -> Result<u8, String>;
}

/// What a [`Record::ItemOut`] takes out of a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferOut<'a> {
    /// The game server's request number.
    pub req: u64,
    /// The item's unit id in the game (0 when only gold moves).
    pub unit: u32,
    /// The item's realm item ID.
    pub item_id: Option<u32>,
    /// A stackable: its code and the count taken.
    pub stack: Option<(&'a str, u32)>,
    /// Gold taken.
    pub gold: u64,
}

/// What a [`Record::ItemIn`] brings to a character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferIn<'a> {
    /// The main server's request number.
    pub req: u64,
    pub goods: &'a Goods,
    /// Gold brought.
    pub gold: u64,
    /// The item goes on the cursor (else the inventory).
    pub cursor: bool,
}

/// What differed first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mismatch {
    /// The record was written at another game frame than the re-run is at.
    Tick { want: u32, got: u32 },
    /// The game's state differs.
    StateHash { want: Digest, got: Digest },
    /// The save the game made differs from the journal's.
    Save {
        lease: u64,
        seq: u32,
        want: Digest,
        got: Digest,
    },
    /// The journal has a save the re-run game didn't make.
    SaveMissing { lease: u64, seq: u32 },
    /// The re-run game saved a character the journal has no save for.
    SaveExtra { client: Client },
    /// A save names a lease no client joined with.
    UnknownLease { lease: u64 },
    /// A join could not be re-run (the record is unknown or doesn't load).
    Join { client: Client, why: String },
    /// A frame failed.
    Frame(String),
    /// A transfer could not be re-run: what an `ItemOut` takes is not on
    /// the character, or the game can't take an `ItemIn`'s goods.
    Transfer { kind: u8, why: String },
    /// A transfer that brings goods ended otherwise in the re-run (the
    /// `result` of [`Record::ItemIn`]).
    TransferResult { want: u8, got: u8 },
}

/// Where a re-run first differed from its journal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Divergence {
    /// The record's index in the journal.
    pub record: usize,
    /// The record's tick: the game differs at or before it.
    pub tick: u32,
    /// The last tick where the state hash still matched (the start's tick
    /// when none did): the game first differs after it.
    pub last_good: u32,
    pub what: Mismatch,
}

/// A save that matched, for the main server to compare with the record it
/// stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveMatched {
    pub tick: u32,
    pub lease: u64,
    pub seq: u32,
    pub record: Digest,
}

/// What a re-run found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayReport {
    /// Records run.
    pub records: usize,
    /// Host frames run.
    pub frames: u64,
    /// The game frame at the end.
    pub last_tick: u32,
    /// State hashes compared and equal.
    pub state_hashes: u32,
    /// Saves compared and equal, in order.
    pub saves: Vec<SaveMatched>,
    /// Transfers re-run (an `ItemIn` with the journal's result).
    pub transfers: u32,
    /// The journal ended with its End record.
    pub ended: bool,
    /// The first difference, if any.
    pub divergence: Option<Divergence>,
}

impl ReplayReport {
    /// The re-run matched everywhere.
    pub fn matched(&self) -> bool {
        self.divergence.is_none()
    }

    /// Checks a record the main server stored against the re-run: `Ok` when
    /// the save of `lease` and `seq` was re-made with this digest.
    pub fn check_stored(&self, lease: u64, seq: u32, record: &Digest) -> Result<(), String> {
        match self.saves.iter().find(|s| s.lease == lease && s.seq == seq) {
            Some(s) if s.record == *record => Ok(()),
            Some(s) => Err(format!(
                "the stored save {seq} of lease {lease} differs from the game's (tick {})",
                s.tick
            )),
            None => Err(format!("the re-run made no save {seq} of lease {lease}")),
        }
    }
}

/// Re-runs `journal` on `game` (built from `journal.start()`).
pub fn replay(journal: &Journal, game: &mut impl ReplayGame) -> ReplayReport {
    let mut report = ReplayReport::default();
    let mut clients: BTreeMap<u64, Client> = BTreeMap::new();
    let mut pending: VecDeque<(Client, Digest)> = VecDeque::new();
    let mut sender_now = 0u32;
    let mut last_good = journal.entries.first().map_or(0, |e| e.tick);
    for (i, e) in journal.entries.iter().enumerate() {
        let good = last_good;
        let fail = |what| Divergence {
            record: i,
            tick: e.tick,
            last_good: good,
            what,
        };
        let at = game.frame_no();
        if e.tick != at {
            report.divergence = Some(fail(Mismatch::Tick {
                want: e.tick,
                got: at,
            }));
            break;
        }
        report.records = i + 1;
        match &e.record {
            // The credits only say what the item taken out just before
            // became for the stash; the game does nothing for them.
            Record::Start(_) | Record::ItemIds { .. } | Record::ItemCredits { .. } => {}
            Record::Join {
                client,
                lease,
                record,
            } => {
                let c = Client::from(*client);
                if let Err(why) = game.join(c, *lease, record) {
                    report.divergence = Some(fail(Mismatch::Join { client: c, why }));
                    break;
                }
                clients.insert(*lease, c);
            }
            Record::Leave { client, .. } => game.leave(Client::from(*client)),
            Record::Clock { host_ms } => sender_now = *host_ms,
            Record::Intent {
                client,
                system,
                msg,
            } => game.send(Client::from(*client), *system, sender_now, msg),
            Record::Frame { host_ms } => {
                if let Err(why) = game.frame(*host_ms) {
                    report.divergence = Some(fail(Mismatch::Frame(why)));
                    break;
                }
                report.frames += 1;
            }
            Record::StateHash(want) => {
                let got = game.state_hash();
                if got != *want {
                    report.divergence = Some(fail(Mismatch::StateHash { want: *want, got }));
                    break;
                }
                report.state_hashes += 1;
                last_good = e.tick;
            }
            Record::Save { lease, seq, record } => {
                let Some(&want_client) = clients.get(lease) else {
                    report.divergence = Some(fail(Mismatch::UnknownLease { lease: *lease }));
                    break;
                };
                pending.extend(game.take_saves());
                if pending.is_empty() {
                    // Not a save the game made itself (a leave, its own
                    // periodic pass): the game server's periodic save.
                    game.save_all();
                    pending.extend(game.take_saves());
                }
                match pending.pop_front() {
                    Some((c, got)) if c == want_client && got == *record => {
                        report.saves.push(SaveMatched {
                            tick: e.tick,
                            lease: *lease,
                            seq: *seq,
                            record: got,
                        });
                    }
                    Some((c, got)) => {
                        report.divergence = Some(fail(if c == want_client {
                            Mismatch::Save {
                                lease: *lease,
                                seq: *seq,
                                want: *record,
                                got,
                            }
                        } else {
                            Mismatch::SaveMissing {
                                lease: *lease,
                                seq: *seq,
                            }
                        }));
                        break;
                    }
                    None => {
                        report.divergence = Some(fail(Mismatch::SaveMissing {
                            lease: *lease,
                            seq: *seq,
                        }));
                        break;
                    }
                }
            }
            Record::End { .. } => {
                pending.extend(game.take_saves());
                if let Some((client, _)) = pending.pop_front() {
                    report.divergence = Some(fail(Mismatch::SaveExtra { client }));
                    break;
                }
                report.ended = true;
            }
            Record::ItemOut {
                client,
                req,
                unit,
                item_id,
                stack,
                gold,
            } => {
                let out = TransferOut {
                    req: *req,
                    unit: *unit,
                    item_id: *item_id,
                    stack: stack.as_ref().map(|(code, n)| (code.as_str(), *n)),
                    gold: *gold,
                };
                if let Err(why) = game.item_out(Client::from(*client), &out) {
                    report.divergence = Some(fail(Mismatch::Transfer {
                        kind: kind::ITEM_OUT,
                        why,
                    }));
                    break;
                }
                report.transfers += 1;
            }
            Record::ItemIn {
                client,
                req,
                goods,
                gold,
                cursor,
                result,
            } => {
                let item_in = TransferIn {
                    req: *req,
                    goods,
                    gold: *gold,
                    cursor: *cursor,
                };
                let got = match game.item_in(Client::from(*client), &item_in) {
                    Ok(got) => got,
                    Err(why) => {
                        report.divergence = Some(fail(Mismatch::Transfer {
                            kind: kind::ITEM_IN,
                            why,
                        }));
                        break;
                    }
                };
                if got != *result {
                    report.divergence = Some(fail(Mismatch::TransferResult { want: *result, got }));
                    break;
                }
                report.transfers += 1;
            }
        }
    }
    report.last_tick = game.frame_no();
    report
}
