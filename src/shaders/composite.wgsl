// Deferred resolve: sky, water (waves, reflections, refraction, sea bed), haze.
@group(1) @binding(0) var t_color: texture_2d<f32>;
@group(1) @binding(1) var t_aux: texture_2d<f32>;
@group(1) @binding(2) var t_depth: texture_depth_2d;
@group(1) @binding(3) var t_ao: texture_2d<f32>;
@group(1) @binding(4) var t_amb: texture_2d<f32>;
// Keep the original samples at water silhouettes: resolved material parameters
// mix water depth/class with foreground normals and cannot be shaded correctly.
@group(1) @binding(5) var t_color_ms: texture_multisampled_2d<f32>;
@group(1) @binding(6) var t_aux_ms: texture_multisampled_2d<f32>;
@group(1) @binding(7) var t_depth_ms: texture_depth_multisampled_2d;
@group(1) @binding(8) var t_amb_ms: texture_multisampled_2d<f32>;
// Precomputed per-frame frustum atmosphere and cloud-reflection LUTs (atmlut.wgsl).
@group(1) @binding(9) var t_ap_l: texture_3d<f32>;
@group(1) @binding(10) var t_ap_t: texture_3d<f32>;
@group(1) @binding(11) var t_sky_frustum: texture_2d_array<f32>;
@group(1) @binding(12) var t_cloud_refl: texture_2d<f32>;
// 8x8 reverse-Z max-depth pyramid (hiz.wgsl) for L1-cached SSR coarse reject.
@group(1) @binding(13) var t_hiz: texture_2d<f32>;

// Exact Depth16Unorm class markers (65535 = 15 * 4369, so k / 15.0 is exact).
const CLS_OPAQUE: u32 = 4u;
const CLS_GLASS: u32 = 5u;
const CLS_WATER: u32 = 6u;
const CLS_WATER_EDGE: u32 = 7u;
const CLS_OPAQUE_EDGE: u32 = 12u;
const CLS_GLASS_EDGE: u32 = 13u;
const CLS_SKY: u32 = 14u;

const CREFL_S: f32 = 20000.0;

@vertex fn vs_cls(@builtin(vertex_index) i: u32, @builtin(instance_index) inst: u32) -> FsIn {
  let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  return FsIn(vec4f(p * 2.0 - 1.0, f32(inst) / 15.0, 1.0), vec2f(p.x, 1.0 - p.y));
}

// Lookup precomputed reflected cloud premultiplied radiance (.rgb) and cover (.a).
fn cloud_refl_at(P_hit: vec3f) -> vec4f {
  let h2 = vec2f(dot(P_hit, G.east.xyz), dot(P_hit, G.north.xyz));
  let r = length(h2);
  let uv = h2 / (r + CREFL_S) * 0.5 + 0.5;
  return textureSampleLevel(t_cloud_refl, s_clamp, uv, 0.0);
}

// Aerial perspective lookup from the frustum-aligned 3D volume (atmlut.wgsl).
fn sample_ap(uv: vec2f, dist: f32) -> Scatter {
  let ap_max = max(G.lite.z, 100000.0);
  let u = clamp(dist / ap_max, 0.0, 1.0);
  let z = (sqrt(u) * 31.0 + 0.5) / 32.0;
  let uvw = vec3f(uv, z);
  return Scatter(
    textureSampleLevel(t_ap_l, s_clamp, uvw, 0.0).rgb,
    textureSampleLevel(t_ap_t, s_clamp, uvw, 0.0).rgb,
  );
}
fn apply_ap(col: vec3f, uv: vec2f, dist: f32) -> vec3f {
  if (dist <= 150.0) { return col; }
  let sc = sample_ap(uv, dist);
  return col * sc.T + sc.L;
}

// Wave texture lookup at the mip that matches the pixel footprint (fp, metres).
fn wtex(uv: vec2f, size: f32, fp: f32) -> vec4f {
  return textureSampleLevel(t_wave, s_repeat, uv, log2(max(fp * 256.0 / size, 1.0)));
}

fn ssr_refine(c0: vec4f, cd: vec4f, dims: vec2f, lo_in: f32, hi_in: f32, uv_in: vec2f) -> vec4f {
  var lo = lo_in;
  var hi = hi_in;
  var huv = uv_in;
  for (var k = 0; k < 5; k++) {
    let m = (lo + hi) * 0.5;
    let c2 = c0 + cd * m;
    let n2 = c2.xyz / c2.w;
    huv = n2.xy * vec2f(0.5, -0.5) + 0.5;
    if (textureLoad(t_depth, vec2i(huv * dims), 0) > n2.z) { hi = m; } else { lo = m; }
  }
  let px = vec2i(huv * dims);
  let c = textureLoad(t_color, px, 0);
  // (any water share at all: its samples hold the unlit tint, not a colour)
  if (textureLoad(t_aux, px, 0).a > 0.004 || c.a < 0.99 || textureLoad(t_depth, px, 0) == 0.0) { return vec4f(0.0); }
  let e = min(min(huv.x, 1.0 - huv.x), min(huv.y, 1.0 - huv.y));
  return vec4f(c.rgb, smoothstep(0.0, 0.08, e));
}

fn ssr(P: vec3f, R: vec3f, jit: f32) -> vec4f {
  let dims = vec2f(textureDimensions(t_depth));
  let c0 = G.view_proj * vec4f(P, 1.0);
  let cd = G.view_proj * vec4f(R, 0.0);
  var step_len = max(length(P) * 0.012, 0.4);
  var t = step_len * (0.5 + jit);
  var prev = 0.0;
  for (var i = 0; i < 30; i++) {
    let clip = c0 + cd * t;
    if (clip.w <= 0.0) { break; }
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * vec2f(0.5, -0.5) + 0.5;
    if (any(uv < vec2f(0.0)) || any(uv > vec2f(1.0))) { break; }
    let zs = textureLoad(t_depth, vec2i(uv * dims), 0);
    if (zs > ndc.z) {
      let ds = G.misc.x / zs;
      let dr = G.misc.x / ndc.z;
      if (dr - ds > max(step_len * 2.5, 8.0)) { break; }
      return ssr_refine(c0, cd, dims, prev, t, uv);
    }
    prev = t;
    step_len *= 1.22;
    t += step_len;
  }
  return vec4f(0.0);
}

