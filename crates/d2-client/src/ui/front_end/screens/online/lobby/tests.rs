use d2_formats::d2s::status;

use super::*;
use crate::ui::front_end::screens::online::fake::{character, FakeServer};
use crate::ui::front_end::screens::online::{NodeInfo, NodeState, Online, PingTable, REALM_PING};

/// Every glyph 8 px wide (Font16), 5 px in the small font: wider than the
/// game's fonts, so what fits here fits there.
fn adv(font: u16, t: &[u16]) -> i32 {
    let per = if font == FONT8 { 5 } else { 8 };
    t.len() as i32 * per
}

fn w8(s: &str) -> i32 {
    adv(FONT8, &s.encode_utf16().collect::<Vec<u16>>())
}

fn game(name: &str) -> GameInfo {
    GameInfo {
        id: 1,
        name: name.into(),
        players: 1,
        max_players: 8,
        expansion: true,
        ..Default::default()
    }
}

fn softcore() -> CharInfo {
    character("Werwer", 4, 34, 5, status::EXPANSION)
}

fn lobby_with(games: Vec<GameInfo>, nodes: Vec<NodeInfo>, pings: PingTable) -> Lobby {
    let online = Online::new(Box::<FakeServer>::default(), Vec::new());
    {
        let mut o = online.borrow_mut();
        o.state.selected = Some(softcore());
        o.state.games = games;
        o.state.nodes = nodes;
        o.state.pings = pings;
    }
    Lobby::new(online)
}

fn node(id: &str, name: &str) -> NodeInfo {
    NodeInfo {
        node: id.into(),
        name: name.into(),
        location: "Helsinki".into(),
        ping_addr: "127.0.0.1:9".into(),
        cert_sha256: [1; 32],
        load_pct: 10,
        state: NodeState::Open,
        suggested: false,
    }
}

/// Rule (lobby contract section 7): the grey text of each refusal, the
/// kind from the game's flags.
#[test]
fn grey_reasons_name_the_rule() {
    let c = softcore();
    let with = |joinable: ErrorCode, f: &dyn Fn(&mut GameInfo)| {
        let mut g = game("g");
        g.joinable = joinable.0;
        f(&mut g);
        grey_reason(&g, Some(&c))
    };
    assert_eq!(with(ErrorCode(0), &|_| {}), "");
    assert_eq!(with(ErrorCode::DEAD_HARDCORE, &|_| {}), "dead character");
    assert_eq!(
        with(ErrorCode::WRONG_GAME_KIND, &|g| g.hardcore = true),
        "hardcore game"
    );
    assert_eq!(
        with(ErrorCode::WRONG_GAME_KIND, &|g| g.expansion = false),
        "classic game"
    );
    let hc = character("Hc", 4, 34, 5, status::HARDCORE);
    let mut g = game("g");
    g.joinable = ErrorCode::WRONG_GAME_KIND.0;
    assert_eq!(grey_reason(&g, Some(&hc)), "softcore game");
    g.hardcore = true;
    assert_eq!(grey_reason(&g, Some(&hc)), "expansion game");
    assert_eq!(
        with(ErrorCode::DIFFICULTY_LOCKED, &|g| g.difficulty = 2),
        "Hell not unlocked"
    );
    assert_eq!(
        with(lobby_code::LEVEL_RANGE, &|g| {
            g.level_min = 40;
            g.level_max = 70;
        }),
        "levels 40-70"
    );
    assert_eq!(
        with(lobby_code::LEVEL_RANGE, &|g| g.level_min = 80),
        "levels 80+"
    );
    assert_eq!(with(ErrorCode::GAME_FULL, &|_| {}), "full");
}

/// The selected grey game's line says the reason as a sentence.
#[test]
fn the_selected_grey_game_says_why() {
    let c = softcore();
    let st = OnlineState::default();
    let mut g = game("baal");
    g.difficulty = 2;
    g.joinable = ErrorCode::DIFFICULTY_LOCKED.0;
    assert_eq!(
        selected_line(&st, &g, Some(&c)),
        ("baal: Hell is not unlocked yet.".to_string(), RED)
    );
    g.joinable = ErrorCode::WRONG_GAME_KIND.0;
    g.hardcore = true;
    assert_eq!(
        selected_line(&st, &g, Some(&c)).0,
        "baal: For hardcore characters."
    );
    g.joinable = lobby_code::LEVEL_RANGE.0;
    g.level_min = 40;
    g.level_max = 70;
    assert_eq!(
        selected_line(&st, &g, Some(&c)).0,
        "baal: For levels 40-70 only."
    );
}

