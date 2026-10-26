#!/bin/sh
# The online smoke of the shipped client on real servers over loopback:
#   sh tools/mp-test/online-smoke.sh [--release]
# Builds d2rust-realm and d2rust-node and starts them on 127.0.0.1, each its own process, in a
# fresh folder: the main server (new identity, open registration with the test limits: three
# per address a minute, game data from D2_GAME_DIR) writes game servers n1's and n2's bundles
# and identities, and both link to it. Address and certificate come from the main server's
# ready line. Then d2-client's online_smoke tests run
# (feature `online`; --release: the release profile, as shipped): register, create a character,
# create a game, join it on a game server through the main server's JoinNode (a second
# connection), enter the world; the create form's SERVER drop-down lists n1 and n2, and the
# game is created on the one chosen. The servers are stopped at the end whatever happens.
# D2RUST_SMOKE_ADDR=host:port D2RUST_SMOKE_CERT=<64 hex> skip the local servers (a remote one).
# D2_SHOTS=DIR also captures the drop-down on these servers (ui test real_realm_server_drop_down)
# into DIR.
# Exit code: the test's (0 = pass), 2 when a server did not start.
set -u
profile=
[ "${1:-}" = --release ] && profile=--release
: "${D2_GAME_DIR:?D2_GAME_DIR is not set}"
pid=
node_pids=
dir=
cleanup() {
    for p in $node_pids; do kill "$p" 2>/dev/null && wait "$p" 2>/dev/null; done
    [ -n "$pid" ] && kill "$pid" 2>/dev/null && wait "$pid" 2>/dev/null
    [ -n "$dir" ] && rm -rf "$dir"
}
trap cleanup EXIT INT TERM
# Waits for the ready line in $1 of process $2 (named $3).
wait_ready() {
    i=0
    until grep -q '^ready ' "$1" 2>/dev/null; do
        i=$((i + 1))
        if [ $i -gt 600 ] || ! kill -0 "$2" 2>/dev/null; then
            echo "online-smoke: $3 did not start:"; tail -20 "$1"; exit 2
        fi
        sleep 0.2
    done
}
if [ -z "${D2RUST_SMOKE_ADDR:-}" ]; then
    out=${CARGO_TARGET_DIR:-target}/$([ -n "$profile" ] && echo release || echo debug)
    dir=$(mktemp -d)
    cargo build --locked $profile -p d2rust-realm -p d2rust-node || exit 2
    bin=$(cd "$out" && pwd)/d2rust-realm
    node_bin=$(cd "$out" && pwd)/d2rust-node
    printf 'listen = 127.0.0.1:0\ngame_data = %s\ndata = data\nlogs = logs\nregistration = open\nlimits = test\ndev = test\nbundle_dir = bundles\ntest_nodes = n1,n2\n' \
        "$D2_GAME_DIR" > "$dir/d2rust-realm.cfg"
    (cd "$dir" && exec "$bin" --config d2rust-realm.cfg > console.log 2>&1 < /dev/null) &
    pid=$!
    wait_ready "$dir/console.log" "$pid" d2rust-realm
    realm_addr=$(grep -m1 '^ready ' "$dir/console.log" | sed -n 's/.* listen=\([^ ]*\).*/\1/p')
    for n in n1 n2; do
        mkdir -p "$dir/$n"
        printf 'listen = 127.0.0.1:0\nlogs = logs\nrealm_addr = %s\nbundle = %s\nidentity = %s\ngame_data = %s\n' \
            "$realm_addr" "$dir/bundles/node-$n.bundle" "$dir/bundles/node-$n.identity" "$D2_GAME_DIR" \
            > "$dir/$n/d2rust-node.cfg"
        (cd "$dir/$n" && exec "$node_bin" --config d2rust-node.cfg > console.log 2>&1 < /dev/null) &
        node_pid=$!
        node_pids="$node_pids $node_pid"
        wait_ready "$dir/$n/console.log" "$node_pid" "d2rust-node $n"
        echo "online-smoke: $n $(grep -m1 '^ready ' "$dir/$n/console.log")"
    done
    ready=$(grep -m1 '^ready ' "$dir/console.log")
    echo "online-smoke: $ready"
    D2RUST_SMOKE_ADDR=$(printf %s "$ready" | sed -n 's/.* listen=\([^ ]*\).*/\1/p')
    D2RUST_SMOKE_CERT=$(printf %s "$ready" | sed -n 's/.* cert_sha256=\([0-9a-f]*\).*/\1/p')
fi
export D2RUST_SMOKE_ADDR D2RUST_SMOKE_CERT
cargo nextest run --locked $profile -p d2-client --features online --test front_end --run-ignored only \
    --no-capture -E 'test(online_smoke)'
rc=$?
if [ $rc -eq 0 ] && [ -n "${D2_SHOTS:-}" ]; then
    # The tests above used this minute's three registrations.
    sleep 61
    cargo nextest run --locked $profile -p d2-client --features online --test ui --run-ignored only \
        --no-capture -E 'test(real_realm_server_drop_down)'
    rc=$?
fi
[ -n "$dir" ] && { echo "online-smoke: main server console (tail):"; tail -15 "$dir/console.log"; }
for n in n1 n2; do
    [ -f "$dir/$n/console.log" ] && { echo "online-smoke: $n console (tail):"; tail -15 "$dir/$n/console.log"; }
done
echo "online-smoke: exit $rc"
exit $rc
