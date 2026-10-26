//! Guilds (kinds 0xF0-0xF7) and friends, ignores and privacy (0xF8-0xFF)
//! on the control stream (`docs/plans/server/lobby-contract.md` sections 4
//! and 5).
//!
//! The framing is the social messages' ([`super::social`]): every C→S
//! message is a [`ToRealm`] with a request number; a request that isn't a
//! list is answered with `social::Done`, a list with `social::ListHead`,
//! its rows (this module's [`FromRealm`]) and `social::FromRealm::ListEnd`.
//! Guilds, friends and ignores are per account.

use super::lobby::MAX_FROM;
use super::social::{read_place, Who, MAX_GUILD_NAME, MAX_TAG, MAX_TARGET};
use super::{Reader, Writer};
use crate::NetError;

/// The guild and friends kinds.
pub mod kind {
    pub const GUILD_GET: u8 = 0xF0;
    pub const GUILD_INFO: u8 = 0xF1;
    pub const GUILD_MEMBER: u8 = 0xF2;
    pub const GUILD_CREATE: u8 = 0xF3;
    pub const GUILD_INVITE: u8 = 0xF4;
    pub const GUILD_NOTICE: u8 = 0xF5;
    pub const GUILD_ANSWER: u8 = 0xF6;
    pub const GUILD_MANAGE: u8 = 0xF7;
    pub const FRIENDS_GET: u8 = 0xF8;
    pub const FRIEND_ROW: u8 = 0xF9;
    pub const FRIEND_EDIT: u8 = 0xFA;
    pub const FRIEND_NOTICE: u8 = 0xFB;
}

/// A guild's message of the day.
pub const MAX_MOTD: usize = 120;
/// A tag a request names: wider than the rule (2-4), so the realm answers
/// `BadGuildName` instead of closing the connection.
pub const MAX_TAG_WIRE: usize = 16;
/// A guild name a request names: wider than the rule (3-24).
pub const MAX_GUILD_NAME_WIRE: usize = 48;
/// A game name in a [`FriendGame`].
pub const MAX_GAME_NAME: usize = super::lobby::MAX_GAME_NAME;

/// Guild ranks (`u8`).
pub mod rank {
    pub const MEMBER: u8 = 0;
    pub const OFFICER: u8 = 1;
    pub const LEADER: u8 = 2;
    /// In [`super::GuildInfo::my_rank`]: the asker isn't a member.
    pub const NONE: u8 = 255;
}

/// What a [`Ask::GuildGet`] asks (`u8`).
pub mod guild_get {
    /// The asker's guild, with its members.
    pub const MINE: u8 = 0;
    /// The invites to the asker.
    pub const INVITES: u8 = 1;
    /// A guild by tag: public facts only.
    pub const BY_TAG: u8 = 2;
}

/// The ops of [`Ask::GuildManage`] (`u8`).
pub mod guild_op {
    pub const LEAVE: u8 = 0;
    pub const KICK: u8 = 1;
    pub const PROMOTE: u8 = 2;
    pub const DEMOTE: u8 = 3;
    pub const MAKE_LEADER: u8 = 4;
    /// `arg` must be the tag, as a confirm.
    pub const DISBAND: u8 = 5;
    pub const MOTD: u8 = 6;
}

/// What a [`GuildNotice`] says (`u8`).
pub mod guild_notice {
    pub const INVITED_YOU: u8 = 1;
    pub const JOINED: u8 = 2;
    pub const LEFT: u8 = 3;
    pub const KICKED: u8 = 4;
    pub const PROMOTED: u8 = 5;
    pub const DEMOTED: u8 = 6;
    pub const LEADER_CHANGED: u8 = 7;
    pub const DISBANDED: u8 = 8;
    pub const MOTD: u8 = 9;
}

/// What a [`Ask::FriendsGet`] lists (`u8`).
pub mod friends_get {
    pub const FRIENDS: u8 = 0;
    pub const IGNORED: u8 = 1;
}

/// What a [`FriendNotice`] says (`u8`).
pub mod friend_notice {
    pub const ONLINE: u8 = 1;
    pub const OFFLINE: u8 = 2;
    pub const ENTERED_GAME: u8 = 3;
    pub const LEFT_GAME: u8 = 4;
    pub const ADDED_YOU: u8 = 5;
}

