//! A mod game's traders: an NPC class the NPC table does not know, with a
//! trade role from the mod, gets a store record, and its store is made from
//! the mod's stock. D2Gamma's own vendor records stay what the table says,
//! with no mod rules and with a mod that restates D2Gamma's roles.

use std::sync::Arc;

use d2_data::tables::Monstats;

use super::*;
use crate::items::q;
use crate::rules::{ModRules, NpcProvider, Ruleset, StoreProvider};
use crate::world::npc::{parse_npc_table, NpcRoles, VENDORS_TSV};
use crate::world::vendors::stock::{StockHost, StockItem, StockRequest};

/// The mod's NPC class: an `interact` row the NPC table has no entry for.
const MOD_NPC: u16 = 390;
/// The mod's NPC that only gambles.
const MOD_GAMBLER: u16 = 391;

/// Class 390 trades and 391 gambles (both unknown to the NPC table); every
/// other class keeps D2Gamma's roles.
#[derive(Debug)]
struct Roles;

impl NpcProvider for Roles {
    fn roles(&self, class: u16) -> Option<NpcRoles> {
        match class {
            MOD_NPC => Some(NpcRoles {
                trades: true,
                ..NpcRoles::default()
            }),
            MOD_GAMBLER => Some(NpcRoles {
                gambles: true,
                ..NpcRoles::default()
            }),
            _ => None,
        }
    }
}

/// Class 390's store is one cap, whatever the item columns say.
#[derive(Debug)]
struct Stock;

impl StoreProvider for Stock {
    fn stock(
        &self,
        _: &VendorTables,
        _: &dyn StockHost,
        req: &StockRequest,
        _: &mut Seed,
    ) -> Option<Vec<StockItem>> {
        (req.class == MOD_NPC).then(|| {
            vec![StockItem {
                record: CAP,
                quality: q::NORMAL,
                ilvl: 1,
            }]
        })
    }
}

#[derive(Debug)]
struct Game;

impl ModRules for Game {
    fn name(&self) -> &str {
        "mod traders"
    }
    fn npcs(&self) -> Option<&dyn NpcProvider> {
        Some(&Roles)
    }
    fn stores(&self) -> Option<&dyn StoreProvider> {
        Some(&Stock)
    }
}

/// A mod that says nothing about NPCs.
#[derive(Debug)]
struct Silent;

impl ModRules for Silent {
    fn name(&self) -> &str {
        "silent"
    }
}

/// A mod that gives every class D2Gamma's own roles.
#[derive(Debug)]
struct Restated;

impl NpcProvider for Restated {
    fn roles(&self, class: u16) -> Option<NpcRoles> {
        Some(NpcRoles::d2rust(class))
    }
}

impl ModRules for Restated {
    fn name(&self) -> &str {
        "restated"
    }
    fn npcs(&self) -> Option<&dyn NpcProvider> {
        Some(self)
    }
}

fn modded() -> Ruleset {
    Ruleset::Mod(Arc::new(Game))
}

/// Monstats rows 0..`n` with `interact` and `npc` set on `classes`.
fn monstats_with(n: usize, classes: &[u16]) -> Vec<Monstats> {
    let mut v: Vec<Monstats> = (0..n)
        .map(|_| Monstats::decode(&vec![0u8; Monstats::SIZE]))
        .collect();
    for &c in classes {
        v[usize::from(c)].npc = true;
        v[usize::from(c)].interact = true;
    }
    v
}

/// A world under `rules` whose NPC control also knows the mod's classes.
/// Its vendor records come from `build`.
fn world_with(rules: Ruleset, build: fn(&Ruleset, &mut World) -> InteractionState) -> World {
    let mut w = World::new(false);
    w.hooks.rules = rules.clone();
    let classes = [class::AKARA, MOD_NPC, MOD_GAMBLER];
    let mut seed = Seed::init_low(GAME_SEED);
    w.ctl = NpcControl::new(
        &monstats_with(N_MONSTATS, &classes),
        vec![],
        false,
        0,
        &mut seed,
    )
    .expect("npc control");
    w.vendor_tables.interact = classes.to_vec();
    w.state = build(&rules, &mut w);
    w
}

fn built_for(rules: &Ruleset, w: &mut World) -> InteractionState {
    let globals = GlobalLists::build_for(&w.vendor_tables, rules);
    InteractionState::new_for(rules, &w.ctl, &globals)
}

