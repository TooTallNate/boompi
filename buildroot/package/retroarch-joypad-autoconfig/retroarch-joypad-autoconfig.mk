################################################################################
#
# retroarch-joypad-autoconfig
#
################################################################################

RETROARCH_JOYPAD_AUTOCONFIG_VERSION = 7a44e4aa65f1c5380867e9088d8764d585341aa4
RETROARCH_JOYPAD_AUTOCONFIG_SITE = $(call github,libretro,retroarch-joypad-autoconfig,$(RETROARCH_JOYPAD_AUTOCONFIG_VERSION))
RETROARCH_JOYPAD_AUTOCONFIG_LICENSE = MIT

# Only the udev driver's profiles: that is the input driver the image
# builds RetroArch with (no SDL/X11), and the full pack is ~4x the
# size for drivers that cannot exist here.
define RETROARCH_JOYPAD_AUTOCONFIG_INSTALL_TARGET_CMDS
	$(INSTALL) -d $(TARGET_DIR)/usr/share/retroarch/autoconfig/udev
	$(INSTALL) -m 0644 $(@D)/udev/*.cfg \
		$(TARGET_DIR)/usr/share/retroarch/autoconfig/udev/
endef

$(eval $(generic-package))
