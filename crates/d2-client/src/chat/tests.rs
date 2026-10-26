use super::commands::{parse, Command, MAX_TEXT};
use super::fake::{player, FakeChat};
use super::model::{ChatModel, Host, LineKind, Tab, PRESENCE_PAIR_MS};
use super::source::{kind, ChatIn, ChatOut, ChatSource, FriendOp, Room};
use super::{GameFeed, Merged, RealmChat};

fn lobby(fake: &mut FakeChat) -> ChatModel {
    let mut m = ChatModel::new(Host::Lobby);
    m.pump(fake, 0);
    m
}

/// Rule (contract 3.3): every command parses; an unknown one stays local.
#[test]
fn commands_parse() {
    let w = |to: &str, text: &str| Command::Whisper {
        to: to.into(),
        text: text.into(),
    };
    assert_eq!(parse("/w Kaern hi there"), w("Kaern", "hi there"));
    assert_eq!(parse("/whisper *nugwa yo"), w("*nugwa", "yo"));
    assert_eq!(parse("/W Kaern  hi "), w("Kaern", "hi"));
    assert_eq!(parse("/r ok"), Command::Reply("ok".into()));
    assert_eq!(parse("/g meet"), Command::Say(Room::Guild, "meet".into()));
    assert_eq!(parse("/global hi"), Command::Say(Room::Global, "hi".into()));
    assert_eq!(parse("/m hi"), Command::Say(Room::Mod, "hi".into()));
    assert_eq!(parse("/me waves"), Command::Emote("waves".into()));
    assert_eq!(
        parse("/ignore Spammer"),
        Command::Friend(FriendOp::Ignore, "Spammer".into())
    );
    assert_eq!(
        parse("/unignore Spammer"),
        Command::Friend(FriendOp::Unignore, "Spammer".into())
    );
    assert_eq!(
        parse("/f add Kaern"),
        Command::Friend(FriendOp::Add, "Kaern".into())
    );
    assert_eq!(
        parse("/f remove Kaern"),
        Command::Friend(FriendOp::Remove, "Kaern".into())
    );
    assert_eq!(parse("hello all"), Command::Plain("hello all".into()));
    assert_eq!(parse("   "), Command::Empty);
    assert_eq!(parse("/w Kaern"), Command::Usage("/w NAME TEXT"));
    assert_eq!(parse("/g"), Command::Usage("/g TEXT"));
    assert!(matches!(parse("/f list"), Command::Usage(_)));
    assert_eq!(parse("/dance now"), Command::Unknown("/dance".into()));
    // Text is cut to MAX_CHAT bytes at a character boundary.
    let long = "é".repeat(150);
    let Command::Plain(t) = parse(&long) else {
        panic!()
    };
    assert!(t.len() <= MAX_TEXT && t.chars().all(|c| c == 'é'));

    // An unknown command is never sent.
    let mut fake = FakeChat::seeded();
    let mut m = lobby(&mut fake);
    let before = fake.sent.len();
    m.input = "/dance now".into();
    m.submit();
    m.pump(&mut fake, 0);
    assert_eq!(fake.sent.len(), before);
    assert!(m
        .lines(Tab::Room(Room::Mod))
        .iter()
        .any(|l| l.kind == LineKind::System && l.text == "Unknown command: /dance"));
    // A plain line goes to the open tab; /g to the guild; /w and /r whisper.
    m.input = "hi all".into();
    m.submit();
    m.input = "/g guild line".into();
    m.submit();
    m.input = "/w Kaern psst".into();
    m.submit();
    m.pump(&mut fake, 0);
    let says: Vec<&ChatOut> = fake.sent[before..]
        .iter()
        .filter(|o| matches!(o, ChatOut::Say { .. } | ChatOut::Whisper { .. }))
        .collect();
    assert_eq!(
        says,
        [
            &ChatOut::Say {
                room: Room::Mod,
                emote: false,
                text: "hi all".into()
            },
            &ChatOut::Say {
                room: Room::Guild,
                emote: false,
                text: "guild line".into()
            },
            &ChatOut::Whisper {
                to: "Kaern".into(),
                text: "psst".into()
            },
        ]
    );
    fake.dm_from("Sparks", "hey");
    m.pump(&mut fake, 0);
    m.input = "/r yo".into();
    m.submit();
    m.pump(&mut fake, 0);
    assert_eq!(
        fake.sent.last(),
        Some(&ChatOut::Whisper {
            to: "*Sparks".into(),
            text: "yo".into()
        })
    );
}

