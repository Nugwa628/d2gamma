//! The ladder service on a real accounts database in a temp folder.

use std::net::IpAddr;
use std::path::PathBuf;

use d2_accounts::{AccountsConfig, HashCost, NewChar};
use d2_net::msg::ladder::{mode, row_flags, split};

use super::*;

fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2rust-realm-ladder")
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

struct Fixture {
    ladder: Arc<Ladder>,
    accounts: Accounts,
    account: AccountId,
}

/// A ladder (on, ranks read at every query) with account "Nugwa".
async fn fixture(test: &str, on: bool) -> Fixture {
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
        on,
        refresh: Duration::ZERO,
    };
    Fixture {
        ladder: Ladder::new(accounts.clone(), "d2rust", cfg, None, log),
        accounts,
        account,
    }
}

impl Fixture {
    async fn create(&self, name: &str, class: u8, hardcore: bool, season: u16) -> CharSummary {
        let new = NewChar {
            name: name.into(),
            class,
            hardcore,
            expansion: true,
            progression: 0,
        };
        self.accounts
            .create_character_in(self.account, new, season)
            .await
            .unwrap()
    }

    fn asker(&self, character: Option<&str>) -> (Asker, mpsc::UnboundedReceiver<ControlOut>) {
        let (out, rx) = ControlTx::new(1);
        let who = Asker {
            account: self.account,
            character: character.map(str::to_owned),
            out,
        };
        (who, rx)
    }

    /// The answer to `ask`: every message up to its end.
    async fn ask(&self, character: Option<&str>, ask: Ask) -> Vec<FromRealm> {
        let (who, mut rx) = self.asker(character);
        let online = |_: i64, name: &str| name == "Online";
        self.ladder
            .request(&who, ToRealm { req: 9, ask }, &online)
            .await;
        let mut got = Vec::new();
        while let Ok(ControlOut::Body(b)) = rx.try_recv() {
            let m = FromRealm::parse(&b).unwrap();
            assert_eq!(m.req(), 9);
            got.push(m);
        }
        got
    }

    async fn query(&self, character: Option<&str>, q: LadderQuery) -> (LadderHead, Vec<LadderRow>) {
        let got = self.ask(character, Ask::Query(q)).await;
        let mut head = None;
        let mut rows = Vec::new();
        for m in got {
            match m {
                FromRealm::Head(h) => head = Some(h),
                FromRealm::Row { row, .. } => rows.push(row),
                FromRealm::End(e) => assert!(e.is_ok(), "{e:?}"),
                other => panic!("{other:?}"),
            }
        }
        (head.expect("a head"), rows)
    }

    async fn end_of(&self, ask: Ask) -> LadderEnd {
        match self.ask(None, ask).await.pop() {
            Some(FromRealm::End(e)) => e,
            other => panic!("{other:?}"),
        }
    }
}

use crate::session::ControlOut;
use d2_net::msg::account::CharSummary;

fn sc_exp(season: u16) -> LadderQuery {
    LadderQuery {
        season,
        split: split::EXPANSION,
        ..LadderQuery::default()
    }
}

#[test]
fn seasons_list_newest_first_then_season_0() {
    run(async {
        let f = fixture("seasons", true).await;
        let list = |got: Vec<FromRealm>| -> Vec<(u16, String, bool, bool)> {
            got.into_iter()
                .filter_map(|m| match m {
                    FromRealm::SeasonRow { row, .. } => {
                        Some((row.season, row.name, row.test, row.current))
                    }
                    FromRealm::End(e) => {
                        assert!(e.is_ok());
                        None
                    }
                    other => panic!("{other:?}"),
                })
                .collect()
        };
        assert_eq!(
            list(f.ask(None, Ask::Seasons).await),
            [(0, NON_LADDER_NAME.to_owned(), false, true)]
        );
        assert_eq!(f.ladder.current_season().await.unwrap(), None);
        f.ladder.season_start("Season 1", false).await.unwrap();
        f.ladder.season_start("Test season", true).await.unwrap();
        assert_eq!(f.ladder.current_season().await.unwrap(), Some(2));
        assert_eq!(
            list(f.ask(None, Ask::Seasons).await),
            [
                (2, "Test season".to_owned(), true, true),
                (1, "Season 1".to_owned(), false, false),
                (0, NON_LADDER_NAME.to_owned(), false, false),
            ]
        );
        f.ladder.season_end().await.unwrap();
        assert!(f.ladder.season_end().await.is_err(), "none open");
        assert_eq!(f.ladder.current_season().await.unwrap(), None);
        assert!(f.ladder.season_start("", false).await.is_err());
        assert!(f.ladder.season_start(&"x".repeat(33), false).await.is_err());
    });
}

