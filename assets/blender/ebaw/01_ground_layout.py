"""EBAW (Antwerp International Airport) - step 1: ground layout.

Builds collection 01_Ground_Layout: runway 11/29, taxiways, aprons,
shoulders, grass, and the ground markings, at 1:1 scale with procedural
PBR materials. Then it renders a check view.

    blender -b --factory-startup --python assets/blender/ebaw/01_ground_layout.py -- \
        [--render /tmp/ebaw_step1.png] [--shot overview|thr11|thr29|apron] [--samples 48] \
        [--res 1920x1080] [--save assets/blender/ebaw/ebaw.blend]

The layout (outlines, centrelines, stands, holding points) comes from
ebaw_layout.json, made by ebaw_osm.py from OpenStreetMap (c) OpenStreetMap
contributors, ODbL. The markings themselves follow ICAO Annex 14 vol. I
ch. 5 and are drawn here.

Frame (shared by every EBAW module)
-----------------------------------
* 1 Blender unit = 1 m. X = east, Y = north, Z = up. Z = 0 = paved surface.
* Origin (0, 0, 0) = runway 11/29 midpoint, 51.1899861 N 4.4630533 E,
  aerodrome elevation 12 m (39 ft) AMSL. Object EBAW_Origin carries it.
* Runway 11 true heading 109.76 deg (from OSM). "u" runs along it from the
  origin, "v" to the left of a runway 11 departure.
* Every ground object has an identity transform, so object coordinates are
  the frame's coordinates.
* All ground areas sit in one plane at Z = 0 without overlapping each other
  (the runway, taxiways, aprons, shoulders and grass split the aerodrome
  between them). Paint is a decal 1 cm above it (black backing 0.8 cm).

Key points, local metres (x, y):
    runway 11 end   (-710.6, +255.2)   runway 29 end   (+710.6, -255.2)
    threshold 11    (-575.0, +206.5)   displaced 144 m (LDA 11 = 1366 m)
    threshold 29    (+710.6, -255.2)
    Aprons 1 + 2 (terminal, business and Stampe hangars, stands 12-132)
                    centroid (-624, -209), 76 150 m2
    north apron (private)              centroid (-208, +419)
    helipads H2 (-230, -307), H3 (-85, +192)
    compass calibration pad ("Kalibreerschijf", ruins) (+619, -826)
"""

import json
import math
import sys
from pathlib import Path

import bpy
from mathutils import Vector
from mathutils.geometry import tessellate_polygon

sys.path.insert(0, str(Path(__file__).resolve().parent))
from ebaw_lib import Graph, args, camera, check_render, collection, mesh_object  # noqa: E402

HERE = Path(__file__).resolve().parent
LAYOUT = json.loads((HERE / "ebaw_layout.json").read_text())
COLL = "01_Ground_Layout"
PREVIEW = "99_Preview"

RWY = LAYOUT["runway"]
HDG = RWY["hdg_11"]
L, W = RWY["length"], RWY["width"]
U = Vector((math.sin(math.radians(HDG)), math.cos(math.radians(HDG)), 0.0))  # along runway 11
V = Vector((-U.y, U.x, 0.0))                                                  # left of runway 11

# Declared distances (Belgian AIP, EBAW AD 2.13): RWY 11 LDA 1366 m against a
# 1510 m TORA, so threshold 11 is taken as displaced by 144 m. That is an
# inference from the distances; change it here if the AIP chart says otherwise.
DISPLACED_11 = 144.0
DISPLACED_29 = 0.0

PAINT_Z = 0.010
BLACK_Z = 0.008


# ---------------------------------------------------------------- scene setup

class Builder:
    """Collects flat polygons (with holes) at a height and triangulates them."""

    def __init__(self, z):
        self.z, self.verts, self.faces = z, [], []

    def poly(self, rings):
        rings = [r for r in rings if len(r) >= 3]
        if not rings:
            return
        base = len(self.verts)
        flat = [p for r in rings for p in r]
        self.verts += [(x, y, self.z) for x, y in flat]
        loops = [[Vector((x, y, 0.0)) for x, y in r] for r in rings]
        for t in tessellate_polygon(loops):
            a, b, c = (flat[i] for i in t)
            # every face up (+Z)
            if (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) < 0:
                t = (t[0], t[2], t[1])
            self.faces.append(tuple(base + i for i in t))

    def quad(self, pts):
        self.poly([pts])

    def build(self, name, coll, mat):
        return mesh_object(name, coll, self.verts, self.faces, mat)


