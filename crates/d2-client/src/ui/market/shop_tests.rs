//! Selling (the New listing form) and My shop over the fake market.

use super::sell::{self, PriceDraft, PriceSpot, SellItem};
use super::shop::{self, ShopAct, ShopSpot};
use super::window::{self, Places, Spot};
use super::*;
use crate::market::fake::{FakeSource, FEE_REV};
use crate::market::source::{
    Group, ListingMode, ListingState, MarketFees, Post, PostGoods, ShopOp, ShopTab,
};
use crate::ui::geom::Point;
use crate::ui::layout::Screen;
use crate::ui::panel::{PointerButton, UiEvent};

/// The trading side's own listings, in My shop from the start (the
/// fake's offers on them).
const OWN: usize = 3;

fn fake() -> FakeSource {
    let src = FakeSource::new(&|_| None, false, 0, true);
    assert_eq!(src.shop.len(), OWN);
    src
}

/// Online over the fake market, which has answered; the window closed.
fn online() -> (MarketUi, FakeSource) {
    let mut ui = MarketUi::new();
    let mut src = fake();
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

fn form() -> sell::FormPlaces {
    sell::FormPlaces::of(&Screen::R800)
}

fn centre(r: crate::ui::geom::Rect) -> Point {
    Point::new(r.x + i32::from(r.w) / 2, r.y + i32::from(r.h) / 2)
}

fn left(at: Point) -> UiEvent {
    UiEvent::Press {
        button: PointerButton::Left,
        at,
    }
}

/// Types `s` into the focused box of the form.
fn type_form(ui: &mut MarketUi, s: &str) {
    for c in s.chars() {
        assert!(sell::event(ui, &form(), UiEvent::Char(c as u16), false));
    }
}

#[test]
fn sell_slot_takes_stash_item_only() {
    let (mut ui, _src) = online();
    let p = form();
    // An item of the character's on the cursor is refused.
    ui.sell.refuse_cursor();
    assert!(ui.sell.item.is_none());
    assert!(ui
        .sell
        .line
        .as_ref()
        .is_some_and(|(l, refused)| *refused && l.contains("shared stash")));
    // A stash item picked up and dropped on the slot (a drag's release).
    ui.sell.hold(item(7));
    assert_eq!(ui.sell.faded, [7]);
    let release = UiEvent::Release {
        button: PointerButton::Left,
        at: centre(p.slot()),
    };
    assert!(sell::event(&mut ui, &p, release, false));
    assert_eq!(ui.sell.item.as_ref().map(|i| i.id), Some(7));
    assert!(ui.sell.held.is_none() && ui.sell.line.is_none());
    // The market is told what the slot showed.
    assert_eq!(ui.sell.seen.len(), 1);
    assert_eq!(ui.sell.seen[0].item_id, 7);
    // Another picked up and clicked onto the slot replaces it; a click on
    // the slot with nothing held empties it.
    ui.sell.hold(item(8));
    assert!(sell::event(&mut ui, &p, left(centre(p.slot())), false));
    assert_eq!(ui.sell.item.as_ref().map(|i| i.id), Some(8));
    assert_eq!(ui.sell.faded, [8]);
    assert!(sell::event(&mut ui, &p, left(centre(p.slot())), false));
    assert!(ui.sell.item.is_none() && ui.sell.faded.is_empty());
    // Post without an item says why and asks nothing.
    sell::post(&mut ui);
    assert!(ui.take_asks().is_empty());
    assert!(ui.sell.line.as_ref().is_some_and(|(_, r)| *r));
}

#[test]
fn shared_stash_market_press_picks_for_the_slot() {
    use crate::ui::shared_stash::{self as stash, Category, MarketPick, Mods, ShownItem};
    use d2_shared_stash::{Cell, ItemId};
    let mut st = stash::SharedStashUi {
        on: true,
        ..Default::default()
    };
    st.category.0 = Category::Market;
    st.items.push(ShownItem {
        id: ItemId(7),
        cell: Cell {
            page: 0,
            x: 0,
            y: 0,
        },
        size: (1, 3),
        code: "hax".into(),
        name: "The Gnasher".into(),
        quality: 7,
        level_req: 5,
        sockets: 0,
        ethereal: false,
        look: None,
        fillers: Vec::new(),
    });
    assert!(st.selling() && st.covers());
    let places = stash::Places::of(&Screen::R800);
    let at = centre(places.cell(0, 1));
    // A press picks the item up, Ctrl+Left puts it straight on the slot:
    // never a take from the stash.
    assert!(stash::press(
        &mut st,
        &places,
        at,
        Mods::keys(false, false),
        None
    ));
    assert_eq!(
        st.market_pick.take(),
        Some(MarketPick::Item {
            id: ItemId(7),
            ctrl: false
        })
    );
    assert!(stash::press(
        &mut st,
        &places,
        at,
        Mods::keys(true, false),
        None
    ));
    assert_eq!(
        st.market_pick.take(),
        Some(MarketPick::Item {
            id: ItemId(7),
            ctrl: true
        })
    );
    // With the character's item on the cursor: refused, nothing put in.
    assert!(stash::press(
        &mut st,
        &places,
        at,
        Mods::keys(false, false),
        Some(5)
    ));
    assert_eq!(st.market_pick.take(), Some(MarketPick::Cursor));
    let empty = centre(places.cell(5, 5));
    assert!(stash::press(
        &mut st,
        &places,
        empty,
        Mods::keys(false, false),
        None
    ));
    assert_eq!(st.market_pick.take(), Some(MarketPick::Empty));
    assert!(st.take_asks().is_empty());
}

#[test]
fn post_form_modes() {
    let cur: Vec<String> = ["r20", "r23"].map(String::from).to_vec();
    let mut d = PriceDraft::default();
    assert_eq!(d.mode, ListingMode::Price);
    // A priced mode needs gold, currencies or both.
    assert!(d.price().is_err());
    d.gold = "25000".into();
    assert_eq!(d.price().unwrap().map(|p| p.gold), Some(25_000));
    // Currencies: added from the picker, left +1, right -1 (gone at 0).
    sell::click_price(&mut d, &cur, PriceSpot::AddCoin, false);
    assert!(d.picking);
    sell::click_price(&mut d, &cur, PriceSpot::Pick(1), false);
    assert!(!d.picking);
    sell::click_price(&mut d, &cur, PriceSpot::Coin(0), false);
    assert_eq!(d.stacks, [("r23".to_owned(), 2)]);
    let p = d.price().unwrap().unwrap();
    assert_eq!(
        (p.gold, p.stacks.clone()),
        (25_000, vec![("r23".to_owned(), 2)])
    );
    sell::click_price(&mut d, &cur, PriceSpot::Coin(0), true);
    sell::click_price(&mut d, &cur, PriceSpot::Coin(0), true);
    assert!(d.stacks.is_empty());
    // Offers only sends no price; the builder hides gold and currencies.
    sell::click_price(
        &mut d,
        &cur,
        PriceSpot::Mode(ListingMode::OffersOnly),
        false,
    );
    assert_eq!(d.price(), Ok(None));
    let spots = sell::price_spots(&d, &cur, Point::new(0, 0), 296);
    assert!(spots.iter().all(|(s, _)| matches!(s, PriceSpot::Mode(_))));
    // Price + offers: a price like a fixed one.
    sell::click_price(
        &mut d,
        &cur,
        PriceSpot::Mode(ListingMode::PriceAndOffers),
        false,
    );
    assert_eq!(d.price().unwrap().map(|p| p.gold), Some(25_000));
    // The days offered: up to the realm's longest.
    assert_eq!(sell::day_choices(7), [1, 3, 7]);
    assert_eq!(sell::day_choices(14), [1, 3, 7, 14]);
    assert_eq!(sell::day_choices(2), [1, 2]);
}

#[test]
fn fee_lines_hidden_at_zero() {
    assert!(sell::fee_lines(&MarketFees::default()).is_empty());
    let fees = MarketFees {
        listing_fee: 1_500,
        sale_fee_pct: 5,
        ..MarketFees::default()
    };
    let lines = sell::fee_lines(&fees);
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("1,500") || lines[0].contains("1500"));
    assert!(lines[1].contains("5%"));
}