// Identical SSR raymarch with an L1-cached 8x8 max-depth coarse reject before
// reading full-res t_depth: bit-exact results, skips >95% of full-res depth loads.
fn ssr_hiz(P: vec3f, R: vec3f, jit: f32) -> vec4f {
  let dims = vec2f(textureDimensions(t_depth));
  let hdims = vec2i(textureDimensions(t_hiz)) - vec2i(1);
  let c0 = G.view_proj * vec4f(P, 1.0);
  let cd = G.view_proj * vec4f(R, 0.0);
  var step_len = max(length(P) * 0.012, 0.4);
  var t = step_len * (0.5 + jit);
  var prev = 0.0;
  for (var i = 0; i < 30; i++) {
    let clip = c0 + cd * t;
    if (clip.w <= 0.0) { break; }
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * vec2f(0.5, -0.5) + 0.5;
    if (any(uv < vec2f(0.0)) || any(uv > vec2f(1.0))) { break; }
    let px = vec2i(uv * dims);
    let hz = textureLoad(t_hiz, clamp(px >> vec2u(3u), vec2i(0), hdims), 0).r;
    if (hz > ndc.z) {
      let zs = textureLoad(t_depth, px, 0);
      if (zs > ndc.z) {
        let ds = G.misc.x / zs;
        let dr = G.misc.x / ndc.z;
        if (dr - ds > max(step_len * 2.5, 8.0)) { break; }
        return ssr_refine(c0, cd, dims, prev, t, uv);
      }
    }
    prev = t;
    step_len *= 1.22;
    t += step_len;
  }
  return vec4f(0.0);
}

// Fast glass environment reflection matching common.wgsl `env_reflect`, using the
// baked cloud-reflection map instead of per-pixel volumetric cloud density.
fn env_reflect_fast(P: vec3f, R: vec3f, upn: vec3f, clouds: bool) -> vec3f {
  let ru = dot(R, upn);
  var sky = sky_lut(normalize(R + upn * max(0.015 - ru, 0.0)));
  if (clouds && G.lite.y > 0.5 && ru > 0.02) {
    let h = height_of(P);
    let mid = G.cloud.y + 400.0;
    if (h < mid) {
      let tc = (mid - h) / ru;
      if (tc < 60000.0) {
        let cr = cloud_refl_at(P + R * tc);
        let mu = dot(upn, G.sun_dir.xyz);
        let cl = sun_trans(mid, mu) * G.sun_dir.w * 0.05 + sky_lut(upn) * 1.2;
        sky = mix(sky, cl, cr.a * (1.0 - smoothstep(25000.0, 60000.0, tc)));
      }
    }
  }
  if (ru > 0.01) { return sky; }
  let hz = R - upn * ru;
  let hl = length(hz);
  let hzn = select(G.east.xyz, hz / max(hl, 1e-4), hl > 1e-3);
  let mu = dot(upn, G.sun_dir.xyz);
  let gnd = vec3f(0.06, 0.06, 0.055) * (sun_trans(0.0, mu) * max(mu, 0.0) * G.sun_dir.w / PI + sky_lut(upn) * 0.6);
  let below = mix(sky_lut(normalize(hzn + upn * 0.01)), gnd, smoothstep(0.0, 0.3, -ru));
  return mix(sky, below, smoothstep(0.01, -0.05, ru));
}

const SUN_SLOPE2 = 2.5e-6;

fn smith_lambda(z: f32, s2: f32) -> f32 {
  let zc = clamp(z, 1e-3, 0.9999);
  let a = zc / sqrt((1.0 - zc * zc) * 2.0 * max(s2, 1e-7));
  if (a >= 1.6) { return 0.0; }
  return (1.0 - 1.259 * a + 0.396 * a * a) / (3.535 * a + 2.181 * a * a);
}
fn az_var(d: vec3f, Tx: vec3f, Ty: vec3f, cv: vec3f) -> f32 {
  let x = dot(d, Tx);
  let y = dot(d, Ty);
  return (cv.x * x * x + 2.0 * cv.z * x * y + cv.y * y * y) / max(x * x + y * y, 1e-8);
}
fn sea_glint(Ls: vec3f, V: vec3f, N: vec3f, Tx: vec3f, Ty: vec3f, cv: vec3f) -> f32 {
  let H = normalize(Ls - V);
  let zh = dot(H, N);
  if (zh <= 0.01) { return 0.0; }
  let zx = dot(H, Tx) / zh;
  let zy = dot(H, Ty) / zh;
  let det = max(cv.x * cv.y - cv.z * cv.z, 1e-12);
  let q = (cv.y * zx * zx - 2.0 * cv.z * zx * zy + cv.x * zy * zy) / det;
  let p = exp(-0.5 * q) / (2.0 * PI * sqrt(det));
  let zl = max(dot(Ls, N), 0.01);
  let zv = max(dot(-V, N), 0.01);
  let fr = 0.02 + 0.98 * pow(1.0 - max(dot(-V, H), 0.0), 5.0);
  let g = 1.0 + smith_lambda(zl, az_var(Ls, Tx, Ty, cv)) + smith_lambda(zv, az_var(-V, Tx, Ty, cv));
  return fr * p / (4.0 * zv * zh * zh * zh * zh * g);
}

// ------------------------------------------------------------ breakers
struct ShoreAt { d0: f32, d: f32, dir: vec2f, w: f32, hb: f32 };
fn shore_at(P: vec3f) -> ShoreAt {
  var o = ShoreAt(G.shore.z, G.shore.z, vec2f(0.0), 0.0, 0.0);
  if (G.shore.x < 0.5) { return o; }
  let q = G.shore_vp * vec4f(P, 1.0);
  let uv = q.xy * vec2f(0.5, -0.5) + 0.5;
  let edge = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y)) * G.shore.w;
  if (edge <= 0.0) { return o; }
  let f0 = textureSampleLevel(t_shore, s_clamp, uv, 0.0);
  if (f0.x >= G.shore.z - 2.0) { return o; }
  let lod = clamp(log2((0.35 * f0.x + 15.0) / G.shore.y), 0.0, 6.0);
  let f = textureSampleLevel(t_shore, s_clamp, uv, lod);
  let sl = length(f.yz);
  o.d0 = f0.x;
  o.d = f.x;
  o.dir = select(vec2f(0.0), f.yz / sl, sl > 1e-3);
  o.w = 1.0 - smoothstep(edge * 0.75, edge, f0.x);
  o.hb = max(f.w, 0.0);
  return o;
}

fn surf_hash(i: i32) -> f32 {
  var h = bitcast<u32>(i) * 747796405u + 2891336453u;
  h = ((h >> ((h >> 28u) + 4u)) ^ h) * 277803737u;
  return f32((h >> 22u) ^ h) / 4294967295.0;
}

fn surf_exp(x: f32, a: f32, b: f32) -> f32 {
  let ae = a / (1.0 + a * b);
  return (ae / a) * select(exp(x / (0.006 + b)), exp(-ae * x), x >= 0.0);
}

struct Surf {
  foam: f32,
  slope: vec2f,
  h: f32,
  sv: vec3f,
};

