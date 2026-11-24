# The Gulfstream G650 (GVI, the 2012 baseline, not the G650ER) for FableFlight, built
# procedurally in Blender.
#
#   blender -b --factory-startup --python assets/blender/g650.py -- <repo>/assets [--render <dir>] [--guides <dir>]
#
# Built the way Concorde is (assets/blender/concorde.py, whose mesh toolkit this
# file carries a copy of): the airframe from lofted sections, one object per
# piece that moves or breaks off on its own, saved as assets/blender/g650.blend
# and exported to assets/g650.bin.gz, which src/aircraft/g650 loads. That
# rebuild starts from scratch, so it throws away anything edited by hand in the
# .blend. To keep hand edits, export the .blend as it stands instead (save it first):
#
#   blender -b assets/blender/g650.blend --python assets/blender/g650.py -- <repo>/assets --export-only
#
# Edit freely: reshape meshes, add modifiers (they are applied on export), move
# or turn pieces (each carries its hinge along), assign the g650_ materials.
# Keep the piece names and the "G650" collection: the game finds pieces by name
# (see EXPECTED below); objects with other names are ignored. How each piece
# moves is set in Rust (src/aircraft/g650/mod.rs) by its name; each object
# records where its hinge is in its own frame (custom properties "pivot",
# "axis", "angle": the angle a hinge turns through, or how far, in metres, a
# slide moves).
#
# Openings in the skin (the flight deck's windshield and side windows, the main
# door) are cut with boolean modifiers whose cutters' faces are then deleted, so
# the skin is left with a clean hole; the glass and the door are the skin
# inside the same cutters. The gear wells are cut the same way but keep the
# cutters' walls (finish "well"). Every export ends with a gear check, as
# Concorde's: a sweep of the nose leg from up to down that names any part
# passing through the skin outside its well, and any part of the stowed gear
# left outside the skin.
#
# --render writes preview pictures (with the gear down as the game has it, and
# the gear part way up), --still <png> an unpainted clay still. The start
# screen's painted still (assets/aircraft/g650.png) is made from the saved
# .blend afterwards with assets/blender/g650_still.py. --guides writes the four orthographic views
# (top, bottom, left side, right side) the paint (g650_paint.py) is projected
# from, each with its silhouette mask and a geometry pass (<view>_geo.npz).
#
# Dimensions and where they come from: see src/aircraft/g650/mod.rs. In short:
# the EASA type certificate data sheet EASA.IM.A.169 (span 30.36 m, overall
# length 30.41 m, fuselage 2.74 m wide, MAC 4.756 m, tyres 21x7.25-10 and
# H37.5x12.0R19); 7.82 m tall (Gulfstream); the 16 windows 28 in wide; the
# rest measured off photographs and a published three-view (estimates).
#
# Two frames are used here: drawing positions `d` (m aft of the radome's tip)
# and `h` (m above the ground with the aircraft at rest), and Blender's: +X
# right, +Y forward (nose), +Z up, origin at the reference centre of gravity
# (d = CG_D, h = CG_H), metres. The game's body frame is +X right, +Y up, -Z
# forward, so a point (x, y, z) here is (x, z, -y) there.
import bpy, bmesh, math, struct, gzip, sys, os
from mathutils import Vector as V, Matrix

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
ASSETS = os.path.abspath(argv[0]) if argv else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RENDER = argv[argv.index("--render") + 1] if "--render" in argv else None
GUIDES = argv[argv.index("--guides") + 1] if "--guides" in argv else None
STILL = argv[argv.index("--still") + 1] if "--still" in argv else None
EXPORT_ONLY = "--export-only" in argv

def lerp(a, b, t):
    return a + (b - a) * t


def smoothstep(a, b, x):
    t = max(0.0, min(1.0, (x - a) / (b - a)))
    return t * t * (3.0 - 2.0 * t)


def M(v, s):
    """Mirror to side s (+1 right, -1 left)."""
    return V((v.x * s, v.y, v.z))


def game(v):
    return (v.x, v.z, -v.y)



# ---------------------------------------------------------------- materials
# name: (game material id, surface pattern, linear colour, extra); ids as airframe::mat.
# Pattern 17 is the painted skin (plane.wgsl): the paint projected from the
# aircraft's paint pictures (g650_paint.py); the vertex colour shows only where
# no picture faces the surface. Material 11 (exhaust) extra: 0 outside a
# nozzle, 1 inside it.
MATS = {
    "skin": (15, 17, (0.80, 0.80, 0.78), 0),       # painted (white)
    "lip": (2, 0, (0.72, 0.73, 0.75), 0),          # polished leading edges and intake lips (bare metal)
    "inlet": (10, 0, (0.05, 0.05, 0.055), 0),      # inside the intakes
    "fan": (3, 0, (0.30, 0.31, 0.33), 0),          # the fan's blades, titanium
    "spinner": (10, 0, (0.03, 0.03, 0.032), 0),
    "cascade": (10, 0, (0.02, 0.02, 0.022), 0),    # the reversers' cascades
    "exhaust": (11, 0, (0.34, 0.32, 0.30), 0),
    "nozzle_in": (11, 0, (0.07, 0.065, 0.06), 1),
    "glass": (7, 0, (0.10, 0.10, 0.10), 0),
    "frame": (10, 0, (0.03, 0.03, 0.032), 0),      # the windows' rubber seals
    "cabin": (15, 0, (0.55, 0.53, 0.50), 0),       # the entry's lining, seen through the open door
    "carpet": (6, 0, (0.10, 0.09, 0.085), 0),
    "strut": (0, 0, (0.74, 0.74, 0.72), 0),        # the gear's paint
    "door_in": (15, 0, (0.70, 0.70, 0.68), 0),
    "tread": (15, 0, (0.20, 0.20, 0.21), 0),       # the airstair's steps
    "well": (15, 0, (0.55, 0.57, 0.55), 0),        # inside the gear wells
    "chrome": (3, 0, (0.85, 0.86, 0.88), 1),
    "steel": (3, 0, (0.32, 0.32, 0.33), 0),
    "rubber": (4, 0, (0.018, 0.018, 0.02), 0),
    "hub": (2, 0, (0.66, 0.67, 0.68), 0),
    "nav_red": (8, 0, (1.0, 0.06, 0.05), 0),
    "nav_green": (8, 0, (0.05, 1.0, 0.25), 1),
    "tail_white": (8, 0, (1.0, 1.0, 1.0), 2),
    "strobe": (8, 0, (1.0, 1.0, 1.0), 3),
    "beacon": (8, 0, (1.0, 0.10, 0.06), 4),
    "landing": (8, 0, (1.0, 1.0, 0.95), 5),
    "cut": (10, 0, (1.0, 0.0, 1.0), 0),            # cutters' faces: deleted after a cut
}

MAT = {}


def make_materials():
    for name, (gid, _pat, col, _x) in MATS.items():
        m = bpy.data.materials.get("g650_" + name) or bpy.data.materials.new("g650_" + name)
        srgb = tuple(max(c, 0.0) ** (1 / 2.2) for c in col)
        m.diffuse_color = (*srgb, 0.3 if gid in (7, 17) else 1.0)
        m["game"] = name
        MAT[name] = m


# ---------------------------------------------------------------- mesh building
class Geo:
    def __init__(self):
        self.bm = bmesh.new()
        self.mats = []
        self.facing = []  # (face, point it must face): open sheets, whose outside cannot be worked out

    def mi(self, m):
        if m not in self.mats:
            self.mats.append(m)
        return self.mats.index(m)

    def face(self, vs, m, toward=None):
        try:
            f = self.bm.faces.new(vs)
        except ValueError:
            return None
        f.material_index = self.mi(m)
        if toward is not None:
            self.facing.append((f, toward))
        return f

    def poly(self, pts, m, toward=None):
        """A flat face through these points."""
        return self.face([self.bm.verts.new(p) for p in pts], m, toward)

    def loft(self, rings, m, cap0=True, cap1=True, cap_mat=None, skip=None):
        """Join closed rings of equal length; a ring that is a single point closes to it.
        `skip(row, i)`: leave out the face between ring `row` and the next at segment i."""
        vr = []
        for r in rings:
            if max((p - r[0]).length for p in r) < 1e-5:
                vr.append([self.bm.verts.new(r[0])])
            else:
                vr.append([self.bm.verts.new(p) for p in r])
        n = len(rings[0])
        for row, (a, b) in enumerate(zip(vr, vr[1:])):
            for i in range(n):
                j = (i + 1) % n
                if skip and skip(row, i):
                    continue
                if len(a) == 1 and len(b) == 1:
                    continue
                if len(a) == 1:
                    self.face([a[0], b[i], b[j]], m)
                elif len(b) == 1:
                    self.face([a[i], a[j], b[0]], m)
                else:
                    self.face([a[i], a[j], b[j], b[i]], m)
        if cap0 and len(vr[0]) > 2:
            self.face(vr[0], cap_mat or m)
        if cap1 and len(vr[-1]) > 2:
            self.face(vr[-1], cap_mat or m)

    def strip(self, rows, m, toward=None):
        """Join open rows of points of equal length (a sheet, not a tube), facing `toward`."""
        vr = [[self.bm.verts.new(p) for p in r] for r in rows]
        for a, b in zip(vr, vr[1:]):
            for i in range(len(a) - 1):
                self.face([a[i], a[i + 1], b[i + 1], b[i]], m, toward)

    def box(self, c, h, m, basis=None):
        ax, ay, az = basis or (V((1, 0, 0)), V((0, 1, 0)), V((0, 0, 1)))
        vs = [self.bm.verts.new(c + ax * (h[0] * sx) + ay * (h[1] * sy) + az * (h[2] * sz)) for sx in (-1, 1) for sy in (-1, 1) for sz in (-1, 1)]
        for q in ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)):
            self.face([vs[k] for k in q], m)

    def plate(self, outline, normal, t, m, m_back=None):
        """A flat plate: this outline (3D points in one plane) made 2t thick along
        `normal`; `m_back` is the finish of the face away from the normal."""
        normal = normal.normalized()
        a = [self.bm.verts.new(p + normal * t) for p in outline]
        b = [self.bm.verts.new(p - normal * t) for p in outline]
        self.face(a, m)
        self.face(b[::-1], m_back or m)
        n = len(outline)
        for i in range(n):
            j = (i + 1) % n
            self.face([a[i], b[i], b[j], a[j]], m)

    def finish(self, name, smooth=38.0, pivot=None, axis=None, angle=0.0, recalc=True, parent=None, coll=None):
        bm = self.bm
        if recalc:
            bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        for f, toward in self.facing:
            f.normal_update()
            if (toward - f.calc_center_median()).dot(f.normal) < 0:
                f.normal_flip()
        me = bpy.data.meshes.new(name)
        bm.to_mesh(me)
        bm.free()
        for m in self.mats:
            me.materials.append(MAT[m])
        for p in me.polygons:
            p.use_smooth = True
        me.set_sharp_from_angle(angle=math.radians(smooth))
        ob = bpy.data.objects.new(name, me)
        (coll or COLL).objects.link(ob)
        # The hinge, in this object's own (local) frame: moving or turning the
        # object in Blender carries its hinge along.
        ob["pivot"] = list(pivot or V((0, 0, 0)))
        ob["axis"] = list((axis or V((1, 0, 0))).normalized())
        ob["angle"] = float(angle)
        if parent:
            ob["parent"] = parent  # what it hangs from (as PIECES in Rust), for previews
        return ob


def cutter(name, rings):
    """A solid lofted through closed rings that a well is cut with: kept in its
    own collection, out of the export. What it cuts takes its finish (well)."""
    g = Geo()
    g.loft(rings, "well")
    ob = g.finish(name, smooth=30, coll=CUTS)
    ob.hide_render = True
    ob.display_type = "WIRE"
    return ob


def carve(ob, cutters, smooth=38.0):
    """Cut these solids out of a piece's skin, leaving their walls inside it (the wells)."""
    for c in cutters:
        md = ob.modifiers.new("well", "BOOLEAN")
        md.operation, md.solver, md.object = "DIFFERENCE", "EXACT", c
        md.material_mode = "TRANSFER"
        md.use_hole_tolerant = True
    deps = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(ob.evaluated_get(deps))
    ob.modifiers.clear()
    old = ob.data
    ob.data = me
    bpy.data.meshes.remove(old)
    for p in me.polygons:
        p.use_smooth = True
    me.set_sharp_from_angle(angle=math.radians(smooth))


def join(ob, other, smooth=38.0):
    """Take another object's mesh into ob's, and remove it."""
    names = [m.name for m in ob.data.materials]
    for m in other.data.materials:
        if m.name not in names:
            ob.data.materials.append(m)
            names.append(m.name)
    remap = [names.index(m.name) for m in other.data.materials]
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    n0 = len(bm.faces)
    bm.from_mesh(other.data)
    bm.faces.ensure_lookup_table()
    for f in bm.faces[n0:]:
        f.material_index = remap[f.material_index]
    bm.to_mesh(ob.data)
    bm.free()
    bpy.data.objects.remove(other, do_unlink=True)
    for p in ob.data.polygons:
        p.use_smooth = True
    ob.data.set_sharp_from_angle(angle=math.radians(smooth))


