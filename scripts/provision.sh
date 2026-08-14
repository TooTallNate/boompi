#!/bin/sh
# Provision a box profile onto a running appliance over ssh.
#
# Copies boxes/<name>/ (config.txt, cmdline.txt, hardware.toml, env -
# whichever exist) to /data/box/ on the target and restarts boompid so
# the hardware profile takes effect. The firmware fragment reaches the
# boot partitions the next time one is written (any OS update), or
# immediately with --apply.
#
# --apply runs boompi-apply-box-config --all on the target. Only do
# this on boxes already running a board-generic image: on the old
# tailored images the box config is baked into the base config.txt and
# the fenced fragment would duplicate it (dtoverlay loaded twice).
#
# Usage: scripts/provision.sh <box-name> <ssh-host> [--apply]
#   e.g. scripts/provision.sh georges root@192.168.1.118
set -eu

BOX="${1:?usage: provision.sh <box-name> <ssh-host> [--apply]}"
HOST="${2:?usage: provision.sh <box-name> <ssh-host> [--apply]}"
APPLY="${3:-}"

DIR="$(dirname "$0")/../boxes/$BOX"
[ -d "$DIR" ] || { echo "no such profile: $DIR" >&2; exit 1; }

SSH_OPTS="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null"

# shellcheck disable=SC2086
ssh $SSH_OPTS "$HOST" "mkdir -p /data/box /data/ssh && chmod 700 /data/ssh"
for k in "$HOME"/.ssh/id_ed25519.pub "$HOME"/.ssh/id_rsa.pub; do
    if [ -f "$k" ]; then
        # shellcheck disable=SC2086
        scp -q -O $SSH_OPTS "$k" "$HOST:/data/ssh/authorized_keys"
        # shellcheck disable=SC2086
        ssh $SSH_OPTS "$HOST" "chmod 600 /data/ssh/authorized_keys"
        echo "  /data/ssh/authorized_keys ($k)"
        break
    fi
done
for f in config.txt cmdline.txt hardware.toml env; do
    if [ -f "$DIR/$f" ]; then
        # shellcheck disable=SC2086
        scp -q -O $SSH_OPTS "$DIR/$f" "$HOST:/data/box/$f"
        echo "  /data/box/$f"
    fi
done

if [ "$APPLY" = "--apply" ]; then
    # shellcheck disable=SC2086
    ssh $SSH_OPTS "$HOST" "boompi-apply-box-config --all"
fi

# shellcheck disable=SC2086
ssh $SSH_OPTS "$HOST" "systemctl restart boompid"
echo "provisioned $BOX -> $HOST (firmware fragment lands on the next boot-partition write${APPLY:+d now})"
