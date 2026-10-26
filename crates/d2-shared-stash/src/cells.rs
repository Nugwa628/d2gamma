//! Where items lie on the item pages: the game's [`StashShape`] (D2Gamma:
//! [`PAGES`] grids of [`COLS`] x [`ROWS`] cells), one rectangle per item,
//! no two overlapping. A mod game's category pages
//! ([`crate::StackPage::Category`]) also hold whole items, one per slot,
//! at page ids from [`CATEGORY_PAGE`] on ([`Grid`]).
//!
//! The economy stores which items are in the stash; the cells are kept
//! beside it ([`Cells`]) and are only the layout. When the two disagree
//! (a crash between a move and its cell write), [`Cells::repair`] places
//! every item without a valid cell on the first free cells and drops the
//! cells of items no longer in the stash, so a crash can move an item to
//! another cell but never loses it.

use std::collections::{BTreeMap, BTreeSet};

use d2_economy::rusqlite::{params, Connection};

use crate::StoreError;
use d2_economy::ItemId;

/// Shared item pages, I to X (D2Gamma's shape, [`StashShape::DEFAULT`]).
pub const PAGES: u8 = 10;
/// Columns of one page (D2Gamma's shape).
pub const COLS: u8 = 8;
/// Rows of one page (D2Gamma's shape).
pub const ROWS: u8 = 11;

/// The longest page name, in characters ([`StashShape::renamable`]).
pub const PAGE_NAME_MAX: usize = 12;

/// The first page id of the category pages' whole-item slots: category
/// `c`'s slots are on page `CATEGORY_PAGE + c`.
pub const CATEGORY_PAGE: u8 = 100;

/// The largest item size, for an item whose size was lost.
const LARGEST: (u8, u8) = (2, 4);

/// The item pages of a game's shared stash: how many, their grid, and
/// whether the player may name them. A mod game sets its own
/// ([`crate::LocalStash::open_with`]); D2Gamma's is [`StashShape::DEFAULT`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StashShape {
    /// Item pages (at most [`CATEGORY_PAGE`]).
    pub pages: u8,
    /// Columns of one page.
    pub cols: u8,
    /// Rows of one page.
    pub rows: u8,
    /// The player may name the pages (up to [`PAGE_NAME_MAX`] characters).
    pub renamable: bool,
}

impl StashShape {
    /// D2Gamma's stash: 10 pages of 8 x 11 cells, named I to X.
    pub const DEFAULT: StashShape = StashShape {
        pages: PAGES,
        cols: COLS,
        rows: ROWS,
        renamable: false,
    };

    /// The default name of page `page` (0 is I): its roman numeral.
    pub fn default_name(page: u8) -> String {
        roman(u32::from(page) + 1)
    }
}

impl Default for StashShape {
    fn default() -> StashShape {
        StashShape::DEFAULT
    }
}

fn roman(mut n: u32) -> String {
    const DIGITS: [(u32, &str); 9] = [
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (v, d) in DIGITS {
        while n >= v {
            out.push_str(d);
            n -= v;
        }
    }
    out
}

/// A top-left cell on an item page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Cell {
    /// 0 is page I.
    pub page: u8,
    pub x: u8,
    pub y: u8,
}

impl Cell {
    pub fn new(page: u8, x: u8, y: u8) -> Cell {
        Cell { page, x, y }
    }

    /// Whether an item of `size` fits on D2Gamma's page from this cell.
    pub fn fits(self, size: (u8, u8)) -> bool {
        self.fits_in(size, &StashShape::DEFAULT)
    }

    /// Whether an item of `size` fits on an item page of `shape` from
    /// this cell.
    pub fn fits_in(self, size: (u8, u8), shape: &StashShape) -> bool {
        self.page < shape.pages.min(CATEGORY_PAGE)
            && size.0 > 0
            && size.1 > 0
            && u16::from(self.x) + u16::from(size.0) <= u16::from(shape.cols)
            && u16::from(self.y) + u16::from(size.1) <= u16::from(shape.rows)
    }

    /// The category of a category page cell ([`CATEGORY_PAGE`]).
    pub fn category(self) -> Option<u8> {
        self.page.checked_sub(CATEGORY_PAGE)
    }
}

