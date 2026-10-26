//! What the online screens send to the server and what comes back, in the
//! screens' own terms. The screens never touch the network: they push
//! [`Request`]s into an [`OnlineLink`] and read [`Event`]s from it once per
//! tick. The game's link (the `online` feature) maps these onto the control
//! stream messages of `docs/plans/server/multiplayer-contracts.md` (sections
//! 3.4-3.8); tests and offscreen captures use [`super::fake::FakeServer`].
//!
//! A `Request::CreateGame` carries no player count: the link sends
//! `max_players` 0, the server's default (contracts section 3.6).
//!
//! Multiplayer v2's game-server choice (contracts section 11.4) is in the
//! same terms: the main server's list of official game servers
//! ([`Event::Nodes`]), the client's pings to them ([`Event::Pings`]),
//! where a new game runs ([`Request::CreateGame`]'s `place`) and which
//! server a listed game runs on ([`GameInfo::server`]).
//!
//! The realm lobby (`docs/plans/server/lobby-contract.md` v1) adds chat
//! rooms, direct messages (DMs), user lists, guilds, friends and the
//! ladder in the same terms. The link numbers those requests and puts the
//! contract's lists (a head, one message per row, an end) back together,
//! so each list reaches the screens as one event ([`Event::Users`],
//! [`Event::Ladder`], ...). A request that changes something is answered
//! with [`Event::Done`]; any refusal of these requests is
//! [`Event::Refused`], naming what was asked ([`Asked`]), never
//! [`Event::Error`].

use std::collections::BTreeMap;

pub use d2_net::msg::nodelist::{
    code as node_code, NodeInfo, NodeState, Placement, PLAYER_HOSTED_NOTE,
};
pub use d2_net::msg::ErrorCode;

/// The realm lobby's refusals (lobby contract section 8), as `d2_net`
/// numbers them.
pub mod lobby_code {
    use super::ErrorCode;

    pub const LEVEL_RANGE: ErrorCode = ErrorCode::LEVEL_RANGE;
    pub const NOT_IN_GUILD: ErrorCode = ErrorCode::NOT_IN_GUILD;
    pub const GUILD_RANK: ErrorCode = ErrorCode::GUILD_RANK;
    pub const GUILD_TAG_TAKEN: ErrorCode = ErrorCode::GUILD_TAG_TAKEN;
    pub const BAD_GUILD_NAME: ErrorCode = ErrorCode::BAD_GUILD_NAME;
    pub const ALREADY_IN_GUILD: ErrorCode = ErrorCode::ALREADY_IN_GUILD;
    pub const GUILD_FULL: ErrorCode = ErrorCode::GUILD_FULL;
    pub const UNKNOWN_PLAYER: ErrorCode = ErrorCode::UNKNOWN_PLAYER;
    pub const INBOX_FULL: ErrorCode = ErrorCode::INBOX_FULL;
    pub const WHISPER_BLOCKED: ErrorCode = ErrorCode::WHISPER_BLOCKED;
    pub const FRIEND_LIMIT: ErrorCode = ErrorCode::FRIEND_LIMIT;
    pub const MUTED: ErrorCode = ErrorCode::MUTED;
    pub const ROOM_UNAVAILABLE: ErrorCode = ErrorCode::ROOM_UNAVAILABLE;
    pub const NO_SUCH_SEASON: ErrorCode = ErrorCode::NO_SUCH_SEASON;
    pub const BAD_TEXT: ErrorCode = ErrorCode::BAD_TEXT;
    pub const INVITE_GONE: ErrorCode = ErrorCode::INVITE_GONE;
}

/// One server of `servers.toml`, as the login screen lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerEntry {
    pub id: String,
    pub name: String,
    /// `host:port`, shown under the name.
    pub address: String,
}

/// An online character (the contract's `CharSummary`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharInfo {
    pub name: String,
    /// 0-6.
    pub class: u8,
    pub level: u8,
    /// The save status word: hardcore, dead, expansion, progression.
    pub status: u16,
    /// Unlocked difficulties: bit 0 Normal, bit 1 Nightmare, bit 2 Hell.
    pub difficulties: u8,
    /// The save header's appearance bytes: 16 components, then 16 colours.
    pub look: Vec<u8>,
    /// Unix seconds, 0 if never played.
    pub last_played: u64,
    /// The ladder season it belongs to (0: non-ladder).
    pub season: u16,
}

