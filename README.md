# Boompi v2

Software stack for two custom-built Raspberry Pi boomboxes, rebuilt as a
native appliance:

- **`boompid`** - Rust backend daemon: Bluetooth (BlueZ), Spotify Connect
  (librespot), AirPlay (shairport-sync), PipeWire volume, INA260 battery
  telemetry, FFT visualizer, album art. Serves a WebSocket/HTTP API.
- **`boompi-ui`** - Slint touchscreen UI: renders directly on DRM/KMS on the
  boombox, or as a desktop app on a laptop for development.
- **`buildroot/`** - flashable, pre-configured SD card images (silent boot,
  read-only rootfs, first-boot setup).

The v1 stack (Node.js + Next.js + Chromium kiosk) lives on the `main` branch.
The full design and phased plan is in [`docs/PLAN.md`](docs/PLAN.md).

## Development

No hardware needed for UI work:

```sh
make sim   # terminal 1: backend with simulated sources/battery/visualizer
make ui    # terminal 2: the UI, connected to the local sim
```

Against a real boombox:

```sh
make ui BACKEND=ws://boombox.local:3001/ws
```

Other targets: `make check`, `make test`, `make fmt`, `make clippy`.
