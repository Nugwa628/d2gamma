//! The ladder store (lobby contract sections 6 and 10) on a database in
//! memory.

use d2_formats::d2s::status;
use d2_net::msg::account::CharSummary;

use crate::db::Db;
use crate::ladder::*;

const GAME: &str = "d2gamma";
const T0: i64 = 1_800_000_000_000;

fn entry(name: &str, experience: u64, reached_ms: i64) -> LadderEntry {
    LadderEntry {
        season: 1,
        name: name.into(),
        account: 1,
        class: 3,
        level: 50,
        experience,
        split: Split {
            hardcore: false,
            expansion: true,
        },
        dead: false,
        reached_ms,
    }
}

fn names(rows: &[LadderEntry]) -> Vec<&str> {
    rows.iter().map(|e| e.name.as_str()).collect()
}

#[test]
fn ladder_order() {
    let db = Db::in_memory().unwrap();
    let l = db.ladder();
    let split = entry("x", 0, 0).split;
    // Experience first, then who reached it first, then the name in any
    // case.
    for e in [
        entry("Low", 10, T0),
        entry("Late", 500, T0 + 9),
        entry("bravo", 500, T0 + 1),
        entry("Alpha", 500, T0 + 1),
        entry("Top", 900, T0 + 50),
    ] {
        l.upsert(GAME, &e).unwrap();
    }
    assert_eq!(
        names(&l.split(GAME, 1, split).unwrap()),
        ["Top", "Alpha", "bravo", "Late", "Low"]
    );
}

#[test]
fn reached_stays_while_experience_is_unchanged() {
    let db = Db::in_memory().unwrap();
    let l = db.ladder();
    l.upsert(GAME, &entry("Kashya", 100, T0)).unwrap();
    // A later save with the same experience (a new level-up elsewhere is
    // impossible without it): the time stays, the rest follows.
    let mut e = entry("KASHYA", 100, T0 + 1000);
    e.level = 51;
    l.upsert(GAME, &e).unwrap();
    let got = l.entry(GAME, 1, "kashya").unwrap().unwrap();
    assert_eq!(
        (got.reached_ms, got.level, got.name.as_str()),
        (T0, 51, "KASHYA")
    );
    l.upsert(GAME, &entry("Kashya", 150, T0 + 2000)).unwrap();
    let got = l.entry(GAME, 1, "Kashya").unwrap().unwrap();
    assert_eq!((got.reached_ms, got.experience), (T0 + 2000, 150));
    assert_eq!(
        l.split(GAME, 1, got.split).unwrap().len(),
        1,
        "one row per character"
    );
}

#[test]
fn splits_seasons_and_games_apart() {
    let db = Db::in_memory().unwrap();
    let l = db.ladder();
    for (name, hc, exp) in [
        ("Sc", false, true),
        ("Hc", true, true),
        ("Classic", false, false),
        ("HcCl", true, false),
    ] {
        let mut e = entry(name, 1, T0);
        e.split = Split {
            hardcore: hc,
            expansion: exp,
        };
        l.upsert(GAME, &e).unwrap();
    }
    let mut other_season = entry("Old", 1, T0);
    other_season.season = 0;
    l.upsert(GAME, &other_season).unwrap();
    l.upsert("pd2", &entry("OtherMod", 1, T0)).unwrap();
    for (bits, want) in [(0, "Classic"), (1, "HcCl"), (2, "Sc"), (3, "Hc")] {
        assert_eq!(
            names(&l.split(GAME, 1, Split::from_bits(bits)).unwrap()),
            [want],
            "{bits}"
        );
        assert_eq!(Split::from_bits(bits).bits(), bits);
    }
    assert_eq!(
        names(&l.split(GAME, 0, Split::from_bits(2)).unwrap()),
        ["Old"]
    );
    assert_eq!(
        names(&l.split("pd2", 1, Split::from_bits(2)).unwrap()),
        ["OtherMod"]
    );
}

