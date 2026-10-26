# Architecture

## The one diagram that matters

```
                 ┌──────────────────────────────────────┐
 player's files  │ the player's own D2 install          │
                 │ (MPQs, never in git)                 │
                 └───────────────┬──────────────────────┘
                                 │ d2-convert, once per install
                                 │ (every file round-trip checked)
                 ┌───────────────▼──────────────────────┐
                 │ native folder (PNG, TOML, TSV)       │
                 │ + mod layers (mods/<layer>/)         │
                 └───────────────┬──────────────────────┘
                                 │ read at run time
                                 │ (debug builds may read the MPQs)
                ┌────────────────▼─────────────────┐
                │ d2-formats → d2-native → d2-data │  plain Rust
                │ readers    layer stack   tables  │
                │            (+ mod patch layers)  │
                └───────┬──────────────────┬───────┘
                        │                  │
          ┌─────────────▼──────┐   ┌───────▼───────────────────────┐
          │ d2-sim             │   │ d2-client (Bevy, the game exe)│
          │ deterministic      │   │  front end, then the game     │
          │ 60 ticks/sec       │   │  bridge ◄── S→C messages      │
          │ Ruleset: Original  │   │  world view, GPU compositor,  │
          │   or Mod(ModRules) │   │  UI, audio, input, ClientMods │
          └─────────┬──────────┘   └───────┬───────────────────────┘
                    │ owned by             │ C→S intents
          ┌─────────▼──────────┐   ┌───────▼───────────┐
          │ d2-server          │◄──┤ d2-net transport  │
          │ host: 1 ms clock,  │   │ d2-proto messages │
          │ hub, game thread   │   │ memory or QUIC    │
          └────────────────────┘   └───────────────────┘
```

Every game, single player or online, is the same `d2-server` host
running `d2-sim`, and every client talks to it in the same `d2-proto`
messages over a `d2-net` transport. What differs is where the host runs
and which transport carries the messages:

```
 SINGLE PLAYER: one process
 ┌───────────────────────── D2Gamma-Game ───────────────────────────┐
 │ d2-client bridge ◄── d2-net in-memory ──► d2-server hub, host    │
 │                                           + d2-sim (game thread) │
 │ characters: .d2s in Documents\D2Gamma\saves                      │
 │ shared stash: d2-shared-stash LocalStash (SQLite, save folder)   │
 └──────────────────────────────────────────────────────────────────┘

 ONLINE: the game exe built with its `online` feature
 ┌──── D2Gamma-Game ─────┐ control stream   ┌──── d2rust-realm (Linux) ──────┐
 │ d2-online Session     ├────── QUIC ─────►│ main server, runs no games:    │
 │  login, characters,   │ (stays open in   │ accounts, characters (.d2rc),  │
 │  lobby, chat, ladder, │  the game)       │ lobby, chat, ladder, upgrades, │
 │  stash, market        │                  │ shared stash + market, item    │
 │                       │                  │ registry, save checks, journal │
 │                       │                  │ replays (SQLite)               │
 │                       │                  └───────────────┬────────────────┘
 │                       │                    seat offer +  │ d2-link over QUIC
 │                       │                    the game's    │ (pinned certs)
 │                       │ game stream +      saves         │
 │ bridge remote link    │ join ticket      ┌───────────────▼────────────────┐
 │  (paced frames)       ├────── QUIC ─────►│ d2rust-node (Linux, Windows)   │
 └───────────────────────┘                  │ game server: d2-games runs     │
                                            │ each game on its own thread    │
                                            │ (d2-server host + d2-sim),     │
                                            │ journal recorder; no accounts, │
                                            │ no characters, no database     │
                                            └────────────────────────────────┘
```

There is no separate "offline mode" code path to maintain: single player
is the authoritative server in the same process, connected through the
in-memory transport.

## Crates

Game and data (no Bevy):

| Crate | What it is |
|---|---|
| `d2-formats` | Readers for the game's file formats (MPQ, DC6, DCC, DT1, DS1, COF, AnimData, palettes, fonts, string tables, WAV) and the `.d2s` save codec. |
| `d2-native` | The native data layer: plain, moddable files (PNG, TOML sidecars, TSV tables) holding exactly what the packed formats hold, and the run-time layer stack (base + mod layers) the loaders read through. |
| `d2-data` | Typed game tables: the `.txt` / `.bin` loaders, the formula compiler, string tables, the game-data hash and mod patch layers applied over the player's tables. |
| `d2-sim` | The deterministic game simulation (see Boundaries), with the `Ruleset` and the mod seams (`ModRules` and the item, missile and monster providers). |
| `d2-proto` | Every client↔server message, generated from `tables/sim/*-messages.tsv`; the mod message envelope; the dev command channel. |
| `d2-net` | The transport seam (`Transport`), the in-memory transport of single player, and the wire format (frames, versioned handshake, goodbyes) and the account, lobby, social, ladder, stash, market and game-server link messages. |
| `d2-quic` | The QUIC stream layer of every online connection: TLS 1.3 with a pinned certificate, keepalive, timeouts, connection migration, dual-stack (IPv4 + IPv6) sockets. |
| `d2-server` | The authoritative game host: message queues, dispatcher, per-client buffers, the tick schedule on a 1 ms base clock, the hub that serves clients over a `Transport`, the game thread, and the seams (`seams`) other crates plug into. |