/// A guild as [`Ask::GuildGet`] answers it (S→C 0xF1, after its `req`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildInfo {
    pub guild: u64,
    pub tag: String,
    pub name: String,
    pub motd: String,
    pub created_ms: u64,
    pub members: u16,
    pub leader_char: String,
    /// [`rank`], [`rank::NONE`] not a member.
    pub my_rank: u8,
}

impl GuildInfo {
    fn encode(&self, w: &mut Writer) {
        w.u64(self.guild)
            .str8(&self.tag)
            .str8(&self.name)
            .str8(&self.motd)
            .u64(self.created_ms)
            .u16(self.members)
            .str8(&self.leader_char)
            .u8(self.my_rank);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let g = Self {
            guild: r.u64()?,
            tag: r.str8(MAX_TAG)?,
            name: r.str8(MAX_GUILD_NAME)?,
            motd: r.str8(MAX_MOTD)?,
            created_ms: r.u64()?,
            members: r.u16()?,
            leader_char: r.str8(MAX_FROM)?,
            my_rank: r.u8()?,
        };
        if g.my_rank > rank::LEADER && g.my_rank != rank::NONE {
            return Err(NetError::Malformed);
        }
        Ok(g)
    }
}

/// A member row of the asker's guild (S→C 0xF2, after its `req`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildMember {
    pub who: Who,
    /// `social::place`.
    pub place: u8,
    /// [`rank`].
    pub rank: u8,
    pub joined_ms: u64,
    pub last_seen_ms: u64,
}

impl GuildMember {
    fn encode(&self, w: &mut Writer) {
        self.who.encode(w);
        w.u8(self.place)
            .u8(self.rank)
            .u64(self.joined_ms)
            .u64(self.last_seen_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let m = Self {
            who: Who::decode(r)?,
            place: read_place(r)?,
            rank: r.u8()?,
            joined_ms: r.u64()?,
            last_seen_ms: r.u64()?,
        };
        if m.rank > rank::LEADER {
            return Err(NetError::Malformed);
        }
        Ok(m)
    }
}

/// S→C 0xF5: pushed to every session of every member (and of the invitee
/// for [`guild_notice::INVITED_YOU`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildNotice {
    /// [`guild_notice`].
    pub what: u8,
    pub guild: u64,
    pub tag: String,
    pub name: String,
    /// The member it is about, a character name.
    pub who: String,
    pub by: String,
    pub at_ms: u64,
}

impl GuildNotice {
    fn encode(&self, w: &mut Writer) {
        w.u8(self.what)
            .u64(self.guild)
            .str8(&self.tag)
            .str8(&self.name)
            .str8(&self.who)
            .str8(&self.by)
            .u64(self.at_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let n = Self {
            what: r.u8()?,
            guild: r.u64()?,
            tag: r.str8(MAX_TAG)?,
            name: r.str8(MAX_GUILD_NAME)?,
            who: r.str8(MAX_FROM)?,
            by: r.str8(MAX_FROM)?,
            at_ms: r.u64()?,
        };
        if !(guild_notice::INVITED_YOU..=guild_notice::MOTD).contains(&n.what) {
            return Err(NetError::Malformed);
        }
        Ok(n)
    }
}

/// A mutual friend's game: what the Join button needs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FriendGame {
    pub game_id: u32,
    pub game_name: String,
    /// 0, or the code why the asker's selected character can't join (as
    /// `GameInfo::joinable`).
    pub joinable: u16,
}

fn put_game(w: &mut Writer, g: Option<&FriendGame>) {
    w.opt(g, |w, g| {
        w.u32(g.game_id).str8(&g.game_name).u16(g.joinable);
    });
}

fn get_game(r: &mut Reader) -> Result<Option<FriendGame>, NetError> {
    r.opt(|r| {
        Ok(FriendGame {
            game_id: r.u32()?,
            game_name: r.str8(MAX_GAME_NAME)?,
            joinable: r.u16()?,
        })
    })
}

/// A row of a [`Ask::FriendsGet`] list (S→C 0xF9, after its `req`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FriendRow {
    /// Their last character.
    pub who: Who,
    /// `social::place`.
    pub place: u8,
    pub mutual: bool,
    /// Mutual friends only.
    pub game: Option<FriendGame>,
    pub last_seen_ms: u64,
    pub added_ms: u64,
}

