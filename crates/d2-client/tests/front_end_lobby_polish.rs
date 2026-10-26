//! The lobby polish on the in-memory fake server and a fake chat, no game
//! files: every text of every lobby view inside its room at the window
//! sizes the lobby is played at, and the chat's round unread badges.

use d2_client::app::front_host::pill_rows;
use d2_client::chat::fake::{player, NOW_MS};
use d2_client::chat::source::{ChatIn, LineWhat, RoomLine, Who};
use d2_client::chat::{FakeChat, Room};
use d2_client::gpu_compositor::ui_pass::UiPlacement;
use d2_client::rules::camera::FrameSize;
use d2_client::ui::front_end::screens::online::chat_pane::ChatPane;
use d2_client::ui::front_end::screens::online::fake::{character, FakeServer, SharedFake};
use d2_client::ui::front_end::screens::online::lobby::Lobby;
use d2_client::ui::front_end::screens::online::text_fit;
use d2_client::ui::front_end::screens::online::{
    self, GameInfo, NodeInfo, NodeState, Online, ServerEntry,
};
use d2_client::ui::front_end::*;
use d2_client::ui::geom::{Point, Rect};
use d2_formats::d2s::status;

/// The windows the lobby is played in, as the native UI pass places the
/// 800 x 600 frame in them (its shape kept): 1280 x 720, 1920 x 1080 and
/// 3840 x 2160.
pub const WINDOWS: [UiPlacement; 3] = [
    UiPlacement {
        frame_w: 800,
        frame_h: 600,
        out_w: 960,
        out_h: 720,
    },
    UiPlacement {
        frame_w: 800,
        frame_h: 600,
        out_w: 1440,
        out_h: 1080,
    },
    UiPlacement {
        frame_w: 800,
        frame_h: 600,
        out_w: 2880,
        out_h: 2160,
    },
];

/// Each font's advance per glyph, wider than the game's (what fits here
/// fits there): Font8 5, Font16 8, Font30 16, FontFormal12 9, Font6 4,
/// Font24 12, FontExocet10 9, FontRidiculous 6, FontExocet8 7.
fn adv(font: u16, t: &[u16]) -> i32 {
    let per = match font {
        0 => 5,
        1 => 8,
        2 => 16,
        5 => 9,
        6 => 4,
        7 => 12,
        9 => 9,
        10 => 6,
        11 => 7,
        _ => 8,
    };
    t.len() as i32 * per
}

fn game(name: &str, difficulty: u8, players: u8) -> GameInfo {
    GameInfo {
        id: 0,
        name: name.into(),
        difficulty,
        players,
        max_players: 8,
        expansion: true,
        ..Default::default()
    }
}

/// The polish captures' lobby: Werwer (non-ladder, guild WG) on games
/// with every greyed reason; the chat with unread lines in Global, Guild
/// and DM, its own reconnects, and players with long names.
fn lobby() -> FrontEnd {
    lobby_with(|_, _| {})
}

