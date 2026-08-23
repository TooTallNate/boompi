# Boompi Mainboard Rev A

## Purpose

This document codifies the current hardware game plan for Boompi Mainboard Rev A before schematic capture begins.

The goal of Rev A is to turn the existing hand-built Boompi electronics into a clean, repeatable hardware platform while keeping the highest-risk subsystems modular where appropriate.

Rev A should replace the current collection of Raspberry Pi boards, HATs, USB audio devices, sensor breakouts, fan wiring, improvised power wiring, and adapter boards with a purpose-built carrier/mainboard.

It should remain flexible enough to support multiple Boompi enclosure sizes and display sizes.

---

# 1. Rev A design philosophy

Rev A should integrate the parts that benefit most from a custom PCB:

- Compute Module carrier
- I2S DAC
- USB hub
- battery/current telemetry
- always-on management MCU
- fan control
- temperature sensing
- power-button handling
- main 5 V regulation
- Ethernet
- MIPI DSI display interface
- amplifier enable/reset control
- debug and expansion headers

Rev A should keep these modular for now:

- stereo power amplifier module
- subwoofer amplifier module
- battery pack
- battery BMS
- USB-C PD bidirectional charger/source board

The USB-C PD subsystem is a generic but relatively high-risk power-electronics problem, so it should be implemented from a proven reference architecture rather than invented as part of the first carrier-board spin.

---

# 2. Locked component and architecture decisions

## Compute Module

**Baseline:** Raspberry Pi Compute Module 4

- CM4 is the primary target for Rev A.
- CM5 compatibility should be preserved where practical, but not at the expense of a reliable CM4 design.
- Prefer eMMC-capable CM4 variants for finished Boompi units.
- Use onboard Wi-Fi.
- Do not rely on onboard Bluetooth for Boompi audio.

## Bluetooth

**Official Rev A Bluetooth radio:** TP-Link UB500

Reasoning:

- Existing Boompi builds have experienced poor Bluetooth audio behavior when using the Raspberry Pi onboard Bluetooth simultaneously with Wi-Fi.
- Bluetooth is a core Boompi feature.
- The UB500 is already known to work reliably in the current systems.

The mainboard should provide an internal USB connection suitable for the UB500.

Preferred implementation:

- internal USB-A receptacle
- UB500 plugs directly into the board

## USB hub

**Microchip USB2514B**

- 4-port USB 2.0 Hi-Speed hub
- mature embedded-Linux part
- no proprietary driver
- suitable for internal USB fanout

Proposed allocation:

```text
CM4 USB 2.0
     |
     v
USB2514B
     |
     +-- Port 1 -> internal USB-A -> TP-Link UB500
     +-- Port 2 -> external USB-C data path
     +-- Port 3 -> internal/service USB header
     +-- Port 4 -> spare / future accessory
```

The USB-C PD circuitry and USB data topology should remain logically separate.

## Audio DAC

**Texas Instruments PCM5122**

Reasons:

- I2S input
- I2C/SPI control
- no external MCLK required
- mature Raspberry Pi/Linux support
- ground-centered line outputs
- integrated volume/mute/filtering capabilities
- future EQ/profile support
- suitable for enclosure-specific audio tuning

Proposed connection:

```text
CM4 I2S ----> PCM5122 ----> stereo line output ----> external amplifier
      |
      +---- I2C control
```

The custom audio section should be laid out carefully enough that the current inline ground-loop isolator is no longer necessary.

## Main 5 V regulator

**Analog Devices LT8645S**

Target:

- approximately 5.1 V output
- up to 8 A
- powered directly from the 6S battery/system bus

Reasons:

- current Adafruit MP2307 UBEC is inadequate
- current builds report undervoltage events, especially during boot
- 6S battery voltage reaches approximately 25.2 V
- CM4 + display + USB + fans can produce significant transient load
- LT8645S offers wide input range, high current capacity, strong transient behavior, and low EMI

The LT8645S should live on the mainboard, close enough to the CM4 5 V plane to control voltage drop and transient response.

## Always-on 3.3 V regulator

**Analog Devices LT8609S**

Purpose:

- supply a low-power always-on domain
- power the RP2040
- power INA260 logic
- support power-button and telemetry behavior while the main 5 V rail is off

Target topology:

```text
6S battery/system bus
        |
        v
     LT8609S
        |
     3.3V_AON
        |
        +-- RP2040
        +-- INA260 logic
        +-- low-power temperature/power-button logic
```

## Management MCU

**RP2040**

