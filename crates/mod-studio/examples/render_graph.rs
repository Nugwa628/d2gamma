//! Capture the real visual editor with an invented graph and fake-event trace.
fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let output = args.next().expect("output PNG");
    let source = args.next().is_some_and(|a| a == "source");
    let root = std::env::temp_dir().join(format!("studio-visual-graph-{}", std::process::id()));
    let mut manifest = mod_studio::default_manifest();
    manifest.name = "Echo Workshop".into();
    mod_studio::create_project(&root, &manifest, 0).unwrap();
    std::fs::create_dir_all(root.join("files/scripts")).unwrap();
    let text = r#"version=1
[[nodes]]
id='kill'
kind='event'
event='kill'
next='roll'
x=20
y=20
[[nodes]]
id='roll'
kind='api'
name='random'
next='branch'
x=290
y=20
[nodes.inputs]
unit={node='kill',pin='unit'}
upper=100
[[nodes]]
id='chance'
kind='condition'
op='less'
x=290
y=320
[nodes.inputs]
a={node='roll',pin='value'}
b=10
[[nodes]]
id='branch'
kind='branch'
yes='message'
x=560
y=20
[nodes.inputs]
condition={node='chance',pin='value'}
[[nodes]]
id='message'
kind='api'
name='message'
x=830
y=20
[nodes.inputs]
unit={node='kill',pin='unit'}
text='A friendly echo answers your victory.'
"#;
    std::fs::write(root.join("files/scripts/echo.graph.toml"), text).unwrap();
    let graph = d2_modkit::graph::Graph::parse(text).unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    studio.show_graphs();
    studio.graphs.generated = source;
    studio.graphs.selected_node = Some("message".into());
    for seed in 1..100 {
        let trace = mod_studio::graph_testfire::fire(&graph, "kill", 0, seed).unwrap();
        if trace.calls.len() == 2 {
            studio.graphs.trace = Some(trace);
            studio.graphs.seed = seed;
            break;
        }
    }
    studio.graphs.event = d2_modkit::graph::EVENTS
        .iter()
        .position(|e| *e == "kill")
        .unwrap();
    studio.render = Some(output.into());
    let result = eframe::run_native(
        "Mod Studio",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1840., 1120.]),
            ..Default::default()
        },
        Box::new(|_| Ok(Box::new(studio))),
    );
    std::fs::remove_dir_all(root).unwrap();
    result
}
