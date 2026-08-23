#!/usr/bin/env python3
"""Boompi Mainboard Rev A - start of Milestone 10: power distribution.

Run with KiCad's bundled Python after gen_pcb.py:
  .../Python.framework/Versions/3.9/bin/python3 scripts/route_power.py

Implements the PLAN.md section 10 stackup and the first routing pass:

  L1 (F.Cu)   components + fanout tracks + GND pour
  L2 (In1.Cu) solid GND plane
  L3 (In2.Cu) +5V_MAIN plane
  L4 (In3.Cu) SYSTEM_BAT+ / +3V3_AON / +3V3_CM4 split power plane
  L5 (In4.Cu) solid GND plane
  L6 (B.Cu)   GND pour (signals later)

SMD power pads get an automatic via fanout to their plane; through-hole
pads reach the planes directly.  GND stitching vias are sprinkled on a
grid so the outer pours stay connected.  Signal routing (USB, DSI,
Ethernet, I2S, control) remains manual by design.
"""

import os
import json

import pcbnew

HERE = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(HERE)
BOARD_PATH = os.path.join(PROJ, "boompi-mainboard-rev-a.kicad_pcb")
PRO_PATH = os.path.join(PROJ, "boompi-mainboard-rev-a.kicad_pro")

BX0, BY0, BX1, BY1 = 20.0, 20.0, 140.0, 105.0

def mm(v):
    return pcbnew.FromMM(v)

def to_mm(v):
    return pcbnew.ToMM(v)

board = pcbnew.LoadBoard(BOARD_PATH)
board.SetCopperLayerCount(6)

nets = board.GetNetsByName()

def net(name):
    n = nets[name]
    return n

# ---------------------------------------------------------------------------
# zones / planes
# ---------------------------------------------------------------------------
def add_zone(netname, layer, poly, priority=0, name=""):
    z = pcbnew.ZONE(board)
    z.SetLayer(layer)
    chain = pcbnew.SHAPE_LINE_CHAIN()
    for x, y in poly:
        chain.Append(pcbnew.VECTOR2I(mm(x), mm(y)))
    chain.SetClosed(True)
    z.Outline().AddOutline(chain)
    z.SetNet(net(netname))
    try:
        z.SetAssignedPriority(priority)
    except AttributeError:
        z.SetPriority(priority)
    z.SetZoneName(name or ("%s_%s" % (netname.split('/')[-1],
                                      board.GetLayerName(layer))))
    z.SetLocalClearance(mm(0.3))
    z.SetMinThickness(mm(0.25))
    # solid connections: reflow assembly, avoids starved-thermal errors on
    # fine-pitch parts; revisit per-pad during detailed layout if desired
    z.SetPadConnection(pcbnew.ZONE_CONNECTION_FULL)
    board.Add(z)
    return z

FULL = [(BX0 + 0.3, BY0 + 0.3), (BX1 - 0.3, BY0 + 0.3),
        (BX1 - 0.3, BY1 - 0.3), (BX0 + 0.3, BY1 - 0.3)]

add_zone("GND", pcbnew.F_Cu, FULL, 0, "GND_top_pour")
add_zone("GND", pcbnew.B_Cu, FULL, 0, "GND_bottom_pour")
add_zone("GND", pcbnew.In1_Cu, FULL, 0, "GND_plane_L2")
add_zone("GND", pcbnew.In4_Cu, FULL, 0, "GND_plane_L5")
add_zone("+5V_MAIN", pcbnew.In2_Cu, FULL, 0, "5V_plane_L3")
# L4 split power plane
add_zone("SYSTEM_BAT+", pcbnew.In3_Cu, FULL, 0, "BAT_plane_L4")
add_zone("+3V3_AON", pcbnew.In3_Cu,
         [(84, 36), (134, 36), (134, 62), (84, 62)], 2, "AON_island_L4")
add_zone("+3V3_CM4", pcbnew.In3_Cu,
         [(94, 20.3), (139.7, 20.3), (139.7, 35), (94, 35)], 2, "CM4_3V3_island_L4")

RECTS = {
    "+3V3_AON": [(84, 36, 134, 62)],
    "+3V3_CM4": [(94, 20.3, 139.7, 35)],
}

