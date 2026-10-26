//! Enforces the crate dependency rules from CONTRIBUTING.md (rule 5) by inspecting
//! each crate's full normal+build dependency tree with `cargo tree`.
//!
//! Usage: `cargo run -p depcheck`. Exits non-zero on any violation.

mod determinism;

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

/// Crates that must never pull in Bevy, directly or transitively.
const NO_BEVY: &[&str] = &[
    "d2-formats",
    "d2-data",
    "d2-sim",
    "d2-save-checks",
    "d2-proto",
    "d2-net",
    "d2-quic",
    "d2-online",
    "d2-server",
    "d2-service",
    "d2-accounts",
    "d2-games",
    "d2-journal",
    "d2-link",
    "d2rust-realm",
    "d2rust-node",
    "d2-verify",
    "d2-native",
    "d2-report",
    "d2-economy",
    "d2-shared-stash",
    "mp-test",
    "d2-tickets",
    "d2-item-registry",
    "d2-reach",
    "d2-audit",
    "d2-host-control",
    "d2-limits",
    "d2rust-audit",
];

/// (crate, forbidden dependency) pairs: layering rules beyond Bevy.
const FORBIDDEN: &[(&str, &str)] = &[
    // The sim does no I/O and knows nothing about transport or the client.
    ("d2-sim", "d2-net"),
    ("d2-sim", "d2-server"),
    ("d2-sim", "d2-client"),
    ("d2-sim", "d2-proto"),
    // No ambient randomness in the sim (rule 6): only the seeded RNG.
    ("d2-sim", "rand"),
    ("d2-sim", "getrandom"),
    // The server never depends on the client.
    ("d2-server", "d2-client"),
    // The server programs' shared libraries have no window, and the games
    // hold no accounts, no database and no character folder: a game server
    // links d2-games without any of them (multiplayer v2, N0a).
    ("d2-service", "d2-client"),
    ("d2-accounts", "d2-client"),
    ("d2-games", "d2-client"),
    ("d2-games", "d2-accounts"),
    ("d2-games", "rusqlite"),
    ("d2-games", "argon2"),
    ("d2-service", "d2-accounts"),
    ("d2-service", "d2-games"),
    ("d2-service", "rusqlite"),
    // The game-server link and the two programs (N0b): a game server holds
    // no accounts, no database and no character folder (`cargo tree -p
    // d2rust-node`), the main server runs no games, and neither has a window.
    ("d2-link", "d2-client"),
    ("d2-link", "d2-accounts"),
    ("d2-link", "rusqlite"),
    ("d2rust-node", "d2-client"),
    ("d2rust-node", "d2-accounts"),
    ("d2rust-node", "rusqlite"),
    ("d2rust-node", "argon2"),
    ("d2rust-realm", "d2-client"),
    ("d2rust-realm", "d2-games"),
    // The multiplayer test tool's bots are headless: they speak the wire
    // protocol, never the game client.
    ("mp-test", "d2-client"),
    // The online session library has no game code: the game and the test
    // bots share it.
    ("d2-online", "d2-sim"),
    ("d2-online", "d2-server"),
    ("d2-online", "d2-client"),
    // The journal is read and written by servers and audits, never the
    // game client's window code.
    ("d2-journal", "d2-client"),
    // The save checks are rules only: the main server links them, never a
    // game server or the client.
    ("d2-save-checks", "d2-server"),
    ("d2-save-checks", "d2-client"),
    // The report format is shared with the launcher and the report service.
    ("d2-report", "d2-client"),
    // Join tickets are checked on game servers, which hold no database.
    ("d2-tickets", "rusqlite"),
    // Audits read a replay's outcome through their engine seam; the game
    // itself stays out of the audit queue's library.
    ("d2-audit", "d2-server"),
    ("d2-audit", "d2-sim"),
    // The economy is storage and rules only: no game, no network, no client.
    ("d2-economy", "d2-client"),
    ("d2-economy", "d2-net"),
    ("d2-economy", "d2-server"),
    ("d2-economy", "d2-sim"),
    // "Host on my PC" starts the game server as its own process: the
    // client side holds no game-server, game or database code.
    ("d2-host-control", "d2-client"),
    ("d2-host-control", "d2-games"),
    ("d2-host-control", "d2-server"),
    ("d2-host-control", "d2-sim"),
    ("d2-host-control", "rusqlite"),
    // Rate limits run on game servers too (Windows and Linux): no database.
    ("d2-limits", "rusqlite"),
    // The shared stash stores are storage only: the game and the client
    // reach them through their traits.
    ("d2-shared-stash", "d2-client"),
    ("d2-shared-stash", "d2-server"),
    ("d2-shared-stash", "d2-sim"),
];

fn deps_of(krate: &str) -> Result<Vec<String>> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args([
            "tree",
            "-p",
            krate,
            "-e",
            "normal,build",
            "--prefix",
            "none",
        ])
        .args(["--format", "{p}"])
        .output()
        .with_context(|| format!("running cargo tree for {krate}"))?;
    if !out.status.success() {
        bail!(
            "cargo tree -p {krate} failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    // Lines look like "bevy v0.19.1" or "d2-data v0.0.0 (path)".
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_whitespace().next())
        .map(str::to_owned)
        .collect())
}

fn main() -> Result<()> {
    let mut violations = Vec::new();

    for krate in NO_BEVY {
        let deps = deps_of(krate)?;
        for dep in deps.iter().filter(|d| d.starts_with("bevy")) {
            violations.push(format!("{krate} depends on {dep} (Bevy is client-only)"));
        }
        for (_, bad) in FORBIDDEN.iter().filter(|(k, _)| k == krate) {
            if deps.iter().any(|d| d == bad) {
                violations.push(format!("{krate} must not depend on {bad}"));
            }
        }
    }

    // Hard rule 6: determinism lint over d2-sim's non-test sources.
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = here.join("../..");
    violations.extend(determinism::check(
        &root,
        &here.join("determinism-allow.txt"),
    )?);

    if violations.is_empty() {
        println!(
            "depcheck: OK ({} crates checked, d2-sim determinism lint clean)",
            NO_BEVY.len()
        );
        Ok(())
    } else {
        for v in &violations {
            eprintln!("depcheck: {v}");
        }
        bail!("{} dependency rule violation(s)", violations.len());
    }
}
