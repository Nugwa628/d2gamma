//! The online screens on the user's install, rendered without a window
//! (`front_host::compose`), against the in-memory fake server: one PNG per
//! screen and state for review. Ignored: needs `D2_GAME_DIR`; `D2_SHOTS`
//! names the output folder (default: the temp dir). Run with
//! `--features online` for the main menu's ONLINE button. The game-server
//! choice (multiplayer v2) is also captured at 2160p through the native
//! UI pass (`FrontHost::native_rgba`), on a stub list of game servers,
//! and (feature `online`) on a real main server's own list
//! (tools/mp-test/online-smoke.sh with `D2_SHOTS`).
//! The lobby's movable panels (plan L15) are captured classic, moved,
//! resized, locked and reset, with both covers of the painted frames.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use d2_client::app::front_host::{add_front_end, compose_canvas_split, FrontArt, FrontHost};
use d2_client::app::single_player::GameData;
use d2_client::ui::front_end::screens::online::fake::{character, FakeServer, SharedFake};
use d2_client::ui::front_end::screens::online::lobby::WIDE_RIGHT;
use d2_client::ui::front_end::screens::online::{
    self, GameInfo, NodeInfo, NodeState, Online, OnlineHandle, PingTable, ServerEntry,
};
use d2_client::ui::front_end::*;
use d2_client::ui::geom::Point;
use d2_formats::d2s::status;

fn app(host: FrontHost) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        bevy::asset::AssetPlugin::default(),
        bevy::input::InputPlugin,
    ))
    .init_asset::<Image>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        40,
    )));
    add_front_end(&mut app, host);
    app
}

fn host(a: &mut App) -> Mut<'_, FrontHost> {
    a.world_mut().non_send_mut::<FrontHost>()
}

fn ticks(a: &mut App, n: u32) {
    for _ in 0..n {
        a.update();
    }
}

fn input(a: &mut App, ev: FrontInput) {
    host(a).front.input(ev);
    ticks(a, 2);
}

fn click_id(a: &mut App, id: &str) {
    let e = host(a)
        .front
        .ui_elements()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no {id}"));
    host(a).front.input(FrontInput::Move(e.at));
    host(a).front.input(FrontInput::Down(e.at));
    ticks(a, 1);
    host(a).front.input(FrontInput::Up(e.at));
    ticks(a, 6);
}

fn has_id(a: &mut App, id: &str) -> bool {
    host(a).front.ui_elements().iter().any(|e| e.id == id)
}

fn typed(a: &mut App, s: &str) {
    for u in s.encode_utf16() {
        host(a).front.input(FrontInput::Char(u));
    }
    ticks(a, 2);
}

fn shot(a: &mut App, out: &Path, name: &str) -> Vec<u8> {
    // The pointer out of the way.
    let corner = corner(a);
    host(a).front.input(FrontInput::Move(corner));
    ticks(a, 7);
    capture(a, out, name)
}

/// The canvas's bottom-right pixel (the 16:9 lobby's canvas is wider).
fn corner(a: &mut App) -> Point {
    let c = host(a).front.canvas();
    Point::new(c.width - 1, c.height - 1)
}

