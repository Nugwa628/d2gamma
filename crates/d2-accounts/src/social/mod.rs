//! The realm lobby's social store (lobby contract sections 3-5 and 10):
//! guilds, friends and ignores, privacy, direct messages and mutes, all
//! per account, in the accounts database (migration 2).
//!
//! Each part is a view on the open [`Db`]: [`Db::guilds`], [`Db::friends`],
//! [`Db::dms`], [`Db::prefs`], [`Db::mutes`]. Every call is blocking (run
//! them off the network threads, as [`crate::Accounts`] does) and checks
//! the stored rules: a broken rule is a [`Refusal`], answered with its
//! contract code; rate limits, rooms and presence are the realm's.
//!
//! Times are Unix milliseconds, as on the wire, except `accounts.last_seen`
//! (Unix seconds, like the accounts table's other times).

mod dms;
mod friends;
mod guilds;
mod mutes;
mod prefs;

pub use dms::{
    Conversation, DmLimits, DmRow, DmSent, Dms, CONVERSATIONS_PAGE, HISTORY_ROWS_MAX, LAST_TEXT_MAX,
};
pub use friends::{FriendRow, Friends, FRIENDS_MAX, IGNORE_MAX};
pub use guilds::{
    check_guild_name, check_guild_tag, GuildMember, GuildRow, Guilds, Left, Rank,
    GUILD_MAX_MEMBERS, INVITE_DAYS, MOTD_MAX,
};
pub use mutes::{Mute, Mutes};
pub use prefs::{Prefs, Privacy, ShowWhere, Whispers};

use d2_net::msg::ErrorCode;
use rusqlite::{Connection, OptionalExtension};

use crate::db::{Db, DbError};
use crate::rules::name_key;
use crate::Denied;

/// Migration 2 (lobby contract section 10).
pub(crate) const SCHEMA: &str = "
ALTER TABLE accounts ADD COLUMN last_seen INTEGER;
CREATE TABLE guilds(id INTEGER PRIMARY KEY, tag TEXT NOT NULL, tag_key TEXT NOT NULL UNIQUE,
                    name TEXT NOT NULL, motd TEXT NOT NULL DEFAULT '', created INTEGER NOT NULL);
CREATE TABLE guild_members(account_id INTEGER PRIMARY KEY REFERENCES accounts(id),
                           guild_id INTEGER NOT NULL REFERENCES guilds(id), rank INTEGER NOT NULL,
                           joined INTEGER NOT NULL);
CREATE INDEX guild_members_guild ON guild_members(guild_id);
CREATE TABLE guild_invites(guild_id INTEGER NOT NULL REFERENCES guilds(id),
                           account_id INTEGER NOT NULL REFERENCES accounts(id),
                           by_account INTEGER NOT NULL, at INTEGER NOT NULL, PRIMARY KEY(guild_id, account_id));
CREATE TABLE friends(account_id INTEGER NOT NULL REFERENCES accounts(id),
                     friend_id INTEGER NOT NULL REFERENCES accounts(id), added INTEGER NOT NULL,
                     PRIMARY KEY(account_id, friend_id));
CREATE INDEX friends_back ON friends(friend_id);
CREATE TABLE ignores(account_id INTEGER NOT NULL REFERENCES accounts(id),
                     ignored_id INTEGER NOT NULL REFERENCES accounts(id), added INTEGER NOT NULL,
                     PRIMARY KEY(account_id, ignored_id));
CREATE TABLE social_prefs(account_id INTEGER PRIMARY KEY REFERENCES accounts(id),
                          whispers INTEGER NOT NULL DEFAULT 0, show_where INTEGER NOT NULL DEFAULT 0);
CREATE TABLE dms(id INTEGER PRIMARY KEY, from_id INTEGER NOT NULL, to_id INTEGER NOT NULL,
                 from_char TEXT NOT NULL, to_char TEXT NOT NULL, text TEXT NOT NULL, at INTEGER NOT NULL,
                 delivered INTEGER NOT NULL DEFAULT 0);
CREATE INDEX dms_to ON dms(to_id, id);
CREATE INDEX dms_from ON dms(from_id, id);
CREATE TABLE dm_read(account_id INTEGER NOT NULL, peer_id INTEGER NOT NULL, up_to INTEGER NOT NULL,
                     PRIMARY KEY(account_id, peer_id));
CREATE TABLE mutes(account_id INTEGER PRIMARY KEY, until INTEGER NOT NULL, reason TEXT NOT NULL DEFAULT '');
";

