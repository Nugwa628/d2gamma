//! Mod data seams over the synthetic set: columns the base text lacks and
//! a level's wrapped `Id` byte.
//!
//! The synthetic text carries every column of every field list, so the
//! tests strip the columns a base `.txt` lacks (a level's `mon11`..`mon25`,
//! an affix's `etype4`, `etype5`) before the layers apply, then compile
//! the patched text as a mod game does (`DataRules::Mod`).

use std::path::PathBuf;

use d2_data::bin::{self, read_excel, DataRules};
use d2_data::calc::NO_KEYWORDS;
use d2_data::compile_set::{compile_all_under, CompiledSet};
use d2_data::patch::{self, rules_for, Code, Finding, PatchData};
use d2_data::strings::StringTables;
use d2_data::tables::{Levels, Magicsuffix, Record};
use d2_formats::mpq::ArchiveSet;
use d2_sim::items::tables::AffixRec;
use d2_sim::monsters::population::data::LevelPop;
use test_fixtures::install;
use test_fixtures::synth::{self, Synthetic};

fn dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()))
}

/// The synthetic set as a base game's text has it: `levels` without the
/// `mon11`..`mon25` columns, the affix tables without `etype4`/`etype5`.
/// Level 2 (a field) lists `beast1` ten times.
fn base_text() -> Synthetic {
    let mut data = synth::synthetic();
    for n in 3..=10 {
        data.tables.set("levels", 2, &format!("mon{n}"), "beast1");
    }
    let strip = |data: &mut Synthetic, file: &str, drop: &dyn Fn(&str) -> bool| {
        let f = data.tables.files.get_mut(file).expect("a compiled table");
        let keep: Vec<bool> = f.columns.iter().map(|c| !drop(c)).collect();
        let pick = |cells: &mut Vec<String>| {
            let mut i = 0;
            cells.retain(|_| {
                i += 1;
                keep[i - 1]
            });
        };
        pick(&mut f.columns);
        f.rows.iter_mut().for_each(pick);
    };
    let high = |c: &str| {
        let c = c.to_ascii_lowercase();
        c.strip_prefix("mon")
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| n > 10)
    };
    strip(&mut data, "levels.txt", &high);
    for file in ["magicsuffix.txt", "magicprefix.txt"] {
        strip(&mut data, file, &|c| {
            c.eq_ignore_ascii_case("etype4") || c.eq_ignore_ascii_case("etype5")
        });
    }
    data
}

fn read_base<'a>(
    data: &'a Synthetic,
) -> impl FnMut(&str) -> Result<Option<(String, Vec<u8>)>, String> + 'a {
    |name| {
        // Runtime sound environments are patchable too, but are stored
        // separately from the tables compiled by the data load sequence.
        let bytes = if name.eq_ignore_ascii_case("soundenviron.txt") {
            data.soundenviron.clone()
        } else {
            data.tables.file(name).bytes()
        };
        Ok(Some(("synthetic".to_owned(), bytes)))
    }
}

/// Applies `body` (statements after `table <table>`) over `data`'s text
/// under `rules`, and returns the patched data and the findings.
fn patched(data: &Synthetic, rules: DataRules, layers: &[&str]) -> (PatchData, Vec<Finding>) {
    let mut patch_data = PatchData::from_base(&rules_for(rules), &mut read_base(data))
        .unwrap_or_else(|f| panic!("{f:?}"));
    let layers: Vec<_> = layers
        .iter()
        .enumerate()
        .map(|(pos, text)| {
            let (layer, findings) = patch::parse_layer("mod.d2patch", text.as_bytes(), pos + 1);
            assert!(findings.is_empty(), "{findings:?}");
            layer
        })
        .collect();
    let findings = patch::apply_stack(&mut patch_data, &layers, "mod.d2stack");
    (patch_data, findings)
}

/// Compiles the patched text (and the base text of any table the patch
/// data does not hold) under `rules`.
fn compile(data: &Synthetic, patched: &PatchData, rules: DataRules, name: &str) -> CompiledSet {
    let dir = dir(name);
    install::write_text_install(&dir, data).expect("writes the text install");
    let set = ArchiveSet::open_dir(&dir).expect("opens");
    let strings = StringTables::load(&set, bin::DEFAULT_LANGUAGE, true).expect("strings");
    let mut read = |file: &str| {
        let own = patched.tables.iter().find(|t| t.rules.txt_name == file);
        match own {
            Some(t) => Ok(Some(("patched".to_owned(), t.render()))),
            None => read_excel(&set, file).map_err(|e| e.to_string()),
        }
    };
    compile_all_under(&mut read, &strings, &NO_KEYWORDS, rules).expect("compiles")
}

fn records<R: Record>(set: &CompiledSet, table: &str) -> Vec<R> {
    let c = &set.table(table).expect("compiled").compiled;
    c.records
        .chunks_exact(c.record_size)
        .map(R::decode)
        .collect()
}

const LEVELS_LAYER: &str = "d2patch 1
table levels
addcol mon11 text
addcol mon12 text
";

/// A `set` of `column` (empty before) on row `row` of `table`.
fn set_cell(patched: &PatchData, table: &str, row: usize, column: &str, new: &str) -> String {
    let key = String::from_utf8_lossy(patched.table(table).expect("table").key(row)).into_owned();
    format!(
        "set #{row} [{key}] {column} [] -> {new}
"
    )
}