/// The frame as it is (the pointer where it is) to `out/name.png`, at the
/// canvas's size.
fn capture(a: &mut App, out: &Path, name: &str) -> Vec<u8> {
    let mut h = host(a);
    let h = &mut *h;
    let mut items = h.front.draw();
    let over = match h.art.as_mut() {
        Some(art) => {
            let art = std::cell::RefCell::new(art);
            h.front
                .overlay(&|font, text| art.borrow_mut().text_width(font, text))
        }
        None => h.front.overlay(&|_, _| 0),
    };
    items.extend(over);
    let palette = h.front.palette();
    if let Some(art) = h.art.as_mut() {
        art.use_palette(palette);
    }
    let c = h.front.canvas();
    let size = (c.width as u32, c.height as u32);
    let rgba = compose_canvas_split(&items, items.len(), h.art.as_mut(), size, |_, _| {});
    let path = out.join(format!("{name}.png"));
    let file = std::fs::File::create(&path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size.0, size.1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    println!("wrote {} (screen {:?})", path.display(), h.front.current());
    rgba
}

/// The frame at 2160p through the native UI pass: 2880x2160 for a 4:3
/// screen, 3840x2160 for the 16:9 lobby.
fn native_shot(a: &mut App, out: &Path, name: &str) {
    let c = host(a).front.canvas();
    let (w, h) = (if c.full { 3840 } else { 2880 }, 2160);
    let place = d2_client::gpu_compositor::ui_pass::UiPlacement {
        frame_w: c.width as u32,
        frame_h: c.height as u32,
        out_w: w,
        out_h: h,
    };
    let corner = corner(a);
    host(a).front.input(FrontInput::Move(corner));
    ticks(a, 7);
    let rgba = host(a).native_rgba(place).expect("native frame");
    let path = out.join(format!("{name}.png"));
    let file = std::fs::File::create(&path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    println!("wrote {}", path.display());
}

/// Where the captures go (`D2_SHOTS`, else the temp dir).
fn shots_dir() -> PathBuf {
    let out = std::env::var_os("D2_SHOTS")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&out).unwrap();
    out
}

/// The front end on the install's art with the online screens on `fake`.
fn front_on_install(fake: FakeServer) -> (App, SharedFake, OnlineHandle) {
    front_on_install_with(fake, None)
}

/// [`front_on_install`] with the lobby's chat on `chat` (a fake realm chat).
fn front_on_install_with(
    fake: FakeServer,
    chat: Option<Box<dyn d2_client::chat::ChatSource>>,
) -> (App, SharedFake, OnlineHandle) {
    front_on_install_cover(fake, chat, online::layout::Cover::Patch)
}

/// [`front_on_install_with`], the lobby covering its painted frames with
/// `cover` when a panel moves.
fn front_on_install_cover(
    fake: FakeServer,
    chat: Option<Box<dyn d2_client::chat::ChatSource>>,
    cover: online::layout::Cover,
) -> (App, SharedFake, OnlineHandle) {
    let server = SharedFake(std::rc::Rc::new(std::cell::RefCell::new(fake)));
    let servers = vec![ServerEntry {
        id: "eu1".into(),
        name: "D2Gamma".into(),
        address: "realm.example:6112".into(),
    }];
    let (a, handle) = front_on_install_link(Box::new(server.clone()), servers, chat, cover);
    (a, server, handle)
}

/// The front end on the install's art with the online screens on `link`.
fn front_on_install_link(
    link: Box<dyn online::OnlineLink>,
    servers: Vec<ServerEntry>,
    chat: Option<Box<dyn d2_client::chat::ChatSource>>,
    cover: online::layout::Cover,
) -> (App, OnlineHandle) {
    use d2_data::bin::TableFiles;
    let game_dir = PathBuf::from(std::env::var("D2_GAME_DIR").expect("D2_GAME_DIR"));
    let data = GameData::select(Some(&game_dir)).expect("live data");
    let GameData::Live(d) = &data;
    let mut art = FrontArt::new(d.archives.source());
    if let Ok(t) = d2_client::app::strings::TableStrings::load(
        d.archives.as_ref(),
        d2_client::app::strings::LANG,
    ) {
        art = art.with_strings(move |id| u16::try_from(id).map(|i| t.by_id(i)).unwrap_or_default());
    }
    let handle = Online::new(link, servers);
    let mut reg = Registry::default();
    screens::register_all(&mut reg);
    d2_client::app::front_host::register_credits(&mut reg, &art);
    online::register_with(&mut reg, handle.clone());
    if let Some(src) = chat {
        use online::chat_pane::ChatPane;
        use online::lobby::Lobby;
        let mut lobby = Lobby::with_chat(handle.clone(), ChatPane::with_source(src));
        lobby.layout.cover = cover;
        reg.register(online::ONLINE_LOBBY, Box::new(lobby));
    }
    let front = FrontEnd::new(d.archives.lod(), Box::new(false), reg);
    let mut a = app(FrontHost::with_front(front, Some(art), false));
    ticks(&mut a, 30);
    (a, handle)
}

fn game(name: &str, difficulty: u8, players: u8, locked: bool) -> GameInfo {
    GameInfo {
        id: 0,
        name: name.into(),
        difficulty,
        players,
        max_players: 8,
        locked,
        hardcore: false,
        expansion: true,
        joinable: 0,
        ..Default::default()
    }
}

#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn online_screens_on_the_install() {
    let out = shots_dir();
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![
            character("Werwer", 4, 34, 5, status::EXPANSION),
            character("Sparks", 1, 12, 0, status::EXPANSION),
            character("Ironwolf", 6, 71, 10, status::EXPANSION),
        ],
    );
    fake.add_account("Other", "password2", vec![character("Taken", 0, 1, 0, 0)]);
    fake.add_game(game("cows", 0, 3, false), "");
    fake.add_game(game("baal run", 2, 7, false), "");
    fake.add_game(game("nm chaos", 1, 2, true), "moo");
    fake.add_game(game("full house", 0, 8, false), "");
    let (mut a, server, _handle) = front_on_install(fake);
    shot(&mut a, &out, "online_00_main_menu");

    // Login.
    host(&mut a).front.trigger(Trigger::Online);
    ticks(&mut a, 10);
    shot(&mut a, &out, "online_01_login");
    typed(&mut a, "nugwa");
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "wrong-pass");
    input(&mut a, FrontInput::Key(13));
    ticks(&mut a, 4);
    shot(&mut a, &out, "online_02_login_wrong_password");
    click_id(&mut a, "online_login.message_ok");
    click_id(&mut a, "online_login.new_account");
    click_id(&mut a, "online_login.field(1)");
    typed(&mut a, "secret12");
    shot(&mut a, &out, "online_03_new_account");
    click_id(&mut a, "online_login.new_account");
    server.0.borrow_mut().hold = true;
    click_id(&mut a, "online_login.field(1)");
    for _ in 0..40 {
        host(&mut a).front.input(FrontInput::Key(8));
    }
    ticks(&mut a, 2);
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(13));
    shot(&mut a, &out, "online_04_logging_in");
    server.0.borrow_mut().release();
    ticks(&mut a, 4);
    server.0.borrow_mut().release();
    ticks(&mut a, 6);
    server.0.borrow_mut().release();
    ticks(&mut a, 6);

    // Characters.
    shot(&mut a, &out, "online_05_characters");
    click_id(&mut a, "online_chars.char(Sparks)");
    click_id(&mut a, "online_chars.delete");
    click_id(&mut a, "online_chars.yes");
    typed(&mut a, "pass");
    shot(&mut a, &out, "online_06_delete_password");
    click_id(&mut a, "online_chars.delete_cancel");

    // Create in online mode, a taken name.
    click_id(&mut a, "online_chars.create_new");
    click_id(&mut a, "online_create.class.amazon");
    ticks(&mut a, 40);
    typed(&mut a, "Taken");
    input(&mut a, FrontInput::Key(13));
    ticks(&mut a, 4);
    shot(&mut a, &out, "online_07_create_name_taken");
    click_id(&mut a, "online_create.message_ok");
    input(&mut a, FrontInput::Key(27));
    ticks(&mut a, 6);

    // Lobby with Werwer (Nightmare unlocked, Hell not).
    click_id(&mut a, "online_chars.char(Werwer)");
    click_id(&mut a, "online_chars.ok");
    ticks(&mut a, 6);
    server
        .0
        .borrow_mut()
        .chat_from("Ironwolf", "anyone for cows?");
    server
        .0
        .borrow_mut()
        .chat_from("Sparks", "need a key for the nightmare game, it is moo");
    ticks(&mut a, 4);
    shot(&mut a, &out, "online_08_lobby");
    click_id(&mut a, "online_lobby.game(baal run)");
    shot(&mut a, &out, "online_09_lobby_locked_difficulty");
    click_id(&mut a, "online_lobby.game(nm chaos)");
    click_id(&mut a, "online_lobby.primary");
    typed(&mut a, "mo");
    shot(&mut a, &out, "online_10_password_prompt");
    click_id(&mut a, "online_lobby.prompt_cancel");
    click_id(&mut a, "online_lobby.tab_create");
    click_id(&mut a, "online_lobby.field(0)");
    typed(&mut a, "werwer games");
    click_id(&mut a, "online_lobby.field(1)");
    typed(&mut a, "pw");
    shot(&mut a, &out, "online_11_create_game");
    click_id(&mut a, "online_lobby.field(0)");
    for _ in 0..12 {
        host(&mut a).front.input(FrontInput::Key(8));
    }
    typed(&mut a, "cows");
    click_id(&mut a, "online_lobby.primary");
    ticks(&mut a, 4);
    shot(&mut a, &out, "online_12_create_name_taken");
    click_id(&mut a, "online_lobby.message_ok");
    click_id(&mut a, "online_lobby.tab_join");
    typed(&mut a, "hello all");
    shot(&mut a, &out, "online_13_lobby_chat_typing");

    // A lost connection: back to login with the reason.
    server
        .0
        .borrow_mut()
        .drop_connection("The connection to the server was lost.");
    ticks(&mut a, 6);
    shot(&mut a, &out, "online_14_connection_lost");
}

