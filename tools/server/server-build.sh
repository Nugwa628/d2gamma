#!/bin/sh
# server-build.sh: builds and packages the server programs of tools/server/programs.txt.
#
#   sh tools/server/server-build.sh linux     the Debian 13 package: tarball, .deb, SHA256SUMS
#   sh tools/server/server-build.sh windows   the game server zip for Windows (cargo-xwin)
#
# Runs on Linux: Debian 13 for the Linux package, any Linux with cargo-xwin for the Windows zip.
# The version is the game's (the d2-client crate's) plus the build
# number $D2RUST_BUILD as a 4th part, as the programs report it. Output goes to dist/server/:
#
#   d2rust-server-<version>-linux-x86_64.tar.gz    d2rust-server-<version>/ with bin/, server-ctl,
#   d2rust-server_<version>_amd64.deb               the config examples, systemd/, data/realms/<mod>/,
#                                                   README.txt, VERSION and GLIBC (the newest glibc
#                                                   the programs need); the .deb holds the same
#                                                   folder as /opt/d2rust-server
#   d2rust-node-<version>-windows-x86_64.zip       d2rust-node-<version>/ with the .exe files,
#                                                   node-ctl.ps1, the config examples, README.txt
#                                                   and version.json
#   SHA256SUMS.linux / SHA256SUMS.windows          sha256sum lines of the packages above
#
# Linux builds must run on Debian 13 (glibc 2.41), so the programs never need a newer glibc than
# Debian 13 has; SERVER_BUILD_ANY_OS=1 skips that check for a local try. Low CPU priority.
set -eu
[ $# -eq 1 ] || { sed -n 2,5p "$0" | sed 's/^# \{0,1\}//'; exit 2; }
PLATFORM=$1
HERE=$(cd "$(dirname "$0")" && pwd)
LIST=$HERE/programs.txt
OUT=dist/server
# The Linux package's name is the server folder's on the box (~/d2rust-server).
LINUX_NAME=d2rust-server
WINDOWS_NAME=d2rust-node
# Debian 13's glibc: no program may need a newer one.
DEBIAN_GLIBC=2.41
# The mod realms whose data folders the package lays out (data/realms/<mod>/realm.sqlite).
MODS=d2rust
TARGET_DIR=${CARGO_TARGET_DIR:-target}
case $PLATFORM in linux | windows) ;; *) echo "server-build.sh: linux or windows, not '$PLATFORM'" >&2; exit 2 ;; esac

die() { echo "server-build.sh: $*" >&2; exit 1; }

# "<program> <package>" for every program of this platform.
programs() {
    awk -v p="$PLATFORM" '!/^#/ && NF == 3 { n = split($3, a, ","); for (i = 1; i <= n; i++) if (a[i] == p) print $1, $2 }' "$LIST"
}
[ -n "$(programs)" ] || die "programs.txt lists no program for $PLATFORM"

# The game's version (the d2-client crate's, which the programs' build.rs read too: client and
# servers match exactly) plus the build number.
crate=$(cargo pkgid -p d2-client | sed 's/.*[#@]//')
expr "$crate" : '[0-9]*\.[0-9]*\.[0-9]*$' >/dev/null || die "unexpected d2-client version '$crate'"
VERSION=$crate.${D2RUST_BUILD:-0}
pkgs=$(programs | awk '{ printf " -p %s", $2 }')
rm -rf "$OUT/stage-$PLATFORM"
mkdir -p "$OUT"

