// D2RUST's own: what the shared stash holds, for the market window.
//! The offer form offers what the account's shared stash holds
//! ([`super::source::MarketSource::holdings`]). Online the realm's market
//! sends no holdings: they come from the online stash's own answers (plan
//! N28, `app/online_stash.rs`), which note each page, the slots and the
//! gold they read here. A market change that moves stash goods (a post, a
//! buy, an offer, a claim) marks the book stale and the stash reads it
//! again.
//!
//! The items are those of the pages read so far (the panel reads the page
//! it shows): an item on a page not opened yet is not offered.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::source::{Goods, Group, ItemView};
use super::trade::{Holdings, NamedItem};
use super::wire::Reading;

/// The stash as its last answers read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StashBook {
    gold: u64,
    stacks: BTreeMap<String, u32>,
    /// Item IDs and records by page.
    pages: BTreeMap<u8, Vec<(u32, Vec<u8>)>>,
    /// An answer came.
    known: bool,
    /// A market change moved goods since.
    stale: bool,
}

/// The book the online stash writes and the realm source reads.
pub type SharedBook = Arc<Mutex<StashBook>>;

impl StashBook {
    /// The stash's answer: its gold and slots, and page `page`'s items
    /// (`None`: no page was read).
    pub fn saw(
        &mut self,
        page: Option<u8>,
        gold: u64,
        stacks: BTreeMap<String, u32>,
        items: Vec<(u32, Vec<u8>)>,
    ) {
        self.gold = gold;
        self.stacks = stacks;
        if let Some(p) = page {
            self.pages.insert(p, items);
        }
        self.known = true;
        self.stale = false;
    }

    /// A market change moved stash goods: the book wants reading again.
    pub fn touch(&mut self) {
        self.stale = true;
    }

    /// Whether the book wants reading, once.
    pub fn take_stale(&mut self) -> bool {
        std::mem::take(&mut self.stale)
    }

    /// The holdings with each item read; `None` before the stash answered.
    pub fn holdings(&self, read: &Reading) -> Option<Holdings> {
        if !self.known {
            return None;
        }
        let items = self
            .pages
            .values()
            .flatten()
            .map(|(id, record)| {
                let goods = Goods::Item {
                    item_id: *id,
                    record: record.clone(),
                };
                let (group, item) = read
                    .goods(&goods)
                    .unwrap_or((Group::Other, ItemView::default()));
                NamedItem {
                    item_id: *id,
                    record: record.clone(),
                    group,
                    item,
                }
            })
            .collect();
        Some(Holdings {
            gold: self.gold,
            stacks: self.stacks.clone(),
            items,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_one(record: &[u8]) -> Option<(Group, ItemView)> {
        let q = *record.first()?;
        Some((
            Group::of_code(q)?,
            ItemView {
                name: format!("Item {q}"),
                ..ItemView::default()
            },
        ))
    }

    /// The pages read so far, the latest slots and gold; a change marks
    /// it stale once.
    #[test]
    fn the_book_keeps_what_the_stash_answered() {
        let read = Reading {
            item: &read_one,
            name: &|_| None,
        };
        let mut b = StashBook::default();
        assert_eq!(b.holdings(&read), None, "nothing before an answer");
        let stacks = BTreeMap::from([("r24".to_owned(), 3)]);
        b.saw(Some(0), 500, stacks.clone(), vec![(7, vec![5])]);
        b.saw(Some(1), 400, stacks.clone(), vec![(8, vec![2])]);
        // A read without a page keeps the pages read.
        b.saw(None, 300, BTreeMap::new(), Vec::new());
        let h = b.holdings(&read).expect("known");
        assert_eq!((h.gold, h.stacks.len()), (300, 0));
        let items: Vec<(u32, Group, String)> = h
            .items
            .iter()
            .map(|i| (i.item_id, i.group, i.item.name.clone()))
            .collect();
        assert_eq!(
            items,
            [
                (7, Group::Unique, "Item 5".to_owned()),
                (8, Group::Rare, "Item 2".to_owned())
            ]
        );
        // A page read again replaces its items.
        b.saw(Some(0), 300, stacks, Vec::new());
        assert_eq!(b.holdings(&read).expect("known").items.len(), 1);
        assert!(!b.take_stale());
        b.touch();
        assert!(b.take_stale());
        assert!(!b.take_stale());
    }
}
