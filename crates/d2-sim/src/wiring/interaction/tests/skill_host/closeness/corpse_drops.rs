//! Generated economy rows for corpse finding; no extracted game assets.
use super::*;
use crate::items::tables::ItemRec;
use crate::items::{ty, ItemTables};
use crate::skills::fake::blank;
use crate::treasure::class_pick::{ClassPicks, PickRow};
use crate::treasure::{ItemData, TcEntry, TreasureClass, TreasureClasses};
use crate::wiring::economy::{DeathDrops, DropTables, GameFields};
use d2_data::fixup::maps::EquivMatrix;
use d2_data::tables::{Itemratio, Itemtypes};
pub(super) fn drop_tables() -> DropTables {
    let n = usize::from(ty::GOLD.max(ty::MISC)) + 1;
    let words = n.div_ceil(32);
    let mut equiv = EquivMatrix {
        n,
        words,
        bits: vec![0; n * words],
    };
    for i in 1..n {
        equiv.bits[i * words] |= 1;
        equiv.bits[i * words + i / 32] |= 1 << (i % 32);
    }
    let (g, misc) = (usize::from(ty::GOLD), usize::from(ty::MISC));
    equiv.bits[g * words + misc / 32] |= 1 << (misc % 32);
    let mut itemtypes: Vec<Itemtypes> = (0..n)
        .map(|_| {
            let mut t: Itemtypes = blank();
            (t.class, t.staffmods, t.rare) = (0xFF, 0xFF, 1);
            t
        })
        .collect();
    // Gold is always normal quality (itemtypes `Normal`).
    itemtypes[g].normal = 1;
    let mut ratio: Itemratio = blank();
    ratio.version = 1;
    let items = ItemTables {
        items: vec![ItemRec {
            code: *b"gld ",
            type_: ty::GOLD as i16,
            level: 1,
            ..ItemRec::default()
        }],
        itemtypes,
        equiv,
        itemratio: vec![ratio],
        valshift: vec![0; 359],
        stat_shift: 6,
        stat_mask: 0x3F,
        ..ItemTables::default()
    };
    let treasure_items = vec![ItemData {
        code: *b"gld ",
        ubercode: [0; 4],
        ultracode: [0; 4],
        version: 0,
        level: 1,
        type_: ty::GOLD,
        type2: 0,
        unique: 0,
        quest: 0,
        spawnable: 1,
    }];
    let tc = |name: &[u8], entries: Vec<TcEntry>, total| TreasureClass {
        name: name.to_vec(),
        group: 0,
        level: 0,
        total_classic: total,
        total_expansion: total,
        picks: 1,
        nodrop: 0,
        mods: [0; 6],
        entries,
    };
    let gold = TcEntry {
        start_classic: 0,
        start_expansion: 0,
        id: 0,
        row: 0,
        flags: 0,
        mods: [0; 6],
    };
    DropTables {
        items,
        tcs: TreasureClasses {
            tcs: vec![tc(b"none", Vec::new(), 0), tc(b"gold", vec![gold], 1)],
            group_offset: 0,
            chest: [None; 45],
            notes: Vec::new(),
        },
        treasure_items,
        superuniques: Vec::new(),
    }
}

pub(super) fn setup(fx: &mut Fx, p: UnitId, corpse: UnitId) {
    let saved = fx.sim.sys.units.get(p).unwrap().seed;
    let codes = fx.sim.skill_use(&mut fx.game, |w| {
        [[100, 0], [0, 100], [0, 0]].map(|bands| {
            crate::skills::use_::bodies::helpers3::potion_code_with_bands(w, p, bands)
                .expect("generated act potion code")
        })
    });
    fx.sim.sys.units.get_mut(p).unwrap().seed = saved;
    let mut tables = drop_tables();
    tables.items.itemtypes[usize::from(ty::MISC)].normal = 1;
    for code in codes {
        tables.items.items.push(ItemRec {
            code,
            type_: ty::MISC as i16,
            level: 1,
            ..ItemRec::default()
        });
    }
    let rows = tables
        .items
        .items
        .iter()
        .map(|r| PickRow {
            code: r.code,
            spawnable: 1,
            level: 1,
            rarity: 1,
            type_: r.type_,
            ..PickRow::default()
        })
        .collect();
    let d = DeathDrops::new(
        Arc::new(tables),
        GameFields::new(crate::rng::Seed::init(), false),
    )
    .with_picks(Arc::new(ClassPicks {
        rows,
        weapons: 0,
        armor: 0,
        parts: None,
    }));
    fx.sim.sys.hooks.object_drops = Some(Box::new(d));
    let t = Arc::make_mut(&mut fx.sim.sys.hooks.tables);
    t.combat.monstats[0].treasureclass1 = 1;
    t.combat.monstats2[0].corpsesel = true;
    t.combat.monstats2[0].soft = true;
    fx.sim.sys.units.get_mut(corpse).unwrap().mode = 12;
    fx.set(corpse, &[(sst::HITPOINTS, 0)]);
    fx.sim.with(&mut fx.game, |_, v| {
        v.set_state(corpse, 118, true);
        assert!(
            v.stats.has_state(corpse, 118),
            "generated corpse supports consumed state"
        );
        v.set_state(corpse, 118, false);
    });
}
pub(super) fn observe(fx: &Fx, out: &mut Outcome, corpse: UnitId) {
    out.corpse_consumed = i64::from(fx.sim.sys.stats.has_state(corpse, 118));
    let drops = fx.sim.sys.hooks.object_drops.as_ref().unwrap();
    assert!(drops.errors.is_empty(), "{:?}", drops.errors);
    assert!(drops.failures.is_empty(), "{:?}", drops.failures);
    assert!(drops.pick_errors.is_empty(), "{:?}", drops.pick_errors);
    out.items_dropped = drops.placed.len() as i64;
    out.item_classes = drops
        .placed
        .iter()
        .map(|(u, _)| i64::from(fx.sim.sys.units.get(*u).unwrap().class) + 1)
        .sum();
    assert_eq!(
        drops.placed.len(),
        fx.game.lists.units_of_type(UnitType::Item).len()
    );
}
