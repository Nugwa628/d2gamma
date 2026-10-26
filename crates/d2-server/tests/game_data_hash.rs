//! The game-data hash of a game server's data (`LiveData::data_hash`,
//! `d2_data::data_hash`): the same install hashes the same on every load,
//! and one changed byte of a table or a level file changes the hash.
//!
//! The first tests run on the playable synthetic install (CI). The last
//! (`#[ignore]`, `D2_GAME_DIR`) runs on the game's own files and prints
//! what loading and hashing cost at start-up.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use d2_formats::mpq::ArchiveSet;
use d2_server::game::LiveData;
use d2_server::game_files::GameFiles;
use test_fixtures::{install, play};

/// The playable install, built afresh in its own folder, and loaded.
fn synthetic(name: &str) -> LiveData {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("data-hash-{name}-{}", std::process::id()));
    let i = install::build(&dir, &play::playable()).unwrap_or_else(|e| panic!("{e}"));
    LiveData::load(Arc::new(GameFiles::archives(Arc::new(i.archives))))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn hex(h: [u8; 32]) -> String {
    h.iter().map(|b| format!("{b:02x}")).collect()
}

/// Flips one bit of byte `i` (wrapped to the table) of every table and
/// checks each flip gives a hash not seen before; the data is put back.
fn every_table_flip_changes_it(live: &mut LiveData, i: usize) -> usize {
    let base = live.data_hash();
    let mut seen = vec![base];
    let mut flipped = 0;
    for t in 0..live.tables.bins.tables.len() {
        let len = live.tables.bins.tables[t].records.len();
        if len == 0 {
            continue;
        }
        let at = i % len;
        live.tables.bins.tables[t].records[at] ^= 1;
        let h = live.data_hash();
        live.tables.bins.tables[t].records[at] ^= 1;
        let name = &live.tables.bins.tables[t].name;
        assert!(!seen.contains(&h), "table {name} byte {at}");
        seen.push(h);
        flipped += 1;
    }
    assert_eq!(live.data_hash(), base, "the data is put back");
    flipped
}

#[test]
fn two_loads_of_one_install_hash_the_same() {
    let a = synthetic("a");
    let b = synthetic("b");
    let h = a.data_hash();
    assert_ne!(h, [0; 32]);
    assert_eq!(a.data_hash(), h, "hashing twice");
    assert_eq!(b.data_hash(), h, "another build and load of the install");
}

#[test]
fn one_changed_byte_of_a_table_or_a_level_file_changes_it() {
    let mut live = synthetic("flip");
    let base = live.data_hash();
    assert!(every_table_flip_changes_it(&mut live, 0) > 50);
    every_table_flip_changes_it(&mut live, 1_000_003);

    let mut seen = vec![base];
    let mut changed = |live: &mut LiveData, what: &str| {
        let h = live.data_hash();
        assert!(!seen.contains(&h), "{what}");
        seen.push(h);
    };
    live.tables.bins.hitclass.records[0] ^= 1;
    changed(&mut live, "hitclass");
    let code = live
        .tables
        .bins
        .code
        .values_mut()
        .next()
        .expect("a code buffer");
    code.bytes[0] ^= 1;
    changed(&mut live, "a code buffer");
    let cell = live
        .files
        .ds1
        .0
        .values_mut()
        .flat_map(|d| d.walls.iter_mut().flatten())
        .next();
    *cell.expect("a preset DS1 wall cell") ^= 1;
    changed(&mut live, "a preset DS1 cell");
    let sub = live.files.subs.0.values_mut().next().expect("a lvlsub DS1");
    sub.method ^= 3;
    changed(&mut live, "a lvlsub DS1");
    let tile = live.files.dt1.0.values_mut().flatten().next();
    tile.expect("a DT1 tile").subtile_flags[0] ^= 1;
    changed(&mut live, "a DT1 tile");
    let anim = live.tables.anim.buckets.iter_mut().flatten().next();
    anim.expect("an AnimData record").frames ^= 1;
    changed(&mut live, "an AnimData record");
}

#[test]
#[ignore = "needs the game files (D2_GAME_DIR)"]
fn the_games_own_data_hashes_the_same_on_every_load() {
    let dir = std::env::var("D2_GAME_DIR").expect("D2_GAME_DIR must be set");
    let load = || {
        let set = ArchiveSet::open_dir(&dir).expect("archives open");
        let t = Instant::now();
        let live = LiveData::load(Arc::new(GameFiles::archives(Arc::new(set))))
            .unwrap_or_else(|e| panic!("{e}"));
        (live, t.elapsed())
    };
    let (mut a, load_a) = load();
    let (b, _) = load();
    let t = Instant::now();
    let h = a.data_hash();
    let hash_time = t.elapsed();
    assert_eq!(b.data_hash(), h);

    let table_bytes: usize = a.tables.bins.tables.iter().map(|t| t.records.len()).sum();
    let ds1_cells: usize = a
        .files
        .ds1
        .0
        .values()
        .flat_map(|d| d.walls.iter().chain(&d.floors).chain(&d.orientations))
        .map(Vec::len)
        .sum();
    eprintln!(
        "game data hash {}: {} tables ({table_bytes} record bytes), {} preset DS1 \
         ({ds1_cells} layer cells), {} lvlsub DS1, {} DT1; LiveData::load {:.1} ms, \
         data_hash {:.2} ms",
        hex(h),
        a.tables.bins.tables.len(),
        a.files.ds1.0.len(),
        a.files.subs.0.len(),
        a.files.dt1.0.len(),
        load_a.as_secs_f64() * 1e3,
        hash_time.as_secs_f64() * 1e3,
    );
    assert!(every_table_flip_changes_it(&mut a, 7) > 60);
}
