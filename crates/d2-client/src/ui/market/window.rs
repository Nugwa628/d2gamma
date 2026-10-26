// D2RUST's own: the market window's frame, places, draw and input.
//! Where the window's parts sit, what each is called, how it is drawn
//! and what a click on it does. The window is centred over the play area
//! (above the control panel) in the UI's own pixels, so it is scaled with
//! the rest of the UI; it is drawn like the shared stash's panel, in the
//! Display v3 kit's colours matched to the act palette
//! ([`crate::ui::shared_stash::Kit`]).
//!
//! Every clickable part is a [`Spot`] with a rectangle ([`spots`]): the
//! draw, the clicks ([`event`]) and the test driver's UI ids
//! ([`ui_elements`]) read the same list.

use crate::market::source::{Group, MarketSort, PriceMode, Tri};
use crate::ui::draw::{LineRequest, RectRequest, UiDraw, UiDrawSink};
use crate::ui::geom::{Point, Rect};
use crate::ui::item_tip::{ItemTips, TipAnchor, TipLine};
use crate::ui::layout::Screen;
use crate::ui::original::FontMeasure;
use crate::ui::panel::PointerButton;
use crate::ui::panel::UiEvent;
use crate::ui::panels::text;
use crate::ui::shared_stash::Kit;
use crate::ui::ui_ids::UiElement;

use super::deliveries::{self, DeliverySpot};
use super::details::{self, Compare};
use super::filters::{self, Flag, TYPE_TREE};
use super::offer_form::{self, FormSpot};
use super::offers::{self, OfferSpot};
use super::shop::{self, ShopSpot};
use super::{grid, notices, table, MarketTab, MarketUi, ResultsView, TextBox, PAGE_SIZES};

/// Font 6 (`Font6`): most of the window.
pub const SMALL: u16 = 6;
/// Font 1 (`Font16`): the title and the item's name.
pub const LABEL: u16 = 1;
/// Text colours of the string tables' colour codes.
pub const WHITE: u16 = 0;
pub const RED: u16 = 1;
pub const GOLD: u16 = 4;
pub const GREY: u16 = 5;
pub const TAN: u16 = 7;

/// The rectangle draw mode that writes the colour (mode 0 blends it).
const OPAQUE: u8 = 5;

/// The window's size at most (smaller screens get a smaller window).
pub const WIN_W: i32 = 720;
pub const WIN_H: i32 = 460;

/// The filter column's left and width; the details column's width.
const FX: i32 = 6;
const FW: i32 = 150;
const RX: i32 = 162;
const DW: i32 = 184;

/// The type tree's rows shown at once.
pub const TREE_ROWS: usize = 8;

/// The window's rectangles on a screen, from its top-left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Places {
    pub x0: i32,
    pub y0: i32,
    pub w: i32,
    pub h: i32,
}

impl Places {
    /// Centred over the play area above the control panel.
    pub fn of(s: &Screen) -> Places {
        let w = WIN_W.min(s.w - 16);
        let h = WIN_H.min(s.h - 48 - 16);
        Places {
            x0: (s.w - w) / 2,
            y0: ((s.h - 48 - h) / 2).max(0),
            w,
            h,
        }
    }

    pub fn r(&self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect::new(self.x0 + x, self.y0 + y, w.max(0) as u16, h.max(0) as u16)
    }

    pub fn window(&self) -> Rect {
        self.r(0, 0, self.w, self.h)
    }

    pub fn tab(&self, i: usize) -> Rect {
        self.r(70 + 88 * i as i32, 4, 84, 18)
    }

    pub fn close(&self) -> Rect {
        self.r(self.w - 22, 4, 18, 18)
    }

    pub fn search(&self) -> Rect {
        self.r(FX, 28, FW, 16)
    }

    pub fn suggestion(&self, i: usize) -> Rect {
        self.r(FX, 44 + 12 * i as i32, FW + 60, 12)
    }

    pub fn quality(&self, i: usize) -> Rect {
        self.r(FX, 60 + 12 * i as i32, FW, 12)
    }

    pub fn find(&self) -> Rect {
        self.r(FX, 170, FW, 14)
    }

    pub fn tree_row(&self, k: usize) -> Rect {
        self.r(FX, 186 + 12 * k as i32, FW, 12)
    }

    /// The type tree's rows, for the wheel.
    pub fn tree(&self) -> Rect {
        self.r(FX, 186, FW, 12 * TREE_ROWS as i32)
    }

    pub fn flag(&self, i: usize) -> Rect {
        self.r(FX, 296 + 13 * i as i32, FW, 12)
    }

    pub fn price(&self, i: usize) -> Rect {
        self.r(FX + (i as i32 % 2) * 76, 350 + (i as i32 / 2) * 14, 74, 13)
    }

