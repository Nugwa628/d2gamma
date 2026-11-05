//! Synthetic editor wiring, save guards and the generated-Luau fake runner.
use super::*;
fn root(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("studio-graphs-{tag}-{}", std::process::id()));
    fs::create_dir_all(&p).unwrap();
    p
}
#[test]
fn graphs_preserve_invalid_drafts_and_external_changes_and_absence() {
    let root = root("save");
    let mut files = Graphs::open(&root).unwrap();
    assert!(!files.save().unwrap());
    assert!(!root.join("files/scripts").exists());
    let i = files.add("nested/echo.graph.toml").unwrap();
    assert!(files.dirty());
    files.save().unwrap();
    assert!(!files.dirty());
    files.documents[i].text = "version=99".into();
    files.save().unwrap();
    assert_eq!(
        fs::read_to_string(root.join("files/scripts/nested/echo.graph.toml")).unwrap(),
        "version=99"
    );
    files.documents[i].text = "my draft".into();
    fs::write(
        root.join("files/scripts/nested/echo.graph.toml"),
        "external",
    )
    .unwrap();
    assert!(files
        .save()
        .unwrap_err()
        .to_string()
        .contains("outside Studio"));
    assert!(files.add("../escape.graph.toml").is_err());
    assert!(files.add("bad.luau").is_err());
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn canvas_connections_check_types_and_headless_paint_works() {
    let root = root("wire");
    let mut files = Graphs::open(&root).unwrap();
    let i = files.add("main.graph.toml").unwrap();
    let mut g = files.documents[i].graph().unwrap();
    let action = add_node(&mut g, "api", "message").unwrap();
    connect(&mut g, "entry", "unit", &action, "unit").unwrap();
    assert!(connect(&mut g, "entry", "unit", &action, "text").is_err());
    g.nodes[0].next = action;
    files.documents[i].replace(&g).unwrap();
    let mut editor = crate::graphs_ui::GraphEditor::default();
    editor.graphs = Some(files);
    let ctx = eframe::egui::Context::default();
    let _ = ctx.run(Default::default(), |ctx| {
        eframe::egui::CentralPanel::default().show(ctx, |ui| editor.draw(ui));
    });
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn fake_event_executes_generated_code_with_seeded_rng_and_trace() {
    let mut g = Graph {
        version: 1,
        nodes: vec![Node {
            id: "entry".into(),
            kind: "event".into(),
            event: "kill".into(),
            ..Default::default()
        }],
    };
    let roll = add_node(&mut g, "api", "random").unwrap();
    let write = add_node(&mut g, "api", "set_stat").unwrap();
    g.nodes[1]
        .inputs
        .insert("upper".into(), Input::Integer(1000));
    g.nodes[0].next = roll.clone();
    g.nodes[1].next = write.clone();
    connect(&mut g, &roll, "value", &write, "value").unwrap();
    let first = crate::graph_testfire::fire(&g, "kill", 0, 1).unwrap();
    let second = crate::graph_testfire::fire(&g, "kill", 0, 1).unwrap();
    assert_eq!(first.nodes, vec!["entry".to_string(), roll, write]);
    assert_eq!(first.stats, second.stats);
    assert_eq!(first.calls, second.calls);
    assert_eq!(first.calls.len(), 2);
    assert!(crate::graph_testfire::fire(&g, "unknown", 0, 1).is_err());
}

#[test]
fn fake_event_keeps_partial_trace_on_range_and_budget_failures() {
    let mut g = Graph {
        version: 1,
        nodes: vec![Node {
            id: "entry".into(),
            kind: "event".into(),
            event: "kill".into(),
            ..Default::default()
        }],
    };
    let action = add_node(&mut g, "api", "random").unwrap();
    g.nodes[0].next = action.clone();
    g.nodes[1].inputs.insert("upper".into(), Input::Integer(0));
    let trace = crate::graph_testfire::fire(&g, "kill", 0, 1).unwrap();
    assert_eq!(trace.nodes, vec!["entry".to_string(), action]);
    assert!(trace.error.unwrap().contains("M2 range"));
    let loop_id = add_node(&mut g, "loop", "").unwrap();
    g.nodes[0].next = loop_id;
    g.nodes[2].body = g.nodes[1].id.clone();
    g.nodes[2].count = 64;
    let nested = add_node(&mut g, "loop", "").unwrap();
    g.nodes[2].body = nested;
    g.nodes[3].count = 64;
    g.nodes[3].body = g.nodes[1].id.clone();
    g.nodes[1]
        .inputs
        .insert("upper".into(), Input::Integer(100));
    let trace = crate::graph_testfire::fire(&g, "kill", 0, 1).unwrap();
    assert_eq!(trace.calls.len(), 128);
    assert!(trace.error.unwrap().contains("budget exceeded"));
    assert!(trace.nodes.len() < 4096);
}

#[test]
fn fake_event_player_capabilities_reject_the_monster() {
    let mut g = Graph {
        version: 1,
        nodes: vec![Node {
            id: "entry".into(),
            kind: "event".into(),
            event: "kill".into(),
            ..Default::default()
        }],
    };
    let action = add_node(&mut g, "api", "message").unwrap();
    g.nodes[0].next = action;
    g.nodes[1].inputs.insert(
        "unit".into(),
        Input::Link {
            node: "entry".into(),
            pin: "other".into(),
        },
    );
    let trace = crate::graph_testfire::fire(&g, "kill", 0, 1).unwrap();
    assert!(trace.error.unwrap().contains("fake player"));
}

#[test]
fn rename_preserves_all_wires_and_rejects_invalid_or_duplicate_ids() {
    let mut g = Graph {
        version: 1,
        nodes: vec![Node {
            id: "entry".into(),
            kind: "event".into(),
            event: "kill".into(),
            ..Default::default()
        }],
    };
    let message = add_node(&mut g, "api", "message").unwrap();
    g.nodes[0].next = message.clone();
    let source = g.compile().unwrap();
    rename_node(&mut g, "entry", "slain").unwrap();
    assert!(matches!(&g.nodes[1].inputs["unit"],Input::Link{node,..} if node=="slain"));
    rename_node(&mut g, &message, "notice").unwrap();
    assert_eq!(g.nodes[0].next, "notice");
    assert!(g.compile().is_ok());
    let unchanged = g.clone();
    assert!(rename_node(&mut g, "slain", "notice").is_err());
    assert!(rename_node(&mut g, "slain", "1bad").is_err());
    assert_eq!(g, unchanged);
    assert!(!source.is_empty());
}
