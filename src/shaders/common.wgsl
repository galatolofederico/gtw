// Shared by every pass: frame globals, atmosphere, clouds, waves, lighting.
struct Globals {
  view_proj: mat4x4f,  // includes this frame's sub-pixel offset (see taa.wgsl)
  inv_view_proj: mat4x4f,
  shadow_vp: array<mat4x4f, 3>,
  planet: vec4f,     // xyz planet centre relative to camera, w radius
  sun_dir: vec4f,    // xyz, w illuminance
  east: vec4f,       // xyz, w wave offset x
  north: vec4f,      // xyz, w wave offset y
  up: vec4f,         // xyz, w camera altitude
  screen: vec4f,     // w, h, 1/w, 1/h
  time: vec4f,       // seconds, dt, exposure, frame
  cloud: vec4f,      // coverage, base altitude, thickness, ceiling (top including storm towers)
  light_pos: vec4f,  // fire light, xyz relative to camera, w radius of the glowing ball
  light_col: vec4f,
  vol_o: vec4f,      // explosion volume corner relative to camera, w cell size
  vol_u: vec4f,      // volume up axis, w = 1 while it holds smoke
  misc: vec4f,       // near plane, stops the night exposure is closed down for city lights, water quality, pixel angle
  csm: vec4f,        // world size of a shadow texel in each cascade, w = shadows on
  scorch: array<vec4f, 8>,
  crater: array<vec4f, 8>,  // xyz relative to camera, w radius
  prev_vp: mat4x4f,  // last frame's view-projection (without its sub-pixel offset), relative to last frame's camera
  cam_delta: vec4f,  // xyz camera movement since last frame, w = 1 if history is usable
  moon: vec4f,       // xyz direction to the moon, w illuminance
  vol_e: vec4f,      // volume east axis
  vol_n: vec4f,      // volume north axis
  pshadow_vp: mat4x4f, // the aircraft's own shadow map
  pshadow: vec4f,    // texel size (m), depth bias, share of the layer used, w = 1 when in use
  taa: vec4f,        // xy this frame's sub-pixel offset (uv), z = 1 if the frame history is usable, w = 1 if the aircraft is drawn
  plane_inv: mat4x4f,  // camera-relative position to the aircraft's body frame
  plane_prev: mat4x4f, // body frame to last frame's clip space
  plane_box: array<vec4f, 2>, // centre and half size of the aircraft in its body frame
  weather: vec4f,    // xyz how far the wind has carried the clouds (planet frame, m), w haze (1 = clear day)
  storm: vec4f,      // xyz direction the wind aloft blows towards (planet frame), w number of storm cells
  storm_cells: array<vec4f, 32>, // per cell: centre (km on the sphere, cloud space) + tower radius (km); maturity
  det: array<vec4f, 16>, // ground material layers: m per repeat, m of relief, scale of the second sample, photo contrast (0 = procedural)
  photo_vp: mat4x4f, // photorealistic 3D tiles: camera-relative to coverage mask
  photo: vec4f,      // zone radius (m, 0 = off), dissolve band (m), mask valid
  photo_c: vec4f,    // zone centre on the ground, relative to the camera
  photo_sun: vec4f,  // the sun the photographs were taken under, w how sure (-1 = light them flat)
  // Destroyed buildings cut out of the 3D tiles (tiles3d.rs, Cut), G.photo.w of
  // them: floor middle (camera-relative) + radius; east axis + vertex count;
  // north axis + first vertex. Their outlines, two vertices (east, north) a row.
  cut: array<vec4f, 48>,
  cut_v: array<vec4f, 128>,
  // Cloud shadow maps (render.rs CloudShadow, see cloud_sun): the one lookups read
  // (t_cshadow), then the one being drawn. Each, six vec4s: anchor centre direction
  // + inner half size; east axis + outer half size; north axis + first layer; base
  // and top of its bands, centre (m, east and north of the anchor on its tangent
  // plane); the sun it was drawn for + valid; the cloud drift (G.weather.xyz) when
  // it was begun. The looked-up one is a cross-fade of two maps, with their sun and
  // drift blended the same way.
  cs: array<vec4f, 12>,
  alto: vec4f,       // mid-level cloud: amount 0..1, height (m)
  // The ocean (ocean.rs): offset of its coordinates + direction the wind blows
  // toward (east, north); wind (m/s), significant wave height (m), whitecap
  // cover, 1; slope covariance (east-east, north-north, east-north) of the
  // ripples shorter than any cascade; then at 3 + 9 c + m the slope covariance
  // cascade c's mip level m no longer holds (see `ocean_lost`).
  ocean: array<vec4f, 30>,
  shore_vp: mat4x4f, // camera-relative to the shore distance field (shore.rs)
  shore: vec4f,      // field valid, metres per texel, farthest distance kept (m), side of the square (m)
  // Churned white water where a crash went in, two each: centre (camera-relative)
  // + radius (m, 0 = none); strength 0..1, age (s), seed.
  foam: array<vec4f, 16>,
  // The start screen: x = 1 while it shows (deep-space sky, no clouds).
  start: vec4f,
  // Storms close up (storm.rs): x bit mask of the cells near enough to darken
  // the ground (storm_shade), y rain at the camera 0..1, z lightning flashes
  // lit, w how deep the camera is in a storm's shade 0..1.
  stormfx: vec4f,
  // Lightning lighting the scene, two each: one end of the lit stretch
  // (camera-relative) + brightness; the other end + kind (1 = to the ground).
  flash: array<vec4f, 8>,
  // The rain falling around the camera (precip.wgsl): how far it has moved past
  // the camera (east, north, up, m, wrapped) + intensity; its velocity relative
  // to the camera (m/s) + nearest drop drawn (m).
  rain: array<vec4f, 2>,
  // Graphics settings: cloud step length (1 = High), render resolution scale,
  // 1 / window width and height (what the tonemap writes).
  quality: vec4f,
  // Graphics settings: building level-of-detail distance factor, tree
  // impostor distance factor, how far out trees are drawn (m). 0 = High.
  detail: vec4f,
  // The rain deck (nimbostratus, weather.rs deck_globals): centre of its area
  // (cloud space, km on the sphere) + amount 0..1 (0 = none); tangent axis +
  // mean base (m); the other axis + top (m); how dark it is under it, -, how
  // far its ragged scud hangs below the base (m), the area's radius (km).
  nimbo: array<vec4f, 4>,
  // City light in the air (skyglow.rs): map centre relative to the camera + half
  // its side (0 = none); its east axis + ground height under the camera; its
  // north axis + how far the lights are on (0 by day).
  glow_c: vec4f,
  glow_e: vec4f,
  glow_n: vec4f,
  // The flown aircraft's landing lights (`spot_light`): two lamps, each
  // camera-relative position + intensity (0 = off); the beam's direction +
  // cosine of its half-angle.
  spot: array<vec4f, 3>,
  // The flown aircraft's NAV 1 (main.rs `nav_globals`): course needle -1..1
  // (+ fly right), glide slope needle -1..1 (+ fly up), OBS (deg), flags (1
  // alive, 2 TO, 4 FROM, 8 glide slope alive).
  nav: vec4f,
  // Rows 0-7: the Cessna's instruments, radios and navigation plot (main.rs
  // cockpit_globals). Rows 8-11, for the glass cockpits (deck650.wgsl): the
  // autopilot (mode bits, heading bug deg, preselected altitude ft, speed kt;
  // V/S target fpm, trim deg, speedbrake lever, spoilers 0..1); reversers,
  // flags (1 running, 2 starter, 4 fuel on, 8 bus powered, 16 on the ground,
  // 32 park brake), toe brakes 0..1, angle of attack / stall angle; battery
  // volts and amps, cabin altitude ft, airspeed trend (kt in 6 s).
  cockpit: array<vec4f, 12>,
  // Snow lying on the ground (snow.rs `globals`): snow line under the camera
  // (m), its change northwards (m per m), +1 / -1 for the north / south pole,
  // 1 when there is any snow.
  snow: vec4f,
  // Today's real snow cover (snowmap.rs, t_snow): the map's centre relative to
  // the camera + half its side; its east axis + how much it counts (0 = not
  // at all); its north axis.
  snow_c: vec4f,
  snow_e: vec4f,
  snow_n: vec4f,
  // Snowflakes around the camera (precip.wgsl): how far they have moved past
  // the camera (east, north, up, m, wrapped) + how much of what falls here is
  // snow; their velocity relative to the camera (m/s) + blowing snow 0..1.
  flakes: vec4f,
  flakes_v: vec4f,
  // Fresh snow where it has snowed (snow.rs Fresh): its snow line (m), how
  // far it has settled 0..1.
  fresh: vec4f,
  // Dynamic snow tracks (snowtrack.rs, t_snowtrack): SW corner of the active
  // window relative to the camera + side length in m (0 = none); its east axis
  // + toroidal u offset (m); its north axis + toroidal v offset (m).
  snow_tr_c: vec4f,
  snow_tr_e: vec4f,
  snow_tr_n: vec4f,
  // Precomputed horizontal bounding circles of the storm cells (storm.rs `bound`):
  // xyz centre on the planet surface (planet frame, m) + w radius (m).
  storm_bounds: array<vec4f, 16>,
  // Precomputed lone-cell turrets (5 per cell: tc.xy, rk, ck) and flanking
  // towers (3 per cell: fo.xy, fr, ft) (storm.rs `turrets`).
  storm_turrets: array<vec4f, 128>,
  // Graphics settings (graphics.rs): x the water's detail (0 plain, 1 full,
  // 2 full + screen-space reflections), y 1 while the clouds are drawn.
  lite: vec4f,
  // Precomputed top-mip means for the ground detail arrays (render.rs GroundMats):
  // det_mean: rgb = 1 / max(albedo mean, 0.02), w = cavity mean; det_hmean = height mean (4 per vec4).
  det_mean: array<vec4f, 16>,
  det_hmean: array<vec4f, 4>,
  // Apron floodlights (world.rs `flood_globals`), eight, two each: lamp
  // (camera-relative) + intensity (0 = none, and none after it); beam
  // direction + cosine of its half-angle.
  flood: array<vec4f, 16>,
};
@group(0) @binding(0) var<uniform> G: Globals;
@group(0) @binding(1) var t_trans: texture_2d<f32>;
@group(0) @binding(2) var t_ms: texture_2d<f32>;
@group(0) @binding(3) var t_sky: texture_2d<f32>;
@group(0) @binding(4) var t_noise: texture_3d<f32>;
@group(0) @binding(5) var t_wave: texture_2d<f32>;
@group(0) @binding(6) var s_clamp: sampler;
@group(0) @binding(7) var s_repeat: sampler;
@group(0) @binding(8) var t_shadow: texture_depth_2d_array;
@group(0) @binding(9) var s_shadow: sampler_comparison;
@group(0) @binding(10) var s_aniso: sampler;
// Lighting of the smoke volume (see vol.wgsl): x = optical depth toward the
// sun, y = fire light, z = optical depth upward, w = density.
@group(0) @binding(11) var t_vlight: texture_3d<f32>;
// Ground material library (detail.wgsl): albedo + height, normal + cavity.
@group(0) @binding(12) var t_dalb: texture_2d_array<f32>;
@group(0) @binding(13) var t_dnrm: texture_2d_array<f32>;
@group(0) @binding(14) var s_detail: sampler;
// Where the photorealistic 3D tiles are drawn, seen from above (tiles3d.rs).
@group(0) @binding(15) var t_photo: texture_2d<f32>;
// Cloud shadow map, the two cascades (see cloud_sun).
@group(0) @binding(16) var t_cshadow: texture_2d_array<f32>;
// The ocean's three wave cascades (ocean.wgsl): layer c = (Dx, Dy, height,
// Dxy), layer c + 3 = (dh/dx, dh/dy, Dxx, Dyy); layer 6 the whitecaps on
// cascade 0's grid (foam cover, fresh foam, fold threshold on the Jacobian, 0).
@group(0) @binding(17) var t_ocean: texture_2d_array<f32>;
// Distance to the shore (shore.rs): metres to the nearest land, seaward
// direction (east, north), 1.
@group(0) @binding(18) var t_shore: texture_2d<f32>;
// Light rising off the towns around the camera (skyglow.rs): the night-lights map
// averaged over 1.2 km cells, then blurred for the light reaching ~1.5, 4.5, 15 km up.
@group(0) @binding(19) var t_glow: texture_2d<f32>;
// The start screen globe's cloud map (globeclouds.rs): cloud cover from an
// infrared composite, equirectangular, 180°W at u = 0, 90°N at v = 0.
@group(0) @binding(20) var t_gclouds: texture_2d<f32>;
// Today's real snow cover around the camera (snowmap.rs): R share of the
// cell under snow, G how much of it the analysis knows (land, northern
// hemisphere).
@group(0) @binding(21) var t_snow: texture_2d<f32>;
// Dynamic snow tracks and crash trails around the aircraft (snowtrack.rs):
// R cleared/compressed snow trough, G displaced snow berm along the rims.
@group(0) @binding(22) var t_snowtrack: texture_2d<f32>;

// GPU particles (explosions, smoke, dust, spray) and their simulation constants.
struct Particle {
  pos: vec3f, age: f32,
  vel: vec3f, life: f32,
  size0: f32, size1: f32, temp: f32, kind: f32,
  drag: f32, buoy: f32, seed: f32, alpha: f32,
};
// ve/vn: east and north axes of the smoke volume (w = cell size, lowest altitude).
// shift: how far the frame moved this step (every live particle is carried along).
struct PSim { anchor: vec4f, upv: vec4f, cam_r: vec4f, cam_u: vec4f, wind: vec4f, ve: vec4f, vn: vec4f, shift: vec4f };

// The light matrix of whichever shadow cascade is being rendered.
struct Cascade { vp: mat4x4f };

const PI = 3.14159265;
const ATM_H = 100000.0;
const RAYLEIGH = vec3f(5.802e-6, 13.558e-6, 33.1e-6);
const MIE_S = 3.996e-6;
const MIE_A = 4.4e-6;
const OZONE = vec3f(0.650e-6, 1.881e-6, 0.085e-6);
// Moonlight is sunlight, but night vision sees it blue (Purkinje shift).
const MOON_TINT = vec3f(0.72, 0.95, 1.45);
// Sodium and LED city light scattered back down by the haze.
const SKYGLOW = vec3f(1.0, 0.62, 0.36);

struct FsIn { @builtin(position) pos: vec4f, @location(0) uv: vec2f };
@vertex fn vs_full(@builtin(vertex_index) i: u32) -> FsIn {
  let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  return FsIn(vec4f(p * 2.0 - 1.0, 0.0, 1.0), vec2f(p.x, 1.0 - p.y));
}

// The graphics settings' factor on the distances buildings change level of
// detail at, and on the distance trees turn into impostors at (1 = High).
fn city_k() -> f32 { return select(G.detail.x, 1.0, G.detail.x <= 0.0); }
fn tree_imp_k() -> f32 { return select(G.detail.y, 1.0, G.detail.y <= 0.0); }
// How far out trees are drawn (m).
fn tree_far() -> f32 { return select(G.detail.z, 1e9, G.detail.z <= 0.0); }
fn ign(p: vec2f) -> f32 { return fract(52.9829189 * fract(dot(p, vec2f(0.06711056, 0.00583715)))); }
fn hash1(p: f32) -> f32 { return fract(sin(p * 127.1) * 43758.5453); }
fn hash2(p: vec2f) -> f32 { return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453); }
fn hash3(p: vec3f) -> f32 { return fract(sin(dot(p, vec3f(127.1, 311.7, 74.7))) * 43758.5453); }
fn remap(v: f32, a: f32, b: f32, c: f32, d: f32) -> f32 { return c + (v - a) / (b - a) * (d - c); }
fn luma(c: vec3f) -> f32 { return dot(c, vec3f(0.2126, 0.7152, 0.0722)); }
// The sky and bounce light in the colour being shaded, set by the lighting
// functions, so the G-buffer can record how much of a pixel ambient occlusion
// should darken (sunlight, lamps and glow stay as they are).
var<private> g_amb: vec3f;
fn amb_share(c: vec3f) -> f32 { return clamp(luma(g_amb) / max(luma(c), 1e-12), 0.0, 1.0); }

// Camera ray for a screen uv, and the point at a given reverse-Z depth.
fn view_ray(uv: vec2f) -> vec3f {
  let p = G.inv_view_proj * vec4f(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 1.0, 1.0);
  return p.xyz / p.w;
}

// Numerically careful ray/sphere: oc = ray origin - centre. Returns (-1,-1) on a miss.
fn ray_sphere(oc: vec3f, rd: vec3f, r: f32) -> vec2f {
  let b = dot(oc, rd);
  let dp = length(oc - rd * b);
  if (dp > r) { return vec2f(-1.0); }
  let s = sqrt((r - dp) * (r + dp));
  return vec2f(-b - s, -b + s);
}

// ------------------------------------------------------------- atmosphere
fn atm_density(h: f32) -> vec3f {
  let hh = max(h, 0.0);
  // Haze scales the aerosol (Mie) layer only; it lives in the lowest kilometre or two.
  return vec3f(exp(-hh / 8000.0), exp(-hh / 1200.0) * G.weather.w, max(0.0, 1.0 - abs(hh - 25000.0) / 15000.0));
}
fn atm_ext(d: vec3f) -> vec3f { return RAYLEIGH * d.x + vec3f((MIE_S + MIE_A) * d.y) + OZONE * d.z; }

fn trans_uv(h_in: f32, mu: f32) -> vec2f {
  let rg = G.planet.w;
  let h = clamp(h_in, 0.0, ATM_H);
  let hh = sqrt(ATM_H * (2.0 * rg + ATM_H));
  let rho = sqrt(h * (2.0 * rg + h));
  let r = rg + h;
  let rt = rg + ATM_H;
  let disc = rt * rt - r * r * (1.0 - mu * mu);
  let d = max(0.0, -r * mu + sqrt(max(disc, 0.0)));
  let dmin = ATM_H - h;
  let dmax = rho + hh;
  return vec2f(clamp((d - dmin) / max(dmax - dmin, 1.0), 0.0, 1.0), rho / hh) * vec2f(255.0 / 256.0, 63.0 / 64.0) + vec2f(0.5 / 256.0, 0.5 / 64.0);
}
// Sunlight colour reaching a point at altitude h where the sun's zenith cosine is mu.
fn sun_trans(h: f32, mu: f32) -> vec3f {
  let rg = G.planet.w;
  let hc = max(h, 0.0);
  let mu_h = -sqrt(hc * (2.0 * rg + hc)) / (rg + hc);
  let vis = smoothstep(mu_h - 0.006, mu_h + 0.006, mu);
  if (vis <= 0.0) { return vec3f(0.0); }
  return textureSampleLevel(t_trans, s_clamp, trans_uv(hc, mu), 0.0).rgb * vis;
}
fn ms_lut(h: f32, mu: f32) -> vec3f {
  return textureSampleLevel(t_ms, s_clamp, vec2f(mu * 0.5 + 0.5, clamp(h / ATM_H, 0.0, 1.0)) * (31.0 / 32.0) + 0.5 / 32.0, 0.0).rgb;
}
fn phase_r(c: f32) -> f32 { return 3.0 / (16.0 * PI) * (1.0 + c * c); }
fn phase_m(c: f32, g: f32) -> f32 {
  let g2 = g * g;
  return 3.0 / (8.0 * PI) * (1.0 - g2) * (1.0 + c * c) / ((2.0 + g2) * pow(1.0 + g2 - 2.0 * g * c, 1.5));
}

struct Scatter { L: vec3f, T: vec3f };
// Single scattering plus the multi-scatter LUT along a ray segment. oc is the
// ray origin relative to the planet centre.
fn atm_march(oc: vec3f, rd: vec3f, tmin: f32, tmax: f32, steps: i32, jit: f32) -> Scatter {
  let rg = G.planet.w;
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
  let cs_on = G.cs[4].w >= 0.5 && G.start.x <= 0.0;
  let cs_top = G.cs[3].y;
  var T = vec3f(1.0);
  var L = vec3f(0.0);
  let len = max(tmax - tmin, 0.0);
  var tp = tmin;
  // Under a storm the air around is in its shade too (fading out over ~15 km).
  let sdark = G.stormfx.w;
  for (var i = 0; i < steps; i++) {
    let f = f32(i + 1) / f32(steps);
    let tn = tmin + len * f * f;
    let dt = tn - tp;
    let tm = mix(tp, tn, jit);
    let p = oc + rd * tm;
    let r = length(p);
    let h = r - rg;
    let mu = (oc_sun + c * tm) / r;
    let d = atm_density(h);
    let ext = atm_ext(d);
    let sr = RAYLEIGH * d.x;
    let sm = MIE_S * d.y;
    // Air in the shadow of a cloud gets no sunlight (light shafts through the gaps);
    // under a closed deck the light that remains is the deck's dull grey.
    var msl = ms_lut(h, mu);
    var vis = 1.0;
    if (cs_on && h < cs_top) {
      let cl = cloud_sun_at(p, r, h, false);
      msl = mix(msl, vec3f(luma(msl)), cl.closed * 0.75) * (1.0 - 0.45 * cl.closed);
      vis = cl.vis;
    }
    if (sdark > 0.0) {
      let k = 1.0 - 0.8 * sdark * exp(-mix(tp, tn, 0.5) / 15000.0);
      msl *= k;
      vis *= k;
    }
    var S = sun_trans(h, mu) * vis * (sr * pr + vec3f(sm * pm)) + msl * (sr + vec3f(sm));
    if (moon_on) {
      let mum = (oc_moon + cm * tm) / r;
      S += (sun_trans(h, mum) * (sr * prm + vec3f(sm * pmm)) + ms_lut(h, mum) * (sr + vec3f(sm))) * mk;
    }
    let st = exp(-ext * dt);
    L += T * S * (vec3f(1.0) - st) / max(ext, vec3f(1e-12));
    T *= st;
    tp = tn;
  }
  // 1.35: artistic lift so the sky and haze read as bright as they do in photographs.
  return Scatter(L * G.sun_dir.w * 1.35, T);
}

