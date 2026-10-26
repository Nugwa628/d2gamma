//! Season end on a real accounts database in a temp folder: the seat
//! path's load hook and the ended season's final ranks.

use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use d2_accounts::{AccountsConfig, HashCost, NewChar};
use d2_formats::d2s::Header;

use super::*;
use crate::ladder::LadderConfig;
use crate::log::ServerLog;
use crate::realm::saves::{LeaseTable, SaveDesk, SeatLoaded};
use d2_accounts::Accounts;

const GAME: &str = "d2rust";

fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2rust-realm-season-end")
        .join(format!("{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap()
        .block_on(f)
}

/// A save record of `name` at `level` with status word `st`.
fn record(name: &str, level: u8, st: u16) -> Vec<u8> {
    let mut h = Header {
        class: 3,
        level,
        status: st,
        ..Header::default()
    };
    h.set_name(name.as_bytes()).unwrap();
    h.to_bytes().to_vec()
}

fn status_of(record: &[u8]) -> u16 {
    summary::of_record(record, 0).unwrap().status
}

struct Fixture {
    ladder: Arc<Ladder>,
    desk: SaveDesk,
    accounts: Accounts,
    account: AccountId,
}

/// A ladder that is on, account "Nugwa", and a save desk whose seats go
/// through the ladder's load hook.
async fn fixture(test: &str) -> Fixture {
    let dir = temp(test);
    let accounts = Accounts::open_with(AccountsConfig {
        cost: HashCost::Test,
        fail_delay: Duration::ZERO,
        max_characters: 32,
        ..AccountsConfig::new(&dir)
    })
    .unwrap();
    let account = accounts
        .register("Nugwa", "correct horse", IpAddr::from([10, 0, 0, 1]))
        .await
        .unwrap()
        .id;
    let log = Arc::new(ServerLog::open_named(&dir, "test.log").unwrap());
    let cfg = LadderConfig {
        on: true,
        refresh: Duration::ZERO,
    };
    let ladder = Ladder::new(accounts.clone(), GAME, cfg, None, log.clone());
    let desk = SaveDesk::new(LeaseTable::new(1), accounts.clone(), log)
        .on_seat_load(SeatLoaded(ladder.clone()));
    Fixture {
        ladder,
        desk,
        accounts,
        account,
    }
}

impl Fixture {
    /// A character of `season`, played once: its record (with the ladder
    /// bit for a ladder season) stored as a save would store it.
    async fn played(&self, name: &str, season: u16, level: u8) {
        let new = NewChar {
            name: name.into(),
            class: 3,
            hardcore: false,
            expansion: true,
            progression: 0,
        };
        self.accounts
            .create_character_in(self.account, new, season)
            .await
            .unwrap();
        let mut st = status::EXPANSION;
        if season != NON_LADDER {
            st |= status::LADDER;
        }
        self.accounts
            .store_record(self.account, name, record(name, level, st), 100)
            .await
            .unwrap();
    }

    /// The character as loaded for a game (through the load hook).
    async fn seat(&self, name: &str) -> CharSeat {
        self.desk.seat(self.account, name).await.unwrap()
    }

    async fn season_of(&self, name: &str) -> u16 {
        self.accounts
            .character(self.account, name)
            .await
            .unwrap()
            .season
    }

    async fn entry(&self, season: u16, name: &str) -> Option<d2_accounts::ladder::LadderEntry> {
        let name = name.to_owned();
        self.accounts
            .store(move |db| Ok(db.ladder().entry(GAME, season, &name)?))
            .await
            .unwrap()
    }
}

#[test]
fn an_ended_seasons_character_is_non_ladder_once_loaded() {
    run(async {
        let f = fixture("retire").await;
        f.ladder.season_start("Season 1", false).await.unwrap();
        f.played("Kashya", 1, 30).await;
        f.ladder.seed("Kashya", 30, 1_000_000, false).await.unwrap();
        let said = f.ladder.season_end().await.unwrap();
        assert!(said.contains("1 ranked characters"), "{said}");
        // Not loaded yet: still in the ended season.
        assert_eq!(f.season_of("Kashya").await, 1);

        let seat = f.seat("Kashya").await;
        assert_eq!(seat.summary.season, NON_LADDER);
        assert_eq!(seat.summary.status & status::LADDER, 0);
        // The game gets the record without the ladder bit, and the file
        // holds it too.
        let offered = seat.record.as_deref().unwrap();
        assert_eq!(status_of(offered), status::EXPANSION);
        let stored = f.accounts.seat(f.account, "Kashya").await.unwrap();
        assert_eq!(stored.record.as_deref(), Some(offered));
        assert_eq!(f.season_of("Kashya").await, NON_LADDER);
        let row = f.accounts.character(f.account, "Kashya").await.unwrap();
        assert_eq!(row.status & status::LADDER, 0);
        // The ended season keeps its row: its final rank.
        let kept = f.entry(1, "Kashya").await.expect("the season 1 row");
        assert_eq!((kept.level, kept.experience), (30, 1_000_000));

        // A second load changes nothing.
        let again = f.seat("Kashya").await;
        assert_eq!(again.record.as_deref(), Some(offered));
        assert_eq!(again.summary.season, NON_LADDER);
    });
}

#[test]
fn open_season_and_non_ladder_characters_load_unchanged() {
    run(async {
        let f = fixture("unchanged").await;
        f.ladder.season_start("Season 1", false).await.unwrap();
        f.played("Akara", 1, 10).await;
        f.played("Warriv", NON_LADDER, 12).await;
        let seat = f.seat("Akara").await;
        assert_eq!(seat.summary.season, 1);
        let offered = seat.record.as_deref().unwrap();
        assert_ne!(status_of(offered) & status::LADDER, 0);
        assert_eq!(f.season_of("Akara").await, 1);
        let seat = f.seat("Warriv").await;
        assert_eq!(seat.summary.season, NON_LADDER);

        // A character of the next season isn't touched by the last one's
        // end, nor is one that left an ended season by an open one.
        f.ladder.season_end().await.unwrap();
        f.ladder.season_start("Season 2", false).await.unwrap();
        f.played("Gheed", 2, 5).await;
        assert_eq!(f.seat("Akara").await.summary.season, 0);
        assert_eq!(f.seat("Gheed").await.summary.season, 2);
        assert_eq!(f.season_of("Akara").await, NON_LADDER);
    });
}

#[test]
fn a_never_played_character_of_an_ended_season_moves_without_a_file() {
    run(async {
        let f = fixture("unplayed").await;
        f.ladder.season_start("Season 1", true).await.unwrap();
        let new = NewChar {
            name: "Charsi".into(),
            class: 1,
            hardcore: false,
            expansion: true,
            progression: 0,
        };
        f.accounts
            .create_character_in(f.account, new, 1)
            .await
            .unwrap();
        f.ladder.season_end().await.unwrap();
        let seat = f.seat("Charsi").await;
        assert!(seat.record.is_none() && seat.new.is_some());
        assert_eq!(seat.summary.season, NON_LADDER);
        assert!(!seat.file.exists());
        assert_eq!(f.season_of("Charsi").await, NON_LADDER);
    });
}

#[test]
fn only_ended_seasons_are_frozen() {
    run(async {
        let f = fixture("frozen").await;
        assert!(!f.ladder.frozen(NON_LADDER).await.unwrap());
        assert!(!f.ladder.frozen(1).await.unwrap(), "no such season");
        f.ladder.season_start("Season 1", false).await.unwrap();
        assert!(!f.ladder.frozen(1).await.unwrap(), "open");
        f.ladder.season_start("Season 2", false).await.unwrap();
        assert!(f.ladder.frozen(1).await.unwrap(), "ended by the next one");
        assert!(!f.ladder.frozen(2).await.unwrap());
        f.ladder.season_end().await.unwrap();
        assert!(f.ladder.frozen(2).await.unwrap());
    });
}

#[test]
fn the_ladder_bit_is_cleared_only_where_set() {
    let set = record("Kashya", 9, status::EXPANSION | status::LADDER);
    let cleared = cleared_record(&set).expect("the bit was set");
    assert_eq!(status_of(&cleared), status::EXPANSION);
    assert_eq!(summary::of_record(&cleared, 0).unwrap().level, 9);
    assert!(cleared_record(&record("Kashya", 9, status::EXPANSION)).is_none());
    let s = CharSummary {
        name: "Kashya".into(),
        status: status::HARDCORE | status::LADDER,
        season: 3,
        ..CharSummary::default()
    };
    let n = non_ladder(&s);
    assert_eq!((n.status, n.season), (status::HARDCORE, NON_LADDER));
}