/// One open game in the lobby's list (the contract's `GameInfo`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameInfo {
    pub id: u32,
    pub name: String,
    /// 0 Normal, 1 Nightmare, 2 Hell.
    pub difficulty: u8,
    pub players: u8,
    pub max_players: u8,
    /// Has a password.
    pub locked: bool,
    pub hardcore: bool,
    pub expansion: bool,
    /// 0: the selected character can join; else the error code why not.
    pub joinable: u16,
    /// The game server it runs on: an official server's name, or the
    /// host's character name for a player-hosted game; empty from a server
    /// without game servers (v1).
    pub server: String,
    /// Runs on a player's PC: the list shows [`PLAYER_HOSTED_NOTE`].
    pub player_hosted: bool,
    /// The game server's id in [`Event::Nodes`]; empty for a
    /// player-hosted game.
    pub node_id: String,
    /// Player-hosted only: the host's round trip to the realm (0 unknown),
    /// for the list's `~` estimate.
    pub host_rtt_ms: u16,
    /// The creator's description (0-40 bytes).
    pub description: String,
    /// The level range (0: no bound).
    pub level_min: u8,
    pub level_max: u8,
    /// When it was created (Unix ms).
    pub created_ms: u64,
}

/// The client's pings by game-server id: the round trip in milliseconds,
/// `None` when no ping was answered; a server not in it is being measured.
pub type PingTable = BTreeMap<String, Option<u32>>;

/// A game the server let the character into: the host starts the game
/// stream with the ticket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Joined {
    pub game_id: u32,
    /// The dedicated server's ticket (its `JoinOk`); unused for a game on
    /// a game server.
    pub ticket: [u8; 16],
    pub difficulty: u8,
    /// The game's name, for the loading screen.
    pub name: String,
    /// The main server's `JoinNode`: the game runs on that game server.
    pub node: Option<d2_net::msg::lobby::JoinNode>,
}

/// A failed request (the contract's `ERROR`, `Refused`, or the connection
/// failing).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The error code (section 3.8); `ErrorCode(0)` for a connection
    /// failure of the client's own (no route, timeout, certificate
    /// mismatch).
    pub code: ErrorCode,
    /// Seconds to wait, for rate limits and locks.
    pub wait_s: u32,
    /// The server's English text (shown for a code the client does not know).
    pub text: String,
}

/// The chat rooms (the contract's `Room`): a session is in Global, its Mod
/// room and, with a guild, its Guild room. Game is never on the realm: it
/// is the in-game window's tab for the game's own chat.
pub mod room {
    pub const GLOBAL: u8 = 0;
    pub const MOD: u8 = 1;
    pub const GUILD: u8 = 2;
    pub const GAME: u8 = 3;

    /// A room's bit in `SocialHello.rooms` and `SocialState.rooms`.
    pub fn bit(room: u8) -> u16 {
        1u16.checked_shl(u32::from(room)).unwrap_or(0)
    }
}

/// [`Who::flags`] bits.
pub mod who_flag {
    pub const HARDCORE: u8 = 1;
    pub const DEAD: u8 = 1 << 1;
    pub const EXPANSION: u8 = 1 << 2;
    pub const LADDER: u8 = 1 << 3;
    pub const MODERATOR: u8 = 1 << 4;
    pub const IN_GAME: u8 = 1 << 5;
    pub const AWAY: u8 = 1 << 6;
}

/// A player as chat lines and lists show them (the contract's `Who`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Who {
    /// The selected character's name (empty for the realm itself).
    pub char: String,
    /// The account, when the asker may see it (friends, own guild, DMs).
    pub account: String,
    /// The guild tag, shown as `<TAG>`.
    pub tag: Option<String>,
    /// 0-6, 255 none.
    pub class: u8,
    pub level: u8,
    /// [`who_flag`] bits.
    pub flags: u8,
}

