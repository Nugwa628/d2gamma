// D2RUST's own: the in-game chat window.
//! The chat module (`crate::chat`, drawn by `crate::ui::chat`) in a game on
//! a realm: a framed window at the bottom left, above the control panel,
//! with the realm's tabs (Global, Mod, Guild, DM) and the game's own
//! (Game: the players of this game, and every game or system message the
//! top-left list showed before: quests, "joined the game", hostility, the
//! game's notes). It replaces the old player chat of the top-left list
//! and its message log in an online game; single player keeps the list
//! (and draws no window).
//!
//! - **Hide / show:** `ToggleChatWindow` (default M, rebindable).
//! - **Enter** (`OpenChat`) shows the window and focuses its input line;
//!   Enter sends the line and leaves the input, Esc leaves it (the line
//!   goes). While the input has the focus the game's key bindings wait.
//! - **Collapse** (the `-` box at the strip's right): the window becomes a
//!   one-line bar of the tabs with their unread counts; a tab or the `+`
//!   box opens it again.
//! - **Idle:** after [`IDLE_MS`] without input, a new line or the mouse
//!   over it, its backing turns light ([`IDLE_MODE`]); a line, a key, the
//!   mouse or typing brings it back ([`ACTIVE_MODE`]).
//! - **Move and resize** (a floating panel, `crate::ui::float`): the tab
//!   strip (or the collapsed bar) drags the window, its edges and corners
//!   resize it; it snaps to the frame's edges and to the top of the
//!   control panel, and stays in the rows above it. A press there becomes
//!   a drag once the pointer moves [`DRAG_START`] pixels; a tab of the
//!   bar opens on a release without a drag. The `=` box beside `-` opens
//!   the window's menu: lock the position (no drag, no resize) and reset
//!   it to the default spot. Collapsing, hiding and a new view size keep
//!   the rect (anchored to its nearest corner of the frame); the host saves
//!   it per player ([`ChatWindow::take_layout`], scene `game` of
//!   `layouts.toml`). At its default spot the window keeps beside the
//!   open side panels; a moved window keeps its rect and is only shifted
//!   clear of them while they are open.
//!
//! A line said in the Game tab goes to the game server as its chat
//! message (C→S 0x15, [`game_chat_body`]); the server sends it back to
//! every player of the game (the sender too) as a chat line.

use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::input::ButtonState;

use crate::chat::source::ChatSource;
use crate::chat::{ChatModel, Host, Merged, Tab};
use crate::controls::Action;
use crate::ui::chat::tabs::{self, TabBox};
use crate::ui::chat::{self, ChatFont, ChatLayout, ChatPaint, Hit};
use crate::ui::draw::{LineRequest, RectRequest, TextRequest, TextStyle, UiDraw, UiDrawSink};
use crate::ui::float::{
    geom, grab, Anchored, Canvas, FloatLayer, FloatPanel, MinSize, SavedLayout, Zone,
};
use crate::ui::geom::{Point, Rect};
use crate::ui::layout::Screen;
use crate::ui::panel::{ActionId, PointerButton, UiEvent};
use crate::ui::text::TextOpts;
use crate::ui::ui_ids::UiElement;

use super::FontMeasure;

/// The line font (the font of the game's chat lines) and the small font
/// (tabs that don't fit, badges, the user column, hints).
pub const LINE_FONT: u16 = 13;
pub const SMALL_FONT: u16 = 6;
const LINE_ROW: i32 = 15;
const SMALL_ROW: i32 = 10;

/// The window's size at most at its default spot; a narrow frame gets a
/// narrower window.
pub const WIN_W: i32 = 400;
/// The narrowest window: less room beside the side panels hides it.
pub const MIN_W: i32 = 200;
pub const WIN_H: i32 = 162;
/// Lines shown at once at the default size.
pub const ROWS: i32 = 8;
/// The rows of a window around its lines: the strip above, the input line
/// below.
const CHROME_H: i32 = 3 + STRIP_H + 4 + 20;
/// The lowest window: two lines.
pub const MIN_H: i32 = CHROME_H + 2 * LINE_ROW;
/// Rows kept free under the window: the control panel and its globes.
pub const HUD_CLEAR: i32 = 104;
/// The gap to the frame's left edge.
const MARGIN: i32 = 6;
/// The tab strip's and the bar's height.
const STRIP_H: i32 = 15;
/// The user column's width (with its divider).
const COLUMN_W: u16 = 104;
/// The collapse / expand box and the menu box.
const BOX_W: i32 = 14;
/// The menu's width and row height.
const MENU_W: i32 = 124;
const MENU_ROW: i32 = 16;

/// The id the window's rect is saved under (`layouts.toml`).
pub const PANEL: &str = "chat";
/// How far (pixels, either way) the pointer moves before a press on the
/// strip, the bar or an edge becomes a drag.
pub const DRAG_START: i32 = 3;

