// D2RUST's own: the market window over the lobby.
//! The market window ([`crate::ui::market`], plan `market-ui.md` K2)
//! hosted by the lobby (plan `lobby-ui.md` L18): the lobby's MARKET button
//! and the market key (Ctrl+I) open it over the lobby, on the selected
//! character's economy.
//!
//! - **The market** is the link's ([`OnlineLink::market`]): on a realm the
//!   realm's own (`market::realm::RealmSource` on the lobby link's
//!   queues: the link stays the one reader of the session); the fake
//!   market ([`MarketParts::fake`], `D2RUST_MARKET=fake`) for tests and
//!   shots. Each fresh lobby entry starts it over and asks the realm's
//!   state; the button is greyed with the reason until the market is open.
//! - **Browse and buy only:** selling and claiming stay at the stash in a
//!   game (market owner decision 1). My shop and Deliveries show what they
//!   hold, the window's own Claim says "Claim at your stash", and a line
//!   under those tabs says where selling and claiming happen.
//! - **Drawing:** the window draws as in a game (the stash kit's colours,
//!   the game's fonts); its rectangles, lines and text become the front
//!   end's own items ([`front_items`]) on the lobby's palette. What it
//!   needs from the game's files (the palette, the fonts, the item tips)
//!   comes from the host's [`PartsLoader`] once.
//! - **Input:** while it is open it takes the mouse, the wheel, typing and
//!   Esc (Esc closes a box or the window); BACK closes it; the lobby keeps
//!   its state underneath. Its parts are the game's UI ids (`market.tab(Browse)`, ...).
//! - Market notices (a sale, an offer) go to the lobby's chat.

use std::rc::Rc;

use crate::market::fake::FakeSource;
use crate::market::source::MarketSource;
use crate::ui::draw::{LineRequest, UiDraw};
use crate::ui::front_end::DrawItem;
use crate::ui::geom::{Point, Rect};
use crate::ui::item_tip::{draw_tip_at, ItemTips};
use crate::ui::layout::Screen;
use crate::ui::market::window::{self, DrawIn, Pen, Places};
use crate::ui::market::{Availability, MarketKey, MarketTab, MarketUi};
use crate::ui::original::FontMeasure;
use crate::ui::panel::{ActionId, PointerButton, UiEvent};
use crate::ui::shared_stash::Kit;
use crate::ui::ui_ids::UiElement;

use super::OnlineHandle;

/// The line under My shop and Deliveries in the lobby.
pub const STASH_NOTE: &str =
    "In the lobby you browse and buy. Sell and claim at your stash in a game.";
/// The MARKET button's tip while the market opens.
pub const OPEN_TIP: &str = "Open the market (Ctrl+I)";
/// The tip while the realm's market has not answered yet.
pub const ASKING_TIP: &str = "Asking the realm's market...";
/// The tip without a market on the link.
pub const NO_MARKET_TIP: &str = "No market on this server";

/// What the window needs from the game's files.
#[derive(Clone, Default)]
pub struct MarketParts {
    /// The lobby's palette (the menus' sky palette), for the kit's colours.
    pub palette: Option<Vec<[u8; 3]>>,
    /// The fonts' widths (6 and 1).
    pub fonts: Option<FontMeasure>,
    /// The item tips: the listings' names and stats.
    pub tips: Option<ItemTips>,
    /// The in-process fake market instead of the link's.
    pub fake: bool,
}

/// The host's way to load [`MarketParts`] (once, when the lobby first
/// starts its market).
pub type PartsLoader = Rc<dyn Fn() -> MarketParts>;

/// The parts from the user's files: the menus' sky palette, fonts 6 and 1,
/// and the item tips (a part that doesn't load is left out).
pub fn parts_of(live: &d2_server::game::LiveData) -> MarketParts {
    let source = live.archives.source();
    let palette = crate::assets::path::read_palette(&*source, crate::ui::front_end::SKY_PALETTE[0])
        .and_then(Result::ok)
        .map(|p| p.colors.iter().map(|c| [c.r, c.g, c.b]).collect());
    let fonts = FontMeasure::load(&*source, &[window::SMALL, window::LABEL]).ok();
    let tips = live
        .tables
        .item_tables()
        .map_err(|e| e.to_string())
        .and_then(|t| crate::app::items::item_tips(live.archives.as_ref(), t))
        .ok();
    MarketParts {
        palette,
        fonts,
        tips,
        fake: false,
    }
}

/// [`parts_of`] once, the first time the lobby asks.
pub fn loader(live: std::sync::Arc<d2_server::game::LiveData>) -> PartsLoader {
    let once = std::cell::OnceCell::new();
    Rc::new(move || once.get_or_init(|| parts_of(&live)).clone())
}

/// The window's place on the lobby's 800 × 600 canvas.
pub fn places() -> Places {
    Places::of(&Screen::R800)
}

/// The market window over the lobby.
pub struct MarketHost {
    pub ui: MarketUi,
    source: Option<Box<dyn MarketSource>>,
    parts: Option<MarketParts>,
    mouse: Point,
    /// Ctrl is held.
    ctrl: bool,
}

