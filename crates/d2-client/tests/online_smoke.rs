//! The online smoke on a real `d2rust-realm` with two game servers
//! (tools/mp-test/online-smoke.sh starts them on loopback and passes the
//! main server's address and certificate): the
//! game's own screens and link, as a player uses them. A new account
//! registers, creates a character, selects it, creates a game in the lobby
//! on the game server it picks (the realm lists two, n1 and n2) and joins
//! it; the game's remote link then sends the character's join (C→S 0x67,
//! then 0x6B after S→C 0x02, as the bridge does) and the server puts the
//! player into the world (S→C 0x59 with the character's name).

#![cfg(feature = "online")]

use std::time::{Duration, Instant};

use d2_client::app::online::{self as app_online, JoinTask, OnlineJoin, ServerList};
use d2_client::app::single_player;
use d2_client::bridge::link::{SendQueue, ServerLink};
use d2_client::bridge::remote::RemoteLink;
use d2_client::ui::front_end::screens::online::net_link::{entries, NetLink};
use d2_client::ui::front_end::screens::online::{self, *};
use d2_client::ui::front_end::*;
use d2_online::{parse_fingerprint, ServerTrust};
use d2_sim::tick::TickRate;

const TAB: u16 = 9;
const ENTER: u16 = 13;

struct Smoke {
    f: FrontEnd,
}

impl Smoke {
    fn ids(&self) -> Vec<String> {
        self.f.ui_elements().into_iter().map(|e| e.id).collect()
    }

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

    /// Clicks `id` once it is shown (the server's answers take real time).
    fn click(&mut self, id: &str) {
        let end = Instant::now() + Duration::from_secs(30);
        let e = loop {
            if let Some(e) = self.f.ui_elements().into_iter().find(|e| e.id == id) {
                break e;
            }
            assert!(
                Instant::now() < end,
                "no {id} on {:?}: {:?}",
                self.f.current(),
                self.ids()
            );
            std::thread::sleep(Duration::from_millis(10));
            self.f.tick();
        };
        self.f.input(FrontInput::Down(e.at));
        self.f.input(FrontInput::Up(e.at));
        self.f.tick();
        self.f.tick();
    }

    fn key(&mut self, vk: u16) {
        self.f.input(FrontInput::Key(vk));
        self.f.tick();
    }

    fn typed(&mut self, s: &str) {
        for u in s.encode_utf16() {
            self.f.input(FrontInput::Char(u));
        }
        self.f.tick();
    }

    /// Ticks in real time until `done` (at most 30 s).
    fn until(&mut self, what: &str, mut done: impl FnMut(&mut Self) -> bool) {
        let end = Instant::now() + Duration::from_secs(30);
        while !done(self) {
            let shown = self.texts();
            assert!(
                Instant::now() < end,
                "timed out waiting for {what}: {shown:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
            self.f.tick();
        }
        eprintln!("smoke: {what}");
    }
}

/// A front end on the smoke's server, at the login screen.
fn on_the_server() -> (Smoke, OnlineHandle, app_online::OnlineSession) {
    let address = std::env::var("D2RUST_SMOKE_ADDR").expect("D2RUST_SMOKE_ADDR");
    let cert = std::env::var("D2RUST_SMOKE_CERT").expect("D2RUST_SMOKE_CERT");
    let list = ServerList {
        servers: vec![app_online::ServerEntry {
            id: "smoke".into(),
            name: "Smoke".into(),
            game: "d2rust".into(),
            address,
            trust: ServerTrust::Pinned(parse_fingerprint(&cert).expect("the certificate")),
            version: d2_client::launch::VERSION.into(),
        }],
    };
    let session = app_online::OnlineSession::default();
    let servers = entries(&list);
    let online = Online::new(Box::new(NetLink::new(session.clone(), list)), servers);
    let mut reg = Registry::default();
    screens::register_all(&mut reg);
    online::register_with(&mut reg, online.clone());
    let mut f = FrontEnd::new(true, Box::new(false), reg);
    f.start(false);
    let mut s = Smoke { f };
    s.f.trigger(Trigger::Online);
    s.f.tick();
    assert_eq!(s.f.current(), ONLINE_LOGIN);
    (s, online, session)
}

impl Smoke {
    /// Registers `account` (a fresh name per run: the server keeps
    /// accounts), creates `name` of `class` (`online_create.class.<class>`)
    /// and enters the lobby with it.
    fn enter_the_lobby(&mut self, account: &str, class: &str, name: &str) {
        let s = self;
        s.click("online_login.new_account");
        s.typed(account);
        s.key(TAB);
        s.typed("password1");
        s.key(TAB);
        s.typed("password1");
        s.key(ENTER);
        s.until("registered and logged in", |s| {
            s.f.current() == ONLINE_CHARS
        });
        s.click("online_chars.create_new");
        assert_eq!(s.f.current(), ONLINE_CREATE);
        s.click(&format!("online_create.class.{class}"));
        for _ in 0..30 {
            s.f.tick();
        }
        s.typed(name);
        s.key(ENTER);
        let id = format!("online_chars.char({name})");
        s.until("the character created", |s| {
            s.f.current() == ONLINE_CHARS && s.ids().contains(&id)
        });
        s.click(&id);
        s.click("online_chars.ok");
        s.until("the lobby", |s| s.f.current() == ONLINE_LOBBY);
    }

