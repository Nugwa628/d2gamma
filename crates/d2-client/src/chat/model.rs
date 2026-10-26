//! The chat's state, shared by the lobby and the game: the tabs (Game in a
//! game; Global, Mod, Guild with a guild; DM), a ring of lines and an unread
//! count per tab, the DM conversations, the user list of the open room, the
//! typed line and its history. Hosts feed it from a [`ChatSource`]
//! ([`ChatModel::pump`]) and draw it with `ui::chat`.
//!
//! Rules (lobby contract section 3 and `lobby-ui.md` L9):
//! - A line counts as unread while its tab is not the one in view (the open
//!   tab of a window the player sees, [`ChatModel::set_seen`]); opening the
//!   tab clears it. The backlog the realm sends after the hello (lines older
//!   than its `now_ms`) doesn't count. Game messages (quests, "joined the
//!   game") never count; the game's player lines do.
//! - DM unread is per conversation; the DM tab's count is their sum (or the
//!   realm's `dm_unread` while the conversations are not listed yet). A
//!   conversation in view is marked read on the realm (`DmRead`) up to its
//!   newest line; one with unread lines but none loaded gets its history
//!   first.
//! - Market notices go to the Mod tab and to the tab in view.
//! - Commands: [`super::commands`]; an unknown one stays local.

use std::collections::{BTreeSet, VecDeque};

use super::commands::{parse, Command};
use super::source::{
    code, done_text, kind, ChatIn, ChatOut, ChatSource, DmConversation, DmLine, FriendOp, LineWhat,
    PresenceOp, Room, RoomLine, SocialState, UserRow, Who,
};

/// Lines kept per tab.
pub const TAB_KEEP: usize = 500;
/// Typed lines kept for Up / Down.
pub const HISTORY_KEEP: usize = 32;
/// A player's left and joined lines this close together cancel out.
pub const PRESENCE_PAIR_MS: u64 = 60_000;
/// Rows asked per DM history page.
pub const DM_PAGE_ROWS: u8 = 50;

/// A chat tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tab {
    Room(Room),
    Dm,
}

/// Which host shows the chat: the game adds the Game tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Lobby,
    Game,
}

/// What a line is, for its colours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Said,
    Emote,
    Joined,
    Left,
    /// A realm notice.
    Notice,
    /// A guild event.
    Guild,
    /// A market notice.
    Market,
    /// The client's own note (a refusal, a command's usage).
    System,
    /// A game message in the game's text colour.
    Game(u8),
    /// A DM from the peer.
    DmIn,
    /// A DM to the peer.
    DmOut,
}

/// One line of a tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: LineKind,
    /// The speaker (a DM's peer); empty for system and game messages.
    pub who: Who,
    pub text: String,
    pub at_ms: u64,
    /// A DM's id and peer account (0 and empty otherwise).
    pub dm: u64,
    pub peer_account: String,
    /// A DM that waited for an offline player.
    pub stored: bool,
}

impl Line {
    fn plain(kind: LineKind, text: impl Into<String>, at_ms: u64) -> Line {
        Line {
            kind,
            who: Who::default(),
            text: text.into(),
            at_ms,
            dm: 0,
            peer_account: String::new(),
            stored: false,
        }
    }

    fn dm(d: &DmLine) -> Line {
        Line {
            kind: if d.outgoing {
                LineKind::DmOut
            } else {
                LineKind::DmIn
            },
            who: Who {
                account: d.peer_account.clone(),
                ..Who::named(&d.peer_char)
            },
            text: d.text.clone(),
            at_ms: d.at_ms,
            dm: d.dm,
            peer_account: d.peer_account.clone(),
            stored: d.stored,
        }
    }
}

/// A DM conversation as the DM tab lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conversation {
    pub row: DmConversation,
    /// The newest DM id known (0: no line loaded).
    pub newest_dm: u64,
    history_asked: bool,
}