    pub fn save(&self) -> Rect {
        self.r(FX, 398, 92, 16)
    }

    pub fn reset(&self) -> Rect {
        self.r(FX + 96, 398, FW - 96, 16)
    }

    /// The read-only economy chips: softcore or hardcore, ladder or not.
    pub fn split(&self, i: usize) -> Rect {
        self.r(FX + 76 * i as i32, 420, 74, 14)
    }

    fn rw(&self) -> i32 {
        self.w - RX - DW - 12
    }

    pub fn results_head(&self) -> Rect {
        self.r(RX, 28, self.rw(), 12)
    }

    pub fn view(&self, i: usize) -> Rect {
        self.r(RX + 40 * i as i32, 44, 38, 14)
    }

    pub fn page_size(&self, i: usize) -> Rect {
        self.r(RX + 90 + 26 * i as i32, 44, 24, 14)
    }

    pub fn prev(&self) -> Rect {
        self.r(RX + self.rw() - 84, 44, 18, 14)
    }

    pub fn page_line(&self) -> Rect {
        self.r(RX + self.rw() - 64, 44, 44, 14)
    }

    pub fn next(&self) -> Rect {
        self.r(RX + self.rw() - 18, 44, 18, 14)
    }

    /// The results: the grid or the table.
    pub fn area(&self) -> Rect {
        self.r(RX, 62, self.rw(), self.h - 62 - 24)
    }

    pub fn note(&self) -> Rect {
        self.r(RX, self.h - 20, self.rw(), 14)
    }

    pub fn details(&self) -> Rect {
        self.r(self.w - DW - 6, 28, DW, self.h - 34)
    }

    pub fn buy(&self) -> Rect {
        self.r(self.w - DW - 6, self.h - 50, DW, 18)
    }

    pub fn offer(&self) -> Rect {
        self.r(self.w - DW - 6, self.h - 28, DW, 18)
    }

    pub fn confirm_box(&self) -> Rect {
        self.r((self.w - 280) / 2, (self.h - 140) / 2, 280, 140)
    }

    pub fn confirm(&self) -> Rect {
        let b = self.confirm_box();
        Rect::new(b.x + 20, b.y + 108, 110, 22)
    }

    pub fn cancel(&self) -> Rect {
        let b = self.confirm_box();
        Rect::new(b.x + 150, b.y + 108, 110, 22)
    }
}

/// A clickable part of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    Close,
    Tab(MarketTab),
    Search,
    Suggestion(usize),
    Quality(Group),
    /// A type tree parent's open / close sign.
    Expand(usize),
    Type(usize),
    Find,
    Flag(Flag),
    Price(PriceMode),
    Save,
    Reset,
    View(ResultsView),
    PageSize(u8),
    Prev,
    Next,
    Sort(MarketSort),
    /// Listing `i` of the page.
    Result(usize),
    Buy,
    Offer,
    Confirm,
    Cancel,
    /// A part of the Offers tab or its answer's confirm.
    Offers(OfferSpot),
    /// A part of the Deliveries tab.
    Deliveries(DeliverySpot),
    /// A part of the offer form.
    Form(FormSpot),
    /// A part of the My shop tab.
    Shop(ShopSpot),
    /// The details panel's Price check.
    PriceCheck,
    /// The price check's span chip (days).
    PriceDays(u16),
    PriceClose,
    /// A point of the price graph (series, bucket): its tip on hover.
    PricePoint(usize, usize),
}

/// The UI id word of a sort.
pub fn sort_key(s: MarketSort) -> &'static str {
    match s {
        MarketSort::Posted => "posted",
        MarketSort::Price => "price",
        MarketSort::Stat => "stat",
        MarketSort::Level => "level",
        MarketSort::Name => "name",
        MarketSort::Seller => "seller",
    }
}

