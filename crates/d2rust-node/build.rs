//! The program's game version: the game's own (the `d2-client` crate's
//! version) plus the build number `$D2RUST_BUILD` (else 0), the same four
//! parts the game puts in its hello. Client and server must match exactly,
//! so both read the one place the version is written.

fn main() {
    let manifest = "../d2-client/Cargo.toml";
    println!("cargo:rerun-if-changed={manifest}");
    println!("cargo:rerun-if-env-changed=D2RUST_BUILD");
    println!("cargo:rerun-if-env-changed=D2RS_BUILD");
    let text = std::fs::read_to_string(manifest).expect("the game crate's manifest");
    let version = text
        .lines()
        .find_map(|l| {
            let v = l.strip_prefix("version")?.trim().strip_prefix('=')?.trim();
            Some(v.trim_matches('"').to_owned())
        })
        .expect("the game crate's version");
    let build: u16 = std::env::var("D2RUST_BUILD")
        .or_else(|_| std::env::var("D2RS_BUILD"))
        .ok()
        .and_then(|b| b.trim().parse().ok())
        .unwrap_or(0);
    let version = format!("{version}.{build}");
    println!("cargo:rustc-env=D2RUST_VERSION={version}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_version(&version);
    }
}

/// Keep the package's version readable in the PE even when optimized
/// String construction embeds its bytes in machine instructions.
fn windows_version(version: &str) {
    let parts = version.replace('.', ",");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = out.join("d2rust-node.rc");
    std::fs::write(
        &rc,
        format!(
            r#"1 VERSIONINFO
FILEVERSION {parts}
PRODUCTVERSION {parts}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Nugwa"
      VALUE "FileDescription", "D2Gamma game server"
      VALUE "FileVersion", "{version}"
      VALUE "ProductName", "D2Gamma"
      VALUE "ProductVersion", "{version}"
      VALUE "OriginalFilename", "d2rust-node.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        ),
    )
    .unwrap();
    embed_resource::compile_for(&rc, ["d2rust-node"], embed_resource::NONE)
        .manifest_required()
        .unwrap();
}
