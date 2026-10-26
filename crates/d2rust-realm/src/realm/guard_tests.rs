//! The save guard on synthetic tables (`d2-sim`'s item fixtures, the save
//! checks' ring and experience rows) and a realm database in a temp
//! folder: blocks, the registry's modes and the checks' modes.

use std::path::PathBuf;
use std::sync::Arc;

use d2_data::tables::{Charstats, Experience, Record};
use d2_formats::d2s::{self, status, Body, D2s, ItemEntry, StatEntry as CharStat, StatSave, Stats};
use d2_save_checks::{CheckTables, Mode};
use d2_sim::bench_fixtures::{item_rec, item_tables, push_item};
use d2_sim::combat::vitals::VitalsTables;
use d2_sim::items::bitstream::{write_save, Isc, StatEntry, StreamItem};
use d2_sim::items::tables::{AffixRec, ItemTables, PropRec, PropSlot, PropertyRec};
use d2_sim::items::{flag, q};
use d2_sim::world::hirelings::rows::HirelingRows;

use super::*;

/// The fixture ring's itemtypes row and the to-hit stat its suffix writes.
const RING: u16 = 10;
const TOHIT: u16 = 19;
/// The suffix "of accuracy": to-hit 5-10.
const SUFFIX_ACCURACY: u16 = 1;
/// The character stats' save widths (`itemstatcost` ids).
const CHAR_BITS: [(u16, u8); 6] = [(0, 10), (1, 10), (2, 10), (3, 10), (12, 7), (13, 32)];

fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2rust-realm-guard")
        .join(format!("{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn items() -> ItemTables {
    let mut t = item_tables();
    let mut isc = vec![Isc::default(); 360];
    isc[usize::from(TOHIT)].save_bits = 10;
    t.isc = isc;
    let mut p = PropertyRec::default();
    p.slots[0] = PropSlot {
        func: 1,
        stat: TOHIT,
        set: 0,
        val: 0,
    };
    t.properties = vec![p];
    let mut mods = [PropRec::NONE; 3];
    mods[0] = PropRec {
        code: 0,
        param: 0,
        min: 5,
        max: 10,
    };
    let unused = AffixRec {
        mods: [PropRec::NONE; 3],
        ..Default::default()
    };
    t.magic = vec![unused; 747];
    t.magic[usize::from(SUFFIX_ACCURACY) - 1] = AffixRec {
        spawnable: 1,
        level: 1,
        group: 1,
        rare: 1,
        classspecific: 0xFF,
        frequency: 1,
        itype: [RING as i16, 0, 0, 0, 0, 0, 0],
        mods,
        ..Default::default()
    };
    t.n_suffix = 747;
    t.n_prefix = 0;
    push_item(&mut t, item_rec(RING, b"rin "));
    t
}

fn vitals() -> VitalsTables {
    let mut cs = Charstats::decode(&[0u8; Charstats::SIZE]);
    (cs.str, cs.dex, cs.vit, cs.int, cs.statperlevel) = (20, 25, 20, 15, 5);
    let row = |v: u32| {
        let mut e = Experience::decode(&[0u8; Experience::SIZE]);
        (e.amazon, e.sorceress, e.necromancer, e.paladin) = (v, v, v, v);
        (e.barbarian, e.druid, e.assassin) = (v, v, v);
        e
    };
    // Row 0 is the level cap; row L + 1 the experience level L + 1 needs.
    let mut experience = vec![row(99)];
    experience.extend((0..=99u32).map(|l| row(1000 * l * l)));
    VitalsTables {
        charstats: vec![cs; 7],
        experience,
    }
}

fn tables() -> Arc<CheckTables> {
    let mut stat_saves = vec![StatSave::default(); 16];
    for (id, bits) in CHAR_BITS {
        stat_saves[usize::from(id)] = StatSave {
            bits,
            param: 0,
            signed: false,
        };
    }
    Arc::new(CheckTables::from_parts(
        items(),
        vitals(),
        stat_saves,
        HirelingRows::default(),
    ))
}

/// A level-2 expansion Amazon named `name` with the experience level 2
/// needs, wearing a ring per id of `rings`.
fn save(name: &str, level: u8, rings: &[u32]) -> Vec<u8> {
    let mut s = D2s::new_stub(name.as_bytes(), 0, status::EXPANSION, 0).expect("name fits");
    s.header.status &= !status::NEW;
    s.header.level = level;
    let stats = [
        (0, 20),
        (1, 15),
        (2, 25),
        (3, 20),
        (12, i32::from(level)),
        (13, 1000),
    ];
    let isc = items().isc;
    s.body = Some(Body {
        skills: vec![0; 30],
        stats: Stats::Bits(
            stats
                .iter()
                .map(|&(id, value)| CharStat {
                    id,
                    layer: 0,
                    value,
                })
                .collect(),
        ),
        items: rings
            .iter()
            .map(|&id| ItemEntry {
                bytes: write_save(&ring(id), &isc).expect("item fits").0,
            })
            .collect(),
        ..Body::default()
    });
    let t = tables();
    d2s::write(&s, &t.reading(true)).expect("writes")
}

/// A magic ring "of accuracy" with item ID `id`.
fn ring(id: u32) -> StreamItem {
    StreamItem {
        flags: flag::IDENTIFIED,
        version: 101,
        code: *b"rin ",
        unit28: id,
        ilvl: 30,
        quality: q::MAGIC,
        file_index: -1,
        suffix: [SUFFIX_ACCURACY, 0, 0],
        runeword: 0xFFFF,
        main: Some(vec![StatEntry {
            stat: TOHIT,
            param: 0,
            value: 7,
        }]),
        ..StreamItem::default()
    }
}

fn guard(dir: &std::path::Path, config: GuardConfig) -> SaveGuard {
    let log = Arc::new(ServerLog::open_named(&dir.join("logs"), "realm.log").unwrap());
    SaveGuard::open(
        &realm_db(&dir.join("data"), "d2rust"),
        "d2rust",
        Some(tables()),
        config,
        log,
    )
    .unwrap()
}

fn config(checks: ChecksMode, registry: RegistryMode) -> GuardConfig {
    GuardConfig {
        checks,
        check_modes: Vec::new(),
        registry,
    }
}

fn log_of(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("logs").join("realm.log")).unwrap_or_default()
}