impl Spot {
    /// Its UI id.
    pub fn id(self, ui: &MarketUi) -> String {
        match self {
            Spot::Close => "market.close".into(),
            Spot::Tab(t) => format!("market.tab({})", t.key()),
            Spot::Search => "market.search".into(),
            Spot::Suggestion(i) => format!("market.suggestion({i})"),
            Spot::Quality(g) => format!("market.filter.quality({})", g.key()),
            Spot::Expand(n) => format!("market.filter.expand({})", TYPE_TREE[n].key),
            Spot::Type(n) => format!("market.filter.type({})", TYPE_TREE[n].key),
            Spot::Find => "market.filter.find".into(),
            Spot::Flag(f) => format!("market.filter.flag({})", f.key()),
            Spot::Price(m) => format!("market.filter.price({})", m.key()),
            Spot::Save => "market.filter.save".into(),
            Spot::Reset => "market.filter.reset".into(),
            Spot::View(v) => format!("market.view({})", v.key()),
            Spot::PageSize(n) => format!("market.page_size({n})"),
            Spot::Prev => "market.page.prev".into(),
            Spot::Next => "market.page.next".into(),
            Spot::Sort(s) => format!("market.sort({})", sort_key(s)),
            Spot::Result(i) => format!("market.result({i})"),
            Spot::Buy => "market.buy".into(),
            Spot::Offer => "market.offer".into(),
            Spot::Confirm => "market.confirm".into(),
            Spot::Cancel => "market.cancel".into(),
            Spot::Offers(s) => s.id(),
            Spot::Deliveries(s) => s.id(),
            Spot::Form(s) => s.id(),
            Spot::Shop(s) => s.id(ui),
            Spot::PriceCheck => "market.price_check".into(),
            Spot::PriceDays(d) => format!("market.price.days({d})"),
            Spot::PriceClose => "market.price.close".into(),
            Spot::PricePoint(s, b) => format!("market.price.point({s},{b})"),
        }
    }
}

/// The type tree rows shown now: `(node, slot)`.
pub fn tree_rows(ui: &MarketUi) -> Vec<(usize, usize)> {
    filters::shown_rows(&ui.tree_open, &ui.type_find)
        .into_iter()
        .skip(ui.tree_top)
        .take(TREE_ROWS)
        .enumerate()
        .map(|(k, n)| (n, k))
        .collect()
}

/// Every clickable part shown now, bottom-most first (a box over the
/// window leaves only its own buttons).
pub fn spots(ui: &MarketUi, p: &Places) -> Vec<(Spot, Rect)> {
    let mut v = Vec::new();
    if !ui.shown() {
        return v;
    }
    if ui.confirm.is_some() {
        v.push((Spot::Confirm, p.confirm()));
        v.push((Spot::Cancel, p.cancel()));
        return v;
    }
    if let Some(f) = &ui.form {
        let form = offer_form::spots(ui, f, p);
        v.extend(form.into_iter().map(|(s, r)| (Spot::Form(s), r)));
        return v;
    }
    if ui.offers.deal.is_some() {
        let deal = offers::spots(ui, p);
        v.extend(deal.into_iter().map(|(s, r)| (Spot::Offers(s), r)));
        return v;
    }
    if ui.prices.open {
        return super::price_graph::spots(ui, p);
    }
    let boxed = ui.tab == MarketTab::MyShop && ui.shop.boxed.is_some();
    if !boxed {
        v.push((Spot::Close, p.close()));
        for (i, t) in MarketTab::ALL.into_iter().enumerate() {
            v.push((Spot::Tab(t), p.tab(i)));
        }
    }
    if ui.tab == MarketTab::MyShop {
        v.extend(
            shop::spots(ui, p)
                .into_iter()
                .map(|(s, r)| (Spot::Shop(s), r)),
        );
        return v;
    }
    match ui.tab {
        MarketTab::Browse => {}
        MarketTab::Offers => {
            let tab = offers::spots(ui, p);
            v.extend(tab.into_iter().map(|(s, r)| (Spot::Offers(s), r)));
            return v;
        }
        MarketTab::Deliveries => {
            let tab = deliveries::spots(ui, p);
            v.extend(tab.into_iter().map(|(s, r)| (Spot::Deliveries(s), r)));
            return v;
        }
        MarketTab::MyShop => return v,
    }
    v.push((Spot::Search, p.search()));
    for (i, g) in Group::ALL.into_iter().enumerate() {
        v.push((Spot::Quality(g), p.quality(i)));
    }
    v.push((Spot::Find, p.find()));
    for (n, k) in tree_rows(ui) {
        let r = p.tree_row(k);
        if filters::has_children(n) && ui.type_find.trim().is_empty() {
            v.push((Spot::Expand(n), Rect::new(r.x, r.y, 10, r.h)));
            v.push((Spot::Type(n), Rect::new(r.x + 10, r.y, r.w - 10, r.h)));
        } else {
            v.push((Spot::Type(n), r));
        }
    }
    for (i, f) in Flag::ALL.into_iter().enumerate() {
        v.push((Spot::Flag(f), p.flag(i)));
    }
    for (i, m) in PriceMode::ALL.into_iter().enumerate() {
        v.push((Spot::Price(m), p.price(i)));
    }
    v.push((Spot::Save, p.save()));
    v.push((Spot::Reset, p.reset()));
    v.push((Spot::View(ResultsView::Grid), p.view(0)));
    v.push((Spot::View(ResultsView::Table), p.view(1)));
    for (i, n) in PAGE_SIZES.into_iter().enumerate() {
        v.push((Spot::PageSize(n), p.page_size(i)));
    }
    v.push((Spot::Prev, p.prev()));
    v.push((Spot::Next, p.next()));
    let area = p.area();
    match ui.view {
        ResultsView::Grid => {
            for c in grid::cells(ui, area) {
                if let grid::Cell::Item(i, r) = c {
                    v.push((Spot::Result(i), r));
                }
            }
        }
        ResultsView::Table => {
            for (s, _, r) in table::headers(area) {
                if let Some(s) = s {
                    v.push((Spot::Sort(s), r));
                }
            }
            for (i, r) in table::rows(ui, area) {
                v.push((Spot::Result(i), r));
            }
        }
    }
    if ui.selected.is_some() {
        v.push((Spot::PriceCheck, super::price_graph::button(p)));
    }
    if ui.can_buy() {
        v.push((Spot::Buy, p.buy()));
    }
    if ui.can_offer() {
        v.push((Spot::Offer, p.offer()));
    }
    for i in 0..ui.suggestions().len() {
        v.push((Spot::Suggestion(i), p.suggestion(i)));
    }
    v
}

