#!/usr/bin/env python3
"""Boompi Mainboard Rev A schematic generator.

Implements the architecture defined in hardware/mainboard-rev-a/PLAN.md.
Run from anywhere:  python3 scripts/gen_schematics.py
Then open boompi-mainboard-rev-a.kicad_pro in KiCad 10 and/or run:
  kicad-cli sch erc boompi-mainboard-rev-a.kicad_sch
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(HERE)
sys.path.insert(0, HERE)

import kicad_gen as kg
from kicad_gen import extract_symbol, make_box_symbol, Design, Emitter

CM4IO_SCH = os.path.join(PROJ, "reference", "CM4_GPIO.kicad_sch")

# ---------------------------------------------------------------------------
# Footprints
# ---------------------------------------------------------------------------
R0603 = "Resistor_SMD:R_0603_1608Metric"
C0603 = "Capacitor_SMD:C_0603_1608Metric"
C0805 = "Capacitor_SMD:C_0805_2012Metric"
C1210 = "Capacitor_SMD:C_1210_3225Metric"
CPOL = "Capacitor_SMD:CP_Elec_8x10.5"
LED0603 = "LED_SMD:LED_0603_1608Metric"
SOD123 = "Diode_SMD:D_SOD-123"
DSMC = "Diode_SMD:D_SMC"
SOT23 = "Package_TO_SOT_SMD:SOT-23"
SOT23_5 = "Package_TO_SOT_SMD:SOT-23-5"
SOT23_6 = "Package_TO_SOT_SMD:SOT-23-6"
TSSOP16 = "Package_SO:TSSOP-16_4.4x5mm_P0.65mm"
TSSOP28 = "Package_SO:TSSOP-28_4.4x9.7mm_P0.65mm"
SOIC8_208 = "Package_SO:SOIC-8_5.3x5.3mm_P1.27mm"
SOIC8 = "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm"
QFN56 = "Package_DFN_QFN:QFN-56-1EP_7x7mm_P0.4mm_EP3.2x3.2mm"
QFN36 = "Package_DFN_QFN:QFN-36-1EP_6x6mm_P0.5mm_EP4.1x4.1mm"
XTAL3225 = "Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm"
L_XFL4020 = "Inductor_SMD:L_Coilcraft_XxL4020"
L_XEL6060 = "Inductor_SMD:L_Coilcraft_XAL6060-XXX"
FUSE_NANO2 = "Fuse:Fuse_Littelfuse-NANO2-451_453"
POLYFUSE = "Fuse:Fuse_1812_4532Metric"
MICROFIT4 = "Connector_Molex:Molex_Micro-Fit_3.0_43650-0415_1x04_P3.00mm_Vertical"
PH2 = "Connector_JST:JST_PH_B2B-PH-K_1x02_P2.00mm_Vertical"
PH3 = "Connector_JST:JST_PH_B3B-PH-K_1x03_P2.00mm_Vertical"
PH4 = "Connector_JST:JST_PH_B4B-PH-K_1x04_P2.00mm_Vertical"
PH5 = "Connector_JST:JST_PH_B5B-PH-K_1x05_P2.00mm_Vertical"
XH4 = "Connector_JST:JST_XH_B4B-XH-A_1x04_P2.50mm_Vertical"
HDR = "Connector_PinHeader_2.54mm:PinHeader_1x%02d_P2.54mm_Vertical"
USB_A_FP = "Connector_USB:USB_A_Kycon_KUSBX-AS1N-B_Horizontal"
USB_UB_FP = "Connector_USB:USB_Micro-B_Molex-105017-0001"
FFC22 = "Connector_FFC-FPC:Hirose_FH12-22S-0.5SH_1x22-1MP_P0.50mm_Horizontal"
TP_FP = "TestPoint:TestPoint_Pad_D1.5mm"
SJ_FP = "Jumper:SolderJumper-2_P1.3mm_Open_TrianglePad1.0x1.5mm"
SW_FP = "Button_Switch_SMD:SW_SPST_TL3342"
CM4_FP = "CM4IO:Raspberry-Pi-4-Compute-Module"
MAGJACK_FP = "CM4IO:TRJG0926HENL"

# ---------------------------------------------------------------------------
# Symbols
# ---------------------------------------------------------------------------
S_R = extract_symbol("Device", "R")
S_C = extract_symbol("Device", "C")
S_CP = extract_symbol("Device", "C_Polarized")
S_L = extract_symbol("Device", "L")
S_D = extract_symbol("Device", "D")
S_TVS = extract_symbol("Device", "D_TVS")
S_LED = extract_symbol("Device", "LED")
S_FB = extract_symbol("Device", "FerriteBead")
S_FUSE = extract_symbol("Device", "Fuse")
S_PFUSE = extract_symbol("Device", "Polyfuse")
S_XTAL = extract_symbol("Device", "Crystal_GND24")
S_NMOS = extract_symbol("Transistor_FET", "2N7002")
S_PMOS = extract_symbol("Transistor_FET", "BSS84")
S_SW = extract_symbol("Switch", "SW_Push")
S_TP = extract_symbol("Connector", "TestPoint")
S_USB_A = extract_symbol("Connector", "USB_A")
S_USB_UB = extract_symbol("Connector", "USB_B_Micro")
S_SJ = extract_symbol("Jumper", "SolderJumper_2_Open")
S_ESD = extract_symbol("Power_Protection", "USBLC6-2SC6")
S_LDO = extract_symbol("Regulator_Linear", "AP2112K-3.3")
S_RP2040 = extract_symbol("MCU_RaspberryPi", "RP2040")
S_INA260 = extract_symbol("Sensor_Energy", "INA260")
S_TMP1075 = extract_symbol("Sensor_Temperature", "TMP1075D")
S_FLASH = extract_symbol("Memory_Flash", "W25Q32JVSS")
S_HUB = extract_symbol("Interface_USB", "USB2514B_Bi")
S_DAC = extract_symbol("Audio", "PCM5122PW")
S_CONN = {n: extract_symbol("Connector_Generic", "Conn_01x%02d" % n)
          for n in (2, 3, 4, 5, 8, 10, 22)}
S_CM4 = extract_symbol("CM4IO", "CM4IO:ComputeModule4-CM4",
                       new_lib_id="boompi:CM4-Module", lib_path=CM4IO_SCH)
# Pin 84 stays power_out (drives +3V3_CM4); demote duplicate outputs so the
# two 3.3V pins may be tied together without a power_out/power_out ERC error.
kg.patch_pin_types(S_CM4, {"86": "passive", "88": "passive", "90": "passive"})
S_MAGJACK = extract_symbol("CM4IO", "CM4IO:MagJack-A70-112-331N126",
                           new_lib_id="boompi:MagJack-A70-112-331N126",
                           lib_path=CM4IO_SCH)
# ADR strap pins are hard-tied to GND; passive avoids bidi-vs-power ERC noise.
kg.patch_pin_types(S_DAC, {"16": "passive", "24": "passive"})
# Micro-B service connector: GND must not fight the GND PWR_FLAG.
kg.patch_pin_types(S_USB_UB, {"5": "passive"})

S_LT8645S = make_box_symbol(
    "LT8645S", "LT8645S",
    pins_left=[
        ("25", "EN/UV", "input"),
        ("26", "RT", "passive"),
        ("29", "TR/SS", "passive"),
        ("28", "SYNC/MODE", "input"),
        ("32", "FB", "input"),
        ("31", "PG", "open_collector"),
        ("27", "CLKOUT", "output"),
        ("2", "INTVCC", "passive"),
        ("11", "BST", "passive"),
        ("3", "NC1", "passive"),
        ("7", "NC2", "passive"),
        ("20", "NC3", "passive"),
        ("24", "NC4", "passive"),
    ],
    pins_right=[
        ("4", "VIN", "power_in"),
        ("5", "VIN", "power_in"),
        ("6", "VIN", "power_in"),
        ("21", "VIN", "power_in"),
        ("22", "VIN", "power_in"),
        ("23", "VIN", "power_in"),
        ("1", "BIAS", "power_in"),
        ("12", "SW", "passive"),
        ("13", "SW", "passive"),
        ("14", "SW", "passive"),
        ("15", "SW", "passive"),
        ("16", "SW", "passive"),
        ("8", "GND", "power_in"),
        ("9", "GND", "power_in"),
        ("10", "GND", "power_in"),
        ("17", "GND", "power_in"),
        ("18", "GND", "power_in"),
        ("19", "GND", "power_in"),
        ("30", "GND", "power_in"),
        ("33", "EP_GND", "power_in"),
        ("34", "EP_GND", "power_in"),
        ("35", "EP_GND", "power_in"),
        ("36", "EP_GND", "power_in"),
        ("37", "EP_GND", "power_in"),
        ("38", "EP_GND", "power_in"),
    ],
    footprint="boompi:LT8645S_LQFN32_4x6mm",
    description="65V 8A synchronous buck, Silent Switcher 2, LQFN-32 4x6mm",
    datasheet="https://www.analog.com/media/en/technical-documentation/data-sheets/LT8645S-8646S.pdf")

S_LT8609S = make_box_symbol(
    "LT8609S", "LT8609S",
    pins_left=[
        ("11", "EN/UV", "input"),
        ("1", "RT", "passive"),
        ("15", "TR/SS", "passive"),
        ("16", "SYNC", "input"),
        ("13", "FB", "input"),
        ("12", "PG", "open_collector"),
        ("2", "INTVCC", "passive"),
        ("7", "NC", "passive"),
    ],
    pins_right=[
        ("9", "VIN", "power_in"),
        ("10", "VIN", "power_in"),
        ("5", "SW", "passive"),
        ("6", "SW", "passive"),
        ("3", "GND", "power_in"),
        ("4", "GND", "power_in"),
        ("8", "GND", "power_in"),
        ("14", "GND", "power_in"),
        ("17", "EP_GND", "power_in"),
    ],
    footprint="boompi:LT8609S_LQFN16_3x3mm",
    description="42V 2A synchronous buck, Silent Switcher 2, LQFN-16 3x3mm",
    datasheet="https://www.analog.com/media/en/technical-documentation/data-sheets/LT8609S.pdf")

# net spec shorthand
def g(n): return ("g", n)
def l(n): return ("l", n)
def p(n): return ("p", n)
NC = "NC"
GND = p("GND")
V5 = p("+5V_MAIN")
V3A = p("+3V3_AON")
V3C = p("+3V3_CM4")
V3U = p("+3V3_USB")
V1 = p("+1V1_RP")
VBAT = p("SYSTEM_BAT+")
VRAW = p("BAT_RAW+")

d = Design("boompi-mainboard-rev-a", "df277475-e6f3-473c-a0e7-cf021a9c3f66", PROJ)

# ===========================================================================
# Sheet 01 - BATTERY / AON
# ===========================================================================
s1 = d.sheet("01_BATTERY_AON", "01_battery_aon.kicad_sch", paper="A2",
             title="Battery input, INA260 telemetry, AON 3V3, RP2040 supervisor")

s1.part("J101", S_CONN[4], value="BATTERY_6S", footprint=MICROFIT4,
        mpn="Molex 43650-0415",
        netmap={"1": l("BAT_IN"), "2": l("BAT_IN"), "3": GND, "4": GND})
s1.part("F101", S_FUSE, value="15A", footprint=FUSE_NANO2,
        mpn="Littelfuse 0451015.MRL",
        netmap={"1": l("BAT_IN"), "2": VRAW})
s1.part("D105", S_TVS, value="SMDJ26A", footprint=DSMC, mpn="Littelfuse SMDJ26A",
        netmap={"1": GND, "2": VRAW})
s1.part("U101", S_INA260, value="INA260", footprint=TSSOP16, mpn="TI INA260AIPW",
        netmap={"1": VRAW, "2": VRAW, "3": VRAW,
                "14": VBAT, "15": VBAT, "16": VBAT,
                "12": VBAT, "4": GND, "5": GND, "6": GND, "11": GND,
                "10": V3A, "7": g("PWR_ALERT_N"),
                "8": g("I2C_AON_SDA"), "9": g("I2C_AON_SCL"), "13": NC})
s1.part("C101", S_C, value="100nF", footprint=C0603,
        netmap={"1": V3A, "2": GND})
s1.part("C103", S_CP, value="100uF/35V", footprint=CPOL,
        mpn="Panasonic EEE-FK1V101P", netmap={"1": VBAT, "2": GND})
s1.part("C104", S_C, value="4.7uF/50V", footprint=C1210,
        netmap={"1": VBAT, "2": GND})

# --- LT8609S always-on 3.3 V ----------------------------------------------
s1.part("U102", S_LT8609S, value="LT8609S", mpn="ADI LT8609SEV#PBF",
        footprint="boompi:LT8609S_LQFN16_3x3mm",
        netmap={"1": l("AON_RT"), "2": l("AON_INTVCC"),
                "3": GND, "4": GND, "8": GND, "14": GND, "17": GND, "7": GND,
                "5": l("AON_SW"), "6": l("AON_SW"),
                "9": VBAT, "10": VBAT,
                "11": l("AON_EN"), "12": l("AON_PG"), "13": l("AON_FB"),
                "15": l("AON_SS"), "16": GND})
s1.part("C105", S_C, value="4.7uF/50V", footprint=C1210, netmap={"1": VBAT, "2": GND})
s1.part("C106", S_C, value="1uF", footprint=C0603, netmap={"1": l("AON_INTVCC"), "2": GND})
s1.part("R101", S_R, value="18.2k", footprint=R0603, netmap={"1": l("AON_RT"), "2": GND})
s1.part("C107", S_C, value="10nF", footprint=C0603, netmap={"1": l("AON_SS"), "2": GND})
s1.part("L101", S_L, value="2.2uH", footprint=L_XFL4020,
        mpn="Coilcraft XFL4020-222ME", netmap={"1": l("AON_SW"), "2": V3A})
s1.part("C108", S_C, value="22uF", footprint=C1210, netmap={"1": V3A, "2": GND})
s1.part("C109", S_C, value="22uF", footprint=C1210, netmap={"1": V3A, "2": GND})
s1.part("R102", S_R, value="1M", footprint=R0603, netmap={"1": V3A, "2": l("AON_FB")})
s1.part("R103", S_R, value="309k", footprint=R0603, netmap={"1": l("AON_FB"), "2": GND})
s1.part("C110", S_C, value="10pF", footprint=C0603, netmap={"1": V3A, "2": l("AON_FB")})
s1.part("R104", S_R, value="100k", footprint=R0603, netmap={"1": V3A, "2": l("AON_PG")})
s1.part("TP101", S_TP, value="AON_PG", footprint=TP_FP, netmap={"1": l("AON_PG")})

# --- power button wake / AON latch ----------------------------------------
s1.part("R105", S_R, value="499k", footprint=R0603, netmap={"1": l("AON_EN"), "2": GND})
s1.part("R106", S_R, value="10k", footprint=R0603,
        netmap={"1": l("POWER_HOLD"), "2": l("PH_A")})
s1.part("D101", S_D, value="1N4148W", footprint=SOD123, mpn="1N4148W-7-F",
        netmap={"2": l("PH_A"), "1": l("AON_EN")})
s1.part("Q101", S_PMOS, value="BSS84", footprint=SOT23, mpn="onsemi BSS84LT1G",
        netmap={"1": l("BTN_GATE"), "2": VBAT, "3": l("WAKE_D")})
s1.part("R107", S_R, value="100k", footprint=R0603,
        netmap={"1": l("WAKE_D"), "2": l("WAKE_A")})
s1.part("D102", S_D, value="1N4148W", footprint=SOD123, mpn="1N4148W-7-F",
        netmap={"2": l("WAKE_A"), "1": l("AON_EN")})
s1.part("R108", S_R, value="1M", footprint=R0603,
        netmap={"1": VBAT, "2": l("BTN_GATE")})
s1.part("R109", S_R, value="1.5M", footprint=R0603,
        netmap={"1": l("BTN_GATE"), "2": l("PWR_BUTTON")})
s1.part("J102", S_CONN[2], value="PWR_BUTTON", footprint=PH2, mpn="JST B2B-PH-K-S",
        netmap={"1": l("PWR_BUTTON"), "2": GND})
s1.part("SW101", S_SW, value="PWR_BTN", footprint=SW_FP, mpn="E-Switch TL3342F160QG",
        netmap={"1": l("PWR_BUTTON"), "2": GND})
s1.part("C111", S_C, value="100nF", footprint=C0603,
        netmap={"1": l("PWR_BUTTON"), "2": GND})
s1.part("R110", S_R, value="1M", footprint=R0603,
        netmap={"1": l("PWR_BUTTON"), "2": l("PWR_BTN_SENSE")})
s1.part("R111", S_R, value="130k", footprint=R0603,
        netmap={"1": l("PWR_BTN_SENSE"), "2": GND})
s1.part("C112", S_C, value="10nF", footprint=C0603,
        netmap={"1": l("PWR_BTN_SENSE"), "2": GND})

# --- RP2040 supervisor ------------------------------------------------------
s1.part("U103", S_RP2040, value="RP2040", footprint=QFN56, mpn="Raspberry Pi RP2040",
        netmap={
            "1": V3A, "10": V3A, "22": V3A, "33": V3A, "42": V3A, "49": V3A,
            "23": V1, "50": V1, "44": V3A, "45": V1, "48": V3A, "43": V3A,
            "57": GND, "19": GND,
            "20": l("RP_XIN"), "21": l("RP_XOUT_R"),
            "24": l("SWCLK"), "25": l("SWDIO"), "26": l("RP_RUN"),
            "51": l("QSPI_SD3"), "52": l("QSPI_SCLK"), "53": l("QSPI_SD0"),
            "54": l("QSPI_SD2"), "55": l("QSPI_SD1"), "56": l("QSPI_CS"),
            "2": g("RP_UART_TX"), "3": g("RP_UART_RX"),
            "4": g("I2C_CM4_SDA"), "5": g("I2C_CM4_SCL"),
            "6": g("I2C_AON_SDA"), "7": g("I2C_AON_SCL"),
            "8": g("MAIN_5V_EN"), "9": l("POWER_HOLD"),
            "11": g("AMP_MAIN_EN"), "12": g("AMP_SUB_EN"),
            "13": g("AMP_MAIN_FAULT"), "14": g("AMP_SUB_FAULT"),
            "15": g("FAN1_PWM_CTL"), "16": g("FAN1_TACH"),
            "17": g("FAN2_PWM_CTL"), "18": g("FAN2_TACH"),
            "27": g("FAN3_PWM_CTL"), "28": g("FAN3_TACH"),
            "29": l("PWR_BTN_SENSE"),
            "30": g("CM4_SHUTDOWN_REQ"), "31": g("CM4_POWER_STATE"),
            "32": g("CM4_GLOBAL_EN_N"), "34": l("RP_RUN_SENSE"),
            "35": l("LED1"), "36": l("LED2"),
            "37": g("MAIN_5V_PG"), "38": l("VBAT_SENSE"),
            "39": g("PWR_ALERT_N"), "40": g("AMP_OTW"), "41": g("DAC_MUTE"),
            "46": l("RP_USB_DM"), "47": l("RP_USB_DP"),
        })
for i, ref in enumerate(["C113", "C114", "C115", "C116", "C117", "C118"]):
    s1.part(ref, S_C, value="100nF", footprint=C0603, netmap={"1": V3A, "2": GND})
s1.part("C119", S_C, value="1uF", footprint=C0603, netmap={"1": V3A, "2": GND})
s1.part("C120", S_C, value="1uF", footprint=C0603, netmap={"1": V1, "2": GND})
s1.part("C121", S_C, value="1uF", footprint=C0603, netmap={"1": V1, "2": GND})
s1.part("Y101", S_XTAL, value="12MHz", footprint=XTAL3225,
        mpn="Abracon ABM8-12.000MHZ-B2-T",
        netmap={"1": l("RP_XIN"), "3": l("RP_XTAL2"), "2": GND, "4": GND})
s1.part("C122", S_C, value="27pF", footprint=C0603, netmap={"1": l("RP_XIN"), "2": GND})
s1.part("C123", S_C, value="27pF", footprint=C0603, netmap={"1": l("RP_XTAL2"), "2": GND})
s1.part("R112", S_R, value="1k", footprint=R0603,
        netmap={"1": l("RP_XOUT_R"), "2": l("RP_XTAL2")})
s1.part("R113", S_R, value="10k", footprint=R0603, netmap={"1": V3A, "2": l("RP_RUN")})
s1.part("U104", S_FLASH, value="W25Q32JVSS", footprint=SOIC8_208,
        mpn="Winbond W25Q32JVSSIQ",
        netmap={"1": l("QSPI_CS"), "2": l("QSPI_SD1"), "3": l("QSPI_SD2"),
                "4": GND, "5": l("QSPI_SD0"), "6": l("QSPI_SCLK"),
                "7": l("QSPI_SD3"), "8": V3A})
s1.part("C124", S_C, value="100nF", footprint=C0603, netmap={"1": V3A, "2": GND})
s1.part("R114", S_R, value="10k", footprint=R0603, netmap={"1": V3A, "2": l("QSPI_CS")})
s1.part("SW102", S_SW, value="BOOTSEL", footprint=SW_FP,
        netmap={"1": l("BOOTSEL_A"), "2": GND})
s1.part("R115", S_R, value="1k", footprint=R0603,
        netmap={"1": l("QSPI_CS"), "2": l("BOOTSEL_A")})
s1.part("J106", S_CONN[5], value="RP2040_SWD", footprint=HDR % 5,
        netmap={"1": V3A, "2": l("SWDIO"), "3": l("SWCLK"),
                "4": l("RP_RUN"), "5": GND})
s1.part("R116", S_R, value="27R", footprint=R0603,
        netmap={"1": l("RP_USB_DP"), "2": l("RP_USB_DP_C")})
s1.part("R117", S_R, value="27R", footprint=R0603,
        netmap={"1": l("RP_USB_DM"), "2": l("RP_USB_DM_C")})
s1.part("J107", S_CONN[3], value="RP2040_USB", footprint=HDR % 3,
        netmap={"1": l("RP_USB_DP_C"), "2": l("RP_USB_DM_C"), "3": GND})

# --- telemetry / misc -------------------------------------------------------
s1.part("U105", S_TMP1075, value="TMP1075D", footprint=SOIC8, mpn="TI TMP1075DR",
        netmap={"1": g("I2C_AON_SDA"), "2": g("I2C_AON_SCL"),
                "3": g("PWR_ALERT_N"), "4": GND, "5": GND, "6": GND, "7": GND,
                "8": V3A})
s1.part("C125", S_C, value="100nF", footprint=C0603, netmap={"1": V3A, "2": GND})
s1.part("R118", S_R, value="4.7k", footprint=R0603, netmap={"1": V3A, "2": g("I2C_AON_SDA")})
s1.part("R119", S_R, value="4.7k", footprint=R0603, netmap={"1": V3A, "2": g("I2C_AON_SCL")})
s1.part("R120", S_R, value="10k", footprint=R0603, netmap={"1": V3A, "2": g("PWR_ALERT_N")})
s1.part("R121", S_R, value="100k", footprint=R0603, netmap={"1": VBAT, "2": l("VBAT_SENSE")})
s1.part("R122", S_R, value="10k", footprint=R0603, netmap={"1": l("VBAT_SENSE"), "2": GND})
s1.part("C126", S_C, value="100nF", footprint=C0603, netmap={"1": l("VBAT_SENSE"), "2": GND})
s1.part("R123", S_R, value="330R", footprint=R0603, netmap={"1": l("LED1"), "2": l("LED1_A")})
s1.part("D103", S_LED, value="STATUS1", footprint=LED0603,
        netmap={"2": l("LED1_A"), "1": GND})
s1.part("R124", S_R, value="330R", footprint=R0603, netmap={"1": l("LED2"), "2": l("LED2_A")})
s1.part("D104", S_LED, value="STATUS2", footprint=LED0603,
        netmap={"2": l("LED2_A"), "1": GND})
s1.part("R125", S_R, value="100k", footprint=R0603,
        netmap={"1": g("CM4_RUN_PG"), "2": l("RP_RUN_SENSE")})

# --- battery-bus power connectors ------------------------------------------
s1.part("J103", S_CONN[4], value="AMP_MAIN_PWR", footprint=MICROFIT4,
        mpn="Molex 43650-0415",
        netmap={"1": VBAT, "2": VBAT, "3": GND, "4": GND})
s1.part("J104", S_CONN[4], value="AMP_SUB_PWR", footprint=MICROFIT4,
        mpn="Molex 43650-0415",
        netmap={"1": VBAT, "2": VBAT, "3": GND, "4": GND})
s1.part("J105", S_CONN[4], value="USBC_PD_PWR", footprint=MICROFIT4,
        mpn="Molex 43650-0415",
        netmap={"1": VBAT, "2": VBAT, "3": GND, "4": GND})

s1.flag("GND")
s1.flag("BAT_RAW+")
s1.flag("SYSTEM_BAT+")
s1.flag("+3V3_AON")

s1.text("SHEET 01 - BATTERY INPUT / INA260 / AON 3V3 / RP2040 SUPERVISOR\n"
        "Power path: J101 (6S pack, keyed Micro-Fit) -> F101 15A -> BAT_RAW+ -> INA260 shunt -> SYSTEM_BAT+.\n"
        "The USB-C PD charger board (J105) attaches on the SYSTEM side of the INA260 so charge\n"
        "current reads negative (PLAN.md sec.2). Reverse-polarity protection is by keyed connector + pack BMS.\n"
        "Power states: BTN press pulls PWR_BUTTON low -> Q101 drives AON_EN high -> LT8609S starts ->\n"
        "RP2040 boots and latches POWER_HOLD (via D101). STORAGE OFF = RP2040 drops POWER_HOLD.\n"
        "PWR_BTN_SENSE: ~2.9V idle, 0V pressed (GPIO18). VBAT_SENSE = SYSTEM_BAT+/11 (ADC0).\n"
        "I2C addresses: INA260=0x40, TMP1075=0x48 (remote sensor strap 0x49), RP2040 slave to CM4 on I2C_CM4.\n"
        "TVS D105 SMDJ26A: standoff 26V > 25.2V pack max; verify clamp < LT8609S 42V abs max at realistic surge.",
        30, 15, 2.0)

# ===========================================================================
# Sheet 02 - CM4 CORE
# ===========================================================================
s2 = d.sheet("02_CM4_CORE", "02_cm4_core.kicad_sch", paper="A2",
             title="CM4 module, boot support, control interfaces")

cm4_map = {}
for pin in S_CM4.pins:
    n, name = pin["num"], pin["name"]
    if name == "GND":
        cm4_map[n] = GND
    elif name.startswith("+5v"):
        cm4_map[n] = V5
    elif name.startswith("+3.3v"):
        cm4_map[n] = V3C
    else:
        cm4_map[n] = NC
cm4_map.update({
    "3": g("ETH_P3_P"), "4": g("ETH_P1_P"), "5": g("ETH_P3_N"), "6": g("ETH_P1_N"),
    "9": g("ETH_P2_N"), "10": g("ETH_P0_N"), "11": g("ETH_P2_P"), "12": g("ETH_P0_P"),
    "15": g("ETH_nLED3"), "17": g("ETH_nLED2"),
    "20": l("EEPROM_WP_N"), "21": l("ACT_LED_K"),
    "24": g("CM4_SHUTDOWN_REQ"), "25": g("I2S_DOUT"), "26": g("I2S_LRCLK"),
    "27": g("EXP_GPIO20"), "28": g("EXP_GPIO13"), "29": g("EXP_GPIO16"),
    "30": g("EXP_GPIO6"), "31": g("EXP_GPIO12"), "34": g("EXP_GPIO5"),
    "37": g("EXP_SPI_CE1"), "38": g("EXP_SPI_SCLK"), "39": g("EXP_SPI_CE0"),
    "40": g("EXP_SPI_MISO"), "41": g("CM4_POWER_STATE"), "44": g("EXP_SPI_MOSI"),
    "49": g("I2S_BCLK"), "50": g("EXP_GPIO17"),
    "51": g("CM4_UART_RX"), "54": g("EXP_GPIO4"), "55": g("CM4_UART_TX"),
    "56": g("I2C_CM4_SCL"), "58": g("I2C_CM4_SDA"),
    "78": V3C, "80": g("DSI_SCL0"), "82": g("DSI_SDA0"),
    "89": l("WL_DIS"), "91": l("BT_DIS"), "92": g("CM4_RUN_PG"),
    "93": g("nRPIBOOT"), "99": l("CM4_GLOBAL_EN"),
    "101": g("USB_OTG_ID"), "103": g("USB_UP_DM"), "105": g("USB_UP_DP"),
    "175": g("DSI1_D0_N"), "177": g("DSI1_D0_P"),
    "181": g("DSI1_D1_N"), "183": g("DSI1_D1_P"),
    "187": g("DSI1_C_N"), "189": g("DSI1_C_P"),
    "193": g("DSI1_D2_N"), "195": g("DSI1_D2_P"),
    "194": g("DSI1_D3_N"), "196": g("DSI1_D3_P"),
})
s2.part("U201", S_CM4, value="CM4 (eMMC + WiFi)", footprint=CM4_FP,
        mpn="Raspberry Pi CM4104032", netmap=cm4_map, stub=3.81)

s2.part("R201", S_R, value="2.2k", footprint=R0603, netmap={"1": V3C, "2": g("I2C_CM4_SDA")})
s2.part("R202", S_R, value="2.2k", footprint=R0603, netmap={"1": V3C, "2": g("I2C_CM4_SCL")})
s2.part("R203", S_R, value="10k", footprint=R0603, netmap={"1": V3C, "2": g("CM4_RUN_PG")})
s2.part("R204", S_R, value="330R", footprint=R0603, netmap={"1": V3C, "2": l("ACT_A")})
s2.part("D201", S_LED, value="ACT", footprint=LED0603,
        netmap={"2": l("ACT_A"), "1": l("ACT_LED_K")})
s2.part("JP201", S_SJ, value="nRPIBOOT", footprint=SJ_FP,
        netmap={"1": g("nRPIBOOT"), "2": GND})
s2.part("JP202", S_SJ, value="EEPROM_WP", footprint=SJ_FP,
        netmap={"1": l("EEPROM_WP_N"), "2": GND})
s2.part("JP203", S_SJ, value="WL_DISABLE", footprint=SJ_FP,
        netmap={"1": l("WL_DIS"), "2": GND})
s2.part("JP204", S_SJ, value="BT_DISABLE", footprint=SJ_FP,
        netmap={"1": l("BT_DIS"), "2": GND})
s2.part("Q201", S_NMOS, value="2N7002", footprint=SOT23,
        netmap={"1": l("GEN_GATE"), "2": GND, "3": l("CM4_GLOBAL_EN")})
s2.part("R205", S_R, value="100R", footprint=R0603,
        netmap={"1": g("CM4_GLOBAL_EN_N"), "2": l("GEN_GATE")})
s2.part("R206", S_R, value="100k", footprint=R0603,
        netmap={"1": l("GEN_GATE"), "2": GND})
for ref in ("C201", "C202", "C203", "C204"):
    s2.part(ref, S_C, value="4.7uF", footprint=C0805, netmap={"1": V5, "2": GND})
for ref in ("C205", "C206", "C207", "C208"):
    s2.part(ref, S_C, value="100nF", footprint=C0603, netmap={"1": V5, "2": GND})
for ref in ("C209", "C210"):
    s2.part(ref, S_C, value="100nF", footprint=C0603, netmap={"1": V3C, "2": GND})

s2.text("SHEET 02 - CM4 CORE (per Raspberry Pi CM4IO reference design)\n"
        "eMMC CM4 variants: SD_* pins intentionally unconnected. HDMI / CSI / PCIe / composite unused in Rev A.\n"
        "DSI1 (4-lane) is the Boompi display interface -> sheet 06. Onboard WiFi used; onboard BT NOT used for audio\n"
        "(UB500 on USB, sheet 04). JP201 nRPIBOOT: close to flash eMMC over the service micro-USB (also holds hub reset).\n"
        "Global_EN has an internal module pull-up; Q201 lets the RP2040 force a hard power-cycle.\n"
        "RUN_PG pulled to 3V3_CM4 (R203) and sensed by RP2040 through R125 (sheet 01).",
        30, 15, 2.0)

# ===========================================================================
# Sheet 03 - MAIN 5V
# ===========================================================================
s3 = d.sheet("03_MAIN_5V", "03_main_5v.kicad_sch", paper="A3",
             title="LT8645S 6S -> 5.1V/8A main rail")

s3.part("U301", S_LT8645S, value="LT8645S", mpn="ADI LT8645SEV#PBF",
        footprint="boompi:LT8645S_LQFN32_4x6mm",
        netmap={"1": V5, "2": NC, "3": GND, "4": VBAT, "5": VBAT, "6": VBAT,
                "7": GND, "8": GND, "9": GND, "10": GND, "11": NC,
                "12": l("SW_5V"), "13": l("SW_5V"), "14": l("SW_5V"),
                "15": l("SW_5V"), "16": l("SW_5V"),
                "17": GND, "18": GND, "19": GND, "20": GND,
                "21": VBAT, "22": VBAT, "23": VBAT, "24": GND,
                "25": g("MAIN_5V_EN"), "26": l("RT_5V"), "27": NC,
                "28": GND, "29": l("SS_5V"), "30": GND,
                "31": g("MAIN_5V_PG"), "32": l("FB_5V"),
                "33": GND, "34": GND, "35": GND, "36": GND, "37": GND, "38": GND})
s3.part("L301", S_L, value="2.2uH", footprint=L_XEL6060,
        mpn="Coilcraft XEL6060-222MEB",
        netmap={"1": l("SW_5V"), "2": V5})
s3.part("C301", S_C, value="4.7uF/50V", footprint=C1210, netmap={"1": VBAT, "2": GND})
s3.part("C302", S_C, value="4.7uF/50V", footprint=C1210, netmap={"1": VBAT, "2": GND})
s3.part("C303", S_C, value="470nF/50V", footprint=C0603, netmap={"1": VBAT, "2": GND})
s3.part("C304", S_C, value="470nF/50V", footprint=C0603, netmap={"1": VBAT, "2": GND})
for ref in ("C305", "C306", "C307", "C308"):
    s3.part(ref, S_C, value="47uF/6.3V", footprint=C1210, netmap={"1": V5, "2": GND})
s3.part("C309", S_CP, value="470uF/6.3V poly", footprint=CPOL,
        mpn="Panasonic 6TPE470MI", netmap={"1": V5, "2": GND})
s3.part("R301", S_R, value="1M", footprint=R0603, netmap={"1": V5, "2": l("FB_5V")})
s3.part("R302", S_R, value="232k", footprint=R0603, netmap={"1": l("FB_5V"), "2": GND})
s3.part("C310", S_C, value="4.7pF", footprint=C0603, netmap={"1": V5, "2": l("FB_5V")})
s3.part("R303", S_R, value="100k", footprint=R0603, netmap={"1": g("MAIN_5V_EN"), "2": GND})
s3.part("R304", S_R, value="41.2k", footprint=R0603, netmap={"1": l("RT_5V"), "2": GND})
s3.part("C311", S_C, value="10nF", footprint=C0603, netmap={"1": l("SS_5V"), "2": GND})
s3.part("R305", S_R, value="100k", footprint=R0603, netmap={"1": V3A, "2": g("MAIN_5V_PG")})
s3.part("J301", S_CONN[4], value="DISPLAY_5V", footprint=XH4, mpn="JST B4B-XH-A",
        netmap={"1": V5, "2": V5, "3": GND, "4": GND})
s3.part("TP301", S_TP, value="+5V_MAIN", footprint=TP_FP, netmap={"1": V5})
s3.part("TP302", S_TP, value="GND", footprint=TP_FP, netmap={"1": GND})
s3.flag("+5V_MAIN")

s3.text("SHEET 03 - MAIN 5V RAIL (LT8645S Silent Switcher 2, per ADI datasheet fig.8)\n"
        "6S battery (max 25.2V) -> 5.15V nominal (R301/R302 = 1M/232k, VFB=0.97V) at up to 8A.\n"
        "fSW = 1MHz (RT=41.2k). SYNC/MODE=GND (Burst). INTVCC & BST caps are INTERNAL - pins float.\n"
        "Enable from RP2040 MAIN_5V_EN (GPIO6); R303 keeps rail OFF at power-up. PG -> GPIO25 via R305.\n"
        "Replaces the MP2307 UBEC that caused kernel undervoltage events (PLAN.md sec.2).\n"
        "LAYOUT: follow ADI demo board DC2874A - CIN tight to VIN/GND pins, small SW island, thermal vias under EP.",
        30, 15, 2.0)

# ===========================================================================
# Sheet 04 - USB
# ===========================================================================
s4 = d.sheet("04_USB", "04_usb.kicad_sch", paper="A2",
             title="USB2514B hub, UB500 port, external/service ports")

s4.part("U401", S_HUB, value="USB2514B", footprint=QFN36, mpn="Microchip USB2514B-AEZC",
        netmap={
            "1": l("USB1H_DM"), "2": l("USB1H_DP"),
            "3": l("USB2H_DM"), "4": l("USB2H_DP"),
            "6": l("USB3H_DM"), "7": l("USB3H_DP"),
            "8": l("USB4_DM"), "9": l("USB4_DP"),
            "5": V3U, "10": V3U, "29": V3U, "36": V3U, "15": V3U, "23": V3U,
            "37": GND, "11": GND,
            "12": NC, "16": NC, "18": NC, "20": NC,
            "13": l("OCS_PU"), "17": l("OCS_PU"), "19": l("OCS_PU"), "21": l("OCS_PU"),
            "14": l("CRFILT"), "34": l("PLLFILT"), "35": l("RBIAS"),
            "22": l("NONREM1"), "24": l("CFG0"), "25": NC,
            "26": l("HUB_RST_N"), "27": l("HUB_VBUS_DET"), "28": l("LOCALPWR"),
            "30": g("USB_UP_DM"), "31": g("USB_UP_DP"),
            "32": l("HUB_XO"), "33": l("HUB_XI")})
s4.part("R402", S_R, value="12k 1%", footprint=R0603, netmap={"1": l("RBIAS"), "2": GND})
s4.part("C412", S_C, value="1uF", footprint=C0603, netmap={"1": l("CRFILT"), "2": GND})
s4.part("C413", S_C, value="100nF", footprint=C0603, netmap={"1": l("PLLFILT"), "2": GND})
s4.part("R406", S_R, value="10k", footprint=R0603, netmap={"1": l("NONREM1"), "2": GND})
s4.part("R407", S_R, value="10k", footprint=R0603, netmap={"1": l("CFG0"), "2": GND})
s4.part("R408", S_R, value="10k", footprint=R0603, netmap={"1": V3U, "2": l("LOCALPWR")})
s4.part("R409", S_R, value="10k", footprint=R0603, netmap={"1": V3U, "2": l("OCS_PU")})
s4.part("R403", S_R, value="10k", footprint=R0603, netmap={"1": V3U, "2": l("HUB_RST_N")})
s4.part("C414", S_C, value="1uF", footprint=C0603, netmap={"1": l("HUB_RST_N"), "2": GND})
s4.part("D401", S_D, value="1N4148W", footprint=SOD123,
        netmap={"2": l("HUB_RST_N"), "1": g("nRPIBOOT")})
s4.part("R404", S_R, value="47k", footprint=R0603, netmap={"1": V5, "2": l("HUB_VBUS_DET")})
s4.part("R405", S_R, value="68k", footprint=R0603, netmap={"1": l("HUB_VBUS_DET"), "2": GND})
s4.part("Y401", S_XTAL, value="24MHz", footprint=XTAL3225,
        mpn="Abracon ABM8-24.000MHZ-B2-T",
        netmap={"1": l("HUB_XI"), "3": l("HUB_XO"), "2": GND, "4": GND})
s4.part("C410", S_C, value="18pF", footprint=C0603, netmap={"1": l("HUB_XI"), "2": GND})
s4.part("C411", S_C, value="18pF", footprint=C0603, netmap={"1": l("HUB_XO"), "2": GND})
for ref in ("C418", "C419", "C420"):
    s4.part(ref, S_C, value="100nF", footprint=C0603, netmap={"1": V3U, "2": GND})
s4.part("U403", S_LDO, value="AP2112K-3.3", footprint=SOT23_5,
        mpn="Diodes AP2112K-3.3TRG1",
        netmap={"1": V5, "2": GND, "3": V5, "4": NC, "5": V3U})
s4.part("C416", S_C, value="1uF", footprint=C0603, netmap={"1": V5, "2": GND})
s4.part("C417", S_C, value="1uF", footprint=C0603, netmap={"1": V3U, "2": GND})

# Port 1: internal USB-A for TP-Link UB500
s4.part("U402", S_ESD, value="USBLC6-2SC6", footprint=SOT23_6,
        mpn="ST USBLC6-2SC6",
        netmap={"1": l("USB1H_DM"), "3": l("USB1H_DP"),
                "6": l("USB1C_DM"), "4": l("USB1C_DP"),
                "5": l("VBUS_UB500"), "2": GND})
s4.part("J401", S_USB_A, value="UB500_PORT", footprint=USB_A_FP,
        mpn="Kycon KUSBX-AS1N-B",
        netmap={"1": l("VBUS_UB500"), "2": l("USB1C_DM"), "3": l("USB1C_DP"),
                "4": GND, "SH": GND})
s4.part("F401", S_PFUSE, value="1.1A", footprint=POLYFUSE, mpn="Littelfuse 1812L110",
        netmap={"1": V5, "2": l("VBUS_UB500")})
s4.part("C401", S_C, value="22uF", footprint=C0805, netmap={"1": l("VBUS_UB500"), "2": GND})

# Port 2: external USB-C data path (connector lives on the PD board)
s4.part("U404", S_ESD, value="USBLC6-2SC6", footprint=SOT23_6,
        netmap={"1": l("USB2H_DM"), "3": l("USB2H_DP"),
                "6": l("USB2C_DM"), "4": l("USB2C_DP"),
                "5": l("VBUS_EXT"), "2": GND})
s4.part("J403", S_CONN[5], value="USBC_DATA", footprint=PH5, mpn="JST B5B-PH-K-S",
        netmap={"1": l("VBUS_EXT"), "2": l("USB2C_DM"), "3": l("USB2C_DP"),
                "4": GND, "5": GND})

# Port 3: internal service header
s4.part("U405", S_ESD, value="USBLC6-2SC6", footprint=SOT23_6,
        netmap={"1": l("USB3H_DM"), "3": l("USB3H_DP"),
                "6": l("USB3C_DM"), "4": l("USB3C_DP"),
                "5": l("VBUS_SVC"), "2": GND})
s4.part("J404", S_CONN[4], value="USB_SERVICE", footprint=HDR % 4,
        netmap={"1": l("VBUS_SVC"), "2": l("USB3C_DM"), "3": l("USB3C_DP"), "4": GND})
s4.part("F402", S_PFUSE, value="1.1A", footprint=POLYFUSE, mpn="Littelfuse 1812L110",
        netmap={"1": V5, "2": l("VBUS_SVC")})
s4.part("C402", S_C, value="22uF", footprint=C0805, netmap={"1": l("VBUS_SVC"), "2": GND})

# Port 4: spare (DNP)
s4.part("J405", S_CONN[4], value="USB_SPARE", footprint=HDR % 4, dnp=True,
        netmap={"1": l("VBUS_SPARE"), "2": l("USB4_DM"), "3": l("USB4_DP"), "4": GND})
s4.part("F403", S_PFUSE, value="1.1A", footprint=POLYFUSE, dnp=True,
        netmap={"1": V5, "2": l("VBUS_SPARE")})

# Service micro-USB on the CM4 upstream lines (rpiboot eMMC flashing)
s4.part("J402", S_USB_UB, value="RPIBOOT_USB", footprint=USB_UB_FP,
        mpn="Molex 105017-0001",
        netmap={"1": l("VBUS_SVCUSB"), "2": l("USBUP_C_DM"), "3": l("USBUP_C_DP"),
                "4": g("USB_OTG_ID"), "5": GND, "SH": GND})
s4.part("U406", S_ESD, value="USBLC6-2SC6", footprint=SOT23_6,
        netmap={"1": l("USBUP_C_DM"), "3": l("USBUP_C_DP"),
                "6": g("USB_UP_DM"), "4": g("USB_UP_DP"),
                "5": l("VBUS_SVCUSB"), "2": GND})
s4.part("TP401", S_TP, value="VBUS_SVCUSB", footprint=TP_FP,
        netmap={"1": l("VBUS_SVCUSB")})
s4.flag_local("VBUS_UB500")
s4.flag_local("VBUS_SVC")

s4.text("SHEET 04 - USB2514B 4-PORT HUB (PLAN.md sec.2)\n"
        "CM4 USB2 -> hub upstream. Port1 -> internal USB-A (TP-Link UB500 Bluetooth). Port2 -> external\n"
        "USB-C data path to the PD board (J403; VBUS/CC handled on PD board). Port3 -> service header.\n"
        "Port4 -> spare (DNP). Straps: SMBus disabled (SDA/SCL 10k low) = default config; LOCAL_PWR high = self-powered.\n"
        "OCS_N tied high (no per-port current sense IC; polyfuses instead). VBUS_DET from +5V_MAIN divider.\n"
        "rpiboot: close JP201 (sheet 02) -> CM4 nRPIBOOT low AND hub RESET_N held low via D401; then the\n"
        "service micro-USB J402 talks straight to the CM4 as a device. Keep J402 stub short in layout.",
        30, 15, 2.0)

# ===========================================================================
# Sheet 05 - AUDIO
# ===========================================================================
s5 = d.sheet("05_AUDIO", "05_audio.kicad_sch", paper="A3",
             title="PCM5122 I2S DAC, line outputs")

s5.part("U501", S_DAC, value="PCM5122", footprint=TSSOP28, mpn="TI PCM5122PW",
        netmap={"1": l("CPVDD_F"), "2": l("CP_P"), "3": GND, "4": l("CP_M"),
                "5": l("VNEG"), "6": l("OUT_L_RAW"), "7": l("OUT_R_RAW"),
                "8": l("AVDD_F"), "9": GND, "10": l("VCOM"),
                "11": g("I2C_CM4_SDA"), "12": g("I2C_CM4_SCL"),
                "13": NC, "14": NC, "15": NC, "16": GND, "17": GND, "18": GND,
                "19": NC, "20": GND,
                "21": g("I2S_BCLK"), "22": g("I2S_DOUT"), "23": g("I2S_LRCLK"),
                "24": GND, "25": l("XSMT"), "26": l("LDOO"), "27": GND,
                "28": V3C})
s5.part("FB501", S_FB, value="600R@100MHz", footprint="Inductor_SMD:L_0805_2012Metric",
        mpn="Murata BLM21PG600SN1D", netmap={"1": V3C, "2": l("AVDD_F")})
s5.part("C501", S_C, value="10uF", footprint=C0805, netmap={"1": l("AVDD_F"), "2": GND})
s5.part("C502", S_C, value="100nF", footprint=C0603, netmap={"1": l("AVDD_F"), "2": GND})
s5.part("FB502", S_FB, value="600R@100MHz", footprint="Inductor_SMD:L_0805_2012Metric",
        mpn="Murata BLM21PG600SN1D", netmap={"1": V3C, "2": l("CPVDD_F")})
s5.part("C503", S_C, value="2.2uF", footprint=C0603, netmap={"1": l("CPVDD_F"), "2": GND})
s5.part("C504", S_C, value="100nF", footprint=C0603, netmap={"1": V3C, "2": GND})
s5.part("C505", S_C, value="10uF", footprint=C0805, netmap={"1": V3C, "2": GND})
s5.part("C506", S_C, value="2.2uF", footprint=C0603, netmap={"1": l("CP_P"), "2": l("CP_M")})
s5.part("C507", S_C, value="2.2uF", footprint=C0603, netmap={"1": l("VNEG"), "2": GND})
s5.part("C508", S_C, value="1uF", footprint=C0603, netmap={"1": l("LDOO"), "2": GND})
s5.part("C509", S_C, value="1uF", footprint=C0603, netmap={"1": l("VCOM"), "2": GND})
s5.part("R501", S_R, value="10k", footprint=R0603, netmap={"1": V3C, "2": l("XSMT")})
s5.part("Q501", S_NMOS, value="2N7002", footprint=SOT23,
        netmap={"1": l("MUTE_G"), "2": GND, "3": l("XSMT")})
s5.part("R502", S_R, value="1k", footprint=R0603, netmap={"1": g("DAC_MUTE"), "2": l("MUTE_G")})
s5.part("R503", S_R, value="100k", footprint=R0603, netmap={"1": l("MUTE_G"), "2": GND})
s5.part("R504", S_R, value="100R", footprint=R0603,
        netmap={"1": l("OUT_L_RAW"), "2": l("LINE_L")})
s5.part("C510", S_C, value="2.2nF C0G", footprint=C0603, netmap={"1": l("LINE_L"), "2": GND})
s5.part("R505", S_R, value="100R", footprint=R0603,
        netmap={"1": l("OUT_R_RAW"), "2": l("LINE_R")})
s5.part("C511", S_C, value="2.2nF C0G", footprint=C0603, netmap={"1": l("LINE_R"), "2": GND})
s5.part("J501", S_CONN[4], value="LINE_OUT_MAIN", footprint=PH4, mpn="JST B4B-PH-K-S",
        netmap={"1": l("LINE_L"), "2": GND, "3": l("LINE_R"), "4": GND})
s5.part("J502", S_CONN[4], value="LINE_OUT_SUB", footprint=PH4, mpn="JST B4B-PH-K-S",
        netmap={"1": l("LINE_L"), "2": GND, "3": l("LINE_R"), "4": GND})
s5.part("TP501", S_TP, value="LINE_L", footprint=TP_FP, netmap={"1": l("LINE_L")})
s5.part("TP502", S_TP, value="LINE_R", footprint=TP_FP, netmap={"1": l("LINE_R")})
s5.part("TP503", S_TP, value="AGND", footprint=TP_FP, netmap={"1": GND})
s5.flag_local("AVDD_F")
s5.flag_local("CPVDD_F")

s5.text("SHEET 05 - PCM5122 AUDIO DAC (PLAN.md sec.2)\n"
        "I2S from CM4 (GPIO18/19/21), control via I2C_CM4 @ 0x4C (ADR1=ADR2=0). MODE1/MODE2=0 -> I2C mode.\n"
        "SCK grounded: internal PLL generates the master clock from BCK (HiFiBerry-style, no external MCLK).\n"
        "2.0 VRMS ground-centered line out (VNEG charge pump) -> no output caps, no ground-loop isolator.\n"
        "XSMT: pulled up (unmuted) once 3V3_CM4 is up; RP2040 DAC_MUTE (GPIO29) can hard-mute via Q501\n"
        "during power transitions for pop-free sequencing (PLAN.md sec.5). J501 -> stereo amp module,\n"
        "J502 -> subwoofer amp module (module sums L+R).\n"
        "LAYOUT: keep this section away from LT8645S / USB / Ethernet / fan switching (PLAN.md sec.9).",
        30, 15, 2.0)

# ===========================================================================
# Sheet 06 - DSI
# ===========================================================================
s6 = d.sheet("06_DSI", "06_dsi.kicad_sch", paper="A3",
             title="MIPI DSI display FFC + display power")

s6.part("J601", S_CONN[22], value="DSI_DISPLAY", footprint=FFC22,
        mpn="Hirose FH12-22S-0.5SH(55)",
        netmap={"1": GND, "4": GND, "7": GND, "10": GND, "13": GND,
                "16": GND, "19": GND,
                "2": g("DSI1_D0_N"), "3": g("DSI1_D0_P"),
                "5": g("DSI1_D1_N"), "6": g("DSI1_D1_P"),
                "8": g("DSI1_C_N"), "9": g("DSI1_C_P"),
                "11": g("DSI1_D2_N"), "12": g("DSI1_D2_P"),
                "14": g("DSI1_D3_N"), "15": g("DSI1_D3_P"),
                "17": NC, "18": NC,
                "20": g("DSI_SCL0"), "21": g("DSI_SDA0"), "22": V3C})
s6.part("R601", S_R, value="2.2k", footprint=R0603, netmap={"1": V3C, "2": g("DSI_SDA0")})
s6.part("R602", S_R, value="2.2k", footprint=R0603, netmap={"1": V3C, "2": g("DSI_SCL0")})
s6.part("J602", S_CONN[4], value="DISPLAY_PWR_5V", footprint=XH4, mpn="JST B4B-XH-A",
        netmap={"1": V5, "2": V5, "3": GND, "4": GND})

s6.text("SHEET 06 - DSI DISPLAY (PLAN.md sec.2)\n"
        "Raspberry Pi-standard 22-pin 0.5mm FFC on CM4 DSI1 - all 4 lanes wired so both 2-lane panels\n"
        "(e.g. 4.3in OSOYOO) and 4-lane panels (Touch Display 2) work; selection via device-tree overlay.\n"
        "Pinout copied from CM4IO J16 (DISP1). Touch I2C = CM4 SDA0/SCL0 (pins 20/21), pulled to 3V3_CM4.\n"
        "Display 5V comes from the separate keyed connector J602 (or J301 on sheet 03) - not through the FFC.\n"
        "LAYOUT: 100R differential pairs, matched within pair, solid GND reference; keep < 100mm to connector.",
        30, 15, 2.0)

# ===========================================================================
# Sheet 07 - ETHERNET
# ===========================================================================
s7 = d.sheet("07_ETHERNET", "07_ethernet.kicad_sch", paper="A3",
             title="Gigabit Ethernet magjack")

s7.part("U701", S_MAGJACK, value="TRJG0926HENL", footprint=MAGJACK_FP,
        mpn="TRP TRJG0926HENL",
        netmap={"1": g("ETH_P0_P"), "2": g("ETH_P0_N"),
                "3": g("ETH_P1_P"), "6": g("ETH_P1_N"),
                "7": g("ETH_P2_P"), "8": g("ETH_P2_N"),
                "9": g("ETH_P3_P"), "10": g("ETH_P3_N"),
                "4": l("ETH_CT45"), "5": l("ETH_CT45"),
                "11": l("TAP0"), "12": l("TAP1"), "13": l("TAP2"), "14": l("TAP3"),
                "15": V3C, "16": l("LEDG_K"), "17": V3C, "18": l("LEDY_K"),
                "19": GND, "20": GND})
s7.part("C701", S_C, value="100nF", footprint=C0603, netmap={"1": l("ETH_CT45"), "2": GND})
s7.part("R701", S_R, value="75R", footprint=R0603, netmap={"1": l("TAP0"), "2": l("BS_TERM")})
s7.part("R702", S_R, value="75R", footprint=R0603, netmap={"1": l("TAP1"), "2": l("BS_TERM")})
s7.part("R703", S_R, value="75R", footprint=R0603, netmap={"1": l("TAP2"), "2": l("BS_TERM")})
s7.part("R704", S_R, value="75R", footprint=R0603, netmap={"1": l("TAP3"), "2": l("BS_TERM")})
s7.part("C702", S_C, value="1nF/2kV", footprint="Capacitor_SMD:C_1812_4532Metric",
        netmap={"1": l("BS_TERM"), "2": GND})
s7.part("R705", S_R, value="330R", footprint=R0603,
        netmap={"1": l("LEDG_K"), "2": g("ETH_nLED2")})
s7.part("R706", S_R, value="330R", footprint=R0603,
        netmap={"1": l("LEDY_K"), "2": g("ETH_nLED3")})

s7.text("SHEET 07 - GIGABIT ETHERNET (debug / rescue / development, PLAN.md sec.2)\n"
        "CM4 has the PHY on-module (BCM54210) -> pairs go straight to an integrated-magnetics magjack.\n"
        "Wiring copied from the CM4IO reference (same magjack family). Center taps: Bob Smith termination\n"
        "75R x4 into 1nF/2kV. LEDs driven by CM4 Ethernet_nLED2 (link/green) / nLED3 (activity/yellow).\n"
        "Some enclosures leave the RJ45 internally inaccessible - that is fine, nothing else depends on it.\n"
        "LAYOUT: 100R differential pairs, pair-matched; void other signals under the jack.",
        30, 15, 2.0)

# ===========================================================================
# Sheet 08 - THERMAL / FANS
# ===========================================================================
s8 = d.sheet("08_THERMAL_FANS", "08_thermal_fans.kicad_sch", paper="A3",
             title="PWM fan drive, tach, remote temp sensor")

for i, dnp in ((1, False), (2, False), (3, True)):
    s8.part("J80%d" % i, S_CONN[4], value="FAN%d" % i, footprint=HDR % 4, dnp=dnp,
            mpn="Molex 47053-1000 (4-pin fan)",
            netmap={"1": GND, "2": V5,
                    "3": l("FAN%d_T_RAW" % i), "4": l("FAN%d_PWM" % i)})
    s8.part("Q80%d" % i, S_NMOS, value="2N7002", footprint=SOT23, dnp=dnp,
            netmap={"1": l("FAN%d_G" % i), "2": GND, "3": l("FAN%d_PWM" % i)})
    s8.part("R8%d1" % i, S_R, value="100R", footprint=R0603, dnp=dnp,
            netmap={"1": g("FAN%d_PWM_CTL" % i), "2": l("FAN%d_G" % i)})
    s8.part("R8%d2" % i, S_R, value="100k", footprint=R0603, dnp=dnp,
            netmap={"1": l("FAN%d_G" % i), "2": GND})
    s8.part("R8%d3" % i, S_R, value="10k", footprint=R0603, dnp=dnp,
            netmap={"1": V5, "2": l("FAN%d_PWM" % i)})
    s8.part("R8%d4" % i, S_R, value="10k", footprint=R0603, dnp=dnp,
            netmap={"1": V3A, "2": l("FAN%d_T_RAW" % i)})
    s8.part("R8%d5" % i, S_R, value="1k", footprint=R0603, dnp=dnp,
            netmap={"1": l("FAN%d_T_RAW" % i), "2": g("FAN%d_TACH" % i)})
s8.part("J804", S_CONN[4], value="REMOTE_TEMP", footprint=PH4, mpn="JST B4B-PH-K-S",
        netmap={"1": V3A, "2": GND, "3": g("I2C_AON_SDA"), "4": g("I2C_AON_SCL")})

s8.text("SHEET 08 - THERMAL / FANS (PLAN.md sec.6)\n"
        "Two populated 4-wire 5V PWM fan channels + optional third (FAN3 parts DNP).\n"
        "PWM: 2N7002 open-drain per Intel 4-wire fan spec (control input is pulled up INSIDE the fan;\n"
        "R8x3 is a board-side backup pull-up) - note the drive is INVERTING, handle in RP2040 firmware.\n"
        "TACH: open-collector from fan, pulled to 3V3_AON, 1k series into RP2040 counters.\n"
        "J804: optional remote TMP1075 board in the battery compartment on I2C_AON (strap addr 0x49).\n"
        "Firmware: CPU-temp-driven policy w/ hysteresis (PLAN.md table), fail-safe 100% if boompid heartbeat lost.",
        30, 15, 2.0)

# ===========================================================================
# Sheet 09 - CONNECTORS / DEBUG
# ===========================================================================
s9 = d.sheet("09_CONNECTORS_DEBUG", "09_connectors_debug.kicad_sch", paper="A3",
             title="Amp control, debug UART/SWD, expansion, test points")

s9.part("J901", S_CONN[5], value="AMP_MAIN_CTRL", footprint=PH5, mpn="JST B5B-PH-K-S",
        netmap={"1": g("AMP_MAIN_EN"), "2": g("AMP_MAIN_FAULT"), "3": g("AMP_OTW"),
                "4": GND, "5": GND})
s9.part("R901", S_R, value="100k", footprint=R0603, netmap={"1": g("AMP_MAIN_EN"), "2": GND})
s9.part("R902", S_R, value="10k", footprint=R0603, netmap={"1": V3A, "2": g("AMP_MAIN_FAULT")})
s9.part("R903", S_R, value="10k", footprint=R0603, netmap={"1": V3A, "2": g("AMP_OTW")})
s9.part("J902", S_CONN[4], value="AMP_SUB_CTRL", footprint=PH4, mpn="JST B4B-PH-K-S",
        netmap={"1": g("AMP_SUB_EN"), "2": g("AMP_SUB_FAULT"), "3": GND, "4": GND})
s9.part("R904", S_R, value="100k", footprint=R0603, netmap={"1": g("AMP_SUB_EN"), "2": GND})
s9.part("R905", S_R, value="10k", footprint=R0603, netmap={"1": V3A, "2": g("AMP_SUB_FAULT")})
s9.part("J903", S_CONN[3], value="CM4_UART", footprint=HDR % 3,
        netmap={"1": g("CM4_UART_TX"), "2": g("CM4_UART_RX"), "3": GND})
s9.part("J904", S_CONN[3], value="RP2040_UART", footprint=HDR % 3,
        netmap={"1": g("RP_UART_TX"), "2": g("RP_UART_RX"), "3": GND})
s9.part("J905", S_CONN[4], value="I2C_EXP_CM4", footprint=PH4, mpn="JST B4B-PH-K-S",
        netmap={"1": V3C, "2": GND, "3": g("I2C_CM4_SDA"), "4": g("I2C_CM4_SCL")})
s9.part("J906", S_CONN[8], value="SPI_EXP", footprint=HDR % 8,
        netmap={"1": V3C, "2": GND, "3": g("EXP_SPI_MOSI"), "4": g("EXP_SPI_MISO"),
                "5": g("EXP_SPI_SCLK"), "6": g("EXP_SPI_CE0"), "7": g("EXP_SPI_CE1"),
                "8": GND})
s9.part("J907", S_CONN[10], value="GPIO_EXP", footprint=HDR % 10,
        netmap={"1": V3C, "2": GND, "3": g("EXP_GPIO4"), "4": g("EXP_GPIO5"),
                "5": g("EXP_GPIO6"), "6": g("EXP_GPIO12"), "7": g("EXP_GPIO13"),
                "8": g("EXP_GPIO16"), "9": g("EXP_GPIO17"), "10": g("EXP_GPIO20")})
s9.part("TP901", S_TP, value="SYSTEM_BAT+", footprint=TP_FP, netmap={"1": VBAT})
s9.part("TP902", S_TP, value="+5V_MAIN", footprint=TP_FP, netmap={"1": V5})
s9.part("TP903", S_TP, value="+3V3_AON", footprint=TP_FP, netmap={"1": V3A})
s9.part("TP904", S_TP, value="+3V3_CM4", footprint=TP_FP, netmap={"1": V3C})
s9.part("TP905", S_TP, value="GND", footprint=TP_FP, netmap={"1": GND})
s9.part("TP906", S_TP, value="GND", footprint=TP_FP, netmap={"1": GND})
s9.part("TP907", S_TP, value="I2S_BCLK", footprint=TP_FP, netmap={"1": g("I2S_BCLK")})
s9.part("TP908", S_TP, value="I2S_LRCLK", footprint=TP_FP, netmap={"1": g("I2S_LRCLK")})
s9.part("TP909", S_TP, value="I2S_DOUT", footprint=TP_FP, netmap={"1": g("I2S_DOUT")})

s9.text("SHEET 09 - CONNECTORS / DEBUG (PLAN.md sec.7)\n"
        "Amp module control per PLAN.md amp-module requirement (EN + FAULT + OTW):\n"
        "  J901 -> TPA3221 stereo module (EN default-off via R901; FAULT/OTW are open-drain, pulled to 3V3_AON).\n"
        "  J902 -> TPA3221 PBTL subwoofer module. Amp POWER comes from J103/J104 on sheet 01 (SYSTEM_BAT+).\n"
        "Debug: CM4 console UART (GPIO14/15), RP2040 console UART (GPIO0/1), RP2040 SWD on sheet 01 (J106).\n"
        "Expansion: CM4 SPI0 + 8 spare CM4 GPIOs + CM4 I2C. All 3V3 logic. Test points on every major rail.",
        30, 15, 2.0)

# ===========================================================================
if __name__ == "__main__":
    em = Emitter(d)
    em.emit_all()
    print("wrote %d sheets + root into %s" % (len(d.sheets), PROJ))
