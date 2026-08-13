#!/usr/bin/env bash
# Merge the shared + board Buildroot config fragments into a defconfig.
#
# The fragments are designed to share zero symbols, so this is a plain
# concatenation (kconfig would let the later file win on overlap, but
# overlap here means a review problem, so scream instead).
#
# Usage:
#   scripts/gen-defconfig.sh > /tmp/boompi_defconfig
#   make -C ~/buildroot O=$OUT BR2_EXTERNAL=$REPO/buildroot \
#     BR2_DEFCONFIG=/tmp/boompi_defconfig defconfig
set -euo pipefail

CONFIGS="$(cd "$(dirname "$0")/../buildroot/configs" && pwd)"
COMMON="$CONFIGS/boompi-common.frag"
FRAG="$CONFIGS/boompi.frag"

# No symbol may appear in both fragments: overlap means the board file
# silently overrides the shared one and drift creeps back in.
dupes=$(cat "$COMMON" "$FRAG" | sed -n 's/^\(BR2_[A-Z0-9_]*\)=.*/\1/p' | sort | uniq -d)
if [ -n "$dupes" ]; then
    echo "symbol(s) defined in both fragments:" >&2
    echo "$dupes" >&2
    exit 1
fi

cat "$COMMON" "$FRAG"
