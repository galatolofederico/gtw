// Frustum-aligned atmosphere and cloud-reflection LUTs, rebuilt once per frame
// in compute so the full-resolution deferred composite (composite.wgsl) replaces
// per-pixel 12-24 step atmosphere marches and 3x volumetric cloud-density
// evaluations with single hardware-filtered texture lookups.

@group(1) @binding(0) var t_ap_l_out: texture_storage_3d<rgba16float, write>;
@group(1) @binding(1) var t_ap_t_out: texture_storage_3d<rgba16float, write>;
@group(1) @binding(2) var t_sky_out: texture_storage_2d_array<rgba16float, write>;
@group(1) @binding(3) var t_crefl_out: texture_storage_2d<rgba16float, write>;

const AP_W: u32 = 96u;
const AP_H: u32 = 54u;
const AP_D: u32 = 32u;

const SKY_W: u32 = 256u;
const SKY_H: u32 = 144u;

const CREFL_N: u32 = 384u;
const CREFL_S: f32 = 20000.0;

// 3D frustum aerial perspective volume: one thread per (x, y) frustum ray marches
// front-to-back through 31 quadratic depth slices, accumulating both atmospheric
// scattering (with cloud shadows) and night city skyglow into t_ap_l_out, and
// RGB transmittance into t_ap_t_out.
@compute @workgroup_size(8, 8, 1)
fn ap_build(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= AP_W || id.y >= AP_H) { return; }
  let uv = (vec2f(f32(id.x), f32(id.y)) + 0.5) / vec2f(f32(AP_W), f32(AP_H));
  let rd = normalize(view_ray(uv));
  let oc = -G.planet.xyz;
  let rg = G.planet.w;
  let top = ray_sphere(oc, rd, rg + ATM_H);
  let tmax_atm = max(top.y, 0.0);
  let ap_max = max(G.lite.z, 100000.0);

  let sun = G.sun_dir.xyz;
  let c = dot(rd, sun);
  let pr = phase_r(c);
  let pm = phase_m(c, 0.8);
  let oc_sun = dot(oc, sun);
  let moon = G.moon.xyz;
  let cm = dot(rd, moon);
  let prm = phase_r(cm);
  let pmm = phase_m(cm, 0.8);
  let oc_moon = dot(oc, moon);
  let mk = MOON_TINT * (G.moon.w / G.sun_dir.w);
  let moon_on = G.moon.w > 0.0;
  let sdark = G.stormfx.w;
  let cs_on = G.cs[4].w >= 0.5 && G.start.x <= 0.0;
  let cs_top = G.cs[3].y;

  let glow = glow_on();
  let gnd_h = G.glow_e.w;
  var g_t0 = 1e9;
  var g_t1 = 0.0;
  if (glow) {
    let top_g = ray_sphere(oc, rd, rg + gnd_h + 7000.0);
    if (top_g.y > 0.0) {
      g_t0 = max(top_g.x, 0.0);
      g_t1 = min(top_g.y, G.glow_c.w * 1.2);
    }
  }

  var T = vec3f(1.0);
  var L = vec3f(0.0);
  var Tg = vec3f(1.0);
  var Lg = vec3f(0.0);
  textureStore(t_ap_l_out, vec3u(id.xy, 0u), vec4f(0.0, 0.0, 0.0, 1.0));
  textureStore(t_ap_t_out, vec3u(id.xy, 0u), vec4f(1.0, 1.0, 1.0, 1.0));

  var tp = 0.0;
  for (var k = 1u; k < AP_D; k++) {
    let fk = f32(k) / f32(AP_D - 1u);
    let tn = ap_max * fk * fk;
    let ta_end = min(tn, tmax_atm);
    let ta_start = min(tp, tmax_atm);
    if (ta_end > ta_start) {
      let dt = ta_end - ta_start;
      let tm = 0.5 * (ta_start + ta_end);
      let p = oc + rd * tm;
      let r = length(p);
      let h = r - rg;
      let mu = (oc_sun + c * tm) / r;
      let d = atm_density(h);
      let ext = atm_ext(d);
      let sr = RAYLEIGH * d.x;
      let sm = MIE_S * d.y;
      var vis = 1.0;
      var msl = ms_lut(h, mu);
      if (cs_on && h < cs_top) {
        let cl = cloud_sun_at(p, r, h, false);
        msl = mix(msl, vec3f(luma(msl)), cl.closed * 0.75) * (1.0 - 0.45 * cl.closed);
        vis = cl.vis;
      }
      if (sdark > 0.0) {
        let kk = 1.0 - 0.8 * sdark * exp(-tm / 15000.0);
        msl *= kk;
        vis *= kk;
      }
      var S = sun_trans(h, mu) * vis * (sr * pr + vec3f(sm * pm)) + msl * (sr + vec3f(sm));
      if (moon_on) {
        let mum = (oc_moon + cm * tm) / r;
        S += (sun_trans(h, mum) * (sr * prm + vec3f(sm * pmm)) + ms_lut(h, mum) * (sr + vec3f(sm))) * mk;
      }
      let st = exp(-ext * dt);
      L += T * S * (vec3f(1.0) - st) / max(ext, vec3f(1e-12));
      T *= st;
    }
    if (glow && g_t1 > g_t0) {
      let ga = max(tp, g_t0);
      let gb = min(tn, g_t1);
      if (gb > ga) {
        let gdt = gb - ga;
        let tm = 0.5 * (ga + gb);
        let p = oc + rd * tm;
        let r = length(p);
        let upn = p / r;
        let h = r - rg;
        let hag = h - gnd_h;
        let dm = exp(-max(hag, 0.0) / 1600.0) * G.weather.w;
        let sr = RAYLEIGH * exp(-max(h, 0.0) / 8000.0);
        let sm = MIE_S * dm;
        let ext = sr + vec3f((MIE_S + MIE_A) * dm);
        let cu = dot(upn, -rd);
        let E = city_up(p + G.planet.xyz, hag);
        let S = E * (sr * phase_r(cu) * 0.35 + vec3f(sm * URBAN_HAZE * mix(phase_m(cu, 0.35), 0.25 / PI, 0.5)));
        let st = exp(-ext * gdt);
        Lg += Tg * S * (vec3f(1.0) - st) / max(ext, vec3f(1e-12));
        Tg *= st;
      }
    }
    textureStore(t_ap_l_out, vec3u(id.xy, k), vec4f(L * G.sun_dir.w * 1.35 + Lg * GLOW_GAIN, 1.0));
    textureStore(t_ap_t_out, vec3u(id.xy, k), vec4f(T, 1.0));
    tp = tn;
  }
}

