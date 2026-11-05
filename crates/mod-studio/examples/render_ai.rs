//! Capture the real Studio window on the monster AI editor, with the monster AI
//! template; no install is needed. `render_ai out.png [preset|phase]`.
use mod_studio::ui::Studio;

const TEMPLATE: &str = include_str!("../../../templates/monster-ai-graph/files/ai.toml");

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let output = args.next().expect("output PNG");
    let mode = args.next().unwrap_or_default();
    let root = std::env::temp_dir().join(format!("studio-ai-render-{}", std::process::id()));
    let mut manifest = mod_studio::default_manifest();
    manifest.name = "Stalker Pack".into();
    mod_studio::create_project(&root, &manifest, 0).expect("synthetic project");
    std::fs::write(root.join(mod_studio::ai::AI_FILE), TEMPLATE).expect("template file");
    let mut studio = Studio::default();
    studio.open(&root);
    studio.ai.entry = usize::from(mode == "preset");
    if mode == "phase" {
        if let Some(file) = &mut studio.ai.file {
            file.entries[0].phases.push(mod_studio::ai::Phase {
                name: "enrage".into(),
                when: vec![mod_studio::ai::Item {
                    kind: "life_below".into(),
                    keys: [("percent".to_owned(), toml::Value::Integer(50))].into(),
                }],
                state: Some("fight".into()),
                graph: None,
                immune_seconds: None,
            });
        }
        studio.ai.phase = Some(0);
    }
    studio.ai.state = 0;
    studio.show_ai(None);
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
