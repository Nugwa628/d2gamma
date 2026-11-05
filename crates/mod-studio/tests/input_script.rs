//! These checks drive egui input, rather than calling project/editor helpers.
use eframe::egui;
use mod_studio::input_script::{Runner, Script};
use std::path::PathBuf;

fn root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("studio-input-{}-{name}", std::process::id()))
}
#[test]
fn input_creates_a_project_using_the_wizard() {
    let root = root("wizard");
    let project = root.join("project");
    let _ = std::fs::remove_dir_all(&root);
    let script: Script = serde_json::from_value(serde_json::json!({"steps":[
        {"do":"click","text":"New mod"},
        {"do":"fill","text":"Name","value":"Input Lantern"},
        {"do":"fill","text":"ID","value":"input-lantern"},
        {"do":"fill","text":"Author","value":"Nugwa"},
        {"do":"fill","text":"Destination folder","offset":[150,0],"value":project},
        {"do":"click","text":"Create project"},
        {"do":"wait_text","text":"Manifest valid","seconds":2}
    ]}))
    .unwrap();
    let mut runner = Runner::new(script, &root.join("captures")).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    let ctx = egui::Context::default();
    for frame in 0..200 {
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
    assert!(runner.finished, "step {}", runner.step);
    assert!(runner.error.is_none(), "{:?}", runner.error);
    let manifest =
        d2_modkit::Manifest::parse(&std::fs::read_to_string(project.join("mod.toml")).unwrap())
            .unwrap();
    assert_eq!(manifest.id, "input-lantern");
    assert_eq!(manifest.name, "Input Lantern");
    assert_eq!(manifest.author, "Nugwa");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn scripts_reject_unknown_operations_keys_and_capture_paths() {
    assert!(
        serde_json::from_str::<Script>(r#"{"steps":[{"do":"write_project","value":"bad"}]}"#)
            .is_err()
    );
    for step in [
        serde_json::json!({"do":"key","key":"not-a-key"}),
        serde_json::json!({"do":"capture","name":"../escape"}),
    ] {
        let script = serde_json::from_value(serde_json::json!({"steps":[step]})).unwrap();
        assert!(Runner::new(script, &root("invalid")).is_err());
    }
}

#[test]
fn asset_heavy_project_keeps_editor_navigation_visible() {
    let root = root("navigation");
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    std::fs::create_dir_all(root.join("files/icons")).unwrap();
    for n in 0..260 {
        std::fs::write(
            root.join(format!("files/icons/item-{n}.png")),
            include_bytes!("../../../templates/showcase/files/icons/lantern.png"),
        )
        .unwrap();
    }
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    let ctx = egui::Context::default();
    let script: Script = serde_json::from_value(
        serde_json::json!({"steps":[{"do":"click","text":"Network values & actions"},{"do":"wait_text","text":"Values flow from the server","seconds":1}]}),
    )
    .unwrap();
    let captures = root.with_extension("captures");
    let mut runner = Runner::new(script, &captures).unwrap();
    for frame in 0..80 {
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
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(captures).unwrap();
}

#[test]
fn online_features_can_be_disabled_in_the_manifest_form() {
    let root = root("online-switches");
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    let script:Script=serde_json::from_value(serde_json::json!({"steps":[
 {"do":"click","text":"Enable online market"},{"do":"click","text":"Enable online ladder"},{"do":"click","text":"Save"},{"do":"wait_text","text":"Saved.","seconds":1}]})).unwrap();
    let captures = root.with_extension("captures");
    let mut runner = Runner::new(script, &captures).unwrap();
    let ctx = egui::Context::default();
    for frame in 0..160 {
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
    let manifest =
        d2_modkit::Manifest::parse(&std::fs::read_to_string(root.join("mod.toml")).unwrap())
            .unwrap();
    assert!(!manifest.market && !manifest.ladder);
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(captures).unwrap();
}
#[test]
fn popup_text_takes_priority_over_matching_text_behind_it() {
    let output = root("popup-target");
    let script =
        serde_json::from_value(serde_json::json!({"steps":[{"do":"click","text":"Choice"}]}))
            .unwrap();
    let mut runner = Runner::new(script, &output).unwrap();
    let ctx = egui::Context::default();
    let mut background = false;
    let mut foreground = false;
    for frame in 0..50 {
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
            egui::Area::new("back".into())
                .fixed_pos(egui::pos2(20., 100.))
                .show(ctx, |ui| {
                    background |= ui.button("Choice").clicked();
                });
            egui::Area::new("front".into())
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(200., 200.))
                .show(ctx, |ui| {
                    foreground |= ui.button("Choice").clicked();
                });
            runner.observe(ctx).unwrap();
        });
        if runner.finished {
            break;
        }
    }
    assert!(
        runner.finished && runner.error.is_none(),
        "{:?}",
        runner.error
    );
    assert!(foreground && !background);
    std::fs::remove_dir_all(output).unwrap();
}
#[test]
fn scrolling_to_text_uses_bounded_wheel_input() {
    let output = root("scroll-target");
    let script=serde_json::from_value(serde_json::json!({"steps":[{"do":"scroll_to","text":"Row 80","point":[300,300],"delta":[0,-180]},{"do":"click","text":"Row 80"}]})).unwrap();
    let mut runner = Runner::new(script, &output).unwrap();
    let ctx = egui::Context::default();
    let mut clicked = false;
    for frame in 0..800 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600., 500.),
            )),
            time: Some(frame as f64 / 60.),
            ..Default::default()
        };
        runner.input(&mut input);
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for n in 0..100 {
                            if ui.button(format!("Row {n}")).clicked() && n == 80 {
                                clicked = true;
                            }
                        }
                    });
            });
            runner.observe(ctx).unwrap();
        });
        if runner.finished {
            break;
        }
    }
    assert!(
        runner.finished && runner.error.is_none(),
        "step {}: {:?}",
        runner.step,
        runner.error
    );
    assert!(clicked);
    std::fs::remove_dir_all(output).unwrap();
}