/// Where a player is (the contract's `Where`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Where {
    #[default]
    Offline,
    /// Character select or the lobby.
    Lobby,
    InGame,
    /// The player's privacy hides it.
    Hidden,
}

impl Where {
    pub fn from_wire(v: u8) -> Option<Self> {
        Some(match v {
            0 => Where::Offline,
            1 => Where::Lobby,
            2 => Where::InGame,
            3 => Where::Hidden,
            _ => return None,
        })
    }

    pub fn wire(self) -> u8 {
        match self {
            Where::Offline => 0,
            Where::Lobby => 1,
            Where::InGame => 2,
            Where::Hidden => 3,
        }
    }
}

/// The account's guild in [`SocialState`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildBrief {
    pub guild: u64,
    pub tag: String,
    pub name: String,
    /// [`guild_rank`].
    pub my_rank: u8,
}

/// Guild ranks.
pub mod guild_rank {
    pub const MEMBER: u8 = 0;
    pub const OFFICER: u8 = 1;
    pub const LEADER: u8 = 2;
    /// [`super::GuildInfo::my_rank`] of a guild the asker isn't in.
    pub const NONE: u8 = 255;
}

/// The session's chat facts (the contract's `SocialState`): the answer to
/// [`Request::SocialHello`], and pushed again when one changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SocialState {
    /// The rooms granted ([`room::bit`]; Guild only with a guild).
    pub rooms: u16,
    pub mod_id: String,
    /// The Mod tab's label ("D2Gamma").
    pub mod_label: String,
    pub guild: Option<GuildBrief>,
    /// Unread DM lines in all conversations, and the conversations with
    /// unread lines.
    pub dm_unread: u16,
    pub dm_conversations_unread: u16,
    pub friends_online: u16,
    pub guild_invites: u16,
    /// Privacy ([`Privacy`]'s values).
    pub whispers: u8,
    pub show_where: u8,
    /// The chat limits: lines per window in every room, and in Global.
    pub say_lines: u8,
    pub say_window_s: u16,
    pub global_lines: u8,
    pub global_window_s: u16,
    pub muted_until_ms: Option<u64>,
    pub now_ms: u64,
}

/// [`RoomLine::what`] values.
pub mod line_what {
    pub const SAID: u8 = 0;
    pub const EMOTE: u8 = 1;
    pub const JOINED: u8 = 2;
    pub const LEFT: u8 = 3;
    pub const NOTICE: u8 = 4;
    pub const GUILD_EVENT: u8 = 5;
}

/// A line of a chat room (backlog lines arrive the same way).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomLine {
    pub room: u8,
    /// Rising per room.
    pub line: u64,
    /// [`line_what`].
    pub what: u8,
    pub who: Who,
    pub text: String,
    pub at_ms: u64,
}

/// A direct message line, live or from a conversation's history.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DmLine {
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

/// One DM conversation in the DM tab's list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DmConversation {
    pub peer_account: String,
    /// Their last character.
    pub peer_char: String,
    pub place: Where,
    pub unread: u16,
    pub last_ms: u64,
    /// The last line, cut to 60 bytes.
    pub last_text: String,
}

/// A player of a room's user list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserRow {
    pub who: Who,
    pub place: Where,
}

/// [`RoomPresence::op`] values.
pub mod presence_op {
    pub const JOINED: u8 = 0;
    pub const LEFT: u8 = 1;
    pub const CHANGED: u8 = 2;
}

/// A change in the watched room's user list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomPresence {
    pub room: u8,
    /// [`presence_op`].
    pub op: u8,
    pub who: Who,
    pub place: Where,
    /// Members now.
    pub count: u16,
}

/// A guild (the contract's `GuildInfo`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildInfo {
    pub guild: u64,
    pub tag: String,
    pub name: String,
    pub motd: String,
    pub created_ms: u64,
    pub members: u16,
    pub leader_char: String,
    /// [`guild_rank`] ([`guild_rank::NONE`]: not a member).
    pub my_rank: u8,
}

