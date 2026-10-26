//! The fake's chat rooms, DMs, user lists, guilds and friends (lobby
//! contract sections 3-5), seeded with a realm's worth of players: 37
//! others online (so a room's list says "In chat (38)" with the session),
//! the guild `<NUG>` of three with the session as its leader, an invite to
//! `<PK>`, friends in each place (in the lobby, in a game, offline, hidden,
//! one-sided) and two DM conversations, one with an unread line.

use super::super::link::{
    guild_notice, guild_rank, line_what, lobby_code, room, who_flag, Asked, DmConversation, DmLine,
    Done, ErrorCode, Event, FriendGame, FriendNotice, FriendOp, FriendRow, GuildBrief, GuildInfo,
    GuildMember, GuildNotice, GuildOp, GuildQuery, Privacy, Request, RoomLine, SocialState,
    UserRow, Where, Who,
};
use super::{refused, FakeServer};

/// Room lines kept per room, and sent as backlog on `SocialHello`.
const ROOM_KEEP: usize = 50;
const BACKLOG: usize = 20;
/// DM conversations a page.
const DM_PAGE: usize = 30;
/// Chat text: 1-200 bytes.
const MAX_TEXT: usize = 200;
const MAX_MOTD: usize = 120;
const GUILD_MAX_MEMBERS: usize = 100;
const FRIENDS_MAX: usize = 100;
const IGNORE_MAX: usize = 200;
const HOUR_MS: u64 = 3_600_000;

/// Another player of the realm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub account: String,
    /// Their selected (or last) character; `who.tag` follows their guild.
    pub who: Who,
    pub place: Where,
    /// The game they are in: id and name.
    pub game: Option<(u32, String)>,
    pub last_seen_ms: u64,
    /// They added the session's account as a friend (mutual when the
    /// session added them too).
    pub added_me: bool,
    /// They ignore the session's account.
    pub ignores_me: bool,
    pub privacy: Privacy,
}

/// A guild of the realm. The session's own membership is
/// [`FakeSocial::my_guild`]; `members` are the others.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeGuild {
    pub id: u64,
    pub tag: String,
    pub name: String,
    pub motd: String,
    pub created_ms: u64,
    /// Other members: account, rank, joined.
    pub members: Vec<(String, u8, u64)>,
}

/// A DM line of the session's account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredDm {
    pub line: DmLine,
    /// An incoming line the account has read.
    pub read: bool,
}

/// The realm lobby's social state as the session sees it.
#[derive(Clone, Debug)]
pub struct FakeSocial {
    pub mod_id: String,
    pub mod_label: String,
    pub players: Vec<Player>,
    pub guilds: Vec<FakeGuild>,
    /// The session's guild and rank.
    pub my_guild: Option<(u64, u8)>,
    /// Invites to the session: guild, inviting character, when.
    pub invites: Vec<(u64, String, u64)>,
    /// Invites the session's guild sent (accounts).
    pub invited: Vec<String>,
    /// Friends and ignores: account, added.
    pub friends: Vec<(String, u64)>,
    pub ignored: Vec<(String, u64)>,
    pub privacy: Privacy,
    pub dms: Vec<StoredDm>,
    /// Room lines, oldest first (at most [`ROOM_KEEP`] per room).
    pub lines: Vec<RoomLine>,
    pub muted_until_ms: Option<u64>,
    /// The rooms the session asked for in `SocialHello` (`None`: not yet).
    pub hello: Option<u16>,
    /// The room whose user list stays live.
    pub watched: Option<u8>,
    next_line: u64,
    next_dm: u64,
    next_guild: u64,
}

/// The seeded others online (besides the named players).
const CROWD: [&str; 31] = [
    "Aldric", "Brannoc", "Caelith", "Dravik", "Elowen", "Fenrask", "Gorrin", "Halvard", "Isolde",
    "Jorund", "Kestrel", "Lorcan", "Maelis", "Norrik", "Orsolya", "Perrin", "Quillon", "Rhosyn",
    "Sevrin", "Thessa", "Ulfric", "Valka", "Wendrel", "Xandor", "Yrsa", "Zoltar", "Brisa",
    "Corvus", "Dagny", "Eirik", "Faelan",
];

fn player(account: &str, char: &str, class: u8, level: u8, place: Where) -> Player {
    let mut flags = who_flag::EXPANSION;
    if place == Where::InGame {
        flags |= who_flag::IN_GAME;
    }
    Player {
        account: account.into(),
        who: Who {
            char: char.into(),
            account: String::new(),
            tag: None,
            class,
            level,
            flags,
        },
        place,
        game: None,
        last_seen_ms: 0,
        added_me: false,
        ignores_me: false,
        privacy: Privacy::default(),
    }
}

/// Text rule: 1-`max` bytes after trimming, no control characters.
fn text_ok(t: &str, max: usize) -> bool {
    let t = t.trim();
    !t.is_empty() && t.len() <= max && !t.chars().any(char::is_control)
}

/// `s` cut to at most `n` bytes at a character boundary.
fn cut(s: &str, n: usize) -> String {
    let mut n = n.min(s.len());
    while !s.is_char_boundary(n) {
        n -= 1;
    }
    s[..n].to_string()
}

