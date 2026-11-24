// Bloom chain, tonemap.
@group(1) @binding(0) var t_src: texture_2d<f32>;
@group(1) @binding(1) var t_bloom: texture_2d<f32>;
@group(1) @binding(2) var t_depth: texture_depth_2d;

@fragment fn fs_down(in: FsIn) -> @location(0) vec4f {
  let tx = 1.0 / vec2f(textureDimensions(t_src));
  var c = textureSampleLevel(t_src, s_clamp, in.uv, 0.0).rgb * 4.0;
  c += textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(-1.0, -1.0), 0.0).rgb;
  c += textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(1.0, -1.0), 0.0).rgb;
  c += textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(-1.0, 1.0), 0.0).rgb;
  c += textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(1.0, 1.0), 0.0).rgb;
  c = clamp(c / 8.0, vec3f(0.0), vec3f(3000.0));
  // Soft clamp so a single hot pixel cannot flood the screen.
  return vec4f(c / (1.0 + luma(c) * 0.002), 1.0);
}
@fragment fn fs_up(in: FsIn) -> @location(0) vec4f {
  let tx = 1.0 / vec2f(textureDimensions(t_src));
  var c = textureSampleLevel(t_src, s_clamp, in.uv, 0.0).rgb * 4.0;
  c += (textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(-1.0, 0.0), 0.0).rgb + textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(1.0, 0.0), 0.0).rgb
      + textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(0.0, -1.0), 0.0).rgb + textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(0.0, 1.0), 0.0).rgb) * 2.0;
  c += textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(-1.0, -1.0), 0.0).rgb + textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(1.0, -1.0), 0.0).rgb
     + textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(-1.0, 1.0), 0.0).rgb + textureSampleLevel(t_src, s_clamp, in.uv + tx * vec2f(1.0, 1.0), 0.0).rgb;
  return vec4f(c / 16.0, 1.0);
}

// AgX (Troy Sobotka's filmic transform): rolls bright colours off towards
// white the way film and camera sensors do instead of skewing their hue, and
// keeps a long, soft toe so shadows hold detail. Returns display-encoded RGB.
fn agx(x: vec3f) -> vec3f {
  let inset = mat3x3f(0.842479062253094, 0.0423282422610123, 0.0423756549057051,
                      0.0784335999999992, 0.878468636469772, 0.0784336,
                      0.0792237451477643, 0.0791661274605434, 0.879142973793104);
  let outset = mat3x3f(1.19687900512017, -0.0528968517574562, -0.0529716355144438,
                       -0.0980208811401368, 1.15190312990417, -0.0980434501171241,
                       -0.0990297440797205, -0.0989611768448433, 1.15107367264116);
  let lo = -12.47393;
  let hi = 4.026069;
  var v = inset * max(x, vec3f(1e-10));
  v = (clamp(log2(v), vec3f(lo), vec3f(hi)) - lo) / (hi - lo);
  let v2 = v * v;
  let v4 = v2 * v2;
  v = 15.5 * v4 * v2 - 40.14 * v4 * v + 31.96 * v4 - 6.868 * v2 * v + 0.4298 * v2 + 0.1191 * v - 0.00232;
  // A gentle 'punchy' look: a little more contrast and colour than plain AgX,
  // closer to what a camera's picture profile does. The S-curve pivots on
  // mid-grey, so it deepens shadows and lifts highlights without darkening
  // the midtones the way a plain power would.
  v = clamp(v, vec3f(0.0), vec3f(1.0));
  v = mix(v, v * v * (3.0 - 2.0 * v), AGX_CONTRAST);
  let l = dot(v, vec3f(0.2126, 0.7152, 0.0722));
  v = l + (v - l) * AGX_SAT;
  v = outset * v;
  return clamp(v, vec3f(0.0), vec3f(1.0));
}
const AGX_CONTRAST = 0.45;
const AGX_SAT = 1.2;
const AGX_EXPOSURE = 1.6;

