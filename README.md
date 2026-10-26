# D2Gamma

D2Gamma is a rewrite of Diablo II: Lord of Destruction in Rust. It plays like the original game and modernizes the
engine underneath: a 60 Hz game loop, 16:9 and high resolutions, smooth animation, and one server design for both
single player and online play.

**You need your own copy of Diablo II: Lord of Destruction.** This repository has no game files. D2Gamma reads the
data archives (`d2data.mpq`, `d2exp.mpq`, ...) from your own install when it runs. See
[docs/REQUIRED_FILES.md](docs/REQUIRED_FILES.md).

> Status: early beta (0.1). Expect bugs.

## Features

- **The whole game:** five acts, every class, skills, items, quests, waypoints, towns, vendors, gambling and the
  Horadric Cube, all built from the original game data tables.
- **60 Hz game loop:** game logic runs 60 ticks per second instead of 25. Speeds and durations are converted from
  the game's own tables, so the game plays the same but responds faster and has finer attack-speed steps.
- **Smooth animation:** extra in-between animation frames are made from your game files once, at install, and
  then stored on disk. You can switch back to the original frames in Video options.
- **Modern display:** 16:9 and 4:3 views, sharp scaling up to 4K, windowed, borderless and fullscreen modes, VSync
  and a frame limit, and HD text.
- **One server for everything:** single player runs the same game server inside the client, so offline and online
  use the same code. The server simulation is deterministic: the same inputs always give the same game.
- **Online play:** a main server (accounts, characters, lobby, chat, ladder, shared stash and market) and game
  servers that run the games. Both are in this repository.
- **Your characters:** saved in D2Gamma's own format under `Documents\D2Gamma`. Diablo II `.d2s` saves can be
  imported.
- **Mods as their own games:** a mod runs on D2Gamma as its own game, with its own files and saves. See
  [docs/mods](docs/mods) and [examples/mod-game](examples/mod-game).
