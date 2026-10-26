use rusqlite::Connection;

use super::{adopt, realm, realm_with, NOW};
use crate::{Block, Config, Error, Registry};

#[test]
fn blocks_rise_and_never_overlap() {
    let (conn, reg) = realm();
    let a = reg.grant(&conn, 1, NOW).unwrap();
    let b = reg.grant(&conn, 2, NOW).unwrap();
    let c = reg.grant(&conn, 1, NOW).unwrap();
    assert_eq!(
        a,
        Block {
            first: 1,
            count: 4096
        }
    );
    assert_eq!(
        b,
        Block {
            first: 4097,
            count: 4096
        }
    );
    assert_eq!(c.first, 8193);
    assert_eq!(reg.blocks(&conn, 1).unwrap(), vec![a, c]);
    assert_eq!(reg.blocks(&conn, 2).unwrap(), vec![b]);
}

#[test]
fn blocks_skip_registered_ids() {
    let (conn, reg) = realm();
    // Old items seen first, with IDs at 10, 11 and 12.
    adopt(&reg, &conn, "old", &[10, 11, 12]);
    let a = reg.grant(&conn, 1, NOW).unwrap();
    let b = reg.grant(&conn, 1, NOW).unwrap();
    assert_eq!(a, Block { first: 1, count: 9 });
    assert_eq!(
        b,
        Block {
            first: 13,
            count: 4096
        }
    );
}

#[test]
fn closed_game_blocks_are_not_reused() {
    let (conn, reg) = realm();
    let a = reg.grant(&conn, 1, NOW).unwrap();
    reg.close_game(&conn, 1, NOW).unwrap();
    assert!(reg.blocks(&conn, 1).unwrap().is_empty());
    // The same game number again (after a restart) gets fresh IDs.
    let b = reg.grant(&conn, 1, NOW).unwrap();
    assert_eq!(b.first, a.first + a.count);
}

#[test]
fn the_counter_ends_at_the_last_id() {
    let (conn, reg) = realm_with(Config {
        first_id: u32::MAX - 9,
        ..Config::default()
    });
    let a = reg.grant(&conn, 1, NOW).unwrap();
    assert_eq!(
        a,
        Block {
            first: u32::MAX - 9,
            count: 10
        }
    );
    assert!(a.contains(u32::MAX));
    assert_eq!(reg.grant(&conn, 1, NOW), Err(Error::Exhausted));
}

#[test]
fn a_database_serves_one_mod() {
    let conn = Connection::open_in_memory().unwrap();
    Registry::open(&conn, "d2rust", Config::default()).unwrap();
    Registry::open(&conn, "d2rust", Config::default()).unwrap();
    assert_eq!(
        Registry::open(&conn, "pd2", Config::default()).unwrap_err(),
        Error::OtherMod {
            ours: "pd2".into(),
            theirs: "d2rust".into()
        }
    );
}

#[test]
fn the_counter_survives_reopening() {
    let conn = Connection::open_in_memory().unwrap();
    let reg = Registry::open(&conn, "d2rust", Config::default()).unwrap();
    reg.grant(&conn, 1, NOW).unwrap();
    let again = Registry::open(&conn, "d2rust", Config::default()).unwrap();
    assert_eq!(again.grant(&conn, 2, NOW).unwrap().first, 4097);
}
