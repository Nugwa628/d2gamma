//! The market window through the play UI: the market key (Ctrl+I by
//! default, a Controls setting) opens it online and never lets I open the
//! inventory with Ctrl held; the stash panel's Market button is greyed
//! with its reason in single player and with the realm's market off.

use super::*;
use crate::controls::{Key, Preset};
use crate::market::fake::FakeSource;
use crate::market::source::MarketSource;
use crate::ui::states::id;
use crate::ui::ui_ids::UiElement;
use bevy::input::keyboard::KeyCode;

fn elements(u: &Ui, w: &ClientWorld) -> Vec<UiElement> {
    u.root.ui_elements(&UiCtx {
        tick: 0,
        world: w,
        strings: &NoStrings,
    })
}

fn element<'a>(all: &'a [UiElement], id: &str) -> &'a UiElement {
    all.iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no {id}"))
}

/// The play UI online over the fake market (it has answered), or in
/// single player.
fn market_ui(online: bool) -> (Ui, Box<dyn MarketSource>) {
    let mut u = ui(Some(areas()), true);
    let mut src: Box<dyn MarketSource> = Box::new(FakeSource::new(&|_| None, false, 0, true));
    let s = online.then_some(&mut *src);
    u.ui.pump_market(online, s);
    if online {
        u.ui.pump_market(true, Some(&mut *src));
    }
    (u, src)
}

/// The keys left for the game's bindings after the market key looked at
/// `code` pressed with Ctrl held or not, and the actions they run.
fn press(u: &mut Ui, ctrl: bool, code: KeyCode) -> Vec<UiEvent> {
    u.ui.set_ctrl(ctrl);
    let mut pressed = vec![code];
    crate::world_view::present::take_market_key(Some(&mut u.ui), &mut pressed);
    u.ui.set_ctrl(false);
    let b = Preset::Original.bindings().unwrap();
    crate::ui::edge::key_actions_in_mode(&b, &pressed, 1)
}

fn inventory() -> UiEvent {
    UiEvent::Action(ActionId(
        crate::controls::Action::ToggleInventory.index() as u16
    ))
}

#[test]
fn ctrl_i_opens_market_not_inventory() {
    let (mut u, mut src) = market_ui(true);
    assert!(press(&mut u, true, KeyCode::KeyI).is_empty());
    assert!(u.ui.market().shown());
    // The window asked for its first page.
    u.ui.pump_market(true, Some(&mut *src));
    assert!(u.ui.market().results.is_some());
    let w = ClientWorld::default();
    let all = elements(&u, &w);
    element(&all, "market.filter.quality(Unique)");
    element(&all, "market.result(3)");
    // I alone is the inventory's, as before; Ctrl+I again closes.
    assert_eq!(press(&mut u, false, KeyCode::KeyI), [inventory()]);
    assert!(u.ui.market().shown());
    assert!(press(&mut u, true, KeyCode::KeyI).is_empty());
    assert!(!u.ui.market().shown());
}

#[test]
fn open_market_is_rebindable() {
    let (mut u, _src) = market_ui(true);
    let mut s = crate::app::config::Settings {
        market_key: crate::ui::market::MarketKey::CtrlM.index() as u8,
        ..Default::default()
    };
    u.ui.set_settings(s);
    // Ctrl+I is not the market's now: I runs its own binding.
    assert_eq!(press(&mut u, true, KeyCode::KeyI), [inventory()]);
    assert!(!u.ui.market().shown());
    press(&mut u, true, KeyCode::KeyM);
    assert!(u.ui.market().shown());
    // Off: no chord opens it.
    s.market_key = crate::ui::market::MarketKey::Off.index() as u8;
    u.ui.set_settings(s);
    u.ui.market().close();
    for k in [KeyCode::KeyI, KeyCode::KeyM, KeyCode::KeyK] {
        press(&mut u, true, k);
        assert!(!u.ui.market().shown(), "{k:?}");
    }
    assert!(!u.ui.market().key.is_chord(Key::I, true));
}

