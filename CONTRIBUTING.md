# Contributing to D2Gamma

D2Gamma is Diablo II: Lord of Destruction rebuilt in Rust. The player brings their own original LoD files; the game
reads them at run time. Before any architectural change, read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

Players see the game as **D2Gamma**: the exe `D2Gamma-Game.exe`, the window title and version line
(`launch::NAME`), and the player's folder `Documents\D2Gamma`. Crate names, code identifiers, `D2RUST_*`
environment variables, file formats and the multiplayer protocol ids keep the earlier name, d2rust.

## The code describes itself

- Comments and docs explain what the code does and why, in game and design terms. They don't cite addresses in
  the original executable.
- Changes, fixes and mods are judged against how D2Gamma plays today. Mods are design choices, made on purpose.
- Some older comments still carry `// Spec:` headers or `§` references. Don't add new ones; when you touch such a
  comment, rewrite it in D2Gamma's own terms.

## Hard rules

Code comments cite these by number ("rule 6").

1. **Clean room: no Blizzard files.** Executables, DLLs, MPQs, saves, extracted assets and full extracted tables
   never go in this repository, in their original form or in any re-encoding a process could rebuild them from.
   Never `include_bytes!` / `include_str!` a game file. Game files always come from the player's own install at
   run time. Our own measured numbers (offsets, counts, ids, timings), like the tables in `tables/`, are allowed:
   they hold no Blizzard text or art.
2. **No decompiled code.** Rust code is written in our own words and structure, never pasted or line-translated
   from decompiler output. Every module starts with a comment saying what part of the game it is and the rules it
   follows.
3. **License hygiene.** No third-party code is adapted. GPL projects (OpenDiablo2, OpenD2) may be read for
   understanding only; never copy their code. [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) holds the Blizzard
   trademark statement and the licences of the fonts the game ships (`crates/d2-client/fonts/`).
4. **Bevy is for the client only.** `d2-client` is a Bevy app. Game logic never lives in Bevy systems, components
   or resources.
5. **Layering.** `d2-sim`, `d2-data`, `d2-formats`, `d2-net` and `d2-server` must not depend on Bevy. The client
   talks to the game only through `d2-client::bridge`. `tools/depcheck` checks the crate dependency rules.
6. **Determinism in `d2-sim`.** No I/O, no wall-clock time, no global or thread RNG, no `HashMap` iteration
   affecting outcomes, no floating point in game logic (integer / fixed-point math). All randomness flows through
   the seeded game RNG, in a fixed draw order. `crates/d2-sim/clippy.toml` and `tools/depcheck` enforce it.
7. **Server-authoritative.** Even single player runs as a local in-process server. The client never decides
   outcomes (damage, drops, item stats, movement validity); it sends intents and renders state. Every decoder of
   outside input (messages, saves, mod packs) must refuse bad input without panicking; property tests and the
   fuzz targets in `fuzz/` check it.
8. **Original behaviour is the default.** Changes to how the game plays go in data layers or behind explicit rule
   switches, never as edits to the original-behaviour code paths. Mod data is expressed as patches applied to the
   player's own tables at load time; full modified Blizzard tables never go in the repository or a release.
9. **The file check confirms the player has the game.** It never compares an install against fixed hashes and
   never scans beyond the game folder.

## Conventions

- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace` must pass.
- Bevy is pinned to one version in the workspace `Cargo.toml`. Upgrades are their own change and touch only
  `d2-client`.
- `thiserror` in libraries, `anyhow` in binaries and tools.
- Binary parsing: explicit little-endian reads; no `unsafe` transmutes of file data.
- Tests that need game files are `#[ignore]` and read `D2_GAME_DIR`; the default test run needs no game files.
- Every persisted format (saves, protocol, caches, settings) has a version number from its first version.
- Comments state what the code does and the game rule it follows, in plain words; no history in code.
- One small change per pull request, with what changed and how it was tested.

## Repository layout

```
docs/                 architecture, clean room, required files, playtest, local run, mods, test tools
tables/               small number tables the game logic embeds (include_str!)
crates/               the game, its servers and their libraries (see README.md)
tools/                developer tools: d2-convert, mpq-tool, d2s-tool, data-tool, depcheck, img-diff,
                      autoplay, perf, playcheck, soak, scenarios, mp-test, local servers
fuzz/                 cargo-fuzz targets (a separate workspace)
examples/mod-game/    a small mod game
game/                 your local Diablo II install for development (gitignored, never committed)
```