/// Idle after this long without input, a new line or the mouse over it.
pub const IDLE_MS: u64 = 5_000;
/// The backing's rectangle draw mode: the most opaque translucency while
/// in use, the most transparent while idle.
pub const ACTIVE_MODE: u8 = 2;
pub const IDLE_MODE: u8 = 0;
/// The backing's colour (black in the act palettes).
const BLACK: u8 = 0;

/// The longest line the input takes (the realm's `MAX_CHAT`).
pub const MAX_INPUT: usize = 200;

/// The typed characters of the edge's private-use arrows
/// (`crate::ui::edge::key_chars`).
const UP: u16 = 0xF026;
const DOWN: u16 = 0xF028;
const ENTER: u16 = 0x0D;
const ESC: u16 = 0x1B;
const BACKSPACE: u16 = 8;

/// The window's places on a screen (box space: the 800-wide UI box; the
/// frame's left columns are negative in a wider frame).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Places {
    /// The whole window.
    pub window: Rect,
    /// The chat view inside it.
    pub layout: ChatLayout,
    /// The collapse box at the strip's right.
    pub collapse: Rect,
    /// The menu box left of it.
    pub menu: Rect,
    /// The collapsed bar: its tab strip and its expand box.
    pub bar: Rect,
    pub bar_tabs: Rect,
    pub expand: Rect,
}

impl Places {
    /// The places of a window at its default size from column `x0` to at
    /// most `x1` (box space).
    pub fn of(s: &Screen, x0: i32, x1: i32) -> Places {
        let w = WIN_W.min(x1 - x0).max(MIN_W);
        let y = (s.h - HUD_CLEAR - WIN_H).max(0);
        Places::at(Rect::new(x0, y, w as u16, WIN_H as u16))
    }

    /// The places of a window at `window` (box space): as many lines as
    /// its height holds.
    pub fn at(window: Rect) -> Places {
        let (x, y) = (window.x, window.y);
        let (w, h) = (i32::from(window.w), i32::from(window.h));
        let rows = ((h - CHROME_H) / LINE_ROW).max(1);
        let inner_w = (w - 10).max(0) as u16;
        let collapse = Rect::new(x + w - BOX_W - 3, y + 3, BOX_W as u16, STRIP_H as u16);
        let menu = Rect::new(collapse.x - BOX_W - 3, y + 3, BOX_W as u16, STRIP_H as u16);
        let tabs = Rect::new(
            x + 3,
            y + 3,
            (menu.x - 3 - (x + 3)).max(0) as u16,
            STRIP_H as u16,
        );
        let body = Rect::new(
            x + 5,
            y + 3 + STRIP_H + 4,
            inner_w,
            (rows * LINE_ROW) as u16,
        );
        let input = Rect::new(x + 5, y + h - 3 - 16, inner_w, 16);
        let bar_y = y + h - STRIP_H - 4;
        let bar = Rect::new(x, bar_y, w as u16, (STRIP_H + 4) as u16);
        Places {
            window,
            layout: ChatLayout {
                tabs,
                body,
                column_w: COLUMN_W,
                input,
            },
            collapse,
            menu,
            bar,
            bar_tabs: Rect::new(
                x + 3,
                bar_y + 2,
                (w - 3 - BOX_W - 6).max(0) as u16,
                STRIP_H as u16,
            ),
            expand: Rect::new(x + w - BOX_W - 3, bar_y + 2, BOX_W as u16, STRIP_H as u16),
        }
    }

    /// Where the window goes at its default spot with the side panels
    /// `left` / `right` open: at the frame's left, right of an open left
    /// panel, never over an open right panel; `None` when no room is left
    /// between them.
    pub fn beside(s: &Screen, left: bool, right: bool) -> Option<Places> {
        let x0 = if left { s.left_end() } else { s.frame_left() } + MARGIN;
        let x1 = if right {
            s.w - s.left_end()
        } else {
            s.frame_right()
        } - MARGIN;
        (x1 - x0 >= MIN_W).then(|| Places::of(s, x0, x1))
    }

    /// The menu's rows (lock, reset) under the menu box, at the window's
    /// right.
    pub fn menu_rows(&self) -> [Rect; 2] {
        let x = self.window.x + i32::from(self.window.w) - 3 - MENU_W;
        let y = self.menu.y + i32::from(self.menu.h) + 1;
        let row = |i: i32| Rect::new(x, y + i * MENU_ROW, MENU_W as u16, MENU_ROW as u16);
        [row(0), row(1)]
    }
}

/// The area the window moves in (canvas space): the frame's columns from
/// its first one, the rows above the control panel.
pub fn canvas(s: &Screen) -> Canvas {
    let w = (s.frame_right() - s.frame_left()).clamp(MIN_W, i32::from(u16::MAX));
    let h = (s.h - HUD_CLEAR).clamp(MIN_H, i32::from(u16::MAX));
    Canvas::new(w as u16, h as u16)
}

