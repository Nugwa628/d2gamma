// D2RUST's own: the market window's Offers tab.
//! Offers (mock 3): the offers others made on the player's listings, with
//! Accept, Counter... and Decline, and the offers the player made, with
//! their state, Accept (a counter) and Withdraw. A row shows the listing,
//! who offered and the offer exactly (gold, currencies and items by name
//! in their colour); the picked offer fills the right column with the
//! listing's tip, the offer and the counter, and the answer buttons.
//!
//! Every answer is confirmed with what moves exactly (and the fee line
//! when the realm's fees are not 0) and carries the fee revision the
//! player saw; a counter is built in the bundle form
//! ([`super::offer_form`]). Answers and refusals show on the status line;
//! the lists are asked again after every answer.

use crate::market::source::{MarketAsk, MarketCode, MarketDone, Price, Req};
use crate::market::trade::{
    AnswerHow, NamedItem, OfferId, OfferRow, OfferState, OffersPage, OffersWhich,
};
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::{Point, Rect};
use crate::ui::item_tip::TipLine;

use super::details::{age, listing_price_of, thousands};
use super::offer_form::{self, price_parts, BundleForm};
use super::window::{DrawIn, Pen, Places, GOLD, GREY, LABEL, SMALL, TAN, WHITE};
use super::MarketUi;

/// Rows of each list shown at once.
pub const LIST_ROWS: usize = 7;

/// A row's height (two lines).
const ROW_H: i32 = 24;

/// The Offers tab's state.
#[derive(Clone, Debug)]
pub struct OffersTab {
    /// Offers on the player's listings, and offers the player made: the
    /// last pages the market sent.
    pub pages: [Option<OffersPage>; 2],
    /// The pages asked for, by [`OffersWhich::code`].
    pub reqs: [Option<Req>; 2],
    /// Only offers still waiting on an answer (open or countered).
    pub live_only: bool,
    /// The first shown row of each list.
    pub top: [usize; 2],
    /// The picked offer.
    pub picked: Option<(OffersWhich, OfferId)>,
    /// The answer confirm.
    pub deal: Option<Deal>,
    /// The answer and the new offer in flight: `(req, offer or listing,
    /// what, the goods' name)`.
    pub answer_req: Option<(Req, OfferId, AnswerHow, String)>,
    pub offer_req: Option<(Req, u64, String)>,
}

impl Default for OffersTab {
    fn default() -> Self {
        OffersTab {
            pages: [None, None],
            reqs: [None, None],
            live_only: true,
            top: [0, 0],
            picked: None,
            deal: None,
            answer_req: None,
            offer_req: None,
        }
    }
}

/// An answer waiting for the player's yes: what moves, exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deal {
    pub offer: OfferId,
    pub how: AnswerHow,
    /// The goods' name.
    pub name: String,
    /// What the player gets or pays, part by part, with each part's colour.
    pub gets: Vec<(String, u16)>,
    pub pays: Vec<(String, u16)>,
    /// The fee taken (0 by default; the line is hidden then).
    pub fee: u64,
    pub fee_rev: u32,
}

impl OffersTab {
    pub fn page(&self, which: OffersWhich) -> Option<&OffersPage> {
        self.pages[usize::from(which.code())].as_ref()
    }

    /// Row `i` of list `which`.
    pub fn row(&self, which: OffersWhich, i: usize) -> Option<&OfferRow> {
        self.page(which)?.rows.get(i)
    }

    /// The picked offer, while a list holds it.
    pub fn picked_row(&self) -> Option<(OffersWhich, &OfferRow)> {
        let (w, id) = self.picked?;
        let r = self.page(w)?.rows.iter().find(|o| o.offer == id)?;
        Some((w, r))
    }

    /// Any row of either list with offer `id`.
    fn find(&self, id: OfferId) -> Option<&OfferRow> {
        self.pages
            .iter()
            .flatten()
            .flat_map(|p| p.rows.iter())
            .find(|o| o.offer == id)
    }
}

/// The answers the player may give to an offer of list `which`: the
/// seller an open offer; the buyer a counter (accept or withdraw) or an
/// open offer (withdraw).
pub fn answers(which: OffersWhich, o: &OfferRow) -> Vec<AnswerHow> {
    match (which, o.state) {
        (OffersWhich::OnMyListings, OfferState::Open) => {
            vec![AnswerHow::Accept, AnswerHow::Counter, AnswerHow::Decline]
        }
        (OffersWhich::MadeByMe, OfferState::Countered) => {
            vec![AnswerHow::Accept, AnswerHow::Withdraw]
        }
        (OffersWhich::MadeByMe, OfferState::Open) => vec![AnswerHow::Withdraw],
        _ => Vec::new(),
    }
}

/// The offered items as the row named them.
fn named(o: &OfferRow) -> Vec<&NamedItem> {
    o.items.iter().collect()
}

impl MarketUi {
    /// Asks for both offer lists (the tab opened, or something changed).
    pub fn open_offers(&mut self) {
        self.asks
            .retain(|a| !matches!(a, MarketAsk::OffersOpen { .. }));
        for which in OffersWhich::ALL {
            self.asks.push(MarketAsk::OffersOpen {
                which,
                live_only: self.offers.live_only,
                page: 0,
            });
            self.offers.reqs[usize::from(which.code())] = None;
        }
    }

