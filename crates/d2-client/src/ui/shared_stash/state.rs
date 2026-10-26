// D2RUST's own: what the shared stash panel shows and asks.
//! The panel's state: the shown tab and category, what the store last
//! said (the shown item page, the slots' counts, the gold, the search
//! hits), and the asks the panel queues for the app, which runs them on
//! the store ([`d2_shared_stash::StashStore`]) and gives the answer back.
//! The panel decides nothing about goods: every move is the store's.

use std::collections::BTreeMap;

use d2_shared_stash::{Catalog, Cell, ItemId, StackPage};

use super::layout::{Category, Tab};

/// An item of the shown item page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownItem {
    pub id: ItemId,
    pub cell: Cell,
    pub size: (u8, u8),
    pub code: String,
    pub name: String,
    /// The save format's quality number (2 normal ... 7 unique, 8 crafted).
    pub quality: u8,
    pub level_req: u8,
    pub sockets: u8,
    pub ethereal: bool,
    /// The item as the game client showed it (its item message stream):
    /// the picture and the tip read it.
    pub look: Option<Vec<u8>>,
    /// Its socketed items as item message streams, socket order (read
    /// from the record it is kept with): the tip lists them.
    pub fillers: Vec<Vec<u8>>,
}

/// What the panel asks of the store. Item IDs of the character are the
/// item GUIDs the client knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StashAsk {
    /// Read the shown page again (a tab or category changed, the panel
    /// opened).
    Refresh,
    /// An item of the character (on the cursor or in the inventory) onto
    /// item page `page` at `at` (taken or `None`: the first free cell from
    /// that page on).
    PutItem {
        guid: u32,
        page: u8,
        at: Option<(u8, u8)>,
    },
    /// An item of the character (on the cursor or in the inventory) to its
    /// stackable slot.
    PutStack {
        guid: u32,
    },
    /// A shared item onto the cursor (or into the inventory).
    TakeItem {
        id: ItemId,
        to_cursor: bool,
    },
    /// `count` of a slot as single items into the inventory (as many as
    /// fit).
    TakeStack {
        code: String,
        count: u32,
    },
    /// Pack an item page.
    Arrange {
        page: u8,
    },
    /// All the character's gold in (`true`) or as much as fits out.
    Gold {
        put: bool,
    },
    Search {
        text: String,
    },
    /// The inventory's "Send stackables to stash": these inventory items
    /// (the store moves only the stackables of a listed kind).
    SendStackables {
        guids: Vec<u32>,
    },
}

/// A press on the Market category's item page, for the market's sell slot
/// (`crate::ui::market::sell`), which the panel hands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarketPick {
    /// A shared item, with Ctrl held (straight onto the sell slot) or not
    /// (picked up to drop on it).
    Item { id: ItemId, ctrl: bool },
    /// An item of the character's on the cursor: not the stash's.
    Cursor,
    /// An empty cell.
    Empty,
}

/// The "how many" box of Shift+Left on a slot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CountBox {
    pub code: String,
    pub text: String,
}

/// The shared stash panel's state.
#[derive(Debug, Default)]
pub struct SharedStashUi {
    /// A store is there (a saved single-player game, or online).
    pub on: bool,
    pub catalog: Option<Catalog>,
    pub tab: ShownTab,
    pub category: ShownCategory,
    pub items: Vec<ShownItem>,
    pub stacks: BTreeMap<String, u32>,
    pub gold: u64,
    pub search: String,
    pub search_focus: bool,
    pub hits: Vec<(ItemId, Cell)>,
    pub count_box: Option<CountBox>,
    /// The inventory's send button is held down (pressed, not yet
    /// released).
    pub send_pressed: bool,
    /// The last press on the Market category's page, for the sell slot.
    pub market_pick: Option<MarketPick>,
    /// The asks since the app last took them, in order.
    pub asks: Vec<StashAsk>,
}

/// [`Tab`] with a default (P).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShownTab(pub Tab);

impl Default for ShownTab {
    fn default() -> Self {
        ShownTab(Tab::Personal)
    }
}

/// [`Category`] with a default (Stash).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShownCategory(pub Category);

impl Default for ShownCategory {
    fn default() -> Self {
        ShownCategory(Category::Stash)
    }
}

/// What the page area shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// The game's own stash (tab P with category Stash).
    Personal,
    Page(u8),
    Stacks(StackPage),
}

impl SharedStashUi {
    pub fn shown(&self) -> Shown {
        match (self.category.0, self.tab.0) {
            (Category::Stacks(p), _) => Shown::Stacks(p),
            (Category::Stash, Tab::Personal) => Shown::Personal,
            (Category::Stash | Category::Market, Tab::Shared(p)) => Shown::Page(p),
            // Selling lists shared items only: tab P shows page I.
            (Category::Market, Tab::Personal) => Shown::Page(0),
        }
    }

    /// The Market category is shown: the item page sells.
    pub fn selling(&self) -> bool {
        self.on && self.category.0 == Category::Market
    }

    /// The shared stash covers the game's panel (a shared or stackable page
    /// is shown).
    pub fn covers(&self) -> bool {
        self.on && self.shown() != Shown::Personal
    }

    /// The item page the panel's moves go to: the shown one, else I.
    pub fn page(&self) -> u8 {
        match self.tab.0 {
            Tab::Shared(p) => p,
            Tab::Personal => 0,
        }
    }

    pub fn ask(&mut self, a: StashAsk) {
        self.asks.push(a);
    }

    pub fn take_asks(&mut self) -> Vec<StashAsk> {
        std::mem::take(&mut self.asks)
    }

    /// Whether item `id` is a search hit (a search is set and matched it).
    pub fn hit(&self, id: ItemId) -> bool {
        self.hits.iter().any(|(h, _)| *h == id)
    }
}