def frame_at(t, ref=None):
    t = t.normalized()
    if ref is None:
        ref = V((0, 0, 1)) if abs(t.z) < 0.9 else V((1, 0, 0))
    u = t.cross(ref).normalized()
    return u, t.cross(u)


def tube(g, pts, r, m, n=10, caps=True, ref=None):
    rings = []
    for i, p in enumerate(pts):
        t = pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]
        u, w = frame_at(t, ref)
        rr = r(i / (len(pts) - 1)) if callable(r) else r
        rings.append([p + (u * math.cos(a) + w * math.sin(a)) * rr for a in (2 * math.pi * k / n for k in range(n))])
    g.loft(rings, m, caps, caps)


def lathe(g, c, axis, prof, m, n=24, cap_mat=None, caps=(True, True), zig=None):
    """Revolve (radius, distance along axis) about the axis through c. `zig`, one
    value for each point of the profile: every other point round that ring sits
    this fraction further in (overlapping petals)."""
    axis = axis.normalized()
    u, w = frame_at(axis)
    zig = zig or [0.0] * len(prof)
    rings = [[c + axis * a + (u * math.cos(t) + w * math.sin(t)) * (r * (1.0 - z * (k % 2))) for k, t in ((k, 2 * math.pi * k / n) for k in range(n))] for (r, a), z in zip(prof, zig)]
    g.loft(rings, m, caps[0], caps[1], cap_mat)


def ball(g, c, r, m, n=16, squash=(1.0, 1.0, 1.0)):
    rings = []
    for j in range(n // 2 + 1):
        t = math.pi * j / (n // 2)
        rings.append([c + V((r * math.sin(t) * math.cos(a) * squash[0], r * math.sin(t) * math.sin(a) * squash[1], -r * math.cos(t) * squash[2])) for a in (2 * math.pi * k / n for k in range(n))])
    g.loft(rings, m, False, False)


def catmull(p0, p1, p2, p3, t):
    t2, t3 = t * t, t * t * t
    return 0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 + (-p0 + 3 * p1 - 3 * p2 + p3) * t3)


def spline_rows(table, i0, i1, sub):
    """Rows of a station table from station i0 to i1, Catmull-Rom between them."""
    out = []
    n = len(table)
    for i in range(i0, i1):
        a, b, c, d = table[max(i - 1, 0)], table[i], table[i + 1], table[min(i + 2, n - 1)]
        for k in range(sub):
            t = k / sub
            out.append(tuple(catmull(a[j], b[j], c[j], d[j], t) for j in range(len(b))))
    out.append(table[i1])
    return out


def table_at(table, y):
    """Linear interpolation of a station table (stations by descending y)."""
    if y >= table[0][0]:
        return table[0]
    for a, b in zip(table, table[1:]):
        if a[0] >= y >= b[0]:
            t = (a[0] - y) / (a[0] - b[0])
            return tuple(lerp(a[k], b[k], t) for k in range(len(a)))
    return table[-1]

def blade(g, base, chord, height, t, down=False, m="skin"):
    """A swept blade antenna standing on `base`."""
    s = -1.0 if down else 1.0
    out = [base + V((0, chord / 2, 0)), base + V((0, -chord / 2, 0)), base + V((0, -chord / 2 - 0.02, s * height)), base + V((0, chord * 0.05, s * height))]
    g.plate(out, V((1, 0, 0)), t, m)



def naca_t(x, t):
    x = max(0.0, min(1.0, x))
    return 5 * t * (0.2969 * math.sqrt(x) - 0.1260 * x - 0.3516 * x * x + 0.2843 * x ** 3 - 0.1036 * x ** 4)


def foil_ring(sect, t, a, b, m=14):
    """A closed section from chord fraction a to b; t the thickness ratio, or a
    function of the chord fraction (a root fillet)."""
    le, c, cdir, up = sect
    tt = t if callable(t) else (lambda x: t)
    xs = [a + (b - a) * (1 - math.cos(math.pi * k / m)) / 2 for k in range(m + 1)]
    upper = [le + cdir * (c * x) + up * max(c * naca_t(x, tt(x)), 0.004) for x in xs]
    lower = [le + cdir * (c * x) - up * max(c * naca_t(x, tt(x)), 0.004) for x in reversed(xs)]
    return upper + lower


def surface(g, sect, u0, u1, a, b, t, m="skin", nu=6, tk=None):
    """`tk(u, x)`: how much thicker than t the section at u is at chord fraction x."""
    rings = []
    for k in range(nu):
        u = lerp(u0, u1, k / (nu - 1))
        rings.append(foil_ring(sect(u), (lambda x, u=u: t * tk(u, x)) if tk else t, a, b))
    g.loft(rings, m)


def hinge(sect, u, xc):
    le, c, cdir, _ = sect(u)
    return le + cdir * (c * xc)


def wick(g, p, aft=V((0, -1, 0)), up=V((0, 0, 1))):
    """A static discharger trailing from point p: a fine rod on a little base."""
    g.box(p + aft * 0.02, (0.006, 0.025, 0.004), "skin", (up.cross(aft).normalized(), aft, up))
    tube(g, [p + aft * 0.03, p + aft * 0.14], 0.0035, "rubber", n=5)


def pair_axis(right_axis, s):
    """The axis a mirrored piece turns about so equal angles move both sides alike (as Builder::pair)."""
    return right_axis if s > 0 else -M(right_axis, -1)


def wheel(g, c, r, hw):
    lathe(g, c, V((1, 0, 0)), [(0.0, -hw * 0.85), (r * 0.55, -hw * 0.85), (r * 0.55, -hw), (r - 0.04, -hw), (r, -hw * 0.6), (r, hw * 0.6), (r - 0.04, hw), (r * 0.55, hw), (r * 0.55, hw * 0.85), (0.0, hw * 0.85)], "rubber", n=28)
    lathe(g, c, V((1, 0, 0)), [(0.0, -hw * 0.9), (r * 0.56, -hw * 0.9), (r * 0.56, hw * 0.9), (0.0, hw * 0.9)], "hub", n=20)
    lathe(g, c, V((1, 0, 0)), [(0.0, -hw * 1.02), (r * 0.16, -hw * 1.02), (r * 0.20, -hw * 0.9), (r * 0.20, hw * 0.9), (r * 0.16, hw * 1.02), (0.0, hw * 1.02)], "steel", n=12)
    # Hub bolts round the flange.
    for k in range(8):
        a = 2 * math.pi * k / 8
        for side in (-1, 1):
            p = c + V((side * hw * 0.92, math.cos(a) * r * 0.36, math.sin(a) * r * 0.36))
            g.box(p, (0.008, 0.012, 0.012), "steel")


def stow(pivot, down, up):
    d0, d1 = down - pivot, up - pivot
    return d0.cross(d1).normalized(), d0.angle(d1)


def scissors(g, top, bottom, back, m="strut"):
    """Torque links: two arms from the strut's ends meeting behind it."""
    knee = (top + bottom) / 2 + back
    for a, b in ((top, knee), (knee, bottom)):
        tube(g, [a, b], 0.014, m, n=6)
    g.box(knee, (0.03, 0.02, 0.02), m)


def hermite(table, x, col):
    """Column `col` of a station table at x (its first column, rising or
    falling): a cubic through the stations with finite-difference slopes."""
    n = len(table)
    lo, hi = min(table[0][0], table[-1][0]), max(table[0][0], table[-1][0])
    x = max(lo, min(hi, x))
    for i in range(n - 1):
        x0, x1 = table[i][0], table[i + 1][0]
        if min(x0, x1) <= x <= max(x0, x1):
            v0, v1 = table[i][col], table[i + 1][col]
            a, d = table[max(i - 1, 0)], table[min(i + 2, n - 1)]
            m0 = (v1 - a[col]) / (x1 - a[0])
            m1 = (d[col] - v0) / (d[0] - x0)
            h = x1 - x0
            t = (x - x0) / h
            t2, t3 = t * t, t * t * t
            return (2 * t3 - 3 * t2 + 1) * v0 + (t3 - 2 * t2 + t) * h * m0 + (-2 * t3 + 3 * t2) * v1 + (t3 - t2) * h * m1
    return table[-1][col]


def aim(g, n0, toward):
    """Make the faces added since n0 look toward `toward(centre)`: for open
    sheets and funnels, which recalc_face_normals may turn inside out."""
    g.bm.faces.ensure_lookup_table()
    for f in g.bm.faces[n0:]:
        g.facing.append((f, toward(f.calc_center_median())))


def stations(x0, x1, step):
    n = max(1, round(abs(x1 - x0) / step))
    return [lerp(x0, x1, k / n) for k in range(n + 1)]



# ---------------------------------------------------------------- the two frames
# (kept in step with src/aircraft/g650/mod.rs: CG_D, CG_H)
CG_D = 14.35  # the reference centre of gravity, aft of the radome's tip: 30% of the 4.756 m MAC
CG_H = 2.55   # its height above the ground, at rest


def Y(d):
    return CG_D - d


def Z(h):
    return h - CG_H


def P(x, d, h):
    return V((x, Y(d), Z(h)))


# ---------------------------------------------------------------- fuselage
# (kept in step with FUSE in src/aircraft/g650/mod.rs)
# d, half width, top, height of the widest point, bottom (h), superellipse
# exponents of the upper and the lower half. Fitted to a left-side photograph
# (N47TR, Wagga Wagga) scaled by the 28 in windows and 21 in nose tyres and to
# the 2.74 m width of the TCDS; the belly's wing-to-body fairing (d 9..19)
# bulges below the round section; the tail cone ends at the APU's exhaust.
FUSE_DH = [
    (0.00, 0.000, 2.080, 2.080, 2.080, 2.0, 2.0),
    (0.06, 0.120, 2.170, 2.070, 1.980, 2.0, 2.0),
    (0.20, 0.240, 2.270, 2.060, 1.860, 2.0, 2.0),
    (0.40, 0.360, 2.390, 2.050, 1.750, 2.0, 2.0),
    (0.70, 0.500, 2.540, 2.060, 1.660, 2.0, 2.0),
    (1.00, 0.620, 2.680, 2.100, 1.570, 2.0, 2.0),
    (1.40, 0.770, 2.870, 2.170, 1.525, 2.0, 2.0),
    (1.85, 0.900, 3.080, 2.260, 1.505, 2.0, 2.0),
    (2.30, 1.020, 3.420, 2.360, 1.490, 2.0, 2.0),
    (2.80, 1.130, 3.700, 2.460, 1.460, 2.0, 2.0),
    (3.30, 1.220, 3.870, 2.550, 1.440, 2.0, 2.0),
    (3.80, 1.290, 3.990, 2.620, 1.420, 2.0, 2.0),
    (4.40, 1.340, 4.070, 2.680, 1.400, 2.0, 2.0),
    (5.20, 1.365, 4.120, 2.720, 1.385, 2.0, 2.0),
    (6.00, 1.370, 4.140, 2.750, 1.380, 2.0, 2.0),
    (8.40, 1.370, 4.140, 2.760, 1.380, 2.0, 2.0),
    (9.40, 1.370, 4.140, 2.760, 1.300, 2.0, 2.6),
    (10.4, 1.370, 4.140, 2.760, 1.200, 2.0, 3.2),
    (11.5, 1.370, 4.140, 2.760, 1.150, 2.0, 3.6),
    (16.4, 1.370, 4.140, 2.760, 1.150, 2.0, 3.6),
    (17.4, 1.370, 4.140, 2.770, 1.200, 2.0, 3.0),
    (18.4, 1.360, 4.140, 2.790, 1.330, 2.0, 2.4),
    (19.4, 1.335, 4.130, 2.830, 1.500, 2.0, 2.0),
    (20.4, 1.290, 4.110, 2.890, 1.660, 2.0, 2.0),
    (21.4, 1.230, 4.080, 2.970, 1.840, 2.0, 2.0),
    (22.4, 1.140, 4.040, 3.070, 2.050, 2.0, 2.0),
    (23.4, 1.020, 3.990, 3.170, 2.280, 2.0, 2.0),
    (24.4, 0.870, 3.930, 3.250, 2.540, 2.0, 2.0),
    (25.4, 0.680, 3.850, 3.320, 2.820, 2.0, 2.0),
    (26.3, 0.450, 3.750, 3.380, 3.080, 2.0, 2.0),
    (26.8, 0.260, 3.660, 3.420, 3.220, 2.0, 2.0),
    (27.0, 0.150, 3.600, 3.430, 3.280, 2.0, 2.0),
]
FUSE = [(Y(d), hw, Z(t), Z(m), Z(b), pu, pl) for d, hw, t, m, b, pu, pl in FUSE_DH]
RING_N = 96
D_RADOME = 0.85   # the radome's joint
D_DECK = 4.40     # the flight deck's bulkhead: the nose section ends here
D_CONE = 21.40    # where the tail cone joins the fuselage
D_TAIL = 27.0     # the tail cone's end (the APU's exhaust)


def fuse_row(y):
    return (y,) + tuple(hermite(FUSE, y, k) for k in range(1, 7))


def fuse_ring(y, hw, top, mid, bot, pu, pl, n=RING_N):
    hw = max(hw, 0.0)
    pts = []
    for k in range(n):
        a = 2 * math.pi * k / n
        c, s = math.cos(a), math.sin(a)
        p = pu if s >= 0 else pl
        f = abs(s) ** (2 / p)
        pts.append(V((hw * math.copysign(abs(c) ** (2 / p), c), y, mid + (top - mid) * f if s >= 0 else mid - (mid - bot) * f)))
    return pts


def ring_at(y, n=RING_N):
    return fuse_ring(*fuse_row(y), n=n)


def fuse_z(y, x, upper=True):
    """Height of the fuselage skin at station y, x out from the centreline."""
    _, hw, top, mid, bot, pu, pl = fuse_row(y)
    p = pu if upper else pl
    f = max(0.0, 1.0 - (min(abs(x), hw) / max(hw, 1e-6)) ** p) ** (1 / p)
    return mid + (top - mid) * f if upper else mid - (mid - bot) * f


def fuse_hw(y, z):
    """Half width of the fuselage at station y, height z."""
    _, hw, top, mid, bot, pu, pl = fuse_row(y)
    p = pu if z >= mid else pl
    f = min(1.0, abs(z - mid) / max((top - mid) if z >= mid else (mid - bot), 1e-6))
    return hw * max(0.0, 1.0 - f ** p) ** (1 / p)


def fuse_depth(p):
    """How far inside the fuselage's skin p is (negative: outside)."""
    if not FUSE[-1][0] < p.y < FUSE[0][0]:
        return -1.0
    return min(p.z - fuse_z(p.y, p.x, False), fuse_z(p.y, p.x, True) - p.z, fuse_hw(p.y, p.z) - abs(p.x))


def fuse_axis(y):
    return V((0, y, fuse_row(y)[3]))


def fuse_out(c):
    """A point well outside the fuselage's skin from c on it."""
    r = V((c.x, 0, c.z - fuse_row(c.y)[3]))
    return c + (r.normalized() if r.length > 1e-4 else V((0, 0, 1))) * 20.0


def to_skin(p, lift=0.0):
    """The point of the skin straight out from the fuselage's axis through p (lifted off it by `lift`)."""
    c = fuse_axis(p.y)
    r = V((p.x, 0, p.z - c.z))
    if r.length < 1e-6:
        r = V((0, 0, 1))
    r.normalize()
    lo, hi = 0.0, 3.0
    for _ in range(40):
        m = (lo + hi) / 2
        if fuse_depth(c + r * m) > 0:
            lo = m
        else:
            hi = m
    return c + r * (lo + lift)


def side_pt(d, h, s):
    y, z = Y(d), Z(h)
    return V((s * fuse_hw(y, z), y, z))


def top_pt(d, x):
    y = Y(d)
    return V((x, y, fuse_z(y, x, True)))


def outline(pts, step=0.03):
    """A closed outline through these points of the skin, every `step` metres, lying on it."""
    out = []
    for a, b in zip(pts, pts[1:] + pts[:1]):
        n = max(1, round((b - a).length / step))
        for k in range(n):
            out.append(to_skin(a.lerp(b, k / n)))
    return out


def window_cutter(name, pts, out=0.25, inn=0.10):
    """A prism through the skin along this outline (points on the skin), for cutting an opening."""
    rings = []
    for k in (out, -inn):
        rings.append([p + V((p.x, 0, p.z - fuse_axis(p.y).z)).normalized() * k for p in pts])
    g = Geo()
    g.loft(rings, "cut")
    ob = g.finish(name, smooth=10, coll=CUTS)
    ob.hide_render = True
    ob.display_type = "WIRE"
    return ob


def boolean(ob, cutters, op="DIFFERENCE", smooth=38.0):
    for c in cutters:
        md = ob.modifiers.new("cut", "BOOLEAN")
        md.operation, md.solver, md.object = op, "EXACT", c
        md.material_mode = "TRANSFER"
        md.use_hole_tolerant = True
    deps = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(ob.evaluated_get(deps))
    ob.modifiers.clear()
    old = ob.data
    ob.data = me
    bpy.data.meshes.remove(old)
    for p in me.polygons:
        p.use_smooth = True
    me.set_sharp_from_angle(angle=math.radians(smooth))


def drop_faces(ob, mat="cut"):
    """Delete the faces of this finish (what the cutters left behind)."""
    me = ob.data
    idx = [i for i, m in enumerate(me.materials) if m and m.name.startswith("g650_" + mat)]
    bm = bmesh.new()
    bm.from_mesh(me)
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if f.material_index in idx], context="FACES")
    bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
    bm.to_mesh(me)
    bm.free()


