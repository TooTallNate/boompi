################################################################################
#
# boompid
#
# NOTE: skeleton — finalized in Phase 4. Builds the `boompid` crate out of
# the workspace in ../rust using Buildroot's cargo infrastructure.
#
################################################################################

BOOMPID_VERSION = 2.0.0-dev
BOOMPID_SITE = $(BR2_EXTERNAL_BOOMPI_PATH)/../rust
BOOMPID_SITE_METHOD = local
BOOMPID_LICENSE = MIT
# Build only the daemon crate out of the workspace.
BOOMPID_CARGO_BUILD_OPTS = -p boompid

# TODO(Phase 4): systemd unit installation, /data/boompi.toml seeding,
# runtime deps (bluez5-utils, pipewire, wireplumber).

$(eval $(cargo-package))