The RP2040 acts as Boompi's hardware supervisor.

Responsibilities:

- momentary power-button handling
- graceful shutdown coordination
- forced shutdown on long hold
- storage-off latch control
- LT8645S main-rail enable
- amplifier enable/reset control
- fan PWM
- fan tachometer measurement
- enclosure temperature monitoring
- battery telemetry access
- watchdog/heartbeat supervision
- status LEDs
- future hardware controls

Communication with Linux/`boompid` should use I2C.

## Battery current/voltage monitor

**Texas Instruments INA260**

The INA260 should remain on the battery side of the system so Boompi can measure:

- total system current
- total battery-side power
- charging current
- discharging current
- charge/discharge direction

Placement:

```text
BATTERY+
   |
   v
INA260 internal shunt
   |
   v
SYSTEM_BAT+
   |
   +-- LT8645S -> 5.1 V main rail
   +-- stereo amplifier
   +-- subwoofer amplifier
   +-- USB-C PD / charger
   +-- LT8609S AON regulator
```

The charger must connect on the **system side** of the INA260 so charging current flows through the INA260 in the reverse direction.

The INA260 logic supply comes from `3V3_AON`.

At current Boompi scale, INA260's 15 A continuous rating is acceptable.

## Display interface

**Standard Raspberry Pi-style MIPI DSI over FFC + separate 5 V power**

Boompi should not be locked to one physical display size or resolution.

Reasons:

- `boompi-ui` already handles dynamic resolutions
- DSI is mechanically cleaner than separate HDMI + USB touch cables
- display size should be an enclosure choice, not a mainboard choice

Baseline supported display family:

- official Raspberry Pi Touch Display 2
- compatible third-party DSI displays such as the 4.3-inch OSOYOO panel
- future supported DSI displays via hardware profile/device-tree configuration

Requirements:

- Raspberry Pi-standard 22-pin FFC where practical
- separate keyed 5 V display power connector
- no custom Boompi-specific display cable in Rev A
- preserve 2-lane / 4-lane DSI flexibility where practical

## Ethernet

**Gigabit Ethernet from CM4 to an RJ45 on the mainboard edge**

Ethernet is primarily for:

- debugging
- emergency rescue
- development

Some enclosures may expose the RJ45. Smaller variants may leave it internally inaccessible.

No separate Ethernet daughterboard is required for Rev A unless mechanical constraints later force one.

## Temperature sensing

**TI TMP1075-class I2C sensor**

Initial plan:

- one onboard enclosure/ambient sensor
- optional second remote sensor/header for battery-compartment temperature

CM4 CPU temperature remains available through Linux and should be reported by `boompid` to the RP2040 for thermal control.

## Fans

**Standard 4-wire 5 V PWM fan interface**

Minimum:

- 2 populated fan headers
- optional third fan footprint

Each channel:

```text
1  GND
2  +5V
3  TACH
4  PWM
```

RP2040 handles PWM generation and tach counting.

PWM control should use proper open-drain/open-collector-style interface circuitry rather than driving the fan control input directly.

## Amplifier family

### Large / standard Boompi

**Preferred silicon: TI TPA3221**

Reasons:

- 7-30 V supply range fits 6S battery operation directly
- efficient Class-D architecture
- supports stereo BTL operation
- supports mono/PBTL operation
- appropriate for subwoofer use
- proper shutdown/reset behavior
- avoids needing high-current external power switching for normal mute/off

For large Boompi variants:

```text
TPA3221 stereo
   -> main left/right speakers

TPA3221 PBTL
   -> subwoofer
```

A dual-4-ohm subwoofer can be wired in parallel for a 2-ohm mono load where appropriate.

### Mini Boompi

**Possible alternate silicon: TI TPA3116D2**

This may be more appropriate for smaller/cheaper builds.

### Amplifier module requirement

Prefer modules exposing:

```text
BAT+
GND
LINE input
ENABLE / RESET / STANDBY

Preferred:
FAULT
OTW / thermal warning
```

Avoid modules that expose only power and audio with no enable/reset capability.

## USB-C PD / charging

**Preferred architecture: TI TPS25751D + BQ25756**

Use TI's bidirectional USB-C PD / battery-charge reference architecture as the baseline.

Desired capabilities:

- USB-C PD sink
- USB-C PD source
- bidirectional power
- 6S battery charging
- approximately 65-100 W class operation
- USB data role coordination

This should remain a separate board for Rev A so the carrier/mainboard can be validated independently.

---

# 3. Power-state architecture

