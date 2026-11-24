// Sky as seen through a pixel with no geometry: atmosphere, sun, stars, moon.
// Shared by the composite (sky pixels) and the cloud upsample (anti-aliased edges).

// The star field turns with the sun (near enough to sidereal for a flight).
fn celestial(rd: vec3f) -> vec3f {
  let s = normalize(G.sun_dir.xy + vec2f(1e-6, 0.0));
  return vec3f(s.x * rd.x + s.y * rd.y, -s.y * rd.x + s.x * rd.y, rd.z);
}
// One layer of stars: at most one per cell of a 3D grid of n cells per radian,
// each drawn as a small gaussian at least a pixel wide so it never aliases.
fn star_layer(q: vec3f, n: f32, dens: f32, seed: f32) -> vec3f {
  let p = q * n + seed;
  let c = floor(p);
  if (hash3(c) > dens) { return vec3f(0.0); }
  let off = vec3f(hash3(c + 1.3), hash3(c + 2.7), hash3(c + 4.1)) * 0.6 + 0.2;
  let d = length(fract(p) - off) / n;
  let sig = G.misc.w * 0.7;
  let g = exp(-d * d / (2.0 * sig * sig));
  if (g < 0.002) { return vec3f(0.0); }
  let hb = hash3(c + 9.0);
  let ht = hash3(c + 5.5);
  // Mostly faint ones, a few bright; blue-white through to orange giants.
  let b = exp2(-hb * 7.5);
  let col = mix(vec3f(0.62, 0.76, 1.0), vec3f(1.0, 0.78, 0.52), ht * ht) / mix(0.75, 0.82, ht * ht);
  let tw = 1.0 + 0.35 * sin(G.time.x * (7.0 + 9.0 * ht) + hb * 60.0) * sin(G.time.x * (3.1 + 4.0 * hb) + ht * 30.0);
  return col * b * g * tw;
}
fn stars(rd: vec3f) -> vec3f {
  let q = celestial(rd);
  var c = star_layer(q, 58.0, 0.075, 0.0) + star_layer(q.zxy, 41.0, 0.06, 17.3) * 0.8;
  // The Milky Way: a faint, blotchy band with dark dust lanes down its middle.
  let gp = normalize(vec3f(-0.42, 0.55, 0.72));
  let lat = dot(q, gp);
  let band = exp(-lat * lat / 0.03);
  if (band > 0.01 && (night_cam() > 0.0 || G.up.w > 15000.0)) {
    let n1 = textureSampleLevel(t_noise, s_repeat, q * 1.7 + vec3f(0.2, 0.5, 0.1), 0.0);
    let n2 = textureSampleLevel(t_noise, s_repeat, q * 6.5 + vec3f(0.7, 0.3, 0.9), 0.0);
    let core = exp(-pow(length(q - normalize(vec3f(0.62, 0.71, -0.33))), 2.0) / 0.35);
    let dust = smoothstep(0.45, 0.75, n2.g * 0.6 + n1.b * 0.5) * exp(-lat * lat / 0.004);
    let glow = band * (0.45 + 0.9 * n1.r * n2.r + 1.4 * core) * (1.0 - 0.75 * dust);
    c += vec3f(0.85, 0.88, 1.0) * glow * 0.0065;
    // Unresolved stars crowd the band.
    c += star_layer(q.yzx, 97.0, 0.12 * band, 5.1) * 0.35;
  }
  return c;
}
// Deep space for the start screen, where the planet hangs in the dark: just
// stars, denser than the night sky shows, with no Milky Way or nebulae.
// Display-ready (the night sky scales `stars` down behind the atmosphere).
fn space(rd: vec3f) -> vec3f {
  let q = celestial(rd);
  var c = (star_layer(q, 58.0, 0.075, 0.0) + star_layer(q.zxy, 41.0, 0.06, 17.3) * 0.8) * 0.2;
  // Faint dust of stars too far to pick out one by one.
  c += star_layer(q.yxz, 150.0, 0.14, 31.7) * 0.05 + star_layer(q.zyx, 240.0, 0.12, 47.1) * 0.025;
  return c;
}
// The moon's disc: lit by the sun so it shows its phase, with darker maria.
fn moon_disc(rd: vec3f) -> vec3f {
  let m = G.moon.xyz;
  let R = 0.0058;
  if (dot(rd, m) < 1.0 - R * R * 0.75) { return vec3f(0.0); }
  let ex = normalize(cross(vec3f(0.0, 0.0, 1.0), m) + vec3f(1e-6, 0.0, 0.0));
  let ey = cross(m, ex);
  let q = vec2f(dot(rd, ex), dot(rd, ey)) / R;
  let r = length(q);
  let edge = clamp((1.0 - r) * R / G.misc.w + 0.5, 0.0, 1.0);
  if (edge <= 0.0) { return vec3f(0.0); }
  let z = sqrt(max(1.0 - r * r, 0.0));
  let nrm = ex * q.x + ey * q.y - m * z;
  let lit = smoothstep(-0.02, 0.1, dot(nrm, G.sun_dir.xyz));
  let mp = vec3f(q, z);
  let a = textureSampleLevel(t_noise, s_repeat, mp * 0.38 + vec3f(0.31, 0.12, 0.71), 0.0);
  let b = textureSampleLevel(t_noise, s_repeat, mp * 1.9 + vec3f(0.2, 0.6, 0.4), 0.0);
  let maria = smoothstep(0.52, 0.66, a.r * 0.8 + a.g * 0.3);
  let alb = (1.0 - 0.42 * maria) * (0.86 + 0.28 * b.g) * (0.82 + 0.18 * z);
  return vec3f(1.0, 0.97, 0.92) * alb * (lit + 0.012) * edge * G.moon.w * 8.0;
}
// How dark it is at the camera (1 = full night).
fn night_cam() -> f32 { return 1.0 - smoothstep(-0.2, 0.05, dot(G.up.xyz, G.sun_dir.xyz)); }

