// Volumetric cloud layer: raymarched at half resolution with a fresh jitter every
// frame, accumulated over frames by reprojection, then upsampled onto the scene.
@group(1) @binding(0) var t_depth: texture_depth_2d;
@group(1) @binding(6) var t_cls_data: texture_2d<u32>;

const SIGMA = 0.05;   // extinction per unit density, 1/m
const MS_GAIN = 2.6;  // energy the octave sum misses in thick cloud

fn hg(c: f32, g: f32) -> f32 {
  let d = 1.0 + g * g - 2.0 * g * c;
  return (1.0 - g * g) / (4.0 * PI * d * sqrt(d));
}
// Droplet (Mie) phase as three lobes: a sharp forward peak, a broad forward lobe that
// carries the silver lining out to ~45 degrees from the sun, and a weak back lobe.
// k < 1 flattens it for light that has already scattered a few times.
// A narrow fourth lobe stands in for the diffraction peak (a big share of a droplet's
// scattering stays within a few degrees of the sun): thin cloud over the sun blazes.
fn cloud_phase(c: f32, k: f32) -> f32 {
  return hg(c, 0.86 * k) * 0.42 + hg(c, 0.5 * k) * 0.3 + hg(c, -0.25 * k) * 0.2 + hg(c, 0.97 * k) * 0.08;
}
// Light that has diffused right through a cloud still mostly travels away from the
// sun, so a cloud in front of a low sun glows through its body instead of going grey.
// Diffuse transmission falls off slowly with depth, ~1/(1 + 0.75 (1 - g) od) for g ~ 0.85.
// Thin parts let more through than thick ones (second term), so the glow keeps the
// cloud's structure. The SILVER term is light scattered forward a few times near the
// thin edges; the outline itself gets the RIM term after the march (see fs).
const GLOW = 0.06;
const SILVER = 0.5;
const RIM = 1.5;
fn cloud_glow(cs: f32, od: f32) -> f32 {
  return GLOW * hg(cs, 0.85) * (1.0 / (1.0 + 0.3 * od) + exp(-0.3 * od))
        + SILVER * hg(cs, 0.9) * exp(-0.35 * od);
}

// ---------------------------------------------------------- mid-level cloud
// A thin layer at G.alto.y: altocumulus (rafts of small cloudlets, lined up in rows
// across the wind) that thickens into a grey altostratus sheet as the amount grows.
// Each cloudlet is a lens reaching from ALTO_BOT below the layer height to ALTO_TOP
// above it where it is thickest (render.rs ALTO_TOP).
const ALTO_TOP = 700.0;
const ALTO_BOT = 300.0;
// How much mid-level cloud this region has (0..1): the slider, in patches ~100 km across.
fn alto_amount(p: vec3f) -> f32 {
  let q = p + G.weather.xyz * 1.5;
  let m = textureSampleLevel(t_noise, s_repeat, q / 95000.0 + vec3f(0.27, 0.64, 0.11), 0.0);
  return clamp(G.alto.x * 1.5 - 0.3 + (m.r - 0.5) * 1.5 + (m.g - 0.6) * 0.7, 0.0, 1.0);
}
// The lens at p: x bottom, y top (metres from the layer height), z density inside;
// w how far it has thickened into altostratus. far = 1 when the cloudlets are
// smaller than a pixel (their mean is used instead).
fn alto_lens(p: vec3f, a: f32, far: f32) -> vec4f {
  let q = p + G.weather.xyz * 1.5;
  let st = smoothstep(0.6, 1.0, a);
  // Squeezed along the wind, so the cloudlets line up in rows across it.
  let w = G.storm.xyz;
  let qs = q + w * (dot(q, w) * 0.6);
  let big = textureSampleLevel(t_noise, s_repeat, qs / 24000.0 + vec3f(0.53, 0.18, 0.77), 0.0);
  let el = textureSampleLevel(t_noise, s_repeat, qs / 6500.0 + vec3f(0.12, 0.91, 0.38), 0.0);
  let cl = mix(el.b * 0.7 + el.a * 0.3, 0.6, far);
  let raft = big.g * 0.7 + big.r * 0.3;
  let v = mix(cl * 0.65 + raft * 0.45, 0.45 + raft * 0.4 + big.b * 0.15, st);
  let thr = 1.02 - a * 0.75;
  // Altostratus is not uniform either: thicker and thinner stretches, darker and
  // paler from below.
  let d = smoothstep(thr, thr + mix(0.12, 0.35, st), v) * mix(1.0, 0.35 + 0.9 * smoothstep(0.45, 0.85, raft * 0.75 + cl * 0.35), st);
  return vec4f(-ALTO_BOT * d * mix(0.5, 1.0, st), ALTO_TOP * d * mix(0.55, 1.0, st), mix(0.55, 0.3, st), st);
}
// Where a ray (from oc) is inside the layer's slab: x entry, y exit, x < 0 = never.
fn alto_range(oc: vec3f, rd: vec3f) -> vec2f {
  let rg = G.planet.w;
  let ah = G.alto.y;
  let s0 = ray_sphere(oc, rd, rg + ah - ALTO_BOT);
  let s1 = ray_sphere(oc, rd, rg + ah + ALTO_TOP);
  let alt = G.up.w;
  var t1 = s1.y;
  if (s0.x > 0.0) { t1 = s0.x; }
  if (alt < ah - ALTO_BOT) { return vec2f(s0.y, s1.y); }
  if (alt < ah + ALTO_TOP) { return vec2f(0.0, t1); }
  if (s1.x <= 0.0) { return vec2f(-1.0); }
  return vec2f(s1.x, t1);
}
// Premultiplied colour and transmittance of the layer between t0 and t1, lit as the
// cumulus are (same phase function and multiple-scattering octaves).
// The towns' light on a cloud at p (planet-relative), from below: the clouds
// over a city glow orange, brightest underneath.
fn city_amb(p: vec3f) -> vec3f {
  if (!glow_on()) { return vec3f(0.0); }
  // Light scattered many times inside the cloud comes out paler than it went in.
  let c = city_up(p + G.planet.xyz, length(p) - G.planet.w - G.glow_e.w);
  return mix(c, vec3f(luma(c)), 0.3) * (0.75 / PI);
}
fn alto_march(oc: vec3f, rd: vec3f, t0: f32, t1: f32, a: f32, jit: f32, cs: f32, sky_amb: vec3f, gnd_amb: vec3f) -> vec4f {
  var T = 1.0;
  var L = vec3f(0.0);
  let n = 6;
  let dt = (t1 - t0) / f32(n);
  let rg = G.planet.w;
  let ah = G.alto.y;
  // By the ground a pixel stretches along the ray takes up (distance over
  // the grazing angle), not distance alone.
  let tm = 0.5 * (t0 + t1);
  let pm = oc + rd * tm;
  let upm = normalize(pm);
  let mu_v = abs(dot(rd, upm));
  let far = smoothstep(180000.0, 2200000.0, tm / max(mu_v, 0.01));
  let sun = G.sun_dir.xyz;
  // Hoist sunlight/moonlight across the thin slab, shadowing only from clouds
  // strictly above the alto layer (storm anvils in cshadow band 2) so alto
  // puffs never self-shadow against their own shadow-map band.
  let mus = dot(upm, sun);
  var sun_v = 1.0;
  if (G.storm.w > 0.0) {
    let f_alto = sqrt(clamp((ah - G.cs[3].x) / max(G.cs[3].y - G.cs[3].x, 1.0), 0.0, 1.0)) * 3.0;
    if (f_alto < 1.98) {
      let h_hi = max(ah + ALTO_TOP + 200.0, mix(G.cs[3].x, G.cs[3].y, 0.455));
      sun_v = cloud_sun(pm + sun * (max(h_hi - ah, 0.0) / max(mus, 0.12)), false).vis;
    }
  }
  let light_m = sun_trans(ah, mus) * G.sun_dir.w * sun_v + moon_light(upm, ah);
  let inv_mu = 1.0 / max(mus, 0.08);
  for (var i = 0; i < n; i++) {
    let ta_i = t0 + dt * f32(i);
    let tb_i = ta_i + dt;
    let z0 = length(oc + rd * ta_i) - rg - ah;
    let z1 = length(oc + rd * tb_i) - rg - ah;
    let zlo = min(z0, z1);
    let zhi = max(z0, z1);
    let span = zhi - zlo;
    let p = oc + rd * (ta_i + dt * jit);
    let ln = alto_lens(p, a, far);
    let thick = ln.y - ln.x;
    if (thick <= 1.0) { continue; }
    let ov_lo = max(zlo, ln.x);
    let ov_hi = min(zhi, ln.y);
    if (span > 10.0 && ov_hi <= ov_lo) { continue; }
    let z = select(mix(z0, z1, jit), mix(ov_lo, ov_hi, jit), span > 10.0);
    let s = smoothstep(ln.x, ln.x + 50.0, z) * smoothstep(ln.y, ln.y - 70.0, z);
    let frac = select(1.0, clamp((ov_hi - ov_lo) / span, 0.0, 1.0), span > 10.0);
    let rho = ln.z * s * frac;
    if (rho <= 0.001) { continue; }
    let od = SIGMA * (ln.z * s) * max(ln.y - z, 0.0) * inv_mu;
    var ms = 0.0;
    var ka = 1.0;
    var kb = 1.0;
    var kc = 1.0;
    for (var k = 0; k < 4; k++) {
      ms += ka * exp(-od * kb) * cloud_phase(cs, kc);
      ka *= 0.55;
      kb *= 0.38;
      kc *= 0.5;
    }
    ms += 0.4 / (4.0 * PI) / (1.0 + 0.25 * od);
    ms += cloud_glow(cs, od);
    let hn = clamp((z - ln.x) / thick, 0.0, 1.0);
    let amb = sky_amb * mix(0.35, 1.0, hn) + gnd_amb * mix(1.0, 0.3, hn);
    let e = rho * SIGMA;
    let S = (light_m * ms * MS_GAIN + amb) * e;
    let st = exp(-e * dt);
    L += T * S * (1.0 - st) / e;
    T *= st;
  }
  return vec4f(L, T);
}

// --------------------------------------------------------------- lightning
// A flash lights the cloud around its channel from inside: the light diffuses
// through a few km of cloud, blinding white close to the channel, a glow
// further out (on top of the plain falloff that lights the base from below).
fn flash_cloud(P: vec3f) -> vec3f {
  let n = u32(G.stormfx.z);
  var e = 0.0;
  for (var i = 0u; i < n; i++) {
    let a = G.flash[2u * i];
    let d = seg_dist(P, a.xyz, G.flash[2u * i + 1u].xyz);
    e += a.w * FLASH_C * (1.0 + 6.0 * exp(-d / 2500.0)) / (d * d + 1000.0 * 1000.0);
  }
  return FLASH_COL * e;
}