    /// Live offers only, or every offer of the last 30 days.
    pub fn toggle_live_only(&mut self) {
        self.offers.live_only = !self.offers.live_only;
        self.offers.top = [0, 0];
        self.open_offers();
    }

    /// Picks row `i` of list `which`.
    pub fn pick_offer(&mut self, which: OffersWhich, i: usize) {
        if let Some(o) = self.offers.row(which, i) {
            self.offers.picked = Some((which, o.offer));
        }
    }

    /// An answer button on the picked offer: Counter... opens the form,
    /// the others the confirm with what moves.
    pub fn answer_offer(&mut self, how: AnswerHow) {
        let Some((which, o)) = self.offers.picked_row() else {
            return;
        };
        if !answers(which, o).contains(&how) {
            return;
        }
        let o = o.clone();
        if how == AnswerHow::Counter {
            self.form = Some(BundleForm::counter(self, &o));
            return;
        }
        let fees = self.fees();
        let items = named(&o);
        let offered = price_parts(self, &o.offered, &items);
        let goods = vec![(o.item.name.clone(), o.group.color())];
        let (gets, pays, fee) = match (which, how) {
            (OffersWhich::OnMyListings, AnswerHow::Accept) => {
                let fee = o.offered.gold * u64::from(fees.sale_fee_pct) / 100;
                (offered, goods, fee)
            }
            (OffersWhich::MadeByMe, AnswerHow::Accept) => {
                let counter = o.counter.clone().unwrap_or_default();
                (goods, price_parts(self, &counter, &items), fees.buyer_fee)
            }
            (OffersWhich::MadeByMe, AnswerHow::Withdraw) => (offered, Vec::new(), 0),
            _ => (Vec::new(), Vec::new(), 0),
        };
        self.offers.deal = Some(Deal {
            offer: o.offer,
            how,
            name: o.item.name.clone(),
            gets,
            pays,
            fee,
            fee_rev: fees.fee_rev,
        });
    }

    /// The confirm's yes: the answer goes to the market.
    pub fn deal_yes(&mut self) {
        let Some(d) = self.offers.deal.take() else {
            return;
        };
        self.note = Some(format!("{} the offer on {}...", doing(d.how), d.name));
        self.asks.push(MarketAsk::Answer {
            offer: d.offer,
            how: d.how,
            counter: None,
            fee_rev: d.fee_rev,
        });
    }

    /// An offer ask went out as `req`.
    pub(super) fn sent_offer(&mut self, ask: &MarketAsk, req: Req) {
        match ask {
            MarketAsk::OffersOpen { which, .. } => {
                self.offers.reqs[usize::from(which.code())] = Some(req);
            }
            MarketAsk::Answer { offer, how, .. } => {
                let name = self
                    .offers
                    .find(*offer)
                    .map_or_else(|| "the item".to_owned(), |o| o.item.name.clone());
                self.offers.answer_req = Some((req, *offer, *how, name));
            }
            MarketAsk::Offer { listing, .. } => {
                let name = self
                    .selected
                    .as_ref()
                    .filter(|l| l.listing == *listing)
                    .map_or_else(|| "the item".to_owned(), |l| l.item.name.clone());
                self.offers.offer_req = Some((req, *listing, name));
            }
            _ => {}
        }
    }

    /// A page of offers arrived.
    pub(super) fn apply_offers(&mut self, req: Req, page: OffersPage) {
        let k = usize::from(page.which.code());
        if self.offers.reqs[k] != Some(req) {
            return;
        }
        self.offers.reqs[k] = None;
        let rows = page.rows.len();
        self.offers.top[k] = self.offers.top[k].min(rows.saturating_sub(LIST_ROWS));
        self.offers.pages[k] = Some(page);
        // The picked offer stays picked while a list holds it.
        if self.offers.picked.is_some() && self.offers.picked_row().is_none() {
            let gone = self
                .offers
                .picked
                .is_some_and(|(w, _)| usize::from(w.code()) == k);
            if gone {
                self.offers.picked = None;
            }
        }
    }

    /// A market answer to an offer or an answer: true when it was one.
    pub(super) fn done_offer(&mut self, d: &MarketDone) -> bool {
        if let Some((req, _, how, name)) = self.offers.answer_req.clone() {
            if req == d.req {
                self.offers.answer_req = None;
                self.note = Some(if d.code == MarketCode::Done {
                    answered(how, &name)
                } else {
                    d.line()
                });
                if d.code == MarketCode::FeeChanged {
                    self.asks.push(MarketAsk::Hello);
                }
                self.open_offers();
                if self.deliveries.page.is_some() {
                    self.open_deliveries();
                }
                return true;
            }
        }
        if let Some((req, listing, name)) = self.offers.offer_req.clone() {
            if req == d.req {
                self.offers.offer_req = None;
                self.note = Some(if d.code == MarketCode::Done {
                    format!("Offer made on {name}. It waits in escrow for the seller.")
                } else {
                    d.line()
                });
                if d.code == MarketCode::FeeChanged {
                    self.asks.push(MarketAsk::Hello);
                }
                // The listing's own offer and count, as they are now.
                if self.selected.as_ref().is_some_and(|l| l.listing == listing) {
                    self.asks.push(MarketAsk::ListingGet(listing));
                }
                if self.offers.pages.iter().any(Option::is_some) {
                    self.open_offers();
                }
                return true;
            }
        }
        false
    }
}

