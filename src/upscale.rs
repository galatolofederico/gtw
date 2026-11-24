//! The renderer's side of upscaling (graphics settings, graphics.rs
//! `Upscaler`): the frame drawn at the render resolution is brought up to the
//! window's by one of
//!
//! - **Bilinear**: the tonemap reads the frame with a bilinear filter, after
//!   the game's own temporal AA (what the game always did; the cheapest);
//! - **FSR 1**: AMD's spatial upscaler, EASU then RCAS sharpening, after the
//!   game's temporal AA (post.wgsl `fs_easu`, `fs_final_fsr1`);
//! - **FSR 3.1**: AMD's temporal upscaler in place of the game's temporal AA,
//!   from our port of AMD's shaders (fsr3.rs), then RCAS (`fs_final_fsr3`);
//! - **FSR 4.1**: AMD's machine-learning upscaler, from AMD's own runtime
//!   downloaded on request (fsr4.rs); FSR 3.1 stands in where it cannot run.
//!
//! The upscalers only apply below 100% render resolution, except FSR 3.1/4.1,
//! which also anti-alias at 100% (AMD's "Native AA"). Above 100% the frame is
//! supersampled and the tonemap box-filters it down as before.

use crate::graphics::Upscaler;
use crate::render::{make_tex, tex_res, view};

pub struct Upscale {
    pub p_motion: wgpu::RenderPipeline,
    pub p_easu: wgpu::RenderPipeline,
    pub p_final_fsr1: wgpu::RenderPipeline,
    pub p_final_fsr3: wgpu::RenderPipeline,
    /// the mode asked for (graphics settings)
    want: Upscaler,
    /// render resolution as a share of the window's
    scale: f32,
    pub fsr3: Option<crate::fsr3::Fsr3>,
    /// AMD's FSR 4 runtime and what it reads and writes, while it runs
    pub fsr4: Option<Fsr4>,
    /// why the FSR 4 runtime is not running, once it was tried
    fsr4_err: Option<String>,
    /// whether the device was opened with FSR 4's extensions (render.rs)
    pub fsr4_device: bool,
    /// puts the images where AMD's runtime expects them (`TOUCH`)
    pub p_touch: wgpu::ComputePipeline,
    /// something went wrong mid-frame: rebuild after it
    pub stale: bool,
    pub t: Option<Targets>,
    /// whether last frame's upscaled history can be built on
    pub hist_ok: bool,
}

pub struct Targets {
    /// motion vectors (render resolution)
    pub motion: wgpu::TextureView,
    pub motion_tex: wgpu::Texture,
    /// reads the depth for `fs_motion` (post layout)
    pub bg_motion: wgpu::BindGroup,
    /// FSR 1's upscaled, squashed frame (window resolution)
    pub up: wgpu::TextureView,
    /// EASU reading the frame from `hdr` or either temporal AA texture (render.rs `bg_final` order)
    pub bg_easu: [wgpu::BindGroup; 3],
    pub bg_final_fsr1: wgpu::BindGroup,
}

/// The views a rebuild needs from the renderer's own targets.
pub struct Inputs<'a> {
    pub render: (u32, u32),
    pub display: (u32, u32),
    pub hdr: &'a wgpu::TextureView,
    pub taa: [&'a wgpu::TextureView; 2],
    pub depth: &'a wgpu::TextureView,
    pub bloom: &'a wgpu::TextureView,
    pub black: &'a wgpu::TextureView,
    pub l_post: &'a wgpu::BindGroupLayout,
}