/// The legend under the list names each reason once, in list order.
#[test]
fn the_grey_legend_names_each_reason_once() {
    let c = softcore();
    let mut full = game("full house");
    full.joinable = ErrorCode::GAME_FULL.0;
    let mut hc = game("hc trades");
    hc.hardcore = true;
    hc.joinable = ErrorCode::WRONG_GAME_KIND.0;
    let games = vec![game("cows"), full.clone(), hc, full];
    assert_eq!(
        grey_legend(&games, Some(&c)).as_deref(),
        Some("Grey: full / hardcore game")
    );
    assert_eq!(grey_legend(&[game("cows")], Some(&c)), None);
}

/// Rule (plan L10): the server column holds the longest server label and
/// the longest host name: the label is cut, the ping stays whole, and the
/// cell never passes the list's right edge.
#[test]
fn server_cells_fit_the_column_at_the_longest_label() {
    let long = "S".repeat(32);
    let mut pings = PingTable::new();
    pings.insert("n1".into(), Some(999));
    pings.insert(REALM_PING.into(), Some(500));
    let lobby = lobby_with(Vec::new(), vec![node("n1", &long)], pings);
    let st = lobby.online.borrow().state.clone();
    let room = LIST_RIGHT - COL_SERVER;
    let mut official = game("g");
    official.server = long.clone();
    let cell = server_cell(&st, &official, room, &w8);
    assert!(cell.ends_with("999 ms"), "{cell}");
    assert!(w8(&cell) <= room, "{cell}");
    let mut hosted = game("h");
    hosted.player_hosted = true;
    hosted.server = "H".repeat(32);
    hosted.host_rtt_ms = 499;
    let cell = server_cell(&st, &hosted, room, &w8);
    assert!(cell.ends_with("~999 ms"), "{cell}");
    assert!(w8(HOST_MARK) + w8(" ") + w8(&cell) <= room, "{cell}");
    // A short label stays whole.
    let mut eu = game("eu");
    eu.server = "EU N1".into();
    let mut p = PingTable::new();
    p.insert("n1".into(), Some(18));
    let lobby = lobby_with(Vec::new(), vec![node("n1", "EU N1")], p);
    let st = lobby.online.borrow().state.clone();
    assert_eq!(server_cell(&st, &eu, room, &w8), "EU N1 18 ms");
}

/// The host's ping is an estimate (the realm round trip plus the host's)
/// and unknown without either.
#[test]
fn a_player_hosts_ping_is_an_estimate() {
    let mut g = game("h");
    g.player_hosted = true;
    g.host_rtt_ms = 42;
    let mut st = OnlineState::default();
    assert_eq!(st.host_ping(&g), None);
    st.pings.insert(REALM_PING.into(), Some(22));
    assert_eq!(st.host_ping(&g), Some(64));
    g.host_rtt_ms = 0;
    assert_eq!(st.host_ping(&g), None);
}

/// Rule (POLISH: the PW marker was cut off at the right edge): every cell
/// of every row, the `PW` marker after the longest name included, stays
/// inside the list.
#[test]
fn every_row_stays_inside_the_list() {
    let mut pings = PingTable::new();
    pings.insert("n1".into(), Some(18));
    let mut long = game("WWWWWWWWWWWWWWW");
    long.locked = true;
    long.server = "S".repeat(32);
    let mut hosted = game("host game");
    hosted.id = 2;
    hosted.player_hosted = true;
    hosted.server = "H".repeat(15);
    hosted.locked = true;
    let lobby = lobby_with(vec![long, hosted], vec![node("n1", &"S".repeat(32))], pings);
    let items = lobby.list_overlay(&adv);
    let mut saw_pw = 0;
    for it in &items {
        let DrawItem::Text { text, font, at, .. } = it else {
            continue;
        };
        let w = adv(*font, &text.encode_utf16().collect::<Vec<u16>>());
        assert!(at.x >= PANEL.x, "{text} at {at:?}");
        assert!(
            at.x + w <= PANEL.x + PANEL.w as i32,
            "{text} at {at:?} ends at {}",
            at.x + w
        );
        if text.trim() == "PW" {
            saw_pw += 1;
            assert!(
                at.x + w <= COL_DIFF,
                "PW at {at:?} runs into the next column"
            );
        }
    }
    assert_eq!(saw_pw, 2, "{items:?}");
}