/// A member of the own guild.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildMember {
    pub who: Who,
    pub place: Where,
    /// [`guild_rank`].
    pub rank: u8,
    pub joined_ms: u64,
    pub last_seen_ms: u64,
}

/// [`GuildNotice::what`] values.
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

/// Something happened in the guild (or an invite to one).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuildNotice {
    /// [`guild_notice`].
    pub what: u8,
    pub guild: u64,
    pub tag: String,
    pub name: String,
    /// The member it is about (a character name).
    pub who: String,
    pub by: String,
    pub at_ms: u64,
}

/// What [`Request::GuildGet`] asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuildQuery {
    /// The own guild with its members ([`Event::Guild`]).
    Mine,
    /// The invites to the account ([`Event::GuildInvites`]).
    Invites,
    /// A guild's public facts ([`Event::GuildFound`]).
    ByTag(String),
}

/// What [`Request::GuildManage`] does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GuildOp {
    Leave,
    /// A character name or `*account`.
    Kick(String),
    Promote(String),
    Demote(String),
    MakeLeader(String),
    /// The guild's tag, typed to confirm.
    Disband(String),
    Motd(String),
}

/// A friend's game, for mutual friends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FriendGame {
    pub game_id: u32,
    pub game_name: String,
    /// 0, or why the selected character can't join (as
    /// [`GameInfo::joinable`]).
    pub joinable: u16,
}

/// A row of the friends or ignore list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FriendRow {
    /// Their last character.
    pub who: Who,
    pub place: Where,
    /// Both added each other: the place and the game are shown.
    pub mutual: bool,
    pub game: Option<FriendGame>,
    pub last_seen_ms: u64,
    pub added_ms: u64,
}

/// [`FriendNotice::what`] values.
pub mod friend_notice {
    pub const ONLINE: u8 = 1;
    pub const OFFLINE: u8 = 2;
    pub const ENTERED_GAME: u8 = 3;
    pub const LEFT_GAME: u8 = 4;
    pub const ADDED_YOU: u8 = 5;
}

/// A friend came online, left, entered or left a game, or added you.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FriendNotice {
    /// [`friend_notice`].
    pub what: u8,
    pub who: Who,
    pub place: Where,
    pub game: Option<FriendGame>,
    pub at_ms: u64,
}

/// The two privacy choices (contract section 3.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Privacy {
    /// 0 everyone, 1 friends and guild only, 2 nobody.
    pub whispers: u8,
    /// 0 mutual friends see where I am, 1 nobody.
    pub show_where: u8,
}

/// What [`Request::FriendEdit`] does (a character name or `*account`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FriendOp {
    Add(String),
    Remove(String),
    Ignore(String),
    Unignore(String),
    Privacy(Privacy),
}

/// A ladder season ("Season 1", "Test season"; season 0 is non-ladder).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LadderSeason {
    pub season: u16,
    pub name: String,
    pub started_ms: u64,
    /// 0 while open.
    pub ended_ms: u64,
    pub test: bool,
    pub current: bool,
}

/// [`LadderQuery::split`] bits.
pub mod ladder_split {
    pub const HARDCORE: u8 = 1;
    pub const EXPANSION: u8 = 1 << 1;
}

/// [`LadderRow::flags`] bits.
pub mod ladder_flag {
    pub const DEAD: u8 = 1;
    /// A character of the asker's account.
    pub const MY_ACCOUNT: u8 = 1 << 1;
    /// The asker's selected character.
    pub const ME: u8 = 1 << 2;
    pub const ONLINE: u8 = 1 << 3;
    /// The own row pinned under a page it isn't on.
    pub const PINNED: u8 = 1 << 4;
}

/// Where a ladder page starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LadderAt {
    /// From a rank (1-based).
    Rank(u32),
    /// The page with this character (whole name, any case).
    Find(String),
    /// The page around the selected character.
    Me,
}

/// One ladder view and where the page starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LadderQuery {
    pub season: u16,
    /// [`ladder_split`] bits.
    pub split: u8,
    /// 0-6, 255 all.
    pub class: u8,
    pub at: LadderAt,
    /// 1-50.
    pub rows: u8,
}

