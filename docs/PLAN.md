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
    Handles devices paired at runtime (v1 TODO). Register the agent with
    `DisplayYesNo` capability to drive the on-screen confirm; v1 ran
    bluez-tools `bt-agent -c NoInputNoOutput` (auto-accept everything).
  - *spotify*: librespot (subprocess with event hooks first; embed-as-crate
    is a later option). Art via metadata CDN URLs.
  - *airplay*: shairport-sync spawned as a boompid child (generated config,
    receiver name = speaker name), pipe backend → pw-cat for audio, native
    `org.gnome.ShairportSync` D-Bus interface for metadata/progress/DACP
    transport control. **Decided & shipped**: D-Bus beat the metadata pipe
    (structured properties + free DACP remote). Cover art files are raw
    buffer dumps — trim to the image (EOI/IEND) and decode-validate before
    publishing. Classic AirPlay (AP2 needs nqptp; not in Buildroot 2025.02).
- **artwork**: per-source art acquisition → decode → downscale (~480 px) →
  content-addressed LRU cache in `/data/art`. Track messages carry an
  `artwork_id`; clients fetch `GET /art/{id}`.
  - *Bluetooth art* uses **AVRCP 1.6 Cover Art** (validated end-to-end in
    Phase 0 — real 200×200 JPEGs from an iPhone): `MediaPlayer1.ObexPort`
    (BIP OBEX PSM; iOS uses 4105) + `Track.ImgHandle` → obexd client
    session (`Target=bip-avrcp`, `PSM`) → `org.bluez.obex.Image1`
    `GetThumbnail`/`Get`. Requires BlueZ ≥ ~5.79 with `Experimental = true`
    on `bluetoothd`. Implementation rules (Phase 0): hold the obexd session
    from a **persistent** D-Bus connection (it dies with its owner);
    create it as soon as a player exposes `ObexPort` — `ImgHandle` only
    appears in `Track` while the BIP session is alive; iOS permits exactly
    **one** BIP channel, so `mpris-proxy` (enabled by default on Debian)
    must not run alongside boompid. BlueZ's `tools/mpris-proxy.c` (5.81+)
    is the reference implementation.
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
On stock RPi OS Lite first (fast iteration, no Buildroot yet).
**Pi 3 runbook with exact commands: [`docs/PHASE0-PI3.md`](PHASE0-PI3.md)**
(spike app: `rust/kms-test`).
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

