---
id: mod-seams
title: Mod seams (what mod games override)
area: mods
status: in-progress
order: later
modules: MS0-MS16
light: MS0-MS4, MS16
depends_on: []
---

# Plan: Mod seams

Status: **in progress** (2026-10-17; table checked against main 2026-10-26: every module but E1 is merged). It's built with `docs/FEATURE-METHOD.md`. Lane MOD-SEAMS.

## Goal

**Each mod is its own game.** A total conversion (Median XL, Project Diablo 2, …) ships as its own files and is
launched directly. It is its own binary, built from a workspace that depends on the D2Gamma crates plus the mod's
own crates (the D2RUST-MXL repo is the first one). The mod binary installs its overrides when it builds or starts a
game. **The D2Gamma exe never loads or selects a mod:** it always runs D2Gamma's own rules.

D2Gamma is the engine and a remaster of Diablo 2: about 99% exact, with some key systems modernized on purpose, and
it still plays as Diablo 2. Large creative changes belong in mod games, and a mod game may override anything, up to
a whole system. This plan builds the **override points** (seams) that mod games install. It is not a closed plugin
API: when a mod game needs to override something that has no seam yet, the seam is added here.

Rules that apply to every module:
- **Seams change nothing by themselves.** With `Ruleset::Original`, how the game plays, frame hashes, determinism,
  save formats and the protocol stay exactly as D2Gamma has them. Every module proves it with the existing
  determinism and hash tests. (A deliberate change to D2Gamma's own rules is a separate PR that says so.)
- **Seams are mod-agnostic.** No mod's code, data, names or ids go into D2Gamma. A seam is shaped so any mod game
  could use it; the first users (the PD2 and MXL port lanes) only tell us which shapes are needed. Tests and the
  example use small synthetic overrides.
- **Mod data is patches** applied to the player's own tables at load time; mod games never ship copies of the
  game's tables or string tables.
- **Zero cost when unused,** where practical: under `Original` a seam costs at most one branch on an enum.
- **Mod hooks are deterministic** (rule 5): no I/O, no clock, no floats, randomness only through the game's
  seeded RNG passed in by the world.
- **Saves belong to each game.** A mod game has its own save folder and its own save format. D2Gamma's saves are
  never extended for mods.
- **Multiplayer is shared.** D2Gamma and the mod games use one multiplayer architecture: the same server and
  session layer and the same protocol, with one extension envelope that carries a mod game's own messages.

## Where things are today

- **Rule switch:** none. `CONTRIBUTING.md` rule 7 names `Ruleset::Original` / `Ruleset::Mod`, but no such type exists in
  any crate.
- **Game assembly:** single player is built in `d2-client/src/app/single_player/mod.rs` (`build`, `build_with`),
  from the private `GameParts` and a fixed `LocalSeams` (`single_player/seams.rs`). The per-game wiring state is
  `d2_sim::wiring::action::ActionHooks<X>` (`ActionHooks::new`), which already carries game-wide switches such
  as `dev: DevSwitches`. `d2-server` hosts the loop over its own seams (`d2-server/src/seams.rs`, `host.rs`).
- **Mod files at runtime:** `d2_native::layers` stacks `base/` and then the `mods/<name>/{mod.toml, files/…}` trees
  that `mods/order.toml` enables, and `d2_native::source` applies their patches. Since MS16 the D2Gamma exe opens
  `base/` alone and never reads the order file; a mod game's binary passes its own layer list (MS2's `Layers`).
- **Skill functions:** `d2_sim::skills::use_::SkillFunctions` (`srvst`, `srvdo` by slot index) with the slot
  table in `skills/use_/table.rs` (`START_SLOTS` 91, `DO_SLOTS` 191, `lookup`). Providers answer only filled
  slots; there is no way to replace a built-in body.
- **Other dispatch:** monster AI is a fixed `AI_TABLE: [AiRecord; 148]` (`monsters/ai/table.rs`); unique-monster
  mods are a `match` (`monsters/init/umods.rs`); missile do/hit run through `units/hooks.rs` `missile_do` and the
  wiring; item properties through `items/props.rs` `apply_property`; item-use reads `pspell`
  (`items/inventory/tables.rs`) with no dispatch seam; cube op codes are a closed `match` in
  `world/cube.rs` `op_info` (with `CubeWorld`); stat callbacks through `skills/stat_cb.rs` `StatCbWorld`; item and
  skill events through `combat/events.rs` `EventWorld`; quests through per-quest code and `QuestRest`.
- **Formulas:** hit chance `combat/hit.rs` (`attack_rating`, `hit_chance`); players-X through
  `CombatWorld::player_count_bonus` (`combat/mod.rs`); resists, avoid/block and damage in `combat/`, all fixed.
- **Calc:** `d2_data::calc::Keyword` is a fixed enum; the evaluator is `d2_sim::skills::calc::eval` over
  `CalcContext`.
- **Data:** `d2_data::patch` reads `d2patch 1` (`set`, `add … like`, `check`, `diff_tables`, `apply_stack`,
  `compile_patched`). No `addcol`/`delcol`/`remove` and no string-table layers. A mod can't ship its own side
  tables. Several id and row limits are constants.
- **Saves:** the character format is `.d2s` (`d2-formats/src/d2s.rs`, `VERSION` 0x60); the save folder is fixed
  by the client.
- **Protocol:** `d2-proto` uses fixed id tables; C→S ids 0x71..0xFE and S→C ids from 0xB5 are invalid.
- **Server and sessions:** `d2-server` runs the tick and keys queues and buffers by `ClientId`; the transport seam,
  several clients, QUIC and the dedicated server are built by the multiplayer plan
  ([multiplayer](../server/multiplayer.md)) on its own branch.
- **Client:** HUD and panels are fixed (`d2-client/src/ui/`); no override point.

## Decisions (owner, 2026-10-17)

The owner answered the questions this plan raised; they are recorded in `docs/plans/DECISIONS.md`.

