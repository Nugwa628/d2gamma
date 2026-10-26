//! What the chat model sends and receives, in the client's terms of the lobby
//! contract (`docs/plans/server/lobby-contract.md`, section 3): rooms, room
//! lines, DMs, DM conversations, user lists and the realm's answers. A
//! [`ChatSource`] carries them: [`super::RealmChat`] to the realm,
//! [`super::FakeChat`] for tests and offscreen captures.
//!
//! Lists arrive whole ([`ChatIn::Conversations`], [`ChatIn::History`],
//! [`ChatIn::Users`]): numbering requests and assembling a list's head, rows
//! and end by `req` is the realm client's job, not the model's.

/// A chat room (contract section 2, `Room`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Room {
    /// Every session of the realm.
    Global,
    /// The players of the same mod.
    Mod,
    /// The members of the player's guild.
    Guild,
    /// The players of the current game (the game stream's chat, never on
    /// the realm).
    Game,
}

impl Room {
    pub const ALL: [Room; 4] = [Room::Global, Room::Mod, Room::Guild, Room::Game];

    /// The contract's room number.
    pub fn number(self) -> u8 {
        self as u8
    }

    pub fn from_number(n: u8) -> Option<Room> {
        Room::ALL.get(usize::from(n)).copied()
    }

    /// The room's bit in `SocialHello.rooms` and `SocialState.rooms`.
    pub fn bit(self) -> u16 {
        1 << self.number()
    }
}

/// A player as lines and lists show them (contract section 2, `Who`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Who {
    /// The selected character's name; empty for the realm.
    pub char: String,
    /// The account, when the realm lets the asker see it.
    pub account: String,
    /// The guild tag, without the brackets.
    pub tag: Option<String>,
    /// 0-6, 255 none.
    pub class: u8,
    pub level: u8,
    /// [`who`] bits.
    pub flags: u8,
}

/// [`Who::flags`] bits.
pub mod who {
    pub const HARDCORE: u8 = 1;
    pub const DEAD: u8 = 1 << 1;
    pub const EXPANSION: u8 = 1 << 2;
    pub const LADDER: u8 = 1 << 3;
    pub const MODERATOR: u8 = 1 << 4;
    pub const IN_GAME: u8 = 1 << 5;
    pub const AWAY: u8 = 1 << 6;
}

impl Who {
    pub fn named(char: &str) -> Who {
        Who {
            char: char.to_string(),
            class: 255,
            ..Who::default()
        }
    }
}

/// Where a player is (contract section 2, `Where`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Where {
    #[default]
    Offline,
    Lobby,
    InGame,
    /// The player's privacy hides it.
    Hidden,
}

/// What a room line is (`RoomLine.what`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineWhat {
    Said,
    Emote,
    Joined,
    Left,
    /// A realm notice (admin).
    Notice,
    /// A guild event (joined, left, kicked, promoted, motd).
    GuildEvent,
}

/// The player's guild (`SocialState`'s `GuildBrief`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildBrief {
    pub guild: u64,
    pub tag: String,
    pub name: String,
    /// 0 member, 1 officer, 2 leader.
    pub my_rank: u8,
}

/// The realm's view of the player's chat (`SocialState`), the parts the
/// chat uses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SocialState {
    /// Granted rooms ([`Room::bit`]): Guild only with a guild.
    pub rooms: u16,
    /// The mod's label for the Mod tab ("D2Gamma"); empty: the tab says
    /// "Chat" (a realm with one room).
    pub mod_label: String,
    pub guild: Option<GuildBrief>,
    /// Unread DM lines over all conversations.
    pub dm_unread: u16,
    pub muted_until_ms: Option<u64>,
    pub now_ms: u64,
}

/// A line said in a room (`RoomLine`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomLine {
    pub room: Room,
    /// Per room, rising.
    pub line: u64,
    pub what: LineWhat,
    pub who: Who,
    pub text: String,
    pub at_ms: u64,
}

/// A direct message (`DmLine`), live or from a history page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DmLine {
    pub dm: u64,
    pub peer_account: String,
    /// The character the line was sent as or to.
    pub peer_char: String,
    /// Sent by this account.
    pub outgoing: bool,
    /// It waited for an offline player.
    pub stored: bool,
    pub text: String,
    pub at_ms: u64,
}

/// One DM conversation (`DmConversationRow`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DmConversation {
    pub peer_account: String,
    /// Their last character.
    pub peer_char: String,
    pub place: Where,
    pub unread: u16,
    pub last_ms: u64,
    pub last_text: String,
}

/// One row of a room's user list (`UserRow`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserRow {
    pub who: Who,
    pub place: Where,
}

/// A change to the watched user list (`RoomPresence.op`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresenceOp {
    Joined,
    Left,
    Changed,
}

/// The realm's answer to a request (`SocialDone`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Done {
    /// The request's kind ([`kind`]).
    pub request: u8,
    /// 0 done, else the refusal ([`code`]).
    pub code: u16,
    pub wait_s: u32,
    /// A whisper waits for an offline player (`flags` bit 0).
    pub stored: bool,
    /// The realm's English text, for a code the client doesn't know.
    pub text: String,
}

