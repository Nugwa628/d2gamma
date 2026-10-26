//! The audit service end to end on an in-memory database: triggers and
//! sampling, the queue, runs with a scripted engine, findings, rollback and
//! undo, the determinism watch, and journal retention with its holds.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use d2_audit::engine::{
    Divergence, IntentFault, IntentResult, RemadeSave, ReplayOutcome, ScriptedEngine, SeatSpan,
};
use d2_audit::{
    AuditConfig, AuditError, AuditState, Audits, CharacterStore, FindingKind, FindingState,
    GameKind, GameStart, Joined, Reason, RollbackMode, RollbackState, SeatJoin,
};

const DAY: u64 = 86_400_000;

#[derive(Default)]
struct MemStore {
    chars: RefCell<BTreeMap<String, Vec<u8>>>,
}

impl MemStore {
    fn get(&self, name: &str) -> Vec<u8> {
        self.chars.borrow()[name].clone()
    }
    fn set(&self, name: &str, b: &[u8]) {
        self.chars.borrow_mut().insert(name.into(), b.to_vec());
    }
}

impl CharacterStore for MemStore {
    fn load(&self, c: &str) -> Result<Vec<u8>, String> {
        self.chars
            .borrow()
            .get(c)
            .cloned()
            .ok_or_else(|| format!("no {c}"))
    }
    fn store(&self, c: &str, r: &[u8]) -> Result<(), String> {
        self.set(c, r);
        Ok(())
    }
    fn fresh(&self, c: &str, class: u8, _: bool, _: u8) -> Result<Vec<u8>, String> {
        Ok(format!("fresh {c} {class}").into_bytes())
    }
}

