use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::*;
use crate::rules::{ModRules, Ruleset};

const P: UnitId = UnitId(1);

/// A player's items as a list, records as `[guid]`.
#[derive(Default)]
struct Items {
    held: Vec<HeldItem>,
    town: bool,
    destroyed: Vec<u32>,
    /// `destroy` fails (the item vanished between the read and the removal).
    lose: bool,
}

fn item(guid: u32, code: &[u8; 4], place: ItemPlace) -> HeldItem {
    HeldItem {
        guid,
        code: *code,
        size: (1, 1),
        place,
        cell: (0, 0),
    }
}

impl ItemHost for Items {
    fn items(&mut self, _: UnitId) -> Vec<HeldItem> {
        self.held.clone()
    }
    fn item_stat(&mut self, _: UnitId, _: u32, _: u16) -> Option<i32> {
        None
    }
    fn in_town(&mut self, _: UnitId) -> bool {
        self.town
    }
    fn cursor_free(&mut self, _: UnitId) -> bool {
        true
    }
    fn size_of(&self, _: [u8; 4]) -> Option<(u8, u8)> {
        Some((1, 1))
    }
    fn room(&mut self, _: UnitId, _: [u8; 4]) -> u32 {
        0
    }
    fn record(&mut self, _: UnitId, guid: u32) -> Option<Vec<u8>> {
        self.held
            .iter()
            .any(|i| i.guid == guid)
            .then(|| vec![guid as u8])
    }
    fn destroy(&mut self, _: UnitId, guid: u32) -> bool {
        if self.lose {
            return false;
        }
        self.held.retain(|i| i.guid != guid);
        self.destroyed.push(guid);
        true
    }
    fn place(&mut self, _: UnitId, _: &[u8], _: PlaceAt) -> Result<u32, ItemError> {
        Err(ItemError::NoRoom)
    }
    fn create(
        &mut self,
        _: UnitId,
        code: [u8; 4],
        _: &[(u16, i32)],
        _: PlaceAt,
    ) -> Result<u32, ItemError> {
        Err(ItemError::UnknownCode(code))
    }
}

/// A tiny mod stash: runes go to a counter, anything else whole to page
/// 7 in a row; `full` refuses everything.
#[derive(Debug, Default)]
struct Tab {
    counts: Mutex<BTreeMap<[u8; 4], u32>>,
    pages: Mutex<Vec<(u16, u8, Vec<u8>)>>,
    full: bool,
    anywhere: bool,
}

impl StashHooks for Tab {
    fn town_only(&self) -> bool {
        !self.anywhere
    }
    fn put(
        &self,
        _: UnitId,
        item: &HeldItem,
        record: &[u8],
        drop: StashDrop,
    ) -> Result<StashDest, StashRefusal> {
        if self.full {
            return Err(StashRefusal::Full);
        }
        if &item.code == b"r01 " {
            *self.counts.lock().unwrap().entry(item.code).or_default() += 1;
            return Ok(StashDest::Deposit);
        }
        let mut pages = self.pages.lock().unwrap();
        let (page, x) = match drop {
            StashDrop::At { page, x, .. } => (page, x),
            _ => (7, pages.len() as u8),
        };
        pages.push((page, x, record.to_vec()));
        Ok(StashDest::Page { page, x, y: 0 })
    }
    fn put_undone(&self, _: UnitId, item: &HeldItem, dest: StashDest) {
        match dest {
            StashDest::Deposit => {
                *self.counts.lock().unwrap().get_mut(&item.code).unwrap() -= 1;
            }
            StashDest::Page { .. } => {
                self.pages.lock().unwrap().pop();
            }
        }
    }
    fn stock(&self, _: UnitId, code: [u8; 4], in_town: bool) -> u32 {
        if !in_town {
            return 0;
        }
        self.counts.lock().unwrap().get(&code).copied().unwrap_or(0)
    }
    fn pay(&self, _: UnitId, code: [u8; 4], need: u32, in_town: bool) -> u32 {
        if !in_town {
            return 0;
        }
        let mut c = self.counts.lock().unwrap();
        let have = c.entry(code).or_default();
        let n = need.min(*have);
        *have -= n;
        n
    }
}

fn town(held: Vec<HeldItem>) -> Items {
    Items {
        held,
        town: true,
        ..Items::default()
    }
}