- **Dev tools:** an in-game dev panel and console (press `` ` ``): god mode, levels, items, warps and more. They are
  on by default during the beta. Turn them off with `--no-dev`.

## Playing

1. Download `D2Gamma-Game-*.zip` from the [latest release](https://github.com/Nugwa628/d2rust/releases/latest) and
   unzip it, or build the game yourself (below).
2. Put `D2Gamma-Game.exe` in your Diablo II folder (the folder that has `d2data.mpq`), or point it at the folder:
   ```powershell
   D2Gamma-Game.exe play --game-dir "C:\Program Files (x86)\Diablo II"
   ```
   You can also set the `D2_GAME_DIR` environment variable instead.
3. Start it. The main menu opens.

Shortcuts:

```powershell
D2Gamma-Game.exe --new sorceress MyName          # straight into a new character
D2Gamma-Game.exe --save "path\to\MyName.d2s"     # load (or import) a save
D2Gamma-Game.exe --difficulty nightmare ...      # normal / nightmare / hell
```

Key bindings are in `Documents\D2Gamma\controls.toml`. They start as the original game's default keys.
[docs/PLAYTEST.md](docs/PLAYTEST.md) has the full command line and how to report a bug.

## Building from source

You need [Rust](https://rustup.rs). `rust-toolchain.toml` picks the right version automatically.

```powershell
git clone https://github.com/Nugwa628/d2rust
cd d2rust
$env:D2_GAME_DIR = "C:\path\to\Diablo II"
cargo run --release -p d2-client -- play
```

Online play is behind the `online` feature (`cargo run --release -p d2-client --features online -- play`). A
build from source has no official server in its list; `tools/server/local-realm.sh` starts a main server and a
game server on your own PC and writes the game's `servers.toml` for them.

## Running your own realm

Online play needs a main server (`d2rust-realm`) and at least one game server (`d2rust-node`). Both are on the
[latest release](https://github.com/Nugwa628/d2rust/releases/latest) (`D2Gamma-Realm-*-linux-x86_64.tar.gz`,
`D2Gamma-Node-*-linux-x86_64.tar.gz`, `D2Gamma-Node-*-windows-x86_64.zip`), or build them from this repository
(`tools/server/server-build.sh`).

1. Unpack the realm package as the user that will run it, then `./server-ctl setup && ./server-ctl install
   bin/d2rust-realm`. Setup writes `d2rust-realm.cfg` from `d2rust-realm.cfg.example`.
2. The server needs the game data from your own Diablo II install: set `game_data` in `d2rust-realm.cfg`
   (`tools/server/game-data.sh` makes the smallest folder it needs).
3. **Turn the market on.** The main server's market is off unless its config says otherwise. The example
   config already has it on; in an existing config, add or change this line and restart (`./server-ctl restart`):
   ```
   market          = on
   ```
   With it on, players list, search, buy and make offers from the Market tab in realm games and in town.
4. Add a game server: `d2rust-realm --add-node <id>` writes its bundle; unpack the node package on the game
   server's machine and run `./server-ctl setup --node` (Linux) or `node-ctl.ps1 setup <id>` (Windows).
5. Players list your realm in `servers.toml` next to `D2Gamma-Game.exe`, with your address and the certificate
   fingerprint that `./server-ctl fingerprint` prints:
   ```toml
   [[server]]
   id = "my-realm"
   name = "My D2Gamma realm"
   game = "d2rust"
   address = "realm.example:6112"
   cert_sha256 = "<64 hex digits>"
   version = "0.1.1.2159"
   ```

More detail: [tools/server/README.md](tools/server/README.md).

## Platforms

Windows is the main platform. The game client uses [Bevy](https://bevyengine.org) and needs a GPU with Vulkan,
DirectX 12 or Metal. The servers and the tools also build on Linux.

Tests:

```powershell
cargo test --workspace                       # everything that needs no game files
cargo test --release -p d2-sim -- --ignored  # tests that read your game files (D2_GAME_DIR)
```

[docs/LOCAL-RUN.md](docs/LOCAL-RUN.md) lists every local check that needs the game files or a GPU, and
[CONTRIBUTING.md](CONTRIBUTING.md) the rules the code follows.

## How it fits together

```
your Diablo II files ──► d2-formats ──► d2-data ──┬──► d2-sim (60 Hz, deterministic) ◄── d2-server
                         (MPQ, DC6,      (tables)  │                                       ▲
                          DCC, COF ...)            └──► d2-client (Bevy: draw, UI, audio) ─┘ d2-proto / d2-net
```

| Crate | What it does |
|---|---|
| `d2-formats` | Reads the game's file formats: MPQ archives, tables, sprites, palettes, maps, sounds |
| `d2-data` | Turns the tables into typed game data, with mod patch layers on top |
| `d2-sim` | The game rules: units, combat, skills, items, AI, quests, levels. Deterministic, headless |
| `d2-server` | Runs the simulation. Owns characters and games |
| `d2-proto`, `d2-net`, `d2-quic` | The messages between client and server, and how they travel |
| `d2-online` | The online client session: connect, account, lobby and the game stream |
| `d2rust-realm` | The main online server: accounts, characters, lobby, chat, ladder, stash and market |
| `d2rust-node` | A game server: runs online games and links to its main server |
| `d2-client` | The game window: drawing, UI, sound and input (Bevy). Builds `D2Gamma-Game` |
| `d2-verify` | Checks that the game files are present |
| `tools/` | Developer tools: `mpq-tool`, `data-tool`, `d2s-tool`, `d2-convert`, `img-diff` and others |

Crate names, `D2RUST_*` environment variables and file formats keep the project's earlier name, d2rust.
More detail: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Documentation

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): the crates and the rules between them
- [docs/REQUIRED_FILES.md](docs/REQUIRED_FILES.md): which game files D2Gamma reads
- [docs/LOCAL-RUN.md](docs/LOCAL-RUN.md): building and checking on a PC with the game installed
- [docs/PLAYTEST.md](docs/PLAYTEST.md): how to play-test and report a bug
- [docs/mods](docs/mods): making a mod game
- [docs/tools](docs/tools): the test tools (scenarios, sweeps, multiplayer tests, picture checks)
- [CONTRIBUTING.md](CONTRIBUTING.md): the rules the code follows
- [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md): third-party licenses

## Legal

D2Gamma is a fan project. It is not affiliated with, endorsed by or connected to Blizzard Entertainment. Diablo is a
trademark of Blizzard Entertainment, Inc. This repository contains no Blizzard game files, art, sound or other
content. To play, you need your own legally bought copy of Diablo II: Lord of Destruction.

The D2Gamma source code is released under the [MIT License](LICENSE).
