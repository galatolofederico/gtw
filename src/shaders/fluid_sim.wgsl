// Fire and smoke simulation on a grid around a crash site: semi-Lagrangian
// advection, fuel that burns into heat, soot and expanding gas, buoyancy,
// vorticity confinement, the ground, buildings and trees as obstacles, and a
// pressure projection (red-black SOR) that keeps the flow incompressible
// apart from the expansion of the burning gas.
struct Src { p: vec4f, v: vec4f, k: vec4f, m: vec4f, n: vec4f };
// p: centre (domain metres), radius. v: velocity, plus outward speed.
// k: fuel, soot, dust per second at the centre; temperature it heats to.
// m: how fast it imposes its velocity (1/s), how fast it heats (1/s),
//    how much its fuel flickers (0..1), volume it adds (1/s).
// n: turbulence of the velocity it imposes (share of the outward speed), seed.

@group(0) @binding(0) var<uniform> U: FU;
@group(0) @binding(1) var<storage, read> srcs: array<Src>;
@group(0) @binding(2) var vel_in: texture_3d<f32>;
@group(0) @binding(3) var scal_in: texture_3d<f32>;
@group(0) @binding(4) var curl_in: texture_3d<f32>;
@group(0) @binding(5) var vel_out: texture_storage_3d<rgba16float, write>;
@group(0) @binding(6) var scal_out: texture_storage_3d<rgba16float, write>;
@group(0) @binding(7) var curl_out: texture_storage_3d<rgba16float, write>;
@group(0) @binding(8) var<storage, read_write> pres: array<f32>;
@group(0) @binding(9) var<storage, read_write> rhs: array<f32>;
@group(0) @binding(10) var<storage, read_write> expand: array<f32>;
// Per column: ground top, deck base, deck top (domain metres; solid below
// the ground and between the deck's base and top).
@group(0) @binding(11) var obst: texture_2d<f32>;
@group(0) @binding(12) var s_lin: sampler;
@group(0) @binding(13) var light_out: texture_storage_3d<rgba16float, write>;
// Per lighting workgroup: emitted light and its centre; soot leaving the top and its centre.
@group(0) @binding(14) var<storage, read_write> stats: array<vec4f>;
// Per column: tree crown bottom, top (domain metres), leaf density.
@group(0) @binding(15) var trees: texture_2d<f32>;
@group(0) @binding(16) var scal_hat: texture_3d<f32>;
// Per cell: which neighbours are fluid (bits 0-5) or open air outside the
// grid (bits 6-11), and bit 12 if the cell itself is solid.
@group(0) @binding(17) var<storage, read_write> flags: array<u32>;
// Per block of 8^3 cells: which sources reach it (128 bits).
@group(0) @binding(18) var<storage, read> bins: array<vec4u>;

// The scene's coarse smoke volume (vol.wgsl): fixed-point extinction per cell.
@group(0) @binding(19) var<storage, read_write> coarse_acc: array<atomic<u32>>;

const BLOCKS = vec3i(i32(FNX / 8u), i32(FNY / 8u), i32(FNZ / 8u));
fn bin_of(c: vec3i) -> vec4u {
  let b = c / 8;
  return bins[(b.z * BLOCKS.y + b.y) * BLOCKS.x + b.x];
}
const NB = array<vec3i, 6>(vec3i(1, 0, 0), vec3i(-1, 0, 0), vec3i(0, 1, 0), vec3i(0, -1, 0), vec3i(0, 0, 1), vec3i(0, 0, -1));

override parity: u32 = 0u;

