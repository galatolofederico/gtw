"""EBAW (Antwerp International Airport) - step 3: hangars and airside buildings.

Builds collection 03_Hangars: every building along the aprons and taxiways
(41 from OpenStreetMap), each from its OSM footprint's minimum rectangle,
doors on the side facing the pavement. Kinds:

  hangar   ribbed metal cladding, shallow gable roof with skylights, big
           sliding doors on several tracks with rails, guide header, hazard
           stripes and a door-sweep zone, floodlights, gutters and downpipes,
           roof ladder, condensers and wall lights; two are drawn open with
           their steel trusses, floor and high-bay lights inside
  museum   the Stampe & Vertongen museum hangar (white, open, lettered)
  fire     the airport fire station: flat roof, four red sectional doors
  t_row    rows of small private hangars, mono-pitch roof, a door a bay
  shed     small stores and workshops: flat roof, roller door
  office   the terminal annex: rendered, two floors of ribbon windows

    blender -b assets/blender/ebaw/ebaw.blend --python assets/blender/ebaw/03_hangars.py -- \
        [--render /tmp/ebaw_step3.png] [--shot south|open|museum|north|fire] [--samples 48] \
        [--res 1920x1080] [--save assets/blender/ebaw/ebaw.blend]

Footprints, door sides and kinds come from ebaw_layout.json (ebaw_osm.py;
map data (c) OpenStreetMap contributors, ODbL). Which building is the
Stampe & Vertongen museum (OSM way 44802700, the white hangar north of the
terminal on the photographs) and which hangars are open are choices made in
ebaw_osm.py. Heights are estimated from the spans (eaves about 4 m + 11 % of
the span, 6 deg roofs), not surveyed. Colours are varied per building.

Frame: the shared EBAW frame. Each building sits under an empty HGR_<osm id>
at the centre of its rectangle, local X along the door side, local Y out
through the doors, Z up from the ground.
"""

import json
import math
import runpy
import sys
from pathlib import Path

import bpy
from mathutils import Vector

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from ebaw_lib import (MATS, Graph, Mesh, add_bevel, args, bays, camera, check_render, collection,  # noqa: E402
                      facade, instance, octagon, text_object, vmul, window_mesh)

LAYOUT = json.loads((HERE / "ebaw_layout.json").read_text())
COLL = "03_Hangars"
PITCH = math.radians(6.0)
OVERHANG = 0.35


# ---------------------------------------------------------------- materials

def mat_cladding(name, rgb, axis="xy", pitch=0.2):
    """Trapezoidal-rib steel cladding: the ribs are a bump profile across the
    sheet (axis "xy" -> vertical ribs on walls, "x"/"y" -> roof ribs down the
    slope, "z" -> horizontal grooves of sectional doors); weathering streaks,
    dirt at the foot, a little chalking."""
    g = Graph(name)
    sep = g.node("ShaderNodeSeparateXYZ")
    g.put(sep.inputs[0], g.pos)
    x, y, z = sep.outputs["X"], sep.outputs["Y"], sep.outputs["Z"]
    s = {"xy": g.math("ADD", x, y), "x": x, "y": y, "z": z}[axis]
    prof = g.smooth(g.math("SINE", g.math("MULTIPLY", s, 2 * math.pi / pitch)), -0.35, 0.35)
    streak = g.smooth(g.noise(vmul(g, g.pos, (3.0, 3.0, 0.1)), 1.0, 5.0, 0.6), 0.6, 0.8)
    blotch = g.noise(g.pos, 0.4, 4.0)
    foot = g.math("SUBTRACT", 1.0, g.smooth(z, 0.05, 1.2))
    c = g.mix(g.smooth(blotch, 0.3, 0.7), rgb, tuple(min(1.0, v * 1.12 + 0.01) for v in rgb))
    c = g.mix(g.math("MULTIPLY", streak, 0.35), c, tuple(v * 0.55 for v in rgb))
    c = g.mix(g.math("MULTIPLY", foot, 0.5), c, (0.06, 0.055, 0.05))
    c = g.mix(g.math("MULTIPLY", prof, 0.08), c, tuple(v * 0.85 for v in rgb))  # rib shadows read even flat-lit
    g.bump(g.math("ADD", prof, g.math("MULTIPLY", g.noise(g.pos, 30.0, 4.0), 0.05)), 0.6, 0.03)
    return g.finish(c, g.lerp(streak, 0.42, 0.6), 0.5)


