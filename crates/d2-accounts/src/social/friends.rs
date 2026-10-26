//! Friends and ignores (lobby contract section 5). Friends are one-sided:
//! adding someone needs no answer; two accounts that added each other are
//! mutual friends. An ignored account's room lines and DMs never reach the
//! one who ignores it (the realm applies it).

use rusqlite::{params, Connection, OptionalExtension};

use super::{account_name, last_character, Refusal, SocialResult};
use crate::db::{Db, DbError};

/// Friends per account unless the config says otherwise (`friends_max`).
pub const FRIENDS_MAX: u16 = 100;
/// Ignored accounts per account unless the config says otherwise
/// (`ignore_max`).
pub const IGNORE_MAX: u16 = 200;

/// An account on a friends or ignore list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendRow {
    pub account: i64,
    pub account_name: String,
    /// Their last played character, if any.
    pub last_char: Option<String>,
    /// They added the asker too (friends list only; `false` on the ignore
    /// list).
    pub mutual: bool,
    pub added_ms: i64,
    /// `accounts.last_seen` (Unix seconds), if set.
    pub last_seen: Option<i64>,
}

/// The friends and ignore lists of the database ([`Db::friends`]).
#[derive(Clone, Copy, Debug)]
pub struct Friends<'a> {
    conn: &'a Connection,
}

impl Db {
    pub fn friends(&self) -> Friends<'_> {
        Friends { conn: &self.conn }
    }
}

/// Which list.
#[derive(Clone, Copy)]
enum List {
    Friends,
    Ignores,
}

impl List {
    fn table(self) -> &'static str {
        match self {
            List::Friends => "friends",
            List::Ignores => "ignores",
        }
    }

    fn column(self) -> &'static str {
        match self {
            List::Friends => "friend_id",
            List::Ignores => "ignored_id",
        }
    }
}

