// D2RUST's own: the in-game chat window's frame step and its realm side.
//! Each frame the chat window learns whether the game is online
//! ([`crate::ui::original::OriginalUi::pump_chat`]): a game on a realm (its
//! link is a realm's) has the window, single player has none (the top-left
//! message list stays). The window's Game tab is the game stream's own
//! chat; its realm tabs (Global, Mod, Guild, DM) read the in-game realm
//! reader ([`crate::app::realm_pump`]): [`GameRealmChat`] (online builds)
//! takes the reader's social queue and sends through its outbox, with the
//! realm's social client ([`d2_online::SocialClient`]) numbering the
//! requests and putting the lists together.
//!
//! A realm without the social service refuses the hello: the chat then
//! speaks the realm's one-room chat (`Chat` 0x49 / `ChatLine` 0x4A, lobby
//! contract section 7.1) as the Mod room, as the lobby does.
//!
//! The window's place is saved per player: in `layouts.toml` beside
//! `settings.toml` ([`crate::ui::float::store`]), under the realm, the
//! account ([`LayoutOwner`]) and the scene `game`. A new window takes the
//! saved one; a moved, resized, locked or reset window is written back.
//!
//! Tests and offscreen shots turn the in-process fake realm chat on with
//! `D2RUST_CHAT=fake` ([`crate::chat::FakeChat`]): the game counts as
//! online and the fake answers (its layout is saved as realm and account
//! `fake`).

use bevy::log::warn;
use bevy::prelude::{App, Local, NonSendMut, Res, Resource, Update};

use crate::app::config::ConfigRes;
use crate::app::realm_pump::{RealmInbox, RealmOutbox};
use crate::bridge::link::ServerLink;
use crate::bridge::BridgeResource;
use crate::chat::{ChatSource, FakeChat};
use crate::ui::float::{LayoutKey, LayoutStore, Scene};
use crate::ui::original::chat_window::{tab_name, ChatWindow};
use crate::world_view::present::WorldViewUi;

/// The environment variable that turns the fake realm chat on.
pub const ENV: &str = "D2RUST_CHAT";

/// Whether `D2RUST_CHAT=fake` asks for the fake realm chat.
pub fn fake_wanted() -> bool {
    crate::launch::env_os(ENV).is_some_and(|v| v == "fake")
}

/// Whose chat window layout a game saves: the realm (its server id) and
/// the account. The online game's install puts it in.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct LayoutOwner {
    pub realm: String,
    pub account: String,
}

/// The owner of the fake realm chat's layout.
pub const FAKE_OWNER: &str = "fake";

/// The chat window's frame system, once per app.
pub fn register_chat(app: &mut App) {
    app.add_systems(Update, run_chat);
}

#[allow(clippy::too_many_arguments)]
fn run_chat(
    bridge: Option<Res<BridgeResource>>,
    ui: Option<NonSendMut<WorldViewUi>>,
    inbox: Option<Res<RealmInbox>>,
    outbox: Option<Res<RealmOutbox>>,
    config: Option<Res<ConfigRes>>,
    owner: Option<Res<LayoutOwner>>,
    mut fake: Local<Option<bool>>,
    mut placed: Local<bool>,
) {
    let Some(mut ui) = ui else {
        return;
    };
    let Some(o) = ui.original.as_mut() else {
        return;
    };
    let fake = *fake.get_or_insert_with(fake_wanted);
    let remote = bridge.as_ref().is_some_and(|b| b.0.link().is_remote());
    let mut realm = || -> Option<Box<dyn ChatSource>> {
        if fake {
            return Some(Box::new(FakeChat::seeded()));
        }
        realm_source(inbox.as_deref()?, outbox.as_deref()?)
    };
    o.pump_chat(remote || fake, &mut realm);
    let Some(mut win) = o.chat_window_mut() else {
        *placed = false;
        return;
    };
    let key = match (owner.as_deref(), fake) {
        (Some(o), _) => LayoutKey::new(&o.realm, &o.account, Scene::Game),
        (None, true) => LayoutKey::new(FAKE_OWNER, FAKE_OWNER, Scene::Game),
        (None, false) => return,
    };
    let Some(dir) = config.as_ref().map(|c| c.dir.as_path()) else {
        return;
    };
    place_window(&mut win, &key, dir, &mut placed);
}