#[test]
fn a_mod_layer_adds_the_level_monster_columns_the_base_text_lacks() {
    let data = base_text();
    // Under the mod rules `addcol` names the fields the text lacks.
    let (probe, _) = patched(&data, DataRules::Mod, &[LEVELS_LAYER]);
    let body = format!(
        "{LEVELS_LAYER}{}{}",
        set_cell(&probe, "levels", 2, "mon11", "ghoul1"),
        set_cell(&probe, "levels", 2, "mon12", "keeper"),
    );
    let (p, findings) = patched(&data, DataRules::Mod, &[&body]);
    assert!(findings.is_empty(), "{findings:?}");
    let levels = p.table("levels").unwrap();
    // They are the compiler's columns, not extension columns.
    assert!(p.ext_store().is_empty());
    assert_eq!(levels.adopted.len(), 2);
    let col = |name: &str| {
        levels
            .header
            .iter()
            .position(|h| h == name.as_bytes())
            .unwrap()
    };
    assert!(levels.bound[col("mon11")] && levels.bound[col("mon12")]);

    let set = compile(&data, &p, DataRules::Mod, "cols-levels");
    let lv: Vec<Levels> = records(&set, "levels");
    // `monstats` rows: beast1 0, ghoul1 1, keeper 2.
    assert_eq!((lv[2].mon11, lv[2].mon12), (1, 2));
    // Level generation sees twelve monster slots on the field.
    let pop = LevelPop::from_record(&lv[2]);
    assert_eq!(pop.mon.len(), 12);
    assert_eq!(&pop.mon[10..], &[1, 2]);
    // Rows the layer did not set read as an empty slot, as before.
    assert_eq!((lv[3].mon11, lv[3].mon12), (0xFFFF, 0xFFFF));
    assert_eq!(LevelPop::from_record(&lv[3]).mon.len(), 2);
}

#[test]
fn an_empty_adopted_column_compiles_like_a_missing_one() {
    let data = base_text();
    let (plain, f) = patched(&data, DataRules::Mod, &[]);
    assert!(f.is_empty());
    let (adopted, f) = patched(&data, DataRules::Mod, &[LEVELS_LAYER]);
    assert!(f.is_empty(), "{f:?}");
    let a = compile(&data, &plain, DataRules::Mod, "cols-missing");
    let b = compile(&data, &adopted, DataRules::Mod, "cols-empty");
    for table in ["levels", "leveldefs"] {
        assert_eq!(
            a.table(table).unwrap().compiled.records,
            b.table(table).unwrap().compiled.records,
            "{table}"
        );
    }
}

#[test]
fn a_mod_layer_adds_the_affix_exclusion_columns() {
    let data = base_text();
    let layer = "d2patch 1\ntable magicsuffix\naddcol etype4 text\naddcol etype5 text\n";
    let (probe, f) = patched(&data, DataRules::Mod, &[layer]);
    assert!(f.is_empty(), "{f:?}");
    let key = String::from_utf8_lossy(probe.table("magicsuffix").unwrap().key(0)).into_owned();
    let body =
        format!("{layer}set #0 [{key}] etype4 [] -> weap\nset #0 [{key}] etype5 [] -> armo\n");
    let (p, f) = patched(&data, DataRules::Mod, &[&body]);
    assert!(f.is_empty(), "{f:?}");
    let set = compile(&data, &p, DataRules::Mod, "cols-affix");
    let rows: Vec<Magicsuffix> = records(&set, "magicsuffix");
    let (weap, armo) = (rows[0].etype4, rows[0].etype5);
    assert!(weap >= 1 && armo >= 1 && weap != armo, "{weap} {armo}");
    // The item rules see the exclusions in the fourth and fifth slot.
    let affix = AffixRec::from(&rows[0]);
    assert_eq!(&affix.etype[3..], &[weap as i16, armo as i16]);
    // A row the layer did not set excludes nothing there.
    assert!(AffixRec::from(&rows[1]).etype[3..].iter().all(|&t| t < 1));
}

#[test]
fn the_original_rules_still_refuse_those_columns() {
    let data = base_text();
    let (p, findings) = patched(&data, DataRules::Original, &[LEVELS_LAYER]);
    assert_eq!(
        findings
            .iter()
            .map(|f| f.code)
            .filter(|c| *c == Code::A16)
            .count(),
        1,
        "{findings:?}"
    );
    assert!(p.table("levels").unwrap().adopted.is_empty());
    // A column the base text has is still refused under the mod rules.
    let (_, findings) = patched(
        &data,
        DataRules::Mod,
        &["d2patch 1\ntable levels\naddcol mon10 text\n"],
    );
    assert!(findings.iter().any(|f| f.code == Code::A16), "{findings:?}");
}

#[test]
fn a_level_id_past_255_is_stored_modulo_256() {
    let mut data = base_text();
    data.tables.set("levels", 2, "Id", "256");
    data.tables.set("levels", 3, "Id", "279");
    let (p, f) = patched(&data, DataRules::Mod, &[]);
    assert!(f.is_empty(), "{f:?}");
    let set = compile(&data, &p, DataRules::Mod, "cols-levelid");
    let lv: Vec<Levels> = records(&set, "levels");
    assert_eq!((lv[2].id, lv[3].id), (0, 23));
    assert_eq!(lv[4].id, 4);
}
