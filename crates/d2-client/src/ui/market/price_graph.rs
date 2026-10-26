// D2RUST's own: the market's price check.
//! The price check (module K5 of `docs/plans/market-ui.md`): what items
//! like the checked one sold for lately, as a graph over time with one
//! series per currency they were paid in. Nothing is converted between
//! currencies: a series paid in gold is drawn on a gold axis, one paid in
//! one rune on that rune's axis, each in its own strip with its own scale,
//! over one shared time axis. A bundle ("1 Rune 23 + 1 Rune 24") is drawn
//! as points, its full price on hover. Beside the graph: what is listed
//! now (lowest and median per currency), how many sold in the span, and
//! the last ten sales. Under five sales the check says "not enough data"
//! instead of drawing.
//!
//! It opens from the details panel's "Price check" button as a box over
//! the window. The New listing form ([`super::sell`]) checks the item on
//! its Sell slot as it lands ([`MarketUi::check_sell_line`]) for its
//! "Similar active / last sold" line ([`similar_line`], all of it on
//! hover), and its Price check opens the box over the form. The
//! span is 30 days by default; 2, 7 and 90 days are a click away. The
//! answer comes from the realm (`MarketPrices`, 0xC8), so the values stay
//! as sent and the drawing can change without a wire change.

use crate::market::prices::{
    PriceHistory, PriceOf, PriceSeries, PriceShape, Spread, DAYS, DEFAULT_DAYS, HOUR_MS, MIN_SALES,
};
use crate::market::source::{MarketDone, Price, Req};
use crate::ui::draw::{LineRequest, UiDraw, UiDrawSink};
use crate::ui::geom::Rect;
use crate::ui::item_tip::{TipAnchor, TipLine};
use crate::ui::panel::{PointerButton, UiEvent};

use super::details::{age, coin_name, thousands};
use super::window::{DrawIn, Pen, Places, Spot, GOLD, GREY, LABEL, SMALL, TAN, WHITE};
use super::MarketUi;

/// The series' colours in strip order: the line's RGB, matched in the act
/// palette, and the text colour (string tables' colour code) of its name.
const SERIES: [((u8, u8, u8), u16); 6] = [
    ((0xD8, 0xB8, 0x58), 4),
    ((0xE8, 0x80, 0x28), 8),
    ((0x68, 0x88, 0xF0), 3),
    ((0x40, 0xC0, 0x40), 2),
    ((0xB0, 0x70, 0xE0), 11),
    ((0xE0, 0xE0, 0xE0), 0),
];

/// The graph's palette entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inks {
    pub series: [u8; SERIES.len()],
    /// The min-max whiskers and the "lowest now" rule.
    pub muted: u8,
}

impl Inks {
    /// The nearest palette entries of the graph's colours.
    pub fn of(palette: &[[u8; 3]]) -> Option<Inks> {
        let n = |c: (u8, u8, u8)| crate::ui::inv_grid::nearest_index(palette, c);
        let mut series = [0; SERIES.len()];
        for (s, (rgb, _)) in series.iter_mut().zip(SERIES) {
            *s = n(rgb)?;
        }
        Some(Inks {
            series,
            muted: n((0x70, 0x68, 0x58))?,
        })
    }
}

/// The price check's state in the window.
#[derive(Clone, Debug, Default)]
pub struct PriceCheck {
    /// The box is shown.
    pub open: bool,
    /// What is checked.
    pub of: Option<PriceOf>,
    /// The Sell slot's stash item, when the check is of it (the sell
    /// form's line reads it).
    pub stash: Option<u32>,
    /// The checked item's name and its text colour; the line under it.
    pub title: String,
    pub title_color: u16,
    pub sub: String,
    /// The span in days (0 before the first check: the default).
    pub days: u16,
    /// An ask waits to go out; then its number.
    pub queued: bool,
    pub req: Option<Req>,
    /// The answer shown.
    pub history: Option<PriceHistory>,
    /// Why the check failed.
    pub error: Option<String>,
    /// The graph's colours, once the palette is known.
    pub inks: Option<Inks>,
}

impl PriceCheck {
    /// The span in force.
    pub fn span(&self) -> u16 {
        if DAYS.contains(&self.days) {
            self.days
        } else {
            DEFAULT_DAYS
        }
    }
}

