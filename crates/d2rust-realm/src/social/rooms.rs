//! The chat rooms (lobby contract 3.1): Global (every session that asked
//! for it), Mod (the realm's mod game) and each guild's room.
//!
//! - A session is in its rooms from its `SocialHello` until it ends, in
//!   the lobby and in a game alike; a hello again (a new character, a
//!   reconnect) changes only what it asked for.
//! - A room keeps its last [`RING`] lines in memory; a hello gets the last
//!   [`BACKLOG`] of each granted room.
//! - Members are accounts with a selected character. Joined and left lines
//!   go to rooms of at most `chat_join_lines_max` members; the watched
//!   user lists get [`RoomPresence`] for every member who joins, leaves or
//!   changes (level, guild, in a game).
//! - An account never gets lines from the accounts it ignores.
//! - Sessions that never sent a hello (clients before the chat tabs) get
//!   the Mod room's lines as the old lobby chat line (0x4A).
//!
//! No I/O but the sends, no clock: the caller passes the time.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use d2_net::msg::lobby::LobbyEvent;
use d2_net::msg::social::{
    line_what, place, presence_op, room, show_where, who_flags, FromRealm, GuildBrief, RoomLine,
    RoomPresence, Who,
};

use super::presence::{Here, Presence};
use crate::session::SessionId;

/// Lines a room keeps.
pub const RING: usize = 50;
/// Lines a hello gets of each room.
pub const BACKLOG: usize = 20;

/// The rooms the realm serves (the others are reserved).
pub const SERVED: u16 = room::bit(room::GLOBAL) | room::bit(room::MOD) | room::bit(room::GUILD);

/// One room.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoomKey {
    Global,
    Mod,
    Guild(u64),
}

impl RoomKey {
    pub fn number(self) -> u8 {
        match self {
            RoomKey::Global => room::GLOBAL,
            RoomKey::Mod => room::MOD,
            RoomKey::Guild(_) => room::GUILD,
        }
    }
}

/// What the rooms keep of an online account (read from the store at its
/// hello and when it changes).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountView {
    pub guild: Option<GuildBrief>,
    /// The accounts it ignores.
    pub ignores: BTreeSet<i64>,
    /// `social::whispers`.
    pub whispers: u8,
    /// `social::show_where`.
    pub show_where: u8,
}

#[derive(Debug, Default)]
struct Room {
    /// The last line's number.
    last: u64,
    ring: VecDeque<RoomLine>,
    /// Member accounts and their sessions in the room.
    members: BTreeMap<i64, BTreeSet<SessionId>>,
    /// The members as last told to watchers.
    shown: BTreeMap<i64, (Who, u8)>,
}

#[derive(Debug, Default)]
struct Joined {
    asked: u16,
    rooms: BTreeSet<RoomKey>,
    watch: Option<RoomKey>,
}

/// Every room and who is in it.
#[derive(Debug, Default)]
pub struct Rooms {
    rooms: BTreeMap<RoomKey, Room>,
    sessions: BTreeMap<SessionId, Joined>,
    views: BTreeMap<i64, AccountView>,
    /// `chat_join_lines_max`.
    join_lines_max: usize,
}

/// A room's line as the old lobby chat's (0x4A).
fn legacy_line(l: &RoomLine) -> Vec<u8> {
    LobbyEvent::ChatLine {
        from: l.who.char.clone(),
        text: l.text.clone(),
        at: l.at_ms / 1000,
    }
    .body()
}

impl Rooms {
    pub fn new(join_lines_max: u16) -> Self {
        Self {
            join_lines_max: usize::from(join_lines_max),
            ..Self::default()
        }
    }

    /// What the rooms hold, for the realm's `stats` line: `rooms` rooms,
    /// `members` sessions in them (a session counts once per room),
    /// `shown` members as last told to watchers, `lines` kept, `hello`
    /// sessions that said hello, `views` online accounts' views and
    /// `watching` sessions that watch a user list.
    pub fn stats(&self) -> String {
        let members: usize = self
            .rooms
            .values()
            .map(|r| r.members.values().map(BTreeSet::len).sum::<usize>())
            .sum();
        let shown: usize = self.rooms.values().map(|r| r.shown.len()).sum();
        let lines: usize = self.rooms.values().map(|r| r.ring.len()).sum();
        let watching = self.sessions.values().filter(|j| j.watch.is_some()).count();
        format!(
            "rooms={} members={members} shown={shown} lines={lines} hello={} views={} watching={watching}",
            self.rooms.len(),
            self.sessions.len(),
            self.views.len(),
        )
    }

