// D2RUST's own: the shared stash panel's clicks and keys.
//! A press on the shared stash's parts becomes a [`StashAsk`] (or a change
//! of what is shown). Plain clicks work as the game's stash: a click on an
//! item picks it up onto the cursor, a click with an item held drops it.
//! The item-moving shortcuts of the stash pages
//! ([`crate::ui::item_moves::Shortcut`], each behind its settings switch):
//! - `TakeCount` (5) Shift+Left on a slot: take a chosen number (the "how
//!   many" box);
//! - `TakeAll` (6) Ctrl+Left on a slot: take as many as fit;
//! - `TabDrop` (10) Shift+Left on a page tab with an item held: drop it
//!   onto that page;
//! - `Stash` (1) Ctrl+Left on a shared item: into the inventory.
//!
//! A stackable dropped anywhere on a stackable page goes to its own slot.
//! On the Market category's page a press lifts nothing: it becomes a
//! [`MarketPick`] for the market's sell slot (Ctrl+Left: straight onto it).

use d2_shared_stash::{Cell, StackPage};

use super::layout::{Category, Places, Spot, Tab};
use super::state::{CountBox, MarketPick, SharedStashUi, Shown, StashAsk};
use crate::ui::geom::Point;
use crate::ui::item_moves::{Shortcut, ShortcutSet};

/// The modifier keys of a press and the shortcuts switched on (the
/// settings' switches less a mod game's, `ItemsUi::shortcut_set`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub on: ShortcutSet,
}

impl Mods {
    /// The keys with every shortcut on.
    pub fn keys(ctrl: bool, shift: bool) -> Mods {
        Mods {
            ctrl,
            shift,
            on: ShortcutSet::ALL,
        }
    }

    /// Shift held (not Ctrl) and `s` on.
    fn shift_for(self, s: Shortcut) -> bool {
        self.shift && !self.ctrl && self.on.has(s)
    }

    /// Ctrl held (not Shift) and `s` on.
    fn ctrl_for(self, s: Shortcut) -> bool {
        self.ctrl && !self.shift && self.on.has(s)
    }
}

/// The largest count a slot holds, the "take all" count.
const ALL: u32 = u32::MAX;

/// The stackable page's column count.
pub fn slot_cols(page: StackPage) -> u8 {
    page.cols()
}

/// A left press at `p`. `cursor`: the GUID of the item held on the
/// cursor. Returns whether the shared stash took the press (else the
/// game's panel handles it).
pub fn press(
    st: &mut SharedStashUi,
    places: &Places,
    p: Point,
    m: Mods,
    cursor: Option<u32>,
) -> bool {
    if !st.on {
        return false;
    }
    let shown = st.shown();
    let slots = match shown {
        Shown::Stacks(page) => Some(slot_cols(page)),
        _ => None,
    };
    let spot = places.spot(p, st.covers(), slots);
    // A press anywhere else ends the typing in the search box.
    if spot != Some(Spot::Search) {
        st.search_focus = false;
    }
    if spot.is_some() {
        st.count_box = None;
    }
    let Some(spot) = spot else {
        return st.covers() && places.in_cover(p);
    };
    match spot {
        Spot::Tab(t) => {
            if let (Some(guid), Tab::Shared(page), true) =
                (cursor, t, m.shift_for(Shortcut::TabDrop))
            {
                st.ask(StashAsk::PutItem {
                    guid,
                    page,
                    at: None,
                });
                return true;
            }
            st.tab.0 = t;
            if st.category.0 != Category::Market {
                st.category.0 = Category::Stash;
            }
            st.ask(StashAsk::Refresh);
        }
        Spot::Category(c) => {
            st.category.0 = c;
            st.ask(StashAsk::Refresh);
        }
        Spot::Cell { x, y } => {
            let Shown::Page(page) = shown else {
                return true;
            };
            if st.selling() {
                st.market_pick = Some(match (cursor, item_at(st, page, x, y)) {
                    (Some(_), _) => MarketPick::Cursor,
                    (None, Some(id)) => MarketPick::Item {
                        id,
                        ctrl: m.ctrl && !m.shift,
                    },
                    (None, None) => MarketPick::Empty,
                });
                return true;
            }
            match cursor {
                Some(guid) => st.ask(StashAsk::PutItem {
                    guid,
                    page,
                    at: Some((x, y)),
                }),
                None => {
                    if let Some(it) = item_at(st, page, x, y) {
                        st.ask(StashAsk::TakeItem {
                            id: it,
                            to_cursor: !m.ctrl_for(Shortcut::Stash),
                        });
                    }
                }
            }
        }
        // A stackable dropped anywhere on its page goes to its own slot.
        Spot::Slot { .. } | Spot::SlotArea if cursor.is_some() => {
            if let Some(guid) = cursor {
                st.ask(StashAsk::PutStack { guid });
            }
        }
        Spot::SlotArea => {}
        Spot::Slot { x, y } => {
            let Shown::Stacks(page) = shown else {
                return true;
            };
            let Some(code) = st
                .catalog
                .as_ref()
                .and_then(|c| c.at(page, (x, y)))
                .map(|s| s.code.clone())
            else {
                return true;
            };
            if st.stacks.get(&code).copied().unwrap_or(0) == 0 {
                return true;
            }
            if m.shift_for(Shortcut::TakeCount) {
                st.count_box = Some(CountBox {
                    code,
                    text: String::new(),
                });
            } else if m.ctrl_for(Shortcut::TakeAll) {
                st.ask(StashAsk::TakeStack { code, count: ALL });
            } else {
                st.ask(StashAsk::TakeStack { code, count: 1 });
            }
        }
        Spot::GoldIn => st.ask(StashAsk::Gold { put: true }),
        Spot::GoldOut => st.ask(StashAsk::Gold { put: false }),
        Spot::Search => st.search_focus = true,
        Spot::Sort => {
            if let Shown::Page(page) = shown {
                st.ask(StashAsk::Arrange { page });
            }
        }
    }
    true
}

