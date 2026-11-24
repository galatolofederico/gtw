"""Shared helpers for the EBAW scenery scripts (01_ground_layout.py, 02_terminal_atc.py, ...).

Scene plumbing (collections, meshes with planar UVs), a small shader-graph
builder for the procedural PBR materials, and the check render (sky, sun,
Cycles) every module ends with.
"""

import math
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Vector
from mathutils.geometry import tessellate_polygon

# Materials by key, filled by each module before it builds meshes.
MATS = {}
MODULE = 1.5          # window module width
FONT = "/usr/share/fonts/liberation/LiberationSans-Bold.ttf"


def args(**defaults):
    a = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    opt = {"render": "/tmp/ebaw_step1.png", "shot": "overview", "samples": 48, "res": "1920x1080", "save": None}
    opt.update(defaults)
    i = 0
    while i < len(a):
        k = a[i].lstrip("-")
        if k in opt and i + 1 < len(a):
            opt[k] = a[i + 1]
            i += 2
        else:
            i += 1
    return opt


def collection(name, parent=None):
    c = bpy.data.collections.get(name)
    if c is None:
        c = bpy.data.collections.new(name)
        (parent or bpy.context.scene.collection).children.link(c)
    for ob in list(c.objects):  # re-running the script rebuilds the module
        bpy.data.objects.remove(ob, do_unlink=True)
    return c


def mesh_object(name, coll, verts, faces, mat):
    me = bpy.data.meshes.new(name)
    me.from_pydata(verts, [], faces)
    me.validate()
    # Planar UVs, 10 m per tile, in the frame, ready for baking at export (step 5).
    uv = me.uv_layers.new(name="UVMap")
    for loop in me.loops:
        co = me.vertices[loop.vertex_index].co
        uv.data[loop.index].uv = (co.x / 10.0, co.y / 10.0)
    for p in me.polygons:
        p.use_smooth = False
    if mat:
        me.materials.append(mat)
    ob = bpy.data.objects.new(name, me)
    coll.objects.link(ob)
    return ob


