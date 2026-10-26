//! A mod game's item use on another item, and the actions an item use may
//! start ([`UseHost`]): arming on the first use, the target's level
//! requirement, a property record on the target, a warp, a monster, an
//! object and a skill and stat reset.

use std::sync::{Arc, Mutex};

use super::*;
use crate::items::moves::iflag;
use crate::items::provider::{ItemProvider, UseCall, UseHost};
use crate::items::tables::{PropRec, PropSlot, PropertyRec};
use crate::rules::{ModRules, Ruleset};

/// A use function id D2RUST has none for: an orb used on another item.
const ORB: u32 = 40;
/// A use function that starts the actions.
const ACTIONS: u32 = 41;
const DEX: u16 = 2;

/// [`ORB`] arms first, then adds +7 dexterity to its target and keeps the
/// target's level requirement; [`ACTIONS`] warps, spawns and resets.
#[derive(Debug, Default)]
struct Uses {
    calls: Mutex<Vec<UseCall>>,
    level_req: Mutex<Option<i32>>,
}

impl ItemProvider for Uses {
    fn has_use_fn(&self, entry: u32) -> bool {
        matches!(entry, ORB | ACTIONS)
    }
    fn use_arms_first(&self, entry: u32) -> bool {
        entry == ORB
    }
    fn use_fn(&self, host: &mut dyn UseHost, call: &UseCall) -> Option<bool> {
        self.calls.lock().unwrap().push(*call);
        let p = host.unit_of(call.player)?;
        match call.entry {
            ORB => {
                let target = call.target.guid;
                *self.level_req.lock().unwrap() = Some(host.item_level_req(target, p));
                let rec = PropRec {
                    code: 0,
                    param: 0,
                    min: 7,
                    max: 7,
                };
                Some(host.apply_property(target, 0, &rec))
            }
            _ => {
                host.warp_to_level(p, 40, 2);
                host.spawn_monster(p, 7, 1);
                host.spawn_object(p, 9, 0);
                host.respec(call.player);
                Some(true)
            }
        }
    }
}

#[derive(Debug)]
struct Game(Arc<Uses>);

impl ModRules for Game {
    fn name(&self) -> &str {
        "use-host"
    }
    fn items(&self) -> Option<&dyn ItemProvider> {
        Some(self.0.as_ref())
    }
}

/// A world whose key has use function `entry` and whose properties row 0
/// adds dexterity (function 1).
fn world(entry: u32, rules: Ruleset) -> World {
    let mut t = inv_tables();
    t.items[KEY].pspell = entry;
    t.items[KEY].useable = 1;
    let mut w = World::with_tables(t);
    let mut p = PropertyRec::default();
    p.slots[0] = PropSlot {
        func: 1,
        stat: DEX,
        set: 0,
        val: 0,
    };
    w.tables.properties = vec![p];
    w.hooks.rules = rules;
    w
}

fn stored(w: &mut World, record: usize, x: u32, y: u32) -> Guid {
    let g = w.cursor_item(record);
    assert_eq!(w.handle(&insert(g, x, y, 0)), Ok(0));
    w.drain();
    g
}

fn modded(uses: &Arc<Uses>) -> Ruleset {
    Ruleset::Mod(Arc::new(Game(uses.clone())))
}

/// The first use arms the orb and runs nothing; the second runs the use
/// function on the target: +7 dexterity on the cap's own list.
#[test]
fn an_orb_arms_first_then_changes_its_target() {
    let uses = Arc::new(Uses::default());
    let mut w = world(ORB, modded(&uses));
    let k = stored(&mut w, KEY, 0, 0);
    let cap = stored(&mut w, CAP, 2, 0);
    let me = w.me();
    assert!(!w.desk(|d| d.dispatch_use(me, k, Owner::item(cap), 0, 0)));
    assert!(uses.calls.lock().unwrap().is_empty(), "armed only");
    assert_ne!(w.data(k).flags & iflag::TARGETING, 0);
    assert!(w.desk(|d| d.dispatch_use(me, k, Owner::item(cap), 0, 0)));
    assert_eq!(uses.calls.lock().unwrap().len(), 1);
    let cu = w.unit(cap).unwrap();
    assert_eq!(w.stats.unit_total(cu, DEX, 0), 7);
    let p = w.player;
    let want = w.desk(|d| d.item_level_requirement(cu, Some(p)));
    assert_eq!(*uses.level_req.lock().unwrap(), Some(want));
}

/// An entry the mod doesn't mark runs on the first use.
#[test]
fn an_unmarked_entry_runs_on_the_first_use() {
    let uses = Arc::new(Uses::default());
    let mut w = world(ACTIONS, modded(&uses));
    let k = stored(&mut w, KEY, 0, 0);
    let me = w.me();
    assert!(w.desk(|d| d.dispatch_use(me, k, Owner::item(k), 0, 0)));
    assert_eq!(uses.calls.lock().unwrap().len(), 1);
}

/// The actions reach the game's hooks: the warp is queued as a level
/// warp, the monster and object are asked near the player, and the reset
/// is the item-move reset of a Token of Absolution.
#[test]
fn the_actions_reach_the_game() {
    let uses = Arc::new(Uses::default());
    let mut w = world(ACTIONS, modded(&uses));
    let k = stored(&mut w, KEY, 0, 0);
    let me = w.me();
    w.rest.log.clear();
    assert!(w.desk(|d| d.dispatch_use(me, k, Owner::item(k), 0, 0)));
    let p = w.player;
    assert_eq!(w.hooks.warps, [(p, 40, 2)]);
    assert_eq!(w.hooks.spawns, [("monster", p, 7, 1), ("object", p, 9, 0)]);
    let reset = format!("reset_skills_stats {}", me.guid);
    assert!(w.rest.log.contains(&reset), "{:?}", w.rest.log);
}

/// Under D2RUST's rules entry 40 is no use entry: no arming, no call.
#[test]
fn original_rules_have_no_orb() {
    let mut w = world(ORB, Ruleset::Original);
    let k = stored(&mut w, KEY, 0, 0);
    let cap = stored(&mut w, CAP, 2, 0);
    let me = w.me();
    assert!(!w.desk(|d| d.dispatch_use(me, k, Owner::item(cap), 0, 0)));
    assert_eq!(w.data(k).flags & iflag::TARGETING, 0);
    assert_eq!(w.stats.unit_total(w.unit(cap).unwrap(), DEX, 0), 0);
}
