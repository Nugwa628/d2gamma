//! Two games returning at once: two connections to one realm database
//! file, each game's calls in its own immediate transaction.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;

use crate::{open_file, write, Block, Config, Registry, SaveItems, Split, UnknownIds};

/// A fresh database file for this test.
fn db_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("d2-item-registry-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir.join("realm.sqlite")
}

fn registry(path: &Path, config: Config) -> (rusqlite::Connection, Registry) {
    let conn = open_file(path).expect("open");
    let reg = Registry::open(&conn, "d2rust", config).expect("registry");
    (conn, reg)
}

#[test]
fn two_games_granting_at_once_get_disjoint_blocks() {
    let path = db_file("grants");
    registry(&path, Config::default());
    let start = Arc::new(Barrier::new(2));
    let games: Vec<_> = [1u32, 2]
        .into_iter()
        .map(|game| {
            let (path, start) = (path.clone(), start.clone());
            thread::spawn(move || {
                let (mut conn, reg) = registry(&path, Config::default());
                start.wait();
                (0..50)
                    .map(|_| write(&mut conn, |tx| reg.grant(tx, game, 0)).expect("grant"))
                    .collect::<Vec<Block>>()
            })
        })
        .collect();
    let mut all: Vec<Block> = games
        .into_iter()
        .flat_map(|g| g.join().expect("thread"))
        .collect();
    all.sort_by_key(|b| b.first);
    assert_eq!(all.len(), 100);
    for w in all.windows(2) {
        assert_eq!(w[0].first + w[0].count, w[1].first, "{:?}", w);
    }
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn two_games_returning_the_same_id_at_once_one_wins() {
    let path = db_file("race");
    let config = Config {
        unknown: UnknownIds::Adopt,
        ..Config::default()
    };
    {
        let (mut conn, reg) = registry(&path, config);
        write(&mut conn, |tx| {
            reg.grant(tx, 1, 0)?;
            reg.grant(tx, 2, 0)
        })
        .unwrap();
    }
    // A copied item comes back from two game servers at the same moment,
    // 40 times over.
    let start = Arc::new(Barrier::new(2));
    let games: Vec<_> = [(1u32, "amy"), (2, "bob")]
        .into_iter()
        .map(|(game, character)| {
            let (path, start) = (path.clone(), start.clone());
            thread::spawn(move || {
                let (mut conn, reg) = registry(&path, config);
                start.wait();
                (0..40u32)
                    .map(|i| {
                        let ids = [0x8000_0000 + i];
                        let s = SaveItems {
                            game,
                            character,
                            split: Split::DEFAULT,
                            ids: &ids,
                            now: 0,
                        };
                        write(&mut conn, |tx| reg.accept_save(tx, &s))
                            .expect("save")
                            .accepted()
                    })
                    .collect::<Vec<bool>>()
            })
        })
        .collect();
    let won: Vec<Vec<bool>> = games.into_iter().map(|g| g.join().unwrap()).collect();
    assert_eq!(won[0].len(), 40);
    for (i, (amy, bob)) in won[0].iter().zip(&won[1]).enumerate() {
        assert!(amy != bob, "item {i}: exactly one save may hold it");
    }
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
