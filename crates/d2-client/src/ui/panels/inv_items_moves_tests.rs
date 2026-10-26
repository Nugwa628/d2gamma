//! The item-moving shortcuts on the client model: what each modifier-click
//! sends (the lift and the drag's own drop message), and what it refuses
//! with nothing sent. Synthetic fixtures, no game files.

use super::super::tests::{world, Fixture};
use super::*;
use crate::bridge::items::{mode, ItemArtRow, ItemArtRows};
use crate::ui::item_moves::Trade;

const CTRL: Mods = Mods {
    ctrl: true,
    shift: false,
};
const CTRL_SHIFT: Mods = Mods {
    ctrl: true,
    shift: true,
};

/// The base game's grids: inventory 10 × 4, expansion stash 6 × 8, cube
/// 3 × 4, trade box 10 × 4.
const SIZES: PageSizes = PageSizes {
    inventory: (10, 4),
    stash: (6, 8),
    cube: (3, 4),
    trade: (10, 4),
};

/// Art rows: a 1 × 1 potion, a 1 × 2 rune-sized item, a 2 × 2 gem stand-in,
/// a 2 × 4 two-handed weapon and the 2 × 2 cube.
fn items_ui() -> ItemsUi {
    let mut art = ItemArtRows::default();
    let row = |w, h, f: &str| ItemArtRow {
        inv_w: w,
        inv_h: h,
        inv_file: f.into(),
        flippy_file: String::new(),
        beltable: false,
    };
    art.0.insert(*b"hp1 ", row(1, 1, "invhp1"));
    art.0.insert(*b"jav ", row(1, 2, "invjav"));
    art.0.insert(*b"gem2", row(2, 2, "invgem2"));
    art.0.insert(*b"2hs ", row(2, 4, "inv2hs"));
    art.0.insert(*b"box ", row(2, 2, "invbox"));
    ItemsUi {
        art,
        ..ItemsUi::default()
    }
}

fn stash() -> Open {
    Open {
        stash: true,
        ..Open::default()
    }
}

fn cube_open() -> Open {
    Open {
        cube: true,
        ..Open::default()
    }
}

fn trade(locked: bool) -> Open {
    Open {
        trade: Some(Trade { locked }),
        ..Open::default()
    }
}

/// The message bytes of a shortcut's outputs.
fn sent(o: &ShortcutOutcome) -> Vec<Vec<u8>> {
    o.as_ref()
        .expect("sent")
        .iter()
        .map(|o| match o {
            PanelOutput::Intent(i) => i.0.clone(),
            other => panic!("{other:?}"),
        })
        .collect()
}

fn lift(item: u32) -> Vec<u8> {
    [&[0x19u8][..], &item.to_le_bytes()].concat()
}

fn place(item: u32, x: u32, y: u32, page: u32) -> Vec<u8> {
    [
        &[0x18u8][..],
        &item.to_le_bytes(),
        &x.to_le_bytes(),
        &y.to_le_bytes(),
        &page.to_le_bytes(),
    ]
    .concat()
}

/// The shortcut of a `mods` + `button` click on item `guid` of `w`.
fn press(
    u: &ItemsUi,
    w: &ClientWorld,
    guid: u32,
    mods: Mods,
    button: Button,
    open: Open,
) -> Option<ShortcutOutcome> {
    let it = items::local_items(w)
        .into_iter()
        .find(|i| i.key.guid == guid)
        .expect("the item");
    u.shortcut_press(w, &SIZES, &it, mods, button, open)
}

/// `n` 2 × 2 gems filling page `page` (stored location page + 1) of a
/// `cols` × `rows` grid from guid 100 on, column by column.
fn fill(cols: u16, rows: u16, page: u8) -> Vec<Fixture<'static>> {
    let mut v = Vec::new();
    for x in (0..cols).step_by(2) {
        for y in (0..rows).step_by(2) {
            v.push((
                100 + v.len() as u32,
                mode::STORED,
                (0, x, y, page + 1),
                b"gem2",
            ));
        }
    }
    v
}

#[test]
fn ctrl_left_sends_an_inventory_item_to_the_stash_s_first_free_spot() {
    let u = items_ui();
    // A gem at stash (0, 0): the next 2 × 2 spot down column 0 is (0, 2).
    let w = world(
        &[
            (9, mode::STORED, (0, 4, 1, 1), b"gem2"),
            (8, mode::STORED, (0, 0, 0, 5), b"gem2"),
        ],
        None,
    );
    let o = press(&u, &w, 9, CTRL, Button::Left, stash()).unwrap();
    assert_eq!(sent(&o), [lift(9), place(9, 0, 2, 4)]);
    // And back: the inventory's first free spot.
    let w = world(&[(8, mode::STORED, (0, 3, 5, 5), b"gem2")], None);
    let o = press(&u, &w, 8, CTRL, Button::Left, stash()).unwrap();
    assert_eq!(sent(&o), [lift(8), place(8, 0, 0, 0)]);
}

