// D2RUST's own: selling from the shared stash (the stash's Market category).
//! The New listing form (mock 2 of `docs/plans/market-ui.md`, module K3):
//! at the town stash, the shared stash panel's Market category shows the
//! shared item pages, and this form stands beside the panel, over the
//! inventory's place (listings come from the shared stash only).
//!
//! - The Sell slot takes a shared item: picked up from the page with a
//!   press and dropped on the slot (a drag, or a click then a click), or
//!   Ctrl+Left straight onto it. An item of the character's (on the
//!   cursor) is refused: it has to go into the shared stash first.
//! - Fixed price, Offers only or Price + offers; the price is gold and the
//!   realm's currencies ([`PriceDraft`], also My shop's Set price...).
//! - How long it runs (up to the realm's longest), the slots left, a
//!   public note, "post hidden", and the fee lines (none while the fees
//!   are 0).
//! - Post listing sends a `MarketPost` with the fee revision the player
//!   saw; the goods go into escrow at once ([`SellForm::escrowed`]).
//! - The slot's item is price checked as it lands: the "Similar active /
//!   last sold" line under the price, all of it on hover, and Price check
//!   opens the graph over the form ([`super::price_graph`]).
//!
//! Its parts are named UI elements (`market.sell.slot`,
//! `market.sell.mode(offers)`, `market.sell.coin(r23)`, `market.sell.post`,
//! `market.sell.price_check`, `market.sell.similar`).

use crate::market::source::{
    Group, ItemView, ListingMode, MarketAsk, MarketCode, MarketDone, MarketFees, Post, PostGoods,
    Price, Req, StashGoods, NOTE_MAX,
};
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::{Point, Rect};
use crate::ui::layout::Screen;
use crate::ui::panel::{PointerButton, UiEvent};

use super::details::{coin_name, thousands};
use super::window::{Pen, GOLD, GREY, LABEL, RED, SMALL, TAN};
use super::{Availability, MarketUi};

/// A shared stash item as the sell slot shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SellItem {
    /// The stash's item ID.
    pub id: u32,
    pub name: String,
    pub code: String,
    pub group: Group,
    /// In cells.
    pub size: (u8, u8),
    /// The item as the game client showed it (picture and tip).
    pub look: Option<Vec<u8>>,
    pub level_req: u8,
    pub sockets: u8,
    pub ethereal: bool,
    /// The tip's lines after the name.
    pub stats: Vec<String>,
}

impl SellItem {
    /// What the market is told the slot shows.
    pub fn goods(&self) -> StashGoods {
        StashGoods {
            item_id: self.id,
            group: self.group,
            record: Vec::new(),
            item: ItemView {
                name: self.name.clone(),
                base: self.code.clone(),
                code: self.code.clone(),
                item_type: String::new(),
                level_req: self.level_req,
                sockets: self.sockets,
                ethereal: self.ethereal,
                identified: true,
                stats: self.stats.clone(),
            },
        }
    }
}

/// The browser's group of a save-format quality (1 low ... 8 crafted).
pub fn group_of_quality(q: u8) -> Group {
    match q {
        4 => Group::Magic,
        5 => Group::Set,
        6 => Group::Rare,
        7 => Group::Unique,
        8 => Group::Crafted,
        _ => Group::Other,
    }
}

/// The colour code of an item's stat lines.
const BLUE: u16 = 3;

/// The most currencies one price names here.
pub const COINS_MAX: usize = 8;
/// The most of one currency.
const COIN_MAX: u32 = 999;
/// The most digits the gold box takes.
const GOLD_DIGITS: usize = 12;

/// A price being written: the mode, the gold as typed and the
/// currencies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriceDraft {
    pub mode: ListingMode,
    pub gold: String,
    /// `(currency code, count)`, in the order added.
    pub stacks: Vec<(String, u32)>,
    /// The currency picker is open.
    pub picking: bool,
}

impl Default for PriceDraft {
    fn default() -> Self {
        PriceDraft {
            mode: ListingMode::Price,
            gold: String::new(),
            stacks: Vec::new(),
            picking: false,
        }
    }
}

impl PriceDraft {
    /// A draft of `mode` and `price` (Set price... starts from the
    /// listing's).
    pub fn of(mode: ListingMode, price: Option<&Price>) -> Self {
        let p = price.cloned().unwrap_or_default();
        PriceDraft {
            mode,
            gold: if p.gold > 0 {
                p.gold.to_string()
            } else {
                String::new()
            },
            stacks: p.stacks,
            picking: false,
        }
    }