fn idx(c: vec3i) -> u32 { return (u32(c.z) * FNY + u32(c.y)) * FNX + u32(c.x); }
// Pressure, its right-hand side and the neighbour flags are stored split by
// red-black colour, so a sweep over one colour reads memory contiguously.
const HALF = FNX * FNY * FNZ / 2u;
fn sidx(c: vec3i) -> u32 { return (u32(c.x + c.y + c.z) & 1u) * HALF + (idx(c) >> 1u); }
fn inside(c: vec3i) -> bool { return all(c >= vec3i(0)) && all(c < FN); }
fn solid(c: vec3i) -> bool {
  if (c.z < 0) { return true; }
  let o = textureLoad(obst, clamp(c.xy, vec2i(0), FN.xy - 1), 0);
  let z = (f32(c.z) + 0.5) * U.a.y;
  return z < o.x || (z > o.y && z < o.z);
}
fn ld(t: texture_3d<f32>, c: vec3i) -> vec4f { return textureLoad(t, clamp(c, vec3i(0), FN - 1), 0); }
fn uvw(g: vec3f) -> vec3f { return g / vec3f(FN); }
fn vel_at(g: vec3f) -> vec3f { return textureSampleLevel(vel_in, s_lin, uvw(g), 0.0).xyz; }
// The wind over open ground: a log profile from the site's ground.
fn wind_at(z: f32) -> vec3f {
  let hz = max(z - U.ax_e.w, 0.3);
  return U.wind.xyz * clamp(log(hz / 0.3) / log(10.0 / 0.3), 0.0, 1.35);
}

@compute @workgroup_size(4, 4, 4)
fn clear(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (!inside(c)) { return; }
  textureStore(vel_out, c, vec4f(0.0));
  textureStore(scal_out, c, vec4f(0.0));
  textureStore(curl_out, c, vec4f(0.0));
  pres[sidx(c)] = 0.0;
  expand[idx(c)] = 0.0;
}

// Value noise for disturbance and flicker (hashed lattice, smooth).
fn hash3i(p: vec3i) -> f32 {
  var n = (u32(p.x) * 73856093u) ^ (u32(p.y) * 19349663u) ^ (u32(p.z) * 83492791u);
  n = (n ^ (n >> 13u)) * 1274126177u;
  n = n ^ (n >> 16u);
  return f32(n & 0xffffffu) / 16777215.0;
}
fn vnoise(p: vec3f) -> f32 {
  let i = vec3i(floor(p));
  let f = p - floor(p);
  let w = f * f * (3.0 - 2.0 * f);
  let a = mix(mix(hash3i(i), hash3i(i + vec3i(1, 0, 0)), w.x), mix(hash3i(i + vec3i(0, 1, 0)), hash3i(i + vec3i(1, 1, 0)), w.x), w.y);
  let b = mix(mix(hash3i(i + vec3i(0, 0, 1)), hash3i(i + vec3i(1, 0, 1)), w.x), mix(hash3i(i + vec3i(0, 1, 1)), hash3i(i + vec3i(1, 1, 1)), w.x), w.y);
  return mix(a, b, w.z);
}
fn vnoise3(p: vec3f) -> vec3f {
  return vec3f(vnoise(p), vnoise(p + vec3f(31.4, 17.7, 9.2)), vnoise(p + vec3f(-12.3, 45.1, 27.9))) - 0.5;
}

