//! A mod game's hire list and hire offer ([`NpcProvider::hire_list`],
//! [`NpcProvider::hire_offer`]), with a synthetic provider: its list offers
//! every even name, each slot on the next NPC-control seed; its offer is
//! row 3 at level 7 for 123 gold, and name 2002 gets no offer. A provider
//! without them leaves D2RUST's list and offer.

use std::sync::Arc;

use super::*;
use crate::rules::{ModRules, NpcProvider, Ruleset};
use crate::world::hirelings::rows::Offer;
use crate::world::npc::npc_rules::HireRequest;

#[derive(Debug)]
struct Hires;

impl NpcProvider for Hires {
    fn hire_list(&self, _class: u16, row: &HireRow, seed: &mut Seed) -> Option<Vec<HireSlot>> {
        Some(
            (row.name_first..=row.name_last)
                .map(|name| HireSlot {
                    name,
                    seed: seed.step(),
                    hired: false,
                    offered: name % 2 == 0,
                })
                .collect(),
        )
    }
    fn hire_offer(&self, req: &HireRequest) -> Option<Option<Offer>> {
        Some((req.name != 2002).then_some(Offer {
            row: 3,
            id: 0,
            level: 7,
            life: 0,
            strength: 0,
            dexterity: 0,
            price: 123,
            experience: 0,
            defense: 0,
            min_damage: 0,
            max_damage: 0,
            share: 0,
            resist: 0,
            hire_desc: 0,
        }))
    }
}

#[derive(Debug)]
struct Bare;

impl NpcProvider for Bare {}

#[derive(Debug)]
struct Game<P>(P);

impl<P: NpcProvider + 'static> ModRules for Game<P> {
    fn name(&self) -> &str {
        "hire test mod"
    }
    fn npcs(&self) -> Option<&dyn NpcProvider> {
        Some(&self.0)
    }
}

fn modded<P: NpcProvider + 'static>(p: P) -> Ruleset {
    Ruleset::Mod(Arc::new(Game(p)))
}

#[test]
fn a_mod_draws_the_hire_list() {
    let mut c = control(0);
    let mut s = c.seed;
    c.make_hire_list_for(&modded(Hires), class::GREIZ).unwrap();
    let want: Vec<HireSlot> = (2000..=2002)
        .map(|name| HireSlot {
            name,
            seed: s.step(),
            hired: false,
            offered: name % 2 == 0,
        })
        .collect();
    let r = c.record(class::GREIZ).unwrap();
    assert!(r.hire_made);
    assert_eq!(r.hire.as_ref().unwrap().slots, want);
    assert_eq!(c.seed, s);
}

/// Greiz under `rules`, talked to by a player with 10000 gold.
fn greiz(rules: Ruleset) -> (NpcControl, Fake) {
    let mut c = control(0);
    let mut w = Fake::new();
    w.rules = rules;
    let greiz = w.npc(class::GREIZ, 9);
    talk(&mut c, &mut w, greiz);
    w.set(PLAYER, stat::GOLD, 10_000);
    (c, w)
}

#[test]
fn a_mod_prices_the_hire() {
    let (mut c, mut w) = greiz(modded(Hires));
    let seed = c.record(class::GREIZ).unwrap().hire.clone().unwrap().slots[0].seed;
    assert_eq!(c.hire(&mut w, PLAYER, &msg36(9, 2000)).unwrap(), 0);
    assert_eq!(w.get(PLAYER, stat::GOLD), 10_000 - 123);
    assert!(w.has(&format!("init 101 row 3 name 2000 seed {seed}")));
    // No offer for name 2002: nothing is paid or made.
    w.log.clear();
    assert_eq!(c.hire(&mut w, PLAYER, &msg36(9, 2002)).unwrap(), 0);
    assert_eq!(w.get(PLAYER, stat::GOLD), 10_000 - 123);
    assert!(!w.log.iter().any(|l| l.starts_with("init ")));
}

#[test]
fn the_engines_hire_list_and_offer_are_unchanged() {
    let mut own = control(0);
    own.make_hire_list(class::KASHYA).unwrap();
    let mut bare = control(0);
    bare.make_hire_list_for(&modded(Bare), class::KASHYA)
        .unwrap();
    assert_eq!(
        bare.record(class::KASHYA).unwrap().hire,
        own.record(class::KASHYA).unwrap().hire
    );
    assert_eq!(bare.seed, own.seed);
    let hire = |rules: Ruleset| {
        let (mut c, mut w) = greiz(rules);
        c.hire(&mut w, PLAYER, &msg36(9, 2000)).unwrap();
        (w.get(PLAYER, stat::GOLD), w.log.clone(), c.seed)
    };
    let o = hire(Ruleset::Original);
    assert!(o.0 < 10_000, "D2RUST's price was paid");
    assert_eq!(hire(modded(Bare)), o);
}
