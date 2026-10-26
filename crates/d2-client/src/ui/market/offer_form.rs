// D2RUST's own: the market's offer and counter-offer form.
//! A bundle of gold, currencies and stash items, built in a box over the
//! window: Offer... on a listing that takes offers (paid from the
//! account's shared stash into escrow: only what the stash holds, at most
//! the realm's items per offer), and Counter... on an offer made to the
//! player's listing (any gold and currencies, and which of the offered
//! items to keep; it starts as the offer). The bundle is shown exactly
//! before it goes, with the fee line (none while the realm's fees are 0),
//! and the ask carries the fee revision the player saw.

use std::collections::{BTreeMap, BTreeSet};

use crate::market::source::{Group, ListingId, MarketAsk, Price};
use crate::market::trade::{AnswerHow, NamedItem, OfferId};
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::Rect;
use crate::ui::item_tip::{ItemTips, TipLine};

use super::details::{coin_name, price_text, thousands};
use super::window::{DrawIn, Places, GOLD, GREY, LABEL, RED, SMALL, TAN, WHITE};
use super::{MarketUi, TextBox};

/// Items one offer may name when the realm has not said (contract
/// `max_offer_items`, 10 by default).
pub const OFFER_ITEMS: usize = 10;

/// Rows of the currency and item lists shown at once.
pub const ROWS: usize = 8;

/// The most digits the gold box takes.
const GOLD_DIGITS: usize = 10;

/// What the form builds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormKind {
    /// An offer on a listing.
    Offer {
        listing: ListingId,
        /// The listing's price, `None` for offers only.
        listed: Option<Price>,
    },
    /// A counter to an offer on the player's listing: what it offered and
    /// its items (the ones the counter may keep).
    Counter {
        offer: OfferId,
        offered: Price,
        items: Vec<NamedItem>,
    },
}

/// The offer or counter form's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BundleForm {
    pub kind: FormKind,
    /// The goods' name and group (the title line).
    pub name: String,
    pub group: Group,
    /// The gold typed.
    pub gold: String,
    /// The chosen count of each currency.
    pub stacks: BTreeMap<String, u32>,
    /// The chosen items (stash items for an offer, kept items for a
    /// counter).
    pub items: BTreeSet<u32>,
    /// The first shown row of each list (the wheel scrolls them).
    pub coin_top: usize,
    pub item_top: usize,
    /// The exact bundle is shown for a last look.
    pub review: bool,
    /// The fee revision the player saw.
    pub fee_rev: u32,
    /// Why the bundle cannot go, when it cannot.
    pub error: Option<String>,
}

impl BundleForm {
    /// An empty offer on `listing`.
    pub fn offer(ui: &MarketUi, listing: ListingId) -> Option<BundleForm> {
        let l = ui.selected.as_ref().filter(|l| l.listing == listing)?;
        let fee_rev = ui
            .detail
            .as_ref()
            .map_or_else(|| ui.fees().fee_rev, |d| d.fee_rev);
        Some(BundleForm {
            kind: FormKind::Offer {
                listing,
                listed: l.price.clone(),
            },
            name: l.item.name.clone(),
            group: l.group,
            gold: String::new(),
            stacks: BTreeMap::new(),
            items: BTreeSet::new(),
            coin_top: 0,
            item_top: 0,
            review: false,
            fee_rev,
            error: None,
        })
    }

    /// A counter to `offer`, starting as the offer itself (every item
    /// kept).
    pub fn counter(ui: &MarketUi, o: &crate::market::trade::OfferRow) -> BundleForm {
        BundleForm {
            kind: FormKind::Counter {
                offer: o.offer,
                offered: o.offered.clone(),
                items: o.items.clone(),
            },
            name: o.item.name.clone(),
            group: o.group,
            gold: if o.offered.gold > 0 {
                o.offered.gold.to_string()
            } else {
                String::new()
            },
            stacks: o.offered.stacks.iter().cloned().collect(),
            items: o.offered.items.iter().copied().collect(),
            coin_top: 0,
            item_top: 0,
            review: false,
            fee_rev: ui.fees().fee_rev,
            error: None,
        }
    }

    pub fn is_counter(&self) -> bool {
        matches!(self.kind, FormKind::Counter { .. })
    }