/// [`lobby`] with its server and chat set up further by `prep`.
fn lobby_with(prep: impl FnOnce(&mut FakeServer, &mut FakeChat)) -> FrontEnd {
    let mut fake = FakeServer::default();
    fake.server_name = "D2Gamma".into();
    fake.add_account(
        "Nugwa",
        "password1",
        vec![character("Werwer", 4, 34, 5, status::EXPANSION)],
    );
    let mut baal = game("baal runs", 1, 5);
    baal.description = "fast runs, no leech".into();
    baal.level_min = 30;
    baal.level_max = 60;
    let mut hc = game("hc trades", 0, 3);
    hc.hardcore = true;
    let mut ubers = game("ubers", 1, 4);
    ubers.level_min = 80;
    for (g, pw) in [
        (baal, ""),
        (game("cows", 0, 3), ""),
        (
            GameInfo {
                locked: true,
                ..game("chill trading", 0, 2)
            },
            "moo",
        ),
        (hc, ""),
        (game("hell rush", 2, 1), ""),
        (game("packed", 1, 8), ""),
        (ubers, ""),
    ] {
        fake.add_game(g, pw);
    }
    let mut chat = FakeChat::seeded();
    let me = chat.me.clone();
    for i in 0..3 {
        chat.fresh_presence(Room::Mod, &me, false, NOW_MS + 10 + 4 * i);
        chat.fresh_presence(Room::Mod, &me, true, NOW_MS + 12 + 4 * i);
    }
    chat.fresh_say(
        Room::Global,
        &player("Ironwolf", 4, 52, None),
        "trading ber for jah",
    );
    chat.add_user(0, player("Andarielslayer", 1, 88, None));
    chat.add_user(1, player("Archbishopthorn", 2, 99, Some("WG")));
    prep(&mut fake, &mut chat);
    let server = SharedFake(std::rc::Rc::new(std::cell::RefCell::new(fake)));
    let online = Online::new(
        Box::new(server),
        vec![ServerEntry {
            id: "eu1".into(),
            name: "D2Gamma".into(),
            address: "127.0.0.1:6112".into(),
        }],
    );
    let mut reg = Registry::default();
    screens::register_all(&mut reg);
    online::register_with(&mut reg, online.clone());
    let lobby = Lobby::with_chat(online, ChatPane::with_source(Box::new(chat)));
    reg.register(online::ONLINE_LOBBY, Box::new(lobby));
    let mut f = FrontEnd::new(true, Box::new(false), reg);
    f.start(false);
    f.trigger(Trigger::Online);
    ticks(&mut f, 2);
    for u in "nugwa".encode_utf16() {
        f.input(FrontInput::Char(u));
    }
    f.input(FrontInput::Key(9));
    for u in "password1".encode_utf16() {
        f.input(FrontInput::Char(u));
    }
    f.input(FrontInput::Key(13));
    ticks(&mut f, 4);
    click(&mut f, "online_chars.char(Werwer)");
    click(&mut f, "online_chars.ok");
    ticks(&mut f, 4);
    assert_eq!(f.current(), online::ONLINE_LOBBY);
    f
}

fn ticks(f: &mut FrontEnd, n: u32) {
    for _ in 0..n {
        f.tick();
    }
}

fn click(f: &mut FrontEnd, id: &str) {
    let e = f
        .ui_elements()
        .into_iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no {id} on {:?}", f.current()));
    f.input(FrontInput::Move(e.at));
    f.input(FrontInput::Down(e.at));
    f.input(FrontInput::Up(e.at));
    ticks(f, 3);
}

/// The frame's items as the host stacks them.
fn items(f: &mut FrontEnd) -> Vec<DrawItem> {
    let mut items = f.draw();
    let over = f.overlay(&adv);
    let at = items
        .iter()
        .position(|d| matches!(d, DrawItem::Dim { .. }))
        .unwrap_or(items.len());
    items.splice(at..at, over);
    items
}

/// What leaves its room at each window size.
fn clashes(f: &mut FrontEnd) -> Vec<String> {
    let rooms: Vec<Rect> = f
        .ui_elements()
        .into_iter()
        .filter(|e| e.id.starts_with("online_lobby.text_room("))
        .map(|e| e.rect)
        .collect();
    assert!(rooms.len() >= 2, "the lobby names its rooms");
    let texts = text_fit::placed(&items(f), &rooms, (800, 600), &adv);
    WINDOWS
        .iter()
        .flat_map(|w| text_fit::clashes(&texts, *w))
        .collect()
}

