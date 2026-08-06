#!/bin/bash
# Assemble the Pi 3 A/B SD card image: generate the per-slot cmdline
# variants + autoboot.txt, then stitch everything with genimage.
# Mirrors pi4/post-image.sh so both boards share the OTA scripts.

set -e

BOARD_DIR="$(dirname "$0")"
GENIMAGE_CFG="${BOARD_DIR}/genimage.cfg"
GENIMAGE_TMP="${BUILD_DIR}/genimage.tmp"

# Per-slot kernel cmdlines: identical except the root device.
BASE_CMDLINE="$(cat "${BOARD_DIR}/cmdline.txt")"
echo "${BASE_CMDLINE}" | sed 's|root=/dev/mmcblk0p[0-9]*|root=/dev/mmcblk0p3|' \
	> "${BINARIES_DIR}/cmdline-a.txt"
echo "${BASE_CMDLINE}" | sed 's|root=/dev/mmcblk0p[0-9]*|root=/dev/mmcblk0p5|' \
	> "${BINARIES_DIR}/cmdline-b.txt"

# Initial boot selection: slot A. Updates are trialled via kexec
# (boompi-update-slot) and committed by rewriting this file.
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