#[test]
fn the_fixture_save_is_clean() {
    let dir = temp("clean");
    let g = guard(&dir, config(ChecksMode::Standard, RegistryMode::Refuse));
    let block = g.grant(1).unwrap();
    let j = g.judge(1, "Tester", None, &save("Tester", 2, &[block.first]));
    assert_eq!(j, Judgement::default());
}

#[test]
fn blocks_never_repeat_across_restarts() {
    let dir = temp("blocks");
    let g = guard(&dir, GuardConfig::default());
    let a = g.grant(1).unwrap();
    let b = g.grant(2).unwrap();
    assert_eq!(a.first, 1);
    assert_eq!(b.first, a.first + a.count);
    drop(g);
    let g = guard(&dir, GuardConfig::default());
    assert_eq!(g.grant(1).unwrap().first, b.first + b.count);
    // The former run's games 1 and 2 were closed at the start.
    assert!(log_of(&dir).contains("closed 2 games of a former run"));
}

/// The stash's check of a put item's ID: one of the game's blocks, or one
/// the registry placed, is known; one no block holds and no save showed
/// is a game server's invention.
#[test]
fn an_item_id_never_granted_is_not_known() {
    let dir = temp("knows");
    let g = guard(&dir, config(ChecksMode::Flag, RegistryMode::Flag));
    let block = g.grant(1).unwrap();
    assert_eq!(g.knows(1, block.first), Ok(true));
    // The ID a hostile test build makes up.
    let invented = 0x7FFF_FFF0;
    assert_eq!(g.knows(1, invented), Ok(false));
    // Another game's block isn't this game's.
    assert_eq!(g.knows(2, block.first), Ok(false));
    // Once a save showed it (adopted in flag mode), it has a place.
    let j = g.judge(1, "Tester", None, &save("Tester", 2, &[0x7000_0000]));
    assert_eq!(j.refused, None);
    assert_eq!(g.knows(1, 0x7000_0000), Ok(true));
    drop(g);
    // A registry that is off judges nothing.
    let dir = temp("knows-off");
    let g = guard(&dir, config(ChecksMode::Off, RegistryMode::Off));
    assert_eq!(g.knows(1, invented), Ok(true));
}

