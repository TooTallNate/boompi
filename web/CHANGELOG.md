# boompi

## 2.3.0

### Minor Changes

- 5596997: Boxes now tell clients what they can do. The hosted remote and the
  upcoming iOS app outlive any given box's software, so the connection
  greeting grew a capabilities list (wifi, wifi_scan, battery,
  bluetooth, games, ...) and the UIs hide what a box doesn't have:
  connect to a speaker whose software predates Wi-Fi-over-Bluetooth and
  the Wi-Fi page says so instead of scanning into the void; connect to
  a hard-wired box with no battery sensor and the Battery page simply
  isn't there. Hardware-dependent flags read live state, old boxes that
  predate the field get a sensible legacy set, and unknown future flags
  are ignored - so mismatched client/box versions degrade politely in
  both directions.
- 139439b: Connected clients now keep the clock honest when the internet can't.
  The boxes have no RTC, so off-network the clock drifts months into
  fantasy the moment NTP is unreachable. Every client that connects
  already knows the time, so now they offer it: the web app sends its
  clock on every WebSocket connect, and native apps can write the same
  `set_time` message to the BLE control characteristic (documented in
  docs/BLE.md for the upcoming iOS app). The offer is applied only when
  timesyncd reports the clock was never NTP-disciplined this boot,
  implausible values are rejected, and NTP silently overwrites
  client-set time whenever it becomes reachable again - a working
  internet connection always wins.
- eac1c6e: The box now reads the time off your phone. Phones expose the Bluetooth
  Current Time Service as a GATT server - iPhones in particular - so
  whenever a connected device's GATT database resolves and offers it,
  boompid reads the phone's clock (plus its UTC offset when published)
  and steps the system clock through the same guarded path as the web
  app's time offers. It is strictly the last resort: never when NTP has
  synchronized this boot, never when any client (web, app) recently set
  or confirmed the clock, and only from bonded devices - because the
  encrypted read can otherwise pop a pairing dialog on the phone
  mid-connection. In practice that leaves exactly the case it exists
  for: an off-grid box playing music from a paired phone with no
  controlling client in sight. Between this, the web
  app, and the upcoming iOS app, an RTC-less box now has three ways to
  learn the time before it ever finds the internet.
- 49a31ef: Games launch from anywhere now: the web remote, the on-box settings
  page, and the iOS app all list the library with a Play button (BIOS
  files excluded), riding the same protocol action the panel uses. Pick
  a game from the couch; the speaker's screen does the rest.
- a144d99: The boombox is secretly a game console. RetroArch ships in the image
  with cores for NES, SNES, Game Boy/Color/Advance, Nintendo 64, and
  PlayStation; ROMs are yours to upload from the web settings page
  (stored on /data, which now grows to fill the SD card on first
  boot). Pair a Bluetooth gamepad with the same pairing button the
  speakers use, pick a title on the panel's new Games screen, and the
  display hands over to the game - Start+Select opens the RetroArch
  menu to quit, and the web page has a Stop button as the no-gamepad
  escape hatch. Music and gameplay coexist: an active AirPlay/
  Bluetooth/Spotify stream ducks the game's volume to a configurable
  level (50% by default), because it is still a speaker first. Save
  files and states live on /data and survive OS updates.
- 5584586: The configurator lives on the box: a "Box hardware" section in the
  web settings UI edits the box profile live - presets for the known
  builds, editors for the firmware fragment, kernel arguments, hardware
  TOML, and panel environment. Apply writes /data/box/, re-fences both
  boot slots, and prompts a reboot only when the boot config actually
  changed; Download packages the profile as the boompi-box.tar bundle
  for provisioning the next SD card. Shipping the configurator inside
  the image it configures means the profile schema and its editor can
  never drift apart. Validation refuses the foot-guns (root= overrides,
  multi-line cmdline, fence markers, unparseable TOML).
- 5ebd385: Root filesystems are now 1024MiB (doubled from 512MiB). The 512M slots
  had ~100MB of headroom left for new features; both boxes' partitions
  were grown in place by boompi-migrate-roots - no reflash, no data
  loss, no SD card extraction. Images from this release require migrated
  (or freshly flashed) root slots; the updater refuses delivery to
  unmigrated boxes with instructions rather than risking the neighbor
  partition.
