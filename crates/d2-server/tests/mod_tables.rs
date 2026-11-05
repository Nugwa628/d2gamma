//! A mod game's own compiled tables load through the whole game path
//! under its data rules (`GameData::open_with`, `DataRules::Mod`), on the
//! synthetic playable install with a mod archive layered on top
//! (`GameSource::LayeredArchives`) that has each trait a mod game's tables
//! may have:
//!
//! - `runes.bin` records wider than D2Gamma's (six more property slots);
//! - an `automap.bin` whose `LevelName` cells hold level type numbers,
//!   one of them (51) past D2Gamma's names;
//! - an `experience.bin` of 122 rows (level 120);
//!
//! and a saved character past level 99 joins a game on it. D2Gamma's own
//! data rules refuse the same files.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use d2_data::bin::{BinTable, DataRules, AUTOMAP_LEVEL_NAMES};
use d2_formats::d2s::{self, Body, D2s, Golem, Header, StatEntry, Stats};
use d2_formats::mpq::writer::{FileOptions, MpqWriter};
use d2_server::adapters::character::LoadContext;
use d2_server::adapters::ProtoSizes;
use d2_server::game::character_save::read_live;
use d2_server::game::client_tables::client_resist_penalties;
use d2_server::game::{
    create_request_for, Character, GameBuilder, GameData, GameSource, LiveData, Sim,
};
use d2_server::host::Host;
use d2_server::seams::{ClientId, Clock, MessageSink, SessionHandler};
use d2_sim::rules::{ModRules, Ruleset};
use test_fixtures::{install, play};

/// Bytes the mod's `runes` record has after D2Gamma's 288.
const RUNES_EXTRA: usize = 96;
/// The mod's highest level.
const MAX_LEVEL: u32 = 120;
/// The level of the saved character.
const SAVED_LEVEL: u32 = 110;

/// The experience to reach level `l + 1` past the install's table: 1,000
/// more per level than row 100.
fn past(row_100: u32, row: usize) -> u32 {
    row_100 + 1000 * (row as u32 - 100)
}

/// `.bin` bytes of `t` with records `records`.
fn bin_bytes(records: &[Vec<u8>]) -> Vec<u8> {
    let mut b = (records.len() as u32).to_le_bytes().to_vec();
    for r in records {
        b.extend_from_slice(r);
    }
    b
}

fn rows(t: &BinTable) -> Vec<Vec<u8>> {
    t.iter().map(<[u8]>::to_vec).collect()
}

/// The mod archive over the install `i`: its `runes`, `automap` and
/// `experience` tables in the mod game's own forms.
fn write_mod_archive(i: &install::Install, path: &Path) {
    let table = |n: &str| i.loaded.table(n).unwrap_or_else(|| panic!("{n}"));
    let mut w = MpqWriter::new().with_listfile();
    let excel = |n: &str| format!("data\\global\\excel\\{n}");

    // runes: D2Gamma's columns, then the mod's own (record i: byte i).
    let runes = table("runes");
    assert!(runes.count > 0, "the install has runes rows");
    let wide: Vec<Vec<u8>> = runes
        .iter()
        .enumerate()
        .map(|(n, r)| {
            let mut r = r.to_vec();
            r.extend(std::iter::repeat_n(n as u8 ^ 0xA5, RUNES_EXTRA));
            r
        })
        .collect();
    w.add(
        &excel("runes.bin"),
        bin_bytes(&wide),
        FileOptions::default(),
    );

    // automap: the level type number in place of each name, and one more
    // row at level type 51.
    let mut automap = rows(table("automap"));
    for r in &mut automap {
        let end = r[..16].iter().position(|&b| b == 0).unwrap_or(16);
        let n = AUTOMAP_LEVEL_NAMES
            .iter()
            .position(|name| name.as_bytes() == &r[..end])
            .unwrap_or(0) as u32;
        r[..16].fill(0);
        r[..4].copy_from_slice(&n.to_le_bytes());
    }
    let mut extra = automap.last().expect("automap rows").clone();
    extra[..4].copy_from_slice(&51u32.to_le_bytes());
    automap.push(extra);
    w.add(
        &excel("automap.bin"),
        bin_bytes(&automap),
        FileOptions::default(),
    );

    // experience: row 0 (the max level) at 120 for every class, and a
    // row per level up to 120 (row L + 1 holds level L).
    let mut exp = rows(table("experience"));
    assert_eq!(exp.len(), 101);
    let row_100 = |c: usize| u32::from_le_bytes(exp[100][4 * c..4 * c + 4].try_into().unwrap());
    let tops: Vec<u32> = (0..7).map(row_100).collect();
    for c in 0..7 {
        exp[0][4 * c..4 * c + 4].copy_from_slice(&MAX_LEVEL.to_le_bytes());
    }
    for row in 101..=MAX_LEVEL as usize + 1 {
        let mut r = exp[100].clone();
        for (c, &top) in tops.iter().enumerate() {
            r[4 * c..4 * c + 4].copy_from_slice(&past(top, row).to_le_bytes());
        }
        exp.push(r);
    }
    w.add(
        &excel("experience.bin"),
        bin_bytes(&exp),
        FileOptions::default(),
    );
    w.write(path).expect("the mod archive is written");
}