fn sky_color(rd: vec3f, jit: f32) -> vec3f {
  let gk = globe_k();
  var globe = vec3f(0.0);
  if (gk > 0.0) {
    globe = globe_sky(rd, fract(jit + G.time.w * 0.618034));
    if (gk >= 1.0) { return globe; }
  }
  let oc = -G.planet.xyz;
  let sun = G.sun_dir.xyz;
  // Fresh jitter every frame: cloud shadows cross the march (light shafts), and the
  // temporal AA smooths what the steps miss.
  let sc = sky_march(rd, 24, fract(jit + G.time.w * 0.618034));
  var col = sc.L;
  let gnd = ray_sphere(oc, rd, G.planet.w);
  if (gnd.x > 0.0) {
    // Bare planet where no tile is drawn yet: a plain lit ocean.
    let p = oc + rd * gnd.x;
    let mu = dot(normalize(p), sun);
    let mum = dot(normalize(p), G.moon.xyz);
    col += sc.T * vec3f(0.012, 0.035, 0.07) * ((sun_trans(0.0, mu) * max(mu, 0.0) + 0.02) * G.sun_dir.w + moon_light(normalize(p), 0.0) * max(mum, 0.0)) / PI;
  } else {
    let cs = dot(rd, sun);
    col += sc.T * smoothstep(0.999978, 0.999990, cs) * G.sun_dir.w * 600.0;
    col += sc.T * (select(stars(rd) * 0.11, space(rd), G.start.x > 0.0) + moon_disc(rd));
    let el = dot(rd, G.up.xyz);
    // Airglow and scattered starlight: even a moonless sky is deep blue, not black.
    col += vec3f(0.0004, 0.0008, 0.0019) * night_cam() * (1.0 - 0.4 * max(el, 0.0));
  }
  // City light scattered by the haze: domes over the towns on the horizon.
  col += glow_march(rd, select(1e9, gnd.x, gnd.x > 0.0), 12, fract(jit + G.time.w * 0.381966));
  return mix(col, globe, gk);
}

// ------------------------------------------------------ start screen globe
// The planet seen from orbit on the start screen (G.start.x > 0), made to look
// like a good photograph of the Earth: a thin blue rim of air at the limb, the
// surface's colours only lightly veiled, real clouds (globeclouds.rs) draped
// over it, a soft specular sheen off the sea, city lights on the night side.
// Only from high up (common.wgsl `globe_k`).
// Camera heights over which the draped clouds fade in, m.
const GLOBE_CLOUD_LO = 1000000.0;
const GLOBE_CLOUD_HI = 3000000.0;
// Height of the draped cloud layer (its shadow's offset), m.
const GLOBE_CLOUD_H = 6000.0;
// How much of the light the air scatters toward the eye in front of the
// surface is kept. The full amount reads as a pale blue veil at the
// display's exposure (a camera's "haze filter" in effect); the limb keeps it all.
const GLOBE_HAZE = 0.55;
// The aerosol (Mie) layer's share on the globe: the clean air of a clear day
// over the whole planet (flight's default haze, edge-on at the limb, whitens
// the rim into a lavender glare where photographs show it blue).
const GLOBE_AEROSOL = 0.35;


