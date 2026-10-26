//! An account's privacy (lobby contract section 3.4): who may whisper it
//! and who sees where it is.

use rusqlite::{params, Connection, OptionalExtension};

use super::{account_name, SocialResult};
use crate::db::{Db, DbError};

/// Who may whisper an account (the wire's `u8`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Whispers {
    /// Everyone (the default).
    #[default]
    Everyone = 0,
    /// The account's friends (those it added) and its guild.
    FriendsAndGuild = 1,
    /// Nobody, except replies in a conversation the account started.
    Nobody = 2,
}

/// Who sees where an account is (the wire's `u8`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShowWhere {
    /// Mutual friends (the default).
    #[default]
    MutualFriends = 0,
    /// Nobody: the account shows as hidden.
    Nobody = 1,
}

impl Whispers {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Whispers::Everyone),
            1 => Some(Whispers::FriendsAndGuild),
            2 => Some(Whispers::Nobody),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

impl ShowWhere {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(ShowWhere::MutualFriends),
            1 => Some(ShowWhere::Nobody),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// An account's privacy choices.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Privacy {
    pub whispers: Whispers,
    pub show_where: ShowWhere,
}

/// The privacy choices of the database ([`Db::prefs`]).
#[derive(Clone, Copy, Debug)]
pub struct Prefs<'a> {
    conn: &'a Connection,
}

impl Db {
    pub fn prefs(&self) -> Prefs<'_> {
        Prefs { conn: &self.conn }
    }
}

pub(crate) fn privacy(conn: &Connection, account: i64) -> Result<Privacy, DbError> {
    let row: Option<(i64, i64)> = conn
        .query_row(
            "SELECT whispers, show_where FROM social_prefs WHERE account_id = ?1",
            [account],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((w, s)) = row else {
        return Ok(Privacy::default());
    };
    let byte = |v: i64| u8::try_from(v).unwrap_or(u8::MAX);
    Ok(Privacy {
        whispers: Whispers::from_u8(byte(w)).unwrap_or_default(),
        show_where: ShowWhere::from_u8(byte(s)).unwrap_or_default(),
    })
}

impl Prefs<'_> {
    /// `account`'s choices; the defaults if it never set any.
    pub fn get(&self, account: i64) -> Result<Privacy, DbError> {
        privacy(self.conn, account)
    }

    pub fn set(&self, account: i64, p: Privacy) -> SocialResult<()> {
        account_name(self.conn, account)?;
        self.conn.execute(
            "INSERT INTO social_prefs(account_id, whispers, show_where) VALUES (?1, ?2, ?3)
             ON CONFLICT(account_id) DO UPDATE SET whispers = ?2, show_where = ?3",
            params![account, p.whispers.as_u8(), p.show_where.as_u8()],
        )?;
        Ok(())
    }
}