// Advect velocity (vel_in -> vel_out) and apply buoyancy, confinement,
// disturbance, drag, wind and the sources' push; carry the scalars along
// unchanged (scal_in -> scal_out, the first half of a MacCormack step).
@compute @workgroup_size(4, 4, 4)
fn advect(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (!inside(c)) { return; }
  if (solid(c)) {
    textureStore(vel_out, c, vec4f(0.0));
    textureStore(scal_out, c, vec4f(0.0));
    return;
  }
  let dt = U.a.x;
  let h = U.a.y;
  let g = vec3f(c) + 0.5;
  let u0 = textureLoad(vel_in, c, 0).xyz;
  // Second-order backtrace.
  let um = vel_at(g - 0.5 * u0 * dt / h);
  let gb = g - um * dt / h;
  var u = vel_at(gb);
  let s = max(textureSampleLevel(scal_in, s_lin, uvw(gb), 0.0), vec4f(0.0));
  textureStore(scal_out, c, s);
  let soot = s.x;
  let T = s.y;
  let dust = s.w;

  // Buoyancy of hot gas (g * dT / T), the weight of dust.
  u.z += dt * (U.k1.x * T / (0.29 + T) - U.k3.y * dust);
  // Vorticity confinement puts back the small eddies the grid smears out;
  // stronger in hot, burning gas.
  let w = textureLoad(curl_in, c, 0);
  let gw = vec3f(ld(curl_in, c + vec3i(1, 0, 0)).w - ld(curl_in, c - vec3i(1, 0, 0)).w,
                 ld(curl_in, c + vec3i(0, 1, 0)).w - ld(curl_in, c - vec3i(0, 1, 0)).w,
                 ld(curl_in, c + vec3i(0, 0, 1)).w - ld(curl_in, c - vec3i(0, 0, 1)).w);
  let lg = length(gw);
  let P = g * h;
  let hot = smoothstep(0.0, 0.8, T);
  if (lg > 1e-5) {
    let eps = U.wind.w * (0.35 + 0.65 * hot + 0.5 * smoothstep(0.0, 0.3, soot + dust));
    u += dt * eps * h * cross(gw / lg, w.xyz);
  }
  // Disturbance: small random pushes in hot gas and fresh smoke, the
  // turbulence a real fire has at scales below the grid's.
  if (T + soot > 0.01) {
    let tm = U.a.z;
    let n = vnoise3(P * U.k4.y + vec3f(0.0, 0.0, -tm * 1.3)) + 0.5 * vnoise3(P * U.k4.y * 2.3 + vec3f(tm * 0.7, 0.0, -tm * 2.1));
    u += dt * U.k4.x * n * (hot + 0.25 * smoothstep(0.0, 0.4, soot));
  }
  // Tree crowns are porous: they slow the gas that blows through them.
  let z = P.z;
  let tr = textureLoad(trees, c.xy, 0);
  if (tr.z > 0.0 && z > tr.x && z < tr.y) {
    u *= exp(-dt * U.k3.z * tr.z);
  }
  // The wind: pulled toward it gently everywhere and firmly at the open sides.
  let edge = min(min(min(g.x, f32(FN.x) - g.x), min(g.y, f32(FN.y) - g.y)), f32(FN.z) - g.z);
  let pull = U.k3.w + 3.0 * (1.0 - smoothstep(1.0, 5.0, edge));
  u += (wind_at(z) - u) * (1.0 - exp(-dt * pull));
  // The sources' push.
  let bin = bin_of(c);
  for (var wd = 0u; wd < 4u; wd++) {
   var bits = bin[wd];
   while (bits != 0u) {
    let k = wd * 32u + firstTrailingBit(bits);
    bits &= bits - 1u;
    let sr = srcs[k];
    let d = P - sr.p.xyz;
    let r2 = dot(d, d) / (sr.p.w * sr.p.w);
    if (r2 >= 1.0 || sr.m.x <= 0.0) { continue; }
    let wt = (1.0 - r2) * (1.0 - r2);
    // Outward speed grows linearly to the rim; a blast is ragged.
    var tv = sr.v.xyz + d * (sr.v.w / sr.p.w);
    if (sr.n.x > 0.0) {
      tv += (vnoise3(P * 0.3 + sr.n.y * 17.0) + 0.5 * vnoise3(P * 0.75 - sr.n.y * 11.0)) * (sr.v.w * sr.n.x * 2.0);
    }
    u = mix(u, tv, 1.0 - exp(-dt * sr.m.x * wt));
   }
  }
  // Nothing moves faster than the grid can carry in a step or two.
  let sp = length(u);
  let vmax = 2.5 * h / max(dt, 1e-4);
  if (sp > vmax) { u *= vmax / sp; }
  textureStore(vel_out, c, vec4f(u, 0.0));
}