// ------------------------------------------------------------ city glow
// Colour of the towns' light: sodium orange with some whiter LED in it.
const GLOW_COL = vec3f(1.0, 0.58, 0.30);
// Radiance of a map value of 1 (the terrain's brightest downtown, terrain.wgsl GLOW_K).
const CITY_K = 0.0016;
// Artistic lift of the scattered light: the haze over real cities is murkier
// (smog, humidity) than the clear-day aerosol the atmosphere is built on.
const GLOW_GAIN = 16.0;
// Extra aerosol scattering the towns' light (multiplies the haze layer's).
const URBAN_HAZE = 4.0;
fn glow_on() -> bool { return G.glow_n.w > 0.0 && G.glow_c.w > 0.0; }
// City light arriving from below at P (camera-relative), hag metres above the
// ground: the light of the ground around, over an area that grows with height
// (a lit plane seen from height h: half its light comes from within h), dimmed
// by the haze on the way up. Illuminance, render units.
fn city_up(P: vec3f, hag: f32) -> vec3f {
  let q = P - G.glow_c.xyz;
  let uv = vec2f(dot(q, G.glow_e.xyz), -dot(q, G.glow_n.xyz)) / (2.0 * G.glow_c.w) + 0.5;
  let m = min(uv, 1.0 - uv);
  let edge = smoothstep(0.0, 0.1, min(m.x, m.y));
  if (edge <= 0.0) { return vec3f(0.0); }
  let s = textureSampleLevel(t_glow, s_clamp, uv, 0.0);
  // The channels stand for 0.4, 1.5, 4.5 and 15 km up.
  let lh = log2(clamp(hag, 400.0, 15000.0) / 400.0);
  var v = mix(s.z, s.w, (lh - 3.492) / 1.737);
  if (lh < 1.907) { v = mix(s.x, s.y, lh / 1.907); }
  else if (lh < 3.492) { v = mix(s.y, s.z, (lh - 1.907) / 1.585); }
  let hh = max(hag, 0.0);
  let tau = (MIE_S + MIE_A) * G.weather.w * 1800.0 * (1.0 - exp(-hh / 1200.0)) + 1.2e-5 * hh;
  return GLOW_COL * (PI * CITY_K * G.sun_dir.w * G.glow_n.w) * v * exp(-tau) * edge;
}
// The haze lit from below by the towns, along the view ray out to tmax (or
// where it leaves the low air / the map). Light scattered toward the camera,
// already dimmed on its way there. Seen from above a city, the haze scatters
// the rising light forward into the eye; from afar it is a dome on the horizon.
fn glow_march(rd: vec3f, tmax_in: f32, steps: i32, jit: f32) -> vec3f {
  if (!glow_on()) { return vec3f(0.0); }
  let oc = -G.planet.xyz;
  let gnd = G.glow_e.w;
  let top = ray_sphere(oc, rd, G.planet.w + gnd + 7000.0);
  if (top.y <= 0.0) { return vec3f(0.0); }
  let t0 = max(top.x, 0.0);
  let t1 = min(min(tmax_in, top.y), G.glow_c.w * 1.2);
  if (t1 <= t0) { return vec3f(0.0); }
  var T = vec3f(1.0);
  var L = vec3f(0.0);
  let len = t1 - t0;
  var tp = t0;
  for (var i = 0; i < steps; i++) {
    let f = f32(i + 1) / f32(steps);
    let tn = t0 + len * f * f;
    let dt = tn - tp;
    let p = oc + rd * mix(tp, tn, jit);
    let r = length(p);
    let upn = p / r;
    let h = r - G.planet.w;
    let hag = h - gnd;
    // The haze follows the ground (the boundary layer), not sea level; over
    // towns it is stirred up higher by the heat of the city.
    let dm = exp(-max(hag, 0.0) / 1600.0) * G.weather.w;
    let sr = RAYLEIGH * exp(-max(h, 0.0) / 8000.0);
    let sm = MIE_S * dm;
    let ext = sr + vec3f((MIE_S + MIE_A) * dm);
    let cu = dot(upn, -rd);
    let E = city_up(p + G.planet.xyz, hag);
    // Over towns the air holds far more aerosol than the open-country model:
    // the glow is mostly that grey-orange haze, the blue air scatters a little.
    // The light comes up from a wide patch of ground and scatters more than
    // once, so the haze's phase is half evened out.
    let S = E * (sr * phase_r(cu) * 0.35 + vec3f(sm * URBAN_HAZE * mix(phase_m(cu, 0.35), 0.25 / PI, 0.5)));
    let st = exp(-ext * dt);
    L += T * S * (vec3f(1.0) - st) / max(ext, vec3f(1e-12));
    T *= st;
    tp = tn;
  }
  return L * GLOW_GAIN;
}

// Sky along a ray from the camera to space or the ground.
fn sky_march(rd: vec3f, steps: i32, jit: f32) -> Scatter {
  let oc = -G.planet.xyz;
  let top = ray_sphere(oc, rd, G.planet.w + ATM_H);
  if (top.y <= 0.0) { return Scatter(vec3f(0.0), vec3f(1.0)); }
  var tmax = top.y;
  let gnd = ray_sphere(oc, rd, G.planet.w);
  if (gnd.x > 0.0) { tmax = gnd.x; }
  return atm_march(oc, rd, max(top.x, 0.0), tmax, steps, jit);
}

fn sky_uv(rd: vec3f) -> vec2f {
  let el = asin(clamp(dot(rd, G.up.xyz), -1.0, 1.0));
  let az = atan2(dot(rd, G.north.xyz), dot(rd, G.east.xyz));
  let v = 0.5 + 0.5 * sign(el) * sqrt(abs(el) / (PI * 0.5));
  return vec2f(az / (2.0 * PI) + 0.5, clamp(1.0 - v, 0.5 / 128.0, 1.0 - 0.5 / 128.0));
}
// Cheap sky lookup (reflections, ambient).
fn sky_lut(rd: vec3f) -> vec3f { return textureSampleLevel(t_sky, s_repeat, sky_uv(rd), 0.0).rgb; }

// ------------------------------------------------------------------ clouds
// The layer spans G.cloud.y .. G.cloud.y + G.cloud.z. A weather field decides,
// per column, how much cloud there is, what kind (stratus, stratocumulus,
// cumulus) and how tall it grows; 3D noise carves the actual shapes. Rare storm
// cells (cumulonimbus) tower on up to G.cloud.w and spread into anvils there.
const CLOUD_SHAPE = 7600.0;   // tile size of the base shape noise, metres
const CLOUD_DETAIL = 1350.0;  // tile size of the erosion noise, metres
const CLOUD_FINE = 310.0;     // tile size of the close-up erosion octave, metres
const CU_SHAPE = 15000.0;     // tile size of the cumulus cell noise, metres
const CIRRUS_H = 8800.0;

// cb: how deep inside a storm cell this point is, 0 outside .. 1 well inside.
// ci: index of the winning storm cell in G.storm_cells / G.storm_turrets (-1 = none).
// cell: stratocumulus cell pattern, 1 at the heart of a cell, low between cells.
// ns: how far into the rain deck's area the column is (nimbostratus), 0..1.
struct Weather { cov: f32, typ: f32, top: f32, cb: f32, ci: i32, cell: f32, ns: f32 };
fn no_weather() -> Weather { return Weather(0.0, 0.0, 0.0, 0.0, -1, 0.0, 0.0); }
// Signed distance (m, + inside) and radius (km) of the closest storm cell's
// bound envelope, set by storm_find / cloud_weather_m for step-sizing.
// g_storm_sd2 holds the 2nd-closest cell's bound so exact storm_sd distances
// from the winning cell can safely expand clear-air strides without ever
// skipping over a neighbouring storm cell.
var<private> g_storm_sd: f32;
var<private> g_storm_sd2: f32;
var<private> g_storm_sd_full: f32;
var<private> g_storm_r: f32;
// Set briefly on the main march step when storm_find's upper bound already
// proves the sample lies outside storm_reach (skips redundant storm_sd(..., true)).
var<private> g_skip_storm: bool = false;
var<private> g_meso: bool = false;

// The rain deck: where rain is set (weather.rs rain_set) the layer closes into
// a deep grey nimbostratus over a lobed area ~90 km across around where it was
// set, drifting with the wind. How far into that area a point is, 0..1 (soft
// edge NIMBO_EDGE km). Must match weather.rs `deck_area`. (Callers check
// G.nimbo[0].w > 0 first.)
const NIMBO_EDGE = 10.0;
fn nimbo_area(pp: vec3f) -> f32 {
  let d = normalize(pp) * 6371.0 + G.weather.xyz * 0.001 - G.nimbo[0].xyz;
  let x = dot(d, G.nimbo[1].xyz);
  let y = dot(d, G.nimbo[2].xyz);
  let wob = 9.0 * sin(0.083 * x + 1.7 * sin(0.051 * y)) + 6.0 * sin(0.13 * y + 0.9 + 0.8 * sin(0.07 * x));
  let R = G.nimbo[3].w;
  return smoothstep(R, R - NIMBO_EDGE, length(vec2f(x, y)) + wob);
}
// How far under (or deep in) the deck a point (planet-centred) is, 0..1: there
// only the dull light diffusing through it reaches. Times G.nimbo[3].x, this
// is weather.rs `deck_shade`.
fn nimbo_under(pp: vec3f) -> f32 {
  if (G.nimbo[0].w <= 0.0) { return 0.0; }
  let h = length(pp) - G.planet.w;
  let b = G.nimbo[1].w;
  let t = G.nimbo[2].w;
  return nimbo_area(pp) * (1.0 - smoothstep(b, t - 0.15 * (t - b), h));
}
// The deck's extinction (cloud_shape_f's stratus density x clouds.wgsl SIGMA), 1/m.
const NIMBO_SIGMA = 0.0275;
// The light under the deck (or in it at height h): sun and sky on its top,
// diffused down through all of it (two-stream, g ~ 0.85: 1 / (1 + 0.75 (1 - g)
// tau), a little lower for the rain in it). A radiance: what its base sends
// down, and what a white surface under it sends back. A cool, flat grey.
// (nimbo_top: the light on its top, as a radiance; nimbo_down: what is left of
// it at height h. The cloud march takes the top once per pixel.)
fn nimbo_top(upn: vec3f) -> vec3f {
  let mu = dot(upn, G.sun_dir.xyz);
  let e = luma(sun_trans(G.nimbo[2].w, mu)) * G.sun_dir.w * (max(mu, 0.0) + 0.12 * smoothstep(-0.12, 0.1, mu));
  return vec3f(0.9, 0.95, 1.0) * (e / PI);
}
fn nimbo_down(top: vec3f, h: f32) -> vec3f {
  return top / (1.0 + 0.15 * NIMBO_SIGMA * max(G.nimbo[2].w - max(h, G.nimbo[1].w), 0.0));
}
fn nimbo_light(upn: vec3f, h: f32) -> vec3f { return nimbo_down(nimbo_top(upn), h); }
// The wall cloud under a big storm hangs down to this far below the cloud base, m.
const STORM_BELOW = 800.0;
// ...and a squall line's shelf cloud this far (how far down storm rays march).
const SHELF_BELOW = 1150.0;
// How deep its nose hangs at the deepest, m under the base (shelf_depth x
// this; its lumpy underside up to a fifth deeper).
const SHELF_D = 920.0;
// The ragged ceiling (the storm's dark base) a squall line spreads out over its
// shelf cloud and a little ahead of it: how deep it hangs under the base, m,
// and how far ahead of the shelf's nose it reaches (x R).
const SHELF_CEIL = 260.0;
const SHELF_AHEAD = 0.35;
// A squall line's shelf cloud: its leading edge this far ahead of the line's
// axis, and how far back from there its upper face rises into the base (x the
// line's radius R). Must match storm.rs.
const SHELF_F = 1.42;
const SHELF_W = 0.50;
// ...and over how much of that its steep upper face rises (x R).
const SHELF_FACE = 0.14;

// How deep a squall line's shelf cloud hangs at cxl metres along the line (a
// share of its full depth): scalloped, one shallow arc after another, deepest
// in their middles, and swelling and easing over tens of km. (At one depth
// all along, its lower edge ran ruler-straight across the sky.)
// (shelf_lobe: where in its scallop, 1 in the middle, 0 at the cusps.)
fn shelf_lobe(cxl: f32, seed: f32) -> f32 {
  let a = fract(cxl / 3600.0 + seed * 7.0) * 2.0 - 1.0;
  return 1.0 - a * a;
}
fn shelf_depth(cxl: f32, seed: f32) -> f32 {
  return (0.74 + 0.26 * shelf_lobe(cxl, seed)) * (0.9 + 0.1 * sin(cxl / 14000.0 + seed * 5.0));
}
// Each scallop bulges forward in its middle as well, a rounded lobe, the cusps
// between them tucked back (m to add to u): along a straight nose the shelf
// stood as a flat stripe across the view.
fn shelf_bulge(cxl: f32, seed: f32, R: f32) -> f32 { return 0.07 * R * (shelf_lobe(cxl, seed) - 0.67); }

// The height (m from the base) of a squall line's shelf cloud's upper face u
// metres back from its nose, Ds its depth there: a blunt front SHELF_NOSE of
// Ds tall, then three laminar tiers (a flat ledge, then a steep riser),
// climbing into the ceiling over it (to the base, its upper tiers hid inside
// that). (Each tier as one smooth ramp, the face showed as one even band.)
const SHELF_NOSE = 0.33;
fn shelf_tier(xc: f32) -> f32 {
  let xt = min(xc * 3.0, 2.999);
  return (floor(xt) + smoothstep(0.5, 0.85, fract(xt))) / 3.0;
}
fn shelf_top(u: f32, R: f32, Ds: f32) -> f32 {
  let xc = clamp(u / (SHELF_FACE * R), 0.0, 1.0);
  let lo = Ds * (1.0 - SHELF_NOSE);
  return -lo + (lo - 0.5 * SHELF_CEIL) * (0.3 * sqrt(xc) + 0.7 * shelf_tier(xc));
}
// Rounded intersection of two half-spaces at distances a and b (radius r).
fn round_min(a: f32, b: f32, r: f32) -> f32 { return max(r, min(a, b)) - length(max(vec2f(r - a, r - b), vec2f(0.0))); }
// Smooth polynomial union of two signed distances (+ inside) with blend radius k.
fn smooth_max(a: f32, b: f32, k: f32) -> f32 {
  let h = max(k - abs(a - b), 0.0) / max(k, 1.0);
  return max(a, b) + h * h * k * 0.25;
}