/// "Accepting", "Declining" ...
fn doing(how: AnswerHow) -> &'static str {
    match how {
        AnswerHow::Accept => "Accepting",
        AnswerHow::Decline => "Declining",
        AnswerHow::Counter => "Countering",
        AnswerHow::Withdraw => "Withdrawing",
    }
}

/// The status line after an answer went through.
fn answered(how: AnswerHow, name: &str) -> String {
    match how {
        AnswerHow::Accept => format!("Accepted: {name}. What you get waits in Deliveries."),
        AnswerHow::Decline => format!("Declined the offer on {name}."),
        AnswerHow::Counter => format!("Countered the offer on {name}. The buyer answers next."),
        AnswerHow::Withdraw => {
            format!("Withdrew your offer on {name}. Your goods wait in Deliveries.")
        }
    }
}

/// The word of a button's UI id and its label.
pub fn answer_key(how: AnswerHow) -> &'static str {
    match how {
        AnswerHow::Accept => "accept",
        AnswerHow::Decline => "decline",
        AnswerHow::Counter => "counter",
        AnswerHow::Withdraw => "withdraw",
    }
}

fn answer_label(which: OffersWhich, how: AnswerHow) -> &'static str {
    match (which, how) {
        (OffersWhich::MadeByMe, AnswerHow::Accept) => "Accept counter",
        (_, AnswerHow::Accept) => "Accept",
        (_, AnswerHow::Decline) => "Decline",
        (_, AnswerHow::Counter) => "Counter...",
        (_, AnswerHow::Withdraw) => "Withdraw",
    }
}

/// The tab's clickable parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferSpot {
    /// Row `i` of a list.
    Row(OffersWhich, usize),
    LiveOnly,
    Answer(AnswerHow),
    /// The confirm's yes and no.
    Yes,
    No,
}

impl OfferSpot {
    pub fn id(self) -> String {
        match self {
            OfferSpot::Row(w, i) => format!("market.offers.{}({i})", w.key()),
            OfferSpot::LiveOnly => "market.offers.live".into(),
            OfferSpot::Answer(h) => format!("market.offer.{}", answer_key(h)),
            OfferSpot::Yes => "market.deal.yes".into(),
            OfferSpot::No => "market.deal.no".into(),
        }
    }
}

/// The tab's places.
pub struct OfferPlaces<'a> {
    pub p: &'a Places,
}

impl OfferPlaces<'_> {
    /// The list column's width.
    fn lw(&self) -> i32 {
        self.p.details().x - self.p.x0 - 12
    }

    /// List `which`: its header line's top.
    fn head_y(&self, which: OffersWhich) -> i32 {
        match which {
            OffersWhich::OnMyListings => 28,
            OffersWhich::MadeByMe => 28 + 14 + LIST_ROWS as i32 * ROW_H + 10,
        }
    }

    pub fn list(&self, which: OffersWhich) -> Rect {
        let y = self.head_y(which) + 14;
        self.p.r(6, y, self.lw(), LIST_ROWS as i32 * ROW_H)
    }

    pub fn row(&self, which: OffersWhich, k: usize) -> Rect {
        let l = self.list(which);
        Rect::new(l.x, l.y + ROW_H * k as i32, l.w, ROW_H as u16)
    }

    pub fn live(&self) -> Rect {
        self.p.r(6 + self.lw() - 90, 26, 90, 14)
    }

    /// The answer buttons in the details column: up to three, two rows.
    pub fn answer(&self, n: usize, of: usize) -> Rect {
        let (buy, offer) = (self.p.buy(), self.p.offer());
        let half = (buy.w - 4) / 2;
        match (of, n) {
            (3, 0) => Rect::new(buy.x, buy.y, half, buy.h),
            (3, 1) => offer,
            (3, _) => Rect::new(buy.x + i32::from(half) + 4, buy.y, half, buy.h),
            (2, 0) => buy,
            _ => offer,
        }
    }

    pub fn note(&self) -> Rect {
        self.p.r(6, self.p.h - 18, self.lw(), 14)
    }

    pub fn deal_box(&self) -> Rect {
        self.p
            .r((self.p.w - 300) / 2, (self.p.h - 190) / 2, 300, 190)
    }

    pub fn yes(&self) -> Rect {
        let b = self.deal_box();
        Rect::new(b.x + 20, b.y + 158, 120, 22)
    }

    pub fn no(&self) -> Rect {
        let b = self.deal_box();
        Rect::new(b.x + 160, b.y + 158, 120, 22)
    }
}

/// The tab's parts shown now (the confirm, when open, alone).
pub fn spots(ui: &MarketUi, p: &Places) -> Vec<(OfferSpot, Rect)> {
    let op = OfferPlaces { p };
    let mut v = Vec::new();
    if ui.offers.deal.is_some() {
        v.push((OfferSpot::Yes, op.yes()));
        v.push((OfferSpot::No, op.no()));
        return v;
    }
    v.push((OfferSpot::LiveOnly, op.live()));
    for which in OffersWhich::ALL {
        let k = usize::from(which.code());
        let n = ui.offers.page(which).map_or(0, |p| p.rows.len());
        for (slot, i) in (ui.offers.top[k]..n).take(LIST_ROWS).enumerate() {
            v.push((OfferSpot::Row(which, i), op.row(which, slot)));
        }
    }
    if let Some((which, o)) = ui.offers.picked_row() {
        let a = answers(which, o);
        for (n, how) in a.iter().enumerate() {
            v.push((OfferSpot::Answer(*how), op.answer(n, a.len())));
        }
    }
    v
}