/// Rule (L9): unread counts per tab and per DM conversation, cleared when
/// the tab opens; a DM conversation in view is marked read on the realm.
#[test]
fn unread_counts() {
    let mut fake = FakeChat::seeded();
    let mut m = lobby(&mut fake);
    // The preview: the backlog doesn't count, the three new guild lines
    // do, the DM conversation has one unread line.
    assert_eq!(m.open_tab_now(), Tab::Room(Room::Mod));
    assert_eq!(
        m.tabs(),
        [
            Tab::Room(Room::Global),
            Tab::Room(Room::Mod),
            Tab::Room(Room::Guild),
            Tab::Dm
        ]
    );
    assert_eq!(m.tab_label(Tab::Room(Room::Mod)), "Mod: D2Gamma");
    assert_eq!(m.tab_label(Tab::Room(Room::Guild)), "Guild: NUG");
    assert_eq!(m.unread(Tab::Room(Room::Global)), 0);
    assert_eq!(m.unread(Tab::Room(Room::Mod)), 0);
    assert_eq!(m.unread(Tab::Room(Room::Guild)), 3);
    assert_eq!(m.unread(Tab::Dm), 1);
    // The open room's users are watched.
    assert!(fake.sent.contains(&ChatOut::Users {
        room: Room::Mod,
        watch: true
    }));
    assert_eq!(m.users.count, 38);

    // A line in another room counts; one in the open room doesn't.
    let ironwolf = player("Ironwolf", 4, 52, None);
    fake.say_from(Room::Global, &ironwolf, "one");
    fake.say_from(Room::Mod, &ironwolf, "two");
    m.pump(&mut fake, 0);
    assert_eq!(m.unread(Tab::Room(Room::Global)), 1);
    assert_eq!(m.unread(Tab::Room(Room::Mod)), 0);
    m.open_tab(Tab::Room(Room::Guild));
    m.pump(&mut fake, 0);
    assert_eq!(m.unread(Tab::Room(Room::Guild)), 0);
    assert_eq!(m.users.count, 3);

    // A collapsed or hidden window counts lines of the open tab too.
    m.set_seen(false);
    fake.say_from(Room::Guild, &ironwolf, "three");
    m.pump(&mut fake, 0);
    assert_eq!(m.unread(Tab::Room(Room::Guild)), 1);
    m.set_seen(true);
    assert_eq!(m.unread(Tab::Room(Room::Guild)), 0);

    // Opening DM: the history of the unread conversation comes first, then
    // the read mark up to its newest line; the count clears.
    m.open_tab(Tab::Dm);
    m.pump(&mut fake, 0);
    assert!(fake.sent.contains(&ChatOut::Users {
        room: Room::Guild,
        watch: false
    }));
    assert!(fake.sent.contains(&ChatOut::Read {
        peer_account: "Kaern".into(),
        up_to_dm: 13
    }));
    assert_eq!(m.unread(Tab::Dm), 0);
    assert_eq!(m.lines(Tab::Dm).len(), 3);

    // A DM while another tab is open counts on its conversation.
    m.open_tab(Tab::Room(Room::Mod));
    fake.dm_from("Sparks", "hey");
    fake.dm_from("Sparks", "you there?");
    m.pump(&mut fake, 0);
    assert_eq!(m.unread(Tab::Dm), 2);
    assert_eq!(m.conversations[0].row.peer_account, "Sparks");
    assert_eq!(m.conversations[0].row.unread, 2);
    // Opening Kaern's conversation leaves Sparks' unread.
    m.open_conversation(Some("Kaern"));
    m.pump(&mut fake, 0);
    assert_eq!(m.unread(Tab::Dm), 2);
    m.open_conversation(Some("Sparks"));
    m.pump(&mut fake, 0);
    assert_eq!(m.unread(Tab::Dm), 0);
    assert_eq!(
        fake.sent.last(),
        Some(&ChatOut::Read {
            peer_account: "Sparks".into(),
            up_to_dm: 102
        })
    );
    // Typing in the conversation whispers its peer.
    m.input = "here".into();
    m.submit();
    m.pump(&mut fake, 0);
    assert!(fake.sent.contains(&ChatOut::Whisper {
        to: "*Sparks".into(),
        text: "here".into()
    }));
}