impl MarketUi {
    /// Opens the price check of `of`, shown as `title` (in text colour
    /// `color`) over `sub`, over the span last chosen.
    pub fn check_prices(&mut self, of: PriceOf, title: &str, color: u16, sub: &str) {
        let p = &mut self.prices;
        p.days = p.span();
        p.open = true;
        p.of = Some(of);
        p.title = title.to_owned();
        p.title_color = color;
        p.sub = sub.to_owned();
        p.history = None;
        p.stash = None;
        self.confirm = None;
        self.form = None;
        self.offers.deal = None;
        self.ask_prices();
    }

    /// The details panel's Price check: the shown listing's.
    pub fn check_selected(&mut self) {
        let Some(l) = self.selected.clone() else {
            return;
        };
        let sub = if l.item.base.is_empty() || l.item.base == l.item.name {
            l.group.label().to_owned()
        } else {
            format!("{} · {}", l.group.label(), l.item.base)
        };
        self.check_prices(
            PriceOf::Listing(l.listing),
            &l.item.name,
            l.group.color(),
            &sub,
        );
    }

    /// The Sell slot's item is checked for the form's "Similar active"
    /// line (the box stays shut): asked when it is new on the slot, or when
    /// a check of something else or a dropped one took its place. The
    /// window, shown, has the check to itself.
    pub fn check_sell_line(&mut self) {
        if self.shown() {
            return;
        }
        let Some(it) = &self.sell.item else {
            return;
        };
        let p = &self.prices;
        let asked = p.queued || p.req.is_some() || p.history.is_some() || p.error.is_some();
        if p.stash == Some(it.id) && asked {
            return;
        }
        let (id, group, title) = (it.id, it.group, it.name.clone());
        let p = &mut self.prices;
        p.days = p.span();
        p.of = Some(PriceOf::StashItem(id));
        p.stash = Some(id);
        p.title = title;
        p.title_color = group.color();
        p.sub = format!("{} · in your shared stash", group.label());
        p.history = None;
        self.ask_prices();
    }

    /// The sell form's Price check: the box over the form, of the Sell
    /// slot's item.
    pub fn check_sell_item(&mut self) {
        self.check_sell_line();
        if self.sell.item.is_some() {
            self.prices.open = true;
        }
    }

    /// Another span: asked again by the key the first answer gave.
    pub fn price_days(&mut self, days: u16) {
        if !DAYS.contains(&days) || days == self.prices.span() {
            return;
        }
        self.prices.days = days;
        if let Some(h) = &self.prices.history {
            self.prices.of = Some(PriceOf::Key(h.key.clone()));
        }
        self.ask_prices();
    }

    /// Closes the box; a late answer is dropped.
    pub fn close_prices(&mut self) {
        let p = &mut self.prices;
        p.open = false;
        p.queued = false;
        p.req = None;
        self.asks
            .retain(|a| !matches!(a, crate::market::source::MarketAsk::Prices { .. }));
    }

    fn ask_prices(&mut self) {
        let Some(of) = self.prices.of.clone() else {
            return;
        };
        use crate::market::source::MarketAsk;
        // Only the newest check counts.
        self.asks.retain(|a| !matches!(a, MarketAsk::Prices { .. }));
        self.asks.push(MarketAsk::Prices {
            of,
            days: self.prices.span(),
        });
        self.prices.queued = true;
        self.prices.req = None;
        self.prices.error = None;
    }

    /// The check went out as `req`.
    pub(super) fn prices_sent(&mut self, req: Req) {
        self.prices.queued = false;
        self.prices.req = Some(req);
    }

    /// The realm's answer to request `req`.
    pub(super) fn prices_answer(&mut self, req: Req, h: PriceHistory) {
        let p = &mut self.prices;
        if p.queued || p.req != Some(req) {
            return;
        }
        p.req = None;
        p.history = Some(h);
    }

    /// Whether `d` ends the check in flight; it then shows why.
    pub(super) fn prices_done(&mut self, d: &MarketDone) -> bool {
        if self.prices.req != Some(d.req) {
            return false;
        }
        self.prices.req = None;
        self.prices.error = Some(d.line());
        true
    }
}

/// A currency's name: "gold", else the game's name of the stackable.
pub fn currency_name<'a>(ui: &'a MarketUi, code: &'a str) -> &'a str {
    if code.is_empty() {
        "gold"
    } else {
        coin_name(ui, code)
    }
}