// How far the turrets push a storm cell's surface out past its envelope, m.
fn storm_bulge(sc: vec4f) -> f32 { return clamp(sc.w * 250.0, 1000.0, 3200.0); }
// How far out past the envelope there can be any cloud at all: the billows push
// out by at most 0.22 x 1.3 of the bulge, x 1.9 on the flanks (cloud_shape_f;
// under the base, hb < 0, a quarter of 0.3), the low scud by 300 m. (The whole bulge is a far looser
// bound than that.)
fn storm_reach(sc: vec4f, hb: f32) -> f32 { return select(0.56, 0.08, hb < 0.0) * storm_bulge(sc) + 320.0; }
// How high a storm cell grows, as a share of base..anvil: young towers stop short.
fn storm_top(sm: f32) -> f32 { return mix(0.3, 1.0, smoothstep(0.0, 0.55, sm)); }
// How far a storm cell's anvil streams downwind (x) and upwind (y) of its core,
// and its half-width across the wind at the core (z), metres.
fn storm_anvil(sc: vec4f, sp: vec4f) -> vec3f {
  let R = sc.w * 1000.0;
  let mature = smoothstep(0.5, 0.8, sp.x);
  // (A squall line's shared anvil reaches a little less far ahead: a lone
  // cell's reach, all along a 100 km line, filled the sky with thin ice the
  // march had to cross.) A supercell's spreads 100 km and more downwind, and
  // is sheared back a little way upwind.
  let sup = storm_super(sp);
  return vec3f(R * (1.0 + mix(select(4.0, 2.6, sp.w >= 1.0), 7.8, sup) * mature), R * (0.9 + 0.35 * mature + 0.35 * sup), R * (1.1 + 0.65 * mature + 0.25 * sup));
}
// A supercell (weather.rs CellKind::Supercell): its maturity runs on past 1, to
// 2 for a full one; 0..1 how much of one it is.
fn storm_super(sp: vec4f) -> f32 { return clamp(sp.x - 1.0, 0.0, 1.0); }
// How far a cell's tower leans downwind per metre of height (wind shear): a
// supercell's, in the strong shear it grows in, the most. Must match storm.rs.
fn storm_lean(sp: vec4f) -> f32 { return mix(0.12, 0.22, smoothstep(0.5, 0.8, sp.x)) + 0.08 * storm_super(sp); }
// Signed distance (m, + inside) to the smooth envelope of storm cell (sc, sp) at a
// point hb metres above the cloud base: a fat tower of updrafts leaning downwind,
// young ones rounding off into a dome; mature ones spread into an anvil under the
// tropopause (flat, slightly domed top; underside sloping up and out from the
// tower; fanning out downwind), with the core overshooting it in a dome, and under
// the base a wall cloud (upwind of the core); a flanking line of younger towers
// steps down from it upwind. The billows are noise added on top of this
// (cloud_shape_f).
// full = false gives an upper bound instead (lobes at their widest, the flanking
// line as one fat cylinder), cheaper, for picking the cell and sizing strides.
// (A cheaper look-alike for the long light steps was tried: steps that missed
// the lobes and flanking towers left those flat-lit.)
var<private> g_anvil: f32;
// How much the wall cloud is the part of the envelope that counts (set by storm_sd).
var<private> g_wall: f32;
fn storm_sd(pp: vec3f, hb: f32, ci: u32) -> f32 {
  g_anvil = 0.0;
  g_wall = 0.0;
  let sc = G.storm_cells[2u * ci];
  let sp = G.storm_cells[2u * ci + 1u];
  if (sc.w <= 0.0) { return -1e6; }
  let R = sc.w * 1000.0;
  // A squall line: sp.w's whole part is its length (km, 0 = a lone cell). It
  // lies across the wind, a row of merged cells: the lone cell's tower, anvil
  // and base drawn out along it (cx = how far past its nearer end), its turrets
  // and overshooting tops repeated along it, and a shelf cloud in place of the
  // wall cloud.
  let L2 = floor(sp.w) * 500.0;
  let seed = sp.w - floor(sp.w);
  let mature = smoothstep(0.5, 0.8, sp.x);
  let sup = storm_super(sp);
  let lean = storm_lean(sp);
  let at = sp.y - G.cloud.y;               // anvil top above the base
  let htop = at * storm_top(sp.x);         // this tower's top
  let size = smoothstep(4000.0, 13000.0, R);
  let W = G.storm.xyz;
  let un = normalize(pp);
  let X = normalize(cross(un, W));
  let dv = (un * 6371.0 + G.weather.xyz * 0.001 - sc.xyz) * 1000.0;
  let dw0 = dot(dv, W);
  let cxl = dot(dv, X);
  let cx = sign(cxl) * max(abs(cxl) - L2, 0.0);
  // (A squall line's towers and anvil come down towards its ends, or it ended
  // in one vertical cut through them all.)
  let lend = smoothstep(L2 + 0.5 * R, L2 - 3.0 * R, abs(cxl));
  // Tower: leans downwind with height (wind shear). A core that tapers as it
  // climbs, ringed by turrets (rounded columns of their own height), so the
  // outline steps in and narrows upwards like a cluster of updrafts instead of
  // standing as one straight-walled drum. Far from it (outside its widest turret)
  // the distance to that bound will do.
  let hc = clamp(hb / max(htop, 1.0), 0.0, 1.2);
  let dw = dw0 - hb * lean;
  let r = length(vec2f(dw, cx));
  let flare = mature * smoothstep(0.66, 1.0, hc);
  let rmax = R * (select(select(1.65, 1.25, sup > 0.5), 1.48, L2 > 0.0) + select(0.30, 0.58, sup > 0.5) * flare);
  var sd = min(min(rmax - r, htop - hb), hb + 30.0);
  // Well below the cloud base only the wall/shelf cloud can reach; skip the
  // tower, turrets, flanking line and anvil entirely.
  let above_sub = hb > -150.0;
  if (above_sub && sd > -2500.0) {
    let ang = atan2(cx, dw);
    // (Along a squall line its face bulges in and out too, not one straight wall.)
    // (A supercell's lobes wind round it as they climb: the rotating
    // updraft's corkscrew.)
    let twist = sup * hb / 1900.0;
    let lob = 1.0 + (0.11 * sin(2.0 * (ang - twist) + seed * 40.0 + hc * 1.7) + 0.075 * sin(3.0 * (ang - twist) + seed * 73.0 - hc * 2.3)) * (1.0 + 0.45 * sup) + 0.12 * min(L2, 1.0) * sin(cxl / (1.15 * R) + seed * 29.0 + hc * 1.3);
    // Young towers round off into a broad cauliflower dome over their upper half.
    let dm = max(hc - 0.56, 0.0) / 0.44;
    let dome = sqrt(max(1.0 - dm * dm, 0.0));
    // Broad convective base tapering smoothly upward; mature ones flare into the anvil.
    let taper = mix(1.06, 0.80, smoothstep(0.0, 0.88, hc));
    let fq = smoothstep(0.68, 1.0, hc);
    // Its foot tucks in over the lowest few hundred metres, a rounded edge to
    // the base instead of a square corner (a straight dark wall seen from afar).
    let foot = 1.0 - 0.09 * (1.0 - smoothstep(0.0, 750.0, hb));
    // Broader, imposing towers for lone cells, supercells and squall lines alike.
    var Rt = R * select(mix(0.92, 0.80, sup), 0.72, L2 > 0.0) * lob * taper * foot * mix(dome, 1.0 + 1.15 * fq * sqrt(fq), mature);
    if (sup > 0.0) {
      // A supercell's lower third is the rotating updraft (the mesocyclone):
      // a broad, imposing striated bell over the rain-free base, narrowing as
      // it climbs in stacked laminar plates ("mothership").
      let mz = sup * (1.0 - smoothstep(0.28, 0.46, hc));
      let hw = hb + 0.09 * dot(vec2f(dw, cx), vec2f(cos(seed * 6.3), sin(seed * 6.3))) + 260.0 * sin(hb / 1300.0 + seed * 7.0);
      let fb = fract(hw / 680.0 + ang * 0.159155 + seed * 3.0);
      let plate = (smoothstep(0.0, 0.82, fb) - smoothstep(0.82, 1.0, fb)) * (0.55 + 0.45 * sin(ang + seed * 11.0 + hb / 2100.0));
      Rt = mix(Rt, R * mix(0.84, 0.68, smoothstep(0.0, 0.46, hc)) * (0.97 + 0.07 * plate) * foot * (1.0 + 0.06 * sin(2.0 * (ang - twist) + seed * 40.0)), mz);
      g_wall = -mz * (1.0 - smoothstep(0.95 * R, 1.15 * R, r));
    }
    // (The flat base is put on below, once, rounded into the walls.)
    sd = min(Rt - r, select(htop, 0.76 * htop, L2 > 0.0) - hb);
    let deep_core = sd > 1.25 * storm_bulge(sc) + 900.0;
    if (!deep_core && r < R * select(1.75, 1.48, L2 > 0.0) && hb < htop * 0.94 && sup <= 0.5) {
      // Five turrets around the core, smooth-blended into a muscular convective
      // mass instead of separate narrow sticks. Along a squall line, closely
      // overlapping towers form a continuous boiling rampart under the anvil.
      let kt = clamp(R * 0.14, 550.0, 1500.0);
      if (L2 > 0.0) {
        let D = 0.78 * R;
        let nj = floor(L2 / D);
        let j0 = round(clamp(cxl, -L2, L2) / D);
        for (var k = 0; k < 3; k++) {
          let fk = f32(k);
          let jk = clamp(j0 + fk - 1.0, -nj, nj);
          var hp = fract(vec4f(seed * 91.7 + (jk * 1.618 + 5.0) * 12.989) * vec4f(0.1031, 0.1030, 0.0973, 0.1099));
          hp += dot(hp, hp.wzxy + 33.33);
          let hs = fract((hp.xxyz + hp.yzzw) * hp.zywx);
          let rk = R * (0.56 + 0.26 * hs.z);
          let ft = htop * mix(0.72, 1.04, 1.0 - (1.0 - hs.w) * (1.0 - hs.w)) * mix(0.5, 1.0, lend);
          let tc = vec2f(R * (0.28 - 0.50 * hs.w + 0.22 * (hs.y - 0.5)), (jk + 0.55 * (hs.x - 0.5)) * D);
          let dk = length(vec2f(dw, cxl) - tc);
          let ck = max(ft - rk, 0.0);
          let sk = rk * (1.0 - 0.15 * min(hb / max(ck, 1.0), 1.0)) - length(vec2f(dk, max(hb - ck, 0.0)));
          sd = smooth_max(sd, sk, kt);
        }
      } else {
        let base_t = 8u * ci;
        let p_t = vec2f(dw, cxl);
        for (var k = 0u; k < 5u; k++) {
          let t = G.storm_turrets[base_t + k];
          let dk = length(p_t - t.xy);
          let sk = t.z * (1.0 - 0.15 * min(hb / max(t.w, 1.0), 1.0)) - length(vec2f(dk, max(hb - t.w, 0.0)));
          sd = smooth_max(sd, sk, kt);
        }
      }
    }
  }
  // Flanking line: a row of younger feeder towers stepping down upwind and off
  // to one side, smooth-blended into the main convective mass.
  if (L2 <= 0.0 && above_sub && hb < htop * 0.65 + 2200.0 && sd <= 1.25 * storm_bulge(sc) + 900.0) {
    let fc = vec2f(dw0 - hb * 0.1, cx);
    let kf = clamp(R * 0.15, 600.0, 1600.0);
    let base_f = 8u * ci + 5u;
    for (var k = 0u; k < 3u; k++) {
      let t = G.storm_turrets[base_f + k];
      let fh = clamp(hb / max(t.w, 1.0), 0.0, 1.2);
      let rf = length(fc - t.xy);
      let fdm = max(fh - 0.48, 0.0) / 0.52;
      let fd = sqrt(max(1.0 - fdm * fdm, 0.0));
      sd = smooth_max(sd, min(t.z * fd - rf, t.w - hb), kf);
    }
  }
  // The flat base, 30 m under the layer's: every column (core, turrets,
  // flanking towers) rounds off into it over its lowest few hundred metres.
  if (above_sub) {
    let rb = clamp(R * 0.22, 450.0, 1100.0);
    let bb = hb + 30.0;
    sd = max(rb, min(sd, bb)) - length(max(vec2f(rb - sd, rb - bb), vec2f(0.0)));
  }
  // Mature cells: anvil, overshooting top.
  if (mature > 0.0 && above_sub) {
    let th = mix(mix(1800.0, 3800.0, size), 3600.0, sup) * mature * select(1.0, mix(0.45, 1.0, lend), L2 > 0.0);
    let k_anv = clamp(R * 0.20, 1100.0, 2400.0) * mature;
    let ab_min = at - th - 150.0 - 750.0 * sup;
    // Far below the anvil's lowest possible base, neither the anvil nor the
    // overshooting top can affect the SDF near the surface.
    if (hb >= ab_min - k_anv - 900.0) {
      let an = storm_anvil(sc, sp);
      let xa = dw0 - at * lean;
      let lc = an.z + max(xa, 0.0) * mix(0.42, 0.28, sup);
      let ax_s = select(an.y, an.x, xa > 0.0);
      let qn = vec2f(xa / ax_s, cx / lc);
      let rn0 = length(qn);
      // Organic lobed planform via trig-free harmonics so the anvil never reads
      // as a geometric ellipse, skipped outside its radial/height band.
      var rn = rn0;
      let wob_fade = smoothstep(0.25, 0.85, rn0);
      if (rn0 > 0.25 && rn0 < 1.32 && hb > ab_min - k_anv) {
        let u = qn / rn0;
        let ph = seed * 6.2831853;
        let cp = cos(ph);
        let sp_ = sin(ph);
        let ur = vec2f(u.x * cp - u.y * sp_, u.x * sp_ + u.y * cp);
        let c2 = ur.x * ur.x - ur.y * ur.y;
        let s2 = 2.0 * ur.x * ur.y;
        let c3 = c2 * ur.x - s2 * ur.y;
        let s3 = s2 * ur.x + c2 * ur.y;
        let s5 = s3 * c2 + c3 * s2;
        let awob = (0.09 * s2 + 0.06 * c3 + 0.045 * s5) * mix(1.0, 0.55, sup * smoothstep(0.1 * R, -0.4 * R, xa));
        rn = max(rn0 + awob * wob_fade, 0.0);
      }
      let edge = max(1.0 - rn, 0.0);
      var ab = at - th * smoothstep(0.0, 0.48, edge) * (0.78 + 0.22 * edge) - 150.0;
      var atop = at + 420.0 * edge;
      if (edge > 0.0 && hb > ab_min) {
        // Gentle convective undulations on the anvil canopy near the core.
        atop += 180.0 * edge * sin(xa / (0.65 * R) + seed * 13.0) * cos(cx / (0.65 * R) + seed * 29.0);
      }
      if (sup > 0.0) {
        let en = clamp(edge / mix(0.3, 0.22, smoothstep(0.2 * R, -0.5 * R, xa)), 0.0, 1.0);
        let nose = sqrt(en * (2.0 - en));
        ab = mix(ab, at - th * nose * (0.8 + 0.2 * edge) - 150.0, sup);
        atop -= 0.45 * th * (1.0 - nose) * sup;
        if (xa > 0.0 && hb > ab_min - 500.0 && hb < at + 200.0) {
          let mreg = sup * smoothstep(0.06 * an.x, 0.2 * an.x, xa) * (1.0 - smoothstep(0.45 * an.x, 0.8 * an.x, xa)) * smoothstep(0.1, 0.35, edge);
          if (mreg > 0.0) {
            let nz = textureSampleLevel(t_noise, s_repeat, vec3f(dw0, cx, seed * 9000.0) / 7000.0 + vec3f(0.43, 0.19, 0.77), 0.0);
            let pouch = smoothstep(0.42, 0.85, nz.g * 0.75 + nz.b * 0.25);
            ab -= mreg * (300.0 + 450.0 * nz.r) * pouch * (2.0 - pouch);
            g_wall = min(g_wall, -mreg * smoothstep(0.0, 0.25, pouch));
          }
        }
      }
      let lat_scale = max(4500.0, min(ax_s, lc));
      let sa = min(min(hb - ab, atop - hb), (1.0 - rn) * lat_scale);
      // The overshooting top: only rises out of the core near anvil height.
      var so = sd;
      if (hb > 0.70 * at) {
        let jo = round(clamp(cxl, -L2, L2) / (2.6 * R));
        let ro = length(vec2f(dw0 - min(hb, at) * lean, cxl - jo * 2.6 * R));
        let Ro = R * mix(mix(0.45, 0.58, size), 0.44, sup) * sqrt(smoothstep(0.3 * at, 0.75 * at, hb));
        let hf = select(1.0, 0.3 + 0.7 * fract(sin(jo * 7.13 + seed * 31.0) * 43758.5), L2 > 0.0);
        let ho = at + sp.z * hf * sqrt(max(1.0 - (ro / max(Ro, 1.0)) * (ro / max(Ro, 1.0)), 0.0));
        so = max(sd, min(min(ho - hb, Ro - ro), hb - select(0.3, 0.74, L2 > 0.0) * at));
      }
      g_anvil = mature * max(smoothstep(-500.0, 500.0, sa - so), smoothstep(0.82, 0.97, hc) * select(1.0, 0.0, L2 > 0.0)) * (1.0 - sup * smoothstep(at + 100.0, at + 600.0, hb) * smoothstep(-300.0, 300.0, so - sa));
      // Smooth fillet where the rising tower spreads continuously into the anvil.
      sd = smooth_max(so, sa, k_anv);
    }
  }
  // Under the base (only big, mature cells): a wall cloud under the updraft, a
  // little upwind of the core. (A shelf cloud along a lone cell's gust front was
  // tried: as a smooth ring outside the tower it caught full sun and read as a
  // white box. A squall line gets one, below, kept in the anvil's shade.)
  // It hangs as a lumpy lens, deepest in the middle and rising to the base at
  // its rim, its underside in rounded lumps about a km across: as a lobed drum
  // (vertical sides, flat bottom) it stood under the storm as a dark slab with
  // straight edges from every side. The bound takes the deepest lumps.
  // (Only where there is one, and only round it: without the horizontal bound a
  // cell with no wall cloud had sd = 0.8 hb under the base everywhere within
  // its bound circle, and the billow noise grew flat sunlit scraps there that
  // stood out as bright specks against every cloud base around a shower.)
  let low = mature * smoothstep(1500.0, 5000.0, R);
  if (hb < 0.0 && low > 0.0) {
    let line = L2 > 0.0;
    let wv = vec2f(dw0 + R * 0.35, cx);
    let rw = length(wv);
    let wa = atan2(wv.y, wv.x);
    let wl = 1.0 + 0.22 * sin(3.0 * wa + seed * 23.0) + 0.14 * sin(5.0 * wa - seed * 41.0);
    // (A supercell's is narrower, deeper and more sharply cut: a distinct
    // lowered block under the rotating updraft. A tail cloud feeds into it
    // along the base from the rain core's side: tt along it, dt off it.)
    let ww = R * mix(0.5, 0.2, sup) * wl;
    let tb = vec2f(0.6, 0.62) * R;
    let tt = clamp(dot(wv, tb) / dot(tb, tb), 0.0, 1.0);
    let dt = length(wv - tb * tt);
    // A squall line's shelf cloud: u metres back from its leading edge, which
    // bows in and out along the line (storm.rs shelf_u).
    let u = R * (SHELF_F + 0.12 * sin(cxl / 9000.0 + seed * 40.0) + 0.07 * sin(cxl / 3700.0 + seed * 17.0)) - dw0;
    let Ws = SHELF_W * R;
    var lump = 1.0;
    if (select((rw < ww * 1.1 || (sup > 0.0 && dt < 0.15 * R)), (u > -SHELF_AHEAD * R - 900.0 && u < 2.0 * Ws), line)) {
      let nz = textureSampleLevel(t_noise, s_repeat, (pp + G.weather.xyz) / select(3200.0, 1700.0, line) + vec3f(0.27, 0.61, 0.09), 0.0);
      lump = nz.g * 0.65 + nz.b * 0.35;
    }
    var swc = 0.0;
    if (line) {
      // A long wedge hanging under the base ahead of the rain: its nose (the
      // gust front) low down, its smooth upper face rising steeply from it and
      // rounding back into the base in three laminar tiers (steep risers facing
      // the sky ahead, flatter ledges between: the bands seen from in front),
      // its underside rising back slowly, ragged and lumpy, to meet the rain.
      // Fading out at the line's ends. (A face sloping up at ~10 degrees was
      // seen edge-on from anywhere near the ground: a thin sliver. With its
      // underside rising right from the nose, the nose was a knife edge the
      // billows wore through, and the bright sky far behind the line showed
      // through it in specks along the shelf's foot: a rounded nose, its
      // underside lumpy right to the front.)
      let lend2 = 1.0 - smoothstep(L2 - R, L2 + 0.3 * R, abs(cxl));
      let Ds = SHELF_D * low * shelf_depth(cxl, seed) * lend2;
      let x = u / Ws;
      let zbot = -Ds * (1.0 - 0.55 * x * smoothstep(0.0, 0.35, x)) - Ds * 0.3 * (lump - 0.5) * mix(0.4, 1.0, smoothstep(0.0, 0.5, x));
      let ub = u + shelf_bulge(cxl, seed, R);
      let st = shelf_top(ub, R, Ds) - hb;
      let sb = hb - zbot;
      let lat = L2 + 0.3 * R - abs(cxl);
      // (A blunt, rounded nose: tapering to a knife edge at the front, its
      // lowest few hundred metres were thinner than the billows that wear the
      // underside, and the shelf hung a kilometre back and far higher up
      // than meant: a thin band on the horizon.)
      let sw = min(round_min(round_min(st, sb, 120.0), ub, 160.0) * 0.9, min((1.9 * Ws - u) * 0.5, lat));
      // The storm's base does not end at the shelf: it hangs over it as a
      // dark, ragged ceiling a few hundred metres under the base, lumpy
      // underneath, and reaches on a little ahead of the nose before it
      // breaks off in a torn edge. (With the base ending where the towers
      // stand, what showed above the shelf from in front was the cumulus
      // field under the anvil, mottled and lit by the sun in specks.) It
      // runs on up to the base unbounded (a top of its own, a little over
      // the base, left it thin enough in the middle for the billows to wear
      // holes right through it; a slab running on up into the layer cost
      // ~7% wherever a squall line was in view).
      let Hc = SHELF_CEIL * (0.55 + 0.9 * lump) * lend2;
      let sc = min(hb + Hc, min((u + SHELF_AHEAD * R + 900.0 * (lump - 0.5)) * 0.4, min(1.9 * Ws - u, lat)));
      swc = max(sw, sc);
      // (Packed for cloud_shape_f: negative where the smooth upper face (or
      // the blunt front) is the nearer one, which keeps it laminar instead of
      // billowed; above 1 on the underside and the ceiling, which keeps a
      // crisper, ragged edge.)
      let lamf = smoothstep(-120.0, 120.0, min(sb, ub) - st) * smoothstep(-80.0, 80.0, sw - sc);
      let gw = smoothstep(-300.0, 300.0, swc - sd);
      g_wall = gw * (2.0 * (1.0 - lamf) - lamf);
    } else {
      // (A supercell's hangs down to 900-1000 m under the base: at most
      // SHELF_BELOW, as far down as storm rays march.)
      let drop = mix(STORM_BELOW * 0.5, 900.0, sup) * low;
      let s2 = rw * rw / (ww * ww);
      // (A supercell's with steep sides and a flat, lumpy bottom: as a lens
      // it merged into the base, one smooth bulge.)
      let zb = -drop * mix(1.0 - s2, 1.0 - smoothstep(0.5, 1.0, sqrt(s2)), sup) * mix(0.4 + 0.9 * lump, 0.8 + 0.3 * lump, sup);
      swc = min((hb - zb) * 0.8, ww * 1.2 - rw);
      if (sup > 0.0) {
        let zt = -drop * 0.45 * (1.0 - 0.75 * tt) * (0.5 + 0.8 * lump) * sup;
        swc = max(swc, min(R * 0.075 * (1.0 - 0.45 * tt) - dt, (hb - zt) * 0.8));
      }
      // (A supercell's wall cloud keeps its shape: as ragged as an ordinary
      // one, the big storm's billows wore it away to nothing. Mostly
      // laminar, a crisp edge, a little scud.)
      g_wall = smoothstep(-300.0, 300.0, swc - sd) * (1.0 - 1.75 * sup);
    }
    sd = max(sd, swc);
  }
  return sd;
}

// A circle (horizontal) holding all of storm cell i: xyz its centre on the ground
// (planet-centred), w its radius (m). Precomputed once per frame on the CPU
// (storm.rs `bound`) to avoid repeating the trig/normalize math on every ray step.
fn storm_bound(i: u32) -> vec4f { return G.storm_bounds[i]; }
// A squall line's half-length, m (0 for a lone cell): its bound is a capsule,
// that circle drawn out across the wind.
fn storm_len2(i: u32) -> f32 { return floor(G.storm_cells[2u * i + 1u].w) * 500.0; }
// Horizontal offset from a bound's centre c (planet-centred, up its
// direction) to v, the length of a squall line (L2) taken out.
fn storm_hoff(v: vec3f, up: vec3f, L2: f32) -> vec3f {
  var hv = v - up * dot(v, up);
  if (L2 > 0.0) {
    let X = normalize(cross(up, G.storm.xyz));
    hv -= X * clamp(dot(hv, X), -L2, L2);
  }
  return hv;
}