/// A click on a part of the tab.
pub fn click(ui: &mut MarketUi, s: OfferSpot) {
    match s {
        OfferSpot::Row(w, i) => ui.pick_offer(w, i),
        OfferSpot::LiveOnly => ui.toggle_live_only(),
        OfferSpot::Answer(h) => ui.answer_offer(h),
        OfferSpot::Yes => ui.deal_yes(),
        OfferSpot::No => ui.offers.deal = None,
    }
}

/// The wheel over a list.
pub fn wheel(ui: &mut MarketUi, p: &Places, at: Point, steps: i32) {
    let op = OfferPlaces { p };
    for which in OffersWhich::ALL {
        if op.list(which).contains(at) {
            let k = usize::from(which.code());
            let n = ui.offers.page(which).map_or(0, |p| p.rows.len());
            let t = ui.offers.top[k] as i64 - i64::from(steps);
            ui.offers.top[k] = t.clamp(0, n.saturating_sub(LIST_ROWS) as i64) as usize;
        }
    }
}

/// The parts of a price drawn one after another from `x`, cut at `right`;
/// the items' rectangles (for their tips).
fn draw_parts(
    pen: &Pen,
    (x, y, right): (i32, i32, i32),
    parts: &[(String, u16)],
    out: &mut dyn UiDrawSink,
) -> i32 {
    let mut x = x;
    for (n, (s, c)) in parts.iter().enumerate() {
        let s = if n + 1 < parts.len() {
            format!("{s} +")
        } else {
            s.clone()
        };
        let room = right - x;
        if room < 20 {
            break;
        }
        let s = pen.fit(SMALL, &s, room);
        pen.text(x, y, &s, SMALL, *c, out);
        x += pen.width(SMALL, &s) + 4;
    }
    x
}

/// The state words of an offer row.
fn state_text(o: &OfferRow, now: u64) -> String {
    let when = if o.answered_ms > 0 {
        o.answered_ms
    } else {
        o.at_ms
    };
    format!("{} · {}", o.state.label(), age(now, when))
}

/// Draws the tab.
pub fn draw(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen, k) = (i.ui, &i.places, &i.pen, i.pen.kit);
    let op = OfferPlaces { p };
    for which in OffersWhich::ALL {
        let page = ui.offers.page(which);
        let total = page.map_or(0, |p| p.total);
        let head = match which {
            OffersWhich::OnMyListings => format!("OFFERS ON YOUR LISTINGS ({total})"),
            OffersWhich::MadeByMe => format!("YOUR OFFERS ({total})"),
        };
        pen.text(
            p.x0 + 8,
            p.y0 + op.head_y(which) + 2,
            &head,
            SMALL,
            GREY,
            out,
        );
        let list = op.list(which);
        pen.fill(list, k.field, out);
        pen.frame(list, k.edge, out);
        let Some(page) = page else {
            pen.text(list.x + 6, list.y + 6, "Asking...", SMALL, GREY, out);
            continue;
        };
        if page.rows.is_empty() {
            let s = match which {
                OffersWhich::OnMyListings => "No offers on your listings.",
                OffersWhich::MadeByMe => "You have no offers out.",
            };
            pen.text(list.x + 6, list.y + 6, s, SMALL, GREY, out);
        }
        let kk = usize::from(which.code());
        for (slot, o) in page
            .rows
            .iter()
            .enumerate()
            .skip(ui.offers.top[kk])
            .take(LIST_ROWS)
        {
            let r = op.row(which, slot - ui.offers.top[kk]);
            let picked = ui.offers.picked == Some((which, o.offer));
            if picked {
                pen.fill(r, k.on, out);
                pen.frame(r, k.gold, out);
            } else if r.contains(pen.mouse) {
                pen.fill(r, k.hit, out);
            }
            draw_row(i, which, o, r, page.now_ms, out);
        }
    }
    pen.button(
        op.live(),
        if ui.offers.live_only {
            "Live only"
        } else {
            "All offers"
        },
        !ui.offers.live_only,
        out,
    );
    draw_details(i, out);
    if let Some(n) = &ui.note {
        pen.line(op.note(), n, SMALL, GOLD, out);
    }
}

fn draw_row(
    i: &DrawIn,
    which: OffersWhich,
    o: &OfferRow,
    r: Rect,
    now: u64,
    out: &mut dyn UiDrawSink,
) {
    let (ui, pen) = (i.ui, &i.pen);
    let right = r.x + i32::from(r.w) - 4;
    let state = state_text(o, now);
    let sw = pen.width(SMALL, &state);
    let color = match o.state {
        OfferState::Open | OfferState::Countered => GOLD,
        OfferState::Accepted => 2,
        _ => GREY,
    };
    pen.text_right(right, r.y + 2, &state, SMALL, color, out);
    let name = pen.fit(
        SMALL,
        &o.item.name,
        (i32::from(r.w) / 2).min(i32::from(r.w) - sw - 24),
    );
    pen.text(r.x + 4, r.y + 2, &name, SMALL, o.group.color(), out);
    let x = r.x + 8 + pen.width(SMALL, &name);
    let listed = format!("· {}", listing_price_of(ui, o.price.as_ref()));
    let room = right - sw - 8 - x;
    if room > 20 {
        pen.text(x, r.y + 2, &pen.fit(SMALL, &listed, room), SMALL, GREY, out);
    }
    let y = r.y + 12;
    let who = match which {
        OffersWhich::OnMyListings => format!("from {}:", o.buyer),
        OffersWhich::MadeByMe => "you offer:".to_owned(),
    };
    pen.text(r.x + 8, y, &who, SMALL, WHITE, out);
    let x = r.x + 12 + pen.width(SMALL, &who);
    let items = named(o);
    let offered = price_parts(ui, &o.offered, &items);
    let x = draw_parts(pen, (x, y, right), &offered, out);
    if let Some(c) = &o.counter {
        let head = "-> countered:";
        if right - x > pen.width(SMALL, head) + 24 {
            pen.text(x, y, head, SMALL, TAN, out);
            let x = x + pen.width(SMALL, head) + 4;
            draw_parts(pen, (x, y, right), &price_parts(ui, c, &items), out);
        }
    }
}