/// A ranked character.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LadderRow {
    pub rank: u32,
    pub char: String,
    pub class: u8,
    pub level: u8,
    pub experience: u64,
    pub tag: Option<String>,
    /// [`ladder_flag`] bits.
    pub flags: u8,
}

/// One ladder page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LadderPage {
    pub season: u16,
    pub split: u8,
    pub class: u8,
    /// Characters in this view.
    pub total: u32,
    pub first_rank: u32,
    /// The selected character's rank in this view.
    pub my_rank: Option<u32>,
    /// When the ranks were computed, and the realm's clock.
    pub updated_ms: u64,
    pub now_ms: u64,
    pub rows: Vec<LadderRow>,
    /// The own row, when it is ranked off this page.
    pub pinned: Option<LadderRow>,
}

/// Which realm-lobby request an answer belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    SocialHello,
    Say,
    Whisper,
    DmConversations,
    DmHistory,
    DmRead,
    UserList,
    GuildGet,
    GuildCreate,
    GuildInvite,
    GuildAnswer,
    GuildManage,
    FriendsGet,
    FriendEdit,
    LadderSeasons,
    LadderQuery,
}

/// A realm-lobby request that changed something went through.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Done {
    pub asked: Asked,
    /// Bit 0: a whisper was stored for an offline player.
    pub flags: u8,
    /// A new DM's or guild's id.
    pub id: Option<u64>,
}

impl Done {
    /// The whisper waits for an offline player.
    pub fn stored(&self) -> bool {
        self.flags & 1 != 0
    }
}

/// What a screen asks of the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Open the connection and do the handshake with server `index` of the
    /// list.
    Connect {
        index: usize,
    },
    Register {
        account: String,
        password: String,
    },
    Login {
        account: String,
        password: String,
    },
    CharList,
    /// `ladder`: a character of the realm's current ladder season (refused
    /// with `NO_SUCH_SEASON` when none is open).
    CharCreate {
        name: String,
        class: u8,
        hardcore: bool,
        expansion: bool,
        ladder: bool,
    },
    /// `password`: the account password, to confirm.
    CharDelete {
        name: String,
        password: String,
    },
    CharSelect {
        name: String,
    },
    /// Subscribe to the game list (a snapshot, then updates).
    ListGames,
    CreateGame {
        name: String,
        password: String,
        difficulty: u8,
        /// Where it runs ([`Placement::Automatic`] on a server without game
        /// servers).
        place: Placement,
        /// Optional: 0-40 bytes shown in the list.
        description: String,
        /// Optional level range (0: no bound; the creator's level must be
        /// inside).
        level_min: u8,
        level_max: u8,
    },
    JoinGame {
        game_id: u32,
        password: String,
    },
    /// Stop the list updates (back to character select).
    LeaveLobby,
    /// The old one-room lobby chat.
    Chat {
        text: String,
    },
    /// End the session and close the connection.
    Logout,
    /// Join the chat rooms ([`room::bit`]s): after login, after a
    /// reconnect and when the selected character changes. Answered with
    /// [`Event::SocialState`], then each room's backlog.
    SocialHello {
        rooms: u16,
    },
    /// Say a line in a room (Global, Mod, Guild); `emote` is `/me`.
    Say {
        room: u8,
        emote: bool,
        text: String,
    },
    /// A DM to a character name or `*account`.
    Whisper {
        to: String,
        text: String,
    },
    /// The DM conversations, 30 a page, newest first.
    DmConversations {
        page: u16,
    },
    /// A conversation's lines before DM `before_dm` (0: the newest).
    DmHistory {
        peer: String,
        before_dm: u64,
        rows: u8,
    },
    /// Marks a conversation read up to DM `up_to_dm`.
    DmRead {
        peer_account: String,
        up_to_dm: u64,
    },
    /// A room's user list; `watch` keeps it live ([`Event::RoomPresence`]).
    UserList {
        room: u8,
        watch: bool,
    },
    GuildGet(GuildQuery),
    GuildCreate {
        tag: String,
        name: String,
    },
    GuildInvite {
        who: String,
    },
    /// Accept or decline an invite.
    GuildAnswer {
        guild: u64,
        accept: bool,
    },
    GuildManage(GuildOp),
    /// The friends list, or the ignore list.
    FriendsGet {
        ignored: bool,
    },
    FriendEdit(FriendOp),
    LadderSeasons,
    LadderQuery(LadderQuery),
}

