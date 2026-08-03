# Boompi Buildroot external tree

`BR2_EXTERNAL` tree for building the Boompi appliance SD card images.
Fleshed out in **Phase 4** of `docs/PLAN.md`; the structure here is the
skeleton the phases build into.

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
