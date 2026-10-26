//! Tests of the item registry, on in-memory databases (and one shared
//! file for two games at once).

mod blocks;
mod concurrency;
mod economy;
mod ids;
mod saves;

use rusqlite::Connection;

use crate::{Config, Registry, SaveItems, SaveVerdict, Split, UnknownIds};

/// A fresh in-memory realm database with the `d2rust` registry.
fn realm() -> (Connection, Registry) {
    realm_with(Config::default())
}

fn realm_with(config: Config) -> (Connection, Registry) {
    let conn = Connection::open_in_memory().expect("in-memory database");
    let reg = Registry::open(&conn, "d2rust", config).expect("registry");
    (conn, reg)
}

/// A registry that adopts unknown IDs.
fn adopting() -> (Connection, Registry) {
    realm_with(Config {
        unknown: UnknownIds::Adopt,
        ..Config::default()
    })
}

const NOW: i64 = 1_000;

fn save(reg: &Registry, conn: &Connection, game: u32, character: &str, ids: &[u32]) -> SaveVerdict {
    reg.accept_save(
        conn,
        &SaveItems {
            game,
            character,
            split: Split::DEFAULT,
            ids,
            now: NOW,
        },
    )
    .expect("registry call")
}

/// First sight of `character` with `ids`.
fn adopt(reg: &Registry, conn: &Connection, character: &str, ids: &[u32]) -> crate::Adopted {
    reg.adopt(
        conn,
        &SaveItems {
            game: 0,
            character,
            split: Split::DEFAULT,
            ids,
            now: NOW,
        },
    )
    .expect("registry call")
}

/// The (place, place_id) of `id`.
fn at(reg: &Registry, conn: &Connection, id: u32) -> Option<(crate::Place, String)> {
    reg.place_of(conn, id)
        .expect("registry call")
        .map(|w| (w.place, w.place_id))
}