class Graph:
    """A tiny helper to wire shader nodes."""

    def __init__(self, name):
        self.mat = bpy.data.materials.get(name) or bpy.data.materials.new(name)
        self.mat.use_nodes = True
        self.nt = self.mat.node_tree
        self.nt.nodes.clear()
        self.x = 0
        self.out = self.node("ShaderNodeOutputMaterial")
        self.bsdf = self.node("ShaderNodeBsdfPrincipled")
        self.link(self.bsdf.outputs["BSDF"], self.out.inputs["Surface"])
        tc = self.node("ShaderNodeTexCoord")
        self.pos = tc.outputs["Object"]

    def node(self, kind, **props):
        n = self.nt.nodes.new(kind)
        n.location = (self.x, 0)
        self.x -= 40
        for k, v in props.items():
            setattr(n, k, v)
        return n

    def link(self, a, b):
        self.nt.links.new(a, b)

    def put(self, sock, v):
        if hasattr(v, "is_output"):
            self.link(v, sock)
        else:
            if isinstance(v, tuple) and len(v) == 3 and len(sock.default_value) == 4:
                v = (*v, 1.0)  # RGB -> RGBA
            sock.default_value = v

    def math(self, op, a, b=0.0, clamp=False):
        n = self.node("ShaderNodeMath", operation=op, use_clamp=clamp)
        self.put(n.inputs[0], a)
        self.put(n.inputs[1], b)
        return n.outputs[0]

    def smooth(self, x, lo, hi, out_lo=0.0, out_hi=1.0):
        n = self.node("ShaderNodeMapRange", interpolation_type="SMOOTHSTEP", clamp=True)
        self.put(n.inputs["Value"], x)
        n.inputs["From Min"].default_value = lo
        n.inputs["From Max"].default_value = hi
        n.inputs["To Min"].default_value = out_lo
        n.inputs["To Max"].default_value = out_hi
        return n.outputs["Result"]

    def frame(self, u_axis, v_axis):
        """Two coordinates (dot products) of the object position along u_axis and v_axis."""
        def dot(vec):
            n = self.node("ShaderNodeVectorMath", operation="DOT_PRODUCT")
            self.put(n.inputs[0], self.pos)
            n.inputs[1].default_value = vec
            return n.outputs["Value"]
        return dot(tuple(u_axis)), dot(tuple(v_axis))

    def vec(self, x, y, z=0.0):
        n = self.node("ShaderNodeCombineXYZ")
        self.put(n.inputs[0], x)
        self.put(n.inputs[1], y)
        self.put(n.inputs[2], z)
        return n.outputs[0]

    def noise(self, vec, scale, detail=4.0, rough=0.55, out="Fac", dims="3D"):
        n = self.node("ShaderNodeTexNoise", noise_dimensions=dims)
        self.put(n.inputs["Vector"], vec)
        n.inputs["Scale"].default_value = scale
        n.inputs["Detail"].default_value = detail
        n.inputs["Roughness"].default_value = rough
        return n.outputs[out]

    def white(self, vec):
        n = self.node("ShaderNodeTexWhiteNoise", noise_dimensions="3D")
        self.put(n.inputs["Vector"], vec)
        return n.outputs["Value"]

    def voronoi_edge(self, vec, scale):
        n = self.node("ShaderNodeTexVoronoi", feature="DISTANCE_TO_EDGE")
        self.put(n.inputs["Vector"], vec)
        n.inputs["Scale"].default_value = scale
        return n.outputs["Distance"]

    def mix(self, fac, a, b):
        n = self.node("ShaderNodeMix", data_type="RGBA", clamp_factor=True)
        ins = [s for s in n.inputs if s.type == "RGBA"]
        self.put(n.inputs[0], fac)
        self.put(ins[0], a if hasattr(a, "is_output") else (*a, 1.0))
        self.put(ins[1], b if hasattr(b, "is_output") else (*b, 1.0))
        return [s for s in n.outputs if s.type == "RGBA"][0]

    def lerp(self, fac, a, b):
        n = self.node("ShaderNodeMix", data_type="FLOAT", clamp_factor=True)
        ins = [s for s in n.inputs if s.type == "VALUE"]
        self.put(ins[0], fac)
        self.put(ins[1], a)
        self.put(ins[2], b)
        return [s for s in n.outputs if s.type == "VALUE"][0]

    def bump(self, height, strength, distance):
        n = self.node("ShaderNodeBump")
        self.put(n.inputs["Height"], height)
        n.inputs["Strength"].default_value = strength
        n.inputs["Distance"].default_value = distance
        self.link(n.outputs["Normal"], self.bsdf.inputs["Normal"])

    def finish(self, color, rough, spec=0.5):
        self.put(self.bsdf.inputs["Base Color"], color)
        self.put(self.bsdf.inputs["Roughness"], rough)
        self.bsdf.inputs["Specular IOR Level"].default_value = spec
        return self.mat


def check_render(opt, cam):
    """Sky, sun and Cycles settings for the check renders; renders if asked."""
    scene = bpy.context.scene
    scene.camera = cam
    sun_parent = cam.users_collection[0]
    # sky and sun: a clear late-morning sun from the south-east
    world = bpy.data.worlds.get("EBAW_Sky") or bpy.data.worlds.new("EBAW_Sky")
    scene.world = world
    world.use_nodes = True
    nt = world.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputWorld")
    bg = nt.nodes.new("ShaderNodeBackground")
    sky = nt.nodes.new("ShaderNodeTexSky")
    kinds = [i.identifier for i in sky.bl_rna.properties["sky_type"].enum_items]
    for want in ("MULTIPLE_SCATTERING", "NISHITA", "SINGLE_SCATTERING", "HOSEK_WILKIE"):
        if want in kinds:
            sky.sky_type = want
            break
    # the Sun lamp below lights the scene; the sky gives only the blue fill
    for k, v in (("sun_disc", False), ("sun_elevation", math.radians(32)), ("sun_rotation", math.radians(150)), ("altitude", 12.0)):
        if hasattr(sky, k):
            setattr(sky, k, v)
    bg.inputs["Strength"].default_value = 0.25
    nt.links.new(sky.outputs[0], bg.inputs[0])
    nt.links.new(bg.outputs[0], out.inputs[0])
    old = bpy.data.objects.get("Sun")
    if old is not None:
        bpy.data.objects.remove(old, do_unlink=True)
    sun_d = bpy.data.lights.new("Sun", "SUN")
    sun_d.energy = 3.2
    sun_d.angle = math.radians(0.6)
    sun = bpy.data.objects.new("Sun", sun_d)
    sun_parent.objects.link(sun)
    # light coming from azimuth 150 deg (SSE), 32 deg up
    az, el = math.radians(150), math.radians(32)
    d = Vector((math.sin(az) * math.cos(el), math.cos(az) * math.cos(el), math.sin(el)))
    sun.rotation_euler = d.to_track_quat("Z", "Y").to_euler()

    try:
        scene.render.engine = "CYCLES"
    except TypeError:
        pass
    scene.cycles.samples = int(opt["samples"])
    scene.cycles.use_denoising = True
    scene.cycles.device = "CPU"
    w, h = (int(x) for x in opt["res"].split("x"))
    scene.render.resolution_x, scene.render.resolution_y = w, h
    scene.render.resolution_percentage = 100
    try:
        scene.view_settings.view_transform = "AgX"
    except TypeError:
        pass
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = opt["render"]
    if opt["save"]:
        bpy.ops.wm.save_as_mainfile(filepath=str(Path(opt["save"]).resolve()))
    if opt["render"] and opt["render"] != "none":
        bpy.ops.render.render(write_still=True)
        print("rendered", opt["render"])


