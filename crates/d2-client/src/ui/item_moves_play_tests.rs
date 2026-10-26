//! The item-moving shortcuts through the play UI: a modifier-click on a
//! stash or backpack item sends the lift and the drag's own drop message,
//! a move that can't fit sends nothing and queues a note, and the
//! settings' and a mod's switches turn a shortcut back into the plain
//! click. The tool tip of a stash item ends with its shortcut lines.

use super::*;
use crate::bridge::items::{self, mode, ItemArtRow};
use crate::mods::ClientMods;
use crate::ui::item_moves::Shortcut;
use crate::ui::panels::inv_items::tests::{world as item_world, Fixture};
use crate::ui::states::id;

/// Stash cell (0, 0) and backpack cell (2, 0) at 800 × 600 (the
/// expansion stash, record 28).
const STASH_00: Point = Point::new(154 + 14, 142 + 14);
const INV_20: Point = Point::new(419 + 29 * 2 + 14, 315 + 14);

fn shortcuts_ui(open: &[u8]) -> Ui {
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
    art.0.insert(*b"box ", row(2, 2, "invbox"));
    art.0.insert(*b"cap ", row(2, 2, "invcap"));
    u.ui.set_item_art(art);
    u.ui.set_item_frame_sizes(BTreeMap::from([
        ("invgem2".to_string(), (56, 56)),
        ("invbox".to_string(), (56, 56)),
        ("invcap".to_string(), (56, 56)),
    ]));
    for &s in open {
        u.ui.set_ui(u32::from(s), 0, false).unwrap();
    }
    u.ui.sync_root(&mut u.root);
    u
}

/// An expansion game whose local player stands (mode 1) in town with
/// `items`.
fn items_world(items: &[Fixture]) -> ClientWorld {
    let mut w = item_world(items, None);
    let p = w.local_player.unwrap();
    w.units.get_mut(&p).unwrap().mode = 1;
    w.expansion = 1;
    w
}

/// The message bytes of a `button` click at `at` with Ctrl (and Shift
/// when `shift`) held.
fn mod_click(
    u: &mut Ui,
    w: &ClientWorld,
    at: Point,
    button: PointerButton,
    shift: bool,
) -> Vec<Vec<u8>> {
    u.ui.set_ctrl(true);
    u.ui.set_shift(shift);
    u.send(w, UiEvent::CursorMoved(at));
    u.send(w, UiEvent::Press { button, at });
    u.send(w, UiEvent::Release { button, at });
    u.ui.set_ctrl(false);
    u.ui.set_shift(false);
    u.root.take_intents().into_iter().map(|i| i.0).collect()
}

fn bytes(m: &d2_proto::client::InsertItemInBuffer) -> Vec<u8> {
    ClientIntent::from_message(m).0
}

/// C→S `id` with the little-endian `args`.
fn msg(id: u8, args: &[u32]) -> Vec<u8> {
    std::iter::once(id)
        .chain(args.iter().flat_map(|a| a.to_le_bytes()))
        .collect()
}

fn notes(u: &Ui) -> Vec<String> {
    u.ui.shared.borrow().messages.pending_notes()
}

#[test]
fn ctrl_left_moves_between_the_stash_and_the_backpack() {
    let mut u = shortcuts_ui(&[id::STASH]);
    let w = items_world(&[(9, mode::STORED, (0, 0, 0, 5), b"gem2")]);
    let got = mod_click(&mut u, &w, STASH_00, PointerButton::Left, false);
    assert_eq!(got, [msg(0x19, &[9]), bytes(&items::insert(9, 0, 0, 0))]);
    let w = items_world(&[(9, mode::STORED, (0, 2, 0, 1), b"gem2")]);
    let got = mod_click(&mut u, &w, INV_20, PointerButton::Left, false);
    assert_eq!(got, [msg(0x19, &[9]), bytes(&items::insert(9, 0, 0, 4))]);
    assert!(notes(&u).is_empty());
}

#[test]
fn a_full_stash_sends_nothing_and_queues_a_note() {
    let mut u = shortcuts_ui(&[id::STASH]);
    let mut items: Vec<Fixture> = Vec::new();
    for x in (0..6).step_by(2) {
        for y in (0..8).step_by(2) {
            items.push((
                100 + items.len() as u32,
                mode::STORED,
                (0, x, y, 5),
                b"gem2",
            ));
        }
    }
    items.push((9, mode::STORED, (0, 2, 0, 1), b"gem2"));
    let w = items_world(&items);
    assert!(mod_click(&mut u, &w, INV_20, PointerButton::Left, false).is_empty());
    assert_eq!(notes(&u), ["Not enough room in the stash"]);
}

