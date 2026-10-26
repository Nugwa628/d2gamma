//! Guilds v1 (lobby contract section 4): one guild per account, ranks
//! member, officer and leader, invites that last [`INVITE_DAYS`].

use rusqlite::{params, Connection, OptionalExtension};

use super::{account_name, check_text, count16, last_character, Refusal, SocialResult};
use crate::db::{Db, DbError};

/// Members a guild may have unless the config says otherwise
/// (`guild_max_members`).
pub const GUILD_MAX_MEMBERS: u16 = 100;
/// How long an invite lasts.
pub const INVITE_DAYS: i64 = 7;
/// The longest message of the day, in bytes.
pub const MOTD_MAX: usize = 120;

const DAY_MS: i64 = 24 * 3600 * 1000;

/// A member's rank (the wire's `u8`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Member = 0,
    Officer = 1,
    Leader = 2,
}

impl Rank {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Rank::Member),
            1 => Some(Rank::Officer),
            2 => Some(Rank::Leader),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    fn of_sql(v: i64) -> Self {
        u8::try_from(v)
            .ok()
            .and_then(Rank::from_u8)
            .unwrap_or(Rank::Member)
    }
}

/// A guild's public facts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildRow {
    pub id: i64,
    /// As its creator typed it; shown as `<TAG>`.
    pub tag: String,
    pub name: String,
    pub motd: String,
    pub created_ms: i64,
    pub members: u16,
    /// The leader's account.
    pub leader: i64,
    /// The leader's last played character, if any.
    pub leader_char: Option<String>,
}

/// A member of a guild.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuildMember {
    pub account: i64,
    pub account_name: String,
    /// The last played character, for a member who is offline.
    pub last_char: Option<String>,
    pub rank: Rank,
    pub joined_ms: i64,
    /// `accounts.last_seen` (Unix seconds), if set.
    pub last_seen: Option<i64>,
}

/// What leaving did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Left {
    pub guild: i64,
    /// The account was the last member: the guild is gone and its tag free.
    pub disbanded: bool,
}

/// Checks a guild tag: 2 to 4 ASCII letters or digits.
pub fn check_guild_tag(tag: &str) -> Result<(), Refusal> {
    if !(2..=4).contains(&tag.len()) {
        return Err(Refusal::BadGuildName(
            "A guild tag must be 2 to 4 characters long.",
        ));
    }
    if !tag.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return Err(Refusal::BadGuildName(
            "A guild tag may only have letters and digits.",
        ));
    }
    Ok(())
}

/// Checks a guild name: 3 to 24 ASCII letters, digits and single spaces
/// between words.
pub fn check_guild_name(name: &str) -> Result<(), Refusal> {
    if !(3..=24).contains(&name.len()) {
        return Err(Refusal::BadGuildName(
            "A guild name must be 3 to 24 characters long.",
        ));
    }
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b' ') {
        return Err(Refusal::BadGuildName(
            "A guild name may only have letters, digits and spaces.",
        ));
    }
    if name.starts_with(' ') || name.ends_with(' ') || name.contains("  ") {
        return Err(Refusal::BadGuildName(
            "A guild name can't start or end with a space or have two in a row.",
        ));
    }
    Ok(())
}

/// The guilds of the database ([`Db::guilds`]).
#[derive(Clone, Copy, Debug)]
pub struct Guilds<'a> {
    conn: &'a Connection,
}

impl Db {
    pub fn guilds(&self) -> Guilds<'_> {
        Guilds { conn: &self.conn }
    }
}

/// The guild and rank of `account`, if it is in one.
fn membership(conn: &Connection, account: i64) -> Result<Option<(i64, Rank)>, DbError> {
    Ok(conn
        .query_row(
            "SELECT guild_id, rank FROM guild_members WHERE account_id = ?1",
            [account],
            |r| Ok((r.get(0)?, Rank::of_sql(r.get(1)?))),
        )
        .optional()?)
}

fn member_count(conn: &Connection, guild: i64) -> Result<i64, DbError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM guild_members WHERE guild_id = ?1",
        [guild],
        |r| r.get(0),
    )?)
}

fn delete_guild(conn: &Connection, guild: i64) -> Result<(), DbError> {
    conn.execute("DELETE FROM guild_invites WHERE guild_id = ?1", [guild])?;
    conn.execute("DELETE FROM guild_members WHERE guild_id = ?1", [guild])?;
    conn.execute("DELETE FROM guilds WHERE id = ?1", [guild])?;
    Ok(())
}