/// `n` of currency `code`, exactly: "280,000 gold", "2 Rune 24".
pub fn amount(ui: &MarketUi, code: &str, n: u64) -> String {
    format!("{} {}", thousands(n), currency_name(ui, code))
}

/// Per-currency amounts as one price: "1 Rune 23 + 1 Rune 24".
pub fn amounts(ui: &MarketUi, currencies: &[String], values: &[u64]) -> String {
    currencies
        .iter()
        .zip(values)
        .map(|(c, n)| amount(ui, c, *n))
        .collect::<Vec<_>>()
        .join(" + ")
}

/// A series' name: its currencies joined ("Rune 23 + Rune 24").
pub fn series_name(ui: &MarketUi, s: &PriceSeries) -> String {
    match s.shape {
        PriceShape::WithItems => "with items".into(),
        _ => s
            .currencies
            .iter()
            .map(|c| currency_name(ui, c))
            .collect::<Vec<_>>()
            .join(" + "),
    }
}

/// A sale's price: the price, items counted.
fn sale_text(ui: &MarketUi, p: &Price) -> String {
    super::details::price_text(ui, p)
}

/// The sell form's line (mock 2): "Similar active: 4 listings, lowest 1
/// Rune 24 · last sold: 1 Rune 23 + 1 Rune 24".
pub fn similar_line(ui: &MarketUi, h: &PriceHistory) -> String {
    let mut s = format!(
        "Similar active: {} listing{}",
        h.listed_now,
        if h.listed_now == 1 { "" } else { "s" }
    );
    if let Some(top) = h
        .series
        .iter()
        .filter(|s| s.listed_now > 0 && !s.lowest_now.is_empty())
        .max_by_key(|s| s.listed_now)
    {
        s.push_str(&format!(
            ", lowest {}",
            amounts(ui, &top.currencies, &top.lowest_now)
        ));
    }
    if let Some(last) = h.recent.first() {
        s.push_str(&format!(" · last sold: {}", sale_text(ui, &last.price)));
    }
    s
}

/// The check of the Sell slot's item, once the realm answered.
pub fn sell_history(ui: &MarketUi) -> Option<&PriceHistory> {
    let it = ui.sell.item.as_ref()?;
    let p = &ui.prices;
    p.history.as_ref().filter(|_| p.stash == Some(it.id))
}

/// The sell form's line under the mouse: all the line says, per series,
/// and the last sales.
pub fn similar_tip(ui: &MarketUi, h: &PriceHistory) -> Vec<String> {
    let mut v = vec![format!(
        "Similar active: {} listing{}",
        h.listed_now,
        if h.listed_now == 1 { "" } else { "s" }
    )];
    let mut priced = 0;
    for s in h.series.iter().filter(|s| s.listed_now > 0) {
        priced += s.listed_now;
        let mut l = format!("{}: {} listed", series_name(ui, s), s.listed_now);
        if !s.lowest_now.is_empty() {
            l.push_str(&format!(
                ", lowest {}",
                amounts(ui, &s.currencies, &s.lowest_now)
            ));
        }
        if !s.median_now.is_empty() {
            l.push_str(&format!(
                ", median {}",
                amounts(ui, &s.currencies, &s.median_now)
            ));
        }
        v.push(l);
    }
    if h.listed_now > priced {
        v.push(format!("{} take offers only", h.listed_now - priced));
    }
    v.push(format!("Sold in the last {} days: {}", h.days, h.sold));
    // Commas: the tip font has no middle dot.
    for s in h.recent.iter().take(3) {
        let when = match age(h.now_ms, s.at_ms).as_str() {
            "now" => "just now".to_owned(),
            a => format!("{a} ago"),
        };
        v.push(format!("Sold {}, {when}", sale_text(ui, &s.price)));
    }
    v.push("Price check: the graph per currency".into());
    v
}

/// What the box does with an event while it stands over the sell form
/// (the window routes its own): it takes every press and Esc, which
/// shuts it; the line keeps its answer.
pub fn form_event(ui: &mut MarketUi, p: &Places, e: UiEvent, esc: bool) -> bool {
    match e {
        UiEvent::Press {
            button: PointerButton::Left,
            at,
        } => {
            let hit = spots(ui, p).into_iter().rev().find(|(_, r)| r.contains(at));
            match hit.map(|(s, _)| s) {
                Some(Spot::PriceClose) => ui.prices.open = false,
                Some(Spot::PriceDays(d)) => ui.price_days(d),
                _ => {}
            }
            true
        }
        UiEvent::Press { .. } | UiEvent::Release { .. } | UiEvent::Wheel { .. } => true,
        UiEvent::Action(_) if esc => {
            ui.prices.open = false;
            true
        }
        UiEvent::Action(_) | UiEvent::Char(_) => false,
        UiEvent::CursorMoved(_) | UiEvent::CursorLeft => false,
    }
}

