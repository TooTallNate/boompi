# boompi

## 2.2.0

### Minor Changes

- 70fbd59: Foundation for board-generic images: box-specific configuration now
  has a home that survives OS updates. A box profile in /data/box/ can
  carry a firmware config fragment (config.txt - display overlay,
  rotation, wiring, amp GPIO), a hardware.toml merged over the boompid
  config, and an env file for the panel service (e.g. rotation). The
  firmware fragment is re-materialized into a fenced section of
  config.txt whenever a boot partition is written - by the on-box
  updater and by boompi-update-slot - so a box keeps its identity
  across A/B updates. Boxes without a profile behave exactly as
  before; extracting the two bench boxes' specifics out of the pi3/pi4
  images comes next.
- 418ee4e: Drag-drop provisioning from any OS: flash the generic image, copy a
  boompi-box/ directory (the box profile) onto the boot partition your
  OS mounts, and boot. The appliance ingests the bundle into /data/box/
  on startup - before boompid launches, so the hardware profile applies
  immediately - merges the firmware config into both boot slots,
  renames the bundle \*.applied (drop a fresh one to re-provision), and
  reboots once only if the active boot config actually changed.
  scripts/provision-sd.sh packages a boxes/ profile onto a mounted card
  for convenience; the manual copy works identically.
- 734ad93: The pi3/pi4 images are now board-generic: everything specific to one
  physical build (display overlay, rotation, DAC HAT, panel video mode,
  battery wiring, UI scale seed) moved out of the images into box
  profiles (/data/box/, worked examples in boxes/). An unprovisioned
  image boots to a recovery posture - HDMI, onboard Bluetooth/audio,
  ssh, web settings - and profile-dependent features explain what is
  missing. Profiles also carry kernel arguments now (cmdline.txt
  fragment, e.g. the video= mode for an EDID-less panel), and a
  profile's [settings] only seeds the first boot so user choices are
  never clobbered. scripts/provision.sh provisions a running box over
  ssh; docs/PROVISIONING.md documents the design.
- 73c992d: On-box low-battery safeguard. The panel shows a warning banner (and
  wakes the screensaver) when state of charge drops to 15%, clearing
  with hysteresis or whenever the charger is connected. If SoC falls to
  5% or the pack holds below 18.3V for a sustained 60 seconds while
  discharging, the box announces itself on the panel and powers off
  cleanly before the pack reaches the BMS cutoff - a deep discharge
  corrupted an SD card once already. Thresholds are configurable and
  the auto-shutdown can be disabled in [battery] config. The web UI
  shows the low state, and the MQTT battery payload carries it for
  Home Assistant automations.
- a728b7e: The pi3 image now builds the same kernel config as the pi4
  (bcm2711_defconfig - the config Raspberry Pi OS's kernel8.img uses to
  boot Pi 3/3+/4/Zero 2 from one binary), the first structural step
  toward a single unified image. The pi3's TF-A armstub is gone with
  it: it existed to provide PSCI for the retired kexec trial mechanism,
  and its fixed load addresses imposed a 24MB kernel ceiling the fatter
  unified kernel would have hit. Stock firmware boot chain, spin-table
  SMP. The change ships through the pi3's crash-safe PM_RSTS trial: a
  candidate that fails to boot falls back to the old slot on its own.
- a8d84bc: One image for every board, and two release assets total. The pi3/pi4
  builds collapse into a single board-generic image: one kernel
  (bcm2711 config, proven on the Pi 3 by A/B trial), both GPU firmware
  sets and DTBs on each boot slot, [pi3]/[pi4] conditional sections in
  config.txt, and the box profile carrying everything hardware-specific
  as before. Releases now publish exactly a flashable sdcard image and
  one self-contained update bundle (boompi-update.tar: checksums and
  version stamp first, then the zstd payloads) which the updater
  consumes as a single stream, routing what it needs onto the inactive
  slot's partitions and skipping the rest. The Pi 3 trial-boot arming
  now works on both kernel eras: TF-A kernels preserve a pre-written
  PM_RSTS through BL31, stock kernels get the partition via the reboot
  syscall argument (the restart handler otherwise clobbers PM_RSTS -
  bench-falsified and fixed the same night).
- 6c2bba5: Battery intelligence: a proper state-of-charge estimator replaces the
  static voltage map. Full charge is detected the way chargers define it
  (sustained current tapering to zero at a voltage plateau), and the
  plateau voltage is learned and persisted per box - so every box
  self-calibrates to its own CC/CV converter setpoint, including after
  the setpoint is changed. Once a full charge anchors the estimator, SoC
  is coulomb-counted from the INA260 (immune to load sag), pack capacity
  is learned from ordinary partial discharges, and a time-remaining
  estimate appears while discharging. New in Home Assistant: battery
  time remaining and charging entities. The panel battery screen gains a
  TIME LEFT stat and a full badge, and the web settings UI now shows
  battery status.

### Patch Changes

- 5d447e7: Home Assistant gains a Battery current sensor (amps, signed - it goes
  negative while charging), and all entities now declare device-scoped
  names so newly added ones get clean entity ids instead of a doubled
  device prefix.
