//! Graph discovery shares the real folder/pack script loader and M2 VM.
use super::*;
use d2_native::layers::{Layer, LayerFiles};
#[test]
fn graph_event_palette_tracks_the_m2_event_contract() {
    assert_eq!(
        d2_modkit::graph::EVENTS,
        EventKind::ALL.map(EventKind::name)
    );
}
#[test]
fn graph_sources_load_effective_folder_and_pack_and_name_node_errors() {
    let root = std::env::temp_dir().join(format!("graph-layer-{}", std::process::id()));
    std::fs::create_dir_all(root.join("files/scripts/nested")).unwrap();
    std::fs::write(root.join("mod.toml"), "synthetic").unwrap();
    let path = root.join("files/scripts/nested/echo.graph.toml");
    std::fs::write(
        &path,
        "version=1\n[[nodes]]\nid='entry'\nkind='event'\nevent='kill'",
    )
    .unwrap();
    let pack = root.with_extension("d2pack");
    d2_native::pack::build(&root, &pack, 1).unwrap();
    let base = Layer::dir("base", root.join("files"), None);
    assert!(Sources::from_layers(&LayerStack {
        layers: vec![base.clone()]
    })
    .unwrap()
    .instantiate()
    .unwrap()
    .is_none());
    for layer in [
        Layer::dir("mod", root.join("files"), None),
        Layer {
            name: "packed".into(),
            files: LayerFiles::Pack(Arc::new(d2_native::pack::Pack::open(&pack).unwrap())),
            info: None,
        },
    ] {
        let runtime = Sources::from_layers(&LayerStack {
            layers: vec![base.clone(), layer],
        })
        .unwrap()
        .instantiate()
        .unwrap()
        .unwrap();
        runtime.export("nested/echo.graph.toml", "on_kill").unwrap();
    }
    std::fs::write(
        &path,
        "version=1\n[[nodes]]\nid='broken'\nkind='event'\nevent='not_an_event'",
    )
    .unwrap();
    let error = Sources::from_layers(&LayerStack {
        layers: vec![base, Layer::dir("mod", root.join("files"), None)],
    })
    .unwrap_err();
    assert!(
        error.contains("nested/echo.graph.toml: node broken"),
        "{error}"
    );
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_file(pack).unwrap();
}