1. **Shape of `Ruleset`.** `enum Ruleset { Original, Mod(Arc<dyn ModRules>) }` in a new `d2_sim::rules`
   module. `ModRules` is one object-safe trait whose methods are added by later modules, each with a default that
   means "no change" (`None` or the unchanged value). A mod game implements only what it changes.
2. **Where it lives.** The game's `Ruleset` is set once at game creation and held by the per-game wiring state
   (`ActionHooks::rules`), next to `dev`. It never changes during a game. The client and server read it from there.
   The D2Gamma exe always passes `Original`; only a mod game's binary passes `Mod`.
3. **Provider shape.** A replaceable table (skill functions, AI, missiles, …) is a provider the `ModRules` hands
   out per seam (`fn skills(&self) -> Option<&dyn SkillProvider>`). A provider answers `Some(result)` for a slot it
   owns and `None` to fall through to D2Gamma's own body (mod first, D2Gamma after). Providers get the world as the
   seam's existing world trait with its unit type fixed to `UnitId`, as `&mut dyn`; where a seam trait isn't
   object-safe today, its module adds a narrow object-safe view. A mod game may replace a whole system, not only
   single entries; seams are added when a mod needs them.
4. **Saves: no mod data in D2Gamma saves** (the earlier `.d2rmod` sidecar is dropped). Each mod game owns its save
   folder and save format. D2Gamma only lets a game choose its save folder and character codec (MS2).
5. **Mod messages: one envelope id per direction** in the shared protocol, C→S 0x71 and S→C 0xB5:
   `[id][u16 length][payload]`, the payload opaque to D2Gamma and handed to the mod game's dispatch hook. Plain
   D2Gamma (`Original`) rejects both ids as invalid, exactly as today. A mod game adds messages through the envelope
   instead of forking the netcode.
6. **Shared server and session layer.** The server and session layer is an engine part: mod games reuse
   D2Gamma's server, sessions and transport, built with their own `Ruleset` (MS15).
7. **Ids:** module ids are `MS0`…; PR titles are `Seams: <what>`.

## Modules

Ranked by how much each unblocks **both** ports (PD2's `S1`-`S20`, MXL's seams 1-10, listed per module), then cost,
then risk (the protocol and the server go late).

| Rank | Id | Module | Tier | Size | Depends on | PD2 | MXL | Status |
|---|---|---|---|---|---|---|---|---|
| 0 | MS0 | `Ruleset` and `ModRules`, threaded through game creation | light | S | none | S1 | 1 | merged: #174 |
| 1 | MS1 | Skill function providers: replace srvst/srvdo bodies, mod first then D2Gamma | light | S | MS0 | S6 | 4 | merged: #181 |
| 2 | MS2 | Public game builder: tables, layers, save folder and codec, and `Ruleset` in | light | M | MS0 | S15, S11 | 9, 6 | merged: #203 |
| 3 | MS16 | Mod game binary example; the D2Gamma exe stops selecting mods | light | S | MS2 | S15 | 9 | merged: #205 |
| 4 | MS3 | Formula hooks: hit chance, avoid/block, resists and caps, damage, players-X, experience length | light | M | MS0 | S1 | 2 | merged: #231 |
| 5 | MS4 | `addcol` in `d2patch` (extension columns on base tables) | light | M | none | S2, S10 | 5 | merged: #184 |
| 6 | MS5 | Monster providers: AI table, unique-monster mods | full | M | MS1 | S9 | 4 | merged: #234 |
| 7 | MS6 | Missile providers: do and hit functions | full | M | MS1 | S9b | 4 | merged: #238 |
| 8 | MS7 | Item providers: property funcs, item events, item use (`pspell`), stat callbacks, item generation | full | M (MS7a-e) | MS1 | S8 | 4 | merged: #244, #279, #280, #281, #327 |
| 9 | MS8 | World providers: cube ops and outputs, object operate, NPC interaction and stores, quest callbacks | full | M (MS8a-e) | MS1 | S7, S16 | 4 | merged: #245, #251, #253, #257, #259 |
| 10 | MS9 | Calc registry: mod keywords and opcodes per family | full | M | MS0 | — | 3 | merged: #190 |
| 11 | MS10 | Mod table data: string layers, ids and rows past limits, side tables, item bit columns, compile tolerance | full | L (MS10a-d) | MS4 | S3, S5, S14, S20 | 5 | merged: #202, #218, #221, #222 |
| 12 | MS11 | Event hooks: game create/free, join, unit killed, periodic, level enter, chat command | full | M | MS0 | S19 | 7 | merged: #246 |
| 13 | MS13 | Mod message envelope in `d2-proto`, server and client dispatch | full | M | MS0 | S12 | 8 | merged: #263 |
| 14 | MS15 | Shared server and session layer: mod games run D2Gamma's server with their own rules | full | M | MS2, MS13 | S12 | 8 | merged: game servers #824 (`d2rust-node` as a library, `NodeGame`); realm `GAME` setting #847 (E3) |
| 15 | MS14 | Client hooks: HUD lines, panels, item-name filter, hover-item lines, audio categories | full | L (MS14a-c) | MS2 | S13, S18 | 10 | merged: #258, #265, #266 |
| 16 | E1 | The windowed game takes a mod game's rules, saves, layers and client hooks, plus its identity (name, title, Documents folder, credits) | full | M | MS2, MS14 | S15 | 9 | in review: #822 |
| 17 | F1 | The front end (main menu, character select/create) as a library entry point (`play::run_front_end`); a mod game's characters in its own save format | full | M | E1, E-UI2 | S15 | 9 | in review: #881 |
| — | MS12 | ~~Mod save sidecar~~ | — | — | — | — | — | dropped: each mod game owns its saves (MS2) |

The light tier (MS0-MS4, MS16) is what lets both ports start building their own game binary: the rule switch, skill
bodies, the builder and an example of a mod game using it, the core combat formulas and their extra table columns.