/// Rule (plan L10): the form's level range: blank is none, min ≤ max,
/// levels 1-99, the character's own level inside.
#[test]
fn the_level_range_rules() {
    assert_eq!(parse_levels("", 34), Ok((0, 0)));
    assert_eq!(parse_levels("30-40", 34), Ok((30, 40)));
    assert_eq!(parse_levels("30-", 34), Ok((30, 0)));
    assert_eq!(parse_levels("30", 34), Ok((30, 0)));
    assert_eq!(parse_levels("-40", 34), Ok((0, 40)));
    assert_eq!(parse_levels(" 34-34 ", 34), Ok((34, 34)));
    let err = |t: &str| parse_levels(t, 34).unwrap_err();
    assert!(err("40-30").contains("must not be above"));
    assert!(err("40-70").contains("Your level (34)"));
    assert!(err("1-20").contains("Your level (34)"));
    assert!(err("0-50").contains("1 to 99"));
    assert!(err("10-100").contains("1 to 99"));
    assert!(parse_levels("-", 34).is_err());
    assert!(parse_levels("1-2-3", 34).is_err());
}

/// Rule (POLISH: "Blank: a public game." overlapped the password box): no
/// text of the create form overlaps one of its boxes, and all of it stays
/// inside the panel.
#[test]
fn no_form_text_overlaps_a_box() {
    let mut lobby = lobby_with(Vec::new(), Vec::new(), PingTable::new());
    let mut v = Vec::new();
    lobby.form_controls(&mut v);
    let boxes: Vec<Rect> = v
        .iter()
        .filter(|c| c.art == Some(TEXTBOX))
        .map(|c| c.hit_box())
        .collect();
    assert_eq!(boxes.len(), 4);
    let overlaps = |a: &Rect, b: &Rect| {
        i64::from(a.x) < b.right()
            && i64::from(b.x) < a.right()
            && i64::from(a.y) < b.bottom()
            && i64::from(b.y) < a.bottom()
    };
    for t in v
        .iter()
        .filter(|c| c.kind == ControlKind::Text && c.art.is_none())
    {
        let Some(text) = &t.text else { continue };
        if text.is_empty() {
            continue; // a field's own text
        }
        let r = t.hit_box();
        for b in &boxes {
            assert!(
                !overlaps(&r, b),
                "{text:?} at {r:?} overlaps a box at {b:?}"
            );
        }
        assert!(
            r.right() <= PANEL.right() && r.bottom() <= PANEL.bottom(),
            "{text:?} at {r:?} leaves the panel"
        );
    }
}

/// The character line: the guild's tag and the ladder rank, or what of it
/// is known.
#[test]
fn the_standing_line() {
    assert_eq!(
        standing_line(Some("NUG"), Some(37), 1).as_deref(),
        Some("Guild <NUG> - Ladder rank #37")
    );
    // A non-ladder character: no rank line, its guild alone.
    assert_eq!(standing_line(None, Some(5), 0), None);
    assert_eq!(
        standing_line(Some("WG"), Some(1), 0).as_deref(),
        Some("Guild <WG>")
    );
    assert_eq!(
        standing_line(Some("NUG"), None, 1).as_deref(),
        Some("Guild <NUG>")
    );
    assert_eq!(standing_line(None, None, 1), None);
}

