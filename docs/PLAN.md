# Boompi v2 — Implementation Plan

Boompi v2 is a ground-up rewrite of the boombox software stack as a native
appliance: a Rust backend daemon, a Slint touchscreen UI, and flashable
Buildroot SD card images. The v1 stack (Node.js + Next.js + Chromium kiosk on
Raspberry Pi OS desktop) lives on the `main` branch and serves as the
behavioral reference/spec.

## Goals

- **Native stack**: Rust daemon (`boompid`) + Slint UI (`boompi-ui`).
  No browser, no desktop environment, no Node.js.
- **Appliance-grade images**: flashable, pre-configured Buildroot SD images.
  Silent boot, **< 10 s power → UI** target, read-only rootfs.
- **Feature parity with v1**: Bluetooth A2DP sink + AVRCP (metadata,
  transport controls, absolute volume), system volume, INA260 battery
  telemetry + diagnostics chart, audio spectrum visualizer, clock,
  "connect your device" screen.
- **New features**:
  - Pairing UI (discoverable mode + on-screen confirm; v1 required
    pre-paired devices).
  - Spotify Connect via librespot.
  - AirPlay via shairport-sync (+ nqptp for AirPlay 2 if packaging allows).
  - **Album art** on all sources (see below).
  - First-boot setup: touchscreen wizard + phone via AP-mode captive portal.
  - Settings screen (incl. toggle for online album-art fallback).
- **Preserve the laptop dev loop**: the UI always talks to the backend over
  WebSocket/IP, so `cargo run` on a laptop against a real boombox works
  exactly like the on-device UI. Additionally `boompid --sim` provides a
  simulated backend (fake track/battery/visualizer data) for UI development
  with no hardware at all.

## Decisions (with rationale)

| Decision | Choice | Rationale |
|---|---|---|
| Backend language | Rust | zbus for BlueZ D-Bus, in-process FFT replaces the v1 custom cava fork, librespot is Rust, Slint synergy |
| Frontend | Slint | `linuxkms` backend renders directly on DRM/KMS + libinput touch (no X/Wayland/compositor); cross-platform so the same app runs on a laptop (winit backend) for development |
| Audio plumbing | PipeWire + WirePlumber | Mixes the three sources (BT / Spotify / AirPlay); monitor tap feeds the visualizer; disabling node suspend properly fixes the DAC crackle (v1 used an `ffplay /dev/zero` hack) |
| OS / image | Buildroot (BR2_EXTERNAL tree in-repo) | Minimal, fast boot, full control of versions (notably BlueZ, see album art); two defconfigs (pi3/pi4) |
| Init | systemd | Sanest PipeWire/BlueZ/D-Bus integration; worth the size cost |
| Rootfs | squashfs read-only + writable `/data` partition | Appliance robustness; `/data` holds config, BT pairings (`/var/lib/bluetooth`), Spotify cache, artwork cache |
| Protocol | Redesigned v2 (see below) | Clean typed messages; no wire compat with v1 needed since the Slint UI replaces the web UI |
| Processes | UI and daemon are separate processes, localhost WS on-device | Dev/prod parity, crash isolation, independent restarts |

## Architecture

```
BlueZ ──D-Bus(zbus)──┐
librespot ───────────┤                      ┌─ boompi-ui (Slint: linuxkms on Pi / winit on laptop)
shairport-sync ──────┴─→ boompid (Rust) ─WS─┤
                          │  axum HTTP ─────┴─ phone captive-portal setup page, GET /art/{id}
        PipeWire ←────────┤
          │               ├─ source manager (arbitration, metadata normalization)
          ▼               ├─ artwork pipeline (per-source providers → resize → LRU cache)
     ALSA sink            ├─ INA260 via embedded-hal/i2cdev (bus/addr from config, optional)
 (DAC HAT / USB audio)    └─ FFT visualizer (PipeWire monitor capture → bars)
```

### boompid modules

- **server**: axum HTTP + WebSocket. Serves protocol v2, `/art/{id}`, and the
  captive-portal setup page during first boot.
