#!/bin/sh
# The market window's realm source on a real main server over loopback:
#   sh tools/mp-test/online-market.sh [--release]
# Builds d2rust-realm and starts it on 127.0.0.1 in a fresh folder with the market on
# (`market = on`, game data from D2_GAME_DIR) and its test lines on (`dev = test`): its standard
# input is a named pipe the test writes `econ-seed` lines to, its answers go to console.log. Then
# d2-client's online_market test runs (feature `online`): two accounts, each with the window's
# RealmSource over its own session, open the market, search, post, buy, offer, answer, edit the
# shop, claim and check prices. The server is stopped at the end whatever happens.
# Exit code: the test's (0 = pass), 2 when the server did not start.
set -u
profile=
[ "${1:-}" = --release ] && profile=--release
: "${D2_GAME_DIR:?D2_GAME_DIR is not set}"
pid=
dir=
cleanup() {
    [ -n "$pid" ] && kill "$pid" 2>/dev/null && wait "$pid" 2>/dev/null
    exec 3>&- 2>/dev/null
    [ -n "$dir" ] && rm -rf "$dir"
}
trap cleanup EXIT INT TERM
out=${CARGO_TARGET_DIR:-target}/$([ -n "$profile" ] && echo release || echo debug)
dir=$(mktemp -d)
cargo build --locked $profile -p d2rust-realm || exit 2
bin=$(cd "$out" && pwd)/d2rust-realm
printf 'listen = 127.0.0.1:0\ngame_data = %s\ndata = data\nlogs = logs\nregistration = open\ndev = test\nmarket = on\n' \
    "$D2_GAME_DIR" > "$dir/d2rust-realm.cfg"
mkfifo "$dir/console.in"
# Held open read-write here, so the server never reads an end and the test's writes never block.
exec 3<>"$dir/console.in"
(cd "$dir" && exec "$bin" --config d2rust-realm.cfg > console.log 2>&1 < console.in) &
pid=$!
i=0
until grep -q '^ready ' "$dir/console.log" 2>/dev/null; do
    i=$((i + 1))
    if [ $i -gt 600 ] || ! kill -0 "$pid" 2>/dev/null; then
        echo "online-market: d2rust-realm did not start:"; tail -20 "$dir/console.log"; exit 2
    fi
    sleep 0.2
done
ready=$(grep -m1 '^ready ' "$dir/console.log")
echo "online-market: $ready"
D2RUST_SMOKE_ADDR=$(printf %s "$ready" | sed -n 's/.* listen=\([^ ]*\).*/\1/p')
D2RUST_SMOKE_CERT=$(printf %s "$ready" | sed -n 's/.* cert_sha256=\([0-9a-f]*\).*/\1/p')
D2RUST_SMOKE_CONSOLE=$dir/console.in
D2RUST_SMOKE_LOG=$dir/console.log
export D2RUST_SMOKE_ADDR D2RUST_SMOKE_CERT D2RUST_SMOKE_CONSOLE D2RUST_SMOKE_LOG
cargo nextest run --locked $profile -p d2-client --features online --test front_end --run-ignored only \
    --no-capture -E 'test(online_market)'
rc=$?
echo "online-market: main server console (tail):"; tail -15 "$dir/console.log"
echo "online-market: exit $rc"
exit $rc
