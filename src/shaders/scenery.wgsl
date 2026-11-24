// Hand-made airport scenery (scenery.rs): meshes modelled in Blender, each
// vertex naming its material; a material is a colour (or a texture layer:
// base colour + alpha, and normal xy + roughness + metallic) with its
// emission, glass and cut-out flags. Texture coordinates are in metres, the
// layer repeating every `tile` metres. Instances use the solids' layout
// (render::Inst); ext.w = 1: the scenery gives way to the 3D tiles.
struct SMat { base: vec4f, p: vec4f, emit: vec4f, q: vec4f };
// p: roughness, metallic, tile (m), layer (-1 none). q: transmission, flags (1 glass, 2 cut-out, 4 lit).
@group(1) @binding(0) var<uniform> SM: array<SMat, 128>;
@group(1) @binding(1) var t_sc_base: texture_2d_array<f32>;
@group(1) @binding(2) var t_sc_nrm: texture_2d_array<f32>;
@group(2) @binding(0) var<uniform> CS: Cascade;

struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) P: vec3f,
  @location(1) N: vec3f,
  @location(2) T: vec4f,
  @location(3) uv: vec2f,
  @location(4) @interpolate(flat) mat: u32,
  @location(5) @interpolate(flat) ext: vec4f,
  // position in the mesh's own frame (steady wear), baked ambient occlusion
  @location(6) L: vec3f,
  @location(7) ao: f32,
  @location(8) @interpolate(flat) tint: vec3f,
};
struct SOut { @builtin(position) clip: vec4f, @location(0) uv: vec2f, @location(1) @interpolate(flat) mat: u32 };
struct GOut { @location(0) c: vec4f, @location(1) n: vec4f, @location(2) amb: f32 };

fn sc_world(p: vec3f, w: f32, m0: vec4f, m1: vec4f, m2: vec4f) -> vec3f {
  let v = vec4f(p, w);
  return vec3f(dot(m0, v), dot(m1, v), dot(m2, v));
}
// Three steady random numbers 0..1 for an integer cell.
fn sc_hash(c: vec3i) -> vec3f {
  var h = bitcast<vec3u>(c) * vec3u(1597334673u, 3812015801u, 2798796415u);
  h = (h.x ^ h.y ^ h.z) * vec3u(1597334673u, 3812015801u, 2798796415u);
  h ^= h >> vec3u(16u);
  return vec3f(h & vec3u(0xffffu)) / 65535.0;
}
fn sc_layer(m: SMat) -> i32 { return i32(max(m.p.w, 0.0)); }

@vertex fn vs(@location(0) pos: vec3f, @location(1) nrm: vec4f, @location(2) tan: vec4f, @location(8) uv: vec2f, @location(9) mat: u32,
              @location(3) m0: vec4f, @location(4) m1: vec4f, @location(5) m2: vec4f, @location(6) tint: vec4f, @location(7) ext: vec4f) -> VOut {
  let P = sc_world(pos, 1.0, m0, m1, m2);
  // (the instance rotation is orthonormal: no inverse scale needed)
  let N = sc_world(nrm.xyz, 0.0, m0, m1, m2);
  let T = sc_world(tan.xyz, 0.0, m0, m1, m2);
  return VOut(G.view_proj * vec4f(P, 1.0), P, N, vec4f(T, select(-1.0, 1.0, tan.w >= 0.0)), uv, mat, ext, pos, clamp(nrm.w, 0.0, 1.0), tint.rgb);
}

@vertex fn vs_shadow(@location(0) pos: vec3f, @location(8) uv: vec2f, @location(9) mat: u32,
                     @location(3) m0: vec4f, @location(4) m1: vec4f, @location(5) m2: vec4f, @location(7) ext: vec4f) -> SOut {
  if (ext.w > 0.5 && photo_shadow_gone(vec3f(m0.w, m1.w, m2.w))) { return SOut(vec4f(2.0, 2.0, 0.0, 1.0), uv, mat); }
  return SOut(CS.vp * vec4f(sc_world(pos, 1.0, m0, m1, m2), 1.0), uv, mat);
}
// The chain-link fence casts the shadow of its wires only.
@fragment fn fs_shadow(in: SOut) {
  let m = SM[in.mat];
  let a = textureSampleLevel(t_sc_base, s_repeat, in.uv / m.p.z, sc_layer(m), 1.0).a;
  if ((u32(m.q.y) & 2u) != 0u && a < 0.5) { discard; }
}

