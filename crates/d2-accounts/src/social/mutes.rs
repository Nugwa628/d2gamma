//! Admin mutes (lobby contract section 9): a muted account can't say or
//! whisper until its mute ends; it still reads everything.

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::{Db, DbError};

/// A mute in force.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mute {
    /// Unix milliseconds when it ends.
    pub until_ms: i64,
    pub reason: String,
}

/// The mutes of the database ([`Db::mutes`]).
#[derive(Clone, Copy, Debug)]
pub struct Mutes<'a> {
    conn: &'a Connection,
}

impl Db {
    pub fn mutes(&self) -> Mutes<'_> {
        Mutes { conn: &self.conn }
    }
}

impl Mutes<'_> {
    /// Mutes `account` until `until_ms`, replacing a mute it had.
    pub fn mute(&self, account: i64, until_ms: i64, reason: &str) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO mutes(account_id, until, reason) VALUES (?1, ?2, ?3)
             ON CONFLICT(account_id) DO UPDATE SET until = ?2, reason = ?3",
            params![account, until_ms, reason],
        )?;
        Ok(())
    }

    /// `false` if `account` wasn't muted.
    pub fn unmute(&self, account: i64) -> Result<bool, DbError> {
        Ok(self
            .conn
            .execute("DELETE FROM mutes WHERE account_id = ?1", [account])?
            == 1)
    }

    /// `account`'s mute if it still holds at `now_ms`.
    pub fn get(&self, account: i64, now_ms: i64) -> Result<Option<Mute>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT until, reason FROM mutes WHERE account_id = ?1 AND until > ?2",
                params![account, now_ms],
                |r| {
                    Ok(Mute {
                        until_ms: r.get(0)?,
                        reason: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }

    /// Deletes the mutes that ended by `now_ms`. Returns how many.
    pub fn sweep(&self, now_ms: i64) -> Result<usize, DbError> {
        Ok(self
            .conn
            .execute("DELETE FROM mutes WHERE until <= ?1", [now_ms])?)
    }
}
