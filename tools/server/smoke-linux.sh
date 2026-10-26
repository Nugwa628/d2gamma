#!/bin/sh
# smoke-linux.sh: the Linux server package starts on a plain Debian 13.
#
#   sh smoke-linux.sh <package.tar.gz> <game data folder> [program]
#
# Meant for a bare debian:trixie container (the server-build workflow's linux-smoke job), so a
# library the programs need but Debian's base system lacks shows up here. It unpacks the
# package to a temporary folder and checks that:
#   - every program in bin/ answers --version with the package's VERSION;
#   - [program] (default d2rust-realm, the main server) makes its identity, passes --check, starts on a loopback
#     port with the given game data, prints its `ready ...` line within $SMOKE_WAIT s (default
#     180), and on SIGTERM exits 0.
# Needs only sh, tar, gzip and coreutils. Exit 0 = passed. Everything it makes is removed.
set -eu
[ $# -ge 2 ] || { sed -n 2,4p "$0" | sed 's/^# \{0,1\}//'; exit 2; }
PKG=$1
GAME=$2
PROGRAM=${3:-d2rust-realm}
WAIT=${SMOKE_WAIT:-180}
[ -f "$PKG" ] || { echo "smoke: no package $PKG" >&2; exit 2; }
[ -d "$GAME" ] || { echo "smoke: no game data folder $GAME" >&2; exit 2; }
case $GAME in /*) ;; *) GAME=$(cd "$GAME" && pwd) ;; esac

T=$(mktemp -d)
pid=
cleanup() {
    [ -n "$pid" ] && kill -KILL "$pid" 2>/dev/null
    rm -rf "$T"
}
trap cleanup EXIT
trap 'exit 130' INT TERM
fail() { echo "smoke: FAIL $*" >&2; [ -f "$T/out" ] && tail -n 30 "$T/out" >&2; exit 1; }

if [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    echo "smoke: on $(. /etc/os-release && echo "$PRETTY_NAME")"
fi
tar -C "$T" -xzf "$PKG"
dir=$(find "$T" -mindepth 1 -maxdepth 1 -type d | head -n 1)
[ -n "$dir" ] && [ -f "$dir/VERSION" ] || fail "the package has no top folder with a VERSION"
ver=$(cat "$dir/VERSION")
for b in "$dir"/bin/*; do
    got=$("$b" --version 2>&1) || fail "$(basename "$b") --version: $got"
    [ "$got" = "$(basename "$b") $ver" ] || fail "$(basename "$b") --version printed '$got'"
    echo "smoke: ok $got"
done

bin=$dir/bin/$PROGRAM
[ -x "$bin" ] || fail "no bin/$PROGRAM in the package"
run=$T/run
mkdir -p "$run"
# A random high loopback port (a bare container has no tool to ask for a free one; the
# container's own network namespace keeps clashes unlikely).
port=$(( 20000 + $(od -An -N2 -tu2 /dev/urandom | tr -d ' ') % 20000 ))
cat >"$run/$PROGRAM.cfg" <<EOF
listen    = 127.0.0.1:$port
game_data = $GAME
data      = data
logs      = logs
identity  = identity
backups   = backups
EOF
"$bin" --config "$run/$PROGRAM.cfg" --new-identity >"$T/out" 2>&1 || fail "--new-identity"
"$bin" --config "$run/$PROGRAM.cfg" --check >>"$T/out" 2>&1 || fail "--check"

t0=$(date +%s)
"$bin" --config "$run/$PROGRAM.cfg" >"$T/out" 2>&1 &
pid=$!
ready=
while [ $(($(date +%s) - t0)) -lt "$WAIT" ]; do
    ready=$(grep -m 1 '^ready ' "$T/out" || :)
    [ -n "$ready" ] && break
    kill -0 "$pid" 2>/dev/null || fail "$PROGRAM exited before its ready line"
    sleep 0.5
done
[ -n "$ready" ] || fail "no ready line in $WAIT s"
echo "smoke: ok ready in $(($(date +%s) - t0)) s: $ready"
case $ready in *"build=$ver "*) ;; *) fail "the ready line names another build than $ver" ;; esac

kill -TERM "$pid"
rc=0
wait "$pid" || rc=$?
pid=
[ "$rc" = 0 ] || fail "$PROGRAM exited with $rc on SIGTERM"
echo "smoke: ok stopped cleanly on SIGTERM"
echo "smoke: passed"
