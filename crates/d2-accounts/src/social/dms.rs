//! Direct messages (lobby contract section 3.2): a conversation per
//! account pair, kept in both directions for [`DmLimits::keep_days`], also
//! for a peer who is offline; read marks per account and peer.

use d2_net::msg::lobby::MAX_CHAT;
use rusqlite::{params, Connection, OptionalExtension};

use super::{account_name, check_text, count16, prefs, Refusal, SocialResult, Whispers};
use crate::db::{Db, DbError};

/// Conversations in one page of the list.
pub const CONVERSATIONS_PAGE: u16 = 30;
/// A conversation row's last line is cut to this many bytes.
pub const LAST_TEXT_MAX: usize = 60;
/// The most lines one history page holds.
pub const HISTORY_ROWS_MAX: u8 = 50;

const DAY_MS: i64 = 24 * 3600 * 1000;

/// The DM rules the config sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmLimits {
    /// Undelivered lines one account may have waiting (`dm_waiting_max`).
    pub waiting_max: u16,
    /// Of those, from one sender.
    pub from_one_max: u16,
    /// Days a line is kept (`dm_keep_days`).
    pub keep_days: u16,
}

impl Default for DmLimits {
    fn default() -> Self {
        Self {
            waiting_max: 200,
            from_one_max: 20,
            keep_days: 30,
        }
    }
}

/// A stored line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DmRow {
    pub id: i64,
    pub from: i64,
    pub to: i64,
    /// The character it was sent as.
    pub from_char: String,
    /// The character it was sent to.
    pub to_char: String,
    pub text: String,
    pub at_ms: i64,
    /// It has reached a session of the recipient.
    pub delivered: bool,
}

impl DmRow {
    /// The other account of the line, seen from `account`.
    pub fn peer_of(&self, account: i64) -> i64 {
        if self.from == account {
            self.to
        } else {
            self.from
        }
    }
}

/// What [`Dms::send`] stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmSent {
    pub id: i64,
    /// It waits for a recipient who is offline.
    pub stored: bool,
}

/// A row of the conversation list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub peer: i64,
    pub peer_account: String,
    /// The peer's character in the newest line.
    pub peer_char: String,
    /// Lines from the peer after the read mark.
    pub unread: u16,
    /// The newest line.
    pub last_dm: i64,
    pub last_ms: i64,
    /// The newest line, cut to [`LAST_TEXT_MAX`] bytes.
    pub last_text: String,
}

/// The direct messages of the database ([`Db::dms`]).
#[derive(Clone, Copy, Debug)]
pub struct Dms<'a> {
    conn: &'a Connection,
}

impl Db {
    pub fn dms(&self) -> Dms<'_> {
        Dms { conn: &self.conn }
    }
}

const DM_COLUMNS: &str = "id, from_id, to_id, from_char, to_char, text, at, delivered";

fn dm_of_row(r: &rusqlite::Row) -> rusqlite::Result<DmRow> {
    Ok(DmRow {
        id: r.get(0)?,
        from: r.get(1)?,
        to: r.get(2)?,
        from_char: r.get(3)?,
        to_char: r.get(4)?,
        text: r.get(5)?,
        at_ms: r.get(6)?,
        delivered: r.get(7)?,
    })
}

/// `text` cut to at most `max` bytes on a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn exists(conn: &Connection, sql: &str, p: impl rusqlite::Params) -> Result<bool, DbError> {
    Ok(conn.query_row(sql, p, |_| Ok(())).optional()?.is_some())
}