// Bloom, exposure, vignette and tonemap of a scene colour.
fn finish(scene: vec3f, in: FsIn) -> vec4f {
  let uv = in.uv;
  var c = scene;
  let bloom = textureSampleLevel(t_bloom, s_clamp, uv, 0.0).rgb;
  // Lights bloom more at night, when the eye is dark-adapted (judged by the
  // exposure before it closes down for city lights, which bloom as much).
  // (The start screen's globe, against black space, only a touch: a bright
  // planet would otherwise wear a wide milky halo.)
  c = mix(c, bloom, mix(0.035, 0.06, smoothstep(1.0, 7.5, G.time.z * exp2(G.misc.y))) * mix(1.0, 0.3, globe_k()));
  c *= G.time.z;
  // Gentle vignette.
  let q = in.uv - 0.5;
  if (G.detail.w > -0.5) { c *= 1.0 - dot(q, q) * 0.35; }
  c = agx(c * AGX_EXPOSURE);
  c += (ign(in.pos.xy + 17.0) - 0.5) / 255.0;
  return vec4f(c, still_alpha(uv));
}
// Aircraft-picker stills (main.rs `--still`, G.detail.w < 0): opaque where
// the surface seen lies inside the aircraft's box (in its body frame; it is
// shot held in the air, so nothing else is near), transparent elsewhere.
fn still_alpha(uv: vec2f) -> f32 {
  if (G.detail.w > -0.5) { return 1.0; }
  let dim = vec2f(textureDimensions(t_depth));
  let d = textureLoad(t_depth, vec2i(clamp(uv * dim, vec2f(0.0), dim - 1.0)), 0);
  if (d <= 0.0) { return 0.0; }
  let b = (G.plane_inv * vec4f(view_ray(uv) / d, 1.0)).xyz - G.plane_box[0].xyz;
  let h = G.plane_box[1].xyz;
  return select(0.0, 1.0, all(abs(b) < h));
}
// The frame at a window pixel when it was drawn at another resolution
// (graphics settings): bilinear when upscaling; supersampled frames average
// four bilinear taps over the window pixel's footprint (a box filter).
fn scaled(uv: vec2f) -> vec4f {
  if (G.quality.y > 1.001) {
    let o = 0.25 * G.quality.zw;
    return 0.25 * (textureSampleLevel(t_src, s_clamp, uv + vec2f(-o.x, -o.y), 0.0) + textureSampleLevel(t_src, s_clamp, uv + vec2f(o.x, -o.y), 0.0)
                 + textureSampleLevel(t_src, s_clamp, uv + vec2f(-o.x, o.y), 0.0) + textureSampleLevel(t_src, s_clamp, uv + vec2f(o.x, o.y), 0.0));
  }
  return textureSampleLevel(t_src, s_clamp, uv, 0.0);
}
fn native() -> bool { return abs(G.quality.y - 1.0) < 0.001 || G.quality.y <= 0.0; }
@fragment fn fs_final(in: FsIn) -> @location(0) vec4f {
  return finish(scaled(in.uv).rgb, in);
}
// After temporal anti-aliasing (taa.wgsl), whose alpha says how much each
// pixel is moving: what moves has been softened by the blend, so sharpen it
// back, never past the brightest or darkest of its neighbours (no halos).
const TAA_SHARPEN = 0.8;
@fragment fn fs_final_taa(in: FsIn) -> @location(0) vec4f {
  if (!native()) {
    // The same sharpening at another resolution, over the larger of a frame
    // texel and a window pixel (so an upscaled frame gets its edges back).
    let d = max(1.0 / vec2f(textureDimensions(t_src)), G.quality.zw);
    let c = scaled(in.uv);
    let n = scaled(in.uv + vec2f(0.0, -d.y)).rgb;
    let s = scaled(in.uv + vec2f(0.0, d.y)).rgb;
    let w = scaled(in.uv + vec2f(-d.x, 0.0)).rgb;
    let e = scaled(in.uv + vec2f(d.x, 0.0)).rgb;
    let k = TAA_SHARPEN * max(c.a, select(0.0, 0.35, G.quality.y < 1.0));
    return finish(sharpen(c.rgb, n, s, w, e, k), in);
  }
  let px = vec2i(in.pos.xy);
  let dmax = vec2i(textureDimensions(t_src)) - 1;
  let c = textureLoad(t_src, px, 0);
  let n = textureLoad(t_src, clamp(px + vec2i(0, -1), vec2i(0), dmax), 0).rgb;
  let s = textureLoad(t_src, clamp(px + vec2i(0, 1), vec2i(0), dmax), 0).rgb;
  let w = textureLoad(t_src, clamp(px + vec2i(-1, 0), vec2i(0), dmax), 0).rgb;
  let e = textureLoad(t_src, clamp(px + vec2i(1, 0), vec2i(0), dmax), 0).rgb;
  return finish(sharpen(c.rgb, n, s, w, e, TAA_SHARPEN * c.a), in);
}
fn sharpen(c: vec3f, n: vec3f, s: vec3f, w: vec3f, e: vec3f, k: f32) -> vec3f {
  let lo = min(min(min(n, s), min(w, e)), c);
  let hi = max(max(max(n, s), max(w, e)), c);
  return clamp(c + (c - (n + s + w + e) * 0.25) * k, lo, hi);
}