- 555d6d3: Home Assistant gains a Battery state sensor (full / charging /
  discharging / idle). The full detection already existed on the panel
  and in the payload, but HA only had a charging binary - so chargers
  that terminate and periodically top the pack back up (rather than
  holding a float) looked like they cycled forever without finishing.
- 05734e2: UIs explain an absent battery instead of hiding it. The panel's
  footer battery icon is always visible (empty outline without
  telemetry) and the battery screen distinguishes "not configured"
  (with the exact /data/box/hardware.toml snippet to add) from
  "sensor not responding" (with the probe error). The web settings
  page shows the same guidance. Groundwork for board-generic images,
  where a fresh unprovisioned box is a normal state rather than a
  mystery.
- aff470e: Bluetooth configuration is now identical on every board:
  SecureConnections=off moves into the shared main.conf (it was pi3-only
  for the bench box's counterfeit-CSR dongle, but any cheap dongle can
  have the same defect, dongles migrate between boxes, and JustWorks
  pairing has no MITM protection either way). The per-board rootfs
  overlays now differ only in the model name.
- c0c0253: Out-of-box support for the common Bluetooth USB dongle chipset
  families, befitting a generic image. The TP-Link UB600 turned out to
  be the same RTL8761BU as the UB500 hiding under TP-Link's own USB
  vendor id, which the pinned 6.6 kernel does not map to the Realtek
  firmware loader - hci0 appears but scans find nothing. The upstream
  fix (v7.2-rc1) is backported as a kernel patch. Firmware coverage
  grows to Realtek combo adapters (8821/8822/8852) and MediaTek
  MT7921/MT7922, with post-build assertions per family.
- 945e89b: The Bluetooth dongle self-heal ladder now recovers a controller that
  vanishes entirely, not just one that refuses to power on. The pi3
  migration surfaced the gap: the 6.6.78 boot wedge can remove the hci
  outright, and the old ladder both keyed off a present-but-unpowered
  adapter and located the dongle by walking from the hci - so a dead
  controller was invisible to it twice over. Recovery candidates now
  also come from a USB device-class scan, stuck-disabled hub ports from
  interrupted escalations are re-enabled first, adapter removal is
  handled (clearing state and surfacing "unavailable" pairing), and a
  30-second health tick retries on a loop that was previously purely
  event-driven. Boxes with onboard or no Bluetooth stay quiet.
- 3c7d198: The CPU temperature sensor in Home Assistant now updates every
  minute - previously it only published when the MQTT session
  (re)connected, leaving hours-wide gaps in the history graph.
- a9dc07e: Home Assistant now correctly offers edge builds as updates: HA
  compares versions with semver, where the edge stamp's "-sha" suffix
  means prerelease - so an edge build ranked OLDER than its base
  release and HA showed "Up-to-date" despite listing a newer version.
  Suffixed stamps are now presented to HA in a non-semver shape
  ("v2.1.0 (f06b1b6)"), falling back to plain string comparison;
  stable tags keep real semver ordering.
- f06b1b6: Fix the unique-hostname unit failing on freshly written OS slots: it
  ran while the rootfs was still read-only, the /etc/hostname write
  failed silently, and NetworkManager later reverted the hostname to
  the stale default - which also re-registered the speaker in Home
  Assistant as a duplicate device after an update. The unit now waits
  for the rw remount, and the MQTT device identity derives directly
  from the SoC serial so it can never follow a stale hostname.
- fab197d: The pi3 and pi4 images now differ only in board facts (kernel,
  firmware, TF-A). The vestigial model config key is gone (the Hello
  handshake reports the device-tree model string instead), the
  per-board rootfs overlays are deleted, onboard Bluetooth UART
  firmware ships on both boards (the generic pi3 image left onboard BT
  enabled but never shipped its .hcd), and the post-build assertions
  now check the real A/B mechanism (PM_RSTS/autoboot tooling and the
  box-profile apply script) instead of the retired kexec - plus both
  BT firmware families unconditionally, replacing a pi4-only check
  that had gone silently dead.
- c5e6294: The Matrix screensaver now fills any screen width: the rain computes
  its column count from the display instead of assuming 800px (which
  clipped the last column on the Pi 3 and left dead space on the Pi 4's
  wider panel), and the column field centers itself.
- 3b4127d: Images now ship Realtek Bluetooth USB dongle firmware (RTL8761B/BU:
  TP-Link UB500, ASUS USB-BT500, newer UB400 revisions) so a
  recommended dongle works the moment it is plugged in - previously it
  would enumerate but hci0 would never appear. The Realtek btusb kernel
  option is pinned by fragment, and a post-build assertion guards the
  firmware files. Comments no longer call the pi3's dongle counterfeit:
  it is a genuine TP-Link UB400 whose CSR8510 chip (BT 4.0) simply
  predates Secure Connections while advertising it.
- cab380e: Audio output paths get the same guard treatment as Bluetooth dongles:
  kernel pins and post-build assertions for the USB Audio Class driver
  (one driver covers essentially every USB sound card - no per-chipset
  firmware, unlike Bluetooth) and the common I2S DAC HAT modules
  (HiFiBerry-compatible machine drivers + PCM51xx codecs). A boombox
  image that cannot make sound must not build.

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