#[test]
fn a_full_stash_or_inventory_sends_nothing_and_says_so() {
    let u = items_ui();
    let mut items = fill(6, 8, 4);
    items.push((9, mode::STORED, (0, 0, 0, 1), b"hp1 "));
    let w = world(&items, None);
    assert_eq!(
        press(&u, &w, 9, CTRL, Button::Left, stash()),
        Some(Err(Refusal::NoRoom(4)))
    );
    // The inventory full: a stash item stays.
    let mut items = fill(10, 4, 0);
    items.push((9, mode::STORED, (0, 0, 0, 5), b"hp1 "));
    let w = world(&items, None);
    assert_eq!(
        press(&u, &w, 9, CTRL, Button::Left, stash()),
        Some(Err(Refusal::NoRoom(0)))
    );
}

#[test]
fn a_two_handed_item_needs_its_whole_footprint_free() {
    let u = items_ui();
    // Stash columns 0–3 full, column 4 full, column 5 free: a 2-wide item
    // has no room, a 1 × 2 one goes to (5, 0).
    let mut items: Vec<Fixture> = fill(4, 8, 4);
    for y in 0..8 {
        items.push((200 + u32::from(y), mode::STORED, (0, 4, y, 5), b"hp1 "));
    }
    items.push((9, mode::STORED, (0, 0, 0, 1), b"2hs "));
    items.push((10, mode::STORED, (0, 2, 0, 1), b"jav "));
    let w = world(&items, None);
    assert_eq!(
        press(&u, &w, 9, CTRL, Button::Left, stash()),
        Some(Err(Refusal::NoRoom(4)))
    );
    let o = press(&u, &w, 10, CTRL, Button::Left, stash()).unwrap();
    assert_eq!(sent(&o), [lift(10), place(10, 5, 0, 4)]);
    // In an empty stash the two-hander takes the corner.
    let w = world(&[(9, mode::STORED, (0, 0, 0, 1), b"2hs ")], None);
    let o = press(&u, &w, 9, CTRL, Button::Left, stash()).unwrap();
    assert_eq!(sent(&o), [lift(9), place(9, 0, 0, 4)]);
}

#[test]
fn ctrl_left_with_the_cube_open_moves_in_and_out_of_the_cube() {
    let u = items_ui();
    let w = world(
        &[
            (6, mode::STORED, (0, 8, 0, 1), b"box "),
            (9, mode::STORED, (0, 0, 0, 1), b"gem2"),
            (8, mode::STORED, (0, 0, 0, 4), b"hp1 "),
        ],
        None,
    );
    // Cube (0, 0) holds a potion: the gem goes below it, to (0, 1).
    let o = press(&u, &w, 9, CTRL, Button::Left, cube_open()).unwrap();
    assert_eq!(sent(&o), [lift(9), place(9, 0, 1, 3)]);
    // The gem takes inventory (0, 0)–(1, 1): the potion goes to (0, 2).
    let o = press(&u, &w, 8, CTRL, Button::Left, cube_open()).unwrap();
    assert_eq!(sent(&o), [lift(8), place(8, 0, 2, 0)]);
    // The cube itself can't go in.
    assert_eq!(
        press(&u, &w, 6, CTRL, Button::Left, cube_open()),
        Some(Err(Refusal::CubeInCube))
    );
    // A full cube.
    let mut items = fill(2, 4, 3);
    items.extend([
        (300, mode::STORED, (0, 2, 0, 4), b"jav "),
        (301, mode::STORED, (0, 2, 2, 4), b"jav "),
        (9, mode::STORED, (0, 0, 0, 1), b"hp1 "),
    ]);
    let w = world(&items, None);
    assert_eq!(
        press(&u, &w, 9, CTRL, Button::Left, cube_open()),
        Some(Err(Refusal::NoRoom(3)))
    );
}

#[test]
fn ctrl_left_with_a_trade_open_fills_the_own_box_unless_locked() {
    let u = items_ui();
    let w = world(
        &[
            (9, mode::STORED, (0, 0, 0, 1), b"gem2"),
            (8, mode::STORED, (0, 0, 0, 3), b"hp1 "),
        ],
        None,
    );
    let o = press(&u, &w, 9, CTRL, Button::Left, trade(false)).unwrap();
    assert_eq!(sent(&o), [lift(9), place(9, 0, 1, 2)]);
    let o = press(&u, &w, 8, CTRL, Button::Left, trade(false)).unwrap();
    assert_eq!(sent(&o), [lift(8), place(8, 0, 2, 0)]);
    for guid in [9, 8] {
        assert_eq!(
            press(&u, &w, guid, CTRL, Button::Left, trade(true)),
            Some(Err(Refusal::TradeLocked))
        );
    }
}

#[test]
fn ctrl_right_drops_an_inventory_item_at_the_feet() {
    let u = items_ui();
    let w = world(&[(9, mode::STORED, (0, 3, 1, 1), b"2hs ")], None);
    let o = press(&u, &w, 9, CTRL, Button::Right, Open::default()).unwrap();
    assert_eq!(
        sent(&o),
        [lift(9), [&[0x17u8][..], &9u32.to_le_bytes()].concat()]
    );
    assert_eq!(
        press(&u, &w, 9, CTRL, Button::Right, trade(false)),
        Some(Err(Refusal::Trading))
    );
}

