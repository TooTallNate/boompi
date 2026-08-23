#!/usr/bin/env python3
"""Minimal KiCad 10 schematic generator for the Boompi mainboard Rev A project.

Generates hierarchical .kicad_sch files from a declarative design description.
Connectivity style: every symbol pin gets a short wire stub terminated by a
global label, local label, power symbol, or no-connect marker.  This is a
machine-generated "netlist style" schematic intended to be ERC-clean and to
serve as the starting point for hand-tuned layout/graphics work in eeschema.

Symbols are extracted from the official KiCad 10 libraries (and from the
Raspberry Pi CM4IO reference design for the CM4 module / magjack symbols),
flattened (extends resolved) and embedded into each sheet's lib_symbols.
"""

import re
import uuid
import os

KICAD_SYMBOL_DIR = "/Applications/KiCad/KiCad.app/Contents/SharedSupport/symbols"
SCH_VERSION = "20260306"
GENERATOR_VERSION = "10.0"
NS = uuid.UUID("a70112aa-0000-4000-8000-boompirevaaa".replace("boompirevaaa", "9e1a2b3c4d5e"))

# ---------------------------------------------------------------------------
# S-expression parsing / serialization
# ---------------------------------------------------------------------------

class Q(str):
    """A quoted string atom."""

def tokenize(s):
    i, n = 0, len(s)
    while i < n:
        c = s[i]
        if c.isspace():
            i += 1
        elif c in "()":
            yield c
            i += 1
        elif c == '"':
            j = i + 1
            buf = []
            while s[j] != '"':
                if s[j] == "\\":
                    nxt = s[j + 1]
                    buf.append({"n": "\n", "t": "\t", '"': '"', "\\": "\\"}.get(nxt, "\\" + nxt))
                    j += 2
                else:
                    buf.append(s[j])
                    j += 1
            yield Q("".join(buf))
            i = j + 1
        else:
            j = i
            while j < n and not s[j].isspace() and s[j] not in '()"':
                j += 1
            yield s[i:j]
            i = j

def parse(s):
    stack = [[]]
    for tok in tokenize(s):
        if tok == "(":
            stack.append([])
        elif tok == ")":
            done = stack.pop()
            stack[-1].append(done)
        else:
            stack[-1].append(tok)
    return stack[0]

def dump(node, indent=0):
    if isinstance(node, Q):
        esc = (node.replace("\\", "\\\\").replace('"', '\\"')
                   .replace("\n", "\\n").replace("\t", "\\t"))
        return '"%s"' % esc
    if isinstance(node, str):
        return node
    pad = "\t" * indent
    if all(not isinstance(x, list) for x in node):
        return "(" + " ".join(dump(x) for x in node) + ")"
    parts = []
    head = []
    i = 0
    while i < len(node) and not isinstance(node[i], list):
        head.append(dump(node[i]))
        i += 1
    out = "(" + " ".join(head)
    for x in node[i:]:
        if isinstance(x, list):
            out += "\n" + pad + "\t" + dump(x, indent + 1)
        else:
            out += " " + dump(x)
    out += "\n" + pad + ")"
    return out

def find_all(node, key):
    return [x for x in node if isinstance(x, list) and x and x[0] == key]

def find_one(node, key):
    r = find_all(node, key)
    return r[0] if r else None

# ---------------------------------------------------------------------------
# Symbol library handling
# ---------------------------------------------------------------------------

_libfile_cache = {}

def _load_lib_file(path):
    if path not in _libfile_cache:
        _libfile_cache[path] = parse(open(path).read())[0]
    return _libfile_cache[path]

def _strip_ids(node):
    """Remove legacy (id N) tokens and normalize bare 'hide' atoms."""
    if not isinstance(node, list):
        return
    node[:] = [x for x in node if not (isinstance(x, list) and x and x[0] == "id")]
    for i, x in enumerate(node):
        if isinstance(x, str) and not isinstance(x, Q) and x == "hide" and i > 0:
            node[i] = ["hide", "yes"]
        else:
            _strip_ids(x)

