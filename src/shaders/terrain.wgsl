// offset: xyz tile centre relative to the camera, w = grid cell size (m).
// info: x = zoom, yz = the tile's corner in building tiles (zoom 14) from the
// corner of the cover square, w = that corner (x | y << 14, as float bits; see
// world.rs `cover_info`). tess: the block of cells drawn tessellated (x0, y0, w, h).
struct Tile { offset: vec4f, info: vec4f, tess: vec4f };
@group(1) @binding(0) var<uniform> T: Tile;
@group(1) @binding(1) var t_img: texture_2d<f32>;
// R water coverage, G water class * 80, B ground class | seed << 4, A field direction.
@group(1) @binding(2) var t_mask: texture_2d<f32>;
// The tile's own vertex buffer (10 floats per vertex), read by the tessellator.
@group(1) @binding(3) var<storage, read> TV: array<f32>;
// How much artificial light the ground gives off at night (sources.rs `NightLights`, from NASA Black Marble).
@group(1) @binding(4) var t_lights: texture_2d<f32>;
// How far the building tile at each place in the square around the camera
// has faded in (render.rs COVER_N, world.rs `light_cover`).
@group(1) @binding(5) var t_cover: texture_2d<f32>;

struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) P: vec3f,
  @location(1) N: vec3f,
  @location(2) uv: vec2f,
  @location(3) depth: f32,
};
struct GOut { @location(0) c: vec4f, @location(1) n: vec4f, @location(2) amb: f32 };

const GRID = 64u;
const GV = 65u;

// ------------------------------------------------------------ ground classes
const GC_WOOD = 1u;
const GC_SCRUB = 2u;
const GC_GRASS = 3u;
const GC_FARM = 4u;
const GC_WET = 5u;
const GC_SAND = 6u;
const GC_ROCK = 7u;
const GC_PAVED = 8u;
const GC_PITCH = 9u;
// Material layers (detail.wgsl).
const L_GRASS = 0;
const L_SOIL = 1;
const L_ROCK = 2;
const L_SAND = 3;
const L_ASPH = 4;
const L_LITTER = 5;
const L_CEREAL = 6;
const L_MOSS = 7;
// Mid-scale layers (aerial photo scans, blank until loaded).
const M_GRASS = 8;
const M_GROUND = 9;
const M_ROCK = 10;
const M_SAND = 11;
const M_PAVED = 12;
// Ground detail is faded out by this distance (m).
const DET_FAR = 1000.0;
const TAU = 6.28318530718;

// Per layer (G.det, set by the renderer): metres per repeat, metres of relief,
// z = for procedural layers the scale of the second, larger (or, at 1, just
// rotated) sample that hides the repeat, for photo scans (materials.rs) how
// much of their colour shifts (rather than brightness) to keep, and w = for
// photo scans how much of their variation to keep (0 = procedural layer).
fn det_size(l: i32) -> f32 { return G.det[l].x; }
fn det_range(l: i32) -> f32 { return G.det[l].y; }
fn det_photo(l: i32) -> f32 { return G.det[l].w; }
fn det_macro(l: i32) -> f32 { return select(G.det[l].z, 1.0, det_photo(l) > 0.0); }
fn det_chroma(l: i32) -> f32 { return G.det[l].z; }
const ROT = mat2x2f(0.8, 0.6, -0.6, 0.8);

// ------------------------------------------------------------ hex tiling
// Photo scans would repeat visibly, so they are hex-tiled (after Mikkelsen,
// "Practical Real-Time Hex-Tiling", JCGT 2022): the plane is cut into
// hexagons about half a texture across, each showing the texture under its own
// random rotation and offset. Three samples are blended across the borders,
// with weights sharpened by height so the seams run along the relief rather
// than along a lattice.
struct HexCells { p: vec3f, v0: vec2i, v1: vec2i, v2: vec2i };

fn hx_pcg(v: u32) -> u32 {
  let s = v * 747796405u + 2891336453u;
  let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
  return (w >> 22u) ^ w;
}
fn rot2(cs: vec2f, v: vec2f) -> vec2f { return vec2f(cs.x * v.x - cs.y * v.y, cs.y * v.x + cs.x * v.y); }
fn irot2(cs: vec2f, v: vec2f) -> vec2f { return vec2f(cs.x * v.x + cs.y * v.y, -cs.y * v.x + cs.x * v.y); }
fn hex_cs(v: vec2i) -> vec2f {
  let a = f32(hx_pcg(bitcast<u32>(v.x) ^ hx_pcg(bitcast<u32>(v.y) + 0x9e3779b9u)) >> 8u) * (TAU / 16777216.0);
  return vec2f(cos(a), sin(a));
}
fn hex_off(v: vec2i) -> vec2f {
  let h = hx_pcg(bitcast<u32>(v.y) ^ hx_pcg(bitcast<u32>(v.x) + 0x7f4a7c15u));
  return vec2f(f32(h & 0xffffu), f32(h >> 16u)) / 65536.0;
}
// Where hexagon centre v shows st (texture repeats).
fn hex_st(v: vec2i, st: vec2f, cs: vec2f) -> vec2f {
  let cen = vec2f(f32(v.x) * 0.28867513 + f32(v.y) * 0.14433757, f32(v.y) * 0.25);
  return rot2(cs, st - cen) + cen + hex_off(v);
}
// Sharpened shares of the three hexagons before height; a hexagon whose
// share is negligible is not sampled at all (over most of the plane only one
// is), so its share is zeroed.
fn hex_p(w: vec3f) -> vec3f {
  let w2 = w * w;
  let w4 = w2 * w2;
  let k = w4 * w2 * w;
  let p = k / (k.x + k.y + k.z);
  return select(vec3f(0.0), p, p > vec3f(0.004));
}
// Three surrounding hexagon centres sorted by descending weight (p0 >= p1 >= p2),
// so every lane uses slot 0 with zero warp divergence and slots 1/2 only run on
// hex borders/corners. Returning only cell IDs keeps register pressure low.
fn hex_cells(st: vec2f) -> HexCells {
  let p = st * 3.4641016;
  let sk = vec2f(p.x - 0.57735027 * p.y, 1.15470054 * p.y);
  let base = vec2i(floor(sk));
  let f = fract(sk);
  let z = 1.0 - f.x - f.y;
  let up = z < 0.0;
  let s1 = select(0.0, 1.0, up);
  let s2 = 2.0 * s1 - 1.0;
  let pw = hex_p(vec3f(-z * s2, s1 - f.y * s2, s1 - f.x * s2));
  let i = select(0, 1, up);
  var p0 = pw.x; var p1 = pw.y; var p2 = pw.z;
  var v0 = base + vec2i(i, i);
  var v1 = base + vec2i(i, 1 - i);
  var v2 = base + vec2i(1 - i, i);
  if (p0 < p1) { let tp = p0; p0 = p1; p1 = tp; let tv = v0; v0 = v1; v1 = tv; }
  if (p1 < p2) { let tp = p1; p1 = p2; p2 = tp; let tv = v1; v1 = v2; v2 = tv; }
  if (p0 < p1) { let tp = p0; p0 = p1; p1 = tp; let tv = v0; v0 = v1; v1 = tv; }
  return HexCells(vec3f(p0, p1, p2), v0, v1, v2);
}
// Final blend weights from those shares and the three heights.
fn hex_w(p: vec3f, h: vec3f) -> vec3f {
  let k = p * (0.25 + h);
  return k / max(k.x + k.y + k.z, 1e-6);
}
// Height only, for parallax (st and its derivatives in texture repeats).
fn hex_h(l: i32, st: vec2f, dx: vec2f, dy: vec2f) -> f32 {
  let hc = hex_cells(st);
  let p = hc.p;
  let r0 = hex_cs(hc.v0);
  let h0 = textureSampleGrad(t_dalb, s_detail, hex_st(hc.v0, st, r0), l, rot2(r0, dx), rot2(r0, dy)).a;
  if (p.y <= 0.0) { return h0; }
  let r1 = hex_cs(hc.v1);
  let h1 = textureSampleGrad(t_dalb, s_detail, hex_st(hc.v1, st, r1), l, rot2(r1, dx), rot2(r1, dy)).a;
  var h2 = 0.0;
  if (p.z > 0.0) {
    let r2 = hex_cs(hc.v2);
    h2 = textureSampleGrad(t_dalb, s_detail, hex_st(hc.v2, st, r2), l, rot2(r2, dx), rot2(r2, dy)).a;
  }
  let h = vec3f(h0, h1, h2);
  return dot(hex_w(p, h), h);
}
fn hex_h_lod(l: i32, st: vec2f, lod: f32) -> f32 {
  let hc = hex_cells(st);
  let p = hc.p;
  let r0 = hex_cs(hc.v0);
  let h0 = textureSampleLevel(t_dalb, s_detail, hex_st(hc.v0, st, r0), l, lod).a;
  if (p.y <= 0.0) { return h0; }
  let r1 = hex_cs(hc.v1);
  let h1 = textureSampleLevel(t_dalb, s_detail, hex_st(hc.v1, st, r1), l, lod).a;
  var h2 = 0.0;
  if (p.z > 0.0) {
    let r2 = hex_cs(hc.v2);
    h2 = textureSampleLevel(t_dalb, s_detail, hex_st(hc.v2, st, r2), l, lod).a;
  }
  let h = vec3f(h0, h1, h2);
  return dot(hex_w(p, h), h);
}
// A layer's height at uv (m) in a vertex shader, filtered for spacing sp (m):
// the large copy for procedural layers, hex-tiled for photo scans.
fn layer_h_lod(l: i32, uv: vec2f, sp: f32) -> f32 {
  let s = det_size(l) * det_macro(l);
  let lod = max(log2(sp * f32(textureDimensions(t_dalb).x) / s), 0.0);
  if (lod >= f32(det_top())) { return layer_hmean(l); }
  if (det_photo(l) > 0.0) { return hex_h_lod(l, uv / s, lod); }
  return textureSampleLevel(t_dalb, s_detail, ROT * uv / s + vec2f(0.37, 0.71), l, lod).a;
}

