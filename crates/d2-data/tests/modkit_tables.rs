//! Synthetic mod-table acceptance tests; no game data is embedded.
use d2_data::{
    bin::DataRules,
    compile::{compile_table, Linkers, StdCallbacks},
    patch::*,
    schema::{self, FieldDef, FieldType, Link},
    strings::StringTables,
    txt::TxtTable,
};
use sha2::{Digest, Sha256};

fn table(name: &str, text: &[u8]) -> PatchTable {
    let rule = rules_for(DataRules::Mod)
        .into_iter()
        .find(|r| r.name == name)
        .unwrap();
    PatchTable::from_base(&rule, "synthetic", text).unwrap()
}
fn base() -> PatchData {
    PatchData::from_tables(vec![table(
        "itemtypes",
        b"code\tequiv1\trepair\r\nax1\t\t1\r\nax2\tax1\t1\r\nax3\t\t1\r\n",
    )])
}

#[test]
fn callback_expressions_reject_removed_targets_and_allow_builtin_words() {
    let base = PatchData::from_tables(vec![
        table("itemtypes", b"code\r\nax1\r\nax2\r\n"),
        table("cubemain", b"description\tinput 1\toutput\r\nrecipe\t\"ax1,qty=2\"\tax1\r\nbuiltin\tany\tuseitem\r\n"),
        table("compcode", b"code\r\nlit\r\nmed\r\n"),
        table("monstats2", b"Id\tHDv\r\ntest\t\"lit,med\"\r\n"),
        table("monstats", b"Id\r\nmonster\r\n"),
        table("monpreset", b"Act\tPlace\r\n1\tmonster\r\n"),
    ]);
    assert!(references(&base).is_empty(), "{:?}", references(&base));
    let mut patched = base.clone();
    let (layer, errors) = parse_layer("test.d2patch", b"d2patch 1\ntable itemtypes\nremove ax1\ntable compcode\nremove med\ntable monstats\nremove monster\n", 1);
    assert!(errors.is_empty());
    assert!(!has_errors(&apply_stack(
        &mut patched,
        &[layer],
        "test.d2stack"
    )));
    let errors = check_references(&base, &patched);
    assert_eq!(errors.len(), 4, "{errors:?}");
    for (table, column) in [
        ("cubemain", b"input 1".as_slice()),
        ("cubemain", b"output"),
        ("monstats2", b"HDv"),
        ("monpreset", b"Place"),
    ] {
        assert!(
            errors
                .iter()
                .any(|f| f.table.as_deref() == Some(table) && f.column.as_deref() == Some(column)),
            "{errors:?}"
        );
    }
}

#[test]
fn cube_output_numeric_affixes_and_monster_skillmode_targets_are_checked() {
    let base = PatchData::from_tables(vec![
        table("magicprefix", b"Name\r\nnone\r\nprefix\r\n"),
        table(
            "cubemain",
            b"description\toutput\r\nrecipe\t\"useitem,pre=2\"\r\n",
        ),
        table("monseq", b"sequence\r\nseq\r\n"),
        table(
            "monstats",
            b"Id\tSkill1\tSk1mode\r\nmonster\tskill\tseq\r\ninactive\t\tseq\r\n",
        ),
        table("skills", b"skill\r\nskill\r\n"),
    ]);
    assert!(references(&base).is_empty(), "{:?}", references(&base));
    let mut patched = base.clone();
    let (layer, errors) = parse_layer(
        "test.d2patch",
        b"d2patch 1\ntable magicprefix\nremove prefix\ntable monseq\nremove seq\n",
        1,
    );
    assert!(errors.is_empty());
    assert!(!has_errors(&apply_stack(
        &mut patched,
        &[layer],
        "test.d2stack"
    )));
    let errors = check_references(&base, &patched);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors
        .iter()
        .any(|f| f.table.as_deref() == Some("cubemain")));
    assert!(errors
        .iter()
        .any(|f| f.table.as_deref() == Some("monstats") && f.row.as_ref().unwrap().0 == 0));
}

#[test]
fn callback_target_priority_matches_the_compiler_even_with_disabled_keys() {
    let base = PatchData::from_tables(vec![
        table("itemtypes", b"code\r\naxe\r\n"),
        table("weapons", b"name\tcode\r\nitem\taxe\r\n"),
        table(
            "cubemain",
            b"description\tinput 1\toutput\r\nrecipe\taxe\taxe\r\n",
        ),
    ]);
    assert!(references(&base).is_empty());
    let mut patched = base.clone();
    assert!(apply(&mut patched, "remove axe").is_empty());
    let errors = check_references(&base, &patched);
    assert_eq!(errors.len(), 1, "{errors:?}");
    // Input prefers item types; output prefers items, which remain active.
    assert_eq!(errors[0].column.as_deref(), Some(b"input 1".as_slice()));
}

