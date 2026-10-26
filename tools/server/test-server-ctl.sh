#!/bin/bash
# test-server-ctl.sh: checks server-ctl's install, switch, rollback, restart-on-crash, stop and
# backup paths against a fake server binary, its upgrades (signed releases, the countdown, the
# cancel), a second realm on the same box, a game server's fetch, swap on exit 75 and rollback, and the `nodes` table, in a temporary
# folder with cron mode and a crontab stub. Linux only.
#
#   bash tools/server/test-server-ctl.sh        exit 0 = every check passed
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
T=$(mktemp -d)
export D2RUST_SERVER_HOME=$T/home D2RUST_SERVER_MODE=cron D2RUST_SERVER_START_WAIT=8 D2RUST_SERVER_STOP_WAIT=5
# cron mode without touching the real crontab: a crontab stub on PATH.
mkdir -p "$T/stub"
printf '#!/bin/sh\n[ "$1" = -l ] || cat >/dev/null\n' >"$T/stub/crontab"
chmod 755 "$T/stub/crontab"
export PATH=$T/stub:$PATH
CTL=$D2RUST_SERVER_HOME/server-ctl
fails=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fails=$((fails + 1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }

# A fake d2rust-realm: `--version`, `--check`, `--new-identity`, `--fingerprint`, `--backup`,
# and a run mode that writes status.json, answers backup requests, and on SIGTERM records a save.
# HEALTHY=0 never writes status.json; CRASH_ONCE crashes once while the flag file exists. It
# follows the countdown's files as the main server does (a minute of the request is a second
# here): run/upgrade.request starts it, run/upgrade.cancel calls it off, and with
# run/upgrade.ready at zero it goes to phase swap; logs/upgrade.json says where it is.
fake() {
    local ver=$1 healthy=${2:-1} out=$T/fake-$1
    cat >"$out" <<EOF
#!/bin/bash
H=\$D2RUST_SERVER_HOME
case "\$*" in
  --version) echo "d2rust-realm $ver"; exit 0 ;;
  *--check*) exit 0 ;;
  *--new-identity*) mkdir -p "\$H/identity"; echo cert >"\$H/identity/cert.der"; exit 0 ;;
  *--fingerprint*) echo "cert_sha256 = 00ff"; exit 0 ;;
  *--backup*) echo "offline backup"; exit 0 ;;
  *--list-nodes*) echo "eu-1 Finland"; exit 0 ;;
esac
trap 'echo "$ver saved" >>"\$H/logs/saves.log"; exit 0' TERM
echo "$ver started" >>"\$H/logs/d2rust-realm.log"
if [ -f "\$H/crash-once" ]; then rm -f "\$H/crash-once"; exit 3; fi
up() { printf '{"phase": "%s", "version": "%s", "seconds_left": 0}\n' "\$1" "\$2" >"\$H/logs/upgrade.json"; echo "\$1 \$2" >>"\$H/logs/d2rust-realm.log"; }
phase=idle; target=; zero=0
up idle ""
while :; do
  [ $healthy = 1 ] && printf '{"version": "$ver"}\n' >"\$H/logs/status.json"
  if [ -f "\$H/run/backup.request" ]; then rm -f "\$H/run/backup.request"; echo "ok backups/x" >"\$H/run/backup.done"; fi
  if [ -f "\$H/run/upgrade.request" ]; then
    target=\$(sed -n 's/^version=//p' "\$H/run/upgrade.request")
    zero=\$((\$(date +%s) + \$(sed -n 's/^minutes=//p' "\$H/run/upgrade.request")))
    rm -f "\$H/run/upgrade.request"; phase=countdown; up countdown "\$target"
  fi
  if [ \$phase = countdown ] && [ -f "\$H/run/upgrade.cancel" ]; then
    rm -f "\$H/run/upgrade.cancel"; phase=cancelled; up cancelled "\$target"
  fi
  if [ \$phase = countdown ] && [ \$(date +%s) -ge \$zero ] && [ -f "\$H/run/upgrade.ready" ]; then
    phase=swap; up swap "\$target"
  fi
  sleep 0.2
done
EOF
    chmod 755 "$out"
    printf '%s' "$out"
}

