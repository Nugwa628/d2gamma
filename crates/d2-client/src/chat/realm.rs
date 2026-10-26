//! The realm as a chat source. [`RealmChat`] is a queue of what came from
//! the realm ([`RealmChat::feed`]) and a sink for what goes there: the host
//! wires the sink to its realm link (the in-game realm pump's outbox) and
//! feeds it the realm's chat messages. A request the sink can't carry is
//! answered `NotSupported` here, so the model hides what the realm doesn't
//! have.
//!
//! The lobby's link ([`RealmChat::lobby`]) carries the social service of
//! the lobby contract (section 3): the hello, rooms, DMs and user lists go
//! out as the link's requests, and the lobby hands its events back through
//! [`RealmChat::feed_event`]. A realm without the social service refuses
//! the hello: the chat then speaks today's one-room lobby chat (`Chat`
//! 0x49 / `ChatLine` 0x4A, contract section 7.1) as the Mod room (labelled
//! "Chat") with no DMs or user lists, as the in-game chat does.
//!
//! The lobby's Friends panel shares the link: a friends-list change the
//! chat didn't ask (`/f add`, `/ignore`) is the panel's, so the chat counts
//! its own and leaves the other answers alone.
//!
//! [`OnlineLink`]: crate::ui::front_end::screens::online::OnlineLink

use std::collections::VecDeque;

use super::source::{
    code, kind, ChatIn, ChatOut, ChatSource, DmConversation, DmLine, Done, FriendOp, GuildBrief,
    LineWhat, PresenceOp, Room, RoomLine, SocialState, UserRow, Where, Who,
};
use crate::ui::front_end::screens::online::link::{self, line_what, presence_op, Asked, ErrorCode};
use crate::ui::front_end::screens::online::{Event, OnlineHandle, Request};

/// Hands a request to the realm link; `false`: the link can't carry it.
pub type Sink = Box<dyn FnMut(&ChatOut) -> bool>;

pub struct RealmChat {
    sink: Sink,
    inbox: VecDeque<ChatIn>,
    /// The lobby's link: requests go out as its [`Request`]s.
    link: Option<OnlineHandle>,
    /// The realm refused the hello: it speaks only the one-room lobby chat.
    one_room: bool,
    /// Friends-list changes the chat sent whose answers haven't come.
    friend_edits: u32,
    line: u64,
}

impl RealmChat {
    /// A realm source over `sink`, fed by the host.
    pub fn new(sink: impl FnMut(&ChatOut) -> bool + 'static) -> RealmChat {
        RealmChat {
            sink: Box::new(sink),
            inbox: VecDeque::new(),
            link: None,
            one_room: false,
            friend_edits: 0,
            line: 0,
        }
    }

    /// The lobby's link: the social service's requests go out on it; its
    /// events come in through [`RealmChat::feed_event`].
    pub fn lobby(online: OnlineHandle) -> RealmChat {
        RealmChat {
            link: Some(online),
            ..RealmChat::new(|_| false)
        }
    }

    /// A message from the realm.
    pub fn feed(&mut self, m: ChatIn) {
        self.inbox.push_back(m);
    }

    /// The realm speaks only the one-room chat (it refused the hello).
    pub fn one_room(&self) -> bool {
        self.one_room
    }

