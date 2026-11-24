// Smoke volume: the gas particles are splatted into a density grid around the
// effects anchor, smoothed, and then lit the way the cloud layer is: by
// marching toward the sun through the density to get how much light gets in.
// The result also shadows the ground and lets smoke glow over the fire.
@group(1) @binding(0) var<uniform> S: PSim;
@group(1) @binding(1) var<storage, read> parts: array<Particle>;
// Fixed-point (density, emitter density) per cell.
@group(1) @binding(2) var<storage, read_write> acc: array<atomic<u32>>;
@group(1) @binding(3) var<storage, read_write> tmp_a: array<vec2f>;
@group(1) @binding(4) var<storage, read_write> tmp_b: array<vec2f>;
@group(1) @binding(5) var vol_out: texture_storage_3d<rgba16float, write>;
@group(1) @binding(6) var t_vol: texture_3d<f32>;
@group(1) @binding(7) var light_out: texture_storage_3d<rgba16float, write>;

const NX = 40u;
const NY = 40u;
const NZ = 48u;
const FIX = 4096.0;

fn cell_index(c: vec3i) -> u32 { return (u32(c.z) * NY + u32(c.y)) * NX + u32(c.x); }
fn inside(c: vec3i) -> bool { return all(c >= vec3i(0)) && all(c < vec3i(i32(NX), i32(NY), i32(NZ))); }

// Optical depth straight through the middle of a fresh puff, per unit alpha.
fn puff_tau(kind: i32) -> f32 {
  if (kind == 0 || kind == 8) { return 3.2; }
  if (kind == 5) { return 1.4; }
  return 2.2;
}
// How strongly the gas glows, from its temperature parameter.
fn emitter(temp: f32, kind: i32) -> f32 {
  if (kind != 0) { return 0.0; }
  return smoothstep(0.35, 2.2, temp) * 2.5;
}
// Current size and opacity fade of a particle (must match particles_draw).
fn puff_size(p: Particle) -> f32 {
  let k = p.age / p.life;
  return mix(p.size0, p.size1, 1.0 - (1.0 - k) * (1.0 - k));
}
fn puff_fade(p: Particle) -> f32 {
  let k = p.age / p.life;
  return smoothstep(0.0, 0.06, k) * (1.0 - smoothstep(0.55, 1.0, k));
}

fn add(c: vec3i, w: f32, sig: f32, heat: f32) {
  if (!inside(c) || w <= 0.0) { return; }
  let i = cell_index(c) * 2u;
  atomicAdd(&acc[i], u32(sig * w * FIX + 0.5));
  if (heat > 0.0) { atomicAdd(&acc[i + 1u], u32(heat * w * FIX + 0.5)); }
}

@compute @workgroup_size(64)
fn splat(@builtin(global_invocation_id) id: vec3u) {
  let i = id.x;
  if (i >= arrayLength(&parts)) { return; }
  let p = parts[i];
  if (p.age >= p.life) { return; }
  let kind = i32(p.kind + 0.5);
  if (!(kind == 0 || kind == 2 || kind == 5 || kind == 7 || kind == 8)) { return; }
  let cs = S.ve.w;
  let size = puff_size(p);
  let tau = p.alpha * puff_fade(p) * puff_tau(kind);
  if (tau <= 0.001) { return; }
  // Total extinction of the puff (tau / diameter * volume), spread over cells.
  let mass = tau * 2.0944 * size * size / (cs * cs * cs);
  let heat = mass * emitter(p.temp, kind);
  let up = S.upv.xyz;
  let q = vec3f(dot(p.pos, S.ve.xyz), dot(p.pos, S.vn.xyz), dot(p.pos, up) - S.vn.w);
  let g = q / cs + vec3f(f32(NX) * 0.5, f32(NY) * 0.5, 0.0) - 0.5;
  let R = size / cs;
  if (R < 1.5) {
    // Small puff: trilinear point splat.
    let c0 = vec3i(floor(g));
    let f = g - floor(g);
    for (var k = 0; k < 8; k++) {
      let o = vec3i(k & 1, (k >> 1) & 1, (k >> 2) & 1);
      let w3 = mix(1.0 - f, f, vec3f(o));
      add(c0 + o, w3.x * w3.y * w3.z, mass, heat);
    }
    return;
  }
  // Big puff: a parabolic ball of cells, capped so a huge one stays affordable.
  let Rc = min(R, 3.5);
  let norm = 1.0 / (1.6755 * Rc * Rc * Rc);  // 8/15 pi R^3
  let lo = vec3i(ceil(g - Rc));
  let hi = vec3i(floor(g + Rc));
  for (var z = lo.z; z <= hi.z; z++) {
    for (var y = lo.y; y <= hi.y; y++) {
      for (var x = lo.x; x <= hi.x; x++) {
        let d = vec3f(f32(x), f32(y), f32(z)) - g;
        let w = 1.0 - dot(d, d) / (Rc * Rc);
        add(vec3i(x, y, z), w * norm, mass, heat);
      }
    }
  }
}

