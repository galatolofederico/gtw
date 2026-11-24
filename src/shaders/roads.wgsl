// Roads: ribbons laid over the terrain and blended onto it, with lanes,
// markings, kerbs and sidewalks drawn procedurally.
struct BTile { offset: vec4f, ax: vec4f, ay: vec4f };
@group(1) @binding(0) var<uniform> T: BTile;

struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) P: vec3f,
  @location(1) q: vec4f,                       // across, along, half width, distance to end
  @location(2) @interpolate(flat) kind: u32,
};

@vertex fn vs(@location(0) pos: vec3f, @location(1) q: vec4f, @location(2) kind: f32) -> VOut {
  var P = pos + T.offset.xyz;
  P += normalize(P - G.planet.xyz) * crater_field(P).w;
  var clip = G.view_proj * vec4f(P, 1.0);
  // Pull slightly toward the eye so the ribbon wins against the terrain it lies on.
  clip.z *= 1.0 + 0.0018 + 0.35 / max(clip.w, 0.5);
  return VOut(clip, P, q, u32(kind + 0.5));
}

struct GOut { @location(0) c: vec4f, @location(1) n: vec4f, @location(2) amb: f32 };
@fragment fn fs(in: VOut) -> GOut {
  // Inside the 3D tiles zone the mesh stands in for this (tiles3d.rs).
  if (photo_hides(in.P, in.clip.xy)) { discard; }
  let dist = length(in.P);
  let fw = max(fwidth(in.q.x), fwidth(in.q.y));
  // Which way the nearest junction lies along the road.
  let app = sign(dpdx(in.q.w) * dpdx(in.q.y) + dpdy(in.q.w) * dpdy(in.q.y));
  let rs = road_surface(in.q.x, in.q.y, in.q.z, in.kind, in.q.w, app, fw);
  var a = rs.a * (1.0 - smoothstep(2600.0, 4200.0, dist));
  if (a < 0.004) { discard; }
  var N = normalize(in.P - G.planet.xyz);
  let cr = crater_field(in.P);
  N = normalize(N - cr.xyz);
  // Roads break up and scorch where the ground was blown out.
  a *= 1.0 - clamp(abs(cr.w) * 2.0, 0.0, 1.0);
  var albedo = rs.rgb;
  for (var i = 0; i < 8; i++) {
    let s = G.scorch[i];
    if (s.w > 0.0) { albedo = mix(albedo, vec3f(0.012), smoothstep(1.0, 0.3, length(in.P - s.xyz) / s.w) * 0.9); }
  }
  // Dirt tracks and footpaths are not cleared: they disappear under the snow
  // like the ground around them (snow.rs; the terrain's own noise is left
  // out, so their edges stay a little softer than the fields' patches).
  let rclass = in.kind & 15u;
  var snowy = 0.0;
  if ((rclass == 7u || rclass == 8u) && snow_on()) {
    snowy = snow_at(in.P, N, N, 0.0);
    let ks_orig = snowy;
    var sc = SNOW_ALBEDO;
    if (snowy > 0.005) {
      let tr = snow_track_at(in.P, dist, fw);
      if (tr.x > 0.002 || tr.y > 0.002) {
        let reveal = smoothstep(0.12, 0.68, tr.x);
        snowy = clamp(ks_orig * (1.0 - reveal) + tr.y * 0.30 * ks_orig, 0.0, 1.0);
        sc *= mix(1.0, 0.78, tr.x * (1.0 - reveal * 0.45));
        albedo *= 1.0 - 0.32 * reveal * ks_orig;
        let n_fade = (1.0 - smoothstep(180.0, 480.0, dist)) * ks_orig;
        N = normalize(N - (G.snow_tr_e.xyz * tr.z + G.snow_tr_n.xyz * tr.w) * (1.05 * n_fade));
      }
    }
    albedo = mix(albedo, sc, snowy);
  }
  // Wet-look sheen toward the sun at grazing angles.
  let V = normalize(in.P);
  let Hh = normalize(G.sun_dir.xyz - V);
  let l = light_at(in.P);
  let spec = pow(max(dot(N, Hh), 0.0), 40.0) * 0.04 * shadow_at(in.P, N) * (1.0 - snowy);
  // Pools of light under the street lamps.
  var lamps = vec3f(0.0);
  let night = night_at(l.upn);
  if (night > 0.0) { lamps = albedo * LAMP_COL * lamp_pool(in.q.x, in.q.y, in.q.z, in.kind) * night * 0.09 * G.sun_dir.w; }
  return GOut(vec4f(shade(in.P, N, albedo) + l.sun * spec + lamps, a), vec4f(0.0), 0.0);
}

// Our street lamps seen from above, into the blue channel of the 3D tiles'
// coverage mask (alpha: the road's height): the photogrammetry mesh, which
// hides these roads, takes its night light from it (photo.wgsl).
@group(2) @binding(0) var<uniform> CS: Cascade;
struct TopOut {
  @builtin(position) clip: vec4f,
  @location(0) q: vec4f,
  @location(1) h: f32,
  @location(2) @interpolate(flat) kind: u32,
  @location(3) dist: f32,
};
@vertex fn vs_top(@location(0) pos: vec3f, @location(1) q: vec4f, @location(2) kind: f32) -> TopOut {
  let P = pos + T.offset.xyz;
  return TopOut(CS.vp * vec4f(P, 1.0), q, height_of(P), u32(kind + 0.5), length(P));
}
@fragment fn fs_top(in: TopOut) -> @location(0) vec4f {
  // (fading out with distance a little sooner than the roads themselves, fs
  // above: on the mesh the light spills over pavements and building fronts
  // too, so far streets would read as broad bright bands)
  let pool = lamp_pool(in.q.x, in.q.y, in.q.z, in.kind) * (1.0 - smoothstep(1500.0, 3800.0, in.dist));
  if (pool < 0.01) { discard; }
  return vec4f(0.0, 0.0, pool, in.h);
}