The game exe:

| Crate | What it is |
|---|---|
| `d2-client` | The Bevy app, built as `D2Gamma-Game`: the front end (`app::play::run_front_end`, also the entry point of every mod game's binary), the bridge, the world view and its CPU / GPU compositors, UI, audio, input, the client hooks of mod games (`mods::ClientMods`), and the dev and test subcommands (`verify`, `sweep`, `soak`, `state-dump`, `bake-anims`, ...). Uses mimalloc as its allocator (default feature `mimalloc`). |
| `d2-online` | The online client session: QUIC connect and handshake, account and lobby requests, chat, ladder, stash and market requests, the game stream, server pings. Linked by the exe only with its `online` feature. |
| `d2-shared-stash` | The shared stash as the client sees it: one seam, a local SQLite store in single player and the online store through the main server. |
| `d2-verify` | The ownership gate's place: checks that the chosen folder holds a usable LoD install. |
| `d2-report` | The bug report bundle format, shared by the game, the launcher and the report service. |

Multiplayer (the two server programs and their parts):

| Crate | What it is |
|---|---|
| `d2rust-realm` | The main server (Linux): sessions, accounts, lobby, chat, ladder, the market and shared stash, the game-server list and links, upgrades. Runs no games. |
| `d2rust-node` | A game server (Linux and Windows), as a library (`d2rust_node::main(NodeGame)`) and D2Gamma's binary over it: runs games only and links out to its main server. A mod game's game server is the same library with its own `NodeGame`. |
| `d2-games` | Running games for a game server: each game on its own thread at the server tick, its seats and its saves (handed to a `SaveSink`). |
| `d2-link` | The game-server link both programs share: control stream, record and journal streams, the version match, a game server's bundle. |
| `d2-tickets` | Join tickets: the main server signs a seat, the game server checks it once. |
| `d2-service` | What both server programs share: config reader, identity, log, `status.json`, the ready line, stop signals, the game-data hash. |
| `d2-accounts` | Accounts and online characters (SQLite plus one `.d2rc` file per character), guilds, friends, ladder rows. Main server only. |
| `d2-economy` | The shared stash and the market of one mod realm (SQLite, one transaction per move). |
| `d2-item-registry` | Realm item IDs: blocks granted per game, every saved ID checked, duplicates found. |
| `d2-save-checks` | Every save a game server returns is decoded and checked against the realm's game rules before it is stored. |
| `d2-journal` | A game's journal (inputs and checkpoints), its recorder on the game server, and the replay audit. |
| `d2-audit` | The audit queue, findings, rollbacks and the determinism watch; used by the `d2rust-audit` tool. |
| `d2-limits` | Rate limits (token buckets per account, address and game server; a per-address gate before the QUIC handshake). |
| `d2-reach`, `d2-host-control` | Reachability (port mapping, outside check, hole punching) and the client side of hosting a game server on a player's PC. Libraries only: no program uses them until player hosting is designed. |

Tests and tools: `test-fixtures` (synthetic game data built at test time),
`mp-test` (multiplayer scenario bots, `mp-load`), `examples/mod-game` (the
example mod game), and under `tools/`: `d2-convert` (install → native
folder), `data-tool`, `mpq-tool`, `d2s-tool`, `img-diff`, `depcheck`
(enforces the dependency rules), `report-service`, `d2rust-audit`.

The launcher is a separate program, versioned on its own. It
installs and updates the game, writes `servers.toml` next to the exe for
online play from its signed manifest, and starts the game with
`play --game-dir <dir>`; it reads the exe's `version.json` and
`--version` line.

## Boundaries

**d2-sim**
- Input: the previous state + a list of player intents for this tick.
- Output: new state + a list of events (sound, visual effect, chat, etc.).
- Fixed 60 Hz logic tick for base play. `TickRate::BASE` remains the 25 Hz
  unit of table data; speeds and durations convert through the game rate.
  The host schedules tick n at floor(n * 1000 / hz) ms on its 1 ms base
  clock, without drift. The client uses the server rate. Never variable
  timestep.
- Unit update order, RNG draw order and integer math are fixed, so the same
  inputs always give the same game.
- Can be run headless by tests, the servers and tools.

**d2-server host**
- Wall-clock time comes in through an injected clock and never reaches the
  sim; the host extends it to a 1 ms base clock that never wraps.
- One frame drains the client queues, runs at most one due tick and
  flushes every client.
- Single player drives the host from the client's frame (through
  `d2-server`'s game thread); a game server's `d2-games` thread loops it at
  the server tick.

**d2-client::bridge**
- Receives the server's S→C messages (the `d2-proto` wire format) and
  dispatches them into a plain-Rust client world model.
- Spawns/updates/despawns Bevy entities that *mirror* that model for
  drawing.
- Two server links: the local link (the in-process hub over the in-memory
  transport) and the remote link (a game on a game server; it buffers whole
  server frames and hands the bridge one per tick interval, and reconnects
  while the server holds the player's slot).
- Between messages, views advance by the client's own rules
  (`d2-client::rules`, animation): the local player's own walk is
  predicted along the server's path code over the client's map, monster
  walks and client missiles step per tick. The server's next message
  always wins.
- Converts input into intents (move, attack, cast, pick up, etc.): C→S
  messages built with `d2-proto`.
- Decides no outcome. If you need to know whether an action is legal,
  the server tells you.

**d2-client (Bevy)**
- The world view turns the client world model into a draw list each frame
  (sprite frame, position, clip, shade chain, blend op, draw key), ordered
  by D2's isometric draw-cell rules, not a generic depth sort.
- A GPU compute compositor composes the draw list into an 8-bit index
  framebuffer with integer math, then maps it to colour through the frame
  palette. A CPU reference compositor composes the same list; `verify`
  compares the two byte for byte.
- Lighting, shading and blend modes are plain-Rust rules (`rules`)
  feeding the draw items.
- Assets load through the `mpq://` asset source, backed by the native
  folder (or the MPQs in debug builds). Generated in-between animation
  frames are baked into files at install (`bake-anims`) and only read at
  run time. Any decoded cache stays on the player's machine.
- Display: a 1280 × 720 standard view, scaled to the window, with the
  player's video options.

## Data and mods

- Release builds read the native folder `d2-convert` made from the player's
  install (`<data dir>/d2gamma/native`, or `--native DIR`); debug builds
  can read the MPQs directly (`--source mpq`).
- Mod layers are folders under the native folder's `mods/` (a `mod.toml`
  plus files), or mod archives laid over an install. Table changes are
  patch files (add/change/remove rows and columns) applied in order at load
  time to the player's own tables; a mod release holds only these patches
  plus the mod's own assets.
- Each mod is its own game: its own binary on these crates
  (`docs/mods/mod-game.md`, `examples/mod-game/`), with its own identity
  (name, Documents folder, credits), its own saves and its own layers.
  The D2Gamma exe never loads a mod; it always builds `Ruleset::Original`.
- A mod game's overrides go through seams (`docs/plans/mods/seams.md`):
  `Ruleset::Mod(ModRules)` and the providers in `d2-sim`, `ClientMods` for
  presentation and the mod's own screens in `d2-client`, its messages in
  the `d2-proto` mod envelope (C→S 0x71, S→C 0xB5, only on a mod game's
  wire), and its game server through `NodeGame` in `d2rust-node`. Every
  seam falls back to D2Gamma's own rules when the mod has no answer.

## Persistence

- Single-player characters are `.d2s` files in the player's save folder
  (`Documents\D2Gamma\saves`); a mod game uses its own folder and may use
  its own format (`CharacterCodec`).
- Online characters live on the main server in our own versioned
  envelope (`.d2rc`, `d2-accounts`), never on the client; `.d2s` import /
  export is a tool, not the online storage format.
- Accounts, the online shared stash, the market and the item registry are
  SQLite databases on the main server; a single-player shared stash is a
  SQLite file in the save folder.
- Every persisted format carries a version and has migrations.

Tick-indexed recordings carry `tick_rate`: state/packet/RNG JSON headers use a number,
frame schedules and clock/seed tables use `# tick_rate 60`, and soak logs use
`tick_rate=60` in the header. Autoplay routes start with
`autoplay-route 1 tick_rate=60`. Repro reports, state-tail headers and standalone
client snapshots also state the rate. Missing metadata identifies a legacy 25 Hz recording;
replay refuses it and requests re-recording. Changing the header alone is not a migration.
Character saves contain character progress and Unix-second timestamps, with no game
frame counter, timer expiration, animation phase or cooldown carried into the next game.
