# Birds for FableFlight, built procedurally in Blender: five species, each a
# body and four wing segments that flap about their shoulder and wrist hinges,
# plus a one-piece model for far away.
#
#   blender -b --factory-startup --python assets/blender/birds.py -- <repo>/assets [--render <dir>]
#
# Saves assets/blender/birds.blend and exports assets/birds.bin.gz (the same
# file format as the aircraft, see starship.py; read by src/birds.rs). The
# rebuild starts from scratch, so it throws away anything edited by hand in
# the .blend. To keep hand edits, export the .blend as it stands instead:
#
#   blender -b assets/blender/birds.blend --python assets/blender/birds.py -- <repo>/assets --export-only
#
# Pieces, per species <sp> (gull, crow, starling, buzzard, goose): "<sp> body",
# "<sp> left inner", "<sp> left outer", "<sp> right inner", "<sp> right
# outer" (each wing segment records its hinge: custom properties "pivot" and
# "axis"), and "<sp> far" (all of it in one piece, wings spread). The colours
# are a "Col" colour attribute on the face corners (linear albedo).
#
# Sizes (BTO BirdFacts, https://www.bto.org/understanding-birds/birdfacts:
# typical length, wingspan, mass):
#   herring gull      Larus argentatus   60 cm, 144 cm, 1.0 kg
#   carrion crow      Corvus corone      47 cm,  98 cm, 0.5 kg
#   common starling   Sturnus vulgaris   21 cm,  40 cm, 0.08 kg
#   common buzzard    Buteo buteo        54 cm, 120 cm, 0.8 kg
#   greylag goose     Anser anser        82 cm, 164 cm, 3.3 kg
# Shapes and colours from field-guide plates and photographs of birds in
# flight: the gull's long pointed grey wings with black tips; the crow's and
# the buzzard's fingered primaries (the buzzard's broad, pale underneath with
# dark wrist patches); the starling's short triangular wings; the goose's long
# neck and long pointed wings. The models are simplified: no feather detail,
# no legs (tucked in flight).
#
# --render writes preview pictures of each species from above and below.
#
# Blender frame: +X right, +Y forward (the beak), +Z up, the origin at the
# shoulders' mid point, metres. The game's frame is +X right, +Y up, -Z
# forward, so a point (x, y, z) here is (x, z, -y) there.
import bpy, bmesh, math, struct, gzip, sys, os
from mathutils import Vector as V

argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
ASSETS = os.path.abspath(argv[0]) if argv else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
RENDER = argv[argv.index("--render") + 1] if "--render" in argv else None
EXPORT_ONLY = "--export-only" in argv

# Colours (linear albedo).
WHITE = (0.80, 0.80, 0.78)
GULL_GREY = (0.30, 0.33, 0.37)
INK = (0.020, 0.020, 0.022)
CROW = (0.022, 0.022, 0.026)
STARLING = (0.030, 0.030, 0.034)
BUZZ_BROWN = (0.10, 0.060, 0.030)
BUZZ_PALE = (0.42, 0.34, 0.24)
GOOSE_BACK = (0.17, 0.15, 0.12)
GOOSE_GREY = (0.28, 0.27, 0.25)
GOOSE_BELLY = (0.45, 0.43, 0.40)
YELLOW = (0.70, 0.48, 0.05)
ORANGE = (0.70, 0.26, 0.05)
DARK_BILL = (0.04, 0.04, 0.04)

