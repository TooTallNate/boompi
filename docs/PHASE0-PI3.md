# Phase 0 runbook — Pi 3 box (HyperPixel 4.0)

Validation spikes from `docs/PLAN.md`, as concrete commands. Run them on the
actual Pi 3 boombox so the real display, touch, INA260, and speakers are in
the loop.

> **Use a spare SD card.** The v1 card stays untouched (backup:
> `~/boompi-pi3-v1-backup.img.gz`). Everything below happens on a fresh
> Raspberry Pi OS **Lite 64-bit** install (current release; verified with
> the 2026-06-18 image = Debian 13 "Trixie", BlueZ 5.82) on a different card.

> ⚠️ **Power**: first boot is write-heavy and a sagging 5V rail corrupts the
> SD filesystem (learned the hard way — a deeply discharged boombox pack
> browned out mid-first-boot: intermittent no-boot + EXT4 journal aborts).
> Use a solid supply (bench/wall 5V ≥2.5A) or a charged pack, and check
> `vcgencmd get_throttled` after boot (`0x0` = clean; `0x5000x` bits =
> undervoltage happened).

Facts recovered from the v1 card that these spikes rely on:

| Fact | Value |
|---|---|
| Display overlay | `dtoverlay=vc4-kms-dpi-hyperpixel4` |
| Rotation params | `dtparam=rotate=270,touchscreen-swapped-x-y,touchscreen-inverted-x` |
| Touch controller | Goodix GT911 @ 0x14/0x5d on the overlay's i2c-gpio bus |
| INA260 | 0x40 on the same bus — `/dev/i2c-11` |
| Audio out | USB audio dongle |

## 0. Base install

1. Raspberry Pi Imager → Raspberry Pi OS **Lite (64-bit)**. In the imager's
   settings: hostname `boompi-dev`, enable SSH, user `pi`, your Wi-Fi creds.
2. Boot (headless — the panel shows nothing until step 1), `ssh pi@boompi-dev.local`.
3. `sudo apt update && sudo apt full-upgrade -y && sudo reboot`

Record: `uname -a`, `cat /etc/os-release | head -2`, `bluetoothctl --version`.

## 1. Spike A — display + touch (Slint on KMS)

### 1a. Enable the panel

The config lives at `/boot/firmware/config.txt` (editable from a laptop by
mounting the card's `bootfs` FAT partition — handy since the panel is dark
until this is done). Append:

```
dtoverlay=vc4-kms-dpi-hyperpixel4
dtparam=rotate=270,touchscreen-swapped-x-y,touchscreen-inverted-x
```

`sudo reboot`. The Linux console should appear on the HyperPixel, in
landscape (header at top). ✔ *Verified working on the 2026-06-18 Trixie
image — console renders rotated on the panel.*

### 1b. Verify KMS + touch at the OS level

```sh
ls /dev/dri                       # expect card0 (+ card1) and renderD128
sudo apt install -y libdrm-tests libinput-tools
modetest -M vc4 -c | head -30     # expect a connected DPI-1 connector, 480x800 mode
dmesg | grep -i -e goodix -e i2c  # expect Goodix-TS 11-0014 (or 11-005d) → bus 11
sudo libinput debug-events        # touch the panel; expect TOUCH_DOWN/MOTION events
```

If `modetest` shows the connector and libinput sees touches, the hardware
layer is good regardless of what Slint does next.

### 1c. Build and run kms-test

**Cross-compile from the Mac** — building on the Pi 3 doesn't work: rustup's
rustc segfaults deterministically on Trixie/Pi 3 (SIGSEGV in
`Symbol::intern`, not fixed by `RUST_MIN_STACK`), and even if it worked,
1 GB RAM makes Slint builds miserable.

On the **Pi** (one-time): install the C libraries Slint links against, so
they can be pulled into the cross sysroot:

```sh
sudo apt install -y libinput-dev libudev-dev libxkbcommon-dev libfontconfig1-dev libdrm-dev libgbm-dev
```

On the **Mac** (one-time):

```sh
brew install zig cargo-zigbuild
rustup target add aarch64-unknown-linux-gnu
ssh-copy-id pi@boompi-dev.local          # so rsync/scp don't prompt
make sysroot PI=pi@boompi-dev.local      # pulls /usr/include + libs from the Pi
```

Build + ship:

