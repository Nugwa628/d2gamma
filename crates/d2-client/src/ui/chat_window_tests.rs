use super::*;
use crate::chat::{FakeChat, Room};
use bevy::prelude::Entity;

fn act(a: Action) -> UiEvent {
    UiEvent::Action(ActionId(a.index() as u16))
}

fn ev(w: &mut ChatWindow, e: UiEvent) -> Taken {
    w.event(e, &Screen::R800, None)
}

fn typed(w: &mut ChatWindow, s: &str) {
    for u in s.encode_utf16() {
        assert_eq!(ev(w, UiEvent::Char(u)), Taken::Yes);
    }
}

fn draws(w: &mut ChatWindow, now: u64) -> Vec<UiDraw> {
    let mut out: Vec<UiDraw> = Vec::new();
    w.pump(now);
    w.draw(&Screen::R800, None, Some(1), now, &mut out);
    out
}

fn texts(d: &[UiDraw]) -> Vec<String> {
    d.iter()
        .filter_map(|d| match d {
            UiDraw::Text(t) => Some(String::from_utf16_lossy(&t.text)),
            _ => None,
        })
        .collect()
}

fn game_lines(w: &ChatWindow) -> Vec<String> {
    w.model
        .lines(Tab::Room(Room::Game))
        .iter()
        .map(|l| format!("{}|{}", l.who.char, l.text))
        .collect()
}

#[test]
fn the_game_tabs_line_is_the_games_chat_message() {
    // C→S 0x15: type 1, language 0, the text, its NUL, no whisper target.
    assert_eq!(game_chat_body("hi"), [0x15, 1, 0, b'h', b'i', 0, 0, 0]);
    // Latin-1 as is, others as `?`; NULs dropped; at most 255 bytes.
    assert_eq!(game_chat_body("é€\0"), [0x15, 1, 0, 0xE9, b'?', 0, 0, 0]);
    assert_eq!(game_chat_body(&"x".repeat(300)).len(), 3 + 255 + 3);
}

#[test]
fn enter_opens_the_input_and_enter_sends_the_line_to_the_game() {
    let mut w = ChatWindow::new(None);
    w.pump(0);
    assert!(!w.focused);
    // The key's action, then its typed Enter (the same press): the line
    // opens and stays open.
    assert_eq!(ev(&mut w, act(Action::OpenChat)), Taken::Yes);
    assert_eq!(ev(&mut w, UiEvent::Char(0x0D)), Taken::Yes);
    assert!(w.focused);
    w.pump(10);
    typed(&mut w, "Hi all!");
    assert_eq!(w.model.input, "Hi all!");
    typed(&mut w, "\u{8}");
    assert_eq!(w.model.input, "Hi all");
    assert_eq!(
        ev(&mut w, UiEvent::Char(0x0D)),
        Taken::Send(game_chat_body("Hi all"))
    );
    assert!(!w.focused);
    assert!(w.model.input.is_empty());
    // Not echoed: the server sends the line back to every player.
    w.pump(20);
    assert!(game_lines(&w).is_empty());
}

#[test]
fn esc_leaves_the_input_and_drops_the_line() {
    let mut w = ChatWindow::new(None);
    w.pump(0);
    ev(&mut w, act(Action::OpenChat));
    w.pump(1);
    typed(&mut w, "abc");
    assert_eq!(ev(&mut w, UiEvent::Char(0x1B)), Taken::Yes);
    assert!(!w.focused && w.model.input.is_empty());
    // Not typing: characters are the game's.
    assert_eq!(ev(&mut w, UiEvent::Char(u16::from(b'i'))), Taken::No);
}

#[test]
fn the_toggle_hides_and_shows_the_window() {
    let mut w = ChatWindow::new(None);
    let s = Screen::R800;
    let shown = w.rect(&s);
    assert!(shown.w > 0);
    assert_eq!(ev(&mut w, act(Action::ToggleChatWindow)), Taken::Yes);
    assert!(!w.shown);
    assert_eq!(w.rect(&s), Rect::new(0, 0, 0, 0));
    assert!(draws(&mut w, 0).is_empty());
    assert!(w.ui_elements(&s).is_empty());
    ev(&mut w, act(Action::ToggleChatWindow));
    assert!(w.shown);
    assert_eq!(w.rect(&s), shown);
    // Other actions are not the window's.
    assert_eq!(ev(&mut w, act(Action::ToggleInventory)), Taken::No);
}

