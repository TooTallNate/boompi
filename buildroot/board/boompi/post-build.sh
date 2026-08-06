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

# Buildroot's OpenSSH installs a sshd_config *without* upstream's Include
# line, which silently ignores the drop-in and would ship a locked-out
# image. Inject it at the top: sshd's first-obtained-value-wins semantics
# make the drop-in authoritative.
if ! grep -q "^Include /etc/ssh/sshd_config.d" "${TARGET_DIR}/etc/ssh/sshd_config"; then
    sed -i '1i Include /etc/ssh/sshd_config.d/*.conf' "${TARGET_DIR}/etc/ssh/sshd_config"
fi

grep -q "^Include /etc/ssh/sshd_config.d" "${TARGET_DIR}/etc/ssh/sshd_config" \
    || fail "sshd_config lacks the Include line - the drop-in would be ignored"

[ -e "${TARGET_DIR}/usr/sbin/sshd" ] || [ -e "${TARGET_DIR}/usr/bin/sshd" ] \
    || fail "sshd binary missing"

# Overlay files land root-owned in the image regardless of build-host
# ownership, but permissions are preserved - sshd rejects sloppy ones.
chmod 700 "${TARGET_DIR}/root/.ssh"
chmod 600 "${TARGET_DIR}/root/.ssh/authorized_keys"

# --- State/persistence invariants. ------------------------------------------
[ -f "${TARGET_DIR}/etc/systemd/system/data.mount" ] \
    || fail "missing data.mount (persistent /data)"

[ -f "${TARGET_DIR}/etc/wireplumber/wireplumber.conf.d/50-boompi.conf" ] \
    || fail "missing wireplumber overrides (A2DP roles / anti-crackle)"

# --- Runtime binaries boompid/the panel shell out to. -----------------------
# Kconfig silently drops packages with unmet `depends on`, which can
# ship an image without WirePlumber (no Lua) or without pw-cat/nmcli:
# assert every binary we exec actually landed.
for bin in nmcli wireplumber wpctl pw-cat pw-record \
           shairport-sync avahi-daemon dnsmasq; do
    find "${TARGET_DIR}/usr/bin" "${TARGET_DIR}/usr/sbin" \
         "${TARGET_DIR}/bin" "${TARGET_DIR}/sbin" \
         -maxdepth 1 -name "$bin" 2>/dev/null | grep -q . \
        || fail "runtime binary '$bin' missing from the image"
done

# --- AVRCP cover art plumbing (both boards). ---------------------------------
# obexd only speaks to a session bus; the image runs a private one
# (obex-bus.service). Missing pieces here = silently no cover art.
[ -x "${TARGET_DIR}/usr/libexec/bluetooth/obexd" ] \
    || fail "obexd binary missing (AVRCP cover art)"
[ -f "${TARGET_DIR}/etc/systemd/system/obexd.service" ] \
    || fail "obexd.service missing (AVRCP cover art)"
grep -q "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/obex-bus" \
    "${TARGET_DIR}/etc/systemd/system/boompid.service" \
    || fail "boompid.service lacks the obex bus environment"

# --- A/B update mechanism (boards with the pi4 overlay). ---------------------
# The trial boot is kexec-based (firmware tryboot is unusable: Pi 4B
# pre-1.4 reboots power-cycle and wipe the flag; Pi 3 has no tryboot
# EEPROM). An A/B image without kexec cannot take updates safely.
if [ -x "${TARGET_DIR}/usr/bin/boompi-update-slot" ]; then
    find "${TARGET_DIR}/usr/bin" "${TARGET_DIR}/usr/sbin" \
         "${TARGET_DIR}/bin" "${TARGET_DIR}/sbin" \
         -maxdepth 1 -name kexec 2>/dev/null | grep -q . \
        || fail "kexec missing (A/B trial boot needs it)"

    # Pi 4 box: onboard Bluetooth (BCM43455) - the UART BT firmware must
    # ship or hci0 never appears (and pairing shows "unavailable").
    find "${TARGET_DIR}/lib/firmware" -name "BCM4345C0*.hcd" 2>/dev/null | grep -q . \
        || fail "BCM4345C0.hcd missing (onboard Bluetooth firmware)"
fi

echo "post-build assertions OK"