#[test]
fn cube_affix_id_boundaries_report_errors_without_panicking() {
    let data = PatchData::from_tables(vec![
        table("magicprefix", b"Name\r\nprefix\r\n"),
        table("cubemain", b"description\toutput\r\nnegative\t\"useitem,pre=-2147483648\"\r\nlarge\t\"useitem,pre=2147483647\"\r\nlast_valid\t\"useitem,pre=1\"\r\n"),
    ]);
    let errors = references(&data);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors.iter().all(|e| e.row.as_ref().unwrap().0 < 2));
}
fn apply(data: &mut PatchData, body: &str) -> Vec<Finding> {
    let (layer, findings) = parse_layer(
        "test.d2patch",
        format!("d2patch 1\ntable itemtypes\n{body}\n").as_bytes(),
        1,
    );
    assert!(findings.is_empty(), "{findings:?}");
    apply_stack(data, &[layer], "test.d2stack")
}
#[test]
fn remove_preserves_ids_and_reports_every_reference() {
    let base = base();
    let mut patched = base.clone();
    assert!(!has_errors(&apply(&mut patched, "remove ax1")));
    let t = patched.table("itemtypes").unwrap();
    assert_eq!(t.rows.len(), 3);
    assert_eq!(t.key(2), b"ax3");
    assert!(t.disabled(0));
    let errors = check_references(&base, &patched);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].table.as_deref(), Some("itemtypes"));
    assert_eq!(errors[0].row.as_ref().unwrap().0, 1);
    assert_eq!(errors[0].column.as_deref(), Some(b"equiv1".as_slice()));
    assert_eq!(errors[0].found.as_deref(), Some(b"ax1".as_slice()));
    assert!(!has_errors(&apply(
        &mut patched,
        "set ax2 equiv1 ax1 -> []"
    )));
    assert!(check_references(&base, &patched).is_empty());
    assert!(!has_errors(&apply(&mut patched, "add #3 ax4")));
    assert_eq!(patched.table("itemtypes").unwrap().key(3), b"ax4");
}
#[test]
fn disabled_slots_compile_and_round_trip_without_shifting_linker_ids() {
    let mut patched = base();
    assert!(!has_errors(&apply(&mut patched, "disable ax1")));
    let t = patched.table("itemtypes").unwrap();
    let rendered = t.render();
    let restored = table("itemtypes", &rendered);
    assert!(restored.disabled(0));
    let txt = TxtTable::parse("test", &rendered).unwrap();
    let fields = vec![
        FieldDef::new(
            "code",
            FieldType::AsciiToCode,
            0,
            0,
            Link::Linker("itemtypes.code".into()),
        ),
        FieldDef::new("repair", FieldType::Dword, 0, 4, Link::None),
    ];
    let mut linkers = Linkers::default();
    let strings = StringTables::default();
    let compiled = compile_table(
        "test",
        &txt,
        &fields,
        8,
        &mut linkers,
        &mut StdCallbacks::new(&strings),
    )
    .unwrap();
    assert_eq!(compiled.count, 3);
    assert_eq!(&compiled.records[4..8], &[0; 4]);
    assert_eq!(&compiled.records[20..24], &1u32.to_le_bytes());
    assert_eq!(
        linkers
            .code("itemtypes.code")
            .unwrap()
            .find(u32::from_le_bytes(*b"ax3 ")),
        Some(2)
    );
    let original = base();
    let diff = diff_tables(&[(original.table("itemtypes").unwrap(), &rendered)], false).unwrap();
    assert!(String::from_utf8_lossy(&diff).contains("remove ax1"));
    let (layer, errors) = parse_layer("diff.d2patch", &diff, 1);
    assert!(errors.is_empty());
    let mut replay = original;
    assert!(!has_errors(&apply_stack(
        &mut replay,
        &[layer],
        "diff.d2stack"
    )));
    assert_eq!(replay.table("itemtypes").unwrap().render(), rendered);
    assert!(has_errors(&apply(&mut patched, "remove ax1")));
    assert!(has_errors(&apply(&mut patched, "set ax1 repair 1 -> 2")));
    assert!(has_errors(&apply(
        &mut patched,
        "addcol __d2_disabled int 1"
    )));
}
#[test]
fn all_broken_values_are_reported_even_if_the_base_cell_was_already_unresolved() {
    let mut base = base();
    base.tables[0].rows[0].cells[1] = b"old_missing".to_vec();
    let mut patched = base.clone();
    assert!(!has_errors(&apply(
        &mut patched,
        "set ax1 equiv1 old_missing -> new_missing\nset ax3 equiv1 [] -> absent"
    )));
    let errors = check_references(&base, &patched);
    assert_eq!(errors.len(), 2);
    assert!(errors
        .iter()
        .all(|f| f.column.as_deref() == Some(b"equiv1".as_slice())));
    assert!(check_references(&base, &base).is_empty());
}
#[test]
fn id_exhaustion_is_an_error_and_never_reuses_a_disabled_slot() {
    for (name, limit) in [
        ("itemtypes", 65_535),
        ("states", 65_535),
        ("itemstatcost", 65_535),
    ] {
        assert_eq!(DataRules::Mod.max_count(name), Some(limit));
        let mut data = base();
        data.tables[0].rules.name = name.into();
        let row = data.tables[0].rows[0].clone();
        data.tables[0].rows.resize(limit, row);
        let (layer, errors) = parse_layer(
            "limit.d2patch",
            format!("d2patch 1\ntable {name}\nadd #{limit} new\n").as_bytes(),
            1,
        );
        assert!(errors.is_empty());
        let errors = apply_stack(&mut data, &[layer], "limit.d2stack");
        assert!(errors
            .iter()
            .any(|f| f.code == Code::A23 && f.to_string().contains("id space exhausted")));
        assert_eq!(data.tables[0].rows.len(), limit);
    }
}
#[test]
fn schema_export_is_in_sync() {
    let exported = schema::export_tables();
    assert_eq!(exported.len(), rules().len());
    assert!(exported["soundenviron"].contains("served_as_text\":true"));
    let golden: String = exported
        .iter()
        .map(|(name, json)| format!("{name} {:x}\n", Sha256::digest(json.as_bytes())))
        .collect();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/schema-golden.sha256");
    if std::env::var_os("UPDATE_MODKIT_SCHEMA").is_some() {
        std::fs::write(&path, &golden).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        golden,
        "regenerate intentionally with UPDATE_MODKIT_SCHEMA=1"
    );
}