struct Fixture {
    audits: Audits,
    dir: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn fixture(name: &str, cfg: AuditConfig) -> Fixture {
    let dir = std::env::temp_dir().join(format!("d2-audit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("journals")).unwrap();
    let audits = Audits::in_memory(&dir.join("audits"), cfg).unwrap();
    Fixture { audits, dir }
}

impl Fixture {
    fn journal(&self, game: u32) -> PathBuf {
        let p = self.dir.join("journals").join(format!("{game}.d2j.gz"));
        std::fs::write(&p, b"journal").unwrap();
        p
    }

    fn start(&self, game: u32, kind: GameKind, now: u64) {
        let j = self.journal(game);
        self.audits
            .game_started(
                &GameStart {
                    game,
                    realm: "d2rust".into(),
                    node: if kind == GameKind::Official {
                        "fi-1".into()
                    } else {
                        "h-0123456789ab".into()
                    },
                    kind,
                },
                &j,
                now,
            )
            .unwrap();
    }

    fn seat(&self, game: u32, lease: u64, name: &str, host: bool, record: &[u8]) {
        self.audits
            .seat_joined(
                game,
                &SeatJoin {
                    lease,
                    character: name.into(),
                    host,
                    joined: Joined::Record(record.to_vec()),
                },
                1,
            )
            .unwrap();
    }
}

fn no_spots() -> AuditConfig {
    AuditConfig {
        spot_official_pct: 0,
        spot_hosted_pct: 0,
        ..AuditConfig::default()
    }
}

fn matched(seats: &[(u64, u32, Option<u32>)]) -> ReplayOutcome {
    ReplayOutcome {
        records: 100,
        frames: 1200,
        last_tick: 1200,
        state_hashes: 2,
        ended: true,
        seats: seats
            .iter()
            .map(|&(lease, joined, left)| SeatSpan {
                lease,
                joined,
                left,
            })
            .collect(),
        ..ReplayOutcome::default()
    }
}

fn diverged(seats: &[(u64, u32, Option<u32>)], tick: u32, last_good: u32) -> ReplayOutcome {
    ReplayOutcome {
        divergence: Some(Divergence {
            tick,
            last_good,
            what: "state hash differs".into(),
        }),
        ended: false,
        ..matched(seats)
    }
}

fn engine(game: u32, o: Result<ReplayOutcome, String>) -> ScriptedEngine {
    ScriptedEngine {
        outcomes: BTreeMap::from([(game, o)]),
    }
}

#[test]
fn host_character_audited_every_game() {
    let f = fixture("host", no_spots());
    for game in 1..=20 {
        f.start(game, GameKind::PlayerHosted, 0);
        f.seat(game, u64::from(game) * 10, "Host", true, b"h");
        f.seat(game, u64::from(game) * 10 + 1, "Guest", false, b"g");
    }
    let queued = f.audits.audits(Some(AuditState::Queued)).unwrap();
    assert_eq!(queued.len(), 20);
    for a in &queued {
        assert_eq!(a.reasons.len(), 1);
        assert_eq!(a.reasons[0].0, Reason::HostCharacter);
        assert_eq!(a.reasons[0].2, "Host");
    }
}

#[test]
fn spot_shares_of_official_games_and_hosted_players() {
    let f = fixture("shares", AuditConfig::default());
    for game in 0..4000 {
        f.start(game, GameKind::Official, 0);
    }
    let official = f.audits.audits(None).unwrap().len();
    // 1% of 4,000 is 40.
    assert!((15..=70).contains(&official), "official {official}");
    assert!(f
        .audits
        .audits(None)
        .unwrap()
        .iter()
        .all(|a| a.reasons[0].0 == Reason::SpotOfficial));

    for game in 10_000..10_500 {
        f.start(game, GameKind::PlayerHosted, 0);
        for p in 0..8 {
            f.seat(
                game,
                u64::from(game) * 10 + p,
                &format!("P{p}"),
                false,
                b"r",
            );
        }
    }
    let spots: usize = f
        .audits
        .audits(None)
        .unwrap()
        .iter()
        .flat_map(|a| a.reasons.iter())
        .filter(|r| r.0 == Reason::SpotHosted)
        .count();
    // 5% of 4,000 characters is 200.
    assert!((140..=260).contains(&spots), "hosted {spots}");
}

#[test]
fn audits_off_keeps_only_asked_audits() {
    let cfg = AuditConfig {
        on: false,
        spot_official_pct: 100,
        spot_hosted_pct: 100,
        ..AuditConfig::default()
    };
    let f = fixture("off", cfg);
    f.start(1, GameKind::Official, 0);
    f.start(2, GameKind::PlayerHosted, 0);
    f.seat(2, 20, "Host", true, b"h");
    assert!(f.audits.audits(None).unwrap().is_empty());
    f.audits.request(1, "owner", 5).unwrap();
    f.audits.report_game(2, "acct-7", 1, "dupes", 5).unwrap();
    f.audits.save_flagged(2, 20, &["item-ranges"], 5).unwrap();
    let all = f.audits.audits(None).unwrap();
    assert_eq!(all.len(), 2);
    let g2 = f.audits.audit_of_game(2).unwrap();
    let reasons: Vec<Reason> = g2.reasons.iter().map(|r| r.0).collect();
    assert_eq!(reasons, [Reason::Report, Reason::SaveFlagged]);
}

#[test]
fn one_open_audit_per_game_and_it_waits_for_the_end() {
    let f = fixture("merge", no_spots());
    f.start(1, GameKind::PlayerHosted, 0);
    f.seat(1, 10, "Host", true, b"h");
    let a = f.audits.report_game(1, "acct-1", 2, "", 3).unwrap();
    assert_eq!(f.audits.request(1, "owner", 4).unwrap(), a);
    assert_eq!(f.audits.audit(a).unwrap().reasons.len(), 3);
    let store = MemStore::default();
    let e = engine(1, Ok(matched(&[(10, 0, None)])));
    // Not ended: nothing to run, and running it by number is refused.
    assert!(f.audits.run_next(&e, &store, 5).unwrap().is_none());
    assert!(matches!(
        f.audits.run(a, &e, &store, 5),
        Err(AuditError::State(_))
    ));
    f.audits.game_ended(1, 6).unwrap();
    let r = f.audits.run_next(&e, &store, 7).unwrap().unwrap();
    assert_eq!((r.audit, r.state), (a, AuditState::Passed));
    // A finished audit doesn't take new reasons: a new one is queued.
    assert_ne!(f.audits.request(1, "owner", 8).unwrap(), a);
}

#[test]
fn asked_audits_run_before_standing_checks() {
    let f = fixture("order", no_spots());
    for g in 1..=3 {
        f.start(g, GameKind::PlayerHosted, 0);
        f.seat(g, u64::from(g), "Host", true, b"h");
        f.audits.game_ended(g, 1).unwrap();
    }
    f.audits.report_game(3, "acct", 1, "", 2).unwrap();
    let store = MemStore::default();
    let e = ScriptedEngine {
        outcomes: (1..=3)
            .map(|g| (g, Ok(matched(&[(u64::from(g), 0, None)]))))
            .collect(),
    };
    let order: Vec<u32> = std::iter::from_fn(|| f.audits.run_next(&e, &store, 9).unwrap())
        .map(|r| r.game)
        .collect();
    assert_eq!(order, [3, 1, 2]);
}

#[test]
fn passed_audit_writes_its_report() {
    let f = fixture("pass", no_spots());
    f.start(4, GameKind::PlayerHosted, 0);
    f.seat(4, 40, "Host", true, b"h");
    f.audits.save_stored(4, 40, 1, 600, &[9; 32]).unwrap();
    f.audits.game_ended(4, 1).unwrap();
    let mut o = matched(&[(40, 0, None)]);
    o.saves.push(RemadeSave {
        tick: 600,
        lease: 40,
        seq: 1,
        digest: [9; 32],
    });
    let r = f
        .audits
        .run_next(&engine(4, Ok(o)), &MemStore::default(), 2)
        .unwrap()
        .unwrap();
    assert_eq!(r.state, AuditState::Passed);
    assert!(r.findings.is_empty() && r.rollbacks.is_empty());
    assert_eq!(r.report, f.dir.join("audits").join("4.md"));
    let text = std::fs::read_to_string(&r.report).unwrap();
    assert!(text.starts_with("# Audit 1 of game 4: passed"), "{text}");
    assert!(
        text.contains("host's own character (lease 40): Host"),
        "{text}"
    );
}

/// A player-hosted game with four seats: A (host) all game, B left before
/// the last good tick, C left after it, D joined after the divergence.
fn hosted_failure(f: &Fixture, store: &MemStore) -> d2_audit::RunResult {
    f.start(7, GameKind::PlayerHosted, 0);
    for (lease, name) in [(1, "Ann"), (2, "Bob"), (3, "Cid"), (4, "Dee")] {
        f.seat(
            7,
            lease,
            name,
            lease == 1,
            format!("{name} joined").as_bytes(),
        );
        store.set(name, format!("{name} after").as_bytes());
    }
    f.audits.game_ended(7, DAY).unwrap();
    let o = diverged(
        &[
            (1, 0, None),
            (2, 0, Some(500)),
            (3, 0, Some(700)),
            (4, 2000, None),
        ],
        1200,
        600,
    );
    f.audits
        .run_next(&engine(7, Ok(o)), store, DAY)
        .unwrap()
        .unwrap()
}

#[test]
fn failed_audit_proposes_rollbacks_owner_confirms() {
    let f = fixture("confirm", no_spots());
    let store = MemStore::default();
    let r = hosted_failure(&f, &store);
    assert_eq!(r.state, AuditState::Failed);
    assert_eq!(r.findings.len(), 1);
    let fd = &r.findings[0];
    assert_eq!(fd.kind, FindingKind::Divergence);
    assert_eq!((fd.tick, fd.last_good), (Some(1200), Some(600)));
    assert_eq!(fd.state, FindingState::Open);
    let chars: Vec<&str> = r.rollbacks.iter().map(|b| b.character.as_str()).collect();
    assert_eq!(chars, ["Ann", "Cid"]);
    assert!(r
        .rollbacks
        .iter()
        .all(|b| b.state == RollbackState::Proposed));
    // Nothing changed until the owner confirms.
    assert_eq!(store.get("Ann"), b"Ann after");

    let done = f.audits.confirm_audit(r.audit, &store, DAY + 1).unwrap();
    assert_eq!(done.len(), 2);
    assert_eq!(store.get("Ann"), b"Ann joined");
    assert_eq!(store.get("Cid"), b"Cid joined");
    assert_eq!(store.get("Bob"), b"Bob after");
    assert_eq!(
        f.audits.findings_of(r.audit).unwrap()[0].state,
        FindingState::RolledBack
    );
    let text = std::fs::read_to_string(&r.report).unwrap();
    assert!(text.contains("| Ann | 1 | applied |"), "{text}");

    // Undo puts the record back.
    let ann = r.rollbacks[0].id;
    f.audits.undo_rollback(ann, &store, false, DAY + 2).unwrap();
    assert_eq!(store.get("Ann"), b"Ann after");
    assert!(matches!(
        f.audits.undo_rollback(ann, &store, false, DAY + 3),
        Err(AuditError::State(_))
    ));

    // A character that played again since its rollback is not undone
    // without force.
    let cid = r.rollbacks[1].id;
    store.set("Cid", b"Cid played again");
    assert!(matches!(
        f.audits.undo_rollback(cid, &store, false, DAY + 4),
        Err(AuditError::State(_))
    ));
    assert_eq!(store.get("Cid"), b"Cid played again");
    f.audits.undo_rollback(cid, &store, true, DAY + 5).unwrap();
    assert_eq!(store.get("Cid"), b"Cid after");
    assert_eq!(
        f.audits.findings_of(r.audit).unwrap()[0].state,
        FindingState::Dismissed
    );
}

#[test]
fn failed_audit_rolls_back_at_once_when_automatic() {
    let cfg = AuditConfig {
        rollback: RollbackMode::Automatic,
        ..no_spots()
    };
    let f = fixture("auto", cfg);
    let store = MemStore::default();
    let r = hosted_failure(&f, &store);
    assert!(r
        .rollbacks
        .iter()
        .all(|b| b.state == RollbackState::Applied));
    assert_eq!(store.get("Ann"), b"Ann joined");
    assert_eq!(store.get("Dee"), b"Dee after");
}

#[test]
fn declined_rollback_keeps_the_character() {
    let f = fixture("decline", no_spots());
    let store = MemStore::default();
    let r = hosted_failure(&f, &store);
    for b in &r.rollbacks {
        f.audits.decline_rollback(b.id, DAY + 1).unwrap();
    }
    assert_eq!(store.get("Ann"), b"Ann after");
    assert!(matches!(
        f.audits.apply_rollback(r.rollbacks[0].id, &store, DAY + 2),
        Err(AuditError::State(_))
    ));
    assert_eq!(
        f.audits.findings_of(r.audit).unwrap()[0].state,
        FindingState::Dismissed
    );
}

#[test]
fn new_character_rolls_back_to_a_fresh_one() {
    let f = fixture("fresh", no_spots());
    let store = MemStore::default();
    f.start(8, GameKind::PlayerHosted, 0);
    f.audits
        .seat_joined(
            8,
            &SeatJoin {
                lease: 1,
                character: "Neo".into(),
                host: true,
                joined: Joined::New {
                    class: 3,
                    hardcore: false,
                    progression: 0,
                },
            },
            0,
        )
        .unwrap();
    store.set("Neo", b"level 40 in one game");
    f.audits.game_ended(8, 1).unwrap();
    let r = f
        .audits
        .run_next(&engine(8, Ok(diverged(&[(1, 0, None)], 60, 0))), &store, 2)
        .unwrap()
        .unwrap();
    f.audits.confirm_audit(r.audit, &store, 3).unwrap();
    assert_eq!(store.get("Neo"), b"fresh Neo 3");
}

#[test]
fn official_divergence_is_a_determinism_bug_not_a_cheat() {
    let f = fixture(
        "determinism",
        AuditConfig {
            spot_official_pct: 100,
            ..AuditConfig::default()
        },
    );
    let store = MemStore::default();
    for g in 1..=4 {
        f.start(g, GameKind::Official, 0);
        f.seat(g, u64::from(g), "Ann", false, b"joined");
        f.audits.game_ended(g, 1).unwrap();
    }
    store.set("Ann", b"after");
    let e = ScriptedEngine {
        outcomes: (1..=4)
            .map(|g| {
                let o = if g == 3 {
                    diverged(&[(3, 0, None)], 900, 600)
                } else {
                    matched(&[(u64::from(g), 0, None)])
                };
                (g, Ok(o))
            })
            .collect(),
    };
    while f.audits.run_next(&e, &store, 2).unwrap().is_some() {}
    let a = f.audits.audit_of_game(3).unwrap();
    assert_eq!(a.state, AuditState::Failed);
    let fd = f.audits.findings_of(a.id).unwrap();
    assert_eq!(fd[0].kind, FindingKind::Determinism);
    assert_eq!(fd[0].state, FindingState::Bug);
    assert!(f.audits.rollbacks().unwrap().is_empty());
    assert_eq!(store.get("Ann"), b"after");
    let w = f.audits.watch().unwrap();
    assert_eq!((w.official_audited, w.official_matched), (4, 3));
    assert_eq!((w.spot_audited, w.spot_matched), (4, 3));
    assert_eq!(w.determinism_bugs, [3]);
    assert!((w.matched_pct() - 75.0).abs() < 1e-9);
    let text = std::fs::read_to_string(f.dir.join("audits/3.md")).unwrap();
    assert!(
        text.starts_with("# Audit 3 of game 3: determinism bug"),
        "{text}"
    );
    // The journal of the bug is held past the window.
    assert!(f.audits.prune(30 * DAY).unwrap().iter().all(|g| *g != 3));
    assert!(f.audits.game(3).unwrap().held);
}

#[test]
fn stored_save_that_differs_affects_only_its_character() {
    let f = fixture("stored", no_spots());
    let store = MemStore::default();
    f.start(9, GameKind::PlayerHosted, 0);
    f.seat(9, 1, "Ann", true, b"a");
    f.seat(9, 2, "Bob", false, b"b");
    store.set("Ann", b"a2");
    store.set("Bob", b"b2");
    f.audits.save_stored(9, 1, 1, 600, &[1; 32]).unwrap();
    // The game server journaled one save of Bob and sent the main server
    // another.
    f.audits.save_stored(9, 2, 1, 600, &[5; 32]).unwrap();
    f.audits.game_ended(9, 1).unwrap();
    let mut o = matched(&[(1, 0, None), (2, 0, None)]);
    for (lease, d) in [(1, [1; 32]), (2, [2; 32])] {
        o.saves.push(RemadeSave {
            tick: 600,
            lease,
            seq: 1,
            digest: d,
        });
    }
    let r = f
        .audits
        .run_next(&engine(9, Ok(o)), &store, 2)
        .unwrap()
        .unwrap();
    assert_eq!(r.state, AuditState::Failed);
    assert_eq!(r.findings.len(), 1);
    assert_eq!(r.findings[0].kind, FindingKind::StoredSave);
    assert_eq!(r.findings[0].lease, Some(2));
    let chars: Vec<&str> = r.rollbacks.iter().map(|b| b.character.as_str()).collect();
    assert_eq!(chars, ["Bob"]);
}

#[test]
fn stored_saves_after_a_divergence_are_covered_by_it() {
    let f = fixture("covered", no_spots());
    f.start(10, GameKind::PlayerHosted, 0);
    f.seat(10, 1, "Ann", true, b"a");
    f.audits.save_stored(10, 1, 1, 600, &[1; 32]).unwrap();
    f.audits.save_stored(10, 1, 2, 1800, &[2; 32]).unwrap();
    f.audits.game_ended(10, 1).unwrap();
    let mut o = diverged(&[(1, 0, None)], 1300, 1200);
    o.saves.push(RemadeSave {
        tick: 600,
        lease: 1,
        seq: 1,
        digest: [1; 32],
    });
    let r = f
        .audits
        .run_next(&engine(10, Ok(o)), &MemStore::default(), 2)
        .unwrap()
        .unwrap();
    let kinds: Vec<FindingKind> = r.findings.iter().map(|x| x.kind).collect();
    assert_eq!(kinds, [FindingKind::Divergence]);
}

#[test]
fn intents_that_differ_affect_everyone_from_their_tick() {
    let f = fixture("intents", no_spots());
    f.start(11, GameKind::PlayerHosted, 0);
    f.seat(11, 1, "Ann", true, b"a");
    f.seat(11, 2, "Bob", false, b"b");
    f.seat(11, 3, "Cid", false, b"c");
    f.audits.game_ended(11, 1).unwrap();
    let mut o = matched(&[(1, 0, None), (2, 0, None), (3, 0, Some(300))]);
    o.intents = vec![
        IntentResult {
            lease: 1,
            fault: None,
        },
        IntentResult {
            lease: 2,
            fault: Some(IntentFault {
                span: 4,
                tick: Some(2400),
            }),
        },
    ];
    let r = f
        .audits
        .run_next(&engine(11, Ok(o)), &MemStore::default(), 2)
        .unwrap()
        .unwrap();
    assert_eq!(r.findings.len(), 1);
    assert_eq!(r.findings[0].kind, FindingKind::Intents);
    assert_eq!(r.findings[0].lease, Some(2));
    let chars: Vec<&str> = r.rollbacks.iter().map(|b| b.character.as_str()).collect();
    assert_eq!(chars, ["Ann", "Bob"]);
}

#[test]
fn engine_error_keeps_the_journal() {
    let f = fixture("error", no_spots());
    f.start(12, GameKind::PlayerHosted, 0);
    f.seat(12, 1, "Ann", true, b"a");
    f.audits.game_ended(12, 0).unwrap();
    let r = f
        .audits
        .run_next(
            &engine(12, Err("the game ran on other game data".into())),
            &MemStore::default(),
            1,
        )
        .unwrap()
        .unwrap();
    assert_eq!(r.state, AuditState::Error);
    assert_eq!(
        f.audits.audit(r.audit).unwrap().detail,
        "the game ran on other game data"
    );
    assert!(f.audits.prune(30 * DAY).unwrap().is_empty());
    assert!(f.audits.game(12).unwrap().journal.exists());
}

#[test]
fn journals_kept_seven_days_unless_held() {
    let f = fixture("retention", no_spots());
    let store = MemStore::default();
    // 1: nothing holds it. 2: a report holds it until its audit passes.
    // 3: the owner holds it. 4: still running.
    for g in 1..=4 {
        f.start(g, GameKind::Official, 0);
        f.seat(g, u64::from(g), "Ann", false, b"a");
    }
    for g in 1..=3 {
        f.audits.game_ended(g, DAY).unwrap();
    }
    f.audits.report_game(2, "acct", 3, "it froze", DAY).unwrap();
    f.audits.hold(3, true).unwrap();
    let journal = |g| f.audits.game(g).unwrap().journal;

    // Inside the window nothing goes.
    assert!(f.audits.prune(7 * DAY).unwrap().is_empty());
    assert_eq!(f.audits.prune(8 * DAY).unwrap(), [1]);
    assert!(!journal(1).exists());
    assert!(journal(2).exists() && journal(3).exists() && journal(4).exists());
    assert_eq!(f.audits.seats(1).unwrap()[0].joined, None);
    assert!(matches!(
        f.audits.request(1, "owner", 8 * DAY),
        Err(AuditError::State(_))
    ));

    f.audits
        .run_next(&engine(2, Ok(matched(&[(2, 0, None)]))), &store, 8 * DAY)
        .unwrap()
        .unwrap();
    assert_eq!(f.audits.prune(8 * DAY).unwrap(), [2]);
    f.audits.hold(3, false).unwrap();
    assert_eq!(f.audits.prune(8 * DAY).unwrap(), [3]);
    assert!(journal(4).exists());
}

#[test]
fn reports_are_kept_with_the_game() {
    let f = fixture("reports", no_spots());
    f.start(5, GameKind::PlayerHosted, 0);
    let long = "x".repeat(300);
    f.audits.report_game(5, "acct-1", 1, &long, 1).unwrap();
    f.audits
        .report_game(5, "acct-1", 4, "changed my mind", 2)
        .unwrap();
    f.audits.report_game(5, "acct-2", 2, "", 3).unwrap();
    let r = f.audits.reports_of(5).unwrap();
    assert_eq!(r.len(), 2);
    assert_eq!((r[1].reason, r[1].text.as_str()), (4, "changed my mind"));
    assert!(matches!(
        f.audits.report_game(99, "acct-1", 1, "", 1),
        Err(AuditError::NotFound(_))
    ));
}

#[test]
fn config_keys() {
    let cfg = AuditConfig::parse(
        "# main server\nlisten = 0.0.0.0:6112\naudits = on\nspot_official_pct = 2\n\
         spot_hosted_pct = 10\njournal_keep_days = 14\naudit_rollback = automatic\n",
    )
    .unwrap();
    assert_eq!(cfg.spot_official_pct, 2);
    assert_eq!(cfg.spot_hosted_pct, 10);
    assert_eq!(cfg.journal_keep_days, 14);
    assert_eq!(cfg.rollback, RollbackMode::Automatic);
    let d = AuditConfig::default();
    assert_eq!(
        (
            d.spot_official_pct,
            d.spot_hosted_pct,
            d.journal_keep_days,
            d.rollback
        ),
        (1, 5, 7, RollbackMode::OwnerConfirms)
    );
    assert!(AuditConfig::parse("spot_hosted_pct = 101").is_err());
    assert!(AuditConfig::parse("audit_rollback = maybe").is_err());
}

#[test]
fn database_file_reopens_with_its_state() {
    let dir = std::env::temp_dir().join(format!("d2-audit-file-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("audits.sqlite");
    let reports = dir.join("audits");
    let j = Path::new("journal.d2j.gz");
    {
        let a = Audits::open(&db, &reports, no_spots()).unwrap();
        a.game_started(
            &GameStart {
                game: 1,
                realm: "d2rust".into(),
                node: "fi-1".into(),
                kind: GameKind::Official,
            },
            j,
            0,
        )
        .unwrap();
        a.request(1, "owner", 1).unwrap();
    }
    let a = Audits::open(&db, &reports, no_spots()).unwrap();
    assert_eq!(a.audits(Some(AuditState::Queued)).unwrap().len(), 1);
    drop(a);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An engine that dies mid-run, as a crashed main server would.
struct Panics;

impl d2_audit::ReplayEngine for Panics {
    fn replay(&self, _: &d2_audit::engine::ReplayJob<'_>) -> Result<ReplayOutcome, String> {
        panic!("the main server stopped");
    }
}

#[test]
fn interrupted_audit_goes_back_in_the_queue() {
    let f = fixture("interrupted", no_spots());
    f.start(13, GameKind::PlayerHosted, 0);
    f.seat(13, 1, "Ann", true, b"a");
    f.audits.game_ended(13, 1).unwrap();
    let store = MemStore::default();
    let crashed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        f.audits.run_next(&Panics, &store, 2)
    }));
    assert!(crashed.is_err());
    assert_eq!(
        f.audits.audit_of_game(13).unwrap().state,
        AuditState::Running
    );
    assert_eq!(f.audits.recover_interrupted().unwrap(), 1);
    let r = f
        .audits
        .run_next(&engine(13, Ok(matched(&[(1, 0, None)]))), &store, 3)
        .unwrap()
        .unwrap();
    assert_eq!(r.state, AuditState::Passed);
}