// Where a 2D ray (origin o, direction d) is inside the box |x| <= hx, clipped
// to 0..tmax (x >= y: it never is).
fn ray_box2(o: vec2f, d: vec2f, hx: vec2f, tmax: f32) -> vec2f {
  let inv = 1.0 / select(d, vec2f(1e-9), abs(d) < vec2f(1e-9));
  let ta = (-hx - o) * inv;
  let tb = (hx - o) * inv;
  let t0 = max(max(min(ta.x, tb.x), min(ta.y, tb.y)), 0.0);
  let t1 = min(min(max(ta.x, tb.x), max(ta.y, tb.y)), tmax);
  return vec2f(t0, t1);
}
// Storm cells (cumulonimbus) are picked on the CPU (Weather::storm_cells): a short
// list, nearest first. mask selects which of them to consider (bit i = cell i);
// the cloud march passes only those its ray can meet. Picks the cell whose
// envelope reaches furthest here, so where two cells' anvils meet they merge
// instead of one cutting the other off.
// Ultra-fast conservative upper bound on storm_sd(..., true) for cell picking
// and clear-air step sizing: zero trig, zero loops, zero texture reads, with
// the planet-frame basis (q_s, X) hoisted once per storm_find call.
fn storm_sd_bound(q_s: vec3f, X: vec3f, hb: f32, sc: vec4f, sp: vec4f, ci: u32) -> f32 {
  g_anvil = 0.0;
  if (sc.w <= 0.0) { return -1e6; }
  let R = sc.w * 1000.0;
  let L2 = floor(sp.w) * 500.0;
  let mature = smoothstep(0.5, 0.8, sp.x);
  let sup = storm_super(sp);
  let lean = storm_lean(sp);
  let at = sp.y - G.cloud.y;
  let htop = at * storm_top(sp.x);
  let dv = (q_s - sc.xyz) * 1000.0;
  let dw0 = dot(dv, G.storm.xyz);
  let cxl = dot(dv, X);
  let cx = sign(cxl) * max(abs(cxl) - L2, 0.0);
  let hc = clamp(hb / max(htop, 1.0), 0.0, 1.2);
  let flare = mature * smoothstep(0.66, 1.0, hc);
  let dw = dw0 - hb * lean;
  let r = length(vec2f(dw, cx));
  let rmax = R * (select(select(1.65, 1.25, sup > 0.5), 1.48, L2 > 0.0) + select(0.30, 0.58, sup > 0.5) * flare);
  var sd = min(rmax - r, htop + sp.z - hb);
  if (hb > -150.0) {
    if (L2 <= 0.0 && hb < htop * 0.65 + 2200.0) {
      let fc = vec2f(dw0 - hb * 0.1, cx);
      let kf = clamp(R * 0.15, 600.0, 1600.0);
      let base_f = 8u * ci + 5u;
      for (var k = 0u; k < 3u; k++) {
        let t = G.storm_turrets[base_f + k];
        let sf = min(t.z - length(fc - t.xy), t.w - hb);
        sd = smooth_max(sd, sf, kf);
      }
    }
    sd = min(sd, hb + 30.0);
    if (mature > 0.0) {
      let size = smoothstep(4000.0, 13000.0, R);
      let lend = smoothstep(L2 + 0.5 * R, L2 - 3.0 * R, abs(cxl));
      let th = mix(mix(1800.0, 3800.0, size), 3600.0, sup) * mature * select(1.0, mix(0.45, 1.0, lend), L2 > 0.0);
      let ab = at - th - 150.0 - 750.0 * sup;
      let atop = at + 600.0;
      let an = storm_anvil(sc, sp);
      let xa = dw0 - at * lean;
      let lc = an.z + max(xa, 0.0) * mix(0.42, 0.28, sup);
      let ax_s = select(an.y, an.x, xa > 0.0);
      let rn0 = length(vec2f(xa / ax_s, cx / lc));
      let rn = max(rn0 - 0.195 * smoothstep(0.25, 0.85, rn0), 0.0);
      let sa = min(min(hb - ab, atop - hb), (1.0 - rn) * max(4500.0, min(ax_s, lc)));
      let k_anv = clamp(R * 0.20, 1100.0, 2400.0) * mature;
      if (sa > sd + 400.0) { g_anvil = mature; }
      sd = smooth_max(sd, sa, k_anv);
    }
  } else {
    sd = min(sd, hb + 30.0);
  }
  if (hb < 0.0 && mature > 0.0 && R > 1500.0) {
    var swc = -1e6;
    if (L2 > 0.0) {
      let u0 = R * SHELF_F - dw0;
      let Ws = SHELF_W * R;
      swc = min(min(hb + SHELF_D * 1.15, -hb), min(u0 + (SHELF_AHEAD + 0.25) * R, 1.95 * Ws - u0));
      swc = min(swc, L2 + 0.4 * R - abs(cxl));
    } else {
      let rw = length(vec2f(dw0 + R * 0.35, cx));
      let sub_d = select(680.0, 1080.0, sup > 0.0);
      swc = min(min(hb + sub_d, -hb), select(0.86, 0.96, sup > 0.0) * R - rw);
    }
    if (swc > sd) { g_anvil = 0.0; sd = swc; }
  }
  return sd;
}