// Low-frequency noise (period s metres), filtered for a footprint of fw metres.
fn wn(wc: vec2f, s: f32, fw: f32) -> f32 {
  return textureSampleLevel(t_wave, s_repeat, wc / s, max(log2(fw * 256.0 / s), 0.0)).a;
}

struct Ground {
  la: i32,          // main layer
  lb: i32,          // second layer
  wb: f32,          // share of the second layer (before height blending)
  rock: f32,        // share of rock (triplanar on steep ground)
  amp: f32,         // relief scale: 0 where nothing may stick up (roads)
  soil: vec3f,      // the second layer's own colour, where it is not the photo's
  soil_abs: f32,    // how much of that colour to use
  plant: vec3f,     // the main layer's colour (the photo, or what is left of it once the soil is taken out)
  hrow: f32,        // crop row / furrow / wheel track relief (m)
  grow: vec2f,      // its gradient (east, north)
  tint: vec3f,
  grassy: f32,      // blades: gaps close up and it gets a sheen at grazing angles
  mid: i32,         // mid-scale layer
  midk: f32,        // and how strongly it shows
};

fn ground_mask(uv: vec2f, wc: vec2f, fw: f32) -> vec4f {
  // Nearest texel, looked up through a wobble so class edges are ragged
  // rather than a staircase of texels.
  let jit = vec2f(wn(wc, 3.1, fw), wn(wc + 1.7, 4.3, fw)) - 0.5;
  let dim = vec2f(textureDimensions(t_mask, 0));
  let tx = vec2i(clamp(floor(uv * dim + jit * 2.5), vec2f(0.0), dim - 1.0));
  return textureLoad(t_mask, tx, 0);
}

// Wheel tracks every 20-24 m across a cereal field.
fn tramlines(g: ptr<function, Ground>, across: f32, seed: u32, fa: f32) {
  let per = 20.0 + 4.0 * f32(seed & 1u);
  let local = (fract(across / per + f32(seed) * 0.13) - 0.5) * per;
  let off = abs(abs(local) - 0.9);
  let w = (1.0 - smoothstep(0.18, 0.3 + fa, off)) * (1.0 - smoothstep(1.5, 4.0, fa));
  (*g).wb = max((*g).wb, w);
  (*g).hrow -= 0.05 * w;
  (*g).soil_abs = max((*g).soil_abs, w);
}

fn ground(uv: vec2f, wc: vec2f, img: vec3f, slope: f32, dx: vec2f, dy: vec2f, V: vec3f) -> Ground {
  let fw = max(length(dx), length(dy));
  var g = Ground(L_GRASS, L_SOIL, 0.0, 0.0, 1.0, vec3f(0.0), 0.0, img, 0.0, vec2f(0.0), vec3f(1.0), 0.0, M_GRASS, 1.0);
  let m = ground_mask(uv, wc, fw);
  let b8 = u32(round(m.b * 255.0));
  let cls = b8 & 15u;
  let seed = b8 >> 4u;
  let sum = img.r + img.g + img.b + 0.02;
  let gex = (2.0 * img.g - img.r - img.b) / sum;
  let lum = sum / 3.0;
  let veg = clamp(gex * 9.0 - 0.1, 0.0, 1.0) * (1.0 - smoothstep(0.35, 0.6, lum));
  // Bare earth as it would look here: brown, about as bright as the photo.
  g.soil = vec3f(0.36, 0.28, 0.20) * clamp(lum / 0.26, 0.45, 1.6) * 0.9;
  var rock_thr = 0.2;
  var have_n2 = false;
  var n2 = 0.5;
  switch cls {
    case GC_WOOD: {
      let n1 = wn(wc, 23.0, fw);
      g.la = L_LITTER; g.lb = L_MOSS;
      g.wb = smoothstep(0.5, 0.75, n1);
      rock_thr = 0.28;
      g.mid = M_GROUND; g.midk = 0.6;
    }
    case GC_SCRUB: {
      n2 = wn(wc + 7.3, 7.0, fw);
      have_n2 = true;
      g.wb = 0.35 + 0.5 * (n2 - 0.5);
      g.soil_abs = 0.6; g.grassy = 0.7;
      g.tint = vec3f(1.05, 1.0, 0.9);
    }
    case GC_GRASS, GC_PITCH: {
      g.soil_abs = 0.7; g.grassy = 1.0;
      if (cls == GC_PITCH) {
        let ang = m.a * (255.0 / 256.0) * PI;
        let dir = vec2f(cos(ang), sin(ang));
        let perp = vec2f(-dir.y, dir.x);
        let across = dot(wc, perp);
        let fa = abs(dot(dx, perp)) + abs(dot(dy, perp));
        g.wb = 0.04;
        g.midk = 0.3;
        // Mowing stripes: the grass is laid toward or away from the viewer.
        let stripe = select(-1.0, 1.0, fract(across / 10.0) < 0.5);
        let hv = normalize(vec2f(dot(V, G.east.xyz), dot(V, G.north.xyz)) + 1e-4);
        let k = 1.0 - smoothstep(1.5, 4.0, fa);
        // Only on the turf, not on tracks or paths inside the pitch.
        g.tint = vec3f(1.0 + 0.11 * stripe * dot(hv, dir) * k * veg);
      } else {
        let n1 = wn(wc, 23.0, fw);
        g.wb = smoothstep(0.62, 0.85, n1) * 0.45 + (1.0 - veg) * 0.25;
      }
    }
    case GC_FARM: {
      let ang = m.a * (255.0 / 256.0) * PI;
      let dir = vec2f(cos(ang), sin(ang));
      let perp = vec2f(-dir.y, dir.x);
      let across = dot(wc, perp);
      let fa = abs(dot(dx, perp)) + abs(dot(dy, perp));
      let yel = (img.r + img.g - 2.0 * img.b) / sum;
      g.soil_abs = 1.0;
      g.mid = M_GROUND; g.midk = 0.6;
      if (gex > 0.05 && (seed & 1u) == 0u) {
        // Row crop: leafy plants in rows on low ridges, soil between.
        g.la = L_LITTER; g.lb = L_SOIL; g.grassy = 0.4;
        let sp = 0.6 + 0.1 * f32((seed >> 1u) & 3u);
        let cover = clamp(0.3 + gex * 2.5, 0.3, 0.9);
        let ph = fract(across / sp) - 0.5;
        let x = abs(ph) * 2.0;
        let aa = smoothstep(0.12, 0.35, fa / sp);
        let wp = mix(1.0 - smoothstep(cover - 0.15, cover + 0.1, x), cover, aa);
        g.wb = 1.0 - wp;
        g.hrow = 0.06 * cos(TAU * ph) * (1.0 - aa);
        g.grow = perp * (-0.06 * TAU / sp * sin(TAU * ph) * (1.0 - aa));
        // The photo shows plants and soil together: what is left once the soil is taken out is plant.
        g.plant = max((img - (1.0 - cover) * g.soil) / cover, img * 0.4);
      } else if (gex > 0.05 || (yel > 0.14 && lum > 0.22)) {
        // Cereal, green or ripe, with wheel tracks.
        let n1 = wn(wc, 23.0, fw);
        g.la = L_CEREAL; g.lb = L_SOIL; g.grassy = 0.8;
        g.mid = M_GRASS; g.midk = 0.4;
        g.tint = vec3f(1.0 + 0.1 * (n1 - 0.5));
        tramlines(&g, across, seed, fa);
      } else {
        // Ploughed or tilled: furrows.
        g.la = L_SOIL; g.lb = L_SOIL;
        let sp = 0.3 + 0.05 * f32(seed & 3u);
        let ph = fract(across / sp) - 0.5;
        let aa = smoothstep(0.12, 0.35, fa / sp);
        // Rounded ridges, sharp furrow bottoms.
        let r = 1.0 - 4.0 * ph * ph;
        g.hrow = 0.09 * (r - 0.66) * (1.0 - aa);
        g.grow = perp * (-0.09 * 8.0 * ph / sp * (1.0 - aa));
        g.plant = img;
        g.tint = vec3f(0.9 + 0.2 * r * (1.0 - aa));
      }
    }
    case GC_WET: {
      n2 = wn(wc + 7.3, 7.0, fw);
      have_n2 = true;
      g.la = L_MOSS; g.lb = L_GRASS;
      g.wb = 0.5 + 0.8 * (n2 - 0.5);
      g.tint = vec3f(0.85); g.grassy = 0.5;
    }
    case GC_SAND: {
      g.la = L_SAND; g.lb = L_SAND;
      g.mid = M_SAND;
      rock_thr = 0.45;
    }
    case GC_ROCK: {
      g.rock = 1.0;
      g.mid = M_GROUND;
    }
    case GC_PAVED: {
      g.la = L_ASPH; g.lb = L_ASPH;
      g.amp = 0.0; rock_thr = 2.0;
      g.mid = M_PAVED;
    }
    default: {
      // Unmapped: lawn where the photo is green, else paving in town
      // (grey) and bare earth in the country (brown).
      let grey = 1.0 - smoothstep(0.03, 0.09, (img.r - img.b) / sum);
      g.lb = select(L_SOIL, L_ASPH, grey > 0.5);
      g.wb = 1.0 - smoothstep(0.15, 0.6, veg);
      g.grassy = veg;
      g.amp = mix(1.0, 0.3, grey);
      g.mid = select(select(M_GROUND, M_PAVED, grey > 0.5), M_GRASS, veg > 0.5);
      g.midk = 0.8;
    }
  }
  if (g.rock < 1.0) {
    if (slope >= rock_thr + 0.18) {
      g.rock = 1.0;
    } else if (slope > rock_thr - 0.06) {
      if (!have_n2) { n2 = wn(wc + 7.3, 7.0, fw); }
      g.rock = max(g.rock, smoothstep(rock_thr, rock_thr + 0.12, slope + (n2 - 0.5) * 0.12));
    }
  }
  return g;
}

