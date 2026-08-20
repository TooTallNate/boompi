# Boompi appliance - shared Buildroot configuration fragment.
#
# This is NOT a complete defconfig: concatenate it with a board fragment
# (boompi-pi3.frag / boompi-pi4.frag) to get one - the board fragments
# hold only genuine hardware deltas (CPU, kernel, boot firmware, TF-A
# platform, board overlay). scripts/gen-defconfig.sh does the merge;
# CI feeds the result to `make defconfig BR2_DEFCONFIG=...`. Everything
# feature-level lives here so the two boxes cannot silently diverge.
#
# App binaries (boompid, boompi-ui) are injected via rootfs-overlay-ci by
# CI (cross-built with cargo-zigbuild against this build's staging sysroot)
# - see .github/workflows/image.yml.

BR2_aarch64=y

# Prebuilt Bootlin toolchain (saves ~40 min of gcc bootstrap per CI run)
BR2_TOOLCHAIN_EXTERNAL=y
BR2_TOOLCHAIN_EXTERNAL_BOOTLIN=y
BR2_TOOLCHAIN_EXTERNAL_BOOTLIN_AARCH64_GLIBC_STABLE=y

BR2_CCACHE=y

# No BR2_SYSTEM_DHCP: NetworkManager owns all interfaces (ethernet DHCPs
# by default); a networkd config here would fight it for eth0.
BR2_TARGET_GENERIC_HOSTNAME="boompi"
BR2_TARGET_GENERIC_ISSUE="Boompi v2"
BR2_TARGET_GENERIC_ROOT_PASSWD="boompi"
BR2_INIT_SYSTEMD=y
# NetworkManager owns all interfaces; networkd would fight it for eth0
# (and its wait-online unit fails the boot health picture).
# BR2_PACKAGE_SYSTEMD_NETWORKD is not set

# Games easter egg: RetroArch (KMS/GBM + GLES2, same display stack as
# the panel UI) + the six shipped libretro cores. ROMs are user
# content on /data/games; nothing copyrighted ships in the image.
BR2_PACKAGE_RETROARCH=y
BR2_PACKAGE_LIBRETRO_CORES=y

# Guest SMB share of the games library (drag-drop ROMs from any OS;
# scoped to /data/games - see the smb.conf comments and SECURITY.md).
BR2_PACKAGE_SAMBA4=y

# Partition tooling for boompi-grow-data (grow /data to fill the SD
# card on first boot): sfdisk + partx from util-linux, resize2fs +
# e2fsck from e2fsprogs.
BR2_PACKAGE_UTIL_LINUX=y
BR2_PACKAGE_UTIL_LINUX_BINARIES=y
BR2_PACKAGE_UTIL_LINUX_FDISK=y
BR2_PACKAGE_UTIL_LINUX_PARTX=y
BR2_PACKAGE_E2FSPROGS=y
BR2_PACKAGE_E2FSPROGS_RESIZE2FS=y
BR2_PACKAGE_E2FSPROGS_FSCK=y

# Package patches (e.g. the bluez AVRCP absolute-volume backport).
BR2_GLOBAL_PATCH_DIR="$(BR2_EXTERNAL_BOOMPI_PATH)/patches"
BR2_ROOTFS_POST_BUILD_SCRIPT="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/post-build.sh"

# Kernel: same Raspberry Pi kernel pin as upstream raspberrypi3_64_defconfig
BR2_PACKAGE_HOST_LINUX_HEADERS_CUSTOM_6_6=y
BR2_LINUX_KERNEL=y
BR2_LINUX_KERNEL_CUSTOM_TARBALL=y
BR2_LINUX_KERNEL_CUSTOM_TARBALL_LOCATION="$(call github,raspberrypi,linux,bba53a117a4a5c29da892962332ff1605990e17a)/linux-bba53a117a4a5c29da892962332ff1605990e17a.tar.gz"
# kexec for A/B trial boots - firmware tryboot is not used on either
# board (see board/boompi/linux-kexec.fragment).
BR2_LINUX_KERNEL_CONFIG_FRAGMENT_FILES="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/linux-kexec.fragment $(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/linux-bt.fragment $(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/linux-audio.fragment $(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/linux-gamepads.fragment"
BR2_LINUX_KERNEL_DTS_SUPPORT=y
BR2_LINUX_KERNEL_DTB_OVERLAY_SUPPORT=y
BR2_LINUX_KERNEL_NEEDS_HOST_OPENSSL=y

