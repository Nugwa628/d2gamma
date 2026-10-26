#![no_main]
//! The main server's save checks (d2-save-checks) over arbitrary save
//! bytes: `CheckList::run_records` decodes the record and runs every
//! standard check, with small synthetic tables (rings, seven classes with
//! one stat row, a 99-level experience table; no game files). The input
//! is checked as a new character's first save and, cut in two, as a save
//! after a stored record. A record that doesn't decode is refused, never a
//! panic; seeds: `seeds/save_loader`.
use std::sync::{Arc, OnceLock};

use d2_data::tables::{Charstats, Experience, Record};
use d2_formats::d2s::{self, StatSave};
use d2_save_checks::{CheckList, CheckTables, SaveInfo};
use d2_sim::bench_fixtures::{item_rec, item_tables, push_item};
use d2_sim::combat::vitals::VitalsTables;
use d2_sim::items::bitstream::Isc;
use d2_sim::world::hirelings::rows::HirelingRows;
use libfuzzer_sys::fuzz_target;

/// Character stat ids and their save widths: strength, energy, dexterity,
/// vitality, stat points, skill points, level, experience, gold, stash gold.
const CHAR_BITS: [(u16, u8); 10] = [
    (0, 10),
    (1, 10),
    (2, 10),
    (3, 10),
    (4, 10),
    (5, 8),
    (12, 7),
    (13, 32),
    (14, 25),
    (15, 25),
];

fn list() -> &'static CheckList {
    static LIST: OnceLock<CheckList> = OnceLock::new();
    LIST.get_or_init(|| {
        let mut items = item_tables();
        let mut isc = vec![Isc::default(); 360];
        isc[194].save_bits = 4;
        items.isc = isc;
        push_item(&mut items, item_rec(10, b"rin "));
        let mut cs = Charstats::decode(&[0u8; Charstats::SIZE]);
        cs.str = 20;
        cs.dex = 25;
        cs.vit = 20;
        cs.int = 15;
        cs.statperlevel = 5;
        let row = |v: u32| {
            let mut e = Experience::decode(&[0u8; Experience::SIZE]);
            e.amazon = v;
            e.sorceress = v;
            e.necromancer = v;
            e.paladin = v;
            e.barbarian = v;
            e.druid = v;
            e.assassin = v;
            e
        };
        let mut experience = vec![row(99)];
        experience.extend((0..=99u32).map(|l| row(1000 * l * l)));
        let mut stat_saves = vec![StatSave::default(); 16];
        for (id, bits) in CHAR_BITS {
            stat_saves[usize::from(id)] = StatSave {
                bits,
                param: 0,
                signed: false,
            };
        }
        let tables = CheckTables::from_parts(
            items,
            VitalsTables {
                charstats: vec![cs; 7],
                experience,
            },
            stat_saves,
            HirelingRows::default(),
        );
        CheckList::standard(Arc::new(tables))
    })
}

fuzz_target!(|data: &[u8]| {
    d2_fuzz::arm("save_checks", data);
    let list = list();
    let info = SaveInfo {
        played_ms: Some(60_000),
        difficulty: Some(0),
        registry: None,
    };
    let _ = list.run_records(None, data, info);
    // A valid checksum and size let mutations reach the body and the checks.
    if data.len() >= d2s::HEADER_SIZE {
        let mut fixed = data.to_vec();
        d2s::finish(&mut fixed);
        let _ = list.run_records(None, &fixed, info);
        let _ = list.run_records(Some(&fixed), &fixed, info);
    }
    if data.len() >= 2 {
        let (a, b) = data.split_at(data.len() / 2);
        let _ = list.run_records(Some(a), b, info);
        let _ = list.run_records(Some(data), data, info);
    }
});
