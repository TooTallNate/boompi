################################################################################
#
# retroarch
#
################################################################################

RETROARCH_VERSION = 1.22.2
RETROARCH_SITE = $(call github,libretro,RetroArch,v$(RETROARCH_VERSION))
RETROARCH_LICENSE = GPL-3.0
RETROARCH_LICENSE_FILES = COPYING
RETROARCH_DEPENDENCIES = \
	host-pkgconf mesa3d libdrm alsa-lib zlib freetype udev pipewire

# RetroArch's configure is its own "qb" script, not autoconf: it
# accepts --prefix and feature toggles but chokes on the full set of
# flags buildroot's autotools infra passes, so drive it manually.
# Target: KMS/GBM + GLES2 (matches the panel UI's stack), udev
# joypads, PipeWire audio with ALSA fallback, nothing X11/Wayland.
define RETROARCH_CONFIGURE_CMDS
	cd $(@D) && \
	$(TARGET_CONFIGURE_OPTS) \
	PKG_CONF_PATH=$(HOST_DIR)/bin/pkg-config \
	./configure \
		--prefix=/usr \
		--disable-x11 \
		--disable-wayland \
		--disable-sdl \
		--disable-sdl2 \
		--disable-vulkan \
		--disable-opengl \
		--enable-opengles \
		--enable-egl \
		--enable-kms \
		--disable-videocore \
		--enable-udev \
		--enable-alsa \
		--enable-pipewire \
		--disable-pulse \
		--disable-jack \
		--disable-oss \
		--enable-freetype \
		--enable-zlib \
		--enable-threads \
		--enable-networking \
		--disable-discord \
		--disable-qt \
		--disable-cheevos \
		--disable-ffmpeg \
		--disable-vg \
		--disable-cg
endef

define RETROARCH_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(MAKE) -C $(@D)
endef

define RETROARCH_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0755 $(@D)/retroarch $(TARGET_DIR)/usr/bin/retroarch
	$(INSTALL) -d $(TARGET_DIR)/usr/share/retroarch
	cp -r $(@D)/media/assets/xmb $(TARGET_DIR)/usr/share/retroarch/ 2>/dev/null || true
endef

$(eval $(generic-package))
