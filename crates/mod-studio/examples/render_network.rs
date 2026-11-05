//! Capture the network editor with invented declarations and no game assets.
fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("output PNG");
    let actions = args.next().is_some_and(|a| a == "actions");
    let root = std::env::temp_dir().join(format!("studio-network-render-{}", std::process::id()));
    let mut manifest = mod_studio::default_manifest();
    manifest.name = "Ember Workshop".into();
    mod_studio::create_project(&root, &manifest, 0).unwrap();
    std::fs::create_dir_all(root.join("files/scripts")).unwrap();
    std::fs::write(
        root.join("files/net.toml"),
        "version=1\n[[values]]\nid=1\nname='rage'\nsource='var.combat/rage'",
    )
    .unwrap();
    std::fs::write(
        root.join("files/scripts/combat.luau"),
        "return { can_allocate = function(value) return value < 200 and 1 or 0 end }",
    )
    .unwrap();
    let mut studio = mod_studio::ui::Studio::default();
    studio.open(&root);
    studio.show_network();
    if actions {
        studio.net.add(true).unwrap();
        let a = studio
            .net
            .table
            .get_mut("actions")
            .unwrap()
            .as_array_mut()
            .unwrap()[0]
            .as_table_mut()
            .unwrap();
        for (key,value) in "name='allocate'\nstat='strength'\ncost_stat='statpts'\ncost=1\n[validator]\nmodule='combat.luau'\nfunction='can_allocate'".parse::<toml::Table>().unwrap(){a.insert(key,value);}
    }
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