def mat_hazard():
    """Yellow and black diagonal stripes, 25 cm, worn."""
    g = Graph("M_Hazard_Stripes")
    sep = g.node("ShaderNodeSeparateXYZ")
    g.put(sep.inputs[0], g.pos)
    s = g.math("ADD", g.math("ADD", sep.outputs["X"], sep.outputs["Y"]), sep.outputs["Z"])
    band = g.math("GREATER_THAN", g.math("FRACT", g.math("DIVIDE", s, 0.5)), 0.5)
    wear = g.smooth(g.noise(g.pos, 3.0, 6.0), 0.6, 0.75)
    c = g.mix(band, (0.80, 0.55, 0.0), (0.015, 0.015, 0.015))
    c = g.mix(g.math("MULTIPLY", wear, 0.5), c, (0.25, 0.24, 0.22))
    return g.finish(c, 0.55, 0.5)


def mat_simple(name, rgb, rough=0.6, metallic=0.0, emit=0.0, transmission=0.0):
    g = Graph(name)
    g.bsdf.inputs["Metallic"].default_value = metallic
    if transmission:
        g.bsdf.inputs["Transmission Weight"].default_value = transmission
    if emit:
        g.bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
        g.bsdf.inputs["Emission Strength"].default_value = emit
    fine = g.noise(g.pos, 8.0, 4.0)
    c = g.mix(g.math("MULTIPLY", fine, 0.2), rgb, tuple(v * 0.8 for v in rgb))
    g.bump(fine, 0.05, 0.002)
    return g.finish(c, rough, 0.5)


def mat_gravel():
    g = Graph("M_Roof_Gravel")
    fine = g.noise(g.pos, 30.0, 8.0, 0.7)
    c = g.mix(g.smooth(fine, 0.3, 0.7), (0.12, 0.115, 0.105), (0.24, 0.23, 0.21))
    g.bump(fine, 0.6, 0.01)
    return g.finish(c, 0.92, 0.3)


def mat_concrete():
    g = Graph("M_Concrete_Plinth")
    fine = g.noise(g.pos, 16.0, 6.0, 0.65)
    c = g.mix(g.smooth(g.noise(g.pos, 1.0, 4.0), 0.3, 0.7), (0.26, 0.255, 0.24), (0.34, 0.33, 0.31))
    g.bump(fine, 0.3, 0.003)
    return g.finish(c, 0.85, 0.4)


PALETTE = [  # cladding colours seen on the photographs and usual for the type
    ("Grey", (0.30, 0.31, 0.31)), ("Anthracite", (0.045, 0.047, 0.05)), ("OffWhite", (0.55, 0.54, 0.50)),
    ("BlueGrey", (0.12, 0.15, 0.19)), ("Sand", (0.38, 0.34, 0.27)), ("Silver", (0.42, 0.43, 0.43)),
]


def materials():
    for nm, rgb in PALETTE:
        MATS["clad_" + nm] = mat_cladding("M_Cladding_" + nm, rgb)
    MATS["roof_x"] = mat_cladding("M_Roof_Ribs_X", (0.33, 0.34, 0.34), axis="x", pitch=0.25)
    MATS["roof_y"] = mat_cladding("M_Roof_Ribs_Y", (0.33, 0.34, 0.34), axis="y", pitch=0.25)
    MATS["door_red"] = mat_cladding("M_Door_Sectional_Red", (0.45, 0.02, 0.015), axis="z", pitch=0.5)
    MATS["door_grey"] = mat_cladding("M_Door_Roller_Grey", (0.36, 0.37, 0.37), axis="z", pitch=0.1)
    MATS["hazard"] = mat_hazard()
    MATS["trim"] = mat_concrete()
    MATS["gravel"] = mat_gravel()
    MATS["galv"] = mat_simple("M_Metal_Galvanised", (0.45, 0.46, 0.47), 0.4, 0.9)
    MATS["steel"] = mat_simple("M_Steel_Primer", (0.16, 0.20, 0.24), 0.55, 0.2)
    MATS["dark"] = mat_simple("M_Metal_Dark", (0.035, 0.037, 0.04), 0.45, 0.7)
    MATS["epoxy"] = mat_simple("M_Floor_Epoxy", (0.16, 0.19, 0.18), 0.25)
    MATS["skylight"] = mat_simple("M_Polycarbonate", (0.75, 0.76, 0.72), 0.35, 0.0, 0.0, 0.6)
    MATS["lamp"] = mat_simple("M_Light_Cool", (1.0, 0.95, 0.88), 0.4, 0.0, 12.0)
    MATS["render"] = MATS.get("render") or mat_simple("M_Render_White", (0.6, 0.595, 0.57), 0.85)
    MATS["band"] = MATS["trim"]
    MATS["frame"] = mat_simple("M_Frame_Anthracite", (0.025, 0.027, 0.03), 0.35, 0.6)
    MATS["glass"] = mat_simple("M_Glass_Clear", (0.80, 0.86, 0.84), 0.03, 0.0, 0.0, 1.0)
    MATS["letters"] = mat_simple("M_Letters_Anthracite", (0.04, 0.045, 0.05), 0.4, 0.5)
    MATS["red_band"] = mat_simple("M_Trim_Red", (0.45, 0.02, 0.015), 0.5)