# Every regular file of a package folder, for the content check below.
check_contents() {
    (cd "$1" && find . -type f | sed 's#^\./##' | sort) | while read -r f; do
        case $f in
            bin/* | server-ctl | node-ctl.ps1 | *.cfg.example | systemd/*.service | systemd/*.timer | README.txt | VERSION | GLIBC | *.exe | version.json) ;;
            *) die "unexpected file in the package: $f" ;;
        esac
    done
}

if [ "$PLATFORM" = linux ]; then
    if [ "${SERVER_BUILD_ANY_OS:-0}" != 1 ]; then
        # shellcheck disable=SC1091
        os=$(. /etc/os-release && echo "$ID $VERSION_ID")
        [ "$os" = "debian 13" ] || die "the Linux server build runs on Debian 13 (here: $os); see the header"
    fi
    # shellcheck disable=SC2086
    nice -n 10 cargo build --release --locked $pkgs
    name=$LINUX_NAME-$VERSION
    stage=$OUT/stage-linux/$name
    mkdir -p "$stage/bin" "$stage/systemd"
    glibc=0
    for prog in $(programs | awk '{ print $1 }'); do
        cp "$TARGET_DIR/release/$prog" "$stage/bin/$prog"
        strip "$stage/bin/$prog"
        got=$("$stage/bin/$prog" --version)
        [ "$got" = "$prog $VERSION" ] || die "$prog --version printed '$got', not '$prog $VERSION'"
        need=$(objdump -T "$stage/bin/$prog" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -n 1)
        [ "$(printf '%s\n%s\n' "$need" "$DEBIAN_GLIBC" | sort -V | tail -n 1)" = "$DEBIAN_GLIBC" ] ||
            die "$prog needs glibc $need, newer than Debian 13's $DEBIAN_GLIBC"
        glibc=$(printf '%s\n%s\n' "$need" "$glibc" | sort -V | tail -n 1)
        [ -f "$HERE/$prog.cfg.example" ] && cp "$HERE/$prog.cfg.example" "$stage/"
        echo "$prog $VERSION: needs glibc $need"
    done
    cp "$HERE/server-ctl" "$stage/server-ctl"
    chmod -R u=rwX,go=rX "$stage"
    chmod 755 "$stage/server-ctl" "$stage"/bin/*
    # The user units, as server-ctl setup writes them for ~/d2rust-server.
    D2RUST_SERVER_HOME='%h/d2rust-server' bash "$HERE/server-ctl" units "$stage/systemd"
    for mod in $MODS; do mkdir -p "$stage/data/realms/$mod"; done
    echo "$VERSION" >"$stage/VERSION"
    echo "$glibc" >"$stage/GLIBC"
    cat >"$stage/README.txt" <<EOF
D2RUST server $VERSION for Linux (Debian 13, x86-64; needs glibc $glibc or newer)

bin/                 the server programs: $(programs | awk '{ printf "%s ", $1 }')
server-ctl           runs them without root: setup, install, upgrade, start, stop, status, ...
*.cfg.example        the config server-ctl setup starts from
systemd/             the user units server-ctl setup installs (%h is the home folder)
data/realms/<mod>/   one folder per mod realm; its realm.sqlite holds that mod's characters
                     and economy (accounts stay in data/accounts.sqlite)

First install, as the service user: unpack this folder, then
  ./server-ctl setup && ./server-ctl install bin/$LINUX_NAME
Later versions, on the box: ~/d2rust-server/server-ctl upgrade <version>
The whole guide is tools/server/README.md in the D2RUST repository.
EOF
    check_contents "$stage"
    tarball=$name-linux-x86_64.tar.gz
    tar -C "$OUT/stage-linux" --sort=name --owner=0 --group=0 --numeric-owner -czf "$OUT/$tarball" "$name"
    # The .deb: the same folder under /opt/d2rust-server (dpkg-deb, in the image and on Debian).
    deb=${LINUX_NAME}_${VERSION}_amd64.deb
    droot=$OUT/stage-linux/deb
    mkdir -p "$droot/DEBIAN" "$droot/opt"
    cp -R "$stage" "$droot/opt/$LINUX_NAME"
    cat >"$droot/DEBIAN/control" <<EOF
Package: $LINUX_NAME
Version: $VERSION
Architecture: amd64
Maintainer: Nugwa <nugwa@users.noreply.invalid>
Depends: libc6 (>= $glibc)
Section: games
Priority: optional
Description: D2RUST multiplayer server programs
 The server programs, server-ctl, the config examples and the systemd user units, in
 /opt/$LINUX_NAME. Run /opt/$LINUX_NAME/server-ctl setup as the service user.
EOF
    dpkg-deb --root-owner-group -Zxz --build "$droot" "$OUT/$deb" >/dev/null
    (cd "$OUT" && sha256sum "$tarball" "$deb" >SHA256SUMS.linux)
    ls -l "$OUT/$tarball" "$OUT/$deb"
else
    # shellcheck disable=SC2086
    nice -n 10 cargo xwin build --target x86_64-pc-windows-msvc --release --locked $pkgs
    name=$WINDOWS_NAME-$VERSION
    stage=$OUT/stage-windows/$name
    mkdir -p "$stage"
    progs=
    for prog in $(programs | awk '{ print $1 }'); do
        cp "$TARGET_DIR/x86_64-pc-windows-msvc/release/$prog.exe" "$stage/$prog.exe"
        # It can't run here: the version check reads the exe (PE header and the version it carries).
        python3 "$HERE/check-exe.py" "$stage/$prog.exe" "$VERSION"
        [ -f "$HERE/$prog.cfg.example" ] && cp "$HERE/$prog.cfg.example" "$stage/"
        progs="$progs${progs:+,}\"$prog.exe\""
    done
    cp "$HERE/node-ctl.ps1" "$stage/"
    printf '{"version":"%s","programs":[%s]}\n' "$VERSION" "$progs" >"$stage/version.json"
    cat >"$stage/README.txt" <<EOF
D2RUST game server $VERSION for Windows (x86-64)

The game client starts it for "Host on my PC"; it can also run on its own as an official game
server. Run a program with --version to see its version.

node-ctl.ps1 runs it as a game server, hidden, and upgrades it: from an elevated PowerShell in
this folder, once: powershell -ExecutionPolicy Bypass -File .\node-ctl.ps1 setup <id>
(tools/server/README.md, "A Windows game server").
EOF
    check_contents "$stage"
    zipfile=$name-windows-x86_64.zip
    rm -f "$OUT/$zipfile"
    (cd "$OUT/stage-windows" && python3 -m zipfile -c "../$zipfile" "$name")
    (cd "$OUT" && sha256sum "$zipfile" >SHA256SUMS.windows)
    ls -l "$OUT/$zipfile"
fi
rm -rf "$OUT/stage-$PLATFORM"
echo "VERSION=$VERSION"
