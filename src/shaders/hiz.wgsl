// 8x8 reverse-Z max-depth pyramid for L1-cached coarse ray rejection in SSR.
@group(0) @binding(0) var t_depth: texture_depth_2d;
@group(0) @binding(1) var t_hiz: texture_storage_2d<r32float, write>;

var<workgroup> sm: array<f32, 64>;

@compute @workgroup_size(8, 8, 1)
fn main(
  @builtin(global_invocation_id) gid: vec3u,
  @builtin(workgroup_id) wid: vec3u,
  @builtin(local_invocation_index) li: u32,
) {
  let dims = textureDimensions(t_depth);
  let p = min(gid.xy, dims - vec2u(1u));
  sm[li] = textureLoad(t_depth, vec2i(p), 0);
  workgroupBarrier();
  if (li < 32u) { sm[li] = max(sm[li], sm[li + 32u]); }
  workgroupBarrier();
  if (li < 16u) { sm[li] = max(sm[li], sm[li + 16u]); }
  workgroupBarrier();
  if (li < 8u) { sm[li] = max(sm[li], sm[li + 8u]); }
  workgroupBarrier();
  if (li < 4u) { sm[li] = max(sm[li], sm[li + 4u]); }
  workgroupBarrier();
  if (li < 2u) { sm[li] = max(sm[li], sm[li + 2u]); }
  workgroupBarrier();
  if (li == 0u) {
    let hdims = textureDimensions(t_hiz);
    if (wid.x < hdims.x && wid.y < hdims.y) {
      textureStore(t_hiz, wid.xy, vec4f(max(sm[0], sm[1]), 0.0, 0.0, 0.0));
    }
  }
}
