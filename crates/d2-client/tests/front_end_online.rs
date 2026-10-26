//! The online screens on the in-memory fake server, no game files: the
//! flow main menu -> login -> characters -> lobby -> game, and the rules
//! the screens show (wrong login, account rules, password games, the
//! difficulty rule from the character's progression, a lost connection).

use d2_client::ui::front_end::screens::online::fake::{character, FakeServer, SharedFake};
use d2_client::ui::front_end::screens::online::lobby::{Lobby, View};
use d2_client::ui::front_end::screens::online::{self, *};
use d2_client::ui::front_end::*;
use d2_formats::d2s::status;

const ESC: u16 = 27;
const ENTER: u16 = 13;
const TAB: u16 = 9;

struct Rig {
    f: FrontEnd,
    server: SharedFake,
    online: OnlineHandle,
}

fn servers() -> Vec<ServerEntry> {
    vec![ServerEntry {
        id: "eu1".into(),
        name: "D2Gamma Test".into(),
        address: "127.0.0.1:6112".into(),
    }]
}

fn two_servers() -> Vec<ServerEntry> {
    let mut s = servers();
    s.push(ServerEntry {
        id: "us1".into(),
        name: "D2Gamma West".into(),
        address: "127.0.0.2:6112".into(),
    });
    s
}

fn rig_with(server: FakeServer, servers: Vec<ServerEntry>) -> Rig {
    let server = SharedFake(std::rc::Rc::new(std::cell::RefCell::new(server)));
    let online = Online::new(Box::new(server.clone()), servers);
    let mut reg = Registry::default();
    screens::register_all(&mut reg);
    online::register_with(&mut reg, online.clone());
    let mut f = FrontEnd::new(true, Box::new(false), reg);
    f.start(false);
    Rig { f, server, online }
}

/// A server with one account, `nugwa` / `password1`, holding a level 30
/// expansion barbarian that unlocked Nightmare (progression 5) and a fresh
/// sorceress.
fn rig() -> Rig {
    let mut s = FakeServer::default();
    s.add_account(
        "Nugwa",
        "password1",
        vec![
            character("Bar", 4, 30, 5, status::EXPANSION),
            character("Sorc", 1, 1, 0, status::EXPANSION),
        ],
    );
    rig_with(s, servers())
}

impl Rig {
    fn ticks(&mut self, n: u32) {
        for _ in 0..n {
            self.f.tick();
        }
    }

    fn ids(&self) -> Vec<String> {
        self.f.ui_elements().into_iter().map(|e| e.id).collect()
    }

    fn click(&mut self, id: &str) {
        let e = self
            .f
            .ui_elements()
            .into_iter()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("no {id} on {:?}: {:?}", self.f.current(), self.ids()));
        self.f.input(FrontInput::Down(e.at));
        self.f.input(FrontInput::Up(e.at));
        self.ticks(2);
    }

    fn key(&mut self, vk: u16) {
        self.f.input(FrontInput::Key(vk));
        self.ticks(2);
    }

    fn typed(&mut self, s: &str) {
        for u in s.encode_utf16() {
            self.f.input(FrontInput::Char(u));
        }
        self.ticks(1);
    }

    /// Every text the screen shows now (controls and overlay).
    fn texts(&mut self) -> Vec<String> {
        let mut items = self.f.draw();
        items.extend(self.f.overlay(&|_, t| t.len() as i32 * 8));
        items
            .into_iter()
            .filter_map(|i| match i {
                DrawItem::Text { text, .. } if !text.is_empty() => Some(text),
                _ => None,
            })
            .collect()
    }

    fn shows(&mut self, s: &str) -> bool {
        self.texts().iter().any(|t| t.contains(s))
    }

    /// Main menu -> login (the flow's trigger: the button is there only
    /// with the `online` feature).
    fn open_login(&mut self) {
        self.f.trigger(Trigger::Online);
        self.ticks(1);
    }

    fn login(&mut self, account: &str, password: &str) {
        self.open_login();
        assert_eq!(self.f.current(), ONLINE_LOGIN);
        self.typed(account);
        self.key(TAB);
        self.typed(password);
        self.key(ENTER);
        self.ticks(2);
    }

    fn enter_lobby(&mut self, name: &str) {
        self.login("nugwa", "password1");
        assert_eq!(self.f.current(), ONLINE_CHARS);
        self.click(&format!("online_chars.char({name})"));
        self.click("online_chars.ok");
        self.ticks(2);
        assert_eq!(self.f.current(), ONLINE_LOBBY, "{:?}", self.texts());
    }

    /// The pointer on MARKET.
    fn hover_market(&mut self) {
        let m = Lobby::market_rect();
        let at = d2_client::ui::geom::Point::new(m.x + 20, m.y + 10);
        self.f.input(FrontInput::Move(at));
        self.ticks(1);
    }

    /// The lobby's market on the fake market.
    fn with_fake_market(self) -> Self {
        self.online.borrow_mut().market_parts =
            Some(std::rc::Rc::new(|| market_host::MarketParts {
                fake: true,
                ..Default::default()
            }));
        self
    }

    fn seen(&self) -> Vec<Request> {
        self.server.0.borrow().seen.clone()
    }
}

/// Rule (contracts 9): with the `online` feature the main menu's ONLINE
/// opens the login screen; without it there is no ONLINE button.
#[test]
fn main_menu_online_button_follows_the_feature() {
    let mut r = rig();
    let has = r.ids().contains(&"menu.online".to_string());
    assert_eq!(has, cfg!(feature = "online"));
    assert_eq!(r.shows("ONLINE"), cfg!(feature = "online"));
    if has {
        r.click("menu.online");
        assert_eq!(r.f.current(), ONLINE_LOGIN);
    }
}

/// A single realm needs no name or selector; BACK returns to the main menu.
#[test]
fn login_hides_the_single_server_and_back_returns() {
    let mut r = rig();
    r.open_login();
    assert_eq!(r.f.current(), ONLINE_LOGIN);
    assert!(!r.ids().contains(&"online_login.server".to_string()));
    assert!(!r.shows("SERVER:"));
    assert!(!r.shows("D2GAMMA TEST"));
    assert!(!r.shows("D2Gamma Test"));
    r.key(ESC);
    assert_eq!(r.f.current(), MAIN_MENU);
}

/// Several realms keep the server button and cycle through the list.
#[test]
fn login_shows_the_server_button_only_when_there_is_a_choice() {
    for (list, selectable) in [(servers(), false), (two_servers(), true)] {
        let mut r = rig_with(FakeServer::default(), list);
        r.open_login();
        assert_eq!(
            r.ids().contains(&"online_login.server".to_string()),
            selectable
        );
        assert_eq!(r.shows("SERVER: D2GAMMA TEST"), selectable);
        if selectable {
            r.click("online_login.server");
            assert_eq!(r.online.borrow().state.server, 1);
            assert!(r.shows("SERVER: D2GAMMA WEST"));
            r.click("online_login.server");
            assert_eq!(r.online.borrow().state.server, 0);
            assert!(r.shows("SERVER: D2GAMMA TEST"));
        }
    }
}

/// The lobby names the game for one realm, and the realm for several.
#[test]
fn lobby_title_follows_the_number_of_realms_and_keeps_the_account() {
    for (list, title) in [(servers(), "D2Gamma"), (two_servers(), "D2Gamma Europe")] {
        let mut r = rig();
        r.online.borrow_mut().state.servers = list;
        r.server.0.borrow_mut().server_name = "D2Gamma Europe".into();
        r.enter_lobby("Bar");
        let texts = r.texts();
        assert!(texts.iter().any(|t| t == title), "{texts:?}");
        assert!(texts.iter().any(|t| t == "Account Nugwa"), "{texts:?}");
        if title == "D2Gamma" {
            assert!(!texts.iter().any(|t| t.contains("Europe")), "{texts:?}");
        }
    }
}

