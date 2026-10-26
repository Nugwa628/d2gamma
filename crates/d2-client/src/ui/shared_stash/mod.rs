// D2RUST's own: the shared stash on the stash panel.
//! The shared stash (multiplayer v2 plan E1, modules N28a-c; single player
//! too) drawn on the game's stash panel (ui 0x19):
//! - page tabs across the top: P, the character's own stash (the game's
//!   panel, unchanged), then the shared item pages I..X;
//! - category buttons down the left edge: Stash (the tabs' pages), Runes,
//!   Gems, Misc (the stackable pages: one fixed slot per kind, with its
//!   count, greyed at 0);
//! - at the bottom: the shared gold, the search box and "Sort".
//!
//! The panel decides nothing about goods: every press becomes a
//! [`StashAsk`] ([`input`]) that the app runs on the store
//! ([`d2_shared_stash::StashStore`]) and answers with a [`StashView`].
//! The parts are laid out in the panel's own pixels ([`layout`]), so they
//! follow the panel art in every frame size and UI Scale.

pub mod draw;
pub mod input;
pub mod layout;
pub mod state;

use std::collections::BTreeMap;

use d2_shared_stash::{Cell, ItemId, PageItem};

use crate::ui::item_tip::ItemTips;

pub use draw::Kit;
pub use input::{key, press, Mods};
pub use layout::{Category, Places, Spot, Tab, MARKET_SLOT};
pub use state::{CountBox, MarketPick, SharedStashUi, Shown, ShownItem, StashAsk};

/// What the store said after an ask: the shown item page, the slots'
/// counts, the shared gold and the search's hits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StashView {
    pub items: Vec<PageItem>,
    pub stacks: BTreeMap<String, u32>,
    pub gold: u64,
    pub hits: Vec<(ItemId, Cell)>,
}

impl SharedStashUi {
    /// Shows the store's answer. `tips`: the item tips, which read the
    /// socket fillers out of the items' records.
    pub fn apply(&mut self, v: StashView, tips: Option<&ItemTips>) {
        self.items = v.items.into_iter().map(|p| shown_item(p, tips)).collect();
        self.stacks = v.stacks;
        self.gold = v.gold;
        self.hits = v.hits;
    }

    /// The item page to read for the view: the shown one, else none.
    pub fn view_page(&self) -> Option<u8> {
        match self.shown() {
            Shown::Page(p) => Some(p),
            _ => None,
        }
    }

    /// Typing goes to the panel (the search box or the "how many" box):
    /// the game's key bindings wait.
    pub fn text_focus(&self) -> bool {
        self.on && (self.search_focus || self.count_box.is_some())
    }
}

fn shown_item(p: PageItem, tips: Option<&ItemTips>) -> ShownItem {
    let code = p.facts.as_ref().map(|f| f.code.clone()).unwrap_or_else(|| {
        p.look
            .as_deref()
            .and_then(crate::bridge::items::peek)
            .and_then(|h| h.6)
            .map(|c| String::from_utf8_lossy(&c).trim_end().to_owned())
            .unwrap_or_default()
    });
    let f = p.facts.unwrap_or_default();
    ShownItem {
        id: p.id,
        cell: p.cell,
        size: p.size,
        name: if f.name.is_empty() {
            code.clone()
        } else {
            f.name
        },
        code,
        quality: f.quality,
        level_req: f.level_req,
        sockets: f.sockets,
        ethereal: f.ethereal,
        fillers: tips.map(|t| t.filler_looks(&p.record)).unwrap_or_default(),
        look: p.look,
    }
}