    /// The account's view.
    pub fn view(&self, account: i64) -> Option<&AccountView> {
        self.views.get(&account)
    }

    /// Whether the session sent a hello.
    pub fn said_hello(&self, session: SessionId) -> bool {
        self.sessions.contains_key(&session)
    }

    /// The rooms (bits) the session is in.
    pub fn granted(&self, session: SessionId) -> u16 {
        self.sessions.get(&session).map_or(0, |j| {
            j.rooms.iter().fold(0, |b, k| b | room::bit(k.number()))
        })
    }

    /// The room `number` of the session's account: Guild only with a
    /// guild.
    pub fn key_of(&self, account: i64, number: u8) -> Option<RoomKey> {
        match number {
            room::GLOBAL => Some(RoomKey::Global),
            room::MOD => Some(RoomKey::Mod),
            room::GUILD => self
                .views
                .get(&account)
                .and_then(|v| v.guild.as_ref())
                .map(|g| RoomKey::Guild(g.guild)),
            _ => None,
        }
    }

    /// The session's room `number`, if it is in it.
    pub fn joined_key(&self, session: SessionId, account: i64, number: u8) -> Option<RoomKey> {
        let key = self.key_of(account, number)?;
        self.sessions
            .get(&session)
            .filter(|j| j.rooms.contains(&key))
            .map(|_| key)
    }

    /// The rooms (bits) a session of `view`'s account asking for `asked`
    /// is granted: the served ones, Guild only with a guild.
    pub fn grant(asked: u16, view: &AccountView) -> u16 {
        let mut g = asked & SERVED;
        if view.guild.is_none() {
            g &= !room::bit(room::GUILD);
        }
        g
    }

    /// A `SocialHello` asking for `asked`: `state` (the answer) goes to the
    /// session, then the backlog of each room it gets (oldest line first
    /// per room), then it is in those rooms (and out of the others).
    pub fn hello(
        &mut self,
        p: &Presence,
        here: &Here,
        asked: u16,
        view: AccountView,
        state: Vec<u8>,
        at_ms: u64,
    ) {
        let account = here.account.0;
        self.views.insert(account, view);
        here.out.send(state);
        for n in [room::GLOBAL, room::MOD, room::GUILD] {
            if asked & room::bit(n) == 0 {
                continue;
            }
            if let Some(r) = self.key_of(account, n).and_then(|k| self.rooms.get(&k)) {
                let skip = r.ring.len().saturating_sub(BACKLOG);
                for l in r.ring.iter().skip(skip) {
                    here.out.send(FromRealm::RoomLine(l.clone()).body());
                }
            }
        }
        self.sessions.entry(here.session).or_default().asked = asked;
        self.place_session(p, here.session, account, at_ms);
    }

    /// Puts the session in the rooms it asked for and may be in (its
    /// guild's), out of the others.
    fn place_session(&mut self, p: &Presence, session: SessionId, account: i64, at_ms: u64) {
        let Some(asked) = self.sessions.get(&session).map(|j| j.asked) else {
            return;
        };
        let want: BTreeSet<RoomKey> = [room::GLOBAL, room::MOD, room::GUILD]
            .into_iter()
            .filter(|n| asked & room::bit(*n) != 0)
            .filter_map(|n| self.key_of(account, n))
            .collect();
        let had = self
            .sessions
            .get(&session)
            .map(|j| j.rooms.clone())
            .unwrap_or_default();
        for k in had.difference(&want) {
            if let Some(r) = self.rooms.get_mut(k) {
                if let Some(s) = r.members.get_mut(&account) {
                    s.remove(&session);
                    if s.is_empty() {
                        r.members.remove(&account);
                    }
                }
            }
        }
        for k in want.difference(&had) {
            self.rooms
                .entry(*k)
                .or_default()
                .members
                .entry(account)
                .or_default()
                .insert(session);
        }
        if let Some(j) = self.sessions.get_mut(&session) {
            if j.watch.is_some_and(|w| !want.contains(&w)) {
                j.watch = None;
            }
            j.rooms = want.clone();
        }
        for k in had.union(&want) {
            self.sync(p, *k, account, at_ms);
        }
    }

