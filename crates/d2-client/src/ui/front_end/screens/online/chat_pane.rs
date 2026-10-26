//! The lobby's chat: the chat module (`crate::chat`, `crate::ui::chat`) in
//! the chat pane of `realmbckg`: the tab strip above the art's left frame,
//! the lines and the "In chat" column inside it, the input line in the
//! art's box under it ([`LAYOUT`]); a moved or resized chat lays out the
//! same way in its own rect (`super::layout`). Text in Font16 (lines, tabs) and Font8 (the
//! column, badges, hints) in the game's text colours.
//!
//! The source is the realm over the lobby's link ([`RealmChat::lobby`]):
//! the lobby hands it its link events ([`ChatPane::feed`]); tests and
//! captures put a [`crate::chat::FakeChat`] in instead.

use crate::chat::{ChatModel, ChatSource, Host, RealmChat};
use crate::ui::chat::tabs::TabBox;
use crate::ui::chat::{self, ChatFont, ChatLayout, ChatPaint, Hit};
use crate::ui::front_end::DrawItem;
use crate::ui::geom::{Point, Rect};
use crate::ui::ui_ids::UiElement;

use super::link::Event;
use super::widgets::{FONT16, FONT8};
use super::OnlineHandle;

/// The pane in the lobby's 800 × 600 (the art's left frame and input box):
/// the classic place.
pub const LAYOUT: ChatLayout = ChatLayout {
    tabs: Rect::new(50, 160, 350, 18),
    body: Rect::new(68, 186, 326, 264),
    column_w: 112,
    input: Rect::new(122, 480, 270, 20),
};

/// Rows: Font16 lines, Font8 column rows.
const TEXT_ROW: i32 = 16;
const SMALL_ROW: i32 = 10;

enum Source {
    Realm(RealmChat),
    Other(Box<dyn ChatSource>),
}

pub struct ChatPane {
    pub model: ChatModel,
    source: Source,
    /// The character the realm was greeted with.
    me: Option<String>,
    /// The tab boxes as last drawn (clicks and UI elements).
    boxes: Vec<TabBox>,
    now_ms: u64,
    /// Where the pane is ([`LAYOUT`] unless the chat was moved).
    layout: ChatLayout,
    /// The pointer (a shortened tab's tip).
    pointer: Option<Point>,
}

/// The front end's items as a chat paint.
struct FrontPaint<'a> {
    adv: &'a dyn Fn(u16, &[u16]) -> i32,
    out: Vec<DrawItem>,
}

fn font(f: ChatFont) -> u16 {
    match f {
        ChatFont::Text => FONT16,
        ChatFont::Small => FONT8,
    }
}

impl ChatPaint for FrontPaint<'_> {
    fn width(&self, f: ChatFont, text: &str) -> i32 {
        (self.adv)(font(f), &text.encode_utf16().collect::<Vec<u16>>())
    }

    fn row_height(&self, f: ChatFont) -> i32 {
        match f {
            ChatFont::Text => TEXT_ROW,
            ChatFont::Small => SMALL_ROW,
        }
    }

    fn text(&mut self, f: ChatFont, x: i32, y: i32, colour: u8, text: &str) {
        if text.is_empty() {
            return;
        }
        self.out.push(DrawItem::Text {
            string_id: 0,
            text: text.to_string(),
            font: font(f),
            at: Point::new(x, y),
            label: None,
            color: i32::from(colour),
            boxed: None,
        });
    }

    fn panel(&mut self, r: Rect) {
        self.out.push(DrawItem::Rect {
            at: r.origin(),
            w: i32::from(r.w),
            h: i32::from(r.h),
        });
    }

    fn outline(&mut self, r: Rect) {
        self.out.push(DrawItem::Border {
            at: r.origin(),
            w: i32::from(r.w),
            h: i32::from(r.h),
        });
    }

    /// The round red badge of the approved preview, the count in white.
    fn badge(&mut self, r: Rect, count: &str) {
        self.out.push(DrawItem::Pill {
            at: r.origin(),
            w: i32::from(r.w),
            h: i32::from(r.h),
            rgb: BADGE_RED,
            rim: BADGE_RIM,
        });
        let x = r.x + (i32::from(r.w) - self.width(ChatFont::Small, count) + 1) / 2;
        self.text(
            ChatFont::Small,
            x,
            r.y + i32::from(r.h) - 1,
            chat::colour::WHITE,
            count,
        );
    }
}

/// The unread badge's red and its darker rim.
const BADGE_RED: [u8; 3] = [170, 32, 24];
const BADGE_RIM: [u8; 3] = [90, 14, 10];

impl ChatPane {
    /// The realm's chat over the lobby's link.
    pub fn new(online: OnlineHandle) -> ChatPane {
        ChatPane::with(Source::Realm(RealmChat::lobby(online)))
    }

    /// Another source (tests, captures).
    pub fn with_source(src: Box<dyn ChatSource>) -> ChatPane {
        ChatPane::with(Source::Other(src))
    }

    fn with(source: Source) -> ChatPane {
        ChatPane {
            model: ChatModel::new(Host::Lobby),
            source,
            me: None,
            boxes: Vec::new(),
            now_ms: 0,
            layout: LAYOUT,
            pointer: None,
        }
    }

