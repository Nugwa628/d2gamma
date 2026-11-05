//! Real egui framebuffer captures with invented check findings and table rows.
#[path = "../tests/support/mod.rs"]
mod support;
use mod_studio::{tables::Tables, ui::Studio};
fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let output = args.next().expect("output PNG");
    let mode = args.next().unwrap_or_else(|| "errors".into());
    let root = std::env::temp_dir().join(format!("studio-problems-render-{}", std::process::id()));
    let mut manifest = mod_studio::default_manifest();
    manifest.name = "Ember Workshop".into();
    mod_studio::create_project(&root, &manifest, 0).expect("synthetic project");
    let mut studio = Studio::default();
    studio.open(&root);
    studio.settings.game = "D2Gamma-Game".into();
    studio.settings.native = "/converted-install".into();
    studio.show_tables(
        Tables::from_data(support::data(), root.join("files/patches/~studio.d2patch")),
        false,
    );
    let mut findings = vec![
        serde_json::json!({"severity":"warning","category":"items sweep findings (Unverified)","location":"weapons row 1","message":"Unverified candidate: review the generated item scenario before publishing."}),
        serde_json::json!({"severity":"info","category":"content summaries","location":"mod layers","message":"3 changed rows checked; native assets decoded; base install is unchanged."}),
    ];
    if mode != "warnings" {
        findings.insert(0,serde_json::json!({"severity":"error","category":"broken tables or references","location":"weapons row 0 column type","message":"Item type 'unknown' does not resolve. Choose a valid item type in the highlighted cell."}));
        findings.insert(1,serde_json::json!({"severity":"error","category":"invalid global rules files","location":root.join("files/rules.toml"),"message":"Example error: hit recovery threshold must be positive."}));
    }
    let json = serde_json::json!({"format_version":1,"target":root,"exit_code":if mode=="warnings" {0} else {1},"completed":["manifest, API and dependency checks","table references","asset decoding"],"findings":findings});
    studio.show_report(&json.to_string()).expect("report");
    if mode == "jump" {
        studio.tables.jump("weapons", 0, Some("type")).unwrap();
    }
    studio.render = Some(output.into());
    let result = eframe::run_native(
        "Mod Studio",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1500.0, 1000.0]),
            ..Default::default()
        },
        Box::new(|_| Ok(Box::new(studio))),
    );
    std::fs::remove_dir_all(root).expect("clean synthetic project");
    result
}
