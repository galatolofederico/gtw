"""EBAW (Antwerp International Airport) - the game's scenery pack.

Turns the built scene (ebaw.blend: the terminal and tower, the hangars, the
airside props, and the floodlight masts) into assets/scenery/ebaw/pack.bin.gz,
which src/scenery.rs reads and the textured scenery pipeline
(src/shaders/scenery.wgsl) draws. The ground (01) and the lights (05) are not
in it: the game draws its own pavement, paint and aeronautical lights.

    blender -b assets/blender/ebaw/ebaw.blend --python assets/blender/ebaw/07_game_pack.py -- \
        [--out assets/scenery/ebaw] [--tex 256]

The .blend is never saved: chamfers are cut to one segment for the game in
memory only.

Format FFSC version 3, little-endian, gzip:
  "FFSC" u32 version
  f64 origin_lat, f64 origin_lon (degrees), f32 elevation (m)     the frame: x east, y north, z up, metres
  u32 flags                                                       procedural objects the pack replaces: 1 windsocks, 2 floodlight masts
  u32 tex_size, u32 n_layers, then per layer:
      u32 len, bytes (base colour + alpha: JPEG or PNG), u32 len, bytes (PNG RGBA: normal x, y, roughness, metallic)
  u32 n_materials, per material (16 x 4 bytes):
      f32 base rgba, f32 roughness, f32 metallic, f32 emission rgb, f32 emission strength,
      f32 transmission, f32 tile (metres per texture repeat), i32 layer (-1 none),
      u32 flags (1 glass, 2 alpha cut-out, 4 lit at night, 8 takes the instance's tint), f32 pad
  u32 n_meshes, per mesh:
      u32 name length, name, f32 radius (from its origin),
      u32 n_vertices, vertices of 32 bytes: f32 pos[3], i8 normal[3] + ambient occlusion (0..127),
          i8 tangent[4] (w = bitangent sign),
          f32 uv[2] (metres), u32 material
      u32 n_indices, u32 indices
  u32 n_instances, per instance: u32 mesh, u32 class (0 building, 1 prop, 2 windsock, 3 small prop),
      f32 anchor[2] (where it stands on the ground), f32 matrix[12] (3 rows of 4, pack frame),
      f32 tint[3] (multiplies the materials flagged 8: a roof's colour from the orthophoto)
  u32 n_floods, per floodlight: f32 pos[3], f32 dir[3]
  u32 n_masks, per mask: u32 n, f32 xy[2 n]     footprints (pack frame) where OpenStreetMap's own
                                                 buildings give way to the pack's
  u32 n_ground, per patch: u32 kind (1 concrete), f32 slab direction (radians from east),
      u32 n_rings, per ring: u32 n, f32 xy[2 n]  (outer ring first, then holes; pack frame)
                                                 the aprons' concrete pads, seen on the orthophoto

A windsock's mesh hangs from its origin along +Y; the game turns it with the
wind (world.rs, as its own socks).
"""

import gzip
import io
import json
import math
import struct
import sys
from pathlib import Path

import bpy
import numpy as np
from mathutils import Vector

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from ebaw_bake import bake_all, module_objects  # noqa: E402
from ebaw_lib import box_uv  # noqa: E402

LAYOUT = json.loads((HERE / "ebaw_layout.json").read_text())
MODULES = ["02_Terminal_ATC", "03_Hangars", "04_Airside_Props"]
F_SOCKS, F_MASTS = 1, 2


def opts():
    a = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    o = {"out": str(HERE.parent.parent / "scenery" / "ebaw"), "tex": "256"}
    for i in range(0, len(a) - 1, 2):
        o[a[i].lstrip("-")] = a[i + 1]
    return o


def objects():
    """(object, class) for everything that goes in the pack."""
    out = []
    for mod in MODULES:
        for ob in module_objects(mod):
            if ob.type != "MESH":
                continue
            n = ob.name
            if n.startswith("WINDSOCK_") and n.endswith("_Sock"):
                cls = 2
            elif mod == "04_Airside_Props" and (n.startswith(("FENCE_Post", "PROP_"))):
                cls = 3
            elif mod == "04_Airside_Props":
                cls = 1
            else:
                cls = 0
            out.append((ob, cls))
    for ob in module_objects("05_Lighting"):
        if ob.type == "MESH" and ob.name.startswith("MAST_Flood"):
            out.append((ob, 0))
    return out


def anchor(ob):
    """Where an object stands: its outermost parent's position (a building's
    parts all sit on the ground under the building's own origin)."""
    r = ob
    while r.parent is not None:
        r = r.parent
    return (r.matrix_world.translation.x, r.matrix_world.translation.y)