/// The user list of the open room.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserList {
    pub room: Option<Room>,
    /// Sorted by character name (any case).
    pub rows: Vec<UserRow>,
    /// Members now (may be more than the rows).
    pub count: u16,
    /// The list arrived.
    pub loaded: bool,
}

pub struct ChatModel {
    host: Host,
    state: Option<SocialState>,
    /// `now_ms` of the state that answered the hello: older room lines are
    /// the backlog.
    hello_ms: Option<u64>,
    open: Tab,
    seen: bool,
    rooms: [VecDeque<Line>; 4],
    unread: [u16; 4],
    dm_lines: VecDeque<Line>,
    pub conversations: Vec<Conversation>,
    conversations_asked: bool,
    /// The conversation the DM tab shows (a peer account); `None`: all.
    pub open_dm: Option<String>,
    /// The realm's DM unread count, less what was read here since.
    realm_dm_unread: u16,
    pub users: UserList,
    /// The account that whispered last (`/r`).
    pub last_whisper_from: Option<String>,
    /// Request kinds the realm answered `NotSupported`: their tab or column
    /// hides.
    unsupported: BTreeSet<u8>,
    pending_friend: VecDeque<(FriendOp, String)>,
    /// The typed line.
    pub input: String,
    history: VecDeque<String>,
    history_at: Option<usize>,
    /// Rows scrolled back from the newest.
    pub scroll: usize,
    out: Vec<ChatOut>,
    now_ms: u64,
    /// The player's selected character ([`ChatModel::set_me`]).
    me: Option<String>,
}

impl ChatModel {
    /// A model for `host`; its first send is the hello for the host's realm
    /// rooms.
    pub fn new(host: Host) -> ChatModel {
        let mut m = ChatModel {
            host,
            state: None,
            hello_ms: None,
            open: match host {
                Host::Lobby => Tab::Room(Room::Mod),
                Host::Game => Tab::Room(Room::Game),
            },
            seen: true,
            rooms: Default::default(),
            unread: [0; 4],
            dm_lines: VecDeque::new(),
            conversations: Vec::new(),
            conversations_asked: false,
            open_dm: None,
            realm_dm_unread: 0,
            users: UserList::default(),
            last_whisper_from: None,
            unsupported: BTreeSet::new(),
            pending_friend: VecDeque::new(),
            input: String::new(),
            history: VecDeque::new(),
            history_at: None,
            scroll: 0,
            out: Vec::new(),
            now_ms: 0,
            me: None,
        };
        m.hello();
        m
    }

    /// Says hello again (a reconnect, another selected character): the
    /// realm answers with its state and the rooms' backlog.
    pub fn hello(&mut self) {
        self.hello_ms = None;
        self.out.push(ChatOut::Hello {
            rooms: Room::Global.bit() | Room::Mod.bit() | Room::Guild.bit(),
        });
    }

    pub fn host(&self) -> Host {
        self.host
    }

    pub fn state(&self) -> Option<&SocialState> {
        self.state.as_ref()
    }

    pub fn open_tab_now(&self) -> Tab {
        self.open
    }

    /// Takes what is queued for the source.
    pub fn take_out(&mut self) -> Vec<ChatOut> {
        std::mem::take(&mut self.out)
    }

    /// Sends what is queued and applies everything that arrived; `true`
    /// when something did.
    pub fn pump(&mut self, src: &mut dyn ChatSource, now_ms: u64) -> bool {
        self.now_ms = now_ms;
        let mut changed = false;
        loop {
            for o in self.take_out() {
                src.send(o);
            }
            let Some(m) = src.poll() else { break };
            self.apply(m);
            changed = true;
        }
        for o in self.take_out() {
            src.send(o);
        }
        changed
    }

    fn granted(&self, room: Room) -> bool {
        match room {
            Room::Game => self.host == Host::Game,
            Room::Guild => self
                .state
                .as_ref()
                .is_some_and(|s| s.rooms & room.bit() != 0 && s.guild.is_some()),
            _ => self
                .state
                .as_ref()
                .is_some_and(|s| s.rooms & room.bit() != 0),
        }
    }