// ------------------------------------------------------------ material samples
// c: colour relative to the layer's mean, h: height 0..1, hm: metres,
// n: world-space normal tilt, cav: cavity.
struct Det { c: vec3f, h: f32, hm: f32, n: vec3f, cav: f32 };

fn det_top() -> i32 { return i32(textureNumLevels(t_dalb)) - 1; }
fn layer_hmean(l: i32) -> f32 {
  let r = G.det_hmean[u32(l) >> 2u];
  return r[u32(l) & 3u];
}
// Mean cavity of a layer as det() combines it.
fn cav_mean(l: i32) -> f32 {
  let c = G.det_mean[l].w;
  return select(c, c * (1.0 + c) * 0.5, det_macro(l) > 1.0);
}

// Photo-scan layer (det_photo(l) > 0), hex-tiled. Samples albedo and normal
// back-to-back per active hex so rotation/uv/grad registers die immediately.
fn det_photo_only(l: i32, uv: vec2f, dx: vec2f, dy: vec2f, t: vec3f, b: vec3f, lite: f32) -> Det {
  let s = det_size(l);
  let r = det_range(l);
  let dm = G.det_mean[l];
  let inv_mean = dm.rgb;
  let cm = dm.w;
  let photo = det_photo(l);
  let inv_s = 1.0 / s;
  let gx = dx * inv_s;
  let gy = dy * inv_s;
  // Exact 1x1 top-mip short-circuit (8x anisotropy clamps to mip 10 once
  // max(min_axis, max_axis / 8) >= 1.0): every hex yields the exact mean.
  if (lite >= 1.0) {
    let glx = dot(gx, gx);
    let gly = dot(gy, gy);
    if (max(min(glx, gly), max(glx, gly) * (1.0 / 64.0)) >= 1.0) {
      let hm = layer_hmean(l);
      return Det(vec3f(1.0), hm, (hm - 0.5) * r, vec3f(0.0), cm);
    }
  }
  let st = uv * inv_s;
  let hc = hex_cells(st);
  let p = hc.p;
  let need_n = lite < 1.0;
  // Hex 0 (always active).
  let r0 = hex_cs(hc.v0);
  let s0 = hex_st(hc.v0, st, r0);
  let g0x = rot2(r0, gx);
  let g0y = rot2(r0, gy);
  let a0 = textureSampleGrad(t_dalb, s_detail, s0, l, g0x, g0y);
  var g0 = vec2f(0.0);
  var c0 = cm;
  if (need_n) {
    let n0 = textureSampleGrad(t_dnrm, s_detail, s0, l, g0x, g0y);
    g0 = irot2(r0, n0.xy * 2.0 - 1.0);
    c0 = n0.z;
  }
  var a = a0;
  var gg = g0;
  var cav = c0;
  if (p.y > 0.0) {
    let r1 = hex_cs(hc.v1);
    let s1 = hex_st(hc.v1, st, r1);
    let g1x = rot2(r1, gx);
    let g1y = rot2(r1, gy);
    let a1 = textureSampleGrad(t_dalb, s_detail, s1, l, g1x, g1y);
    var g1 = vec2f(0.0);
    var c1 = cm;
    if (need_n) {
      let n1 = textureSampleGrad(t_dnrm, s_detail, s1, l, g1x, g1y);
      g1 = irot2(r1, n1.xy * 2.0 - 1.0);
      c1 = n1.z;
    }
    var a2 = vec4f(0.0);
    var g2 = vec2f(0.0);
    var c2 = cm;
    if (p.z > 0.0) {
      let r2 = hex_cs(hc.v2);
      let s2 = hex_st(hc.v2, st, r2);
      let g2x = rot2(r2, gx);
      let g2y = rot2(r2, gy);
      a2 = textureSampleGrad(t_dalb, s_detail, s2, l, g2x, g2y);
      if (need_n) {
        let n2 = textureSampleGrad(t_dnrm, s_detail, s2, l, g2x, g2y);
        g2 = irot2(r2, n2.xy * 2.0 - 1.0);
        c2 = n2.z;
      }
    }
    let w = hex_w(p, vec3f(a0.a, a1.a, a2.a));
    a = a0 * w.x + a1 * w.y + a2 * w.z;
    if (need_n) {
      gg = g0 * w.x + g1 * w.y + g2 * w.z;
      cav = c0 * w.x + c1 * w.y + c2 * w.z;
    }
  }
  if (need_n) {
    gg *= 1.0 - lite;
    cav = mix(cav, cm, lite);
  }
  // Mostly brightness: a scan's own colour shifts (grey gravel in green
  // moss) turn odd on top of a different satellite colour.
  let rc = a.rgb * inv_mean;
  let rl = max(dot(rc, vec3f(0.2126, 0.7152, 0.0722)), 1e-3);
  return Det(
    mix(vec3f(1.0), mix(vec3f(rl), rc, det_chroma(l)), photo),
    a.a,
    (a.a - 0.5) * r,
    t * gg.x + b * gg.y,
    cav,
  );
}

// One layer, sampled twice (as is, and larger or rotated) on the plane
// spanned by tangents t and b, with uv in metres.
// lite (0..1): far away only one sample is taken, the large copy for
// self-similar layers, the base one for the rest (whose detail is long gone).
fn det(l: i32, uv: vec2f, dx: vec2f, dy: vec2f, t: vec3f, b: vec3f, wc: vec2f, fw: f32, lite: f32) -> Det {
  if (det_photo(l) > 0.0) {
    return det_photo_only(l, uv, dx, dy, t, b, lite);
  }
  let s = det_size(l);
  let r = det_range(l);
  let dm = G.det_mean[l];
  let inv_mean = dm.rgb;
  let cm = dm.w;
  var o: Det;
  let m = det_macro(l);
  let u2 = ROT * uv / (s * m) + vec2f(0.37, 0.71);
  if (m > 1.0) {
    // Self-similar layers: the large copy adds relief and colour variation on top.
    let a2 = textureSampleGrad(t_dalb, s_detail, u2, l, ROT * dx / (s * m), ROT * dy / (s * m));
    let n2 = textureSampleGrad(t_dnrm, s_detail, u2, l, ROT * dx / (s * m), ROT * dy / (s * m));
    var a1c = vec3f(1.0);
    var a1h = a2.a;
    var n1 = vec4f(0.5, 0.5, cm, 0.0);
    if (lite < 1.0) {
      let s1 = textureSampleGrad(t_dalb, s_detail, uv / s, l, dx / s, dy / s);
      a1c = mix(s1.rgb * inv_mean, vec3f(1.0), lite);
      a1h = mix(s1.a, a2.a, lite);
      n1 = mix(textureSampleGrad(t_dnrm, s_detail, uv / s, l, dx / s, dy / s), n1, lite);
    }
    // The rotated sample's slopes are in the rotated frame.
    let gg = (n1.xy * 2.0 - 1.0) + transpose(ROT) * (n2.xy * 2.0 - 1.0) * 0.5;
    o.c = a1c * mix(vec3f(1.0), a2.rgb * inv_mean, 0.6);
    o.h = clamp(a1h * 0.6 + a2.a * 0.4, 0.0, 1.0);
    o.hm = (a1h - 0.5) * r + (a2.a - 0.5) * r * m * 0.5;
    o.n = t * gg.x + b * gg.y;
    o.cav = n1.b * mix(1.0, n2.b, 0.5);
  } else {
    let a1 = textureSampleGrad(t_dalb, s_detail, uv / s, l, dx / s, dy / s);
    let n1 = textureSampleGrad(t_dnrm, s_detail, uv / s, l, dx / s, dy / s);
    var a2 = a1;
    var n2 = n1;
    var k = 0.0;
    if (lite < 1.0) {
      // Same scale: patches of one copy or the other, joined at the low points.
      a2 = textureSampleGrad(t_dalb, s_detail, u2, l, ROT * dx / s, ROT * dy / s);
      n2 = textureSampleGrad(t_dnrm, s_detail, u2, l, ROT * dx / s, ROT * dy / s);
      n2 = vec4f(transpose(ROT) * (n2.xy * 2.0 - 1.0) * 0.5 + 0.5, n2.zw);
      k = smoothstep(0.35, 0.65, wn(wc + vec2f(3.1, 1.3), s * 9.0, fw) + (a2.a - a1.a) * 0.3) * (1.0 - lite);
    }
    o.c = mix(a1.rgb, a2.rgb, k) * inv_mean;
    o.h = mix(a1.a, a2.a, k);
    o.hm = (o.h - 0.5) * r;
    let gg = mix(n1.xy, n2.xy, k) * 2.0 - 1.0;
    o.n = t * gg.x + b * gg.y;
    o.cav = mix(n1.b, n2.b, k);
  }
  return o;
}

