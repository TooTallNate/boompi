#!/usr/bin/env bash
# Over-the-air OS update for the boomboxes (A/B slots with trial boot).
#
# Fetches the newest green update bundle from CI (or takes a local
# bundle dir), pushes it to the box, and stages it into the inactive
# slot. The box trial-boots the candidate without touching autoboot.txt;
# it commits only after boompid answers healthz, else any reboot or
# power-cycle falls back to the old slot.
#
# Trial mechanism per board (mirrors on-box boompi-trial-boot):
#   pi3: one-shot PM_RSTS partition request (the pre-tryboot `reboot N`
#        protocol) - bootcode.bin boots the requested partition once
#        and clears the request; the candidate gets a full firmware
#        boot and any crash/power-cycle falls back to the old slot.
#        Firmware tryboot proper does not work here: the mailbox
#        tryboot flag lives in PM_RSTS bit 1, which does not survive
#        the watchdog reset on BCM2837.
#   pi4: commit-with-rollback - autoboot.txt is flipped to the
#        candidate before the reboot and boompi-boot-commit flips it
#        back if the candidate boots sick. No one-shot mechanism
#        exists on rev <= 1.3 (the PMIC power-cycles on every reboot,
#        wiping PM_RSTS and the tryboot flag - both bench-falsified on
#        rev 1.2), and kexec into a different kernel build hangs after
#        "Bye!" on both boards (kexec is retired).
#
# Usage:
#   scripts/update-appliance.sh                # latest CI bundle
#   scripts/update-appliance.sh <bundle-dir>   # local bundle
#
# Env: PI (default root@boompi.local), REPO (default TooTallNate/boompi),
#      BOARD (pi4 default; pi3 for the Pi 3 box - same layout + scripts)
#      TRIAL=0 to skip the trial boot: flip autoboot + firmware reboot
#      (commit-without-trial). Recovery if the new slot were unbootable:
#      edit autoboot.txt on the card.
set -euo pipefail

PI="${PI:-root@boompi.local}"
REPO="${REPO:-TooTallNate/boompi}"
BOARD="${BOARD:-pi4}"
BUNDLE="${1:-}"

if [ -z "$BUNDLE" ]; then
    BUNDLE="$(mktemp -d)/bundle"
    RUN=$(gh run list --repo "$REPO" --workflow image --status success --limit 1 --json databaseId --jq '.[0].databaseId')
    echo "downloading $BOARD update bundle from run $RUN"
    gh run download "$RUN" --repo "$REPO" -n "boompi-update" -D "$BUNDLE"
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

if [ "${TRIAL:-1}" = 0 ]; then
    echo "committing without trial: flipping autoboot + firmware reboot"
    if [ "$TARGET_BOOT" = /dev/mmcblk0p1 ]; then TARGET_PART=1; else TARGET_PART=2; fi
    ssh "$PI" "set -eu
MNT=\$(mktemp -d)
mount /dev/mmcblk0p1 "\$MNT"
printf '[all]\nboot_partition=%s\n' $TARGET_PART > "\$MNT/autoboot.txt"
umount "\$MNT"; rmdir "\$MNT"
sync
reboot" || true
    echo "box is firmware-booting the new slot; verify in ~90s:"
    echo "  ssh $PI cat /proc/cmdline"
    exit 0
fi

if [ "$BOARD" = pi3 ]; then
    # Spread the target boot partition number into PM_RSTS bits
    # 0,2,4,6,8,10 with the 0x5a password byte: p1 -> 0x1, p2 -> 0x4.
    # Arm BOTH mechanisms, mirroring on-box boompi-trial-boot: PSCI
    # kernels preserve the devmem write; spin-table kernels rewrite
    # PM_RSTS on reboot but parse the reboot argument into the
    # partition bits. (A plain `reboot` on a spin-table kernel
    # clobbers the devmem request with partition 0 - bench-bitten:
    # the box boots straight back into the old slot.)
    if [ "$TARGET_BOOT" = /dev/mmcblk0p1 ]; then RSTS=0x5a000001; PART=1; else RSTS=0x5a000004; PART=2; fi
    echo "arming one-shot PM_RSTS partition request (box reboots now)"
    ssh "$PI" "set -eu
# boot-a.vfat carries the image-default autoboot.txt (boot_partition=1,
# the candidate); the current slot must stay the fallback until commit.
if [ $TARGET_BOOT = /dev/mmcblk0p1 ]; then
    MNT=\$(mktemp -d)
    mount /dev/mmcblk0p1 \"\$MNT\"
    printf '[all]\nboot_partition=%s\n' $CURRENT_PART > \"\$MNT/autoboot.txt\"
    umount \"\$MNT\"; rmdir \"\$MNT\"
fi
echo $TARGET_ROOT > $MARKER
sync
devmem 0x3f100020 32 $RSTS
systemctl reboot --reboot-argument=$PART || reboot" || true # ssh drops at reboot
    echo
    echo "Box is firmware-booting the candidate slot once. Verify in ~90s:"
    echo "  ssh $PI cat /proc/cmdline           # new slot's root device"
    echo "  ssh $PI systemctl status boompi-boot-commit   # committed?"
    exit 0
fi

# pi4 commit-with-rollback: flip autoboot to the candidate, mark the
# trial, firmware reboot. boompi-boot-commit rolls autoboot back if
# the candidate boots sick.
if [ "$TARGET_BOOT" = /dev/mmcblk0p1 ]; then TARGET_PART=1; else TARGET_PART=2; fi
echo "flipping autoboot to the candidate + rebooting (rollback if sick)"
ssh "$PI" "set -eu
MNT=\$(mktemp -d)
mount /dev/mmcblk0p1 \"\$MNT\"
printf '[all]\nboot_partition=%s\n' $TARGET_PART > \"\$MNT/autoboot.txt\"
umount \"\$MNT\"; rmdir \"\$MNT\"
echo $TARGET_ROOT > $MARKER
sync
reboot" || true # ssh drops at reboot

echo
echo "Box is firmware-booting the candidate slot. Verify in ~90s:"
echo "  ssh $PI cat /proc/cmdline           # new slot's root device"
echo "  ssh $PI systemctl status boompi-boot-commit   # committed?"