bash "$HERE/server-ctl" setup >/dev/null
check "setup makes the folder and a 0600 config" '[ -d $D2RUST_SERVER_HOME/data ] && [ "$(stat -c %a $D2RUST_SERVER_HOME/d2rust-realm.cfg)" = 600 ]'
check "setup keeps a copy of server-ctl in the folder" '[ -x $CTL ]'

# game-data.sh cleans <out dir> before it makes it, so the guard must refuse every path
# it did not make itself. GUARD_ONLY stops just before the conversion (no cargo, no game).
GD=$HERE/game-data.sh
mkdir -p "$T/victim" && echo keep >"$T/victim/keep"
for out in "" "/" "$HOME" "$T/victim" "$T/victim/.." "relative/game"; do
    if D2RUST_GAME_DATA_GUARD_ONLY=1 sh "$GD" /nonexistent "$out" >"$T/gd" 2>&1; then
        bad "game-data refuses the out dir '$out'"
    else
        ok "game-data refuses the out dir '$out'"
    fi
done
check "game-data left the folder it refused alone" '[ -f $T/victim/keep ]'
if D2RUST_GAME_DATA_GUARD_ONLY=1 sh "$GD" /nonexistent "$T/fresh/game" >"$T/gd" 2>&1; then
    ok "game-data accepts a new out dir"
else
    bad "game-data accepts a new out dir"
fi
mkdir -p "$T/own/game" && : >"$T/own/game/.d2rust-game-data"
if D2RUST_GAME_DATA_GUARD_ONLY=1 sh "$GD" /nonexistent "$T/own/game" >"$T/gd" 2>&1; then
    ok "game-data replaces its own earlier out dir"
else
    bad "game-data replaces its own earlier out dir"
fi

# A first install whose binary never reports healthy has nothing to roll back to: it must
# stop, not be left restarting.
D2RUST_SERVER_START_WAIT=2 "$CTL" install "$(fake 0.5.0.0 0)" >"$T/out" 2>&1
check "a first install that never reports healthy fails" '[ $? != 0 ]'
check "and leaves no current version to restart" '[ ! -e $D2RUST_SERVER_HOME/current ]'
sleep 3
check "and does not restart it" '[ ! -f $D2RUST_SERVER_HOME/run/server.pid ] || ! kill -0 $(cat $D2RUST_SERVER_HOME/run/server.pid) 2>/dev/null'

"$CTL" install "$(fake 1.0.0.0)" >"$T/out" 2>&1
check "install of a first version succeeds" '[ $? = 0 ]'
check "install prints the version and the fingerprint" 'grep -q "running: d2rust-realm 1.0.0.0" $T/out && grep -q cert_sha256 $T/out'
check "install makes the identity once" '[ -f $D2RUST_SERVER_HOME/identity/cert.der ]'
pid1=$(cat "$D2RUST_SERVER_HOME/run/server.pid")

"$CTL" install "$(fake 1.1.0.0)" >"$T/out" 2>&1
check "install of a second version succeeds" '[ $? = 0 ]'
check "current is the new version, previous the old" '[ "$(basename $(readlink $D2RUST_SERVER_HOME/current))" = d2rust-realm-1.1.0.0 ] && [ "$(basename $(readlink $D2RUST_SERVER_HOME/previous))" = d2rust-realm-1.0.0.0 ]'
check "the old version was stopped with SIGTERM (it saved)" 'grep -q "1.0.0.0 saved" $D2RUST_SERVER_HOME/logs/saves.log'
check "the old process is gone" '! kill -0 $pid1 2>/dev/null'
check "server-ctl logs reads the server log" '"$CTL" logs 5 | grep -q "1.1.0.0 started"'

"$CTL" install "$(fake 2.0.0.0 0)" >"$T/out" 2>&1
check "install of a version that never reports healthy fails" '[ $? != 0 ]'
check "and rolls back to the version before" '[ "$(basename $(readlink $D2RUST_SERVER_HOME/current))" = d2rust-realm-1.1.0.0 ]'
sleep 2
check "the rolled-back version runs" 'grep -q "\"1.1.0.0\"" $D2RUST_SERVER_HOME/logs/status.json'

pid=$(cat "$D2RUST_SERVER_HOME/run/server.pid")
touch "$D2RUST_SERVER_HOME/crash-once"
kill -KILL "$pid"
sleep 4
pid2=$(cat "$D2RUST_SERVER_HOME/run/server.pid" 2>/dev/null)
check "a killed server is restarted (after a crash on the way)" '[ -n "$pid2" ] && [ "$pid2" != "$pid" ] && kill -0 $pid2'