@fragment fn fs(in: VOut) -> GOut {
  let m = SM[in.mat];
  let flags = u32(m.q.y);
  let textured = m.p.w >= 0.0;
  // Sampled before any discard, so the derivatives are there for the mip levels.
  let uv = in.uv / m.p.z;
  let tb = textureSample(t_sc_base, s_aniso, uv, sc_layer(m));
  let tn = textureSample(t_sc_nrm, s_aniso, uv, sc_layer(m));
  if (in.ext.w > 0.5 && photo_hides(in.P, in.clip.xy)) { discard; }
  let alpha = select(m.base.a, tb.a, textured);
  if ((flags & 2u) != 0u && alpha < 0.5) { discard; }
  // Wear at the scale of a building, steady on each object and different
  // from one to the next (ext.y), so the repeating texture tiles never line
  // up the same; roofs the colour the orthophoto shows them (tint, flag 8).
  let wn = textureSampleLevel(t_noise, s_repeat, in.L * 0.035 + vec3f(in.ext.y * 7.31), 0.0);
  let tinted = select(vec3f(1.0), in.tint, (flags & 8u) != 0u);
  let albedo = select(m.base.rgb, tb.rgb, textured) * tinted * (0.9 + 0.2 * wn.r);
  let rough = clamp(select(m.p.x, tn.b, textured) + (wn.g - 0.5) * 0.15, 0.04, 1.0);
  let metal = select(m.p.y, tn.a, textured);
  let V = normalize(in.P);
  var Ng = normalize(in.N);
  if (dot(Ng, Ng) < 0.5) { Ng = -V; }
  // Thin parts (glass, fabric, signs) are seen from both sides.
  let back = dot(Ng, V) > 0.0;
  if (back) { Ng = -Ng; }
  var N = Ng;
  if (textured && dot(in.T.xyz, in.T.xyz) > 1e-6) {
    let T = normalize(in.T.xyz - Ng * dot(Ng, in.T.xyz));
    let B = cross(Ng, T) * in.T.w * select(1.0, -1.0, back);
    let t2 = tn.rg * 2.0 - 1.0;
    N = normalize(T * t2.x + B * t2.y + Ng * sqrt(max(1.0 - dot(t2, t2), 0.0)));
  }
  let glass = select(0.0, 1.0, (flags & 1u) != 0u);
  let l = light_at(in.P);
  let sh = shadow_at(in.P, Ng);
  // Snow on what faces up (snow.rs), as on the other buildings' roofs.
  let ks = snow_top(in.P, Ng, 0.55, 0.8);
  let f0 = mix(vec3f(0.04), albedo, metal);
  // the occlusion baked in Blender: corners, under the canopies, the foot of the walls
  let occ = mix(1.0, in.ao, 0.85);
  let diffuse = mix(albedo * (1.0 - metal) * (1.0 - glass * 0.75), SNOW_ALBEDO, ks) * occ;
  var c = shade(in.P, N, diffuse);
  let gloss = (1.0 - rough) * (1.0 - rough);
  let ndv = max(dot(-V, N), 0.0);
  let Fe = f0 + (1.0 - f0) * pow(1.0 - ndv, 5.0);
  let keep = 1.0 - ks;
  c += env_reflect(in.P, reflect(V, N), l.upn, false) * Fe * max(gloss, glass) * keep * mix(1.0, occ, 0.6);
  let H = normalize(G.sun_dir.xyz - V);
  c += l.sun * pow(max(dot(N, H), 0.0), mix(8.0, 600.0, gloss)) * gloss * Fe * sh * keep;
  // Lamps, lit legends and windows glow at dusk and through the night.
  let night = 1.0 - smoothstep(-0.05, 0.12, dot(l.upn, G.sun_dir.xyz));
  if (m.emit.w > 0.0) {
    c += m.emit.rgb * min(m.emit.w, 60.0) * 0.02 * G.sun_dir.w * (0.1 + night);
  }
  // The rooms behind the glass: at night about half the windows show a lit
  // room, each pane-sized cell of a facade (and each window instance, ext.y)
  // on or off for good, some under warm lamps, some under office tubes.
  if ((flags & 1u) != 0u && night > 0.0) {
    let cell = vec3i(floor(in.L / vec3f(2.6, 2.6, 3.4))) + vec3i(i32(in.ext.y * 65536.0));
    let h = sc_hash(cell);
    let on = step(0.45, h.x);
    let lamp = mix(vec3f(1.0, 0.72, 0.42), vec3f(0.95, 0.93, 0.88), step(0.65, h.y));
    c += lamp * on * (0.5 + 0.5 * h.z) * 0.016 * G.sun_dir.w * night * (1.0 - ks);
  }
  return GOut(vec4f(c, 1.0), vec4f(N, -glass * 0.6), amb_share(c));
}
