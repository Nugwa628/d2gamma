// D2RUST's own: the market window's My shop tab.
//! My shop (example 8 of `docs/plans/market-ui.md`, module K3): the
//! seller's listings in four tabs, Shown in market, Hidden, Sold and
//! Expired (expired and cancelled), each with its count, and the slots
//! used. Rows are ticked one by one or all at once; the buttons under
//! them act on every ticked row with **one** `MarketShopEdit`: Set
//! price..., Note..., Hide (Show on the Hidden tab), Relist 7 d and
//! Cancel. The realm answers per row; a row it refused stays ticked and
//! shows why in red, the others move on and the page is read again.
//!
//! Its parts are named UI elements (`market.shop.tab(Sold)`,
//! `market.shop.row(2)`, `market.shop.hide`, `market.shop.box.apply`).

use std::collections::{BTreeMap, BTreeSet};

use crate::market::source::{
    ListingId, ListingState, MarketAsk, MarketCode, MarketDone, Req, ShopOp, ShopPage, ShopRow,
    ShopSort, ShopTab, NOTE_MAX, SHOP_PAGE,
};
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::{Point, Rect};

use super::details::{age, price_text};
use super::sell::{self, PriceDraft, PriceSpot};
use super::window::{DrawIn, Places, GOLD, GREY, LABEL, RED, SMALL, TAN, WHITE};
use super::{MarketUi, TextBox};

/// A button under the rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShopAct {
    SetPrice,
    Note,
    Hide,
    Show,
    Relist,
    Cancel,
}

impl ShopAct {
    pub fn label(self) -> &'static str {
        match self {
            ShopAct::SetPrice => "Set price...",
            ShopAct::Note => "Note...",
            ShopAct::Hide => "Hide",
            ShopAct::Show => "Show",
            ShopAct::Relist => "Relist 7 d",
            ShopAct::Cancel => "Cancel",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            ShopAct::SetPrice => "set_price",
            ShopAct::Note => "note",
            ShopAct::Hide => "hide",
            ShopAct::Show => "show",
            ShopAct::Relist => "relist",
            ShopAct::Cancel => "cancel",
        }
    }

    /// The buttons a tab has.
    pub fn of_tab(t: ShopTab) -> &'static [ShopAct] {
        match t {
            ShopTab::Shown => &[
                ShopAct::SetPrice,
                ShopAct::Note,
                ShopAct::Hide,
                ShopAct::Relist,
                ShopAct::Cancel,
            ],
            ShopTab::Hidden => &[
                ShopAct::SetPrice,
                ShopAct::Note,
                ShopAct::Show,
                ShopAct::Relist,
                ShopAct::Cancel,
            ],
            ShopTab::Sold => &[],
            ShopTab::Expired => &[ShopAct::Relist],
        }
    }
}

/// The box over the tab: Set price... or Note....
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShopBox {
    Price(PriceDraft),
    Note(String),
}

/// My shop's state.
#[derive(Debug)]
pub struct ShopUi {
    pub tab: ShopTab,
    pub page: u16,
    pub sort: ShopSort,
    pub desc: bool,
    /// The page shown.
    pub view: Option<ShopPage>,
    /// The ticked rows.
    pub selected: BTreeSet<ListingId>,
    /// The page asked for: queued, then numbered.
    pub open_queued: bool,
    pub open_req: Option<Req>,
    /// The edit sent: queued, then numbered.
    pub editing: bool,
    pub edit_req: Option<Req>,
    /// The listings the edit names.
    pub edit_n: usize,
    /// The rows the last edit was refused on, with the realm's reason.
    pub marks: BTreeMap<ListingId, MarketCode>,
    pub boxed: Option<ShopBox>,
    /// The status line, and whether it is a refusal.
    pub line: Option<(String, bool)>,
    /// The first row shown (the wheel scrolls it).
    pub scroll: usize,
}