/// An official game server of the stub list.
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

/// The game-server choice (plan N12) at 2160p: the game list with pings,
/// the create form's SERVER drop-down while the pings are measured, with
/// them (Automatic on the best ping), open (a full and a closing server
/// grey) and another server chosen, the last three also at 800x600. The
/// servers show by their short region name (owner 2026-10-24); no Host on
/// my PC while the hosting switch is off.
#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn game_server_choice_on_the_install() {
    let out = shots_dir();
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.hosting = false;
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    let nodes = vec![
        node("hel-1", "Finland", "Finland", 42, NodeState::Open),
        node("fra-1", "Germany", "Germany", 12, NodeState::Open),
        node("nyc-1", "US East", "US East", 100, NodeState::Full),
        node("sgp-1", "Singapore", "Singapore", 30, NodeState::Draining),
    ];
    fake.nodes = nodes.clone();
    let on = |name: &str, difficulty: u8, players: u8, server: &str, hosted: bool| GameInfo {
        server: server.into(),
        player_hosted: hosted,
        ..game(name, difficulty, players, false)
    };
    fake.add_game(on("cows", 0, 3, "Finland", false), "");
    fake.add_game(on("baal run", 1, 7, "Germany", false), "");
    fake.add_game(on("nyc rush", 0, 8, "US East", false), "");
    let (mut a, server, _handle) = front_on_install(fake);
    let pinged: PingTable = [
        ("hel-1", Some(21)),
        ("fra-1", Some(34)),
        ("nyc-1", Some(96)),
        ("sgp-1", None),
    ]
    .into_iter()
    .map(|(n, p)| (n.to_string(), p))
    .collect();

    host(&mut a).front.trigger(Trigger::Online);
    ticks(&mut a, 10);
    typed(&mut a, "nugwa");
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(13));
    ticks(&mut a, 8);
    click_id(&mut a, "online_chars.char(Werwer)");
    click_id(&mut a, "online_chars.ok");
    ticks(&mut a, 6);
    // The pings arrive while the list is up.
    server
        .0
        .borrow_mut()
        .set_nodes(nodes.clone(), pinged.clone());
    ticks(&mut a, 4);
    click_id(&mut a, "online_lobby.game(cows)");
    native_shot(&mut a, &out, "n12_01_game_list_2160");

    // The form while the pings are measured, then with them.
    server
        .0
        .borrow_mut()
        .set_nodes(nodes.clone(), PingTable::new());
    ticks(&mut a, 4);
    click_id(&mut a, "online_lobby.tab_create");
    click_id(&mut a, "online_lobby.field(0)");
    typed(&mut a, "werwer games");
    native_shot(&mut a, &out, "n12_03_create_measuring_2160");
    server.0.borrow_mut().set_nodes(nodes.clone(), pinged);
    ticks(&mut a, 4);
    shot(&mut a, &out, "n12_04_create_automatic");
    native_shot(&mut a, &out, "n12_04_create_automatic_2160");
    click_id(&mut a, "online_lobby.server");
    shot(&mut a, &out, "n12_05_create_drop_down_open");
    native_shot(&mut a, &out, "n12_05_create_drop_down_open_2160");
    click_id(&mut a, "online_lobby.server(1)");
    shot(&mut a, &out, "n12_06_create_server_chosen");
    native_shot(&mut a, &out, "n12_06_create_server_chosen_2160");
}

/// The SERVER drop-down on a real main server with two game servers (n1,
/// n2: tools/mp-test/online-smoke.sh starts them and passes
/// `D2RUST_SMOKE_ADDR`/`D2RUST_SMOKE_CERT`), at 800x600 and 2160p: a new
/// account's create form (Automatic), the drop-down open with the realm's
/// own list, then n2 chosen.
#[cfg(feature = "online")]
#[test]
#[ignore = "needs D2_GAME_DIR and a running main server: tools/mp-test/online-smoke.sh"]
fn real_realm_server_drop_down_on_the_install() {
    use d2_client::app::online::{self as app_online, ServerList};
    use d2_client::ui::front_end::screens::online::net_link::{entries, NetLink};
    use d2_online::{parse_fingerprint, ServerTrust};
    let out = shots_dir();
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
    let servers = entries(&list);
    let link = NetLink::new(app_online::OnlineSession::default(), list);
    let (mut a, online) =
        front_on_install_link(Box::new(link), servers, None, online::layout::Cover::Patch);
    // Real time: the server's answers come over loopback.
    let until = |a: &mut App, what: &str, done: &dyn Fn(&mut App) -> bool| {
        let end = std::time::Instant::now() + Duration::from_secs(30);
        while !done(a) {
            if std::time::Instant::now() >= end {
                let ids: Vec<String> = host(a)
                    .front
                    .ui_elements()
                    .into_iter()
                    .map(|e| e.id)
                    .collect();
                panic!("timed out waiting for {what}: {ids:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
            ticks(a, 1);
        }
    };
    let has = |a: &mut App, id: &str| host(a).front.ui_elements().iter().any(|e| e.id == id);
    host(&mut a).front.trigger(Trigger::Online);
    ticks(&mut a, 10);
    click_id(&mut a, "online_login.new_account");
    typed(&mut a, &format!("shots{}", std::process::id() % 100_000));
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(13));
    until(&mut a, "registered", &|a| has(a, "online_chars.create_new"));
    click_id(&mut a, "online_chars.create_new");
    click_id(&mut a, "online_create.class.paladin");
    ticks(&mut a, 30);
    typed(&mut a, "Picker");
    input(&mut a, FrontInput::Key(13));
    until(&mut a, "the character", &|a| {
        has(a, "online_chars.char(Picker)")
    });
    click_id(&mut a, "online_chars.char(Picker)");
    click_id(&mut a, "online_chars.ok");
    until(&mut a, "the lobby", &|a| has(a, "online_lobby.tab_create"));
    // The lobby takes clicks once it has entered: again until the form shows.
    until(&mut a, "the create form", &|a| {
        if has(a, "online_lobby.field(0)") {
            return true;
        }
        click_id(a, "online_lobby.tab_create");
        false
    });
    until(&mut a, "both game servers, open, and their pings", &|a| {
        let pinged = {
            let st = &online.borrow().state;
            st.nodes.len() == 2
                && st
                    .nodes
                    .iter()
                    .all(|n| st.ping(&n.node).flatten().is_some())
        };
        pinged && has(a, "online_lobby.server")
    });
    ticks(&mut a, 4);
    click_id(&mut a, "online_lobby.field(0)");
    typed(&mut a, "drop-down game");
    shot(&mut a, &out, "real_realm_01_create_closed");
    native_shot(&mut a, &out, "real_realm_01_create_closed_2160");
    click_id(&mut a, "online_lobby.server");
    until(&mut a, "the drop-down open", &|a| {
        has(a, "online_lobby.server(0)") && has(a, "online_lobby.server(1)")
    });
    assert!(!has(&mut a, "online_lobby.server.host"));
    shot(&mut a, &out, "real_realm_02_drop_down_open");
    native_shot(&mut a, &out, "real_realm_02_drop_down_open_2160");
    click_id(&mut a, "online_lobby.server(1)");
    assert!(!has(&mut a, "online_lobby.server(1)"));
    shot(&mut a, &out, "real_realm_03_n2_chosen");
    native_shot(&mut a, &out, "real_realm_03_n2_chosen_2160");
}

/// The lobby's chat module (plan L9) on a fake realm chat, next to the
/// approved preview's left half: the Mod tab with its lines, a market
/// notice and the "In chat (38)" column, the unread badges of Guild and DM;
/// then the Guild tab, the DM tab with Kaern's conversation, and a typed
/// whisper.
#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn lobby_chat_on_the_install() {
    use d2_client::chat::FakeChat;
    let out = shots_dir();
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    fake.add_game(game("cows", 0, 3, false), "");
    fake.add_game(game("nm chaos", 1, 2, true), "moo");
    let (mut a, _server, _handle) = front_on_install_with(fake, Some(Box::new(FakeChat::seeded())));
    host(&mut a).front.trigger(Trigger::Online);
    ticks(&mut a, 10);
    typed(&mut a, "nugwa");
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(13));
    ticks(&mut a, 8);
    click_id(&mut a, "online_chars.char(Werwer)");
    click_id(&mut a, "online_chars.ok");
    ticks(&mut a, 6);
    shot(&mut a, &out, "l9_01_lobby_mod_tab");
    click_id(&mut a, "online_lobby.chat_tab(guild)");
    shot(&mut a, &out, "l9_02_lobby_guild_tab");
    click_id(&mut a, "online_lobby.chat_tab(dm)");
    shot(&mut a, &out, "l9_03_lobby_dm_tab");
    click_id(&mut a, "online_lobby.chat_tab(mod)");
    typed(&mut a, "/w Kaern see you at 9");
    shot(&mut a, &out, "l9_04_lobby_typing");
}