#[test]
fn a_game_line_shows_in_the_game_tab() {
    let mut w = ChatWindow::new(None);
    w.player_line("Bob", "hello");
    w.game_message("Ana joined our world. Diablo's minions grow stronger.", 4);
    let d = draws(&mut w, 0);
    assert_eq!(
        game_lines(&w),
        [
            "Bob|hello",
            "|Ana joined our world. Diablo's minions grow stronger."
        ]
    );
    let t = texts(&d);
    assert!(t.iter().any(|s| s.eq_ignore_ascii_case("game")), "{t:?}");
    assert!(t.iter().any(|s| s.contains("hello")), "{t:?}");
}

#[test]
fn the_collapsed_bar_counts_the_unread_lines() {
    let mut w = ChatWindow::new(None);
    let s = Screen::R800;
    draws(&mut w, 0);
    let pl = Places::beside(&s, false, false).unwrap();
    // The `-` box collapses the window to its bar.
    assert_eq!(
        ev(
            &mut w,
            UiEvent::Press {
                button: PointerButton::Left,
                at: centre(pl.collapse)
            }
        ),
        Taken::Yes
    );
    assert!(w.collapsed);
    assert_eq!(w.rect(&s), pl.bar);
    w.player_line("Bob", "one");
    w.player_line("Bob", "two");
    let d = draws(&mut w, 10);
    assert_eq!(w.model.unread(Tab::Room(Room::Game)), 2);
    let t = texts(&d);
    assert!(t.iter().any(|s| s == "2"), "{t:?}");
    assert!(!t.iter().any(|s| s.contains("one")), "{t:?}");
    // The bar is one line high, at the window's bottom.
    assert!(pl.bar.h < 20 && pl.bar.y + i32::from(pl.bar.h) == pl.window.y + WIN_H);
    // The `+` box opens it again; the lines are then read.
    ev(
        &mut w,
        UiEvent::Press {
            button: PointerButton::Left,
            at: centre(pl.expand),
        },
    );
    assert!(!w.collapsed);
    draws(&mut w, 20);
    assert_eq!(w.model.unread(Tab::Room(Room::Game)), 0);
}

#[test]
fn the_window_turns_light_when_idle() {
    let backing = |d: &[UiDraw]| {
        d.iter().find_map(|d| match d {
            UiDraw::Rect(r) => Some(r.mode),
            _ => None,
        })
    };
    let mut w = ChatWindow::new(None);
    assert_eq!(backing(&draws(&mut w, 0)), Some(ACTIVE_MODE));
    assert_eq!(backing(&draws(&mut w, IDLE_MS - 1)), Some(ACTIVE_MODE));
    assert_eq!(backing(&draws(&mut w, IDLE_MS)), Some(IDLE_MODE));
    // A new line wakes it.
    w.player_line("Bob", "hey");
    assert_eq!(backing(&draws(&mut w, IDLE_MS + 100)), Some(ACTIVE_MODE));
    assert_eq!(backing(&draws(&mut w, 2 * IDLE_MS + 100)), Some(IDLE_MODE));
    // The mouse over it, or typing, keeps it awake.
    let pl = Places::beside(&Screen::R800, false, false).unwrap();
    w.mouse_at(&Screen::R800, centre(pl.window));
    assert_eq!(backing(&draws(&mut w, 4 * IDLE_MS)), Some(ACTIVE_MODE));
    w.mouse_at(&Screen::R800, Point::new(700, 50));
    ev(&mut w, act(Action::OpenChat));
    assert_eq!(backing(&draws(&mut w, 8 * IDLE_MS)), Some(ACTIVE_MODE));
}

#[test]
fn a_press_elsewhere_ends_the_typing_and_is_the_games() {
    let mut w = ChatWindow::new(None);
    let s = Screen::R800;
    ev(&mut w, act(Action::OpenChat));
    // While typing the window takes the whole frame's presses …
    assert_eq!(w.rect(&s), s.frame_rect());
    let at = Point::new(600, 100);
    assert_eq!(
        ev(
            &mut w,
            UiEvent::Press {
                button: PointerButton::Left,
                at
            }
        ),
        Taken::No
    );
    // … and gives one outside back, typing over.
    assert!(!w.focused);
    // A press on the input line starts typing again.
    let pl = Places::beside(&s, false, false).unwrap();
    draws(&mut w, 0);
    ev(
        &mut w,
        UiEvent::Press {
            button: PointerButton::Left,
            at: centre(pl.layout.input),
        },
    );
    assert!(w.focused);
}