    /// A fresh entry into the lobby with `character`: the typed line goes;
    /// another character greets the realm again (its rooms and guild).
    pub fn enter(&mut self, character: Option<&str>) {
        self.model.input.clear();
        self.model.scroll = 0;
        let name = character.map(str::to_string);
        if self.me.is_some() && self.me != name {
            self.model.hello();
        }
        if self.me != name {
            if let Some(n) = &name {
                self.model.note(format!("You are in the lobby as {n}."));
            }
        }
        self.model.set_me(name.as_deref());
        self.me = name;
    }

    /// The pointer moved (`None`: the pane doesn't have it).
    pub fn hover(&mut self, p: Option<Point>) {
        self.pointer = p;
    }

    /// One of the lobby link's events (its chat lines).
    pub fn feed(&mut self, ev: &Event) {
        if let Source::Realm(r) = &mut self.source {
            r.feed_event(ev);
        }
    }

    /// Sends and receives; `true` when something arrived.
    pub fn pump(&mut self, now_ms: u64) -> bool {
        self.now_ms = now_ms;
        let src: &mut dyn ChatSource = match &mut self.source {
            Source::Realm(r) => r,
            Source::Other(o) => o.as_mut(),
        };
        self.model.pump(src, now_ms)
    }

    pub fn has_input(&self) -> bool {
        !self.model.input.trim().is_empty()
    }

    /// Types a character (the line takes up to 200).
    pub fn type_char(&mut self, c: char) {
        if !c.is_control() && self.model.input.chars().count() < 200 {
            self.model.input.push(c);
        }
    }

    pub fn backspace(&mut self) {
        self.model.input.pop();
    }

    /// Enter: sends the typed line.
    pub fn submit(&mut self) {
        self.model.submit();
        self.pump(self.now_ms);
    }

    /// The pane moved or was resized.
    pub fn set_layout(&mut self, layout: ChatLayout) {
        self.layout = layout;
    }

    /// `p` is on a tab as last drawn.
    pub fn on_tab(&self, p: Point) -> bool {
        self.boxes.iter().any(|b| b.at.contains(p))
    }

    /// A press at `p`: `true` when it was the chat's.
    pub fn click(&mut self, p: Point) -> bool {
        if let Some(b) = self.boxes.iter().find(|b| b.at.contains(p)) {
            let h = Hit::Tab(b.tab);
            chat::click(&mut self.model, &h);
            self.pump(self.now_ms);
            return true;
        }
        if self.layout.tabs.contains(p) {
            return false;
        }
        let paint = FrontPaint {
            adv: &|_, _| 0,
            out: Vec::new(),
        };
        match chat::hit(&self.model, &self.layout, &paint, p) {
            Some(h) => {
                chat::click(&mut self.model, &h);
                self.pump(self.now_ms);
                true
            }
            None => false,
        }
    }

    /// The pane's items; `off` replaces the input line.
    pub fn draw(
        &mut self,
        adv: &dyn Fn(u16, &[u16]) -> i32,
        now_ms: u64,
        focused: bool,
        off: Option<&str>,
    ) -> Vec<DrawItem> {
        let mut p = FrontPaint {
            adv,
            out: Vec::new(),
        };
        self.boxes = chat::tabs::boxes(&self.model, self.layout.tabs, &p);
        chat::draw(&self.model, &self.layout, &mut p, focused, now_ms, off);
        // A shortened tab's full label under it while the pointer is on it.
        if let Some((b, tip)) = self
            .pointer
            .and_then(|at| chat::tabs::tip_at(&self.boxes, at))
        {
            let w = p.width(ChatFont::Small, tip) + 8;
            let strip = self.layout.tabs;
            let x = b.at.x.min(strip.x + i32::from(strip.w) - w);
            let r = Rect::new(
                x,
                b.at.y + i32::from(b.at.h) + 2,
                w as u16,
                SMALL_ROW as u16 + 4,
            );
            p.panel(r);
            p.outline(r);
            p.text(
                ChatFont::Small,
                r.x + 4,
                r.y + i32::from(r.h) - 2,
                chat::colour::GOLD,
                tip,
            );
        }
        p.out
    }

    /// The parts of the pane its texts stay in (`text_fit`): the tab
    /// strip, the lines, the column (when the open tab has one), the input
    /// line.
    pub fn text_rooms(&self) -> Vec<(&'static str, Rect)> {
        let l = self.layout;
        let (lines, column) = l.split(&self.model);
        let mut v = vec![("chat_tabs", l.tabs), ("chat_lines", lines)];
        v.extend(column.map(|c| ("chat_column", c)));
        v.push(("chat_input", l.input));
        v
    }

    /// The tabs as UI elements (`online_lobby.chat_tab(mod)`), as last drawn.
    pub fn ui_elements(&self) -> Vec<UiElement> {
        self.boxes
            .iter()
            .map(|b| {
                let name = match b.tab {
                    crate::chat::Tab::Room(r) => format!("{r:?}").to_lowercase(),
                    crate::chat::Tab::Dm => "dm".into(),
                };
                UiElement::new(format!("online_lobby.chat_tab({name})"), b.at)
            })
            .collect()
    }
}
