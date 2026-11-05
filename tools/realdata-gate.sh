#!/bin/sh
# The real-data gate: every d2-client test marked
#   #[ignore = "real data: needs D2_GAME_DIR (tools/realdata-gate.sh)"]
# (and the other game-file tests of d2-client), run against the owner's own
# install on a machine that holds it: the CI server (/work/game) or a heavy
# runner (/cache/game). Prints one line per failed test and a pass/fail
# summary; the exit code is nextest's (0: all passed).
#
#   sh tools/realdata-gate.sh [nextest args...]
#   sh tools/realdata-gate.sh -E 'test(app_play_quests::)'   # one group
#
# Environment (defaults are the CI server's paths):
#   D2_GAME_DIR        the install's MPQs                  /work/game
#   D2_SAVE            a level-1 test character (.d2s)     /work/game-facts/saves/ScnSor.d2s
#   D2_CREDITS_SCENES  measured credits draw lists (TSV)   /work/game-facts/render
#   REALDATA_OUT       the full nextest log                $CARGO_TARGET_DIR/realdata-gate.log
#
# The tests that open a real game window (the `play` process tests: chat
# window, market, the control channel) need a display. Without DISPLAY or
# WAYLAND_DISPLAY the suite runs under `xvfb-run -a` (an X server in
# memory; wgpu then renders on the software Vulkan driver, lavapipe).
#
# `controls::original::tests::defaults_equal_game_exe_table` reads the
# install's Game.exe. A machine that holds only the MPQs (the CI server
# does) can't run it: the gate leaves it out and says so in the summary.
set -u

cd "$(dirname "$0")/.." || exit 2

: "${D2_GAME_DIR:=/work/game}"
: "${D2_SAVE:=/work/game-facts/saves/ScnSor.d2s}"
: "${D2_CREDITS_SCENES:=/work/game-facts/render}"
: "${REALDATA_OUT:=${CARGO_TARGET_DIR:-target}/realdata-gate.log}"
export D2_GAME_DIR D2_SAVE D2_CREDITS_SCENES

missing=
[ -f "$D2_GAME_DIR/d2data.mpq" ] || { echo "realdata-gate: no install at D2_GAME_DIR=$D2_GAME_DIR" >&2; exit 2; }
[ -f "$D2_SAVE" ] || missing="$missing D2_SAVE=$D2_SAVE"
[ -d "$D2_CREDITS_SCENES" ] || missing="$missing D2_CREDITS_SCENES=$D2_CREDITS_SCENES"
[ -n "$missing" ] && echo "realdata-gate: missing$missing (the tests that read them will fail)" >&2

# The tests left out on this machine, as a nextest filter.
skip=
skip_note=
if [ ! -f "$D2_GAME_DIR/Game.exe" ]; then
    skip="test(=controls::original::tests::defaults_equal_game_exe_table)"
    skip_note="not run: controls::original::tests::defaults_equal_game_exe_table (no Game.exe in D2_GAME_DIR)"
fi

# The caller's -E filter, narrowed by the left-out tests.
filter=
pass=
while [ $# -gt 0 ]; do
    case $1 in
        -E) filter=$2; shift 2 ;;
        *) pass="$pass $(printf "'%s'" "$(printf %s "$1" | sed "s/'/'\\\\''/g")")"; shift ;;
    esac
done
[ -z "$filter" ] && filter="all()"
[ -n "$skip" ] && filter="($filter) - $skip"

run=
if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    if command -v xvfb-run >/dev/null 2>&1; then
        run="xvfb-run -a -s '-screen 0 1920x1080x24'"
    else
        echo "realdata-gate: no display and no xvfb-run: the window tests will fail" >&2
    fi
fi

echo "realdata-gate: D2_GAME_DIR=$D2_GAME_DIR ${run:+(under xvfb-run)}; log $REALDATA_OUT"
eval "$run cargo nextest run --locked -p d2-client --features d2-client/dylib \
    --run-ignored only --no-fail-fast -E \"\$filter\" $pass" >"$REALDATA_OUT" 2>&1
rc=$?

# One line per failed test, then nextest's summary line.
grep -E '^ +(FAIL|TIMEOUT|SIGSEGV|SIGABRT|SIGKILL) ' "$REALDATA_OUT" | sed 's/^ *//' | sort -u -k4
summary=$(grep -E '^ +Summary ' "$REALDATA_OUT" | tail -1 | sed 's/^ *//')
if [ -n "$summary" ]; then
    echo "$summary"
else
    echo "realdata-gate: no nextest summary (build failed?); last lines:"
    tail -20 "$REALDATA_OUT"
fi
[ -n "$skip_note" ] && echo "$skip_note"
if [ $rc -eq 0 ]; then echo "realdata-gate: PASS"; else echo "realdata-gate: FAIL ($rc)"; fi
exit $rc
