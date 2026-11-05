use eframe::egui;
use mod_studio::{
    definitions::Document,
    input_script::{Runner, Script},
};
use std::path::PathBuf;
fn root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("studio-defs-{}-{name}", std::process::id()))
}
fn stats(root: &std::path::Path) -> Document {
    Document::open(root, "files/stats.toml", "version=1", |s| {
        d2_sim::stats::named::StatRules::from_toml(s)
            .map(|_| ())
            .map_err(Into::into)
    })
}
#[test]
fn definitions_preserve_absence_invalid_sources_and_external_edits() {
    let root = root("conflicts");
    let _ = std::fs::remove_dir_all(&root);
    let mut doc = stats(&root);
    doc.save().unwrap();
    assert!(!root.exists());
    doc.table = "version=1\n[[stat]]\nname='resonance'\nid=40000"
        .parse()
        .unwrap();
    doc.save().unwrap();
    let expected = std::fs::read(root.join("files/stats.toml")).unwrap();
    let mut doc = stats(&root);
    doc.save().unwrap();
    assert_eq!(
        std::fs::read(root.join("files/stats.toml")).unwrap(),
        expected
    );
    doc.table.get_mut("stat").unwrap().as_array_mut().unwrap()[0]
        .as_table_mut()
        .unwrap()
        .insert("id".into(), 40001.into());
    std::fs::write(root.join("files/stats.toml"), "version=99").unwrap();
    assert!(doc
        .save()
        .unwrap_err()
        .to_string()
        .contains("outside Studio"));
    let mut doc = stats(&root);
    assert!(doc.error.is_some());
    assert!(doc.save().is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("files/stats.toml")).unwrap(),
        "version=99"
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn named_stat_can_be_authored_from_the_empty_project_ui() {
    let root = root("input");
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    let script: Script = serde_json::from_value(serde_json::json!({"steps":[
        {"do":"click","text":"Named stats"},
        {"do":"click","text":"Add named stat"},
        {"do":"fill","text":"Stat name","value":"resonance"},
        {"do":"click","text":"Save"},
        {"do":"wait_text","text":"Saved.","seconds":1}
    ]}))
    .unwrap();
    let captures = root.with_extension("captures");
    let mut runner = Runner::new(script, &captures).unwrap();
    let ctx = egui::Context::default();
    for n in 0..150 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100., 800.),
            )),
            time: Some(n as f64 / 60.),
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
    let actual: toml::Table = std::fs::read_to_string(root.join("files/stats.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let expected: toml::Table = include_str!("../../../templates/showcase/files/stats.toml")
        .parse()
        .unwrap();
    assert_eq!(actual, expected);
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(captures).unwrap();
}

#[test]
fn four_difficulties_are_authored_in_order_through_egui() {
    let root = root("difficulty-input");
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    let script: Script = serde_json::from_value(serde_json::json!({"steps":[
        {"do":"click","text":"Difficulties"},
        {"do":"fill","text":"Difficulty 0","value":"Dawn"},
        {"do":"fill","text":"Difficulty 1","value":"Gale"},
        {"do":"fill","text":"Difficulty 2","value":"Dusk"},
        {"do":"click","text":"Add difficulty"},
        {"do":"fill","text":"Difficulty 3","value":"Eclipse"},
        {"do":"click","text":"Save"},
        {"do":"wait_text","text":"Saved.","seconds":1}
    ]}))
    .unwrap();
    drive(&mut studio, script, &root);
    let actual: toml::Table = std::fs::read_to_string(root.join("files/difficulties.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let expected: toml::Table = include_str!("../../../templates/showcase/files/difficulties.toml")
        .parse()
        .unwrap();
    assert_eq!(actual, expected);
    std::fs::remove_dir_all(root).unwrap();
}

fn drive(studio: &mut mod_studio::ui::Studio, script: Script, root: &std::path::Path) {
    let captures = root.with_extension("captures");
    let mut runner = Runner::new(script, &captures).unwrap();
    let ctx = egui::Context::default();
    for n in 0..600 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100., 800.),
            )),
            time: Some(n as f64 / 60.),
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
    std::fs::remove_dir_all(captures).unwrap();
}

#[test]
fn act_town_and_waypoints_are_authored_through_egui() {
    let root = root("act-input");
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    let script: Script = serde_json::from_value(serde_json::json!({"steps":[
    {"do":"click","text":"Acts & waypoints"},
    {"do":"click","text":"Add act"},
    {"do":"fill","text":"Act name","value":"Lantern Reach"},
    {"do":"fill","text":"Start level","value":"137","offset":[60,0]},
    {"do":"fill","text":"End level (exclusive)","value":"140","offset":[100,0]},
    {"do":"fill","text":"Town level","value":"137","offset":[60,0]},
    {"do":"click","text":"Add Waypoint"},
    {"do":"click","text":"Add Waypoint"},
    {"do":"fill","text":"Waypoint level","nth":1,"value":"138","offset":[65,0]},
    {"do":"click","text":"Add Waypoint"},
    {"do":"fill","text":"Waypoint level","nth":2,"value":"139","offset":[65,0]},
    {"do":"click","text":"Save"},
    {"do":"wait_text","text":"Saved.","seconds":1}
    ]}))
    .unwrap();
    drive(&mut studio, script, &root);
    let actual: toml::Table = std::fs::read_to_string(root.join("files/acts.toml"))
        .unwrap()
        .parse()
        .unwrap();
    let expected: toml::Table = include_str!("../../../templates/showcase/files/acts.toml")
        .parse()
        .unwrap();
    assert_eq!(actual, expected);
    std::fs::remove_dir_all(root).unwrap();
}
