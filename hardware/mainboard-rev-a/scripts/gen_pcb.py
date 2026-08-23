#!/usr/bin/env python3
"""Boompi Mainboard Rev A - Milestone 9: initial PCB placement.

Run with KiCad's bundled Python (needs the pcbnew module):
  /Applications/KiCad/KiCad.app/Contents/Frameworks/Python.framework/Versions/3.9/bin/python3 scripts/gen_pcb.py

Reads the exported XML netlist, loads every footprint, assigns nets and
schematic UUID paths (so "Update PCB from Schematic" stays in sync), and
places parts inside the 120 x 85 mm working envelope using the zoning plan
from PLAN.md section 9:

    CM4 (top-left)          audio (top-right)
    USB hub (mid-left)      internal USB-A / RP2040 (mid-right)
    LT8645S (bottom-left)   battery / amp connectors (right edge)
    [RJ45]   [DSI FFC]   [service/debug]   (bottom edge)

Placement is COARSE - it is a starting point for interactive layout,
not a routed board.
"""

import os
import sys
import xml.etree.ElementTree as ET

import pcbnew

HERE = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(HERE)
KICAD_FP_DIR = "/Applications/KiCad/KiCad.app/Contents/SharedSupport/footprints"
NETLIST_XML = "/tmp/boompi-net.xml"
BOARD_PATH = os.path.join(PROJ, "boompi-mainboard-rev-a.kicad_pcb")

# board envelope (mm, page coordinates)
BX0, BY0, BX1, BY1 = 20.0, 20.0, 140.0, 105.0

# ---------------------------------------------------------------------------
# anchored placement of majors: ref -> (x, y, rot_deg)
# (auto-nudged outward in a spiral if the anchor spot collides)
# ---------------------------------------------------------------------------
EXPLICIT = {
    # CM4 (top-left) - keep clear area above for heatsink airflow
    "U201": (56, 45, 90),
    # audio (top-right corner, away from switchers/USB/Ethernet)
    "U501": (112, 26, 0),
    "TP501": (126, 21, 0), "TP502": (130, 21, 0), "TP503": (134, 21, 0),
    # RP2040 supervisor (mid-right)
    "U103": (103, 55, 0), "Y101": (95, 61, 0), "U104": (112, 62, 0),
    "U105": (121, 53, 0), "SW102": (94, 50, 0), "SW101": (94, 44, 0),
    "J106": (126, 47, 0), "J107": (126, 58, 0), "J804": (126, 42, 0),
    # USB hub (below CM4) + ports
    "U401": (57, 73, 0), "Y401": (49, 80, 0), "U403": (66, 80, 0),
    "J401": (90, 74, 270), "U402": (86, 69, 0),
    "U406": (80, 95, 0),
    "J403": (64, 95, 90), "J404": (70, 90, 0), "J405": (84, 88, 0),
    "U404": (60, 88, 0), "U405": (74, 86, 0),
    # main 5V switcher (bottom-left)
    "U301": (36, 78, 0), "L301": (46, 84, 0), "C309": (54, 77, 0),
    "TP301": (30, 74, 0), "TP302": (34, 74, 0),
    # battery entry / telemetry / AON (near right-edge power connectors)
    "F101": (126, 66, 0), "D105": (118, 66, 0),
    "U101": (112, 71, 0), "U102": (102, 78, 0), "L101": (109, 82, 0),
    "C103": (120, 76, 0),
    # display power (inboard, next to the DSI FFC)
    "J602": (47, 89, 0), "J301": (60, 89, 0),
    # fans (top edge)
    "J801": (90, 21, 0), "J802": (100, 21, 0), "J803": (110, 21, 0),
    # amp control / debug (bottom-right, inboard row)
    "J903": (98, 95, 0), "J904": (108, 95, 0),
    "J906": (121, 93, 0), "J907": (119, 87, 0),
}