#[test]
fn ladder_off_lists_season_0_only() {
    run(async {
        let f = fixture("off", false).await;
        assert!(f.ladder.season_start("Season 1", false).await.is_err());
        assert_eq!(f.ladder.current_season().await.unwrap(), None);
        let got = f.ask(None, Ask::Seasons).await;
        assert_eq!(got.len(), 2, "season 0, then the end: {got:?}");
        let end = f.end_of(Ask::Query(sc_exp(1))).await;
        assert_eq!(end.code, ErrorCode::NO_SUCH_SEASON);
        // Season 0 is always there.
        let (h, rows) = f.query(None, sc_exp(0)).await;
        assert_eq!((h.total, rows.len()), (0, 0));
    });
}

#[test]
fn ladder_and_non_ladder_characters_rank_apart() {
    run(async {
        let f = fixture("apart", true).await;
        f.ladder.season_start("Season 1", false).await.unwrap();
        let s = f.create("Laddie", 1, false, 1).await;
        assert_eq!(s.season, 1);
        assert_ne!(s.status & status::LADDER, 0, "the ladder bit is set");
        let s = f.create("Plain", 2, false, NON_LADDER).await;
        assert_eq!((s.season, s.status & status::LADDER), (0, 0));
        let listed = f.accounts.characters(f.account).await;
        let seasons: Vec<u16> = listed.iter().map(|c| c.season).collect();
        assert_eq!(seasons, [1, 0], "the row keeps the season");
        f.ladder.seed("Laddie", 10, 5_000, false).await.unwrap();
        f.ladder.seed("plain", 20, 9_000, false).await.unwrap();
        let (h, rows) = f.query(Some("Laddie"), sc_exp(1)).await;
        assert_eq!((h.total, h.my_rank), (1, Some(1)));
        assert_eq!(
            (rows[0].char.as_str(), rows[0].level, rows[0].experience),
            ("Laddie", 10, 5_000)
        );
        assert_eq!(
            rows[0].flags,
            row_flags::MY_ACCOUNT | row_flags::MY_CHARACTER
        );
        let (h, rows) = f.query(Some("Laddie"), sc_exp(0)).await;
        assert_eq!(
            (h.total, h.my_rank, rows[0].char.as_str()),
            (1, None, "Plain")
        );
        // The summary took the seeded level.
        let listed = f.accounts.characters(f.account).await;
        assert_eq!(listed[1].level, 20);
        // Other splits are empty.
        let mut q = sc_exp(1);
        q.split = split::HARDCORE | split::EXPANSION;
        assert_eq!(f.query(None, q).await.0.total, 0);
    });
}

#[test]
fn deleted_leaves() {
    run(async {
        let f = fixture("deleted", true).await;
        for (name, exp) in [("Stays", 10), ("Goes", 20)] {
            f.create(name, 0, false, NON_LADDER).await;
            f.ladder.seed(name, 1, exp, false).await.unwrap();
        }
        assert_eq!(f.query(None, sc_exp(0)).await.0.total, 2);
        f.accounts
            .delete_character(f.account, "Goes", "correct horse")
            .await
            .unwrap();
        f.ladder.deleted("Goes").await;
        let (h, rows) = f.query(None, sc_exp(0)).await;
        assert_eq!((h.total, rows[0].char.as_str()), (1, "Stays"));
        let mut q = sc_exp(0);
        q.mode = mode::FIND;
        q.find = "goes".into();
        assert_eq!(
            f.end_of(Ask::Query(q)).await.code,
            ErrorCode::UNKNOWN_PLAYER
        );
    });
}