/// Rule (contract 3.1): a market notice goes to the Mod tab and to the
/// tab in view.
#[test]
fn routes_market_notice() {
    let mut fake = FakeChat::seeded();
    let mut m = lobby(&mut fake);
    let market = |m: &ChatModel, t: Tab| {
        m.lines(t)
            .iter()
            .filter(|l| l.kind == LineKind::Market)
            .count()
    };
    assert_eq!(market(&m, Tab::Room(Room::Mod)), 1);
    m.open_tab(Tab::Room(Room::Global));
    fake.market("Your Shako sold for 2 Um.");
    m.pump(&mut fake, 0);
    assert_eq!(market(&m, Tab::Room(Room::Mod)), 2);
    assert_eq!(market(&m, Tab::Room(Room::Global)), 1);
    assert_eq!(market(&m, Tab::Room(Room::Guild)), 0);
    assert_eq!(m.unread(Tab::Room(Room::Mod)), 1);
    // With Mod open it is printed once.
    m.open_tab(Tab::Room(Room::Mod));
    fake.market("Your Jah sold.");
    m.pump(&mut fake, 0);
    assert_eq!(market(&m, Tab::Room(Room::Mod)), 3);
    assert_eq!(market(&m, Tab::Room(Room::Global)), 1);
}

/// Owner (2026-10-23): the old chat goes; the Game tab shows the game's
/// player lines and its game and system messages, and what is said there
/// goes to the game stream.
#[test]
fn game_tab_shows_game_and_system_messages() {
    let mut src = Merged::new(Some(Box::new(FakeChat::seeded())));
    let mut m = ChatModel::new(Host::Game);
    m.pump(&mut src, 0);
    assert_eq!(m.tabs()[0], Tab::Room(Room::Game));
    assert_eq!(m.open_tab_now(), Tab::Room(Room::Game));
    src.game.message(
        "Kaern joined our world. Diablo's minions grow stronger.",
        4,
        5,
    );
    src.game.player_line("Kaern", "tp up", 6);
    m.pump(&mut src, 0);
    // The seeded market notice is printed in the tab in view too.
    let lines = m.lines(Tab::Room(Room::Game));
    assert_eq!(lines[0].kind, LineKind::Market);
    let lines = &lines[1..];
    assert_eq!(lines[0].kind, LineKind::Game(4));
    assert_eq!(lines[1].kind, LineKind::Said);
    assert_eq!(lines[1].who.char, "Kaern");
    // Said in the Game tab: to the game, not the realm.
    m.input = "omw".into();
    m.submit();
    m.input = "/m to the mod room".into();
    m.submit();
    m.pump(&mut src, 0);
    assert_eq!(src.game.take_said(), [(false, "omw".to_string())]);
    // Game messages don't count as unread; player lines do.
    m.open_tab(Tab::Room(Room::Mod));
    src.game.message("A quest is done.", 0, 7);
    src.game.player_line("Kaern", "come", 8);
    m.pump(&mut src, 0);
    assert_eq!(m.unread(Tab::Room(Room::Game)), 1);
    assert!(m
        .lines(Tab::Room(Room::Mod))
        .iter()
        .any(|l| l.text == "to the mod room"));

    // A game without a realm session: only the Game tab.
    let mut alone = GameFeed::default();
    let mut m = ChatModel::new(Host::Game);
    m.pump(&mut alone, 0);
    assert_eq!(m.tabs(), [Tab::Room(Room::Game)]);
}

