#!/bin/sh
# local-realm.sh: a small setup on one machine for development: d2rust-realm on UDP 6112 and
# d2rust-node on UDP 6113, two processes linked over loopback (plan N5).
#   sh tools/server/local-realm.sh [--release]
# The first run makes the folder (D2RUST_LOCAL_DIR, default target/local-realm): the main
# server's config, identity and data, and game server n1 (`d2rust-realm --add-node`: its entry
# in the game-server list, its bundle and identity). Later runs keep accounts and characters.
# It builds both programs, starts them, waits for their ready lines and writes servers.toml next
# to the game exe of the same profile with one server, `local`, pinned to the main server's
# certificate; then the game plays against the pair:
#   cargo run --features online -p d2-client -- play --online --server local
# Game data from D2_GAME_DIR. Ctrl+C stops both (the game server saves its games first). Their
# logs are in the folder's logs/ and n1/logs/.
set -u
profile=
[ "${1:-}" = --release ] && profile=--release
: "${D2_GAME_DIR:?D2_GAME_DIR is not set}"
dir=${D2RUST_LOCAL_DIR:-target/local-realm}
out=${CARGO_TARGET_DIR:-target}/$([ -n "$profile" ] && echo release || echo debug)
exe=
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) exe=.exe ;; esac
cargo build --locked $profile -p d2rust-realm -p d2rust-node || exit 2
out=$(cd "$out" && pwd)
realm=$out/d2rust-realm$exe
node=$out/d2rust-node$exe
mkdir -p "$dir/n1"
dir=$(cd "$dir" && pwd)
if [ ! -f "$dir/d2rust-realm.cfg" ]; then
    printf 'listen = 127.0.0.1:6112\ngame_data = %s\ndata = data\nlogs = logs\nregistration = open\n' \
        "$D2_GAME_DIR" >"$dir/d2rust-realm.cfg"
fi
if [ ! -f "$dir/node-n1.bundle" ]; then
    (cd "$dir" && "$realm" --config d2rust-realm.cfg --add-node n1 --name Local --location local \
        --realm d2rust --realm-addr 127.0.0.1:6112 --out .) || exit 2
fi
if [ ! -f "$dir/n1/d2rust-node.cfg" ]; then
    printf 'listen = 127.0.0.1:6113\nbundle = %s\nidentity = %s\ngame_data = %s\nlogs = logs\n' \
        "$dir/node-n1.bundle" "$dir/node-n1.identity" "$D2_GAME_DIR" >"$dir/n1/d2rust-node.cfg"
fi
realm_pid=
node_pid=
stop() {
    [ -n "$node_pid" ] && kill "$node_pid" 2>/dev/null && wait "$node_pid" 2>/dev/null
    [ -n "$realm_pid" ] && kill "$realm_pid" 2>/dev/null && wait "$realm_pid" 2>/dev/null
}
trap stop EXIT INT TERM
# Waits for the ready line in $1 of process $2 (named $3).
wait_ready() {
    i=0
    until grep -q '^ready ' "$1" 2>/dev/null; do
        i=$((i + 1))
        if [ $i -gt 600 ] || ! kill -0 "$2" 2>/dev/null; then
            echo "local-realm: $3 did not start:"; tail -20 "$1"; exit 2
        fi
        sleep 0.2
    done
}
(cd "$dir" && exec "$realm" --config d2rust-realm.cfg >console.log 2>&1 </dev/null) &
realm_pid=$!
wait_ready "$dir/console.log" "$realm_pid" d2rust-realm
(cd "$dir/n1" && exec "$node" --config d2rust-node.cfg >console.log 2>&1 </dev/null) &
node_pid=$!
wait_ready "$dir/n1/console.log" "$node_pid" "d2rust-node n1"
ready=$(grep -m1 '^ready ' "$dir/console.log")
echo "local-realm: $ready"
echo "local-realm: $(grep -m1 '^ready ' "$dir/n1/console.log")"
cert=$(printf %s "$ready" | sed -n 's/.* cert_sha256=\([0-9a-f]*\).*/\1/p')
build=$(printf %s "$ready" | sed -n 's/.* build=\([^ ]*\).*/\1/p')
printf '[[server]]\nid = "local"\nname = "Local realm"\ngame = "d2rust"\naddress = "127.0.0.1:6112"\ncert_sha256 = "%s"\nversion = "%s"\n' \
    "$cert" "$build" >"$out/servers.toml"
echo "local-realm: wrote $out/servers.toml; play: cargo run $profile --features online -p d2-client -- play --online --server local"
echo "local-realm: running; Ctrl+C stops both"
wait "$realm_pid"
