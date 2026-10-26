// D2RUST's own: the shared stash's places on the stash panel.
//! Where the shared stash's parts sit on the stash panel (ui 0x19), in
//! the panel's own pixels: x from the left panel slot's first column
//! ([`Screen::sx`]), y from the row the game's stash grid is laid out from
//! (`−sy + panel_dy`; the grid's top is 82 below it). The panel art is
//! drawn at its native size, so every rectangle follows the art in any
//! frame and UI Scale (the UI frame is scaled as a whole).
//!
//! - Page tabs across the top of the inner frame, above the game's gold
//!   line: P (the character's own stash) and the shared pages I..X.
//! - Category buttons down the left edge: Stash, Runes, Gems, Misc, and
//!   under them Market (online selling: the item page with the market's
//!   sell form beside the panel, `crate::ui::market::sell`; the button is
//!   the market's, greyed when the market cannot open).
//! - The page area: an item page's [`COLS`] x [`ROWS`] cells, or a
//!   stackable page's slots. It covers the game's gold lines and grid.
//! - Under it, left of the close button: the shared gold with its two
//!   buttons, the search box and "Sort".

use d2_shared_stash::{StackPage, COLS, PAGES, ROWS};

use super::state::{SharedStashUi, Shown};
use crate::ui::geom::{Point, Rect};
use crate::ui::layout::Screen;
use crate::ui::ui_ids::UiElement;

/// One item cell's size, as the game's stash grid.
pub const CELL: i32 = 29;
/// A stackable slot: the item picture's cell and its count line.
pub const SLOT_W: i32 = 29;
pub const SLOT_H: i32 = 38;

/// The page tabs: P then I..X.
pub const TABS: usize = 1 + PAGES as usize;
const TAB_X: i32 = 6;
const TAB_Y: i32 = 4;
const TAB_W: i32 = 26;
const TAB_H: i32 = 20;
const TAB_STEP: i32 = 28;

const CAT_X: i32 = 4;
const CAT_Y: i32 = 90;
const CAT_W: i32 = 64;
const CAT_H: i32 = 26;
const CAT_STEP: i32 = 30;

/// The page area's top-left: the game grid's left column, under the tabs.
pub const AREA_X: i32 = 74;
pub const AREA_Y: i32 = 30;

const GOLD_Y: i32 = 354;
const SEARCH_Y: i32 = 376;
const ROW_H: i32 = 18;
/// The bottom rows end left of the close button (x 273).
const ROWS_END: i32 = 268;

/// A page tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    /// The character's own stash (the game's panel, unchanged).
    Personal,
    /// Shared item page 0..[`PAGES`].
    Shared(u8),
}

impl Tab {
    pub fn index(self) -> usize {
        match self {
            Tab::Personal => 0,
            Tab::Shared(p) => 1 + usize::from(p),
        }
    }

    pub fn of_index(i: usize) -> Option<Tab> {
        match i {
            0 => Some(Tab::Personal),
            i if i < TABS => Some(Tab::Shared((i - 1) as u8)),
            _ => None,
        }
    }

    /// "P", then Roman numerals.
    pub fn label(self) -> &'static str {
        const ROMAN: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
        match self {
            Tab::Personal => "P",
            Tab::Shared(p) => ROMAN.get(usize::from(p)).copied().unwrap_or("?"),
        }
    }
}

/// A category button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    /// The item pages (the selected tab).
    Stash,
    Stacks(StackPage),
    /// The item pages for selling: a press puts an item on the market's
    /// sell slot instead of lifting it.
    Market,
}

/// The category buttons the shared stash draws; Market's sits under them
/// ([`MARKET_SLOT`]) and is the market's button.
pub const CATEGORIES: [Category; 4] = [
    Category::Stash,
    Category::Stacks(StackPage::Runes),
    Category::Stacks(StackPage::Gems),
    Category::Stacks(StackPage::Misc),
];

/// The place of the Market category's button under the others.
pub const MARKET_SLOT: usize = CATEGORIES.len();

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Stash => "Stash",
            Category::Stacks(StackPage::Runes) => "Runes",
            Category::Stacks(StackPage::Gems) => "Gems",
            Category::Stacks(StackPage::Misc) => "Misc",
            // A mod game's category page: D2Gamma's panel shows none.
            Category::Stacks(StackPage::Category(_)) => "Category",
            Category::Market => "Market",
        }
    }
}

/// What a point of the panel is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    Tab(Tab),
    Category(Category),
    /// A cell of the shown item page.
    Cell {
        x: u8,
        y: u8,
    },
    /// A slot of the shown stackable page, by its place.
    Slot {
        x: u8,
        y: u8,
    },
    /// The stackable page's free area (a drop there goes to the item's slot).
    SlotArea,
    GoldIn,
    GoldOut,
    Search,
    Sort,
}

/// The shared stash's rectangles on a screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Places {
    x0: i32,
    y0: i32,
}