/// The default spot on canvas `c` (canvas space): [`Places::beside`] with
/// no side panel open.
pub fn default_rect(c: Canvas) -> Rect {
    let w = WIN_W.min(i32::from(c.w) - 2 * MARGIN).max(MIN_W);
    let y = (i32::from(c.h) - WIN_H).max(0);
    Rect::new(MARGIN, y, w as u16, WIN_H as u16)
}

/// The smallest window.
pub const MIN_SIZE: MinSize = MinSize::new(MIN_W as u16, MIN_H as u16);

/// `r` moved by `dx` columns.
fn shift(r: Rect, dx: i32) -> Rect {
    Rect::new(r.x + dx, r.y, r.w, r.h)
}

/// A moved window `r` (box space) kept clear of the open side panels
/// `left` / `right`: shifted (and narrowed, not under [`MIN_W`]) into the
/// columns between them; `None` when they leave less than that.
pub fn clear_of_panels(s: &Screen, r: Rect, (left, right): (bool, bool)) -> Option<Rect> {
    let lo = if left {
        s.left_end() + MARGIN
    } else {
        s.frame_left()
    };
    let hi = if right {
        s.w - s.left_end() - MARGIN
    } else {
        s.frame_right()
    };
    if hi - lo < MIN_W {
        return None;
    }
    let w = i32::from(r.w).min(hi - lo);
    Some(Rect::new(r.x.clamp(lo, hi - w), r.y, w as u16, r.h))
}

/// The body of the game's chat message for `text` said to every player
/// of the game (C→S 0x15: type 1, language 0, the text and its NUL, an
/// empty whisper target, no extra bytes). The text goes as the game's
/// 8-bit chat code page (Latin-1: the chat lines are read so); other
/// characters become `?`; at most 255 bytes.
pub fn game_chat_body(text: &str) -> Vec<u8> {
    let mut m = vec![0x15, 1, 0];
    m.extend(
        text.chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .filter(|&b| b != 0)
            .take(255),
    );
    m.extend([0, 0, 0]);
    m
}

/// What [`ChatWindow::event`] did with an event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Taken {
    /// Not the window's.
    No,
    /// The window's.
    Yes,
    /// The window's: a line for the game server (C→S 0x15).
    Send(Vec<u8>),
}

/// A press on the strip, the bar or an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Drag {
    /// Not a drag until the pointer moves [`DRAG_START`]: the zone, where
    /// the press was and the window then (box space), and the bar's tab
    /// under it (opened by a release without a drag).
    Pending {
        zone: Zone,
        start: Point,
        from: Rect,
        tab: Option<Tab>,
    },
    /// The layer holds the window; the pointer (box space) less `offset`
    /// is the layer's pointer (canvas space).
    Held { offset: Point },
}

/// The in-game chat window: the model, its source (the game's feed and
/// the realm's), and how the window shows.
pub struct ChatWindow {
    pub model: ChatModel,
    src: Merged,
    /// Shown (`ToggleChatWindow` hides it).
    pub shown: bool,
    /// Collapsed to the one-line bar.
    pub collapsed: bool,
    /// The input line has the typing.
    pub focused: bool,
    /// The mouse is over the window.
    pub hover: bool,
    /// The side panels open (left, right): the window keeps beside them.
    pub panels: (bool, bool),
    /// The window's menu is open.
    pub menu: bool,
    /// The window's rect (canvas space), its lock and its default spot.
    layer: FloatLayer,
    /// A press on the strip, the bar or an edge.
    drag: Option<Drag>,
    /// A saved layout for the next frame, which knows the screen.
    loaded: Option<SavedLayout>,
    /// The layout changed and the layer's own mark was spent.
    dirty: bool,
    /// The last input, line or hover (UI clock, ms).
    active_ms: u64,
    now_ms: u64,
    /// The tab boxes as last drawn (clicks, UI ids).
    boxes: Vec<TabBox>,
    /// The places as last drawn (none: not drawn, or hidden).
    drawn: Option<Places>,
    /// Lines for the game server not handed out yet.
    said: Vec<Vec<u8>>,
    /// The input line was just opened: the Enter that opened it (its
    /// typed character follows its action) is not a send. Until the next
    /// frame.
    fresh: bool,
}

impl ChatWindow {
    /// A window on the realm's chat `realm` (none: only the Game tab).
    pub fn new(realm: Option<Box<dyn ChatSource>>) -> ChatWindow {
        let c = canvas(&Screen::PLAY);
        ChatWindow {
            model: ChatModel::new(Host::Game),
            src: Merged::new(realm),
            shown: true,
            collapsed: false,
            focused: false,
            hover: false,
            panels: (false, false),
            menu: false,
            layer: FloatLayer::new(c, vec![FloatPanel::new(PANEL, default_rect(c), MIN_SIZE)]),
            drag: None,
            loaded: None,
            dirty: false,
            active_ms: 0,
            now_ms: 0,
            boxes: Vec::new(),
            drawn: None,
            said: Vec::new(),
            fresh: false,
        }
    }

