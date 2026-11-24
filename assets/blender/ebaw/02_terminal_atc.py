"""EBAW (Antwerp International Airport) - step 2: terminal and control tower.

Builds collection 02_Terminal_ATC on top of 01_Ground_Layout (built first
if missing): the terminal hall, its canopies with the "ANTWERP AIRPORT" /
"LUCHTHAVEN ANTWERPEN" lettering, the panoramic gallery and roof terrace on
the airside, a suggested interior behind the glass, and the control tower
(shaft, glazed observation level, inclined cab with consoles, antennas and
red obstacle lights).

    blender -b assets/blender/ebaw/ebaw.blend --python assets/blender/ebaw/02_terminal_atc.py -- \
        [--render /tmp/ebaw_step2.png] [--shot airside|landside|aerial|tower] [--samples 48] \
        [--res 1920x1080] [--save assets/blender/ebaw/ebaw.blend]

Run inside an open Blender (Text editor, or the MCP bridge), it rebuilds the
collection without rendering.

References: photographs of the building (Wikimedia Commons
"Luchthavengebouw Antwerpen-Deurne"; Belga / made-in.be; rateone.be).
The building as it stands is white render with dark ribbon windows, not
brick: the photographs win over the brief. Its footprint and the tower's
position are from OpenStreetMap (ways 4414121 and 909567225), simplified to
a hall along the airside facade. Heights are estimated from the photographs
(about 3.6 m a storey; the tower cab's roof about 23 m above the apron):
they are estimates, not survey figures.

Frame: the shared EBAW frame (see 01_ground_layout.py). The building sits
under the empty TERMINAL_Frame at the north end of its airside facade,
(-804.55, -53.97); its local X ("a") runs along the facade to the south-south-
east (bearing 166.9 deg), local Y ("c") out of the facade towards the apron.

    hall               a 4..90, c -50..0, walls to 9.0 m (parapet), roof 8.4 m
    airside canopy     a 10..84, c 0..4.6, fascia 3.4..5.3 m
    landside canopy    a 20..70, c -54.6..-50
    tower shaft        a 35.8..45.8, c -36.4..-26.4 (OSM tower at a 40.8, c -31.4)
    observation level  15.35..18.3 m; cab 18.7..22.3 m; cab roof 22.7 m
    panorama gallery   a 8..34, c -8..0, on the roof (north, as photographed); open terrace a 56..80
"""

import math
import runpy
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Vector
from mathutils.geometry import tessellate_polygon

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from ebaw_lib import (MATS, MODULE, Graph, Mesh, add_bevel, args, bays, box_uv, camera, check_render,  # noqa: E402
                      collection, facade, instance, octagon, rail, text_object, vmul, window_mesh)

import json  # noqa: E402

LAYOUT = json.loads((HERE / "ebaw_layout.json").read_text())
T = LAYOUT["terminal"]
COLL = "02_Terminal_ATC"
O = Vector((*T["origin"], 0.0))
A = Vector((*T["a_axis"], 0.0))
C = Vector((*T["c_axis"], 0.0))
THETA = math.atan2(A.y, A.x)
HALL_A0, HALL_A1, HALL_C0, HALL_C1 = T["hall"]
TWR_A, TWR_C = T["tower"]

WALL_T = 0.35        # wall thickness
Z_SLAB = 4.2         # first floor level
Z_ROOF = 8.4         # roof surface, behind the parapet
Z_TOP = 9.0          # parapet top


def W(a, c, z=0.0):
    """Terminal frame -> EBAW frame."""
    return O + A * a + C * c + Vector((0.0, 0.0, z))


# ---------------------------------------------------------------- materials

def mat_render():
    """White mineral render: slight blotching, rain streaks under the sills, splash dirt at the foot."""
    g = Graph("M_Render_White")
    blotch = g.noise(g.pos, 0.6, 5.0, 0.6)
    fine = g.noise(g.pos, 25.0, 6.0, 0.7)
    streak = g.smooth(g.noise(vmul(g, g.pos, (3.0, 3.0, 0.12)), 1.0, 5.0, 0.6), 0.6, 0.78)
    sep = g.node("ShaderNodeSeparateXYZ")
    g.put(sep.inputs[0], g.pos)
    foot = g.math("SUBTRACT", 1.0, g.smooth(sep.outputs["Z"], 0.05, 0.9))
    c = g.mix(g.smooth(blotch, 0.3, 0.7), (0.58, 0.575, 0.55), (0.66, 0.655, 0.63))
    c = g.mix(g.math("MULTIPLY", streak, 0.45), c, (0.33, 0.32, 0.30))
    c = g.mix(g.math("MULTIPLY", foot, 0.6), c, (0.22, 0.21, 0.19))
    g.bump(fine, 0.18, 0.003)
    return g.finish(c, g.math("ADD", 0.82, g.math("MULTIPLY", fine, 0.12)), 0.4)


def mat_band():
    """Light concrete / reconstituted stone bands and copings."""
    g = Graph("M_Concrete_Band")
    fine = g.noise(g.pos, 18.0, 6.0, 0.65)
    c = g.mix(g.smooth(g.noise(g.pos, 1.2, 4.0), 0.3, 0.7), (0.40, 0.395, 0.375), (0.48, 0.47, 0.445))
    c = g.mix(g.math("MULTIPLY", g.smooth(fine, 0.62, 0.8), 0.5), c, (0.30, 0.29, 0.27))
    g.bump(fine, 0.3, 0.003)
    return g.finish(c, g.math("ADD", 0.75, g.math("MULTIPLY", fine, 0.15)), 0.45)