/// The form's fixed lines stay inside the form and off each other: the
/// SELL FOR label sat on the slot's bottom, the currency hint on the gold
/// total and the escrow note ran past the right edge.
#[test]
fn sell_form_lines_fit() {
    use super::window::SMALL;
    let pen = window::Pen {
        kit: crate::ui::shared_stash::Kit {
            panel: 0,
            edge: 0,
            gold: 0,
            on: 0,
            field: 0,
            hit: 0,
        },
        fonts: None,
        mouse: Point::new(-1, -1),
    };
    let p = form();
    let (slot, o, win) = (p.slot(), p.builder(), p.frame());
    let right = win.x + sell::FORM_W - 8;
    // The label's row is beside the slot, clear of it.
    let tx = slot.x + i32::from(slot.w) + 8;
    let label_end = tx + pen.width(SMALL, "SELL FOR");
    let hint_start = o.x + sell::FORM_W - 16 - pen.width(SMALL, sell::COIN_HINT);
    assert!(label_end + 8 <= hint_start);
    // The gold total's row (gold box at 30..140, the total from 146).
    let total = pen.width(SMALL, "= 999,999,999 gold");
    assert!(o.x + 146 + total <= right);
    // The note beside Post listing, with room to spare for the font.
    let post = p.post();
    let px = post.x + i32::from(post.w) + 8;
    for l in sell::POST_NOTE {
        assert!(px + pen.width(SMALL, l) * 11 / 10 <= right, "{l}");
    }
}