    /// One of the lobby link's events: the chat's own, mapped; the rest
    /// (another screen's answers, the game list) is left alone.
    pub fn feed_event(&mut self, ev: &Event) {
        let m = match ev {
            Event::ChatLine { from, text } => {
                self.line += 1;
                ChatIn::Line(RoomLine {
                    room: Room::Mod,
                    line: self.line,
                    what: LineWhat::Said,
                    who: Who::named(from),
                    text: text.clone(),
                    at_ms: 0,
                })
            }
            Event::SocialState(s) => ChatIn::State(state(s)),
            Event::RoomLine(l) => match Room::from_number(l.room) {
                Some(room) if room != Room::Game => ChatIn::Line(RoomLine {
                    room,
                    line: l.line,
                    what: what(l.what),
                    who: who(&l.who),
                    text: l.text.clone(),
                    at_ms: l.at_ms,
                }),
                _ => return,
            },
            Event::DmLine(d) => ChatIn::Dm(dm(d)),
            Event::DmConversations { rows, .. } => ChatIn::Conversations(
                rows.iter()
                    .map(|c| DmConversation {
                        peer_account: c.peer_account.clone(),
                        peer_char: c.peer_char.clone(),
                        place: place(c.place),
                        unread: c.unread,
                        last_ms: c.last_ms,
                        last_text: c.last_text.clone(),
                    })
                    .collect(),
            ),
            Event::DmHistory { peer, lines } => ChatIn::History {
                peer_account: lines.first().map_or_else(
                    || peer.trim_start_matches('*').to_string(),
                    |l| l.peer_account.clone(),
                ),
                lines: lines.iter().map(dm).collect(),
            },
            Event::Users { room, rows, .. } => match Room::from_number(*room) {
                Some(room) => ChatIn::Users {
                    room,
                    rows: rows.iter().map(user).collect(),
                },
                None => return,
            },
            Event::RoomPresence(p) => match Room::from_number(p.room) {
                Some(room) => ChatIn::Presence {
                    room,
                    op: match p.op {
                        presence_op::JOINED => PresenceOp::Joined,
                        presence_op::LEFT => PresenceOp::Left,
                        _ => PresenceOp::Changed,
                    },
                    row: UserRow {
                        who: who(&p.who),
                        place: place(p.place),
                    },
                    count: p.count,
                },
                None => return,
            },
            Event::Done(d) => match self.mine(d.asked) {
                Some(request) => ChatIn::Done(Done {
                    request,
                    stored: d.stored(),
                    ..Done::default()
                }),
                None => return,
            },
            Event::Refused { asked, failure }
                if *asked == Asked::SocialHello && failure.code == ErrorCode::NOT_SUPPORTED =>
            {
                self.one_room = true;
                one_room_state()
            }
            Event::Refused { asked, failure } => match self.mine(*asked) {
                Some(request) => ChatIn::Done(Done {
                    request,
                    code: failure.code.0,
                    wait_s: failure.wait_s,
                    stored: false,
                    text: failure.text.clone(),
                }),
                None => return,
            },
            _ => return,
        };
        self.feed(m);
    }

    /// The chat's request kind an answer belongs to (`None`: another
    /// screen's). Only the chat sends the social requests; a friends-list
    /// change is the chat's while one of its own is out.
    fn mine(&mut self, asked: Asked) -> Option<u8> {
        Some(match asked {
            Asked::SocialHello => kind::SOCIAL_HELLO,
            Asked::Say => kind::SAY,
            Asked::Whisper => kind::WHISPER,
            Asked::DmConversations => kind::DM_CONVERSATIONS,
            Asked::DmHistory => kind::DM_HISTORY,
            Asked::DmRead => kind::DM_READ,
            Asked::UserList => kind::USER_LIST,
            Asked::FriendEdit if self.friend_edits > 0 => {
                self.friend_edits -= 1;
                kind::FRIEND_EDIT
            }
            _ => return None,
        })
    }