- 55d1a7d: The panel's settings screen got the same declutter treatment as the
  web app: instead of one long touch-scroll through a dozen cards, a
  compact icon rail (icons only - the 4-7" panels have no room for
  labels) splits settings into five focused tabs: appearance, media,
  Wi-Fi, Bluetooth, and software. Same grouping as the web sidebar, so
  the two UIs teach each other's layout. The Wi-Fi tab hides itself on
  boxes without Wi-Fi hardware, and the whole rail scales with the
  text-size setting like everything else on the panel - including
  shrinking to fit: at large text sizes on the small panels the tabs
  and their gaps compress together so the rail always ends above the
  footer instead of running past it.
- 519a37c: Security posture rework. SSH is key-only (PasswordAuthentication no;
  the root password works exclusively on the HDMI console and is
  documented as such) and the image ships trusting nobody: the baked
  authorized_keys is gone, per-box keys live at /data/ssh/ and arrive
  via the flash-time bundle, the web hardware page, the provision
  scripts, or `boompi-box add-key`. The hardware page/API can be
  locked - refused unless an ssh key is authorized first, so the lock
  can never remove the last remote path in - after which boot
  configuration is ssh-only via the new `boompi-box` CLI (show, edit,
  apply, lock/unlock, add-key, export - the provisioning-bundle
  convenience works on locked boxes). Factory reset is removed from
  the web UI and network APIs entirely: `boompi-factory-reset` over
  ssh or console. Recovery matrix and the full story in
  docs/SECURITY.md.
- be1482e: The settings web app grew out of its single endless scroll. Fourteen
  stacked cards became nine focused pages behind a collapsible sidebar
  (shadcn/ui) - General, Audio & AirPlay, Display, Bluetooth, Wi-Fi,
  Games, Battery, Home Assistant, Software - so finding a setting is a
  click, not an archaeology dig. Under the hood the whole UI moved to
  shadcn components on a shared workspace package (@boompi/ui) that
  also carries the protocol types and a transport abstraction: the same
  section components now run against WebSocket+REST on the box or a
  BLE GATT link on the hosted remote app, with IP-only features
  (network scans, ROM uploads) degrading gracefully when there's no IP
  path. Same dark boompi palette, real design system underneath.
- 4609fcc: Each box now offers a guest SMB network share of its games library -
  drag ROMs straight from Finder or Explorer onto smb://<box>/games,
  no credentials. The share is scoped to /data/games and nothing else:
  ssh keys, Wi-Fi credentials, and the box profile are outside the
  exported tree by construction (a build assertion keeps it that way).
  The boxes appear in the network sidebar automatically via mDNS.
  macOS metadata droppings are vetoed server-side and ignored by the
  library scanner.
- 72482ce: Volume has been rebuilt around two independent tracks. The music
  track is one level shared by every audio source - phone volume
  buttons, the panel slider, and the web slider all move the same
  value, it follows you across Bluetooth, AirPlay, and Spotify, it's
  pushed to newly connected phones so switching sources never jumps the
  loudness, and it survives reboots. The game track is RetroArch's own
  level, set from the panel or web - no more ducking when music plays.
  The system output stays at reference always, the spectrum bars now
  show the music itself regardless of how loud it's playing, and the
  per-device Bluetooth volume-mode setting is gone (obsolete now that
  senders deliver full-quality audio and the speaker renders volume).
- 79d0c46: Wi-Fi management no longer needs an IP path. Scanning and
  password-joins now ride the protocol (`scan` answers with a
  `wifi_networks` broadcast, `connect` carries the psk), so the
  Bluetooth remote at boompi.n8.io manages Wi-Fi exactly like the box's
  own settings page: see nearby networks with signal strength, join new
  ones, disconnect, forget, toggle the radio and hotspot - all over the
  radio, which conveniently survives the Wi-Fi changes it causes. Join
  progress broadcasts the same way the setup wizard's does. The REST
  endpoint stays as the synchronous-error flavor the on-box web app
  prefers, but the two paths now render one identical Wi-Fi UI.

### Patch Changes

- af9e3b5: The control channel's Bluetooth name is now "🎛️ <speaker name>"
  instead of "Boompi Remote - <speaker name>" - the emoji prefix costs 8
  of the advert's hard 29 bytes instead of 16, more than doubling the
  space for the name you chose. To guarantee the advert always fits,
  speaker names are now capped at 21 UTF-8 bytes (server-enforced,
  emoji-safe), and every name field - web settings, setup wizard, iOS -
  shows a live bytes-used counter while you type.