/// The lobby's right panel v2 (plan L10), next to the previews
/// `preview-lobby-games.png` and `preview-lobby-ladder.png`: the four
/// buttons, the greyed market, the character block with its standing, the
/// game list with the server column (an official server's ping, a player
/// host's estimate), greyed games with their reasons and a password game;
/// a selected game; the create form with a description and a level range;
/// the ladder and friends panels; the character create screen's Ladder
/// box; and the game list at 2160p through the native UI pass.
#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn lobby_v2_on_the_install() {
    let out = shots_dir();
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    let nodes = vec![
        node("eu-1", "EU N1", "Helsinki, Finland", 30, NodeState::Open),
        node("eu-2", "EU N2", "Frankfurt, Germany", 60, NodeState::Open),
    ];
    let pings: PingTable = [
        ("eu-1", Some(18)),
        ("eu-2", Some(31)),
        (online::REALM_PING, Some(22)),
    ]
    .into_iter()
    .map(|(n, p)| (n.to_string(), p))
    .collect();
    fake.set_nodes(nodes, pings);
    let on = |g: GameInfo, server: &str| GameInfo {
        server: server.into(),
        ..g
    };
    let mut baal = on(game("baal runs", 1, 5, false), "EU N1");
    baal.description = "fast runs, no leech".into();
    baal.level_min = 30;
    baal.level_max = 60;
    let mut chill = on(game("chill trading", 0, 2, true), "Rhaego");
    chill.player_hosted = true;
    chill.host_rtt_ms = 42;
    let mut hc = on(game("hc trades", 0, 3, false), "EU N2");
    hc.hardcore = true;
    let mut ubers = on(game("ubers", 1, 4, false), "EU N1");
    ubers.level_min = 80;
    for (g, pw) in [
        (baal, ""),
        (on(game("cows", 0, 3, false), "EU N2"), ""),
        (chill, "moo"),
        (hc, ""),
        (on(game("hell rush", 2, 1, false), "EU N1"), ""),
        (on(game("packed", 1, 8, false), "EU N2"), ""),
        (ubers, ""),
    ] {
        fake.add_game(g, pw);
    }
    // The chat from `FakeChat` (Werwer of guild NUG: the tag); the games,
    // servers and ladder from the online fake.
    let chat = d2_client::chat::FakeChat::seeded();
    let (mut a, _server, _handle) = front_on_install_with(fake, Some(Box::new(chat)));
    host(&mut a).front.trigger(Trigger::Online);
    ticks(&mut a, 10);
    typed(&mut a, "nugwa");
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(13));
    ticks(&mut a, 8);
    click_id(&mut a, "online_chars.char(Werwer)");
    click_id(&mut a, "online_chars.ok");
    ticks(&mut a, 6);
    shot(&mut a, &out, "l10_01_games");
    click_id(&mut a, "online_lobby.game(baal runs)");
    shot(&mut a, &out, "l10_02_games_selected");
    click_id(&mut a, "online_lobby.game(chill trading)");
    shot(&mut a, &out, "l10_03_player_host_selected");
    click_id(&mut a, "online_lobby.game(hc trades)");
    shot(&mut a, &out, "l10_04_grey_selected");
    native_shot(&mut a, &out, "l10_05_games_2160p");
    click_id(&mut a, "online_lobby.tab_create");
    shot(&mut a, &out, "l10_06_create_empty");
    click_id(&mut a, "online_lobby.field(0)");
    typed(&mut a, "baal runs 2");
    click_id(&mut a, "online_lobby.field(3)");
    typed(&mut a, "fast runs, no leech");
    click_id(&mut a, "online_lobby.field(4)");
    typed(&mut a, "30-40");
    shot(&mut a, &out, "l10_07_create_filled");
    click_id(&mut a, "online_lobby.tab_ladder");
    shot(&mut a, &out, "l10_08_ladder");
    click_id(&mut a, "online_lobby.tab_friends");
    shot(&mut a, &out, "l10_09_friends");
    click_id(&mut a, "online_lobby.back");
    click_id(&mut a, "online_lobby.back");
    ticks(&mut a, 6);
    click_id(&mut a, "online_chars.create_new");
    ticks(&mut a, 10);
    click_id(&mut a, "online_create.class.barbarian");
    ticks(&mut a, 30);
    click_id(&mut a, "online_create.ladder");
    shot(&mut a, &out, "l10_10_char_create_ladder");
}