    /// The price to send: `None` for offers only; a priced mode needs
    /// gold, currencies or both.
    pub fn price(&self) -> Result<Option<Price>, &'static str> {
        if !self.mode.has_price() {
            return Ok(None);
        }
        let gold = if self.gold.is_empty() {
            0
        } else {
            self.gold
                .parse::<u64>()
                .map_err(|_| "That much gold can't be a price.")?
        };
        let mut stacks: Vec<(String, u32)> = self
            .stacks
            .iter()
            .filter(|(_, n)| *n > 0)
            .cloned()
            .collect();
        stacks.sort();
        let p = Price {
            gold,
            stacks,
            items: Vec::new(),
        };
        if p.is_empty() {
            return Err("Set a price: gold, currencies or both.");
        }
        Ok(Some(p))
    }

    /// One more of currency `code` (added when new).
    pub fn add_coin(&mut self, code: &str) {
        if let Some(e) = self.stacks.iter_mut().find(|(c, _)| c == code) {
            e.1 = (e.1 + 1).min(COIN_MAX);
        } else if self.stacks.len() < COINS_MAX {
            self.stacks.push((code.to_owned(), 1));
        }
    }

    /// One less of the `i`th currency (gone at 0).
    pub fn sub_coin(&mut self, i: usize) {
        if let Some(e) = self.stacks.get_mut(i) {
            e.1 = e.1.saturating_sub(1);
            if e.1 == 0 {
                self.stacks.remove(i);
            }
        }
    }

    /// A typed character into the gold box: digits only.
    fn type_gold(&mut self, c: char) {
        if c.is_ascii_digit()
            && self.gold.len() < GOLD_DIGITS
            && !(self.gold.is_empty() && c == '0')
        {
            self.gold.push(c);
        }
    }
}

/// The modes in the chips' order.
pub const MODES: [ListingMode; 3] = [
    ListingMode::Price,
    ListingMode::OffersOnly,
    ListingMode::PriceAndOffers,
];

/// A mode's chip.
pub fn mode_label(m: ListingMode) -> &'static str {
    match m {
        ListingMode::Price => "Fixed price",
        ListingMode::OffersOnly => "Offers only",
        ListingMode::PriceAndOffers => "Price + offers",
    }
}

/// The word of a mode's UI id.
pub fn mode_key(m: ListingMode) -> &'static str {
    match m {
        ListingMode::Price => "fixed",
        ListingMode::OffersOnly => "offers",
        ListingMode::PriceAndOffers => "both",
    }
}

/// A part of the price builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceSpot {
    Mode(ListingMode),
    Gold,
    /// The `i`th currency of the draft.
    Coin(usize),
    AddCoin,
    /// The `i`th of the realm's currencies, in the picker.
    Pick(usize),
}

impl PriceSpot {
    /// Its UI id under `prefix` (`market.sell`).
    pub fn id(self, prefix: &str, d: &PriceDraft, currencies: &[String]) -> String {
        match self {
            PriceSpot::Mode(m) => format!("{prefix}.mode({})", mode_key(m)),
            PriceSpot::Gold => format!("{prefix}.gold"),
            PriceSpot::Coin(i) => format!(
                "{prefix}.coin({})",
                d.stacks.get(i).map_or("", |(c, _)| c.as_str())
            ),
            PriceSpot::AddCoin => format!("{prefix}.add_coin"),
            PriceSpot::Pick(i) => format!(
                "{prefix}.pick({})",
                currencies.get(i).map_or("", String::as_str)
            ),
        }
    }
}

/// The price builder's height.
pub const BUILDER_H: i32 = 90;
const CHIP_W: i32 = 70;
const CHIP_STEP: i32 = 72;
const CHIP_H: i32 = 14;
const ADD_W: i32 = 84;
const PICK_W: i32 = 58;
const PICK_H: i32 = 12;
const PICK_COLS: usize = 5;

