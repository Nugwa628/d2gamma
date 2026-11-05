#!/bin/sh
# Run from the repository root on the installed CI game machine.
set -eu
: "${STUDIO_BIN:?set STUDIO_BIN to the Mod-Studio executable}"
: "${TOUR_HOME:?set TOUR_HOME to a fresh isolated player home}"
: "${TOUR_NATIVE:?set TOUR_NATIVE to the converted native install}"
: "${TOUR_GAME:?set TOUR_GAME to the D2Gamma-Game executable}"
: "${TOUR_OUTPUT:?set TOUR_OUTPUT to a fresh evidence directory}"
test ! -e "$TOUR_HOME/mods/showcase" || {
    echo "Tour requires a fresh project destination: $TOUR_HOME/mods/showcase" >&2
    exit 1
}
python3 tools/studio-tour/prepare.py --home "$TOUR_HOME" --native "$TOUR_NATIVE" \
    --game "$TOUR_GAME" --output "$TOUR_OUTPUT/input"
mkdir -p "$TOUR_OUTPUT/settings"
compare_showcase() {
    STUDIO_SHOWCASE_ROOT="$TOUR_HOME/mods/showcase" D2_SHOWCASE_NATIVE="$TOUR_NATIVE" \
        cargo nextest run --locked -p mod-studio --test showcase_diff --run-ignored ignored-only
}
for editor in foundation assets states graphs script-net skills maps rules ai quests \
    layout quest-layout effects tables addon; do
    if [ "$editor" = foundation ]; then
        set --
    else
        set -- "$TOUR_HOME/mods/showcase"
    fi
    xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 HOME="$TOUR_OUTPUT/settings" \
        D2RUST_HOME="$TOUR_HOME" "$STUDIO_BIN" "$@" \
        --input-script "$TOUR_OUTPUT/input/$editor.json" \
        --captures "$TOUR_OUTPUT/captures/$editor"
    echo "$editor: exit 0"
done
compare_showcase
xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 HOME="$TOUR_OUTPUT/settings" \
    D2RUST_HOME="$TOUR_HOME" "$STUDIO_BIN" "$TOUR_HOME/mods/showcase" \
    --input-script "$TOUR_OUTPUT/input/finish.json" \
    --captures "$TOUR_OUTPUT/captures/finish"
# A real Play session adds player files beside the installed project.
compare_showcase
python3 tools/studio-tour/collect.py --home "$TOUR_HOME" --output "$TOUR_OUTPUT"