def set_mat(ob, mat):
    me = ob.data
    me.materials.clear()
    me.materials.append(MAT[mat])
    for p in me.polygons:
        p.material_index = 0


def copy_ob(ob, name):
    o = ob.copy()
    o.data = ob.data.copy()
    o.name = name
    for k in ("pivot", "axis", "angle", "parent"):
        if k in o:
            del o[k]
    COLL.objects.link(o)
    return o


def inset(ob, k):
    """Move every vertex k along its normal (k < 0: in)."""
    me = ob.data
    for v in me.vertices:
        v.co += v.normal * k


# ---------------------------------------------------------------- the flight deck's glazing and the main door
# (kept in step with WINDSHIELD and SIDE_WINDOW in src/aircraft/g650/mod.rs)
# In drawing terms (d, h), on the right side (the left is its mirror image):
# each windshield pane runs from the centre post (5 cm either side of the
# centreline) round to the A-post, its lower edge along h = WS_SILL; the side
# window behind it, its top edge at h = GLAZE_TOP, its aft edge leaning back.
WS_SILL = 3.08
GLAZE_TOP = 3.54
POST = 0.035
A_POST = ((2.52, 3.50), (2.62, WS_SILL))   # the A-post's top and foot (its forward edge)
WS_TOP_CENTRE = 2.60                         # where the windshield's top edge meets the centre post
WS_SILL_CENTRE = 1.88
SIDE_WINDOW = ((2.70, WS_SILL), (3.30, WS_SILL), (3.74, GLAZE_TOP), (2.59, GLAZE_TOP))


def windshield_pts(s):
    pts = [top_pt(WS_SILL_CENTRE, s * POST)]
    pts += [side_pt(lerp(WS_SILL_CENTRE + 0.06, A_POST[1][0], k / 6), WS_SILL, s) for k in range(7)]
    pts.append(side_pt(*A_POST[0], s))
    xa = abs(pts[-1].x)
    pts += [top_pt(lerp(A_POST[0][0], WS_TOP_CENTRE, k / 4), s * lerp(xa, POST, k / 4)) for k in range(1, 5)]
    return pts if s > 0 else pts[::-1]


def side_window_pts(s):
    pts = [side_pt(d, h, s) for d, h in SIDE_WINDOW]
    return pts if s < 0 else pts[::-1]


# The main door, on the left: an airstair hinged at its sill that swings out
# and down onto the ground, its steps on its inside. d from, to; h from, to.
DOOR = (4.55, 5.45, 1.70, 3.80)
DOOR_R = 0.13
DOOR_OPEN = math.radians(128.0)


def door_pts():
    d0, d1, h0, h1 = DOOR
    pts = []
    for (cd, ch, a0) in ((d0 + DOOR_R, h0 + DOOR_R, 180), (d1 - DOOR_R, h0 + DOOR_R, 270), (d1 - DOOR_R, h1 - DOOR_R, 0), (d0 + DOOR_R, h1 - DOOR_R, 90)):
        for k in range(5):
            a = math.radians(a0 + 90 * k / 4)
            pts.append(side_pt(cd + DOOR_R * math.cos(a), ch + DOOR_R * math.sin(a), -1))
    return pts


# The cabin windows: eight a side, ovals 0.71 m (28 in) by 0.51 m, their middles at these stations, at h WIN_H.
WIN_D = [6.65 + 1.375 * k for k in range(8)]
WIN_H = 3.04
WIN_SIZE = (0.71, 0.51)

# The gear wells (cut into the skin, their walls left as the well's finish).
NOSE_WELL = (0.95, 3.32, 0.29, 2.16)          # d from, to; half width; roof (h)
MAIN_BAY = (0.08, 1.22, 14.95, 16.02, 2.08)   # x from, to; d from, to; roof
LEG_SLOT = (1.21, 2.45, 14.85, 15.70)          # x from, to; d from, to (through the wing's lower skin; meeting the bay)


_CUTTERS = {}


def once(name, make):
    if name not in _CUTTERS:
        _CUTTERS[name] = make()
    return _CUTTERS[name]


def box_cutter(name, x0, x1, d0, d1, h0, h1, mat="well"):
    def make():
        g = Geo()
        g.loft([[P(x0, d, h0), P(x1, d, h0), P(x1, d, h1), P(x0, d, h1)] for d in (d0, d1)], mat)
        ob = g.finish(name, smooth=30, coll=CUTS)
        ob.hide_render = True
        ob.display_type = "WIRE"
        return ob
    return once(name, make)


def nose_well():
    d0, d1, hw, roof = NOSE_WELL
    return box_cutter("nose well", -hw, hw, d0, d1, -0.5, roof)


def main_bay(s):
    x0, x1, d0, d1, roof = MAIN_BAY
    a, b = sorted((x0 * s, x1 * s))
    return box_cutter(("right" if s > 0 else "left") + " main bay", a, b, d0, d1, -0.5, roof)


def leg_slot(s):
    x0, x1, d0, d1 = LEG_SLOT
    a, b = sorted((x0 * s, x1 * s))
    return box_cutter(("right" if s > 0 else "left") + " leg slot", a, b, d0, d1, -0.5, 1.96)


