// A mod's class skill without a skilldesc record (`DataRules::Mod`): no
// skill tab, so no tab bonus, and the skill still levels. Under the
// game's own rules the same tables stay fatal (`tests.rs`
// `class_skill_without_skilldesc_is_fatal`).
use super::fake::*;
use super::*;
use crate::units::UnitType;
use d2_data::bin::{BinSet, BinTable, CodeFile};
use d2_data::schema::CalcBuffer;
use d2_data::strings::StringTables;
use d2_data::txt::TxtTable;
use std::collections::BTreeMap;

fn native(skill: i32, base: i32) -> SkillEntry {
    SkillEntry {
        skill,
        base,
        owner_guid: -1,
        ..SkillEntry::default()
    }
}

/// One class skill of class 1 whose skilldesc index is past the table
/// (`skill_rec` points it at 0xFFFF), loaded under `rules`.
fn tables(rules: DataRules) -> SkillTables {
    let mut r = skill_rec();
    r.charclass = 1;
    SkillTables {
        data_rules: rules,
        ..skill_tables(vec![r])
    }
}

/// A class-1 player with every bonus stat the class-skill branch reads:
/// allskills 2, class skills 1, a tab bonus 5 on every tab layer,
/// nonclassskill 7 (capped at 3).
fn bonus_player(f: &mut Fake) -> usize {
    let u = f.add(FUnit::new(UnitType::Player, 1).with(127, 2));
    f.units[u].stats.insert((83, 1), 1);
    for tab in 0..64 {
        f.units[u].stats.insert((188, tab), 5);
    }
    f.units[u].stats.insert((97, 0), 7);
    u
}

#[test]
fn mod_class_skill_without_skilldesc_gets_no_tab_bonus() {
    let t = tables(DataRules::Mod);
    assert!(t.skilldesc_of(&t.skills[0]).is_none());
    let mut f = Fake::default();
    let u = bonus_player(&mut f);
    let e = native(0, 4);
    // allskills + class skills + nonclassskill (capped); no tab bonus.
    assert_eq!(bonus_level(&f, &t, u, &e), 2 + 1 + 3);
    assert_eq!(skill_level(&f, &t, Some(u), Some(&e), true), 4 + 6);
    assert_eq!(skill_level(&f, &t, Some(u), Some(&e), false), 4);
}

#[test]
#[should_panic(expected = "skilldesc")]
fn original_class_skill_without_skilldesc_stays_fatal() {
    let t = tables(DataRules::Original);
    let mut f = Fake::default();
    let u = bonus_player(&mut f);
    bonus_level(&f, &t, u, &native(0, 4));
}

/// With a skilldesc record the rules don't matter: the tab bonus of its
/// page is added under both.
#[test]
fn class_skill_with_skilldesc_same_under_both_rules() {
    for rules in [DataRules::Original, DataRules::Mod] {
        let mut t = tables(rules);
        t.skills[0].skilldesc = 0;
        t.skilldesc[0].skillpage = 2;
        let mut f = Fake::default();
        let u = bonus_player(&mut f);
        assert_eq!(
            bonus_level(&f, &t, u, &native(0, 4)),
            2 + 1 + 5 + 3,
            "{rules:?}"
        );
    }
}

fn empty_table(name: &str, record_size: usize) -> BinTable {
    BinTable {
        name: name.into(),
        source: "test".into(),
        count: 0,
        record_size,
        records: Vec::new(),
    }
}

fn empty_txt() -> TxtTable {
    TxtTable {
        header: vec![b"Sound".to_vec()],
        records: Vec::new(),
        removed_lines: Vec::new(),
    }
}

/// The smallest set `from_bin` reads, checked under `rules`.
fn bin_set(rules: DataRules) -> BinSet {
    use d2_data::tables::{Missiles, Record, Skilldesc, Skills};
    let code = [CalcBuffer::SkillsCode, CalcBuffer::MissCode]
        .into_iter()
        .map(|buffer| {
            let file = CodeFile {
                buffer,
                source: "test".into(),
                bytes: Vec::new(),
            };
            (buffer, file)
        })
        .collect();
    BinSet {
        lod: true,
        rules,
        strings: StringTables::default(),
        wide: BTreeMap::new(),
        tables: vec![
            empty_table("skills", Skills::SIZE),
            empty_table("skilldesc", Skilldesc::SIZE),
            empty_table("missiles", Missiles::SIZE),
            empty_table("itemstatcost", 1),
        ],
        code,
        code_reports: BTreeMap::new(),
        hitclass: empty_table("hitclass", 4),
        sounds: empty_txt(),
        soundenviron: empty_txt(),
    }
}

#[test]
fn from_bin_keeps_the_sets_data_rules() {
    for rules in [DataRules::Original, DataRules::Mod] {
        let t = SkillTables::from_bin(&bin_set(rules), LEVEL_CAP_114D).expect("tables");
        assert_eq!(t.data_rules, rules);
    }
}
