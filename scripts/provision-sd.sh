#!/bin/sh
# Provision a freshly flashed SD card with a box profile.
#
# Flash the generic image (boompi-sdcard-*.img.xz) with any tool, let
# the OS mount the boot partition (the first FAT - macOS/Windows show
# it automatically), then:
#
#   scripts/provision-sd.sh georges /Volumes/bootfs
#
# This drops a `boompi-box/` bundle onto the FAT; the appliance
# ingests it on first boot (boompi-ingest-provision): profile copied
# to /data/box/, firmware config merged into both boot slots, one
# automatic reboot if needed. The bundle is renamed *.applied after
# ingest - drop a fresh one any time to re-provision.
#
# Works from any OS with a shell; no root, no loop mounts, no ext4.
set -eu

BOX="${1:?usage: provision-sd.sh <box-name> <mounted-boot-path> [--ssh-key file]}"
BOOT="${2:?usage: provision-sd.sh <box-name> <mounted-boot-path> [--ssh-key file]}"
SSH_KEY=""
if [ "${3:-}" = "--ssh-key" ]; then
    SSH_KEY="${4:?--ssh-key needs a file}"
else
    # Default: the user's own public key, so a freshly provisioned box
    # is reachable over (key-only) ssh. Keys are per-box state - never
    # committed in boxes/.
    for k in "$HOME"/.ssh/id_ed25519.pub "$HOME"/.ssh/id_rsa.pub; do
        [ -f "$k" ] && { SSH_KEY="$k"; break; }
    done
fi

DIR="$(dirname "$0")/../boxes/$BOX"
[ -d "$DIR" ] || { echo "no such profile: $DIR" >&2; exit 1; }
[ -f "$BOOT/config.txt" ] || {
    echo "$BOOT does not look like a boompi boot partition (no config.txt)" >&2
    exit 1
}

rm -rf "$BOOT/boompi-box"
mkdir -p "$BOOT/boompi-box"
for f in config.txt cmdline.txt hardware.toml env; do
    if [ -f "$DIR/$f" ]; then
        cp "$DIR/$f" "$BOOT/boompi-box/$f"
        echo "  boompi-box/$f"
    fi
done
if [ -n "$SSH_KEY" ]; then
    cp "$SSH_KEY" "$BOOT/boompi-box/authorized_keys"
    echo "  boompi-box/authorized_keys ($SSH_KEY)"
else
    echo "  (no ssh key found - the box will be web/console only; see --ssh-key)"
fi
sync
echo "provisioned $BOX -> $BOOT (ingested on first boot)"
