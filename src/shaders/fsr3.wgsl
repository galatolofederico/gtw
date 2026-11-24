// AMD FidelityFX Super Resolution 3.1.6 (the temporal upscaler), ported to
// WGSL from AMD's FSR SDK v2.4.0 (Kits/FidelityFX/upscalers/fsr3/include/gpu/
// fsr3upscaler/*.h, MIT licence, copyright Advanced Micro Devices, Inc.; the
// notice is in docs/upscaling.md). The passes, constants and maths follow
// AMD's 32-bit paths line for line, configured as the game uses it: HDR colour
// in, reversed infinite depth, render-resolution motion vectors without jitter,
// the reference Lanczos for the history and the approximate one for the new
// samples. Where WGSL differs, the port changes the plumbing only:
//
// - The luminance pyramid (SPD) is only kept for what the upscaler reads from
//   it when the game supplies the exposure: the farthest depth at half
//   resolution (`cs_farthest_mip1`, the same 2x2 mean as SPD's first mip).
// - The shading-change pyramid is built as plain 2x2 means one mip at a time
//   (`cs_shading_pyramid` then `cs_downsample`), which is what SPD computes.
// - The reconstructed previous depth is a storage buffer with atomicMax
//   (WGSL has no image atomics); the new-lock mask is a storage buffer.
// - Formats WebGPU cannot write from a shader (R16F, RG16F, R8) are R32F,
//   RG32F, RGBA16F or RGBA8 instead: as precise or more.
// - Out-of-range loads AMD leaves to D3D (which returns zero) do so explicitly.
// - RCAS runs in the tonemap (post.wgsl `fs_final_fsr3`), not as its own pass.

struct Fsr3C {
  render_size: vec2i,
  prev_render_size: vec2i,
  upscale_size: vec2i,
  prev_upscale_size: vec2i,
  max_render_size: vec2i,
  max_upscale_size: vec2i,
  device_to_view: vec4f,
  jitter: vec2f,
  prev_jitter: vec2f,
  mv_scale: vec2f,
  downscale: vec2f,
  mv_jitter_cancel: vec2f,
  tan_half_fov: f32,
  jitter_seq_len: f32,
  delta_time: f32,
  delta_pre_exposure: f32,
  view_to_meters: f32,
  frame_index: f32,
  velocity_factor: f32,
  reactiveness_scale: f32,
  shading_change_scale: f32,
  accumulation_added_per_frame: f32,
  min_disocclusion_accumulation: f32,
  exposure: f32,
  _pad0: f32,
  _pad1: f32,
}

@group(0) @binding(0) var<uniform> C: Fsr3C;
@group(0) @binding(1) var s_lin: sampler;
// inputs
@group(0) @binding(2) var t_depth: texture_depth_2d;
@group(0) @binding(3) var t_mv: texture_2d<f32>;
@group(0) @binding(4) var t_color: texture_2d<f32>;
// prepare inputs
@group(0) @binding(5) var w_dmv: texture_storage_2d<rg32float, write>;
@group(0) @binding(6) var w_ddepth: texture_storage_2d<r32float, write>;
@group(0) @binding(7) var<storage, read_write> b_recon: array<atomic<u32>>;
@group(0) @binding(8) var w_farthest: texture_storage_2d<r32float, write>;
@group(0) @binding(9) var w_luma: texture_storage_2d<rgba16float, write>;
// farthest depth, half resolution
@group(0) @binding(10) var t_farthest: texture_2d<f32>;
@group(0) @binding(11) var w_fmip1: texture_storage_2d<rgba16float, write>;
// shading change pyramid
@group(0) @binding(12) var t_luma: texture_2d<f32>;
@group(0) @binding(13) var t_pluma: texture_2d<f32>;
@group(0) @binding(14) var t_dmv: texture_2d<f32>;
@group(0) @binding(15) var w_spd: texture_storage_2d<rgba16float, write>;
@group(0) @binding(16) var t_spd_src: texture_2d<f32>;
// shading change
@group(0) @binding(18) var t_spd: texture_2d<f32>;
@group(0) @binding(19) var w_shc: texture_storage_2d<rgba8unorm, write>;
// prepare reactivity
@group(0) @binding(20) var t_ddepth: texture_2d<f32>;
@group(0) @binding(21) var<storage, read> b_recon_r: array<u32>;
@group(0) @binding(22) var t_accum: texture_2d<f32>;
@group(0) @binding(23) var t_shc: texture_2d<f32>;
@group(0) @binding(24) var w_masks: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(25) var<storage, read_write> b_locks: array<f32>;
@group(0) @binding(26) var w_accum: texture_storage_2d<rgba8unorm, write>;
// luma instability
@group(0) @binding(27) var t_masks: texture_2d<f32>;
@group(0) @binding(28) var t_lhist: texture_2d<f32>;
@group(0) @binding(29) var t_fmip1: texture_2d<f32>;
@group(0) @binding(30) var w_lhist: texture_storage_2d<rgba16float, write>;
@group(0) @binding(31) var w_linst: texture_storage_2d<rgba16float, write>;
// accumulate
@group(0) @binding(32) var t_hist: texture_2d<f32>;
@group(0) @binding(33) var t_linst: texture_2d<f32>;
@group(0) @binding(34) var w_hist: texture_storage_2d<rgba16float, write>;