    /// The bundle as it stands.
    pub fn price(&self) -> Price {
        Price {
            gold: self.gold.parse().unwrap_or(0),
            stacks: self
                .stacks
                .iter()
                .filter(|(_, n)| **n > 0)
                .map(|(c, n)| (c.clone(), *n))
                .collect(),
            items: self.items.iter().copied().collect(),
        }
    }

    /// The currency codes the form lists: for an offer, those the stash
    /// holds; for a counter, every currency of the realm, the offered ones
    /// first.
    pub fn coins(&self, ui: &MarketUi) -> Vec<String> {
        let realm: Vec<String> = ui
            .state
            .as_ref()
            .map(|s| s.currencies.clone())
            .unwrap_or_default();
        match &self.kind {
            FormKind::Offer { .. } => {
                let held = ui.holdings.as_ref().map(|h| &h.stacks);
                let have = |c: &String| held.and_then(|h| h.get(c)).copied().unwrap_or(0) > 0;
                let mut v: Vec<String> = realm.iter().filter(|c| have(c)).cloned().collect();
                // A held stackable the realm did not name is still offered.
                if let Some(h) = held {
                    v.extend(
                        h.iter()
                            .filter(|(c, n)| **n > 0 && !realm.contains(c))
                            .map(|(c, _)| c.clone()),
                    );
                }
                v
            }
            FormKind::Counter { offered, .. } => {
                let mut v: Vec<String> = offered.stacks.iter().map(|(c, _)| c.clone()).collect();
                for c in realm {
                    if !v.contains(&c) {
                        v.push(c);
                    }
                }
                v
            }
        }
    }

    /// The items the form lists: the stash's for an offer, the offered
    /// ones for a counter.
    pub fn listed_items<'a>(&'a self, ui: &'a MarketUi) -> Vec<&'a NamedItem> {
        match &self.kind {
            FormKind::Offer { .. } => ui
                .holdings
                .as_ref()
                .map(|h| h.items.iter().collect())
                .unwrap_or_default(),
            FormKind::Counter { items, .. } => items.iter().collect(),
        }
    }

    /// The most of currency `code` the form takes: what the stash holds for
    /// an offer; any count for a counter.
    pub fn most(&self, ui: &MarketUi, code: &str) -> u32 {
        match &self.kind {
            FormKind::Offer { .. } => ui
                .holdings
                .as_ref()
                .and_then(|h| h.stacks.get(code))
                .copied()
                .unwrap_or(0),
            FormKind::Counter { .. } => 999,
        }
    }

    /// The fee shown with the bundle: the buyer fee on an offer, the sale
    /// fee on a counter's gold (both 0 by default).
    pub fn fee(&self, ui: &MarketUi) -> u64 {
        let f = ui.fees();
        match self.kind {
            FormKind::Offer { .. } => f.buyer_fee,
            FormKind::Counter { .. } => self.price().gold * u64::from(f.sale_fee_pct) / 100,
        }
    }

    /// Why the bundle cannot go now, `None` when it can.
    pub fn problem(&self, ui: &MarketUi) -> Option<String> {
        let p = self.price();
        if p.is_empty() {
            return Some("Put gold, currencies or items in it.".into());
        }
        match &self.kind {
            FormKind::Offer { .. } => {
                let max = ui.max_offer_items();
                if p.items.len() > max {
                    return Some(format!("An offer names at most {max} items."));
                }
                let Some(h) = &ui.holdings else {
                    return Some("Your shared stash is not known yet.".into());
                };
                let mut need = p.clone();
                need.gold += self.fee(ui);
                if !h.holds(&need) {
                    return Some("Your shared stash doesn't hold that.".into());
                }
            }
            FormKind::Counter { offered, .. } => {
                if p == *offered {
                    return Some("That is the offer itself: accept it instead.".into());
                }
            }
        }
        None
    }

    /// The ask the form sends.
    pub fn ask(&self) -> MarketAsk {
        match &self.kind {
            FormKind::Offer { listing, .. } => MarketAsk::Offer {
                listing: *listing,
                price: self.price(),
                fee_rev: self.fee_rev,
            },
            FormKind::Counter { offer, .. } => MarketAsk::Answer {
                offer: *offer,
                how: AnswerHow::Counter,
                counter: Some(self.price()),
                fee_rev: self.fee_rev,
            },
        }
    }

