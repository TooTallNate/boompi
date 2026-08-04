#!/usr/bin/env bash
# Cross-compile librespot (Spotify Connect) for the Pi.
#
# Feature selection keeps the build 100% pure Rust (no OpenSSL, no ALSA,
# no avahi), so it cross-compiles with zig exactly like our own crates:
#   - rustls-tls-webpki-roots: TLS without system libraries or CA bundles
#   - with-libmdns: built-in zeroconf/mDNS discovery
#   - audio output uses the always-available `pipe` backend; boompid pipes
#     the raw PCM into pw-play (see boompid/src/spotify.rs)
#
# Output: build/librespot/aarch64-unknown-linux-gnu/release/librespot

set -euo pipefail

VERSION="${LIBRESPOT_VERSION:-0.8.0}"
TARGET="aarch64-unknown-linux-gnu"
GLIBC="${BOOMPI_GLIBC:-2.41}"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_DIR="$REPO_ROOT/build/librespot"
SRC_DIR="$BUILD_DIR/librespot-$VERSION"

mkdir -p "$BUILD_DIR"
if [ ! -d "$SRC_DIR" ]; then
    echo "downloading librespot $VERSION..."
    curl -fsSL -H "User-Agent: boompi-build" \
        -o "$BUILD_DIR/librespot.crate" \
        "https://crates.io/api/v1/crates/librespot/$VERSION/download"
    tar -xzf "$BUILD_DIR/librespot.crate" -C "$BUILD_DIR"
fi

exec cargo zigbuild \
    --manifest-path "$SRC_DIR/Cargo.toml" \
    --target-dir "$BUILD_DIR" \
    --target "$TARGET.$GLIBC" \
    --release \
    --no-default-features \
    --features rustls-tls-webpki-roots,with-libmdns
