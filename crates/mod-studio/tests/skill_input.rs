use eframe::egui;
use mod_studio::input_script::{Runner, Script};
#[test]
fn skill_hit_control_is_authored_through_the_form() {
    let root = std::env::temp_dir().join(format!("studio-skill-input-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    let script: Script = serde_json::from_value(serde_json::json!({"steps":[
        {"do":"click","text":"Skills"},
        {"do":"fill","text":"New skill name","offset":[0,0],"value":"Interrupting pulse"},
        {"do":"click","text":"Add"},
        {"do":"scroll_to","text":"Add crowd control","point":[1000,650],"delta":[0,-200]},
        {"do":"click","text":"Add crowd control"},
        {"do":"click","text":"Save"},{"do":"wait_text","text":"Saved.","seconds":2}
    ]}))
    .unwrap();
    let captures = root.with_extension("captures");
    let mut runner = Runner::new(script, &captures).unwrap();
    let ctx = egui::Context::default();
    for frame in 0..800 {
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
    let mut kit = mod_studio::kit::Kit::load(&root).unwrap();
    assert_eq!(
        kit.skills[0].parts[0].controls_mut().unwrap()[0].kind,
        "interrupt"
    );
    let saved = std::fs::read_to_string(root.join("files/skills.toml")).unwrap();
    assert!(d2_sim::rules::data_mod::SkillRules::from_toml(&saved).is_ok());
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(captures).unwrap();
}