// ------------------------------------------------------------- rain shafts
// Under a storm or a shower the rain hangs from the base to the ground as a grey
// curtain, downwind of the updraft, streaked and thickest in the middle. Each
// flagged cell's shaft is a vertical cylinder; the ray's stretch through it (under
// the base, before the scene) is integrated in a few samples. Returns premultiplied
// colour and transmittance; z of dist = where along the ray it mostly is.
struct Rain { L: vec3f, T: f32, t: f32 };
fn seed_of(sp: vec4f) -> f32 { return sp.w - floor(sp.w); }
// How far back from a squall line's shelf-cloud nose a point p is (storm_sd's u,
// m; cen its rain's centre, X along it, Y downwind).
fn shelf_u(p: vec3f, cen: vec3f, X: vec3f, Y: vec3f, R: f32, sp: vec4f) -> f32 {
  let ph = p - cen;
  let cxl = dot(ph, X);
  return R * (SHELF_F + 0.12 * sin(cxl / 9000.0 + seed_of(sp) * 40.0) + 0.07 * sin(cxl / 3700.0 + seed_of(sp) * 17.0)) - dot(ph, Y) - 0.45 * R;
}
// A squall line's rain u metres back from its leading edge (line radius R):
// x how much (none under the shelf, heavy right behind it, then a long tail of
// lighter rain easing off), y how much of it is the heavy curtain. Must match
// storm.rs line_rain.
fn line_rain(u: f32, R: f32) -> vec2f {
  let ws = SHELF_W * R;
  let heavy = 1.0 - smoothstep(ws + 0.45 * R, ws + 0.95 * R, u);
  let tail = 0.35 * (1.0 - smoothstep(ws + 0.95 * R, ws + 3.2 * R, u));
  let on = smoothstep(0.7 * ws, 1.1 * ws, u);
  return vec2f(on * mix(tail, 1.0, heavy), on * heavy);
}
fn storm_rain(oc: vec3f, rd: vec3f, tmax: f32, mask: u32, sb: vec2f, jit: f32, sky_amb: vec3f, gnd_amb: vec3f) -> Rain {
  var o = Rain(vec3f(0.0), 1.0, 0.0);
  let base = G.cloud.y;
  // Rays that stay above the cloud base never meet a rain shaft.
  if (G.up.w >= base && sb.x <= 0.0) { return o; }
  let n = u32(G.storm.w);
  let rg = G.planet.w;
  var wsum = 0.0;
  var rem = mask & select((1u << n) - 1u, 0xffffffffu, n >= 32u);
  while (rem != 0u) {
    let i = firstTrailingBit(rem);
    rem &= rem - 1u;
    let c = G.storm_cells[2u * i];
    let sp = G.storm_cells[2u * i + 1u];
    let R = c.w * 1000.0;
    // Rain once the tower is a few km deep: a shower's light to moderate, an
    // ordinary storm's heavy, a big mature one's the heaviest (storm.rs
    // shaft_amount).
    let htop = (sp.y - base) * storm_top(sp.x);
    let amt = smoothstep(1800.0, 3500.0, htop) * smoothstep(800.0, 2000.0, R) * (0.35 + 0.4 * smoothstep(0.45, 0.8, sp.x) + 0.25 * smoothstep(3000.0, 9000.0, R));
    if (amt <= 0.0) { continue; }
    let young = 1.0 - smoothstep(0.4, 0.6, sp.x);
    let up = normalize(c.xyz - G.weather.xyz * 0.001);
    // A squall line (its half-length L2; storm_sd): a curtain of heavy rain all
    // along it right behind the shelf cloud, then lighter rain trailing far
    // behind (storm.rs line_rain). X along it, Y downwind.
    let L2 = floor(sp.w) * 500.0;
    let line = L2 > 0.0;
    let X = normalize(cross(up, G.storm.xyz));
    let Y = cross(X, up);
    // A supercell's core (rain and hail) falls off to the downwind left of
    // its updraft, a dense, dark shaft with a sharp edge, leaving the base
    // over the updraft and its wall cloud rain-free (storm.rs shaft_at).
    let sup = storm_super(sp);
    let cen = up * rg + G.storm.xyz * (R * mix(0.45, 0.32, sup)) + X * (R * 0.28 * sup);
    let rr = R * mix(0.8, 0.42, sup);
    // (The test cylinder also holds the shaft's slant and its foot.)
    let rt = rr * 1.75 + base * 0.15;
    let v = cen - oc;
    let vh = v - up * dot(v, up);
    let rh = rd - up * dot(rd, up);
    let a = dot(rh, rh);
    let b = dot(vh, rh);
    let cc = dot(vh, vh) - rt * rt;
    var t0 = 0.0;
    var t1 = tmax;
    if (line) {
      // The box holding all of it: along the line, and from the gust front back
      // to the end of the trailing rain (centred 0.95 R upwind of cen).
      let o2 = vec2f(dot(-vh, X), dot(-vh, Y) + 1.4 * R);
      let d2 = vec2f(dot(rh, X), dot(rh, Y));
      let hx = vec2f(L2 + 1.2 * R, 2.6 * R + base * 0.15);
      let inv = 1.0 / select(d2, vec2f(1e-9), abs(d2) < vec2f(1e-9));
      let ta = (-hx - o2) * inv;
      let tb = (hx - o2) * inv;
      t0 = max(max(min(ta.x, tb.x), min(ta.y, tb.y)), 0.0);
      t1 = min(min(max(ta.x, tb.x), max(ta.y, tb.y)), tmax);
    } else if (a > 1e-7) {
      let disc = b * b - a * cc;
      if (disc <= 0.0) { continue; }
      let sq = sqrt(disc);
      t0 = max((b - sq) / a, 0.0);
      t1 = min((b + sq) / a, tmax);
    } else if (cc > 0.0) { continue; }
    // Only under the cloud base.
    if (G.up.w < base) { t1 = min(t1, sb.y); } else { t0 = max(t0, sb.x); }
    if (t1 <= t0) { continue; }
    let ns = select(select(6, 8, line), select(4, 5, line), storm_tier() <= 1);
    let dt = (t1 - t0) / f32(ns);
    // The sun reaching the shaft, looked up once in its middle. (A squall
    // line's rain lies under its towers and anvil, and the stretch through it
    // can be tens of km long: its middle says little, so per sample.)
    var vis = cloud_sun(oc + rd * (0.5 * (t0 + t1)), false).vis;
    for (var k = 0; k < ns; k++) {
      let ts = t0 + dt * (f32(k) + jit);
      let p = oc + rd * ts;
      let h = length(p) - rg;
      // The shaft slants: the rain lower down has been carried further
      // downwind, and spreads out in a foot where the downdraft meets the ground.
      let below = clamp(base - h, 0.0, base);
      let ph = p - cen - G.storm.xyz * (below * 0.15);
      let rad = length(ph - up * dot(ph, up)) / (rr * (1.0 + 0.3 * smoothstep(500.0, 0.0, h)));
      // Curtains and streaks: noise squashed flat along the vertical (columnar),
      // drifting with the wind; fading out towards the ground in places (virga).
      let q = p + G.weather.xyz;
      var qc = q - up * dot(q, up) * 0.93;
      let cxl = dot(ph, X);
      let us = shelf_u(p, cen, X, Y, R, sp) + below * 0.15;
      // A squall line's curtain is seen end-on, through kilometres of it: its
      // streaks are sheets across the line (the same all through it, varying
      // along it and slanting a little), or they averaged out along the ray
      // into one flat, even band. Finer than a lone shaft's: it is seen close.
      var sc = vec2f(9000.0, 2600.0);
      if (line) {
        qc = X * (cxl + h * 0.2) + up * (h * 0.07) + Y * (us * 0.08);
        sc = vec2f(2600.0, 800.0);
      }
      let sn = textureSampleLevel(t_noise, s_repeat, qc / sc.x + vec3f(0.6, 0.2, 0.4), 0.0);
      // Finer sheets of heavier rain within the shaft.
      let sf = textureSampleLevel(t_noise, s_repeat, qc / sc.y + vec3f(0.17, 0.83, 0.52), 0.0);
      let streak = smoothstep(0.25, 0.75, sn.g * 0.45 + sn.b * 0.3 + sf.g * 0.35 - 0.05);
      var core = (1.0 - smoothstep(mix(0.35, 0.55, sup), 1.0, rad + (sn.r - 0.5) * 0.5));
      var heavy = 0.75 * sup;
      var cores = 1.0;
      var vtop = base;
      if (line) {
        let u = us + (sn.r - 0.5) * 0.3 * R;
        // It falls from under the shelf cloud's underside (storm_sd), not
        // through the shelf: from under the base this rain is drawn in front
        // of the cloud, and inside the shelf it veiled the shelf's face.
        let ws = SHELF_W * R;
        let Dn = SHELF_D * shelf_depth(cxl, seed_of(sp));
        // (From a little inside it: the billows wear its underside away, and
        // between that and the rain grazing rays saw the bright sky far
        // behind the line.)
        vtop -= select(0.0, max(Dn * (1.0 - 0.55 * us / ws) - 600.0, 0.0), us < 1.9 * ws);
        let lr = line_rain(u, R) * (1.0 - smoothstep(L2 - 0.3 * R, L2 + 0.7 * R, abs(cxl)));
        heavy = lr.y;
        core = lr.x;
        // Heavier in cores along the line and thinner between them (as one even
        // curtain it stood as a flat grey slab under the shelf).
        cores = mix(0.6, 1.0, smoothstep(0.2, 0.8, 0.5 + 0.4 * sin(cxl / 6000.0 + seed_of(sp) * 13.0) + (sn.b - 0.5) * 0.8));
      }
      let vert = smoothstep(0.0, 250.0, vtop - h) * smoothstep(-40.0, 40.0, h) * mix(0.55, 1.0, smoothstep(0.0, base, h) * sn.a + (1.0 - sn.a));
      // Heavy rain: a few km of it is all one sees through.
      // (A shower's shaft is narrow and its rain lighter, but still stands out
      // grey from afar: extinction goes as the square root of the amount.)
      // A young cell's (a shower's) rain falls as one denser, more even shaft.
      // (A squall line's heavy rain falls in denser, more even sheets.)
      let e = 0.0015 * (1.0 + 0.5 * sup) * sqrt(amt) * core * cores * mix(mix(mix(0.5, 0.1, young), mix(0.5, 0.12, f32(line)), heavy) * (1.0 - 0.4 * sup), 1.0, streak * streak) * vert * (1.0 + 2.0 * young);
      if (e <= 1e-7) { continue; }
      let upn = p / length(p);
      let mu = dot(upn, G.sun_dir.xyz);
      if (line) { vis = cloud_sun(p, false).vis; }
      // Under the storm the rain is in deep shade: the storm above takes most of
      // the sky light, and the sun only gets in where the shadow map lets it (low
      // sun under the anvil's edge). So it stands out dark against the sky.
      // Inside it, under the storm, the light is the storm's dull grey.
      // (Its own cell's shade only, as storm_shade has it: looping over every
      // cell per sample cost a few % fps whenever a shaft was in view.)
      let pc = storm_hoff(ph, up, L2) + G.storm.xyz * (0.2 * R);
      // (Even a shower's base hides half the sky from its rain.)
      let dark = max((1.0 - smoothstep(0.7, 1.7, length(pc) / R)) * max(smoothstep(0.3, 0.75, sp.x) * smoothstep(1500.0, 4000.0, R), 0.5), sup);
      // (A squall line's heavy core takes on the green-grey of hail-laden rain.)
      // (A supercell's core, dense with rain and hail, the darkest of all.)
      var amb = storm_light(sky_amb * 0.13 + gnd_amb * 0.3, dark) * (1.0 - 0.45 * sup);
      // A squall line's curtain hangs right behind the gust front, open to the
      // bright sky low ahead: lit from the front, a dull grey-green, paler
      // where it is dense (it sends that light back) and towards the ground,
      // darker deeper in under the storm. (In the storm's shade from above it
      // was a flat blue-black band, darker than the shelf's underside.)
      if (line) { amb = vec3f(luma(sky_amb)) * mix(0.26, 0.07, smoothstep(0.6 * SHELF_W * R, (SHELF_W + 1.4) * R, us)) * mix(1.25, 0.8, smoothstep(0.0, base * 0.8, h)); }
      var S = (sun_trans(h, mu) * G.sun_dir.w * vis * 0.03 + amb) * mix(vec3f(0.88, 0.93, 0.97), vec3f(0.8, 0.94, 0.86), heavy);
      // Lightning lights up the rain around it.
      if (G.stormfx.z > 0.0) { S += flash_light(p - oc) * 0.08; }
      let st = exp(-e * dt);
      o.L += o.T * S * (1.0 - st);
      let w = o.T * (1.0 - st);
      o.t += ts * w;
      wsum += w;
      o.T *= st;
    }
  }
  if (wsum > 0.0) { o.t /= wsum; }
  return o;
}

// ------------------------------------------------------ the rain deck's rain
// Under the rain deck (nimbostratus) rain fills the air from its ragged base to
// the ground: a grey veil (heavy rain cuts the view to a few km) in broad sheets
// drifting with the wind, ending where the deck does. Integrated in a few
// samples, closer together near the camera, along the stretch of the ray under
// the deck's base; col = the dull light in it, about what the base sends down.
const NIMBO_E = 0.0008;  // extinction of the heaviest rain, 1/m
const NIMBO_RAIN_K = 0.9;
fn nimbo_rain(oc: vec3f, rd: vec3f, tmax: f32, jit: f32, col: vec3f) -> Rain {
  var o = Rain(vec3f(0.0), 1.0, 0.0);
  let rg = G.planet.w;
  let top = G.nimbo[1].w - 0.5 * G.nimbo[3].z;
  let sb = ray_sphere(oc, rd, rg + top);
  var t0 = 0.0;
  var t1 = min(tmax, 90000.0);
  if (G.up.w < top) {
    t1 = min(t1, sb.y);
  } else {
    if (sb.x <= 0.0) { return o; }
    t0 = sb.x;
    t1 = min(t1, sb.y);
  }
  if (t1 <= t0) { return o; }
  let ext = NIMBO_E * pow(G.nimbo[0].w, 1.2);
  let len = t1 - t0;
  var tp = t0;
  var wsum = 0.0;
  let n = 6;
  for (var k = 0; k < n; k++) {
    let f = f32(k + 1) / f32(n);
    let tn = t0 + len * f * f;
    let dt = tn - tp;
    let ts = mix(tp, tn, jit);
    tp = tn;
    let p = oc + rd * ts;
    let a = nimbo_area(p);
    if (a <= 0.0) { continue; }
    let r = length(p);
    let h = r - rg;
    let up = p / r;
    // Sheets of heavier and lighter rain: noise squashed flat along the vertical.
    let q = p + G.weather.xyz;
    let qc = q - up * dot(q, up) * 0.93;
    let sn = textureSampleLevel(t_noise, s_repeat, qc / 7000.0 + vec3f(0.31, 0.77, 0.12), 0.0);
    let sheet = mix(0.45, 1.3, smoothstep(0.3, 0.75, sn.g * 0.6 + sn.b * 0.4));
    let e = ext * smoothstep(0.3, 0.8, a) * sheet * smoothstep(top, top - 250.0, h);
    if (e <= 1e-8) { continue; }
    let st = exp(-e * dt);
    o.L += o.T * col * (1.0 - st);
    let w = o.T * (1.0 - st);
    o.t += ts * w;
    wsum += w;
    o.T *= st;
  }
  if (wsum > 0.0) { o.t /= wsum; }
  return o;
}

// dist.x: distance (km) to the clouds this texel sees; dist.y: distance (km) to the
// scene surface the march stopped at, for the depth-aware passes below.
struct CloudOut { @location(0) col: vec4f, @location(1) dist: vec2f };

const SKY_KM = 10000.0;  // scene distance stored for sky texels
// How well two scene distances agree, 1 = same surface.
fn depth_match(a: f32, b: f32) -> f32 {
  let rel = abs(a - b) / max(max(a, b), 1e-6);
  return exp(-rel * rel * 200.0);
}