// Second half of the MacCormack step (scal_in = old, scal_hat = advected ->
// scal_out), then dilution, combustion, cooling and the sources.
@compute @workgroup_size(4, 4, 4)
fn react(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (!inside(c)) { return; }
  let i = idx(c);
  if (solid(c)) {
    textureStore(scal_out, c, vec4f(0.0));
    expand[i] = 0.0;
    return;
  }
  let shat = textureLoad(scal_hat, c, 0);
  let bin = bin_of(c);
  let old = textureLoad(scal_in, c, 0);
  // Empty air with nothing feeding it stays empty.
  if (all(bin == vec4u(0u)) && max(max(shat.x, shat.y), max(shat.z, shat.w)) < 1e-5 && max(max(old.x, old.y), max(old.z, old.w)) < 1e-5) {
    textureStore(scal_out, c, vec4f(0.0));
    expand[i] = 0.0;
    return;
  }
  let dt = U.a.x;
  let h = U.a.y;
  let g = vec3f(c) + 0.5;
  let u0 = textureLoad(vel_in, c, 0).xyz;
  let um = vel_at(g - 0.5 * u0 * dt / h);
  let gb = g - um * dt / h;
  // Trace the advected field forward again; half the round trip's error is
  // the first step's error, so take it back out.
  let gf = g + um * dt / h;
  let sback = textureSampleLevel(scal_hat, s_lin, uvw(gf), 0.0);
  var s = shat + 0.5 * (old - sback);
  // Never outside what the cells around the backtrace held (keeps it stable).
  let b0 = vec3i(floor(gb - 0.5));
  var lo = vec4f(1e9);
  var hi = vec4f(-1e9);
  for (var k = 0; k < 8; k++) {
    let v = ld(scal_in, b0 + vec3i(k & 1, (k >> 1) & 1, (k >> 2) & 1));
    lo = min(lo, v);
    hi = max(hi, v);
  }
  s = clamp(s, lo, hi);
  // Expanding gas spreads what it carries over more volume (advection alone
  // would stretch it and so create soot and fuel from nothing).
  let dil = exp(-expand[i] * dt);
  var soot = max(s.x, 0.0) * dil;
  var T = max(s.y, 0.0);
  var fuel = max(s.z, 0.0) * dil;
  var dust = max(s.w, 0.0) * dil;

  // Cooling: mixing with the air, and radiation (strong only when hot).
  let t2 = T * T;
  T = max(T - dt * (U.k1.y * T + 0.25 * t2 * t2), 0.0);
  soot *= exp(-dt * U.k2.w);
  dust *= exp(-dt * U.k3.x);
  // Combustion: fuel burns where it meets air, so mostly at the skin of a
  // fuel cloud (the flame front moves into the fuel at a turbulent flame
  // speed), slowly throughout where it is hot. Where it burns hard the gas
  // is at flame temperature; once the fuel is spent it cools.
  let ign = U.k2.z;
  if (fuel > 1e-4 && T > ign) {
    let gf3 = vec3f(ld(scal_in, c + vec3i(1, 0, 0)).z - ld(scal_in, c - vec3i(1, 0, 0)).z,
                    ld(scal_in, c + vec3i(0, 1, 0)).z - ld(scal_in, c - vec3i(0, 1, 0)).z,
                    ld(scal_in, c + vec3i(0, 0, 1)).z - ld(scal_in, c - vec3i(0, 0, 1)).z) / (2.0 * h);
    let lit = smoothstep(ign, ign + 0.3, T);
    let rate = lit * (U.k1.z + U.k4.z * length(gf3) / max(fuel, 0.05));
    let burn = fuel * (1.0 - exp(-rate * dt));
    let br = burn / max(dt, 1e-5);
    fuel -= burn;
    soot += burn * U.k2.x;
    expand[i] = br * U.k1.w;
    // Hardest at the skin where the flame front is, cooler in a lazily
    // burning core.
    T = max(T, U.k2.y * sqrt(br / (br + 0.5)));
  } else {
    expand[i] = 0.0;
  }

  // Sources. Fuel comes out in flickering tongues.
  let P = g * h;
  let tm = U.a.z;
  var flick = -1.0;
  for (var wd = 0u; wd < 4u; wd++) {
   var bits = bin[wd];
   while (bits != 0u) {
    let k = wd * 32u + firstTrailingBit(bits);
    bits &= bits - 1u;
    let sr = srcs[k];
    let d = P - sr.p.xyz;
    let r2 = dot(d, d) / (sr.p.w * sr.p.w);
    if (r2 >= 1.0) { continue; }
    let wt = (1.0 - r2) * (1.0 - r2);
    if (flick < 0.0 && sr.m.z > 0.0) {
      flick = vnoise(P * 0.45 + vec3f(0.0, 0.0, -tm * 3.5)) * 0.65 + vnoise(P * 1.1 + vec3f(tm, 0.0, -tm * 6.0)) * 0.35;
    }
    let fk = mix(1.0, smoothstep(0.25, 0.75, max(flick, 0.0)) * 2.0, U.k4.w * sr.m.z);
    fuel += sr.k.x * wt * dt * fk;
    // A blast pushes the air aside: the source adds volume.
    expand[i] += sr.m.w * wt;
    soot += sr.k.y * wt * dt;
    dust += sr.k.z * wt * dt;
    if (sr.k.w > T) { T = mix(T, sr.k.w, 1.0 - exp(-dt * sr.m.y * wt * fk)); }
   }
  }
  T = min(T, 2.6);
  textureStore(scal_out, c, vec4f(soot, T, fuel, dust));
}