fn breakers(wc: vec2f, sh: ShoreAt, wclass: f32, fp: f32, t: f32) -> Surf {
  var o = Surf(0.0, vec2f(0.0), 0.0, vec3f(0.0));
  let d = sh.d;
  if (sh.w <= 0.0 || min(d, sh.d0) >= G.shore.z - 2.0) { return o; }
  let sdir = sh.dir;
  let wind = G.ocean[1].x;
  let onshore = -dot(G.ocean[0].zw, sdir);
  var H0: f32;
  var T: f32;
  var s: f32;
  var fk: f32;
  var wmax: f32;
  if (wclass > 2.5) { H0 = min(0.03 + 0.005 * wind, sh.hb + 0.01); T = 1.3 + 0.08 * wind; s = 0.08; fk = 0.3; wmax = 0.3; }
  else if (wclass > 1.5) { H0 = min(0.05 + 0.014 * wind, sh.hb + 0.01); T = 1.6 + 0.13 * wind; s = 0.06; fk = 0.55; wmax = 0.6; }
  else {
    H0 = sh.hb;
    T = min(2.0 + 6.5 * sqrt(H0), 14.0);
    s = mix(0.06, 0.02, smoothstep(0.15, 0.7, H0));
    fk = 1.0;
    wmax = 0.4 + 4.5 * H0;
  }
  H0 *= 0.85 + 0.15 * onshore;
  if (H0 < 0.015) { return o; }
  fk *= smoothstep(0.04, 0.5, H0);
  let nb = textureSampleLevel(t_noise, s_repeat, vec3f(wc / 2600.0, 0.43), 0.0);
  let na = textureSampleLevel(t_noise, s_repeat, vec3f(wc / 1100.0, 0.77), 0.0);
  s *= mix(0.55, 1.7, smoothstep(0.25, 0.75, nb.r));
  let psi = (na.r - 0.5) * 5.0;

  let h0 = 0.08;
  let h = s * d + h0;
  let w = 2.0 * PI / T;
  let kl = w / sqrt(9.81 * h);
  let lam = 2.0 * PI / kl;
  let phi = 2.0 * w * (sqrt(h) - sqrt(h0)) / (s * sqrt(9.81));
  let cyc = (phi + psi) / (2.0 * PI) + t / T;
  let mf = floor(cyc);
  let u = cyc - mf;
  let m = i32(mf);
  let b = fp / lam;
  let far = smoothstep(0.18, 0.55, b);

  let dbm = max(H0 * 0.9 / 0.78 - h0, 0.0) / s;
  let env = (1.0 - smoothstep(min(4.0 * dbm + 40.0, 300.0), min(8.0 * dbm + 90.0, 0.9 * G.shore.z), d)) * sh.w;

  var H_0 = H0;
  var H_1 = H0;
  var foam = 0.0;
  if (far < 1.0 || env > 0.0) {
    let z0 = surf_hash(m);
    let z1 = surf_hash(m + 1);
    let sec0 = textureSampleLevel(t_noise, s_repeat, vec3f(wc / 900.0, z0), 0.0).r;
    let sec1 = textureSampleLevel(t_noise, s_repeat, vec3f(wc / 900.0, z1), 0.0).r;
    H_0 = H0 * (0.65 + 0.6 * z0) * mix(0.5, 1.2, smoothstep(0.3, 0.7, sec0));
    H_1 = H0 * (0.65 + 0.6 * z1) * mix(0.5, 1.2, smoothstep(0.3, 0.7, sec1));
    if (far < 1.0) {
      let db0 = max(H_0 / 0.78 - h0, 0.0) / s;
      let db1 = max(H_1 / 0.78 - h0, 0.0) / s;
      let br0 = smoothstep(db0 * 1.08 + 0.5, db0 * 0.85, d) * (0.8 + 0.5 * exp(-pow((d - db0 * 0.85) / (0.25 * db0 + 1.0), 2.0)));
      let br1 = smoothstep(db1 * 1.08 + 0.5, db1 * 0.85, d) * (0.8 + 0.5 * exp(-pow((d - db1 * 0.85) / (0.25 * db1 + 1.0), 2.0)));
      let fr = 0.5;
      foam = br0 * (fr * surf_exp(u, 16.0, b) + (1.0 - fr) * surf_exp(u, 3.0, b)) + br1 * (fr * surf_exp(u - 1.0, 16.0, b) + (1.0 - fr) * surf_exp(u - 1.0, 3.0, b));
    }
  }
  let mean = smoothstep(dbm * 1.15 + 0.5, dbm * 0.7, d) * 0.22;
  foam = mix(foam, mean, far);
  foam += 0.18 * smoothstep(dbm * 1.2 + 0.5, dbm * 0.3, d);
  let us = fract(psi / (2.0 * PI) + t / T);
  let ws = wmax * (0.25 + 0.75 * pow(1.0 - us, 2.0)) * (0.7 + 0.6 * surf_hash(i32(floor(psi / (2.0 * PI) + t / T))));
  let fringe = smoothstep(ws + 0.4 + fp, ws * 0.4, sh.d0);
  foam = max(foam, mix(fringe, smoothstep(wmax + fp, 0.0, sh.d0) * 0.5, smoothstep(0.3, 1.0, fp / wmax)));
  o.foam = clamp(foam, 0.0, 1.0) * fk * sh.w;

  if (env > 0.0) {
    let Hb = mix(H_0, H_1, smoothstep(0.55, 0.85, u));
    let Hl = min(Hb, 0.78 * h) * env;
    let back = u < 0.7;
    let wu = select(0.5 + 0.5 * (u - 0.7) / 0.3, 0.5 * u / 0.7, back);
    let dw = select(0.5 / 0.3, 0.5 / 0.7, back);
    let amp = 0.5 * Hl * kl;
    let fine = 1.0 - smoothstep(0.12, 0.4, b);
    o.slope = sdir * (-amp * sin(2.0 * PI * wu) * dw * fine);
    o.h = 0.5 * Hl * cos(2.0 * PI * wu) * fine;
    let var_s = 0.6 * amp * amp * (1.0 - fine);
    o.sv = vec3f(sdir.x * sdir.x, sdir.y * sdir.y, sdir.x * sdir.y) * var_s;
  }
  return o;
}

struct WaterPre {
  l: Light,
  e: vec3f,
  n: vec3f,
  N: vec3f,
  Tx: vec3f,
  Ty: vec3f,
  cv: vec3f,
  refl: vec3f,
  Rs: vec3f,
  F: f32,
  ndv: f32,
  wc: vec2f,
  xo: vec2f,
  x0: vec2f,
  fp: f32,
  hgt: f32,
  surf_h: f32,
  surf_foam: f32,
  st: vec3f,
  lod0: f32,
  J01: f32,
  wd: f32,
  wclass: f32,
  wdir: vec2f,
  wn: vec2f,
};

