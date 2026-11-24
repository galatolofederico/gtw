// Airfield pavement laid over the levelled terrain: runways with their
// markings, taxiways, aprons, and painted lines and numbers (airfield.rs).
struct BTile { offset: vec4f, ax: vec4f, ay: vec4f };
@group(1) @binding(0) var<uniform> T: BTile;

struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) P: vec3f,
  @location(1) q: vec4f,
  @location(2) @interpolate(flat) r: vec4f,
  @location(3) loc: vec2f,
  @location(4) @interpolate(flat) kind: u32,
};

@vertex fn vs(@location(0) pos: vec3f, @location(1) q: vec4f, @location(2) r: vec4f, @location(3) kind: f32, @location(4) loc: vec2f) -> VOut {
  let P = pos + T.offset.xyz;
  var clip = G.view_proj * vec4f(P, 1.0);
  // Pulled toward the eye so the pavement wins against the terrain under it.
  // Far off, that terrain is coarse and not levelled, so the pull grows.
  let far = smoothstep(2500.0, 4500.0, clip.w);
  clip.z *= 1.0 + 0.0018 + 0.35 / max(clip.w, 0.5) + 0.02 * far;
  return VOut(clip, P, q, r, loc, u32(kind + 0.5));
}

fn wn(p: vec2f, s: f32, fw: f32) -> f32 {
  return textureSampleLevel(t_wave, s_repeat, p / s, log2(max(fw * 256.0 / s, 1.0))).a;
}
// Coverage of the band |x| < w/2, antialiased over the pixel size fw.
fn band(x: f32, w: f32, fw: f32) -> f32 {
  return clamp((w * 0.5 - abs(x)) / fw + 0.5, 0.0, 1.0) * min(1.0, w / fw);
}
// Coverage of lo < x < hi.
fn span(x: f32, lo: f32, hi: f32, fw: f32) -> f32 {
  return band(x - (lo + hi) * 0.5, hi - lo, fw);
}
// Dark joints of a slab grid (a x b metres).
fn joints(u: vec2f, a: vec2f, fw: f32) -> f32 {
  let f = abs(fract(u / a + 0.5) - 0.5) * a;
  let w = 0.035;
  return max(band(f.x, w * 2.0, fw), band(f.y, w * 2.0, fw)) * (1.0 - smoothstep(0.02, 0.12, fw));
}

// Sealed cracks: wobbly lines on a jittered `cell`-metre grid, in some
// cells only; coverage 0..1, faded out before they would alias.
fn cracks(u: vec2f, cell: f32, share: f32, fw: f32) -> f32 {
  let fade = 1.0 - smoothstep(0.03, 0.14, fw);
  if (fade <= 0.0) { return 0.0; }
  let w = u / cell + (vec2f(wn(u, 3.1, fw), wn(u + vec2f(31.0, 17.0), 3.7, fw)) - 0.5) * 0.5;
  let id = floor(w);
  let f = fract(w) - 0.5;
  let h = hash2(id + vec2f(7.0, 11.0));
  let fwc = fw / cell;
  let along_x = band(f.x, 0.022, fwc) * step(1.0 - share, h);
  let along_y = band(f.y, 0.022, fwc) * step(1.0 - share * 0.6, fract(h * 7.31));
  return max(along_x, along_y) * fade;
}

