// Ground-truth ambient occlusion (after Jimenez et al. 2016, "Practical
// Realtime Strategies for Accurate Indirect Occlusion", and XeGTAO). For each
// pixel a few slices through the depth buffer: in each, the highest horizon on
// either side bounds how much sky the surface sees, and the cosine-weighted
// visible arc is integrated exactly. Worked in camera-relative world space.
// The slice directions turn in a 4x4 pattern that `fs_blur` averages away.
@group(1) @binding(0) var t_depth: texture_depth_2d;
@group(1) @binding(1) var t_aux: texture_2d<f32>;
@group(1) @binding(2) var t_ao: texture_2d<f32>;

const SLICES = 2;
const STEPS = 6;
// World radius of the search: a share of the distance (so streets, canopies
// and the cockpit all get contact shading at their own scale), within limits.
const RADIUS_K = 0.04;
const RADIUS_MIN = 0.35;
const RADIUS_MAX = 40.0;
const MAX_PX = 110.0;
const HALF_PI = 1.5707963;

// Camera-relative position of a pixel at reverse-Z depth z. The unprojected
// point is affine in the pixel coordinates before the divide, so a pass sets
// up its row and column steps once (`setup`) and each tap costs a few FMAs.
struct Unproj { h0: vec4f, hx: vec4f, hy: vec4f };
fn setup() -> Unproj {
  let m = G.inv_view_proj;
  let sx = 2.0 * G.screen.z;
  let sy = -2.0 * G.screen.w;
  return Unproj(m * vec4f(-1.0 + 0.5 * sx, 1.0 + 0.5 * sy, 1.0, 1.0), m[0] * sx, m[1] * sy);
}
fn pos_at(u: Unproj, px: vec2f, z: f32) -> vec3f {
  let h = u.h0 + u.hx * px.x + u.hy * px.y;
  return h.xyz / (h.w * z);
}

fn bayer4(p: vec2i) -> f32 {
  let m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
  return m[(p.y & 3) * 4 + (p.x & 3)] / 16.0;
}