/// Polish: no clipped text at 1280 x 720, 1920 x 1080 and 3840 x 2160:
/// in every lobby view (the games, a grey game selected, the form with a
/// long description typed, the ladder, friends and guild, the DM tab, the
/// right panel over the chat and back under it) each text stays in its room
/// (a button between the art's ends, a label in its box, a cell in its
/// panel or part of the chat) and no two run into each other on a line.
#[test]
fn every_lobby_text_fits_its_room_at_each_window_size() {
    let mut f = lobby();
    let mut all = Vec::new();
    let mut view = |f: &mut FrontEnd, name: &str| {
        all.extend(clashes(f).into_iter().map(|c| format!("{name}: {c}")));
    };
    view(&mut f, "games");
    click(&mut f, "online_lobby.game(hc trades)");
    view(&mut f, "grey selected");
    click(&mut f, "online_lobby.tab_create");
    click(&mut f, "online_lobby.field(3)");
    for u in "fast runs, no leech".encode_utf16() {
        f.input(FrontInput::Char(u));
    }
    // A frame measures the fields, the next tick shows them.
    items(&mut f);
    ticks(&mut f, 1);
    view(&mut f, "create");
    click(&mut f, "online_lobby.tab_ladder");
    view(&mut f, "ladder");
    click(&mut f, "online_lobby.tab_friends");
    view(&mut f, "friends");
    click(&mut f, "online_lobby.friends_tab(guild)");
    ticks(&mut f, 2);
    view(&mut f, "guild");
    click(&mut f, "online_lobby.tab_join");
    click(&mut f, "online_lobby.chat_tab(dm)");
    view(&mut f, "dm tab");
    // The right panel taken over the chat.
    let (from, to) = (Point::new(700, 258), Point::new(550, 258));
    f.input(FrontInput::Move(from));
    f.input(FrontInput::Down(from));
    f.input(FrontInput::Move(to));
    f.input(FrontInput::Up(to));
    ticks(&mut f, 3);
    view(&mut f, "right over chat");
    // The chat taken back on top.
    let (from, to) = (Point::new(386, 166), Point::new(380, 166));
    f.input(FrontInput::Move(from));
    f.input(FrontInput::Down(from));
    f.input(FrontInput::Move(to));
    f.input(FrontInput::Up(to));
    ticks(&mut f, 3);
    view(&mut f, "chat over right");
    assert!(all.is_empty(), "{all:#?}");
}

/// Polish: a chat narrowed to its least width still shows every tab whole
/// (the short labels), and the tab under the pointer shows its full label.
#[test]
fn a_narrow_chat_keeps_every_tab() {
    let mut f = lobby();
    let el = |f: &FrontEnd, id: &str| {
        f.ui_elements()
            .into_iter()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("no {id}"))
    };
    // The chat's right edge dragged in as far as it goes.
    let (from, to) = (Point::new(397, 300), Point::new(317, 300));
    f.input(FrontInput::Move(from));
    f.input(FrontInput::Down(from));
    f.input(FrontInput::Move(to));
    f.input(FrontInput::Up(to));
    ticks(&mut f, 3);
    // The tabs are laid out as drawn.
    items(&mut f);
    let strip = el(&f, "online_lobby.text_room(chat_tabs)").rect;
    assert!(strip.w < 300, "{strip:?}");
    for t in ["global", "mod", "guild", "dm"] {
        let tab = el(&f, &format!("online_lobby.chat_tab({t})")).rect;
        assert!(tab.right() <= strip.right(), "{t}: {tab:?} past {strip:?}");
    }
    assert!(clashes(&mut f).is_empty(), "{:#?}", clashes(&mut f));
    let tab = el(&f, "online_lobby.chat_tab(mod)").rect;
    f.input(FrontInput::Move(Point::new(tab.x + 3, tab.y + 3)));
    ticks(&mut f, 1);
    let shown = items(&mut f);
    assert!(
        shown
            .iter()
            .any(|i| matches!(i, DrawItem::Text { text, .. } if text == "MOD: D2GAMMA")),
        "the full label as the tab's tip"
    );
}

/// Polish: an unread count rides in a round red badge (a filled capsule,
/// as the approved preview), the count in white inside it.
#[test]
fn unread_counts_ride_round_badges() {
    let mut f = lobby();
    let shown = items(&mut f);
    let pills: Vec<Rect> = shown
        .iter()
        .filter_map(|i| match i {
            DrawItem::Pill { at, w, h, .. } => Some(Rect::new(at.x, at.y, *w as u16, *h as u16)),
            _ => None,
        })
        .collect();
    // Global, Guild and DM have unread lines.
    assert!(pills.len() >= 3, "{pills:?}");
    for p in &pills {
        let count = shown.iter().find(|i| {
            matches!(i, DrawItem::Text { at, text, color: 0, .. }
                if p.contains(Point::new(at.x, at.y - 1)) && text.parse::<u16>().is_ok())
        });
        assert!(count.is_some(), "a count in white in {p:?}");
        assert!(p.w >= p.h, "{p:?}");
    }
    // The badge's rows: a disc for one digit, a capsule for more.
    let disc = pill_rows(12, 12);
    assert_eq!(disc.len(), 12);
    assert!(disc[0].0 > 2 && disc[0].2 < 8, "{disc:?}");
    assert_eq!(disc[6], (0, 6, 12));
    for (y, row) in disc.iter().enumerate() {
        assert_eq!(row.0 * 2 + row.2, 12, "row {y} centred");
        assert_eq!(
            *row,
            (disc[11 - y].0, y as i32, disc[11 - y].2),
            "symmetric"
        );
    }
    let capsule = pill_rows(20, 12);
    assert!(capsule.iter().all(|r| r.2 >= 8));
}