def fuselage():
    # The radome.
    g = Geo()
    ds = [0.0, 0.015, 0.04, 0.08, 0.14, 0.22, 0.32, 0.45, 0.60, 0.72, D_RADOME]
    g.loft([ring_at(Y(d)) for d in ds], "skin")
    g.finish("radome")

    # The nose section and the flight deck: a closed loft, then the windows
    # cut out of it and the nose well into it.
    ds = stations(D_RADOME, 1.6, 0.08) + stations(1.65, 4.0, 0.05)[1:] + [D_DECK]
    g = Geo()
    g.loft([ring_at(Y(d)) for d in ds], "skin")
    for s in (1, -1):
        # Pitot-static probes low on each side, an angle-of-attack vane and an ice detector ahead of the windshield.
        p = side_pt(1.30, 2.30, s)
        tube(g, [p - V((0.02 * s, 0, 0)), p + V((0.09 * s, 0.02, 0)), p + V((0.10 * s, 0.24, 0))], lambda t: 0.014 - 0.006 * t, "steel", n=8)
        p = side_pt(1.75, 2.55, s)
        g.plate([p, p + V((0.0, -0.10, 0.0)), p + V((0.07 * s, -0.12, 0.0)), p + V((0.07 * s, -0.05, 0.0))], V((0, 0, 1)), 0.004, "steel")
        p = side_pt(1.10, 2.62, s)
        tube(g, [p - V((0.01 * s, 0, 0)), p + V((0.07 * s, -0.02, 0))], 0.009, "steel", n=6)
    nose = g.finish("nose section")
    clean = copy_ob(nose, "glass source")
    boolean(nose, [nose_well()])
    cut = []
    for s, side in ((1, "right"), (-1, "left")):
        cut.append(window_cutter(side + " windshield cut", outline(windshield_pts(s))))
        cut.append(window_cutter(side + " side window cut", outline(side_window_pts(s))))
    boolean(nose, cut)
    drop_faces(nose)
    # The glass: the skin inside the same cutters, a few millimetres in.
    one = copy_ob(cut[0], "glazing cutter")
    for c in cut[1:]:
        tmp = copy_ob(c, "tmp")
        join(one, tmp)
    one.hide_render = True
    bpy.data.collections["G650"].objects.unlink(one)
    CUTS.objects.link(one)
    set_mat(clean, "glass")
    boolean(clean, [one], op="INTERSECT", smooth=60)
    drop_faces(clean)
    inset(clean, -0.006)
    clean.name = "windshield"
    clean.data.name = "windshield"
    clean["pivot"], clean["axis"], clean["angle"] = [0, 0, 0], [1, 0, 0], 0.0
    # Their seals and the posts' edges.
    g = Geo()
    for s in (1, -1):
        for pts in (outline(windshield_pts(s), 0.04), outline(side_window_pts(s), 0.04)):
            ring = [to_skin(p, -0.004) for p in pts]
            tube(g, ring + ring[:1], 0.010, "frame", n=6)
    g.finish("windshield frame", smooth=80)

    # The cabin: from the flight deck's bulkhead back to the tail cone, the
    # main door cut out of it on the left and the main gear's bays under it.
    ds = stations(D_DECK, 5.6, 0.1) + stations(6.0, 8.0, 1.0)[1:] + stations(8.6, 12.0, 0.4) + stations(12.5, 15.0, 0.5)[1:] + stations(15.4, 19.8, 0.4) + stations(20.2, D_CONE, 0.3)[1:]
    g = Geo()
    g.loft([ring_at(Y(d)) for d in sorted(set(round(d, 4) for d in ds))], "skin")
    # Blade antennas on top (VHF, satcom) and underneath (VHF, transponders), the red beacons.
    blade(g, top_pt(6.2, 0) - V((0, 0, 0.01)), 0.40, 0.26, 0.012)
    blade(g, top_pt(11.6, 0) - V((0, 0, 0.01)), 0.30, 0.18, 0.010)
    blade(g, top_pt(18.6, 0) - V((0, 0, 0.01)), 0.40, 0.26, 0.012)
    for d, ht in ((6.0, 0.24), (8.2, 0.12), (19.6, 0.22)):
        blade(g, V((0, Y(d), fuse_z(Y(d), 0, False) + 0.01)), 0.34, ht, 0.010, down=True)
    g.box(top_pt(13.5, 0) + V((0, 0, 0.02)), (0.045, 0.10, 0.03), "beacon")
    g.box(V((0, Y(12.9), fuse_z(Y(12.9), 0, False) - 0.02)), (0.045, 0.10, 0.03), "beacon")
    ob = g.finish("fuselage")
    door_cut = window_cutter("door cut", outline(door_pts(), 0.03), out=0.3, inn=0.12)
    door = copy_ob(ob, "entry door")
    boolean(ob, [door_cut])
    drop_faces(ob)
    boolean(ob, [main_bay(1), main_bay(-1), leg_slot(1), leg_slot(-1)])
    g = Geo()
    # The entry, seen through the open door: its floor, the bulkheads fore and
    # aft, the wall opposite and the lining under the roof (5 cm inside the skin).
    d0, d1, h0, h1 = DOOR
    da, db, xi, fl = d0 - 0.20, d1 + 0.20, -0.30, 2.08

    def lining(d, h):
        return side_pt(d, h, -1) * 1.0 + V((0.05, 0, 0))

    hs = stations(fl, 4.02, 0.12)
    g.poly([P(xi, da, fl), lining(da, fl), lining(db, fl), P(xi, db, fl)], "carpet", toward=P(-0.6, 5.0, 3.0))
    for dd in (da, db):
        pts = [P(xi, dd, fl)] + [lining(dd, h) for h in hs if abs(side_pt(dd, h, -1).x) > abs(xi) + 0.06] + [P(xi, dd, hs[-1])]
        g.poly(pts, "cabin", toward=P(-0.6, 5.0, 3.0))
    g.poly([P(xi, da, fl), P(xi, db, fl), P(xi, db, hs[-1]), P(xi, da, hs[-1])], "cabin", toward=P(-1.0, 5.0, 3.0))
    roof = [h for h in hs if abs(side_pt(da, h, -1).x) > abs(xi) + 0.06 and h > 3.3]
    g.strip([[lining(dd, h) for h in roof] + [P(xi, dd, hs[-1])] for dd in (da, db)], "cabin", toward=P(-0.5, 5.0, 2.6))
    join(ob, g.finish("entry"))

    # The main door: the skin inside the cutter, given its thickness, and the airstair on its inside.
    boolean(door, [door_cut], op="INTERSECT", smooth=30)
    drop_faces(door)
    keep_outer_skin(door)
    set_mat(door, "skin")
    md = door.modifiers.new("thick", "SOLIDIFY")
    md.thickness, md.offset, md.use_rim = 0.07, -1.0, True
    door.data.materials.append(MAT["door_in"])
    md.material_offset, md.material_offset_rim = 1, 1
    deps = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(door.evaluated_get(deps))
    door.modifiers.clear()
    door.data = me
    hinge0 = side_pt(DOOR[0] + 0.05, DOOR[2] + 0.02, -1)
    hinge1 = side_pt(DOOR[1] - 0.05, DOOR[2] + 0.02, -1)
    axis = (hinge1 - hinge0).normalized()   # (pointing aft: a positive turn swings the door's top out and down)
    # The steps, built as they stand with the door open (treads level), turned back shut with it.
    shut = Matrix.Translation(hinge0) @ Matrix.Rotation(-DOOR_OPEN, 4, axis) @ Matrix.Translation(-hinge0)
    opened = Matrix.Translation(hinge0) @ Matrix.Rotation(DOOR_OPEN, 4, axis) @ Matrix.Translation(-hinge0)
    top_open = opened @ side_pt((DOOR[0] + DOOR[1]) / 2, DOOR[3], -1)
    foot = (hinge0 + hinge1) / 2
    along = (top_open - foot)
    g = Geo()
    n0 = 0
    for k in range(1, 7):
        c = foot + along * (k / 7.0) + V((0, 0, 0.17))
        g.box(c, (0.14, 0.36, 0.018), "tread")
    for dy in (-0.40, 0.40):
        a, b = foot + V((0, dy, 0.03)), top_open + V((0, dy, 0.03))
        g.plate([a, b, b + V((0, 0, 0.22)), a + V((0, 0, 0.22))], V((0, 1, 0)), 0.012, "door_in")
    # Handrail posts and the rail, on the outboard edge.
    for dy in (-0.42,):
        for t in (0.25, 0.95):
            base = foot + along * t + V((0, dy, 0.15))
            tube(g, [base, base + V((0, 0, 0.85))], 0.012, "chrome", n=6)
        tube(g, [foot + along * 0.25 + V((0, dy, 1.0)), foot + along * 0.95 + V((0, dy, 1.0))], 0.012, "chrome", n=6)
    st = g.finish("steps")
    st.data.transform(shut)
    join(door, st, smooth=30)
    door["pivot"], door["axis"], door["angle"] = list(hinge0), list(axis), DOOR_OPEN

    # The cabin windows: tinted acrylic a few millimetres proud of the skin (their frames are painted).
    g = Geo()
    for s in (1, -1):
        for d in WIN_D:
            ring = []
            for k in range(28):
                a = 2 * math.pi * k / 28
                ring.append(to_skin(side_pt(d + WIN_SIZE[0] / 2 * math.cos(a), WIN_H + WIN_SIZE[1] / 2 * math.sin(a), s), 0.004))
            c = to_skin(side_pt(d, WIN_H, s), 0.005)
            vc = g.bm.verts.new(c)
            vr = [g.bm.verts.new(p) for p in ring]
            for a, b in zip(vr, vr[1:] + vr[:1]):
                g.face([vc, a, b], "glass", toward=fuse_out(c))
    g.finish("cabin windows", smooth=60, recalc=False)

    # The tail cone, with the APU's exhaust at its end.
    g = Geo()
    ds = stations(D_CONE, 26.4, 0.25) + [26.6, 26.8, 26.92, D_TAIL]
    rings = [ring_at(Y(d)) for d in ds]
    g.loft(rings, "skin", True, False)
    aim(g, 0, fuse_out)
    end = rings[-1]
    c = sum(end, V()) / len(end)
    inner = [c + (p - c) * 0.80 for p in end]
    deep = [c + (p - c) * 0.75 + V((0, 0.35, 0)) for p in end]
    n0 = len(g.bm.faces)
    g.loft([end, inner], "exhaust", False, False)
    aim(g, n0, lambda q: q + V((0, -20, 0)))
    n0 = len(g.bm.faces)
    g.loft([inner, deep], "nozzle_in", False, True)
    aim(g, n0, lambda q: V((0, q.y, c.z)) if q.y < c.y + 0.34 else q + V((0, -20, 0)))
    g.finish("tail cone", recalc=False)
    g = Geo()
    g.box(P(0, D_TAIL - 0.35, 3.62), (0.03, 0.05, 0.02), "tail_white")
    g.finish("tail light", smooth=10)


def keep_outer_skin(ob):
    """Of a skin patch cut out with a cutter, keep only the faces that look outward."""
    bm = bmesh.new()
    bm.from_mesh(ob.data)
    bad = []
    for f in bm.faces:
        c = f.calc_center_median()
        r = V((c.x, 0, c.z - fuse_axis(c.y).z))
        if f.normal.dot(r) < 0:
            bad.append(f)
    bmesh.ops.delete(bm, geom=bad, context="FACES")
    bm.to_mesh(ob.data)
    bm.free()


# ---------------------------------------------------------------- wing
# (kept in step with wing_sect in src/aircraft/g650/mod.rs)
# A reference trapezoid fitted to the 119.2 m2 area, the 28.55 m span at the
# tips (without the winglets) and the TCDS's 4.756 m MAC: 7.0 m chord on the
# centreline, 1.35 m at the tip, the leading edge swept 39.6 degrees (36 at the
# quarter chord). Inboard of 4.6 m the trailing edge is less swept (the root's
# extension). Its chord plane rises 4.5 degrees outboard; sections are
# symmetric, 12.5% thick at the root, 9% at the tip (estimates).
W_X1 = 14.27            # the tip, where the winglet takes over
W_LE0, W_TAN = 8.33, 0.826
W_TE0, W_TE_TAN = 15.33, 0.430
W_KINK, W_TE_IN = 4.6, 0.14
W_H0, W_DIH = 1.66, math.tan(math.radians(4.5))
W_T0, W_T1 = 0.125, 0.09
# Span stations of the moving surfaces, and the chord fraction they hinge at.
FLAPS = (("inboard flap", 1.60, 4.58, 0.80), ("outboard flap", 4.66, 10.58, 0.75))
AILERON = (10.66, 13.72, 0.75)
SPOILERS = (("ground spoiler", 1.75, 4.45, 0.60, 0.79), ("spoiler 1", 4.85, 6.70, 0.56, 0.745), ("spoiler 2", 6.75, 8.60, 0.56, 0.745), ("spoiler 3", 8.65, 10.40, 0.56, 0.745))
FLAP_FULL = math.radians(39.0)   # the flap lever's last notch (0, 10, 20, 39)
SPOILER_UP = math.radians(48.0)


def wing_le(x):
    return W_LE0 + W_TAN * x


def wing_te(x):
    return W_TE0 + W_TE_TAN * x if x >= W_KINK else W_TE0 + W_TE_TAN * W_KINK - W_TE_IN * (W_KINK - x)


def wing_c(x):
    return wing_te(x) - wing_le(x)


def wing_t(x):
    return lerp(W_T0, W_T1, min(max(x / W_X1, 0.0), 1.0))


def wing_sect(x, s=1):
    """Leading edge, chord, chord direction, up: the section at span station x (side s)."""
    le = P(x * s, wing_le(x), W_H0 + W_DIH * x)
    return le, wing_c(x), V((0, -1, 0)), V((-W_DIH * s, 0, 1)).normalized()


def wing_ring(s, x, a, b, m=18):
    return foil_ring(wing_sect(x, s), wing_t(x), a, b, m)


def wing_pt(x, xc, side, s=1):
    le, c, cdir, up = wing_sect(x, s)
    return le + cdir * (c * xc) + up * (side * max(c * naca_t(xc, wing_t(x)), 0.004))


def wing_z(x, y, top):
    """Height of the wing's upper or lower surface at (x, y)."""
    x = min(max(abs(x), 0.0), W_X1)
    c = wing_c(x)
    xc = min(max(((CG_D - y) - wing_le(x)) / c, 0.0), 1.0)
    return wing_pt(x, xc, 1 if top else -1).z


def in_wing(p, tol=0.0):
    x = abs(p.x)
    d = CG_D - p.y
    return 0.0 <= x <= W_X1 and wing_le(x) < d < wing_te(x) and wing_z(x, p.y, False) + tol < p.z < wing_z(x, p.y, True) - tol


def hinge_pt(x, xc, s=1):
    le, c, cdir, _ = wing_sect(x, s)
    return le + cdir * (c * xc)


# The winglet: blended up from the tip on a curve, then straight, canted 18
# degrees out from the vertical, its tip 15.18 m out (span 30.36 m) and about
# 1.7 m above the wing's tip; swept back, 1.35 m chord at its root, 0.55 at its tip.
WL_R, WL_CANT, WL_TOP_X = 0.50, math.radians(72.0), 15.16
WL_LE_TIP, WL_C_TIP = 22.60, 0.55


def winglet_sects(s, n=12):
    out = []
    bend = WL_R * WL_CANT
    straight = (WL_TOP_X - W_X1 - WL_R * math.sin(WL_CANT)) / math.cos(WL_CANT)
    total = bend + straight
    le0, h0 = wing_le(W_X1), W_H0 + W_DIH * W_X1
    for k in range(n + 1):
        u = k / n
        l = total * u
        if l <= bend:
            ph = l / WL_R
            x, h = W_X1 + WL_R * math.sin(ph), h0 + WL_R * (1 - math.cos(ph))
        else:
            ph = WL_CANT
            x = W_X1 + WL_R * math.sin(ph) + (l - bend) * math.cos(ph)
            h = h0 + WL_R * (1 - math.cos(ph)) + (l - bend) * math.sin(ph)
        ph = max(ph, math.atan(W_DIH))
        le = P(x * s, lerp(le0, WL_LE_TIP, u ** 1.15), h)
        c = lerp(wing_c(W_X1), WL_C_TIP, u)
        out.append(((le, c, V((0, -1, 0)), V((-math.sin(ph) * s, 0, math.cos(ph))).normalized()), lerp(W_T1, 0.08, u)))
    return out


