//! AMD FSR 3.1.6 temporal upscaling, run from our own WGSL port of AMD's
//! shaders (shaders/fsr3.wgsl; MIT, see docs/upscaling.md). This side does
//! what AMD's `ffx_fsr3upscaler.cpp` does for its shaders: it owns the
//! intermediate textures, fills the per-frame constants, clears what has to be
//! cleared on a reset and dispatches the passes in AMD's order.
//!
//! In: the frame as drawn at the render resolution (HDR colour whose alpha
//! marks rain and lightning, reversed infinite depth) and its motion vectors
//! (post.wgsl `fs_motion`). Out: the frame at the window's resolution, which
//! the tonemap sharpens with RCAS (post.wgsl `fs_final_fsr3`).

use crate::render::tex_res;

/// Per-frame constants, laid out as `Fsr3C` in fsr3.wgsl (AMD's cbFSR3Upscaler
/// plus the exposure the game supplies).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable, Default)]
struct Consts {
    render_size: [i32; 2],
    prev_render_size: [i32; 2],
    upscale_size: [i32; 2],
    prev_upscale_size: [i32; 2],
    max_render_size: [i32; 2],
    max_upscale_size: [i32; 2],
    device_to_view: [f32; 4],
    jitter: [f32; 2],
    prev_jitter: [f32; 2],
    mv_scale: [f32; 2],
    downscale: [f32; 2],
    mv_jitter_cancel: [f32; 2],
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
    _pad: [f32; 2],
}

/// AMD's jitter phase count for a render width upscaled to a display width
/// (`ffxFsr3UpscalerGetJitterPhaseCount`): 8 at native, more as the ratio grows.
pub fn jitter_phases(render_w: u32, display_w: u32) -> u32 {
    ((8.0 * (display_w as f32 / render_w.max(1) as f32).powi(2)) as u32).max(1)
}

/// The sub-pixel offset (pixels, -0.5..0.5) for a frame: AMD's Halton (2, 3)
/// sequence (`ffxFsr3UpscalerGetJitterOffset`).
pub fn jitter_offset(frame: u64, phases: u32) -> [f32; 2] {
    let halton = |mut i: u64, b: u64| {
        let (mut f, mut r) = (1.0f32, 0.0f32);
        while i > 0 {
            f /= b as f32;
            r += f * (i % b) as f32;
            i /= b;
        }
        r
    };
    let k = frame % phases.max(1) as u64 + 1;
    [halton(k, 2) - 0.5, halton(k, 3) - 0.5]
}

/// What the renderer hands over each frame (the textures were bound in `resize`).
pub struct Frame {
    /// sub-pixel offset this frame (render pixels, AMD's sign convention)
    pub jitter: [f32; 2],
    pub near: f32,
    pub exposure: f32,
    pub dt: f32,
    /// throw the history away (first frame, a camera jump)
    pub reset: bool,
}

struct Targets {
    render: (u32, u32),
    display: (u32, u32),
    accum: [wgpu::TextureView; 2],
    recon: wgpu::Buffer,
    // bind groups, by pass, for each parity (which of the ping-pong pairs is written)
    bg: [[wgpu::BindGroup; 2]; 8],
    bg_down: [wgpu::BindGroup; 2],
    spd_size: (u32, u32),
    fmip_size: (u32, u32),
    /// the tonemap's bind group (post layout) reading each `hist`
    pub bg_final: [wgpu::BindGroup; 2],
}

pub struct Fsr3 {
    pipes: [wgpu::ComputePipeline; 8],
    ub: wgpu::Buffer,
    sampler: wgpu::Sampler,
    t: Option<Targets>,
    c: Consts,
    parity: usize,
    first: bool,
    frame: f32,
}

const PREPARE: usize = 0;
const FARTHEST: usize = 1;
const SHADING_PYRAMID: usize = 2;
const DOWNSAMPLE: usize = 3;
const SHADING_CHANGE: usize = 4;
const REACTIVITY: usize = 5;
const INSTABILITY: usize = 6;
const ACCUMULATE: usize = 7;
const ENTRIES: [&str; 8] = ["cs_prepare_inputs", "cs_farthest_mip1", "cs_shading_pyramid", "cs_downsample", "cs_shading_change", "cs_prepare_reactivity", "cs_luma_instability", "cs_accumulate"];

fn buf_entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource: buffer.as_entire_binding() }
}

