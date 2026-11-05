//! The converter must use the complete headless checker, with no fallback.
#[cfg(unix)]
#[test]
fn publication_forwards_arguments_and_exit_code_to_configured_game() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("convert-package-command-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let game = root.join("game fixture");
    std::fs::write(&game, "#!/bin/sh\nprintf '<%s>\\n' \"$@\"\nexit 7\n").unwrap();
    std::fs::set_permissions(&game, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_d2-convert"))
        .args(["package-mod", "my mod", "--base", "my native", "--game"])
        .arg(&game)
        .args(["--out", "my pack.d2pack", "--report", "my report"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(7));
    assert_eq!(String::from_utf8(output.stdout).unwrap(),"<package-mod>\n<my mod>\n<--base>\n<my native>\n<--out>\n<my pack.d2pack>\n<--report>\n<my report>\n");
    let missing = std::process::Command::new(env!("CARGO_BIN_EXE_d2-convert"))
        .args(["package-mod", "my mod", "--game"])
        .arg(root.join("missing"))
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8(missing.stderr)
        .unwrap()
        .contains("could not start the full checker"));
    std::fs::remove_dir_all(root).unwrap();
}