    fn dm_ok(&self) -> bool {
        self.state.is_some()
            && !self.unsupported.contains(&kind::WHISPER)
            && !self.unsupported.contains(&kind::DM_CONVERSATIONS)
    }

    /// The realm lists room users.
    pub fn users_ok(&self) -> bool {
        self.state.is_some() && !self.unsupported.contains(&kind::USER_LIST)
    }

    /// The tabs shown, in order: Game (in a game), Global, Mod, Guild, DM.
    pub fn tabs(&self) -> Vec<Tab> {
        let mut v: Vec<Tab> = [Room::Game, Room::Global, Room::Mod, Room::Guild]
            .into_iter()
            .filter(|r| self.granted(*r))
            .map(Tab::Room)
            .collect();
        if self.dm_ok() {
            v.push(Tab::Dm);
        }
        v
    }

    /// The tab's label.
    pub fn tab_label(&self, t: Tab) -> String {
        match t {
            Tab::Room(Room::Global) => "Global".into(),
            Tab::Room(Room::Mod) => match self.state.as_ref().map(|s| s.mod_label.as_str()) {
                Some(l) if !l.is_empty() => format!("Mod: {l}"),
                _ => "Chat".into(),
            },
            Tab::Room(Room::Guild) => match self.state.as_ref().and_then(|s| s.guild.as_ref()) {
                Some(g) => format!("Guild: {}", g.tag),
                None => "Guild".into(),
            },
            Tab::Room(Room::Game) => "Game".into(),
            Tab::Dm => "DM".into(),
        }
    }

    /// The tab's short label, for a strip too narrow for the full ones.
    pub fn tab_short_label(&self, t: Tab) -> String {
        match t {
            Tab::Room(Room::Mod) => match self.state.as_ref().map(|s| s.mod_label.as_str()) {
                Some(l) if !l.is_empty() => "Mod".into(),
                _ => "Chat".into(),
            },
            Tab::Room(Room::Guild) => "Guild".into(),
            _ => self.tab_label(t),
        }
    }

    /// The player's selected character: its own joined and left lines
    /// (its reconnects, its lobby entries) are not shown.
    pub fn set_me(&mut self, character: Option<&str>) {
        self.me = character.map(str::to_string);
    }

    /// Unread lines of a tab.
    pub fn unread(&self, t: Tab) -> u16 {
        match t {
            Tab::Room(r) => self.unread[r.number() as usize],
            Tab::Dm => {
                let sum = self
                    .conversations
                    .iter()
                    .fold(0u16, |a, c| a.saturating_add(c.row.unread));
                sum.max(self.realm_dm_unread)
            }
        }
    }

    /// Whether the player sees the open tab (a collapsed or hidden window
    /// doesn't): unread counts follow.
    pub fn set_seen(&mut self, seen: bool) {
        self.seen = seen;
        if seen {
            self.clear_open();
        }
    }

    fn viewing(&self, t: Tab) -> bool {
        self.seen && self.open == t
    }

    fn clear_open(&mut self) {
        match self.open {
            Tab::Room(r) => self.unread[r.number() as usize] = 0,
            Tab::Dm => self.read_viewed(),
        }
    }

    /// Opens a tab: its unread clears, the room's user list is watched.
    pub fn open_tab(&mut self, t: Tab) {
        if !self.tabs().contains(&t) {
            return;
        }
        self.scroll = 0;
        let was = self.open;
        self.open = t;
        match t {
            Tab::Room(r) if r != Room::Game && self.users_ok() => {
                if self.users.room != Some(r) {
                    self.users = UserList {
                        room: Some(r),
                        ..UserList::default()
                    };
                    self.out.push(ChatOut::Users {
                        room: r,
                        watch: true,
                    });
                }
            }
            _ => {
                if let (Some(r), true) = (self.users.room, was != t) {
                    self.out.push(ChatOut::Users {
                        room: r,
                        watch: false,
                    });
                    self.users = UserList::default();
                }
            }
        }
        if t == Tab::Dm && !self.conversations_asked && self.dm_ok() {
            self.conversations_asked = true;
            self.out.push(ChatOut::Conversations { page: 0 });
        }
        self.clear_open();
    }

