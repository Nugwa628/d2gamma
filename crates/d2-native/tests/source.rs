//! The native source against the archives, on the synthetic install (no
//! game files): every asset decodes to the same value from both sides and
//! the table loader builds the same tables; a later mod wins over an
//! earlier one; a broken mod or a bad manifest refuses the whole root.

use std::path::{Path, PathBuf};

use d2_data::bin;
use d2_formats::mpq::ArchiveSet;
use d2_formats::wav::Wav;
use d2_native::kind::{grey_palette, NativeKind};
use d2_native::manifest::{write_files_tsv, Manifest};
use d2_native::source::{
    decode_original, fold, sha256_hex, NativeAsset, NativeSource, SourceError, KNOWN_KINDS,
    RAND_TRANSFORMS,
};
use test_fixtures::install;
use test_fixtures::sprites::{dc6_file, dc6_frames, Dc6Shape};
use test_fixtures::synth::synthetic;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("d2native-source-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn dc6(seed: u32) -> Vec<u8> {
    let shape = Dc6Shape {
        directions: 1,
        frames: 2,
        width: 5,
        height: 4,
        panel: false,
    };
    dc6_file(&dc6_frames(shape, seed), 1, 2)
}

fn pal(seed: u8) -> Vec<u8> {
    (0..768u32).map(|i| (i as u8).wrapping_mul(seed)).collect()
}

/// A mono 16-bit, 22,050 Hz sound.
fn wav(seed: i16) -> Wav {
    Wav {
        format_tag: 1,
        channels: 1,
        rate: 22_050,
        byte_rate: 44_100,
        bits: 16,
        block_align: 2,
        samples: (0..64).map(|i| i * seed).collect(),
    }
}

/// A palette shift file: 8 maps of 256 indices.
fn shift(seed: u8) -> Vec<u8> {
    (0..8 * 256u32)
        .map(|i| (i as u8).wrapping_mul(seed))
        .collect()
}

/// Writes the native files of decoded asset `a` (canonical path `p`)
/// under `dir` (a layer folder), as the converter does.
fn write_native(dir: &Path, p: &str, a: &NativeAsset) {
    let view = grey_palette();
    let files = match a {
        NativeAsset::Dc6(x) => x.write(p, &view).unwrap(),
        NativeAsset::Dt1(x) => x.write(p, &view).unwrap(),
        NativeAsset::Pal(x) => x.write(p, &view).unwrap(),
        NativeAsset::Ds1(x) => {
            put(
                dir,
                &format!("{p}.toml"),
                d2_native::toml_kinds::write_ds1(p, x).unwrap().as_bytes(),
            );
            return;
        }
        NativeAsset::Tbl(x) => {
            let t = d2_native::tbl::write_tbl(p, x, true).unwrap();
            put(dir, &format!("{p}.tsv"), t.tsv.as_bytes());
            put(dir, &format!("{p}.toml"), t.toml.as_bytes());
            return;
        }
        NativeAsset::Excel(b) | NativeAsset::PalShift(b) => {
            put(dir, p, b);
            return;
        }
        NativeAsset::Wav(w) => {
            put(dir, p, &d2_native::wav::write_wav(p, w).unwrap());
            return;
        }
        other => panic!("{p}: no writer in this test for {other:?}"),
    };
    for f in files {
        put(dir, &f.path, &f.bytes);
    }
}

fn manifest(root: &Path, format_version: Option<u32>) {
    let tsv = write_files_tsv(&[]);
    put(root, "files.tsv", tsv.as_bytes());
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
    let mut text = m.to_toml();
    if let Some(v) = format_version {
        text = text.replace("format_version = 1", &format!("format_version = {v}"));
    }
    put(root, "manifest.toml", text.as_bytes());
}

const SHEET: &str = "data/global/ui/panel/sourcetest.dc6";
const PAL: &str = "data/global/palette/act1/pal.dat";
const PALSHIFT: &str = "data/global/monsters/zm/cof/palshift.dat";
const SOUND: &str = "data/global/sfx/cursor/sourcetest.wav";
const ITEM: &str = "data/global/items/zz_mod_inventory.dc6";
const MAP: &str = "data/global/tiles/zz_mod_room.ds1";
const TILES: &str = "data/global/tiles/zz_mod_tiles.dt1";

fn mod_layer(root: &Path, name: &str) -> PathBuf {
    put(
        root,
        &format!("mods/{name}/mod.toml"),
        format!("name = \"{name}\"\nversion = \"1\"\nnative_format_version = 1\n").as_bytes(),
    );
    root.join(format!("mods/{name}/files"))
}