@fragment fn fs(in: FsIn) -> CloudOut {
  var o = CloudOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.0));
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  // Each texel covers 2x2 scene pixels. Alternate texels march to the nearest and the
  // farthest of them (reverse Z: larger = nearer), so at a silhouette both surfaces
  // are represented in every 2x2 texel neighbourhood and the upsample can pick the
  // matching one instead of smearing cloud over the plane or clear sky around it.
  let hp = vec2i(in.pos.xy);
  let q = hp * 2;
  let z4 = vec4f(textureLoad(t_depth, q, 0), textureLoad(t_depth, q + vec2i(1, 0), 0),
                 textureLoad(t_depth, q + vec2i(0, 1), 0), textureLoad(t_depth, q + vec2i(1, 1), 0));
  var z = min(min(z4.x, z4.y), min(z4.z, z4.w));
  if (((hp.x + hp.y) & 1) == 0) { z = max(max(z4.x, z4.y), max(z4.z, z4.w)); }
  var tscene = 1e9;
  if (z > 0.0) { tscene = length(ray / z); }
  o.dist.y = min(tscene * 0.001, SKY_KM);
  // No clouds at all (the globe on the start screen): even at zero cover the
  // noise leaves scattered puffs and cirrus veils.
  if (G.start.x > 0.0) { return o; }
  let rg = G.planet.w;
  let oc = -G.planet.xyz;
  // Where no terrain tile wrote depth, the sky pass draws the bare planet
  // ocean at the sphere surface: stop clouds and rain shafts there so shafts
  // never X-ray through the Earth (o.dist.y stays SKY_KM for bilateral match).
  if (z == 0.0) {
    let gnd = ray_sphere(oc, rd, rg);
    if (gnd.x > 0.0) { tscene = gnd.x; }
  }
  let base = G.cloud.y;
  let alt = G.up.w;
  let sun = G.sun_dir.xyz;
  let cs = dot(rd, sun);
  let jit = fract(ign(in.pos.xy) + G.time.w * 0.618034);
  // A second, independent jitter for the light steps: tied to jit, the light offsets
  // moved in step with the sample positions and left rings of equal distance.
  let jit2 = ign(in.pos.xy + vec2f(23.0, 61.0) + 5.588238 * (G.time.w % 64.0));

  var T = 1.0;
  var L = vec3f(0.0);
  var tw = 0.0;
  var ws = 0.0;
  var vshw = 0.0;  // opacity-weighted shadow visibility and sunward transmittance, for the rim
  var litw = 0.0;
  var cw = 0.0;
  // Whether the last sample was in cloud: the ray then ended in it, not past it.
  var incloud = false;

  // Sky light for the ambient term: zenith and horizon from the sky table, plus
  // sunlight bounced off the ground below. Keep enough of the sky's natural blue
  // chroma and restrain warm ground bounce so shaded cumulus bases read clean
  // blue-grey rather than muddy beige.
  let mu0 = dot(G.up.xyz, sun);
  let sky_side = 0.5 * (sky_lut(normalize(G.east.xyz + G.up.xyz * 0.15)) + sky_lut(normalize(-G.east.xyz + G.up.xyz * 0.15)));
  let sky_raw = sky_lut(G.up.xyz) * 0.5 + sky_side * 0.5;
  let sky_amb = mix(sky_raw, vec3f(luma(sky_raw)), 0.28);
  var gnd_amb = vec3f(0.068, 0.072, 0.076) * sun_trans(0.0, mu0) * max(mu0, 0.0) * G.sun_dir.w / PI * mix(1.0, 0.45, G.cloud.x);
  // Under a storm the ground below is in its shade: little sunlight comes back up.
  gnd_amb *= 1.0 - 0.9 * G.stormfx.w;

  // Only rays that pass near a storm cell march up to the storm ceiling (and down
  // to the wall and shelf clouds under the base when a mature cell can reach
  // there); the rest keep to the ordinary layer.
  let smask = storm_mask(oc, rd, min(tscene, 320000.0));
  let rmask = g_rain_mask;
  let s_start = g_storm_start;
  let s_end = g_storm_end;
  let topa = select(base + G.cloud.z, clamp(g_storm_top, base + G.cloud.z, G.cloud.w), smask != 0u);
  let bot = select(base, g_low_bot, smask != 0u && g_storm_low);
  let outer = ray_sphere(oc, rd, rg + topa);
  let inner = ray_sphere(oc, rd, rg + bot);
  let s_base = ray_sphere(oc, rd, rg + base);
  let ord = ray_sphere(oc, rd, rg + base + G.cloud.z);
  let ord_y = ord.y;
  var ta = 0.0;
  var tb = outer.y;
  var march = outer.y > 0.0;
  if (alt < bot) {
    if (z == 0.0 && ray_sphere(oc, rd, rg).x > 0.0) { march = false; }
    ta = inner.y;
  } else {
    if (alt > topa) { ta = outer.x; }
    if (inner.x > 0.0) { tb = inner.x; }
  }
  // Skip empty sub-cloud air in front of the camera before either the ordinary
  // base or the first storm's bound begins.
  if (alt < base && smask != 0u && g_storm_low) {
    ta = max(ta, min(select(1e9, s_base.y, s_base.y > 0.0), s_start));
  }
  // Above the ordinary cloud layer, skip empty sky before whichever comes first:
  // entering the ordinary layer (ord.x) or entering the first storm bound.
  if (alt > base + G.cloud.z && smask != 0u) {
    ta = max(ta, min(select(s_start, ord.x, ord.x > 0.0), s_start));
  }
  tb = min(min(tb, tscene), ta + select(160000.0, 320000.0, smask != 0u));
  // Past the last storm's bound only the ordinary layer is left: rays climbing
  // out of it need not march on up to the storm ceiling for hundreds of km.
  if (smask != 0u) { tb = min(tb, max(ord_y, s_end)); }
  if (tb <= ta || ta < 0.0) { march = false; }

  // The mid-level layer is composited where the march passes its middle (in front
  // of or behind the cumulus and storm towers it meets), or after it.
  var ar = vec2f(-1.0);
  var aamt = 0.0;
  if (G.alto.x > 0.0) {
    ar = alto_range(oc, rd);
    ar.y = min(min(ar.y, ar.x + 60000.0), tscene);
    let ahr = rg + G.alto.y + ALTO_TOP;
    let alto_far = 250000.0 + sqrt(max((rg + alt) * (rg + alt) - ahr * ahr, 0.0));
    if (ar.x >= 0.0 && ar.x < min(tscene, alto_far) && ar.y > ar.x) { aamt = alto_amount(oc + rd * (0.5 * (ar.x + ar.y))); }
  }

  // Rain shafts under the storms: in front of the cloud seen from under the base,
  // behind it seen from above.
  var rain = Rain(vec3f(0.0), 1.0, 0.0);
  let rain_front = alt < base;
  let defer_srain = !rain_front && !g_storm_low;
  if (rmask != 0u && !defer_srain) { rain = storm_rain(oc, rd, min(tscene, 320000.0), rmask, s_base, jit, sky_amb, gnd_amb); }
  if (G.nimbo[0].w > 0.0) {
    let ncol = (nimbo_down(nimbo_top(G.up.xyz), G.nimbo[1].w) + gnd_amb * 0.3) * NIMBO_RAIN_K;
    let nr = nimbo_rain(oc, rd, min(tscene, 320000.0), jit, ncol);
    if (nr.T < 1.0) {
      let w0 = 1.0 - rain.T;
      let w1 = rain.T * (1.0 - nr.T);
      rain.t = (rain.t * w0 + nr.t * w1) / (w0 + w1);
      rain.L += rain.T * nr.L;
      rain.T *= nr.T;
    }
  }
  if (rain_front && smask == 0u && rain.T < 1.0) {
    L += rain.L;
    tw += rain.t * (1.0 - rain.T);
    ws += 1.0 - rain.T;
    T = rain.T;
  }

  if (march) {
    let qs = select(clamp(G.quality.x, 0.5, 2.0), 1.0, G.quality.x <= 0.0);
    let stq = storm_tier();
    let sqs = select(select(select(1.85, 1.4, stq == 2), 1.0, stq == 3), 0.72, stq >= 4);
    let mqs = select(qs, max(qs, sqs), smask != 0u && stq <= 2);
    let n_max = i32(clamp(200.0 / mqs, 90.0, 280.0));
    var t = ta;
    var clear = 0.0;
    var sclear = 0.0;
    var slook = 0.0;
    var sjump = 0.0;
    var ufree = false;
    var pts = ta;
    var pempty = true;
    var owx = 0u;
    var twx = -1e9;
    var st_cb = 0.0;
    for (var i = 0; i < n_max; i++) {
      // Once past the ordinary layer's far exit (ord_y), only storms remain:
      // jump straight across empty upper sky to s_start, or stop past s_end.
      if (smask != 0u && t > ord_y) {
        if (t >= s_end) { break; }
        if (t < s_start) {
          t = s_start;
          clear = 0.0;
          sclear = 0.0;
          slook = 0.0;
          sjump = 0.0;
          st_cb = 0.0;
          pts = t;
          pempty = true;
        }
      }
      let dt0 = clamp(t * select(0.016, 0.03, ufree), 40.0, 2500.0) * mqs;
      ufree = false;
      var dt = max(dt0 * (1.0 + clear), sjump);
      if (smask != 0u && sjump <= 0.0 && t >= s_start && t <= s_end) { dt = min(dt, max(sclear, dt0 * 2.0)); }
      sjump = 0.0;
      if (i >= n_max * 11 / 20 && clear > 0.0) { dt = max(dt, (tb - t) / f32(max(n_max - 4 - i, 4))); }
      dt = min(dt, tb - t);
      let ts = t + dt * jit;
      t += dt;
      sclear -= dt;
      slook -= dt;
      let atm_ = 0.5 * (ar.x + ar.y);
      if (aamt > 0.0 && ts > atm_) {
        let la = alto_march(oc, rd, ar.x, ar.y, aamt, jit, cs, sky_amb, gnd_amb + city_amb(oc + rd * atm_));
        aamt = 0.0;
        L += T * la.rgb;
        let wa = T * (1.0 - la.a);
        tw += atm_ * wa;
        ws += wa;
        T *= la.a;
        if (T < 0.004) { T = 0.0; break; }
      }
      if ((smask != 0u || rmask != 0u) && rain.T < 1.0 && ts > rain.t) {
        L += T * rain.L;
        let wr = T * (1.0 - rain.T);
        tw += rain.t * wr;
        ws += wr;
        T *= rain.T;
        rain.T = 1.0;
        if (T < 0.004) { T = 0.0; break; }
      }
      if (ts >= tb) { break; }
      let p = oc + rd * ts;
      let r = length(p);
      let h = r - rg;
      let hs = h - base;
      let in_s = smask != 0u && ts >= s_start && ts <= s_end;
      let look = in_s && (sclear <= 0.0 || (slook <= 0.0 && hs >= 0.0 && hs < G.cloud.z));
      if (hs >= 0.0 && hs < G.cloud.z && (ts - twx >= 400.0)) {
        owx = cloud_ord_pack(p, hs);
        twx = ts;
      }
      var wx = cloud_weather_unpack(p, hs, select(0u, smask, look), owx);
      if (look) {
        st_cb = wx.cb;
      } else if (in_s && st_cb > 0.0) {
        wx.cb = st_cb;
        if (hs >= 0.0 && hs < G.cloud.z) {
          let u_ow = unpack4x8unorm(owx);
          let c_ow = u_ow.x * 2.0 - 0.5;
          wx.cov = clamp(mix(c_ow * (1.0 - st_cb), 1.1, wx.ns), 0.0, 1.0);
          wx.typ = max(u_ow.y, st_cb * 0.5) * (1.0 - wx.ns);
        }
      } else if (!in_s) {
        st_cb = 0.0;
      }
      let anv_b = g_anvil;
      // The close-up erosion octave only pays off within a few km.
      let fine = 1.0 - smoothstep(1500.0, 4500.0, ts);
      g_erode = 1.0 - smoothstep(12000.0, 45000.0, ts);
      // When storm_find's upper bound already places this point outside the
      // storm's reach (tighter on smooth anvils than on billowing towers),
      // skip full storm_sd on the main step and binary search.
      let sc_w = vec4f(0.0, 0.0, 0.0, g_storm_r);
      let s_reach = select(mix(0.56, 0.16, anv_b), 0.08, hs < 0.0) * storm_bulge(sc_w) + mix(320.0, 200.0, anv_b);
      let skip_s = look && wx.ci >= 0 && g_storm_sd <= -s_reach && (wx.ns == 0.0 || g_storm_sd <= -DECK_SWELL_D);
      g_skip_storm = skip_s;
      let d = cloud_shape_f(p, h, wx, true, fine);
      let anvp = select(255.0, g_anvil, wx.ci >= 0 && !skip_s);
      if (look) {
        sclear = max(-g_storm_sd - s_reach, 0.0) * 0.72;
        slook = max(max(-g_storm_sd - 2500.0, 0.0) * 0.75, 350.0);
        if (wx.ci >= 0 && !skip_s) {
          let anv_f = (anvp - floor(anvp)) / 0.99;
          let true_sd = max(g_storm_sd_full, g_storm_sd2);
          let sc_c = G.storm_cells[2u * u32(wx.ci)];
          let s_reach_f = select(mix(0.56, 0.16, anv_f), 0.08, hs < 0.0) * storm_bulge(sc_c) + mix(320.0, 200.0, anv_f);
          sclear = max(sclear, max(-true_sd - s_reach_f, 0.0) * select(0.55, 0.65, anv_f > 0.5));
        }
      }
      if (d <= 0.002) {
        g_skip_storm = false;
        incloud = false;
        pempty = true;
        pts = ts;
        let clear_hi = wx.cb <= 0.0 && (h > base + G.cloud.z || hs > G.cloud.z * wx.top + 300.0 || wx.cov <= 0.02);
        clear = min(clear + select(0.3, 0.5, clear_hi), select(1.4, 3.0, clear_hi));
        if (in_s && sclear > dt0) {
          if (h > base + G.cloud.z) {
            sjump = select(min(sclear, h - base - G.cloud.z), sclear, t > ord_y || dot(p, rd) >= 0.0);
          } else if (h < base) {
            sjump = select(min(sclear, base - h), sclear, dot(p, rd) <= 0.0);
          }
        }
        ufree = h < base && in_s;
        continue;
      }
      clear = 0.0;
      incloud = true;
      var pl = p;
      var hl = h;
      var dl = dt;
      if (pempty && dt > 40.0) {
        var lo = pts;
        var hi = ts;
        for (var k = 0; k < select(3, 5, dt > 600.0); k++) {
          let tm = 0.5 * (lo + hi);
          let pm = oc + rd * tm;
          if (cloud_shape_f(pm, length(pm) - rg, wx, true, fine) > 0.002) { hi = tm; } else { lo = tm; }
        }
        pl = oc + rd * min(hi + 40.0, ts);
        hl = length(pl) - rg;
        dl = clamp(t - hi, 0.5 * (t - ts), dt);
      }
      g_skip_storm = false;
      let sdq0 = floor(anvp);
      let anv = (anvp - sdq0) / 0.99;
      let anv_smooth = anv > 0.65;
      // Inside a smooth glaciated anvil canopy or deep interior, stride cleanly
      // after the entry surface has been refined by binary search.
      if (!pempty && (anv > 0.70 || T < 0.22)) { ufree = true; }
      pempty = false;
      pts = ts;
      // Optical depth towards the sun; the column's weather is reused along the way.
      // Smooth anvils and deep cloud interiors (T < 0.14) use 4 wider probes
      // covering the same sunward depth; full detail stays on every visible surface.
      let has_sc = wx.ci >= 0;
      let sc_h = select(vec4f(0.0), G.storm_cells[2u * u32(max(wx.ci, 0))], has_sc);
      let sp_h = select(vec4f(0.0), G.storm_cells[2u * u32(max(wx.ci, 0)) + 1u], has_sc);
      var od = 0.0;
      let hc_l = max(hl, 0.0);
      let sun_lit = G.sun_dir.w > 0.0 && dot(pl, sun) > -sqrt(hc_l * (2.0 * rg + hc_l)) - 0.007 * (rg + hc_l);
      if (sun_lit || G.moon.w > 0.0) {
        let farl = ts > 30000.0 || anv_smooth;
        let deep_t = T < 0.14;
        let low_s = wx.ci >= 0 && stq <= 1;
        let med_s = wx.ci >= 0 && stq == 2;
        var ls = select(select(36.0, 65.0, deep_t), 90.0, farl);
        var lt = 0.0;
        var nl = select(select(select(6, 7, column_top(wx) > 2500.0), 3, hl < base && sp_h.w >= 1.0), 4, farl || deep_t);
        var lg = select(select(1.85, 2.3, deep_t), 2.5, farl);
        if (low_s) {
          nl = 3;
          ls = select(70.0, 120.0, farl || deep_t);
          lg = select(2.4, 2.7, farl || deep_t);
        } else if (med_s) {
          nl = select(select(select(4, 5, column_top(wx) > 2500.0), 3, hl < base && sp_h.w >= 1.0), 3, farl || deep_t);
          ls = select(select(48.0, 75.0, deep_t), 110.0, farl);
          lg = select(select(2.1, 2.4, deep_t), 2.6, farl);
        }
        let jl = select(0.5 + 0.4 * (jit - 0.5), jit2, wx.cb > 0.0);
        let ldet = !farl && !deep_t && !low_s && (!med_s || ts < 20000.0);
        for (var j = 0; j < nl; j++) {
          let lp = pl + sun * (lt + ls * jl);
          let lh = length(lp) - rg;
          if (od >= 1800.0 || (dot(pl, sun) > 0.0 && lh >= select(base + G.cloud.z, G.cloud.w, has_sc))) { break; }
          g_mean_erode = (ldet && j == 1) || (low_s && j == 0);
          od += cloud_shape(lp, lh, wx, ldet && j == 0) * ls;
          lt += ls;
          ls *= lg;
        }
        g_mean_erode = false;
        od *= SIGMA;
      }
      let upn = normalize(pl);
      let meso = sdq0 >= 256.0 && sdq0 < 512.0;
      let sdq = sdq0 - select(0.0, 256.0, meso);
      var nsw = wx.ns;
      var ntop = G.nimbo[2].w;
      if (nsw > 0.0 && sdq < 255.0) {
        let sdp = -16.0 * sdq;
        ntop += deck_swell(sdp, wx);
        nsw *= 1.0 - max(smoothstep(-600.0, 0.0, sdp), smoothstep(ntop - 150.0, ntop + 150.0, hl));
      }
      let hn = clamp((hl - base - (1.0 - nsw) * G.nimbo[3].y) / max(column_top(wx), 1.0), 0.0, 1.0);
      var occ = 1.0;
      let lsq = has_sc && sp_h.w >= 1.0;
      let Rq = sc_h.w * 1000.0;
      let sseed = fract(sp_h.w);
      var cxl = 0.0;
      var u = 1e9;
      if (lsq) {
        let dvq = (upn * 6371.0 + G.weather.xyz * 0.001 - sc_h.xyz) * 1000.0;
        cxl = dot(dvq, normalize(cross(upn, G.storm.xyz)));
        u = Rq * (SHELF_F + 0.12 * sin(cxl / 9000.0 + sseed * 40.0) + 0.07 * sin(cxl / 3700.0 + sseed * 17.0)) - dot(dvq, G.storm.xyz);
      }
      var vw = select(0.0, 1.0, hl < base && wx.cb > 0.0);
      var lift = 0.0;
      if (smask != 0u && hl >= base) {
        vw = smoothstep(0.0, 0.12, wx.cb) * (1.0 - smoothstep(0.15, 0.4, wx.cb));
        lift = max(base + G.cloud.z - hl, 0.0) / max(dot(upn, sun), 0.1);
        if (lsq) {
          let vl = smoothstep(0.7 * Rq, 0.3 * Rq, u) * (1.0 - smoothstep(base + 800.0, base + 1300.0, hl));
          if (vl > vw) { vw = vl; lift = 2000.0; }
        }
        if (meso) { vw = 0.8; }
      }
      var vsh = 1.0;
      if (vw > 0.0) { vsh = mix(1.0, cloud_sun(pl + sun * lift, meso).vis, vw); }
      if (lsq && hl < base) {
        let lobe = shelf_lobe(cxl, sseed);
        let ub = u + shelf_bulge(cxl, sseed, Rq);
        let xc = clamp(ub / (SHELF_FACE * Rq), 0.0, 1.0);
        let f = fract(min(xc * 3.0, 2.999));
        let Ds = SHELF_D * shelf_depth(cxl, sseed);
        let ztop = shelf_top(ub, Rq, Ds);
        let z = hl - base;
        let dz = ztop - z;
        let front = smoothstep(-300.0, -60.0, ub) * (1.0 - smoothstep(120.0, 380.0, ub)) * smoothstep(-Ds + 20.0, -Ds + 140.0, z) * (1.0 - smoothstep(-Ds * (1.0 - SHELF_NOSE), -Ds * (1.0 - SHELF_NOSE) + 120.0, z));
        let face = max(smoothstep(-90.0, 0.0, dz) * (1.0 - smoothstep(60.0, 320.0, dz)), front) * (1.0 - smoothstep(-SHELF_CEIL * 1.1, -SHELF_CEIL * 0.4, z));
        let lip = smoothstep(0.45, 0.9, f);
        let fo = 2.9 * mix(0.45, 1.2, lip) * mix(0.75, 1.15, lobe) * mix(1.3, 0.75, sqrt(xc)) * mix(1.35, 1.0, smoothstep(0.0, 0.08, xc));
        let uo = 0.24 * mix(0.6, 1.5, smoothstep(-SHELF_CEIL * 0.9, -SHELF_CEIL * 1.7, z));
        occ = mix(mix(1.0, uo, smoothstep(-0.3, 0.1, dot(rd, upn))) * mix(0.45, 1.0, vsh), fo, face);
      } else {
        let n_occ = select(1, 2, wx.cb > 0.0 && T >= 0.14 && !anv_smooth && stq >= 3);
        occ = 1.0;
        for (var oi = 0; oi < n_occ; oi++) {
          let ho = select(select(450.0, 150.0 + 1150.0 * fract(jit2 + 0.5), wx.cb > 0.0), 2500.0 * (0.6 + 0.8 * jit2), oi == 1);
          let odo = cloud_shape(pl + upn * ho, hl + ho, wx, false) * select(900.0, 3000.0, oi == 1) * SIGMA;
          occ *= mix(1.0 / (1.0 + select(0.15, 0.06, oi == 1) * odo), 1.0, select(0.12, 0.3, oi == 1));
        }
        occ = mix(occ, 0.35, 0.75 * (1.0 - vsh)) * mix(0.45, 1.0, vsh);
      }
      if (nsw > 0.0) {
        let tau = SIGMA * 0.55 * max(ntop - hl, 0.0) * nsw;
        od = max(od, tau / max(dot(upn, sun), 0.3));
        occ = min(occ, 0.1 + 0.9 / (1.0 + 0.1 * tau));
      }
      var ms = 0.0;
      var a = 1.0;
      var b = 1.0;
      var c = 1.0;
      for (var k = 0; k < 4; k++) {
        ms += a * exp(-od * b) * cloud_phase(cs, c);
        a *= 0.55;
        b *= 0.38;
        c *= 0.5;
      }
      ms += 0.4 / (4.0 * PI) / (1.0 + 0.25 * od);
      ms += cloud_glow(cs, od * (1.0 + 4.0 * wx.cb));
      let dprob = 0.08 + pow(d, remap(clamp(hn, 0.3, 0.85), 0.3, 0.85, 0.5, 2.0));
      let vprob = pow(clamp(remap(hn, 0.05, 0.2, 0.25, 1.0), 0.25, 1.0), 0.8);
      let powder = mix(1.0, clamp(dprob * vprob * 1.25, 0.0, 1.0), 0.55 * (1.0 - smoothstep(0.0, 0.6, cs)));
      let mu = dot(upn, sun);
      var sunc = sun_trans(hl, mu) * G.sun_dir.w;
      sunc *= vsh;
      let anv_occ = mix(0.32, 0.62, smoothstep(0.70, 0.98, hn));
      occ = mix(occ, max(occ, anv_occ), anv);
      var cg = vec3f(0.0);
      if (glow_on()) { cg = city_amb(pl); }
      var amb = (sky_amb * mix(0.32, 0.96, hn) * mix(0.18, 1.0, occ) + gnd_amb * mix(1.0, 0.25, hn * (1.0 - 0.7 * anv)) * (1.0 - 0.75 * wx.cb) * mix(0.5, 1.0, occ) + cg * (mix(1.0, 0.25, hn * (1.0 - 0.7 * anv)) + (1.0 - hn) * 0.5)) * mix(0.5, 1.0, powder);
      let moonc = moon_light(upn, hl);
      var S = ((sunc + moonc) * ms * MS_GAIN * powder + amb) * d * SIGMA;
      if (nsw > 0.0) {
        let scud = 1.0 - smoothstep(0.0, G.nimbo[3].z * 1.3, hl - G.nimbo[1].w + G.nimbo[3].z);
        S = max(S, nimbo_down(nimbo_top(G.up.xyz), hl) * (mix(0.75, 1.2, wx.cell) * mix(0.55, 1.0, powder) * mix(1.0, 0.5, scud) * nsw * d * SIGMA));
      }
      if (G.stormfx.z > 0.0) { S += flash_cloud(pl - oc) * (0.1 * d * SIGMA); }
      let e = d * SIGMA;
      let st = exp(-e * dl);
      L += T * S * (1.0 - st) / e;
      let wgt = T * (1.0 - st);
      vshw += wgt * vsh;
      litw += wgt * exp(-od);
      cw += wgt;
      tw += ts * wgt;
      ws += wgt;
      T *= st;
      if (T < 0.012) { T = 0.0; break; }
    }
    let cut = incloud && tb >= tscene;
    if (cw > 0.0 && cs > 0.5 && T > 0.0 && !cut) {
      let tau = -log(max(T, 1e-4));
      let lit = litw / cw;
      let p_rim = oc + rd * (tw / max(ws, 1e-6));
      let h_rim = length(p_rim) - rg;
      let sunc_rim = sun_trans(h_rim, dot(normalize(p_rim), sun)) * G.sun_dir.w * (vshw / cw);
      L += sunc_rim * (RIM * hg(cs, 0.94) * tau * exp(-tau) * (1.0 - lit));
    }
  }
  if (aamt > 0.0 && T > 0.004) {
    let atm_ = 0.5 * (ar.x + ar.y);
    let la = alto_march(oc, rd, ar.x, ar.y, aamt, jit, cs, sky_amb, gnd_amb + city_amb(oc + rd * atm_));
    L += T * la.rgb;
    let wa = T * (1.0 - la.a);
    tw += atm_ * wa;
    ws += wa;
    T *= la.a;
  }
  if (defer_srain && rmask != 0u && T > 0.004) {
    let sr = storm_rain(oc, rd, min(tscene, 320000.0), rmask, s_base, jit, sky_amb, gnd_amb);
    if (sr.T < 1.0) {
      L += T * sr.L;
      let wr = T * (1.0 - sr.T);
      tw += sr.t * wr;
      ws += wr;
      T *= sr.T;
    }
  }
  if ((!rain_front || smask != 0u || rmask != 0u) && rain.T < 1.0 && T > 0.004) {
    L += T * rain.L;
    let wr = T * (1.0 - rain.T);
    tw += rain.t * wr;
    ws += wr;
    T *= rain.T;
  }
  // Distant clouds sink into the haze.
  if (ws > 0.0) {
    let tc = tw / ws;
    o.dist.x = tc * 0.001;
    let top = ray_sphere(oc, rd, rg + ATM_H);
    let sc = atm_march(oc, rd, max(top.x, 0.0), tc, 6, jit);
    L = L * sc.T + sc.L * (1.0 - T);
  }

  // Cirrus sheet, behind the cumulus layer when seen from below.
  if (T > 0.01) {
    let ci = ray_sphere(oc, rd, rg + CIRRUS_H);
    var tci = -1.0;
    if (alt < CIRRUS_H) { tci = ci.y; } else if (ci.x > 0.0 && ray_sphere(oc, rd, rg).x < 0.0) { tci = ci.x; }
    if (tci > 0.0 && tci < tscene && tci < 400000.0) {
      let p = oc + rd * tci;
      let r = length(p);
      let upn = p / r;
      let slant = 1.0 / max(abs(dot(rd, upn)), 0.08);
      let tau = cirrus(p) * slant * 0.35;
      if (tau > 0.001) {
        let mu = dot(upn, sun);
        let sunc = sun_trans(CIRRUS_H, mu) * G.sun_dir.w;
        let ph = mix(hg(cs, 0.75), hg(cs, -0.1), 0.35) + 0.04;
        var Lc = ((sunc + moon_light(upn, CIRRUS_H)) * ph + sky_lut(G.up.xyz) * 0.5 + gnd_amb * 0.3) * (1.0 - exp(-tau)) * 0.95;
        let ac = 1.0 - exp(-tau);
        let top = ray_sphere(oc, rd, rg + ATM_H);
        let sc = atm_march(oc, rd, max(top.x, 0.0), tci, 4, jit);
        Lc = Lc * sc.T + sc.L * ac;
        L += T * Lc;
        if (ws <= 0.0) { o.dist.x = tci * 0.001; }
        T *= 1.0 - ac;
      }
    }
  }
  o.col = vec4f(L, T);
  return o;
}