/// The lobby's link to a realm without the social service: the hello is
/// refused, the chat falls back to today's one-room chat: one "Chat" tab,
/// no DMs and no user list; a line goes out as the lobby chat.
#[test]
fn one_room_realm() {
    use crate::ui::front_end::screens::online::link::Asked;
    use crate::ui::front_end::screens::online::{
        ErrorCode, Event, Failure, Online, OnlineLink, Request,
    };
    struct Seen(std::rc::Rc<std::cell::RefCell<Vec<Request>>>);
    impl OnlineLink for Seen {
        fn send(&mut self, r: Request) {
            self.0.borrow_mut().push(r);
        }
        fn poll(&mut self) -> Option<Event> {
            None
        }
    }
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let online = Online::new(Box::new(Seen(seen.clone())), Vec::new());
    let mut src = RealmChat::lobby(online);
    let mut m = ChatModel::new(Host::Lobby);
    m.pump(&mut src, 0);
    assert_eq!(*seen.borrow(), [Request::SocialHello { rooms: 0b111 }]);
    src.feed_event(&Event::Refused {
        asked: Asked::SocialHello,
        failure: Failure {
            code: ErrorCode::NOT_SUPPORTED,
            wait_s: 0,
            text: String::new(),
        },
    });
    m.pump(&mut src, 0);
    assert!(src.one_room());
    seen.borrow_mut().clear();
    assert_eq!(m.tabs(), [Tab::Room(Room::Mod)]);
    assert_eq!(m.tab_label(Tab::Room(Room::Mod)), "Chat");
    assert!(!m.users_ok());
    m.input = "hi".into();
    m.submit();
    m.pump(&mut src, 0);
    assert_eq!(*seen.borrow(), [Request::Chat { text: "hi".into() }]);
    src.feed_event(&Event::ChatLine {
        from: "Friend".into(),
        text: "hello there".into(),
    });
    m.pump(&mut src, 0);
    let l = m.lines(Tab::Room(Room::Mod));
    assert_eq!(
        (l[0].who.char.as_str(), l[0].text.as_str()),
        ("Friend", "hello there")
    );
    // A whisper is refused locally.
    m.input = "/w Friend hi".into();
    m.submit();
    m.pump(&mut src, 0);
    assert_eq!(seen.borrow().len(), 1);
    let _ = kind::WHISPER;
}

/// Regression (lobby plan L12; L10 found the lobby's chat never greeted
/// the social service): the lobby's link says hello to a realm with the
/// social service, so the realm's state (the guild for the character
/// block's `<TAG>`, the rooms, DMs) reaches the chat, and room lines go
/// out as `Say`. A friends-list answer the chat didn't ask (the Friends
/// panel's) leaves the chat alone.
#[test]
fn lobby_realm_speaks_social() {
    use crate::ui::front_end::screens::online::fake::{character, FakeServer};
    use crate::ui::front_end::screens::online::link;
    use crate::ui::front_end::screens::online::{Online, OnlineHandle, Request};
    use d2_formats::d2s::status;

    fn drain(online: &OnlineHandle, src: &mut RealmChat, m: &mut ChatModel) {
        for _ in 0..4 {
            m.pump(src, 0);
            let evs = online.borrow_mut().take_events();
            for e in &evs {
                src.feed_event(e);
            }
        }
        m.pump(src, 0);
    }
    let mut fake = FakeServer::default();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    let online = Online::new(Box::new(fake), Vec::new());
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
    let mut src = RealmChat::lobby(online.clone());
    let mut m = ChatModel::new(Host::Lobby);
    drain(&online, &mut src, &mut m);
    assert!(!src.one_room());
    let tag = m
        .state()
        .and_then(|s| s.guild.as_ref())
        .map(|g| g.tag.as_str());
    assert_eq!(tag, Some("NUG"));
    assert_eq!(
        m.tabs(),
        [
            Tab::Room(Room::Global),
            Tab::Room(Room::Mod),
            Tab::Room(Room::Guild),
            Tab::Dm
        ]
    );
    assert_eq!(m.tab_label(Tab::Room(Room::Mod)), "Mod: D2Gamma");
    // The backlog arrived.
    assert!(m
        .lines(Tab::Room(Room::Global))
        .iter()
        .any(|l| l.text == "anyone selling a Shako?"));
    // A line in the Mod tab goes out as `Say` and comes back as a room line.
    m.open_tab(Tab::Room(Room::Mod));
    m.input = "hi all".into();
    m.submit();
    drain(&online, &mut src, &mut m);
    assert!(m
        .lines(Tab::Room(Room::Mod))
        .iter()
        .any(|l| l.text == "hi all" && l.who.char == "Werwer"));
    // The panel's friends-list change: no note in the chat.
    online
        .borrow_mut()
        .send(Request::FriendEdit(link::FriendOp::Add("Grim".into())));
    drain(&online, &mut src, &mut m);
    let notes = |m: &ChatModel| {
        m.lines(Tab::Room(Room::Mod))
            .iter()
            .filter(|l| l.kind == LineKind::System && l.text.contains("friends list"))
            .count()
    };
    assert_eq!(notes(&m), 0);
    // The chat's own: noted.
    m.input = "/f add Kestrel".into();
    m.submit();
    drain(&online, &mut src, &mut m);
    assert!(m
        .lines(Tab::Room(Room::Mod))
        .iter()
        .any(|l| l.text == "Kestrel is on your friends list."));
}