- 068b470: Track info and cover art no longer freeze mid-session. Phones
  (iOS especially) silently drop the AVRCP control channel while A2DP
  audio keeps streaming, and a missed D-Bus event could leave boompid
  showing the same song and cover forever - skipping tracks did nothing
  because the box never heard about them. A 30-second reconciliation
  sweep now compares believed state against BlueZ's actual object tree:
  missed players get adopted, vanished players clear the stale track,
  and a connected phone that lost its control channel gets the profile
  actively reconnected (verified to resurrect metadata live). Art
  fetches follow track changes again, as they always should have.
- 93e6471: Bluetooth on the boxes got dramatically calmer. The advertising
  keep-alive was re-registering every 15 seconds, which the UB500's
  controller handles badly: a stream of "unexpected advertising set
  terminated" kernel events, EBUSY races, and disturbed connections and
  pairing while it churned. Re-assertion is now event-driven - it fires
  the moment a remote disconnects (the case that actually leaves the
  broadcast dead) with a 5-minute safety net behind it, instead of
  hammering the radio on a timer. And while the Bluetooth pairing
  window is open, LE advertising parks entirely: classic inquiry and LE
  advertising fight over the dongle's radio ("Failed to set mode:
  Busy") and game controllers could never see the box - pairing a
  gamepad while a remote stays connected now just works.
- 1082988: The speaker stays visible in the Bluetooth choosers - including while
  a remote is already connected. Two quirks of the fleet's TP-Link
  UB500 dongle (RTL8761B) shaped this: it silently stops broadcasting
  its LE advertisement after connect/disconnect churn while BlueZ still
  reports it active, and cycling the advertisement registration while a
  client is connected drops that client's connection. So boompid now
  re-asserts the advertisement on a timer only while idle (healing the
  silent death), and the moment a remote connects it registers a spare
  advertising instance instead - the controller runs three, so a second
  remote can still discover and connect. Verified live: the iOS app and
  the web remote controlling the same speaker simultaneously, changes
  reflecting in both directions, while the box stays discoverable.
- fb4826b: Boompi has a face: the boombox-with-a-raspberry logo now fronts the
  README, greets you on the panel's first-boot screen and the web setup
  wizard, tops the hosted remote's connect card, appears as the favicon
  and sidebar mark in the web UIs, and is the iOS app icon. The README
  also drops the "two custom boomboxes" framing - Boompi is a general
  build-your-own boombox OS for the Raspberry Pi, and now it reads
  like one.
- 77d3071: Experimental kernel patch: the Bluetooth dongle's extended advertising
  is now bypassed entirely. Its firmware claims LE Extended Advertising
  support but delivers spec-violating termination events, EBUSY races,
  and broadcasts that silently die - the root soil of every advertising
  workaround this month. A new kernel quirk (candidate for upstream
  submission, sibling of the extended-scan quirk mainline already
  applied to this chip) drops the host back to legacy advertising, the
  decade-hardened path where BlueZ multiplexes instances in software.
  If the field results hold, the parking/re-assert machinery becomes
  belt-and-suspenders instead of load-bearing.
- b69fb96: Bluetooth device lists now group by what a device is - Phones & audio,
  Game controllers, Other devices - instead of one flat list where an
  iPhone's audio pairing, its remote-control connection, and a DualSense
  all looked alike. The box classifies from BlueZ's device class and the
  web remote, iOS app, and on-box settings page all render the groups.
- 4c52c35: Switching away from Bluetooth (to AirPlay or Spotify) and back no
  longer strands the session in limbo: the panel showed track titles
  but no source in the footer, and freshly fetched cover art was thrown
  away. Bluetooth now reclaims the display the moment it publishes
  while no other source holds it, and a failed cover-art fetch retries
  after ten seconds instead of giving up on that track forever.
- 15daef1: Bluetooth gets a nightly immune-system reset. The fleet's USB dongle
  has now been caught three times silently losing controller state -
  advertising broadcasts, and most recently the classic device name
  (the box became invisible to phones trying to pair, while every
  setting read back correct). Each flavor got a targeted fix, but the
  pattern predicts more, so: every night at 3:30 the Bluetooth stack
  restarts and rewrites every controller register from scratch - only
  when nothing is connected (an active music session or game controller
  skips the refresh until the next night), and the box's services
  re-register automatically like they already do. Any state rot, known
  or not-yet-discovered, now lives at most a day.