/// The playable install and the mod archive over it, built once.
fn source() -> &'static (GameSource, install::Install) {
    static S: OnceLock<(GameSource, install::Install)> = OnceLock::new();
    S.get_or_init(|| {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("mod-tables-{}", std::process::id()));
        let i = install::build(&dir.join("install"), &play::playable())
            .unwrap_or_else(|e| panic!("{e}"));
        let archive = dir.join("mod-tables.mpq");
        write_mod_archive(&i, &archive);
        (
            GameSource::LayeredArchives(dir.join("install"), vec![archive]),
            i,
        )
    })
}

fn mod_data() -> &'static GameData {
    static D: OnceLock<GameData> = OnceLock::new();
    D.get_or_init(|| {
        GameData::open_with(&source().0, DataRules::Mod).unwrap_or_else(|e| panic!("{e}"))
    })
}

fn live() -> &'static LiveData {
    let GameData::Live(d) = mod_data();
    d
}

#[test]
fn d2gammas_own_data_rules_refuse_the_mod_tables() {
    let e = GameData::open_with(&source().0, DataRules::Original).unwrap_err();
    assert!(e.to_string().contains("runes"), "{e}");
    // `open` is D2Gamma's rules.
    assert!(GameData::open(&source().0).is_err());
}

#[test]
fn the_mod_tables_load_under_the_mod_data_rules() {
    let (_, i) = source();
    let bins = &live().tables.bins;
    assert_eq!(bins.rules, DataRules::Mod);
    // runes: every reader sees D2Gamma's columns; the full records are
    // the mod's.
    let runes = bins.table("runes").unwrap();
    let own = i.loaded.table("runes").unwrap();
    assert_eq!(
        (runes.count, runes.record_size, &runes.records),
        (own.count, own.record_size, &own.records)
    );
    let wide = &bins.wide["runes"];
    assert_eq!(
        (wide.count, wide.record_size),
        (runes.count, runes.record_size + RUNES_EXTRA)
    );
    assert_eq!(&wide.record(0)[..runes.record_size], runes.record(0));
    assert!(wide.record(0)[runes.record_size..]
        .iter()
        .all(|&b| b == 0xA5));
    assert_eq!(bins.wide.len(), 1);
    // automap: read as level type numbers, 51 included.
    let automap = &live().tables.fixed.automap;
    let last = automap.records.last().unwrap();
    assert_eq!(i32::from_le_bytes(last[..4].try_into().unwrap()), 51);
    assert_eq!(automap.ranges.len(), 52);
    let town = i.loaded.table("automap").unwrap().record(0);
    let first = automap.records[0];
    let name_end = town[..16].iter().position(|&b| b == 0).unwrap();
    let named = AUTOMAP_LEVEL_NAMES
        .iter()
        .position(|n| n.as_bytes() == &town[..name_end])
        .unwrap();
    assert_eq!(
        i32::from_le_bytes(first[..4].try_into().unwrap()),
        named as i32
    );
    // experience: 122 rows.
    assert_eq!(bins.table("experience").unwrap().count, 122);
    // The other readers of the same files load under the same rules (a
    // client table reader).
    assert!(client_resist_penalties(live().archives.as_ref()).is_ok());
}

/// A mod game with no rules of its own beyond its name.
#[derive(Debug)]
struct Plain;

