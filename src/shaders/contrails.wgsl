// Volumetric adaptive-LOD contrail ribbons (contrails.rs).
// Drawn in one instanced pass over the lit scene and clouds (before glass & TAA):
// adjacent segments share exact mitered endpoints from `nodes`, atmospheric
// lighting and aerial perspective are hoisted into the vertex shader, and the
// fragment shader sculpts multi-scale 3D Worley/Perlin cauliflower billows
// (locked to fixed physical arc-length octaves for zero swimming), volumetric
// sun-direction self-shadowing, downwind wind-shear cirrus wisps, soft depth
// occlusion, and cloud cover.

@group(1) @binding(0) var t_depth: texture_depth_2d;
@group(1) @binding(1) var t_cloud: texture_2d<f32>;
@group(1) @binding(3) var t_cdist: texture_2d<f32>;

struct ContrailNode {
  // xyz camera-relative position (m), w physical half-width radius (m)
  pos_r: vec4f,
  // xyz unit tangent along trail, w monotonic arc-length s (m)
  tan_s: vec4f,
  // xyz horizontal wind-shear unit vector (planet frame), w downwind stretch (0..3.5)
  shear: vec4f,
  // x optical density, y age fraction (0..1), z supersat/spread (0..1), w trail seed
  props: vec4f,
};

@group(2) @binding(0) var<storage, read> nodes: array<ContrailNode>;
@group(2) @binding(1) var<storage, read> segs: array<vec2<u32>>;

struct COut {
  @builtin(position) clip: vec4f,
  // x: signed across coord in core-radius units (expanded by QUAD_PAD), y: s_coord (m), z: aa_cov, w: px_width
  @location(0) uv: vec4f,
  // camera-relative world position on ribbon
  @location(1) W: vec3f,
  // ribbon unit across axis (camera-facing)
  @location(2) side: vec3f,
  // x: density, y: k_age, z: supersat, w: seed
  @location(3) props: vec4f,
  // x: shear_proj (-1..1), y: shear_stretch, z: radius (m), w: slant_boost
  @location(4) sh: vec4f,
  // precomputed sunlit + moonlight irradiance at this point
  @location(5) sun_c: vec3f,
  // precomputed sky ambient irradiance at this point
  @location(6) amb_c: vec3f,
  // precomputed aerial-perspective transmittance + horizon inscatter
  @location(7) air_t: vec3f,
  @location(8) air_l: vec3f,
  // trail tangent (for 3D volumetric sun-shadow offset)
  @location(9) tan_v: vec3f,
};

// Generous quad headroom so dramatic billowing puffs & lobes never clip on straight borders.
const QUAD_PAD = 1.75;

fn c_hg(c: f32, g: f32) -> f32 {
  let d = 1.0 + g * g - 2.0 * g * c;
  return (1.0 - g * g) / (4.0 * PI * d * sqrt(d));
}

// Cloud transmittance in front of a contrail at distance `d` (m).
fn contrail_cloud_cover(px: vec2f, d: f32) -> f32 {
  if (G.lite.y < 0.5) { return 1.0; }
  let dims = vec2i(textureDimensions(t_cloud)) - 1;
  let c = clamp(vec2i(px * 0.5), vec2i(0), dims);
  let T = textureLoad(t_cloud, c, 0).a;
  let dc = textureLoad(t_cdist, c, 0).r * 1000.0;
  if (dc <= 0.0) { return 1.0; }
  return mix(T, 1.0, smoothstep(-0.14, 0.14, (dc - d) / max(d, 1.0)));
}

// Compute a continuous, well-behaved across vector for a node given only its own
// position and tangent so adjacent segments meet seamlessly with zero cracks.
fn node_side(pos: vec3f, tan_v: vec3f) -> vec4f {
  let d = max(length(pos), 1e-3);
  let v = pos / d;
  let cr = cross(tan_v, v);
  let cr_len = length(cr);
  let fallback_axis = select(G.up.xyz, G.east.xyz, abs(dot(tan_v, G.up.xyz)) > 0.9);
  let fb = normalize(cross(tan_v, fallback_axis));
  let side = normalize(mix(fb, cr / max(cr_len, 1e-4), smoothstep(0.04, 0.18, cr_len)));
  let slant = 1.0 / max(cr_len, 0.34);
  return vec4f(side, min(slant, 2.0));
}