# support parts placed in a tight ring around their IC: target -> [refs]
SATELLITES = {
    "U103": ["C113", "C114", "C115", "C116", "C117", "C118", "C119",
             "C120", "C121", "R113"],
    "Y101": ["C122", "C123", "R112"],
    "U104": ["C124", "R114", "R115"],
    "U101": ["C101"],
    "U105": ["C125"],
    "U102": ["C105", "C106", "C107", "C108", "C109", "C110",
             "R101", "R102", "R103", "R104"],
    "U201": ["C201", "C202", "C203", "C204", "C205", "C206", "C207",
             "C208", "C209", "C210", "R201", "R202", "R203"],
    "U301": ["C301", "C302", "C303", "C304", "C305", "C306", "C307",
             "C308", "C310", "C311", "R301", "R302", "R304"],
    "U401": ["C412", "C413", "C414", "C418", "C419", "C420",
             "R402", "R403"],
    "Y401": ["C410", "C411"],
    "U403": ["C416", "C417"],
    "U501": ["C501", "C502", "C503", "C504", "C505", "C506", "C507",
             "C508", "C509", "FB501", "FB502", "R501"],
}
SATELLITE_REFS = {r for v in SATELLITES.values() for r in v}

# connectors packed along the board edges using their real sizes.
# right edge, top to bottom; (ref, rot)
RIGHT_EDGE = [("J501", 90), ("J502", 90), ("J101", 90), ("J103", 90), ("J104", 90)]
# bottom edge, left to right
BOTTOM_EDGE = [("U701", 0), ("J601", 180), ("J402", 180), ("J902", 0),
               ("J901", 0), ("J105", 0)]
EDGE_REFS = {r for r, _ in RIGHT_EDGE} | {r for r, _ in BOTTOM_EDGE}

# per-sheet packing regions for everything not explicitly placed
# sheet number (ref hundreds digit) -> list of (x0, y0, x1, y1)
REGIONS = {
    1: [(86, 40, 134, 66), (86, 64, 118, 74)],
    2: [(80, 22, 98, 42), (24, 20, 78, 24)],
    3: [(24, 66, 56, 74), (24, 84, 56, 92), (44, 74, 52, 80)],
    4: [(46, 84, 92, 98), (46, 66, 52, 76)],
    5: [(96, 20, 136, 48)],
    6: [(46, 94, 70, 100)],
    7: [(42, 86, 60, 100)],
    8: [(86, 20, 134, 32)],
    9: [(92, 78, 136, 100)],
}
FALLBACK = [(24, 20, 138, 103)]

def mm(v):
    return pcbnew.FromMM(v)

def sheet_of(ref):
    digits = "".join(c for c in ref if c.isdigit())
    if len(digits) >= 3:
        return int(digits[0])
    return 0

# ---------------------------------------------------------------------------
# netlist
# ---------------------------------------------------------------------------
root = ET.parse(NETLIST_XML).getroot()
comps = {}
for c in root.iter("comp"):
    ref = c.get("ref")
    fp = c.findtext("footprint") or ""
    val = c.findtext("value") or ""
    sheetpath = c.find("sheetpath")
    spath = sheetpath.get("tstamps") if sheetpath is not None else "/"
    tstamp = (c.findtext("tstamps") or "").split()[0].split(",")[0]
    dnp = any(p.get("name") == "dnp" for p in c.findall("property"))
    comps[ref] = dict(fp=fp, val=val, path=spath + tstamp, dnp=dnp)

pad_nets = {}   # (ref, pad) -> netname
netnames = set()
for n in root.iter("net"):
    name = n.get("name")
    if name.startswith("unconnected-"):
        continue
    netnames.add(name)
    for x in n.findall("node"):
        pad_nets[(x.get("ref"), x.get("pin"))] = name

# ---------------------------------------------------------------------------
# board
# ---------------------------------------------------------------------------
board = pcbnew.NewBoard(BOARD_PATH)

# nets
net_objs = {}
for name in sorted(netnames):
    ni = pcbnew.NETINFO_ITEM(board, name)
    board.Add(ni)
    net_objs[name] = ni

# outline
rect = pcbnew.PCB_SHAPE(board)
rect.SetShape(pcbnew.SHAPE_T_RECT)
rect.SetStart(pcbnew.VECTOR2I(mm(BX0), mm(BY0)))
rect.SetEnd(pcbnew.VECTOR2I(mm(BX1), mm(BY1)))
rect.SetLayer(pcbnew.Edge_Cuts)
rect.SetWidth(mm(0.1))
board.Add(rect)

