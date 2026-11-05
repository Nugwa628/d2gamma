//! A `--game-dir` that is not an install stops the game with an error
//! naming the folder (exit code 3), instead of playing another install.

use std::path::PathBuf;
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let p = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn a_mistyped_game_dir_is_an_error_naming_it() {
    let dir = scratch("game_dir_typo");
    let not_install = dir.join("empty");
    std::fs::create_dir_all(&not_install).unwrap();
    for (game_dir, why) in [
        (dir.join("no-such-folder"), "no such folder"),
        (not_install, "no d2data.mpq"),
    ] {
        let error_file = dir.join("error.txt");
        let _ = std::fs::remove_file(&error_file);
        let out = Command::new(env!("CARGO_BIN_EXE_D2Gamma-Game"))
            .arg("--error-file")
            .arg(&error_file)
            .args(["--mute", "play", "--game-dir"])
            .arg(&game_dir)
            .env("D2RUST_LOG_DIR", dir.join("logs"))
            .env("D2_GAME_DIR", &dir)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(3), "{out:?}");
        let reason = std::fs::read_to_string(&error_file).unwrap();
        assert!(
            reason.contains(&*game_dir.to_string_lossy()) && reason.contains(why),
            "{reason}"
        );
    }
}
