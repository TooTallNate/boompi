################################################################################
#
# libretro-snes9x
#
################################################################################

LIBRETRO_SNES9X_VERSION = 97c65a34a2eb8592de6c7b44a0ad681895684a41
LIBRETRO_SNES9X_SITE = $(call github,libretro,snes9x,$(LIBRETRO_SNES9X_VERSION))
LIBRETRO_SNES9X_LICENSE = LGPL-2.1 + non-commercial

# Toolchain only - never $(TARGET_CONFIGURE_OPTS): command-line make
# variables override the core Makefile's own CFLAGS accumulation
# (-D__LIBRETRO__, version defines, ...) and the build breaks in
# undeclared-identifier ways.
define LIBRETRO_SNES9X_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(MAKE) CC="$(TARGET_CC)" CXX="$(TARGET_CXX)" AR="$(TARGET_AR)" -C $(@D) \
		-C libretro platform=unix GIT_VERSION=boompi
endef

define LIBRETRO_SNES9X_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/libretro/snes9x_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/snes9x_libretro.so
endef

$(eval $(generic-package))