fn water_pre(P: vec3f, V: vec3f, dist: f32, aux: vec4f) -> WaterPre {
  let plain = G.lite.x < 0.5;
  let wd = max(aux.x, 0.0);
  let wclass = clamp(aux.y, 0.0, 3.0);
  let shore = clamp(aux.z, 0.0, 1.0);
  var l = light_at(P);
  let upn = l.upn;
  l.sun *= shadow_at(P + upn * select(0.0, 4.0, photo_covered(P)), upn);
  let e = normalize(G.east.xyz - upn * dot(G.east.xyz, upn));
  let n = cross(upn, e);
  let wc = wave_coord(P);
  let t = G.time.x;
  let cosv = max(dot(-V, upn), 0.03);
  let fp = dist * G.misc.w / cosv;
  var st = sea_state(wclass, upn.z, shore);
  let wind = G.ocean[1].x;

  let wdir = G.ocean[0].zw;
  let wn = vec2f(-wdir.y, wdir.x);
  let ww = vec2f(dot(wc, wdir), dot(wc, wn));
  var g1r = 0.5;
  if (fp < 45.0) {
    let g1 = textureSampleLevel(t_noise, s_repeat, vec3f(ww.x / 520.0 - t * 0.012, ww.y / 170.0, 0.37), 0.0);
    g1r = mix(g1.r, 0.5, smoothstep(10.0, 45.0, fp));
  }
  var g2g = 0.5;
  if (fp < 10.0) {
    let g2 = textureSampleLevel(t_noise, s_repeat, vec3f(ww.x / 95.0 - t * 0.035, ww.y / 38.0, 0.81), 0.0);
    g2g = mix(g2.g, 0.5, smoothstep(2.5, 10.0, fp));
  }
  let calm = 1.0 - smoothstep(1.5, 7.0, wind);
  let ruffle = smoothstep(0.28, 0.72, g1r * 0.65 + g2g * 0.45 - 0.02 + 0.2 * (1.0 - st.z) - 0.3 * calm);
  st.z *= mix(0.3, 1.2, ruffle);
  st.y *= mix(0.8, 1.1, ruffle);
  let surf = breakers(wc, shore_at(P), wclass, fp, t);

  let xo = ocean_coord(P);
  var lod = vec3f(0.0);
  for (var c = 0; c < 3; c++) { lod[c] = log2(max(fp * 256.0 / ocean_size(c), 1.0)); }
  var x0 = xo;
  var hgt = 0.0;
  for (var it = 0; it < 2; it++) {
    var ds = vec3f(0.0);
    for (var c = 0; c < 2; c++) {
      if (st[c] > 0.0 && lod[c] < 8.0) { ds += textureSampleLevel(t_ocean, s_repeat, x0 / ocean_size(c), c, lod[c]).xyz * st[c]; }
    }
    x0 = xo - ds.xy;
    hgt = ds.z;
    if (dot(ds.xy, ds.xy) < 1e-4) { break; }
  }
  var sl = vec2f(0.0);
  var jd = vec3f(0.0);
  var jd01 = vec3f(0.0);
  var cv = G.ocean[2].xyz * st.z * st.z;
  for (var c = 0; c < 3; c++) {
    let w = st[c];
    if (w <= 0.0) { continue; }
    let L = ocean_size(c);
    if (lod[c] < 8.0) {
      let a = textureSampleLevel(t_ocean, s_repeat, x0 / L, c, lod[c]);
      let b = textureSampleLevel(t_ocean, s_repeat, x0 / L, c + 3, lod[c]);
      sl += b.xy * w;
      jd += vec3f(b.zw, a.w) * w;
      if (c < 2) { jd01 += vec3f(b.zw, a.w) * w; }
    }
    cv += ocean_lost(c, lod[c]) * (w * w);
  }
  cv += surf.sv;
  cv += vec3f(SUN_SLOPE2, SUN_SLOPE2, 0.0);
  let jxx = 1.0 + jd.x;
  let jyy = 1.0 + jd.y;
  let J = jxx * jyy - jd.z * jd.z;
  var slope = vec2f(sl.x * jyy - sl.y * jd.z, sl.y * jxx - sl.x * jd.z) / max(J, 0.25) + surf.slope;
  let smag = length(slope);
  if (smag > 1.5) { slope *= 1.5 / smag; }
  let J01 = (1.0 + jd01.x) * (1.0 + jd01.y) - jd01.z * jd01.z;
  let N = normalize(upn - e * slope.x - n * slope.y);
  let Nw = normalize(upn - e * (slope.x - surf.slope.x) - n * (slope.y - surf.slope.y));
  let Tx = normalize(e - N * dot(e, N));
  let Ty = cross(N, Tx);

  let vh = normalize(vec2f(dot(V, e), dot(V, n)) + vec2f(1e-6, 0.0));
  let vc = vec2f(-vh.y, vh.x);
  let sv2 = cv.x * vh.x * vh.x + 2.0 * cv.z * vh.x * vh.y + cv.y * vh.y * vh.y;
  let sc2 = cv.x * vc.x * vc.x + 2.0 * cv.z * vc.x * vc.y + cv.y * vc.y * vc.y;
  let sv = sqrt(max(sv2, 0.0));
  let Ta = Tx * vh.x + Ty * vh.y;
  let Tb = Tx * vc.x + Ty * vc.y;

  var R = reflect(V, N);
  let ru = dot(R, upn);
  if (ru < 0.03) { R = normalize(R + upn * (0.03 - ru)); }
  let ndv = max(dot(-V, N), 0.0);
  let F = 0.02 + 0.98 * pow(1.0 - ndv, 5.0 * exp(-2.69 * sv)) / (1.0 + 22.7 * pow(sv, 1.5));
  let dv = sqrt(3.0 * max(sv2, 0.0));
  let dc = sqrt(3.0 * max(sc2, 0.0));
  var refl = sky_lut(R);
  if (!plain && max(dv, dc) >= 0.005) {
    refl /= 3.0;
    for (var k = 0; k < 4; k++) {
      let o = select(Tb * select(dc, -dc, k == 3), Ta * select(dv, -dv, k == 1), k < 2);
      var Rk = reflect(V, normalize(N - o));
      Rk = normalize(Rk + upn * max(0.03 - dot(Rk, upn), 0.0));
      refl += sky_lut(Rk) / 6.0;
    }
  }
  let mid = G.cloud.y + 400.0;
  if (G.lite.y > 0.5 && l.h < mid) {
    let hc = max(mid - l.h, 100.0);
    let kc = clamp(1500.0 * cosv * cosv / hc, 0.0, 1.0);
    let Nc = normalize(upn + (N - upn) * kc);
    let ext = atm_ext(atm_density(0.5 * (l.h + mid)));
    let kext = dot(ext, vec3f(0.2126, 0.7152, 0.0722));
    var acc_l = vec3f(0.0);
    var cov_c = 0.0;
    let nk = select(3, 1, plain);
    for (var k = 0; k < nk; k++) {
      let off = select(select(0.0, dv, k == 1), -dv, k == 2);
      var Rc = reflect(V, normalize(Nc - Ta * off));
      Rc = normalize(Rc + upn * max(0.03 - dot(Rc, upn), 0.0));
      let tc = hc / max(dot(Rc, upn), 0.03);
      if (tc < 90000.0) {
        let cr = cloud_refl_at(P + Rc * tc);
        let w = select(select(1.0 / 6.0, 2.0 / 3.0, k == 0), 1.0, plain) * (1.0 - smoothstep(40000.0, 90000.0, tc)) * exp(-tc * kext);
        acc_l += cr.rgb * w;
        cov_c += cr.a * w;
      }
    }
    refl = refl * (1.0 - cov_c) + acc_l;
  }
  let ks = mix(0.5, 0.12, smoothstep(250.0, 2500.0, dist));
  var Rs = reflect(V, normalize(upn + (Nw - upn) * ks));
  Rs = normalize(Rs + upn * max(0.03 - dot(Rs, upn), 0.0));
  return WaterPre(l, e, n, N, Tx, Ty, cv, refl, Rs, F, ndv, wc, xo, x0, fp, hgt, surf.h, surf.foam, st, lod[0], J01, wd, wclass, wdir, wn);
}