/// The builder's parts with its top-left at `o`, `w` wide; the picker's
/// last (it is drawn over the rest).
pub fn price_spots(
    d: &PriceDraft,
    currencies: &[String],
    o: Point,
    w: i32,
) -> Vec<(PriceSpot, Rect)> {
    let r = |x: i32, y: i32, w: i32, h: i32| Rect::new(o.x + x, o.y + y, w.max(0) as u16, h as u16);
    let mut v = Vec::new();
    let mw = (w - 8) / 3;
    for (i, m) in MODES.into_iter().enumerate() {
        v.push((PriceSpot::Mode(m), r((mw + 4) * i as i32, 0, mw, 16)));
    }
    if !d.mode.has_price() {
        return v;
    }
    v.push((PriceSpot::Gold, r(30, 22, 110, 16)));
    let per_row = ((w + 2) / CHIP_STEP).max(1) as usize;
    let at = |k: usize, cw: i32| {
        r(
            CHIP_STEP * (k % per_row) as i32,
            42 + 16 * (k / per_row) as i32,
            cw,
            CHIP_H,
        )
    };
    for i in 0..d.stacks.len() {
        v.push((PriceSpot::Coin(i), at(i, CHIP_W)));
    }
    if d.stacks.len() < COINS_MAX {
        v.push((PriceSpot::AddCoin, at(d.stacks.len(), ADD_W)));
    }
    if d.picking {
        let top = 42 + 16 * ((d.stacks.len() / per_row) as i32 + 1);
        for (i, _) in currencies.iter().enumerate() {
            let (cx, cy) = ((i % PICK_COLS) as i32, (i / PICK_COLS) as i32);
            v.push((
                PriceSpot::Pick(i),
                r(2 + cx * (PICK_W + 1), top + 2 + cy * PICK_H, PICK_W, PICK_H),
            ));
        }
    }
    v
}

/// A press on builder part `s` (`right`: the right button: one currency
/// less).
pub fn click_price(d: &mut PriceDraft, currencies: &[String], s: PriceSpot, right: bool) {
    if !matches!(s, PriceSpot::AddCoin | PriceSpot::Pick(_)) {
        d.picking = false;
    }
    match s {
        PriceSpot::Mode(m) => d.mode = m,
        PriceSpot::Gold => {}
        PriceSpot::Coin(i) if right => d.sub_coin(i),
        PriceSpot::Coin(i) => {
            if let Some((c, _)) = d.stacks.get(i).cloned() {
                d.add_coin(&c);
            }
        }
        PriceSpot::AddCoin => d.picking = !d.picking,
        PriceSpot::Pick(i) => {
            if let Some(c) = currencies.get(i) {
                d.add_coin(c);
            }
            d.picking = false;
        }
    }
}

/// Draws the builder (the gold box focused when `gold_focus`).
pub fn draw_price(
    pen: &Pen,
    ui: &MarketUi,
    d: &PriceDraft,
    o: Point,
    w: i32,
    gold_focus: bool,
    out: &mut dyn UiDrawSink,
) {
    let currencies = currencies(ui);
    let spots = price_spots(d, &currencies, o, w);
    let mut picker = Vec::new();
    for (s, r) in &spots {
        match *s {
            PriceSpot::Mode(m) => pen.button(*r, mode_label(m), d.mode == m, out),
            PriceSpot::Gold => {
                pen.text(o.x, r.y + 4, "Gold", SMALL, GOLD, out);
                pen.text_box(*r, &d.gold, "0", gold_focus, out);
            }
            PriceSpot::Coin(i) => {
                let (c, n) = &d.stacks[i];
                pen.button(*r, &format!("{n} {}", coin_name(ui, c)), false, out);
            }
            PriceSpot::AddCoin => pen.button(*r, "+ add currency", d.picking, out),
            PriceSpot::Pick(_) => picker.push((*s, *r)),
        }
    }
    if !d.mode.has_price() {
        let line = "Buyers make offers; you accept, counter or decline.";
        pen.text(o.x, o.y + 26, line, SMALL, TAN, out);
        return;
    }
    if let Some(p) = d.price().ok().flatten() {
        let gold = if p.gold > 0 {
            format!("= {} gold", thousands(p.gold))
        } else {
            String::new()
        };
        pen.text(o.x + 146, o.y + 26, &gold, SMALL, GREY, out);
    }
    // On the label's row: the gold total has the gold box's row.
    if !d.stacks.is_empty() && !d.picking {
        pen.text_right(o.x + w, o.y - 12, COIN_HINT, SMALL, GREY, out);
    }
    if let (Some(first), Some(last)) = (picker.first(), picker.last()) {
        let back = Rect::new(
            o.x,
            first.1.y - 2,
            w as u16,
            (last.1.y + PICK_H + 2 - (first.1.y - 2)) as u16,
        );
        pen.fill(back, pen.kit.panel, out);
        pen.frame(back, pen.kit.gold, out);
        for (s, r) in picker {
            let PriceSpot::Pick(i) = s else { continue };
            if r.contains(pen.mouse) {
                pen.fill(r, pen.kit.hit, out);
            }
            pen.line(r, coin_name(ui, &currencies[i]), SMALL, GOLD, out);
        }
    }
}