impl Places {
    pub fn of(s: &Screen) -> Places {
        Places {
            x0: s.sx(),
            y0: -s.sy() + s.panel_dy(),
        }
    }

    fn r(&self, x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect::new(self.x0 + x, self.y0 + y, w as u16, h as u16)
    }

    pub fn tab(&self, t: Tab) -> Rect {
        self.r(TAB_X + TAB_STEP * t.index() as i32, TAB_Y, TAB_W, TAB_H)
    }

    pub fn category(&self, i: usize) -> Rect {
        self.r(CAT_X, CAT_Y + CAT_STEP * i as i32, CAT_W, CAT_H)
    }

    /// The page area: the item page's cells.
    pub fn area(&self) -> Rect {
        self.r(
            AREA_X,
            AREA_Y,
            CELL * i32::from(COLS),
            CELL * i32::from(ROWS),
        )
    }

    /// Cell (x, y) of the item page.
    pub fn cell(&self, x: u8, y: u8) -> Rect {
        self.r(
            AREA_X + CELL * i32::from(x),
            AREA_Y + CELL * i32::from(y),
            CELL,
            CELL,
        )
    }

    /// Slot (x, y) of a stackable page with `cols` columns, centred in the
    /// area.
    pub fn slot(&self, cols: u8, x: u8, y: u8) -> Rect {
        let pad = (CELL * i32::from(COLS) - SLOT_W * i32::from(cols)) / 2;
        self.r(
            AREA_X + pad + SLOT_W * i32::from(x),
            AREA_Y + SLOT_H * i32::from(y),
            SLOT_W,
            SLOT_H,
        )
    }

    /// The shared gold line (the value), then its two buttons.
    pub fn gold(&self) -> Rect {
        self.r(AREA_X, GOLD_Y, ROWS_END - 82 - AREA_X, ROW_H)
    }
    pub fn gold_in(&self) -> Rect {
        self.r(ROWS_END - 78, GOLD_Y, 36, ROW_H)
    }
    pub fn gold_out(&self) -> Rect {
        self.r(ROWS_END - 38, GOLD_Y, 38, ROW_H)
    }
    pub fn search(&self) -> Rect {
        self.r(AREA_X, SEARCH_Y, ROWS_END - 62 - AREA_X, ROW_H)
    }
    pub fn sort(&self) -> Rect {
        self.r(ROWS_END - 58, SEARCH_Y, 58, ROW_H)
    }

    /// The part of the panel the shared stash covers on a shared page:
    /// the page area (over the game's gold lines and grid), then the bottom
    /// rows left of the close button.
    pub fn cover(&self) -> [Rect; 2] {
        let area_bottom = AREA_Y + CELL * i32::from(ROWS) + 3;
        [
            self.r(
                AREA_X - 4,
                AREA_Y - 4,
                CELL * i32::from(COLS) + 8,
                area_bottom - (AREA_Y - 4),
            ),
            self.r(
                AREA_X - 4,
                area_bottom,
                ROWS_END + 4 - (AREA_X - 4),
                SEARCH_Y + ROW_H + 4 - area_bottom,
            ),
        ]
    }

    /// `p` is on the cover.
    pub fn in_cover(&self, p: Point) -> bool {
        self.cover().iter().any(|r| r.contains(p))
    }

    /// What `p` is. `shared`: an item page or stackable page is shown
    /// (on tab P only the tabs and the category buttons are the shared
    /// stash's; the rest is the game's panel). `slots`: the shown
    /// stackable page's column count, `None` on an item page.
    pub fn spot(&self, p: Point, shared: bool, slots: Option<u8>) -> Option<Spot> {
        for i in 0..TABS {
            let t = Tab::of_index(i)?;
            if self.tab(t).contains(p) {
                return Some(Spot::Tab(t));
            }
        }
        for (i, c) in CATEGORIES.into_iter().enumerate() {
            if self.category(i).contains(p) {
                return Some(Spot::Category(c));
            }
        }
        if !shared {
            return None;
        }
        let checks = [
            (self.gold_in(), Spot::GoldIn),
            (self.gold_out(), Spot::GoldOut),
            (self.search(), Spot::Search),
            (self.sort(), Spot::Sort),
        ];
        if let Some((_, s)) = checks.iter().find(|(r, _)| r.contains(p)) {
            return Some(*s);
        }
        let area = self.area();
        if !area.contains(p) {
            return None;
        }
        match slots {
            None => Some(Spot::Cell {
                x: ((p.x - area.x) / CELL) as u8,
                y: ((p.y - area.y) / CELL) as u8,
            }),
            Some(cols) => {
                let rows = (i32::from(ROWS) * CELL / SLOT_H) as u8;
                for y in 0..rows {
                    for x in 0..cols {
                        if self.slot(cols, x, y).contains(p) {
                            return Some(Spot::Slot { x, y });
                        }
                    }
                }
                Some(Spot::SlotArea)
            }
        }
    }
}