/// The character list names the realm only when there are several, as
/// the login and the lobby do: one realm shows the account alone.
#[test]
fn character_list_names_the_realm_only_when_there_are_several() {
    for (list, named) in [(servers(), false), (two_servers(), true)] {
        let mut r = rig();
        r.online.borrow_mut().state.servers = list;
        r.server.0.borrow_mut().server_name = "D2Gamma Europe".into();
        r.login("nugwa", "password1");
        assert_eq!(r.f.current(), ONLINE_CHARS);
        let texts = r.texts();
        assert_eq!(
            texts.iter().any(|t| t.contains("Europe")),
            named,
            "{texts:?}"
        );
    }
}

/// Rule (contracts 3.4, 3.5): LOG IN connects, then logs in; the list
/// screen asks for the account's characters and shows them.
#[test]
fn login_connects_then_lists_characters() {
    let mut r = rig();
    r.login("nugwa", "password1");
    assert_eq!(r.f.current(), ONLINE_CHARS);
    let seen = r.seen();
    assert_eq!(seen[0], Request::Connect { index: 0 });
    assert_eq!(
        seen[1],
        Request::Login {
            account: "nugwa".into(),
            password: "password1".into()
        }
    );
    assert_eq!(seen[2], Request::CharList);
    let ids = r.ids();
    assert!(
        ids.contains(&"online_chars.char(Bar)".to_string()),
        "{ids:?}"
    );
    assert!(ids.contains(&"online_chars.char(Sorc)".to_string()));
    assert!(r.shows("Nugwa"));
}

/// Rule (contracts 3.8 code 23): a wrong password shows one message for
/// unknown account and wrong password; the screen stays.
#[test]
fn wrong_login_says_so_and_stays() {
    let mut r = rig();
    r.login("nugwa", "nope-nope");
    assert_eq!(r.f.current(), ONLINE_LOGIN);
    assert!(r.shows("Wrong account name or password."));
    r.click("online_login.message_ok");
    assert!(!r.shows("Wrong account name"));
    // The typed name stays for the next try.
    assert!(r.shows("nugwa"));
}

/// Rule (contracts 4): account names and passwords are checked before the
/// request; a new account logs in at once.
#[test]
fn new_account_rules_and_register() {
    let mut r = rig();
    r.open_login();
    r.click("online_login.new_account");
    assert!(r.shows("NEW ACCOUNT"));
    r.typed("x");
    r.key(TAB);
    r.typed("password1");
    r.key(TAB);
    r.typed("password1");
    r.key(ENTER);
    assert!(r.shows("Account names are 2-15"));
    assert!(r.seen().is_empty(), "nothing sent for a bad name");
    r.click("online_login.message_ok");
    r.click("online_login.field(0)");
    r.typed("yz");
    r.click("online_login.field(2)");
    r.typed("!");
    r.key(ENTER);
    assert!(r.shows("The two passwords are not the same."));
    r.click("online_login.message_ok");
    r.click("online_login.field(2)");
    r.f.input(FrontInput::Key(8));
    r.ticks(1);
    r.key(ENTER);
    assert_eq!(r.f.current(), ONLINE_CHARS);
    assert!(r.seen().contains(&Request::Register {
        account: "xyz".into(),
        password: "password1".into()
    }));
}

/// Rule (contracts 9): without `servers.toml` the login screen says where
/// the list comes from.
#[test]
fn no_server_list_says_to_use_the_launcher() {
    let mut r = rig_with(FakeServer::default(), Vec::new());
    r.open_login();
    assert!(r.shows("Start the game once from the launcher"));
}

/// Rule (owner decision 4, contracts 6): the lobby's list greys out games
/// the character can't join and says why; a password game asks for the
/// password; blank and wrong passwords are refused; the right one starts
/// an online game load.
#[test]
fn lobby_join_rules_and_password_prompt() {
    let mut r = rig();
    {
        let mut s = r.server.0.borrow_mut();
        let game = |name: &str, difficulty: u8, locked: bool| online::GameInfo {
            id: 0,
            name: name.into(),
            difficulty,
            players: 1,
            max_players: 8,
            locked,
            hardcore: false,
            expansion: true,
            joinable: 0,
            ..Default::default()
        };
        s.add_game(game("cows", 0, false), "");
        s.add_game(game("baal", 2, false), "");
        s.add_game(game("secret", 1, true), "moo");
    }
    r.enter_lobby("Bar");
    assert!(r.shows("cows"));
    assert!(r.shows("Difficulties: Normal, Nightmare"));
    // Hell is locked for this character: the list says why.
    r.click("online_lobby.game(baal)");
    assert!(r.shows("Hell is not unlocked yet."));
    r.click("online_lobby.primary");
    assert!(r.shows("Your character has not unlocked that difficulty."));
    r.click("online_lobby.message_ok");
    // The password game: prompt, blank refused, wrong refused, right joins.
    r.click("online_lobby.game(secret)");
    r.click("online_lobby.primary");
    assert!(r.shows("This game needs a password."));
    r.click("online_lobby.prompt_ok");
    assert!(r.shows("That game needs a password."));
    r.click("online_lobby.message_ok");
    r.click("online_lobby.primary");
    r.typed("oink");
    r.click("online_lobby.prompt_ok");
    r.ticks(2);
    assert!(r.shows("Wrong game password."), "{:?}", r.texts());
    r.click("online_lobby.message_ok");
    r.click("online_lobby.primary");
    r.typed("moo");
    r.key(ENTER);
    r.ticks(2);
    assert_eq!(
        r.f.outcome(),
        Some(Outcome::GameLoad(GameLoad {
            difficulty: None,
            new_character: false,
            online: true,
        }))
    );
    let j = r.online.borrow_mut().take_joined().expect("joined");
    assert_eq!((j.difficulty, j.name.as_str()), (1, "secret"));
    assert!(r.seen().contains(&Request::LeaveLobby));
}

/// Rule (contracts 6): a public game (blank password) joins with no prompt.
#[test]
fn public_game_joins_at_once() {
    let mut r = rig();
    r.server.0.borrow_mut().add_game(
        online::GameInfo {
            id: 0,
            name: "open".into(),
            difficulty: 0,
            players: 2,
            max_players: 8,
            locked: false,
            hardcore: false,
            expansion: true,
            joinable: 0,
            ..Default::default()
        },
        "",
    );
    r.enter_lobby("Sorc");
    r.click("online_lobby.game(open)");
    r.click("online_lobby.primary");
    r.ticks(2);
    assert!(matches!(r.f.outcome(), Some(Outcome::GameLoad(g)) if g.online));
    assert!(r.seen().contains(&Request::JoinGame {
        game_id: 1,
        password: String::new()
    }));
}

/// Rule (owner decision 4): the create form offers only the difficulties
/// the character unlocked; a fresh character creates at Normal only.
#[test]
fn create_form_limits_difficulty_to_progression() {
    let mut r = rig();
    r.enter_lobby("Sorc");
    r.click("online_lobby.tab_create");
    let ids = r.ids();
    assert!(ids.contains(&"online_lobby.difficulty.normal".to_string()));
    assert!(!ids.contains(&"online_lobby.difficulty.nightmare".to_string()));
    assert!(!ids.contains(&"online_lobby.difficulty.hell".to_string()));
    assert!(r.shows("Grey: not unlocked"));
    r.click("online_lobby.field(0)");
    r.typed("my game");
    r.click("online_lobby.primary");
    r.ticks(2);
    assert!(matches!(r.f.outcome(), Some(Outcome::GameLoad(g)) if g.online));
    assert!(r.seen().contains(&Request::CreateGame {
        name: "my game".into(),
        password: String::new(),
        difficulty: 0,
        place: online::Placement::Automatic,
        description: String::new(),
        level_min: 0,
        level_max: 0,
    }));
}

