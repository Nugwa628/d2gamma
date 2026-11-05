//! Real Studio capture using an invented recipe and no game assets.
fn main() -> eframe::Result {
    let out = std::env::args().nth(1).expect("output PNG");
    let root = std::env::temp_dir().join(format!("studio-effects-render-{}", std::process::id()));
    let mut manifest = mod_studio::default_manifest();
    manifest.name = "Ember Workshop".into();
    mod_studio::create_project(&root, &manifest, 0).unwrap();
    std::fs::create_dir_all(root.join("files/effects")).unwrap();
    std::fs::write(root.join("files/effects/ember.toml"),"version=1\n[[effects]]\nname='ember/windup'\nduration=80\n[effects.light]\nradius=12\nred=255\ngreen=120\nblue=45\n[effects.particles]\ncount=96\nspread=60\nspeed=16\ncolor=220").unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    studio.show_effects();
    studio.render = Some(out.into());
    let result = eframe::run_native(
        "Mod Studio",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1400., 1050.]),
            ..Default::default()
        },
        Box::new(|_| Ok(Box::new(studio))),
    );
    std::fs::remove_dir_all(root).unwrap();
    result
}
