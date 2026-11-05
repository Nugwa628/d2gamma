//! Synthetic publication and load tests; no install assets or private keys.
use d2_modkit::{MemoryPack, PackSource};
use d2_native::{
    layers::LayerStack,
    manifest::{write_files_tsv, FileRow, NativeFile, Status},
    pack, package,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("d2native-package-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}
fn put(root: &Path, path: &str, bytes: &[u8]) {
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, bytes).unwrap();
}
fn rows(kind: &str) -> Vec<FileRow> {
    (0..10)
        .map(|i| FileRow {
            path: format!("data/base{i}.txt"),
            kind: kind.into(),
            archive: "synthetic.mpq".into(),
            source_sha256: "11".repeat(32),
            native: vec![NativeFile {
                name: format!("data/base{i}.txt"),
                size: 8,
                sha256: d2_native::source::sha256_hex(format!("base{i}").as_bytes()),
            }],
            status: Status::Ok,
        })
        .collect()
}
fn folder(root: &Path) -> PathBuf {
    let f = root.join("mod");
    put(&f, "mod.toml", b"id = 'test-mod'\nname = 'Test Mod'\nversion = '1.0.0'\nauthor = 'A'\ndescription = ''\napi = 1\nkind = 'full'\n");
    put(&f, "files/rules.toml", b"# new rules");
    f
}
fn repack(root: &Path, src: &MemoryPack, out: &Path) {
    for (path, bytes) in &src.files {
        put(root, path, bytes);
    }
    pack::build(root, out, 1).unwrap();
}

#[test]
fn round_trip_deterministic_and_loader_refuses_tampering() {
    let root = root("round-trip");
    let folder = folder(&root);
    put(
        &root,
        "native/files.tsv",
        write_files_tsv(&rows("dc6")).as_bytes(),
    );
    let native = root.join("native");
    let key = root.join("user/author.pk8");
    let out = root.join("test-mod.d2pack");
    let checks = |folder: &Path, _: &Path| {
        assert!(folder.join("files/rules.toml").is_file());
        assert!(!key.exists()); // checks precede signing/key creation
        Ok(())
    };
    package::package_with_checks(&folder, &native, &out, &key, checks).unwrap();
    let first = fs::read(&out).unwrap();
    let out2 = root.join("second.d2pack");
    package::package_with_checks(&folder, &native, &out2, &key, |_, _| Ok(())).unwrap();
    assert_eq!(first, fs::read(&out2).unwrap());
    let p = pack::Pack::open(&out).unwrap();
    d2_modkit::publish::verify(&p).unwrap();
    let resolved = d2_modkit::resolve::resolve_with(
        &d2_modkit::ModSpec::Dir(out.clone()),
        &root,
        d2_modkit::api::Date::today(),
        package::open_mod,
    )
    .unwrap();
    assert_eq!(resolved.picked().manifest.id, "test-mod");
    assert_eq!(
        resolved.layer_entries(),
        [fs::canonicalize(&out).unwrap().display().to_string()]
    );
    fs::create_dir_all(root.join("test-mod/saves")).unwrap();
    let installed = d2_modkit::resolve::resolve_with(
        &d2_modkit::ModSpec::Id("test-mod".into()),
        &root,
        d2_modkit::api::Date::today(),
        package::open_mod,
    )
    .unwrap();
    assert_eq!(installed.picked().manifest.id, "test-mod");
    assert_eq!(d2_modkit::read_manifest(&p).unwrap().api, 1);
    LayerStack::with_mods(&native, &[out.display().to_string()]).unwrap();
    let mut memory = MemoryPack {
        files: PackSource::paths(&p)
            .into_iter()
            .map(|path| {
                let bytes = PackSource::read(&p, &path).unwrap();
                (path, bytes)
            })
            .collect(),
    };
    memory.files.get_mut("files/rules.toml").unwrap().push(b'!');
    let tampered = root.join("tampered.d2pack");
    repack(&root.join("tampered"), &memory, &tampered);
    let err = LayerStack::with_mods(&native, &[tampered.display().to_string()])
        .unwrap_err()
        .to_string();
    assert!(err.contains("changed or damaged"), "{err}");
    assert!(package::open_mod(&tampered, d2_modkit::api::Date::today())
        .unwrap_err()
        .to_string()
        .contains("changed or damaged"));
    memory.files.get_mut("files/rules.toml").unwrap().pop();
    let record = memory.files.get_mut(d2_modkit::publish::RECORD).unwrap();
    let start = String::from_utf8(record.clone())
        .unwrap()
        .find("ed25519:")
        .unwrap()
        + 8;
    record[start] = if record[start] == b'0' { b'1' } else { b'0' };
    repack(&root.join("wrong"), &memory, &tampered);
    let err = LayerStack::with_mods(&native, &[tampered.display().to_string()])
        .unwrap_err()
        .to_string();
    assert!(err.contains("signature is not valid"), "{err}");
    memory.files.remove(d2_modkit::publish::RECORD);
    repack(&root.join("unsigned"), &memory, &tampered);
    assert!(
        LayerStack::with_mods(&native, &[tampered.display().to_string()])
            .unwrap_err()
            .to_string()
            .contains("no author signature")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn patch_targets_count_and_asset_halves_count_once() {
    let mut rows = rows("excel");
    let mut src = MemoryPack::default();
    src.files.insert(
        "files/patches/test.d2patch".into(),
        b"d2patch 1\ntable base0\n".to_vec(),
    );
    package::check_content(&src, &rows).unwrap();
    src.files
        .get_mut("files/patches/test.d2patch")
        .unwrap()
        .extend_from_slice(b"table base1\n");
    assert!(package::check_content(&src, &rows)
        .unwrap_err()
        .contains("base tables"));
    src.files.clear();
    for row in &mut rows {
        row.kind = "dc6".into();
        let mut half = row.native[0].clone();
        half.name.push_str(".png");
        row.native.push(half);
    }
    src.files
        .insert("files/data/base0.txt".into(), b"modified sidecar".to_vec());
    src.files.insert(
        "files/data/base0.txt.png".into(),
        b"modified pixels".to_vec(),
    );
    package::check_content(&src, &rows).unwrap();
}

#[test]
fn content_limits_each_kind_boundary_new_files_and_renamed_copies() {
    for kind in ["excel", "dc6", "wav", "ds1"] {
        let rows = rows(kind);
        let mut src = MemoryPack::default();
        src.files
            .insert("mod.toml".into(), b"# irrelevant to overlap".to_vec());
        src.files
            .insert("files/new.txt".into(), b"new bytes".to_vec());
        package::check_content(&src, &rows).unwrap();
        src.files
            .insert("files/data/base0.txt".into(), b"modified".to_vec());
        package::check_content(&src, &rows).unwrap(); // exactly 10%
        src.files
            .insert("files/data/base1.txt".into(), b"modified".to_vec());
        assert!(package::check_content(&src, &rows)
            .unwrap_err()
            .contains("limit is 10%"));
        src.files.remove("files/data/base1.txt");
        src.files
            .insert("files/renamed.txt".into(), b"base7".to_vec());
        assert!(package::check_content(&src, &rows)
            .unwrap_err()
            .contains("unmodified base file"));
    }
}

#[test]
fn failed_checks_do_not_write_pack_or_key() {
    let root = root("failed");
    let folder = folder(&root);
    put(
        &root,
        "native/files.tsv",
        write_files_tsv(&rows("dc6")).as_bytes(),
    );
    let out = root.join("mod.d2pack");
    let key = root.join("user/key.pk8");
    let err = package::package_with_checks(&folder, &root.join("native"), &out, &key, |_, _| {
        Err("broken reference".into())
    })
    .unwrap_err();
    assert_eq!(err, "broken reference");
    assert!(!out.exists() && !key.exists());
    assert!(!folder.join(d2_modkit::publish::RECORD).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn relative_folder_layers_resolve_from_the_requested_directory() {
    let relative = PathBuf::from(format!("package-relative-{}", std::process::id()));
    let target = folder(&relative);
    let loaded = package::open_mod(&target, d2_modkit::api::Date::today()).unwrap();
    assert!(loaded.dir.is_absolute());
    assert_eq!(loaded.dir, fs::canonicalize(&target).unwrap());
    fs::remove_dir_all(relative).unwrap();
}