/// How a strip draws its series.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// One currency: the medians joined by a line, min-max whiskers.
    Line,
    /// A bundle: points only.
    Points,
}

/// One strip of the graph: one series, its own axis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Strip {
    /// Its series' place in the answer.
    pub series: usize,
    pub mark: Mark,
    /// Its colour slot ([`Inks::series`], [`SERIES`]).
    pub slot: usize,
    /// The axis's ends in the series' currency (a line's only), both
    /// included.
    pub lo: u64,
    pub hi: u64,
}

/// What the graph draws: the strips, and what it leaves out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plot {
    pub strips: Vec<Strip>,
    /// Series with sales not drawn for room.
    pub more: usize,
    /// Sales with items (counted, no values).
    pub with_items: u32,
}

/// The graph of `h` in at most `room` strips: every series with sales in
/// one currency or a bundle, most sold first.
pub fn plot(h: &PriceHistory, room: usize) -> Plot {
    let mut out = Plot::default();
    for (n, s) in h.series.iter().enumerate() {
        if s.shape == PriceShape::WithItems {
            out.with_items += s.sold;
            continue;
        }
        if s.sold == 0 || s.buckets.is_empty() {
            continue;
        }
        if out.strips.len() >= room {
            out.more += 1;
            continue;
        }
        let mark = if s.shape == PriceShape::One {
            Mark::Line
        } else {
            Mark::Points
        };
        let (lo, hi) = if mark == Mark::Line { axis(s) } else { (0, 0) };
        let slot = out.strips.len() % SERIES.len();
        out.strips.push(Strip {
            series: n,
            mark,
            slot,
            lo,
            hi,
        });
    }
    out
}

/// A one-currency series' axis: its lowest and highest sale, widened
/// when they are one value. (The lowest price listed now is drawn only
/// when it falls inside: an odd listing must not flatten the sales.)
pub fn axis(s: &PriceSeries) -> (u64, u64) {
    let vals = s
        .buckets
        .iter()
        .filter_map(|b| b.values.first())
        .flat_map(|v| [v.min, v.max]);
    let (mut lo, mut hi) = vals.fold((u64::MAX, 0), |(lo, hi), v| (lo.min(v), hi.max(v)));
    if lo > hi {
        return (0, 1);
    }
    if lo == hi {
        let pad = (lo / 10).max(1);
        lo = lo.saturating_sub(pad);
        hi += pad;
    }
    (lo, hi)
}

/// A drawn part of a strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// The line between two buckets' medians.
    Join { x0: i32, y0: i32, x1: i32, y1: i32 },
    /// A bucket's min-max whisker.
    Whisker { x: i32, top: i32, bottom: i32 },
    /// A bucket's median (a line) or the bundle's point; `bucket` is its
    /// place in the series.
    Dot { x: i32, y: i32, bucket: usize },
    /// The lowest price listed now, across the strip.
    Now { y: i32 },
}

/// The x of a moment `age_ms` before the answer's now, in `r` (now at the
/// right edge).
pub fn x_of(h: &PriceHistory, r: Rect, age_ms: u64) -> i32 {
    let span = h.span_ms().max(1);
    let w = i64::from(r.w.saturating_sub(1));
    let left = (age_ms.min(span) as i64 * w / span as i64) as i32;
    r.x + i32::from(r.w.saturating_sub(1)) - left
}

/// The y of value `v` on a strip's axis, in `r` (`hi` on top).
pub fn y_of(st: &Strip, r: Rect, v: u64) -> i32 {
    let h = i64::from(r.h.saturating_sub(1));
    let span = (st.hi - st.lo).max(1) as i64;
    let up = (v.clamp(st.lo, st.hi) - st.lo) as i64 * h / span;
    r.y + i32::from(r.h.saturating_sub(1)) - up as i32
}