#[test]
fn post_sends_fee_rev() {
    let (mut ui, mut src) = online();
    let p = form();
    ui.sell.put(item(7));
    // Gold typed into its box, a note, 3 days.
    let at = |s: sell::SellSpot| {
        sell::spots(&ui, &p)
            .into_iter()
            .find(|(x, _)| *x == s)
            .map(|(_, r)| centre(r))
            .unwrap_or_else(|| panic!("{s:?} not shown"))
    };
    let gold = at(sell::SellSpot::Price(PriceSpot::Gold));
    let note = at(sell::SellSpot::Note);
    let days = at(sell::SellSpot::Days(3));
    let post = at(sell::SellSpot::Post);
    assert!(sell::event(&mut ui, &p, left(gold), false));
    type_form(&mut ui, "60000");
    assert!(sell::event(&mut ui, &p, left(note), false));
    // The game's key bindings wait while the box has the typing.
    assert!(ui.text_focus());
    type_form(&mut ui, "no trades");
    assert!(sell::event(&mut ui, &p, left(days), false));
    assert!(sell::event(&mut ui, &p, left(post), false));
    let asks = ui.asks.clone();
    assert_eq!(
        asks.last(),
        Some(&MarketAsk::Post(Post {
            goods: PostGoods::Item(7),
            mode: ListingMode::Price,
            price: Some(crate::market::source::Price::gold(60_000)),
            days: 3,
            note: "no trades".into(),
            hidden: false,
            fee_rev: FEE_REV,
        }))
    );
    ui.pump(&mut src);
    // Listed: the item is in escrow, the page leaves it out.
    assert!(ui.sell.item.is_none());
    assert_eq!(ui.sell.escrowed, [7]);
    assert!(ui
        .sell
        .line
        .as_ref()
        .is_some_and(|(l, r)| !r && l.contains("escrow")));
    assert_eq!(src.shop.len(), OWN + 1);
    assert_eq!(src.shop[OWN].row.note, "no trades");
    ui.pump(&mut src);
    assert_eq!(
        ui.state.as_ref().map(|s| usize::from(s.active_listings)),
        Some(OWN + 1)
    );
    // A post with the fee revision of before is refused and the state is
    // asked for again.
    ui.sell.put(item(8));
    ui.sell.draft.gold = "100".into();
    src.set_fees(MarketFees {
        fee_rev: FEE_REV + 1,
        ..MarketFees::default()
    });
    sell::post(&mut ui);
    ui.pump(&mut src);
    assert!(ui.sell.line.as_ref().is_some_and(|(_, r)| *r));
    assert_eq!(ui.sell.item.as_ref().map(|i| i.id), Some(8));
    assert!(ui.asks.contains(&MarketAsk::Hello));
    ui.pump(&mut src);
    sell::post(&mut ui);
    ui.pump(&mut src);
    assert_eq!(ui.sell.escrowed, [7, 8]);
}