fn map(seed: u32) -> NativeAsset {
    use d2_formats::ds1::Ds1;
    NativeAsset::Ds1(Ds1 {
        version: 18,
        width: 1,
        height: 1,
        act: 1,
        tag_type: 0,
        files: vec![],
        unknown_header: None,
        walls: vec![],
        orientations: vec![],
        floors: vec![vec![seed]],
        shadow: vec![0],
        tags: None,
        objects: vec![],
        unknown_groups: None,
        groups: vec![],
        groups_truncated: false,
        paths: vec![],
        trailing: vec![],
    })
}

fn tiles(seed: u8) -> NativeAsset {
    use d2_formats::dt1::{Dt1, Dt1Block, Dt1Tile};
    NativeAsset::Dt1(Dt1 {
        version: 7,
        minor_version: 6,
        tiles: vec![Dt1Tile {
            light_direction: 0,
            roof_height: 0,
            material_flags: 0,
            height: -32,
            width: 32,
            unknown_height: 0,
            orientation: 0,
            main_index: 0,
            sub_index: 0,
            rarity: 1,
            unknown_color: 0,
            subtile_flags: [0; 25],
            unknown_58: 0,
            cache_index: 0,
            unknown_5c: 0,
            blocks: vec![Dt1Block {
                x: 0,
                y: 0,
                unknown1: 0,
                grid_x: 0,
                grid_y: 0,
                format: 0x1001,
                unknown2: 0,
                pixels: vec![seed; 32 * 32],
            }],
        }],
    })
}

/// A patch statement over fixture row zero, preserving the expected old value.
fn set_cell(root: &Path, table: &str, column: &str, value: &str) -> String {
    let txt = d2_data::txt::TxtTable::parse(
        table,
        &std::fs::read(root.join(format!("base/data/global/excel/{table}.txt"))).unwrap(),
    )
    .unwrap();
    let c = txt
        .header
        .iter()
        .position(|h| h.eq_ignore_ascii_case(column.as_bytes()))
        .unwrap();
    let token = |b: &[u8]| String::from_utf8(d2_data::patch::canonical_token(b)).unwrap();
    let rules = d2_data::patch::rules_for(bin::DataRules::Original);
    let rule = rules.iter().find(|r| r.name == table).unwrap();
    let patched = d2_data::patch::PatchTable::from_base(
        rule,
        "test",
        &std::fs::read(root.join(format!("base/data/global/excel/{table}.txt"))).unwrap(),
    )
    .unwrap();
    format!(
        "table {table}\nset #0 {} {} {} -> {}\n",
        token(patched.key(0)),
        token(column.as_bytes()),
        token(&txt.records[0].cells[c]),
        token(value.as_bytes())
    )
}

