// D2RUST's own: the market browser's details column and boxes.
//! The right column of the browser: the picked listing's tip (the game's
//! from its record, [`super::tip`]), its comparison with the item the
//! player wears in its slot, the seller and the price, exactly, Price
//! check ([`super::price_graph`]) and Buy / Offer...; and the boxes drawn
//! over the window: the buy confirm (the price, the realm's buyer fee when
//! there is one, what is paid), the offer form ([`super::offer_form`]) and
//! an answer's confirm.

use crate::market::source::{Group, ListingRow, Price};
use crate::ui::draw::UiDrawSink;
use crate::ui::geom::Rect;
use crate::ui::item_tip::TipLine;
use crate::ui::panels::inventory::body_loc;

use super::window::{DrawIn, Pen, GOLD, GREY, LABEL, SMALL, TAN, WHITE};
use super::MarketUi;

/// The equipped item in a listing's slot, as the panel finds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Compare {
    /// The slot's word ("helm", "ring").
    pub slot: String,
    /// The equipped item's tip.
    pub lines: Vec<TipLine>,
}

/// The body location a listing's item type is worn at, and the slot's
/// word, for the comparison.
pub fn slot_of(item_type: &str) -> Option<(u8, &'static str)> {
    Some(match item_type {
        "helm" | "circ" | "pelt" | "phlm" => (body_loc::HEAD, "helm"),
        "tors" => (body_loc::TORSO, "armor"),
        "shie" | "shld" | "ashd" | "head" => (body_loc::LEFT_HAND, "shield"),
        "glov" => (body_loc::GLOVES, "gloves"),
        "boot" => (body_loc::FEET, "boots"),
        "belt" => (body_loc::BELT, "belt"),
        "amul" => (body_loc::NECK, "amulet"),
        "ring" => (body_loc::RIGHT_RING, "ring"),
        "swor" | "axe" | "mace" | "club" | "hamm" | "scep" | "knif" | "spea" | "pole" | "staf"
        | "wand" | "orb" | "bow" | "xbow" | "jave" | "h2h" | "h2h2" | "weap" | "mele" | "miss"
        | "blun" => (body_loc::RIGHT_HAND, "weapon"),
        _ => return None,
    })
}

/// The name of a currency by its stackable code: the game's when known,
/// else the code.
pub fn coin_name<'a>(ui: &'a MarketUi, code: &'a str) -> &'a str {
    ui.coin_names.get(code).map_or(code, String::as_str)
}

/// A price, exactly: "120,000 gold + 2 Rune 24 + 1 Rune 26".
pub fn price_text(ui: &MarketUi, p: &Price) -> String {
    let mut parts = Vec::new();
    if p.gold > 0 {
        parts.push(format!("{} gold", thousands(p.gold)));
    }
    for (code, n) in &p.stacks {
        parts.push(format!("{n} {}", coin_name(ui, code)));
    }
    if !p.items.is_empty() {
        parts.push(format!("{} item(s)", p.items.len()));
    }
    if parts.is_empty() {
        "nothing".into()
    } else {
        parts.join(" + ")
    }
}

/// A listing's price, or how it sells without one.
pub fn listing_price(ui: &MarketUi, l: &ListingRow) -> String {
    listing_price_of(ui, l.price.as_ref())
}

/// A listing price (`None`: offers only).
pub fn listing_price_of(ui: &MarketUi, p: Option<&Price>) -> String {
    match p {
        Some(p) => price_text(ui, p),
        None => "Offers only".into(),
    }
}

/// `n` with thousands separated: 1,234,567.
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// How long ago a listing was posted: "5m", "3h", "2d".
pub fn age(now_ms: u64, posted_ms: u64) -> String {
    let s = now_ms.saturating_sub(posted_ms) / 1000;
    match s {
        0..60 => "now".into(),
        60..3_600 => format!("{}m", s / 60),
        3_600..86_400 => format!("{}h", s / 3_600),
        _ => format!("{}d", s / 86_400),
    }
}

/// Lines of `lines` fitted into `w`, drawn down from `y`; returns the
/// next free row.
fn tip_block(
    pen: &Pen,
    x: i32,
    mut y: i32,
    w: i32,
    lines: &[TipLine],
    bottom: i32,
    out: &mut dyn UiDrawSink,
) -> i32 {
    for l in lines {
        if y + 10 > bottom {
            break;
        }
        let s = String::from_utf16_lossy(&l.text);
        let s = pen.fit(SMALL, &s, w);
        pen.text(x, y, &s, SMALL, l.color, out);
        y += 10;
    }
    y
}

