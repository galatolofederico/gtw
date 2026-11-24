"""EBAW (Antwerp International Airport) - step 5a: aeronautical ground lights and apron floodlighting.

Builds collection 05_Lighting:

  runway edge     white, elevated, every 60 m, 1.5 m outside both edges; the
                  last 600 m before each end yellow (caution zone of an
                  instrument runway); red towards the approach in the displaced
                  length of runway 11
  thresholds      green: across the runway 29 threshold (3 m apart), wing bars
                  at the displaced threshold 11
  runway ends     red, across both extremities
  centreline      inset, every 30 m; white, then alternately red and white
                  from 900 m before the end, red the last 300 m (EBAW may not
                  have them: their own object, hide it if not)
  PAPI            four units left of each runway at the aiming point (300 m),
                  3 deg, inner unit 15 m from the edge, 9 m apart; in Blender the
                  on-slope picture (two white, two red); the units carry their
                  setting angles as custom properties for the game
  approach 29     a simple approach lighting system, 420 m, lights every 60 m and
                  a 30 m crossbar at 300 m (assumed: the AIP was not readable)
  taxiway/apron   blue edge lights (OSM outlines, every 30 m, 15 m on bends)
  floodlights     four 25 m apron masts, red and white like the photographs,
                  six floods each and a real spot light for the night renders

Every fixture is a linked duplicate of a few meshes; the lenses are emissive.

    blender -b assets/blender/ebaw/ebaw.blend --python assets/blender/ebaw/05_lighting.py -- \
        [--render /tmp/ebaw_step5.png] [--shot night|approach|papi|apron] [--samples 64] \
        [--res 1920x1080] [--save assets/blender/ebaw/ebaw.blend]

The shots are night renders (a dark sky, no sun).

Sources: ICAO Annex 14 vol. I, 5.3 (light colours, spacing, PAPI layout);
light positions along the runway from the step 1 frame; blue edge lights from
the OSM pavement outlines (ebaw_layout.json). EBAW's own AD 2.14/2.15 lighting
was not available: the approach system, the centreline lights and the PAPI
angles are assumptions to check against the AIP.
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
from ebaw_lib import MATS, Graph, Mesh, args, camera, collection, instance, octagon  # noqa: E402

LAYOUT = json.loads((HERE / "ebaw_layout.json").read_text())
COLL = "05_Lighting"
RWY = LAYOUT["runway"]
HDG = RWY["hdg_11"]
L, WID = RWY["length"], RWY["width"]
U = Vector((math.sin(math.radians(HDG)), math.cos(math.radians(HDG)), 0.0))
V = Vector((-U.y, U.x, 0.0))
DISPLACED_11 = 144.0          # as in 01_ground_layout.py
APPROACH_29 = "simple"        # "simple" (420 m) or None
CENTRELINE = True
T = LAYOUT["terminal"]
TO, TA, TC = Vector((*T["origin"], 0)), Vector((*T["a_axis"], 0)), Vector((*T["c_axis"], 0))
MASTS = [(40.0, 65.0), (85.0, 65.0), (120.0, 40.0), (-20.0, 100.0)]  # terminal frame (a, c), on apron 1


def R(u, v):
    p = U * u + V * v
    return Vector((p.x, p.y, 0.0))


# ---------------------------------------------------------------- materials

def lens(name, rgb, strength):
    g = Graph(name)
    g.bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
    g.bsdf.inputs["Emission Strength"].default_value = strength
    return g.finish(tuple(c * 0.5 for c in rgb), 0.2, 0.5)


def plain(name, rgb, rough=0.5, metallic=0.0):
    g = Graph(name)
    g.bsdf.inputs["Metallic"].default_value = metallic
    return g.finish(rgb, rough, 0.5)


def mast_paint():
    """Red and white bands, 3 m, for the obstacle-marked floodlight masts."""
    g = Graph("M_Mast_RedWhite")
    sep = g.node("ShaderNodeSeparateXYZ")
    g.put(sep.inputs[0], g.pos)
    band = g.math("MODULO", g.math("FLOOR", g.math("DIVIDE", sep.outputs["Z"], 3.0)), 2.0)
    c = g.mix(band, (0.7, 0.7, 0.68), (0.6, 0.04, 0.02))
    return g.finish(c, 0.45, 0.5)


def materials():
    MATS.update({
        "l_white": lens("M_Lens_White", (1.0, 0.92, 0.78), 400.0),
        "l_yellow": lens("M_Lens_Yellow", (1.0, 0.65, 0.05), 400.0),
        "l_red": lens("M_Lens_Red", (1.0, 0.03, 0.01), 400.0),
        "l_green": lens("M_Lens_Green", (0.05, 1.0, 0.35), 400.0),
        "l_blue": lens("M_Lens_Blue", (0.05, 0.2, 1.0), 300.0),
        "l_flood": lens("M_Lens_Flood", (1.0, 0.85, 0.65), 40.0),
        "fixture": plain("M_Fixture_Yellow", (0.75, 0.55, 0.02), 0.4),
        "steel": plain("M_Fixture_Steel", (0.4, 0.41, 0.42), 0.35, 0.9),
        "dark": plain("M_Fixture_Dark", (0.03, 0.03, 0.035), 0.4, 0.6),
        "mast": mast_paint(),
        "concrete": plain("M_Concrete_Base", (0.3, 0.29, 0.28), 0.85),
    })


# ---------------------------------------------------------------- fixtures (instanced)

def kept(m, name, coll):
    ob = m.build(name, coll, None)
    me = ob.data
    bpy.data.objects.remove(ob)
    return me


def elevated(coll, key, name):
    """Elevated light: frangible yellow stem, 0.35 m, glass dome lens."""
    m = Mesh(["fixture", key, "dark"])
    m.prism(octagon(0.09), 0.0, 0.05, "dark")
    m.prism(octagon(0.025), 0.05, 0.30, "fixture", top=False, bottom=False)
    m.prism(octagon(0.07), 0.30, 0.33, "dark")
    lo, hi = octagon(0.065), octagon(0.03)
    m.frustum(lo, hi, 0.33, 0.42, key)
    m.face([(x, y, 0.42) for x, y in hi], key)
    return kept(m, name, coll)


def inset(coll, key, name):
    """Inset (flush) light: a steel disc with a raised lens prism."""
    m = Mesh(["steel", key])
    m.prism(octagon(0.15), 0.0, 0.012, "steel")
    m.box(-0.05, 0.05, -0.02, 0.02, 0.012, 0.022, key)
    return kept(m, name, coll)


def papi_unit(coll, upper):
    """A PAPI box on three legs; two projector lenses showing white (upper) or red."""
    m = Mesh(["dark", "steel", "l_white" if upper else "l_red"])
    for x, y in ((-0.5, -0.2), (0.5, -0.2), (0.0, 0.3)):
        m.prism(octagon(0.04, cx=x, cy=y), 0.0, 0.5, "steel")
    m.box(-0.65, 0.65, -0.35, 0.35, 0.5, 0.95, "dark")
    for x in (-0.3, 0.3):
        m.prism(octagon(0.12, cx=x, cy=-0.36), 0.6, 0.85, "l_white" if upper else "l_red")
    return kept(m, "PAPI_Unit_" + ("White" if upper else "Red"), coll)


# ---------------------------------------------------------------- build

def build():
    materials()
    coll = collection(COLL)
    F = {k: elevated(coll, "l_" + k, f"LIGHT_Elevated_{k.title()}") for k in ("white", "yellow", "red", "green", "blue")}
    I = {k: inset(coll, "l_" + k, f"LIGHT_Inset_{k.title()}") for k in ("white", "red", "green")}
    rot_u = math.atan2(U.y, U.x)
    counts = {}

    def put(me, p, name, rz=rot_u, group=None):
        ob = instance(me, name, coll, None, (p.x, p.y, 0.0), rz)
        counts[group or name] = counts.get(group or name, 0) + 1
        return ob

    u11, u29 = -L / 2 + DISPLACED_11, L / 2   # thresholds
    end_w, end_e = -L / 2, L / 2               # extremities
    ve = WID / 2 + 1.5
    # runway edge lights, every 60 m between the extremities
    n = int(L // 60.0)
    for i in range(n + 1):
        u = end_w + L * i / n
        if u < u11 - 1.0:
            key = "red"       # displaced length, as seen from the approach to 11
        elif u > end_e - 600.0 or u < end_w + 600.0:
            key = "yellow"    # caution zone: the last 600 m before either end
        else:
            key = "white"
        for s in (-1, 1):
            put(F[key], R(u, s * ve), f"RWY_Edge_{key}", group="runway edge")
    # threshold 29: green across, 3 m apart; runway ends: red across (at the east end both share the line)
    k = int((WID + 2.0) // 3.0)
    for i in range(k + 1):
        v = -(WID / 2 + 1.0) + (WID + 2.0) * i / k
        put(F["green"], R(u29 + 0.0, v), "RWY_THR29_Green", group="threshold")
        put(F["red"], R(u29 + 1.2, v), "RWY_End_Red", group="runway end")
        put(F["red"], R(end_w - 1.2, v), "RWY_End_Red", group="runway end")
    # threshold 11 (displaced): green wing bars, 5 lights each side out from the edge
    for s in (-1, 1):
        for i in range(5):
            put(F["green"], R(u11, s * (ve + 1.5 + i * 3.0)), "RWY_THR11_WingBar", group="threshold")
    # centreline, every 30 m, 0.6 m off the centre (clear of the painted line)
    if CENTRELINE:
        u = u11 + 15.0
        while u < u29 - 15.0:
            to_end = min(u29 - u, u - u11)   # bidirectional: colour seen from the nearer end
            key = "red" if to_end < 300.0 else ("red" if to_end < 900.0 and int(u // 30.0) % 2 else "white")
            put(I[key], R(u, 0.6), f"RWY_Centreline_{key}", group="centreline")
            u += 30.0
    # PAPI: left of each landing direction at the aiming point
    pw, pr = papi_unit(coll, True), papi_unit(coll, False)
    angles = [3.5, 3.1666, 2.8333, 2.5]  # outer to inner, deg (3 deg glide path)
    for thr, sgn, name in ((u11, 1, "11"), (u29, -1, "29")):
        left = sgn  # +v is left of runway 11; -v is left of runway 29
        for j in range(4):
            # outermost unit first, inner unit 15 m from the edge
            v = left * (WID / 2 + 15.0 + (3 - j) * 9.0)
            ob = put(pw if j < 2 else pr, R(thr + sgn * 300.0, v), f"PAPI_{name}_{j + 1}",
                     rot_u + (0.0 if sgn > 0 else math.pi) + math.pi / 2, group="PAPI")
            ob["papi_runway"] = name
            ob["papi_setting_deg"] = angles[j]
    # approach lights 29: east of the runway end, along the extended centreline
    if APPROACH_29 == "simple":
        for i in range(1, 8):
            d = 60.0 * i
            for dv in ((-1.5, 0.0, 1.5) if i > 1 else (0.0,)):
                put(F["white"], R(end_e + d, dv), "APP_29_Centreline", group="approach")
        for i in range(-5, 6):
            if i == 0:
                continue
            put(F["white"], R(end_e + 300.0, i * 3.0), "APP_29_Crossbar", group="approach")
    # taxiway and apron edges, blue
    for x, y in LAYOUT["lights"]["taxiway_edge"]:
        put(F["blue"], Vector((x, y, 0.0)), "TWY_Edge_Blue", 0.0, group="taxiway edge")
    for x, y in LAYOUT["lights"]["apron_edge"]:
        put(F["blue"], Vector((x, y, 0.0)), "APR_Edge_Blue", 0.0, group="apron edge")
    masts(coll, counts)
    return coll, counts


def masts(coll, counts):
    m = Mesh(["mast", "concrete", "dark", "l_flood", "steel"])
    m.prism(octagon(0.9), 0.0, 0.3, "concrete")
    lo, hi = octagon(0.32), octagon(0.16)
    m.frustum(lo, hi, 0.3, 25.0, "mast")
    m.box(-1.6, 1.6, -0.5, 0.5, 25.0, 25.15, "steel")         # head frame
    m.box(-1.6, 1.6, -0.06, 0.06, 25.15, 26.2, "steel")
    for k in range(6):
        x = -1.35 + k * 0.54
        m.box(x - 0.22, x + 0.22, 0.06, 0.4, 25.3, 26.0, "dark")     # flood housings, facing +Y
        m.face([(x - 0.18, 0.41, 25.35), (x - 0.18, 0.41, 25.95), (x + 0.18, 0.41, 25.95), (x + 0.18, 0.41, 25.35)][::-1], "l_flood")
    m.prism(octagon(0.08, cx=0.0, cy=0.0), 26.2, 26.35, "dark")
    me = kept(m, "MAST_Flood", coll)
    obst = Mesh(["l_red"])
    obst.prism(octagon(0.09), 26.35, 26.5, "l_red")
    obst_me = kept(obst, "MAST_Obstacle_Red", coll)
    centre = TO + TA * 60.0 + TC * 45.0   # floods aim at the apron in front of the terminal
    for i, (a, c) in enumerate(MASTS):
        p = TO + TA * a + TC * c
        d = (centre - p)
        d = TC * -1.0 if d.length < 1 else d.normalized()
        rz = math.atan2(d.y, d.x) - math.pi / 2  # local +Y towards the apron centre
        instance(me, f"MAST_Flood_{i + 1}", coll, None, (p.x, p.y, 0.0), rz)
        instance(obst_me, f"MAST_Flood_{i + 1}_Obstacle", coll, None, (p.x, p.y, 0.0), rz)
        ld = bpy.data.lights.new(f"MAST_Flood_{i + 1}_Spot", "SPOT")
        ld.energy = 60000.0
        ld.spot_size = math.radians(110)
        ld.spot_blend = 0.6
        ld.color = (1.0, 0.86, 0.68)
        ld.shadow_soft_size = 0.8
        lo_ = bpy.data.objects.new(f"MAST_Flood_{i + 1}_Spot", ld)
        coll.objects.link(lo_)
        lo_.location = (p.x + d.x * 0.6, p.y + d.y * 0.6, 25.6)
        tgt = p + d * min(35.0, max(20.0, (centre - p).length))  # each mast lights its own patch
        aim = Vector((tgt.x, tgt.y, 0.0)) - Vector(lo_.location)
        lo_.rotation_euler = aim.to_track_quat("-Z", "Y").to_euler()
        counts["floodlight masts"] = counts.get("floodlight masts", 0) + 1


# ---------------------------------------------------------------- night render

def night(opt, cam):
    """Night: a dark blue sky, no sun, the lamps and lenses alone; renders if asked."""
    scene = bpy.context.scene
    scene.camera = cam
    sun = bpy.data.objects.get("Sun")
    if sun is not None:
        bpy.data.objects.remove(sun, do_unlink=True)
    world = bpy.data.worlds.get("EBAW_Night") or bpy.data.worlds.new("EBAW_Night")
    scene.world = world
    world.use_nodes = True
    nt = world.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputWorld")
    bg = nt.nodes.new("ShaderNodeBackground")
    grad = nt.nodes.new("ShaderNodeTexGradient")
    tc = nt.nodes.new("ShaderNodeTexCoord")
    mp = nt.nodes.new("ShaderNodeMapping")
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    mp.inputs["Rotation"].default_value = (0.0, -math.pi / 2, 0.0)
    nt.links.new(tc.outputs["Generated"], mp.inputs["Vector"])
    nt.links.new(mp.outputs["Vector"], grad.inputs["Vector"])
    nt.links.new(grad.outputs["Fac"], ramp.inputs["Fac"])
    ramp.color_ramp.elements[0].color = (0.010, 0.016, 0.035, 1.0)   # horizon: city glow
    ramp.color_ramp.elements[1].color = (0.001, 0.002, 0.006, 1.0)   # zenith
    nt.links.new(ramp.outputs["Color"], bg.inputs["Color"])
    bg.inputs["Strength"].default_value = 1.0
    nt.links.new(bg.outputs[0], out.inputs[0])
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
        scene.view_settings.exposure = 1.0
    except TypeError:
        pass
    glow(scene)
    scene.render.image_settings.file_format = "PNG"
    scene.render.filepath = opt["render"]
    if opt["save"]:
        bpy.ops.wm.save_as_mainfile(filepath=str(Path(opt["save"]).resolve()))
    if opt["render"] and opt["render"] != "none":
        bpy.ops.render.render(write_still=True)
        print("rendered", opt["render"])


def glow(scene):
    """Compositor bloom, so that lights a few centimetres across still show as
    points of light from far away, as they do to the eye (Blender 5 node-group
    compositor)."""
    try:
        ng = bpy.data.node_groups.get("EBAW_Night_Glow") or bpy.data.node_groups.new("EBAW_Night_Glow", "CompositorNodeTree")
        ng.nodes.clear()
        if not ng.interface.items_tree:
            ng.interface.new_socket("Image", in_out="OUTPUT", socket_type="NodeSocketColor")
        rl = ng.nodes.new("CompositorNodeRLayers")
        gl = ng.nodes.new("CompositorNodeGlare")
        out = ng.nodes.new("NodeGroupOutput")
        gl.inputs["Type"].default_value = "Bloom"
        gl.inputs["Threshold"].default_value = 2.0
        gl.inputs["Strength"].default_value = 0.6
        gl.inputs["Size"].default_value = 0.35 if gl.inputs["Size"].type == "VALUE" else 7
        ng.links.new(rl.outputs["Image"], gl.inputs["Image"])
        ng.links.new(gl.outputs["Image"], out.inputs[0])
        scene.compositing_node_group = ng
        scene.render.use_compositing = True
    except (AttributeError, KeyError, TypeError) as e:
        print("no bloom:", e)


def shots():
    t29 = R(L / 2, 0.0)
    t11 = R(-L / 2 + DISPLACED_11, 0.0)
    ap = TO + TA * 60.0 + TC * 30.0
    return {
        # on a 3 deg approach to runway 29, 2.5 km out
        "approach": ((t29 + U * 1600.0 + Vector((0, 0, 85.0))).to_tuple(), (t29 - U * 500.0).to_tuple(), 55.0),
        "night": ((-250.0, -1350.0, 420.0), (-120.0, -40.0, 0.0), 32.0),
        "papi": ((t11 - U * 900.0 + Vector((0, 0, 48.0))).to_tuple(), (t11 + U * 300.0).to_tuple(), 60.0),
        "apron": ((ap + TC * 160.0 + Vector((0, 0, 30.0))).to_tuple(), (ap + Vector((0, 0, 4.0))).to_tuple(), 30.0),
    }


def main():
    opt = args(render="/tmp/ebaw_step5.png", shot="night", samples=64)
    if "01_Ground_Layout" not in bpy.data.collections:
        saved = sys.argv
        sys.argv = [saved[0], "--", "--render", "none"]
        runpy.run_path(str(HERE / "01_ground_layout.py"), run_name="__main__")
        sys.argv = saved
    coll, counts = build()
    print("05_Lighting:", ", ".join(f"{v} {k}" for k, v in counts.items()))
    prev = bpy.data.collections.get("99_Preview") or collection("99_Preview")
    loc, tgt, lens_mm = shots().get(opt["shot"], shots()["night"])
    cam = camera("CheckCam_Lighting", prev, loc, tgt, lens_mm)
    if bpy.app.background:
        night(opt, cam)
    else:
        bpy.context.scene.camera = cam


main()
