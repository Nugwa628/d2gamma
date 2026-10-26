// D2RUST's own: the market window's Deliveries tab.
//! Deliveries (mock 3): everything the market gave the player, waiting to
//! be claimed into the shared stash: what was bought, a sale's proceeds,
//! listings back from cancel or expiry, offers' goods back and the change
//! a counter left. The market sends one row per thing; the tab shows the
//! rows of one sale or one offer as one line ("Sold: Grim Crown → 1 Lem +
//! 2 Ko"), each with its own Claim, and "Claim all into the stash".
//!
//! Claiming is a client rule (contract, "Claiming"): only at the town
//! stash (its panel open); elsewhere the buttons say "Claim at your
//! stash" and send nothing. A claim moves row by row: what does not fit
//! stays listed and the status line says why.

use crate::market::source::{Goods, MarketAsk, MarketCode, MarketDone, Req};
use crate::market::trade::{
    DeliveriesPage, DeliveryId, DeliveryReason, DeliveryRow, DeliverySource, CLAIM_MAX, PAGE_ROWS,
};
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::{Point, Rect};
use crate::ui::item_tip::TipLine;

use super::details::{age, coin_name, thousands};
use super::window::{DrawIn, Places, GOLD, GREY, SMALL, TAN, WHITE};
use super::MarketUi;

/// Lines shown at once.
pub const LINES: usize = 22;

/// A line's height.
const LINE_H: i32 = 14;

/// What the claim buttons say away from the stash.
pub const AT_STASH: &str = "Claim at your stash";

/// The Deliveries tab's state.
#[derive(Clone, Debug, Default)]
pub struct DeliveriesTab {
    /// The last page the market sent.
    pub page: Option<DeliveriesPage>,
    /// The page shown (from 0).
    pub page_no: u16,
    pub req: Option<Req>,
    /// The claim in flight and the deliveries it named (empty: all).
    pub claim_req: Option<(Req, Vec<DeliveryId>)>,
    /// A claim queued, not yet numbered.
    pub claim_queued: bool,
    /// The first shown line.
    pub top: usize,
}

/// One shown line: the rows of one sale, one buy or one offer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub source: DeliverySource,
    /// Why its goods came back (the first row's).
    pub reason: DeliveryReason,
    pub about: String,
    /// Indexes of its rows on the page.
    pub rows: Vec<usize>,
    pub at_ms: u64,
}

/// The page's rows as lines: rows of the same source, listing and offer
/// together, in the page's order (newest first).
pub fn lines(page: &DeliveriesPage) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let mut keys: Vec<(DeliverySource, Option<u64>, Option<u64>)> = Vec::new();
    for (i, d) in page.rows.iter().enumerate() {
        // A buy or a listing's own goods coming back stands alone without
        // a listing to group by.
        let key = (d.source, d.listing, d.offer);
        let alone = d.listing.is_none() && d.offer.is_none();
        match keys.iter().position(|k| *k == key).filter(|_| !alone) {
            Some(n) => out[n].rows.push(i),
            None => {
                keys.push(key);
                out.push(Line {
                    source: d.source,
                    reason: d.reason,
                    about: d.about.clone(),
                    rows: vec![i],
                    at_ms: d.at_ms,
                });
            }
        }
    }
    // A line's goods in the order the market delivered them.
    for l in &mut out {
        l.rows.sort_by_key(|i| page.rows[*i].delivery);
    }
    out
}

/// One delivery's goods: "2 Rune 26", "50,000 gold", an item's name.
pub fn goods_text(ui: &MarketUi, d: &DeliveryRow) -> (String, u16) {
    match &d.goods {
        Goods::Gold(g) => (format!("{} gold", thousands(*g)), GOLD),
        Goods::Stack { code, count } => (format!("{count} {}", coin_name(ui, code)), 8),
        Goods::Item { .. } => match &d.item {
            Some((g, v)) => (v.name.clone(), g.color()),
            None => (d.about.clone(), WHITE),
        },
    }
}