#[test]
fn a_quick_move_asks_the_mod_and_the_item_leaves() {
    let tab = Tab::default();
    let mut items = town(vec![
        item(5, b"r01 ", ItemPlace::Inventory),
        item(6, b"cap ", ItemPlace::Cursor),
    ]);
    assert_eq!(
        put(&mut items, &tab, P, 5, StashDrop::Quick),
        Ok(StashDest::Deposit)
    );
    assert_eq!(tab.counts.lock().unwrap()[b"r01 "], 1);
    let at = StashDrop::At {
        page: 3,
        x: 4,
        y: 0,
    };
    assert_eq!(
        put(&mut items, &tab, P, 6, at),
        Ok(StashDest::Page {
            page: 3,
            x: 4,
            y: 0
        })
    );
    // The mod got the record the engine read.
    assert_eq!(tab.pages.lock().unwrap()[0], (3, 4, vec![6]));
    assert_eq!(items.destroyed, vec![5, 6]);
    assert!(items.held.is_empty());
}

#[test]
fn moves_are_town_only_unless_the_mod_says_otherwise() {
    let tab = Tab::default();
    let mut items = Items {
        held: vec![item(5, b"r01 ", ItemPlace::Inventory)],
        ..Items::default()
    };
    assert_eq!(
        put(&mut items, &tab, P, 5, StashDrop::Quick),
        Err(StashRefusal::NotInTown)
    );
    assert_eq!(send(&mut items, &tab, P), Err(StashRefusal::NotInTown));
    assert!(items.destroyed.is_empty());
    let anywhere = Tab {
        anywhere: true,
        ..Tab::default()
    };
    assert_eq!(
        put(&mut items, &anywhere, P, 5, StashDrop::Quick),
        Ok(StashDest::Deposit)
    );
}

#[test]
fn worn_belt_stash_and_missing_items_never_reach_the_mod() {
    let tab = Tab {
        full: true,
        ..Tab::default()
    };
    let mut items = town(vec![
        item(1, b"cap ", ItemPlace::Equipped),
        item(2, b"hp1 ", ItemPlace::Belt),
        item(3, b"cap ", ItemPlace::Stash),
    ]);
    for guid in [1, 2, 3] {
        assert_eq!(
            put(&mut items, &tab, P, guid, StashDrop::Quick),
            Err(StashRefusal::NotStashable)
        );
    }
    assert_eq!(
        put(&mut items, &tab, P, 9, StashDrop::Quick),
        Err(StashRefusal::NoItem)
    );
    assert_eq!(items.held.len(), 3);
}

#[test]
fn a_refused_move_changes_nothing() {
    let tab = Tab {
        full: true,
        ..Tab::default()
    };
    let mut items = town(vec![item(5, b"cap ", ItemPlace::Inventory)]);
    assert_eq!(
        put(&mut items, &tab, P, 5, StashDrop::Quick),
        Err(StashRefusal::Full)
    );
    assert_eq!(items.held.len(), 1);
    assert!(items.destroyed.is_empty());
}

#[test]
fn an_item_lost_after_the_mod_accepted_is_undone_in_the_mod() {
    let tab = Tab::default();
    let mut items = Items {
        lose: true,
        ..town(vec![
            item(5, b"r01 ", ItemPlace::Inventory),
            item(6, b"cap ", ItemPlace::Inventory),
        ])
    };
    assert_eq!(
        put(&mut items, &tab, P, 5, StashDrop::Quick),
        Err(StashRefusal::NoItem)
    );
    assert_eq!(
        put(&mut items, &tab, P, 6, StashDrop::Quick),
        Err(StashRefusal::NoItem)
    );
    assert_eq!(tab.counts.lock().unwrap()[b"r01 "], 0);
    assert!(tab.pages.lock().unwrap().is_empty());
}

#[test]
fn send_offers_the_inventory_and_cube_only() {
    let tab = Tab::default();
    let mut items = town(vec![
        item(1, b"cap ", ItemPlace::Equipped),
        item(2, b"r01 ", ItemPlace::Inventory),
        item(3, b"hp1 ", ItemPlace::Belt),
        item(4, b"cap ", ItemPlace::Cube),
        item(5, b"r01 ", ItemPlace::Cursor),
    ]);
    let r = send(&mut items, &tab, P).unwrap();
    let page7 = StashDest::Page {
        page: 7,
        x: 0,
        y: 0,
    };
    assert_eq!(r.sent, vec![(2, StashDest::Deposit), (4, page7)]);
    assert!(r.left.is_empty());
    let left: Vec<u32> = items.held.iter().map(|i| i.guid).collect();
    assert_eq!(left, vec![1, 3, 5]);
    let full = Tab {
        full: true,
        ..Tab::default()
    };
    let mut items = town(vec![item(8, b"cap ", ItemPlace::Inventory)]);
    let r = send(&mut items, &full, P).unwrap();
    assert!(r.sent.is_empty());
    assert_eq!(r.left, vec![(8, StashRefusal::Full)]);
    assert_eq!(items.held.len(), 1);
}

