//! The realm's chat on the control stream (kinds 0x70-0x7F,
//! `docs/plans/server/lobby-contract.md` sections 1-3): the hello and the
//! pushed state, rooms and their lines, direct messages (kept for offline
//! players), user lists and their presence pushes, and the done answer and
//! list framing the guild and friends messages ([`super::guild`]) share.
//!
//! - Every C→S message is a [`ToRealm`]: a request number (`req`, rising
//!   per session from 1) and an [`Ask`]. Every answer, and every row of a
//!   list, repeats the `req`; 0 is a push.
//! - A list is [`FromRealm::ListBegin`], one message per row with the same
//!   `req`, then [`FromRealm::ListEnd`]; pushes may arrive in between. A
//!   refused request is answered with [`Done`] instead of the list.
//! - The decoders enforce the wire bounds that keep every row inside one
//!   frame (names, tags, text). The chat's own rules (empty or control
//!   text, page sizes, who may speak where) are the realm's to answer with
//!   a refusal.

use super::account::MAX_CHAR_NAME_WIRE;
use super::lobby::{MAX_CHAT, MAX_FROM};
use super::{ErrorCode, Reader, Writer, MAX_ERROR_TEXT};
use crate::NetError;

/// The social kinds.
pub mod kind {
    pub const HELLO: u8 = 0x70;
    pub const STATE: u8 = 0x71;
    pub const SAY: u8 = 0x72;
    pub const WHISPER: u8 = 0x73;
    pub const ROOM_LINE: u8 = 0x74;
    pub const DM_LINE: u8 = 0x75;
    pub const DM_CONVERSATIONS: u8 = 0x76;
    pub const DM_CONVERSATION_ROW: u8 = 0x77;
    pub const DM_HISTORY: u8 = 0x78;
    pub const DM_READ: u8 = 0x79;
    pub const USER_LIST: u8 = 0x7A;
    pub const USER_ROW: u8 = 0x7B;
    pub const ROOM_PRESENCE: u8 = 0x7C;
    pub const DONE: u8 = 0x7D;
    pub const LIST_BEGIN: u8 = 0x7E;
    pub const LIST_END: u8 = 0x7F;
}

/// An account name as the realm sends it (the account rule's longest).
pub const MAX_ACCOUNT: usize = 15;
/// Who a request names: a character name, or `*` and an account name.
/// Wider than the rules, so the realm answers `UnknownPlayer` instead of
/// closing the connection.
pub const MAX_TARGET: usize = MAX_CHAR_NAME_WIRE + 1;
/// A guild tag as the realm sends it (2-4 letters or digits).
pub const MAX_TAG: usize = 4;
/// A guild name as the realm sends it (3-24).
pub const MAX_GUILD_NAME: usize = 24;
/// A mod's id and its label ("D2Gamma").
pub const MAX_MOD: usize = 32;
/// The last line of a DM conversation row, cut by the realm.
pub const MAX_LAST_TEXT: usize = 60;
/// DM conversations per page.
pub const DM_CONVERSATIONS_PAGE: u16 = 30;
/// Most lines one [`Ask::DmHistory`] asks for.
pub const MAX_HISTORY_ROWS: u8 = 50;

/// The chat rooms (`u8`; a bit each in the hello's and the state's
/// `rooms`).
pub mod room {
    /// Every session of the realm.
    pub const GLOBAL: u8 = 0;
    /// The sessions of the asker's mod game.
    pub const MOD: u8 = 1;
    /// The members of the asker's guild.
    pub const GUILD: u8 = 2;
    /// Never on the realm: the in-game window's tab for the game stream's
    /// chat, numbered here so client and realm number rooms alike.
    pub const GAME: u8 = 3;
    /// The highest room number; 4-15 are kept for later rooms.
    pub const MAX: u8 = 15;

    /// The `rooms` bit of `room`.
    pub const fn bit(room: u8) -> u16 {
        1 << room
    }
}

/// Where a player is (`u8`).
pub mod place {
    pub const OFFLINE: u8 = 0;
    /// Character select or the lobby.
    pub const LOBBY: u8 = 1;
    pub const IN_GAME: u8 = 2;
    /// The player's privacy hides it.
    pub const HIDDEN: u8 = 3;
}