/// The rank comes from the character's own view: its season and kind,
/// every class, the page around it.
#[test]
fn the_rank_query_is_the_characters_own_view() {
    let mut c = character("Hc", 4, 34, 5, status::HARDCORE | status::EXPANSION);
    c.season = 3;
    let q = rank_query(&c);
    assert_eq!(q.season, 3);
    assert_eq!(q.split, 3);
    assert_eq!(q.class, 255);
    assert_eq!(q.at, LadderAt::Me);
}

/// The text cut keeps what fits and says it was cut.
#[test]
fn fit_cuts_with_dots() {
    let w = |s: &str| s.len() as i32 * 8;
    assert_eq!(fit("cows", 40, &w), "cows");
    assert_eq!(fit("nm chaos runs", 64, &w), "nm ch...");
    assert_eq!(fit("abc", 16, &w), "");
}

/// Rule (plan L10): the lines under the list: the selected game wraps onto
/// two lines at most, the legend takes what is left, the last line cut.
#[test]
fn the_lines_under_the_list_wrap_and_cut() {
    let w = |s: &str| s.len() as i32 * 8;
    let mut lines = Vec::new();
    push_wrapped(&mut lines, "aaaa bbbb cccc dddd", WHITE, 2, 80, &w);
    assert_eq!(
        lines,
        [("aaaa bbbb".into(), WHITE), ("cccc dddd".into(), WHITE)]
    );
    push_wrapped(&mut lines, "Grey: full / hardcore game", GREY, 1, 80, &w);
    assert_eq!(lines[2], ("Grey: f...".into(), GREY));
    push_wrapped(&mut lines, "more", GREY, 0, 80, &w);
    assert_eq!(lines.len(), 3);
    let mut long = Vec::new();
    push_wrapped(&mut long, "aaaa bbbb cccc dddd eeee", WHITE, 2, 80, &w);
    assert_eq!(long[1].0, "cccc dd...");
}

fn build(lobby: &mut Lobby) -> Vec<Control> {
    let mut flow = crate::ui::front_end::FlowCtx::default();
    let mut ctx = FrontCtx {
        expansion: true,
        flow: &mut flow,
        now_ms: 0,
    };
    lobby.build(&mut ctx)
}

fn act_on(lobby: &mut Lobby, id: u32) {
    let mut flow = crate::ui::front_end::FlowCtx::default();
    let mut ctx = FrontCtx {
        expansion: true,
        flow: &mut flow,
        now_ms: 0,
    };
    lobby.action(&mut ctx, id);
}

fn press(lobby: &mut Lobby, p: Point, now_ms: u64) {
    let mut flow = crate::ui::front_end::FlowCtx::default();
    let mut ctx = FrontCtx {
        expansion: true,
        flow: &mut flow,
        now_ms,
    };
    lobby.mouse(&mut ctx, true, p);
    lobby.mouse(&mut ctx, false, p);
}

/// Drags a panel by its title strip from `from` by (`dx`, `dy`).
fn drag_panel(lobby: &mut Lobby, from: Point, (dx, dy): (i32, i32)) {
    let mut flow = crate::ui::front_end::FlowCtx::default();
    let mut ctx = FrontCtx {
        expansion: true,
        flow: &mut flow,
        now_ms: 0,
    };
    let to = Point::new(from.x + dx, from.y + dy);
    lobby.mouse(&mut ctx, true, from);
    lobby.pointer(&mut ctx, to);
    lobby.mouse(&mut ctx, false, to);
    assert!(lobby.layout.take_moved(), "nothing moved from {from:?}");
    lobby.rebuild = true;
}

fn find<'a>(v: &'a [Control], text: &str) -> &'a Control {
    v.iter()
        .find(|c| c.text.as_deref() == Some(text))
        .unwrap_or_else(|| panic!("no {text}"))
}

