//! Capture the actual Studio workbench with invented data and real engine results.
#[path = "../tests/workbench_support/mod.rs"]
mod support;
use mod_studio::{tables::Tables, ui::Studio, workbench::cube::CubeOptions};
fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let output = args.next().expect("output PNG");
    let mode = args.next().unwrap_or_else(|| "items".into());
    let view = match mode.as_str() {
        "drops" => 1,
        "cube" => 2,
        _ => 0,
    };
    let mut snapshot = support::snapshot();
    let table = snapshot
        .data
        .tables
        .iter_mut()
        .find(|t| t.name() == "weapons")
        .unwrap();
    for (key, value) in [("reqstr", "15"), ("gemsockets", "2")] {
        table.header.push(key.as_bytes().to_vec());
        for row in &mut table.rows {
            row.cells.push(value.as_bytes().to_vec());
        }
    }
    let c = table.header.iter().position(|h| h == b"mindam").unwrap();
    table.rows[0].cells[c] = b"6".to_vec();
    let engine = snapshot.compile().expect("synthetic engine");
    let preview = engine
        .preview("weapons", 0, 0, 1)
        .unwrap()
        .iter()
        .map(|l| String::from_utf16_lossy(&l.text))
        .collect::<Vec<_>>()
        .join("\n");
    let result = if view == 1 {
        let tc = engine
            .tcs
            .tcs
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(b"Test pool"))
            .unwrap() as u16;
        let counts = engine.drops(tc, 0, 10_000, 1, 85).unwrap();
        engine.format_drops(&counts, 10_000)
    } else if view == 2 {
        engine
            .cube(
                &mod_studio::workbench_ui::parse_inputs("qa1,2,10").unwrap(),
                &CubeOptions::default(),
            )
            .unwrap()
    } else {
        String::new()
    };
    let root = std::env::temp_dir().join(format!("studio-workbench-{}", std::process::id()));
    let mut manifest = mod_studio::default_manifest();
    manifest.name = "Ember Workshop".into();
    mod_studio::create_project(&root, &manifest, 0).unwrap();
    let mut studio = Studio::default();
    studio.open(&root);
    let data = Tables::from_data(snapshot.data, root.join("files/patches/~studio.d2patch"));
    studio.workbench.prepare_render(&data, preview, result);
    studio.show_workbench(data, view);
    studio.render = Some(output.into());
    let result = eframe::run_native(
        "Mod Studio",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1500., 900.]),
            ..Default::default()
        },
        Box::new(|_| Ok(Box::new(studio))),
    );
    std::fs::remove_dir_all(root).unwrap();
    result
}