# Per species: length, wingspan (m); body half width and height (m); head
# radius; beak length; tail length and half width; neck: how much of the body
# length is a thin neck (goose); wing: wrist at this share of the half span,
# chords at root, wrist and tip, how far back the tip is swept (m), fingers
# (slotted primaries) and how long; dihedral at rest (deg).
SPECIES = {
    "gull": dict(length=0.60, span=1.44, bw=0.060, bh=0.058, head=0.034, beak=0.050, tail=0.13, tail_w=0.065, neck=0.0,
                 wrist=0.42, c_root=0.21, c_wrist=0.20, c_tip=0.035, sweep=0.20, fingers=0, finger_len=0.0, dihedral=2.0),
    "crow": dict(length=0.47, span=0.98, bw=0.052, bh=0.050, head=0.032, beak=0.050, tail=0.15, tail_w=0.055, neck=0.0,
                 wrist=0.45, c_root=0.20, c_wrist=0.21, c_tip=0.13, sweep=0.05, fingers=5, finger_len=0.08, dihedral=3.0),
    "starling": dict(length=0.21, span=0.40, bw=0.026, bh=0.025, head=0.015, beak=0.024, tail=0.055, tail_w=0.025, neck=0.0,
                     wrist=0.42, c_root=0.085, c_wrist=0.080, c_tip=0.018, sweep=0.06, fingers=0, finger_len=0.0, dihedral=2.0),
    "buzzard": dict(length=0.54, span=1.20, bw=0.062, bh=0.058, head=0.036, beak=0.035, tail=0.20, tail_w=0.085, neck=0.0,
                    wrist=0.45, c_root=0.30, c_wrist=0.31, c_tip=0.20, sweep=0.04, fingers=5, finger_len=0.11, dihedral=8.0),
    "goose": dict(length=0.82, span=1.64, bw=0.085, bh=0.080, head=0.040, beak=0.060, tail=0.13, tail_w=0.075, neck=0.38,
                  wrist=0.44, c_root=0.30, c_wrist=0.26, c_tip=0.06, sweep=0.18, fingers=0, finger_len=0.0, dihedral=2.0),
}

PIECES = ["body", "left inner", "left outer", "right inner", "right outer", "far"]
EXPECTED = [f"{sp} {p}" for sp in SPECIES for p in PIECES]


def colour(sp, part, p, n, s):
    """Albedo of a face corner: species, part ("body", "beak", "tail", "wing"),
    position, face normal (Blender frame), and for the wings the share of the
    half span from the shoulder (0) to the tip (1)."""
    up = n.z > 0.0
    if sp == "gull":
        if part == "beak":
            return YELLOW
        if part == "wing":
            if s > 0.80:
                return INK  # black tips
            return GULL_GREY if up else WHITE
        if part == "body" and up and abs(p.x) < 0.07 and -0.10 < p.y < 0.06:
            return GULL_GREY  # the mantle between the wings
        return WHITE
    if sp == "crow":
        return DARK_BILL if part == "beak" else CROW
    if sp == "starling":
        return YELLOW if part == "beak" else STARLING
    if sp == "buzzard":
        if part == "beak":
            return DARK_BILL
        if part == "wing":
            if up:
                return BUZZ_BROWN
            # Underneath: pale, with dark wrist patches, dark fingertips and a dark trailing band.
            if s > 0.85:
                return INK
            if 0.38 < s < 0.52:
                return (0.06, 0.04, 0.025)
            return BUZZ_PALE
        if part == "tail":
            return BUZZ_BROWN if up else BUZZ_PALE
        return BUZZ_BROWN if (up or p.y > 0.05) else BUZZ_PALE
    if sp == "goose":
        if part == "beak":
            return ORANGE
        if part == "wing":
            if s > 0.75:
                return (0.10, 0.09, 0.08)
            return GOOSE_BACK if up else GOOSE_GREY
        if part == "tail":
            return GOOSE_BACK if up else WHITE
        return GOOSE_BACK if up and p.y < 0.10 else (GOOSE_GREY if p.y >= 0.10 else GOOSE_BELLY)
    return WHITE


def loft(bm, rings, cap_start=True, cap_end=True):
    """Faces between rings of vertices (each a list of BMVerts, same count),
    closed round, with fans over the ends."""
    n = len(rings[0])
    for a, b in zip(rings, rings[1:]):
        for i in range(n):
            j = (i + 1) % n
            bm.faces.new((a[i], a[j], b[j], b[i]))
    if cap_start:
        c = bm.verts.new(sum((v.co for v in rings[0]), V()) / n)
        for i in range(n):
            bm.faces.new((c, rings[0][(i + 1) % n], rings[0][i]))
    if cap_end:
        c = bm.verts.new(sum((v.co for v in rings[-1]), V()) / n)
        for i in range(n):
            bm.faces.new((c, rings[-1][i], rings[-1][(i + 1) % n]))


