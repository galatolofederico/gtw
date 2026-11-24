"""EBAW (Antwerp International Airport) - step 5b: checks and modular glTF export.

Opens the built scene (ebaw.blend, steps 1-5), checks it, bakes the procedural
materials into textures and exports one glTF per module:

    export/ebaw_01_ground_layout.gltf   + .bin
    export/ebaw_02_terminal_atc.gltf
    export/ebaw_03_hangars.gltf
    export/ebaw_04_airside_props.gltf
    export/ebaw_05_lighting.gltf
    export/textures/                    shared by all modules
    export/ebaw_manifest.json           frame, modules, animated nodes, checks

    blender -b assets/blender/ebaw/ebaw.blend --python assets/blender/ebaw/06_export.py -- \
        [--out assets/blender/ebaw/export] [--format gltf|glb]

The .blend file is never saved by this script: the materials are rewritten
in memory for the export only.

Checks (printed and written to the manifest):
  * transforms: no object carries a scale other than 1 (nothing to apply:
    every mesh is built at 1:1 in metres); instances keep their rotation and
    location, which the export keeps as nodes (EXT_mesh_gpu_instancing)
  * meshes: Blender's validation (degenerate or duplicate geometry fixed and
    counted), zero-area faces, every mesh has UVs
  * normals: the ground faces all point up; every closed mesh encloses a
    positive volume (outward normals)
  * modifiers (wall thickness, chamfers) are applied in the export

Materials: every procedural material is baked to a swatch covering one UV
tile (10 m on the ground meshes, 2 m on the buildings and props, 1 m for the
fence fabric): base colour (sRGB JPEG), occlusion-roughness-metallic (JPEG)
and a tangent-space normal map (PNG); alpha for the chain-link fence (PNG).
Emissive lenses and lamps keep their colour and strength
(KHR_materials_emissive_strength), glass its transmission. The swatches are
not seamless: the procedural noise does not repeat, so a faint seam can show
at tile edges.

glTF frame: the exporter turns Blender's Z-up into glTF's Y-up: +X east,
+Y up, -Z north, metres, origin at the runway midpoint (see the manifest
for its latitude, longitude and elevation). That is FableFlight's own frame
convention (+X right, +Y up, -Z forward).

Animated in the game, not in the file: the windsocks. Each WINDSOCK_n_Sock
node has its pivot at the mouth of the sock and its +Z (Blender +Y, the
sock's length) along the sock; turn it about the vertical to point downwind
and pitch it down in light wind, as src/world.rs already does for its own
socks. The PAPI units carry their setting angle (extras: papi_setting_deg).
"""

import json
import math
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Vector

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from ebaw_lib import box_uv  # noqa: E402
from ebaw_bake import bake_all, module_objects  # noqa: E402

MODULES = ["01_Ground_Layout", "02_Terminal_ATC", "03_Hangars", "04_Airside_Props", "05_Lighting"]
LAYOUT = json.loads((HERE / "ebaw_layout.json").read_text())


def opts():
    a = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    o = {"out": str(HERE / "export"), "format": "gltf"}
    for i in range(0, len(a) - 1, 2):
        o[a[i].lstrip("-")] = a[i + 1]
    return o


# ---------------------------------------------------------------- checks

def check(report):
    dg = bpy.context.evaluated_depsgraph_get()
    seen = set()
    for mod in MODULES:
        r = {"objects": 0, "instances": 0, "triangles": 0, "scaled": [], "fixed_by_validate": 0,
             "zero_area_faces": 0, "no_uv": 0, "ground_faces_down": 0, "closed_inside_out": [], "lights": 0}
        for ob in module_objects(mod):
            r["objects"] += 1
            if ob.type == "LIGHT":
                r["lights"] += 1
            if any(abs(s - 1.0) > 1e-6 for s in ob.scale):
                r["scaled"].append(ob.name)
            if ob.type != "MESH":
                continue
            me = ob.data
            if me.users > 1:
                r["instances"] += 1
            ev = ob.evaluated_get(dg)
            em = ev.to_mesh()
            r["triangles"] += sum(len(p.vertices) - 2 for p in em.polygons)
            ev.to_mesh_clear()
            if me.name in seen:
                continue
            seen.add(me.name)
            if me.validate(verbose=False):
                r["fixed_by_validate"] += 1
            if not me.uv_layers:
                box_uv(me)
                r["no_uv"] += 1
            zero = sum(1 for p in me.polygons if p.area < 1e-8)
            if zero:  # slivers left where facade openings meet: dissolve them
                bm = bmesh.new()
                bm.from_mesh(me)
                bmesh.ops.dissolve_degenerate(bm, dist=1e-5, edges=bm.edges)
                bm.to_mesh(me)
                bm.free()
                zero_after = sum(1 for p in me.polygons if p.area < 1e-8)
                r["zero_area_faces"] += zero
                r["zero_area_left"] = r.get("zero_area_left", 0) + zero_after
            if mod == "01_Ground_Layout":
                r["ground_faces_down"] += sum(1 for p in me.polygons if p.normal.z < 0.0)
            # closed meshes (no boundary edges, no modifiers): outward normals enclose a positive volume
            if not ob.modifiers:
                bm = bmesh.new()
                bm.from_mesh(me)
                if bm.edges and all(e.is_manifold for e in bm.edges):
                    if bm.calc_volume(signed=True) < 0:
                        r["closed_inside_out"].append(me.name)
                        bmesh.ops.reverse_faces(bm, faces=bm.faces)
                        bm.to_mesh(me)
                bm.free()
        r["closed_inside_out_fixed"] = len(r["closed_inside_out"])
        r["closed_inside_out"] = r["closed_inside_out"][:10]
        report[mod] = r
        print(f"{mod}: {r['objects']} objects ({r['instances']} instances), {r['triangles']} triangles, "
              f"scaled {len(r['scaled'])}, validate fixes {r['fixed_by_validate']}, zero-area {r['zero_area_faces']} "
              f"(left {r.get('zero_area_left', 0)}), "
              f"no UV {r['no_uv']}, ground faces down {r['ground_faces_down']}, inside-out fixed {r['closed_inside_out_fixed']}")