title = pcbnew.PCB_TEXT(board)
title.SetText("Boompi Mainboard Rev A")
title.SetPosition(pcbnew.VECTOR2I(mm(80), mm(23)))
title.SetLayer(pcbnew.F_SilkS)
title.SetTextSize(pcbnew.VECTOR2I(mm(2), mm(2)))
board.Add(title)

def load_footprint(fpid):
    lib, name = fpid.split(":", 1)
    if lib in ("boompi", "CM4IO"):
        path = os.path.join(PROJ, "libraries", lib + ".pretty")
    else:
        path = os.path.join(KICAD_FP_DIR, lib + ".pretty")
    fp = pcbnew.FootprintLoad(path, name)
    if fp is None:
        raise RuntimeError("footprint not found: " + fpid)
    return fp

# occupied rectangles (x0,y0,x1,y1) in mm
occupied = []

def bbox_mm(fp):
    bb = fp.GetBoundingBox(False)   # exclude text, courtyard-ish extent
    return (pcbnew.ToMM(bb.GetLeft()), pcbnew.ToMM(bb.GetTop()),
            pcbnew.ToMM(bb.GetRight()), pcbnew.ToMM(bb.GetBottom()))

def collides(r, margin=0.4):
    x0, y0, x1, y1 = r
    for ox0, oy0, ox1, oy1 in occupied:
        if x0 - margin < ox1 and x1 + margin > ox0 and \
           y0 - margin < oy1 and y1 + margin > oy0:
            return True
    return False

def place(fp, x, y, rot):
    fp.SetPosition(pcbnew.VECTOR2I(mm(x), mm(y)))
    fp.SetOrientationDegrees(rot)

def anchor_place(fp, x, y, rot):
    """Center the footprint bbox on the anchor; spiral-nudge if it collides."""
    for radius in range(0, 15):
        for dx in range(-radius, radius + 1):
            for dy in range(-radius, radius + 1):
                if max(abs(dx), abs(dy)) != radius:
                    continue
                tx, ty = x + dx, y + dy
                place(fp, tx, ty, rot)
                bb = bbox_mm(fp)
                cx, cy = (bb[0] + bb[2]) / 2, (bb[1] + bb[3]) / 2
                place(fp, tx + (tx - cx), ty + (ty - cy), rot)
                bb = bbox_mm(fp)
                if (bb[0] >= BX0 + 0.5 and bb[2] <= BX1 - 0.5 and
                        bb[1] >= BY0 + 0.5 and bb[3] <= BY1 - 0.5 and
                        not collides(bb)):
                    occupied.append(bb)
                    return True
    return False

def auto_place(fp, regions):
    for (rx0, ry0, rx1, ry1) in regions + FALLBACK:
        x = rx0
        while x <= rx1:
            y = ry0
            while y <= ry1:
                place(fp, x, y, 0)
                bb = bbox_mm(fp)
                if (bb[0] >= BX0 + 0.5 and bb[2] <= BX1 - 0.5 and
                        bb[1] >= BY0 + 0.5 and bb[3] <= BY1 - 0.5 and
                        bb[0] >= rx0 - 3 and not collides(bb)):
                    occupied.append(bb)
                    return True
                y += 1.0
            x += 1.0
    return False

# mounting holes first (board-only items)
for i, (hx, hy) in enumerate([(24, 24), (136, 24), (24, 101), (136, 101)]):
    hole = load_footprint("MountingHole:MountingHole_3.2mm_M3")
    hole.SetReference("H%d" % (i + 1))
    try:
        hole.SetAttributes(hole.GetAttributes() |
                           pcbnew.FP_BOARD_ONLY | pcbnew.FP_EXCLUDE_FROM_BOM)
    except AttributeError:
        pass
    place(hole, hx, hy, 0)
    board.Add(hole)
    occupied.append(bbox_mm(hole))

def make_footprint(ref):
    info = comps[ref]
    fp = load_footprint(info["fp"])
    fp.SetReference(ref)
    fp.SetValue(info["val"])
    try:
        fp.SetPath(pcbnew.KIID_PATH(info["path"]))
    except Exception:
        pass
    if info["dnp"]:
        try:
            fp.SetAttributes(fp.GetAttributes() | pcbnew.FP_DNP)
        except AttributeError:
            pass
    board.Add(fp)
    for pad in fp.Pads():
        key = (ref, pad.GetNumber())
        if key in pad_nets:
            pad.SetNet(net_objs[pad_nets[key]])
    return fp

