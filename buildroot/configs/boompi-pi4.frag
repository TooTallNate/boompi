# Boompi Pi 4 box - hardware deltas only (see boompi-common.frag).
# 1024×600 HDMI touch panel, Raspiaudio Audio+ DAC HAT, onboard
# Bluetooth (BCM43455), INA260 on the standard I2C bus.
# Based on Buildroot's raspberrypi4_64_defconfig.

BR2_cortex_a72=y

BR2_ROOTFS_OVERLAY="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/rootfs-overlay $(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/rootfs-overlay-ci"
BR2_ROOTFS_POST_IMAGE_SCRIPT="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/pi4/post-image.sh"

BR2_LINUX_KERNEL_DEFCONFIG="bcm2711"
BR2_LINUX_KERNEL_INTREE_DTS_NAME="broadcom/bcm2711-rpi-4-b"

# Pi 4 boots from EEPROM; no bootcode.bin.
BR2_PACKAGE_RPI_FIRMWARE_VARIANT_PI4=y
BR2_PACKAGE_RPI_FIRMWARE_CONFIG_FILE="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/pi4/config.txt"
BR2_PACKAGE_RPI_FIRMWARE_CMDLINE_FILE="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/pi4/cmdline.txt"

# TF-A rpi4 port (bl31.bin, loaded via armstub=bl31.bin in config.txt).
# Historical: PSCI for the retired kexec trial mechanism. The pi3
# dropped its armstub with kexec; removing this one is a separate
# trial (the pi4's rollback path is harsher, so it goes last).
BR2_TARGET_ARM_TRUSTED_FIRMWARE=y
BR2_TARGET_ARM_TRUSTED_FIRMWARE_PLATFORM="rpi4"
BR2_TARGET_ARM_TRUSTED_FIRMWARE_BL31=y

# Onboard Bluetooth (BCM43455): UART BT firmware (BCM4345C0.hcd). The
# Pi 4 box has no USB dongle - v1 used the onboard radio. (The Pi 3 box
# disables its onboard radio and uses a USB dongle instead.)