Boompi should use a momentary power button rather than the current inline latching master switch.

Desired states:

## ON

- CM4 powered
- display powered
- USB powered
- fans available
- amplifiers enabled as needed
- RP2040 active
- INA260 active

## SOFT OFF

- main 5 V rail disabled
- CM4/display/USB/fans off
- amplifiers disabled
- RP2040 remains on the AON rail
- INA260 can remain available
- button can wake the system

## STORAGE OFF

- RP2040 drops the AON power latch
- LT8609S shuts down
- RP2040 loses power
- INA260 logic loses power
- only BMS and minimal passive wake/leakage paths remain

Suggested button behavior:

```text
short press while OFF
-> power on

short press while ON
-> request graceful shutdown

long hold
-> force main system off

very long hold / explicit software command
-> storage-off
```

The exact timing thresholds can be defined later in firmware.

---

# 4. Proposed high-level power tree

```text
6S BATTERY
   |
   v
INA260
   |
   v
SYSTEM_BAT+
   |
   +----> LT8645S ----> 5.1V_MAIN
   |                      |
   |                      +-- CM4
   |                      +-- DSI display
   |                      +-- USB2514B
   |                      +-- UB500
   |                      +-- fans
   |
   +----> stereo amplifier module
   |
   +----> subwoofer amplifier module
   |
   +----> TPS25751D + BQ25756 USB-C PD board
   |
   +----> LT8609S ----> 3V3_AON
                          |
                          +-- RP2040
                          +-- INA260 logic
                          +-- TMP1075
                          +-- power-button logic
```

---

# 5. Audio startup/shutdown sequencing

Startup:

```text
momentary power button
        |
        v
RP2040 wakes / latches AON
        |
        v
RP2040 enables LT8645S
        |
        v
CM4 boots
        |
        v
boompid starts
        |
        v
PCM5122 initialized and muted
        |
        v
RP2040 releases amplifier RESET / ENABLE
        |
        v
PCM5122 soft-unmutes
```

Shutdown:

```text
power request
    |
    v
boompid soft-mutes PCM5122
    |
    v
RP2040 disables amplifier
    |
    v
Linux shuts down
    |
    v
RP2040 disables LT8645S
    |
    v
optional: remain SOFT OFF
or
drop AON latch for STORAGE OFF
```

This architecture is intended to reduce startup/shutdown pops and eliminate unnecessary amplifier idle power.

---

# 6. Thermal architecture

Thermal management is a functional requirement.

Existing Boompi units can approach or exceed CPU throttling temperatures during workloads such as N64 emulation.

Rev A should support:

- substantial CM4 heatsink
- deliberate enclosure airflow
- at least two 4-wire PWM fans
- CPU-temperature-driven fan policy
- enclosure temperature sensing
- fan RPM monitoring
- fail-safe fan behavior if Linux/`boompid` stops communicating

Example initial fan policy:

```text
< 50 C     off / minimum
50-60 C    20%
60-70 C    40%
70-75 C    65%
75-80 C    85%
> 80 C     100%
```

Add hysteresis.

The final thermal target is sustained heavy workloads without CPU throttling.

---

# 7. Proposed KiCad project structure

Repository layout:

```text
hardware/
└── mainboard-rev-a/
    ├── boompi-mainboard-rev-a.kicad_pro
    ├── boompi-mainboard-rev-a.kicad_sch
    ├── boompi-mainboard-rev-a.kicad_pcb
    ├── libraries/
    ├── datasheets/
    ├── reference/
    └── bom/
```

Top-level schematic arrangement:

```text
00_TOP
├── 01_BATTERY_AON
├── 02_CM4_CORE
├── 03_MAIN_5V
├── 04_USB
├── 05_AUDIO
├── 06_DSI
├── 07_ETHERNET
├── 08_THERMAL_FANS
└── 09_CONNECTORS_DEBUG
```

## 01_BATTERY_AON

Contains:

- battery connector
- input protection
- INA260
- system battery bus
- LT8609S AON regulator
- momentary button wake/latch
- RP2040
- RP2040 SWD / USB boot support
- `POWER_HOLD`
- `MAIN_5V_EN`
- `AMP_MAIN_EN`
- `AMP_SUB_EN`
- AON I2C
- temperature sensing

## 02_CM4_CORE

Contains:

- CM4 connectors
- 5 V / ground connections
- boot/eMMC support
- USB boot
- UART debug
- global enable / run signals
- interface nets exported to other sheets

Use Raspberry Pi CM4 IO Board reference design heavily.