// ffx_fsr3upscaler_common.h
const FP16_MIN = 6.10e-05;
const FP16_MAX = 65504.0;
const EPSILON = FP16_MIN;
const TONEMAP_EPSILON = FP16_MIN;
const FP32_MAX = 3.402823466e+38;
const FP32_MIN = 1.175494351e-38;
const RECON_BILINEAR_WEIGHT_THRESHOLD = EPSILON * 10.0;
const UPSAMPLE_LANCZOS_WEIGHT_SCALE = 1.0 / 16.0;
const AVERAGE_LANCZOS_WEIGHT_PER_FRAME = 0.74 * UPSAMPLE_LANCZOS_WEIGHT_SCALE;
const LOCK_THRESHOLD = 1.0;
const LOCK_MAX = 2.0;
const REACTIVE = 0;
const DISOCCLUSION = 1;
const SHADING_CHANGE = 2;
const ACCUMULATION = 3;

fn rs() -> vec2i { return C.render_size; }
fn rsf() -> vec2f { return vec2f(C.render_size); }
fn prs() -> vec2i { return C.prev_render_size; }
fn us() -> vec2i { return C.upscale_size; }
fn shading_change_size() -> vec2i { return vec2i(vec2f(C.render_size) * 0.5); }
fn shading_change_max_size() -> vec2i { return vec2i(vec2f(C.max_render_size) * 0.5); }
fn exposure() -> f32 { return select(C.exposure, 1.0, C.exposure == 0.0); }

fn recon_mv_px_threshold(near_m: f32) -> f32 { return mix(0.25, 0.75, saturate(near_m / 100.0)); }
fn velocity_4k(mv: vec2f) -> f32 { return length(mv * vec2f(3840.0, 2160.0)); }
fn min_div_max(v0: f32, v1: f32, on_zero: f32) -> f32 {
  let m = max(v0, v1);
  return select(on_zero, min(v0, v1) / m, m != 0.0);
}
fn ycocg_to_rgb(c: vec3f) -> vec3f { return vec3f(c.x + c.y - c.z, c.x + c.z, c.x - c.y - c.z); }
fn rgb_to_ycocg(c: vec3f) -> vec3f {
  return vec3f(0.25 * c.r + 0.5 * c.g + 0.25 * c.b, 0.5 * c.r - 0.5 * c.b, -0.25 * c.r + 0.5 * c.g - 0.25 * c.b);
}
fn rgb_to_luma(c: vec3f) -> f32 { return dot(c, vec3f(0.2126, 0.7152, 0.0722)); }
fn tonemap(c: vec3f) -> vec3f { return c / (max(max(0.0, c.r), max(c.g, c.b)) + 1.0); }
fn inverse_tonemap(c: vec3f) -> vec3f { return c / max(TONEMAP_EPSILON, 1.0 - max(c.r, max(c.g, c.b))); }
fn uv_inside(uv: vec2f) -> bool { return uv.x >= 0.0 && uv.x <= 1.0 && uv.y >= 0.0 && uv.y <= 1.0; }
fn clamp_load(p: vec2i, off: vec2i, size: vec2i) -> vec2i { return clamp(p + off, vec2i(0), size - 1); }
fn clamp_uv(uv: vec2f, tex_size: vec2i, res_size: vec2i) -> vec2f {
  let loc = uv * vec2f(tex_size);
  return max(vec2f(0.5), min(loc, vec2f(tex_size) - 0.5)) / vec2f(res_size);
}
fn on_screen(p: vec2i, size: vec2i) -> bool { return all(vec2u(p) < vec2u(size)); }
fn hr_from_lr(lr: vec2i) -> vec2i {
  let jittered = vec2f(lr) + 0.5 - C.jitter;
  return vec2i(floor(jittered / rsf() * vec2f(us())));
}
fn view_depth(d: f32) -> f32 { return C.device_to_view.y / (d - C.device_to_view.x); }
fn view_depth_m(d: f32) -> f32 { return view_depth(d) * C.view_to_meters; }

// The bilinear footprint of a uv on a texture (GetBilinearSamplingData).
struct Bilinear { base: vec2i, w: vec4f }
fn bilinear(uv: vec2f, size: vec2i) -> Bilinear {
  let p = uv * vec2f(size) - 0.5;
  let f = fract(p);
  return Bilinear(vec2i(floor(p)), vec4f((1.0 - f.x) * (1.0 - f.y), f.x * (1.0 - f.y), (1.0 - f.x) * f.y, f.x * f.y));
}
const BILINEAR_OFF = array<vec2i, 4>(vec2i(0, 0), vec2i(1, 0), vec2i(0, 1), vec2i(1, 1));

// Resource accessors (ffx_fsr3upscaler_callbacks_hlsl.h).
fn load_mv(p: vec2i) -> vec2f { return textureLoad(t_mv, p, 0).xy * C.mv_scale; }
fn load_dmv(p: vec2i) -> vec2f { return textureLoad(t_dmv, p, 0).xy; }
fn load_reactive(p: vec2i) -> f32 {
  // The game marks what lasts a frame or two (rain streaks, lightning) in
  // the colour's alpha (precip.wgsl), as the temporal AA reads it.
  return clamp((1.0 - textureLoad(t_color, p, 0).a) * 4.0, 0.0, 1.0) * C.reactiveness_scale;
}
fn recon_index(p: vec2i) -> u32 { return u32(p.y) * u32(C.render_size.x) + u32(p.x); }
fn lock_index(p: vec2i) -> u32 { return u32(p.y) * u32(C.upscale_size.x) + u32(p.x); }