/// A line's parts: the source's word, the goods' name, what came.
pub fn line_parts(ui: &MarketUi, page: &DeliveriesPage, l: &Line) -> Vec<(String, u16)> {
    let goods: Vec<(String, u16)> = l
        .rows
        .iter()
        .map(|i| goods_text(ui, &page.rows[*i]))
        .collect();
    let joined = |v: &[(String, u16)]| -> Vec<(String, u16)> {
        let n = v.len();
        v.iter()
            .enumerate()
            .map(|(k, (s, c))| {
                if k + 1 < n {
                    (format!("{s} +"), *c)
                } else {
                    (s.clone(), *c)
                }
            })
            .collect()
    };
    // An item line is named in its own colour.
    let named = || -> (String, u16) {
        goods
            .iter()
            .find(|_| {
                l.rows
                    .iter()
                    .any(|i| matches!(page.rows[*i].goods, Goods::Item { .. }))
            })
            .cloned()
            .unwrap_or((l.about.clone(), WHITE))
    };
    let mut out = Vec::new();
    match l.source {
        DeliverySource::Sold => {
            out.push(("Sold:".into(), TAN));
            out.push((format!("{} ->", l.about), WHITE));
            out.extend(joined(&goods));
        }
        DeliverySource::Bought => {
            out.push(("Bought:".into(), TAN));
            out.extend(joined(&goods));
        }
        DeliverySource::Cancelled | DeliverySource::Expired => {
            out.push(("Returned:".into(), TAN));
            out.push(named());
            let why = if l.source == DeliverySource::Expired {
                "(expired)"
            } else {
                "(cancelled)"
            };
            out.push((why.into(), GREY));
        }
        DeliverySource::Returned => {
            out.push(("Refund:".into(), TAN));
            out.push((format!("offer on {} ·", l.about), GREY));
            out.extend(joined(&goods));
            if let Some(why) = l.reason.label() {
                out.push((format!("({why})"), GREY));
            }
        }
        DeliverySource::Change => {
            out.push(("Change:".into(), TAN));
            out.push((format!("offer on {} ·", l.about), GREY));
            out.extend(joined(&goods));
        }
        DeliverySource::Other(_) => {
            out.push((format!("{} ·", l.about), WHITE));
            out.extend(joined(&goods));
        }
    }
    out
}

impl MarketUi {
    /// Asks for the shown page of Deliveries.
    pub fn open_deliveries(&mut self) {
        self.asks
            .retain(|a| !matches!(a, MarketAsk::DeliveriesOpen { .. }));
        self.asks.push(MarketAsk::DeliveriesOpen {
            page: self.deliveries.page_no,
        });
        self.deliveries.req = None;
    }

    /// The number of pages of Deliveries.
    pub fn delivery_pages(&self) -> u16 {
        let total = self.deliveries.page.as_ref().map_or(0, |p| p.total);
        total
            .div_ceil(PAGE_ROWS as u32)
            .clamp(1, u32::from(u16::MAX)) as u16
    }

    pub fn delivery_page(&mut self, page: u16) {
        let page = page.min(self.delivery_pages() - 1);
        if page != self.deliveries.page_no {
            self.deliveries.page_no = page;
            self.deliveries.top = 0;
            self.open_deliveries();
        }
    }

    /// Claims are taken only at the stash; elsewhere the reason.
    pub fn claim_blocked(&self) -> Option<&'static str> {
        (!self.at_stash).then_some(AT_STASH)
    }

    /// Claim all into the stash (at the stash only).
    pub fn claim_all(&mut self) {
        self.claim(Vec::new());
    }

    /// Claims line `n`'s deliveries (at the stash only).
    pub fn claim_line(&mut self, n: usize) {
        let Some(page) = &self.deliveries.page else {
            return;
        };
        let Some(l) = lines(page).into_iter().nth(n) else {
            return;
        };
        let ids = l.rows.iter().map(|i| page.rows[*i].delivery).collect();
        self.claim(ids);
    }

    /// `ids` (none: all) into the stash.
    fn claim(&mut self, ids: Vec<DeliveryId>) {
        if let Some(why) = self.claim_blocked() {
            self.note = Some(format!(
                "{why}: your Deliveries go into the shared stash there."
            ));
            return;
        }
        if self.deliveries.claim_req.is_some() || self.deliveries.claim_queued {
            return;
        }
        let all = ids.is_empty();
        self.note = Some("Claiming into your stash...".into());
        self.asks.push(MarketAsk::Claim {
            all,
            deliveries: ids.into_iter().take(CLAIM_MAX).collect(),
        });
        self.deliveries.claim_queued = true;
    }

    /// A Deliveries ask went out as `req`.
    pub(super) fn sent_delivery(&mut self, ask: &MarketAsk, req: Req) {
        match ask {
            MarketAsk::DeliveriesOpen { .. } => self.deliveries.req = Some(req),
            MarketAsk::Claim { deliveries, .. } => {
                self.deliveries.claim_queued = false;
                self.deliveries.claim_req = Some((req, deliveries.clone()));
            }
            _ => {}
        }
    }

    /// A page of Deliveries arrived; its total is the count waiting.
    pub(super) fn apply_deliveries(&mut self, req: Req, page: DeliveriesPage) {
        if self.deliveries.req != Some(req) {
            return;
        }
        self.deliveries.req = None;
        if let Some(s) = self.state.as_mut() {
            s.deliveries_waiting = page.total.min(u32::from(u16::MAX)) as u16;
        }
        self.deliveries.page_no = page.page;
        let n = lines(&page).len();
        self.deliveries.top = self.deliveries.top.min(n.saturating_sub(LINES));
        self.deliveries.page = Some(page);
    }

    /// The claim's answer: what went in, what did not and why. True when
    /// `d` answered it.
    pub(super) fn done_claim(&mut self, d: &MarketDone) -> bool {
        if self.deliveries.claim_req.as_ref().map(|c| c.0) != Some(d.req) {
            return false;
        }
        self.deliveries.claim_req = None;
        self.note = Some(claim_line(d));
        self.open_deliveries();
        true
    }
}