impl Upscale {
    pub fn new(d: &wgpu::Device, layout: &wgpu::PipelineLayout, m_post: &wgpu::ShaderModule, out: wgpu::TextureFormat) -> Self {
        let full = |label: &str, fs: &str, format: wgpu::TextureFormat| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState { module: m_post, entry_point: Some("vs_full"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: m_post, entry_point: Some(fs), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
                multiview_mask: None,
                cache: None,
            })
        };
        let touch = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("fsr4 touch"), source: wgpu::ShaderSource::Wgsl(TOUCH.into()) });
        let p_touch = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("fsr4 touch"), layout: None, module: &touch, entry_point: Some("cs_touch"), compilation_options: Default::default(), cache: None });
        Upscale {
            fsr4: None,
            fsr4_err: None,
            fsr4_device: false,
            p_touch,
            stale: false,
            p_motion: full("motion vectors", "fs_motion", MOTION),
            p_easu: full("fsr1 easu", "fs_easu", crate::render::HDR),
            p_final_fsr1: full("final fsr1", "fs_final_fsr1", out),
            p_final_fsr3: full("final fsr3", "fs_final_fsr3", out),
            want: Upscaler::Bilinear,
            scale: 1.0,
            fsr3: None,
            t: None,
            hist_ok: false,
        }
    }

    /// The upscaler in use: the one asked for, where it applies at this
    /// render resolution, FSR 3.1 standing in for FSR 4.1 unless AMD's
    /// runtime is running.
    pub fn active(&self) -> Upscaler {
        let a = active(self.want, self.scale);
        if a == Upscaler::Fsr3 && self.want == Upscaler::Fsr4 && self.fsr4.is_some() { Upscaler::Fsr4 } else { a }
    }

    /// A new mode or render resolution; true if the targets need a rebuild.
    pub fn set(&mut self, want: Upscaler, scale: f32) -> bool {
        let before = (self.want, self.active());
        self.want = want;
        self.scale = scale;
        (self.want, self.active()) != before
    }

    /// The provider AMD's runtime picked (FSR 4.1.2, or AMD's FSR 3.1.6).
    pub fn fsr4_provider(&self) -> Option<&str> {
        self.fsr4.as_ref().map(|f| f.rt.provider.as_str())
    }

    /// AMD's runtime failed mid-frame: fall back to FSR 3.1 from the next frame.
    pub fn fsr4_failed(&mut self, e: String) {
        log::warn!("AMD FSR runtime: {e}; using FSR 3.1");
        self.fsr4_err = Some(e);
        self.stale = true;
    }

    /// Why AMD's FSR 4 runtime is not running, when FSR 4.1 is asked for.
    pub fn fsr4_error(&self) -> Option<&str> {
        if self.want != Upscaler::Fsr4 || self.fsr4.is_some() {
            return None;
        }
        if let Some(e) = &self.fsr4_err {
            return Some(e);
        }
        (crate::fsr4::state() == crate::fsr4::State::Installed && !self.fsr4_device).then_some("restart the game to set the GPU up for it")
    }

    /// A new render resolution; true if the upscaler in use changes with it.
    pub fn rescale(&mut self, scale: f32) -> bool {
        self.set(self.want, scale)
    }

    /// Makes what the mode in use needs, drops what it does not.
    pub fn rebuild(&mut self, d: &wgpu::Device, i: &Inputs) {
        self.hist_ok = false;
        self.stale = false;
        // AMD's runtime reads our textures behind wgpu's back: let the GPU
        // finish with them before any are replaced.
        if self.fsr4.is_some() {
            let _ = d.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        }
        let try4 = self.want == Upscaler::Fsr4 && active(self.want, self.scale).temporal() && self.fsr4_device && self.fsr4_err.is_none() && crate::fsr4::blocked_os().is_none();
        if !try4 {
            self.fsr4 = None;
        } else if self.fsr4.as_ref().is_none_or(|f| f.rt.render != i.render || f.rt.display != i.display) {
            self.fsr4 = None;
            match crate::fsr4::rt::Runtime::new(d, i.render, i.display) {
                Ok(rt) => {
                    let mk = |label: &str, (w, h): (u32, u32), format: wgpu::TextureFormat, usage: wgpu::TextureUsages| d.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] });
                    let exposure = mk("fsr4 exposure", (1, 1), wgpu::TextureFormat::Rg32Float, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
                    let output = mk("fsr4 upscaled", i.display, crate::render::HDR, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING);
                    self.fsr4 = Some(Fsr4 { rt, bg_final: None, bg_touch: None, exposure, output });
                }
                Err(e) => {
                    log::warn!("AMD FSR runtime: {e}; using FSR 3.1");
                    self.fsr4_err = Some(e);
                }
            }
        }
        let mode = self.active();
        if mode == Upscaler::Bilinear {
            self.t = None;
            self.fsr3 = None;
            return;
        }
        let rt = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let (rw, rh) = i.render;
        let (dw, dh) = i.display;
        let motion_tex = make_tex(d, "motion vectors", rw, rh, 1, 1, MOTION, rt);
        let motion = view(&motion_tex);
        // FSR 1's output only exists while FSR 1 is in use (a 1x1 stand-in otherwise).
        let (uw, uh) = if mode == Upscaler::Fsr1 { (dw, dh) } else { (1, 1) };
        let up = view(&make_tex(d, "fsr1 upscaled", uw, uh, 1, 1, crate::render::HDR, rt));
        let post = |src: &wgpu::TextureView, bl: &wgpu::TextureView| d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: i.l_post, entries: &[tex_res(0, src), tex_res(1, bl), tex_res(2, i.depth)] });
        let t = Targets {
            bg_motion: post(i.hdr, i.black),
            bg_easu: [i.hdr, i.taa[0], i.taa[1]].map(|src| post(src, i.black)),
            bg_final_fsr1: post(&up, i.bloom),
            motion,
            motion_tex,
            up,
        };
        if let Some(f) = self.fsr4.as_mut() {
            let out = view(&f.output);
            f.bg_final = Some(post(&out, i.bloom));
            f.bg_touch = Some(d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fsr4 touch"),
                layout: &self.p_touch.get_bind_group_layout(0),
                entries: &[tex_res(0, i.hdr), tex_res(1, i.depth), tex_res(2, &t.motion), tex_res(3, &view(&f.exposure)), tex_res(4, &out)],
            }));
        }
        if mode == Upscaler::Fsr3 {
            let f = self.fsr3.get_or_insert_with(|| crate::fsr3::Fsr3::new(d));
            f.resize(d, i.render, i.display, i.hdr, i.depth, &t.motion, i.l_post, i.bloom);
        } else {
            self.fsr3 = None;
        }
        self.t = Some(t);
    }
}