// Separable [1 4 6 4 1] smoothing, one axis per pass.
fn kw(k: i32) -> f32 { let a = abs(k); return (6.0 - 2.0 * f32(a) - select(0.0, 1.0, a == 2)) / 16.0; }
fn read_acc(c: vec3i) -> vec2f {
  if (!inside(c)) { return vec2f(0.0); }
  let i = cell_index(c) * 2u;
  return vec2f(f32(atomicLoad(&acc[i])), f32(atomicLoad(&acc[i + 1u]))) / FIX;
}
@compute @workgroup_size(4, 4, 4)
fn blur_x(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (!inside(c)) { return; }
  var s = vec2f(0.0);
  for (var k = -2; k <= 2; k++) { s += read_acc(c + vec3i(k, 0, 0)) * kw(k); }
  tmp_a[cell_index(c)] = s;
}
@compute @workgroup_size(4, 4, 4)
fn blur_y(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (!inside(c)) { return; }
  var s = vec2f(0.0);
  for (var k = -2; k <= 2; k++) {
    let n = c + vec3i(0, k, 0);
    if (inside(n)) { s += tmp_a[cell_index(n)] * kw(k); }
  }
  tmp_b[cell_index(c)] = s;
}
@compute @workgroup_size(4, 4, 4)
fn blur_z(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (!inside(c)) { return; }
  var s = vec2f(0.0);
  for (var k = -2; k <= 2; k++) {
    let n = c + vec3i(0, 0, k);
    if (inside(n)) { s += tmp_b[cell_index(n)] * kw(k); }
  }
  textureStore(vol_out, c, vec4f(s, 0.0, 0.0));
}

// Lighting per cell: x = optical depth toward the sun, y = light from the
// burning gas around it, z = optical depth straight up (how much sky it sees),
// w = density.
fn dens(g: vec3f) -> vec2f {
  let uvw = g / vec3f(f32(NX), f32(NY), f32(NZ));
  if (any(uvw < vec3f(0.0)) || any(uvw > vec3f(1.0))) { return vec2f(0.0); }
  return textureSampleLevel(t_vol, s_clamp, uvw, 0.0).rg;
}
// constant tables live in constant memory.
const VOL_DIRS = array<vec3f, 14>(
  vec3f(1.0, 0.0, 0.0), vec3f(-1.0, 0.0, 0.0), vec3f(0.0, 1.0, 0.0), vec3f(0.0, -1.0, 0.0), vec3f(0.0, 0.0, 1.0), vec3f(0.0, 0.0, -1.0),
  vec3f(0.577, 0.577, 0.577), vec3f(-0.577, 0.577, 0.577), vec3f(0.577, -0.577, 0.577), vec3f(-0.577, -0.577, 0.577),
  vec3f(0.577, 0.577, -0.577), vec3f(-0.577, 0.577, -0.577), vec3f(0.577, -0.577, -0.577), vec3f(-0.577, -0.577, -0.577));
const VOL_RS = array<f32, 4>(0.0, 1.0, 2.5, 4.5);
const VOL_DR = array<f32, 4>(0.6, 1.2, 1.7, 2.4);
@compute @workgroup_size(4, 4, 4)
fn light(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (!inside(c)) { return; }
  let cs = S.ve.w;
  let up = S.upv.xyz;
  let g = vec3f(c) + 0.5;
  let here = dens(g);
  let sun = vec3f(dot(G.sun_dir.xyz, S.ve.xyz), dot(G.sun_dir.xyz, S.vn.xyz), dot(G.sun_dir.xyz, up));
  // Toward the sun, in cell-sized steps that lengthen as the ray leaves the core.
  var od = 0.0;
  var t = 0.5;
  var st = 1.0;
  for (var i = 0; i < 28; i++) {
    let p = g + sun * t;
    if (any(p < vec3f(-1.0)) || any(p > vec3f(f32(NX), f32(NY), f32(NZ)) + 1.0)) { break; }
    od += dens(p).x * st * cs;
    t += st;
    st = min(st * 1.12, 3.0);
  }
  var odu = 0.0;
  t = 0.5;
  for (var i = 0; i < 14; i++) {
    let p = g + vec3f(0.0, 0.0, t);
    if (p.z > f32(NZ) + 1.0) { break; }
    odu += dens(p).x * 1.5 * cs;
    t += 1.5;
  }
  // Fire light: average emission seen over a sphere of directions, each ray
  // dimmed by the smoke along it.
  var glow = 0.0;
  for (var k = 0; k < 14; k++) {
    var tr = 1.0;
    for (var j = 0; j < 4; j++) {
      let s = dens(g + VOL_DIRS[k] * VOL_RS[j]);
      let l = VOL_DR[j] * cs;
      glow += s.y * l * tr;
      tr *= exp(-s.x * l * 0.8);
    }
  }
  textureStore(light_out, c, vec4f(od, glow / 14.0, odu, here.x));
}