def ellipse_ring(bm, y, z, rx, rz, n=10):
    return [bm.verts.new((rx * math.cos(2 * math.pi * k / n), y, z + rz * math.sin(2 * math.pi * k / n))) for k in range(n)]


def shoulder(d):
    """Where the wings join the body (Blender frame): the origin is the
    shoulders' mid point, so it is (half the body width, 0, a little up)."""
    return V((d["bw"] * 0.7, 0.0, d["bh"] * 0.35))


def body_mesh(sp, d):
    """Body, head and beak as one lofted solid, and the tail fan; returns
    (bmesh, part of each face)."""
    bm = bmesh.new()
    parts = []
    L, bw, bh, hr = d["length"], d["bw"], d["bh"], d["head"]
    # Lengthwise: tail tip at the back, beak tip at the front.
    body_len = L - d["tail"] - d["beak"]
    y_back = -body_len * 0.55
    y_front = body_len * 0.45
    def at(t):
        return y_back + (y_front - y_back) * t
    if d["neck"] > 0.0:
        # Goose: the body takes the back part, the neck carries the head forward.
        nk = d["neck"]
        prof = [(0.00, 0.30, 0.25, 0.0), (0.08, 0.75, 0.70, 0.0), (0.22, 1.00, 1.00, 0.0), (0.40, 0.95, 0.95, 0.05),
                (1.0 - nk, 0.55, 0.60, 0.20), (1.0 - nk + 0.08, 0.30, 0.32, 0.45), (0.80, 0.27, 0.29, 0.75),
                (0.88, hr / bw * 0.95, hr / bh * 0.95, 0.95), (0.95, hr / bw, hr / bh * 0.9, 1.0), (1.00, hr / bw * 0.45, hr / bh * 0.45, 1.0)]
    else:
        prof = [(0.00, 0.28, 0.22, 0.05), (0.15, 0.75, 0.70, 0.0), (0.35, 1.00, 1.00, 0.0), (0.55, 0.95, 0.95, 0.05),
                (0.70, 0.70, 0.72, 0.15), (0.78, hr / bw * 0.80, hr / bh * 0.82, 0.30), (0.88, hr / bw, hr / bh, 0.38),
                (1.00, hr / bw * 0.45, hr / bh * 0.45, 0.38)]
    rings = []
    for t, sx, sz, rise in prof:
        rings.append(ellipse_ring(bm, at(t), rise * bh, bw * sx, bh * sz))
    nf = len(bm.faces)
    loft(bm, rings, cap_start=True, cap_end=False)
    parts += ["body"] * (len(bm.faces) - nf)
    # Beak: a cone from the front ring.
    tip = bm.verts.new((0.0, y_front + d["beak"], prof[-1][3] * bh - 0.15 * hr))
    front = rings[-1]
    nf = len(bm.faces)
    for i in range(len(front)):
        bm.faces.new((front[i], front[(i + 1) % len(front)], tip))
    parts += ["beak"] * (len(bm.faces) - nf)
    # Tail: a thin fan behind the body, spread wider at its end.
    y0, y1 = y_back + 0.02 * L, y_back - d["tail"]
    th = 0.12 * bh
    tw0, tw1 = bw * 0.45, d["tail_w"]
    pts = [(-tw0, y0), (tw0, y0), (tw1, y1 + 0.02 * L), (0.0, y1), (-tw1, y1 + 0.02 * L)]
    top = [bm.verts.new((x, y, th * 0.5)) for x, y in pts]
    bot = [bm.verts.new((x, y, -th * 0.5)) for x, y in pts]
    nf = len(bm.faces)
    bm.faces.new(top)
    bm.faces.new(list(reversed(bot)))
    for i in range(len(pts)):
        j = (i + 1) % len(pts)
        bm.faces.new((top[j], top[i], bot[i], bot[j]))
    parts += ["tail"] * (len(bm.faces) - nf)
    return bm, parts