    fn shows(&mut self, text: &str) -> bool {
        self.texts().iter().any(|t| t.contains(text))
    }

    /// A text shown whole (a friend's place, not a chat line's words).
    fn shows_whole(&mut self, text: &str) -> bool {
        self.texts().iter().any(|t| t == text)
    }

    fn has(&self, id: &str) -> bool {
        self.ids().iter().any(|i| i == id)
    }
}

#[test]
#[ignore = "needs a running d2rust-realm and game server: tools/mp-test/online-smoke.sh"]
fn online_smoke_register_create_join_and_enter_the_world() {
    let t0 = Instant::now();
    let (mut s, online, session) = on_the_server();
    // A new account and a new sorceress.
    let account = format!("smoke{}", std::process::id() % 100_000);
    s.enter_the_lobby(&account, "sorceress", "SmokeSorc");
    // The main server is pinged beside its game servers (a player-hosted
    // game's ping is estimated from it).
    s.until("the main server's ping", |_| {
        matches!(online.borrow().state.pings.get(REALM_PING), Some(Some(_)))
    });

    // The form's SERVER drop-down: both of the realm's game servers by the
    // name it sends; no Host on my PC.
    s.click("online_lobby.tab_create");
    s.until("the game servers", |_| {
        online.borrow().state.nodes.len() == 2
    });
    s.click("online_lobby.server");
    s.until("both game servers in the drop-down", |s| {
        s.has("online_lobby.server(0)") && s.has("online_lobby.server(1)")
    });
    let nodes: Vec<String> = online
        .borrow()
        .state
        .nodes
        .iter()
        .map(|n| n.node.clone())
        .collect();
    assert_eq!(nodes, ["n1", "n2"]);
    // Test setups add their game servers with the id as the region.
    for text in ["n1", "n2"] {
        assert!(s.shows_whole(text), "{text}: {:?}", s.texts());
    }
    assert!(s.shows_whole("Automatic (lowest ping)"), "{:?}", s.texts());
    assert!(!s.has("online_lobby.server.host"), "{:?}", s.ids());
    assert!(!s.shows("Host on my PC"), "{:?}", s.texts());
    s.click("online_lobby.server(1)");
    // Closed, n2 in the box.
    assert!(!s.has("online_lobby.server(0)"), "{:?}", s.ids());
    assert!(s.shows_whole("n2"), "{:?}", s.texts());

    // A new game on n2 with a description and a level range, joined at
    // once.
    s.click("online_lobby.field(0)");
    s.typed("smoke game");
    s.click("online_lobby.field(3)");
    s.typed("smoke run");
    s.click("online_lobby.field(4)");
    s.typed("1-99");
    s.click("online_lobby.primary");
    s.until(
        "the game created and joined",
        |s| matches!(s.f.outcome(), Some(Outcome::GameLoad(g)) if g.online),
    );

    // The game: the lobby's ticket on the game stream (a game server's
    // `JoinNode`: on a connection of its own there), the remote link.
    let join = OnlineJoin::take(&mut online.borrow_mut()).expect("the lobby join");
    assert_eq!((join.name.as_str(), join.class), ("SmokeSorc", 1));
    let n = join.node.as_ref().expect("a game on a game server");
    assert_eq!(n.node, "n2", "the game server picked");
    eprintln!(
        "smoke: the game runs on the picked game server {} at {}",
        n.node, n.addr
    );
    let task = JoinTask::start_join(&session, &join).unwrap();
    let end = Instant::now() + Duration::from_secs(30);
    let game = loop {
        if let Some(a) = task.poll() {
            break a.unwrap_or_else(|e| panic!("the join: {}", e.message()));
        }
        assert!(Instant::now() < end, "no answer to the join");
        std::thread::sleep(Duration::from_millis(5));
    };
    eprintln!("smoke: the game stream attached");
    let rate = TickRate::new(game.welcome().tick_rate).expect("the server's tick rate");
    let mut link = RemoteLink::new(game, rate);
    let character = single_player::new_character("sorceress", "SmokeSorc").unwrap();
    let mut request = single_player::create_request_for(&character);
    request.difficulty = join.difficulty;
    link.send(SendQueue::System, &request.encode()).unwrap();
    let mut answered = false;
    let end = Instant::now() + Duration::from_secs(60);
    let assigned = 'world: loop {
        assert!(Instant::now() < end, "the player never entered the world");
        link.pump().expect("the link");
        for m in link.receive() {
            if m.first() == Some(&0x02) && !answered {
                answered = true;
                link.send(SendQueue::System, &[0x6B]).unwrap();
            }
            if m.first() == Some(&0x59) && m.len() >= 22 && &m[6..15] == b"SmokeSorc" {
                break 'world m;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(answered, "S→C 0x02 came before the player");
    assert_eq!(assigned[5], 1, "a sorceress");
    eprintln!(
        "smoke: SmokeSorc entered the world (S→C 0x59) {:.1} s after the start",
        t0.elapsed().as_secs_f64()
    );
}

/// The lobby's Friends panel and its Guild tab (plan L12) on the realm's
/// social service: two players add each other (mutual: the place shows),
/// one creates a guild (its tag in the character block), invites the
/// other, who accepts; both rosters show both.
#[test]
#[ignore = "needs a running d2rust-realm and game server: tools/mp-test/online-smoke.sh"]
fn online_smoke_friends_and_guild_panels() {
    let pid = std::process::id() % 100_000;
    let (mut a, _online_a, _) = on_the_server();
    a.enter_the_lobby(&format!("smokea{pid}"), "amazon", "Ashkar");
    let (mut b, _online_b, _) = on_the_server();
    b.enter_the_lobby(&format!("smokeb{pid}"), "sorceress", "Brennic");

    // Friends: one-sided, then mutual.
    a.click("online_lobby.tab_friends");
    a.until("A's empty friends list", |s| {
        s.shows("No friends yet: add one below.")
    });
    a.click("online_lobby.friends.name");
    a.typed("Brennic");
    a.click("online_lobby.friends.add");
    a.until("Brennic on A's list", |s| {
        s.has("online_lobby.friend(Brennic)") && s.shows("Brennic is on your friends list.")
    });
    // Not mutual yet: online, but not where.
    assert!(a.shows_whole("online"), "{:?}", a.texts());
    assert!(!a.shows_whole("in the lobby"), "{:?}", a.texts());
    b.click("online_lobby.tab_friends");
    b.until("B's friends panel", |s| s.has("online_lobby.friends.name"));
    b.click("online_lobby.friends.name");
    b.typed("Ashkar");
    b.click("online_lobby.friends.add");
    b.until("Ashkar on B's list, mutual, in the lobby", |s| {
        s.has("online_lobby.friend(Ashkar)") && s.shows_whole("in the lobby")
    });
    a.until("Brennic mutual on A's list: in the lobby", |s| {
        s.shows_whole("in the lobby")
    });

    // Guild: A creates one; the tag reaches its character block.
    a.click("online_lobby.friends_tab(guild)");
    a.until("A's create form", |s| s.has("online_lobby.guild.create"));
    a.click("online_lobby.guild.tag");
    a.typed("SMK");
    a.click("online_lobby.guild.name");
    a.typed("Smoke Guild");
    a.click("online_lobby.guild.create");
    a.until("A leads <SMK>, its tag in the character block", |s| {
        s.has("online_lobby.guild_member(Ashkar)")
            && s.has("online_lobby.guild.disband")
            && s.shows("Guild <SMK>")
    });
    a.click("online_lobby.guild.text");
    a.typed("Brennic");
    a.click("online_lobby.guild.invite");
    a.until("Brennic invited", |s| {
        s.shows("Brennic is invited to <SMK>.")
    });

    // B accepts: both rosters show both, B's character block the tag.
    b.click("online_lobby.friends_tab(guild)");
    b.until("B's invite to <SMK>", |s| {
        s.has("online_lobby.guild_invite(SMK)")
    });
    b.click("online_lobby.guild.accept");
    b.until("B in <SMK>", |s| {
        s.has("online_lobby.guild_member(Ashkar)")
            && s.has("online_lobby.guild_member(Brennic)")
            && s.has("online_lobby.guild.leave")
            && !s.has("online_lobby.guild.kick")
            && s.shows("Guild <SMK>")
    });
    a.until("Brennic on A's roster", |s| {
        s.has("online_lobby.guild_member(Brennic)")
    });
}