#[test]
fn mod_assets_override_and_add_through_folders_and_packs() {
    let p = pair("asset-stack");
    let root = &p.native;
    write_native(&root.join("base"), MAP, &map(1));
    write_native(&root.join("base"), TILES, &tiles(1));
    let a = mod_layer(root, "a");
    let b = mod_layer(root, "b");
    let art = decode_original(ITEM, &dc6(19)).unwrap().unwrap();
    for (dir, seed) in [(&a, 2), (&b, 3)] {
        write_native(dir, SHEET, &art);
        write_native(dir, SOUND, &NativeAsset::Wav(wav(seed as i16)));
        write_native(dir, "data/global/sfx/zz_new.wav", &NativeAsset::Wav(wav(9)));
        write_native(dir, MAP, &map(seed));
        write_native(dir, TILES, &tiles(seed as u8));
        write_native(dir, "data/global/tiles/zz_added.ds1", &map(9));
    }
    // A table patch may reference an asset/string supplied by a later layer.
    write_native(&b, ITEM, &art);
    let palette = decode_original(PAL, &pal(13)).unwrap().unwrap();
    write_native(&b, PAL, &palette);
    let patch = format!(
        "d2patch 1\n{}{}{}{}{}",
        set_cell(root, "weapons", "invfile", "zz_mod_inventory"),
        set_cell(root, "weapons", "namestr", "zz_asset_name"),
        set_cell(root, "sounds", "FileName", "zz_new.wav"),
        set_cell(root, "lvlprest", "File1", "zz_mod_room.ds1"),
        set_cell(root, "lvltypes", "File 1", "zz_mod_tiles.dt1")
    );
    put(&a, "patches/a.d2patch", patch.as_bytes());
    put(
        &b,
        "patches/b.d2patch",
        b"d2patch 1\nstradd zz_asset_name [New item]\nstrset zz_asset_name [Later item]\n",
    );
    for name in ["a", "b"] {
        d2_native::pack::build(
            &root.join(format!("mods/{name}")),
            &root.join(format!("{name}.d2pack")),
            1,
        )
        .unwrap();
    }
    let packs: Vec<String> = ["a", "b"]
        .map(|n| {
            root.join(format!("{n}.d2pack"))
                .to_string_lossy()
                .into_owned()
        })
        .into();
    for order in [vec!["a".into(), "b".into()], packs] {
        let src = NativeSource::open_with(root, Some(&order)).unwrap();
        for (path, expected) in [
            (ITEM, art.clone()),
            (SHEET, art.clone()),
            (SOUND, NativeAsset::Wav(wav(3))),
            ("data/global/sfx/zz_new.wav", NativeAsset::Wav(wav(9))),
            (MAP, map(3)),
            (TILES, tiles(3)),
            (PAL, palette.clone()),
            ("data/global/tiles/zz_added.ds1", map(9)),
        ] {
            assert_eq!(src.read_native(path).unwrap().unwrap(), expected, "{path}");
        }
        let loaded = bin::load_from(&src.tables("eng").unwrap(), "eng").unwrap();
        let id = loaded.strings.lookup(b"zz_asset_name").unwrap();
        assert_eq!(id, d2_data::strings::MOD_BASE);
        assert_eq!(
            loaded.strings.by_index(id).unwrap(),
            "Later item".encode_utf16().collect::<Vec<_>>()
        );
    }
    let src = NativeSource::open_with(root, Some(&[])).unwrap();
    assert!(src.read_native(ITEM).is_none());
    assert_eq!(src.read_native(MAP).unwrap().unwrap(), map(1));
    assert_eq!(src.read_native(TILES).unwrap().unwrap(), tiles(1));
    assert_eq!(
        src.read_native(SOUND).unwrap().unwrap(),
        NativeAsset::Wav(wav(3))
    );
}

#[test]
fn new_asset_references_must_exist_and_decode() {
    let p = pair("asset-references");
    let root = &p.native;
    let dir = mod_layer(root, "m");
    let order = ["m".into()];
    for (table, column, value) in [
        ("weapons", "invfile", "zz_missing"),
        ("weapons", "namestr", "zz_missing"),
        ("sounds", "FileName", "zz_missing.wav"),
        ("lvlprest", "File1", "zz_missing.ds1"),
        ("lvltypes", "File 1", "zz_missing.dt1"),
        ("weapons", "invfile", "../zz_outside"),
        (
            "weapons",
            "invfile",
            "zz_filename_is_far_longer_than_thirty_one_bytes",
        ),
    ] {
        put(
            &dir,
            "patches/a.d2patch",
            format!("d2patch 1\n{}", set_cell(root, table, column, value)).as_bytes(),
        );
        let src = NativeSource::open_with(root, Some(&order)).unwrap();
        let e = src.tables("eng").unwrap_err().to_string();
        assert!(
            e.contains(table) && e.contains(column) && e.contains(value),
            "{e}"
        );
        if value.starts_with("../") {
            assert!(e.contains("without . or .."), "{e}");
        }
        if value.len() > 31 {
            assert!(e.contains("truncated"), "{e}");
        }
    }
    let patch = format!(
        "d2patch 1\n{}",
        set_cell(root, "weapons", "invfile", "zz_mod_inventory")
    );
    put(&dir, "patches/a.d2patch", patch.as_bytes());
    let art = decode_original(ITEM, &dc6(19)).unwrap().unwrap();
    write_native(&dir, ITEM, &art);
    put(
        &dir,
        &format!("{ITEM}.png"),
        &d2_native::png::write_indexed("tiny", 1, 1, &[1], &grey_palette()).unwrap(),
    );
    let src = NativeSource::open_with(root, Some(&order)).unwrap();
    let e = src.tables("eng").unwrap_err().to_string();
    assert!(
        e.contains(ITEM) && e.contains("sidecar") && e.contains("layer `m`"),
        "{e}"
    );
    // The renderer also accepts a DC6 overlay when the DCC is absent.
    write_native(&dir, "data/global/overlays/zz_effect.dc6", &art);
    put(
        &dir,
        "patches/a.d2patch",
        format!(
            "d2patch 1\n{}",
            set_cell(root, "overlay", "Filename", "zz_effect")
        )
        .as_bytes(),
    );
    NativeSource::open_with(root, Some(&order))
        .unwrap()
        .tables("eng")
        .unwrap();
}