def mat_metal(name, rgb, metallic, rough):
    g = Graph(name)
    fine = g.noise(g.pos, 40.0, 4.0)
    smudge = g.smooth(g.noise(g.pos, 2.0, 5.0), 0.55, 0.75)
    c = g.mix(g.math("MULTIPLY", smudge, 0.3), rgb, tuple(x * 0.7 for x in rgb))
    g.bsdf.inputs["Metallic"].default_value = metallic
    g.bump(fine, 0.05, 0.001)
    return g.finish(c, g.lerp(smudge, rough, rough + 0.2), 0.5)


def mat_glass(name, tint, rough=0.02):
    g = Graph(name)
    g.bsdf.inputs["Transmission Weight"].default_value = 1.0
    g.bsdf.inputs["IOR"].default_value = 1.15  # single sheet: little refraction, still some Fresnel
    dirt = g.smooth(g.noise(g.pos, 3.0, 6.0), 0.6, 0.8)
    return g.finish(tint, g.lerp(dirt, rough, rough + 0.08), 0.6)


def mat_gravel():
    g = Graph("M_Roof_Gravel")
    fine = g.noise(g.pos, 30.0, 8.0, 0.7)
    c = g.mix(g.smooth(fine, 0.3, 0.7), (0.12, 0.115, 0.105), (0.24, 0.23, 0.21))
    c = g.mix(g.math("MULTIPLY", g.smooth(g.noise(g.pos, 0.3, 4.0), 0.6, 0.75), 0.5), c, (0.07, 0.075, 0.06))
    g.bump(fine, 0.6, 0.01)
    return g.finish(c, 0.92, 0.3)


def mat_terrazzo():
    g = Graph("M_Floor_Terrazzo")
    chips = g.node("ShaderNodeTexVoronoi", feature="F1")
    g.put(chips.inputs["Vector"], g.pos)
    chips.inputs["Scale"].default_value = 30.0
    c = g.mix(g.smooth(chips.outputs["Distance"], 0.1, 0.2), (0.12, 0.115, 0.11), (0.42, 0.41, 0.39))
    return g.finish(c, 0.25, 0.5)


def mat_flat(name, rgb, rough=0.7, emit=0.0):
    g = Graph(name)
    if emit:
        g.bsdf.inputs["Emission Color"].default_value = (*rgb, 1.0)
        g.bsdf.inputs["Emission Strength"].default_value = emit
    fine = g.noise(g.pos, 6.0, 4.0)
    c = g.mix(g.math("MULTIPLY", fine, 0.15), rgb, tuple(x * 0.8 for x in rgb))
    return g.finish(c, rough, 0.5)


def mat_wood():
    g = Graph("M_Wood_Deck")
    sep = g.node("ShaderNodeSeparateXYZ")
    g.put(sep.inputs[0], g.pos)
    board = g.white(g.vec(g.math("FLOOR", g.math("DIVIDE", sep.outputs["Y"], 0.15)), 0.0))
    grain = g.noise(vmul(g, g.pos, (0.3, 8.0, 1.0)), 3.0, 6.0)
    gap = g.math("SUBTRACT", 1.0, g.smooth(g.math("ABSOLUTE", g.math("SUBTRACT", g.math("FRACT", g.math("DIVIDE", sep.outputs["Y"], 0.15)), 0.5)), 0.44, 0.47))
    c = g.mix(board, (0.16, 0.10, 0.06), (0.26, 0.17, 0.10))
    c = g.mix(g.math("MULTIPLY", grain, 0.4), c, (0.10, 0.06, 0.035))
    c = g.mix(g.math("SUBTRACT", 1.0, gap), c, (0.02, 0.015, 0.01))
    g.bump(grain, 0.2, 0.002)
    return g.finish(c, 0.6, 0.4)



def materials():
    MATS.update({
        "render": mat_render(),
        "band": mat_band(),
        "frame": mat_metal("M_Frame_Anthracite", (0.025, 0.027, 0.03), 0.6, 0.35),
        "glass": mat_glass("M_Glass_Clear", (0.80, 0.86, 0.84)),
        "glass_twr": mat_glass("M_Glass_Tower_Tinted", (0.10, 0.13, 0.13)),
        "gravel": mat_gravel(),
        "galv": mat_metal("M_Metal_Galvanised", (0.45, 0.46, 0.47), 0.9, 0.4),
        "metal_dark": mat_metal("M_Metal_Dark", (0.035, 0.037, 0.04), 0.7, 0.45),
        "terrazzo": mat_terrazzo(),
        "interior": mat_flat("M_Interior_Wall", (0.22, 0.21, 0.19), 0.8),
        "ceiling": mat_flat("M_Ceiling", (0.35, 0.35, 0.34), 0.9),
        "light": mat_flat("M_Light_Warm", (1.0, 0.78, 0.55), 0.5, emit=6.0),
        "screen": mat_flat("M_Screen", (0.25, 0.45, 0.85), 0.3, emit=3.0),
        "sign_light": mat_flat("M_Sign_Backlit", (0.85, 0.9, 1.0), 0.4, emit=4.0),
        "yellow": mat_flat("M_Sign_Yellow", (0.85, 0.6, 0.0), 0.45),
        "letters": mat_metal("M_Letters_Anthracite", (0.04, 0.045, 0.05), 0.5, 0.4),
        "red": mat_flat("M_Obstacle_Red", (1.0, 0.02, 0.01), 0.3, emit=40.0),
        "wood": mat_wood(),
        "desk": mat_flat("M_Desk_Laminate", (0.55, 0.55, 0.53), 0.4),
        "membrane": mat_flat("M_Roof_Membrane", (0.055, 0.056, 0.06), 0.6),
    })


# ---------------------------------------------------------------- mesh building

# ---------------------------------------------------------------- facades