impl Request {
    /// The realm-lobby request it is (`None`: an account or game-list
    /// request).
    pub fn asked(&self) -> Option<Asked> {
        Some(match self {
            Request::SocialHello { .. } => Asked::SocialHello,
            Request::Say { .. } => Asked::Say,
            Request::Whisper { .. } => Asked::Whisper,
            Request::DmConversations { .. } => Asked::DmConversations,
            Request::DmHistory { .. } => Asked::DmHistory,
            Request::DmRead { .. } => Asked::DmRead,
            Request::UserList { .. } => Asked::UserList,
            Request::GuildGet(_) => Asked::GuildGet,
            Request::GuildCreate { .. } => Asked::GuildCreate,
            Request::GuildInvite { .. } => Asked::GuildInvite,
            Request::GuildAnswer { .. } => Asked::GuildAnswer,
            Request::GuildManage(_) => Asked::GuildManage,
            Request::FriendsGet { .. } => Asked::FriendsGet,
            Request::FriendEdit(_) => Asked::FriendEdit,
            Request::LadderSeasons => Asked::LadderSeasons,
            Request::LadderQuery(_) => Asked::LadderQuery,
            _ => return None,
        })
    }
}

/// What the server answered or sent on its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Connected {
        server_name: String,
        registration_open: bool,
    },
    LoggedIn {
        account: String,
        max_characters: u8,
    },
    Chars(Vec<CharInfo>),
    CharCreated(CharInfo),
    CharDeleted(String),
    CharSelected(CharInfo),
    /// A list snapshot starts: the screen drops its list.
    GameListBegin,
    /// A new or changed game.
    Game(GameInfo),
    GameListEnd,
    GameGone(u32),
    /// The main server's official game servers (the whole list).
    Nodes(Vec<NodeInfo>),
    /// The client's pings to them so far (the whole table).
    Pings(PingTable),
    JoinOk(Joined),
    ChatLine {
        from: String,
        text: String,
    },
    /// A request failed; the session goes on.
    Error(Failure),
    /// The connection is gone (refused, lost, taken over, logged out).
    Disconnected(Failure),
    /// The chat facts: the answer to [`Request::SocialHello`], or a change.
    SocialState(SocialState),
    RoomLine(RoomLine),
    /// A live DM line, to or from this account (also what another session
    /// of the account sent).
    DmLine(DmLine),
    DmConversations {
        total: u32,
        page: u16,
        rows: Vec<DmConversation>,
    },
    /// A conversation's lines, oldest first; `peer` as it was asked.
    DmHistory {
        peer: String,
        lines: Vec<DmLine>,
    },
    /// A room's user list, sorted by character name.
    Users {
        room: u8,
        total: u32,
        rows: Vec<UserRow>,
    },
    RoomPresence(RoomPresence),
    /// The own guild and its members.
    Guild {
        info: GuildInfo,
        members: Vec<GuildMember>,
    },
    /// The invites to the account.
    GuildInvites(Vec<GuildInfo>),
    /// A guild by tag (public facts).
    GuildFound(GuildInfo),
    GuildNotice(GuildNotice),
    Friends {
        ignored: bool,
        rows: Vec<FriendRow>,
    },
    FriendNotice(FriendNotice),
    /// Newest first, season 0 last.
    LadderSeasons(Vec<LadderSeason>),
    Ladder(LadderPage),
    /// A realm-lobby request went through.
    Done(Done),
    /// A realm-lobby request was refused (also `NotSupported` from a realm
    /// without the area: the screens then hide its tab or button).
    Refused {
        asked: Asked,
        failure: Failure,
    },
}