// ---- Upscalers (graphics settings; render.rs `Upscale`) ----

// Motion vectors for the temporal upscalers: where each pixel's surface was
// on screen last frame, as a uv offset (previous minus current, without the
// sub-pixel jitter), found the way the temporal AA finds it (taa.wgsl): the
// sky only turns with the camera, the aircraft flies along with it.
@fragment fn fs_motion(in: FsIn) -> @location(0) vec4f {
  let px = vec2i(in.pos.xy);
  let uvn = (vec2f(px) + 0.5) * G.screen.zw;
  let z = textureLoad(t_depth, px, 0);
  let ray = view_ray(uvn);
  var pc: vec4f;
  if (z == 0.0) {
    pc = G.prev_vp * vec4f(ray, 0.0);
  } else {
    let P = ray / z;
    let b = (G.plane_inv * vec4f(P, 1.0)).xyz;
    if (G.taa.w > 0.5 && all(abs(b - G.plane_box[0].xyz) < G.plane_box[1].xyz)) {
      pc = G.plane_prev * vec4f(b, 1.0);
    } else {
      pc = G.prev_vp * vec4f(P + G.cam_delta.xyz, 1.0);
    }
  }
  if (pc.w <= 0.0) { return vec4f(0.0); }
  let prev = pc.xy / pc.w * vec2f(0.5, -0.5) + 0.5;
  return vec4f(prev - (uvn - G.taa.xy), 0.0, 0.0);
}

// A reversible squash into 0..1 (AMD's "Tonemap" in FSR) for the spatial
// upscaler and its sharpening, which are made for display-range colour: the
// scene is put at its display exposure first, as the tonemap would.
fn up_exposure() -> f32 { return max(G.time.z * AGX_EXPOSURE, 1e-6); }
fn squash3(c: vec3f) -> vec3f {
  let e = max(c, vec3f(0.0)) * up_exposure();
  return e / (max(e.r, max(e.g, e.b)) + 1.0);
}
fn unsquash3(s: vec3f) -> vec3f {
  return s / max(1.0 - max(s.r, max(s.g, s.b)), 6.1e-5) / up_exposure();
}