/// The bits of [`Who::flags`].
pub mod who_flags {
    pub const HARDCORE: u8 = 1 << 0;
    pub const DEAD: u8 = 1 << 1;
    pub const EXPANSION: u8 = 1 << 2;
    pub const LADDER: u8 = 1 << 3;
    pub const MODERATOR: u8 = 1 << 4;
    pub const IN_GAME: u8 = 1 << 5;
    pub const AWAY: u8 = 1 << 6;
}

/// What a [`RoomLine`] is (`u8`).
pub mod line_what {
    pub const SAID: u8 = 0;
    pub const EMOTE: u8 = 1;
    pub const JOINED: u8 = 2;
    pub const LEFT: u8 = 3;
    pub const REALM_NOTICE: u8 = 4;
    pub const GUILD_EVENT: u8 = 5;
}

/// What a [`RoomPresence`] says (`u8`).
pub mod presence_op {
    pub const JOINED: u8 = 0;
    pub const LEFT: u8 = 1;
    /// Level, guild or the in-game flag changed.
    pub const CHANGED: u8 = 2;
}

/// The bits of [`Done::flags`].
pub mod done_flags {
    /// A whisper was stored for an offline player.
    pub const STORED: u8 = 1 << 0;
}

/// Who may whisper (`SocialState.whispers`, contract 3.4).
pub mod whispers {
    pub const EVERYONE: u8 = 0;
    pub const FRIENDS_AND_GUILD: u8 = 1;
    /// Nobody (replies to the player's own DMs still pass).
    pub const NOBODY: u8 = 2;
}

/// Who sees where the player is (`SocialState.show_where`).
pub mod show_where {
    pub const MUTUAL_FRIENDS: u8 = 0;
    pub const NOBODY: u8 = 1;
}

/// Character classes 0-6, or this for none.
pub const NO_CLASS: u8 = 255;

/// A player as lists and lines show them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Who {
    /// The character name; empty for the realm.
    pub char: String,
    /// Empty unless the asker may see it (friends, own guild, DMs).
    pub account: String,
    /// The guild tag.
    pub tag: Option<String>,
    /// 0-6, [`NO_CLASS`] none.
    pub class: u8,
    pub level: u8,
    /// [`who_flags`].
    pub flags: u8,
}

impl Who {
    pub fn encode(&self, w: &mut Writer) {
        w.str8(&self.char).str8(&self.account);
        w.opt(self.tag.as_ref(), |w, t| {
            w.str8(t);
        });
        w.u8(self.class).u8(self.level).u8(self.flags);
    }

    pub fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let who = Self {
            char: r.str8(MAX_FROM)?,
            account: r.str8(MAX_ACCOUNT)?,
            tag: r.opt(|r| r.str8(MAX_TAG))?,
            class: r.u8()?,
            level: r.u8()?,
            flags: r.u8()?,
        };
        if !valid_class(who.class) || who.flags & 0x80 != 0 {
            return Err(NetError::Malformed);
        }
        Ok(who)
    }
}

/// A class byte: 0-6, or [`NO_CLASS`].
pub(crate) fn valid_class(class: u8) -> bool {
    class < super::account::CLASSES || class == NO_CLASS
}

/// A [`place`] byte.
pub(crate) fn read_place(r: &mut Reader) -> Result<u8, NetError> {
    let p = r.u8()?;
    if p > place::HIDDEN {
        return Err(NetError::Malformed);
    }
    Ok(p)
}

/// A room number of a line or a request: 0-[`room::MAX`].
fn read_room(r: &mut Reader) -> Result<u8, NetError> {
    let n = r.u8()?;
    if n > room::MAX {
        return Err(NetError::Malformed);
    }
    Ok(n)
}

/// The guild of a [`SocialState`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildBrief {
    pub guild: u64,
    pub tag: String,
    pub name: String,
    /// `guild::rank`.
    pub my_rank: u8,
}

/// The chat's rate limits as the realm runs them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChatLimits {
    pub say_lines: u8,
    pub say_window_s: u16,
    pub global_lines: u8,
    pub global_window_s: u16,
}

