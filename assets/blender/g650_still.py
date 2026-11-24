# The G650's picture for the start screen (assets/aircraft/g650.png): painted,
# as the other Blender-built types' stills are. Rendered with Cycles, the paint
# (assets/g650_paint.jpg) projected onto the skin as the game's plane.wgsl
# (`hornet_paint`) projects it: four orthographic pictures, top, bottom and
# each side, weighted by the rest normal. The gear is down, with its doors as
# the game has them then (the nose well's forward doors shut). Run it on the
# saved model, after g650.py:
#
#   blender -b assets/blender/g650.blend --python assets/blender/g650_still.py -- assets/aircraft/g650.png
#
# 680 x 392 RGBA on a clear background. (The light is a sky and a sun; the
# image is rendered linear and tone mapped here, so it does not depend on
# Blender's colour management.)
import bpy, sys, math, os
import numpy as np
from mathutils import Vector as V

HERE = os.path.dirname(os.path.abspath(__file__))
args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
OUT = os.path.abspath(args[0]) if args else os.path.join(os.path.dirname(HERE), "aircraft", "g650.png")
SRC = os.path.join(HERE, "g650.py")
PAINT = os.path.join(os.path.dirname(HERE), "g650_paint.jpg")
EYE, LOOK, LENS = V((30, 22, 19)), V((-0.4, -2.4, 0.0)), 40.0
SAMPLES = 256
SKY, SUN, EXPOSURE = 0.10, 2.5, -1.2

code = open(SRC).read().replace("\nmain()\n", "\n")
ns = {"__name__": "g650mod", "__file__": SRC}
argv0 = sys.argv
sys.argv = [sys.argv[0], "--", "/tmp/unused"]
exec(compile(code, SRC, "exec"), ns)
sys.argv = argv0
COLL = bpy.data.collections["G650"]
obs = {o.name: o for o in COLL.all_objects}
EXPECTED = ns["EXPECTED"]
MATS = ns["MATS"]

# Only the pieces the game draws.
for o in bpy.data.objects:
    if o.type == "MESH":
        o.hide_render = o.name not in EXPECTED
# Gear down, the doors as the game has them then.
for o in obs.values():
    if o.name in EXPECTED and (o.get("parent") or "gear door" in o.name or o.name.endswith(" leg")):
        o.matrix_world = ns["gear_world"](obs, o.name, 1.0)

# ---- the paint (aircraft::Paint of src/aircraft/g650/mod.rs and render.rs)
SPAN, LENGTH = 31.4, 31.8
CG_D, CG_H = ns["CG_D"], ns["CG_H"]
CENTRE = (CG_D - 15.2, 3.9 - CG_H)
OFF = (SPAN * 0.5, CENTRE[0] + LENGTH * 0.5, SPAN * 0.5 - CENTRE[1])
# The paint sheet's four pictures side by side (top, bottom, left, right): one image each.
sheet = bpy.data.images.load(PAINT)
sw, sh = sheet.size
sp = np.array(sheet.pixels[:], dtype=np.float32).reshape(sh, sw, 4)
tw = sw // 4
imgs = []
for i in range(4):
    im = bpy.data.images.new(f"paint{i}", tw, sh, alpha=False)
    im.colorspace_settings.name = sheet.colorspace_settings.name
    im.pixels[:] = np.ascontiguousarray(sp[:, i * tw:(i + 1) * tw]).ravel()
    imgs.append(im)