// ---- Prepare inputs (ffx_fsr3upscaler_prepare_inputs.h) ----

fn reconstruct_prev_depth(px: vec2i, depth: f32, mv_in: vec2f) {
  let near_m = min(view_depth_m(depth), FP16_MAX);
  let threshold = recon_mv_px_threshold(near_m);
  // Discard small mvs
  let mv = mv_in * f32(velocity_4k(mv_in) > threshold);
  let uv = (vec2f(px) + 0.5) / rsf();
  let b = bilinear(uv + mv, rs());
  var offs = BILINEAR_OFF;
  // Project current depth into previous frame locations: push to every pixel
  // with some bilinear contribution.
  for (var i = 0; i < 4; i++) {
    if (b.w[i] > RECON_BILINEAR_WEIGHT_THRESHOLD) {
      let p = b.base + offs[i];
      if (on_screen(p, rs())) {
        // max: reversed depth
        atomicMax(&b_recon[recon_index(p)], bitcast<u32>(depth));
      }
    }
  }
}

const EXTENT_OFF = array<vec2i, 9>(vec2i(0, 0), vec2i(1, 0), vec2i(0, 1), vec2i(0, -1), vec2i(-1, 0), vec2i(-1, 1), vec2i(1, 1), vec2i(-1, -1), vec2i(1, -1));

@compute @workgroup_size(8, 8, 1)
fn cs_prepare_inputs(@builtin(global_invocation_id) gid: vec3u) {
  let px = vec2i(gid.xy);
  if (!on_screen(px, rs())) { return; }
  // FindDepthExtents (reversed depth: nearer is larger)
  var nearest = textureLoad(t_depth, px, 0);
  var farthest = nearest;
  var nearest_px = px;
  var offs = EXTENT_OFF;
  for (var i = 1; i < 9; i++) {
    let p = px + offs[i];
    if (on_screen(p, rs())) {
      let d = textureLoad(t_depth, p, 0);
      if (d > nearest) {
        farthest = min(farthest, d);
        nearest_px = p;
        nearest = d;
      }
    }
  }
  // Low resolution motion vectors: the nearest surface's.
  let mv = load_mv(nearest_px);
  reconstruct_prev_depth(px, nearest, mv);
  textureStore(w_dmv, px, vec4f(mv, 0.0, 0.0));
  textureStore(w_ddepth, px, vec4f(nearest, 0.0, 0.0, 0.0));
  textureStore(w_farthest, px, vec4f(min(view_depth_m(farthest), FP16_MAX), 0.0, 0.0, 0.0));
  let rgb = max(vec3f(0.0), textureLoad(t_color, px, 0).rgb);
  textureStore(w_luma, px, vec4f(rgb_to_luma(rgb), 0.0, 0.0, 0.0));
}

// ---- Farthest depth at half resolution (SPD mip 0 of the luma pyramid) ----

@compute @workgroup_size(8, 8, 1)
fn cs_farthest_mip1(@builtin(global_invocation_id) gid: vec3u) {
  let p = vec2i(gid.xy);
  if (!on_screen(p, vec2i(textureDimensions(w_fmip1)))) { return; }
  var s = 0.0;
  var offs = BILINEAR_OFF;
  for (var i = 0; i < 4; i++) {
    s += textureLoad(t_farthest, clamp_load(p * 2, offs[i], rs()), 0).x;
  }
  textureStore(w_fmip1, p, vec4f(s * 0.25, 0.0, 0.0, 0.0));
}

// ---- Shading change pyramid (ffx_fsr3upscaler_shading_change_pyramid.h) ----

const SET_OFF = array<vec2i, 5>(vec2i(0, 0), vec2i(-1, 0), vec2i(1, 0), vec2i(0, -1), vec2i(0, 1));

fn sort5(v: array<f32, 5>) -> array<f32, 5> {
  var s = v;
  // the same compare-and-swap network as SortSet
  var t: f32;
  t = min(s[0], s[3]); s[3] = max(s[0], s[3]); s[0] = t;
  t = min(s[1], s[4]); s[4] = max(s[1], s[4]); s[1] = t;
  t = min(s[0], s[2]); s[2] = max(s[0], s[2]); s[0] = t;
  t = min(s[1], s[3]); s[3] = max(s[1], s[3]); s[1] = t;
  t = min(s[0], s[1]); s[1] = max(s[0], s[1]); s[0] = t;
  t = min(s[2], s[4]); s[4] = max(s[2], s[4]); s[2] = t;
  t = min(s[1], s[2]); s[2] = max(s[1], s[2]); s[1] = t;
  t = min(s[3], s[4]); s[4] = max(s[3], s[4]); s[3] = t;
  t = min(s[2], s[3]); s[3] = max(s[2], s[3]); s[2] = t;
  return s;
}