// Velocity of a neighbour, for the divergence: walls are still, the open
// sides and top let the flow out as it is.
fn vel_nb(c: vec3i) -> vec3f {
  if (!inside(c)) {
    if (c.z < 0) { return vec3f(0.0); }
    return ld(vel_in, c).xyz;
  }
  if (solid(c)) { return vec3f(0.0); }
  return textureLoad(vel_in, c, 0).xyz;
}

@compute @workgroup_size(4, 4, 4)
fn divergence(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (!inside(c)) { return; }
  let i = idx(c);
  let si = sidx(c);
  if (solid(c)) {
    rhs[si] = 0.0;
    pres[si] = 0.0;
    flags[si] = 1u << 12u;
    return;
  }
  var f = 0u;
  for (var k = 0; k < 6; k++) {
    let q = c + NB[k];
    if (!inside(q)) {
      if (q.z >= 0) { f |= 1u << u32(k + 6); }
    } else if (!solid(q)) {
      f |= 1u << u32(k);
    }
  }
  flags[si] = f;
  let h = U.a.y;
  let div = (vel_nb(c + vec3i(1, 0, 0)).x - vel_nb(c - vec3i(1, 0, 0)).x
           + vel_nb(c + vec3i(0, 1, 0)).y - vel_nb(c - vec3i(0, 1, 0)).y
           + vel_nb(c + vec3i(0, 0, 1)).z - vel_nb(c - vec3i(0, 0, 1)).z) / (2.0 * h);
  rhs[si] = (div - expand[i]) * h * h;
}

// One colour of a red-black SOR sweep, dispatched over half the cells. Open
// sides are at ambient pressure; walls reflect (they drop out of the stencil).
@compute @workgroup_size(16, 4, 1)
fn pressure(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(i32(id.x) * 2 + i32((id.y + id.z + parity) & 1u), i32(id.y), i32(id.z));
  if (!inside(c)) { return; }
  let i = idx(c);
  let si = parity * HALF + (i >> 1u);
  let f = flags[si];
  if ((f & (1u << 12u)) != 0u) { return; }
  let n = f32(countOneBits(f & 0xfffu));
  if (n < 0.5) { return; }
  var sum = 0.0;
  let other = (1u - parity) * HALF;
  let st = array<i32, 6>(1, -1, i32(FNX), -i32(FNX), i32(FNX * FNY), -i32(FNX * FNY));
  for (var k = 0; k < 6; k++) {
    if ((f & (1u << u32(k))) != 0u) { sum += pres[other + (u32(i32(i) + st[k]) >> 1u)]; }
  }
  let pn = (sum - rhs[si]) / n;
  pres[si] = mix(pres[si], pn, U.sun.w);
}

// Pressure of neighbour k of cell i: its own if fluid, ambient (0) if open
// air, this cell's own if a wall.
fn p_nb(i: u32, other: u32, f: u32, k: u32, di: i32, pc: f32) -> f32 {
  if ((f & (1u << k)) != 0u) { return pres[other + (u32(i32(i) + di) >> 1u)]; }
  if ((f & (1u << (k + 6u))) != 0u) { return 0.0; }
  return pc;
}

// Subtract the pressure gradient (vel_in -> vel_out).
@compute @workgroup_size(4, 4, 4)
fn project(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (!inside(c)) { return; }
  let i = idx(c);
  let si = sidx(c);
  let f = flags[si];
  if ((f & (1u << 12u)) != 0u) {
    textureStore(vel_out, c, vec4f(0.0));
    return;
  }
  let h = U.a.y;
  let pc = pres[si];
  let o = (1u - (u32(c.x + c.y + c.z) & 1u)) * HALF;
  let sx = 1;
  let sy = i32(FNX);
  let sz = i32(FNX * FNY);
  let gp = vec3f(p_nb(i, o, f, 0u, sx, pc) - p_nb(i, o, f, 1u, -sx, pc),
                 p_nb(i, o, f, 2u, sy, pc) - p_nb(i, o, f, 3u, -sy, pc),
                 p_nb(i, o, f, 4u, sz, pc) - p_nb(i, o, f, 5u, -sz, pc)) / (2.0 * h);
  let u = textureLoad(vel_in, c, 0).xyz - gp;
  textureStore(vel_out, c, vec4f(u, 0.0));
}