// AMD FidelityFX Super Resolution 1.0 EASU (edge-adaptive spatial upsampling),
// ported from ffx_fsr1.h in AMD's FSR SDK v2.4.0 (MIT; docs/upscaling.md). The
// twelve taps are loaded one by one (clamped to the frame) rather than
// gathered, so they can be squashed first; the maths is AMD's float path.
fn easu_tap(c: vec3f, w: f32, off: vec2f, dir: vec2f, len: vec2f, lob: f32, clp: f32, col: vec3f) -> vec4f {
  var v = vec2f(off.x * dir.x + off.y * dir.y, off.x * -dir.y + off.y * dir.x);
  v *= len;
  let d2 = min(v.x * v.x + v.y * v.y, clp);
  var wb = (2.0 / 5.0) * d2 - 1.0;
  var wa = lob * d2 - 1.0;
  wb *= wb;
  wa *= wa;
  wb = (25.0 / 16.0) * wb - (25.0 / 16.0 - 1.0);
  let wt = wb * wa;
  return vec4f(c + col * wt, w + wt);
}
// One of the four bilinear corners' share of the edge direction and length.
fn easu_set(acc: vec3f, pp: vec2f, corner: i32, lA: f32, lB: f32, lC: f32, lD: f32, lE: f32) -> vec3f {
  var w = 0.0;
  if (corner == 0) { w = (1.0 - pp.x) * (1.0 - pp.y); }
  if (corner == 1) { w = pp.x * (1.0 - pp.y); }
  if (corner == 2) { w = (1.0 - pp.x) * pp.y; }
  if (corner == 3) { w = pp.x * pp.y; }
  let dc = lD - lC;
  let cb = lC - lB;
  var lenx = 1.0 / max(abs(dc), abs(cb));
  let dirx = lD - lB;
  lenx = saturate(abs(dirx) * lenx);
  lenx *= lenx;
  let ec = lE - lC;
  let ca = lC - lA;
  var leny = 1.0 / max(abs(ec), abs(ca));
  let diry = lE - lA;
  leny = saturate(abs(diry) * leny);
  leny *= leny;
  return acc + vec3f(dirx * w, diry * w, (lenx + leny) * w);
}
fn easu_luma(c: vec3f) -> f32 { return c.b * 0.5 + (c.r * 0.5 + c.g); }
@fragment fn fs_easu(in: FsIn) -> @location(0) vec4f {
  let src = vec2f(textureDimensions(t_src));
  let dst = 1.0 / G.quality.zw;
  let dmax = vec2i(src) - 1;
  // con0: input over output, and the half-pixel shift
  var pp = floor(in.pos.xy) * (src / dst) + (0.5 * src / dst - 0.5);
  let fp = floor(pp);
  pp -= fp;
  let q = vec2i(fp);
  var t: array<vec3f, 12>;
  // b c / e f g h / i j k l / n o
  var offs = array<vec2i, 12>(vec2i(0, -1), vec2i(1, -1), vec2i(-1, 0), vec2i(0, 0), vec2i(1, 0), vec2i(2, 0), vec2i(-1, 1), vec2i(0, 1), vec2i(1, 1), vec2i(2, 1), vec2i(0, 2), vec2i(1, 2));
  for (var i = 0; i < 12; i++) {
    t[i] = squash3(textureLoad(t_src, clamp(q + offs[i], vec2i(0), dmax), 0).rgb);
  }
  let b = t[0]; let c = t[1]; let e = t[2]; let f = t[3]; let g = t[4]; let h = t[5];
  let i_ = t[6]; let j = t[7]; let k = t[8]; let l = t[9]; let n = t[10]; let o = t[11];
  let bL = easu_luma(b); let cL = easu_luma(c); let eL = easu_luma(e); let fL = easu_luma(f);
  let gL = easu_luma(g); let hL = easu_luma(h); let iL = easu_luma(i_); let jL = easu_luma(j);
  let kL = easu_luma(k); let lL = easu_luma(l); let nL = easu_luma(n); let oL = easu_luma(o);
  var dl = vec3f(0.0);
  dl = easu_set(dl, pp, 0, bL, eL, fL, gL, jL);
  dl = easu_set(dl, pp, 1, cL, fL, gL, hL, kL);
  dl = easu_set(dl, pp, 2, fL, iL, jL, kL, nL);
  dl = easu_set(dl, pp, 3, gL, jL, kL, lL, oL);
  var dir = dl.xy;
  var len = dl.z;
  let dir2 = dir * dir;
  var dirR = dir2.x + dir2.y;
  let zro = dirR < 1.0 / 32768.0;
  dirR = select(inverseSqrt(dirR), 1.0, zro);
  dir.x = select(dir.x, 1.0, zro);
  dir *= dirR;
  len = len * 0.5;
  len *= len;
  let stretch = (dir.x * dir.x + dir.y * dir.y) / max(abs(dir.x), abs(dir.y));
  let len2 = vec2f(1.0 + (stretch - 1.0) * len, 1.0 - 0.5 * len);
  let lob = 0.5 + ((1.0 / 4.0 - 0.04) - 0.5) * len;
  let clp = 1.0 / lob;
  let mn4 = min(min(f, g), min(j, k));
  let mx4 = max(max(f, g), max(j, k));
  var a = vec4f(0.0);
  a = easu_tap(a.rgb, a.w, vec2f(0.0, -1.0) - pp, dir, len2, lob, clp, b);
  a = easu_tap(a.rgb, a.w, vec2f(1.0, -1.0) - pp, dir, len2, lob, clp, c);
  a = easu_tap(a.rgb, a.w, vec2f(-1.0, 1.0) - pp, dir, len2, lob, clp, i_);
  a = easu_tap(a.rgb, a.w, vec2f(0.0, 1.0) - pp, dir, len2, lob, clp, j);
  a = easu_tap(a.rgb, a.w, vec2f(0.0, 0.0) - pp, dir, len2, lob, clp, f);
  a = easu_tap(a.rgb, a.w, vec2f(-1.0, 0.0) - pp, dir, len2, lob, clp, e);
  a = easu_tap(a.rgb, a.w, vec2f(1.0, 1.0) - pp, dir, len2, lob, clp, k);
  a = easu_tap(a.rgb, a.w, vec2f(2.0, 1.0) - pp, dir, len2, lob, clp, l);
  a = easu_tap(a.rgb, a.w, vec2f(2.0, 0.0) - pp, dir, len2, lob, clp, h);
  a = easu_tap(a.rgb, a.w, vec2f(1.0, 0.0) - pp, dir, len2, lob, clp, g);
  a = easu_tap(a.rgb, a.w, vec2f(1.0, 2.0) - pp, dir, len2, lob, clp, o);
  a = easu_tap(a.rgb, a.w, vec2f(0.0, 2.0) - pp, dir, len2, lob, clp, n);
  return vec4f(min(mx4, max(mn4, a.rgb / a.w)), 1.0);
}

