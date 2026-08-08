# Changesets

The appliance is versioned as a single package (`boompi`, in `web/` -
the version there is the version of the whole OS image, not just the
web UI). To record a change for the next release:

    pnpm changeset

Pick a bump level and describe the change. CI opens/updates a
"Version Packages" PR collecting pending changesets; merging that PR
bumps the version, updates `web/CHANGELOG.md`, and publishes a GitHub
Release (`vX.Y.Z`) with the SD card images and OTA update assets from
the image build of that commit.