### MS0: `Ruleset` and `ModRules`

- **What:** the switch every later seam reads. Nothing plays differently.
- **Files:** new `crates/d2-sim/src/rules.rs` (`Ruleset`, `ModRules`, `Ruleset::is_original`, `Ruleset::mod_rules`);
  `wiring/action/mod.rs` (`ActionHooks::rules`, default `Original`); `d2-client/src/app/single_player/mod.rs`
  (`build_with_rules`; `build` and `build_with` pass `Original`).
- **Seam:** `Ruleset` (default `Original`); `ModRules: Send + Sync + Debug` with only `name()` for now (the mod
  game's name, for logs and reports).
- **Done when:** `rules` unit tests (default, clone, `mod_rules()`); `cargo nextest -p d2-sim` and the client
  determinism/frame-hash tests green with no expectation changed; `pre_pr` for d2-sim and d2-client.
- **Risks:** `ActionHooks` is shared with the tick-rate and PERF lanes; the diff adds one field and its default
  only.

### MS1: skill function providers

- **What:** a mod game replaces or adds skill start/do bodies by slot; the others keep D2Gamma's.
- **Files:** `skills/use_/` (the call sites of `SkillFunctions::srvst`/`srvdo` and `table::lookup`), `rules.rs`.
- **Seam:** `ModRules::skills() -> Option<&dyn SkillProvider>`; `SkillProvider::{srvst, srvdo}` return
  `Option<i32>`; slot bounds come from the provider (`slots(kind)`) under `Mod`, the constants under `Original`.
- **Done when:** a test provider overrides one filled slot and fills one past `DO_SLOTS`; the others run D2Gamma's
  bodies; `Original` tests unchanged.

### MS2: public game builder

- **What:** a mod game's binary starts a single-player game with its own tables, layers, save folder and rules.
- **Files:** `single_player/mod.rs` (`GameParts` → a public `GameSetup`/builder), `single_player/seams.rs`, the
  client's save-folder setting.
- **Seam:** `GameBuilder { data, seed, character, rules, layers, saves }` with `build()`; `saves` is the save
  folder plus a character codec (D2Gamma's `.d2s` codec by default; a mod game may pass its own). Today's
  `build`/`build_with` become thin wrappers that pass D2Gamma's defaults.
- **Done when:** the existing build tests pass through the builder; a test builds with a `Mod` ruleset, its own
  layer list and a save folder in a temp directory.
- **Risks:** client file, shared with the UI and PERF lanes only at the edges; post before touching.
- **As built:** `single_player/builder.rs`. The layers are chosen when the data is opened, since the tables come
  from them: `GameData::open(&GameSource::Native(dir, Layers::List(mods)))` (`Layers::OrderFile` is today's
  `mods/order.toml`, `Layers::base()` is `base/` alone; `d2_native::source::NativeSource::open_with`). Then
  `GameBuilder::new(data).seed(..).character(..).rules(..).saves(Saves::new(folder, codec))` with `build()` or
  `start(clock)`. `CharacterCodec` reads and writes a game's save bytes (`D2sCodec` is D2Gamma's); the server's
  `FileStore` and `save::share_with` write through it. The MS4 extension columns and MS9 calc registry are to be
  passed in with the `GameSource` and applied in `LiveData::load` (a marked hook there).

### MS16: mod game binary example

- **What:** shows the shape every mod game has, and takes mod selection out of the D2Gamma exe.
  - An example binary (`examples/mod-game/`, a workspace example, not shipped in any release) that depends on the
    D2Gamma crates, implements a tiny synthetic `ModRules` (one visible rule, e.g. a formula or a skill body), names
    its own save folder, and starts the game through `GameBuilder`. It carries no mod data and no real mod's
    names; its layers come from a folder the example is pointed at.
  - The D2Gamma exe opens only the `base` layer and passes `Ruleset::Original`. It no longer reads `mods/order.toml`;
    `d2_native::layers` stays as engine code, and the layer list comes from the builder (a mod game passes its own).
  - `docs/mods/` (or the plan's README) gets a short "make a mod game" page: workspace layout, which crates to
    depend on, where overrides are installed, own save folder, shared server.
- **Done when:** the example builds and a headless test starts it for a few ticks with its rule visible; the
  D2Gamma exe's tests show the same layers and frame hashes as before with no `mods/` folder present.
- **Risks:** players who used `mods/order.toml` lose it in the D2Gamma exe; that is the intended change (owner,
  2026-10-17). The release notes say so.
- **As built:** workspace member `examples/mod-game` (`mod-game-example`, `publish = false`): `ExampleRules` adds
  one skill "do" body past D2Gamma's table (MS1 seam), `Options` reads `--native DIR --layer NAME… --saves DIR` or
  `--game-dir`, and the game starts through `GameBuilder` and runs headless on a stepped clock. The exe's
  `select_data` opens native folders with `Layers::base()`. Guide: `docs/mods/mod-game.md`.

### MS3: formula hooks

- **What:** a mod game changes hit chance, avoid/dodge/evade/block, resist penalty per difficulty and resist/DR
  caps, weapon and elemental damage, players-X scaling, and the experience table length.
- **Files:** `rules.rs`, `combat/formulas.rs`, `combat/hit.rs`, `combat/damage.rs`, `combat/vitals/`,
  `monsters/init/` (players-X), and the world hosts that hand out the ruleset.
- **Seam:** `ModRules::formulas() -> Option<&dyn FormulaHooks>`. The mod answers first: each method returns
  `Option`, and `None` (every default) runs D2Gamma's own formula. Hooks that need the units get a `FormulaHost`
  (`Side::{Attacker, Defender}`: unit type, class, stats, states, difficulty, expansion and, where the formula may
  draw, the unit's seed). Under `Ruleset::Original` nothing is asked.
- **Not wired yet (D2Gamma's values stay):** missile damage fill (moves to MS6, which gives the missile world its
  ruleset), the client vitals view and HUD/character-screen resist and experience display, summon equipment init,
  and the quest/server max-level callers.
- **Done when:** unit tests per hook with a test provider; combat determinism tests unchanged.

### MS4: `addcol` in `d2patch`

- **What:** a patch layer adds columns to a base table (and its records carry them), so mod games can extend
  itemstatcost, levels, weapons, misc, magic affixes, sounds, runes slots, …
- **Files:** `d2-data/src/patch/` (syntax, apply, check), `d2-data/src/schema.rs` (extension fields),
  `data-tool patch` docs.
- **Seam:** `addcol <name> <type> [default]` inside a `table` zone (types `int`, `text`; A16 name taken, A17 value
  not of the type); extension columns are read through a typed accessor on the
  record (`ExtStore::ext(table, record, name)`, on `PatchedCompile::ext`), never through the base struct, so base
  records stay the same size and order. Not in `BinSet` yet (MS2/MS10 carry it to the game builder).
- **Done when:** parse/apply/check tests; a layer without `addcol` produces byte-identical compiled tables;
  format version stays `d2patch 1` (`addcol` is a new statement, older layers parse unchanged).

### MS5-MS8: providers for the other dispatch tables

Same pattern as MS1, one PR per module: an `Option<&dyn …Provider>` getter on `ModRules`, `Option` results that
fall through to D2Gamma's own code, slot counts from the provider under `Mod`. MS5: `AI_TABLE` and umods. MS6:
missile do/hit, and the MS3 damage hook on the missile damage fill. MS7: property funcs, item events, item use,
stat callbacks, item generation. MS8: cube op codes (`op_info` falls through for unknown codes) and outputs, object
operate, NPC interaction/stores, per-quest callbacks. **Done when:** a test provider per table; `Original` tests unchanged.

- **MS5 as built:** `ModRules::monsters() -> Option<&dyn MonsterProvider>` (`monsters/provider.rs`).
  `ai_record(ai)` and `special_record(state, class)` give rows past D2Gamma's tables or in place of its own
  (`ai::Ctx::record`; `install` accepts a special state past the table when the mod has a row for it).
  `ai_init` / `ai_think` answer inside `ai::run_init` / `ai::run_function`, so every place that runs an AI id goes
  through the mod first, with no direct calls around it. `umod_init` answers inside `init::run_umod_init` and
  `umod_callback` inside `init::dispatch`, per umod in the list, before D2Gamma's row lookup (umods past the table
  work). The bodies get `ai::Ctx<dyn AiHost>` and `dyn InitHost` (`AsDynAi`, `AsDynInit`), so they can call
  D2Gamma's AI and init helpers. The hosts read the rules through `AiUnits::rules` and `InitHost::rules`
  (default `Original`). Tests: `monsters/ai/tests/mod_ai.rs`, `monsters/init/tests/mod_umods.rs`.

- **MS6 as built:** `ModRules::missiles() -> Option<&dyn MissileProvider>` (`missiles/provider.rs`).
  `has(slot, index)` adds server-do / server-hit slots past D2Gamma's tables (or fills its empty ones):
  `catalogue::srv_do_called` / `srv_hit_called` gate the class handler, the hit handler and the no-owner hit path.
  `srv_do` / `srv_hit` answer first inside `catalogue::run_srv_do` / `run_srv_hit`; the bodies that ran another
  slot directly (server-do 34 → 3, 21 → 20, 32 → 7; server-hit 3 → 44) now go through those, so a mod body
  replaces D2Gamma's wherever its index is reached. The default flight stays a direct helper call. The bodies get
  `missiles::Ctx<dyn MissileWorld>` (`AsDynMissile`); the world reads the rules through `MissileUnits::rules`
  (default `Original`). Tests: `missiles/tests/mod_missiles.rs`.

MS7 is split in five PRs: MS7a property functions, MS7b item events, MS7c item use (`pspell`),
MS7d item generation, MS7e stat callbacks. They share `d2_sim::items::provider::ItemProvider` (`ModRules::items`).
- MS7a property functions, PR #244. The item tables carry the game's rules (`ItemTables::rules`, `Original` from
  `from_fixed`; the client's game build hands a mod game's to its item and drop tables, `GameParts::set_rules`), so
  every item creation and property path sees them. `props::apply_property` asks
  `ItemProvider::property_fn` first for each slot whose function id the provider `has_property_fn`, also ids
  D2Gamma has no function for; `None` runs D2Gamma's, and a slot neither knows ends the row. The function gets the
  slot (`PropCall`: id, mode, record, set, stat, val, prev) and the item through `PropHost` (tables, record, item
  level, quality, `roll` on the item seed, D2Gamma's stat writer `add_stat`, the item's stats). Format-0 items keep
  the legacy table. Left for later: the client's item views (tool tips) build their item tables without the
  game's rules (`app/play.rs`); a mod game's client hands them over with the client hooks (MS14).
- MS7b item events, PR #279. `combat::events::call` asks `EventWorld::mod_event` first; the skill use view
  (`UseView`) hands the event to `ItemProvider::event_fn` for each function id the provider `has_event_fn`, also
  ids D2Gamma has no function for, with the world as `SkillHost`, the call (event, H, O, key, level) and the
  event's damage record. `None` runs D2Gamma's function. Stat callback overrides are provided by MS7e below.
- MS7c item use, PR #280. The inventory desk's use dispatcher (`InvDesk::dispatch_use`) asks
  `ItemProvider::use_fn` first for each use function id (`pSpell`) the provider `has_use_fn`, also ids past
  D2Gamma's 30 (the item then counts as usable), with the desk as `UseHost` (units, item record and code, unit
  stats, draws on a unit's seed) and the call (`UseCall`: id, books skill, player, item, target, point).
  `Some(false)` refuses with D2Gamma's failure reset; a used item is consumed as a used scroll is (a tome
  loses a charge); `None` runs D2Gamma's body, and an id past D2Gamma's is then refused. Identify keeps its
  two steps.