# Raspberry Pi boot firmware + matching dtb overlays
# (vc4-kms-dpi-hyperpixel4, disable-bt, ...)
BR2_PACKAGE_RPI_FIRMWARE=y
BR2_PACKAGE_RPI_FIRMWARE_INSTALL_DTB_OVERLAYS=y


# System bits
BR2_PACKAGE_BUSYBOX_SHOW_OTHERS=y
BR2_PACKAGE_BUSYBOX_CONFIG_FRAGMENT_FILES="$(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/busybox.fragment"
# kexec(8): chain-loads the candidate slot's kernel for A/B update
# trials (boompi-update-slot) without touching autoboot.txt.
BR2_PACKAGE_KEXEC=y
BR2_PACKAGE_KMOD=y
BR2_PACKAGE_KMOD_TOOLS=y
BR2_PACKAGE_XZ=y
BR2_PACKAGE_I2C_TOOLS=y

# Bluetooth: BlueZ with experimental (AVRCP cover art), audio plugins,
# bluetoothctl, obexd (BIP client)
BR2_PACKAGE_BLUEZ5_UTILS=y
BR2_PACKAGE_BLUEZ5_UTILS_CLIENT=y
BR2_PACKAGE_BLUEZ5_UTILS_OBEX=y
BR2_PACKAGE_BLUEZ5_UTILS_TOOLS=y
BR2_PACKAGE_BLUEZ5_UTILS_EXPERIMENTAL=y
BR2_PACKAGE_BLUEZ5_UTILS_PLUGINS_AUDIO=y
# HID plugin: without it bluetoothd has no input profile at all -
# a paired DualSense connects, finds "no more profiles to connect
# to", is dropped, and powers itself off. Classic BT pads (DS4/DS5,
# Switch Pro) need HID; the option selects HOG for BLE pads (newer
# Xbox). Kernel side (hidp/uhid/hid-playstation) was already in
# linux-gamepads.fragment.
BR2_PACKAGE_BLUEZ5_UTILS_PLUGINS_HID=y
# No bluez-tools/bt-agent package: boompid registers its own Agent1
# (NoInputNoOutput/JustWorks). Consent = the explicit pairing window:
# while it's open, any pairing is accepted; no on-screen confirm.

# Audio: PipeWire + WirePlumber + SBC codec (A2DP).
# NB: Kconfig silently drops packages with unmet
# `depends on` - WirePlumber needs Lua, pw-cat/pw-record need libsndfile.
# post-build.sh asserts the binaries actually landed.
BR2_PACKAGE_PIPEWIRE=y
BR2_PACKAGE_WIREPLUMBER=y
BR2_PACKAGE_LUA=y
BR2_PACKAGE_LUA_5_4=y
BR2_PACKAGE_LIBSNDFILE=y
BR2_PACKAGE_SBC=y
BR2_PACKAGE_ALSA_UTILS=y

# mDNS: avahi is the single owner of UDP :5353 - shairport-sync links
# against it and librespot's discovery uses its D-Bus API. (Running a second
# responder, e.g. librespot's libmdns, alongside a daemon on :5353 dies with
# 'responder died' panics.)
BR2_PACKAGE_AVAHI=y
BR2_PACKAGE_AVAHI_DAEMON=y
BR2_PACKAGE_AVAHI_LIBAVAHI_CLIENT=y

# AirPlay 2: shairport-sync 4.3.7 (Buildroot ≥ 2026.02), spawned by boompid
# (pipe backend → pw-cat; org.gnome.ShairportSync D-Bus control); the dbus
# policy files ship with the package. AIRPLAY2 selects nqptp (PTP clock
# daemon - systemd unit lives in our overlay; the package only ships SysV)
# plus ffmpeg/libplist/libsodium/libgcrypt.
BR2_PACKAGE_SHAIRPORT_SYNC=y
BR2_PACKAGE_SHAIRPORT_SYNC_AIRPLAY2=y
BR2_PACKAGE_SHAIRPORT_SYNC_DBUS=y

