//! Class copies use invented rows, never bundled install tables.
#[path = "kit_support/mod.rs"]
#[allow(dead_code)]
mod support;

use mod_studio::{kit::Kit, kit_ui::lift::player, tables::Tables};

fn set(t: &mut Tables, row: usize, column: &str, value: &str) {
    let table = t
        .baseline
        .tables
        .iter()
        .position(|t| t.name() == "skills")
        .unwrap();
    let col = t.baseline.tables[table]
        .header
        .iter()
        .position(|c| c.eq_ignore_ascii_case(column.as_bytes()))
        .unwrap();
    t.baseline.tables[table].rows[row].cells[col] = value.as_bytes().to_vec();
    t.edited.tables[table].rows[row].cells[col] = value.as_bytes().to_vec();
}

#[test]
fn class_copy_preserves_other_classes_unsupported_and_authored_rows() {
    for class in player::classes() {
        let root = std::env::temp_dir().join(format!(
            "player-skills-{}-{}",
            std::process::id(),
            class.code
        ));
        let mut kit = Kit::load(&root).unwrap();
        let mut t = support::tables(&root);
        set(&mut t, 5, "charclass", class.code);
        set(&mut t, 5, "srvdofunc", "0");
        set(&mut t, 5, "srvmissile", "firebolt");
        set(&mut t, 6, "charclass", class.code);
        // An unassigned slot stays unsupported as new base families land.
        set(&mut t, 6, "srvdofunc", "65535");
        set(&mut t, 7, "charclass", "not-this-class");
        set(&mut t, 7, "srvdofunc", "1");
        t.use_baseline_as_reference();
        let before = t.edited.clone();
        let rows = player::skills(&t, class);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].reason.is_none());
        assert!(rows[1]
            .reason
            .as_ref()
            .unwrap()
            .contains("do function 65535 is not lifted"));
        assert_eq!(t.edited, before, "listing is read-only");
        let copied = player::copy(&mut kit, &mut t, class);
        assert_eq!(copied.ids, [5]);
        assert_eq!(copied.skipped.len(), 1);
        assert_eq!(kit.skills.len(), 1);
        assert!(kit.texts().unwrap()[1].contains("missile"));
        // Copying again must never overwrite an authored entry or its row.
        kit.skills[0].name = "my edited spell".to_owned();
        let after = t.edited.clone();
        let again = player::copy(&mut kit, &mut t, class);
        assert!(again.ids.is_empty());
        assert_eq!(again.skipped.len(), 2);
        assert_eq!(kit.skills[0].name, "my edited spell");
        assert_eq!(t.edited, after);
    }
}

#[test]
fn sorceress_pack_is_available_and_uses_the_install_class_code() {
    let class = player::classes().find(|c| c.code == "sor").unwrap();
    assert_eq!(class.id, 1);
    assert_eq!(class.name, "Sorceress");
}