impl FriendRow {
    fn encode(&self, w: &mut Writer) {
        self.who.encode(w);
        w.u8(self.place).bool(self.mutual);
        put_game(w, self.game.as_ref());
        w.u64(self.last_seen_ms).u64(self.added_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            who: Who::decode(r)?,
            place: read_place(r)?,
            mutual: r.bool()?,
            game: get_game(r)?,
            last_seen_ms: r.u64()?,
            added_ms: r.u64()?,
        })
    }
}

/// S→C 0xFB: pushed to the accounts that added this one (mutual friends
/// get the place).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FriendNotice {
    /// [`friend_notice`].
    pub what: u8,
    pub who: Who,
    /// `social::place`.
    pub place: u8,
    pub game: Option<FriendGame>,
    pub at_ms: u64,
}

impl FriendNotice {
    fn encode(&self, w: &mut Writer) {
        w.u8(self.what);
        self.who.encode(w);
        w.u8(self.place);
        put_game(w, self.game.as_ref());
        w.u64(self.at_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let n = Self {
            what: r.u8()?,
            who: Who::decode(r)?,
            place: read_place(r)?,
            game: get_game(r)?,
            at_ms: r.u64()?,
        };
        if !(friend_notice::ONLINE..=friend_notice::ADDED_YOU).contains(&n.what) {
            return Err(NetError::Malformed);
        }
        Ok(n)
    }
}

/// What a [`Ask::FriendEdit`] does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FriendEdit {
    Add(String),
    Remove(String),
    Ignore(String),
    Unignore(String),
    /// `social::whispers` and `social::show_where`.
    Privacy {
        whispers: u8,
        show_where: u8,
    },
}

/// What a C→S guild or friends message asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// [`guild_get`]; `tag` for [`guild_get::BY_TAG`].
    GuildGet {
        what: u8,
        tag: String,
    },
    GuildCreate {
        tag: String,
        name: String,
    },
    /// A character name or `*account`.
    GuildInvite {
        who: String,
    },
    GuildAnswer {
        guild: u64,
        accept: bool,
    },
    /// [`guild_op`] and its argument (a character name, the tag, the motd).
    GuildManage {
        op: u8,
        arg: String,
    },
    /// [`friends_get`].
    FriendsGet {
        what: u8,
    },
    FriendEdit(FriendEdit),
}

impl Ask {
    pub fn kind(&self) -> u8 {
        match self {
            Self::GuildGet { .. } => kind::GUILD_GET,
            Self::GuildCreate { .. } => kind::GUILD_CREATE,
            Self::GuildInvite { .. } => kind::GUILD_INVITE,
            Self::GuildAnswer { .. } => kind::GUILD_ANSWER,
            Self::GuildManage { .. } => kind::GUILD_MANAGE,
            Self::FriendsGet { .. } => kind::FRIENDS_GET,
            Self::FriendEdit(_) => kind::FRIEND_EDIT,
        }
    }
}

/// A C→S guild or friends message: its request number and what it asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToRealm {
    pub req: u32,
    pub ask: Ask,
}

