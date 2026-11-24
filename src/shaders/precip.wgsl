// Weather close up, drawn over the lit scene and the clouds (behind the glass):
// rain falling around the camera, as streaks blurred along their motion
// relative to the camera, and lightning channels (storm.rs). Both are thin
// lines drawn as screen-space quads a few pixels wide, tested against the
// scene's depth and dimmed by the clouds and rain shafts in front of them.
// They write how much they cover into alpha, which taa.wgsl reads.
@group(1) @binding(0) var t_depth: texture_depth_2d;
@group(1) @binding(1) var t_cloud: texture_2d<f32>;
@group(1) @binding(3) var t_cdist: texture_2d<f32>;

// Exposure time the rain streaks are blurred over, s (about the eye's 1/80 s).
const SHUTTER = 0.009;
// Drops are spread over four nested boxes around the camera, 8, 20, 50 and 200
// m (render.rs RAIN_DROPS instances; the box sizes must divide storm.rs WRAP):
// each box fills a sphere half its size, so the nearer the denser.
const RAIN_N0 = 8192u;
const RAIN_N1 = 32768u;
const RAIN_N2 = 81920u;
// Lens aperture (m) the defocus of near drops is worked out for (focused far off).
const APERTURE = 0.014;

fn pcg(v: u32) -> u32 {
  let s = v * 747796405u + 2891336453u;
  let w = ((s >> ((s >> 28u) + 4u)) ^ s) * 277803737u;
  return (w >> 22u) ^ w;
}
fn u01(h: u32) -> f32 { return f32(h & 0xffffffu) / 16777216.0; }

// How much of what lies d metres along this pixel's ray still shows through the
// clouds and rain shafts drawn over the scene: the cloud pass keeps their
// transmittance and (weighted) distance at half resolution.
fn cloud_cover(px: vec2f, d: f32) -> f32 {
  let dims = vec2i(textureDimensions(t_cloud)) - 1;
  let c = clamp(vec2i(px * 0.5), vec2i(0), dims);
  let T = textureLoad(t_cloud, c, 0).a;
  let dc = textureLoad(t_cdist, c, 0).r * 1000.0;
  if (dc <= 0.0) { return 1.0; }
  return mix(T, 1.0, smoothstep(-0.12, 0.12, (dc - d) / max(d, 1.0)));
}

// A line from a to b (camera-relative) as a quad around it on screen: half
// width hw pixels, also reaching hw past each end. Corner vi of 6. Returns the
// clip position, and in q: pixels across, pixels along (from a), length in pixels.
struct Line { clip: vec4f, q: vec3f, ok: bool };
fn line_quad(a_in: vec3f, b_in: vec3f, hw: f32, vi: u32) -> Line {
  var o: Line;
  o.ok = false;
  var a = a_in;
  var b = b_in;
  // Cut off at a plane just in front of the camera.
  let wn = G.misc.x * 2.0;
  var ca = G.view_proj * vec4f(a, 1.0);
  var cb = G.view_proj * vec4f(b, 1.0);
  if (ca.w < wn && cb.w < wn) { return o; }
  if (ca.w < wn) { a = mix(a, b, (wn - ca.w) / (cb.w - ca.w)); ca = G.view_proj * vec4f(a, 1.0); }
  if (cb.w < wn) { b = mix(b, a, (wn - cb.w) / (ca.w - cb.w)); cb = G.view_proj * vec4f(b, 1.0); }
  let half = G.screen.xy * 0.5;
  let sa = ca.xy / ca.w * half;
  let sb = cb.xy / cb.w * half;
  let dv = sb - sa;
  let len = length(dv);
  var dir = vec2f(0.0, 1.0);
  if (len > 1e-3) { dir = dv / len; }
  let perp = vec2f(-dir.y, dir.x);
  let corners = array<vec2f, 6>(vec2f(-1.0, 0.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0), vec2f(-1.0, 0.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0));
  let c = corners[vi];
  var clip = mix(ca, cb, c.y);
  let off = perp * (c.x * hw) + dir * ((c.y * 2.0 - 1.0) * hw);
  clip = vec4f(clip.xy + off / half * clip.w, clip.zw);
  o.clip = clip;
  o.q = vec3f(c.x * hw, c.y * len + (c.y * 2.0 - 1.0) * hw, len);
  o.ok = true;
  return o;
}

