//! Data packs against the folders they were built from, on a synthetic
//! layer (no game files): every file reads back byte for byte, through the
//! pack and through the layer stack; a pack layer and a folder layer give
//! the same stack; a cut-off or damaged pack is refused, naming the pack.

use std::path::{Path, PathBuf};

use d2_native::kind::{grey_palette, NativeKind};
use d2_native::layers::{is_pack_entry, mod_name, valid_mod_entry, LayerFiles, LayerStack};
use d2_native::manifest::{write_files_tsv, Manifest};
use d2_native::pack::{self, Pack};
use d2_native::source::{decode_original, sha256_hex, NativeAsset, NativeSource, KNOWN_KINDS};
use test_fixtures::sprites::{dc6_file, dc6_frames, Dc6Shape};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("d2native-pack-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

const SHEET: &str = "data/global/ui/panel/packtest.dc6";
const MONSTER: &str = "data/global/monsters/zz/cof/zzneu1hth.dc6";

fn sheet(seed: u32) -> d2_formats::dc6::Dc6 {
    let shape = Dc6Shape {
        directions: 1,
        frames: 2,
        width: 5,
        height: 4,
        panel: false,
    };
    match decode_original(SHEET, &dc6_file(&dc6_frames(shape, seed), 1, 2))
        .unwrap()
        .unwrap()
    {
        NativeAsset::Dc6(d) => d,
        _ => unreachable!(),
    }
}

/// Writes sprite sheet `p` (seed `seed`) as native files under `dir`.
fn write_sheet(dir: &Path, p: &str, seed: u32) {
    for f in sheet(seed).write(p, &grey_palette()).unwrap() {
        put(dir, &f.path, &f.bytes);
    }
}

/// A mod layer folder `<root>/mods/<name>/` with a sheet, a patch, a side
/// table, a large compressible text file, an incompressible file, an empty
/// file and deep folders: every way a file can be stored.
fn layer(root: &Path, name: &str, requires: &str, seed: u32) -> PathBuf {
    let dir = root.join("mods").join(name);
    put(
        &dir,
        "mod.toml",
        format!(
            "name = \"{name}\"\nversion = \"2.0\"\nnative_format_version = 1\nrequires = [{requires}]\n"
        )
        .as_bytes(),
    );
    let files = dir.join("files");
    write_sheet(&files, SHEET, seed);
    write_sheet(&files, MONSTER, seed + 1);
    put(&files, "patches/10-items.d2patch", b"# patch\nset x = 1\n");
    put(&files, "tables/side.txt", b"a\tb\n1\t2\n");
    let text: String = (0..20_000).map(|i| format!("line {i}\tvalue\n")).collect();
    put(&files, "data/global/text/big.txt", text.as_bytes());
    let mut noise = Vec::with_capacity(70_000);
    let mut x = seed.wrapping_mul(2_654_435_761).max(1);
    for _ in 0..70_000 {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        noise.push(x as u8);
    }
    put(&files, "data/global/sfx/noise.wav.bin", &noise);
    put(&files, "data/empty.bin", b"");
    put(&files, "data/a/b/c/d/deep.txt", name.as_bytes());
    put(&dir, "extra-notes.tsv", b"path\tsource\n");
    dir
}

/// A native root with a base layer (the sheet, seed 7) and a manifest.
fn root(name: &str) -> PathBuf {
    let root = scratch(name).join("native");
    write_sheet(&root.join("base"), SHEET, 7);
    let tsv = write_files_tsv(&[]);
    put(&root, "files.tsv", tsv.as_bytes());
    let m = Manifest {
        converter_version: "0.1.0".into(),
        converter_commit: "test".into(),
        kinds: KNOWN_KINDS.iter().map(|&(k, v)| (k.into(), v)).collect(),
        language: "eng".into(),
        lod: true,
        archives: Vec::new(),
        counts: [("excel".to_string(), Default::default())].into(),
        unnamed_blocks: Default::default(),
        complete: true,
        files_sha256: sha256_hex(tsv.as_bytes()),
    };
    put(&root, "manifest.toml", m.to_toml().as_bytes());
    root
}

#[test]
fn every_file_of_the_folder_reads_back_identical_from_the_pack() {
    let root = root("bytes");
    let dir = layer(&root, "m", "", 11);
    let out = root.parent().unwrap().join("m.d2pack");
    let stats = pack::build(&dir, &out, 3).unwrap();
    let files = pack::walk(&dir).unwrap();
    assert_eq!(stats.files, files.len());
    assert!(
        stats.compressed >= 1 && stats.compressed < files.len(),
        "{stats:?}"
    );
    assert_eq!(stats.pack_bytes, std::fs::metadata(&out).unwrap().len());

    let p = Pack::open(&out).unwrap();
    assert_eq!(p.paths().collect::<Vec<_>>(), files);
    for f in &files {
        let want = std::fs::read(dir.join(f)).unwrap();
        assert_eq!(p.read(f).unwrap().unwrap(), want, "{f}");
        assert_eq!(p.file_len(f), Some(want.len() as u64), "{f}");
    }
    assert!(pack::verify(&out, Some(&dir)).unwrap().is_empty());

    // The same folder packs to the same bytes: a pack is reproducible.
    let again = root.parent().unwrap().join("again.d2pack");
    pack::build(&dir, &again, 1).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), std::fs::read(&again).unwrap());
}