/// Draws the details column.
pub fn draw(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen, k) = (i.ui, &i.places, &i.pen, i.pen.kit);
    let r = p.details();
    pen.fill(r, k.field, out);
    pen.frame(r, k.edge, out);
    let (x, w) = (r.x + 6, i32::from(r.w) - 12);
    let bottom = super::price_graph::button(p).y - 6;
    let Some(l) = &ui.selected else {
        pen.text(
            x,
            r.y + 8,
            "Pick a listing to see it here.",
            SMALL,
            GREY,
            out,
        );
        return;
    };
    let mut y = r.y + 6;
    let name = pen.fit(LABEL, &l.item.name, w);
    pen.text(x, y, &name, LABEL, l.group.color(), out);
    y += 14;
    let lines = super::tip::listing_lines(l, i.tips);
    // The name is the tip's first line; it is drawn above.
    let body = if lines.len() > 1 {
        &lines[1..]
    } else {
        &[][..]
    };
    y = tip_block(pen, x, y, w, body, bottom - 70, out);
    if let Some(c) = i.compare {
        y += 4;
        pen.text(
            x,
            y,
            &format!("vs. your {} (equipped)", c.slot),
            SMALL,
            GREY,
            out,
        );
        y += 10;
        y = tip_block(pen, x, y, w, &c.lines, bottom - 44, out);
    }
    y = y.max(bottom - 40);
    let sales = if l.seller_sales > 0 {
        format!(" ({} sold)", l.seller_sales)
    } else {
        String::new()
    };
    let now = ui.results.as_ref().map_or(0, |r| r.now_ms);
    let seller = format!("{}{sales} · {}", l.seller, age(now, l.posted_ms));
    pen.text(x, y, &pen.fit(SMALL, &seller, w), SMALL, TAN, out);
    y += 10;
    if !l.note.is_empty() {
        let note = format!("\"{}\"", l.note);
        pen.text(x, y, &pen.fit(SMALL, &note, w), SMALL, GREY, out);
    }
    y += 10;
    let price = format!("Price: {}", listing_price(ui, l));
    pen.text(x, y, &pen.fit(SMALL, &price, w), SMALL, GOLD, out);
    let check = super::price_graph::button(p);
    pen.button(check, "Price check", ui.prices.open, out);
    if ui.can_buy() {
        let s = format!("Buy · {}", listing_price(ui, l));
        pen.button(p.buy(), &s, false, out);
    } else if l.mine {
        pen.label(p.buy(), "Your listing", SMALL, GREY, out);
    }
    if ui.can_offer() {
        let s = if l.offers > 0 {
            format!("Offer... ({} made)", l.offers)
        } else {
            "Offer...".into()
        };
        pen.button(p.offer(), &s, false, out);
    }
}

/// A box over the window: its back, frame and title.
fn boxed(pen: &Pen, r: Rect, title: &str, out: &mut dyn UiDrawSink) {
    let k = pen.kit;
    pen.fill(r, k.panel, out);
    pen.frame(r, k.gold, out);
    pen.frame(Rect::new(r.x + 2, r.y + 2, r.w - 4, r.h - 4), k.edge, out);
    pen.label(Rect::new(r.x, r.y + 6, r.w, 14), title, LABEL, GOLD, out);
}

/// Draws the buy confirm, the offer form or an answer's confirm, when
/// open.
pub fn draw_boxes(i: &DrawIn, out: &mut dyn UiDrawSink) {
    let (ui, p, pen) = (i.ui, &i.places, &i.pen);
    if let Some(b) = &ui.confirm {
        let r = p.confirm_box();
        boxed(pen, r, "Buy this item?", out);
        let (x, w) = (r.x + 16, i32::from(r.w) - 32);
        let mut y = r.y + 28;
        let line = |y: i32, s: &str, color: u16, out: &mut dyn UiDrawSink| {
            pen.text(x, y, &pen.fit(SMALL, s, w), SMALL, color, out);
        };
        line(y, &b.name, WHITE, out);
        y += 14;
        line(y, &format!("Price: {}", price_text(ui, &b.price)), TAN, out);
        y += 12;
        let mut paid = b.price.clone();
        if b.buyer_fee > 0 {
            let fee = format!("Market fee: {} gold", thousands(b.buyer_fee));
            line(y, &fee, TAN, out);
            y += 12;
            paid.gold += b.buyer_fee;
        }
        line(y, &format!("You pay: {}", price_text(ui, &paid)), GOLD, out);
        y += 14;
        line(y, "It goes to Deliveries, then to your stash.", GREY, out);
        pen.button(p.confirm(), "Buy", false, out);
        pen.button(p.cancel(), "Cancel", false, out);
    } else if ui.form.is_some() {
        super::offer_form::draw(i, out);
    } else if ui.offers.deal.is_some() {
        super::offers::draw_deal(i, out);
    }
}

/// A group's header word in the grid.
pub fn group_title(g: Group) -> &'static str {
    match g {
        Group::Unique => "UNIQUE",
        Group::Set => "SET",
        Group::Runeword => "RUNEWORD",
        Group::Rare => "RARE",
        Group::Crafted => "CRAFTED",
        Group::Magic => "MAGIC",
        Group::Other => "NORMAL",
        Group::Stackable => "RUNES AND STACKS",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_read_exactly() {
        let mut ui = MarketUi::new();
        ui.coin_names.insert("r24".into(), "Rune 24".into());
        let p = Price {
            gold: 1_234_567,
            stacks: vec![("r24".into(), 2), ("r99".into(), 1)],
            items: Vec::new(),
        };
        assert_eq!(price_text(&ui, &p), "1,234,567 gold + 2 Rune 24 + 1 r99");
        assert_eq!(price_text(&ui, &Price::gold(900)), "900 gold");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(age(10 * 86_400_000, 0), "10d");
        assert_eq!(age(90_000, 0), "1m");
        assert_eq!(slot_of("circ"), Some((body_loc::HEAD, "helm")));
        assert_eq!(slot_of("rune"), None);
    }
}