- c34e570: The spectrum bars no longer flatline when an iPhone plays over
  Bluetooth at moderate volume. iOS scales its own PCM (the box's sink
  stays at reference), so the captured samples carried the phone's
  steep volume taper straight into the display while AirPlay and
  Spotify bars kept dancing. The visualizer now undoes the phone-side
  attenuation and re-applies the same linear volume every other source
  gets - consistent bars at the same loudness, whatever the source.
- 0e1ed6c: Bluetooth volume now works like AirPlay and Spotify Connect: the
  phone sends full-quality audio and volume commands, and the speaker
  renders the volume. iOS's previous behavior - scaling the audio on
  the phone before sending it - turned out to be a reaction to
  PipeWire's participation in the Bluetooth volume handshake, not an
  Apple constant (verified against the v1 image, where the same phone
  behaved correctly). One PipeWire setting restores spec behavior, the
  iPhone-specific volume mode is no longer auto-assigned, and the
  speaker's volume slider is authoritative for every source.
- 6bef719: Bluetooth volume is now correct, confirmed by measurement. The
  speaker keeps absolute-volume negotiation on (the phone sends volume
  commands that drive the music track while streaming constant-level
  audio), the "stuck very quiet" state turned out to be a stale bond -
  re-pairing anchors the session at full level - and a bench-calibrated
  +4.3dB makeup gain on the Bluetooth stream makes identical content
  measure identically loud across Bluetooth, AirPlay, and Spotify
  (the latter two already agreed to 0.1dB).
- c768d81: Kernel patch: the fleet's Bluetooth dongle (RTL8761BU) claims HCI 5.1
  but its firmware doesn't implement the LE extended scan commands,
  producing EBUSY storms whenever BlueZ scans with a connection active.
  Mainline Linux fixed this after our kernel's release by quirking the
  chip back to legacy scan commands; the image now carries that fix as
  a backport. One more entry in this chip's lying-about-its-features
  rap sheet - and this one is upstream-certified.
- ccf5520: Bluetooth on the Pi 3 no longer corrupts itself under concurrent load.
  The Pi 3's USB controller can complete transfers out of order when a
  gamepad, a pairing burst, and USB audio all share the bus; the
  Bluetooth dongle's HCI stream then reassembles garbage and the radio
  wedges until a reboot (one crash of the Bluetooth daemon on the bench
  traced back to this). The kernel's force_poll_sync option serializes
  the completion path and is now set for every box.
- d65a74f: pgrep and pkill exist on the box now. Buildroot's default busybox
  config omits them, and every bench-debugging session rediscovered
  that the hard way ("sh: pkill: not found" while trying to stop a dev
  daemon). A busybox config fragment turns them on.
- f1f53ea: Native browser confirm() popups are gone from the web UI. Restart,
  device unpair, ROM delete, and the hardware profile apply/lock flows
  now use a proper styled confirmation dialog (shadcn AlertDialog via a
  shared ConfirmButton) - keyboard-dismissable, themed, and immune to
  webviews that silently swallow window.confirm.
- 7847aba: CPU temperature is no longer a Home Assistant exclusive. The box
  broadcasts its thermal state over the protocol (30s cadence, on
  change), so the web UIs show it on the General page and the iOS app
  in General > About - along with a live "throttled" warning whenever
  the firmware is actively limiting the clock from heat or a sagging
  power supply, the invisible condition that once cost a full bench
  session to diagnose. MQTT keeps publishing the same reading for HA.
- 2a6e867: Boxes on the bleeding-edge channel now check for updates every 10
  minutes instead of every 6 hours - a green build lands with most
  pushes, and the whole point of opting in is riding the front of the
  wave. Stable stays at 6 hours, and flipping the channel toggle takes
  effect at the next wakeup without a restart.