// Specialized cloud march for zero-storm, zero-rain-deck skies (render.rs
// `p_cloud_plain`): identical ordinary cumulus/stratus/alto/cirrus rendering
// without storm_sd, storm_rain, or nimbo_rain compiled into the pipeline,
// halving register pressure and raising GPU occupancy.
@fragment fn fs_plain(in: FsIn) -> CloudOut {
  var o = CloudOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.0));
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  let hp = vec2i(in.pos.xy);
  let q = hp * 2;
  let z4 = vec4f(textureLoad(t_depth, q, 0), textureLoad(t_depth, q + vec2i(1, 0), 0),
                 textureLoad(t_depth, q + vec2i(0, 1), 0), textureLoad(t_depth, q + vec2i(1, 1), 0));
  var z = min(min(z4.x, z4.y), min(z4.z, z4.w));
  if (((hp.x + hp.y) & 1) == 0) { z = max(max(z4.x, z4.y), max(z4.z, z4.w)); }
  var tscene = 1e9;
  if (z > 0.0) { tscene = length(ray / z); }
  o.dist.y = min(tscene * 0.001, SKY_KM);
  if (G.start.x > 0.0) { return o; }
  let rg = G.planet.w;
  let oc = -G.planet.xyz;
  if (z == 0.0) {
    let gnd = ray_sphere(oc, rd, rg);
    if (gnd.x > 0.0) { tscene = gnd.x; }
  }
  let base = G.cloud.y;
  let alt = G.up.w;
  let sun = G.sun_dir.xyz;
  let cs = dot(rd, sun);
  let jit = fract(ign(in.pos.xy) + G.time.w * 0.618034);

  var T = 1.0;
  var L = vec3f(0.0);
  var tw = 0.0;
  var ws = 0.0;
  var litw = 0.0;
  var cw = 0.0;
  var incloud = false;

  let mu0 = dot(G.up.xyz, sun);
  let sky_side = 0.5 * (sky_lut(normalize(G.east.xyz + G.up.xyz * 0.15)) + sky_lut(normalize(-G.east.xyz + G.up.xyz * 0.15)));
  let sky_raw = sky_lut(G.up.xyz) * 0.5 + sky_side * 0.5;
  let sky_amb = mix(sky_raw, vec3f(luma(sky_raw)), 0.28);
  var gnd_amb = vec3f(0.068, 0.072, 0.076) * sun_trans(0.0, mu0) * max(mu0, 0.0) * G.sun_dir.w / PI * mix(1.0, 0.45, G.cloud.x);
  gnd_amb *= 1.0 - 0.9 * G.stormfx.w;

  let topa = base + G.cloud.z;
  let outer = ray_sphere(oc, rd, rg + topa);
  let inner = ray_sphere(oc, rd, rg + base);
  var ta = 0.0;
  var tb = outer.y;
  var march = outer.y > 0.0;
  if (alt < base) {
    if (z == 0.0 && ray_sphere(oc, rd, rg).x > 0.0) { march = false; }
    ta = inner.y;
  } else {
    if (alt > topa) { ta = outer.x; }
    if (inner.x > 0.0) { tb = inner.x; }
  }
  tb = min(min(tb, tscene), ta + 160000.0);
  if (tb <= ta || ta < 0.0) { march = false; }

  var ar = vec2f(-1.0);
  var aamt = 0.0;
  if (G.alto.x > 0.0) {
    ar = alto_range(oc, rd);
    ar.y = min(min(ar.y, ar.x + 60000.0), tscene);
    let ahr = rg + G.alto.y + ALTO_TOP;
    let alto_far = 250000.0 + sqrt(max((rg + alt) * (rg + alt) - ahr * ahr, 0.0));
    if (ar.x >= 0.0 && ar.x < min(tscene, alto_far) && ar.y > ar.x) { aamt = alto_amount(oc + rd * (0.5 * (ar.x + ar.y))); }
  }

  if (march) {
    let qs = select(clamp(G.quality.x, 0.5, 2.0), 1.0, G.quality.x <= 0.0);
    let n_max = i32(200.0 / qs);
    var t = ta;
    var clear = 0.0;
    var pts = ta;
    var pempty = true;
    var owx = 0u;
    var twx = -1e9;
    for (var i = 0; i < n_max; i++) {
      let dt0 = clamp(t * 0.016, 40.0, 2500.0) * qs;
      var dt = dt0 * (1.0 + clear);
      if (i >= n_max * 11 / 20 && clear > 0.0) { dt = max(dt, (tb - t) / f32(max(n_max - 4 - i, 4))); }
      dt = min(dt, tb - t);
      let ts = t + dt * jit;
      t += dt;
      let atm_ = 0.5 * (ar.x + ar.y);
      if (aamt > 0.0 && ts > atm_) {
        let la = alto_march(oc, rd, ar.x, ar.y, aamt, jit, cs, sky_amb, gnd_amb + city_amb(oc + rd * atm_));
        aamt = 0.0;
        L += T * la.rgb;
        let wa = T * (1.0 - la.a);
        tw += atm_ * wa;
        ws += wa;
        T *= la.a;
        if (T < 0.004) { T = 0.0; break; }
      }
      if (ts >= tb) { break; }
      let p = oc + rd * ts;
      let h = length(p) - rg;
      let hs = h - base;
      if (ts - twx >= 400.0) {
        owx = cloud_ord_pack(p, max(hs, 0.0));
        twx = ts;
      }
      let u = unpack4x8unorm(owx);
      let ow = vec4f(clamp(u.x * 2.0 - 0.5, 0.0, 1.0), u.y, u.z, u.w);
      let fine = 1.0 - smoothstep(1500.0, 4500.0, ts);
      g_erode = 1.0 - smoothstep(12000.0, 45000.0, ts);
      let d = cloud_shape_plain(p, h, ow, true, fine);
      if (d <= 0.002) {
        incloud = false;
        pempty = true;
        pts = ts;
        let clear_hi = hs > G.cloud.z * ow.z + 300.0 || ow.x <= 0.02;
        clear = min(clear + select(0.3, 0.5, clear_hi), select(1.4, 3.0, clear_hi));
        continue;
      }
      clear = 0.0;
      incloud = true;
      var pl = p;
      var hl = h;
      var dl = dt;
      if (pempty && dt > 40.0) {
        var lo = pts;
        var hi = ts;
        for (var k = 0; k < select(3, 5, dt > 600.0); k++) {
          let tm = 0.5 * (lo + hi);
          let pm = oc + rd * tm;
          if (cloud_shape_plain(pm, length(pm) - rg, ow, true, fine) > 0.002) { hi = tm; } else { lo = tm; }
        }
        pl = oc + rd * min(hi + 40.0, ts);
        hl = length(pl) - rg;
        dl = clamp(t - hi, 0.5 * (t - ts), dt);
      }
      pempty = false;
      pts = ts;
      var od = 0.0;
      let hc_l = max(hl, 0.0);
      let sun_lit = G.sun_dir.w > 0.0 && dot(pl, sun) > -sqrt(hc_l * (2.0 * rg + hc_l)) - 0.007 * (rg + hc_l);
      if (sun_lit || G.moon.w > 0.0) {
        let farl = ts > 30000.0;
        let deep_t = T < 0.14;
        var ls = select(select(36.0, 65.0, deep_t), 90.0, farl);
        var lt = 0.0;
        let nl = select(select(6, 7, G.cloud.z * ow.z > 2500.0), 4, farl || deep_t);
        let lg = select(select(1.85, 2.3, deep_t), 2.5, farl);
        let jl = 0.5 + 0.4 * (jit - 0.5);
        let ldet = !farl && !deep_t;
        for (var j = 0; j < nl; j++) {
          let lp = pl + sun * (lt + ls * jl);
          let lh = length(lp) - rg;
          if (od >= 1800.0 || (dot(pl, sun) > 0.0 && lh >= topa)) { break; }
          g_mean_erode = ldet && j == 1;
          od += cloud_shape_plain(lp, lh, ow, ldet && j == 0, 0.0) * ls;
          lt += ls;
          ls *= lg;
        }
        g_mean_erode = false;
        od *= SIGMA;
      }
      let upn = normalize(pl);
      let hn = clamp((hl - base) / max(G.cloud.z * ow.z, 1.0), 0.0, 1.0);
      let od_up = cloud_shape_plain(pl + upn * 450.0, hl + 450.0, ow, false, 0.0) * 900.0 * SIGMA;
      let occ = mix(1.0 / (1.0 + 0.15 * od_up), 1.0, 0.12);
      var ms = 0.0;
      var a = 1.0;
      var b = 1.0;
      var c = 1.0;
      for (var k = 0; k < 4; k++) {
        ms += a * exp(-od * b) * cloud_phase(cs, c);
        a *= 0.55;
        b *= 0.38;
        c *= 0.5;
      }
      ms += 0.4 / (4.0 * PI) / (1.0 + 0.25 * od);
      ms += cloud_glow(cs, od);
      let dprob = 0.08 + pow(d, remap(clamp(hn, 0.3, 0.85), 0.3, 0.85, 0.5, 2.0));
      let vprob = pow(clamp(remap(hn, 0.05, 0.2, 0.25, 1.0), 0.25, 1.0), 0.8);
      let powder = mix(1.0, clamp(dprob * vprob * 1.25, 0.0, 1.0), 0.55 * (1.0 - smoothstep(0.0, 0.6, cs)));
      let mu = dot(upn, sun);
      let sunc = sun_trans(hl, mu) * G.sun_dir.w;
      var cg = vec3f(0.0);
      if (glow_on()) { cg = city_amb(pl); }
      let amb = (sky_amb * mix(0.32, 0.96, hn) * mix(0.18, 1.0, occ) + gnd_amb * mix(1.0, 0.25, hn) * mix(0.5, 1.0, occ) + cg * (mix(1.0, 0.25, hn) + (1.0 - hn) * 0.5)) * mix(0.5, 1.0, powder);
      let moonc = moon_light(upn, hl);
      var S = ((sunc + moonc) * ms * MS_GAIN * powder + amb) * d * SIGMA;
      if (G.stormfx.z > 0.0) { S += flash_cloud(pl - oc) * (0.1 * d * SIGMA); }
      let e = d * SIGMA;
      let st = exp(-e * dl);
      L += T * S * (1.0 - st) / e;
      let wgt = T * (1.0 - st);
      litw += wgt * exp(-od);
      cw += wgt;
      tw += ts * wgt;
      ws += wgt;
      T *= st;
      if (T < 0.004) { T = 0.0; break; }
    }
    let cut = incloud && tb >= tscene;
    if (cw > 0.0 && cs > 0.5 && T > 0.0 && !cut) {
      let tau = -log(max(T, 1e-4));
      let lit = litw / cw;
      let p_rim = oc + rd * (tw / max(ws, 1e-6));
      let h_rim = length(p_rim) - rg;
      let sunc_rim = sun_trans(h_rim, dot(normalize(p_rim), sun)) * G.sun_dir.w;
      L += sunc_rim * (RIM * hg(cs, 0.94) * tau * exp(-tau) * (1.0 - lit));
    }
  }
  if (aamt > 0.0 && T > 0.004) {
    let atm_ = 0.5 * (ar.x + ar.y);
    let la = alto_march(oc, rd, ar.x, ar.y, aamt, jit, cs, sky_amb, gnd_amb + city_amb(oc + rd * atm_));
    L += T * la.rgb;
    let wa = T * (1.0 - la.a);
    tw += atm_ * wa;
    ws += wa;
    T *= la.a;
  }
  if (ws > 0.0) {
    let tc = tw / ws;
    o.dist.x = tc * 0.001;
    let top = ray_sphere(oc, rd, rg + ATM_H);
    let sc = atm_march(oc, rd, max(top.x, 0.0), tc, 6, jit);
    L = L * sc.T + sc.L * (1.0 - T);
  }
  if (T > 0.01) {
    let ci = ray_sphere(oc, rd, rg + CIRRUS_H);
    var tci = -1.0;
    if (alt < CIRRUS_H) { tci = ci.y; } else if (ci.x > 0.0 && ray_sphere(oc, rd, rg).x < 0.0) { tci = ci.x; }
    if (tci > 0.0 && tci < tscene && tci < 400000.0) {
      let p = oc + rd * tci;
      let r = length(p);
      let upn = p / r;
      let slant = 1.0 / max(abs(dot(rd, upn)), 0.08);
      let tau = cirrus(p) * slant * 0.35;
      if (tau > 0.001) {
        let mu = dot(upn, sun);
        let sunc = sun_trans(CIRRUS_H, mu) * G.sun_dir.w;
        let ph = mix(hg(cs, 0.75), hg(cs, -0.1), 0.35) + 0.04;
        var Lc = ((sunc + moon_light(upn, CIRRUS_H)) * ph + sky_lut(G.up.xyz) * 0.5 + gnd_amb * 0.3) * (1.0 - exp(-tau)) * 0.95;
        let ac = 1.0 - exp(-tau);
        let top = ray_sphere(oc, rd, rg + ATM_H);
        let sc = atm_march(oc, rd, max(top.x, 0.0), tci, 4, jit);
        Lc = Lc * sc.T + sc.L * ac;
        L += T * Lc;
        if (ws <= 0.0) { o.dist.x = tci * 0.001; }
        T *= 1.0 - ac;
      }
    }
  }
  o.col = vec4f(L, T);
  return o;
}

