################################################################################
#
# boompi-ui
#
# NOTE: skeleton - finalized in Phase 4. Builds the `boompi-ui` crate with
# Slint's linuxkms backend (DRM/KMS + libinput, no compositor).
#
################################################################################

BOOMPI_UI_VERSION = 2.0.0-dev
BOOMPI_UI_SITE = $(BR2_EXTERNAL_BOOMPI_PATH)/../rust
BOOMPI_UI_SITE_METHOD = local
BOOMPI_UI_LICENSE = MIT
# Swap the default winit backend for direct KMS rendering on the appliance.
BOOMPI_UI_CARGO_BUILD_OPTS = \
	-p boompi-ui \
	--no-default-features \
	--features slint/backend-linuxkms,slint/renderer-skia

# TODO(Phase 4): confirm feature set (GL vs software renderer per box),
# systemd unit, seatd/libinput deps, fontconfig/fonts.

$(eval $(cargo-package))
