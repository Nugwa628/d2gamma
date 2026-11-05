//! A mod game's lobby screen reaches the realm through the front end's
//! `FrontStart::realm()`: it connects, logs in, picks a character, lists
//! and creates games, chats and joins, with the engine's own lobby
//! protocol on the in-memory test realm the engine's lobby tests use. A
//! join ends the front end in an online game load whose ticket is the
//! engine's (`Online::take_joined`). No game files.

use std::cell::RefCell;

use d2_client::mods::front_hand_back::FrontStart;
use d2_client::mods::front_realm_link::{Event, RealmLink, Request};
use d2_client::mods::ui_hooks::FrontScreenFactory;
use d2_client::mods::ClientMods;
use d2_client::ui::front_end::screens::online::fake::{character, SharedFake};
use d2_client::ui::front_end::screens::online::{Online, Placement, ServerEntry, ONLINE_LOBBY};
use d2_client::ui::front_end::{FrontCtx, FrontEnd, Outcome, Registry, Screen, Trigger, MAIN_MENU};
use d2_formats::d2s::status;

thread_local! {
    /// Everything the mod's lobby screen saw, for the test to read.
    static LOG: RefCell<Vec<Event>> = const { RefCell::new(Vec::new()) };
    /// The realm link the screen was made with.
    static LINK: RefCell<Option<RealmLink>> = const { RefCell::new(None) };
}

/// A mod lobby screen with no art: a script over the realm link. Each
/// event the realm sends moves the script one step on; a `JoinOk` ends it
/// with the flow's `Ok`.
struct ScriptedLobby {
    realm: RealmLink,
}

impl Screen for ScriptedLobby {
    fn tick(&mut self, _ctx: &mut FrontCtx) -> Option<Trigger> {
        let mut out = None;
        for event in self.realm.take_events() {
            match &event {
                Event::Connected { .. } => self.realm.send(Request::Login {
                    account: "Nugwa".into(),
                    password: "password1".into(),
                }),
                Event::LoggedIn { .. } => self.realm.send(Request::CharList),
                Event::Chars(list) => self.realm.send(Request::CharSelect {
                    name: list[0].name.clone(),
                }),
                Event::CharSelected(_) => self.realm.send(Request::ListGames),
                Event::GameListEnd => {
                    self.realm.send(Request::Chat {
                        text: "hello realm".into(),
                    });
                    self.realm.send(Request::CreateGame {
                        name: "modgame".into(),
                        password: String::new(),
                        difficulty: 0,
                        place: Placement::Automatic,
                        description: "from a mod screen".into(),
                        level_min: 0,
                        level_max: 0,
                    });
                }
                Event::JoinOk(_) => out = Some(Trigger::Ok),
                _ => {}
            }
            LOG.with_borrow_mut(|log| log.push(event));
        }
        out
    }
}

struct MakeLobby;

impl FrontScreenFactory for MakeLobby {
    fn make(&self) -> Box<dyn Screen> {
        panic!("made without the game start")
    }

    fn make_with(&self, start: &FrontStart) -> Box<dyn Screen> {
        let realm = start
            .realm()
            .expect("the visit has a realm session")
            .clone();
        LINK.with_borrow_mut(|link| *link = Some(realm.clone()));
        Box::new(ScriptedLobby { realm })
    }
}

#[test]
fn a_mod_lobby_screen_talks_to_the_realm_and_joins_a_game() {
    let server = SharedFake::default();
    server.0.borrow_mut().add_account(
        "Nugwa",
        "password1",
        vec![character("Sorc", 1, 1, 0, status::EXPANSION)],
    );
    let servers = vec![ServerEntry {
        id: "eu1".into(),
        name: "Test".into(),
        address: "127.0.0.1:6112".into(),
    }];
    let session = Online::new(Box::new(server.clone()), servers);
    let start = FrontStart::new(
        &std::env::temp_dir(),
        &d2_client::app::front_start::StartHandles::default(),
        true,
    )
    .with_realm(session.clone());

    let mods = ClientMods::new().with_front_screen(ONLINE_LOBBY, MakeLobby);
    let mut front = FrontEnd::new(true, Box::new(true), Registry::default());
    front.start(false);
    assert_eq!(front.current(), MAIN_MENU);
    mods.apply_front_with(&mut front, &start);
    front.goto(ONLINE_LOBBY);

    // The screen's realm is the session the engine's screens and the game
    // start share.
    let realm = LINK.with_borrow(Clone::clone).expect("the screen was made");
    assert!(std::rc::Rc::ptr_eq(realm.session(), &session));
    realm.send(Request::Connect { index: 0 });
    for _ in 0..10 {
        front.tick();
        if front.outcome().is_some() {
            break;
        }
    }

    // The engine's own lobby requests went out, in this order.
    let seen = server.0.borrow().seen.clone();
    let kinds: Vec<&str> = seen
        .iter()
        .map(|r| match r {
            Request::Connect { .. } => "connect",
            Request::Login { .. } => "login",
            Request::CharList => "chars",
            Request::CharSelect { .. } => "select",
            Request::ListGames => "list",
            Request::Chat { .. } => "chat",
            Request::CreateGame { .. } => "create",
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        ["connect", "login", "chars", "select", "list", "chat", "create"]
    );
    // The state the screen reads is the engine's.
    {
        let state = realm.state();
        assert!(state.connected);
        assert_eq!(state.account.as_deref(), Some("Nugwa"));
        assert_eq!(
            state.selected.as_ref().map(|c| c.name.as_str()),
            Some("Sorc")
        );
    }
    assert!(LOG.with_borrow(|log| log
        .iter()
        .any(|e| matches!(e, Event::JoinOk(j) if j.name == "modgame"))));

    // The join ended the front end in an online game, whose join is the
    // realm's answer, taken once.
    let Some(Outcome::GameLoad(load)) = front.outcome() else {
        panic!("{:?}", front.outcome());
    };
    assert!(load.online);
    let joined = session.borrow_mut().take_joined().expect("the join");
    assert_eq!(joined.name, "modgame");
    assert!(session.borrow_mut().take_joined().is_none());
}
