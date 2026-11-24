"""Bakes the EBAW procedural materials into texture swatches (shared by
06_export.py, the glTF export, and 07_game_pack.py, the game's pack).

Each material is baked on a plane covering one UV tile (10 m on the ground,
2 m on buildings and props, 1 m on the fence fabric), horizontal or
vertical after where it is mostly used: base colour (+ alpha), roughness,
metallic (as an ORM image) and the tangent-space normal of its bump.
"""

import bpy

def module_objects(name):
    c = bpy.data.collections[name]
    return [o for o in c.all_objects]


# ---------------------------------------------------------------- baking

def material_usage(modules):
    """For each material: the UV tile size of the meshes using it, and whether
    it is mostly on horizontal or vertical faces."""
    use = {}
    for mod in modules:
        for ob in module_objects(mod):
            if ob.type != "MESH":
                continue
            me = ob.data
            tile = 10.0 if mod == "01_Ground_Layout" else (1.0 if me.name == "FENCE_Fabric" else 2.0)
            for p in me.polygons:
                if p.material_index >= len(me.materials) or me.materials[p.material_index] is None:
                    continue
                m = me.materials[p.material_index]
                u = use.setdefault(m.name, {"tile": tile, "h": 0.0, "v": 0.0})
                if tile < u["tile"] and u["tile"] != 10.0:
                    u["tile"] = tile  # ground (10 m) wins; otherwise the finest tile
                u["h" if abs(p.normal.z) > 0.7 else "v"] += p.area
    return use


def bake_plane(tile, vertical):
    me = bpy.data.meshes.new("BAKE_plane")
    if vertical:
        z0 = 1.5
        verts = [(0, 0, z0), (tile, 0, z0), (tile, 0, z0 + tile), (0, 0, z0 + tile)]
    else:
        verts = [(0, 0, 0), (tile, 0, 0), (tile, tile, 0), (0, tile, 0)]
    me.from_pydata(verts, [], [(0, 1, 2, 3)])
    uv = me.uv_layers.new(name="UVMap")
    for li, q in zip(range(4), ((0, 0), (1, 0), (1, 1), (0, 1))):
        uv.data[li].uv = q
    ob = bpy.data.objects.new("BAKE_plane", me)
    bpy.context.scene.collection.objects.link(ob)
    return ob


def principled(mat):
    if not mat.use_nodes:
        return None
    return next((n for n in mat.node_tree.nodes if n.type == "BSDF_PRINCIPLED"), None)


def bake_channel(ob, mat, src_socket, img, kind="EMIT"):
    """Bake src_socket (through an emission shader) or the normal into img."""
    nt = mat.node_tree
    out = next(n for n in nt.nodes if n.type == "OUTPUT_MATERIAL")
    old = out.inputs["Surface"].links[0].from_socket if out.inputs["Surface"].links else None
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = img
    nt.nodes.active = tex
    em = None
    if kind == "EMIT":
        em = nt.nodes.new("ShaderNodeEmission")
        nt.links.new(src_socket, em.inputs["Color"])
        nt.links.new(em.outputs["Emission"], out.inputs["Surface"])
    bpy.context.view_layer.objects.active = ob
    ob.select_set(True)
    bpy.ops.object.bake(type=kind, use_clear=True, margin=0)
    if em is not None:
        nt.nodes.remove(em)
        if old is not None:
            nt.links.new(old, out.inputs["Surface"])
    nt.nodes.remove(tex)


