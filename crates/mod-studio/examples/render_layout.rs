//! Capture the real layout editor with invented UI documents and no game assets.
fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("output PNG");
    let four_k = args.next().is_some_and(|s| s == "4k");
    let root = std::env::temp_dir().join(format!("studio-layout-render-{}", std::process::id()));
    let mut manifest = mod_studio::default_manifest();
    manifest.name = "Ember Workshop".into();
    mod_studio::create_project(&root, &manifest, 0).expect("project");
    std::fs::create_dir_all(root.join("files/ui")).unwrap();
    std::fs::write(root.join("files/ui/ember.toml"),"version=1\nname='ember_hud'\n[[actions]]\nname='allocate'\nstat='strength'\ndelta=1\n[[elements]]\nid='resource_frame'\nkind='frame'\nx=400\ny=30\nwidth=480\nheight=110\n[[elements]]\nid='resource_label'\nkind='text'\nx=425\ny=50\nwidth=300\nheight=30\ntext='Ember resource'\n[[elements]]\nid='resource_bar'\nkind='bar'\nx=425\ny=90\nwidth=420\nheight=20\n[elements.bind]\nsource='var.combat/ember'\nmaximum=100\nformat='percent'\n[[elements]]\nid='allocate_button'\nkind='button'\nx=425\ny=165\nwidth=220\nheight=35\ntext='Allocate strength'\naction='allocate'").unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    studio.show_layout(None);
    studio.layout.four_k = four_k;
    studio.layout.element = Some(2);
    studio.render = Some(out.into());
    let result = eframe::run_native(
        "Mod Studio",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1600., 1050.]),
            ..Default::default()
        },
        Box::new(|_| Ok(Box::new(studio))),
    );
    std::fs::remove_dir_all(root).unwrap();
    result
}
