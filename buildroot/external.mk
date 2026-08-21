include $(sort $(wildcard $(BR2_EXTERNAL_BOOMPI_PATH)/package/*/*.mk))

# Replace the kernel's built-in penguin with the generated Boompi artwork.
# This runs after Linux is extracted and patched, before its build starts.
define BOOMPI_LINUX_INSTALL_LOGO
	cp $(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/kernel-logo.ppm \
		$(@D)/drivers/video/logo/logo_linux_clut224.ppm
endef
LINUX_POST_PATCH_HOOKS += BOOMPI_LINUX_INSTALL_LOGO