// AMD FidelityFX RCAS (robust contrast-adaptive sharpening), ffx_fsr1.h's
// FsrRcasF, on five display-range taps. `sharp` is exp2(-stops); `denoise`
// (as FSR 3 uses it) eases off on noise.
const RCAS_LIMIT = 0.25 - 1.0 / 16.0;
fn rcas(b: vec3f, d: vec3f, e: vec3f, f: vec3f, h: vec3f, sharp: f32, denoise: bool) -> vec3f {
  let bL = easu_luma(b); let dL = easu_luma(d); let eL = easu_luma(e); let fL = easu_luma(f); let hL = easu_luma(h);
  var nz = 0.25 * bL + 0.25 * dL + 0.25 * fL + 0.25 * hL - eL;
  nz = saturate(abs(nz) / (max(max(max(bL, dL), eL), max(fL, hL)) - min(min(min(bL, dL), eL), min(fL, hL))));
  nz = -0.5 * nz + 1.0;
  let mn4 = min(min(b, d), min(f, h));
  let mx4 = max(max(b, d), max(f, h));
  let lower = saturate(eL / min(min(bL, dL), min(fL, hL)));
  let hit_min = mn4 / (4.0 * mx4) * lower;
  let hit_max = (1.0 - mx4) / (4.0 * mn4 - 4.0);
  let lobe3 = max(-hit_min, hit_max);
  var lobe = max(-RCAS_LIMIT, min(max(lobe3.r, max(lobe3.g, lobe3.b)), 0.0)) * sharp;
  if (denoise) { lobe *= nz; }
  let rcp = 1.0 / (4.0 * lobe + 1.0);
  return (lobe * (b + d + h + f) + e) * rcp;
}
// The five taps of the window-sized upscaled frame around this pixel.
fn up_taps(px: vec2i) -> array<vec3f, 5> {
  let dmax = vec2i(textureDimensions(t_src)) - 1;
  return array<vec3f, 5>(
    textureLoad(t_src, clamp(px + vec2i(0, -1), vec2i(0), dmax), 0).rgb,
    textureLoad(t_src, clamp(px + vec2i(-1, 0), vec2i(0), dmax), 0).rgb,
    textureLoad(t_src, clamp(px, vec2i(0), dmax), 0).rgb,
    textureLoad(t_src, clamp(px + vec2i(1, 0), vec2i(0), dmax), 0).rgb,
    textureLoad(t_src, clamp(px + vec2i(0, 1), vec2i(0), dmax), 0).rgb,
  );
}
// After FSR 1: the EASU frame is squashed already; sharpen, unsquash, tonemap.
const FSR1_SHARP = 0.25;
@fragment fn fs_final_fsr1(in: FsIn) -> @location(0) vec4f {
  let t = up_taps(vec2i(in.pos.xy));
  let s = rcas(t[0], t[1], t[2], t[3], t[4], exp2(-FSR1_SHARP), false);
  return finish(unsquash3(s), in);
}
// After FSR 3.1 (fsr3.wgsl): RCAS on the exposed colour, as AMD's RCAS pass
// does (sharpness 0.8 of 1, i.e. 0.4 stops), then the tonemap.
const FSR3_SHARP = 0.4;
@fragment fn fs_final_fsr3(in: FsIn) -> @location(0) vec4f {
  let t = up_taps(vec2i(in.pos.xy));
  let e = up_exposure();
  let s = rcas(t[0] * e, t[1] * e, t[2] * e, t[3] * e, t[4] * e, exp2(-FSR3_SHARP), true);
  return finish(max(s / e, vec3f(0.0)), in);
}

// Final copy to the window.
@fragment fn fs_blit(in: FsIn) -> @location(0) vec4f {
  return vec4f(textureSampleLevel(t_src, s_clamp, in.uv, 0.0).rgb, 1.0);
}
@fragment fn fs_blit_srgb(in: FsIn) -> @location(0) vec4f {
  return vec4f(pow(textureSampleLevel(t_src, s_clamp, in.uv, 0.0).rgb, vec3f(2.2)), 1.0);
}