/// The shown item covering cell (x, y) of `page`.
fn item_at(st: &SharedStashUi, page: u8, x: u8, y: u8) -> Option<d2_shared_stash::ItemId> {
    st.items
        .iter()
        .find(|i| {
            let c: Cell = i.cell;
            c.page == page
                && (c.x..c.x + i.size.0).contains(&x)
                && (c.y..c.y + i.size.1).contains(&y)
        })
        .map(|i| i.id)
}

/// A typed character (UTF-16 unit) or key: to the "how many" box, else to
/// the search box when it has the focus. Returns whether it was taken.
pub fn key(st: &mut SharedStashUi, unit: u16) -> bool {
    const BACKSPACE: u16 = 0x08;
    const ENTER: u16 = 0x0D;
    const ESCAPE: u16 = 0x1B;
    if let Some(b) = &mut st.count_box {
        match unit {
            ENTER => {
                let n: u32 = b.text.parse().unwrap_or(0);
                let code = b.code.clone();
                st.count_box = None;
                if n > 0 {
                    st.ask(StashAsk::TakeStack { code, count: n });
                }
            }
            ESCAPE => st.count_box = None,
            BACKSPACE => {
                b.text.pop();
            }
            u if (u16::from(b'0')..=u16::from(b'9')).contains(&u) && b.text.len() < 4 => {
                b.text.push(char::from(u as u8));
            }
            _ => {}
        }
        return true;
    }
    if !st.search_focus {
        return false;
    }
    match unit {
        ENTER | ESCAPE => st.search_focus = false,
        BACKSPACE => {
            st.search.pop();
            let text = st.search.clone();
            st.ask(StashAsk::Search { text });
        }
        u if u >= 0x20 && st.search.chars().count() < 24 => {
            if let Some(c) = char::from_u32(u32::from(u)) {
                st.search.push(c);
                let text = st.search.clone();
                st.ask(StashAsk::Search { text });
            }
        }
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::layout::Screen;
    use crate::ui::shared_stash::state::ShownItem;
    use d2_shared_stash::ItemId;

    fn mid(r: crate::ui::geom::Rect) -> Point {
        Point::new(r.x + i32::from(r.w) / 2, r.y + i32::from(r.h) / 2)
    }

    fn ui() -> SharedStashUi {
        SharedStashUi {
            on: true,
            catalog: Some(d2_shared_stash::Catalog::base(|c| Some(c.to_owned()))),
            ..SharedStashUi::default()
        }
    }

    #[test]
    fn tabs_and_categories_change_the_page() {
        let p = Places::of(&Screen::WIDE);
        let mut st = ui();
        assert!(press(
            &mut st,
            &p,
            mid(p.tab(Tab::Shared(2))),
            Mods::default(),
            None
        ));
        assert_eq!(st.shown(), Shown::Page(2));
        assert!(press(
            &mut st,
            &p,
            mid(p.category(1)),
            Mods::default(),
            None
        ));
        assert_eq!(st.shown(), Shown::Stacks(StackPage::Runes));
        assert!(press(
            &mut st,
            &p,
            mid(p.tab(Tab::Personal)),
            Mods::default(),
            None
        ));
        assert_eq!(st.shown(), Shown::Personal);
        assert_eq!(st.take_asks(), vec![StashAsk::Refresh; 3]);
        // The game's panel takes the rest of tab P.
        assert!(!press(
            &mut st,
            &p,
            mid(p.cell(2, 2)),
            Mods::default(),
            None
        ));
    }

    #[test]
    fn items_are_picked_up_dropped_and_sent_on() {
        let p = Places::of(&Screen::WIDE);
        let mut st = ui();
        st.tab.0 = Tab::Shared(4);
        st.items.push(ShownItem {
            id: ItemId(7),
            cell: Cell::new(4, 1, 1),
            size: (2, 3),
            code: "xpl".into(),
            name: "Plate".into(),
            quality: 2,
            level_req: 0,
            sockets: 0,
            ethereal: false,
            look: None,
            fillers: Vec::new(),
        });
        press(&mut st, &p, mid(p.cell(2, 3)), Mods::default(), None);
        press(
            &mut st,
            &p,
            mid(p.cell(2, 3)),
            Mods::keys(true, false),
            None,
        );
        press(&mut st, &p, mid(p.cell(6, 9)), Mods::default(), Some(55));
        // Shortcut 10: Shift+Left on tab IX with an item held.
        press(
            &mut st,
            &p,
            mid(p.tab(Tab::Shared(8))),
            Mods::keys(false, true),
            Some(55),
        );
        press(&mut st, &p, mid(p.sort()), Mods::default(), None);
        assert_eq!(
            st.take_asks(),
            vec![
                StashAsk::TakeItem {
                    id: ItemId(7),
                    to_cursor: true
                },
                StashAsk::TakeItem {
                    id: ItemId(7),
                    to_cursor: false
                },
                StashAsk::PutItem {
                    guid: 55,
                    page: 4,
                    at: Some((6, 9))
                },
                StashAsk::PutItem {
                    guid: 55,
                    page: 8,
                    at: None
                },
                StashAsk::Arrange { page: 4 },
            ]
        );
        assert_eq!(st.shown(), Shown::Page(4), "a tab drop doesn't switch");
    }

    #[test]
    fn slots_take_one_some_or_all_and_catch_drops() {
        let p = Places::of(&Screen::WIDE);
        let mut st = ui();
        st.category.0 = Category::Stacks(StackPage::Runes);
        st.stacks.insert("r01".into(), 12);
        let el = mid(p.slot(8, 0, 0));
        let eld = mid(p.slot(8, 1, 0));
        press(&mut st, &p, el, Mods::default(), None);
        press(&mut st, &p, el, Mods::keys(true, false), None);
        // Empty slot: nothing.
        press(&mut st, &p, eld, Mods::default(), None);
        // A drop anywhere on the page goes to the item's slot.
        press(&mut st, &p, mid(p.cell(7, 10)), Mods::default(), Some(9));
        press(&mut st, &p, el, Mods::keys(false, true), None);
        for u in "25\r".encode_utf16() {
            assert!(key(&mut st, u));
        }
        assert_eq!(
            st.take_asks(),
            vec![
                StashAsk::TakeStack {
                    code: "r01".into(),
                    count: 1
                },
                StashAsk::TakeStack {
                    code: "r01".into(),
                    count: ALL
                },
                StashAsk::PutStack { guid: 9 },
                StashAsk::TakeStack {
                    code: "r01".into(),
                    count: 25
                },
            ]
        );
        // Switched off: Ctrl+Left is a plain take of one.
        let off = Mods {
            on: ShortcutSet::ALL.without(ShortcutSet(Shortcut::TakeAll.bit())),
            ..Mods::keys(true, false)
        };
        press(&mut st, &p, el, off, None);
        assert_eq!(
            st.take_asks(),
            vec![StashAsk::TakeStack {
                code: "r01".into(),
                count: 1
            }]
        );
    }

    #[test]
    fn the_search_box_takes_typing_while_focused() {
        let p = Places::of(&Screen::WIDE);
        let mut st = ui();
        st.tab.0 = Tab::Shared(0);
        assert!(!key(&mut st, u16::from(b'a')), "no focus: not taken");
        press(&mut st, &p, mid(p.search()), Mods::default(), None);
        for u in "sha".encode_utf16() {
            key(&mut st, u);
        }
        key(&mut st, 0x08);
        assert_eq!(st.search, "sh");
        assert_eq!(
            st.take_asks().last(),
            Some(&StashAsk::Search { text: "sh".into() })
        );
        key(&mut st, 0x0D);
        assert!(!st.search_focus);
    }
}