"$CTL" backup >"$T/out" 2>&1
check "backup of a running server goes through the server" '[ $? = 0 ] && grep -q "^ok" $T/out'

"$CTL" stop
check "stop ends the server and sets the stop flag" '[ -z "$(cat $D2RUST_SERVER_HOME/run/server.pid 2>/dev/null)" ] || ! kill -0 $(cat $D2RUST_SERVER_HOME/run/server.pid) 2>/dev/null'
"$CTL" ensure
sleep 1
check "ensure does not start a stopped server" '[ ! -f $D2RUST_SERVER_HOME/run/supervisor.pid ] || ! kill -0 $(cat $D2RUST_SERVER_HOME/run/supervisor.pid) 2>/dev/null'
"$CTL" backup >"$T/out" 2>&1
check "backup of a stopped server runs offline" 'grep -q "offline backup" $T/out'

rm -f "$D2RUST_SERVER_HOME/run/stopped"
"$CTL" ensure
sleep 2
check "ensure starts the server again once the flag is gone" 'kill -0 $(cat $D2RUST_SERVER_HOME/run/server.pid) 2>/dev/null'

"$CTL" rollback >/dev/null 2>&1
sleep 2
check "rollback switches to the previous version" '[ "$(basename $(readlink $D2RUST_SERVER_HOME/current))" = d2rust-realm-1.0.0.0 ] && grep -q "\"1.0.0.0\"" $D2RUST_SERVER_HOME/logs/status.json'

# A rollback target that also never reports healthy is stopped, not left restarting.
bad=$(fake 1.0.0.0 0)
mv "$bad" "$D2RUST_SERVER_HOME/bin/d2rust-realm-1.0.0.0"
D2RUST_SERVER_START_WAIT=2 "$CTL" install "$(fake 2.1.0.0 0)" >"$T/out" 2>&1
check "an install whose rollback target is unhealthy fails" '[ $? != 0 ]'
check "and leaves no current version to restart" '[ ! -e $D2RUST_SERVER_HOME/current ]'
sleep 3
check "and does not restart it" '[ ! -f $D2RUST_SERVER_HOME/run/server.pid ] || ! kill -0 $(cat $D2RUST_SERVER_HOME/run/server.pid) 2>/dev/null'

# The units the package ships: the same text setup installs, with %h for the home folder.
D2RUST_SERVER_HOME='%h/d2rust-server' bash "$HERE/server-ctl" units "$T/units"
check "units writes the server, backup and timer units" '[ -f $T/units/d2rust-realm.service ] && [ -f $T/units/d2rust-realm-backup.service ] && [ -f $T/units/d2rust-realm-backup.timer ]'
check "and they name the home folder as %h" 'grep -qx "ExecStart=%h/d2rust-server/current --config %h/d2rust-server/d2rust-realm.cfg" $T/units/d2rust-realm.service'
check "and keep their comments as written (nothing run while writing them)" 'grep -q "^# Every connection is QUIC (d2-quic) over UDP, on the config.s listen address: 6112" $T/units/d2rust-realm.service'

# A fake d2rust-node: `--version`, `--check`, `--fingerprint`, and a run mode that writes
# status.json (HEALTHY=0: never), and exits 75 (an upgrade is staged) once the flag file
# exit75 appears in its folder, as d2rust-node does at an upgrade's zero.
fake_node() {
    local ver=$1 healthy=${2:-1} out=$T/fake-node-$1
    cat >"$out" <<EOF
#!/bin/bash
H=\$(dirname "\$2")
case "\$*" in
  --version) echo "d2rust-node $ver"; exit 0 ;;
  *--check*) exit 0 ;;
  *--fingerprint*) echo "cert_sha256 = 00ee"; exit 0 ;;
esac
trap 'echo "$ver saved" >>"\$H/logs/saves.log"; exit 0' TERM
echo "$ver started" >>"\$H/logs/d2rust-node.log"
while :; do
  [ $healthy = 1 ] && printf '{"version": "$ver"}\n' >"\$H/logs/status.json"
  if [ -f "\$H/exit75" ]; then rm -f "\$H/exit75"; echo "$ver swap" >>"\$H/logs/d2rust-node.log"; exit 75; fi
  sleep 0.2