/// A line of the picked offer's column: its text, colour, font, top,
/// and the offered item it names (for its tip).
struct DLine {
    text: String,
    color: u16,
    font: u16,
    y: i32,
    item: Option<u32>,
}

/// The lines of a column, laid down from a top, kept while they fit.
struct Column {
    v: Vec<DLine>,
    y: i32,
    bottom: i32,
}

impl Column {
    fn push(&mut self, text: String, color: u16, item: Option<u32>) {
        if self.y + 10 <= self.bottom {
            self.v.push(DLine {
                text,
                color,
                font: SMALL,
                y: self.y,
                item,
            });
        }
        self.y += 10;
    }

    /// A price's parts under `head`, one a line; items by name.
    fn price(&mut self, ui: &MarketUi, o: &OfferRow, head: &str, pr: &Price) {
        self.push(head.to_owned(), GREY, None);
        if pr.gold > 0 {
            self.push(format!("  {} gold", thousands(pr.gold)), GOLD, None);
        }
        for (c, n) in &pr.stacks {
            let s = format!("  {n} {}", super::details::coin_name(ui, c));
            self.push(s, 8, None);
        }
        for id in &pr.items {
            match o.named(*id) {
                Some(it) => self.push(format!("  {}", it.item.name), it.group.color(), Some(*id)),
                None => self.push(format!("  item {id}"), WHITE, Some(*id)),
            }
        }
    }
}

/// The picked offer's column, laid out once for the draw and the tips.
fn detail_lines(ui: &MarketUi, p: &Places, which: OffersWhich, o: &OfferRow) -> Vec<DLine> {
    let r = p.details();
    let now = ui.offers.page(which).map_or(0, |p| p.now_ms);
    let mut c = Column {
        v: vec![DLine {
            text: o.item.name.clone(),
            color: o.group.color(),
            font: LABEL,
            y: r.y + 6,
            item: None,
        }],
        y: r.y + 20,
        bottom: p.buy().y - 6,
    };
    let tip = super::tip::view_lines(o.group, &o.item, 1);
    for l in tip.iter().skip(1).take(4) {
        c.push(String::from_utf16_lossy(&l.text), l.color, None);
    }
    c.y += 4;
    let listed = format!("Listed: {}", listing_price_of(ui, o.price.as_ref()));
    c.push(listed, TAN, None);
    let who = match which {
        OffersWhich::OnMyListings => format!("Offer from {} · {}", o.buyer, age(now, o.at_ms)),
        OffersWhich::MadeByMe => format!("Your offer to {} · {}", o.seller, age(now, o.at_ms)),
    };
    c.push(who, WHITE, None);
    c.y += 2;
    c.price(ui, o, "OFFERED", &o.offered);
    if let Some(counter) = &o.counter {
        c.price(ui, o, "COUNTER (final)", counter);
    }
    let state = match (which, o.state) {
        (OffersWhich::MadeByMe, OfferState::Countered) => "Countered: accept or withdraw.".into(),
        (OffersWhich::MadeByMe, OfferState::Open) => "Waiting for the seller.".into(),
        (OffersWhich::OnMyListings, OfferState::Countered) => {
            "Countered: waiting for the buyer.".into()
        }
        (_, s) => format!("This offer is {}.", s.label()),
    };
    c.push(state, TAN, None);
    c.v
}

/// The right column: the picked offer.
fn draw_details(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen, k) = (i.ui, &i.places, &i.pen, i.pen.kit);
    let r = p.details();
    pen.fill(r, k.field, out);
    pen.frame(r, k.edge, out);
    let (x, w) = (r.x + 6, i32::from(r.w) - 12);
    let Some((which, o)) = ui.offers.picked_row() else {
        pen.text(x, r.y + 8, "Pick an offer to answer it.", SMALL, GREY, out);
        return;
    };
    for l in detail_lines(ui, p, which, o) {
        if l.item.is_some() && Rect::new(r.x, l.y, r.w, 10).contains(pen.mouse) {
            pen.fill(Rect::new(r.x + 2, l.y, r.w - 4, 10), k.hit, out);
        }
        pen.text(x, l.y, &pen.fit(l.font, &l.text, w), l.font, l.color, out);
    }
    let a = answers(which, o);
    let op = OfferPlaces { p };
    for (n, how) in a.iter().enumerate() {
        pen.button(op.answer(n, a.len()), answer_label(which, *how), false, out);
    }
}

