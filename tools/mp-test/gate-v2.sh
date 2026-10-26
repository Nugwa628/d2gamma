#!/bin/sh
# The v2 setups gate (plan N1): runs tools/mp-test/scenarios/v2 on a split:2 setup and on a host
# setup, each process its own program.
#   sh tools/mp-test/gate-v2.sh [mp-test args]
# Two pairs of programs: the stand-ins (mp-test-synth as the main server, mp-test-stub-node as the
# game servers, which only link; no game files) and the real ones, d2rust-realm and d2rust-node
# (game data from D2_GAME_DIR; skipped without it). Scenarios for modules not built yet are
# pending. MP_REALM and MP_NODE choose one pair instead (for example MP_REALM=d2rust-realm
# MP_NODE=d2rust-node). Extra arguments go to mp-test (for example --filter node).
# d2rust-node is the hostile test build (cargo feature `hostile`): it misbehaves only in a
# scenario that asks for it (`hostile MODE`), so the others run as on a release build.
# Exit code: 0 when no rule broke (pending scenarios don't count), 1 when one did, 2 when the
# build failed or a scenario doesn't parse.
# Reports: target/mp-test-v2/<realm>-<setup>/mp-test.md and mp-test.json.
# Niced like gate.sh; MP_NICE overrides the level.
set -u
n=${MP_NICE:-15}
if [ -n "${MP_REALM:-}${MP_NODE:-}" ]; then
    pairs="${MP_REALM:-d2rust-realm}:${MP_NODE:-d2rust-node}"
elif [ -n "${D2_GAME_DIR:-}" ]; then
    pairs="synth:stub d2rust-realm:d2rust-node"
else
    pairs=synth:stub
    echo "mp-test gate-v2: D2_GAME_DIR is not set; d2rust-realm and d2rust-node not run"
fi
pkgs="-p mp-test"
for p in $pairs; do
    r=${p%%:*} node=${p#*:}
    case $r in synth) ;; *) pkgs="$pkgs -p $r" ;; esac
    case $node in stub) ;; d2rust-node) pkgs="$pkgs -p $node --features d2rust-node/hostile" ;; *) pkgs="$pkgs -p $node" ;; esac
done
# shellcheck disable=SC2086
nice -n "$n" cargo build --locked $pkgs || exit 2
bin=${CARGO_TARGET_DIR:-target}/debug
rc=0
rm -rf target/mp-test-v2
for p in $pairs; do
    r=${p%%:*} node=${p#*:}
    for s in split:2 host; do
        out=target/mp-test-v2/$r-$(echo "$s" | tr : -)
        nice -n "$n" "$bin/mp-test" --setup "$s" --realm "$r" --node "$node" --out "$out" "$@"
        x=$?
        [ $x -gt 1 ] && x=2
        [ $x -gt $rc ] && rc=$x
    done
done
for f in target/mp-test-v2/*/mp-test.md; do
    [ -f "$f" ] && { echo; echo "==== $f"; cat "$f"; }
done
echo "mp-test gate-v2: exit $rc"
exit $rc