def walls(name, coll, parent, outline, z0, z1, plan, bands, win):
    """Solid walls with openings round a CCW outline, thickened inwards and
    chamfered; window modules instanced in the openings.

    plan(side_index, length) -> list of (x0, n_modules, za, zb, module_mesh)."""
    m = Mesh(["render", "band"])
    placed = []
    n = len(outline)
    for i in range(n):
        p0, p1 = outline[i], outline[(i + 1) % n]
        L = math.dist(p0, p1)
        groups = plan(i, L)
        opens = [(g[0], g[0] + g[1] * MODULE, g[2], g[3]) for g in groups]
        _, d = facade(m, p0, p1, z0, z1, opens, bands)
        for x0, k, za, zb, wm in groups:
            for j in range(k):
                # the module's +Y must face out (right of d), so it runs backwards
                # along d from its far end; it sits 12 cm into the reveal
                at = Vector((*p0, 0.0)) + d * (x0 + (j + 1) * MODULE) + Vector((d.y, -d.x, 0.0)) * -0.12
                placed.append((wm, (at.x, at.y, za), math.atan2(d.y, d.x) + math.pi))
    ob = m.build(name, coll, parent)
    sd = ob.modifiers.new("Wall", "SOLIDIFY")
    sd.thickness = WALL_T
    sd.offset = -1.0          # into the building
    sd.use_even_offset = True
    sd.use_rim = True         # reveals round the openings, coping on top
    sd.material_offset_rim = 1  # reveals and coping in the band material
    add_bevel(ob, 0.03)
    for k, (wm, at, rz) in enumerate(placed):
        instance(wm, f"{name}_Win_{k:03d}", win, parent, at, rz)
    return ob


# ---------------------------------------------------------------- building