/// The lobby's Ladder panel (plan L11) next to `preview-lobby-ladder.png`:
/// Werwer of Season 1 (ranked 45th) with the DM tab open; the first page
/// with the own row pinned; the hardcore ladder with its dead characters;
/// a found character; My rank; the Barbarians; a name not on the ladder;
/// and the panel at 2160p through the native UI pass.
#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn lobby_ladder_on_the_install() {
    let out = shots_dir();
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    let mut werwer = character("Werwer", 4, 34, 5, status::EXPANSION);
    werwer.season = 1;
    fake.add_account("Nugwa", "password1", vec![werwer]);
    let chat = d2_client::chat::FakeChat::seeded();
    let (mut a, _server, _handle) = front_on_install_with(fake, Some(Box::new(chat)));
    host(&mut a).front.trigger(Trigger::Online);
    ticks(&mut a, 10);
    typed(&mut a, "nugwa");
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(13));
    ticks(&mut a, 8);
    click_id(&mut a, "online_chars.char(Werwer)");
    click_id(&mut a, "online_chars.ok");
    ticks(&mut a, 6);
    click_id(&mut a, "online_lobby.chat_tab(dm)");
    click_id(&mut a, "online_lobby.tab_ladder");
    shot(&mut a, &out, "l11_01_ladder");
    click_id(&mut a, "online_lobby.ladder_chip(hc)");
    shot(&mut a, &out, "l11_02_hardcore_dead");
    click_id(&mut a, "online_lobby.ladder_chip(sc)");
    click_id(&mut a, "online_lobby.ladder_find");
    typed(&mut a, "kelmir");
    input(&mut a, FrontInput::Key(13));
    shot(&mut a, &out, "l11_03_found");
    click_id(&mut a, "online_lobby.panel(1)");
    shot(&mut a, &out, "l11_04_my_rank");
    click_id(&mut a, "online_lobby.ladder_chip(bar)");
    shot(&mut a, &out, "l11_05_barbarians");
    click_id(&mut a, "online_lobby.ladder_find");
    for _ in 0..6 {
        input(&mut a, FrontInput::Key(8));
    }
    typed(&mut a, "Nobody");
    input(&mut a, FrontInput::Key(13));
    shot(&mut a, &out, "l11_06_not_found");
    click_id(&mut a, "online_lobby.ladder_chip(all)");
    native_shot(&mut a, &out, "l11_07_ladder_2160p");
}

/// The Friends panel and its Guild tab (plan L12) over the fake's realm
/// link (the lobby's own chat source, so the character block's guild tag
/// comes from the realm's social hello): the friends list with a mutual
/// friend in a game selected, a name typed to add, the ignore list, the
/// privacy choices; the guild as its leader with a member selected, the
/// disband prompt, the guild handed over (the officer's buttons), and after
/// leaving: the invite and the create form filled in.
#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn friends_and_guild_on_the_install() {
    let out = shots_dir();
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    fake.add_game(game("cows", 0, 3, false), "");
    let (mut a, _server, _handle) = front_on_install(fake);
    host(&mut a).front.trigger(Trigger::Online);
    ticks(&mut a, 10);
    typed(&mut a, "nugwa");
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(13));
    ticks(&mut a, 8);
    click_id(&mut a, "online_chars.char(Werwer)");
    click_id(&mut a, "online_chars.ok");
    ticks(&mut a, 6);
    click_id(&mut a, "online_lobby.tab_friends");
    click_id(&mut a, "online_lobby.friend(Rhaego)");
    shot(&mut a, &out, "l12_01_friends");
    click_id(&mut a, "online_lobby.friends.name");
    typed(&mut a, "Kestrel");
    shot(&mut a, &out, "l12_02_friends_typing");
    click_id(&mut a, "online_lobby.friends.add");
    ticks(&mut a, 4);
    shot(&mut a, &out, "l12_03_friends_added");
    click_id(&mut a, "online_lobby.friends_tab(ignored)");
    shot(&mut a, &out, "l12_04_ignored");
    click_id(&mut a, "online_lobby.friends_tab(privacy)");
    shot(&mut a, &out, "l12_05_privacy");
    click_id(&mut a, "online_lobby.friends_tab(guild)");
    ticks(&mut a, 4);
    click_id(&mut a, "online_lobby.guild_member(Kaern)");
    shot(&mut a, &out, "l12_06_guild_leader");
    click_id(&mut a, "online_lobby.guild.disband");
    shot(&mut a, &out, "l12_07_guild_disband_prompt");
    click_id(&mut a, "online_lobby.guild.leader");
    click_id(&mut a, "online_lobby.guild.leader");
    ticks(&mut a, 6);
    shot(&mut a, &out, "l12_08_guild_officer");
    click_id(&mut a, "online_lobby.guild.leave");
    click_id(&mut a, "online_lobby.guild.leave");
    ticks(&mut a, 6);
    shot(&mut a, &out, "l12_09_no_guild");
    click_id(&mut a, "online_lobby.guild.tag");
    typed(&mut a, "NEW");
    click_id(&mut a, "online_lobby.guild.name");
    typed(&mut a, "New Order");
    shot(&mut a, &out, "l12_10_create_filled");
}

/// Presses at `from`, moves to `to` in steps (a drag), lets go.
fn drag(a: &mut App, from: Point, to: Point) {
    input(a, FrontInput::Move(from));
    input(a, FrontInput::Down(from));
    for i in 1..=4 {
        let p = Point::new(
            from.x + (to.x - from.x) * i / 4,
            from.y + (to.y - from.y) * i / 4,
        );
        input(a, FrontInput::Move(p));
    }
    input(a, FrontInput::Up(to));
    ticks(a, 4);
}

/// The lobby of `lobby_v2_on_the_install`'s games on the install, Werwer
/// logged in.
fn lobby_on_install(cover: online::layout::Cover) -> App {
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    let mut baal = game("baal runs", 1, 5, false);
    baal.description = "fast runs, no leech".into();
    for (g, pw) in [
        (baal, ""),
        (game("cows", 0, 3, false), ""),
        (game("chill trading", 0, 2, true), "moo"),
        (game("hell rush", 2, 1, false), ""),
        (game("packed", 1, 8, false), ""),
        (game("ubers", 1, 4, false), ""),
        (game("tomb runs", 0, 2, false), ""),
        (game("trade hub", 0, 6, false), ""),
        (game("anc help", 0, 3, false), ""),
    ] {
        fake.add_game(g, pw);
    }
    let chat = d2_client::chat::FakeChat::seeded();
    let (mut a, _server, _handle) = front_on_install_cover(fake, Some(Box::new(chat)), cover);
    host(&mut a).front.trigger(Trigger::Online);
    ticks(&mut a, 10);
    typed(&mut a, "nugwa");
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(13));
    ticks(&mut a, 8);
    click_id(&mut a, "online_chars.char(Werwer)");
    click_id(&mut a, "online_chars.ok");
    ticks(&mut a, 6);
    a
}