#[test]
fn the_window_keeps_clear_of_the_side_panels() {
    for s in [Screen::R800, Screen::R640] {
        let free = Places::beside(&s, false, false).unwrap();
        assert_eq!(free.window.x, s.frame_left() + 6, "{s:?}");
        assert!(free.window.y + WIN_H + HUD_CLEAR <= s.h);
        // Right of a left panel.
        let l = Places::beside(&s, true, false).unwrap();
        assert!(l.window.x >= s.left_end(), "{s:?}");
        assert!(l.window.x + i32::from(l.window.w) <= s.frame_right());
        // Left of a right panel.
        let r = Places::beside(&s, false, true).unwrap();
        assert!(
            r.window.x + i32::from(r.window.w) <= s.w - s.left_end(),
            "{s:?}"
        );
    }
    // Both sides open in a 4:3 frame: no room; drawn only while typing.
    let s = Screen::R800;
    assert!(Places::beside(&s, true, true).is_none());
    let mut w = ChatWindow::new(None);
    w.panels = (true, true);
    assert_eq!(w.rect(&s), Rect::new(0, 0, 0, 0));
    assert!(draws(&mut w, 0).is_empty());
    ev(&mut w, act(Action::OpenChat));
    assert!(!draws(&mut w, 1).is_empty());
}

#[test]
fn realm_tabs_come_with_the_realm_and_count_unread_lines() {
    let mut w = ChatWindow::new(None);
    draws(&mut w, 0);
    assert_eq!(w.model.tabs(), [Tab::Room(Room::Game)]);
    w.set_realm(Box::new(FakeChat::seeded()));
    let d = draws(&mut w, 10);
    let tabs = w.model.tabs();
    assert!(tabs.contains(&Tab::Room(Room::Mod)), "{tabs:?}");
    assert!(tabs.contains(&Tab::Dm), "{tabs:?}");
    // The Game tab stays the open one; the realm's seeded lines are unread
    // in their tabs.
    assert_eq!(w.model.open_tab_now(), Tab::Room(Room::Game));
    assert!(w.model.unread(Tab::Room(Room::Guild)) > 0);
    let ids: Vec<String> = w
        .ui_elements(&Screen::R800)
        .into_iter()
        .map(|e| e.id)
        .collect();
    assert!(ids.contains(&"chat_window.tab(mod)".to_string()), "{ids:?}");
    assert!(ids.contains(&"chat_window.input".to_string()), "{ids:?}");
    assert!(texts(&d)
        .iter()
        .any(|s| s.to_lowercase().starts_with("mod")));
}

#[test]
fn key_presses_type_their_text() {
    let k = |code, logical: Key, text: Option<&str>| KeyboardInput {
        key_code: code,
        logical_key: logical,
        state: ButtonState::Pressed,
        text: text.map(Into::into),
        repeat: false,
        window: Entity::PLACEHOLDER,
    };
    assert_eq!(
        typed_units(&k(KeyCode::KeyA, Key::Character("A".into()), Some("A"))),
        [u16::from(b'A')]
    );
    assert_eq!(
        typed_units(&k(KeyCode::Digit1, Key::Character("!".into()), Some("!"))),
        [u16::from(b'!')]
    );
    assert_eq!(
        typed_units(&k(KeyCode::Space, Key::Space, Some(" "))),
        [u16::from(b' ')]
    );
    assert_eq!(
        typed_units(&k(KeyCode::Enter, Key::Enter, Some("\r"))),
        [ENTER]
    );
    assert_eq!(typed_units(&k(KeyCode::Escape, Key::Escape, None)), [ESC]);
    assert_eq!(
        typed_units(&k(KeyCode::Backspace, Key::Backspace, None)),
        [BACKSPACE]
    );
    assert_eq!(typed_units(&k(KeyCode::ArrowUp, Key::ArrowUp, None)), [UP]);
    assert!(typed_units(&k(KeyCode::ShiftLeft, Key::Shift, None)).is_empty());
    let mut up = k(KeyCode::KeyA, Key::Character("a".into()), Some("a"));
    up.state = ButtonState::Released;
    assert!(typed_units(&up).is_empty());
}

