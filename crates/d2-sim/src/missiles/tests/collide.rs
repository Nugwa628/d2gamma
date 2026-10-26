//! Collide types past the engine's table ([`crate::missiles::collide`]):
//! a mod game's own entries, D2RUST's table unchanged, and the area
//! search with a collide mask.

use std::sync::Arc;

use super::*;
use crate::missiles::collide::{area_units_in_sight, collide_entry, LAST_COLLIDE_TYPE};
use crate::rules::{MissileProvider, ModRules, Ruleset};

/// Type 9: monsters, mask 0x180. Type 10: no callback, mask 0x2080. Type
/// 3 is left to D2RUST.
#[derive(Debug)]
struct ModCollide;

impl MissileProvider for ModCollide {
    fn collide_mode(&self, mode: u8) -> Option<CollideMode> {
        match mode {
            9 => Some(CollideMode {
                callback: Some(Accept::Monsters),
                mask: 0x180,
            }),
            10 => Some(CollideMode {
                callback: None,
                mask: 0x2080,
            }),
            _ => None,
        }
    }
}

impl ModRules for ModCollide {
    fn name(&self) -> &str {
        "mod-collide"
    }
    fn missiles(&self) -> Option<&dyn MissileProvider> {
        Some(self)
    }
}

fn mod_rules() -> Ruleset {
    Ruleset::Mod(Arc::new(ModCollide))
}

#[test]
fn d2rust_table_is_unchanged_and_past_it_reads_as_eight() {
    let o = Ruleset::Original;
    for mode in 0..=LAST_COLLIDE_TYPE {
        assert_eq!(collide_entry(&o, mode), collide_mode(mode), "mode {mode}");
    }
    for mode in [9, 10, 0xFF] {
        assert_eq!(collide_entry(&o, mode), collide_mode(8), "mode {mode}");
    }
}

#[test]
fn mod_entries_answer_first() {
    let r = mod_rules();
    assert_eq!(collide_entry(&r, 9).map(|c| c.mask), Some(0x180));
    assert_eq!(collide_entry(&r, 10).map(|c| c.mask), Some(0x2080));
    // Types the mod leaves: D2RUST's, past the table as 8.
    assert_eq!(collide_entry(&r, 3), collide_mode(3));
    assert_eq!(collide_entry(&r, 11), collide_mode(8));
}

#[test]
fn creation_takes_the_mod_mask() {
    let mut r = row();
    r.collidetype = 9;
    let mut w = World::new(r.clone());
    w.fake.rules = mod_rules();
    w.create(&w.params()).unwrap();
    assert!(w.fake.calls.iter().any(|c| c == "move 0x180"));

    let mut w = World::new(r);
    w.create(&w.params()).unwrap();
    assert!(w.fake.calls.iter().any(|c| c == "move 0x185"));
}

#[test]
fn flight_uses_the_mod_entry() {
    // Type 9 hits the monster through the mod's callback.
    let mut r = row();
    r.collidetype = 9;
    let mut w = World::new(r);
    w.fake.rules = mod_rules();
    let m = w.create(&w.params()).unwrap();
    contact(&mut w);
    w.frame();
    assert!(!w.alive(m));
    assert_eq!(w.fake.logged("event0"), 1);

    // Type 10 has no callback in the mod: the contact removes the missile
    // without a hit (as type 4), where D2RUST's rules read it as type 8
    // and hit.
    let mut r = row();
    r.collidetype = 10;
    let mut w = World::new(r.clone());
    w.fake.rules = mod_rules();
    let m = w.create(&w.params()).unwrap();
    contact(&mut w);
    w.frame();
    assert!(!w.alive(m));
    assert_eq!(w.fake.logged("event0"), 0);

    let mut w = World::new(r);
    let m = w.create(&w.params()).unwrap();
    contact(&mut w);
    w.frame();
    assert!(!w.alive(m));
    assert_eq!(w.fake.logged("event0"), 1);
}

#[test]
fn area_search_drops_units_behind_the_mask() {
    let mut w = World::new(row());
    let m = w.create(&w.params()).unwrap();
    let (owner, monster) = (w.owner, w.monster);
    w.fake.pos.insert(m, (100, 100));
    w.fake.mb.area = vec![owner, monster];
    // A wall between the missile and the monster.
    w.fake.mb.walls.insert(((100, 100), (110, 100)));
    let mut cx = cx!(w);
    let seen = area_units_in_sight(&mut w.game, &mut cx, owner, m, (100, 100), 5, 0x583, 0x180);
    assert_eq!(seen, [owner]);
    // Mask 0: every area unit.
    let all = area_units_in_sight(&mut w.game, &mut cx, owner, m, (100, 100), 5, 0x583, 0);
    assert_eq!(all, [owner, monster]);
    assert_eq!(w.fake.logged("scan (100, 100) 5 0x583"), 2);
}
