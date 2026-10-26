//! A mod game's set bonus tiers
//! ([`ItemProvider::set_tiers`](crate::items::provider::ItemProvider::set_tiers)):
//! which of a set item's partial lists are active for the worn pieces.

use std::sync::{Arc, Mutex};

use super::*;
use crate::items::props::apply_set_item;
use crate::items::provider::ItemProvider;
use crate::items::q;
use crate::items::tables::{ItemTables, PropRec, PropSlot, PropertyRec, SetItemRec, SetRec};
use crate::rules::{ModRules, Ruleset};

const DEX: u16 = 2;

/// Tier 0 is active whenever the piece in set slot 0 is worn; records
/// each call's (setitems row, worn mask).
#[derive(Debug, Default)]
struct Tiers {
    calls: Mutex<Vec<(usize, u32)>>,
}

impl ItemProvider for Tiers {
    fn set_tiers(&self, _: &ItemTables, setitem: usize, worn: u32) -> Option<u32> {
        self.calls.lock().unwrap().push((setitem, worn));
        Some(worn & 1)
    }
}

#[derive(Debug)]
struct Game(Arc<Tiers>);

impl ModRules for Game {
    fn name(&self) -> &str {
        "set-tiers"
    }
    fn items(&self) -> Option<&dyn ItemProvider> {
        Some(self.0.as_ref())
    }
}

/// A set of three whose cap (slot 0, `add func` 2) carries +5 dexterity
/// in its first partial record (tier 0): D2RUST gives it with two pieces
/// worn.
fn world(rules: Ruleset) -> World {
    let mut w = World::new();
    w.state.link_item_stats = true;
    w.state.equip_rules = true;
    w.hooks.rules = rules;
    let mut p = PropertyRec::default();
    p.slots[0] = PropSlot {
        func: 1,
        stat: DEX,
        set: 0,
        val: 0,
    };
    w.tables.properties = vec![p];
    w.tables.sets = vec![SetRec {
        count: 3,
        version: 0,
        partial: [PropRec::NONE; 8],
        full: [PropRec::NONE; 8],
    }];
    let mut aprops = [PropRec::NONE; 10];
    aprops[0] = PropRec {
        code: 0,
        param: 0,
        min: 5,
        max: 5,
    };
    w.tables.setitems = vec![SetItemRec {
        set: 0,
        slot: 0,
        add_func: 2,
        props: [PropRec::NONE; 9],
        aprops,
        ..Default::default()
    }];
    w
}

/// The cap as a set item with its partial lists.
fn cap(w: &mut World) -> Guid {
    let g = w.cursor_item(CAP);
    let u = w.unit(g).unwrap();
    let it = w.items.get_mut(u).unwrap();
    it.quality = q::SET;
    it.file_index = 0;
    w.desk(|d| d.econ.with_item(u, |s| apply_set_item(s.tables, s.item)))
        .unwrap();
    g
}

#[test]
fn one_piece_has_no_tier_under_original() {
    let mut w = world(Ruleset::Original);
    let c = cap(&mut w);
    assert_eq!(w.handle(&body(0x1A, c, 1)), Ok(0));
    assert_eq!(w.stats.unit_total(w.player, DEX, 0), 10);
}

#[test]
fn a_mods_tiers_pick_the_active_lists() {
    let tiers = Arc::new(Tiers::default());
    let mut w = world(Ruleset::Mod(Arc::new(Game(tiers.clone()))));
    let c = cap(&mut w);
    assert_eq!(w.handle(&body(0x1A, c, 1)), Ok(0));
    assert_eq!(w.stats.unit_total(w.player, DEX, 0), 15, "tier 0 active");
    assert!(tiers.calls.lock().unwrap().contains(&(0, 1)));
}
