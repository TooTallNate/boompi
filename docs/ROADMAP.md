# Boompi Roadmap

What comes after the first stable release. [`PLAN.md`](PLAN.md) is the
historical implementation plan (Phases 0-6, all shipped);
[`UPDATES.md`](UPDATES.md) documents the A/B update system. This file
is the forward-looking list, roughly ordered by how much each item
contributes to "the quintessential Raspberry Pi boombox".

## Before / alongside the first stable release

- ~~**Low-battery safeguard.**~~ Shipped: warning banner on the panel
  (SoC threshold with hysteresis, clears on charge) and an automatic
  orderly poweroff when SoC or sustained pack voltage crosses the
  configured floor - the sustain window exists because bass transients
  sag the pack for seconds at a time. Thresholds and an opt-out live
  in `[battery]` config. Poweroff into the latching switch leaves the
  amp + LED drawing, but the Pi is down cleanly and the SD card safe.
- ~~**Boot-time measurement.**~~ Measured (2026-08, alongside the
  boot splash work): boompi-ui starts at ~8 s monotonic on the Pi 4,
  similar on the Pi 3 - the < 10 s goal is met. The formerly dark
  window is now a branded splash: the kernel draws
  `branding/logo.png` (generated + validated at build time, centered,
  rotation-aware) at ~2 s on the Pi 3; the Pi 4's HDMI panel shows it
  at ~6 s only because the panel itself takes ~4 s to re-lock after
  the firmware→vc4 handoff - kernel-side it is identical (~1.6 s).
  Undervoltage spam is off the panel (`loglevel=2`); everything still
  lands in the journal.

## Tier 1 - earns the title