// The cloud map's texel for a direction from the planet centre (planet frame:
// z to the north pole, x to 0° longitude).
fn globe_uv(n: vec3f) -> vec2f {
  let lon = atan2(n.y, n.x);
  let lat = asin(clamp(n.z, -1.0, 1.0));
  return vec2f(lon / (2.0 * PI) + 0.5, clamp(0.5 - lat / PI, 0.0005, 0.9995));
}
// Cloud cover 0..1 (globeclouds.rs `cover`).
fn globe_cloud(n: vec3f) -> f32 {
  return textureSampleLevel(t_gclouds, s_repeat, globe_uv(n), 0.0).r;
}

// Single and multiple scattering along a ray through the air, without the
// cloud and storm terms of `atm_march` (none on the globe). Samples crowd
// toward the far end (`toward_end`, a ray down to the surface: the air
// thickens there) or spread evenly (a ray grazing the limb).
fn globe_air(oc: vec3f, rd: vec3f, tmin: f32, tmax: f32, steps: i32, jit: f32, toward_end: bool) -> Scatter {
  let sun = G.sun_dir.xyz;
  let c = dot(rd, sun);
  let pr = phase_r(c);
  let pm = phase_m(c, 0.8);
  let moon = G.moon.xyz;
  let cm = dot(rd, moon);
  let mk = MOON_TINT * (G.moon.w / G.sun_dir.w) * phase_r(cm);
  var T = vec3f(1.0);
  var L = vec3f(0.0);
  let len = max(tmax - tmin, 0.0);
  var tp = tmin;
  for (var i = 0; i < steps; i++) {
    let f = f32(i + 1) / f32(steps);
    let tn = tmin + len * select(f, 1.0 - (1.0 - f) * (1.0 - f), toward_end);
    let dt = tn - tp;
    let p = oc + rd * mix(tp, tn, jit);
    let r = length(p);
    let h = r - G.planet.w;
    let mu = dot(p / r, sun);
    var d = atm_density(h);
    d.y *= GLOBE_AEROSOL;
    let ext = atm_ext(d);
    let sr = RAYLEIGH * d.x;
    let sm = MIE_S * d.y;
    var S = sun_trans(h, mu) * (sr * pr + vec3f(sm * pm)) + ms_lut(h, mu) * (sr + vec3f(sm));
    if (G.moon.w > 0.0) { S += sun_trans(h, dot(p / r, moon)) * sr * mk; }
    let st = exp(-ext * dt);
    L += T * S * (vec3f(1.0) - st) / max(ext, vec3f(1e-12));
    T *= st;
    tp = tn;
  }
  return Scatter(L * G.sun_dir.w, T);
}

// Sunlight and skylight on a cloud top, sea or land at the globe's surface.
fn globe_sun(mu: f32, h: f32) -> vec3f { return sun_trans(h, mu) * G.sun_dir.w; }
fn globe_amb(mu: f32) -> vec3f {
  let day = min(smoothstep(-0.12, 0.3, mu), 0.06 * exp(20.0 * mu));
  // (with the sun low, the sky lighting the cloud tops is mostly the glow
  // round the sun: warm, not the blue overhead)
  let c = mix(vec3f(0.52, 0.42, 0.36), vec3f(0.30, 0.43, 0.70), smoothstep(0.02, 0.3, mu));
  return c * 0.11 * day * G.sun_dir.w;
}

// The sea from orbit: the imagery's deep blue, lit, with the sun's glint
// spread over the wave slopes (Cox-Munk, a light breeze) and kept to a soft
// sheen rather than a white blot.
fn globe_ocean(n: vec3f, V: vec3f, img: vec3f) -> vec3f {
  let sun = G.sun_dir.xyz;
  let mu = dot(n, sun);
  let lit = globe_sun(mu, 0.0);
  let moon = moon_light(n, 0.0) * max(dot(n, G.moon.xyz), 0.0);
  var col = img * ((lit * max(mu, 0.0) + moon) / PI + globe_amb(mu));
  let nv = max(dot(n, V), 0.02);
  if (mu > 0.0) {
    let H = normalize(sun + V);
    let nh = max(dot(n, H), 1e-3);
    let s2 = 0.03;
    let t2 = (1.0 - nh * nh) / (nh * nh);
    let p = exp(-t2 / s2) / (PI * s2 * nh * nh * nh * nh);
    let F = 0.02 + 0.98 * pow(1.0 - max(dot(V, H), 0.0), 5.0);
    col += lit * (F * p / (4.0 * nv)) * 0.3;
  }
  // The sky's blue in the water's reflection.
  col += globe_amb(mu) * (0.02 + 0.2 * pow(1.0 - nv, 5.0));
  return col;
}

