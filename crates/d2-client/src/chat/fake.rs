//! An in-memory realm chat for tests and offscreen captures: the approved
//! preview's lobby (Mod: D2Gamma with its lines and a market notice, three
//! new lines in Guild: NUG, a DM conversation with Kaern with one unread
//! line, 38 players in chat). It answers every request as the realm would.

use std::collections::VecDeque;

use super::source::{
    code, ChatIn, ChatOut, ChatSource, DmConversation, DmLine, Done, GuildBrief, LineWhat, Room,
    RoomLine, SocialState, UserRow, Where, Who,
};

/// The fake's clock at the hello: older lines are the backlog.
pub const NOW_MS: u64 = 1_790_000_000_000;

pub struct FakeChat {
    /// The player's selected character and account.
    pub me: Who,
    pub state: SocialState,
    /// Room lines and market notices sent after the hello, in order.
    pub backlog: Vec<ChatIn>,
    /// Lines that arrive after the hello (they count as unread).
    pub fresh: Vec<ChatIn>,
    pub users: Vec<UserRow>,
    pub guild_users: Vec<UserRow>,
    pub dms: Vec<DmLine>,
    pub conversations: Vec<DmConversation>,
    /// Everything the chat sent, in order.
    pub sent: Vec<ChatOut>,
    out: VecDeque<ChatIn>,
    line: u64,
    next_dm: u64,
}

/// A player of the fake: name, class, level, tag.
pub fn player(name: &str, class: u8, level: u8, tag: Option<&str>) -> Who {
    Who {
        char: name.into(),
        account: String::new(),
        tag: tag.map(str::to_string),
        class,
        level,
        flags: super::source::who::EXPANSION,
    }
}

fn row(who: Who) -> UserRow {
    UserRow {
        who,
        place: Where::Lobby,
    }
}

impl FakeChat {
    /// An empty realm chat with every room granted and no guild.
    pub fn empty(me: Who) -> FakeChat {
        FakeChat {
            me,
            state: SocialState {
                rooms: Room::Global.bit() | Room::Mod.bit(),
                mod_label: "D2Gamma".into(),
                guild: None,
                dm_unread: 0,
                muted_until_ms: None,
                now_ms: NOW_MS,
            },
            backlog: Vec::new(),
            fresh: Vec::new(),
            users: Vec::new(),
            guild_users: Vec::new(),
            dms: Vec::new(),
            conversations: Vec::new(),
            sent: Vec::new(),
            out: VecDeque::new(),
            line: 0,
            next_dm: 100,
        }
    }