const ALL_STORMS = 0xffffffffu;
fn storm_find(pp: vec3f, hb: f32, mask: u32) -> Weather {
  var w = no_weather();
  let n = u32(G.storm.w);
  let valid = select((1u << n) - 1u, 0xffffffffu, n >= 32u);
  var rem = mask & valid;
  if (rem == 0u) {
    g_storm_sd = -1e6;
    g_storm_sd2 = -1e6;
    g_storm_r = 0.0;
    g_anvil = 0.0;
    return w;
  }
  let un = normalize(pp);
  let X = normalize(cross(un, G.storm.xyz));
  let q_s = un * 6371.0 + G.weather.xyz * 0.001;
  let inv_rg = 1.0 / G.planet.w;
  var best = -1e7;
  var best2 = -1e7;
  var best_anv = 0.0;
  var best_r = 0.0;
  var best_i = -1;
  while (rem != 0u) {
    let i = firstTrailingBit(rem);
    rem &= rem - 1u;
    let c = G.storm_cells[2u * i];
    let p = G.storm_cells[2u * i + 1u];
    // Far outside a cell's bounding circle (or well above its top), skip bound SDF.
    let bd = storm_bound(i);
    var v = bd.w - length(storm_hoff(pp - bd.xyz, bd.xyz * inv_rg, floor(p.w) * 500.0));
    let top_i = (p.y - G.cloud.y) * storm_top(p.x) + p.z + 600.0;
    v = min(v, top_i - hb);
    var anv_i = 0.0;
    if (v > max(best, -6000.0)) {
      v = storm_sd_bound(q_s, X, hb, c, p, i);
      anv_i = g_anvil;
    }
    if (v > best) {
      best2 = best;
      best = v;
      best_anv = anv_i;
      best_r = c.w;
      best_i = i32(i);
      if (best > 3000.0) { best2 = 3000.0; break; }
    } else if (v > best2) {
      best2 = v;
    }
  }
  w.cb = smoothstep(-2500.0, 600.0, best);
  g_storm_sd = best;
  g_storm_sd2 = best2;
  g_storm_r = best_r;
  g_anvil = best_anv;
  // Only carry the winning cell into binary search / light march / sky occlusion
  // when close enough to matter: cumulus puffs > 2.6 km below an anvil or away
  // from a tower skip storm_sd completely.
  let keep_d = select(-2600.0, -3500.0, G.nimbo[0].w > 0.0);
  if (best > keep_d) {
    w.ci = best_i;
  }
  return w;
}
// Horizontal cylinder (radius rad around cen, vertical axis up) intersection,
// clipped to [0, tmax]. Returns x >= y on a miss.
fn ray_cyl_h(oc: vec3f, rd: vec3f, cen: vec3f, up: vec3f, rad: f32, tmax: f32) -> vec2f {
  let v = cen - oc;
  let vh = v - up * dot(v, up);
  let rh = rd - up * dot(rd, up);
  let a = dot(rh, rh);
  let b = dot(vh, rh);
  let cc = dot(vh, vh) - rad * rad;
  if (a > 1e-7) {
    let disc = b * b - a * cc;
    if (disc <= 0.0) { return vec2f(1.0, 0.0); }
    let sq = sqrt(disc);
    return vec2f(max((b - sq) / a, 0.0), min((b + sq) / a, tmax));
  }
  return select(vec2f(1.0, 0.0), vec2f(0.0, tmax), cc < 0.0);
}
// Ray interval inside spherical altitude shell [h_lo, h_hi], clipped to [0, tmax].
fn ray_shell(oc: vec3f, rd: vec3f, alt0: f32, h_lo: f32, h_hi: f32, tmax: f32) -> vec2f {
  let rg = G.planet.w;
  let s_hi = ray_sphere(oc, rd, rg + h_hi);
  if (s_hi.y <= 0.0) { return vec2f(1.0, 0.0); }
  var t0 = 0.0;
  var t1 = s_hi.y;
  if (alt0 > h_hi) {
    if (s_hi.x <= 0.0) { return vec2f(1.0, 0.0); }
    t0 = s_hi.x;
    if (h_lo > 0.0) {
      let s_lo = ray_sphere(oc, rd, rg + h_lo);
      if (s_lo.x > 0.0 && s_lo.y >= tmax) { t1 = min(t1, s_lo.x); }
    }
  } else if (alt0 < h_lo) {
    let s_lo = ray_sphere(oc, rd, rg + h_lo);
    if (s_lo.y <= 0.0) { return vec2f(1.0, 0.0); }
    t0 = s_lo.y;
  } else if (h_lo > 0.0) {
    let s_lo = ray_sphere(oc, rd, rg + h_lo);
    if (s_lo.x > 0.0 && s_lo.y >= tmax) { t1 = min(t1, s_lo.x); }
  }
  return vec2f(max(t0, 0.0), min(t1, tmax));
}
// Bit i set when the ray from oc (planet-centred) along rd (up to tmax m)
// passes within reach of storm cell i's 3D cloud envelope: tight tower core,
// oriented flanking-line box, oriented anvil box clipped strictly to its
// altitude shell, and tight sub-cloud wall/shelf cloud volume. Rain shafts
// below the cloud base are tracked separately in g_rain_mask so sub-cloud
// rays that only cross rain never drag the cloud march down below the base.
var<private> g_storm_top: f32;
var<private> g_storm_start: f32;
var<private> g_storm_end: f32;
var<private> g_storm_low: bool;
var<private> g_low_bot: f32;
var<private> g_rain_mask: u32;
fn storm_mask(oc: vec3f, rd: vec3f, tmax: f32) -> u32 {
  var m = 0u;
  g_storm_top = 0.0;
  g_storm_start = tmax;
  g_storm_end = 0.0;
  g_storm_low = false;
  g_low_bot = G.cloud.y;
  g_rain_mask = 0u;
  let n = u32(G.storm.w);
  let rg = G.planet.w;
  let base = G.cloud.y;
  let alt0 = length(oc) - rg;
  let has_nimbo = G.nimbo[0].w > 0.0;
  var has_sub = alt0 < base;
  if (!has_sub && dot(oc, rd) < 0.0) {
    let s_base_ray = ray_sphere(oc, rd, rg + base);
    has_sub = s_base_ray.x > 0.0 && s_base_ray.x < tmax;
  }
  for (var i = 0u; i < n; i++) {
    let sp = G.storm_cells[2u * i + 1u];
    let L2 = floor(sp.w) * 500.0;
    let bd = storm_bound(i);
    let rel_b = bd.xyz - oc;
    let proj = dot(rel_b, rd);
    let rad_b = bd.w + L2 + 22000.0;
    if (proj < -rad_b || proj > tmax + rad_b) { continue; }
    if (dot(rel_b, rel_b) - proj * proj > rad_b * rad_b) { continue; }
    let c = G.storm_cells[2u * i];
    let R = c.w * 1000.0;
    let mature = smoothstep(0.5, 0.8, sp.x);
    let sup = storm_super(sp);
    let lean = storm_lean(sp);
    let at = sp.y - base;
    let htop = at * storm_top(sp.x);
    let bulge = storm_bulge(c);
    let keep = select(0.56 * bulge + 350.0, 3550.0, has_nimbo);
    let h_top = base + htop + sp.z + 0.56 * bulge + 600.0;
    let up0 = normalize(c.xyz - G.weather.xyz * 0.001);
    let cen0 = up0 * rg;
    let X0 = normalize(cross(up0, G.storm.xyz));
    let Y0 = cross(X0, up0);
    var t0_i = tmax;
    var t1_i = 0.0;
    var hit_i = false;
    var top_i = 0.0;
    // 1. Core + turrets (2-tier for lone cells so lower-altitude rays avoid
    //    the upper flared crown cylinder; cloud base - 150 m up to h_top).
    if (L2 > 0.0) {
      let s_ch = ray_shell(oc, rd, alt0, base - 150.0, h_top, tmax);
      if (s_ch.y > s_ch.x) {
        let y_min = -(1.55 * R + keep);
        let y_max = 1.55 * R + htop * lean + keep;
        let cen_c = cen0 + Y0 * (0.5 * (y_min + y_max));
        let v = cen_c - oc;
        let vh = v - up0 * dot(v, up0);
        let rh = rd - up0 * dot(rd, up0);
        var iv = ray_box2(vec2f(dot(-vh, X0), dot(-vh, Y0)), vec2f(dot(rh, X0), dot(rh, Y0)), vec2f(L2 + 1.55 * R + keep, 0.5 * (y_max - y_min)), tmax);
        iv = vec2f(max(iv.x, s_ch.x), min(iv.y, s_ch.y));
        if (iv.y > iv.x) {
          hit_i = true;
          t0_i = min(t0_i, iv.x);
          t1_i = max(t1_i, iv.y);
          top_i = max(top_i, h_top);
        }
      }
    } else {
      let h_mid = base + 0.66 * htop;
      let s_lo = ray_shell(oc, rd, alt0, base - 150.0, h_mid, tmax);
      if (s_lo.y > s_lo.x) {
        let sh_lo = 0.33 * htop * lean;
        let r_lo = select(1.65, 1.25, sup > 0.5) * R + sh_lo + keep;
        var iv = ray_cyl_h(oc, rd, cen0 + Y0 * sh_lo, up0, r_lo, tmax);
        iv = vec2f(max(iv.x, s_lo.x), min(iv.y, s_lo.y));
        if (iv.y > iv.x) {
          hit_i = true;
          t0_i = min(t0_i, iv.x);
          t1_i = max(t1_i, iv.y);
          top_i = max(top_i, h_mid);
        }
      }
      let s_hi = ray_shell(oc, rd, alt0, h_mid, h_top, tmax);
      if (s_hi.y > s_hi.x) {
        let sh_hi = 0.83 * htop * lean;
        let r_hi = select(1.96, 1.85, sup > 0.5) * R + 0.17 * htop * lean + max(at - htop, 0.0) * lean + keep;
        var iv = ray_cyl_h(oc, rd, cen0 + Y0 * sh_hi, up0, r_hi, tmax);
        iv = vec2f(max(iv.x, s_hi.x), min(iv.y, s_hi.y));
        if (iv.y > iv.x) {
          hit_i = true;
          t0_i = min(t0_i, iv.x);
          t1_i = max(t1_i, iv.y);
          top_i = max(top_i, h_top);
        }
      }
    }
    // 2. Flanking line (lone cells only): tight oriented box along fl = (-0.75, -0.66).
    if (L2 <= 0.0) {
      let fh0 = htop * 0.64 * mix(0.65, 1.0, mature) + 450.0;
      let h_fl_hi = base + fh0 + keep;
      let s_fh = ray_shell(oc, rd, alt0, base - 150.0, h_fl_hi, tmax);
      if (s_fh.y > s_fh.x) {
        let fl_w = normalize(Y0 * (-0.75) + X0 * (-0.66));
        let fl_p = cross(up0, fl_w);
        let cen_fl = cen0 + fl_w * (1.82 * R) + Y0 * (0.05 * fh0);
        let v = cen_fl - oc;
        let vh = v - up0 * dot(v, up0);
        let rh = rd - up0 * dot(rd, up0);
        let hx = vec2f(1.90 * R + 0.05 * fh0 + keep, (0.32 + 0.58 * (1.0 - 0.35 * sup)) * R + 0.05 * fh0 + keep);
        var iv = ray_box2(vec2f(dot(-vh, fl_w), dot(-vh, fl_p)), vec2f(dot(rh, fl_w), dot(rh, fl_p)), hx, tmax);
        iv = vec2f(max(iv.x, s_fh.x), min(iv.y, s_fh.y));
        if (iv.y > iv.x) {
          hit_i = true;
          t0_i = min(t0_i, iv.x);
          t1_i = max(t1_i, iv.y);
          top_i = max(top_i, base + fh0 + 0.56 * bulge + 400.0);
        }
      }
    }
    // 3. Anvil canopy (mature cells only): tight oriented 2D box clipped to altitude shell.
    if (mature > 0.0) {
      let size = smoothstep(4000.0, 13000.0, R);
      let th_max = mix(mix(1800.0, 3800.0, size), 3600.0, sup) * mature;
      let k_anv = clamp(R * 0.20, 1100.0, 2400.0) * mature;
      let ab_min = at - th_max - 150.0 - 750.0 * sup;
      let sreach_anv = 0.09 * bulge + 100.0;
      let low_anv = has_nimbo && (base + ab_min - 3550.0 < base + G.cloud.z);
      let m_anv = max(sreach_anv + 0.25 * k_anv, select(0.0, 3550.0, low_anv)) + 200.0;
      let h_anv_lo = base + ab_min - m_anv;
      let h_anv_hi = base + at + 600.0 + sreach_anv;
      let s_ah = ray_shell(oc, rd, alt0, h_anv_lo, h_anv_hi, tmax);
      if (s_ah.y > s_ah.x) {
        let an = storm_anvil(c, sp);
        let y_min = at * lean - 1.20 * an.y - m_anv;
        let y_max = at * lean + 1.20 * an.x + m_anv;
        let x_half = L2 + 1.20 * an.z + 0.72 * an.x * mix(0.42, 0.28, sup) + m_anv;
        let cen_a = cen0 + Y0 * (0.5 * (y_min + y_max));
        let v = cen_a - oc;
        let vh = v - up0 * dot(v, up0);
        let rh = rd - up0 * dot(rd, up0);
        var iv = ray_box2(vec2f(dot(-vh, X0), dot(-vh, Y0)), vec2f(dot(rh, X0), dot(rh, Y0)), vec2f(x_half, 0.5 * (y_max - y_min)), tmax);
        iv = vec2f(max(iv.x, s_ah.x), min(iv.y, s_ah.y));
        if (iv.y > iv.x) {
          hit_i = true;
          t0_i = min(t0_i, iv.x);
          t1_i = max(t1_i, iv.y);
          top_i = max(top_i, h_anv_hi);
        }
      }
    }
    // 4. Sub-cloud wall / shelf cloud (only when the ray enters h < base).
    if (has_sub && mature > 0.0 && R > 1500.0) {
      let sub_d = select(min(select(680.0, 1080.0, sup > 0.0) + 0.08 * bulge + 320.0, SHELF_BELOW), SHELF_BELOW, L2 > 0.0);
      let sub_bot = base - sub_d;
      let s_sub = ray_shell(oc, rd, alt0, sub_bot, base, tmax);
      if (s_sub.y > s_sub.x) {
        var iv = vec2f(1.0, 0.0);
        if (L2 > 0.0) {
          let y_min = R * (SHELF_F - 1.9 * SHELF_W - 0.22) - 600.0;
          let y_max = R * (SHELF_F + SHELF_AHEAD + 0.22) + 1200.0;
          let cen_s = cen0 + Y0 * (0.5 * (y_min + y_max));
          let v = cen_s - oc;
          let vh = v - up0 * dot(v, up0);
          let rh = rd - up0 * dot(rd, up0);
          iv = ray_box2(vec2f(dot(-vh, X0), dot(-vh, Y0)), vec2f(dot(rh, X0), dot(rh, Y0)), vec2f(L2 + 0.4 * R + 500.0, 0.5 * (y_max - y_min)), tmax);
        } else {
          let r_w = select(0.86, 0.96, sup > 0.0) * R + 0.08 * bulge + 380.0;
          iv = ray_cyl_h(oc, rd, cen0 - Y0 * (0.35 * R), up0, r_w, tmax);
        }
        iv = vec2f(max(iv.x, s_sub.x), min(iv.y, s_sub.y));
        if (iv.y > iv.x) {
          hit_i = true;
          g_storm_low = true;
          g_low_bot = min(g_low_bot, sub_bot);
          t0_i = min(t0_i, iv.x);
          t1_i = max(t1_i, iv.y);
          top_i = max(top_i, base);
        }
      }
    }
    // 5. Sub-cloud rain shaft (tracked separately in g_rain_mask).
    if (has_sub && htop > 1800.0 && R > 800.0) {
      let s_rn = ray_shell(oc, rd, alt0, -500.0, base, tmax);
      if (s_rn.y > s_rn.x) {
        let cen_r = cen0 + G.storm.xyz * (R * mix(0.45, 0.32, sup)) + X0 * (R * 0.28 * sup);
        var iv = vec2f(1.0, 0.0);
        if (L2 > 0.0) {
          let v = cen_r - oc;
          let vh = v - up0 * dot(v, up0);
          let rh = rd - up0 * dot(rd, up0);
          iv = ray_box2(vec2f(dot(-vh, X0), dot(-vh, Y0) + 1.4 * R), vec2f(dot(rh, X0), dot(rh, Y0)), vec2f(L2 + 1.2 * R, 2.6 * R + base * 0.15), tmax);
        } else {
          let rt = R * mix(0.8, 0.42, sup) * 1.75 + base * 0.15;
          iv = ray_cyl_h(oc, rd, cen_r, up0, rt, tmax);
        }
        iv = vec2f(max(iv.x, s_rn.x), min(iv.y, s_rn.y));
        if (iv.y > iv.x) { g_rain_mask |= 1u << i; }
      }
    }
    if (hit_i) {
      m |= 1u << i;
      g_storm_start = min(g_storm_start, t0_i);
      g_storm_end = max(g_storm_end, t1_i);
      g_storm_top = max(g_storm_top, top_i);
    }
  }
  return m;
}
// Pack the slowly-varying ordinary cloud weather (16-1700 km scales) into a
// single u32 so the cloud march and shadow map can reuse it across short steps
// (< 400 m) without re-sampling four 3D textures every 40 m.
fn cloud_ord_pack(pp: vec3f, hb: f32) -> u32 {
  let un = normalize(pp);
  let q = un * 6371.0;  // km on the sphere
  let w = G.weather.xyz * 0.001;
  let big = textureSampleLevel(t_noise, s_repeat, q / 1700.0 + vec3f(0.13, 0.71, 0.37), 0.0);
  let mid = textureSampleLevel(t_noise, s_repeat, (q + w) / 173.0, 0.0);
  let cell = textureSampleLevel(t_noise, s_repeat, (q + w) / 29.0 + vec3f(0.53, 0.21, 0.87), 0.0);
  let c = G.cloud.x + (big.r - 0.5) * 0.4 + (mid.r - 0.5) * 0.45 + (cell.g - 0.62) * 0.2;
  let c0 = clamp(c, 0.0, 1.0);
  let typ = clamp(1.0 - smoothstep(0.68, 0.98, c0) + (big.g - 0.62) * 0.6, 0.0, 1.0);
  let tower = smoothstep(0.25, 0.95, c0 * 0.75 + cell.r * 0.55 - 0.1) * (1.0 - 0.75 * smoothstep(0.7, 0.92, c0));
  let cu_top = 0.28 + 0.72 * tower * tower;
  let top = mix(mix(0.1, 0.22, smoothstep(0.0, 0.5, typ)), cu_top, smoothstep(0.4, 1.0, typ));
  var cellv = 0.0;
  if (typ < 0.9 && (hb < G.cloud.z * top + 400.0 || G.nimbo[0].w > 0.0)) {
    cellv = textureSampleLevel(t_noise, s_repeat, (pp + G.weather.xyz - un * (hb * 0.85)) / 16000.0 + vec3f(0.41, 0.77, 0.19), 0.0).g;
  }
  return pack4x8unorm(vec4f(clamp(c * 0.5 + 0.25, 0.0, 1.0), typ, top, cellv));
}
fn cloud_weather_unpack(pp: vec3f, hs: f32, mask: u32, packed: u32) -> Weather {
  g_storm_sd = -1e6;
  g_storm_sd2 = -1e6;
  g_storm_r = 0.0;
  var st = no_weather();
  if (mask != 0u && G.storm.w > 0.0) { st = storm_find(pp, hs, mask); }
  if (hs >= G.cloud.z || hs < 0.0) { return Weather(0.0, 0.0, 0.0, st.cb, st.ci, 0.0, 0.0); }
  let u = unpack4x8unorm(packed);
  let c = u.x * 2.0 - 0.5;
  var ns = 0.0;
  if (G.nimbo[0].w > 0.0) { ns = nimbo_area(pp); }
  let cov = clamp(mix(c * (1.0 - st.cb), 1.1, ns), 0.0, 1.0);
  let typ = max(u.y, st.cb * 0.5) * (1.0 - ns);
  var top = u.z;
  if (ns > 0.0) { top = mix(top, clamp((G.nimbo[2].w - G.cloud.y) / G.cloud.z, 0.05, 1.0), ns); }
  return Weather(cov, typ, top, st.cb, st.ci, u.w, ns);
}
fn cloud_weather(pp: vec3f) -> Weather { return cloud_weather_m(pp, ALL_STORMS); }
fn cloud_weather_m(pp: vec3f, mask: u32) -> Weather {
  let hs = length(pp) - G.planet.w - G.cloud.y;
  g_storm_sd = -1e6;
  g_storm_sd2 = -1e6;
  g_storm_r = 0.0;
  var st = no_weather();
  if (mask != 0u && G.storm.w > 0.0) { st = storm_find(pp, hs, mask); }
  // Above the ordinary layer (or under its base) only storm cells reach.
  if (hs >= G.cloud.z || hs < 0.0) { return Weather(0.0, 0.0, 0.0, st.cb, st.ci, 0.0, 0.0); }
  let un = normalize(pp);
  let q = un * 6371.0;  // km on the sphere
  let w = G.weather.xyz * 0.001;
  let hb = max(hs, 0.0);
  // The rain deck closes the sky over its area (its edge breaks up as the
  // cover there allows).
  var ns = 0.0;
  if (G.nimbo[0].w > 0.0) { ns = nimbo_area(pp); }
  let big = textureSampleLevel(t_noise, s_repeat, q / 1700.0 + vec3f(0.13, 0.71, 0.37), 0.0);
  let mid = textureSampleLevel(t_noise, s_repeat, (q + w) / 173.0, 0.0);
  let cell = textureSampleLevel(t_noise, s_repeat, (q + w) / 29.0 + vec3f(0.53, 0.21, 0.87), 0.0);
  // Regional coverage: the slider, pushed around by weather systems and clearings.
  let c = G.cloud.x + (big.r - 0.5) * 0.4 + (mid.r - 0.5) * 0.45 + (cell.g - 0.62) * 0.2;
  // Storm towers take over their surroundings (and the air around them sinks):
  // no ordinary cumulus inside or against them.
  let cov = clamp(mix(c * (1.0 - st.cb), 1.1, ns), 0.0, 1.0);
  // Broken skies grow cumulus; as it closes in the cloud flattens into stratocumulus, then stratus.
  let typ = clamp(1.0 - smoothstep(0.68, 0.98, cov) + (big.g - 0.62) * 0.6, 0.0, 1.0) * (1.0 - ns);
  // How tall the cumulus in this area grow: mostly fair-weather puffs, some towers.
  let tower = smoothstep(0.25, 0.95, cov * 0.75 + cell.r * 0.55 - 0.1) * (1.0 - 0.75 * smoothstep(0.7, 0.92, cov));
  let cu_top = 0.28 + 0.72 * tower * tower;
  var top = mix(mix(0.1, 0.22, smoothstep(0.0, 0.5, typ)), cu_top, smoothstep(0.4, 1.0, typ));
  // The deck reaches up to its own top.
  if (ns > 0.0) { top = mix(top, clamp((G.nimbo[2].w - G.cloud.y) / G.cloud.z, 0.05, 1.0), ns); }
  // Stratocumulus decks are a mosaic of cells 1-2 km across (Worley, columnar so each
  // cell is one lump). Looked up per column, so light probes reuse it.
  var cellv = 0.0;
  if (typ < 0.9 && (hb < G.cloud.z * top || ns > 0.0)) { cellv = textureSampleLevel(t_noise, s_repeat, (pp + G.weather.xyz - un * (hb * 0.85)) / 16000.0 + vec3f(0.41, 0.77, 0.19), 0.0).g; }
  return Weather(cov, typ, top, st.cb, st.ci, cellv, ns);
}
// Height of a column's cloud above the base, metres: storm cores reach their anvil.
fn column_top(wx: Weather) -> f32 {
  if (wx.ci < 0) { return G.cloud.z * wx.top; }
  let sp = G.storm_cells[2u * u32(wx.ci) + 1u];
  return mix(G.cloud.z * wx.top, (sp.y - G.cloud.y) * storm_top(sp.x), smoothstep(0.3, 0.8, wx.cb));
}
// Cloud density (0..1) given the weather of its column. detail = erode the edges with
// fine noise; fine = how much of the close-up octave to add (0 when far away).
// How much of the erosion to apply (the cloud march fades it out with distance:
// cells a few pixels across aliased into worm-like ridges on distant towers).
var<private> g_erode: f32 = 1.0;
// Set by the cloud shadow map: without detail, erode by the erosion noise's mean
// (EF_MEAN) instead of not at all. Uneroded, every cloud cast the shadow of its
// whole envelope (the band the erosion carves away is most of a thin
// puff), so shadows stood bigger and darker than the clouds above them. Costs
// no texture reads.
var<private> g_mean_erode: bool = false;
// Mean of the erosion noise (its Worley fbm, noise3d.wgsl GBA channels).
const EF_MEAN = 0.48;
// The rain deck rises against a storm tower's foot (m, within this far of its
// envelope), a broad swell instead of a flat sheet butting into a wall.
const DECK_SWELL = 650.0;
const DECK_SWELL_D = 3500.0;
// How far the rain deck swells up (m) at sd (m) from a storm tower's envelope:
// highest against the tower, falling away from it (a fillet, not a
// flat-topped plateau, which stood round the tower as a pale skirt), and
// heaving up in lumps (the deck's cells), not as one smooth ramp.
fn deck_swell(sd: f32, wx: Weather) -> f32 {
  let sx = 1.0 - clamp(-sd / DECK_SWELL_D, 0.0, 1.0);
  return DECK_SWELL * wx.ns * sx * sx * mix(0.35, 1.5, wx.cell);
}
fn cloud_shape_f(pp: vec3f, h: f32, wx: Weather, detail: bool, fine: f32) -> f32 {
  // (hs: above the layer's base, for the storms; hb: above this column's base.
  // Under the rain deck the layer reaches down to its scud; elsewhere the
  // ordinary cloud keeps to the base set for it.)
  let hs = h - G.cloud.y;
  if (hs <= -SHELF_BELOW || h >= G.cloud.w) { return 0.0; }
  let hb = hs - (1.0 - wx.ns) * G.nimbo[3].y;
  // The storm envelope is re-evaluated here, not taken from the column: towers are
  // only a few km across, so light probes leave them quickly. Skip the noise where
  // even the biggest turret bulge cannot reach it.
  var storm = false;
  var sd = -1e6;
  var bulge = 0.0;
  var anv0 = 0.0;
  var sc = vec4f(0.0);
  var sp = vec4f(0.0);
  if (wx.ci >= 0 && !g_skip_storm) {
    let ci = u32(wx.ci);
    sc = G.storm_cells[2u * ci];
    sp = G.storm_cells[2u * ci + 1u];
    sd = storm_sd(pp, hs, ci);
    g_storm_sd_full = sd;
    anv0 = g_anvil;
    // The envelope's distance goes out packed with the anvil weight, for the
    // cloud march to tell the rain deck from the towers standing in it by
    // (16 m steps, out to 4 km). (+256: a supercell's laminar rotating updraft.)
    g_anvil = round(clamp(-sd, 0.0, 4080.0) * 0.0625) + 0.99 * anv0 + select(0.0, 256.0, g_wall < -0.3 && sp.x > 1.0 && hs < 0.5 * (sp.y - G.cloud.y));
    bulge = storm_bulge(sc);
    // Tighter reach on the smooth anvil than on the billowing flanks skips
    // ~1 km of empty-air noise fetches above and below wide anvils.
    let sreach = select(mix(0.56, 0.14, anv0), 0.08, hs < 0.0) * bulge + mix(320.0, 180.0, anv0);
    storm = sd > -sreach;
  }
  let ctop0 = G.cloud.z * wx.top;
  var swell = 0.0;
  if (wx.ns > 0.0 && sd > -DECK_SWELL_D) { swell = deck_swell(sd, wx); }
  let hn = hb / max(ctop0 + swell, 1.0);  // 0 at the base, 1 at the top of this column
  let cumulus = hb > 0.0 && hn < 1.0 && wx.cov > 0.02;
  if (!storm && !cumulus) { return 0.0; }
  let wind = G.weather.xyz;
  // Seen from far above, single puffs are sub-pixel: a smooth field is enough.
  let far = smoothstep(40000.0, 400000.0, G.up.w);
  let smooth_d = wx.cov * wx.cov * 0.5 * smoothstep(0.0, 0.2, hn) * smoothstep(1.0, 0.6, hn);
  if (far >= 1.0) { return smooth_d; }
  // Inside storm cells the same stretch is used above and below the layer top
  // (the column type switches off there), or the tower shows a seam at that height.
  let tyc = max(wx.typ, wx.cb);
  // Cumulus (cuw = 1) and stratocumulus / stratus take different noise; both only
  // where the types blend.
  let cuw = select(0.0, smoothstep(0.5, 0.8, wx.typ), cumulus);
  let un = normalize(pp);
  var n = vec4f(0.5);
  if (cuw < 1.0 || storm) {
    // The base shape, stretched upwards (half the vertical frequency) as it turns
    // cumulus-like, so a lump keeps its footprint as it climbs.
    n = textureSampleLevel(t_noise, s_repeat, (pp + wind - un * (max(hb, 0.0) * 0.5 * tyc)) / CLOUD_SHAPE, 0.0);
  }
  let fbm = n.g * 0.625 + n.b * 0.25 + n.a * 0.125;
  let base = remap(n.r, fbm - 1.0, 1.0, 0.0, 1.0);
  // Pre-test cumulus puff threshold before reading the warp texture, so ONE
  // single early warp lookup serves cumulus, nimbo, and storm-base branches
  // while still skipping the 3D fetch on clear-air cumulus steps.
  var cu_diff = -1.0;
  var cu_hk = 1.0;
  if (cuw > 0.0) {
    let m = textureSampleLevel(t_noise, s_repeat, (pp + wind - un * (hb * 0.8)) / CU_SHAPE + vec3f(0.29, 0.53, 0.71), 0.0);
    let puff = 1.0 - (1.0 - m.g) * (1.0 - m.g) * 1.15;
    let ch = pow(smoothstep(0.3, 0.8, m.r), 1.6);
    let hc = G.cloud.z * clamp(wx.top * mix(0.3, 1.7, ch), 0.07, 1.0);
    cu_hk = hb / hc;
    if (cu_hk < 1.0) {
      let p0 = mix(0.8, 0.28, wx.cov) - (m.r - 0.5) * 0.2;
      cu_diff = puff - (p0 + (1.0 - p0) * pow(cu_hk, mix(2.4, 1.7, ch)));
    }
  }
  let lowz = select(0.0, (1.0 - smoothstep(0.0, 1500.0, hs)) * smoothstep(-STORM_BELOW, -STORM_BELOW + 250.0, hs), storm);
  let need_wv_pre = (cu_diff > -0.20) || (cumulus && cuw < 1.0 && wx.ns > 0.0) || (lowz > 0.0 && anv0 < 0.99);
  var wv = select(vec3f(0.0), vec3f(EF_MEAN - 0.62), g_mean_erode);
  var have_wv = !detail;
  if (detail && need_wv_pre) {
    wv = textureSampleLevel(t_noise, s_repeat, (pp + wind * 1.15) / 2900.0 + vec3f(0.31, 0.62, 0.17), 0.0).gba - 0.62;
    have_wv = true;
  }
  var d = 0.0;
  if (cumulus && cuw < 1.0) {
    let stw = 1.0 - smoothstep(0.45, 0.9, wx.typ);
    var tl = mix(1.0, mix(0.55, 1.0, smoothstep(0.25, 0.8, wx.cell)), stw);
    var bv = mix(base, base * 0.5 + wx.cell * 0.55 + 0.05, stw);
    var bottom = smoothstep(0.0, mix(150.0, 70.0, wx.typ), hb);
    var tf = tl * 0.7;
    if (wx.ns > 0.0) {
      tl = mix(tl, 0.9 + 0.1 * wx.cell, wx.ns);
      tf = mix(tf, tl - 0.12, wx.ns);
      let rag = G.nimbo[3].z;
      let hr = h - (G.nimbo[1].w - rag);
      let rb = rag * clamp(0.6 + (0.5 - base) * 1.4 - (wv.x * 0.7 + wv.y * 0.3) * 1.8 + (0.5 - wx.cell) * 0.5, 0.0, 1.0);
      bottom = mix(bottom, smoothstep(rb, rb + 260.0, hr), wx.ns);
      bv = mix(bv, 0.6 + 0.4 * bv, wx.ns);
    }
    let dome = 1.0 - pow(hn, mix(3.0, 1.6, wx.typ));
    let prof = bottom * mix(smoothstep(tl, tf, hn), dome, wx.typ);
    let cv = mix(min(wx.cov * 1.15, 1.0), wx.cov, wx.typ);
    d = remap(bv, min(1.0 - cv * prof, 0.999), 1.0, 0.0, 1.0) * (1.0 - cuw);
    if (wx.ns > 0.0 && sd > -300.0) { d *= 1.0 - smoothstep(-300.0, 300.0, sd) * wx.ns; }
  }
  if (cu_diff > -0.20) {
    let bottom = smoothstep(0.0, 60.0, hb);
    let lobe = (wv.x * 0.7 + wv.y * 0.3) * 0.5 * smoothstep(0.05, 0.3, cu_hk);
    d = max(d, clamp((cu_diff + lobe) / 0.5, 0.0, 1.0) * bottom * cuw);
  }
  // Storm cell: the smooth envelope pushed in and out by billowy turrets
  // (Worley bulges ~2.5-3.5 km and ~1 km), strongest on the tower's flanks and
  // overshooting top; the anvil keeps subtle glaciated structure and fibrous edges.
  var anvil = 0.0;
  if (storm) {
    let at = sp.y - G.cloud.y;
    anvil = anv0;
    let rel = (un * 6371.0 + G.weather.xyz * 0.001 - sc.xyz) * 1000.0 + un * hs;
    let tq = (pp + wind) / 20000.0 + rel * ((1.0 / clamp(sc.w / 7.0, 0.45, 1.35) - 1.0) / 20000.0);
    let tn = textureSampleLevel(t_noise, s_repeat, tq + vec3f(0.71, 0.33, 0.12), 0.0);
    let tw = tn.g * 0.5 + tn.b * 0.3 + n.g * 0.2;
    let bl = 1.0 - (1.0 - tw) * (1.0 - tw);
    let under = 1.0 - smoothstep(-200.0, 0.0, hs);
    let hcf = hs / max(at * storm_top(sp.x), 1.0);
    let wallq = (1.0 - smoothstep(0.4, 0.6, sp.x)) * smoothstep(0.0, 400.0, hs + 30.0 - sd);
    let flank = max(smoothstep(0.08, 0.3, hcf), wallq) * (1.0 - smoothstep(0.6, 0.84, hcf)) * (1.0 - anvil);
    let bo = mix(bl, base * 0.6 + bl * 0.4, under) - 0.78;
    let lam = max(-g_wall, 0.0);
    let lineu = max(g_wall - 1.0, 0.0);
    var sdn = sd + bo * bulge * mix(1.3, 0.36, anvil) * (1.0 + 0.9 * flank * step(0.0, bo)) * (1.0 - 0.85 * lam) * select(mix(0.3, 1.0, max(smoothstep(0.0, 2000.0, hs), wallq)), mix(0.1, 0.25, min(abs(g_wall), 1.0)) * (1.0 - 0.15 * lineu), hs < 0.0);
    if (lowz > 0.0 && anvil < 0.99 && lam < 0.99) {
      sdn += (wv.x * 0.7 + wv.y * 0.3 + 0.08) * 650.0 * lowz * (1.0 - anvil) * (1.0 - 0.9 * lam) * (1.0 - 0.3 * lineu);
    }
    let env = smoothstep(0.0, mix(mix(mix(mix(220.0, 380.0, anvil), 700.0, under), 150.0, lam), 120.0, lineu), sdn);
    let body = mix(0.6 + base * 0.6, 0.55 + base * 0.35, anvil);
    d = max(d, mix(remap(body * env, 0.2, 1.0, 0.0, 1.0), body * env, anvil));
  }
  if (d <= 0.0) { return smooth_d * far; }
  let edge_w = mix(0.22, 0.16, tyc);
  if (detail || g_mean_erode) {
    let erode_k = mix(mix(0.5, 0.62, tyc), 0.6, cuw) * mix(1.0, 0.50, anvil) * mix(0.35, 1.0, g_erode) * (1.0 - 0.7 * max(-g_wall, 0.0) * select(0.0, 1.0, storm));
    // Exact saturation check: whenever d >= erode_k + edge_w * (1 - erode_k),
    // even maximum erosion (ed = 1) leaves smoothstep(0, edge_w, remap(d, ...))
    // identically 1.0, so all detail/anvil/fine 3D texture reads can be skipped.
    if (d < erode_k + edge_w * (1.0 - erode_k)) {
      var ef = EF_MEAN;
      if (detail) {
        if (!have_wv) {
          wv = textureSampleLevel(t_noise, s_repeat, (pp + wind * 1.15) / 2900.0 + vec3f(0.31, 0.62, 0.17), 0.0).gba - 0.62;
        }
        // Curl-like warp so the erosion swirls instead of looking like stamped noise.
        let pe = pp + wind * 1.3 + wv * 420.0;
        let e = textureSampleLevel(t_noise, s_repeat, pe / CLOUD_DETAIL, 0.0);
        ef = e.g * 0.625 + e.b * 0.25 + e.a * 0.125;
        // In the anvil the ice is drawn out along the wind into fibres.
        if (anvil > 0.02) {
          let ps = pe - G.storm.xyz * (dot(pe, G.storm.xyz) * 0.75);
          let ea = textureSampleLevel(t_noise, s_repeat, ps / CLOUD_DETAIL + vec3f(0.37, 0.11, 0.73), 0.0);
          ef = mix(ef, ea.g * 0.5 + ea.b * 0.3 + ea.a * 0.2, anvil);
        }
        if (fine > 0.0) {
          // Close up, a finer octave keeps the billows crisp instead of going soft.
          let e2 = textureSampleLevel(t_noise, s_repeat, pe / CLOUD_FINE + vec3f(0.17, 0.59, 0.43), 0.0);
          ef = mix(ef, ef * 0.72 + (e2.g * 0.7 + e2.b * 0.3) * 0.28, fine);
        }
      }
      let low = select(1.0 - smoothstep(0.02, 0.13, hn), 1.0 - smoothstep(-STORM_BELOW, 300.0, hs), storm);
      let wisp = max(low * 0.8, anvil);
      let ed = mix(1.0 - ef, ef, wisp);
      d = remap(d, ed * erode_k, 1.0, 0.0, 1.0);
    } else {
      d = 1.0;
    }
  }
  // Real cloud edges are abrupt: a few tens of metres from clear air to dense.
  d = smoothstep(0.0, edge_w, d) * mix(0.55, 1.0, max(wx.typ, min(wx.cb * 4.0, 1.0)));
  // Anvil ice scatters less per metre than the tower's water droplets: light gets
  // deeper in, so the sheet glows instead of hanging as a dark slab.
  d *= 1.0 - 0.5 * anvil;
  return mix(d, smooth_d, far);
}
fn cloud_shape(pp: vec3f, h: f32, wx: Weather, detail: bool) -> f32 { return cloud_shape_f(pp, h, wx, detail, 0.0); }
// Specialized ordinary-cloud density evaluator for zero-storm, zero-rain-deck
// scenes (clouds.wgsl `fs_plain` and `cshadow_ray_plain`): identical cumulus /
// stratocumulus / stratus math without compiling storm_sd or nimbo into the
// pipeline, halving register footprint and boosting GPU occupancy.
// ow: vec4f(cov, typ, top, cellv).
fn cloud_shape_plain(pp: vec3f, h: f32, ow: vec4f, detail: bool, fine: f32) -> f32 {
  let hb = h - G.cloud.y;
  if (hb <= 0.0 || ow.x <= 0.02) { return 0.0; }
  let hn = hb / max(G.cloud.z * ow.z, 1.0);
  if (hn >= 1.0) { return 0.0; }
  let far = smoothstep(40000.0, 400000.0, G.up.w);
  let smooth_d = ow.x * ow.x * 0.5 * smoothstep(0.0, 0.2, hn) * smoothstep(1.0, 0.6, hn);
  if (far >= 1.0) { return smooth_d; }
  let wind = G.weather.xyz;
  let cuw = smoothstep(0.5, 0.8, ow.y);
  let un = normalize(pp);
  var cu_diff = -1.0;
  var cu_hk = 1.0;
  if (cuw > 0.0) {
    let m = textureSampleLevel(t_noise, s_repeat, (pp + wind - un * (hb * 0.8)) / CU_SHAPE + vec3f(0.29, 0.53, 0.71), 0.0);
    let puff = 1.0 - (1.0 - m.g) * (1.0 - m.g) * 1.15;
    let ch = pow(smoothstep(0.3, 0.8, m.r), 1.6);
    let hc = G.cloud.z * clamp(ow.z * mix(0.3, 1.7, ch), 0.07, 1.0);
    cu_hk = hb / hc;
    if (cu_hk < 1.0) {
      let p0 = mix(0.8, 0.28, ow.x) - (m.r - 0.5) * 0.2;
      cu_diff = puff - (p0 + (1.0 - p0) * pow(cu_hk, mix(2.4, 1.7, ch)));
    }
  }
  var wv = select(vec3f(0.0), vec3f(EF_MEAN - 0.62), g_mean_erode);
  var have_wv = !detail;
  if (detail && cu_diff > -0.20) {
    wv = textureSampleLevel(t_noise, s_repeat, (pp + wind * 1.15) / 2900.0 + vec3f(0.31, 0.62, 0.17), 0.0).gba - 0.62;
    have_wv = true;
  }
  var d = 0.0;
  if (cuw < 1.0) {
    let n = textureSampleLevel(t_noise, s_repeat, (pp + wind - un * (hb * 0.5 * ow.y)) / CLOUD_SHAPE, 0.0);
    let fbm = n.g * 0.625 + n.b * 0.25 + n.a * 0.125;
    let base = remap(n.r, fbm - 1.0, 1.0, 0.0, 1.0);
    let stw = 1.0 - smoothstep(0.45, 0.9, ow.y);
    let tl = mix(1.0, mix(0.55, 1.0, smoothstep(0.25, 0.8, ow.w)), stw);
    let bv = mix(base, base * 0.5 + ow.w * 0.55 + 0.05, stw);
    let bottom = smoothstep(0.0, mix(150.0, 70.0, ow.y), hb);
    let dome = 1.0 - pow(hn, mix(3.0, 1.6, ow.y));
    let prof = bottom * mix(smoothstep(tl, tl * 0.7, hn), dome, ow.y);
    let cv = mix(min(ow.x * 1.15, 1.0), ow.x, ow.y);
    d = remap(bv, min(1.0 - cv * prof, 0.999), 1.0, 0.0, 1.0) * (1.0 - cuw);
  }
  if (cu_diff > -0.20) {
    let bottom = smoothstep(0.0, 60.0, hb);
    let lobe = (wv.x * 0.7 + wv.y * 0.3) * 0.5 * smoothstep(0.05, 0.3, cu_hk);
    d = max(d, clamp((cu_diff + lobe) / 0.5, 0.0, 1.0) * bottom * cuw);
  }
  if (d <= 0.0) { return smooth_d * far; }
  let edge_w = mix(0.22, 0.16, ow.y);
  if (detail || g_mean_erode) {
    let erode_k = mix(mix(0.5, 0.62, ow.y), 0.6, cuw) * mix(0.35, 1.0, g_erode);
    if (d < erode_k + edge_w * (1.0 - erode_k)) {
      var ef = EF_MEAN;
      if (detail) {
        if (!have_wv) {
          wv = textureSampleLevel(t_noise, s_repeat, (pp + wind * 1.15) / 2900.0 + vec3f(0.31, 0.62, 0.17), 0.0).gba - 0.62;
        }
        let pe = pp + wind * 1.3 + wv * 420.0;
        let e = textureSampleLevel(t_noise, s_repeat, pe / CLOUD_DETAIL, 0.0);
        ef = e.g * 0.625 + e.b * 0.25 + e.a * 0.125;
        if (fine > 0.0) {
          let e2 = textureSampleLevel(t_noise, s_repeat, pe / CLOUD_FINE + vec3f(0.17, 0.59, 0.43), 0.0);
          ef = mix(ef, ef * 0.72 + (e2.g * 0.7 + e2.b * 0.3) * 0.28, fine);
        }
      }
      let low = 1.0 - smoothstep(0.02, 0.13, hn);
      let ed = mix(1.0 - ef, ef, low * 0.8);
      d = remap(d, ed * erode_k, 1.0, 0.0, 1.0);
    } else {
      d = 1.0;
    }
  }
  d = smoothstep(0.0, edge_w, d) * mix(0.55, 1.0, ow.y);
  return mix(d, smooth_d, far);
}
// Storm cell quality tier from Globals.detail.w: 1 Low, 2 Medium, 3 High, 4 Ultra
// (0 falls back to High so zeroed Globals keep full quality).
fn storm_tier() -> i32 {
  let w = abs(G.detail.w); // negative for --still shots
  return select(i32(w + 0.5), 3, w < 0.5);
}
// Pure storm-cell density evaluator (zero ordinary cumulus/stratocumulus/nimbo)
// for the decoupled storm pass (`clouds.wgsl` `fs_storm`): keeps register
// footprint small so the GPU runs at high occupancy while rendering full storms.
fn cloud_shape_storm_only(pp: vec3f, h: f32, ci: u32, cb: f32, detail: bool, fine: f32) -> f32 {
  let hs = h - G.cloud.y;
  if (hs <= -SHELF_BELOW || h >= G.cloud.w) { return 0.0; }
  let sc = G.storm_cells[2u * ci];
  let sp = G.storm_cells[2u * ci + 1u];
  let sd = storm_sd(pp, hs, ci);
  g_storm_sd_full = sd;
  let anvil = g_anvil;
  let bulge = storm_bulge(sc);
  let lowz = (1.0 - smoothstep(0.0, 1500.0, hs)) * smoothstep(-STORM_BELOW, -STORM_BELOW + 250.0, hs);
  let sreach = select(mix(0.55, 0.085, anvil), 0.08, hs < 0.0) * bulge + 300.0 * lowz * (1.0 - anvil) + 40.0;
  if (sd <= -sreach) { return 0.0; }
  let far = smoothstep(40000.0, 400000.0, G.up.w);
  if (far >= 1.0) { return 0.0; }
  g_meso = g_wall < -0.3 && sp.x > 1.0 && hs < 0.5 * (sp.y - G.cloud.y);
  let wind = G.weather.xyz;
  let un = normalize(pp);
  let n = textureSampleLevel(t_noise, s_repeat, (pp + wind - un * (max(hs, 0.0) * 0.5 * cb)) / CLOUD_SHAPE, 0.0);
  let fbm = n.g * 0.625 + n.b * 0.25 + n.a * 0.125;
  let base = remap(n.r, fbm - 1.0, 1.0, 0.0, 1.0);
  let at = sp.y - G.cloud.y;
  let rel = (un * 6371.0 + G.weather.xyz * 0.001 - sc.xyz) * 1000.0 + un * hs;
  let tq = (pp + wind) / 20000.0 + rel * ((1.0 / clamp(sc.w / 7.0, 0.45, 1.35) - 1.0) / 20000.0);
  let tn = textureSampleLevel(t_noise, s_repeat, tq + vec3f(0.71, 0.33, 0.12), 0.0);
  let tw = tn.g * 0.5 + tn.b * 0.3 + n.g * 0.2;
  let bl = 1.0 - (1.0 - tw) * (1.0 - tw);
  let under = 1.0 - smoothstep(-200.0, 0.0, hs);
  let hcf = hs / max(at * storm_top(sp.x), 1.0);
  let wallq = (1.0 - smoothstep(0.4, 0.6, sp.x)) * smoothstep(0.0, 400.0, hs + 30.0 - sd);
  let flank = max(smoothstep(0.08, 0.3, hcf), wallq) * (1.0 - smoothstep(0.6, 0.84, hcf)) * (1.0 - anvil);
  let bo = mix(bl, base * 0.6 + bl * 0.4, under) - 0.78;
  let lam = max(-g_wall, 0.0);
  let lineu = max(g_wall - 1.0, 0.0);
  var sdn = sd + bo * bulge * mix(1.3, 0.36, anvil) * (1.0 + 0.9 * flank * step(0.0, bo)) * (1.0 - 0.85 * lam) * select(mix(0.3, 1.0, max(smoothstep(0.0, 2000.0, hs), wallq)), mix(0.1, 0.25, min(abs(g_wall), 1.0)) * (1.0 - 0.15 * lineu), hs < 0.0);
  var wv = select(vec3f(0.0), vec3f(EF_MEAN - 0.62), g_mean_erode);
  var have_wv = !detail;
  if (lowz > 0.0 && anvil < 0.99 && lam < 0.99) {
    if (detail) {
      wv = textureSampleLevel(t_noise, s_repeat, (pp + wind * 1.15) / 2900.0 + vec3f(0.31, 0.62, 0.17), 0.0).gba - 0.62;
      have_wv = true;
    }
    sdn += (wv.x * 0.7 + wv.y * 0.3 + 0.08) * 650.0 * lowz * (1.0 - anvil) * (1.0 - 0.9 * lam) * (1.0 - 0.3 * lineu);
  }
  let env = smoothstep(0.0, mix(mix(mix(mix(220.0, 380.0, anvil), 700.0, under), 150.0, lam), 120.0, lineu), sdn);
  let body = mix(0.6 + base * 0.6, 0.55 + base * 0.35, anvil);
  var d = mix(remap(body * env, 0.2, 1.0, 0.0, 1.0), body * env, anvil);
  if (d <= 0.0) { return 0.0; }
  let edge_w = mix(0.22, 0.16, cb);
  if (detail || g_mean_erode) {
    let erode_k = mix(0.5, 0.62, cb) * mix(1.0, 0.50, anvil) * mix(0.35, 1.0, g_erode) * (1.0 - 0.7 * lam);
    if (d < erode_k + edge_w * (1.0 - erode_k)) {
      var ef = EF_MEAN;
      if (detail) {
        if (!have_wv) {
          wv = textureSampleLevel(t_noise, s_repeat, (pp + wind * 1.15) / 2900.0 + vec3f(0.31, 0.62, 0.17), 0.0).gba - 0.62;
        }
        let pe = pp + wind * 1.3 + wv * 420.0;
        let e = textureSampleLevel(t_noise, s_repeat, pe / CLOUD_DETAIL, 0.0);
        ef = e.g * 0.625 + e.b * 0.25 + e.a * 0.125;
        if (anvil > 0.02 && storm_tier() >= 2) {
          let ps = pe - G.storm.xyz * (dot(pe, G.storm.xyz) * 0.75);
          let ea = textureSampleLevel(t_noise, s_repeat, ps / CLOUD_DETAIL + vec3f(0.37, 0.11, 0.73), 0.0);
          ef = mix(ef, ea.g * 0.5 + ea.b * 0.3 + ea.a * 0.2, anvil);
        }
        if (fine > 0.0) {
          let e2 = textureSampleLevel(t_noise, s_repeat, pe / CLOUD_FINE + vec3f(0.17, 0.59, 0.43), 0.0);
          ef = mix(ef, ef * 0.72 + (e2.g * 0.7 + e2.b * 0.3) * 0.28, fine);
        }
      }
      let low = 1.0 - smoothstep(-STORM_BELOW, 300.0, hs);
      let wisp = max(low * 0.8, anvil);
      let ed = mix(1.0 - ef, ef, wisp);
      d = remap(d, ed * erode_k, 1.0, 0.0, 1.0);
    } else {
      d = 1.0;
    }
  }
  d = smoothstep(0.0, edge_w, d) * mix(0.55, 1.0, min(cb * 4.0, 1.0));
  d *= (1.0 - 0.5 * anvil) * (1.0 - far);
  return d;
}
fn cloud_density(pp: vec3f, h: f32, cheap: bool) -> f32 {
  if (h <= G.cloud.y - SHELF_BELOW || h >= G.cloud.w) { return 0.0; }
  return cloud_shape(pp, h, cloud_weather(pp), !cheap);
}
// Sunlight through the clouds at a point (planet-centred), from the cloud shadow map.
// Its texels are rays along the sun from points on the cloud base, so any point
// below or inside the layer finds its ray by sliding along the sun to the base.
// Each holds the optical depth of cloud from the base up to 1/9, 4/9 and all of
// the way (base..top, bands squeezed towards the base where the cumulus are), and
// in w how closed the deck is there (1 = overcast).
// vis: the share of direct sunlight left (with the forward-scattered glow that
// thinner cloud lets through); closed: the deck overhead, for points under it.
// hq: smooth (cubic B-spline) filtering, for surfaces; the haze marches, which
// call it many times a pixel, take one bilinear fetch.
struct CloudSun { vis: f32, closed: f32 };
const CS_RES = 1024.0;
fn cs_fetch(uv: vec2f, layer: i32, hq: bool) -> vec4f {
  if (!hq) { return textureSampleLevel(t_cshadow, s_clamp, uv, layer, 0.0); }
  // Cubic B-spline from four bilinear taps: no texel corners in the shadow's outline.
  let p = uv * CS_RES - 0.5;
  let i = floor(p);
  let f = p - i;
  let f2 = f * f;
  let f3 = f2 * f;
  let w0 = (1.0 - 3.0 * f + 3.0 * f2 - f3) / 6.0;
  let w1 = (4.0 - 6.0 * f2 + 3.0 * f3) / 6.0;
  let w2 = (1.0 + 3.0 * f + 3.0 * f2 - 3.0 * f3) / 6.0;
  let w3 = f3 / 6.0;
  let g0 = w0 + w1;
  let g1 = w2 + w3;
  let c0 = (i - 0.5 + w1 / g0) / CS_RES;
  let c1 = (i + 1.5 + w3 / g1) / CS_RES;
  return g0.y * (g0.x * textureSampleLevel(t_cshadow, s_clamp, c0, layer, 0.0) + g1.x * textureSampleLevel(t_cshadow, s_clamp, vec2f(c1.x, c0.y), layer, 0.0))
       + g1.y * (g0.x * textureSampleLevel(t_cshadow, s_clamp, vec2f(c0.x, c1.y), layer, 0.0) + g1.x * textureSampleLevel(t_cshadow, s_clamp, c1, layer, 0.0));
}
fn cloud_sun_at(pc: vec3f, r: f32, h: f32, hq: bool) -> CloudSun {
  var o = CloudSun(1.0, 0.0);
  let base = G.cs[3].x;
  let top = G.cs[3].y;
  if (G.cs[4].w < 0.5 || G.start.x > 0.0 || h >= top) { return o; }
  // The sun the maps were drawn for (it differs from this frame's by a fraction
  // of a degree while the clock runs fast).
  let sun = G.cs[4].xyz;
  let mu = dot(pc, sun) / r;
  let R = G.planet.w + base;
  // Exactly along the sun (a low sun's rays cross the base tens of km away): forward
  // to the base from below it, back down to it from inside the layer.
  var ts = ray_sphere(pc, sun, R).y;
  if (h >= base) {
    let s = ray_sphere(pc, -sun, R);
    ts = select((base - h) / max(mu, 0.04), -s.x, s.x > 0.0);
  }
  // The clouds have drifted on with the wind since the maps were drawn.
  let qn = normalize(pc + sun * ts + (G.weather.xyz - G.cs[5].xyz));
  let loc = vec2f(dot(qn, G.cs[1].xyz), dot(qn, G.cs[2].xyz)) / max(dot(qn, G.cs[0].xyz), 0.5) * R - G.cs[3].zw;
  let ui = loc / G.cs[0].w * 0.5 + 0.5;
  let uo = loc / G.cs[1].w * 0.5 + 0.5;
  let ei = max(abs(ui.x - 0.5), abs(ui.y - 0.5)) * 2.0;
  let eo = max(abs(uo.x - 0.5), abs(uo.y - 0.5)) * 2.0;
  if (eo >= 1.0) { return o; }
  var v: vec4f;
  if (ei < 0.8) {
    v = cs_fetch(ui, 0, hq);
  } else {
    v = cs_fetch(uo, 1, hq);
    if (ei < 1.0) { v = mix(cs_fetch(ui, 0, hq), v, smoothstep(0.8, 0.98, ei)); }
  }
  // Optical depth still above this point.
  let f = sqrt(clamp((h - base) / (top - base), 0.0, 1.0)) * 3.0;
  let below = select(select(mix(v.y, v.z, f - 2.0), mix(v.x, v.y, f - 1.0), f < 2.0), v.x * f, f < 1.0);
  let od = max(v.z - below, 0.0);
  let fade = smoothstep(0.85, 1.0, eo);
  let t = exp(-od);
  // (The forward-scattered glow fades out under really thick cloud: a storm.)
  o.vis = mix(t + (1.0 - t) * 0.22 / (1.0 + 0.12 * od + 0.01 * od * od), 1.0, fade);
  o.closed = v.w * (1.0 - fade) * (1.0 - smoothstep(base, base + 600.0, h));
  return o;
}
fn cloud_sun(pc: vec3f, hq: bool) -> CloudSun {
  let r = length(pc);
  return cloud_sun_at(pc, r, r - G.planet.w, hq);
}
// High cirrus: a thin streaky sheet at CIRRUS_H. Returns optical thickness.
fn cirrus(pp: vec3f) -> f32 {
  let q = pp + G.weather.xyz * 2.2;
  let m = textureSampleLevel(t_noise, s_repeat, q / vec3f(140000.0, 60000.0, 60000.0) + vec3f(0.7, 0.1, 0.4), 0.0);
  let amount = clamp((m.r - 0.42) * 2.6 + (m.g - 0.6) + G.cloud.x * 0.35, 0.0, 1.0);
  if (amount <= 0.0) { return 0.0; }
  let a = textureSampleLevel(t_noise, s_repeat, q / vec3f(42000.0, 5200.0, 5200.0) + (m.gba - 0.6) * 0.25, 0.0);
  let b = textureSampleLevel(t_noise, s_repeat, q / vec3f(9000.0, 1500.0, 1500.0) + (a.gba - 0.6) * 0.18, 0.0);
  let v = a.r * 0.7 + b.g * 0.18 + b.a * 0.12;
  // Wide threshold: soft veils and streaks, never isolated specks.
  return smoothstep(0.82 - amount * 0.35, 1.05 - amount * 0.15, v) * amount * 0.8;
}