/// The realm's currencies, the picker's list.
pub fn currencies(ui: &MarketUi) -> Vec<String> {
    ui.state
        .as_ref()
        .map(|s| s.currencies.clone())
        .unwrap_or_default()
}

/// The fee lines of a post or relist; none while the fees are 0 (K6 shows
/// them on the realms that charge).
pub fn fee_lines(fees: &MarketFees) -> Vec<String> {
    let mut v = Vec::new();
    if fees.listing_fee > 0 {
        v.push(format!(
            "Listing fee: {} gold, from your shared stash.",
            thousands(fees.listing_fee)
        ));
    }
    if fees.sale_fee_pct > 0 {
        v.push(format!(
            "The market keeps {}% of the gold when it sells.",
            fees.sale_fee_pct
        ));
    }
    v
}

/// The days a listing can run, offered as chips: 1, 3 and 7 up to the
/// realm's longest, and the longest.
pub fn day_choices(max: u8) -> Vec<u8> {
    let max = max.max(1);
    let mut v: Vec<u8> = [1, 3, 7].into_iter().filter(|d| *d <= max).collect();
    if !v.contains(&max) {
        v.push(max);
    }
    v
}

/// A text box of the form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SellBox {
    Gold,
    Note,
}

/// The New listing form's state.
#[derive(Debug, Default)]
pub struct SellForm {
    /// The item on the Sell slot.
    pub item: Option<SellItem>,
    /// The item picked up from the page for the slot.
    pub held: Option<SellItem>,
    pub draft: PriceDraft,
    /// The days chosen; 0: the default (7, or the realm's longest).
    pub days: u8,
    pub note: String,
    /// Post straight to My shop's Hidden tab.
    pub hidden: bool,
    pub focus: Option<SellBox>,
    /// The post queued (not yet numbered), then its number.
    pub posting: bool,
    pub post_req: Option<Req>,
    /// The form's status line, and whether it is a refusal.
    pub line: Option<(String, bool)>,
    /// Items posted from here, in escrow now: the page leaves them out.
    pub escrowed: Vec<u32>,
    /// The items on the slot and held: the page draws them faded.
    pub faded: Vec<u32>,
    /// Stash items the market is to be told the slot showed.
    pub seen: Vec<StashGoods>,
}

impl SellForm {
    fn refade(&mut self) {
        self.faded = [&self.item, &self.held]
            .into_iter()
            .flatten()
            .map(|i| i.id)
            .collect();
    }

    /// The days a post runs: the chosen, else 7, within the realm's
    /// longest.
    pub fn days(&self, max: u8) -> u8 {
        let max = max.max(1);
        if self.days == 0 {
            7.min(max)
        } else {
            self.days.min(max)
        }
    }

    /// `it` goes on the Sell slot (the one there goes back to the page).
    pub fn put(&mut self, it: SellItem) {
        self.seen.push(it.goods());
        self.item = Some(it);
        self.held = None;
        self.line = None;
        self.refade();
    }

    /// `it` is picked up for the slot.
    pub fn hold(&mut self, it: SellItem) {
        self.held = Some(it);
        self.refade();
    }

    /// The held item goes back to the page.
    pub fn drop_held(&mut self) {
        self.held = None;
        self.refade();
    }

    /// The Sell slot is emptied.
    pub fn clear(&mut self) {
        self.item = None;
        self.refade();
    }

    /// An item of the character's was offered to the slot.
    pub fn refuse_cursor(&mut self) {
        self.line = Some((
            "List items from your shared stash: put it in the stash first.".into(),
            true,
        ));
    }
}

/// The form's rectangles: beside the stash panel, in the right panel
/// slot (over the inventory's place), in the panels' own pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormPlaces {
    x0: i32,
    y0: i32,
}

/// The form's size.
pub const FORM_W: i32 = 312;
pub const FORM_H: i32 = 412;

/// The lines beside Post listing.
pub const POST_NOTE: [&str; 2] = [
    "The item goes into escrow now;",
    "if it ends unsold, see Deliveries.",
];

