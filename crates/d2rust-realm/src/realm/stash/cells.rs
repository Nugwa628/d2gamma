//! Where items lie on the online stash's item pages: the realm's shape
//! (D2Gamma: [`PAGES`] grids of [`COLS`] x [`ROWS`] cells, the same pages
//! as the single-player stash's, `d2-shared-stash`), one rectangle per
//! item, no two overlapping; and a mod realm's category pages, one whole
//! item per slot, at page ids from `CATEGORY_PAGE` on
//! ([`super::settings`]).

use std::collections::{BTreeMap, BTreeSet};

use d2_net::msg::stash::StashCell;
use d2_shared_stash::{Grid, CATEGORY_PAGE};

use super::settings::cell;

/// D2Gamma's item pages, I to X.
pub const PAGES: u8 = d2_shared_stash::PAGES;
/// Columns of one of D2Gamma's pages.
pub const COLS: u8 = d2_shared_stash::COLS;
/// Rows of one of D2Gamma's pages.
pub const ROWS: u8 = d2_shared_stash::ROWS;

/// The size given to an item whose size was lost.
const LARGEST: (u8, u8) = (2, 4);

/// One placed item: its top-left cell and its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub cell: StashCell,
    pub size: (u8, u8),
}

impl Placement {
    fn overlaps(&self, o: &Placement) -> bool {
        if self.cell.page >= CATEGORY_PAGE {
            // A category slot holds one item whatever its size.
            return self.cell == o.cell;
        }
        self.cell.page == o.cell.page
            && self.cell.x < o.cell.x + o.size.0
            && o.cell.x < self.cell.x + self.size.0
            && self.cell.y < o.cell.y + o.size.1
            && o.cell.y < self.cell.y + self.size.1
    }
}

/// Every item's place in one stash, on the realm's pages (`grid`;
/// D2Gamma's by default).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub by_item: BTreeMap<u32, Placement>,
    pub grid: Grid,
}

impl Layout {
    /// An empty layout on `grid`.
    pub fn on(grid: Grid) -> Layout {
        Layout {
            by_item: BTreeMap::new(),
            grid,
        }
    }

    /// Whether `p` lies on a page: inside an item page, or on a category
    /// slot.
    pub fn holds(&self, p: &Placement) -> bool {
        let c = cell(p.cell);
        if c.category().is_some() {
            return p.size.0 > 0 && p.size.1 > 0 && self.grid.slot_of(c).is_some();
        }
        c.fits_in(p.size, &self.grid.shape)
    }

    /// Whether `p` is on the pages and free of every item but `except`.
    pub fn is_free(&self, p: &Placement, except: Option<u32>) -> bool {
        self.holds(p)
            && self
                .by_item
                .iter()
                .filter(|(id, _)| Some(**id) != except)
                .all(|(_, o)| !p.overlaps(o))
    }

    /// The first free cell for `size` on the item pages: page by page from
    /// `first`, then the pages before it; on each page row by row, left
    /// to right.
    pub fn first_free(&self, size: (u8, u8), first: u8) -> Option<StashCell> {
        let pages = self.grid.shape.pages.min(CATEGORY_PAGE);
        if pages == 0 {
            return None;
        }
        (0..pages)
            .map(|i| ((u16::from(first % pages) + u16::from(i)) % u16::from(pages)) as u8)
            .find_map(|page| self.first_free_on(size, page))
    }

    /// The first free cell for `size` on page `page`; on a category page
    /// its first free whole-item slot.
    pub fn first_free_on(&self, size: (u8, u8), page: u8) -> Option<StashCell> {
        if let Some(c) = page.checked_sub(CATEGORY_PAGE) {
            let &(_, slots) = self.grid.categories.get(usize::from(c))?;
            return (0..slots).find_map(|i| {
                let at = self.grid.slot_cell(c, i)?;
                let cell = StashCell {
                    page: at.page,
                    x: at.x,
                    y: at.y,
                };
                self.is_free(&Placement { cell, size }, None)
                    .then_some(cell)
            });
        }
        for y in 0..self.grid.shape.rows {
            for x in 0..self.grid.shape.cols {
                let p = Placement {
                    cell: StashCell { page, x, y },
                    size,
                };
                if self.is_free(&p, None) {
                    return Some(p.cell);
                }
            }
        }
        None
    }