fn asphalt(loc: vec2f, fw: f32, tone: f32) -> vec3f {
  let n1 = wn(loc, 47.0, fw);
  let n2 = wn(loc, 6.1, fw);
  let n3 = wn(loc, 1.3, fw);
  var c = vec3f(tone, tone, tone * 1.03) * (0.72 + 0.3 * n1 + 0.22 * n2 + 0.18 * n3);
  // Repairs: rectangles of newer (darker) or older (greyer) asphalt.
  let pid = floor(loc / vec2f(9.0, 6.5));
  let ph = hash2(pid + vec2f(5.1, 2.3));
  let pf = fract(loc / vec2f(9.0, 6.5));
  let inside = span(pf.x, 0.12, 0.88, fw / 9.0) * span(pf.y, 0.15, 0.85, fw / 6.5);
  c *= mix(1.0, select(1.13, 0.8, fract(ph * 13.7) > 0.5), inside * step(0.88, ph));
  // Crack sealant, tar black and a little glossy.
  c = mix(c, vec3f(0.018, 0.018, 0.02), cracks(loc, 7.0, 0.3, fw) * 0.85);
  // Oil drips.
  c *= 1.0 - 0.25 * smoothstep(0.7, 0.86, wn(loc + vec2f(3.0, 29.0), 4.5, fw));
  return c;
}
fn concrete(loc: vec2f, slab: vec2f, size: vec2f, fw: f32) -> vec3f {
  let id = floor(slab / size);
  let h = hash2(id + vec2f(17.0, 3.0));
  let n1 = wn(loc, 33.0, fw);
  let n2 = wn(loc, 2.3, fw);
  // (aged apron concrete reads about 0.58 sRGB on the orthophoto: 0.3 linear)
  var c = vec3f(0.27, 0.267, 0.254) * (0.84 + 0.14 * h + 0.2 * n1 + 0.1 * n2);
  // Stains and tyre marks.
  c *= 1.0 - 0.2 * smoothstep(0.7, 0.88, wn(loc + vec2f(41.0, 7.0), 9.0, fw));
  // Spalled, darker slab edges, and a crack across some slabs.
  let f = abs(fract(slab / size + 0.5) - 0.5) * size;
  c *= 1.0 - 0.12 * (1.0 - smoothstep(0.05, 0.3, min(f.x, f.y))) * (1.0 - smoothstep(0.02, 0.1, fw));
  c = mix(c, c * 0.45, cracks(slab + h * 3.0, size.x, 0.18, fw));
  // Joints: dark sealant, moss and grass where it has worked loose.
  let j = joints(slab, size, fw);
  let moss = smoothstep(0.45, 0.7, wn(loc + vec2f(13.0, 5.0), 3.3, fw));
  let joint = mix(vec3f(0.035, 0.034, 0.032), vec3f(0.07, 0.09, 0.035), moss);
  return mix(c, joint, j * 0.85);
}

const PAINT_W = vec3f(0.62, 0.62, 0.60);
const PAINT_Y = vec3f(0.60, 0.42, 0.05);

// The runway's markings: coverage of white paint at (across, distance past
// the threshold), seen from one end. Must match `mark_layout` in airfield.rs.
fn rwy_marks(x: f32, st: f32, s: f32, half: f32, len: f32, cls: u32, fw: f32) -> f32 {
  let ks = clamp(half * 2.0 * 0.42, 6.0, 18.0) / 18.0;
  let sc = clamp(half / 22.9, 0.4, 1.0);
  let ax = abs(x);
  var a = 0.0;
  if (cls >= 1u) {
    // Piano keys.
    let gap = 3.0 * ks;
    let u = (ax - gap) / (3.6 * ks);
    let nmax = floor((half - 3.0 * ks - gap) / (3.6 * ks));
    if (u > 0.0 && u < nmax) {
      a = max(a, span(st, 6.0 * ks, 51.0 * ks, fw) * band(fract(u) - 0.25, 0.5, fw / (3.6 * ks)));
    }
    // Aiming point, and the touchdown zone bars on the bigger runways.
    var ap = max(len * 0.22, 120.0);
    if (len >= 2400.0) { ap = 400.0; } else if (len >= 1200.0) { ap = 300.0; }
    let inner = 11.0 * sc;
    a = max(a, span(st, ap, ap + 45.0 * ks, fw) * span(ax, inner, inner + 9.0 * sc, fw));
    if (cls >= 2u && st < len * 0.5) {
      let groups = array<f32, 5>(150.0, 450.0, 600.0, 750.0, 900.0);
      let counts = array<f32, 5>(3.0, 2.0, 2.0, 1.0, 1.0);
      for (var k = 0; k < 5; k++) {
        let g = groups[k];
        if (abs(g - ap) < 60.0) { continue; }
        let lane = (ax - inner) / (3.3 * sc);
        if (lane > 0.0 && lane < counts[k]) {
          a = max(a, span(st, g, g + 22.5, fw) * band(fract(lane) - 0.5 * 1.8 / 3.3, 1.8 / 3.3, fw / (3.3 * sc)));
        }
      }
    }
  }
  return a;
}