/// The top-most part at `at`.
pub fn hit(ui: &MarketUi, p: &Places, at: Point) -> Option<Spot> {
    spots(ui, p)
        .into_iter()
        .rev()
        .find(|(_, r)| r.contains(at))
        .map(|(s, _)| s)
}

/// The window's UI elements.
pub fn ui_elements(ui: &MarketUi, p: &Places) -> Vec<UiElement> {
    spots(ui, p)
        .into_iter()
        .map(|(s, r)| UiElement::new(s.id(ui), r))
        .collect()
}

/// A click on `s`.
pub fn click(ui: &mut MarketUi, s: Spot) {
    if !matches!(
        s,
        Spot::Search | Spot::Find | Spot::Suggestion(_) | Spot::Form(_)
    ) {
        ui.focus = None;
    }
    match s {
        Spot::Close => ui.close(),
        Spot::Tab(t) => ui.set_tab(t),
        Spot::Search => ui.focus = Some(TextBox::Search),
        Spot::Find => ui.focus = Some(TextBox::TypeFind),
        Spot::Suggestion(i) => ui.pick_suggestion(i),
        Spot::Quality(g) => {
            ui.filters.toggle_group(g);
            ui.refilter();
        }
        Spot::Expand(n) => {
            if !ui.tree_open.remove(&n) {
                ui.tree_open.insert(n);
            }
        }
        Spot::Type(n) => {
            ui.filters.pick_type(n);
            ui.refilter();
        }
        Spot::Flag(f) => {
            ui.filters.cycle(f);
            ui.refilter();
        }
        Spot::Price(m) => {
            ui.filters.price = if ui.filters.price == m {
                PriceMode::Any
            } else {
                m
            };
            ui.refilter();
        }
        Spot::Save => ui.save_default(),
        Spot::Reset => ui.reset(),
        Spot::View(v) => ui.set_view(v),
        Spot::PageSize(n) => ui.set_page_size(n),
        Spot::Prev => ui.go_page(ui.page.saturating_sub(1)),
        Spot::Next => ui.go_page(ui.page.saturating_add(1)),
        Spot::Sort(s) => ui.sort_by(s),
        Spot::Result(i) => ui.select(i),
        Spot::Buy => ui.ask_buy(),
        Spot::Offer => ui.ask_offer(),
        Spot::Confirm => ui.confirm_buy(),
        Spot::Cancel => ui.confirm = None,
        Spot::Offers(s) => offers::click(ui, s),
        Spot::Deliveries(s) => deliveries::click(ui, s),
        Spot::Form(s) => offer_form::click(ui, s),
        Spot::Shop(s) => shop::click(ui, s, false),
        Spot::PriceCheck => ui.check_selected(),
        Spot::PriceDays(d) => ui.price_days(d),
        Spot::PriceClose => ui.close_prices(),
        Spot::PricePoint(..) => {}
    }
}

/// The longest text a box takes.
const TEXT_MAX: usize = 60;

/// A typed character with a box focused.
fn type_char(ui: &mut MarketUi, unit: u16) {
    let Some(focus) = ui.focus else { return };
    if focus == TextBox::Gold {
        match (unit, ui.form.as_mut()) {
            (0x0D | 0x1B, _) | (_, None) => ui.focus = None,
            (u, Some(f)) => f.type_gold(u),
        }
        return;
    }
    if matches!(focus, TextBox::ShopGold | TextBox::ShopNote) {
        shop::type_char(ui, focus, unit);
        return;
    }
    match unit {
        0x0D => {
            ui.focus = None;
            if focus == TextBox::Search {
                ui.refilter();
            }
        }
        0x1B => ui.focus = None,
        0x08 => {
            browse_text(ui, focus).pop();
            ui.tree_top = 0;
        }
        u => {
            let Some(c) = char::from_u32(u32::from(u)).filter(|c| !c.is_control()) else {
                return;
            };
            let t = browse_text(ui, focus);
            if t.len() < TEXT_MAX {
                t.push(c);
            }
            ui.tree_top = 0;
        }
    }
}

