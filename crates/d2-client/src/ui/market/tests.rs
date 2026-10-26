use super::window::{self, Places, Spot};
use super::*;
use crate::market::fake::{FakeSource, FEE_REV};
use crate::market::source::{Group, MarketFees, MarketSort, Price, PriceMode};
use crate::ui::layout::Screen;

pub(super) fn fake() -> FakeSource {
    FakeSource::new(&|_| None, false, 0, true)
}

/// An online window over the fake market, open on Browse with its first
/// page shown.
pub(super) fn open() -> (MarketUi, FakeSource) {
    let mut ui = MarketUi::new();
    let mut src = fake();
    ui.set_online(true);
    ui.pump(&mut src);
    assert_eq!(ui.availability(), Availability::Open);
    ui.toggle().unwrap();
    ui.pump(&mut src);
    assert!(ui.shown() && ui.results.is_some());
    (ui, src)
}

pub(super) fn places() -> Places {
    Places::of(&Screen::R800)
}

/// Clicks `s` through the window's own spots (it must be shown) and lets
/// the fake answer.
pub(super) fn click(ui: &mut MarketUi, src: &mut FakeSource, s: Spot) {
    let p = places();
    let (_, r) = window::spots(ui, &p)
        .into_iter()
        .find(|(x, _)| *x == s)
        .unwrap_or_else(|| panic!("{s:?} is not shown"));
    let at = crate::ui::geom::Point::new(r.x + i32::from(r.w) / 2, r.y + i32::from(r.h) / 2);
    let e = crate::ui::panel::UiEvent::Press {
        button: crate::ui::panel::PointerButton::Left,
        at,
    };
    assert!(window::event(ui, &p, e, false), "{s:?} takes the click");
    ui.pump(src);
}

pub(super) fn ids(ui: &MarketUi) -> Vec<String> {
    window::ui_elements(ui, &places())
        .into_iter()
        .map(|e| e.id)
        .collect()
}

#[test]
fn market_off_button_greyed_with_reason() {
    let mut ui = MarketUi::new();
    assert_eq!(ui.availability().why_not(), Some("Online only"));
    assert_eq!(ui.toggle(), Err("Online only"));
    assert!(!ui.shown() && ui.take_asks().is_empty());
    // Online, before the realm answers and with its market off.
    ui.set_online(true);
    assert_eq!(ui.availability(), Availability::Closed);
    let mut src = fake();
    src.set_on(false);
    ui.pump(&mut src);
    assert_eq!(
        ui.toggle(),
        Err("The market is closed on this realm"),
        "{:?}",
        ui.state
    );
    // Switched off with the window open: it closes and says so.
    let (mut ui, mut src) = open();
    src.set_on(false);
    ui.pump(&mut src);
    assert!(!ui.shown());
    assert_eq!(ui.chat, ["The market is closed on this realm."]);
}

#[test]
fn filter_counts_update_and_ids_name_the_parts() {
    let (mut ui, mut src) = open();
    let all = ui.results.as_ref().unwrap().total;
    let uniques = ui.facets.groups[&Group::Unique];
    assert!(uniques > 0 && uniques < all);
    click(&mut ui, &mut src, Spot::Quality(Group::Unique));
    let r = ui.results.as_ref().unwrap();
    assert_eq!(r.total, uniques);
    assert!(r.rows.iter().all(|l| l.group == Group::Unique));
    // The type tree's counts are the uniques' now.
    let armor = filters::node_count(&ui.facets, 0);
    assert!(armor > 0 && armor <= uniques, "{armor}");
    let ids = ids(&ui);
    for id in [
        "market.tab(Browse)",
        "market.filter.quality(Unique)",
        "market.filter.type(armor)",
        "market.filter.expand(armor)",
        "market.filter.flag(ethereal)",
        "market.filter.price(runes)",
        "market.view(table)",
        "market.page_size(100)",
        "market.result(3)",
    ] {
        assert!(ids.iter().any(|i| i == id), "{id} in {ids:?}");
    }
    for id in &ids {
        assert!(crate::ui::ui_ids::valid_id(id), "{id}");
    }
    // Reset gives the saved default back.
    click(&mut ui, &mut src, Spot::Reset);
    assert_eq!(ui.results.as_ref().unwrap().total, all);
}