#[test]
fn single_player_button_greyed_and_hotkey_inert() {
    let (mut u, _src) = market_ui(false);
    // Ctrl+I opens nothing (and still not the inventory).
    assert!(press(&mut u, true, KeyCode::KeyI).is_empty());
    assert!(!u.ui.market().shown());
    assert!(u.ui.market().take_asks().is_empty());
    let w = ClientWorld::default();
    u.ui.set_ui(u32::from(id::STASH), 0, false).unwrap();
    u.ui.sync_root(&mut u.root);
    let all = elements(&u, &w);
    let b = element(&all, "stash.market");
    assert_eq!(b.idle, Some("Online only"));
    assert!(!all.iter().any(|e| e.id.starts_with("market.")));
    // A click on the greyed button is taken and opens nothing.
    let at = b.at;
    u.click(&w, at);
    assert!(!u.ui.market().shown());
}

#[test]
fn market_off_button_says_the_realm_has_it_closed() {
    // Online without a market (a realm game today): closed.
    let mut u = ui(Some(areas()), true);
    u.ui.pump_market(true, None);
    let w = ClientWorld::default();
    u.ui.set_ui(u32::from(id::STASH), 0, false).unwrap();
    u.ui.sync_root(&mut u.root);
    let all = elements(&u, &w);
    let b = element(&all, "stash.market");
    assert_eq!(b.idle, Some("The market is closed on this realm"));
    // With the fake on, the button opens the window.
    let (mut u, mut src) = market_ui(true);
    u.ui.set_ui(u32::from(id::STASH), 0, false).unwrap();
    u.ui.sync_root(&mut u.root);
    let all = elements(&u, &w);
    let b = element(&all, "stash.market");
    assert_eq!(b.idle, None);
    let at = b.at;
    u.click(&w, at);
    assert!(u.ui.market().shown());
    u.ui.pump_market(true, Some(&mut *src));
    let all = elements(&u, &w);
    element(&all, "market.view(table)");
}

#[test]
fn market_button_sells_from_the_shared_stash() {
    use crate::ui::shared_stash::layout::Tab;
    use crate::ui::shared_stash::state::ShownTab;
    use crate::ui::shared_stash::{Places as StashPlaces, ShownItem, StashAsk};
    use d2_shared_stash::{Cell, ItemId};
    let (mut u, mut src) = market_ui(true);
    for s in [id::INVENTORY, id::STASH] {
        u.ui.set_ui(u32::from(s), 0, false).unwrap();
    }
    {
        let mut st = u.ui.shared_stash();
        st.on = true;
        st.tab = ShownTab(Tab::Shared(0));
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
    }
    u.ui.sync_root(&mut u.root);
    let w = ClientWorld::default();
    // With the shared stash there, the Market button shows its Market
    // category and the New listing form beside it, not the window.
    let all = elements(&u, &w);
    u.click(&w, element(&all, "stash.market").at);
    assert!(u.ui.shared_stash().selling());
    assert!(!u.ui.market().shown());
    u.ui.shared_stash().take_asks();
    let all = elements(&u, &w);
    element(&all, "market.sell.slot");
    element(&all, "market.sell.mode(offers)");
    // Ctrl+Left on the stash item puts it on the Sell slot; the stash is
    // asked for nothing (the item stays until it is posted).
    let r = StashPlaces::of(&u.ui.shared.borrow().config.screen).cell(0, 1);
    let at = Point::new(r.x + 14, r.y + 14);
    let button = PointerButton::Left;
    u.ui.set_ctrl(true);
    u.send(&w, UiEvent::Press { button, at });
    u.send(&w, UiEvent::Release { button, at });
    u.ui.set_ctrl(false);
    assert_eq!(u.ui.market().sell.item.as_ref().map(|i| i.id), Some(7));
    let asks = u.ui.shared_stash().take_asks();
    assert!(
        !asks.iter().any(|a| matches!(a, StashAsk::TakeItem { .. })),
        "{asks:?}"
    );
    // The fake learns what the slot showed on the next pump.
    u.ui.pump_market(true, Some(&mut *src));
    // The button again: back to the item pages, the form gone.
    let all = elements(&u, &w);
    u.click(&w, element(&all, "stash.market").at);
    assert!(!u.ui.shared_stash().selling());
    let all = elements(&u, &w);
    assert!(!all.iter().any(|e| e.id.starts_with("market.sell")));
}