    /// Shows one DM conversation (`None`: all of them).
    pub fn open_conversation(&mut self, peer_account: Option<&str>) {
        self.open_dm = peer_account.map(str::to_string);
        if self.open != Tab::Dm {
            self.open_tab(Tab::Dm);
        }
        self.scroll = 0;
        if let Some(p) = peer_account {
            if let Some(c) = self
                .conversations
                .iter_mut()
                .find(|c| c.row.peer_account == p)
            {
                if !c.history_asked {
                    c.history_asked = true;
                    self.out.push(ChatOut::History {
                        peer: format!("*{p}"),
                        before_dm: 0,
                        rows: DM_PAGE_ROWS,
                    });
                }
            }
        }
        self.read_viewed();
    }

    /// Marks the conversations in view read on the realm; one with unread
    /// lines but none loaded gets its history first.
    fn read_viewed(&mut self) {
        if !self.viewing(Tab::Dm) {
            return;
        }
        let only = self.open_dm.clone();
        let mut cleared = 0u16;
        for c in &mut self.conversations {
            if c.row.unread == 0 || only.as_ref().is_some_and(|p| *p != c.row.peer_account) {
                continue;
            }
            if c.newest_dm == 0 {
                if !c.history_asked {
                    c.history_asked = true;
                    self.out.push(ChatOut::History {
                        peer: format!("*{}", c.row.peer_account),
                        before_dm: 0,
                        rows: DM_PAGE_ROWS,
                    });
                }
                continue;
            }
            self.out.push(ChatOut::Read {
                peer_account: c.row.peer_account.clone(),
                up_to_dm: c.newest_dm,
            });
            cleared = cleared.saturating_add(c.row.unread);
            c.row.unread = 0;
        }
        self.realm_dm_unread = self.realm_dm_unread.saturating_sub(cleared);
    }

    /// The lines of a tab, oldest first (the DM tab: the open conversation,
    /// or all of them, and the client's notes).
    pub fn lines(&self, t: Tab) -> Vec<&Line> {
        match t {
            Tab::Room(r) => self.rooms[r.number() as usize].iter().collect(),
            Tab::Dm => self
                .dm_lines
                .iter()
                .filter(|l| match &self.open_dm {
                    Some(p) => l.peer_account.is_empty() || l.peer_account == *p,
                    None => true,
                })
                .collect(),
        }
    }

    fn push_room(&mut self, r: Room, line: Line, counts: bool) {
        let ring = &mut self.rooms[r.number() as usize];
        ring.push_back(line);
        while ring.len() > TAB_KEEP {
            ring.pop_front();
        }
        if counts && !self.viewing(Tab::Room(r)) {
            let u = &mut self.unread[r.number() as usize];
            *u = u.saturating_add(1);
        }
    }

    /// A DM line in id order (a repeat of a known id is dropped); the
    /// client's notes (id 0) go last.
    fn push_dm(&mut self, line: Line) -> bool {
        if line.dm != 0 {
            if self.dm_lines.iter().any(|l| l.dm == line.dm) {
                return false;
            }
            let at = self
                .dm_lines
                .iter()
                .position(|l| l.dm > line.dm)
                .unwrap_or(self.dm_lines.len());
            self.dm_lines.insert(at, line);
        } else {
            self.dm_lines.push_back(line);
        }
        while self.dm_lines.len() > TAB_KEEP {
            self.dm_lines.pop_front();
        }
        true
    }

    /// A note of the client's in the tab in view.
    pub fn note(&mut self, text: impl Into<String>) {
        let line = Line::plain(LineKind::System, text, self.now_ms);
        match self.open {
            Tab::Room(r) => self.push_room(r, line, false),
            Tab::Dm => {
                self.push_dm(line);
            }
        }
    }