// ------------------------------------------------------------------- waves
// Position in metres on the camera's local tangent plane, continuous as the camera moves.
fn wave_coord(p: vec3f) -> vec2f {
  return vec2f(dot(p, G.east.xyz) + G.east.w, dot(p, G.north.xyz) + G.north.w);
}
// Position in the ocean's own coordinates (m, east and north), which wrap at a
// multiple of every cascade's size (ocean.rs WRAP).
fn ocean_coord(p: vec3f) -> vec2f {
  return vec2f(dot(p, G.east.xyz), dot(p, G.north.xyz)) + G.ocean[0].xy;
}
// Slope covariance (east-east, north-north, east-north) of the waves that
// cascade c loses when read at mip level lv (ocean.rs `lost_slopes`).
fn ocean_lost(c: i32, lv: f32) -> vec3f {
  let l = clamp(lv, 0.0, 8.0);
  let i = i32(floor(l));
  let j = min(i + 1, 8);
  return mix(G.ocean[3 + c * 9 + i].xyz, G.ocean[3 + c * 9 + j].xyz, l - f32(i));
}
fn ocean_size(c: i32) -> f32 { return select(select(6.0, 250.0 / 6.0, c == 1), 250.0, c == 0); }
// Sea state: how much of each wave cascade (about 250 m, 40 m and 6 m across)
// a stretch of water shows, by its class, latitude and how open it is (shore:
// 0 by the coast, 1 out on open water). Short fetch - bays, lakes, rivers -
// keeps the short chop and loses the long waves and the swell. ocean.rs
// `sea_state` mirrors it for the floating aircraft.
fn sea_state(wclass: f32, sinlat: f32, shore: f32) -> vec3f {
  if (wclass > 2.5) { return vec3f(0.0, 0.1, 0.45); }   // river
  if (wclass > 1.5) { return vec3f(0.05, 0.45, 0.85); } // lake
  let rough = 1.0 + 0.3 * smoothstep(0.55, 0.85, abs(sinlat));
  return vec3f(rough * mix(0.3, 1.0, shore), mix(0.75, 1.0, shore), 1.0);
}
// Sideways (x = east, y = north) and vertical displacement of the sea surface
// above ocean coordinates x, filtered for a vertex spacing of sp metres;
// w = sea_state.
fn ocean_disp(x: vec2f, sp: f32, w: vec3f) -> vec3f {
  var o = vec3f(0.0);
  for (var c = 0; c < 3; c++) {
    let L = ocean_size(c);
    let lod = log2(max(2.0 * sp * 256.0 / L, 1.0));
    if (lod < 8.0 && w[c] > 0.0) {
      o += textureSampleLevel(t_ocean, s_repeat, x / L, c, lod).xyz * w[c];
    }
  }
  return o;
}

