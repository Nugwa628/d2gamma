use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use d2_net::msg::node::{update_phase, RealmToNode};

use super::*;
use crate::log::ServerLog;

fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2rust-realm-upgrade")
        .join(format!("{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const MIN: u64 = 60_000;

#[test]
fn schedule_marks_ten_minutes() {
    let s: Vec<u64> = vec![10, 5, 4, 3, 2, 1]
        .into_iter()
        .map(|m| m * MIN)
        .collect();
    let mut want = s;
    want.push(30_000);
    assert_eq!(marks(10, MIN), want);
    assert_eq!(marks(10, MIN).len(), 7);
}

#[test]
fn schedule_marks_three_minutes() {
    assert_eq!(marks(3, MIN), vec![3 * MIN, 2 * MIN, MIN, 30_000]);
    // A longer countdown starts with its own length.
    assert_eq!(marks(15, MIN)[..2], [15 * MIN, 10 * MIN]);
    // A test's short minute: the same marks, scaled.
    assert_eq!(marks(2, 400), vec![800, 400, 200]);
}

#[test]
fn schedule_marks_nothing_at_once() {
    assert!(marks(0, MIN).is_empty());
}

#[test]
fn lines_say_minutes_and_seconds() {
    assert_eq!(
        line(10 * MIN, MIN),
        "Restarting for an update in 10 minutes"
    );
    assert_eq!(line(MIN, MIN), "Restarting for an update in 1 minute");
    assert_eq!(line(30_000, MIN), "Restarting for an update in 30 seconds");
    // A test's minute says the same words.
    assert_eq!(line(4 * 400, 400), "Restarting for an update in 4 minutes");
    assert_eq!(line(200, 400), "Restarting for an update in 30 seconds");
}

#[test]
fn a_request_file_reads() {
    let r = Request::parse_file("version=0.1.3.0\nminutes=3\n", false).unwrap();
    assert_eq!(r, Request::new("0.1.3.0", 3, MIN).unwrap());
    let r = Request::parse_file("version = 0.1.3.0\n", false).unwrap();
    assert_eq!(r.minutes, DEFAULT_MINUTES);
    // A test's minute counts on a test setup only.
    let t = "version=0.1.3.0\nminutes=2\nminute_ms=400\n";
    assert_eq!(Request::parse_file(t, true).unwrap().minute_ms, 400);
    assert_eq!(Request::parse_file(t, false).unwrap().minute_ms, MIN);
    for bad in [
        "",
        "minutes=3",
        "version=v1",
        "version=0.1.3.0\nminutes=x",
        "version=0.1.3.0\nminutes=99999",
        "version=0.1.3.0\nhurry=yes",
        "version 0.1.3.0",
    ] {
        assert!(Request::parse_file(bad, true).is_err(), "{bad:?}");
    }
}

fn texts(actions: &[Action]) -> Vec<String> {
    actions
        .iter()
        .filter_map(|a| a.text().map(str::to_owned))
        .collect()
}

fn phases(actions: &[Action]) -> Vec<u8> {
    actions
        .iter()
        .filter_map(|a| match a.to_nodes() {
            Some(RealmToNode::Update { phase, .. }) => Some(phase),
            _ => None,
        })
        .collect()
}

/// Every tick of a countdown from `from_ms` to `until_ms` after its start,
/// every 50 ms.
fn run(up: &Upgrade, t0: Instant, from_ms: u64, until_ms: u64, linked: &[String]) -> Vec<Action> {
    let mut out = Vec::new();
    let mut ms = from_ms;
    while ms <= until_ms {
        out.extend(up.tick(t0 + Duration::from_millis(ms), linked));
        ms += 50;
    }
    out
}

#[test]
fn the_countdown_reaches_every_game_server_and_global() {
    let up = Upgrade::new("0.1.2.0");
    let t0 = Instant::now();
    let nodes = vec!["n1".to_owned(), "n2".to_owned()];
    up.start(&Request::new("0.1.3.0", 10, 400).unwrap(), t0, true)
        .unwrap();
    assert_eq!(up.phase(), Phase::Countdown);
    let before = run(&up, t0, 0, 4000 - 50, &nodes);
    assert_eq!(
        texts(&before),
        [
            "Restarting for an update in 10 minutes",
            "Restarting for an update in 5 minutes",
            "Restarting for an update in 4 minutes",
            "Restarting for an update in 3 minutes",
            "Restarting for an update in 2 minutes",
            "Restarting for an update in 1 minute",
            "Restarting for an update in 30 seconds",
        ]
    );
    assert!(phases(&before)
        .iter()
        .all(|&p| p == update_phase::COUNTDOWN));
    // Each line goes out at its time: the 5 minute one at 5 test minutes.
    let up2 = Upgrade::new("0.1.2.0");
    up2.start(&Request::new("0.1.3.0", 10, 400).unwrap(), t0, true)
        .unwrap();
    assert_eq!(texts(&run(&up2, t0, 0, 1950, &nodes)).len(), 1);
    assert_eq!(
        texts(&run(&up2, t0, 2000, 2000, &nodes)),
        ["Restarting for an update in 5 minutes"]
    );
    // Zero: the restart, and the main server waits for its game servers.
    let zero = up.tick(t0 + Duration::from_millis(4000), &nodes);
    assert_eq!(texts(&zero), [NOW_TEXT]);
    assert_eq!(phases(&zero), [update_phase::RESTART]);
    assert_eq!(up.phase(), Phase::Restart);
    up.node_restarting("n1");
    assert!(up.tick(t0 + Duration::from_millis(4100), &nodes).is_empty());
    up.node_restarting("n2");
    let swap = up.tick(t0 + Duration::from_millis(4200), &nodes);
    assert_eq!(
        swap,
        [Action::Swap {
            version: "0.1.3.0".into()
        }]
    );
    assert_eq!(up.phase(), Phase::Swap);
    assert!(up.json(t0).contains("\"phase\": \"swap\""));
}

#[test]
fn a_late_tick_speaks_only_the_latest_mark() {
    let up = Upgrade::new("0.1.2.0");
    let t0 = Instant::now();
    up.start(&Request::new("0.1.3.0", 10, 400).unwrap(), t0, true)
        .unwrap();
    up.tick(t0, &[]);
    // The server was busy from 10 to 3.5 minutes left.
    let late = up.tick(t0 + Duration::from_millis(6 * 400 + 200), &[]);
    assert_eq!(texts(&late), ["Restarting for an update in 4 minutes"]);
}

#[test]
fn a_cancel_stops_it() {
    let up = Upgrade::new("0.1.2.0");
    let t0 = Instant::now();
    assert!(up.cancel("x").is_err());
    up.start(&Request::new("0.1.3.0", 3, 400).unwrap(), t0, true)
        .unwrap();
    // Another upgrade can't start over it.
    assert!(up
        .start(&Request::new("0.1.4.0", 3, 400).unwrap(), t0, true)
        .is_err());
    run(&up, t0, 0, 500, &[]);
    let c = up.cancel("asked").unwrap();
    assert_eq!(c.text(), Some(CANCEL_TEXT));
    assert!(matches!(
        c.to_nodes(),
        Some(RealmToNode::Update { phase: update_phase::CANCEL, ref version, .. }) if version == "0.1.3.0"
    ));
    assert_eq!(up.phase(), Phase::Idle);
    assert!(run(&up, t0, 550, 3000, &[]).is_empty());
    assert!(up.json(t0).contains("\"phase\": \"cancelled\""));
    // Once the restart began, it can't be called off.
    up.start(&Request::new("0.1.3.0", 0, 400).unwrap(), t0, true)
        .unwrap();
    up.tick(t0, &["n1".to_owned()]);
    assert_eq!(up.phase(), Phase::Restart);
    assert!(up.cancel("late").is_err());
}

#[test]
fn the_running_version_is_no_upgrade() {
    let up = Upgrade::new("0.1.2.0");
    let req = Request::new("0.1.2.0", 3, MIN).unwrap();
    assert!(up.start(&req, Instant::now(), true).is_err());
}

#[test]
fn zero_waits_for_the_main_servers_package() {
    let up = Upgrade::new("0.1.2.0");
    let t0 = Instant::now();
    up.start(&Request::new("0.1.3.0", 1, 400).unwrap(), t0, false)
        .unwrap();
    let all = run(&up, t0, 0, 1000, &[]);
    assert!(!phases(&all).contains(&update_phase::RESTART));
    assert_eq!(up.phase(), Phase::Countdown);
    up.set_ready();
    let zero = up.tick(t0 + Duration::from_millis(1050), &[]);
    // Nobody to wait for: the restart and the swap at once.
    assert_eq!(phases(&zero), [update_phase::RESTART]);
    assert!(matches!(zero.last(), Some(Action::Swap { .. })));

    // Never ready: called off.
    let up = Upgrade::new("0.1.2.0");
    up.start(&Request::new("0.1.3.0", 0, 400).unwrap(), t0, false)
        .unwrap();
    assert!(up.tick(t0, &[]).is_empty());
    let off = up.tick(t0 + READY_WAIT, &[]);
    assert_eq!(texts(&off), [CANCEL_TEXT]);
    assert_eq!(up.phase(), Phase::Idle);
}

#[test]
fn the_restart_waits_at_most_a_minute() {
    let up = Upgrade::new("0.1.2.0");
    let t0 = Instant::now();
    let nodes = vec!["n1".to_owned(), "n2".to_owned()];
    up.start(&Request::new("0.1.3.0", 0, 400).unwrap(), t0, true)
        .unwrap();
    up.tick(t0, &nodes);
    // n2's link is gone: only n1 is waited for, and it never answers.
    let gone = vec!["n1".to_owned()];
    assert!(up.tick(t0 + Duration::from_secs(30), &gone).is_empty());
    let swap = up.tick(t0 + RESTART_WAIT, &gone);
    assert!(matches!(swap[..], [Action::Swap { .. }]));
}

#[test]
fn a_new_link_hears_the_countdown() {
    let up = Upgrade::new("0.1.2.0");
    let t0 = Instant::now();
    assert_eq!(up.for_new_link(t0), None);
    up.start(&Request::new("0.1.3.0", 3, MIN).unwrap(), t0, true)
        .unwrap();
    assert_eq!(
        up.for_new_link(t0 + Duration::from_secs(60)),
        Some(RealmToNode::Update {
            phase: update_phase::COUNTDOWN,
            version: "0.1.3.0".into(),
            seconds: 120,
            text: String::new(),
        })
    );
}

#[test]
fn another_version_catches_up_but_not_the_one_restarted_for() {
    let up = Upgrade::new("0.1.2.0");
    let t0 = Instant::now();
    let catch = |m: Result<RealmToNode, String>| match m {
        Ok(RealmToNode::Update { phase, version, .. }) => Some((phase, version)),
        _ => None,
    };
    assert_eq!(
        catch(up.catch_up("0.1.1.0")),
        Some((update_phase::CATCH_UP, "0.1.2.0".to_owned()))
    );
    up.start(&Request::new("0.1.3.0", 0, 400).unwrap(), t0, true)
        .unwrap();
    up.tick(t0, &["n1".to_owned()]);
    let refused = up.catch_up("0.1.3.0").unwrap_err();
    assert!(refused.contains("restarting for 0.1.3.0"), "{refused}");
    assert!(up.catch_up("0.1.1.0").is_ok());
}

#[derive(Default)]
struct Heard {
    global: Mutex<Vec<String>>,
    nodes: Mutex<Vec<RealmToNode>>,
}

impl Outlet for Heard {
    fn global(&self, text: &str) {
        self.global.lock().unwrap().push(text.to_owned());
    }
    fn nodes(&self, m: &RealmToNode) {
        self.nodes.lock().unwrap().push(m.clone());
    }
    fn linked(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The request files drive it: a request, `ready`, the lines in Global and
/// to the game servers, and `upgrade.json` at `swap`.
#[test]
fn the_request_files_drive_it() {
    let dir = temp("drive");
    let (folder, logs) = (dir.join("server"), dir.join("logs"));
    std::fs::create_dir_all(folder.join("run")).unwrap();
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(
        folder.join("run").join(REQUEST_FILE),
        "version=0.1.3.0\nminutes=1\nminute_ms=400\n",
    )
    .unwrap();
    std::fs::write(folder.join("run").join(READY_FILE), "0.1.3.0\n").unwrap();
    let log = Arc::new(ServerLog::open_named(&logs, "test.log").unwrap());
    let up = Arc::new(Upgrade::new("0.1.2.0"));
    let heard = Arc::new(Heard::default());
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let state = logs.join(STATE_FILE);
    rt.block_on(async {
        let driver = tokio::spawn(drive(
            up.clone(),
            heard.clone(),
            folder.clone(),
            logs.clone(),
            true,
            log,
        ));
        for _ in 0..40 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let json = std::fs::read_to_string(&state).unwrap_or_default();
            if json.contains("\"phase\": \"swap\"") {
                break;
            }
        }
        driver.abort();
    });
    assert_eq!(up.phase(), Phase::Swap);
    assert!(!folder.join("run").join(REQUEST_FILE).exists());
    assert!(!folder.join("run").join(READY_FILE).exists());
    assert_eq!(
        *heard.global.lock().unwrap(),
        [
            "Restarting for an update in 1 minute",
            "Restarting for an update in 30 seconds",
            NOW_TEXT
        ]
    );
    assert_eq!(heard.nodes.lock().unwrap().len(), 3);
    let json = std::fs::read_to_string(&state).unwrap();
    assert!(json.contains("\"version\": \"0.1.3.0\""), "{json}");
}
