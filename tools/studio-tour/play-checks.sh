#!/bin/sh
# Exercise only Studio-created content, after run.sh succeeds on the CI install.
set -eu
: "${TOUR_HOME:?set TOUR_HOME to the UI-created player home}"
: "${TOUR_OUTPUT:?set TOUR_OUTPUT to the tour evidence directory}"
: "${TOUR_NATIVE:?set TOUR_NATIVE to the converted native install}"
test -f "$TOUR_OUTPUT/tour-summary.json"
export D2_SHOWCASE_NATIVE="$TOUR_NATIVE"
export D2RUST_HOME="$TOUR_HOME"
# Player preferences are not mod authoring. The maker-approved add-on is opt-in.
printf '[enabled]\nshowcase=["showcase-notes"]\n' > "$TOUR_HOME/mods/addons.toml"
export D2_SHOWCASE_PACKED="$TOUR_HOME/mods/showcase"
xvfb-run -a cargo nextest run --locked -p d2-client --features dylib,online \
    --test app --run-ignored ignored-only --no-fail-fast \
    -E 'test(showcase_walk_) | test(showcase_arena_)'
mkdir -p "$TOUR_OUTPUT/folder-game"
for name in waypoint-state.json walk-quest-state.json walk-quest-values.json boss-arena.trace; do
    cp ".lane/dogfood-results/$name" "$TOUR_OUTPUT/folder-game/"
done
# Resolve a second installation containing only the two UI-produced signed packs.
export D2RUST_HOME="$TOUR_OUTPUT/packed-home"
mkdir -p "$D2RUST_HOME/mods"
for mod_id in showcase showcase-notes; do
    cp "$TOUR_HOME/mods/$mod_id-1.0.0.d2pack" "$D2RUST_HOME/mods/$mod_id.d2pack"
done
printf '[enabled]\nshowcase=["showcase-notes"]\n' > "$D2RUST_HOME/mods/addons.toml"
export D2_SHOWCASE_PACKED="$D2RUST_HOME/mods/showcase.d2pack"
xvfb-run -a cargo nextest run --locked -p d2-client --features dylib,online \
    --test app --run-ignored ignored-only --no-fail-fast \
    -E 'test(showcase_walk_) | test(showcase_arena_)'
mkdir -p "$TOUR_OUTPUT/packed-game"
for name in waypoint-state.json walk-quest-state.json walk-quest-values.json boss-arena.trace; do
    cp ".lane/dogfood-results/$name" "$TOUR_OUTPUT/packed-game/"
done