#[test]
fn a_pack_layer_and_a_folder_layer_give_the_same_stack() {
    let folder_root = root("stack-folder");
    let pack_root = root("stack-pack");
    let a = layer(&folder_root, "a", "", 21);
    let b = layer(&folder_root, "b", "\"a\"", 31);
    // The pack root holds `a` as `mods/a.d2pack` and `b` as a pack outside
    // the root, named by its path.
    pack::build(&a, &pack_root.join("mods/a.d2pack"), 2).unwrap();
    let outside = pack_root.parent().unwrap().join("ship/b.d2pack");
    std::fs::create_dir_all(outside.parent().unwrap()).unwrap();
    pack::build(&b, &outside, 2).unwrap();

    let by_folder = LayerStack::with_mods(&folder_root, &["a".into(), "b".into()]).unwrap();
    let by_pack = LayerStack::with_mods(
        &pack_root,
        &["a".into(), outside.to_string_lossy().into_owned()],
    )
    .unwrap();
    let names = |s: &LayerStack| s.layers.iter().map(|l| l.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&by_folder), ["base", "a", "b"]);
    assert_eq!(names(&by_pack), names(&by_folder));
    assert!(matches!(by_pack.layers[1].files, LayerFiles::Pack(_)));
    assert!(matches!(by_pack.layers[2].files, LayerFiles::Pack(_)));
    for (x, y) in by_folder.layers.iter().zip(&by_pack.layers) {
        assert_eq!(x.info, y.info, "{}", x.name);
        assert_eq!(x.files_under("").unwrap(), y.files_under("").unwrap());
        for f in x.files_under("").unwrap() {
            assert_eq!(
                x.read(&f).unwrap().unwrap(),
                y.read(&f).unwrap().unwrap(),
                "{}: {f}",
                x.name
            );
        }
    }
    for f in by_folder.layers[2].files_under("").unwrap() {
        assert_eq!(
            by_folder.find(&f).map(|l| &l.name),
            by_pack.find(&f).map(|l| &l.name),
            "{f}"
        );
        assert_eq!(
            by_folder.read(&f).unwrap().unwrap(),
            by_pack.read(&f).unwrap().unwrap()
        );
    }
    assert!(by_pack.find("data/not-there.txt").is_none());
    assert_eq!(by_folder.patches().unwrap(), by_pack.patches().unwrap());
    assert_eq!(
        by_folder.side_tables().unwrap(),
        by_pack.side_tables().unwrap()
    );
    assert_eq!(by_pack.patches().unwrap().len(), 2);
    assert_eq!(
        by_folder.layers[1]
            .files_under("data/global/monsters")
            .unwrap(),
        by_pack.layers[1]
            .files_under("data/global/monsters")
            .unwrap()
    );

    // The native source decodes the same asset from either.
    let read = |root: &Path, order: &[String]| {
        NativeSource::open_with(root, Some(order))
            .unwrap()
            .read_native(SHEET)
            .unwrap()
            .unwrap()
    };
    let packed = read(
        &pack_root,
        &["a".into(), outside.to_string_lossy().into_owned()],
    );
    assert_eq!(packed, read(&folder_root, &["a".into(), "b".into()]));
    assert_eq!(packed, NativeAsset::Dc6(sheet(31)));
    // Base alone is untouched by the packs.
    assert_eq!(read(&pack_root, &[]), NativeAsset::Dc6(sheet(7)));
    // The order file can name a pack too.
    put(&pack_root, "mods/order.toml", b"order = [\"a.d2pack\"]\n");
    let src = NativeSource::open(&pack_root).unwrap();
    assert_eq!(
        src.read_native(SHEET).unwrap().unwrap(),
        NativeAsset::Dc6(sheet(21))
    );
}