    /// A request on the lobby's link.
    fn send_lobby(&mut self, link: &OnlineHandle, out: ChatOut) {
        if let ChatOut::Hello { .. } = out {
            // A new hello asks the social service again (another
            // character, or another realm).
            self.one_room = false;
        }
        if self.one_room {
            match out {
                ChatOut::Say {
                    room: Room::Mod,
                    emote: false,
                    text,
                } => link.borrow_mut().send(Request::Chat { text }),
                other => self.refuse(other.kind()),
            }
            return;
        }
        let r = match out {
            ChatOut::Hello { rooms } => Request::SocialHello { rooms },
            ChatOut::Say {
                room: Room::Game, ..
            } => return self.refuse(kind::SAY),
            ChatOut::Say { room, emote, text } => Request::Say {
                room: room.number(),
                emote,
                text,
            },
            ChatOut::Whisper { to, text } => Request::Whisper { to, text },
            ChatOut::Conversations { page } => Request::DmConversations { page },
            ChatOut::History {
                peer,
                before_dm,
                rows,
            } => Request::DmHistory {
                peer,
                before_dm,
                rows,
            },
            ChatOut::Read {
                peer_account,
                up_to_dm,
            } => Request::DmRead {
                peer_account,
                up_to_dm,
            },
            ChatOut::Users { room, watch } => Request::UserList {
                room: room.number(),
                watch,
            },
            ChatOut::Friend { op, who } => {
                self.friend_edits += 1;
                Request::FriendEdit(match op {
                    FriendOp::Add => link::FriendOp::Add(who),
                    FriendOp::Remove => link::FriendOp::Remove(who),
                    FriendOp::Ignore => link::FriendOp::Ignore(who),
                    FriendOp::Unignore => link::FriendOp::Unignore(who),
                })
            }
        };
        link.borrow_mut().send(r);
    }

    fn refuse(&mut self, request: u8) {
        self.feed(ChatIn::Done(Done {
            request,
            code: code::NOT_SUPPORTED,
            ..Done::default()
        }));
    }
}

impl ChatSource for RealmChat {
    fn send(&mut self, out: ChatOut) {
        if let Some(link) = self.link.clone() {
            return self.send_lobby(&link, out);
        }
        if !(self.sink)(&out) {
            self.refuse(out.kind());
        }
    }

    fn poll(&mut self) -> Option<ChatIn> {
        self.inbox.pop_front()
    }
}

/// The one-room chat's hello answer: the Mod room only.
fn one_room_state() -> ChatIn {
    ChatIn::State(SocialState {
        rooms: Room::Mod.bit(),
        ..SocialState::default()
    })
}

fn state(s: &link::SocialState) -> SocialState {
    SocialState {
        rooms: s.rooms,
        mod_label: s.mod_label.clone(),
        guild: s.guild.as_ref().map(|g| GuildBrief {
            guild: g.guild,
            tag: g.tag.clone(),
            name: g.name.clone(),
            my_rank: g.my_rank,
        }),
        dm_unread: s.dm_unread,
        muted_until_ms: s.muted_until_ms,
        now_ms: s.now_ms,
    }
}

fn who(w: &link::Who) -> Who {
    Who {
        char: w.char.clone(),
        account: w.account.clone(),
        tag: w.tag.clone(),
        class: w.class,
        level: w.level,
        flags: w.flags,
    }
}

fn place(p: link::Where) -> Where {
    match p {
        link::Where::Offline => Where::Offline,
        link::Where::Lobby => Where::Lobby,
        link::Where::InGame => Where::InGame,
        link::Where::Hidden => Where::Hidden,
    }
}

fn user(u: &link::UserRow) -> UserRow {
    UserRow {
        who: who(&u.who),
        place: place(u.place),
    }
}

fn what(w: u8) -> LineWhat {
    match w {
        line_what::EMOTE => LineWhat::Emote,
        line_what::JOINED => LineWhat::Joined,
        line_what::LEFT => LineWhat::Left,
        line_what::NOTICE => LineWhat::Notice,
        line_what::GUILD_EVENT => LineWhat::GuildEvent,
        _ => LineWhat::Said,
    }
}

fn dm(d: &link::DmLine) -> DmLine {
    DmLine {
        dm: d.dm,
        peer_account: d.peer_account.clone(),
        peer_char: d.peer_char.clone(),
        outgoing: d.outgoing,
        stored: d.stored,
        text: d.text.clone(),
        at_ms: d.at_ms,
    }
}
