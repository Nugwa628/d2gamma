//! A tiny mod stash panel: each press and key becomes the input the hook
//! promises, and the fields open, take typing and finish as they should.

use std::sync::{Arc, Mutex};

use super::*;
use crate::ui::draw::UiDraw;

/// Its spots in a row of 20-pixel squares at y 100: book 0, page 3,
/// category 1, send, sort, slot 77, cell (4, 5), other 9.
pub(crate) struct Book {
    pub seen: Arc<Mutex<Vec<StashInput>>>,
}

pub(crate) const SPOTS: [StashSpot; 8] = [
    StashSpot::Book(0),
    StashSpot::Page(3),
    StashSpot::Category(1),
    StashSpot::SendToStash,
    StashSpot::Sort,
    StashSpot::Slot(77),
    StashSpot::Cell { x: 4, y: 5 },
    StashSpot::Other(9),
];

/// The middle of spot `i`.
pub(crate) fn at(i: usize) -> Point {
    Point::new(30 + 20 * i as i32, 110)
}

impl StashPanelMod for Book {
    fn draw(&self, _ctx: &StashCtx, out: &mut dyn UiDrawSink) {
        out.push(crate::ui::panels::text(
            "mod stash".encode_utf16().collect(),
            40,
            90,
            1,
            0,
        ));
    }
    fn spot(&self, _ctx: &StashCtx, at: Point) -> Option<StashSpot> {
        if !(100..120).contains(&at.y) || at.x < 20 {
            return None;
        }
        SPOTS.get(((at.x - 20) / 20) as usize).copied()
    }
    fn input(&mut self, _ctx: &StashCtx, input: StashInput) -> Vec<StashOut> {
        self.seen.lock().unwrap().push(input.clone());
        match input {
            StashInput::Store { guid: 13 } => Vec::new(),
            StashInput::Sort => vec![StashOut::Sound(4)],
            _ => vec![StashOut::Send(b"mx".to_vec())],
        }
    }
    fn page_name(&self, page: u8) -> Option<String> {
        (page == 3).then(|| "Runes".to_string())
    }
    fn amount_max(&self, slot: u32) -> Option<u32> {
        (slot == 77).then_some(500)
    }
}

pub(crate) struct BookFactory {
    pub seen: Arc<Mutex<Vec<StashInput>>>,
}

impl StashPanelFactory for BookFactory {
    fn make(&self, _files: &UiFiles) -> Box<dyn StashPanelMod> {
        Box::new(Book {
            seen: self.seen.clone(),
        })
    }
}

fn stash() -> (ModStash, Arc<Mutex<Vec<StashInput>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let m = ModStash::new(Box::new(Book { seen: seen.clone() }));
    (m, seen)
}

fn with_ctx<R>(f: impl FnOnce(&StashCtx) -> R) -> R {
    let world = ClientWorld::default();
    let files = UiFiles::new(&[]);
    let ctx = StashCtx {
        world: &world,
        screen: Screen::R800,
        files: &files,
        fonts: None,
        mouse: Point::new(0, 0),
        cursor: Some(5),
    };
    f(&ctx)
}

fn typed(m: &mut ModStash, ctx: &StashCtx, s: &str) {
    for u in s.encode_utf16() {
        assert_eq!(m.key(ctx, u), Some(Vec::new()));
    }
}

const ENTER: u16 = 0x0D;
const ESCAPE: u16 = 0x1B;
const BACKSPACE: u16 = 0x08;

#[test]
fn left_presses_become_their_inputs() {
    let (mut m, seen) = stash();
    with_ctx(|c| {
        let none = Keys::default();
        for i in 0..SPOTS.len() {
            assert_ne!(m.press(c, at(i), false, none), Pressed::Missed);
        }
        assert_eq!(m.press(c, Point::new(5, 5), false, none), Pressed::Missed);
        // The mod's answers come back as they are.
        assert_eq!(
            m.press(c, at(4), false, none),
            Pressed::Taken(vec![StashOut::Sound(4)])
        );
    });
    let keys = Keys::default();
    assert_eq!(
        seen.lock().unwrap()[..8],
        [
            StashInput::Book(0),
            StashInput::Page(3),
            StashInput::Category(1),
            StashInput::SendToStash,
            StashInput::Sort,
            StashInput::Withdraw {
                slot: 77,
                how: Withdraw::One
            },
            StashInput::Cell {
                x: 4,
                y: 5,
                right: false,
                keys,
                cursor: Some(5)
            },
            StashInput::Other {
                id: 9,
                right: false,
                keys,
                cursor: Some(5)
            },
        ]
    );
}