# 1) edge-packed connectors (real sizes, 1.5 mm gaps, clear of mount holes)
y = 26.0
for ref, rot in RIGHT_EDGE:
    fp = make_footprint(ref)
    while True:
        place(fp, 130, y, rot)
        bb = bbox_mm(fp)
        # butt against right edge, top of bbox at y
        place(fp, 130 + (BX1 - 0.8 - bb[2]), y + (y - bb[1]), rot)
        bb = bbox_mm(fp)
        if not collides(bb):
            break
        y += 1.0
    occupied.append(bb)
    y = bb[3] + 1.5

x = 25.0
for ref, rot in BOTTOM_EDGE:
    fp = make_footprint(ref)
    while True:
        place(fp, x, 95, rot)
        bb = bbox_mm(fp)
        place(fp, x + (x - bb[0]), 95 + (BY1 - 0.8 - bb[3]), rot)
        bb = bbox_mm(fp)
        if not collides(bb):
            break
        x += 1.0
    occupied.append(bb)
    x = bb[2] + 1.5

def ring_place(fp, target_bb, max_margin=6.0):
    """Place fp adjacent to a target bbox, walking outward rings."""
    margin = 1.6
    while margin <= max_margin:
        x0, y0 = target_bb[0] - margin, target_bb[1] - margin
        x1, y1 = target_bb[2] + margin, target_bb[3] + margin
        # perimeter candidates
        cands = []
        step = 1.0
        x = x0
        while x <= x1:
            cands += [(x, y0), (x, y1)]
            x += step
        y = y0
        while y <= y1:
            cands += [(x0, y), (x1, y)]
            y += step
        for cx, cy in cands:
            place(fp, cx, cy, 0)
            bb = bbox_mm(fp)
            ccx, ccy = (bb[0] + bb[2]) / 2, (bb[1] + bb[3]) / 2
            place(fp, cx + (cx - ccx), cy + (cy - ccy), 0)
            bb = bbox_mm(fp)
            if (bb[0] >= BX0 + 0.5 and bb[2] <= BX1 - 0.5 and
                    bb[1] >= BY0 + 0.5 and bb[3] <= BY1 - 0.5 and
                    not collides(bb, margin=0.25)):
                occupied.append(bb)
                return True
        margin += 1.2
    return False

# 2) anchored majors
placed_fps = {}
missing = []
for ref in sorted(EXPLICIT, key=lambda r: (sheet_of(r), r)):
    if ref not in comps:
        continue
    fp = make_footprint(ref)
    placed_fps[ref] = fp
    x, y, rot = EXPLICIT[ref]
    if not anchor_place(fp, x, y, rot) and not auto_place(fp, []):
        print("WARN: anchor failed for", ref)

# 3) satellites in a tight ring around their IC
for target, refs in SATELLITES.items():
    if target not in placed_fps:
        continue
    tbb = bbox_mm(placed_fps[target])
    for ref in refs:
        if ref not in comps or not comps[ref]["fp"]:
            continue
        fp = make_footprint(ref)
        if not ring_place(fp, tbb):
            if not auto_place(fp, REGIONS.get(sheet_of(ref), [])):
                print("WARN: no space for satellite", ref)

# 4) everything else packed per sheet zone
rest = sorted(r for r in comps if r not in EDGE_REFS
              and r not in SATELLITE_REFS and r not in EXPLICIT)
for ref in sorted(rest, key=lambda r: (sheet_of(r), r)):
    if not comps[ref]["fp"]:
        missing.append(ref)
        continue
    fp = make_footprint(ref)
    if not auto_place(fp, REGIONS.get(sheet_of(ref), [])):
        print("WARN: no space found for", ref)

ds = board.GetDesignSettings()
try:
    ds.m_MinThroughDrill = mm(0.2)      # CM4 module footprint uses 0.2mm holes
    ds.m_HoleToHoleMin = mm(0.2)
except Exception:
    pass

pcbnew.SaveBoard(BOARD_PATH, board)
print("saved", BOARD_PATH)
print("components:", len(comps), " nets:", len(netnames))
if missing:
    print("no footprint assigned:", ", ".join(missing))