impl Default for MarketHost {
    fn default() -> Self {
        Self::new()
    }
}

impl MarketHost {
    pub fn new() -> Self {
        MarketHost {
            ui: MarketUi::new(),
            source: None,
            parts: None,
            mouse: Point::new(0, 0),
            ctrl: false,
        }
    }

    /// A fresh lobby entry: a new market on `online`'s link (the fake one
    /// when the parts say so), which asks the realm's state.
    pub fn start(&mut self, online: &OnlineHandle) {
        if self.parts.is_none() {
            let loader = online.borrow().market_parts.clone();
            let mut parts = loader.map(|l| l()).unwrap_or_default();
            parts.fake |= crate::app::market::fake_wanted();
            self.parts = Some(parts);
        }
        let parts = self.parts.clone().unwrap_or_default();
        self.source = if parts.fake {
            let tips = parts.tips.as_ref();
            let names = |c: &str| tips.and_then(|t| t.base_name(c));
            let mut f = FakeSource::new(&names, false, 0, true);
            f.seed_shop(&names);
            Some(Box::new(f))
        } else {
            online.borrow_mut().link.market(parts.tips.clone())
        };
        let keep = std::mem::take(&mut self.ui);
        self.ui = MarketUi::new();
        // What the player set stays: the default filters, views and sizes.
        self.ui.default_filters = keep.default_filters;
        self.ui.view = keep.view;
        self.ui.grid_size = keep.grid_size;
        self.ui.table_size = keep.table_size;
        self.ui.key = keep.key;
        if let Some(t) = &parts.tips {
            self.ui.set_names(crate::ui::market::tip::names(t));
        }
        self.ctrl = false;
        if self.source.is_some() {
            self.ui.set_online(true);
        }
    }

    /// The window is open over the lobby.
    pub fn shown(&self) -> bool {
        self.ui.shown()
    }

    /// The market can open now.
    pub fn can_open(&self) -> bool {
        self.source.is_some() && self.ui.availability() == Availability::Open
    }

    /// The MARKET button's tip: how it opens, or why it can't.
    pub fn tip(&self) -> &'static str {
        if self.source.is_none() {
            return NO_MARKET_TIP;
        }
        match (self.ui.availability(), &self.ui.state) {
            (Availability::Open, _) => OPEN_TIP,
            (_, None) => ASKING_TIP,
            (a, Some(_)) => a.why_not().unwrap_or(OPEN_TIP),
        }
    }

    /// The MARKET button or the market key: opens or closes the window
    /// when the market is open. Whether it changed.
    pub fn toggle(&mut self) -> bool {
        if !self.can_open() {
            return false;
        }
        self.ui.toggle().is_ok()
    }

    pub fn close(&mut self) {
        self.ui.close();
    }

    /// The window's asks out and the market's answers in: the lobby's
    /// tick. The market's lines for the chat (notices, "closed").
    pub fn tick(&mut self) -> Vec<String> {
        if let Some(src) = self.source.as_deref_mut() {
            self.ui.pump(src);
        }
        let tips = self.parts.as_ref().and_then(|p| p.tips.as_ref());
        if self.ui.coin_names.is_empty() {
            if let (Some(t), Some(st)) = (tips, self.ui.state.as_ref()) {
                self.ui.coin_names = crate::ui::market::tip::coin_names(t, &st.currencies);
            }
        }
        std::mem::take(&mut self.ui.chat)
    }

    /// Ctrl went down (`true`) or up.
    pub fn ctrl(&mut self, down: bool) {
        self.ctrl = down;
    }

    /// Ctrl is held: typed letters are a chord, not text.
    pub fn ctrl_held(&self) -> bool {
        self.ctrl
    }

    /// Whether key `vk` is the market key with Ctrl held.
    pub fn is_chord(&self, vk: u16) -> bool {
        let Some(c) = self.ui.key.letter() else {
            return false;
        };
        self.ctrl && vk == c as u16
    }

    /// The market key in force (the game's Controls setting, `[controls]
    /// market_key`).
    pub fn set_key(&mut self, key: MarketKey) {
        self.ui.key = key;
    }

    pub fn pointer(&mut self, p: Point) {
        self.mouse = p;
    }

    /// A press (`down`) or release at `p`: whether the window took it.
    pub fn mouse(&mut self, down: bool, p: Point) -> bool {
        self.mouse = p;
        let e = if down {
            UiEvent::Press {
                button: PointerButton::Left,
                at: p,
            }
        } else {
            UiEvent::Release {
                button: PointerButton::Left,
                at: p,
            }
        };
        self.event(e)
    }

    /// The wheel, `delta` in units of 120 a notch (positive away from the
    /// player).
    pub fn wheel(&mut self, delta: i32) -> bool {
        let steps = delta.signum() * (delta.abs() / 120).max(1);
        let at = self.mouse;
        self.event(UiEvent::Wheel { steps, at })
    }

    /// A typed UTF-16 unit (Backspace 8, Enter 13 included).
    pub fn char(&mut self, unit: u16) -> bool {
        self.event(UiEvent::Char(unit))
    }

    /// Esc: the open box or text box first, then the window.
    pub fn esc(&mut self) -> bool {
        let id = ActionId(crate::controls::Action::GameMenu.index() as u16);
        let p = places();
        window::event(&mut self.ui, &p, UiEvent::Action(id), true)
    }

    fn event(&mut self, e: UiEvent) -> bool {
        if !self.ui.shown() {
            return false;
        }
        let p = places();
        window::event(&mut self.ui, &p, e, false)
    }

    /// The tab shown says where selling and claiming happen (My shop,
    /// Deliveries).
    pub fn stash_note(&self) -> bool {
        self.ui.shown() && matches!(self.ui.tab, MarketTab::MyShop | MarketTab::Deliveries)
    }

    /// The strip under the window with [`STASH_NOTE`].
    pub fn note_rect() -> Rect {
        let w = places().window();
        Rect::new(w.x, w.y + i32::from(w.h), w.w, 16)
    }

    /// The window (and the tip under the mouse) as the front end's items.
    pub fn draw(&self) -> Vec<DrawItem> {
        if !self.ui.shown() {
            return Vec::new();
        }
        let parts = self.parts.as_ref();
        let kit = parts
            .and_then(|p| p.palette.as_deref())
            .and_then(Kit::of)
            .unwrap_or(FALLBACK_KIT);
        let fonts = parts.and_then(|p| p.fonts.as_ref());
        let i = DrawIn {
            ui: &self.ui,
            places: places(),
            pen: Pen {
                kit,
                fonts,
                mouse: self.mouse,
            },
            tips: parts.and_then(|p| p.tips.as_ref()),
            compare: None,
        };
        let mut draws: Vec<UiDraw> = Vec::new();
        window::draw(&i, &mut draws);
        if self.stash_note() {
            let r = Self::note_rect();
            i.pen.fill(r, kit.panel, &mut draws);
            i.pen.frame(r, kit.gold, &mut draws);
            i.pen
                .label(r, STASH_NOTE, window::SMALL, window::GOLD, &mut draws);
        }
        if let Some((lines, anchor)) = window::hover_tip(&i) {
            draw_tip_at(&lines, anchor, (800, 600), fonts, &mut draws);
        }
        front_items(draws)
    }

    /// The window's parts as UI elements (the game's ids).
    pub fn ui_elements(&self) -> Vec<UiElement> {
        if !self.ui.shown() {
            return Vec::new();
        }
        window::ui_elements(&self.ui, &places())
    }

    /// The window's rectangle.
    pub fn rect(&self) -> Rect {
        places().window()
    }
}