/// Rule (owner decision 4): a character with Nightmare unlocked starts the
/// form at its highest difficulty and may pick Normal.
#[test]
fn create_form_defaults_to_highest_unlocked() {
    let mut r = rig();
    r.enter_lobby("Bar");
    r.click("online_lobby.tab_create");
    let ids = r.ids();
    assert!(ids.contains(&"online_lobby.difficulty.nightmare".to_string()));
    assert!(!ids.contains(&"online_lobby.difficulty.hell".to_string()));
    r.click("online_lobby.field(0)");
    r.typed("nm run");
    r.click("online_lobby.field(1)");
    r.typed("pw");
    r.click("online_lobby.primary");
    r.ticks(2);
    assert!(r.seen().contains(&Request::CreateGame {
        name: "nm run".into(),
        password: "pw".into(),
        difficulty: 1,
        place: online::Placement::Automatic,
        description: String::new(),
        level_min: 0,
        level_max: 0,
    }));
}

/// Rule (contracts 3.6): the list follows the server's updates while the
/// player is in the lobby; chat lines show; back out goes to the list.
#[test]
fn lobby_follows_updates_and_chat() {
    let mut r = rig();
    r.enter_lobby("Bar");
    assert!(r.shows("No open games"));
    let id = r.server.0.borrow_mut().add_game(
        online::GameInfo {
            id: 0,
            name: "late".into(),
            difficulty: 0,
            players: 1,
            max_players: 8,
            locked: false,
            hardcore: false,
            expansion: true,
            joinable: 0,
            ..Default::default()
        },
        "",
    );
    // The fake sends no update for add_game: the list is asked again.
    r.click("online_lobby.back");
    r.click("online_chars.ok");
    r.ticks(2);
    assert!(r.shows("late"));
    r.server.0.borrow_mut().close_game(id);
    r.ticks(2);
    assert!(!r.shows("late"));
    r.server.0.borrow_mut().chat_from("Friend", "hello there");
    r.ticks(2);
    assert!(r.shows("Friend: ") && r.shows("hello there"));
    r.typed("hi");
    r.key(ENTER);
    r.ticks(2);
    assert!(r.shows("Bar: ") && r.shows("hi"));
    r.key(ESC);
    assert_eq!(r.f.current(), ONLINE_CHARS);
}

/// Rule (contracts 3.6): a server without chat hides the input line. No
/// chat at all: neither the one-room chat nor the social service's rooms
/// (the lobby's chat speaks the social service first, plan L12).
#[test]
fn chat_off_hides_input() {
    let mut s = FakeServer::default();
    s.chat_off = true;
    s.social_off = true;
    s.add_account(
        "Nugwa",
        "password1",
        vec![character("Bar", 4, 30, 5, status::EXPANSION)],
    );
    let mut r = rig_with(s, servers());
    r.enter_lobby("Bar");
    r.typed("hi");
    r.key(ENTER);
    r.ticks(2);
    assert!(r.shows("Chat is not available on this server."));
}

/// Rule (contracts 9): a lost connection goes back to login with the reason.
#[test]
fn lost_connection_returns_to_login_with_reason() {
    let mut r = rig();
    r.enter_lobby("Bar");
    r.server
        .0
        .borrow_mut()
        .drop_connection("The server closed the connection.");
    r.ticks(2);
    assert_eq!(r.f.current(), ONLINE_LOGIN);
    assert!(r.shows("The server closed the connection."));
    assert!(!r.online.borrow().state.connected);
}

/// Rule (contracts 3.5, 9): online create sends the new character to the
/// server (nothing on disk); a taken name keeps the create screen with the
/// reason; created goes back to the list with it.
#[test]
fn online_create_sends_to_server() {
    let mut s = FakeServer::default();
    s.add_account("Nugwa", "password1", Vec::new());
    s.add_account("Other", "password2", vec![character("Taken", 0, 1, 0, 0)]);
    let mut r = rig_with(s, servers());
    r.login("nugwa", "password1");
    r.click("online_chars.create_new");
    assert_eq!(r.f.current(), ONLINE_CREATE);
    r.click("online_create.class.barbarian");
    r.ticks(30);
    r.typed("Taken");
    r.key(ENTER);
    r.ticks(2);
    assert_eq!(r.f.current(), ONLINE_CREATE);
    assert!(
        r.shows("That character name is already taken."),
        "{:?}",
        r.texts()
    );
    r.click("online_create.message_ok");
    for _ in 0..5 {
        r.f.input(FrontInput::Key(8));
    }
    r.ticks(1);
    r.typed("Fresh");
    r.key(ENTER);
    r.ticks(2);
    assert_eq!(r.f.current(), ONLINE_CHARS);
    assert!(r.ids().contains(&"online_chars.char(Fresh)".to_string()));
    assert!(r
        .seen()
        .iter()
        .any(|q| matches!(q, Request::CharCreate { name, class: 4, .. } if name == "Fresh")));
}

/// Rule (contracts 3.5): delete asks for the account password; the server
/// deletes and the list drops the character.
#[test]
fn delete_asks_for_the_account_password() {
    let mut r = rig();
    r.login("nugwa", "password1");
    r.click("online_chars.char(Sorc)");
    r.click("online_chars.delete");
    r.click("online_chars.yes");
    assert!(r.shows("Type your account password to delete Sorc."));
    r.typed("password1");
    r.click("online_chars.delete_ok");
    r.ticks(2);
    assert!(!r.ids().contains(&"online_chars.char(Sorc)".to_string()));
    assert!(r.seen().contains(&Request::CharDelete {
        name: "Sorc".into(),
        password: "password1".into()
    }));
}

/// The lobby's state starts on the list.
#[test]
fn lobby_view_starts_on_list() {
    let o = Online::new(Box::<NoLink>::default(), Vec::new());
    let l = Lobby::new(o);
    assert_eq!(l.view, View::List);
}

// ---- The host's side: the lobby's join becomes the online game, and the
// front end comes back after it (cl-mp-client).

/// A front end on `online` opened at `screen`, the way the host opens it
/// again after a game.
fn reopen(online: &OnlineHandle, screen: ScreenId) -> FrontEnd {
    let mut reg = Registry::default();
    screens::register_all(&mut reg);
    online::register_with(&mut reg, online.clone());
    let mut f = FrontEnd::new(true, Box::new(false), reg);
    f.start(false);
    f.goto(screen);
    f
}

/// A public Normal game the fresh sorceress can join.
fn open_game(r: &Rig) {
    r.server.0.borrow_mut().add_game(
        online::GameInfo {
            id: 0,
            name: "open".into(),
            difficulty: 0,
            players: 2,
            max_players: 8,
            locked: false,
            hardcore: false,
            expansion: true,
            joinable: 0,
            ..Default::default()
        },
        "",
    );
}

fn join_open_game(r: &mut Rig) {
    open_game(r);
    r.enter_lobby("Sorc");
    r.click("online_lobby.game(open)");
    r.click("online_lobby.primary");
    r.ticks(2);
    assert!(matches!(r.f.outcome(), Some(Outcome::GameLoad(g)) if g.online));
}

/// Rule (contracts 9): leaving the game returns to the lobby on the same
/// session; why the game ended shows there, and the lobby lists the games
/// again. A join the game did not use is spent.
#[test]
fn after_a_game_the_lobby_opens_on_the_same_session() {
    use d2_client::app::front_start::{after_online_game_on, Entry};
    let mut r = rig();
    join_open_game(&mut r);
    let lists = |r: &Rig| {
        r.seen()
            .iter()
            .filter(|q| **q == Request::ListGames)
            .count()
    };
    let before = lists(&r);
    assert_eq!(
        after_online_game_on(&r.online, true, Some("The game closed.".into())),
        Entry::Lobby
    );
    assert!(r.online.borrow().state.joined.is_none());
    r.f = reopen(&r.online, ONLINE_LOBBY);
    r.ticks(2);
    assert_eq!(r.f.current(), ONLINE_LOBBY);
    assert!(r.shows("The game closed."));
    assert_eq!(lists(&r), before + 1, "the lobby lists the games again");
    assert_eq!(
        r.online
            .borrow()
            .state
            .selected
            .as_ref()
            .map(|c| c.name.as_str()),
        Some("Sorc")
    );
}