def paint_material(m, base_col):
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    N = nt.nodes.new
    L = nt.links.new
    out = N("ShaderNodeOutputMaterial")
    bsdf = N("ShaderNodeBsdfPrincipled")
    L(bsdf.outputs[0], out.inputs[0])
    tc = N("ShaderNodeTexCoord")
    pos = N("ShaderNodeSeparateXYZ")
    L(tc.outputs["Object"], pos.inputs[0])
    nrm = N("ShaderNodeSeparateXYZ")
    L(tc.outputs["Normal"], nrm.inputs[0])

    def math(op, a, b=None, c=None):
        n = N("ShaderNodeMath")
        n.operation = op
        for k, x in enumerate((a, b, c)):
            if x is None:
                continue
            if isinstance(x, (int, float)):
                n.inputs[k].default_value = x
            else:
                L(x, n.inputs[k])
        return n.outputs[0]

    # game L = (bx, bz, -by); v = (L.z + off.y) / length, Blender's image v = 1 - v
    gx, gy, gz = pos.outputs[0], pos.outputs[2], math("MULTIPLY", pos.outputs[1], -1.0)
    vimg = math("SUBTRACT", 1.0, math("DIVIDE", math("ADD", gz, OFF[1]), LENGTH))
    us = [math("DIVIDE", math("ADD", gx, OFF[0]), SPAN), math("DIVIDE", math("SUBTRACT", OFF[0], gx), SPAN), math("DIVIDE", math("ADD", gy, OFF[2]), SPAN)]
    nx, ny = nrm.outputs[0], nrm.outputs[2]
    w = [math("POWER", math("MAXIMUM", ny, 0.0), 4.0), math("POWER", math("MAXIMUM", math("MULTIPLY", ny, -1.0), 0.0), 4.0), math("POWER", math("ABSOLUTE", nx), 4.0)]
    total = math("ADD", math("ADD", w[0], w[1]), w[2])
    cols = []
    for layer, u in ((0, us[0]), (1, us[1]), (2, us[2]), (3, us[2])):
        uv = N("ShaderNodeCombineXYZ")
        L(u, uv.inputs[0])
        L(vimg, uv.inputs[1])
        tex = N("ShaderNodeTexImage")
        tex.image = imgs[layer]
        tex.interpolation = "Cubic"
        tex.extension = "EXTEND"
        L(uv.outputs[0], tex.inputs[0])
        cols.append(tex.outputs[0])
    # the side picture: the left one, or the right one where the surface faces right
    side = N("ShaderNodeMix")
    side.data_type = "RGBA"
    L(math("GREATER_THAN", nx, 0.0), side.inputs[0])
    L(cols[2], side.inputs[6])
    L(cols[3], side.inputs[7])

    def scaled(col, wt):
        n = N("ShaderNodeVectorMath")
        n.operation = "SCALE"
        L(col, n.inputs[0])
        L(math("DIVIDE", wt, math("MAXIMUM", total, 1e-6)), n.inputs[3])
        return n.outputs[0]

    a = N("ShaderNodeVectorMath")
    a.operation = "ADD"
    L(scaled(cols[0], w[0]), a.inputs[0])
    L(scaled(cols[1], w[1]), a.inputs[1])
    b = N("ShaderNodeVectorMath")
    b.operation = "ADD"
    L(a.outputs[0], b.inputs[0])
    L(scaled(side.outputs[2], w[2]), b.inputs[1])
    # where no picture faces the surface: the vertex colour (the material's)
    fb = N("ShaderNodeMix")
    fb.data_type = "RGBA"
    L(math("GREATER_THAN", total, 0.02), fb.inputs[0])
    fb.inputs[6].default_value = (*base_col, 1.0)
    L(b.outputs[0], fb.inputs[7])
    L(fb.outputs[2], bsdf.inputs["Base Color"])
    bsdf.inputs["Roughness"].default_value = 0.28
    bsdf.inputs["Coat Weight"].default_value = 0.35
    bsdf.inputs["Coat Roughness"].default_value = 0.08