```sh
make cross-kms-test        # or cross-kms-test-gl for the GLES variant
scp rust/target/aarch64-unknown-linux-gnu/release/kms-test pi@boompi-dev.local:
```

Run it on the Pi. The linuxkms backend needs direct access to `/dev/dri`
and `/dev/input` — simplest is root, and it must own the display (fine on
Lite, nothing else does):

```sh
sudo ./kms-test
```

### 1d. What to check + record

- [ ] App renders fullscreen; resolution readout says **800 × 480**
      (landscape). If it says 480 × 800 or appears sideways, the panel
      orientation hint isn't honored — retry with
      `sudo SLINT_KMS_ROTATION=90 ./target/release/kms-test`
      (try 180/270 too) and record which value fixes it.
- [ ] TL/TR/BL/BR labels are in the expected corners (header at top).
- [ ] The sweeping bar animates smoothly (no tearing/stutter). Note CPU:
      `top` in a second SSH session.
- [ ] Crosshair tracks your finger accurately in all four corners, taps
      increment. If axes are swapped/mirrored, record exactly how (this
      tells us whether the dtparam touch transforms apply below libinput
      or need handling in our stack).
- [ ] Optional: build `--features kms-gl` and compare smoothness + CPU
      (GLES via vc4 vs software renderer).

**Go/no-go**: software renderer at 800×480 with working touch = green light
for the Slint frontend plan. Sideways-with-no-workaround or broken touch =
escalate before Phase 2.

## 2. Spike B — headless Bluetooth A2DP sink via PipeWire

USB audio dongle plugged in, speakers on.

```sh
sudo apt install -y pipewire pipewire-pulse wireplumber libspa-0.2-bluetooth
sudo loginctl enable-linger pi        # user session (pipewire) runs at boot
systemctl --user status pipewire wireplumber   # both active
wpctl status                          # ALSA section lists the USB sink
```

Pair a phone (spike-grade agent — auto-accept, like v1):

```sh
bluetoothctl
  power on
  agent NoInputNoOutput
  default-agent
  discoverable on
  pairable on
  # pair from the phone now; then:
  trust <PHONE_MAC>
  quit
```

Play music on the phone. Checks:

- [ ] Audio comes out the speakers; `wpctl status` shows a `bluez_input`
      stream routed to the USB sink.
- [ ] Volume: `wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.3` works live.
- [ ] Metadata over D-Bus:
  ```sh
  busctl --system tree org.bluez | grep player
  busctl --system get-property org.bluez \
      /org/bluez/hci0/dev_XX_XX_XX_XX_XX_XX/player0 \
      org.bluez.MediaPlayer1 Track
  ```
  Expect Title/Artist/Album/Duration.
- [ ] Absolute volume: change volume on the phone; watch
  `busctl --system monitor org.bluez` for `MediaTransport1.Volume` changes.
- [ ] Visualizer feed — capture the sink monitor while music plays:
  ```sh
  parec --device="$(pactl get-default-sink).monitor" --raw | head -c 200000 > /tmp/cap.raw
  xxd /tmp/cap.raw | grep -v "0000 0000 0000 0000" | head -3   # non-silence
  ```
- [ ] Crackle test: pause music ~30 s, resume — listen for pops as the sink
      suspends/resumes. Then disable suspend (WirePlumber ≥0.5 `.conf`
      format on Trixie) and retest:
  ```sh
  mkdir -p ~/.config/wireplumber/wireplumber.conf.d
  cat > ~/.config/wireplumber/wireplumber.conf.d/51-disable-suspend.conf <<'EOF'
  monitor.alsa.rules = [
    {
      matches = [ { node.name = "~alsa_output.*" } ]
      actions = { update-props = { session.suspend-timeout-seconds = 0 } }
    }
  ]
  EOF
  systemctl --user restart wireplumber
  ```

**Go/no-go**: streaming + metadata + monitor capture all working = PipeWire
architecture confirmed (and the `ffplay /dev/zero` hack is officially dead).

## 3. Spike C — AVRCP cover art

Cover art needs BlueZ ≥ ~5.79. **Trixie ships 5.82 — no source build
needed.** Two pieces:

1. `bluetoothd` must run with experimental D-Bus interfaces (that's what
   gates `MediaPlayer1.ObexPort` and `Track.ImgHandle`):

   ```sh
   # set Experimental = true under [General] in /etc/bluetooth/main.conf
   sudo sed -i 's/^#\?\s*Experimental *=.*/Experimental = true/' /etc/bluetooth/main.conf
   grep -n "Experimental" /etc/bluetooth/main.conf   # verify exactly one, = true
   sudo systemctl restart bluetooth
   ```

2. `obexd` (the OBEX/BIP client) — separate package, runs on the user
   session bus via D-Bus activation, needs **no flag** (the BIP client is a
   compiled-in plugin):

   ```sh
   sudo apt install -y bluez-obexd
   busctl --user introspect org.bluez.obex /org/bluez/obex   # activates it; expect Client1
   ```

Reconnect the phone (re-pair via `bluetoothctl` if needed), play music, then:

```sh
PLAYER=$(busctl --system tree org.bluez | grep -o '/org/bluez/hci0/dev_[A-F0-9_]*/player[0-9]*' | head -1)

# 1. Does the phone offer cover art? (property only exists if yes)
busctl --system get-property org.bluez $PLAYER org.bluez.MediaPlayer1 ObexPort

# 2. Grab the current track's image handle
busctl --system get-property org.bluez $PLAYER org.bluez.MediaPlayer1 Track
#    → look for "ImgHandle" (e.g. "1000001")

# 3. BIP OBEX session to the phone (PSM = the ObexPort value)
PHONE=XX:XX:XX:XX:XX:XX   # colon form
busctl --user call org.bluez.obex /org/bluez/obex org.bluez.obex.Client1 \
    CreateSession "sa{sv}" $PHONE 2 Target s bip-avrcp PSM q <OBEX_PORT>
#    → returns a session object path

# 4. Pull the thumbnail
busctl --user call org.bluez.obex <SESSION_PATH> org.bluez.obex.Image1 \
    GetThumbnail "ss" /tmp/cover.jpg <IMG_HANDLE>
sleep 2 && file /tmp/cover.jpg   # expect JPEG image data
```

Record per phone (iPhone / Android): ObexPort present? ImgHandle present?
Image retrieved? Notes in the results table below.