/// Polish (overlapping panels): a button the moved right panel covers in
/// part (RESET PANELS under its bottom edge) takes a click where it is
/// seen, not where the panel is over it.
#[test]
fn a_button_partly_under_a_panel_clicks_where_it_is_seen() {
    let mut f = lobby();
    let reset = |f: &FrontEnd| {
        f.ui_elements()
            .into_iter()
            .find(|e| e.id == "online_lobby.reset_panels")
    };
    // The right panel taken by its title down and left over the button
    // (which then takes clicks: a panel moved).
    let (from, to) = (Point::new(700, 258), Point::new(550, 268));
    f.input(FrontInput::Move(from));
    f.input(FrontInput::Down(from));
    f.input(FrontInput::Move(to));
    f.input(FrontInput::Up(to));
    ticks(&mut f, 3);
    let panel = f
        .ui_elements()
        .into_iter()
        .find(|e| e.id == "online_lobby.text_room(right)")
        .unwrap()
        .rect;
    let button = reset(&f).expect("RESET PANELS").rect;
    assert!(
        panel.x < 400 && panel.bottom() > i64::from(button.y),
        "{panel:?} {button:?}"
    );
    // Under the panel: not the button's.
    let under = Point::new(button.x + 20, button.y + 2);
    assert!(panel.contains(under));
    f.input(FrontInput::Move(under));
    f.input(FrontInput::Down(under));
    f.input(FrontInput::Up(under));
    ticks(&mut f, 3);
    let moved = f
        .ui_elements()
        .into_iter()
        .find(|e| e.id == "online_lobby.text_room(right)")
        .unwrap()
        .rect;
    assert_eq!(moved, panel, "the press under the panel reset nothing");
    // Where it is seen it resets the panels.
    let e = reset(&f).expect("RESET PANELS still clickable");
    assert!(!panel.contains(e.at), "{:?} in {panel:?}", e.at);
    click(&mut f, "online_lobby.reset_panels");
    let back = f
        .ui_elements()
        .into_iter()
        .find(|e| e.id == "online_lobby.text_room(right)")
        .unwrap()
        .rect;
    assert_ne!(back, panel, "reset");
}

/// The windows the 16:9 lobby is played in: its 1068 x 600 canvas filling
/// 1280 x 720, 1920 x 1080 and 3840 x 2160.
const WIDE_WINDOWS: [(u32, u32); 3] = [(1280, 720), (1920, 1080), (3840, 2160)];

/// A server upgrade's countdown line, as the main server says it in Global
/// (the chat shows it as a realm notice, wrapped in the narrow column).
const COUNTDOWN: &str = "Restarting for an update in 3 minutes";

fn wide_node(id: &str, name: &str) -> NodeInfo {
    NodeInfo {
        node: id.into(),
        name: name.into(),
        location: name.into(),
        ping_addr: "127.0.0.1:9".into(),
        cert_sha256: [1; 32],
        load_pct: 10,
        state: NodeState::Open,
        suggested: id == "hel-1",
    }
}

fn shows(f: &mut FrontEnd, s: &str) -> Option<usize> {
    items(f)
        .iter()
        .position(|i| matches!(i, DrawItem::Text { text, .. } if text.contains(s)))
}

