//! Helpers shared by the unit tests: scratch folders and a sample mod.

use std::fs;
use std::path::PathBuf;

/// A fresh empty folder under the system temp dir.
pub(crate) fn temp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("d2-modkit-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

pub(crate) const MANIFEST: &str = "id = \"test-mod\"\nname = \"Test Mod\"\nversion = \"0.1.0\"\nauthor = \"T\"\ndescription = \"d\"\napi = 1\nkind = \"full\"\n";

/// A small valid mod folder with text and binary files.
pub(crate) fn sample(name: &str) -> PathBuf {
    let d = temp(name);
    fs::write(d.join("mod.toml"), MANIFEST).unwrap();
    for dir in ["patches", "tables", "art"] {
        fs::create_dir_all(d.join("files").join(dir)).unwrap();
    }
    fs::write(d.join("files/rules.toml"), "# rules\n").unwrap();
    fs::write(
        d.join("files/patches/items.d2patch"),
        "d2patch 1\ntable weapons\n".repeat(50),
    )
    .unwrap();
    fs::write(d.join("files/tables/extra.txt"), "name\tcode\nSword\tsw1\n").unwrap();
    fs::write(d.join("files/art/pic.bin"), [0u8, 1, 2, 3, 250, 251, 252]).unwrap();
    d
}