/// The offered item under the mouse in the picked offer's column, with
/// its line.
fn hovered_item<'a>(ui: &'a MarketUi, p: &Places, mouse: Point) -> Option<(&'a NamedItem, Rect)> {
    let (which, o) = ui.offers.picked_row()?;
    let r = p.details();
    detail_lines(ui, p, which, o).into_iter().find_map(|l| {
        let line = Rect::new(r.x, l.y, r.w, 10);
        line.contains(mouse)
            .then_some(l.item)
            .flatten()
            .and_then(|id| o.named(id))
            .map(|it| (it, line))
    })
}

/// The tip under the mouse in the tab: an offered item's, or a row's
/// listing's.
pub fn hover_tip(i: &DrawIn) -> Option<(Vec<TipLine>, Rect)> {
    let ui = i.ui;
    if ui.offers.deal.is_some() || ui.form.is_some() {
        return None;
    }
    if let Some((it, r)) = hovered_item(ui, &i.places, i.pen.mouse) {
        return Some((offer_form::item_tip(it, i.tips), r));
    }
    let op = OfferPlaces { p: &i.places };
    for which in OffersWhich::ALL {
        let k = usize::from(which.code());
        let n = ui.offers.page(which).map_or(0, |p| p.rows.len());
        for (slot, idx) in (ui.offers.top[k]..n).take(LIST_ROWS).enumerate() {
            let r = op.row(which, slot);
            if r.contains(i.pen.mouse) {
                let o = ui.offers.row(which, idx)?;
                return Some((super::tip::view_lines(o.group, &o.item, 1), r));
            }
        }
    }
    None
}

/// Draws the answer confirm, when open.
pub fn draw_deal(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, pen) = (i.ui, &i.pen);
    let Some(d) = &ui.offers.deal else { return };
    let op = OfferPlaces { p: &i.places };
    let b = op.deal_box();
    let k = pen.kit;
    pen.fill(b, k.panel, out);
    pen.frame(b, k.gold, out);
    pen.frame(Rect::new(b.x + 2, b.y + 2, b.w - 4, b.h - 4), k.edge, out);
    let title = match d.how {
        AnswerHow::Accept => "Accept this offer?",
        AnswerHow::Decline => "Decline this offer?",
        AnswerHow::Withdraw => "Withdraw your offer?",
        AnswerHow::Counter => "Counter this offer?",
    };
    pen.label(Rect::new(b.x, b.y + 6, b.w, 14), title, LABEL, GOLD, out);
    let (x, w) = (b.x + 16, i32::from(b.w) - 32);
    let mut y = b.y + 26;
    let line = |y: &mut i32, s: &str, c: u16, out: &mut dyn UiDrawSink| {
        if *y < b.y + 150 {
            pen.text(x, *y, &pen.fit(SMALL, s, w), SMALL, c, out);
        }
        *y += 11;
    };
    line(&mut y, &d.name, WHITE, out);
    if !d.gets.is_empty() {
        let head = if d.how == AnswerHow::Withdraw {
            "Back to your Deliveries:"
        } else {
            "You get (in Deliveries):"
        };
        line(&mut y, head, TAN, out);
        for (s, c) in &d.gets {
            line(&mut y, &format!("  {s}"), *c, out);
        }
    }
    if !d.pays.is_empty() {
        line(&mut y, "You pay:", TAN, out);
        for (s, c) in &d.pays {
            line(&mut y, &format!("  {s}"), *c, out);
        }
    }
    if d.fee > 0 {
        line(
            &mut y,
            &format!("Market fee: {} gold", thousands(d.fee)),
            TAN,
            out,
        );
    }
    if d.how == AnswerHow::Decline {
        line(&mut y, "The buyer's goods go back to them.", GREY, out);
    }
    pen.button(
        op.yes(),
        answer_label(OffersWhich::OnMyListings, d.how),
        false,
        out,
    );
    pen.button(op.no(), "Cancel", false, out);
}

/// A price as the details read it, for tests.
pub fn deal_text(d: &Deal) -> String {
    let join = |v: &[(String, u16)]| {
        v.iter()
            .map(|(s, _)| s.as_str())
            .collect::<Vec<_>>()
            .join(" + ")
    };
    format!("gets {} · pays {}", join(&d.gets), join(&d.pays))
}

/// The offer's price, exactly, for tests and the note.
pub fn offered_text(ui: &MarketUi, o: &OfferRow) -> String {
    offer_form::bundle_text(ui, &o.offered, &named(o))
}

#[cfg(test)]
mod tests {
    use super::super::offer_form::{FormKind, FormSpot};
    use super::super::tests::{click, ids, open, places};
    use super::super::window::{self, Spot};
    use super::*;
    use crate::market::fake::browse::NOW_MS;
    use crate::market::fake::offers::ME;
    use crate::market::fake::FakeSource;
    use crate::market::source::{MarketEvent, MarketSource};
    use crate::market::trade::{DeliverySource, Holdings};

    /// The window open on the Offers tab with both lists shown.
    fn offers_tab() -> (MarketUi, FakeSource) {
        let (mut ui, mut src) = open();
        click(
            &mut ui,
            &mut src,
            Spot::Tab(super::super::MarketTab::Offers),
        );
        assert!(ui.offers.page(OffersWhich::OnMyListings).is_some());
        assert!(ui.offers.page(OffersWhich::MadeByMe).is_some());
        (ui, src)
    }

    fn row(which: OffersWhich, i: usize) -> Spot {
        Spot::Offers(OfferSpot::Row(which, i))
    }

    fn answer(how: AnswerHow) -> Spot {
        Spot::Offers(OfferSpot::Answer(how))
    }