done
EOF
    chmod 755 "$out"
    printf '%s' "$out"
}

# upgrade: fake signed releases in a folder, downloaded through file:// links (no network).
# rel <version> <healthy 1|0> <signing key> [glibc]: a package as server-build.sh makes it.
REL=$T/release
export D2RUST_RELEASE_BASE=file://$REL
rel() {
    local ver=$1 healthy=$2 key=$3 glibc=${4:-2.36} name=d2rust-server-$1 d
    d=$T/pkg/$name
    rm -rf "$d"
    mkdir -p "$d/bin" "$REL/server-$ver"
    cp "$(fake "$ver" "$healthy")" "$d/bin/d2rust-realm"
    cp "$(fake_node "$ver" "$healthy")" "$d/bin/d2rust-node"
    { cat "$HERE/server-ctl"; echo "# packaged with $ver"; } >"$d/server-ctl"
    chmod 755 "$d/server-ctl"
    cp "$HERE/d2rust-realm.cfg.example" "$d/"
    echo "$ver" >"$d/VERSION"
    echo "$glibc" >"$d/GLIBC"
    tar -C "$T/pkg" -czf "$REL/server-$ver/$name-linux-x86_64.tar.gz" "$name"
    (cd "$REL/server-$ver" && sha256sum "$name-linux-x86_64.tar.gz" >SHA256SUMS.linux &&
        rm -f SHA256SUMS.linux.sig && ssh-keygen -q -Y sign -f "$key" -n d2rust-release SHA256SUMS.linux 2>/dev/null)
}
current_ver() { basename "$(readlink "$D2RUST_SERVER_HOME/current" 2>/dev/null)"; }
if command -v ssh-keygen >/dev/null; then
    ssh-keygen -q -t ed25519 -N '' -C release -f "$T/relkey"
    ssh-keygen -q -t ed25519 -N '' -C other -f "$T/otherkey"
    rel 3.0.0.0 1 "$T/relkey"
    "$CTL" upgrade 3.0.0.0 >"$T/out" 2>&1
    check "upgrade without a release key is refused" '[ $? != 0 ] && grep -q release-signers $T/out && [ -z "$(current_ver)" ]'
    echo "d2rust-release $(cat "$T/relkey.pub")" >"$D2RUST_SERVER_HOME/release-signers"

    rel 3.0.0.0 1 "$T/otherkey"
    "$CTL" upgrade 3.0.0.0 >"$T/out" 2>&1
    check "upgrade of a release signed with another key is refused" '[ $? != 0 ] && grep -q "signature" $T/out && [ -z "$(current_ver)" ]'

    rel 3.0.0.0 1 "$T/relkey"
    echo tampered >>"$REL/server-3.0.0.0/d2rust-server-3.0.0.0-linux-x86_64.tar.gz"
    "$CTL" upgrade 3.0.0.0 >"$T/out" 2>&1
    check "upgrade of a package that doesn't match its signed sums is refused" '[ $? != 0 ] && grep -q "does not match" $T/out && [ -z "$(current_ver)" ]'

    rel 3.0.0.0 1 "$T/relkey" 99.0
    "$CTL" upgrade 3.0.0.0 >"$T/out" 2>&1
    check "upgrade of a package for a newer glibc is refused" '[ $? != 0 ] && grep -q "needs glibc 99.0" $T/out && [ -z "$(current_ver)" ]'

    "$CTL" upgrade 4.0.0.0 >"$T/out" 2>&1
    check "upgrade to a version with no release fails" '[ $? != 0 ] && grep -q "downloading" $T/out'

    rel 3.0.0.0 1 "$T/relkey"
    "$CTL" upgrade 3.0.0.0 >"$T/out" 2>&1
    check "upgrade of a signed release installs it" '[ $? = 0 ] && [ "$(current_ver)" = d2rust-realm-3.0.0.0 ] && grep -q "upgraded to 3.0.0.0" $T/out'
    check "and it runs" 'sleep 2; grep -q "\"3.0.0.0\"" $D2RUST_SERVER_HOME/logs/status.json'
    check "and server-ctl is the package's" 'grep -q "^# packaged with 3.0.0.0" $CTL'
    check "and the download is cleaned up" '[ ! -d $D2RUST_SERVER_HOME/downloads/3.0.0.0 ]'

    rel 3.1.0.0 0 "$T/relkey"
    D2RUST_SERVER_START_WAIT=3 "$CTL" upgrade 3.1.0.0 >"$T/out" 2>&1
    check "upgrade to a version that never reports healthy fails" '[ $? != 0 ]'
    check "and rolls back, keeping server-ctl" '[ "$(current_ver)" = d2rust-realm-3.0.0.0 ] && grep -q "^# packaged with 3.0.0.0" $CTL'

    # A download token, when there is one, goes to curl on stdin, never on its command line.
    printf 'secret-token-123\n' >"$D2RUST_SERVER_HOME/release.token"
    mkdir -p "$T/curlstub"
    printf '#!/bin/sh\necho "$*" >>%s/curl.args\ncat >>%s/curl.stdin\nexec %s "$@" </dev/null\n' "$T" "$T" "$(command -v curl)" >"$T/curlstub/curl"
    chmod 755 "$T/curlstub/curl"
    rel 3.2.0.0 1 "$T/relkey"
    PATH=$T/curlstub:$PATH "$CTL" upgrade 3.2.0.0 >"$T/out" 2>&1
    check "upgrade with a download token installs it" '[ $? = 0 ] && [ "$(current_ver)" = d2rust-realm-3.2.0.0 ]'
    check "and sends the token as a header on stdin" 'grep -q "^Authorization: token secret-token-123$" $T/curl.stdin'
    check "and never on a command line" '! grep -q secret-token $T/curl.args'
    rm -f "$D2RUST_SERVER_HOME/release.token"

    # The countdown: a running main server takes the request, counts down, and is installed only
    # once it says `swap` (every game saved).
    RLOG=$D2RUST_SERVER_HOME/logs/d2rust-realm.log
    rel 3.3.0.0 1 "$T/relkey"
    "$CTL" upgrade 3.3.0.0 --in 2 >"$T/out" 2>&1
    check "a countdown upgrade of a running main server installs it" '[ $? = 0 ] && [ "$(current_ver)" = d2rust-realm-3.3.0.0 ] && grep -q "upgraded to 3.3.0.0" $T/out'
    check "and only after the main server said swap" 'a=$(grep -n "^swap 3.3.0.0" $RLOG | cut -d: -f1); b=$(grep -n "^3.3.0.0 started" $RLOG | cut -d: -f1); [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ]'
    check "and the old one saved on its way out" 'grep -q "3.2.0.0 saved" $D2RUST_SERVER_HOME/logs/saves.log'
    check "and upgrade --status says how it went" '"$CTL" upgrade --status | grep -q "ok: upgraded to 3.3.0.0"'

    rel 3.4.0.0 1 "$T/relkey"
    "$CTL" upgrade 3.4.0.0 --in 30 >"$T/out" 2>&1 &
    job=$!
    for i in $(seq 50); do grep -q '"countdown"' "$D2RUST_SERVER_HOME/logs/upgrade.json" 2>/dev/null && break; sleep 0.2; done
    "$CTL" upgrade --cancel >"$T/out2" 2>&1
    check "upgrade --cancel calls the countdown off" '[ $? = 0 ] && grep -q "called off" $T/out2'
    wait "$job"
    check "and the upgrade ends without installing" '[ $? != 0 ] && grep -q cancelled $T/out && [ "$(current_ver)" = d2rust-realm-3.3.0.0 ]'
    check "and the old version still runs" 'sleep 1; grep -q "\"3.3.0.0\"" $D2RUST_SERVER_HOME/logs/status.json'
    "$CTL" upgrade --cancel >"$T/out2" 2>&1
    check "a cancel with no countdown is refused" '[ $? != 0 ] && grep -q "no countdown" $T/out2'

    rel 3.4.0.0 1 "$T/otherkey"
    "$CTL" upgrade 3.4.0.0 --in 30 >"$T/out" 2>&1
    check "a bad signature during the countdown calls it off" '[ $? != 0 ] && grep -q signature $T/out && grep -q "called off" $T/out && [ "$(current_ver)" = d2rust-realm-3.3.0.0 ]'
    check "and the main server heard the cancel" 'grep -q "\"cancelled\"" $D2RUST_SERVER_HOME/logs/upgrade.json'

    # A game server: its own folder, found from the script's place (no D2RUST_SERVER_HOME).
    N=$T/node
    nctl() { D2RUST_SERVER_HOME='' "$N/server-ctl" "$@"; }
    D2RUST_SERVER_HOME=$N bash "$HERE/server-ctl" setup --node >"$T/out" 2>&1
    check "setup --node makes a game server folder" '[ "$(cat $N/program)" = d2rust-node ] && [ -f $N/d2rust-node.cfg ] && [ ! -d $N/backups ] && [ -x $N/server-ctl ]'
    check "its config fetches upgrades with server-ctl" 'grep -qx "update_command  = server-ctl" $N/d2rust-node.cfg'
    nctl install "$(fake_node 3.0.0.0)" >"$T/out" 2>&1
    check "install of a game server version succeeds" '[ $? = 0 ] && grep -q "running: d2rust-node 3.0.0.0" $T/out'
    nctl backup >"$T/out" 2>&1
    check "a game server has no backup" '[ $? != 0 ] && grep -q "no characters" $T/out'
    cp "$D2RUST_SERVER_HOME/release-signers" "$N/"
    rel 3.5.0.0 1 "$T/otherkey"
    nctl fetch 3.5.0.0 >"$T/out" 2>&1
    check "fetch refuses a bad signature and stages nothing" '[ $? != 0 ] && grep -q signature $T/out && [ ! -d $N/downloads/3.5.0.0 ]'
    rel 3.5.0.0 1 "$T/relkey"
    nctl fetch 3.5.0.0 >"$T/out" 2>&1
    check "fetch stages a signed version without switching" '[ $? = 0 ] && [ -x $N/downloads/3.5.0.0/d2rust-server-3.5.0.0/bin/d2rust-node ] && [ "$(basename $(readlink $N/current))" = d2rust-node-3.0.0.0 ]'
    echo 3.5.0.0 >"$N/run/swap"
    touch "$N/exit75"
    for i in $(seq 75); do grep -q '"3.5.0.0"' "$N/logs/status.json" 2>/dev/null && [ ! -f "$N/run/trial" ] && break; sleep 0.2; done
    check "exit 75 with run/swap swaps to the staged version and starts it" '[ "$(basename $(readlink $N/current))" = d2rust-node-3.5.0.0 ] && grep -q "\"3.5.0.0\"" $N/logs/status.json'
    check "and it passed its trial: server-ctl is the package's, the download is gone" '[ ! -f $N/run/trial ] && grep -q "^# packaged with 3.5.0.0" $N/server-ctl && [ ! -d $N/downloads/3.5.0.0 ]'
    rel 3.6.0.0 0 "$T/relkey"
    nctl fetch 3.6.0.0 >/dev/null 2>&1
    echo 3.6.0.0 >"$N/run/swap"
    touch "$N/exit75"
    for i in $(seq 100); do
        [ "$(basename "$(readlink "$N/current")")" = d2rust-node-3.5.0.0 ] && grep -q "3.6.0.0 started" "$N/logs/d2rust-node.log" && [ ! -f "$N/run/trial" ] && break
        sleep 0.2
    done
    check "a swapped-in version that never reports healthy is rolled back" '[ "$(basename $(readlink $N/current))" = d2rust-node-3.5.0.0 ] && grep -q "3.6.0.0 started" $N/logs/d2rust-node.log'
    check "and the version before runs again" 'sleep 2; [ "$(tail -n 1 $N/logs/d2rust-node.log)" = "3.5.0.0 started" ] && grep -q "\"3.5.0.0\"" $N/logs/status.json'
    echo 9.9.9.9 >"$N/run/swap"
    touch "$N/exit75"
    sleep 2
    check "a swap marker with nothing staged keeps the version" '[ "$(basename $(readlink $N/current))" = d2rust-node-3.5.0.0 ] && [ ! -f $N/run/swap ]'
    nctl units "$T/nunits" >/dev/null
    check "the game server unit swaps before a start and restarts on exit 75" 'grep -qx "ExecStartPre=$N/server-ctl pre-start" $T/nunits/d2rust-node.service && grep -qx "RestartForceExitStatus=75" $T/nunits/d2rust-node.service && grep -qx "ExecStartPost=$N/server-ctl trial-check" $T/nunits/d2rust-node.service'
    nctl stop