// Decoupled pure-storm cloud march (render.rs `p_cloud_storm_cls`): runs via
// Early-Z only on storm-intersecting pixels, reads cached `storm_mask` bounds
// from `t_cls_data`, marches ONLY `[s_start, s_end]` with unrestricted
// sphere-tracing at every altitude, and omits all ordinary-cloud, alto, cirrus,
// haze, and rain logic so GPU register pressure stays low. Outputs unhazed
// `(L_storm, T_storm)` and `(t_weighted_km, t_entry_km)` for `fs_plain_comp`.
@fragment fn fs_storm(in: FsIn) -> CloudOut {
  var o = CloudOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.0));
  if (G.start.x > 0.0) { return o; }
  let hp = vec2i(in.pos.xy);
  let cls = textureLoad(t_cls_data, hp, 0);
  let smask = cls.x & 0xffffu;
  if (smask == 0u) { return o; }
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  let q = hp * 2;
  let z4 = vec4f(textureLoad(t_depth, q, 0), textureLoad(t_depth, q + vec2i(1, 0), 0),
                 textureLoad(t_depth, q + vec2i(0, 1), 0), textureLoad(t_depth, q + vec2i(1, 1), 0));
  var z = min(min(z4.x, z4.y), min(z4.z, z4.w));
  if (((hp.x + hp.y) & 1) == 0) { z = max(max(z4.x, z4.y), max(z4.z, z4.w)); }
  var tscene = 1e9;
  if (z > 0.0) { tscene = length(ray / z); }
  let rg = G.planet.w;
  let oc = -G.planet.xyz;
  if (z == 0.0) {
    let gnd = ray_sphere(oc, rd, rg);
    if (gnd.x > 0.0) { tscene = gnd.x; }
  }
  let base = G.cloud.y;
  let alt = G.up.w;
  let s_start = bitcast<f32>(cls.y);
  let s_end = bitcast<f32>(cls.z);
  let storm_top_h = f32(cls.w & 0xffffu);
  let low_drop = cls.w >> 16u;
  let topa = clamp(storm_top_h, base + G.cloud.z, G.cloud.w);
  let bot = select(base, base - f32(low_drop), low_drop > 0u);
  let outer = ray_sphere(oc, rd, rg + topa);
  if (outer.y <= 0.0) { return o; }
  let inner = ray_sphere(oc, rd, rg + bot);
  var ta = 0.0;
  var tb = outer.y;
  if (alt < bot) {
    if (z == 0.0 && ray_sphere(oc, rd, rg).x > 0.0) { return o; }
    ta = inner.y;
  } else {
    if (alt > topa) { ta = outer.x; }
    if (inner.x > 0.0) { tb = inner.x; }
  }
  ta = max(ta, s_start);
  tb = min(min(min(tb, tscene), ta + 320000.0), s_end);
  if (tb <= ta || ta < 0.0) { return o; }

  let sun = G.sun_dir.xyz;
  let cs = dot(rd, sun);
  let jit = fract(ign(in.pos.xy) + G.time.w * 0.618034);
  let jit2 = ign(in.pos.xy + vec2f(23.0, 61.0) + 5.588238 * (G.time.w % 64.0));
  let mu0 = dot(G.up.xyz, sun);
  let sky_side = 0.5 * (sky_lut(normalize(G.east.xyz + G.up.xyz * 0.15)) + sky_lut(normalize(-G.east.xyz + G.up.xyz * 0.15)));
  let sky_raw = sky_lut(G.up.xyz) * 0.5 + sky_side * 0.5;
  let sky_amb = mix(sky_raw, vec3f(luma(sky_raw)), 0.28);
  var gnd_amb = vec3f(0.068, 0.072, 0.076) * sun_trans(0.0, mu0) * max(mu0, 0.0) * G.sun_dir.w / PI * mix(1.0, 0.45, G.cloud.x);
  gnd_amb *= 1.0 - 0.9 * G.stormfx.w;

  var T = 1.0;
  var L = vec3f(0.0);
  var tw = 0.0;
  var ws = 0.0;
  var vshw = 0.0;
  var litw = 0.0;
  var cw = 0.0;
  var first_t = 0.0;
  var incloud = false;

  let stq = storm_tier();
  let sqs = select(select(select(1.85, 1.4, stq == 2), 1.0, stq == 3), 0.72, stq >= 4);
  let n_max = i32(clamp(160.0 / sqs, 80.0, 240.0));
  var t = ta;
  var clear = 0.0;
  var ufree = false;
  var pts = ta;
  var pempty = true;
  for (var i = 0; i < n_max; i++) {
    let dt0 = clamp(t * select(0.016, 0.03, ufree), 40.0, 2500.0) * sqs;
    ufree = false;
    var dt = dt0 * (1.0 + clear);
    if (i >= n_max * 11 / 20 && clear > 0.0) { dt = max(dt, (tb - t) / f32(max(n_max - 4 - i, 4))); }
    dt = min(dt, tb - t);
    let ts = t + dt * jit;
    t += dt;
    if (ts >= tb) { break; }
    let p = oc + rd * ts;
    let h = length(p) - rg;
    let hs = h - base;
    g_storm_sd = -1e6;
    g_storm_sd2 = -1e6;
    g_storm_r = 0.0;
    g_anvil = 0.0;
    let st = storm_find(p, hs, smask);
    let anv_b = g_anvil;
    let lowz_b = (1.0 - smoothstep(0.0, 1500.0, hs)) * smoothstep(-STORM_BELOW, -STORM_BELOW + 250.0, hs);
    let s_reach = select(mix(0.55, 0.085, anv_b), 0.08, hs < 0.0) * storm_bulge(vec4f(0.0, 0.0, 0.0, g_storm_r)) + 300.0 * lowz_b * (1.0 - anv_b) + 40.0;
    if (st.ci < 0 || g_storm_sd <= -s_reach) {
      incloud = false;
      pempty = true;
      let sclear = max(-g_storm_sd - s_reach, 0.0) * select(0.72, 0.76, anv_b > 0.5);
      let t_safe = min(ts + sclear, tb);
      if (t_safe > t) {
        t = t_safe;
        pts = t_safe;
      } else {
        pts = ts;
      }
      clear = min(clear + 0.5, 3.0);
      ufree = h < base;
      continue;
    }
    let ci = u32(st.ci);
    let cb = st.cb;
    let fine = select(select(1.0 - smoothstep(1000.0, 2600.0, ts), 1.0 - smoothstep(1500.0, 4500.0, ts), stq >= 3), 0.0, stq <= 1);
    g_erode = select(select(1.0 - smoothstep(9000.0, 32000.0, ts), 1.0 - smoothstep(12000.0, 45000.0, ts), stq >= 3), 1.0 - smoothstep(5000.0, 18000.0, ts), stq <= 1);
    let far_det = stq <= 1 && ts > 18000.0;
    // Single cloud_shape_storm_only call site serving main (0), bisect (1),
    // sunward light march (2), and sky occlusion (3).
    var smode = 0u;
    var sidx = 0;
    var d = 0.0;
    var anv = 0.0;
    var meso = false;
    var lo = pts;
    var hi = ts;
    var nb = 0;
    var pl = p;
    var hl = h;
    var dl = dt;
    var upn = vec3f(0.0);
    var lsq = false;
    var do_occ = true;
    var ldet = false;
    var nl = 0;
    var n_occ = 0;
    var od = 0.0;
    var occ = 1.0;
    var ls = 0.0;
    var lt = 0.0;
    var lg = 1.0;
    var sun_up = false;
    var ctop = 0.0;
    var sclear_f = 0.0;
    loop {
      var sp_p = p;
      var sp_h = h;
      var sp_det = !far_det;
      var sp_fine = fine;
      g_mean_erode = far_det;
      if (smode == 1u) {
        let tm = 0.5 * (lo + hi);
        sp_p = oc + rd * tm;
        sp_h = length(sp_p) - rg;
      } else if (smode == 2u) {
        sp_p = pl + sun * (lt + ls * jit2);
        sp_h = length(sp_p) - rg;
        if (od >= 1800.0 || (sun_up && sp_h >= G.cloud.w)) {
          if (n_occ > 0) {
            smode = 3u;
            sidx = 0;
            continue;
          }
          break;
        }
        sp_det = ldet && sidx == 0;
        g_mean_erode = (ldet && sidx == 1) || (!ldet && stq <= 1 && sidx == 0);
        sp_fine = 0.0;
      } else if (smode == 3u) {
        let ho = select(150.0 + 1150.0 * fract(jit2 + 0.5), 2500.0 * (0.6 + 0.8 * jit2), sidx == 1);
        sp_p = pl + upn * ho;
        sp_h = hl + ho;
        sp_det = false;
        g_mean_erode = false;
        sp_fine = 0.0;
      }
      let dm = cloud_shape_storm_only(sp_p, sp_h, ci, cb, sp_det, sp_fine);
      if (smode == 0u) {
        d = dm;
        anv = g_anvil;
        meso = g_meso;
        if (d <= 0.002) {
          let true_sd = max(g_storm_sd_full, g_storm_sd2);
          let sc_c = G.storm_cells[2u * ci];
          let s_reach_c = select(mix(0.55, 0.085, anv), 0.08, hs < 0.0) * storm_bulge(sc_c) + 300.0 * lowz_b * (1.0 - anv) + 40.0;
          let s_reach_f = select(s_reach_c, 2100.0, g_storm_sd2 > g_storm_sd_full);
          sclear_f = max(-true_sd - s_reach_f, 0.0) * select(0.72, 0.76, anv > 0.5 && g_storm_sd2 <= g_storm_sd_full);
          break;
        }
        let nb_max = select(select(3, select(3, 5, dt > 600.0), stq >= 3), 2, stq <= 1);
        nb = select(0, nb_max, pempty && dt > 40.0);
        if (nb > 0) {
          smode = 1u;
          sidx = 0;
          continue;
        }
      } else if (smode == 1u) {
        let tm = 0.5 * (lo + hi);
        if (dm > 0.002) { hi = tm; } else { lo = tm; }
        sidx += 1;
        if (sidx < nb) { continue; }
        pl = oc + rd * min(hi + 40.0, ts);
        hl = length(pl) - rg;
        dl = clamp(t - hi, 0.5 * (t - ts), dt);
      } else if (smode == 2u) {
        od += dm * ls;
        lt += ls;
        ls *= lg;
        sidx += 1;
        if (sidx < nl) { continue; }
        if (n_occ > 0) {
          smode = 3u;
          sidx = 0;
          continue;
        }
        break;
      } else {
        let odo = dm * select(900.0, 3000.0, sidx == 1) * SIGMA;
        occ *= mix(1.0 / (1.0 + select(0.15, 0.06, sidx == 1) * odo), 1.0, select(0.12, 0.3, sidx == 1));
        sidx += 1;
        if (sidx < n_occ) { continue; }
        break;
      }
      let anv_smooth = anv > 0.65;
      let sp_h_tmp = G.storm_cells[2u * ci + 1u];
      upn = normalize(pl);
      lsq = sp_h_tmp.w >= 1.0;
      do_occ = !(lsq && hl < base);
      let hc_l = max(hl, 0.0);
      let sun_lit = G.sun_dir.w > 0.0 && dot(pl, sun) > -sqrt(hc_l * (2.0 * rg + hc_l)) - 0.007 * (rg + hc_l);
      let farl = ts > 30000.0 || anv_smooth;
      let deep_t = T < 0.14;
      ldet = !farl && !deep_t && stq >= 2 && (stq >= 3 || ts < 20000.0);
      ctop = (sp_h_tmp.y - base) * storm_top(sp_h_tmp.x);
      let want_light = sun_lit || G.moon.w > 0.0;
      if (!want_light) {
        nl = 0;
      } else if (stq <= 1) {
        nl = 3;
        ls = select(70.0, 120.0, farl || deep_t);
        lg = select(2.4, 2.7, farl || deep_t);
      } else if (stq == 2) {
        nl = select(select(select(4, 5, ctop > 2500.0), 3, hl < base && lsq), 3, farl || deep_t);
        ls = select(select(48.0, 75.0, deep_t), 110.0, farl);
        lg = select(select(2.1, 2.4, deep_t), 2.6, farl);
      } else {
        nl = select(select(select(6, 7, ctop > 2500.0), 3, hl < base && lsq), 4, farl || deep_t);
        ls = select(select(36.0, 65.0, deep_t), 90.0, farl);
        lg = select(select(1.85, 2.3, deep_t), 2.5, farl);
      }
      n_occ = select(0, select(1, 2, !deep_t && !anv_smooth && stq >= 3), do_occ);
      lt = 0.0;
      sun_up = dot(upn, sun) > 0.0;
      sidx = 0;
      if (nl > 0) {
        smode = 2u;
      } else if (n_occ > 0) {
        smode = 3u;
      } else {
        break;
      }
    }
    g_mean_erode = false;
    if (d <= 0.002) {
      incloud = false;
      pempty = true;
      let t_safe = min(ts + sclear_f, tb);
      if (t_safe > t) {
        t = t_safe;
        pts = t_safe;
      } else {
        pts = ts;
      }
      clear = min(clear + 0.5, 3.0);
      ufree = h < base;
      continue;
    }
    if (first_t <= 0.0) { first_t = select(ts, hi, nb > 0); }
    clear = 0.0;
    incloud = true;
    if (!pempty && (anv > 0.70 || T < 0.22)) { ufree = true; }
    pempty = false;
    pts = ts;
    od *= SIGMA;
    let sc_h = G.storm_cells[2u * ci];
    let sp_h = G.storm_cells[2u * ci + 1u];
    let hn = clamp((hl - base) / max(ctop, 1.0), 0.0, 1.0);
    let Rq = sc_h.w * 1000.0;
    let sseed = fract(sp_h.w);
    var cxl = 0.0;
    var u = 1e9;
    if (lsq) {
      let dvq = (upn * 6371.0 + G.weather.xyz * 0.001 - sc_h.xyz) * 1000.0;
      cxl = dot(dvq, normalize(cross(upn, G.storm.xyz)));
      u = Rq * (SHELF_F + 0.12 * sin(cxl / 9000.0 + sseed * 40.0) + 0.07 * sin(cxl / 3700.0 + sseed * 17.0)) - dot(dvq, G.storm.xyz);
    }
    var vw = select(0.0, 1.0, hl < base);
    var lift = 0.0;
    if (hl >= base) {
      vw = smoothstep(0.0, 0.12, cb) * (1.0 - smoothstep(0.15, 0.4, cb));
      lift = max(base + G.cloud.z - hl, 0.0) / max(dot(upn, sun), 0.1);
      if (lsq) {
        let vl = smoothstep(0.7 * Rq, 0.3 * Rq, u) * (1.0 - smoothstep(base + 800.0, base + 1300.0, hl));
        if (vl > vw) { vw = vl; lift = 2000.0; }
      }
      if (meso) { vw = 0.8; }
    }
    var vsh = 1.0;
    if (vw > 0.0) { vsh = mix(1.0, cloud_sun(pl + sun * lift, meso).vis, vw); }
    if (!do_occ) {
      let lobe = shelf_lobe(cxl, sseed);
      let ub = u + shelf_bulge(cxl, sseed, Rq);
      let xc = clamp(ub / (SHELF_FACE * Rq), 0.0, 1.0);
      let f = fract(min(xc * 3.0, 2.999));
      let Ds = SHELF_D * shelf_depth(cxl, sseed);
      let ztop = shelf_top(ub, Rq, Ds);
      let z = hl - base;
      let dz = ztop - z;
      let front = smoothstep(-300.0, -60.0, ub) * (1.0 - smoothstep(120.0, 380.0, ub)) * smoothstep(-Ds + 20.0, -Ds + 140.0, z) * (1.0 - smoothstep(-Ds * (1.0 - SHELF_NOSE), -Ds * (1.0 - SHELF_NOSE) + 120.0, z));
      let face = max(smoothstep(-90.0, 0.0, dz) * (1.0 - smoothstep(60.0, 320.0, dz)), front) * (1.0 - smoothstep(-SHELF_CEIL * 1.1, -SHELF_CEIL * 0.4, z));
      let lip = smoothstep(0.45, 0.9, f);
      let fo = 2.9 * mix(0.45, 1.2, lip) * mix(0.75, 1.15, lobe) * mix(1.3, 0.75, sqrt(xc)) * mix(1.35, 1.0, smoothstep(0.0, 0.08, xc));
      let uo = 0.24 * mix(0.6, 1.5, smoothstep(-SHELF_CEIL * 0.9, -SHELF_CEIL * 1.7, z));
      occ = mix(mix(1.0, uo, smoothstep(-0.3, 0.1, dot(rd, upn))) * mix(0.45, 1.0, vsh), fo, face);
    } else {
      occ = mix(occ, 0.35, 0.75 * (1.0 - vsh)) * mix(0.45, 1.0, vsh);
    }
    var ms = 0.0;
    var a = 1.0;
    var b = 1.0;
    var c = 1.0;
    for (var k = 0; k < 4; k++) {
      ms += a * exp(-od * b) * cloud_phase(cs, c);
      a *= 0.55;
      b *= 0.38;
      c *= 0.5;
    }
    ms += 0.4 / (4.0 * PI) / (1.0 + 0.25 * od);
    ms += cloud_glow(cs, od * (1.0 + 4.0 * cb));
    let dprob = 0.08 + pow(d, remap(clamp(hn, 0.3, 0.85), 0.3, 0.85, 0.5, 2.0));
    let vprob = pow(clamp(remap(hn, 0.05, 0.2, 0.25, 1.0), 0.25, 1.0), 0.8);
    let powder = mix(1.0, clamp(dprob * vprob * 1.25, 0.0, 1.0), 0.55 * (1.0 - smoothstep(0.0, 0.6, cs)));
    let mu = dot(upn, sun);
    let sunc = sun_trans(hl, mu) * G.sun_dir.w * vsh;
    let anv_occ = mix(0.32, 0.62, smoothstep(0.70, 0.98, hn));
    occ = mix(occ, max(occ, anv_occ), anv);
    var cg = vec3f(0.0);
    if (glow_on()) { cg = city_amb(pl); }
    let amb = (sky_amb * mix(0.32, 0.96, hn) * mix(0.18, 1.0, occ) + gnd_amb * mix(1.0, 0.25, hn * (1.0 - 0.7 * anv)) * (1.0 - 0.75 * cb) * mix(0.5, 1.0, occ) + cg * (mix(1.0, 0.25, hn * (1.0 - 0.7 * anv)) + (1.0 - hn) * 0.5)) * mix(0.5, 1.0, powder);
    let moonc = moon_light(upn, hl);
    var S = ((sunc + moonc) * ms * MS_GAIN * powder + amb) * d * SIGMA;
    if (G.stormfx.z > 0.0) { S += flash_cloud(pl - oc) * (0.1 * d * SIGMA); }
    let e = d * SIGMA;
    let st_e = exp(-e * dl);
    L += T * S * (1.0 - st_e) / e;
    let wgt = T * (1.0 - st_e);
    vshw += wgt * vsh;
    litw += wgt * exp(-od);
    cw += wgt;
    tw += ts * wgt;
    ws += wgt;
    T *= st_e;
    if (T < 0.012) { T = 0.0; break; }
  }
  let cut = incloud && tb >= tscene;
  if (cw > 0.0 && cs > 0.5 && T > 0.0 && !cut) {
    let tau = -log(max(T, 1e-4));
    let lit = litw / cw;
    let p_rim = oc + rd * (tw / max(ws, 1e-6));
    let h_rim = length(p_rim) - rg;
    let sunc_rim = sun_trans(h_rim, dot(normalize(p_rim), sun)) * G.sun_dir.w * (vshw / cw);
    L += sunc_rim * (RIM * hg(cs, 0.94) * tau * exp(-tau) * (1.0 - lit));
  }
  if (ws > 0.0) {
    o.dist = vec2f((tw / ws) * 0.001, first_t * 0.001);
  }
  o.col = vec4f(L, T);
  return o;
}