    /// Whether the realm part is wired.
    pub fn has_realm(&self) -> bool {
        self.src.realm.is_some()
    }

    /// Wires the realm part (the realm session came after the window).
    pub fn set_realm(&mut self, realm: Box<dyn ChatSource>) {
        self.src.realm = Some(realm);
        self.model.hello();
    }

    /// A player of the game said `text` (a chat line of the game stream).
    pub fn player_line(&mut self, from: &str, text: &str) {
        self.src.game.player_line(from, text, self.now_ms);
    }

    /// A game or system message in the game's text colour.
    pub fn game_message(&mut self, text: &str, colour: u8) {
        self.src.game.message(text, colour, self.now_ms);
    }

    /// One frame at UI clock `now_ms`: what was queued goes out, what came
    /// in is applied (a new line wakes an idle window); the player sees
    /// the open tab while the window is shown and open.
    pub fn pump(&mut self, now_ms: u64) {
        self.now_ms = now_ms;
        self.model.set_seen(self.shown && !self.collapsed);
        if self.model.pump(&mut self.src, now_ms) {
            self.active_ms = now_ms;
        }
        for (_, text) in self.src.game.take_said() {
            self.said.push(game_chat_body(&text));
        }
        self.fresh = false;
    }

    /// The UI clock of the last frame (ms).
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// The UI clock of the last input, line or hover (ms).
    pub fn active_ms(&self) -> u64 {
        self.active_ms
    }

