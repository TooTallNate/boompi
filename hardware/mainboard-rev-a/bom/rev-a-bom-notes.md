# Boompi Mainboard Rev A — critical-component BOM notes

Per PLAN.md §12: no generic descriptions for finalized parts; DigiKey/Mouser
first, JLCPCB/LCSC where appropriate for assembly. Lifecycle/assembly columns
to be confirmed at order time.

| Reference | Function | Manufacturer | MPN | Package | Primary supplier | Secondary supplier | Notes |
|---|---|---|---|---|---|---|---|
| U201 | Compute Module | Raspberry Pi | CM4104032 (4GB/32GB/WiFi) | CM4 (2x DF40 100p) | Authorized RPi resellers (DigiKey, PiShop) | Mouser | eMMC + WiFi variant per PLAN; any CM4xxxxxx fits |
| J-CM4 (in U201 fp) | CM4 mating connectors | Hirose | DF40C-100DS-0.4V(51) | DF40 100-pos 0.4mm | DigiKey | Mouser | 2 pcs; 1.5–3.0mm stack height option per heatsink plan |
| U103 | Management MCU | Raspberry Pi | RP2040 | QFN-56 7x7 | DigiKey | Mouser, LCSC | |
| U104 | RP2040 QSPI flash | Winbond | W25Q32JVSSIQ | SOIC-8 208mil | DigiKey | Mouser, LCSC | 4MB is ample for supervisor firmware |
| Y101 | RP2040 crystal | Abracon | ABM8-12.000MHZ-B2-T | 3225 | DigiKey | Mouser | CL=10pF, 27pF loads + 1k series per RP2040 HW design guide |
| U401 | USB 2.0 hub | Microchip | USB2514B-AEZC | QFN-36 6x6 | DigiKey | Mouser | Default-config straps, self-powered |
| Y401 | Hub crystal | Abracon | ABM8-24.000MHZ-B2-T | 3225 | DigiKey | Mouser | |
| U403 | 3.3V LDO (hub) | Diodes Inc | AP2112K-3.3TRG1 | SOT-23-5 | DigiKey | Mouser, LCSC | 600mA; hub draws ~100mA |
| U501 | Audio DAC | TI | PCM5122PW | TSSOP-28 | DigiKey | Mouser | I2C addr 0x4C; SCK grounded (BCK PLL mode) |
| U301 | Main 5V buck | Analog Devices | LT8645SEV#PBF | LQFN-32 6x4 | DigiKey | Mouser, Arrow | 5.15V @ 1MHz; follow DC2874A/demo layout |
| L301 | 5V buck inductor | Coilcraft | XEL6060-222MEB | 6.56x6.36 | DigiKey | Mouser, Coilcraft direct | 2.2µH, ISAT ≥ 11A required |
| U102 | AON 3.3V buck | Analog Devices | LT8609SEV#PBF | LQFN-16 3x3 | DigiKey | Mouser | NOT the MSOP LT8609 — different pinout |
| L101 | AON buck inductor | Coilcraft | XFL4020-222MEC | 4x4 | DigiKey | Mouser | 2.2µH |
| U101 | Battery monitor | TI | INA260AIPW | TSSOP-16 | DigiKey | Mouser | Addr 0x40; charger on system side of shunt |
| U105 | Temperature sensor | TI | TMP1075DR | SOIC-8 | DigiKey | Mouser | Addr 0x48; remote board straps 0x49 |
| U402/404/405/406 | USB ESD | ST | USBLC6-2SC6 | SOT-23-6 | DigiKey | Mouser, LCSC | |
| Q101 | Wake P-FET | onsemi | BSS84LT1G | SOT-23 | DigiKey | Mouser, LCSC | Vgs kept ≤ ~10V by R108/R109 divider |
| Q201/501/801-803 | Logic N-FET | onsemi/Diodes | 2N7002LT1G | SOT-23 | DigiKey | Mouser, LCSC | |
| D101/102/401 | Small-signal diode | Diodes Inc | 1N4148W-7-F | SOD-123 | DigiKey | Mouser, LCSC | |
| D105 | Input TVS | Littelfuse | SMDJ26A | SMC | DigiKey | Mouser | Standoff 26V > 25.2V pack; verify clamp vs LT8609S 42V abs max |
| F101 | Main fuse | Littelfuse | 0451015.MRL | Nano2 451/453 | DigiKey | Mouser | 15A fast; revisit vs. amp load profile |
| F401/402/403 | Port polyfuse | Littelfuse | 1812L110/33MR | 1812 | DigiKey | Mouser | 1.1A hold |
| J101/103/104/105 | Battery/amp/PD power | Molex | 43650-0415 + 43645-0400 plug | Micro-Fit 3.0 1x4 vert | DigiKey | Mouser | Keyed; crimp 43030-0007 |
| J102/501/502/804/901/902/905 | Signal connectors | JST | B2B/B4B/B5B-PH-K-S | JST-PH vert | DigiKey | Mouser, LCSC | |
| J301/602 | Display 5V | JST | B4B-XH-A | JST-XH vert | DigiKey | Mouser | Keyed, 2x 5V + 2x GND |
| J401 | UB500 USB-A | Kycon | KUSBX-AS1N-B | USB-A horiz TH | DigiKey | Mouser | Internal port; verify UB500 body clearance |
| J402 | Service micro-USB | Molex | 105017-0001 | micro-B SMD | DigiKey | Mouser, LCSC | rpiboot/eMMC flashing |
| J601 | DSI FFC | Hirose | FH12-22S-0.5SH(55) | 22p 0.5mm FFC | DigiKey | Mouser | Pi-standard 22-pin display pinout (CM4IO J16) |
| U701 | GbE magjack | TRP/Bel | TRJG0926HENL | RJ45 + magnetics | DigiKey | Mouser | Same family as CM4IO reference |
| J801/802/803 | Fan headers | Molex | 47053-1000 | 4-pin fan TH | DigiKey | Mouser | FAN3 DNP |
| SW101/102 | Buttons | E-Switch | TL3342F160QG | SMD tact | DigiKey | Mouser | Bench power + BOOTSEL |
| C309 | 5V bulk polymer | Panasonic | 6TPE470MI | 7343 | DigiKey | Mouser | 470µF 6.3V POSCAP |
| C103 | Battery bulk | Panasonic | EEE-FK1V101P | SMD electrolytic | DigiKey | Mouser | 100µF 35V |
| FB501/502 | Audio ferrites | Murata | BLM21PG600SN1D | 0805 | DigiKey | Mouser, LCSC | |