def rwy_pt(u, v):
    p = U * u + V * v
    return (p.x, p.y)


def rwy_rect(b, u0, u1, v0, v1):
    b.quad([rwy_pt(u0, v0), rwy_pt(u1, v0), rwy_pt(u1, v1), rwy_pt(u0, v1)])


def ribbon(b, pts, width, dash=None):
    """A painted line along a polyline, mitred at the bends; dash = (on, off) metres."""
    pts = [Vector((x, y, 0.0)) for x, y in pts]
    if len(pts) < 2:
        return
    if dash:
        # split into dashes along the length
        on, off = dash
        segs, s, d = [], 0.0, [pts[0]]
        period = on + off
        for a, c in zip(pts, pts[1:]):
            seg = (c - a).length
            t = 0.0
            while t < seg:
                phase = (s + t) % period
                step = min(seg - t, (on - phase) if phase < on else (period - phase))
                q = a + (c - a) * ((t + step) / seg)
                if phase < on:
                    d.append(q)
                    if abs((s + t + step) % period - on) < 1e-6 or (s + t + step) % period == 0:
                        segs.append(d)
                        d = [q]
                else:
                    d = [q]
                t += step
            s += seg
        if len(d) > 1:
            segs.append(d)
        for sgm in segs:
            ribbon(b, [(p.x, p.y) for p in sgm], width)
        return
    h = width / 2
    left = []
    for i, p in enumerate(pts):
        n = Vector((0.0, 0.0, 0.0))
        if i > 0:
            t = (p - pts[i - 1]).normalized()
            n += Vector((-t.y, t.x, 0.0))
        if i < len(pts) - 1:
            t = (pts[i + 1] - p).normalized()
            n += Vector((-t.y, t.x, 0.0))
        n.normalize()
        # mitre, kept sane on tight bends
        if 0 < i < len(pts) - 1:
            t = (pts[i + 1] - p).normalized()
            k = max(abs(n.dot(Vector((-t.y, t.x, 0.0)))), 0.35)
            n /= k
        left.append(n * h)
    for i in range(len(pts) - 1):
        a, c = pts[i], pts[i + 1]
        b.quad([(a - left[i]).xy, (c - left[i + 1]).xy, (c + left[i + 1]).xy, (a + left[i]).xy])


# ---------------------------------------------------------------- materials

def rubber_mask(g, u, v):
    """Tyre rubber in the touchdown zones of both thresholds, streaked along the runway."""
    t11 = -L / 2 + DISPLACED_11
    t29 = L / 2 - DISPLACED_29
    z11 = g.math("MULTIPLY", g.smooth(u, t11 + 150, t11 + 300), g.math("SUBTRACT", 1.0, g.smooth(u, t11 + 500, t11 + 750)))
    z29 = g.math("MULTIPLY", g.smooth(u, t29 - 300, t29 - 150, 1, 0), g.smooth(u, t29 - 750, t29 - 500))
    along = g.math("MAXIMUM", z11, z29)
    lat = g.math("SUBTRACT", 1.0, g.smooth(g.math("ABSOLUTE", v), 5.0, 12.0))
    streak = g.smooth(g.noise(g.vec(g.math("MULTIPLY", u, 0.015), g.math("MULTIPLY", v, 0.9)), 1.0, 6.0, 0.6), 0.38, 0.72)
    m = g.math("MULTIPLY", along, lat)
    return g.math("MULTIPLY", m, g.math("ADD", 0.35, g.math("MULTIPLY", streak, 0.65)))


def mat_runway():
    g = Graph("M_Asphalt_Runway")
    u, v = g.frame(U, V)
    fine = g.noise(g.pos, 9.0, 8.0, 0.65)
    broad = g.noise(g.pos, 0.012, 3.0)
    lane = g.white(g.vec(g.math("FLOOR", g.math("DIVIDE", g.math("ADD", v, 22.5), 5.0)), 0.0))  # 5 m paving lanes
    base = g.mix(g.smooth(fine, 0.3, 0.7), (0.042, 0.040, 0.038), (0.078, 0.074, 0.068))
    base = g.mix(g.math("MULTIPLY", g.smooth(broad, 0.35, 0.65), 0.5), base, (0.10, 0.098, 0.093))
    base = g.mix(g.math("MULTIPLY", lane, 0.18), base, (0.09, 0.09, 0.088))
    rub = rubber_mask(g, u, v)
    color = g.mix(g.math("MULTIPLY", rub, 0.9), base, (0.012, 0.012, 0.013))
    rough = g.lerp(rub, g.math("ADD", 0.82, g.math("MULTIPLY", fine, 0.14)), 0.66)
    g.bump(g.noise(g.pos, 22.0, 6.0, 0.7), 0.25, 0.004)
    return g.finish(color, rough, 0.4)


