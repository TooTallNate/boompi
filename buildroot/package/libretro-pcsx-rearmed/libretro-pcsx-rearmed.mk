################################################################################
#
# libretro-pcsx-rearmed
#
################################################################################

LIBRETRO_PCSX_REARMED_VERSION = da2cb8ecd17fd0932ab6d94774c0522beebce6e3
LIBRETRO_PCSX_REARMED_SITE = $(call github,libretro,pcsx_rearmed,$(LIBRETRO_PCSX_REARMED_VERSION))
LIBRETRO_PCSX_REARMED_LICENSE = GPL-2.0

# Toolchain only - never $(TARGET_CONFIGURE_OPTS): command-line make
# variables override the core Makefile's own CFLAGS accumulation
# (-D__LIBRETRO__, version defines, ...) and the build breaks in
# undeclared-identifier ways.
define LIBRETRO_PCSX_REARMED_BUILD_CMDS
	$(TARGET_MAKE_ENV) $(MAKE) CC="$(TARGET_CC)" CXX="$(TARGET_CXX)" AR="$(TARGET_AR)" -C $(@D) \
		-f Makefile.libretro platform=unix ARCH=aarch64 DYNAREC=ari64 GIT_VERSION=boompi
endef

define LIBRETRO_PCSX_REARMED_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/pcsx_rearmed_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/pcsx_rearmed_libretro.so
endef

$(eval $(generic-package))