def bake_all(texdir, modules, res_tile=512, res_ground=2048):
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 4
    scene.cycles.device = "CPU"
    scene.render.bake.margin = 0
    texdir.mkdir(parents=True, exist_ok=True)
    use = material_usage(modules)
    done = {}
    for name, u in sorted(use.items()):
        mat = bpy.data.materials[name]
        b = principled(mat)
        if b is None:
            continue
        info = {"emission": None, "transmission": b.inputs["Transmission Weight"].default_value}
        if b.inputs["Emission Strength"].default_value > 0.0:
            info["emission"] = (tuple(b.inputs["Emission Color"].default_value)[:3], b.inputs["Emission Strength"].default_value)
        linked = {k: b.inputs[k].links[0].from_socket if b.inputs[k].links else None for k in ("Base Color", "Roughness", "Metallic", "Alpha")}
        has_bump = bool(b.inputs["Normal"].links)
        info["const"] = {k: (tuple(b.inputs[k].default_value) if k == "Base Color" else b.inputs[k].default_value) for k in linked}
        tile = u["tile"]
        vertical = u["v"] > u["h"]
        res = res_ground if tile >= 10.0 else res_tile
        info["tile"] = tile
        info["vertical"] = vertical
        if not any(linked.values()) and not has_bump:
            done[name] = info
            continue
        ob = bake_plane(tile, vertical)
        ob.data.materials.append(mat)
        for o in bpy.context.view_layer.objects:
            o.select_set(False)
        stem = "".join(ch if ch.isalnum() or ch in "_-" else "_" for ch in name)
        if linked["Base Color"] is not None or linked["Alpha"] is not None:
            img = bpy.data.images.new(f"{stem}_basecolor", res, res, alpha=linked["Alpha"] is not None)
            if linked["Base Color"] is not None:
                bake_channel(ob, mat, linked["Base Color"], img)
            else:
                img.pixels[:] = list(info["const"]["Base Color"]) * (res * res)
            if linked["Alpha"] is not None:
                a = bpy.data.images.new("tmp_alpha", res, res)
                bake_channel(ob, mat, linked["Alpha"], a)
                px = list(img.pixels)
                ap = list(a.pixels)
                px[3::4] = ap[0::4]
                img.pixels[:] = px
                bpy.data.images.remove(a)
            ext = "png" if linked["Alpha"] is not None else "jpg"
            img.filepath_raw = str(texdir / f"{stem}_basecolor.{ext}")
            img.file_format = "PNG" if ext == "png" else "JPEG"
            img.save()
            info["base"] = img.filepath_raw
        if linked["Roughness"] is not None or linked["Metallic"] is not None:
            rimg = bpy.data.images.new(f"{stem}_orm", res, res)
            rimg.colorspace_settings.name = "Non-Color"
            r = g = None
            tmp = bpy.data.images.new("tmp_r", res, res)
            tmp.colorspace_settings.name = "Non-Color"
            if linked["Roughness"] is not None:
                bake_channel(ob, mat, linked["Roughness"], tmp)
                r = list(tmp.pixels)[0::4]
            if linked["Metallic"] is not None:
                bake_channel(ob, mat, linked["Metallic"], tmp)
                g = list(tmp.pixels)[0::4]
            bpy.data.images.remove(tmp)
            n = res * res
            rough = r or [b.inputs["Roughness"].default_value] * n
            metal = g or [b.inputs["Metallic"].default_value] * n
            px = [0.0] * (4 * n)
            px[0::4] = [1.0] * n
            px[1::4] = rough
            px[2::4] = metal
            px[3::4] = [1.0] * n
            rimg.pixels[:] = px
            rimg.filepath_raw = str(texdir / f"{stem}_orm.jpg")
            rimg.file_format = "JPEG"
            rimg.save()
            info["orm"] = rimg.filepath_raw
        if has_bump:
            nimg = bpy.data.images.new(f"{stem}_normal", res, res)
            nimg.colorspace_settings.name = "Non-Color"
            bake_channel(ob, mat, None, nimg, kind="NORMAL")
            nimg.filepath_raw = str(texdir / f"{stem}_normal.png")
            nimg.file_format = "PNG"
            nimg.save()
            info["normal"] = nimg.filepath_raw
        bpy.data.objects.remove(ob)
        done[name] = info
        print(f"baked {name} ({'vertical' if vertical else 'horizontal'}, {tile:g} m tile, {res}px)")
    return done