#[test]
fn wrapped_label_targets_exclude_leading_layout_indentation() {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(600., 500.),
        )),
        ..Default::default()
    };
    let mut texts = Vec::new();
    let _ = ctx.run(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(120.);
                ui.label("per stack");
            });
        });
        texts = mod_studio::input_script::painted_text(ctx);
    });
    let label = texts.iter().find(|t| t.text == "per stack").unwrap();
    assert!(label.bounds[0] > 120., "{:?}", label.bounds);
}

#[test]
fn filling_an_empty_value_clears_the_selected_text() {
    let output = root("empty-fill");
    let script = serde_json::from_value(serde_json::json!({"steps":[
        {"do":"fill","text":"Executable","offset":[100,0],"value":""}
    ]}))
    .unwrap();
    let mut runner = Runner::new(script, &output).unwrap();
    let ctx = egui::Context::default();
    let mut value = String::from("/work/target/debug/D2Gamma-Game");
    for frame in 0..100 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(600., 500.),
            )),
            time: Some(frame as f64 / 60.),
            ..Default::default()
        };
        runner.input(&mut input);
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Executable");
                    ui.text_edit_singleline(&mut value);
                });
            });
            runner.observe(ctx).unwrap();
        });
        if runner.finished {
            break;
        }
    }
    assert!(runner.finished, "step {}", runner.step);
    assert!(runner.error.is_none(), "{:?}", runner.error);
    assert!(value.is_empty(), "{value}");
    std::fs::remove_dir_all(output).unwrap();
}

