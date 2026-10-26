//! `bake-backdrops` on the user's install (`app::backdrop_bake`): the
//! game's own exe, as the client starts it, writes one 1280 × 720 PNG per
//! lobby background into `--out` and reports each with its time and size.
//! The stills go to `D2_STILLS` (default: a temp folder), where the
//! lobby's shots (`game_lobby_picture_shots`) can take them. Ignored:
//! needs `D2_GAME_DIR`.

use std::path::PathBuf;
use std::time::Instant;

use d2_client::app::backdrop_bake::{missing, read_png, still_path};
use d2_client::ui::front_end::screens::online::backdrop::Backdrop;

#[test]
#[ignore = "needs D2_GAME_DIR (the user's install)"]
fn the_bake_writes_a_still_per_background() {
    let game_dir = PathBuf::from(std::env::var("D2_GAME_DIR").expect("D2_GAME_DIR"));
    let out = std::env::var_os("D2_STILLS")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("backdrop-stills"));
    let _ = std::fs::remove_dir_all(&out);
    let t = Instant::now();
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_D2Gamma-Game"))
        .args(["--mute", "bake-backdrops", "--game-dir"])
        .arg(&game_dir)
        .arg("--out")
        .arg(&out)
        .output()
        .expect("the game's exe runs");
    let stdout = String::from_utf8_lossy(&run.stdout);
    println!("{stdout}");
    assert!(
        run.status.success(),
        "bake-backdrops: {}\n{}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    println!("bake-backdrops: {:.1} s in all", t.elapsed().as_secs_f64());
    assert!(missing(&out).is_empty(), "every still is there");
    for b in Backdrop::ALL {
        let path = still_path(&out, b);
        let (rgba, size) = read_png(&path).unwrap();
        assert_eq!(size, (1280, 720), "{}", b.key());
        let lit = rgba.chunks(4).filter(|p| p[0] | p[1] | p[2] > 8).count();
        assert!(
            lit > rgba.len() / 4 / 4,
            "{}: the place is drawn ({lit} lit pixels)",
            b.key()
        );
        assert!(
            stdout.contains(&format!("bake-backdrops: still {} ", b.key())),
            "{} reported",
            b.key()
        );
        println!(
            "{}: {} bytes",
            b.key(),
            std::fs::metadata(&path).unwrap().len()
        );
    }
    assert!(stdout.contains("bake-backdrops: done 6"));
    // A second run on complete stills does nothing.
    let again = std::process::Command::new(env!("CARGO_BIN_EXE_D2Gamma-Game"))
        .args(["bake-backdrops", "--game-dir"])
        .arg(&game_dir)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert!(again.status.success());
    assert!(String::from_utf8_lossy(&again.stdout).contains("bake-backdrops: todo 0"));
}