/// The status line of a claim's answer.
pub fn claim_line(d: &MarketDone) -> String {
    if d.code != MarketCode::Done {
        return d.line();
    }
    let ok = d
        .rows
        .iter()
        .filter(|(_, c)| *c == MarketCode::Done)
        .count();
    let failed: Vec<&MarketCode> = d
        .rows
        .iter()
        .filter(|(_, c)| *c != MarketCode::Done)
        .map(|(_, c)| c)
        .collect();
    let mut s = match ok {
        0 => "Nothing was claimed.".to_owned(),
        1 => "Claimed 1 delivery into your stash.".to_owned(),
        n => format!("Claimed {n} deliveries into your stash."),
    };
    if let Some(first) = failed.first() {
        let why = first.text(d.wait_s).unwrap_or_default();
        s.push_str(&format!(" {} stayed: {why}", failed.len()));
    }
    s
}

/// The tab's clickable parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeliverySpot {
    /// Line `n`'s Claim.
    Claim(usize),
    ClaimAll,
    Prev,
    Next,
}

impl DeliverySpot {
    pub fn id(self) -> String {
        match self {
            DeliverySpot::Claim(n) => format!("market.deliveries.claim({n})"),
            DeliverySpot::ClaimAll => "market.deliveries.claim_all".into(),
            DeliverySpot::Prev => "market.deliveries.prev".into(),
            DeliverySpot::Next => "market.deliveries.next".into(),
        }
    }
}

/// The tab's places.
pub struct DeliveryPlaces<'a> {
    pub p: &'a Places,
}

impl DeliveryPlaces<'_> {
    fn w(&self) -> i32 {
        self.p.w - 12
    }

    pub fn list(&self) -> Rect {
        self.p.r(6, 44, self.w(), LINES as i32 * LINE_H + 4)
    }

    pub fn line(&self, k: usize) -> Rect {
        let l = self.list();
        Rect::new(l.x, l.y + 2 + LINE_H * k as i32, l.w, LINE_H as u16)
    }

    pub fn claim(&self, k: usize) -> Rect {
        let r = self.line(k);
        Rect::new(
            r.x + i32::from(r.w) - 112,
            r.y + 1,
            108,
            (LINE_H - 2) as u16,
        )
    }

    pub fn claim_all(&self) -> Rect {
        self.p.r(self.w() - 214, self.p.h - 30, 220, 20)
    }

    pub fn prev(&self) -> Rect {
        self.p.r(self.w() - 70, 26, 18, 14)
    }

    pub fn next(&self) -> Rect {
        self.p.r(self.w() - 12, 26, 18, 14)
    }

    pub fn note(&self) -> Rect {
        self.p.r(6, self.p.h - 18, self.w() - 230, 14)
    }
}