    /// Lines for the game server, oldest first.
    pub fn take_said(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.said)
    }

    /// Idle at `now_ms`: the backing turns light.
    pub fn idle(&self, now_ms: u64) -> bool {
        !self.focused
            && !self.hover
            && self.drag.is_none()
            && !self.menu
            && now_ms.saturating_sub(self.active_ms) >= IDLE_MS
    }

    fn wake(&mut self) {
        self.active_ms = self.now_ms;
    }

    /// Shows or hides the window (`ToggleChatWindow`).
    pub fn toggle(&mut self) {
        self.shown = !self.shown;
        self.focused = false;
        self.menu = false;
        self.drop_drag();
        self.wake();
    }

    /// `OpenChat`: shows and opens the window and focuses the input.
    pub fn open_input(&mut self) {
        self.shown = true;
        self.collapsed = false;
        self.fresh = !self.focused;
        self.focused = true;
        self.wake();
    }

    /// Collapses to the bar or opens it again.
    pub fn set_collapsed(&mut self, collapsed: bool) {
        self.collapsed = collapsed;
        if collapsed {
            self.focused = false;
            self.menu = false;
        }
        self.wake();
    }

    /// The position is locked: no drag, no resize.
    pub fn locked(&self) -> bool {
        self.layer.locked()
    }

    /// Locks or unlocks the position (the window's menu).
    pub fn set_locked(&mut self, locked: bool) {
        self.drop_drag();
        self.layer.set_locked(locked);
    }

    /// The window back at its default spot and size (the window's menu);
    /// the lock stays.
    pub fn reset_layout(&mut self) {
        self.drop_drag();
        self.layer.reset();
    }

    /// The player moved or resized the window (it is not at its default
    /// spot).
    pub fn moved(&self) -> bool {
        !self.layer.is_classic()
    }

    /// Puts back a saved layout (the host's `layouts.toml`), from the next
    /// event or draw on.
    pub fn apply_layout(&mut self, saved: SavedLayout) {
        self.drop_drag();
        self.loaded = Some(saved);
    }

    /// The layout to save when it changed (moved, resized, locked,
    /// unlocked or reset) since the last call.
    pub fn take_layout(&mut self) -> Option<SavedLayout> {
        let changed = self.layer.take_changed() | std::mem::take(&mut self.dirty);
        changed.then(|| self.layer.saved())
    }

    /// The layer on the canvas of screen `s` (a new view size: the window
    /// keeps its gaps to its nearest corner; at the default spot it stays
    /// there), and a saved layout waiting applied.
    fn fit(&mut self, s: &Screen) {
        let c = canvas(s);
        if c != self.layer.canvas() {
            let at_default = self.layer.is_classic();
            self.drop_drag();
            self.layer.set_canvas(c);
            self.layer.set_classic(PANEL, default_rect(c));
            if at_default {
                // Not a change to save: the default spot of this view.
                self.dirty |= self.layer.take_changed();
                self.layer.reset();
                self.layer.take_changed();
            }
        }
        if let Some(saved) = self.loaded.take() {
            self.dirty |= self.layer.take_changed();
            self.layer.apply(&saved);
        }
    }

    /// A press or a drag dropped, the window where it was before the drag.
    fn drop_drag(&mut self) {
        if matches!(self.drag.take(), Some(Drag::Held { .. })) {
            self.layer.cancel();
        }
    }

    /// The moved window's rect on screen `s` (box space) before it keeps
    /// clear of the side panels; `None` at the default spot.
    fn moved_rect(&self, s: &Screen) -> Option<Rect> {
        if self.layer.is_classic() {
            return None;
        }
        let r = self.layer.rect(PANEL)?;
        let (was, c) = (self.layer.canvas(), canvas(s));
        let r = if was == c {
            r
        } else {
            geom::at_least(Anchored::of(r, was).place(c), MIN_SIZE, c)
        };
        Some(shift(r, s.frame_left()))
    }

    /// Where the window is: at its default spot beside the open side
    /// panels, or where the player moved it (shifted clear of the open
    /// side panels; as it is while dragged); with no room between the
    /// panels only while typing, over them; nowhere when hidden.
    pub fn places(&self, s: &Screen) -> Option<Places> {
        if !self.shown {
            return None;
        }
        let (l, r) = self.panels;
        let Some(moved) = self.moved_rect(s) else {
            return Places::beside(s, l, r).or_else(|| {
                self.focused
                    .then(|| Places::of(s, s.frame_left() + MARGIN, s.frame_right() - MARGIN))
            });
        };
        if matches!(self.drag, Some(Drag::Held { .. })) {
            return Some(Places::at(moved));
        }
        clear_of_panels(s, moved, self.panels)
            .or_else(|| self.focused.then_some(moved))
            .map(Places::at)
    }

    /// Where the window takes pointer events: the whole frame while the
    /// input has the typing, the menu is open or a press on the window
    /// holds (a press elsewhere ends the typing or the menu), else the
    /// window or the bar; nothing when hidden.
    pub fn rect(&self, s: &Screen) -> Rect {
        let Some(p) = self.places(s) else {
            return Rect::new(0, 0, 0, 0);
        };
        if self.focused || self.menu || self.drag.is_some() {
            s.frame_rect()
        } else if self.collapsed {
            p.bar
        } else {
            p.window
        }
    }

    /// The mouse is at `p` (box space): over the window, it stays awake.
    pub fn mouse_at(&mut self, s: &Screen, p: Point) {
        let over = self.places(s).is_some_and(|pl| self.over(&pl, p));
        if over {
            self.wake();
        }
        self.hover = over;
    }

    /// One UI event (box space).
    pub fn event(&mut self, e: UiEvent, s: &Screen, fonts: Option<&FontMeasure>) -> Taken {
        if let UiEvent::Action(a) = e {
            return self.action(a);
        }
        self.fit(s);
        if self.drag.is_some() {
            match e {
                UiEvent::CursorMoved(at) => {
                    self.drag_to(s, at);
                    return Taken::Yes;
                }
                UiEvent::Release {
                    button: PointerButton::Left,
                    at,
                } => {
                    self.release(s, at);
                    return Taken::Yes;
                }
                // The release was lost (the pointer left the window):
                // the window lands where the pointer is.
                UiEvent::Press { at, .. } => self.release(s, at),
                _ => {}
            }
        }
        let Some(pl) = self.places(s) else {
            return Taken::No;
        };
        match e {
            UiEvent::Char(c) if self.focused => self.typed(c),
            UiEvent::Char(_) => Taken::No,
            UiEvent::Press { button, at } => self.press(button, at, &pl, fonts),
            UiEvent::Release { at, .. } => {
                if self.over(&pl, at) {
                    Taken::Yes
                } else {
                    Taken::No
                }
            }
            UiEvent::Wheel { steps, at } if !self.collapsed && pl.window.contains(at) => {
                self.model.scroll_by(steps * 3);
                self.wake();
                Taken::Yes
            }
            UiEvent::CursorMoved(at) => {
                self.mouse_at(s, at);
                Taken::No
            }
            UiEvent::CursorLeft => {
                self.hover = false;
                Taken::No
            }
            _ => Taken::No,
        }
    }

    fn over(&self, pl: &Places, at: Point) -> bool {
        if self.collapsed {
            pl.bar.contains(at)
        } else {
            pl.window.contains(at)
        }
    }

    /// The pointer moved to `at` (box space) with a press on the window:
    /// a pending press past [`DRAG_START`] takes the window, a held one
    /// follows.
    fn drag_to(&mut self, s: &Screen, at: Point) {
        self.wake();
        let fl = s.frame_left();
        match self.drag {
            Some(Drag::Pending {
                zone, start, from, ..
            }) => {
                if (at.x - start.x).abs() < DRAG_START && (at.y - start.y).abs() < DRAG_START {
                    return;
                }
                let from = shift(from, -fl);
                if self.layer.rect(PANEL) != Some(from) {
                    // Taken where it shows (beside a side panel, or the
                    // default spot's place for this view).
                    self.dirty |= self.layer.take_changed();
                    self.layer.apply(&SavedLayout {
                        locked: self.layer.locked(),
                        panels: vec![(PANEL.to_owned(), Anchored::of(from, self.layer.canvas()))],
                    });
                }
                let start = shift_point(start, -fl);
                // The strip and the bar move the window by its title:
                // the layer takes the press in the title's middle.
                let take = match zone {
                    Zone::Title => {
                        Point::new(from.x + i32::from(from.w) / 2, from.y + grab::EDGE + 2)
                    }
                    Zone::Resize(_) => start,
                };
                if self.layer.press(take) {
                    let offset = Point::new(start.x - take.x + fl, start.y - take.y);
                    self.drag = Some(Drag::Held { offset });
                    self.drag_to(s, at);
                } else {
                    self.drag = None;
                }
            }
            Some(Drag::Held { offset }) => {
                self.layer
                    .drag(Point::new(at.x - offset.x, at.y - offset.y));
            }
            None => {}
        }
    }

    /// The press ends at `at` (box space): a held window lands there; a
    /// press that never became a drag is a click (a tab of the bar opens).
    fn release(&mut self, s: &Screen, at: Point) {
        self.drag_to(s, at);
        match self.drag.take() {
            Some(Drag::Held { offset }) => {
                self.layer
                    .release(Point::new(at.x - offset.x, at.y - offset.y));
            }
            Some(Drag::Pending { tab: Some(t), .. }) => {
                self.set_collapsed(false);
                self.model.open_tab(t);
            }
            _ => {}
        }
    }

    fn action(&mut self, a: ActionId) -> Taken {
        let is = |x: Action| usize::from(a.0) == x.index();
        if is(Action::ToggleChatWindow) {
            self.toggle();
            Taken::Yes
        } else if is(Action::OpenChat) {
            self.open_input();
            Taken::Yes
        } else {
            Taken::No
        }
    }

    fn typed(&mut self, c: u16) -> Taken {
        self.wake();
        match c {
            ENTER if std::mem::take(&mut self.fresh) => Taken::Yes,
            ENTER => {
                self.focused = false;
                self.model.submit();
                self.pump(self.now_ms);
                match self.said.is_empty() {
                    true => Taken::Yes,
                    false => Taken::Send(self.said.remove(0)),
                }
            }
            ESC => {
                self.focused = false;
                self.model.input.clear();
                Taken::Yes
            }
            BACKSPACE => {
                self.model.input.pop();
                Taken::Yes
            }
            UP => {
                self.model.history_prev();
                Taken::Yes
            }
            DOWN => {
                self.model.history_next();
                Taken::Yes
            }
            c => {
                if let Some(ch) = char::from_u32(u32::from(c)).filter(|ch| !ch.is_control()) {
                    if self.model.input.chars().count() < MAX_INPUT {
                        self.model.input.push(ch);
                    }
                }
                Taken::Yes
            }
        }
    }

    /// A press on the menu's row `at`: lock or unlock, or reset; `false`
    /// off its rows.
    fn menu_press(&mut self, pl: &Places, at: Point) -> bool {
        let [lock, reset] = pl.menu_rows();
        if lock.contains(at) {
            self.set_locked(!self.locked());
        } else if reset.contains(at) {
            self.reset_layout();
        } else {
            return false;
        }
        true
    }

    fn press(
        &mut self,
        button: PointerButton,
        at: Point,
        pl: &Places,
        fonts: Option<&FontMeasure>,
    ) -> Taken {
        if std::mem::take(&mut self.menu) {
            if button == PointerButton::Left && self.menu_press(pl, at) {
                self.wake();
                return Taken::Yes;
            }
            if pl.menu.contains(at) {
                // The menu box shuts its menu.
                return Taken::Yes;
            }
        }
        if !self.over(pl, at) {
            // A press elsewhere ends the typing; the press is the game's.
            self.focused = false;
            return Taken::No;
        }
        self.wake();
        if button != PointerButton::Left {
            return Taken::Yes;
        }
        let free = !self.locked();
        let title = |tab| Drag::Pending {
            zone: Zone::Title,
            start: at,
            from: pl.window,
            tab,
        };
        if self.collapsed {
            if pl.expand.contains(at) {
                self.set_collapsed(false);
                return Taken::Yes;
            }
            let tab = self.boxes.iter().find(|b| b.at.contains(at)).map(|b| b.tab);
            if free {
                // The bar drags the window; a tab opens on a release
                // without a drag.
                self.drag = Some(title(tab));
            } else if let Some(t) = tab {
                self.set_collapsed(false);
                self.model.open_tab(t);
            }
            return Taken::Yes;
        }
        if pl.collapse.contains(at) {
            self.set_collapsed(true);
            return Taken::Yes;
        }
        if pl.menu.contains(at) {
            self.menu = true;
            return Taken::Yes;
        }
        if free {
            if let Some(Zone::Resize(e)) = grab::zone_at(pl.window, at, true) {
                self.drag = Some(Drag::Pending {
                    zone: Zone::Resize(e),
                    start: at,
                    from: pl.window,
                    tab: None,
                });
                return Taken::Yes;
            }
        }
        if let Some(b) = self.boxes.iter().find(|b| b.at.contains(at)) {
            let t = b.tab;
            self.model.open_tab(t);
            if free {
                self.drag = Some(title(None));
            }
            return Taken::Yes;
        }
        if free && at.y - pl.window.y < grab::TITLE_H {
            self.drag = Some(title(None));
            return Taken::Yes;
        }
        let paint = Measure(fonts);
        match chat::hit(&self.model, &pl.layout, &paint, at) {
            Some(Hit::Input) => self.focused = true,
            Some(h) => {
                chat::click(&mut self.model, &h);
                if matches!(h, Hit::User(_)) {
                    self.focused = true;
                }
            }
            None => {}
        }
        Taken::Yes
    }

    /// Draws the window (or the bar) at UI clock `now_ms`.
    pub fn draw(
        &mut self,
        s: &Screen,
        fonts: Option<&FontMeasure>,
        gold: Option<u8>,
        now_ms: u64,
        out: &mut dyn UiDrawSink,
    ) {
        self.fit(s);
        self.drawn = self.places(s);
        let Some(pl) = self.drawn else {
            self.boxes.clear();
            return;
        };
        let mode = if self.idle(now_ms) {
            IDLE_MODE
        } else {
            ACTIVE_MODE
        };
        let mut p = GamePaint {
            fonts,
            out,
            clip: s.frame_rect(),
            gold,
            mode,
        };
        if self.collapsed {
            p.fill(pl.bar);
            p.outline(pl.bar);
            self.boxes = tabs::boxes(&self.model, pl.bar_tabs, &p);
            tabs::draw(&self.model, pl.bar_tabs, &mut p);
            draw_box(&mut p, pl.expand, "+");
            return;
        }
        p.fill(pl.window);
        p.outline(pl.window);
        self.boxes = tabs::boxes(&self.model, pl.layout.tabs, &p);
        draw_box(&mut p, pl.menu, "=");
        draw_box(&mut p, pl.collapse, "-");
        let row = pl.layout.input;
        p.outline(Rect::new(row.x - 2, row.y - 1, row.w + 4, row.h + 2));
        chat::draw(&self.model, &pl.layout, &mut p, self.focused, now_ms, None);
        if self.menu {
            let rows = pl.menu_rows();
            let all = Rect::new(rows[0].x, rows[0].y, rows[0].w, (2 * MENU_ROW) as u16);
            // Over the lines: opaque whatever the idle state.
            p.mode = ACTIVE_MODE;
            p.fill(all);
            p.fill(all);
            p.outline(all);
            for (r, label) in rows.into_iter().zip(self.menu_labels()) {
                p.text(
                    ChatFont::Text,
                    r.x + 5,
                    r.y + (i32::from(r.h) + LINE_ROW) / 2 - 2,
                    chat::colour::GOLD,
                    label,
                );
            }
        }
    }

    /// The places as last drawn (none: hidden, or not drawn yet).
    pub fn drawn(&self) -> Option<Places> {
        self.drawn
    }

    /// The menu's rows as they read now.
    pub fn menu_labels(&self) -> [&'static str; 2] {
        let lock = if self.locked() {
            "Unlock position"
        } else {
            "Lock position"
        };
        [lock, "Reset position"]
    }

    /// The window's parts as UI elements (`chat_window.tab(game)`,
    /// `chat_window.collapse`, `chat_window.menu`, `chat_window.lock`,
    /// `chat_window.reset`, `chat_window.expand`, `chat_window.input`), as
    /// last drawn.
    pub fn ui_elements(&self, s: &Screen) -> Vec<UiElement> {
        let Some(pl) = self.places(s) else {
            return Vec::new();
        };
        let mut v: Vec<UiElement> = self
            .boxes
            .iter()
            .map(|b| UiElement::new(format!("chat_window.tab({})", tab_name(b.tab)), b.at))
            .collect();
        if self.collapsed {
            v.push(UiElement::new("chat_window.expand", pl.expand));
        } else {
            v.push(UiElement::new("chat_window.collapse", pl.collapse));
            v.push(UiElement::new("chat_window.menu", pl.menu));
            v.push(UiElement::new("chat_window.input", pl.layout.input));
            if self.menu {
                let [lock, reset] = pl.menu_rows();
                v.push(UiElement::new("chat_window.lock", lock));
                v.push(UiElement::new("chat_window.reset", reset));
            }
        }
        v
    }
}