// Lean ordinary-cloud march + depth-ordered storm composite (render.rs
// `p_cloud_plain_comp`): marches ordinary cumulus/stratocumulus/stratus at
// full GPU occupancy, composites the pre-marched storm layer (`t_cur`/`t_cdist`)
// and mid-level alto in depth order, applies storm-anvil shadows via band-2
// `cloud_sun`, and finishes with aerial haze and cirrus.
@fragment fn fs_plain_comp(in: FsIn) -> CloudOut {
  var o = CloudOut(vec4f(0.0, 0.0, 0.0, 1.0), vec2f(0.0));
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  let hp = vec2i(in.pos.xy);
  let q = hp * 2;
  let z4 = vec4f(textureLoad(t_depth, q, 0), textureLoad(t_depth, q + vec2i(1, 0), 0),
                 textureLoad(t_depth, q + vec2i(0, 1), 0), textureLoad(t_depth, q + vec2i(1, 1), 0));
  var z = min(min(z4.x, z4.y), min(z4.z, z4.w));
  if (((hp.x + hp.y) & 1) == 0) { z = max(max(z4.x, z4.y), max(z4.z, z4.w)); }
  var tscene = 1e9;
  if (z > 0.0) { tscene = length(ray / z); }
  o.dist.y = min(tscene * 0.001, SKY_KM);
  if (G.start.x > 0.0) { return o; }
  let rg = G.planet.w;
  let oc = -G.planet.xyz;
  if (z == 0.0) {
    let gnd = ray_sphere(oc, rd, rg);
    if (gnd.x > 0.0) { tscene = gnd.x; }
  }
  let base = G.cloud.y;
  let alt = G.up.w;
  let sun = G.sun_dir.xyz;
  let cs = dot(rd, sun);
  let jit = fract(ign(in.pos.xy) + G.time.w * 0.618034);

  let st_col = textureLoad(t_cur, hp, 0);
  let st_dst = textureLoad(t_cdist, hp, 0).rg;
  let st_L = st_col.rgb;
  var st_T = st_col.a;
  let st_t = st_dst.x * 1000.0;
  let st_entry = st_dst.y * 1000.0;

  var T = 1.0;
  var L = vec3f(0.0);
  var tw = 0.0;
  var ws = 0.0;
  var vshw = 0.0;
  var litw = 0.0;
  var cw = 0.0;
  var incloud = false;

  let mu0 = dot(G.up.xyz, sun);
  let sky_side = 0.5 * (sky_lut(normalize(G.east.xyz + G.up.xyz * 0.15)) + sky_lut(normalize(-G.east.xyz + G.up.xyz * 0.15)));
  let sky_raw = sky_lut(G.up.xyz) * 0.5 + sky_side * 0.5;
  let sky_amb = mix(sky_raw, vec3f(luma(sky_raw)), 0.28);
  var gnd_amb = vec3f(0.068, 0.072, 0.076) * sun_trans(0.0, mu0) * max(mu0, 0.0) * G.sun_dir.w / PI * mix(1.0, 0.45, G.cloud.x);
  gnd_amb *= 1.0 - 0.9 * G.stormfx.w;

  let topa = base + G.cloud.z;
  let outer = ray_sphere(oc, rd, rg + topa);
  let inner = ray_sphere(oc, rd, rg + base);
  var ta = 0.0;
  var tb = outer.y;
  var march = outer.y > 0.0;
  if (alt < base) {
    if (z == 0.0 && ray_sphere(oc, rd, rg).x > 0.0) { march = false; }
    ta = inner.y;
  } else {
    if (alt > topa) { ta = outer.x; }
    if (inner.x > 0.0) { tb = inner.x; }
  }
  tb = min(min(tb, tscene), ta + 160000.0);
  if (st_T < 0.012 && st_t > 0.0) {
    if (st_t <= ta) {
      march = false;
    } else {
      tb = min(tb, st_t + 1.0);
    }
  }
  if (tb <= ta || ta < 0.0) { march = false; }

  var ar = vec2f(-1.0);
  var aamt = 0.0;
  if (G.alto.x > 0.0) {
    ar = alto_range(oc, rd);
    ar.y = min(min(ar.y, ar.x + 60000.0), tscene);
    let ahr = rg + G.alto.y + ALTO_TOP;
    let alto_far = 250000.0 + sqrt(max((rg + alt) * (rg + alt) - ahr * ahr, 0.0));
    if (ar.x >= 0.0 && ar.x < min(tscene, alto_far) && ar.y > ar.x) { aamt = alto_amount(oc + rd * (0.5 * (ar.x + ar.y))); }
  }

  let h_sh = base + G.cloud.z + 300.0;
  if (march) {
    let qs = select(clamp(G.quality.x, 0.5, 2.0), 1.0, G.quality.x <= 0.0);
    let n_max = i32(200.0 / qs);
    var t = ta;
    var clear = 0.0;
    var pts = ta;
    var pempty = true;
    var owx = 0u;
    var twx = -1e9;
    for (var i = 0; i < n_max; i++) {
      let dt0 = clamp(t * 0.016, 40.0, 2500.0) * qs;
      var dt = dt0 * (1.0 + clear);
      if (i >= n_max * 11 / 20 && clear > 0.0) { dt = max(dt, (tb - t) / f32(max(n_max - 4 - i, 4))); }
      dt = min(dt, tb - t);
      let ts = t + dt * jit;
      t += dt;
      let atm_ = 0.5 * (ar.x + ar.y);
      let hit_a = aamt > 0.0 && ts > atm_;
      let hit_s = st_T < 1.0 && ts > st_t;
      if (hit_s && (!hit_a || st_t <= atm_)) {
        L += T * st_L;
        let wst = T * (1.0 - st_T);
        tw += st_t * wst;
        ws += wst;
        T *= st_T;
        st_T = 1.0;
        if (T < 0.012) { T = 0.0; break; }
      }
      if (hit_a) {
        let la = alto_march(oc, rd, ar.x, ar.y, aamt, jit, cs, sky_amb, gnd_amb + city_amb(oc + rd * atm_));
        aamt = 0.0;
        L += T * la.rgb;
        let wa = T * (1.0 - la.a);
        tw += atm_ * wa;
        ws += wa;
        T *= la.a;
        if (T < 0.012) { T = 0.0; break; }
      }
      if (st_T < 1.0 && ts > st_t) {
        L += T * st_L;
        let wst = T * (1.0 - st_T);
        tw += st_t * wst;
        ws += wst;
        T *= st_T;
        st_T = 1.0;
        if (T < 0.012) { T = 0.0; break; }
      }
      if (ts >= tb) { break; }
      let p = oc + rd * ts;
      let h = length(p) - rg;
      let hs = h - base;
      if (ts - twx >= 400.0) {
        owx = cloud_ord_pack(p, max(hs, 0.0));
        twx = ts;
      }
      let u = unpack4x8unorm(owx);
      var cov = clamp(u.x * 2.0 - 0.5, 0.0, 1.0);
      if (st_entry > 0.0) { cov *= 1.0 - smoothstep(2600.0, 200.0, abs(ts - st_entry)); }
      let ow = vec4f(cov, u.y, u.z, u.w);
      let fine = 1.0 - smoothstep(1500.0, 4500.0, ts);
      g_erode = 1.0 - smoothstep(12000.0, 45000.0, ts);
      let d = cloud_shape_plain(p, h, ow, true, fine);
      if (d <= 0.002) {
        incloud = false;
        pempty = true;
        pts = ts;
        let clear_hi = hs > G.cloud.z * ow.z + 300.0 || ow.x <= 0.02;
        clear = min(clear + select(0.3, 0.5, clear_hi), select(1.4, 3.0, clear_hi));
        continue;
      }
      clear = 0.0;
      incloud = true;
      var pl = p;
      var hl = h;
      var dl = dt;
      if (pempty && dt > 40.0) {
        var lo = pts;
        var hi = ts;
        for (var k = 0; k < select(3, 5, dt > 600.0); k++) {
          let tm = 0.5 * (lo + hi);
          let pm = oc + rd * tm;
          if (cloud_shape_plain(pm, length(pm) - rg, ow, true, fine) > 0.002) { hi = tm; } else { lo = tm; }
        }
        pl = oc + rd * min(hi + 40.0, ts);
        hl = length(pl) - rg;
        dl = clamp(t - hi, 0.5 * (t - ts), dt);
      }
      pempty = false;
      pts = ts;
      var od = 0.0;
      let hc_l = max(hl, 0.0);
      let sun_lit = G.sun_dir.w > 0.0 && dot(pl, sun) > -sqrt(hc_l * (2.0 * rg + hc_l)) - 0.007 * (rg + hc_l);
      if (sun_lit || G.moon.w > 0.0) {
        let farl = ts > 30000.0;
        let deep_t = T < 0.14;
        var ls = select(select(36.0, 65.0, deep_t), 90.0, farl);
        var lt = 0.0;
        let nl = select(select(6, 7, G.cloud.z * ow.z > 2500.0), 4, farl || deep_t);
        let lg = select(select(1.85, 2.3, deep_t), 2.5, farl);
        let jl = 0.5 + 0.4 * (jit - 0.5);
        let ldet = !farl && !deep_t;
        for (var j = 0; j < nl; j++) {
          let lp = pl + sun * (lt + ls * jl);
          let lh = length(lp) - rg;
          if (od >= 1800.0 || (dot(pl, sun) > 0.0 && lh >= topa)) { break; }
          g_mean_erode = ldet && j == 1;
          od += cloud_shape_plain(lp, lh, ow, ldet && j == 0, 0.0) * ls;
          lt += ls;
          ls *= lg;
        }
        g_mean_erode = false;
        od *= SIGMA;
      }
      let upn = normalize(pl);
      let hn = clamp((hl - base) / max(G.cloud.z * ow.z, 1.0), 0.0, 1.0);
      let od_up = cloud_shape_plain(pl + upn * 450.0, hl + 450.0, ow, false, 0.0) * 900.0 * SIGMA;
      let lift = max(h_sh - hl, 0.0) / max(dot(upn, sun), 0.1);
      let vsh = cloud_sun(pl + sun * lift, ts < 15000.0).vis;
      var occ = mix(1.0 / (1.0 + 0.15 * od_up), 1.0, 0.12);
      occ = mix(occ, 0.45, 0.55 * (1.0 - vsh)) * mix(0.65, 1.0, vsh);
      var ms = 0.0;
      var a = 1.0;
      var b = 1.0;
      var c = 1.0;
      for (var k = 0; k < 4; k++) {
        ms += a * exp(-od * b) * cloud_phase(cs, c);
        a *= 0.55;
        b *= 0.38;
        c *= 0.5;
      }
      ms += 0.4 / (4.0 * PI) / (1.0 + 0.25 * od);
      ms += cloud_glow(cs, od);
      let dprob = 0.08 + pow(d, remap(clamp(hn, 0.3, 0.85), 0.3, 0.85, 0.5, 2.0));
      let vprob = pow(clamp(remap(hn, 0.05, 0.2, 0.25, 1.0), 0.25, 1.0), 0.8);
      let powder = mix(1.0, clamp(dprob * vprob * 1.25, 0.0, 1.0), 0.55 * (1.0 - smoothstep(0.0, 0.6, cs)));
      let mu = dot(upn, sun);
      let sunc = sun_trans(hl, mu) * G.sun_dir.w * vsh;
      var cg = vec3f(0.0);
      if (glow_on()) { cg = city_amb(pl); }
      let amb = (sky_amb * mix(0.32, 0.96, hn) * mix(0.18, 1.0, occ) + gnd_amb * mix(1.0, 0.25, hn) * mix(0.5, 1.0, occ) + cg * (mix(1.0, 0.25, hn) + (1.0 - hn) * 0.5)) * mix(0.5, 1.0, powder);
      let moonc = moon_light(upn, hl);
      var S = ((sunc + moonc) * ms * MS_GAIN * powder + amb) * d * SIGMA;
      if (G.stormfx.z > 0.0) { S += flash_cloud(pl - oc) * (0.1 * d * SIGMA); }
      let e = d * SIGMA;
      let st_e = exp(-e * dl);
      L += T * S * (1.0 - st_e) / e;
      let wgt = T * (1.0 - st_e);
      vshw += wgt * vsh;
      litw += wgt * exp(-od);
      cw += wgt;
      tw += ts * wgt;
      ws += wgt;
      T *= st_e;
      if (T < 0.012) { T = 0.0; break; }
    }
    let cut = incloud && tb >= tscene;
    if (cw > 0.0 && cs > 0.5 && T > 0.0 && !cut) {
      let tau = -log(max(T, 1e-4));
      let lit = litw / cw;
      let p_rim = oc + rd * (tw / max(ws, 1e-6));
      let h_rim = length(p_rim) - rg;
      let sunc_rim = sun_trans(h_rim, dot(normalize(p_rim), sun)) * G.sun_dir.w * (vshw / cw);
      L += sunc_rim * (RIM * hg(cs, 0.94) * tau * exp(-tau) * (1.0 - lit));
    }
  }
  let atm_post = 0.5 * (ar.x + ar.y);
  let has_a = aamt > 0.0;
  let has_s = st_T < 1.0;
  if (has_s && (!has_a || st_t <= atm_post) && T > 0.012) {
    L += T * st_L;
    let wst = T * (1.0 - st_T);
    tw += st_t * wst;
    ws += wst;
    T *= st_T;
    st_T = 1.0;
  }
  if (has_a && T > 0.012) {
    let la = alto_march(oc, rd, ar.x, ar.y, aamt, jit, cs, sky_amb, gnd_amb + city_amb(oc + rd * atm_post));
    L += T * la.rgb;
    let wa = T * (1.0 - la.a);
    tw += atm_post * wa;
    ws += wa;
    T *= la.a;
  }
  if (st_T < 1.0 && T > 0.012) {
    L += T * st_L;
    let wst = T * (1.0 - st_T);
    tw += st_t * wst;
    ws += wst;
    T *= st_T;
  }
  if (ws > 0.0) {
    let tc = tw / ws;
    o.dist.x = tc * 0.001;
    let top = ray_sphere(oc, rd, rg + ATM_H);
    let sc = atm_march(oc, rd, max(top.x, 0.0), tc, 6, jit);
    L = L * sc.T + sc.L * (1.0 - T);
  }
  if (T > 0.01) {
    let ci = ray_sphere(oc, rd, rg + CIRRUS_H);
    var tci = -1.0;
    if (alt < CIRRUS_H) { tci = ci.y; } else if (ci.x > 0.0 && ray_sphere(oc, rd, rg).x < 0.0) { tci = ci.x; }
    if (tci > 0.0 && tci < tscene && tci < 400000.0) {
      let p = oc + rd * tci;
      let r = length(p);
      let upn = p / r;
      let slant = 1.0 / max(abs(dot(rd, upn)), 0.08);
      let tau = cirrus(p) * slant * 0.35;
      if (tau > 0.001) {
        let mu = dot(upn, sun);
        let sunc = sun_trans(CIRRUS_H, mu) * G.sun_dir.w;
        let ph = mix(hg(cs, 0.75), hg(cs, -0.1), 0.35) + 0.04;
        var Lc = ((sunc + moon_light(upn, CIRRUS_H)) * ph + sky_lut(G.up.xyz) * 0.5 + gnd_amb * 0.3) * (1.0 - exp(-tau)) * 0.95;
        let ac = 1.0 - exp(-tau);
        let top = ray_sphere(oc, rd, rg + ATM_H);
        let sc = atm_march(oc, rd, max(top.x, 0.0), tci, 4, jit);
        Lc = Lc * sc.T + sc.L * ac;
        L += T * Lc;
        if (ws <= 0.0) { o.dist.x = tci * 0.001; }
        T *= 1.0 - ac;
      }
    }
  }
  o.col = vec4f(L, T);
  return o;
}

