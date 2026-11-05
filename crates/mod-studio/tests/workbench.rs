//! Acceptance checks exercise the same engine entry points as the Studio workers.
mod workbench_support;
use mod_studio::{
    workbench::cube::{CubeInput, CubeOptions},
    workbench_ui::parse_inputs,
};
#[test]
fn tooltip_changes_with_unsaved_patched_damage() {
    let mut snapshot = workbench_support::snapshot();
    let before = snapshot
        .compile()
        .unwrap()
        .preview("weapons", 0, 0, 1)
        .unwrap();
    let table = snapshot
        .data
        .tables
        .iter_mut()
        .find(|t| t.name() == "weapons")
        .unwrap();
    let c = table.header.iter().position(|h| h == b"mindam").unwrap();
    table.rows[0].cells[c] = b"6".to_vec();
    let after = snapshot
        .compile()
        .unwrap()
        .preview("weapons", 0, 0, 1)
        .unwrap();
    assert_ne!(before, after);
    let text = after
        .iter()
        .map(|l| String::from_utf16_lossy(&l.text))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains('6'), "{text}");
}
#[test]
fn seeded_drop_counts_are_deterministic_and_use_patched_weights() {
    let mut snapshot = workbench_support::snapshot();
    let engine = snapshot.compile().unwrap();
    let tc = engine
        .tcs
        .tcs
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(b"Test pool"))
        .unwrap() as u16;
    let a = engine.drops(tc, 0, 10_000, 1234, 85).unwrap();
    assert_eq!(a, engine.drops(tc, 0, 10_000, 1234, 85).unwrap());
    assert!(a.keys().any(|(code, _)| code == "qa1"));
    assert!(a.keys().any(|(code, _)| code == "qa2"));
    let table = snapshot
        .data
        .tables
        .iter_mut()
        .find(|t| t.name() == "treasureclassex")
        .unwrap();
    let c = table.header.iter().position(|h| h == b"Prob2").unwrap();
    table.rows[0].cells[c] = b"0".to_vec();
    let b = snapshot
        .compile()
        .unwrap()
        .drops(tc, 0, 10_000, 1234, 85)
        .unwrap();
    assert!(!b.keys().any(|(code, _)| code == "qa2"));
    assert_ne!(a, b);
}
#[test]
fn cube_matches_a_recipe_and_makes_its_engine_output() {
    let engine = workbench_support::snapshot().compile().unwrap();
    let inputs = vec![CubeInput {
        code: "qa1".into(),
        quality: 2,
        level: 10,
        quantity: 1,
        sockets: 0,
        ethereal: false,
    }];
    let result = engine.cube(&inputs, &CubeOptions::default()).unwrap();
    assert!(result.contains("Recipe #0"), "{result}");
    assert!(
        result.contains("Test wand (qa2) · quality Normal"),
        "{result}"
    );
    assert!(result.contains("Committed: true"), "{result}");
    let bad = parse_inputs("qa2").unwrap();
    assert!(engine
        .cube(&bad, &CubeOptions::default())
        .unwrap()
        .contains("No recipe"));
}
#[test]
fn default_working_set_has_no_patch_and_labels_keep_custom_keys() {
    let s = workbench_support::snapshot();
    let editor = mod_studio::tables::Tables::from_data(s.data, "unused.d2patch".into());
    assert_eq!(editor.diff().unwrap(), b"d2patch 1\n");
    assert_eq!(mod_studio::labels::label("weapons", "mindam"), "Min damage");
    assert_eq!(
        mod_studio::labels::label("weapons", "custom_damage"),
        "custom_damage"
    );
    assert!(parse_inputs("qa1,nope").is_err());
}
#[test]
fn affix_and_runeword_previews_apply_engine_properties() {
    let engine = workbench_support::snapshot().compile().unwrap();
    let text = |table| {
        engine
            .preview(table, 0, 0, 1)
            .unwrap()
            .iter()
            .map(|line| String::from_utf16_lossy(&line.text))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let base = text("weapons");
    for table in ["magicprefix", "magicsuffix", "automagic", "runes"] {
        let preview = text(table);
        assert_ne!(preview, base, "{table}");
        assert!(preview.contains("7"), "{table}: {preview}");
        if table == "runes" {
            assert!(preview.contains("Testword"), "{preview}");
        }
    }
}
#[test]
fn drop_display_names_groups_percentages_and_defaults() {
    let engine = workbench_support::snapshot().compile().unwrap();
    let tc = engine
        .tcs
        .tcs
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(b"Test pool"))
        .unwrap() as u16;
    let default = engine.drops(tc, 0, 100, 1, 85).unwrap();
    assert_eq!(
        default,
        engine
            .drops_with_options(tc, 0, 100, 1, 85, &Default::default())
            .unwrap()
    );
    let counts = std::collections::BTreeMap::from([
        (("qa1".into(), 2), 12),
        (("qa2".into(), 2), 25),
        (("qa1".into(), 7), 3),
        ((String::new(), 0), 60),
    ]);
    let text = engine.format_drops(&counts, 100);
    assert!(text.contains("Test blade (qa1)"), "{text}");
    assert!(text.contains("Normal · 37 · 37.00%"));
    assert!(text.contains("Unique · 3 · 3.00%"));
    assert!(text.contains("Nothing dropped · 60 · 60.00%"));
    assert!(text.find("(qa2)").unwrap() < text.find("(qa1)").unwrap());
    assert!(text.contains('━'));
}
#[test]
fn players_and_mod_rules_use_engine_walk() {
    let mut snapshot = workbench_support::snapshot();
    let engine = snapshot.compile().unwrap();
    let tc = engine
        .tcs
        .tcs
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(b"Test pool"))
        .unwrap() as u16;
    let options = mod_studio::workbench::DropOptions {
        players: 8,
        magic_find: 500,
    };
    let base = engine.drops(tc, 0, 1000, 1, 85).unwrap();
    let more = engine
        .drops_with_options(tc, 0, 1000, 1, 85, &options)
        .unwrap();
    assert!(more[&(String::new(), 0)] < base[&(String::new(), 0)]);
    snapshot.drop_rules = d2_sim::rules::global::GlobalRules::from_toml(
        "drops.players_setting = false\ndrops.magic_find_percent = 0",
    )
    .unwrap()
    .drops;
    assert_eq!(
        base,
        snapshot
            .compile()
            .unwrap()
            .drops_with_options(tc, 0, 1000, 1, 85, &options)
            .unwrap()
    );
}
#[test]
fn all_item_fields_have_sections() {
    use mod_studio::labels::group;
    assert_eq!(group("namestr"), "Basics");
    assert_eq!(group("2handmindam"), "Damage");
    assert_eq!(group("reqstr"), "Requirements");
    assert_eq!(group("gemsockets"), "Sockets");
    assert_eq!(group("custom_field"), "Other");
}
#[test]
fn magic_find_uses_quality_rolls_and_mod_cap() {
    let mut snapshot = workbench_support::snapshot();
    let weapons = snapshot
        .data
        .tables
        .iter_mut()
        .find(|t| t.name() == "weapons")
        .unwrap();
    weapons.header.push(b"bitfield1".to_vec());
    for row in &mut weapons.rows {
        row.cells.push(b"1".to_vec());
    }
    let types = snapshot
        .data
        .tables
        .iter_mut()
        .find(|t| t.name() == "itemtypes")
        .unwrap();
    let normal = types
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(b"Normal"))
        .unwrap();
    for row in &mut types.rows {
        row.cells[normal] = b"0".to_vec();
    }
    let engine = snapshot.compile().unwrap();
    let tc = engine
        .tcs
        .tcs
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(b"Test pool"))
        .unwrap() as u16;
    let base = engine.drops(tc, 0, 2000, 1, 1).unwrap();
    let options = mod_studio::workbench::DropOptions {
        players: 1,
        magic_find: 1000,
    };
    assert_ne!(
        base,
        engine
            .drops_with_options(tc, 0, 2000, 1, 1, &options)
            .unwrap()
    );
    snapshot.drop_rules = d2_sim::rules::global::GlobalRules::from_toml("drops.magic_find_cap = 0")
        .unwrap()
        .drops;
    assert_eq!(
        base,
        snapshot
            .compile()
            .unwrap()
            .drops_with_options(tc, 0, 2000, 1, 1, &options)
            .unwrap()
    );
}