    fn conversation(&mut self, peer_account: &str) -> &mut Conversation {
        let i = match self
            .conversations
            .iter()
            .position(|c| c.row.peer_account == peer_account)
        {
            Some(i) => i,
            None => {
                self.conversations.insert(
                    0,
                    Conversation {
                        row: DmConversation {
                            peer_account: peer_account.to_string(),
                            ..DmConversation::default()
                        },
                        ..Conversation::default()
                    },
                );
                0
            }
        };
        &mut self.conversations[i]
    }

    /// Applies one message from the source.
    pub fn apply(&mut self, m: ChatIn) {
        match m {
            ChatIn::State(s) => {
                let first = self.state.is_none();
                if self.hello_ms.is_none() {
                    self.hello_ms = Some(s.now_ms);
                }
                self.realm_dm_unread = s.dm_unread;
                self.state = Some(s);
                if first && self.dm_ok() && !self.conversations_asked {
                    self.conversations_asked = true;
                    self.out.push(ChatOut::Conversations { page: 0 });
                }
                // A tab that went away (a guild left) gives way to the first.
                let tabs = self.tabs();
                if !tabs.contains(&self.open) {
                    if let Some(t) = tabs.first().copied() {
                        self.open_tab(t);
                    }
                } else if first {
                    self.open_tab(self.open);
                }
            }
            ChatIn::Line(l) => self.room_line(l),
            ChatIn::Dm(d) => {
                let line = Line::dm(&d);
                if !self.push_dm(line) {
                    return;
                }
                let viewing = self.viewing(Tab::Dm)
                    && self.open_dm.as_ref().is_none_or(|p| *p == d.peer_account);
                if !d.outgoing {
                    self.last_whisper_from = Some(d.peer_account.clone());
                }
                let c = self.conversation(&d.peer_account);
                c.row.peer_char = d.peer_char.clone();
                c.row.last_ms = d.at_ms;
                c.row.last_text = d.text.clone();
                c.newest_dm = c.newest_dm.max(d.dm);
                if !d.outgoing {
                    c.row.unread = c.row.unread.saturating_add(1);
                }
                // Newest conversation first.
                let i = self
                    .conversations
                    .iter()
                    .position(|c| c.row.peer_account == d.peer_account)
                    .unwrap_or(0);
                let c = self.conversations.remove(i);
                self.conversations.insert(0, c);
                if viewing {
                    self.read_viewed();
                }
            }
            ChatIn::Conversations(rows) => {
                for r in rows {
                    let c = self.conversation(&r.peer_account);
                    c.row = r;
                }
                self.conversations
                    .sort_by_key(|c| std::cmp::Reverse(c.row.last_ms));
                self.read_viewed();
            }
            ChatIn::History {
                peer_account,
                lines,
            } => {
                let newest = lines.iter().map(|d| d.dm).max().unwrap_or(0);
                for d in &lines {
                    self.push_dm(Line::dm(d));
                }
                let c = self.conversation(&peer_account);
                c.history_asked = true;
                c.newest_dm = c.newest_dm.max(newest);
                if let Some(d) = lines.last() {
                    if c.row.peer_char.is_empty() {
                        c.row.peer_char = d.peer_char.clone();
                    }
                }
                self.read_viewed();
            }
            ChatIn::Users { room, rows } if self.users.room == Some(room) => {
                let mut rows = rows;
                rows.sort_by_key(|r| r.who.char.to_lowercase());
                self.users.count = rows.len().try_into().unwrap_or(u16::MAX);
                self.users.rows = rows;
                self.users.loaded = true;
            }
            ChatIn::Users { .. } => {}
            ChatIn::Presence {
                room,
                op,
                row,
                count,
            } if self.users.room == Some(room) => {
                let key = row.who.char.to_lowercase();
                self.users
                    .rows
                    .retain(|r| !r.who.char.eq_ignore_ascii_case(&row.who.char));
                if op != PresenceOp::Left {
                    let at = self
                        .users
                        .rows
                        .iter()
                        .position(|r| r.who.char.to_lowercase() > key)
                        .unwrap_or(self.users.rows.len());
                    self.users.rows.insert(at, row);
                }
                self.users.count = count;
            }
            ChatIn::Presence { .. } => {}
            ChatIn::Done(d) => self.done(d),
            ChatIn::MarketNotice { text, at_ms } => {
                let line = Line::plain(LineKind::Market, text, at_ms);
                if self.open != Tab::Room(Room::Mod) {
                    match self.open {
                        Tab::Room(r) => self.push_room(r, line.clone(), false),
                        Tab::Dm => {
                            self.push_dm(line.clone());
                        }
                    }
                }
                if self.granted(Room::Mod) {
                    self.push_room(Room::Mod, line, true);
                }
            }
            ChatIn::GameLine { from, text, at_ms } => {
                let line = Line {
                    who: Who::named(&from),
                    ..Line::plain(LineKind::Said, text, at_ms)
                };
                self.push_room(Room::Game, line, true);
            }
            ChatIn::GameMessage {
                text,
                colour,
                at_ms,
            } => {
                let line = Line::plain(LineKind::Game(colour), text, at_ms);
                self.push_room(Room::Game, line, false);
            }
        }
    }