def wings():
    area = 0.0
    xs = stations(0.0, W_X1, 0.05)
    for a, b in zip(xs, xs[1:]):
        area += (wing_c(a) + wing_c(b)) * (b - a)
    print(f"wing: {area:.1f} m2 to the tips with the root's extension (reference trapezoid 119.2 m2)")
    for s, side in ((1, "right"), (-1, "left")):
        # The root, which the main gear's bay and its leg's slot are cut into: a
        # closed loft of its own (flaps' chord stops behind 1.6 m), carved, then joined.
        ROOT = 2.6
        LE_STRIP = 0.05
        g = Geo()
        g.loft([wing_ring(s, x, LE_STRIP, 1.0 if x < FLAPS[0][1] else FLAPS[0][3]) for x in stations(0.0, 1.55, 0.25) + stations(FLAPS[0][1], ROOT, 0.25)], "skin")
        root = g.finish(side + " wing root", smooth=30)
        boolean(root, [main_bay(s), leg_slot(s)], smooth=30)
        cuts = [(ROOT, FLAPS[0][2], FLAPS[0][3]), (FLAPS[1][1], FLAPS[1][2], FLAPS[1][3])]
        cuts += [(FLAPS[0][2], FLAPS[1][1], 1.0), (FLAPS[1][2], AILERON[0], 1.0), (AILERON[0], AILERON[1], AILERON[2]), (AILERON[1], W_X1, 1.0)]
        cuts.sort()
        g = Geo()
        g.loft([wing_ring(s, x, 0.0, LE_STRIP, m=8) for x in stations(0.0, ROOT, 0.25)], "lip", True, True)
        for x0, x1, xh in cuts:
            xs = stations(x0, x1, 0.25)
            g.loft([wing_ring(s, x, LE_STRIP, xh) for x in xs], "skin", True, True)
            g.loft([wing_ring(s, x, 0.0, LE_STRIP, m=8) for x in xs], "lip", True, True)
        # The winglet, carried on from the tip.
        secs = winglet_sects(s)
        g.loft([foil_ring(sc, t, LE_STRIP, 1.0) for sc, t in secs], "skin", True, True)
        g.loft([foil_ring(sc, t, 0.0, LE_STRIP, m=8) for sc, t in secs], "lip", True, True)
        ob = g.finish(side + " wing", smooth=30)
        join(ob, root, smooth=30)

        # Flaps: Fowler flaps, hinged below the wing so that they run back as they go down.
        for name, x0, x1, xh in FLAPS:
            g = Geo()
            g.loft([wing_ring(s, x, xh + 0.012, 1.0, m=10) for x in stations(x0 + 0.01, x1 - 0.01, 0.3)], "skin")
            cf = wing_c((x0 + x1) / 2) * (1 - xh)
            p0, p1 = hinge_pt(x0, xh + 0.10 * (1 - xh), s), hinge_pt(x1, xh + 0.10 * (1 - xh), s)
            down = V((0, 0, -0.45 * cf))
            g.finish(f"{side} {name}", smooth=30, pivot=p0 + down, axis=pair_axis(M((p1 - p0), s).normalized(), s), angle=FLAP_FULL)
        # The aileron, with its static wicks.
        x0, x1, xh = AILERON
        g = Geo()
        g.loft([wing_ring(s, x, xh + 0.012, 1.0, m=10) for x in stations(x0 + 0.01, x1 - 0.01, 0.3)], "skin")
        for xw in (lerp(x0, x1, 0.35), lerp(x0, x1, 0.85)):
            wick(g, hinge_pt(xw, 1.0, s))
        p0, p1 = hinge_pt(x0, xh, s), hinge_pt(x1, xh, s)
        g.finish(f"{side} aileron", smooth=30, pivot=p0, axis=pair_axis(M((p1 - p0), s).normalized(), s))
        # Spoilers: panels lying on the upper skin, hinged along their front edges, rising trailing edge up.
        for name, x0, x1, a, b in SPOILERS:
            g = Geo()
            n0 = len(g.bm.faces)
            xs = stations(x0, x1, 0.3)
            xcs = [lerp(a, b, k / 6) for k in range(7)]
            top = [[wing_pt(x, xc, 1, s) + V((0, 0, 0.006)) for xc in xcs] for x in xs]
            bot = [[p - V((0, 0, 0.014)) for p in row] for row in top]
            g.strip(top, "skin", toward=V((0, 0, 60)))
            g.strip(bot, "door_in", toward=V((0, 0, -60)))
            pc = sum((p for r in top for p in r), V()) / (len(top) * len(top[0]))
            for ra, rb in ((top[0], bot[0]), (top[-1], bot[-1]), ([r[0] for r in top], [r[0] for r in bot]), ([r[-1] for r in top], [r[-1] for r in bot])):
                em = sum(ra, V()) / len(ra)
                g.strip([ra, rb], "door_in", toward=em + (em - pc) * 10)
            p0, p1 = wing_pt(x0, a, 1, s) + V((0, 0, 0.006)), wing_pt(x1, a, 1, s) + V((0, 0, 0.006))
            g.finish(f"{side} {name}", smooth=30, pivot=p0, axis=pair_axis(-M((p1 - p0), s).normalized(), s), angle=SPOILER_UP, recalc=False)
        # Lights at the tip: navigation (red left, green right) at the winglet's foot, a white strobe behind it.
        g = Geo()
        le = wing_sect(W_X1 - 0.06, s)[0]
        g.box(le + V((0, -0.10, 0)), (0.04, 0.07, 0.025), "nav_green" if s > 0 else "nav_red")
        g.box(le + V((0, -0.24, 0)), (0.035, 0.05, 0.022), "strobe")
        g.finish(side + " nav light", smooth=10)
        # Landing light in the wing root's leading edge.
        g = Geo()
        p = wing_pt(1.9, 0.01, 0, s)
        g.box(p + V((0, 0.005, 0)), (0.09, 0.008, 0.045), "landing")
        g.finish(side + " landing light", smooth=10)


# ---------------------------------------------------------------- engines
# Two Rolls-Royce BR725 A1-12s in nacelles (Spirit AeroSystems) on short pylons
# either side of the rear fuselage. The cowl's aft part is the thrust
# reverser's translating sleeve (its jigsaw seam shows in photographs): it
# slides back to open the cascades. The exhaust leaves through the BR725's
# scalloped mixer nozzle round a plug. Station (d) and radius of the cowl.
NAC_X, NAC_H = 2.20, 3.75
NAC_PROFILE = [(18.85, 0.655), (18.90, 0.705), (19.00, 0.735), (19.25, 0.765), (19.70, 0.785), (20.40, 0.790), (21.40, 0.780), (22.20, 0.755), (22.75, 0.728)]
SLEEVE = [(22.77, 0.727), (23.10, 0.700), (23.40, 0.660), (23.58, 0.630)]
SLEEVE_TRAVEL = 0.45
LIP_IN = 0.600   # the intake's throat
FAN_D = 19.35
NOZZLE = (23.50, 24.00, 0.555, 0.470)   # the mixer nozzle: from, to; radius at each
PLUG = (23.70, 24.45, 0.30)             # the plug: from, its point; its radius


def nac_ring(s, d, r, n=40, sc=None):
    c = P(NAC_X * s, d, NAC_H)
    return [c + V((r * math.cos(2 * math.pi * k / n), 0, r * math.sin(2 * math.pi * k / n))) for k in range(n)]


def nacelles():
    for s, side in ((1, "right"), (-1, "left")):
        c = P(NAC_X * s, 0, NAC_H)
        out = lambda q: q + V((q.x - c.x, 0, q.z - c.z)).normalized() * 20
        g = Geo()
        # The cowl.
        g.loft([nac_ring(s, d, r) for d, r in NAC_PROFILE], "skin", False, False)
        aim(g, 0, out)
        # The lip, polished, rolling in to the throat, and the intake back to the fan.
        n0 = len(g.bm.faces)
        lip = [(18.85, 0.655), (18.80, 0.640), (18.78, 0.620), (18.80, 0.605), (18.86, 0.600), (18.95, LIP_IN)]
        g.loft([nac_ring(s, d, r) for d, r in lip], "lip", False, False)
        aim(g, n0, lambda q: q + (q - (V((c.x, q.y, c.z)) + V((q.x - c.x, 0, q.z - c.z)).normalized() * 0.628) - V((0, 0.06, 0)) * 0) * 20)
        n0 = len(g.bm.faces)
        g.loft([nac_ring(s, d, r) for d, r in ((18.95, LIP_IN), (19.20, 0.630), (FAN_D, 0.635))], "inlet", False, False)
        aim(g, n0, lambda q: V((c.x, q.y, c.z)))
        # The fan: a spinner and 22 wide-chord blades in front of a dark face.
        lathe(g, P(NAC_X * s, FAN_D + 0.02, NAC_H), V((0, -1, 0)), [(0.0, -0.30), (0.10, -0.24), (0.17, -0.12), (0.19, 0.0), (0.19, 0.06)], "spinner", n=24, caps=(False, False))
        g.poly(nac_ring(s, FAN_D + 0.10, 0.64, n=40), "inlet", toward=P(NAC_X * s, 0.0, NAC_H))
        for k in range(22):
            a = 2 * math.pi * (k + 0.5) / 22
            u = V((math.cos(a), 0, math.sin(a)))
            t = V((-math.sin(a), 0, math.cos(a)))
            base = P(NAC_X * s, FAN_D + 0.03, NAC_H) + u * 0.19
            tip = P(NAC_X * s, FAN_D + 0.03, NAC_H) + u * 0.625
            tw = V((0, 1, 0))
            g.plate([base + t * 0.05 + tw * 0.04, tip + t * 0.10 + tw * 0.02, tip - t * 0.10 - tw * 0.02, base - t * 0.05 - tw * 0.04], u.cross(t * 0.05 + tw * 0.04).normalized(), 0.004, "fan")
        # The reverser's cascades, under the sleeve, and the bypass duct's inner wall.
        n0 = len(g.bm.faces)
        g.loft([nac_ring(s, 22.70, 0.722), nac_ring(s, 22.80, 0.640), nac_ring(s, 23.25, 0.620)], "cascade", False, False)
        aim(g, n0, out)
        for k in range(16):
            a = 2 * math.pi * k / 16
            u = V((math.cos(a), 0, math.sin(a)))
            g.box(P(NAC_X * s, 23.02, NAC_H) + u * 0.625, (0.010, 0.21, 0.012), "cascade", (u.cross(V((0, 1, 0))).normalized(), V((0, 1, 0)), u))
        # The mixer nozzle: 24 scallops round its trailing edge; the plug inside.
        n0 = len(g.bm.faces)
        d0, d1, r0, r1 = NOZZLE
        rows = []
        for k in range(6):
            t = k / 5
            row = []
            for j in range(72):
                a = 2 * math.pi * j / 72
                scallop = 0.5 + 0.5 * math.cos(24 * a)
                dd = lerp(d0, d1 - 0.10 * scallop, t)
                rr = lerp(r0, r1, t)
                row.append(P(NAC_X * s, dd, NAC_H) + V((rr * math.cos(a), 0, rr * math.sin(a))))
            rows.append(row)
        g.loft(rows, "exhaust", False, False)
        aim(g, n0, out)
        n0 = len(g.bm.faces)
        g.loft([[p - (p - V((c.x, p.y, c.z))).normalized() * 0.015 for p in row] for row in rows], "nozzle_in", False, False)
        aim(g, n0, lambda q: V((c.x, q.y, c.z)))
        g.loft([[p - (p - V((c.x, p.y, c.z))).normalized() * 0.015 for p in row] for row in (rows[-1],)] + [rows[-1]], "exhaust", False, False)
        lathe(g, P(NAC_X * s, PLUG[0], NAC_H), V((0, -1, 0)), [(PLUG[2], 0.0), (PLUG[2] * 0.97, 0.25), (PLUG[2] * 0.75, 0.48), (PLUG[2] * 0.35, 0.68), (0.0, PLUG[1] - PLUG[0])], "exhaust", n=24, caps=(True, False))
        # The pylon to the fuselage.
        def pylon(u):
            x = lerp(0.95, NAC_X - 0.70, u)
            d0 = lerp(20.10, 20.55, u)
            return P(x * s, d0, NAC_H - 0.05), lerp(3.0, 2.45, u), V((0, -1, 0)), V((0, 0, 1))
        surface(g, pylon, 0.0, 1.0, 0.0, 1.0, 0.07, nu=4)
        g.finish(side + " nacelle", smooth=35, recalc=False)

        # The translating sleeve: slides aft along the engine's axis.
        g = Geo()
        g.loft([nac_ring(s, d, r) for d, r in SLEEVE], "skin", False, False)
        aim(g, 0, out)
        n0 = len(g.bm.faces)
        g.loft([nac_ring(s, d, r - 0.035) for d, r in SLEEVE], "nozzle_in", False, False)
        aim(g, n0, lambda q: V((c.x, q.y, c.z)))
        g.loft([nac_ring(s, SLEEVE[-1][0], SLEEVE[-1][1]), nac_ring(s, SLEEVE[-1][0], SLEEVE[-1][1] - 0.035)], "skin", False, False)
        g.loft([nac_ring(s, SLEEVE[0][0], SLEEVE[0][1] - 0.035), nac_ring(s, SLEEVE[0][0], SLEEVE[0][1])], "door_in", False, False)
        g.finish(side + " reverser", smooth=35, pivot=P(NAC_X * s, SLEEVE[0][0], NAC_H), axis=V((0, -1, 0)), angle=SLEEVE_TRAVEL, recalc=False)