// Early-Z pixel classification + storm_mask cache for storm skies (render.rs
// `p_cloud_classify`): writes 0.0 where the ray never enters any 3D storm cloud
// volume (routed to lean `fs_plain` via depth Greater against z = 0.5) and 1.0
// where it does (routed to `fs_storm` via depth Less), while packing smask,
// rmask, s_start, s_end, storm_top, and sub-cloud drop into `cloud_cls_data`.
struct ClsOut {
  @location(0) data: vec4<u32>,
  @builtin(frag_depth) depth: f32,
};
@vertex fn vs_full_z05(@builtin(vertex_index) i: u32) -> FsIn {
  let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  return FsIn(vec4f(p * 2.0 - 1.0, 0.5, 1.0), vec2f(p.x, 1.0 - p.y));
}
@fragment fn fs_classify(in: FsIn) -> ClsOut {
  var out = ClsOut(vec4<u32>(0u), 0.0);
  if (G.start.x > 0.0) { return out; }
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  let hp = vec2i(in.pos.xy);
  let q = hp * 2;
  let z4 = vec4f(textureLoad(t_depth, q, 0), textureLoad(t_depth, q + vec2i(1, 0), 0),
                 textureLoad(t_depth, q + vec2i(0, 1), 0), textureLoad(t_depth, q + vec2i(1, 1), 0));
  var z = min(min(z4.x, z4.y), min(z4.z, z4.w));
  if (((hp.x + hp.y) & 1) == 0) { z = max(max(z4.x, z4.y), max(z4.z, z4.w)); }
  var tscene = 1e9;
  if (z > 0.0) { tscene = length(ray / z); }
  let rg = G.planet.w;
  let oc = -G.planet.xyz;
  if (z == 0.0) {
    let gnd = ray_sphere(oc, rd, rg);
    if (gnd.x > 0.0) { tscene = gnd.x; }
  }
  let smask = storm_mask(oc, rd, min(tscene, 320000.0));
  let rmask = g_rain_mask;
  let base = G.cloud.y;
  let top_u = u32(clamp(ceil(g_storm_top), 0.0, 65535.0));
  let drop_u = select(0u, u32(clamp(ceil(base - g_low_bot), 1.0, 65535.0)), g_storm_low);
  out.data = vec4<u32>(
    (smask & 0xffffu) | ((rmask & 0xffffu) << 16u),
    bitcast<u32>(g_storm_start),
    bitcast<u32>(g_storm_end),
    top_u | (drop_u << 16u),
  );
  out.depth = select(0.0, 1.0, smask != 0u);
  return out;
}

// Sub-cloud storm rain shaft composite pass (render.rs `p_cloud_rain`): runs
// after `fs_plain` + `fs_storm` so `storm_rain` is never compiled into the
// heavy cloud march shader and rain-only rays stay on `fs_plain`.
@fragment fn fs_storm_rain(in: FsIn) -> CloudOut {
  let hp = vec2i(in.pos.xy);
  let cur = textureLoad(t_cur, hp, 0);
  let cd = textureLoad(t_cdist, hp, 0).rg;
  var o = CloudOut(cur, cd);
  if (G.start.x > 0.0) { return o; }
  let cls = textureLoad(t_cls_data, hp, 0);
  let rmask = cls.x >> 16u;
  if (rmask == 0u) { return o; }
  let base = G.cloud.y;
  let alt = G.up.w;
  let storm_low = (cls.w >> 16u) > 0u;
  if (alt >= base && !storm_low && cur.a <= 0.012) { return o; }
  let ray = view_ray(in.uv);
  let rd = normalize(ray);
  let q = hp * 2;
  let z4 = vec4f(textureLoad(t_depth, q, 0), textureLoad(t_depth, q + vec2i(1, 0), 0),
                 textureLoad(t_depth, q + vec2i(0, 1), 0), textureLoad(t_depth, q + vec2i(1, 1), 0));
  var z = min(min(z4.x, z4.y), min(z4.z, z4.w));
  if (((hp.x + hp.y) & 1) == 0) { z = max(max(z4.x, z4.y), max(z4.z, z4.w)); }
  var tscene = 1e9;
  if (z > 0.0) { tscene = length(ray / z); }
  let rg = G.planet.w;
  let oc = -G.planet.xyz;
  if (z == 0.0) {
    let gnd = ray_sphere(oc, rd, rg);
    if (gnd.x > 0.0) { tscene = gnd.x; }
  }
  let s_base = ray_sphere(oc, rd, rg + base);
  let jit = fract(ign(in.pos.xy) + G.time.w * 0.618034);
  let sun = G.sun_dir.xyz;
  let mu0 = dot(G.up.xyz, sun);
  let sky_side = 0.5 * (sky_lut(normalize(G.east.xyz + G.up.xyz * 0.15)) + sky_lut(normalize(-G.east.xyz + G.up.xyz * 0.15)));
  let sky_raw = sky_lut(G.up.xyz) * 0.5 + sky_side * 0.5;
  let sky_amb = mix(sky_raw, vec3f(luma(sky_raw)), 0.28);
  var gnd_amb = vec3f(0.068, 0.072, 0.076) * sun_trans(0.0, mu0) * max(mu0, 0.0) * G.sun_dir.w / PI * mix(1.0, 0.45, G.cloud.x);
  gnd_amb *= 1.0 - 0.9 * G.stormfx.w;
  let sr = storm_rain(oc, rd, min(tscene, 320000.0), rmask, s_base, jit, sky_amb, gnd_amb);
  if (sr.T >= 0.9999) { return o; }
  let top = ray_sphere(oc, rd, rg + ATM_H);
  let sc = atm_march(oc, rd, max(top.x, 0.0), sr.t, 4, jit);
  let rL = sr.L * sc.T + sc.L * (1.0 - sr.T);
  let front = cd.x <= 0.0 || sr.t <= cd.x * 1000.0;
  let L = select(cur.rgb + cur.a * rL, rL + sr.T * cur.rgb, front);
  let T = cur.a * sr.T;
  let wc = select(1.0 - cur.a, sr.T * (1.0 - cur.a), front) * select(0.0, 1.0, cd.x > 0.0);
  let wr = select(cur.a * (1.0 - sr.T), 1.0 - sr.T, front);
  o.col = vec4f(L, T);
  if (wc + wr > 0.0) { o.dist.x = (cd.x * wc + sr.t * 0.001 * wr) / (wc + wr); }
  return o;
}