def camera(name, coll, loc, tgt, lens):
    old = bpy.data.objects.get(name)
    if old is not None:
        bpy.data.objects.remove(old, do_unlink=True)
    cd = bpy.data.cameras.new(name)
    cd.lens = lens
    cd.clip_start = 0.5
    cd.clip_end = 20000.0
    cam = bpy.data.objects.new(name, cd)
    coll.objects.link(cam)
    cam.location = loc
    cam.rotation_euler = (Vector(tgt) - Vector(loc)).to_track_quat("-Z", "Y").to_euler()
    return cam


def vmul(g, vec, scale):
    """Graph helper: a vector scaled per axis (stretched noise for streaks)."""
    n = g.node("ShaderNodeVectorMath", operation="MULTIPLY")
    g.put(n.inputs[0], vec)
    n.inputs[1].default_value = scale
    return n.outputs[0]


# ---------------------------------------------------------------- meshes

class Mesh:
    """Accumulates verts and faces (with material slots) for one object."""

    def __init__(self, slots):
        self.slots = slots          # material keys, by slot index
        self.v, self.f, self.m = [], [], []

    def _slot(self, key):
        return self.slots.index(key)

    def face(self, pts, key):
        base = len(self.v)
        self.v += [tuple(p) for p in pts]
        self.f.append(tuple(range(base, base + len(pts))))
        self.m.append(self._slot(key))

    def box(self, x0, x1, y0, y1, z0, z1, key, skip=()):
        p = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
             (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
        sides = {"bottom": (0, 3, 2, 1), "top": (4, 5, 6, 7), "front": (0, 1, 5, 4),
                 "right": (1, 2, 6, 5), "back": (2, 3, 7, 6), "left": (3, 0, 4, 7)}
        for k, idx in sides.items():
            if k not in skip:
                self.face([p[i] for i in idx], key)

    def prism(self, pts, z0, z1, key, top=True, bottom=True, side_key=None):
        """A vertical prism on a counter-clockwise outline."""
        n = len(pts)
        for i in range(n):
            (x0, y0), (x1, y1) = pts[i], pts[(i + 1) % n]
            self.face([(x0, y0, z0), (x1, y1, z0), (x1, y1, z1), (x0, y0, z1)], side_key or key)
        if top:
            self.face([(x, y, z1) for x, y in pts], key)
        if bottom:
            self.face([(x, y, z0) for x, y in reversed(pts)], key)

    def frustum(self, lo, hi, z0, z1, key):
        """Side faces between two counter-clockwise outlines (an inclined band)."""
        n = len(lo)
        for i in range(n):
            j = (i + 1) % n
            self.face([(*lo[i], z0), (*lo[j], z0), (*hi[j], z1), (*hi[i], z1)], key)

    def slab(self, rings, z, key, up=True):
        """A flat polygon with holes (rings: outer CCW first, then holes)."""
        loops = [[Vector((x, y, 0.0)) for x, y in r] for r in rings]
        flat = [p for r in rings for p in r]
        for t in tessellate_polygon(loops):
            a, b, c = (flat[i] for i in t)
            ccw = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0
            tri = [a, b, c] if ccw == up else [a, c, b]
            self.face([(x, y, z) for x, y in tri], key)

    def build(self, name, coll, parent, bevel=0.0, merge=True, smooth=False):
        me = bpy.data.meshes.new(name)
        me.from_pydata(self.v, [], self.f)
        for k in self.slots:
            me.materials.append(MATS[k])
        for p, m in zip(me.polygons, self.m):
            p.material_index = m
            p.use_smooth = smooth
        if merge:
            bm = bmesh.new()
            bm.from_mesh(me)
            bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-4)
            bm.to_mesh(me)
            bm.free()
        me.validate()
        box_uv(me)
        ob = bpy.data.objects.new(name, me)
        coll.objects.link(ob)
        ob.parent = parent
        if bevel > 0:
            add_bevel(ob, bevel)
        return ob