#[test]
fn pack_entries_are_recognised_and_named_by_their_file() {
    for (entry, name) in [
        ("mxl", "mxl"),
        ("mxl.d2pack", "mxl"),
        ("C:/games/x/my-mod.D2PACK", "my-mod"),
        ("/opt/game/m_1.d2pack", "m_1"),
        ("dir\\m.d2pack", "m"),
    ] {
        assert!(valid_mod_entry(entry), "{entry}");
        assert_eq!(mod_name(entry), name, "{entry}");
    }
    assert!(!is_pack_entry("mxl"));
    for bad in ["", ".d2pack", "a b.d2pack", "../x", "a/b", "m.zip"] {
        assert!(!valid_mod_entry(bad), "{bad}");
    }
}

#[test]
fn a_folder_and_a_pack_of_the_same_mod_are_refused_together() {
    let root = root("both");
    let dir = layer(&root, "m", "", 41);
    pack::build(&dir, &root.join("mods/m.d2pack"), 1).unwrap();
    let e = LayerStack::with_mods(&root, &["m".into()])
        .unwrap_err()
        .to_string();
    assert!(e.contains("mods/m/") && e.contains("mods/m.d2pack"), "{e}");
    // Without the folder the pack is the layer.
    std::fs::remove_dir_all(&dir).unwrap();
    let s = LayerStack::with_mods(&root, &["m".into()]).unwrap();
    assert!(matches!(s.layers[1].files, LayerFiles::Pack(_)));
    // The same mod twice (by name and by pack) is refused.
    let e = LayerStack::with_mods(&root, &["m".into(), "m.d2pack".into()])
        .unwrap_err()
        .to_string();
    assert!(e.contains("enabled twice"), "{e}");
}

#[test]
fn a_pack_breaking_a_mod_rule_is_refused_like_a_folder() {
    let root = root("rules");
    let dir = layer(&root, "m", "", 51);
    std::fs::remove_file(dir.join("files").join(format!("{SHEET}.png"))).unwrap();
    let out = root.join("mods/m.d2pack");
    std::fs::rename(&dir, root.join("m-src")).unwrap();
    pack::build(&root.join("m-src"), &out, 1).unwrap();
    let e = LayerStack::with_mods(&root, &["m".into()])
        .unwrap_err()
        .to_string();
    assert!(e.contains(&format!("{SHEET}.png")), "{e}");
}

/// Builds a pack of a fresh layer and returns its path and bytes.
fn built(name: &str) -> (PathBuf, PathBuf, Vec<u8>) {
    let root = root(name);
    let dir = layer(&root, "m", "", 61);
    let out = root.join("mods/m.d2pack");
    let moved = root.join("m-src");
    std::fs::rename(&dir, &moved).unwrap();
    pack::build(&moved, &out, 2).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    (root, out, bytes)
}