#[test]
fn narrow_reference_ids_fail_clearly_before_truncation() {
    let txt = TxtTable::parse("synthetic", b"ref\r\ntarget\r\n").unwrap();
    let mut linkers = Linkers::default();
    let linker = linkers
        .ensure("test.code", schema::LinkerKind::Code)
        .unwrap();
    let d2_data::compile::Linker::Code(codes) = linker else {
        unreachable!()
    };
    for i in 0..255 {
        codes.add(i);
    }
    codes.add(u32::from_le_bytes(*b"targ"));
    let fields = [FieldDef::new(
        "ref",
        FieldType::CodeToByte,
        0,
        0,
        Link::Linker("test.code".into()),
    )];
    let strings = StringTables::default();
    let error = d2_data::compile::compile_table_with(
        "synthetic",
        &txt,
        &fields,
        1,
        &mut linkers,
        &mut StdCallbacks::new(&strings),
        DataRules::Mod,
    )
    .unwrap_err();
    assert!(error.to_string().contains("id space exhausted"), "{error}");
}

#[test]
fn removed_treasure_class_retains_later_hand_built_ids() {
    let t = table(
        "treasureclassex",
        b"Treasure Class\tPicks\r\nfirst\t1\r\nmiddle\t1\r\nlast\t1\r\n",
    );
    let mut data = PatchData::from_tables(vec![t]);
    let (layer, errors) = parse_layer(
        "test.d2patch",
        b"d2patch 1\ntable treasureclassex\nremove middle\n",
        1,
    );
    assert!(errors.is_empty());
    assert!(!has_errors(&apply_stack(
        &mut data,
        &[layer],
        "test.d2stack"
    )));
    let def = schema::schema().table("treasureclassex").unwrap();
    // Only identity and picks are needed to exercise allocation; the full
    // callback list requires the surrounding game's item and stat linkers.
    let fields: Vec<_> = def
        .fields
        .iter()
        .filter(|f| f.column == b"treasure class" || f.column == b"picks")
        .cloned()
        .collect();
    let rendered = data.tables[0].render();
    let txt = TxtTable::parse("synthetic", &rendered).unwrap();
    let strings = StringTables::default();
    let compiled = compile_table(
        "synthetic",
        &txt,
        &fields,
        def.record_size,
        &mut Linkers::default(),
        &mut StdCallbacks::new(&strings),
    )
    .unwrap();
    let tc = d2_data::compile::tc_linker([], compiled.records.chunks_exact(compiled.record_size))
        .unwrap();
    assert_eq!(tc.find(b"last"), Some(3));
    assert_eq!(
        &compiled.records[def.record_size + 32..def.record_size + 36],
        &[0; 4]
    );
}

#[test]
fn raw_item_codes_and_numeric_level_ids_are_references() {
    let base = PatchData::from_tables(vec![
        table("weapons", b"name\tcode\r\nAxe\tax1\r\n"),
        table("gamble", b"code\r\nax1\r\n"),
        table("levels", b"Name\tId\r\nTown\t1\r\nField\t2\r\n"),
        table("lvlprest", b"Name\tDef\tLevelId\r\nField preset\t1\t2\r\n"),
    ]);
    assert!(references(&base).is_empty());
    let mut patched = base.clone();
    let (layer, errors) = parse_layer(
        "refs.d2patch",
        b"d2patch 1\ntable weapons\nremove ax1\ntable levels\nremove [Field]\n",
        1,
    );
    assert!(errors.is_empty());
    assert!(!has_errors(&apply_stack(
        &mut patched,
        &[layer],
        "refs.d2stack"
    )));
    let errors = check_references(&base, &patched);
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors
        .iter()
        .any(|f| f.table.as_deref() == Some("lvlprest")
            && f.found.as_deref() == Some(b"2".as_slice())));
    assert!(errors
        .iter()
        .any(|f| f.table.as_deref() == Some("gamble")
            && f.found.as_deref() == Some(b"ax1".as_slice())));
}