// ---------------------------------------------------------------- lighting
// Cascaded sun shadows: each cascade covers a wider area around the aircraft
// at a coarser resolution; they cross-fade so there is no visible boundary.
// The aircraft's own shadow: a fine map a few metres across (drawn into the
// top-left corner of its layer, pshadow.z of it), used everywhere
// (so its shadow on the ground is as sharp as on its own wing).
fn plane_shadow(P: vec3f, N: vec3f) -> f32 {
  if (G.pshadow.w <= 0.0) { return 1.0; }
  let tex = G.pshadow.x;
  let q = G.pshadow_vp * vec4f(P + N * tex * 1.5 + G.sun_dir.xyz * tex, 1.0);
  let uv = q.xy * vec2f(0.5, -0.5) + 0.5;
  if (any(uv <= vec2f(0.002)) || any(uv >= vec2f(0.998)) || q.z <= 0.0 || q.z >= 1.0) { return 1.0; }
  let texel = 1.0 / f32(textureDimensions(t_shadow).x);
  var sh = 0.0;
  for (var j = -1; j <= 1; j++) { for (var i = -1; i <= 1; i++) {
    sh += textureSampleCompareLevel(t_shadow, s_shadow, uv * G.pshadow.z + vec2f(f32(i), f32(j)) * texel, 3, q.z - G.pshadow.y);
  } }
  return sh / 9.0;
}
fn shadow_at(P: vec3f, N: vec3f) -> f32 {
  if (G.csm.w <= 0.0) { return 1.0; }
  return shadow_world(P, N) * plane_shadow(P, N);
}
fn shadow_world(P: vec3f, N: vec3f) -> f32 {
  let cmask = u32(G.lite.w);
  if (cmask == 0u) { return 1.0; }
  var acc = 0.0;
  var rem = 1.0;
  let texel = 1.0 / f32(textureDimensions(t_shadow).x);
  for (var c = 0; c < 3; c++) {
    if ((cmask & (1u << u32(c))) == 0u) { continue; }
    let tex = G.csm[c];
    let q = G.shadow_vp[c] * vec4f(P + N * tex * 1.8 + G.sun_dir.xyz * tex * 0.8, 1.0);
    let uv = q.xy * vec2f(0.5, -0.5) + 0.5;
    let e = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    if (e <= 0.0 || q.z <= 0.0 || q.z >= 1.0) { continue; }
    var sh = 0.0;
    for (var j = -1; j <= 1; j++) { for (var i = -1; i <= 1; i++) {
      sh += textureSampleCompareLevel(t_shadow, s_shadow, uv + vec2f(f32(i), f32(j)) * texel, c, q.z - 0.00003);
    } }
    let w = smoothstep(0.0, 0.07, e);
    acc += rem * w * sh / 9.0;
    rem *= 1.0 - w;
    if (rem < 0.01) { break; }
  }
  return acc + rem;
}

// ---------------------------------------------------------------- craters
// Ground knocked out by impacts. Returns the height change in w and the
// world-space slope (to bend normals) in xyz, for a point relative to camera.
fn crater_field(P: vec3f) -> vec4f {
  var o = vec4f(0.0);
  for (var i = 0; i < 8; i++) {
    let c = G.crater[i];
    if (c.w <= 0.0) { break; }
    let v = P - c.xyz;
    if (abs(v.x) > c.w * 2.0 || abs(v.y) > c.w * 2.0 || abs(v.z) > c.w * 2.0) { continue; }
    let upn = normalize(c.xyz - G.planet.xyz);
    let hv = v - upn * dot(v, upn);
    let r = length(hv);
    let d = r / c.w;
    if (d > 1.8) { continue; }
    let D = c.w * 0.34;
    let k = (d - 1.1) / 0.24;
    let rim = 0.22 * exp(-k * k);
    var h = rim;
    var dh = rim * (-2.0 * k / 0.24);
    if (d < 1.0) {
      let b = 1.0 - d * d;
      h -= b * b;
      dh += 4.0 * d * b;
    }
    o += vec4f(hv / max(r, 0.001) * (dh * D / c.w), h * D);
  }
  return o;
}

// ------------------------------------------------------------------ roads
fn stripe(x: f32, w: f32, fw: f32) -> f32 {
  return clamp((w * 0.5 - abs(x)) / fw + 0.5, 0.0, 1.0) * min(1.0, w / fw);
}
// Procedural road surface. across/along in metres, half = half width,
// kind = rclass | oneway<<4 | yellow<<5 | sidewalk<<6 | lanes<<8 | parking<<12 | keep-left<<13 | lit<<14,
// endd = distance to the nearest junction, measured so the junction's edge is at
// `half`; app = +1 where `along` runs away from that junction, -1 toward it;
// fw = pixel size in metres. Returns albedo and coverage.
fn road_surface(across: f32, along: f32, half: f32, kind: u32, endd: f32, app: f32, fw_in: f32) -> vec4f {
  let rclass = kind & 15u;
  let oneway = (kind & 16u) != 0u;
  let yellow = (kind & 32u) != 0u;
  let walk = (kind & 64u) != 0u;
  let lanes = f32((kind >> 8u) & 15u);
  let parking = (kind & 4096u) != 0u;
  let keep_left = (kind & 8192u) != 0u;
  let fw = max(fw_in, 0.002);
  let uv = vec2f(across, along);
  let n1 = textureSampleLevel(t_wave, s_repeat, uv / 31.0, 0.0).a;
  let n2 = textureSampleLevel(t_wave, s_repeat, uv / 3.7, log2(max(fw * 256.0 / 3.7, 1.0))).a;
  let n3 = textureSampleLevel(t_wave, s_repeat, uv / 0.9, log2(max(fw * 256.0 / 0.9, 1.0))).a;
  let ax = abs(across);
  var edge = clamp((half - ax) / max(fw, 0.12) + 0.5, 0.0, 1.0);
  if (rclass == 9u) {
    // Railway: ballast, sleepers, two rails.
    var col = vec3f(0.16, 0.14, 0.12) * (0.7 + 0.6 * n3);
    let sl = stripe(fract(along / 0.65) - 0.5, 0.36, fw / 0.65) * step(ax, 1.25);
    col = mix(col, vec3f(0.09, 0.075, 0.06), sl);
    col = mix(col, vec3f(0.42, 0.40, 0.38), stripe(ax - 0.72, 0.09, fw));
    return vec4f(col, edge);
  }
  if (rclass == 7u) {
    // Dirt track: two worn ruts, ragged edges.
    let rut = stripe(ax - 0.8, 0.5, fw);
    let col = mix(vec3f(0.20, 0.16, 0.11), vec3f(0.27, 0.22, 0.16), rut) * (0.75 + 0.5 * n2);
    return vec4f(col, clamp((half - ax + (n2 - 0.5) * 0.9) / max(fw, 0.25), 0.0, 1.0));
  }
  if (rclass == 8u) {
    let col = vec3f(0.30, 0.29, 0.27) * (0.8 + 0.4 * n2);
    return vec4f(col, edge);
  }
  // Asphalt, newer and darker on the big roads.
  var tone = 0.050;
  if (rclass <= 1u) { tone = 0.036; } else if (rclass >= 5u) { tone = 0.062; }
  if (rclass == 6u) { tone = 0.075; }
  var col = vec3f(tone, tone, tone * 1.04) * (0.72 + 0.35 * n1 + 0.25 * n3);
  // Patches and cracks close up.
  col *= 1.0 - 0.25 * smoothstep(0.62, 0.7, n2) * (1.0 - smoothstep(0.02, 0.2, fw));
  let close = 1.0 - smoothstep(0.03, 0.25, fw);
  if (close > 0.0 && rclass <= 6u) {
    // Trench repairs: long strips of newer, darker asphalt, sealed along their edges.
    let pid = floor(along / 23.0);
    let ph = hash2(vec2f(pid, half * 7.0));
    if (ph < 0.3) {
      let px = across - (hash1(pid + 3.0) - 0.5) * half;
      let py = fract(along / 23.0) * 23.0 - 4.0;
      let pl = 4.0 + 13.0 * hash1(pid + 9.0);
      let inside = stripe(px, 1.3, fw) * stripe(py - pl * 0.5, pl, fw);
      let seal = inside - stripe(px, 1.14, fw) * stripe(py - pl * 0.5, pl - 0.16, fw);
      col = mix(col, col * 0.72, inside * close);
      col = mix(col, vec3f(0.012), clamp(seal, 0.0, 1.0) * close * 0.8);
    }
    // Tar-sealed cracks wandering along the lanes, oil down the middle of each one.
    let wob = (textureSampleLevel(t_wave, s_repeat, vec2f(along / 19.0, floor(across / 3.3) * 0.37), 0.0).a - 0.5) * 2.2;
    let crack = stripe(fract(across / 3.3 + 0.5) * 3.3 - 1.65 - wob, 0.045, fw) * smoothstep(0.52, 0.6, n1);
    col = mix(col, vec3f(0.012), crack * close * 0.75);
    // Manhole covers.
    let mid = floor(along / 41.0);
    let mc = vec2f(across - (hash1(mid * 1.3 + half) - 0.5) * half, fract(along / 41.0) * 41.0 - 20.0);
    let ring = clamp((0.34 - length(mc)) / fw + 0.5, 0.0, 1.0) * step(hash1(mid + 17.0), 0.55);
    col = mix(col, vec3f(0.035, 0.032, 0.03) * (0.7 + 0.6 * step(0.5, fract(length(mc) / 0.11))), ring * close);
  }
  var road_half = half;
  if (walk) {
    road_half = half - 1.9;
    // Kerb and paved sidewalk.
    let sw = smoothstep(road_half - fw, road_half + fw, ax);
    let joint = 1.0 - 0.35 * stripe(fract(along / 1.5) - 0.5, 0.03, fw / 1.5);
    let pave = vec3f(0.30, 0.29, 0.27) * (0.8 + 0.3 * n2) * joint;
    col = mix(col, pave, sw);
    col = mix(col, vec3f(0.38, 0.37, 0.35), stripe(ax - road_half, 0.16, fw));
  }
  if (rclass <= 5u && lanes > 0.5) {
    let fade = smoothstep(half + 5.6, half + 6.6, endd);
    var mk = 0.0;
    var mcol = vec3f(0.62);
    let park = select(0.0, 2.2, parking);
    let drive_half = road_half - park;
    let lw = min(2.0 * drive_half / lanes, 3.7);
    let x0 = -lw * lanes * 0.5;
    let dash = step(fract(along / 12.0), 0.25);
    if (rclass <= 4u) {
      // Lane lines.
      let li = (across - x0) / lw;
      let near = round(li);
      if (near > 0.5 && near < lanes - 0.5) {
        let x = (li - near) * lw;
        let centre = !oneway && abs(near - lanes * 0.5) < 0.25;
        if (centre) {
          if (yellow) {
            mk = max(stripe(x - 0.12, 0.11, fw), stripe(x + 0.12, 0.11, fw));
            mcol = vec3f(0.62, 0.42, 0.04);
          } else {
            mk = stripe(x, 0.14, fw);
          }
        } else {
          mk = stripe(x, 0.12, fw) * dash;
        }
      }
      // Edge lines on the bigger roads.
      if (rclass <= 3u) { mk = max(mk, stripe(ax - (lw * lanes * 0.5 + 0.15), 0.12, fw) * select(1.0, 0.0, parking)); }
    } else if (!oneway && lanes > 1.5) {
      mk = stripe(across, 0.11, fw) * dash * 0.8;
      if (yellow) { mcol = vec3f(0.62, 0.42, 0.04); }
    }
    mk *= fade;
    // Where streets meet: a stop line for arriving traffic, and a zebra
    // crossing from kerb to kerb if people walk here.
    if (endd < half + 6.0) {
      let side = select(1.0, -1.0, keep_left) * app;
      let mine = select(clamp(-across * side / fw + 0.5, 0.0, 1.0), select(0.0, 1.0, app < 0.0), oneway);
      let stop = stripe(endd - (half + 5.2), 0.4, fw) * mine * step(ax, drive_half + park);
      var zebra = 0.0;
      if (walk) { zebra = stripe(endd - (half + 2.5), 3.0, fw) * stripe(fract(across / 1.1) - 0.5, 0.5, fw / 1.1) * step(ax, road_half - 0.2); }
      let jm = max(stop, zebra);
      mk = max(mk * (1.0 - jm), jm);
      mcol = mix(mcol, vec3f(0.62), jm);
    }
    // Paint wears.
    mk *= 0.55 + 0.45 * smoothstep(0.3, 0.6, n2);
    col = mix(col, mcol, mk);
  }
  // Asphalt and markings are solid; only the feathered outer edge blends
  // against terrain. A constant 0.9 made the ground show through every road.
  return vec4f(col, edge);
}

// Street lights: must match `lamp_spec` in roads.rs. Returns the light landing
// on the road surface from the lamps up and down the street (1 right under one).
fn lamp_pool(across: f32, along: f32, half: f32, kind: u32) -> f32 {
  if ((kind & 16384u) == 0u) { return 0.0; }
  let big = (kind & 15u) <= 1u;
  let sp = select(36.0, 48.0, big);
  let h = select(8.0, 11.0, big);
  let off = half - select(3.3, 2.9, big);
  let k0 = floor(along / sp);
  var o = 0.0;
  for (var i = -1; i <= 1; i++) {
    let k = k0 + f32(i);
    let sgn = 1.0 - 2.0 * abs(k - 2.0 * floor(k * 0.5 + 0.25));
    let d = vec2f(across + sgn * off, along - (k + 0.5) * sp);
    let r2 = dot(d, d) + h * h;
    o += h * h * h / (r2 * sqrt(r2));
  }
  return o;
}
const LAMP_COL = vec3f(1.0, 0.72, 0.40);
// 1 once it is dark enough for the lights to be on.
// ------------------------------------------------------------ 3D tiles zone
// Inside the zone of photorealistic 3D tiles our own ground, buildings, trees
// and cars give way to the mesh wherever it has been drawn into the coverage
// mask. Across the band at the edge of the zone the two are dithered into
// each other pixel by pixel, the same way on both sides (TAA smooths it).
fn photo_fade(P: vec3f) -> f32 {
  let q = P - G.photo_c.xyz;
  let up = normalize(G.photo_c.xyz - G.planet.xyz);
  let d = length(q - up * dot(q, up));
  return smoothstep(G.photo.x - G.photo.y, G.photo.x, d);
}
// The mesh is drawn at this pixel (P on it).
fn photo_keep(P: vec3f, px: vec2f) -> bool {
  return ign(px + 5.588238 * (G.time.w % 64.0)) >= photo_fade(P);
}
// The mask at P: r = the mesh is there, g = height of our water surface
// there (far below everything where there is none).
fn photo_mask(P: vec3f) -> vec2f {
  let q = G.photo_vp * vec4f(P, 1.0);
  let uv = q.xy * vec2f(0.5, -0.5) + 0.5;
  if (any(uv <= vec2f(0.0)) || any(uv >= vec2f(1.0))) { return vec2f(0.0, -1e4); }
  return textureSampleLevel(t_photo, s_clamp, uv, 0.0).rg;
}
fn photo_covered(P: vec3f) -> bool {
  if (G.photo.z <= 0.0) { return false; }
  return photo_mask(P).r > 0.5;
}
// Height above the planet of a camera-relative point, good to a few cm
// within tens of km (a straight length() loses half a metre in f32).
fn height_of(P: vec3f) -> f32 {
  let v = dot(P, G.up.xyz);
  return G.up.w + v + (dot(P, P) - v * v) / (2.0 * G.planet.w);
}
// Whether P lies inside a destroyed building's outline (and, with `floor`,
// above its floor): the mesh is not drawn there and our own scenery is.
fn cut_vert(k: u32) -> vec2f {
  let r = G.cut_v[k / 2u];
  return select(r.xy, r.zw, (k & 1u) == 1u);
}
fn photo_cut_at(P: vec3f, floor: bool) -> bool {
  let n = u32(G.photo.w);
  for (var i = 0u; i < n; i++) {
    let h0 = G.cut[i * 3u];
    let d = P - h0.xyz;
    let e = G.cut[i * 3u + 1u];
    let nn = G.cut[i * 3u + 2u];
    let q = vec2f(dot(d, e.xyz), dot(d, nn.xyz));
    if (dot(q, q) > h0.w * h0.w) { continue; }
    if (floor && dot(d, cross(e.xyz, nn.xyz)) < 0.0) { continue; }
    let first = u32(nn.w);
    let count = u32(e.w);
    var c = false;
    var pj = cut_vert(first + count - 1u);
    for (var k = 0u; k < count; k++) {
      let pk = cut_vert(first + k);
      if ((pk.y > q.y) != (pj.y > q.y) && q.x < (pj.x - pk.x) * (q.y - pk.y) / (pj.y - pk.y) + pk.x) { c = !c; }
      pj = pk;
    }
    if (c) { return true; }
  }
  return false;
}
fn photo_cut(P: vec3f) -> bool { return photo_cut_at(P, true); }
// Our own scenery at P stands under the mesh, so casts no shadow (the mesh
// casts its own): the shadow passes see the coverage mask; the band at the
// edge of the zone switches over in its middle.
fn photo_shadow_gone(P: vec3f) -> bool {
  return G.photo.z > 0.0 && photo_fade(P) < 0.5 && photo_mask(P).r > 0.5 && !photo_cut_at(P, false);
}
// Our own scenery at P (pixel px) is hidden by the mesh.
fn photo_hides(P: vec3f, px: vec2f) -> bool {
  return photo_covered(P) && photo_keep(P, px) && !photo_cut_at(P, false);
}

// The start screen's globe look (sky.wgsl `globe_surface`), 0..1 with the
// camera's height: below GLOBE_LO the start screen draws the ordinary flight
// scene (a picked spot seen close up), from GLOBE_HI the planet from orbit.
const GLOBE_LO = 150000.0;
const GLOBE_HI = 800000.0;
fn globe_k() -> f32 { return select(0.0, smoothstep(GLOBE_LO, GLOBE_HI, G.up.w), G.start.x > 0.0); }
fn night_at(upn: vec3f) -> f32 { return 1.0 - smoothstep(-0.08, 0.1, dot(upn, G.sun_dir.xyz)); }
// Moonlight arriving at a point (dimmed by the atmosphere near the horizon and
// by the cloud deck as a whole).
fn moon_light(upn: vec3f, h: f32) -> vec3f {
  if (G.moon.w <= 0.0) { return vec3f(0.0); }
  return sun_trans(h, dot(upn, G.moon.xyz)) * G.moon.w * MOON_TINT * mix(1.0, 0.35, G.cloud.x);
}

// ------------------------------------------------------------ smoke volume
const VOL_N = vec3f(40.0, 40.0, 48.0);
fn vol_uvw(P: vec3f) -> vec3f {
  let q = P - G.vol_o.xyz;
  return vec3f(dot(q, G.vol_e.xyz), dot(q, G.vol_n.xyz), dot(q, G.vol_u.xyz)) / (G.vol_o.w * VOL_N);
}
// Smoke lighting at a point relative to camera; zero outside the volume.
fn vol_light(P: vec3f) -> vec4f {
  if (G.vol_u.w <= 0.0) { return vec4f(0.0); }
  let uvw = vol_uvw(P);
  let m = min(uvw, 1.0 - uvw);
  let e = min(m.x, min(m.y, m.z));
  if (e <= 0.0) { return vec4f(0.0); }
  return textureSampleLevel(t_vlight, s_clamp, uvw, 0.0) * smoothstep(0.0, 0.05, e);
}