impl Friends<'_> {
    fn has(&self, list: List, account: i64, other: i64) -> Result<bool, DbError> {
        let (t, c) = (list.table(), list.column());
        Ok(self
            .conn
            .query_row(
                &format!("SELECT 1 FROM {t} WHERE account_id = ?1 AND {c} = ?2"),
                params![account, other],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    /// Adds `other` to `account`'s list; `Ok(false)` if it was there
    /// already. Refused for oneself, an unknown account and a full list.
    fn add(
        &self,
        list: List,
        account: i64,
        other: i64,
        now_ms: i64,
        max: u16,
    ) -> SocialResult<bool> {
        let (t, c) = (list.table(), list.column());
        let tx = self.conn.unchecked_transaction()?;
        if account == other {
            return Err(Refusal::UnknownPlayer.into());
        }
        account_name(&tx, other)?;
        if self.has(list, account, other)? {
            return Ok(false);
        }
        let n: i64 = tx.query_row(
            &format!("SELECT COUNT(*) FROM {t} WHERE account_id = ?1"),
            [account],
            |r| r.get(0),
        )?;
        if n >= i64::from(max) {
            return Err(Refusal::FriendLimit.into());
        }
        tx.execute(
            &format!("INSERT INTO {t}(account_id, {c}, added) VALUES (?1, ?2, ?3)"),
            params![account, other, now_ms],
        )?;
        tx.commit()?;
        Ok(true)
    }

    fn remove(&self, list: List, account: i64, other: i64) -> Result<bool, DbError> {
        let (t, c) = (list.table(), list.column());
        Ok(self.conn.execute(
            &format!("DELETE FROM {t} WHERE account_id = ?1 AND {c} = ?2"),
            params![account, other],
        )? == 1)
    }

    fn list(&self, list: List, account: i64) -> Result<Vec<FriendRow>, DbError> {
        let (t, c) = (list.table(), list.column());
        let mutual = match list {
            List::Friends => {
                "EXISTS(SELECT 1 FROM friends b WHERE b.account_id = l.friend_id AND b.friend_id = l.account_id)"
            }
            List::Ignores => "0",
        };
        let mut st = self.conn.prepare(&format!(
            "SELECT l.{c}, a.name, {mutual}, l.added, a.last_seen
             FROM {t} l JOIN accounts a ON a.id = l.{c}
             WHERE l.account_id = ?1 ORDER BY a.name_key"
        ))?;
        let rows = st.query_map([account], |r| {
            Ok(FriendRow {
                account: r.get(0)?,
                account_name: r.get(1)?,
                last_char: None,
                mutual: r.get(2)?,
                added_ms: r.get(3)?,
                last_seen: r.get(4)?,
            })
        })?;
        let mut rows: Vec<FriendRow> = rows.collect::<Result<_, _>>()?;
        for f in &mut rows {
            f.last_char = last_character(self.conn, f.account)?;
        }
        Ok(rows)
    }

    fn ids(&self, sql: &str, account: i64) -> Result<Vec<i64>, DbError> {
        let mut st = self.conn.prepare(sql)?;
        let rows = st.query_map([account], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// `account` adds `friend`; `Ok(false)` if it had already. At most
    /// `max` friends ([`Refusal::FriendLimit`]).
    pub fn add_friend(
        &self,
        account: i64,
        friend: i64,
        now_ms: i64,
        max: u16,
    ) -> SocialResult<bool> {
        self.add(List::Friends, account, friend, now_ms, max)
    }

    /// `false` if `friend` wasn't on the list.
    pub fn remove_friend(&self, account: i64, friend: i64) -> Result<bool, DbError> {
        self.remove(List::Friends, account, friend)
    }

    /// `account` ignores `other`; `Ok(false)` if it did already. At most
    /// `max` ignored ([`Refusal::FriendLimit`]).
    pub fn ignore(&self, account: i64, other: i64, now_ms: i64, max: u16) -> SocialResult<bool> {
        self.add(List::Ignores, account, other, now_ms, max)
    }

    /// `false` if `other` wasn't ignored.
    pub fn unignore(&self, account: i64, other: i64) -> Result<bool, DbError> {
        self.remove(List::Ignores, account, other)
    }

    /// `account`'s friends, by account name.
    pub fn list_friends(&self, account: i64) -> Result<Vec<FriendRow>, DbError> {
        self.list(List::Friends, account)
    }

    /// The accounts `account` ignores, by account name.
    pub fn list_ignored(&self, account: i64) -> Result<Vec<FriendRow>, DbError> {
        self.list(List::Ignores, account)
    }

    /// `account` has added `other`.
    pub fn is_friend(&self, account: i64, other: i64) -> Result<bool, DbError> {
        self.has(List::Friends, account, other)
    }

    /// Both have added each other.
    pub fn is_mutual(&self, a: i64, b: i64) -> Result<bool, DbError> {
        Ok(self.is_friend(a, b)? && self.is_friend(b, a)?)
    }

    /// `account` ignores `other`.
    pub fn ignores(&self, account: i64, other: i64) -> Result<bool, DbError> {
        self.has(List::Ignores, account, other)
    }

    /// The accounts that added `account` (who get its friend notices).
    pub fn followers(&self, account: i64) -> Result<Vec<i64>, DbError> {
        self.ids(
            "SELECT account_id FROM friends WHERE friend_id = ?1 ORDER BY account_id",
            account,
        )
    }

    /// The accounts `account` ignores.
    pub fn ignored_ids(&self, account: i64) -> Result<Vec<i64>, DbError> {
        self.ids(
            "SELECT ignored_id FROM ignores WHERE account_id = ?1 ORDER BY ignored_id",
            account,
        )
    }

    /// Sets `accounts.last_seen` (Unix seconds): when the account's last
    /// session ended.
    pub fn set_last_seen(&self, account: i64, unix_s: i64) -> Result<(), DbError> {
        self.conn.execute(
            "UPDATE accounts SET last_seen = ?2 WHERE id = ?1",
            params![account, unix_s],
        )?;
        Ok(())
    }

    /// `accounts.last_seen` (Unix seconds), if set.
    pub fn last_seen(&self, account: i64) -> Result<Option<i64>, DbError> {
        Ok(self
            .conn
            .query_row(
                "SELECT last_seen FROM accounts WHERE id = ?1",
                [account],
                |r| r.get(0),
            )
            .optional()?
            .flatten())
    }
}