def in_plane_region(netname, x, y):
    if netname in ("GND", "+5V_MAIN"):
        return True
    if netname == "SYSTEM_BAT+":
        # anywhere not carved out by the 3V3 islands
        for (x0, y0, x1, y1) in RECTS["+3V3_AON"] + RECTS["+3V3_CM4"]:
            if x0 - 1 < x < x1 + 1 and y0 - 1 < y < y1 + 1:
                return False
        return True
    for (x0, y0, x1, y1) in RECTS.get(netname, []):
        if x0 + 1 < x < x1 - 1 and y0 + 1 < y < y1 - 1:
            return True
    return False

# ---------------------------------------------------------------------------
# obstacle model for fanout
# ---------------------------------------------------------------------------
pads = []          # (x, y, half_w, half_h, netname, is_tht, clearance)
for fp in board.GetFootprints():
    for pad in fp.Pads():
        bb = pad.GetBoundingBox()
        drill = pad.GetDrillSize()
        hw = max(to_mm(bb.GetWidth()) / 2, to_mm(drill.x) / 2)
        hh = max(to_mm(bb.GetHeight()) / 2, to_mm(drill.y) / 2)
        clr = 0.0
        try:
            lc = pad.GetLocalClearance()
            if lc is not None:
                clr = to_mm(lc)
        except TypeError:
            pass
        pads.append((to_mm(pad.GetPosition().x), to_mm(pad.GetPosition().y),
                     hw, hh, pad.GetNetname(),
                     pad.GetAttribute() != pcbnew.PAD_ATTRIB_SMD,
                     max(clr, 0.25)))

vias = []          # (x, y, netname)

def via_ok(x, y, netname):
    if not (BX0 + 1 < x < BX1 - 1 and BY0 + 1 < y < BY1 - 1):
        return False
    for (px, py, hw, hh, pnet, tht, clr) in pads:
        if pnet == netname and not tht:
            continue
        # required edge clearance from via barrel (r=0.3) to pad
        need = (0.3 + clr) if pnet != netname else 0.35
        if abs(x - px) < hw + need and abs(y - py) < hh + need:
            return False
    for (vx, vy, vnet) in vias:
        if (x - vx) ** 2 + (y - vy) ** 2 < 1.0 ** 2:
            return False
    return True

def track_ok(x0, y0, x1, y1, netname):
    steps = 6
    for i in range(steps + 1):
        x = x0 + (x1 - x0) * i / steps
        y = y0 + (y1 - y0) * i / steps
        for (px, py, hw, hh, pnet, tht, clr) in pads:
            if pnet == netname:
                continue
            need = 0.2 + clr
            if abs(x - px) < hw + need and abs(y - py) < hh + need:
                return False
    return True

def add_via(x, y, netname):
    v = pcbnew.PCB_VIA(board)
    v.SetPosition(pcbnew.VECTOR2I(mm(x), mm(y)))
    v.SetDrill(mm(0.3))
    v.SetWidth(mm(0.6))
    v.SetNet(net(netname))
    board.Add(v)
    vias.append((x, y, netname))

def add_track(x0, y0, x1, y1, netname, width=0.4):
    t = pcbnew.PCB_TRACK(board)
    t.SetStart(pcbnew.VECTOR2I(mm(x0), mm(y0)))
    t.SetEnd(pcbnew.VECTOR2I(mm(x1), mm(y1)))
    t.SetWidth(mm(width))
    t.SetLayer(pcbnew.F_Cu)
    t.SetNet(net(netname))
    board.Add(t)

OFFSETS = []
for d in (1.0, 1.3, 1.7, 2.1, 2.6, 3.2):
    OFFSETS += [(d, 0), (-d, 0), (0, d), (0, -d),
                (d * 0.7, d * 0.7), (-d * 0.7, d * 0.7),
                (d * 0.7, -d * 0.7), (-d * 0.7, -d * 0.7)]

POWER_NETS = ("+5V_MAIN", "SYSTEM_BAT+", "+3V3_AON", "+3V3_CM4")
fanned, skipped = 0, 0
for fp in board.GetFootprints():
    for pad in fp.Pads():
        netname = pad.GetNetname()
        if netname not in POWER_NETS:
            continue
        if pad.GetAttribute() != pcbnew.PAD_ATTRIB_SMD:
            continue                     # THT pads reach the planes directly
        px = to_mm(pad.GetPosition().x)
        py = to_mm(pad.GetPosition().y)
        if not in_plane_region(netname, px, py):
            skipped += 1
            continue
        # reuse a same-net via right next to this pad if there is one
        done = False
        for (vx, vy, vnet) in vias:
            if vnet == netname and (px - vx) ** 2 + (py - vy) ** 2 < 1.6 ** 2 \
                    and track_ok(px, py, vx, vy, netname):
                add_track(px, py, vx, vy, netname)
                done = True
                break
        if done:
            fanned += 1
            continue
        for dx, dy in OFFSETS:
            x, y = px + dx, py + dy
            if via_ok(x, y, netname) and track_ok(px, py, x, y, netname):
                add_via(x, y, netname)
                add_track(px, py, x, y, netname)
                fanned += 1
                done = True
                break
        if not done:
            skipped += 1