// ----------------------------------------------------------- storms close up
// Colour of lightning's light: a hot, slightly blue-violet white.
const FLASH_COL = vec3f(0.82, 0.86, 1.0);
// Light a flash of brightness 1 throws: illuminance FLASH_E / (d^2 + FLASH_D^2)
// at d metres from its channel (the lit cloud around it spreads it out close by).
// An in-cloud flash's light mostly stays in the cloud (half reaches the ground).
// FLASH_C: the same inside the cloud, where it diffuses (clouds.wgsl flash_cloud).
const FLASH_E = 4.0e5;
const FLASH_D = 2000.0;
const FLASH_C = 3.0e6;
fn seg_dist(p: vec3f, a: vec3f, b: vec3f) -> f32 {
  let ab = b - a;
  let k = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-3), 0.0, 1.0);
  return length(p - a - ab * k);
}
// Lightning's light arriving at P (camera-relative).
fn flash_light(P: vec3f) -> vec3f {
  let n = u32(G.stormfx.z);
  var e = 0.0;
  for (var i = 0u; i < n; i++) {
    let a = G.flash[2u * i];
    let b = G.flash[2u * i + 1u];
    let d = seg_dist(P, a.xyz, b.xyz);
    e += a.w * FLASH_E * (0.5 + 0.5 * b.w) / (d * d + FLASH_D * FLASH_D);
  }
  return FLASH_COL * e;
}
// How deep in a storm's shade a point (planet-centred) lies, 0..1: under the
// tower and its rain nearly all the sky is dark cloud; under the anvil a thick
// grey lid. Must match storm.rs `shade`.
fn storm_shade(pp: vec3f) -> f32 {
  let m = u32(G.stormfx.x);
  if (m == 0u) { return 0.0; }
  let un = normalize(pp);
  let h = length(pp) - G.planet.w;
  let W = G.storm.xyz;
  let X = normalize(cross(un, W));
  let q = un * 6371.0 + G.weather.xyz * 0.001;
  var dark = 0.0;
  let n = u32(G.storm.w);
  var rem = m & select((1u << n) - 1u, 0xffffffffu, n >= 32u);
  while (rem != 0u) {
    let i = firstTrailingBit(rem);
    rem &= rem - 1u;
    let c = G.storm_cells[2u * i];
    let sp = G.storm_cells[2u * i + 1u];
    let R = c.w * 1000.0;
    let dv = (q - c.xyz) * 1000.0;
    let dw0 = dot(dv, W);
    let cxl = dot(dv, X);
    let cx = sign(cxl) * max(abs(cxl) - floor(sp.w) * 500.0, 0.0);
    let rc = length(vec2f(dw0 - 0.25 * R, cx)) / R;
    let core = (1.0 - smoothstep(0.7, 1.7, rc)) * smoothstep(0.3, 0.75, sp.x) * smoothstep(1500.0, 4000.0, R);
    let an = storm_anvil(c, sp);
    let lc = an.z + max(dw0, 0.0) * mix(0.42, 0.28, storm_super(sp));
    let rn = length(vec2f(dw0 / select(an.y, an.x, dw0 > 0.0), cx / lc));
    let lid = (1.0 - smoothstep(0.5, 1.05, rn)) * 0.55 * smoothstep(0.5, 0.8, sp.x);
    dark = max(dark, max(core, lid));
  }
  return dark * (1.0 - smoothstep(G.cloud.y, G.cloud.y + 3000.0, h));
}
// Storm light: under a storm the sky light dims to a dull, faintly green grey.
fn storm_light(amb: vec3f, dark: f32) -> vec3f {
  let grey = vec3f(luma(amb)) * vec3f(0.93, 0.97, 1.0);
  return mix(amb, grey, dark * 0.75) * (1.0 - 0.7 * dark);
}

struct Light { sun: vec3f, amb: vec3f, upn: vec3f, h: f32 };
// Sun and sky light at a point, shaded by the smoke of any fire nearby.
fn light_at(P: vec3f) -> Light {
  var l = light_at_nv(P);
  if (G.vol_u.w > 0.0) {
    let v = vol_light(P);
    l.sun *= exp(-v.x);
    l.amb *= 0.55 + 0.45 * exp(-v.z * 0.7);
  }
  return l;
}
// Sun and sky light arriving at a point (relative to camera).
fn light_at_nv(P: vec3f) -> Light {
  let pp = P - G.planet.xyz;
  let r = length(pp);
  let upn = pp / r;
  let h = r - G.planet.w;
  let mu = dot(upn, G.sun_dir.xyz);
  let st = sun_trans(h, mu);
  var sun = st * G.sun_dir.w;
  if (sun.x + sun.y + sun.z > 0.0) { sun *= cloud_sun_at(pp, r, h, true).vis; }
  // Skylight falls off steeply once the sun is down: about half a decade per
  // 3 degrees of twilight (the exp term), so a street at blue hour is far
  // darker than the glowing sky above it rather than as bright as overcast day.
  let day = min(smoothstep(-0.12, 0.3, mu), 0.06 * exp(20.0 * mu));
  let dusk = smoothstep(-0.2, 0.0, mu) * (1.0 - smoothstep(0.0, 0.3, mu));
  // Skylight is a pale blue, not a saturated one (the sky is bright near the
  // horizon and around the sun, which whitens what it adds up to), and the
  // white undersides of clouds grey it further as the cover grows. In
  // twilight it turns the deep blue-violet of the blue hour.
  var skyc = mix(vec3f(0.30, 0.43, 0.70), vec3f(0.47, 0.49, 0.52), clamp(G.cloud.x, 0.0, 1.0) * 0.7);
  skyc = mix(skyc, vec3f(0.38, 0.36, 0.62), dusk * 0.6);
  var amb = skyc * 0.11 * day * G.sun_dir.w;
  // Night: the moonlit sky, starlight and airglow, and the city's own glow
  // reflected back down off the haze. It takes over as the twilight fades.
  let mum = dot(upn, G.moon.xyz);
  let night = 1.0 - smoothstep(-0.12, 0.02, mu);
  amb += vec3f(0.30, 0.45, 0.85) * 0.16 * smoothstep(-0.1, 0.35, mum) * G.moon.w * mix(1.0, 0.7, G.cloud.x);
  amb += vec3f(0.0011, 0.0021, 0.0052) * night;
  // The towns' light sent back down by the haze and the cloud base above.
  var cup = vec3f(0.0);
  if (glow_on()) { cup = city_up(P, 600.0) * (GLOW_GAIN / PI); }
  amb += cup * (0.0025 + 0.032 * clamp(G.cloud.x, 0.0, 1.0));
  var sun_s = sun;
  if (G.nimbo[0].w > 0.0) {
    // Under the rain deck: only the flat grey light diffusing through it (the
    // shadow map takes the sun, which can still slant in under its edge), and
    // the city's glow off its base at night.
    let nd = nimbo_under(pp);
    amb = mix(amb, nimbo_light(upn, h) + cup * 0.062, nd);
  }
  if (G.stormfx.x > 0.0) {
    // (The sun can still slant in under the edge of an anvil, not under the core.)
    let dark = storm_shade(pp);
    amb = storm_light(amb, dark);
    sun_s *= 1.0 - dark * dark;
  }
  if (G.stormfx.z > 0.0) { amb += flash_light(P); }
  return Light(sun_s, amb, upn, h);
}
// What a mirror at P sees along R: the sky (with clouds), or below the
// horizon the hazy ground and city.
fn env_reflect(P: vec3f, R: vec3f, upn: vec3f, clouds: bool) -> vec3f {
  let ru = dot(R, upn);
  var sky = sky_lut(normalize(R + upn * max(0.015 - ru, 0.0)));
  if (clouds && ru > 0.02 && G.cloud.x > 0.001 && G.lite.y > 0.5) {
    let pp = P - G.planet.xyz;
    let h = length(pp) - G.planet.w;
    let mid = G.cloud.y + 400.0;
    if (h < mid) {
      let tc = (mid - h) / ru;
      if (tc < 60000.0) {
        let cd = cloud_density(pp + R * tc, mid, true);
        let mu = dot(upn, G.sun_dir.xyz);
        let cl = sun_trans(mid, mu) * G.sun_dir.w * 0.05 + sky_lut(upn) * 1.2;
        sky = mix(sky, cl, (1.0 - exp(-cd * 7.0)) * (1.0 - smoothstep(25000.0, 60000.0, tc)));
      }
    }
  }
  if (ru > 0.01) { return sky; }
  let hz = R - upn * ru;
  let hl = length(hz);
  let hzn = select(G.east.xyz, hz / max(hl, 1e-4), hl > 1e-3);
  let mu = dot(upn, G.sun_dir.xyz);
  let gnd = vec3f(0.06, 0.06, 0.055) * (sun_trans(0.0, mu) * max(mu, 0.0) * G.sun_dir.w / PI + sky_lut(upn) * 0.6);
  // Near the horizon the ground fades into the haze.
  let below = mix(sky_lut(normalize(hzn + upn * 0.01)), gnd, smoothstep(0.0, 0.3, -ru));
  return mix(sky, below, smoothstep(0.01, -0.05, ru));
}
// Light of a fire: a glowing ball of radius light_pos.w whose light falls off
// with the square of the distance beyond it.
fn point_light(P: vec3f, N: vec3f) -> vec3f {
  return fire_light(P, N) + spot_light(P, N) + flood_light(P, N);
}
// Apron floodlights on their masts (G.flood): wide beams aimed down at the
// apron, bright pools under each mast fading into the dark beyond, falling
// off with the square of the distance. No shadows: what stands under them is
// lit where it faces the mast.
fn flood_light(P: vec3f, N: vec3f) -> vec3f {
  var c = 0.0;
  for (var i = 0; i < 8; i++) {
    let s = G.flood[2 * i];
    if (s.w <= 0.0) { break; }
    let b = G.flood[2 * i + 1];
    let d = s.xyz - P;
    let r2 = max(dot(d, d), 25.0);
    let l = d * inverseSqrt(r2);
    let ca = dot(-l, b.xyz);
    if (ca <= b.w) { continue; }
    let x = (ca - b.w) / (1.0 - b.w);
    let beam = x * x * (3.0 - 2.0 * x) * (0.4 + 0.6 * x);
    c += s.w * beam * max(dot(N, l), 0.0) / r2 * exp(-sqrt(r2) * 2.5e-5 * max(G.weather.w, 0.2));
  }
  return vec3f(1.0, 0.88, 0.70) * c;
}
// The one burning light (a crash's fire, the afterburners).
fn fire_light(P: vec3f, N: vec3f) -> vec3f {
  let d = G.light_pos.xyz - P;
  let r2 = dot(d, d);
  let rad = G.light_pos.w;
  if (rad <= 0.0) { return vec3f(0.0); }
  let att = rad * rad / (rad * rad + r2);
  return G.light_col.rgb * att * max(dot(N, d * inverseSqrt(max(r2, 1e-6))) * 0.7 + 0.3, 0.0);
}
// The flown aircraft's landing lights: two beams, a bright core fading out
// into a wider spill, falling off with the square of the distance (and
// through the haze). They graze the ground; rough pavement and runway paint
// throw much of it back toward the lamps (and the eye beside them), which
// the softened incidence term stands in for.
fn spot_light(P: vec3f, N: vec3f) -> vec3f {
  var c = vec3f(0.0);
  let dir = G.spot[2].xyz;
  let cmin = G.spot[2].w;
  for (var i = 0; i < 2; i++) {
    let s = G.spot[i];
    if (s.w <= 0.0) { continue; }
    let d = s.xyz - P;
    // The falloff stops growing inside 10 m; the direction is the true one
    // (a car's lamps light the road from a few metres off).
    let dd = max(dot(d, d), 1e-4);
    let r2 = max(dd, 100.0);
    let l = d * inverseSqrt(dd);
    let ca = dot(-l, dir);
    if (ca <= cmin) { continue; }
    let x = (ca - cmin) / (1.0 - cmin);
    let beam = x * x * (0.35 + 0.65 * smoothstep(0.55, 0.95, x));
    c += s.w * beam * sqrt(max(dot(N, l), 0.0)) / r2 * exp(-sqrt(r2) * 2.5e-5 * max(G.weather.w, 0.2));
  }
  return vec3f(1.0, 0.92, 0.80) * c;
}
// ------------------------------------------------------------- lying snow
// Snow on the ground, trees and roofs (snow.rs; keep the constants and
// `snow_cover` the same as there). G.snow: snow line under the camera (m),
// its change northwards (m per m), +1 / -1 for the north / south pole, 1 when
// there is any snow.
const SNOW_BAND = 400.0;       // bare to fully white over this height (m)
const SNOW_ASPECT = 500.0;     // the line drops this much on a slope facing the pole (per unit of its normal's poleward part)
const SNOW_HOLD_COS = 0.866;   // holds fully up to 30 degrees
const SNOW_SLIDE_COS = 0.5;    // none left from 60 degrees
const SNOW_ALBEDO = vec3f(0.80, 0.82, 0.86);
fn snow_on() -> bool { return G.snow.w > 0.0; }
// An integer hash of a lattice point, 0..1 (sin-based hashes lose their
// precision this far from the origin).
fn snow_hash(i: vec2f) -> f32 {
  let q = bitcast<vec2u>(vec2i(i));
  var h = (q.x * 0x27d4eb2du) ^ ((q.y + 0x9e3779b9u) * 0x85ebca6bu);
  h ^= h >> 15u;
  h *= 0x2c1b3c6du;
  h ^= h >> 12u;
  return f32(h >> 8u) / 16777216.0;
}
// Smooth value noise 0..1 with no tiling (the snow's drifts and bare patches).
fn snow_vnoise(p: vec2f) -> f32 {
  let i = floor(p);
  let f = p - i;
  let u = f * f * (3.0 - 2.0 * f);
  let a = snow_hash(i);
  let b = snow_hash(i + vec2f(1.0, 0.0));
  let c = snow_hash(i + vec2f(0.0, 1.0));
  let d = snow_hash(i + vec2f(1.0, 1.0));
  return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
// The same, faded to its mean once a cell is under a few pixels.
fn snow_noise(wc: vec2f, scale: f32, fw: f32) -> f32 {
  return mix(snow_vnoise(wc / scale), 0.5, smoothstep(0.15, 0.5, fw / scale));
}
fn snow_cover(alt: f32, cos_slope: f32, poleward: f32, line: f32) -> f32 {
  let z = alt - line + SNOW_ASPECT * poleward;
  return smoothstep(-0.5 * SNOW_BAND, 0.5 * SNOW_BAND, z) * smoothstep(SNOW_SLIDE_COS, SNOW_HOLD_COS, cos_slope);
}
// How much snow lies at P (camera-relative) on ground whose normal is N;
// `lift` (m) moves the point up or down through the band (noise).
fn snow_at(P: vec3f, N: vec3f, upn: vec3f, lift: f32) -> f32 {
  if (!snow_on()) { return 0.0; }
  let alt = length(P - G.planet.xyz) - G.planet.w;
  return snow_cover(alt + lift, dot(N, upn), dot(N, G.north.xyz) * G.snow.z, snow_line_at(P));
}
// The snow line at P (camera-relative). Where today's analysis is known, it
// decides at its 4 km scale: under snow the line comes down SNOW_DOWN below
// the worked-out one (a cell is snowy when most of it is, so its lowest
// ground may still be bare; the slope and the patches still shape it); where
// there is none it rises SNOW_BARE above it, so only the peaks well above it
// stay white. Its cells blend into each other, and into the worked-out line at the
// map's edges and where it knows nothing (open water, the south).
const SNOW_DOWN = 1500.0;
const SNOW_BARE = 800.0;
fn snow_line_at(P: vec3f) -> f32 {
  let l = snow_line_real(P);
  // Snow that has fallen during the flight lies on top of it all.
  return mix(l, min(l, G.fresh.x), G.fresh.y);
}
fn snow_line_real(P: vec3f) -> f32 {
  let line = G.snow.x + dot(P, G.north.xyz) * G.snow.y;
  if (G.snow_e.w <= 0.0 || G.snow_c.w <= 0.0) { return line; }
  let q = P - G.snow_c.xyz;
  let uv = vec2f(dot(q, G.snow_e.xyz), -dot(q, G.snow_n.xyz)) / (2.0 * G.snow_c.w) + 0.5;
  let edge = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
  if (edge <= 0.0) { return line; }
  let m = textureSampleLevel(t_snow, s_clamp, uv, 0.0);
  // (R is the share of the known part: undo the blend with unknown texels.)
  let share = m.r / max(m.g, 1e-3) * step(1e-3, m.g);
  let w = G.snow_e.w * smoothstep(0.0, 1.0, m.g) * smoothstep(0.0, 0.05, edge);
  return mix(line, mix(line + SNOW_BARE, line - SNOW_DOWN, clamp(share, 0.0, 1.0)), w);
}
// Snow resting on top of something above the ground (a crown, a roof): as
// much as lies on flat ground there, where the surface faces up between
// `lo` and `hi` (cosines with the vertical).
fn snow_top(P: vec3f, N: vec3f, lo: f32, hi: f32) -> f32 {
  if (!snow_on()) { return 0.0; }
  let upn = normalize(P - G.planet.xyz);
  return snow_at(P, upn, upn, 0.0) * smoothstep(lo, hi, dot(N, upn));
}
// Dynamic snow tracks and crash trails (snowtrack.rs; keep constants the same).
// Returns (clear, berm, dh/dx, dh/dy) in the track map's East/North frame.
const SNOW_TR_N = 2048.0;
const SNOW_TR_CELL = 0.25;
fn snow_track_at(P: vec3f, dist: f32, fw: f32) -> vec4f {
  let size = G.snow_tr_c.w;
  if (size <= 0.0 || dist > 1400.0) { return vec4f(0.0); }
  let q = P - G.snow_tr_c.xyz;
  let lx = dot(q, G.snow_tr_e.xyz);
  let ly = dot(q, G.snow_tr_n.xyz);
  let margin = min(min(lx, size - lx), min(ly, size - ly));
  if (margin <= 0.0) { return vec4f(0.0); }
  let edge = smoothstep(0.0, 16.0, margin) * (1.0 - smoothstep(1000.0, 1400.0, dist));
  let uv = (vec2f(lx, ly) + vec2f(G.snow_tr_e.w, G.snow_tr_n.w)) / size;
  var c = textureSampleLevel(t_snowtrack, s_repeat, uv, 0.0).rg;
  if (fw > 0.45) {
    let d = min(fw * 0.22, 0.75) / size;
    let avg = 0.25 * (
      textureSampleLevel(t_snowtrack, s_repeat, uv + vec2f(d, d), 0.0).rg +
      textureSampleLevel(t_snowtrack, s_repeat, uv + vec2f(-d, d), 0.0).rg +
      textureSampleLevel(t_snowtrack, s_repeat, uv + vec2f(d, -d), 0.0).rg +
      textureSampleLevel(t_snowtrack, s_repeat, uv + vec2f(-d, -d), 0.0).rg
    );
    // Keep narrow tracks legible from altitude instead of diluting them away.
    c = mix(c, max(c * 0.65, min(avg * 1.25, vec2f(1.0))), smoothstep(0.45, 1.8, fw));
  }
  c *= edge;
  if (c.x < 0.002 && c.y < 0.002) { return vec4f(0.0); }
  var gx = 0.0;
  var gy = 0.0;
  if (dist < 480.0) {
    let step = 1.0 / SNOW_TR_N;
    let xp = textureSampleLevel(t_snowtrack, s_repeat, uv + vec2f(step, 0.0), 0.0).rg;
    let xm = textureSampleLevel(t_snowtrack, s_repeat, uv - vec2f(step, 0.0), 0.0).rg;
    let yp = textureSampleLevel(t_snowtrack, s_repeat, uv + vec2f(0.0, step), 0.0).rg;
    let ym = textureSampleLevel(t_snowtrack, s_repeat, uv - vec2f(0.0, step), 0.0).rg;
    let hp = xp.y * 0.45 - xp.x * 0.70;
    let hm = xm.y * 0.45 - xm.x * 0.70;
    let vp = yp.y * 0.45 - yp.x * 0.70;
    let vm = ym.y * 0.45 - ym.x * 0.70;
    let inv = 0.5 / SNOW_TR_CELL * edge;
    gx = (hp - hm) * inv;
    gy = (vp - vm) * inv;
  }
  return vec4f(c.x, c.y, gx, gy);
}

fn shade(P: vec3f, N: vec3f, albedo: vec3f) -> vec3f {
  let l = light_at(P);
  let ndl = max(dot(N, G.sun_dir.xyz), 0.0);
  var sh = 1.0;
  if (ndl > 0.0 && (l.sun.x + l.sun.y + l.sun.z) > 0.0) { sh = shadow_at(P, N); }
  return shade_lit(P, N, albedo, l, sh);
}
// The same with the light at P (light_at) and its shadow for N (shadow_at)
// already in hand: both cost texture reads (shadow_at up to 36 compares), and
// the drivers do not merge a second identical call.
fn shade_lit(P: vec3f, N: vec3f, albedo: vec3f, l: Light, sh: f32) -> vec3f {
  let ndl = max(dot(N, G.sun_dir.xyz), 0.0);
  let sky = 0.55 + 0.45 * dot(N, l.upn);
  let ndm = max(dot(N, G.moon.xyz), 0.0);
  var moon = vec3f(0.0);
  if (ndm > 0.0 && G.moon.w > 0.0) { moon = moon_light(l.upn, l.h) * ndm; }
  // Light bounced off the ground: what keeps walls and overhangs from going
  // flat blue-black in shade.
  let mu = max(dot(l.upn, G.sun_dir.xyz), 0.0);
  let bounce = vec3f(0.15, 0.14, 0.12) * (l.sun * mu * 0.8 / PI + l.amb) * (0.5 - 0.5 * dot(N, l.upn));
  g_amb = albedo * (l.amb * sky + bounce);
  return albedo * ((l.sun * ndl * sh + moon) / PI + l.amb * sky + bounce + point_light(P, N));
}