    /// One more (`up`) or one less of the listed currency `i`.
    pub fn step(&mut self, ui: &MarketUi, i: usize, up: bool) {
        let Some(code) = self.coins(ui).get(i).cloned() else {
            return;
        };
        let most = self.most(ui, &code);
        let n = self.stacks.entry(code).or_default();
        *n = if up {
            (*n + 1).min(most)
        } else {
            n.saturating_sub(1)
        };
        self.error = None;
    }

    /// Picks or drops the listed item `i`.
    pub fn toggle_item(&mut self, ui: &MarketUi, i: usize) {
        let Some(id) = self.listed_items(ui).get(i).map(|it| it.item_id) else {
            return;
        };
        if !self.items.remove(&id) {
            self.items.insert(id);
        }
        self.error = None;
    }

    /// A key typed into the gold box: digits only.
    pub fn type_gold(&mut self, unit: u16) {
        match unit {
            0x08 => {
                self.gold.pop();
            }
            u => {
                let Some(c) = char::from_u32(u32::from(u)).filter(char::is_ascii_digit) else {
                    return;
                };
                if self.gold.len() < GOLD_DIGITS && !(self.gold.is_empty() && c == '0') {
                    self.gold.push(c);
                }
            }
        }
        self.error = None;
    }
}

impl MarketUi {
    /// Offer... on the shown listing: the form opens, empty.
    pub fn ask_offer(&mut self) {
        if !self.can_offer() {
            return;
        }
        let Some(id) = self.selected.as_ref().map(|l| l.listing) else {
            return;
        };
        self.form = BundleForm::offer(self, id);
        self.confirm = None;
    }

    /// The form's main button: first the last look, then the ask.
    pub fn form_send(&mut self) {
        let Some(f) = self.form.as_ref() else { return };
        if let Some(why) = f.problem(self) {
            if let Some(f) = self.form.as_mut() {
                f.error = Some(why);
                f.review = false;
            }
            return;
        }
        if !f.review {
            if let Some(f) = self.form.as_mut() {
                f.review = true;
            }
            return;
        }
        let Some(f) = self.form.take() else { return };
        let ask = f.ask();
        self.note = Some(if f.is_counter() {
            format!("Countering the offer on {}...", f.name)
        } else {
            format!("Offering on {}...", f.name)
        });
        self.focus = None;
        self.asks.push(ask);
    }

    /// The form's second button: back from the last look, or away.
    pub fn form_back(&mut self) {
        match self.form.as_mut() {
            Some(f) if f.review => f.review = false,
            _ => {
                self.form = None;
                if self.focus == Some(TextBox::Gold) {
                    self.focus = None;
                }
            }
        }
    }

    /// The items one offer may name.
    pub fn max_offer_items(&self) -> usize {
        match self.state.as_ref().map(|s| s.max_offer_items) {
            Some(n) if n > 0 => usize::from(n),
            _ => OFFER_ITEMS,
        }
    }
}

/// The form's box and its parts.
pub struct FormPlaces {
    pub b: Rect,
}

/// The box's size.
const BW: i32 = 380;
const BH: i32 = 300;

impl FormPlaces {
    pub fn of(p: &Places) -> FormPlaces {
        FormPlaces {
            b: p.r((p.w - BW) / 2, (p.h - BH) / 2, BW, BH),
        }
    }

    fn r(&self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect::new(self.b.x + x, self.b.y + y, w as u16, h as u16)
    }

    pub fn gold(&self) -> Rect {
        self.r(48, 54, 96, 14)
    }

    pub fn coin_row(&self, k: usize) -> Rect {
        self.r(10, 88 + 13 * k as i32, 180, 12)
    }

    pub fn coins(&self) -> Rect {
        self.r(10, 88, 180, 13 * ROWS as i32)
    }

    pub fn less(&self, k: usize) -> Rect {
        let r = self.coin_row(k);
        Rect::new(r.x + 112, r.y, 14, 12)
    }

    pub fn more(&self, k: usize) -> Rect {
        let r = self.coin_row(k);
        Rect::new(r.x + 154, r.y, 14, 12)
    }

    pub fn item_row(&self, k: usize) -> Rect {
        self.r(198, 88 + 13 * k as i32, 172, 12)
    }

