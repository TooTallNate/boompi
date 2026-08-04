# Boompi Buildroot external tree

`BR2_EXTERNAL` tree for building the Boompi appliance SD card images.

**CI**: `.github/workflows/image.yml` builds the Pi 3 image on every commit
and uploads `sdcard.img.xz` as an artifact. The system layer comes from
Buildroot (pinned release, Bootlin external toolchain, dl+ccache cached);
the Rust binaries are cross-built with cargo-zigbuild against the build's
own staging sysroot and injected via `board/boompi/rootfs-overlay-ci/`
(gitignored, populated by CI — the `package/boompid` etc. stubs remain for
a future in-Buildroot build).

The rootfs overlay encodes the Phase 0 findings: root system services
(pipewire/wireplumber/boompid/boompi-ui/bt-agent) sharing
`PIPEWIRE_RUNTIME_DIR=/run/pipewire`, WirePlumber seat-monitoring disabled
+ ALSA suspend off, BlueZ `Experimental = true` + `DiscoverableTimeout=0`,
HyperPixel KMS overlay + `SLINT_KMS_ROTATION=270`, `disable-bt` (USB dongle
only), and no pre-seeded rfkill blocks. Dev login: `root` / `boompi`.

## Intended layout

```
external.desc / external.mk / Config.in    BR2_EXTERNAL plumbing (present)
package/
  boompid/                                 backend daemon (cargo package, present as stub)
  boompi-ui/                               Slint UI (cargo package, present as stub)
  librespot/                               only if not available upstream
configs/
  boompi_pi3_defconfig                     Pi 3 box (HyperPixel 4.0 DPI, USB audio)
  boompi_pi4_defconfig                     Pi 4 box (HDMI + USB touch, I2S DAC HAT)
board/boompi/
  rootfs-overlay/                          systemd units, PipeWire/WirePlumber/BlueZ config
  genimage.cfg                             boot (FAT) + rootfs (squashfs RO) + /data (ext4)
  config-pi3.txt / config-pi4.txt          dtoverlays (vc4-kms-dpi-hyperpixel4, DAC, i2c-gpio), silent boot
  post-build.sh / post-image.sh
```

## Key decisions (see docs/PLAN.md)

- systemd init; BlueZ pinned **recent** (>= ~5.79) with `--experimental` on
  `bluetoothd` and `obexd` for AVRCP cover art; PipeWire + WirePlumber with
  node suspend disabled (crackle fix); shairport-sync (+ nqptp if
  available) and librespot as services.
- Read-only squashfs rootfs; `/data` writable partition for
  `boompi.toml`, `/var/lib/bluetooth`, Spotify cache, artwork cache.
- Per-box defconfigs; hardware facts seeded into `/data/boompi.toml`.
- Build with ccache; SSH stays enabled for the `make deploy` dev loop.

## Usage (once Phase 4 lands)

```
make image-pi3   # from the repo root
make image-pi4
```