/// What the currency chips do, beside SELL FOR.
pub const COIN_HINT: &str = "Currency: left +1, right -1";

impl FormPlaces {
    pub fn of(s: &Screen) -> FormPlaces {
        FormPlaces {
            x0: s.right_x0() + 4,
            y0: s.panel_dy() + 10,
        }
    }

    pub fn r(&self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect::new(self.x0 + x, self.y0 + y, w.max(0) as u16, h.max(0) as u16)
    }

    pub fn frame(&self) -> Rect {
        self.r(0, 0, FORM_W, FORM_H)
    }

    /// The Sell slot: two cells by four.
    pub fn slot(&self) -> Rect {
        self.r(8, 26, 58, 116)
    }

    /// The price builder's top-left.
    pub fn builder(&self) -> Point {
        Point::new(self.x0 + 8, self.y0 + 148)
    }

    pub fn days(&self, i: usize) -> Rect {
        self.r(8 + 40 * i as i32, 242, 36, 14)
    }

    pub fn note(&self) -> Rect {
        self.r(8, 262, FORM_W - 16, 16)
    }

    pub fn hidden(&self) -> Rect {
        self.r(8, 284, 230, 12)
    }

    pub fn post(&self) -> Rect {
        self.r(8, 328, 96, 22)
    }

    /// The Price check button, under the item's lines beside the slot.
    pub fn check(&self) -> Rect {
        self.r(FORM_W - 82, 128, 74, 14)
    }

    /// The "Similar active" line, under the price builder.
    pub fn similar(&self) -> Rect {
        self.r(8, 224, FORM_W - 16, 12)
    }

    pub fn browse(&self) -> Rect {
        self.r(8, 386, 140, 18)
    }

    pub fn shop(&self) -> Rect {
        self.r(FORM_W - 148, 386, 140, 18)
    }
}

/// A clickable part of the form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SellSpot {
    Slot,
    Price(PriceSpot),
    Days(u8),
    Note,
    Hidden,
    Post,
    /// The price check of the slot's item (the box over the form).
    PriceCheck,
    /// The "Similar active" line (its tip on hover).
    Similar,
    /// Opens the market window on Browse.
    Browse,
    /// Opens the market window on My shop.
    Shop,
}

/// The form is shown: the stash panel's Market category is open and the
/// market is.
pub fn form_shown(ui: &MarketUi, selling: bool) -> bool {
    selling && ui.availability() == Availability::Open && !ui.shown()
}

/// The form's parts, bottom-most first.
pub fn spots(ui: &MarketUi, p: &FormPlaces) -> Vec<(SellSpot, Rect)> {
    let f = &ui.sell;
    let max = ui.state.as_ref().map_or(7, |s| s.max_days);
    let mut v = vec![(SellSpot::Slot, p.slot())];
    for (i, d) in day_choices(max).into_iter().enumerate() {
        v.push((SellSpot::Days(d), p.days(i)));
    }
    v.push((SellSpot::Note, p.note()));
    v.push((SellSpot::Hidden, p.hidden()));
    if f.item.is_some() {
        v.push((SellSpot::Post, p.post()));
        v.push((SellSpot::PriceCheck, p.check()));
        if super::price_graph::sell_history(ui).is_some() {
            v.push((SellSpot::Similar, p.similar()));
        }
    }
    v.push((SellSpot::Browse, p.browse()));
    v.push((SellSpot::Shop, p.shop()));
    // The builder last: its picker covers the rows under it.
    let cur = currencies(ui);
    for (s, r) in price_spots(&f.draft, &cur, p.builder(), FORM_W - 16) {
        v.push((SellSpot::Price(s), r));
    }
    v
}

/// The top-most part at `at`.
pub fn hit(ui: &MarketUi, p: &FormPlaces, at: Point) -> Option<SellSpot> {
    spots(ui, p)
        .into_iter()
        .rev()
        .find(|(_, r)| r.contains(at))
        .map(|(s, _)| s)
}