> Fallback if `ObexPort`/`ImgHandle` never appear despite
> `Experimental = true` and an iPhone sender: build BlueZ master from
> source (instructions were in this file's git history) — but with 5.82
> packaged this should not be needed.

**Fallback if no cover art**: the online-lookup fallback (Settings-gated)
covers these senders — this spike determines how often it will be needed.

## 4. Spike D — INA260 sanity (5 minutes)

```sh
sudo apt install -y i2c-tools
sudo modprobe i2c-dev
i2cdetect -l                      # expect an "i2c@0" / i2c-gpio adapter, likely i2c-11
sudo i2cdetect -y 11              # expect 0x14 (or 0x5d) = GT911, 0x40 = INA260
sudo i2cget -y 11 0x40 0xfe w     # manufacturer ID; 0x4954 byte-swapped = "TI" (0x5449)
sudo i2cget -y 11 0x40 0x02 w     # bus voltage, byte-swapped BE word; swap bytes × 1.25 mV
```

Example: `0x2f4d` → swap → `0x4d2f` = 19759 × 1.25 mV ≈ 24.7 V.

- [ ] Adapter present and numbered as expected (record the number!)
- [ ] INA260 answers at 0x40, manufacturer ID correct, plausible voltage

## 5. Results

| Spike | Result | Notes / versions |
|---|---|---|
| A: console on panel | ✔ pass | Trixie 2026-06-18: console renders rotated on HyperPixel |
| A: touch driver | ✔ pass | GT911 at `11-005d` (ID 911 v1060), input device registered; 0x14 probe fail is normal (dual-address overlay) |
| A: Slint KMS render | ✔ pass | Software renderer, smooth at 800×480. Panel-orientation hint NOT auto-applied: bare run renders portrait 480×800; `SLINT_KMS_ROTATION=270` is correct (90 = upside down). Cross-compiled from macOS (zig), ~25 s builds. |
| A: touch mapping | ✔ pass | Perfect with DT touch transforms (`touchscreen-swapped-x-y,touchscreen-inverted-x`) + `SLINT_KMS_ROTATION=270` — no double-transform. **Image recipe: keep v1 config.txt lines + set `SLINT_KMS_ROTATION=270` in the UI service env.** |
| A: emoji in user text | ✔ pass | UI chrome uses drawn icons (no fonts). User content emoji: **Skia OpenGL renderer + Noto Color Emoji + fontconfig alias `emoji → "Noto Color Emoji"`** renders full color. (Software renderer is outline-only: color CBDT fonts = silent empty, monochrome Noto Emoji = outline glyphs — kept as fallback recipe.) Slint embeds Inter for regular text. |
| A: Skia OpenGL on KMS | ✔ pass | "Using Skia OpenGL renderer" on Pi 3 (vc4 GLES via EGL/GBM, dlopen'd — mesa runtime required: `libegl1 libegl-mesa0 libgles2 libgl1-mesa-dri`). Rotation via `SLINT_KMS_ROTATION=270` works; also falls back to Skia CPU raster when GL is absent. **Pi UI renderer of record.** |
| B: A2DP + metadata | ✔ pass | iPhone streams to USB sink; track metadata + bidirectional AVRCP absolute volume verified **through the real boompid BlueZ source** (not just busctl). See gotchas below — four independent blockers. |
| B: monitor capture | | |
| B: crackle fix | | |
| C: cover art (iPhone) | ✔ **pass** | Real 200×200 JPEGs retrieved from the iPhone (Spotify playing) via ObexPort 4105 + `Image1.GetThumbnail`. Validated with BlueZ's in-tree `mpris-proxy` (5.81+ implements this exact flow). **Phase 3 implementation rules learned:** (1) the OBEX session dies with its creating D-Bus connection — must be held by a persistent connection (boompid's zbus conn qualifies; one-shot `busctl` can never work); (2) iOS allows **one** BIP channel — competing clients get L2CAP `refused: no resources (0x0004)`, and Debian ships `mpris-proxy` *enabled by default* (must be absent/disabled wherever boompid runs); (3) `Track.ImgHandle` is only included while a BIP session is alive — connect the session when the player appears, handles arrive on subsequent track changes; (4) iOS has no browsing channel; control-channel `GetElementAttributes` is the metadata path; (5) works fine with `sc off` on the clone dongle — no Secure Connections requirement. |
| C: cover art (Android) | | |
| D: INA260 | ✔ pass | `i2c-11` on Trixie (same as v1); 0x40 responds, touch shows `UU` at 0x5d on same bus |

Environment: Debian 13 (Trixie), BlueZ 5.82, `throttled=0x0` on wall power
(the boombox pack is deeply discharged and browned out the Pi — do not run
spikes from the pack until it's charged/inspected).

## Bluetooth gotchas (all bake into the Buildroot image)

1. **Image ships Bluetooth rfkill-blocked**: `/var/lib/systemd/rfkill/*`
   state files are pre-seeded `1` at image build time. Unblock live
   (`/sys/class/rfkill/*/soft`) *and* fix the state file, or it re-blocks
   on reboot. Our image: ship state files as `0`.
2. **WirePlumber gates Bluetooth on an active logind seat** — headless/
   SSH/linger sessions have none, so the bluez monitor loads but never
   registers A2DP endpoints (adapter Class stays non-audio → invisible to
   iPhones, which filter by class). Fix (rootfs overlay):
   `wireplumber.conf.d` fragment setting profile `main`
   `monitor.bluez.seat-monitoring = disabled`.
3. **Discoverable times out after 3 min by default** — set
   `DiscoverableTimeout u 0` (boompid owns discoverable state in Phase 3).
4. **The CSR-clone USB dongle (`00:1A:7D:...`) firmware hard-locks on
   Secure Connections pairing with iOS** (`hardware error 0x00`,
   controller stops accepting commands; USB reset required to recover).
   Fix: `btmgmt sc off` (command is `sc`, not `secure-conn`). Historically
   reliable dongle otherwise; Pi 3 onboard BT causes audio stutter (shared
   antenna/UART) and stays disabled via `dtoverlay=disable-bt`.
5. Pairing confirmations need an agent: bluez-tools `bt-agent -c
   NoInputNoOutput` as a user service (v1 parity) until boompid's `Agent1`
   with on-screen confirm lands in Phase 3.
6. Bonus: installing `bluez-tools` pulls in `bluez-obexd` → Spike C's
   dependency is already present.

When filled in, update `docs/PLAN.md` (risks + open items) accordingly.