# GND stitching grid (outer pours <-> inner planes)
stitch = 0
gy = BY0 + 4
while gy < BY1 - 3:
    gx = BX0 + 4
    while gx < BX1 - 3:
        if via_ok(gx, gy, "GND"):
            add_via(gx, gy, "GND")
            stitch += 1
        gx += 7.0
    gy += 7.0

# GND fanout for SMD GND pads that the top pour may not reach cleanly
gnd_fan = 0
for fp in board.GetFootprints():
    for pad in fp.Pads():
        if pad.GetNetname() != "GND":
            continue
        if pad.GetAttribute() != pcbnew.PAD_ATTRIB_SMD:
            continue
        px = to_mm(pad.GetPosition().x)
        py = to_mm(pad.GetPosition().y)
        near = any(vnet == "GND" and (px - vx) ** 2 + (py - vy) ** 2 < 3.5 ** 2
                   for (vx, vy, vnet) in vias)
        if near:
            continue
        for dx, dy in OFFSETS:
            x, y = px + dx, py + dy
            if via_ok(x, y, "GND") and track_ok(px, py, x, y, "GND"):
                add_via(x, y, "GND")
                add_track(px, py, x, y, "GND")
                gnd_fan += 1
                break

print("power fanout: %d pads, %d left for manual routing" % (fanned, skipped))
print("GND stitching vias: %d grid + %d pad fanouts" % (stitch, gnd_fan))

# ---------------------------------------------------------------------------
# design rules + fill + save
# ---------------------------------------------------------------------------
ds = board.GetDesignSettings()
ds.m_MinThroughDrill = mm(0.2)
ds.m_HoleToHoleMin = mm(0.2)

filler = pcbnew.ZONE_FILLER(board)
filler.Fill(board.Zones())
pcbnew.SaveBoard(BOARD_PATH, board)
print("saved", BOARD_PATH)

# net classes in the project file (informational until routing)
with open(PRO_PATH) as f:
    pro = json.load(f)
ns = pro.setdefault("net_settings", {})
ns["classes"] = [
    {"name": "Default", "clearance": 0.2, "track_width": 0.25,
     "via_diameter": 0.6, "via_drill": 0.3,
     "diff_pair_width": 0.2, "diff_pair_gap": 0.25},
    {"name": "Power", "clearance": 0.2, "track_width": 0.5,
     "via_diameter": 0.7, "via_drill": 0.4},
    {"name": "Power_Heavy", "clearance": 0.2, "track_width": 1.5,
     "via_diameter": 0.8, "via_drill": 0.5},
    {"name": "USB_90R_Diff", "clearance": 0.2, "track_width": 0.25,
     "diff_pair_width": 0.25, "diff_pair_gap": 0.2},
    {"name": "Diff_100R", "clearance": 0.2, "track_width": 0.2,
     "diff_pair_width": 0.2, "diff_pair_gap": 0.25},
]
ns["netclass_patterns"] = [
    {"netclass": "Power_Heavy", "pattern": "SYSTEM_BAT+"},
    {"netclass": "Power_Heavy", "pattern": "BAT_RAW+"},
    {"netclass": "Power_Heavy", "pattern": "+5V_MAIN"},
    {"netclass": "Power", "pattern": "+3V3*"},
    {"netclass": "Power", "pattern": "*VBUS*"},
    {"netclass": "USB_90R_Diff", "pattern": "USB_*"},
    {"netclass": "USB_90R_Diff", "pattern": "*/USB*_D[PM]"},
    {"netclass": "Diff_100R", "pattern": "DSI1_*"},
    {"netclass": "Diff_100R", "pattern": "ETH_P*"},
]
with open(PRO_PATH, "w") as f:
    json.dump(pro, f, indent=2)
print("net classes written to project (final widths after fab stackup)")