#[test]
fn dead_hardcore_stays_marked() {
    let db = Db::in_memory().unwrap();
    let l = db.ladder();
    let mut e = entry("Brave", 300, T0);
    e.split.hardcore = true;
    l.upsert(GAME, &e).unwrap();
    e.dead = true;
    e.reached_ms = T0 + 5;
    l.upsert(GAME, &e).unwrap();
    let rows = l.split(GAME, 1, e.split).unwrap();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].dead);
    assert_eq!(rows[0].reached_ms, T0, "dying adds no experience");
}

#[test]
fn deleted_leaves_every_season() {
    let db = Db::in_memory().unwrap();
    let l = db.ladder();
    l.upsert(GAME, &entry("Gone", 5, T0)).unwrap();
    let mut s2 = entry("gone", 9, T0);
    s2.season = 2;
    l.upsert(GAME, &s2).unwrap();
    l.upsert("pd2", &entry("Gone", 5, T0)).unwrap();
    l.upsert(GAME, &entry("Stays", 5, T0)).unwrap();
    assert_eq!(l.remove(GAME, "GONE").unwrap(), 2);
    assert_eq!(l.entry(GAME, 2, "Gone").unwrap(), None);
    assert_eq!(names(&l.split(GAME, 1, s2.split).unwrap()), ["Stays"]);
    assert!(
        l.entry("pd2", 1, "Gone").unwrap().is_some(),
        "another game's ladder"
    );
}

#[test]
fn seasons_start_and_end() {
    let db = Db::in_memory().unwrap();
    let l = db.ladder();
    assert_eq!(l.current(GAME).unwrap(), None);
    assert_eq!(l.end_season(GAME, T0).unwrap(), None);
    let s1 = l.start_season(GAME, "Season 1", false, T0).unwrap();
    assert_eq!((s1.id, s1.ended_ms, s1.test), (1, None, false));
    assert_eq!(l.current(GAME).unwrap(), Some(s1.clone()));
    // A new season ends the open one.
    let s2 = l.start_season(GAME, "Test season", true, T0 + 10).unwrap();
    assert_eq!((s2.id, s2.test), (2, true));
    let all = l.seasons(GAME).unwrap();
    assert_eq!(
        all.iter().map(|s| s.id).collect::<Vec<_>>(),
        [2, 1],
        "newest first"
    );
    assert_eq!(all[1].ended_ms, Some(T0 + 10));
    let ended = l.end_season(GAME, T0 + 20).unwrap().unwrap();
    assert_eq!((ended.id, ended.ended_ms), (2, Some(T0 + 20)));
    assert_eq!(l.current(GAME).unwrap(), None);
    assert_eq!(l.season(GAME, 2).unwrap().unwrap().ended_ms, Some(T0 + 20));
    assert_eq!(l.season(GAME, 3).unwrap(), None);
    // Each game numbers its own seasons.
    assert_eq!(l.start_season("pd2", "Season 1", false, T0).unwrap().id, 1);
    assert_eq!(
        l.start_season(GAME, "Season 3", false, T0 + 30).unwrap().id,
        3
    );
}

#[test]
fn a_characters_season() {
    let mut db = Db::in_memory().unwrap();
    let a = db.insert_account("A1", "a1", "h", 1).unwrap().unwrap();
    let s = CharSummary {
        name: "Kashya".into(),
        class: 1,
        level: 1,
        status: status::EXPANSION,
        difficulties: 1,
        look: vec![0xFF; 32],
        last_played: 0,
        season: 0,
    };
    db.insert_character(a, &s, 10, 0).unwrap();
    let l = db.ladder();
    assert_eq!(l.character_season("kashya").unwrap(), Some(NON_LADDER));
    assert!(l.set_character_season("KASHYA", 4).unwrap());
    assert_eq!(l.character_season("Kashya").unwrap(), Some(4));
    assert!(!l.set_character_season("nobody", 4).unwrap());
    assert_eq!(l.character_season("nobody").unwrap(), None);
}