/// Rule (contracts 9): a game whose connection is gone goes back to the
/// login with the reason; the online state starts over.
#[test]
fn after_a_lost_game_the_login_opens_with_the_reason() {
    use d2_client::app::front_start::{after_online_game_on, Entry};
    let mut r = rig();
    join_open_game(&mut r);
    let why = "The connection to the server was lost (timed out).";
    assert_eq!(
        after_online_game_on(&r.online, false, Some(why.into())),
        Entry::OnlineError
    );
    r.f = reopen(&r.online, ONLINE_LOGIN);
    r.ticks(2);
    assert_eq!(r.f.current(), ONLINE_LOGIN);
    assert!(r.shows(why));
    let st = &r.online.borrow().state;
    assert!(!st.connected && st.selected.is_none() && st.joined.is_none());
}

/// Rule (plan N13): a game whose game server is lost while the main
/// server's session lives on goes back to the lobby, saying the character
/// is as it was at its last checkpoint; with the session gone too, the
/// login shows why.
#[cfg(feature = "online")]
#[test]
fn a_lost_game_server_goes_back_to_the_lobby_with_the_checkpoint() {
    use d2_client::app::front_start::{after_online_game_on, Entry};
    use d2_client::app::online::lost_game;
    let mut r = rig();
    join_open_game(&mut r);
    let end = lost_game("the reconnect attempt stopped", true);
    assert!(end.keep_session);
    assert_eq!(
        after_online_game_on(&r.online, true, Some(end.notice)),
        Entry::Lobby
    );
    r.f = reopen(&r.online, ONLINE_LOBBY);
    r.ticks(2);
    assert_eq!(r.f.current(), ONLINE_LOBBY);
    // The message box wraps the notice: its lines read as one text.
    let all = r.texts().join(" ");
    assert!(
        all.contains(
            "The game server was lost. Your character is as it was at its last checkpoint."
        ),
        "{all}"
    );
    // The main server went too: the login, with the connection's reason.
    let gone = lost_game("timed out", false);
    assert_eq!(
        gone,
        d2_client::app::online::LostGame {
            keep_session: false,
            notice: "timed out".into(),
        }
    );
}

#[cfg(feature = "online")]
mod host {
    use super::*;
    use d2_client::app::front_start::{front_host_with, Entry, StartHandles};
    use d2_client::app::online::OnlineJoin;

    fn save_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("d2-fe-online-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The host's front end on `handles`, opened at `entry`, as a rig.
    fn host_rig(handles: &StartHandles, server: SharedFake, entry: Entry) -> Rig {
        let dir = save_dir("host");
        let mut host = front_host_with(&dir, None, true, entry, handles);
        let f = std::mem::replace(
            &mut host.front,
            FrontEnd::new(true, Box::new(false), Registry::default()),
        );
        let online = handles.online.borrow().clone().unwrap();
        Rig { f, server, online }
    }

    fn fake_handles(server: &SharedFake, servers: Vec<ServerEntry>) -> StartHandles {
        let h = StartHandles::default();
        *h.online.borrow_mut() = Some(Online::new(Box::new(server.clone()), servers));
        h
    }

    /// Rule (contracts 9): `play --online --server ID` opens the login
    /// screen on that server, past the main menu.
    #[test]
    fn play_online_opens_the_login_on_the_chosen_server() {
        let server = SharedFake(std::rc::Rc::new(std::cell::RefCell::new(
            FakeServer::default(),
        )));
        let mut h = fake_handles(&server, two_servers());
        h.server = Some("us1".into());
        let mut r = host_rig(&h, server, Entry::Online);
        r.ticks(1);
        assert_eq!(r.f.current(), ONLINE_LOGIN);
        assert_eq!(r.online.borrow().state.server, 1);
        assert!(r.shows("SERVER: D2GAMMA WEST"), "{:?}", r.texts());
    }

    /// Rule (contracts 9, 3.7): the lobby's join becomes the game's join:
    /// the `JoinOk` ticket and difficulty with the selected character's
    /// name, class and status, taken once. The front end opened again on
    /// the same handles comes back to the lobby.
    #[test]
    fn the_lobby_join_is_the_online_games_join() {
        let base = rig();
        let server = base.server.clone();
        let h = fake_handles(&server, servers());
        let mut r = host_rig(&h, server, Entry::MainMenu);
        assert_eq!(r.f.current(), MAIN_MENU);
        join_open_game(&mut r);
        let join = OnlineJoin::take(&mut r.online.borrow_mut()).expect("a join");
        let sorc = r.online.borrow().state.selected.clone().unwrap();
        let account = r.online.borrow().state.account.clone().unwrap_or_default();
        assert!(!account.is_empty());
        let joined_ticket = join.ticket;
        assert_eq!(
            join,
            OnlineJoin {
                ticket: joined_ticket,
                node: None,
                name: "Sorc".into(),
                class: 1,
                status: sorc.status,
                difficulty: 0,
                realm: "eu1".into(),
                account,
            }
        );
        assert!(OnlineJoin::take(&mut r.online.borrow_mut()).is_none());

        let again = host_rig(&h, r.server.clone(), Entry::Lobby);
        assert_eq!(again.f.current(), ONLINE_LOBBY);
        assert!(std::rc::Rc::ptr_eq(&again.online, &r.online));
    }
}

/// The screens and the game start on a real session over loopback QUIC:
/// a server scripted to the contract (accounts, lobby, game attach) behind
/// a pinned certificate, the game's own `NetLink` and `JoinTask`.
#[cfg(feature = "online")]
mod loopback {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use d2_client::app::front_start::{after_online_game, front_host_with, Entry, StartHandles};
    use d2_client::app::online::{self as app_online, JoinTask, OnlineJoin, ServerList};
    use d2_client::ui::front_end::screens::online::net_link::{entries, NetLink};
    use d2_net::msg::account::{AccountEvent, AccountRequest, CharSummary};
    use d2_net::msg::lobby::{GameInfo as WireGame, LobbyEvent, LobbyRequest};
    use d2_net::wire::{Attach, ResumeToken, SessionWelcome, Welcome};
    use d2_net::Timing;
    use d2_online::ServerTrust;
    use d2_quic::{fingerprint, FramedStream, Identity, QuicConn, QuicServer};

    const TICKET: [u8; 16] = [7; 16];
    const GAME_TOKEN: ResumeToken = ResumeToken([9; 16]);
    /// The game server answers the attach this late: the join must not
    /// wait for it on the game thread.
    const ATTACH_DELAY: Duration = Duration::from_millis(300);

    fn sorc() -> CharSummary {
        CharSummary {
            name: "Sorc".into(),
            class: 1,
            level: 12,
            status: status::EXPANSION,
            difficulties: 0b001,
            look: vec![0; 32],
            last_played: 0,
            season: 0,
        }
    }

    fn game() -> WireGame {
        WireGame {
            game_id: 3,
            name: "loop".into(),
            difficulty: 0,
            players: 1,
            max_players: 8,
            locked: false,
            hardcore: false,
            expansion: true,
            joinable: 0,
            ..WireGame::default()
        }
    }

