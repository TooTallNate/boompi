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

# --- A/B update mechanism (both boards). -------------------------------------
# Trial boots are PM_RSTS one-shots (Pi 3, via devmem) or autoboot.txt
# flips (Pi 4); kexec is retired. Assert the pieces the mechanism (and
# the box-profile re-materialization it performs) actually execs.
for tool in boompi-update-slot boompi-trial-boot boompi-boot-commit \
            boompi-apply-box-config; do
    [ -x "${TARGET_DIR}/usr/bin/$tool" ] \
        || fail "$tool missing (A/B updater)"
done
find "${TARGET_DIR}/usr/bin" "${TARGET_DIR}/usr/sbin" \
     "${TARGET_DIR}/bin" "${TARGET_DIR}/sbin" \
     -maxdepth 1 -name devmem 2>/dev/null | grep -q . \
    || fail "devmem missing (Pi 3 PM_RSTS trial boot needs it)"

# --- Onboard Bluetooth UART firmware (both boards). --------------------------
# The generic images leave onboard BT enabled; without the .hcd
# firmware hci0 never appears (pairing shows "unavailable"). Both
# families ship in every image so the images stay identical.
for hcd in BCM4345C0 BCM43430A1; do
    find "${TARGET_DIR}/lib/firmware" -name "${hcd}*.hcd" 2>/dev/null | grep -q . \
        || fail "${hcd}.hcd missing (onboard Bluetooth firmware)"
done

# --- USB Bluetooth dongle firmware (both boards). -----------------------------
# Generic-image promise: the common dongle chipset families work out
# of the box. One representative file per family. (The UB600
# additionally relies on the btusb quirks backport in patches/linux/,
# which buildroot hard-fails on if it stops applying.)
for fw in rtl_bt/rtl8761bu_fw.bin rtl_bt/rtl8761bu_config.bin \
          rtl_bt/rtl8761b_fw.bin \
          mediatek/BT_RAM_CODE_MT7961_1_2_hdr.bin \
          mediatek/BT_RAM_CODE_MT7922_1_1_hdr.bin; do
    [ -f "${TARGET_DIR}/lib/firmware/$fw" ] \
        || fail "$fw missing (USB Bluetooth dongle firmware)"
done
find "${TARGET_DIR}/lib/firmware/rtl_bt" -name "rtl88*.bin" 2>/dev/null | grep -q . \
    || fail "rtl_bt/rtl88*.bin missing (Realtek combo BT firmware)"

echo "post-build assertions OK"
