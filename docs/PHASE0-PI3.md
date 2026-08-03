# Phase 0 runbook — Pi 3 box (HyperPixel 4.0)

Validation spikes from `docs/PLAN.md`, as concrete commands. Run them on the
actual Pi 3 boombox so the real display, touch, INA260, and speakers are in
the loop.

> **Use a spare SD card.** The v1 card stays untouched (backup:
> `~/boompi-pi3-v1-backup.img.gz`). Everything below happens on a fresh
> Raspberry Pi OS **Lite 64-bit (Bookworm)** install on a different card.

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

Bookworm's config lives at `/boot/firmware/config.txt`. Append:

```
dtoverlay=vc4-kms-dpi-hyperpixel4
dtparam=rotate=270,touchscreen-swapped-x-y,touchscreen-inverted-x
```

`sudo reboot`. The Linux console should appear on the HyperPixel, in
landscape (header at top).

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

Rust on the Pi (one-time, slow-ish):

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env
sudo apt install -y git pkg-config clang libinput-dev libudev-dev libgbm-dev libdrm-dev
```

The Pi 3 has 1 GB RAM — bump swap before the first build:

```sh
sudo sed -i 's/CONF_SWAPSIZE=.*/CONF_SWAPSIZE=1024/' /etc/dphys-swapfile
sudo systemctl restart dphys-swapfile
```

Get the code and build (first build may take a long while; limit jobs to
avoid OOM):

```sh
git clone -b v2 https://github.com/TooTallNate/boompi.git
cd boompi/rust
CARGO_BUILD_JOBS=2 cargo build --release -p kms-test --no-default-features --features kms
```

Run it. The linuxkms backend needs direct access to `/dev/dri` and
`/dev/input` — simplest is root, and it must own the display (fine on Lite,
nothing else does):

```sh
sudo ./target/release/kms-test
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
      suspends/resumes. Then disable suspend (WirePlumber 0.4 / Lua config
      on Bookworm) and retest:
  ```sh
  mkdir -p ~/.config/wireplumber/main.lua.d
  cat > ~/.config/wireplumber/main.lua.d/51-disable-suspend.lua <<'EOF'
  table.insert(alsa_monitor.rules, {
    matches = { { { "node.name", "matches", "alsa_output.*" } } },
    apply_properties = { ["session.suspend-timeout-seconds"] = 0 },
  })
  EOF
  systemctl --user restart wireplumber
  ```

**Go/no-go**: streaming + metadata + monitor capture all working = PipeWire
architecture confirmed (and the `ffplay /dev/zero` hack is officially dead).

## 3. Spike C — AVRCP cover art (needs recent BlueZ)

Bookworm ships BlueZ 5.66; cover art needs ≥ ~5.79 with `--experimental`.
Build from source (~20–40 min on the Pi 3):

```sh
sudo apt install -y libglib2.0-dev libdbus-1-dev libudev-dev libical-dev libreadline-dev
wget https://www.kernel.org/pub/linux/bluetooth/bluez-5.79.tar.xz
tar xf bluez-5.79.tar.xz && cd bluez-5.79
./configure --prefix=/usr --sysconfdir=/etc --localstatedir=/var \
    --enable-experimental --disable-manpages --disable-mesh
make -j2
```

Run the freshly built daemons (don't install; keep the system clean):

```sh
sudo systemctl stop bluetooth
sudo ./src/bluetoothd -E -n -d 2>/tmp/bluetoothd.log &   # -E = experimental

systemctl --user stop obex 2>/dev/null                    # if present
./obexd/src/obexd -n -d 2>/tmp/obexd.log &                # session-bus daemon, run as pi
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
| A: Slint KMS render | | renderer (sw/GL), rotation method, CPU % |
| A: touch mapping | | |
| B: A2DP + metadata | | |
| B: monitor capture | | |
| B: crackle fix | | |
| C: cover art (iPhone) | | ObexPort / ImgHandle / image |
| C: cover art (Android) | | |
| D: INA260 | | bus number |

When filled in, update `docs/PLAN.md` (risks + open items) accordingly.