    /// The control stream: handshake, accounts and lobby as the contract
    /// answers them; counts the `ListGames`.
    async fn serve(conn: QuicConn, mut control: FramedStream, lists: Arc<AtomicU32>) {
        if control.recv_async().await.is_err() {
            return;
        }
        let welcome = SessionWelcome {
            tick_rate: d2_sim::tick::TickRate::GAME.hz(),
            max_players: 8,
            session: ResumeToken([5; 16]),
            server_name: "Loopback realm".into(),
            registration_open: false,
            resume: None,
        };
        if control.send(welcome.body()).is_err() {
            return;
        }
        tokio::spawn(serve_games(conn));
        while let Ok(b) = control.recv_async().await {
            let answers: Vec<Vec<u8>> = if let Ok(r) = AccountRequest::parse(&b) {
                match r {
                    AccountRequest::Login { account, .. } => vec![AccountEvent::LoggedIn {
                        account,
                        max_characters: 8,
                    }
                    .body()],
                    AccountRequest::CharList => vec![AccountEvent::CharList(vec![sorc()]).body()],
                    AccountRequest::CharSelect { .. } => {
                        vec![AccountEvent::CharSelected(sorc()).body()]
                    }
                    _ => vec![],
                }
            } else if let Ok(r) = LobbyRequest::parse(&b) {
                match r {
                    LobbyRequest::ListGames => {
                        lists.fetch_add(1, Ordering::SeqCst);
                        vec![
                            LobbyEvent::GameListBegin { count: 1 }.body(),
                            LobbyEvent::GameEntry(game()).body(),
                            LobbyEvent::GameListEnd.body(),
                        ]
                    }
                    LobbyRequest::JoinGame { game_id: 3, .. } => vec![LobbyEvent::JoinOk {
                        game_id: 3,
                        ticket: TICKET,
                        difficulty: 0,
                    }
                    .body()],
                    _ => vec![],
                }
            } else {
                vec![]
            };
            for a in answers {
                let _ = control.send(a);
            }
        }
    }

    /// The game streams: the lobby ticket attaches, late.
    async fn serve_games(conn: QuicConn) {
        while let Some(mut stream) = conn.accept_stream().await {
            let Ok(first) = stream.recv_async().await else {
                continue;
            };
            if let Ok(Attach::Ticket(TICKET)) = Attach::parse(&first) {
                tokio::time::sleep(ATTACH_DELAY).await;
                let w = Welcome {
                    client: 1,
                    tick_rate: d2_sim::tick::TickRate::GAME.hz(),
                    max_players: 8,
                    resume: GAME_TOKEN,
                };
                let _ = stream.send(w.body());
                while stream.recv_async().await.is_ok() {}
            }
        }
    }

    /// Ticks the front end in real time until `done` (at most 10 s).
    fn until(r: &mut Rig, what: &str, mut done: impl FnMut(&mut Rig) -> bool) {
        let end = Instant::now() + Duration::from_secs(10);
        while !done(r) {
            assert!(Instant::now() < end, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(5));
            r.ticks(1);
        }
    }