impl ModRules for Plain {
    fn name(&self) -> &str {
        "mod tables"
    }
}

/// An expansion amazon saved at level 110, with the experience of it.
fn saved_past_99(exp: u32) -> Character {
    let mut header = Header::default();
    header.set_name(b"Past").unwrap();
    header.class = 0;
    header.level = SAVED_LEVEL as u8;
    header.status = d2s::status::EXPANSION;
    header.towns = [0x80, 0, 0];
    let stats = [
        (0, 100),
        (1, 100),
        (2, 100),
        (3, 100),
        (6, 500 << 8),
        (7, 500 << 8),
        (8, 200 << 8),
        (9, 200 << 8),
        (10, 300 << 8),
        (11, 300 << 8),
        (12, SAVED_LEVEL as i32),
        (13, exp as i32),
    ]
    .map(|(id, value)| StatEntry {
        id,
        layer: 0,
        value,
    })
    .to_vec();
    Character::Save(
        Box::new(D2s {
            header,
            body: Some(Body {
                stats: Stats::Bits(stats),
                skills: vec![0; 30],
                hireling_items: Some(None),
                golem: Some(Golem {
                    flag: 0,
                    item: None,
                }),
                ..Body::default()
            }),
        }),
        LoadContext {
            difficulty: 0,
            map_seed_applies: false,
        },
    )
}

struct NoSession;

impl SessionHandler for NoSession {
    fn system_message(&mut self, _: ClientId, _: &[u8], _: usize, _: &mut dyn MessageSink) {}
}

#[derive(Default)]
struct Steps(u32);

impl Clock for Steps {
    fn now_ms(&mut self) -> u32 {
        self.0
    }
}

#[test]
fn a_character_saved_past_level_99_joins_a_game_on_the_mod_tables() {
    let exp_row = |row: usize| {
        let t = live().tables.bins.table("experience").unwrap();
        u32::from_le_bytes(t.record(row)[..4].try_into().unwrap())
    };
    let character = saved_past_99(exp_row(SAVED_LEVEL as usize));
    let sim: Sim = GameBuilder::new(mod_data().clone())
        .rules(Ruleset::Mod(std::sync::Arc::new(Plain)))
        .character(character.clone())
        .build()
        .expect("the game builds");
    let mut host = Host::new(sim, ProtoSizes, NoSession, Steps(1000));
    host.connect(0);
    let req = create_request_for(&character, 800, 600);
    host.send_system(0, &req.encode()).expect("queued");
    let mut asked = false;
    let mut joined = false;
    for _ in 0..200 {
        host.clock.0 += 40;
        host.frame().expect("frame");
        let got = host.receive(0);
        if !asked && got.iter().any(|m| m.first() == Some(&0x02)) {
            host.send_system(0, &[0x6B]).expect("queued");
            asked = true;
        }
        if asked && host.game.player_of(0).is_some() {
            joined = true;
            break;
        }
    }
    assert!(joined, "the player joined");
    let p = host.game.player_of(0).unwrap();
    let live_stats = read_live(&mut host.game, p);
    let stat = |id: u16| {
        live_stats
            .stats
            .iter()
            .find(|&&(s, _)| s == id)
            .map(|&(_, v)| v)
    };
    assert_eq!(stat(12), Some(SAVED_LEVEL as i32));
    assert_eq!(stat(13), Some(exp_row(SAVED_LEVEL as usize) as i32));
    // The load's next-level step ran on the game's experience table, which
    // is the mod's: level 110's threshold is its row 111. (The synthetic
    // stat table has no `nextexp` row to hold the value.)
    let hooks = host.game.events.action.hooks();
    assert!(
        !hooks.x.log.iter().any(|l| l.contains("nextexp")),
        "{:?}",
        hooks.x.log
    );
    let vitals = hooks.vitals.clone().expect("the game's vitals tables");
    assert_eq!(vitals.max_level(0), MAX_LEVEL);
    assert_eq!(
        vitals.threshold(0, SAVED_LEVEL),
        exp_row(SAVED_LEVEL as usize + 1)
    );
    assert_eq!(
        vitals.level_from_exp(0, exp_row(SAVED_LEVEL as usize)),
        SAVED_LEVEL
    );
}