#[test]
fn bad_mod_assets_report_the_file_and_layer_for_folders_and_packs() {
    let p = pair("bad-assets");
    let root = &p.native;
    let dir = mod_layer(root, "m");
    let art = decode_original(ITEM, &dc6(11)).unwrap().unwrap();
    write_native(&dir, ITEM, &art);
    let valid_png = std::fs::read(dir.join(format!("{ITEM}.png"))).unwrap();
    // Remove only the synthetic PNG's palette chunk, leaving all other CRCs valid.
    let mut no_palette = valid_png[..8].to_vec();
    let mut at = 8;
    while at < valid_png.len() {
        let size = u32::from_be_bytes(valid_png[at..at + 4].try_into().unwrap()) as usize;
        let end = at + size + 12;
        if &valid_png[at + 4..at + 8] != b"PLTE" {
            no_palette.extend_from_slice(&valid_png[at..end]);
        }
        at = end;
    }
    let tiny = d2_native::png::write_indexed("tiny", 1, 1, &[1], &grey_palette()).unwrap();
    let mut rgb = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut rgb, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[1, 2, 3]).unwrap();
        writer.finish().unwrap();
    }
    let map_text = match map(1) {
        NativeAsset::Ds1(d) => d2_native::toml_kinds::write_ds1(MAP, &d).unwrap(),
        _ => unreachable!(),
    };
    let mut wrong_rate = wav(1);
    wrong_rate.rate = 44_100;
    wrong_rate.byte_rate = 88_200;
    let mut wrong_align = wav(1);
    wrong_align.block_align = 4;
    let bad_sounds =
        [wrong_rate, wrong_align].map(|w| d2_native::wav::write_wav(SOUND, &w).unwrap());
    let cases = [
        (ITEM, format!("{ITEM}.png"), no_palette, "palette"),
        (ITEM, format!("{ITEM}.png"), tiny, "sidecar"),
        (ITEM, format!("{ITEM}.png"), rgb, "must be indexed"),
        (ITEM, format!("{ITEM}.png"), b"not a PNG".to_vec(), "PNG"),
        (SOUND, SOUND.to_owned(), bad_sounds[0].clone(), "22050"),
        (
            SOUND,
            SOUND.to_owned(),
            bad_sounds[1].clone(),
            "block_align",
        ),
        (SOUND, SOUND.to_owned(), b"not a WAV".to_vec(), "44"),
        (
            MAP,
            format!("{MAP}.toml"),
            map_text
                .replace("width = 1", "width = 4294967295")
                .into_bytes(),
            "expected 4294967295",
        ),
        (
            PAL,
            format!("{PAL}.pal"),
            b"JASC-PAL\n0100\n256".to_vec(),
            "missing colour",
        ),
    ];
    for (path, file, bytes, message) in cases {
        // Reset the sprite so each case tests just its own corruption.
        write_native(&dir, ITEM, &art);
        put(&dir, &file, &bytes);
        let packed = root.join("bad.d2pack");
        d2_native::pack::build(&root.join("mods/m"), &packed, 1).unwrap();
        for entry in ["m".to_owned(), packed.to_string_lossy().into_owned()] {
            let src = NativeSource::open_with(root, Some(&[entry])).unwrap();
            let error = src.read_native(path).unwrap().unwrap_err();
            assert!(
                error.contains(path) && error.contains(message) && error.contains("layer `"),
                "{error}"
            );
        }
    }
    // Base files are still decoded as before, including unplayable source audio.
    let mut base_sound = wav(1);
    base_sound.rate = 44_100;
    write_native(
        &root.join("base"),
        SOUND,
        &NativeAsset::Wav(base_sound.clone()),
    );
    let src = NativeSource::open_with(root, Some(&[])).unwrap();
    assert_eq!(
        src.read_native(SOUND).unwrap().unwrap(),
        NativeAsset::Wav(base_sound)
    );
}

/// The synthetic install with a sprite sheet and a palette added, and its
/// conversion: the archives, the native root and every converted path.
struct Pair {
    archives: ArchiveSet,
    native: PathBuf,
    names: Vec<String>,
}