- d0f08cd: Gamepads work in-game now, whatever the brand: the image ships the
  full udev autoconfig pack (DualShock/DualSense, Switch Pro, every
  8BitDo mode, Xbox and friends - 400+ profiles), the kernel grew xpad
  for wired/X-input pads, and BlueZ accepts reconnects from pads that
  never bond properly (8BitDo's specialty). RetroArch also rotates with
  the panel now instead of assuming the display is landscape, and the
  web settings page gained a speaker volume slider - the same control
  as the panel's, from your phone.
- 6ed1913: Gamepad pairing actually works now. Three bugs conspired against the
  DualSense: bluetoothd shipped without its HID input profile (the pad
  would pair, find nothing to connect to, and power itself off - now
  enabled, with HoG for BLE pads), boompid's post-pair audio dial-back
  treated the pad like a silent phone and disconnected it after 8
  seconds (gamepads are now exempt), and the autopair flow flashed a
  Pair/Reject dialog that auto-resolved before anyone could read it
  (replaced by a proper "Pairing..." progress state on both the panel
  and the web page).
- 9dee7b6: Pairing an iPhone while a gamepad is connected no longer ends in
  "Connection Unsuccessful". When A2DP setup runs long (a busy radio -
  gamepad traffic, pairing bursts, and USB audio all share one bus on
  the Pi 3), the box now brings the audio profile up over the existing
  link instead of disconnecting the phone mid-setup, and it stops
  poking the phone's hands-free profile a speaker can't answer anyway.
- 5bbb500: The box hardware configurator moves off the main settings page onto
  its own page (#/hardware), reachable only through a quiet footer
  link, with a warning banner and a confirmation before applying -
  boot-configuration edits should take deliberate navigation, not an
  accidental scroll-and-click next to the everyday settings.
- 244dbcd: The version in the connection greeting (shown as "Software" in the
  apps' About screens) now reports the real OS image version instead of
  a fossilized "2.0.0-dev". Changesets bump the image version while the
  Rust workspace keeps a placeholder, and the greeting was reading the
  wrong one - the Software Update page was already correct, since it
  read the on-disk image stamp. One source of truth now: everything
  reads /etc/boompi-version.
- e9b71b1: There's now a hosted remote control at boompi-remote.vercel.app: the
  same settings UI as the box's own web app (shared @boompi/ui
  sections), but connected over Web Bluetooth to the speaker's BLE GATT
  control bridge - no shared Wi-Fi, no IP network, no install. The
  browser's Bluetooth chooser is the discovery step (it lists nearby
  boompis by their advertised control service), and the link speaks the
  identical JSON protocol as the WebSocket, chunk-framed to the ATT MTU.
  IP-only features (network scans, ROM uploads, timezone) explain
  themselves and point at the on-box settings page; the hotspot toggle
  works over BLE as the escape hatch that creates an IP path. Chrome and
  Edge today; iOS needs the upcoming native app (Safari has no Web
  Bluetooth). Nothing in the OS image changes - this entry is for the
  changelog trail.
- 775c82b: The native iOS app exists (ios/): CoreBluetooth against the same GATT
  control bridge the hosted remote uses, so it works with no Wi-Fi, no
  account, no setup. It scans for the boompi service, auto-connects to
  the most recently used speaker the moment it's in range (the common
  one-boompi case never sees a picker), offers the phone's clock to the
  RTC-less box on connect, and gates every section on the box's
  declared capabilities - a hard-wired box shows no battery, an
  un-updated box explains its missing Wi-Fi management instead of
  breaking. All logic lives in a Swift package that builds and
  self-checks with the bare toolchain; Xcode is only needed to produce
  the app itself. Nothing in the OS image changes - changelog trail
  entry.
- 29430f2: The iOS app reaches feature parity with the web remote. New in the
  drill-downs: panel text size and visualizer opacity (Display), the
  emoji style catalog with downloads and progress (Display > Emoji
  Style), online album art fallback, screensaver preview, the AirPlay
  device-icon picker with native SF Symbols for the HomePods and Apple
  TV plus the classic-AirPlay toggle, game volume, Home Assistant MQTT
  configuration, and the Wi-Fi radio switch. Everything rides the same
  protocol messages the web UIs use, and every screen stays
  capability-gated. Still REST-only by design: timezone/NTP, ROM
  uploads, box hardware - those live on the speaker's own settings page.
- e95c974: The OS image now ships iperf3, so network throughput between the box
  and other machines can be measured directly from the bench without
  copying binaries around.
- 2fcdfbf: Uptime in General → System now ticks live (30s cadence, derived from
  the handshake snapshot plus elapsed wall time - a reconnect after a
  reboot resets the baseline) and spells out its units: "2 days 5 hr
  42 min" instead of a four-digit pile of minutes.
