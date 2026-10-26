//! A mod game's creation fill ([`crate::missiles::fill`]): it replaces
//! D2RUST's damage setup when it answers, D2RUST's runs when it doesn't,
//! and D2RUST's own rules keep their setup.

use std::sync::{Arc, Mutex};

use super::*;
use crate::missiles::fill::FillRequest;
use crate::rules::{MissileProvider, ModRules, Ruleset};

/// Answers `bits` (none: leaves the fill to D2RUST) and logs the requests.
#[derive(Debug, Default)]
struct ModFill {
    bits: Option<u32>,
    seen: Mutex<Vec<FillRequest>>,
}

impl MissileProvider for ModFill {
    fn fill_damage(
        &self,
        _game: &mut Game,
        cx: &mut Ctx<'_, dyn MissileWorld + '_>,
        req: FillRequest,
    ) -> Option<u32> {
        self.seen.lock().unwrap().push(req);
        // The fill writes the missile's damage through the world.
        cx.world.set_stat(req.missile, stat::MINDAMAGE, 0x300);
        self.bits
    }
}

impl ModRules for ModFill {
    fn name(&self) -> &str {
        "mod-fill"
    }
    fn missiles(&self) -> Option<&dyn MissileProvider> {
        Some(self)
    }
}

fn params(w: &World) -> MissileParams {
    MissileParams {
        skill: 7,
        level: 4,
        ..w.params()
    }
}

#[test]
fn the_mod_fill_replaces_d2rust_setup() {
    let mut w = World::new(row());
    let p = Arc::new(ModFill {
        bits: Some(0x10),
        ..ModFill::default()
    });
    w.fake.rules = Ruleset::Mod(p.clone());
    let m = w.create(&params(&w)).unwrap();
    assert_eq!(w.fake.logged("setup"), 0);
    assert_eq!(w.store.get(m).unwrap().flags & 0x10, 0x10);
    assert_eq!(w.fake.stats.get(&(m, stat::MINDAMAGE)), Some(&0x300));
    let seen = p.seen.lock().unwrap().clone();
    assert_eq!(
        seen,
        [FillRequest {
            owner: w.owner,
            origin: Some(w.owner),
            missile: m,
            skill: 7,
            level: 4,
        }]
    );
}

#[test]
fn without_an_answer_d2rust_fills() {
    let mut w = World::new(row());
    let p = Arc::new(ModFill::default());
    w.fake.rules = Ruleset::Mod(p.clone());
    w.create(&params(&w)).unwrap();
    assert_eq!(w.fake.logged("setup"), 1);
    assert_eq!(p.seen.lock().unwrap().len(), 1);

    let mut w = World::new(row());
    w.create(&params(&w)).unwrap();
    assert_eq!(w.fake.logged("setup"), 1);
}