class SymbolDef:
    def __init__(self, sexpr, lib_id):
        self.sexpr = sexpr          # full (symbol ...) sexpr, renamed to lib_id
        self.lib_id = lib_id
        self.pins = []              # (number, name, type, x, y, angle)
        self._collect_pins()

    def _collect_pins(self):
        base = self.lib_id.split(":", 1)[1]
        for sub in find_all(self.sexpr, "symbol"):
            subname = str(sub[1])
            m = re.match(re.escape(base) + r"_(\d+)_(\d+)$", subname)
            unit = int(m.group(1)) if m else 0
            for pin in find_all(sub, "pin"):
                ptype = str(pin[1])
                at = find_one(pin, "at")
                x, y = float(at[1]), float(at[2])
                ang = int(float(at[3])) if len(at) > 3 else 0
                name = str(find_one(pin, "name")[1])
                num = str(find_one(pin, "number")[1])
                self.pins.append(dict(num=num, name=name, type=ptype,
                                      x=x, y=y, angle=ang, unit=unit))

    def unit_pins(self, unit):
        return [p for p in self.pins if p["unit"] in (0, unit)]

    def units(self):
        us = sorted({p["unit"] for p in self.pins if p["unit"] != 0})
        return us or [1]

    def bbox(self, unit):
        pins = self.unit_pins(unit)
        if not pins:
            return (-5, -5, 5, 5)
        xs = [p["x"] for p in pins]
        ys = [p["y"] for p in pins]
        return (min(xs), min(ys), max(xs), max(ys))

def extract_symbol(lib_name, sym_name, new_lib_id=None, lib_path=None):
    """Extract a symbol from a library file, flattening derived symbols."""
    path = lib_path or os.path.join(KICAD_SYMBOL_DIR, lib_name + ".kicad_sym")
    root = _load_lib_file(path)
    container = root
    if root and str(root[0]) == "kicad_sch":
        container = find_one(root, "lib_symbols")
    defs = {str(s[1]): s for s in find_all(container, "symbol")}
    if sym_name not in defs:
        # embedded schematic lib_symbols use "LIB:NAME" ids
        alt = [k for k in defs if k.endswith(":" + sym_name)]
        if not alt:
            raise KeyError("symbol %s not in %s" % (sym_name, path))
        sym_name = alt[0]
    node = defs[sym_name]

    ext = find_one(node, "extends")
    if ext:
        parent_name = str(ext[1])
        parent = defs[parent_name]
        merged = _clone(parent)
        # child properties override parent's
        child_props = {str(p[1]): p for p in find_all(node, "property")}
        merged[:] = [x for x in merged
                     if not (isinstance(x, list) and x and x[0] == "property"
                             and str(x[1]) in child_props)]
        idx = 2
        for pname, prop in child_props.items():
            merged.insert(idx, _clone(prop))
            idx += 1
        node = merged
        old_base = parent_name
    else:
        node = _clone(node)
        old_base = sym_name

    lib_id = new_lib_id or ("%s:%s" % (lib_name, sym_name.split(":")[-1]))
    new_base = lib_id.split(":", 1)[1]
    node[1] = Q(lib_id)
    old_plain = old_base.split(":")[-1]
    for sub in find_all(node, "symbol"):
        sub[1] = Q(re.sub("^" + re.escape(old_plain), new_base, str(sub[1])))
    _strip_ids(node)
    return SymbolDef(node, lib_id)

def _clone(node):
    if isinstance(node, list):
        return [_clone(x) for x in node]
    return node

def patch_pin_types(symdef, num_to_type):
    """Change the electrical type of specific pins (by pin number)."""
    for sub in find_all(symdef.sexpr, "symbol"):
        for pin in find_all(sub, "pin"):
            num = str(find_one(pin, "number")[1])
            if num in num_to_type:
                pin[1] = num_to_type[num]
    for p in symdef.pins:
        if p["num"] in num_to_type:
            p["type"] = num_to_type[p["num"]]

# ---------------------------------------------------------------------------
# Custom symbol builders
# ---------------------------------------------------------------------------

def make_power_symbol(rail):
    """Clone power:+5V graphics, rename to boompi:<rail>."""
    sd = extract_symbol("power", "+5V", new_lib_id="boompi:" + rail)
    node = sd.sexpr
    for prop in find_all(node, "property"):
        if str(prop[1]) == "Value":
            prop[2] = Q(rail)
    for sub in find_all(node, "symbol"):
        for pin in find_all(sub, "pin"):
            nm = find_one(pin, "name")
            nm[1] = Q(rail)
    return SymbolDef(node, "boompi:" + rail)

