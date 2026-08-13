# Boompi unified board fragment - one image boots every supported
# board (Pi 3 + Pi 4). Everything shared with no board angle lives in
# boompi-common.frag; this file is what remains of the per-board
# fragments after the box profiles (/data/box/) absorbed the
# hardware-specific config.

# Lowest common denominator CPU: the Pi 3's Cortex-A53. The Pi 4's
# A72 runs the same ARMv8-A code; per-CPU scheduling tuning was never
# worth a second image.
BR2_cortex_a53=y

BR2_ROOTFS_OVERLAY="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/rootfs-overlay $(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/rootfs-overlay-ci"
BR2_ROOTFS_POST_IMAGE_SCRIPT="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/image/post-image.sh"

# One kernel for both boards: bcm2711_defconfig is what Raspberry Pi
# OS builds kernel8.img from (boots Pi 3/3+/4/Zero 2 from one binary).
# Both DTBs ship on the boot FAT; the firmware picks its own.
BR2_LINUX_KERNEL_DEFCONFIG="bcm2711"
BR2_LINUX_KERNEL_INTREE_DTS_NAME="broadcom/bcm2710-rpi-3-b broadcom/bcm2711-rpi-4-b"

# Firmware: the package installs the Pi 4 GPU variant (start4.elf);
# bootcode.bin is the Pi 3's first-stage loader (and the thing that
# reads autoboot.txt/boot_partition there). post-image.sh copies the
# Pi 3 GPU pair (start.elf/fixup.dat) from the same package release.
BR2_PACKAGE_RPI_FIRMWARE_BOOTCODE_BIN=y
BR2_PACKAGE_RPI_FIRMWARE_VARIANT_PI4=y
BR2_PACKAGE_RPI_FIRMWARE_CONFIG_FILE="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/image/config.txt"
BR2_PACKAGE_RPI_FIRMWARE_CMDLINE_FILE="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/image/cmdline.txt"

# TF-A rpi4 port (bl31.bin, loaded via armstub=bl31.bin under the
# [pi4] section of config.txt; inert on the Pi 3). Historical: PSCI
# for the retired kexec trial mechanism. Removing it is a separate
# trial - the pi4's rollback path is harsher, so it goes last.
BR2_TARGET_ARM_TRUSTED_FIRMWARE=y
BR2_TARGET_ARM_TRUSTED_FIRMWARE_PLATFORM="rpi4"
BR2_TARGET_ARM_TRUSTED_FIRMWARE_BL31=y