/// The window open on My shop over a fake with its made-up shop.
fn shop_open() -> (MarketUi, FakeSource) {
    let (mut ui, mut src) = online();
    src.seed_shop(&|_| None);
    ui.toggle().unwrap();
    ui.pump(&mut src);
    click(&mut ui, &mut src, Spot::Tab(MarketTab::MyShop));
    ui.pump(&mut src);
    assert!(ui.shop.view.is_some());
    (ui, src)
}

fn places() -> Places {
    Places::of(&Screen::R800)
}

fn click(ui: &mut MarketUi, src: &mut FakeSource, s: Spot) {
    let p = places();
    let (_, r) = window::spots(ui, &p)
        .into_iter()
        .find(|(x, _)| *x == s)
        .unwrap_or_else(|| panic!("{s:?} is not shown"));
    assert!(window::event(ui, &p, left(centre(r)), false), "{s:?}");
    ui.pump(src);
}

fn ids(ui: &MarketUi) -> Vec<String> {
    window::ui_elements(ui, &places())
        .into_iter()
        .map(|e| e.id)
        .collect()
}

#[test]
fn shop_tabs_counts_and_slots() {
    let (mut ui, mut src) = shop_open();
    let v = ui.shop.view.clone().unwrap();
    assert_eq!(v.tab, ShopTab::Shown);
    assert_eq!(v.counts, [4 + OWN as u32, 1, 3, 2]);
    assert_eq!((usize::from(v.slots_used), v.slots_max), (5 + OWN, 50));
    assert_eq!(v.rows.len(), 4 + OWN);
    let all = ids(&ui);
    for id in [
        "market.shop.tab(Shown)",
        "market.shop.tab(Expired)",
        "market.shop.all",
        "market.shop.row(3)",
        "market.shop.sort(name)",
    ] {
        assert!(all.iter().any(|x| x == id), "{id}");
    }
    // Nothing ticked: no buttons to press.
    assert!(!all.iter().any(|x| x == "market.shop.hide"));
    click(&mut ui, &mut src, Spot::Shop(ShopSpot::Tab(ShopTab::Sold)));
    let v = ui.shop.view.clone().unwrap();
    assert_eq!(v.tab, ShopTab::Sold);
    assert_eq!(v.rows.len(), 3);
    assert!(v
        .rows
        .iter()
        .all(|r| r.state == ListingState::Sold && r.sold.is_some()));
    // Sold rows have no buttons, even ticked.
    click(&mut ui, &mut src, Spot::Shop(ShopSpot::All));
    assert!(ShopAct::of_tab(ShopTab::Sold).is_empty());
    click(
        &mut ui,
        &mut src,
        Spot::Shop(ShopSpot::Tab(ShopTab::Expired)),
    );
    let v = ui.shop.view.clone().unwrap();
    assert_eq!(v.rows.len(), 2);
    assert!(ui.shop.selected.is_empty(), "a tab change drops the ticks");
    click(
        &mut ui,
        &mut src,
        Spot::Shop(ShopSpot::Tab(ShopTab::Hidden)),
    );
    assert_eq!(ui.shop.rows().len(), 1);
    assert!(ui.shop.rows()[0].hidden);
}