fn tri_w(N: vec3f) -> vec3f {
  let nw = abs(vec3f(dot(N, G.up.xyz), dot(N, G.east.xyz), dot(N, G.north.xyz)));
  let nw2 = nw * nw;
  let w = nw2 * nw2;
  return w / (w.x + w.y + w.z);
}

// Layer l (rock) projected along the three local axes, so cliffs do not smear.
fn tri_det(l: i32, wc: vec2f, alt: f32, w: vec3f, dwc: mat2x2f, dalt: vec2f, fw: f32, lite: f32) -> Det {
  let e = G.east.xyz;
  let nn = G.north.xyz;
  let u = G.up.xyz;
  var o = Det(vec3f(0.0), 0.0, 0.0, vec3f(0.0), 0.0);
  var tw = 0.0;
  if (w.x > 0.02) {
    let d = det(l, wc, dwc[0], dwc[1], e, nn, wc, fw, lite);
    let k = w.x * (0.5 + d.h);
    o.c += d.c * k; o.h += d.h * k; o.hm += d.hm * k; o.n += d.n * k; o.cav += d.cav * k; tw += k;
  }
  if (w.y > 0.02) {
    let q = vec2f(wc.y, alt);
    let d = det(l, q, vec2f(dwc[0].y, dalt.x), vec2f(dwc[1].y, dalt.y), nn, u, wc, fw, lite);
    let k = w.y * (0.5 + d.h);
    o.c += d.c * k; o.h += d.h * k; o.hm += d.hm * k; o.n += d.n * k; o.cav += d.cav * k; tw += k;
  }
  if (w.z > 0.02) {
    let q = vec2f(wc.x, alt);
    let d = det(l, q, vec2f(dwc[0].x, dalt.x), vec2f(dwc[1].x, dalt.y), e, u, wc, fw, lite);
    let k = w.z * (0.5 + d.h);
    o.c += d.c * k; o.h += d.h * k; o.hm += d.hm * k; o.n += d.n * k; o.cav += d.cav * k; tw += k;
  }
  let iw = 1.0 / max(tw, 1e-4);
  return Det(o.c * iw, o.h * iw, o.hm * iw, o.n * iw, o.cav * iw);
}

fn tri_det_photo(l: i32, wc: vec2f, alt: f32, w: vec3f, dwc: mat2x2f, dalt: vec2f, lite: f32) -> Det {
  let e = G.east.xyz;
  let nn = G.north.xyz;
  let u = G.up.xyz;
  var o = Det(vec3f(0.0), 0.0, 0.0, vec3f(0.0), 0.0);
  var tw = 0.0;
  if (w.x > 0.02) {
    let d = det_photo_only(l, wc, dwc[0], dwc[1], e, nn, lite);
    let k = w.x * (0.5 + d.h);
    o.c += d.c * k; o.h += d.h * k; o.hm += d.hm * k; o.n += d.n * k; o.cav += d.cav * k; tw += k;
  }
  if (w.y > 0.02) {
    let q = vec2f(wc.y, alt);
    let d = det_photo_only(l, q, vec2f(dwc[0].y, dalt.x), vec2f(dwc[1].y, dalt.y), nn, u, lite);
    let k = w.y * (0.5 + d.h);
    o.c += d.c * k; o.h += d.h * k; o.hm += d.hm * k; o.n += d.n * k; o.cav += d.cav * k; tw += k;
  }
  if (w.z > 0.02) {
    let q = vec2f(wc.x, alt);
    let d = det_photo_only(l, q, vec2f(dwc[0].x, dalt.x), vec2f(dwc[1].x, dalt.y), e, u, lite);
    let k = w.z * (0.5 + d.h);
    o.c += d.c * k; o.h += d.h * k; o.hm += d.hm * k; o.n += d.n * k; o.cav += d.cav * k; tw += k;
  }
  let iw = 1.0 / max(tw, 1e-4);
  return Det(o.c * iw, o.h * iw, o.hm * iw, o.n * iw, o.cav * iw);
}

// Blend toward layer b by share w, with b showing first in a's low points.
fn hblend(ha: f32, hb: f32, w: f32) -> f32 {
  let x = (w - 0.5) * 2.4 + (hb - ha) * 0.9;
  var t = smoothstep(-0.3, 0.3, x);
  t = min(t, w * 10.0);
  return max(t, 1.0 - (1.0 - w) * 10.0);
}

// Parallax occlusion: walk the main layer's height field along the view ray
// (Vt in the tangent frame) and return where the ray first hits it.
fn pom(l: i32, wc: vec2f, Vt: vec3f, depth: f32, dx: vec2f, dy: vec2f) -> vec2f {
  let inv_s = 1.0 / det_size(l);
  let is_photo = det_photo(l) > 0.0;
  let gx = dx * inv_s;
  let gy = dy * inv_s;
  let steps = i32(mix(14.0, 5.0, clamp(Vt.z, 0.0, 1.0)));
  let dl = 1.0 / f32(steps);
  let shift = -Vt.xy / max(Vt.z, 0.25) * depth * dl;
  let step_st = shift * inv_s;
  var cur = wc;
  var cur_st = wc * inv_s;
  var d = 0.0;
  var prev = wc;
  var prev_gap = 1.0;
  for (var i = 0; i < steps; i++) {
    var h: f32;
    if (is_photo) { h = hex_h(l, cur_st, gx, gy); } else { h = textureSampleGrad(t_dalb, s_detail, cur_st, l, gx, gy).a; }
    let gap = (1.0 - h) - d;
    if (gap <= 0.0) {
      let t = prev_gap / max(prev_gap - gap, 1e-5);
      return mix(prev, cur, select(t, 1.0, i == 0));
    }
    prev = cur;
    prev_gap = gap;
    d += dl;
    cur += shift;
    cur_st += step_st;
  }
  return cur;
}

// Negative T.info.x marks tiles with zero water texels (render.rs).
fn tile_wet() -> bool { return T.info.x > 0.0; }

// ------------------------------------------------------------ vertices
// sp: spacing of the vertices around this one (m): the waves are filtered to it.
fn finish(Pin: vec3f, nrm: vec3f, uv: vec2f, depth: f32, skirt: f32, sp: f32) -> VOut {
  var P = Pin;
  if (skirt < 0.5) {
    if (tile_wet() && dot(P, P) < 36000000.0) {
      let m = textureSampleLevel(t_mask, s_clamp, uv, 1.0);
      if (m.r > 0.5) {
        let d = length(P);
        let fade = 1.0 - smoothstep(2500.0, 6000.0, d);
        if (fade > 0.0) {
          let upn = normalize(P - G.planet.xyz);
          let shore = smoothstep(0.5, 1.0, textureSampleLevel(t_mask, s_clamp, uv, 3.0).r);
          let st = sea_state(m.g * 255.0 / 80.0, upn.z, shore);
          // The FFT ocean: height, and the sideways shift that sharpens the crests.
          let w = ocean_disp(ocean_coord(P), sp, st) * fade;
          P += G.east.xyz * w.x + G.north.xyz * w.y + upn * w.z;
        }
      }
    }
    if (G.crater[0].w > 0.0) { P += normalize(P - G.planet.xyz) * crater_field(P).w; }
  }
  return VOut(G.view_proj * vec4f(P, 1.0), P, nrm, uv, depth);
}

// Our water seen from above, into the green channel of the 3D tiles' coverage
// mask: the height of its surface, so the mesh can give way to it (photo.wgsl).
@group(2) @binding(0) var<uniform> CS: Cascade;
struct TopOut { @builtin(position) clip: vec4f, @location(0) uv: vec2f, @location(1) h: f32, @location(2) skirt: f32 };
@vertex fn vs_top(@location(0) pos: vec3f, @location(3) skirt: f32, @location(4) uv: vec2f) -> TopOut {
  let P = pos + T.offset.xyz;
  return TopOut(CS.vp * vec4f(P, 1.0), uv, height_of(P), skirt);
}
@fragment fn fs_top(in: TopOut) -> @location(0) vec4f {
  if (in.skirt > 0.5 || textureSampleLevel(t_mask, s_clamp, in.uv, 0.0).r < 0.5) { discard; }
  return vec4f(0.0, in.h, 0.0, 0.0);
}
// Where any ground was drawn at all, into the red channel of the shore mask
// (shore.rs): land there is ground without our water on it.
@fragment fn fs_cover(in: TopOut) -> @location(0) vec4f {
  if (in.skirt > 0.5) { discard; }
  return vec4f(1.0, 0.0, 0.0, 0.0);
}

@vertex fn vs(@location(0) pos: vec3f, @location(1) depth: f32, @location(2) nrm: vec3f, @location(3) skirt: f32, @location(4) uv: vec2f) -> VOut {
  return finish(pos + T.offset.xyz, nrm, uv, depth, skirt, T.offset.w);
}

// Tessellation without a tessellation stage: every coarse grid cell near the
// camera is drawn as a TN x TN patch whose vertices snap to a coarser lattice
// with distance. Each edge picks its level from its own midpoint, so the two
// cells sharing it agree and the surface stays closed. Most cells need far
// fewer than TN x TN vertices: each is drawn as an n x n patch (n a power of
// two, from world.rs `tess_cells`) whose vertices are every (TN / n)-th of the
// full patch's, which makes the very same triangles where n is large enough.
const TN = 16u;
// Triangles are kept to about TESS_K of their distance across.
const TESS_K = 0.03;