- MS7d item generation, PR #281. The quality dispatch (`quality::dispatch`) asks `ItemProvider::quality_fn`
  first for a quality the provider `has_quality_fn`, also qualities past D2Gamma's (a request names one), with the
  new item as `GenHost` (item level, draws on the item seed, file index, D2Gamma's property path for property
  records). `Some(false)` makes no item; `None` runs D2Gamma's routine with its downgrades, and a quality neither
  side knows makes no item. The item is then finished as D2Gamma's are (ethereal roll, automagic; sockets only for
  low, normal and superior). Left for later: a mod quality's own save fields (the save writes D2Gamma's per-quality
  fields only) come with the save column seams.

- MS7e stat callbacks, PR #327. The stat-list value-change dispatcher asks
  `ItemProvider::stat_callback` first for each stat id the provider `has_stat_callback`,
  including stats without a D2Gamma callback body. The loaded row must enable
  `fCallback` and its unit list must have a value callback. `ActionHooks` supplies
  the game's item provider. The hook receives the mutable stat lists, stat host
  and `CallbackEvent` (owner, propagation unit, stat/layer key, old/new total).
  `Some(())` replaces the server callback; `None` runs D2Gamma's item event registration,
  skill updates and vital rescaling. Trace notification still runs first. Hooks
  must avoid recursive writes to their triggering stat. Tests cover changed stat
  results, fallthrough, absent hooks, layers, attachment/removal and eligibility.