fn minimum_difference(set0: array<f32, 5>, set1: array<f32, 5>) -> f32 {
  var min_diff = FP16_MAX - 1.0;
  var a = 0;
  var b = 0;
  let s0 = sort5(set0);
  let s1 = sort5(set1);
  let fmax = min(s0[4], s1[4]);
  if (fmax > FP32_MIN) {
    for (var i = 0; i < 5 && min_diff < FP16_MAX; i++) {
      var diff = s0[a] - s1[b];
      if (abs(diff) > FP16_MIN) {
        diff = sign(diff) * (1.0 - min_div_max(s0[a], s1[b], 0.0));
        min_diff = select(min_diff, diff, abs(diff) < abs(min_diff));
        a += i32(s0[a] < s1[b]);
        // (AMD's reads s0 at the new `a`, which may step one past the end on
        // the last round; held to the last element.)
        b += i32(s0[min(a, 4)] >= s1[b]);
        a = min(a, 4);
        b = min(b, 4);
      } else {
        min_diff = FP16_MAX;
      }
    }
  }
  return min_diff * f32(min_diff < FP16_MAX - 1.0);
}

fn shading_diff(p: vec2i, uv: vec2f, mv: vec2f) -> f32 {
  // GetCurrentLumaBilinearSamples
  let cur_base = vec2i(floor((uv + C.jitter / rsf()) * rsf()));
  var offs = SET_OFF;
  var cur: array<f32, 5>;
  for (var i = 0; i < 5; i++) {
    cur[i] = max(textureLoad(t_luma, clamp_load(cur_base, offs[i], rs()), 0).x * exposure(), EPSILON);
  }
  // GetPreviousLumaBilinearSamples
  let ruv = uv + C.prev_jitter / vec2f(prs()) + mv;
  if (!uv_inside(ruv)) { return 0.0; }
  let prev_base = vec2i(floor(ruv * vec2f(prs())));
  var prev: array<f32, 5>;
  for (var i = 0; i < 5; i++) {
    prev[i] = max(textureLoad(t_pluma, clamp_load(prev_base, offs[i], prs()), 0).x * C.delta_pre_exposure * exposure(), EPSILON);
  }
  return minimum_difference(cur, prev);
}

fn shading_source(p_in: vec2i) -> vec2f {
  let p = clamp_load(p_in, vec2i(0), rs());
  let mv = load_dmv(p);
  let uv = (vec2f(p) + 0.5) / rsf();
  let d = shading_diff(p, uv, mv);
  return vec2f(d, select(0.0, sign(d), d != 0.0));
}

// mip 0: the 2x2 mean of the per-pixel difference and its sign
@compute @workgroup_size(8, 8, 1)
fn cs_shading_pyramid(@builtin(global_invocation_id) gid: vec3u) {
  let p = vec2i(gid.xy);
  if (!on_screen(p, vec2i(textureDimensions(w_spd)))) { return; }
  var s = vec2f(0.0);
  var offs = BILINEAR_OFF;
  for (var i = 0; i < 4; i++) {
    s += shading_source(p * 2 + offs[i]);
  }
  textureStore(w_spd, p, vec4f(s * 0.25, 1.0, 0.0));
}

// the following mips: 2x2 means of the one before
@compute @workgroup_size(8, 8, 1)
fn cs_downsample(@builtin(global_invocation_id) gid: vec3u) {
  let p = vec2i(gid.xy);
  if (!on_screen(p, vec2i(textureDimensions(w_spd)))) { return; }
  let src = vec2i(textureDimensions(t_spd_src));
  var s = vec4f(0.0);
  var offs = BILINEAR_OFF;
  for (var i = 0; i < 4; i++) {
    s += textureLoad(t_spd_src, clamp_load(p * 2, offs[i], src), 0);
  }
  textureStore(w_spd, p, s * 0.25);
}

// ---- Shading change (ffx_fsr3upscaler_shading_change.h) ----

@compute @workgroup_size(8, 8, 1)
fn cs_shading_change(@builtin(global_invocation_id) gid: vec3u) {
  let p = vec2i(gid.xy);
  if (!on_screen(p, shading_change_size())) { return; }
  let uv = (vec2f(p) + 0.5) / vec2f(shading_change_size());
  let uvj = uv + C.jitter / rsf();
  let mip_uv = clamp_uv(uvj, shading_change_size(), vec2i(textureDimensions(t_spd, 0)));
  var sc = 0.0;
  for (var mip = 0; mip < 3; mip++) {
    let s = textureSampleLevel(t_spd, s_lin, mip_uv, f32(mip)).xy;
    let v = abs(s.x * s.y);
    if (v > 0.0) { sc = max(sc, v); }
  }
  textureStore(w_shc, p, vec4f(saturate(sc), 0.0, 0.0, 0.0));
}

// ---- Prepare reactivity (ffx_fsr3upscaler_prepare_reactivity.h) ----

