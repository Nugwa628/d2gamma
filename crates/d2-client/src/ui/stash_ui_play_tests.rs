//! The shared stash's Ctrl+Left on an inventory item through the play UI
//! (`item_moves` shortcut 4 and shortcut 1): a stackable with a slot goes
//! to that slot, any other item to the shown shared page, each as a store
//! ask and no game message; switched off, or with tab P shown, the game's
//! own move to its stash runs. The item's tip line says which. The
//! inventory's send button: the game's button frame with our cel, up,
//! hover and pressed.

use super::*;
use crate::bridge::items::{self, mode, ItemArtRow};
use crate::bridge::world::UnitKey;
use crate::ui::item_moves::Shortcut;
use crate::ui::own_art::{send_frame, SEND_STASH};
use crate::ui::panels::inv_items::tests::{world as item_world, Fixture};
use crate::ui::shared_stash::layout::Tab;
use crate::ui::shared_stash::state::{ShownTab, StashAsk};
use crate::ui::states::id;

/// Backpack cell (2, 0) at 800 × 600.
const INV_20: Point = Point::new(419 + 29 * 2 + 14, 315 + 14);

/// The play UI with the stash and inventory open and a shared stash on (the base
/// catalog), showing `tab`.
fn stash_ui(tab: Tab) -> Ui {
    let mut u = ui(Some(areas()), true);
    let mut art = ItemArtRows::default();
    let row = |w, h, f: &str| ItemArtRow {
        inv_w: w,
        inv_h: h,
        inv_file: f.into(),
        flippy_file: String::new(),
        beltable: false,
    };
    art.0.insert(*b"gem2", row(2, 2, "invgem2"));
    art.0.insert(*b"r01 ", row(1, 1, "invrEl"));
    u.ui.set_item_art(art);
    u.ui.set_item_frame_sizes(BTreeMap::from([
        ("invgem2".to_string(), (56, 56)),
        ("invrEl".to_string(), (28, 28)),
    ]));
    for s in [id::INVENTORY, id::STASH] {
        u.ui.set_ui(u32::from(s), 0, false).unwrap();
    }
    {
        let mut st = u.ui.shared_stash();
        st.on = true;
        st.catalog = Some(d2_shared_stash::Catalog::base(|c| Some(c.to_owned())));
        st.tab = ShownTab(tab);
    }
    u.ui.sync_root(&mut u.root);
    u
}

/// An expansion game whose local player stands in town with item 9 of
/// `code` at backpack cell (2, 0).
fn backpack_world(code: &[u8; 4]) -> ClientWorld {
    let items: [Fixture; 1] = [(9, mode::STORED, (0, 2, 0, 1), code)];
    let mut w = item_world(&items, None);
    let p = w.local_player.unwrap();
    w.units.get_mut(&p).unwrap().mode = 1;
    w.expansion = 1;
    w
}

/// The message bytes and store asks of a Ctrl+Left at `at`.
fn ctrl_left(u: &mut Ui, w: &ClientWorld, at: Point) -> (Vec<Vec<u8>>, Vec<StashAsk>) {
    let button = PointerButton::Left;
    u.ui.set_ctrl(true);
    u.send(w, UiEvent::CursorMoved(at));
    u.send(w, UiEvent::Press { button, at });
    u.send(w, UiEvent::Release { button, at });
    u.ui.set_ctrl(false);
    let msgs = u.root.take_intents().into_iter().map(|i| i.0).collect();
    (msgs, u.ui.shared_stash().take_asks())
}

fn switched_off(u: &mut Ui, s: Shortcut) {
    let mut c = crate::app::config::Settings::default();
    c.item_shortcuts.set(s, false);
    u.ui.set_settings(c);
}

/// The Ctrl+Left tip lines of backpack item 9.
fn ctrl_left_hints(u: &Ui, w: &ClientWorld) -> Vec<String> {
    let it = items::item(w, UnitKey::new(items::ITEM, 9)).expect("item 9");
    let lines = u.ui.shared.borrow().shortcut_hints(w, &it);
    lines
        .iter()
        .map(|l| String::from_utf16_lossy(&l.text))
        .filter(|t| t.starts_with("Ctrl+Left"))
        .collect()
}