// Temporal accumulation: blend this frame's noisy march into the reprojected
// history, clamped to the current neighbourhood so moving clouds don't ghost.
@group(1) @binding(2) var t_cur: texture_2d<f32>;
@group(1) @binding(3) var t_cdist: texture_2d<f32>;
@group(1) @binding(4) var t_hist: texture_2d<f32>;
@fragment fn fs_temporal(in: FsIn) -> @location(0) vec4f {
  let dims = vec2i(textureDimensions(t_cur));
  let px = vec2i(in.pos.xy);
  let cur = textureLoad(t_cur, px, 0);
  let cd = textureLoad(t_cdist, px, 0).rg;
  // Neighbourhood statistics over texels on the same surface only: across a
  // silhouette the clear texels in front would otherwise widen the clamp and let
  // the history bleed cloud onto the plane (and clear sky into the cloud).
  // Behind cloud this texel sees as (nearly) opaque, surfaces well beyond it
  // count as one: at the horizon the texels marching to the ground and those
  // marching to the sky see the same cloud, but each got half a neighbourhood,
  // a tighter clamp and less smoothing, which drew a dotted seam right across
  // storm towers. (Not through thin cloud: what lies behind it shows.)
  let cap = select(1e9, cd.x * 3.0, cd.x > 0.0 && cur.a < 0.15);
  var m1 = vec4f(0.0);
  var m2 = vec4f(0.0);
  var n = 0.0;
  var mn = cur;
  var mx = cur;
  for (var y = -1; y <= 1; y++) {
    for (var x = -1; x <= 1; x++) {
      let c = clamp(px + vec2i(x, y), vec2i(0), dims - 1);
      if (depth_match(min(textureLoad(t_cdist, c, 0).g, cap), min(cd.y, cap)) < 0.5) { continue; }
      let s = textureLoad(t_cur, c, 0);
      m1 += s;
      m2 += s * s;
      n += 1.0;
      mn = min(mn, s);
      mx = max(mx, s);
    }
  }
  let mean = m1 / n;
  let sd = sqrt(max(m2 / n - mean * mean, vec4f(0.0)));
  let lo = max(mn, mean - sd * 1.6);
  let hi = min(mx, mean + sd * 1.6);
  if (G.cam_delta.w < 0.5) { return cur; }
  // Reproject through the distance of the cloud this pixel sees.
  var dist = cd.x * 1000.0;
  if (dist <= 0.0) { dist = 60000.0; }
  let P = normalize(view_ray(in.uv)) * dist;
  let pc = G.prev_vp * vec4f(P + G.cam_delta.xyz, 1.0);
  if (pc.w <= 0.0) { return cur; }
  let puv = pc.xy / pc.w * vec2f(0.5, -0.5) + 0.5;
  if (any(puv < vec2f(0.0)) || any(puv > vec2f(1.0))) { return cur; }
  let hist = clamp(textureSampleLevel(t_hist, s_clamp, puv, 0.0), lo, hi);
  return mix(hist, cur, 0.1);
}

// Upsample onto the scene: out = cloud + scene * transmittance (done by the blend state).
// Joint bilateral: the four nearest texels are weighted by bilinear position and by
// how well the scene distance they marched to matches the wanted one, falling back to
// the best-matching texel when none agrees.
@group(1) @binding(1) var t_cloud: texture_2d<f32>;
@group(1) @binding(5) var t_color: texture_2d<f32>;
fn cloud_at(pos: vec2f, d: f32) -> vec4f {
  let dims = vec2i(textureDimensions(t_cloud));
  let f = pos * 0.5 - 0.5;
  let b = vec2i(floor(f));
  let fr = f - floor(f);
  var acc = vec4f(0.0);
  var wsum = 0.0;
  var best = vec4f(0.0, 0.0, 0.0, 1.0);
  var bestw = -1.0;
  for (var j = 0; j < 2; j++) {
    for (var i = 0; i < 2; i++) {
      let c = clamp(b + vec2i(i, j), vec2i(0), dims - 1);
      let s = textureLoad(t_cloud, c, 0);
      let m = depth_match(textureLoad(t_cdist, c, 0).g, d);
      let bw = select(1.0 - fr.x, fr.x, i == 1) * select(1.0 - fr.y, fr.y, j == 1);
      if (m > bestw) { bestw = m; best = s; }
      acc += s * (bw * m);
      wsum += bw * m;
    }
  }
  return (acc + best * 0.01) / (wsum + 0.01);
}
@fragment fn fs_apply(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let z = textureLoad(t_depth, px, 0);
  // The composite filled the part of an anti-aliased edge that geometry does not
  // cover (1 - alpha) with sky; that part needs the clouds in front of the sky,
  // the covered part those in front of the geometry.
  let cov = textureLoad(t_color, px, 0).a;
  if (z == 0.0 || cov < 0.004) { return cloud_at(in.pos.xy, SKY_KM); }
  let ray = view_ray(in.uv);
  let cg = cloud_at(in.pos.xy, min(length(ray / z) * 0.001, SKY_KM));
  if (cov > 0.996) { return cg; }
  let cs = cloud_at(in.pos.xy, SKY_KM);
  // Same sky (and jitter) the composite blended in. With dst = cov*G + (1-cov)*S,
  // dst*cg.a + L = cov*(G*cg.a + cg.rgb) + (1-cov)*(S*cs.a + cs.rgb).
  let S = clamp(sky_color(normalize(ray), ign(in.pos.xy)), vec3f(0.0), vec3f(3000.0));
  return vec4f(cov * cg.rgb + (1.0 - cov) * (cs.rgb + S * (cs.a - cg.a)), cg.a);
}

// Cloud shadow map (render.rs CloudShadow, read by cloud_sun): per texel, a ray along
// the sun from a point on the cloud base, with the optical depth it crosses in three
// height bands (cumulative) and how closed the deck is there. The map being drawn
// is G.cs[6..12] (see cloud_sun).
fn cs_band(h: f32) -> vec3f {
  let f = sqrt(clamp((h - G.cs[9].x) / (G.cs[9].y - G.cs[9].x), 0.0, 1.0)) * 3.0;
  return select(select(vec3f(0.0, 0.0, 1.0), vec3f(0.0, 1.0, 0.0), f < 2.0), vec3f(1.0, 0.0, 0.0), f < 1.0);
}
fn cshadow_ray(px: vec2f, ext: f32) -> vec4f {
  // Shadows of the clouds as drawn (eroded), not of their whole envelopes.
  g_mean_erode = true;
  let base = G.cs[9].x;
  let rg = G.planet.w;
  let R = rg + base;
  let loc = (px / CS_RES * 2.0 - 1.0) * ext + G.cs[9].zw;
  let qn = normalize(G.cs[6].xyz + (G.cs[7].xyz * loc.x + G.cs[8].xyz * loc.y) / R);
  // The clouds as they were when this map was begun: its later slices are drawn
  // while the clouds drift on, and the lookups make up for the drift since.
  let dq = G.cs[11].xyz - G.weather.xyz;
  let q = qn * R + dq;
  let sun = G.cs[10].xyz;
  var closed = smoothstep(0.45, 0.95, cloud_weather_m(q, 0u).cov);
  if (dot(qn, sun) <= 0.0) { return vec4f(0.0, 0.0, 0.0, closed); }
  var b = vec3f(0.0);
  // Only rays that pass near a storm cell climb to the storm ceiling.
  let smask = storm_mask(q, sun, 120000.0);
  let s_start = g_storm_start;
  let s_end = g_storm_end;
  let lid = select(base + G.cloud.z, clamp(g_storm_top, base + G.cloud.z, G.cloud.w), smask != 0u);
  let ord_y = ray_sphere(q, sun, rg + base + G.cloud.z).y;
  var tl = min(ray_sphere(q, sun, rg + lid).y, 120000.0);
  if (smask != 0u) { tl = min(tl, max(ord_y, s_end)); }
  let stq = storm_tier();
  let n_lo = select(select(24.0, 32.0, stq >= 3), 20.0, stq <= 1);
  let n_hi = select(select(44.0, 56.0, stq >= 3), 36.0, stq <= 1);
  let n_div = select(select(360.0, 300.0, stq >= 3), 450.0, stq <= 1);
  let n = i32(clamp(tl / n_div, n_lo, n_hi));
  let nf = f32(n);
  let jit = ign(px);
  const K = 0.2;
  let ts = tl / (1.0 + K);
  // The column weather changes slowly along the ray: reuse ordinary weather
  // across short steps (< 400 m) and update storm membership every other step.
  var wx = no_weather();
  var owx = 0u;
  var twx = -1e9;
  var skip_s = false;
  for (var i = 0; i < n; i++) {
    let u = (f32(i) + jit) / nf;
    let tc = ts * u * (u + K);
    let p = q + sun * tc;
    let h = length(p) - rg;
    let hs = h - base;
    let in_s = smask != 0u && tc >= s_start && tc <= s_end;
    if (hs >= G.cloud.z && !in_s) {
      if ((i & 1) == 0) { wx = no_weather(); skip_s = false; }
      continue;
    }
    if ((i & 1) == 0) {
      let uw = (f32(i) + 1.0) / nf;
      let twc = ts * uw * (uw + K);
      let pw = q + sun * twc;
      let hsw = length(pw) - rg - base;
      if (hsw >= 0.0 && hsw < G.cloud.z && (twc - twx >= 400.0)) {
        owx = cloud_ord_pack(pw, hsw);
        twx = twc;
      }
      let sm = select(0u, smask, smask != 0u && twc >= s_start && twc <= s_end);
      wx = cloud_weather_unpack(pw, hsw, sm, owx);
      let anv_b = g_anvil;
      let s_reach = mix(0.56, 0.16, anv_b) * storm_bulge(vec4f(0.0, 0.0, 0.0, g_storm_r)) + mix(320.0, 200.0, anv_b);
      // Allow margin for the two steps (i and i+1) that share this weather lookup.
      let step_m = ts * (2.0 * uw + K) / nf;
      skip_s = wx.ci >= 0 && g_storm_sd <= -(s_reach + step_m) && (wx.ns == 0.0 || g_storm_sd <= -(DECK_SWELL_D + step_m));
    }
    g_skip_storm = skip_s;
    let d = cloud_shape(p, h, wx, false);
    g_skip_storm = false;
    if (d > 0.0) { b += cs_band(h) * (d * ts * (2.0 * u + K) / nf); }
  }
  // The mid-level layer (which drifts half as fast again): one crossing of its lens.
  if (G.alto.x > 0.0) {
    let pa = q + sun * ray_sphere(q, sun, rg + G.alto.y).y + dq * 0.5;
    let aa = alto_amount(pa);
    if (aa > 0.0) {
      let ln = alto_lens(pa, aa, 0.0);
      b += cs_band(G.alto.y) * (ln.z * (ln.y - ln.x) / max(dot(normalize(pa), sun), 0.08));
      closed = max(closed, ln.w * smoothstep(0.0, 300.0, ln.y - ln.x) * 0.85);
    }
  }
  b *= SIGMA;
  return vec4f(b.x, b.x + b.y, b.x + b.y + b.z, closed);
}
@fragment fn fs_cshadow_in(in: FsIn) -> @location(0) vec4f { return cshadow_ray(in.pos.xy, G.cs[6].w); }
@fragment fn fs_cshadow_out(in: FsIn) -> @location(0) vec4f { return cshadow_ray(in.pos.xy, G.cs[7].w); }

// Specialized cloud shadow ray for zero-storm, zero-rain-deck skies.
fn cshadow_ray_plain(px: vec2f, ext: f32) -> vec4f {
  g_mean_erode = true;
  let base = G.cs[9].x;
  let rg = G.planet.w;
  let R = rg + base;
  let loc = (px / CS_RES * 2.0 - 1.0) * ext + G.cs[9].zw;
  let qn = normalize(G.cs[6].xyz + (G.cs[7].xyz * loc.x + G.cs[8].xyz * loc.y) / R);
  let dq = G.cs[11].xyz - G.weather.xyz;
  let q = qn * R + dq;
  let sun = G.cs[10].xyz;
  let u0 = unpack4x8unorm(cloud_ord_pack(q, 0.0));
  var closed = smoothstep(0.45, 0.95, clamp(u0.x * 2.0 - 0.5, 0.0, 1.0));
  if (dot(qn, sun) <= 0.0) { return vec4f(0.0, 0.0, 0.0, closed); }
  var b = vec3f(0.0);
  let tl = min(ray_sphere(q, sun, rg + base + G.cloud.z).y, 120000.0);
  let n = i32(clamp(tl / 300.0, 32.0, 56.0));
  let nf = f32(n);
  let jit = ign(px);
  const K = 0.2;
  let ts = tl / (1.0 + K);
  var ow = vec4f(clamp(u0.x * 2.0 - 0.5, 0.0, 1.0), u0.y, u0.z, u0.w);
  var twx = 0.0;
  for (var i = 0; i < n; i++) {
    let u = (f32(i) + jit) / nf;
    let tc = ts * u * (u + K);
    let p = q + sun * tc;
    let h = length(p) - rg;
    let hs = h - base;
    if ((i & 1) == 0 && (tc - twx >= 400.0)) {
      let uw = (f32(i) + 1.0) / nf;
      let twc = ts * uw * (uw + K);
      let pw = q + sun * twc;
      let hsw = max(length(pw) - rg - base, 0.0);
      let upk = unpack4x8unorm(cloud_ord_pack(pw, hsw));
      ow = vec4f(clamp(upk.x * 2.0 - 0.5, 0.0, 1.0), upk.y, upk.z, upk.w);
      twx = twc;
    }
    let d = cloud_shape_plain(p, h, ow, false, 0.0);
    if (d > 0.0) { b += cs_band(h) * (d * ts * (2.0 * u + K) / nf); }
  }
  if (G.alto.x > 0.0) {
    let pa = q + sun * ray_sphere(q, sun, rg + G.alto.y).y + dq * 0.5;
    let aa = alto_amount(pa);
    if (aa > 0.0) {
      let ln = alto_lens(pa, aa, 0.0);
      b += cs_band(G.alto.y) * (ln.z * (ln.y - ln.x) / max(dot(normalize(pa), sun), 0.08));
      closed = max(closed, ln.w * smoothstep(0.0, 300.0, ln.y - ln.x) * 0.85);
    }
  }
  b *= SIGMA;
  return vec4f(b.x, b.x + b.y, b.x + b.y + b.z, closed);
}
@fragment fn fs_cshadow_in_plain(in: FsIn) -> @location(0) vec4f { return cshadow_ray_plain(in.pos.xy, G.cs[6].w); }
@fragment fn fs_cshadow_out_plain(in: FsIn) -> @location(0) vec4f { return cshadow_ray_plain(in.pos.xy, G.cs[7].w); }