impl Default for ShopUi {
    fn default() -> Self {
        ShopUi {
            tab: ShopTab::Shown,
            page: 0,
            sort: ShopSort::Posted,
            desc: true,
            view: None,
            selected: BTreeSet::new(),
            open_queued: false,
            open_req: None,
            editing: false,
            edit_req: None,
            edit_n: 0,
            marks: BTreeMap::new(),
            boxed: None,
            line: None,
            scroll: 0,
        }
    }
}

impl ShopUi {
    /// The rows of the shown page.
    pub fn rows(&self) -> &[ShopRow] {
        self.view.as_ref().map_or(&[], |v| v.rows.as_slice())
    }

    /// The ticked rows of the page that `act` applies to.
    pub fn targets(&self, act: ShopAct) -> Vec<ListingId> {
        self.rows()
            .iter()
            .filter(|r| self.selected.contains(&r.row.listing))
            .filter(|r| match act {
                ShopAct::Relist => r.state == ListingState::Active || r.relistable,
                _ => r.state == ListingState::Active,
            })
            .map(|r| r.row.listing)
            .collect()
    }

    pub fn pages(&self) -> u16 {
        let Some(v) = &self.view else { return 1 };
        let n = v.counts[usize::from(self.tab.code())] as usize;
        n.div_ceil(SHOP_PAGE).clamp(1, usize::from(u16::MAX)) as u16
    }
}

/// Asks for the page of the tab, sort and page in force.
pub fn refresh(ui: &mut MarketUi) {
    let s = &mut ui.shop;
    ui.asks.retain(|a| !matches!(a, MarketAsk::ShopOpen { .. }));
    ui.asks.push(MarketAsk::ShopOpen {
        tab: s.tab,
        page: s.page,
        sort: s.sort,
        desc: s.desc,
    });
    s.open_queued = true;
    s.open_req = None;
}

/// Shows tab `t` from its first page; the ticks go.
pub fn set_tab(ui: &mut MarketUi, t: ShopTab) {
    let s = &mut ui.shop;
    if s.tab != t {
        s.selected.clear();
        s.marks.clear();
        s.line = None;
    }
    s.tab = t;
    s.page = 0;
    s.scroll = 0;
    refresh(ui);
}

/// The relist's days: 7, within the realm's longest.
fn relist_days(ui: &MarketUi) -> u8 {
    7.min(ui.state.as_ref().map_or(7, |s| s.max_days).max(1))
}

/// Sends `op` for the ticked rows `act` applies to: one edit.
pub fn edit(ui: &mut MarketUi, act: ShopAct, op: ShopOp) {
    let listings = ui.shop.targets(act);
    if listings.is_empty() || ui.shop.editing || ui.shop.edit_req.is_some() {
        return;
    }
    let fee_rev = ui.state.as_ref().map_or(0, |s| s.fees.fee_rev);
    let n = listings.len();
    ui.asks.push(MarketAsk::ShopEdit {
        op,
        listings,
        fee_rev,
    });
    let s = &mut ui.shop;
    s.editing = true;
    s.edit_n = n;
    s.marks.clear();
    s.line = Some((
        format!("{} on {n} listing(s)...", act.label().trim_end_matches('.')),
        false,
    ));
}

/// A button under the rows: the edits go at once, Set price... and
/// Note... open their box first.
pub fn act(ui: &mut MarketUi, a: ShopAct) {
    if ui.shop.targets(a).is_empty() {
        return;
    }
    match a {
        ShopAct::SetPrice => {
            // The box starts from the first ticked row's price.
            let first = ui
                .shop
                .rows()
                .iter()
                .find(|r| ui.shop.selected.contains(&r.row.listing))
                .map(|r| PriceDraft::of(r.row.mode, r.row.price.as_ref()))
                .unwrap_or_default();
            ui.shop.boxed = Some(ShopBox::Price(first));
        }
        ShopAct::Note => {
            let first = ui
                .shop
                .rows()
                .iter()
                .find(|r| ui.shop.selected.contains(&r.row.listing))
                .map(|r| r.row.note.clone())
                .unwrap_or_default();
            ui.shop.boxed = Some(ShopBox::Note(first));
            ui.focus = Some(TextBox::ShopNote);
        }
        ShopAct::Hide => edit(ui, a, ShopOp::Hide),
        ShopAct::Show => edit(ui, a, ShopOp::Show),
        ShopAct::Relist => {
            let days = relist_days(ui);
            edit(ui, a, ShopOp::Relist { days });
        }
        ShopAct::Cancel => edit(ui, a, ShopOp::Cancel),
    }
}

