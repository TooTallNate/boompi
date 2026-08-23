# LT8609S — 42V, 2A (3A peak) Synchronous Step-Down Silent Switcher 2

**Source:** Analog Devices LT8609S datasheet, **Rev. A (10/17)**, downloaded as PDF to
`hardware/mainboard-rev-a/datasheets/lt8609s.pdf` (mirror of
`analog.com/media/en/technical-documentation/data-sheets/LT8609S.pdf`).
**Confidence: HIGH** — pin table and application values transcribed directly from the actual
datasheet PDF (Pin Configuration p.2, Pin Functions p.9, Typical Applications p.20).

> **PACKAGE CORRECTION:** The LT8609S is a **16-lead 3mm × 3mm × 0.94mm LQFN** (LTC DWG
> # 05-08-1516 Rev B), *not* a 10-lead 3mm × 2mm package. The 10-lead MSOP (MSE) is the plain
> LT8609/LT8609A/LT8609B — a different, non-Silent-Switcher part with a different pinout
> (and it has a BST pin, which the LT8609S does not). Do not mix up the footprints.

## Ordering / Package

| Ordering P/N | Temp Range | Notes |
|---|---|---|
| **LT8609SEV#PBF** | –40°C to 125°C (E grade) | RoHS |
| LT8609SIV#PBF | –40°C to 125°C (I grade) | RoHS |

- Package designator **V** = **LQFN 16-lead, 3mm × 3mm × 0.94mm** (laminate package with QFN
  footprint). Same footprint as a standard 3mm × 3mm QFN. Part marking: `LGYN`. MSL 3, Au pad finish.
- Silent Switcher 2: VIN bypass and boost capacitors are integrated — **no BST pin, no BIAS pin.**

## Pin Table (LQFN-16 "V", top view; pin 1 = RT)

| Pin | Name | Type | Function |
|---|---|---|---|
| 1 | RT | Passive (input) | Frequency-set resistor to GND. |
| 2 | INTVCC | Output (internal rail) | Internal 3.5V LDO bypass. **Requires ≥1µF low-ESR ceramic to power GND** (unlike LT8645S, the cap is external here). Do not load externally. Max 20mA (internal use). |
| 3 | GND | Power (ground) | Ground. Connect to input cap negative terminal. |
| 4 | GND | Power (ground) | " |
| 5 | SW | Power out (switch node) | Switch node — connect to inductor. Keep node small. (Boost cap internal; nothing else connects here.) |
| 6 | SW | Power out (switch node) | " |
| 7 | N/C | — (no connect) | Connect to ground plane for mechanical robustness (temp cycling). |
| 8 | GND | Power (ground) | Ground. |
| 9 | VIN | Power in | Input supply; bypass locally, positive cap terminal tight to VIN pins. |
| 10 | VIN | Power in | " |
| 11 | EN/UV | Input (analog/logic) | Enable / undervolt. Shutdown when low. Thresholds: 1.05V rising / 1.00V falling. Tie to VIN if unused, or divider from VIN for UVLO. |
| 12 | PG | Output (open-drain) | Power good. High-Z when FB within ±8.5% of regulation, no faults; external pull-up required. Valid for VIN > 3.2V. |
| 13 | FB | Input (analog) | Feedback. Regulated to **0.774V**. Connect divider tap; 10pF phase-lead cap from VOUT when using large divider resistors. |
| 14 | GND | Power (ground) | Ground. |
| 15 | TR/SS | Input (analog) | Tracking/soft-start. Internal 2µA pull-up; cap to GND sets VOUT ramp. Regulates FB to TR/SS below 0.774V. Pulled low via internal 300Ω FET during shutdown/faults. |
| 16 | SYNC | Input (logic/clock) | GND = Burst Mode (ultralow IQ); float = pulse-skipping; tie to INTVCC (or 3.2V–5.0V) = pulse-skip + spread spectrum; external clock (200kHz–2.2MHz, valleys <0.9V, peaks >2.7V) = sync. |
| 17 | GND (exposed pad) | Power (ground/thermal) | Exposed pad — **must** be soldered to PCB ground (electrical + thermal). |
| corner pads | N/C | Mechanical | Solder to ground plane for mechanical performance. |

## Typical Application: 3.3V at 2A, VIN up to 28V

Directly from the datasheet "3.3V Step Down" typical application (p.20, `8609S TA02`);
specified there for VIN = 3.8V to 42V, so 28V max input is well within range. fSW = 2MHz.

```
VIN (3.8–28V) ──┬── VIN pins (9,10)
                ├── CIN: 4.7µF ≥50V X7R ceramic, tight to VIN/GND pins
                └── EN/UV (11): tie to VIN (always-on)

SW (5,6) ── L1 2.2µH (XFL4020-222ME) ──┬── VOUT 3.3V / 2A (3A peak <1s)
                                        ├── COUT: 22µF X7R 1206
                                        ├── R2 1M ──┬── FB (13)   (Cff 10pF from VOUT to FB)
                                        │           └── R3 309k to GND
                                        └── RPG 100k pull-up to VOUT (PG pin 12), optional

RT (1): 18.2k to GND  → fSW = 2MHz
INTVCC (2): 1µF X7R ceramic to GND  (REQUIRED)
TR/SS (15): 10nF to GND (soft-start)
SYNC (16): tie to GND (Burst Mode, ultralow IQ) — simple always-works choice
N/C pin 7 + corner pads: solder to GND
Exposed pad 17: GND, solder down with thermal vias
```

### Feedback divider (FB reference = 0.774V)

`R1 = R2 × (VOUT/0.774 − 1)` (top resistor VOUT→FB vs bottom FB→GND)

| Target VOUT | Top (VOUT→FB) | Bottom (FB→GND) | Actual VOUT |
|---|---|---|---|
| **3.3V (datasheet)** | **1M** | **309k** | **3.28V** |
| 5.0V (datasheet) | 1M | 182k | 5.03V |

1% resistors; large values preserve the 2.5µA no-load IQ. 10pF feedforward cap VOUT→FB.

### Key component notes

- **Inductor:** 2.2µH, **Coilcraft XFL4020-222ME** (datasheet-specified). Ripple at 28V in:
  ΔIL ≈ 0.66A → peak ≈ 2.35A at 2A load (≈3.35A during 3A transients). IRMS ≥ 2A,
  ISAT ≥ 4A, DCR < 40mΩ. (XFL4020-222ME: ISAT ≈ 5.4A — fine.)
- **RT:** 18.2kΩ → 2MHz. Other datasheet Table 1 values: 1MHz → 40.2k, 700kHz → 60.4k,
  500kHz → 86.6k, 400kHz → 110k. Range 200kHz–2.2MHz.
- 2MHz max-frequency check at 28V in / 3.3V out: fSW(MAX) ≈ 2.8MHz (45ns min on-time,
  VSW(TOP) ≈ 0.4V, VSW(BOT) ≈ 0.25V) — 2MHz OK. If you want more margin, drop to 1MHz
  (RT = 40.2k) and use L ≈ 3.3µH–4.7µH, COUT ≈ 47µF (COUT ≈ 100/(VOUT·fSW) µF rule).
- **Input cap:** 4.7µF–10µF X7R/X5R ceramic (≥50V for a 28V rail), placed tight to
  pins 9/10 and GND. No Y5V.
- **Output cap:** 22µF X7R 1206 at 2MHz (datasheet rule COUT = 100/(VOUT·fSW) ≈ 15µF → 22µF).
- **Current limit:** top switch 4.75A typ (low duty) → 4.0A at D = 0.8; continuous output
  must stay ≤2A (thermal), 3A peaks <1s allowed.
- **No BST cap, no BIAS pin** — both functions are internal on this "S" part.

### Simple always-works pin strategy summary

| Pin | Strategy |
|---|---|
| EN/UV | Tie to VIN |
| SYNC | Tie to GND (Burst Mode) |
| TR/SS | 10nF to GND |
| INTVCC | 1µF ceramic to GND (mandatory) |
| PG | 100k pull-up to VOUT (or float/omit if unused) |
| N/C + corner pads | Solder to GND |
| Exposed pad (17) | GND + thermal vias |