fn pair(name: &str) -> Pair {
    let dir = scratch(name);
    let mut data = synthetic();
    // The synthetic schema lookup list has only sound names; the runtime
    // sound table also accepts file paths, authored here without game data.
    let sounds = data.tables.files.get_mut("sounds.txt").unwrap();
    sounds.columns.push("FileName".into());
    for row in &mut sounds.rows {
        row.push(String::new());
    }
    data.files.push((SHEET.replace('/', "\\"), dc6(7)));
    data.files.push((PAL.replace('/', "\\"), pal(3)));
    data.files.push((PALSHIFT.replace('/', "\\"), shift(5)));
    data.files.push((
        SOUND.replace('/', "\\"),
        d2_native::wav::write_wav(SOUND, &wav(3)).unwrap(),
    ));
    data.files
        .push((r"data\global\monsters\RandTransforms.dat".into(), shift(7)));
    let inst = install::build(&dir.join("install"), &data).unwrap();
    let native = dir.join("native");
    let base = native.join("base");

    let mut names: Vec<String> = data.files.iter().map(|(n, _)| fold(n)).collect();
    for f in ["string.tbl", "patchstring.tbl", "expansionstring.tbl"] {
        names.push(format!("data/local/lng/eng/{f}"));
    }
    for (txt, _) in data.tables.render() {
        names.push(format!("data/global/excel/{}", txt.to_ascii_lowercase()));
    }
    names.push("data/global/excel/soundenviron.txt".into());
    names.sort();
    names.dedup();
    for p in &names {
        let bytes = inst.archives.read(&p.replace('/', "\\")).unwrap();
        let a = decode_original(p, &bytes).expect("converted kind").unwrap();
        write_native(&base, p, &a);
    }
    manifest(&native, None);
    Pair {
        archives: inst.archives,
        native,
        names,
    }
}

#[test]
fn publication_checks_compile_references_and_decode_new_assets() {
    let p = pair("publication-checks");
    let folder = p.native.join("editable");
    put(&folder, "mod.toml", b"id = 'test-mod'\nname = 'Test Mod'\nversion = '1.0.0'\nauthor = 'A'\ndescription = ''\napi = 1\nkind = 'full'\n");
    write_native(
        &folder.join("files"),
        "data/global/sfx/new.wav",
        &NativeAsset::Wav(wav(11)),
    );
    d2_native::package::check_mod(&folder, &p.native).unwrap();
    let out = p.native.join("test-mod.d2pack");
    d2_native::package::package(&folder, &p.native, &out, &p.native.join("user/author.pk8"))
        .unwrap();
    let packed = NativeSource::open_with(&p.native, Some(&[out.display().to_string()])).unwrap();
    assert_eq!(
        packed
            .read_native("data/global/sfx/new.wav")
            .unwrap()
            .unwrap(),
        NativeAsset::Wav(wav(11))
    );
    put(&folder, "files/data/global/sfx/new.wav", b"broken wave");
    assert!(d2_native::package::check_mod(&folder, &p.native)
        .unwrap_err()
        .contains("new.wav"));
}

#[test]
fn native_equals_mpq_assets_and_tables() {
    let p = pair("same");
    let src = NativeSource::open(&p.native).unwrap();
    let mut kinds = std::collections::HashSet::new();
    for name in &p.names {
        let bytes = p.archives.read(&name.replace('/', "\\")).unwrap();
        let mpq = decode_original(name, &bytes).unwrap().unwrap();
        let native = src.read_native(name).expect("native file").unwrap();
        kinds.insert(std::mem::discriminant(&mpq));
        if let (NativeAsset::Tbl(_), NativeAsset::Tbl(_)) = (&mpq, &native) {
            continue; // compared through the loaded string tables below
        }
        assert_eq!(mpq, native, "{name}");
    }
    assert!(kinds.len() >= 5, "kinds covered: {}", kinds.len());

    // Tables: the same loader, both sources.
    let a = bin::load(&p.archives, "eng").unwrap();
    let b = bin::load_from(&src.tables("eng").unwrap(), "eng").unwrap();
    assert_eq!(a.lod, b.lod);
    assert_eq!(a.tables.len(), b.tables.len());
    for (x, y) in a.tables.iter().zip(&b.tables) {
        assert_eq!((&x.name, x.records.len()), (&y.name, y.records.len()));
        assert_eq!(x.records, y.records, "table {}", x.name);
    }
    assert_eq!(a.hitclass.records, b.hitclass.records);
    for (k, c) in &a.code {
        assert_eq!(c.bytes, b.code[k].bytes, "{k:?}");
    }
    assert_eq!(a.sounds, b.sounds);
    assert_eq!(a.soundenviron, b.soundenviron);
    for base in [0u32, 10_000, 20_000] {
        for id in base..base + 64 {
            assert_eq!(
                a.strings.by_index(id),
                b.strings.by_index(id),
                "string {id}"
            );
        }
    }
}

