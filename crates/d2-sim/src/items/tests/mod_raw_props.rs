//! A mod game's property functions writing raw values
//! ([`PropHost::write_raw`]) and reading the mode's target list
//! ([`PropHost::target_stat`]).

use std::sync::Arc;

use super::*;
use crate::items::props::{apply_property, PropCtx};
use crate::items::provider::{ItemProvider, PropCall, PropHost};
use crate::rules::{ModRules, Ruleset};

/// Writes `min` raw (set or added by the slot's `set`) at layer `param`.
const RAW_FN: u8 = 41;
/// Adds the target list's value of [`STAT_A`] plus 1 to the slot's stat,
/// then clears [`STAT_A`] with a raw 0.
const MOVE_FN: u8 = 42;
const STAT_A: u16 = 7;
const STAT_B: u16 = 9;

#[derive(Debug)]
struct Raw;

impl ItemProvider for Raw {
    fn has_property_fn(&self, func: u8) -> bool {
        matches!(func, RAW_FN | MOVE_FN)
    }
    fn property_fn(&self, host: &mut dyn PropHost, call: &PropCall) -> Option<i32> {
        match call.func {
            RAW_FN => {
                host.write_raw(call.set, call.stat, call.rec.param as u16, call.rec.min);
                Some(call.rec.min)
            }
            MOVE_FN => {
                let a = host.target_stat(STAT_A, 0);
                host.write_raw(false, call.stat, 0, a + 1);
                host.write_raw(true, STAT_A, 0, 0);
                Some(a)
            }
            _ => None,
        }
    }
}

#[derive(Debug)]
struct Game;

impl ModRules for Game {
    fn name(&self) -> &str {
        "raw-props"
    }
    fn items(&self) -> Option<&dyn ItemProvider> {
        Some(&Raw)
    }
}

fn tables_with(rules: Ruleset, func: u8, stat: u16, set: bool) -> (ItemTables, usize) {
    let mut t = tables();
    let i = push_item(&mut t, item_rec(ty::STAF, b"sst "));
    let mut p = PropertyRec::default();
    p.slots[0] = PropSlot {
        func,
        stat,
        set: u8::from(set),
        val: 0,
    };
    t.properties = vec![p];
    // A shifted stat: `add_stat` would write `value << 8`.
    t.valshift[usize::from(STAT_A)] = 8;
    t.rules = rules;
    (t, i)
}

fn modded() -> Ruleset {
    Ruleset::Mod(Arc::new(Game))
}

/// A raw add skips the value shift and lands on the record's layer.
#[test]
fn a_raw_write_is_unshifted() {
    let (t, i) = tables_with(modded(), RAW_FN, STAT_A, false);
    let mut it = item(i, 7);
    let mut ctx = PropCtx::item(0);
    apply_property(&t, &mut it, &mut ctx, &rec(0, 3, 5, 5));
    apply_property(&t, &mut it, &mut ctx, &rec(0, 3, 5, 5));
    assert_eq!(it.stats.item_list(STAT_A, 3), 10);
    assert_eq!(it.stats.item_list(STAT_A, 0), 0);
}

/// A raw set writes a 0 over a value (`add_stat` skips a 0).
#[test]
fn a_raw_set_writes_zero() {
    let (t, i) = tables_with(modded(), RAW_FN, STAT_A, true);
    let mut it = item(i, 7);
    it.stats.list_set(ListKey::ITEM, STAT_A, 0, 99);
    apply_property(&t, &mut it, &mut PropCtx::item(0), &rec(0, 0, 0, 0));
    assert_eq!(it.stats.item_list(STAT_A, 0), 0);
    assert!(it.stats.lists[&ListKey::ITEM].contains_key(&(STAT_A, 0)));
}

/// The target list is the foreign owner's when the mode has one: the
/// function reads and writes there, not on the item.
#[test]
fn the_target_list_is_the_owners() {
    let (t, i) = tables_with(modded(), MOVE_FN, STAT_B, false);
    let mut it = item(i, 7);
    it.stats.list_set(ListKey::ITEM, STAT_A, 0, 40);
    let mut owner = FakeStats::default();
    owner.list_set(ListKey::ITEM, STAT_A, 0, 6);
    let mut ctx = PropCtx {
        mode: 4,
        list: ListKey::ITEM,
        owner: Some(&mut owner),
        extra: None,
    };
    apply_property(&t, &mut it, &mut ctx, &rec(0, 0, 0, 0));
    assert_eq!(owner.item_list(STAT_B, 0), 7);
    assert_eq!(owner.item_list(STAT_A, 0), 0);
    assert_eq!(
        it.stats.item_list(STAT_A, 0),
        40,
        "the item's own list is untouched"
    );
    assert_eq!(it.stats.item_list(STAT_B, 0), 0);
}

/// Under D2RUST's rules the mod's function ids end the row: nothing is
/// written.
#[test]
fn original_rules_write_nothing() {
    let (t, i) = tables_with(Ruleset::Original, RAW_FN, STAT_A, true);
    let mut it = item(i, 7);
    apply_property(&t, &mut it, &mut PropCtx::item(0), &rec(0, 0, 5, 5));
    assert!(!it.stats.any);
}