/// The text of a Browse box (the others take their typing before).
fn browse_text(ui: &mut MarketUi, focus: TextBox) -> &mut String {
    match focus {
        TextBox::TypeFind => &mut ui.type_find,
        _ => &mut ui.filters.text,
    }
}

/// What the window does with an event: true when it takes it. `esc` is
/// the Esc key's action (the game menu's).
pub fn event(ui: &mut MarketUi, p: &Places, e: UiEvent, esc: bool) -> bool {
    if !ui.shown() {
        return false;
    }
    let inside = |at: Point| p.window().contains(at);
    match e {
        UiEvent::Press {
            button: PointerButton::Left,
            at,
        } => {
            if let Some(s) = hit(ui, p, at) {
                click(ui, s);
                return true;
            }
            if inside(at) {
                ui.focus = None;
            }
            inside(at) || ui.boxed()
        }
        // A right press takes one currency off the Set price... box.
        UiEvent::Press {
            button: PointerButton::Right,
            at,
        } => {
            if let Some(Spot::Shop(s)) = hit(ui, p, at) {
                shop::click(ui, s, true);
            }
            inside(at)
        }
        UiEvent::Press { at, .. } | UiEvent::Release { at, .. } => inside(at),
        UiEvent::Wheel { steps, at } => {
            wheel(ui, p, at, steps);
            inside(at)
        }
        UiEvent::Char(u) => {
            if ui.focus.is_some() {
                type_char(ui, u);
                return true;
            }
            false
        }
        UiEvent::Action(_) if esc => {
            if ui.focus.is_some() {
                ui.focus = None;
            } else if ui.prices.open {
                ui.close_prices();
            } else if ui.confirm.is_some() {
                ui.confirm = None;
            } else if ui.form.is_some() {
                ui.form = None;
            } else if ui.offers.deal.is_some() {
                ui.offers.deal = None;
            } else if ui.shop.boxed.is_some() {
                ui.shop.boxed = None;
            } else {
                ui.close();
            }
            true
        }
        // The game's key bindings wait while a box has the typing.
        UiEvent::Action(_) => ui.focus.is_some(),
        UiEvent::CursorMoved(_) | UiEvent::CursorLeft => false,
    }
}

/// The wheel at `at`: the form's lists over everything, else the shown
/// tab's lists.
fn wheel(ui: &mut MarketUi, p: &Places, at: Point, steps: i32) {
    if ui.form.is_some() {
        offer_form::wheel(ui, p, at, steps);
        return;
    }
    if ui.boxed() {
        return;
    }
    match ui.tab {
        MarketTab::Offers => offers::wheel(ui, p, at, steps),
        MarketTab::Deliveries => deliveries::wheel(ui, p, at, steps),
        MarketTab::MyShop => {
            if p.window().contains(at) {
                shop::wheel(ui, p, steps);
            }
        }
        MarketTab::Browse => {
            if p.area().contains(at) {
                let max = match ui.view {
                    ResultsView::Grid => {
                        grid::row_count(ui).saturating_sub(grid::visible(p.area()))
                    }
                    ResultsView::Table => {
                        table::row_count(ui).saturating_sub(table::visible(p.area()))
                    }
                };
                ui.scroll = scrolled(ui.scroll, steps * 3, max);
            } else if p.tree().contains(at) {
                let n = filters::shown_rows(&ui.tree_open, &ui.type_find).len();
                ui.tree_top = scrolled(ui.tree_top, steps, n.saturating_sub(TREE_ROWS));
            }
        }
    }
}

/// `top` moved `steps` up (away from the user) or down, within `0..=max`.
fn scrolled(top: usize, steps: i32, max: usize) -> usize {
    let t = top as i64 - i64::from(steps);
    t.clamp(0, max as i64) as usize
}

/// What a draw reads.
pub struct DrawIn<'a> {
    pub ui: &'a MarketUi,
    pub places: Places,
    pub pen: Pen<'a>,
    /// The game's item tips, when the game data is loaded.
    pub tips: Option<&'a ItemTips>,
    /// The equipped item in the shown listing's slot.
    pub compare: Option<&'a Compare>,
}

/// The kit's drawing helpers.
#[derive(Clone, Copy)]
pub struct Pen<'a> {
    pub kit: Kit,
    pub fonts: Option<&'a FontMeasure>,
    pub mouse: Point,
}

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// A font's text height (the pen sits on its bottom row).
fn height(font: u16) -> i32 {
    if font == SMALL {
        7
    } else {
        10
    }
}