/// Where items may lie: the item pages of the shape, and the whole-item
/// slots of the catalog's category pages (`(cols, slots)` per category).
/// A slot holds one item whatever its size: slot `i` of category `c` is
/// the cell `(CATEGORY_PAGE + c, i % cols, i / cols)`. The slot index is
/// the item's order among the category's whole items; where the panel
/// draws it (after the counters) is the panel's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grid {
    pub shape: StashShape,
    pub categories: Vec<(u8, u16)>,
}

impl Grid {
    /// The item pages of `shape` and the category slots of `catalog`.
    pub fn new(shape: StashShape, catalog: &crate::Catalog) -> Grid {
        Grid {
            shape,
            categories: catalog
                .categories()
                .iter()
                .map(|c| (c.cols.max(1), c.item_slots))
                .collect(),
        }
    }

    /// The category and slot index of a category page cell; `None` for
    /// an item page cell or a cell past the category's slots.
    pub fn slot_of(&self, cell: Cell) -> Option<(u8, u16)> {
        let c = cell.category()?;
        let &(cols, slots) = self.categories.get(usize::from(c))?;
        if cell.x >= cols {
            return None;
        }
        let i = u16::from(cell.y) * u16::from(cols) + u16::from(cell.x);
        (i < slots).then_some((c, i))
    }

    /// The cell of slot `i` of category `c`.
    pub fn slot_cell(&self, c: u8, i: u16) -> Option<Cell> {
        let &(cols, slots) = self.categories.get(usize::from(c))?;
        let page = CATEGORY_PAGE.checked_add(c)?;
        let y = u8::try_from(i / u16::from(cols)).ok()?;
        (i < slots).then(|| Cell::new(page, (i % u16::from(cols)) as u8, y))
    }

    /// Whether `p` lies on a page: inside an item page's grid, or on a
    /// category slot.
    pub fn holds(&self, p: &Placement) -> bool {
        if p.cell.page >= CATEGORY_PAGE {
            return p.size.0 > 0 && p.size.1 > 0 && self.slot_of(p.cell).is_some();
        }
        p.cell.fits_in(p.size, &self.shape)
    }
}

/// One placed item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub cell: Cell,
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