- **sources**: one provider per audio source (`bluetooth`, `spotify`,
  `airplay`), each emitting normalized events (connected/disconnected, track,
  playback status, position, volume, artwork). A **source manager**
  arbitrates: last-active-wins, pauses others.
  - *bluetooth*: zbus proxies for `org.bluez.Device1`, `MediaTransport1`
    (absolute volume 0–127), `MediaPlayer1` (transport + `Track` metadata),
    `Adapter1` (discoverable), `Agent1`/`AgentManager1` (pairing flow).
    Handles devices paired at runtime (v1 TODO).
  - *spotify*: librespot (subprocess with event hooks first; embed-as-crate
    is a later option). Art via metadata CDN URLs.
  - *airplay*: shairport-sync metadata (D-Bus/MPRIS or metadata pipe,
    whichever proves more robust). Art arrives natively (PICT).
- **artwork**: per-source art acquisition → decode → downscale (~480 px) →
  content-addressed LRU cache in `/data/art`. Track messages carry an
  `artwork_id`; clients fetch `GET /art/{id}`.
  - *Bluetooth art* uses **AVRCP 1.6 Cover Art** (new in recent BlueZ,
    `[experimental]`): `MediaPlayer1.ObexPort` (BIP OBEX PSM) +
    `Track.ImgHandle` → obexd client session (`org.bluez.obex.Image1`)
    → `GetThumbnail`/`Get`. Requires BlueZ ≥ ~5.79 with the experimental
    flag on `bluetoothd`/`obexd` (we control this in Buildroot; Pi OS
    Bookworm's 5.66 is too old — build from source during dev phases).
  - *Online fallback* (*user-toggleable in Settings, default TBD*): when a
    source provides no art, look up artist+album via iTunes Search /
    MusicBrainz Cover Art Archive over Wi-Fi; cache results.
- **audio**: PipeWire default-sink volume control, synced bidirectionally
  with AVRCP absolute volume; node-suspend disabled via config (crackle fix).
- **visualizer**: PipeWire monitor capture → FFT (`realfft`) → N bars with
  cava-style smoothing/falloff → binary WS frames at ~30 fps.
- **battery**: INA260 polling (30 s slow / 1 s fast-poll on request), linear
  percentage between configured min/max voltages (v1: 18.0–24.98 V).
- **setup**: first-boot state machine. Unconfigured → setup mode: Slint
  wizard (speaker name, output check) + `Boompi-Setup` Wi-Fi AP
  (hostapd/dnsmasq) with captive portal for Wi-Fi credentials from a phone.
  Persists to `/data`, sets BT alias + hostname.
- **config**: TOML at `/data/boompi.toml` (device-specific hardware config
  seeded by the image: I2C bus/address for INA260 or absence, audio sink
  hints, display rotation, etc.) + user settings.
- **sim**: `boompid --sim` runs everywhere (incl. macOS) with fake sources,
  battery, and a sine-sweep visualizer for hardware-free UI development.

### boompi-ui screens

1. **Connect** — "To play music, connect to: {name}" (+ pairing mode button).
2. **Now Playing** — album art, title/artist/album (marquee on overflow),
   position bar with client-side interpolation, transport controls, volume
   slider, visualizer bars in the background.
3. **Battery panel** — live voltage/current chart (custom Slint component),
   volts/amps/watts/percent readouts; enables backend fast-poll while open.
4. **Settings** — speaker name, online-art fallback toggle, pairing,
   Wi-Fi info, about/version.
5. **Setup wizard** — first boot only.
6. **Footer** (persistent) — clock, connected device, volume, BT, battery.

## Protocol v2

WebSocket (default port **3001**). Text frames are JSON envelopes
`{"type": ..., ...}`; binary frames are `[u8 tag][payload]`.

Server → client:
- `hello { proto_version, name, model, version, uptime_secs }` then a full
  `state` snapshot, then deltas:
- `source { active: "bluetooth"|"spotify"|"airplay"|null, device_name }`
- `track { title, artist, album, duration_ms, position_ms, status, artwork_id?, updated_at }`
- `volume { level }` (0.0–1.0)
- `battery { voltage, current, power, percentage, charging, ts }`
- `pairing { state: "idle"|"discoverable"|"confirm", device_name?, passkey? }`
- `setup { ... }` (first-boot flow)
- `settings { online_art_fallback, ... }`
- binary `0x01`: visualizer bars (N × u16 LE)

Client → server:
- `play` / `pause` / `next` / `previous`
- `set_volume { level }`
- `battery_fast_poll { enabled }`
- `pairing { action: "enable"|"cancel"|"confirm"|"reject" }`
- `set_settings { ... }`
- `setup { ... }`

HTTP: `GET /art/{id}` (image bytes, immutable cache), `GET /healthz`,
setup/captive-portal routes in setup mode.

Canonical Rust types live in the `boompi-proto` crate, shared by daemon
and UI.

## Hardware matrix

| | Pi 4 box | Pi 3 box |
|---|---|---|
| SoC | BCM2711 (aarch64) | BCM2837 (aarch64) |
| Display | HDMI monitor (KMS trivial) | Pimoroni HyperPixel 4.0 800×480, 18-bit DPI (`dpi_18bit_cpadhi_gpio0`), `dtoverlay=vc4-kms-dpi-hyperpixel4` + `dtparam=rotate=270,touchscreen-swapped-x-y,touchscreen-inverted-x` ✔ confirmed from v1 card |
| Touch | USB HID (from monitor) — libinput, no extra work | Goodix GT911 on overlay-created `i2c-gpio` bus (GPIO 10=SDA / 11=SCL, ~100 kHz) ✔ |
| Audio | I2S DAC HAT — **model/overlay TBD** (read from Pi 4 v1 image dump) | USB audio |
| INA260 | bus 1 @ 0x40 (confirm from Pi 4 card) | ✔ On the overlay's `i2c-gpio` bus (GPIO 10/11) = `/dev/i2c-11` (dynamic: DTB aliases reserve 0–10). v1's deployed `kiosk.sh` ran `ln -sf /dev/i2c-11 /dev/i2c-1` at boot so its hardcoded bus 1 worked. v2: config takes the real bus, and boompid should optionally locate the adapter **by name** (`/sys/class/i2c-adapter/*/name`) since i2c-gpio numbering is dynamic. |
| v1 OS | (dump pending) | Raspberry Pi OS Bullseye 2022-04-04 (pi-gen stage4), kernel 5.15 ✔ |

> The Pi 3's KMS status is a major Phase 0 de-risk: the panel already runs
> the modern `vc4-kms-dpi-hyperpixel4` overlay that Slint's `linuxkms`
> backend requires. Display rotation (270°) + touch transforms must be
> carried into the v2 config.

## Repository layout

```
docs/PLAN.md            this file
rust/                   cargo workspace
  boompi-proto/         protocol v2 types (serde), shared
  boompid/              backend daemon
  boompi-ui/            Slint UI
  ina260/               INA260 driver (embedded-hal), port of v1 TS driver
buildroot/              BR2_EXTERNAL tree
  external.desc / external.mk / Config.in
  configs/boompi_pi3_defconfig, boompi_pi4_defconfig
  package/boompid/, package/boompi-ui/, package/librespot/ (if not upstream)
  board/boompi/         rootfs overlay (systemd units, PipeWire/BlueZ/WirePlumber
                        config), genimage.cfg, config-{pi3,pi4}.txt, post-*.sh
Makefile                dev loop: check/build/deploy-over-ssh/image targets
```

## Phases

### Phase 0 — Validation spikes (gate for everything else)
On stock RPi OS Lite first (fast iteration, no Buildroot yet):
1. **Slint on KMS**: hello-world with `linuxkms` backend on Pi 3 + HyperPixel
   (`vc4-kms-dpi-hyperpixel4`) and Pi 4 + HDMI, touch included.
   Fallback: Slint software renderer.
2. **Headless BT audio**: BlueZ + PipeWire + WirePlumber A2DP sink; verify
   `MediaPlayer1` metadata, absolute volume, monitor capture.
3. **AVRCP cover art**: build recent BlueZ from source, run with
   `--experimental`, verify `ObexPort`/`ImgHandle` appear with the actual
   phones and pull an image via `busctl` before writing Rust for it.
4. **Buildroot minimal boot** on both Pis (serial console + network).
5. Resolve remaining hardware TBDs from the v1 image dump.

### Phase 1 — boompid core (dev on RPi OS Lite)
Workspace scaffolding (done in initial commit), then: config store, WS server
+ protocol v2, BlueZ source via zbus, PipeWire volume, INA260 polling, FFT
visualizer, `--sim` mode, cross-compile + `make deploy` loop.

### Phase 2 — Slint UI to parity
Connect / Now Playing / battery panel / footer; runs on laptop against a real
box and against `--sim`.

### Phase 3 — Multi-source, pairing, artwork
librespot + shairport-sync providers, source arbitration, pairing agent +
UI flow, artwork pipeline for all three sources + online fallback + Settings
screen with the fallback toggle.

### Phase 4 — Buildroot appliance image
BR2_EXTERNAL tree fleshed out: both defconfigs, custom packages, rootfs
overlay, RO rootfs + `/data`, genimage SD layout, Wi-Fi/BT firmware, recent
BlueZ pinned with experimental flags, obexd service wiring, SSH for dev.

### Phase 5 — First-boot setup
Setup state machine, Slint wizard, AP mode + captive portal, persistence.

### Phase 6 — Boot polish + release
Silent boot (`quiet loglevel=0`, `disable_splash=1`, no cursor/rainbow),
boot-time tuning to < 10 s, service hardening (`Restart=always`, watchdog),
CI image builds with ccache. Tag v2.0.

## Risks

| Risk | Mitigation |
|---|---|
| Slint KMS on HyperPixel DPI | Phase 0 gate; software-renderer fallback |
| AVRCP cover art is `[experimental]` in BlueZ; sender support varies (iOS good, Android varies) | Phase 0 spike with real phones; online fallback covers gaps |
| AirPlay 2 (nqptp) availability in Buildroot | Verify in Phase 0; fall back to AirPlay classic |
| librespot API/version churn | Subprocess integration keeps it swappable |
| Buildroot iteration is slow | App dev on RPi OS Lite through Phase 3; ccache; `make deploy` pushes binaries without image rebuilds |
| obexd expects a session bus | Run under the boompi service user with a dedicated bus (systemd) |

## Open items

- [ ] DAC HAT model on Pi 4 (→ dtoverlay) — from Pi 4 v1 image dump
- [x] INA260 I2C bus on Pi 3: `/dev/i2c-11` (overlay's i2c-gpio; v1 symlinked
      it to `/dev/i2c-1` in kiosk.sh). v2: seed `battery.i2c_bus = 11` for the
      Pi 3 image + implement find-adapter-by-name in Phase 1 for robustness.
- [x] HyperPixel touch variant: touch (Goodix GT911, per KMS overlay)
- [x] v1 pairing mechanism: a `bt-agent.service` (bluez-tools) ran a
      NoInputNoOutput-style agent on the box — replaced by boompid's own
      `Agent1` implementation in Phase 3. (Deployed kiosk.sh/units drifted
      from git; the v1 image dump is the authoritative reference.)
- [ ] AirPlay 2 vs classic decision after Buildroot packaging check
- [ ] Default state of online-art fallback (suggest: off until Wi-Fi configured)
- [ ] v2 must handle display rotation on the Pi 3 (panel is `rotate=270`):
      verify Slint linuxkms rotation handling (e.g. panel orientation from
      DRM vs. `SLINT_KMS_ROTATION`-style config) during the Phase 0 spike
