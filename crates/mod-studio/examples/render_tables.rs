//! Render the real Studio window with invented table rows; no install is needed.
#[path = "../tests/support/mod.rs"]
mod support;
use mod_studio::{tables::Tables, ui::Studio};
fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let output = args.next().expect("output PNG");
    let mode = args.next();
    let dropdown = mode.as_deref() == Some("dropdown");
    let root = std::env::temp_dir().join(format!("studio-render-{}", std::process::id()));
    let mut manifest = mod_studio::default_manifest();
    manifest.name = "Ember Workshop".into();
    mod_studio::create_project(&root, &manifest, 0).expect("synthetic project");
    let mut studio = Studio::default();
    studio.open(&root);
    let mut tables = Tables::from_data(support::data(), root.join("files/patches/~studio.d2patch"));
    tables.edited.tables[0].rows[0].cells[4] = b"6".to_vec();
    tables.edited.tables[0].rows[1].cells[5] = b"22".to_vec();
    tables.edited.tables[0].rows[2].cells[3] = b"18".to_vec();
    tables.dirty = true;
    tables.validate();
    if mode.as_deref() != Some("manifest") {
        studio.show_tables(tables, dropdown);
    }
    studio.render = Some(output.into());
    let result = eframe::run_native(
        "Mod Studio",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1500.0, 900.0]),
            ..Default::default()
        },
        Box::new(|_| Ok(Box::new(studio))),
    );
    std::fs::remove_dir_all(root).expect("clean synthetic project");
    result
}
