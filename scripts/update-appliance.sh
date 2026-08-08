#!/usr/bin/env bash
# Over-the-air OS update for the Pi 4 boombox (A/B slots, kexec trial).
#
# Fetches the newest green update bundle from CI (or takes a local
# bundle dir), pushes it to the box, and stages it into the inactive
# slot. The box kexecs into the candidate without touching autoboot.txt;
# it commits only after boompid answers healthz, else any reboot or
# power-cycle falls back to the old slot.
#
# Usage:
#   scripts/update-appliance.sh                # latest CI bundle
#   scripts/update-appliance.sh <bundle-dir>   # local bundle
#
# Env: PI (default root@boompi.local), REPO (default TooTallNate/boompi),
#      BOARD (pi4 default; pi3 for the Pi 3 box - same layout + scripts)
set -euo pipefail

PI="${PI:-root@boompi.local}"
REPO="${REPO:-TooTallNate/boompi}"
BOARD="${BOARD:-pi4}"
BUNDLE="${1:-}"

if [ -z "$BUNDLE" ]; then
    BUNDLE="$(mktemp -d)/bundle"
    RUN=$(gh run list --repo "$REPO" --workflow image --status success --limit 1 --json databaseId --jq '.[0].databaseId')
    echo "downloading $BOARD update bundle from run $RUN"
    gh run download "$RUN" --repo "$REPO" -n "boompi-$BOARD-update" -D "$BUNDLE"
    # Artifact files may arrive xz-compressed.
    for f in "$BUNDLE"/*.xz; do [ -e "$f" ] && xz -d "$f"; done
fi

for f in rootfs.ext4 boot-a.vfat boot-b.vfat; do
    [ -f "$BUNDLE/$f" ] || { echo "missing $BUNDLE/$f" >&2; exit 1; }
done

# Stream the images straight into the inactive slot's partitions - no
# staging copy anywhere on the box. /tmp is tmpfs sized at half of RAM,
# and on the Pi 3 (1GB) the 640MB bundle simply does not fit; streaming
# also halves update time on the Pi 4. The logic below mirrors the
# on-box boompi-update-slot (kept for local/manual use).
MARKER=/data/boompi-trial
if ssh "$PI" "[ -f $MARKER ]"; then
    echo "a trial is still pending on the box (commit or reboot first)" >&2
    exit 1
fi

case "$(ssh "$PI" cat /proc/cmdline)" in
    *root=/dev/mmcblk0p3*) ACTIVE=A ;;
    *root=/dev/mmcblk0p5*) ACTIVE=B ;;
    *) echo "cannot determine active slot" >&2; exit 1 ;;
esac
if [ "$ACTIVE" = A ]; then
    TARGET_BOOT=/dev/mmcblk0p2; TARGET_ROOT=/dev/mmcblk0p5
    BOOT_IMG="$BUNDLE/boot-b.vfat"; CURRENT_PART=1
else
    TARGET_BOOT=/dev/mmcblk0p1; TARGET_ROOT=/dev/mmcblk0p3
    BOOT_IMG="$BUNDLE/boot-a.vfat"; CURRENT_PART=2
fi
echo "active slot: $ACTIVE - streaming update to $TARGET_ROOT / $TARGET_BOOT"

stream() { # <local-file> <remote-dev>
    ssh "$PI" "dd of=$2 bs=4M conv=fsync status=none" < "$1"
    want=$(shasum -a 256 "$1" | cut -d' ' -f1)
    size=$(wc -c < "$1" | tr -d ' ')
    got=$(ssh "$PI" "head -c $size $2 | sha256sum | cut -d' ' -f1")
    [ "$want" = "$got" ] || { echo "verify FAILED: $2 != $1" >&2; exit 1; }
    echo "  $2 written + verified"
}
stream "$BUNDLE/rootfs.ext4" "$TARGET_ROOT"
stream "$BOOT_IMG" "$TARGET_BOOT"

echo "arming trial + kexec'ing into candidate (box reboots now)"
ssh "$PI" "set -eu
rm -rf /tmp/boompi-update /tmp/boompi-trial-Image # stale staging leftovers
# boot-a.vfat carries the image-default autoboot.txt (boot_partition=1,
# the candidate); the current slot must stay the fallback until commit.
if [ $TARGET_BOOT = /dev/mmcblk0p1 ]; then
    MNT=\$(mktemp -d)
    mount /dev/mmcblk0p1 "\$MNT"
    printf '[all]\nboot_partition=%s\n' $CURRENT_PART > "\$MNT/autoboot.txt"
    umount "\$MNT"; rmdir "\$MNT"
fi
MNT=\$(mktemp -d)
mount -o ro $TARGET_BOOT "\$MNT"
cp "\$MNT/Image" /tmp/boompi-trial-Image
umount "\$MNT"; rmdir "\$MNT"
CMDLINE=\$(sed 's|root=/dev/mmcblk0p[0-9]*|root=$TARGET_ROOT|' /proc/cmdline)
kexec -l /tmp/boompi-trial-Image --dtb=/sys/firmware/fdt --command-line="\$CMDLINE"
echo $TARGET_ROOT > $MARKER
sync
systemctl kexec" || true # ssh drops at kexec

echo
echo "Box is kexec'ing into the candidate slot. Verify in ~60s:"
echo "  ssh $PI cat /proc/cmdline           # new slot's root device"
echo "  ssh $PI systemctl status boompi-boot-commit   # committed?"