#[test]
fn ctrl_left_sends_a_backpack_rune_to_its_stash_slot() {
    let w = backpack_world(b"r01 ");
    let mut u = stash_ui(Tab::Personal);
    let (msgs, asks) = ctrl_left(&mut u, &w, INV_20);
    assert!(msgs.is_empty(), "{msgs:?}");
    assert_eq!(asks, [StashAsk::PutStack { guid: 9 }]);
    assert_eq!(
        ctrl_left_hints(&u, &w),
        ["Ctrl+Left: Move to its stash slot"]
    );
}

#[test]
fn ctrl_left_puts_a_backpack_item_on_the_shown_shared_page() {
    let w = backpack_world(b"gem2");
    let mut u = stash_ui(Tab::Shared(2));
    let (msgs, asks) = ctrl_left(&mut u, &w, INV_20);
    assert!(msgs.is_empty(), "{msgs:?}");
    let put = StashAsk::PutItem {
        guid: 9,
        page: 2,
        at: None,
    };
    assert_eq!(asks, [put]);
    assert_eq!(ctrl_left_hints(&u, &w), ["Ctrl+Left: Move to shared stash"]);
}

#[test]
fn with_tab_p_or_the_shortcut_off_the_game_s_stash_move_runs() {
    // Not a stackable, tab P shown: the game's own stash.
    let w = backpack_world(b"gem2");
    let mut u = stash_ui(Tab::Personal);
    let (msgs, asks) = ctrl_left(&mut u, &w, INV_20);
    assert_eq!(msgs.first(), Some(&vec![0x19, 9, 0, 0, 0]));
    assert!(asks.is_empty());
    assert_eq!(ctrl_left_hints(&u, &w), ["Ctrl+Left: Move to stash"]);
    // A rune with shortcut 4 off.
    let w = backpack_world(b"r01 ");
    let mut u = stash_ui(Tab::Personal);
    switched_off(&mut u, Shortcut::StackSlot);
    let (msgs, asks) = ctrl_left(&mut u, &w, INV_20);
    assert_eq!(msgs.first(), Some(&vec![0x19, 9, 0, 0, 0]));
    assert!(asks.is_empty());
    // A shared page shown with shortcut 1 off: no move at all.
    let w = backpack_world(b"gem2");
    let mut u = stash_ui(Tab::Shared(1));
    switched_off(&mut u, Shortcut::Stash);
    let (msgs, asks) = ctrl_left(&mut u, &w, INV_20);
    assert!(asks.is_empty());
    assert!(msgs.is_empty(), "{msgs:?}");
}

/// The send button's rectangle (its control-channel element), after a
/// move to a corner.
fn send_button(u: &mut Ui, w: &ClientWorld) -> Rect {
    u.send(w, UiEvent::CursorMoved(Point::new(5, 5)));
    let ctx = UiCtx {
        tick: 0,
        world: w,
        strings: &NoStrings,
    };
    let els = u.root.ui_elements(&ctx);
    crate::ui::ui_ids::find(&els, "inventory.send_stackables")
        .expect("the send button")
        .rect
}

/// (file, frame, x, y) of the send button's draws: the game's up or
/// pressed button frame and our cel.
fn send_draws(u: &Ui, w: &ClientWorld) -> Vec<(String, u32, i32, i32)> {
    u.images(w)
        .into_iter()
        .filter(|(n, f, ..)| n == SEND_STASH || (n == "panel\\buysellbtn" && *f < 2))
        .collect()
}