/// A new window (`placed` false) takes the layout saved under `key` in
/// `dir`; a changed one is saved there. The file is read again before a
/// save, so the lobby's layouts in it stay; a damaged file is left as it
/// is (the window keeps its place for this game).
fn place_window(win: &mut ChatWindow, key: &LayoutKey, dir: &std::path::Path, placed: &mut bool) {
    if !std::mem::replace(placed, true) {
        match LayoutStore::load(dir) {
            Ok(store) => {
                if let Some(l) = store.get(key) {
                    win.apply_layout(l.clone());
                }
            }
            Err(e) => warn!("chat window: layouts not read: {e}"),
        }
    }
    if let Some(l) = win.take_layout() {
        let saved = LayoutStore::load(dir).and_then(|mut store| {
            store.set(key.clone(), l);
            store.save(dir)
        });
        if let Err(e) = saved {
            warn!("chat window: layout not saved: {e}");
        }
    }
}

/// The realm's chat over the in-game realm reader (online builds).
#[cfg(feature = "online")]
fn realm_source(inbox: &RealmInbox, outbox: &RealmOutbox) -> Option<Box<dyn ChatSource>> {
    Some(Box::new(GameRealmChat::new(inbox.clone(), outbox.clone())))
}

/// No realm client in this build: only the Game tab.
#[cfg(not(feature = "online"))]
fn realm_source(_: &RealmInbox, _: &RealmOutbox) -> Option<Box<dyn ChatSource>> {
    None
}

/// The chat window in a control `state` reply: what a driver checks
/// (shown, collapsed, typing, idle, the tabs with their unread counts,
/// the open tab's lines, the input line).
pub fn state(win: &ChatWindow) -> serde_json::Value {
    let m = &win.model;
    let open = m.open_tab_now();
    let tabs: serde_json::Map<String, serde_json::Value> = m
        .tabs()
        .into_iter()
        .map(|t| (tab_name(t), m.unread(t).into()))
        .collect();
    let lines: Vec<String> = m
        .lines(open)
        .iter()
        .map(|l| match l.who.char.as_str() {
            "" => l.text.clone(),
            who => format!("{who}: {}", l.text),
        })
        .collect();
    let rect = |r: crate::ui::geom::Rect| serde_json::json!([r.x, r.y, r.w, r.h]);
    let drawn = win.drawn();
    serde_json::json!({
        "shown": win.shown,
        "collapsed": win.collapsed,
        "window": drawn.map(|p| rect(p.window)),
        "bar": drawn.map(|p| rect(p.bar)),
        "moved": win.moved(),
        "locked": win.locked(),
        "menu": win.menu,
        "typing": win.focused,
        "idle": win.idle(win.now_ms()),
        "hover": win.hover,
        "now_ms": win.now_ms(),
        "active_ms": win.active_ms(),
        "realm": win.has_realm(),
        "tab": tab_name(open),
        "tabs": tabs,
        "lines": lines,
        "input": m.input,
    })
}

#[cfg(feature = "online")]
pub use realm::GameRealmChat;

#[cfg(feature = "online")]
mod realm {
    use std::collections::{BTreeMap, VecDeque};

    use d2_net::msg::guild::{self, FriendEdit};
    use d2_net::msg::lobby::{LobbyEvent, LobbyRequest};
    use d2_net::msg::social as wire;
    use d2_net::msg::ErrorMsg;
    use d2_online::{SocialAnswer, SocialAsk, SocialClient};

    use crate::app::realm_pump::{Owner, RealmInbox, RealmOutbox};
    use crate::chat::source::{
        code, kind, ChatIn, ChatOut, ChatSource, DmConversation, DmLine, Done, FriendOp,
        GuildBrief, LineWhat, PresenceOp, Room, RoomLine, SocialState, UserRow, Where, Who,
    };

    /// The realm's chat in a game: the reader's social queue in, its
    /// outbox out (module doc).
    pub struct GameRealmChat {
        client: SocialClient,
        inbox: RealmInbox,
        outbox: RealmOutbox,
        /// The link's failures seen: a new one loses what was in flight
        /// and the hello goes again once the session is back.
        losses: u64,
        /// A failure seen, the session not back yet.
        lost: bool,
        /// The rooms of the last hello.
        hello: Option<u16>,
        /// The realm has no social service: its one-room chat is the Mod
        /// room.
        one_room: bool,
        /// The one-room chat's line numbers.
        line: u64,
        /// What a history or user list asked, by request number.
        histories: BTreeMap<u32, String>,
        users: BTreeMap<u32, Room>,
        out: VecDeque<ChatIn>,
    }

    impl GameRealmChat {
        pub fn new(inbox: RealmInbox, outbox: RealmOutbox) -> GameRealmChat {
            let losses = inbox.link().losses;
            GameRealmChat {
                client: SocialClient::new(),
                inbox,
                outbox,
                losses,
                lost: false,
                hello: None,
                one_room: false,
                line: 0,
                histories: BTreeMap::new(),
                users: BTreeMap::new(),
                out: VecDeque::new(),
            }
        }

        fn ask(&mut self, ask: impl Into<SocialAsk>) -> u32 {
            let (req, body) = self.client.prepare(ask);
            self.outbox.send(body);
            req
        }