/// S→C 0x71: the answer to a hello, and pushed (`req` 0) when a field
/// changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SocialState {
    pub req: u32,
    /// The rooms granted ([`room::bit`]): Guild only with a guild.
    pub rooms: u16,
    pub mod_id: String,
    /// "D2Gamma".
    pub mod_label: String,
    pub guild: Option<GuildBrief>,
    /// Unread DM lines, all conversations.
    pub dm_unread: u16,
    /// Conversations with unread lines.
    pub dm_conversations_unread: u16,
    pub friends_online: u16,
    pub guild_invites: u16,
    /// [`whispers`].
    pub whispers: u8,
    /// [`show_where`].
    pub show_where: u8,
    pub limits: ChatLimits,
    /// Set while the account is muted.
    pub muted_until_ms: Option<u64>,
    pub now_ms: u64,
}

impl SocialState {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.req)
            .u16(self.rooms)
            .str8(&self.mod_id)
            .str8(&self.mod_label);
        w.opt(self.guild.as_ref(), |w, g| {
            w.u64(g.guild).str8(&g.tag).str8(&g.name).u8(g.my_rank);
        });
        w.u16(self.dm_unread)
            .u16(self.dm_conversations_unread)
            .u16(self.friends_online)
            .u16(self.guild_invites)
            .u8(self.whispers)
            .u8(self.show_where)
            .u8(self.limits.say_lines)
            .u16(self.limits.say_window_s)
            .u8(self.limits.global_lines)
            .u16(self.limits.global_window_s);
        w.opt(self.muted_until_ms.as_ref(), |w, v| {
            w.u64(*v);
        });
        w.u64(self.now_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let s = Self {
            req: r.u32()?,
            rooms: r.u16()?,
            mod_id: r.str8(MAX_MOD)?,
            mod_label: r.str8(MAX_MOD)?,
            guild: r.opt(|r| {
                Ok(GuildBrief {
                    guild: r.u64()?,
                    tag: r.str8(MAX_TAG)?,
                    name: r.str8(MAX_GUILD_NAME)?,
                    my_rank: r.u8()?,
                })
            })?,
            dm_unread: r.u16()?,
            dm_conversations_unread: r.u16()?,
            friends_online: r.u16()?,
            guild_invites: r.u16()?,
            whispers: r.u8()?,
            show_where: r.u8()?,
            limits: ChatLimits {
                say_lines: r.u8()?,
                say_window_s: r.u16()?,
                global_lines: r.u8()?,
                global_window_s: r.u16()?,
            },
            muted_until_ms: r.opt(|r| r.u64())?,
            now_ms: r.u64()?,
        };
        if s.whispers > whispers::NOBODY
            || s.show_where > show_where::NOBODY
            || s.guild
                .as_ref()
                .is_some_and(|g| g.my_rank > super::guild::rank::LEADER)
        {
            return Err(NetError::Malformed);
        }
        Ok(s)
    }
}

/// S→C 0x74: a line of a room (said, an emote, joined, left, a realm
/// notice, a guild event). Never a list row: lines carry no `req`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomLine {
    pub room: u8,
    /// Per room, rising.
    pub line: u64,
    /// [`line_what`].
    pub what: u8,
    pub who: Who,
    pub text: String,
    pub at_ms: u64,
}

impl RoomLine {
    fn encode(&self, w: &mut Writer) {
        w.u8(self.room).u64(self.line).u8(self.what);
        self.who.encode(w);
        w.str8(&self.text).u64(self.at_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let l = Self {
            room: read_room(r)?,
            line: r.u64()?,
            what: r.u8()?,
            who: Who::decode(r)?,
            text: r.str8(MAX_CHAT)?,
            at_ms: r.u64()?,
        };
        if l.what > line_what::GUILD_EVENT {
            return Err(NetError::Malformed);
        }
        Ok(l)
    }
}

/// S→C 0x75: a direct message line. Pushed (`req` 0) to every session of
/// both accounts, or a row of a [`Ask::DmHistory`] list (its `req`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DmLine {
    pub req: u32,
    pub dm: u64,
    pub peer_account: String,
    /// The character the line was sent as or to.
    pub peer_char: String,
    pub outgoing: bool,
    /// It waited for an offline player.
    pub stored: bool,
    pub text: String,
    pub at_ms: u64,
}

