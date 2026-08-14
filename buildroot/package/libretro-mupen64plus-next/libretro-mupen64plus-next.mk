################################################################################
#
# libretro-mupen64plus-next
#
################################################################################

LIBRETRO_MUPEN64PLUS_NEXT_VERSION = 3a676196500545b637b83cb19fb393d2359e1f9d
LIBRETRO_MUPEN64PLUS_NEXT_SITE = $(call github,libretro,mupen64plus-libretro-nx,$(LIBRETRO_MUPEN64PLUS_NEXT_VERSION))
LIBRETRO_MUPEN64PLUS_NEXT_LICENSE = GPL-3.0
LIBRETRO_MUPEN64PLUS_NEXT_DEPENDENCIES = zlib mesa3d

# SYSTEM_ZLIB: the bundled custom/dependencies/libzlib is missing its
# unistd.h includes and gcc 14 makes implicit declarations fatal;
# staging zlib works fine.
# Toolchain only - never $(TARGET_CONFIGURE_OPTS): command-line make
# variables override the core Makefile's own CFLAGS accumulation
# (-D__LIBRETRO__, version defines, ...) and the build breaks in
# undeclared-identifier ways.
define LIBRETRO_MUPEN64PLUS_NEXT_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(MAKE) CC="$(TARGET_CC)" CXX="$(TARGET_CXX)" AR="$(TARGET_AR)" -C $(@D) \
		platform=unix FORCE_GLES=1 WITH_DYNAREC=aarch64 SYSTEM_ZLIB=1 GIT_VERSION=boompi
endef

define LIBRETRO_MUPEN64PLUS_NEXT_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/mupen64plus_next_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/mupen64plus_next_libretro.so
endef

$(eval $(generic-package))
