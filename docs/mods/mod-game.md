# Making a mod game

A mod for D2Gamma is **its own game**: its own binary, built on the D2Gamma crates, shipped as its own files and
started directly by the player. The D2Gamma exe never loads or selects a mod; it always plays D2Gamma's own rules on
the `base/` layer of the player's files. The working example is `examples/mod-game/` (synthetic, never shipped).

## Workspace layout

A mod game lives in its own repository with a Cargo workspace:

```
my-mod/
  Cargo.toml          workspace; depends on the D2Gamma crates (git or path)
  game/               the mod game's binary (like examples/mod-game)
  crates/             the mod's own rule crates (its ModRules, providers)
  layers/             the mod's layer folders: mod.toml + files/ (patches, art)
```

Depend on:
- `d2-sim`: `rules::{Ruleset, ModRules}` and the provider traits of the seams the mod overrides;
- `d2-client`: `app::single_player::{GameBuilder, GameSource, Layers, Saves, CharacterCodec}` and the app;
- `d2-server`: the server and session layer, shared by D2Gamma and every mod game.

## Starting a game

1. **Data and layers.** Open the player's files with the mod's layers:
   `GameData::open(&GameSource::Native(dir, Layers::List(vec!["my-mod".into()])))`. Each layer is a folder under
   the native folder's `mods/` with a `mod.toml` and a `files/` tree; table changes are patches applied to the
   player's own tables, never copies of them.
2. **Rules.** Implement `ModRules` with the seams the mod changes (every method defaults to D2Gamma's rules), and
   pass it as `Ruleset::Mod(Arc::new(MyRules))`. The ruleset is set when the game is built and never changes.
   Overrides stay deterministic: no I/O, no clock, no floats in game state, randomness only from the game's RNG.
3. **Saves.** Give the game its own folder, `Saves::new(folder, codec)`. `D2sCodec` writes D2Gamma's `.d2s`; a mod
   whose characters hold more implements `CharacterCodec` with its own format. D2Gamma's saves are never extended.
4. **Build.** `GameBuilder::new(data).rules(rules).saves(saves)`, then `build()` or `start(clock)`.

The example's binary does all four and runs a few headless frames:

```sh
cargo run -p mod-game-example -- --native <native folder> --layer <layer> --saves <folder>
```

A mod game whose art and tables ship as archives lays them over the player's install instead:
`GameSource::LayeredArchives(install, vec![my_mod_mpq])` searches the mod's archives first, the first listed first.

## Playing in a window

1. **Identity.** First thing in `main`, install the game's identity, before `d2_client::user_dir::init` and
   before any window opens: `d2_client::identity::install(GameIdentity::new("My Mod", "1.0", "MyMod"))`. It sets
   the name and version lines, the window title, the folder under Documents (`Documents\MyMod`: saves, settings,
   logs, cache) and, with `.with_credits(..)`, the credits screen's text. Without it the game is D2Gamma's own.
2. **The front end.** `app::play::run_front_end(FrontEndConfig::new(data, game_dir, rules, saves, mods))` opens
   the window at the main menu, as D2Gamma's exe does: character select and create on the game's save folder, then
   each game the player starts with the game's `rules`, `saves` and `mods`; Save and Exit goes back to the main
   menu. The game's own screens replace D2Gamma's through its `ClientMods` (`with_front_screen`,
   `with_front_canvas`, `with_menu_sides_default`). Characters are read and written with the game's
   `CharacterCodec` (`front_session::CodecFiles`: the select list, the new character's file, Convert, the load),
   so a game with its own save format and extension lists only its own files. A custom `make_game` builds each
   game's `PlayConfig` itself. `front_session::HeadlessFrontEnd` runs the same session without a window, for tests.
3. **Straight into a game.** Fill an `app::play::PlayConfig` with the game's `data`, `rules`, `saves` and `mods`
   (its `ClientMods`, empty when it has none) and the character to play, then `app::play::run(config)`. A host on
   the mod wire gets a bridge on the mod wire; `ClientMods::with_mod_messages` handles its S→C messages.

```sh
cargo run -p mod-game-example -- --game-dir <install> --window [--archive <mod.mpq>]   # front end
cargo run -p mod-game-example -- --game-dir <install> --window --name Hero              # straight in
```

## Where overrides go

The seams (override points) are listed in `docs/plans/mods/seams.md`. When a mod needs to change something that has
no seam yet, the seam is added to D2Gamma there; mod games don't fork D2Gamma's code.

## Multiplayer

D2Gamma and the mod games share one server and session layer and one protocol.

- **Mod messages.** A mod game's own messages travel in the protocol's mod message envelope (`d2_proto::envelope`:
  C→S 0x71, S→C 0xB5), only on a mod game's wire; D2Gamma's own wire refuses both ids. The client sends them with
  `Bridge::send_mod` and reads them through `ClientMods::with_mod_messages`; the mod stash panel uses them.
- **Game server.** A mod game's game server is `d2rust-node` as a library with the mod game's own `NodeGame`
  (`d2rust_node::main(NodeGame { program, game, .. })`): a binary of its own whose games are built with the mod
  game's layers, rules and character format. Its link hello names its game, and a main server of another game
  refuses it.
- **Main server.** Each mod realm keeps its own economy (shared stash, market, item registry) in
  `data/realms/<mod>/realm.sqlite`, and game servers are added to a realm with `d2rust-realm --add-node ... --realm
  MOD`.

## Animation frame packs

Mods generate animation packs in their own build pipeline with the shared [animation generator](animation-generation.md), then install validated [frame packs](animation-frame-pack.md) through [pack playback](animation-playback.md). The base-game runtime generator is disabled in shared mod wiring. Mod source art and generated packs stay in the mod repository.