impl DmLine {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.req)
            .u64(self.dm)
            .str8(&self.peer_account)
            .str8(&self.peer_char)
            .bool(self.outgoing)
            .bool(self.stored)
            .str8(&self.text)
            .u64(self.at_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            req: r.u32()?,
            dm: r.u64()?,
            peer_account: r.str8(MAX_ACCOUNT)?,
            peer_char: r.str8(MAX_FROM)?,
            outgoing: r.bool()?,
            stored: r.bool()?,
            text: r.str8(MAX_CHAT)?,
            at_ms: r.u64()?,
        })
    }
}

/// A row of a [`Ask::DmConversations`] list (S→C 0x77, after its `req`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DmConversationRow {
    pub peer_account: String,
    /// Their last character.
    pub peer_char: String,
    /// [`place`].
    pub place: u8,
    pub unread: u16,
    pub last_ms: u64,
    /// Cut to [`MAX_LAST_TEXT`] bytes.
    pub last_text: String,
}

impl DmConversationRow {
    fn encode(&self, w: &mut Writer) {
        w.str8(&self.peer_account)
            .str8(&self.peer_char)
            .u8(self.place)
            .u16(self.unread)
            .u64(self.last_ms)
            .str8(&self.last_text);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            peer_account: r.str8(MAX_ACCOUNT)?,
            peer_char: r.str8(MAX_FROM)?,
            place: read_place(r)?,
            unread: r.u16()?,
            last_ms: r.u64()?,
            last_text: r.str8(MAX_LAST_TEXT)?,
        })
    }
}

/// S→C 0x7C: pushed for the session's watched room.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomPresence {
    pub room: u8,
    /// [`presence_op`].
    pub op: u8,
    pub who: Who,
    /// [`place`].
    pub place: u8,
    /// Members now.
    pub count: u16,
}

impl RoomPresence {
    fn encode(&self, w: &mut Writer) {
        w.u8(self.room).u8(self.op);
        self.who.encode(w);
        w.u8(self.place).u16(self.count);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        let p = Self {
            room: read_room(r)?,
            op: r.u8()?,
            who: Who::decode(r)?,
            place: read_place(r)?,
            count: r.u16()?,
        };
        if p.op > presence_op::CHANGED {
            return Err(NetError::Malformed);
        }
        Ok(p)
    }
}

/// S→C 0x7D: a social, guild or friends request's answer when it is not
/// a list (or a list that was refused): done (code 0) or the refusal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Done {
    pub req: u32,
    /// The kind of the request this answers.
    pub request_kind: u8,
    /// [`ErrorCode`]; 0 done.
    pub code: ErrorCode,
    /// For [`ErrorCode::RATE_LIMITED`] and [`ErrorCode::MUTED`].
    pub wait_s: u32,
    /// [`done_flags`].
    pub flags: u8,
    /// The DM or the guild a request made.
    pub id: Option<u64>,
    /// English, for a code the client doesn't know.
    pub text: String,
}

impl Done {
    /// A request carried out.
    pub fn ok(req: u32, request_kind: u8) -> Self {
        Self::refused(req, request_kind, ErrorCode(0))
    }

    /// A request refused with the code's own words.
    pub fn refused(req: u32, request_kind: u8, code: ErrorCode) -> Self {
        Self {
            req,
            request_kind,
            code,
            wait_s: 0,
            flags: 0,
            id: None,
            text: if code.0 == 0 {
                String::new()
            } else {
                code.text().to_owned()
            },
        }
    }

    pub fn is_ok(&self) -> bool {
        self.code.0 == 0
    }

    /// What the player reads for a refusal: this build's words for a code
    /// it knows (with the wait), the realm's text for one it doesn't.
    pub fn message(&self) -> String {
        let words = match self.code {
            c if c.0 == 0 => return String::new(),
            c if ErrorCode::ALL.contains(&c) && c != ErrorCode::OTHER => c.text().to_owned(),
            _ => self.text.clone(),
        };
        match self.wait_s {
            0 => words,
            s => format!("{words} Wait {s} s."),
        }
    }

