################################################################################
#
# libretro-mupen64plus-next
#
################################################################################

LIBRETRO_MUPEN64PLUS_NEXT_VERSION = 3a676196500545b637b83cb19fb393d2359e1f9d
LIBRETRO_MUPEN64PLUS_NEXT_SITE = $(call github,libretro,mupen64plus-libretro-nx,$(LIBRETRO_MUPEN64PLUS_NEXT_VERSION))
LIBRETRO_MUPEN64PLUS_NEXT_LICENSE = GPL-3.0
LIBRETRO_MUPEN64PLUS_NEXT_DEPENDENCIES = zlib mesa3d

# Recipe follows Batocera's proven buildroot cross-build (their
# BCM2837/BCM2711 targets run this core on real Pi hardware):
#
# - ARCH=aarch64 EXPLICITLY: the Makefile defaults ARCH from
#   `uname -m`, which is the x86_64 BUILD HOST in a cross build. The
#   dynarec sources were selected correctly via WITH_DYNAREC, but
#   ARCH-conditional pieces (including the awk-generated asm struct
#   offsets the JIT relies on) went x86_64 - and the resulting
#   recompiler segfaulted on its first emitted block (bench core dump:
#   PC in anonymous JIT memory, garbage stack).
# - platform=rpi3_64-mesa: the dedicated Pi target (Cortex-A53 tuning
#   runs on both our boards; the pi3's VC4 caps us at GLES2, which
#   this platform selects with MESA=1). WITH_DYNAREC=aarch64 is
#   implied by the platform when ARCH=aarch64.
# - CFLAGS/CXXFLAGS as ENVIRONMENT (never make command-line args:
#   those override the Makefile's own += accumulation and break the
#   build). EGL_NO_X11 for X11-less mesa headers.
#
# SYSTEM_ZLIB: the bundled custom/dependencies/libzlib is missing its
# unistd.h includes and gcc 14 makes implicit declarations fatal;
# staging zlib works fine.
define LIBRETRO_MUPEN64PLUS_NEXT_BUILD_CMDS
	cd $(@D) && \
	$(TARGET_MAKE_ENV) \
	CFLAGS="$(TARGET_CFLAGS) -DEGL_NO_X11" \
	CXXFLAGS="$(TARGET_CXXFLAGS) -DEGL_NO_X11" \
	LDFLAGS="$(TARGET_LDFLAGS)" \
	$(MAKE) CC="$(TARGET_CC)" CXX="$(TARGET_CXX)" AR="$(TARGET_AR)" \
		platform=rpi3_64-mesa ARCH=aarch64 SYSTEM_ZLIB=1 GIT_VERSION=boompi
endef

define LIBRETRO_MUPEN64PLUS_NEXT_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/mupen64plus_next_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/mupen64plus_next_libretro.so
endef

$(eval $(generic-package))