## 03_MAIN_5V

Contains:

- LT8645S
- battery/system-bus input
- 5.1 V main rail
- bulk capacitance
- local CM4 power distribution
- display power output
- USB/fan power distribution
- `MAIN_5V_EN`

## 04_USB

Contains:

- USB2514B
- CM4 upstream USB
- internal UB500 port
- external USB-C data path
- internal/service USB header
- spare fourth downstream port
- ESD protection
- port power switching where appropriate

## 05_AUDIO

Contains:

- CM4 I2S
- PCM5122
- I2C control
- DAC power filtering
- output filter/network
- stereo line-level output
- mute/enable coordination
- amplifier connector(s)
- analog test points

## 06_DSI

Contains:

- CM4 DSI signals
- Raspberry Pi-standard FFC connector(s)
- 5 V display power connector
- optional control/I2C lines as required
- DSI-related ESD/layout requirements

## 07_ETHERNET

Contains:

- CM4 Gigabit Ethernet interface
- magnetics / magjack
- RJ45
- ESD protection
- status LEDs as appropriate

## 08_THERMAL_FANS

Contains:

- 2 populated 4-wire PWM fan headers
- optional third footprint
- PWM interface transistors
- tach pull-ups/conditioning
- additional temperature-sensor headers if needed

## 09_CONNECTORS_DEBUG

Contains:

- UART
- SWD
- I2C expansion
- SPI expansion
- spare GPIO
- test points
- service connectors
- amplifier enable/fault interfaces

---

# 8. Initial schematic net names

Suggested net naming:

```text
BAT_RAW+
SYSTEM_BAT+

+5V_MAIN
+3V3_AON

PWR_BUTTON
POWER_HOLD
MAIN_5V_EN

AMP_MAIN_EN
AMP_SUB_EN
AMP_MAIN_FAULT
AMP_SUB_FAULT

I2C_AON_SDA
I2C_AON_SCL

I2S_BCLK
I2S_LRCLK
I2S_DOUT

CM4_SHUTDOWN_REQ
CM4_POWER_STATE

FAN1_PWM
FAN1_TACH
FAN2_PWM
FAN2_TACH

USB_UP_DP
USB_UP_DM
```

---

# 9. Preliminary PCB target

Initial working board envelope:

**approximately 120 mm x 85 mm**

This is a development target, not a final enclosure constraint.

Expected practical range:

```text
optimistic      ~100 x 75 mm
likely          ~110 x 80 mm
comfortable     ~120 x 85 mm
very relaxed    ~130 x 90 mm
```

The CM4 itself is approximately 55 x 40 mm, so connector placement, heatsink clearance, power layout, USB, Ethernet, and analog isolation will dominate board size more than IC package dimensions.

## Preliminary board zoning

```text
┌──────────────────────────────────────────────┐
│                                              │
│  CM4 + heatsink       PCM5122 AUDIO          │
│  ┌──────────────┐     ┌──────────────┐       │
│  │              │     │ DAC / filter │ OUT   │
│  │    CM4       │     └──────────────┘       │
│  │              │                            │
│  └──────────────┘      RP2040 / sensors      │
│                                              │
│  USB2514B         internal USB-A / UB500      │
│                                              │
│  LT8645S      battery / amp / fan connectors │
│                                              │
│ [RJ45]       [DSI FFC]       [debug]         │
└──────────────────────────────────────────────┘
```

Guidelines:

- place CM4 toward an edge to support directed airflow
- reserve approximately 20-30 mm above CM4 for heatsink/airflow
- keep PCM5122 analog section physically distant from LT8645S, USB, Ethernet, fan switching, and amplifier power wiring
- route 5 V using wide pours/planes rather than narrow traces
- keep high-current battery and amplifier wiring near board edges
- preserve easy access to debug and test points

---

# 10. PCB stackup

Initial target:

**6-layer PCB**

Conceptual stack:

```text
L1  components + high-speed signals
L2  solid GND
L3  power / signals
L4  power / signals
L5  solid GND
L6  components + slower signals
```

Final impedance-controlled stackup should be selected from the chosen PCB fabricator before routing USB, Ethernet, and other high-speed interfaces.

---

# 11. Connector strategy

Preferred families:

## High-current power

**Molex Micro-Fit 3.0 or similar**

Use for:

- battery/system bus
- amplifier power
- other higher-current harnesses

## Low-current signals

**JST-PH / JST-XH class connectors**

Use for:

- buttons
- LEDs
- sensors
- line-level audio
- low-current control harnesses