// 2D frustum sky scattering table (layer 0 = in-scattered L + city glow, layer 1 = transmittance T).
// Rays well below the mathematical planet horizon are never sampled by above-horizon sky pixels and
// exit early; a narrow band just below the horizon is lifted onto the horizon tangent so bilinear
// filtering right above the limb never pulls in stale texels.
@compute @workgroup_size(8, 8, 1)
fn sky_build(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= SKY_W || id.y >= SKY_H) { return; }
  let uv = (vec2f(f32(id.x), f32(id.y)) + 0.5) / vec2f(f32(SKY_W), f32(SKY_H));
  let oc = -G.planet.xyz;
  let rg = G.planet.w;
  var rd = normalize(view_ray(uv));
  let r0 = length(oc);
  let up0 = oc / r0;
  let h0 = max(r0 - rg, 0.0);
  let mu_h = -sqrt(h0 * (2.0 * rg + h0)) / r0;
  let el = dot(rd, up0);
  if (el < mu_h - 0.04) { return; }
  if (el < mu_h + 1e-4) {
    let horiz = rd - up0 * el;
    let hl = length(horiz);
    if (hl > 1e-6) {
      let ec = mu_h + 1e-4;
      rd = horiz * (sqrt(max(1.0 - ec * ec, 0.0)) / hl) + up0 * ec;
    }
  }
  let top = ray_sphere(oc, rd, rg + ATM_H);
  var sc = Scatter(vec3f(0.0), vec3f(1.0));
  if (top.y > 0.0) {
    sc = atm_march(oc, rd, max(top.x, 0.0), top.y, 24, 0.5);
  }
  let gl = glow_march(rd, 1e9, 12, 0.5);
  textureStore(t_sky_out, id.xy, 0, vec4f(sc.L + gl, 1.0));
  textureStore(t_sky_out, id.xy, 1, vec4f(sc.T, 1.0));
}