#[test]
fn grid_groups_by_quality() {
    let (ui, _) = open();
    let lines = grid::lines(&ui);
    let heads: Vec<Group> = lines
        .iter()
        .filter_map(|l| match l {
            grid::Line::Header(g) => Some(*g),
            _ => None,
        })
        .collect();
    // In the contract's grid order, each group once.
    let order: Vec<Group> = Group::ALL
        .into_iter()
        .filter(|g| heads.contains(g))
        .collect();
    assert_eq!(heads, order);
    assert!(heads.len() > 2, "{heads:?}");
    let mut current = None;
    for l in &lines {
        match l {
            grid::Line::Header(g) => current = Some(*g),
            grid::Line::Names(ix) => {
                assert!(!ix.is_empty() && ix.len() <= grid::COLUMNS);
                for i in ix {
                    assert_eq!(Some(ui.listing(*i).unwrap().group), current);
                }
            }
        }
    }
}

#[test]
fn table_sorts_by_column() {
    let (mut ui, mut src) = open();
    click(&mut ui, &mut src, Spot::View(ResultsView::Table));
    assert_eq!(ui.view, ResultsView::Table);
    assert!(ids(&ui).iter().any(|i| i == "market.sort(seller)"));
    click(&mut ui, &mut src, Spot::Sort(MarketSort::Name));
    let names = |ui: &MarketUi| -> Vec<String> {
        ui.results
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|l| l.item.name.clone())
            .collect()
    };
    let up = names(&ui);
    assert!(up.windows(2).all(|w| w[0] <= w[1]), "{up:?}");
    click(&mut ui, &mut src, Spot::Sort(MarketSort::Name));
    let down = names(&ui);
    assert!(ui.desc);
    assert!(down.windows(2).all(|w| w[0] >= w[1]), "{down:?}");
    click(&mut ui, &mut src, Spot::Sort(MarketSort::Level));
    let levels: Vec<u8> = ui
        .results
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .map(|l| l.item.level_req)
        .collect();
    assert!(levels.windows(2).all(|w| w[0] <= w[1]), "{levels:?}");
    // Back to the grid: it is ordered by group first again.
    click(&mut ui, &mut src, Spot::View(ResultsView::Grid));
    assert!(matches!(src.asks.last(), Some((_, MarketAsk::Search(q))) if q.group_first));
}

#[test]
fn page_size_choice() {
    let (mut ui, mut src) = open();
    assert_eq!(ui.page_size(), 100);
    assert_eq!(ui.results.as_ref().unwrap().rows.len(), 100);
    click(&mut ui, &mut src, Spot::PageSize(25));
    assert_eq!(ui.results.as_ref().unwrap().rows.len(), 25);
    assert_eq!(ui.pages(), 120_u16.div_ceil(25));
    click(&mut ui, &mut src, Spot::Next);
    let r = ui.results.as_ref().unwrap();
    assert_eq!((ui.page, r.page), (1, 1));
    // Each view keeps its own size.
    click(&mut ui, &mut src, Spot::View(ResultsView::Table));
    assert_eq!((ui.page_size(), ui.page), (50, 0));
    ui.set_page_size(77);
    assert_eq!(ui.page_size(), 50);
    click(&mut ui, &mut src, Spot::View(ResultsView::Grid));
    assert_eq!(ui.page_size(), 25);
}

/// Listing `i` of the page with a price, picked.
fn pick_priced(ui: &mut MarketUi, src: &mut FakeSource) -> usize {
    let i = ui
        .results
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|l| l.price.is_some() && !l.mine)
        .unwrap();
    click(ui, src, Spot::Result(i));
    assert!(ui.detail.is_some(), "the listing is looked up");
    i
}