impl Pen<'_> {
    pub fn width(&self, font: u16, s: &str) -> i32 {
        let t = units(s);
        self.fonts
            .and_then(|f| f.width_a(font, &t))
            .unwrap_or(t.len() as i32 * if font == SMALL { 5 } else { 8 })
    }

    /// `s` cut to fit `w` pixels, with ".." where it was cut.
    pub fn fit(&self, font: u16, s: &str, w: i32) -> String {
        if self.width(font, s) <= w {
            return s.to_owned();
        }
        let mut t: String = s.to_owned();
        while !t.is_empty() && self.width(font, &format!("{t}..")) > w {
            t.pop();
        }
        format!("{}..", t.trim_end())
    }

    /// `s` with its top-left at (x, y).
    pub fn text(&self, x: i32, y: i32, s: &str, font: u16, color: u16, out: &mut dyn UiDrawSink) {
        out.push(text(units(s), x, y + height(font), font, color));
    }

    /// `s` ending at `right`.
    pub fn text_right(
        &self,
        right: i32,
        y: i32,
        s: &str,
        font: u16,
        color: u16,
        out: &mut dyn UiDrawSink,
    ) {
        let w = self.width(font, s);
        self.text(right - w, y, s, font, color, out);
    }

    /// `s` fitted into `r` and centred in it.
    pub fn label(&self, r: Rect, s: &str, font: u16, color: u16, out: &mut dyn UiDrawSink) {
        let s = self.fit(font, s, i32::from(r.w) - 4);
        let w = self.width(font, &s);
        let x = r.x + (i32::from(r.w) - w) / 2;
        let y = r.y + (i32::from(r.h) - height(font)) / 2;
        self.text(x, y, &s, font, color, out);
    }

    /// `s` fitted into `r`, from its left, centred in its height.
    pub fn line(&self, r: Rect, s: &str, font: u16, color: u16, out: &mut dyn UiDrawSink) {
        let s = self.fit(font, s, i32::from(r.w) - 4);
        let y = r.y + (i32::from(r.h) - height(font)) / 2;
        self.text(r.x + 2, y, &s, font, color, out);
    }

    pub fn fill(&self, r: Rect, color: u8, out: &mut dyn UiDrawSink) {
        out.push(UiDraw::Rect(RectRequest::sized(
            r.x,
            r.y,
            i32::from(r.w),
            i32::from(r.h),
            color,
            OPAQUE,
        )));
    }

    /// A one-pixel frame inside `r`.
    pub fn frame(&self, r: Rect, color: u8, out: &mut dyn UiDrawSink) {
        let (x0, y0) = (r.x, r.y);
        let (x1, y1) = (r.x + i32::from(r.w) - 1, r.y + i32::from(r.h) - 1);
        for (a, b, c, d) in [
            (x0, y0, x1, y0),
            (x1, y0, x1, y1),
            (x1, y1, x0, y1),
            (x0, y1, x0, y0),
        ] {
            out.push(UiDraw::Line(LineRequest {
                x0: a,
                y0: b,
                x1: c,
                y1: d,
                color,
            }));
        }
    }

    pub fn hline(&self, x0: i32, x1: i32, y: i32, color: u8, out: &mut dyn UiDrawSink) {
        out.push(UiDraw::Line(LineRequest {
            x0,
            y0: y,
            x1,
            y1: y,
            color,
        }));
    }

    /// A button: filled, framed in gold when on, its label centred, gold
    /// when on or under the mouse.
    pub fn button(&self, r: Rect, s: &str, on: bool, out: &mut dyn UiDrawSink) {
        let k = self.kit;
        self.fill(r, if on { k.on } else { k.panel }, out);
        self.frame(r, if on { k.gold } else { k.edge }, out);
        let color = if on || r.contains(self.mouse) {
            GOLD
        } else {
            TAN
        };
        self.label(r, s, SMALL, color, out);
    }

    /// A text box: the text or, empty, its hint; a caret when focused.
    pub fn text_box(
        &self,
        r: Rect,
        value: &str,
        hint: &str,
        focused: bool,
        out: &mut dyn UiDrawSink,
    ) {
        let k = self.kit;
        self.fill(r, k.field, out);
        self.frame(r, if focused { k.gold } else { k.edge }, out);
        let w = i32::from(r.w) - 8;
        if value.is_empty() && !focused {
            self.line(r, hint, SMALL, GREY, out);
            return;
        }
        let shown = if focused {
            format!("{value}_")
        } else {
            value.to_owned()
        };
        // The end of a long text stays in view.
        let mut s = shown.as_str();
        while self.width(SMALL, s) > w && !s.is_empty() {
            let mut cs = s.chars();
            cs.next();
            s = cs.as_str();
        }
        self.line(r, s, SMALL, WHITE, out);
    }
}