def mat_taxiway():
    g = Graph("M_Asphalt_Taxiway")
    fine = g.noise(g.pos, 8.0, 8.0, 0.65)
    patch = g.smooth(g.noise(g.pos, 0.035, 3.0), 0.58, 0.62)               # resurfaced patches
    warp = g.noise(g.pos, 0.4, 2.0, out="Color")
    pos2 = g.node("ShaderNodeVectorMath", operation="MULTIPLY_ADD")
    g.put(pos2.inputs[0], warp)
    pos2.inputs[1].default_value = (3.0, 3.0, 0.0)
    g.put(pos2.inputs[2], g.pos)
    crack = g.math("SUBTRACT", 1.0, g.smooth(g.voronoi_edge(pos2.outputs[0], 0.07), 0.004, 0.011))  # sealed cracks
    stain = g.smooth(g.noise(g.pos, 0.25, 4.0), 0.66, 0.78)
    base = g.mix(g.smooth(fine, 0.3, 0.7), (0.060, 0.060, 0.061), (0.105, 0.103, 0.099))
    base = g.mix(g.math("MULTIPLY", patch, 0.7), base, (0.045, 0.045, 0.047))
    base = g.mix(g.math("MULTIPLY", stain, 0.5), base, (0.03, 0.028, 0.026))
    color = g.mix(g.math("MULTIPLY", crack, 0.9), base, (0.015, 0.015, 0.016))
    rough = g.lerp(g.math("MAXIMUM", crack, g.math("MULTIPLY", stain, 0.6)), g.math("ADD", 0.84, g.math("MULTIPLY", fine, 0.12)), 0.55)
    g.bump(g.math("ADD", g.noise(g.pos, 20.0, 6.0, 0.7), g.math("MULTIPLY", crack, -0.6)), 0.25, 0.004)
    return g.finish(color, rough, 0.4)


def mat_apron(name="M_Concrete_Apron", slab=5.0):
    g = Graph(name)
    u, v = g.frame(U, V)
    # slab x slab concrete slabs, parallel to the runway
    su, sv = g.math("DIVIDE", u, slab), g.math("DIVIDE", v, slab)
    cell = g.white(g.vec(g.math("FLOOR", su), g.math("FLOOR", sv)))

    def joint_dist(s):  # metres to the nearest joint
        f = g.math("SUBTRACT", g.math("FRACT", g.math("ADD", s, 0.5)), 0.5)
        return g.math("MULTIPLY", g.math("ABSOLUTE", f), slab)
    jd = g.math("MINIMUM", joint_dist(su), joint_dist(sv))
    joint = g.math("SUBTRACT", 1.0, g.smooth(jd, 0.008, 0.022))
    fine = g.noise(g.pos, 14.0, 8.0, 0.6)
    stain = g.smooth(g.noise(g.pos, 0.18, 5.0), 0.6, 0.76)
    tyre = g.smooth(g.noise(g.vec(g.math("MULTIPLY", u, 0.2), g.math("MULTIPLY", v, 0.02)), 1.0, 4.0), 0.62, 0.75)
    base = g.mix(cell, (0.19, 0.185, 0.172), (0.235, 0.228, 0.212))
    base = g.mix(g.math("MULTIPLY", g.smooth(fine, 0.3, 0.7), 0.6), base, (0.31, 0.30, 0.285))
    base = g.mix(g.math("MULTIPLY", stain, 0.55), base, (0.12, 0.115, 0.10))
    base = g.mix(g.math("MULTIPLY", tyre, 0.25), base, (0.08, 0.08, 0.08))
    color = g.mix(joint, base, (0.03, 0.03, 0.03))
    rough = g.lerp(stain, g.math("ADD", 0.78, g.math("MULTIPLY", fine, 0.16)), 0.6)
    rough = g.lerp(joint, rough, 0.5)
    g.bump(g.math("ADD", g.math("MULTIPLY", g.noise(g.pos, 30.0, 6.0, 0.6), 0.4), g.math("MULTIPLY", joint, -1.0)), 0.3, 0.006)
    return g.finish(color, rough, 0.45)