/// The shapes of strip `st` of `h` drawn in `r`, oldest bucket first.
pub fn shapes(h: &PriceHistory, st: &Strip, r: Rect) -> Vec<Shape> {
    let s = &h.series[st.series];
    let mut order: Vec<usize> = (0..s.buckets.len()).collect();
    order.sort_by_key(|&k| std::cmp::Reverse(s.buckets[k].index));
    let mut out = Vec::new();
    if st.mark == Mark::Line {
        if let Some(&low) = s
            .lowest_now
            .first()
            .filter(|&&v| (st.lo..=st.hi).contains(&v))
        {
            out.push(Shape::Now {
                y: y_of(st, r, low),
            });
        }
    }
    let mut last: Option<(i32, i32)> = None;
    for k in order {
        let b = &s.buckets[k];
        let x = x_of(h, r, h.bucket_age_ms(b.index));
        match st.mark {
            Mark::Line => {
                let v = b.values.first().copied().unwrap_or_default();
                let y = y_of(st, r, v.median);
                if v.max > v.min {
                    out.push(Shape::Whisker {
                        x,
                        top: y_of(st, r, v.max),
                        bottom: y_of(st, r, v.min),
                    });
                }
                if let Some((x0, y0)) = last {
                    out.push(Shape::Join {
                        x0,
                        y0,
                        x1: x,
                        y1: y,
                    });
                }
                out.push(Shape::Dot { x, y, bucket: k });
                last = Some((x, y));
            }
            Mark::Points => {
                let y = r.y + i32::from(r.h) / 2;
                out.push(Shape::Dot { x, y, bucket: k });
            }
        }
    }
    out
}

/// "now", "5h ago", "12 d ago" for a bucket of `h`.
pub fn bucket_when(h: &PriceHistory, index: u32) -> String {
    if index == 0 {
        return if h.bucket_ms <= HOUR_MS {
            "this hour".into()
        } else {
            "today".into()
        };
    }
    if h.bucket_ms <= HOUR_MS {
        format!("{index}h ago")
    } else if index == 1 {
        "yesterday".into()
    } else {
        format!("{index} days ago")
    }
}

/// The tip of bucket `k` of series `n`: when, how many sales, and the
/// prices exactly (a bundle's in full).
pub fn bucket_tip(
    ui: &MarketUi,
    h: &PriceHistory,
    n: usize,
    k: usize,
    slot: usize,
) -> Vec<TipLine> {
    let s = &h.series[n];
    let b = &s.buckets[k];
    let mut v = vec![
        TipLine::new(series_name(ui, s), SERIES[slot % SERIES.len()].1),
        TipLine::new(
            format!(
                // The tip's font has no middle dot.
                "{}, {} sale{}",
                bucket_when(h, b.index),
                b.sales,
                if b.sales == 1 { "" } else { "s" }
            ),
            WHITE,
        ),
    ];
    let pick = |f: fn(&Spread) -> u64| -> Vec<u64> { b.values.iter().map(f).collect() };
    let (lo, med, hi) = (pick(|x| x.min), pick(|x| x.median), pick(|x| x.max));
    if b.sales == 1 || lo == hi {
        v.push(TipLine::new(amounts(ui, &s.currencies, &med), GOLD));
    } else {
        v.push(TipLine::new(
            format!("median {}", amounts(ui, &s.currencies, &med)),
            GOLD,
        ));
        v.push(TipLine::new(
            format!("lowest {}", amounts(ui, &s.currencies, &lo)),
            TAN,
        ));
        v.push(TipLine::new(
            format!("highest {}", amounts(ui, &s.currencies, &hi)),
            TAN,
        ));
    }
    v
}

/// The box over the window.
pub fn box_rect(p: &Places) -> Rect {
    let w = 600.min(p.w - 20);
    let h = 336.min(p.h - 30);
    p.r((p.w - w) / 2, (p.h - h) / 2, w, h)
}

/// The details panel's Price check button, above Buy.
pub fn button(p: &Places) -> Rect {
    let b = p.buy();
    Rect::new(b.x, b.y - 22, b.w, b.h)
}

pub fn close_rect(p: &Places) -> Rect {
    let b = box_rect(p);
    Rect::new(b.x + i32::from(b.w) - 22, b.y + 4, 18, 18)
}

/// The left column: name, listed now, sold, the last sales.
fn column(p: &Places) -> Rect {
    let b = box_rect(p);
    Rect::new(b.x + 10, b.y + 26, 184, b.h - 36)
}