    /// Where an item of `size` dropped on `wanted` goes: there when free;
    /// on a category page its first free slot; else the first free cell
    /// of the item pages from that page on.
    pub fn place(&self, size: (u8, u8), wanted: Option<StashCell>) -> Option<Placement> {
        if let Some(cell) = wanted {
            let p = Placement { cell, size };
            if self.is_free(&p, None) {
                return Some(p);
            }
            if cell.page >= CATEGORY_PAGE {
                return self
                    .first_free_on(size, cell.page)
                    .map(|cell| Placement { cell, size });
            }
        }
        let first = wanted.map_or(0, |c| c.page);
        self.first_free(size, first)
            .map(|cell| Placement { cell, size })
    }

    /// The layout agreeing with the stash: `stash` the items in it,
    /// `flying` its items in flight to a character (their cells are kept
    /// for their return). Cells of other items go; stash items without a
    /// valid, free cell get the first free one, in item order. Returns the
    /// layout and whether it changed.
    pub fn repaired(&self, stash: &BTreeSet<u32>, flying: &BTreeSet<u32>) -> (Layout, bool) {
        let mut kept = Layout::on(self.grid.clone());
        let mut homeless = Vec::new();
        for (&id, p) in &self.by_item {
            if !stash.contains(&id) && !flying.contains(&id) {
                continue;
            }
            if kept.is_free(p, None) {
                kept.by_item.insert(id, *p);
            } else if stash.contains(&id) {
                homeless.push((id, p.size));
            }
        }
        for &id in stash {
            if !self.by_item.contains_key(&id) {
                homeless.push((id, LARGEST));
            }
        }
        homeless.sort_unstable();
        for (id, size) in homeless {
            // A stash past its pages (it can't happen while the item limit
            // fits them) keeps the item off the pages until room is made.
            if let Some(cell) = kept.first_free(size, 0) {
                kept.by_item.insert(id, Placement { cell, size });
            }
        }
        let changed = kept != *self;
        (kept, changed)
    }

    /// Packs page `page`: its items, largest first (then by `key`), from
    /// the top-left. Returns the items' new places.
    pub fn arranged<K: Ord>(&self, page: u8, key: impl Fn(u32) -> K) -> Vec<(u32, Placement)> {
        let mut on: Vec<(u32, Placement)> = self
            .by_item
            .iter()
            .filter(|(_, p)| p.cell.page == page)
            .map(|(id, p)| (*id, *p))
            .collect();
        let area = |p: &Placement| u16::from(p.size.0) * u16::from(p.size.1);
        on.sort_by(|a, b| {
            area(&b.1)
                .cmp(&area(&a.1))
                .then_with(|| key(a.0).cmp(&key(b.0)))
                .then(a.0.cmp(&b.0))
        });
        let mut packed = Layout {
            grid: self.grid.clone(),
            by_item: self
                .by_item
                .iter()
                .filter(|(_, p)| p.cell.page != page)
                .map(|(id, p)| (*id, *p))
                .collect(),
        };
        let mut out = Vec::new();
        for (id, p) in on {
            // Every item fitted before, so packing from the top-left finds
            // room; if not, the item keeps its cell.
            let cell = packed.first_free_on(p.size, page).unwrap_or(p.cell);
            let np = Placement { cell, size: p.size };
            packed.by_item.insert(id, np);
            out.push((id, np));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(page: u8, x: u8, y: u8, w: u8, h: u8) -> Placement {
        Placement {
            cell: StashCell { page, x, y },
            size: (w, h),
        }
    }

    #[test]
    fn places_where_dropped_or_first_free() {
        let mut l = Layout::default();
        l.by_item.insert(1, at(0, 0, 0, 2, 4));
        // Dropped on a free cell: there.
        let p = l.place(
            (1, 1),
            Some(StashCell {
                page: 0,
                x: 5,
                y: 5,
            }),
        );
        assert_eq!(p, Some(at(0, 5, 5, 1, 1)));
        // Dropped on the first item: the first free cell of that page.
        let p = l.place(
            (2, 2),
            Some(StashCell {
                page: 0,
                x: 1,
                y: 1,
            }),
        );
        assert_eq!(p, Some(at(0, 2, 0, 2, 2)));
        // Off the grid: not free.
        assert!(!l.is_free(&at(0, 7, 0, 2, 1), None));
        assert!(!l.is_free(&at(PAGES, 0, 0, 1, 1), None));
    }

    #[test]
    fn a_full_stash_has_no_place() {
        let mut l = Layout::default();
        let mut id = 0;
        for page in 0..PAGES {
            for y in 0..ROWS {
                for x in 0..COLS {
                    id += 1;
                    l.by_item.insert(id, at(page, x, y, 1, 1));
                }
            }
        }
        assert_eq!(l.place((1, 1), None), None);
    }

    #[test]
    fn repair_drops_gone_items_and_places_new_ones() {
        let mut l = Layout::default();
        l.by_item.insert(1, at(0, 0, 0, 1, 1));
        l.by_item.insert(2, at(0, 1, 0, 1, 1));
        // Overlaps item 1.
        l.by_item.insert(3, at(0, 0, 0, 1, 1));
        l.by_item.insert(4, at(0, 3, 0, 1, 1));
        let stash: BTreeSet<u32> = [1, 3, 5].into();
        let flying: BTreeSet<u32> = [4].into();
        let (r, changed) = l.repaired(&stash, &flying);
        assert!(changed);
        // 2 is gone; 4 keeps its cell while in flight; 3 and 5 are placed.
        assert_eq!(r.by_item.get(&1), Some(&at(0, 0, 0, 1, 1)));
        assert!(!r.by_item.contains_key(&2));
        assert_eq!(r.by_item.get(&4), Some(&at(0, 3, 0, 1, 1)));
        assert!(r.by_item.contains_key(&3) && r.by_item.contains_key(&5));
        let all: Vec<&Placement> = r.by_item.values().collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(!a.overlaps(b), "{a:?} {b:?}");
            }
        }
        // A repaired layout repairs to itself.
        assert!(!r.repaired(&stash, &flying).1);
    }

