#!/bin/bash
# Build-time assertions: fail the image build (i.e. CI) when access or
# state invariants are broken, instead of discovering them on a bench
# with the SD card sealed inside a boombox.
#
# Runs after the rootfs overlays are applied; $TARGET_DIR is the rootfs.
set -eu

fail() {
    echo "post-build assertion FAILED: $*" >&2
    exit 1
}

# --- SSH access: never ship an image we cannot get into. -------------------
[ -s "${TARGET_DIR}/root/.ssh/authorized_keys" ] \
    || fail "missing /root/.ssh/authorized_keys (key-based root SSH)"

[ -f "${TARGET_DIR}/etc/ssh/sshd_config.d/boompi.conf" ] \
    || fail "missing sshd_config.d/boompi.conf (PermitRootLogin)"

grep -q "^Include /etc/ssh/sshd_config.d" "${TARGET_DIR}/etc/ssh/sshd_config" \
    || fail "sshd_config lacks the Include line — the drop-in would be ignored"

[ -e "${TARGET_DIR}/usr/sbin/sshd" ] || [ -e "${TARGET_DIR}/usr/bin/sshd" ] \
    || fail "sshd binary missing"

# Overlay files land root-owned in the image regardless of build-host
# ownership, but permissions are preserved — sshd rejects sloppy ones.
chmod 700 "${TARGET_DIR}/root/.ssh"
chmod 600 "${TARGET_DIR}/root/.ssh/authorized_keys"

# --- State/persistence invariants. ------------------------------------------
[ -f "${TARGET_DIR}/etc/systemd/system/data.mount" ] \
    || fail "missing data.mount (persistent /data)"

[ -f "${TARGET_DIR}/etc/wireplumber/wireplumber.conf.d/50-boompi.conf" ] \
    || fail "missing wireplumber overrides (A2DP roles / anti-crackle)"

echo "post-build assertions OK"
