# Studio item tools

The Items, Drops and Cube pages share the table editor's current working set. Edits in an item form immediately appear in Tables, and Save writes the same `~studio.d2patch`. Friendly labels cover common columns; hovering a form label shows its actual table key. Custom columns keep their keys.

Items supports weapons, armor, miscellaneous items, unique items, set items, runewords, magic prefixes/suffixes and automagic. Search for a row and filter its fields. For affixes and runewords choose a base item to preview. Runewords check compatibility through the engine matcher, then apply the configured socket fillers and activate the word. Incompatible bases show an error. The tooltip updates after a short pause, including unsaved edits. It uses the same `d2-item-text` builder as the game client, after engine table compilation, fixups, item property application and item bitstream encoding/decoding. English strings come from the configured converted install, with patched strings applied. Damage uses a neutral class 0 viewer with no player bonuses; panel-unit requirements and client mod hooks have no player context in Studio.

Drops accepts a treasure class or a normal-rank monster, difficulty, seed, level and up to one million trials. It runs the engine treasure-class walker and item creation, counting the final quality of each item. The fixed scenario is expansion, one player, zero magic find and a six-drop limit; placement always succeeds. No-drop rolls produce no counted item. Seeded runs use a fresh simulation world each time.

Cube accepts one inventory item per line:

```
code, quality, level, quantity, sockets, ethereal
```

Only code is required; defaults are normal quality (2), level 1, quantity 1, zero sockets and non-ethereal. It runs the engine's recipe selection and transmute, reports the matched recipe and actual outputs, and builds output tooltips with the shared builder. The scenario uses expansion/ladder, player class 0, day 1 and weekday 1. Placement succeeds; portal outputs are reported as events. Input items do not model filled sockets, personal names, or specific unique/set identities. This is an item-and-recipe sandbox, not a save editor.

Table compilation and simulation run on worker threads over an immutable snapshot. A tooltip result for an older edit is discarded. Large simulations do not block editing. Runs use table and string edits; global rules and executable mod hooks are outside this sandbox.

`d2-item-text` contains the engine's existing renderer-free tooltip implementation and shared item decoder adapter. The client retains drawing and its item name/line hooks. The server reexports the shared adapter. Studio depends on the simulation library, without linking the game client, server or Bevy.

Validation uses invented tables in `tests/workbench_support`, including patched damage, seeded treasure weights and a cube recipe. `examples/render_workbench.rs` renders the actual eframe views with engine-computed results and no game assets:

```
xvfb-run -a cargo run -p mod-studio --example render_workbench -- items.png items
xvfb-run -a cargo run -p mod-studio --example render_workbench -- drops.png drops
xvfb-run -a cargo run -p mod-studio --example render_workbench -- cube.png cube
```
