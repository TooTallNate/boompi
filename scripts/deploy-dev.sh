#!/usr/bin/env bash
# Deploy cross-built binaries to the dev Pi, race-free.
#
# The naive `pkill && mv && nohup` pattern raced the old instance's port
# release, leaving zombies serving stale binaries (with `/proc/PID/exe ->
# ... (deleted)`). This kills hard, waits for the ports to actually free,
# and verifies the running process executes the new inode.
#
# Usage: scripts/deploy-dev.sh [boompid] [boompi-ui]
set -euo pipefail

PI="${PI:-pi@boompi-dev-2.local}"
TARGET_DIR="rust/target/aarch64-unknown-linux-gnu/release"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

WANT=("$@")
[ ${#WANT[@]} -eq 0 ] && WANT=(boompid boompi-ui)

ssh "$PI" 'mkdir -p ~/staging'
for bin in "${WANT[@]}"; do
    scp -q "$TARGET_DIR/$bin" "$PI:~/staging/$bin"
done

ssh "$PI" "
set -e
for bin in ${WANT[*]}; do pkill -9 -x \"\$bin\" 2>/dev/null || true; done
pkill -9 -x shairport-sync 2>/dev/null || true
for i in \$(seq 1 20); do ss -ltn | grep -qE ':3001|:8080' || break; sleep 0.5; done
if ss -ltn | grep -qE ':3001|:8080'; then echo 'ERROR: ports still held' >&2; exit 1; fi
for bin in ${WANT[*]}; do mv ~/staging/\$bin ~/\$bin; chmod +x ~/\$bin; done
if printf '%s\n' ${WANT[*]} | grep -qx boompid; then
    nohup ./boompid --config /home/pi/boompi-dev.toml >>/tmp/boompid.log 2>&1 < /dev/null &
    disown
    sleep 3
    P=\$(pgrep -x boompid) || { echo 'ERROR: boompid did not start' >&2; exit 1; }
    readlink /proc/\$P/exe | grep -q deleted && { echo 'ERROR: stale inode' >&2; exit 1; }
    curl -sf http://127.0.0.1:3001/healthz > /dev/null || { echo 'ERROR: healthz' >&2; exit 1; }
fi
if printf '%s\n' ${WANT[*]} | grep -qx boompi-ui; then
    nohup env SLINT_KMS_ROTATION=270 ./boompi-ui >/tmp/boompi-ui.log 2>&1 < /dev/null &
    disown
    sleep 2
    pgrep -x boompi-ui > /dev/null || { echo 'ERROR: boompi-ui did not start' >&2; exit 1; }
fi
echo 'deploy OK'
"