## Fans

Standard 4-pin PWM fan connectors.

## Debug

2.54 mm headers or compact keyed service headers where appropriate.

Exact connector MPNs should be selected before footprint lock.

---

# 12. Sourcing policy

Do not treat Amazon as the primary source for PCB silicon.

Preferred sourcing:

1. DigiKey
2. Mouser
3. other authorized distributors as fallback
4. JLCPCB/LCSC where appropriate for assembly

For every critical component, track:

```text
Reference
Manufacturer
Manufacturer Part Number
Package
Primary supplier
Secondary supplier
Lifecycle status
Assembly availability
```

Do not use generic BOM descriptions such as "10 uF capacitor" for finalized parts.

---

# 13. Current major locked parts

| Function | Part / architecture |
|---|---|
| Compute | Raspberry Pi CM4 |
| Management MCU | RP2040 |
| USB hub | Microchip USB2514B |
| Audio DAC | TI PCM5122 |
| Main 5 V regulator | Analog Devices LT8645S |
| AON 3.3 V regulator | Analog Devices LT8609S |
| Battery monitor | TI INA260 |
| Display | MIPI DSI FFC + separate 5 V |
| Bluetooth | TP-Link UB500 |
| Ethernet | CM4 GbE + mainboard RJ45 |
| Temperature | TI TMP1075-class |
| Fans | standard 4-wire 5 V PWM |
| Large amp family | TI TPA3221 |
| Mini amp candidate | TI TPA3116D2 |
| USB-C PD | TI TPS25751D + BQ25756 |
| Battery | 6S lithium-ion |
| Power button | momentary, RP2040-managed |

---

# 14. Rev A success criteria

Rev A is successful when:

- CM4 boots reliably
- no kernel undervoltage events occur under expected workloads
- sustained N64 emulation does not thermally throttle
- MIPI DSI display works
- multiple supported DSI resolutions/sizes work through hardware profiles
- UB500 Bluetooth works reliably while onboard Wi-Fi is active
- USB hub works reliably
- Ethernet works
- PCM5122 produces clean line-level audio
- ground-loop isolator is no longer required
- INA260 reports battery voltage/current and charge/discharge direction
- charging current is visible through the battery monitor
- fan PWM and tach monitoring work
- RP2040 can coordinate graceful shutdown
- long-hold forced shutdown works
- storage-off state has extremely low battery drain
- amplifier startup/shutdown sequencing avoids loud pops
- major subsystems have accessible test points
- no direct soldered wiring to Raspberry Pi header pins is required
- the same mainboard can support multiple Boompi enclosure/display variants

---

# 15. Immediate implementation plan

## Milestone 1: project scaffolding

Create the KiCad project and hierarchical sheets.

## Milestone 2: Sheet 01 — battery/AON

Build and verify:

```text
battery connector
    ->
input protection
    ->
INA260
    ->
SYSTEM_BAT+
    ->
LT8609S
    ->
3V3_AON
    ->
RP2040
```

Also implement:

- momentary wake/latch
- `POWER_HOLD`
- `MAIN_5V_EN`
- amp enable outputs
- SWD/debug

Goal:

**Sheet 01 passes KiCad ERC and every component has a known purpose and candidate MPN.**

## Milestone 3: CM4 core

Use Raspberry Pi CM4 IO Board reference design to establish:

- module connectors
- boot/eMMC
- UART
- DSI
- USB
- Ethernet
- relevant GPIO interfaces

## Milestone 4: LT8645S main 5 V rail

Implement 6S -> 5.1 V / 8 A power stage using the Analog Devices reference layout as closely as practical.

## Milestone 5: USB

Add USB2514B and downstream ports.

## Milestone 6: audio

Add PCM5122 and validate analog layout.

## Milestone 7: display + Ethernet

Add DSI and RJ45.

## Milestone 8: thermal/control

Finish fan, temperature, amplifier-control, and debug interfaces.

## Milestone 9: initial PCB placement

Place major components inside the 120 x 85 mm working envelope before detailed routing.

## Milestone 10: PCB routing and review

Perform:

- DRC
- ERC
- high-speed review
- power review
- analog-audio review
- thermal review
- BOM availability review
- mechanical clearance review

before ordering Rev A.

---

# 16. Guiding principle

Rev A should optimize for:

**known-good architecture, clean power, clean audio, thermal stability, serviceability, and repeatability**

rather than maximum integration or minimum board size.

Rev B can shrink, integrate, and optimize after Rev A is proven.