/// Polish at 16:9 (the lobby over its picture): every text stays
/// in its room on the wide canvas in the games, the form and with the right
/// panel over the chat; a server upgrade's countdown line in Global shows
/// in the chat; the form's SERVER list opens inside the right panel at the
/// right edge and is drawn over everything; the background picker opens
/// and Esc closes it.
#[test]
fn the_wide_lobby_keeps_the_polish_the_server_list_and_the_countdown() {
    let mut f = lobby_with(|fake, chat| {
        fake.nodes = vec![wide_node("hel-1", "Finland"), wide_node("fra-1", "Germany")];
        chat.fresh.push(ChatIn::Line(RoomLine {
            room: Room::Global,
            line: 900,
            what: LineWhat::Notice,
            who: Who::default(),
            text: COUNTDOWN.into(),
            at_ms: NOW_MS + 20,
        }));
    });
    f.set_canvas(FrameSize::WIDE);
    ticks(&mut f, 2);
    assert!(f.wide() && f.canvas().full, "{:?}", f.canvas());
    let wide_clashes = |f: &mut FrontEnd| -> Vec<String> {
        let rooms: Vec<Rect> = f
            .ui_elements()
            .into_iter()
            .filter(|e| e.id.starts_with("online_lobby.text_room("))
            .map(|e| e.rect)
            .collect();
        let size = (FrameSize::WIDE.width as u16, 600);
        let texts = text_fit::placed(&items(f), &rooms, size, &adv);
        WIDE_WINDOWS
            .iter()
            .flat_map(|&(out_w, out_h)| {
                let place = UiPlacement {
                    frame_w: u32::from(size.0),
                    frame_h: u32::from(size.1),
                    out_w,
                    out_h,
                };
                text_fit::clashes(&texts, place)
            })
            .collect()
    };
    let mut all = Vec::new();
    all.extend(
        wide_clashes(&mut f)
            .into_iter()
            .map(|c| format!("games: {c}")),
    );
    // Global has it (its unread count), shown on its tab.
    click(&mut f, "online_lobby.chat_tab(global)");
    assert!(
        shows(&mut f, "[Realm] Restarting for an").is_some(),
        "the countdown in Global"
    );

    // The background picker: open, its rows; Esc closes it.
    click(&mut f, "online_lobby.background");
    let ids: Vec<String> = f.ui_elements().into_iter().map(|e| e.id).collect();
    assert!(
        ids.contains(&"online_lobby.background(tristram)".to_string()),
        "{ids:?}"
    );
    f.input(FrontInput::Key(27));
    ticks(&mut f, 2);
    assert!(f
        .ui_elements()
        .iter()
        .all(|e| !e.id.starts_with("online_lobby.background(")));

    // The form's SERVER list, in the right panel at the right edge.
    click(&mut f, "online_lobby.tab_create");
    all.extend(
        wide_clashes(&mut f)
            .into_iter()
            .map(|c| format!("create: {c}")),
    );
    click(&mut f, "online_lobby.server");
    let entry = f
        .ui_elements()
        .into_iter()
        .find(|e| e.id == "online_lobby.server(1)")
        .expect("the list's entries");
    let panel = f
        .ui_elements()
        .into_iter()
        .find(|e| e.id == "online_lobby.text_room(right)")
        .expect("the right panel's room");
    assert!(panel.rect.contains(entry.at), "{entry:?} in {panel:?}");
    assert!(entry.at.x > 800, "at the right edge: {entry:?}");
    let germany = shows(&mut f, "Germany").expect("the list's names");
    let chat = shows(&mut f, "[Realm] Restarting for an").expect("the chat under it");
    assert!(germany > chat, "the open list over everything");
    click(&mut f, "online_lobby.server.close");
    click(&mut f, "online_lobby.tab_join");

    // The right panel taken over the chat.
    let right = panel.rect;
    let (from, to) = (
        Point::new(right.x + 160, right.y + 8),
        Point::new(right.x - 400, right.y + 8),
    );
    f.input(FrontInput::Move(from));
    f.input(FrontInput::Down(from));
    f.input(FrontInput::Move(to));
    f.input(FrontInput::Up(to));
    ticks(&mut f, 3);
    all.extend(
        wide_clashes(&mut f)
            .into_iter()
            .map(|c| format!("right over chat: {c}")),
    );
    assert!(all.is_empty(), "{all:#?}");
}
