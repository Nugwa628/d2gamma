//! Tests of the save checks: a passing and a failing save per rule, on
//! synthetic tables (`d2-sim`'s item fixtures, a seven-class `charstats`
//! and an experience table).

mod character;
mod items;
mod list;
mod progress;

use std::sync::Arc;

use d2_data::tables::{Charstats, Experience, Record};
use d2_formats::d2s::{self, status, Body, D2s, ItemEntry, StatSave, Stats};
use d2_sim::bench_fixtures::{item_rec, item_tables, push_item};
use d2_sim::combat::vitals::VitalsTables;
use d2_sim::items::bitstream::{write_save, Isc, StatEntry, StreamItem};
use d2_sim::items::tables::{
    AffixRec, ItemRec, ItemTables, PropRec, PropSlot, PropertyRec, UniqueRec,
};
use d2_sim::items::{flag, q};
use d2_sim::world::hirelings::rows::HirelingRows;

use crate::decoded::stat;
use crate::{CheckList, CheckTables, SaveInfo, Verdict};

/// The synthetic ring's itemtypes row (`d2-sim`'s fixture `RING`).
const RING: u16 = 10;
/// The to-hit stat the fixture's affixes write.
const TOHIT: u16 = 19;
/// The fixture's suffix "of accuracy": to-hit 5–10, levels 1+.
const SUFFIX_ACCURACY: u16 = 1;
/// The fixture's prefix "fine": to-hit 20–30, levels 1+.
const PREFIX_FINE: u16 = 748;
/// The fixture's prefix "masterly": to-hit 40–50, levels 50+.
const PREFIX_MASTERLY: u16 = 749;
/// The fixture's unique ring: to-hit 50–60, item level 10+.
const UNIQUE_RING: i32 = 0;
/// The synthetic axe's itemtypes row (`d2-sim`'s fixture `AXE`, a weapon).
const AXE: u16 = 28;
/// The fixture's axe: damage 5–8.
const AXE_MAXDAM: u8 = 8;
/// The stats of enhanced and maximum damage.
const MAXDAMAGE_PERCENT: u16 = 17;
const MINDAMAGE_PERCENT: u16 = 18;
const MAXDAMAGE: u16 = 22;
/// The fixture's suffix "of maiming": maximum damage 3–4 on axes.
const SUFFIX_MAIMING: u16 = 2;
/// The fixture's prefix "sharp": enhanced damage 10–20% on axes.
const PREFIX_SHARP: u16 = 750;
/// Character stat bits of the fixture's `itemstatcost`.
const CHAR_BITS: [(u16, u8); 10] = [
    (stat::STRENGTH, 10),
    (stat::ENERGY, 10),
    (stat::DEXTERITY, 10),
    (stat::VITALITY, 10),
    (stat::STATPTS, 10),
    (stat::NEWSKILLS, 8),
    (stat::LEVEL, 7),
    (stat::EXPERIENCE, 32),
    (stat::GOLD, 25),
    (stat::GOLDBANK, 25),
];

fn prop(func: u8, stat: u16) -> PropertyRec {
    let mut p = PropertyRec::default();
    p.slots[0] = PropSlot {
        func,
        stat,
        set: 0,
        val: 0,
    };
    p
}

/// The fixture's properties rows: to-hit, enhanced damage (function 7),
/// maximum damage (function 6).
const PROP_TOHIT: i32 = 0;
const PROP_DMG_PERCENT: i32 = 1;
const PROP_DMG_MAX: i32 = 2;

fn prop_rec(code: i32, min: i32, max: i32) -> PropRec {
    PropRec {
        code,
        param: 0,
        min,
        max,
    }
}

fn tohit(min: i32, max: i32) -> PropRec {
    prop_rec(PROP_TOHIT, min, max)
}

fn affix_on(itype: u16, group: i32, level: i32, rec: PropRec) -> AffixRec {
    let mut mods = [PropRec::NONE; 3];
    mods[0] = rec;
    AffixRec {
        spawnable: 1,
        level,
        group,
        rare: 1,
        classspecific: 0xFF,
        frequency: 1,
        itype: [itype as i16, 0, 0, 0, 0, 0, 0],
        mods,
        ..Default::default()
    }
}

fn affix(group: i32, level: i32, min: i32, max: i32) -> AffixRec {
    affix_on(RING, group, level, tohit(min, max))
}

fn item_fixture() -> ItemTables {
    let mut t = item_tables();
    let mut isc = vec![Isc::default(); 360];
    let mut set = |s: u16, bits: u8, add: u32| {
        isc[usize::from(s)] = Isc {
            valshift: 0,
            save_bits: bits,
            save_add: add,
            save_param_bits: 0,
        }
    };
    set(TOHIT, 10, 0);
    set(31, 11, 10);
    set(72, 9, 0);
    set(73, 8, 0);
    set(194, 4, 0);
    set(MAXDAMAGE_PERCENT, 9, 0);
    set(MINDAMAGE_PERCENT, 9, 0);
    set(MAXDAMAGE, 7, 0);
    t.isc = isc;
    t.properties = vec![prop(1, TOHIT), prop(7, 0xFFFF), prop(6, 0xFFFF)];
    // 747 suffix rows (the save format's prefix offset), then 2 prefixes.
    let unused = AffixRec {
        mods: [PropRec::NONE; 3],
        ..Default::default()
    };
    t.magic = vec![unused; 747];
    t.magic[usize::from(SUFFIX_ACCURACY) - 1] = affix(1, 1, 5, 10);
    t.magic[usize::from(SUFFIX_MAIMING) - 1] = affix_on(AXE, 4, 1, prop_rec(PROP_DMG_MAX, 3, 4));
    t.magic.push(affix(2, 1, 20, 30));
    t.magic.push(affix(3, 50, 40, 50));
    t.magic
        .push(affix_on(AXE, 5, 1, prop_rec(PROP_DMG_PERCENT, 10, 20)));
    t.n_suffix = 747;
    t.n_prefix = 3;
    push_item(&mut t, item_rec(RING, b"rin "));
    push_item(
        &mut t,
        ItemRec {
            mindam: 5,
            maxdam: AXE_MAXDAM,
            ..item_rec(AXE, b"axe ")
        },
    );
    let mut props = [PropRec::NONE; 12];
    props[0] = tohit(50, 60);
    t.uniques = vec![UniqueRec {
        code: *b"rin ",
        enabled: true,
        lvl: 10,
        props,
        ..Default::default()
    }];
    t
}