/// A count beside a filter.
fn count(n: u32) -> String {
    n.to_string()
}

/// Draws the window.
pub fn draw(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let ui = i.ui;
    if !ui.shown() {
        return;
    }
    let (p, pen, k) = (&i.places, &i.pen, i.pen.kit);
    let win = p.window();
    pen.fill(win, k.panel, out);
    pen.frame(win, k.gold, out);
    pen.frame(
        Rect::new(win.x + 2, win.y + 2, win.w - 4, win.h - 4),
        k.edge,
        out,
    );
    pen.text(p.x0 + 10, p.y0 + 7, "Market", LABEL, GOLD, out);
    for (n, t) in MarketTab::ALL.into_iter().enumerate() {
        // My shop shows its slots used: "My shop 3/50".
        let label = match (&ui.state, t) {
            (Some(s), MarketTab::MyShop) => {
                format!("{} {}/{}", t.label(), s.active_listings, s.max_listings)
            }
            _ => notices::tab_label(ui, t),
        };
        pen.button(p.tab(n), &label, ui.tab == t, out);
    }
    pen.button(p.close(), "X", false, out);
    pen.hline(
        win.x + 4,
        win.x + i32::from(win.w) - 5,
        p.y0 + 24,
        k.edge,
        out,
    );
    match ui.tab {
        MarketTab::Browse => {
            draw_filters(i, out);
            draw_results(i, out);
            details::draw(i, out);
            draw_suggestions(i, out);
        }
        MarketTab::Offers => offers::draw(i, out),
        MarketTab::Deliveries => deliveries::draw(i, out),
        MarketTab::MyShop => shop::draw(i, out),
    }
    details::draw_boxes(i, out);
    super::price_graph::draw(i, out);
}

fn draw_filters(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen, k) = (i.ui, &i.places, &i.pen, i.pen.kit);
    let f = &ui.facets;
    pen.text_box(
        p.search(),
        &ui.filters.text,
        "Search names...",
        ui.focus == Some(TextBox::Search),
        out,
    );
    let head = |y: i32, s: &str, out: &mut dyn UiDrawSink| {
        pen.text(p.x0 + FX, p.y0 + y, s, SMALL, GREY, out);
    };
    head(50, "QUALITY", out);
    for (n, g) in Group::ALL.into_iter().enumerate() {
        let r = p.quality(n);
        let on = ui.filters.groups.contains(&g);
        row_back(pen, r, on, out);
        pen.line(r, g.label(), SMALL, g.color(), out);
        let c = count(f.groups.get(&g).copied().unwrap_or(0));
        pen.text_right(r.x + i32::from(r.w) - 3, r.y + 2, &c, SMALL, TAN, out);
    }
    head(160, "TYPE", out);
    pen.text_box(
        p.find(),
        &ui.type_find,
        "Find a type...",
        ui.focus == Some(TextBox::TypeFind),
        out,
    );
    let finding = !ui.type_find.trim().is_empty();
    for (n, slot) in tree_rows(ui) {
        let r = p.tree_row(slot);
        let node = &TYPE_TREE[n];
        row_back(pen, r, ui.filters.type_node == Some(n), out);
        let mut x = r.x + 2;
        if filters::has_children(n) && !finding {
            let sign = if ui.tree_open.contains(&n) { "-" } else { "+" };
            pen.text(x, r.y + 2, sign, SMALL, GOLD, out);
        }
        x += 10;
        if node.parent.is_some() && !finding {
            x += 8;
        }
        let w = r.x + i32::from(r.w) - x - 26;
        let label = pen.fit(SMALL, node.label, w);
        pen.text(x, r.y + 2, &label, SMALL, WHITE, out);
        let c = count(filters::node_count(f, n));
        pen.text_right(r.x + i32::from(r.w) - 3, r.y + 2, &c, SMALL, TAN, out);
    }
    head(286, "FLAGS", out);
    for (n, fl) in Flag::ALL.into_iter().enumerate() {
        let r = p.flag(n);
        let t = ui.filters.flag(fl);
        row_back(pen, r, t != Tri::Any, out);
        pen.line(r, fl.label(), SMALL, WHITE, out);
        let color = match t {
            Tri::Any => TAN,
            Tri::Yes => GOLD,
            Tri::No => RED,
        };
        let s = format!(
            "{}  {}",
            t.label(),
            crate::market::source::Facets::flag(fl.counts(f), t)
        );
        pen.text_right(r.x + i32::from(r.w) - 3, r.y + 2, &s, SMALL, color, out);
    }
    head(340, "PRICE", out);
    for (n, m) in PriceMode::ALL.into_iter().enumerate() {
        let c = f.price_modes[usize::from(m.code())];
        let s = format!("{} {c}", m.label());
        pen.button(p.price(n), &s, ui.filters.price == m, out);
    }
    pen.button(p.save(), "Save as default", false, out);
    pen.button(p.reset(), "Reset", false, out);
    // The character's economy, shown, never chosen.
    if let Some(st) = &ui.state {
        let chips = [
            if st.hardcore { "Hardcore" } else { "Softcore" },
            if st.ladder > 0 {
                "Ladder"
            } else {
                "Non-ladder"
            },
        ];
        for (n, c) in chips.into_iter().enumerate() {
            let r = p.split(n);
            pen.fill(r, k.field, out);
            pen.frame(r, k.edge, out);
            pen.label(r, c, SMALL, TAN, out);
        }
    }
}