/// Guild tags: 2-4 ASCII letters or digits.
fn tag_ok(t: &str) -> bool {
    (2..=4).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Guild names: 3-24 letters, digits and single spaces, no leading or
/// trailing space.
fn guild_name_ok(n: &str) -> bool {
    (3..=24).contains(&n.len())
        && n.chars().all(|c| c.is_alphanumeric() || c == ' ')
        && !n.starts_with(' ')
        && !n.ends_with(' ')
        && !n.contains("  ")
}

impl FakeSocial {
    /// The seeded realm at clock `now`.
    pub fn seeded(now: u64) -> Self {
        let mut players = vec![
            // A mutual friend in the lobby, officer of the guild.
            Player {
                added_me: true,
                ..player("kaern", "Kaern", 4, 82, Where::Lobby)
            },
            // A mutual friend in a game.
            Player {
                added_me: true,
                game: Some((101, "baal run".into())),
                ..player("rhaego", "Rhaego", 3, 91, Where::InGame)
            },
            // A one-sided friend (online, the place not shown).
            player("mirelle", "Mirelle", 1, 57, Where::Lobby),
            // A mutual friend who hides where they are.
            Player {
                added_me: true,
                privacy: Privacy {
                    whispers: 0,
                    show_where: 1,
                },
                ..player("vexa", "Vexa", 6, 74, Where::Lobby)
            },
            // A mutual friend, offline for two hours.
            Player {
                added_me: true,
                last_seen_ms: now - 2 * HOUR_MS,
                ..player("tobben", "Tobben", 0, 66, Where::Offline)
            },
            // A guild member, offline since yesterday.
            Player {
                last_seen_ms: now - 26 * HOUR_MS,
                ..player("ashka", "Ashka", 5, 48, Where::Offline)
            },
            // The leader of `<PK>`, who invited the session.
            player("grim", "Grim", 2, 88, Where::Lobby),
            // Ignored by the session.
            player("spammer", "Goldseller", 0, 1, Where::Lobby),
        ];
        for (i, name) in CROWD.iter().enumerate() {
            let place = if i % 4 == 3 {
                Where::InGame
            } else {
                Where::Lobby
            };
            let account = name.to_ascii_lowercase();
            players.push(player(
                &account,
                name,
                (i % 7) as u8,
                20 + (i * 7 % 70) as u8,
                place,
            ));
        }
        for p in &mut players {
            if ["kaern", "ashka"].contains(&p.account.as_str()) {
                p.who.tag = Some("NUG".into());
            } else if p.account == "grim" {
                p.who.tag = Some("PK".into());
            }
        }
        let guilds = vec![
            FakeGuild {
                id: 1,
                tag: "NUG".into(),
                name: "Nugwa".into(),
                motd: "Baal runs every evening, ask Kaern.".into(),
                created_ms: now - 40 * 24 * HOUR_MS,
                members: vec![
                    ("kaern".into(), guild_rank::OFFICER, now - 30 * 24 * HOUR_MS),
                    ("ashka".into(), guild_rank::MEMBER, now - 10 * 24 * HOUR_MS),
                ],
            },
            FakeGuild {
                id: 2,
                tag: "PK".into(),
                name: "Player Killers".into(),
                motd: String::new(),
                created_ms: now - 5 * 24 * HOUR_MS,
                members: vec![("grim".into(), guild_rank::LEADER, now - 5 * 24 * HOUR_MS)],
            },
        ];
        let said = |room: u8, line: u64, char: &str, text: &str, ago_s: u64| RoomLine {
            room,
            line,
            what: line_what::SAID,
            who: Who {
                char: char.into(),
                class: 4,
                level: 60,
                flags: who_flag::EXPANSION,
                ..Who::default()
            },
            text: text.into(),
            at_ms: now - ago_s * 1000,
        };
        let lines = vec![
            said(room::GLOBAL, 1, "Aldric", "anyone selling a Shako?", 300),
            said(room::GLOBAL, 2, "Kaern", "lf baal runs, hell", 240),
            said(room::GLOBAL, 3, "Elowen", "wtb Spirit runes", 120),
            said(
                room::MOD,
                4,
                "Rhaego",
                "the new uniques are in, check the market",
                200,
            ),
            said(room::MOD, 5, "Isolde", "gz on the ladder top 10", 60),
            said(
                room::GUILD,
                6,
                "Kaern",
                "baal run at 20:00, bring a pally",
                600,
            ),
        ];
        let dm =
            |dm: u64, peer: &str, char: &str, outgoing: bool, text: &str, ago_s: u64| StoredDm {
                line: DmLine {
                    dm,
                    peer_account: peer.into(),
                    peer_char: char.into(),
                    outgoing,
                    stored: false,
                    text: text.into(),
                    at_ms: now - ago_s * 1000,
                },
                read: true,
            };
        let mut dms = vec![
            dm(1, "kaern", "Kaern", false, "you up for baal?", 900),
            dm(2, "kaern", "Kaern", true, "sure, 5 min", 840),
            dm(3, "kaern", "Kaern", false, "game is baal run, pw 123", 60),
            StoredDm {
                line: DmLine {
                    stored: true,
                    ..dm(4, "tobben", "Tobben", true, "got your Enigma base", 3600).line
                },
                read: true,
            },
        ];
        dms[2].read = false;
        FakeSocial {
            mod_id: "d2gamma".into(),
            mod_label: "D2Gamma".into(),
            players,
            guilds,
            my_guild: Some((1, guild_rank::LEADER)),
            invites: vec![(2, "Grim".into(), now - HOUR_MS)],
            invited: Vec::new(),
            friends: ["kaern", "rhaego", "mirelle", "vexa", "tobben"]
                .iter()
                .map(|a| (a.to_string(), now - 20 * 24 * HOUR_MS))
                .collect(),
            ignored: vec![("spammer".into(), now - 3 * 24 * HOUR_MS)],
            privacy: Privacy::default(),
            dms,
            lines,
            muted_until_ms: None,
            hello: None,
            watched: None,
            next_line: 7,
            next_dm: 5,
            next_guild: 3,
        }
    }

    /// The session's guild.
    pub fn my_guild(&self) -> Option<&FakeGuild> {
        let (id, _) = self.my_guild?;
        self.guilds.iter().find(|g| g.id == id)
    }

    /// The rooms granted for the rooms asked: Guild only with a guild,
    /// never Game.
    fn granted(&self, asked: u16) -> u16 {
        let mut can = room::bit(room::GLOBAL) | room::bit(room::MOD);
        if self.my_guild.is_some() {
            can |= room::bit(room::GUILD);
        }
        asked & can
    }

    /// A player by character name or `*account` (any case).
    pub fn find(&self, who: &str) -> Option<usize> {
        match who.strip_prefix('*') {
            Some(acc) => self
                .players
                .iter()
                .position(|p| p.account.eq_ignore_ascii_case(acc)),
            None => self
                .players
                .iter()
                .position(|p| p.who.char.eq_ignore_ascii_case(who)),
        }
    }

    fn by_account(&self, account: &str) -> Option<&Player> {
        self.players
            .iter()
            .find(|p| p.account.eq_ignore_ascii_case(account))
    }

    fn is_friend(&self, account: &str) -> bool {
        self.friends
            .iter()
            .any(|(a, _)| a.eq_ignore_ascii_case(account))
    }

    fn is_ignored(&self, account: &str) -> bool {
        self.ignored
            .iter()
            .any(|(a, _)| a.eq_ignore_ascii_case(account))
    }

    fn in_my_guild(&self, account: &str) -> bool {
        self.my_guild().is_some_and(|g| {
            g.members
                .iter()
                .any(|(a, ..)| a.eq_ignore_ascii_case(account))
        })
    }

    /// A player as this session may see them: the account for friends,
    /// the own guild and DM peers.
    fn who_of(&self, p: &Player) -> Who {
        let mut w = p.who.clone();
        let dm_peer = self
            .dms
            .iter()
            .any(|d| d.line.peer_account.eq_ignore_ascii_case(&p.account));
        if self.is_friend(&p.account) || self.in_my_guild(&p.account) || dm_peer {
            w.account = p.account.clone();
        }
        w
    }

    /// Where a player is, as this session may see it: mutual friends see
    /// the place (unless the player hides it), others online or offline.
    fn place_of(&self, p: &Player) -> (Where, Option<FriendGame>) {
        let mutual = self.is_friend(&p.account) && p.added_me;
        match p.place {
            Where::Offline => (Where::Offline, None),
            _ if !mutual || p.privacy.show_where != 0 => (Where::Hidden, None),
            place => {
                let game = p.game.as_ref().map(|(id, name)| FriendGame {
                    game_id: *id,
                    game_name: name.clone(),
                    joinable: 0,
                });
                (place, game)
            }
        }
    }

    fn unread(&self) -> (u16, u16) {
        let mut lines = 0u16;
        let mut peers: Vec<&str> = Vec::new();
        for d in self.dms.iter().filter(|d| !d.read && !d.line.outgoing) {
            lines = lines.saturating_add(1);
            if !peers.contains(&d.line.peer_account.as_str()) {
                peers.push(&d.line.peer_account);
            }
        }
        (lines, peers.len() as u16)
    }

    /// The session ended: it is in no room and watches no list.
    pub(super) fn session_ends(&mut self) {
        self.hello = None;
        self.watched = None;
    }
}

impl FakeServer {
    /// The session's chat facts now.
    fn social_state(&self) -> SocialState {
        let s = &self.social;
        let (dm_unread, dm_conversations_unread) = s.unread();
        SocialState {
            rooms: s.granted(s.hello.unwrap_or(0)),
            mod_id: s.mod_id.clone(),
            mod_label: s.mod_label.clone(),
            guild: s.my_guild().map(|g| GuildBrief {
                guild: g.id,
                tag: g.tag.clone(),
                name: g.name.clone(),
                my_rank: s.my_guild.map_or(guild_rank::NONE, |(_, r)| r),
            }),
            dm_unread,
            dm_conversations_unread,
            friends_online: s
                .friends
                .iter()
                .filter(|(a, _)| s.by_account(a).is_some_and(|p| p.place != Where::Offline))
                .count() as u16,
            guild_invites: s.invites.len() as u16,
            whispers: s.privacy.whispers,
            show_where: s.privacy.show_where,
            say_lines: 5,
            say_window_s: 10,
            global_lines: 2,
            global_window_s: 30,
            muted_until_ms: s.muted_until_ms.filter(|&t| t > self.now_ms),
            now_ms: self.now_ms,
        }
    }

    fn push_state(&mut self) {
        let st = self.social_state();
        self.out.push_back(Event::SocialState(st));
    }

    fn done(&mut self, asked: Asked, flags: u8, id: Option<u64>) {
        self.out.push_back(Event::Done(Done { asked, flags, id }));
    }

    fn refuse(&mut self, asked: Asked, code: ErrorCode, text: &str) {
        self.out.push_back(refused(asked, code, text));
    }

    /// The seconds left of a mute (`None`: not muted).
    fn muted_s(&self) -> Option<u32> {
        let until = self.social.muted_until_ms.filter(|&t| t > self.now_ms)?;
        Some((until - self.now_ms).div_ceil(1000) as u32)
    }

    /// A line in a room, kept and pushed to the session when it is in it.
    fn add_line(&mut self, room: u8, what: u8, who: Who, text: &str) {
        let s = &mut self.social;
        let line = RoomLine {
            room,
            line: s.next_line,
            what,
            who,
            text: text.to_string(),
            at_ms: self.now_ms,
        };
        s.next_line += 1;
        s.lines.push(line.clone());
        if s.lines.iter().filter(|l| l.room == room).count() > ROOM_KEEP {
            if let Some(i) = s.lines.iter().position(|l| l.room == room) {
                s.lines.remove(i);
            }
        }
        let heard = s.hello.is_some_and(|h| s.granted(h) & room::bit(room) != 0);
        if heard {
            self.out.push_back(Event::RoomLine(line));
        }
    }

    /// A DM line of the session's account, pushed at once.
    fn add_dm(&mut self, peer: &Player, outgoing: bool, stored: bool, text: &str) -> u64 {
        let s = &mut self.social;
        let line = DmLine {
            dm: s.next_dm,
            peer_account: peer.account.clone(),
            peer_char: peer.who.char.clone(),
            outgoing,
            stored,
            text: text.to_string(),
            at_ms: self.now_ms,
        };
        s.next_dm += 1;
        s.dms.push(StoredDm {
            line: line.clone(),
            read: outgoing,
        });
        self.out.push_back(Event::DmLine(line.clone()));
        line.dm
    }

    /// Somebody of the realm says a line in a room (by character name; an
    /// unknown name speaks as a stranger).
    pub fn say_from(&mut self, room: u8, char: &str, text: &str) {
        let who = match self.social.find(char) {
            Some(i) => {
                let p = &self.social.players[i];
                if self.social.is_ignored(&p.account) {
                    return;
                }
                self.social.who_of(p)
            }
            None => Who {
                char: char.into(),
                class: 255,
                ..Who::default()
            },
        };
        self.add_line(room, line_what::SAID, who, text);
    }

    /// A realm notice in Global (the console's `notice`).
    pub fn notice(&mut self, text: &str) {
        self.add_line(room::GLOBAL, line_what::NOTICE, Who::default(), text);
    }

    /// A player whispers the session's account (by character name or
    /// `*account`); ignored accounts are dropped as the realm does.
    pub fn whisper_from(&mut self, who: &str, text: &str) {
        let Some(i) = self.social.find(who) else {
            return;
        };
        let p = self.social.players[i].clone();
        if self.social.is_ignored(&p.account) {
            return;
        }
        self.add_dm(&p, false, false, text);
        self.push_state();
    }

    /// A player moves (logs in or out, enters or leaves a game): friends
    /// get a `FriendNotice`, the watched room's list a `RoomPresence`.
    pub fn player_moves(&mut self, who: &str, place: Where, game: Option<(u32, String)>) {
        use super::super::link::{friend_notice, presence_op, RoomPresence};
        let Some(i) = self.social.find(who) else {
            return;
        };
        let before = self.social.players[i].place;
        {
            let p = &mut self.social.players[i];
            p.place = place;
            p.game = game;
            if place == Where::Offline {
                p.last_seen_ms = self.now_ms;
            }
            if place == Where::InGame {
                p.who.flags |= who_flag::IN_GAME;
            } else {
                p.who.flags &= !who_flag::IN_GAME;
            }
        }
        let p = self.social.players[i].clone();
        let who_seen = self.social.who_of(&p);
        let (seen, mut fgame) = self.social.place_of(&p);
        if let Some(g) = &mut fgame {
            g.joinable = self.joinable(g.game_id);
        }
        if self.social.is_friend(&p.account) && p.added_me {
            let what = match (before, place) {
                (Where::Offline, _) => friend_notice::ONLINE,
                (_, Where::Offline) => friend_notice::OFFLINE,
                (_, Where::InGame) => friend_notice::ENTERED_GAME,
                _ => friend_notice::LEFT_GAME,
            };
            self.out.push_back(Event::FriendNotice(FriendNotice {
                what,
                who: who_seen.clone(),
                place: seen,
                game: fgame,
                at_ms: self.now_ms,
            }));
        }
        if let Some(watched) = self.social.watched {
            if watched != room::GUILD || self.social.in_my_guild(&p.account) {
                let op = match (before, place) {
                    (Where::Offline, _) => presence_op::JOINED,
                    (_, Where::Offline) => presence_op::LEFT,
                    _ => presence_op::CHANGED,
                };
                let count = self.room_users(watched).len() as u16;
                self.out.push_back(Event::RoomPresence(RoomPresence {
                    room: watched,
                    op,
                    who: who_seen,
                    place: seen,
                    count,
                }));
            }
        }
    }

    /// The online members of a room the session is in, the session
    /// included, sorted by character name.
    fn room_users(&self, r: u8) -> Vec<UserRow> {
        let s = &self.social;
        let mut rows: Vec<UserRow> = s
            .players
            .iter()
            .filter(|p| p.place != Where::Offline)
            .filter(|p| r != room::GUILD || s.in_my_guild(&p.account))
            .map(|p| UserRow {
                who: s.who_of(p),
                place: s.place_of(p).0,
            })
            .collect();
        rows.push(UserRow {
            who: self.me_who(),
            place: Where::Lobby,
        });
        rows.sort_by_key(|u| u.who.char.to_ascii_lowercase());
        rows
    }

    /// The session's guild as `GuildInfo`, and the members with the session.
    fn guild_view(&self, g: &super::FakeGuild) -> GuildInfo {
        let s = &self.social;
        let mine = s.my_guild.filter(|(id, _)| *id == g.id);
        let leader_char = match mine {
            Some((_, guild_rank::LEADER)) => self.me_who().char,
            _ => g
                .members
                .iter()
                .find(|(_, r, _)| *r == guild_rank::LEADER)
                .and_then(|(a, ..)| s.by_account(a))
                .map(|p| p.who.char.clone())
                .unwrap_or_default(),
        };
        GuildInfo {
            guild: g.id,
            tag: g.tag.clone(),
            name: g.name.clone(),
            motd: g.motd.clone(),
            created_ms: g.created_ms,
            members: (g.members.len() + usize::from(mine.is_some())) as u16,
            leader_char,
            my_rank: mine.map_or(guild_rank::NONE, |(_, r)| r),
        }
    }

    fn guild_members(&self, g: &super::FakeGuild) -> Vec<GuildMember> {
        let s = &self.social;
        let mut out: Vec<GuildMember> = g
            .members
            .iter()
            .filter_map(|(a, rank, joined)| {
                let p = s.by_account(a)?;
                Some(GuildMember {
                    who: s.who_of(p),
                    // The own guild sees where its members are.
                    place: p.place,
                    rank: *rank,
                    joined_ms: *joined,
                    last_seen_ms: p.last_seen_ms,
                })
            })
            .collect();
        if let Some((_, rank)) = s.my_guild.filter(|(id, _)| *id == g.id) {
            out.push(GuildMember {
                who: self.me_who(),
                place: Where::Lobby,
                rank,
                joined_ms: g.created_ms,
                last_seen_ms: self.now_ms,
            });
        }
        out.sort_by_key(|m| (std::cmp::Reverse(m.rank), m.who.char.to_ascii_lowercase()));
        out
    }

    /// A guild event: the notice to the session, the line in the Guild room.
    fn guild_event(&mut self, what: u8, who: &str, text: &str) {
        let Some(g) = self.social.my_guild().cloned() else {
            return;
        };
        let by = self.me_who().char;
        self.out.push_back(Event::GuildNotice(GuildNotice {
            what,
            guild: g.id,
            tag: g.tag.clone(),
            name: g.name.clone(),
            who: who.to_string(),
            by,
            at_ms: self.now_ms,
        }));
        self.add_line(room::GUILD, line_what::GUILD_EVENT, Who::default(), text);
    }

    /// Chat rooms, DMs, user lists, guilds and friends.
    pub(super) fn answer_social(&mut self, asked: Asked, r: Request) {
        match r {
            Request::SocialHello { rooms } => {
                self.social.hello = Some(rooms);
                self.push_state();
                let granted = self.social.granted(rooms);
                for rm in [room::GLOBAL, room::MOD, room::GUILD] {
                    if granted & room::bit(rm) == 0 {
                        continue;
                    }
                    let lines: Vec<RoomLine> = self
                        .social
                        .lines
                        .iter()
                        .filter(|l| l.room == rm)
                        .cloned()
                        .collect();
                    let skip = lines.len().saturating_sub(BACKLOG);
                    self.out
                        .extend(lines.into_iter().skip(skip).map(Event::RoomLine));
                }
            }
            Request::Say {
                room: rm,
                emote,
                text,
            } => {
                let granted = self.social.granted(self.social.hello.unwrap_or(0));
                if self.selected.is_none() {
                    self.refuse(asked, ErrorCode::NO_CHAR_SELECTED, "");
                } else if let Some(wait_s) = self.muted_s() {
                    self.out.push_back(Event::Refused {
                        asked,
                        failure: super::super::link::Failure {
                            code: lobby_code::MUTED,
                            wait_s,
                            text: String::new(),
                        },
                    });
                } else if !text_ok(&text, MAX_TEXT) {
                    self.refuse(asked, lobby_code::BAD_TEXT, "");
                } else if rm == room::GUILD && self.social.my_guild.is_none() {
                    self.refuse(asked, lobby_code::NOT_IN_GUILD, "");
                } else if granted & room::bit(rm) == 0 || rm > room::GUILD {
                    self.refuse(asked, lobby_code::ROOM_UNAVAILABLE, "");
                } else {
                    self.done(asked, 0, None);
                    let what = if emote {
                        line_what::EMOTE
                    } else {
                        line_what::SAID
                    };
                    let who = self.me_who();
                    self.add_line(rm, what, who, text.trim());
                }
            }
            Request::Whisper { to, text } => {
                let found = self
                    .social
                    .find(&to)
                    .map(|i| self.social.players[i].clone());
                if let Some(wait_s) = self.muted_s() {
                    self.out.push_back(Event::Refused {
                        asked,
                        failure: super::super::link::Failure {
                            code: lobby_code::MUTED,
                            wait_s,
                            text: String::new(),
                        },
                    });
                } else if !text_ok(&text, MAX_TEXT) {
                    self.refuse(asked, lobby_code::BAD_TEXT, "");
                } else if let Some(p) = found {
                    let replying = self
                        .social
                        .dms
                        .iter()
                        .any(|d| !d.line.outgoing && d.line.peer_account == p.account);
                    let friend_or_guild = p.added_me || self.social.in_my_guild(&p.account);
                    let blocked = p.ignores_me
                        || match p.privacy.whispers {
                            0 => false,
                            1 => !friend_or_guild && !replying,
                            _ => !replying,
                        };
                    if blocked {
                        self.refuse(asked, lobby_code::WHISPER_BLOCKED, "");
                    } else {
                        let stored = p.place == Where::Offline;
                        let dm = self.add_dm(&p, true, stored, text.trim());
                        // The answer before the line, as the realm sends it.
                        let line = self.out.pop_back();
                        self.done(asked, u8::from(stored), Some(dm));
                        self.out.extend(line);
                    }
                } else {
                    self.refuse(asked, lobby_code::UNKNOWN_PLAYER, "");
                }
            }
            Request::DmConversations { page } => {
                let s = &self.social;
                let mut peers: Vec<String> = Vec::new();
                let mut by_new: Vec<&StoredDm> = s.dms.iter().collect();
                by_new.sort_by_key(|d| std::cmp::Reverse(d.line.dm));
                for d in &by_new {
                    if !peers.contains(&d.line.peer_account) {
                        peers.push(d.line.peer_account.clone());
                    }
                }
                let total = peers.len() as u32;
                let rows = peers
                    .iter()
                    .skip(usize::from(page) * DM_PAGE)
                    .take(DM_PAGE)
                    .filter_map(|peer| {
                        let last = by_new.iter().find(|d| d.line.peer_account == *peer)?;
                        let p = s.by_account(peer);
                        Some(DmConversation {
                            peer_account: peer.clone(),
                            peer_char: p
                                .map_or(last.line.peer_char.clone(), |p| p.who.char.clone()),
                            place: p.map_or(Where::Offline, |p| s.place_of(p).0),
                            unread: s
                                .dms
                                .iter()
                                .filter(|d| {
                                    d.line.peer_account == *peer && !d.read && !d.line.outgoing
                                })
                                .count() as u16,
                            last_ms: last.line.at_ms,
                            last_text: cut(&last.line.text, 60),
                        })
                    })
                    .collect();
                self.out
                    .push_back(Event::DmConversations { total, page, rows });
            }
            Request::DmHistory {
                peer,
                before_dm,
                rows,
            } => {
                let account = match peer.strip_prefix('*') {
                    Some(a) => Some(a.to_string()),
                    None => self
                        .social
                        .find(&peer)
                        .map(|i| self.social.players[i].account.clone()),
                };
                match account {
                    None => self.refuse(asked, lobby_code::UNKNOWN_PLAYER, ""),
                    Some(acc) => {
                        let mut lines: Vec<DmLine> = self
                            .social
                            .dms
                            .iter()
                            .map(|d| &d.line)
                            .filter(|l| l.peer_account.eq_ignore_ascii_case(&acc))
                            .filter(|l| before_dm == 0 || l.dm < before_dm)
                            .cloned()
                            .collect();
                        lines.sort_by_key(|l| l.dm);
                        let keep = usize::from(rows.clamp(1, 50));
                        let skip = lines.len().saturating_sub(keep);
                        lines.drain(..skip);
                        self.out.push_back(Event::DmHistory { peer, lines });
                    }
                }
            }
            Request::DmRead {
                peer_account,
                up_to_dm,
            } => {
                let mut any = false;
                for d in &mut self.social.dms {
                    if d.line.peer_account.eq_ignore_ascii_case(&peer_account) {
                        any = true;
                        if d.line.dm <= up_to_dm {
                            d.read = true;
                        }
                    }
                }
                if any {
                    self.done(asked, 0, None);
                    self.push_state();
                } else {
                    self.refuse(asked, lobby_code::UNKNOWN_PLAYER, "");
                }
            }
            Request::UserList { room: rm, watch } => {
                let granted = self.social.granted(self.social.hello.unwrap_or(0));
                if rm == room::GUILD && self.social.my_guild.is_none() {
                    self.refuse(asked, lobby_code::NOT_IN_GUILD, "");
                } else if granted & room::bit(rm) == 0 {
                    self.refuse(asked, lobby_code::ROOM_UNAVAILABLE, "");
                } else {
                    if watch {
                        self.social.watched = Some(rm);
                    } else if self.social.watched == Some(rm) {
                        self.social.watched = None;
                    }
                    let rows = self.room_users(rm);
                    self.out.push_back(Event::Users {
                        room: rm,
                        total: rows.len() as u32,
                        rows,
                    });
                }
            }
            Request::GuildGet(q) => match q {
                GuildQuery::Mine => match self.social.my_guild().cloned() {
                    None => self.refuse(asked, lobby_code::NOT_IN_GUILD, ""),
                    Some(g) => {
                        let info = self.guild_view(&g);
                        let members = self.guild_members(&g);
                        self.out.push_back(Event::Guild { info, members });
                    }
                },
                GuildQuery::Invites => {
                    let list = self
                        .social
                        .invites
                        .iter()
                        .filter_map(|(id, ..)| self.social.guilds.iter().find(|g| g.id == *id))
                        .map(|g| self.guild_view(g))
                        .collect();
                    self.out.push_back(Event::GuildInvites(list));
                }
                GuildQuery::ByTag(tag) => {
                    let found = self
                        .social
                        .guilds
                        .iter()
                        .find(|g| g.tag.eq_ignore_ascii_case(&tag))
                        .map(|g| self.guild_view(g));
                    match found {
                        Some(info) => self.out.push_back(Event::GuildFound(info)),
                        None => self.refuse(asked, lobby_code::UNKNOWN_PLAYER, "No such guild."),
                    }
                }
            },
            Request::GuildCreate { tag, name } => {
                if self.social.my_guild.is_some() {
                    self.refuse(asked, lobby_code::ALREADY_IN_GUILD, "");
                } else if !tag_ok(&tag) {
                    self.refuse(
                        asked,
                        lobby_code::BAD_GUILD_NAME,
                        "Guild tags are 2-4 letters or digits.",
                    );
                } else if !guild_name_ok(&name) {
                    self.refuse(
                        asked,
                        lobby_code::BAD_GUILD_NAME,
                        "Guild names are 3-24 letters, digits and single spaces.",
                    );
                } else if self
                    .social
                    .guilds
                    .iter()
                    .any(|g| g.tag.eq_ignore_ascii_case(&tag))
                {
                    self.refuse(asked, lobby_code::GUILD_TAG_TAKEN, "");
                } else {
                    let s = &mut self.social;
                    let id = s.next_guild;
                    s.next_guild += 1;
                    s.guilds.push(FakeGuild {
                        id,
                        tag,
                        name,
                        motd: String::new(),
                        created_ms: self.now_ms,
                        members: Vec::new(),
                    });
                    s.my_guild = Some((id, guild_rank::LEADER));
                    self.done(asked, 0, Some(id));
                    self.push_state();
                }
            }
            Request::GuildInvite { who } => {
                let Some((gid, rank)) = self.social.my_guild else {
                    return self.refuse(asked, lobby_code::NOT_IN_GUILD, "");
                };
                let target = self
                    .social
                    .find(&who)
                    .map(|i| self.social.players[i].clone());
                let size = self.social.my_guild().map_or(0, |g| g.members.len() + 1);
                match target {
                    _ if rank < guild_rank::OFFICER => {
                        self.refuse(asked, lobby_code::GUILD_RANK, "")
                    }
                    None => self.refuse(asked, lobby_code::UNKNOWN_PLAYER, ""),
                    Some(p) if p.who.tag.is_some() => {
                        self.refuse(asked, lobby_code::ALREADY_IN_GUILD, "")
                    }
                    Some(_) if size >= GUILD_MAX_MEMBERS => {
                        self.refuse(asked, lobby_code::GUILD_FULL, "")
                    }
                    Some(p) => {
                        if !self.social.invited.contains(&p.account) {
                            self.social.invited.push(p.account);
                        }
                        self.done(asked, 0, Some(gid));
                    }
                }
            }
            Request::GuildAnswer { guild, accept } => {
                match self.social.invites.iter().position(|(g, ..)| *g == guild) {
                    None => self.refuse(asked, lobby_code::INVITE_GONE, ""),
                    Some(_) if accept && self.social.my_guild.is_some() => {
                        self.refuse(asked, lobby_code::ALREADY_IN_GUILD, "")
                    }
                    Some(i) => {
                        self.social.invites.remove(i);
                        if accept {
                            self.social.my_guild = Some((guild, guild_rank::MEMBER));
                        }
                        self.done(asked, 0, Some(guild));
                        self.push_state();
                        if accept {
                            let me = self.me_who().char;
                            self.guild_event(guild_notice::JOINED, &me, &format!("{me} joined."));
                        }
                    }
                }
            }
            Request::GuildManage(op) => self.guild_manage(asked, op),
            Request::FriendsGet { ignored } => {
                let s = &self.social;
                let list = if ignored { &s.ignored } else { &s.friends };
                let rows = list
                    .iter()
                    .filter_map(|(a, added)| {
                        let p = s.by_account(a)?;
                        let (place, mut game) = if ignored {
                            (Where::Offline, None)
                        } else {
                            s.place_of(p)
                        };
                        if let Some(g) = &mut game {
                            g.joinable = self.joinable(g.game_id);
                        }
                        Some(FriendRow {
                            who: s.who_of(p),
                            place,
                            mutual: !ignored && p.added_me,
                            game,
                            last_seen_ms: p.last_seen_ms,
                            added_ms: *added,
                        })
                    })
                    .collect();
                self.out.push_back(Event::Friends { ignored, rows });
            }
            Request::FriendEdit(op) => self.friend_edit(asked, op),
            _ => {}
        }
    }

    fn guild_manage(&mut self, asked: Asked, op: GuildOp) {
        let Some((gid, rank)) = self.social.my_guild else {
            return self.refuse(asked, lobby_code::NOT_IN_GUILD, "");
        };
        let me = self.me_who().char;
        // A member named by the op (character or `*account`): its index in
        // the guild's others and its rank.
        let member = |s: &FakeSocial, who: &str| -> Option<(usize, u8, String, String)> {
            let p = &s.players[s.find(who)?];
            let g = s.my_guild()?;
            let i = g
                .members
                .iter()
                .position(|(a, ..)| a.eq_ignore_ascii_case(&p.account))?;
            Some((i, g.members[i].1, p.account.clone(), p.who.char.clone()))
        };
        match op {
            GuildOp::Leave => {
                let others = self.social.my_guild().map_or(0, |g| g.members.len());
                if rank == guild_rank::LEADER && others > 0 {
                    return self.refuse(
                        asked,
                        lobby_code::GUILD_RANK,
                        "Make another member leader first.",
                    );
                }
                self.guild_event(guild_notice::LEFT, &me, &format!("{me} left."));
                self.social.my_guild = None;
                if others == 0 {
                    self.social.guilds.retain(|g| g.id != gid);
                }
                self.done(asked, 0, None);
                self.push_state();
            }
            GuildOp::Kick(who)
            | GuildOp::Promote(who)
            | GuildOp::Demote(who)
            | GuildOp::MakeLeader(who)
                if member(&self.social, &who).is_none() =>
            {
                self.refuse(asked, lobby_code::UNKNOWN_PLAYER, "")
            }
            GuildOp::Kick(who) => {
                let (i, their, _, char) = member(&self.social, &who).unwrap_or_default();
                let may = rank == guild_rank::LEADER
                    || (rank == guild_rank::OFFICER && their == guild_rank::MEMBER);
                if !may {
                    return self.refuse(asked, lobby_code::GUILD_RANK, "");
                }
                if let Some(g) = self.social.guilds.iter_mut().find(|g| g.id == gid) {
                    g.members.remove(i);
                }
                if let Some(p) = self.social.find(&who) {
                    self.social.players[p].who.tag = None;
                }
                self.done(asked, 0, None);
                self.guild_event(
                    guild_notice::KICKED,
                    &char,
                    &format!("{char} was kicked by {me}."),
                );
            }
            GuildOp::Promote(_) | GuildOp::Demote(_) | GuildOp::MakeLeader(_)
                if rank != guild_rank::LEADER =>
            {
                self.refuse(asked, lobby_code::GUILD_RANK, "")
            }
            GuildOp::Promote(who) => self.set_rank(asked, &who, guild_rank::OFFICER),
            GuildOp::Demote(who) => self.set_rank(asked, &who, guild_rank::MEMBER),
            GuildOp::MakeLeader(who) => {
                let (i, _, _, char) = member(&self.social, &who).unwrap_or_default();
                if let Some(g) = self.social.guilds.iter_mut().find(|g| g.id == gid) {
                    g.members[i].1 = guild_rank::LEADER;
                }
                self.social.my_guild = Some((gid, guild_rank::OFFICER));
                self.done(asked, 0, None);
                self.push_state();
                self.guild_event(
                    guild_notice::LEADER_CHANGED,
                    &char,
                    &format!("{char} leads the guild now."),
                );
            }
            GuildOp::Disband(tag) => {
                let ours = self
                    .social
                    .my_guild()
                    .is_some_and(|g| g.tag.eq_ignore_ascii_case(&tag));
                if rank != guild_rank::LEADER {
                    self.refuse(asked, lobby_code::GUILD_RANK, "")
                } else if !ours {
                    self.refuse(
                        asked,
                        lobby_code::BAD_GUILD_NAME,
                        "Type the guild's tag to disband it.",
                    )
                } else {
                    self.guild_event(guild_notice::DISBANDED, &me, "The guild was disbanded.");
                    self.social.guilds.retain(|g| g.id != gid);
                    self.social.my_guild = None;
                    for p in &mut self.social.players {
                        if p.who
                            .tag
                            .as_ref()
                            .is_some_and(|t| t.eq_ignore_ascii_case(&tag))
                        {
                            p.who.tag = None;
                        }
                    }
                    self.done(asked, 0, None);
                    self.push_state();
                }
            }
            GuildOp::Motd(text) => {
                let ok = text.len() <= MAX_MOTD && !text.chars().any(char::is_control);
                if rank < guild_rank::OFFICER {
                    self.refuse(asked, lobby_code::GUILD_RANK, "")
                } else if !ok {
                    self.refuse(asked, lobby_code::BAD_TEXT, "")
                } else {
                    if let Some(g) = self.social.guilds.iter_mut().find(|g| g.id == gid) {
                        g.motd = text.clone();
                    }
                    self.done(asked, 0, None);
                    self.guild_event(guild_notice::MOTD, &me, &text);
                }
            }
        }
    }

    /// The leader promotes a member to officer or demotes an officer.
    fn set_rank(&mut self, asked: Asked, who: &str, to: u8) {
        let me = self.me_who().char;
        let s = &mut self.social;
        let Some(i) = s.find(who) else {
            return self.refuse(asked, lobby_code::UNKNOWN_PLAYER, "");
        };
        let (account, char) = (s.players[i].account.clone(), s.players[i].who.char.clone());
        let Some((gid, _)) = s.my_guild else {
            return self.refuse(asked, lobby_code::NOT_IN_GUILD, "");
        };
        let slot = s.guilds.iter_mut().find(|g| g.id == gid).and_then(|g| {
            g.members
                .iter_mut()
                .find(|(a, ..)| a.eq_ignore_ascii_case(&account))
        });
        let Some(slot) = slot else {
            return self.refuse(asked, lobby_code::UNKNOWN_PLAYER, "");
        };
        slot.1 = to;
        self.done(asked, 0, None);
        let (what, text) = if to == guild_rank::OFFICER {
            (
                guild_notice::PROMOTED,
                format!("{char} was promoted to officer by {me}."),
            )
        } else {
            (
                guild_notice::DEMOTED,
                format!("{char} was demoted to member by {me}."),
            )
        };
        self.guild_event(what, &char, &text);
    }

    fn friend_edit(&mut self, asked: Asked, op: FriendOp) {
        let found = |s: &FakeSocial, who: &str| s.find(who).map(|i| s.players[i].account.clone());
        let now = self.now_ms;
        let s = &mut self.social;
        let code = match op {
            FriendOp::Privacy(p) if p.whispers > 2 || p.show_where > 1 => Some(ErrorCode::OTHER),
            FriendOp::Privacy(p) => {
                s.privacy = p;
                None
            }
            FriendOp::Add(who) | FriendOp::Ignore(who) if found(s, &who).is_none() => {
                Some(lobby_code::UNKNOWN_PLAYER)
            }
            FriendOp::Add(who) => {
                let acc = found(s, &who).unwrap_or_default();
                if s.is_friend(&acc) {
                    Some(ErrorCode::OTHER)
                } else if s.friends.len() >= FRIENDS_MAX {
                    Some(lobby_code::FRIEND_LIMIT)
                } else {
                    s.friends.push((acc, now));
                    None
                }
            }
            FriendOp::Ignore(who) => {
                let acc = found(s, &who).unwrap_or_default();
                if s.is_ignored(&acc) {
                    Some(ErrorCode::OTHER)
                } else if s.ignored.len() >= IGNORE_MAX {
                    Some(lobby_code::FRIEND_LIMIT)
                } else {
                    s.ignored.push((acc, now));
                    None
                }
            }
            FriendOp::Remove(who) => {
                let before = s.friends.len();
                let acc = found(s, &who).unwrap_or_else(|| who.trim_start_matches('*').into());
                s.friends.retain(|(a, _)| !a.eq_ignore_ascii_case(&acc));
                (s.friends.len() == before).then_some(lobby_code::UNKNOWN_PLAYER)
            }
            FriendOp::Unignore(who) => {
                let before = s.ignored.len();
                let acc = found(s, &who).unwrap_or_else(|| who.trim_start_matches('*').into());
                s.ignored.retain(|(a, _)| !a.eq_ignore_ascii_case(&acc));
                (s.ignored.len() == before).then_some(lobby_code::UNKNOWN_PLAYER)
            }
        };
        match code {
            Some(code) => {
                let text = if code == ErrorCode::OTHER {
                    "Already on the list, or not a privacy choice."
                } else {
                    ""
                };
                self.refuse(asked, code, text);
            }
            None => {
                self.done(asked, 0, None);
                self.push_state();
            }
        }
    }
}

/// A session for the panels' tests: account Nugwa logged in, its
/// character Werwer (expansion, softcore) selected and the social hello
/// said, on a fake `set` prepares first. The answers so far are taken.
#[cfg(test)]
pub fn session(
    set: impl FnOnce(&mut FakeServer),
) -> (super::SharedFake, super::super::OnlineHandle) {
    use super::super::Online;
    use d2_formats::d2s::status;

    let fake = super::SharedFake::default();
    {
        let mut f = fake.0.borrow_mut();
        f.add_account(
            "Nugwa",
            "password1",
            vec![super::character("Werwer", 4, 34, 5, status::EXPANSION)],
        );
        set(&mut f);
    }
    let online = Online::new(Box::new(fake.clone()), Vec::new());
    for r in [
        Request::Connect { index: 0 },
        Request::Login {
            account: "Nugwa".into(),
            password: "password1".into(),
        },
        Request::CharSelect {
            name: "Werwer".into(),
        },
    ] {
        online.borrow_mut().send(r);
    }
    online.borrow_mut().take_events();
    online
        .borrow_mut()
        .send(Request::SocialHello { rooms: 0b111 });
    (fake, online)
}
