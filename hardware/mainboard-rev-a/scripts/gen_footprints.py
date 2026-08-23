#!/usr/bin/env python3
"""Generate boompi.pretty footprints for the ADI LQFN packages.

LT8645S: LQFN-32 6mm x 4mm  - land pattern taken from the LT8645S datasheet
         "Suggested PCB Layout" (LTC DWG 05-08-1512 Rev C, datasheet p.28):
         perimeter pads 0.25 x 0.70 on 0.50mm pitch, copper extent
         6.50 x 4.50, exposed pad = 6 segments (2 x 3), D1=2.45 E1=4.45,
         segments 1.125 wide, 1.355/1.34/1.355 tall with 0.20 gaps.
LT8609S: LQFN-16 3mm x 3mm  - standard 16L 3x3 QFN land pattern
         (VERIFY against LTC DWG 05-08-1516 Rev B before footprint lock).
"""

import os

HERE = os.path.dirname(os.path.abspath(__file__))
PROJ = os.path.dirname(HERE)
OUT = os.path.join(PROJ, "libraries", "boompi.pretty")
os.makedirs(OUT, exist_ok=True)

def pad(num, x, y, w, h):
    return ('    (pad "%s" smd roundrect (at %.4g %.4g) (size %.4g %.4g) '
            '(layers "F.Cu" "F.Paste" "F.Mask") (roundrect_rratio 0.2))'
            % (num, x, y, w, h))

def footprint(name, descr, body_w, body_h, pads, courtyard, pin1_xy):
    cw, ch = courtyard
    bw, bh = body_w / 2, body_h / 2
    lines = ['(footprint "%s"' % name,
             '  (version 20240108)',
             '  (generator "boompi_gen")',
             '  (layer "F.Cu")',
             '  (descr "%s")' % descr,
             '  (attr smd)',
             '  (property "Reference" "REF**" (at 0 %.4g 0) (layer "F.SilkS")'
             ' (effects (font (size 1 1) (thickness 0.15))))' % (-ch - 1.2),
             '  (property "Value" "%s" (at 0 %.4g 0) (layer "F.Fab")'
             ' (effects (font (size 1 1) (thickness 0.15))))' % (name, ch + 1.2)]
    # fab outline
    lines.append('  (fp_rect (start %.4g %.4g) (end %.4g %.4g)'
                 ' (stroke (width 0.1) (type default)) (fill none) (layer "F.Fab"))'
                 % (-bw, -bh, bw, bh))
    # courtyard
    lines.append('  (fp_rect (start %.4g %.4g) (end %.4g %.4g)'
                 ' (stroke (width 0.05) (type default)) (fill none) (layer "F.CrtYd"))'
                 % (-cw, -ch, cw, ch))
    # pin 1 markers
    px, py = pin1_xy
    lines.append('  (fp_circle (center %.4g %.4g) (end %.4g %.4g)'
                 ' (stroke (width 0.2) (type default)) (fill solid) (layer "F.SilkS"))'
                 % (px, py, px + 0.1, py))
    lines.append('  (fp_circle (center %.4g %.4g) (end %.4g %.4g)'
                 ' (stroke (width 0.1) (type default)) (fill solid) (layer "F.Fab"))'
                 % (px * 0.6, py * 0.6, px * 0.6 + 0.25, py * 0.6))
    lines += pads
    lines.append(')')
    return "\n".join(lines) + "\n"

# --------------------------------------------------------------------------
# LT8645S LQFN-32 (body 4mm wide x 6mm tall, pin 1 top-left going down)
# --------------------------------------------------------------------------
pads = []
# left column: pins 1-10, x=-1.90, y=-2.25 .. +2.25
for i in range(10):
    pads.append(pad(i + 1, -1.90, -2.25 + 0.5 * i, 0.70, 0.25))
# bottom row: pins 11-16, y=+2.90, x=-1.25 .. +1.25
for i in range(6):
    pads.append(pad(11 + i, -1.25 + 0.5 * i, 2.90, 0.25, 0.70))
# right column: pins 17-26, x=+1.90, y=+2.25 .. -2.25
for i in range(10):
    pads.append(pad(17 + i, 1.90, 2.25 - 0.5 * i, 0.70, 0.25))
# top row: pins 27-32, y=-2.90, x=+1.25 .. -1.25
for i in range(6):
    pads.append(pad(27 + i, 1.25 - 0.5 * i, -2.90, 0.25, 0.70))
# exposed pad segments 33-38 (2 cols x 3 rows)
ep = [(33, -0.6625, -1.5475, 1.125, 1.355), (34, 0.6625, -1.5475, 1.125, 1.355),
      (35, -0.6625, 0.0, 1.125, 1.34), (36, 0.6625, 0.0, 1.125, 1.34),
      (37, -0.6625, 1.5475, 1.125, 1.355), (38, 0.6625, 1.5475, 1.125, 1.355)]
for num, x, y, w, h in ep:
    pads.append(pad(num, x, y, w, h))
open(os.path.join(OUT, "LT8645S_LQFN32_4x6mm.kicad_mod"), "w").write(footprint(
    "LT8645S_LQFN32_4x6mm",
    "ADI LQFN-32 6x4mm (LTC DWG 05-08-1512 Rev C), LT8645S/LT8646S. "
    "Land pattern per datasheet suggested PCB layout. Add thermal vias in EP.",
    4.0, 6.0, pads, (2.55, 3.55), (-2.6, -2.25)))

# --------------------------------------------------------------------------
# LT8609S LQFN-16 (3mm x 3mm, 4 pads/side, 0.5mm pitch)
# TODO: verify against LTC DWG 05-08-1516 Rev B before footprint lock.
# --------------------------------------------------------------------------
pads = []
pos = [-0.75, -0.25, 0.25, 0.75]
for i in range(4):   # left, pins 1-4 top->bottom
    pads.append(pad(i + 1, -1.40, pos[i], 0.70, 0.25))
for i in range(4):   # bottom, pins 5-8 left->right
    pads.append(pad(5 + i, pos[i], 1.40, 0.25, 0.70))
for i in range(4):   # right, pins 9-12 bottom->top
    pads.append(pad(9 + i, 1.40, -pos[i], 0.70, 0.25))
for i in range(4):   # top, pins 13-16 right->left
    pads.append(pad(13 + i, -pos[i], -1.40, 0.25, 0.70))
pads.append(pad(17, 0, 0, 1.70, 1.70))
open(os.path.join(OUT, "LT8609S_LQFN16_3x3mm.kicad_mod"), "w").write(footprint(
    "LT8609S_LQFN16_3x3mm",
    "ADI LQFN-16 3x3mm (LTC DWG 05-08-1516 Rev B), LT8609S. Generic 3x3 QFN16 "
    "land pattern - VERIFY against the datasheet drawing before footprint lock. "
    "Add thermal vias in EP.",
    3.0, 3.0, pads, (2.05, 2.05), (-2.1, -0.75)))

print("wrote LQFN footprints into", OUT)