/// A filter row's back: filled when picked, lit under the mouse.
fn row_back(pen: &Pen, r: Rect, on: bool, out: &mut dyn UiDrawSink) {
    if on {
        pen.fill(r, pen.kit.on, out);
        pen.frame(r, pen.kit.gold, out);
    } else if r.contains(pen.mouse) {
        pen.fill(r, pen.kit.hit, out);
    }
}

fn draw_results(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen, k) = (i.ui, &i.places, &i.pen, i.pen.kit);
    let head = match &ui.results {
        Some(r) if r.total > 0 => {
            let per = u32::from(r.page_size);
            let a = u32::from(r.page) * per + 1;
            let b = (a + r.rows.len() as u32).saturating_sub(1);
            format!("Showing {a}-{b} of {}", r.total)
        }
        Some(_) => "Nothing listed matches.".into(),
        None => "Searching...".into(),
    };
    pen.line(p.results_head(), &head, SMALL, TAN, out);
    pen.button(p.view(0), "Grid", ui.view == ResultsView::Grid, out);
    pen.button(p.view(1), "Table", ui.view == ResultsView::Table, out);
    for (n, s) in PAGE_SIZES.into_iter().enumerate() {
        pen.button(p.page_size(n), &s.to_string(), ui.page_size() == s, out);
    }
    pen.button(p.prev(), "<", false, out);
    let pages = format!("{}/{}", ui.page + 1, ui.pages());
    pen.label(p.page_line(), &pages, SMALL, TAN, out);
    pen.button(p.next(), ">", false, out);
    let area = p.area();
    pen.fill(area, k.field, out);
    pen.frame(area, k.edge, out);
    match ui.view {
        ResultsView::Grid => grid::draw(i, area, out),
        ResultsView::Table => table::draw(i, area, out),
    }
    if let Some(n) = &ui.note {
        pen.line(p.note(), n, SMALL, GOLD, out);
    }
}

fn draw_suggestions(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen, k) = (i.ui, &i.places, &i.pen, i.pen.kit);
    for (n, s) in ui.suggestions().into_iter().enumerate() {
        let r = p.suggestion(n);
        pen.fill(
            r,
            if r.contains(pen.mouse) {
                k.hit
            } else {
                k.field
            },
            out,
        );
        pen.frame(r, k.gold, out);
        pen.line(r, s, SMALL, WHITE, out);
    }
}

/// The tip of the listing under the mouse (grid or table), drawn over
/// everything by the panel.
pub fn hover_tip(i: &DrawIn) -> Option<(Vec<TipLine>, TipAnchor)> {
    let ui = i.ui;
    let anchor = |(lines, r): (Vec<TipLine>, Rect)| {
        let a = TipAnchor {
            x: r.x + i32::from(r.w) / 2,
            top: r.y,
            bottom: r.y + i32::from(r.h),
        };
        (lines, a)
    };
    if ui.prices.open {
        return super::price_graph::hover_tip(i);
    }
    if ui.confirm.is_some() || ui.offers.deal.is_some() {
        return None;
    }
    if ui.form.is_some() {
        return offer_form::hover_tip(i).map(anchor);
    }
    match ui.tab {
        MarketTab::Offers => return offers::hover_tip(i).map(anchor),
        MarketTab::Deliveries => return deliveries::hover_tip(i).map(anchor),
        MarketTab::MyShop => return None,
        MarketTab::Browse => {}
    }
    if !ui.suggestions().is_empty() {
        return None;
    }
    let spot = hit(ui, &i.places, i.pen.mouse)?;
    let row = match spot {
        Spot::Result(n) => ui.listing(n)?,
        Spot::Shop(ShopSpot::Row(n)) => &ui.shop.rows().get(n)?.row,
        _ => return None,
    };
    let r = spots(ui, &i.places)
        .into_iter()
        .find(|(s, _)| *s == spot)?
        .1;
    Some(anchor((super::tip::listing_lines(row, i.tips), r)))
}