#[test]
fn a_cut_off_pack_is_refused_when_opened() {
    let (root, out, bytes) = built("cut");
    for keep in [0, 10, 40, bytes.len() / 2, bytes.len() - 1] {
        std::fs::write(&out, &bytes[..keep]).unwrap();
        let e = Pack::open(&out).unwrap_err().to_string();
        assert!(e.contains("m.d2pack"), "{keep}: {e}");
        assert!(
            e.contains("cut off") || e.contains("too short") || e.contains("not a data pack"),
            "{keep}: {e}"
        );
        // Through the stack the mod is refused, naming the pack.
        let e = LayerStack::with_mods(&root, &["m".into()])
            .unwrap_err()
            .to_string();
        assert!(e.contains("m.d2pack"), "{keep}: {e}");
    }
    assert!(pack::verify(&out, None).is_err());
}

#[test]
fn a_damaged_pack_is_refused_with_a_clear_error() {
    let (root, out, bytes) = built("damage");
    let damaged = |at: usize| {
        let mut b = bytes.clone();
        b[at] ^= 0x5a;
        std::fs::write(&out, &b).unwrap();
    };
    // The header: another format.
    damaged(0);
    let e = Pack::open(&out).unwrap_err().to_string();
    assert!(e.contains("not a data pack"), "{e}");
    let mut b = bytes.clone();
    b[8] = 9;
    std::fs::write(&out, &b).unwrap();
    let e = Pack::open(&out).unwrap_err().to_string();
    assert!(e.contains("format 9 is not known"), "{e}");

    // The index (just before the 56-byte trailer): its checksum fails.
    damaged(bytes.len() - 60);
    let e = Pack::open(&out).unwrap_err().to_string();
    assert!(e.contains("index does not match"), "{e}");

    // A file's bytes: the pack opens, the read of that file fails and
    // names it, and verify lists it.
    std::fs::write(&out, &bytes).unwrap();
    let good = Pack::open(&out).unwrap();
    let victim = "files/data/a/b/c/d/deep.txt";
    let all: Vec<String> = good.paths().map(str::to_owned).collect();
    drop(good);
    // The first data byte belongs to the first file in index order.
    damaged(16);
    let p = Pack::open(&out).unwrap();
    let e = p.read(&all[0]).unwrap().unwrap_err().to_string();
    assert!(e.contains(&all[0]) && e.contains("m.d2pack"), "{e}");
    assert!(
        e.contains("checksum mismatch") || e.contains("decompress"),
        "{e}"
    );
    assert!(p.read(victim).unwrap().is_ok());
    let problems = pack::verify(&out, None).unwrap();
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains(&all[0]), "{problems:?}");
    drop(p);
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

#[test]
fn verify_against_the_folder_names_every_difference() {
    let root = root("against");
    let dir = layer(&root, "m", "", 71);
    let out = root.join("m.d2pack");
    pack::build(&dir, &out, 1).unwrap();
    put(&dir, "files/data/new.txt", b"new");
    put(&dir, "files/data/a/b/c/d/deep.txt", b"changed");
    std::fs::remove_file(dir.join("files/data/empty.bin")).unwrap();
    let problems = pack::verify(&out, Some(&dir)).unwrap().join("\n");
    assert!(problems.contains("files/data/new.txt: in the folder but not in the pack"));
    assert!(problems.contains("files/data/a/b/c/d/deep.txt: differs"));
    assert!(problems.contains("files/data/empty.bin: not in the folder"));
}

#[test]
fn building_from_a_missing_folder_fails_and_leaves_no_file() {
    let root = scratch("missing");
    let out = root.join("x.d2pack");
    let e = pack::build(&root.join("nope"), &out, 1).unwrap_err();
    assert!(e.to_string().contains("not a folder"), "{e}");
    assert!(!out.exists());
    assert!(std::fs::read_dir(&root).unwrap().next().is_none());
}