else
    bad "upgrade checks need ssh-keygen (openssh-client)"
fi

# `nodes`: the table of the main server's logs/nodes.json (a sample in the realm's format).
NJ=$D2RUST_SERVER_HOME/logs/nodes.json
cat >"$NJ" <<'EOF'
{"version": "1.0.0.0", "nodes": [
{"id": "eu-1", "name": "Europe \"1\"", "state": "downloading", "version": "1.0.0.0", "update": "1.1.0.0", "games": 2, "players": 5, "addr": "203.0.113.20:6113", "since_s": 3600},
{"id": "n2", "name": "HOMEPC 2", "state": "refused", "version": "0.9.0.0", "update": "", "games": 0, "players": 0, "addr": "198.51.100.7:6113", "since_s": 4},
{"id": "eu-3", "name": "Europe 3", "state": "offline", "version": "", "update": "", "games": 0, "players": 0, "addr": "", "since_s": 0}
]}
EOF
"$CTL" nodes >"$T/out" 2>&1
check "nodes prints a table of the game servers" '[ $? = 0 ] && head -n 1 $T/out | grep -q "^ID  *NAME  *VERSION  *STATE  *GAMES  *PLAYERS  *ADDRESS$" && [ "$(grep -c "^eu-\|^n2" $T/out)" = 3 ]'
check "with each one's name, version, state and load" 'grep -q "^eu-1  *Europe \"1\"  *1.0.0.0  *downloading 1.1.0.0  *2  *5  *203.0.113.20:6113$" $T/out && grep -q "^n2  *HOMEPC 2  *0.9.0.0  *refused  *0  *0  *198.51.100.7:6113$" $T/out && grep -q "^eu-3  *Europe 3  *offline  *0  *0 *$" $T/out'
check "and the main server's version and the file's age" 'grep -q "^main server 1.0.0.0; nodes.json written [0-9]* s ago$" $T/out && ! grep -q "is the main server running" $T/out'
touch -d '-60 seconds' "$NJ"
"$CTL" nodes >"$T/out" 2>&1
check "an old nodes.json: the main server may not be running, and the list instead" 'grep -q "not written for [0-9]* s: is the main server running" $T/out && grep -qx "eu-1 Finland" $T/out && ! grep -q "^ID " $T/out'
rm -f "$NJ"
"$CTL" nodes >"$T/out" 2>&1
check "no nodes.json: nodes says why and prints the list" '[ $? = 0 ] && grep -q "writes it while it runs" $T/out && grep -qx "eu-1 Finland" $T/out'