// ---------------------------------------------------------------- rain
// Heavy rain is a soft grey veil, not a starfield: many faint streaks at all
// depths, each about as bright as the light around it (a drop shows a little
// fish-eye view of the sky and ground about it, dark under a storm), near ones
// wide and out of focus, far ones too thin to make out, merging into the haze
// (vs_veil). Against bright sky the streaks come out darker than it.
struct RainOut {
  @builtin(position) clip: vec4f,
  // pixels across, along, the drop's distance (m)
  @location(0) q: vec3f,
  // half the streak's blurred width (px), its length (px), its ends' softness (px)
  @location(1) @interpolate(flat) p: vec3f,
  @location(2) @interpolate(flat) col: vec4f,
};

// The light raindrops (and the rain between them) send back: the ambient sky
// and ground light at the camera, a faint share of the sun, and only a little
// of a lightning flash (the haze and clouds take most of that).
fn rain_light(vd: vec3f, k: f32) -> vec3f {
  let lt = light_at_nv(vec3f(0.0));
  var amb = lt.amb;
  if (G.stormfx.z > 0.0) { amb -= flash_light(vec3f(0.0)) * 0.75; }
  let glint = pow(max(dot(vd, G.sun_dir.xyz), 0.0), 8.0);
  // (Scaled to match what the clouds and haze come out at: as bright as a white
  // surface in the shade, the veil washed the storm out to a pale grey.)
  return (amb * 0.18 + lt.sun * (0.002 + 0.03 * glint)) * k;
}

// Snow (snow.rs, main.rs): G.flakes.w of what falls here is snow, and blown
// snow (G.flakes_v.w) fills the air near the ground in a blizzard. Each
// particle is a raindrop or a snowflake; flakes ride their own field, falling
// at about 1 m/s and swirling.
const BLOW_K = 0.8;
fn flake_amount() -> f32 { return fract(G.rain[0].w) * G.flakes.w + BLOW_K * G.flakes_v.w; }