#[test]
fn the_default_hooks_take_nothing() {
    #[derive(Debug)]
    struct Nothing;
    impl StashHooks for Nothing {}
    let mut items = town(vec![item(5, b"r01 ", ItemPlace::Inventory)]);
    assert_eq!(
        put(&mut items, &Nothing, P, 5, StashDrop::Quick),
        Err(StashRefusal::NotStashable)
    );
    assert!(Nothing.town_only());
    assert_eq!(Nothing.stock(P, *b"r01 ", true), 0);
    assert_eq!(Nothing.pay(P, *b"r01 ", 3, true), 0);
    assert!(items.destroyed.is_empty());
}

#[test]
fn the_host_reads_records_with_its_item_table() {
    let items = Items::default();
    assert_eq!(items.facts(&[1, 2, 3]), None);
}

#[derive(Debug)]
struct WithTab(Tab);

impl ModRules for WithTab {
    fn name(&self) -> &str {
        "with-tab"
    }
    fn stash(&self) -> Option<&dyn StashHooks> {
        Some(&self.0)
    }
}

#[test]
fn the_ruleset_hands_out_the_mods_stash_and_d2gamma_has_none() {
    assert!(Ruleset::Original.stash().is_none());
    let tab = Tab::default();
    tab.counts.lock().unwrap().insert(*b"r01 ", 4);
    let r = Ruleset::Mod(Arc::new(WithTab(tab)));
    let s = r.stash().unwrap();
    assert_eq!(s.stock(P, *b"r01 ", true), 4);
    assert_eq!(s.stock(P, *b"r01 ", false), 0);
    assert_eq!(s.pay(P, *b"r01 ", 3, true), 3);
    assert_eq!(s.stock(P, *b"r01 ", true), 1);
}

/// A tiny mod stash that folds a container (`box `) into counts: the
/// container's code decides, the item is read through the host.
#[derive(Debug, Default)]
struct Folds;

impl StashHooks for Folds {
    fn credits(&self, items: &mut dyn ItemHost, player: UnitId, guid: u32) -> Vec<Credit> {
        let Some(held) = items.items(player).into_iter().find(|i| i.guid == guid) else {
            return Vec::new();
        };
        if &held.code != b"box " {
            return Vec::new();
        }
        vec![
            ("r01".into(), 3),
            ("g01".into(), 0),
            (String::new(), 9),
            ("r02".into(), 2),
            ("r01".into(), 4),
        ]
    }
}

#[test]
fn the_mods_credits_are_called_and_cleaned() {
    let mut items = town(vec![
        item(5, b"box ", ItemPlace::Inventory),
        item(6, b"r01 ", ItemPlace::Inventory),
    ]);
    assert_eq!(
        credits(&mut items, &Folds, P, 5),
        vec![("r01".to_owned(), 7), ("r02".to_owned(), 2)],
        "zero counts and empty codes dropped, repeated codes added"
    );
    assert!(credits(&mut items, &Folds, P, 6).is_empty());
    assert!(credits(&mut items, &Folds, P, 99).is_empty());
    assert!(items.destroyed.is_empty(), "asking changes nothing");
}

#[test]
fn d2gamma_and_the_default_hooks_credit_nothing() {
    #[derive(Debug)]
    struct Nothing;
    impl StashHooks for Nothing {}
    let mut items = town(vec![item(5, b"box ", ItemPlace::Inventory)]);
    assert!(credits(&mut items, &Nothing, P, 5).is_empty());
    assert!(Ruleset::Original.stash().is_none());
    // A mod's stash that never overrides it folds nothing either.
    let r = Ruleset::Mod(Arc::new(WithTab(Tab::default())));
    assert!(credits(&mut items, r.stash().unwrap(), P, 5).is_empty());
}
