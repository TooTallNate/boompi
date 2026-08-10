# boompi

## 2.1.0

### Minor Changes

- d736050: Experimental "Classic AirPlay only" toggle (web settings, AirPlay
  section): modern iPhones do not let AirPlay 2 receivers control
  playback, so the speaker's play/pause/next buttons only work on
  classic AirPlay. Enabling the toggle advertises the speaker as a
  classic-only receiver - transport controls work, multi-speaker audio
  does not. Off by default; needs real-phone verification.
- 3478ccc: 12/24-hour clock option: the footer and screensaver clocks can now
  show 24-hour time, or 12-hour with an AM/PM indicator (the default;
  previously 12-hour with no indicator). Toggle lives in the panel
  settings and the web UI's Clock & timezone section.
- 54c0de6: Built-in software updates. The speaker checks GitHub Releases for new
  OS versions (a "bleeding edge" toggle follows every green dev build
  instead of tagged releases), shows the running version in both settings
  UIs, and installs updates itself: assets stream straight into the
  inactive A/B slot, are sha256-verified, and boot through the usual
  fail-safe trial. Boot is also quiet now - no more console text on the
  panel during startup.
- c6db67b: Home Assistant integration: point the speaker at your MQTT broker
  (web settings, Home Assistant section) and it appears in HA
  automatically via MQTT discovery - one device per boombox with
  playback state and album art, volume, transport buttons, a Bluetooth
  pairing switch, battery sensors (with long-term history for free),
  CPU temperature, screensaver and update-channel selects, reboot, and
  an OS update entity: boompi updates show up in HA's update dashboard
  with live install progress, installable from the couch.
- 6246b69: Idle screensavers: after a configurable number of idle minutes with
  nothing playing, the panel switches to mostly-black moving content -
  a drifting clock (default), Matrix digital rain, or drifting album
  art - protecting the display from the burn-in both boxes were showing
  after long static idle. A tap or starting playback wakes the screen.
  Style is selectable on the panel and in the web UI (which also has
  the timeout).
- a391570: Touch ripples are back: every tap on the panel spawns a v1-style
  expanding water-drop ring at the touch point, confirming the touch
  registered and where. Renders above everything (including the
  screensaver) and costs nothing while the screen is untouched.

### Patch Changes

- 7266c77: Finer progress reporting: update and emoji font downloads now report
  in 1% steps (was 2%), and software installs describe what is
  happening at each stage (downloading system, verifying checksums,
  boot files, restarting) in both the panel and web settings UIs.
- f799e95: Bluetooth dongle recovery now escalates to a USB port power cycle
  when softer resets fail - the boot-time wedge some kernels produce
  (HCI dead from second ten) previously required physically power
  cycling the whole box; it now self-heals in about fifteen seconds
  (sibling USB devices briefly re-enumerate). Touch ripples also land
  under the finger on rotated panels now instead of at mirrored
  positions.
- 7fb57a9: Two reliability fixes from a bench incident: AirPlay audio could go
  permanently silent because its PCM pipe lived in /tmp, where the
  daily tmpfiles clean could reap it (the boxes boot with a months-old
  clock until NTP lands, so boot-created files look ancient) - it now
  lives in /run/boompi, which is never age-cleaned. And the Bluetooth
  dongle recovery no longer USB-resets a truly-dead dongle every four
  seconds forever: resets back off exponentially (up to 10 minutes),
  recover an interrupted reset that left the dongle de-authorized, and
  never strand the device in the off state.
- 7d7da9d: The panel's transport controls now dim (with a hint) during AirPlay
  sessions that cannot be controlled remotely. Modern iOS runs no DACP
  server for AirPlay 2 streams, so play/pause/next from the speaker
  silently did nothing; the buttons now reflect reality and light up
  automatically for senders that do support remote control.
- 78180bd: Fix AirPlay connections failing on current iOS with the Bookshelf or
  TV icon presets: the third-party icon feature bits double as
  authentication requirements (bit 26 demands MFi hardware auth and the
  sender aborts the handshake; bit 51 demands HomeKit PIN pairing). The
  non-Apple icon presets are removed; Generic and the Apple model
  presets (HomePod mini, HomePod, Apple TV) connect fine. Boxes that
  had the Bookshelf or TV preset selected keep their custom model name
  but no longer advertise the poisoned bits.
- d736050: Kernel updated to the current rpi-6.6.y head, picking up ten months
  of VC4/HVS display driver fixes aimed at the GPU hangs that could
  freeze the Pi 3's screen during long playback sessions.
- 8493270: Withdraw the AirPlay pairing-code option: iOS answers the pairing
  advertisement by demanding MFi-authenticated setup, which needs
  Apple's authentication chip, so the code prompt can never succeed.
  The bookshelf icon it would have unlocked is gone with it - custom
  icons are MFi-only territory. Generic and the Apple model presets are
  unaffected.
- d49f248: Now-playing layout always fits the screen: at larger text sizes the
  volume slider could overflow into the footer and become unreachable.
  The AirPlay "controls not supported" hint no longer occupies layout
  space - disabled transport buttons now show a brief toast when tapped
  instead. Volume slider drags are also throttled (~10 updates/s with
  the final position always applied), so the level tracks the finger
  instead of crawling after it.
- 613ea20: The AirPlay device icon can now be chosen from the speaker's own
  settings screen, not just the web UI, and both pickers use new icon
  artwork (generic speaker, HomePod mini, HomePod, Apple TV).
- 55d1e84: Retire kexec update trials: kexec into a different kernel build hangs
  after "Bye!" on both boards (long known on the Pi 3, confirmed on the
  Pi 4 during the v2.0.0 rollout). The Pi 3 keeps its one-shot PM_RSTS
  firmware trial; the Pi 4, whose rev <= 1.3 PMIC power-cycle wipes
  every firmware one-shot flag, now commits the candidate before the
  reboot and rolls back automatically if it boots unhealthy.
- b76204c: Fix vanishing AirPlay/Spotify adverts: systemd-resolved's built-in
  mDNS responder fought avahi for the speaker's .local hostname, which
  could leave avahi renaming itself in an endless loop (no service
  adverts at all) or advertising under a shifted name that broke
  AirPlay connects. resolved now leaves multicast DNS entirely to
  avahi.
- 594e292: Allow re-checking for updates while an update offer is already shown
  (panel and web): a newer build can supersede the stored offer between
  the six-hourly automatic checks, especially on the edge channel.
- b2c7d8d: Screensaver Preview button: trigger the selected screensaver
  immediately from the panel settings or the web UI ("Preview on
  speaker"). Previewing while music plays no longer self-dismisses -
  only playback _starting_ wakes the screen. Also corrects the Classic
  AirPlay toggle's description: speaker-side controls are missing on
  AirPlay 2 because its control channel is encrypted and not yet
  reverse-engineered (iOS drops the classic channel on AP2 sessions),
  not because iPhones forbid receiver control outright.
- 692afdf: Unknown keys in the persisted config are now warned about and ignored
  instead of failing the parse - previously a leftover key from a
  withdrawn feature (or a config written by a newer build) could keep
  the daemon from starting at boot.