## Off-board (modular per PLAN.md §1)

| Function | Part / architecture | Status |
|---|---|---|
| Stereo amp module | TI TPA3221 (BTL stereo) | module spec: BAT+, GND, line in, EN, FAULT, OTW → J103 + J901 |
| Subwoofer amp module | TI TPA3221 (PBTL) | → J104 + J902 |
| USB-C PD / charger board | TI TPS25751D + BQ25756 reference | → J105 (power) + J403 (USB2 data) |
| Bluetooth | TP-Link UB500 | plugs into J401 |
| Battery | 6S li-ion + BMS | keyed Micro-Fit at J101 |
| Remote temp sensor | TMP1075 breakout @ 0x49 | → J804 |

## Open items before footprint/BOM lock

- Verify LT8609S LQFN-16 land pattern against LTC DWG 05-08-1516 Rev B
  (current footprint is a generic 3x3 QFN-16 pattern).
- Confirm F101 rating against measured worst-case amp + charge current.
- Confirm SMDJ26A clamping at realistic surge vs LT8609S 42V abs max
  (consider SMDJ24A or series RC snubber if marginal).
- Select exact fan header MPN (47053-1000 vs generic 2.54mm w/ friction lock).
- Pick Micro-Fit stack (vertical vs right-angle) per enclosure harness routing.
- CM4 connector stack height (DF40C-100DS-0.4V vs 1.5/2.0/3.0mm variants)
  must match the heatsink/board-standoff decision.