    pub fn items(&self) -> Rect {
        self.r(198, 88, 172, 13 * ROWS as i32)
    }

    pub fn send(&self) -> Rect {
        self.r(40, BH - 30, 130, 20)
    }

    pub fn back(&self) -> Rect {
        self.r(BW - 170, BH - 30, 130, 20)
    }
}

/// The form's clickable parts, `(spot, rect)`: index of the listed
/// currency or item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormSpot {
    Gold,
    Less(usize),
    More(usize),
    Item(usize),
    Send,
    Back,
}

impl FormSpot {
    pub fn id(self) -> String {
        match self {
            FormSpot::Gold => "market.form.gold".into(),
            FormSpot::Less(i) => format!("market.form.less({i})"),
            FormSpot::More(i) => format!("market.form.more({i})"),
            FormSpot::Item(i) => format!("market.form.item({i})"),
            FormSpot::Send => "market.form.send".into(),
            FormSpot::Back => "market.form.back".into(),
        }
    }
}

/// The form's parts shown now.
pub fn spots(ui: &MarketUi, f: &BundleForm, p: &Places) -> Vec<(FormSpot, Rect)> {
    let fp = FormPlaces::of(p);
    let mut v = vec![(FormSpot::Send, fp.send()), (FormSpot::Back, fp.back())];
    if f.review {
        return v;
    }
    v.push((FormSpot::Gold, fp.gold()));
    let coins = f.coins(ui).len();
    for (k, i) in (f.coin_top..coins).take(ROWS).enumerate() {
        v.push((FormSpot::Less(i), fp.less(k)));
        v.push((FormSpot::More(i), fp.more(k)));
    }
    let items = f.listed_items(ui).len();
    for (k, i) in (f.item_top..items).take(ROWS).enumerate() {
        v.push((FormSpot::Item(i), fp.item_row(k)));
    }
    v
}

/// A click on a part of the form.
pub fn click(ui: &mut MarketUi, s: FormSpot) {
    if s != FormSpot::Gold && ui.focus == Some(TextBox::Gold) {
        ui.focus = None;
    }
    let Some(mut f) = ui.form.take() else { return };
    match s {
        FormSpot::Gold => ui.focus = Some(TextBox::Gold),
        FormSpot::Less(i) => f.step(ui, i, false),
        FormSpot::More(i) => f.step(ui, i, true),
        FormSpot::Item(i) => f.toggle_item(ui, i),
        FormSpot::Send | FormSpot::Back => {}
    }
    ui.form = Some(f);
    match s {
        FormSpot::Send => ui.form_send(),
        FormSpot::Back => ui.form_back(),
        _ => {}
    }
}

/// The wheel over the form's lists.
pub fn wheel(ui: &mut MarketUi, p: &Places, at: crate::ui::geom::Point, steps: i32) {
    let fp = FormPlaces::of(p);
    let Some(mut f) = ui.form.take() else { return };
    let scroll = |top: usize, n: usize| -> usize {
        (top as i64 - i64::from(steps)).clamp(0, n.saturating_sub(ROWS) as i64) as usize
    };
    if fp.coins().contains(at) {
        f.coin_top = scroll(f.coin_top, f.coins(ui).len());
    } else if fp.items().contains(at) {
        f.item_top = scroll(f.item_top, f.listed_items(ui).len());
    }
    ui.form = Some(f);
}

/// A price's parts, one per line: gold, each currency, each item by name.
pub fn price_parts(ui: &MarketUi, p: &Price, items: &[&NamedItem]) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    if p.gold > 0 {
        out.push((format!("{} gold", thousands(p.gold)), GOLD));
    }
    for (c, n) in &p.stacks {
        out.push((format!("{n} {}", coin_name(ui, c)), 8));
    }
    for id in &p.items {
        match items.iter().find(|i| i.item_id == *id) {
            Some(i) => out.push((i.item.name.clone(), i.group.color())),
            None => out.push((format!("item {id}"), WHITE)),
        }
    }
    out
}

/// A price with its items named: "1 Rune 24 + Sapphire Jewel".
pub fn bundle_text(ui: &MarketUi, p: &Price, items: &[&NamedItem]) -> String {
    let parts = price_parts(ui, p, items);
    if parts.is_empty() {
        "nothing".into()
    } else {
        parts
            .into_iter()
            .map(|(s, _)| s)
            .collect::<Vec<_>>()
            .join(" + ")
    }
}

