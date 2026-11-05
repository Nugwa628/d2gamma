//! Studio project workflows use synthetic, asset-free mods.
use mod_studio::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "mod-studio-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn each_template_creates_a_valid_readable_project() {
    let temp = Temp::new();
    for (i, (_, rules)) in TEMPLATES.iter().enumerate() {
        let p = create_project(
            &temp.0.join(format!("template-{i}")),
            &default_manifest(),
            i,
        )
        .unwrap();
        assert_eq!(p.rules, *rules);
        assert_eq!(p.folder.manifest, default_manifest());
        assert!(p.folder.files.iter().any(|f| f.path == "files/rules.toml"));
        assert!(create_project(&p.folder.root, &default_manifest(), i).is_err());
    }
    // Empty template explicitly uses version 1 and no gameplay overrides.
    let rules: toml::Table = TEMPLATES[0].1.parse().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules["version"].as_integer(), Some(1));
}
#[test]
fn manifest_edits_round_trip_without_losing_optional_fields() {
    let temp = Temp::new();
    let mut m = default_manifest();
    m.allow_derived = true;
    m.approved_addons.push("sample-addon".into());
    let mut p = create_project(&temp.0.join("project"), &m, 0).unwrap();
    p.manifest.insert(
        "name".into(),
        toml::Value::String("New name with \"quotes\"".into()),
    );
    p.save().unwrap();
    let p = Project::open(&p.folder.root).unwrap();
    assert_eq!(p.folder.manifest.name, "New name with \"quotes\"");
    assert!(p.folder.manifest.allow_derived);
    assert_eq!(p.folder.manifest.approved_addons, m.approved_addons);
    let before = fs::read(p.folder.root.join("mod.toml")).unwrap();
    let mut p = p;
    p.manifest
        .insert("id".into(), toml::Value::String("INVALID".into()));
    assert!(p.save().is_err());
    assert_eq!(fs::read(p.folder.root.join("mod.toml")).unwrap(), before);
}
#[test]
fn only_permitted_installed_bases_are_offered() {
    let temp = Temp::new();
    let mut m = default_manifest();
    create_project(&temp.0.join("private"), &m, 0).unwrap();
    m.id = "public-base".into();
    m.allow_derived = true;
    create_project(&temp.0.join("public"), &m, 0).unwrap();
    let bases = installed_bases(&temp.0);
    assert_eq!(bases.len(), 1);
    let mut draft = default_manifest();
    select_base(&mut draft, Some(&bases[0])).unwrap();
    draft.check_builds_on(&m).unwrap();
    m.allow_derived = false;
    assert!(select_base(&mut draft, Some(&m)).is_err());
}
#[test]
fn screen_only_addon_has_no_rules_file() {
    let temp = Temp::new();
    let mut m = default_manifest();
    m.kind = d2_modkit::Kind::Addon;
    select_base(&mut m, None).unwrap();
    let p = create_project(&temp.0.join("addon"), &m, 0).unwrap();
    assert!(!p.folder.root.join("files/rules.toml").exists());
    assert!(create_project(&temp.0.join("bad"), &m, 1).is_err());
    m.kind = d2_modkit::Kind::Full;
    select_base(&mut m, None).unwrap();
    assert!(!m.screen_only);
    assert!(create_project(&temp.0.join("full"), &m, 1)
        .unwrap()
        .folder
        .root
        .join("files/rules.toml")
        .exists());
}
#[test]
fn commands_preserve_spaces_as_single_arguments() {
    let game = PathBuf::from("C:/Game tools/D2Gamma-Game.exe");
    let root = PathBuf::from("C:/My mods/Ember age");
    let native = PathBuf::from("C:/Game data/native");
    let out = PathBuf::from("C:/Packs/ember.d2pack");
    let play = ToolCommand::play(&game, &root);
    assert_eq!(play.command().get_program(), game.as_os_str());
    assert_eq!(
        play.args,
        vec![
            std::ffi::OsString::from("play"),
            std::ffi::OsString::from("--mod-dir"),
            root.as_os_str().into()
        ]
    );
    // Studio appends the configured install after constructing this command.
    // Without `play`, stripping --mod-dir leaves --native in the command slot.
    assert_eq!(
        play.command()
            .arg("--native")
            .arg(&native)
            .get_args()
            .collect::<Vec<_>>(),
        [
            std::ffi::OsStr::new("play"),
            std::ffi::OsStr::new("--mod-dir"),
            root.as_os_str(),
            std::ffi::OsStr::new("--native"),
            native.as_os_str(),
        ]
    );
    let pack = ToolCommand::package(&game, &root, &native, &out);
    assert_eq!(
        pack.command().get_args().collect::<Vec<_>>(),
        [
            std::ffi::OsStr::new("package-mod"),
            root.as_os_str(),
            std::ffi::OsStr::new("--base"),
            native.as_os_str(),
            std::ffi::OsStr::new("--out"),
            out.as_os_str()
        ]
    );
}
#[test]
fn documented_rules_expand_both_unit_classes() {
    let fields = rule_fields();
    assert!(fields
        .iter()
        .any(|(k, _, _)| k == "players.hit_recovery.enabled"));
    assert!(fields
        .iter()
        .any(|(k, _, _)| k == "monsters.hit_recovery.enabled"));
    assert!(fields
        .iter()
        .any(|(k, _, _)| k == "death_experience_percent"));
}

#[test]
fn addon_targets_include_private_full_mods_and_screen_only_projects_stay_restricted() {
    let temp = Temp::new();
    let base = default_manifest();
    create_project(&temp.0.join(&base.id), &base, 0).unwrap();
    assert!(installed_bases(&temp.0).is_empty());
    assert_eq!(installed_addon_targets(&temp.0).len(), 1);
    let mut addon = default_manifest();
    addon.kind = d2_modkit::Kind::Addon;
    addon.id = "test-addon".into();
    addon.screen_only = true;
    select_base(&mut addon, Some(&base)).unwrap();
    assert_eq!(addon.attaches_to.as_deref(), Some(base.id.as_str()));
    assert!(addon.screen_only);
    let mut project = create_project(&temp.0.join("addon"), &addon, 0).unwrap();
    assert!(!project.folder.root.join("files/rules.toml").exists());
    project.rules = "version=1".into();
    assert!(project.save().is_err());
    assert!(addon
        .check_addon_for(Some(&base))
        .unwrap_err()
        .contains("has not approved"));
}