/// The box's Apply: the edit goes with its price or note.
pub fn apply_box(ui: &mut MarketUi) {
    let Some(b) = ui.shop.boxed.clone() else {
        return;
    };
    match b {
        ShopBox::Price(d) => match d.price() {
            Ok(price) => {
                ui.shop.boxed = None;
                edit(
                    ui,
                    ShopAct::SetPrice,
                    ShopOp::Reprice {
                        mode: d.mode,
                        price,
                    },
                );
            }
            Err(why) => ui.shop.line = Some((why.into(), true)),
        },
        ShopBox::Note(n) => {
            ui.shop.boxed = None;
            edit(ui, ShopAct::Note, ShopOp::Note(n.trim().to_owned()));
        }
    }
    ui.focus = None;
}

/// Ask `ask` went out as request `req`.
pub fn sent(ui: &mut MarketUi, ask: &MarketAsk, req: Req) {
    let s = &mut ui.shop;
    match ask {
        MarketAsk::ShopOpen { .. } => {
            s.open_queued = false;
            s.open_req = Some(req);
        }
        MarketAsk::ShopEdit { .. } => {
            s.editing = false;
            s.edit_req = Some(req);
        }
        _ => {}
    }
}

/// A page of My shop arrived for request `req`.
pub fn on_page(ui: &mut MarketUi, req: Req, page: ShopPage) {
    let s = &mut ui.shop;
    if s.open_queued || s.open_req != Some(req) {
        return;
    }
    s.open_req = None;
    let on_page: BTreeSet<ListingId> = page.rows.iter().map(|r| r.row.listing).collect();
    s.selected.retain(|id| on_page.contains(id));
    s.page = page.page;
    s.tab = page.tab;
    s.view = Some(page);
    s.scroll = s.scroll.min(s.rows().len().saturating_sub(1));
}

/// The market answered request `d`: true when it was My shop's.
pub fn on_done(ui: &mut MarketUi, d: &MarketDone) -> bool {
    let s = &mut ui.shop;
    if s.open_req == Some(d.req) {
        s.open_req = None;
        s.line = Some((d.line(), true));
        return true;
    }
    if s.edit_req != Some(d.req) {
        return false;
    }
    s.edit_req = None;
    if d.code != MarketCode::Done {
        s.line = Some((d.line(), true));
        if d.code == MarketCode::FeeChanged {
            ui.asks.push(MarketAsk::Hello);
        }
        return true;
    }
    s.marks = d
        .rows
        .iter()
        .filter(|(_, c)| *c != MarketCode::Done)
        .map(|(id, c)| (*id, c.clone()))
        .collect();
    let refused = s.marks.len();
    let done = s.edit_n.saturating_sub(refused);
    // The refused rows stay ticked; the others are done with.
    let marks = &s.marks;
    s.selected.retain(|id| marks.contains_key(id));
    s.line = Some(if refused == 0 {
        (format!("Done: {done} listing(s)."), false)
    } else {
        (
            format!("Done: {done}; refused: {refused} (marked in red)."),
            true,
        )
    });
    ui.asks.push(MarketAsk::Hello);
    refresh(ui);
    true
}

