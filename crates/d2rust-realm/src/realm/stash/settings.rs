//! The online stash's settings: its item pages' shape, the highest count
//! of one stackable slot, and the category pages whose whole-item slots
//! the stash keeps (page ids from [`CATEGORY_PAGE`] on). D2Gamma's are the
//! defaults; a mod realm (its `game` key) sets its own in its config file:
//!
//! ```text
//! stash_pages     = 10               # item pages (default 10, at most 64)
//! stash_cols      = 14               # columns of a page (default 8)
//! stash_rows      = 14               # rows of a page (default 11)
//! stash_renamable = on               # players name the pages (default off)
//! stash_cap       = 9999999          # one slot's highest count (default 9999)
//! stash_category  = Currency;14;140  # a category page: name; columns;
//! stash_category  = Gems;14;0        #   whole-item slots (one line each)
//! ```
//!
//! A category page's counters are the realm's `stackables`; the stash only
//! needs each page's whole-item slots, in the order the game lists them.

use d2_net::msg::stash::{StashCell, MAX_PAGE_NAMES};
use d2_shared_stash::{CategoryInfo, Cell, Grid, StashShape, CATEGORY_PAGE, STACK_CAP};

/// One realm's stash settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StashSettings {
    /// The item pages.
    pub shape: StashShape,
    /// The highest count of one stackable slot.
    pub cap: u32,
    /// The category pages, in the game's order.
    pub categories: Vec<CategoryInfo>,
}

impl Default for StashSettings {
    fn default() -> StashSettings {
        StashSettings {
            shape: StashShape::DEFAULT,
            cap: STACK_CAP,
            categories: Vec::new(),
        }
    }
}

fn number<T: std::str::FromStr + PartialOrd + std::fmt::Display>(
    value: &str,
    range: std::ops::RangeInclusive<T>,
) -> Result<T, String> {
    value
        .trim()
        .parse::<T>()
        .ok()
        .filter(|v| range.contains(v))
        .ok_or_else(|| format!("a number in {}..={}", range.start(), range.end()))
}

fn name(value: &str, max: usize) -> Result<String, String> {
    let v = value.trim();
    if v.is_empty() || v.len() > max || v.chars().any(char::is_control) {
        return Err(format!("1 to {max} printable bytes"));
    }
    Ok(v.to_owned())
}

impl StashSettings {
    /// The keys read once each.
    pub const KEYS: &'static [&'static str] = &[
        "stash_pages",
        "stash_cols",
        "stash_rows",
        "stash_renamable",
        "stash_cap",
    ];
    /// The key given once per category page.
    pub const CATEGORY_KEY: &'static str = "stash_category";

    /// Sets `key` (one of [`StashSettings::KEYS`] or
    /// [`StashSettings::CATEGORY_KEY`]) from its config value.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "stash_pages" => self.shape.pages = number(value, 1..=MAX_PAGE_NAMES as u8)?,
            "stash_cols" => self.shape.cols = number(value, 1..=16u8)?,
            "stash_rows" => self.shape.rows = number(value, 1..=16u8)?,
            "stash_renamable" => {
                self.shape.renamable = match value.trim() {
                    "on" => true,
                    "off" => false,
                    _ => return Err("`on` or `off`".into()),
                }
            }
            "stash_cap" => self.cap = number(value, 1..=u32::MAX)?,
            "stash_category" => {
                let parts: Vec<&str> = value.split(';').collect();
                let [n, cols, slots] = parts[..] else {
                    return Err("NAME; COLUMNS; WHOLE-ITEM SLOTS".into());
                };
                if self.categories.len() >= usize::from(u8::MAX - CATEGORY_PAGE) {
                    return Err("too many category pages".into());
                }
                self.categories.push(CategoryInfo {
                    name: name(n, 32)?,
                    cols: number(cols, 1..=255u8)?,
                    item_slots: number(slots, 0..=4096u16)?,
                });
            }
            _ => return Err(format!("no stash key `{key}`")),
        }
        Ok(())
    }

    /// Where items may lie: the item pages and the category slots.
    pub fn grid(&self) -> Grid {
        Grid {
            shape: self.shape,
            categories: self
                .categories
                .iter()
                .map(|c| (c.cols.max(1), c.item_slots))
                .collect(),
        }
    }

    /// Whether `page` is an item page or a category page with whole-item
    /// slots (the pages `StashOpen` lists items of).
    pub fn holds_page(&self, page: u16) -> bool {
        let Ok(page) = u8::try_from(page) else {
            return false;
        };
        match page.checked_sub(CATEGORY_PAGE) {
            Some(c) => self
                .categories
                .get(usize::from(c))
                .is_some_and(|c| c.item_slots > 0),
            None => page < self.shape.pages,
        }
    }
}

/// The stash's cell as the shared stash's.
pub fn cell(c: StashCell) -> Cell {
    Cell::new(c.page, c.x, c.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d2gammas_settings_are_the_defaults() {
        let s = StashSettings::default();
        assert_eq!(s.shape, StashShape::DEFAULT);
        assert_eq!((s.shape.pages, s.shape.cols, s.shape.rows), (10, 8, 11));
        assert_eq!(s.cap, 9_999);
        assert!(s.categories.is_empty());
        assert!(s.holds_page(9) && !s.holds_page(10) && !s.holds_page(100));
    }

    #[test]
    fn a_mod_realm_reads_its_own() {
        let mut s = StashSettings::default();
        for (k, v) in [
            ("stash_pages", "10"),
            ("stash_cols", "14"),
            ("stash_rows", "14"),
            ("stash_renamable", "on"),
            ("stash_cap", "9999999"),
            ("stash_category", "Currency; 14; 140"),
            ("stash_category", "Runes;14;0"),
        ] {
            s.set(k, v).unwrap();
        }
        assert_eq!(
            s.shape,
            StashShape {
                pages: 10,
                cols: 14,
                rows: 14,
                renamable: true
            }
        );
        assert_eq!(s.cap, 9_999_999);
        assert_eq!(s.categories.len(), 2);
        assert_eq!(s.grid().categories, vec![(14, 140), (14, 0)]);
        assert!(s.holds_page(100) && !s.holds_page(101) && !s.holds_page(102));
        for (k, v) in [
            ("game", "mymod"),
            ("stash_pages", "0"),
            ("stash_pages", "65"),
            ("stash_cols", "17"),
            ("stash_renamable", "yes"),
            ("stash_cap", "0"),
            ("stash_category", "Currency;14"),
            ("stash_category", "Currency;0;3"),
        ] {
            assert!(s.set(k, v).is_err(), "{k} = {v}");
        }
    }
}