fn water_post(P: vec3f, V: vec3f, dist: f32, tint: vec3f, wp: WaterPre, ssr_hit: vec4f) -> vec3f {
  let plain = G.lite.x < 0.5;
  let l = wp.l;
  let upn = l.upn;
  let t = G.time.x;
  let wind = G.ocean[1].x;
  let refl = mix(wp.refl, ssr_hit.rgb, ssr_hit.a);

  var spec = sea_glint(G.sun_dir.xyz, V, wp.N, wp.Tx, wp.Ty, wp.cv) * l.sun;
  if (G.moon.w > 0.0) {
    spec += sea_glint(G.moon.xyz, V, wp.N, wp.Tx, wp.Ty, wp.cv) * moon_light(upn, l.h);
  }

  let alat = abs(upn.z);
  let trop = 1.0 - smoothstep(0.33, 0.58, alat);
  let polar = smoothstep(0.78, 0.93, alat);
  var turb = mix(0.8, 0.06, trop);
  var body = mix(vec3f(0.004, 0.024, 0.046), vec3f(0.003, 0.05, 0.075), trop);
  body = mix(body, vec3f(0.006, 0.022, 0.03), polar);
  var weedy = mix(1.0, 0.82, trop);
  if (wp.wclass > 2.5) { turb = 3.2; body = vec3f(0.045, 0.048, 0.022); weedy = 0.9; }
  else if (wp.wclass > 1.5) { turb = 1.5; body = vec3f(0.012, 0.03, 0.022); weedy = 1.1; }
  let tl = max(luma(tint), 0.015);
  body = mix(body, tint / tl * luma(body), 0.4);
  let ext = vec3f(0.36, 0.052, 0.024) + vec3f(0.22, 0.2, 0.3) * turb;

  let Rf = refract(V, wp.N, 0.75);
  let path = wp.wd / max(dot(-Rf, upn), 0.25);
  let Tv = exp(-ext * path);
  let mu = max(dot(upn, G.sun_dir.xyz), 0.0);
  var under = body * (l.sun * (0.3 + 0.7 * mu) / PI + l.amb) * 3.2 * (vec3f(1.0) - Tv);
  if (plain && luma(Tv) > 0.01) {
    let bed = mix(vec3f(0.58, 0.51, 0.37), tint * 2.4, 0.4);
    let Td = exp(-ext * wp.wd / max(mu, 0.35));
    under += bed * (l.sun * mu * Td / PI + l.amb * Td) * Tv;
  } else if (luma(Tv) > 0.01) {
    let bp = wp.wc + vec2f(dot(Rf, wp.e), dot(Rf, wp.n)) * path;
    let near = 1.0 - smoothstep(300.0, 2500.0, dist);
    let nA = textureSampleLevel(t_noise, s_repeat, vec3f(bp / 140.0, 0.37), 0.0);
    let nB = textureSampleLevel(t_noise, s_repeat, vec3f(bp / 23.0, 0.71), 0.0);
    let n1 = nA.r;
    let n2 = nB.r;
    var bed = vec3f(0.58, 0.51, 0.37) * (0.75 + 0.5 * n2);
    bed = mix(bed, vec3f(0.14, 0.13, 0.12), smoothstep(0.55, 0.75, n1 * 0.7 + nB.g * 0.4) * 0.85 * near);
    var weed = 0.0;
    if (near > 0.0 && wp.wd > 0.6) {
      let sway = vec2f(sin(t * 0.8 + bp.y * 0.35), cos(t * 0.65 + bp.x * 0.3)) * 0.35;
      let wA = textureSampleLevel(t_noise, s_repeat, vec3f((bp + sway) / 61.0, 0.13), 0.0);
      let wB = textureSampleLevel(t_noise, s_repeat, vec3f((bp + sway * 0.6) / 9.0, 0.53), 0.0);
      weed = smoothstep(0.52, 0.66, wA.r * weedy) * smoothstep(0.35, 0.6, wB.g + wB.b * 0.4) * smoothstep(0.6, 2.5, wp.wd) * near;
      bed = mix(bed, vec3f(0.02, 0.07, 0.022) * (0.5 + 1.0 * wB.b), weed * 0.92);
    }
    bed = mix(bed, tint * 2.4, 0.4 * (1.0 - weed));
    var ca = 0.0;
    if (near > 0.0 && wp.wd > 0.1) {
      let c1 = wtex(bp / 6.3 + vec2f(0.03, 0.02) * t, 6.3, wp.fp).b;
      let c2 = wtex(bp / 4.9 - vec2f(0.025, 0.04) * t, 4.9, wp.fp).b;
      ca = pow(1.0 - abs(c1 - c2), 9.0) * 3.0 * exp(-wp.wd * 0.12) * near * smoothstep(0.1, 0.8, wp.wd);
    }
    let Td = exp(-ext * wp.wd / max(mu, 0.35));
    under += bed * (l.sun * mu * Td * (1.0 + ca) / PI + l.amb * Td) * Tv;
  }

  let hs = max(G.ocean[1].y * (wp.st.x + wp.st.y) * 0.5, 0.2);
  let crest_h = clamp((wp.hgt + wp.surf_h) / hs + 0.45, 0.0, 1.0);
  let toward = pow(max(dot(V, G.sun_dir.xyz), 0.0), 4.0);
  let sss = toward * crest_h * (0.3 + 0.7 * (1.0 - wp.ndv)) * max(dot(G.sun_dir.xyz, upn), 0.0);
  under += (body * 8.0 + vec3f(0.01, 0.06, 0.05)) * l.sun * sss * 0.4 / PI;

  var col = mix(under, refl, clamp(wp.F, 0.0, 1.0)) + spec + point_light(P, upn) * body * 4.0;

  let fnear = 1.0 - smoothstep(2500.0, 9000.0, dist);
  if (fnear > 0.0) {
    let far_a = smoothstep(1.5, 6.0, wp.fp);
    var foam = 0.0;
    if (wp.surf_foam > 0.0) {
      var fa_g = 0.5;
      var lace = 0.55;
      if (!plain) {
        if (wp.fp < 6.0) {
          let fa = textureSampleLevel(t_noise, s_repeat, vec3f(wp.wc / 37.0 + vec2f(0.012, 0.007) * t, t * 0.02), 0.0);
          fa_g = mix(fa.g, 0.5, far_a);
        }
        if (wp.fp < 0.7) {
          let fb = textureSampleLevel(t_noise, s_repeat, vec3f(wp.wc / 5.3 - vec2f(0.02, 0.03) * t, 0.4 + t * 0.03), 0.0);
          lace = mix(fb.g * 0.6 + fb.b * 0.5, 0.55, smoothstep(0.12, 0.7, wp.fp));
        }
      }
      foam = clamp(wp.surf_foam * (0.45 + lace + (fa_g - 0.5) * 0.3) * 1.2, 0.0, 1.0);
    }
    let capw = step(wp.wclass, 1.5) * smoothstep(0.35, 0.95, 0.5 * (wp.st.x + wp.st.y));
    if (capw > 0.0) {
      let fz = textureSampleLevel(t_ocean, s_repeat, wp.x0 / ocean_size(0), 6, wp.lod0);
      var spent = fz.r;
      if (!plain && wp.fp < 0.8 && fz.r > 0.0) {
        let wl = wp.wc + (wp.x0 - wp.xo);
        let wwl = vec2f(dot(wl, wp.wdir) - 0.03 * wind * t, dot(wl, wp.wn));
        let sA = textureSampleLevel(t_noise, s_repeat, vec3f(wwl.x / 40.0, wwl.y / 12.0, 0.29), 0.0);
        var sBg = 0.55;
        if (wp.fp < 0.2) {
          let sB = textureSampleLevel(t_noise, s_repeat, vec3f(wwl.x / 12.0, wwl.y / 4.0, 0.63), 0.0);
          sBg = mix(sB.g, 0.55, smoothstep(0.04, 0.2, wp.fp));
        }
        let nc = (sA.r - 0.5) * 0.7 + (sBg - 0.55) * 0.5;
        spent = mix(smoothstep(-0.1, 0.1, fz.r - 0.5 - nc * 2.2), fz.r, smoothstep(0.15, 0.8, wp.fp));
      }
      let crest = fz.g * smoothstep(fz.b + 0.12, fz.b - 0.08, wp.J01);
      foam += max(spent, crest) * capw * mix(0.95, 0.75, far_a);
    }
    foam = clamp(foam * fnear, 0.0, 1.0);
    var aer = 0.0;
    for (var i = 0; i < 8; i++) {
      let f = G.foam[2 * i];
      if (f.w <= 0.0) { continue; }
      let fg = G.foam[2 * i + 1];
      let dp = P - f.xyz;
      let d = length(dp - upn * dot(dp, upn)) / f.w;
      if (d >= 1.4) { continue; }
      let q = vec2f(dot(dp, wp.e), dot(dp, wp.n)) + fg.z * 131.0;
      let nA = textureSampleLevel(t_noise, s_repeat, vec3f(q / max(f.w * 0.7, 3.0), 0.23 + fg.y * 0.003), 0.0);
      let nB = textureSampleLevel(t_noise, s_repeat, vec3f(q / 5.3, 0.57 + fg.y * 0.006), 0.0);
      let nC = textureSampleLevel(t_noise, s_repeat, vec3f(q / 1.7, 0.11 + fg.y * 0.01), 0.0);
      let body = smoothstep(1.0, 0.35, d + (nA.r - 0.5) * 0.9);
      let lc = mix(nB.g * 0.55 + nC.b * 0.45 + (nA.g - 0.5) * 0.5, 0.5, smoothstep(0.15, 0.9, wp.fp));
      let fresh = exp(-fg.y / 2.5);
      let k = body * fg.x * (0.42 + 0.58 * fresh);
      aer = max(aer, body * fg.x);
      foam = max(foam, clamp(k * (1.0 + 0.9 * fresh) - (1.0 - lc) * (1.0 - k) * 1.8, 0.0, 1.0) * fnear);
    }
    var bub2_b = 0.5;
    if (!plain && foam > 0.0) {
      let bub2 = textureSampleLevel(t_noise, s_repeat, vec3f(wp.wc / 0.55, 0.6), 0.0);
      bub2_b = bub2.b;
      if (wp.fp < 0.2) {
        let bub = textureSampleLevel(t_noise, s_repeat, vec3f(wp.wc / 2.1, 0.2 + t * 0.04), 0.0);
        let holes = smoothstep(0.3, 0.65, bub.r * 0.6 + bub2.g * 0.5);
        foam *= mix(1.0, 0.45 + 0.7 * holes, 1.0 - smoothstep(0.04, 0.2, wp.fp));
      }
    }
    if (aer > 0.0) {
      col = mix(col, vec3f(0.12, 0.26, 0.25) * (l.sun * (0.3 + 0.7 * mu) / PI + l.amb), aer * 0.55 * fnear);
    }
    if (foam > 0.0) {
      col = mix(col, vec3f(0.78, 0.8, 0.8) * (0.85 + 0.3 * bub2_b) * (l.sun * (0.4 + 0.6 * mu) / PI + l.amb), foam);
    }
  }
  return max(col, vec3f(0.0));
}

