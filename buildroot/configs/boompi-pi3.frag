# Boompi Pi 3 box - hardware deltas only (see boompi-common.frag).
# HyperPixel 4.0 (DPI/KMS), USB audio adapter, USB CSR BT dongle
# (onboard radio disabled), INA260 on the HyperPixel's bit-banged i2c.
# Based on Buildroot's raspberrypi3_64_defconfig.

BR2_cortex_a53=y

BR2_ROOTFS_OVERLAY="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/rootfs-overlay $(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/rootfs-overlay-ci"
BR2_ROOTFS_POST_IMAGE_SCRIPT="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/pi3/post-image.sh"

BR2_LINUX_KERNEL_DEFCONFIG="bcmrpi3"
BR2_LINUX_KERNEL_INTREE_DTS_NAME="broadcom/bcm2710-rpi-3-b"

# Pi 3 boots via bootcode.bin (supports autoboot.txt/boot_partition).
BR2_PACKAGE_RPI_FIRMWARE_BOOTCODE_BIN=y
BR2_PACKAGE_RPI_FIRMWARE_VARIANT_PI=y
BR2_PACKAGE_RPI_FIRMWARE_CONFIG_FILE="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/pi3/config.txt"
BR2_PACKAGE_RPI_FIRMWARE_CMDLINE_FILE="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/pi3/cmdline.txt"

# TF-A rpi3 port (armstub8.bin): needs fixed kernel/DTB addresses
# (matching kernel_address/device_tree_address in pi3/config.txt) and a
# local patch to advertise PSCI in the DT like the rpi4 port does
# (patches/arm-trusted-firmware/). 'all' builds armstub8.bin (bl1+fip);
# FIP=y just pulls the host-openssl dependency fiptool needs.
BR2_TARGET_ARM_TRUSTED_FIRMWARE_PLATFORM="rpi3"
BR2_TARGET_ARM_TRUSTED_FIRMWARE_FIP=y
BR2_TARGET_ARM_TRUSTED_FIRMWARE_ADDITIONAL_VARIABLES="RPI3_DIRECT_LINUX_BOOT=1 RPI3_PRELOADED_DTB_BASE=0x03800000 PRELOADED_BL33_BASE=0x02000000"