    /// The account's view changed (its guild, ignores or privacy): its
    /// sessions' rooms follow. The sessions whose rooms changed.
    pub fn set_view(
        &mut self,
        p: &Presence,
        account: i64,
        view: AccountView,
        at_ms: u64,
    ) -> Vec<SessionId> {
        let online = p.sessions_of(crate::accounts::AccountId(account));
        // Only online accounts keep a view (`leave` drops it): one read as
        // the account logged out would stay until it said hello again.
        if online.is_empty() {
            return Vec::new();
        }
        self.views.insert(account, view);
        let sessions: Vec<SessionId> = online
            .iter()
            .map(|h| h.session)
            .filter(|s| self.sessions.contains_key(s))
            .collect();
        let mut changed = Vec::new();
        for s in sessions {
            let before = self.granted(s);
            self.place_session(p, s, account, at_ms);
            if self.granted(s) != before {
                changed.push(s);
            }
        }
        // A guild tag or a privacy change shows in the user lists.
        let keys: Vec<RoomKey> = self.rooms_of(account);
        for k in keys {
            self.sync(p, k, account, at_ms);
        }
        changed
    }

    /// The rooms the account is in.
    fn rooms_of(&self, account: i64) -> Vec<RoomKey> {
        self.rooms
            .iter()
            .filter(|(_, r)| r.members.contains_key(&account) || r.shown.contains_key(&account))
            .map(|(k, _)| *k)
            .collect()
    }

    /// The session changed (its character, its game): the user lists
    /// follow.
    pub fn changed(&mut self, p: &Presence, account: i64, at_ms: u64) {
        for k in self.rooms_of(account) {
            self.sync(p, k, account, at_ms);
        }
    }

    /// The session ended: it leaves its rooms.
    pub fn leave(&mut self, p: &Presence, session: SessionId, account: i64, at_ms: u64) {
        let Some(j) = self.sessions.remove(&session) else {
            return;
        };
        for k in &j.rooms {
            if let Some(r) = self.rooms.get_mut(k) {
                if let Some(s) = r.members.get_mut(&account) {
                    s.remove(&session);
                    if s.is_empty() {
                        r.members.remove(&account);
                    }
                }
            }
            self.sync(p, *k, account, at_ms);
        }
        if p.sessions_of(crate::accounts::AccountId(account))
            .is_empty()
        {
            self.views.remove(&account);
        }
    }

    /// How the account shows in room `key` now: the who and place of its
    /// session there with a character; `None` when it isn't a member.
    fn shown_now(&self, p: &Presence, key: RoomKey, account: i64) -> Option<(Who, u8)> {
        let r = self.rooms.get(&key)?;
        let sessions = r.members.get(&account)?;
        let here = sessions
            .iter()
            .filter_map(|s| p.get(*s))
            .find(|h| h.character.is_some())?;
        Some(self.show(&here, matches!(key, RoomKey::Guild(_))))
    }

    /// A session as lines and user lists show it, with its privacy.
    fn show(&self, here: &Here, in_guild_room: bool) -> (Who, u8) {
        let view = self.views.get(&here.account.0);
        let tag = view.and_then(|v| v.guild.as_ref()).map(|g| g.tag.as_str());
        let mut who = here.who(tag, in_guild_room);
        let mut at = here.place();
        if view.is_some_and(|v| v.show_where == show_where::NOBODY) {
            at = place::HIDDEN;
            who.flags &= !who_flags::IN_GAME;
        }
        (who, at)
    }

    /// Tells room `key`'s watchers (and, in a small room, every member)
    /// what changed about `account` since they were last told.
    fn sync(&mut self, p: &Presence, key: RoomKey, account: i64, at_ms: u64) {
        let now = self.shown_now(p, key, account);
        let (op, count, who, at) = {
            let Some(r) = self.rooms.get_mut(&key) else {
                return;
            };
            let before = r.shown.get(&account).cloned();
            let op = match (&before, &now) {
                (None, Some(n)) => {
                    r.shown.insert(account, n.clone());
                    presence_op::JOINED
                }
                (Some(_), None) => {
                    r.shown.remove(&account);
                    presence_op::LEFT
                }
                (Some(b), Some(n)) if b != n => {
                    r.shown.insert(account, n.clone());
                    presence_op::CHANGED
                }
                _ => return,
            };
            let (who, at) = now.or(before).expect("one of them is set");
            (op, r.shown.len(), who, at)
        };
        let push = FromRealm::RoomPresence(RoomPresence {
            room: key.number(),
            op,
            who: who.clone(),
            place: at,
            count: u16::try_from(count).unwrap_or(u16::MAX),
        })
        .body();
        for s in self.watchers(key) {
            if let Some(h) = p.get(s) {
                h.out.send(push.clone());
            }
        }
        // A left line goes to a room that was small before its member left.
        let size = if op == presence_op::LEFT {
            count + 1
        } else {
            count
        };
        if op != presence_op::CHANGED && size <= self.join_lines_max {
            let what = if op == presence_op::JOINED {
                line_what::JOINED
            } else {
                line_what::LEFT
            };
            self.add_line(p, key, what, who, String::new(), at_ms, None);
        }
    }