    /// The preview's lobby, for Werwer (account Nugwa) of guild NUG.
    pub fn seeded() -> FakeChat {
        let me = Who {
            account: "Nugwa".into(),
            ..player("Werwer", 4, 34, Some("NUG"))
        };
        let mut f = FakeChat::empty(me.clone());
        f.state.rooms |= Room::Guild.bit();
        f.state.guild = Some(GuildBrief {
            guild: 7,
            tag: "NUG".into(),
            name: "Nugwa".into(),
            my_rank: 2,
        });
        f.state.dm_unread = 1;
        let ironwolf = player("Ironwolf", 4, 52, None);
        let sparks = player("Sparks", 1, 44, None);
        let gravel = player("Gravel", 3, 61, None);
        let kaern = player("Kaern", 1, 91, Some("NUG"));
        let old = NOW_MS - 60_000;
        f.backlog = vec![
            f.line(
                Room::Global,
                LineWhat::Said,
                &gravel,
                "wts shako, pm me",
                old,
            ),
            f.line(
                Room::Global,
                LineWhat::Said,
                &sparks,
                "anyone up for baal runs?",
                old,
            ),
            f.line(
                Room::Mod,
                LineWhat::Said,
                &ironwolf,
                "anyone for cows?",
                old,
            ),
            f.line(
                Room::Mod,
                LineWhat::Said,
                &sparks,
                "need a key for nm chaos, it is moo",
                old,
            ),
            ChatIn::MarketNotice {
                text: "Your Grim Crown sold for 1 Ist.".into(),
                at_ms: old,
            },
            f.line(
                Room::Mod,
                LineWhat::Said,
                &gravel,
                "hc trades open, bring ber",
                old,
            ),
            f.line(
                Room::Mod,
                LineWhat::Joined,
                &player("Necrolord", 2, 70, None),
                "",
                old,
            ),
        ];
        f.fresh = vec![
            f.line(
                Room::Guild,
                LineWhat::Said,
                &kaern,
                "meeting at 9 in the guild game",
                NOW_MS,
            ),
            f.line(
                Room::Guild,
                LineWhat::Said,
                &kaern,
                "bring your hell keys",
                NOW_MS,
            ),
            f.line(
                Room::Guild,
                LineWhat::GuildEvent,
                &kaern,
                "Kaern promoted Gravel to officer.",
                NOW_MS,
            ),
        ];
        let mut users = vec![
            ironwolf.clone(),
            sparks.clone(),
            gravel.clone(),
            kaern.clone(),
            me.clone(),
        ];
        let names = [
            "Aldric",
            "Bonebreaker",
            "Cain",
            "Deckard",
            "Elzix",
            "Fara",
            "Gheed",
            "Hratli",
            "Isenhart",
            "Jamella",
            "Kashya",
            "Larzuk",
            "Malah",
            "Natalya",
            "Ormus",
            "Pindle",
            "Qual",
            "Rakanishu",
            "Shenk",
            "Tyrael",
            "Ulf",
            "Varriku",
            "Warriv",
            "Xazax",
            "Yshari",
            "Zakarum",
            "Alkor",
            "Bishibosh",
            "Coldcrow",
            "Drognan",
            "Eldritch",
            "Frozenstein",
            "Griswold",
        ];
        for (i, n) in names.iter().enumerate() {
            users.push(player(n, (i % 7) as u8, 10 + (i * 7 % 85) as u8, None));
        }
        f.users = users.into_iter().map(row).collect();
        f.guild_users = vec![row(kaern), row(gravel), row(me)];
        let dm = |dm: u64, outgoing: bool, text: &str, at: u64| DmLine {
            dm,
            peer_account: "Kaern".into(),
            peer_char: "Kaern".into(),
            outgoing,
            stored: false,
            text: text.into(),
            at_ms: at,
        };
        f.dms = vec![
            dm(11, false, "grats on 85", old),
            dm(12, true, "thanks! baal at 9?", old + 1_000),
            dm(13, false, "yes, game baal-hell-12, pw moo", old + 2_000),
        ];
        f.conversations = vec![DmConversation {
            peer_account: "Kaern".into(),
            peer_char: "Kaern".into(),
            place: Where::Lobby,
            unread: 1,
            last_ms: old + 2_000,
            last_text: "yes, game baal-hell-12, pw moo".into(),
        }];
        f
    }

    fn line(&mut self, room: Room, what: LineWhat, who: &Who, text: &str, at_ms: u64) -> ChatIn {
        self.line += 1;
        let text = match what {
            LineWhat::Joined if text.is_empty() => String::new(),
            _ => text.to_string(),
        };
        ChatIn::Line(RoomLine {
            room,
            line: self.line,
            what,
            who: who.clone(),
            text,
            at_ms,
        })
    }

    /// Someone says `text` in `room` now.
    pub fn say_from(&mut self, room: Room, who: &Who, text: &str) {
        let l = self.line(room, LineWhat::Said, who, text, NOW_MS + 1);
        self.out.push_back(l);
    }

    /// After the hello, `who` joins (or leaves) `room` at `at_ms`.
    pub fn fresh_presence(&mut self, room: Room, who: &Who, joined: bool, at_ms: u64) {
        let what = if joined {
            LineWhat::Joined
        } else {
            LineWhat::Left
        };
        let l = self.line(room, what, who, "", at_ms);
        self.fresh.push(l);
    }

    /// After the hello, `who` says `text` in `room` (unread there).
    pub fn fresh_say(&mut self, room: Room, who: &Who, text: &str) {
        let l = self.line(room, LineWhat::Said, who, text, NOW_MS + 1);
        self.fresh.push(l);
    }

    /// A player in the open rooms' user list (`index` in its order).
    pub fn add_user(&mut self, index: usize, who: Who) {
        let at = index.min(self.users.len());
        self.users.insert(at, row(who));
    }