#[test]
fn finished_script_closes_without_drawing_another_interactive_frame() {
    use mod_studio::input_script::ScriptApp;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    let root = root("finish-close");
    let runner = Runner::new(Script { steps: Vec::new() }, &root).unwrap();
    let failed = Arc::new(AtomicBool::new(false));
    let mut app = ScriptApp {
        studio: Default::default(),
        runner,
        failed: failed.clone(),
    };
    let ctx = egui::Context::default();
    for _ in 0..8 {
        let _ = ctx.run(Default::default(), |ctx| app.draw(ctx));
        if app.runner.finished {
            break;
        }
    }
    assert!(app.runner.finished);
    assert!(!failed.load(Ordering::Relaxed));
    let output = ctx.run(Default::default(), |ctx| {
        app.draw(ctx);
        assert!(
            mod_studio::input_script::painted_text(ctx).is_empty(),
            "a completed run redrew Studio and can reopen its close dialog"
        );
    });
    let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
    assert!(commands
        .iter()
        .any(|c| matches!(c, egui::ViewportCommand::Close)));
    assert!(!commands
        .iter()
        .any(|c| matches!(c, egui::ViewportCommand::CancelClose)));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn input_saves_two_independent_quests() {
    let root = root("two-quests");
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    let script = serde_json::from_value(serde_json::json!({"steps":[
        {"do":"click","text":"Quests"},
        {"do":"click","text":"Add quest"},
        {"do":"fill","text":"Name","offset":[150,0],"value":"First quest"},
        {"do":"click","text":"Save"},
        {"do":"click","text":"Add quest"},
        {"do":"fill","text":"Name","offset":[150,0],"value":"Second quest"},
        {"do":"click","text":"Save"}
    ]}))
    .unwrap();
    let captures = root.with_extension("captures");
    let mut runner = Runner::new(script, &captures).unwrap();
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
            studio.draw(ctx);
            runner.observe(ctx).unwrap();
        });
        if runner.finished {
            break;
        }
    }
    assert!(
        runner.finished && runner.error.is_none(),
        "step {}: {:?}",
        runner.step,
        runner.error
    );
    let saved = mod_studio::kit::Kit::load(&root).unwrap();
    assert_eq!(saved.quests.len(), 2);
    assert_eq!(saved.quests[0].name, "First quest");
    assert_eq!(saved.quests[1].name, "Second quest");
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(captures).unwrap();
}
#[test]
fn input_pacing_uses_time_when_repaint_frames_arrive_quickly() {
    let output = root("input-pacing");
    let script = serde_json::from_value(serde_json::json!({"steps":[
        {"do":"wait","frames":0},{"do":"wait","frames":0}
    ]}))
    .unwrap();
    let mut runner = Runner::new(script, &output).unwrap();
    let ctx = egui::Context::default();
    for _ in 0..50 {
        let _ = ctx.run(
            egui::RawInput {
                time: Some(0.),
                ..Default::default()
            },
            |ctx| {
                runner.observe(ctx).unwrap();
            },
        );
    }
    assert_eq!(
        runner.step, 1,
        "repaints must not consume the settling interval"
    );
    let _ = ctx.run(
        egui::RawInput {
            time: Some(0.1),
            ..Default::default()
        },
        |ctx| {
            runner.observe(ctx).unwrap();
        },
    );
    assert_eq!(runner.step, 2);
    std::fs::remove_dir_all(output).unwrap();
}

#[test]
fn addon_wizard_identifies_the_kind_being_created() {
    let output = root("addon-heading");
    let script = serde_json::from_value(serde_json::json!({"steps":[
        {"do":"click","text":"New add-on"},{"do":"wait","frames":10}
    ]}))
    .unwrap();
    let mut runner = Runner::new(script, &output).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    let ctx = egui::Context::default();
    for frame in 0..100 {
        let mut raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100., 800.),
            )),
            time: Some(frame as f64 / 60.),
            ..Default::default()
        };
        runner.input(&mut raw);
        let _ = ctx.run(raw, |ctx| {
            studio.draw(ctx);
            runner.observe(ctx).unwrap();
        });
        if runner.finished {
            break;
        }
    }
    assert!(
        runner.finished && runner.error.is_none(),
        "{:?}",
        runner.error
    );
    assert_eq!(
        runner
            .visible()
            .iter()
            .filter(|p| p.text == "New add-on")
            .count(),
        2,
        "button and wizard heading must both identify add-on creation"
    );
    assert_eq!(
        runner
            .visible()
            .iter()
            .filter(|p| p.text == "New mod")
            .count(),
        1,
        "only the separate full-mod button should say New mod"
    );
    std::fs::remove_dir_all(output).unwrap();
}