fn water(P: vec3f, V: vec3f, dist: f32, tint: vec3f, aux: vec4f) -> vec3f {
  let wp = water_pre(P, V, dist, aux);
  var hit = vec4f(0.0);
  if (G.misc.z > 0.5) { hit = ssr(P, wp.Rs, 0.5); }
  return water_post(P, V, dist, tint, wp, hit);
}

fn water_hiz(P: vec3f, V: vec3f, dist: f32, tint: vec3f, aux: vec4f) -> vec3f {
  let wp = water_pre(P, V, dist, aux);
  var hit = vec4f(0.0);
  if (G.misc.z > 0.5) { hit = ssr_hiz(P, wp.Rs, 0.5); }
  return water_post(P, V, dist, tint, wp, hit);
}

// ------------------------------------------------------------ split passes
@fragment fn fs_classify(in: FsIn) -> @builtin(frag_depth) f32 {
  let px = vec2i(in.pos.xy);
  let a_raw = textureLoad(t_aux, px, 0).a;
  if (a_raw < 0.0 || (a_raw > 0.0 && a_raw < 1.0)) {
    let ns = i32(textureNumSamples(t_aux_ms));
    if (ns > 1) {
      var ws = 0;
      for (var i = 0; i < ns; i++) {
        if (textureLoad(t_aux_ms, px, i).a > 0.996) { ws++; }
      }
      if (ws > 0 && ws < ns) { return f32(CLS_WATER_EDGE) / 15.0; }
    }
  }
  let z = textureLoad(t_depth, px, 0);
  let cov = textureLoad(t_color, px, 0).a;
  if (z == 0.0 || cov < 0.004) { return f32(CLS_SKY) / 15.0; }
  let aux_a = a_raw / cov;
  if (aux_a > 0.996) { return f32(CLS_WATER) / 15.0; }
  let edge = cov < 0.996;
  if (aux_a < -0.01 && G.misc.z > 0.5) {
    return f32(select(CLS_GLASS, CLS_GLASS_EDGE, edge)) / 15.0;
  }
  return f32(select(CLS_OPAQUE, CLS_OPAQUE_EDGE, edge)) / 15.0;
}

