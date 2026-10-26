//! Rollback (plan C5): a failed audit of a player-hosted game puts each
//! affected character back to the record it joined that game with. Its
//! progress since (that game and any later one) is gone with it. Every
//! rollback keeps the record it replaced, so the owner can undo it.
//!
//! Open question 15 is a config value ([`crate::RollbackMode`]): the
//! rollbacks wait for the owner (default) or apply at once.

use rusqlite::params;
use sha2::{Digest as _, Sha256};

use crate::db::{self, RollbackId, RollbackRow, RollbackState};
use crate::engine::Joined;
use crate::{AuditError, AuditId, Audits};

/// Where the characters are: the main server's character database. A
/// rollback reads a character's record and writes another.
pub trait CharacterStore {
    /// The character's stored record.
    fn load(&self, character: &str) -> Result<Vec<u8>, String>;
    /// Replaces the character's record. Refuses while the character is in
    /// a game (its next save would undo the rollback).
    fn store(&self, character: &str, record: &[u8]) -> Result<(), String>;
    /// The record of a character that has never played (a rollback of a
    /// character whose first game failed its audit).
    fn fresh(
        &self,
        character: &str,
        class: u8,
        hardcore: bool,
        progression: u8,
    ) -> Result<Vec<u8>, String> {
        let _ = (class, hardcore, progression);
        Err(format!(
            "{character} joined as a new character; this store can't make a new character's record"
        ))
    }
}

fn digest(b: &[u8]) -> Vec<u8> {
    Sha256::digest(b).to_vec()
}

impl Audits {
    fn rollback_row(&self, id: RollbackId) -> Result<RollbackRow, AuditError> {
        db::rollbacks_where(&self.conn(), "WHERE id = ?1", id)?
            .pop()
            .ok_or_else(|| AuditError::NotFound(format!("rollback {id}")))
    }

    fn expect(r: &RollbackRow, want: RollbackState) -> Result<(), AuditError> {
        if r.state == want {
            Ok(())
        } else {
            Err(AuditError::State(format!(
                "rollback {} is {}, not {}",
                r.id,
                r.state.name(),
                want.name()
            )))
        }
    }

    /// Applies a proposed rollback: the character gets its join record of
    /// the failed game back.
    pub fn apply_rollback(
        &self,
        id: RollbackId,
        store: &dyn CharacterStore,
        now: u64,
    ) -> Result<RollbackRow, AuditError> {
        let r = self.rollback_row(id)?;
        Self::expect(&r, RollbackState::Proposed)?;
        let joined = db::seats_of(&self.conn(), r.game)?
            .into_iter()
            .find(|s| s.lease == r.lease)
            .and_then(|s| s.joined)
            .ok_or_else(|| {
                AuditError::State(format!(
                    "{}'s record of game {} is gone",
                    r.character, r.game
                ))
            })?;
        let before = store.load(&r.character).map_err(AuditError::Store)?;
        let to = match joined {
            Joined::Record(b) => b,
            Joined::New {
                class,
                hardcore,
                progression,
            } => store
                .fresh(&r.character, class, hardcore, progression)
                .map_err(AuditError::Store)?,
        };
        store.store(&r.character, &to).map_err(AuditError::Store)?;
        self.conn().execute(
            "UPDATE rollbacks SET state = 1, before = ?2, after_digest = ?3, applied = ?4 WHERE id = ?1",
            params![id, before, digest(&to), now as i64],
        )?;
        self.write_report(r.audit)?;
        self.rollback_row(id)
    }

    /// Applies every proposed rollback of an audit (the owner confirms it).
    pub fn confirm_audit(
        &self,
        audit: AuditId,
        store: &dyn CharacterStore,
        now: u64,
    ) -> Result<Vec<RollbackRow>, AuditError> {
        let mut done = Vec::new();
        for r in self.rollbacks_of(audit)? {
            if r.state == RollbackState::Proposed {
                done.push(self.apply_rollback(r.id, store, now)?);
            }
        }
        Ok(done)
    }

    /// Declines a proposed rollback: the character keeps its record.
    pub fn decline_rollback(&self, id: RollbackId, now: u64) -> Result<RollbackRow, AuditError> {
        let r = self.rollback_row(id)?;
        Self::expect(&r, RollbackState::Proposed)?;
        self.conn().execute(
            "UPDATE rollbacks SET state = 3, undone = ?2 WHERE id = ?1",
            params![id, now as i64],
        )?;
        self.write_report(r.audit)?;
        self.rollback_row(id)
    }

    /// Undoes an applied rollback: the character gets back the record the
    /// rollback replaced. Refused when the character changed since (it
    /// played again), unless `force`.
    pub fn undo_rollback(
        &self,
        id: RollbackId,
        store: &dyn CharacterStore,
        force: bool,
        now: u64,
    ) -> Result<RollbackRow, AuditError> {
        let r = self.rollback_row(id)?;
        Self::expect(&r, RollbackState::Applied)?;
        let (before, after): (Vec<u8>, Vec<u8>) = self.conn().query_row(
            "SELECT before, after_digest FROM rollbacks WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let now_record = store.load(&r.character).map_err(AuditError::Store)?;
        if digest(&now_record) != after && !force {
            return Err(AuditError::State(format!(
                "{} changed since rollback {id} (played again?); undo with force to replace that too",
                r.character
            )));
        }
        store
            .store(&r.character, &before)
            .map_err(AuditError::Store)?;
        self.conn().execute(
            "UPDATE rollbacks SET state = 2, undone = ?2 WHERE id = ?1",
            params![id, now as i64],
        )?;
        self.write_report(r.audit)?;
        self.rollback_row(id)
    }
}