#[test]
fn ctrl_right_drops_and_ctrl_shift_right_puts_in_the_cube() {
    let mut u = shortcuts_ui(&[id::INVENTORY]);
    let w = items_world(&[(9, mode::STORED, (0, 2, 0, 1), b"gem2")]);
    let got = mod_click(&mut u, &w, INV_20, PointerButton::Right, false);
    assert_eq!(got, [msg(0x19, &[9]), msg(0x17, &[9])]);
    // A stash item into the backpack's cube, the cube left closed.
    let mut u = shortcuts_ui(&[id::STASH]);
    let w = items_world(&[
        (6, mode::STORED, (0, 8, 0, 1), b"box "),
        (9, mode::STORED, (0, 0, 0, 5), b"gem2"),
    ]);
    let got = mod_click(&mut u, &w, STASH_00, PointerButton::Right, true);
    assert_eq!(got, [msg(0x19, &[9]), msg(0x2A, &[9, 6])]);
    assert!(!u.ui.is_open(id::CUBE));
}

#[test]
fn the_settings_and_a_mod_switch_a_shortcut_off() {
    let w = items_world(&[(9, mode::STORED, (0, 2, 0, 1), b"gem2")]);
    let mut u = shortcuts_ui(&[id::STASH]);
    let mut s = crate::app::config::Settings::default();
    s.item_shortcuts.set(Shortcut::Stash, false);
    u.ui.set_settings(s);
    assert!(mod_click(&mut u, &w, INV_20, PointerButton::Left, false).is_empty());
    assert!(notes(&u).is_empty());
    let mut u = shortcuts_ui(&[id::STASH]);
    u.ui.set_mods(ClientMods::new().without_item_shortcuts(&[Shortcut::Stash]));
    assert!(mod_click(&mut u, &w, INV_20, PointerButton::Left, false).is_empty());
    // The others stay on.
    let mut u = shortcuts_ui(&[id::INVENTORY]);
    u.ui.set_mods(ClientMods::new().without_item_shortcuts(&[Shortcut::Stash]));
    assert_eq!(
        mod_click(&mut u, &w, INV_20, PointerButton::Right, false).len(),
        2
    );
}

/// A compact, identified stash record of `code` at stash cell (0, 0),
/// which the tool tip decodes.
fn compact_stash_stream(code: &[u8; 4]) -> Vec<u8> {
    use d2_proto::item_bits::hflag;
    let bits: Vec<(u32, u32)> = vec![
        (hflag::COMPACT | hflag::IDENTIFIED, 32),
        (0x65, 10),
        (u32::from(mode::STORED), 3),
        (0, 4),
        (0, 4),
        (0, 4),
        (5, 3),
        (u32::from_le_bytes(*code), 32),
    ];
    let (mut out, mut acc, mut n) = (Vec::new(), 0u64, 0u32);
    for (v, w) in bits {
        acc |= u64::from(v) << n;
        n += w;
        while n >= 8 {
            out.push(acc as u8);
            acc >>= 8;
            n -= 8;
        }
    }
    if n > 0 {
        out.push(acc as u8);
    }
    out
}

#[test]
fn a_stash_item_s_tip_ends_with_its_shortcut_lines() {
    use crate::bridge::world::{KindData, UnitKey};
    let mut u = shortcuts_ui(&[id::STASH]);
    u.ui.set_item_tips(crate::ui::item_tip::tests::tips());
    let mut w = items_world(&[(9, mode::STORED, (0, 0, 0, 5), b"cap ")]);
    let unit = w.units.get_mut(&UnitKey::new(items::ITEM, 9)).unwrap();
    let KindData::Item(d) = &mut unit.kind else {
        unreachable!()
    };
    d.last.as_mut().unwrap().stream = compact_stash_stream(b"cap ");
    u.send(&w, UiEvent::CursorMoved(STASH_00));
    let ctx = UiCtx {
        tick: 0,
        world: &w,
        strings: &NoStrings,
    };
    let mut out: Vec<UiDraw> = Vec::new();
    u.root.draw(&ctx, &mut out);
    let tip = out
        .iter()
        .find_map(|d| match d {
            UiDraw::Text(t) => Some(String::from_utf16_lossy(&t.text)),
            _ => None,
        })
        .expect("the stash item's tip");
    assert!(tip.contains("Cap"), "{tip:?}");
    assert!(tip.contains("Ctrl+Left: Move to inventory"), "{tip:?}");
    // No cube in the backpack, and a stash item doesn't drop.
    assert!(!tip.contains("Ctrl+Right"), "{tip:?}");
    assert!(!tip.contains("Ctrl+Shift+Right"), "{tip:?}");
}