def build():
    materials()
    coll = collection(COLL)
    win = bpy.data.collections.get(COLL + "_Windows")
    if win is None:
        win = bpy.data.collections.new(COLL + "_Windows")
        coll.children.link(win)
    for ob in list(win.objects):
        bpy.data.objects.remove(ob, do_unlink=True)

    root = bpy.data.objects.new("TERMINAL_Frame", None)
    root.empty_display_type = "ARROWS"
    root.empty_display_size = 10.0
    root.location = O
    root.rotation_euler = (0.0, 0.0, THETA)
    coll.objects.link(root)

    # the modules every facade instances
    W_RIBBON = window_mesh("WIN_Ribbon", MODULE, 2.0, 2, 1)
    W_GROUND = window_mesh("WIN_Ground", MODULE, 2.1, 2, 1)
    W_CURTAIN = window_mesh("WIN_Curtain", MODULE, 3.4, 1, 2)
    W_TOWER = window_mesh("WIN_Tower", MODULE, 1.8, 2, 1, glass="glass_twr")

    tv = LAYOUT.get("terminal_vl")
    if tv:
        # The terminal as measured (Flemish open data, ebaw_vl.py): blocks of
        # their own roof height on the cadastral footprint, the tower where the
        # height model peaks, scaled to its measured 19 m.
        measured_terminal(coll, win, root, {"ribbon": W_RIBBON, "ground": W_GROUND, "curtain": W_CURTAIN}, tv)
        t = tv["tower"]
        rel = Vector((t["x"], t["y"], 0.0)) - O
        ta, tc = rel.dot(A), rel.dot(C)
        shift = bpy.data.objects.new("TOWER_Shift", None)
        coll.objects.link(shift)
        shift.parent = root
        shift.location = (0.0, 0.0, t["top"] - 22.7)   # the cab roof at the measured height
        base = block_height_at(tv, t["x"], t["y"])
        tower(coll, shift, win, W_TOWER, None, ta, tc, shaft_from=base - (t["top"] - 22.7))
        return coll, root

    a0, a1, c0, c1 = HALL_A0, HALL_A1, HALL_C0, HALL_C1
    hall = [(a0, c0), (a1, c0), (a1, c1), (a0, c1)]  # CCW: landside, south end, airside, north end

    def hall_plan(side, L):
        out = []
        # canopy stretches in side coordinates: airside (side 2) runs from a1 to a0
        canopy = {2: (a1 - 84.0, a1 - 10.0), 0: (20.0 - a0, 70.0 - a0)}.get(side)
        if canopy:
            out += [(x, k, 0.0, 3.4, W_CURTAIN) for x, k in bays(canopy[0] + 0.6, canopy[1] - 0.6, 4, 1.5)]
            for lo, hi in ((1.5, canopy[0] - 1.0), (canopy[1] + 1.0, L - 1.5)):
                out += [(x, k, 0.9, 3.0, W_GROUND) for x, k in bays(lo, hi, 3)]
        else:
            out += [(x, k, 0.9, 3.0, W_GROUND) for x, k in bays(1.5, L - 1.5, 3)]
        # first floor: one long ribbon, a pier every 8 modules
        out += [(x, k, 5.2, 7.2, W_RIBBON) for x, k in bays(1.2, L - 1.2, 8, 0.6)]
        return out
    walls("TERM_Hall_Walls", coll, root, hall, 0.0, Z_TOP, hall_plan, [(3.8, 4.2), (8.7, 9.0)], win)

    # floors, ceilings, roof (with holes for what stands on it)
    i0, i1, j0, j1 = a0 + WALL_T, a1 - WALL_T, c0 + WALL_T, c1 - WALL_T
    inner = [(i0, j0), (i1, j0), (i1, j1), (i0, j1)]
    tw = (TWR_A - 5.0, TWR_A + 5.0, TWR_C - 5.0, TWR_C + 5.0)
    gal = (8.0, 34.0, -8.0, j1)
    ter = (56.0, 80.0, -7.0, j1)

    def rect(x0, x1, y0, y1):
        return [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]

    def hole(x0, x1, y0, y1):
        return [(x0, y0), (x0, y1), (x1, y1), (x1, y0)]
    fl = Mesh(["terrazzo", "ceiling", "interior", "gravel", "wood", "band"])
    fl.slab([inner], 0.0, "terrazzo")
    fl.slab([inner], 3.8, "ceiling", up=False)
    fl.slab([inner], Z_SLAB, "terrazzo")
    fl.slab([inner], 8.0, "ceiling", up=False)
    fl.slab([inner, hole(*tw), hole(*gal), hole(*ter)], Z_ROOF, "gravel")
    fl.slab([rect(*gal)], Z_ROOF, "wood")
    fl.slab([rect(*ter)], Z_ROOF, "wood")
    # first-floor offices behind the ribbon windows: a dark backdrop, lit strips in front
    fl.prism(rect(10.0, 84.0, -44.0, -6.0), Z_SLAB, 7.9, "interior", top=False, bottom=False)
    # tower core through the hall
    for zb, zt in ((0.0, 3.8), (Z_SLAB, 8.0)):
        fl.prism(rect(tw[0] + 1.0, tw[1] - 1.0, tw[2] + 1.0, tw[3] - 1.0), zb, zt, "interior", top=False, bottom=False)
    fl.build("TERM_Floors_Roof", coll, root)

    # interior: ceiling lights, columns, check-in desks, benches
    lt = Mesh(["light", "sign_light"])
    for a in range(int(i0) + 3, int(i1) - 1, 4):
        for c in range(int(j0) + 3, int(j1) - 1, 5):
            if not (tw[0] <= a <= tw[1] and tw[2] <= c <= tw[3]):
                lt.face([(a, c, 3.79), (a, c + 0.6, 3.79), (a + 1.2, c + 0.6, 3.79), (a + 1.2, c, 3.79)], "light")
        lt.face([(a, -3.0, 7.99), (a, -2.7, 7.99), (a + 3.0, -2.7, 7.99), (a + 3.0, -3.0, 7.99)], "light")
    for k in range(12):  # backlit desk signs
        a = 22.0 + k * 5.0
        lt.box(a - 0.8, a + 0.8, -42.6, -42.5, 2.4, 2.8, "sign_light")
    lt.build("TERM_Interior_Lights", coll, root)

    col = Mesh(["band"])
    col.prism(octagon(0.3), 0.0, 3.8, "band", top=False, bottom=False)
    col_me = col.build("COL_Hall", coll, root, merge=True).data
    bpy.data.objects.remove(bpy.data.objects["COL_Hall"])
    for a in [11.5 + 7.5 * i for i in range(11)]:
        for c in (-7.5, -15.0, -22.5, -37.5, -45.0):
            if not (tw[0] - 1 <= a <= tw[1] + 1 and tw[2] - 1 <= c <= tw[3] + 1):
                instance(col_me, "COL_Hall", coll, root, (a, c, 0.0))

    desk = Mesh(["desk", "metal_dark"])
    desk.box(-0.9, 0.9, -0.4, 0.4, 0.0, 1.0, "desk")
    desk.box(-0.95, 0.95, -0.45, 0.45, 1.0, 1.06, "metal_dark")
    desk_me = desk.build("DESK_CheckIn", coll, root).data
    bpy.data.objects.remove(bpy.data.objects["DESK_CheckIn"])
    for k in range(12):
        instance(desk_me, "DESK_CheckIn", coll, root, (22.0 + k * 5.0, -41.0, 0.0))

    bench = Mesh(["metal_dark", "frame"])
    bench.box(-1.5, 1.5, -0.3, 0.3, 0.42, 0.48, "metal_dark")
    bench.box(-1.5, 1.5, 0.25, 0.3, 0.48, 0.9, "metal_dark")
    for x in (-1.3, 1.3):
        bench.box(x - 0.03, x + 0.03, -0.25, 0.25, 0.0, 0.42, "frame")
    bench_me = bench.build("BENCH", coll, root).data
    bpy.data.objects.remove(bpy.data.objects["BENCH"])
    for k in range(10):
        instance(bench_me, "BENCH", coll, root, (16.0 + k * 6.5, -5.5, 0.0), math.pi)

    # canopies: slab, fascia with the lettering, slim round columns
    cp = Mesh(["render", "band", "light"])
    cp.box(10.0, 84.0, 0.0, 4.3, 3.55, 3.85, "band", skip=("front",))
    cp.box(10.0, 84.0, 4.3, 4.6, 3.4, 5.3, "render")
    cp.box(20.0, 70.0, -54.3, -50.0, 3.55, 3.85, "band", skip=("back",))
    cp.box(20.0, 70.0, -54.6, -54.3, 3.4, 5.3, "render")
    for a in range(12, 84, 3):
        cp.face([(a, 1.8, 3.54), (a + 0.4, 1.8, 3.54), (a + 0.4, 2.2, 3.54), (a, 2.2, 3.54)], "light")
    cp.build("TERM_Canopies", coll, root, bevel=0.03)
    cc = Mesh(["galv"])
    cc.prism(octagon(0.14), 0.0, 3.55, "galv", top=False, bottom=False)
    cc_me = cc.build("COL_Canopy", coll, root).data
    bpy.data.objects.remove(bpy.data.objects["COL_Canopy"])
    for a in [10.6 + 7.5 * i for i in range(10)]:
        instance(cc_me, "COL_Canopy", coll, root, (a, 3.9, 0.0))
    for a in [20.6 + 7.2 * i for i in range(7)]:
        instance(cc_me, "COL_Canopy", coll, root, (a, -53.9, 0.0))

    # lettering, as on the photographs: airside and landside fascias
    text_object("ANTWERP AIRPORT", 1.05, 0.08, coll, root, (28.0, 4.64, 4.35), (math.pi / 2, 0.0, math.pi), "letters")
    text_object("ANTWERP AIRPORT", 1.05, 0.08, coll, root, (52.0, -54.64, 4.35), (math.pi / 2, 0.0, 0.0), "letters")
    text_object("LUCHTHAVEN ANTWERPEN", 0.7, 0.06, coll, root, (31.0, -54.63, 4.35), (math.pi / 2, 0.0, 0.0), "letters")

    # panoramic gallery on the roof: glass leaning out over the apron
    g = Mesh(["band", "frame", "glass", "render"])
    lean = 0.7
    g.box(7.6, 34.4, -8.4, 0.3, 11.95, 12.35, "band")
    g.box(8.0, 34.0, -8.0, -7.7, Z_ROOF, 11.95, "render")
    for a in (8.0, 34.0):
        g.face([(a, -7.7, Z_ROOF), (a, -0.9, Z_ROOF), (a, -0.9 + lean, 11.95), (a, -7.7, 11.95)][:: 1 if a > 20 else -1], "glass")
    for k in range(13):
        a = 8.0 + k * 2.0
        g.face([(a, -0.9, Z_ROOF), (a + 2.0, -0.9, Z_ROOF), (a + 2.0, -0.9 + lean, 11.95), (a, -0.9 + lean, 11.95)], "glass")
    g.build("TERM_Gallery", coll, root, bevel=0.02)
    mu = Mesh(["frame"])
    mu.face([(-0.04, -0.9, Z_ROOF), (0.04, -0.9, Z_ROOF), (0.04, -0.9 + lean, 11.95), (-0.04, -0.9 + lean, 11.95)], "frame")
    mu.face([(0.04, -1.0, Z_ROOF), (-0.04, -1.0, Z_ROOF), (-0.04, -1.0 + lean, 11.95), (0.04, -1.0 + lean, 11.95)], "frame")
    mu.face([(0.04, -0.9, Z_ROOF), (0.04, -1.0, Z_ROOF), (0.04, -1.0 + lean, 11.95), (0.04, -0.9 + lean, 11.95)], "frame")
    mu.face([(-0.04, -1.0, Z_ROOF), (-0.04, -0.9, Z_ROOF), (-0.04, -0.9 + lean, 11.95), (-0.04, -1.0 + lean, 11.95)], "frame")
    mu_me = mu.build("MULL_Gallery", coll, root).data
    bpy.data.objects.remove(bpy.data.objects["MULL_Gallery"])
    for k in range(14):
        instance(mu_me, "MULL_Gallery", coll, root, (8.0 + k * 2.0, 0.0, 0.0))
    gl = Mesh(["light", "wood", "metal_dark"])
    for a in range(10, 33, 3):
        gl.face([(a, -5.0, 11.94), (a, -4.6, 11.94), (a + 1.5, -4.6, 11.94), (a + 1.5, -5.0, 11.94)], "light")
    gl.build("TERM_Gallery_Lights", coll, root)
    tb = Mesh(["wood", "metal_dark"])
    tb.prism(octagon(0.45), Z_ROOF + 0.72, Z_ROOF + 0.76, "wood")
    tb.prism(octagon(0.04), Z_ROOF, Z_ROOF + 0.72, "metal_dark", top=False, bottom=False)
    tb_me = tb.build("TABLE", coll, root).data
    bpy.data.objects.remove(bpy.data.objects["TABLE"])
    for k in range(7):
        instance(tb_me, "TABLE", coll, root, (10.5 + k * 3.6, -3.5, 0.0))

    # open roof terrace: glass balustrade on the parapet, benches
    tr = Mesh(["glass", "galv"])
    tr.face([(56.0, -0.2, Z_TOP), (80.0, -0.2, Z_TOP), (80.0, -0.2, Z_TOP + 1.1), (56.0, -0.2, Z_TOP + 1.1)], "glass")
    rail(tr, [(56.0, -0.2), (80.0, -0.2)], Z_TOP, 1.12, closed=False, post_every=2.0)
    tr.build("TERM_Terrace_Balustrade", coll, root)
    for k in range(3):
        instance(bench_me, "BENCH", coll, root, (60.0 + k * 7.0, -4.5, Z_ROOF), math.pi)

    # roof plant: air handling units (instanced), ducts
    hv = Mesh(["galv", "metal_dark"])
    hv.box(-1.2, 1.2, -0.7, 0.7, 0.0, 1.5, "galv")
    hv.box(-0.5, 0.5, -0.4, 0.4, 1.5, 1.62, "metal_dark")
    hv_me = hv.build("HVAC_Unit", coll, root, bevel=0.02).data
    bpy.data.objects.remove(bpy.data.objects["HVAC_Unit"])
    for k, a in enumerate((14.0, 19.0, 24.0, 60.0, 66.0, 72.0, 78.0)):
        instance(hv_me, "HVAC_Unit", coll, root, (a, -42.0, Z_ROOF))

    tower(coll, root, win, W_TOWER, bench_me)
    return coll, root