def grass_color(g, v):
    big = g.noise(g.pos, 0.02, 4.0)
    mid = g.noise(g.pos, 0.25, 6.0, 0.6)
    fine = g.noise(g.pos, 6.0, 4.0)
    mow = g.math("MULTIPLY", g.smooth(g.math("SINE", g.math("MULTIPLY", v, math.pi / 7.5)), -0.15, 0.15), 0.35)  # 7.5 m mower passes
    c = g.mix(g.smooth(mid, 0.35, 0.65), (0.030, 0.055, 0.014), (0.062, 0.090, 0.024))
    c = g.mix(g.math("MULTIPLY", g.smooth(big, 0.5, 0.7), 0.6), c, (0.105, 0.098, 0.045))     # drier patches
    c = g.mix(mow, c, (0.085, 0.115, 0.035))
    c = g.mix(g.math("MULTIPLY", g.smooth(fine, 0.4, 0.8), 0.3), c, (0.02, 0.035, 0.01))
    return c, fine


def mat_grass():
    g = Graph("M_Grass")
    u, v = g.frame(U, V)
    c, fine = grass_color(g, v)
    g.bump(g.noise(g.pos, 40.0, 4.0), 0.4, 0.01)
    return g.finish(c, g.math("ADD", 0.86, g.math("MULTIPLY", fine, 0.1)), 0.3)


def mat_shoulder():
    g = Graph("M_Shoulder")
    u, v = g.frame(U, V)
    grass, fine = grass_color(g, v)
    gravel = g.mix(g.smooth(g.noise(g.pos, 18.0, 8.0, 0.7), 0.3, 0.7), (0.065, 0.058, 0.045), (0.12, 0.108, 0.085))
    tufts = g.smooth(g.noise(g.pos, 2.5, 6.0, 0.65), 0.36, 0.56)  # grass creeping over the stabilised soil
    color = g.mix(tufts, gravel, grass)
    g.bump(g.noise(g.pos, 35.0, 6.0, 0.7), 0.5, 0.01)
    return g.finish(color, g.lerp(tufts, 0.9, 0.88), 0.35)


def mat_paint(name, rgb, on_runway=False):
    g = Graph(name)
    wear = g.smooth(g.noise(g.pos, 1.6, 8.0, 0.7), 0.58, 0.75)
    flake = g.smooth(g.noise(g.pos, 25.0, 4.0), 0.6, 0.72)
    worn = tuple(c * 0.35 + 0.03 for c in rgb)
    color = g.mix(g.math("MAXIMUM", g.math("MULTIPLY", wear, 0.55), g.math("MULTIPLY", flake, 0.4)), rgb, worn)
    rough = g.lerp(wear, 0.5, 0.75)
    if on_runway:
        u, v = g.frame(U, V)
        rub = rubber_mask(g, u, v)
        color = g.mix(g.math("MULTIPLY", rub, 0.85), color, (0.015, 0.015, 0.016))
        rough = g.lerp(rub, rough, 0.68)
    g.bump(g.noise(g.pos, 30.0, 4.0), 0.15, 0.002)
    return g.finish(color, rough, 0.5)


def mat_preview():
    g = Graph("M_Preview_Farmland")
    cell = g.node("ShaderNodeTexVoronoi", feature="F1")
    g.put(cell.inputs["Vector"], g.pos)
    cell.inputs["Scale"].default_value = 0.004
    c = g.mix(cell.outputs["Distance"], (0.05, 0.075, 0.02), (0.10, 0.09, 0.045))
    c = g.mix(g.math("MULTIPLY", g.smooth(g.noise(g.pos, 0.05, 5.0), 0.4, 0.6), 0.5), c, (0.035, 0.05, 0.02))
    return g.finish(c, 0.9, 0.3)


# ---------------------------------------------------------------- markings

