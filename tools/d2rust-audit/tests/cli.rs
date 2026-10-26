//! The tool on a test realm's data folder: a failed audit of a
//! player-hosted game (made with the library and a scripted engine) is
//! listed, shown, rolled back and undone through the binary, on real
//! character files.

use std::path::{Path, PathBuf};
use std::process::Command;

use d2_accounts::characters::CharacterStore as Folder;
use d2_audit::engine::{Divergence, ReplayOutcome, ScriptedEngine, SeatSpan};
use d2_audit::{AuditConfig, Audits, CharacterStore, GameKind, GameStart, Joined, SeatJoin};

fn tool(data: &Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_d2rust-audit"))
        .arg("--data")
        .arg(data)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

struct NoStore;

impl CharacterStore for NoStore {
    fn load(&self, c: &str) -> Result<Vec<u8>, String> {
        Err(format!("no {c}"))
    }
    fn store(&self, c: &str, _: &[u8]) -> Result<(), String> {
        Err(format!("no {c}"))
    }
}

fn realm(name: &str) -> PathBuf {
    let data = std::env::temp_dir().join(format!("d2rust-audit-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).unwrap();
    data
}

#[test]
fn lists_rolls_back_and_undoes() {
    let data = realm("rollback");
    let chars = Folder::open(data.join("characters").join("7")).unwrap();
    chars.create("Ann", b"Ann after the game").unwrap();
    {
        let audits = Audits::open(
            &data.join("audits.sqlite"),
            &data.join("audits"),
            AuditConfig::default(),
        )
        .unwrap();
        audits
            .game_started(
                &GameStart {
                    game: 42,
                    realm: "d2rust".into(),
                    node: "h-0123456789ab".into(),
                    kind: GameKind::PlayerHosted,
                },
                &data.join("journals/42.d2j.gz"),
                0,
            )
            .unwrap();
        audits
            .seat_joined(
                42,
                &SeatJoin {
                    lease: 5,
                    character: "Ann".into(),
                    host: true,
                    joined: Joined::Record(b"Ann as she joined".to_vec()),
                },
                0,
            )
            .unwrap();
        audits.game_ended(42, 1).unwrap();
        let engine = ScriptedEngine {
            outcomes: [(
                42,
                Ok(ReplayOutcome {
                    seats: vec![SeatSpan {
                        lease: 5,
                        joined: 0,
                        left: None,
                    }],
                    divergence: Some(Divergence {
                        tick: 3000,
                        last_good: 2400,
                        what: "state hash differs".into(),
                    }),
                    ..ReplayOutcome::default()
                }),
            )]
            .into(),
        };
        audits.run_next(&engine, &NoStore, 2).unwrap().unwrap();
    }

    let (ok, out, _) = tool(&data, &["list"]);
    assert!(ok);
    let row = out.lines().nth(1).unwrap_or_default();
    let cols: Vec<&str> = row.split_whitespace().collect();
    assert_eq!(cols[..4], ["1", "42", "hosted", "failed"], "{out}");
    assert!(row.ends_with("host's own character"), "{out}");
    let (ok, out, _) = tool(&data, &["list", "passed"]);
    assert!(ok && out.lines().count() == 1, "{out}");

    let (ok, out, _) = tool(&data, &["show", "1"]);
    assert!(ok && out.contains("| 1 | Ann | 5 | proposed |"), "{out}");
    let (ok, out, _) = tool(&data, &["findings"]);
    assert!(
        ok && out.contains("divergence") && out.contains("3000"),
        "{out}"
    );

    let (ok, out, err) = tool(&data, &["rollback", "1"]);
    assert!(ok, "{err}");
    assert!(
        out.contains("Ann is back to its record of game 42"),
        "{out}"
    );
    assert_eq!(chars.load("Ann").unwrap(), b"Ann as she joined");
    let (_, out, _) = tool(&data, &["rollbacks"]);
    assert!(out.contains("applied"), "{out}");

    let (ok, _, err) = tool(&data, &["undo", "1"]);
    assert!(ok, "{err}");
    assert_eq!(chars.load("Ann").unwrap(), b"Ann after the game");
    let (ok, _, err) = tool(&data, &["undo", "1"]);
    assert!(!ok && err.contains("undone, not applied"), "{err}");

    // The run says plainly that this build can't re-run journals yet.
    tool(&data, &["queue", "42"]);
    let (ok, _, err) = tool(&data, &["run"]);
    assert!(!ok && err.contains("no replay engine"), "{err}");
    let (_, out, _) = tool(&data, &["list", "queued"]);
    assert_eq!(out.lines().count(), 2, "{out}");

    let (ok, out, _) = tool(&data, &["watch"]);
    assert!(ok && out.contains("official games re-run: 0"), "{out}");
    let (ok, _, err) = tool(&data, &["frobnicate"]);
    assert!(!ok && err.contains("unknown command"), "{err}");
    let _ = std::fs::remove_dir_all(&data);
}