def plain_material(m, name, col):
    m.use_nodes = True
    nt = m.node_tree
    bsdf = nt.nodes.get("Principled BSDF") or nt.nodes.new("ShaderNodeBsdfPrincipled")
    out = nt.nodes.get("Material Output") or nt.nodes.new("ShaderNodeOutputMaterial")
    nt.links.new(bsdf.outputs[0], out.inputs[0])
    bsdf.inputs["Base Color"].default_value = (*col, 1.0)
    gid = MATS[name][0]
    rough, metal = 0.5, 0.0
    if name in ("chrome", "lip", "hub", "fan", "steel"):
        rough, metal = (0.12 if name == "chrome" else 0.3), 1.0
    elif name == "glass":
        rough = 0.03
        bsdf.inputs["Base Color"].default_value = (0.012, 0.014, 0.018, 1.0)
        bsdf.inputs["Specular IOR Level"].default_value = 0.35
    elif name in ("rubber", "tread", "frame", "inlet", "spinner", "cascade", "carpet"):
        rough = 0.8
    elif name in ("strut", "door_in", "well", "cabin"):
        rough = 0.4
    elif gid == 8:
        bsdf.inputs["Emission Color"].default_value = (*col, 1.0)
        bsdf.inputs["Emission Strength"].default_value = 2.0
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal


for m in bpy.data.materials:
    name = m.get("game")
    if name not in MATS:
        continue
    col = MATS[name][2]
    if name == "skin":
        paint_material(m, col)
    else:
        plain_material(m, name, col)

# ---- light and camera
sc = bpy.context.scene
sc.render.engine = "CYCLES"
sc.cycles.device = "CPU"  # (this Blender has no CUDA kernels built)
sc.cycles.samples = SAMPLES
sc.cycles.use_denoising = True
sc.render.film_transparent = True
sc.render.resolution_x, sc.render.resolution_y = 680, 392
sc.render.resolution_percentage = 100
world = bpy.data.worlds.new("sky")
sc.world = world
world.use_nodes = True
wn = world.node_tree
sky = wn.nodes.new("ShaderNodeTexSky")
try:
    sky.sky_type = "NISHITA"
except Exception:
    pass
sky.sun_elevation = math.radians(48)
try:
    sky.sun_disc = False
except Exception:
    pass
sky.sun_rotation = math.radians(150)
bg = wn.nodes["Background"]
bg.inputs[1].default_value = SKY
wn.links.new(sky.outputs[0], bg.inputs[0])
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.data.energy = SUN
sun.data.angle = math.radians(1.0)
sun.data.color = (1.0, 0.97, 0.92)
sc.collection.objects.link(sun)
sun.rotation_euler = (V((0.45, 0.55, 0.70)).normalized() * -1).to_track_quat("-Z", "Y").to_euler()
cam_data = bpy.data.cameras.new("cam")
cam = bpy.data.objects.new("cam", cam_data)
sc.collection.objects.link(cam)
sc.camera = cam
cam_data.lens = LENS
cam_data.clip_end = 1000.0
cam.location = EYE
cam.rotation_euler = (LOOK - EYE).to_track_quat("-Z", "Y").to_euler()
# Rendered to a linear EXR (whatever the view transform) and tone mapped
# here, so the result does not depend on the local OCIO configuration.
sc.render.image_settings.file_format = "OPEN_EXR"
sc.render.image_settings.color_mode = "RGBA"
sc.render.image_settings.color_depth = "32"
import shutil, tempfile
TMP = tempfile.mkdtemp()
EXR = os.path.join(TMP, "g650_still.exr")
sc.render.filepath = EXR
bpy.ops.render.render(write_still=True)
img = bpy.data.images.load(EXR)
w, h = img.size
px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)
bpy.data.images.remove(img)
shutil.rmtree(TMP, ignore_errors=True)
a = px[..., 3:4]
rgb = px[..., :3] / np.maximum(a, 1e-6) * 2.0 ** EXPOSURE
# ACES (Narkowicz) curve, then sRGB
x = rgb
t = np.clip((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 0, 1)
srgb = np.where(t <= 0.0031308, 12.92 * t, 1.055 * np.power(t, 1 / 2.4) - 0.055)
outp = np.concatenate([srgb, np.clip(a, 0, 1)], axis=2)
o = bpy.data.images.new("still", w, h, alpha=True)
o.pixels[:] = outp.ravel()
o.filepath_raw = OUT
o.file_format = "PNG"
o.save()
print("STILL", OUT)