# ---------------------------------------------------------------- tail
# The fin (d, h): its leading edge from (23.4, 4.4) swept back 33 degrees, a
# dorsal fillet ahead of it; the rudder's trailing edge from the tail cone's end
# (27.0, 3.5) to (28.35, 7.3). On top, the bullet fairing to 7.82 m, and the
# T-tail's stabiliser at h 7.45: 2.70 m chord at the root, 1.26 m at its tips
# 5.45 m out, their trailing edges 30.41 m from the nose.
FIN_H = (3.60, 7.45)
RUDDER = (3.78, 7.15, 0.70)   # from, to (h); hinge (chord fraction)
STAB_H, STAB_X1 = 7.45, 5.45
STAB_LE, STAB_TAN = 25.45, 0.679
STAB_C0, STAB_C1 = 2.70, 1.26
ELEVATOR = (0.32, 5.28, 0.70)
BULLET = (24.85, 28.75, 7.55, 0.27)


def fin_le(h):
    return min(23.4 + 0.65 * (h - 4.4), 21.3 + (h - 3.90) * 3.19)


def fin_te(h):
    return 27.0 + 0.355 * (h - 3.5)


def fin_sect(h):
    le = P(0, fin_le(h), h)
    return le, fin_te(h) - fin_le(h), V((0, -1, 0)), V((1, 0, 0))


def fin_hinge(h):
    """The rudder's hinge line (a straight line): 30% of the fin's chord (without the fillet) ahead of its trailing edge."""
    main_le = 23.4 + 0.65 * (h - 4.4)
    xd = fin_te(h) - (1 - RUDDER[2]) * (fin_te(h) - main_le)
    return (xd - fin_le(h)) / (fin_te(h) - fin_le(h))


def fin_t(h):
    c = fin_te(h) - fin_le(h)
    return min(0.11, 0.40 / c)


def stab_sect(x):
    ax = abs(x)
    return P(x, STAB_LE + STAB_TAN * ax, STAB_H), STAB_C0 - (STAB_C0 - STAB_C1) * ax / STAB_X1, V((0, -1, 0)), V((0, 0, 1))


def tail():
    g = Geo()
    hs = [FIN_H[0], 3.75, RUDDER[0]] + stations(3.9, 4.8, 0.15)[1:] + stations(5.1, RUDDER[1], 0.35) + [FIN_H[1]]
    hs = sorted(set(round(h, 4) for h in hs))
    for a, b in zip(hs, hs[1:]):
        rud = RUDDER[0] <= a and b <= RUDDER[1]
        g.loft([foil_ring(fin_sect(h), fin_t(h), 0.0, fin_hinge(h) if rud else 1.0, 16) for h in (a, b)], "skin")
    # The bullet on top.
    d0, d1, hb, rb = BULLET
    lathe(g, P(0, d0, hb), V((0, -1, 0)), [(0.0, 0.0), (rb * 0.45, 0.10), (rb * 0.80, 0.45), (rb, 1.2), (rb, 2.8), (rb * 0.80, 3.4), (rb * 0.35, 3.82), (0.0, d1 - d0)], "skin", n=24, caps=(False, False))
    g.finish("fin")
    g = Geo()
    g.loft([foil_ring(fin_sect(h), fin_t(h), fin_hinge(h) + 0.01, 1.0, 10) for h in stations(RUDDER[0] + 0.01, RUDDER[1] - 0.01, 0.3)], "skin")
    for hw in (lerp(RUDDER[0], RUDDER[1], 0.45), lerp(RUDDER[0], RUDDER[1], 0.85)):
        le, c, cdir, _ = fin_sect(hw)
        wick(g, le + cdir * c, up=V((1, 0, 0)))
    p0, p1 = hinge(fin_sect, RUDDER[0], fin_hinge(RUDDER[0])), hinge(fin_sect, RUDDER[1], fin_hinge(RUDDER[1]))
    g.finish("rudder", pivot=p0, axis=(p1 - p0).normalized())

    g = Geo()
    x0, x1, xh = ELEVATOR
    cuts = [-STAB_X1, -x1, -x0, x0, x1, STAB_X1]
    for k, (a, b) in enumerate(zip(cuts, cuts[1:])):
        xb = xh if k in (1, 3) else 1.0
        xs = stations(a, b, 0.3)
        g.loft([foil_ring(stab_sect(x), 0.10, 0.04, xb) for x in xs], "skin", k == 0, k == 4)
        g.loft([foil_ring(stab_sect(x), 0.10, 0.0, 0.04, 8) for x in xs], "lip", k == 0, k == 4)
    g.finish("stabiliser")
    for s, side in ((1, "right"), (-1, "left")):
        g = Geo()
        g.loft([foil_ring(stab_sect(x * s), 0.10, xh + 0.012, 1.0, 10) for x in stations(x0 + 0.01, x1 - 0.01, 0.3)], "skin")
        for xw in (lerp(x0, x1, 0.4), lerp(x0, x1, 0.9)):
            le, c, cdir, _ = stab_sect(xw * s)
            wick(g, le + cdir * c)
        p0, p1 = hinge(stab_sect, x0, xh), hinge(stab_sect, x1, xh)
        g.finish(side + " elevator", pivot=M(p0, s), axis=pair_axis((p1 - p0).normalized(), s))


# ---------------------------------------------------------------- landing gear (modelled down and locked, the oleos extended)
# The tyres meet the ground with the oleos fully extended at h = GROUND_H (10 cm
# below the ground at rest, where they settle under the aircraft's weight).
# Nose: twin 21 x 7.25-10 tyres on a steerable leg raked forward, retracting
# forward into its well under the flight deck. Mains: twin H37.5 x 12.0 R19 on
# each leg, hung from the wing just outboard of the fuselage, retracting
# inward, the wheels lying flat in bays in the belly fairing.
GROUND_H = -0.10
NOSE_R, NOSE_HW = 0.267, 0.092
NOSE_PIVOT_DH = (3.10, 1.86)
NOSE_AXLE_D = 2.40
NOSE_STOW_DH = (1.30, 1.88)
NOSE_TRACK = 0.13          # each tyre's middle from the leg's
MAIN_R, MAIN_HW = 0.476, 0.152
MAIN_PIVOT = (2.18, 15.05, 1.86)   # x, d, h
MAIN_AXLE = (1.90, 15.55)          # x, d
MAIN_SWING = math.radians(74.5)    # the main legs swing in about a fore-and-aft axis through their pivots
WHEEL_X = 0.19                     # each tyre's middle from the axle's middle


def nose_points():
    pivot = P(0, *NOSE_PIVOT_DH)
    axle = P(0, NOSE_AXLE_D, GROUND_H + NOSE_R)
    l = (axle - pivot).length
    dz = Z(NOSE_STOW_DH[1]) - pivot.z
    stowed = pivot + V((0, math.sqrt(max(l * l - dz * dz, 0.0)), dz))
    return pivot, axle, stowed


def main_points(s=1):
    x, d, h = MAIN_PIVOT
    pivot = P(x * s, d, h)
    axle = P(MAIN_AXLE[0] * s, MAIN_AXLE[1], GROUND_H + MAIN_R)
    axis = V((0, s, 0))
    stowed = pivot + Matrix.Rotation(MAIN_SWING, 3, axis) @ (axle - pivot)
    return pivot, axle, stowed, axis


def skin_patch(g, s, x0, x1, y0, y1, zf, drop, t, nx, ny):
    """A patch of the underside's skin (x0..x1 out from the centreline on side
    s, stations y0 back to y1, at height zf(x, y) less `drop`), t thick: a
    door, shut. Returns its vertices."""
    n0 = len(g.bm.faces)
    ys = [lerp(y0, y1, k / ny) for k in range(ny + 1)]
    lo = [[g.bm.verts.new(V((s * x, y, zf(x, y) - drop))) for x in (lerp(x0, x1, (1 - math.cos(math.pi * k / nx)) / 2) for k in range(nx + 1))] for y in ys]
    hi = [[g.bm.verts.new(v.co + V((0, 0, t))) for v in row] for row in lo]
    for j in range(ny):
        for i in range(nx):
            g.face([lo[j][i], lo[j][i + 1], lo[j + 1][i + 1], lo[j + 1][i]], "skin")
            g.face([hi[j][i], hi[j + 1][i], hi[j + 1][i + 1], hi[j][i + 1]], "door_in")
    rim = [(j, 0) for j in range(ny)] + [(ny, i) for i in range(nx)] + [(j, nx) for j in range(ny, 0, -1)] + [(0, i) for i in range(nx, 0, -1)]
    for (ja, ia), (jb, ib) in zip(rim, rim[1:] + rim[:1]):
        g.face([lo[ja][ia], lo[jb][ib], hi[jb][ib], hi[ja][ia]], "door_in")
    if (s < 0) != (y1 > y0):
        g.bm.faces.ensure_lookup_table()
        bmesh.ops.reverse_faces(g.bm, faces=g.bm.faces[n0:])
    return [v for row in lo + hi for v in row]


def patch_door(name, s, x0, x1, y0, y1, hinge_x, open_angle, zf, nx=10, ny=12, t=0.02, brackets=(0.2, 0.8)):
    """A gear door cut from the skin itself, hinged along x = hinge_x; modelled
    open (turned by open_angle about the hinge, its free edge going down),
    hung on two brackets at those fractions of its length."""
    g = Geo()
    vs = skin_patch(g, s, x0, x1, y0, y1, zf, 0.002, t, nx, ny)
    h0 = V((s * hinge_x, y0, zf(hinge_x, y0)))
    h1 = V((s * hinge_x, y1, zf(hinge_x, y1)))
    axis = (h1 - h0).normalized()
    xf = x0 + x1 - hinge_x
    free = V((s * xf, (y0 + y1) / 2, zf(xf, (y0 + y1) / 2)))
    rot = Matrix.Translation(h0) @ Matrix.Rotation(open_angle, 4, axis) @ Matrix.Translation(-h0)
    if (rot @ free).z > free.z:
        axis = -axis
        rot = Matrix.Translation(h0) @ Matrix.Rotation(open_angle, 4, axis) @ Matrix.Translation(-h0)
    for v in vs:
        v.co = rot @ v.co
    for f in brackets:
        g.box(rot @ (h0.lerp(h1, f) + V((0, 0, 0.04))), (0.03, 0.05, 0.04), "well")
    g.finish(name, smooth=30, pivot=h0, axis=axis, angle=-open_angle, recalc=True)


def belly(x, y):
    """The underside's height at (x, y): the fuselage's or the wing's, whichever is lower."""
    zs = [fuse_z(y, x, False)] if abs(x) < fuse_row(y)[1] else []
    if abs(x) <= W_X1 and wing_le(abs(x)) < CG_D - y < wing_te(abs(x)):
        zs.append(wing_z(x, y, False))
    return min(zs) if zs else fuse_z(y, x, False)