#[test]
fn dead_hardcore_stays_marked() {
    run(async {
        let f = fixture("dead", true).await;
        f.create("Gone", 3, true, NON_LADDER).await;
        f.create("Online", 3, true, NON_LADDER).await;
        f.ladder.seed("Gone", 30, 900, true).await.unwrap();
        f.ladder.seed("Online", 10, 100, false).await.unwrap();
        let mut q = sc_exp(0);
        q.split = split::HARDCORE | split::EXPANSION;
        let (h, rows) = f.query(None, q).await;
        assert_eq!(h.total, 2);
        assert_eq!(rows[0].char, "Gone");
        assert_eq!(
            rows[0].flags,
            row_flags::DEAD_HARDCORE | row_flags::MY_ACCOUNT
        );
        assert_eq!(rows[1].flags, row_flags::MY_ACCOUNT | row_flags::ONLINE);
        assert!(f.ladder.seed("Nobody", 1, 1, false).await.is_err());
    });
}

#[test]
fn pinned_own_row_and_pages() {
    run(async {
        let f = fixture("pinned", true).await;
        // 60 characters, 59 on two other accounts, ours the last.
        let mut others = Vec::new();
        for (name, ip) in [("Other", 2), ("Third", 3)] {
            let a = f
                .accounts
                .register(name, "correct horse", IpAddr::from([10, 0, 0, ip]))
                .await
                .unwrap();
            others.push(a.id);
        }
        let seed_name = |i: u64| {
            let letter = |n: u64| char::from(b'a' + u8::try_from(n).unwrap());
            format!("Seed{}{}", letter(i / 26), letter(i % 26))
        };
        for i in 0..59u64 {
            let other = others[usize::from(i >= 30)];
            let new = NewChar {
                name: seed_name(i),
                class: u8::try_from(i % 7).unwrap(),
                hardcore: false,
                expansion: true,
                progression: 0,
            };
            f.accounts.create_character(other, new).await.unwrap();
            f.ladder
                .seed(&seed_name(i), 50, 100_000 - i, false)
                .await
                .unwrap();
        }
        f.create("Mine", 0, false, NON_LADDER).await;
        f.ladder.seed("Mine", 2, 10, false).await.unwrap();
        let (h, rows) = f.query(Some("Mine"), sc_exp(0)).await;
        assert_eq!(
            (h.total, h.first_rank, h.rows, h.my_rank),
            (60, 1, 50, Some(60))
        );
        assert_eq!(rows.len(), 51, "50 and the pinned row");
        let pinned = rows.last().unwrap();
        assert_eq!((pinned.rank, pinned.char.as_str()), (60, "Mine"));
        assert_ne!(pinned.flags & row_flags::PINNED, 0);
        assert!(rows[..50].iter().all(|r| r.flags & row_flags::PINNED == 0));
        // Around me: the second page, nothing pinned.
        let mut q = sc_exp(0);
        q.mode = mode::AROUND_ME;
        let (h, rows) = f.query(Some("Mine"), q).await;
        assert_eq!((h.first_rank, h.rows, rows.len()), (51, 10, 10));
        // A class view.
        let mut q = sc_exp(0);
        q.class = 6;
        let (h, rows) = f.query(None, q).await;
        assert_eq!(h.total, 8);
        assert!(rows.iter().all(|r| r.class == 6));
    });
}

#[test]
fn queries_are_limited_per_account() {
    run(async {
        let f = fixture("limited", true).await;
        for _ in 0..QUERIES_PER_MIN {
            assert!(f.end_of(Ask::Seasons).await.is_ok());
        }
        let end = f.end_of(Ask::Query(sc_exp(0))).await;
        assert_eq!(end.code, ErrorCode::RATE_LIMITED);
        assert!(end.wait_s >= 1);
    });
}
