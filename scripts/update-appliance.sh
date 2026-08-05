#!/usr/bin/env bash
# Over-the-air OS update for the Pi 4 boombox (A/B tryboot slots).
#
# Fetches the newest green pi4 update bundle from CI (or takes a local
# bundle dir), pushes it to the box, and stages it into the inactive
# slot. The box reboots into the candidate exactly once; it commits
# only after boompid answers healthz, else the next reboot falls back.
#
# Usage:
#   scripts/update-appliance.sh                # latest CI bundle
#   scripts/update-appliance.sh <bundle-dir>   # local bundle
#
# Env: PI (default root@boompi.local), REPO (default TooTallNate/boompi)
set -euo pipefail

PI="${PI:-root@boompi.local}"
REPO="${REPO:-TooTallNate/boompi}"
BUNDLE="${1:-}"

if [ -z "$BUNDLE" ]; then
    BUNDLE="$(mktemp -d)/bundle"
    RUN=$(gh run list --repo "$REPO" --workflow image --status success --limit 1 --json databaseId --jq '.[0].databaseId')
    echo "downloading pi4 update bundle from run $RUN"
    gh run download "$RUN" --repo "$REPO" -n boompi-pi4-update -D "$BUNDLE"
    # Artifact files may arrive xz-compressed.
    for f in "$BUNDLE"/*.xz; do [ -e "$f" ] && xz -d "$f"; done
fi

for f in rootfs.ext4 boot-a.vfat boot-b.vfat; do
    [ -f "$BUNDLE/$f" ] || { echo "missing $BUNDLE/$f" >&2; exit 1; }
done

echo "pushing bundle to $PI"
ssh "$PI" 'rm -rf /tmp/boompi-update && mkdir -p /tmp/boompi-update'
scp -q "$BUNDLE"/rootfs.ext4 "$BUNDLE"/boot-a.vfat "$BUNDLE"/boot-b.vfat "$PI:/tmp/boompi-update/"

echo "staging into inactive slot + tryboot (box reboots now)"
ssh "$PI" 'boompi-update-slot /tmp/boompi-update' || true  # ssh drops at reboot

echo
echo "Box is rebooting into the candidate slot. Verify in ~60s:"
echo "  ssh $PI cat /proc/cmdline           # new slot's root device"
echo "  ssh $PI systemctl status boompi-boot-commit   # committed?"