/// Span chip `i` ([`DAYS`]).
pub fn days_rect(p: &Places, i: usize) -> Rect {
    let b = box_rect(p);
    let right = b.x + i32::from(b.w) - 10;
    Rect::new(right - 40 * (DAYS.len() - i) as i32, b.y + 26, 38, 14)
}

/// The graph's area: the strips, then the time axis and a foot line.
pub fn graph_rect(p: &Places) -> Rect {
    let b = box_rect(p);
    let x = b.x + 204;
    Rect::new(
        x,
        b.y + 46,
        (b.x + i32::from(b.w) - 10 - x) as u16,
        b.h - 46 - 36,
    )
}

/// The left margin of a strip that holds its axis numbers.
const AXIS_W: i32 = 46;
/// A strip's header row.
const HEAD_H: i32 = 11;
/// The most strips the graph shows.
pub const MAX_STRIPS: usize = 4;

/// Strip `k` of `n` in the graph: its whole rectangle and its plot area.
pub fn strip_rects(p: &Places, k: usize, n: usize) -> (Rect, Rect) {
    let g = graph_rect(p);
    let n = n.max(1) as i32;
    let sh = i32::from(g.h) / n;
    let whole = Rect::new(g.x, g.y + sh * k as i32, g.w, (sh - 4).max(0) as u16);
    let plot = Rect::new(
        whole.x + AXIS_W,
        whole.y + HEAD_H + 2,
        (i32::from(whole.w) - AXIS_W).max(0) as u16,
        (i32::from(whole.h) - HEAD_H - 4).max(0) as u16,
    );
    (whole, plot)
}

/// The box's parts: its buttons, and the graph's points (hover targets
/// a driver can find by id).
pub fn spots(ui: &MarketUi, p: &Places) -> Vec<(Spot, Rect)> {
    let mut v = vec![(Spot::PriceClose, close_rect(p))];
    for (i, d) in DAYS.into_iter().enumerate() {
        v.push((Spot::PriceDays(d), days_rect(p, i)));
    }
    for (r, s, b, _) in targets(ui, p) {
        v.push((Spot::PricePoint(s, b), r));
    }
    v
}

/// The dots of the graph as hover targets: rectangle, series, bucket,
/// colour slot.
fn targets(ui: &MarketUi, p: &Places) -> Vec<(Rect, usize, usize, usize)> {
    let Some(h) = ui.prices.history.as_ref().filter(|h| h.enough()) else {
        return Vec::new();
    };
    let pl = plot(h, MAX_STRIPS);
    let mut v = Vec::new();
    for (k, st) in pl.strips.iter().enumerate() {
        let (_, r) = strip_rects(p, k, pl.strips.len());
        for s in shapes(h, st, r) {
            if let Shape::Dot { x, y, bucket } = s {
                v.push((Rect::new(x - 3, y - 3, 7, 7), st.series, bucket, st.slot));
            }
        }
    }
    v
}

/// The tip of the graph point under the mouse.
pub fn hover_tip(i: &DrawIn) -> Option<(Vec<TipLine>, TipAnchor)> {
    let ui = i.ui;
    let h = ui.prices.history.as_ref()?;
    let (r, n, k, slot) = targets(ui, &i.places)
        .into_iter()
        .rev()
        .find(|(r, ..)| r.contains(i.pen.mouse))?;
    let anchor = TipAnchor {
        x: r.x + i32::from(r.w) / 2,
        top: r.y,
        bottom: r.y + i32::from(r.h),
    };
    Some((bucket_tip(ui, h, n, k, slot), anchor))
}

/// A palette line.
fn seg(x0: i32, y0: i32, x1: i32, y1: i32, color: u8, out: &mut dyn UiDrawSink) {
    out.push(UiDraw::Line(LineRequest {
        x0,
        y0,
        x1,
        y1,
        color,
    }));
}

/// The graph's colour of slot `slot` (the kit's gold before the palette).
fn ink(ui: &MarketUi, pen: &Pen, slot: usize) -> u8 {
    ui.prices
        .inks
        .map_or(pen.kit.gold, |k| k.series[slot % SERIES.len()])
}

fn muted(ui: &MarketUi, pen: &Pen) -> u8 {
    ui.prices.inks.map_or(pen.kit.edge, |k| k.muted)
}

