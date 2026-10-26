//! The binary: `--check` reads the config, opens the folders and the log;
//! bad configs and missing game data fail with a message; the offline
//! admin commands open the accounts.

use std::path::PathBuf;
use std::process::{Command, Output};

fn realm() -> Command {
    Command::new(env!("CARGO_BIN_EXE_d2rust-realm"))
}

/// A new empty folder for one test.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("d2rust-realm-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(cfg: &std::path::Path) -> Output {
    realm()
        .arg("--config")
        .arg(cfg)
        .arg("--check")
        .output()
        .unwrap()
}

#[test]
fn check_opens_the_folders_and_logs() {
    let dir = temp_dir("check");
    std::fs::create_dir_all(dir.join("d2")).unwrap();
    let cfg = dir.join("d2rust-realm.cfg");
    std::fs::write(&cfg, "listen = 127.0.0.1:7000\ngame_data = d2\n").unwrap();
    let out = check(&cfg);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("127.0.0.1:7000"), "{stdout}");
    // The accounts' data folder (database and characters) is opened.
    assert!(dir.join("data").is_dir(), "{stdout}");
    let log = std::fs::read_to_string(dir.join("logs").join("d2rust-realm.log")).unwrap();
    assert!(log.contains("listen 127.0.0.1:7000"), "{log}");
}

#[test]
fn missing_game_data_fails() {
    let dir = temp_dir("nodata");
    let cfg = dir.join("d2rust-realm.cfg");
    std::fs::write(&cfg, "game_data = missing\n").unwrap();
    let out = check(&cfg);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("doesn't exist"));
}

#[test]
fn a_bad_config_names_its_line() {
    let dir = temp_dir("bad");
    let cfg = dir.join("d2rust-realm.cfg");
    std::fs::write(&cfg, "game_data = d2\nmax_players = 99\n").unwrap();
    let out = check(&cfg);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("line 2"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn bad_arguments_print_the_usage() {
    let out = realm().arg("--nope").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage"));
    // --set-progression needs a character and a number.
    let out = realm()
        .args(["--set-progression", "Nobody"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage"));
}

/// `--set-progression` is an offline admin command: it opens the accounts
/// and names the character it can't find.
#[test]
fn set_progression_names_the_character() {
    let dir = temp_dir("prog");
    let cfg = dir.join("d2rust-realm.cfg");
    std::fs::write(&cfg, "game_data = d2\n").unwrap();
    let out = realm()
        .arg("--config")
        .arg(&cfg)
        .args(["--set-progression", "Nobody", "5"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("no character named"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `--node-name` sets the name players see a game server by, without
/// adding it again; `--list-nodes` shows it. An unknown id is refused.
#[test]
fn node_name_renames_a_game_server_in_place() {
    let dir = temp_dir("nodename");
    let cfg = dir.join("d2rust-realm.cfg");
    std::fs::write(&cfg, "listen = 127.0.0.1:7100\ngame_data = d2\n").unwrap();
    let run = |args: &[&str]| {
        let out = realm()
            .arg("--config")
            .arg(&cfg)
            .args(args)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), text)
    };
    let (ok, text) = run(&[
        "--add-node",
        "n2",
        "--name",
        "DEVPC",
        "--location",
        "EU",
        "--realm",
        "d2rust",
    ]);
    assert!(ok, "{text}");
    let (ok, text) = run(&["--list-nodes"]);
    assert!(ok && text.contains("shown as \"EU\""), "{text}");
    let (ok, text) = run(&["--node-name", "n2", "JP South"]);
    assert!(ok && text.contains("shows as \"JP South\""), "{text}");
    let (ok, text) = run(&["--list-nodes"]);
    assert!(ok, "{text}");
    assert!(text.contains("shown as \"JP South\""), "{text}");
    // What it was added with stays.
    assert!(text.contains("name \"DEVPC\", location \"EU\""), "{text}");
    let list = std::fs::read_to_string(dir.join("data").join("nodes.toml")).unwrap();
    assert!(list.contains("display_name = \"JP South\""), "{list}");
    assert!(list.contains("cert_sha256"), "{list}");
    let (ok, text) = run(&["--node-name", "n9", "Finland"]);
    assert!(!ok && text.contains("no game server"), "{text}");
    let out = realm().args(["--node-name", "n2"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2), "a name is needed");
}

/// A second realm (`game = mymod`) takes game servers for its own realm
/// only, writes them to its own list, and the first realm still takes
/// `d2rust`: `--add-node` checks against the configured game.
#[test]
fn add_node_checks_against_the_configured_game() {
    let dir = temp_dir("game");
    let add = |cfg_text: &str, folder: &str, game: &str| {
        let sub = dir.join(folder);
        std::fs::create_dir_all(&sub).unwrap();
        let cfg = sub.join("d2rust-realm.cfg");
        std::fs::write(
            &cfg,
            format!("listen = 127.0.0.1:7200\ngame_data = d2\n{cfg_text}"),
        )
        .unwrap();
        let out = realm()
            .arg("--config")
            .arg(&cfg)
            .args([
                "--add-node",
                "n1",
                "--name",
                "N",
                "--location",
                "EU",
                "--realm",
            ])
            .arg(game)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (
            out.status.success(),
            text,
            sub.join("data").join("nodes.toml"),
        )
    };
    let (ok, text, list) = add("game = mymod\nmod_label = My Mod\n", "mymod", "mymod");
    assert!(ok, "{text}");
    assert!(std::fs::read_to_string(list)
        .unwrap()
        .contains("game = \"mymod\""));
    let (ok, text, list) = add("game = mymod\n", "mymod-wrong", "d2rust");
    assert!(!ok && text.contains("runs the mod realm mymod"), "{text}");
    assert!(!list.exists(), "a refused game server is not listed");
    let (ok, text, _) = add("", "plain-wrong", "mymod");
    assert!(!ok && text.contains("runs the mod realm d2rust"), "{text}");
    let (ok, text, list) = add("", "plain", "d2rust");
    assert!(ok, "{text}");
    assert!(std::fs::read_to_string(list)
        .unwrap()
        .contains("game = \"d2rust\""));
}

/// `--backup DIR` copies the stopped main server's characters.
#[test]
fn backup_copies_a_stopped_main_server() {
    let dir = temp_dir("backup");
    let cfg = dir.join("d2rust-realm.cfg");
    std::fs::write(&cfg, "game_data = d2\n").unwrap();
    let dest = dir.join("before-update");
    let out = realm()
        .arg("--config")
        .arg(&cfg)
        .arg("--backup")
        .arg(&dest)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("backed up 0 characters"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(dest.is_dir());
}

#[test]
fn version_prints_the_game_version() {
    let out = realm().arg("--version").output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success());
    let v = text.trim().strip_prefix("d2rust-realm ").expect("the name");
    assert_eq!(v.split('.').count(), 4, "{text}");
}