# Block numerals for runway designators, drawn in 4 m x 9 m cells (defined 3 wide), strokes 0.9 m wide,
# as rectangles (a0, a1, b0, b1): a across (left to right as read), b along
# (0 at the foot of the numeral, nearest the threshold).
S = 0.9
GLYPHS = {
    "1": [(1.05, 1.95, 0, 9), (0.3, 1.05, 7.6, 8.3)],
    # no two rectangles overlap: coplanar overlaps shade black in Cycles
    "2": [(0, 3, 9 - S, 9), (3 - S, 3, 4.5 + S / 2, 9 - S), (0, 3, 4.5 - S / 2, 4.5 + S / 2), (0, S, S, 4.5 - S / 2), (0, 3, 0, S)],
    "9": [(0, 3, 9 - S, 9), (0, S, 4.5 + S / 2, 9 - S), (3 - S, 3, S, 9 - S), (0, 3 - S, 4.5 - S / 2, 4.5 + S / 2), (0, 3, 0, S)],
}


def runway_markings(b):
    """ICAO markings on a 45 m precision-approach runway (Annex 14, 5.2)."""
    edge = 0.9
    # side stripes, the full length
    for s in (-1, 1):
        rwy_rect(b, -L / 2, L / 2, s * (W / 2 - edge), s * W / 2)
    ends = [(-L / 2 + DISPLACED_11, 1, "11", DISPLACED_11), (L / 2 - DISPLACED_29, -1, "29", DISPLACED_29)]
    starts = []
    for thr, s, num, disp in ends:
        def at(d0, d1, v0, v1):
            rwy_rect(b, thr + s * d0, thr + s * d1, v0, v1)

        # threshold: 12 stripes 1.8 x 30 m, 3 m in from the edges, double gap at the centre
        n, sw = 12, 1.8
        gap = (W - 6.0 - n * sw) / (n + 0.0)
        v = -W / 2 + 3.0
        for i in range(n):
            at(6.0, 36.0, v, v + sw)
            v += sw + gap * (2 if i == n // 2 - 1 else 1)
        # designator, 12 m beyond the stripes, read from the approach
        foot = 36.0 + 12.0
        left = s  # +v is the left of a runway 11 landing, -v of a 29 landing
        for k, ch in enumerate(num):
            vc = left * (3.5 if k == 0 else -3.5)
            for a0, a1, b0, b1 in GLYPHS[ch]:
                # a runs to the pilot's right, which is -left*v; cells drawn 4 m wide
                a0, a1 = a0 * 4.0 / 3.0, a1 * 4.0 / 3.0
                va, vb = vc - left * (a0 - 2.0), vc - left * (a1 - 2.0)
                at(foot + b0, foot + b1, min(va, vb), max(va, vb))
        starts.append(thr + s * (foot + 9.0 + 12.0))
        # aiming point (LDA 1200-2400 m): 300 m, 45 x 6 m, 18 m apart
        for sv in (-1, 1):
            at(300.0, 345.0, sv * 9.0, sv * 15.0)
        # touchdown zone pairs, 22.5 x 1.8 m stripes 1.5 m apart, every 150 m
        for d, k in ((150.0, 3), (450.0, 2), (600.0, 1)):
            for sv in (-1, 1):
                for i in range(k):
                    v0 = 9.0 + i * (1.8 + 1.5)
                    at(d, d + 22.5, sv * v0, sv * (v0 + 1.8))
        if disp > 0:
            # displaced threshold: a white transverse bar and arrows in the displaced length
            at(-1.8, 0.0, -W / 2 + edge, W / 2 - edge)
            for d in (-30.0, -75.0, -120.0):
                at(d - 15.0, d, -0.45, 0.45)                                  # shaft
                tip, base, half = thr + s * (d + 6.0), thr + s * d, 2.25        # head
                b.poly([[rwy_pt(base, -half), rwy_pt(tip, 0.0), rwy_pt(base, half)]])
    # centreline: 30 m dashes, 20 m gaps, 0.9 m wide, between the designators
    u0, u1 = starts[0], starts[1]
    n = int((u1 - u0 + 20.0) // 50.0)
    pad = ((u1 - u0) - (n * 50.0 - 20.0)) / 2
    for i in range(n):
        a = u0 + pad + i * 50.0
        rwy_rect(b, a, a + 30.0, -0.45, 0.45)
    return starts


def holding_point(b, x, y, hdg, width=16.0):
    """Pattern A: two solid lines (holding side) then two dashed (runway side), 0.15 m."""
    f = Vector((math.sin(math.radians(hdg)), math.cos(math.radians(hdg)), 0.0))
    r = Vector((f.y, -f.x, 0.0))
    c = Vector((x, y, 0.0))
    for k, dashed in ((-0.75, False), (-0.45, False), (-0.15, True), (0.15, True)):
        p = c + f * (k + 0.075)
        line = [(p - r * width / 2).xy, (p + r * width / 2).xy]
        ribbon(b, line, 0.15, dash=(1.0, 1.0) if dashed else None)


def text_mesh(text, size):
    """A flat mesh of the text, centred, in metres, read along +Y."""
    cu = bpy.data.curves.new("tmp_text", "FONT")
    cu.body = text
    cu.size = size
    cu.align_x = "CENTER"
    cu.align_y = "CENTER"
    cu.fill_mode = "FRONT"
    ob = bpy.data.objects.new("tmp_text", cu)
    bpy.context.scene.collection.objects.link(ob)
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(ob.evaluated_get(dg))
    verts = [v.co.copy() for v in me.vertices]
    faces = [tuple(p.vertices) for p in me.polygons]
    bpy.data.objects.remove(ob)
    bpy.data.curves.remove(cu)
    bpy.data.meshes.remove(me)
    return verts, faces


def place(b, verts, faces, x, y, hdg):
    """Add a mesh read along +Y so it reads along bearing hdg, at (x, y)."""
    a = math.radians(hdg)
    ca, sa = math.cos(a), math.sin(a)
    base = len(b.verts)
    for v in verts:
        # +Y -> bearing direction, +X -> to its right
        b.verts.append((x + v.x * ca + v.y * math.sin(a), y - v.x * sa + v.y * ca, b.z))
    b.faces += [tuple(base + i for i in f) for f in faces]


# ---------------------------------------------------------------- build

def build():
    root = bpy.context.scene.collection
    coll = collection(COLL, root)
    areas = LAYOUT["areas"]

    def area(name, key, mat):
        bd = Builder(0.0)
        for p in areas[key]:
            bd.poly(p)
        return bd.build(name, coll, mat)

    area("EBAW_Grass", "grass", mat_grass())
    area("EBAW_Shoulders", "shoulder", mat_shoulder())
    area("EBAW_Taxiways", "taxiway", mat_taxiway())
    area("EBAW_Aprons", "apron", mat_apron())
    area("EBAW_Terminal_Forecourt", "plaza", mat_apron("M_Concrete_Pavers", 1.2))
    rb = Builder(0.0)
    rb.poly(RWY["outline"])
    rb.build("EBAW_Runway_11_29", coll, mat_runway())

    # paint, one mesh per colour (one draw call each)
    white = Builder(PAINT_Z)
    starts = runway_markings(white)
    yellow = Builder(PAINT_Z)
    for ln in LAYOUT["paint"]["taxi_centre"]:
        ribbon(yellow, ln, 0.15)
    for h in LAYOUT["holds"]:
        holding_point(yellow, h["x"], h["y"], h["hdg"])
    black = Builder(BLACK_Z)
    tv, tf = {}, {}
    for st in LAYOUT["stands"]:
        f = Vector((math.sin(math.radians(st["hdg"])), math.cos(math.radians(st["hdg"])), 0.0))
        r = Vector((f.y, -f.x, 0.0))
        p = Vector((st["x"], st["y"], 0.0))
        # stop bar across the lead-in, at the nose-wheel stop point
        ribbon(yellow, [(p - r * 2.0).xy, (p + r * 2.0).xy], 0.3)
        # stand number on a black panel, 9 m before the stop, read when taxiing in
        q = p - f * 9.0
        if st["ref"] not in tv:
            tv[st["ref"]], tf[st["ref"]] = text_mesh(st["ref"], 1.6)
        black.quad([(q - r * 1.9 - f * 1.1).xy, (q + r * 1.9 - f * 1.1).xy, (q + r * 1.9 + f * 1.1).xy, (q - r * 1.9 + f * 1.1).xy])
        place(yellow, tv[st["ref"]], tf[st["ref"]], q.x, q.y, st["hdg"])
    red = Builder(PAINT_Z)
    for ln in LAYOUT["paint"]["red_limit"]:
        ribbon(red, ln, 0.15)
    # helipads: white TLOF edge and H
    for hp in LAYOUT["helipads"]:
        cx, cy, rr = hp["x"], hp["y"], hp["r"]
        ring = [(cx + rr * math.cos(t * math.pi / 24), cy + rr * math.sin(t * math.pi / 24)) for t in range(49)]
        ribbon(white, ring, 0.3)
        for a0, a1, b0, b1 in ((-1.5, -0.9, -2, 2), (0.9, 1.5, -2, 2), (-0.9, 0.9, -0.3, 0.3)):
            white.quad([(cx + a0, cy + b0), (cx + a1, cy + b0), (cx + a1, cy + b1), (cx + a0, cy + b1)])

    white.build("Paint_White", coll, mat_paint("M_Paint_White", (0.70, 0.70, 0.68), on_runway=True))
    yellow.build("Paint_Yellow", coll, mat_paint("M_Paint_Yellow", (0.80, 0.52, 0.02)))
    red.build("Paint_Red", coll, mat_paint("M_Paint_Red", (0.55, 0.03, 0.02)))
    black.build("Paint_Black", coll, mat_paint("M_Paint_Black", (0.012, 0.012, 0.012)))

    # reference empties for the next modules (lighting, signs)
    def empty(name, x, y, hdg=None, **props):
        e = bpy.data.objects.new(name, None)
        e.empty_display_type = "ARROWS" if hdg is not None else "PLAIN_AXES"
        e.empty_display_size = 20.0
        e.location = (x, y, 0.0)
        if hdg is not None:
            e.rotation_euler = (0.0, 0.0, -math.radians(hdg))  # local +Y along the bearing
        for k, v in props.items():
            e[k] = v
        coll.objects.link(e)
    fr = LAYOUT["frame"]
    empty("EBAW_Origin", 0.0, 0.0, None, lat=fr["origin_lat"], lon=fr["origin_lon"], elevation_m=fr["elevation_m"])
    t11 = U * (-L / 2 + DISPLACED_11)
    t29 = U * (L / 2 - DISPLACED_29)
    empty("RWY_THR_11", t11.x, t11.y, HDG, displaced_m=DISPLACED_11)
    empty("RWY_THR_29", t29.x, t29.y, (HDG + 180) % 360, displaced_m=DISPLACED_29)
    for i, w in enumerate(LAYOUT["windsocks"]):
        empty(f"WINDSOCK_{i + 1}", w[0], w[1])
    for i, h in enumerate(LAYOUT["holds"]):
        empty(f"HOLD_{i + 1}", h["x"], h["y"], h["hdg"])
    return coll, t11, t29


def preview_and_render(opt, t11, t29):
    scene = bpy.context.scene
    prev = collection(PREVIEW)
    # farmland round the aerodrome, just below it, only for the check renders
    s = 15000.0
    mesh_object("Preview_Ground", prev, [(-s, -s, -0.05), (s, -s, -0.05), (s, s, -0.05), (-s, s, -0.05)], [(0, 1, 2, 3)], mat_preview())

    shots = {
        # (camera location, target, lens mm)
        "overview": ((-620.0, -1450.0, 1050.0), (40.0, -40.0, 0.0), 34.0),
        "thr11": ((t11 - U * 260 + Vector((0, 0, 55))).to_tuple(), (t11 + U * 120).to_tuple(), 35.0),
        "thr29": ((t29 + U * 260 + Vector((0, 0, 55))).to_tuple(), (t29 - U * 120).to_tuple(), 35.0),
        "apron": ((-420.0, -520.0, 160.0), (-600.0, -170.0, 0.0), 30.0),
    }
    loc, tgt, lens = shots.get(opt["shot"], shots["overview"])
    cam = camera("CheckCam", prev, loc, tgt, lens)
    check_render(opt, cam)


def main():
    opt = args()
    # drop the factory startup's cube, light and camera; the other modules' collections stay
    factory = bpy.data.collections.get("Collection")
    if factory is not None:
        for ob in list(factory.objects):
            bpy.data.objects.remove(ob, do_unlink=True)
        bpy.data.collections.remove(factory)
    coll, t11, t29 = build()
    tris = sum(len(o.data.polygons) for o in coll.objects if o.type == "MESH")
    print(f"01_Ground_Layout: {len(coll.objects)} objects, {tris} triangles; "
          f"THR 11 at ({t11.x:.1f}, {t11.y:.1f}), THR 29 at ({t29.x:.1f}, {t29.y:.1f})")
    preview_and_render(opt, t11, t29)


main()