/// The form's UI elements.
pub fn ui_elements(ui: &MarketUi, p: &FormPlaces) -> Vec<crate::ui::ui_ids::UiElement> {
    let cur = currencies(ui);
    spots(ui, p)
        .into_iter()
        .map(|(s, r)| {
            let id = match s {
                SellSpot::Slot => "market.sell.slot".into(),
                SellSpot::Price(ps) => ps.id("market.sell", &ui.sell.draft, &cur),
                SellSpot::Days(d) => format!("market.sell.days({d})"),
                SellSpot::Note => "market.sell.note".into(),
                SellSpot::Hidden => "market.sell.hidden".into(),
                SellSpot::Post => "market.sell.post".into(),
                SellSpot::PriceCheck => "market.sell.price_check".into(),
                SellSpot::Similar => "market.sell.similar".into(),
                SellSpot::Browse => "market.sell.browse".into(),
                SellSpot::Shop => "market.sell.shop".into(),
            };
            crate::ui::ui_ids::UiElement::new(id, r)
        })
        .collect()
}

/// A press on `s` (`right`: the right button).
pub fn click(ui: &mut MarketUi, s: SellSpot, right: bool) {
    let f = &mut ui.sell;
    f.focus = match s {
        SellSpot::Note => Some(SellBox::Note),
        SellSpot::Price(PriceSpot::Gold) => Some(SellBox::Gold),
        _ => None,
    };
    match s {
        SellSpot::Slot => match f.held.take() {
            Some(it) => f.put(it),
            None => f.clear(),
        },
        SellSpot::Price(ps) => {
            let cur = currencies(ui);
            click_price(&mut ui.sell.draft, &cur, ps, right);
        }
        SellSpot::Days(d) => f.days = d,
        SellSpot::Note => {}
        SellSpot::Hidden => f.hidden = !f.hidden,
        SellSpot::Post => post(ui),
        SellSpot::PriceCheck => ui.check_sell_item(),
        SellSpot::Similar => {}
        SellSpot::Browse | SellSpot::Shop => {
            if ui.toggle().is_ok() && s == SellSpot::Shop {
                ui.set_tab(super::MarketTab::MyShop);
            }
        }
    }
}

/// Post listing: the slot's item, the price, the days, the note; the fee
/// revision the player saw. A form that can't post says why.
pub fn post(ui: &mut MarketUi) {
    if ui.sell.posting || ui.sell.post_req.is_some() {
        return;
    }
    let Some(state) = ui.state.as_ref().filter(|s| s.on) else {
        ui.sell.line = Some(("The market is closed on this realm.".into(), true));
        return;
    };
    let (fee_rev, max_days) = (state.fees.fee_rev, state.max_days);
    let f = &mut ui.sell;
    let Some(it) = &f.item else {
        f.line = Some((
            "Put an item from your shared stash on the Sell slot.".into(),
            true,
        ));
        return;
    };
    let price = match f.draft.price() {
        Ok(p) => p,
        Err(why) => {
            f.line = Some((why.into(), true));
            return;
        }
    };
    if f.note.len() > NOTE_MAX {
        f.line = Some(("The note is too long.".into(), true));
        return;
    }
    let ask = MarketAsk::Post(Post {
        goods: PostGoods::Item(it.id),
        mode: f.draft.mode,
        price,
        days: f.days(max_days),
        note: f.note.trim().to_owned(),
        hidden: f.hidden,
        fee_rev,
    });
    f.line = Some((format!("Posting {}...", it.name), false));
    f.posting = true;
    f.focus = None;
    ui.asks.push(ask);
}

/// The market answered request `d`: true when it was the post's.
pub fn on_done(ui: &mut MarketUi, d: &MarketDone) -> bool {
    if ui.sell.post_req != Some(d.req) {
        return false;
    }
    let f = &mut ui.sell;
    f.post_req = None;
    match d.code {
        MarketCode::Done => {
            let name = f.item.as_ref().map_or("The item", |i| i.name.as_str());
            f.line = Some((format!("Listed: {name}, in escrow. See My shop."), false));
            if let Some(it) = f.item.take() {
                f.escrowed.push(it.id);
            }
            f.note.clear();
            f.hidden = false;
            f.refade();
            ui.asks.push(MarketAsk::Hello);
            ui.shop_changed();
        }
        _ => {
            f.line = Some((d.line(), true));
            if d.code == MarketCode::ItemNotFound {
                f.clear();
            }
            if d.code == MarketCode::FeeChanged {
                ui.asks.push(MarketAsk::Hello);
            }
        }
    }
    true
}

/// A typed character (or Enter, Escape, Backspace) into `t`; `digits`:
/// the gold box. Returns false when the typing ends (Enter or Escape).
pub fn type_into(t: &mut String, unit: u16, max: usize) -> bool {
    match unit {
        0x0D | 0x1B => return false,
        0x08 => {
            t.pop();
        }
        u => {
            if let Some(c) = char::from_u32(u32::from(u)).filter(|c| !c.is_control()) {
                if t.len() + c.len_utf8() <= max {
                    t.push(c);
                }
            }
        }
    }
    true
}