@fragment fn fs_gtao(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let z = textureLoad(t_depth, px, 0);
  if (z == 0.0) { return vec4f(1.0); }
  let aux = textureLoad(t_aux, px, 0);
  // Water is shaded later, from scratch.
  if (aux.a > 0.004) { return vec4f(1.0); }
  let nl = length(aux.xyz);
  if (nl < 1e-4) { return vec4f(1.0); }
  let U = setup();
  let P = pos_at(U, vec2f(px), z);
  let dist = length(P);
  let V = -P / dist;
  // Normals that face away (interpolated, or the back of a leaf) are bent
  // just far enough to face the camera.
  var N = aux.xyz / nl;
  N = normalize(N + V * max(0.05 - dot(N, V), 0.0));
  let R = clamp(dist * RADIUS_K, RADIUS_MIN, RADIUS_MAX);
  // Size of a pixel at this distance.
  let pix = length(pos_at(U, vec2f(px) + vec2f(1.0, 0.0), z) - P);
  let rpx = min(R / max(pix, 1e-6), MAX_PX);
  if (rpx < 2.0) { return vec4f(1.0); }
  let falloff_range = 0.6 * R;
  let falloff_from = R - falloff_range;
  let dims = vec2i(textureDimensions(t_depth));
  let rot = bayer4(px);
  let jit = fract(bayer4(px.yx + vec2i(1, 2)) + 0.03125);
  var vis = 0.0;
  for (var s = 0; s < SLICES; s++) {
    let phi = (f32(s) + rot) * PI / f32(SLICES);
    let dir = vec2f(cos(phi), sin(phi));
    // The slice: the screen direction as a world direction across the view.
    let dw = pos_at(U, vec2f(px) + dir, z) - P;
    let ortho = normalize(dw - V * dot(dw, V));
    let axis = cross(ortho, V);
    let pn = N - axis * dot(N, axis);
    let pnl = length(pn);
    if (pnl < 1e-4) { vis += 1.0; continue; }
    let cosn = clamp(dot(pn, V) / pnl, 0.0, 1.0);
    let n = sign(dot(ortho, pn)) * acos(cosn);
    let low0 = cos(n + HALF_PI);
    let low1 = cos(n - HALF_PI);
    var hc0 = low0;
    var hc1 = low1;
    for (var j = 0; j < STEPS; j++) {
      var t = (f32(j) + jit) / f32(STEPS);
      t = t * t;
      let off = vec2i(round(dir * (t * rpx + 1.0)));
      let p0 = clamp(px + off, vec2i(0), dims - 1);
      let p1 = clamp(px - off, vec2i(0), dims - 1);
      let z0 = textureLoad(t_depth, p0, 0);
      let z1 = textureLoad(t_depth, p1, 0);
      if (z0 > 0.0) {
        let d = pos_at(U, vec2f(p0), z0) - P;
        let l = length(d);
        let w = clamp(1.0 - (l - falloff_from) / falloff_range, 0.0, 1.0);
        hc0 = max(hc0, mix(low0, dot(d, V) / max(l, 1e-6), w));
      }
      if (z1 > 0.0) {
        let d = pos_at(U, vec2f(p1), z1) - P;
        let l = length(d);
        let w = clamp(1.0 - (l - falloff_from) / falloff_range, 0.0, 1.0);
        hc1 = max(hc1, mix(low1, dot(d, V) / max(l, 1e-6), w));
      }
    }
    var h0 = -acos(clamp(hc1, -1.0, 1.0));
    var h1 = acos(clamp(hc0, -1.0, 1.0));
    h0 = n + clamp(h0 - n, -HALF_PI, HALF_PI);
    h1 = n + clamp(h1 - n, -HALF_PI, HALF_PI);
    let sn = sin(n);
    let a0 = cosn + 2.0 * h0 * sn - cos(2.0 * h0 - n);
    let a1 = cosn + 2.0 * h1 * sn - cos(2.0 * h1 - n);
    vis += pnl * (a0 + a1) * 0.25;
  }
  vis = clamp(vis / f32(SLICES), 0.0, 1.0);
  // Fades out where the search would only find a pixel or two, and partly
  // with distance, where the haze would wash it out anyway and it only dulls.
  vis = mix(1.0, vis, smoothstep(2.0, 6.0, rpx) * (1.0 - 0.65 * smoothstep(400.0, 3000.0, dist)));
  return vec4f(vis, 0.0, 0.0, 1.0);
}

// Averages the 4x4 pattern over neighbours on the same surface: a sample
// counts in proportion to how close it lies to this pixel's tangent plane.
@fragment fn fs_blur(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let z = textureLoad(t_depth, px, 0);
  if (z == 0.0) { return vec4f(1.0); }
  let aux = textureLoad(t_aux, px, 0);
  let nl = length(aux.xyz);
  let U = setup();
  let P = pos_at(U, vec2f(px), z);
  let dist = length(P);
  var N = -P / dist;
  if (nl > 1e-4 && aux.a <= 0.004) { N = aux.xyz / nl; }
  let tol = 1.0 / (0.012 * dist + 0.03);
  let dims = vec2i(textureDimensions(t_depth));
  var sum = 0.0;
  var wsum = 0.0;
  for (var y = -2; y < 2; y++) {
    for (var x = -2; x < 2; x++) {
      let q = clamp(px + vec2i(x, y), vec2i(0), dims - 1);
      let zq = textureLoad(t_depth, q, 0);
      if (zq == 0.0) { continue; }
      let w = max(1.0 - abs(dot(pos_at(U, vec2f(q), zq) - P, N)) * tol, 0.0) + 1e-4;
      sum += textureLoad(t_ao, q, 0).r * w;
      wsum += w;
    }
  }
  return vec4f(sum / max(wsum, 1e-6), 0.0, 0.0, 1.0);
}