@compute @workgroup_size(4, 4, 4)
fn curl(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (!inside(c)) { return; }
  let h2 = 2.0 * U.a.y;
  let dx = (ld(vel_in, c + vec3i(1, 0, 0)).xyz - ld(vel_in, c - vec3i(1, 0, 0)).xyz) / h2;
  let dy = (ld(vel_in, c + vec3i(0, 1, 0)).xyz - ld(vel_in, c - vec3i(0, 1, 0)).xyz) / h2;
  let dz = (ld(vel_in, c + vec3i(0, 0, 1)).xyz - ld(vel_in, c - vec3i(0, 0, 1)).xyz) / h2;
  let w = vec3f(dy.z - dz.y, dz.x - dx.z, dx.y - dy.x);
  textureStore(curl_out, c, vec4f(w, length(w)));
}

// ---------------------------------------------------------------- lighting
fn sc_at(g: vec3f) -> vec4f { return textureSampleLevel(scal_in, s_lin, uvw(g), 0.0); }
fn sig_at(g: vec3f) -> f32 { return fl_sigma(sc_at(g), U.ext); }
fn emit_at(g: vec3f) -> f32 {
  let s = sc_at(g);
  return fl_emitter(s, U.ext, U.k2.z) * fl_radiance(fl_kelvin(s.y));
}

var<workgroup> red_e: array<vec4f, 64>;
var<workgroup> red_s: array<vec4f, 64>;
var<workgroup> red_t: array<f32, 64>;

// Per lighting cell: x = optical depth toward the sun, y = light from the
// burning gas around it (relative units), z = optical depth straight up,
// w = whether there is anything here or next to it (for skipping empty space).
// constant tables live in constant memory.
const FSIM_DIRS = array<vec3f, 14>(
  vec3f(1.0, 0.0, 0.0), vec3f(-1.0, 0.0, 0.0), vec3f(0.0, 1.0, 0.0), vec3f(0.0, -1.0, 0.0), vec3f(0.0, 0.0, 1.0), vec3f(0.0, 0.0, -1.0),
  vec3f(0.577, 0.577, 0.577), vec3f(-0.577, 0.577, 0.577), vec3f(0.577, -0.577, 0.577), vec3f(-0.577, -0.577, 0.577),
  vec3f(0.577, 0.577, -0.577), vec3f(-0.577, 0.577, -0.577), vec3f(0.577, -0.577, -0.577), vec3f(-0.577, -0.577, -0.577));
