//! The stackable pages' fixed slots: one slot per stackable kind, on the
//! Runes, Gems or Misc page, each holding a count.
//!
//! The base game's list is every rune (El to Zod), every gem by type and
//! quality, the healing, mana and rejuvenation potions and the other small
//! potions, keys, the Hellfire keys, essences, the Token of Absolution and
//! the Hellfire organs. A mod adds its own currencies from its data
//! ([`Catalog::with_extra`]); they go to the end of the Misc page.
//!
//! "Affinity": a stackable dropped anywhere on a stackable page goes to
//! its own slot ([`Catalog::slot`]).
//!
//! A mod game may add its own named category pages
//! ([`Catalog::with_categories`]): each lists its counter slots in order
//! and has a number of whole-item slots on the same page (items kept
//! whole, one per slot, see [`crate::cells::Grid`]), and may raise or
//! lower the count one slot holds ([`Catalog::with_cap`]).

use d2_economy::Stackable;

/// The highest count one slot holds by default ([`Catalog::cap`]). A put
/// past the cap is refused ([`d2_economy::Refusal::StashFull`]) and the
/// items stay on the character.
pub const STACK_CAP: u32 = 9_999;

/// The page a slot is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StackPage {
    Runes,
    Gems,
    Misc,
    /// A mod game's category page, by its index in
    /// [`Catalog::categories`].
    Category(u8),
}

impl StackPage {
    pub const ALL: [StackPage; 3] = [StackPage::Runes, StackPage::Gems, StackPage::Misc];

    /// The page's grid width in slots; a category page's is its
    /// catalog's ([`Catalog::cols`]), 8 here.
    pub fn cols(self) -> u8 {
        match self {
            StackPage::Runes | StackPage::Misc | StackPage::Category(_) => 8,
            // One column per gem type.
            StackPage::Gems => 7,
        }
    }
}

/// A mod game's category page: its name, its counter slots in order and
/// how many whole items it also holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryPage {
    pub name: String,
    /// Slots per row.
    pub cols: u8,
    /// The counters, in page order. A code already in the catalog keeps
    /// its first slot.
    pub codes: Vec<Stackable>,
    /// Whole-item slots on the page (0: counters only).
    pub item_slots: u16,
}

/// A category page as the catalog keeps it (its counters are slots).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CategoryInfo {
    pub name: String,
    pub cols: u8,
    pub item_slots: u16,
}

/// One fixed slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    pub code: String,
    pub name: String,
    pub page: StackPage,
    /// Column and row on its page.
    pub at: (u8, u8),
}

/// The runes, El (`r01`) to Zod (`r33`).
const RUNES: u8 = 33;

/// The gem kinds by column, each with its five codes from chipped to
/// perfect (one row per quality).
const GEMS: [[&str; 5]; 7] = [
    ["gcv", "gfv", "gsv", "gzv", "gpv"],
    ["gcy", "gfy", "gsy", "gly", "gpy"],
    ["gcb", "gfb", "gsb", "glb", "gpb"],
    ["gcg", "gfg", "gsg", "glg", "gpg"],
    ["gcr", "gfr", "gsr", "glr", "gpr"],
    ["gcw", "gfw", "gsw", "glw", "gpw"],
    ["skc", "skf", "sku", "skl", "skz"],
];

/// The Misc page in order: healing and mana potions by size, the
/// rejuvenation potions, the antidote, stamina and thawing potions, keys,
/// the three Hellfire keys, the four essences, the Token of Absolution
/// and the three Hellfire organs.
const MISC: [&str; 27] = [
    "hp1", "hp2", "hp3", "hp4", "hp5", "mp1", "mp2", "mp3", "mp4", "mp5", "rvs", "rvl", "yps",
    "vps", "wms", "key", "pk1", "pk2", "pk3", "tes", "ceh", "bet", "fed", "toa", "mbr", "dhn",
    "bey",
];

/// Every slot of a game, in page order, and the count one slot holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    slots: Vec<Slot>,
    cap: u32,
    categories: Vec<CategoryInfo>,
}

impl Catalog {
    /// The base game's slots, named by `name` (the game's item names; the
    /// code when it has none).
    pub fn base(name: impl Fn(&str) -> Option<String>) -> Catalog {
        let mut slots = Vec::new();
        let mut push = |code: String, page: StackPage, at: (u8, u8)| {
            let n = name(&code).unwrap_or_else(|| code.clone());
            slots.push(Slot {
                code,
                name: n,
                page,
                at,
            });
        };
        for i in 0..RUNES {
            let cols = StackPage::Runes.cols();
            push(
                format!("r{:02}", i + 1),
                StackPage::Runes,
                (i % cols, i / cols),
            );
        }
        for (col, codes) in GEMS.iter().enumerate() {
            for (row, code) in codes.iter().enumerate() {
                push((*code).to_owned(), StackPage::Gems, (col as u8, row as u8));
            }
        }
        for (i, code) in MISC.iter().enumerate() {
            let cols = StackPage::Misc.cols();
            let i = i as u8;
            push((*code).to_owned(), StackPage::Misc, (i % cols, i / cols));
        }
        Catalog {
            slots,
            cap: STACK_CAP,
            categories: Vec::new(),
        }
    }