#[test]
fn the_send_button_is_the_game_s_frame_with_our_cel_up_hover_and_pressed() {
    let w = backpack_world(b"r01 ");
    let mut u = stash_ui(Tab::Personal);
    let r = send_button(&mut u, &w);
    assert_eq!((r.w, r.h), (32, 32));
    // Cel draws name their bottom-left pixel.
    let (x, y) = (r.x, r.y + 31);
    let frame = |f| ("panel\\buysellbtn".to_string(), f, x, y);
    let icon = |f, d| (SEND_STASH.to_string(), f, x + d, y + d);
    u.send(&w, UiEvent::CursorMoved(Point::new(5, 5)));
    assert_eq!(send_draws(&u, &w), [frame(0), icon(send_frame::NORMAL, 0)]);
    let at = crate::ui::ui_ids::centre(r);
    u.send(&w, UiEvent::CursorMoved(at));
    assert_eq!(send_draws(&u, &w), [frame(0), icon(send_frame::HOVER, 0)]);
    let button = PointerButton::Left;
    u.send(&w, UiEvent::Press { button, at });
    assert_eq!(send_draws(&u, &w), [frame(1), icon(send_frame::PRESSED, 1)]);
    assert_eq!(
        u.ui.shared_stash().take_asks(),
        [StashAsk::SendStackables { guids: vec![9] }]
    );
    // Held and moved off: up again; the release ends the press.
    u.send(&w, UiEvent::CursorMoved(Point::new(5, 5)));
    assert_eq!(send_draws(&u, &w), [frame(0), icon(send_frame::NORMAL, 0)]);
    u.send(&w, UiEvent::CursorMoved(at));
    u.send(&w, UiEvent::Release { button, at });
    assert_eq!(send_draws(&u, &w), [frame(0), icon(send_frame::HOVER, 0)]);
    assert!(u.ui.shared_stash().take_asks().is_empty());
}

#[test]
fn the_send_button_sits_under_the_grid_s_last_column() {
    let w = backpack_world(b"r01 ");
    let mut u = stash_ui(Tab::Personal);
    let r = send_button(&mut u, &w);
    // Its right edge is the backpack grid's, its top 8 below the grid.
    let sh = u.ui.shared.borrow();
    let g = sh
        .items
        .layout(Facts::of(&w).class, &sh.config.screen)
        .unwrap()
        .grid;
    assert_eq!((r.x + 32, r.y), (g.right, g.bottom + 8));
}

mod mod_panel {
    //! A mod game's stash panel (`crate::mods::stash`) through the play
    //! UI: drawn on the open stash in place of the shared stash's parts,
    //! its presses, typing and Ctrl+Left from the inventory going out as
    //! the mod's messages; without one the shared stash is as before.

    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::mods::stash::tests::{at, BookFactory};
    use crate::mods::stash::StashInput;
    use crate::mods::ClientMods;

    /// The play UI with `mods`, the stash and inventory open and a shared
    /// stash on (base catalog, tab P).
    fn with_mods(mods: Option<ClientMods>) -> Ui {
        let config = UiConfig {
            screen: Screen::R800,
            expansion_installed: true,
        };
        let mut ui = OriginalUi::new(config, Some(areas())).unwrap();
        if let Some(m) = mods {
            ui.set_mods(m);
        }
        let mut root = UiRoot::new(Box::new(NoPanelRules));
        ui.install(&mut root).unwrap();
        let mut u = Ui { ui, root };
        // Any palette: the shared stash draws in its kit colours.
        let colors = [d2_formats::palette::Rgb::default(); 256];
        u.ui.set_palette(&d2_formats::palette::Palette { colors });
        for s in [id::INVENTORY, id::STASH] {
            u.ui.set_ui(u32::from(s), 0, false).unwrap();
        }
        {
            let mut st = u.ui.shared_stash();
            st.on = true;
            st.catalog = Some(d2_shared_stash::Catalog::base(|c| Some(c.to_owned())));
        }
        u.ui.sync_root(&mut u.root);
        u
    }

    fn book() -> (Ui, Arc<Mutex<Vec<StashInput>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mods = ClientMods::new().with_stash_panel(BookFactory { seen: seen.clone() });
        (with_mods(Some(mods)), seen)
    }

    fn texts(u: &Ui, w: &ClientWorld) -> Vec<String> {
        let ctx = UiCtx {
            tick: 0,
            world: w,
            strings: &NoStrings,
        };
        let mut out: Vec<UiDraw> = Vec::new();
        u.root.draw(&ctx, &mut out);
        out.into_iter()
            .filter_map(|d| match d {
                UiDraw::Text(t) => Some(String::from_utf16_lossy(&t.text)),
                _ => None,
            })
            .collect()
    }