    fn encode(&self, w: &mut Writer) {
        w.u32(self.req)
            .u8(self.request_kind)
            .u16(self.code.0)
            .u32(self.wait_s)
            .u8(self.flags);
        w.opt(self.id.as_ref(), |w, v| {
            w.u64(*v);
        });
        w.str8(&self.text);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            req: r.u32()?,
            request_kind: r.u8()?,
            code: ErrorCode(r.u16()?),
            wait_s: r.u32()?,
            flags: r.u8()?,
            id: r.opt(|r| r.u64())?,
            text: r.str8(MAX_ERROR_TEXT)?,
        })
    }
}

/// S→C 0x7E: a list follows (social, guild and friends lists alike).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListHead {
    pub req: u32,
    /// The kind of the request it answers.
    pub what: u8,
    pub total: u32,
    pub page: u16,
    /// Rows that follow.
    pub rows: u16,
    pub now_ms: u64,
}

impl ListHead {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.req)
            .u8(self.what)
            .u32(self.total)
            .u16(self.page)
            .u16(self.rows)
            .u64(self.now_ms);
    }

    fn decode(r: &mut Reader) -> Result<Self, NetError> {
        Ok(Self {
            req: r.u32()?,
            what: r.u8()?,
            total: r.u32()?,
            page: r.u16()?,
            rows: r.u16()?,
            now_ms: r.u64()?,
        })
    }
}

/// What a C→S social message asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// After login, after a reconnect, and when the selected character
    /// changes: the rooms the client shows ([`room::bit`]).
    Hello { rooms: u16 },
    /// A line in a room (0-2); the line comes back as a [`RoomLine`].
    Say { room: u8, emote: bool, text: String },
    /// A DM to a character name or `*account`.
    Whisper { to: String, text: String },
    /// A page of DM conversations ([`DM_CONVERSATIONS_PAGE`] a page,
    /// newest first).
    DmConversations { page: u16 },
    /// DM lines with a peer, oldest first, before `before_dm` (0 the
    /// newest).
    DmHistory {
        peer: String,
        before_dm: u64,
        rows: u8,
    },
    /// Marks a conversation read up to a line.
    DmRead { peer_account: String, up_to_dm: u64 },
    /// A room's user list; `watch` keeps it live with [`RoomPresence`]
    /// (one watched room per session).
    UserList { room: u8, watch: bool },
}

impl Ask {
    pub fn kind(&self) -> u8 {
        match self {
            Self::Hello { .. } => kind::HELLO,
            Self::Say { .. } => kind::SAY,
            Self::Whisper { .. } => kind::WHISPER,
            Self::DmConversations { .. } => kind::DM_CONVERSATIONS,
            Self::DmHistory { .. } => kind::DM_HISTORY,
            Self::DmRead { .. } => kind::DM_READ,
            Self::UserList { .. } => kind::USER_LIST,
        }
    }
}

/// A C→S social message: its request number and what it asks.
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
            Ask::Hello { rooms } => {
                w.u16(*rooms);
            }
            Ask::Say { room, emote, text } => {
                w.u8(*room).bool(*emote).str8(text);
            }
            Ask::Whisper { to, text } => {
                w.str8(to).str8(text);
            }
            Ask::DmConversations { page } => {
                w.u16(*page);
            }
            Ask::DmHistory {
                peer,
                before_dm,
                rows,
            } => {
                w.str8(peer).u64(*before_dm).u8(*rows);
            }
            Ask::DmRead {
                peer_account,
                up_to_dm,
            } => {
                w.str8(peer_account).u64(*up_to_dm);
            }
            Ask::UserList { room, watch } => {
                w.u8(*room).bool(*watch);
            }
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut rest) = Reader::new(body)?;
        let r = &mut rest;
        let req = r.u32()?;
        let ask = match k {
            kind::HELLO => Ask::Hello { rooms: r.u16()? },
            kind::SAY => Ask::Say {
                room: read_room(r)?,
                emote: r.bool()?,
                text: r.str8(MAX_CHAT)?,
            },
            kind::WHISPER => Ask::Whisper {
                to: r.str8(MAX_TARGET)?,
                text: r.str8(MAX_CHAT)?,
            },
            kind::DM_CONVERSATIONS => Ask::DmConversations { page: r.u16()? },
            kind::DM_HISTORY => Ask::DmHistory {
                peer: r.str8(MAX_TARGET)?,
                before_dm: r.u64()?,
                rows: r.u8()?,
            },
            kind::DM_READ => Ask::DmRead {
                peer_account: r.str8(MAX_TARGET)?,
                up_to_dm: r.u64()?,
            },
            kind::USER_LIST => Ask::UserList {
                room: read_room(r)?,
                watch: r.bool()?,
            },
            _ => return Err(NetError::Malformed),
        };
        rest.finish()?;
        Ok(Self { req, ask })
    }
}

