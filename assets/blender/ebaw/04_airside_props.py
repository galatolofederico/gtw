"""EBAW (Antwerp International Airport) - step 4: airside props and ground equipment.

Builds collection 04_Airside_Props:

  windsocks      the three OSM windsocks: hinged mast, ICAO sock (3.6 m long,
                 0.9 m mouth, five orange and white bands), red obstacle light.
                 The socks are driven by the empty WIND_Control: its custom
                 properties wind_from_deg and wind_kt turn and droop all of
                 them (drivers with simple expressions, no scripts needed).
  signs          at every runway holding position: the mandatory sign (white
                 "11-29" or "29-11" on red, left of the line) and the location
                 sign outboard (yellow taxiway letter on black), on frangible
                 legs, the legends lit.
  fuel           the fuel station (OSM aeroway=fuel): two horizontal tanks in a
                 concrete bund, JET A-1 and AVGAS 100LL, a dispenser island, and
                 a refueller for each grade parked beside it.
  GSE            baggage tugs and dolly trains by the terminal, ground power
                 units, wheel cones and fire extinguisher trolleys at the stands,
                 crowd barriers along the terminal front.
  fence          the perimeter fence along the aerodrome boundary (kept clear of
                 the buildings): chain-link fabric, posts every 3 m, three strands
                 of barbed wire on top.
  navaids        the radio aids where OSM has them (airmark=beacon), sized from
                 the Flemish orthophoto and LiDAR (ebaw_vl.py's sources):
                 - DVOR/DME "ANT": a 29 m sixteen-sided counterpoise 7.3 m up on
                   steel legs, 48 antennas round a 13.5 m circle, the carrier and
                   DME antennas in the middle, the equipment shelter beneath;
                 - ILS 29 localizer "IAD" past the far end of runway 11: twenty
                   log-periodic antennas 2.4 m apart (48 m) across the runway's
                   line, about 2 m high;
                 - its glide slope (with the DME) 120 m south of runway 29's
                   touchdown zone: a 14 m lattice mast with three antennas facing
                   the approach, on a concrete pad with its shelter (the LiDAR
                   misses so thin a mast: the height is the usual one).

Everything repeated is a linked duplicate of one mesh: vehicles, cones,
posts, barriers, signs' legs.

    blender -b assets/blender/ebaw/ebaw.blend --python assets/blender/ebaw/04_airside_props.py -- \
        [--render /tmp/ebaw_step4.png] [--shot stands|fuel|sign|windsock|fence] [--samples 48] \
        [--res 1920x1080] [--save assets/blender/ebaw/ebaw.blend]

Positions come from ebaw_layout.json (ebaw_osm.py; OpenStreetMap
contributors, ODbL). Equipment is generic, unbranded: no operator's livery.
The windsock and sign sizes follow ICAO Annex 14 (5.1.1, 5.4); vehicle sizes
are typical of the class, not of a given model.
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
from ebaw_lib import (MATS, Graph, Mesh, add_bevel, args, camera, check_render, collection,  # noqa: E402
                      instance, octagon, rail, text_object)

LAYOUT = json.loads((HERE / "ebaw_layout.json").read_text())
COLL = "04_Airside_Props"
RWY_U = Vector((math.sin(math.radians(LAYOUT["runway"]["hdg_11"])), math.cos(math.radians(LAYOUT["runway"]["hdg_11"])), 0.0))
T = LAYOUT["terminal"]
TO, TA, TC = Vector((*T["origin"], 0)), Vector((*T["a_axis"], 0)), Vector((*T["c_axis"], 0))


def W(a, c):
    """Terminal frame (a along the facade, c towards the apron) -> EBAW frame."""
    p = TO + TA * a + TC * c
    return (p.x, p.y)


def bearing_rot(h):
    """Rotation about Z that turns local +Y to the true bearing h (degrees)."""
    return -math.radians(h)


TA_BEARING = math.degrees(math.atan2(TA.x, TA.y)) % 360


# ---------------------------------------------------------------- materials

def mat_paint(name, rgb, rough=0.35, metallic=0.0):
    """Vehicle or equipment paint: slight orange peel, dust and scuffs low down."""
    g = Graph(name)
    sep = g.node("ShaderNodeSeparateXYZ")
    g.put(sep.inputs[0], g.pos)
    dust = g.math("MULTIPLY", g.math("SUBTRACT", 1.0, g.smooth(sep.outputs["Z"], 0.1, 0.9)), g.smooth(g.noise(g.pos, 3.0, 5.0), 0.4, 0.7))
    scuff = g.smooth(g.noise(g.pos, 12.0, 6.0), 0.7, 0.8)
    c = g.mix(g.math("MULTIPLY", dust, 0.6), rgb, (0.16, 0.14, 0.11))
    c = g.mix(g.math("MULTIPLY", scuff, 0.4), c, tuple(v * 0.6 for v in rgb))
    g.bsdf.inputs["Metallic"].default_value = metallic
    g.bump(g.noise(g.pos, 60.0, 3.0), 0.03, 0.001)
    return g.finish(c, g.lerp(dust, rough, 0.8), 0.5)


def mat_emit(name, rgb, strength, rough=0.4):
    g = Graph(name)
    g.bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
    g.bsdf.inputs["Emission Strength"].default_value = strength
    return g.finish(rgb, rough, 0.5)


def mat_sock():
    """Windsock fabric: five bands, orange at the mouth, white between (ICAO)."""
    g = Graph("M_Windsock_Fabric")
    sep = g.node("ShaderNodeSeparateXYZ")
    g.put(sep.inputs[0], g.pos)
    band = g.math("MODULO", g.math("FLOOR", g.math("DIVIDE", sep.outputs["Y"], 0.72)), 2.0)
    c = g.mix(band, (0.85, 0.18, 0.01), (0.75, 0.74, 0.70))
    fade = g.smooth(g.noise(g.pos, 2.0, 4.0), 0.5, 0.7)
    c = g.mix(g.math("MULTIPLY", fade, 0.25), c, (0.45, 0.40, 0.33))
    g.bump(g.noise(vmul3(g, (1.0, 12.0, 1.0)), 1.0, 3.0), 0.2, 0.004)
    return g.finish(c, 0.8, 0.3)


def vmul3(g, scale):
    n = g.node("ShaderNodeVectorMath", operation="MULTIPLY")
    g.put(n.inputs[0], g.pos)
    n.inputs[1].default_value = scale
    return n.outputs[0]


def mat_cone():
    """Traffic cone: orange with retro-reflective white bands."""
    g = Graph("M_Cone")
    sep = g.node("ShaderNodeSeparateXYZ")
    g.put(sep.inputs[0], g.pos)
    z = sep.outputs["Z"]
    band = g.math("MAXIMUM", g.math("MULTIPLY", g.math("GREATER_THAN", z, 0.38), g.math("LESS_THAN", z, 0.48)),
                  g.math("MULTIPLY", g.math("GREATER_THAN", z, 0.55), g.math("LESS_THAN", z, 0.62)))
    c = g.mix(band, (0.9, 0.2, 0.01), (0.8, 0.8, 0.78))
    return g.finish(c, g.lerp(band, 0.55, 0.3), 0.5)


def mat_chainlink():
    """Galvanised chain-link fabric: a 60 mm diamond mesh as an alpha mask, from the UVs (u = metres along the fence, v = height)."""
    g = Graph("M_ChainLink")
    uv = g.node("ShaderNodeTexCoord").outputs["UV"]
    sep = g.node("ShaderNodeSeparateXYZ")
    g.put(sep.inputs[0], uv)
    u, v = sep.outputs["X"], sep.outputs["Y"]

    def wires(s):
        f = g.math("SUBTRACT", g.math("FRACT", g.math("DIVIDE", s, 0.085)), 0.5)
        return g.math("SUBTRACT", 1.0, g.smooth(g.math("ABSOLUTE", f), 0.04, 0.09))
    mask = g.math("MAXIMUM", wires(g.math("ADD", u, v)), wires(g.math("SUBTRACT", u, v)))
    g.put(g.bsdf.inputs["Alpha"], mask)
    g.bsdf.inputs["Metallic"].default_value = 0.85
    try:
        g.mat.surface_render_method = "DITHERED"
    except (AttributeError, TypeError):
        pass
    return g.finish((0.42, 0.43, 0.43), 0.45, 0.5)


def mat_plain(name, rgb, rough=0.6, metallic=0.0):
    g = Graph(name)
    g.bsdf.inputs["Metallic"].default_value = metallic
    fine = g.noise(g.pos, 10.0, 4.0)
    c = g.mix(g.math("MULTIPLY", fine, 0.2), rgb, tuple(v * 0.8 for v in rgb))
    g.bump(fine, 0.05, 0.002)
    return g.finish(c, rough, 0.5)


def materials():
    MATS.update({
        "yellow_paint": mat_paint("M_Paint_Safety_Yellow", (0.78, 0.55, 0.02)),
        "white_paint": mat_paint("M_Paint_Vehicle_White", (0.70, 0.70, 0.68)),
        "blue_paint": mat_paint("M_Paint_Dolly_Blue", (0.04, 0.10, 0.25)),
        "red_paint": mat_paint("M_Paint_Red", (0.55, 0.02, 0.015)),
        "alu": mat_paint("M_Aluminium", (0.55, 0.56, 0.57), 0.35, 0.9),
        "dark": mat_plain("M_Metal_Dark", (0.035, 0.037, 0.04), 0.45, 0.7),
        "rubber": mat_plain("M_Rubber_Tyre", (0.025, 0.025, 0.025), 0.85),
        "galv": mat_plain("M_Metal_Galvanised", (0.45, 0.46, 0.47), 0.4, 0.9),
        "concrete": mat_plain("M_Concrete_Bund", (0.30, 0.295, 0.28), 0.85),
        "glass_dark": mat_plain("M_Glass_Vehicle", (0.02, 0.025, 0.03), 0.05),
        "sock": mat_sock(),
        "cone": mat_cone(),
        "chainlink": mat_chainlink(),
        "sign_red": mat_emit("M_Sign_Red_Lit", (0.55, 0.02, 0.015), 0.6),
        "sign_black": mat_plain("M_Sign_Black", (0.012, 0.012, 0.012), 0.5),
        "sign_yellow": mat_emit("M_Sign_Yellow_Lit", (0.85, 0.6, 0.0), 0.8),
        "legend_white": mat_emit("M_Legend_White_Lit", (0.85, 0.85, 0.82), 1.5),
        "legend_yellow": mat_emit("M_Legend_Yellow_Lit", (0.9, 0.62, 0.0), 1.5),
        "legend_black": mat_plain("M_Legend_Black", (0.01, 0.01, 0.01), 0.5),
        "obstacle": mat_emit("M_Obstacle_Red", (1.0, 0.02, 0.01), 40.0),
        "label_jet": mat_plain("M_Label_JetA1_Black", (0.01, 0.01, 0.01), 0.4),
        "label_avgas": mat_plain("M_Label_Avgas_Red", (0.55, 0.02, 0.015), 0.4),
        "lamp": mat_emit("M_Lamp_Amber", (1.0, 0.45, 0.0), 6.0),
        # the orthophoto's rusty brown counterpoise, a little less saturated
        "deck": mat_plain("M_DVOR_Counterpoise", (0.30, 0.21, 0.16), 0.7, 0.3),
        "radome": mat_plain("M_Radome_White", (0.72, 0.72, 0.70), 0.45),
        "shelter": mat_plain("M_Shelter_Beige", (0.50, 0.47, 0.40), 0.7),
    })
    MATS["letters"] = MATS["legend_black"]


# ---------------------------------------------------------------- mesh helpers

def cyl(m, axis, a0, a1, c1, c2, r, key, n=12, caps=True):
    """A cylinder along axis 'x', 'y' or 'z' from a0 to a1, centred at (c1, c2) on the other two axes."""
    ring = [(c1 + r * math.cos(2 * math.pi * i / n), c2 + r * math.sin(2 * math.pi * i / n)) for i in range(n)]

    def P(a, p):
        return {"x": (a, p[0], p[1]), "y": (p[1], a, p[0]), "z": (p[0], p[1], a)}[axis]
    for i in range(n):
        j = (i + 1) % n
        m.face([P(a0, ring[i]), P(a0, ring[j]), P(a1, ring[j]), P(a1, ring[i])], key)
    if caps:
        m.face([P(a1, p) for p in ring], key)
        m.face([P(a0, p) for p in reversed(ring)], key)


def wheel(m, x, y, r, w):
    """A tyre with a hub, axle along X."""
    cyl(m, "x", x - w / 2, x + w / 2, y, r, r, "rubber", 14)
    cyl(m, "x", x - w / 2 - 0.005, x + w / 2 + 0.005, y, r, r * 0.55, "galv", 10, caps=True)


def kept(mesh, name, coll):
    """Build a mesh to be instanced: returns its data, no object of its own."""
    ob = mesh.build(name, coll, None)
    me = ob.data
    if any(md.type == "BEVEL" for md in ob.modifiers):
        pass
    bpy.data.objects.remove(ob)
    return me


# ---------------------------------------------------------------- equipment (local: +Y forward, X right, Z up)

def tug_mesh(coll):
    m = Mesh(["yellow_paint", "dark", "rubber", "galv", "glass_dark", "lamp"])
    m.box(-0.75, 0.75, -1.4, 1.4, 0.35, 0.85, "yellow_paint")
    m.box(-0.7, 0.7, 0.5, 1.45, 0.85, 1.2, "yellow_paint")          # bonnet
    m.box(-0.45, 0.45, -0.6, 0.0, 0.85, 1.25, "dark")              # seat
    m.box(-0.45, 0.45, -0.75, -0.6, 1.25, 1.75, "dark")            # backrest
    for x in (-0.7, 0.7):
        for y in (-1.25, 0.45):
            m.box(x - 0.03, x + 0.03, y - 0.03, y + 0.03, 0.85, 2.05, "dark")
    m.box(-0.78, 0.78, -1.35, 0.55, 2.05, 2.12, "yellow_paint")    # roof
    m.box(-0.7, 0.7, 0.42, 0.46, 1.25, 2.0, "glass_dark")          # windscreen
    m.box(-0.12, 0.12, -1.6, -1.4, 0.4, 0.5, "galv")               # tow hitch
    m.prism(octagon(0.08, cx=0.0, cy=-0.9), 2.12, 2.22, "lamp")    # beacon
    for x in (-0.68, 0.68):
        for y in (-0.9, 0.95):
            wheel(m, x, y, 0.3, 0.22)
    return kept(m, "GSE_Tug", coll)


def dolly_mesh(coll):
    m = Mesh(["alu", "blue_paint", "rubber", "galv", "dark"])
    m.box(-0.75, 0.75, -1.5, 1.5, 0.45, 0.55, "alu")                 # bed
    m.box(-0.75, 0.75, -1.5, -1.45, 0.55, 1.9, "blue_paint")         # end walls
    m.box(-0.75, 0.75, 1.45, 1.5, 0.55, 1.9, "blue_paint")
    m.box(-0.78, 0.78, -1.53, 1.53, 1.9, 1.98, "blue_paint")         # roof
    for x in (-0.75, 0.75):
        for y in (-1.5, 0.0, 1.5):
            m.box(x - 0.025, x + 0.025, y - 0.025, y + 0.025, 0.55, 1.9, "galv")
    m.box(-0.74, -0.73, -1.45, 1.45, 1.1, 1.85, "dark")              # side curtains, rolled half down
    m.box(0.73, 0.74, -1.45, 1.45, 1.1, 1.85, "dark")
    m.box(-0.04, 0.04, 1.5, 2.6, 0.35, 0.42, "galv")                 # drawbar
    for x in (-0.6, 0.6):
        for y in (-1.1, 1.1):
            wheel(m, x, y, 0.2, 0.14)
    return kept(m, "GSE_Dolly", coll)


def gpu_mesh(coll):
    m = Mesh(["white_paint", "yellow_paint", "rubber", "galv", "dark"])
    m.box(-0.6, 0.6, -1.1, 1.1, 0.4, 1.55, "white_paint")
    m.box(-0.61, 0.61, -1.11, 1.11, 0.4, 0.55, "yellow_paint")
    for y in (-0.8, -0.5, -0.2):
        m.box(0.6, 0.62, y, y + 0.2, 0.8, 1.4, "dark")                # louvres
    cyl(m, "x", -0.65, -0.6, 0.4, 1.1, 0.3, "dark", 12)                # cable reel
    m.box(-0.04, 0.04, 1.1, 2.0, 0.35, 0.42, "galv")
    for x in (-0.62, 0.62):
        wheel(m, x, -0.3, 0.3, 0.18)
    return kept(m, "GSE_GPU", coll)


def refueller_mesh(coll, grade):
    """A rigid refueller: cab forward, elliptical tank, hose reel at the back, grade label."""
    lab = "label_jet" if grade == "JET A-1" else "label_avgas"
    m = Mesh(["white_paint", "dark", "rubber", "galv", "glass_dark", lab, "lamp"])
    m.box(-1.25, 1.25, 2.6, 4.4, 0.9, 3.0, "white_paint")             # cab
    m.box(-1.2, 1.2, 4.41, 4.43, 1.9, 2.8, "glass_dark")              # windscreen
    for x in (-1.26, 1.25):
        m.box(x, x + 0.01, 3.4, 4.3, 1.9, 2.7, "glass_dark")           # side windows
    m.box(-1.0, 1.0, -4.0, 2.6, 0.6, 1.0, "dark")                    # chassis
    # elliptical tank: an octagon section stretched, along Y
    sec = [(1.2 * math.cos(math.pi / 8 + i * math.pi / 4), 1.6 + 0.8 * math.sin(math.pi / 8 + i * math.pi / 4)) for i in range(8)]
    for i in range(8):
        j = (i + 1) % 8
        m.face([(sec[i][0], -3.6, sec[i][1]), (sec[j][0], -3.6, sec[j][1]), (sec[j][0], 2.4, sec[j][1]), (sec[i][0], 2.4, sec[i][1])][::-1], "white_paint")
    m.face([(x, 2.4, z) for x, z in sec][::-1], "white_paint")
    m.face([(x, -3.6, z) for x, z in sec], "white_paint")
    m.box(-1.0, 1.0, -0.6, 0.6, 1.0, 1.15, lab)                       # grade band on the side, under the label
    m.box(-0.8, 0.8, -4.3, -3.6, 0.9, 2.0, "galv")                    # rear cabinet, hose reel
    cyl(m, "x", -0.7, 0.7, -3.95, 1.5, 0.35, "dark", 12)
    m.prism(octagon(0.1, cx=0.0, cy=3.5), 3.0, 3.12, "lamp")
    for x in (-1.05, 1.05):
        for y in (3.4, -1.8, -3.0):
            wheel(m, x, y, 0.5, 0.35)
    return kept(m, f"GSE_Refueller_{grade.replace(' ', '_').replace('-', '')}", coll)


def cone_mesh(coll):
    m = Mesh(["cone", "dark"])
    m.box(-0.19, 0.19, -0.19, 0.19, 0.0, 0.03, "dark")
    lo, hi = octagon(0.14), octagon(0.025)
    m.frustum(lo, hi, 0.03, 0.75, "cone")
    m.face([(x, y, 0.75) for x, y in hi], "cone")
    return kept(m, "PROP_Cone", coll)


def extinguisher_mesh(coll):
    m = Mesh(["red_paint", "rubber", "galv", "dark"])
    cyl(m, "z", 0.25, 1.15, 0.0, 0.0, 0.22, "red_paint", 12)
    m.prism(octagon(0.05), 1.15, 1.25, "dark")
    m.box(-0.3, 0.3, -0.32, -0.28, 0.2, 1.1, "galv")                 # frame
    m.box(-0.3, 0.3, -0.33, -0.27, 1.1, 1.14, "galv")
    for x in (-0.3, 0.3):
        wheel(m, x, -0.1, 0.2, 0.08)
    return kept(m, "PROP_Extinguisher", coll)


def barrier_mesh(coll):
    """A crowd-control barrier, 2.3 m, feet sticking out."""
    m = Mesh(["galv"])
    for z in (0.15, 1.08):
        m.box(-1.15, 1.15, -0.02, 0.02, z, z + 0.04, "galv")
    for x in (-1.15, 1.15):
        m.box(x - 0.02, x + 0.02, -0.02, 0.02, 0.1, 1.12, "galv")
        m.box(x - 0.02, x + 0.02, -0.35, 0.35, 0.0, 0.04, "galv")
    for k in range(13):
        x = -1.05 + k * 0.175
        m.box(x - 0.008, x + 0.008, -0.008, 0.008, 0.19, 1.08, "galv")
    return kept(m, "PROP_CrowdBarrier", coll)


def post_mesh(coll):
    m = Mesh(["galv", "dark"])
    cyl(m, "z", 0.0, 2.45, 0.0, 0.0, 0.04, "galv", 8)
    m.box(-0.02, 0.02, -0.02, 0.35, 2.4, 2.45, "galv")                # outrigger for the barbed wire
    return kept(m, "FENCE_Post", coll)


# ---------------------------------------------------------------- windsock

def windsocks(coll, ctl):
    mast = Mesh(["galv", "concrete", "obstacle", "dark"])
    mast.prism(octagon(0.5), 0.0, 0.15, "concrete")
    cyl(mast, "z", 0.15, 6.3, 0.0, 0.0, 0.07, "galv", 10)
    mast.box(-0.12, 0.12, -0.12, 0.12, 1.0, 1.25, "dark")             # tilt hinge
    mast.prism(octagon(0.1), 6.3, 6.42, "dark")
    mast.prism(octagon(0.07), 6.42, 6.55, "obstacle")
    mast_me = kept(mast, "WINDSOCK_Mast", coll)

    sock = Mesh(["sock", "galv"])
    n, L, r0, r1 = 16, 3.6, 0.45, 0.15
    rings = 10
    pts = []
    for k in range(rings + 1):
        y = L * k / rings
        r = r0 + (r1 - r0) * k / rings
        pts.append([(r * math.cos(2 * math.pi * i / n), y, r * math.sin(2 * math.pi * i / n)) for i in range(n)])
    for k in range(rings):
        for i in range(n):
            j = (i + 1) % n
            sock.face([pts[k][i], pts[k][j], pts[k + 1][j], pts[k + 1][i]], "sock")
    for i in range(n):  # mouth hoop
        j = (i + 1) % n
        a, b = Vector(pts[0][i]), Vector(pts[0][j])
        sock.face([(a * 1.03).to_tuple(), (b * 1.03).to_tuple(), (b * 1.03 + Vector((0, 0.04, 0))).to_tuple(), (a * 1.03 + Vector((0, 0.04, 0))).to_tuple()], "galv")
    sock.box(-0.02, 0.02, -0.25, 0.0, -0.02, 0.02, "galv")           # swivel arm
    sock_ob = sock.build("tmp_sock", coll, None)
    sock_me = sock_ob.data
    sock_me.name = "WINDSOCK_Sock"
    bpy.data.objects.remove(sock_ob)
    for i, (x, y) in enumerate(LAYOUT["windsocks"]):
        instance(mast_me, f"WINDSOCK_{i + 1}_Mast", coll, None, (x, y, 0.0))
        s = instance(sock_me, f"WINDSOCK_{i + 1}_Sock", coll, None, (x, y, 6.15))
        s.rotation_mode = "XYZ"
        # heading: the sock points downwind; droop: limp below 3 kt, horizontal from 15 kt (ICAO)
        for idx, expr in ((2, "-radians(frm + 180.0)"), (0, "-radians(75.0) * max(0.0, min(1.0, (15.0 - kt) / 12.0))")):
            fc = s.driver_add("rotation_euler", idx)
            drv = fc.driver
            drv.type = "SCRIPTED"
            for vn, path in (("frm", '["wind_from_deg"]'), ("kt", '["wind_kt"]')):
                v = drv.variables.new()
                v.name = vn
                v.type = "SINGLE_PROP"
                v.targets[0].id_type = "OBJECT"
                v.targets[0].id = ctl
                v.targets[0].data_path = path
            drv.expression = expr


# ---------------------------------------------------------------- navaids

def beacon(kind):
    """The OSM beacon of a kind ('DVOR', 'localizer', 'glideslope'), or None."""
    for n in LAYOUT["navaids"]:
        t = n["tags"]
        if (kind == "DVOR" and "DVOR" in t.get("beacon:type", "")) or t.get(kind) == "yes":
            return Vector((n["x"], n["y"], 0.0))
    return None


def to_local(v, h):
    """A world (east, north) offset in the frame bearing_rot(h) turns to the world."""
    a = math.radians(h)
    return (v[0] * math.cos(a) - v[1] * math.sin(a), v[0] * math.sin(a) + v[1] * math.cos(a))


def dvor(coll, at):
    m = Mesh(["galv", "deck", "radome", "white_paint", "shelter", "dark", "obstacle", "concrete"])
    n, R, z = 16, 14.5, 7.3
    ring = lambda r, k=n, o=0.0: [(r * math.cos(o + 2 * math.pi * i / k), r * math.sin(o + 2 * math.pi * i / k)) for i in range(k)]
    m.prism(ring(R), z - 0.15, z, "deck")
    for x, y in ring(13.2) + ring(6.0, 8, math.pi / 8):                   # legs on footings
        m.box(x - 0.6, x + 0.6, y - 0.6, y + 0.6, -0.3, 0.15, "concrete")
        m.box(x - 0.13, x + 0.13, y - 0.13, y + 0.13, 0.15, z - 0.15, "galv")
    for x, y in ring(R - 0.2):                                            # radial beams
        L = math.hypot(x, y)
        ux, uy = x / L, y / L
        q = [(-uy * 0.1, ux * 0.1), (x - uy * 0.1, y + ux * 0.1), (x + uy * 0.1, y - ux * 0.1), (uy * 0.1, -ux * 0.1)]
        m.prism(q[::-1] if (q[1][0] - q[0][0]) * (q[2][1] - q[0][1]) - (q[1][1] - q[0][1]) * (q[2][0] - q[0][0]) < 0 else q, z - 0.45, z - 0.15, "galv")
    rail(m, ring(R - 0.1), z, 1.1)
    for x, y in ring(6.75, 48):                                           # the sideband antennas
        m.box(x - 0.04, x + 0.04, y - 0.04, y + 0.04, z, z + 1.2, "galv")
        m.prism([(x + dx, y + dy) for dx, dy in octagon(0.28)], z + 1.2, z + 1.55, "radome")
    for x, y in ring(2.6, 8):                                             # field monitors
        m.box(x - 0.03, x + 0.03, y - 0.03, y + 0.03, z, z + 0.7, "white_paint")
    m.prism(octagon(0.6), z, z + 1.8, "radome")                           # carrier antenna
    m.prism(octagon(0.14), z + 1.8, z + 3.2, "radome")                    # DME
    m.prism(octagon(0.09), z + 3.2, z + 3.35, "obstacle")
    m.box(-3.0, 3.0, -1.25, 1.25, 0.0, 2.8, "shelter")                    # equipment shelter
    m.box(-0.45, 0.45, -1.27, -1.25, 0.0, 2.1, "dark")
    m.box(9.5, 10.5, -0.4, 0.4, 0.0, z, "galv", skip=("top",))           # ladder cage to the deck
    me = m.build("NAV_DVOR_ANT", coll, None)
    me.location = (at.x, at.y, 0.0)
    return me


def localizer(coll, at, h):
    m = Mesh(["galv", "white_paint", "concrete"])
    m.box(-24.6, 24.6, -0.5, 0.5, -0.2, 0.1, "concrete")                   # the footing strip
    for i in range(20):
        x = (i - 9.5) * 2.4
        m.box(x - 0.05, x + 0.05, -0.05, 0.05, 0.1, 1.95, "galv")
        m.box(x - 0.03, x + 0.03, -0.5, 2.0, 1.95, 2.03, "galv")           # boom toward the approach
        for k in range(8):                                                 # dipoles, longest at the back
            y = -0.4 + k * 0.32
            hl = 0.85 - k * 0.07
            m.box(x - hl, x + hl, y - 0.015, y + 0.015, 1.97, 2.01, "white_paint")
    me = m.build("NAV_LOC_IAD", coll, None)
    me.location = (at.x, at.y, 0.0)
    me.rotation_euler = (0.0, 0.0, bearing_rot(h))
    return me


def glideslope(coll, at, h, pad_off):
    m = Mesh(["galv", "white_paint", "concrete", "shelter", "dark", "obstacle", "radome"])
    px, py = to_local(pad_off, h)
    m.box(px - 7.5, px + 7.5, py - 7.5, py + 7.5, -0.2, 0.12, "concrete")
    top = 14.0
    legs = [(0.0, 0.45), (-0.4, -0.25), (0.4, -0.25)]
    for x, y in legs:
        m.box(x - 0.05, x + 0.05, y - 0.05, y + 0.05, 0.12, top, "galv")
    for k in range(1, 14):                                                 # rungs
        zz = k * 1.0
        for (x0, y0), (x1, y1) in zip(legs, legs[1:] + legs[:1]):
            L = math.dist((x0, y0), (x1, y1))
            nx, ny = -(y1 - y0) / L * 0.02, (x1 - x0) / L * 0.02
            q = [(x0 - nx, y0 - ny), (x1 - nx, y1 - ny), (x1 + nx, y1 + ny), (x0 + nx, y0 + ny)]
            m.prism(q if (q[1][0] - q[0][0]) * (q[2][1] - q[0][1]) - (q[1][1] - q[0][1]) * (q[2][0] - q[0][0]) > 0 else q[::-1], zz - 0.02, zz + 0.02, "galv")
    for zz in (4.5, 9.0, 13.3):                                            # the three antennas, facing the approach (+Y)
        m.box(-0.08, 0.08, 0.45, 0.9, zz - 0.06, zz + 0.06, "galv")
        m.box(-0.7, 0.7, 0.9, 1.25, zz - 0.35, zz + 0.35, "radome")
    m.prism(octagon(0.13), top, top + 1.3, "radome")                       # DME
    m.prism(octagon(0.09), top + 1.3, top + 1.45, "obstacle")
    sx, sy = px - 3.5, py - 3.0                                            # shelter on the pad
    m.box(sx - 1.5, sx + 1.5, sy - 1.2, sy + 1.2, 0.12, 2.7, "shelter")
    m.box(sx - 0.45, sx + 0.45, sy - 1.22, sy - 1.2, 0.12, 2.2, "dark")
    me = m.build("NAV_GS_29", coll, None)
    me.location = (at.x, at.y, 0.0)
    me.rotation_euler = (0.0, 0.0, bearing_rot(h))
    return me


def navaids(coll):
    h = LAYOUT["runway"]["hdg_11"]          # both ILS aerials radiate toward runway 29's approach
    done = []
    p = beacon("DVOR")
    if p:
        done.append(dvor(coll, p + Vector((0.9, 1.2, 0.0))))   # the LiDAR's centre of the counterpoise
    p = beacon("localizer")
    if p:
        along = Vector((math.sin(math.radians(h - 90)), math.cos(math.radians(h - 90)), 0.0))
        done.append(localizer(coll, p + RWY_U * 2.5 - along * 2.0, h))  # the array as the orthophoto shows it
    p = beacon("glideslope")
    if p:
        done.append(glideslope(coll, p, h, (-3.0, 4.0)))
    return done


# ---------------------------------------------------------------- signs

def sign(coll, x, y, hdg, text, panel, legend, name):
    """A lit taxiway sign on two frangible legs, its legend facing traffic coming along hdg."""
    w = 0.6 + 0.42 * len(text)
    root = bpy.data.objects.new(name, None)
    root.empty_display_size = 1.0
    root.location = (x, y, 0.0)
    root.rotation_euler = (0.0, 0.0, bearing_rot(hdg + 180.0))  # local +Y faces the aircraft
    coll.objects.link(root)
    m = Mesh([panel, "dark", "galv", "sign_yellow"])
    m.box(-w / 2, w / 2, -0.15, 0.15, 0.45, 1.25, "dark")
    m.face([(-w / 2 + 0.03, 0.151, 0.48), (-w / 2 + 0.03, 0.151, 1.22), (w / 2 - 0.03, 0.151, 1.22), (w / 2 - 0.03, 0.151, 0.48)], panel)
    if panel == "sign_black":  # location signs carry a yellow border
        m.face([(-w / 2 + 0.06, 0.152, 0.51), (-w / 2 + 0.06, 0.152, 1.19), (w / 2 - 0.06, 0.152, 1.19), (w / 2 - 0.06, 0.152, 0.51)], "sign_yellow")
        m.face([(-w / 2 + 0.11, 0.153, 0.56), (-w / 2 + 0.11, 0.153, 1.14), (w / 2 - 0.11, 0.153, 1.14), (w / 2 - 0.11, 0.153, 0.56)], panel)
    for lx in (-w / 2 + 0.25, w / 2 - 0.25):
        cyl(m, "z", 0.0, 0.45, lx, 0.0, 0.04, "galv", 8)
    m.build(name + "_Panel", coll, root, bevel=0.01)
    t = text_object(text, 0.55, 0.01, coll, root, (0.0, 0.16, 0.85), (math.pi / 2, 0.0, math.pi), legend)
    t.name = name + "_Legend"
    return w


def holding_signs(coll):
    for i, h in enumerate(LAYOUT["holds"]):
        f = Vector((math.sin(math.radians(h["hdg"])), math.cos(math.radians(h["hdg"])), 0.0))
        left = Vector((-f.y, f.x, 0.0))
        # turning left onto the runway heads towards the 29 end: that is runway 11
        text = "11-29" if left.dot(RWY_U) > 0 else "29-11"
        p = Vector((h["x"], h["y"], 0.0)) + left * 11.0
        wm = sign(coll, p.x, p.y, h["hdg"], text, "sign_red", "legend_white", f"SIGN_Hold_{i + 1}_{text}")
        if h.get("twy"):
            q = p + left * (wm / 2 + 0.35 + (0.6 + 0.42 * len(h["twy"])) / 2)
            sign(coll, q.x, q.y, h["hdg"], h["twy"], "sign_black", "legend_yellow", f"SIGN_Hold_{i + 1}_Loc_{h['twy']}")


# ---------------------------------------------------------------- fuel station

def fuel_station(coll, refuellers):
    for k, fu in enumerate(LAYOUT["fuel"]):
        root = bpy.data.objects.new(f"FUEL_{k + 1}", None)
        root.location = (fu["x"], fu["y"], 0.0)
        root.rotation_euler = (0.0, 0.0, fu["rot"])
        coll.objects.link(root)
        L, D = fu["L"], fu["D"]
        m = Mesh(["concrete", "white_paint", "galv", "label_jet", "label_avgas", "dark", "red_paint"])
        # bund wall round the tanks, 0.9 m
        bx, by = L / 2 - 0.3, D / 2 - 0.3
        for (x0, x1, y0, y1) in ((-bx, bx, -by, -by + 0.25), (-bx, bx, by - 0.25, by), (-bx, -bx + 0.25, -by + 0.25, by - 0.25), (bx - 0.25, bx, -by + 0.25, by - 0.25)):
            m.box(x0, x1, y0, y1, 0.0, 0.9, "concrete")
        # two horizontal tanks on saddles, 2.6 m across, 8 m long
        tl = min(8.0, L - 3.0)
        for j, (yc, lab, grade) in enumerate(((-D / 4, "label_jet", "JET A-1"), (D / 4 - 0.5, "label_avgas", "AVGAS 100LL"))):
            cyl(m, "x", -tl / 2, tl / 2, yc, 2.0, 1.3, "white_paint", 16)
            cyl(m, "x", -0.5, 0.5, yc, 2.0, 1.32, lab, 16, caps=False)   # grade band round the middle
            for xs in (-tl / 3, tl / 3):
                m.box(xs - 0.2, xs + 0.2, yc - 1.0, yc + 1.0, 0.0, 1.0, "concrete")
            m.prism(octagon(0.12, cx=tl / 4, cy=yc), 3.25, 3.7, "galv")   # vent
            m.box(tl / 2, tl / 2 + 0.6, yc - 0.1, yc + 0.1, 0.9, 1.1, "galv")  # outlet pipe
        # dispenser island on the apron side
        m.box(-2.5, 2.5, D / 2 + 1.0, D / 2 + 2.2, 0.0, 0.18, "concrete")
        for xd, lab in ((-1.4, "label_jet"), (1.4, "label_avgas")):
            m.box(xd - 0.5, xd + 0.5, D / 2 + 1.3, D / 2 + 1.9, 0.18, 2.0, "white_paint")
            m.box(xd - 0.51, xd + 0.51, D / 2 + 1.29, D / 2 + 1.91, 1.6, 1.8, lab)  # grade colour band
        m.box(-0.2, 0.2, D / 2 + 1.4, D / 2 + 1.8, 0.18, 1.2, "red_paint")  # emergency stop post
        m.build(f"FUEL_{k + 1}_Installation", coll, root, bevel=0.02)
        for j, (lab, txt) in enumerate((("legend_black", "JET A-1"), ("legend_white", "AVGAS 100LL"))):
            yc = -D / 4 if j == 0 else D / 4 - 0.5
            for side, rz in ((1.0, math.pi), (-1.0, 0.0)):  # both sides of the tank
                t = text_object(txt, 0.45, 0.01, coll, root, (tl / 4 + 1.2, yc + side * 1.33, 2.0), (math.pi / 2, 0.0, rz), lab)
                t.name = f"FUEL_{k + 1}_Label_{j}"
        # the refuellers parked beside the station, side by side, facing the apron
        for j, me in enumerate(refuellers):
            instance(me, f"GSE_Refueller_{j + 1}", coll, root, (L / 2 + 3.0 + j * 3.6, 0.0, 0.0), 0.0)


# ---------------------------------------------------------------- fence

def fence(coll, post_me):
    """Chain-link fabric (one mesh, UVs in metres), posts every 3 m, barbed wire."""
    verts, faces, uvs = [], [], []
    wire = Mesh(["galv"])
    k = 0
    for line in LAYOUT["fence"]:
        pts = [Vector((x, y, 0.0)) for x, y in line]
        s = 0.0
        for a, b in zip(pts, pts[1:]):
            seg = (b - a).length
            if seg < 0.01:
                continue
            base = len(verts)
            verts += [(a.x, a.y, 0.05), (b.x, b.y, 0.05), (b.x, b.y, 2.4), (a.x, a.y, 2.4)]
            faces.append((base, base + 1, base + 2, base + 3))
            uvs.append([(s, 0.05), (s + seg, 0.05), (s + seg, 2.4), (s, 2.4)])
            d = (b - a).normalized()
            out = Vector((d.y, -d.x, 0.0))
            for z, o in ((2.48, 0.1), (2.58, 0.22), (2.68, 0.34)):
                p0, p1 = a + out * o, b + out * o
                wire.face([(p0.x, p0.y, z), (p1.x, p1.y, z), (p1.x, p1.y, z + 0.008), (p0.x, p0.y, z + 0.008)], "galv")
            t = 0.0
            ang = math.atan2(d.y, d.x)
            while t < seg:
                p = a + d * t
                instance(post_me, "FENCE_Post", coll, None, (p.x, p.y, 0.0), ang - math.pi / 2 + math.pi)
                k += 1
                t += 3.0
            s += seg
    me = bpy.data.meshes.new("FENCE_Fabric")
    me.from_pydata(verts, [], faces)
    uv = me.uv_layers.new(name="UVMap")
    for poly, quv in zip(me.polygons, uvs):
        for li, q in zip(poly.loop_indices, quv):
            uv.data[li].uv = q
    me.materials.append(MATS["chainlink"])
    ob = bpy.data.objects.new("FENCE_Fabric", me)
    coll.objects.link(ob)
    wire.build("FENCE_BarbedWire", coll, None)
    return k


# ---------------------------------------------------------------- GSE placement

def stands_gse(coll, gse):
    st = {s["ref"]: s for s in LAYOUT["stands"]}

    def fwd(s):
        return Vector((math.sin(math.radians(s["hdg"])), math.cos(math.radians(s["hdg"])), 0.0))

    for i, ref in enumerate(("110", "111", "121", "123", "131")):
        if ref not in st:
            continue
        s = st[ref]
        f = fwd(s)
        r = Vector((f.y, -f.x, 0.0))
        p = Vector((s["x"], s["y"], 0.0))
        g = p + r * 4.5 - f * 2.0
        instance(gse["gpu"], f"GSE_GPU_{ref}", coll, None, g.to_tuple(), bearing_rot(s["hdg"] + 180))
    for s in LAYOUT["stands"]:
        f = fwd(s)
        r = Vector((f.y, -f.x, 0.0))
        p = Vector((s["x"], s["y"], 0.0))
        for side in (-1, 1):  # wingtip cones, nose cone
            q = p - f * 4.0 + r * side * 6.5
            instance(gse["cone"], f"PROP_Cone_{s['ref']}", coll, None, q.to_tuple())
        instance(gse["cone"], f"PROP_Cone_{s['ref']}", coll, None, (p + f * 1.5).to_tuple())
    for s in LAYOUT["stands"][::2]:
        f = fwd(s)
        r = Vector((f.y, -f.x, 0.0))
        q = Vector((s["x"], s["y"], 0.0)) - f * 10.0 + r * 9.0
        instance(gse["ext"], f"PROP_Extinguisher_{s['ref']}", coll, None, q.to_tuple(), bearing_rot(s["hdg"]))
    # two baggage trains waiting along the terminal, heading north along the facade
    hdg = (TA_BEARING + 180.0) % 360
    for row, c in enumerate((13.0, 18.0)):
        a0 = 24.0 + row * 14.0
        x, y = W(a0, c)
        instance(gse["tug"], f"GSE_Tug_{row + 1}", coll, None, (x, y, 0.0), bearing_rot(hdg))
        for k in range(4 if row == 0 else 3):
            x, y = W(a0 + 4.0 + k * 4.4, c)
            instance(gse["dolly"], f"GSE_Dolly_{row + 1}_{k + 1}", coll, None, (x, y, 0.0), bearing_rot(hdg))
    # crowd barriers along the terminal front, leaving the doors free
    for k in range(30):
        a = 11.5 + k * 2.4
        if 44.0 < a < 52.0:
            continue
        x, y = W(a, 6.5)
        instance(gse["barrier"], "PROP_CrowdBarrier", coll, None, (x, y, 0.0), bearing_rot(TA_BEARING) + math.pi / 2)


# ---------------------------------------------------------------- build

def build():
    materials()
    coll = collection(COLL)
    ctl = bpy.data.objects.new("WIND_Control", None)
    ctl.empty_display_type = "SINGLE_ARROW"
    ctl.empty_display_size = 5.0
    ctl["wind_from_deg"] = 240.0
    ctl["wind_kt"] = 12.0
    ui = ctl.id_properties_ui("wind_from_deg")
    ui.update(min=0.0, max=360.0, description="Wind direction (from), true degrees")
    ui = ctl.id_properties_ui("wind_kt")
    ui.update(min=0.0, max=60.0, description="Wind speed, knots: the socks droop below about 15 kt")
    coll.objects.link(ctl)
    gse = {"tug": tug_mesh(coll), "dolly": dolly_mesh(coll), "gpu": gpu_mesh(coll), "cone": cone_mesh(coll),
           "ext": extinguisher_mesh(coll), "barrier": barrier_mesh(coll)}
    windsocks(coll, ctl)
    holding_signs(coll)
    fuel_station(coll, [refueller_mesh(coll, "JET A-1"), refueller_mesh(coll, "AVGAS 100LL")])
    stands_gse(coll, gse)
    posts = fence(coll, post_mesh(coll))
    navaids(coll)
    return coll, posts


def shots():
    st = {s["ref"]: s for s in LAYOUT["stands"]}
    s = st.get("121") or LAYOUT["stands"][0]
    h = LAYOUT["holds"][0]
    fu = LAYOUT["fuel"][0] if LAYOUT["fuel"] else {"x": 0, "y": 0}
    ws = LAYOUT["windsocks"][0]
    f = Vector((math.sin(math.radians(h["hdg"])), math.cos(math.radians(h["hdg"])), 0.0))
    hp = Vector((h["x"], h["y"], 0.0))
    fl = LAYOUT["fence"][0][len(LAYOUT["fence"][0]) // 2]
    vo, lo, gs = (beacon(k) or Vector() for k in ("DVOR", "localizer", "glideslope"))
    return {
        "stands": (W(52.0, 34.0) + (7.0,), W(30.0, 12.0) + (1.0,), 30.0),
        "fuel": ((fu["x"] + 26.0, fu["y"] + 20.0, 7.0), (fu["x"], fu["y"], 1.5), 30.0),
        "sign": ((hp - f * 22.0 + Vector((0, 0, 2.2))).to_tuple(), (hp + Vector((-f.y, f.x, 0)) * 12.0 + Vector((0, 0, 0.8))).to_tuple(), 35.0),
        "windsock": ((ws[0] + 12.0, ws[1] - 9.0, 3.0), (ws[0], ws[1], 5.0), 35.0),
        "fence": ((fl[0] + 10.0, fl[1] + 10.0, 2.0), (fl[0], fl[1], 1.4), 30.0),
        "dvor": ((vo.x + 30.0, vo.y - 26.0, 14.0), (vo.x, vo.y, 6.0), 35.0),
        "loc": ((lo.x + 30.0, lo.y - 20.0, 6.0), (lo.x, lo.y, 1.5), 35.0),
        "gs": ((gs.x + 22.0, gs.y + 14.0, 6.0), (gs.x, gs.y, 7.0), 35.0),
    }


def main():
    opt = args(render="/tmp/ebaw_step4.png", shot="stands")
    if "01_Ground_Layout" not in bpy.data.collections:
        saved = sys.argv
        sys.argv = [saved[0], "--", "--render", "none"]
        runpy.run_path(str(HERE / "01_ground_layout.py"), run_name="__main__")
        sys.argv = saved
    coll, posts = build()
    objs = [o for o in coll.all_objects if o.type == "MESH"]
    inst = sum(1 for o in objs if o.data.users > 1)
    print(f"04_Airside_Props: {len(objs)} mesh objects ({inst} instances of shared meshes), {posts} fence posts")
    prev = bpy.data.collections.get("99_Preview") or collection("99_Preview")
    loc, tgt, lens = shots().get(opt["shot"], shots()["stands"])
    cam = camera("CheckCam_Props", prev, loc, tgt, lens)
    if bpy.app.background:
        check_render(opt, cam)
    else:
        bpy.context.scene.camera = cam


main()