/// The tip of a named item: the game's from its record, else its own
/// lines.
pub fn item_tip(it: &NamedItem, tips: Option<&ItemTips>) -> Vec<TipLine> {
    let game = match tips {
        Some(t) => super::tip::record_lines(t, &it.record),
        None => Vec::new(),
    };
    if game.is_empty() {
        super::tip::view_lines(it.group, &it.item, 1)
    } else {
        game
    }
}

/// The listed item at `mouse`, with its row.
pub fn hovered<'a>(
    ui: &'a MarketUi,
    p: &Places,
    mouse: crate::ui::geom::Point,
) -> Option<(&'a NamedItem, Rect)> {
    let f = ui.form.as_ref()?;
    if f.review {
        return None;
    }
    let fp = FormPlaces::of(p);
    let items = f.listed_items(ui);
    (f.item_top..items.len())
        .take(ROWS)
        .enumerate()
        .map(|(k, i)| (items[i], fp.item_row(k)))
        .find(|(_, r)| r.contains(mouse))
}

/// The tip of the form's item under the mouse.
pub fn hover_tip(i: &DrawIn) -> Option<(Vec<TipLine>, Rect)> {
    let (it, r) = hovered(i.ui, &i.places, i.pen.mouse)?;
    Some((item_tip(it, i.tips), r))
}

/// Draws the form, when open.
pub fn draw(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, pen) = (i.ui, &i.pen);
    let Some(f) = &ui.form else { return };
    let fp = FormPlaces::of(&i.places);
    let b = fp.b;
    let k = pen.kit;
    pen.fill(b, k.panel, out);
    pen.frame(b, k.gold, out);
    pen.frame(Rect::new(b.x + 2, b.y + 2, b.w - 4, b.h - 4), k.edge, out);
    let title = if f.is_counter() {
        "Counter-offer"
    } else {
        "Make an offer"
    };
    pen.label(Rect::new(b.x, b.y + 6, b.w, 14), title, LABEL, GOLD, out);
    let (x, w) = (b.x + 10, i32::from(b.w) - 20);
    let line = |y: i32, s: &str, color: u16, out: &mut dyn UiDrawSink| {
        pen.text(x, b.y + y, &pen.fit(SMALL, s, w), SMALL, color, out);
    };
    line(24, &f.name, f.group.color(), out);
    let sub = match &f.kind {
        FormKind::Offer { listed, .. } => match listed {
            Some(p) => format!("Listed at {}", price_text(ui, p)),
            None => "Offers only".into(),
        },
        FormKind::Counter { offered, items, .. } => {
            let named: Vec<&NamedItem> = items.iter().collect();
            format!("They offer: {}", bundle_text(ui, offered, &named))
        }
    };
    line(36, &sub, TAN, out);
    let p = f.price();
    let items = f.listed_items(ui);
    if f.review {
        draw_review(i, f, &fp, &items, out);
        return;
    }
    pen.text(x, b.y + 57, "Gold", SMALL, TAN, out);
    pen.text_box(
        fp.gold(),
        &f.gold,
        "0",
        ui.focus == Some(TextBox::Gold),
        out,
    );
    if let (FormKind::Offer { .. }, Some(h)) = (&f.kind, &ui.holdings) {
        let have = format!("You have {} gold", thousands(h.gold));
        pen.text(b.x + 152, b.y + 57, &have, SMALL, GREY, out);
    }
    let heads = if f.is_counter() {
        ("ASK FOR", "KEEP FROM THE OFFER")
    } else {
        ("CURRENCIES", "ITEMS FROM YOUR STASH")
    };
    pen.text(x, b.y + 76, heads.0, SMALL, GREY, out);
    pen.text(b.x + 198, b.y + 76, heads.1, SMALL, GREY, out);
    let coins = f.coins(ui);
    if coins.is_empty() {
        pen.text(x, b.y + 90, "None in your stash.", SMALL, GREY, out);
    }
    for (row, c) in coins.iter().enumerate().skip(f.coin_top).take(ROWS) {
        let k = row - f.coin_top;
        let r = fp.coin_row(k);
        let n = f.stacks.get(c).copied().unwrap_or(0);
        let name = pen.fit(SMALL, coin_name(ui, c), 104);
        pen.text(
            r.x + 2,
            r.y + 2,
            &name,
            SMALL,
            if n > 0 { 8 } else { WHITE },
            out,
        );
        pen.button(fp.less(k), "-", false, out);
        pen.label(
            Rect::new(r.x + 126, r.y, 28, 12),
            &n.to_string(),
            SMALL,
            if n > 0 { GOLD } else { TAN },
            out,
        );
        pen.button(fp.more(k), "+", false, out);
        if !f.is_counter() {
            let of = format!("/{}", f.most(ui, c));
            pen.text(r.x + 170, r.y + 2, &of, SMALL, GREY, out);
        }
    }
    if items.is_empty() {
        let s = if f.is_counter() {
            "The offer names no items."
        } else {
            "No items in your stash."
        };
        pen.text(b.x + 198, b.y + 90, s, SMALL, GREY, out);
    }
    for (row, it) in items.iter().enumerate().skip(f.item_top).take(ROWS) {
        let r = fp.item_row(row - f.item_top);
        let on = f.items.contains(&it.item_id);
        if r.contains(pen.mouse) {
            pen.fill(r, k.hit, out);
        }
        pen.text(
            r.x + 2,
            r.y + 2,
            if on { "[x]" } else { "[ ]" },
            SMALL,
            GOLD,
            out,
        );
        let name = pen.fit(SMALL, &it.item.name, i32::from(r.w) - 24);
        pen.text(r.x + 20, r.y + 2, &name, SMALL, it.group.color(), out);
    }
    let named: Vec<&NamedItem> = items.clone();
    let what = if f.is_counter() { "Counter" } else { "Offer" };
    line(
        198,
        &format!("{what}: {}", bundle_text(ui, &p, &named)),
        GOLD,
        out,
    );
    let fee = f.fee(ui);
    if fee > 0 {
        line(
            210,
            &format!("Market fee: {} gold", thousands(fee)),
            TAN,
            out,
        );
    }
    if let Some(e) = &f.error {
        line(224, e, RED, out);
    } else if !f.is_counter() {
        let max = ui.max_offer_items();
        line(
            224,
            &format!("Up to {max} items. Paid from your shared stash into escrow."),
            GREY,
            out,
        );
    } else {
        line(
            224,
            "A counter is final: the buyer accepts it or withdraws.",
            GREY,
            out,
        );
    }
    pen.button(
        fp.send(),
        if f.is_counter() {
            "Counter..."
        } else {
            "Offer..."
        },
        false,
        out,
    );
    pen.button(fp.back(), "Cancel", false, out);
}