#[test]
fn palette_shifts_come_from_the_owning_layer() {
    let p = pair("palshift");
    let root = &p.native;
    let read = |src: &NativeSource, path: &str| src.read_native(path).unwrap().unwrap();
    let base = NativeSource::open(root).unwrap();
    assert_eq!(read(&base, PALSHIFT), NativeAsset::PalShift(shift(5)));
    assert_eq!(
        read(&base, RAND_TRANSFORMS),
        NativeAsset::PalShift(shift(7))
    );

    // A mod replaces a class's shifts and adds its own class (a `#` token).
    let own = "data/global/monsters/#b/cof/palshift.dat";
    put(
        root,
        "mods/m/mod.toml",
        b"name = \"m\"\nversion = \"1\"\nnative_format_version = 1\nrequires = []\n",
    );
    put(root, &format!("mods/m/files/{PALSHIFT}"), &shift(9));
    put(root, &format!("mods/m/files/{own}"), &shift(13));
    put(root, "mods/order.toml", b"order = [\"m\"]\n");
    let src = NativeSource::open(root).unwrap();
    assert_eq!(read(&src, PALSHIFT), NativeAsset::PalShift(shift(9)));
    assert_eq!(read(&src, own), NativeAsset::PalShift(shift(13)));
    assert_eq!(read(&src, RAND_TRANSFORMS), NativeAsset::PalShift(shift(7)));
    assert!(base.read_native(own).is_none());
}

#[test]
fn sounds_come_from_the_owning_layer() {
    let p = pair("sound");
    let root = &p.native;
    let read = |src: &NativeSource, path: &str| src.read_native(path).unwrap().unwrap();
    let base = NativeSource::open(root).unwrap();
    assert_eq!(read(&base, SOUND), NativeAsset::Wav(wav(3)));

    // A mod replaces a sound and adds its own.
    let own = "data/global/sfx/monster/newclass/attack1.wav";
    put(
        root,
        "mods/m/mod.toml",
        b"name = \"m\"\nversion = \"1\"\nnative_format_version = 1\nrequires = []\n",
    );
    let native = |w: &Wav| d2_native::wav::write_wav("x.wav", w).unwrap();
    put(root, &format!("mods/m/files/{SOUND}"), &native(&wav(5)));
    put(root, &format!("mods/m/files/{own}"), &native(&wav(7)));
    put(root, "mods/order.toml", b"order = [\"m\"]\n");
    let src = NativeSource::open(root).unwrap();
    assert_eq!(read(&src, SOUND), NativeAsset::Wav(wav(5)));
    assert_eq!(read(&src, own), NativeAsset::Wav(wav(7)));

    // A native sound is a canonical WAV: any other layout is refused,
    // naming the file.
    put(
        root,
        &format!("mods/m/files/{own}"),
        &[native(&wav(7)), vec![0]].concat(),
    );
    let src = NativeSource::open(root).unwrap();
    let e = src.read_native(own).unwrap().unwrap_err();
    assert!(e.contains(own), "{e}");
}

