//! Tests of the economy on real SQLite databases: the stash's moves, their
//! answers to repeated requests, crash injection between every write, and
//! concurrent connections on one file.

mod concurrent;
mod crash;
mod facets;
mod market;
mod offers;
mod search;
mod shop;
mod stash;

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::*;

pub(crate) const MOD: &str = "d2rust";

/// The test realm's settings: four stackables, and the market on.
pub(crate) fn config() -> EconomyConfig {
    EconomyConfig {
        market: true,
        stackables: vec![
            Stackable::new("r01", "El Rune"),
            Stackable::new("r30", "Ber Rune"),
            Stackable::new("r31", "Jah Rune"),
            Stackable::new("gpw", "Perfect Diamond"),
        ],
        ..EconomyConfig::default()
    }
}

pub(crate) fn econ() -> Economy {
    Economy::in_memory(MOD, config()).unwrap()
}

pub(crate) fn temp_db(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2-economy-tests")
        .join(format!("{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("realm.sqlite")
}

pub(crate) fn open(path: &std::path::Path) -> Economy {
    Economy::open(path, MOD, config()).unwrap()
}

pub(crate) fn owner(account: i64) -> Owner {
    Owner::new(account, EconomyKey::new(MOD))
}

pub(crate) fn chr(name: &str, lease: u64) -> CharRef {
    CharRef {
        name: name.into(),
        game: 7,
        lease,
    }
}

pub(crate) fn req(account: i64, n: u64) -> Req {
    Req::account(AccountId(account), n)
}

pub(crate) fn save(seq: u32) -> Save {
    Save {
        record: vec![0x55, 0xAA, seq as u8],
        seq,
    }
}

/// A rare ring with a few stats; `id` picks the name and values.
pub(crate) fn item(id: u32) -> Item {
    Item {
        id: ItemId(id),
        record: vec![b'J', b'M', id as u8, (id >> 8) as u8],
        facts: ItemFacts {
            code: "rin".into(),
            item_type: "ring".into(),
            quality: 6,
            name: format!("Havoc Loop {id}"),
            name_parts: vec!["Ring".into(), "Havoc".into(), "Loop".into()],
            level_req: (id % 90) as u8,
            ilvl: 80,
            sockets: 0,
            ethereal: false,
            class: None,
            stats: vec![(39, (id % 30) as i32), (105, 10)],
            ..ItemFacts::default()
        },
    }
}

pub(crate) fn put_item(e: &mut Economy, account: i64, n: u64, it: Item) -> Result<Done, Error> {
    e.put(
        &owner(account),
        &req(account, n),
        &chr("Hero", 1),
        PutGoods::Item(it),
        &save(1),
        1000,
    )
}

pub(crate) fn put_gold(e: &mut Economy, account: i64, n: u64, gold: u64) -> Result<Done, Error> {
    e.put(
        &owner(account),
        &req(account, n),
        &chr("Hero", 1),
        PutGoods::Gold(gold),
        &save(1),
        1000,
    )
}

pub(crate) fn put_stack(
    e: &mut Economy,
    account: i64,
    n: u64,
    code: &str,
    count: u32,
) -> Result<Done, Error> {
    e.put(
        &owner(account),
        &req(account, n),
        &chr("Hero", 1),
        PutGoods::Stack {
            code: code.into(),
            count,
        },
        &save(1),
        1000,
    )
}

/// Every table, every row, in a fixed order: two equal snapshots are two
/// equal databases.
pub(crate) fn snapshot(e: &Economy) -> String {
    snapshot_except(e, "")
}

/// [`snapshot`] without one table.
pub(crate) fn snapshot_except(e: &Economy, skip: &str) -> String {
    let conn = e.conn();
    let tables: Vec<String> = {
        let mut st = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        let rows = st.query_map([], |r| r.get(0)).unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    };
    let mut out = String::new();
    for t in tables.into_iter().filter(|t| t != skip) {
        let mut st = conn.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
        let n = st.column_count();
        let mut rows: Vec<String> = st
            .query_map([], |r| {
                let mut s = String::new();
                for i in 0..n {
                    let v: rusqlite::types::Value = r.get(i)?;
                    s.push_str(&format!("{v:?}|"));
                }
                Ok(s)
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        rows.sort();
        out.push_str(&format!("[{t}]\n{}\n", rows.join("\n")));
    }
    out
}

/// What the economy holds in total, wherever it is (stashes, escrow,
/// deliveries, transfers in flight): item IDs, gold and stack counts.
/// Moves inside the economy never change it; fees take gold out.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Totals {
    pub items: Vec<u32>,
    pub gold: i64,
    pub stacks: BTreeMap<String, i64>,
}

pub(crate) fn totals(e: &Economy) -> Totals {
    let conn = e.conn();
    let items = {
        let mut st = conn
            .prepare("SELECT item_id FROM econ_items ORDER BY item_id")
            .unwrap();
        let rows = st.query_map([], |r| r.get(0)).unwrap();
        rows.collect::<Result<_, _>>().unwrap()
    };
    let sum = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    let gold = sum("SELECT COALESCE(SUM(gold), 0) FROM stash_gold")
        + sum("SELECT COALESCE(SUM(gold), 0) FROM in_flight")
        + sum("SELECT COALESCE(SUM(gold), 0) FROM escrow")
        + sum("SELECT COALESCE(SUM(gold), 0) FROM deliveries");
    let mut stacks = BTreeMap::new();
    for sql in [
        "SELECT code, SUM(count) FROM stash_stacks GROUP BY code",
        "SELECT code, SUM(count) FROM in_flight WHERE code IS NOT NULL GROUP BY code",
        "SELECT code, SUM(count) FROM escrow_stacks GROUP BY code",
        "SELECT code, SUM(count) FROM deliveries WHERE code IS NOT NULL GROUP BY code",
    ] {
        let mut st = conn.prepare(sql).unwrap();
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap();
        for row in rows {
            let (code, n) = row.unwrap();
            *stacks.entry(code).or_insert(0) += n;
        }
    }
    Totals {
        items,
        gold,
        stacks,
    }
}