fn disocclusions(uv: vec2f, mv_in: vec2f, cur_view: f32) -> f32 {
  let near_m = min(cur_view * C.view_to_meters, FP16_MAX);
  let threshold = recon_mv_px_threshold(near_m);
  let mv = mv_in * f32(velocity_4k(mv_in) > threshold);
  let b = bilinear(uv + mv, rs());
  var disocc = 0.0;
  var wsum = 0.0;
  var potential = true;
  var offs = BILINEAR_OFF;
  for (var i = 0; i < 4 && potential; i++) {
    let p = clamp_load(b.base, offs[i], rs());
    if (on_screen(p, rs())) {
      let w = b.w[i];
      if (w > RECON_BILINEAR_WEIGHT_THRESHOLD) {
        let prev_view = view_depth(bitcast<f32>(b_recon_r[recon_index(p)]));
        let dd = cur_view - prev_view;
        potential = potential && dd > FP32_MIN;
        if (potential) {
          let half_viewport = length(rsf() * 0.5);
          let threshold_depth = max(cur_view, prev_view);
          let ksep = 1.37e-05;
          let required = ksep * half_viewport * threshold_depth;
          disocc += saturate(required / dd) * w;
          wsum += w;
        }
      }
    }
  }
  return select(0.0, saturate(1.0 - disocc / wsum), potential && wsum > 0.0);
}

fn motion_divergence(uv: vec2f, mv: vec2f, cur_depth: f32) -> f32 {
  let rp = vec2i((uv + mv) * rsf());
  // (out of range reads as zero, as on D3D)
  let inside = on_screen(rp, rs());
  let rp_c = clamp(rp, vec2i(0), rs() - 1);
  let r_depth = select(0.0, textureLoad(t_ddepth, rp_c, 0).x, inside);
  let r_mv = select(vec2f(0.0), load_dmv(rp_c), inside);
  let r_vel = velocity_4k(r_mv);
  let vel = velocity_4k(mv);
  let nucleus_m = view_depth_m(r_depth);
  let cur_m = view_depth_m(cur_depth);
  let distance_factor = min_div_max(nucleus_m, cur_m, 0.0);
  let velocity_factor = saturate(vel / 10.0);
  // (with no motion the confidence is nothing, whatever the ratio)
  let ratio = select(0.0, saturate(r_vel / vel), vel > 0.0);
  return (1.0 - ratio) * distance_factor * velocity_factor;
}

fn dilate_reactive(p: vec2i) -> f32 {
  var r = 0.0;
  for (var y = -1; y <= 1; y++) {
    for (var x = -1; x <= 1; x++) {
      r = max(r, load_reactive(clamp_load(p, vec2i(x, y), rs())));
    }
  }
  return r;
}

const THIN_OFF = array<vec2i, 9>(vec2i(0, 0), vec2i(-1, -1), vec2i(0, -1), vec2i(1, -1), vec2i(-1, 0), vec2i(1, 0), vec2i(-1, 1), vec2i(0, 1), vec2i(1, 1));
const REJECTION_MASKS = array<u32, 4>(
  (1u << 1u) | (1u << 2u) | (1u << 4u) | 1u,
  (1u << 2u) | (1u << 3u) | (1u << 5u) | 1u,
  (1u << 4u) | (1u << 6u) | (1u << 7u) | 1u,
  (1u << 5u) | (1u << 7u) | (1u << 8u) | 1u,
);

fn thin_feature_confidence(p: vec2i) -> f32 {
  var s: array<f32, 9>;
  var lo = FP32_MAX;
  var hi = FP32_MIN;
  var offs = THIN_OFF;
  var masks = REJECTION_MASKS;
  for (var i = 0; i < 9; i++) {
    s[i] = textureLoad(t_luma, clamp_load(p, offs[i], rs()), 0).x * exposure();
    lo = min(lo, s[i]);
    hi = max(hi, s[i]);
  }
  // (a flat neighbourhood is no ridge; AMD gets there through 0/0 comparisons)
  if (hi <= lo) { return 0.0; }
  var dis_lo = FP32_MAX;
  var dis_hi = 0.0;
  var pattern = 1u;
  for (var i = 1; i < 9; i++) {
    let d = abs(s[i] - s[0]) / (hi - lo);
    if (d < 0.9) {
      pattern |= 1u << u32(i);
    } else {
      dis_lo = min(dis_lo, s[i]);
      dis_hi = max(dis_hi, s[i]);
    }
  }
  let ridge = s[0] > dis_hi || s[0] < dis_lo;
  if (!ridge) { return 0.0; }
  for (var i = 0; i < 4; i++) {
    if ((pattern & masks[i]) == masks[i]) { return 0.0; }
  }
  return 1.0 - lo / hi;
}

fn update_accumulation(p: vec2i, uv: vec2f, mv: vec2f, disocc: f32, shading: f32) -> f32 {
  let ruv = uv + mv;
  var acc = 0.0;
  if (uv_inside(ruv)) {
    acc = saturate(textureSampleLevel(t_accum, s_lin, clamp_uv(ruv, prs(), C.max_render_size), 0.0).x);
  }
  acc = mix(acc, 0.0, shading);
  acc = mix(acc, min(C.min_disocclusion_accumulation, acc), disocc);
  acc *= f32(round(acc * 100.0) > 1.0);
  textureStore(w_accum, p, vec4f(saturate(acc + C.accumulation_added_per_frame), 0.0, 0.0, 0.0));
  return acc;
}