    fn all_draws(u: &Ui, w: &ClientWorld) -> Vec<UiDraw> {
        let ctx = UiCtx {
            tick: 0,
            world: w,
            strings: &NoStrings,
        };
        let mut out: Vec<UiDraw> = Vec::new();
        u.root.draw(&ctx, &mut out);
        out
    }

    fn mx() -> Vec<u8> {
        crate::bridge::intent::mod_message(b"mx").unwrap()
    }

    #[test]
    fn the_mod_panel_draws_in_place_of_the_shared_stash() {
        let w = backpack_world(b"gem2");
        let plain = with_mods(None);
        let t = texts(&plain, &w);
        assert!(t.iter().any(|s| s == "Runes"), "{t:?}");
        assert!(!t.iter().any(|s| s == "mod stash"));
        let (u, _) = book();
        let t = texts(&u, &w);
        assert!(t.iter().any(|s| s == "mod stash"), "{t:?}");
        assert!(!t.iter().any(|s| s == "Runes"), "{t:?}");
        // The inventory's send button is the shared stash's: gone too.
        assert!(send_draws(&u, &w).is_empty());
    }

    #[test]
    fn without_a_stash_panel_the_shared_stash_is_unchanged() {
        let w = backpack_world(b"gem2");
        let plain = with_mods(None);
        let empty = with_mods(Some(ClientMods::new()));
        assert!(ClientMods::new().stash_panel().is_none());
        assert_eq!(all_draws(&plain, &w), all_draws(&empty, &w));
        // Ctrl+Left still asks the shared store.
        let mut u = with_mods(Some(ClientMods::new()));
        {
            let mut st = u.ui.shared_stash();
            st.tab = ShownTab(Tab::Shared(1));
        }
        let (msgs, asks) = ctrl_left(&mut u, &w, INV_20);
        assert!(msgs.is_empty());
        assert_eq!(asks.len(), 1);
    }

    #[test]
    fn presses_on_the_panel_go_to_the_mod_as_its_messages() {
        let w = backpack_world(b"gem2");
        let (mut u, seen) = book();
        u.click(&w, at(3));
        assert_eq!(*seen.lock().unwrap(), [StashInput::SendToStash]);
        let sent: Vec<Vec<u8>> = u.root.take_intents().into_iter().map(|i| i.0).collect();
        assert_eq!(sent, [mx()]);
        // Inside the cover nothing reaches the game's stash grid.
        u.click(&w, Point::new(100, 300));
        assert!(u.root.take_intents().is_empty());
        assert!(u.ui.shared_stash().take_asks().is_empty());
    }

    #[test]
    fn the_rename_field_takes_the_typing() {
        let w = backpack_world(b"gem2");
        let (mut u, seen) = book();
        let at = at(1);
        let b = PointerButton::Right;
        u.send(&w, UiEvent::Press { button: b, at });
        u.send(&w, UiEvent::Release { button: b, at });
        assert!(u.ui.text_focus());
        for c in "!".encode_utf16().chain([0x0D]) {
            u.root_char(&w, c);
        }
        assert!(!u.ui.text_focus());
        assert_eq!(
            *seen.lock().unwrap(),
            [StashInput::Rename {
                page: 3,
                name: "Runes!".into()
            }]
        );
        assert_eq!(
            u.root
                .take_intents()
                .into_iter()
                .map(|i| i.0)
                .collect::<Vec<_>>(),
            [mx()]
        );
    }

    #[test]
    fn ctrl_left_on_an_inventory_item_is_the_mod_s() {
        let w = backpack_world(b"r01 ");
        let (mut u, seen) = book();
        let (msgs, asks) = ctrl_left(&mut u, &w, INV_20);
        assert_eq!(*seen.lock().unwrap(), [StashInput::Store { guid: 9 }]);
        assert_eq!(msgs, [mx()]);
        assert!(asks.is_empty());
    }
}
