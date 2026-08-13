# Boompi Pi 3 box - hardware deltas only (see boompi-common.frag).
# HyperPixel 4.0 (DPI/KMS), USB audio adapter, USB CSR BT dongle
# (onboard radio disabled), INA260 on the HyperPixel's bit-banged i2c.
# Based on Buildroot's raspberrypi3_64_defconfig.

BR2_cortex_a53=y

BR2_ROOTFS_OVERLAY="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/rootfs-overlay $(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/rootfs-overlay-ci"
BR2_ROOTFS_POST_IMAGE_SCRIPT="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/pi3/post-image.sh"

# Same kernel config as the Pi 4 (and as Raspberry Pi OS's kernel8.img,
# which boots Pi 3/3+/4/Zero 2 from one binary): the first step toward
# a single unified image is both boards building the identical kernel.
BR2_LINUX_KERNEL_DEFCONFIG="bcm2711"
BR2_LINUX_KERNEL_INTREE_DTS_NAME="broadcom/bcm2710-rpi-3-b"

# Pi 3 boots via bootcode.bin (supports autoboot.txt/boot_partition).
BR2_PACKAGE_RPI_FIRMWARE_BOOTCODE_BIN=y
BR2_PACKAGE_RPI_FIRMWARE_VARIANT_PI=y
BR2_PACKAGE_RPI_FIRMWARE_CONFIG_FILE="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/pi3/config.txt"
BR2_PACKAGE_RPI_FIRMWARE_CMDLINE_FILE="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/pi3/cmdline.txt"

# No TF-A: the rpi3 armstub existed to provide PSCI for kexec (parked
# CPUs), and kexec is retired. Stock firmware boot chain - no fixed
# kernel/DTB addresses, spin-table SMP. This also frees the bcm2711
# kernel (bigger than bcmrpi3's) from the old 24MB Image ceiling the
# fixed addresses imposed.
