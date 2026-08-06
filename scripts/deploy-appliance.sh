#!/usr/bin/env bash
# Fast dev loop for the *appliance* (Buildroot image): cross-compile
# boompid + boompi-ui from macOS and swap the binaries in place over SSH.
# The OS layer stays whatever image the box is running; only the app
# binaries change. Full-image changes still go through CI + OTA
# (scripts/update-appliance.sh).
#
# One-time setup:
#   brew install zig cargo-zigbuild
#   rustup target add aarch64-unknown-linux-gnu
#   scripts/deploy-appliance.sh --sysroot   # pull /usr/lib from the box
#
# Usage:
#   scripts/deploy-appliance.sh             # build both + deploy + restart
#   scripts/deploy-appliance.sh --sysroot   # (re)fetch the sysroot only
#
# Env: PI (default root@boompi.local), BOOMPI_APPLIANCE_SYSROOT,
#      BOOMPI_GLIBC (default 2.41 - Buildroot 2026.02 bootlin toolchain)
set -euo pipefail

PI="${PI:-root@boompi.local}"
SYS="${BOOMPI_APPLIANCE_SYSROOT:-$HOME/boompi-appliance-sysroot}"
GLIBC="${BOOMPI_GLIBC:-2.41}"
TARGET="aarch64-unknown-linux-gnu"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

fetch_sysroot() {
    echo "fetching sysroot from $PI (tar over ssh; the image has no rsync)"
    mkdir -p "$SYS"
    ssh "$PI" 'tar -C / -cf - usr/lib 2>/dev/null' | tar -C "$SYS" -xf -
    # Minimal .pc stubs: the -sys crates only need the link line; the
    # target image ships .so files but no pkg-config metadata or headers.
    mkdir -p "$SYS/usr/lib/pkgconfig"
    local name ver lib
    for spec in fontconfig:2.15.0:fontconfig libinput:1.28.0:input \
                xkbcommon:1.7.0:xkbcommon libudev:257:udev; do
        IFS=: read -r name ver lib <<< "$spec"
        cat > "$SYS/usr/lib/pkgconfig/$name.pc" <<EOF
prefix=/usr
libdir=\${prefix}/lib
includedir=\${prefix}/include

Name: $name
Description: $name (appliance sysroot stub)
Version: $ver
Libs: -L\${libdir} -l$lib
Cflags: -I\${includedir}
EOF
    done
    echo "sysroot ready: $SYS"
}

if [ "${1:-}" = "--sysroot" ]; then
    fetch_sysroot
    exit 0
fi

[ -d "$SYS/usr/lib" ] || { echo "no sysroot at $SYS - run: $0 --sysroot" >&2; exit 1; }

export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_SYSROOT_DIR="$SYS"
export PKG_CONFIG_LIBDIR="$SYS/usr/lib/pkgconfig"
export RUSTFLAGS="-L $SYS/usr/lib ${RUSTFLAGS:-}"

echo "== cross-building boompid (embeds web/dist) + boompi-ui =="
cargo zigbuild --manifest-path "$REPO_ROOT/rust/Cargo.toml" \
    --target "$TARGET.$GLIBC" --release -p boompid
cargo zigbuild --manifest-path "$REPO_ROOT/rust/Cargo.toml" \
    --target "$TARGET.$GLIBC" --release \
    -p boompi-ui --no-default-features --features kms

BIN="$REPO_ROOT/rust/target/$TARGET/release"

echo "== deploying to $PI =="
scp -q "$BIN/boompid" "$BIN/boompi-ui" "$PI:/tmp/"
# Move into place + restart under one ssh so a dropped connection can't
# leave a half-copied binary running.
ssh "$PI" '
    set -e
    mv /tmp/boompid /usr/bin/boompid
    mv /tmp/boompi-ui /usr/bin/boompi-ui
    chmod 755 /usr/bin/boompid /usr/bin/boompi-ui
    systemctl restart boompid boompi-ui
    sleep 1
    systemctl is-active boompid boompi-ui
'
echo "deployed + restarted"