def wing_stations(d):
    """The left wing's planform (Blender frame, x negative): stations from
    the shoulder out, each (x, leading edge y, chord, thickness, share of the
    half span)."""
    sh = shoulder(d)
    # (The fingers reach on past the end of the hand.)
    half = d["span"] / 2.0 - d["finger_len"] * 0.97
    reach = half - sh.x
    out = []
    n_in, n_out = 4, 5
    for k in range(n_in + n_out + 1):
        s = k / (n_in + n_out) if k <= n_in + n_out else 1.0
        # More stations inboard of the wrist, then out to the tip.
        if k <= n_in:
            s = d["wrist"] * k / n_in
        else:
            s = d["wrist"] + (1.0 - d["wrist"]) * (k - n_in) / n_out
        if s <= d["wrist"]:
            u = s / d["wrist"]
            chord = d["c_root"] + (d["c_wrist"] - d["c_root"]) * u
            le = 0.25 * d["c_root"] + 0.015 * u * d["span"]
        else:
            u = (s - d["wrist"]) / (1.0 - d["wrist"])
            # The hand: the chord narrows (pointed wings) or keeps up (fingered
            # ones), and the leading edge sweeps back.
            chord = d["c_wrist"] + (d["c_tip"] - d["c_wrist"]) * (u ** 1.3)
            le = 0.25 * d["c_root"] + 0.015 * d["span"] - d["sweep"] * u ** 1.5
        thick = max(0.10 * chord * (1.0 - 0.75 * s), 0.004)
        out.append((-(sh.x + reach * s), le, chord, thick, s))
    return out


def section(bm, x, le, chord, thick, z):
    """An airfoil section: leading edge, upper, trailing edge, lower."""
    return [bm.verts.new((x, le, z)),
            bm.verts.new((x, le - 0.30 * chord, z + 0.65 * thick)),
            bm.verts.new((x, le - chord, z)),
            bm.verts.new((x, le - 0.40 * chord, z - 0.35 * thick))]


def wing_mesh(d, outer):
    """The left wing's inner (shoulder to wrist) or outer (wrist to tip)
    segment, with fingers on the outer one; returns (bmesh, pivot, share of
    the half span at each vertex)."""
    bm = bmesh.new()
    st = wing_stations(d)
    sh = shoulder(d)
    dih = math.radians(d["dihedral"])
    def z_at(x):
        return sh.z + (abs(x) - sh.x) * math.tan(dih)
    k_w = 4
    sel = st[k_w:] if outer else st[:k_w + 1]
    rings = [section(bm, x, le, c, t, z_at(x)) for x, le, c, t, s in sel]
    loft(bm, rings)
    if outer and d["fingers"] > 0:
        # Slotted primaries: separate tapering feathers off the tip.
        x, le, c, t, s = st[-1]
        nf = d["fingers"]
        for i in range(nf):
            f = (i + 0.5) / nf
            y0 = le - c * (0.08 + 0.84 * f)
            w = c / nf * 0.55
            ln = d["finger_len"] * (1.0 - 0.45 * abs(f - 0.35))
            a = math.radians(-12 + 30 * f)  # they fan out backwards
            dx, dy = -math.cos(a) * ln, -math.sin(a) * ln
            r0 = [bm.verts.new((x + 0.002, y0 + w * 0.5, z_at(x))), bm.verts.new((x + 0.002, y0, z_at(x) + 0.004)),
                  bm.verts.new((x + 0.002, y0 - w * 0.5, z_at(x))), bm.verts.new((x + 0.002, y0, z_at(x) - 0.002))]
            tip = [bm.verts.new((x + dx, y0 + dy + w * 0.2, z_at(x - dx) - 0.01 * f)), bm.verts.new((x + dx, y0 + dy, z_at(x - dx) + 0.002)),
                   bm.verts.new((x + dx, y0 + dy - w * 0.2, z_at(x - dx) - 0.01 * f)), bm.verts.new((x + dx, y0 + dy, z_at(x - dx) - 0.001))]
            loft(bm, [r0, tip])
    pivot = V((st[k_w][0], st[k_w][1] - 0.3 * st[k_w][2], z_at(st[k_w][0]))) if outer else V((-sh.x, sh.y, sh.z))
    return bm, pivot