/// An S→C social message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FromRealm {
    State(SocialState),
    RoomLine(RoomLine),
    DmLine(DmLine),
    DmConversationRow { req: u32, row: DmConversationRow },
    UserRow { req: u32, who: Who, place: u8 },
    RoomPresence(RoomPresence),
    Done(Done),
    ListBegin(ListHead),
    ListEnd { req: u32 },
}

impl FromRealm {
    pub fn kind(&self) -> u8 {
        match self {
            Self::State(_) => kind::STATE,
            Self::RoomLine(_) => kind::ROOM_LINE,
            Self::DmLine(_) => kind::DM_LINE,
            Self::DmConversationRow { .. } => kind::DM_CONVERSATION_ROW,
            Self::UserRow { .. } => kind::USER_ROW,
            Self::RoomPresence(_) => kind::ROOM_PRESENCE,
            Self::Done(_) => kind::DONE,
            Self::ListBegin(_) => kind::LIST_BEGIN,
            Self::ListEnd { .. } => kind::LIST_END,
        }
    }

    /// The request this answers; 0 for a push, `None` for a message that
    /// never carries one (room lines and presence).
    pub fn req(&self) -> Option<u32> {
        match self {
            Self::State(s) => Some(s.req),
            Self::DmLine(l) => Some(l.req),
            Self::Done(d) => Some(d.req),
            Self::ListBegin(h) => Some(h.req),
            Self::DmConversationRow { req, .. }
            | Self::UserRow { req, .. }
            | Self::ListEnd { req } => Some(*req),
            Self::RoomLine(_) | Self::RoomPresence(_) => None,
        }
    }

    pub fn body(&self) -> Vec<u8> {
        let mut w = Writer::new(self.kind());
        let w = &mut w;
        match self {
            Self::State(s) => s.encode(w),
            Self::RoomLine(l) => l.encode(w),
            Self::DmLine(l) => l.encode(w),
            Self::DmConversationRow { req, row } => {
                w.u32(*req);
                row.encode(w);
            }
            Self::UserRow { req, who, place } => {
                w.u32(*req);
                who.encode(w);
                w.u8(*place);
            }
            Self::RoomPresence(p) => p.encode(w),
            Self::Done(d) => d.encode(w),
            Self::ListBegin(h) => h.encode(w),
            Self::ListEnd { req } => {
                w.u32(*req);
            }
        }
        w.finish()
    }

    pub fn parse(body: &[u8]) -> Result<Self, NetError> {
        let (k, mut rest) = Reader::new(body)?;
        let r = &mut rest;
        let m = match k {
            kind::STATE => Self::State(SocialState::decode(r)?),
            kind::ROOM_LINE => Self::RoomLine(RoomLine::decode(r)?),
            kind::DM_LINE => Self::DmLine(DmLine::decode(r)?),
            kind::DM_CONVERSATION_ROW => Self::DmConversationRow {
                req: r.u32()?,
                row: DmConversationRow::decode(r)?,
            },
            kind::USER_ROW => Self::UserRow {
                req: r.u32()?,
                who: Who::decode(r)?,
                place: read_place(r)?,
            },
            kind::ROOM_PRESENCE => Self::RoomPresence(RoomPresence::decode(r)?),
            kind::DONE => Self::Done(Done::decode(r)?),
            kind::LIST_BEGIN => Self::ListBegin(ListHead::decode(r)?),
            kind::LIST_END => Self::ListEnd { req: r.u32()? },
            _ => return Err(NetError::Malformed),
        };
        rest.finish()?;
        Ok(m)
    }
}

#[cfg(test)]
#[path = "social_tests.rs"]
mod tests;