/// A typed character into the box's text.
pub fn type_char(ui: &mut MarketUi, focus: TextBox, unit: u16) {
    let Some(b) = ui.shop.boxed.as_mut() else {
        ui.focus = None;
        return;
    };
    let going = match (focus, b) {
        (TextBox::ShopNote, ShopBox::Note(t)) => sell::type_into(t, unit, NOTE_MAX),
        (TextBox::ShopGold, ShopBox::Price(d)) => match unit {
            0x0D | 0x1B => false,
            0x08 => {
                d.gold.pop();
                true
            }
            u => {
                let c = char::from_u32(u32::from(u)).filter(char::is_ascii_digit);
                if let Some(c) = c {
                    if d.gold.len() < 12 && !(d.gold.is_empty() && c == '0') {
                        d.gold.push(c);
                    }
                }
                true
            }
        },
        _ => false,
    };
    if !going {
        ui.focus = None;
    }
}

/// A clickable part of the tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShopSpot {
    Tab(ShopTab),
    /// The header's tick: all rows of the page.
    All,
    /// Row `i` of the page.
    Row(usize),
    Sort(ShopSort),
    Act(ShopAct),
    Prev,
    Next,
    BoxPrice(PriceSpot),
    BoxNote,
    BoxApply,
    BoxCancel,
}

impl ShopSpot {
    pub fn id(self, ui: &MarketUi) -> String {
        match self {
            ShopSpot::Tab(t) => format!("market.shop.tab({})", t.key()),
            ShopSpot::All => "market.shop.all".into(),
            ShopSpot::Row(i) => format!("market.shop.row({i})"),
            ShopSpot::Sort(s) => format!("market.shop.sort({})", sort_key(s)),
            ShopSpot::Act(a) => format!("market.shop.{}", a.key()),
            ShopSpot::Prev => "market.shop.prev".into(),
            ShopSpot::Next => "market.shop.next".into(),
            ShopSpot::BoxPrice(p) => {
                let draft = match &ui.shop.boxed {
                    Some(ShopBox::Price(d)) => d.clone(),
                    _ => PriceDraft::default(),
                };
                p.id("market.shop.box", &draft, &sell::currencies(ui))
            }
            ShopSpot::BoxNote => "market.shop.box.note".into(),
            ShopSpot::BoxApply => "market.shop.box.apply".into(),
            ShopSpot::BoxCancel => "market.shop.box.cancel".into(),
        }
    }
}

fn sort_key(s: ShopSort) -> &'static str {
    match s {
        ShopSort::Posted => "posted",
        ShopSort::Ends => "ends",
        ShopSort::Gold => "gold",
        ShopSort::Name => "name",
    }
}

/// The table's top and its rows' height.
const HEAD_Y: i32 = 52;
const ROW_H: i32 = 13;
const ROWS_Y: i32 = HEAD_Y + 15;
/// The buttons' row, from the window's bottom.
const ACTS_DY: i32 = 46;

fn shop_tab(p: &Places, i: usize) -> Rect {
    p.r(8 + 124 * i as i32, 30, 120, 16)
}

/// The rows shown at once.
pub fn visible(p: &Places) -> usize {
    ((p.h - ROWS_Y - ACTS_DY - 6) / ROW_H).max(1) as usize
}

/// The shown rows: the row's index on the page and its rectangle.
pub fn row_rects(ui: &MarketUi, p: &Places) -> Vec<(usize, Rect)> {
    let n = ui.shop.rows().len();
    let top = ui.shop.scroll.min(n.saturating_sub(1));
    (top..n)
        .take(visible(p))
        .enumerate()
        .map(|(k, i)| (i, p.r(8, ROWS_Y + ROW_H * k as i32, p.w - 16, ROW_H)))
        .collect()
}

