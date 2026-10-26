use super::*;
use crate::chat::source::Room;
use crate::chat::Tab;

#[test]
fn the_control_state_names_the_tabs_and_the_lines() {
    let mut w = ChatWindow::new(None);
    w.player_line("Bob", "hi");
    w.game_message("Ana joined our world.", 4);
    w.pump(0);
    let s = state(&w);
    assert_eq!(s["shown"], true);
    assert_eq!(s["realm"], false);
    assert_eq!(s["tab"], "game");
    assert_eq!(s["tabs"]["game"], 0);
    assert_eq!(
        s["lines"],
        serde_json::json!(["Bob: hi", "Ana joined our world."])
    );
    assert_eq!(s["input"], "");
    assert_eq!(w.model.open_tab_now(), Tab::Room(Room::Game));
}

/// The window's place is saved per player in `layouts.toml` and a new
/// window (the next game) takes it; another player's stays apart, and the
/// lobby's layout in the same file is kept.
#[test]
fn the_window_layout_is_saved_per_player_and_comes_back() {
    use crate::ui::float::SavedLayout;
    use crate::ui::layout::Screen;
    use crate::ui::panel::{PointerButton, UiEvent};
    let dir = std::env::temp_dir().join(format!("d2rust-chat-layout-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let lobby = LayoutKey::new("eu1", "Nugwa", Scene::Lobby);
    let mut store = LayoutStore::default();
    store.set(
        lobby.clone(),
        SavedLayout {
            locked: true,
            panels: Vec::new(),
        },
    );
    store.save(&dir).unwrap();

    let s = Screen::R800;
    let drawn = |w: &mut ChatWindow| {
        let mut out: Vec<crate::ui::draw::UiDraw> = Vec::new();
        w.draw(&s, None, Some(1), 0, &mut out);
        w.drawn().expect("drawn").window
    };
    let key = LayoutKey::new("eu1", "Nugwa", Scene::Game);
    let mut w = ChatWindow::new(None);
    let mut placed = false;
    place_window(&mut w, &key, &dir, &mut placed);
    assert!(placed);
    let home = drawn(&mut w);
    // Dragged by its strip.
    let button = PointerButton::Left;
    let from = crate::ui::geom::Point::new(home.x + 100, home.y + 10);
    let to = crate::ui::geom::Point::new(from.x + 60, from.y - 90);
    w.event(UiEvent::Press { button, at: from }, &s, None);
    w.event(UiEvent::CursorMoved(to), &s, None);
    w.event(UiEvent::Release { button, at: to }, &s, None);
    let moved = drawn(&mut w);
    assert_ne!(moved, home);
    place_window(&mut w, &key, &dir, &mut placed);
    let file = LayoutStore::load(&dir).unwrap();
    assert!(file.get(&key).and_then(|l| l.panel("chat")).is_some());
    assert!(file.get(&lobby).unwrap().locked, "the lobby's layout stays");

    // The next game: the window comes back where it was.
    let mut again = ChatWindow::new(None);
    let mut placed = false;
    place_window(&mut again, &key, &dir, &mut placed);
    assert_eq!(drawn(&mut again), moved);
    // Another account: the default spot.
    let mut other = ChatWindow::new(None);
    let mut placed = false;
    let key = LayoutKey::new("eu1", "Someone", Scene::Game);
    place_window(&mut other, &key, &dir, &mut placed);
    assert_eq!(drawn(&mut other), home);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The realm side over the in-game realm reader: requests go out in its
/// outbox as social messages, the realm's answers and pushes come back
/// from its inbox as the chat's.
#[cfg(feature = "online")]
mod realm {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use d2_net::msg::lobby::{LobbyEvent, LobbyRequest};
    use d2_net::msg::social::{self as wire, FromRealm};
    use d2_net::msg::{ErrorCode, ErrorMsg};

    use crate::app::realm_pump::{RealmPump, RealmWire};
    use crate::chat::source::{ChatIn, ChatOut, ChatSource, LineWhat, Room};

    use super::super::GameRealmChat;

    #[derive(Default)]
    struct Wire {
        incoming: VecDeque<Vec<u8>>,
        sent: Vec<Vec<u8>>,
        broken: bool,
    }

    impl RealmWire for Wire {
        type Error = String;

        fn send(&mut self, body: Vec<u8>) -> Result<(), String> {
            if self.broken {
                return Err("closed".into());
            }
            self.sent.push(body);
            Ok(())
        }

        fn poll(&mut self) -> Result<Option<Vec<u8>>, String> {
            if self.broken {
                return Err("closed".into());
            }
            Ok(self.incoming.pop_front())
        }
    }

    struct Rig {
        slot: Arc<Mutex<Option<Wire>>>,
        pump: RealmPump<Wire>,
        chat: GameRealmChat,
        now: Instant,
    }

    impl Rig {
        fn new() -> Rig {
            let slot = Arc::new(Mutex::new(Some(Wire::default())));
            let pump = RealmPump::new(slot.clone());
            let chat = GameRealmChat::new(pump.inbox().clone(), pump.outbox().clone());
            Rig {
                slot,
                pump,
                chat,
                now: Instant::now(),
            }
        }

        fn arrive(&mut self, bodies: &[Vec<u8>]) {
            let mut s = self.slot.lock().unwrap();
            s.as_mut().unwrap().incoming.extend(bodies.iter().cloned());
        }

        /// One frame of the reader: what the chat queued goes out, what
        /// arrived is routed.
        fn frame(&mut self) {
            self.now += Duration::from_millis(40);
            self.pump.step(self.now);
        }

        fn sent(&mut self) -> Vec<wire::Ask> {
            let mut s = self.slot.lock().unwrap();
            std::mem::take(&mut s.as_mut().unwrap().sent)
                .iter()
                .filter_map(|b| wire::ToRealm::parse(b).ok().map(|m| m.ask))
                .collect()
        }

        fn sent_raw(&mut self) -> Vec<Vec<u8>> {
            let mut s = self.slot.lock().unwrap();
            std::mem::take(&mut s.as_mut().unwrap().sent)
        }

        fn got(&mut self) -> Vec<ChatIn> {
            std::iter::from_fn(|| self.chat.poll()).collect()
        }
    }

    fn line(room: u8, n: u64, who: &str, text: &str) -> Vec<u8> {
        FromRealm::RoomLine(wire::RoomLine {
            room,
            line: n,
            what: wire::line_what::SAID,
            who: wire::Who {
                char: who.into(),
                class: wire::NO_CLASS,
                ..wire::Who::default()
            },
            text: text.into(),
            at_ms: 5,
        })
        .body()
    }

    #[test]
    fn the_hello_and_a_say_go_out_as_social_messages() {
        let mut r = Rig::new();
        r.chat.send(ChatOut::Hello { rooms: 7 });
        r.chat.send(ChatOut::Say {
            room: Room::Global,
            emote: false,
            text: "hi".into(),
        });
        r.frame();
        assert_eq!(
            r.sent(),
            [
                wire::Ask::Hello { rooms: 7 },
                wire::Ask::Say {
                    room: wire::room::GLOBAL,
                    emote: false,
                    text: "hi".into()
                }
            ]
        );
    }

    #[test]
    fn the_realms_state_and_lines_come_back_as_the_chats() {
        let mut r = Rig::new();
        r.chat.send(ChatOut::Hello { rooms: 7 });
        r.frame();
        r.sent();
        let state = wire::SocialState {
            req: 1,
            rooms: 0b111,
            mod_label: "D2Gamma".into(),
            dm_unread: 2,
            ..wire::SocialState::default()
        };
        r.arrive(&[
            FromRealm::State(state).body(),
            line(wire::room::MOD, 9, "Kaern", "wts shako"),
        ]);
        r.frame();
        let got = r.got();
        assert_eq!(got.len(), 2, "{got:?}");
        match &got[0] {
            ChatIn::State(s) => {
                assert_eq!(
                    (s.rooms, s.mod_label.as_str(), s.dm_unread),
                    (7, "D2Gamma", 2)
                );
            }
            m => panic!("{m:?}"),
        }
        match &got[1] {
            ChatIn::Line(l) => {
                assert_eq!(l.room, Room::Mod);
                assert_eq!((l.line, l.what), (9, LineWhat::Said));
                assert_eq!(
                    (l.who.char.as_str(), l.text.as_str()),
                    ("Kaern", "wts shako")
                );
            }
            m => panic!("{m:?}"),
        }
    }

    #[test]
    fn a_user_list_comes_back_for_the_room_it_asked() {
        let mut r = Rig::new();
        r.chat.send(ChatOut::Users {
            room: Room::Mod,
            watch: true,
        });
        r.frame();
        let req = match r.sent().as_slice() {
            [wire::Ask::UserList {
                room: 1,
                watch: true,
            }] => 1,
            s => panic!("{s:?}"),
        };
        r.arrive(&[
            FromRealm::ListBegin(wire::ListHead {
                req,
                what: wire::kind::USER_LIST,
                total: 1,
                rows: 1,
                ..wire::ListHead::default()
            })
            .body(),
            FromRealm::UserRow {
                req,
                who: wire::Who {
                    char: "Ana".into(),
                    ..wire::Who::default()
                },
                place: wire::place::IN_GAME,
            }
            .body(),
            FromRealm::ListEnd { req }.body(),
        ]);
        r.frame();
        match r.got().as_slice() {
            [ChatIn::Users { room, rows }] => {
                assert_eq!(*room, Room::Mod);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].who.char, "Ana");
            }
            g => panic!("{g:?}"),
        }
    }

    #[test]
    fn a_realm_without_the_social_service_speaks_its_one_room_chat() {
        let mut r = Rig::new();
        r.chat.send(ChatOut::Hello { rooms: 7 });
        r.frame();
        r.sent();
        r.arrive(&[ErrorMsg::new(wire::kind::HELLO, ErrorCode::NOT_SUPPORTED).body()]);
        r.frame();
        match r.got().as_slice() {
            [ChatIn::State(s)] => assert_eq!(s.rooms, Room::Mod.bit()),
            g => panic!("{g:?}"),
        }
        // A Mod line goes out as the one-room chat; the room's lines come
        // in as Mod lines.
        r.chat.send(ChatOut::Say {
            room: Room::Mod,
            emote: false,
            text: "hello".into(),
        });
        r.frame();
        let sent = r.sent_raw();
        assert_eq!(
            sent.iter()
                .map(|b| LobbyRequest::parse(b).unwrap())
                .collect::<Vec<_>>(),
            [LobbyRequest::Chat {
                text: "hello".into()
            }]
        );
        r.arrive(&[LobbyEvent::ChatLine {
            from: "Bob".into(),
            text: "yo".into(),
            at: 3,
        }
        .body()]);
        r.frame();
        match r.got().as_slice() {
            [ChatIn::Line(l)] => {
                assert_eq!(
                    (l.room, l.who.char.as_str(), l.text.as_str()),
                    (Room::Mod, "Bob", "yo")
                );
            }
            g => panic!("{g:?}"),
        }
        // Anything else is refused.
        r.chat.send(ChatOut::Conversations { page: 0 });
        match r.got().as_slice() {
            [ChatIn::Done(d)] => assert_eq!(d.code, crate::chat::source::code::NOT_SUPPORTED),
            g => panic!("{g:?}"),
        }
    }

    #[test]
    fn a_lost_session_says_hello_again() {
        let mut r = Rig::new();
        r.chat.send(ChatOut::Hello { rooms: 7 });
        r.frame();
        assert_eq!(r.sent().len(), 1);
        r.slot.lock().unwrap().as_mut().unwrap().broken = true;
        r.frame();
        r.slot.lock().unwrap().as_mut().unwrap().broken = false;
        // The chat sees the loss at its next read and waits for the
        // session: nothing goes out while it is down.
        assert!(r.got().is_empty());
        r.frame();
        assert!(r.sent().is_empty());
        // Back (the reader tried it again): the hello goes again.
        r.now += crate::app::realm_pump::RETRY;
        r.frame();
        assert!(r.got().is_empty());
        r.frame();
        assert_eq!(r.sent(), [wire::Ask::Hello { rooms: 7 }]);
    }
}