def make_box_symbol(name, value, pins_left, pins_right, footprint="",
                    description="", datasheet=""):
    """Build a simple rectangular IC symbol.

    pins_left/right: list of (number, name, type) placed top to bottom.
    """
    pitch = 2.54
    n_l, n_r = len(pins_left), len(pins_right)
    rows = max(n_l, n_r)
    height = (rows + 1) * pitch
    half_h = round(height / 2 / 1.27) * 1.27
    width = 20.32
    half_w = width / 2
    lib_id = "boompi:" + name

    def pin_sexpr(num, pname, ptype, x, y, ang):
        return ["pin", ptype, "line",
                ["at", fmt(x), fmt(y), str(ang)], ["length", "3.81"],
                ["name", Q(pname), ["effects", ["font", ["size", "1.27", "1.27"]]]],
                ["number", Q(num), ["effects", ["font", ["size", "1.27", "1.27"]]]]]

    body = ["symbol", Q(name + "_0_1"),
            ["rectangle", ["start", fmt(-half_w), fmt(half_h)],
             ["end", fmt(half_w), fmt(-half_h)],
             ["stroke", ["width", "0.254"], ["type", "default"]],
             ["fill", ["type", "background"]]]]
    unit = ["symbol", Q(name + "_1_1")]
    for i, (num, pname, ptype) in enumerate(pins_left):
        y = half_h - pitch * (i + 1)
        unit.append(pin_sexpr(num, pname, ptype, -half_w - 3.81, y, 0))
    for i, (num, pname, ptype) in enumerate(pins_right):
        y = half_h - pitch * (i + 1)
        unit.append(pin_sexpr(num, pname, ptype, half_w + 3.81, y, 180))

    node = ["symbol", Q(lib_id),
            ["pin_names", ["offset", "1.016"]],
            ["exclude_from_sim", "no"], ["in_bom", "yes"], ["on_board", "yes"],
            ["property", Q("Reference"), Q("U"),
             ["at", "0", fmt(half_h + 2.54), "0"],
             ["effects", ["font", ["size", "1.27", "1.27"]]]],
            ["property", Q("Value"), Q(value),
             ["at", "0", fmt(-half_h - 2.54), "0"],
             ["effects", ["font", ["size", "1.27", "1.27"]]]],
            ["property", Q("Footprint"), Q(footprint),
             ["at", "0", "0", "0"],
             ["effects", ["font", ["size", "1.27", "1.27"]], ["hide", "yes"]]],
            ["property", Q("Datasheet"), Q(datasheet),
             ["at", "0", "0", "0"],
             ["effects", ["font", ["size", "1.27", "1.27"]], ["hide", "yes"]]],
            ["property", Q("Description"), Q(description),
             ["at", "0", "0", "0"],
             ["effects", ["font", ["size", "1.27", "1.27"]], ["hide", "yes"]]],
            body, unit]
    return SymbolDef(node, lib_id)

def fmt(v):
    if isinstance(v, str):
        return v
    v = round(v, 4)
    if v == int(v):
        return str(int(v))
    return ("%.4f" % v).rstrip("0").rstrip(".")

# ---------------------------------------------------------------------------
# Sheet / design model
# ---------------------------------------------------------------------------

PAPER_SIZES = {"A4": (297, 210), "A3": (420, 297), "A2": (594, 420)}

class Part:
    def __init__(self, ref, symdef, value=None, footprint=None, netmap=None,
                 at=None, mpn=None, dnp=False, fields=None, stub=2.54):
        self.ref = ref
        self.symdef = symdef
        self.value = value or ref
        self.footprint = footprint or ""
        self.netmap = netmap or {}
        self.at = at          # dict unit -> (x, y) or None for autoplace
        self.mpn = mpn
        self.dnp = dnp
        self.fields = fields or {}
        self.stub = stub