fn shift_point(p: Point, dx: i32) -> Point {
    Point::new(p.x + dx, p.y)
}

/// The UTF-16 units a key press types into the input line: Enter, Esc,
/// Backspace and the Up / Down arrows as their control units, a
/// character key as its text; nothing for a release or another key.
pub fn typed_units(k: &KeyboardInput) -> Vec<u16> {
    if k.state != ButtonState::Pressed {
        return Vec::new();
    }
    match k.key_code {
        KeyCode::Enter | KeyCode::NumpadEnter => vec![ENTER],
        KeyCode::Escape => vec![ESC],
        KeyCode::Backspace => vec![BACKSPACE],
        KeyCode::ArrowUp => vec![UP],
        KeyCode::ArrowDown => vec![DOWN],
        _ => match &k.logical_key {
            Key::Character(_) | Key::Space => k
                .text
                .as_deref()
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_control())
                .flat_map(|c| {
                    let mut b = [0; 2];
                    c.encode_utf16(&mut b).to_vec()
                })
                .collect(),
            _ => Vec::new(),
        },
    }
}

/// A tab's id name: `game`, `global`, `mod`, `guild`, `dm`.
pub fn tab_name(t: Tab) -> String {
    match t {
        Tab::Room(r) => format!("{r:?}").to_lowercase(),
        Tab::Dm => "dm".into(),
    }
}