def add_bevel(ob, width):
    """The fine architectural chamfer (2-5 cm) on edges sharper than 30 degrees."""
    md = ob.modifiers.new("Bevel", "BEVEL")
    md.width = width
    md.segments = 2
    md.limit_method = "ANGLE"
    md.angle_limit = math.radians(30)
    md.harden_normals = False


def box_uv(me, size=2.0):
    """Box-projected UVs, 2 m a tile, ready for baking at export."""
    uv = me.uv_layers.new(name="UVMap")
    for p in me.polygons:
        n = p.normal
        ax = max(range(3), key=lambda i: abs(n[i]))
        for li in p.loop_indices:
            co = me.vertices[me.loops[li].vertex_index].co
            u, v = [(co.y, co.z), (co.x, co.z), (co.x, co.y)][ax]
            uv.data[li].uv = (u / size, v / size)


def instance(data, name, coll, parent, loc, rot_z=0.0, rot=None, scale=None):
    """A linked duplicate (Alt+D): the mesh data is shared."""
    ob = bpy.data.objects.new(name, data)
    coll.objects.link(ob)
    ob.parent = parent
    ob.location = loc
    ob.rotation_euler = rot if rot else (0.0, 0.0, rot_z)
    if scale:
        ob.scale = scale
    return ob


def window_mesh(name, w, h, cols, rows, glass="glass", bar=0.06, depth=0.10):
    """A window module: frame bars round `cols` x `rows` panes, glazed.

    Local frame: X along the facade (0..w), Y outward (glass at -depth/2),
    Z up (0..h). Built once and instanced along every facade.
    """
    pw = (w - (cols + 1) * bar) / cols
    ph = (h - (rows + 1) * bar) / rows
    xp = [(bar + i * (pw + bar), bar + i * (pw + bar) + pw) for i in range(cols)]
    zp = [(bar + j * (ph + bar), bar + j * (ph + bar) + ph) for j in range(rows)]
    xs = sorted({0.0, w, *[x for iv in xp for x in iv]})
    zs = sorted({0.0, h, *[z for iv in zp for z in iv]})
    bm = bmesh.new()
    vid = {}

    def v(x, z):
        k = (round(x, 5), round(z, 5))
        if k not in vid:
            vid[k] = bm.verts.new((x, 0.0, z))
        return vid[k]
    frame = []
    for i in range(len(xs) - 1):
        for j in range(len(zs) - 1):
            pane = (xs[i], xs[i + 1]) in xp and (zs[j], zs[j + 1]) in zp
            if not pane:
                # +Y facing
                frame.append(bm.faces.new((v(xs[i], zs[j]), v(xs[i], zs[j + 1]), v(xs[i + 1], zs[j + 1]), v(xs[i + 1], zs[j]))))
    bmesh.ops.solidify(bm, geom=frame, thickness=depth)
    for f in bm.faces:
        f.material_index = 0
    for x0, x1 in xp:
        for z0, z1 in zp:
            vs = [bm.verts.new((x, -depth / 2, z)) for x, z in ((x0, z0), (x0, z1), (x1, z1), (x1, z0))]
            bm.faces.new(vs).material_index = 1
    bm.normal_update()
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    # the frame came out on whichever side solidify chose: put its outer face at Y = 0
    ys = [vt.co.y for vt in me.vertices]
    off = -max(ys) if max(ys) > 1e-6 else 0.0
    for vt in me.vertices:
        vt.co.y += off
    me.materials.append(MATS["frame"])
    me.materials.append(MATS[glass])
    box_uv(me)
    return me