impl Dms<'_> {
    /// `from` may whisper `to`: `to` doesn't ignore `from`, and `to`'s
    /// privacy lets `from` in (friends and guild, or nobody), a reply in a
    /// conversation `to` wrote in always passing.
    pub fn may_whisper(&self, from: i64, to: i64) -> Result<bool, DbError> {
        let c = self.conn;
        if exists(
            c,
            "SELECT 1 FROM ignores WHERE account_id = ?1 AND ignored_id = ?2",
            params![to, from],
        )? {
            return Ok(false);
        }
        let whispers = prefs::privacy(c, to)?.whispers;
        if whispers == Whispers::Everyone
            || exists(
                c,
                "SELECT 1 FROM dms WHERE from_id = ?1 AND to_id = ?2",
                params![to, from],
            )?
        {
            return Ok(true);
        }
        if whispers == Whispers::Nobody {
            return Ok(false);
        }
        exists(
            c,
            "SELECT 1 FROM friends WHERE account_id = ?1 AND friend_id = ?2
             UNION ALL
             SELECT 1 FROM guild_members a JOIN guild_members b ON a.guild_id = b.guild_id
             WHERE a.account_id = ?1 AND b.account_id = ?2",
            params![to, from],
        )
    }

    /// Stores a line from `from` (as `from_char`) to `to` (as `to_char`).
    /// `delivered` says the realm pushes it to a session of `to` now; a
    /// line for a peer who is offline waits, at most
    /// [`DmLimits::waiting_max`] for one recipient and
    /// [`DmLimits::from_one_max`] of them from one sender
    /// ([`Refusal::InboxFull`]). Refused with [`Refusal::WhisperBlocked`]
    /// when [`Self::may_whisper`] says no.
    #[allow(clippy::too_many_arguments)]
    pub fn send(
        &self,
        from: i64,
        to: i64,
        from_char: &str,
        to_char: &str,
        text: &str,
        now_ms: i64,
        delivered: bool,
        limits: DmLimits,
    ) -> SocialResult<DmSent> {
        if text.is_empty() {
            return Err(Refusal::BadText.into());
        }
        check_text(text, MAX_CHAT)?;
        if from == to {
            return Err(Refusal::UnknownPlayer.into());
        }
        let tx = self.conn.unchecked_transaction()?;
        account_name(&tx, from)?;
        account_name(&tx, to)?;
        if !self.may_whisper(from, to)? {
            return Err(Refusal::WhisperBlocked.into());
        }
        if !delivered {
            let (waiting, from_one): (i64, i64) = tx.query_row(
                "SELECT COUNT(*), COALESCE(SUM(from_id = ?2), 0) FROM dms
                 WHERE to_id = ?1 AND delivered = 0",
                params![to, from],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if waiting >= i64::from(limits.waiting_max)
                || from_one >= i64::from(limits.from_one_max)
            {
                return Err(Refusal::InboxFull.into());
            }
        }
        tx.execute(
            "INSERT INTO dms(from_id, to_id, from_char, to_char, text, at, delivered)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![from, to, from_char, to_char, text, now_ms, delivered],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;
        Ok(DmSent {
            id,
            stored: !delivered,
        })
    }

    /// The lines waiting for `to`, oldest first.
    pub fn undelivered(&self, to: i64) -> Result<Vec<DmRow>, DbError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {DM_COLUMNS} FROM dms WHERE to_id = ?1 AND delivered = 0 ORDER BY id"
        ))?;
        let rows = st.query_map([to], dm_of_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Marks `to`'s waiting lines up to `up_to_dm` delivered. Returns how
    /// many.
    pub fn mark_delivered(&self, to: i64, up_to_dm: i64) -> Result<usize, DbError> {
        Ok(self.conn.execute(
            "UPDATE dms SET delivered = 1 WHERE to_id = ?1 AND delivered = 0 AND id <= ?2",
            params![to, up_to_dm],
        )?)
    }

    /// A page of the conversation of `account` and `peer`: the newest
    /// `rows` (1 to [`HISTORY_ROWS_MAX`]) lines before `before_dm` (0: the
    /// newest), oldest first. The next page asks before this one's first
    /// line.
    pub fn history(
        &self,
        account: i64,
        peer: i64,
        before_dm: i64,
        rows: u8,
    ) -> Result<Vec<DmRow>, DbError> {
        let rows = rows.clamp(1, HISTORY_ROWS_MAX);
        let before = if before_dm <= 0 { i64::MAX } else { before_dm };
        let mut st = self.conn.prepare(&format!(
            "SELECT {DM_COLUMNS} FROM dms
             WHERE ((from_id = ?1 AND to_id = ?2) OR (from_id = ?2 AND to_id = ?1)) AND id < ?3
             ORDER BY id DESC LIMIT ?4"
        ))?;
        let lines = st.query_map(params![account, peer, before, rows], dm_of_row)?;
        let mut lines: Vec<DmRow> = lines.collect::<Result<_, _>>()?;
        lines.reverse();
        Ok(lines)
    }

    /// Page `page` (from 0) of `account`'s conversations, newest first, and
    /// how many there are in all.
    pub fn conversations(
        &self,
        account: i64,
        page: u16,
    ) -> Result<(u32, Vec<Conversation>), DbError> {
        const PEERS: &str = "SELECT to_id AS peer, id FROM dms WHERE from_id = ?1
                             UNION ALL SELECT from_id AS peer, id FROM dms WHERE to_id = ?1";
        let total: i64 = self.conn.query_row(
            &format!("SELECT COUNT(DISTINCT peer) FROM ({PEERS})"),
            [account],
            |r| r.get(0),
        )?;
        let mut st = self.conn.prepare(&format!(
            "SELECT peer, MAX(id) AS last FROM ({PEERS}) GROUP BY peer
             ORDER BY last DESC LIMIT ?2 OFFSET ?3"
        ))?;
        let per = i64::from(CONVERSATIONS_PAGE);
        let heads = st.query_map(params![account, per, i64::from(page) * per], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?;
        let heads: Vec<(i64, i64)> = heads.collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(heads.len());
        for (peer, last) in heads {
            let line = self.conn.query_row(
                &format!("SELECT {DM_COLUMNS} FROM dms WHERE id = ?1"),
                [last],
                dm_of_row,
            )?;
            let peer_account: String = self
                .conn
                .query_row("SELECT name FROM accounts WHERE id = ?1", [peer], |r| {
                    r.get(0)
                })
                .optional()?
                .unwrap_or_default();
            let peer_char = if line.from == peer {
                line.from_char.clone()
            } else {
                line.to_char.clone()
            };
            out.push(Conversation {
                peer,
                peer_account,
                peer_char,
                unread: self.unread_from(account, peer)?,
                last_dm: line.id,
                last_ms: line.at_ms,
                last_text: cut(&line.text, LAST_TEXT_MAX).to_owned(),
            });
        }
        Ok((u32::try_from(total).unwrap_or(u32::MAX), out))
    }

    /// Lines from `peer` to `account` after `account`'s read mark.
    pub fn unread_from(&self, account: i64, peer: i64) -> Result<u16, DbError> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM dms WHERE to_id = ?1 AND from_id = ?2
             AND id > COALESCE((SELECT up_to FROM dm_read WHERE account_id = ?1 AND peer_id = ?2), 0)",
            params![account, peer],
            |r| r.get(0),
        )?;
        Ok(count16(n))
    }

    /// `account`'s unread lines in all and the conversations that have
    /// some (`SocialState.dm_unread`, `dm_conversations_unread`).
    pub fn unread(&self, account: i64) -> Result<(u16, u16), DbError> {
        let (lines, conversations): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COUNT(DISTINCT d.from_id) FROM dms d
             LEFT JOIN dm_read r ON r.account_id = d.to_id AND r.peer_id = d.from_id
             WHERE d.to_id = ?1 AND d.id > COALESCE(r.up_to, 0)",
            [account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((count16(lines), count16(conversations)))
    }

    /// `account` has read its conversation with `peer` up to `up_to_dm`. A
    /// mark never moves back.
    pub fn mark_read(&self, account: i64, peer: i64, up_to_dm: i64) -> Result<(), DbError> {
        self.conn.execute(
            "INSERT INTO dm_read(account_id, peer_id, up_to) VALUES (?1, ?2, ?3)
             ON CONFLICT(account_id, peer_id) DO UPDATE SET up_to = MAX(up_to, ?3)",
            params![account, peer, up_to_dm],
        )?;
        Ok(())
    }

    /// `a` and `b` have a conversation (a line either way).
    pub fn has_conversation(&self, a: i64, b: i64) -> Result<bool, DbError> {
        exists(
            self.conn,
            "SELECT 1 FROM dms WHERE (from_id = ?1 AND to_id = ?2) OR (from_id = ?2 AND to_id = ?1)",
            params![a, b],
        )
    }

    /// Deletes the lines older than `keep_days` at `now_ms`, delivered or
    /// not (the hourly sweep). Returns how many.
    pub fn sweep(&self, now_ms: i64, keep_days: u16) -> Result<usize, DbError> {
        Ok(self.conn.execute(
            "DELETE FROM dms WHERE at <= ?1",
            [now_ms - i64::from(keep_days) * DAY_MS],
        )?)
    }
}