/// A column: its title, the sort a click on it picks, its share of the
/// width (of 100).
type Column = (&'static str, Option<ShopSort>, i32);

/// The columns of tab `t`, after the tick.
fn columns(t: ShopTab) -> [Column; 5] {
    match t {
        ShopTab::Shown | ShopTab::Hidden => [
            ("ITEM", Some(ShopSort::Name), 30),
            ("PRICE", Some(ShopSort::Gold), 24),
            ("NOTE", None, 26),
            ("OFFERS", None, 9),
            ("ENDS", Some(ShopSort::Ends), 11),
        ],
        ShopTab::Sold => [
            ("ITEM", Some(ShopSort::Name), 30),
            ("PAID", Some(ShopSort::Gold), 26),
            ("BUYER", None, 18),
            ("FEE", None, 12),
            ("SOLD", None, 14),
        ],
        ShopTab::Expired => [
            ("ITEM", Some(ShopSort::Name), 30),
            ("PRICE", Some(ShopSort::Gold), 24),
            ("NOTE", None, 22),
            ("STATE", None, 13),
            ("ENDED", Some(ShopSort::Ends), 11),
        ],
    }
}

/// The columns' rectangles of a row at `y`.
fn cells(p: &Places, t: ShopTab, y: i32, h: i32) -> Vec<Rect> {
    let x0 = p.x0 + 8 + 18;
    let w = p.w - 16 - 18;
    let mut x = x0;
    columns(t)
        .iter()
        .map(|(_, _, share)| {
            let cw = w * share / 100;
            let r = Rect::new(x, y, cw as u16, h as u16);
            x += cw;
            r
        })
        .collect()
}

fn acts_y(p: &Places) -> i32 {
    p.h - ACTS_DY
}

fn act_rect(p: &Places, i: usize) -> Rect {
    p.r(8 + 84 * i as i32, acts_y(p), 80, 18)
}

/// The Set price... / Note... box.
pub fn box_rect(p: &Places) -> Rect {
    p.r((p.w - 328) / 2, (p.h - 170) / 2, 328, 170)
}

fn box_buttons(p: &Places) -> (Rect, Rect) {
    let b = box_rect(p);
    (
        Rect::new(b.x + 40, b.y + i32::from(b.h) - 28, 110, 20),
        Rect::new(b.x + 178, b.y + i32::from(b.h) - 28, 110, 20),
    )
}

fn box_builder(p: &Places) -> Point {
    let b = box_rect(p);
    Point::new(b.x + 12, b.y + 30)
}

fn box_note(p: &Places) -> Rect {
    let b = box_rect(p);
    Rect::new(b.x + 12, b.y + 50, b.w - 24, 16)
}

/// The tab's parts, bottom-most first: with a box open, only the box's.
pub fn spots(ui: &MarketUi, p: &Places) -> Vec<(ShopSpot, Rect)> {
    let s = &ui.shop;
    let mut v = Vec::new();
    if let Some(b) = &s.boxed {
        match b {
            ShopBox::Price(d) => {
                let cur = sell::currencies(ui);
                for (ps, r) in sell::price_spots(d, &cur, box_builder(p), 304) {
                    v.push((ShopSpot::BoxPrice(ps), r));
                }
            }
            ShopBox::Note(_) => v.push((ShopSpot::BoxNote, box_note(p))),
        }
        let (apply, cancel) = box_buttons(p);
        // Before the builder's picker, which covers them.
        v.insert(0, (ShopSpot::BoxApply, apply));
        v.insert(1, (ShopSpot::BoxCancel, cancel));
        return v;
    }
    for (i, t) in ShopTab::ALL.into_iter().enumerate() {
        v.push((ShopSpot::Tab(t), shop_tab(p, i)));
    }
    if !s.rows().is_empty() {
        v.push((ShopSpot::All, p.r(8, HEAD_Y, 14, 13)));
    }
    for ((_, sort, _), r) in columns(s.tab)
        .iter()
        .zip(cells(p, s.tab, p.y0 + HEAD_Y, 13))
    {
        if let Some(sort) = sort {
            v.push((ShopSpot::Sort(*sort), r));
        }
    }
    for (i, r) in row_rects(ui, p) {
        v.push((ShopSpot::Row(i), r));
    }
    for (i, a) in ShopAct::of_tab(s.tab).iter().enumerate() {
        if !s.targets(*a).is_empty() {
            v.push((ShopSpot::Act(*a), act_rect(p, i)));
        }
    }
    if s.pages() > 1 {
        v.push((ShopSpot::Prev, p.r(p.w - 78, acts_y(p), 18, 18)));
        v.push((ShopSpot::Next, p.r(p.w - 26, acts_y(p), 18, 18)));
    }
    v
}

/// A press on `s` (`right`: the right button).
pub fn click(ui: &mut MarketUi, s: ShopSpot, right: bool) {
    if !matches!(s, ShopSpot::BoxPrice(PriceSpot::Gold) | ShopSpot::BoxNote) {
        ui.focus = None;
    }
    match s {
        ShopSpot::Tab(t) => set_tab(ui, t),
        ShopSpot::All => {
            let ids: Vec<ListingId> = ui.shop.rows().iter().map(|r| r.row.listing).collect();
            let sel = &mut ui.shop.selected;
            if ids.iter().all(|id| sel.contains(id)) {
                sel.clear();
            } else {
                sel.extend(ids);
            }
        }
        ShopSpot::Row(i) => {
            if let Some(id) = ui.shop.rows().get(i).map(|r| r.row.listing) {
                if !ui.shop.selected.remove(&id) {
                    ui.shop.selected.insert(id);
                }
            }
        }
        ShopSpot::Sort(so) => {
            let sh = &mut ui.shop;
            if sh.sort == so {
                sh.desc = !sh.desc;
            } else {
                sh.sort = so;
                sh.desc = so == ShopSort::Posted;
            }
            sh.page = 0;
            refresh(ui);
        }
        ShopSpot::Act(a) => act(ui, a),
        ShopSpot::Prev | ShopSpot::Next => {
            let sh = &mut ui.shop;
            let page = if s == ShopSpot::Prev {
                sh.page.saturating_sub(1)
            } else {
                (sh.page + 1).min(sh.pages() - 1)
            };
            if page != sh.page {
                sh.page = page;
                refresh(ui);
            }
        }
        ShopSpot::BoxPrice(ps) => {
            if ps == PriceSpot::Gold {
                ui.focus = Some(TextBox::ShopGold);
            }
            let cur = sell::currencies(ui);
            if let Some(ShopBox::Price(d)) = ui.shop.boxed.as_mut() {
                sell::click_price(d, &cur, ps, right);
            }
        }
        ShopSpot::BoxNote => ui.focus = Some(TextBox::ShopNote),
        ShopSpot::BoxApply => apply_box(ui),
        ShopSpot::BoxCancel => {
            ui.shop.boxed = None;
            ui.focus = None;
        }
    }
}

/// The wheel over the rows.
pub fn wheel(ui: &mut MarketUi, p: &Places, steps: i32) {
    let max = ui.shop.rows().len().saturating_sub(visible(p));
    let t = ui.shop.scroll as i64 - i64::from(steps) * 3;
    ui.shop.scroll = t.clamp(0, max as i64) as usize;
}

/// "2 h", "4 d" or "today" (red, under a day) until `end`; how long ago for a
/// closed one.
fn ends(now: u64, end: u64) -> (String, u16) {
    const DAY: u64 = 24 * 3600 * 1000;
    if end <= now {
        return ("ended".into(), GREY);
    }
    let left = end - now;
    if left < DAY {
        ("today".into(), RED)
    } else {
        (format!("{} d", left / DAY), WHITE)
    }
}

/// The title of tab `t` with its count.
fn tab_title(ui: &MarketUi, t: ShopTab) -> String {
    match &ui.shop.view {
        Some(v) => format!("{} ({})", t.label(), v.counts[usize::from(t.code())]),
        None => t.label().to_owned(),
    }
}

/// Draws the tab.
pub fn draw(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen) = (i.ui, &i.places, &i.pen);
    let s = &ui.shop;
    let k = pen.kit;
    for (n, t) in ShopTab::ALL.into_iter().enumerate() {
        pen.button(shop_tab(p, n), &tab_title(ui, t), s.tab == t, out);
    }
    if let Some(v) = &s.view {
        let line = format!("{} of {} slots", v.slots_used, v.slots_max);
        pen.text_right(p.x0 + p.w - 10, p.y0 + 34, &line, SMALL, TAN, out);
    }
    let area = p.r(6, HEAD_Y - 2, p.w - 12, p.h - HEAD_Y - ACTS_DY - 2);
    pen.fill(area, k.field, out);
    pen.frame(area, k.edge, out);
    // The header: the all-rows tick, the titles.
    let rows = s.rows();
    let all = !rows.is_empty() && rows.iter().all(|r| s.selected.contains(&r.row.listing));
    if !rows.is_empty() {
        tick(pen, p.r(10, HEAD_Y + 1, 11, 11), all, out);
    }
    for ((title, sort, _), r) in columns(s.tab)
        .iter()
        .zip(cells(p, s.tab, p.y0 + HEAD_Y, 13))
    {
        let on = sort.is_some_and(|so| so == s.sort);
        let t = match (on, s.desc) {
            (true, true) => format!("{title} v"),
            (true, false) => format!("{title} ^"),
            _ => (*title).to_owned(),
        };
        let color = if on {
            GOLD
        } else if sort.is_some() && r.contains(pen.mouse) {
            WHITE
        } else {
            GREY
        };
        pen.line(r, &t, SMALL, color, out);
    }
    pen.hline(p.x0 + 8, p.x0 + p.w - 9, p.y0 + ROWS_Y - 2, k.edge, out);
    let now = s.view.as_ref().map_or(0, |v| v.now_ms);
    for (n, r) in row_rects(ui, p) {
        let Some(row) = rows.get(n) else { continue };
        let id = row.row.listing;
        let on = s.selected.contains(&id);
        let mark = s.marks.get(&id);
        if on {
            pen.fill(r, k.on, out);
        } else if r.contains(pen.mouse) {
            pen.fill(r, k.hit, out);
        }
        if mark.is_some() {
            pen.frame(r, k.gold, out);
        }
        tick(pen, Rect::new(r.x + 2, r.y + 1, 11, 11), on, out);
        let cs = cells(p, s.tab, r.y, ROW_H);
        let l = &row.row;
        let mut texts: Vec<(String, u16)> = vec![(l.item.name.clone(), l.group.color())];
        match s.tab {
            ShopTab::Sold => {
                let sold = row.sold.as_ref();
                texts.push((sold.map_or("-".into(), |x| price_text(ui, &x.paid)), GOLD));
                texts.push((sold.map_or("-".into(), |x| x.buyer.clone()), TAN));
                let fee = sold.map_or(0, |x| x.sale_fee);
                texts.push((
                    if fee > 0 {
                        format!("{fee} g")
                    } else {
                        "-".into()
                    },
                    GREY,
                ));
                let ago = sold.map_or(String::new(), |x| age(now, x.closed_ms));
                texts.push((ago, GREY));
            }
            _ => {
                let price = match &l.price {
                    Some(pr) => (price_text(ui, pr), GOLD),
                    None => ("offers".into(), GREY),
                };
                texts.push(price);
                texts.push(match mark {
                    Some(c) => (refusal(c), RED),
                    None if l.note.is_empty() => (String::new(), GREY),
                    None => (format!("\"{}\"", l.note), TAN),
                });
                if s.tab == ShopTab::Expired {
                    let state = match (row.state, row.relistable) {
                        (ListingState::Cancelled, _) => "cancelled",
                        (_, true) => "expired",
                        _ => "gone",
                    };
                    texts.push((state.into(), GREY));
                    texts.push((age(now, l.expires_ms), GREY));
                } else {
                    let offers = if row.live_offers > 0 {
                        (format!("{} new", row.live_offers), GOLD)
                    } else {
                        ("-".into(), GREY)
                    };
                    texts.push(offers);
                    texts.push(ends(now, l.expires_ms));
                }
            }
        }
        for (c, (t, color)) in cs.into_iter().zip(texts) {
            pen.line(c, &t, SMALL, color, out);
        }
    }
    if s.view.is_some() && rows.is_empty() {
        let r = p.r(8, ROWS_Y + 8, p.w - 16, 14);
        let empty = match s.tab {
            ShopTab::Shown => "Nothing listed. Sell from the Market category of your stash.",
            ShopTab::Hidden => "No hidden listings.",
            ShopTab::Sold => "Nothing sold in the last 30 days.",
            ShopTab::Expired => "Nothing expired or cancelled in the last 30 days.",
        };
        pen.label(r, empty, SMALL, TAN, out);
    }
    // The buttons, greyed with nothing ticked they act on.
    for (n, a) in ShopAct::of_tab(s.tab).iter().enumerate() {
        let r = act_rect(p, n);
        if s.targets(*a).is_empty() {
            pen.fill(r, k.field, out);
            pen.frame(r, k.edge, out);
            pen.label(r, a.label(), SMALL, GREY, out);
        } else {
            pen.button(r, a.label(), false, out);
        }
    }
    let n = ShopAct::of_tab(s.tab).len() as i32;
    let picked = format!("{} selected", s.selected.len());
    pen.text(
        p.x0 + 8 + 84 * n + 4,
        p.y0 + acts_y(p) + 5,
        &picked,
        SMALL,
        TAN,
        out,
    );
    if s.pages() > 1 {
        pen.button(p.r(p.w - 78, acts_y(p), 18, 18), "<", false, out);
        let pages = format!("{}/{}", s.page + 1, s.pages());
        pen.label(p.r(p.w - 58, acts_y(p), 30, 18), &pages, SMALL, TAN, out);
        pen.button(p.r(p.w - 26, acts_y(p), 18, 18), ">", false, out);
    }
    let fees = ui.state.as_ref().map(|st| st.fees).unwrap_or_default();
    let line = s.line.clone().or_else(|| {
        sell::fee_lines(&fees)
            .first()
            .map(|l| (format!("Relist: {l}"), false))
    });
    if let Some((l, refused)) = line {
        let r = p.r(8, p.h - 22, p.w - 16, 14);
        pen.line(r, &l, SMALL, if refused { RED } else { GOLD }, out);
    }
    draw_box(i, out);
}