# ---------------------------------------------------------------- shared parts (instanced)

SHARED = {}


def shared_parts(coll):
    """Meshes every building instances: floodlight, wall light, condenser, split unit, personnel door."""
    def mk(name, mesh):
        ob = mesh.build(name, coll, None)
        me = ob.data
        bpy.data.objects.remove(ob)
        return me
    m = Mesh(["dark", "lamp"])
    m.box(-0.3, 0.3, 0.0, 0.25, -0.12, 0.12, "dark")
    m.face([(0.25, 0.2, -0.13), (0.25, 0.02, -0.13), (-0.25, 0.02, -0.13), (-0.25, 0.2, -0.13)], "lamp")  # faces down
    SHARED["flood"] = mk("LIGHT_Flood", m)
    m = Mesh(["dark", "lamp"])
    m.box(-0.15, 0.15, 0.0, 0.12, -0.18, 0.18, "dark")
    m.face([(-0.12, 0.121, -0.14), (-0.12, 0.121, 0.14), (0.12, 0.121, 0.14), (0.12, 0.121, -0.14)], "lamp")  # faces +Y
    SHARED["wall_light"] = mk("LIGHT_WallPack", m)
    m = Mesh(["galv", "dark"])
    m.box(-0.5, 0.5, -0.2, 0.2, 0.0, 0.85, "galv")
    m.prism(octagon(0.28, cx=0.0, cy=-0.201), 0.2, 0.75, "dark", top=False, bottom=False)
    SHARED["condenser"] = mk("AC_Condenser", m)
    m = Mesh(["galv"])
    m.box(-0.45, 0.45, 0.0, 0.25, -0.15, 0.15, "galv")
    SHARED["split"] = mk("AC_Split", m)
    m = Mesh(["dark", "galv"])
    m.box(-0.5, 0.5, 0.0, 0.05, 0.0, 2.15, "dark")
    m.box(-0.05, 0.05, 0.05, 0.1, 1.0, 1.08, "galv")
    m.box(-0.7, 0.7, 0.0, 1.0, 2.35, 2.45, "galv")  # little canopy over it
    SHARED["pdoor"] = mk("DOOR_Personnel", m)


# ---------------------------------------------------------------- pieces

def rot90(u, v):
    """Canonical roof frame (ridge along u) -> building frame when the ridge runs along Y."""
    return (-v, u)