fn centre(r: Rect) -> Point {
    crate::ui::ui_ids::centre(r)
}

fn left(at: Point) -> UiEvent {
    UiEvent::Press {
        button: PointerButton::Left,
        at,
    }
}

/// A left press at `from`, the pointer to `to`, the release there.
fn drag(w: &mut ChatWindow, from: Point, to: Point) {
    assert_eq!(ev(w, left(from)), Taken::Yes);
    ev(w, UiEvent::CursorMoved(to));
    ev(
        w,
        UiEvent::Release {
            button: PointerButton::Left,
            at: to,
        },
    );
}

/// The places drawn on screen `s`.
fn drawn_on(w: &mut ChatWindow, s: &Screen) -> Option<Places> {
    let mut out: Vec<UiDraw> = Vec::new();
    w.draw(s, None, Some(1), w.now_ms(), &mut out);
    w.drawn()
}

fn window(w: &mut ChatWindow) -> Rect {
    drawn_on(w, &Screen::R800).expect("drawn").window
}

/// The default spot at 800 × 600: the frame's left, above the HUD.
const HOME: Rect = Rect::new(6, 334, 400, 162);

fn at(r: Rect, dx: i32, dy: i32) -> Point {
    Point::new(r.x + dx, r.y + dy)
}

/// The window dragged by its strip from the default spot to (56, 254).
fn moved() -> ChatWindow {
    let mut w = ChatWindow::new(None);
    assert_eq!(window(&mut w), HOME);
    drag(&mut w, at(HOME, 100, 10), at(HOME, 150, -70));
    w
}

fn ids(w: &ChatWindow) -> Vec<String> {
    w.ui_elements(&Screen::R800)
        .into_iter()
        .map(|e| e.id)
        .collect()
}

fn element(w: &ChatWindow, id: &str) -> Point {
    let e = w
        .ui_elements(&Screen::R800)
        .into_iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("{id}"));
    centre(e.rect)
}

#[test]
fn the_tab_strip_drags_the_window() {
    let mut w = moved();
    assert_eq!(window(&mut w), Rect::new(56, 254, 400, 162));
    assert!(w.moved());
    // The move is saved once.
    let l = w.take_layout().expect("a change");
    assert!(!l.locked);
    assert_eq!(l.panels.len(), 1);
    assert_eq!(w.take_layout(), None);
    // Its parts follow it.
    let pl = w.drawn().unwrap();
    assert_eq!(pl, Places::at(pl.window));
    assert_eq!(pl.layout.tabs.y, 257);
    // A tab pressed opens and drags too.
    let mut w = ChatWindow::new(None);
    w.set_realm(Box::new(FakeChat::seeded()));
    draws(&mut w, 0);
    draws(&mut w, 1);
    let global = element(&w, "chat_window.tab(global)");
    drag(&mut w, global, Point::new(global.x + 20, global.y - 30));
    assert_eq!(w.model.open_tab_now(), Tab::Room(Room::Global));
    assert_eq!(window(&mut w), Rect::new(26, 304, 400, 162));
}

#[test]
fn a_press_that_hardly_moves_is_no_drag() {
    let mut w = ChatWindow::new(None);
    window(&mut w);
    drag(&mut w, at(HOME, 100, 10), at(HOME, 102, 8));
    assert_eq!(window(&mut w), HOME);
    assert!(!w.moved());
    assert_eq!(w.take_layout(), None);
    // The lines aren't a grab zone: a press there drags nothing.
    drag(&mut w, at(HOME, 100, 60), at(HOME, 200, 0));
    assert_eq!(window(&mut w), HOME);
}

#[test]
fn the_window_snaps_to_the_frame_and_the_hud_and_stays_above_it() {
    let mut w = moved();
    // 4 px from the frame's left edge: onto it.
    drag(&mut w, Point::new(156, 264), Point::new(104, 264));
    assert_eq!(window(&mut w), Rect::new(0, 254, 400, 162));
    // 5 px above the control panel's top (row 496): onto it.
    drag(&mut w, Point::new(100, 264), Point::new(100, 339));
    assert_eq!(window(&mut w), Rect::new(0, 334, 400, 162));
    // Never over the control panel or off the frame.
    drag(&mut w, Point::new(100, 344), Point::new(900, 900));
    assert_eq!(window(&mut w), Rect::new(400, 334, 400, 162));
    drag(&mut w, Point::new(500, 344), Point::new(-500, -500));
    assert_eq!(window(&mut w), Rect::new(0, 0, 400, 162));
}