/// The chat's title strip right of its tabs (the DM tab's unread count
/// widens it), and the right panel's over its buttons, at its 16:9 place.
const CHAT_TITLE: Point = Point::new(392, 166);
const RIGHT_TITLE: Point = Point::new(700 + WIDE_RIGHT, 258);
/// The right panel's left edge and its bottom right at 16:9.
const RIGHT_EDGE: i32 = 447 + WIDE_RIGHT;
const RIGHT_CORNER: Point = Point::new(560 + WIDE_RIGHT, 538);

/// The panels moved: the chat up and right, the right panel right.
fn move_panels(a: &mut App) {
    drag(
        a,
        CHAT_TITLE,
        Point::new(CHAT_TITLE.x + 24, CHAT_TITLE.y - 40),
    );
    drag(
        a,
        RIGHT_TITLE,
        Point::new(RIGHT_TITLE.x + 24, RIGHT_TITLE.y),
    );
}

/// The lobby's movable panels (plan L15): classic (as before L15), moved
/// (the painted frames covered by the patch, the panels in their own gold
/// frames), resized (a narrower chat, a wider and taller list), locked (a
/// drag does nothing), reset (classic again, the same pixels as the first
/// capture), the moved layout at 2160p through the native UI pass, and
/// the moved layout with the other cover (the character-select art cut at
/// the same place) for comparing the two.
#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn lobby_layout_on_the_install() {
    use online::layout::Cover;
    let out = shots_dir();
    let mut a = lobby_on_install(Cover::Patch);
    assert!(host(&mut a).front.wide(), "the lobby is 16:9");
    let classic = shot(&mut a, &out, "l15_01_classic");
    move_panels(&mut a);
    let moved = shot(&mut a, &out, "l15_02_moved");
    assert_ne!(moved, classic, "the panels did not move");
    assert!(
        has_id(&mut a, "online_lobby.reset_panels"),
        "RESET is on once a panel moved"
    );
    native_shot(&mut a, &out, "l15_03_moved_2160p");
    // Resized: the chat's right edge in by 60, its bottom up by 40; the
    // right panel's left edge out by 60 and its bottom down by 30.
    click_id(&mut a, "online_lobby.reset_panels");
    drag(&mut a, Point::new(397, 300), Point::new(337, 300));
    drag(&mut a, Point::new(200, 502), Point::new(200, 462));
    drag(
        &mut a,
        Point::new(RIGHT_EDGE, 520),
        Point::new(RIGHT_EDGE - 60, 520),
    );
    drag(
        &mut a,
        RIGHT_CORNER,
        Point::new(RIGHT_CORNER.x, RIGHT_CORNER.y + 30),
    );
    shot(&mut a, &out, "l15_04_resized");
    click_id(&mut a, "online_lobby.reset_panels");
    move_panels(&mut a);
    click_id(&mut a, "online_lobby.lock_panels");
    let locked = shot(&mut a, &out, "l15_05_locked");
    // Locked: a drag on a title strip moves nothing.
    let strip = Point::new(CHAT_TITLE.x + 24, CHAT_TITLE.y - 40);
    drag(&mut a, strip, Point::new(300, 200));
    let after = shot(&mut a, &out, "l15_06_locked_after_drag");
    assert_eq!(after, locked, "a locked panel moved");
    click_id(&mut a, "online_lobby.lock_panels");
    click_id(&mut a, "online_lobby.reset_panels");
    let reset = shot(&mut a, &out, "l15_07_reset");
    assert!(reset == classic, "RESET is not the classic lobby");

    let mut b = lobby_on_install(Cover::Scene(r"CharSelect\charselectbckg"));
    move_panels(&mut b);
    shot(&mut b, &out, "l15_08_moved_scene_cover");
}

/// The market window over the lobby (plan L18) on the fake market with
/// the install's palette, fonts and item tips: MARKET's tip on hover, the
/// window on Browse, a listing picked, My shop and Deliveries with the
/// stash line, and the lobby as it was after Esc.
#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn lobby_market_on_the_install() {
    use d2_client::ui::front_end::screens::online::lobby::Lobby;
    use d2_client::ui::front_end::screens::online::market_host;
    let out = shots_dir();
    let game_dir = PathBuf::from(std::env::var("D2_GAME_DIR").expect("D2_GAME_DIR"));
    let GameData::Live(live) = GameData::select(Some(&game_dir)).expect("live data");
    let mut parts = market_host::parts_of(&live);
    assert!(parts.palette.is_some() && parts.fonts.is_some() && parts.tips.is_some());
    parts.fake = true;
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    let chat = d2_client::chat::FakeChat::seeded();
    let (mut a, _server, handle) = front_on_install_with(fake, Some(Box::new(chat)));
    handle.borrow_mut().market_parts = Some(std::rc::Rc::new(move || parts.clone()));
    host(&mut a).front.trigger(Trigger::Online);
    ticks(&mut a, 10);
    typed(&mut a, "nugwa");
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(13));
    ticks(&mut a, 8);
    click_id(&mut a, "online_chars.char(Werwer)");
    click_id(&mut a, "online_chars.ok");
    ticks(&mut a, 6);
    // The tip under MARKET while the mouse is on it ([`shot`] would move
    // the pointer away).
    // MARKET at its 16:9 place.
    let m = Lobby::market_rect();
    let on = Point::new(m.x + WIDE_RIGHT + 20, m.y + 10);
    host(&mut a).front.input(FrontInput::Move(on));
    ticks(&mut a, 2);
    capture(&mut a, &out, "l18_01_market_tip");
    click_id(&mut a, "online_lobby.market");
    ticks(&mut a, 4);
    shot(&mut a, &out, "l18_02_browse");
    click_id(&mut a, "market.result(0)");
    ticks(&mut a, 4);
    shot(&mut a, &out, "l18_03_listing");
    click_id(&mut a, "market.tab(MyShop)");
    ticks(&mut a, 4);
    shot(&mut a, &out, "l18_04_my_shop");
    click_id(&mut a, "market.tab(Deliveries)");
    ticks(&mut a, 4);
    shot(&mut a, &out, "l18_05_deliveries");
    input(&mut a, FrontInput::Key(27));
    ticks(&mut a, 4);
    shot(&mut a, &out, "l18_06_closed");
}