// A drawn surface pixel (or the bare sphere where no tile is drawn yet) on
// the globe: sea, clouds and their shadows, then the air in front.
fn globe_surface(P: vec3f, rd: vec3f, surf: vec3f, water: bool, jit: f32) -> vec3f {
  let pp = P - G.planet.xyz;
  let n = normalize(pp);
  let sun = G.sun_dir.xyz;
  let mu = dot(n, sun);
  var col = surf;
  if (water) { col = globe_ocean(n, -rd, surf); }
  // (the cloud map is ~20 km a texel: it fades out as the view closes in)
  let k = smoothstep(GLOBE_CLOUD_LO, GLOBE_CLOUD_HI, G.up.w);
  // The cloud deck, and its shadow GLOBE_CLOUD_H below, cast away from the sun.
  let cl = globe_cloud(n) * k;
  let ts = sun - n * mu;
  let tl = length(ts);
  if (mu > 0.0 && tl > 1e-4) {
    let off = min(GLOBE_CLOUD_H * tl / max(mu, 0.08), 80000.0) / G.planet.w;
    let cs = globe_cloud(normalize(n + ts / tl * off)) * k;
    col *= 1.0 - 0.55 * cs * smoothstep(0.0, 0.08, mu);
  }
  if (cl > 0.0) {
    // Thick cloud: bright, nearly white, lit round the terminator a little
    // past where the ground goes dark (it stands higher, and scatters).
    let w = clamp((mu + 0.06) / 1.06, 0.0, 1.0);
    let moon = moon_light(n, GLOBE_CLOUD_H) * max(dot(n, G.moon.xyz), 0.0);
    let cc = vec3f(0.78) * ((globe_sun(mu, GLOBE_CLOUD_H) * w + moon) / PI + globe_amb(mu) * 1.3);
    col = mix(col, cc, cl * 0.96);
  }
  let oc = -G.planet.xyz;
  let top = ray_sphere(oc, rd, G.planet.w + ATM_H);
  let t0 = max(top.x, 0.0);
  let t1 = min(length(P), top.y);
  if (t1 > t0) {
    let sc = globe_air(oc, rd, t0, t1, 14, jit, true);
    // Toward the terminator the veil is the low sun's: reddened light
    // scattered blue again (and through the ozone edge-on), a dusky violet
    // over the warm-lit ground and clouds that muddies the edge of the night
    // in a picture exposed for the day. Thinned out there.
    let haze = GLOBE_HAZE * mix(0.3, 1.0, smoothstep(-0.05, 0.3, mu));
    col = col * mix(vec3f(1.0), sc.T, haze) + sc.L * haze;
  }
  return col;
}

// A ray past the globe: deep space, and the air at the limb lit by the sun.
fn globe_sky(rd: vec3f, jit: f32) -> vec3f {
  let oc = -G.planet.xyz;
  let gnd = ray_sphere(oc, rd, G.planet.w);
  if (gnd.x > 0.0) {
    // Where no tile is drawn yet, the bare sphere as sea.
    return globe_surface(rd * gnd.x, rd, vec3f(0.010, 0.025, 0.055), true, jit);
  }
  var col = space(rd) + moon_disc(rd);
  let top = ray_sphere(oc, rd, G.planet.w + ATM_H);
  if (top.y > 0.0) {
    let sc = globe_air(oc, rd, max(top.x, 0.0), top.y, 32, jit, false);
    col = col * sc.T + sc.L;
    // Airglow: the faint shell of light the upper air gives off by itself
    // (oxygen's green line, ~95 km up), seen edge-on as a thin line over the
    // night side's limb, where nothing else outlines the planet.
    let b = -dot(oc, rd);
    let pt = oc + rd * b;
    let ht = length(pt) - G.planet.w;
    let night = 1.0 - smoothstep(-0.2, 0.02, dot(normalize(pt), G.sun_dir.xyz));
    let e = exp(-pow((ht - 95000.0) / 7000.0, 2.0)) + 0.18 * (1.0 - smoothstep(0.0, 95000.0, ht));
    col += vec3f(0.30, 0.62, 0.45) * (0.004 * G.sun_dir.w * e * night);
  }
  return col;
}