# Wi-Fi: NetworkManager on the appliance to match the dev box (RPi OS) -
# one D-Bus/nmcli code path in boompid. `shared` connections give the
# onboarding AP + built-in DHCP (dnsmasq). Onboard brcmfmac needs the
# firmware blob + wireless-regdb; wpa_supplicant needs D-Bus control for NM.
BR2_PACKAGE_NETWORK_MANAGER=y
BR2_PACKAGE_NETWORK_MANAGER_CLI=y
BR2_PACKAGE_WPA_SUPPLICANT=y
BR2_PACKAGE_WPA_SUPPLICANT_AP_SUPPORT=y
BR2_PACKAGE_WPA_SUPPLICANT_DBUS=y
BR2_PACKAGE_DNSMASQ=y
BR2_PACKAGE_WIRELESS_REGDB=y
BR2_PACKAGE_BRCMFMAC_SDIO_FIRMWARE_RPI=y
BR2_PACKAGE_BRCMFMAC_SDIO_FIRMWARE_RPI_WIFI=y
# Onboard Bluetooth UART firmware for every board (BCM43430A1.hcd for
# the Pi 3's BCM43438, BCM4345C0.hcd for the Pi 4's BCM43455): the
# generic images leave onboard BT enabled, so hci0 must come up on an
# unprovisioned box. Profiles that use a USB dongle disable-bt anyway.
BR2_PACKAGE_BRCMFMAC_SDIO_FIRMWARE_RPI_BT=y

# USB Bluetooth dongle firmware. Boompi OS aims to be a generic
# image, so the common dongle chipset families work out of the box -
# without their firmware a dongle enumerates but hci0 never appears
# (or stays in ROM-only mode and scans find nothing), a miserable
# failure mode inside a sealed enclosure.
# - RTL_87XX_BT: RTL8761B/BU - TP-Link UB400v2/UB500/UB600, ASUS
#   USB-BT500, Edimax BT-8500 (the recommended family). NB the UB600
#   also needs the quirks-table backport in patches/linux/.
# - RTL_88XX_BT: RTL8821/8822/8852 WiFi+BT combo USB adapters.
# - MT7921/MT7922 BT: MediaTek combo USB adapters (MT7921AU et al).
# CSR8510 dongles (UB400v1) need no firmware; old Broadcom BCM20702
# dongles need vendor blobs linux-firmware may not ship - not covered.
BR2_PACKAGE_LINUX_FIRMWARE=y
BR2_PACKAGE_LINUX_FIRMWARE_RTL_87XX_BT=y
BR2_PACKAGE_LINUX_FIRMWARE_RTL_88XX_BT=y
BR2_PACKAGE_LINUX_FIRMWARE_MEDIATEK_MT7921_BT=y
BR2_PACKAGE_LINUX_FIRMWARE_MEDIATEK_MT7922_BT=y

# UI runtime deps (Slint linuxkms + Skia GPU renderer on both boards:
# EGL/GLES on KMS/GBM via mesa - V3D binds on the Pi 4, VC4 on the
# Pi 3; both drivers ship in both images so the config stays identical.
# Skia GL on the Pi 3 was validated in Phase 0, docs/PHASE0-PI3.md).
BR2_PACKAGE_LIBINPUT=y
BR2_PACKAGE_LIBXKBCOMMON=y
BR2_PACKAGE_FONTCONFIG=y
# libpng: freetype only decodes CBDT color-emoji bitmaps (Noto Color
# Emoji) when built with PNG support; without it the glyphs are found
# but rasterize as empty.
BR2_PACKAGE_LIBPNG=y
BR2_PACKAGE_MESA3D=y
BR2_PACKAGE_MESA3D_GALLIUM_DRIVER_V3D=y
BR2_PACKAGE_MESA3D_GALLIUM_DRIVER_VC4=y
BR2_PACKAGE_MESA3D_GBM=y
BR2_PACKAGE_MESA3D_OPENGL_EGL=y
BR2_PACKAGE_MESA3D_OPENGL_ES=y

# Dev access + bench quality-of-life
BR2_PACKAGE_OPENSSH=y
BR2_PACKAGE_VIM=y
BR2_PACKAGE_HTOP=y
BR2_PACKAGE_IPERF3=y
BR2_PACKAGE_LIBCURL=y
BR2_PACKAGE_LIBCURL_CURL=y
BR2_PACKAGE_LIBCURL_VERBOSE=y
# CA trust store for on-box curl/wget HTTPS. boompid's updater doesn't
# need it (rustls + compiled-in webpki roots), but without it every
# other HTTPS client on the box fails with a trust-anchor error.
BR2_PACKAGE_CA_CERTIFICATES=y

# Filesystem / image
BR2_TARGET_ROOTFS_EXT2=y
BR2_TARGET_ROOTFS_EXT2_4=y
BR2_TARGET_ROOTFS_EXT2_SIZE="1024M"
# BR2_TARGET_ROOTFS_TAR is not set
BR2_PACKAGE_HOST_DOSFSTOOLS=y
BR2_PACKAGE_HOST_GENIMAGE=y
BR2_PACKAGE_HOST_KMOD_XZ=y
BR2_PACKAGE_HOST_MTOOLS=y