/// Rule (plan L15): the classic layout is today's lobby (the buttons and
/// the list where they always were), and RESET builds exactly it again.
#[test]
fn reset_builds_exactly_the_classic_lobby() {
    let mut lobby = lobby_with(Vec::new(), Vec::new(), PingTable::new());
    build(&mut lobby);
    lobby.online.borrow_mut().state.games = vec![game("a")];
    lobby.rebuild = true;
    let classic = build(&mut lobby);
    let games = find(&classic, "GAMES");
    assert_eq!(
        (games.x, games.y),
        (BUTTONS_X + i32::from(BUTTON_W), BUTTONS_Y)
    );
    assert!(!find(&classic, "RESET PANELS").enabled);

    drag_panel(&mut lobby, Point::new(300, 165), (40, 20));
    drag_panel(&mut lobby, Point::new(600, 258), (-120, -30));
    let moved = build(&mut lobby);
    assert_ne!(moved, classic);
    let (dx, dy) = lobby.layout.right_offset();
    assert!(dx != 0 && dy != 0);
    let g = find(&moved, "GAMES");
    assert_eq!((g.x, g.y), (games.x + dx, games.y + dy));
    assert!(find(&moved, "RESET PANELS").enabled);

    act_on(&mut lobby, act::RESET);
    assert!(lobby.layout.is_classic());
    assert_eq!(build(&mut lobby), classic);
    assert!(lobby.underlay().is_empty());
}

/// A moved right panel takes its clicks where it is now: the list's rows
/// and its outline move with it.
#[test]
fn a_moved_list_takes_clicks_where_it_is() {
    let mut lobby = lobby_with(Vec::new(), Vec::new(), PingTable::new());
    build(&mut lobby);
    lobby.online.borrow_mut().state.games = vec![game("a")];
    drag_panel(&mut lobby, Point::new(600, 258), (-150, -40));
    build(&mut lobby);
    let (dx, dy) = lobby.layout.right_offset();
    let row = Point::new(PANEL.x + 20, Lobby::row_top(0) + 5);
    press(&mut lobby, row, 0);
    assert_eq!(lobby.selected, None, "the list moved away");
    press(&mut lobby, Point::new(row.x + dx, row.y + dy), 1000);
    assert_eq!(lobby.selected, Some(1));
    let el = lobby
        .ui_elements()
        .into_iter()
        .find(|e| e.id == "online_lobby.game(a)")
        .unwrap();
    assert_eq!(
        (el.rect.x, el.rect.y),
        (PANEL.x + dx, Lobby::row_top(0) + dy)
    );
}

/// A taller right panel lists more games; a wider one widens the names.
#[test]
fn a_grown_list_shows_more_rows() {
    let games: Vec<GameInfo> = (1..=12)
        .map(|i| {
            let mut g = game(&format!("g{i}"));
            g.id = i;
            g
        })
        .collect();
    let mut lobby = lobby_with(Vec::new(), Vec::new(), PingTable::new());
    build(&mut lobby);
    lobby.online.borrow_mut().state.games = games;
    assert_eq!(lobby.rows(), ROWS);
    drag_panel(&mut lobby, Point::new(600, 258), (-150, -100));
    let r = lobby.layout.right();
    let corner = Point::new(r.x + i32::from(r.w) - 2, r.y + i32::from(r.h) - 2);
    drag_panel(&mut lobby, corner, (60, 2 * ROW_H + 4));
    build(&mut lobby);
    assert!(lobby.rows() >= ROWS + 2, "{:?}", lobby.layout.right_grow());
    let shown = lobby
        .ui_elements()
        .iter()
        .filter(|e| e.id.starts_with("online_lobby.game("))
        .count();
    assert_eq!(shown, lobby.rows());
    // Every cell stays inside the grown panel.
    let area = lobby.area();
    for it in lobby.list_overlay(&adv) {
        if let DrawItem::Text { text, font, at, .. } = it {
            let w = adv(font, &text.encode_utf16().collect::<Vec<u16>>());
            assert!(at.x >= area.x, "{text} at {at:?}");
            assert!(i64::from(at.x + w) <= area.right(), "{text} at {at:?}");
        }
    }
}

fn grey(name: &str, id: u32, joinable: ErrorCode, f: impl Fn(&mut GameInfo)) -> GameInfo {
    let mut g = game(name);
    g.id = id;
    g.joinable = joinable.0;
    f(&mut g);
    g
}

/// The texts of `items` on baseline `y`, with their colours.
fn on_line(items: &[DrawItem], y: i32) -> Vec<(String, i32)> {
    items
        .iter()
        .filter_map(|i| match i {
            DrawItem::Text {
                text, at, color, ..
            } if at.y == y => Some((text.clone(), *color)),
            _ => None,
        })
        .collect()
}

