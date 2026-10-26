//! Running an audit: the re-run through the [`ReplayEngine`], what it found,
//! and the rollbacks a failed audit of a player-hosted game proposes.
//!
//! - A divergence of the re-run is a finding at its tick. In an official
//!   game it is a determinism bug (the owner's game servers are honest); in
//!   a player-hosted game it affects every character that was in the game
//!   after the last tick that still matched.
//! - A player's intents that differ from their digests: every character in
//!   the game from that tick on.
//! - A save the main server stored that the re-run didn't make the same:
//!   that character.
//!
//! Findings in official games never roll anything back.

use std::collections::BTreeSet;
use std::path::PathBuf;

use rusqlite::{params, OptionalExtension};

use crate::config::RollbackMode;
use crate::db::{self, AuditId, AuditState, FindingKind, FindingRow, GameId, RollbackRow};
use crate::engine::{JobSeat, ReplayEngine, ReplayJob, ReplayOutcome, SeatSpan};
use crate::rollback::CharacterStore;
use crate::{AuditError, Audits, GameKind};

/// What a run did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunResult {
    pub audit: AuditId,
    pub game: GameId,
    pub state: AuditState,
    pub findings: Vec<FindingRow>,
    pub rollbacks: Vec<RollbackRow>,
    /// The report written.
    pub report: PathBuf,
}

/// A finding before it is stored, with the leases it affects.
struct Found {
    kind: FindingKind,
    tick: Option<u32>,
    last_good: Option<u32>,
    lease: Option<u64>,
    detail: String,
    affects: Vec<u64>,
}

fn present_after(seats: &[SeatSpan], from: u32, to: u32) -> Vec<u64> {
    seats
        .iter()
        .filter(|s| s.joined <= to && s.left.is_none_or(|l| l > from))
        .map(|s| s.lease)
        .collect()
}

fn hex8(d: &[u8; 32]) -> String {
    d[..4].iter().map(|b| format!("{b:02x}")).collect()
}

fn findings(kind: GameKind, o: &ReplayOutcome, stored: &[(u64, u32, u32, [u8; 32])]) -> Vec<Found> {
    let mut out = Vec::new();
    if let Some(d) = &o.divergence {
        out.push(Found {
            kind: match kind {
                GameKind::Official => FindingKind::Determinism,
                GameKind::PlayerHosted => FindingKind::Divergence,
            },
            tick: Some(d.tick),
            last_good: Some(d.last_good),
            lease: None,
            detail: d.what.clone(),
            affects: present_after(&o.seats, d.last_good, d.tick),
        });
    }
    for i in &o.intents {
        let Some(f) = i.fault else { continue };
        let from = f.tick.unwrap_or(0);
        out.push(Found {
            kind: FindingKind::Intents,
            tick: f.tick,
            last_good: None,
            lease: Some(i.lease),
            detail: format!("digest span {} differs from the journal's intents", f.span),
            affects: present_after(&o.seats, from.saturating_sub(1), o.last_tick.max(from)),
        });
    }
    for &(lease, seq, tick, digest) in stored {
        let remade = o.saves.iter().find(|s| s.lease == lease && s.seq == seq);
        let detail = match remade {
            Some(s) if s.digest == digest => continue,
            Some(s) => format!(
                "stored save {seq} is {}, the re-run made {}",
                hex8(&digest),
                hex8(&s.digest)
            ),
            // A save after the divergence was never reached: the divergence
            // covers it.
            None if o.divergence.as_ref().is_some_and(|d| tick >= d.last_good) => continue,
            None => format!(
                "stored save {seq} ({}) was not made by the re-run",
                hex8(&digest)
            ),
        };
        out.push(Found {
            kind: FindingKind::StoredSave,
            tick: Some(tick),
            last_good: None,
            lease: Some(lease),
            detail,
            affects: vec![lease],
        });
    }
    out
}

fn summary(o: &ReplayOutcome) -> String {
    format!(
        "{} records, {} frames, last tick {}, {} state hashes equal, {} saves equal{}",
        o.records,
        o.frames,
        o.last_tick,
        o.state_hashes,
        o.saves.len(),
        if o.ended {
            ""
        } else {
            "; the journal has no End record (the game server was lost)"
        }
    )
}