    /// The highest count one slot holds ([`STACK_CAP`] unless a mod game
    /// sets its own).
    pub fn cap(&self) -> u32 {
        self.cap
    }

    /// The same catalog with `cap` as the highest count of one slot.
    pub fn with_cap(mut self, cap: u32) -> Catalog {
        self.cap = cap;
        self
    }

    /// A mod game's category pages after the catalog's pages, as
    /// [`StackPage::Category`] 0, 1, ... in order. A code already in the
    /// catalog keeps its slot. At most 155 categories are kept (their
    /// whole items use page ids 100 to 254).
    pub fn with_categories(mut self, pages: Vec<CategoryPage>) -> Catalog {
        for page in pages {
            let Ok(c) = u8::try_from(self.categories.len()) else {
                break;
            };
            if c >= u8::MAX - crate::cells::CATEGORY_PAGE {
                break;
            }
            let cols = page.cols.max(1);
            let mut i: u16 = 0;
            for s in page.codes {
                if self.slot(&s.code).is_some() {
                    continue;
                }
                let at = (
                    (i % u16::from(cols)) as u8,
                    u8::try_from(i / u16::from(cols)).unwrap_or(u8::MAX),
                );
                self.slots.push(Slot {
                    code: s.code,
                    name: s.name,
                    page: StackPage::Category(c),
                    at,
                });
                i += 1;
            }
            self.categories.push(CategoryInfo {
                name: page.name,
                cols,
                item_slots: page.item_slots,
            });
        }
        self
    }

    /// The mod game's category pages ([`StackPage::Category`] by index).
    pub fn categories(&self) -> &[CategoryInfo] {
        &self.categories
    }

    /// Every stackable page: Runes, Gems, Misc, then the categories.
    pub fn pages(&self) -> Vec<StackPage> {
        let n = self.categories.len() as u8;
        StackPage::ALL
            .into_iter()
            .chain((0..n).map(StackPage::Category))
            .collect()
    }

    /// The page's grid width in slots.
    pub fn cols(&self, page: StackPage) -> u8 {
        match page {
            StackPage::Category(c) => self
                .categories
                .get(usize::from(c))
                .map_or(page.cols(), |i| i.cols),
            _ => page.cols(),
        }
    }

    /// The page's name: Runes, Gems, Misc, or the category's.
    pub fn page_name(&self, page: StackPage) -> &str {
        match page {
            StackPage::Runes => "Runes",
            StackPage::Gems => "Gems",
            StackPage::Misc => "Misc",
            StackPage::Category(c) => self
                .categories
                .get(usize::from(c))
                .map_or("", |i| i.name.as_str()),
        }
    }

    /// A mod's own currencies after the base list, on the Misc page. A
    /// code already listed keeps its slot.
    pub fn with_extra(mut self, extra: &[Stackable]) -> Catalog {
        let cols = StackPage::Misc.cols();
        for s in extra {
            if self.slot(&s.code).is_some() {
                continue;
            }
            let i = self
                .slots
                .iter()
                .filter(|s| s.page == StackPage::Misc)
                .count() as u8;
            self.slots.push(Slot {
                code: s.code.clone(),
                name: s.name.clone(),
                page: StackPage::Misc,
                at: (i % cols, i / cols),
            });
        }
        self
    }

    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// The slots of one page.
    pub fn page(&self, page: StackPage) -> impl Iterator<Item = &Slot> {
        self.slots.iter().filter(move |s| s.page == page)
    }

    /// The slot of `code`: where a stackable of that kind goes.
    pub fn slot(&self, code: &str) -> Option<&Slot> {
        self.slots.iter().find(|s| s.code == code)
    }

    /// The slot at a column and row of a page.
    pub fn at(&self, page: StackPage, at: (u8, u8)) -> Option<&Slot> {
        self.slots.iter().find(|s| s.page == page && s.at == at)
    }

    /// The economy's `stackables` list.
    pub fn stackables(&self) -> Vec<Stackable> {
        self.slots
            .iter()
            .map(|s| Stackable::new(s.code.clone(), s.name.clone()))
            .collect()
    }
}
