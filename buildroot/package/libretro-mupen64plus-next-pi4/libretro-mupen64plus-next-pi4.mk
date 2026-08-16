################################################################################
#
# libretro-mupen64plus-next-pi4
#
################################################################################

LIBRETRO_MUPEN64PLUS_NEXT_PI4_VERSION = 3a676196500545b637b83cb19fb393d2359e1f9d
LIBRETRO_MUPEN64PLUS_NEXT_PI4_SITE = $(call github,libretro,mupen64plus-libretro-nx,$(LIBRETRO_MUPEN64PLUS_NEXT_PI4_VERSION))
LIBRETRO_MUPEN64PLUS_NEXT_PI4_LICENSE = GPL-3.0
LIBRETRO_MUPEN64PLUS_NEXT_PI4_DEPENDENCIES = zlib mesa3d

# The Pi 4 variant of the N64 core: Batocera's BCM2711 recipe
# (platform=rpi4_64 - Cortex-A72 tuning, GLES3+MESA implied by the
# platform). The rpi3_64-mesa build is proven on the Pi 3's A53 but
# its recompiler jumps into never-written JIT memory on the A72
# (bench core dumps on both boards, same binary: A53 runs, A72 dies
# at the first block). Batocera never runs one build on both boards;
# neither do we. Same flag discipline as the pi3 package: ARCH
# explicit (uname leak), CFLAGS as environment, SYSTEM_ZLIB for the
# gcc-14 bundled-zlib breakage.
define LIBRETRO_MUPEN64PLUS_NEXT_PI4_BUILD_CMDS
	cd $(@D) && \
	$(TARGET_MAKE_ENV) \
	CFLAGS="$(TARGET_CFLAGS) -DEGL_NO_X11" \
	CXXFLAGS="$(TARGET_CXXFLAGS) -DEGL_NO_X11" \
	LDFLAGS="$(TARGET_LDFLAGS)" \
	$(MAKE) CC="$(TARGET_CC)" CXX="$(TARGET_CXX)" AR="$(TARGET_AR)" \
		platform=rpi4_64 ARCH=aarch64 SYSTEM_ZLIB=1 GIT_VERSION=boompi
endef

define LIBRETRO_MUPEN64PLUS_NEXT_PI4_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/mupen64plus_next_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/mupen64plus_next_pi4_libretro.so
endef

$(eval $(generic-package))