    fn room_line(&mut self, l: RoomLine) {
        if !self.granted(l.room) && l.room != Room::Game {
            return;
        }
        let kind = match l.what {
            LineWhat::Said => LineKind::Said,
            LineWhat::Emote => LineKind::Emote,
            LineWhat::Joined => LineKind::Joined,
            LineWhat::Left => LineKind::Left,
            LineWhat::Notice => LineKind::Notice,
            LineWhat::GuildEvent => LineKind::Guild,
        };
        if matches!(kind, LineKind::Joined | LineKind::Left)
            && !self.presence_news(l.room, &l.who, kind, l.at_ms)
        {
            return;
        }
        // The backlog, and joined / left lines, don't count.
        let counts = !matches!(kind, LineKind::Joined | LineKind::Left)
            && self.hello_ms.is_some_and(|h| l.at_ms >= h);
        let line = Line {
            who: l.who,
            ..Line::plain(kind, l.text, l.at_ms)
        };
        self.push_room(l.room, line, counts);
    }

    /// Whether a joined or left line of `who` in room `r` at `at_ms` is
    /// shown: never the player's own; and a player who left and came back
    /// (or came and went) within [`PRESENCE_PAIR_MS`] shows neither line,
    /// a repeat of the same line in that time once.
    fn presence_news(&mut self, r: Room, who: &Who, kind: LineKind, at_ms: u64) -> bool {
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        if self.me.as_deref().is_some_and(|me| same(me, &who.char)) {
            return false;
        }
        let ring = &mut self.rooms[r.number() as usize];
        let earlier = ring.iter().rposition(|l| {
            matches!(l.kind, LineKind::Joined | LineKind::Left)
                && same(&l.who.char, &who.char)
                && at_ms.saturating_sub(l.at_ms) <= PRESENCE_PAIR_MS
        });
        match earlier {
            Some(i) if ring[i].kind != kind => {
                ring.remove(i);
                false
            }
            Some(_) => false,
            None => true,
        }
    }

    fn done(&mut self, d: super::source::Done) {
        if d.code == code::NOT_SUPPORTED {
            self.unsupported.insert(d.request);
            if d.request == kind::USER_LIST {
                self.users = UserList::default();
            }
            if !self.tabs().contains(&self.open) {
                if let Some(t) = self.tabs().first().copied() {
                    self.open_tab(t);
                }
            }
        }
        if d.request == kind::FRIEND_EDIT {
            let (op, name) = self
                .pending_friend
                .pop_front()
                .unwrap_or((FriendOp::Add, String::new()));
            if d.code == 0 {
                self.note(match op {
                    FriendOp::Add => format!("{name} is on your friends list."),
                    FriendOp::Remove => format!("{name} is off your friends list."),
                    FriendOp::Ignore => format!("You ignore {name}."),
                    FriendOp::Unignore => format!("You no longer ignore {name}."),
                });
                return;
            }
        }
        if d.code == 0 {
            if d.request == kind::WHISPER && d.stored {
                self.note("They are offline: the message waits for them on the realm.");
            }
            return;
        }
        // Lists asked in the background fail quietly.
        let quiet = matches!(
            d.request,
            kind::USER_LIST | kind::DM_CONVERSATIONS | kind::DM_HISTORY | kind::DM_READ
        ) || d.request == kind::SOCIAL_HELLO;
        if !quiet {
            self.note(done_text(&d));
        }
    }

