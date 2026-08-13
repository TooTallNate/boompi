#!/bin/bash
# Assemble the unified A/B SD card image: add the Pi 3 GPU firmware
# pair the rpi-firmware package variant omits, generate the per-slot
# cmdline variants + autoboot.txt, then stitch everything with
# genimage.

set -e

BOARD_DIR="$(dirname "$0")"
GENIMAGE_CFG="${BOARD_DIR}/genimage.cfg"
GENIMAGE_TMP="${BUILD_DIR}/genimage.tmp"

# The rpi-firmware package installs the Pi 4 variant (start4.elf +
# fixup4.dat) plus bootcode.bin; the Pi 3's GPU wants the plain
# start.elf/fixup.dat pair from the same firmware release. Copy them
# from the package source so both boards boot the same FAT.
RPI_FW_SRC="$(ls -d "${BUILD_DIR}"/rpi-firmware-*/boot 2>/dev/null | head -1)"
[ -n "${RPI_FW_SRC}" ] || { echo "rpi-firmware source dir not found" >&2; exit 1; }
cp "${RPI_FW_SRC}/start.elf" "${RPI_FW_SRC}/fixup.dat" "${BINARIES_DIR}/rpi-firmware/"

# Per-slot kernel cmdlines: identical except the root device.
BASE_CMDLINE="$(cat "${BOARD_DIR}/cmdline.txt")"
echo "${BASE_CMDLINE}" | sed 's|root=/dev/mmcblk0p[0-9]*|root=/dev/mmcblk0p3|' \
	> "${BINARIES_DIR}/cmdline-a.txt"
echo "${BASE_CMDLINE}" | sed 's|root=/dev/mmcblk0p[0-9]*|root=/dev/mmcblk0p5|' \
	> "${BINARIES_DIR}/cmdline-b.txt"

# Initial boot selection: slot A. Updates are trialled per board
# (boompi-trial-boot) and committed by rewriting this file - no
# [tryboot] section; firmware tryboot is unusable on this hardware.
cat > "${BINARIES_DIR}/autoboot.txt" <<EOF
[all]
boot_partition=1
EOF

# genimage copies rootpath into its tmp dir; we only stitch prebuilt
# images together, so hand it an empty dir.
ROOTPATH_TMP="$(mktemp -d)"
trap 'rm -rf "${ROOTPATH_TMP}"' EXIT

rm -rf "${GENIMAGE_TMP}"

genimage \
	--rootpath "${ROOTPATH_TMP}" \
	--tmppath "${GENIMAGE_TMP}" \
	--inputpath "${BINARIES_DIR}" \
	--outputpath "${BINARIES_DIR}" \
	--config "${GENIMAGE_CFG}"