/// The kit without a palette (tests): the entries the sky palette's
/// nearest colours come out near, so a draw still has distinct parts.
const FALLBACK_KIT: Kit = Kit {
    panel: 0,
    edge: 1,
    gold: 2,
    on: 3,
    field: 4,
    hit: 5,
};

/// The in-game UI's draws as the front end's items: a rectangle is a
/// [`DrawItem::Shade`] (its draw mode kept), a line its one-pixel shades,
/// a text a plain text item (pen, font and colour kept). The window draws
/// no pictures.
pub fn front_items(draws: Vec<UiDraw>) -> Vec<DrawItem> {
    let mut out = Vec::with_capacity(draws.len());
    for d in draws {
        match d {
            UiDraw::Rect(r) => {
                let (w, h) = (r.x1 - r.x0, r.y1 - r.y0);
                if w > 0 && h > 0 {
                    out.push(DrawItem::Shade {
                        at: Point::new(r.x0, r.y0),
                        w,
                        h,
                        color: r.color,
                        mode: r.mode,
                    });
                }
            }
            UiDraw::Line(l) => line_items(&l, &mut out),
            UiDraw::Text(t) => out.push(DrawItem::Text {
                string_id: 0,
                text: String::from_utf16_lossy(&t.text),
                font: t.style.font,
                at: t.at,
                label: None,
                color: i32::from(t.style.color),
                boxed: None,
            }),
            UiDraw::Image(_) => {}
        }
    }
    out
}

/// The pixels of line `l` (both ends), opaque: a straight one as one
/// shade, a slanted one (the price graph) a shade per pixel.
fn line_items(l: &LineRequest, out: &mut Vec<DrawItem>) {
    let px = |x: i32, y: i32, w: i32, h: i32| DrawItem::Shade {
        at: Point::new(x, y),
        w,
        h,
        color: l.color,
        mode: 5,
    };
    let (dx, dy) = ((l.x1 - l.x0).abs(), (l.y1 - l.y0).abs());
    if dy == 0 {
        out.push(px(l.x0.min(l.x1), l.y0, dx + 1, 1));
        return;
    }
    if dx == 0 {
        out.push(px(l.x0, l.y0.min(l.y1), 1, dy + 1));
        return;
    }
    let (sx, sy) = ((l.x1 - l.x0).signum(), (l.y1 - l.y0).signum());
    let (mut x, mut y, mut err) = (l.x0, l.y0, dx - dy);
    loop {
        out.push(px(x, y, 1, 1));
        if x == l.x1 && y == l.y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

#[cfg(test)]
#[path = "market_host_tests.rs"]
mod tests;