/// Polish (greyed rows dim but legible): a greyed game's row is written in
/// the list's own colours and dimmed by a light shade over the row (inside
/// its frame's edge), not in the faint grey; a joinable row has none.
#[test]
fn greyed_rows_are_shaded_not_faint() {
    let hc = grey("hc trades", 2, ErrorCode::WRONG_GAME_KIND, |g| {
        g.hardcore = true
    });
    let lobby = lobby_with(vec![game("cows"), hc], Vec::new(), PingTable::new());
    let items = lobby.list_overlay(&adv);
    let shades: Vec<Rect> = items
        .iter()
        .filter_map(|i| match i {
            DrawItem::Shade {
                at,
                w,
                h,
                color: 0,
                mode,
            } if *mode == GREY_ROW_MODE => Some(Rect::new(at.x, at.y, *w as u16, *h as u16)),
            _ => None,
        })
        .collect();
    assert_eq!(shades.len(), 1, "{items:?}");
    let row = Lobby::row_top(1);
    assert_eq!(shades[0].y, row + 1);
    assert!(shades[0].x > PANEL.x);
    let base = row + ROW_BASE;
    assert!(shades[0].contains(Point::new(COL_NAME, base - 1)));
    let cells = on_line(&items, base);
    assert!(cells.iter().any(|c| c.0 == "hc trades"), "{cells:?}");
    for (text, colour) in &cells {
        assert_eq!(*colour, WHITE, "{text}");
    }
    // The shade comes after the row's cells (it dims them).
    let last_cell = items
        .iter()
        .rposition(|i| matches!(i, DrawItem::Text { at, .. } if at.y == base))
        .unwrap();
    let shade = items
        .iter()
        .position(|i| matches!(i, DrawItem::Shade { .. }))
        .unwrap();
    assert!(shade > last_cell);
}

/// Polish: the lines under the list: a selected grey game says why in
/// red; the legend names the other grey games' reasons on the lines left
/// in the frame, never wrapping past the frame's edge.
#[test]
fn the_legend_stays_inside_the_frame() {
    let games = vec![
        grey("hc trades", 1, ErrorCode::WRONG_GAME_KIND, |g| {
            g.hardcore = true
        }),
        grey("hell rush", 2, ErrorCode::DIFFICULTY_LOCKED, |g| {
            g.difficulty = 2
        }),
        grey("packed", 3, ErrorCode::GAME_FULL, |_| {}),
        grey("ubers", 4, lobby_code::LEVEL_RANGE, |g| g.level_min = 80),
        grey("dead", 5, ErrorCode::DEAD_HARDCORE, |_| {}),
    ];
    let mut lobby = lobby_with(games, Vec::new(), PingTable::new());
    let under = |lobby: &Lobby| {
        let items = lobby.list_overlay(&adv);
        UNDER_Y
            .iter()
            .map(|y| on_line(&items, *y))
            .collect::<Vec<_>>()
    };
    // Nothing selected: the legend on the two lines in the frame.
    let lines = under(&lobby);
    assert!(lines[0][0].0.starts_with("Grey: "), "{lines:?}");
    assert!(lines[2].is_empty(), "{lines:?}");
    // A grey game selected: its reason, then the others' on one line.
    lobby.selected = Some(1);
    let lines = under(&lobby);
    assert_eq!(
        lines[0][0],
        ("hc trades: For hardcore characters.".into(), RED)
    );
    assert!(lines[1][0].0.starts_with("Grey: "), "{lines:?}");
    assert!(!lines[1][0].0.contains("hardcore game"), "{lines:?}");
    assert!(lines[2].is_empty(), "{lines:?}");
}