/// What the form does with an event while shown: true when it takes it.
/// `esc`: the Esc key's action.
pub fn event(ui: &mut MarketUi, p: &FormPlaces, e: UiEvent, esc: bool) -> bool {
    match e {
        UiEvent::Press { button, at } => {
            let right = button == PointerButton::Right;
            match hit(ui, p, at) {
                Some(s) => {
                    click(ui, s, right);
                    true
                }
                None => {
                    if p.frame().contains(at) {
                        ui.sell.focus = None;
                        ui.sell.draft.picking = false;
                        true
                    } else {
                        false
                    }
                }
            }
        }
        // A drag ends on the slot.
        UiEvent::Release { at, .. } => {
            if p.slot().contains(at) {
                if let Some(it) = ui.sell.held.take() {
                    ui.sell.put(it);
                }
            }
            p.frame().contains(at)
        }
        UiEvent::Char(u) => {
            let f = &mut ui.sell;
            match f.focus {
                Some(SellBox::Note) => {
                    if !type_into(&mut f.note, u, NOTE_MAX) {
                        f.focus = None;
                    }
                }
                Some(SellBox::Gold) => match u {
                    0x0D | 0x1B => f.focus = None,
                    0x08 => {
                        f.draft.gold.pop();
                    }
                    u => {
                        if let Some(c) = char::from_u32(u32::from(u)) {
                            f.draft.type_gold(c);
                        }
                    }
                },
                None => return false,
            }
            true
        }
        UiEvent::Action(_) if esc => {
            let f = &mut ui.sell;
            if f.focus.is_some() {
                f.focus = None;
            } else if f.draft.picking {
                f.draft.picking = false;
            } else if f.held.is_some() {
                f.drop_held();
            } else {
                return false;
            }
            true
        }
        // The game's key bindings wait while a box has the typing.
        UiEvent::Action(_) => ui.sell.focus.is_some(),
        UiEvent::Wheel { at, .. } => p.frame().contains(at),
        UiEvent::CursorMoved(_) | UiEvent::CursorLeft => false,
    }
}

/// The slot item's "Similar active / last sold" line (mock 2), from its
/// price check ([`super::price_graph`]): "asking" while the check is out,
/// nothing when the realm gave none. The whole of it shows on hover.
fn draw_similar(pen: &Pen, ui: &MarketUi, r: Rect, out: &mut dyn UiDrawSink) {
    let p = &ui.prices;
    let text = match super::price_graph::sell_history(ui) {
        Some(h) => super::price_graph::similar_line(ui, h),
        None if p.stash.is_some() && p.stash == ui.sell.item.as_ref().map(|i| i.id) => {
            if !(p.queued || p.req.is_some()) {
                return;
            }
            "Similar items: asking the market...".into()
        }
        None => return,
    };
    let text = pen.fit(SMALL, &text, i32::from(r.w));
    pen.text(r.x, r.y + 1, &text, SMALL, TAN, out);
}

/// What a form draw reads beyond the window's pen.
pub struct FormDraw<'a> {
    pub ui: &'a MarketUi,
    pub places: FormPlaces,
    pub pen: Pen<'a>,
    /// Draws an item's picture with its top-left at the point, in cells of
    /// 29 pixels.
    pub picture: &'a dyn Fn(&SellItem, Point, &mut dyn UiDrawSink),
}