/// A refusal's short text on its row.
fn refusal(c: &MarketCode) -> String {
    c.text(0)
        .unwrap_or_else(|| format!("Refused ({})", c.code()))
}

/// A tick box.
fn tick(pen: &super::window::Pen, r: Rect, on: bool, out: &mut dyn UiDrawSink) {
    pen.fill(r, pen.kit.field, out);
    pen.frame(r, if on { pen.kit.gold } else { pen.kit.edge }, out);
    if on {
        pen.label(r, "x", SMALL, GOLD, out);
    }
}

fn draw_box(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen) = (i.ui, &i.places, &i.pen);
    let Some(b) = &ui.shop.boxed else { return };
    let k = pen.kit;
    let r = box_rect(p);
    pen.fill(r, k.panel, out);
    pen.frame(r, k.gold, out);
    let n = ui.shop.selected.len();
    let title = match b {
        ShopBox::Price(_) => format!("Set the price of {n} listing(s)"),
        ShopBox::Note(_) => format!("Note on {n} listing(s)"),
    };
    pen.text(r.x + 12, r.y + 8, &title, LABEL, GOLD, out);
    match b {
        ShopBox::Price(d) => sell::draw_price(
            pen,
            ui,
            d,
            box_builder(p),
            304,
            ui.focus == Some(TextBox::ShopGold),
            out,
        ),
        ShopBox::Note(t) => {
            pen.text_box(
                box_note(p),
                t,
                "Public note (empty: none)",
                ui.focus == Some(TextBox::ShopNote),
                out,
            );
        }
    }
    let (apply, cancel) = box_buttons(p);
    pen.button(apply, "Apply", false, out);
    pen.button(cancel, "Cancel", false, out);
}