def layers(done, size, texdir):
    """Texture layers for the baked materials: base (JPEG, or PNG with alpha), normal+roughness+metallic (PNG)."""
    out, index = [], {}
    tmp = texdir / "_tmp"
    tmp.mkdir(parents=True, exist_ok=True)
    for name, info in sorted(done.items()):
        if not any(info.get(k) for k in ("base", "orm", "normal")):
            continue
        n = size * size

        def load(path, alpha=False):
            img = bpy.data.images.load(path)
            img.scale(size, size)
            px = np.array(img.pixels[:], dtype=np.float32).reshape(size, size, 4)
            bpy.data.images.remove(img)
            return px
        c = info["const"]
        base = load(info["base"]) if info.get("base") else np.tile(np.array([*c["Base Color"][:3], 1.0], np.float32), (size, size, 1))
        if info.get("base") and not info["base"].endswith(".png"):
            base[..., 3] = 1.0
        nrm = load(info["normal"]) if info.get("normal") else np.tile(np.array([0.5, 0.5, 1.0, 1.0], np.float32), (size, size, 1))
        orm = load(info["orm"]) if info.get("orm") else None
        rm = np.zeros((size, size, 4), np.float32)
        rm[..., 0] = nrm[..., 0]
        rm[..., 1] = nrm[..., 1]
        rm[..., 2] = orm[..., 1] if orm is not None else c["Roughness"]
        rm[..., 3] = orm[..., 2] if orm is not None else c["Metallic"]
        alpha = info.get("base", "").endswith(".png")
        k = len(out)
        files = []
        for px, kind, fmt, colour in ((base, "base", "PNG" if alpha else "JPEG", "sRGB"), (rm, "nrm", "PNG", "Non-Color")):
            img = bpy.data.images.new(f"{kind}_{k:02d}", size, size, alpha=True)
            img.colorspace_settings.name = colour
            img.pixels[:] = px.reshape(-1).tolist()
            ext = "png" if fmt == "PNG" else "jpg"
            path = tmp / f"{kind}_{k:02d}.{ext}"
            img.filepath_raw = str(path)
            img.file_format = fmt
            if fmt == "PNG":
                img.alpha_mode = "STRAIGHT"
            bpy.context.scene.render.image_settings.quality = 90
            img.save()
            bpy.data.images.remove(img)
            files.append(path.read_bytes())
            path.unlink()
        out.append(tuple(files))
        index[name] = k
        del n
    tmp.rmdir()
    return out, index


def material_table(done, index):
    names = sorted(done)
    rows = []
    for name in names:
        info = done[name]
        c = info["const"]
        flags = 0
        if info["transmission"] > 0.5:
            flags |= 1
        if info.get("base", "").endswith(".png"):
            flags |= 2
        em, strength = info["emission"] if info.get("emission") else ((0.0, 0.0, 0.0), 0.0)
        if strength > 0.0:
            flags |= 4
        if name.startswith("M_Roof"):
            flags |= 8
        rows.append(struct.pack("<4f2f3ff2fiIf", *c["Base Color"][:4], c["Roughness"], c["Metallic"], *em, strength,
                                info["transmission"], info.get("tile", 2.0), index.get(name, -1), flags, 0.0))
    return names, rows


class Occlusion:
    """Ambient occlusion at a point: rays over the hemisphere round its normal
    against its own building (everything under the same outermost parent),
    then darker close to the ground (contact)."""
    N_RAYS, REACH = 24, 8.0

    def __init__(self, objs, dg):
        from mathutils.bvhtree import BVHTree
        self.BVHTree = BVHTree
        self.dg = dg
        self.groups = {}
        for ob, _ in objs:
            self.groups.setdefault(self.root(ob).name, []).append(ob)
        self.trees = {}
        # fixed directions, cosine-weighted (Hammersley), in a z-up frame
        self.dirs = []
        for i in range(self.N_RAYS):
            u = (i + 0.5) / self.N_RAYS
            v = int(bin(i)[:1:-1].ljust(8, "0"), 2) / 256.0
            r, a = math.sqrt(u), 2 * math.pi * v
            self.dirs.append(Vector((r * math.cos(a), r * math.sin(a), math.sqrt(max(0.0, 1 - u)))))

    @staticmethod
    def root(ob):
        while ob.parent is not None:
            ob = ob.parent
        return ob

    def tree(self, ob):
        key = self.root(ob).name
        if key not in self.trees:
            verts, polys = [], []
            for o in self.groups.get(key, [ob]):
                # (a copy: to_mesh() would share, and clear, the mesh being exported)
                me = bpy.data.meshes.new_from_object(o.evaluated_get(self.dg))
                base = len(verts)
                mw = o.matrix_world
                verts += [mw @ v.co for v in me.vertices]
                polys += [[base + i for i in p.vertices] for p in me.polygons]
                bpy.data.meshes.remove(me)
            # (not the ground: the game's ambient light already has it; the
            # contact term darkens the foot of the walls)
            self.trees[key] = self.BVHTree.FromPolygons(verts, polys, all_triangles=False)
        return self.trees[key]

    def at(self, ob, p, n):
        t = self.tree(ob)
        up = Vector((0, 0, 1)) if abs(n.z) < 0.9 else Vector((1, 0, 0))
        tx = n.cross(up).normalized()
        ty = n.cross(tx)
        o = p + n * 0.05
        hit = 0.0
        for d in self.dirs:
            w = tx * d.x + ty * d.y + n * d.z
            loc, _, _, dist = t.ray_cast(o, w, self.REACH)
            if loc is not None:
                hit += 1.0 - 0.5 * (dist / self.REACH)  # near walls shade more than far ones
        ao = 1.0 - 0.7 * hit / len(self.dirs)
        contact = 0.62 + 0.38 * min(1.0, max(0.0, p.z / 1.6)) ** 0.7
        return ao * contact