# A second realm on the same box (`--realm mymod`): its own folder, port, game, units and cron
# lines, and the first realm's untouched. A stateful crontab stub (written whole, as the real one) keeps both realms' lines.
RB=$T/realmbox
mkdir -p "$RB/home"
printf '#!/bin/sh\nif [ "$1" = -l ]; then cat "%s/crontab" 2>/dev/null; else cat >"%s/crontab.new" && mv "%s/crontab.new" "%s/crontab"; fi\n' "$RB" "$RB" "$RB" "$RB" >"$RB/crontab-stub"
mkdir -p "$RB/stub" && mv "$RB/crontab-stub" "$RB/stub/crontab" && chmod 755 "$RB/stub/crontab"
rctl() { env -u D2RUST_SERVER_HOME -u D2RUST_SERVER_REALM HOME="$RB/home" PATH="$RB/stub:$PATH" D2RUST_SERVER_MODE=cron "$HERE/server-ctl" "$@"; }
rctl setup >"$T/out" 2>&1
check "setup without --realm keeps the first realm's folder and config" '[ -f $RB/home/d2rust-server/d2rust-realm.cfg ] && [ ! -e $RB/home/d2rust-server/realm ] && ! grep -q "^game = " $RB/home/d2rust-server/d2rust-realm.cfg'
rctl --realm mymod setup >"$T/out" 2>&1
check "a second realm needs its own port" '[ $? != 0 ] || grep -q "needs --port" $T/out'
check "and made no folder without it" '[ ! -f $RB/home/d2rust-server-mymod/d2rust-realm.cfg ]'
rctl --realm mymod setup --port 6212 --label "My Mod" >"$T/out" 2>&1
check "setup --realm mymod makes a folder of its own with its realm file" '[ -f $RB/home/d2rust-server-mymod/d2rust-realm.cfg ] && [ "$(cat $RB/home/d2rust-server-mymod/realm)" = mymod ] && [ -d $RB/home/d2rust-server-mymod/data/realms ]'
check "its config names the game, the label and the port" 'grep -qx "game = mymod" $RB/home/d2rust-server-mymod/d2rust-realm.cfg && grep -qx "mod_label = My Mod" $RB/home/d2rust-server-mymod/d2rust-realm.cfg && grep -qx "listen = 0.0.0.0:6212" $RB/home/d2rust-server-mymod/d2rust-realm.cfg && [ "$(grep -c "^listen" $RB/home/d2rust-server-mymod/d2rust-realm.cfg)" = 1 ]'
check "the first realm's config is not touched" '! grep -q "mymod" $RB/home/d2rust-server/d2rust-realm.cfg && grep -qx "listen          = 0.0.0.0:6112" $RB/home/d2rust-server/d2rust-realm.cfg'
check "each realm has its own cron lines" '[ "$(grep -c "# d2rust-realm$" $RB/crontab)" = 2 ] && [ "$(grep -c "# d2rust-realm $RB/home/d2rust-server-mymod$" $RB/crontab)" = 2 ] && grep -q "$RB/home/d2rust-server-mymod/server-ctl ensure" $RB/crontab'
rctl setup >/dev/null 2>&1
check "setting up the first realm again keeps the second realm's cron lines" '[ "$(grep -c "# d2rust-realm $RB/home/d2rust-server-mymod$" $RB/crontab)" = 2 ] && [ "$(grep -c "# d2rust-realm$" $RB/crontab)" = 2 ]'
env -u D2RUST_SERVER_HOME HOME="$RB/home" "$RB/home/d2rust-server-mymod/server-ctl" units "$RB/units" >/dev/null 2>&1
check "the second realm's folder names its units after the realm, without a flag" '[ -f $RB/units/d2rust-realm-mymod.service ] && [ -f $RB/units/d2rust-realm-mymod-backup.timer ] && [ -f $RB/units/d2rust-realm-mymod-backup.service ] && [ ! -f $RB/units/d2rust-realm.service ]'
check "and runs its own folder, config and backup" 'grep -qx "WorkingDirectory=$RB/home/d2rust-server-mymod" $RB/units/d2rust-realm-mymod.service && grep -q "ExecStart=.* --config $RB/home/d2rust-server-mymod/d2rust-realm.cfg" $RB/units/d2rust-realm-mymod.service && grep -qx "ExecStart=$RB/home/d2rust-server-mymod/server-ctl backup" $RB/units/d2rust-realm-mymod-backup.service'
env -u D2RUST_SERVER_HOME HOME="$RB/home" "$RB/home/d2rust-server/server-ctl" units "$RB/units1" >/dev/null 2>&1
check "the first realm's units keep their names" '[ -f $RB/units1/d2rust-realm.service ] && [ -f $RB/units1/d2rust-realm-backup.timer ] && [ ! -f $RB/units1/d2rust-realm-mymod.service ]'
rctl --realm backup setup --port 6300 >"$T/out" 2>&1
check "a realm named backup is refused (it would clash with the backup unit)" '[ $? != 0 ] || grep -q "not \"backup\"" $T/out'
rctl --realm "Bad Name" status >"$T/out" 2>&1
check "a realm name of other characters is refused" 'grep -q "realm name" $T/out'
rctl --realm mymod setup --node --port 6213 >"$T/out" 2>&1
check "a realm's game server has a folder, unit and game of its own" '[ "$(cat $RB/home/d2rust-server-mymod-node/realm)" = mymod ] && [ "$(cat $RB/home/d2rust-server-mymod-node/program)" = d2rust-node ] && grep -qx "game = mymod" $RB/home/d2rust-server-mymod-node/d2rust-node.cfg && grep -qx "listen = 0.0.0.0:6213" $RB/home/d2rust-server-mymod-node/d2rust-node.cfg'
env -u D2RUST_SERVER_HOME HOME="$RB/home" "$RB/home/d2rust-server-mymod-node/server-ctl" units "$RB/nunits" >/dev/null 2>&1
check "named d2rust-node-mymod.service" '[ -f $RB/nunits/d2rust-node-mymod.service ]'

"$CTL" stop
rm -rf "$T"
echo "$fails failed"
[ $fails = 0 ]