    /// Someone whispers the player now.
    pub fn dm_from(&mut self, peer: &str, text: &str) {
        self.next_dm += 1;
        let d = DmLine {
            dm: self.next_dm,
            peer_account: peer.into(),
            peer_char: peer.into(),
            outgoing: false,
            stored: false,
            text: text.into(),
            at_ms: NOW_MS + 1,
        };
        self.dms.push(d.clone());
        self.out.push_back(ChatIn::Dm(d));
    }

    /// A market notice now.
    pub fn market(&mut self, text: &str) {
        self.out.push_back(ChatIn::MarketNotice {
            text: text.into(),
            at_ms: NOW_MS + 1,
        });
    }

    fn done(&mut self, request: u8, code: u16) {
        self.out.push_back(ChatIn::Done(Done {
            request,
            code,
            ..Done::default()
        }));
    }

    fn find(&self, name: &str) -> Option<Who> {
        let account = name.strip_prefix('*');
        self.users
            .iter()
            .map(|r| &r.who)
            .find(|w| match account {
                Some(a) => w.account.eq_ignore_ascii_case(a) || w.char.eq_ignore_ascii_case(a),
                None => w.char.eq_ignore_ascii_case(name),
            })
            .cloned()
    }
}

impl ChatSource for FakeChat {
    fn send(&mut self, out: ChatOut) {
        self.sent.push(out.clone());
        let kind = out.kind();
        match out {
            ChatOut::Hello { .. } => {
                self.out.push_back(ChatIn::State(self.state.clone()));
                let mut lines = self.backlog.clone();
                lines.extend(self.fresh.clone());
                self.out.extend(lines);
            }
            ChatOut::Say { room, emote, text } => {
                if self.state.rooms & room.bit() == 0 {
                    return self.done(kind, code::ROOM_UNAVAILABLE);
                }
                let what = if emote {
                    LineWhat::Emote
                } else {
                    LineWhat::Said
                };
                let me = self.me.clone();
                let l = self.line(room, what, &me, &text, NOW_MS + 1);
                self.done(kind, 0);
                self.out.push_back(l);
            }
            ChatOut::Whisper { to, text } => {
                let Some(peer) = self.find(&to) else {
                    return self.done(kind, code::UNKNOWN_PLAYER);
                };
                self.next_dm += 1;
                let account = if peer.account.is_empty() {
                    peer.char.clone()
                } else {
                    peer.account.clone()
                };
                let d = DmLine {
                    dm: self.next_dm,
                    peer_account: account,
                    peer_char: peer.char.clone(),
                    outgoing: true,
                    stored: false,
                    text,
                    at_ms: NOW_MS + 1,
                };
                self.dms.push(d.clone());
                self.done(kind, 0);
                self.out.push_back(ChatIn::Dm(d));
            }
            ChatOut::Conversations { .. } => {
                self.out
                    .push_back(ChatIn::Conversations(self.conversations.clone()));
            }
            ChatOut::History { peer, .. } => {
                let account = peer.trim_start_matches('*').to_string();
                let lines = self
                    .dms
                    .iter()
                    .filter(|d| d.peer_account.eq_ignore_ascii_case(&account))
                    .cloned()
                    .collect();
                self.out.push_back(ChatIn::History {
                    peer_account: account,
                    lines,
                });
            }
            ChatOut::Read { peer_account, .. } => {
                for c in &mut self.conversations {
                    if c.peer_account == peer_account {
                        self.state.dm_unread = self.state.dm_unread.saturating_sub(c.unread);
                        c.unread = 0;
                    }
                }
                self.out.push_back(ChatIn::State(self.state.clone()));
            }
            ChatOut::Users { room, watch } => {
                if !watch {
                    return;
                }
                let rows = match room {
                    Room::Guild => self.guild_users.clone(),
                    _ => self.users.clone(),
                };
                self.out.push_back(ChatIn::Users { room, rows });
            }
            ChatOut::Friend { .. } => self.done(kind, 0),
        }
    }

    fn poll(&mut self) -> Option<ChatIn> {
        self.out.pop_front()
    }
}
