#!/usr/bin/env bash
# Deploy cross-built binaries, race-free.
#
# Two targets:
#   default      — the RPi OS dev box: binaries in ~pi, started via nohup.
#   --appliance  — a flashed Buildroot image: binaries in /usr/bin under
#                  systemd (ssh as root; hostname 'boompi' via avahi).
#
# The naive `pkill && mv && nohup` pattern raced the old instance's port
# release, leaving zombies serving stale binaries (with `/proc/PID/exe ->
# ... (deleted)`). This kills hard, waits for the ports to actually free,
# and verifies the running process executes the new inode.
#
# Usage: scripts/deploy-dev.sh [--appliance] [boompid] [boompi-ui]
set -euo pipefail

TARGET_DIR="rust/target/aarch64-unknown-linux-gnu/release"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

MODE=dev
if [ "${1:-}" = "--appliance" ]; then
    MODE=appliance
    shift
fi

WANT=("$@")
[ ${#WANT[@]} -eq 0 ] && WANT=(boompid boompi-ui)

if [ "$MODE" = "appliance" ]; then
    PI="${PI:-root@boompi.local}"
    ssh "$PI" 'mkdir -p /tmp/staging'
    for bin in "${WANT[@]}"; do
        scp -q "$TARGET_DIR/$bin" "$PI:/tmp/staging/$bin"
    done
    ssh "$PI" "
set -e
for bin in ${WANT[*]}; do
    systemctl stop \"\$bin\" 2>/dev/null || true
    install -m 0755 /tmp/staging/\$bin /usr/bin/\$bin
    rm /tmp/staging/\$bin
    systemctl start \"\$bin\"
done
sleep 3
if printf '%s\n' ${WANT[*]} | grep -qx boompid; then
    systemctl is-active --quiet boompid || { echo 'ERROR: boompid unit not active' >&2; exit 1; }
    curl -sf http://127.0.0.1:3001/healthz > /dev/null || { echo 'ERROR: healthz' >&2; exit 1; }
fi
if printf '%s\n' ${WANT[*]} | grep -qx boompi-ui; then
    systemctl is-active --quiet boompi-ui || { echo 'ERROR: boompi-ui unit not active' >&2; exit 1; }
fi
echo 'deploy OK (appliance)'
"
    exit 0
fi

# Dev box: boompid runs under a systemd *user* unit (~/.config/systemd/
# user/boompid.service, Restart=always) — deploys must go through
# systemctl or the unit's auto-restart races the new instance for the
# ports (learned the hard way: zombie instances serving deleted inodes).
PI="${PI:-pi@boompi-dev-2.local}"
ssh "$PI" 'mkdir -p ~/staging'
for bin in "${WANT[@]}"; do
    scp -q "$TARGET_DIR/$bin" "$PI:~/staging/$bin"
done

ssh "$PI" "
set -e
if printf '%s\n' ${WANT[*]} | grep -qx boompid; then
    systemctl --user stop boompid 2>/dev/null || true
    pkill -9 -x boompid 2>/dev/null || true   # stray non-unit instances
    pkill -9 -x shairport-sync 2>/dev/null || true
    for i in \$(seq 1 20); do ss -ltn | grep -qE ':3001|:8080' || break; sleep 0.5; done
    if ss -ltn | grep -qE ':3001|:8080'; then echo 'ERROR: ports still held' >&2; exit 1; fi
    mv ~/staging/boompid ~/boompid && chmod +x ~/boompid
    systemctl --user start boompid
    sleep 3
    P=\$(pgrep -x boompid) || { echo 'ERROR: boompid did not start' >&2; exit 1; }
    readlink /proc/\$P/exe | grep -q deleted && { echo 'ERROR: stale inode' >&2; exit 1; }
    curl -sf http://127.0.0.1:3001/healthz > /dev/null || { echo 'ERROR: healthz' >&2; exit 1; }
fi
if printf '%s\n' ${WANT[*]} | grep -qx boompi-ui; then
    pkill -9 -x boompi-ui 2>/dev/null || true
    mv ~/staging/boompi-ui ~/boompi-ui && chmod +x ~/boompi-ui
    nohup env SLINT_KMS_ROTATION=270 ./boompi-ui >/tmp/boompi-ui.log 2>&1 < /dev/null &
    disown
    sleep 2
    pgrep -x boompi-ui > /dev/null || { echo 'ERROR: boompi-ui did not start' >&2; exit 1; }
fi
echo 'deploy OK'
"
