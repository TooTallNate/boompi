################################################################################
#
# libretro-mgba
#
################################################################################

LIBRETRO_MGBA_VERSION = e31759b24e7a4e3899285ff720d7b573ac328ae7
LIBRETRO_MGBA_SITE = $(call github,libretro,mgba,$(LIBRETRO_MGBA_VERSION))
LIBRETRO_MGBA_LICENSE = MPL-2.0
LIBRETRO_MGBA_DEPENDENCIES = zlib

# mgba dropped its Makefile.libretro long ago: the libretro core is a
# CMake target. Everything optional is off - the core needs none of
# the frontends, and every USE_* left auto drags in a dependency.
LIBRETRO_MGBA_CONF_OPTS = \
	-DBUILD_LIBRETRO=ON \
	-DBUILD_QT=OFF \
	-DBUILD_SDL=OFF \
	-DBUILD_SUITE=OFF \
	-DUSE_DEBUGGERS=OFF \
	-DUSE_DISCORD_RPC=OFF \
	-DUSE_EDITLINE=OFF \
	-DUSE_ELF=OFF \
	-DUSE_EPOXY=OFF \
	-DUSE_FFMPEG=OFF \
	-DUSE_GDB_STUB=OFF \
	-DUSE_LIBZIP=OFF \
	-DUSE_LZMA=OFF \
	-DUSE_MINIZIP=OFF \
	-DUSE_PNG=OFF \
	-DUSE_SQLITE3=OFF \
	-DUSE_ZLIB=ON

# The libretro target has no install rule worth having; take the .so.
define LIBRETRO_MGBA_INSTALL_TARGET_CMDS
	$(INSTALL) -D -m 0644 $(@D)/mgba_libretro.so \
		$(TARGET_DIR)/usr/lib/libretro/mgba_libretro.so
endef

$(eval $(cmake-package))
