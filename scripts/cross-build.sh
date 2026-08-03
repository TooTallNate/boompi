#!/usr/bin/env bash
# Cross-compile Boompi Rust binaries for the Pi (aarch64-linux-gnu) from
# macOS, using zig as the cross linker (cargo-zigbuild) and a sysroot
# rsync'd from the running Pi for the C libraries Slint's linuxkms backend
# links against (libinput, libudev, libxkbcommon, fontconfig, ...).
#
# One-time setup:
#   brew install zig cargo-zigbuild
#   rustup target add aarch64-unknown-linux-gnu
#   make sysroot PI=pi@boompi-dev.local     # pulls headers/libs from the Pi
#
# Usage:
#   scripts/cross-build.sh <package> [extra cargo args...]
# Examples:
#   scripts/cross-build.sh kms-test --no-default-features --features kms
#   scripts/cross-build.sh boompid
#
# Why not build on the Pi 3? 1 GB RAM, slow, and rustup's rustc currently
# segfaults there (deterministic SIGSEGV in Symbol::intern on Trixie).

set -euo pipefail

SYSROOT="${BOOMPI_SYSROOT:-$HOME/boompi-sysroot}"
TARGET="aarch64-unknown-linux-gnu"
# Target a glibc no newer than the Pi's (Trixie ships 2.41; 2.36 is safe).
GLIBC="${BOOMPI_GLIBC:-2.36}"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PKG="${1:?usage: cross-build.sh <package> [cargo args...]}"
shift || true

LIBDIR="$SYSROOT/usr/lib/aarch64-linux-gnu"
if [ ! -d "$LIBDIR" ]; then
    echo "error: sysroot not found at $SYSROOT" >&2
    echo "run: make sysroot PI=pi@boompi-dev.local" >&2
    exit 1
fi

# Cross pkg-config: resolve .pc files from the sysroot, prefixing paths.
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_SYSROOT_DIR="$SYSROOT"
export PKG_CONFIG_LIBDIR="$LIBDIR/pkgconfig:$SYSROOT/usr/share/pkgconfig"

# Let the linker find the Pi's shared libraries.
export RUSTFLAGS="-L $LIBDIR ${RUSTFLAGS:-}"

exec cargo zigbuild \
    --manifest-path "$REPO_ROOT/rust/Cargo.toml" \
    --target "$TARGET.$GLIBC" \
    --release \
    -p "$PKG" \
    "$@"