/// The state `new` makes under D2Gamma's rules, then moved to the game's
/// rules, as a host that learns the rules late does.
fn rebuilt_for(rules: &Ruleset, w: &mut World) -> InteractionState {
    let mut state = InteractionState::new(&w.ctl, &GlobalLists::build(&w.vendor_tables));
    let globals = GlobalLists::build_for(&w.vendor_tables, rules);
    state.use_rules(rules, &w.ctl, &globals);
    state
}

fn trade(w: &mut World, class: u16) -> usize {
    let player = w.spawn(UnitType::Player, 0);
    let npc = w.npc(class);
    w.set(player, &[(st::GOLD, PLAYER_GOLD), (st::LEVEL, 1)]);
    let m = msg(0x13, &[1, w.guid(npc)]);
    assert_eq!(w.desk(|d, ctl| ctl.interact(d, player, &m)), Ok(Some(0)));
    let m = msg(0x38, &[1, w.guid(npc), 0]);
    assert_eq!(w.desk(|d, ctl| ctl.menu_action(d, player, &m)), Ok(0));
    let i = w.state.vendor_index(class).expect("vendor record");
    w.state.vendors[i].store.len()
}

#[test]
fn a_mod_npc_outside_the_table_gets_a_store_from_the_mod() {
    for build in [built_for, rebuilt_for] {
        let mut w = world_with(modded(), build);
        let i = w.state.vendor_index(MOD_NPC).unwrap();
        assert!(w.state.vendors[i].trader);
        assert_eq!(trade(&mut w, MOD_NPC), 1, "{:?}", w.state.errors);
        let rec = &w.state.vendors[i];
        assert!(rec.store_generated);
        let item = w.items.get(rec.store[0]).unwrap();
        assert_eq!((item.record, item.quality), (CAP, q::NORMAL));
        w.assert_clean();
    }
}

#[test]
fn a_mod_npc_that_only_gambles_keeps_a_gamble_record() {
    let w = world_with(modded(), built_for);
    let rec = &w.state.vendors[w.state.vendor_index(MOD_GAMBLER).unwrap()];
    assert!(rec.trader && rec.has_gamble);
}

#[test]
fn under_d2gamma_rules_the_class_outside_the_table_has_no_store() {
    let mut w = world_with(Ruleset::Original, built_for);
    let i = w.state.vendor_index(MOD_NPC).unwrap();
    assert!(!w.state.vendors[i].trader);
    assert_eq!(trade(&mut w, MOD_NPC), 0);
    assert!(!w.state.vendors[i].store_generated);
}

/// Every class of the NPC table, as an `interact` NPC.
fn table_world() -> (NpcControl, VendorTables) {
    let classes: Vec<u16> = parse_npc_table(VENDORS_TSV)
        .unwrap()
        .iter()
        .map(|r| r.class)
        .collect();
    let mut seed = Seed::init_low(GAME_SEED);
    let ctl = NpcControl::new(&monstats_with(600, &classes), vec![], false, 0, &mut seed).unwrap();
    let tables = VendorTables {
        interact: classes,
        ..vendor_tables()
    };
    (ctl, tables)
}

#[test]
fn d2gamma_vendor_records_follow_the_npc_table() {
    let (ctl, tables) = table_world();
    let globals = GlobalLists::build(&tables);
    let own = InteractionState::new(&ctl, &globals);
    assert!(ctl.records.len() > 40);
    for (rec, npc) in own.vendors.iter().zip(&ctl.records) {
        assert_eq!(rec.trader, npc.trader != 0, "class {}", npc.class);
    }
    // A mod with no NPC roles, and one that restates D2Gamma's roles,
    // change no record.
    for rules in [
        Ruleset::Mod(Arc::new(Silent)),
        Ruleset::Mod(Arc::new(Restated)),
    ] {
        let globals = GlobalLists::build_for(&tables, &rules);
        let theirs = InteractionState::new_for(&rules, &ctl, &globals);
        assert_eq!(theirs.vendors, own.vendors);
    }
    // Moving a state to the original rules changes nothing either.
    let mut moved = InteractionState::new(&ctl, &globals);
    moved.use_rules(&Ruleset::Original, &ctl, &globals);
    assert_eq!(moved.vendors, own.vendors);
}