**Renderer** (decided in Phase 0): the Pi UI uses **Skia OpenGL** on
linuxkms (`kms-skia` feature) — verified on the Pi 3: vc4 GLES via
EGL/GBM works, and Skia is the only Slint renderer that rasterizes color
emoji (software renderer is outline/alpha-only per
`i-slint-renderer-software/fonts/vectorfont.rs`; FemtoVG is monochrome —
upstream gaps slint-ui/slint#8646, #5171). The image therefore ships the
mesa GL stack: `libegl` (glvnd + mesa vendor), `libgles`,
mesa vc4 DRI driver. The software-renderer build (`kms` feature,
monochrome Noto Emoji) remains as a fallback variant.

**Fonts** (recipe validated in Phase 0): UI chrome uses drawn vector icons
(`icons.slint`, zero font dependency). Regular text is **Geist Sans**
(vendored at `rust/boompi-ui/ui/fonts/`, OFL license alongside), embedded
into the binary via Slint's font import + `default-font-family` — so text
renders identically on the laptop, dev Pi, and appliance with no OS font
packages. For emoji in arbitrary user content (speaker name
`George's 🔊`, track/device names) the image additionally ships:
1. Noto **Color** Emoji (CBDT)
2. `/etc/fonts/local.conf` aliasing the `emoji` generic → `Noto Color Emoji`

Emoji **cannot** be bundled into the binary like Geist: Slint's imported
fonts are only used when named explicitly (`font-family` is
single-valued), while emoji in mixed text resolve through fontique's
*fallback* query, which is OS-backed (fontconfig on Linux, CoreText on
macOS). Consequence: laptop dev shows Apple Color Emoji — a known,
accepted, dev-only discrepancy; the appliance is deterministic because
its fontconfig is ours. Possible upstream Slint PR: expose fontique's
`set_generic_families(GenericFamily::Emoji, …)` for imported fonts
(Slint already uses that API internally for its bundled Inter).

### Phase 5 — Settings & first-boot setup
One HTTP config surface (boompid, :80 on the appliance / :8080 dev) serves
both onboarding and day-2 settings; the panel gets touch-appropriate
controls plus a QR code pointing browsers at the web UI for keyboard-heavy
input. Build order (1–4 are dev-Pi-testable; 5–6 need image loops):
1. ✅ Config persistence (atomic TOML save) + `/api/state` + `/api/settings`
   + embedded Vite/React/Tailwind SPA (`web/`, dist committed, rust-embed)
   + speaker rename end-to-end (BT `Adapter1.Alias` in place; AirPlay +
   Spotify restart discovery via a config-generation watch channel).
2. BT device management + real pairing agent (`Agent1` `DisplayYesNo`,
   list/disconnect/remove) replacing the auto-accept bt-agent.
3. Theme (light palette) end-to-end + clock/timezone via
   `org.freedesktop.timedate1` (tzdata + timesyncd in the image).
4. Panel settings screen rework + QR code to the web UI.
5. Wi-Fi client + AP mode ("Boompi-XXXX") + captive portal —
   **NetworkManager on both boxes** (RPi OS ships it; Buildroot packages
   it; `shared` connections give AP+DHCP) so dev and appliance share one
   D-Bus code path.
6. OOBE wizard on the `SetupState` machine: name required, Wi-Fi optional.

### Phase 6 — Boot polish + release
Silent boot (`quiet loglevel=0`, `disable_splash=1`, no cursor/rainbow),
boot-time tuning to < 10 s, service hardening (`Restart=always`, watchdog),
CI image builds with ccache. Tag v2.0.

## Risks

| Risk | Mitigation |
|---|---|
| ~~Slint KMS on HyperPixel DPI~~ | ✔ **Resolved in Phase 0**: software renderer smooth at 800×480, touch perfect. Requires `SLINT_KMS_ROTATION=270` in the UI environment (panel-orientation hint not auto-applied by Slint). |
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
- [x] v1 pairing mechanism confirmed: `bt-agent.service` ran
      `/usr/bin/bt-agent -c NoInputNoOutput` (bluez-tools) — auto-accepts
      all pairing. Replaced by boompid's own `Agent1` (`DisplayYesNo`) in
      Phase 3. (Deployed kiosk.sh/units drifted from git; the v1 image dump
      is the authoritative reference.)
- [x] AirPlay 2 vs classic: **classic** (shairport-sync 3.3.9 in Buildroot
      2025.02; no nqptp package). AP2 later via custom nqptp + shairport 4.x
      packages in our BR2_EXTERNAL if wanted. Coded against the 3.3.9 D-Bus
      property set; 4.x extras (ClientName) used opportunistically.
- [ ] Full source-manager arbitration (pause-others, audio-flow-based claims
      via MediaTransport1.State). Interim shipped with AirPlay: sources only
      write track/source state while they own the display, and async art
      publishes are origin-gated (a late BT BIP thumbnail must not stomp an
      AirPlay cover). Note: iOS mirrors now-playing over AVRCP while
      AirPlaying — the BT provider must never treat that chatter as a claim.
- [ ] Default state of online-art fallback (suggest: off until Wi-Fi configured)
- [ ] Low-battery safeguard (new, motivated by Phase 0: the deeply
      discharged pack browned out the Pi and corrupted the SD mid-boot).
      v2 should surface a low-battery warning in the UI and consider a
      safe-shutdown voltage threshold via the INA260 — v1 had neither.
- [x] Pi 3 display rotation under Slint: `SLINT_KMS_ROTATION=270` env var
      (DRM panel-orientation hint is not auto-applied). Touch works
      unmodified alongside the existing DT touch transforms.