/// The tab's parts shown now. Away from the stash the Claim buttons are
/// still there (they say why nothing moves).
pub fn spots(ui: &MarketUi, p: &Places) -> Vec<(DeliverySpot, Rect)> {
    let dp = DeliveryPlaces { p };
    let mut v = Vec::new();
    let Some(page) = &ui.deliveries.page else {
        return v;
    };
    let n = lines(page).len();
    for (k, i) in (ui.deliveries.top..n).take(LINES).enumerate() {
        v.push((DeliverySpot::Claim(i), dp.claim(k)));
    }
    if n > 0 {
        v.push((DeliverySpot::ClaimAll, dp.claim_all()));
    }
    if ui.delivery_pages() > 1 {
        v.push((DeliverySpot::Prev, dp.prev()));
        v.push((DeliverySpot::Next, dp.next()));
    }
    v
}

pub fn click(ui: &mut MarketUi, s: DeliverySpot) {
    match s {
        DeliverySpot::Claim(n) => ui.claim_line(n),
        DeliverySpot::ClaimAll => ui.claim_all(),
        DeliverySpot::Prev => ui.delivery_page(ui.deliveries.page_no.saturating_sub(1)),
        DeliverySpot::Next => ui.delivery_page(ui.deliveries.page_no.saturating_add(1)),
    }
}

pub fn wheel(ui: &mut MarketUi, p: &Places, at: Point, steps: i32) {
    let dp = DeliveryPlaces { p };
    if !dp.list().contains(at) {
        return;
    }
    let n = ui.deliveries.page.as_ref().map_or(0, |pg| lines(pg).len());
    let t = ui.deliveries.top as i64 - i64::from(steps);
    ui.deliveries.top = t.clamp(0, n.saturating_sub(LINES) as i64) as usize;
}

/// Draws the tab.
pub fn draw(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen, k) = (i.ui, &i.places, &i.pen, i.pen.kit);
    let dp = DeliveryPlaces { p };
    let total = ui.deliveries.page.as_ref().map_or_else(
        || {
            ui.state
                .as_ref()
                .map_or(0, |s| u32::from(s.deliveries_waiting))
        },
        |pg| pg.total,
    );
    pen.text(
        p.x0 + 8,
        p.y0 + 30,
        &format!("DELIVERIES ({total})"),
        SMALL,
        GREY,
        out,
    );
    let hint =
        "Everything the market gives you waits here until you claim it into your shared stash.";
    pen.text(
        p.x0 + 110,
        p.y0 + 30,
        &pen.fit(SMALL, hint, p.w - 200),
        SMALL,
        GREY,
        out,
    );
    if ui.delivery_pages() > 1 {
        pen.button(dp.prev(), "<", false, out);
        let s = format!("{}/{}", ui.deliveries.page_no + 1, ui.delivery_pages());
        pen.label(
            Rect::new(dp.prev().x + 18, dp.prev().y, 40, 14),
            &s,
            SMALL,
            TAN,
            out,
        );
        pen.button(dp.next(), ">", false, out);
    }
    let list = dp.list();
    pen.fill(list, k.field, out);
    pen.frame(list, k.edge, out);
    let blocked = ui.claim_blocked();
    match &ui.deliveries.page {
        None => pen.text(list.x + 6, list.y + 6, "Asking...", SMALL, GREY, out),
        Some(page) if page.rows.is_empty() => {
            pen.text(
                list.x + 6,
                list.y + 6,
                "Nothing waits for you.",
                SMALL,
                GREY,
                out,
            );
        }
        Some(page) => {
            let all = lines(page);
            for (n, l) in all.iter().enumerate().skip(ui.deliveries.top).take(LINES) {
                let slot = n - ui.deliveries.top;
                let r = dp.line(slot);
                if r.contains(pen.mouse) {
                    pen.fill(r, k.hit, out);
                }
                let claim = dp.claim(slot);
                let when = age(page.now_ms, l.at_ms);
                pen.text_right(claim.x - 6, r.y + 3, &when, SMALL, GREY, out);
                let right = claim.x - 12 - pen.width(SMALL, &when);
                let mut x = r.x + 6;
                for (s, c) in line_parts(ui, page, l) {
                    if right - x < 16 {
                        break;
                    }
                    let s = pen.fit(SMALL, &s, right - x);
                    pen.text(x, r.y + 3, &s, SMALL, c, out);
                    x += pen.width(SMALL, &s) + 5;
                }
                pen.button(
                    claim,
                    if blocked.is_some() {
                        "At your stash"
                    } else {
                        "Claim"
                    },
                    false,
                    out,
                );
            }
        }
    }
    let n = ui.deliveries.page.as_ref().map_or(0, |pg| pg.rows.len());
    if n > 0 {
        pen.button(
            dp.claim_all(),
            blocked.unwrap_or("Claim all into the stash"),
            blocked.is_none(),
            out,
        );
    }
    if let Some(s) = &ui.note {
        pen.line(dp.note(), s, SMALL, GOLD, out);
    }
}