class Sheet:
    def __init__(self, name, filename, paper="A3", title=""):
        self.name = name
        self.filename = filename
        self.paper = paper
        self.title = title or name
        self.parts = []
        self.texts = []          # (string, x, y, size)
        self.rail_flags = []     # (rail, is_gnd)
        self.local_flags = []    # local net names needing PWR_FLAG
        self.uuid = str(uuid.uuid5(NS, "sheet:" + filename))
        self.symuuid = str(uuid.uuid5(NS, "sheetsym:" + filename))

    def part(self, ref, symdef, **kw):
        p = Part(ref, symdef, **kw)
        self.parts.append(p)
        return p

    def text(self, s, x, y, size=1.5):
        self.texts.append((s, x, y, size))

    def flag(self, rail):
        self.rail_flags.append((rail, rail == "GND"))

    def flag_local(self, net):
        self.local_flags.append(net)

class Design:
    def __init__(self, project, root_uuid, outdir):
        self.project = project
        self.root_uuid = root_uuid
        self.outdir = outdir
        self.sheets = []
        self.power_syms = {}     # rail -> SymbolDef
        self._pwr_count = 0
        self._flg_count = 0
        self._uuid_count = 0

    def sheet(self, name, filename, **kw):
        s = Sheet(name, filename, **kw)
        self.sheets.append(s)
        return s

    def uid(self, tag):
        self._uuid_count += 1
        return str(uuid.uuid5(NS, "%s#%d" % (tag, self._uuid_count)))

    def power_symdef(self, rail):
        if rail == "GND":
            key = "power:GND"
            if key not in self.power_syms:
                self.power_syms[key] = extract_symbol("power", "GND")
            return self.power_syms[key]
        if rail == "PWR_FLAG":
            key = "power:PWR_FLAG"
            if key not in self.power_syms:
                self.power_syms[key] = extract_symbol("power", "PWR_FLAG")
            return self.power_syms[key]
        key = "boompi:" + rail
        if key not in self.power_syms:
            self.power_syms[key] = make_power_symbol(rail)
        return self.power_syms[key]

# ---------------------------------------------------------------------------
# Emission
# ---------------------------------------------------------------------------

OUT_DIRS = {0: (-1, 0), 90: (0, 1), 180: (1, 0), 270: (0, -1)}
# lib pin angle -> outward unit vector in *sheet* coords (y down)

def _snap(v):
    return round(v / 1.27) * 1.27