def tower(coll, root, win, W_TOWER, bench_me, ta=TWR_A, tc=TWR_C, shaft_from=Z_ROOF):
    """The control tower at (ta, tc). The shaft rises from `shaft_from` (the
    roof it stands on, in this tower's own heights) to the observation floor."""
    shaft = [(ta - 5, tc - 5), (ta + 5, tc - 5), (ta + 5, tc + 5), (ta - 5, tc + 5)]  # CCW, side 2 faces the apron

    def plan(side, L):
        return [(x, k, za, zb, W_TOWER) for za, zb in ((9.8, 11.6), (12.6, 14.4)) if za > shaft_from + 0.3 for x, k in bays(1.0, L - 1.0, 5)]
    if shaft_from < 14.4:
        walls("TWR_Shaft_Walls", coll, root, shaft, shaft_from, 15.0, plan, [(14.6, 15.0)], win)

    t = Mesh(["band", "render", "frame", "glass_twr", "metal_dark", "interior", "terrazzo", "galv"])
    # shaft interior backdrop, floors
    t.prism([(ta - 3.5, tc - 3.5), (ta + 3.5, tc - 3.5), (ta + 3.5, tc + 3.5), (ta - 3.5, tc + 3.5)], Z_ROOF, 14.9, "interior", top=False, bottom=False)
    # observation-floor slab with its balcony
    t.box(ta - 6.2, ta + 6.2, tc - 6.2, tc + 6.2, 15.0, 15.35, "band")
    rail(t, [(ta - 6.1, tc - 6.1), (ta + 6.1, tc - 6.1), (ta + 6.1, tc + 6.1), (ta - 6.1, tc + 6.1)], 15.35, 1.1)
    # glazed observation level: an octagon of vertical glass with mullions
    ob_r = 4.6
    oc = octagon(ob_r, cx=ta, cy=tc)
    t.slab([oc], 15.36, "terrazzo")
    for i in range(8):
        (x0, y0), (x1, y1) = oc[i], oc[(i + 1) % 8]
        t.face([(x0, y0, 15.35), (x1, y1, 15.35), (x1, y1, 16.2), (x0, y0, 16.2)], "render")
        t.face([(x0, y0, 16.2), (x1, y1, 16.2), (x1, y1, 18.3), (x0, y0, 18.3)], "glass_twr")
        t.prism([(x0 - 0.06, y0 - 0.06), (x0 + 0.06, y0 - 0.06), (x0 + 0.06, y0 + 0.06), (x0 - 0.06, y0 + 0.06)], 16.2, 18.3, "frame")
    # walkway slab round the cab, railing on its edge
    t.prism(octagon(5.8, cx=ta, cy=tc), 18.3, 18.7, "band")
    rail(t, octagon(5.65, cx=ta, cy=tc), 18.7, 1.1)
    # the cab: dark sill band, glass leaning outwards (about 11 deg, against reflections), top frame
    r0, r1 = 4.0, 4.7
    z_sill, z_glass, z_head = 18.7, 19.4, 22.0

    def ring(r, z):
        return octagon(r, cx=ta, cy=tc), z
    lo_sill, _ = ring(r0, z_sill)
    k_g = (z_glass - z_sill) / (22.3 - z_sill)
    k_h = (z_head - z_sill) / (22.3 - z_sill)
    mid_g = octagon(r0 + (r1 - r0) * k_g, cx=ta, cy=tc)
    mid_h = octagon(r0 + (r1 - r0) * k_h, cx=ta, cy=tc)
    top = octagon(r1, cx=ta, cy=tc)
    t.frustum(lo_sill, mid_g, z_sill, z_glass, "metal_dark")
    t.frustum(mid_g, mid_h, z_glass, z_head, "glass_twr")
    t.frustum(mid_h, top, z_head, 22.3, "metal_dark")
    t.slab([lo_sill], z_sill + 0.01, "terrazzo")
    # corner mullions following the lean
    for i in range(8):
        b0, b1 = Vector((*mid_g[i], z_glass)), Vector((*mid_h[i], z_head))
        n = Vector((b0.x - ta, b0.y - tc, 0.0)).normalized()
        s = Vector((-n.y, n.x, 0.0)) * 0.05
        o = n * 0.04
        q = [b0 - s + o, b0 + s + o, b1 + s + o, b1 - s + o]
        t.face([p.to_tuple() for p in q], "frame")
    # cab roof, plant, antennas
    t.prism(octagon(5.0, cx=ta, cy=tc), 22.3, 22.7, "metal_dark")
    t.box(ta - 1.0, ta + 1.0, tc - 2.5, tc - 1.3, 22.7, 23.3, "galv")
    for (dx, dy, h, r) in ((1.5, 1.5, 4.0, 0.035), (-1.8, 0.8, 3.0, 0.03), (0.2, 2.6, 2.2, 0.05)):
        t.prism(octagon(r, cx=ta + dx, cy=tc + dy), 22.7, 22.7 + h, "galv")
    t.prism(octagon(0.25, cx=ta + 0.2, cy=tc + 2.6), 24.9, 25.0, "galv")       # wind sensor arm
    t.build("TWR_Structure", coll, root, bevel=0.02)

    # consoles round the cab (instanced), their screens lit
    cs = Mesh(["desk", "screen", "metal_dark"])
    cs.box(-0.9, 0.9, -0.35, 0.35, 0.0, 0.85, "metal_dark")
    cs.box(-0.95, 0.95, -0.4, 0.4, 0.85, 0.9, "desk")
    cs.face([(-0.7, -0.15, 0.9), (0.7, -0.15, 0.9), (0.7, -0.05, 1.35), (-0.7, -0.05, 1.35)], "screen")
    cs_me = cs.build("TWR_Console", coll, root).data
    bpy.data.objects.remove(bpy.data.objects["TWR_Console"])
    for i in range(6):
        ang = math.pi / 8 + i * math.pi / 4 - math.pi / 2  # facing the apron and runway side
        r = 3.1
        instance(cs_me, "TWR_Console", coll, root, (ta + r * math.cos(ang), tc + r * math.sin(ang), z_sill), ang - math.pi / 2)

    # red obstacle lights on the cab roof and the tallest mast
    rl = Mesh(["red"])
    for i in range(4):
        ang = math.pi / 4 + i * math.pi / 2
        rl.prism(octagon(0.09, cx=ta + 4.7 * math.cos(ang), cy=tc + 4.7 * math.sin(ang)), 22.7, 22.9, "red")
    rl.prism(octagon(0.08, cx=ta + 1.5, cy=tc + 1.5), 26.7, 26.85, "red")
    rl.build("TWR_Obstacle_Lights", coll, root, smooth=True)

    # the aerodrome control "C" sign: yellow square on the airside face of the shaft
    # (on a measured terminal it hangs on the tower block instead: measured_terminal)
    if shaft_from < 11.0:
        sg = Mesh(["yellow"])
        sg.box(ta - 0.7, ta + 0.7, tc + 5.0, tc + 5.08, 11.65, 12.55, "yellow", skip=("front",))
        sg.build("TWR_Sign_C_Panel", coll, root, bevel=0.01)
        text_object("C", 0.95, 0.04, coll, root, (ta, tc + 5.1, 12.1), (math.pi / 2, 0.0, math.pi), "letters")


