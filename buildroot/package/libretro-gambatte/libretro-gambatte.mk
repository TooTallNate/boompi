################################################################################
#
# libretro-gambatte
#
################################################################################

LIBRETRO_GAMBATTE_VERSION = 96174369b3c30d9fc57c926fa3379c273dc6a9a5
LIBRETRO_GAMBATTE_SITE = $(call github,libretro,gambatte-libretro,$(LIBRETRO_GAMBATTE_VERSION))
LIBRETRO_GAMBATTE_LICENSE = GPL-2.0

# Toolchain only - never $(TARGET_CONFIGURE_OPTS): command-line make
# variables override the core Makefile's own CFLAGS accumulation
# (-D__LIBRETRO__, version defines, ...) and the build breaks in
# undeclared-identifier ways.
define LIBRETRO_GAMBATTE_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(MAKE) CC="$(TARGET_CC)" CXX="$(TARGET_CXX)" AR="$(TARGET_AR)" -C $(@D) \
		-f Makefile platform=unix GIT_VERSION=boompi
endef

define LIBRETRO_GAMBATTE_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/gambatte_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/gambatte_libretro.so
endef

$(eval $(generic-package))