/// The connection the screens talk through.
pub trait OnlineLink {
    fn send(&mut self, r: Request);
    /// The next event, if one arrived.
    fn poll(&mut self) -> Option<Event>;
    /// The realm's market on this link, for the lobby's market window
    /// (plan L18): a new one each call, reading the item records with
    /// `tips`. `None`: no market here.
    fn market(
        &mut self,
        _tips: Option<crate::ui::item_tip::ItemTips>,
    ) -> Option<Box<dyn crate::market::source::MarketSource>> {
        None
    }
}

/// No link: every request fails at once (a build without online play, or a
/// host that wired none).
#[derive(Debug, Default)]
pub struct NoLink {
    out: std::collections::VecDeque<Event>,
}

impl OnlineLink for NoLink {
    fn send(&mut self, _r: Request) {
        self.out.push_back(Event::Disconnected(Failure {
            code: ErrorCode(0),
            wait_s: 0,
            text: "Online play is not part of this build.".into(),
        }));
    }

    fn poll(&mut self) -> Option<Event> {
        self.out.pop_front()
    }
}

/// The message box text for a failure: the client's own sentence for a
/// known code (the rule the player broke, and what to do), the server's
/// text for an unknown one. Rate limits and locks say how long to wait.
pub fn failure_text(f: &Failure) -> String {
    let wait = || match f.wait_s {
        0 => String::new(),
        s if s < 60 => format!(" Try again in {s} seconds."),
        s => format!(" Try again in {} minutes.", s.div_ceil(60)),
    };
    let own = match f.code {
        ErrorCode(0) if f.text.is_empty() => "Could not reach the server.",
        ErrorCode(0) => return f.text.clone(),
        ErrorCode::WIRE_VERSION | ErrorCode::PROTOCOL | ErrorCode::TICK_RATE => {
            "This server runs another version of D2Gamma. Update the game through the launcher."
        }
        // The server's text names both versions.
        ErrorCode::BUILD if !f.text.is_empty() => return f.text.clone(),
        ErrorCode::BUILD => "This server runs another version of D2Gamma. Update the game through the launcher.",
        ErrorCode::WRONG_GAME => "This server runs another game.",
        ErrorCode::SERVER_FULL => "The server is full. Try again later.",
        ErrorCode::UNKNOWN_RESUME => "Your session has ended. Log in again.",
        ErrorCode::NOT_SUPPORTED => "The server does not support this yet.",
        ErrorCode::MALFORMED => "The connection broke (a message did not decode). Log in again.",
        ErrorCode::ACCOUNT_NAME_TAKEN => "That account name is already taken.",
        ErrorCode::BAD_ACCOUNT_NAME if !f.text.is_empty() => return f.text.clone(),
        ErrorCode::BAD_ACCOUNT_NAME => {
            "Account names are 2-15 letters, digits, - or _, and start with a letter."
        }
        ErrorCode::BAD_PASSWORD => "Passwords are 8 to 64 characters long.",
        ErrorCode::WRONG_LOGIN => "Wrong account name or password.",
        ErrorCode::RATE_LIMITED => return format!("Too many attempts.{}", wait()),
        ErrorCode::ACCOUNT_LOCKED => {
            return format!("This account is locked after too many failed logins.{}", wait())
        }
        ErrorCode::ACCOUNT_DISABLED => "This account is disabled.",
        ErrorCode::NOT_LOGGED_IN => "You are not logged in. Log in again.",
        ErrorCode::REGISTRATION_CLOSED => "This server does not take new accounts.",
        ErrorCode::SESSION_TAKEN => "Someone logged in to this account from another place.",
        ErrorCode::CHAR_NAME_TAKEN => "That character name is already taken.",
        ErrorCode::BAD_CHAR_NAME if !f.text.is_empty() => return f.text.clone(),
        ErrorCode::BAD_CHAR_NAME => "That character name is not allowed.",
        ErrorCode::CHAR_LIMIT => "This account has no free character slot.",
        ErrorCode::UNKNOWN_CHAR => "That character is not on this account.",
        ErrorCode::CHAR_IN_GAME => "That character is in a game right now.",
        ErrorCode::CHAR_MISMATCH => "The game refused the character.",
        ErrorCode::DEAD_HARDCORE => "You cannot create or join games with a dead hardcore character.",
        ErrorCode::NO_CHAR_SELECTED => "Choose a character first.",
        ErrorCode::GAME_NAME_TAKEN => "A game with that name is already open.",
        ErrorCode::BAD_GAME_NAME => "Game names are 1-15 letters, digits, spaces, - or _.",
        ErrorCode::GAME_NOT_FOUND => "That game is gone.",
        ErrorCode::GAME_FULL => "That game is full.",
        ErrorCode::PASSWORD_NEEDED => "That game needs a password.",
        ErrorCode::WRONG_PASSWORD => "Wrong game password.",
        ErrorCode::DIFFICULTY_LOCKED => "Your character has not unlocked that difficulty.",
        ErrorCode::WRONG_GAME_KIND => {
            "That game is for another kind of character (hardcore or softcore, expansion or classic)."
        }
        ErrorCode::TOO_MANY_GAMES => "The server runs as many games as it can. Try again later.",
        ErrorCode::ALREADY_IN_GAME => "You are already in a game.",
        ErrorCode::GAME_CLOSED => "The game closed.",
        ErrorCode::TICKET_INVALID => "The join expired. Join the game again.",
        node_code::NODE_UNAVAILABLE => {
            "That game server is not available. Choose another one, or Automatic."
        }
        node_code::NODE_FULL => "That game server is full. Choose another one, or Automatic.",
        node_code::NODE_DRAINING => {
            "That game server takes no new games right now. Choose another one, or Automatic."
        }
        node_code::NODE_LOST => {
            "The game server was lost. Your character is as it was at its last checkpoint."
        }
        node_code::REALM_UNAVAILABLE => "The server is stopping. Try again in a few minutes.",
        node_code::HOST_UNREACHABLE => {
            "Could not reach the PC that hosts this game. Try another game."
        }
        node_code::HOSTING_OFF => "Hosting games on your PC is not available on this server.",
        lobby_code::LEVEL_RANGE => "Your level is outside that game's level range.",
        lobby_code::NOT_IN_GUILD => "You are not in a guild.",
        lobby_code::GUILD_RANK => "Your guild rank doesn't allow that.",
        lobby_code::GUILD_TAG_TAKEN => "That guild tag is taken.",
        // The realm's text says which rule.
        lobby_code::BAD_GUILD_NAME if !f.text.is_empty() => return f.text.clone(),
        lobby_code::BAD_GUILD_NAME => "That guild tag or name isn't allowed.",
        lobby_code::ALREADY_IN_GUILD => "You (or they) are in a guild already.",
        lobby_code::GUILD_FULL => "The guild is full.",
        lobby_code::UNKNOWN_PLAYER => "No such player.",
        lobby_code::INBOX_FULL => "Their message box is full.",
        lobby_code::WHISPER_BLOCKED => "That player doesn't take whispers from you.",
        lobby_code::FRIEND_LIMIT => "Your friends (or ignore) list is full.",
        lobby_code::MUTED => return format!("You are muted.{}", wait()),
        lobby_code::ROOM_UNAVAILABLE => "That chat isn't available.",
        lobby_code::NO_SUCH_SEASON => "There is no such ladder season.",
        lobby_code::BAD_TEXT => "That message isn't allowed.",
        lobby_code::INVITE_GONE => "That invite expired or was withdrawn.",
        _ if !f.text.is_empty() => return f.text.clone(),
        _ => return format!("The server refused (error {}).", f.code.0),
    };
    own.to_string()
}

/// A ping as the lists show it: `23 ms`, `...` while measuring, `-` when
/// no ping was answered.
pub fn ping_text(p: Option<Option<u32>>) -> String {
    match p {
        Some(Some(ms)) => format!("{ms} ms"),
        Some(None) => "-".into(),
        None => "...".into(),
    }
}

/// A difficulty's name as the lobby shows it.
pub fn difficulty_name(d: u8) -> &'static str {
    match d {
        0 => "Normal",
        1 => "Nightmare",
        _ => "Hell",
    }
}