#[test]
fn edges_and_corners_resize_the_window() {
    let mut w = ChatWindow::new(None);
    window(&mut w);
    // The bottom-right corner pulled far up-left: the smallest window.
    drag(&mut w, at(HOME, 399, 161), Point::new(5, 5));
    assert_eq!(
        window(&mut w),
        Rect::new(6, 334, MIN_W as u16, MIN_H as u16)
    );
    let pl = w.drawn().unwrap();
    assert_eq!(i32::from(pl.layout.body.h), 2 * 15, "two lines");
    // The top edge pulled up: taller, more lines.
    drag(&mut w, Point::new(106, 334), Point::new(106, 184));
    assert_eq!(window(&mut w), Rect::new(6, 184, 200, 222));
    assert_eq!(i32::from(w.drawn().unwrap().layout.body.h), 12 * 15);
    // The right edge pulled past the frame's: it stops there.
    drag(&mut w, Point::new(205, 300), Point::new(802, 300));
    assert_eq!(window(&mut w), Rect::new(6, 184, 794, 222));
    // The parts fit the new size.
    let pl = w.drawn().unwrap();
    assert_eq!(pl.collapse.x + i32::from(pl.collapse.w), 800 - 3);
    assert_eq!(pl.layout.input.y + 16, 184 + 222 - 3);
    assert!(w.take_layout().is_some());
}

#[test]
fn the_rect_stays_through_collapse_hide_and_a_view_change() {
    let mut w = moved();
    w.take_layout();
    let r = window(&mut w);
    // Collapsed: the bar at the moved window's bottom.
    let pl = w.drawn().unwrap();
    assert_eq!(ev(&mut w, left(centre(pl.collapse))), Taken::Yes);
    assert!(w.collapsed);
    let pl = drawn_on(&mut w, &Screen::R800).unwrap();
    assert_eq!(pl.window, r);
    assert_eq!(pl.bar.y + i32::from(pl.bar.h), r.y + i32::from(r.h));
    assert_eq!(w.rect(&Screen::R800), pl.bar);
    ev(&mut w, left(centre(pl.expand)));
    assert!(!w.collapsed);
    assert_eq!(window(&mut w), r);
    // Hidden and shown.
    ev(&mut w, act(Action::ToggleChatWindow));
    assert_eq!(drawn_on(&mut w, &Screen::R800), None);
    ev(&mut w, act(Action::ToggleChatWindow));
    assert_eq!(window(&mut w), r);
    // The 1280 × 720 view: the same gaps to the frame's bottom-left (the
    // frame starts 240 columns left of the box).
    let play = drawn_on(&mut w, &Screen::PLAY).unwrap().window;
    assert_eq!(play, Rect::new(56 - 240, 720 - 104 - 80 - 162, 400, 162));
    assert_eq!(window(&mut w), r);
    assert_eq!(w.take_layout(), None, "nothing new to save");
    // The default spot stays the default spot of each view.
    let mut d = ChatWindow::new(None);
    assert_eq!(window(&mut d), HOME);
    assert_eq!(
        drawn_on(&mut d, &Screen::PLAY).unwrap(),
        Places::beside(&Screen::PLAY, false, false).unwrap()
    );
    assert_eq!(window(&mut d), HOME);
    assert!(!d.moved());
    assert_eq!(d.take_layout(), None);
}

#[test]
fn the_collapsed_bar_drags_and_its_tab_opens_on_a_click() {
    let mut w = ChatWindow::new(None);
    window(&mut w);
    w.set_collapsed(true);
    let bar = drawn_on(&mut w, &Screen::R800).unwrap().bar;
    assert_eq!(bar, Rect::new(6, 334 + 162 - 19, 400, 19));
    drag(&mut w, at(bar, 300, 9), at(bar, 300, -91));
    assert!(w.collapsed);
    // Up 100 rows; 6 px from the frame's left edge, so onto it.
    let pl = drawn_on(&mut w, &Screen::R800).unwrap();
    assert_eq!(pl.window, Rect::new(0, 234, 400, 162));
    assert_eq!(pl.bar.y, 234 + 162 - 19);
    // A click on its tab opens the window there.
    let tab = element(&w, "chat_window.tab(game)");
    drag(&mut w, tab, tab);
    assert!(!w.collapsed);
    assert_eq!(window(&mut w), Rect::new(0, 234, 400, 162));
}