    #[test]
    fn arrange_packs_largest_first() {
        let mut l = Layout::default();
        l.by_item.insert(1, at(0, 7, 10, 1, 1));
        l.by_item.insert(2, at(0, 4, 4, 2, 3));
        l.by_item.insert(3, at(1, 0, 0, 1, 1));
        let out = l.arranged(0, |id| id);
        assert_eq!(out, vec![(2, at(0, 0, 0, 2, 3)), (1, at(0, 2, 0, 1, 1))]);
    }

    /// A mod realm's 14 x 14 pages and a category page of 3 slots.
    fn mod_layout() -> Layout {
        let shape = d2_shared_stash::StashShape {
            pages: 10,
            cols: 14,
            rows: 14,
            renamable: true,
        };
        Layout::on(Grid {
            shape,
            categories: vec![(14, 3), (14, 0)],
        })
    }

    #[test]
    fn a_mod_realms_shape_takes_its_cells() {
        let mut l = mod_layout();
        // Past D2Gamma's 8 x 11, inside 14 x 14.
        assert!(l.is_free(&at(9, 12, 12, 2, 2), None));
        assert!(!l.is_free(&at(9, 13, 12, 2, 2), None));
        assert!(!l.is_free(&at(10, 0, 0, 1, 1), None));
        assert!(!Layout::default().is_free(&at(0, 12, 12, 2, 2), None));
        l.by_item.insert(1, at(0, 0, 0, 14, 14));
        assert_eq!(l.place((1, 1), None), Some(at(1, 0, 0, 1, 1)));
    }

    #[test]
    fn category_slots_hold_one_item_each() {
        let mut l = mod_layout();
        // A 2 x 3 item on a one-cell slot; slot 3 is past the page's 3.
        assert!(l.is_free(&at(100, 2, 0, 2, 3), None));
        assert!(!l.is_free(&at(100, 3, 0, 1, 1), None));
        assert!(!l.is_free(&at(101, 0, 0, 1, 1), None), "no slots");
        assert!(!l.is_free(&at(102, 0, 0, 1, 1), None), "no category");
        l.by_item.insert(1, at(100, 1, 0, 2, 3));
        // Dropped on a taken slot: the category's first free one.
        let wanted = StashCell {
            page: 100,
            x: 1,
            y: 0,
        };
        assert_eq!(l.place((1, 1), Some(wanted)), Some(at(100, 0, 0, 1, 1)));
        l.by_item.insert(2, at(100, 0, 0, 1, 1));
        l.by_item.insert(3, at(100, 2, 0, 1, 1));
        // The category is full: no place, never an item page.
        assert_eq!(l.place((1, 1), Some(wanted)), None);
        // Repair keeps the slots on a grid that has them.
        let ids: BTreeSet<u32> = [1, 2, 3].into();
        assert!(!l.repaired(&ids, &BTreeSet::new()).1);
    }
}