@vertex fn vs_rain(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> RainOut {
  var o: RainOut;
  o.clip = vec4f(2.0, 2.0, 2.0, 1.0);
  // (Hail in the whole part, in fifteenths; main.rs.)
  let rain = fract(G.rain[0].w);
  let hail = floor(G.rain[0].w) / 15.0;
  let fl = flake_amount();
  let inten = clamp(rain * (1.0 - G.flakes.w) + fl, 0.0, 1.0);
  let h0 = pcg(ii * 2654435761u + 7u);
  // The heavier the rain, the more drops (and the bigger): the count grows
  // faster than the intensity, the faintness makes up the rest.
  if (u01(h0) > sqrt(inten)) { return o; }
  let flake = u01(pcg(h0 ^ 0x68e31da4u)) * inten < fl;
  var B = 200.0;
  if (ii < RAIN_N0) { B = 8.0; } else if (ii < RAIN_N1) { B = 20.0; } else if (ii < RAIN_N2) { B = 50.0; }
  let h1 = pcg(h0);
  let h2 = pcg(h1);
  let h3 = pcg(h2);
  let h4 = pcg(h3);
  let off = select(G.rain[0].xyz, G.flakes.xyz, flake);
  var l = (fract(vec3f(u01(h1), u01(h2), u01(h3)) + off / B) - 0.5) * B;
  if (flake) {
    // Each flake swirls about its path, a few tens of centimetres.
    let ph = u01(h2) * 6.2832;
    let f = 0.6 + 0.8 * u01(h3);
    l += vec3f(sin(G.time.x * f + ph), cos(G.time.x * f * 0.8 + ph * 1.7), 0.0) * 0.25;
  }
  let P = G.east.xyz * l.x + G.north.xyz * l.y + G.up.xyz * l.z;
  let dist = length(P);
  let near = G.rain[1].w;
  // Each box fills a sphere (its corners would show as a cube of rain).
  let fade = smoothstep(near, near * 1.6 + 1.0, dist) * (1.0 - smoothstep(0.3 * B, 0.5 * B, dist)) * (0.15 + 0.85 * u01(pcg(h4)));
  if (fade <= 0.0) { return o; }
  // Off screen: skip it (most drops are).
  let cp = G.view_proj * vec4f(P, 1.0);
  if (cp.w > 0.0 && any(abs(cp.xy) > vec2f(cp.w * 1.3 + 2.0))) { return o; }
  // In a supercell's core some of it is hail: stones a centimetre or two
  // across, falling 15-25 m/s faster than the rain, white and opaque.
  let stone = !flake && u01(pcg(h4 ^ 0x5bd1e995u)) < 0.4 * hail;
  var D = mix(0.0007, 0.004, u01(h4) * u01(h4)) * (0.6 + 0.6 * inten);
  // Blurred along its motion relative to the camera over the exposure: at
  // flying speeds the drops rush at the camera as long streaks.
  var v = G.rain[1].xyz;
  if (flake) {
    // Flakes: aggregates of a few millimetres to a centimetre or two.
    D = mix(0.003, 0.015, u01(h4) * u01(h4));
    v = G.flakes_v.xyz;
  }
  if (stone) {
    D = mix(0.008, 0.025, u01(h3));
    v -= G.up.xyz * mix(15.0, 25.0, u01(h2));
  }
  let ls = length(v) * SHUTTER;
  let a = P - v * (SHUTTER * 0.5);
  let b = P + v * (SHUTTER * 0.5);
  // Its width on screen: the drop itself, out of focus close up, and never
  // sharper than a pixel or so.
  let core = D / (dist * G.misc.w);
  let coc = APERTURE / (dist * G.misc.w);
  let w = sqrt(core * core + coc * coc + 1.3 * 1.3);
  let ln = line_quad(a, b, w * 0.75 + 0.5, vi);
  if (!ln.ok) { return o; }
  // How much of the background it hides: the drop covers core/w of the blurred
  // width and is in each spot of its streak only so long; it is mostly clear
  // water, so it hides little even then. (Each streak stands for many smaller
  // drops nobody would draw one by one.)
  // At flying speeds that is very little: the rain is mostly the veil, with
  // faint streaks rushing past, not a bright starfield. (A square root on the
  // drop's size: a softer falloff than the physics, or all but the nearest
  // few vanish.)
  var blur = pow(0.03 / (ls + 0.03), 0.7);
  var alpha = clamp(1.6 * sqrt(min(core / w * 1.6, 1.0)) * blur, 0.0, select(0.25, 0.6, stone)) * fade * mix(0.45, 1.0, inten);
  if (flake) {
    // White and opaque: a flake hides what is behind it, smeared along its
    // streak at flying speeds (the "hyperspace" of snow in the landing lights).
    blur = pow(0.02 / (ls + 0.02), 0.5);
    alpha = clamp(1.4 * sqrt(min(core / w * 1.4, 1.0)) * blur, 0.0, 0.85) * fade;
  }
  o.clip = ln.clip;
  o.q = vec3f(ln.q.xy, dist);
  o.p = vec3f(w * 0.5, ln.q.z, max(w * 0.5, 1.0));
  // (The drops catch a little more light than the haze of rain around them:
  // the brighter sky towards the horizon.)
  o.col = vec4f(rain_light(P / max(dist, 1e-3), select(select(2.5, 6.0, stone), 7.0, flake)), alpha);
  return o;
}

@fragment fn fs_rain(in: RainOut) -> @location(0) vec4f {
  // A soft (Gaussian-like) profile across, soft ends.
  let x = in.q.x / max(in.p.x, 0.5);
  let across = exp(-1.4 * x * x);
  let along = clamp(min(in.q.y, in.p.y - in.q.y) / in.p.z + 0.5, 0.0, 1.0);
  let a = in.col.a * across * along * cloud_cover(in.clip.xy, in.q.z);
  return vec4f(in.col.rgb * a, a);
}

// The rain too far off or too fine to draw drop by drop: a grey veil over
// everything, the more the further, that takes the contrast out of the view.
// It leaves alpha alone (render.rs p_veil), or the anti-aliasing would treat
// the whole screen as moving.
const VEIL_MAX = 0.45;
struct VeilOut {
  @builtin(position) clip: vec4f,
  // rain light, opacity at great distance
  @location(0) @interpolate(flat) col: vec4f,
  // distance (m) it reaches 63% of that
  @location(1) @interpolate(flat) dv: f32,
};
@vertex fn vs_veil(@builtin(vertex_index) vi: u32) -> VeilOut {
  var o: VeilOut;
  let uv = vec2f(f32((vi << 1u) & 2u), f32(vi & 2u));
  o.clip = vec4f(uv * 2.0 - 1.0, 0.9999, 1.0);
  let inten = fract(G.rain[0].w);
  // Snow hides far more than rain of the same strength: white, and the
  // visibility in moderate snow is a kilometre or less, in heavy snow a few
  // hundred metres; blowing snow near the ground less still (a whiteout).
  let sn = G.flakes.w;
  let blow = G.flakes_v.w;
  o.col = vec4f(rain_light(-G.up.xyz, mix(1.0, 2.6, max(sn, blow))), mix(VEIL_MAX * pow(inten, 0.8), 0.8 * pow(inten, 0.6), sn));
  // Under the rain deck the cloud pass draws its rain as a veil already (clouds.wgsl nimbo_rain).
  if (G.nimbo[0].w > 0.0) { o.col.a *= 1.0 - 0.5 * mix(nimbo_area(-G.planet.xyz), 0.0, sn); }
  o.col.a = max(o.col.a, 0.9 * blow);
  o.dv = mix(mix(3000.0, 700.0, inten), mix(1800.0, 250.0, inten), sn);
  o.dv = mix(o.dv, 90.0, blow);
  return o;
}
@fragment fn fs_veil(in: VeilOut) -> @location(0) vec4f {
  let z = textureLoad(t_depth, vec2i(in.clip.xy), 0);
  var d = 1e6;
  if (z > 0.0) { d = length(view_ray(in.clip.xy * G.screen.zw) / z); }
  let a = in.col.a * (1.0 - exp(-d / in.dv));
  return vec4f(in.col.rgb * a, a);
}

// ----------------------------------------------------------- lightning
// How bright a channel is on screen at brightness 1, relative to the exposure:
// far brighter than anything else, day or night (the bloom spreads its glow).
const BOLT_K = 8.0;

struct BoltOut {
  @builtin(position) clip: vec4f,
  // pixels across, along, the channel's distance (m)
  @location(0) q: vec3f,
  // half the core's width (px), half the quad's width (px), brightness, length (px)
  @location(1) @interpolate(flat) p: vec4f,
};

@vertex fn vs_bolt(@builtin(vertex_index) vi: u32, @location(0) a: vec4f, @location(1) b: vec4f) -> BoltOut {
  var o: BoltOut;
  o.clip = vec4f(2.0, 2.0, 2.0, 1.0);
  let mid = 0.5 * (a.xyz + b.xyz);
  let dist = length(mid);
  // The core: its width in metres, but never thinner than a pixel and a half.
  let core = max(a.w / (dist * G.misc.w), 2.2) * 0.5;
  // A soft glow around it (the air it heats and the haze it lights), wider
  // the brighter; the bloom spreads it further.
  let hw = core + 5.0 + 22.0 * clamp(b.w, 0.0, 1.0);
  let ln = line_quad(a.xyz, b.xyz, hw, vi);
  if (!ln.ok) { return o; }
  o.clip = ln.clip;
  o.q = vec3f(ln.q.xy, dist);
  o.p = vec4f(core, hw, b.w, ln.q.z);
  return o;
}

@fragment fn fs_bolt(in: BoltOut) -> @location(0) vec4f {
  // Distance from the segment, rounded at its start and cut square at its end:
  // the next segment's round start fills the joint, so the glow (added up)
  // doesn't bead into bright dots at every kink.
  let len = in.p.w;
  if (in.q.y > len) { discard; }
  let x = select(abs(in.q.x), length(in.q.xy), in.q.y < 0.0);
  let core = clamp(in.p.x + 0.6 - x, 0.0, 1.0);
  // (Falling to nothing at the quad's edge, or that edge shows as a ribbon.)
  let fall = 1.0 - smoothstep(0.4, 1.0, x / in.p.y);
  let halo = (exp(-x / 2.5) * 0.12 + exp(-x / 8.0) * 0.03) * fall;
  // The haze between, and any cloud or rain in front.
  let ext = (RAYLEIGH.g + (MIE_S + MIE_A) * G.weather.w) * 0.8;
  let seen = exp(-in.q.z * ext) * cloud_cover(in.clip.xy, in.q.z);
  let b = in.p.z * seen;
  let col = (vec3f(0.92, 0.94, 1.0) * core + vec3f(0.62, 0.66, 1.0) * halo) * b * BOLT_K / max(G.time.z, 0.05);
  return vec4f(col, clamp(core * min(b * 30.0, 1.0), 0.0, 1.0));
}