    /// The sessions watching room `key`'s user list.
    fn watchers(&self, key: RoomKey) -> Vec<SessionId> {
        self.sessions
            .iter()
            .filter(|(_, j)| j.watch == Some(key))
            .map(|(s, _)| *s)
            .collect()
    }

    /// A line in room `key`: kept, then sent to its members (but those
    /// ignoring `from`), and as the old chat line to sessions without a
    /// hello for the Mod room.
    #[allow(clippy::too_many_arguments)]
    pub fn add_line(
        &mut self,
        p: &Presence,
        key: RoomKey,
        what: u8,
        who: Who,
        text: String,
        at_ms: u64,
        from: Option<i64>,
    ) -> RoomLine {
        let r = self.rooms.entry(key).or_default();
        r.last += 1;
        let line = RoomLine {
            room: key.number(),
            line: r.last,
            what,
            who,
            text,
            at_ms,
        };
        r.ring.push_back(line.clone());
        while r.ring.len() > RING {
            r.ring.pop_front();
        }
        let body = FromRealm::RoomLine(line.clone()).body();
        let members: Vec<(i64, SessionId)> = r
            .members
            .iter()
            .flat_map(|(a, s)| s.iter().map(move |s| (*a, *s)))
            .collect();
        for (account, session) in members {
            let ignored = from.is_some_and(|f| {
                self.views
                    .get(&account)
                    .is_some_and(|v| v.ignores.contains(&f))
            });
            if ignored {
                continue;
            }
            if let Some(h) = p.get(session) {
                h.out.send(body.clone());
            }
        }
        let spoken = what == line_what::SAID || what == line_what::EMOTE;
        if key == RoomKey::Mod && spoken {
            let old = legacy_line(&line);
            for h in p.all() {
                if h.character.is_some() && !self.sessions.contains_key(&h.session) {
                    h.out.send(old.clone());
                }
            }
        }
        line
    }

    /// `Say` from `here` in its room `key`.
    pub fn say(
        &mut self,
        p: &Presence,
        here: &Here,
        key: RoomKey,
        emote: bool,
        text: String,
        at_ms: u64,
    ) -> RoomLine {
        let (who, _) = self.show(here, matches!(key, RoomKey::Guild(_)));
        let what = if emote {
            line_what::EMOTE
        } else {
            line_what::SAID
        };
        self.add_line(p, key, what, who, text, at_ms, Some(here.account.0))
    }

    /// The members of room `key` as user rows, sorted by character name
    /// (any case).
    pub fn users(&self, key: RoomKey) -> Vec<(Who, u8)> {
        let mut rows: Vec<(Who, u8)> = self
            .rooms
            .get(&key)
            .map(|r| r.shown.values().cloned().collect())
            .unwrap_or_default();
        rows.sort_by_key(|(w, _)| w.char.to_ascii_lowercase());
        rows
    }

    /// The session watches room `key`'s user list (`None`: none).
    pub fn watch(&mut self, session: SessionId, key: Option<RoomKey>) {
        if let Some(j) = self.sessions.get_mut(&session) {
            j.watch = key;
        }
    }

    /// The session's watched room.
    pub fn watched(&self, session: SessionId) -> Option<RoomKey> {
        self.sessions.get(&session).and_then(|j| j.watch)
    }

    /// Room `key` is gone (a disbanded guild's): its lines and watchers
    /// with it.
    pub fn forget(&mut self, key: RoomKey) {
        self.rooms.remove(&key);
        for j in self.sessions.values_mut() {
            j.rooms.remove(&key);
            if j.watch == Some(key) {
                j.watch = None;
            }
        }
    }

    /// The members (sessions) of room `key`.
    pub fn member_sessions(&self, key: RoomKey) -> Vec<SessionId> {
        self.rooms
            .get(&key)
            .map(|r| r.members.values().flatten().copied().collect())
            .unwrap_or_default()
    }
}
