################################################################################
#
# libretro-mgba
#
################################################################################

LIBRETRO_MGBA_VERSION = e31759b24e7a4e3899285ff720d7b573ac328ae7
LIBRETRO_MGBA_SITE = $(call github,libretro,mgba,$(LIBRETRO_MGBA_VERSION))
LIBRETRO_MGBA_LICENSE = MPL-2.0

# Toolchain only - never $(TARGET_CONFIGURE_OPTS): command-line make
# variables override the core Makefile's own CFLAGS accumulation
# (-D__LIBRETRO__, version defines, ...) and the build breaks in
# undeclared-identifier ways.
define LIBRETRO_MGBA_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(MAKE) CC="$(TARGET_CC)" CXX="$(TARGET_CXX)" AR="$(TARGET_AR)" -C $(@D) \
		-f Makefile.libretro platform=unix GIT_VERSION=boompi
endef

define LIBRETRO_MGBA_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/mgba_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/mgba_libretro.so
endef

$(eval $(generic-package))