#[test]
fn bulk_select_sends_one_edit() {
    let (mut ui, mut src) = shop_open();
    click(&mut ui, &mut src, Spot::Shop(ShopSpot::Row(0)));
    click(&mut ui, &mut src, Spot::Shop(ShopSpot::Row(2)));
    assert_eq!(ui.shop.selected.len(), 2);
    click(&mut ui, &mut src, Spot::Shop(ShopSpot::Row(2)));
    assert_eq!(ui.shop.selected.len(), 1);
    click(&mut ui, &mut src, Spot::Shop(ShopSpot::All));
    assert_eq!(ui.shop.selected.len(), 4 + OWN);
    let ids_before: Vec<_> = ui.shop.rows().iter().map(|r| r.row.listing).collect();
    // Hide: one edit naming them all, with the fee revision.
    let (_, r) = window::spots(&ui, &places())
        .into_iter()
        .find(|(s, _)| *s == Spot::Shop(ShopSpot::Act(ShopAct::Hide)))
        .unwrap();
    assert!(window::event(&mut ui, &places(), left(centre(r)), false));
    let edits: Vec<_> = ui
        .asks
        .iter()
        .filter(|a| matches!(a, MarketAsk::ShopEdit { .. }))
        .cloned()
        .collect();
    assert_eq!(
        edits,
        [MarketAsk::ShopEdit {
            op: ShopOp::Hide,
            listings: ids_before.clone(),
            fee_rev: FEE_REV,
        }]
    );
    ui.pump(&mut src);
    ui.pump(&mut src);
    let v = ui.shop.view.clone().unwrap();
    assert_eq!(v.counts, [0, 5 + OWN as u32, 3, 2]);
    assert!(ui.shop.selected.is_empty() && ui.shop.marks.is_empty());
    assert!(ui
        .shop
        .line
        .as_ref()
        .is_some_and(|(l, r)| !r && l.contains(&format!("Done: {}", 4 + OWN))));
    // Hidden listings leave the search.
    for id in &ids_before {
        assert!(src.listing(*id).is_none());
    }
    // Set price...: the box, then one reprice.
    click(
        &mut ui,
        &mut src,
        Spot::Shop(ShopSpot::Tab(ShopTab::Hidden)),
    );
    click(&mut ui, &mut src, Spot::Shop(ShopSpot::All));
    click(
        &mut ui,
        &mut src,
        Spot::Shop(ShopSpot::Act(ShopAct::SetPrice)),
    );
    assert!(ui.shop.boxed.is_some());
    assert!(ids(&ui).iter().any(|i| i == "market.shop.box.apply"));
    click(
        &mut ui,
        &mut src,
        Spot::Shop(ShopSpot::BoxPrice(PriceSpot::Mode(ListingMode::Price))),
    );
    click(
        &mut ui,
        &mut src,
        Spot::Shop(ShopSpot::BoxPrice(PriceSpot::Gold)),
    );
    for c in "777".chars() {
        assert!(window::event(
            &mut ui,
            &places(),
            UiEvent::Char(c as u16),
            false
        ));
    }
    let gold = match &ui.shop.boxed {
        Some(shop::ShopBox::Price(d)) => d.gold.clone(),
        _ => panic!("no price box"),
    };
    assert!(gold.ends_with("777"));
    click(&mut ui, &mut src, Spot::Shop(ShopSpot::BoxApply));
    let reprices = src
        .asks
        .iter()
        .filter(|(_, a)| {
            matches!(
                a,
                MarketAsk::ShopEdit {
                    op: ShopOp::Reprice { .. },
                    ..
                }
            )
        })
        .count();
    assert_eq!(reprices, 1);
    ui.pump(&mut src);
    let gold: u64 = gold.parse().unwrap();
    assert!(ui
        .shop
        .rows()
        .iter()
        .all(|r| r.row.price.as_ref().is_some_and(|p| p.gold == gold)));
}

#[test]
fn bulk_row_refusals_marked() {
    let (mut ui, mut src) = shop_open();
    click(&mut ui, &mut src, Spot::Shop(ShopSpot::All));
    let gone = ui.shop.rows()[1].row.listing;
    // The listing sells while the page is shown.
    let i = src.shop.iter().position(|r| r.row.listing == gone).unwrap();
    src.shop[i].state = ListingState::Sold;
    shop::act(&mut ui, ShopAct::Cancel);
    ui.pump(&mut src);
    // The refused row stays ticked with its reason; the others are done.
    assert_eq!(ui.shop.marks.len(), 1);
    assert_eq!(ui.shop.marks.get(&gone), Some(&MarketCode::ListingGone));
    assert_eq!(ui.shop.selected.iter().copied().collect::<Vec<_>>(), [gone]);
    assert!(ui.shop.line.as_ref().is_some_and(|(l, r)| *r
        && l.contains(&format!("Done: {}", 3 + OWN))
        && l.contains("refused: 1")));
    let cancelled = src
        .shop
        .iter()
        .filter(|r| r.state == ListingState::Cancelled)
        .count();
    // The seed's cancelled one and the others.
    assert_eq!(cancelled, 4 + OWN);
}