def meshes(objs, mat_names):
    """Unique meshes, evaluated (modifiers on, chamfers at one segment), triangulated, as pack vertices."""
    for ob, _ in objs:
        for md in ob.modifiers:
            if md.type == "BEVEL":
                md.segments = 1
    dg = bpy.context.evaluated_depsgraph_get()
    mat_id = {n: i for i, n in enumerate(mat_names)}
    blobs, ids, tris = [], {}, 0
    occ = Occlusion(objs, dg)
    for ob, _ in objs:
        key = ob.data.name + "|" + ",".join(f"{m.type}" for m in ob.modifiers)
        if key in ids:
            continue
        if not ob.data.uv_layers:
            box_uv(ob.data)
        ev = ob.evaluated_get(dg)
        me = ev.to_mesh()
        me.calc_loop_triangles()
        uvl = me.uv_layers.active
        try:
            me.calc_tangents(uvmap=uvl.name)
        except RuntimeError:
            pass
        # tile size per material slot, to give the UVs in metres
        slot_mat, slot_tile = [], []
        for m in me.materials:
            nm = m.name if m else None
            slot_mat.append(mat_id.get(nm, 0))
            slot_tile.append(DONE.get(nm, {}).get("tile", 2.0) if nm else 2.0)
        verts, index, idx = [], {}, []
        own = ob.data.users == 1   # occlusion only where the mesh is this object's alone
        mw = ob.matrix_world
        rot3 = mw.to_3x3()
        for tri in me.loop_triangles:
            mi = min(tri.material_index, len(slot_mat) - 1) if slot_mat else 0
            mat = slot_mat[mi] if slot_mat else 0
            tile = slot_tile[mi] if slot_mat else 2.0
            for li in tri.loops:
                lp = me.loops[li]
                co = me.vertices[lp.vertex_index].co
                n = lp.normal
                t = lp.tangent
                s = lp.bitangent_sign
                uv = uvl.data[li].uv if uvl else (0.0, 0.0)

                def q(v):
                    return max(-127, min(127, int(round(v * 127.0))))
                k = (round(co.x, 4), round(co.y, 4), round(co.z, 4), q(n.x), q(n.y), q(n.z), q(t.x), q(t.y), q(t.z), int(s),
                     round(uv[0] * tile, 4), round(uv[1] * tile, 4), mat)
                vi = index.get(k)
                if vi is None:
                    vi = len(verts)
                    index[k] = vi
                    ao = occ.at(ob, mw @ co, (rot3 @ n).normalized()) if own else 1.0
                    verts.append(k + (max(0, min(127, int(round(ao * 127.0)))),))
                idx.append(vi)
        tris += len(idx) // 3
        radius = max((math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2) for v in verts), default=0.0)
        name = ob.data.name.encode()
        b = io.BytesIO()
        b.write(struct.pack("<I", len(name)))
        b.write(name)
        b.write(struct.pack("<fI", radius, len(verts)))
        for v in verts:
            b.write(struct.pack("<3f4b4b2fI", v[0], v[1], v[2], v[3], v[4], v[5], v[13], v[6], v[7], v[8], 1 if v[9] >= 0 else -1, v[10], v[11], v[12]))
        b.write(struct.pack("<I", len(idx)))
        b.write(np.array(idx, dtype="<u4").tobytes())
        ev.to_mesh_clear()
        ids[key] = len(blobs)
        blobs.append(b.getvalue())
    return blobs, ids, tris


def instances(objs, ids):
    rows = []
    for ob, cls in objs:
        key = ob.data.name + "|" + ",".join(f"{m.type}" for m in ob.modifiers)
        m = ob.matrix_world
        ax, ay = anchor(ob)
        tint = list(ob.get("tint", (1.0, 1.0, 1.0)))[:3]
        rows.append(struct.pack("<II2f12f3f", ids[key], cls, ax, ay, *[m[r][c] for r in range(3) for c in range(4)], *tint))
    return rows