/// What a source hands the chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatIn {
    State(SocialState),
    Line(RoomLine),
    /// A live DM (to or from this account).
    Dm(DmLine),
    /// A page of DM conversations, newest first.
    Conversations(Vec<DmConversation>),
    /// A history page of one conversation, oldest first.
    History {
        peer_account: String,
        lines: Vec<DmLine>,
    },
    /// A room's user list, sorted by character name.
    Users {
        room: Room,
        rows: Vec<UserRow>,
    },
    /// The watched room's list changed.
    Presence {
        room: Room,
        op: PresenceOp,
        row: UserRow,
        /// Members now.
        count: u16,
    },
    Done(Done),
    /// A market notice (market contract section 5): printed in the Mod tab
    /// and the open tab.
    MarketNotice {
        text: String,
        at_ms: u64,
    },
    /// A line of the game's own chat (the Game tab, in a game).
    GameLine {
        from: String,
        text: String,
        at_ms: u64,
    },
    /// A game or system message (quests, "joined the game", hostility, the
    /// game's notes) in the Game tab, in the game's text colour `colour`
    /// (0-12, the colours the screen messages use).
    GameMessage {
        text: String,
        colour: u8,
        at_ms: u64,
    },
}

/// A friends or ignore list change (`FriendEdit.op`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FriendOp {
    Add,
    Remove,
    Ignore,
    Unignore,
}

/// What the chat asks of its source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatOut {
    /// `SocialHello`: the rooms the host shows.
    Hello { rooms: u16 },
    /// `Say`; [`Room::Game`] goes to the game stream, not the realm.
    Say {
        room: Room,
        emote: bool,
        text: String,
    },
    /// `Whisper`: `to` is a character name or `*account`.
    Whisper { to: String, text: String },
    /// `DmConversations`.
    Conversations { page: u16 },
    /// `DmHistory`: `peer` is a character name or `*account`; `before_dm` 0
    /// is the newest.
    History {
        peer: String,
        before_dm: u64,
        rows: u8,
    },
    /// `DmRead`.
    Read { peer_account: String, up_to_dm: u64 },
    /// `UserList`.
    Users { room: Room, watch: bool },
    /// `FriendEdit` (add, remove, ignore, unignore).
    Friend { op: FriendOp, who: String },
}

impl ChatOut {
    /// The contract's message kind (what [`Done::request`] names).
    pub fn kind(&self) -> u8 {
        match self {
            ChatOut::Hello { .. } => kind::SOCIAL_HELLO,
            ChatOut::Say { .. } => kind::SAY,
            ChatOut::Whisper { .. } => kind::WHISPER,
            ChatOut::Conversations { .. } => kind::DM_CONVERSATIONS,
            ChatOut::History { .. } => kind::DM_HISTORY,
            ChatOut::Read { .. } => kind::DM_READ,
            ChatOut::Users { .. } => kind::USER_LIST,
            ChatOut::Friend { .. } => kind::FRIEND_EDIT,
        }
    }
}

/// The request kinds of contract sections 3 and 5.
pub mod kind {
    pub const SOCIAL_HELLO: u8 = 0x70;
    pub const SAY: u8 = 0x72;
    pub const WHISPER: u8 = 0x73;
    pub const DM_CONVERSATIONS: u8 = 0x76;
    pub const DM_HISTORY: u8 = 0x78;
    pub const DM_READ: u8 = 0x79;
    pub const USER_LIST: u8 = 0x7A;
    pub const FRIEND_EDIT: u8 = 0xFA;
}

/// The refusal codes the chat meets (contract section 8 and the existing
/// codes it names). The wire lane adds them to `d2_net::msg::ErrorCode`;
/// the numbers are the contract's.
pub mod code {
    pub const NOT_SUPPORTED: u16 = 8;
    pub const RATE_LIMITED: u16 = 24;
    pub const NOT_LOGGED_IN: u16 = 27;
    pub const NO_CHAR_SELECTED: u16 = 47;
    pub const NOT_IN_GUILD: u16 = 121;
    pub const UNKNOWN_PLAYER: u16 = 127;
    pub const INBOX_FULL: u16 = 128;
    pub const WHISPER_BLOCKED: u16 = 129;
    pub const FRIEND_LIMIT: u16 = 130;
    pub const MUTED: u16 = 131;
    pub const ROOM_UNAVAILABLE: u16 = 132;
    pub const BAD_TEXT: u16 = 134;
}

/// The chat line for a refusal: the client's sentence for a known code,
/// the realm's text otherwise; waits say how long.
pub fn done_text(d: &Done) -> String {
    let wait = match d.wait_s {
        0 => String::new(),
        s if s < 60 => format!(" Wait {s} seconds."),
        s => format!(" Wait {} minutes.", s.div_ceil(60)),
    };
    let own = match d.code {
        code::NOT_SUPPORTED => "The realm doesn't support that yet.",
        code::RATE_LIMITED => return format!("You are sending too fast.{wait}"),
        code::NOT_LOGGED_IN => "You are not logged in.",
        code::NO_CHAR_SELECTED => "Choose a character to talk in a room.",
        code::NOT_IN_GUILD => "You are not in a guild.",
        code::UNKNOWN_PLAYER => "No such player.",
        code::INBOX_FULL => "Their message box is full.",
        code::WHISPER_BLOCKED => "That player doesn't take whispers from you.",
        code::FRIEND_LIMIT => "Your friends (or ignore) list is full.",
        code::MUTED => return format!("You are muted.{wait}"),
        code::ROOM_UNAVAILABLE => "That chat isn't available.",
        code::BAD_TEXT => "That message isn't allowed.",
        _ if !d.text.is_empty() => return d.text.clone(),
        c => return format!("The realm refused (error {c})."),
    };
    own.to_string()
}

/// Where chat lines come from and requests go.
pub trait ChatSource {
    fn send(&mut self, out: ChatOut);
    /// The next message, if one arrived (never blocks).
    fn poll(&mut self) -> Option<ChatIn>;
}