#[test]
fn buy_sends_row_hash_and_fee_rev() {
    let (mut ui, mut src) = open();
    pick_priced(&mut ui, &mut src);
    let l = ui.selected.clone().unwrap();
    let waiting = ui.state.as_ref().unwrap().deliveries_waiting;
    assert!(ids(&ui).iter().any(|i| i == "market.buy"));
    click(&mut ui, &mut src, Spot::Buy);
    let c = ui.confirm.clone().unwrap();
    assert_eq!(
        (&c.price, c.buyer_fee, c.fee_rev),
        (l.price.as_ref().unwrap(), 0, FEE_REV)
    );
    // The confirm leaves only its own buttons.
    assert_eq!(ids(&ui), ["market.confirm", "market.cancel"]);
    click(&mut ui, &mut src, Spot::Confirm);
    let buy = src
        .asks
        .iter()
        .rev()
        .find_map(|(_, a)| match a {
            MarketAsk::Buy { .. } => Some(a.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        buy,
        MarketAsk::Buy {
            listing: l.listing,
            price_hash: l.price_hash,
            fee_rev: FEE_REV,
        }
    );
    assert!(src.listing(l.listing).is_none(), "bought");
    let note = ui.note.clone().unwrap();
    assert!(note.contains("Deliveries"), "{note}");
    assert!(ui.selected.is_none());
    assert_eq!(
        ui.state.as_ref().unwrap().deliveries_waiting,
        waiting,
        "until a new hello"
    );
    // The search after the buy goes out on the next frame.
    ui.pump(&mut src);
    assert!(ui
        .results
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .all(|r| r.listing != l.listing));
}

#[test]
fn buyer_fee_shows_in_the_confirm_and_a_new_fee_refuses() {
    let (mut ui, mut src) = open();
    src.set_fees(MarketFees {
        buyer_fee: 500,
        fee_rev: FEE_REV + 1,
        ..MarketFees::default()
    });
    pick_priced(&mut ui, &mut src);
    click(&mut ui, &mut src, Spot::Buy);
    let c = ui.confirm.clone().unwrap();
    assert_eq!((c.buyer_fee, c.fee_rev), (500, FEE_REV + 1));
    // Changed again before the confirm: refused, looked up afresh.
    src.set_fees(MarketFees {
        fee_rev: FEE_REV + 2,
        ..MarketFees::default()
    });
    click(&mut ui, &mut src, Spot::Confirm);
    assert!(ui.selected.is_some());
    ui.pump(&mut src);
    assert_eq!(ui.detail.as_ref().unwrap().fee_rev, FEE_REV + 2);
}

#[test]
fn price_changed_shows_and_refreshes() {
    let (mut ui, mut src) = open();
    pick_priced(&mut ui, &mut src);
    let l = ui.selected.clone().unwrap();
    click(&mut ui, &mut src, Spot::Buy);
    let new = Price::gold(777_777);
    src.reprice(l.listing, new.clone());
    click(&mut ui, &mut src, Spot::Confirm);
    assert!(src.listing(l.listing).is_some(), "not bought");
    let note = ui.note.clone().unwrap();
    assert!(note.to_lowercase().contains("price"), "{note}");
    // The new price is looked up and shown; a new buy carries its hash.
    ui.pump(&mut src);
    let shown = ui.selected.clone().unwrap();
    assert_eq!(shown.price, Some(new));
    assert_ne!(shown.price_hash, l.price_hash);
    click(&mut ui, &mut src, Spot::Buy);
    click(&mut ui, &mut src, Spot::Confirm);
    assert!(src.listing(l.listing).is_none(), "bought at the new price");
}

#[test]
fn offer_opens_the_form_and_tabs_switch() {
    let (mut ui, mut src) = open();
    let i = ui
        .results
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|l| l.mode.takes_offers() && !l.mine)
        .unwrap();
    click(&mut ui, &mut src, Spot::Result(i));
    click(&mut ui, &mut src, Spot::Offer);
    assert!(ui.form.is_some());
    // The form alone takes clicks while it is open.
    assert!(ids(&ui).iter().all(|i| i.starts_with("market.form.")));
    click(
        &mut ui,
        &mut src,
        Spot::Form(super::offer_form::FormSpot::Back),
    );
    assert!(ui.form.is_none());
    click(&mut ui, &mut src, Spot::Tab(MarketTab::Deliveries));
    assert_eq!(ui.tab, MarketTab::Deliveries);
    assert!(!ids(&ui).iter().any(|i| i.starts_with("market.filter")));
    // Esc closes the window; the next open shows Browse.
    let p = places();
    let esc = crate::ui::panel::UiEvent::Action(crate::ui::panel::ActionId(0));
    assert!(window::event(&mut ui, &p, esc, true));
    assert!(!ui.shown());
    ui.toggle().unwrap();
    assert_eq!(ui.tab, MarketTab::Browse);
}

#[test]
fn search_box_types_and_completes_names() {
    let (mut ui, mut src) = open();
    ui.set_names(vec![
        "Ashen Crown".into(),
        "Ash Staff".into(),
        "Crashing Ring".into(),
        "Cap".into(),
    ]);
    click(&mut ui, &mut src, Spot::Search);
    let p = places();
    for c in "as".encode_utf16() {
        assert!(window::event(
            &mut ui,
            &p,
            crate::ui::panel::UiEvent::Char(c),
            false
        ));
    }
    assert_eq!(
        ui.suggestions(),
        ["Ash Staff", "Ashen Crown", "Crashing Ring"]
    );
    // While typing, the game's keys wait.
    let key = crate::ui::panel::UiEvent::Action(crate::ui::panel::ActionId(3));
    assert!(window::event(&mut ui, &p, key, false));
    click(&mut ui, &mut src, Spot::Suggestion(1));
    assert_eq!(ui.filters.text, "Ashen Crown");
    assert!(ui.focus.is_none());
    let r = ui.results.as_ref().unwrap();
    assert!(r.total > 0);
    assert!(r.rows.iter().all(|l| l.item.name == "Ashen Crown"));
    // Off the box, the keys go to the game again.
    assert!(!window::event(&mut ui, &p, key, false));
    let _ = PriceMode::Any;
}