/// The cells table, on the stash file's own connection.
pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS local_cells(
    item_id INTEGER PRIMARY KEY,
    page INTEGER NOT NULL,
    x INTEGER NOT NULL,
    y INTEGER NOT NULL,
    w INTEGER NOT NULL,
    h INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS local_looks(item_id INTEGER PRIMARY KEY, look BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS local_page_names(page INTEGER PRIMARY KEY, name TEXT NOT NULL);
";

/// Every item's cell, read whole (a stash holds at most a few hundred).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Cells {
    pub by_item: BTreeMap<u32, Placement>,
}

impl Cells {
    pub fn load(conn: &Connection) -> Result<Cells, StoreError> {
        let mut st = conn.prepare("SELECT item_id, page, x, y, w, h FROM local_cells")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                Placement {
                    cell: Cell::new(r.get(1)?, r.get(2)?, r.get(3)?),
                    size: (r.get(4)?, r.get(5)?),
                },
            ))
        })?;
        Ok(Cells {
            by_item: rows.collect::<Result<_, _>>()?,
        })
    }

    pub fn set(conn: &Connection, id: ItemId, p: Placement) -> Result<(), StoreError> {
        conn.execute(
            "INSERT INTO local_cells(item_id, page, x, y, w, h) VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(item_id) DO UPDATE SET page = excluded.page, x = excluded.x, \
             y = excluded.y, w = excluded.w, h = excluded.h",
            params![id.0, p.cell.page, p.cell.x, p.cell.y, p.size.0, p.size.1],
        )?;
        Ok(())
    }

    pub fn remove(conn: &Connection, id: ItemId) -> Result<(), StoreError> {
        conn.execute("DELETE FROM local_cells WHERE item_id = ?1", [id.0])?;
        conn.execute("DELETE FROM local_looks WHERE item_id = ?1", [id.0])?;
        Ok(())
    }

    /// Keeps how the game client showed item `id` ([`crate::PageItem::look`]).
    pub fn set_look(conn: &Connection, id: ItemId, look: &[u8]) -> Result<(), StoreError> {
        conn.execute(
            "INSERT OR REPLACE INTO local_looks(item_id, look) VALUES (?1, ?2)",
            d2_economy::rusqlite::params![id.0, look],
        )?;
        Ok(())
    }

    /// Every kept look, by item.
    pub fn looks(conn: &Connection) -> Result<BTreeMap<u32, Vec<u8>>, StoreError> {
        let mut st = conn.prepare("SELECT item_id, look FROM local_looks")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The names the player gave pages, by page.
    pub fn page_names(conn: &Connection) -> Result<BTreeMap<u8, String>, StoreError> {
        let mut st = conn.prepare("SELECT page, name FROM local_page_names")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Names page `page`; an empty name gives it back its default name.
    pub fn set_page_name(conn: &Connection, page: u8, name: &str) -> Result<(), StoreError> {
        if name.is_empty() {
            conn.execute("DELETE FROM local_page_names WHERE page = ?1", [page])?;
        } else {
            conn.execute(
                "INSERT OR REPLACE INTO local_page_names(page, name) VALUES (?1, ?2)",
                params![page, name],
            )?;
        }
        Ok(())
    }

    /// Whether `p` is on the pages of `grid` and free of every item but
    /// `except`.
    pub fn is_free(&self, grid: &Grid, p: &Placement, except: Option<ItemId>) -> bool {
        grid.holds(p)
            && self
                .by_item
                .iter()
                .filter(|(id, _)| Some(ItemId(**id)) != except)
                .all(|(_, o)| !p.overlaps(o))
    }

    /// The first free place for `size` on the item pages: page by page
    /// from `first`, then the pages before it; on each page row by row,
    /// left to right (the game's own order).
    pub fn first_free(&self, grid: &Grid, size: (u8, u8), first: u8) -> Option<Cell> {
        let pages = grid.shape.pages.min(CATEGORY_PAGE);
        if pages == 0 {
            return None;
        }
        (0..pages)
            .map(|i| ((u16::from(first % pages) + u16::from(i)) % u16::from(pages)) as u8)
            .find_map(|page| self.first_free_on(grid, size, page))
    }

    /// The first free place for `size` on page `page`; on a category page
    /// its first free whole-item slot.
    pub fn first_free_on(&self, grid: &Grid, size: (u8, u8), page: u8) -> Option<Cell> {
        if let Some(c) = page.checked_sub(CATEGORY_PAGE) {
            let &(_, slots) = grid.categories.get(usize::from(c))?;
            return (0..slots).find_map(|i| {
                let cell = grid.slot_cell(c, i)?;
                self.is_free(grid, &Placement { cell, size }, None)
                    .then_some(cell)
            });
        }
        for y in 0..grid.shape.rows {
            for x in 0..grid.shape.cols {
                let p = Placement {
                    cell: Cell::new(page, x, y),
                    size,
                };
                if self.is_free(grid, &p, None) {
                    return Some(p.cell);
                }
            }
        }
        None
    }

    /// Makes the cells agree with the stash's items (`ids`: every item in
    /// the stash): cells of items gone are dropped (a move that never
    /// happened, or a take), and items without a valid, free cell on
    /// `grid` get the first free one on the item pages, in item order. An
    /// item's cell is written before the move that brings it in, so its
    /// size is known; one without any (a file changed by hand) is given
    /// the largest size. Returns the items with no room left anywhere.
    pub fn repair(
        conn: &Connection,
        grid: &Grid,
        ids: &BTreeSet<u32>,
    ) -> Result<Vec<ItemId>, StoreError> {
        let old = Cells::load(conn)?;
        let mut kept = Cells::default();
        let mut homeless = Vec::new();
        for &id in ids {
            match old.by_item.get(&id) {
                Some(p) if kept.is_free(grid, p, None) => {
                    kept.by_item.insert(id, *p);
                }
                Some(p) => homeless.push((id, p.size)),
                None => homeless.push((id, LARGEST)),
            }
        }
        let mut full = Vec::new();
        for (id, size) in homeless {
            match kept.first_free(grid, size, 0) {
                Some(cell) => {
                    kept.by_item.insert(id, Placement { cell, size });
                }
                None => full.push(ItemId(id)),
            }
        }
        if kept != old {
            let tx = conn.unchecked_transaction()?;
            tx.execute("DELETE FROM local_cells", [])?;
            for (&id, &p) in &kept.by_item {
                Cells::set(&tx, ItemId(id), p)?;
            }
            tx.commit()?;
        }
        conn.execute(
            "DELETE FROM local_looks WHERE item_id NOT IN (SELECT item_id FROM local_cells)",
            [],
        )?;
        Ok(full)
    }
}