/// Draws the box, when open.
pub fn draw(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen) = (i.ui, &i.places, &i.pen);
    let pc = &ui.prices;
    if !pc.open {
        return;
    }
    let k = pen.kit;
    let b = box_rect(p);
    pen.fill(b, k.panel, out);
    pen.frame(b, k.gold, out);
    pen.frame(Rect::new(b.x + 2, b.y + 2, b.w - 4, b.h - 4), k.edge, out);
    pen.label(
        Rect::new(b.x, b.y + 6, b.w, 14),
        "Price check",
        LABEL,
        GOLD,
        out,
    );
    pen.button(close_rect(p), "X", false, out);
    for (n, d) in DAYS.into_iter().enumerate() {
        pen.button(days_rect(p, n), &format!("{d} d"), pc.span() == d, out);
    }
    draw_column(i, out);
    let g = graph_rect(p);
    pen.fill(g, k.field, out);
    pen.frame(g, k.edge, out);
    let say = |lines: &[(&str, u16)], out: &mut dyn UiDrawSink| {
        let top = g.y + i32::from(g.h) / 2 - 7 * lines.len() as i32;
        for (n, (s, c)) in lines.iter().enumerate() {
            let r = Rect::new(g.x, top + 14 * n as i32, g.w, 12);
            pen.label(r, s, SMALL, *c, out);
        }
    };
    if let Some(e) = &pc.error {
        say(&[(e, GOLD)], out);
        return;
    }
    let Some(h) = &pc.history else {
        say(&[("Checking prices...", TAN)], out);
        return;
    };
    if !h.enough() {
        let line = format!(
            "{} sale{} in the last {} days; a graph needs {MIN_SALES}.",
            h.sold,
            if h.sold == 1 { "" } else { "s" },
            h.days
        );
        let mut lines = vec![("Not enough data", GOLD), (line.as_str(), TAN)];
        if h.days < DAYS[DAYS.len() - 1] {
            lines.push(("A longer span may have more.", GREY));
        }
        say(&lines, out);
        return;
    }
    let pl = plot(h, MAX_STRIPS);
    if pl.strips.is_empty() {
        say(&[("Only sales with items: no prices to draw.", TAN)], out);
        return;
    }
    for (n, st) in pl.strips.iter().enumerate() {
        draw_strip(i, h, st, strip_rects(p, n, pl.strips.len()), out);
    }
    // The time axis under the strips.
    let (_, last) = strip_rects(p, pl.strips.len() - 1, pl.strips.len());
    let y = g.y + i32::from(g.h) + 3;
    let (unit, n) = if h.bucket_ms <= HOUR_MS {
        ("h", u64::from(h.days) * 24)
    } else {
        (" d", u64::from(h.days))
    };
    pen.text(last.x, y, &format!("{n}{unit} ago"), SMALL, GREY, out);
    let mid = format!("{}{unit}", n / 2);
    let mx = last.x + i32::from(last.w) / 2 - pen.width(SMALL, &mid) / 2;
    pen.text(mx, y, &mid, SMALL, GREY, out);
    pen.text_right(last.x + i32::from(last.w), y, "now", SMALL, GREY, out);
    let mut foot = vec!["Never converted between currencies; bundles are points".to_owned()];
    if pl.more > 0 {
        foot.push(format!("{} more series", pl.more));
    }
    let foot = pen.fit(SMALL, &foot.join(" · "), i32::from(g.w));
    pen.text(g.x, y + 12, &foot, SMALL, GREY, out);
}

/// One strip: its name and sales, its axis numbers, its frame and marks.
fn draw_strip(
    i: &DrawIn,
    h: &PriceHistory,
    st: &Strip,
    rects: (Rect, Rect),
    out: &mut dyn UiDrawSink,
) {
    let (ui, pen) = (i.ui, &i.pen);
    let (whole, r) = rects;
    let s = &h.series[st.series];
    let color = SERIES[st.slot % SERIES.len()].1;
    let name = format!(
        "{} · {} sold{}",
        series_name(ui, s),
        s.sold,
        if st.mark == Mark::Points {
            " (bundle)"
        } else {
            ""
        }
    );
    let name = pen.fit(SMALL, &name, i32::from(whole.w));
    pen.text(whole.x, whole.y + 1, &name, SMALL, color, out);
    pen.frame(r, pen.kit.edge, out);
    let ink = ink(ui, pen, st.slot);
    let muted = muted(ui, pen);
    if st.mark == Mark::Line {
        let ax = r.x - 4;
        pen.text_right(ax, r.y, &thousands(st.hi), SMALL, TAN, out);
        pen.text_right(
            ax,
            r.y + i32::from(r.h) - 8,
            &thousands(st.lo),
            SMALL,
            TAN,
            out,
        );
    } else {
        pen.text_right(
            r.x - 4,
            r.y + i32::from(r.h) / 2 - 4,
            "hover",
            SMALL,
            GREY,
            out,
        );
    }
    let right = r.x + i32::from(r.w) - 2;
    for sh in shapes(h, st, r) {
        match sh {
            Shape::Now { y } => {
                let mut x = r.x + 1;
                while x < right {
                    seg(x, y, (x + 2).min(right), y, muted, out);
                    x += 5;
                }
            }
            Shape::Whisker { x, top, bottom } => seg(x, top, x, bottom, muted, out),
            Shape::Join { x0, y0, x1, y1 } => seg(x0, y0, x1, y1, ink, out),
            Shape::Dot { x, y, .. } => {
                pen.fill(Rect::new(x - 1, y - 1, 3, 3), ink, out);
            }
        }
    }
}