/// A stored rule a request broke (lobby contract section 8). The realm
/// answers it with [`Refusal::code`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("You are not in a guild.")]
    NotInGuild,
    #[error("Your guild rank doesn't allow that.")]
    GuildRank,
    #[error("That guild tag is taken.")]
    GuildTagTaken,
    /// The text says which rule.
    #[error("{0}")]
    BadGuildName(&'static str),
    #[error("You (or they) are in a guild already.")]
    AlreadyInGuild,
    #[error("The guild is full.")]
    GuildFull,
    #[error("No such player.")]
    UnknownPlayer,
    #[error("Their message box is full.")]
    InboxFull,
    #[error("That player doesn't take whispers from you.")]
    WhisperBlocked,
    #[error("Your friends (or ignore) list is full.")]
    FriendLimit,
    #[error("That message isn't allowed.")]
    BadText,
    #[error("That invite expired or was withdrawn.")]
    InviteGone,
}

impl Refusal {
    /// The contract's error code (section 8, block 120-135).
    pub fn code(self) -> ErrorCode {
        ErrorCode(match self {
            Refusal::NotInGuild => 121,
            Refusal::GuildRank => 122,
            Refusal::GuildTagTaken => 123,
            Refusal::BadGuildName(_) => 124,
            Refusal::AlreadyInGuild => 125,
            Refusal::GuildFull => 126,
            Refusal::UnknownPlayer => 127,
            Refusal::InboxFull => 128,
            Refusal::WhisperBlocked => 129,
            Refusal::FriendLimit => 130,
            Refusal::BadText => 134,
            Refusal::InviteGone => 135,
        })
    }
}

impl From<Refusal> for Denied {
    fn from(r: Refusal) -> Self {
        Denied::because(r.code(), &r.to_string())
    }
}

/// A social store call that failed: a broken rule, or the database.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum SocialError {
    #[error(transparent)]
    Refused(#[from] Refusal),
    #[error(transparent)]
    Db(#[from] DbError),
}

impl From<rusqlite::Error> for SocialError {
    fn from(e: rusqlite::Error) -> Self {
        SocialError::Db(e.into())
    }
}

pub type SocialResult<T> = Result<T, SocialError>;

/// An account a request named (lobby contract section 1, "Who").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Peer {
    pub account: i64,
    /// The account's name as stored.
    pub account_name: String,
    /// The character the request named, as stored; `None` for `*account`.
    pub char_name: Option<String>,
}

impl Db {
    /// Resolves `who`: a character name (any case, live characters only) to
    /// its account, or `*name` to the account of that name. `None` if
    /// there is no such player.
    pub fn find_player(&self, who: &str) -> Result<Option<Peer>, DbError> {
        if let Some(account) = who.strip_prefix('*') {
            return Ok(self
                .conn
                .query_row(
                    "SELECT id, name FROM accounts WHERE name_key = ?1",
                    [name_key(account)],
                    |r| {
                        Ok(Peer {
                            account: r.get(0)?,
                            account_name: r.get(1)?,
                            char_name: None,
                        })
                    },
                )
                .optional()?);
        }
        Ok(self.conn
            .query_row(
                "SELECT a.id, a.name, c.name FROM characters c JOIN accounts a ON a.id = c.account_id
                 WHERE c.name_key = ?1 AND c.deleted IS NULL",
                [name_key(who)],
                |r| {
                    Ok(Peer {
                        account: r.get(0)?,
                        account_name: r.get(1)?,
                        char_name: Some(r.get(2)?),
                    })
                },
            )
            .optional()?)
    }

    /// The account's last played live character (the newest created on a
    /// tie), if it has any: what lists show for a player who is offline.
    pub fn last_character(&self, account: i64) -> Result<Option<String>, DbError> {
        Ok(last_character(&self.conn, account)?)
    }
}

pub(crate) fn last_character(conn: &Connection, account: i64) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT name FROM characters WHERE account_id = ?1 AND deleted IS NULL
             ORDER BY last_played DESC, created DESC, rowid DESC LIMIT 1",
        [account],
        |r| r.get(0),
    )
    .optional()
}

/// The stored name of account `id`; [`Refusal::UnknownPlayer`] if none.
pub(crate) fn account_name(conn: &Connection, id: i64) -> SocialResult<String> {
    conn.query_row("SELECT name FROM accounts WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .optional()?
    .ok_or(SocialError::Refused(Refusal::UnknownPlayer))
}

/// Text the store keeps (motd, DM lines): at most `max` bytes, no control
/// characters.
pub(crate) fn check_text(text: &str, max: usize) -> Result<(), Refusal> {
    if text.len() > max || text.chars().any(char::is_control) {
        return Err(Refusal::BadText);
    }
    Ok(())
}

/// A count from SQLite as the wire's `u16`, capped.
pub(crate) fn count16(n: i64) -> u16 {
    u16::try_from(n.max(0)).unwrap_or(u16::MAX)
}