- **Pop-free amplifier power sequencing.** Both boxes pop loudly at
  power-on and crackle through boot; once the audio stack is up the
  noise is gone. Fix it at the source: a configurable GPIO drives a
  MOSFET (or the amp's enable/mute pin) so the amplifier is only
  powered after the audio stack has initialized. Per-box pin surveys
  and wiring are worked out in
  [`notes/AMP-ENABLE.md`](notes/AMP-ENABLE.md) (Pi 4: BCM 22; Pi 3:
  MCP23008 on i2c-11 - no free header GPIOs); the boompid `[amp]`
  config/GPIO layer and the hardware mods are unbuilt. Design notes:
  - The pin must default OFF from the firmware onward, not just from
    boompid - the pops happen long before userspace. `gpio=N=op,dl`
    in config.txt holds it low from the bootloader; boompid raises it
    when PipeWire is up (and drops it again on shutdown/reboot).
  - Config: `[amp] gpio`, `active_high`, and a mode - `boot` (on once
    audio is initialized, the simple default) or `playback` (on only
    while something is actually playing, with a configurable linger
    time so pause/track gaps don't clatter the rail).
  - Off by default: this needs a hardware modification (high-side
    MOSFET or an amp enable pin wired to the header), so it's an
    opt-in per-box config, not image behavior.
- **Multi-room / speaker grouping.** Two boompis exist; playing them
  together is the feature nothing else in the Pi space does well.
  Options to evaluate: snapcast-style native sync between boompis vs
  leaning on AirPlay 2's existing multi-speaker targeting (which
  already works from an iPhone today) for the cross-device case.
- **Internet radio + local playback.** A boombox that requires a
  phone is half a boombox. Station presets (Radio Browser API) on the
  panel + play-from-USB-stick makes it self-sufficient - campsite
  mode. The source-provider abstraction is already shaped for a
  fourth source.
- **Source arbitration, finished.** The plan's open item: new source
  claims should pause the others (MediaTransport1-state-driven), so
  AirPlaying over an active Bluetooth stream doesn't mix both. The
  shipped interim only gates the display and artwork.
- **EQ / loudness.** PipeWire filter-chain parametric EQ with panel
  presets ("Outdoor", "Bass boost", "Night"). Cheap to build,
  dramatic to hear, very boombox.
- ~~**Battery intelligence.**~~ Shipped: self-calibrating SoC
  estimator (learned full-charge voltage per box, coulomb counting
  anchored at full, learned pack capacity) and time-remaining while
  discharging, surfaced on the panel, web UI, and Home Assistant.
  Still open: charge/discharge history in the battery panel beyond
  the live 3-minute chart.

## Tier 2 - appliance polish

- **Web now-playing remote.** Partially superseded: the hosted BLE
  remote (`web-remote/`, any Web-Bluetooth browser, no LAN required)
  and the iOS app shipped as the phone remotes. Still open in the
  narrow sense: the LAN web UI (settings surface) has no now-playing
  page of its own.
- ~~**Idle / ambient mode + screensavers.**~~ Shipped: screensavers
  (clock, visualizer, drifting art, Matrix rain) with a battery glyph
  so a shelf glance answers "does it need charging?" without waking
  the panel.
- **Physical controls.** GPIO rotary encoder for volume, play/pause
  button, config-mapped. Tactility makes it hardware, not a computer
  in a box. (Pairs naturally with the amp-enable GPIO work: both grow
  a small GPIO layer in boompid.)
- **Auto-update toggle.** Default off, applies updates at idle (no
  active source) only. Closes the original plan's last update item.
- **Wi-Fi reachability watchdog.** The 2026-08 incident: nates sat
  "connected" (NM activated, panel agreed) but unpingable for hours -
  a silent brcmfmac wedge with zero log entries at any layer. Root
  cause (powersave) is disabled via NM conf.d now, but the class of
  failure remains: boompid should probe the gateway while NM claims
  connectivity and bounce the connection after sustained silence,
  surfacing the truth on the panel instead of NM's association state.
  (boompid already owns the NM D-Bus session; the bounce is one
  ActivateConnection call.)
- ~~**Home Assistant / MQTT discovery.**~~ Shipped (`boompid`'s mqtt
  module): play state, volume, source, battery, CPU temperature as
  discovered entities.

- **AirPlay classic-only toggle: bench-verify.** Shipped as an
  experimental settings toggle (shairport patch 0003: classic record
  set, no _airplay._tcp service, so senders negotiate classic AirPlay
  whose DACP remote control works). Needs live verification with an
  iPhone: does modern iOS still speak classic to a vs=105.1 receiver,
  does DACP come up (panel buttons light via the existing
  RemoteControl.Available watch), and does audio behave. Re-evaluate
  the whole area when Buildroot ships shairport-sync 5.x (upstream
  restored classic remote control there, and metadata is currently
  reported broken in 5.1 - issue #2239).
- **VC4 GPU hang: verify the kernel bump helps.** The reset-storm
  watchdog hard-reboots out of wedges; the kernel pin moved from
  6.6.28 (2024-04) to rpi-6.6.y head (2025-02) picking up ~10 months
  of vc4/HVS fixes in exactly the hang's code path (HVS channel stop
  logic, atomic_flush dev_enter/exit matching, AXI panic modes).
  Watch hang frequency on the bench; if wedges persist, lighter GPU
  load when idle is the next lever (ties into ambient mode).
  NB: the 2026-08 boot-splash work made vc4 (+ DDC i2c, ili9806e
  panel, fbcon) built-in rather than modular - any hang-frequency
  observations from before then need a fresh baseline.

## Tier 3 - keeper of the fleet

- **Diagnostics in settings.** Partially shipped: CPU temperature +
  throttle state surface on the panel, web hardware page, and Home
  Assistant. Still open: Wi-Fi RSSI and a "download support bundle"
  button (journal tail + redacted config).
- **Settings backup/export** for painless reflashes.
- **Hardware watchdog** (`RuntimeWatchdogSec`) for hung-kernel
  recovery - mind the interplay with trial boots (arm only after
  boompi-boot-commit settles).
- **Optional web-UI authentication.** The settings surface is
  deliberately open on the LAN today (home posture); an optional PIN
  gate would suit less-trusted networks.