/// Polish (column alignment): each column of the list starts a gutter past
/// the widest cell before it, however wide the font draws.
#[test]
fn the_list_columns_keep_a_gutter() {
    let wide = |font: u16, t: &[u16]| adv(font, t) * 3 / 2;
    let mut pings = PingTable::new();
    pings.insert("n1".into(), Some(18));
    let mut g = game("cows");
    g.server = "n1".into();
    let lobby = lobby_with(vec![g], vec![node("n1", "EU N1")], pings);
    let items = lobby.list_overlay(&wide);
    let head = LIST_TOP - 6;
    let x = |s: &str| {
        items
            .iter()
            .find_map(|i| match i {
                DrawItem::Text { text, at, .. } if text == s && at.y == head => Some(at.x),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no {s}: {items:?}"))
    };
    let w16 = |s: &str| wide(FONT16, &s.encode_utf16().collect::<Vec<u16>>());
    assert!(x("PL") >= x("DIFF") + w16("Norm").max(w16("DIFF")) + LIST_GUTTER);
    assert!(x("SERVER") >= x("PL") + w16("1/8") + LIST_GUTTER);
}

/// Polish (overlapping panels): the right panel taken over the chat draws
/// its ground, its controls and its items after the chat's; the chat taken
/// back over it draws after the right panel's items on its own ground, and
/// the right panel's controls under it take no input. The classic layout
/// draws as before.
#[test]
fn the_top_panel_hides_the_one_under_it() {
    let mut lobby = lobby_with(vec![game("cows")], Vec::new(), PingTable::new());
    let classic = build(&mut lobby);
    assert!(classic.iter().all(|c| c.raised == 0 && !c.inert));
    let over = lobby.overlay(0, &adv);
    assert!(!over
        .iter()
        .any(|i| matches!(i, DrawItem::Cut { .. } | DrawItem::Raised(_))));
    let pos = |over: &[DrawItem], f: &dyn Fn(&DrawItem) -> bool| over.iter().position(f);
    let is_text = |s: &'static str| move |i: &DrawItem| matches!(i, DrawItem::Text { text, .. } if text.starts_with(s));

    // The right panel over the chat.
    drag_panel(&mut lobby, Point::new(600, 258), (-250, 0));
    let v = build(&mut lobby);
    let raised: Vec<&str> = v
        .iter()
        .filter(|c| c.raised == RAISED_RIGHT)
        .filter_map(|c| c.text.as_deref())
        .collect();
    for b in ["CREATE", "GAMES", "LADDER", "FRIENDS"] {
        assert!(raised.contains(&b), "{b}: {raised:?}");
    }
    let over = lobby.overlay(0, &adv);
    let right = lobby.layout.right();
    let ground = pos(
        &over,
        &|i| matches!(i, DrawItem::Cut { at, .. } if *at == right.origin()),
    )
    .expect("the right panel's ground");
    let chat_text = pos(&over, &is_text("You are in the lobby")).expect("the chat's line");
    let raised_at = pos(&over, &|i| *i == DrawItem::Raised(RAISED_RIGHT)).unwrap();
    let list = pos(&over, &is_text("NAME")).expect("the list's head");
    assert!(chat_text < ground && ground < raised_at && raised_at < list);

    // The chat taken back over it.
    drag_panel(&mut lobby, Point::new(300, 165), (10, 0));
    let v = build(&mut lobby);
    let chat = lobby.layout.chat();
    for (i, c) in v
        .iter()
        .enumerate()
        .filter(|(_, c)| c.raised == RAISED_RIGHT)
    {
        let hit = c.hit_box();
        match layout::clip(hit, chat) {
            // Covered: no click there; covered whole: no input at all.
            Some(part) => {
                assert!(
                    lobby.covered(i, crate::ui::ui_ids::centre(part)),
                    "{:?}",
                    c.text
                );
                assert_eq!(c.inert, part == hit, "{:?} at {hit:?}", c.text);
            }
            None => {
                assert!(!c.inert && !lobby.covered(i, crate::ui::ui_ids::centre(hit)));
            }
        }
    }
    assert!(v
        .iter()
        .any(|c| c.inert && c.text.as_deref() == Some("CREATE")));
    let over = lobby.overlay(0, &adv);
    let ground = pos(
        &over,
        &|i| matches!(i, DrawItem::Cut { at, .. } if *at == chat.origin()),
    )
    .expect("the chat's ground");
    let list = pos(&over, &is_text("NAME")).expect("the list's head");
    let chat_text = pos(&over, &is_text("You are in the lobby")).unwrap();
    assert!(list < ground && ground < chat_text);
}

/// Polish: MARKET's tip shows while the pointer is on the button where it
/// is seen, not where a panel covers it (and a click there isn't the
/// button's).
#[test]
fn the_market_tip_needs_the_button_seen() {
    let mut lobby = lobby_with(Vec::new(), Vec::new(), PingTable::new());
    build(&mut lobby);
    let m = Lobby::market_rect();
    let on = Point::new(m.x + 20, m.y + 10);
    let tip = |lobby: &mut Lobby| {
        let mut flow = crate::ui::front_end::FlowCtx::default();
        let mut ctx = FrontCtx {
            expansion: true,
            flow: &mut flow,
            now_ms: 0,
        };
        lobby.pointer(&mut ctx, on);
        let t = lobby.market.tip().to_string();
        lobby
            .overlay(0, &adv)
            .iter()
            .any(|i| matches!(i, DrawItem::Text { text, .. } if *text == t))
    };
    assert!(tip(&mut lobby));
    // The chat moved over MARKET.
    drag_panel(&mut lobby, Point::new(300, 165), (300, -80));
    let v = build(&mut lobby);
    assert!(
        lobby.layout.chat().contains(on),
        "{:?}",
        lobby.layout.chat()
    );
    assert!(!tip(&mut lobby));
    let market = v
        .iter()
        .position(|c| c.text.as_deref() == Some("MARKET"))
        .unwrap();
    assert!(lobby.covered(market, on), "a click there is the chat's");
}

/// Rule (plan L15 at 16:9): the panels still move by their title strips
/// and RESET puts them back at their 16:9 places, the right panel at the
/// right edge.
#[test]
fn the_wide_lobbys_panels_move_and_reset() {
    let mut lobby = lobby_with(Vec::new(), Vec::new(), PingTable::new());
    lobby.canvas_changed(crate::ui::front_end::canvas::FrontCanvas::full(
        FrameSize::WIDE,
    ));
    build(&mut lobby);
    let (chat, right) = (lobby.layout.chat(), lobby.layout.right());
    assert_eq!(right.x, layout::RIGHT_CLASSIC.x + WIDE_RIGHT);
    assert!(lobby.layout.is_classic());
    drag_panel(&mut lobby, Point::new(chat.x + 20, chat.y + 8), (24, -40));
    drag_panel(&mut lobby, Point::new(right.x + 160, right.y + 8), (-24, 0));
    assert_eq!(
        lobby.layout.chat().origin(),
        Point::new(chat.x + 24, chat.y - 40)
    );
    assert_eq!(
        lobby.layout.right().origin(),
        Point::new(right.x - 24, right.y)
    );
    assert!(!lobby.layout.is_classic());
    act_on(&mut lobby, act::RESET);
    assert_eq!((lobby.layout.chat(), lobby.layout.right()), (chat, right));
}

/// Polish at 16:9 (no lobby art): the right panel taken over the chat
/// draws its ground as the chat blacked out where it lies over it, then
/// its frames on their backing; nothing cuts the 800-column art.
#[test]
fn the_wide_top_panel_hides_the_one_under_it_without_art() {
    let mut lobby = lobby_with(vec![game("cows")], Vec::new(), PingTable::new());
    lobby.canvas_changed(crate::ui::front_end::canvas::FrontCanvas::full(
        FrameSize::WIDE,
    ));
    build(&mut lobby);
    let right = lobby.layout.right();
    drag_panel(
        &mut lobby,
        Point::new(right.x + 160, right.y + 8),
        (-500, 0),
    );
    build(&mut lobby);
    let cover = layout::clip(lobby.layout.chat(), lobby.layout.right()).expect("over the chat");
    let over = lobby.overlay(0, &adv);
    let under = lobby.underlay();
    for items in [&over, &under] {
        assert!(!items.iter().any(|i| matches!(i, DrawItem::Cut { .. })));
    }
    assert!(over.iter().any(|i| *i
        == DrawItem::Rect {
            at: cover.origin(),
            w: i32::from(cover.w),
            h: i32::from(cover.h),
        }));
}