#[test]
fn ctrl_shift_right_puts_an_item_in_the_closed_cube() {
    let u = items_ui();
    let cube = (6, mode::STORED, (0, 8, 0, 1), b"box ");
    let put = |item: u32| [&[0x2Au8][..], &item.to_le_bytes(), &6u32.to_le_bytes()].concat();
    let w = world(
        &[
            cube,
            (9, mode::STORED, (0, 0, 0, 1), b"gem2"),
            (8, mode::STORED, (0, 1, 1, 5), b"hp1 "),
        ],
        None,
    );
    let o = press(&u, &w, 9, CTRL_SHIFT, Button::Right, Open::default()).unwrap();
    assert_eq!(sent(&o), [lift(9), put(9)]);
    // A stash item too, with the stash open.
    let o = press(&u, &w, 8, CTRL_SHIFT, Button::Right, stash()).unwrap();
    assert_eq!(sent(&o), [lift(8), put(8)]);
    // Not the cube into itself.
    assert_eq!(
        press(&u, &w, 6, CTRL_SHIFT, Button::Right, Open::default()),
        Some(Err(Refusal::CubeInCube))
    );
    // No cube in the inventory (one in the stash doesn't count).
    let w = world(
        &[
            (6, mode::STORED, (0, 0, 0, 5), b"box "),
            (9, mode::STORED, (0, 0, 0, 1), b"gem2"),
        ],
        None,
    );
    assert_eq!(
        press(&u, &w, 9, CTRL_SHIFT, Button::Right, Open::default()),
        Some(Err(Refusal::NoCube))
    );
    // A full cube.
    let mut items = fill(2, 4, 3);
    items.extend([
        (300, mode::STORED, (0, 2, 0, 4), b"jav "),
        (301, mode::STORED, (0, 2, 2, 4), b"jav "),
        cube,
        (9, mode::STORED, (0, 0, 0, 1), b"hp1 "),
    ]);
    let w = world(&items, None);
    assert_eq!(
        press(&u, &w, 9, CTRL_SHIFT, Button::Right, Open::default()),
        Some(Err(Refusal::NoRoom(3)))
    );
}

#[test]
fn nothing_is_a_shortcut_while_an_item_is_held() {
    let u = items_ui();
    let w = world(
        &[
            (9, mode::STORED, (0, 0, 0, 1), b"gem2"),
            (7, mode::CURSOR, (0, 0, 0, 0), b"hp1 "),
        ],
        Some(7),
    );
    assert_eq!(press(&u, &w, 9, CTRL, Button::Left, stash()), None);
    assert_eq!(press(&u, &w, 9, CTRL, Button::Right, Open::default()), None);
}

#[test]
fn a_switched_off_shortcut_leaves_the_plain_click() {
    let mut u = items_ui();
    let w = world(&[(9, mode::STORED, (0, 0, 0, 1), b"gem2")], None);
    u.shortcuts.set(Shortcut::Stash, false);
    assert_eq!(press(&u, &w, 9, CTRL, Button::Left, stash()), None);
    u.shortcuts = ShortcutSet::ALL;
    // A mod game's switch-off.
    u.mod_shortcuts = crate::mods::ClientMods::new()
        .without_item_shortcuts(&[Shortcut::Drop])
        .item_shortcuts();
    assert_eq!(press(&u, &w, 9, CTRL, Button::Right, Open::default()), None);
    assert!(press(&u, &w, 9, CTRL, Button::Left, stash()).is_some());
}

#[test]
fn the_vendor_s_sell_is_the_grid_click_s_and_has_its_own_switch() {
    let mut u = items_ui();
    u.tips = Some(crate::ui::item_tip::tests::tips());
    let store = Open {
        store: true,
        ..Open::default()
    };
    let w = world(&[(7, mode::STORED, (0, 2, 3, 1), b"cap ")], None);
    // The shortcut leaves the sell to the grid click (0x33).
    assert_eq!(press(&u, &w, 7, CTRL, Button::Left, store), None);
    let it = items::local_items(&w).remove(0);
    let hints = |u: &ItemsUi| {
        u.shortcut_hints(&w, &it, store)
            .iter()
            .map(|l| String::from_utf16_lossy(&l.text))
            .collect::<Vec<_>>()
    };
    assert_eq!(hints(&u), ["Ctrl+Left: Sell", "Ctrl+Right: Drop"]);
    u.shortcuts.set(Shortcut::Sell, false);
    assert_eq!(hints(&u), ["Ctrl+Right: Drop"]);
}

#[test]
fn an_equipped_item_has_no_shortcut_lines() {
    let u = items_ui();
    let w = world(&[(9, mode::BODY, (3, 0, 0, 0), b"gem2")], None);
    let it = items::local_items(&w).remove(0);
    assert!(u.shortcut_hints(&w, &it, stash()).is_empty());
}