fn shading_change_at(uv: vec2f) -> f32 {
  // jitter put back: the accumulation takes it out again
  let suv = clamp_uv(uv - C.jitter / rsf(), shading_change_size(), shading_change_max_size());
  return saturate(textureSampleLevel(t_shc, s_lin, suv, 0.0).x * C.shading_change_scale);
}

@compute @workgroup_size(8, 8, 1)
fn cs_prepare_reactivity(@builtin(global_invocation_id) gid: vec3u) {
  let p = vec2i(gid.xy);
  if (!on_screen(p, rs())) { return; }
  let uv = (vec2f(p) + 0.5) / rsf();
  let mv = load_dmv(p);
  let ddepth = textureLoad(t_ddepth, p, 0).x;
  let disocc = disocclusions(uv, mv, view_depth(ddepth));
  let shading = max(dilate_reactive(p), shading_change_at(uv));
  let divergence = motion_divergence(uv, mv, ddepth);
  // (no transparency and composition mask: AMD's default is zero)
  let reactive = divergence;
  let acc = update_accumulation(p, uv, mv, disocc, shading);
  textureStore(w_masks, p, vec4f(reactive, disocc, shading, acc));
  let lock = thin_feature_confidence(p);
  if (lock > 1.0 / 100.0) {
    let hr = hr_from_lr(p);
    if (on_screen(hr, us())) { b_locks[lock_index(hr)] = lock; }
  }
}

// ---- Luma instability (ffx_fsr3upscaler_luma_instability.h) ----
// (AMD samples the half-resolution farthest depth here and hands it to
// ComputeLumaInstabilityFactor, which never reads it; left out.)

@compute @workgroup_size(8, 8, 1)
fn cs_luma_instability(@builtin(global_invocation_id) gid: vec3u) {
  let p = vec2i(gid.xy);
  if (!on_screen(p, rs())) { return; }
  var hist = vec4f(0.0);
  var factor = 0.0;
  let mv = load_dmv(p);
  let uv = (vec2f(p) + 0.5) / rsf();
  let uv_cur = uv + C.jitter / rsf();
  let uv_prev = uv + C.prev_jitter / vec2f(prs());
  let ruv = uv_prev + mv;
  if (uv_inside(ruv)) {
    let masks = textureSampleLevel(t_masks, s_lin, clamp_uv(uv_cur, rs(), C.max_render_size), 0.0);
    let reactive = saturate(masks[REACTIVE]);
    let disocc = saturate(masks[DISOCCLUSION]);
    let shading = saturate(masks[SHADING_CHANGE]);
    let acc = saturate(masks[ACCUMULATION]);
    if (acc > 0.9) {
      let cur = textureSampleLevel(t_luma, s_lin, clamp_uv(uv_cur, rs(), C.max_render_size), 0.0).x * exposure();
      hist = textureSampleLevel(t_lhist, s_lin, clamp_uv(ruv, prs(), C.max_render_size), 0.0) * C.delta_pre_exposure * exposure();
      // ComputeLumaInstabilityFactor
      var instability = 0.0;
      let diff0 = cur - hist[0];
      let sim0 = min_div_max(cur, hist[0], 1.0);
      var max_sim = sim0;
      if (sim0 < 1.0) {
        for (var i = 1; i <= 3; i++) {
          let diff1 = cur - hist[i];
          let sim1 = min_div_max(cur, hist[i], 0.0);
          if (sign(diff0) == sign(diff1)) { max_sim = max(max_sim, sim1); }
        }
        instability = f32(max_sim > sim0);
      }
      hist = vec4f(cur, hist[0], hist[1], hist[2]) / exposure();
      factor = instability * f32(hist[3] != 0.0);
      let vel_w = 1.0 - saturate(velocity_4k(mv) / 20.0);
      factor *= vel_w * (1.0 - disocc) * (1.0 - reactive) * (1.0 - shading);
    }
  }
  textureStore(w_lhist, p, hist);
  textureStore(w_linst, p, vec4f(factor, 0.0, 0.0, 0.0));
}

// ---- Accumulate (ffx_fsr3upscaler_accumulate/upsample/reproject/sample.h) ----

