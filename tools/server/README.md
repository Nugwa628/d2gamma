# Multiplayer servers: local tools

Online play is two programs:

- `d2rust-realm`, the main server: accounts, characters, the lobby and the list of game servers, on one UDP port
  with one QUIC endpoint.
- `d2rust-node`, a game server: runs the games and links to its main server over QUIC from its own UDP port. It
  keeps no characters and no accounts. `d2rust-realm --add-node <id>` writes its bundle.

| File | What it does |
|---|---|
| `local-realm.sh` | starts a main server and one game server on one PC for a dev run, and writes the game's `servers.toml` |
| `d2rust-realm.cfg.example` | a commented main-server config |
| `d2rust-node.cfg.example` | a commented game-server config |
| `game-data.sh` | makes the smallest game-data folder a server needs from a game install |
| `smoke-linux.sh` | checks that a Linux server package starts on a plain Debian 13 |
| `server-build.sh` | builds and packages the programs of `programs.txt`: `linux` (Debian 13 tarball and .deb) or `windows` (game-server zip, cargo-xwin) |
| `server-ctl` | runs the main server or a Linux game server without root: setup, install, upgrade, start, stop, status, backups |
| `node-ctl.ps1` | runs a Windows game server, hidden, and upgrades it |
| `check-exe.py` | the version check of a Windows server program built on Linux |
| `test-server-ctl.sh` | the offline test of `server-ctl` |

A dev run:

```sh
D2_GAME_DIR=/path/to/diablo2 sh tools/server/local-realm.sh
cargo run --features online -p d2-client -- play --online --server local
```

The market is off in the main server's code by default; `d2rust-realm.cfg.example` turns it on with
`market          = on` (set it in your `d2rust-realm.cfg` and restart to change it).

A real server, as its own user: unpack the Linux package, then `./server-ctl setup && ./server-ctl install bin/d2rust-realm`
(`./server-ctl setup --node` for a game server). `upgrade` and `fetch` download signed packages from your own
release downloads: set `D2RUST_RELEASE_BASE` (or write the base URL into the server folder's `release-base` file;
`release-base.txt` for `node-ctl.ps1`) and put the release key's public half into `release-signers`.

Game data always comes from your own Diablo II install.