    /// The index of the first row of list `which` that `f` takes.
    fn index_of(ui: &MarketUi, which: OffersWhich, f: impl Fn(&OfferRow) -> bool) -> usize {
        let rows = &ui.offers.page(which).unwrap().rows;
        rows.iter().position(f).expect("a row")
    }

    #[test]
    fn offer_rows_show_items() {
        let (mut ui, mut src) = offers_tab();
        let mine = OffersWhich::OnMyListings;
        let i = index_of(&ui, mine, |o| o.buyer == "Necrolord");
        let o = ui.offers.row(mine, i).unwrap().clone();
        // Gold, currencies and the offered stash item by name.
        assert_eq!(
            offered_text(&ui, &o),
            "3 r15 + 1 r24 + Sapphire Jewel of Fervor"
        );
        assert_eq!(o.items.len(), 1);
        // Rows carry their ids; picking one shows its answers.
        let shown = ids(&ui);
        assert!(
            shown.contains(&format!("market.offers.mine({i})")),
            "{shown:?}"
        );
        assert!(shown.contains(&"market.offers.made(0)".to_owned()));
        click(&mut ui, &mut src, row(mine, i));
        assert_eq!(ui.offers.picked, Some((mine, o.offer)));
        let shown = ids(&ui);
        for id in [
            "market.offer.accept",
            "market.offer.counter",
            "market.offer.decline",
        ] {
            assert!(shown.contains(&id.to_owned()), "{id} in {shown:?}");
        }
        // The countered offer the player made shows both prices.
        let made = OffersWhich::MadeByMe;
        let c = index_of(&ui, made, |o| o.state == OfferState::Countered);
        let o = ui.offers.row(made, c).unwrap();
        assert_eq!(o.buyer, ME);
        assert_eq!(
            offer_form::bundle_text(&ui, o.counter.as_ref().unwrap(), &[]),
            "1 r20 + 1 r24"
        );
        // Live only by default; the other choice lists answered offers too.
        let live = ui.offers.page(mine).unwrap().rows.len();
        click(&mut ui, &mut src, Spot::Offers(OfferSpot::LiveOnly));
        assert!(!ui.offers.live_only);
        assert!(ui.offers.page(mine).unwrap().rows.len() >= live);
    }

    #[test]
    fn counter_form_keeps_items() {
        let (mut ui, mut src) = offers_tab();
        let mine = OffersWhich::OnMyListings;
        let i = index_of(&ui, mine, |o| o.buyer == "Necrolord");
        let id = ui.offers.row(mine, i).unwrap().offer;
        click(&mut ui, &mut src, row(mine, i));
        click(&mut ui, &mut src, answer(AnswerHow::Counter));
        let f = ui.form.clone().expect("the counter form");
        // It starts as the offer: its currencies, every offered item kept.
        assert!(matches!(f.kind, FormKind::Counter { offer, .. } if offer == id));
        assert_eq!(f.items.iter().copied().collect::<Vec<_>>(), [8101]);
        // It lists only the offered items: a counter keeps some of them.
        assert_eq!(f.listed_items(&ui).len(), 1);
        // The offer itself is no counter.
        click(&mut ui, &mut src, Spot::Form(FormSpot::Send));
        assert!(ui.form.as_ref().unwrap().error.is_some());
        // One more Rune 24, the jewel dropped and taken back.
        let coins = ui.form.as_ref().unwrap().coins(&ui);
        let k = coins.iter().position(|c| c == "r24").unwrap();
        click(&mut ui, &mut src, Spot::Form(FormSpot::More(k)));
        click(&mut ui, &mut src, Spot::Form(FormSpot::Item(0)));
        assert!(ui.form.as_ref().unwrap().items.is_empty());
        click(&mut ui, &mut src, Spot::Form(FormSpot::Item(0)));
        click(&mut ui, &mut src, Spot::Form(FormSpot::Send));
        assert!(ui.form.as_ref().unwrap().review, "the last look first");
        click(&mut ui, &mut src, Spot::Form(FormSpot::Send));
        assert!(ui.form.is_none());
        let o = src.trade.offers.iter().find(|o| o.offer == id).unwrap();
        assert_eq!(o.state, OfferState::Countered);
        let c = o.counter.clone().unwrap();
        assert_eq!(c.items, [8101]);
        assert_eq!(c.stacks, [("r15".to_owned(), 3), ("r24".to_owned(), 2)]);
        let note = ui.note.clone().unwrap();
        assert!(note.starts_with("Countered"), "{note}");
        // The list shows it countered, waiting on the buyer.
        ui.pump(&mut src);
        let rows = &ui.offers.page(mine).unwrap().rows;
        let o = rows.iter().find(|o| o.offer == id).unwrap();
        assert_eq!(o.state, OfferState::Countered);
        assert!(answers(mine, o).is_empty());
    }

