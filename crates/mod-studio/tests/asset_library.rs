use std::path::PathBuf;
fn root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("studio-assets-{}-{name}", std::process::id()))
}
#[test]
fn showcase_art_library_round_trips_without_importing_gameplay_definitions() {
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates/showcase/files/data");
    let files = mod_studio::asset_library::library(&source).unwrap();
    assert_eq!(files.len(), 239);
    let root = root("import");
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    assert_eq!(
        mod_studio::asset_library::import(&root, &source, false).unwrap(),
        files.len()
    );
    for f in files {
        assert_eq!(
            std::fs::read(root.join("files").join(&f.path)).unwrap(),
            f.bytes
        );
    }
    assert!(!root.join("files/ai.toml").exists());
    assert!(mod_studio::asset_library::import(&root, &source, false).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn definitions_or_orphan_pixels_are_rejected_before_writing() {
    let source = root("invalid");
    let _ = std::fs::remove_dir_all(&source);
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("net.toml"), "version=1").unwrap();
    assert!(mod_studio::asset_library::library(&source).is_err());
    std::fs::remove_file(source.join("net.toml")).unwrap();
    std::fs::write(source.join("orphan.png"), b"bad").unwrap();
    assert!(mod_studio::asset_library::library(&source).is_err());
    std::fs::remove_dir_all(source).unwrap();
}

#[test]
fn asset_library_is_imported_through_egui_without_direct_project_writes() {
    use eframe::egui;
    use mod_studio::input_script::{Runner, Script};
    let root = root("input");
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../templates/showcase/files/data");
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    let script:Script=serde_json::from_value(serde_json::json!({"steps":[
 {"do":"click","text":"Import art & sound"},{"do":"click","text":"Native asset library"},{"do":"fill","text":"Source folder","value":source},
 {"do":"click","text":"Validate and import asset library"},{"do":"wait_text","text":"native asset files","seconds":10}]})).unwrap();
    let captures = root.with_extension("captures");
    let mut runner = Runner::new(script, &captures).unwrap();
    let ctx = egui::Context::default();
    for frame in 0..2000 {
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
            studio.draw(ctx);
            runner.observe(ctx).unwrap();
        });
        if runner.finished {
            break;
        }
        // This importer runs on a real worker thread, independently of the
        // synthetic egui clock. Give it wall time, even on fast CI repaints.
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        runner.finished,
        "step {}: {:?}",
        runner.step,
        runner.visible()
    );
    assert!(runner.error.is_none(), "{:?}", runner.error);
    assert!(root
        .join("files/data/global/tiles/showcase/harbor.ds1.toml")
        .is_file());
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(captures).unwrap();
}
#[test]
fn icon_import_preserves_png_bytes_and_refuses_invalid_or_existing_destinations() {
    let root = root("icon");
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../templates/showcase/files/icons/lantern.png");
    assert!(mod_studio::asset_library::icon(&root, &source, "../escape.png", false).is_err());
    mod_studio::asset_library::icon(&root, &source, "icons/lantern.png", false).unwrap();
    assert_eq!(
        std::fs::read(&source).unwrap(),
        std::fs::read(root.join("files/icons/lantern.png")).unwrap()
    );
    assert!(mod_studio::asset_library::icon(&root, &source, "icons/lantern.png", false).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