#[test]
fn an_id_not_granted_is_adopted_in_flag_mode_and_refused_in_refuse_mode() {
    let dir = temp("granted");
    let g = guard(&dir, config(ChecksMode::Flag, RegistryMode::Flag));
    g.grant(1).unwrap();
    let j = g.judge(1, "Tester", None, &save("Tester", 2, &[0x7000_0000]));
    assert_eq!(j.refused, None);
    assert!(j.flagged.iter().any(|f| f.contains("adopted")), "{j:?}");
    drop(g);
    let dir = temp("granted-refuse");
    let g = guard(&dir, config(ChecksMode::Flag, RegistryMode::Refuse));
    g.grant(1).unwrap();
    let j = g.judge(1, "Tester", None, &save("Tester", 2, &[0x7000_0000]));
    assert!(j.refused.unwrap().contains("not granted"));
}

#[test]
fn an_id_another_character_holds_is_flagged_or_refused() {
    for (mode, refused) in [(RegistryMode::Flag, false), (RegistryMode::Refuse, true)] {
        let dir = temp(&format!("held-{mode:?}"));
        let g = guard(&dir, config(ChecksMode::Off, mode));
        let id = g.grant(1).unwrap().first;
        assert_eq!(
            g.judge(1, "Tester", None, &save("Tester", 2, &[id])),
            Judgement::default()
        );
        g.left(1, "Tester");
        g.close_game(1);
        g.grant(2).unwrap();
        let j = g.judge(2, "Other", None, &save("Other", 2, &[id]));
        assert_eq!(j.refused.is_some(), refused, "{mode:?}: {j:?}");
        let why = j.refused.unwrap_or_else(|| j.flagged.join("; "));
        assert!(why.contains("held by character tester"), "{why}");
    }
}

#[test]
fn a_stored_record_is_registered_on_first_sight() {
    let dir = temp("first-sight");
    let g = guard(&dir, config(ChecksMode::Off, RegistryMode::Refuse));
    g.grant(1).unwrap();
    // A record from before the registry: its ring's ID was never granted,
    // and stays the character's.
    let old = save("Tester", 2, &[0x6000_0000]);
    assert_eq!(g.judge(1, "Tester", Some(&old), &old), Judgement::default());
}

#[test]
fn table_rules_refuse_in_standard_mode_and_flag_in_flag_mode() {
    // Level 9 with the experience of level 2.
    let bad = save("Tester", 9, &[]);
    let dir = temp("standard");
    let g = guard(&dir, config(ChecksMode::Standard, RegistryMode::Off));
    let j = g.judge(1, "Tester", None, &bad);
    assert!(j.refused.unwrap().contains("level-experience"));
    let dir = temp("flag");
    let g = guard(&dir, config(ChecksMode::Flag, RegistryMode::Off));
    let j = g.judge(1, "Tester", None, &bad);
    assert_eq!(j.refused, None);
    assert!(
        j.flagged.iter().any(|f| f.contains("level-experience")),
        "{j:?}"
    );
    // A check line sets one check back to refuse.
    let dir = temp("one-check");
    let mut c = config(ChecksMode::Flag, RegistryMode::Off);
    c.check_modes
        .push(("level-experience".into(), Mode::Refuse));
    let g = guard(&dir, c);
    assert!(g.judge(1, "Tester", None, &bad).refused.is_some());
}

/// A tiny mod game's save format: its own magic and the character's name,
/// no `.d2s` anywhere.
fn tiny_record(name: &str, level: u8) -> Vec<u8> {
    [b"TINYSAVE".as_slice(), &[level], name.as_bytes()].concat()
}

/// The summary a game server reports with a save of `name`.
fn reported(name: &str, level: u8) -> d2_net::msg::account::CharSummary {
    d2_net::msg::account::CharSummary {
        name: name.into(),
        class: 3,
        level,
        status: status::EXPANSION,
        difficulties: 1,
        look: Vec::new(),
        last_played: 0,
        season: 0,
    }
}

