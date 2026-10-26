use std::sync::Arc;

use d2_net::msg::node::{update_phase, update_state};
use d2_service::log::ServerLog;

use super::*;

fn temp(test: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("d2rust-node-update")
        .join(format!("{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn updater(test: &str, fetcher: Fetcher) -> Arc<Updater> {
    let dir = temp(test);
    let log = Arc::new(ServerLog::open_named(&dir, "test.log").unwrap());
    Updater::new(fetcher, "0.1.2.0", &dir, log)
}

fn states(d: &[Do]) -> Vec<u8> {
    d.iter()
        .filter_map(|a| match a {
            Do::Report { state, .. } => Some(*state),
            _ => None,
        })
        .collect()
}

const LINE: &str = "Restarting for an update in 10 minutes";

#[test]
fn a_countdown_fetches_once() {
    let u = updater("once", Fetcher::Test);
    let first = u.on_update(update_phase::COUNTDOWN, "0.1.3.0", LINE);
    assert_eq!(
        first,
        [
            Do::Announce(LINE.into()),
            report("0.1.3.0", update_state::DOWNLOADING, ""),
            Do::Fetch("0.1.3.0".into()),
        ]
    );
    // The next lines only show; the fetch runs once.
    let next = u.on_update(update_phase::COUNTDOWN, "0.1.3.0", "in 5 minutes");
    assert_eq!(next, [Do::Announce("in 5 minutes".into())]);
    assert_eq!(
        u.fetched("0.1.3.0", Ok(())),
        [report("0.1.3.0", update_state::READY, "")]
    );
    // A failed fetch is tried again with the next line.
    let u = updater("again", Fetcher::Test);
    u.on_update(update_phase::COUNTDOWN, "0.1.3.0", "");
    let failed = u.fetched("0.1.3.0", Err("bad signature".into()));
    assert_eq!(
        failed,
        [report("0.1.3.0", update_state::FAILED, "bad signature")]
    );
    let again = u.on_update(update_phase::COUNTDOWN, "0.1.3.0", "");
    assert!(again.contains(&Do::Fetch("0.1.3.0".into())), "{again:?}");
}

#[test]
fn restart_saves_then_marks_the_swap() {
    let u = updater("restart", Fetcher::Test);
    u.on_update(update_phase::COUNTDOWN, "0.1.3.0", LINE);
    u.fetched("0.1.3.0", Ok(()));
    let zero = u.on_update(update_phase::RESTART, "0.1.3.0", "now");
    assert_eq!(
        zero,
        [
            Do::Announce("now".into()),
            Do::CloseGames,
            report("0.1.3.0", update_state::RESTARTING, ""),
            Do::Swap("0.1.3.0".into()),
        ]
    );
    // Not staged at zero: the games still save, the swap waits for the
    // fetch.
    let u = updater("late", Fetcher::Test);
    u.on_update(update_phase::COUNTDOWN, "0.1.3.0", "");
    let zero = u.on_update(update_phase::RESTART, "0.1.3.0", "");
    assert_eq!(
        zero,
        [
            Do::CloseGames,
            report("0.1.3.0", update_state::RESTARTING, "")
        ]
    );
    assert_eq!(
        u.fetched("0.1.3.0", Ok(())),
        [
            report("0.1.3.0", update_state::READY, ""),
            Do::Swap("0.1.3.0".into())
        ]
    );
}

#[test]
fn a_cancel_drops_the_staged_version() {
    let u = updater("cancel", Fetcher::Test);
    u.on_update(update_phase::COUNTDOWN, "0.1.3.0", "");
    u.fetched("0.1.3.0", Ok(()));
    let c = u.on_update(update_phase::CANCEL, "0.1.3.0", "The update was called off");
    assert_eq!(states(&c), [update_state::IDLE]);
    // A fetch that ends after the cancel changes nothing.
    assert!(u.fetched("0.1.3.0", Ok(())).is_empty());
}

#[test]
fn a_catch_up_fetches_and_swaps() {
    let u = updater("catch", Fetcher::Test);
    let c = u.on_update(update_phase::CATCH_UP, "0.1.1.0", "");
    assert_eq!(c.last(), Some(&Do::Fetch("0.1.1.0".into())));
    assert!(!c.iter().any(|d| matches!(d, Do::Announce(_))));
    // Refused again while fetching: nothing new.
    assert!(u
        .on_update(update_phase::CATCH_UP, "0.1.1.0", "")
        .is_empty());
    let done = u.fetched("0.1.1.0", Ok(()));
    assert_eq!(done.last(), Some(&Do::Swap("0.1.1.0".into())));
}

#[test]
fn without_an_updater_it_only_shows_and_saves() {
    let u = updater("off", Fetcher::Off);
    assert!(!u.managed());
    assert_eq!(
        u.on_update(update_phase::COUNTDOWN, "0.1.3.0", LINE),
        [Do::Announce(LINE.into())]
    );
    assert_eq!(
        u.on_update(update_phase::RESTART, "0.1.3.0", ""),
        [
            Do::CloseGames,
            report("0.1.3.0", update_state::RESTARTING, "")
        ]
    );
    assert!(u
        .on_update(update_phase::CATCH_UP, "0.1.3.0", "")
        .is_empty());
}

#[test]
fn its_own_version_is_not_fetched() {
    let u = updater("own", Fetcher::Test);
    assert!(u
        .on_update(update_phase::COUNTDOWN, "0.1.2.0", "")
        .is_empty());
}

#[test]
fn a_fetch_runs_the_update_command() {
    let dir = temp("command");
    let log = ServerLog::open_named(&dir, "test.log").unwrap();
    // A command that is not there fails with a reason.
    let missing = Fetcher::Run {
        words: vec!["no-such-update-command".into()],
        folder: dir.clone(),
    };
    assert!(fetch(&missing, "0.1.3.0", &log).is_err());
    assert_eq!(fetch(&Fetcher::Test, "0.1.3.0", &log), Ok(()));
    assert!(fetch(&Fetcher::Off, "0.1.3.0", &log).is_err());
}

#[cfg(unix)]
#[test]
fn a_fetch_takes_the_commands_answer() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp("script");
    let log = ServerLog::open_named(&dir, "test.log").unwrap();
    let script = dir.join("fake-ctl");
    std::fs::write(
        &script,
        "#!/bin/sh\n[ \"$1 $2\" = \"fetch 0.1.3.0\" ] && exit 0\necho \"no release $2\" >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let f = Fetcher::Run {
        words: vec!["fake-ctl".into()],
        folder: dir.clone(),
    };
    assert_eq!(fetch(&f, "0.1.3.0", &log), Ok(()));
    assert_eq!(fetch(&f, "0.1.4.0", &log), Err("no release 0.1.4.0".into()));
}
