//! Seat leases and the save desk on a real accounts database in a temp
//! folder (plan N2).

use std::net::IpAddr;
use std::path::{Path, PathBuf};

use d2_accounts::{AccountsConfig, HashCost, NewChar};
use d2_formats::d2s::{status, Header};
use d2_net::msg::account::CharSummary;

use super::*;

fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2rust-realm-saves")
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

/// A save record of `name` at `level`.
fn record(name: &str, level: u8) -> Vec<u8> {
    let mut h = Header {
        class: 3,
        level,
        status: status::EXPANSION,
        ..Header::default()
    };
    h.set_name(name.as_bytes()).unwrap();
    h.to_bytes().to_vec()
}

fn save(lease: Lease, game: u32, seq: u32, name: &str, level: u8) -> NodeSave {
    NodeSave {
        info: SaveInfo {
            game,
            lease: lease.0,
            seq,
            reason: 0,
            journal_tick: 0,
            summary: CharSummary {
                name: name.into(),
                class: 3,
                level,
                status: status::EXPANSION,
                difficulties: 1,
                look: Vec::new(),
                last_played: 0,
                season: 0,
            },
        },
        record: record(name, level),
    }
}

/// A desk on a fresh database with account "Nugwa" and its character
/// "Kashya"; returns the desk, the account and the data folder.
fn desk(test: &str) -> (SaveDesk, AccountId, PathBuf) {
    let dir = temp(test);
    let acc = Accounts::open_with(AccountsConfig {
        cost: HashCost::Test,
        fail_delay: Duration::ZERO,
        ..AccountsConfig::new(&dir)
    })
    .unwrap();
    let a = run(async {
        let a = acc
            .register("Nugwa", "correct horse", IpAddr::from([10, 0, 0, 1]))
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
    let log = Arc::new(ServerLog::open(&dir.join("logs")).unwrap());
    (SaveDesk::new(LeaseTable::new(1), acc, log), a, dir)
}

fn grant(a: AccountId, game: u32, node: &str) -> LeaseGrant {
    LeaseGrant {
        account: a,
        character: "Kashya".into(),
        game,
        node: node.into(),
    }
}

/// The level of Kashya's stored record (None before its first save).
fn stored_level(desk: &SaveDesk, a: AccountId) -> Option<u8> {
    run(desk.accounts.seat(a, "Kashya"))
        .unwrap()
        .record
        .map(|r| d2_accounts::summary::of_record(&r, 0).unwrap().level)
}

fn log_text(dir: &Path) -> String {
    let mut text = String::new();
    for e in std::fs::read_dir(dir.join("logs")).unwrap() {
        text.push_str(&std::fs::read_to_string(e.unwrap().path()).unwrap_or_default());
    }
    text
}

#[test]
fn a_live_lease_stores_its_saves_in_order() {
    let (desk, a, _dir) = desk("live");
    let lease = desk.leases().grant(grant(a, 7, "eu-1")).unwrap();
    let ack = run(desk.accept("eu-1", save(lease, 7, 1, "Kashya", 5))).unwrap();
    assert_eq!(ack.ack, SaveAck { lease, seq: 1 });
    assert_eq!(ack.summary.name, "Kashya");
    assert_eq!(stored_level(&desk, a), Some(5));
    run(desk.accept("eu-1", save(lease, 7, 3, "Kashya", 6))).unwrap();
    assert_eq!(stored_level(&desk, a), Some(6), "a skipped seq is fine");
    assert_eq!(
        run(desk.accounts.character(a, "Kashya")).unwrap().level,
        6,
        "the row took the save"
    );
}

#[test]
fn save_after_close_is_refused() {
    let (desk, a, dir) = desk("closed");
    let lease = desk.leases().grant(grant(a, 7, "eu-1")).unwrap();
    run(desk.accept("eu-1", save(lease, 7, 1, "Kashya", 5))).unwrap();
    desk.end_game("eu-1", 7, LeaseEnd::GameClosed);
    let e = run(desk.accept("eu-1", save(lease, 7, 2, "Kashya", 50))).unwrap_err();
    assert_eq!(e.code, ErrorCode::LEASE_REVOKED);
    assert!(e.text.contains("the game closed"), "{}", e.text);
    assert_eq!(
        stored_level(&desk, a),
        Some(5),
        "the character stays as last saved"
    );
    let log = log_text(&dir);
    assert!(log.contains("ended: the game closed"), "{log}");
    assert!(log.contains("refused: LeaseRevoked"), "{log}");
    // Its seat can be taken again, under a new lease.
    let again = desk.leases().grant(grant(a, 8, "eu-1")).unwrap();
    assert_ne!(again, lease);
}

#[test]
fn save_with_old_seq_is_refused() {
    let (desk, a, _dir) = desk("seq");
    let lease = desk.leases().grant(grant(a, 7, "eu-1")).unwrap();
    run(desk.accept("eu-1", save(lease, 7, 2, "Kashya", 9))).unwrap();
    for (seq, level) in [(1, 3), (2, 4)] {
        let e = run(desk.accept("eu-1", save(lease, 7, seq, "Kashya", level))).unwrap_err();
        assert_eq!(e.code, ErrorCode::SAVE_INVALID, "seq {seq}");
    }
    assert_eq!(stored_level(&desk, a), Some(9));
    run(desk.accept("eu-1", save(lease, 7, 3, "Kashya", 10))).unwrap();
    assert_eq!(stored_level(&desk, a), Some(10));
}

#[test]
fn save_from_other_node_is_refused() {
    let (desk, a, _dir) = desk("other");
    let lease = desk.leases().grant(grant(a, 7, "eu-1")).unwrap();
    let e = run(desk.accept("eu-2", save(lease, 7, 1, "Kashya", 50))).unwrap_err();
    assert_eq!(e.code, ErrorCode::LEASE_REVOKED);
    // Nor for another game, nor a lease nobody granted.
    let e = run(desk.accept("eu-1", save(lease, 8, 1, "Kashya", 50))).unwrap_err();
    assert_eq!(e.code, ErrorCode::LEASE_REVOKED);
    let e = run(desk.accept("eu-1", save(Lease(999), 7, 1, "Kashya", 50))).unwrap_err();
    assert_eq!(e.code, ErrorCode::LEASE_REVOKED);
    assert!(e.text.contains("unknown"), "{}", e.text);
    // Another game server can't end it either.
    desk.end("eu-2", lease, LeaseEnd::Left);
    assert!(desk.leases().get(lease).is_some());
    assert_eq!(stored_level(&desk, a), None, "nothing was written");
}

#[test]
fn a_record_of_another_character_is_refused() {
    let (desk, a, _dir) = desk("name");
    let lease = desk.leases().grant(grant(a, 7, "eu-1")).unwrap();
    let mut s = save(lease, 7, 1, "Kashya", 5);
    s.record = record("Akara", 99);
    let e = run(desk.accept("eu-1", s)).unwrap_err();
    assert_eq!(e.code, ErrorCode::SAVE_INVALID);
    assert_eq!(stored_level(&desk, a), None);
}

#[test]
fn takeover_and_loss_end_leases() {
    let (desk, a, _dir) = desk("takeover");
    let lease = desk.leases().grant(grant(a, 7, "eu-1")).unwrap();
    assert_eq!(
        desk.leases().grant(grant(a, 8, "eu-2")).unwrap_err().code,
        ErrorCode::CHAR_IN_GAME,
        "one live lease per character"
    );
    assert_eq!(desk.leases().of_character("KASHYA"), Some(lease));
    let ended = desk.leases().end_account(a, LeaseEnd::Takeover);
    assert_eq!(ended.iter().map(|(l, _)| *l).collect::<Vec<_>>(), [lease]);
    let e = run(desk.accept("eu-1", save(lease, 7, 1, "Kashya", 5))).unwrap_err();
    assert_eq!(e.code, ErrorCode::LEASE_REVOKED);
    assert!(e.text.contains("logged in again"), "{}", e.text);

    let lease = desk.leases().grant(grant(a, 9, "eu-2")).unwrap();
    desk.end_node("eu-2", LeaseEnd::NodeLost);
    assert!(desk.leases().is_empty());
    let e = run(desk.accept("eu-2", save(lease, 9, 1, "Kashya", 5))).unwrap_err();
    assert!(e.text.contains("game server was lost"), "{}", e.text);
}

#[test]
fn a_node_holds_a_game_while_it_has_a_seat_there() {
    let mut t = LeaseTable::new(1);
    let a = AccountId(1);
    assert!(!t.holds_game("eu-1", 7));
    let lease = t.grant(grant(a, 7, "eu-1")).unwrap();
    assert!(t.holds_game("eu-1", 7));
    assert!(!t.holds_game("eu-2", 7), "another game server");
    assert!(!t.holds_game("eu-1", 8), "another game");
    t.end(lease, LeaseEnd::Left);
    assert!(!t.holds_game("eu-1", 7));
}

#[test]
fn leases_are_never_reused() {
    let mut t = LeaseTable::new(5);
    let g = |c: &str| LeaseGrant {
        account: AccountId(1),
        character: c.into(),
        game: 1,
        node: "eu-1".into(),
    };
    let a = t.grant(g("A")).unwrap();
    t.end(a, LeaseEnd::Left);
    let b = t.grant(g("A")).unwrap();
    assert_eq!((a, b), (Lease(5), Lease(6)));
    assert!(LeaseTable::starting_now().next > 1_600_000_000_000 * 1024);
}

#[test]
fn a_save_and_its_record_pair_in_either_order() {
    let (desk, a, _dir) = desk("pairs");
    let lease = desk.leases().grant(grant(a, 7, "eu-1")).unwrap();
    let t0 = Instant::now();
    let mut p = SavePairs::default();
    let s1 = save(lease, 7, 1, "Kashya", 5);
    assert_eq!(p.info(s1.info.clone(), t0), None);
    assert_eq!(
        p.record(lease.0, 1, s1.record.clone(), t0),
        Some(s1.clone())
    );
    let s2 = save(lease, 7, 2, "Kashya", 6);
    assert_eq!(p.record(lease.0, 2, s2.record.clone(), t0), None);
    assert_eq!(p.info(s2.info.clone(), t0), Some(s2));
    assert_eq!(p.waiting(), 0);
    // A message whose record never comes is given up after PAIR_WAIT.
    let s3 = save(lease, 7, 3, "Kashya", 7);
    p.info(s3.info.clone(), t0);
    p.record(lease.0, 4, vec![1], t0);
    assert!(p.expire(t0 + PAIR_WAIT / 2).is_empty());
    assert_eq!(p.expire(t0 + PAIR_WAIT), vec![s3.info]);
    assert_eq!(p.waiting(), 0);
    // Bounded.
    for seq in 0..(PAIRS_MAX as u32 + 10) {
        p.record(
            lease.0,
            100 + seq,
            vec![],
            t0 + Duration::from_millis(seq.into()),
        );
    }
    assert_eq!(p.waiting(), PAIRS_MAX);
    assert_eq!(
        p.info(save(lease, 7, 100, "Kashya", 1).info, t0),
        None,
        "the oldest went"
    );
}
