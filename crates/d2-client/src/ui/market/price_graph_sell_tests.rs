//! The price check in the New listing form: the Sell slot's item checked
//! as it lands, the "Similar active / last sold" line, and the box over
//! the form.

use super::*;
use crate::market::fake::FakeSource;
use crate::market::source::{Group, MarketAsk};
use crate::ui::draw::UiDraw;
use crate::ui::geom::Point;
use crate::ui::layout::Screen;
use crate::ui::market::sell::{self, FormDraw, FormPlaces, SellItem};
use crate::ui::market::window::Places;
use crate::ui::market::Availability;
use crate::ui::shared_stash::Kit;

const KIT: Kit = Kit {
    panel: 1,
    edge: 2,
    gold: 3,
    on: 4,
    field: 5,
    hit: 6,
};

/// Online over the fake market, which has answered; the window closed.
fn online() -> (MarketUi, FakeSource) {
    let mut ui = MarketUi::new();
    let mut src = FakeSource::new(&|_| None, false, 0, true);
    ui.set_online(true);
    ui.pump(&mut src);
    assert_eq!(ui.availability(), Availability::Open);
    (ui, src)
}

fn item(id: u32) -> SellItem {
    SellItem {
        id,
        name: "The Gnasher".into(),
        code: "hax".into(),
        group: Group::Unique,
        size: (1, 3),
        look: None,
        level_req: 5,
        sockets: 0,
        ethereal: false,
        stats: vec!["+70% Enhanced Damage".into()],
    }
}

fn form() -> FormPlaces {
    FormPlaces::of(&Screen::R800)
}

fn places() -> Places {
    Places::of(&Screen::R800)
}

fn centre(r: Rect) -> Point {
    Point::new(r.x + i32::from(r.w) / 2, r.y + i32::from(r.h) / 2)
}

fn left(at: Point) -> UiEvent {
    UiEvent::Press {
        button: PointerButton::Left,
        at,
    }
}

/// The price checks the fake was asked, in order.
fn checks(src: &FakeSource) -> Vec<PriceOf> {
    src.asks
        .iter()
        .filter_map(|(_, a)| match a {
            MarketAsk::Prices { of, .. } => Some(of.clone()),
            _ => None,
        })
        .collect()
}

