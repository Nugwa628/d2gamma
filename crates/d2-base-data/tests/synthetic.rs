//! The real conversion and reader on a synthetic install (no game files):
//! the shared base is made from the three server archives, opens with
//! this build's reader, and two machines' conversions are the same bytes
//! (a realm compares its game data's hash with each game server's).

use std::fs;
use std::path::{Path, PathBuf};

use d2_base_data::{ensure, format_key, loadable, state, State, Steps};

fn scratch(name: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn files(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut todo = vec![root.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                todo.push(p);
            } else {
                let rel = p
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn the_base_is_converted_from_the_server_archives_and_the_same_everywhere() {
    let t = scratch("base-synthetic");
    let install = t.join("Diablo II");
    // As the real install: monstats has a record 707 (the converter's
    // table check writes its one known override for it).
    let mut data = test_fixtures::synth::synthetic();
    let f = data.tables.files.get_mut("monstats.txt").unwrap();
    let id = f.columns.iter().position(|c| c == "Id").unwrap();
    let mut row = f.rows[0].clone();
    while f.rows.len() < 708 {
        row[id] = format!("pad{}", f.rows.len());
        f.rows.push(row.clone());
    }
    test_fixtures::install::build(&install, &data).unwrap();
    let check = |dir: &Path| loadable(dir);
    let say = |_: &str| {};
    let steps = Steps::real(&check, &say);
    let a = ensure(&t.join("a"), Some(&install), &steps).unwrap();
    assert!(a.converted.is_some());
    assert_eq!(a.dir, t.join("a").join(format_key()));
    assert_eq!(state(&t.join("a")), State::Ready(a.dir.clone()));
    let got = files(&a.dir);
    assert!(
        got.iter()
            .any(|(p, _)| p == "base/data/global/excel/weapons.txt"),
        "the tables"
    );
    assert!(got
        .iter()
        .all(|(p, _)| !p.ends_with(".dcc") && p != "report.txt"));
    // Another machine, another conversion: the same files, byte for byte.
    let b = ensure(&t.join("b"), Some(&install), &steps).unwrap();
    assert_eq!(got, files(&b.dir));
    let _ = fs::remove_dir_all(&t);
}