        fn refuse(&mut self, request: u8) {
            self.out.push_back(ChatIn::Done(Done {
                request,
                code: code::NOT_SUPPORTED,
                ..Done::default()
            }));
        }

        /// The one-room chat's hello answer: the Mod room only.
        fn one_room_state(&mut self) {
            self.out.push_back(ChatIn::State(SocialState {
                rooms: Room::Mod.bit(),
                ..SocialState::default()
            }));
        }

        /// Reads what the reader holds for the chat.
        fn step(&mut self) {
            let link = self.inbox.link();
            if link.losses != self.losses {
                // What was in flight is lost with the session.
                self.losses = link.losses;
                self.lost = true;
                self.client.reset();
                self.histories.clear();
                self.users.clear();
            }
            if self.lost {
                // What arrives meanwhile answers asks that are lost; what
                // is sent meanwhile is dropped by the reader.
                self.inbox.take(Owner::Social);
                if link.down {
                    return;
                }
                // A session again (a new one): the hello goes again (the
                // social service may be there now).
                self.lost = false;
                self.one_room = false;
                if let Some(rooms) = self.hello {
                    self.ask(wire::Ask::Hello { rooms });
                }
            }
            for body in self.inbox.take(Owner::Social) {
                match self.client.feed(&body) {
                    Ok(true) => {}
                    Ok(false) => self.one_room_body(&body),
                    // A social body that doesn't read: dropped.
                    Err(_) => {}
                }
            }
            for a in self.client.take() {
                self.answer(a);
            }
        }

        /// A body of the realm's one-room chat: a line, or its refusal.
        fn one_room_body(&mut self, body: &[u8]) {
            if let Ok(LobbyEvent::ChatLine { from, text, at }) = LobbyEvent::parse(body) {
                self.line += 1;
                self.out.push_back(ChatIn::Line(RoomLine {
                    room: Room::Mod,
                    line: self.line,
                    what: LineWhat::Said,
                    who: Who::named(&from),
                    text,
                    at_ms: at,
                }));
            } else if let Ok(e) = ErrorMsg::parse(body) {
                self.out.push_back(ChatIn::Done(Done {
                    request: kind::SAY,
                    code: e.code.0,
                    wait_s: e.wait_s,
                    text: e.text,
                    ..Done::default()
                }));
            }
        }

        fn answer(&mut self, a: SocialAnswer) {
            let m = match a {
                SocialAnswer::State(s) => ChatIn::State(state(s)),
                SocialAnswer::RoomLine(l) => match Room::from_number(l.room) {
                    Some(room) if room != Room::Game => ChatIn::Line(RoomLine {
                        room,
                        line: l.line,
                        what: line_what(l.what),
                        who: who(l.who),
                        text: l.text,
                        at_ms: l.at_ms,
                    }),
                    _ => return,
                },
                SocialAnswer::Dm(d) => ChatIn::Dm(dm(d)),
                SocialAnswer::Presence(p) => match Room::from_number(p.room) {
                    Some(room) => ChatIn::Presence {
                        room,
                        op: match p.op {
                            wire::presence_op::JOINED => PresenceOp::Joined,
                            wire::presence_op::LEFT => PresenceOp::Left,
                            _ => PresenceOp::Changed,
                        },
                        row: UserRow {
                            who: who(p.who),
                            place: place(p.place),
                        },
                        count: p.count,
                    },
                    None => return,
                },
                SocialAnswer::Done(d) => ChatIn::Done(Done {
                    request: d.request_kind,
                    code: d.code.0,
                    wait_s: d.wait_s,
                    stored: d.flags & wire::done_flags::STORED != 0,
                    text: d.text,
                }),
                SocialAnswer::Error { error, .. } => {
                    if error.request_kind == kind::SOCIAL_HELLO
                        && error.code.0 == code::NOT_SUPPORTED
                    {
                        self.one_room = true;
                        self.one_room_state();
                        return;
                    }
                    ChatIn::Done(Done {
                        request: error.request_kind,
                        code: error.code.0,
                        wait_s: error.wait_s,
                        text: error.text,
                        ..Done::default()
                    })
                }
                SocialAnswer::DmConversations { rows, .. } => ChatIn::Conversations(
                    rows.into_iter()
                        .map(|r| DmConversation {
                            peer_account: r.peer_account,
                            peer_char: r.peer_char,
                            place: place(r.place),
                            unread: r.unread,
                            last_ms: r.last_ms,
                            last_text: r.last_text,
                        })
                        .collect(),
                ),
                SocialAnswer::DmHistory { req, lines, .. } => {
                    let asked = self.histories.remove(&req).unwrap_or_default();
                    let peer_account = lines.first().map_or_else(
                        || asked.trim_start_matches('*').to_string(),
                        |l| l.peer_account.clone(),
                    );
                    ChatIn::History {
                        peer_account,
                        lines: lines.into_iter().map(dm).collect(),
                    }
                }
                SocialAnswer::Users { req, rows, .. } => {
                    let Some(room) = self.users.remove(&req) else {
                        return;
                    };
                    ChatIn::Users {
                        room,
                        rows: rows
                            .into_iter()
                            .map(|r| UserRow {
                                who: who(r.who),
                                place: place(r.place),
                            })
                            .collect(),
                    }
                }
                // The guild's and the friends' own windows read these.
                _ => return,
            };
            self.out.push_back(m);
        }
    }

