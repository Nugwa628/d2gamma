//! The save-check corpus run (plan N6): checks every save under the given
//! directories with the standard check list over the game data's tables,
//! and reports the counts and every failure.
//!
//! ```text
//! cargo run --release -p d2-save-checks --example corpus -- [--game-dir DIR] [--strict] DIR...
//! ```
//!
//! Saves are `.d2rc` records (the main server's envelope) and `.d2s` files.
//! Snapshots of the same character are paired in order: a directory
//! `snap/<n>/...` holds the n-th snapshot of each file under it (the same
//! relative path in a later `<n>` is the same character's next save); a
//! file outside a `snap/` tree is a series of one. The first save of a
//! series is checked against the character's creation, the next ones
//! against the save before. `--strict` exits 1 when any check refused or
//! flagged a save. The game directory defaults to `D2_GAME_DIR`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use d2_data::{bin, fixup};
use d2_formats::mpq::ArchiveSet;
use d2_save_checks::{CheckList, CheckTables, SaveInfo};

type Error = Box<dyn std::error::Error>;

/// The `.d2rc` envelope: magic, version, reserved, length, the record,
/// FNV-1a 32 of everything before.
fn record(bytes: &[u8]) -> Result<&[u8], String> {
    if bytes.len() < 16 || &bytes[..4] != b"D2RC" {
        return Err("not a D2RC envelope".into());
    }
    let len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let end = 12 + len;
    if bytes.len() != end + 4 {
        return Err(format!("envelope length {len} for {} bytes", bytes.len()));
    }
    let mut h: u32 = 0x811C_9DC5;
    for &b in &bytes[..end] {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    let sum = u32::from_le_bytes([bytes[end], bytes[end + 1], bytes[end + 2], bytes[end + 3]]);
    if h != sum {
        return Err("envelope checksum".into());
    }
    Ok(&bytes[12..end])
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.is_dir() {
            walk(&p, out)?;
        } else if matches!(p.extension().and_then(|x| x.to_str()), Some("d2rc" | "d2s")) {
            out.push(p);
        }
    }
    Ok(())
}

/// The series key and order of a save path: under `.../snap/<n>/rest`,
/// (`.../snap/rest`, n); else (path, 0).
fn series(p: &Path) -> (PathBuf, u64) {
    let parts: Vec<_> = p.components().collect();
    for i in (0..parts.len().saturating_sub(2)).rev() {
        if parts[i].as_os_str() == "snap" {
            if let Some(n) = parts[i + 1]
                .as_os_str()
                .to_str()
                .and_then(|s| s.parse().ok())
            {
                let mut key: PathBuf = parts[..=i].iter().collect();
                key.extend(&parts[i + 2..]);
                return (key, n);
            }
        }
    }
    (p.to_path_buf(), 0)
}

fn main() -> Result<(), Error> {
    let mut game = std::env::var_os("D2_GAME_DIR").map(PathBuf::from);
    let mut strict = false;
    let mut dirs = Vec::new();
    let mut args = std::env::args_os().skip(1);
    while let Some(a) = args.next() {
        match a.to_str() {
            Some("--game-dir") => game = args.next().map(PathBuf::from),
            Some("--strict") => strict = true,
            _ => dirs.push(PathBuf::from(a)),
        }
    }
    let game = game.ok_or("no game directory: --game-dir or D2_GAME_DIR")?;
    let t0 = Instant::now();
    let set = ArchiveSet::open_dir(&game)?;
    let bins = bin::load(&set, bin::DEFAULT_LANGUAGE)?;
    let anim = fixup::read_animdata(&set)?;
    let fixed = fixup::apply(&bins, &anim)?;
    let tables = Arc::new(CheckTables::from_fixed(&fixed)?);
    let list = CheckList::standard(tables);
    println!("tables loaded in {:?}", t0.elapsed());

    let mut files = Vec::new();
    for d in &dirs {
        walk(d, &mut files)?;
    }
    let mut by_series: BTreeMap<PathBuf, Vec<(u64, PathBuf)>> = BTreeMap::new();
    for f in files {
        let (key, n) = series(&f);
        by_series.entry(key).or_default().push((n, f));
    }
    let (mut saves, mut clean, mut refused, mut flagged, mut unreadable) =
        (0u32, 0u32, 0u32, 0u32, 0u32);
    let (mut total, mut slowest) = (Duration::ZERO, Duration::ZERO);
    for (key, mut snaps) in by_series {
        snaps.sort();
        let mut before: Option<Vec<u8>> = None;
        for (_, path) in snaps {
            let raw = std::fs::read(&path)?;
            let bytes = if path.extension().is_some_and(|x| x == "d2rc") {
                match record(&raw) {
                    Ok(r) => r.to_vec(),
                    Err(e) => {
                        unreadable += 1;
                        println!("UNREADABLE {}: {e}", path.display());
                        continue;
                    }
                }
            } else {
                raw
            };
            let t = Instant::now();
            let v = list.run_records(before.as_deref(), &bytes, SaveInfo::default());
            let took = t.elapsed();
            total += took;
            slowest = slowest.max(took);
            saves += 1;
            if !v.refused.is_empty() {
                refused += 1;
            }
            if !v.flagged.is_empty() {
                flagged += 1;
            }
            if !v.audit() {
                clean += 1;
            }
            for f in &v.refused {
                println!("REFUSED {} [{}]: {f}", path.display(), key.display());
            }
            for f in &v.flagged {
                println!("FLAGGED {} [{}]: {f}", path.display(), key.display());
            }
            before = Some(bytes);
        }
    }
    let mean = if saves > 0 {
        total / saves
    } else {
        Duration::ZERO
    };
    println!(
        "saves {saves}, clean {clean}, refused {refused}, flagged {flagged}, \
         unreadable {unreadable}; check time mean {mean:?}, max {slowest:?}"
    );
    if strict && (refused > 0 || flagged > 0 || unreadable > 0) {
        std::process::exit(1);
    }
    Ok(())
}