def make_object(name, bm, coll, colour_of):
    """A mesh object from a bmesh, its face corners coloured by colour_of(p, n)."""
    me = bpy.data.meshes.new(name)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(me)
    bm.free()
    attr = me.color_attributes.new(name="Col", type="FLOAT_COLOR", domain="CORNER")
    for poly in me.polygons:
        n = poly.normal
        c = poly.center
        for li in poly.loop_indices:
            p = me.vertices[me.loops[li].vertex_index].co
            r, g, b = colour_of(p, n, c, poly.index)
            attr.data[li].color = (r, g, b, 1.0)
    me.color_attributes.active_color = attr
    me.color_attributes.render_color_index = me.color_attributes.active_color_index
    me.set_sharp_from_angle(angle=math.radians(35))
    ob = bpy.data.objects.new(name, me)
    coll.objects.link(ob)
    return ob


def mirror_bm(bm):
    """The right side of a left-side piece."""
    for v in bm.verts:
        v.co.x = -v.co.x
    bmesh.ops.reverse_faces(bm, faces=bm.faces)
    return bm


def build():
    for ob in list(bpy.data.objects):
        bpy.data.objects.remove(ob)
    root = bpy.data.collections.new("Birds")
    bpy.context.scene.collection.children.link(root)
    for i, (sp, d) in enumerate(SPECIES.items()):
        coll = bpy.data.collections.new(sp)
        root.children.link(coll)
        half = d["span"] / 2.0
        reach = half - shoulder(d).x
        def span_share(p):
            return max(0.0, (abs(p.x) - shoulder(d).x) / reach)
        bm, parts = body_mesh(sp, d)
        body = make_object(f"{sp} body", bm, coll, lambda p, n, c, fi, parts=parts, sp=sp: colour(sp, parts[fi], c, n, 0.0))
        pieces = [body]
        for side in ("left", "right"):
            for seg in ("inner", "outer"):
                bm, pivot = wing_mesh(d, seg == "outer")
                if side == "right":
                    mirror_bm(bm)
                    pivot = V((-pivot.x, pivot.y, pivot.z))
                ob = make_object(f"{sp} {side} {seg}", bm, coll, lambda p, n, c, fi, sp=sp: colour(sp, "wing", c, n, span_share(c)))
                ob["pivot"] = tuple(pivot)
                ob["axis"] = (0.0, 1.0, 0.0)
                pieces.append(ob)
        # The far model: everything in one piece.
        bm = bmesh.new()
        dg = bpy.context.evaluated_depsgraph_get()
        far_me = bpy.data.meshes.new(f"{sp} far")
        for ob in pieces:
            bm.from_mesh(ob.data)
        bm.to_mesh(far_me)
        bm.free()
        # (The corner colours came along with the meshes.)
        far = bpy.data.objects.new(f"{sp} far", far_me)
        coll.objects.link(far)
        far_me.set_sharp_from_angle(angle=math.radians(35))
        print(f"{sp}: span {max(v.co.x for v in far_me.vertices) - min(v.co.x for v in far_me.vertices):.3f} m, "
              f"length {max(v.co.y for v in far_me.vertices) - min(v.co.y for v in far_me.vertices):.3f} m, {len(far_me.polygons)} faces")
    bpy.ops.wm.save_as_mainfile(filepath=os.path.join(ASSETS, "blender", "birds.blend"))


def game(v):
    return (v[0], v[2], -v[1])


