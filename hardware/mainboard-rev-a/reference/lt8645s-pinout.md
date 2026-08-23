# LT8645S — 65V, 8A Synchronous Step-Down Silent Switcher 2

**Source:** Analog Devices LT8645S/LT8646S datasheet, **Rev. B (04/20)**, downloaded as PDF to
`hardware/mainboard-rev-a/datasheets/lt8645s.pdf` (mirror of
`analog.com/media/en/technical-documentation/data-sheets/lt8645s-8646s.pdf`).
**Confidence: HIGH** — pin table and application values transcribed directly from the actual
datasheet PDF (Pin Configuration p.2, Pin Functions pp.12–13, Typical Applications pp.26–27).

## Ordering / Package

| Ordering P/N | Temp Range | Notes |
|---|---|---|
| **LT8645SEV#PBF** | –40°C to 125°C (E grade) | Standard, RoHS |
| LT8645SIV#PBF | –40°C to 125°C (I grade) | Standard, RoHS |
| LT8645SEV#WPBF / LT8645SIV#WPBF | –40°C to 125°C | Automotive (#W) controlled-mfg versions |

- Package designator **V** = **LQFN 32-lead, 6mm × 4mm × 0.94mm** (laminate package with QFN
  footprint; LTC DWG # 05-08-1512 Rev C). Same footprint as a standard 6mm × 4mm QFN.
- Part marking: `8645SV`. Pad finish Au (RoHS), MSL 3.
- Sibling parts: LT8646S (external comp, pin 30 = VC), LT8645S-2 (no internal caps, 150°C grade —
  **separate datasheet**). This file covers the **LT8645S** only.

## Pin Table (LQFN-32 "V", top view; pin 1 = BIAS corner)

| Pin | Name | Type | Function |
|---|---|---|---|
| 1 | BIAS | Power in (aux) | Internal LDO draws from BIAS instead of VIN when BIAS > 3.1V. Tie to VOUT for 3.3V ≤ VOUT ≤ 25V. If tied to another supply, add 1µF local bypass. Tie to GND if unused. |
| 2 | INTVCC | Output (internal rail) | Internal 3.4V regulator bypass. Bypass cap is **internal** (Silent Switcher 2) — **float this pin**. Do not load externally. Max 25mA (internal use). |
| 3 | NC | — (no connect) | Not internally connected; tie anywhere, typically GND. |
| 4 | VIN | Power in | Input supply. All VIN pins must be tied together and locally bypassed (≥4.7µF). |
| 5 | VIN | Power in | " |
| 6 | VIN | Power in | " |
| 7 | NC | — (no connect) | Not internally connected; tie anywhere, typically GND. |
| 8 | GND | Power (ground) | Ground. Input cap negative terminal as close as possible. |
| 9 | GND | Power (ground) | " |
| 10 | GND | Power (ground) | " |
| 11 | BST | Passive (internal) | Bootstrap for topside switch drive. Boost cap is **internal** — **float this pin**. No external BST capacitor required. |
| 12 | SW | Power out (switch node) | Switch node. Tie all SW pins together to inductor. Keep node small for EMI. |
| 13 | SW | Power out (switch node) | " |
| 14 | SW | Power out (switch node) | " |
| 15 | SW | Power out (switch node) | " |
| 16 | SW | Power out (switch node) | " |
| 17 | GND | Power (ground) | Ground. |
| 18 | GND | Power (ground) | Ground. |
| 19 | GND | Power (ground) | Ground. |
| 20 | NC | — (no connect) | Not internally connected; tie anywhere, typically GND. |
| 21 | VIN | Power in | Input supply (tie to pins 4–6). |
| 22 | VIN | Power in | " |
| 23 | VIN | Power in | " |
| 24 | NC | — (no connect) | Not internally connected; tie anywhere, typically GND. |
| 25 | EN/UV | Input (analog/logic) | Enable / undervolt. Shutdown when low; active high. Thresholds: 1.01V rising / 0.965V falling. Tie to VIN if unused, or resistor divider from VIN for UVLO. |
| 26 | RT | Passive (input) | Frequency-set resistor to GND. |
| 27 | CLKOUT | Output (logic) | ~200ns clock pulse at fSW in pulse-skip/spread-spectrum/sync modes; low in Burst Mode. Float if unused. |
| 28 | SYNC/MODE | Input (logic/clock) | GND = Burst Mode (ultralow IQ); float = pulse-skipping; tie to INTVCC (3V–4V) = spread spectrum; external clock = sync (200kHz–2.2MHz). |
| 29 | TR/SS | Input (analog) | Tracking/soft-start. Internal 1.9µA pull-up; cap to GND sets VOUT ramp. Regulates FB to TR/SS voltage below 0.97V. Float if unused. Pulled low by internal 200Ω FET during shutdown/faults. |
| 30 | GND | Power (ground) | **LT8645S: GND** (internally grounded; may float on PCB for LT8646S pin compatibility). *(On LT8646S this pin is VC — error amp output / external compensation.)* |
| 31 | PG | Output (open-drain) | Power good. High-Z when FB within ±8% of regulation and no faults; needs external pull-up. Valid for VIN > 3.4V. |
| 32 | FB | Input (analog) | Feedback. Regulated to **0.97V**. Connect divider tap plus 1pF–10pF phase-lead cap from VOUT to FB. |
| 33–38 | GND (exposed pads) | Power (ground/thermal) | Six exposed pads on package bottom — all GND. Solder to PCB ground for thermal performance (may be left unconnected only if manufacturing requires, with degraded thermals). |
| corner pads | — | Mechanical | Mechanical support only; tie anywhere, typically GND. |

## Typical Application: 5V (≈5.1V) at 8A, VIN up to 28V

Based on the datasheet front-page circuit and Figure 8 (p.26), fSW = 1MHz (datasheet-proven for
5.5V–65V input; 28V max input is comfortably in range).

```
VIN (5.5–28V) ──┬── VIN pins (4,5,6,21,22,23)
                ├── CIN: 4.7µF ≥50V X7R (1210) + 2× 0.47µF 50V 0805 close to pins (one per side)
                └── EN/UV (25): tie to VIN (always-on)

SW (12–16) ── L1 2.2µH ──┬── VOUT 5.1V / 8A
                          ├── COUT: 100µF (or 2× 47µF) 1210 X5R/X7R, 6.3V+ rating
                          ├── BIAS (1)  ← tie to VOUT
                          ├── R1 1M ──┬── FB (32)   (Cff 2.2pF from VOUT to FB, across R1)
                          │           └── R2 to GND (see below)
                          └── PG pull-up 100k to VOUT (PG pin 31), optional

RT (26): 41.2k to GND  → fSW = 1MHz
TR/SS (29): 10nF to GND (soft-start) — or float if not needed
SYNC/MODE (28): tie to GND (Burst Mode, ultralow IQ) — simple always-works choice
INTVCC (2): FLOAT (bypass cap is internal — do NOT add external cap or load)
BST (11): FLOAT (boost cap is internal — no external BST cap)
CLKOUT (27): float
Pin 30 (GND on LT8645S): tie to GND (or float)
NC pins (3,7,20,24): tie to GND
Exposed pads 33–38: GND, solder down with thermal vias
```

### Feedback divider (FB reference = 0.97V)

`R1 = R2 × (VOUT/0.97 − 1)` (R1 = top resistor VOUT→FB, R2 = FB→GND)

| Target VOUT | R1 | R2 | Actual VOUT |
|---|---|---|---|
| 5.0V (datasheet) | 1M | 243k | 4.96V |
| **5.1V** | **1M** | **234k** | **5.115V** |
| 5.1V (alt) | 1.02M | 240k | 5.09V |

Use 1% resistors; large values keep IQ low. Add 2.2pF–10pF phase-lead cap VOUT→FB.

### Key component notes

- **Inductor:** 2.2µH at 1MHz (datasheet eq. 6: L ≈ (VOUT+0.2)/fSW × 0.4 ≈ 2.1µH). Ripple at
  28V in: ΔIL ≈ 1.9A → peak ≈ 9A at 8A load. Choose IRMS ≥ 8A, ISAT ≥ 11A, DCR < 20mΩ —
  e.g. **Coilcraft XEL6060-222** (datasheet-referenced family).
- **RT:** 41.2kΩ → 1MHz (formula RT[kΩ] = 46.5/fSW[MHz] − 5.2). Other datasheet values:
  500kHz → 88.7k, 2MHz → 17.8k. Range 200kHz–2.2MHz.
- **Input caps:** ≥4.7µF ceramic (X7R/X5R, ≥50V for 28V rail) plus two 0.47µF 0603/0805 placed
  tight to the VIN/GND pins on each side of the IC (datasheet layout requirement).
- **Output caps:** 100µF total ceramic 1210 X5R/X7R (datasheet front page uses 100µF at 1MHz;
  Fig. 8 uses 2×47µF at 500kHz).
- **Top switch current limit:** 14A (low duty) to 11.5A (DC=0.9) — 8A + ripple/2 fits.
- 1MHz max-frequency check at 28V in / 5V out: fSW(MAX) ≈ 4.6MHz (40ns min on-time), so 1MHz
  (or even 2MHz with L = 1µH, RT = 17.8k, per Fig. 11) is fine.

### Simple always-works pin strategy summary

| Pin | Strategy |
|---|---|
| EN/UV | Tie to VIN |
| SYNC/MODE | Tie to GND (Burst Mode) |
| TR/SS | 10nF to GND |
| BIAS | Tie to VOUT (5.1V) |
| INTVCC | Float (internal cap) |
| BST | Float (internal cap) |
| CLKOUT | Float |
| PG | 100k pull-up to VOUT (or float/omit if unused) |
| NC / corner pads | GND |