    #[test]
    fn withdraw_only_for_buyer() {
        let (mut ui, mut src) = offers_tab();
        let (mine, made) = (OffersWhich::OnMyListings, OffersWhich::MadeByMe);
        // An offer on the player's listing: no Withdraw.
        click(&mut ui, &mut src, row(mine, 0));
        assert!(!ids(&ui).contains(&"market.offer.withdraw".to_owned()));
        let to_me = ui.offers.row(mine, 0).unwrap().clone();
        assert!(!answers(mine, &to_me).contains(&AnswerHow::Withdraw));
        // The player's own offers: Withdraw; a counter also Accept.
        let c = index_of(&ui, made, |o| o.state == OfferState::Countered);
        let open_one = index_of(&ui, made, |o| o.state == OfferState::Open);
        let countered = ui.offers.row(made, c).unwrap().clone();
        assert_eq!(
            answers(made, &countered),
            [AnswerHow::Accept, AnswerHow::Withdraw]
        );
        let o = ui.offers.row(made, open_one).unwrap().clone();
        assert_eq!(answers(made, &o), [AnswerHow::Withdraw]);
        // The market refuses a seller's withdraw too.
        let req = src.send(MarketAsk::Answer {
            offer: to_me.offer,
            how: AnswerHow::Withdraw,
            counter: None,
            fee_rev: ui.fees().fee_rev,
        });
        let refused = src.poll().into_iter().any(
            |e| matches!(e, MarketEvent::Done(d) if d.req == req && d.code == MarketCode::NotYours),
        );
        assert!(refused);
        // Withdraw: the confirm says what comes back; yes sends it, and
        // the offered goods wait in Deliveries.
        let before = src.trade.deliveries.len();
        click(&mut ui, &mut src, row(made, open_one));
        assert!(ids(&ui).contains(&"market.offer.withdraw".to_owned()));
        click(&mut ui, &mut src, answer(AnswerHow::Withdraw));
        let d = ui.offers.deal.clone().expect("the confirm");
        assert_eq!(deal_text(&d), "gets 50,000 gold · pays ");
        assert_eq!(ids(&ui), ["market.deal.yes", "market.deal.no"]);
        click(&mut ui, &mut src, Spot::Offers(OfferSpot::Yes));
        let w = src
            .trade
            .offers
            .iter()
            .find(|x| x.offer == o.offer)
            .unwrap();
        assert_eq!(w.state, OfferState::Withdrawn);
        assert_eq!(src.trade.deliveries.len(), before + 1);
        let note = ui.note.clone().unwrap();
        assert!(note.starts_with("Withdrew"), "{note}");
    }

    #[test]
    fn accepting_a_counter_pays_the_rest() {
        let (mut ui, mut src) = offers_tab();
        let made = OffersWhich::MadeByMe;
        let c = index_of(&ui, made, |o| o.state == OfferState::Countered);
        let o = ui.offers.row(made, c).unwrap().clone();
        let r20 = src.trade.stash.stacks["r20"];
        click(&mut ui, &mut src, row(made, c));
        click(&mut ui, &mut src, answer(AnswerHow::Accept));
        let d = ui.offers.deal.clone().unwrap();
        assert_eq!(
            deal_text(&d),
            format!("gets {} · pays 1 r20 + 1 r24", o.item.name)
        );
        click(&mut ui, &mut src, Spot::Offers(OfferSpot::Yes));
        // The counter's extra Rune 20 left the stash; the goods wait.
        assert_eq!(src.trade.stash.stacks["r20"], r20 - 1);
        let bought = src
            .trade
            .deliveries
            .iter()
            .any(|d| d.offer == Some(o.offer) && d.source == DeliverySource::Bought);
        assert!(bought);
        assert!(src.listing(o.listing).is_none(), "sold to the player");
    }

    #[test]
    fn offer_form_builds_a_bundle() {
        let (mut ui, mut src) = open();
        let i = ui
            .results
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .position(|l| l.mode.takes_offers() && !l.mine)
            .unwrap();
        let listing = ui.listing(i).unwrap().listing;
        click(&mut ui, &mut src, Spot::Result(i));
        click(&mut ui, &mut src, Spot::Offer);
        let held: Holdings = ui.holdings.clone().expect("the stash is known");
        // Gold typed, a currency stepped up, a stash item picked.
        click(&mut ui, &mut src, Spot::Form(FormSpot::Gold));
        for c in "25000".encode_utf16() {
            let e = crate::ui::panel::UiEvent::Char(c);
            assert!(window::event(&mut ui, &places(), e, false));
        }
        click(&mut ui, &mut src, Spot::Form(FormSpot::More(0)));
        click(&mut ui, &mut src, Spot::Form(FormSpot::Item(1)));
        let p = ui.form.as_ref().unwrap().price();
        assert_eq!(p.gold, 25_000);
        assert_eq!(p.stacks.len(), 1);
        assert_eq!(p.items, [held.items[1].item_id]);
        // Never more of a currency than the stash holds.
        let code = p.stacks[0].0.clone();
        for _ in 0..20 {
            click(&mut ui, &mut src, Spot::Form(FormSpot::More(0)));
        }
        assert_eq!(ui.form.as_ref().unwrap().stacks[&code], held.stacks[&code]);
        click(&mut ui, &mut src, Spot::Form(FormSpot::Send));
        click(&mut ui, &mut src, Spot::Form(FormSpot::Send));
        assert!(ui.form.is_none());
        // The offer is made and paid into escrow from the stash.
        let o = src
            .trade
            .offers
            .iter()
            .find(|o| o.listing == listing && o.at_ms == NOW_MS)
            .expect("the offer");
        assert_eq!(o.offered.gold, 25_000);
        assert_eq!(o.items.len(), 1);
        assert_eq!(src.trade.stash.gold, held.gold - 25_000);
        assert!(!src.trade.stash.stacks.contains_key(&code));
        let note = ui.note.clone().unwrap();
        assert!(note.starts_with("Offer made"), "{note}");
    }
}