- **MS8 is split** (one table group per PR): MS8a cube ops and outputs, MS8b object operate, MS8c NPC interaction,
  MS8d per-quest callbacks, MS8e stores (store columns, vendor and repair lists, store refresh).
- **MS8a as built:** `ModRules::cube() -> Option<&dyn CubeProvider>` (`world/cube/provider.rs`), read through
  `CubeWorld::rules` (default `Original`; `EconomyCube` forwards the unit hooks' rules). `op_scope(op)` claims an
  op code at a scope (recipe on the player, slot-0 candidate, every candidate) and `op_test` is its test; codes
  it doesn't claim run D2Gamma's `op_info` tests (`cube::mod_op`). `output(slot)` answers before D2Gamma looks at
  the slot's kind: `Event(bool)` sets the transmute's success like the portal kinds, `Nothing` skips the slot,
  `Item(unit)` goes through D2Gamma's finishing and is placed on commit. Op codes and kinds are record bytes, so
  there is no slot count to extend. Tests: `world/cube/mod_cube_tests.rs`.
- **MS8b as built:** `ModRules::objects() -> Option<&dyn ObjectProvider>` (`world/objects/provider.rs`), read
  through `ObjectWorld::rules` (default `Original`; `ObjectView` forwards the action hooks' rules). In the operate
  dispatch, `has_operate(n)` claims a function number: a claimed number past D2Gamma's bound (101) is the mod's
  slot, not a fatal error, so the mod sets its own slot count. A claimed function runs first (`operate`, on
  `dyn ObjectHost`); `None` falls through to D2Gamma's route (chests, quest objects, waypoints ...). The player
  refusals and refused classes stay D2Gamma's and run first. Tests: `world/objects/mod_object_tests.rs`.
- **MS8c as built:** `ModRules::npcs() -> Option<&dyn NpcProvider>` (`world/npc/provider.rs`), read through
  `NpcWorld::rules` (default `Original`; the interaction `Desk` forwards the unit hooks' rules). `roles(class)`
  gives an NPC class's `NpcRoles` (trade, gamble, heal, identify, hire, resurrect); `None` keeps D2Gamma's lists,
  so a mod gives its own NPC classes roles and may change D2Gamma's. `menu_action` runs a C→S 0x38 action (on
  `dyn NpcHost`) before D2Gamma's; `None` runs D2Gamma's. The message checks (size, unit check, `interact`, an
  interaction list) stay D2Gamma's and run first. Classes and actions are monstats rows and message fields, so
  there is no slot count. Tests: `world/npc/tests/mod_npc_tests.rs`.
- **MS8d as built:** `ModRules::quests() -> Option<&dyn QuestProvider>` (`world/quests/provider.rs`), read
  through `QuestWorld::rules` (default `Original`; `EconomyQuests` forwards the unit hooks' rules and
  `HostQuests` its inner world's). A quest record's four functions go through the router in
  `world/quests/act1.rs`, and the provider answers first for each: `callback` (an event; `Some(false)` reports
  it unhandled), `active` (NPC quest talk), `status` (quest-log state) and `sequence` (the walk to the next
  quest). `None` runs D2Gamma's code, so a mod replaces single quests or single events. Which events reach a
  record stays its callback bits from the quest table rows. The quest rows and flag records are the save
  format, so there is no slot count: a mod's quests reuse D2Gamma's chains (new chains need a save-format
  seam). Quest timers stay D2Gamma's. Tests: `world/quests/mod_quest_tests.rs`.
- **MS8e as built:** `ModRules::stores() -> Option<&dyn StoreProvider>` (`world/vendors/provider.rs`), read
  through `VendorWorld::rules` (default `Original`; `VendorDesk` forwards the unit hooks' rules and the server's
  `InvVendors` its inner world's). `roles(class)` gives an NPC class's `StoreRoles` (store column, global-list
  column, flag bytes, no refresh at trade open, repairs); `None` keeps D2Gamma's lists. Gamble lists and hire
  lists follow the class's MS8c `NpcRoles`. Game creation takes the rules through `GlobalLists::build_for`,
  `VendorRecord::new_for` and `InteractionState::new_for`; the old constructors are D2Gamma's. The server builds
  D2Gamma's until MS15 passes a game's rules. Store columns are the item tables' 17 vendor columns (a record
  layout), so there is no slot count; a column past them is dropped. Tests:
  `world/vendors/mod_store_tests.rs`.

### MS9: calc registry

- **What:** a mod game adds calc keywords and opcodes per family (skills, missiles, items).
- **Files:** `d2-data/src/calc.rs` (parse unknown keywords through a registry passed by the loader),
  `d2-sim/src/skills/calc.rs` (unknown opcodes call `ModRules::calc()`).
- **Seam:** `d2_data::calc::CalcRegistry` (per family: `ModKeyword { name, arity ≤ 3, names_like }`); a keyword
  gets the function index after the family's built-in keywords (skills 7, missiles 5, items 4) and compiles to
  CALL. The loader takes it through `compile_all_with` / `StdCallbacks::with_calc` / `calc::compile_with`;
  `compile_all` and `compile` pass an empty registry. "Opcodes" are those CALL indices: no new bytecode
  opcodes. At run time `ModRules::calc() -> Option<&dyn CalcProvider>` (`registry()`, `call(family, index,
  args, ctx)`); the evaluator sends a CALL past the family's own table to it when the registry has that index,
  and otherwise pushes 0 as D2Gamma does. Contexts find the rules through `SkillUnits::rules` (skills,
  missiles) and `LifecycleHooks::rules` (items), defaulting to `Original`. Carrying the registry into the
  game builder's table load is MS2's.
- **Done when:** a test keyword compiles and evaluates; D2Gamma's buffers byte-identical.

### MS10: mod table data (MS10a-d)

- MS10a string-table layers (`tbl` add/change by key), PR #202. Done as `stradd <key> <text>` / `strset <key> <text>`
  statements in `d2patch 1` (outside table zones), a `StringLayer` over `StringTables` (changed keys keep their id,
  added keys get ids from 30,000, A18/A19/A20), applied by `compile_patched`, the native loader and the client's
  string lookup.
- MS10b ids and rows past today's limits (skills, itemstatcost, properties, levels): limits from the loaded data
  under `Mod`, PR #218. Done as `d2_data::bin::DataRules` (`Original`, `Mod`), held by `BinSet::rules` and set
  by `bin::load_from_with`; `Ruleset::data_rules()` gives `Mod` under `Mod`, and the game builder passes it to the
  load. Under `Mod` `itemstatcost` and `levels` reach 65,535 rows (16-bit ids, 0xFFFF free) instead of 511 and
  1,023; byte ids (`states`, `pettype`, 255) and signed 16-bit ids (skills, uniques, sets, monsters, 32,766) keep
  their width, and skills (3,018 rows for the largest known mod) already fit. `compile_patched` checks the
  patched set under the live set's rules. Properties have no row limit, and the simulation already sizes its
  tables from the loaded rows. Limits that are wire or save widths stay: stat ids past 510 don't fit D2Gamma's
  9-bit item stat id, and the magic prefix and automagic offsets of the item bit layout are D2Gamma's row counts
  (see MS10c).
- MS10c side tables shipped by a mod game (own `.txt` files under `files/tables/`, never named like a game table)
  and loaded into a keyed store the mod reads; the item bit layout reads extension columns when present.
  PR #221. Done as `d2_data::side::SideTables` (`from_files`, by name, row and column: `cell`, `int`, `find`;
  a later layer replaces a table of the same name; game table names and non-`.txt` files are refused) fed by
  `d2_native::layers::LayerStack::side_tables` (`SIDE_DIR`), and `d2_sim::rules::apply_save_columns`, which
  overrides the item tables' stat save widths from the `itemstatcost` extension columns a mod names in
  `SaveColumns`. The game builder (MS2) calls both under `Mod`. The 9-bit stat id and the magic prefix and
  automagic offsets of the item bit layout stay D2Gamma's; a mod game that needs them wider adds a seam.
- MS10d compile tolerance under `Mod`, PR #222. `compile_table_with` / `compile_all_under` take the data rules
  (`DataRules`, from `Ruleset::data_rules`); under `Mod` an out-of-range integer is clamped into its field's range
  instead of wrapping. Over-long text is cut as before, and a missing link stays "none" (not 0, which would link
  to row 0). The patch check compiles the patched set under the live set's rules: under `Mod` the TextCut,
  IntRange and LinkMiss diagnostics are reported as C04 (a note, the patch still applies) instead of C02, and so
  are StrMiss (a missing string key, read as no string) and CbMiss (a name a compile callback doesn't know, such as a
  mod game's own cube output, read as none; the mod's rules read such names from its own data).
- MS10e data-layer limits for mod tables. Patch layers take lines up to 65,536 bytes and tokens up to 32,768
  (`MAX_LINE`, `MAX_TOKEN`), so long strings fit. Element 0 of `string.tbl` can be changed by `strset`
  (`StringTables::lookup` tells id 0 from "no string"). Under `Mod` the `experience` table may have more than 101
  rows (`DataRules::grows`, `patch::rules_for(DataRules::Mod)` lets a layer add rows to it) and an `automap`
  `LevelName` may be a level type number from 1 to 255 (`automap_level_number`), with the level-name ranges sized
  to the highest one.
- MS10f mod tables in their own compiled forms. Under `Mod` a `.bin` whose records are wider than D2Gamma's layout
  (the mod's columns after D2Gamma's, such as a `runes` record with more property slots) loads: the tables hold
  D2Gamma's columns and `BinSet::wide` the full records by table name, for the mod's own code
  (`d2_data::wide::parse_under`; hashed into the game-data hash only when present). An `automap` table whose
  `LevelName` cells hold the level type numbers themselves (a little-endian integer, the form a compiler that
  resolves the name writes) is read as numbers when any cell starts with a control byte (`d2_data::automap_index`).

### MS11: event hooks

`ModRules::events()`: game created/freed, player joined, unit killed (killer, victim), periodic (every n base
ticks), level entered, chat command. Called at fixed points in the tick, after D2Gamma's own work for that point.
Per-game and per-player mod state lives in the mod game's own `ModRules` object and its own saves.

- **As built:** `d2_sim::rules::{EventHooks, EventHost}` (`rules/events.rs`); the action wiring's `CombatView` is
  the host and `wiring/action/mod_events.rs` fires the hooks. Fire points: `reaction::kill_by` (players and
  monsters), the end of `join_sequence`, the end of `client_level_change` (a real change only), the new
  `TickHooks::end_of_tick` (periodic, counted in base ticks so a faster tick rate keeps the timing), and the server
  session layer for game created (the creating 0x67) and freed (the last client leaves). The chat command is asked
  in the server's chat handler for open lines (not whispers) after the text filter; `true` takes the line.
  Under `Original` each point costs one check of the ruleset.
- **Per-game rules:** `ModRules::for_game(&NewGame)` (`rules/game_state.rs`) makes a game's own rules object; the
  game builder (`d2-server` `build_game`) asks once per game, before any part is built, so a server's games never
  share mod state. Default `None`: one shared object. `EventHost` also gives the game's seed, difficulty and a
  player's hardcore flag.
- **Units on the event host:** `EventHost::{set_stat_layer, stat_layers}` (stats at any layer), `owner` (the
  player owning a summon or hireling), `monster_type` (champion, unique, minion, superunique row), `spawn_monster`
  (near a unit, through population's placement when the monster world is lent) and `grant_skill` (a monster's
  skill entry).
- **Gates and setup hooks:** `EventHooks::may_enter` (warp tile, waypoint and portal, after D2Gamma's own checks;
  `false` keeps the player where it is) and `may_join` (after the character loads; `Some(code)` refuses the join
  with that load result code). `player_left` runs before the player is removed, `pet_created` once a summon is on its
  owner's pet list, `hireling_ready` at the end of the tick a hireling was hired, revived or restored.

### MS12: dropped

The `.d2rmod` save sidecar is dropped (owner, 2026-10-17): each mod game has its own saves, in its own folder and
format, chosen through the builder (MS2). Extra stash pages and a shared stash are the mod game's own save format.

### MS13: mod message envelope

The envelope ids of decision 5 in `d2-proto` (`[id][u16 length][payload]`), a server dispatch hook
(`ModRules::message`) and a client dispatch hook; the mod game versions its payloads with its own version byte.
Under `Original` both ids stay invalid and are rejected like any other unknown id.

Done in PR #263:
- `d2_proto::envelope`: the ids, `encode`, `size`, `payload`, and `Wire { Original, Mod }`. `transport`'s `*_on`
  functions size, classify and split envelopes on `Wire::Mod` only. A whole envelope is at most 0x1FC bytes (what
  the server's queue drain copies of one C→S message), so a payload is at most 0x1F9 bytes.
- Server: `Host::set_wire` and the queue, buffer and receive-list paths take the wire. A drained C→S envelope goes
  to `Intents::mod_message` (`Handled::Mod`), not the dispatcher. `SimGame` hands it to `ModRules::message` with a
  `ModOutbox` that sends S→C envelopes to a player's client, but only under `Ruleset::Mod`.
- Client: `Bridge::set_mod_messages(hook)` switches the bridge to the mod wire. `Bridge::send_mod` sends a payload,
  and each frame's or chunk's S→C payloads go to `ModMessages::message` in receive order. Single player sets the
  host wire from the ruleset (`Started::wire`); the mod game sets the bridge hook.

### MS15: shared server and session layer

- **What:** a mod game's server is D2Gamma's server built with the mod game's `Ruleset`: one host loop, one session
  layer (join, leave, reconnect grace, per-client queues), one transport. Mod games don't fork it.
- **Files:** `d2-server` (host, sessions, seams: a public builder that takes `Ruleset`, tables and layers like
  MS2), `d2-net` (transport), and the dedicated server binary as a thin wrapper a mod game can copy.
- **Seam:** `ServerBuilder { data, rules, layers, characters }`; the envelope dispatch of MS13 is reached through
  it. A handshake field names the game (D2Gamma or the mod game's name and version), so a client of one game never
  joins another.
- **Done when:** a test runs two in-process clients on a server built with a test `Mod` ruleset and exchanges one
  envelope message each way; D2Gamma's own server tests unchanged.
- **Coordination:** built with the MP lane, after multiplayer M1-M3. Multiplayer integration is in review (#165); modules target main after it lands,
  while multiplayer stays out of releases.
- **As built (game servers):** `d2rust-node` is a library plus D2Gamma's thin binary. A mod game's node binary
  calls `d2rust_node::main(NodeGame { program, game, version, layers, rules })`. `rules` is a
  `d2_games::GameRules`: the `Ruleset` every game is built with and the `CharacterCodec` the seats' records are
  read and the saves written with (`Games::with_rules`; `Games::new` keeps `Original` and `.d2s`). The node's
  `NodeHello` names `NodeGame::game` and `version`; a main server of another game refuses it with `NodeGame`.
  `--check` fails on a bundle written for another game; a running node logs a warning and lets the main server
  refuse it. `--probe` plays the probe game under the node's rules. `NodeGame::own()` is D2Gamma's
  (`d2rust-node`, game `d2rust`, the build's version, `base/`, `Original`, `.d2s`), so its node is unchanged.
  Tests: `d2rust-node` `seats::mod_game_tests` (the hello and the refusal; a game the main server starts runs
  the tiny mod's join hook) and `d2-games` `a_mod_games_server_plays_its_rules_and_its_save_format`. E3 (#847)
  makes the realm's game id and chat label settings for separate mod realms.

### MS14: client hooks (MS14a-c)

A client-side `ClientMods` registry in `d2-client` (Bevy stays in the client; it holds no game rules): MS14a HUD
text lines and hover-item lines, MS14b panels (add/replace) and the item-name draw filter, MS14c per-category
audio gains.

- **Seam:** `d2_client::mods::ClientMods`, a Bevy resource the mod game's binary inserts before the UI is added
  (`app::play::PlayConfig::mods` for the windowed client, E1; D2Gamma's empty one is not inserted). Built
  with `ClientMods::new().with_…(hook)`; hooks of one kind run in registration order. Hooks only read the client
  model (`ClientWorld`, decoded `ItemBits`) and change what is drawn; with no hook the UI has the same panels and
  draws as before.
- **MS14a (as built):** `HudLines::lines(world, out)` gives `HudLine { text, at, font, color }` in the 800 × 600
  UI box, drawn by `ui::mod_hud::ModHudUi` (panel 0x116, added right above the HUD only when a source is
  registered; no input). `ItemLines::lines(item, lines)` runs over an item's hover-tip lines as D2Gamma built them
  (`ItemTips::tip_lines_of`: inventory, stash, cube, store); `OriginalUi::set_mods` hands the registry to the
  panels and tips. Tests: `mods::tests`.
- **MS14b (as built):** `with_panel(PanelPlace, PanelFactory)`. Panels aren't `Send`, so the registry holds
  factories and the UI makes the panels at install (`PanelFactory::make(&UiFiles)`; `files()` names the art,
  registered after the game's files so no id moves). `PanelPlace::Add { open }` goes above the game's panels and
  below the Esc menu and cursor layer (ids from `MOD_PANEL_IDS` 0x1000 are free for mods; a panel that is not a UI
  state is open for good or never); `PanelPlace::Replace` swaps the game panel with the same id in place
  (`UiRoot::replace`: same order, same open state, a UI state panel keeps following its flag).
  `ItemNameFilter::filter(world, item, &mut ItemLabel { text, color, shown })` runs over each ground item label (Show
  Items) in `ItemTips::label_of`; `shown = false` leaves the item without a label.
- **MS14c (as built):** `with_audio_gains(AudioGains)`; `AudioGains::percent(SoundCategory, id)` (0-1000, 100
  unchanged) scales a playing sound's volume after the player's master/music volume and the ducks, before
  distance falloff (`SoundSystem::compute`). `SoundCategory` is D2Gamma's own split: `Music` (songs, the music path,
  rows that follow the music volume), `Speech` (the speech ids), `Effects` (the rest); the sound id is passed too,
  so a mod game can split further. Set through `SoundDriver::set_mods` when the play client is installed.
- **MS14d (as built, mod game client seams):** `with_mod_messages(make)` makes a `ModMessages` hook per game;
  `app::play::install_game` hands it to `Bridge::set_mod_messages`, so the bridge speaks the mod wire like its
  host. Panels may answer `UiResponse::Actions(Vec<PanelAction>)` (several intents, `Open`/`Close` of another
  panel: a mod panel in the root, a UI state panel through `SetUIState`; a UI `Sound`; a white `Note` line),
  and get `Panel::tick(&PanelTick { ui, tables, open })` once per UI frame and `Panel::key_down(vk)` for each
  key-down (Windows virtual-key codes; the key still runs its bound action), both only while the mod game has
  panels. `bridge::skill_level(world, tables, unit, skill)` is a skill's level with its bonuses.
  `with_automap_markers(AutomapMarkers)` adds crosses (and names) at subtile spots after the units' markers.

### E1: the windowed game takes a mod game's inputs

The headless builder (MS2) took rules and saves, but the windowed game (`app::play`) always ran `Ruleset::Original`
with D2Gamma's saves. E1 gives the window the same inputs, so a mod game's binary plays in a window without forking
`d2-client`. D2Gamma's exe passes its own values and plays exactly as before.

- **Seam (as built):** `app::play::PlayConfig` gains `rules: Ruleset`, `saves: Saves` (the folder and the
  `CharacterCodec` the save share and the shared stash write with) and `mods: ClientMods` (inserted by
  `install_play`, so a game the main menu starts has them too; `run_with_mods` stays as a shorthand). The game is
  started through `GameBuilder` with those rules. When the host speaks the mod wire (MS13), the window's bridge
  speaks it too and hands S→C mod messages to `ClientMods::with_mod_messages` (dropped without one).
- **Layers:** tables and art come in through `PlayConfig::data`: `GameSource::Native(dir, Layers::List(..))`, or
  the new `GameSource::LayeredArchives(dir, archives)`, which opens a mod game's own archives above the install's
  (`ArchiveSet::open_layered`: the first listed is searched first).
- **Identity:** `d2_client::identity::GameIdentity` (name, version, the line under it, window title, the folder
  under Documents, a credits text hook), installed once by the mod game's `main` (`identity::install`) before
  `user_dir::init`. The windows, the main menu's and the automap's version lines, the dev console, the crash
  dialog and log, the Documents folder (saves, settings, logs, cache) and the credits screen read it. Without an
  install it is D2Gamma's own, with the same strings and folder (old folders migrate only into D2Gamma's own).
- **Example:** `examples/mod-game -- --game-dir DIR [--archive FILE].. --window` plays the example in a window
  with its rules, its name and its own `Documents\ExampleModGame` folder.
- **Tests:** `identity::tests`, `mpq::mutant_tests_archive::archive_set_layers_go_above_the_install`, the example's
  `headless.rs`, and the real-data `app_mod_game` (the windowed game runs the mod rules' events, speaks the mod
  wire and saves in the game's own format and folder).
- **Not in E1:** the front end (main menu, character screens) is still the D2Gamma exe's own flow in `main.rs`; a
  mod game starts its window at a character it chose. Lifting the front-end flow into the library is a later
  module (the MXL plan's X8/X10).

## Not planned

- Loading or selecting mods in the D2Gamma exe, or a launcher that starts "D2Gamma, then a mod": a mod game is
  launched directly as its own binary.
- A closed or versioned plugin API: seams are added as mod games need them.
- Mod data in D2Gamma saves (see MS12).
- Header renames in patches (`renamecol`): the mod's generator writes to the base column name.
- Shipping any mod's code or data in D2Gamma, even as an example: tests and the MS16 example use small synthetic
  overrides.