/// The tip of the item line under the mouse.
pub fn hover_tip(i: &DrawIn) -> Option<(Vec<TipLine>, Rect)> {
    let ui = i.ui;
    let page = ui.deliveries.page.as_ref()?;
    let dp = DeliveryPlaces { p: &i.places };
    let all = lines(page);
    for (slot, n) in (ui.deliveries.top..all.len()).take(LINES).enumerate() {
        let r = dp.line(slot);
        if !r.contains(i.pen.mouse) || dp.claim(slot).contains(i.pen.mouse) {
            continue;
        }
        let d = all[n]
            .rows
            .iter()
            .map(|k| &page.rows[*k])
            .find(|d| matches!(d.goods, Goods::Item { .. }))?;
        let (g, v) = d.item.as_ref()?;
        let record = match &d.goods {
            Goods::Item { record, .. } => record.as_slice(),
            _ => &[],
        };
        let game = match i.tips {
            Some(t) => super::tip::record_lines(t, record),
            None => Vec::new(),
        };
        let lines = if game.is_empty() {
            super::tip::view_lines(*g, v, 1)
        } else {
            game
        };
        return Some((lines, r));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::tests::{click, ids, open};
    use super::super::window::Spot;
    use super::super::MarketTab;
    use super::*;
    use crate::market::fake::FakeSource;

    /// The window open on the Deliveries tab with its page shown.
    fn deliveries_tab() -> (MarketUi, FakeSource) {
        let (mut ui, mut src) = open();
        click(&mut ui, &mut src, Spot::Tab(MarketTab::Deliveries));
        assert!(ui.deliveries.page.is_some());
        (ui, src)
    }

    /// The shown lines as text.
    fn texts(ui: &MarketUi) -> Vec<String> {
        let page = ui.deliveries.page.as_ref().unwrap();
        lines(page)
            .iter()
            .map(|l| {
                line_parts(ui, page, l)
                    .into_iter()
                    .map(|(s, _)| s)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect()
    }

    fn waiting(ui: &MarketUi) -> u16 {
        ui.state.as_ref().unwrap().deliveries_waiting
    }

    #[test]
    fn deliveries_grouped_by_sale() {
        let (mut ui, mut src) = deliveries_tab();
        // Five rows: a sale's two currencies make one line.
        assert_eq!(ui.deliveries.page.as_ref().unwrap().rows.len(), 5);
        assert_eq!(
            texts(&ui),
            [
                "Sold: Duskweave Mantle -> 1 r29 + 2 r26",
                "Bought: Ashen Crown",
                "Returned: Vessa's Vigil (expired)",
                "Refund: offer on Stormcall · 1 r24",
            ]
        );
        assert_eq!(waiting(&ui), 5);
        // A new sale's proceeds come as their own line, at the top.
        src.sell_mine(9002, "Brakka");
        ui.pump(&mut src);
        ui.pump(&mut src);
        let t = texts(&ui);
        assert_eq!(t.len(), 5);
        assert_eq!(t[0], "Sold: Grimward Visage -> 2 r26 + 1 r29");
        assert_eq!(waiting(&ui), 7);
        // Each line has its Claim; the tab has Claim all.
        let shown = ids(&ui);
        for n in 0..5 {
            assert!(shown.contains(&format!("market.deliveries.claim({n})")));
        }
        assert!(shown.contains(&"market.deliveries.claim_all".to_owned()));
    }

    #[test]
    fn claim_only_at_stash() {
        let (mut ui, mut src) = deliveries_tab();
        let before = src.trade.stash.clone();
        // Away from the stash nothing is asked; the line says where.
        assert!(!ui.at_stash);
        assert_eq!(ui.claim_blocked(), Some(AT_STASH));
        click(&mut ui, &mut src, Spot::Deliveries(DeliverySpot::ClaimAll));
        click(&mut ui, &mut src, Spot::Deliveries(DeliverySpot::Claim(0)));
        assert!(ui.deliveries.claim_req.is_none() && !ui.deliveries.claim_queued);
        assert_eq!(src.trade.deliveries.len(), 5);
        assert!(ui.note.clone().unwrap().starts_with(AT_STASH));
        // At the stash one line goes in: both of the sale's runes.
        ui.at_stash = true;
        click(&mut ui, &mut src, Spot::Deliveries(DeliverySpot::Claim(0)));
        assert_eq!(src.trade.deliveries.len(), 3);
        assert_eq!(src.trade.stash.stacks["r29"], before.stacks["r29"] + 1);
        assert_eq!(src.trade.stash.stacks["r26"], before.stacks["r26"] + 2);
        assert_eq!(
            ui.note.clone().unwrap(),
            "Claimed 2 deliveries into your stash."
        );
        // The list is asked for again and shrinks.
        ui.pump(&mut src);
        assert_eq!(texts(&ui).len(), 3);
        assert_eq!(waiting(&ui), 3);
        // Claim all takes the rest.
        click(&mut ui, &mut src, Spot::Deliveries(DeliverySpot::ClaimAll));
        ui.pump(&mut src);
        assert!(src.trade.deliveries.is_empty());
        assert!(ui.deliveries.page.as_ref().unwrap().rows.is_empty());
        assert_eq!(waiting(&ui), 0);
    }

    #[test]
    fn claim_partial_full_stash() {
        let (mut ui, mut src) = deliveries_tab();
        // No room for an item: the runes go in, the two items stay.
        src.trade.room = 0;
        ui.at_stash = true;
        click(&mut ui, &mut src, Spot::Deliveries(DeliverySpot::ClaimAll));
        assert_eq!(
            ui.note.clone().unwrap(),
            "Claimed 3 deliveries into your stash. 2 stayed: Your shared stash is full."
        );
        ui.pump(&mut src);
        assert_eq!(
            texts(&ui),
            ["Bought: Ashen Crown", "Returned: Vessa's Vigil (expired)"]
        );
        assert_eq!(waiting(&ui), 2);
        // Room again: the next claim takes them.
        src.trade.room = 5;
        click(&mut ui, &mut src, Spot::Deliveries(DeliverySpot::Claim(1)));
        assert_eq!(
            ui.note.clone().unwrap(),
            "Claimed 1 delivery into your stash."
        );
        assert!(src
            .trade
            .stash
            .items
            .iter()
            .any(|i| i.item.name == "Vessa's Vigil"));
    }

    #[test]
    fn refunds_say_why() {
        // The realm's reason (contract v2) on an offer's goods back.
        let row = |delivery, offer, reason| DeliveryRow {
            delivery,
            source: DeliverySource::Returned,
            reason,
            listing: Some(7),
            offer: Some(offer),
            about: "Stormcall".into(),
            goods: Goods::Gold(500),
            item: None,
            at_ms: 0,
        };
        let page = DeliveriesPage {
            rows: vec![
                row(1, 70, DeliveryReason::Declined),
                row(2, 71, DeliveryReason::Outbid),
                row(3, 72, DeliveryReason::None),
            ],
            ..DeliveriesPage::default()
        };
        let ui = MarketUi::new();
        let texts: Vec<String> = lines(&page)
            .iter()
            .map(|l| {
                line_parts(&ui, &page, l)
                    .into_iter()
                    .map(|(s, _)| s)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        assert_eq!(
            texts,
            [
                "Refund: offer on Stormcall · 500 gold (declined)",
                "Refund: offer on Stormcall · 500 gold (sold to another)",
                "Refund: offer on Stormcall · 500 gold",
            ]
        );
    }

    #[test]
    fn claim_line_reads_the_rows() {
        let d = |rows: Vec<(u64, MarketCode)>| MarketDone {
            req: 1,
            code: MarketCode::Done,
            wait_s: 0,
            repeated: false,
            id: None,
            rows,
            text: String::new(),
        };
        assert_eq!(claim_line(&d(Vec::new())), "Nothing was claimed.");
        let full = d(vec![(1, MarketCode::StashFull)]);
        assert_eq!(
            claim_line(&full),
            "Nothing was claimed. 1 stayed: Your shared stash is full."
        );
        let mut refused = d(Vec::new());
        refused.code = MarketCode::MarketOff;
        assert_eq!(claim_line(&refused), "The market is closed on this realm.");
    }
}