struct CV { P: vec3f, depth: f32, N: vec3f, uv: vec2f };
fn cv(i: u32) -> CV {
  let b = i * 10u;
  return CV(vec3f(TV[b], TV[b + 1u], TV[b + 2u]) + T.offset.xyz, TV[b + 3u], vec3f(TV[b + 4u], TV[b + 5u], TV[b + 6u]), vec2f(TV[b + 8u], TV[b + 9u]));
}
// Written out so that t = 0 and t = 1 give the end points exactly.
fn lerp_cv(a: CV, b: CV, t: f32) -> CV {
  let s = 1.0 - t;
  return CV(a.P * s + b.P * t, a.depth * s + b.depth * t, a.N * s + b.N * t, a.uv * s + b.uv * t);
}
fn tess_level(mid: vec3f) -> f32 {
  let sp = max(length(mid) * TESS_K, T.offset.w / f32(TN));
  return clamp(exp2(floor(log2(max(T.offset.w / sp, 1.0)))), 1.0, f32(TN));
}
fn snapk(k: u32, lv: f32) -> f32 { return round(f32(k) * lv / f32(TN)) / lv; }

// Relief (m, along the normal) added by the tessellated surface.
fn relief(P: vec3f, N: vec3f, uv: vec2f, sp: f32) -> f32 {
  let wc = wave_coord(P);
  let up = normalize(P - G.planet.xyz);
  let nn = normalize(N);
  let img = textureSampleLevel(t_img, s_clamp, uv, 3.0).rgb;
  // Rows only need ~3 vertices per period to show, so filter for a smaller footprint than the spacing.
  let g = ground(uv, wc, img, 1.0 - dot(nn, up), vec2f(sp * 0.4, 0.0), vec2f(0.0, sp * 0.4), -normalize(P));
  if (g.amp <= 0.0) { return 0.0; }
  var h = 0.0;
  if (g.rock < 1.0) {
    // Large-scale relief only: what is smaller than the mesh is left to parallax and normals.
    var ha = 0.5;
    if (g.wb < 1.0) { ha = layer_h_lod(g.la, wc, sp); }
    var hb = ha;
    if (g.lb != g.la && g.wb > 0.0) { hb = layer_h_lod(g.lb, wc, sp); }
    h = mix((ha - 0.5) * det_range(g.la) * det_macro(g.la) * 0.5, (hb - 0.5) * det_range(g.lb) * det_macro(g.lb) * 0.5, g.wb);
    // Tussocks.
    if (g.grassy > 0.0) { h += (wn(wc, 2.3, sp) - 0.5) * 0.06 * g.grassy; }
    if (det_photo(g.mid) > 0.0 && g.midk > 0.0) { h += (layer_h_lod(g.mid, wc, sp) - 0.5) * det_range(g.mid) * g.midk; }
    h += g.hrow;
  }
  if (g.rock > 0.0) {
    let alt = dot(P, G.up.xyz) + G.up.w;
    let nw = abs(vec3f(dot(nn, G.up.xyz), dot(nn, G.east.xyz), dot(nn, G.north.xyz)));
    let nw2 = nw * nw;
    var w = nw2 * nw2;
    w /= w.x + w.y + w.z;
    var hr = 0.0;
    var tw = 0.0;
    if (w.x > 0.002) { hr += w.x * layer_h_lod(L_ROCK, wc, sp); tw += w.x; }
    if (w.y > 0.002) { hr += w.y * layer_h_lod(L_ROCK, vec2f(wc.y, alt), sp); tw += w.y; }
    if (w.z > 0.002) { hr += w.z * layer_h_lod(L_ROCK, vec2f(wc.x, alt), sp); tw += w.z; }
    hr /= max(tw, 1e-4);
    h = mix(h, (hr - 0.5) * det_range(L_ROCK) * det_macro(L_ROCK) * 0.5, g.rock);
  }
  return h * g.amp;
}

// `cell`: the cell's index in the grid | log2(n) << 16 (render.rs `TessCells`).
@vertex fn vs_tess(@builtin(vertex_index) vi: u32, @location(0) cell: u32) -> VOut {
  let n = 1u << (cell >> 16u);
  let ci = (cell & 0xffffu) % GRID;
  let cj = (cell & 0xffffu) / GRID;
  // The vertex's place in the full TN x TN patch.
  let a = vi % (n + 1u) * (TN / n);
  let b = vi / (n + 1u) * (TN / n);
  let i00 = cj * GV + ci;
  var v: CV;
  if (b == 0u) {
    let c00 = cv(i00);
    let c10 = cv(i00 + 1u);
    v = lerp_cv(c00, c10, snapk(a, tess_level((c00.P + c10.P) * 0.5)));
  } else if (b == TN) {
    let c01 = cv(i00 + GV);
    let c11 = cv(i00 + GV + 1u);
    v = lerp_cv(c01, c11, snapk(a, tess_level((c01.P + c11.P) * 0.5)));
  } else if (a == 0u) {
    let c00 = cv(i00);
    let c01 = cv(i00 + GV);
    v = lerp_cv(c00, c01, snapk(b, tess_level((c00.P + c01.P) * 0.5)));
  } else if (a == TN) {
    let c10 = cv(i00 + 1u);
    let c11 = cv(i00 + GV + 1u);
    v = lerp_cv(c10, c11, snapk(b, tess_level((c10.P + c11.P) * 0.5)));
  } else {
    let c00 = cv(i00);
    let c11 = cv(i00 + GV + 1u);
    let lv = tess_level((c00.P + c11.P) * 0.5);
    let fa = snapk(a, lv);
    let fb = snapk(b, lv);
    let c10 = cv(i00 + 1u);
    let c01 = cv(i00 + GV);
    // Same diagonal as the coarse mesh.
    if (fa + fb <= 1.0) {
      v = CV(c00.P + (c10.P - c00.P) * fa + (c01.P - c00.P) * fb, c00.depth, c00.N + (c10.N - c00.N) * fa + (c01.N - c00.N) * fb, c00.uv + (c10.uv - c00.uv) * fa + (c01.uv - c00.uv) * fb);
    } else {
      let ga = 1.0 - fa;
      let gb = 1.0 - fb;
      v = CV(c11.P + (c01.P - c11.P) * ga + (c10.P - c11.P) * gb, c11.depth, c11.N + (c01.N - c11.N) * ga + (c10.N - c11.N) * gb, c11.uv + (c01.uv - c11.uv) * ga + (c10.uv - c11.uv) * gb);
    }
  }
  var P = v.P;
  let d = length(P);
  // Edges are back to level 1 (unsplit) beyond this: the relief must be gone
  // by then, so the block joins the plain cells around it without cracks. Also
  // fade at tile borders (the neighbour tile cannot follow).
  let far = T.offset.w / (2.0 * TESS_K);
  let edge = min(min(v.uv.x, 1.0 - v.uv.x), min(v.uv.y, 1.0 - v.uv.y)) * f32(GRID);
  let fade = (1.0 - smoothstep(far * 0.5, far * 0.9, d)) * smoothstep(0.0, 1.0, edge);
  if (fade > 0.0) {
    var water = 0.0;
    if (tile_wet()) { water = textureSampleLevel(t_mask, s_clamp, v.uv, 0.0).r; }
    if (water < 0.05) {
      let sp = max(d * TESS_K, T.offset.w / f32(TN));
      P += normalize(v.N) * relief(P, v.N, v.uv, sp) * fade;
    }
  }
  // The waves follow the finer spacing where the cells are split, back to
  // the plain cells' where the block must join them.
  let wsp = mix(T.offset.w, max(d * TESS_K, T.offset.w / f32(TN)), fade);
  return finish(P, v.N, v.uv, v.depth, 0.0, wsp);
}