    impl ChatSource for GameRealmChat {
        fn send(&mut self, out: ChatOut) {
            if let ChatOut::Hello { rooms } = out {
                self.hello = Some(rooms);
            }
            if self.one_room {
                match out {
                    ChatOut::Hello { .. } => self.one_room_state(),
                    ChatOut::Say {
                        room: Room::Mod,
                        emote: false,
                        text,
                    } => self.outbox.send(LobbyRequest::Chat { text }.body()),
                    other => self.refuse(other.kind()),
                }
                return;
            }
            match out {
                ChatOut::Hello { rooms } => {
                    self.ask(wire::Ask::Hello { rooms });
                }
                ChatOut::Say { room, emote, text } => {
                    self.ask(wire::Ask::Say {
                        room: room.number(),
                        emote,
                        text,
                    });
                }
                ChatOut::Whisper { to, text } => {
                    self.ask(wire::Ask::Whisper { to, text });
                }
                ChatOut::Conversations { page } => {
                    self.ask(wire::Ask::DmConversations { page });
                }
                ChatOut::History {
                    peer,
                    before_dm,
                    rows,
                } => {
                    let req = self.ask(wire::Ask::DmHistory {
                        peer: peer.clone(),
                        before_dm,
                        rows,
                    });
                    self.histories.insert(req, peer);
                }
                ChatOut::Read {
                    peer_account,
                    up_to_dm,
                } => {
                    self.ask(wire::Ask::DmRead {
                        peer_account,
                        up_to_dm,
                    });
                }
                ChatOut::Users { room, watch } => {
                    let req = self.ask(wire::Ask::UserList {
                        room: room.number(),
                        watch,
                    });
                    self.users.insert(req, room);
                }
                ChatOut::Friend { op, who } => {
                    let edit = match op {
                        FriendOp::Add => FriendEdit::Add(who),
                        FriendOp::Remove => FriendEdit::Remove(who),
                        FriendOp::Ignore => FriendEdit::Ignore(who),
                        FriendOp::Unignore => FriendEdit::Unignore(who),
                    };
                    self.ask(guild::Ask::FriendEdit(edit));
                }
            }
        }

        fn poll(&mut self) -> Option<ChatIn> {
            if self.out.is_empty() {
                self.step();
            }
            self.out.pop_front()
        }
    }

    fn state(s: wire::SocialState) -> SocialState {
        SocialState {
            rooms: s.rooms,
            mod_label: s.mod_label,
            guild: s.guild.map(|g| GuildBrief {
                guild: g.guild,
                tag: g.tag,
                name: g.name,
                my_rank: g.my_rank,
            }),
            dm_unread: s.dm_unread,
            muted_until_ms: s.muted_until_ms,
            now_ms: s.now_ms,
        }
    }

    fn who(w: wire::Who) -> Who {
        Who {
            char: w.char,
            account: w.account,
            tag: w.tag,
            class: w.class,
            level: w.level,
            flags: w.flags,
        }
    }

    fn dm(d: wire::DmLine) -> DmLine {
        DmLine {
            dm: d.dm,
            peer_account: d.peer_account,
            peer_char: d.peer_char,
            outgoing: d.outgoing,
            stored: d.stored,
            text: d.text,
            at_ms: d.at_ms,
        }
    }

    fn line_what(w: u8) -> LineWhat {
        match w {
            wire::line_what::EMOTE => LineWhat::Emote,
            wire::line_what::JOINED => LineWhat::Joined,
            wire::line_what::LEFT => LineWhat::Left,
            wire::line_what::REALM_NOTICE => LineWhat::Notice,
            wire::line_what::GUILD_EVENT => LineWhat::GuildEvent,
            _ => LineWhat::Said,
        }
    }

    fn place(p: u8) -> Where {
        match p {
            wire::place::LOBBY => Where::Lobby,
            wire::place::IN_GAME => Where::InGame,
            wire::place::HIDDEN => Where::Hidden,
            _ => Where::Offline,
        }
    }
}

#[cfg(test)]
#[path = "chat_tests.rs"]
mod tests;