/// The texts of the form's draw.
fn form_texts(ui: &MarketUi) -> Vec<String> {
    let picture = |_: &SellItem, _: Point, _: &mut dyn UiDrawSink| {};
    let i = FormDraw {
        ui,
        places: form(),
        pen: Pen {
            kit: KIT,
            fonts: None,
            mouse: Point::new(0, 0),
        },
        picture: &picture,
    };
    let mut out = Vec::new();
    sell::draw(&i, &mut out);
    out.iter()
        .filter_map(|d| match d {
            UiDraw::Text(t) => Some(String::from_utf16_lossy(&t.text)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_sell_slot_item_gets_its_similar_line() {
    let (mut ui, mut src) = online();
    ui.sell.put(item(7));
    // The pump tells the fake what the slot shows, then checks it.
    ui.pump(&mut src);
    assert_eq!(checks(&src), [PriceOf::StashItem(7)]);
    assert_eq!(ui.prices.stash, Some(7));
    assert!(!ui.prices.open, "the line alone: no box");
    let h = sell_history(&ui).expect("the answer");
    assert_eq!(h.key, "u:The Gnasher");
    assert!(h.enough(), "the fake makes up sales for a new key");
    let line = similar_line(&ui, h);
    assert!(line.starts_with("Similar active: 0 listings"), "{line}");
    assert!(line.contains("last sold: "), "{line}");
    // Drawn under the price (cut to the form's width when long).
    let shown = form_texts(&ui);
    assert!(
        shown
            .iter()
            .any(|t| t.starts_with("Similar active: 0 listings")),
        "{shown:?}"
    );
    // The tip says it all: the span's sales and the last ones.
    let tip = similar_tip(&ui, h);
    assert_eq!(tip[0], "Similar active: 0 listings");
    assert!(tip
        .iter()
        .any(|l| l.starts_with("Sold in the last 30 days: ")));
    assert!(tip
        .iter()
        .any(|l| l.starts_with("Sold ") && l.ends_with(" ago")));
    assert!(tip.iter().all(|l| !l.contains('·')), "{tip:?}");
    // Asked once: the next frames leave it be.
    ui.pump(&mut src);
    ui.pump(&mut src);
    assert_eq!(checks(&src).len(), 1);
    // Another item on the slot is checked in its turn; the old line goes.
    let mut other = item(8);
    other.name = "Ashen Crown".into();
    ui.sell.put(other);
    assert!(sell_history(&ui).is_none());
    ui.pump(&mut src);
    assert_eq!(checks(&src).last(), Some(&PriceOf::StashItem(8)));
    assert_eq!(
        sell_history(&ui).map(|h| h.key.as_str()),
        Some("u:Ashen Crown")
    );
    // An empty slot: no line, nothing asked.
    ui.sell.clear();
    ui.pump(&mut src);
    assert_eq!(checks(&src).len(), 2);
    assert!(sell_history(&ui).is_none());
    assert!(!form_texts(&ui).iter().any(|t| t.starts_with("Similar")));
}

#[test]
fn the_sell_form_price_check_opens_the_box_over_the_form() {
    let (mut ui, mut src) = online();
    ui.sell.put(item(7));
    ui.pump(&mut src);
    let p = form();
    assert!(sell::spots(&ui, &p)
        .iter()
        .any(|(s, r)| *s == sell::SellSpot::PriceCheck && *r == p.check()));
    assert!(form_texts(&ui).iter().any(|t| t == "Price check"));
    // Price check: the box, from the answer the line has (nothing asked).
    assert!(sell::event(&mut ui, &p, left(centre(p.check())), false));
    assert!(ui.prices.open);
    assert_eq!(ui.prices.title, "The Gnasher");
    assert_eq!(checks(&src).len(), 1);
    // The box takes the presses: a span chip asks again by the key.
    let w = places();
    let seven = days_rect(&w, DAYS.iter().position(|d| *d == 7).unwrap());
    assert!(form_event(&mut ui, &w, left(centre(seven)), false));
    ui.pump(&mut src);
    assert_eq!(
        checks(&src).last(),
        Some(&PriceOf::Key("u:The Gnasher".into()))
    );
    assert_eq!(sell_history(&ui).map(|h| h.days), Some(7));
    // A press outside it is taken too; the form under it is left alone.
    assert!(form_event(&mut ui, &w, left(centre(p.post())), false));
    assert!(ui.sell.item.is_some() && ui.sell.post_req.is_none());
    // Esc shuts it; the line keeps its answer and nothing is asked again.
    assert!(form_event(
        &mut ui,
        &w,
        UiEvent::Action(crate::ui::ActionId(0)),
        true
    ));
    assert!(!ui.prices.open);
    ui.pump(&mut src);
    assert_eq!(checks(&src).len(), 2);
    assert!(sell_history(&ui).is_some());
    // Close shuts it too.
    assert!(sell::event(&mut ui, &p, left(centre(p.check())), false));
    assert!(ui.prices.open);
    assert!(form_event(&mut ui, &w, left(centre(close_rect(&w))), false));
    assert!(!ui.prices.open);
}

#[test]
fn the_window_check_gives_the_line_back_when_it_closes() {
    let (mut ui, mut src) = online();
    ui.sell.put(item(7));
    ui.pump(&mut src);
    // The market window opens over the form: its check is its own (and the
    // form's box does not follow it there).
    ui.prices.open = true;
    ui.toggle().unwrap();
    assert!(!ui.prices.open);
    ui.pump(&mut src);
    ui.select(0);
    ui.pump(&mut src);
    ui.check_selected();
    ui.pump(&mut src);
    assert!(matches!(checks(&src).last(), Some(PriceOf::Listing(_))));
    assert_eq!(ui.prices.stash, None);
    assert!(sell_history(&ui).is_none());
    // Shut: the next frame checks the slot's item again.
    ui.close();
    ui.pump(&mut src);
    assert_eq!(checks(&src).last(), Some(&PriceOf::StashItem(7)));
    assert!(sell_history(&ui).is_some());
}

#[test]
fn a_stash_item_the_realm_does_not_know_is_asked_once() {
    let (mut ui, mut src) = online();
    ui.sell.put(item(9));
    // The realm never heard of it (the fake was not told).
    ui.sell.seen.clear();
    ui.pump(&mut src);
    assert_eq!(checks(&src), [PriceOf::StashItem(9)]);
    assert!(ui.prices.error.is_some());
    ui.pump(&mut src);
    ui.pump(&mut src);
    assert_eq!(checks(&src).len(), 1);
    // No line: the form says nothing rather than something wrong.
    assert!(sell_history(&ui).is_none());
    assert!(!form_texts(&ui).iter().any(|t| t.starts_with("Similar")));
}