const GUILD_COLUMNS: &str = "g.id, g.tag, g.name, g.motd, g.created,
    (SELECT COUNT(*) FROM guild_members m WHERE m.guild_id = g.id),
    (SELECT m.account_id FROM guild_members m WHERE m.guild_id = g.id AND m.rank = 2)";

fn guild_of_row(conn: &Connection, r: &rusqlite::Row) -> rusqlite::Result<GuildRow> {
    let leader: Option<i64> = r.get(6)?;
    let leader = leader.unwrap_or(0);
    Ok(GuildRow {
        id: r.get(0)?,
        tag: r.get(1)?,
        name: r.get(2)?,
        motd: r.get(3)?,
        created_ms: r.get(4)?,
        members: count16(r.get(5)?),
        leader,
        leader_char: last_character(conn, leader)?,
    })
}

impl Guilds<'_> {
    fn one(&self, filter: &str, arg: impl rusqlite::ToSql) -> Result<Option<GuildRow>, DbError> {
        let conn = self.conn;
        Ok(conn
            .query_row(
                &format!("SELECT {GUILD_COLUMNS} FROM guilds g WHERE {filter}"),
                [arg],
                |r| guild_of_row(conn, r),
            )
            .optional()?)
    }

    /// The guild and rank of `account`, if it is in one.
    pub fn membership(&self, account: i64) -> Result<Option<(i64, Rank)>, DbError> {
        membership(self.conn, account)
    }

    pub fn get(&self, guild: i64) -> Result<Option<GuildRow>, DbError> {
        self.one("g.id = ?1", guild)
    }

    /// The guild of `account`.
    pub fn of(&self, account: i64) -> Result<Option<GuildRow>, DbError> {
        match self.membership(account)? {
            Some((guild, _)) => self.get(guild),
            None => Ok(None),
        }
    }

    /// The guild with the tag `tag`, any case.
    pub fn by_tag(&self, tag: &str) -> Result<Option<GuildRow>, DbError> {
        self.one("g.tag_key = ?1", tag.to_ascii_lowercase())
    }

    /// The members of `guild`: the leader, the officers, then the members,
    /// each by join time.
    pub fn members(&self, guild: i64) -> Result<Vec<GuildMember>, DbError> {
        let mut st = self.conn.prepare(
            "SELECT m.account_id, a.name, m.rank, m.joined, a.last_seen
             FROM guild_members m JOIN accounts a ON a.id = m.account_id
             WHERE m.guild_id = ?1 ORDER BY m.rank DESC, m.joined, m.account_id",
        )?;
        let rows = st.query_map([guild], |r| {
            Ok(GuildMember {
                account: r.get(0)?,
                account_name: r.get(1)?,
                last_char: None,
                rank: Rank::of_sql(r.get(2)?),
                joined_ms: r.get(3)?,
                last_seen: r.get(4)?,
            })
        })?;
        let mut members: Vec<GuildMember> = rows.collect::<Result<_, _>>()?;
        for m in &mut members {
            m.last_char = last_character(self.conn, m.account)?;
        }
        Ok(members)
    }

    /// The guilds that invited `account` and whose invite still holds at
    /// `now_ms`, newest invite first.
    pub fn invites(&self, account: i64, now_ms: i64) -> Result<Vec<GuildRow>, DbError> {
        let conn = self.conn;
        let mut st = conn.prepare(&format!(
            "SELECT {GUILD_COLUMNS} FROM guild_invites i JOIN guilds g ON g.id = i.guild_id
             WHERE i.account_id = ?1 AND i.at > ?2 ORDER BY i.at DESC, g.id"
        ))?;
        let rows = st.query_map(params![account, now_ms - INVITE_DAYS * DAY_MS], |r| {
            guild_of_row(conn, r)
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Makes a guild with `account` as its leader. Returns its id.
    pub fn create(&self, account: i64, tag: &str, name: &str, now_ms: i64) -> SocialResult<i64> {
        check_guild_tag(tag)?;
        check_guild_name(name)?;
        let tx = self.conn.unchecked_transaction()?;
        account_name(&tx, account)?;
        if membership(&tx, account)?.is_some() {
            return Err(Refusal::AlreadyInGuild.into());
        }
        let r = tx.execute(
            "INSERT INTO guilds(tag, tag_key, name, created) VALUES (?1, ?2, ?3, ?4)",
            params![tag, tag.to_ascii_lowercase(), name, now_ms],
        );
        match r {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                return Err(Refusal::GuildTagTaken.into());
            }
            Err(e) => return Err(e.into()),
        }
        let guild = tx.last_insert_rowid();
        join(&tx, guild, account, Rank::Leader, now_ms)?;
        tx.commit()?;
        Ok(guild)
    }

    /// `by` (an officer or the leader) invites `target` into its guild; a
    /// repeated invite starts its days again. Returns the guild.
    pub fn invite(&self, by: i64, target: i64, now_ms: i64, max_members: u16) -> SocialResult<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let (guild, rank) = membership(&tx, by)?.ok_or(Refusal::NotInGuild)?;
        if rank < Rank::Officer {
            return Err(Refusal::GuildRank.into());
        }
        account_name(&tx, target)?;
        if membership(&tx, target)?.is_some() {
            return Err(Refusal::AlreadyInGuild.into());
        }
        if member_count(&tx, guild)? >= i64::from(max_members) {
            return Err(Refusal::GuildFull.into());
        }
        tx.execute(
            "INSERT INTO guild_invites(guild_id, account_id, by_account, at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(guild_id, account_id) DO UPDATE SET by_account = ?3, at = ?4",
            params![guild, target, by, now_ms],
        )?;
        tx.commit()?;
        Ok(guild)
    }

    /// `account` accepts or declines `guild`'s invite. Joining drops its
    /// other invites.
    pub fn answer(
        &self,
        account: i64,
        guild: i64,
        accept: bool,
        now_ms: i64,
        max_members: u16,
    ) -> SocialResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        let at: Option<i64> = tx
            .query_row(
                "SELECT at FROM guild_invites WHERE guild_id = ?1 AND account_id = ?2",
                params![guild, account],
                |r| r.get(0),
            )
            .optional()?;
        if at.is_none_or(|at| at <= now_ms - INVITE_DAYS * DAY_MS) {
            return Err(Refusal::InviteGone.into());
        }
        if accept {
            if membership(&tx, account)?.is_some() {
                return Err(Refusal::AlreadyInGuild.into());
            }
            if member_count(&tx, guild)? >= i64::from(max_members) {
                return Err(Refusal::GuildFull.into());
            }
            join(&tx, guild, account, Rank::Member, now_ms)?;
        } else {
            tx.execute(
                "DELETE FROM guild_invites WHERE guild_id = ?1 AND account_id = ?2",
                params![guild, account],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// `account` leaves its guild. The leader can't while others remain
    /// ([`Refusal::GuildRank`]: hand over first); the last member leaving
    /// disbands it.
    pub fn leave(&self, account: i64) -> SocialResult<Left> {
        let tx = self.conn.unchecked_transaction()?;
        let (guild, rank) = membership(&tx, account)?.ok_or(Refusal::NotInGuild)?;
        let disbanded = member_count(&tx, guild)? == 1;
        if rank == Rank::Leader && !disbanded {
            return Err(Refusal::GuildRank.into());
        }
        if disbanded {
            delete_guild(&tx, guild)?;
        } else {
            tx.execute("DELETE FROM guild_members WHERE account_id = ?1", [account])?;
        }
        tx.commit()?;
        Ok(Left { guild, disbanded })
    }

    /// The guild and both ranks when `target` is in `by`'s guild and is
    /// not `by`.
    fn pair(&self, conn: &Connection, by: i64, target: i64) -> SocialResult<(i64, Rank, Rank)> {
        let (guild, rank) = membership(conn, by)?.ok_or(Refusal::NotInGuild)?;
        if target == by {
            return Err(Refusal::GuildRank.into());
        }
        match membership(conn, target)? {
            Some((g, t)) if g == guild => Ok((guild, rank, t)),
            _ => Err(Refusal::UnknownPlayer.into()),
        }
    }

    /// `by` removes `target`: an officer only members, the leader anyone.
    /// Returns the guild.
    pub fn kick(&self, by: i64, target: i64) -> SocialResult<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let (guild, rank, theirs) = self.pair(&tx, by, target)?;
        if rank < Rank::Officer || (rank == Rank::Officer && theirs != Rank::Member) {
            return Err(Refusal::GuildRank.into());
        }
        tx.execute("DELETE FROM guild_members WHERE account_id = ?1", [target])?;
        tx.commit()?;
        Ok(guild)
    }

    fn set_rank(&self, by: i64, target: i64, from: Rank, to: Rank) -> SocialResult<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let (guild, rank, theirs) = self.pair(&tx, by, target)?;
        if rank != Rank::Leader || theirs != from {
            return Err(Refusal::GuildRank.into());
        }
        tx.execute(
            "UPDATE guild_members SET rank = ?2 WHERE account_id = ?1",
            params![target, to.as_u8()],
        )?;
        tx.commit()?;
        Ok(guild)
    }

    /// The leader makes member `target` an officer. Returns the guild.
    pub fn promote(&self, by: i64, target: i64) -> SocialResult<i64> {
        self.set_rank(by, target, Rank::Member, Rank::Officer)
    }

    /// The leader makes officer `target` a member. Returns the guild.
    pub fn demote(&self, by: i64, target: i64) -> SocialResult<i64> {
        self.set_rank(by, target, Rank::Officer, Rank::Member)
    }

    /// The leader hands the guild to `target`; the old leader becomes an
    /// officer. Returns the guild.
    pub fn make_leader(&self, by: i64, target: i64) -> SocialResult<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let (guild, rank, _) = self.pair(&tx, by, target)?;
        if rank != Rank::Leader {
            return Err(Refusal::GuildRank.into());
        }
        tx.execute(
            "UPDATE guild_members SET rank = ?2 WHERE account_id = ?1",
            params![by, Rank::Officer.as_u8()],
        )?;
        tx.execute(
            "UPDATE guild_members SET rank = ?2 WHERE account_id = ?1",
            params![target, Rank::Leader.as_u8()],
        )?;
        tx.commit()?;
        Ok(guild)
    }

    /// The leader ends the guild, typing its tag (any case) to confirm. Its
    /// members, invites and tag go at once. Returns the guild and the
    /// accounts that were its members.
    pub fn disband(&self, by: i64, confirm_tag: &str) -> SocialResult<(i64, Vec<i64>)> {
        let tx = self.conn.unchecked_transaction()?;
        let (guild, rank) = membership(&tx, by)?.ok_or(Refusal::NotInGuild)?;
        if rank != Rank::Leader {
            return Err(Refusal::GuildRank.into());
        }
        let tag: String =
            tx.query_row("SELECT tag_key FROM guilds WHERE id = ?1", [guild], |r| {
                r.get(0)
            })?;
        if !confirm_tag.eq_ignore_ascii_case(&tag) {
            return Err(Refusal::BadGuildName("Type the guild's tag to confirm.").into());
        }
        let members = {
            let mut st = tx.prepare(
                "SELECT account_id FROM guild_members WHERE guild_id = ?1 ORDER BY account_id",
            )?;
            let rows = st.query_map([guild], |r| r.get(0))?;
            rows.collect::<Result<Vec<i64>, _>>()?
        };
        delete_guild(&tx, guild)?;
        tx.commit()?;
        Ok((guild, members))
    }

    /// An officer or the leader sets the message of the day (0 to
    /// [`MOTD_MAX`] bytes). Returns the guild.
    pub fn set_motd(&self, by: i64, motd: &str) -> SocialResult<i64> {
        check_text(motd, MOTD_MAX)?;
        let (guild, rank) = membership(self.conn, by)?.ok_or(Refusal::NotInGuild)?;
        if rank < Rank::Officer {
            return Err(Refusal::GuildRank.into());
        }
        self.conn.execute(
            "UPDATE guilds SET motd = ?2 WHERE id = ?1",
            params![guild, motd],
        )?;
        Ok(guild)
    }

    /// Deletes the invites that ran out by `now_ms`. Returns how many.
    pub fn sweep_invites(&self, now_ms: i64) -> Result<usize, DbError> {
        Ok(self.conn.execute(
            "DELETE FROM guild_invites WHERE at <= ?1",
            [now_ms - INVITE_DAYS * DAY_MS],
        )?)
    }
}

/// Adds `account` to `guild` and drops its invites.
fn join(conn: &Connection, guild: i64, account: i64, rank: Rank, now_ms: i64) -> SocialResult<()> {
    conn.execute(
        "INSERT INTO guild_members(account_id, guild_id, rank, joined) VALUES (?1, ?2, ?3, ?4)",
        params![account, guild, rank.as_u8(), now_ms],
    )?;
    conn.execute("DELETE FROM guild_invites WHERE account_id = ?1", [account])?;
    Ok(())
}