class Emitter:
    def __init__(self, design):
        self.d = design
        self.global_labels = {}      # name -> count
        self.local_labels = {}       # (sheet, name) -> count
        self.point_nets = {}         # (sheet, x, y) -> netkey
        self.errors = []

    # -- helpers ------------------------------------------------------------

    def _claim(self, sheet, x, y, netkey):
        key = (sheet.filename, round(x, 2), round(y, 2))
        prev = self.point_nets.get(key)
        if prev is not None and prev != netkey:
            self.errors.append("collision at %s %.2f,%.2f: %s vs %s"
                               % (sheet.filename, x, y, prev, netkey))
        self.point_nets[key] = netkey

    def wire(self, out, sheet, x1, y1, x2, y2, netkey):
        self._claim(sheet, x1, y1, netkey)
        self._claim(sheet, x2, y2, netkey)
        out.append(["wire", ["pts", ["xy", fmt(x1), fmt(y1)], ["xy", fmt(x2), fmt(y2)]],
                    ["stroke", ["width", "0"], ["type", "default"]],
                    ["uuid", Q(self.d.uid("wire"))]])

    def glabel(self, out, sheet, name, x, y, ang, shape="bidirectional"):
        self.global_labels[name] = self.global_labels.get(name, 0) + 1
        self._claim(sheet, x, y, "g:" + name)
        just = {0: "left", 90: "left", 180: "right", 270: "right"}[ang]
        out.append(["global_label", Q(name), ["shape", shape],
                    ["at", fmt(x), fmt(y), str(ang)],
                    ["effects", ["font", ["size", "1.27", "1.27"]], ["justify", just]],
                    ["uuid", Q(self.d.uid("glabel"))],
                    ["property", Q("Intersheetrefs"), Q("${INTERSHEET_REFS}"),
                     ["at", fmt(x), fmt(y), "0"],
                     ["effects", ["font", ["size", "1.27", "1.27"]], ["hide", "yes"]]]])

    def llabel(self, out, sheet, name, x, y, ang):
        key = (sheet.filename, name)
        self.local_labels[key] = self.local_labels.get(key, 0) + 1
        self._claim(sheet, x, y, "l:" + name)
        just = {0: ["left"], 90: ["left"], 180: ["right"], 270: ["right"]}[ang]
        out.append(["label", Q(name), ["at", fmt(x), fmt(y), str(ang)],
                    ["effects", ["font", ["size", "1.27", "1.27"]],
                     ["justify"] + just],
                    ["uuid", Q(self.d.uid("label"))]])

    def noconn(self, out, sheet, x, y):
        out.append(["no_connect", ["at", fmt(x), fmt(y)],
                    ["uuid", Q(self.d.uid("nc"))]])

    def power_sym(self, out, sheet, rail, x, y, ang):
        symdef = self.d.power_symdef(rail)
        self.d._pwr_count += 1
        ref = "#PWR%04d" % self.d._pwr_count
        self._claim(sheet, x, y, "p:" + rail)
        out.append(self._symbol_instance(sheet, symdef, ref, rail, x, y,
                                         unit=1, angle=ang, power=True))
        return symdef

    def pwr_flag(self, out, sheet, x, y, ang, netkey):
        symdef = self.d.power_symdef("PWR_FLAG")
        self.d._flg_count += 1
        ref = "#FLG%04d" % self.d._flg_count
        self._claim(sheet, x, y, netkey)
        out.append(self._symbol_instance(sheet, symdef, ref, "PWR_FLAG", x, y,
                                         unit=1, angle=ang, power=True))
        return symdef

    def _symbol_instance(self, sheet, symdef, ref, value, x, y, unit=1,
                         angle=0, power=False, part=None):
        path = "/%s/%s" % (self.d.root_uuid, sheet.symuuid)
        minx, miny, maxx, maxy = symdef.bbox(unit)
        props = []
        ref_y = y - maxy - 2.54 if not power else y - 3.81
        val_y = y - miny + 2.54 if not power else y - 6.35
        hide_val = False
        props.append(["property", Q("Reference"), Q(ref),
                      ["at", fmt(x), fmt(ref_y), "0"],
                      ["effects", ["font", ["size", "1.27", "1.27"]]] +
                      ([["hide", "yes"]] if power else [])])
        props.append(["property", Q("Value"), Q(value),
                      ["at", fmt(x), fmt(val_y), "0"],
                      ["effects", ["font", ["size", "1.27", "1.27"]]] +
                      ([["hide", "yes"]] if hide_val else [])])
        fp = part.footprint if part else ""
        ds = ""
        props.append(["property", Q("Footprint"), Q(fp),
                      ["at", fmt(x), fmt(y), "0"],
                      ["effects", ["font", ["size", "1.27", "1.27"]], ["hide", "yes"]]])
        props.append(["property", Q("Datasheet"), Q(ds),
                      ["at", fmt(x), fmt(y), "0"],
                      ["effects", ["font", ["size", "1.27", "1.27"]], ["hide", "yes"]]])
        if part and part.mpn:
            props.append(["property", Q("MPN"), Q(part.mpn),
                          ["at", fmt(x), fmt(y), "0"],
                          ["effects", ["font", ["size", "1.27", "1.27"]], ["hide", "yes"]]])
        if part:
            for k, v in part.fields.items():
                props.append(["property", Q(k), Q(v),
                              ["at", fmt(x), fmt(y), "0"],
                              ["effects", ["font", ["size", "1.27", "1.27"]], ["hide", "yes"]]])
        node = ["symbol",
                ["lib_id", Q(symdef.lib_id)],
                ["at", fmt(x), fmt(y), str(angle)],
                ["unit", str(unit)],
                ["exclude_from_sim", "no"], ["in_bom", "no" if power else "yes"],
                ["on_board", "yes"],
                ["dnp", "yes" if (part and part.dnp) else "no"],
                ["uuid", Q(self.d.uid("sym:" + ref + ":" + str(unit)))]]
        node += props
        node.append(["instances",
                     ["project", Q(self.d.project),
                      ["path", Q(path),
                       ["reference", Q(ref)], ["unit", str(unit)]]]])
        return node

    # -- part placement ------------------------------------------------------

    def place_part(self, out, sheet, part, unit, x, y):
        symdef = part.symdef
        out.append(self._symbol_instance(sheet, symdef, part.ref, part.value,
                                         x, y, unit=unit, part=part))
        # group pins by connection point (stacked pins)
        groups = {}
        for pin in symdef.unit_pins(unit):
            cx, cy = x + pin["x"], y - pin["y"]
            groups.setdefault((round(cx, 2), round(cy, 2)), []).append(pin)
        for (cx, cy), pins in sorted(groups.items()):
            specs = set()
            for p in pins:
                spec = part.netmap.get(p["num"])
                if spec is None:
                    self.errors.append("%s: pin %s (%s) has no net spec"
                                       % (part.ref, p["num"], p["name"]))
                    continue
                specs.add(spec if isinstance(spec, str) else tuple(spec))
            if not specs:
                continue
            if len(specs) > 1:
                self.errors.append("%s: stacked pins at %s disagree: %s"
                                   % (part.ref, (cx, cy), specs))
            spec = specs.pop()
            pin = pins[0]
            dx, dy = OUT_DIRS[pin["angle"]]
            if spec == "NC":
                self.noconn(out, sheet, cx, cy)
                continue
            kind, name = spec
            stub = part.stub
            ex, ey = cx + dx * stub, cy + dy * stub
            ang = {(1, 0): 0, (-1, 0): 180, (0, -1): 90, (0, 1): 270}[(dx, dy)]
            if kind == "p":
                netkey = "p:" + name
                self.wire(out, sheet, cx, cy, ex, ey, netkey)
                rot = self._power_rot(name, (dx, dy))
                self.power_sym(out, sheet, name, ex, ey, rot)
            elif kind == "g":
                netkey = "g:" + name
                self.wire(out, sheet, cx, cy, ex, ey, netkey)
                self.glabel(out, sheet, name, ex, ey, ang)
            elif kind == "l":
                netkey = "l:" + name
                self.wire(out, sheet, cx, cy, ex, ey, netkey)
                self.llabel(out, sheet, name, ex, ey, ang)
            else:
                self.errors.append("%s: bad spec %r" % (part.ref, spec))

    @staticmethod
    def _power_rot(rail, outward):
        if rail == "GND":
            return {(0, 1): 0, (0, -1): 180, (1, 0): 270, (-1, 0): 90}[outward]
        return {(0, -1): 0, (0, 1): 180, (1, 0): 90, (-1, 0): 270}[outward]

    # -- sheet emission -------------------------------------------------------

    def emit_sheet(self, sheet):
        out = []
        used_symdefs = {}
        # autoplacement cursor
        pw, ph = PAPER_SIZES[sheet.paper]
        margin_x, margin_y = 30.0, 35.0
        col_x = margin_x
        cur_y = margin_y
        col_w = 0.0

        def alloc(w, h):
            nonlocal col_x, cur_y, col_w
            if cur_y + h > ph - 20 and cur_y > margin_y:
                col_x += col_w + 18.0
                cur_y = margin_y
                col_w = 0.0
            x = col_x
            y = cur_y
            cur_y += h + 14.0
            col_w = max(col_w, w)
            return x, y

        for part in sheet.parts:
            used_symdefs[part.symdef.lib_id] = part.symdef
            for unit in part.symdef.units():
                minx, miny, maxx, maxy = part.symdef.bbox(unit)
                w = (maxx - minx) + 2 * 26.0
                h = (maxy - miny) + 10.0
                if part.at and unit in part.at:
                    ox, oy = part.at[unit]
                else:
                    ax, ay = alloc(w, h)
                    ox = ax + 26.0 - minx
                    oy = ay + maxy + 5.0
                ox, oy = _snap(ox), _snap(oy)
                self.place_part(out, sheet, part, unit, ox, oy)

        # rail flags
        fx = pw - 130.0
        fy = ph - 28.0
        for rail, is_gnd in sheet.rail_flags:
            fx0, fy0 = _snap(fx), _snap(fy)
            if is_gnd:
                self.wire(out, sheet, fx0, fy0, fx0, fy0 - 2.54, "p:GND")
                self.power_sym(out, sheet, "GND", fx0, fy0, 0)
                self.pwr_flag(out, sheet, fx0, fy0 - 2.54, 0, "p:GND")
            else:
                self.wire(out, sheet, fx0, fy0, fx0, fy0 + 2.54, "p:" + rail)
                self.power_sym(out, sheet, rail, fx0, fy0, 0)
                self.pwr_flag(out, sheet, fx0, fy0 + 2.54, 180, "p:" + rail)
            fx += 15.24
        for net in sheet.local_flags:
            fx0, fy0 = _snap(fx), _snap(fy)
            self.wire(out, sheet, fx0, fy0, fx0, fy0 + 2.54, "l:" + net)
            self.pwr_flag(out, sheet, fx0, fy0, 0, "l:" + net)
            self.llabel(out, sheet, net, fx0, fy0 + 2.54, 270)
            fx += 15.24

        for s, x, y, size in sheet.texts:
            out.append(["text", Q(s), ["exclude_from_sim", "yes"],
                        ["at", fmt(x), fmt(y), "0"],
                        ["effects", ["font", ["size", fmt(size), fmt(size)]],
                         ["justify", "left", "top"]],
                        ["uuid", Q(self.d.uid("text"))]])

        # collect power symbol defs used
        for rail in list(self.d.power_syms):
            pass
        return out, used_symdefs

    def emit_all(self):
        d = self.d
        os.makedirs(d.outdir, exist_ok=True)
        sheet_bodies = {}
        for sheet in d.sheets:
            body, used = self.emit_sheet(sheet)
            sheet_bodies[sheet.filename] = (sheet, body, used)

        # power/flag symbols may have been created during emission; collect per sheet
        for filename, (sheet, body, used) in sheet_bodies.items():
            for item in body:
                if isinstance(item, list) and item[0] == "symbol":
                    lid = str(find_one(item, "lib_id")[1])
                    if lid not in used:
                        for sd in d.power_syms.values():
                            if sd.lib_id == lid:
                                used[lid] = sd
            self._write_sheet_file(sheet, body, used)

        self._write_root()
        self._write_support_files(sheet_bodies)
        self._report()

    def _write_support_files(self, sheet_bodies):
        """Write libraries/boompi.kicad_sym + sym-lib-table + fp-lib-table."""
        d = self.d
        libdir = os.path.join(d.outdir, "libraries")
        os.makedirs(libdir, exist_ok=True)
        # collect every boompi:* symbol definition used anywhere
        boompi_syms = {}
        for _, (_sheet, _body, used) in sheet_bodies.items():
            for lid, sd in used.items():
                if lid.startswith("boompi:"):
                    boompi_syms[lid] = sd
        for sd in d.power_syms.values():
            if sd.lib_id.startswith("boompi:"):
                boompi_syms[sd.lib_id] = sd
        lib = ["kicad_symbol_lib",
               ["version", "20241209"],
               ["generator", Q("boompi_gen")],
               ["generator_version", Q("1.0")]]
        for lid in sorted(boompi_syms):
            node = _clone(boompi_syms[lid].sexpr)
            node[1] = Q(lid.split(":", 1)[1])   # strip lib prefix in lib file
            lib.append(node)
        open(os.path.join(libdir, "boompi.kicad_sym"), "w").write(dump(lib) + "\n")

        open(os.path.join(d.outdir, "sym-lib-table"), "w").write(
            '(sym_lib_table\n  (version 7)\n'
            '  (lib (name "boompi")(type "KiCad")'
            '(uri "${KIPRJMOD}/libraries/boompi.kicad_sym")(options "")'
            '(descr "Boompi custom symbols (generated)"))\n)\n')
        open(os.path.join(d.outdir, "fp-lib-table"), "w").write(
            '(fp_lib_table\n  (version 7)\n'
            '  (lib (name "boompi")(type "KiCad")'
            '(uri "${KIPRJMOD}/libraries/boompi.pretty")(options "")'
            '(descr "Boompi custom footprints"))\n'
            '  (lib (name "CM4IO")(type "KiCad")'
            '(uri "${KIPRJMOD}/libraries/CM4IO.pretty")(options "")'
            '(descr "Footprints from the Raspberry Pi CM4IO reference design"))\n)\n')

    def _write_sheet_file(self, sheet, body, used):
        libs = ["lib_symbols"]
        for lid in sorted(used):
            libs.append(used[lid].sexpr)
        doc = ["kicad_sch",
               ["version", SCH_VERSION],
               ["generator", Q("eeschema")],
               ["generator_version", Q(GENERATOR_VERSION)],
               ["uuid", Q(sheet.uuid)],
               ["paper", Q(sheet.paper)],
               ["title_block",
                ["title", Q("Boompi Mainboard Rev A - " + sheet.title)],
                ["date", Q("2026-08-23")],
                ["rev", Q("A")],
                ["company", Q("Boompi")]],
               libs] + body + [["embedded_fonts", "no"]]
        path = os.path.join(self.d.outdir, sheet.filename)
        open(path, "w").write(dump(doc) + "\n")

    def _write_root(self):
        d = self.d
        doc = ["kicad_sch",
               ["version", SCH_VERSION],
               ["generator", Q("eeschema")],
               ["generator_version", Q(GENERATOR_VERSION)],
               ["uuid", Q(d.root_uuid)],
               ["paper", Q("A4")],
               ["title_block",
                ["title", Q("Boompi Mainboard Rev A - 00_TOP")],
                ["date", Q("2026-08-23")],
                ["rev", Q("A")],
                ["company", Q("Boompi")]],
               ["lib_symbols"]]
        x, y = 25.4, 30.48
        for i, sheet in enumerate(d.sheets):
            page = str(i + 2)
            doc.append(["sheet",
                        ["at", fmt(x), fmt(y)], ["size", "63.5", "17.78"],
                        ["exclude_from_sim", "no"], ["in_bom", "yes"],
                        ["on_board", "yes"], ["dnp", "no"],
                        ["fields_autoplaced", "yes"],
                        ["stroke", ["width", "0.1524"], ["type", "solid"]],
                        ["fill", ["color", "0", "0", "0", "0.0000"]],
                        ["uuid", Q(sheet.symuuid)],
                        ["property", Q("Sheetname"), Q(sheet.name),
                         ["at", fmt(x), fmt(y - 0.8), "0"],
                         ["effects", ["font", ["size", "1.27", "1.27"]],
                          ["justify", "left", "bottom"]]],
                        ["property", Q("Sheetfile"), Q(sheet.filename),
                         ["at", fmt(x), fmt(y + 18.6), "0"],
                         ["effects", ["font", ["size", "1.27", "1.27"]],
                          ["justify", "left", "top"], ["hide", "yes"]]],
                        ["instances",
                         ["project", Q(d.project),
                          ["path", Q("/" + d.root_uuid), ["page", Q(page)]]]]])
            y += 27.94
            if y > 180:
                y = 30.48
                x += 88.9
        doc.append(["text", Q("Boompi Mainboard Rev A\nGenerated per hardware/mainboard-rev-a/PLAN.md\nSee sheet notes for design intent; run scripts/gen_schematics.py to regenerate."),
                    ["exclude_from_sim", "yes"],
                    ["at", "200.66", "35.56", "0"],
                    ["effects", ["font", ["size", "2", "2"]], ["justify", "left", "top"]],
                    ["uuid", Q(d.uid("roottext"))]])
        doc.append(["sheet_instances", ["path", Q("/"), ["page", Q("1")]]])
        doc.append(["embedded_fonts", "no"])
        path = os.path.join(d.outdir, d.project + ".kicad_sch")
        open(path, "w").write(dump(doc) + "\n")

    def _report(self):
        for name, count in sorted(self.global_labels.items()):
            if count < 2:
                self.errors.append("global label '%s' used only once" % name)
        for (fn, name), count in sorted(self.local_labels.items()):
            if count < 2:
                self.errors.append("local label '%s' in %s used only once" % (name, fn))
        if self.errors:
            print("=== GENERATOR WARNINGS/ERRORS (%d) ===" % len(self.errors))
            for e in self.errors:
                print(" -", e)
        else:
            print("generator: no consistency issues")