#[test]
fn later_mod_wins_and_bad_mods_are_refused() {
    let p = pair("mods");
    let root = &p.native;
    let ok_mod = |name: &str, requires: &str| {
        put(
            root,
            &format!("mods/{name}/mod.toml"),
            format!(
                "name = \"{name}\"\nversion = \"1\"\nnative_format_version = 1\nrequires = [{requires}]\n"
            )
            .as_bytes(),
        );
    };
    let sheet = |seed| match decode_original(SHEET, &dc6(seed)).unwrap().unwrap() {
        NativeAsset::Dc6(d) => d,
        _ => unreachable!(),
    };
    ok_mod("a", "");
    ok_mod("b", "\"a\"");
    write_native(
        &root.join("mods/a/files"),
        SHEET,
        &NativeAsset::Dc6(sheet(11)),
    );
    write_native(
        &root.join("mods/b/files"),
        SHEET,
        &NativeAsset::Dc6(sheet(12)),
    );
    let read = || NativeSource::open(root).map(|s| s.read_native(SHEET).unwrap().unwrap());

    // No order file: base.
    assert_eq!(read().unwrap(), NativeAsset::Dc6(sheet(7)));
    put(root, "mods/order.toml", b"order = [\"a\", \"b\"]\n");
    assert_eq!(read().unwrap(), NativeAsset::Dc6(sheet(12)));
    put(root, "mods/order.toml", b"order = [\"a\"]\n");
    assert_eq!(read().unwrap(), NativeAsset::Dc6(sheet(11)));
    // A caller's own layer list wins over the order file: base alone, or
    // its own mods.
    let read_with = |order: &[String]| {
        NativeSource::open_with(root, Some(order)).map(|s| s.read_native(SHEET).unwrap().unwrap())
    };
    assert_eq!(read_with(&[]).unwrap(), NativeAsset::Dc6(sheet(7)));
    let ab = ["a".to_string(), "b".to_string()];
    assert_eq!(read_with(&ab).unwrap(), NativeAsset::Dc6(sheet(12)));
    // `b` requires `a` earlier in the order.
    put(root, "mods/order.toml", b"order = [\"b\", \"a\"]\n");
    let e = read().unwrap_err().to_string();
    assert!(e.contains("requires `a`"), "{e}");

    // A sheet without its sidecar is refused, naming it.
    put(root, "mods/order.toml", b"order = [\"a\"]\n");
    std::fs::remove_file(root.join(format!("mods/a/files/{SHEET}.toml"))).unwrap();
    let e = read().unwrap_err().to_string();
    assert!(e.contains(&format!("{SHEET}.toml")), "{e}");
    write_native(
        &root.join("mods/a/files"),
        SHEET,
        &NativeAsset::Dc6(sheet(11)),
    );

    // A mod `.txt` is refused.
    put(root, "mods/a/files/data/global/excel/weapons.txt", b"x");
    let e = read().unwrap_err().to_string();
    assert!(e.contains("data/global/excel/weapons.txt"), "{e}");
}

#[test]
fn bad_manifest_is_refused() {
    let p = pair("manifest");
    manifest(&p.native, Some(2));
    let e = NativeSource::open(&p.native).unwrap_err();
    assert!(matches!(e, SourceError::Root { .. }));
    let msg = e.to_string();
    assert!(msg.contains("format_version 2"), "{msg}");
    assert!(msg.contains("d2-convert"), "{msg}");

    // An edited files.tsv no longer matches the manifest's hash.
    manifest(&p.native, None);
    put(&p.native, "files.tsv", b"tampered\n");
    let msg = NativeSource::open(&p.native).unwrap_err().to_string();
    assert!(msg.contains("files_sha256"), "{msg}");

    // No manifest at all.
    std::fs::remove_file(p.native.join("manifest.toml")).unwrap();
    let msg = NativeSource::open(&p.native).unwrap_err().to_string();
    assert!(msg.contains("manifest.toml"), "{msg}");
}

/// A mod's string edits reach the loaded string tables: an added key reads
/// at the first mod id, every table string reads as before, and a refused
/// edit refuses the root.
#[test]
fn mod_string_edits_load() {
    let p = pair("strings");
    let root = &p.native;
    put(
        root,
        "mods/s/mod.toml",
        b"name = \"s\"\nversion = \"1\"\nnative_format_version = 1\nrequires = []\n",
    );
    put(root, "mods/order.toml", b"order = [\"s\"]\n");
    put(
        root,
        "mods/s/files/patches/a.d2patch",
        b"d2patch 1\nstradd zz_mod_key [Mod text]\n",
    );
    let base = bin::load(&p.archives, "eng").unwrap();
    let src = NativeSource::open(root).unwrap();
    let t = bin::load_from(&src.tables("eng").unwrap(), "eng").unwrap();
    let first = d2_data::strings::MOD_BASE;
    assert_eq!(t.strings.id(b"zz_mod_key"), first);
    assert_eq!(
        t.strings.by_index(first).unwrap(),
        "Mod text".encode_utf16().collect::<Vec<_>>()
    );
    for base_id in [0u32, 10_000, 20_000] {
        for id in base_id..base_id + 64 {
            assert_eq!(t.strings.by_index(id), base.strings.by_index(id), "{id}");
        }
    }

    put(
        root,
        "mods/s/files/patches/b.d2patch",
        b"d2patch 1\nstrset zz_no_such_key x\n",
    );
    let src = NativeSource::open(root).unwrap();
    let e = src.tables("eng").unwrap_err().to_string();
    assert!(e.contains("A19"), "{e}");
}