// The rest of a tessellated tile, one quad per cell; the cells drawn by
// vs_tess collapse to nothing.
@vertex fn vs_cells(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> VOut {
  let ci = ii % GRID;
  let cj = ii / GRID;
  let t = vec4u(T.tess);
  if (ci >= t.x && ci < t.x + t.z && cj >= t.y && cj < t.y + t.w) {
    return VOut(vec4f(2.0, 2.0, 2.0, 1.0), vec3f(0.0), vec3f(0.0, 0.0, 1.0), vec2f(0.0), 0.0);
  }
  let v = cv(cj * GV + ci + (vi & 1u) + (vi >> 1u) * GV);
  return finish(v.P, v.N, v.uv, v.depth, 0.0, T.offset.w);
}

// ------------------------------------------------------------ city lights
// Building tiles only load within a few km, and only they have lit windows.
// Beyond them the ground itself glows with the city's light (sources.rs
// `NightLights`): up close as scattered points (windows, lamps), each holding
// the light of its whole cell, and once the cells shrink below a pixel or so,
// as their average. Where a building tile is drawn it takes over, fading in
// as the tile does, so the lights never stop at the edge of what is loaded.
const GLOW_CELLS = 128.0;   // cells across a building tile (~15 m at 40 deg)
const GLOW_K = 0.0016;      // radiance of the brightest downtown (x sun_dir.w)
const GLOBE_LIGHTS = 60.0;  // their gain on the start screen's globe, seen from orbit

// The points of one level: cells 2^lv base cells across, each with at most
// one light (chance p, carrying 1/p cells' worth) of random brightness and
// colour. `J`: base cells per pixel along screen x and y. Each light is
// drawn round on screen, about a pixel across.
fn glow_dots(gq: vec2f, dens: f32, J: mat2x2f, lv: u32) -> vec3f {
  let sc = f32(1u << lv);
  let bits = bitcast<u32>(T.info.w);
  let bi = vec2u(bits & 16383u, bits >> 14u) * u32(GLOW_CELLS);
  let m = (1u << lv) - 1u;
  // Exact cell index on Earth: the square's corner in whole cells of this level, and the rest.
  let cq = gq * (GLOW_CELLS / sc) + vec2f(bi & vec2u(m)) / sc;
  let id = vec2i(bi >> vec2u(lv)) + vec2i(floor(cq));
  let h0 = hx_pcg(bitcast<u32>(id.x) ^ hx_pcg(bitcast<u32>(id.y) + 0x9e3779b9u + lv * 0x632be5abu));
  let h1 = hx_pcg(h0);
  let h2 = hx_pcg(h1);
  let h3 = hx_pcg(h2);
  let u = vec4f(f32(h0 >> 8u), f32(h1 >> 8u), f32(h2 >> 8u), f32(h3 >> 8u)) / 16777216.0;
  let p = 1.0 - pow(1.0 - clamp(dens * 1.8, 0.0, 0.999), f32(1u << (2u * min(lv, 8u))));
  if (u.x >= p) { return vec3f(0.0); }
  // Offset from the light in pixels, and the cell's area in pixels.
  let Jl = J * (1.0 / sc);
  let det = Jl[0].x * Jl[1].y - Jl[0].y * Jl[1].x;
  let d = fract(cq) - (0.3 + 0.4 * u.yz);
  let px = vec2f(Jl[1].y * d.x - Jl[1].x * d.y, -Jl[0].y * d.x + Jl[0].x * d.y) / det;
  let area = 1.0 / max(abs(det), 1e-6);
  let rr = 0.85;
  let w = 2.0 * fract(u.w * 7.31);
  let col = select(select(vec3f(0.85, 0.9, 1.0), vec3f(1.0, 0.72, 0.42), u.w < 0.9), vec3f(1.0, 0.55, 0.22), u.w < 0.55);
  // Up close (cells many pixels across) a light is a small blob of its own size, not a pixel.
  let big = clamp(sqrt(area) * 0.12, rr, 6.0);
  return col * (w * dens / p * area * exp(-dot(px, px) / (big * big)) / (PI * big * big));
}

fn cover_at(i: vec2i) -> f32 {
  let n = i32(textureDimensions(t_cover).x);
  if (any(i < vec2i(0)) || any(i >= vec2i(n))) { return 0.0; }
  return textureLoad(t_cover, i, 0).r;
}

fn city_glow(gq: vec2f, dens: f32, dcx: vec2f, dcy: vec2f, upn: vec3f) -> vec3f {
  let night = night_at(upn);
  if (night <= 0.0 || dens < 0.004) { return vec3f(0.0); }
  // Bilinear over the (already softly eroded, world.rs `light_cover`) cover,
  // eased, so the hand-over has no tile-shaped steps.
  let cp = gq - 0.5;
  let c0 = vec2i(floor(cp));
  let cf = cp - floor(cp);
  let a = cover_at(c0);
  let b = cover_at(c0 + vec2i(1, 0));
  let c = cover_at(c0 + vec2i(0, 1));
  let d = cover_at(c0 + vec2i(1, 1));
  let cl = mix(mix(a, b, cf.x), mix(c, d, cf.x), cf.y);
  let cover = cl * cl * (3.0 - 2.0 * cl);
  if (cover >= 0.999) { return vec3f(0.0); }
  // Lights stay points a pixel or two across: as they shrink, neighbouring
  // ones merge into one carrying all their light (powers of two, cross-faded).
  let J = mat2x2f(dcx, dcy);
  // Cells per pixel (linear, from the area a cell covers on screen).
  let fp0 = sqrt(abs(dcx.x * dcy.y - dcx.y * dcy.x));
  let lv = clamp(log2(fp0 / 0.3), 0.0, 12.0);
  let l0 = floor(lv);
  let t = lv - l0;
  var col = glow_dots(gq, dens, J, u32(l0)) * (1.0 - t);
  if (t > 0.0) { col += glow_dots(gq, dens, J, u32(l0) + 1u) * t; }
  // A quarter of it as the warm light spilt on streets and walls between the points.
  col = col * 0.75 + vec3f(1.0, 0.62, 0.34) * dens * 0.25;
  // From orbit (the start screen's globe) a pixel spans whole towns: their
  // average, as smooth as the Black Marble itself.
  let gk = globe_k();
  if (gk > 0.0) { col = mix(col, vec3f(1.0, 0.68, 0.40) * dens * 5.0, gk); }
  // On the start screen's globe the night side is shown as in the long
  // exposures the Black Marble was made from, against the day side's exposure.
  return col * (GLOW_K * G.sun_dir.w * night * (1.0 - cover)) * mix(1.0, GLOBE_LIGHTS, globe_k());
}

// ------------------------------------------------------------ surface
fn fs_inner(in: VOut, check_photo: bool) -> GOut {
  // Derivatives first, while every pixel of the quad is still running.
  let wc0 = wave_coord(in.P);
  let dx = dpdx(wc0);
  let dy = dpdy(wc0);
  let alt = dot(in.P, G.up.xyz) + G.up.w;
  let dalt = vec2f(dpdx(alt), dpdy(alt));
  let duvx = dpdx(in.uv);
  let duvy = dpdy(in.uv);
  let img = textureSample(t_img, s_aniso, in.uv).rgb;
  let wet_tile = tile_wet();
  if (wet_tile) {
    let m = textureSampleGrad(t_mask, s_clamp, in.uv, duvx, duvy);
    if (m.r > 0.5) {
      let shore = smoothstep(0.5, 1.0, textureSampleLevel(t_mask, s_clamp, in.uv, 3.0).r);
      let wclass = round(m.g * 255.0 / 80.0);
      // Bathymetry is coarse (and missing in bays): never shallower than a plausible shore profile.
      let profile = 0.35 + shore * shore * select(select(9.0, 3.5, wclass > 2.5), 14.0, wclass < 1.5);
      var depth = max(mix(min(in.depth, 1.2), in.depth, shore), profile);
      return GOut(vec4f(img, 1.0), vec4f(max(depth, 0.15), wclass, shore, 1.0), 0.0);
    }
  }
  // Inside the 3D tiles zone the mesh stands in for the land (tiles3d.rs);
  // our water stays, its waves and reflections over the mesh's still photo.
  if (check_photo && photo_hides(in.P, in.clip.xy)) { discard; }
  var albedo = img;
  let dist = length(in.P);
  let V = -in.P / max(dist, 1e-3);
  let pp = in.P - G.planet.xyz;
  let pr = length(pp);
  let upn = pp / pr;
  var N = in.N;
  if (dot(N, N) < 1e-8) { N = upn; } else { N = normalize(N); }
  let Ngeo = N;
  var wc = wc0;
  let fw = max(length(dx), length(dy));
  // low preset keeps the ground detail materials to half the distance.
  let lowq = G.quality.x > 1.5;
  let det_far = DET_FAR * select(1.0, 0.5, lowq);
  if (dist < det_far) {
    let fade = 1.0 - smoothstep(det_far * 0.45, det_far, dist);
    let g = ground(in.uv, wc0, img, 1.0 - dot(N, upn), dx, dy, V);
    // Tangent frame along east / north, laid onto the surface.
    let t = normalize(G.east.xyz - N * dot(G.east.xyz, N));
    let b = cross(N, t);
    let graze = 1.0 - abs(dot(V, N));
    let graze2 = graze * graze;
    // Parallax on the main layer up close.
    let pk = (1.0 - smoothstep(18.0, 40.0, dist)) * (1.0 - g.rock) * g.amp;
    // low preset goes without the parallax march.
    if (pk > 0.0 && !lowq) {
      let Vt = vec3f(dot(V, t), dot(V, b), dot(V, N));
      wc = pom(g.la, wc0, Vt, det_range(g.la) * pk, dx, dy);
    }
    let lite = select(smoothstep(120.0, 180.0, dist), smoothstep(30.0, 60.0, dist), lowq);
    var c = vec3f(0.0);
    var nd = vec3f(0.0);
    var cav = 1.0;
    var cavm = 1.0;
    var h = 0.5;
    var kr = 0.0;
    if (g.rock < 1.0) {
      let A = det(g.la, wc, dx, dy, t, b, wc, fw, lite);
      var B = A;
      var wb = g.wb * (1.0 - 0.7 * graze2 * g.grassy);
      if (g.lb != g.la && wb > 0.001) { B = det(g.lb, wc, dx, dy, t, b, wc, fw, lite); }
      let k = hblend(A.h, B.h, wb);
      let ca = g.plant * A.c;
      let cb = mix(img, g.soil, g.soil_abs) * B.c;
      c = mix(ca, cb, k) * g.tint;
      nd = mix(A.n, B.n, k) + t * (-g.grow.x) + b * (-g.grow.y);
      cav = mix(A.cav, B.cav, k);
      cavm = mix(cav_mean(g.la), cav_mean(g.lb), k);
      h = mix(A.h, B.h, k);
    }
    var tw_rock = vec3f(0.0);
    if (g.rock > 0.001) {
      tw_rock = tri_w(N);
      var R: Det;
      if (det_photo(L_ROCK) > 0.0) {
        R = tri_det_photo(L_ROCK, wc, alt, tw_rock, mat2x2f(dx, dy), dalt, lite);
      } else {
        R = tri_det(L_ROCK, wc, alt, tw_rock, mat2x2f(dx, dy), dalt, fw, lite);
      }
      kr = select(hblend(h, R.h, g.rock), 1.0, g.rock >= 1.0);
      // Rock takes the photo's brightness, greyed.
      let rc = mix(img, vec3f(luma(img)), 0.55) * R.c;
      c = mix(c, rc, kr);
      nd = mix(nd, R.n, kr);
      cav = mix(cav, R.cav, kr);
      cavm = mix(cavm, cav_mean(L_ROCK), kr);
    }
    // Mid-scale detail: tussocks, bare patches, stones, fractures, finer than
    // a satellite pixel. Rock gets its own, projected like the close-up rock.
    var mk = g.midk * select(0.0, 1.0, det_photo(g.mid) > 0.0);
    let mrk = kr * select(0.0, 1.0, det_photo(M_ROCK) > 0.0);
    // low preset goes without the mid scale layer.
    if ((mk > 0.0 || mrk > 0.0) && !lowq) {
      let mlite = smoothstep(350.0, 500.0, dist);
      var mcm = cav_mean(g.mid);
      var M = Det(vec3f(1.0), 0.5, 0.0, vec3f(0.0), mcm);
      if (mk > 0.0 && mrk < 1.0) { M = det_photo_only(g.mid, wc0, dx, dy, t, b, mlite); }
      M.c = mix(vec3f(1.0), M.c, mk);
      M.n *= mk;
      if (mrk > 0.0) {
        let R = tri_det_photo(M_ROCK, wc0, alt, tw_rock, mat2x2f(dx, dy), dalt, mlite);
        M = Det(mix(M.c, R.c, mrk), M.h, M.hm, mix(M.n, R.n, mrk), mix(M.cav, R.cav, mrk));
        mcm = mix(mcm, cav_mean(M_ROCK), mrk);
        mk = max(mk, mrk);
      }
      c *= M.c;
      nd += M.n;
      // Its cavities darken like the close-up ones (relative to its mean).
      c *= mix(1.0, (0.55 + 0.45 * M.cav) / (0.55 + 0.45 * mcm), mk);
    }
    // Blades hide the ground between them at grazing angles, and shine.
    cav = mix(cav, 1.0, graze2 * g.grassy);
    c *= 1.0 + 0.18 * (graze2 * graze2) * g.grassy;
    // Sunlit blades are warmer than the hazy photo suggests.
    c *= mix(vec3f(1.0), vec3f(1.03, 1.04, 0.84), g.grassy);
    // Darken the cracks and gaps without changing the average brightness of the photo.
    c *= (0.55 + 0.45 * cav) / (0.55 + 0.45 * mix(cavm, 1.0, graze2 * g.grassy));
    albedo = mix(img, c, fade);
    N = normalize(N + nd * fade);
  }
  // Snow lying on the ground (snow.rs): from the height above the snow line,
  // the slope and the way the ground faces, broken by noise into tongues and
  // bare patches near the line. Water returned above; roads and runways are
  // drawn over it, cleared.
  if (snow_on()) {
    let palt = pr - G.planet.w;
    let sline = snow_line_at(in.P);
    if (palt > sline - 950.0 && dot(Ngeo, upn) > 0.34) {
      let s1 = snow_noise(wc0, 2300.0, fw);
      let s2 = snow_noise(wc0 + 517.0, 610.0, fw);
      // The slope through noise too, so the cut-off does not trace the mesh's triangles.
      let Ns = normalize(Ngeo + upn * ((s2 - 0.5) * 0.35));
      var ks = snow_cover(palt + ((s1 - 0.5) * 0.6 + (s2 - 0.5) * 0.4) * SNOW_BAND * 1.2, dot(Ns, upn), dot(Ns, G.north.xyz) * G.snow.z, sline);
      if (ks > 0.0) {
        let s3 = snow_noise(wc0 - 133.0, 47.0, fw);
        // Ragged edges where it thins out, solid where it is deep.
        ks = clamp(ks + (s3 - 0.5) * 2.0 * min(ks, 1.0 - ks), 0.0, 1.0);
        // The class from the nearest texel: filtered, the packed bits blend into
        // other classes along every boundary.
        let mdim = vec2f(textureDimensions(t_mask, 0));
        let mt = textureLoad(t_mask, vec2i(clamp(floor(in.uv * mdim), vec2f(0.0), mdim - 1.0)), 0);
        let cls = u32(round(mt.b * 255.0)) & 15u;
        // Towns and roads in the photo are partly cleared; in woods the snow on
        // the ground shows between dark trunks and crowns.
        ks *= select(1.0, 0.85, cls == GC_PAVED);
        // Bare rock in the photo is mostly steep or windswept: it keeps some showing.
        ks *= select(1.0, 0.8, cls == GC_ROCK);
        var sc = SNOW_ALBEDO * (0.93 + 0.14 * s3);
        if (cls == GC_WOOD) { sc = mix(img, sc, 0.45); }
        // Dynamic tracks and crash trails carved into the snow (snowtrack.rs):
        // scrape snow away to reveal the ground underneath, compact what remains,
        // pile snow into raised berms along the rims, and bend the normal so the
        // indented ruts and snow banks catch the light.
        var tr_ao = 1.0;
        var tr_grad = vec2f(0.0);
        let ks_orig = ks;
        if (ks > 0.005) {
          let tr = snow_track_at(in.P, dist, fw);
          if (tr.x > 0.002 || tr.y > 0.002) {
            let tn1 = snow_noise(wc0 + 71.0, 0.65, fw);
            let tn2 = snow_noise(wc0 - 37.0, 2.3, fw);
            let rough = tn1 * 0.65 + tn2 * 0.35;
            let reveal = smoothstep(0.12, 0.68, tr.x + (rough - 0.5) * 0.22 * tr.x);
            // Clear snow in the trough; pile displaced snow slightly thicker on berms.
            ks = clamp(ks_orig * (1.0 - reveal) + tr.y * 0.35 * ks_orig * (0.7 + 0.6 * tn1), 0.0, 1.0);
            // Compacted snow left in the rut is slightly darker and cooler in tone;
            // fluffed snow in the raised berms gets subtle clumpy variation.
            let comp = tr.x * (1.0 - reveal * 0.45);
            sc *= mix(1.0, 0.72 + 0.14 * tn1, comp);
            sc = mix(sc, sc * vec3f(0.84, 0.91, 1.04), comp * 0.55);
            sc *= 1.0 + tr.y * 0.09 * (tn1 - 0.30);
            // Dampen the exposed ground beneath scraped snow, and mix a touch of
            // churned topsoil on deep crash/belly gouges.
            albedo *= 1.0 - 0.35 * reveal * ks_orig;
            let gouge = smoothstep(0.82, 1.0, tr.x) * ks_orig * 0.48 * (0.5 + 0.5 * tn2);
            albedo = mix(albedo, vec3f(0.13, 0.10, 0.075) * (0.7 + 0.6 * tn1), gouge);
            tr_ao = 1.0 - 0.22 * tr.x * ks_orig;
            tr_grad = tr.zw;
          }
        }
        albedo = mix(albedo, sc, ks) * tr_ao;
        // It smooths over the ground's small relief, while tracks carve new slopes in it.
        N = normalize(mix(N, Ngeo, ks * 0.75));
        if (dot(tr_grad, tr_grad) > 1e-8) {
          let n_fade = (1.0 - smoothstep(180.0, 480.0, dist)) * ks_orig;
          N = normalize(N - (G.snow_tr_e.xyz * tr_grad.x + G.snow_tr_n.xyz * tr_grad.y) * (1.15 * n_fade));
        }
      }
    }
  }
  if (G.scorch[0].w > 0.0) {
    for (var i = 0; i < 8; i++) {
      let s = G.scorch[i];
      if (s.w <= 0.0) { break; }
      let d = length(in.P - s.xyz) / s.w;
      if (d < 1.0) {
        let n = textureSampleLevel(t_wave, s_repeat, wc0 / 23.0 + f32(i) * 0.37, 0.0).a;
        let k = smoothstep(1.0, 0.25, d + (n - 0.5) * 0.7);
        albedo = mix(albedo, vec3f(0.012, 0.011, 0.01), k * 0.94);
      }
    }
  }
  // Wet sand along the waterline: darker, and faintly glossy.
  if (wet_tile && dist < 3000.0) {
    let wet = smoothstep(0.15, 0.45, textureSampleLevel(t_mask, s_clamp, in.uv, 1.0).r) * (1.0 - smoothstep(800.0, 3000.0, dist));
    albedo *= 1.0 - 0.3 * wet;
  }
  // Craters: bent normals, bare earth and rock where the ground was dug out.
  if (G.crater[0].w > 0.0) {
    let cr = crater_field(in.P);
    if (dot(cr.xyz, cr.xyz) > 1e-6 || cr.w != 0.0) {
      N = normalize(N - cr.xyz);
      let rk = textureSampleLevel(t_wave, s_repeat, wc0 / 4.3, 0.0).a;
      let rk2 = textureSampleLevel(t_wave, s_repeat, wc0 / 0.8, 0.0).a;
      let soil = mix(vec3f(0.11, 0.085, 0.06), vec3f(0.19, 0.17, 0.15), smoothstep(0.4, 0.65, rk)) * (0.6 + 0.8 * rk2);
      albedo = mix(albedo, soil, clamp(abs(cr.w) * 1.6 + length(cr.xyz) * 1.5, 0.0, 1.0));
    }
  }
  var c = shade(in.P, N, albedo);
  if (night_at(upn) > 0.0) {
    let lights = textureSampleGrad(t_lights, s_clamp, in.uv, duvx, duvy).r;
    if (lights >= 0.004) {
      let gq_scale = exp2(14.0 - abs(T.info.x));
      let gq = T.info.yz + in.uv * gq_scale;
      let dcx = duvx * (gq_scale * GLOW_CELLS);
      let dcy = duvy * (gq_scale * GLOW_CELLS);
      c += city_glow(gq, lights, dcx, dcy, upn);
    }
  }
  return GOut(vec4f(c, 1.0), vec4f(N, 0.0), amb_share(c));
}

@fragment fn fs(in: VOut) -> GOut { return fs_inner(in, false); }
@fragment fn fs_photo(in: VOut) -> GOut { return fs_inner(in, true); }

// Distant tiles completely beyond det_far and outside the 3D tiles zone: no
// close-up detail, POM, or hex-tiling code -> tiny register count and high GPU occupancy.
@fragment fn fs_far(in: VOut) -> GOut {
  let duvx = dpdx(in.uv);
  let duvy = dpdy(in.uv);
  let img = textureSample(t_img, s_aniso, in.uv).rgb;
  let wet_tile = tile_wet();
  if (wet_tile) {
    let m = textureSampleGrad(t_mask, s_clamp, in.uv, duvx, duvy);
    if (m.r > 0.5) {
      let shore = smoothstep(0.5, 1.0, textureSampleLevel(t_mask, s_clamp, in.uv, 3.0).r);
      let wclass = round(m.g * 255.0 / 80.0);
      let profile = 0.35 + shore * shore * select(select(9.0, 3.5, wclass > 2.5), 14.0, wclass < 1.5);
      var depth = max(mix(min(in.depth, 1.2), in.depth, shore), profile);
      return GOut(vec4f(img, 1.0), vec4f(max(depth, 0.15), wclass, shore, 1.0), 0.0);
    }
  }
  var albedo = img;
  let pp = in.P - G.planet.xyz;
  let pr = length(pp);
  let upn = pp / pr;
  var N = in.N;
  if (dot(N, N) < 1e-8) { N = upn; } else { N = normalize(N); }
  if (snow_on()) {
    let wc0 = wave_coord(in.P);
    let fw = max(length(dpdx(wc0)), length(dpdy(wc0)));
    let palt = pr - G.planet.w;
    let sline = snow_line_at(in.P);
    if (palt > sline - 950.0 && dot(N, upn) > 0.34) {
      let s1 = snow_noise(wc0, 2300.0, fw);
      let s2 = snow_noise(wc0 + 517.0, 610.0, fw);
      let Ns = normalize(N + upn * ((s2 - 0.5) * 0.35));
      var ks = snow_cover(palt + ((s1 - 0.5) * 0.6 + (s2 - 0.5) * 0.4) * SNOW_BAND * 1.2, dot(Ns, upn), dot(Ns, G.north.xyz) * G.snow.z, sline);
      if (ks > 0.0) {
        let s3 = snow_noise(wc0 - 133.0, 47.0, fw);
        ks = clamp(ks + (s3 - 0.5) * 2.0 * min(ks, 1.0 - ks), 0.0, 1.0);
        let mdim = vec2f(textureDimensions(t_mask, 0));
        let mt = textureLoad(t_mask, vec2i(clamp(floor(in.uv * mdim), vec2f(0.0), mdim - 1.0)), 0);
        let cls = u32(round(mt.b * 255.0)) & 15u;
        ks *= select(1.0, 0.85, cls == GC_PAVED);
        ks *= select(1.0, 0.8, cls == GC_ROCK);
        var sc = SNOW_ALBEDO * (0.93 + 0.14 * s3);
        if (cls == GC_WOOD) { sc = mix(img, sc, 0.45); }
        var tr_ao = 1.0;
        let dist = length(in.P);
        if (ks > 0.005 && dist < 1400.0) {
          let tr = snow_track_at(in.P, dist, fw);
          if (tr.x > 0.002 || tr.y > 0.002) {
            let ks_orig = ks;
            let tn1 = snow_noise(wc0 + 71.0, 0.65, fw);
            let tn2 = snow_noise(wc0 - 37.0, 2.3, fw);
            let rough = tn1 * 0.65 + tn2 * 0.35;
            let reveal = smoothstep(0.12, 0.68, tr.x + (rough - 0.5) * 0.22 * tr.x);
            ks = clamp(ks_orig * (1.0 - reveal) + tr.y * 0.35 * ks_orig * (0.7 + 0.6 * tn1), 0.0, 1.0);
            let comp = tr.x * (1.0 - reveal * 0.45);
            sc *= mix(1.0, 0.72 + 0.14 * tn1, comp);
            sc = mix(sc, sc * vec3f(0.84, 0.91, 1.04), comp * 0.55);
            sc *= 1.0 + tr.y * 0.09 * (tn1 - 0.30);
            albedo *= 1.0 - 0.35 * reveal * ks_orig;
            let gouge = smoothstep(0.82, 1.0, tr.x) * ks_orig * 0.48 * (0.5 + 0.5 * tn2);
            albedo = mix(albedo, vec3f(0.13, 0.10, 0.075) * (0.7 + 0.6 * tn1), gouge);
            tr_ao = 1.0 - 0.22 * tr.x * ks_orig;
          }
        }
        albedo = mix(albedo, sc, ks) * tr_ao;
      }
    }
  }
  if (G.scorch[0].w > 0.0) {
    let wc0 = wave_coord(in.P);
    for (var i = 0; i < 8; i++) {
      let s = G.scorch[i];
      if (s.w <= 0.0) { break; }
      let d = length(in.P - s.xyz) / s.w;
      if (d < 1.0) {
        let n = textureSampleLevel(t_wave, s_repeat, wc0 / 23.0 + f32(i) * 0.37, 0.0).a;
        let k = smoothstep(1.0, 0.25, d + (n - 0.5) * 0.7);
        albedo = mix(albedo, vec3f(0.012, 0.011, 0.01), k * 0.94);
      }
    }
  }
  if (wet_tile) {
    let dist = length(in.P);
    if (dist < 3000.0) {
      let wet = smoothstep(0.15, 0.45, textureSampleLevel(t_mask, s_clamp, in.uv, 1.0).r) * (1.0 - smoothstep(800.0, 3000.0, dist));
      albedo *= 1.0 - 0.3 * wet;
    }
  }
  if (G.crater[0].w > 0.0) {
    let cr = crater_field(in.P);
    if (dot(cr.xyz, cr.xyz) > 1e-6 || cr.w != 0.0) {
      let wc0 = wave_coord(in.P);
      N = normalize(N - cr.xyz);
      let rk = textureSampleLevel(t_wave, s_repeat, wc0 / 4.3, 0.0).a;
      let rk2 = textureSampleLevel(t_wave, s_repeat, wc0 / 0.8, 0.0).a;
      let soil = mix(vec3f(0.11, 0.085, 0.06), vec3f(0.19, 0.17, 0.15), smoothstep(0.4, 0.65, rk)) * (0.6 + 0.8 * rk2);
      albedo = mix(albedo, soil, clamp(abs(cr.w) * 1.6 + length(cr.xyz) * 1.5, 0.0, 1.0));
    }
  }
  // T.tess.x holds the tile's shadow mask: bit 0 = shadow_world, bit 1 = plane_shadow.
  let smask = u32(T.tess.x);
  let l = light_at(in.P);
  var sh = 1.0;
  if (smask != 0u && G.csm.w > 0.0 && dot(N, G.sun_dir.xyz) > 0.0 && (l.sun.x + l.sun.y + l.sun.z) > 0.0) {
    if ((smask & 1u) != 0u) { sh *= shadow_world(in.P, N); }
    if ((smask & 2u) != 0u) { sh *= plane_shadow(in.P, N); }
  }
  var c = shade_lit(in.P, N, albedo, l, sh);
  if (night_at(upn) > 0.0) {
    let lights = textureSampleGrad(t_lights, s_clamp, in.uv, duvx, duvy).r;
    if (lights >= 0.004) {
      let gq_scale = exp2(14.0 - abs(T.info.x));
      let gq = T.info.yz + in.uv * gq_scale;
      let dcx = duvx * (gq_scale * GLOW_CELLS);
      let dcy = duvy * (gq_scale * GLOW_CELLS);
      c += city_glow(gq, lights, dcx, dcy, upn);
    }
  }
  return GOut(vec4f(c, 1.0), vec4f(N, 0.0), amb_share(c));
}

// Tiles whose entire mask is water: minimal G-buffer write with maximum GPU occupancy.
@fragment fn fs_water(in: VOut) -> GOut {
  let img = textureSample(t_img, s_aniso, in.uv).rgb;
  let m = textureSample(t_mask, s_clamp, in.uv);
  let shore = smoothstep(0.5, 1.0, textureSampleLevel(t_mask, s_clamp, in.uv, 3.0).r);
  let wclass = round(m.g * 255.0 / 80.0);
  let profile = 0.35 + shore * shore * select(select(9.0, 3.5, wclass > 2.5), 14.0, wclass < 1.5);
  let depth = max(mix(min(in.depth, 1.2), in.depth, shore), profile);
  return GOut(vec4f(img, 1.0), vec4f(max(depth, 0.15), wclass, shore, 1.0), 0.0);
}