// Lean sky pass: zero raymarch loops (samples t_sky_frustum above horizon and
// t_ap_l/t_ap_t for rare bare-planet pixels below horizon), keeping full-res
// sun disc, stars, moon and airglow.
@fragment fn fs_sky(in: FsIn) -> @location(0) vec4f {
  let rd = normalize(view_ray(in.uv));
  let oc = -G.planet.xyz;
  let sun = G.sun_dir.xyz;
  let gnd = ray_sphere(oc, rd, G.planet.w);
  var col: vec3f;
  if (gnd.x > 0.0) {
    let sc = sample_ap(in.uv, gnd.x);
    let p = oc + rd * gnd.x;
    let np = normalize(p);
    let mu = dot(np, sun);
    let mum = dot(np, G.moon.xyz);
    col = sc.L + sc.T * vec3f(0.012, 0.035, 0.07) * ((sun_trans(0.0, mu) * max(mu, 0.0) + 0.02) * G.sun_dir.w + moon_light(np, 0.0) * max(mum, 0.0)) / PI;
  } else {
    let L = textureSampleLevel(t_sky_frustum, s_clamp, in.uv, 0, 0.0).rgb;
    let T = textureSampleLevel(t_sky_frustum, s_clamp, in.uv, 1, 0.0).rgb;
    col = L;
    let cs = dot(rd, sun);
    col += T * smoothstep(0.999978, 0.999990, cs) * G.sun_dir.w * 600.0;
    col += T * (select(stars(rd) * 0.11, space(rd), G.start.x > 0.0) + moon_disc(rd));
    let el = dot(rd, G.up.xyz);
    col += vec3f(0.0004, 0.0008, 0.0019) * night_cam() * (1.0 - 0.4 * max(el, 0.0));
  }
  return vec4f(clamp(col, vec3f(0.0), vec3f(3000.0)), 1.0);
}

@fragment fn fs_opaque(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let z = textureLoad(t_depth, px, 0);
  let c0 = textureLoad(t_color, px, 0);
  let cov = c0.a;
  let dist = length(view_ray(in.uv)) / z;
  var col = c0.rgb / cov;
  let aux_a = textureLoad(t_aux, px, 0).a / cov;
  if (aux_a <= 0.004) {
    let share = clamp(textureLoad(t_amb, px, 0).r / cov, 0.0, 1.0);
    col *= 1.0 - share * (1.0 - textureLoad(t_ao, px, 0).r);
  }
  col = apply_ap(col, in.uv, dist);
  return vec4f(clamp(col, vec3f(0.0), vec3f(3000.0)), select(cov, 1.0, cov >= 0.996));
}

@fragment fn fs_glass(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let z = textureLoad(t_depth, px, 0);
  let c0 = textureLoad(t_color, px, 0);
  let a0 = textureLoad(t_aux, px, 0);
  let cov = c0.a;
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  let P = ray / z;
  let dist = length(P);
  var col = c0.rgb / cov;
  let aux = a0 / cov;
  let share = clamp(textureLoad(t_amb, px, 0).r / cov, 0.0, 1.0);
  col *= 1.0 - share * (1.0 - textureLoad(t_ao, px, 0).r);
  let Ng = normalize(aux.xyz);
  let R = reflect(rd, Ng);
  let hit = ssr_hiz(P + Ng * 0.25, R, ign(in.pos.xy));
  if (hit.a > 0.0) {
    let upn = normalize(P - G.planet.xyz);
    let env = env_reflect_fast(P, R, upn, dist < 2500.0);
    col = max(col + (hit.rgb - env) * min(-aux.a, 1.0) * hit.a, vec3f(0.0));
  }
  col = apply_ap(col, in.uv, dist);
  return vec4f(clamp(col, vec3f(0.0), vec3f(3000.0)), select(cov, 1.0, cov >= 0.996));
}

@fragment fn fs_water(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let z = textureLoad(t_depth, px, 0);
  let c0 = textureLoad(t_color, px, 0);
  let a0 = textureLoad(t_aux, px, 0);
  let cov = c0.a;
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  let P = ray / z;
  let dist = length(P);
  var col = water_hiz(P, rd, dist, c0.rgb / cov, a0 / cov);
  col = apply_ap(col, in.uv, dist);
  return vec4f(clamp(col, vec3f(0.0), vec3f(3000.0)), 1.0);
}

