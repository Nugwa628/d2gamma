use eframe::egui;
use mod_studio::input_script::Runner;
#[test]
fn imported_map_art_can_start_a_custom_level_and_exact_placements() {
    let root = std::env::temp_dir().join(format!("studio-map-input-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../templates/showcase/files/data");
    mod_studio::asset_library::import(&root, &source, false).unwrap();
    let original =
        std::fs::read(root.join("files/data/global/tiles/showcase/harbor.ds1.toml")).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    let script = serde_json::from_value(serde_json::json!({"steps":[
        {"do":"click","text":"Map editor"},
        {"do":"fill","text":"Source DS1","value":"showcase/harbor.ds1"},
        {"do":"fill","text":"Source DT1","value":"showcase/lantern.dt1"},
        {"do":"click","text":"Create level from project art"},
        {"do":"click","text":"Level rules and placements"},
        {"do":"click","text":"Add object"},
        {"do":"fill","text":"Object class","offset":[70,0],"value":"119"},
        {"do":"click","text":"Save"},
        {"do":"wait_text","text":"Saved.","seconds":2}
    ]}))
    .unwrap();
    let captures = root.with_extension("captures");
    let mut runner = Runner::new(script, &captures).unwrap();
    let ctx = egui::Context::default();
    for frame in 0..400 {
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
    }
    assert!(
        runner.finished,
        "step {}: {:?}",
        runner.step,
        runner.visible()
    );
    assert!(runner.error.is_none(), "{:?}", runner.error);
    let maps = d2_sim::drlg::maps::Maps::parse(
        &std::fs::read_to_string(root.join("files/levels.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!(maps.level.len(), 1);
    assert_eq!(maps.level[0].id, 137);
    assert_eq!(maps.level[0].size, [24, 24]);
    assert_eq!(maps.level[0].object[0].class, 119);
    assert!(!root.join("files/data/global/tiles/modkit").exists());
    assert_eq!(
        original,
        std::fs::read(root.join("files/data/global/tiles/showcase/harbor.ds1.toml")).unwrap()
    );
    assert!(mod_studio::maps::editor::Document::from_project_assets(
        &root,
        138,
        "showcase/lens.ds1",
        "showcase/lantern.dt1"
    )
    .is_err());
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(captures).unwrap();
}
