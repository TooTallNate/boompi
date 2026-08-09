# Boompi Roadmap

What comes after the first stable release. [`PLAN.md`](PLAN.md) is the
historical implementation plan (Phases 0-6, all shipped);
[`UPDATES.md`](UPDATES.md) documents the A/B update system. This file
is the forward-looking list, roughly ordered by how much each item
contributes to "the quintessential Raspberry Pi boombox".

## Before / alongside the first stable release

- **Low-battery safeguard.** The one plan item promoted to
  release-blocker: a deeply discharged pack browned out a Pi mid-boot
  and corrupted its SD card during Phase 0, and today the box measures
  its battery without ever acting on it. Config-driven warning
  threshold (panel banner) + safe-shutdown voltage floor with
  hysteresis via the INA260. Bench-testable when the rebuilt battery
  arrives.
- **Boot-time measurement.** The plan targets < 10 s power → UI;
  quiet boot shipped but the number was never measured. Stopwatch it,
  `systemd-analyze blame` the stragglers, tune or honestly amend the
  goal.

## Tier 1 - earns the title

- **Pop-free amplifier power sequencing.** Both boxes pop loudly at
  power-on and crackle through boot; once the audio stack is up the
  noise is gone. Fix it at the source: a configurable GPIO drives a
  MOSFET (or the amp's enable/mute pin) so the amplifier is only
  powered after the audio stack has initialized. Design notes:
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
- **Battery intelligence.** Beyond the safeguard: time-remaining
  estimation from live current draw, charge/discharge history in the
  battery panel (the INA260 already measures everything needed).

## Tier 2 - appliance polish

- **Web now-playing remote.** The ws protocol already carries
  track/transport/volume; a now-playing page in the existing web UI
  turns every phone in the house into a remote for nearly free.
- **Idle / ambient mode.** Auto-dim + big clock after N idle minutes
  (panel longevity), wake on touch or audio. The visualizer as an
  ambient screensaver variant.
- **Physical controls.** GPIO rotary encoder for volume, play/pause
  button, config-mapped. Tactility makes it hardware, not a computer
  in a box. (Pairs naturally with the amp-enable GPIO work: both grow
  a small GPIO layer in boompid.)
- **Auto-update toggle.** Default off, applies updates at idle (no
  active source) only. Closes the original plan's last update item.
- **Home Assistant / MQTT discovery.** Play state, volume, source as
  entities; the HA crowd overlaps heavily with the audience that
  would build one of these.

## Tier 3 - keeper of the fleet

- **Diagnostics in settings.** CPU temperature + throttle events (a
  Pi in a sealed box wants this), Wi-Fi RSSI, and a "download support
  bundle" button (journal tail + redacted config).
- **Settings backup/export** for painless reflashes.
- **Hardware watchdog** (`RuntimeWatchdogSec`) for hung-kernel
  recovery - mind the interplay with trial boots (arm only after
  boompi-boot-commit settles).
- **Optional web-UI authentication.** The settings surface is
  deliberately open on the LAN today (home posture); an optional PIN
  gate would suit less-trusted networks.