// Per-sample water-edge resolve (guarded by Early-Z CLS_WATER_EDGE in normal flight):
// excludes globe/orbital code and deduplicates water & sky shading across MSAA
// samples that share the same pixel ray.
@fragment fn fs_water_edge(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  let jit = ign(in.pos.xy);
  let oc = -G.planet.xyz;
  let ao = textureLoad(t_ao, px, 0).r;
  let ns = i32(textureNumSamples(t_aux_ms));
  var acc = vec3f(0.0);
  var done_water = false;
  var water_col = vec3f(0.0);
  var done_sky = false;
  var sky_col = vec3f(0.0);
  for (var i = 0; i < ns; i++) {
    let z = textureLoad(t_depth_ms, px, i);
    let c0 = textureLoad(t_color_ms, px, i);
    let cov = c0.a;
    if (z == 0.0 || cov < 0.004) {
      if (!done_sky) {
        sky_col = clamp(sky_color(rd, jit), vec3f(0.0), vec3f(3000.0));
        done_sky = true;
      }
      acc += sky_col;
      continue;
    }
    let a0 = textureLoad(t_aux_ms, px, i);
    let aux = a0 / cov;
    if (aux.a > 0.996) {
      if (!done_water) {
        let P = ray / z;
        let dist = length(P);
        var wc = water(P, rd, dist, c0.rgb / cov, aux);
        if (dist > 150.0) {
          let top = ray_sphere(oc, rd, G.planet.w + ATM_H);
          let t0 = max(top.x, 0.0);
          let t1 = min(dist, top.y);
          if (t1 > t0) {
            let sc = atm_march(oc, rd, t0, t1, 6, fract(jit + G.time.w * 0.618034));
            wc = wc * sc.T + sc.L + glow_march(rd, t1, 4, fract(jit + G.time.w * 0.381966));
          }
        }
        water_col = clamp(wc, vec3f(0.0), vec3f(3000.0));
        done_water = true;
      }
      acc += water_col;
      continue;
    }
    let P = ray / z;
    let dist = length(P);
    var col = c0.rgb / cov;
    if (aux.a <= 0.004) {
      let share = clamp(textureLoad(t_amb_ms, px, i).r / cov, 0.0, 1.0);
      col *= 1.0 - share * (1.0 - ao);
    }
    if (aux.a < -0.01 && G.misc.z > 0.5) {
      let Ng = normalize(aux.xyz);
      let R = reflect(rd, Ng);
      let hit = ssr(P + Ng * 0.25, R, jit);
      if (hit.a > 0.0) {
        let upn = normalize(P - G.planet.xyz);
        let env = env_reflect_fast(P, R, upn, dist < 2500.0);
        col = max(col + (hit.rgb - env) * min(-aux.a, 1.0) * hit.a, vec3f(0.0));
      }
    }
    if (dist > 150.0) {
      let top = ray_sphere(oc, rd, G.planet.w + ATM_H);
      let t0 = max(top.x, 0.0);
      let t1 = min(dist, top.y);
      if (t1 > t0) {
        let sc = atm_march(oc, rd, t0, t1, 6, fract(jit + G.time.w * 0.618034));
        col = col * sc.T + sc.L + glow_march(rd, t1, 4, fract(jit + G.time.w * 0.381966));
      }
    }
    if (cov < 0.996) {
      if (!done_sky) {
        sky_col = clamp(sky_color(rd, jit), vec3f(0.0), vec3f(3000.0));
        done_sky = true;
      }
      col = mix(sky_col, col, cov);
    }
    acc += clamp(col, vec3f(0.0), vec3f(3000.0));
  }
  return vec4f(acc / f32(ns), 1.0);
}

// ------------------------------------------------------------ orbital fallback
fn composite_sample(in: FsIn, z: f32, c0: vec4f, a0: vec4f, amb: f32) -> vec3f {
  let px = vec2i(in.pos.xy);
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  let jit = ign(in.pos.xy);
  let oc = -G.planet.xyz;
  let cov = c0.a;
  if (z == 0.0 || cov < 0.004) {
    return clamp(sky_color(rd, jit), vec3f(0.0), vec3f(3000.0));
  }
  let P = ray / z;
  let dist = length(P);
  var col = c0.rgb / cov;
  let aux = a0 / cov;
  let gk = globe_k();
  var gc = vec3f(0.0);
  if (gk > 0.0) {
    gc = globe_surface(P, rd, col, aux.a > 0.996, fract(jit + G.time.w * 0.618034));
    if (gk >= 1.0) {
      if (cov < 0.996) { gc = mix(sky_color(rd, jit), gc, cov); }
      return clamp(gc, vec3f(0.0), vec3f(3000.0));
    }
  }
  if (aux.a <= 0.004) {
    let share = clamp(amb / cov, 0.0, 1.0);
    col *= 1.0 - share * (1.0 - textureLoad(t_ao, px, 0).r);
  }
  if (aux.a > 0.996) {
    col = water(P, rd, dist, col, aux);
  }
  else if (aux.a < -0.01 && G.misc.z > 0.5) {
    let Ng = normalize(aux.xyz);
    let R = reflect(rd, Ng);
    let hit = ssr(P + Ng * 0.25, R, jit);
    if (hit.a > 0.0) {
      let upn = normalize(P - G.planet.xyz);
      let env = env_reflect_fast(P, R, upn, dist < 2500.0);
      col = max(col + (hit.rgb - env) * min(-aux.a, 1.0) * hit.a, vec3f(0.0));
    }
  }
  if (dist > 150.0) {
    let top = ray_sphere(oc, rd, G.planet.w + ATM_H);
    let t0 = max(top.x, 0.0);
    let t1 = min(dist, top.y);
    if (t1 > t0) {
      let q = max(G.quality.x, 1.0);
      let sc = atm_march(oc, rd, t0, t1, max(6, i32(12.0 / q)), fract(jit + G.time.w * 0.618034));
      col = col * sc.T + sc.L + glow_march(rd, t1, max(4, i32(8.0 / q)), fract(jit + G.time.w * 0.381966));
    }
  }
  if (gk > 0.0) { col = mix(col, gc, gk); }
  if (cov < 0.996) { col = mix(sky_color(rd, jit), col, cov); }
  return clamp(col, vec3f(0.0), vec3f(3000.0));
}

@fragment fn fs(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  return vec4f(composite_sample(in, textureLoad(t_depth, px, 0),
    textureLoad(t_color, px, 0), textureLoad(t_aux, px, 0),
    textureLoad(t_amb, px, 0).r), 1.0);
}

@fragment fn fs_edge(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let a = textureLoad(t_aux, px, 0).a;
  if (!(a < 0.0 || (a > 0.0 && a < 1.0))) { discard; }
  let ns = i32(textureNumSamples(t_aux_ms));
  var water_samples = 0;
  for (var i = 0; i < ns; i++) {
    if (textureLoad(t_aux_ms, px, i).a > 0.996) { water_samples++; }
  }
  if (water_samples == 0 || water_samples == ns) { discard; }
  var col = vec3f(0.0);
  for (var i = 0; i < ns; i++) {
    col += composite_sample(in, textureLoad(t_depth_ms, px, i),
      textureLoad(t_color_ms, px, i), textureLoad(t_aux_ms, px, i),
      textureLoad(t_amb_ms, px, i).r);
  }
  return vec4f(col / f32(ns), 1.0);
}