def gear():
    # Nose.
    pivot, axle, stowed = nose_points()
    ax, ang = stow(pivot, axle, stowed)
    along = (pivot - axle).normalized()
    low = axle + along * 0.62
    g = Geo()
    tube(g, [pivot + V((-0.20, 0, 0)), pivot + V((0.20, 0, 0))], 0.05, "strut", n=10)
    tube(g, [pivot, low], 0.075, "strut", n=14)
    lathe(g, low + along * 0.10, -along, [(0.0, 0.0), (0.10, 0.0), (0.10, 0.12), (0.0, 0.12)], "strut", n=14)
    # The drag brace, back up into the well, and the taxi and landing lights on the leg.
    knee = pivot.lerp(low, 0.55)
    tube(g, [knee, knee + V((0, -0.30, 0.32))], 0.028, "strut", n=8)
    for x in (-0.07, 0.07):
        g.box(knee + V((x, 0.09, 0)), (0.05, 0.03, 0.045), "strut")
        g.box(knee + V((x, 0.122, 0)), (0.04, 0.004, 0.035), "landing")
    for x in (-0.03, 0.03):
        tube(g, [pivot + V((x, -0.08, 0)) - along * 0.05, low + V((x, -0.08, 0))], 0.008, "steel", n=5)
    g.finish("nose leg", pivot=pivot, axis=ax, angle=ang)

    down = (axle - pivot).normalized()
    g = Geo()
    tube(g, [low + along * 0.08, axle], 0.055, "chrome", n=12)
    g.box(axle + along * 0.04, (0.06, 0.05, 0.06), "strut")
    tube(g, [axle + V((-NOSE_TRACK - NOSE_HW, 0, 0)), axle + V((NOSE_TRACK + NOSE_HW, 0, 0))], 0.03, "strut")
    scissors(g, low + V((0, -0.07, 0)), axle + along * 0.06 + V((0, -0.05, 0)), V((0, -0.15, 0)))
    g.finish("nose fork", pivot=axle, axis=down, parent="nose leg")
    g = Geo()
    for x in (-NOSE_TRACK, NOSE_TRACK):
        wheel(g, axle + V((x, 0, 0)), NOSE_R, NOSE_HW)
    g.finish("nose wheel", pivot=axle, axis=V((1, 0, 0)), parent="nose fork")

    # Mains.
    for s, side in ((1, "right"), (-1, "left")):
        pivot, axle, stowed, ax = main_points(s)
        ang = MAIN_SWING
        along = (pivot - axle).normalized()
        low = axle + along * 0.62
        unstow = Matrix.Translation(pivot) @ Matrix.Rotation(-ang, 4, ax) @ Matrix.Translation(-pivot)
        g = Geo()
        tube(g, [pivot + V((0, 0.28, 0)), pivot + V((0, -0.28, 0))], 0.07, "strut", n=10)
        tube(g, [pivot, low], 0.10, "strut", n=14)
        lathe(g, low + along * 0.10, -along, [(0.0, 0.0), (0.125, 0.0), (0.125, 0.12), (0.0, 0.12)], "strut", n=14)
        # The side brace up to the wing, and the hydraulic lines.
        tube(g, [pivot.lerp(low, 0.5), pivot + V((0.0, 0.55, 0.05))], 0.035, "strut", n=8)
        for y in (-0.05, 0.05):
            tube(g, [pivot + V((0.10 * s, y, -0.08)), low + V((0.10 * s, y, 0.08))], 0.009, "steel", n=5)
        # The leg's door: the wing's (and the fairing's) lower skin over its slot, laid out shut and swung down with the leg.
        x0, x1, d0, d1 = LEG_SLOT
        # (From the main bay's edge, where the bay's own door ends.)
        for v in skin_patch(g, s, max(x0, MAIN_BAY[1]) + 0.008, x1 - 0.008, Y(d0 + 0.008), Y(d1 - 0.008), lambda x, y: belly(x, y), 0.004, 0.02, 12, 4):
            v.co = unstow @ v.co
        g.finish(side + " main leg", pivot=pivot, axis=ax, angle=ang)

        # The oleo's piston, carrying the axle and the wheels: it rides up the leg as the oleo compresses.
        g = Geo()
        tube(g, [low + along * 0.08, axle], 0.075, "chrome", n=12)
        g.box(axle, (0.08, 0.10, 0.08), "strut")
        tube(g, [axle + V((-WHEEL_X - MAIN_HW, 0, 0)), axle + V((WHEEL_X + MAIN_HW, 0, 0))], 0.05, "steel", n=10)
        for k in (-1, 1):
            # the carbon brakes inside the hubs
            lathe(g, axle + V((k * (WHEEL_X - MAIN_HW + 0.03), 0, 0)), V((k, 0, 0)), [(0.0, 0.0), (0.24, 0.0), (0.24, 0.10), (0.0, 0.10)], "steel", n=16)
        scissors(g, low + V((0, 0.09, 0)), axle + along * 0.08 + V((0, 0.08, 0)), V((0, 0.20, 0)))
        g.finish(side + " main oleo", pivot=axle, axis=V((0, 0, 1)), parent=side + " main leg")
        g = Geo()
        for k in (-1, 1):
            wheel(g, axle + V((k * WHEEL_X, 0, 0)), MAIN_R, MAIN_HW)
        g.finish(side + " main wheel", pivot=axle, axis=V((1, 0, 0)), parent=side + " main oleo")
        print(f"{side} main gear: leg {(axle - pivot).length:.2f} m, swings {math.degrees(ang):.0f} deg, axle stowed at {tuple(round(c, 2) for c in stowed)}")
    p, a, st = nose_points()
    print(f"nose gear: leg {(a - p).length:.2f} m, swings {math.degrees(stow(p, a, st)[1]):.0f} deg, axle stowed at {tuple(round(c, 2) for c in st)}")


# The nose well's doors are split where the leg comes out of it: the forward
# pair (over the wheels' way in and out) is shut again once the gear is down,
# as Gulfstreams' are ("doors open, uplocks release, gear extends, doors
# close"); the aft pair, either side of the strut, stays open while it is down.
# (The split station is an estimate: the strut leaves the skin at about d 2.9.)
# The forward doors' hinge brackets sit aft of where the wheels pass them.
NOSE_DOOR_SPLIT = 2.62


def doors():
    d0, d1, hw, _ = NOSE_WELL
    bx0, bx1, by0, by1, _ = MAIN_BAY
    gap = 0.008
    for s, side in ((1, "right"), (-1, "left")):
        patch_door(side + " nose gear door", s, gap, hw - gap, Y(d0 + gap), Y(NOSE_DOOR_SPLIT - gap), hw, math.radians(86), lambda x, y: fuse_z(y, x, False), ny=13, brackets=(0.45, 0.9))
        patch_door(side + " aft nose gear door", s, gap, hw - gap, Y(NOSE_DOOR_SPLIT + gap), Y(d1 - gap), hw, math.radians(86), lambda x, y: fuse_z(y, x, False), ny=4)
        # The main wheels' doors are hinged along their inboard edge, by the
        # keel: the wheels swing in under the open door, which hangs clear of their way.
        patch_door(side + " main gear door", s, bx0 + gap, bx1 - gap, Y(by0 + gap), Y(by1 - gap), bx0, math.radians(85), belly, nx=12, ny=10)


def gear_local(obs, n, g):
    """How far a piece is turned out of its modelled pose (gear down, doors
    open) at gear position g (0 up, 1 down), as `anim` in
    src/aircraft/g650/mod.rs moves it: the legs over 0.15..0.85, the nose
    well's forward doors and the main gear's doors open only while the gear
    moves, the nose well's aft doors open while it is down."""
    o = obs[n]
    a = o.get("angle", 0.0)
    if abs(a) < 1e-6:
        return Matrix.Identity(4)
    if n.endswith(" leg"):
        a *= 1 - smoothstep(0.15, 0.85, g)
    elif n.endswith("aft nose gear door"):
        a *= 1 - smoothstep(0.0, 0.15, g)
    elif n.endswith("nose gear door"):
        a *= 1 - smoothstep(0.0, 0.15, g) * (1 - smoothstep(0.88, 1.0, g))
    elif n.endswith("main gear door"):
        a *= 1 - smoothstep(0.0, 0.18, g) * (1 - smoothstep(0.88, 1.0, g))
    else:
        return Matrix.Identity(4)
    p = V(o["pivot"])
    return Matrix.Translation(p) @ Matrix.Rotation(a, 4, V(o["axis"])) @ Matrix.Translation(-p)


def gear_world(obs, n, g):
    """gear_local with what the piece hangs from (its "parent")."""
    par = obs[n].get("parent")
    return (gear_world(obs, par, g) if par else Matrix.Identity(4)) @ gear_local(obs, n, g)


def check_gear():
    """Sweep the gear from up to down (as Rust `anim` moves it: the legs over
    0.15..0.85) and report any nose gear vertex inside the fuselage's skin but
    outside its well, any vertex of the stowed gear left outside the skin
    (the fuselage's or the wing's), and any door meeting the gear."""
    obs = {o.name: o for o in COLL.all_objects}

    def world(n, g):
        return gear_world(obs, n, g)

    def in_well(p):
        d0, d1, hw, roof = NOSE_WELL
        return abs(p.x) < hw + 0.01 and Y(d1) < p.y < Y(d0) and p.z < Z(roof)

    gear = [n for n in obs if "door" not in n and any(k in n for k in ("leg", "oleo", "wheel", "fork"))]
    bad = 0
    for k in range(21):
        g = k / 20
        for n in gear:
            m = world(n, g) @ obs[n].matrix_world
            worst, at = 0.0, None
            for v in obs[n].data.vertices:
                p = m @ v.co
                d = fuse_depth(p)
                e = 0.0
                if "main" in n:
                    if g == 0.0 and d < -0.01 and not in_wing(p, -0.01):
                        e = 0.02 if d < -0.5 else -d
                else:
                    if d > 0.0 and not in_well(p):
                        e = 1.0
                    if g == 0.0 and d < 0.018:
                        e = max(e, 0.018 - d)
                if e > worst:
                    worst, at, w = e, v.co, p
            if worst > 0.005:
                bad += 1
                what = "out of its well" if worst >= 1.0 else f"{worst * 100:.0f} cm outside the skin"
                print(f"  gear check: {n} at gear {g:.2f}: {what} (vertex at {tuple(round(c, 2) for c in at)} as modelled, at d {CG_D - w.y:.2f} h {w.z + CG_H:.2f} x {w.x:.2f})")
    print("gear check:", "clear of the skin all the way up and down" if not bad else f"{bad} clashes")
    # And the doors: none may meet the gear on its way up or down (nor with it down, the doors shut).
    from mathutils.bvhtree import BVHTree

    def tree(n, g):
        m = world(n, g) @ obs[n].matrix_world
        me = obs[n].data
        return BVHTree.FromPolygons([m @ v.co for v in me.vertices], [list(f.vertices) for f in me.polygons])

    pairs = [(d, n) for d in obs if "gear door" in d for n in gear if ("nose" in d) == ("nose" in n) and (d.split()[0] == n.split()[0] or "nose" in d)]
    hits = 0
    for k in range(41):
        g = k / 40
        for d, n in pairs:
            if tree(d, g).overlap(tree(n, g)):
                hits += 1
                print(f"  door check: {d} meets {n} at gear {g:.3f}")
    print("door check:", "the doors clear the gear all the way up and down" if not hits else f"{hits} clashes")
# ---------------------------------------------------------------- export
# Every piece the game looks for (PIECES in src/aircraft/concorde/mod.rs); other objects are ignored.
EXPECTED = ["radome", "nose section", "windshield", "windshield frame", "fuselage", "entry door", "cabin windows", "tail cone", "tail light", "fin", "rudder", "stabiliser", "left elevator", "right elevator", "nose leg", "nose fork", "nose wheel"] + [
    f"{s} {n}" for s in ("left", "right") for n in ("wing", "inboard flap", "outboard flap", "aileron", "ground spoiler", "spoiler 1", "spoiler 2", "spoiler 3", "nav light", "landing light", "nacelle", "reverser", "main leg", "main oleo", "main wheel", "nose gear door", "aft nose gear door", "main gear door")]


def game_mat(m):
    """Which game material a Blender material is: its "game" property, its concorde_ name, or else the skin."""
    if m is None:
        return "skin"
    name = m.get("game") or (m.name[5:].split(".")[0] if m.name.startswith("g650_") else None)
    return name if name in MATS else "skin"


def export(path):
    out = bytearray(b"FFAC")
    obs = [o for o in COLL.all_objects if o.type == "MESH" and o.name in EXPECTED]
    names = {o.name for o in obs}
    missing = [n for n in EXPECTED if n not in names]
    if missing:
        raise SystemExit(f"g650 export: the game needs these pieces, missing from the '{COLL.name}' collection: {', '.join(missing)}")
    deps = bpy.context.evaluated_depsgraph_get()
    out += struct.pack("<II", 1, len(obs))
    total = 0
    for ob in obs:
        ob_eval = ob.evaluated_get(deps)
        me = ob_eval.to_mesh()  # with its modifiers applied
        mw = ob.matrix_world
        nm = mw.to_3x3().inverted().transposed()
        pivot = mw @ V(ob.get("pivot", (0, 0, 0)))
        axis = (mw.to_3x3() @ V(ob.get("axis", (1, 0, 0)))).normalized()
        me.calc_loop_triangles()
        verts, index, idx = [], [], {}
        volume = 0.0
        for tri in me.loop_triangles:
            mat = game_mat(me.materials[tri.material_index] if tri.material_index < len(me.materials) else None)
            gid, pat, col, extra = MATS[mat]
            a, b, c = (mw @ me.vertices[i].co for i in tri.vertices)
            volume += a.dot(b.cross(c)) / 6
            for k in range(3):
                vi = tri.vertices[k]
                n = (nm @ V(tri.split_normals[k])).normalized()
                key = (vi, round(n.x, 3), round(n.y, 3), round(n.z, 3), mat)
                if key not in idx:
                    p = mw @ me.vertices[vi].co
                    idx[key] = len(verts)
                    verts.append((*game(p), *game(n), *col, float(gid), float(pat), 0.3, float(extra)))
                index.append(idx[key])
        ob_eval.to_mesh_clear()
        name = ob.name.encode()
        out += struct.pack("<I", len(name)) + name
        out += struct.pack("<7f", *game(pivot), *game(axis), float(ob.get("angle", 0.0)))
        out += struct.pack("<I", len(verts))
        for v in verts:
            out += struct.pack("<13f", *v)
        out += struct.pack("<I", len(index))
        out += struct.pack("<%dI" % len(index), *index)
        total += len(index) // 3
        # (The signed volume: negative for a closed piece wound inside out.)
        print(f"  {ob.name}: {len(verts)} vertices, {len(index) // 3} triangles, volume {volume:.3f}")
    with gzip.open(path, "wb", compresslevel=9) as f:
        f.write(bytes(out))
    print("wrote", path, len(out), "bytes raw,", total, "triangles")


