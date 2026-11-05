# Studio input tour

Run from the repository root on Linux with Xvfb, a converted native install,
and built Studio/game executables. Heavy builds run on the CI server through
`remote.sh`. The recordings use the 1100×800 default Studio window.

```sh
export STUDIO_BIN="$CARGO_TARGET_DIR/debug/Mod-Studio"
export TOUR_GAME="$CARGO_TARGET_DIR/debug/D2Gamma-Game"
export TOUR_NATIVE=/work/native-k15
export TOUR_HOME="$PWD/target/studio-clean-home"
export TOUR_OUTPUT="$PWD/target/studio-clean-evidence"
export LD_LIBRARY_PATH="$CARGO_TARGET_DIR/debug/deps:$(rustc --print target-libdir):${LD_LIBRARY_PATH:-}"
sh tools/studio-tour/run.sh
sh tools/studio-tour/play-checks.sh
```

Use a fresh `TOUR_HOME` and `TOUR_OUTPUT` for another run. The runner refuses
to overwrite an existing project. `prepare.py` substitutes paths into input
JSON; it never writes mod files. Every project change comes from egui pointer,
keyboard or wheel events. The imported artist library and icon are original
showcase artwork, imported through Studio; gameplay definitions are recreated
through editor controls. Luau text is entered into Studio's script pane.

Each editor produces `result.json`, painted-label metadata and PNG captures.
A missing target or timeout exits nonzero. Saved-file comparison is a separate,
read-only gate: it checks typed declarations, all effective table cells and
strings, and the imported assets, both before and after Check/Package/Play.
Neither comparison test constructs the mod.

Studio owns `files/patches/~studio.d2patch` and `files/effects/studio.toml`.
These intentionally differ from the template's descriptive filenames; the
comparison applies patches and collects named effect recipes through the
production readers. Empty-directory `.gitkeep` scaffold placeholders and
distribution README/license text are outside the runtime comparison. TOML formatting/comments are also not
byte comparisons. The implicit AI enemy target and first start state are
normalized to their resolved defaults.

The final recording runs Check, Package and Play, then stops the owned game.
Package signing keys stay in the isolated home: do not publish or fetch that
whole home. Publish selected Studio-only PNGs, never game-window screenshots
or native game files. For the separate folder/pack gameplay route and boss
checks, run the installed `showcase_walk_` and `showcase_arena_` tests with
`D2_SHOWCASE_PACKED` pointing at the Studio-authored folder or package and
enable the authored `showcase-notes` add-on in the isolated player preferences.
`play-checks.sh` does this explicitly, then runs the route and boss checks first
against the authored folder and then an installation containing only its two
signed packs installed under their canonical mod-id filenames. It never copies
gameplay definitions from the reference template.
`tour-summary.json` records completed input counts and public package hashes.