#[test]
fn slot_presses_withdraw_by_their_keys() {
    let (mut m, seen) = stash();
    let k = |ctrl, shift, alt| Keys { ctrl, shift, alt };
    with_ctx(|c| {
        for keys in [
            k(false, true, false),
            k(true, false, false),
            k(false, false, true),
            // Alt wins over Ctrl, Ctrl over Shift.
            k(true, true, true),
            k(true, true, false),
        ] {
            m.press(c, at(5), false, keys);
        }
    });
    let hows: Vec<Withdraw> = seen
        .lock()
        .unwrap()
        .iter()
        .map(|i| match i {
            StashInput::Withdraw { slot: 77, how } => *how,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        hows,
        [
            Withdraw::ToInventory,
            Withdraw::AllThatFit,
            Withdraw::Pack,
            Withdraw::Pack,
            Withdraw::AllThatFit
        ]
    );
}

#[test]
fn right_on_a_slot_asks_an_amount_up_to_its_max() {
    let (mut m, seen) = stash();
    with_ctx(|c| {
        // No field: the key goes on.
        assert_eq!(m.key(c, u16::from(b'1')), None);
        assert_eq!(
            m.press(c, at(5), true, Keys::default()),
            Pressed::Taken(Vec::new())
        );
        assert_eq!(
            m.field.as_ref().map(|f| f.kind),
            Some(FieldKind::Amount { slot: 77, max: 500 })
        );
        // Only digits; past the max it is the max.
        typed(&mut m, c, "9x9a9");
        assert_eq!(m.field.as_ref().unwrap().text, "999");
        assert_eq!(m.key(c, ENTER), Some(vec![StashOut::Send(b"mx".to_vec())]));
        assert!(m.field.is_none());
        // 0 or nothing typed: no input.
        m.press(c, at(5), true, Keys::default());
        typed(&mut m, c, "0");
        assert_eq!(m.key(c, ENTER), Some(Vec::new()));
        // Escape closes the field; a press elsewhere does too.
        m.press(c, at(5), true, Keys::default());
        assert_eq!(m.key(c, ESCAPE), Some(Vec::new()));
        assert!(m.field.is_none());
        m.press(c, at(5), true, Keys::default());
        m.press(c, Point::new(0, 0), false, Keys::default());
        assert!(m.field.is_none());
    });
    assert_eq!(
        *seen.lock().unwrap(),
        [StashInput::Withdraw {
            slot: 77,
            how: Withdraw::Amount(500)
        }]
    );
}

#[test]
fn right_on_a_named_page_renames_it_up_to_twelve_characters() {
    let (mut m, seen) = stash();
    with_ctx(|c| {
        m.press(c, at(1), true, Keys::default());
        let f = m.field.clone().unwrap();
        assert_eq!(f.kind, FieldKind::Rename { page: 3 });
        assert_eq!(f.text, "Runes");
        typed(&mut m, c, " and gems galore");
        assert_eq!(
            m.field.as_ref().unwrap().text.chars().count(),
            PAGE_NAME_MAX
        );
        m.key(c, BACKSPACE);
        typed(&mut m, c, "\u{7}!");
        m.key(c, ENTER);
        // Right on a book, a category or a button opens nothing.
        for i in [0, 2, 3, 4] {
            assert_eq!(
                m.press(c, at(i), true, Keys::default()),
                Pressed::Taken(Vec::new())
            );
            assert!(m.field.is_none());
        }
    });
    assert_eq!(
        *seen.lock().unwrap(),
        [StashInput::Rename {
            page: 3,
            name: "Runes and g!".to_string()
        }]
    );
}

#[test]
fn a_page_the_mod_does_not_name_is_not_renamed() {
    struct Plain;
    impl StashPanelMod for Plain {
        fn draw(&self, _: &StashCtx, _: &mut dyn UiDrawSink) {}
        fn spot(&self, _: &StashCtx, _: Point) -> Option<StashSpot> {
            Some(StashSpot::Page(1))
        }
    }
    let mut m = ModStash::new(Box::new(Plain));
    with_ctx(|c| {
        assert_eq!(
            m.press(c, at(0), true, Keys::default()),
            Pressed::Taken(Vec::new())
        );
        assert!(m.field.is_none());
        // The default input answers nothing.
        assert_eq!(
            m.press(c, at(0), false, Keys::default()),
            Pressed::Taken(Vec::new())
        );
        assert_eq!(m.store(c, 2), None);
        // The default cover is the whole left half.
        assert_eq!(m.panel.cover(c), Some(Screen::R800.left_side_rect()));
    });
}

#[test]
fn ctrl_left_from_the_inventory_is_the_mod_s_when_it_answers() {
    let (mut m, seen) = stash();
    with_ctx(|c| {
        assert_eq!(m.store(c, 12), Some(vec![StashOut::Send(b"mx".to_vec())]));
        assert_eq!(m.store(c, 13), None);
    });
    assert_eq!(
        *seen.lock().unwrap(),
        [
            StashInput::Store { guid: 12 },
            StashInput::Store { guid: 13 }
        ]
    );
}

#[test]
fn the_panel_draws_through_the_hook() {
    let (m, _) = stash();
    let mut out: Vec<UiDraw> = Vec::new();
    with_ctx(|c| m.panel.draw(c, &mut out));
    assert!(
        matches!(&out[..], [UiDraw::Text(t)] if t.text == "mod stash".encode_utf16().collect::<Vec<_>>())
    );
}