// Fast lighting for reflected clouds at altitude mid: matches light_at_nv using a
// single bilinear cloud-shadow tap instead of a 4-tap cubic B-spline.
fn crefl_light(pp: vec3f) -> vec3f {
  let r = length(pp);
  let upn = pp / r;
  let h = r - G.planet.w;
  let mu = dot(upn, G.sun_dir.xyz);
  var sun_s = sun_trans(h, mu) * G.sun_dir.w * cloud_sun_at(pp, r, h, false).vis;
  let day = min(smoothstep(-0.12, 0.3, mu), 0.06 * exp(20.0 * mu));
  let dusk = smoothstep(-0.2, 0.0, mu) * (1.0 - smoothstep(0.0, 0.3, mu));
  var skyc = mix(vec3f(0.30, 0.43, 0.70), vec3f(0.47, 0.49, 0.52), clamp(G.cloud.x, 0.0, 1.0) * 0.7);
  skyc = mix(skyc, vec3f(0.38, 0.36, 0.62), dusk * 0.6);
  var amb = skyc * 0.11 * day * G.sun_dir.w;
  let mum = dot(upn, G.moon.xyz);
  let night = 1.0 - smoothstep(-0.12, 0.02, mu);
  amb += vec3f(0.30, 0.45, 0.85) * 0.16 * smoothstep(-0.1, 0.35, mum) * G.moon.w * mix(1.0, 0.7, G.cloud.x);
  amb += vec3f(0.0016, 0.0024, 0.0048) * night;
  var cup = vec3f(0.0);
  let P = pp + G.planet.xyz;
  if (glow_on()) { cup = city_up(P, 600.0) * (GLOW_GAIN / PI); }
  amb += cup * (0.012 + 0.05 * clamp(G.cloud.x, 0.0, 1.0));
  if (G.nimbo[0].w > 0.0) {
    let nd = nimbo_under(pp);
    amb = mix(amb, nimbo_light(upn, h) + cup * 0.062, nd);
  }
  if (G.stormfx.x > 0.0) {
    let dark = storm_shade(pp);
    amb = storm_light(amb, dark);
    sun_s *= 1.0 - dark * dark;
  }
  if (G.stormfx.z > 0.0) { amb += flash_light(P); }
  return sun_s * 0.05 + amb * 1.3;
}

fn crefl_store(id: vec2u, pp: vec3f, cd: f32) {
  let cov = 1.0 - exp(-cd * 7.0);
  var col = vec3f(0.0);
  if (cov > 0.001) {
    col = crefl_light(pp) * cov;
  }
  textureStore(t_crefl_out, id, vec4f(col, cov));
}

@compute @workgroup_size(8, 8, 1)
fn cloud_refl_plain_build(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= CREFL_N || id.y >= CREFL_N) { return; }
  let q = ((vec2f(f32(id.x), f32(id.y)) + 0.5) / f32(CREFL_N)) * 2.0 - 1.0;
  let ql = length(q);
  if (ql >= 0.96) {
    textureStore(t_crefl_out, id.xy, vec4f(0.0));
    return;
  }
  let h2 = q * (CREFL_S / (1.0 - ql));
  let mid = G.cloud.y + 400.0;
  let pp = normalize(-G.planet.xyz + G.east.xyz * h2.x + G.north.xyz * h2.y) * (G.planet.w + mid);
  let hs = mid - G.cloud.y;
  var cd = 0.0;
  if (hs > 0.0 && hs < G.cloud.z) {
    let u = unpack4x8unorm(cloud_ord_pack(pp, hs));
    let ow = vec4f(clamp(u.x * 2.0 - 0.5, 0.0, 1.0), u.y, u.z, u.w);
    cd = cloud_shape_plain(pp, mid, ow, false, 0.0);
  }
  crefl_store(id.xy, pp, cd);
}

@compute @workgroup_size(8, 8, 1)
fn cloud_refl_build(@builtin(global_invocation_id) id: vec3u) {
  if (id.x >= CREFL_N || id.y >= CREFL_N) { return; }
  let q = ((vec2f(f32(id.x), f32(id.y)) + 0.5) / f32(CREFL_N)) * 2.0 - 1.0;
  let ql = length(q);
  if (ql >= 0.96) {
    textureStore(t_crefl_out, id.xy, vec4f(0.0));
    return;
  }
  let h2 = q * (CREFL_S / (1.0 - ql));
  let mid = G.cloud.y + 400.0;
  let pp = normalize(-G.planet.xyz + G.east.xyz * h2.x + G.north.xyz * h2.y) * (G.planet.w + mid);
  let cd = cloud_density(pp, mid, true);
  crefl_store(id.xy, pp, cd);
}