/// A guarded save desk (standard checks, registry refusing) on a realm of
/// `records`, with account "Nugwa", its character "Kashya" and a live
/// lease of Kashya in game 1 on game server "eu-1".
fn guarded_desk(
    dir: &std::path::Path,
    records: d2_accounts::RecordFormat,
) -> (super::super::saves::SaveDesk, d2_link::Lease) {
    use super::super::saves::{LeaseGrant, LeaseTable, SaveDesk};
    use d2_accounts::{Accounts, AccountsConfig, HashCost, NewChar};
    let acc = Accounts::open_with(AccountsConfig {
        cost: HashCost::Test,
        fail_delay: std::time::Duration::ZERO,
        records,
        ..AccountsConfig::new(dir)
    })
    .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let a = rt.block_on(async {
        let a = acc
            .register(
                "Nugwa",
                "correct horse",
                std::net::IpAddr::from([10, 0, 0, 1]),
            )
            .await
            .unwrap()
            .id;
        acc.create_character(
            a,
            NewChar {
                name: "Kashya".into(),
                class: 3,
                hardcore: false,
                expansion: true,
                progression: 0,
            },
        )
        .await
        .unwrap();
        a
    });
    let g = guard(dir, config(ChecksMode::Standard, RegistryMode::Refuse));
    g.grant(1).unwrap();
    let log = Arc::new(ServerLog::open_named(&dir.join("logs"), "realm.log").unwrap());
    let desk = SaveDesk::new(LeaseTable::new(1), acc, log).guarded(Arc::new(g));
    let lease = desk
        .leases()
        .grant(LeaseGrant {
            account: a,
            character: "Kashya".into(),
            game: 1,
            node: "eu-1".into(),
        })
        .unwrap();
    (desk, lease)
}

fn node_save(
    lease: d2_link::Lease,
    seq: u32,
    record: Vec<u8>,
    level: u8,
) -> super::super::saves::NodeSave {
    super::super::saves::NodeSave {
        info: d2_net::msg::node::SaveInfo {
            game: 1,
            lease: lease.0,
            seq,
            reason: 1,
            journal_tick: 0,
            summary: reported("Kashya", level),
        },
        record,
    }
}

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(f)
}

/// A realm of a mod game's own records (`records = game`) stores the mod's
/// save as it is, with the game server's summary; the `.d2s` checks and
/// the item registry never see it. A record of another character is still
/// refused.
#[test]
fn a_game_records_realm_stores_a_mod_save_with_the_game_servers_summary() {
    let dir = temp("game-records");
    let (desk, lease) = guarded_desk(&dir, d2_accounts::RecordFormat::Game);
    let record = tiny_record("Kashya", 42);
    let stored = block_on(desk.accept("eu-1", node_save(lease, 1, record.clone(), 42))).unwrap();
    assert_eq!(stored.summary.level, 42, "the game server's summary");
    assert_eq!(stored.summary.name, "Kashya");
    let seat = block_on(desk.accounts().seat(stored.account, "Kashya")).unwrap();
    assert_eq!(seat.record, Some(record), "stored as the game wrote it");
    assert_eq!(seat.summary.level, 42);

    let mut other = node_save(lease, 2, tiny_record("Akara", 50), 50);
    other.info.summary = reported("Akara", 50);
    let e = block_on(desk.accept("eu-1", other)).unwrap_err();
    assert_eq!(e.code, d2_net::msg::ErrorCode::SAVE_INVALID);
    assert!(!log_of(&dir).contains("check "), "no `.d2s` check ran");
}

/// D2Gamma's own realm (`records = d2s`, the default) keeps its guard: a
/// record that is no `.d2s` is refused whatever summary comes with it, and
/// a `.d2s` save still goes through the item registry.
#[test]
fn a_d2s_realm_refuses_a_mod_save_and_still_guards_its_own() {
    assert_eq!(
        d2_accounts::RecordFormat::default(),
        d2_accounts::RecordFormat::D2s
    );
    let dir = temp("d2s-records");
    let (desk, lease) = guarded_desk(&dir, d2_accounts::RecordFormat::D2s);
    let e = block_on(desk.accept("eu-1", node_save(lease, 1, tiny_record("Kashya", 42), 42)))
        .unwrap_err();
    assert_eq!(e.code, d2_net::msg::ErrorCode::SAVE_INVALID);
    // An item ID no block granted is refused by the registry.
    let e = block_on(desk.accept(
        "eu-1",
        node_save(lease, 2, save("Kashya", 2, &[0x7000_0000]), 2),
    ))
    .unwrap_err();
    assert_eq!(e.code, d2_net::msg::ErrorCode::SAVE_INVALID);
    assert!(e.text.contains("not granted"), "{}", e.text);
    // A clean `.d2s` save is stored with the summary read from its record,
    // not the game server's.
    let mut clean = node_save(lease, 3, save("Kashya", 2, &[]), 2);
    clean.info.summary.level = 90;
    let stored = block_on(desk.accept("eu-1", clean)).unwrap();
    assert_eq!(stored.summary.level, 2);
}
