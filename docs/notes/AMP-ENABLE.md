# Amp-enable GPIO - hardware notes (pre-implementation)

Temporary working notes for the amp power-sequencing feature. Live
pin surveys taken from both boxes (`/sys/kernel/debug/pinctrl/*/
pinmux-pins`). Delete this file once the feature ships and the
profiles/docs carry the real configuration.

Goals: kill the power-on pop, kill the idle hiss (amp alive only
while playing), and stop the amp burning current in the latch's
half-on state after software poweroff.

## Pin availability survey

### Pi 4 box (Raspiaudio DAC HAT): wide open

In use: BCM 2/3 (I2C1, INA260), BCM 18-21 (I2S, DAC). Everything
else free.

**Chosen candidate: BCM 22 (physical pin 15).**
- BCM 9-27 power up with default pull-DOWN: low from the first
  microsecond, before any firmware runs. (BCM 0-8 power up pulled
  HIGH - instant amp-on at power, disqualified.)
- Avoids 0/1 (HAT EEPROM), 14/15 (UART), 9-13 (SPI/PWM).
- Profile addition: `gpio=22=op,dl` (firmware holds it low-as-output
  from boot stage onward).
- VERIFY BEFORE SOLDERING: pinmux cannot see passive wiring on the
  HAT. Continuity-check physical pin 15 against the HAT (expect
  open). Fallbacks if tied: BCM 17, 27, or 23-26 (all equivalent).

### Pi 3 box (HyperPixel 4.0): zero free header GPIOs

All 28 consumed: 0-9, 12-17, 20-25 DPI display data (alt2); 10/11
bit-banged I2C (GT911 touch + INA260, i2c-11); 19 backlight; 18/26/27
panel init-SPI - these three are software-idle after init but
PHYSICALLY WIRED to the panel: toggling them clocks garbage into the
display controller. GPIO 28/29 exist on the SoC but are not routed
anywhere solderable on a 3B.

**Chosen approach: MCP23008 I2C GPIO expander on the existing
bit-banged bus (i2c-11), where the INA260 already hangs.**
- MCP23008, NOT PCF8574: the MCP powers up with all pins high-Z
  inputs, so the gate pull-down keeps the amp off from power-on. The
  PCF8574 powers up driving HIGH - it would blast the amp on at boot,
  the exact pop this feature kills.
- Address 0x20 (A0-A2 grounded); no conflict with INA260 at 0x40.
- No device-tree work: boompid already speaks raw I2C on that bus
  (linux_embedded_hal::I2cdev, same as the INA260). Driving the MCP
  is two register writes: IODIR (0x00) to set pin 0 as output, OLAT
  (0x0A) to drive it. Userspace-only keeps the box profile pure
  config.

## Wiring

Shared output stage (both boxes) - N-channel MOSFET (e.g. 2N7000 for
an enable pin, or a logic-level power FET like IRLZ44N if switching
the amp supply low-side):

```
  enable source ----[ 1k ]----+---- gate  N-MOSFET
  (GPIO22 / MCP GP0)          |
                            [ 10k ]      drain ---- amp ENABLE/MUTE pin
                              |                     (or amp supply low side)
                             GND         source --- GND
```

The 10k gate pull-down is the actual pop-killer: it covers every
window software cannot - power-on ramp, firmware handoff, kernel
panic, and the latch half-on state after poweroff.

### Pi 4 box

```
  Pi header pin 15 (BCM 22) ----[ 1k ]----+---- gate
                                          |
                                        [ 10k ]
                                          |
                                         GND (pin 14 or any)
```

### Pi 3 box (via MCP23008 on the HyperPixel I2C breakout)

```
  HyperPixel breakout          MCP23008 (DIP-8/SOIC: pinout for DIP18? use MCP23008 18-pin)
  3.3V  ------------------ VDD (18)
  GND   ------------------ VSS (9), A0 (4), A1 (5), A2 (6)
  SDA (GPIO10 / i2c-11) --- SDA (2)
  SCL (GPIO11 / i2c-11) --- SCL (1)
  3.3V --[ 10k ]----------- RESET (7)   (must be held high)

  GP0 (10) ----[ 1k ]----+---- gate (output stage above)
                         |
                       [ 10k ]
                         |
                        GND
```

Note: the INA260 already lives on this bus/breakout - the MCP shares
SDA/SCL/3V3/GND with it.

## Software sketch (next arc)

- `hardware.toml` grows `[amp]`:
  - Pi 4: `gpio = 22`
  - Pi 3: `expander_bus = 11`, `expander_addr = 0x20`, `pin = 0`
- Pi 4 profile config.txt fragment: `gpio=22=op,dl`
- boompid amp module: enable on playback start, hold a few seconds
  through track gaps, drop on idle / deep screensaver idle /
  low-battery poweroff / shutdown (the safeguard's poweroff path is
  the natural hook).
- Bench validation: scope or ear-test pop at power-on, boot, first
  play, idle drop, poweroff.