/// Where the native UI pass puts the 800 × 600 lobby in a `window_w ×
/// window_h` window (its shape kept, `ScaleMode::Fit`).
fn window_place(window_w: u32, window_h: u32) -> d2_client::gpu_compositor::ui_pass::UiPlacement {
    let p = d2_client::ui::frame::Presentation::with_mode(
        window_w,
        window_h,
        800,
        600,
        d2_client::app::config::ScaleMode::Fit,
    )
    .expect("a window larger than the frame");
    let (out_w, out_h) = p.image_size();
    d2_client::gpu_compositor::ui_pass::UiPlacement {
        frame_w: 800,
        frame_h: 600,
        out_w,
        out_h,
    }
}

/// The frame through the native UI pass as a `window_w × window_h` window
/// presents the 800 × 600 lobby.
fn window_shot(a: &mut App, out: &Path, name: &str, window_w: u32, window_h: u32) {
    let place = window_place(window_w, window_h);
    let (w, h) = (place.out_w, place.out_h);
    host(a).front.input(FrontInput::Move(Point::new(799, 599)));
    ticks(a, 7);
    let rgba = host(a).native_rgba(place).expect("native frame");
    let path = out.join(format!("{name}.png"));
    let file = std::fs::File::create(&path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    println!("wrote {}", path.display());
}

/// The polish windows: 1280 × 720, 1920 × 1080 and 3840 × 2160.
const POLISH_WINDOWS: [(u32, u32); 3] = [(1280, 720), (1920, 1080), (3840, 2160)];

/// The lobby's texts that leave their room or run into another
/// (`text_fit`), measured with the install's fonts, in each polish window.
fn clashes_on_install(a: &mut App, view: &str) -> Vec<String> {
    use d2_client::ui::front_end::screens::online::text_fit;
    let mut h = host(a);
    let h = &mut *h;
    let rooms: Vec<_> = h
        .front
        .ui_elements()
        .into_iter()
        .filter(|e| e.id.starts_with("online_lobby.text_room("))
        .map(|e| e.rect)
        .collect();
    let art = std::cell::RefCell::new(h.art.as_mut().expect("the install's art"));
    let adv = |font: u16, text: &[u16]| art.borrow_mut().text_width(font, text);
    let mut items = h.front.draw();
    let over = h.front.overlay(&adv);
    let at = items
        .iter()
        .position(|d| matches!(d, DrawItem::Dim { .. }))
        .unwrap_or(items.len());
    items.splice(at..at, over);
    let texts = text_fit::placed(&items, &rooms, (800, 600), &adv);
    POLISH_WINDOWS
        .iter()
        .flat_map(|&(w, h)| text_fit::clashes(&texts, window_place(w, h)))
        .map(|c| format!("{view}: {c}"))
        .collect()
}

/// Whether the frame shows the text `t` whole, measured with the
/// install's fonts.
fn shows_on_install(a: &mut App, t: &str) -> bool {
    let mut h = host(a);
    let h = &mut *h;
    let art = std::cell::RefCell::new(h.art.as_mut().expect("the install's art"));
    let mut items = h.front.draw();
    items.extend(
        h.front
            .overlay(&|font, text| art.borrow_mut().text_width(font, text)),
    );
    items
        .iter()
        .any(|i| matches!(i, DrawItem::Text { text, .. } if text == t))
}

/// The brightest channel of the frame's pixels in `r` (an RGBA 800 × 600
/// frame).
fn peak(rgba: &[u8], r: d2_client::ui::geom::Rect) -> u8 {
    // A capture at the canvas's size: 600 rows (800 or, 16:9, 1068 wide).
    let width = rgba.len() / 4 / 600;
    let mut top = 0;
    for y in r.y.max(0)..(r.y + i32::from(r.h)).min(600) {
        for x in r.x.max(0)..(r.x + i32::from(r.w)).min(width as i32) {
            let i = (y as usize * width + x as usize) * 4;
            top = top.max(rgba[i]).max(rgba[i + 1]).max(rgba[i + 2]);
        }
    }
    top
}

/// A greyed game's name is legible: its brightest ink well above the
/// faint grey it was drawn in (80), yet below a joinable row's (196).
const GREY_ROW_PEAK_MIN: u8 = 120;

fn element_rect(a: &mut App, id: &str) -> d2_client::ui::geom::Rect {
    host(a)
        .front
        .ui_elements()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no {id}"))
        .rect
}

/// The lobby of the polish captures: Werwer, a non-ladder character of
/// guild WG, on `lobby_v2_on_the_install`'s games and servers; the chat
/// with unread lines in Global, Guild and DM, its own joined / left lines
/// repeated by reconnects, another player leaving and coming back at once,
/// and players with long names in the user list.
fn polish_lobby() -> App {
    use d2_client::chat::fake::{player, NOW_MS};
    use d2_client::chat::Room;
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    let nodes = vec![
        node("eu-1", "EU N1", "Helsinki, Finland", 30, NodeState::Open),
        node("eu-2", "EU N2", "Frankfurt, Germany", 60, NodeState::Open),
    ];
    let pings: PingTable = [
        ("eu-1", Some(18)),
        ("eu-2", Some(31)),
        (online::REALM_PING, Some(22)),
    ]
    .into_iter()
    .map(|(n, p)| (n.to_string(), p))
    .collect();
    fake.set_nodes(nodes, pings);
    let on = |g: GameInfo, server: &str| GameInfo {
        server: server.into(),
        ..g
    };
    let mut baal = on(game("baal runs", 1, 5, false), "EU N1");
    baal.description = "fast runs, no leech".into();
    baal.level_min = 30;
    baal.level_max = 60;
    let mut chill = on(game("chill trading", 0, 2, true), "Rhaego");
    chill.player_hosted = true;
    chill.host_rtt_ms = 42;
    let mut hc = on(game("hc trades", 0, 3, false), "EU N2");
    hc.hardcore = true;
    let mut ubers = on(game("ubers", 1, 4, false), "EU N1");
    ubers.level_min = 80;
    for (g, pw) in [
        (baal, ""),
        (on(game("cows", 0, 3, false), "EU N2"), ""),
        (chill, "moo"),
        (hc, ""),
        (on(game("hell rush", 2, 1, false), "EU N1"), ""),
        (on(game("packed", 1, 8, false), "EU N2"), ""),
        (ubers, ""),
    ] {
        fake.add_game(g, pw);
    }
    let mut chat = d2_client::chat::FakeChat::seeded();
    let me = chat.me.clone();
    let nugwa = player("Nugwa", 2, 77, None);
    for i in 0..3 {
        chat.fresh_presence(Room::Mod, &me, false, NOW_MS + 10 + 4 * i);
        chat.fresh_presence(Room::Mod, &me, true, NOW_MS + 12 + 4 * i);
    }
    chat.fresh_presence(Room::Mod, &nugwa, false, NOW_MS + 30);
    chat.fresh_presence(Room::Mod, &nugwa, true, NOW_MS + 31);
    chat.fresh_say(
        Room::Global,
        &player("Ironwolf", 4, 52, None),
        "trading ber for jah",
    );
    chat.fresh_say(Room::Global, &player("Gheed", 6, 52, None), "wtb shako");
    chat.add_user(0, player("Andarielslayer", 1, 88, None));
    chat.add_user(1, player("Archbishopthorn", 2, 99, Some("WG")));
    let (mut a, _server, _handle) = front_on_install_with(fake, Some(Box::new(chat)));
    host(&mut a).front.trigger(Trigger::Online);
    ticks(&mut a, 10);
    typed(&mut a, "nugwa");
    input(&mut a, FrontInput::Key(9));
    typed(&mut a, "password1");
    input(&mut a, FrontInput::Key(13));
    ticks(&mut a, 8);
    click_id(&mut a, "online_chars.char(Werwer)");
    click_id(&mut a, "online_chars.ok");
    ticks(&mut a, 6);
    a
}

/// The lobby polish (before and after, the same fake data and sizes):
/// the games with greyed rows and the character block; a greyed game
/// selected; the form; the ladder; friends and guild; the DM tab; the
/// panels overlapping (chat on top, then the right panel on top); a panel
/// over MARKET with the pointer there; and the games, the ladder and the
/// overlap as 1280 × 720, 1920 × 1080 and 3840 × 2160 windows show them.
/// Then: every text of each view inside its room in those windows, with
/// the game's fonts, and a greyed row's name legible.
#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn lobby_polish_on_the_install() {
    let out = shots_dir();
    let mut a = polish_lobby();
    let mut clashes = Vec::new();
    let games = shot(&mut a, &out, "p01_games");
    clashes.extend(clashes_on_install(&mut a, "games"));
    // A greyed row's name (hc trades: hardcore) against a joinable one's.
    let name = |r: d2_client::ui::geom::Rect| d2_client::ui::geom::Rect::new(r.x, r.y, 80, r.h);
    let grey = peak(
        &games,
        name(element_rect(&mut a, "online_lobby.game(hc trades)")),
    );
    let white = peak(
        &games,
        name(element_rect(&mut a, "online_lobby.game(cows)")),
    );
    println!("greyed row peak {grey}, joinable row peak {white}");
    // A name that fits beside its class and level stays whole.
    assert!(shows_on_install(&mut a, "Bishibosh"));
    window_shot(&mut a, &out, "p01_games_1280x720", 1280, 720);
    window_shot(&mut a, &out, "p01_games_1920x1080", 1920, 1080);
    window_shot(&mut a, &out, "p01_games_3840x2160", 3840, 2160);
    click_id(&mut a, "online_lobby.game(hc trades)");
    shot(&mut a, &out, "p02_grey_selected");
    clashes.extend(clashes_on_install(&mut a, "grey selected"));
    click_id(&mut a, "online_lobby.tab_create");
    click_id(&mut a, "online_lobby.field(0)");
    typed(&mut a, "baal runs 2");
    click_id(&mut a, "online_lobby.field(3)");
    typed(&mut a, "fast runs, no leech");
    shot(&mut a, &out, "p03_create");
    clashes.extend(clashes_on_install(&mut a, "create"));
    click_id(&mut a, "online_lobby.tab_ladder");
    shot(&mut a, &out, "p04_ladder");
    clashes.extend(clashes_on_install(&mut a, "ladder"));
    // The own row's name whole beside its mark.
    assert!(shows_on_install(&mut a, "Werwer (you)"));
    window_shot(&mut a, &out, "p04_ladder_3840x2160", 3840, 2160);
    click_id(&mut a, "online_lobby.tab_friends");
    shot(&mut a, &out, "p05_friends");
    clashes.extend(clashes_on_install(&mut a, "friends"));
    click_id(&mut a, "online_lobby.friends_tab(guild)");
    ticks(&mut a, 4);
    shot(&mut a, &out, "p06_guild");
    clashes.extend(clashes_on_install(&mut a, "guild"));
    click_id(&mut a, "online_lobby.tab_join");
    click_id(&mut a, "online_lobby.chat_tab(dm)");
    shot(&mut a, &out, "p07_dm_tab");
    clashes.extend(clashes_on_install(&mut a, "dm tab"));
    click_id(&mut a, "online_lobby.chat_tab(mod)");
    // The right panel dragged left over the chat (on top: taken last),
    // then the chat taken (on top).
    drag(
        &mut a,
        RIGHT_TITLE,
        Point::new(RIGHT_TITLE.x - 150, RIGHT_TITLE.y),
    );
    shot(&mut a, &out, "p08_right_over_chat");
    clashes.extend(clashes_on_install(&mut a, "right over chat"));
    window_shot(&mut a, &out, "p08_right_over_chat_1920x1080", 1920, 1080);
    drag(
        &mut a,
        CHAT_TITLE,
        Point::new(CHAT_TITLE.x - 6, CHAT_TITLE.y),
    );
    shot(&mut a, &out, "p09_chat_over_right");
    clashes.extend(clashes_on_install(&mut a, "chat over right"));
    // The chat over MARKET, the pointer on the covered part.
    click_id(&mut a, "online_lobby.reset_panels");
    drag(
        &mut a,
        CHAT_TITLE,
        Point::new(CHAT_TITLE.x + 300 + WIDE_RIGHT, CHAT_TITLE.y - 80),
    );
    let m = d2_client::ui::front_end::screens::online::lobby::Lobby::market_rect();
    host(&mut a).front.input(FrontInput::Move(Point::new(
        m.x + WIDE_RIGHT + 20,
        m.y + 10,
    )));
    ticks(&mut a, 2);
    capture(&mut a, &out, "p10_market_under_chat");
    click_id(&mut a, "online_lobby.reset_panels");
    // The panels locked (UNLOCK PANELS), then free again.
    click_id(&mut a, "online_lobby.lock_panels");
    clashes.extend(clashes_on_install(&mut a, "locked"));
    click_id(&mut a, "online_lobby.lock_panels");
    // A narrower chat: the tab strip at the chat's least width.
    drag(&mut a, Point::new(397, 300), Point::new(317, 300));
    shot(&mut a, &out, "p11_narrow_chat");
    clashes.extend(clashes_on_install(&mut a, "narrow chat"));
    assert!(clashes.is_empty(), "{clashes:#?}");
    assert!(
        (GREY_ROW_PEAK_MIN..white).contains(&grey),
        "greyed row peak {grey}, joinable {white}"
    );
}