- 2626b8a: boompi-migrate-roots now detaches itself into a transient systemd unit
  before surgery and reboots when done. Quiescing /data cascades into
  NetworkManager and sshd, which kills the ssh session that launched the
  script - and on the pi3's first live migration, the script died with
  it, mid-flight (the surgery had luckily already landed; a power cycle
  recovered everything). Also: the workstation updater now arms the pi3
  one-shot trial via both PM_RSTS and the reboot argument, matching the
  on-box script - devmem alone is discarded by spin-table kernels.
- caa9552: Groundwork for 1GiB root slots. New boompi-migrate-roots grows both
  A/B root partitions from 512MiB to 1024MiB in place - no reflash, no
  data loss: /data's filesystem shrinks from its end, root-b is reborn
  as the last GiB of the card, root-a absorbs its old neighbor. Proven
  against loopback replicas of both fleet layouts (including the pi4's
  legacy packed table) on real hardware and in CI. Updates now refuse
  images larger than their slot instead of silently overflowing into
  the neighbor partition, and grow-data learned to measure free space
  from the last partition on disk. Images still build at 512MiB; the
  size bump lands after the fleet migrates.
- 1c6440c: The full hardening batch from the root-slot migration campaign. The
  migration script now syncs the filesystem shrink to media before any
  partition-table work (a kernel partition resize silently discards
  unflushed page-cache writes - the pi4's shrink evaporated exactly
  this way), verifies the table on disk instead of poking a live
  kernel, and defers the root filesystem grow to a new boot-time
  grow-root service. Recovery independence: a getty on tty2 (USB
  keyboard + Ctrl-Alt-F2), NetworkManager runs without /data so wired
  DHCP always works, and /data is fsck'd before mounting. No failure
  of the data partition can strand the box unreachable again.
- da5746d: N64 games work now. The emulator core was being cross-compiled with a
  split personality: the dynamic recompiler's sources were built for
  the Pi's processor, but architecture-conditional build steps
  (including the generated structure offsets the recompiler reads at
  runtime) defaulted to the build server's x86 - so the JIT crashed on
  its very first translated block, on every game. The build now follows
  Batocera's proven Raspberry Pi recipe with the architecture pinned
  explicitly, and RetroArch shares its GL context with hardware-
  rendered cores (which N64 also needed). Verified on hardware: Super
  Mario 64 and Ocarina of Time running with the dynamic recompiler -
  no interpreter fallback.
- e52040f: N64 works on both boxes now. The recompiler build that runs on the
  Pi 3 turned out to crash on the Pi 4's different CPU core (verified
  with the same binary on both boards), so the image now ships
  per-board builds of the N64 emulator - the launcher picks the right
  one automatically - and the Pi 4 variant gets the nicer GLES3
  graphics path as a bonus.
- 43b01a1: Bluetooth senders now always deliver full-quality, full-scale audio.
  The absolute-volume negotiation is disabled at the Bluetooth stack
  level (a carried bluez patch adds a config option for it): iPhones
  were freezing their transmitted audio at a stale low volume when the
  half-disabled handshake left nobody completing the notification loop
  - the "max volume but very quiet" bug. Phones now treat the box like
    any classic speaker (their volume slider is a local gain on their
    side), and the speaker's own two-track mixer is the one true volume.
    Re-pair phones after updating - they cache the old capabilities.
- 44b4ec5: HTTPS works from the box's command line now. The image never shipped
  a CA trust store, so every on-box HTTPS client except the updater
  (which carries its own compiled-in roots) failed instantly with a
  trust-anchor error - curl, wget, anything a bench session shells out
  to. During a wifi debugging session this masqueraded as the network
  dropping TCP data mid-flow and cost an hour of chasing phantom router
  filtering. The Mozilla CA bundle is now installed, and the build
  asserts it actually landed in the image.
- b111704: Pairing a phone while a gamepad is connected actually works now. The
  pairing window used to start an active gamepad scan, and on the USB
  dongle that inquiry traffic starves the listening side of the radio -
  the phone couldn't find or connect to the box until the gamepad was
  disconnected. The scan now only runs when nothing is connected;
  pairing a second gamepad just requires disconnecting the first.
- bb5aa5c: Pairing a phone no longer sabotages an in-progress game. Opening the
  pairing window used to disconnect every Bluetooth device including
  the gamepad (killing inputs mid-game - pads don't reconnect after a
  host-side drop), and the still-broadcasting pad would then be
  "re-discovered" by gamepad autopair, which promptly closed the
  pairing window before the phone could get in. Gamepads now stay
  connected through pairing mode, and already-paired pads never close
  the window.