fn draw_review(
    i: &DrawIn,
    f: &BundleForm,
    fp: &FormPlaces,
    items: &[&NamedItem],
    out: &mut dyn UiDrawSink,
) {
    let (ui, pen) = (i.ui, &i.pen);
    let b = fp.b;
    let (x, w) = (b.x + 16, i32::from(b.w) - 32);
    let mut y = b.y + 56;
    let line = |y: i32, s: &str, color: u16, out: &mut dyn UiDrawSink| {
        pen.text(x, y, &pen.fit(SMALL, s, w), SMALL, color, out);
    };
    let p = f.price();
    line(
        y,
        if f.is_counter() {
            "You ask for:"
        } else {
            "You offer:"
        },
        TAN,
        out,
    );
    y += 12;
    for (s, c) in price_parts(ui, &p, items) {
        line(y, &format!("  {s}"), c, out);
        y += 11;
    }
    let fee = f.fee(ui);
    if fee > 0 {
        line(y, &format!("Market fee: {} gold", thousands(fee)), TAN, out);
        y += 12;
    }
    y += 6;
    let note = if f.is_counter() {
        [
            "The buyer accepts it or withdraws; offered items",
            "you don't keep go back to them.",
        ]
    } else {
        [
            "It waits in escrow until the seller answers.",
            "Declined or withdrawn, it comes back to Deliveries.",
        ]
    };
    for (k, s) in note.into_iter().enumerate() {
        line(y + 11 * k as i32, s, GREY, out);
    }
    pen.button(
        fp.send(),
        if f.is_counter() {
            "Send counter"
        } else {
            "Make offer"
        },
        false,
        out,
    );
    pen.button(fp.back(), "Back", false, out);
}