fn lanczos2(x_in: f32) -> f32 {
  let PI = 3.141592653589793;
  let x = min(abs(x_in), 2.0);
  if (x < EPSILON) { return 1.0; }
  return (sin(PI * x) / (PI * x)) * (sin(0.5 * PI * x) / (0.5 * PI * x));
}
// FSR1's Lanczos approximation: x2 is x*x, at most 4.
fn lanczos2_approx_sq(x2_in: f32) -> f32 {
  let x2 = min(x2_in, 4.0);
  let a = (2.0 / 5.0) * x2 - 1.0;
  let b = (1.0 / 4.0) * x2 - 1.0;
  return ((25.0 / 16.0) * a * a - (25.0 / 16.0 - 1.0)) * (b * b);
}
// The four 1D weights for a fraction t (taps at -1, 0, 1, 2), normalised.
// Every row of the 4x4 shares the horizontal ones, so they are worked out
// once per axis (AMD's Lanczos2(...) recomputes them per row; same result).
fn lanczos2_weights(t: f32) -> vec4f {
  let w = vec4f(lanczos2(-1.0 - t), lanczos2(-0.0 - t), lanczos2(1.0 - t), lanczos2(2.0 - t));
  return w / (w.x + w.y + w.z + w.w);
}
fn lanczos2_1d(c0: vec4f, c1: vec4f, c2: vec4f, c3: vec4f, w: vec4f) -> vec4f {
  return w.x * c0 + w.y * c1 + w.z * c2 + w.w * c3;
}
fn history_texel(p: vec2i, off: vec2i, size: vec2i) -> vec4f {
  // ClampCoord keeps one texel off the border
  let q = vec2i(max(1, min(p.x + off.x, size.x - 2)), max(1, min(p.y + off.y, size.y - 2)));
  return textureLoad(t_hist, q, 0);
}
// The reprojected history: a 4x4 Lanczos, held between its four middle texels.
fn history_sample(uv: vec2f, size: vec2i) -> vec4f {
  var ps = uv * vec2f(size) - 0.5;
  let f = fract(ps);
  ps = clamp(ps, vec2f(0.0), vec2f(size - 1));
  let p = vec2i(floor(ps));
  let wx = lanczos2_weights(f.x);
  let wy = lanczos2_weights(f.y);
  var rows: array<vec4f, 4>;
  var c11: vec4f;
  var c21: vec4f;
  var c12: vec4f;
  var c22: vec4f;
  for (var y = 0; y < 4; y++) {
    let c0 = history_texel(p, vec2i(-1, y - 1), size);
    let c1 = history_texel(p, vec2i(0, y - 1), size);
    let c2 = history_texel(p, vec2i(1, y - 1), size);
    let c3 = history_texel(p, vec2i(2, y - 1), size);
    rows[y] = lanczos2_1d(c0, c1, c2, c3, wx);
    if (y == 1) { c11 = c1; c21 = c2; }
    if (y == 2) { c12 = c1; c22 = c2; }
  }
  let c = lanczos2_1d(rows[0], rows[1], rows[2], rows[3], wy);
  let lo = min(min(c11, c21), min(c12, c22));
  let hi = max(max(c11, c21), max(c12, c22));
  return clamp(c, lo, hi);
}

struct Box {
  center: vec3f,
  vec: vec3f,
  aabb_min: vec3f,
  aabb_max: vec3f,
  weight: f32,
}

fn prepared_color(p: vec2i) -> vec3f {
  return rgb_to_ycocg(max(vec3f(0.0), textureLoad(t_color, p, 0).rgb) * exposure());
}

