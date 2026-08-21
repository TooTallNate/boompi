include $(sort $(wildcard $(BR2_EXTERNAL_BOOMPI_PATH)/package/*/*.mk))

# Replace the kernel's built-in penguin with the Boompi boot logo,
# generated from branding/logo.png (the single source of truth) and
# validated for the CLUT224 constraints (see gen-kernel-logo.py).
# Runs after Linux is extracted and patched, before its build starts.
define BOOMPI_LINUX_INSTALL_LOGO
	python3 $(BR2_EXTERNAL_BOOMPI_PATH)/board/boompi/gen-kernel-logo.py \
		$(BR2_EXTERNAL_BOOMPI_PATH)/../branding/logo.png \
		$(@D)/drivers/video/logo/logo_linux_clut224.ppm
endef
LINUX_POST_PATCH_HOOKS += BOOMPI_LINUX_INSTALL_LOGO
