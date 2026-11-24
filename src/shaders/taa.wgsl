// Temporal anti-aliasing: every frame is drawn a fraction of a pixel off the
// last (G.taa.xy), and the finished HDR frame is blended into a history of
// the frames before it, found again by reprojecting each pixel's depth.
@group(1) @binding(0) var t_cur: texture_2d<f32>;
@group(1) @binding(1) var t_hist: texture_2d<f32>;
@group(1) @binding(2) var t_depth: texture_depth_2d;

// Share of the new frame in the result: for a pixel at rest, and for one
// crossing the screen. Every frame a moving pixel's history is read from
// between the pixels of the last, which softens it a little each time, so
// once it moves at all the history counts for less (even a twentieth of a
// pixel a frame, as distant scenery drifts in flight, adds up).
const TAA_BLEND = 0.1;
const TAA_BLEND_MOVING = 0.3;
// Width of the colour box the history is held to, in standard deviations of
// the pixel's neighbourhood. Tighter ghosts less, wider flickers less.
const TAA_CLIP = 1.25;

// Blend in a compressed range (undone afterwards) so one very bright sample
// - a glint, a lamp - does not outweigh the rest and flicker.
fn taa_k() -> f32 { return G.time.z * 1.6; }
fn squash(c: vec3f) -> vec3f { return c / (1.0 + luma(c) * taa_k()); }
fn unsquash(c: vec3f) -> vec3f { return c / max(1.0 - luma(c) * taa_k(), 1e-4); }
fn to_ycc(c: vec3f) -> vec3f {
  return vec3f(0.25 * c.r + 0.5 * c.g + 0.25 * c.b, 0.5 * c.r - 0.5 * c.b, -0.25 * c.r + 0.5 * c.g - 0.25 * c.b);
}
fn from_ycc(c: vec3f) -> vec3f { return vec3f(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z); }

// Catmull-Rom lookup in nine bilinear taps: a plain bilinear lookup of the
// history would blur it a little more every frame.
fn history(uv: vec2f) -> vec3f {
  let dims = vec2f(textureDimensions(t_hist));
  let p = uv * dims;
  let c = floor(p - 0.5) + 0.5;
  let f = p - c;
  let w0 = f * (-0.5 + f * (1.0 - 0.5 * f));
  let w1 = 1.0 + f * f * (-2.5 + 1.5 * f);
  let w2 = f * (0.5 + f * (2.0 - 1.5 * f));
  let w3 = f * f * (-0.5 + 0.5 * f);
  let w12 = w1 + w2;
  let t0 = (c - 1.0) / dims;
  let t12 = (c + w2 / w12) / dims;
  let t3 = (c + 2.0) / dims;
  var s = textureSampleLevel(t_hist, s_clamp, vec2f(t0.x, t0.y), 0.0).rgb * (w0.x * w0.y);
  s += textureSampleLevel(t_hist, s_clamp, vec2f(t12.x, t0.y), 0.0).rgb * (w12.x * w0.y);
  s += textureSampleLevel(t_hist, s_clamp, vec2f(t3.x, t0.y), 0.0).rgb * (w3.x * w0.y);
  s += textureSampleLevel(t_hist, s_clamp, vec2f(t0.x, t12.y), 0.0).rgb * (w0.x * w12.y);
  s += textureSampleLevel(t_hist, s_clamp, vec2f(t12.x, t12.y), 0.0).rgb * (w12.x * w12.y);
  s += textureSampleLevel(t_hist, s_clamp, vec2f(t3.x, t12.y), 0.0).rgb * (w3.x * w12.y);
  s += textureSampleLevel(t_hist, s_clamp, vec2f(t0.x, t3.y), 0.0).rgb * (w0.x * w3.y);
  s += textureSampleLevel(t_hist, s_clamp, vec2f(t12.x, t3.y), 0.0).rgb * (w12.x * w3.y);
  s += textureSampleLevel(t_hist, s_clamp, vec2f(t3.x, t3.y), 0.0).rgb * (w3.x * w3.y);
  return max(s, vec3f(0.0));
}

@fragment fn fs_taa(in: FsIn) -> @location(0) vec4f {
  let dims = vec2i(textureDimensions(t_cur));
  let px = vec2i(in.pos.xy);
  let cur4 = textureLoad(t_cur, px, 0);
  let cur = cur4.rgb;
  if (G.taa.z < 0.5) { return vec4f(cur, 0.0); }
  // Rain streaks and lightning (precip.wgsl) mark what they cover in alpha:
  // they last a frame or two, so this frame's colour wins there.
  let react = clamp((1.0 - cur4.a) * 4.0, 0.0, 1.0);
  // The colours around this pixel bound what its history may plausibly be;
  // the nearest surface around it says how it moved (so an edge keeps the
  // motion of the thing in front).
  let yc = to_ycc(squash(cur));
  var m1 = vec3f(0.0);
  var m2 = vec3f(0.0);
  var mn = yc;
  var mx = yc;
  var zc = 0.0;
  var off = vec2i(0);
  for (var y = -1; y <= 1; y++) {
    for (var x = -1; x <= 1; x++) {
      let q = clamp(px + vec2i(x, y), vec2i(0), dims - 1);
      let s = to_ycc(squash(textureLoad(t_cur, q, 0).rgb));
      m1 += s;
      m2 += s * s;
      mn = min(mn, s);
      mx = max(mx, s);
      let z = textureLoad(t_depth, q, 0);
      if (z > zc) { zc = z; off = q - px; }
    }
  }
  // Where that surface was on screen last frame.
  let uvn = (vec2f(px + off) + 0.5) * G.screen.zw;
  let ray = view_ray(uvn);
  var pc: vec4f;
  if (zc == 0.0) {
    // Sky: infinitely far, it only turns with the camera.
    pc = G.prev_vp * vec4f(ray, 0.0);
  } else {
    let P = ray / zc;
    let b = (G.plane_inv * vec4f(P, 1.0)).xyz;
    if (G.taa.w > 0.5 && all(abs(b - G.plane_box[0].xyz) < G.plane_box[1].xyz)) {
      // The aircraft flies along with the camera rather than standing in the world.
      pc = G.plane_prev * vec4f(b, 1.0);
    } else {
      pc = G.prev_vp * vec4f(P + G.cam_delta.xyz, 1.0);
    }
  }
  if (pc.w <= 0.0) { return vec4f(cur, 0.0); }
  // The history is of the image without the sub-pixel offset, so take the offset out of the motion.
  let huv = in.uv + (pc.xy / pc.w * vec2f(0.5, -0.5) + 0.5) - (uvn - G.taa.xy);
  if (any(huv < vec2f(0.0)) || any(huv > vec2f(1.0))) { return vec4f(cur, 0.0); }
  let mean = m1 / 9.0;
  let sd = sqrt(max(m2 / 9.0 - mean * mean, vec3f(0.0)));
  let lo = max(mn, mean - sd * TAA_CLIP);
  let hi = min(mx, mean + sd * TAA_CLIP);
  let h = clamp(to_ycc(squash(history(huv))), lo, hi);
  let speed = length((huv - in.uv) * G.screen.xy);
  let moving = smoothstep(0.02, 0.4, speed);
  let a = max(mix(TAA_BLEND, TAA_BLEND_MOVING, moving), react * 0.9);
  let o = unsquash(max(from_ycc(mix(h, yc, a)), vec3f(0.0)));
  // Alpha: how much the pixel is moving, for the sharpening that follows (post.wgsl).
  return vec4f(clamp(o, vec3f(0.0), vec3f(3000.0)), moving);
}