struct GOut { @location(0) c: vec4f, @location(1) n: vec4f, @location(2) amb: f32 };
@fragment fn fs(in: VOut) -> GOut {
  // Inside the 3D tiles zone the mesh stands in for this (tiles3d.rs).
  if (photo_hides(in.P, in.clip.xy)) { discard; }
  let fw = max(max(fwidth(in.q.x), fwidth(in.q.y)), 0.004);
  let fwl = max(length(fwidth(in.loc)), 0.004);
  var albedo = vec3f(0.0);
  var a = 1.0;
  var shine = 0.04;
  var flood = 0.0;
  if (in.kind == 0u) {
    // ---- runway
    let x = in.q.x;
    let along = in.q.y;
    let half = in.q.z;
    let len = in.q.w;
    let flags = u32(in.r.z + 0.5);
    let cls = flags & 3u;
    let conc = (flags & 4u) != 0u;
    // Seen from whichever end is nearer.
    let near0 = along < len * 0.5;
    let s = select(len - along, along, near0);
    let thr = select(in.r.y, in.r.x, near0);
    let xs = select(-x, x, near0);
    let st = s - thr;
    if (conc) {
      albedo = concrete(in.loc, vec2f(x + 100.0, along), vec2f(7.5, 6.0), fwl);
    } else {
      albedo = asphalt(in.loc, fwl, 0.062);
      // Grooved and grey toward the edges, darker and newer down the middle.
      albedo *= 1.0 + 0.12 * smoothstep(0.4, 1.0, abs(x) / half);
    }
    // Rubber laid down where the wheels touch: black streaks either side of
    // the centreline over the touchdown zone.
    let zone = smoothstep(60.0, 260.0, st) * (1.0 - smoothstep(650.0, 1300.0, st)) * smoothstep(-5.0, 30.0, st);
    let lane = exp(-pow((abs(x) - 4.5) / 6.0, 2.0)) + 0.6 * exp(-pow(x / 3.0, 2.0));
    let streak = 0.6 + 0.8 * wn(vec2f(x * 4.0, along * 0.08), 1.0, fw * 0.3);
    albedo *= 1.0 - clamp(zone * lane * streak * select(0.55, 0.8, conc), 0.0, 0.85);
    // Paint.
    var paint = rwy_marks(xs, st, s, half, len, cls, fw);
    // Side stripes on all but the plainest runways.
    if (cls >= 1u) {
      let sw = clamp(half / 25.0, 0.45, 0.9);
      paint = max(paint, band(abs(x) - (half - 0.6 - sw * 0.5), sw, fw));
    }
    // Centreline dashes between the numbers at each end.
    let ks = clamp(half * 2.0 * 0.42, 6.0, 18.0) / 18.0;
    let has_letter = (flags & 8u) != 0u;
    var yn = select(6.0 * ks, 63.0 * ks, cls >= 1u);
    if (has_letter) { yn += 24.0 * ks; }
    let c0 = along - in.r.x;
    let c1 = len - along - in.r.y;
    if (min(c0, c1) > yn + 30.0 * ks) {
      let cw = select(0.45, 0.9, cls >= 2u);
      paint = max(paint, band(x, cw, fw) * band(fract(along / 60.0) - 0.3, 0.6, fw / 60.0));
    }
    // A displaced threshold: a bar across, and arrows leading up to it.
    if (thr > 1.0) {
      paint = max(paint, span(st, -3.0, 0.0, fw) * step(abs(x), half - 1.0));
      if (st < -10.0 && s > 5.0) {
        let k = (-st) % 60.0;
        var arrow = span(k, 18.0, 46.0, fw) * band(x, 0.9, fw);
        let head = (k - 6.0) * 0.55;
        arrow = max(arrow, span(k, 6.0, 18.0, fw) * clamp((head - abs(x)) / fw + 0.5, 0.0, 1.0) * clamp((abs(x) - (head - 1.3)) / fw + 0.5, 0.0, 1.0));
        paint = max(paint, arrow);
      }
    }
    paint *= 0.82 + 0.18 * wn(in.loc * 3.0, 1.0, fw);
    paint *= 1.0 - 0.6 * clamp(zone * lane * streak * 0.6, 0.0, 1.0);
    albedo = mix(albedo, PAINT_W, paint);
    a = clamp((half - abs(x)) / max(fw, 0.08) + 0.5, 0.0, 1.0) * clamp(min(along, len - along) / max(fw, 0.08) + 0.5, 0.0, 1.0);
    shine = select(0.05, 0.025, conc);
  } else if (in.kind == 1u) {
    // ---- taxiway
    if (in.r.x > 0.5) {
      albedo = concrete(in.loc, in.loc, vec2f(5.0, 5.0), fwl) * 0.95;
    } else {
      albedo = asphalt(in.loc, fwl, 0.075);
    }
    a = clamp((in.q.z - abs(in.q.x)) / max(fw, 0.15) + 0.5, 0.0, 1.0);
  } else if (in.kind == 2u) {
    // ---- apron
    if (in.r.x > 0.5) {
      albedo = concrete(in.loc, in.q.xy, vec2f(5.0, 5.0), fwl);
      // Oil where the aircraft stand.
      albedo *= 1.0 - 0.3 * smoothstep(0.55, 0.8, wn(in.loc + vec2f(9.0, 13.0), 23.0, fwl));
    } else {
      albedo = asphalt(in.loc, fwl, 0.08);
    }
    flood = in.q.z;
  } else {
    // ---- paint: u along, v across (0..w), total length, width
    let colk = u32(in.r.x + 0.5);
    let fwu = max(fwidth(in.q.x), 0.004);
    let fwv = max(fwidth(in.q.y), 0.004);
    var cov = clamp(min(in.q.y, in.q.w - in.q.y) / fwv + 0.5, 0.0, 1.0) * min(1.0, in.q.w / fwv);
    cov *= clamp(min(in.q.x, in.q.z - in.q.x) / fwu + 0.5, 0.0, 1.0);
    if (in.r.y > 0.5) {
      // A holding position: two solid lines, then two broken ones.
      let v = in.q.y / in.q.w * 4.0;
      let line = band(fract(v) - 0.5, 0.45, fwv * 4.0 / in.q.w);
      let dash = select(1.0, band(fract(in.q.x / 1.8) - 0.5, 0.5, fwu / 1.8), v > 2.0);
      cov *= line * dash;
    }
    var col = PAINT_W;
    if (colk == 1u) { col = PAINT_Y; }
    if (colk == 2u) { col = vec3f(0.45, 0.05, 0.04); }
    if (colk == 3u) { col = vec3f(0.02, 0.02, 0.02); }
    albedo = col;
    a = cov * (0.8 + 0.2 * wn(in.loc * 3.0, 1.0, fwl));
    shine = 0.06;
  }
  if (a < 0.004) { discard; }
  let N = normalize(in.P - G.planet.xyz);
  let V = normalize(in.P);
  let Hh = normalize(G.sun_dir.xyz - V);
  let l = light_at(in.P);
  let spec = pow(max(dot(N, Hh), 0.0), 60.0) * shine * shadow_at(in.P, N);
  var extra = vec3f(0.0);
  let night = night_at(l.upn);
  if (flood > 0.0 && night > 0.0) {
    // Floodlit aprons: warm, uneven pools under the masts.
    let pool = 0.6 + 0.4 * wn(in.loc, 140.0, fwl);
    extra = albedo * LAMP_COL * pool * night * 0.06 * G.sun_dir.w;
  }
  return GOut(vec4f(shade(in.P, N, albedo) + l.sun * spec + extra, a), vec4f(0.0), 0.0);
}