/// The left column: the item, what is listed now, what sold, the last
/// sales.
fn draw_column(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, pen) = (i.ui, &i.pen);
    let pc = &ui.prices;
    let c = column(&i.places);
    let (x, w) = (c.x, i32::from(c.w));
    let bottom = c.y + i32::from(c.h);
    let mut y = c.y;
    let title = pc
        .history
        .as_ref()
        .map_or(pc.title.as_str(), |h| h.label.as_str());
    pen.text(x, y, &pen.fit(LABEL, title, w), LABEL, pc.title_color, out);
    y += 14;
    if !pc.sub.is_empty() {
        pen.text(x, y, &pen.fit(SMALL, &pc.sub, w), SMALL, GREY, out);
        y += 10;
    }
    let Some(h) = &pc.history else { return };
    let pl = plot(h, MAX_STRIPS);
    let slot_of = |n: usize| pl.strips.iter().find(|s| s.series == n).map(|s| s.slot);
    let line = |y: &mut i32, s: &str, color: u16, out: &mut dyn UiDrawSink| {
        if *y + 10 <= bottom {
            pen.text(x, *y, &pen.fit(SMALL, s, w), SMALL, color, out);
        }
        *y += 10;
    };
    y += 4;
    line(&mut y, &format!("LISTED NOW ({})", h.listed_now), GREY, out);
    let mut priced = 0;
    for (n, s) in h.series.iter().enumerate() {
        if s.listed_now == 0 || s.lowest_now.is_empty() {
            continue;
        }
        priced += s.listed_now;
        let color = slot_of(n).map_or(TAN, |k| SERIES[k % SERIES.len()].1);
        line(
            &mut y,
            &format!("{} · {}", series_name(ui, s), s.listed_now),
            color,
            out,
        );
        let lowest = amounts(ui, &s.currencies, &s.lowest_now);
        line(&mut y, &format!("  lowest {lowest}"), TAN, out);
        if !s.median_now.is_empty() && s.median_now != s.lowest_now {
            let median = amounts(ui, &s.currencies, &s.median_now);
            line(&mut y, &format!("  median {median}"), TAN, out);
        }
    }
    if h.listed_now > priced {
        line(
            &mut y,
            &format!("{} take offers only", h.listed_now - priced),
            TAN,
            out,
        );
    }
    if h.listed_now == 0 {
        line(&mut y, "Nothing listed", GREY, out);
    }
    y += 4;
    line(
        &mut y,
        &format!("SOLD, LAST {} DAYS ({})", h.days, h.sold),
        GREY,
        out,
    );
    if pl.with_items > 0 {
        let s = format!("{} of them with items (not drawn)", pl.with_items);
        line(&mut y, &s, TAN, out);
    }
    y += 4;
    if h.recent.is_empty() {
        line(&mut y, "No sales", GREY, out);
        return;
    }
    line(&mut y, &format!("LAST {} SOLD", h.recent.len()), GREY, out);
    for sale in &h.recent {
        let when = match age(h.now_ms, sale.at_ms).as_str() {
            "now" => "just now".to_owned(),
            a => format!("{a} ago"),
        };
        let text = format!("{} · {when}", sale_text(ui, &sale.price));
        line(&mut y, &text, GOLD, out);
    }
}

#[cfg(test)]
#[path = "price_graph_sell_tests.rs"]
mod sell_tests;
#[cfg(test)]
#[path = "price_graph_tests.rs"]
mod tests;