def rewrite(done):
    """Turn every baked material into a plain textured Principled the glTF exporter understands."""
    for name, info in done.items():
        mat = bpy.data.materials[name]
        nt = mat.node_tree
        nt.nodes.clear()
        out = nt.nodes.new("ShaderNodeOutputMaterial")
        b = nt.nodes.new("ShaderNodeBsdfPrincipled")
        nt.links.new(b.outputs["BSDF"], out.inputs["Surface"])
        c = info["const"]
        b.inputs["Base Color"].default_value = c["Base Color"]
        b.inputs["Roughness"].default_value = c["Roughness"]
        b.inputs["Metallic"].default_value = c["Metallic"]
        b.inputs["Alpha"].default_value = c["Alpha"]
        b.inputs["Transmission Weight"].default_value = info["transmission"]
        if info.get("emission"):
            col, strength = info["emission"]
            b.inputs["Emission Color"].default_value = (*col, 1.0)
            b.inputs["Emission Strength"].default_value = strength
        if info.get("base"):
            t = nt.nodes.new("ShaderNodeTexImage")
            t.image = bpy.data.images.load(info["base"], check_existing=True)
            nt.links.new(t.outputs["Color"], b.inputs["Base Color"])
            if info["base"].endswith(".png") and c is not None:
                nt.links.new(t.outputs["Alpha"], b.inputs["Alpha"])
                try:
                    mat.surface_render_method = "DITHERED"
                except (AttributeError, TypeError):
                    pass
        if info.get("orm"):
            t = nt.nodes.new("ShaderNodeTexImage")
            t.image = bpy.data.images.load(info["orm"], check_existing=True)
            t.image.colorspace_settings.name = "Non-Color"
            sp = nt.nodes.new("ShaderNodeSeparateColor")
            nt.links.new(t.outputs["Color"], sp.inputs["Color"])
            nt.links.new(sp.outputs["Green"], b.inputs["Roughness"])
            nt.links.new(sp.outputs["Blue"], b.inputs["Metallic"])
        if info.get("normal"):
            t = nt.nodes.new("ShaderNodeTexImage")
            t.image = bpy.data.images.load(info["normal"], check_existing=True)
            t.image.colorspace_settings.name = "Non-Color"
            nm = nt.nodes.new("ShaderNodeNormalMap")
            nt.links.new(t.outputs["Color"], nm.inputs["Color"])
            nt.links.new(nm.outputs["Normal"], b.inputs["Normal"])


# ---------------------------------------------------------------- export

def export(out, fmt, report):
    out.mkdir(parents=True, exist_ok=True)
    vl = bpy.context.view_layer
    files = {}
    for mod in MODULES:
        lc = next(c for c in vl.layer_collection.children if c.name == mod)
        vl.active_layer_collection = lc
        stem = "ebaw_" + mod.lower()
        path = out / (stem + (".glb" if fmt == "glb" else ".gltf"))
        kw = dict(filepath=str(path), export_format="GLB" if fmt == "glb" else "GLTF_SEPARATE",
                  use_active_collection=True, use_active_collection_with_nested=True,
                  export_apply=True, export_gpu_instances=True, export_lights=(mod == "05_Lighting"),
                  export_extras=True, export_yup=True, export_cameras=False, export_image_format="AUTO")
        if fmt != "glb":
            kw["export_texture_dir"] = "textures"
        bpy.ops.export_scene.gltf(**kw)
        files[mod] = path.name
        print("exported", path)
    fr = LAYOUT["frame"]
    socks = [{"node": o.name, "pivot_blender_xyz": [round(v, 3) for v in o.location]}
             for o in bpy.data.objects if o.name.startswith("WINDSOCK_") and o.name.endswith("_Sock")]
    manifest = {
        "airport": "EBAW Antwerp International",
        "frame": {"origin_lat": fr["origin_lat"], "origin_lon": fr["origin_lon"], "elevation_m": fr["elevation_m"],
                  "gltf_axes": "+X east, +Y up, -Z north (metres)", "blender_axes": "+X east, +Y north, +Z up"},
        "modules": files,
        "animated": {"windsocks": socks, "rule": "point downwind; droop when the wind is under about 15 kt (as src/world.rs)"},
        "checks": report,
        "sources": ["OpenStreetMap contributors, ODbL (layout, footprints)", "ICAO Annex 14 vol. I (markings, lights)",
                    "photographs of the terminal (see 02_terminal_atc.py)"],
    }
    (out / "ebaw_manifest.json").write_text(json.dumps(manifest, indent=1))
    return files


def main():
    o = opts()
    out = Path(o["out"]).resolve()
    report = {}
    # the check renders' helpers stay out of the export
    prev = bpy.data.collections.get("99_Preview")
    if prev is not None:
        for lc in bpy.context.view_layer.layer_collection.children:
            if lc.name == "99_Preview":
                lc.exclude = True
    check(report)
    done = bake_all(out / "textures", MODULES)
    rewrite(done)
    export(out, o["format"], report)


main()