/// Input history: Up walks back, Down forward and past the newest clears.
#[test]
fn input_history() {
    let mut fake = FakeChat::seeded();
    let mut m = lobby(&mut fake);
    for t in ["one", "two"] {
        m.input = t.into();
        m.submit();
    }
    m.history_prev();
    assert_eq!(m.input, "two");
    m.history_prev();
    m.history_prev();
    assert_eq!(m.input, "one");
    m.history_next();
    assert_eq!(m.input, "two");
    m.history_next();
    assert_eq!(m.input, "");
}

/// A failed request leaves a note in the tab in view.
#[test]
fn refusals_are_noted() {
    let mut fake = FakeChat::seeded();
    let mut m = lobby(&mut fake);
    m.input = "/w Nobody hi".into();
    m.submit();
    m.pump(&mut fake, 0);
    assert!(m
        .lines(Tab::Room(Room::Mod))
        .iter()
        .any(|l| l.kind == LineKind::System && l.text == "No such player."));
    m.input = "/f add Kaern".into();
    m.submit();
    m.pump(&mut fake, 0);
    assert!(m
        .lines(Tab::Room(Room::Mod))
        .iter()
        .any(|l| l.text == "Kaern is on your friends list."));
    let _ = fake.poll().map(|_: ChatIn| ());
}

/// Polish (the owner's live lobby): the player's own joined and left lines
/// (its reconnects, its lobby entries) are not shown; another player who
/// leaves and comes back within [`PRESENCE_PAIR_MS`] shows neither line, a
/// repeat of the same line once; a later return is news again.
#[test]
fn presence_lines_are_news_only() {
    let mut fake = FakeChat::seeded();
    let me = fake.me.clone();
    let t = super::fake::NOW_MS;
    for i in 0..3 {
        fake.fresh_presence(Room::Mod, &me, false, t + 10 + 4 * i);
        fake.fresh_presence(Room::Mod, &me, true, t + 12 + 4 * i);
    }
    let nugwa = player("Nugwa", 2, 77, None);
    fake.fresh_presence(Room::Mod, &nugwa, false, t + 30);
    fake.fresh_presence(Room::Mod, &nugwa, true, t + 31);
    let ironwolf = player("Ironwolf", 4, 52, None);
    fake.fresh_presence(Room::Mod, &ironwolf, true, t + 40);
    fake.fresh_presence(Room::Mod, &ironwolf, true, t + 41);
    let gravel = player("Gravel", 3, 61, None);
    fake.fresh_presence(Room::Mod, &gravel, false, t + 50);
    fake.fresh_presence(Room::Mod, &gravel, true, t + 51 + PRESENCE_PAIR_MS);
    let mut m = ChatModel::new(Host::Lobby);
    // The lobby names its character (any case).
    m.set_me(Some("werwer"));
    m.pump(&mut fake, 0);
    let presence: Vec<(String, LineKind)> = m
        .lines(Tab::Room(Room::Mod))
        .into_iter()
        .filter(|l| matches!(l.kind, LineKind::Joined | LineKind::Left))
        .map(|l| (l.who.char.clone(), l.kind))
        .collect();
    let line = |n: &str, k: LineKind| (n.to_string(), k);
    assert_eq!(
        presence,
        vec![
            line("Necrolord", LineKind::Joined),
            line("Ironwolf", LineKind::Joined),
            line("Gravel", LineKind::Left),
            line("Gravel", LineKind::Joined),
        ]
    );
}