/// A pass's bind group: the constants (and the sampler, if it samples) then its own resources.
fn make_group(d: &wgpu::Device, pipe: &wgpu::ComputePipeline, label: &str, ub: &wgpu::Buffer, sampler: &wgpu::Sampler, own: Vec<wgpu::BindGroupEntry<'_>>, sampled: bool) -> wgpu::BindGroup {
    // (the downsample is the one pass that reads no constants)
    let mut e = if label == ENTRIES[DOWNSAMPLE] { vec![] } else { vec![wgpu::BindGroupEntry { binding: 0, resource: ub.as_entire_binding() }] };
    if sampled {
        e.push(wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) });
    }
    e.extend(own);
    d.create_bind_group(&wgpu::BindGroupDescriptor { label: Some(label), layout: &pipe.get_bind_group_layout(0), entries: &e })
}

impl Fsr3 {
    pub fn new(d: &wgpu::Device) -> Self {
        let m = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("fsr3"), source: wgpu::ShaderSource::Wgsl(include_str!("shaders/fsr3.wgsl").into()) });
        let pipes = ENTRIES.map(|e| d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(e), layout: None, module: &m, entry_point: Some(e), compilation_options: Default::default(), cache: None }));
        let ub = d.create_buffer(&wgpu::BufferDescriptor { label: Some("fsr3 constants"), size: std::mem::size_of::<Consts>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fsr3 linear clamp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Fsr3 { pipes, ub, sampler, t: None, c: Consts::default(), parity: 0, first: true, frame: 0.0 }
    }

    /// (Re)makes the intermediate textures for these sizes and inputs; called
    /// whenever the renderer's own targets are remade.
    #[allow(clippy::too_many_arguments)]
    pub fn resize(&mut self, d: &wgpu::Device, render: (u32, u32), display: (u32, u32), color: &wgpu::TextureView, depth: &wgpu::TextureView, motion: &wgpu::TextureView, l_post: &wgpu::BindGroupLayout, bloom: &wgpu::TextureView) {
        let (rw, rh) = (render.0.max(1), render.1.max(1));
        let (dw, dh) = (display.0.max(1), display.1.max(1));
        let st = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        let tex = |label: &str, w: u32, h: u32, mips: u32, f: wgpu::TextureFormat, extra: wgpu::TextureUsages| {
            d.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: f,
                usage: st | extra,
                view_formats: &[],
            })
        };
        let v = |t: &wgpu::Texture| t.create_view(&Default::default());
        use wgpu::TextureFormat as F;
        let none = wgpu::TextureUsages::empty();
        let dmv = v(&tex("fsr3 dilated motion", rw, rh, 1, F::Rg32Float, none));
        let ddepth = v(&tex("fsr3 dilated depth", rw, rh, 1, F::R32Float, none));
        let farthest = v(&tex("fsr3 farthest depth", rw, rh, 1, F::R32Float, none));
        let luma = [0, 1].map(|i| v(&tex(&format!("fsr3 luma {i}"), rw, rh, 1, F::Rgba16Float, none)));
        let fmip_size = ((rw / 2).max(1), (rh / 2).max(1));
        let fmip1 = v(&tex("fsr3 farthest depth mip1", fmip_size.0, fmip_size.1, 1, F::Rgba16Float, none));
        let spd_size = fmip_size;
        let spd_tex = tex("fsr3 shading change pyramid", spd_size.0, spd_size.1, 3, F::Rgba16Float, none);
        let spd_mip = |m: u32| spd_tex.create_view(&wgpu::TextureViewDescriptor { base_mip_level: m, mip_level_count: Some(1), ..Default::default() });
        let spd_mips = [spd_mip(0), spd_mip(1), spd_mip(2)];
        let spd_all = v(&spd_tex);
        let shc = v(&tex("fsr3 shading change", spd_size.0, spd_size.1, 1, F::Rgba8Unorm, none));
        let accum = [0, 1].map(|i| v(&tex(&format!("fsr3 accumulation {i}"), rw, rh, 1, F::Rgba8Unorm, wgpu::TextureUsages::RENDER_ATTACHMENT)));
        let masks = v(&tex("fsr3 dilated reactive masks", rw, rh, 1, F::Rgba8Unorm, none));
        let lhist = [0, 1].map(|i| v(&tex(&format!("fsr3 luma history {i}"), rw, rh, 1, F::Rgba16Float, none)));
        let linst = v(&tex("fsr3 luma instability", rw, rh, 1, F::Rgba16Float, none));
        let hist = [0, 1].map(|i| v(&tex(&format!("fsr3 upscaled {i}"), dw, dh, 1, F::Rgba16Float, none)));
        let recon = d.create_buffer(&wgpu::BufferDescriptor { label: Some("fsr3 reconstructed previous depth"), size: (rw * rh * 4) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let locks = d.create_buffer(&wgpu::BufferDescriptor { label: Some("fsr3 new locks"), size: (dw * dh * 4) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

        let (pipes, ub, sampler) = (&self.pipes, &self.ub, &self.sampler);
        let group = |pass: usize, e: Vec<wgpu::BindGroupEntry<'_>>, sampled: bool| make_group(d, &pipes[pass], ENTRIES[pass], ub, sampler, e, sampled);
        let buf = buf_entry;
        // p: the parity written this frame (the other one is last frame's)
        let make = |p: usize| -> [wgpu::BindGroup; 8] {
            let q = 1 - p;
            [
                group(PREPARE, vec![tex_res(2, depth), tex_res(3, motion), tex_res(4, color), tex_res(5, &dmv), tex_res(6, &ddepth), buf(7, &recon), tex_res(8, &farthest), tex_res(9, &luma[p])], false),
                group(FARTHEST, vec![tex_res(10, &farthest), tex_res(11, &fmip1)], false),
                group(SHADING_PYRAMID, vec![tex_res(12, &luma[p]), tex_res(13, &luma[q]), tex_res(14, &dmv), tex_res(15, &spd_mips[0])], false),
                group(DOWNSAMPLE, vec![tex_res(16, &spd_mips[0]), tex_res(15, &spd_mips[1])], false),
                group(SHADING_CHANGE, vec![tex_res(18, &spd_all), tex_res(19, &shc)], true),
                group(REACTIVITY, vec![tex_res(4, color), tex_res(12, &luma[p]), tex_res(14, &dmv), tex_res(20, &ddepth), buf(21, &recon), tex_res(22, &accum[q]), tex_res(23, &shc), tex_res(24, &masks), buf(25, &locks), tex_res(26, &accum[p])], true),
                group(INSTABILITY, vec![tex_res(12, &luma[p]), tex_res(14, &dmv), tex_res(27, &masks), tex_res(28, &lhist[q]), tex_res(30, &lhist[p]), tex_res(31, &linst)], true),
                group(ACCUMULATE, vec![tex_res(4, color), tex_res(14, &dmv), buf(25, &locks), tex_res(27, &masks), tex_res(29, &fmip1), tex_res(32, &hist[q]), tex_res(33, &linst), tex_res(34, &hist[p])], true),
            ]
        };
        let bg = {
            let (a, b) = (make(0), make(1));
            let mut a = a.into_iter();
            let mut b = b.into_iter();
            std::array::from_fn(|_| [a.next().unwrap(), b.next().unwrap()])
        };
        // the second level of the pyramid from the first, the third from the second
        let bg_down = [group(DOWNSAMPLE, vec![tex_res(16, &spd_mips[0]), tex_res(15, &spd_mips[1])], false), group(DOWNSAMPLE, vec![tex_res(16, &spd_mips[1]), tex_res(15, &spd_mips[2])], false)];
        let bg_final = [0, 1].map(|i| d.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("fsr3 final"), layout: l_post, entries: &[tex_res(0, &hist[i]), tex_res(1, bloom), tex_res(2, depth)] }));
        self.t = Some(Targets { render: (rw, rh), display: (dw, dh), accum, recon, bg, bg_down, spd_size, fmip_size, bg_final });
        self.first = true;
    }

    /// Runs the upscaler on this frame; returns the parity written, whose
    /// `bg_final` the tonemap reads.
    pub fn dispatch(&mut self, q: &wgpu::Queue, enc: &mut wgpu::CommandEncoder, f: &Frame, mark: &mut dyn FnMut(&mut wgpu::CommandEncoder, &'static str)) -> Option<usize> {
        let t = self.t.as_ref()?;
        let reset = f.reset || self.first;
        self.first = false;
        self.parity ^= 1;
        let p = self.parity;
        let (rw, rh) = t.render;
        let (dw, dh) = t.display;
        let c = &mut self.c;
        // previous-frame values (on a reset, this frame's)
        c.prev_jitter = if reset { f.jitter } else { c.jitter };
        c.jitter = f.jitter;
        c.prev_render_size = if reset { [rw as i32, rh as i32] } else { c.render_size };
        c.render_size = [rw as i32, rh as i32];
        c.max_render_size = c.render_size;
        c.prev_upscale_size = if reset { [dw as i32, dh as i32] } else { c.upscale_size };
        c.upscale_size = [dw as i32, dh as i32];
        c.max_upscale_size = c.upscale_size;
        // Reversed, infinite depth: view depth = near / (device depth + eps).
        // (x and y factors are only for view positions, which no pass uses.)
        c.device_to_view = [-f32::EPSILON, f.near, 0.0, 0.0];
        c.mv_scale = [1.0, 1.0];
        c.downscale = [rw as f32 / dw as f32, rh as f32 / dh as f32];
        c.mv_jitter_cancel = [0.0, 0.0];
        // AMD eases the phase count towards a new ratio one step a frame.
        let phases = jitter_phases(rw, dw) as f32;
        c.jitter_seq_len = if reset || c.jitter_seq_len == 0.0 { phases } else { c.jitter_seq_len + (phases - c.jitter_seq_len).signum() };
        c.delta_time = f.dt.clamp(0.0, 1.0);
        c.delta_pre_exposure = 1.0;
        c.view_to_meters = 1.0;
        self.frame = if reset { 0.0 } else { self.frame + 1.0 };
        c.frame_index = self.frame;
        c.velocity_factor = 1.0;
        c.reactiveness_scale = 1.0;
        c.shading_change_scale = 1.0;
        c.accumulation_added_per_frame = 1.0 / 3.0;
        c.min_disocclusion_accumulation = -1.0 / 3.0;
        c.exposure = f.exposure;
        q.write_buffer(&self.ub, 0, bytemuck::bytes_of(c));

        // Clears: the reconstructed depth every frame (to the farthest, 0 when
        // reversed); the accumulation read this frame on a reset.
        enc.clear_buffer(&t.recon, 0, None);
        if reset {
            for a in &t.accum {
                let _ = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("fsr3 clear accumulation"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: a, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
                    ..Default::default()
                });
            }
        }
        let groups = |w: u32, h: u32| (w.div_ceil(8), h.div_ceil(8));
        // One compute pass per AMD pass, so the GPU timer (FABLE_GPU_TIMES=1)
        // can tell them apart.
        let mut run = |enc: &mut wgpu::CommandEncoder, pass: usize, bg: &wgpu::BindGroup, (x, y): (u32, u32), label: &'static str| {
            {
                let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some(ENTRIES[pass]), timestamp_writes: None });
                cp.set_pipeline(&self.pipes[pass]);
                cp.set_bind_group(0, bg, &[]);
                cp.dispatch_workgroups(x, y, 1);
            }
            mark(enc, label);
        };
        run(enc, PREPARE, &t.bg[PREPARE][p], groups(rw, rh), "fsr3 prepare");
        run(enc, FARTHEST, &t.bg[FARTHEST][p], groups(t.fmip_size.0, t.fmip_size.1), "fsr3 farthest");
        run(enc, SHADING_PYRAMID, &t.bg[SHADING_PYRAMID][p], groups(t.spd_size.0, t.spd_size.1), "fsr3 shading pyramid");
        run(enc, DOWNSAMPLE, &t.bg_down[0], groups((t.spd_size.0 / 2).max(1), (t.spd_size.1 / 2).max(1)), "fsr3 downsample");
        run(enc, DOWNSAMPLE, &t.bg_down[1], groups((t.spd_size.0 / 4).max(1), (t.spd_size.1 / 4).max(1)), "fsr3 downsample");
        run(enc, SHADING_CHANGE, &t.bg[SHADING_CHANGE][p], groups(t.spd_size.0, t.spd_size.1), "fsr3 shading change");
        run(enc, REACTIVITY, &t.bg[REACTIVITY][p], groups(rw, rh), "fsr3 reactivity");
        run(enc, INSTABILITY, &t.bg[INSTABILITY][p], groups(rw, rh), "fsr3 luma instability");
        run(enc, ACCUMULATE, &t.bg[ACCUMULATE][p], groups(dw, dh), "fsr3 accumulate");
        Some(p)
    }

    /// The tonemap's bind group reading the frame written with this parity.
    pub fn bg_final(&self, parity: usize) -> Option<&wgpu::BindGroup> {
        self.t.as_ref().map(|t| &t.bg_final[parity & 1])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_match_the_shader_layout() {
        // Fsr3C in fsr3.wgsl: 6 ivec2, a vec4, 5 vec2, 12 floats and 2 of padding.
        assert_eq!(std::mem::size_of::<Consts>(), 160);
    }

    #[test]
    fn jitter_follows_amd() {
        // 8 phases at native, 32 at half resolution, 18 at 2/3.
        assert_eq!(jitter_phases(1920, 1920), 8);
        assert_eq!(jitter_phases(960, 1920), 32);
        assert_eq!(jitter_phases(1280, 1920), 18);
        // Halton(2, 3) from index 1, centred.
        assert_eq!(jitter_offset(0, 8), [0.0, 1.0 / 3.0 - 0.5]);
        assert_eq!(jitter_offset(1, 8), [0.25 - 0.5, 2.0 / 3.0 - 0.5]);
        assert_eq!(jitter_offset(8, 8), jitter_offset(0, 8));
        for i in 0..64 {
            let [x, y] = jitter_offset(i, 32);
            assert!(x.abs() <= 0.5 && y.abs() <= 0.5);
        }
    }
}