impl Audits {
    /// Runs the next audit whose game has ended: asked-for audits (on
    /// demand, reports, save checks) before the standing checks, oldest
    /// first. `None` when nothing is waiting.
    pub fn run_next(
        &self,
        engine: &dyn ReplayEngine,
        store: &dyn CharacterStore,
        now: u64,
    ) -> Result<Option<RunResult>, AuditError> {
        let next: Option<AuditId> = self
            .conn()
            .query_row(
                "SELECT a.id FROM audits a JOIN games g ON g.id = a.game \
                 WHERE a.state = 0 AND g.ended IS NOT NULL ORDER BY a.priority, a.id LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;
        next.map(|id| self.run(id, engine, store, now)).transpose()
    }

    /// Runs one queued audit.
    pub fn run(
        &self,
        audit: AuditId,
        engine: &dyn ReplayEngine,
        store: &dyn CharacterStore,
        now: u64,
    ) -> Result<RunResult, AuditError> {
        let (game, seats) = {
            let c = self.conn();
            let a = db::audit_row(&c, audit)?;
            if a.state != AuditState::Queued {
                return Err(AuditError::State(format!(
                    "audit {audit} is {}, not queued",
                    a.state.name()
                )));
            }
            let game = db::get_game(&c, a.game)?;
            if game.ended.is_none() {
                return Err(AuditError::State(format!(
                    "game {} is still running; its audit waits for its end",
                    game.id
                )));
            }
            c.execute("UPDATE audits SET state = 1 WHERE id = ?1", [audit])?;
            (game, db::seats_of(&c, a.game)?)
        };
        let job_seats: Result<Vec<JobSeat>, String> = seats
            .iter()
            .map(|s| {
                s.joined
                    .clone()
                    .map(|joined| JobSeat {
                        lease: s.lease,
                        character: s.character.clone(),
                        joined,
                    })
                    .ok_or_else(|| format!("{}'s join record is gone", s.character))
            })
            .collect();
        // The re-run takes long: the database stays free meanwhile.
        let outcome = job_seats.and_then(|seats| {
            engine.replay(&ReplayJob {
                game: game.id,
                journal: &game.journal,
                seats: &seats,
            })
        });
        let c = self.conn();
        let state = match &outcome {
            Err(e) => {
                c.execute(
                    "UPDATE audits SET state = 4, finished = ?2, detail = ?3 WHERE id = ?1",
                    params![audit, now as i64, e],
                )?;
                c.execute("UPDATE games SET held = 1 WHERE id = ?1", [game.id])?;
                AuditState::Error
            }
            Ok(o) => {
                let stored = db::stored_saves(&c, game.id)?;
                let found = findings(game.kind, o, &stored);
                let state = if found.is_empty() {
                    AuditState::Passed
                } else {
                    AuditState::Failed
                };
                let tx = c.unchecked_transaction()?;
                let mut affected = BTreeSet::new();
                for f in &found {
                    tx.execute(
                        "INSERT INTO findings(audit, game, kind, tick, last_good, lease, detail) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![
                            audit,
                            game.id,
                            f.kind as i64,
                            f.tick,
                            f.last_good,
                            f.lease.map(|l| l as i64),
                            f.detail
                        ],
                    )?;
                    affected.extend(f.affects.iter().copied());
                }
                if game.kind == GameKind::PlayerHosted {
                    for s in seats.iter().filter(|s| affected.contains(&s.lease)) {
                        tx.execute(
                            "INSERT INTO rollbacks(audit, game, lease, character, state, created) \
                             VALUES (?1, ?2, ?3, ?4, 0, ?5)",
                            params![audit, game.id, s.lease as i64, s.character, now as i64],
                        )?;
                    }
                }
                tx.execute(
                    "UPDATE audits SET state = ?2, finished = ?3, detail = ?4 WHERE id = ?1",
                    params![audit, state as i64, now as i64, summary(o)],
                )?;
                if state == AuditState::Failed {
                    tx.execute("UPDATE games SET held = 1 WHERE id = ?1", [game.id])?;
                }
                tx.commit()?;
                state
            }
        };
        drop(c);
        if state == AuditState::Failed && self.cfg.rollback == RollbackMode::Automatic {
            for r in self.rollbacks_of(audit)? {
                // A refused one stays proposed for the owner; the report
                // says so.
                let _ = self.apply_rollback(r.id, store, now);
            }
        }
        let report = self.write_report(audit)?;
        Ok(RunResult {
            audit,
            game: game.id,
            state,
            findings: self.findings_of(audit)?,
            rollbacks: self.rollbacks_of(audit)?,
            report,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(lease: u64, joined: u32, left: Option<u32>) -> SeatSpan {
        SeatSpan {
            lease,
            joined,
            left,
        }
    }

    #[test]
    fn affected_are_the_seats_after_the_last_good_tick() {
        let seats = [
            span(1, 0, None),
            span(2, 0, Some(500)), // left before the last good tick
            span(3, 0, Some(700)), // left after it
            span(4, 2000, None),   // joined after the divergence
            span(5, 900, Some(950)),
        ];
        assert_eq!(present_after(&seats, 600, 1200), vec![1, 3, 5]);
    }
}