#[test]
fn the_menu_locks_and_resets_the_position() {
    let mut w = moved();
    w.take_layout();
    window(&mut w);
    let pl = w.drawn().unwrap();
    assert!(ids(&w).contains(&"chat_window.menu".to_string()));
    assert!(!ids(&w).contains(&"chat_window.lock".to_string()));
    ev(&mut w, left(centre(pl.menu)));
    assert!(w.menu);
    let t = texts(&draws(&mut w, 0));
    assert!(t.iter().any(|s| s == "Lock position"), "{t:?}");
    assert!(t.iter().any(|s| s == "Reset position"), "{t:?}");
    // While open the window takes the frame's presses (one elsewhere
    // shuts it).
    assert_eq!(w.rect(&Screen::R800), Screen::R800.frame_rect());
    let [lock, reset] = pl.menu_rows();
    assert!(ids(&w).contains(&"chat_window.lock".to_string()));
    ev(&mut w, left(centre(lock)));
    assert!(w.locked() && !w.menu);
    assert!(w.take_layout().expect("the lock is saved").locked);
    // Locked: the strip and the edges take nothing.
    let r = window(&mut w);
    drag(&mut w, at(r, 100, 10), at(r, 200, 100));
    drag(&mut w, at(r, 399, 161), at(r, 300, 100));
    assert_eq!(window(&mut w), r);
    assert_eq!(w.take_layout(), None);
    ev(&mut w, left(centre(pl.menu)));
    let t = texts(&draws(&mut w, 0));
    assert!(t.iter().any(|s| s == "Unlock position"), "{t:?}");
    // Reset: the default spot, still locked.
    ev(&mut w, left(centre(reset)));
    assert_eq!(window(&mut w), HOME);
    assert!(!w.moved() && w.locked());
    assert!(w.take_layout().is_some());
    // A press elsewhere shuts the menu and is the game's.
    let menu = w.drawn().unwrap().menu;
    ev(&mut w, left(centre(menu)));
    assert!(w.menu);
    assert_eq!(ev(&mut w, left(Point::new(700, 100))), Taken::No);
    assert!(!w.menu);
}

#[test]
fn a_saved_layout_comes_back() {
    use crate::ui::float::{LayoutKey, LayoutStore, Scene};
    let mut w = moved();
    w.set_locked(true);
    let r = window(&mut w);
    let mut store = LayoutStore::default();
    let key = LayoutKey::new("eu1", "Nugwa", Scene::Game);
    store.set(key.clone(), w.take_layout().unwrap());
    let store = LayoutStore::parse(&store.to_text()).unwrap();
    let mut again = ChatWindow::new(None);
    again.apply_layout(store.get(&key).unwrap().clone());
    assert_eq!(window(&mut again), r);
    assert!(again.locked() && again.moved());
    assert_eq!(again.take_layout(), None, "putting it back is no change");
}

#[test]
fn a_moved_window_keeps_clear_of_open_side_panels() {
    let mut w = ChatWindow::new(None);
    window(&mut w);
    // Against the frame's right edge.
    drag(&mut w, at(HOME, 100, 10), at(HOME, 500, 10));
    let r = window(&mut w);
    assert_eq!(r, Rect::new(400, 334, 400, 162));
    w.take_layout();
    // A right panel open: left of it, narrowed to the room.
    w.panels = (false, true);
    let s = Screen::R800;
    let shown = window(&mut w);
    assert_eq!(shown, Rect::new(0, 334, 394, 162));
    assert!(shown.x + i32::from(shown.w) <= s.w - s.left_end());
    // Both open in a 4:3 frame: no room, so not drawn (but while typing).
    w.panels = (true, true);
    assert_eq!(drawn_on(&mut w, &s), None);
    ev(&mut w, act(Action::OpenChat));
    assert_eq!(window(&mut w), r);
    ev(&mut w, UiEvent::Char(0x1B));
    // Shut again: back where the player put it, nothing to save.
    w.panels = (false, false);
    assert_eq!(window(&mut w), r);
    assert_eq!(w.take_layout(), None);
    // Dragged while beside a panel: taken where it shows.
    w.panels = (false, true);
    drag(&mut w, at(shown, 100, 10), at(shown, 100, -40));
    assert_eq!(window(&mut w), Rect::new(0, 284, 394, 162));
}