# ---------------------------------------------------------------- pictures
def camera():
    sc = bpy.context.scene
    cam_data = bpy.data.cameras.new("cam")
    cam = bpy.data.objects.new("cam", cam_data)
    sc.collection.objects.link(cam)
    sc.camera = cam
    cam_data.clip_end = 1000.0
    return cam, cam_data


def render_previews(outdir):
    os.makedirs(outdir, exist_ok=True)
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_WORKBENCH"
    sc.display.shading.light = "STUDIO"
    sc.display.shading.color_type = "MATERIAL"
    sc.display.shading.show_cavity = True
    sc.render.resolution_x, sc.render.resolution_y = 1280, 800
    sc.render.film_transparent = False
    cam, cam_data = camera()
    target = V((0, 0, 0.6))
    views = (("three_quarter", V((24, 31, 11)), 50, target), ("side", V((-52, 0.0, 0.8)), 50, V((0, -1.5, 0.6))), ("top", V((0.01, -1.0, 75)), 50, V((0, -1.0, 0))),
             ("front", V((0.01, 42, 0.6)), 85, V((0, 0, 0.4))), ("rear_low", V((-17, -27, -3)), 50, target), ("rear", V((9, -26, 5)), 60, V((0, -10, 1.0))),
             ("engines", V((8, -14.5, 3.0)), 60, V((2.2, -8.5, 1.2))), ("cockpit", V((-4.0, 18.5, 2.6)), 55, V((0, 11.9, 0.7))), ("under", V((13, 15, -8)), 50, V((0, 0, -1.0))),
             ("gear", V((8.5, 6.5, -1.4)), 50, V((1.0, -0.5, -1.6))), ("door", V((-9, 13, 0.4)), 45, V((-1.2, 9.3, -0.3))), ("wing", V((14, 2, 6)), 45, V((7, -4, -0.6))))

    def shoot(name, eye, lens, look):
        cam.location = eye
        cam.rotation_euler = (look - eye).to_track_quat("-Z", "Y").to_euler()
        cam_data.lens = lens
        sc.render.filepath = os.path.join(outdir, name + ".png")
        bpy.ops.render.render(write_still=True)

    obs = {o.name: o for o in COLL.all_objects}

    def pose_gear(g):
        # (children follow their parents by parenting only in the game: here, by hand)
        for o in obs.values():
            if o.name in EXPECTED and (o.get("parent") or "gear door" in o.name or o.name.endswith(" leg")):
                o.matrix_world = gear_world(obs, o.name, g)

    # The gear down as it is in flight and on the ground: the nose well's forward doors shut.
    pose_gear(1.0)
    for v in views:
        shoot(*v)
    # The gear on its way up (and down: the same poses).
    for g in (0.92, 0.75, 0.5, 0.25, 0.08):
        pose_gear(g)
        shoot(f"gear_cycle_{round(g * 100):02d}", V((8.5, 6.5, -1.4)), 50, V((1.0, -0.5, -1.6)))
        shoot(f"nose_cycle_{round(g * 100):02d}", V((-5.5, 13.5, -1.8)), 45, V((0, 11.8, -1.6)))
    pose_gear(1.0)
    shoot("nose_down", V((-5.5, 13.5, -1.8)), 45, V((0, 11.8, -1.6)))
    # Configured to land: flaps at 39, spoilers up, reversers out, the door open.

    def turn(o, f):
        p = V(o["pivot"])
        if "reverser" in o.name:
            o.matrix_world = Matrix.Translation(V(o["axis"]) * o["angle"] * f)
        else:
            o.matrix_world = Matrix.Translation(p) @ Matrix.Rotation(o["angle"] * f, 4, V(o["axis"])) @ Matrix.Translation(-p)

    moved = [o for o in obs.values() if any(k in o.name for k in ("flap", "spoiler", "reverser", "entry door"))]
    for o in moved:
        turn(o, 1.0)
    shoot("landing", V((24, -2, 10)), 45, V((6, -3, -0.3)))
    shoot("door_open", V((-9, 13, 0.4)), 45, V((-1.2, 9.3, -0.8)))
    shoot("reverser", V((7, -16, 2.5)), 55, V((2.2, -8.8, 1.2)))
    for o in moved:
        o.matrix_world = Matrix.Identity(4)
    pose_gear(0.0)
    shoot("gear_up_under", V((13, 15, -8)), 50, V((0, 0, -1.0)))
    shoot("gear_up_side", V((-52, 0.0, -1.0)), 50, V((0, -1.5, 0.6)))
    for o in obs.values():
        o.matrix_world = Matrix.Identity(4)


def render_still(path):
    """An unpainted clay still, 680 x 392, on a clear background (the start
    screen's painted one is g650_still.py's)."""
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_WORKBENCH"
    sc.display.shading.light = "STUDIO"
    sc.display.shading.color_type = "MATERIAL"
    sc.display.shading.show_cavity = True
    sc.render.film_transparent = True
    sc.render.resolution_x, sc.render.resolution_y = 680, 392
    sc.render.image_settings.file_format = "PNG"
    sc.render.image_settings.color_mode = "RGBA"
    cam, cam_data = camera()
    eye, look = V((30, 26, 7.0)), V((-0.5, -2.5, 0.6))
    cam.location = eye
    cam.rotation_euler = (look - eye).to_track_quat("-Z", "Y").to_euler()
    cam_data.lens = 38
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)


# The paint is projected straight down, straight up and from each side: these
# are the rectangles (m) those pictures cover. src/aircraft/g650/mod.rs (its
# `Paint`) must be given the same numbers.
GUIDE_SPAN = 31.4   # across the top and bottom views (the 30.36 m span); up the side views
GUIDE_LEN = 31.8    # fore and aft in all of them (the 30.41 m length)
GUIDE_Y = Y(15.2)   # station at the middle of the pictures
GUIDE_Z = Z(3.9)    # height at the middle of the side views
GUIDE_PX = 1800     # pixels fore and aft: 17.7 mm each
GUIDE_EYE = 120.0   # how far off the cameras stand


def render_guides(outdir):
    os.makedirs(outdir, exist_ok=True)
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_WORKBENCH"
    sc.render.film_transparent = False
    sc.view_settings.view_transform = "Standard"
    world = sc.world or bpy.data.worlds.new("guide")
    sc.world = world
    sh = sc.display.shading
    # Gear up: only what the paint goes on.
    hidden = [o for o in COLL.all_objects if o.name not in EXPECTED or any(k in o.name for k in ("leg", "wheel", "fork", "oleo", "gear door", "light"))]
    for o in hidden:
        o.hide_render = True
    cam, cam_data = camera()
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = GUIDE_LEN
    short = round(GUIDE_PX * GUIDE_SPAN / GUIDE_LEN)
    # (The left side is seen nose to the left, the right side nose to the right.)
    views = (("top", V((0, GUIDE_Y, GUIDE_EYE)), V((0, 0, -1)), V((0, 1, 0)), (short, GUIDE_PX)),
             ("bottom", V((0, GUIDE_Y, -GUIDE_EYE)), V((0, 0, 1)), V((0, 1, 0)), (short, GUIDE_PX)),
             ("side", V((-GUIDE_EYE, GUIDE_Y, GUIDE_Z)), V((1, 0, 0)), V((0, 0, 1)), (GUIDE_PX, short)),
             ("right", V((GUIDE_EYE, GUIDE_Y, GUIDE_Z)), V((-1, 0, 0)), V((0, 0, 1)), (GUIDE_PX, short)))
    for name, eye, look, up, (w, h) in views:
        cam.location = eye
        right = look.cross(up)
        cam.rotation_euler = Matrix((right, up, -look)).transposed().to_euler()
        sc.render.resolution_x, sc.render.resolution_y = w, h
        for kind in ("guide", "mask"):
            if kind == "guide":
                sh.light, sh.color_type, sh.single_color = "STUDIO", "SINGLE", (0.62, 0.64, 0.66)
                sh.show_cavity, sh.cavity_type, sh.show_object_outline = True, "BOTH", True
                world.color = (1, 1, 1)
            else:
                sh.light, sh.color_type, sh.single_color = "FLAT", "SINGLE", (1, 1, 1)
                sh.show_cavity, sh.show_object_outline = False, False
                world.color = (0, 0, 0)
            sc.render.filepath = os.path.join(outdir, f"{name}_{kind}.png")
            bpy.ops.render.render(write_still=True)
    geometry_pass(outdir, views)
    for o in hidden:
        o.hide_render = o.name not in EXPECTED


def geometry_pass(outdir, views):
    """What each pixel of a guide is a picture of, for a paint script that
    paints by where on the aircraft a pixel is (as assets/blender/f4_paint.py):
    <view>_geo.npz with, for each pixel, the point of the model seen there
    (`depth`: its height seen from above or below, its x seen from a side, NaN
    off the model; the picture's own grid gives the rest), the surface's normal
    (`nrm`), and which piece and material it is (`piece`, `mat`: indices into
    `pieces` and `mats`). Rays are cast through the pixel centres of the renders above."""
    import numpy as np
    from mathutils.bvhtree import BVHTree
    deps = bpy.context.evaluated_depsgraph_get()
    obs = sorted((o for o in COLL.all_objects if o.type == "MESH" and not o.hide_render and o.name in EXPECTED), key=lambda o: o.name)
    mats = sorted(MATS)
    verts, polys, owner = [], [], []
    for k, ob in enumerate(obs):
        me = ob.evaluated_get(deps).to_mesh()
        base = len(verts)
        verts += [ob.matrix_world @ v.co for v in me.vertices]
        for p in me.polygons:
            polys.append([base + i for i in p.vertices])
            owner.append((k, mats.index(game_mat(me.materials[p.material_index] if p.material_index < len(me.materials) else None))))
        ob.evaluated_get(deps).to_mesh_clear()
    tree = BVHTree.FromPolygons(verts, polys)
    for name, eye, look, up, (w, h) in views:
        right = look.cross(up)
        step = GUIDE_LEN / GUIDE_PX
        pos = np.full((h, w, 3), np.nan, np.float32)
        nrm = np.zeros((h, w, 3), np.int8)
        piece = np.full((h, w), 255, np.uint8)
        mat = np.full((h, w), 255, np.uint8)
        for j in range(h):
            row = eye + up * ((h / 2 - j - 0.5) * step)
            for i in range(w):
                hit, n, f, _ = tree.ray_cast(row + right * ((i + 0.5 - w / 2) * step), look)
                if hit is not None:
                    if n.dot(look) > 0:
                        n = -n
                    pos[j, i] = hit
                    nrm[j, i] = (round(n.x * 127), round(n.y * 127), round(n.z * 127))
                    piece[j, i], mat[j, i] = owner[f]
        np.savez_compressed(os.path.join(outdir, name + "_geo.npz"), depth=pos[..., 2 if abs(look.z) > 0.5 else 0].astype(np.float16), nrm=nrm, piece=piece, mat=mat, pieces=np.array([o.name for o in obs]), mats=np.array(mats))
        print(f"  {name}: geometry pass, {int((piece < 255).sum())} pixels on the model")


def main():
    global COLL, CUTS
    if EXPORT_ONLY:
        # Export the .blend that is open, edits and all, without rebuilding anything.
        COLL = bpy.data.collections.get("G650")
        if COLL is None:
            raise SystemExit("g650 export: no 'G650' collection in this file (open assets/blender/g650.blend)")
        export(os.path.join(ASSETS, "g650.bin.gz"))
        check_gear()
    else:
        for ob in list(bpy.data.objects):
            bpy.data.objects.remove(ob, do_unlink=True)
        for c in list(bpy.data.collections):
            bpy.data.collections.remove(c)
        COLL = bpy.data.collections.new("G650")
        bpy.context.scene.collection.children.link(COLL)
        CUTS = bpy.data.collections.new("G650 cutters")
        bpy.context.scene.collection.children.link(CUTS)
        CUTS.hide_render = True
        make_materials()
        fuselage()
        wings()
        nacelles()
        tail()
        gear()
        doors()
        for o in list(COLL.objects):
            if o.name not in EXPECTED:
                bpy.data.objects.remove(o, do_unlink=True)
        os.makedirs(os.path.join(ASSETS, "blender"), exist_ok=True)
        bpy.context.preferences.filepaths.save_version = 0  # no g650.blend1 beside it
        export(os.path.join(ASSETS, "g650.bin.gz"))
        check_gear()
        bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ASSETS, "blender", "g650.blend"))
    if RENDER:
        render_previews(RENDER)
    if STILL:
        render_still(STILL)
    if GUIDES:
        render_guides(GUIDES)


main()
