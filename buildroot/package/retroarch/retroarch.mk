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
# Explicit staging paths + pkg-config env: qb's header checks compile
# with $CC $CFLAGS only, and the configure runs outside buildroot's
# autotools infra (which normally exports the pkg-config sysroot
# vars). config.log is dumped on failure - qb's error messages name
# the header, not the reason.
define RETROARCH_CONFIGURE_CMDS
	cd $(@D) && \
	$(TARGET_CONFIGURE_OPTS) \
	CFLAGS="$(TARGET_CFLAGS) -I$(STAGING_DIR)/usr/include" \
	CXXFLAGS="$(TARGET_CXXFLAGS) -I$(STAGING_DIR)/usr/include" \
	LDFLAGS="$(TARGET_LDFLAGS) -L$(STAGING_DIR)/usr/lib" \
	PKG_CONF_PATH=$(HOST_DIR)/bin/pkg-config \
	PKG_CONFIG=$(HOST_DIR)/bin/pkg-config \
	PKG_CONFIG_SYSROOT_DIR=$(STAGING_DIR) \
	PKG_CONFIG_LIBDIR=$(STAGING_DIR)/usr/lib/pkgconfig:$(STAGING_DIR)/usr/share/pkgconfig \
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
		--disable-cg \
	|| { echo "=== qb config.log ==="; cat config.log; exit 1; }
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
