use eframe::egui;
use mod_studio::input_script::{Runner, Script};
#[test]
fn node_properties_and_typed_wires_can_be_authored_with_egui_input() {
    let root = std::env::temp_dir().join(format!("studio-graph-input-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    mod_studio::create_project(&root, &mod_studio::default_manifest(), 0).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    let script:Script=serde_json::from_value(serde_json::json!({"steps":[
 {"do":"click","text":"Visual scripts"},
 {"do":"fill","text":"Graph file name","offset":[150,0],"value":"echo.graph.toml"},
 {"do":"click","text":"New graph"},
 {"do":"click","text":"Choose node"}, {"do":"click","text":"entry"},
 {"do":"fill","text":"Node ID","offset":[70,0],"value":"slain"},{"do":"click","text":"Rename node"},
 {"do":"fill","text":"Position X","offset":[60,0],"value":"20"},
 {"do":"fill","text":"Position Y","offset":[60,0],"value":"30"},
        {"do":"fill","text":"Search nodes","offset":[0,0],"value":"message"},
 {"do":"click","text":"api: message"},
 {"do":"fill","text":"Node ID","offset":[70,0],"value":"notice"},{"do":"click","text":"Rename node"},
 {"do":"click","text":"Input unit","offset":[20,20]},{"do":"fill","text":"Find source","offset":[0,22],"value":"sla"},{"do":"click","text":"slain.unit"},
 {"do":"click","text":"Control next","offset":[20,20]},{"do":"wait_text","text":"Find target","seconds":1},{"do":"click","text":"End"},
 {"do":"fill","text":"Value text","offset":[75,0],"value":"The lantern remembers."},
 {"do":"fill","text":"Find node","offset":[100,0],"value":"slain"},{"do":"click","text":"notice"},{"do":"click","text":"slain"},
 {"do":"click","text":"End"},{"do":"fill","text":"Find target","offset":[0,22],"value":"not"},{"do":"click","text":"notice"},
 {"do":"click","text":"Save"},{"do":"wait_text","text":"Saved.","seconds":2}
 ]})).unwrap();
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
    let graph = d2_modkit::graph::Graph::parse(
        &std::fs::read_to_string(root.join("files/scripts/echo.graph.toml")).unwrap(),
    )
    .unwrap();
    assert_eq!((graph.nodes[0].x, graph.nodes[0].y), (20, 30));
    assert_eq!(graph.nodes[0].next, "notice");
    assert_eq!(graph.nodes[1].id, "notice");
    assert!(graph.compile().is_ok());
    let trace = mod_studio::graph_testfire::fire(&graph, "kill", 0, 1).unwrap();
    assert_eq!(trace.nodes, vec!["slain", "notice"]);
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(captures).unwrap();
}