- 0cd1cd3: Update checks route through boompi.n8.io first. The hosted remote
  gained a caching endpoint (/api/release) that proxies the GitHub
  release lookup, so a fleet of edge boxes polling every 10 minutes
  costs GitHub roughly one request per cache window instead of one per
  box. It proxies live release state rather than baking a version at
  deploy time - Vercel deploys in seconds while the image build takes
  minutes, and this way "a release with assets exists" stays the single
  source of truth, no reconciliation needed. The endpoint is a cache,
  not a dependency: any failure and the box asks GitHub directly, like
  before. Downloads were never proxied - they go straight to GitHub's
  CDN either way.
- 48aa199: The control channel introduces itself properly: the BLE advert is now
  named "Boompi Remote - <speaker name>", so a phone's Bluetooth list
  shows two tellable-apart entries - the speaker (audio) and its remote
  (control), the same pattern car keys use. The Boompi apps strip the
  prefix and show just the speaker name. The advert name is trimmed to
  BLE's 29-byte limit - BlueZ rejects oversized registrations outright
  rather than truncating, which silently stopped one box advertising
  until diagnosed in the field.
- c7aed9d: The "fetch album art online" toggle is gone from every settings UI.
  It shipped as a switch without an implementation behind it - a
  promise the box never kept - and the direction is to make the real
  art paths (AVRCP cover art from the phone) work well instead of
  papering over them with a network fallback.
- 2394b6d: Every remote can restart the speaker now: a confirmed Restart button
  on the web UIs' General > System card and at the bottom of the iOS
  app's General screen (the Settings-app spot). Works over Wi-Fi and
  Bluetooth alike - the remotes reconnect on their own once the box is
  back, about half a minute later.
- 8db1807: The RetroArch menu now rotates with the panel instead of rendering
  sideways. Upstream RetroArch has no way to rotate the menu on KMS
  displays (the screen-orientation backend is an unimplemented stub),
  so the image carries a small patch adding a `menu_rotation` setting -
  menu-only, never combined with the rotation a game requests - which
  boompid sets from the same box profile as the game rotation.
  Touchscreen taps in the menu are remapped to match.