const FSIM_RS = array<f32, 4>(1.5, 4.5, 9.5, 17.0);
const FSIM_DR = array<f32, 4>(3.0, 4.0, 6.0, 9.0);
@compute @workgroup_size(4, 4, 4)
fn light(@builtin(global_invocation_id) id: vec3u, @builtin(local_invocation_index) li: u32, @builtin(workgroup_id) wg: vec3u) {
  let c = vec3i(id);
  let h = U.a.y;
  let g = (vec3f(c) + 0.5) * 2.0;
  let here = sc_at(g);
  // Occupancy, dilated by a cell so the march never skips an edge (and the
  // lighting is computed wherever the renderer may sample it).
  var occ = fl_sigma(here, U.ext) + here.y;
  for (var k = 0; k < 8; k++) {
    let o = vec3f(f32(k & 1), f32((k >> 1) & 1), f32((k >> 2) & 1)) * 4.0 - 2.0;
    let s = sc_at(g + o);
    occ = max(occ, fl_sigma(s, U.ext) + s.y);
  }
  let busy = occ > 0.002;
  let sun = U.sun.xyz;
  var od = 0.0;
  var t = 1.0;
  var st = 1.0;
  var odu = 0.0;
  var glow = 0.0;
  if (busy) {
  for (var i = 0; i < 26; i++) {
    let p = g + sun * t;
    if (any(p < vec3f(-1.0)) || any(p > vec3f(FN) + 1.0)) { break; }
    od += sig_at(p) * st * h;
    t += st;
    st = min(st * 1.18, 5.0);
  }
  t = 1.0;
  for (var i = 0; i < 14; i++) {
    let p = g + vec3f(0.0, 0.0, t);
    if (p.z > f32(FN.z)) { break; }
    odu += sig_at(p) * 3.5 * h;
    t += 3.5;
  }
  // Fire light: emission seen over a sphere of directions, each ray dimmed
  // by the smoke along it.
  for (var k = 0; k < 14; k++) {
    var tr = 1.0;
    for (var j = 0; j < 4; j++) {
      let p = g + FSIM_DIRS[k] * FSIM_RS[j];
      let s = sc_at(p);
      let l = FSIM_DR[j] * h;
      glow += fl_emitter(s, U.ext, U.k2.z) * fl_radiance(fl_kelvin(s.y)) * l * tr;
      tr *= exp(-fl_sigma(s, U.ext) * l * 0.8);
    }
  }
  }
  if (all(c < LN)) {
    textureStore(light_out, c, vec4f(od, glow / 14.0, odu, select(0.0, 1.0, busy)));
  }

  // Totals for the CPU: light given off (for lighting the scene), soot
  // leaving through the open sides and top (where the column carries on as
  // particles), and all the soot there is.
  let vol = 8.0 * h * h * h;
  var e = 0.0;
  var sf = 0.0;
  var tot = 0.0;
  if (busy && all(c < LN)) {
    e = emit_at(g) * vol;
    tot = max(here.x, 0.0) * vol;
    let v = vel_at(g);
    var out = 0.0;
    if (c.x == 0) { out += max(-v.x, 0.0); }
    if (c.x == LN.x - 1) { out += max(v.x, 0.0); }
    if (c.y == 0) { out += max(-v.y, 0.0); }
    if (c.y == LN.y - 1) { out += max(v.y, 0.0); }
    if (c.z == LN.z - 1) { out += max(v.z, 0.0); }
    sf = max(here.x, 0.0) * out * 4.0 * h * h;
  }
  let q = (vec3f(c) + 0.5) / vec3f(LN);
  red_e[li] = vec4f(e, e * q);
  red_s[li] = vec4f(sf, sf * q);
  red_t[li] = tot;
  workgroupBarrier();
  for (var s = 32u; s > 0u; s >>= 1u) {
    if (li < s) {
      red_e[li] += red_e[li + s];
      red_s[li] += red_s[li + s];
      red_t[li] += red_t[li + s];
    }
    workgroupBarrier();
  }
  if (li == 0u) {
    let n = (wg.z * u32(LN.y / 4) + wg.y) * u32(LN.x / 4) + wg.x;
    stats[n * 3u] = red_e[0];
    stats[n * 3u + 1u] = red_s[0];
    stats[n * 3u + 2u] = vec4f(red_t[0], 0.0, 0.0, 0.0);
  }
}

// Add the simulation's smoke to the scene's coarse smoke volume, which
// shadows the ground and everything else under the plume.
@compute @workgroup_size(4, 4, 4)
fn coarse(@builtin(global_invocation_id) id: vec3u) {
  let c = vec3i(id);
  if (any(c >= vec3i(i32(VOLNX), i32(VOLNY), i32(VOLNZ)))) { return; }
  let cs = U.coarse.w;
  let lo = U.coarse.xyz + vec3f(c) * cs;
  let size = vec3f(FN) * U.a.y;
  if (any(lo + cs < vec3f(0.0)) || any(lo > size)) { return; }
  var sum = 0.0;
  for (var k = 0; k < 8; k++) {
    let p = lo + (vec3f(f32(k & 1), f32((k >> 1) & 1), f32((k >> 2) & 1)) * 0.5 + 0.25) * cs;
    if (all(p > vec3f(0.0)) && all(p < size)) {
      sum += fl_sigma(textureSampleLevel(scal_in, s_lin, p / size, 0.0), U.ext);
    }
  }
  let sig = sum / 8.0;
  if (sig > 1e-5) {
    let i = ((u32(c.z) * VOLNY + u32(c.y)) * VOLNX + u32(c.x)) * 2u;
    atomicAdd(&coarse_acc[i], u32(min(sig, 50.0) * 4096.0 + 0.5));
  }
}
