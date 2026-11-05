# Studio dogfood findings

The input harness drives ordinary egui pointer, keyboard and wheel input. Its comparison reads the resulting project; it never constructs editor models or writes mod files. Recorded input mistakes are corrected in the tour and are not counted as product defects.

| Area | Finding | Fix / regression | PR |
| --- | --- | --- | --- |
| Named stats | No controls to declare the showcase stat. | Stat definitions, stable IDs and saved-file input regression. | #1245 |
| Difficulties | No controls to name a fourth difficulty. | Names/count editor, round trip and unchanged defaults. | #1248 |
| Acts | No controls for the act, town and waypoint registry. | Typed registry, bounds and save/load checks. | #1249 |
| Rendering | Overlapping native tile rectangles aborted frame construction. | Per-block fallback and kept/culled overlap regression. | #1251 |
| Navigation | Editor names were ambiguous or clipped in an asset-heavy project. | Explicit labels and painted-viewport checks. | #1252 |
| Manifest | Market/ladder switches and maker approval could not be authored. | Manifest controls and round trip. | #1254 |
| Assets | Native artist libraries and PNG icons could not be imported intact. | Import controls and byte-exact original asset tests. | #1257 |
| Maps | First level creation assumed base-game assets; placements needed text edits. | Project asset defaults and typed placement rows. | #1266 |
| Tables | Finding a cell or adding a string required excessive scrolling. | Table/row/column search, cell inspector and string forms. | #1267 |
| Visual graph | Naming/positioning nodes and wiring pins was awkward. | Portable rename, positions and typed pin/edge pickers; compiler/fake-event tests. | #1268 |
| Skills | Declared crowd control could not be authored on hit blocks. | Shared interrupt/knockback/stun/freeze forms and all-hit-block round trips. | #1269 |
| AI phases | The immunity state could not be declared. | Immune-state control preserving other declarations. | #1270 |
| Animation checks | Explicit mod towns used baseline town modes; sweeps started beside a waypoint or exit. | Effective town context and ordinary movement setup; all original 49 rate checks pass. | #1274 |
| Input harness | Wrapped text hit boxes and empty replacement input missed edits. | Painted glyph bounds and Backspace replacement regressions. | #1275 |
| Play | Launched games could not be stopped or their exit reported. | Owned child tracking, Stop game and real child-process test. | #1276 |
| Quests | Objective/reward buttons clipped in a normal-width pane. | Wrapped palettes; all registered buttons must fit. | #1278 |
| Input harness | A failed replay could loop in the dirty-map close dialog. | Script completion closes once; actual viewport close regression. | #1279 |
| Effects | The preview left too little room for properties. | Optional preview and viewport property checks. | #1280 |
| AI cooldowns | Adding a row changed the header identity and collapsed the form. | Stable header ID and actual add-row input regression. | #1281 |
| Input harness | Repaint counts raced egui animations under Xvfb. | Time-based pacing/scroll settling, fast-frame regression and independent quest saves. | #1284 |
| AI palette | Long lists required hunting; search must retain its popup while typing. | Search by name/label, explicit popup policy and a delayed input regression. | #1285 |
| Play command | Configured native installs left `--native` in the command slot and exited with usage. | Explicit `play` subcommand; full command regression and real window/Stop replay. | #1297 |
| Installed folder | Player logs/saves/cache/settings made the mod fail layout validation after Play. | Ignore only known runtime entries; package-byte equality, wrong-kind/link regressions and real Play/Stop. | #1298 |
| Add-on wizard | The add-on form called itself New mod. | Kind-specific heading and actual wizard input regression. | #1289 |

Studio intentionally owns `files/patches/~studio.d2patch` and `files/effects/studio.toml`, rather than the template's `lantern` filenames. The read-only diff applies guarded patches and collects named recipes through production readers; it compares every effective cell/string and declaration. The full runtime file inventory must otherwise match. TOML formatting/comments also differ; AI may omit the resolved default enemy target and first start state. Those defaults are normalized explicitly in the comparison. The template's README and license are distribution documentation, along with Studio's empty-directory `.gitkeep` placeholders, outside the runtime comparison; the tour writes no replacement text files for them. Artist library and icon bytes must match exactly.

The final fresh tour completes 1,984 inputs across 16 recordings and produces 36 captures. Both saved-document and effective-table comparisons pass before Play (`7d0d0282-88be-4c9b-9107-19da2e411aa4`) and after Play (`3da45d36-f2c2-49ed-b346-7a1646ab84f5`). Studio Check, both signed packages, Play/Stop, and the real folder/pack route and boss checks exit 0. The default Studio gate passes 177 tests (3 skipped by default; the two comparisons run explicitly above), all-target Clippy with warnings denied and the executable build; the game build with `dylib,online` also exits 0. Local format, pre-PR, diff and runner syntax checks exit 0.

Real gameplay passes 2/2 tests from the authored folder (`5de2fda9-bff2-40f0-9e29-e5da9b4afb15`, 132.755 seconds) and 2/2 from an installation containing only its signed packs (`d335807c-b9ec-4505-83fd-bb87c8f44510`, 129.923 seconds). The runtime-folder fix also passes all 88 modkit tests and its real Play/Stop regression.

The original 49 animation-rate cases now pass with zero Unverified, zero failed and zero skipped cases (148.518 seconds). The installed Check exits 0 with no warnings or errors; the two pre-existing inverted monster-range informational findings remain. The client gate passes 4,511 tests (397 installed-data cases skipped in that default run), with all-target Clippy also exiting 0.

Human-only checks remain animation smoothness/transitions and sprite alignment; wind-up/immune readability; combat feel/balance; tile seams and maze/set-piece appearance; HUD/log/add-on legibility at 720p and 4K; and exit discoverability. None of the original 49 numeric animation-rate cases remains Unverified.