    /// Sends the typed line (a command or a line for the open tab).
    pub fn submit(&mut self) {
        let typed = std::mem::take(&mut self.input);
        self.history_at = None;
        self.scroll = 0;
        if typed.trim().is_empty() {
            return;
        }
        if self.history.back() != Some(&typed) {
            self.history.push_back(typed.clone());
            while self.history.len() > HISTORY_KEEP {
                self.history.pop_front();
            }
        }
        match parse(&typed) {
            Command::Empty => {}
            Command::Plain(text) => self.in_open_tab(false, text),
            Command::Emote(text) => self.in_open_tab(true, text),
            Command::Say(room, text) => self.say(room, false, text),
            Command::Whisper { to, text } => self.whisper(to, text),
            Command::Reply(text) => match self.last_whisper_from.clone() {
                Some(a) => self.whisper(format!("*{a}"), text),
                None => self.note("Nobody has whispered you yet."),
            },
            Command::Friend(op, who) => {
                if self.state.is_none() || self.unsupported.contains(&kind::FRIEND_EDIT) {
                    self.note("The realm doesn't support that yet.");
                    return;
                }
                self.pending_friend.push_back((op, who.clone()));
                self.out.push(ChatOut::Friend { op, who });
            }
            Command::Usage(u) => self.note(format!("Use: {u}")),
            Command::Unknown(w) => self.note(format!("Unknown command: {w}")),
        }
    }

    fn in_open_tab(&mut self, emote: bool, text: String) {
        match self.open {
            Tab::Room(r) => self.say(r, emote, text),
            Tab::Dm if emote => self.note("/me works in a room."),
            Tab::Dm => match self.open_dm.clone() {
                Some(p) => self.whisper(format!("*{p}"), text),
                None => self.note("Choose a conversation, or type /w NAME TEXT."),
            },
        }
    }

    fn say(&mut self, room: Room, emote: bool, text: String) {
        if !self.granted(room) {
            let d = super::source::Done {
                code: if room == Room::Guild && self.state.is_some() {
                    code::NOT_IN_GUILD
                } else {
                    code::ROOM_UNAVAILABLE
                },
                ..Default::default()
            };
            self.note(done_text(&d));
            return;
        }
        self.out.push(ChatOut::Say { room, emote, text });
    }

    fn whisper(&mut self, to: String, text: String) {
        if !self.dm_ok() {
            self.note("The realm doesn't support whispers yet.");
            return;
        }
        self.out.push(ChatOut::Whisper { to, text });
    }

    /// Up: the previous typed line.
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let at = match self.history_at {
            Some(i) => i.saturating_sub(1),
            None => self.history.len() - 1,
        };
        self.history_at = Some(at);
        self.input = self.history[at].clone();
    }

    /// Down: the next typed line (past the newest: an empty line).
    pub fn history_next(&mut self) {
        let Some(i) = self.history_at else { return };
        if i + 1 < self.history.len() {
            self.history_at = Some(i + 1);
            self.input = self.history[i + 1].clone();
        } else {
            self.history_at = None;
            self.input.clear();
        }
    }

    /// Scrolls the open tab by `rows` (positive: back to older lines); the
    /// view clamps it.
    pub fn scroll_by(&mut self, rows: i32) {
        self.scroll = (self.scroll as i64 + i64::from(rows)).max(0) as usize;
    }

    /// Fills the input with a whisper to `name` (a click on a user).
    pub fn whisper_to(&mut self, name: &str) {
        self.input = format!("/w {name} ");
    }
}