def floods():
    out = []
    for ob in module_objects("05_Lighting"):
        if ob.type == "LIGHT" and ob.name.endswith("_Spot"):
            d = ob.matrix_world.to_quaternion() @ __import__("mathutils").Vector((0.0, 0.0, -1.0))
            p = ob.matrix_world.translation
            out.append(struct.pack("<6f", p.x, p.y, p.z, d.x, d.y, d.z))
    return out


def masks():
    """Footprints of the modelled buildings (and the terminal), pack frame."""
    out = []
    for b in LAYOUT["buildings"]:
        ca, sa = math.cos(b["rot"]), math.sin(b["rot"])
        pts = []
        for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            x, y = sx * (b["L"] / 2 + 1.0), sy * (b["D"] / 2 + 1.0)
            pts.append((b["x"] + x * ca - y * sa, b["y"] + x * sa + y * ca))
        out.append(pts)
    t = LAYOUT["terminal"]
    o, a, c = t["origin"], t["a_axis"], t["c_axis"]
    a0, a1, c0, c1 = t["site"]
    out.append([(o[0] + a[0] * aa + c[0] * cc, o[1] + a[1] * aa + c[1] * cc) for aa, cc in ((a0, c0), (a1, c0), (a1, c1), (a0, c1))])
    # the terminal as measured (GRB footprint, ebaw_vl.py)
    tv = LAYOUT.get("terminal_vl")
    if tv:
        out += [[tuple(p) for p in ring] for ring in tv["footprint"]]
    return [struct.pack("<I", len(p)) + struct.pack(f"<{2 * len(p)}f", *[v for xy in p for v in xy]) for p in out]


def ground():
    """The aprons' concrete pads (ebaw_vl.py, from the 2025 orthophoto)."""
    vl = HERE / "ebaw_vl.json"
    pads = json.loads(vl.read_text()).get("ground", {}).get("concrete", []) if vl.exists() else []
    out = []
    for pd in pads:
        rings = [pd["ring"]] + pd.get("holes", [])
        blob = struct.pack("<IfI", 1, pd["axis"], len(rings))
        for r in rings:
            blob += struct.pack("<I", len(r)) + struct.pack(f"<{2 * len(r)}f", *[v for xy in r for v in xy])
        out.append(blob)
    return out


DONE = {}


def main():
    o = opts()
    out = Path(o["out"]).resolve()
    out.mkdir(parents=True, exist_ok=True)
    size = int(o["tex"])
    for lc in bpy.context.view_layer.layer_collection.children:
        if lc.name in ("99_Preview", "01_Ground_Layout"):
            lc.exclude = True
    objs = objects()
    done = bake_all(out / "_bake", MODULES + ["05_Lighting"], res_tile=size, res_ground=size)
    DONE.update(done)
    lay, index = layers(done, size, out)
    names, mats = material_table(done, index)
    blobs, ids, tris = meshes(objs, names)
    inst = instances(objs, ids)
    fl = floods()
    mk = masks()
    fr = LAYOUT["frame"]
    b = io.BytesIO()
    b.write(b"FFSC")
    b.write(struct.pack("<I", 3))
    b.write(struct.pack("<ddf", fr["origin_lat"], fr["origin_lon"], fr["elevation_m"]))
    b.write(struct.pack("<I", F_SOCKS | F_MASTS))
    b.write(struct.pack("<II", size, len(lay)))
    for base, nrm in lay:
        b.write(struct.pack("<I", len(base)) + base + struct.pack("<I", len(nrm)) + nrm)
    b.write(struct.pack("<I", len(mats)) + b"".join(mats))
    b.write(struct.pack("<I", len(blobs)) + b"".join(blobs))
    b.write(struct.pack("<I", len(inst)) + b"".join(inst))
    b.write(struct.pack("<I", len(fl)) + b"".join(fl))
    b.write(struct.pack("<I", len(mk)) + b"".join(mk))
    gr = ground()
    b.write(struct.pack("<I", len(gr)) + b"".join(gr))
    raw = b.getvalue()
    (out / "pack.bin.gz").write_bytes(gzip.compress(raw, 9))
    for f in (out / "_bake").iterdir():
        f.unlink()
    (out / "_bake").rmdir()
    print(f"pack: {len(names)} materials, {len(lay)} texture layers ({size} px), {len(blobs)} meshes, {tris} triangles, "
          f"{len(inst)} instances, {len(fl)} floodlights, {len(mk)} masks; {len(raw) / 1e6:.1f} MB raw, "
          f"{(out / 'pack.bin.gz').stat().st_size / 1e6:.1f} MB gzipped")


main()