def export(path):
    out = bytearray(b"FFAC")
    obs = [o for o in bpy.data.objects if o.type == "MESH"]
    names = {o.name for o in obs}
    missing = [n for n in EXPECTED if n not in names]
    if missing:
        raise SystemExit(f"birds export: the game needs these pieces: {', '.join(missing)}")
    obs = [o for o in obs if o.name in EXPECTED]
    out += struct.pack("<II", 1, len(obs))
    total = 0
    for ob in obs:
        me = ob.data
        me.calc_loop_triangles()
        col = me.color_attributes.get("Col")
        mw = ob.matrix_world
        nm = mw.to_3x3().inverted().transposed()
        verts, index, idx = [], [], {}
        for tri in me.loop_triangles:
            for k in range(3):
                vi, li = tri.vertices[k], tri.loops[k]
                n = (nm @ V(tri.split_normals[k])).normalized()
                c = tuple(round(x, 4) for x in col.data[li].color[:3]) if col else (0.5, 0.5, 0.5)
                key = (vi, round(n.x, 3), round(n.y, 3), round(n.z, 3), c)
                if key not in idx:
                    idx[key] = len(verts)
                    verts.append((*game(mw @ me.vertices[vi].co), *game(n), *c, 0.0, 0.0, 0.0, 0.0))
                index.append(idx[key])
        pivot = mw @ V(ob.get("pivot", (0.0, 0.0, 0.0)))
        axis = (mw.to_3x3() @ V(ob.get("axis", (0.0, 1.0, 0.0)))).normalized()
        name = ob.name.encode()
        out += struct.pack("<I", len(name)) + name
        out += struct.pack("<7f", *game(pivot), *game(axis), 0.0)
        out += struct.pack("<I", len(verts))
        for v in verts:
            out += struct.pack("<13f", *v)
        out += struct.pack("<I", len(index))
        out += struct.pack("<%dI" % len(index), *index)
        total += len(index) // 3
    with gzip.open(path, "wb", compresslevel=9) as f:
        f.write(bytes(out))
    print("wrote", path, len(out), "bytes raw,", total, "triangles")


def render(outdir):
    """Each species from above and from below, on a plain background."""
    os.makedirs(outdir, exist_ok=True)
    sc = bpy.context.scene
    engines = [e.identifier for e in bpy.types.RenderSettings.bl_rna.properties["engine"].enum_items]
    sc.render.engine = "BLENDER_WORKBENCH" if "BLENDER_WORKBENCH" in engines else engines[0]
    sh = sc.display.shading
    kinds = [i.identifier for i in type(sh).bl_rna.properties["color_type"].enum_items]
    sh.color_type = "VERTEX" if "VERTEX" in kinds else ("ATTRIBUTE" if "ATTRIBUTE" in kinds else kinds[0])
    sh.light = "STUDIO"
    sh.background_type = "VIEWPORT"
    sh.background_color = (0.55, 0.65, 0.80)
    sc.render.resolution_x, sc.render.resolution_y = 900, 600
    sc.world = sc.world or bpy.data.worlds.new("w")
    cam_data = bpy.data.cameras.new("cam")
    cam = bpy.data.objects.new("cam", cam_data)
    sc.collection.objects.link(cam)
    sc.camera = cam
    for sp, d in SPECIES.items():
        for ob in bpy.data.objects:
            if ob.type == "MESH":
                ob.hide_render = not (ob.name.startswith(sp + " ") and not ob.name.endswith(" far"))
        dist = d["span"] * 1.6
        for view, loc in (("above", (0.0, -dist * 0.55, dist)), ("below", (0.0, -dist * 0.4, -dist)), ("side", (dist * 1.1, -dist * 0.3, dist * 0.25))):
            cam.location = loc
            cam.rotation_euler = (V((0, 0, 0)) - V(loc)).to_track_quat("-Z", "Y").to_euler()
            sc.render.filepath = os.path.join(outdir, f"{sp}_{view}.png")
            bpy.ops.render.render(write_still=True)


if not EXPORT_ONLY:
    build()
export(os.path.join(ASSETS, "birds.bin.gz"))
if RENDER:
    render(RENDER)
