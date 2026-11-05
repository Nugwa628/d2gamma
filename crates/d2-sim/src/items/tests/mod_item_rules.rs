//! A mod game's item rules (`items::item_rules`): the ethereal and socket
//! rolls of a new item and the runeword match are the mod's when its item
//! provider answers, the engine's otherwise. A provider may also keep a
//! charm whose affix rolls find nothing.

use std::sync::Arc;

use super::*;
use crate::items::item_rules::{runeword_for, RunewordItem};
use crate::items::provider::{GenHost, ItemProvider};
use crate::items::quality::dispatch;
use crate::items::{flag, q, stat, ty, Fatal, ItemRequest};
use crate::rules::{ModRules, Ruleset};

/// The stat the mod's ethereal roll writes.
const STAT_A: u16 = 7;
/// The stat the mod's runeword match reads.
const STAT_RW: u16 = 294;

/// Every normal item is ethereal (stat 7 set to 25 first) with 4 sockets;
/// an item with stat 294 forms runes row "filler count".
#[derive(Debug)]
struct Rules;

impl ItemProvider for Rules {
    fn ethereal_roll(&self, host: &mut dyn GenHost, quality: u8, _: &ItemRequest) -> Option<bool> {
        host.item_stats().set_base(STAT_A, 0, 25);
        Some(quality == q::NORMAL)
    }
    fn socket_roll(&self, _: &mut dyn GenHost, _: u8, _: &ItemRequest) -> Option<i32> {
        Some(4)
    }
    fn runeword(&self, _: &ItemTables, item: &RunewordItem<'_>) -> Option<Option<usize>> {
        ((item.stat)(STAT_RW) > 0).then_some(Some(item.fillers.len()))
    }
}

/// An item provider that overrides nothing.
#[derive(Debug)]
struct Bare;

impl ItemProvider for Bare {}

#[derive(Debug)]
struct Game<P>(P);

impl<P: ItemProvider + 'static> ModRules for Game<P> {
    fn name(&self) -> &str {
        "item-rules-test"
    }
    fn items(&self) -> Option<&dyn ItemProvider> {
        Some(&self.0)
    }
}

fn ring(rules: Ruleset) -> (ItemTables, usize) {
    let mut t = tables();
    let i = push_item(&mut t, item_rec(RING, b"rin "));
    t.rules = rules;
    (t, i)
}

/// A new normal ring under `rules`.
fn made(rules: Ruleset) -> Item<FakeStats> {
    let (t, i) = ring(rules);
    let mut it = item(i, 0xDEAD_BEEF);
    let mut rq = ItemRequest {
        quality: q::NORMAL,
        ..Default::default()
    };
    assert!(dispatch(&t, &mut FakeGame::default(), &mut it, &mut rq).unwrap());
    it
}

#[test]
fn a_mod_rolls_ethereal_and_sockets() {
    let it = made(Ruleset::Mod(Arc::new(Game(Rules))));
    assert_ne!(it.flags & flag::ETHEREAL, 0);
    assert_ne!(it.flags & flag::SOCKETED, 0);
    assert_eq!(it.stats.stat(stat::NUMSOCKETS, 0), 4);
    assert_eq!(it.stats.stat(STAT_A, 0), 25);
}

/// The engine's ring: no ethereal roll (item format 0), no sockets (no
/// inventory); a provider without the rolls makes the same item.
#[test]
fn the_engines_rolls_are_unchanged() {
    let own = made(Ruleset::Original);
    assert_eq!(own.flags & (flag::ETHEREAL | flag::SOCKETED), 0);
    assert_eq!(own.stats.stat(stat::NUMSOCKETS, 0), 0);
    let bare = made(Ruleset::Mod(Arc::new(Game(Bare))));
    assert_eq!(bare.flags, own.flags);
    assert_eq!(bare.item_seed, own.item_seed);
    assert_eq!(bare.stats.stat(STAT_A, 0), own.stats.stat(STAT_A, 0));
}

#[test]
fn a_mod_matches_runewords() {
    let fillers = [1, 2];
    let item = |t: &ItemTables, i, rw: i32| {
        let stat = move |s| if s == STAT_RW { rw } else { 0 };
        runeword_for(
            t,
            &RunewordItem {
                record: i,
                quality: q::NORMAL,
                sockets: 2,
                fillers: &fillers,
                stat: &stat,
            },
        )
    };
    let (t, i) = ring(Ruleset::Mod(Arc::new(Game(Rules))));
    assert_eq!(item(&t, i, 1), Some(2));
    // The mod's `None`: the engine's match (no runes rows).
    assert_eq!(item(&t, i, 0), None);
    let (t, i) = ring(Ruleset::Original);
    assert_eq!(item(&t, i, 1), None);
}

/// Keeps a charm whose affix rolls find nothing.
#[derive(Debug)]
struct KeepsCharms;

impl ItemProvider for KeepsCharms {
    fn keeps_charm_without_affix(&self) -> bool {
        true
    }
}

/// The normal-quality dispatch of a new charm under `rules`, with no
/// affix rows to roll.
fn charm(rules: Ruleset) -> (Result<bool, Fatal>, Item<FakeStats>) {
    let mut t = tables();
    let i = push_item(&mut t, item_rec(ty::CHAR, b"cm1 "));
    t.rules = rules;
    let mut it = item(i, 0xDEAD_BEEF);
    let mut rq = ItemRequest {
        quality: q::NORMAL,
        ..Default::default()
    };
    let r = dispatch(&t, &mut FakeGame::default(), &mut it, &mut rq);
    (r, it)
}

#[test]
fn a_mod_keeps_a_charm_without_affix() {
    let (r, it) = charm(Ruleset::Mod(Arc::new(Game(KeepsCharms))));
    assert_eq!(r, Ok(true));
    assert_eq!(it.quality, q::NORMAL);
    assert_eq!((it.prefix[0], it.suffix[0]), (0, 0));
    assert_eq!(it.flags & flag::IDENTIFIED, 0);
}

/// The engine's charm routine: no affix is fatal; a provider that does
/// not keep the charm changes nothing.
#[test]
fn a_charm_without_affix_is_fatal_by_default() {
    assert_eq!(charm(Ruleset::Original).0, Err(Fatal::Charm));
    let bare = charm(Ruleset::Mod(Arc::new(Game(Bare))));
    assert_eq!(bare.0, Err(Fatal::Charm));
}