@compute @workgroup_size(8, 8, 1)
fn cs_accumulate(@builtin(global_invocation_id) gid: vec3u) {
  let hr = vec2i(gid.xy);
  if (!on_screen(hr, us())) { return; }

  // InitPassData
  let hr_uv = (vec2f(hr) + 0.5) / vec2f(us());
  let lr_uv_jittered = hr_uv + C.jitter / rsf();
  let lr_uv_hw = clamp_uv(lr_uv_jittered, rs(), C.max_render_size);
  let mv = load_dmv(vec2i(hr_uv * rsf()));
  let vel_4k = velocity_4k(mv);
  let reprojected_uv = hr_uv + mv;
  let existing = uv_inside(reprojected_uv);
  let luma_instability = textureSampleLevel(t_linst, s_lin, clamp_uv(hr_uv, rs(), C.max_render_size), 0.0).x;
  let farthest_m = textureSampleLevel(t_fmip1, s_lin, clamp_uv(lr_uv_jittered, rs() / 2, vec2i(textureDimensions(t_fmip1))), 0.0).x;
  let is_new = !existing || C.frame_index == 0.0;
  let masks = textureSampleLevel(t_masks, s_lin, lr_uv_hw, 0.0);
  let reactive = saturate(masks[REACTIVE]);
  let disocc = saturate(masks[DISOCCLUSION]);
  let shading = saturate(masks[SHADING_CHANGE]);
  var accumulation = saturate(masks[ACCUMULATION]);
  accumulation *= f32(round(accumulation * 100.0) > 1.0);

  var history_color = vec3f(0.0);
  var lock = 0.0;
  if (existing && !is_new) {
    // ReprojectHistoryColor
    let h = history_sample(reprojected_uv, C.prev_upscale_size);
    history_color = rgb_to_ycocg(h.rgb * C.delta_pre_exposure * exposure());
    lock = h.w;
  }

  // UpdateLockStatus
  lock *= f32(!is_new);
  let lifetime_decrease_factor = max(saturate(shading), max(reactive, disocc));
  lock = max(0.0, lock - lifetime_decrease_factor * LOCK_MAX);
  let lock_contribution = saturate(saturate(lock - LOCK_THRESHOLD) * (LOCK_MAX - LOCK_THRESHOLD));
  let new_lock = b_locks[lock_index(hr)] * (1.0 - max(shading * 0.0, reactive));
  lock = max(0.0, min(lock + new_lock, LOCK_MAX));
  let lifetime_decrease = (0.1 / C.jitter_seq_len) * (1.0 - lifetime_decrease_factor);
  lock = max(0.0, lock - lifetime_decrease);
  lock *= f32(uv_inside(hr_uv - mv));

  // ComputeBaseAccumulationWeight
  var base_acc = accumulation;
  base_acc = min(base_acc, mix(base_acc, 0.15, saturate(max(0.0, vel_4k * C.velocity_factor / 0.5))));
  var history_weight = base_acc;

  // ComputeUpsampledColorAndWeight
  let src_output_pos = (vec2f(hr) + 0.5) * C.downscale;
  let src_input_pos = vec2i(floor(src_output_pos));
  let src_unjittered = (vec2f(src_input_pos) + 0.5) - C.jitter;
  let base_offset = src_unjittered - src_output_pos;
  let flip_col = src_unjittered.x > src_output_pos.x;
  let flip_row = src_unjittered.y > src_output_pos.y;
  let offset_tl = vec2i(select(-1, -2, flip_col), select(-1, -2, flip_row));
  let initial = accumulation == 0.0;
  var samples: array<vec3f, 9>;
  for (var row = 0; row < 3; row++) {
    for (var col = 0; col < 3; col++) {
      let cr = vec2i(select(col, 3 - col, flip_col), select(row, 3 - row, flip_row));
      let sp = clamp(src_input_pos + offset_tl + cr, vec2i(0), rs() - 1);
      var s = prepared_color(sp);
      if (initial) { s = rgb_to_ycocg(tonemap(ycocg_to_rgb(s))); }
      samples[row * 3 + col] = s;
    }
  }
  let kernel_bias_max = min(1.99, 1.0 + (1.0 / C.downscale.x - 1.0));
  let kernel_bias_min = max(1.0, (1.0 + kernel_bias_max) * 0.3);
  let kernel_bias_weight = min(1.0 - disocc * 0.5, min(1.0 - shading, saturate(history_weight * 5.0)));
  let kernel_bias = mix(kernel_bias_min, kernel_bias_max, kernel_bias_weight);
  var up_color = vec3f(0.0);
  var up_weight = 0.0;
  var box: Box;
  for (var row = 0; row < 3; row++) {
    for (var col = 0; col < 3; col++) {
      let cr = vec2i(select(col, 3 - col, flip_col), select(row, 3 - row, flip_row));
      let off = base_offset + vec2f(offset_tl) + vec2f(cr);
      let sp = src_input_pos + offset_tl + cr;
      let on = f32(on_screen(sp, rs()));
      let s = samples[row * 3 + col];
      if (!initial) {
        let ob = off * kernel_bias;
        let w = on * lanczos2_approx_sq(dot(ob, ob));
        up_color += s * w;
        up_weight += w;
      }
      let bw = exp(-2.3 * dot(off, off)) * on;
      if (row == 0 && col == 0) {
        box.aabb_min = s;
        box.aabb_max = s;
        box.center = s * bw;
        box.vec = s * s * bw;
        box.weight = bw;
      } else {
        box.aabb_min = min(box.aabb_min, s);
        box.aabb_max = max(box.aabb_max, s);
        box.center += s * bw;
        box.vec += s * s * bw;
        box.weight += bw;
      }
    }
  }
  // RectificationBoxComputeVarianceBoxData
  box.weight = select(1.0, box.weight, abs(box.weight) > FP32_MIN);
  box.center /= box.weight;
  box.vec /= box.weight;
  box.vec = sqrt(abs(box.vec - box.center * box.center));
  up_weight *= f32(up_weight > EPSILON);
  if (up_weight > EPSILON) {
    up_color = up_color / up_weight;
    up_weight *= AVERAGE_LANCZOS_WEIGHT_PER_FRAME;
    up_color = clamp(up_color, box.aabb_min, box.aabb_max);
  }
  if (initial) {
    up_color = rgb_to_ycocg(inverse_tonemap(ycocg_to_rgb(box.center)));
    up_weight = 1.0;
    history_weight = 0.0;
  }

  // RectifyHistory
  {
    let velocity_factor = saturate(vel_4k / 20.0);
    let distance_factor = saturate(0.75 - farthest_m / 20.0);
    let accumulation_factor = 1.0 - accumulation;
    let reactive_factor = pow(reactive, 1.0 / 2.0);
    let box_t = max(velocity_factor, max(distance_factor, max(accumulation_factor, max(reactive_factor, shading))));
    let box_scale = mix(3.0, 1.0, box_t);
    let scaled = box.vec * vec3f(1.7, 1.0, 1.0) * box_scale;
    let clamped = max(scaled, vec3f(1.193e-7));
    let t = (history_color - box.center) / clamped;
    if (length(t) > 1.0) {
      let final_clamped = normalize(t) * scaled + box.center;
      let contribution = max(luma_instability, lock_contribution) * accumulation * (1.0 - disocc);
      history_color = mix(final_clamped, history_color, saturate(contribution));
    }
  }

  // Accumulate (HDR input)
  history_weight *= f32(history_weight > FP16_MIN);
  history_weight = max(EPSILON, history_weight + up_weight);
  let up_t = rgb_to_ycocg(tonemap(ycocg_to_rgb(up_color)));
  let hist_t = rgb_to_ycocg(tonemap(ycocg_to_rgb(history_color)));
  let alpha = saturate(up_weight / history_weight);
  var color = inverse_tonemap(ycocg_to_rgb(mix(hist_t, up_t, alpha)));

  color /= exposure();
  color = max(color, vec3f(0.0));
  textureStore(w_hist, hr, vec4f(color, lock));
  b_locks[lock_index(hr)] = 0.0;
}