fn vitals() -> VitalsTables {
    let mut cs = Charstats::decode(&[0u8; Charstats::SIZE]);
    cs.str = 20;
    cs.dex = 25;
    cs.vit = 20;
    cs.int = 15;
    cs.statperlevel = 5;
    let mut max = Experience::decode(&[0u8; Experience::SIZE]);
    let per_class = |e: &mut Experience, v: u32| {
        e.amazon = v;
        e.sorceress = v;
        e.necromancer = v;
        e.paladin = v;
        e.barbarian = v;
        e.druid = v;
        e.assassin = v;
    };
    per_class(&mut max, 99);
    let mut experience = vec![max];
    // Row L + 1: 1,000 × L², the experience level L + 1 needs (level 2 at
    // 1,000, level 3 at 4,000).
    for l in 0..=99u32 {
        let mut e = Experience::decode(&[0u8; Experience::SIZE]);
        per_class(&mut e, 1000 * l * l);
        experience.push(e);
    }
    VitalsTables {
        charstats: vec![cs; 7],
        experience,
    }
}

fn tables() -> Arc<CheckTables> {
    let mut stat_saves = vec![StatSave::default(); 16];
    for (id, bits) in CHAR_BITS {
        stat_saves[usize::from(id)] = StatSave {
            bits,
            param: 0,
            signed: false,
        };
    }
    Arc::new(CheckTables::from_parts(
        item_fixture(),
        vitals(),
        stat_saves,
        HirelingRows::default(),
    ))
}

fn list() -> CheckList {
    CheckList::standard(tables())
}

/// A level-1 expansion Amazon with the class's start stats, no gold.
fn character() -> D2s {
    let mut s = D2s::new_stub(b"Tester", 0, status::EXPANSION, 0).expect("name fits");
    s.header.status &= !status::NEW;
    s.header.level = 1;
    s.body = Some(Body {
        skills: vec![0; 30],
        ..Body::default()
    });
    for (id, v) in [
        (stat::STRENGTH, 20),
        (stat::ENERGY, 15),
        (stat::DEXTERITY, 25),
        (stat::VITALITY, 20),
        (stat::LEVEL, 1),
    ] {
        set_stat(&mut s, id, v);
    }
    s
}

/// Sets (or adds) a character stat entry.
fn set_stat(s: &mut D2s, id: u16, value: i32) {
    let body = s.body.as_mut().expect("a body");
    let mut entries = body.stats.entries();
    entries.retain(|e| e.id != id);
    if value != 0 {
        entries.push(d2s::StatEntry {
            id,
            layer: 0,
            value,
        });
    }
    entries.sort_by_key(|e| e.id);
    body.stats = Stats::Bits(entries);
}

/// A character at `level` with exactly the experience the level needs.
fn at_level(level: u8) -> D2s {
    let mut s = character();
    s.header.level = level;
    set_stat(&mut s, stat::LEVEL, i32::from(level));
    let t = vitals();
    set_stat(
        &mut s,
        stat::EXPERIENCE,
        t.threshold(0, u32::from(level) - 1) as i32,
    );
    s
}

/// A magic ring: identified, item level 30, with `main` as its stats.
fn ring(id: u32, prefix: u16, suffix: u16, main: &[(u16, i32)]) -> StreamItem {
    StreamItem {
        flags: flag::IDENTIFIED,
        version: 101,
        code: *b"rin ",
        unit28: id,
        ilvl: 30,
        quality: q::MAGIC,
        file_index: -1,
        prefix: [prefix, 0, 0],
        suffix: [suffix, 0, 0],
        runeword: 0xFFFF,
        main: Some(
            main.iter()
                .map(|&(stat, value)| StatEntry {
                    stat,
                    param: 0,
                    value,
                })
                .collect(),
        ),
        ..StreamItem::default()
    }
}

/// `save` with `items` as the player's items.
fn with_items(mut save: D2s, items: &[StreamItem]) -> D2s {
    let isc = item_fixture().isc;
    let body = save.body.as_mut().expect("a body");
    body.items = items
        .iter()
        .map(|i| ItemEntry {
            bytes: write_save(i, &isc).expect("item fits").0,
        })
        .collect();
    save
}

/// Whether the check named `name` failed (refused or flagged).
fn failed(v: &Verdict, name: &str) -> bool {
    v.refused.iter().chain(&v.flagged).any(|f| f.check == name)
}

/// Runs the standard list and returns the verdict.
fn run(before: &D2s, after: &D2s) -> Verdict {
    list().run(before, after, SaveInfo::default())
}

/// The verdict passes every check.
#[track_caller]
fn assert_clean(v: &Verdict) {
    assert!(
        v.refused.is_empty() && v.flagged.is_empty(),
        "unexpected failures: {:?} {:?}",
        v.refused,
        v.flagged
    );
}

#[test]
fn fixture_character_passes_every_check() {
    let s = character();
    assert_clean(&run(&s, &s));
    let s = at_level(30);
    assert_clean(&run(&s, &s));
}