    #[test]
    fn a_lobby_join_starts_the_game_without_waiting_and_comes_back_to_the_lobby() {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let id = Identity::self_signed().unwrap();
        let cert = id.cert.clone();
        let quic = Arc::new(
            QuicServer::bind(
                "127.0.0.1:0".parse().unwrap(),
                id,
                Timing::default(),
                rt.handle().clone(),
            )
            .unwrap(),
        );
        let lists = Arc::new(AtomicU32::new(0));
        {
            let (q, lists) = (quic.clone(), lists.clone());
            rt.spawn(async move {
                while let Some((conn, control)) = q.accept().await {
                    tokio::spawn(serve(conn, control, lists.clone()));
                }
            });
        }
        let list = ServerList {
            servers: vec![app_online::ServerEntry {
                id: "lo".into(),
                name: "Loopback".into(),
                game: "d2rust".into(),
                address: quic.local_addr().unwrap().to_string(),
                trust: ServerTrust::Pinned(fingerprint(&cert)),
                version: d2_client::launch::VERSION.into(),
            }],
        };

        // The host handles, the screens on the game link.
        let h = StartHandles::default();
        let servers = entries(&list);
        *h.online.borrow_mut() = Some(Online::new(
            Box::new(NetLink::new(h.session.clone(), list)),
            servers,
        ));
        let dir = std::env::temp_dir().join(format!("d2-fe-loop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let open = |entry| {
            let mut host = front_host_with(&dir, None, true, entry, &h);
            std::mem::replace(
                &mut host.front,
                FrontEnd::new(true, Box::new(false), Registry::default()),
            )
        };
        let online = h.online.borrow().clone().unwrap();
        let mut r = Rig {
            f: open(Entry::Online),
            server: SharedFake(std::rc::Rc::default()),
            online: online.clone(),
        };
        assert_eq!(r.f.current(), ONLINE_LOGIN);
        r.typed("nugwa");
        r.key(TAB);
        r.typed("password1");
        r.key(ENTER);
        until(&mut r, "the character list", |r| {
            r.ids().contains(&"online_chars.char(Sorc)".to_string())
        });
        r.click("online_chars.char(Sorc)");
        r.click("online_chars.ok");
        until(&mut r, "the lobby list", |r| {
            r.f.current() == ONLINE_LOBBY
                && r.ids().contains(&"online_lobby.game(loop)".to_string())
        });
        r.click("online_lobby.game(loop)");
        r.click("online_lobby.primary");
        until(
            &mut r,
            "the game load",
            |r| matches!(r.f.outcome(), Some(Outcome::GameLoad(g)) if g.online),
        );

        // The game start: the join answer comes on a worker thread.
        let join = OnlineJoin::take(&mut online.borrow_mut()).expect("the lobby join");
        assert_eq!(
            (join.ticket, join.name.as_str(), join.class),
            (TICKET, "Sorc", 1)
        );
        let asked = Instant::now();
        let task = JoinTask::start(&h.session, join.ticket).unwrap();
        assert!(task.poll().is_none(), "the join answered before the server");
        assert!(
            asked.elapsed() < ATTACH_DELAY,
            "starting the join waited for the server"
        );
        let end = Instant::now() + Duration::from_secs(10);
        let game = loop {
            if let Some(a) = task.poll() {
                break a.expect("the game welcome");
            }
            assert!(Instant::now() < end, "no answer to the join");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(game.welcome().resume, GAME_TOKEN);
        drop(game);

        // After the game: the lobby, on the same session.
        assert_eq!(after_online_game(&h, None), Entry::Lobby);
        let before = lists.load(Ordering::SeqCst);
        r.f = open(Entry::Lobby);
        assert_eq!(r.f.current(), ONLINE_LOBBY);
        until(&mut r, "the lobby listed again", |r| {
            lists.load(Ordering::SeqCst) > before
                && r.ids().contains(&"online_lobby.game(loop)".to_string())
        });

        // The session gone: the login, with why.
        if let Some(s) = h.session.0.lock().unwrap().take() {
            s.close();
        }
        assert_eq!(
            after_online_game(&h, Some("The server is gone.".into())),
            Entry::OnlineError
        );
        r.f = open(Entry::OnlineError);
        r.ticks(1);
        assert_eq!(r.f.current(), ONLINE_LOGIN);
        assert!(r.shows("The server is gone."));
        quic.close();
    }
}

/// An official game server of the stub list (until the main server sends
/// one).
fn node(id: &str, name: &str, location: &str, load_pct: u8, state: NodeState) -> NodeInfo {
    NodeInfo {
        node: id.into(),
        name: name.into(),
        location: location.into(),
        ping_addr: "127.0.0.1:9".into(),
        cert_sha256: [1; 32],
        load_pct,
        state,
        suggested: id == "hel-1",
    }
}

fn stub_nodes() -> Vec<NodeInfo> {
    vec![
        node("hel-1", "Finland", "Finland", 80, NodeState::Open),
        node("fra-1", "Germany", "Germany", 10, NodeState::Open),
        node("nyc-1", "US East", "US East", 100, NodeState::Full),
    ]
}

fn pings(table: &[(&str, Option<u32>)]) -> PingTable {
    table.iter().map(|&(n, p)| (n.to_string(), p)).collect()
}

/// A rig whose server lists game servers, the pings not measured yet.
fn rig_with_nodes() -> Rig {
    let r = rig();
    r.server.0.borrow_mut().nodes = stub_nodes();
    r
}

impl Rig {
    /// Creates a game from the open form; the request's placement.
    fn create(&mut self, name: &str) -> online::Placement {
        self.click("online_lobby.field(0)");
        self.typed(name);
        self.click("online_lobby.primary");
        self.ticks(2);
        self.seen()
            .into_iter()
            .rev()
            .find_map(|r| match r {
                Request::CreateGame { name: n, place, .. } if n == name => Some(place),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no CreateGame {name}: {:?}", self.seen()))
    }
}

impl Rig {
    /// The colour of the first text that is `s` (the overlay's last).
    fn colour_of(&mut self, s: &str) -> Option<i32> {
        self.f
            .overlay(&|_, t| t.len() as i32 * 8)
            .into_iter()
            .rev()
            .find_map(|i| match i {
                DrawItem::Text { text, color, .. } if text == s => Some(color),
                _ => None,
            })
    }

    /// The SERVER drop-down's entries shown (their ids).
    fn entries(&self) -> Vec<String> {
        self.ids()
            .into_iter()
            .filter(|i| {
                i.starts_with("online_lobby.server.") || i.starts_with("online_lobby.server(")
            })
            .collect()
    }
}

/// Rule (plan N12, owner 2026-10-24): the create form's SERVER row is a
/// drop-down showing where the game runs, Automatic by default (the best
/// ping, within 15 ms the less loaded server); the list has Automatic
/// first, then each server by its short region name (no brand or machine
/// name, no "OFFICIAL") with its ping, a full one grey with "Full". The
/// chat stays in its pane.
#[test]
fn create_form_has_a_server_drop_down_defaulting_to_the_best_ping() {
    let mut r = rig_with_nodes();
    r.enter_lobby("Bar");
    r.click("online_lobby.tab_create");
    assert!(r.shows("SERVER"));
    assert!(r.shows("Automatic"));
    // The pings are still being measured: no ping in the box.
    assert!(!r.shows(" ms"), "{:?}", r.texts());
    // Closed: only the box.
    assert!(r.ids().contains(&"online_lobby.server".to_string()));
    assert!(r.entries().is_empty(), "{:?}", r.entries());
    // The chat stays while creating.
    r.server.0.borrow_mut().chat_from("Someone", "hello there");
    r.ticks(2);
    assert!(r.shows("hello there"));
    // Finland answers lowest, Germany within 15 ms of it and less loaded;
    // US East is full.
    r.server.0.borrow_mut().set_nodes(
        stub_nodes(),
        pings(&[
            ("hel-1", Some(20)),
            ("fra-1", Some(33)),
            ("nyc-1", Some(90)),
        ]),
    );
    r.ticks(2);
    // Automatic takes Germany: its ping in the box.
    assert!(r.shows("33 ms"), "{:?}", r.texts());
    r.click("online_lobby.server");
    let entries = r.entries();
    for id in ["server.auto", "server(0)", "server(1)"] {
        assert!(
            entries.contains(&format!("online_lobby.{id}")),
            "{id}: {entries:?}"
        );
    }
    // US East is full: listed, grey, no entry to click.
    assert!(!entries.contains(&"online_lobby.server(2)".to_string()));
    assert!(!entries.contains(&"online_lobby.server.host".to_string()));
    for s in [
        "Automatic (lowest ping)",
        "Finland",
        "Germany",
        "US East",
        "20 ms",
        "33 ms",
        "Full",
    ] {
        assert!(r.shows(s), "{s}: {:?}", r.texts());
    }
    assert_eq!(r.colour_of("US East"), Some(online::widgets::GREY));
    assert_eq!(r.colour_of("Full"), Some(online::widgets::GREY));
    assert_ne!(r.colour_of("Finland"), Some(online::widgets::GREY));
    for s in [
        "OFFICIAL",
        "Helsinki",
        "Frankfurt",
        "Host on my PC",
        PLAYER_HOSTED_NOTE,
    ] {
        assert!(
            !r.texts().iter().any(|t| t.contains(s)),
            "{s}: {:?}",
            r.texts()
        );
    }
    // Esc closes it, Automatic still chosen.
    r.key(ESC);
    assert!(r.entries().is_empty(), "{:?}", r.entries());
    assert_eq!(r.f.current(), ONLINE_LOBBY);
    assert!(r.shows("Automatic"));
    assert_eq!(
        r.create("cows"),
        online::Placement::AutomaticPick("fra-1".into())
    );
    assert!(matches!(r.f.outcome(), Some(Outcome::GameLoad(g)) if g.online));
}

/// Rule (plan N12): with no ping answered, Automatic takes the server the
/// main server suggests.
#[test]
fn automatic_without_pings_takes_the_suggested_server() {
    let mut r = rig_with_nodes();
    r.server.0.borrow_mut().pings = pings(&[("hel-1", None), ("fra-1", None), ("nyc-1", None)]);
    r.enter_lobby("Bar");
    r.click("online_lobby.tab_create");
    assert!(r.shows("Automatic"), "{:?}", r.texts());
    assert_eq!(
        r.create("cows"),
        online::Placement::AutomaticPick("hel-1".into())
    );
}

/// Rule (plan N12): choosing a server in the drop-down closes it and
/// places the game there; a full server can't be chosen (a click on it
/// keeps the list's choice); a click outside closes the list.
#[test]
fn choosing_a_server_in_the_drop_down_places_the_game_there() {
    let mut r = rig_with_nodes();
    r.server.0.borrow_mut().pings = pings(&[("hel-1", Some(20)), ("fra-1", Some(60))]);
    r.enter_lobby("Bar");
    r.click("online_lobby.tab_create");
    assert!(r.shows("20 ms"));
    // A click outside the open list closes it.
    r.click("online_lobby.server");
    assert!(!r.entries().is_empty());
    r.click("online_lobby.server.close");
    assert!(r.entries().is_empty(), "{:?}", r.entries());
    // The full server's row takes no click: the list stays open on it.
    r.click("online_lobby.server");
    let full = Rig::full_row(&mut r);
    r.f.input(FrontInput::Down(full));
    r.f.input(FrontInput::Up(full));
    r.ticks(2);
    assert!(!r.entries().is_empty(), "the full row closed the list");
    assert_eq!(
        r.colour_of("Automatic (lowest ping)"),
        Some(online::widgets::GOLD)
    );
    r.click("online_lobby.server(1)");
    assert!(r.entries().is_empty(), "{:?}", r.entries());
    assert!(r.shows("Germany"));
    assert!(r.shows("60 ms"), "{:?}", r.texts());
    assert_eq!(
        r.create("far away"),
        online::Placement::Official("fra-1".into())
    );
    assert!(matches!(r.f.outcome(), Some(Outcome::GameLoad(g)) if g.online));
}

/// Rule (plan N12): Up / Down / Enter while the list is open; Down skips
/// the full server and stops at the last one; the form's keys wait until
/// it is closed.
#[test]
fn the_drop_down_takes_up_down_and_enter() {
    let mut r = rig_with_nodes();
    r.server.0.borrow_mut().pings = pings(&[("hel-1", Some(20)), ("fra-1", Some(60))]);
    r.enter_lobby("Bar");
    r.click("online_lobby.tab_create");
    r.click("online_lobby.server");
    // Opens on Automatic; Down three times: Finland, Germany, (US East is
    // full) Germany still.
    for _ in 0..3 {
        r.key(40);
    }
    assert_eq!(r.colour_of("Germany"), Some(online::widgets::GOLD));
    // Typing goes nowhere while it is open.
    r.typed("zz");
    r.key(38);
    assert_eq!(r.colour_of("Finland"), Some(online::widgets::GOLD));
    r.key(40);
    r.key(ENTER);
    assert!(r.entries().is_empty(), "{:?}", r.entries());
    assert_eq!(r.f.current(), ONLINE_LOBBY);
    assert!(!r
        .seen()
        .iter()
        .any(|q| matches!(q, Request::CreateGame { .. })));
    assert!(!r.shows("zz"));
    assert_eq!(
        r.create("by keys"),
        online::Placement::Official("fra-1".into())
    );
}

/// Rule (plan N12): a chosen server that stops taking games sends the
/// choice back to Automatic.
#[test]
fn a_chosen_server_that_closes_goes_back_to_automatic() {
    let mut r = rig_with_nodes();
    r.server.0.borrow_mut().pings = pings(&[("hel-1", Some(20)), ("fra-1", Some(60))]);
    r.enter_lobby("Bar");
    r.click("online_lobby.tab_create");
    r.click("online_lobby.server");
    r.click("online_lobby.server(1)");
    assert!(r.shows("Germany"));
    // Germany starts closing.
    let mut closing = stub_nodes();
    closing[1].state = NodeState::Draining;
    let table = r.server.0.borrow().pings.clone();
    r.server.0.borrow_mut().set_nodes(closing, table);
    r.ticks(2);
    assert!(
        !r.shows("Germany") && r.shows("Automatic"),
        "{:?}",
        r.texts()
    );
    r.click("online_lobby.server");
    assert!(r.shows("Closing"));
    assert_eq!(r.colour_of("Closing"), Some(online::widgets::GREY));
    assert!(!r.entries().contains(&"online_lobby.server(1)".to_string()));
    r.key(ESC);
    assert_eq!(
        r.create("first"),
        online::Placement::AutomaticPick("hel-1".into())
    );
}

impl Rig {
    /// The middle of the full server's row (US East, the list's fourth)
    /// in the open list: under Finland's row by two rows.
    fn full_row(r: &mut Rig) -> d2_client::ui::geom::Point {
        let at = |r: &Rig, id: &str| {
            r.f.ui_elements()
                .into_iter()
                .find(|e| e.id == id)
                .map(|e| e.at)
                .unwrap_or_else(|| panic!("no {id}"))
        };
        let a = at(r, "online_lobby.server(0)");
        let b = at(r, "online_lobby.server(1)");
        d2_client::ui::geom::Point::new(b.x, b.y + (b.y - a.y))
    }
}

/// Rule (owner 2026-10-24): with the hosting switch off
/// ([`online::PLAYER_HOSTING`]) nothing offers hosting a game on the
/// player's PC, also on a main server that would take one.
#[test]
fn no_host_on_my_pc_while_the_hosting_switch_is_off() {
    const { assert!(!online::PLAYER_HOSTING) };
    let mut r = rig_with_nodes();
    r.server.0.borrow_mut().hosting = true;
    r.enter_lobby("Bar");
    r.click("online_lobby.tab_create");
    r.click("online_lobby.server");
    assert!(r.shows("Automatic (lowest ping)"));
    assert!(!r.ids().iter().any(|i| i.ends_with("server.host")));
    assert!(!r.texts().iter().any(|t| t.contains("Host on my PC")));
    assert!(
        !r.texts().iter().any(|t| t.contains("osting")),
        "{:?}",
        r.texts()
    );
    r.key(ESC);
    assert_ne!(r.create("my place"), online::Placement::HostOnMyPc);
}

/// Rule (plan N12, L10): with game servers the list's SERVER column shows
/// each game's server and ping, Host for a player-hosted game with the
/// note under the list, and the selected game's server.
#[test]
fn the_game_list_shows_pings_and_player_hosted_games() {
    let mut r = rig_with_nodes();
    r.server.0.borrow_mut().pings = pings(&[("hel-1", Some(24)), ("fra-1", None)]);
    let game = |name: &str, server: &str, player_hosted: bool| online::GameInfo {
        name: name.into(),
        players: 1,
        max_players: 8,
        expansion: true,
        server: server.into(),
        player_hosted,
        ..Default::default()
    };
    for g in [
        game("baal runs", "Finland", false),
        game("chill", "Werwer", true),
        // Found in the server list by its id.
        online::GameInfo {
            node_id: "fra-1".into(),
            ..game("trade", "Germany", false)
        },
    ] {
        r.server.0.borrow_mut().add_game(g, "");
    }
    r.enter_lobby("Bar");
    let note = format!("Host: {PLAYER_HOSTED_NOTE}");
    for s in ["SERVER", "24 ms", "Host", note.as_str()] {
        assert!(r.shows(s), "{s}: {:?}", r.texts());
    }
    r.click("online_lobby.game(baal runs)");
    assert!(r.shows("baal runs: on Finland, 24 ms"), "{:?}", r.texts());
    r.click("online_lobby.game(chill)");
    assert!(r.shows("chill: player hosted by Werwer"));
    // No answer from Germany.
    r.click("online_lobby.game(trade)");
    assert!(r.shows("trade: on Germany, -"));
}

/// Rule (contracts 11): a main server without game servers keeps the
/// screens as they were: no SERVER row, no ping column, chat in the pane.
#[test]
fn without_game_servers_the_lobby_is_unchanged() {
    let mut r = rig();
    r.server.0.borrow_mut().add_game(
        online::GameInfo {
            name: "cows".into(),
            players: 1,
            max_players: 8,
            expansion: true,
            ..Default::default()
        },
        "",
    );
    r.enter_lobby("Bar");
    assert!(!r.shows("SERVER"));
    r.click("online_lobby.tab_create");
    assert!(!r.shows("SERVER"));
    assert!(!r.ids().iter().any(|i| i.contains("server")));
    r.server.0.borrow_mut().chat_from("Someone", "hello there");
    r.ticks(2);
    assert!(r.shows("hello there"));
    assert_eq!(r.create("plain"), online::Placement::Automatic);
}

/// Rule (lobby contract section 7, POLISH): every game is listed; the ones
/// the character can't join are grey with the reason under the list, and
/// the selected one says it as a sentence.
#[test]
fn games_of_another_kind_are_greyed_with_the_reason() {
    let mut r = rig();
    let game = |name: &str, hardcore: bool, players: u8| online::GameInfo {
        name: name.into(),
        players,
        max_players: 8,
        hardcore,
        expansion: true,
        ..Default::default()
    };
    for g in [
        game("cows", false, 1),
        game("hc trades", true, 1),
        game("packed", false, 8),
    ] {
        r.server.0.borrow_mut().add_game(g, "");
    }
    r.enter_lobby("Bar");
    for s in ["cows", "hc trades", "packed", "Grey: hardcore game / full"] {
        assert!(r.shows(s), "{s}: {:?}", r.texts());
    }
    r.click("online_lobby.game(hc trades)");
    assert!(
        r.shows("hc trades: For hardcore characters."),
        "{:?}",
        r.texts()
    );
}

/// Rule (plan L10): the four buttons in the game's button art, squeezed to
/// fit the panel; the market button greyed without a market, its tip only
/// while the mouse is on it (plan L18); Ladder and Friends open their
/// panels (the friends panel of plan L12 with its four tabs and the list)
/// and BACK returns.
#[test]
fn the_lobby_buttons_and_panels() {
    let mut r = rig();
    r.enter_lobby("Bar");
    let strips =
        r.f.draw()
            .into_iter()
            .filter(|i| matches!(i, DrawItem::Strip { .. }))
            .count();
    assert_eq!(
        strips, 12,
        "two halves for each of the four buttons, LOCK PANELS and RESET PANELS"
    );
    for s in ["CREATE", "GAMES", "LADDER", "FRIENDS", "MARKET"] {
        assert!(r.shows(s), "{s}: {:?}", r.texts());
    }
    assert!(
        !r.shows(market_host::NO_MARKET_TIP),
        "the tip waits for the mouse"
    );
    assert!(!r.ids().contains(&"online_lobby.market".to_string()));
    r.hover_market();
    assert!(r.shows(market_host::NO_MARKET_TIP), "{:?}", r.texts());
    r.f.input(FrontInput::Move(d2_client::ui::geom::Point::new(10, 10)));
    r.ticks(1);
    assert!(!r.shows(market_host::NO_MARKET_TIP));
    r.click("online_lobby.tab_ladder");
    assert!(r.shows("CHARACTER"), "{:?}", r.texts());
    r.click("online_lobby.back");
    assert_eq!(r.f.current(), ONLINE_LOBBY);
    assert!(r.shows("NAME"));
    r.click("online_lobby.tab_friends");
    for s in [
        "FRIENDS",
        "IGNORED",
        "PRIVACY",
        "GUILD",
        "Kaern",
        "in the lobby",
    ] {
        assert!(r.shows(s), "{s}: {:?}", r.texts());
    }
}

/// Rule (plan L10): the character block asks the ladder for the
/// character's own rank and shows it; a non-ladder character has no rank
/// (nothing asked, no rank line).
#[test]
fn the_character_block_shows_the_ladder_rank() {
    let mut s = FakeServer::default();
    s.add_account(
        "Nugwa",
        "password1",
        vec![
            character("Bar", 4, 30, 5, status::EXPANSION),
            CharInfo {
                season: 1,
                ..character("Lad", 4, 30, 5, status::EXPANSION)
            },
        ],
    );
    let mut r = rig_with(s, servers());
    r.enter_lobby("Lad");
    assert!(r.seen().iter().any(|q| matches!(
        q,
        Request::LadderQuery(link::LadderQuery {
            at: link::LadderAt::Me,
            season: 1,
            ..
        })
    )));
    assert!(r.shows("Ladder rank #"), "{:?}", r.texts());

    let mut r = rig();
    r.enter_lobby("Bar");
    assert!(!r.seen().iter().any(|q| matches!(
        q,
        Request::LadderQuery(link::LadderQuery {
            at: link::LadderAt::Me,
            ..
        })
    )));
    assert!(
        !r.texts().iter().any(|t| t.contains("rank #")),
        "{:?}",
        r.texts()
    );
}

/// Rule (plan L11): the Ladder panel in the lobby: the character's own
/// view first (non-ladder here), the season chip, find with Enter (not
/// JOIN's), a click on a name starts a whisper in the chat's input line,
/// and My rank.
#[test]
fn the_ladder_panel_in_the_lobby() {
    let mut r = rig();
    r.enter_lobby("Bar");
    r.click("online_lobby.tab_ladder");
    assert!(r.shows("NON-LADDER"), "{:?}", r.texts());
    assert!(r.shows("Bar (you)"), "{:?}", r.texts());
    r.click("online_lobby.ladder_chip(season)");
    assert!(r.shows("SEASON 1"), "{:?}", r.texts());
    assert!(r.shows("Ashana"));
    let last_query = |r: &Rig| {
        r.seen()
            .into_iter()
            .rev()
            .find_map(|q| match q {
                Request::LadderQuery(q) => Some(q),
                _ => None,
            })
            .expect("a ladder query")
    };
    r.click("online_lobby.ladder_find");
    r.typed("kelmir");
    r.key(ENTER);
    assert_eq!(last_query(&r).at, link::LadderAt::Find("kelmir".into()));
    assert!(!r
        .seen()
        .iter()
        .any(|q| matches!(q, Request::JoinGame { .. })));
    assert_eq!(r.f.current(), ONLINE_LOBBY);
    r.click("online_lobby.ladder_row(Kelmir)");
    assert!(r.shows("/w Kelmir"), "{:?}", r.texts());
    r.click("online_lobby.panel(1)");
    assert_eq!(last_query(&r).at, link::LadderAt::Me);
}

/// Rule (lobby contract, CharCreate.ladder): the online create screen's
/// Ladder box makes a character of the current season.
#[test]
fn a_new_character_can_be_a_ladder_character() {
    let mut r = rig();
    r.login("nugwa", "password1");
    r.click("online_chars.create_new");
    assert!(!r.ids().contains(&"online_create.ladder".to_string()));
    r.click("online_create.class.barbarian");
    r.ticks(30);
    r.click("online_create.ladder");
    r.typed("Laddie");
    r.key(ENTER);
    r.ticks(2);
    assert_eq!(r.f.current(), ONLINE_CHARS);
    assert!(r.seen().iter().any(|q| matches!(
        q,
        Request::CharCreate { name, ladder: true, .. } if name == "Laddie"
    )));
    let season = r
        .online
        .borrow()
        .state
        .chars
        .iter()
        .find(|c| c.name == "Laddie")
        .map(|c| c.season);
    assert_eq!(season, Some(1));
}

/// Rule (plan L18): MARKET and Ctrl+I open the market window over the
/// lobby once the market answered; its tip shows on hover only; while it
/// is open the lobby's controls are dead (Esc doesn't leave the lobby);
/// Esc, BACK and the window's X close it and the lobby is as it was.
#[test]
fn the_market_opens_over_the_lobby() {
    let mut r = rig().with_fake_market();
    r.enter_lobby("Bar");
    r.ticks(2);
    assert!(
        r.ids().contains(&"online_lobby.market".to_string()),
        "{:?}",
        r.ids()
    );
    assert!(!r.shows(market_host::OPEN_TIP));
    r.hover_market();
    assert!(r.shows(market_host::OPEN_TIP), "{:?}", r.texts());
    // The ladder open underneath, to see it kept.
    r.click("online_lobby.tab_ladder");
    r.click("online_lobby.market");
    let ids = r.ids();
    assert!(ids.contains(&"market.tab(Browse)".to_string()), "{ids:?}");
    assert!(ids.contains(&"online_lobby.back".to_string()));
    assert!(
        !ids.contains(&"online_lobby.tab_ladder".to_string()),
        "{ids:?}"
    );
    assert!(r.shows("Browse") && r.shows("My shop"), "{:?}", r.texts());
    // Esc closes the window, not the lobby.
    r.key(ESC);
    assert_eq!(r.f.current(), ONLINE_LOBBY);
    assert!(!r.ids().contains(&"market.tab(Browse)".to_string()));
    assert!(r.shows("CHARACTER"), "the ladder stayed: {:?}", r.texts());
    // Ctrl+I opens and closes it; I alone types in the chat.
    r.key(17);
    r.key(u16::from(b'I'));
    r.f.input(FrontInput::KeyUp(17));
    assert!(r.ids().contains(&"market.tab(Browse)".to_string()));
    r.key(17);
    r.key(u16::from(b'I'));
    r.f.input(FrontInput::KeyUp(17));
    assert!(!r.ids().contains(&"market.tab(Browse)".to_string()));
    r.key(u16::from(b'I'));
    assert!(!r.ids().contains(&"market.tab(Browse)".to_string()));
    // BACK closes it and stays in the lobby.
    r.click("online_lobby.market");
    r.click("online_lobby.back");
    assert_eq!(r.f.current(), ONLINE_LOBBY);
    assert!(!r.ids().contains(&"market.tab(Browse)".to_string()));
    // The window's X.
    r.click("online_lobby.market");
    r.click("market.close");
    assert!(!r.ids().contains(&"market.tab(Browse)".to_string()));
    assert!(r.ids().contains(&"online_lobby.market".to_string()));
    // My shop says where selling happens.
    r.click("online_lobby.market");
    r.click("market.tab(MyShop)");
    assert!(r.shows(market_host::STASH_NOTE), "{:?}", r.texts());
}