fn draw_box(p: &mut GamePaint<'_>, r: Rect, label: &str) {
    p.outline(r);
    let w = p.width(ChatFont::Text, label);
    let x = r.x + (i32::from(r.w) - w) / 2;
    let y = r.y + (i32::from(r.h) + LINE_ROW) / 2 - 2;
    p.text(ChatFont::Text, x, y, chat::colour::GOLD, label);
}

fn font_id(f: ChatFont) -> u16 {
    match f {
        ChatFont::Text => LINE_FONT,
        ChatFont::Small => SMALL_FONT,
    }
}

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn width(fonts: Option<&FontMeasure>, f: ChatFont, s: &str) -> i32 {
    let t = units(s);
    fonts
        .and_then(|m| m.width_a(font_id(f), &t))
        .unwrap_or(t.len() as i32 * if f == ChatFont::Small { 5 } else { 7 })
}

fn row_height(f: ChatFont) -> i32 {
    match f {
        ChatFont::Text => LINE_ROW,
        ChatFont::Small => SMALL_ROW,
    }
}

/// Measures only (hits).
struct Measure<'a>(Option<&'a FontMeasure>);

impl ChatPaint for Measure<'_> {
    fn width(&self, f: ChatFont, text: &str) -> i32 {
        width(self.0, f, text)
    }
    fn row_height(&self, f: ChatFont) -> i32 {
        row_height(f)
    }
    fn text(&mut self, _: ChatFont, _: i32, _: i32, _: u8, _: &str) {}
    fn panel(&mut self, _: Rect) {}
    fn outline(&mut self, _: Rect) {}
}