/// Draws the form.
pub fn draw(i: &FormDraw, out: &mut dyn UiDrawSink) {
    let (ui, p, pen) = (i.ui, &i.places, &i.pen);
    let f = &ui.sell;
    let k = pen.kit;
    let win = p.frame();
    pen.fill(win, k.panel, out);
    pen.frame(win, k.gold, out);
    pen.text(win.x + 8, win.y + 6, "New listing", LABEL, GOLD, out);
    pen.text_right(
        win.x + FORM_W - 8,
        win.y + 9,
        "Shared stash - Market",
        SMALL,
        GREY,
        out,
    );
    // The Sell slot and what is on it.
    let slot = p.slot();
    pen.fill(slot, k.field, out);
    pen.frame(slot, if f.held.is_some() { k.gold } else { k.edge }, out);
    let tx = slot.x + i32::from(slot.w) + 8;
    let tw = win.x + FORM_W - 8 - tx;
    match &f.item {
        Some(it) => {
            let (w, h) = (29 * i32::from(it.size.0), 29 * i32::from(it.size.1));
            let at = Point::new(
                slot.x + (i32::from(slot.w) - w) / 2,
                slot.y + (i32::from(slot.h) - h) / 2,
            );
            (i.picture)(it, at, out);
            pen.text(
                tx,
                slot.y + 2,
                &pen.fit(LABEL, &it.name, tw),
                LABEL,
                it.group.color(),
                out,
            );
            // Seven lines leave the Price check button its row (the tip
            // shows them all).
            let mut y = slot.y + 18;
            for s in it.stats.iter().take(7) {
                pen.text(tx, y, &pen.fit(SMALL, s, tw), SMALL, BLUE, out);
                y += 11;
            }
            pen.button(p.check(), "Price check", ui.prices.open, out);
            draw_similar(pen, ui, p.similar(), out);
        }
        None => {
            pen.label(slot, "Sell", LABEL, GREY, out);
            let lines = [
                "Drag an item from your shared",
                "stash onto the Sell slot,",
                "or Ctrl+Left it there.",
            ];
            for (n, l) in lines.iter().enumerate() {
                pen.text(tx, slot.y + 8 + 12 * n as i32, l, SMALL, TAN, out);
            }
        }
    }
    let o = p.builder();
    // Beside the slot: the slot's bottom is close above the builder.
    pen.text(tx, o.y - 12, "SELL FOR", SMALL, GREY, out);
    // The rows under the builder.
    let st = ui.state.as_ref();
    let max = st.map_or(7, |s| s.max_days);
    let days = f.days(max);
    for (n, d) in day_choices(max).into_iter().enumerate() {
        pen.button(p.days(n), &format!("{d} d"), d == days, out);
    }
    if let Some(s) = st {
        let left = s.max_listings.saturating_sub(s.active_listings);
        let line = format!("{left} of {} listing slots left", s.max_listings);
        let r = p.days(0);
        pen.text_right(win.x + FORM_W - 8, r.y + 3, &line, SMALL, TAN, out);
    }
    pen.text_box(
        p.note(),
        &f.note,
        "Note for buyers (public, 60 letters)",
        f.focus == Some(SellBox::Note),
        out,
    );
    let h = p.hidden();
    let tick = Rect::new(h.x, h.y, 12, 12);
    pen.fill(tick, k.field, out);
    pen.frame(tick, if f.hidden { k.gold } else { k.edge }, out);
    if f.hidden {
        pen.label(tick, "x", SMALL, GOLD, out);
    }
    pen.text(
        h.x + 16,
        h.y + 2,
        "Post hidden (My shop only, not searched)",
        SMALL,
        TAN,
        out,
    );
    let fees = st.map(|s| s.fees).unwrap_or_default();
    for (n, l) in fee_lines(&fees).iter().enumerate() {
        pen.text(win.x + 8, h.y + 18 + 11 * n as i32, l, SMALL, GOLD, out);
    }
    let post = p.post();
    if f.item.is_some() {
        pen.button(post, "Post listing", f.posting || f.post_req.is_some(), out);
    } else {
        pen.fill(post, k.field, out);
        pen.frame(post, k.edge, out);
        pen.label(post, "Post listing", SMALL, GREY, out);
    }
    let px = post.x + i32::from(post.w) + 8;
    for (n, l) in POST_NOTE.iter().enumerate() {
        let l = pen.fit(SMALL, l, win.x + FORM_W - 8 - px);
        pen.text(px, post.y + 1 + 11 * n as i32, &l, SMALL, GREY, out);
    }
    if let Some((l, refused)) = &f.line {
        let r = p.r(8, 356, FORM_W - 16, 24);
        pen.line(r, l, SMALL, if *refused { RED } else { GOLD }, out);
    }
    pen.button(p.browse(), "Browse the market", false, out);
    pen.button(p.shop(), "My shop", false, out);
    draw_price(
        pen,
        ui,
        &f.draft,
        o,
        FORM_W - 16,
        f.focus == Some(SellBox::Gold),
        out,
    );
    // The item picked up follows the mouse.
    if let Some(it) = &f.held {
        let at = Point::new(pen.mouse.x - 14, pen.mouse.y - 14);
        (i.picture)(it, at, out);
    }
}
