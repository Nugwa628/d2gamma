//! Patch persistence and schema editing use invented tables only.
mod support;
use d2_data::patch;
use mod_studio::tables::{validate_cell, Tables};
#[test]
fn edit_file_reload_round_trip_keeps_a_patch() {
    let base = support::data();
    let dir = std::env::temp_dir().join(format!("studio-table-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("edit.d2patch");
    let mut editor = Tables::from_data(base.clone(), path.clone());
    assert!(editor.problems.is_empty());
    editor.edited.tables[0].rows[0].cells[4] = b"7".to_vec();
    editor.dirty = true;
    editor.save().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"d2patch 1"));
    assert!(!String::from_utf8_lossy(&bytes).contains("Frost spear"));
    let (layer, errors) = patch::parse_layer("edit", &bytes, 0);
    assert!(errors.is_empty());
    let mut reloaded = base.clone();
    assert!(!patch::has_errors(&patch::apply_stack(
        &mut reloaded,
        &[layer],
        "test"
    )));
    assert_eq!(
        editor.edited.tables[0].render(),
        reloaded.tables[0].render()
    );
    assert_eq!(base.tables[0].rows[0].cells[4], b"3");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
#[test]
fn reference_dropdown_lists_readable_rows_and_searches_all_tables() {
    let mut editor = Tables::from_data(support::data(), "unused".into());
    let column = editor.column(0, 2).unwrap();
    let choices = editor.choices(column);
    assert!(choices
        .iter()
        .any(|(v, n)| v == "blad" && n.contains("Blades")));
    assert_eq!(editor.search("Frost spear")[0].0, 0);
    assert_eq!(editor.search("Staves")[0].0, 1);
    editor.disable(1, 1).unwrap();
    assert!(!editor
        .choices(editor.column(0, 2).unwrap())
        .iter()
        .any(|(v, _)| v == "sper"));
}
#[test]
fn disable_keeps_slot_and_warns_about_incoming_references() {
    let mut editor = Tables::from_data(support::data(), "unused".into());
    editor.disable(1, 0).unwrap();
    assert!(editor.edited.tables[1].disabled(0));
    assert_eq!(editor.edited.tables[1].rows.len(), 3);
    assert!(
        editor
            .problems
            .iter()
            .any(|s| s.contains("weapons") && s.contains("type")),
        "{:?}",
        editor.problems
    );
    assert!(editor.save().is_err());
    editor.edited.tables[0].rows[0].cells[2] = b"sper".to_vec();
    editor.validate();
    assert!(editor.problems.is_empty(), "{:?}", editor.problems);
    assert!(String::from_utf8(editor.diff().unwrap())
        .unwrap()
        .contains("remove blad"));
}
#[test]
fn duplicate_and_add_validate_unique_keys_and_ranges() {
    let mut editor = Tables::from_data(support::data(), "unused".into());
    assert!(editor.add(0, Some(0), "eb1").is_err());
    let r = editor.add(0, Some(0), "eb2").unwrap();
    assert_eq!(editor.edited.tables[0].rows[r].cells[4], b"3");
    editor.validate();
    assert!(editor.problems.is_empty(), "{:?}", editor.problems);
    assert!(validate_cell(editor.column(0, 3), b"256").is_err());
    assert!(validate_cell(editor.column(0, 3), b"no").is_err());
    assert!(validate_cell(editor.column(0, 3), b"12").is_ok());
    editor.edited.tables[0].rows[0].cells[3] = b"999999999999".to_vec();
    editor.validate();
    assert!(!editor.problems.is_empty());
}
#[test]
fn untouched_tables_have_no_diff_or_problems() {
    let mut editor = Tables::from_data(support::data(), "unused".into());
    editor.validate();
    assert!(editor.problems.is_empty(), "{:?}", editor.problems);
    assert_eq!(editor.diff().unwrap(), b"d2patch 1\n");
    assert_eq!(editor.schemas.len(), 86);
}
#[test]
fn native_install_and_existing_author_patches_round_trip_without_copies() {
    let root = std::env::temp_dir().join(format!("studio-install-{}", std::process::id()));
    let native = root.join("native");
    let excel = native.join("base/data/global/excel");
    std::fs::create_dir_all(&excel).unwrap();
    for rule in patch::rules_for(d2_data::bin::DataRules::Mod) {
        let header = rule.key_field.as_deref().unwrap_or(b"name");
        let mut bytes = header.to_vec();
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(rule.name.as_bytes());
        bytes.extend_from_slice(b"\r\n");
        std::fs::write(excel.join(rule.txt_name), bytes).unwrap();
    }
    for table in support::data().tables {
        std::fs::write(excel.join(&table.rules.txt_name), table.render()).unwrap();
    }
    let project = root.join("project");
    mod_studio::create_project(&project, &mod_studio::default_manifest(), 0).unwrap();
    let author = project.join("files/patches/author.d2patch");
    let author_bytes = b"d2patch 1\ntable weapons\nset eb1 mindam 3 -> 5\n";
    std::fs::write(&author, author_bytes).unwrap();
    let mut editor = Tables::load(&native, &root.join("mods"), &project).unwrap();
    let t = editor
        .edited
        .tables
        .iter()
        .position(|t| t.name() == "weapons")
        .unwrap();
    assert_eq!(editor.edited.tables[t].rows[0].cells[4], b"5");
    assert!(editor.changed(t, 0, 4));
    editor.edited.tables[t].rows[0].cells[4] = b"7".to_vec();
    editor.dirty = true;
    editor.save().unwrap();
    let loaded = Tables::load(&native, &root.join("mods"), &project).unwrap();
    assert_eq!(loaded.edited.tables[t].rows[0].cells[4], b"7");
    assert_eq!(std::fs::read(&author).unwrap(), author_bytes);
    assert!(project.join("files/patches/~studio.d2patch").is_file());
    assert!(!project.join("files/data/global/excel/weapons.txt").exists());
    std::fs::write(&author, b"d2patch 1\n").unwrap();
    assert!(editor
        .save()
        .unwrap_err()
        .to_string()
        .contains("outside Studio"));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn new_row_can_be_disabled_without_renumbering() {
    let mut editor = Tables::from_data(support::data(), "unused".into());
    let r = editor.add(0, Some(0), "new1").unwrap();
    editor.disable(0, r).unwrap();
    assert!(editor.edited.tables[0].disabled(r));
    assert!(editor.problems.is_empty(), "{:?}", editor.problems);
    let (layer, errors) = patch::parse_layer("new", &editor.diff().unwrap(), 0);
    assert!(errors.is_empty());
    let mut data = support::data();
    assert!(!patch::has_errors(&patch::apply_stack(
        &mut data,
        &[layer],
        "test"
    )));
    assert_eq!(data.tables[0].rows.len(), 4);
    assert!(data.tables[0].disabled(3));
}
#[test]
fn cell_inspector_and_string_labels_round_trip_from_egui_input() {
    use eframe::egui;
    use mod_studio::{input_script::Runner, table_ui::TableEditor};
    let root = std::env::temp_dir().join(format!("studio-cell-input-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let base = support::data();
    let mut editor = TableEditor::default();
    editor.data = Some(Tables::from_data(base.clone(), root.join("edit.d2patch")));
    let script = serde_json::from_value(serde_json::json!({"steps":[
        {"do":"fill","text":"Selected row","offset":[80,0],"value":"1"},{"do":"key","key":"Enter"},
        {"do":"fill","text":"Edit column","value":"mindam"},
        {"do":"fill","text":"Cell value","value":"9","offset":[90,0]},
        {"do":"click","text":"Strings and UI labels"},
        {"do":"fill","text":"String key","value":"lantern_hint"},
        {"do":"fill","text":"String text","value":"Carry the light."},
        {"do":"click","text":"Add / update string"}
    ]}))
    .unwrap();
    let mut runner = Runner::new(script, &root.join("captures")).unwrap();
    let ctx = egui::Context::default();
    for frame in 0..300 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100., 800.),
            )),
            time: Some(frame as f64 / 60.),
            ..Default::default()
        };
        runner.input(&mut input);
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| editor.draw(ui));
            runner.observe(ctx).unwrap();
        });
        if runner.finished {
            break;
        }
    }
    assert!(
        runner.finished,
        "step {}: {:?}",
        runner.step,
        runner.visible()
    );
    assert!(runner.error.is_none(), "{:?}", runner.error);
    let data = editor.data.as_mut().unwrap();
    assert_eq!(data.edited.tables[0].rows[1].cells[4], b"9");
    data.edit_string("lantern_hint", "Carry the lantern.", false)
        .unwrap();
    assert!(data.edit_string("", "bad", false).is_err());
    data.save().unwrap();
    let bytes = std::fs::read(root.join("edit.d2patch")).unwrap();
    let (layer, errors) = patch::parse_layer("saved", &bytes, 0);
    assert!(errors.is_empty(), "{errors:?}");
    let mut loaded = base;
    let diagnostics = patch::apply_stack(&mut loaded, &[layer], "test");
    assert!(!patch::has_errors(&diagnostics), "{diagnostics:?}");
    assert_eq!(loaded.tables[0].rows[1].cells[4], b"9");
    assert_eq!(loaded.strings.len(), 1);
    assert!(
        matches!(&loaded.strings[0].edit,d2_data::strings::StringEdit::Add{key,text} if key==b"lantern_hint" && text==b"Carry the lantern.")
    );
    std::fs::remove_dir_all(root).unwrap();
}