def facade(mesh, p0, p1, z0, z1, openings, bands, wall_key="render", band_key="band"):
    """One wall face from p0 to p1 (outline counter-clockwise, outside on the right),
    with rectangular openings (x0, x1, za, zb) along it; the rows inside `bands`
    (z ranges) get the band material."""
    p0, p1 = Vector((*p0, 0.0)), Vector((*p1, 0.0))
    d = p1 - p0
    L = d.length
    d.normalize()
    xs = sorted({0.0, L, *[x for o in openings for x in o[:2]]})
    zs = sorted({z0, z1, *[z for o in openings for z in o[2:]], *[z for b in bands for z in b if z0 < z < z1]})

    def inside(x, z):
        return any(o[0] <= x <= o[1] and o[2] <= z <= o[3] for o in openings)
    for i in range(len(xs) - 1):
        for j in range(len(zs) - 1):
            xm, zm = (xs[i] + xs[i + 1]) / 2, (zs[j] + zs[j + 1]) / 2
            if inside(xm, zm):
                continue
            key = band_key if any(b[0] <= zm <= b[1] for b in bands) else wall_key
            q = [p0 + d * x for x in (xs[i], xs[i + 1])]
            mesh.face([(q[0].x, q[0].y, zs[j]), (q[1].x, q[1].y, zs[j]), (q[1].x, q[1].y, zs[j + 1]), (q[0].x, q[0].y, zs[j + 1])], key)
    return L, d


def bays(x0, x1, per_bay, pier=0.9):
    """Window groups of per_bay modules separated by piers, centred in x0..x1.
    Returns (group start, modules) pairs."""
    span = per_bay * MODULE + pier
    n = int((x1 - x0 + pier) // span)
    if n < 1:
        k = int((x1 - x0) // MODULE)
        return [((x0 + x1 - k * MODULE) / 2, k)] if k else []
    start = (x0 + x1 - (n * span - pier)) / 2
    return [(start + i * span, per_bay) for i in range(n)]



def octagon(r, rot=math.pi / 8, cx=0.0, cy=0.0):
    return [(cx + r * math.cos(rot + i * math.pi / 4), cy + r * math.sin(rot + i * math.pi / 4)) for i in range(8)]


def rail(m, pts, z0, height, key="galv", closed=True, post_every=1.6):
    """A railing along a polyline: posts, top rail and mid rail (thin boxes along each run)."""
    segs = list(zip(pts, pts[1:] + ([pts[0]] if closed else [])))
    for (x0, y0), (x1, y1) in segs:
        L = math.dist((x0, y0), (x1, y1))
        dx, dy = (x1 - x0) / L, (y1 - y0) / L
        nx, ny = -dy, dx
        for zc, t in ((z0 + height - 0.03, 0.025), (z0 + height * 0.5, 0.012)):
            q = [(x0 + nx * t, y0 + ny * t), (x1 + nx * t, y1 + ny * t), (x1 - nx * t, y1 - ny * t), (x0 - nx * t, y0 - ny * t)]
            q = q if (q[1][0] - q[0][0]) * (q[2][1] - q[0][1]) - (q[1][1] - q[0][1]) * (q[2][0] - q[0][0]) > 0 else q[::-1]
            m.prism(q, zc - t, zc + t, key)
        k = max(1, int(L // post_every))
        for i in range(k):
            px, py = x0 + dx * L * i / k, y0 + dy * L * i / k
            m.prism([(px - 0.025, py - 0.025), (px + 0.025, py - 0.025), (px + 0.025, py + 0.025), (px - 0.025, py + 0.025)], z0, z0 + height - 0.03, key)


def text_object(text, size, depth, coll, parent, loc, rot, key):
    cu = bpy.data.curves.new(f"TXT_{text}", "FONT")
    cu.body = text
    cu.size = size
    cu.extrude = depth / 2
    cu.align_x = "CENTER"
    cu.align_y = "CENTER"
    cu.bevel_depth = 0.0
    cu.resolution_u = 3   # letters 0.5-1 m high: 3 segments a curve is plenty (12 made 50k triangles a sign)
    try:
        cu.font = bpy.data.fonts.load(FONT, check_existing=True)
    except (RuntimeError, OSError):
        pass
    tmp = bpy.data.objects.new("tmp", cu)
    coll.objects.link(tmp)
    dg = bpy.context.evaluated_depsgraph_get()
    me = bpy.data.meshes.new_from_object(tmp.evaluated_get(dg))
    bpy.data.objects.remove(tmp)
    bpy.data.curves.remove(cu)
    me.name = f"TXT_{text}"
    me.materials.clear()
    me.materials.append(MATS[key])
    ob = bpy.data.objects.new(f"Sign_{text.replace(' ', '_')}", me)
    coll.objects.link(ob)
    ob.parent = parent
    ob.location = loc
    ob.rotation_euler = rot
    return ob