- 4d68f0c: Cover art survives BlueZ's mid-connection identity merge. When a phone
  first appears under a private rotating address and BlueZ later resolves
  it to the real device, the AVRCP player is orphaned under the old path -
  and the art fetcher kept dialing that fossil address forever ("Host is
  down"), while audio played happily on the real one. The OBEX target is
  now resolved at request time: use the live device at the latched path,
  or fall back to the connected device when the merge has erased it.
  Diagnosed and verified live against a box stuck in exactly this state.
- f128d76: Spotify Connect works again. The TLS-stack cleanup two releases back
  left both of rustls's crypto providers (ring and aws-lc-rs) in the
  dependency graph, and rustls panics at the first TLS connection when
  it can't auto-pick one - which killed librespot's session task at
  startup on every box. boompid now installs ring as the process
  provider explicitly, so no dependency-graph drift can ever make TLS
  ambiguous again.
- 27da837: Evicted an end-of-life TLS stack from the daemon. librespot's proxy
  support (hyper-proxy2) and the MQTT client (rumqttc) both dragged
  rustls 0.22 / rustls-webpki 0.102 into the build - a line that stopped
  receiving fixes and had collected four advisories (dependabot 22-25,
  including a high-severity CRL parsing panic). MQTT speaks plaintext to
  a LAN broker, so its TLS feature is simply gone; hyper-proxy2 is
  pinned to the upstream commit that moved to the maintained rustls 0.23
  line until a release ships. Every TLS connection the box makes now
  goes through one current rustls.
- e052551: Every screensaver now shows the battery level - a drifting glyph and
  percentage near the bottom, so a glance at the shelf answers "does it
  need charging?" without waking the panel. Boxes without battery
  telemetry show nothing, as before.
- 82fe517: The Speaker name field (and the Home Assistant broker fields) no
  longer show up empty on first load. They captured their initial value
  before the live connection delivered the settings, and only a
  remount refreshed them; untouched fields now always show the server's
  value the moment it arrives.
- 26f1cff: Display rotation is now declared exactly once - in the box profile's
  device tree (`dtparam=rotate=`). The panel UI and the game launcher
  read the kernel's DRM panel-orientation hint instead of carrying
  their own copies, the boot console rotates to match (sideways kernel
  panics are finally readable), and `SLINT_KMS_ROTATION` in the env
  profile becomes an optional override rather than a requirement.
- d1a2c1a: The games share is now actually openable from the Finder sidebar. Two
  bugs conspired: macOS's SMB client fails session setup ("server
  rejected the authentication") whenever the advertised DNS-SD instance
  name contains any character outside the Basic Multilingual Plane -
  emoji, in practice - so "George's 🔊" could be seen but never opened;
  meanwhile smbd was quietly registering its own duplicate advert under
  the machine hostname (BOOMPI-XXXX), which is the entry that _did_
  work and hid the breakage. Rather than losing the personality, the
  advert now translates emoji to classic Unicode stand-ins Apple's
  client can stomach: "George's 🔊" appears as "George's ♪", hearts of
  any color become ♥, 🌟 becomes ★, flags become their letters
  (🇺🇸 → US), and anything without a decent BMP twin is dropped. The
  bug boundary and the substitutes were confirmed experimentally
  against Apple's client, and smbd's duplicate registration is disabled
  - Finder shows exactly one entry, named after the speaker, that
    connects.
- 65fc1a0: The games share now shows up in the Finder sidebar under the
  speaker's name - emoji and all - instead of the machine hostname.
  Renaming the speaker updates the network advert within seconds.
- 2c0161d: The 8BitDo Mod Kit for original SNES controllers works out of the box
  now (D-input mode - hold B while powering on). The upstream autoconfig
  pack covers the M30/N30/P30/S30 Mod Kits but skipped the SN30 one, so
  the image carries the missing profile, mapped and verified on real
  hardware.
- d5b676b: The battery estimator no longer gets stuck below 100% when the
  charger's top voltage drops. It already noticed the lower plateau and
  waited for several full charge cycles to confirm it (guarding against
  one anomalous session) - but a box that lives on its charger may not
  produce cycles for weeks, so the display sat at 88% indefinitely.
  Resting pinned at the candidate plateau with no current flowing now
  counts as confirmation too: about half a day of quiet sitting adopts
  the new full voltage and re-baselines the gauge. The old behavior
  remains for boxes that do cycle - and if your charge voltage dropped
  without you touching the charger, do check the charger and the pack's
  cell balance; the gauge adapting doesn't make the electrons come back.
- 720a728: Settings copy catches up to the two-track mixer: the web Games card and
  the iOS game-volume footer no longer claim music ducks the game -
  music and gameplay are separate tracks, each with its own volume.
- c0a6419: Update-check failures explain themselves now. "error sending request
  for url (...)" told you nothing - was it DNS, TLS, a timeout, a rate
  limit? Errors shown in the settings UIs now carry the HTTP status
  when a response arrived ("HTTP 502 from ...") and the full cause
  chain when one didn't ("request to ... failed: ... dns error: failed
  to lookup address"), so a failed check reads like a diagnosis instead
  of a shrug.
- ec1a6ea: Uptime moves out of the web sidebar header into General → System,
  alongside CPU temperature and the restart control. The sidebar
  header now shows just the version.
- cefdf98: The spectrum bars now render in the album art's secondary palette
  color (the sliders keep the primary), so they no longer drown out the
  volume and playback tracks - and their opacity is adjustable in the
  web settings. The bars are also truly volume-independent now: music
  mixes through a dedicated pre-volume bus that the visualizer taps
  directly, instead of mathematically undoing the volume from the
  attenuated signal (which fell apart at low volumes).
- 4b2f1fe: Volume can never jump suddenly again. Upward volume changes now ramp
  smoothly (about 15% per second - downward stays instant), the speaker
  never wakes louder than 70% regardless of what was persisted, and if
  a music stream ever escapes the volume-controlled mixing bus it gets
  the music volume applied directly instead of playing at full level.
- 94a2b16: IP addresses on the Wi-Fi pages lost their /24 tail - the CIDR prefix
  length meant nothing to anyone reading a settings screen.
- b8dc61c: Saved-but-disconnected Wi-Fi networks now have an explicit Rejoin
  button. Opening or polling the Wi-Fi page also no longer silently
  undoes a deliberate Disconnect: NetworkManager can clear its own
  autoconnect block as a side effect of scanning, so boompid records
  the user's intent in /run and reasserts it after scans. Deliberate
  Rejoin/connect, radio, hotspot, and forget actions clear the latch;
  a reboot clears it naturally.

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