/// The shared stash's parts as named UI elements (the test driver and the
/// shot runs click them by name): `stash.tab(I)`, `stash.category(Runes)`,
/// the shown page's `stash.page.slot(x,y)` or `stash.stack(code)`, and the
/// bottom row's buttons.
pub fn ui_elements(st: &SharedStashUi, p: &Places) -> Vec<UiElement> {
    let mut out = Vec::new();
    for i in 0..TABS {
        if let Some(t) = Tab::of_index(i) {
            out.push(UiElement::new(
                format!("stash.tab({})", t.label()),
                p.tab(t),
            ));
        }
    }
    for (i, c) in CATEGORIES.into_iter().enumerate() {
        out.push(UiElement::new(
            format!("stash.category({})", c.label()),
            p.category(i),
        ));
    }
    match st.shown() {
        Shown::Personal => return out,
        Shown::Page(_) => {
            for y in 0..ROWS {
                for x in 0..COLS {
                    out.push(UiElement::new(
                        format!("stash.page.slot({x},{y})"),
                        p.cell(x, y),
                    ));
                }
            }
        }
        Shown::Stacks(page) => {
            if let Some(cat) = &st.catalog {
                for s in cat.page(page) {
                    out.push(UiElement::new(
                        format!("stash.stack({})", s.code),
                        p.slot(page.cols(), s.at.0, s.at.1),
                    ));
                }
            }
        }
    }
    for (name, r) in [
        ("stash.gold.in", p.gold_in()),
        ("stash.gold.out", p.gold_out()),
        ("stash.search", p.search()),
        ("stash.sort", p.sort()),
    ] {
        out.push(UiElement::new(name, r));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_parts_sit_on_the_panel_and_apart() {
        let s = Screen::WIDE;
        let p = Places::of(&s);
        // The panel slot is the frame's first 320 columns.
        let panel = Rect::new(s.sx(), -s.sy() + s.panel_dy() - 60, 320, 490);
        let mut all = vec![p.area(), p.gold_in(), p.gold_out(), p.search(), p.sort()];
        all.extend((0..TABS).map(|i| p.tab(Tab::of_index(i).unwrap())));
        all.extend((0..CATEGORIES.len()).map(|i| p.category(i)));
        for (i, a) in all.iter().enumerate() {
            assert!(
                a.x >= panel.x && a.right() <= panel.right(),
                "{a:?} in the panel"
            );
            for b in &all[i + 1..] {
                let apart = a.right() <= i64::from(b.x)
                    || b.right() <= i64::from(a.x)
                    || a.bottom() <= i64::from(b.y)
                    || b.bottom() <= i64::from(a.y);
                assert!(apart, "{a:?} and {b:?} overlap");
            }
        }
        // The cover keeps off the close button.
        let close = crate::ui::panels::stash_input::stash_close_rect(&s, true);
        for c in p.cover() {
            let apart = c.right() <= i64::from(close.x) || c.bottom() <= i64::from(close.y);
            assert!(apart, "{c:?} covers the close button {close:?}");
        }
        // The page area covers the game's gold lines and grid.
        let g = crate::ui::panels::stash_items::fallback_stash_grid(true, &s);
        let a = p.area();
        assert_eq!(a.x, g.left);
        assert!(a.y < g.top - 50 && a.bottom() >= i64::from(g.bottom));
    }

    #[test]
    fn points_find_their_parts_in_every_frame() {
        for s in [Screen::R800, Screen::WIDE, Screen::PLAY] {
            let p = Places::of(&s);
            let mid = |r: Rect| Point::new(r.x + i32::from(r.w) / 2, r.y + i32::from(r.h) / 2);
            assert_eq!(
                p.spot(mid(p.tab(Tab::Shared(9))), false, None),
                Some(Spot::Tab(Tab::Shared(9)))
            );
            assert_eq!(
                p.spot(mid(p.category(2)), false, None),
                Some(Spot::Category(CATEGORIES[2]))
            );
            // On tab P the rest is the game's.
            assert_eq!(p.spot(mid(p.cell(3, 3)), false, None), None);
            assert_eq!(
                p.spot(mid(p.cell(7, 10)), true, None),
                Some(Spot::Cell { x: 7, y: 10 })
            );
            assert_eq!(
                p.spot(mid(p.slot(7, 6, 4)), true, Some(7)),
                Some(Spot::Slot { x: 6, y: 4 })
            );
            assert_eq!(
                p.spot(mid(p.cell(7, 10)), true, Some(7)),
                Some(Spot::SlotArea)
            );
            assert_eq!(p.spot(mid(p.sort()), true, None), Some(Spot::Sort));
        }
    }

    #[test]
    fn tabs_are_named_p_then_roman() {
        let names: Vec<&str> = (0..TABS)
            .map(|i| Tab::of_index(i).unwrap().label())
            .collect();
        assert_eq!(
            names,
            ["P", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"]
        );
        assert_eq!(Tab::of_index(TABS), None);
    }
}