impl ToRealm {
    pub fn kind(&self) -> u8 {
        self.ask.kind()
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        w.u32(self.req);
        match &self.ask {
            Ask::GuildGet { what, tag } => {
                w.u8(*what).str8(tag);
            }
            Ask::GuildCreate { tag, name } => {
                w.str8(tag).str8(name);
            }
            Ask::GuildInvite { who } => {
                w.str8(who);
            }
            Ask::GuildAnswer { guild, accept } => {
                w.u64(*guild).bool(*accept);
            }
            Ask::GuildManage { op, arg } => {
                w.u8(*op).str8(arg);
            }
            Ask::FriendsGet { what } => {
                w.u8(*what);
            }
            Ask::FriendEdit(e) => match e {
                FriendEdit::Add(who) => {
                    w.u8(0).str8(who);
                }
                FriendEdit::Remove(who) => {
                    w.u8(1).str8(who);
                }
                FriendEdit::Ignore(who) => {
                    w.u8(2).str8(who);
                }
                FriendEdit::Unignore(who) => {
                    w.u8(3).str8(who);
                }
                FriendEdit::Privacy {
                    whispers,
                    show_where,
                } => {
                    w.u8(5).str8("").u8(*whispers).u8(*show_where);
                }
            },
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut rest) = Reader::new(body)?;
        let r = &mut rest;
        let req = r.u32()?;
        let ask = match k {
            kind::GUILD_GET => {
                let what = r.u8()?;
                if what > guild_get::BY_TAG {
                    return Err(NetError::Malformed);
                }
                Ask::GuildGet {
                    what,
                    tag: r.str8(MAX_TAG_WIRE)?,
                }
            }
            kind::GUILD_CREATE => Ask::GuildCreate {
                tag: r.str8(MAX_TAG_WIRE)?,
                name: r.str8(MAX_GUILD_NAME_WIRE)?,
            },
            kind::GUILD_INVITE => Ask::GuildInvite {
                who: r.str8(MAX_TARGET)?,
            },
            kind::GUILD_ANSWER => Ask::GuildAnswer {
                guild: r.u64()?,
                accept: r.bool()?,
            },
            kind::GUILD_MANAGE => {
                let op = r.u8()?;
                if op > guild_op::MOTD {
                    return Err(NetError::Malformed);
                }
                Ask::GuildManage {
                    op,
                    arg: r.str8(MAX_MOTD)?,
                }
            }
            kind::FRIENDS_GET => {
                let what = r.u8()?;
                if what > friends_get::IGNORED {
                    return Err(NetError::Malformed);
                }
                Ask::FriendsGet { what }
            }
            kind::FRIEND_EDIT => {
                let op = r.u8()?;
                let who = r.str8(MAX_TARGET)?;
                Ask::FriendEdit(match op {
                    0 => FriendEdit::Add(who),
                    1 => FriendEdit::Remove(who),
                    2 => FriendEdit::Ignore(who),
                    3 => FriendEdit::Unignore(who),
                    5 if who.is_empty() => {
                        let whispers = r.u8()?;
                        let show_where = r.u8()?;
                        if whispers > super::social::whispers::NOBODY
                            || show_where > super::social::show_where::NOBODY
                        {
                            return Err(NetError::Malformed);
                        }
                        FriendEdit::Privacy {
                            whispers,
                            show_where,
                        }
                    }
                    _ => return Err(NetError::Malformed),
                })
            }
            _ => return Err(NetError::Malformed),
        };
        rest.finish()?;
        Ok(Self { req, ask })
    }
}

/// An S→C guild or friends message (the done answer and the list framing
/// are `social::FromRealm`'s).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromRealm {
    GuildInfo { req: u32, info: GuildInfo },
    GuildMember { req: u32, member: GuildMember },
    GuildNotice(GuildNotice),
    FriendRow { req: u32, row: FriendRow },
    FriendNotice(FriendNotice),
}

impl FromRealm {
    pub fn kind(&self) -> u8 {
        match self {
            Self::GuildInfo { .. } => kind::GUILD_INFO,
            Self::GuildMember { .. } => kind::GUILD_MEMBER,
            Self::GuildNotice(_) => kind::GUILD_NOTICE,
            Self::FriendRow { .. } => kind::FRIEND_ROW,
            Self::FriendNotice(_) => kind::FRIEND_NOTICE,
        }
    }

    /// The request this answers; `None` for a push.
    pub fn req(&self) -> Option<u32> {
        match self {
            Self::GuildInfo { req, .. }
            | Self::GuildMember { req, .. }
            | Self::FriendRow { req, .. } => Some(*req),
            Self::GuildNotice(_) | Self::FriendNotice(_) => None,
        }
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        let w = &mut w;
        match self {
            Self::GuildInfo { req, info } => {
                w.u32(*req);
                info.encode(w);
            }
            Self::GuildMember { req, member } => {
                w.u32(*req);
                member.encode(w);
            }
            Self::GuildNotice(n) => n.encode(w),
            Self::FriendRow { req, row } => {
                w.u32(*req);
                row.encode(w);
            }
            Self::FriendNotice(n) => n.encode(w),
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut rest) = Reader::new(body)?;
        let r = &mut rest;
        let m = match k {
            kind::GUILD_INFO => Self::GuildInfo {
                req: r.u32()?,
                info: GuildInfo::decode(r)?,
            },
            kind::GUILD_MEMBER => Self::GuildMember {
                req: r.u32()?,
                member: GuildMember::decode(r)?,
            },
            kind::GUILD_NOTICE => Self::GuildNotice(GuildNotice::decode(r)?),
            kind::FRIEND_ROW => Self::FriendRow {
                req: r.u32()?,
                row: FriendRow::decode(r)?,
            },
            kind::FRIEND_NOTICE => Self::FriendNotice(FriendNotice::decode(r)?),
            _ => return Err(NetError::Malformed),
        };
        rest.finish()?;
        Ok(m)
    }
}

#[cfg(test)]
#[path = "guild_tests.rs"]
mod tests;
