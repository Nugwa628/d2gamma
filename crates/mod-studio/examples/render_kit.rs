//! Capture the real Studio window on the state, skill or trigger editor, with the
//! proc-charges template and invented table rows; no install is needed.
#[path = "../tests/kit_support/mod.rs"]
mod support;
use mod_studio::{
    kit::{Fm, PartEntry},
    kit_ui::View,
    ui::Studio,
};
fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let output = args.next().expect("output PNG");
    let mode = args.next().unwrap_or_else(|| "states".into());
    let root = std::env::temp_dir().join(format!("studio-kit-render-{}", std::process::id()));
    let mut manifest = mod_studio::default_manifest();
    manifest.name = "Ember Workshop".into();
    mod_studio::create_project(&root, &manifest, 0).expect("synthetic project");
    for (file, text) in [
        (mod_studio::kit::STATES_FILE, support::STATES),
        (mod_studio::kit::SKILLS_FILE, support::SKILLS),
        (mod_studio::kit::TRIGGERS_FILE, support::TRIGGERS),
    ] {
        std::fs::write(root.join(file), text).expect("template file");
    }
    let mut studio = Studio::default();
    studio.open(&root);
    let view = match mode.as_str() {
        "skills" => View::Skills,
        "triggers" => View::Triggers,
        _ => View::States,
    };
    if let Some(kit) = &mut studio.kit.kit {
        studio.kit.state = 1;
        studio.kit.trigger = 1;
        if let Some(PartEntry::AreaHit { damage, .. }) = kit.skills[0].parts.first_mut() {
            damage.max = Fm::new("min(20 + lvl * 6, par1 * 40)");
        }
    }
    studio.show_kit(view, Some(support::tables(&root)));
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