#[test]
fn color_txt_is_served_verbatim() {
    use d2_native::source::{AssetKind, COLOR_TXT};

    // The green-blood switch file: a copied kind under its folded name.
    assert_eq!(
        AssetKind::of(&fold(r"Data\Local\Color.txt")),
        Some(AssetKind::ColorTxt)
    );
    assert_eq!(AssetKind::of("data/local/other.txt"), None);
    assert_eq!(AssetKind::ColorTxt.primary(COLOR_TXT), COLOR_TXT);

    let p = pair("colortxt");
    put(&p.native.join("base"), COLOR_TXT, b"1");
    let src = NativeSource::open(&p.native).unwrap();
    assert!(src.contains(COLOR_TXT));
    assert_eq!(
        src.read_native(COLOR_TXT).unwrap().unwrap(),
        NativeAsset::ColorTxt(b"1".to_vec())
    );
    assert_eq!(
        decode_original(COLOR_TXT, b"1").unwrap().unwrap(),
        NativeAsset::ColorTxt(b"1".to_vec())
    );
}

/// A mod replaces the front end's pictures the way it replaces any sprite:
/// by shipping the file at the same path. These are the trademark screen's
/// backdrop, the main menu's backdrop and the logo animation.
#[test]
fn a_mod_replaces_the_front_end_art() {
    const ART: [&str; 3] = [
        "data/global/ui/frontend/trademarkscreenexp.dc6",
        "data/global/ui/frontend/gameselectscreenexp.dc6",
        "data/global/ui/frontend/d2logoblackleft.dc6",
    ];
    let p = pair("front-end-art");
    let root = &p.native;
    let sheet = |seed| match decode_original(ART[0], &dc6(seed)).unwrap().unwrap() {
        NativeAsset::Dc6(d) => d,
        _ => unreachable!(),
    };
    for art in ART {
        write_native(&root.join("base"), art, &NativeAsset::Dc6(sheet(7)));
    }
    put(
        root,
        "mods/a/mod.toml",
        b"name = \"a\"\nversion = \"1\"\nnative_format_version = 1\nrequires = []\n",
    );
    // The mod replaces two of the three; the logo stays the base's.
    for art in &ART[..2] {
        write_native(
            &root.join("mods/a/files"),
            art,
            &NativeAsset::Dc6(sheet(21)),
        );
    }
    let read = |order: &[String], art: &str| {
        NativeSource::open_with(root, Some(order))
            .unwrap()
            .read_native(art)
            .unwrap()
            .unwrap()
    };
    let with_mod = ["a".to_string()];
    for art in ART {
        assert_eq!(read(&[], art), NativeAsset::Dc6(sheet(7)), "{art}: base");
    }
    assert_eq!(read(&with_mod, ART[0]), NativeAsset::Dc6(sheet(21)));
    assert_eq!(read(&with_mod, ART[1]), NativeAsset::Dc6(sheet(21)));
    assert_eq!(read(&with_mod, ART[2]), NativeAsset::Dc6(sheet(7)));
}

#[test]
fn table_patch_references_are_checked_before_native_compilation() {
    let p = pair("patch-references");
    let root = &p.native;
    put(
        root,
        "mods/r/mod.toml",
        b"name = \"r\"\nversion = \"1\"\nnative_format_version = 1\nrequires = []\n",
    );
    put(root, "mods/order.toml", b"order = [\"r\"]\n");
    let path = "mods/r/files/patches/test.d2patch";
    // Add a row so this assertion does not depend on fixture base keys.
    let count = d2_data::txt::TxtTable::parse(
        "base",
        &std::fs::read(root.join("base/data/global/excel/itemtypes.txt")).unwrap(),
    )
    .unwrap()
    .records
    .len();
    let patch = format!("d2patch 1\ntable itemtypes\nadd #{count} zz1\nset zz1 equiv1 [] -> missing_type\nset zz1 equiv2 [] -> other_missing\n");
    put(root, path, patch.as_bytes());
    let src = NativeSource::open(root).unwrap();
    let error = src.tables("eng").unwrap_err().to_string();
    assert!(
        error.contains("equiv1") && error.contains("missing_type"),
        "{error}"
    );
    assert!(
        error.contains("equiv2") && error.contains("other_missing"),
        "{error}"
    );
    put(
        root,
        path,
        format!("d2patch 1\ntable itemtypes\nadd #{count} zz1\nremove zz1\n").as_bytes(),
    );
    let src = NativeSource::open(root).unwrap();
    let loaded = bin::load_from(&src.tables("eng").unwrap(), "eng").unwrap();
    assert_eq!(loaded.table("itemtypes").unwrap().count, count + 1);
}