/// The game UI's draws as a chat paint: text in the line and small fonts,
/// boxes as translucent black rectangles, outlines as gold lines.
struct GamePaint<'a> {
    fonts: Option<&'a FontMeasure>,
    out: &'a mut dyn UiDrawSink,
    clip: Rect,
    /// The gold of the outlines (none before the palette is known: no
    /// outlines).
    gold: Option<u8>,
    /// The rectangles' draw mode ([`ACTIVE_MODE`] or [`IDLE_MODE`]).
    mode: u8,
}

impl GamePaint<'_> {
    fn fill(&mut self, r: Rect) {
        self.out.push(UiDraw::Rect(RectRequest::sized(
            r.x,
            r.y,
            i32::from(r.w),
            i32::from(r.h),
            BLACK,
            self.mode,
        )));
    }
}

impl ChatPaint for GamePaint<'_> {
    fn width(&self, f: ChatFont, text: &str) -> i32 {
        width(self.fonts, f, text)
    }

    fn row_height(&self, f: ChatFont) -> i32 {
        row_height(f)
    }

    fn text(&mut self, f: ChatFont, x: i32, y: i32, colour: u8, text: &str) {
        if text.is_empty() {
            return;
        }
        self.out.push(UiDraw::Text(TextRequest {
            text: units(text),
            at: Point::new(x, y),
            style: TextStyle {
                font: font_id(f),
                color: u16::from(colour),
            },
            opts: TextOpts::default(),
            clip: self.clip,
        }));
    }

    fn panel(&mut self, r: Rect) {
        // The window's own backing is under it: a box only darkens it
        // while in use.
        if self.mode == ACTIVE_MODE {
            self.fill(r);
        }
    }

    fn outline(&mut self, r: Rect) {
        let Some(color) = self.gold else { return };
        let (x0, y0) = (r.x, r.y);
        let (x1, y1) = (
            r.x + i32::from(r.w).max(1) - 1,
            r.y + i32::from(r.h).max(1) - 1,
        );
        if r.w <= 1 {
            self.out.push(UiDraw::Line(LineRequest {
                x0,
                y0,
                x1: x0,
                y1,
                color,
            }));
            return;
        }
        for (a, b, c, d) in [
            (x0, y0, x1, y0),
            (x1, y0, x1, y1),
            (x1, y1, x0, y1),
            (x0, y1, x0, y0),
        ] {
            self.out.push(UiDraw::Line(LineRequest {
                x0: a,
                y0: b,
                x1: c,
                y1: d,
                color,
            }));
        }
    }
}

#[cfg(test)]
#[path = "chat_window_tests.rs"]
mod tests;