fn mix_node(a: ContrailNode, b: ContrailNode, t: f32) -> ContrailNode {
  var o: ContrailNode;
  o.pos_r = mix(a.pos_r, b.pos_r, t);
  let tn = mix(a.tan_s.xyz, b.tan_s.xyz, t);
  let tnl = max(length(tn), 1e-5);
  o.tan_s = vec4f(tn / tnl, mix(a.tan_s.w, b.tan_s.w, t));
  let sh = mix(a.shear.xyz, b.shear.xyz, t);
  let shl = max(length(sh), 1e-5);
  o.shear = vec4f(sh / shl, mix(a.shear.w, b.shear.w, t));
  o.props = mix(a.props, b.props, t);
  return o;
}

@vertex fn vs_contrail(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> COut {
  var out: COut;
  out.clip = vec4f(2.0, 2.0, 2.0, 1.0);
  let seg = segs[ii];
  var na = nodes[seg.x];
  var nb = nodes[seg.y];
  if (na.props.x <= 0.001 && nb.props.x <= 0.001) { return out; }

  // Near-plane clipping in clip w so contrails behind or crossing the camera never invert.
  let wn = G.misc.x * 2.5;
  let ca = G.view_proj * vec4f(na.pos_r.xyz, 1.0);
  let cb = G.view_proj * vec4f(nb.pos_r.xyz, 1.0);
  if (ca.w < wn && cb.w < wn) { return out; }
  if (ca.w < wn) {
    let t = clamp((wn - ca.w) / max(cb.w - ca.w, 1e-4), 0.0, 1.0);
    na = mix_node(na, nb, t);
  } else if (cb.w < wn) {
    let t = clamp((wn - cb.w) / max(ca.w - cb.w, 1e-4), 0.0, 1.0);
    nb = mix_node(nb, na, t);
  }

  let corners = array<vec2f, 6>(
    vec2f(-1.0, 0.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0),
    vec2f(-1.0, 0.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0),
  );
  let c = corners[vi];
  var nd = na;
  if (c.y > 0.5) { nd = nb; }

  let P = nd.pos_r.xyz;
  let dist = max(length(P), 1e-3);
  let r_phys = max(nd.pos_r.w, 0.25);
  let side_sl = node_side(P, nd.tan_s.xyz);
  let side = side_sl.xyz;
  let slant = side_sl.w;

  // Asymmetric downwind wind-shear widening + billow headroom.
  let shear_proj = dot(nd.shear.xyz, side);
  let stretch = nd.shear.w;
  let side_ext = select(
    1.0 + max(-shear_proj, 0.0) * stretch,
    1.0 + max(shear_proj, 0.0) * stretch,
    c.x > 0.0,
  );
  let r_side_phys = r_phys * side_ext * QUAD_PAD;

  // Minimum pixel width anti-aliasing so distant 60 km contrails stay silky smooth.
  let px_m = dist * G.misc.w;
  let r_min = px_m * 1.9;
  let r_draw = max(r_side_phys, r_min);
  let aa_cov = clamp(r_side_phys / max(r_draw, 1e-4), 0.0, 1.0);
  let px_width = (r_phys * 2.0) / max(px_m, 1e-4);

  let W = P + side * (c.x * r_draw);
  out.clip = G.view_proj * vec4f(W, 1.0);
  out.uv = vec4f(c.x * side_ext * QUAD_PAD, nd.tan_s.w, aa_cov, px_width);
  out.W = W;
  out.side = side;
  out.props = nd.props;
  out.sh = vec4f(shear_proj, stretch, r_phys, slant);
  out.tan_v = nd.tan_s.xyz;

  // Hoist atmospheric lighting & aerial perspective per vertex (zero LUT samples in fragment shader!).
  let pp = P - G.planet.xyz;
  let rp = max(length(pp), G.planet.w);
  let alt = rp - G.planet.w;
  let upn = pp / rp;
  let mu = dot(upn, G.sun_dir.xyz);
  let moon_c = moon_light(upn, alt);
  out.sun_c = sun_trans(alt, mu) * G.sun_dir.w + moon_c * 2.2;
  // Night airglow floor matches sky.wgsl so night contrails are pale moonlit wisps, NEVER dark black stripes!
  let night = 1.0 - smoothstep(-0.2, 0.05, mu);
  let airglow = vec3f(0.00055, 0.00095, 0.0021) * night;
  let sky_up = sky_lut(upn);
  let sky_dir = sky_lut(P / dist);
  out.amb_c = sky_up * 0.44 + ms_lut(alt, mu) * G.sun_dir.w * 0.08 + airglow * 1.15 + moon_c * 0.35;

  let h_mid = 0.5 * (max(G.up.w, 0.0) + max(alt, 0.0));
  let ext = atm_ext(atm_density(h_mid));
  let air_t = exp(-ext * min(dist, 220000.0));
  out.air_t = air_t;
  out.air_l = (sky_dir + airglow) * (vec3f(1.0) - air_t);
  return out;
}

// Multi-scale locked-octave volumetric density:
// Blends fixed physical arc-length octaves (28 m tight jet turbulence -> 110 m vortex billows
// -> 420 m cirrus/shear clouds) by `radius`, keeping ~1:1 organic cauliflower puff proportions
// at every scale with ZERO texture swimming!
fn contrail_density_at(
  s: f32,
  u_raw: f32,
  z_cyl: f32,
  radius: f32,
  k_age: f32,
  supersat: f32,
  shear_proj: f32,
  stretch: f32,
  seed: f32,
  detail: f32,
) -> f32 {
  let spread_k = supersat * smoothstep(0.03, 0.55, k_age);
  let side_scale = select(1.0, 1.0 + abs(shear_proj) * stretch, u_raw * shear_proj > 0.0);
  let u_edge = abs(u_raw) / max(side_scale * QUAD_PAD, 1.0);
  if (u_edge >= 1.0) { return 0.0; }

  let downwind = max(u_raw * sign(shear_proj), 0.0) * abs(shear_proj);
  let seed_v = vec3f(seed * 4.17, seed * 2.31, seed * 6.73);
  let skew = downwind * (0.45 + 1.35 * spread_k);

  // Choose two adjacent fixed-scale physical octaves based on physical radius:
  // Tight plume (r ~ 1..8 m): 26 m & 105 m octaves -> ~3..12 m turbulent cauliflower puffs!
  // Mature/spreading plume (r ~ 15..250 m): 105 m & 420 m octaves -> ~15..65 m billows & cirrus wisps!
  let big_blend = smoothstep(7.0, 32.0, radius);
  let p_a = vec3f(s / 26.0 - skew * 0.45, u_raw * 0.24 + z_cyl * 0.22, seed * 1.7) + seed_v;
  let p_b = vec3f(s / 105.0 - skew * 0.85, u_raw * 0.22 + z_cyl * 0.20, s / 580.0 + seed * 2.9) + seed_v;
  let n_b = textureSampleLevel(t_noise, s_repeat, p_b, 0.0);
  // Full-range [0..1] cauliflower contrast (t_noise channels sit mostly in 0.28..0.80).
  let puff_b = smoothstep(0.28, 0.78, n_b.r * 0.42 + n_b.g * 0.36 + n_b.b * 0.22);

  var puff = puff_b;
  var wisp_tex = smoothstep(0.26, 0.78, n_b.b * 0.6 + n_b.a * 0.4);
  var meander_n = n_b.r;

  if (detail > 0.01) {
    // Tight/young: 26 m turbulent jet eddies; wide/spreading: 420 m wind-sheared cirrus filaments!
    var q_hi = p_a;
    if (big_blend > 0.5) {
      q_hi = vec3f(s / 420.0 - skew * 1.55, u_raw * 0.58 + z_cyl * 0.28, k_age * 0.22 + seed * 3.7) + seed_v;
    }
    let n_hi = textureSampleLevel(t_noise, s_repeat, q_hi, 0.0);
    let puff_hi = smoothstep(0.26, 0.78, n_hi.g * 0.52 + n_hi.b * 0.30 + n_hi.a * 0.18);
    let mix_k = select(1.0 - big_blend * 1.3, (big_blend - 0.5) * 1.7, big_blend > 0.5);
    let combined = mix(puff_b, puff_b * 0.40 + puff_hi * 0.72, clamp(mix_k, 0.35, 0.88) * detail);
    // Fine Worley erosion carves frothy, lacy turbulent edges!
    let fine_erode = smoothstep(0.25, 0.82, n_hi.b * 0.6 + n_hi.a * 0.4);
    puff = clamp(combined - (1.0 - fine_erode) * mix(0.26, 0.42, spread_k) * detail, 0.0, 1.0);
    wisp_tex = mix(wisp_tex, smoothstep(0.25, 0.76, n_hi.g * 0.5 + n_hi.a * 0.5), detail);
    meander_n = mix(n_b.r, n_hi.r, (1.0 - big_blend) * 0.60 * detail);
  }

  // Organic centerline meander & billowing Crow-instability lumps (zero straight edges!).
  let meander = (meander_n - 0.5) * (0.55 + 0.75 * smoothstep(0.01, 0.30, k_age));
  let u_c = u_raw + meander;
  let r_cyl = length(vec2f(u_c, z_cyl));

  // Frothy cauliflower silhouette: puffs bulge out up to ~1.55r and carve in to ~0.28r!
  let puff_radius = mix(0.28, 1.55, puff);
  let radial = r_cyl / max(puff_radius, 0.18);
  var core = clamp(1.0 - radial * radial, 0.0, 1.0) * mix(0.28, 1.30, puff);
  core = smoothstep(0.0, mix(0.26, 0.52, spread_k), core);

  // Downwind wind-sheared cirrus wisps streaming away from the core in moist/windy air!
  var wisp = 0.0;
  if (stretch > 0.04 && downwind > 0.04) {
    let dw_norm = downwind / max(1.0 + stretch * 0.90, 1.0);
    let env = smoothstep(1.0, 0.06, u_edge) * smoothstep(0.03, 0.35, dw_norm) * (1.0 - smoothstep(0.58, 1.35, dw_norm));
    let streaks = smoothstep(0.20, 0.78, puff * 0.48 + wisp_tex * 0.68);
    wisp = env * streaks * (0.48 + 0.52 * spread_k) * smoothstep(0.04, 0.48, stretch);
  }

  let bound = smoothstep(1.0, 0.78, u_edge);
  let knots = mix(0.55, 1.35, smoothstep(0.24, 0.78, n_b.g));
  return max(core * mix(1.0, 0.72, spread_k), wisp * 0.80) * knots * bound;
}

@fragment fn fs_contrail(in: COut) -> @location(0) vec4f {
  let dens_in = in.props.x;
  if (dens_in <= 0.002) { discard; }

  let dist = max(length(in.W), 1e-3);
  let rd = in.W / dist;
  let radius = in.sh.z;

  // 1. Soft scene-depth occlusion (terrain, wings, cockpit frame, buildings).
  let z = textureLoad(t_depth, vec2i(in.clip.xy), 0);
  var soft_depth = 1.0;
  if (z > 0.0) {
    let t_scene = length(view_ray(in.clip.xy * G.screen.zw) / z);
    let soft_w = max(radius * 0.85, 3.5);
    soft_depth = clamp((t_scene - dist) / soft_w + 0.35, 0.0, 1.0);
    if (soft_depth <= 0.002) { discard; }
  }

  let near_fade = smoothstep(radius * 0.30, radius * 1.45 + 5.0, dist);
  if (near_fade <= 0.002) { discard; }

  let u_raw = in.uv.x;
  let s = in.uv.y;
  let aa_cov = in.uv.z;
  let px_w = in.uv.w;
  let k_age = in.props.y;
  let supersat = in.props.z;
  let seed = in.props.w;
  let shear_proj = in.sh.x;
  let stretch = in.sh.y;
  let slant = in.sh.w;
  let spread_k = supersat * smoothstep(0.04, 0.60, k_age);

  // Distance-adaptive detail LOD: rich multi-octave billows close up, lean single-octave far away!
  let detail = smoothstep(2.0, 9.0, px_w);

  // 2. Volumetric density evaluation (2-sample front+back cylinder when close, 1 sample far).
  let d_front = contrail_density_at(s, u_raw, -0.24, radius, k_age, supersat, shear_proj, stretch, seed, detail);
  var d_val = d_front;
  if (px_w > 6.5) {
    let d_back = contrail_density_at(s, u_raw, 0.28, radius, k_age, supersat, shear_proj, stretch, seed, detail * 0.75);
    d_val = 0.56 * d_front + 0.44 * d_back;
  }
  if (d_val <= 0.003) { discard; }

  // 3. Real volumetric sun-direction self-shadowing (1 offset sample toward the sun!).
  let sun = G.sun_dir.xyz;
  let sun_across = dot(sun, in.side);
  let sun_along = dot(sun, in.tan_v);
  let sun_depth = dot(sun, -rd);
  let sh_step = 0.70;
  let d_sun = contrail_density_at(
    s + sun_along * radius * sh_step,
    u_raw + sun_across * sh_step,
    sun_depth * sh_step * 0.55,
    radius,
    k_age,
    supersat,
    shear_proj,
    stretch,
    seed,
    detail * 0.45,
  );

  // Beer-Lambert optical depth & transmittance.
  let tau = dens_in * d_val * mix(1.0, slant, 0.42) * 1.18;
  let alpha_raw = 1.0 - exp(-tau);

  // Volumetric sun attenuation through the contrail's own billows (gives crisp sunlit crests & blue-grey undersides!).
  let od_sun = dens_in * (0.50 * d_val + 1.10 * d_sun) * mix(1.75, 0.65, spread_k);
  let cs = dot(rd, sun);
  let ph1 = c_hg(cs, 0.78) * 0.42 + c_hg(cs, 0.35) * 0.36 + c_hg(cs, -0.18) * 0.22;
  let ph2 = c_hg(cs, 0.32) * 0.55 + 0.09;
  let ms = exp(-od_sun) * ph1 + 0.42 * exp(-od_sun * 0.30) * ph2;

  let powder = mix(1.0, 1.0 - 0.32 * exp(-tau * 1.8), 1.0 - max(cs, 0.0));
  let silver = exp(-od_sun * 0.45) * pow(max(cs, 0.0), 5.0) * 0.38;

  let u_cyl = clamp(u_raw, -1.0, 1.0);
  let N = normalize(in.side * (u_cyl * 0.75) - rd * sqrt(max(1.0 - u_cyl * u_cyl, 0.12)));
  let sky_fac = 0.74 + 0.26 * max(dot(N, G.up.xyz), -0.4);

  let L_sun = in.sun_c * (ms * 1.25 * powder + silver * 0.35);
  let L_amb = in.amb_c * sky_fac * (0.68 + 0.32 * exp(-od_sun * 0.45));
  let L_local = (L_sun + L_amb) * vec3f(0.95, 0.97, 1.0);

  let L_final = L_local * in.air_t + in.air_l;

  let cloud_vis = contrail_cloud_cover(in.clip.xy, dist);
  let alpha = clamp(alpha_raw * aa_cov * soft_depth * near_fade * cloud_vis, 0.0, 0.94);
  if (alpha <= 0.002) { discard; }

  return vec4f(L_final * alpha, alpha);
}