/// AMD's FSR 4 runtime and its own textures.
pub struct Fsr4 {
    pub rt: crate::fsr4::rt::Runtime,
    /// 1x1: the exposure (the game supplies it; AMD's auto exposure is off)
    pub exposure: wgpu::Texture,
    /// the upscaled frame (window resolution), which the tonemap reads
    pub output: wgpu::Texture,
    pub bg_final: Option<wgpu::BindGroup>,
    pub bg_touch: Option<wgpu::BindGroup>,
}

impl Fsr4 {
    /// Before the runtime records: this frame's exposure, and a dispatch that
    /// uses every image the way AMD's runtime is told they are (sampled, the
    /// output as storage), so wgpu leaves them in those layouts.
    pub fn prepare(&self, q: &wgpu::Queue, enc: &mut wgpu::CommandEncoder, p_touch: &wgpu::ComputePipeline, exposure: f32) {
        q.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.exposure, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            bytemuck::cast_slice(&[exposure, 0.0f32]),
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(8), rows_per_image: Some(1) },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        if let Some(bg) = &self.bg_touch {
            let mut cp = enc.begin_compute_pass(&wgpu::ComputePassDescriptor { label: Some("fsr4 touch"), timestamp_writes: None });
            cp.set_pipeline(p_touch);
            cp.set_bind_group(0, bg, &[]);
            cp.dispatch_workgroups(1, 1, 1);
        }
    }
}

/// Reads every image AMD's runtime is handed (and could write the output),
/// so wgpu's tracking puts them in the layouts the runtime is told about.
const TOUCH: &str = r#"
@group(0) @binding(0) var t_color: texture_2d<f32>;
@group(0) @binding(1) var t_depth: texture_depth_2d;
@group(0) @binding(2) var t_motion: texture_2d<f32>;
@group(0) @binding(3) var t_exposure: texture_2d<f32>;
@group(0) @binding(4) var w_out: texture_storage_2d<rgba16float, write>;
@compute @workgroup_size(1)
fn cs_touch() {
  let a = textureLoad(t_color, vec2i(0), 0) + textureLoad(t_motion, vec2i(0), 0) + textureLoad(t_exposure, vec2i(0), 0) + vec4f(textureLoad(t_depth, vec2i(0), 0));
  // never true: the runtime writes the whole output after this
  if (a.x == -1234567.0) { textureStore(w_out, vec2i(0), a); }
}
"#;

/// Motion vectors: uv offsets, small, signed.
pub const MOTION: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;

/// The upscaler that runs for a mode asked for at a render resolution.
pub fn active(want: Upscaler, scale: f32) -> Upscaler {
    // (FSR 4.1 resolves to FSR 3.1 here; `Upscale::active` promotes it while AMD's runtime runs)
    let want = if want == Upscaler::Fsr4 { Upscaler::Fsr3 } else { want };
    if scale > 1.001 {
        // supersampled: the tonemap's box filter
        Upscaler::Bilinear
    } else if want == Upscaler::Fsr1 && scale > 0.999 {
        // nothing to upscale
        Upscaler::Bilinear
    } else {
        want
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure_matches_the_tonemap() {
        let post = include_str!("shaders/post.wgsl");
        assert!(post.contains(&format!("const AGX_EXPOSURE = {:?};", crate::render::AGX_EXPOSURE)), "render::AGX_EXPOSURE must match post.wgsl");
    }

    #[test]
    fn touch_shader_validates() {
        let m = naga::front::wgsl::parse_str(TOUCH).expect("parses");
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&m).expect("validates");
    }

    #[test]
    fn which_upscaler_runs() {
        assert_eq!(active(Upscaler::Fsr1, 0.67), Upscaler::Fsr1);
        assert_eq!(active(Upscaler::Fsr1, 1.0), Upscaler::Bilinear, "FSR 1 has nothing to do at native");
        assert_eq!(active(Upscaler::Fsr3, 1.0), Upscaler::Fsr3, "FSR 3.1 anti-aliases at native");
        assert_eq!(active(Upscaler::Fsr3, 1.5), Upscaler::Bilinear, "supersampled frames are filtered down");
        assert_eq!(active(Upscaler::Bilinear, 0.5), Upscaler::Bilinear);
    }
}