def tube(m, p, q, w, key):
    """A square tube of side w from p to q (members of trusses, ladder rails)."""
    p, q = Vector(p), Vector(q)
    d = (q - p).normalized()
    a = d.cross(Vector((0, 0, 1))) if abs(d.z) < 0.9 else d.cross(Vector((1, 0, 0)))
    a.normalize()
    b = d.cross(a)
    h = w / 2
    c = [a * sa * h + b * sb * h for sa, sb in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
    for i in range(4):
        j = (i + 1) % 4
        m.face([(p + c[i]).to_tuple(), (p + c[j]).to_tuple(), (q + c[j]).to_tuple(), (q + c[i]).to_tuple()][::-1], key)


def ladder(m, x, y, z0, z1, out):
    """A fixed roof-access ladder standing off a wall; out = outward unit (x, y)."""
    ox, oy = out
    sx, sy = -oy * 0.25, ox * 0.25
    for s in (-1, 1):
        tube(m, (x + s * sx, y + s * sy, z0), (x + s * sx, y + s * sy, z1), 0.05, "galv")
    z = z0 + 0.3
    while z < z1 - 0.1:
        tube(m, (x - sx, y - sy, z), (x + sx, y + sy, z), 0.03, "galv")
        z += 0.3
    for s in (-1, 1):  # stand-off brackets
        for zb in (z0 + 0.4, (z0 + z1) / 2, z1 - 0.3):
            tube(m, (x + s * sx - ox * 0.2, y + s * sy - oy * 0.2, zb), (x + s * sx, y + s * sy, zb), 0.04, "galv")


def gable_building(b, coll, root, clad, name_text=None):
    """Hangar, museum: gable roof, sliding doors, the lot."""
    L, D = b["L"], b["D"]
    ridge_x = L >= D
    span = D if ridge_x else L
    m = b.get("measured")
    if m:
        # measured (DHMV II, ebaw_vl.py): eave and ridge; a flat-looking roof keeps a 2 deg fall
        he = max(3.5, m["eave"])
        hr = max(m["ridge"], he + span / 2 * math.tan(math.radians(2.0))) if m["roof"] == "gable" else he + span / 2 * math.tan(math.radians(2.0))
    else:
        # estimated from the span: eaves about 4 m + 11 % of it
        he = max(5.5, min(11.0, 4.0 + 0.11 * span))
        hr = he + span / 2 * math.tan(PITCH)
    tp = min(max((hr - he) / (span / 2), math.tan(math.radians(2.0))), math.tan(math.radians(20.0)))
    hr = he + span / 2 * tp
    dw = min(L - 3.0, 0.86 * L)
    hd = he - 1.1 if ridge_x else he - 0.4
    mp = (lambda u, v: (u, v)) if ridge_x else rot90
    Lr, Dr = (L, D) if ridge_x else (D, L)

    # walls with the door, a personnel door and office windows; gables on the ends
    w = Mesh(["wall", "trim"])
    MATS["wall"] = MATS[clad]
    out = [(-L / 2, -D / 2), (L / 2, -D / 2), (L / 2, D / 2), (-L / 2, D / 2)]
    win = window_mesh(f"WIN_Office_{b['id']}", 1.5, 1.2, 2, 1)
    wins = []
    for i in range(4):
        p0, p1 = out[i], out[(i + 1) % 4]
        ln = math.dist(p0, p1)
        opens = []
        if i == 2:
            opens.append(((ln - dw) / 2, (ln + dw) / 2, 0.0, hd))
        if i == 1 and ln > 8:
            opens.append((ln - 4.0, ln - 3.0, 0.0, 2.15))
        if i == 0:
            for x0, k in bays(2.0, max(2.5, ln * 0.4), 2):
                opens.append((x0, x0 + k * 1.5, 1.0, 2.2))
                for j in range(k):
                    wins.append((p0, p1, x0 + (j + 1) * 1.5))
        facade(w, p0, p1, 0.0, he, opens, [(0.0, 0.4)], wall_key="wall", band_key="trim")
    # gable triangles
    for (u0, u1) in ((-Lr / 2, Lr / 2),):
        for su in (-1, 1):
            u = su * Lr / 2
            a, c = (u, -Dr / 2 * su), (u, Dr / 2 * su)  # traversed so the face points out along su*u
            pa, pc = mp(*a), mp(*c)
            pm = mp(u, 0.0)
            w.face([(*pa, he), (*pc, he), (*pm, hr)], "wall")
    wob = w.build(f"HGR_{b['id']}_Walls", coll, root)
    sd = wob.modifiers.new("Wall", "SOLIDIFY")
    sd.thickness, sd.offset, sd.use_even_offset, sd.use_rim, sd.material_offset_rim = 0.2, -1.0, True, True, 1
    add_bevel(wob, 0.02)
    for p0, p1, x in wins:
        d = (Vector((*p1, 0)) - Vector((*p0, 0))).normalized()
        at = Vector((*p0, 0)) + d * x + Vector((d.y, -d.x, 0)) * -0.08
        instance(win, f"HGR_{b['id']}_Win", coll, root, (at.x, at.y, 1.0), math.atan2(d.y, d.x) + math.pi)
    instance(SHARED["pdoor"], f"HGR_{b['id']}_PDoor", coll, root, (L / 2, D / 2 - 3.5, 0.0), -math.pi / 2)

    # roof: two slopes with overhang, ridge cap, skylight strips, gutters and downpipes
    r = Mesh(["roof", "skylight", "galv"])
    MATS["roof"] = MATS["roof_x" if ridge_x else "roof_y"]
    o = OVERHANG
    ze = he - o * tp
    for s in (-1, 1):
        pts = [(-Lr / 2 - o, s * (Dr / 2 + o), ze), (Lr / 2 + o, s * (Dr / 2 + o), ze), (Lr / 2 + o, 0.0, hr), (-Lr / 2 - o, 0.0, hr)]
        if s > 0:  # both slopes face up
            pts = pts[::-1]
        r.face([(*mp(u, v), z) for u, v, z in pts], "roof")
        # skylights: 1 m x 3 m sheets a third of the way up
        vk = s * Dr / 4
        zk = hr - abs(vk) * tp + 0.03
        k = 0
        u = -Lr / 2 + 3.0
        while u < Lr / 2 - 5.0:
            q = [(u, vk - 0.5 * s, zk - 0.5 * tp * -1), (u + 3.0, vk - 0.5 * s, zk + 0.5 * tp),
                 (u + 3.0, vk + 0.5 * s, zk - 0.5 * tp), (u, vk + 0.5 * s, zk - 0.5 * tp)]
            q = [(u0, v0, hr - abs(v0) * tp + 0.03) for u0, v0, _ in q]
            if s < 0:
                q = q[::-1]
            r.face([(*mp(u0, v0), z0) for u0, v0, z0 in q], "skylight")
            u += 6.0
            k += 1
        # gutter along the eave
        gv = s * (Dr / 2 + o + 0.08)
        g0, g1 = mp(-Lr / 2 - o, gv - 0.08), mp(Lr / 2 + o, gv + 0.08)
        r.box(min(g0[0], g1[0]), max(g0[0], g1[0]), min(g0[1], g1[1]), max(g0[1], g1[1]), ze - 0.16, ze, "galv")
    c0, c1 = mp(-Lr / 2 - o, -0.2), mp(Lr / 2 + o, 0.2)
    r.box(min(c0[0], c1[0]), max(c0[0], c1[0]), min(c0[1], c1[1]), max(c0[1], c1[1]), hr - 0.02, hr + 0.08, "galv")
    rob = r.build(f"HGR_{b['id']}_Roof", coll, root)
    rob["tint"] = roof_tint(b, (0.33, 0.34, 0.34))
    sd = rob.modifiers.new("Sheet", "SOLIDIFY")
    sd.thickness, sd.offset, sd.use_rim = 0.1, -1.0, True
    add_bevel(rob, 0.02)

    dp = Mesh(["galv"])
    dp.prism(octagon(0.05), 0.0, ze, "galv", top=False, bottom=False)
    dp_me = dp.build("tmp_dp", coll, None).data
    bpy.data.objects.remove(bpy.data.objects["tmp_dp"])
    dp_me.name = f"PIPE_{b['id']}"
    for su in (-1, 1):
        for sv in (-1, 1):
            x, y = mp(su * (Lr / 2 - 0.3), sv * (Dr / 2 + o + 0.08))
            instance(dp_me, f"HGR_{b['id']}_Downpipe", coll, root, (x, y, 0.0))

    # sliding doors on telescopic tracks outside the opening
    n = max(2, round(dw / 7.0))
    lw = dw / n + 0.1
    lf = Mesh(["leaf", "hazard", "glass", "dark"])
    MATS["leaf"] = MATS[clad]
    lf.box(0.0, lw, 0.0, 0.12, 0.0, hd - 0.05, "leaf")
    # decals on the +Y face: listed (x0, z0), (x0, z1), (x1, z1), (x1, z0) so they face +Y
    lf.face([(0.0, 0.122, 0.0), (0.0, 0.122, 0.4), (lw, 0.122, 0.4), (lw, 0.122, 0.0)], "hazard")
    lf.face([(lw - 0.15, 0.122, 0.0), (lw - 0.15, 0.122, hd - 0.05), (lw, 0.122, hd - 0.05), (lw, 0.122, 0.0)], "hazard")
    lf.box(0.3, lw - 0.3, 0.12, 0.13, 2.6, 3.2, "dark")  # vision strip frame
    lf.face([(0.4, 0.131, 2.65), (0.4, 0.131, 3.15), (lw - 0.4, 0.131, 3.15), (lw - 0.4, 0.131, 2.65)], "glass")
    leaf_ob = lf.build("tmp_leaf", coll, None)
    leaf_me = leaf_ob.data
    leaf_me.name = f"DOOR_Leaf_{b['id']}"
    bpy.data.objects.remove(leaf_ob)
    m_tracks = math.ceil(n / 2)
    for i in range(n):
        tr = min(i, n - 1 - i)
        y = D / 2 + 0.08 + tr * 0.16
        if b["open"]:
            x0 = -dw / 2 - 0.05 if i < n / 2 else dw / 2 - lw + 0.05
        else:
            x0 = -dw / 2 + i * (dw / n) - 0.05
        instance(leaf_me, f"HGR_{b['id']}_DoorLeaf", coll, root, (x0, y, 0.0))
    hdr = Mesh(["trim", "steel", "hazard", "galv"])
    ytop = D / 2 + 0.08 + m_tracks * 0.16 + 0.1
    hdr.box(-dw / 2 - 0.6, dw / 2 + 0.6, D / 2, ytop, hd, min(hd + 0.7, he - 0.05), "steel", skip=("front",))
    for t in range(m_tracks):
        y = D / 2 + 0.08 + t * 0.16 + 0.03
        hdr.box(-dw / 2 - 0.5, dw / 2 + 0.5, y, y + 0.06, 0.0, 0.02, "galv", skip=("bottom",))
    # door-sweep zone on the apron, hatched
    hdr.face([(-dw / 2, ytop + 0.2, 0.012), (dw / 2, ytop + 0.2, 0.012), (dw / 2, ytop + 1.6, 0.012), (-dw / 2, ytop + 1.6, 0.012)], "hazard")
    hob = hdr.build(f"HGR_{b['id']}_DoorGear", coll, root)
    add_bevel(hob, 0.01)
    k = max(2, int(dw // 9) + 1)
    for i in range(k):
        x = -dw / 2 + dw * (i + 0.5) / k
        instance(SHARED["flood"], f"HGR_{b['id']}_Flood", coll, root, (x, ytop, min(hd + 0.9, he - 0.2)))
    if name_text:
        text_object(name_text, 0.55 if len(name_text) < 12 else 0.8, 0.05, coll, root,
                    (0.0, ytop + 0.03, min(hd + 1.6, he + 0.3)), (math.pi / 2, 0.0, math.pi), "letters").parent = root
        bpy.context.view_layer.update()

    # ladder up a side wall, condensers and split units on the back, wall lights
    lad = Mesh(["galv"])
    ladder(lad, L / 2 + 0.25, -D / 4, 0.3, he + 1.0, (1.0, 0.0))
    lad.build(f"HGR_{b['id']}_Ladder", coll, root)
    for i, x in enumerate((-L / 4, -L / 4 + 1.4)):
        instance(SHARED["condenser"], f"HGR_{b['id']}_Condenser", coll, root, (x, -D / 2 - 0.5, 0.0), math.pi)
    instance(SHARED["split"], f"HGR_{b['id']}_Split", coll, root, (-L / 2 + 6.0, -D / 2, 2.6), math.pi)
    for x in [-L / 2 + 4.0 + 15.0 * i for i in range(int((L - 8) // 15) + 1)]:
        instance(SHARED["wall_light"], f"HGR_{b['id']}_WallLight", coll, root, (x, -D / 2, 3.6), math.pi)
    for y in (-D / 3, D / 3):
        instance(SHARED["wall_light"], f"HGR_{b['id']}_WallLight", coll, root, (-L / 2, y, 3.6), math.pi / 2)

    if b["open"]:
        interior(b, coll, root, Lr, Dr, he, hr, mp, ridge_x, tp)
    return he


def interior(b, coll, root, Lr, Dr, he, hr, mp, ridge_x, tp):
    """Open hangars: epoxy floor, trusses on columns (instanced), high-bay lights."""
    f = Mesh(["epoxy"])
    x0, y0 = mp(-Lr / 2 + 0.2, -Dr / 2 + 0.2)
    x1, y1 = mp(Lr / 2 - 0.2, Dr / 2 - 0.2)
    f.face([(min(x0, x1), min(y0, y1), 0.02), (max(x0, x1), min(y0, y1), 0.02), (max(x0, x1), max(y0, y1), 0.02), (min(x0, x1), max(y0, y1), 0.02)], "epoxy")
    f.build(f"HGR_{b['id']}_Floor", coll, root)
    t = Mesh(["steel", "lamp"])
    zb = he - 1.4
    half = Dr / 2 - 0.25
    top = lambda v: hr - abs(v) * tp - 0.3
    # truss in the canonical frame (u = 0 plane), mapped later by the instance rotation
    tube(t, (0, -half, zb), (0, half, zb), 0.14, "steel")
    tube(t, (0, -half, top(-half)), (0, 0, top(0)), 0.16, "steel")
    tube(t, (0, 0, top(0)), (0, half, top(half)), 0.16, "steel")
    nseg = max(4, int(half * 2 // 1.8))
    for i in range(nseg + 1):
        v = -half + 2 * half * i / nseg
        tube(t, (0, v, zb), (0, v, top(v)), 0.08, "steel")
        if i < nseg:
            v2 = -half + 2 * half * (i + 1) / nseg
            a, c = ((v, zb), (v2, top(v2))) if v < 0 else ((v, top(v)), (v2, zb))
            tube(t, (0, a[0], a[1]), (0, c[0], c[1]), 0.07, "steel")
    # columns at the truss ends, none in the door opening (a door beam spans it)
    for s in ((-1,) if ridge_x else (-1, 1)):
        tube(t, (0, s * half, 0.0), (0, s * half, top(s * half)), 0.3, "steel")
    for v in (-half / 2, half / 2):
        t.prism(octagon(0.25, cx=0.0, cy=v), zb - 0.5, zb - 0.45, "lamp", top=False)
    tob = t.build("tmp_truss", coll, None)
    tme = tob.data
    tme.name = f"TRUSS_{b['id']}"
    bpy.data.objects.remove(tob)
    rz = 0.0 if ridge_x else math.pi / 2
    u = -Lr / 2 + 3.0
    while u <= Lr / 2 - 2.0:
        x, y = mp(u, 0.0)
        instance(tme, f"HGR_{b['id']}_Truss", coll, root, (x, y, 0.0), rz)
        u += 6.0


def roof_tint(b, base):
    """How much to tint the roof's material to the orthophoto's colour of this
    roof (2025, ebaw_vl.py): a multiplier per channel, the game's scenery shader
    applies it (instance tint)."""
    rgb = b.get("roof_rgb")
    if not rgb:
        return [1.0, 1.0, 1.0]
    lin = [c ** 2.2 for c in rgb]
    return [round(min(max(c / max(bc, 1e-3), 0.3), 3.0), 3) for c, bc in zip(lin, base)]


def flat_building(b, coll, root, clad, kind):
    """Fire station, shed, office, T-hangar row: flat (or mono-pitch) roof behind a parapet."""
    L, D = b["L"], b["D"]
    he = {"fire": 7.5, "office": 7.6, "t_row": 4.4}.get(kind, 4.5 if L * D < 350 else 5.5)
    if b.get("measured"):
        he = max(3.0, b["measured"]["eave"] - 0.4)   # (the parapet stands above the roof)
    w = Mesh(["wall", "trim", "red"])
    MATS["wall"] = MATS["render"] if kind in ("office", "fire") else MATS[clad]
    MATS["red"] = MATS["red_band"]
    out = [(-L / 2, -D / 2), (L / 2, -D / 2), (L / 2, D / 2), (-L / 2, D / 2)]
    doors, wins = [], []
    win = window_mesh(f"WIN_Flat_{b['id']}", 1.5, 1.2, 2, 1)
    for i in range(4):
        p0, p1 = out[i], out[(i + 1) % 4]
        ln = math.dist(p0, p1)
        opens = []
        if i == 2:
            if kind == "fire":
                bw, nb = 4.6, max(2, min(4, int((ln - 6) // 5.6)))
                x0 = (ln - (nb * bw + (nb - 1) * 1.0)) / 2
                for k in range(nb):
                    opens.append((x0 + k * (bw + 1.0), x0 + k * (bw + 1.0) + bw, 0.0, 4.8))
                    doors.append(("door_red", ln, p0, p1, x0 + k * (bw + 1.0), bw, 4.8))
            elif kind == "t_row":
                nb = max(1, int((ln - 1.0) // 12.0))
                bw = (ln - 1.0) / nb - 0.6
                for k in range(nb):
                    xa = 0.8 + k * (bw + 0.6)
                    opens.append((xa, xa + bw, 0.0, 3.4))
                    doors.append(("door_grey", ln, p0, p1, xa, bw, 3.4))
            elif kind == "shed" and ln > 7:
                bw = min(4.5, ln - 3.0)
                opens.append(((ln - bw) / 2, (ln + bw) / 2, 0.0, min(4.0, he - 0.8)))
                doors.append(("door_grey", ln, p0, p1, (ln - bw) / 2, bw, min(4.0, he - 0.8)))
        if kind in ("office", "fire") and i in (0, 1, 3) and ln > 6:
            for zr in ((1.0, 2.2), (4.6, 5.8)) if he > 7 else ((1.0, 2.2),):
                for x0, k in bays(1.2, ln - 1.2, 3):
                    opens.append((x0, x0 + k * 1.5, *zr))
                    for j in range(k):
                        wins.append((p0, p1, x0 + (j + 1) * 1.5, zr[0]))
        bands = [(0.0, 0.4)] + ([(he - 0.6, he)] if kind == "fire" else [])
        facade(w, p0, p1, 0.0, he + 0.6, opens, bands, wall_key="wall", band_key="trim")
    if kind == "fire":
        for f in range(len(w.f)):
            zs = [w.v[i][2] for i in w.f[f]]
            if min(zs) >= he - 0.61:
                w.m[f] = w.slots.index("red")
    wob = w.build(f"HGR_{b['id']}_Walls", coll, root)
    sd = wob.modifiers.new("Wall", "SOLIDIFY")
    sd.thickness, sd.offset, sd.use_even_offset, sd.use_rim, sd.material_offset_rim = 0.25, -1.0, True, True, 1
    add_bevel(wob, 0.02)
    for p0, p1, x, za in wins:
        d = (Vector((*p1, 0)) - Vector((*p0, 0))).normalized()
        at = Vector((*p0, 0)) + d * x + Vector((d.y, -d.x, 0)) * -0.1
        instance(win, f"HGR_{b['id']}_Win", coll, root, (at.x, at.y, za), math.atan2(d.y, d.x) + math.pi)
    r = Mesh(["gravel", "galv"])
    i = 0.25
    if kind == "t_row":  # mono-pitch, falling to the back, steel sheet
        r.slots[0] = "roof_x"
        r.face([(-L / 2 - 0.3, -D / 2 - 0.3, he - 0.5), (L / 2 + 0.3, -D / 2 - 0.3, he - 0.5), (L / 2 + 0.3, D / 2 + 0.3, he + 0.6), (-L / 2 - 0.3, D / 2 + 0.3, he + 0.6)], "roof_x")
    else:
        r.face([(-L / 2 + i, -D / 2 + i, he), (L / 2 - i, -D / 2 + i, he), (L / 2 - i, D / 2 - i, he), (-L / 2 + i, D / 2 - i, he)], "gravel")
    rf = r.build(f"HGR_{b['id']}_Roof", coll, root)
    rf["tint"] = roof_tint(b, (0.33, 0.34, 0.34) if kind == "t_row" else (0.18, 0.17, 0.16))
    for key, ln, p0, p1, x0, bw, h in doors:
        d = (Vector((*p1, 0)) - Vector((*p0, 0))).normalized()
        dm = Mesh([key, "glass", "hazard"])
        dm.box(0.0, bw, -0.1, 0.0, 0.0, h, key)
        if key == "door_red":
            dm.face([(0.3, 0.001, 3.2), (0.3, 0.001, 3.7), (bw - 0.3, 0.001, 3.7), (bw - 0.3, 0.001, 3.2)], "glass")
        dob = dm.build(f"HGR_{b['id']}_Door", coll, root)
        at = Vector((*p0, 0)) + d * (x0 + bw) + Vector((d.y, -d.x, 0)) * -0.15
        dob.location = (at.x, at.y, 0.0)
        dob.rotation_euler = (0.0, 0.0, math.atan2(d.y, d.x) + math.pi)
    for x in [-L / 2 + 3.0 + 12.0 * k for k in range(int((L - 6) // 12) + 1)]:
        instance(SHARED["wall_light"], f"HGR_{b['id']}_WallLight", coll, root, (x, D / 2, min(he - 0.5, 5.2)))
    if kind != "t_row":
        instance(SHARED["condenser"], f"HGR_{b['id']}_Condenser", coll, root, (0.0, -D / 2 - 0.5, 0.0), math.pi)
    return he


# ---------------------------------------------------------------- build

def build():
    materials()
    coll = collection(COLL)
    shared_parts(coll)
    built = []
    for b in LAYOUT["buildings"]:
        if b["kind"] == "rotunda":   # part of the terminal (02_terminal_atc.py)
            continue
        bb = dict(b)
        if bb["kind"] == "t_row" and bb["L"] < bb["D"]:
            # doors along the long side: turn the frame a quarter
            bb["rot"] -= math.pi / 2
            bb["L"], bb["D"] = bb["D"], bb["L"]
        root = bpy.data.objects.new(f"HGR_{b['id']}", None)
        root.empty_display_type = "ARROWS"
        root.empty_display_size = 6.0
        root.location = (bb["x"], bb["y"], 0.0)
        root.rotation_euler = (0.0, 0.0, bb["rot"])
        root["osm_way"] = b["id"]
        root["kind"] = b["kind"]
        coll.objects.link(root)
        clad = "clad_" + (["OffWhite"] if b["kind"] == "museum" else [p[0] for p in PALETTE])[b["id"] % (1 if b["kind"] == "museum" else len(PALETTE))]
        if bb["kind"] in ("hangar", "museum"):
            label = "STAMPE & VERTONGEN MUSEUM" if b["kind"] == "museum" else (b["name"].upper() or None)
            he = gable_building(bb, coll, root, clad, label)
        else:
            he = flat_building(bb, coll, root, clad, bb["kind"])
        root["eave_m"] = round(he, 2)
        built.append(root)
    return coll, built


def shots():
    def find(osm):
        return next(b for b in LAYOUT["buildings"] if b["id"] == osm)

    def front(osm, dist, h, lens, side=1.0):
        b = find(osm)
        n = Vector((-math.sin(b["rot"]), math.cos(b["rot"]), 0.0))
        a = Vector((math.cos(b["rot"]), math.sin(b["rot"]), 0.0))
        c = Vector((b["x"], b["y"], 0.0))
        eye = c + n * (b["D"] / 2 + dist) + a * side * dist * 0.5 + Vector((0, 0, h))
        return eye.to_tuple(), (c + Vector((0, 0, 4.0))).to_tuple(), lens
    return {
        "south": ((-250.0, -180.0, 140.0), (-480.0, -420.0, 0.0), 30.0),
        "open": front(445909590, 55.0, 3.0, 24.0, 0.6),
        "museum": front(44802700, 45.0, 2.5, 26.0, -0.8),
        "fire": front(44961519, 40.0, 2.0, 26.0, 0.5),
        "north": ((-60.0, 200.0, 70.0), (-150.0, 380.0, 0.0), 30.0),
    }


def main():
    opt = args(render="/tmp/ebaw_step3.png", shot="south")
    if "01_Ground_Layout" not in bpy.data.collections:
        saved = sys.argv
        sys.argv = [saved[0], "--", "--render", "none"]
        runpy.run_path(str(HERE / "01_ground_layout.py"), run_name="__main__")
        sys.argv = saved
    coll, built = build()
    objs = [o for o in coll.all_objects if o.type == "MESH"]
    inst = sum(1 for o in objs if o.data.users > 1)
    print(f"03_Hangars: {len(built)} buildings, {len(objs)} mesh objects ({inst} instances of shared meshes)")
    prev = bpy.data.collections.get("99_Preview") or collection("99_Preview")
    loc, tgt, lens = shots().get(opt["shot"], shots()["south"])
    cam = camera("CheckCam_Hangars", prev, loc, tgt, lens)
    if bpy.app.background:
        check_render(opt, cam)
    else:
        bpy.context.scene.camera = cam


main()