def ccw(pts):
    a = sum(pts[i][0] * pts[(i + 1) % len(pts)][1] - pts[(i + 1) % len(pts)][0] * pts[i][1] for i in range(len(pts)))
    return pts if a > 0 else pts[::-1]


def simplify(pts, tol):
    """Drop vertices closer than `tol` to the line of their neighbours (and near-duplicates)."""
    out = list(pts)
    changed = True
    while changed and len(out) > 4:
        changed = False
        for i in range(len(out)):
            a, b, c = Vector((*out[i - 1], 0)), Vector((*out[i], 0)), Vector((*out[(i + 1) % len(out)], 0))
            ac = c - a
            d = (b - a).cross(ac).length / max(ac.length, 1e-6)
            if d < tol or (b - a).length < tol:
                out.pop(i)
                changed = True
                break
    return out


def in_poly(x, y, pts):
    c = False
    j = len(pts) - 1
    for i in range(len(pts)):
        (xi, yi), (xj, yj) = pts[i], pts[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            c = not c
        j = i
    return c


def block_height_at(tv, x, y):
    return max((b["h"] for b in tv["blocks"] if in_poly(x, y, b["poly"])), default=0.0)


def measured_terminal(coll, win, root, mods, tv):
    """Each measured block: walls with a ribbon of windows a storey (glazed down
    to the ground where it faces the apron), a flat roof behind its parapet,
    a floor and lit ceilings inside; canopies with the lettering along the
    longest low airside and landside walls; roof plant."""
    apron = Vector((C.x, C.y, 0.0))
    world = bpy.data.objects.new("TERMINAL_Blocks", None)   # the EBAW frame itself
    coll.objects.link(world)
    rot = next((b for b in LAYOUT["buildings"] if b["kind"] == "rotunda"), None)
    slabs = Mesh(["gravel", "membrane", "terrazzo", "ceiling", "light"])
    edges = []
    for i, b in enumerate(tv["blocks"]):
        pts = ccw(simplify([tuple(p) for p in b["poly"]], 0.7))
        if len(pts) < 3:
            continue
        h = b["h"]
        storeys = max(1, round(h / 3.8))
        sh = h / storeys
        n = len(pts)
        is_rot = rot is not None and in_poly(rot["x"], rot["y"], pts)

        def plan(side, L, pts=pts, sh=sh, storeys=storeys):
            p0, p1 = Vector((*pts[side], 0)), Vector((*pts[(side + 1) % len(pts)], 0))
            d = (p1 - p0).normalized()
            facing = Vector((d.y, -d.x, 0.0)).dot(apron)
            if L < 3.4:
                return []
            res = []
            for k in range(storeys):
                z = k * sh
                if k == 0 and facing > 0.5 and sh >= 3.9:
                    res += [(x, kk, 0.05, 3.45, mods["curtain"]) for x, kk in bays(1.0, L - 1.0, 4, 0.6)]
                elif sh >= 3.0:
                    za = z + max(0.9, (sh - 2.0) * 0.45)
                    res += [(x, kk, za, za + 2.0, mods["ribbon"]) for x, kk in bays(0.8, L - 0.8, 6, 0.5)]
            return res
        bands = [(k * sh - 0.2, k * sh + 0.2) for k in range(1, storeys)] + [(h + 0.3, h + 0.6)]
        walls(f"TERM_Block_{i:02d}", coll, world, pts, 0.0, h + 0.6, plan, bands, win)
        slabs.slab([pts], h, "membrane" if is_rot else "gravel")
        slabs.slab([pts], 0.02, "terrazzo")
        for k in range(storeys):
            slabs.slab([pts], (k + 1) * sh - 0.35, "ceiling", up=False)
        # ceiling lights on the ground floor, a 4 m grid
        xs, ys = [p[0] for p in pts], [p[1] for p in pts]
        x = min(xs) + 2.0
        while x < max(xs):
            y = min(ys) + 2.0
            while y < max(ys):
                if in_poly(x, y, pts) and in_poly(x + 1.2, y + 0.6, pts):
                    z = sh - 0.36
                    slabs.face([(x, y, z), (x, y + 0.6, z), (x + 1.2, y + 0.6, z), (x + 1.2, y, z)], "light")
                y += 4.0
            x += 4.0
        for k in range(n):
            p0, p1 = Vector((*pts[k], 0)), Vector((*pts[(k + 1) % n], 0))
            edges.append((p0, p1, h, i))
    slabs.build("TERM_Floors_Roofs", coll, world)

    def outer(p0, p1, me):
        """Whether the wall p0-p1 faces outdoors (no other block just beyond it)."""
        mid = (p0 + p1) / 2
        d = (p1 - p0).normalized()
        q = mid + Vector((d.y, -d.x, 0.0)) * 1.5
        return not any(in_poly(q.x, q.y, b["poly"]) for j, b in enumerate(tv["blocks"]) if j != me)

    def facing(p0, p1):
        d = (p1 - p0).normalized()
        return Vector((d.y, -d.x, 0.0)).dot(apron)
    low = [e for e in edges if e[2] <= 8.5 and (e[1] - e[0]).length > 10.0 and outer(e[0], e[1], e[3])]
    air = max((e for e in low if facing(e[0], e[1]) > 0.6), key=lambda e: (e[1] - e[0]).length, default=None)
    land = max((e for e in low if facing(e[0], e[1]) < -0.6), key=lambda e: (e[1] - e[0]).length, default=None)
    cc = Mesh(["galv"])
    cc.prism(octagon(0.14), 0.0, 3.4, "galv", top=False, bottom=False)
    cc_ob = cc.build("tmp_col", coll, None)
    col_me = cc_ob.data
    col_me.name = "COL_Canopy"
    bpy.data.objects.remove(cc_ob)
    for side, e, words in (("Air", air, ["ANTWERP AIRPORT"]), ("Land", land, ["LUCHTHAVEN ANTWERPEN", "ANTWERP AIRPORT"])):
        if e is None:
            continue
        p0, p1, h, _ = e
        L = (p1 - p0).length
        d = (p1 - p0).normalized()
        fr = bpy.data.objects.new(f"TERM_Canopy_{side}_Frame", None)
        coll.objects.link(fr)
        fr.location = p0
        fr.rotation_euler = (0.0, 0.0, math.atan2(d.y, d.x))   # local x along the wall, local -y out of it
        top = min(4.0, h - 0.5)
        cp = Mesh(["render", "band", "light"])
        cp.box(0.5, L - 0.5, -3.2, 0.0, top - 0.3, top, "band", skip=("back",))
        cp.box(0.5, L - 0.5, -3.5, -3.2, top - 0.45, top + 1.1, "render")
        x = 1.5
        while x < L - 1.5:
            cp.face([(x, -1.8, top - 0.31), (x + 0.4, -1.8, top - 0.31), (x + 0.4, -1.4, top - 0.31), (x, -1.4, top - 0.31)][::-1], "light")
            x += 3.0
        cp.build(f"TERM_Canopy_{side}", coll, fr, bevel=0.03)
        k = max(2, int(L // 7.5) + 1)
        for j in range(k):
            instance(col_me, "COL_Canopy", coll, fr, (0.8 + (L - 1.6) * j / (k - 1), -3.0, 0.0))
        for j, w in enumerate(words):
            size = 0.95 if w == "ANTWERP AIRPORT" else 0.65
            xpos = L * (0.5 if len(words) == 1 else (0.3 + 0.4 * j))
            t = text_object(w, size, 0.07, coll, fr, (xpos, -3.54, top + 0.32), (math.pi / 2, 0.0, 0.0), "letters")
            t.name = f"Sign_{side}_{j}"
    # roof plant on the larger high roofs
    hv = Mesh(["galv", "metal_dark"])
    hv.box(-1.2, 1.2, -0.7, 0.7, 0.0, 1.5, "galv")
    hv.box(-0.5, 0.5, -0.4, 0.4, 1.5, 1.62, "metal_dark")
    hv_ob = hv.build("tmp_hv", coll, None, bevel=0.0)
    hv_me = hv_ob.data
    hv_me.name = "HVAC_Unit"
    bpy.data.objects.remove(hv_ob)
    for i, b in enumerate(tv["blocks"]):
        if b["area"] > 450 and b["h"] > 7.0:
            cx = sum(p[0] for p in b["poly"]) / len(b["poly"])
            cy = sum(p[1] for p in b["poly"]) / len(b["poly"])
            for k in range(3):
                x, y = cx + (k - 1) * 3.2, cy
                if in_poly(x, y, b["poly"]):
                    instance(hv_me, "HVAC_Unit", coll, world, (x, y, b["h"]))
    # the aerodrome control "C" sign on the tower block's apron face
    t = tv["tower"]
    p = Vector((t["x"], t["y"], 0.0))
    hb = next((b for b in tv["blocks"] if in_poly(p.x, p.y, b["poly"])), None)
    if hb is not None:
        q = p.copy()
        while in_poly(q.x, q.y, hb["poly"]) and (q - p).length < 40:
            q += apron * 0.25
        rel = q - O
        sa, sc = rel.dot(A), rel.dot(C)
        sg = Mesh(["yellow"])
        sg.box(sa - 0.7, sa + 0.7, sc, sc + 0.08, hb["h"] - 2.6, hb["h"] - 1.7, "yellow")
        sg.build("TWR_Sign_C_Panel", coll, root, bevel=0.01)
        text_object("C", 0.95, 0.04, coll, root, (sa, sc + 0.1, hb["h"] - 2.15), (math.pi / 2, 0.0, math.pi), "letters")


def shots():
    def at(a, c, z):
        return W(a, c, z).to_tuple()
    return {
        "airside": (at(62.0, 95.0, 1.7), at(44.0, -12.0, 9.0), 26.0),
        "aerial": (at(150.0, 140.0, 85.0), at(45.0, -25.0, 6.0), 30.0),
        "landside": (at(28.0, -125.0, 1.8), at(46.0, -40.0, 8.0), 26.0),
        "tower": (at(62.0, -10.0, 25.0), at(TWR_A, TWR_C, 19.5), 35.0),
    }


def main():
    background = bpy.app.background
    opt = args(render="/tmp/ebaw_step2.png", shot="airside")
    if "01_Ground_Layout" not in bpy.data.collections:
        saved = sys.argv
        sys.argv = [saved[0], "--", "--render", "none"]
        runpy.run_path(str(HERE / "01_ground_layout.py"), run_name="__main__")
        sys.argv = saved
    coll, root = build()
    objs = [o for o in coll.all_objects if o.type == "MESH"]
    inst = sum(1 for o in objs if o.data.users > 1)
    print(f"02_Terminal_ATC: {len(objs)} mesh objects ({inst} instances of shared meshes), "
          f"{sum(len(o.data.polygons) for o in {o.data: o for o in objs}.values())} unique faces")
    prev = bpy.data.collections.get("99_Preview") or collection("99_Preview")
    loc, tgt, lens = shots().get(opt["shot"], shots()["airside"])
    cam = camera("CheckCam_Terminal", prev, loc, tgt, lens)
    if background:
        check_render(opt, cam)
    else:
        bpy.context.scene.camera = cam


main()
