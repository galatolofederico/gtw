//! wgpu renderer: G-buffer pass, shadow map, deferred sky/water resolve,
//! ambient occlusion (GTAO), volumetric clouds, GPU particles, temporal
//! anti-aliasing, bloom and tonemap.
use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// Distance (km) to the clouds a texel sees (reprojects the cloud history) and to the
/// scene surface it marched to (depth-aware filtering and upsampling).
const CLOUD_DIST: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
/// Share of each pixel's light that is ambient (sky and bounce), and the
/// ambient occlusion that darkens it.
pub(crate) const AMB: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// Side of the sun's shadow map unless the graphics settings say otherwise
/// (`Renderer::set_shadow_res`; the shaders read the size off the texture).
const SHADOW_RES: u32 = 4096;
pub const CASCADES: usize = 3;
/// g-buffer msaa samples (1, 2 or 4: graphics settings), set once at startup.
static MSAA_N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(4);
pub(crate) fn msaa() -> u32 {
    MSAA_N.load(std::sync::atomic::Ordering::Relaxed)
}
const MAX_INST: usize = 16384;
/// scenery instances drawn in a frame, and materials in all the packs together
const MAX_SCENERY: usize = 8192;
const SC_MATS: usize = 128;
const MAX_FALL: usize = 4096;
/// trees and undergrowth close enough to the camera for their leaf cards
pub const MAX_NEAR: usize = 20000;
const BLOOM_MIPS: u32 = 6;
/// post.wgsl's AGX_EXPOSURE: the tonemap puts the scene at `G.time.z` times
/// this, which is the exposure the upscalers are given.
pub const AGX_EXPOSURE: f32 = 1.6;
pub const MAX_PARTICLES: u32 = 1 << 16;
/// Smoke volume around the effects anchor: cells along east, north, up; cell
/// size in metres; height of the bottom of the grid above the anchor.
pub const VOL_N: [u32; 3] = [40, 40, 48];
pub const VOL_CELL: f32 = 8.0;
pub const VOL_ZMIN: f32 = -24.0;
const VOL_CELLS: u64 = (VOL_N[0] * VOL_N[1] * VOL_N[2]) as u64;
const UB_ALIGN: u64 = 256;
const MAX_TILES: u64 = 1024;
const MAX_BUILDS: u64 = 512; // detail tiles; one extra slot is kept for loose trees
/// Bytes between objects' uniforms: two dynamic-offset alignments, as `ObjU`
/// outgrew one when the aircraft's dent and wing bending were added.
const OBJ_STRIDE: u64 = 2 * UB_ALIGN;
/// pieces of aircraft (ours, wreckage, the other players') drawn in a frame
pub const MAX_OBJS: u64 = 1024;
/// airfields drawn at once, and lights on moving things (aircraft)
pub const MAX_FIELDS: usize = 8;
pub const MAX_DYN_LIGHTS: usize = 8192;
/// Shadow map layer that only the aircraft casts into, a few metres across.
pub const PLANE_LAYER: usize = CASCADES;
/// Share of the sun shadow map's side the aircraft's layer uses
/// (`Renderer::plane_shadow_res`): all of it, or half of it on an integrated
/// GPU, where every texel the aircraft covers runs its window and gear-well
/// cut-outs (a few mm a texel either way).
const PLANE_SHADOW_DIV_INTEGRATED: u32 = 2;
/// Photorealistic 3D tiles drawn at once, and the side of their coverage mask
/// (texels; it spans 2 * tiles3d::MASK_HALF metres).
const MAX_PHOTO: u64 = 4096;
const PHOTO_MASK: u32 = 4096;
/// r: the 3D tiles cover this texel; g: height of our water surface (m), or
/// -1e4; b: light from the street lamps on our roads there (night only);
/// a: height of those roads (m), or -1e4
const PHOTO_MASK_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// Slot of the coverage mask's projection among the cascade matrices.
pub const MASK_SLOT: usize = CASCADES + 1;
/// Side of the square of building tiles (sources::DATA_ZOOM) around the camera
/// whose fade-in the terrain reads, to leave their lights to them (`Frame::cover`).
pub const COVER_N: usize = 64;
/// Slot of the shore mask's projection (shore.rs).
pub const SHORE_SLOT: usize = CASCADES + 2;
/// Slot of the shore's wide mask's projection (shore.rs).
pub const SHORE_WIDE_SLOT: usize = CASCADES + 3;

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub inv_view_proj: [[f32; 4]; 4],
    pub shadow_vp: [[[f32; 4]; 4]; CASCADES],
    pub planet: [f32; 4],
    pub sun_dir: [f32; 4],
    pub east: [f32; 4],
    pub north: [f32; 4],
    pub up: [f32; 4],
    pub screen: [f32; 4],
    pub time: [f32; 4],
    pub cloud: [f32; 4],
    pub light_pos: [f32; 4],
    pub light_col: [f32; 4],
    /// explosion volume: corner of the grid relative to camera, w = cell size
    pub vol_o: [f32; 4],
    /// explosion volume up axis, w = 1 while the volume holds live smoke
    pub vol_u: [f32; 4],
    pub misc: [f32; 4],
    pub csm: [f32; 4],
    pub scorch: [[f32; 4]; 8],
    pub crater: [[f32; 4]; 8],
    /// last frame's view-projection (without its sub-pixel offset), relative to last frame's camera
    pub prev_vp: [[f32; 4]; 4],
    /// camera movement since last frame; w = 1 when the cloud history is usable
    pub cam_delta: [f32; 4],
    /// xyz direction to the moon, w its illuminance
    pub moon: [f32; 4],
    /// explosion volume east and north axes
    pub vol_e: [f32; 4],
    pub vol_n: [f32; 4],
    /// the aircraft's own shadow map
    pub pshadow_vp: [[f32; 4]; 4],
    /// texel size (m), depth bias, share of the layer used, w = 1 when in use
    pub pshadow: [f32; 4],
    /// temporal anti-aliasing: xy this frame's sub-pixel offset in uv, z = 1
    /// when it is on, w = 1 when the aircraft is drawn
    pub taa: [f32; 4],
    /// camera-relative position to the aircraft's body frame
    pub plane_inv: [[f32; 4]; 4],
    /// body frame to last frame's clip space
    pub plane_prev: [[f32; 4]; 4],
    /// centre and half size of the aircraft in its body frame
    pub plane_box: [[f32; 4]; 2],
    /// xyz how far the wind has carried the clouds (planet frame, m), w haze (1 = clear day)
    pub weather: [f32; 4],
    /// xyz direction the wind aloft blows towards (planet frame), w number of storm cells
    pub storm: [f32; 4],
    /// storm cells, two each: centre (km, cloud space) and tower radius (km); maturity
    pub storm_cells: [[f32; 4]; 2 * crate::weather::MAX_STORMS],
    /// ground material layers: metres per repeat, metres of relief, scale of
    /// the second sample, photo contrast (0 = procedural); set by the renderer
    pub det: [[f32; 4]; 16],
    /// photorealistic 3D tiles: camera-relative to coverage mask projection
    pub photo_vp: [[f32; 4]; 4],
    /// zone radius (m, 0 = off), dissolve band (m), mask valid
    pub photo: [f32; 4],
    /// zone centre (on the ground) relative to the camera
    pub photo_c: [f32; 4],
    /// the sun the 3D tiles' photographs were taken under (unit), w how sure
    /// that is (0..1), or -1 to light them flat as photographs
    pub photo_sun: [f32; 4],
    /// destroyed buildings cut out of the 3D tiles (tiles3d::PhotoFrame)
    pub cut: [[f32; 4]; 3 * crate::tiles3d::MAX_CUTS],
    pub cut_v: [[f32; 4]; crate::tiles3d::MAX_CUT_V / 2],
    /// cloud shadow map (`CloudShadow`, set by the renderer): the one to look up
    /// (`CloudShadow`, common.wgsl `cloud_sun`): the one lookups read, the one being drawn
    pub cs: [[f32; 4]; 2 * CS_MAP],
    /// mid-level cloud: amount 0..1, height (m)
    pub alto: [f32; 4],
    /// the ocean (ocean.rs `Ocean::globals`): offset of its coordinates and
    /// the wind direction; wind, wave height, whitecap cover; the ripples'
    /// slope covariance; the slope covariance each cascade's mips lose
    pub ocean: [[f32; 4]; crate::ocean::OCEAN_G],
    /// camera-relative to the shore distance field (shore.rs)
    pub shore_vp: [[f32; 4]; 4],
    /// field valid, metres per texel, farthest distance kept (m), side of the square (m)
    pub shore: [f32; 4],
    /// churned white water where a crash went into it, two each: centre
    /// (camera-relative) + radius (m, 0 = none); strength 0..1, age (s), seed
    pub foam: [[f32; 4]; 16],
    /// the start screen: x = 1 while it shows (deep-space sky, no clouds)
    pub start: [f32; 4],
    /// storms close up (storm.rs): bit mask of the cells near enough to darken
    /// the ground, rain at the camera 0..1, lightning flashes lit, how deep the
    /// camera is in a storm's shade 0..1
    pub stormfx: [f32; 4],
    /// lightning lighting the scene, two each: one end of the lit stretch
    /// (camera-relative) + brightness; the other end + kind (1 = to the ground)
    pub flash: [[f32; 4]; 2 * crate::storm::MAX_FLASHES],
    /// the rain falling around the camera (precip.wgsl): how far it has moved
    /// past the camera (east, north, up, m, wrapped) + intensity; its velocity
    /// relative to the camera (planet frame, m/s) + nearest drop drawn (m)
    pub rain: [[f32; 4]; 2],
    /// graphics settings (set by the renderer): cloud step length (1 = High),
    /// render resolution scale, 1 / window width and height (the tonemap's output)
    pub quality: [f32; 4],
    /// graphics settings (graphics.rs): building LOD distance factor, tree
    /// impostor distance factor, how far out trees are drawn (m); 0 = High
    pub detail: [f32; 4],
    /// the rain deck (nimbostratus, weather.rs `deck_globals`): centre of its
    /// area (cloud space, km) + amount (0 = none); axis + mean base (m); axis +
    /// top (m); darkness under it, -, how far its scud hangs (m), area radius (km)
    pub nimbo: [[f32; 4]; 4],
    /// city light in the air (skyglow.rs `SkyGlow::globals`): map centre
    /// relative to the camera + half side (0 = none); its east axis + ground
    /// height under the camera; its north axis + how far the lights are on
    pub glow_c: [f32; 4],
    pub glow_e: [f32; 4],
    pub glow_n: [f32; 4],
    /// the flown aircraft's landing lights (common.wgsl `spot_light`): two
    /// lamps, each camera-relative position + intensity (0 = off); then the
    /// beam's direction + cosine of its half-angle
    pub spot: [[f32; 4]; 3],
    /// the flown aircraft's NAV 1 receiver, for the course deviation
    /// indicators in its cockpit (plane.wgsl, gauge 15): course needle -1..1
    /// (+ fly right), glide slope needle -1..1 (+ fly up), OBS (degrees),
    /// flags (1 alive, 2 TO, 4 FROM, 8 glide slope alive)
    pub nav: [f32; 4],
    /// Cessna instruments, radio receivers and heading-up navigation plot
    /// (rows 0-7); the autoflight and systems state the glass cockpits show
    /// (rows 8-11, main.rs `cockpit_globals`, read by deck650.wgsl).
    pub cockpit: [[f32; 4]; 12],
    /// snow lying on the ground (snow.rs `globals`): snow line under the
    /// camera (m), its change northwards (m per m), +1 / -1 for the north /
    /// south pole, 1 when there is any snow
    pub snow: [f32; 4],
    /// today's real snow cover (snowmap.rs `SnowMap::globals`): map centre
    /// relative to the camera + half side; its east axis + how much it counts
    /// (0 = not at all); its north axis
    pub snow_c: [f32; 4],
    pub snow_e: [f32; 4],
    pub snow_n: [f32; 4],
    /// snowflakes around the camera (precip.wgsl): how far they have moved
    /// past the camera (east, north, up, m, wrapped) + how much of what falls
    /// here is snow (0..1); their velocity relative to the camera (m/s) +
    /// blowing snow (0..1)
    pub flakes: [f32; 4],
    pub flakes_v: [f32; 4],
    /// fresh snow lying where it has snowed (snow.rs `Fresh`): its snow line
    /// (m), how far it has settled (0..1)
    pub fresh: [f32; 4],
    /// dynamic snow tracks (snowtrack.rs `SnowTrack::globals`): SW corner of
    /// the active window relative to the camera + side length in m (0 = none);
    /// its east axis + toroidal u offset (m); its north axis + toroidal v offset (m)
    pub snow_tr_c: [f32; 4],
    pub snow_tr_e: [f32; 4],
    pub snow_tr_n: [f32; 4],
    /// precomputed horizontal bounding circles of the storm cells (storm.rs
    /// `bound`): xyz centre on the planet surface (planet frame, m) + w radius (m)
    pub storm_bounds: [[f32; 4]; crate::weather::MAX_STORMS],
    /// precomputed lone-cell turrets (5 per cell: tc.xy, rk, ck) and flanking
    /// towers (3 per cell: fo.xy, fr, ft) (storm.rs `turrets`)
    pub storm_turrets: [[f32; 4]; 8 * crate::weather::MAX_STORMS],
    /// graphics settings for the shaders (graphics.rs): x the water's detail
    /// (0 plain, 1 full, 2 full + screen-space reflections), y 1 while the
    /// clouds are drawn; zw unused
    pub lite: [f32; 4],
    /// precomputed top-mip means for the ground detail arrays (GroundMats):
    /// rgb = 1 / max(albedo mean, 0.02), w = cavity mean; det_hmean = height mean (4 per vec4)
    pub det_mean: [[f32; 4]; 16],
    pub det_hmean: [[f32; 4]; 4],
    /// apron floodlights lighting the ground and what stands on it (common.wgsl
    /// `flood_light`), eight, two each: camera-relative lamp + intensity (0 =
    /// none, and none after it); beam direction + cosine of its half-angle
    pub flood: [[f32; 4]; 16],
}

/// Raindrops drawn around the camera in rain (as many as the rain is heavy show).
pub const RAIN_DROPS: u32 = 131072;

/// The cloud shadow map: the sun's view of the cloud layer (common.wgsl
/// `cloud_sun`). Each texel is a ray along the sun from a point on the cloud
/// base; it holds the cloud's optical depth in three height bands and how
/// closed the deck is there. Two cascades around the camera, in three slots of
/// a six-layer array: the back map is redrawn a slice of rows per frame, while
/// what lookups read (a separate two-layer texture, csmix.wgsl) cross-fades
/// from the previous map to the front one over the same cycle, so the shadows
/// glide instead of stepping when a map is swapped in. Each map keeps the sun
/// it was drawn for (aimed ahead at where the sun will be while it shows) and
/// the wind drift of the clouds then, which lookups make up for. Map centres
/// snap to whole outer texels of a fixed tangent frame, so one map's texels sit
/// exactly where the last one's did: no crawling, and the cross-fade is a plain
/// texel-by-texel blend.
const CS_RES: u32 = 1024;
const CS_SLICES: u32 = 16;
/// The same on an integrated GPU, where the rays cost a few ms a frame: the
/// map then takes ~2 s to redraw, in which the clouds drift a fraction of a texel.
const CS_SLICES_INTEGRATED: u32 = 64;
const CS_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// half sizes of the two cascades, metres
const CS_INNER: f64 = 40000.0;
const CS_OUTER: f64 = 200000.0;
/// vec4s of Globals.cs per map (common.wgsl `cs_map`)
pub const CS_MAP: usize = 6;
/// how far the mid-level layer reaches above its height (clouds.wgsl ALTO_TOP)
const ALTO_TOP: f32 = 700.0;
type CsMap = [[f32; 4]; CS_MAP];
struct CloudShadow {
    p_in: wgpu::RenderPipeline,
    p_out: wgpu::RenderPipeline,
    p_in_plain: wgpu::RenderPipeline,
    p_out_plain: wgpu::RenderPipeline,
    layers: Vec<wgpu::TextureView>,
    /// the cross-fade into what lookups read
    p_mix: [wgpu::RenderPipeline; 2],
    bg_mix: wgpu::BindGroup,
    mix_ub: wgpu::Buffer,
    mix_layers: Vec<wgpu::TextureView>,
    prev: CsMap,
    front: CsMap,
    back: CsMap,
    /// slots (pairs of layers) of the previous, front and back maps
    slot: [u32; 3],
    slice: u32,
    /// how many frames a redraw is spread over
    slices: u32,
    ok: bool,
    /// the tangent frame map centres snap in: centre, east, north
    anchor: [glam::DVec3; 3],
    /// last frame's sun, and how far it moves per frame (smoothed)
    sun: glam::DVec3,
    sun_v: glam::DVec3,
    /// this frame's cross-fade: layers, usable, shifts; weight
    mix: ([i32; 8], f32),
}

impl CloudShadow {
    /// Where the map should be now: around the point of the cloud base the sun
    /// shines down through onto the ground below the camera. The sun it is drawn
    /// for is `sun` (unit).
    fn aim(&mut self, g: &Globals, sun: glam::DVec3, reanchor: bool) -> CsMap {
        let v = |a: &[f32; 4]| glam::DVec3::new(a[0] as f64, a[1] as f64, a[2] as f64);
        let up = (-v(&g.planet)).normalize();
        // The bands reach up to the mid-level layer when there is one.
        let (base, mut top) = (g.cloud[1], g.cloud[3].max(g.cloud[1] + 100.0));
        if g.alto[0] > 0.0 {
            top = top.max(g.alto[1] + ALTO_TOP);
        }
        let mu = up.dot(sun);
        // Where the sun's ray from the ground below crosses the cloud base (far off for a low sun).
        let rb = g.planet[3] as f64 + base as f64;
        let gp = up * g.planet[3] as f64;
        let b = gp.dot(sun);
        let t = -b + (b * b - gp.length_squared() + rb * rb).max(0.0).sqrt();
        let c = if mu > -0.05 { (gp + sun * t.min(150000.0)).normalize() } else { up };
        // Snap the centre to whole outer texels (5 inner ones) in the anchor's
        // gnomonic plane; a new anchor once the centre has wandered far from it.
        let [ca, ea, na] = self.anchor;
        let gno = |c: glam::DVec3| glam::DVec2::new(c.dot(ea), c.dot(na)) / c.dot(ca).max(0.5) * rb;
        if reanchor || ca.dot(c) < 0.9 || gno(c).length() > 100000.0 {
            let east = v(&g.east);
            let e = (east - c * east.dot(c)).normalize();
            self.anchor = [c, e, c.cross(e)];
        }
        let [ca, ea, na] = self.anchor;
        let step = 2.0 * CS_OUTER / CS_RES as f64;
        let o = (gno(c) / step).round() * step;
        let f = |a: glam::DVec3, w: f64| [a.x as f32, a.y as f32, a.z as f32, w as f32];
        [f(ca, CS_INNER), f(ea, CS_OUTER), f(na, 0.0), [base, top, o.x as f32, o.y as f32], f(sun, 1.0), [g.weather[0], g.weather[1], g.weather[2], 0.0]]
    }

    /// Fill in this frame's maps; true when the back map is to be drawn whole.
    fn prepare(&mut self, g: &mut Globals) -> bool {
        let v = |a: &[f32; 4]| glam::DVec3::new(a[0] as f64, a[1] as f64, a[2] as f64);
        // The sun's motion per frame (fast when time runs faster); a jump of the
        // clock (or the first frame) is not motion.
        let sun = v(&g.sun_dir).normalize();
        let ds = sun - self.sun;
        let sun_jump = !self.ok || ds.length() > 0.015;
        self.sun_v = if sun_jump { glam::DVec3::ZERO } else { self.sun_v * 0.9 + ds * 0.1 };
        self.sun = sun;
        // Aim each map at the sun of the frame it shows at most: two cycles on,
        // when the lookups have cross-faded fully onto it.
        let ahead = (sun + self.sun_v * (2 * self.slices) as f64).normalize();
        // A jump (new flight, teleport, a new cloud base, a clock set by hand) redraws it all at once.
        let want = self.aim(g, ahead, !self.ok);
        let cen = |m: &CsMap| (v(&m[0]) + (v(&m[1]) * m[3][2] as f64 + v(&m[2]) * m[3][3] as f64) / 6.4e6).normalize();
        let jumped = cen(&self.front).dot(cen(&want)) < (0.3 * CS_INNER / 6.4e6).cos()
            || (self.front[3][0] - want[3][0]).abs() > 50.0
            || (self.front[3][1] - want[3][1]).abs() > 200.0
            || sun_jump;
        let full = !self.ok || jumped;
        if full || self.slice == 0 {
            self.back = want;
        }
        if full {
            // Nothing to fade from: every map is the one drawn now.
            let s = (self.slot[1] + 1) % 3;
            self.slot = [s, s, (s + 1) % 3];
            self.slice = 0;
        }
        let layer = |s: u32| (s * 2) as f32;
        self.back[2][3] = layer(if full { self.slot[0] } else { self.slot[2] });
        let (prev, front) = if full { (self.back, self.back) } else { (self.prev, self.front) };
        // How far the lookups have faded from the previous map to the front one;
        // not at all across a change of tangent frame (texels no longer line up).
        let same = prev[..2] == front[..2] && prev[2][..3] == front[2][..3];
        let a = if full || !same { 1.0 } else { (self.slice + 1) as f32 / self.slices as f32 };
        let lerp = |i: usize| -> [f32; 4] { std::array::from_fn(|j| prev[i][j] + (front[i][j] - prev[i][j]) * a) };
        // Lookups use the front map's grid, with the sun and drift in between.
        let mut look = front;
        let s = v(&lerp(4)).normalize();
        look[4] = [s.x as f32, s.y as f32, s.z as f32, 1.0];
        look[5] = lerp(5);
        g.cs[..CS_MAP].copy_from_slice(&look);
        g.cs[CS_MAP..].copy_from_slice(&self.back);
        // Front-to-previous texel shift of each cascade (whole texels: centres snap to outer ones).
        let sh = |ext: f64, k: usize| ((front[3][2 + k] - prev[3][2 + k]) as f64 * CS_RES as f64 / (2.0 * ext)).round() as i32;
        let lay = |s: u32| (s * 2) as i32;
        let (lp, lf) = if full { (lay(self.slot[0]), lay(self.slot[0])) } else { (lay(self.slot[0]), lay(self.slot[1])) };
        let p: [i32; 8] = [lp, lf, (a < 1.0) as i32, 0, sh(CS_INNER, 0), sh(CS_INNER, 1), sh(CS_OUTER, 0), sh(CS_OUTER, 1)];
        self.mix = (p, a);
        full
    }

    /// The cross-fade's parameters (csmix.wgsl `Mix`) for this frame.
    fn mix_bytes(&self) -> [u8; 48] {
        let mut b = [0u8; 48];
        b[..32].copy_from_slice(bytemuck::cast_slice(&self.mix.0));
        b[32..36].copy_from_slice(&self.mix.1.to_ne_bytes());
        b
    }

    /// Draw (a slice of) the back map, then make it the front once it is complete.
    fn draw(&mut self, enc: &mut wgpu::CommandEncoder, bg0: &wgpu::BindGroup, full: bool, plain: bool) {
        let back_layer = self.back[2][3] as usize;
        let rows = CS_RES / self.slices;
        let pipes = if plain { [&self.p_in_plain, &self.p_out_plain] } else { [&self.p_in, &self.p_out] };
        for (k, p) in pipes.into_iter().enumerate() {
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&self.layers[back_layer + k], false))], None));
            rp.set_pipeline(p);
            rp.set_bind_group(0, bg0, &[]);
            if !full {
                rp.set_scissor_rect(0, self.slice * rows, CS_RES, rows);
            }
            rp.draw(0..3, 0..1);
        }
        // What lookups read this frame, from the maps as they were before this
        // frame's slice (the back map's slot is neither of them).
        // on integrated gpus the fade moves 1/64 a frame, so the blend is redrawn every other frame.
        let skip_mix = !full && self.slices > CS_SLICES && self.slice % 2 == 1;
        for (k, p) in self.p_mix.iter().enumerate().filter(|_| !skip_mix) {
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&self.mix_layers[k], false))], None));
            rp.set_pipeline(p);
            rp.set_bind_group(0, &self.bg_mix, &[]);
            rp.draw(0..3, 0..1);
        }
        if full {
            self.prev = self.back;
            self.front = self.back;
            self.ok = true;
            return;
        }
        self.slice += 1;
        if self.slice >= self.slices {
            // previous <- front <- back; the slot neither of them holds is drawn next.
            self.prev = self.front;
            self.front = self.back;
            let (p, f) = (self.slot[1], self.slot[2]);
            self.slot = [p, f, (0..3).find(|&s| s != p && s != f).unwrap()];
            self.slice = 0;
        }
    }
}

/// Pipelines, bind groups and targets for drawing the atmosphere's
/// transmittance and multiple-scattering tables.
struct LutBake {
    p_trans: wgpu::RenderPipeline,
    p_ms: wgpu::RenderPipeline,
    bg_trans: wgpu::BindGroup,
    bg_ms: wgpu::BindGroup,
    trans: wgpu::TextureView,
    ms: wgpu::TextureView,
}

impl LutBake {
    /// Uses whatever is in the globals buffer: only the planet radius and the haze matter.
    fn draw(&self, enc: &mut wgpu::CommandEncoder) {
        for (p, bg, target) in [(&self.p_trans, &self.bg_trans, &self.trans), (&self.p_ms, &self.bg_ms, &self.ms)] {
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(target, true))], None));
            rp.set_pipeline(p);
            rp.set_bind_group(0, bg, &[]);
            rp.draw(0..3, 0..1);
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Default)]
pub struct Particle {
    pub pos: [f32; 3],
    pub age: f32,
    pub vel: [f32; 3],
    pub life: f32,
    pub size0: f32,
    pub size1: f32,
    pub temp: f32,
    pub kind: f32,
    pub drag: f32,
    pub buoy: f32,
    pub seed: f32,
    pub alpha: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Default)]
pub struct PSim {
    pub anchor: [f32; 4],
    pub upv: [f32; 4],
    pub cam_r: [f32; 4],
    pub cam_u: [f32; 4],
    pub wind: [f32; 4],
    /// volume grid: east axis (w cell size), north axis (w lowest altitude)
    pub ve: [f32; 4],
    pub vn: [f32; 4],
    /// xyz: move every live particle by this much, once (the frame was re-anchored)
    pub shift: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub col: [f32; 4],
}

/// A vertex of the aircraft (or its wreckage).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Default, Debug)]
pub struct PartVertex {
    /// body-frame position at rest
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    /// linear albedo, a = baked ambient occlusion
    pub col: [f32; 4],
    /// surface coordinates in metres (seams, rivets, decals) or a local [-1, 1] frame
    pub uv: [f32; 2],
    /// material, surface pattern, exposure to wear, material-specific extra
    pub mat: [f32; 4],
}

/// A vertex of a tree or undergrowth model.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TreeVertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    /// rgb; a = 0 bark, 1 foliage, or on a leaf card the number of leaves along it
    pub col: [f32; 4],
    /// leaf cards: along, across, seed, style. Solids: w = kind of bark
    pub uv: [f32; 4],
    /// leaf cards: aspect and two shape parameters. Solids: how far the vertex
    /// draws in once the cards are out. w = openness to the sky
    pub aux: [f32; 4],
}

/// One tree: where, how big, which way it faces, and how far it has fallen.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TreeInst {
    /// xyz position (relative to its tile centre), w height scale (0 = gone)
    pub a: [f32; 4],
    /// xy fall direction (east, north) times fall angle, z yaw, w width scale
    pub b: [f32; 4],
    /// colour multiplier, 127 = neutral; a = 255, or for undergrowth the distance (m) it is drawn to
    pub tint: [u8; 4],
}

/// One instanced solid: rows of a 3x4 camera-relative model matrix and a tint
/// (tint.a = 1 selects the matte rubble material).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Inst {
    pub m: [[f32; 4]; 3],
    pub tint: [f32; 4],
    /// rubble: a piece of facade's wall coordinates and style (inst.wgsl), else 0
    pub ext: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct RoadVertex {
    pub pos: [f32; 3],
    /// across, along, half width, distance to the nearest end (all metres)
    pub q: [f32; 4],
    pub kind: f32,
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct ObjU {
    model: [[f32; 4]; 4],
    tint: [f32; 4],
    glow: [f32; 4],
    params: [[f32; 4]; 6],
    tear: [[f32; 4]; 2],
    dmg: [f32; 4],
    dmg2: [f32; 4],
    crumple: [f32; 4],
    crumple_dir: [f32; 4],
    pose: [[f32; 4]; 3],
    flex: [f32; 4],
}
const _: () = assert!(std::mem::size_of::<ObjU>() <= OBJ_STRIDE as usize);

/// An airfield's pavement and lights on the GPU.
pub struct FieldGpu {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    n_idx: u32,
    lights: Option<(wgpu::Buffer, u32)>,
}

pub struct TileGpu {
    vbuf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    radius: f32,
    wet: bool,
    all_water: bool,
}
/// The cells of the terrain tiles' tessellated blocks (world.rs `tess_cells`),
/// each `cell | k << 16` for a 2^k x 2^k patch (terrain.wgsl `vs_tess`), in
/// groups of (tile index in `Frame::tiles`, k, first, count).
#[derive(Default)]
pub struct TessCells {
    pub cells: Vec<u32>,
    pub groups: Vec<(u32, u32, u32, u32)>,
}
/// A photorealistic 3D tile (tiles3d.rs) on the GPU.
pub struct PhotoGpu {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    n_idx: u32,
    bind: wgpu::BindGroup,
    /// its texture, for the pieces of a destroyed building (skin.rs)
    pub view: wgpu::TextureView,
    /// radius about the tile's origin (m), to leave it out of cascades it misses
    rad: f32,
}

/// A vertex of the 3D tiles' mesh carried by a piece of a destroyed
/// building (skin.rs): on the piece's own axes, with the slot of the piece.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Default)]
pub struct SkinVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub nrm: [i8; 4],
    pub slot: u32,
}
/// Slots of the pieces one destroyed building's mesh rides on, at most, and
/// over all buildings drawn at once (64 bytes each, photo.wgsl `Slot`).
pub const SKIN_SLOTS: usize = 16384;
const SKIN_SLOT_BUF: usize = 4 * SKIN_SLOTS;
/// The mesh of a destroyed building from one tile, on the GPU.
pub struct SkinGpu {
    vbuf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    n: u32,
}
pub struct BuildGpu {
    vbuf: wgpu::Buffer,
    pub ibuf: wgpu::Buffer,
    /// where the middle, finest and far levels start in the index runs
    n_main: u32,
    n_det: u32,
    n_far: u32,
    /// where each cell's part of each sort group ends in each run, and each
    /// cell's bounding sphere (buildings::BuildingData::cell_end, cell_bound)
    cell_end: Box<crate::buildings::CellEnds>,
    cell_bound: Vec<[f32; 4]>,
    bind: wgpu::BindGroup,
    road: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
    pub trees: Option<wgpu::Buffer>,
}
impl BuildGpu {
    /// The cells (bit per cell) with geometry inside a shadow cascade's box,
    /// `vp` its camera-relative view-projection; `o` the tile's offset from the camera.
    fn cells_in(&self, vp: &glam::Mat4, o: glam::Vec3) -> u64 {
        let (sx, sy) = (vp.row(0).truncate().length(), vp.row(1).truncate().length());
        let mut mask = 0u64;
        for (k, b) in self.cell_bound.iter().enumerate() {
            if b[3] < 0.0 {
                continue;
            }
            let q = *vp * (o + glam::Vec3::new(b[0], b[1], b[2])).extend(1.0);
            if q.x.abs() <= 1.0 + b[3] * sx && q.y.abs() <= 1.0 + b[3] * sy {
                mask |= 1 << k;
            }
        }
        mask
    }
    /// The cells (bit per cell) with geometry in the view frustum (`planes`,
    /// its side planes), and how near and how far from the camera each cell's
    /// geometry may lie; `o` the tile's offset from the camera.
    fn cells_seen(&self, planes: &[glam::Vec4; 4], o: glam::Vec3) -> (u64, [f32; crate::buildings::NCELL], [f32; crate::buildings::NCELL]) {
        let (mut mask, mut near, mut far) = (0u64, [0.0; crate::buildings::NCELL], [0.0; crate::buildings::NCELL]);
        for (k, b) in self.cell_bound.iter().enumerate() {
            if b[3] < 0.0 {
                continue;
            }
            let p = o + glam::Vec3::new(b[0], b[1], b[2]);
            if planes.iter().all(|pl| pl.truncate().dot(p) + pl.w >= -b[3] * pl.truncate().length()) {
                mask |= 1 << k;
                near[k] = (p.length() - b[3]).max(0.0);
                far[k] = p.length() + b[3];
            }
        }
        (mask, near, far)
    }
    /// Index ranges of sort groups `groups` of run `r` (0 base, 1 middle,
    /// 2 finest, 3 far stand-ins) in the cells of `mask`, neighbouring cells
    /// joined into one range.
    fn cell_ranges(&self, r: usize, groups: std::ops::Range<usize>, mask: u64, mut out: impl FnMut(u32, u32)) {
        use crate::buildings::NCELL;
        let start = [0, self.n_main, self.n_det, self.n_far][r];
        let e = &self.cell_end[r];
        let mut at = if groups.start == 0 { start } else { e[groups.start - 1][NCELL - 1] };
        let mut cur: Option<(u32, u32)> = None;
        for g in groups {
            for (c, &end) in e[g].iter().enumerate() {
                if end > at && mask & (1 << c) != 0 {
                    cur = match cur {
                        Some((s, ce)) if ce == at => Some((s, end)),
                        Some((s, ce)) => {
                            out(s, ce);
                            Some((at, end))
                        }
                        None => Some((at, end)),
                    };
                }
                at = end;
            }
        }
        if let Some((s, e)) = cur {
            out(s, e);
        }
    }
    /// The geometry buffers, in the order of `building_payload`.
    pub fn targets(&self) -> [Option<&wgpu::Buffer>; 5] {
        [Some(&self.vbuf), Some(&self.ibuf), self.road.as_ref().map(|r| &r.0), self.road.as_ref().map(|r| &r.1), self.trees.as_ref()]
    }
}
/// The bytes each of `BuildGpu::targets` is filled with.
pub fn building_payload(b: &crate::buildings::BuildingData) -> [&[u8]; 5] {
    [bytemuck::cast_slice(&b.verts), bytemuck::cast_slice(&b.indices), bytemuck::cast_slice(&b.road_verts), bytemuck::cast_slice(&b.road_idx), bytemuck::cast_slice(&b.trees)]
}
/// Bytes of a terrain tile's upload, for the per-frame upload budget.
pub fn tile_bytes(t: &crate::sources::TileData) -> usize {
    std::mem::size_of_val(&t.verts[..]) + t.imagery.iter().chain(&t.mask).chain(&t.lights).map(|m| m.len()).sum::<usize>()
}
/// Distance rings used to sort tree impostor chunks front-to-back for Early-Z occlusion.
pub const IMP_RINGS: u8 = 6;
/// One model drawn for a run of the frame's near instances.
pub struct NearDraw {
    pub mesh: u32,
    pub first: u32,
    pub count: u32,
    /// leaf cards rather than a solid model
    pub cards: bool,
    /// standing tree (already casts into cascade 0 via `dd.trees`), vs undergrowth
    pub tree: bool,
}
pub struct TreeDraw {
    pub mesh: u32,
    pub first: u32,
    pub count: u32,
    pub view: bool,
    /// bit per shadow cascade this batch casts into
    pub casc: u8,
    /// some of its trees may be close enough to have their crowns cut
    pub close: bool,
    /// drawn as impostors (`mesh` is then the model)
    pub imp: bool,
    /// every tree in the batch drawn with `mesh`, whatever its distance (shadows)
    pub fixed: bool,
    /// distance ring (0 = nearest .. IMP_RINGS - 1 = farthest) for front-to-back impostors
    pub ring: u8,
}
/// Everything drawn for one detail tile this frame.
pub struct DetailDraw<'a> {
    pub gpu: &'a BuildGpu,
    /// tile offset, roof atlas mapping, and how far each level of detail has
    /// faded in: middle, finest, the whole tile
    pub u: [f32; 16],
    pub view: bool,
    /// close enough for roads and the middle level of detail
    pub near: bool,
    /// buildings: 0 all close enough for their full geometry, 2 all far enough
    /// for their stand-ins, 1 some of each (the shader dissolves between them)
    pub far: u8,
    /// how many height classes (buildings::HCLASS, tallest first) are close
    /// enough for their full geometry, their middle and their finest level
    pub reach: [u8; 3],
    /// how many height classes (tallest first) have all their full geometry,
    /// middle and finest level inside the distance where it starts to
    /// dissolve, and done fading in (drawn without a discard, see `fs_whole`)
    pub settled: [u8; 3],
    /// the tile's bounding sphere
    pub radius: f32,
    pub casc: u8,
    pub trees: Vec<TreeDraw>,
    pub close: Vec<NearDraw>,
}
struct MeshLib {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    ranges: Vec<(u32, u32, i32)>,
}
pub struct MeshGpu {
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    n_idx: u32,
    /// its own textures (a downloaded aircraft's), in place of `bg_obj`'s
    bind: Option<std::sync::Arc<wgpu::BindGroup>>,
    /// indices before the instrument faces and displays, which come last.
    n_main: u32,
}

/// the aircraft shader specialised for one kind, without the instrument faces.
pub struct ObjFast {
    kind: u32,
    cut: wgpu::RenderPipeline,
    solid: wgpu::RenderPipeline,
}

/// triangles of instrument faces (9) and displays (14) moved to the end, and where they start.
fn split_instruments(verts: &[PartVertex], idx: &[u32]) -> (Vec<u32>, u32) {
    let inst = |t: &[u32]| verts.get(t[0] as usize).is_some_and(|v| matches!(v.mat[0].round() as i32, 9 | 14));
    let mut main: Vec<u32> = Vec::with_capacity(idx.len());
    let mut rest = Vec::new();
    for t in idx.chunks(3) {
        if t.len() == 3 && inst(t) { rest.extend_from_slice(t) } else { main.extend_from_slice(t) }
    }
    let n = main.len() as u32;
    main.extend(rest);
    (main, n)
}

/// the aircraft shader source with its kind fixed and the instrument faces left out.
pub fn specialise(src: &str, kind: u32) -> String {
    src.replacen("const KIND: u32 = 99u;", &format!("const KIND: u32 = {kind}u;"), 1).replacen("const INSTR: u32 = 1u;", "const INSTR: u32 = 0u;", 1)
}

/// Instanced meshes that stand on the street (traffic, the player's cars and
/// the avatar): drawn in view after the road ribbons, not before the terrain.
fn on_street(mesh: u32) -> bool {
    mesh < crate::cars::N_MODELS as u32 || (crate::avatar::BASE..crate::bailout::MESH_BASE + crate::bailout::MESHES as u32).contains(&mesh)
}

/// A geometry pipeline (see the modes in `Renderer::new`).
#[allow(clippy::too_many_arguments)]
fn geom_pipeline(d: &wgpu::Device, l0: &wgpu::BindGroupLayout, l_casc: &wgpu::BindGroupLayout, label: &str, l1: Option<&wgpu::BindGroupLayout>, m: &wgpu::ShaderModule, buffers: &[Option<wgpu::VertexBufferLayout>], mode: u8) -> wgpu::RenderPipeline {
    let shadow = mode == 1 || mode == 4 || mode == 10 || mode == 12 || mode == 13 || mode == 21 || mode == 22;
    let groups = [Some(l0), l1, Some(l_casc)];
    let layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: if shadow { &groups[..] } else { &groups[..2] }, immediate_size: 0 });
    let alpha = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
    let keep = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
    let targets = [
        Some(wgpu::ColorTargetState { format: HDR, blend: if mode == 2 { Some(wgpu::BlendState { color: alpha, alpha: keep }) } else { None }, write_mask: wgpu::ColorWrites::ALL }),
        Some(wgpu::ColorTargetState { format: HDR, blend: None, write_mask: if mode == 2 { wgpu::ColorWrites::empty() } else { wgpu::ColorWrites::ALL } }),
        Some(wgpu::ColorTargetState { format: AMB, blend: None, write_mask: if mode == 2 { wgpu::ColorWrites::empty() } else { wgpu::ColorWrites::ALL } }),
    ];
    d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState { module: m, entry_point: Some(match mode {
            10 | 22 => "vs_shadow_fixed",
            _ if shadow => "vs_shadow",
            3 => "vs_card",
            7 => "vs_near",
            11 => "vs_skin",
            8 | 17 => "vs_tess",
            9 | 18 => "vs_cells",
            _ => "vs",
        }), compilation_options: Default::default(), buffers },
        primitive: wgpu::PrimitiveState {
            cull_mode: match mode {
                // Tree world basis (e, upn, n) has det = -1, so outward model triangles are CW;
                // culling Front keeps all sun-facing faces and culls back-facing shadow triangles.
                12..=14 | 21 | 22 => Some(wgpu::Face::Front),
                8 | 9 | 15..=20 => Some(wgpu::Face::Back),
                _ => None,
            },
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(mode != 2),
            depth_compare: Some(match mode {
                _ if shadow => wgpu::CompareFunction::LessEqual,
                2 => wgpu::CompareFunction::GreaterEqual,
                _ => wgpu::CompareFunction::Greater,
            }),
            stencil: Default::default(),
            bias: if shadow { wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 } } else { Default::default() },
        }),
        multisample: wgpu::MultisampleState { count: if shadow { 1 } else { msaa() }, mask: !0, alpha_to_coverage_enabled: false },
        fragment: if mode == 4 || mode == 13 {
            Some(wgpu::FragmentState { module: m, entry_point: Some("fs_shadow"), compilation_options: Default::default(), targets: &[] })
        } else if shadow {
            None
        } else {
            Some(wgpu::FragmentState { module: m, entry_point: Some(match mode { 3 => "fs_card", 5 => "fs_solid", 7 => "fs_cut", 11 => "fs_skin", 14 => "fs_whole", 16..=18 => "fs_photo", 19 => "fs_far", 20 => "fs_water", _ => "fs" }), compilation_options: Default::default(), targets: &targets })
        },
        multiview_mask: None,
        cache: None,
    })
}

/// Held while the aircraft shader is compiled for a pipeline set (`ObjFast`,
/// `ObjGeneric`): with a cold driver cache each compile of it takes 1.5-2 GB
/// on Intel's compiler, so two at once beside the world loading took a 16 GB
/// laptop out of memory. The specialised set goes first (`FAST_STATE`).
static BIG_COMPILE: std::sync::Mutex<()> = std::sync::Mutex::new(());
/// 0 no specialised set asked for yet, 1 one is being built, 2 built.
static FAST_STATE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
/// Of the two pipelines that draw the aircraft, how many the specialised set
/// being built and the generic set have (for `Renderer::aircraft_prep`).
static FAST_STEPS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static GEN_STEPS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The aircraft not drawn yet, its shaders still being compiled: of the
/// `total` pipelines it needs, `done`, and since when it has waited.
#[derive(Clone, Copy, Debug)]
pub struct AircraftPrep {
    pub done: u32,
    pub total: u32,
    pub secs: f64,
}

/// The aircraft pipelines with the shader that draws every kind and the
/// instruments. Built on a thread of their own, after the first frame: Intel's
/// shader compiler takes minutes over this shader when its cache is cold
/// (about a minute per pipeline on an Alder Lake iGPU with Mesa 25.3), during
/// which the window would stand frozen. Until they are ready the kind flown is
/// drawn with its specialised shader (`ObjFast`), and what needs these waits.
/// The two that draw the aircraft come first (`ObjGenMsg::Draw`), so it shows
/// a minute or two in rather than after all five; shadows and glass follow.
pub struct ObjGeneric {
    obj: wgpu::RenderPipeline,
    solid: wgpu::RenderPipeline,
    /// the aircraft's shadows and windows, once built
    rest: Option<ObjRest>,
}

pub struct ObjRest {
    shadow: wgpu::RenderPipeline,
    shadow_solid: wgpu::RenderPipeline,
    glass: wgpu::RenderPipeline,
}

/// What the generic aircraft pipeline thread sends, in this order.
pub enum ObjGenMsg {
    Draw(wgpu::RenderPipeline, wgpu::RenderPipeline),
    Rest(ObjRest),
}

const A_PART: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2, 4 => Float32x4];

/// The generic `obj` (cut) and `solid` pipelines: what draws the aircraft.
fn obj_generic_draw(d: &wgpu::Device, l0: &wgpu::BindGroupLayout, l_obj: &wgpu::BindGroupLayout, l_casc: &wgpu::BindGroupLayout, m: &wgpu::ShaderModule) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let vb = [Some(wgpu::VertexBufferLayout { array_stride: 64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &A_PART })];
    let geom = |label: &str, mode: u8| geom_pipeline(d, l0, l_casc, label, Some(l_obj), m, &vb, mode);
    use std::sync::atomic::Ordering::Relaxed;
    let obj = geom("obj", 0);
    GEN_STEPS.store(1, Relaxed);
    let solid = geom("obj solid", 5);
    GEN_STEPS.store(2, Relaxed);
    (obj, solid)
}

fn obj_generic_rest(d: &wgpu::Device, l0: &wgpu::BindGroupLayout, l_obj: &wgpu::BindGroupLayout, l_casc: &wgpu::BindGroupLayout, l_glass: &wgpu::BindGroupLayout, m: &wgpu::ShaderModule) -> ObjRest {
    let vb = [Some(wgpu::VertexBufferLayout { array_stride: 64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &A_PART })];
    let geom = |label: &str, mode: u8| geom_pipeline(d, l0, l_casc, label, Some(l_obj), m, &vb, mode);
    let (shadow, shadow_solid) = (geom("obj shadow", 4), geom("obj shadow solid", 1));
    let p_glass = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("glass"),
        layout: Some(&d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(l0), Some(l_obj), Some(l_glass)], immediate_size: 0 })),
        vertex: wgpu::VertexState { module: m, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &vb },
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::Greater), stencil: Default::default(), bias: Default::default() }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: m,
            entry_point: Some("fs_glass"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: HDR,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
                    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    ObjRest { shadow, shadow_solid, glass: p_glass }
}

/// the g-buffer pipeline for aircraft parts, mode 0 or 5 of `geom`.
fn obj_pipeline(d: &wgpu::Device, layout: &wgpu::PipelineLayout, m: &wgpu::ShaderModule, cut: bool) -> wgpu::RenderPipeline {
    const A_PART: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2, 4 => Float32x4];
    let opts = wgpu::PipelineCompilationOptions::default();
    let targets = [
        Some(wgpu::ColorTargetState { format: HDR, blend: None, write_mask: wgpu::ColorWrites::ALL }),
        Some(wgpu::ColorTargetState { format: HDR, blend: None, write_mask: wgpu::ColorWrites::ALL }),
        Some(wgpu::ColorTargetState { format: AMB, blend: None, write_mask: wgpu::ColorWrites::ALL }),
    ];
    d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("obj fast"),
        layout: Some(layout),
        vertex: wgpu::VertexState { module: m, entry_point: Some("vs"), compilation_options: opts.clone(), buffers: &[Some(wgpu::VertexBufferLayout { array_stride: 64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &A_PART })] },
        primitive: Default::default(),
        depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Greater), stencil: Default::default(), bias: Default::default() }),
        multisample: wgpu::MultisampleState { count: msaa(), mask: !0, alpha_to_coverage_enabled: false },
        fragment: Some(wgpu::FragmentState { module: m, entry_point: Some(if cut { "fs" } else { "fs_solid" }), compilation_options: opts, targets: &targets }),
        multiview_mask: None,
        cache: None,
    })
}
pub struct DrawObj {
    pub mesh: usize,
    pub model: Mat4,
    pub tint: [f32; 4],
    pub glow: [f32; 4],
    /// instrument readings and light states (see plane.wgsl)
    pub params: [[f32; 4]; 6],
    /// bit per shadow layer it casts into (bit `PLANE_LAYER` = the aircraft's own)
    pub cast: u8,
    /// drawn in the transparent pass
    pub glass: bool,
    /// has openings cut in it, or a break (needs the discarding pipelines)
    pub cut: bool,
    /// up to two breaks through the body frame: plane normal xyz, offset (see damage.rs)
    pub tear: [[f32; 4]; 2],
    /// mode of each break, scrapes 0..1, raggedness of the edge (m)
    pub dmg: [f32; 4],
    /// oil film on the windshield, crack centre z and y, crack size
    pub dmg2: [f32; 4],
    /// a lasting dent (damage.rs `crumple`): its centre (body frame) and radius (m)
    pub crumple: [f32; 4],
    /// the dent's push direction into the airframe (body frame) and depth (m)
    pub crumple_dir: [f32; 4],
    /// the top three rows of the piece's pose (mesh frame to body frame), so
    /// the dent and the wing bending can be worked out in the body frame
    pub pose: [[f32; 4]; 3],
    /// wing bending: tip deflection over the half-span squared (1/m), 0 for
    /// anything that is not part of a wing
    pub flex: f32,
}

impl DrawObj {
    pub const DEFAULT: DrawObj = DrawObj { mesh: 0, model: Mat4::IDENTITY, tint: [0.0; 4], glow: [0.0; 4], params: [[0.0; 4]; 6], cast: 0, glass: false, cut: false, tear: [[0.0; 4]; 2], dmg: [0.0; 4], dmg2: [0.0; 4], crumple: [0.0; 4], crumple_dir: [0.0; 4], pose: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]], flex: 0.0 };
}

pub struct Frame<'a> {
    pub g: Globals,
    /// terrain tile and its uniform: offset, info, tessellated cell block (see terrain.wgsl)
    pub tiles: Vec<(&'a TileGpu, [f32; 12])>,
    /// the cells of those blocks, by the patch they are drawn with
    pub tess: TessCells,
    pub details: Vec<DetailDraw<'a>>,
    /// photorealistic 3D tiles: offset from the camera and geometric error,
    /// then their level-of-detail crossfade (photo.wgsl `PhotoTile`)
    pub photo: Vec<(&'a PhotoGpu, [[f32; 4]; 2])>,
    /// destroyed buildings' meshes and where their slots start in `skin_slots`
    pub skins: Vec<(&'a SkinGpu, u32)>,
    /// per slot: rotation rows with the camera-relative offset in w, tint (w = 0: gone)
    pub skin_slots: Vec<[[f32; 4]; 4]>,
    /// airfields and their centres relative to the camera
    pub fields: Vec<(&'a FieldGpu, [f32; 3])>,
    /// lights on moving things, camera-relative
    pub lights: Vec<crate::airfield::Light>,
    pub objs: Vec<DrawObj>,
    /// (mesh, instance) for cars, rubble, rocks
    pub insts: Vec<(u32, Inst)>,
    /// (mesh, instance) of the hand-made airport scenery (scenery.rs)
    pub scenery: Vec<(u32, Inst)>,
    /// (tree mesh, instance) for trees that have been knocked loose, camera-relative
    pub fall: Vec<(u32, TreeInst)>,
    /// trees (for their leaf cards) and undergrowth near the camera, relative to their tiles
    pub near: Vec<TreeInst>,
    /// shadow cascades, the aircraft's shadow, the 3D tiles' coverage mask
    pub casc_vp: [[[f32; 4]; 4]; CASCADES + 4],
    /// the shore field's part this frame (shore.rs), and for drawing its
    /// mask: the new square's centre (camera-relative) and the terrain tiles
    /// near it that are out of view (their uniforms follow the view tiles')
    pub shore: Option<crate::shore::Step>,
    pub shore_c: [f32; 3],
    pub shore_tiles: Vec<(&'a TileGpu, [f32; 12])>,
    pub psim: PSim,
    pub sim_steps: bool,
    /// the crash's fire simulation, while there is one
    pub fluid: Option<crate::fluid::FluidFrame>,
    /// this frame's ocean spectrum (ocean.rs)
    pub ocean: Option<crate::ocean::OceanU>,
    pub egui: Option<(Vec<egui::ClippedPrimitive>, egui::TexturesDelta, f32)>,
    pub shot: Option<std::path::PathBuf>,
    /// the start screen's title, drawn at this UI scale (None: not shown)
    /// raindrops to draw around the camera (0: no rain)
    pub rain_drops: u32,
    /// lightning channel segments (storm.rs `Lightning::segments`)
    pub bolts: Vec<[f32; 8]>,
    /// adaptive-LOD contrail ribbon nodes and segments (contrails.rs)
    pub contrails: &'a crate::contrails::DrawData,
    /// COVER_N x COVER_N, row by row: how far the building tile there has faded
    /// in (255 = fully drawn, 0 = none), from the corner tile in
    /// `World::cover_base`. The terrain's city glow gives way to it.
    pub cover: Vec<u8>,
    /// a new city-light map to upload (skyglow.rs, N x N RGBA16F)
    pub glow: Option<Vec<u16>>,
    /// a new snow cover map to upload (snowmap.rs, N x N RG8)
    pub snow: Option<Vec<u8>>,
    /// dirty sub-rectangles of the dynamic snow track map (snowtrack.rs, RG8)
    pub snow_tracks: Vec<crate::snowtrack::Upload>,
}

struct Targets {
    color_ms: wgpu::TextureView,
    aux_ms: wgpu::TextureView,
    amb_ms: wgpu::TextureView,
    depth_ms: wgpu::TextureView,
    ao_raw: wgpu::TextureView,
    ao: wgpu::TextureView,
    bg_gtao: wgpu::BindGroup,
    bg_ao_blur: wgpu::BindGroup,
    bg_dres: wgpu::BindGroup,
    color: wgpu::TextureView,
    aux: wgpu::TextureView,
    amb: wgpu::TextureView,
    depth: wgpu::TextureView,
    depth_tex: wgpu::Texture,
    hdr: wgpu::TextureView,
    hdr_tex: wgpu::Texture,
    /// `hdr` as it was before the glass: what the windows refract
    scene_tex: wgpu::Texture,
    bg_glass: wgpu::BindGroup,
    cloud: wgpu::TextureView,
    cloud_dist: wgpu::TextureView,
    cloud_raw: wgpu::TextureView,
    cloud_dist_raw: wgpu::TextureView,
    cloud_cls_data: wgpu::TextureView,
    cloud_cls_depth: wgpu::TextureView,
    cloud_hist: [wgpu::TextureView; 2],
    /// anti-aliased frames: this one and the one before, taking turns
    taa: [wgpu::TextureView; 2],
    bg_taa: [wgpu::BindGroup; 2],
    bloom: Vec<wgpu::TextureView>,
    ldr_tex: wgpu::Texture,
    ldr: wgpu::TextureView,
    comp_cls_depth: wgpu::TextureView,
    bg_hiz: wgpu::BindGroup,
    bg_comp_cls: wgpu::BindGroup,
    bg_comp_main: wgpu::BindGroup,
    bg_comp_edge: wgpu::BindGroup,
    bg_comp_globe: wgpu::BindGroup,
    bg_cloud: wgpu::BindGroup,
    bg_cloud_storm: wgpu::BindGroup,
    bg_cloud_plain_comp: wgpu::BindGroup,
    bg_cloud_rain: wgpu::BindGroup,
    bg_cloud_temporal: [wgpu::BindGroup; 2],
    bg_cloud_apply: [wgpu::BindGroup; 2],
    bg_particles: wgpu::BindGroup,
    /// Bloom and the tonemap read the frame from `hdr` (index 0) or from either `taa` texture (1, 2).
    bg_down0: [wgpu::BindGroup; 3],
    bg_down: Vec<wgpu::BindGroup>,
    bg_up: Vec<wgpu::BindGroup>,
    bg_final: [wgpu::BindGroup; 3],
    bg_blit: wgpu::BindGroup,
}

pub struct Renderer {
    gpu_timer: Option<crate::gputime::GpuTimer>,
    /// the whole frame's GPU time, read without waiting (dynamic graphics)
    frame_clock: Option<crate::gputime::FrameClock>,
    ground_mats: GroundMats,
    pub device: wgpu::Device,
    /// --fps-log: seconds spent this period in the game's own update, waiting
    /// for a frame to draw into (the GPU catching up), encoding, and submitting
    pub stats: [f64; 4],
    /// --headless: draw every frame in full but never show it
    pub headless: bool,
    /// a GPU that shares the CPU's memory and power (Apple silicon, laptop
    /// chips): the defaults trade a little for speed there
    pub integrated: bool,
    /// the GPU's PCI vendor id (0x8086 Intel, 0x1002 AMD, 0x10de NVIDIA)
    pub vendor: u32,
    /// the volumetric clouds drawn (graphics settings; off skips their march)
    pub clouds: bool,
    /// the cloud shadow map drawn and looked up (graphics settings)
    pub cloud_shadows: bool,
    pub queue: wgpu::Queue,
    instance: wgpu::Instance,
    window: Arc<winit::window::Window>,
    gpu_error: Arc<parking_lot::Mutex<Option<String>>>,
    device_lost: Arc<std::sync::atomic::AtomicBool>,
    surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    globals_ub: wgpu::Buffer,
    bg0: wgpu::BindGroup,
    bg0_pre: wgpu::BindGroup,
    /// for the shadow cascades: as `bg0_pre`, with the 3D tiles' coverage mask
    bg0_shadow: wgpu::BindGroup,
    lut_sky: wgpu::TextureView,
    shadow: Vec<wgpu::TextureView>,
    /// side of the shadow map's layers (texels)
    shadow_res: u32,
    bg0_src: Bg0Src,
    /// render resolution as a share of the window's (graphics settings)
    scale: f32,
    /// ambient occlusion on (off: the AO texture is cleared to unoccluded)
    pub ao: bool,
    /// cloud raymarch step length relative to High (clouds.wgsl)
    pub cloud_step: f32,
    casc_ub: wgpu::Buffer,
    bg_casc: wgpu::BindGroup,
    bg_tileu: wgpu::BindGroup,
    l_dres: wgpu::BindGroupLayout,
    l_ao: wgpu::BindGroupLayout,
    p_gtao: wgpu::RenderPipeline,
    p_ao_blur: wgpu::RenderPipeline,
    p_dres: wgpu::RenderPipeline,
    p_road: wgpu::RenderPipeline,
    p_field: wgpu::RenderPipeline,
    p_lights: wgpu::RenderPipeline,
    dyn_lights: wgpu::Buffer,
    /// falling rain and lightning channels (precip.wgsl), over the clouds
    p_rain: wgpu::RenderPipeline,
    p_veil: wgpu::RenderPipeline,
    p_bolt: wgpu::RenderPipeline,
    bolt_buf: wgpu::Buffer,
    /// adaptive-LOD contrail ribbons (contrails.wgsl), over the clouds
    p_contrails: wgpu::RenderPipeline,
    bg_contrails: wgpu::BindGroup,
    contrail_nodes: wgpu::Buffer,
    contrail_segs: wgpu::Buffer,
    /// loose trees; `p_tree_near` draws the rest short of IMP_DIST (with the
    /// outlines of their crowns cut into clumps), `imp` the ones beyond
    p_tree: wgpu::RenderPipeline,
    p_tree_near: wgpu::RenderPipeline,
    imp: crate::impostor::Impostors,
    p_tree_shadow: wgpu::RenderPipeline,
    p_tree_shadow_fixed: wgpu::RenderPipeline,
    p_card: wgpu::RenderPipeline,
    near_buf: wgpu::Buffer,
    p_inst: wgpu::RenderPipeline,
    p_inst_shadow: wgpu::RenderPipeline,
    inst_buf: wgpu::Buffer,
    fall_buf: wgpu::Buffer,
    tree_lib: Option<MeshLib>,
    inst_lib: Option<MeshLib>,
    /// hand-made airport scenery (scenery.rs): pipelines, its materials and
    /// textures (group 1), its meshes and this frame's instances
    p_scenery: wgpu::RenderPipeline,
    p_scenery_shadow: wgpu::RenderPipeline,
    l_scenery: wgpu::BindGroupLayout,
    scenery_buf: wgpu::Buffer,
    scenery_lib: Option<MeshLib>,
    bg_scenery: Option<wgpu::BindGroup>,
    l_tile: wgpu::BindGroupLayout,
    l_build: wgpu::BindGroupLayout,
    l_comp_cls: wgpu::BindGroupLayout,
    l_comp_main: wgpu::BindGroupLayout,
    l_comp_edge: wgpu::BindGroupLayout,
    l_comp_globe: wgpu::BindGroupLayout,
    l_hiz: wgpu::BindGroupLayout,
    ap_l: wgpu::TextureView,
    ap_t: wgpu::TextureView,
    cloud_refl: wgpu::TextureView,
    bg_atmlut: wgpu::BindGroup,
    bg_comp_sky: wgpu::BindGroup,
    p_hiz_build: wgpu::ComputePipeline,
    p_ap_build: wgpu::ComputePipeline,
    p_sky_build: wgpu::ComputePipeline,
    p_cloud_refl: wgpu::ComputePipeline,
    p_cloud_refl_plain: wgpu::ComputePipeline,
    l_cloud: wgpu::BindGroupLayout,
    l_cloud_storm: wgpu::BindGroupLayout,
    l_cloud_rain: wgpu::BindGroupLayout,
    l_cloud_apply: wgpu::BindGroupLayout,
    l_cloud_temporal: wgpu::BindGroupLayout,
    l_post: wgpu::BindGroupLayout,
    l_pdraw: wgpu::BindGroupLayout,
    /// the glass pass: a copy of the lit scene behind it (to refract) and the depth
    l_glass: wgpu::BindGroupLayout,
    tile_ub: wgpu::Buffer,
    l_photo: wgpu::BindGroupLayout,
    photo_ub: wgpu::Buffer,
    p_photo: wgpu::RenderPipeline,
    p_photo_mask: wgpu::RenderPipeline,
    p_photo_shadow: wgpu::RenderPipeline,
    p_photo_shadow_fast: wgpu::RenderPipeline,
    l_skin: wgpu::BindGroupLayout,
    p_skin: wgpu::RenderPipeline,
    skin_slots: wgpu::Buffer,
    p_water_top: wgpu::RenderPipeline,
    /// where any ground is, into the shore mask (shore.rs)
    p_shore_cover: wgpu::RenderPipeline,
    p_lamp_top: wgpu::RenderPipeline,
    photo_mask: wgpu::TextureView,
    /// the mask holds something that has to be cleared when no tiles are drawn
    photo_mask_dirty: bool,
    /// how far each building tile around the camera has faded in (`Frame::cover`)
    light_cover: wgpu::Texture,
    light_cover_view: wgpu::TextureView,
    /// city light rising off the ground around the camera (skyglow.rs)
    glow_tex: wgpu::Texture,
    /// the start screen globe's cloud map (globeclouds.rs)
    gclouds_tex: wgpu::Texture,
    /// today's real snow cover around the camera (snowmap.rs)
    snow_tex: wgpu::Texture,
    /// dynamic snow tracks and crash trails around the aircraft (snowtrack.rs)
    snow_track_tex: wgpu::Texture,
    build_ub: wgpu::Buffer,
    obj_ub: wgpu::Buffer,
    bg_obj: wgpu::BindGroup,
    l_obj: wgpu::BindGroupLayout,
    /// the aircraft shader source, kept to specialise it per kind off the main thread.
    obj_src: std::sync::Arc<String>,
    pl_obj: wgpu::PipelineLayout,
    obj_fast: Option<ObjFast>,
    obj_fast_rx: Option<(u32, crossbeam_channel::Receiver<ObjFast>)>,
    paint_ub: wgpu::Buffer,
    font_ub: wgpu::Buffer,
    tile_ibuf: wgpu::Buffer,
    tile_nidx: u32,
    p_sky: wgpu::RenderPipeline,
    p_terrain: wgpu::RenderPipeline,
    p_terrain_far: wgpu::RenderPipeline,
    p_terrain_water: wgpu::RenderPipeline,
    p_terrain_tess: wgpu::RenderPipeline,
    p_terrain_cells: wgpu::RenderPipeline,
    p_terrain_photo: wgpu::RenderPipeline,
    p_terrain_tess_photo: wgpu::RenderPipeline,
    p_terrain_cells_photo: wgpu::RenderPipeline,
    /// index buffers of the tessellated cell patches (1, 2, 4 .. TESS_N sub-cells
    /// across, where each starts and ends), and of a single cell quad
    tess_ibuf: wgpu::Buffer,
    tess_idx: [std::ops::Range<u32>; 5],
    /// per tessellated cell: which, and its patch (`TessCells`), and room for so many
    tess_buf: wgpu::Buffer,
    tess_cap: usize,
    cell_ibuf: wgpu::Buffer,
    p_build: wgpu::RenderPipeline,
    /// the same without any discard in the fragment shader (`fs_whole`)
    p_build_whole: wgpu::RenderPipeline,
    p_build_shadow: wgpu::RenderPipeline,
    /// the generic aircraft pipelines, once built (`ObjGeneric`)
    obj_gen: Option<ObjGeneric>,
    obj_gen_rx: Option<crossbeam_channel::Receiver<ObjGenMsg>>,
    /// wait for them rather than draw without them (screenshots, recordings)
    pub wait_objs: bool,
    /// since when the aircraft has waited for its pipelines (`aircraft_prep`)
    obj_wait: Option<(std::time::Instant, AircraftPrep)>,
    p_comp_cls: wgpu::RenderPipeline,
    p_comp_sky: wgpu::RenderPipeline,
    p_comp_opaque: wgpu::RenderPipeline,
    p_comp_glass: wgpu::RenderPipeline,
    p_comp_water: wgpu::RenderPipeline,
    p_comp_edge: Option<wgpu::RenderPipeline>,
    p_composite: wgpu::RenderPipeline,
    /// the composite again, sample by sample, where water shares a pixel
    /// only with MSAA: at one sample there are no water edges to shade sample by sample
    p_composite_edge: Option<wgpu::RenderPipeline>,
    p_cloud: wgpu::RenderPipeline,
    p_cloud_plain: wgpu::RenderPipeline,
    p_cloud_classify: wgpu::RenderPipeline,
    p_cloud_plain_cls: wgpu::RenderPipeline,
    p_cloud_storm_cls: wgpu::RenderPipeline,
    p_cloud_rain: wgpu::RenderPipeline,
    p_cloud_apply: wgpu::RenderPipeline,
    p_cloud_temporal: wgpu::RenderPipeline,
    p_particles: wgpu::RenderPipeline,
    p_taa: wgpu::RenderPipeline,
    p_psim: wgpu::ComputePipeline,
    p_vol: [wgpu::ComputePipeline; 5],
    bg_vol_a: wgpu::BindGroup,
    bg_vol_b: wgpu::BindGroup,
    vol_acc: wgpu::Buffer,
    pub fluid: crate::fluid::FluidGpu,
    ocean: crate::ocean::OceanGpu,
    shore: crate::shore::ShoreGpu,
    fluid_t: crate::fluid::FluidTargets,
    /// game time (s) until which some smoke may still be alive, and the time now
    smoke_until: f32,
    /// until when (game time) any particle may still be alive: past it the
    /// whole ring is dead, and neither simulated nor drawn
    parts_until: f32,
    now: f32,
    p_down: wgpu::RenderPipeline,
    p_up: wgpu::RenderPipeline,
    p_final: wgpu::RenderPipeline,
    p_final_taa: wgpu::RenderPipeline,
    p_blit: wgpu::RenderPipeline,
    psim_ub: wgpu::Buffer,
    pbuf: wgpu::Buffer,
    bg_psim: wgpu::BindGroup,
    phead: u32,
    black: wgpu::TextureView,
    targets: Targets,
    /// which cloud history texture this frame writes; false until it holds a valid frame
    cloud_flip: usize,
    cloud_hist_ok: bool,
    cshadow: CloudShadow,
    /// what the transmittance and multi-scatter tables need to be redrawn
    lut_bake: LutBake,
    /// the haze the tables were last drawn with
    lut_haze: f32,
    /// the same for the anti-aliased frame history
    taa_flip: usize,
    taa_hist_ok: bool,
    /// upscaling below 100% render resolution (upscale.rs)
    pub up: crate::upscale::Upscale,
    /// the GPU drawing (for the upscaler's suggestion, graphics.rs)
    pub gpu: wgpu::AdapterInfo,
    pub meshes: Vec<MeshGpu>,
    pub egui: egui_wgpu::Renderer,
}

pub(crate) fn tex_entry(binding: u32, dim: wgpu::TextureViewDimension, depth: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: ALL_STAGES,
        ty: wgpu::BindingType::Texture {
            sample_type: if depth { wgpu::TextureSampleType::Depth } else { wgpu::TextureSampleType::Float { filterable: true } },
            view_dimension: dim,
            multisampled: false,
        },
        count: None,
    }
}
/// Every stage the game has shaders for. Not `ShaderStages::all()`: that also
/// names the task and mesh stages, and wgpu 30's Metal backend then hands the
/// fragment and compute stages the wrong buffers and textures (it stores a
/// group's resources in one stage order and looks them up in another). The
/// particle step got the 128-byte uniform block in place of the particle
/// buffer and read and wrote megabytes past its end: GPU faults, blank frames.
const ALL_STAGES: wgpu::ShaderStages = wgpu::ShaderStages::VERTEX.union(wgpu::ShaderStages::FRAGMENT).union(wgpu::ShaderStages::COMPUTE);
pub(crate) fn ub_entry(binding: u32, dynamic: bool, size: u64) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: ALL_STAGES,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: dynamic, min_binding_size: wgpu::BufferSize::new(size) },
        count: None,
    }
}
pub(crate) fn samp_entry(binding: u32, ty: wgpu::SamplerBindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: ALL_STAGES, ty: wgpu::BindingType::Sampler(ty), count: None }
}
pub(crate) fn tex_res(binding: u32, v: &wgpu::TextureView) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::TextureView(v) }
}
pub(crate) fn buf_res(binding: u32, b: &wgpu::Buffer, size: u64) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry { binding, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: b, offset: 0, size: wgpu::BufferSize::new(size) }) }
}

/// The sun's shadow map: one layer per cascade and one for the aircraft, as
/// an array (for lookups) and one view per layer (to draw into).
fn make_shadow(d: &wgpu::Device, res: u32) -> (wgpu::TextureView, Vec<wgpu::TextureView>) {
    let tex = d.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow"),
        size: wgpu::Extent3d { width: res, height: res, depth_or_array_layers: CASCADES as u32 + 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let all = tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
    let layers = (0..CASCADES as u32 + 1)
        .map(|i| tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2), base_array_layer: i, array_layer_count: Some(1), ..Default::default() }))
        .collect();
    (all, layers)
}

/// Everything in the main globals bind group (`bg0`) but the shadow map, so
/// it can be made again when the shadow map is.
struct Bg0Src {
    layout: wgpu::BindGroupLayout,
    ub: wgpu::Buffer,
    trans: wgpu::TextureView,
    ms: wgpu::TextureView,
    sky: wgpu::TextureView,
    noise: wgpu::TextureView,
    wave: wgpu::TextureView,
    s_clamp: wgpu::Sampler,
    s_repeat: wgpu::Sampler,
    s_shadow: wgpu::Sampler,
    s_aniso: wgpu::Sampler,
    vol: wgpu::TextureView,
    det_alb: wgpu::TextureView,
    det_nrm: wgpu::TextureView,
    s_detail: wgpu::Sampler,
    mask: wgpu::TextureView,
    cs: wgpu::TextureView,
    ocean: wgpu::TextureView,
    shore: wgpu::TextureView,
    glow: wgpu::TextureView,
    gclouds: wgpu::TextureView,
    snow: wgpu::TextureView,
    snow_track: wgpu::TextureView,
}

impl Bg0Src {
    fn make(&self, d: &wgpu::Device, shadow: &wgpu::TextureView) -> wgpu::BindGroup {
        fn samp(b: u32, s: &wgpu::Sampler) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry { binding: b, resource: wgpu::BindingResource::Sampler(s) }
        }
        d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &self.layout,
            entries: &[
                buf_res(0, &self.ub, 0),
                tex_res(1, &self.trans),
                tex_res(2, &self.ms),
                tex_res(3, &self.sky),
                tex_res(4, &self.noise),
                tex_res(5, &self.wave),
                samp(6, &self.s_clamp),
                samp(7, &self.s_repeat),
                tex_res(8, shadow),
                samp(9, &self.s_shadow),
                samp(10, &self.s_aniso),
                tex_res(11, &self.vol),
                tex_res(12, &self.det_alb),
                tex_res(13, &self.det_nrm),
                samp(14, &self.s_detail),
                tex_res(15, &self.mask),
                tex_res(16, &self.cs),
                tex_res(17, &self.ocean),
                tex_res(18, &self.shore),
                tex_res(19, &self.glow),
                tex_res(20, &self.gclouds),
                tex_res(21, &self.snow),
                tex_res(22, &self.snow_track),
            ],
        })
    }
}

pub(crate) fn make_tex(device: &wgpu::Device,label: &str, w: u32, h: u32, d: u32, mips: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> wgpu::Texture {
    make_tex_ms(device, label, w, h, d, mips, format, usage, 1)
}
#[allow(clippy::too_many_arguments)]
fn make_tex_ms(device: &wgpu::Device, label: &str, w: u32, h: u32, d: u32, mips: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages, samples: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: d },
        mip_level_count: mips,
        sample_count: samples,
        dimension: if d > 1 { wgpu::TextureDimension::D3 } else { wgpu::TextureDimension::D2 },
        format,
        usage,
        view_formats: &[],
    })
}
pub(crate) fn view(t: &wgpu::Texture) -> wgpu::TextureView {
    t.create_view(&wgpu::TextureViewDescriptor::default())
}

/// Upload a full mip chain for a square texture.
fn upload_mips(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, size: u32, format: wgpu::TextureFormat, bpp: u32, mips: &[Vec<u8>]) -> wgpu::TextureView {
    let t = make_tex(device, label, size, size, 1, mips.len() as u32, format, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
    for (i, m) in mips.iter().enumerate() {
        let s = (size >> i).max(1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &t, mip_level: i as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            m,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(s * bpp), rows_per_image: Some(s) },
            wgpu::Extent3d { width: s, height: s, depth_or_array_layers: 1 },
        );
    }
    view(&t)
}

/// An array texture with mip chains from RGBA layers (sRGB) of one size.
fn array_texture(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, w: u32, h: u32, layers: &[&[u8]]) -> wgpu::TextureView {
    let mips = 32 - w.max(h).leading_zeros();
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: layers.len() as u32 },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, data) in layers.iter().enumerate() {
        let mut cur: Vec<u8> = data.to_vec();
        let (mut cw, mut ch) = (w, h);
        for level in 0..mips {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &t, mip_level: level, origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 }, aspect: wgpu::TextureAspect::All },
                &cur,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(cw * 4), rows_per_image: Some(ch) },
                wgpu::Extent3d { width: cw, height: ch, depth_or_array_layers: 1 },
            );
            // The next level: each texel the mean of the four under it (in linear light, alpha weighted).
            let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
            let mut next = vec![0u8; (nw * nh * 4) as usize];
            let lin = |v: u8| { let c = v as f32 / 255.0; if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } };
            let enc = |c: f32| -> u8 { let c = c.clamp(0.0, 1.0); ((if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }) * 255.0 + 0.5) as u8 };
            for y in 0..nh {
                for x in 0..nw {
                    let at = |dx: u32, dy: u32| (((y * 2 + dy).min(ch - 1) * cw + (x * 2 + dx).min(cw - 1)) * 4) as usize;
                    let px = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
                    let a: f32 = px.iter().map(|&i| cur[i + 3] as f32).sum::<f32>();
                    let o = ((y * nw + x) * 4) as usize;
                    for c in 0..3 {
                        let v = if a > 0.0 { px.iter().map(|&i| lin(cur[i + c]) * cur[i + 3] as f32).sum::<f32>() / a } else { px.iter().map(|&i| lin(cur[i + c])).sum::<f32>() / 4.0 };
                        next[o + c] = enc(v);
                    }
                    next[o + 3] = ((a / 4.0) + 0.5) as u8;
                }
            }
            (cur, cw, ch) = (next, nw, nh);
        }
    }
    t.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() })
}

/// Size of plane.wgsl's `Paint` uniform: the box and view (`paint_box`),
/// then the livery (`set_livery`).
const PAINT_UB: u64 = 112;

/// What plane.wgsl's `hornet_paint` needs to know of a paint (its `PT`): the
/// rectangle each picture covers, across and fore and aft; how far ahead of
/// the datum the pictures' top edge is and how far below it the side views'
/// lower edge; how many pictures (0: none).
fn paint_box(paint: Option<&crate::aircraft::Paint>) -> [f32; 8] {
    let Some(p) = paint else { return [0.0; 8] };
    [p.span as f32, p.length as f32, (p.centre[0] + p.length * 0.5) as f32, (p.span * 0.5 - p.centre[1]) as f32, p.views as f32, p.relief, p.sheen, 0.0]
}

/// A texture array from square RGBA8 pictures, with its mip chain (each
/// texel of a level the mean of the four under it).
fn layer_texture(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, size: u32, layers: &[Vec<u8>], format: wgpu::TextureFormat) -> wgpu::TextureView {
    let mips = 32 - size.leading_zeros();
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: layers.len() as u32 },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (layer, px) in layers.iter().enumerate() {
        let (mut cur, mut w) = (px.clone(), size);
        for level in 0..mips {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &t, mip_level: level, origin: wgpu::Origin3d { x: 0, y: 0, z: layer as u32 }, aspect: wgpu::TextureAspect::All },
                &cur,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(w) },
                wgpu::Extent3d { width: w, height: w, depth_or_array_layers: 1 },
            );
            let nw = (w / 2).max(1);
            let mut next = vec![0u8; (nw * nw * 4) as usize];
            for y in 0..nw {
                for x in 0..nw {
                    for c in 0..4 {
                        let s = |dx: u32, dy: u32| cur[(((y * 2 + dy).min(w - 1) * w + (x * 2 + dx).min(w - 1)) * 4 + c) as usize] as u32;
                        next[((y * nw + x) * 4 + c) as usize] = ((s(0, 0) + s(1, 0) + s(0, 1) + s(1, 1) + 2) / 4) as u8;
                    }
                }
            }
            (cur, w) = (next, nw);
        }
    }
    t.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() })
}

/// An aircraft's paint (`aircraft::Paint`, e.g. the Hornet's
/// assets/fa18_paint.jpg, baked by assets/blender/fa18_paint.py): its pictures
/// side by side (the aircraft from above, from below, from the left and
/// perhaps the right), each a layer of the array with its own mip chain.
/// plane.wgsl projects them onto the skin (`hornet_paint`). A type without
/// paint gets a single texel.
fn paint_texture(device: &wgpu::Device, queue: &wgpu::Queue, paint: Option<&crate::aircraft::Paint>) -> wgpu::TextureView {
    let (img, layers) = match paint {
        Some(p) => (image::load_from_memory_with_format(p.jpg, image::ImageFormat::Jpeg).expect("an aircraft's paint").to_rgba8(), p.views),
        None => (image::RgbaImage::new(1, 1), 1),
    };
    let (w, h) = (img.width() / layers, img.height());
    let mips = 32 - w.max(h).leading_zeros();
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("aircraft paint"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: layers },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for layer in 0..layers {
        let mut cur: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let row = ((y * img.width() + layer * w) * 4) as usize;
            cur.extend_from_slice(&img.as_raw()[row..row + (w * 4) as usize]);
        }
        let (mut cw, mut ch) = (w, h);
        for level in 0..mips {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &t, mip_level: level, origin: wgpu::Origin3d { x: 0, y: 0, z: layer }, aspect: wgpu::TextureAspect::All },
                &cur,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(cw * 4), rows_per_image: Some(ch) },
                wgpu::Extent3d { width: cw, height: ch, depth_or_array_layers: 1 },
            );
            // The next level: each texel the mean of the four under it.
            let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
            let mut next = vec![0u8; (nw * nh * 4) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    for c in 0..4 {
                        let s = |dx: u32, dy: u32| cur[(((y * 2 + dy).min(ch - 1) * cw + (x * 2 + dx).min(cw - 1)) * 4 + c) as usize] as u32;
                        next[((y * nw + x) * 4 + c) as usize] = ((s(0, 0) + s(1, 0) + s(0, 1) + s(1, 1) + 2) / 4) as u8;
                    }
                }
            }
            (cur, cw, ch) = (next, nw, nh);
        }
    }
    t.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() })
}

/// Small allocator blocks (host-visible ones are half these sizes). Every
/// upload goes through staging memory that wgpu takes from the 246 MiB
/// CPU-visible VRAM window (BAR1) on cards without resizable BAR. With the
/// default 64-128 MiB blocks a burst of tile uploads grew the staging pool
/// until BAR1 was nearly full; the NVIDIA driver then maps the new block
/// through a slow path (~9 MB/s), and as the allocator keeps its last block
/// for good, every later upload crawled: ~1 fps over cities. Small blocks
/// keep BAR1 use close to what is in flight (see also world::UPLOAD_BUDGET).
const MEMORY_HINTS: wgpu::MemoryHints = wgpu::MemoryHints::Manual { suballocated_device_memory_block_size: (32 << 20)..(64 << 20) };

/// Sub-cells per side of a tessellated terrain cell (TN in terrain.wgsl).
const TESS_N: u16 = 16;
/// Size and layer count of the ground material arrays (see detail.wgsl).
const DET_N: u32 = crate::materials::N;
const DET_LAYERS: u32 = 13;
/// Per-layer metres per repeat, metres of relief, scale of the second sample,
/// photo contrast (0 = procedural layer): G.det, see common.wgsl. Must match
/// SIZE / RANGE in detail.wgsl for the procedural layers. Layers 8 and up
/// (mid-scale scans) are blank until their scan arrives.
const DET_PROC: [[f32; 4]; 16] = [
    [1.0, 0.05, 1.0, 0.0],
    [2.0, 0.06, 4.0, 0.0],
    [4.0, 0.45, 4.0, 0.0],
    [2.0, 0.03, 3.0, 0.0],
    [2.0, 0.006, 1.0, 0.0],
    [2.0, 0.035, 1.0, 0.0],
    [1.0, 0.08, 1.0, 0.0],
    [2.0, 0.05, 1.0, 0.0],
    [20.0, 0.0, 1.0, 0.0],
    [20.0, 0.0, 1.0, 0.0],
    [20.0, 0.0, 1.0, 0.0],
    [20.0, 0.0, 1.0, 0.0],
    [20.0, 0.0, 1.0, 0.0],
    [20.0, 0.0, 1.0, 0.0],
    [20.0, 0.0, 1.0, 0.0],
    [20.0, 0.0, 1.0, 0.0],
];

/// The ground material library: albedo + height and normal + cavity arrays
/// with full mip chains. Generated on the GPU at start-up (detail.wgsl), then
/// layers are replaced by photo scans as they finish loading (materials.rs).
struct GroundMats {
    alb: wgpu::Texture,
    nrm: wgpu::Texture,
    p_down: wgpu::ComputePipeline,
    params: [[f32; 4]; 16],
    means: [[f32; 4]; 16],
    hmeans: [[f32; 4]; 4],
    rb: wgpu::Buffer,
    rb_state: std::sync::Arc<std::sync::atomic::AtomicU8>,
    rb_dirty: bool,
    photos: Option<crossbeam_channel::Receiver<crate::materials::Layer>>,
}

fn det_level(t: &wgpu::Texture, m: u32) -> wgpu::TextureView {
    t.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), base_mip_level: m, mip_level_count: Some(1), ..Default::default() })
}

fn det_run(d: &wgpu::Device, enc: &mut wgpu::CommandEncoder, p: &wgpu::ComputePipeline, entries: &[wgpu::BindGroupEntry], size: u32) {
    let bg = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &p.get_bind_group_layout(0), entries });
    let mut cp = enc.begin_compute_pass(&Default::default());
    cp.set_pipeline(p);
    cp.set_bind_group(0, &bg, &[]);
    cp.dispatch_workgroups(size.div_ceil(8), size.div_ceil(8), DET_LAYERS);
}

impl GroundMats {
    fn new(d: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mips = DET_N.ilog2() + 1;
        let mk = |label: &str| {
            d.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: DET_N, height: DET_N, depth_or_array_layers: DET_LAYERS },
                mip_level_count: mips,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let (alb, nrm) = (mk("ground albedo"), mk("ground normal"));
        let m = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("detail"), source: wgpu::ShaderSource::Wgsl(include_str!("shaders/detail.wgsl").into()) });
        let pipe = |entry: &str| d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(entry), layout: None, module: &m, entry_point: Some(entry), compilation_options: Default::default(), cache: None });
        let (p_gen, p_nrm, p_down) = (pipe("gen"), pipe("nrm"), pipe("down"));
        let mut enc = d.create_command_encoder(&Default::default());
        let (a0, n0) = (det_level(&alb, 0), det_level(&nrm, 0));
        det_run(d, &mut enc, &p_gen, &[tex_res(0, &a0)], DET_N);
        det_run(d, &mut enc, &p_nrm, &[tex_res(1, &a0), tex_res(2, &n0)], DET_N);
        let stride = 256u64;
        let half = stride * DET_LAYERS as u64;
        let rb = d.create_buffer(&wgpu::BufferDescriptor { label: Some("det top"), size: half * 2, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut g = Self { alb, nrm, p_down, params: DET_PROC, means: [[1.0 / 0.35, 1.0 / 0.35, 1.0 / 0.35, 1.0]; 16], hmeans: [[0.5; 4]; 4], rb, rb_state: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)), rb_dirty: false, photos: None };
        g.mips(d, &mut enc);
        // Read back the 1x1 top mip of both arrays once at startup so shaders
        // never need per-pixel textureLoad calls for layer_mean / cav_mean.
        g.copy_top(&mut enc);
        queue.submit([enc.finish()]);
        g.rb.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = d.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
        g.read_rb();
        g
    }

    fn copy_top(&self, enc: &mut wgpu::CommandEncoder) {
        let stride = 256u64;
        let half = stride * DET_LAYERS as u64;
        let top = self.alb.mip_level_count() - 1;
        for (k, t) in [&self.alb, &self.nrm].into_iter().enumerate() {
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: t, mip_level: top, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: &self.rb, layout: wgpu::TexelCopyBufferLayout { offset: k as u64 * half, bytes_per_row: Some(stride as u32), rows_per_image: Some(1) } },
                wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: DET_LAYERS },
            );
        }
    }

    fn read_rb(&mut self) {
        let stride = 256usize;
        let half = stride * DET_LAYERS as usize;
        {
            let data = self.rb.slice(..).get_mapped_range().expect("det top mapped");
            for l in 0..DET_LAYERS as usize {
                let ao = l * stride;
                let no = half + l * stride;
                self.means[l] = [
                    1.0 / (data[ao] as f32 / 255.0).max(0.02),
                    1.0 / (data[ao + 1] as f32 / 255.0).max(0.02),
                    1.0 / (data[ao + 2] as f32 / 255.0).max(0.02),
                    data[no + 2] as f32 / 255.0,
                ];
                self.hmeans[l / 4][l % 4] = data[ao + 3] as f32 / 255.0;
            }
        }
        self.rb.unmap();
    }

    /// Rebuild every mip level from level 0 (both arrays, all layers).
    fn mips(&self, d: &wgpu::Device, enc: &mut wgpu::CommandEncoder) {
        for t in [&self.alb, &self.nrm] {
            for i in 1..t.mip_level_count() {
                let (src, dst) = (det_level(t, i - 1), det_level(t, i));
                det_run(d, enc, &self.p_down, &[tex_res(3, &src), tex_res(4, &dst)], (DET_N >> i).max(1));
            }
        }
    }

    /// Swap in any photo layers that have finished loading.
    fn poll(&mut self, d: &wgpu::Device, queue: &wgpu::Queue) {
        use std::sync::atomic::Ordering;
        if self.rb_state.load(Ordering::Acquire) == 2 {
            self.read_rb();
            self.rb_state.store(0, Ordering::Release);
        }
        let mut any = false;
        if let Some(rx) = &self.photos {
            while let Ok(l) = rx.try_recv() {
                for (t, data) in [(&self.alb, &l.alb), (&self.nrm, &l.nrm)] {
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo { texture: t, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: 0, z: l.layer }, aspect: wgpu::TextureAspect::All },
                        data,
                        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(DET_N * 4), rows_per_image: Some(DET_N) },
                        wgpu::Extent3d { width: DET_N, height: DET_N, depth_or_array_layers: 1 },
                    );
                }
                self.params[l.layer as usize] = l.params;
                self.means[l.layer as usize] = l.mean;
                self.hmeans[l.layer as usize / 4][l.layer as usize % 4] = l.hmean;
                any = true;
            }
        }
        if any {
            self.rb_dirty = true;
        }
        let do_rb = self.rb_dirty && self.rb_state.load(Ordering::Acquire) == 0;
        if any || do_rb {
            let mut enc = d.create_command_encoder(&Default::default());
            if any {
                self.mips(d, &mut enc);
            }
            if do_rb {
                self.copy_top(&mut enc);
                self.rb_dirty = false;
                self.rb_state.store(1, Ordering::Release);
            }
            queue.submit([enc.finish()]);
            if do_rb {
                let st = self.rb_state.clone();
                self.rb.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                    st.store(if r.is_ok() { 2 } else { 0 }, Ordering::Release);
                });
            }
        }
    }

    fn views(&self) -> (wgpu::TextureView, wgpu::TextureView) {
        let all = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        (all(&self.alb), all(&self.nrm))
    }
}

/// Tiling wave texture: RG slope, B height (caustics), A an independent noise
/// channel. The water's waves themselves come from the FFT ocean (ocean.rs).
fn wave_texture() -> Vec<u8> {
    const N: usize = 256;
    let mut rng = 0x9e3779b97f4a7c15u64;
    let mut rnd = move || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng >> 11) as f32 / (1u64 << 53) as f32
    };
    let field = |waves: &[(f32, f32, f32, f32)]| -> Vec<[f32; 3]> {
        let mut f = vec![[0f32; 3]; N * N];
        for y in 0..N {
            for x in 0..N {
                let (u, v) = (x as f32 / N as f32 * std::f32::consts::TAU, y as f32 / N as f32 * std::f32::consts::TAU);
                let mut o = [0f32; 3];
                for (kx, ky, a, ph) in waves {
                    let th = kx * u + ky * v + ph;
                    o[0] += a * th.sin();
                    o[1] += a * kx * th.cos();
                    o[2] += a * ky * th.cos();
                }
                f[y * N + x] = o;
            }
        }
        f
    };
    let mut wgen = |kmin: f32, kmax: f32, pow: f32, n: usize| -> Vec<(f32, f32, f32, f32)> {
        (0..n)
            .map(|_| {
                let k = kmin + (kmax - kmin) * rnd().powf(1.6);
                let a = rnd() * std::f32::consts::TAU;
                let (kx, ky) = ((k * a.cos()).round(), (k * a.sin()).round());
                let kk = (kx * kx + ky * ky).sqrt().max(1.0);
                (kx, ky, kk.powf(-pow), rnd() * std::f32::consts::TAU)
            })
            .collect()
    };
    let w1 = wgen(3.0, 26.0, 1.55, 64);
    let w2 = wgen(1.0, 18.0, 1.2, 48);
    let (f1, f2) = (field(&w1), field(&w2));
    let norm = |f: &[[f32; 3]], c: usize| f.iter().fold(0f32, |m, v| m.max(v[c].abs())).max(1e-6);
    let (hs, sx, sy, ns) = (norm(&f1, 0), norm(&f1, 1), norm(&f1, 2), norm(&f2, 0));
    let s = sx.max(sy);
    let mut out = vec![0u8; N * N * 4];
    for i in 0..N * N {
        out[i * 4] = ((f1[i][1] / s * 0.5 + 0.5) * 255.0) as u8;
        out[i * 4 + 1] = ((f1[i][2] / s * 0.5 + 0.5) * 255.0) as u8;
        out[i * 4 + 2] = ((f1[i][0] / hs * 0.5 + 0.5) * 255.0) as u8;
        out[i * 4 + 3] = ((f2[i][0] / ns * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0) as u8;
    }
    out
}

impl Renderer {
    /// `samples`: the g-buffer's MSAA samples (1, 2 or 4), or None to suit
    /// the GPU (fewer on integrated graphics).
    pub async fn new(window: Arc<winit::window::Window>, vsync: bool, samples: Option<u32>) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, force_fallback_adapter: false, compatible_surface: Some(&surface), ..Default::default() })
            .await?;
        let info = adapter.get_info();
        log::info!("GPU: {} ({:?})", info.name, info.device_type);
        crate::progress::step("gpu");
        let integrated = info.device_type == wgpu::DeviceType::IntegratedGpu;
        let samples = samples.unwrap_or(if !integrated { 4 } else if info.vendor == 0x8086 { 1 } else { 2 });
        // Intel integrated GPUs hang (i915 "GPU HANG ... context reset", then a lost
        // device) within a minute of the start screen at 4x; 2x runs clean (seen on
        // Alder Lake with Mesa 25.3). FABLE_MSAA still forces a sample count.
        let samples = if integrated && info.vendor == 0x8086 && std::env::var_os("FABLE_MSAA").is_none() { samples.min(2) } else { samples };
        // 1x (no MSAA) and 4x are always supported, 2x needs adapter format features.
        let afeat = wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
        let ok = samples == 4 || samples == 1 || (adapter.features().contains(afeat) && [HDR, AMB, DEPTH].iter().all(|f| adapter.get_texture_format_features(*f).flags.sample_count_supported(samples)));
        MSAA_N.store(if ok { samples } else { 4 }, std::sync::atomic::Ordering::Relaxed);
        let required_features = if msaa() != 4 && msaa() != 1 { afeat } else { wgpu::Features::empty() } | crate::gputime::GpuTimer::features(&adapter) | crate::gputime::FrameClock::features(&adapter);
        log::info!("MSAA {}x", msaa());
        let al = adapter.limits();
        if al.max_texture_dimension_2d < 8192
            || al.max_sampled_textures_per_shader_stage < 26
            || al.max_storage_buffers_per_shader_stage < 8
            || al.max_storage_textures_per_shader_stage < 4
        {
            anyhow::bail!(
                "GPU {} does not meet required limits (need max_texture_dimension_2d >= 8192 [{}], sampled_textures >= 26 [{}], storage_buffers >= 8 [{}], storage_textures >= 4 [{}])",
                info.name,
                al.max_texture_dimension_2d,
                al.max_sampled_textures_per_shader_stage,
                al.max_storage_buffers_per_shader_stage,
                al.max_storage_textures_per_shader_stage,
            );
        }
        let defaults = wgpu::Limits::default();
        let required_limits = wgpu::Limits {
            max_texture_dimension_2d: al.max_texture_dimension_2d.min(16384).max(8192),
            max_sampled_textures_per_shader_stage: al.max_sampled_textures_per_shader_stage.clamp(26, 64),
            max_storage_buffers_per_shader_stage: al.max_storage_buffers_per_shader_stage.max(defaults.max_storage_buffers_per_shader_stage).min(16),
            max_storage_textures_per_shader_stage: al.max_storage_textures_per_shader_stage.max(defaults.max_storage_textures_per_shader_stage).min(8),
            max_samplers_per_shader_stage: al.max_samplers_per_shader_stage.max(defaults.max_samplers_per_shader_stage).min(32),
            ..defaults
        };
        // AMD's FSR 4 runtime needs extensions wgpu does not turn on (fsr4.rs):
        // the device is opened through wgpu-hal with them when it may be used,
        // and the usual way otherwise or if that fails.
        let mut fsr4_device = false;
        let opened = if crate::fsr4::rt::wanted(&info) {
            let desc = wgpu::DeviceDescriptor { label: None, required_features: required_features | crate::fsr4::rt::features(&adapter), required_limits: required_limits.clone(), memory_hints: MEMORY_HINTS, ..Default::default() };
            match crate::fsr4::rt::open_device(&adapter, &desc) {
                Ok(dq) => {
                    fsr4_device = true;
                    Some(dq)
                }
                Err(e) => {
                    log::warn!("device for FSR 4: {e}; opening it without");
                    None
                }
            }
        } else {
            None
        };
        let (device, queue) = match opened {
            Some(dq) => dq,
            None => adapter.request_device(&wgpu::DeviceDescriptor { label: None, required_features, required_limits, memory_hints: MEMORY_HINTS, ..Default::default() }).await?,
        };
        let gpu_error: Arc<parking_lot::Mutex<Option<String>>> = Arc::new(parking_lot::Mutex::new(None));
        let device_lost = Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let err_slot = gpu_error.clone();
            device.on_uncaptured_error(Arc::new(move |err| {
                log::error!("GPU error: {err}");
                let mut g = err_slot.lock();
                if g.is_none() {
                    *g = Some(err.to_string());
                }
            }));
        }
        {
            let err_slot = gpu_error.clone();
            let lost_flag = device_lost.clone();
            device.set_device_lost_callback(move |reason, msg| {
                log::error!("GPU device lost ({reason:?}): {msg}");
                lost_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                let mut g = err_slot.lock();
                if g.is_none() {
                    *g = Some(format!("GPU device lost ({reason:?}): {msg}"));
                }
            });
        }
        let size = window.inner_size();
        let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).ok_or_else(|| anyhow::anyhow!("surface unsupported"))?;
        let caps = surface.get_capabilities(&adapter);
        config.format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        config.present_mode = if vsync { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        surface.configure(&device, &config);
        log::info!("surface {:?}, final passes {}", config.format, if direct_out(&config) { "straight to it" } else { "via a blit" });

        let d = &device;
        let common = include_str!("shaders/common.wgsl");
        let module = |name: &str, src: &str| d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(name), source: wgpu::ShaderSource::Wgsl(format!("{common}\n{src}").into()) });
        let facade = include_str!("shaders/facade.wgsl");
        let sky = include_str!("shaders/sky.wgsl");
        let obj_src = format!("{}\n{}\n{}\n{}\n{}", include_str!("shaders/plane.wgsl"), crate::aircraft::b747::deck::strings_wgsl(), include_str!("shaders/deck747.wgsl"), include_str!("shaders/pit18.wgsl"), include_str!("shaders/deck650.wgsl"));
        // Compile shader modules in parallel across threads so startup does not block on one core.
        let (m_obj, (m_lut, m_atmlut, m_terrain, m_photo, m_build), (m_comp, m_cloud, m_post, m_psim, m_pdraw), (m_road, m_field, m_lights, m_tree, m_inst), (m_vol, m_ao, m_taa, m_precip, m_contrails, m_dres)) = std::thread::scope(|s| {
            let t_obj = s.spawn(|| module("plane", &obj_src));
            let t1 = s.spawn(|| (
                module("lut", include_str!("shaders/lut.wgsl")),
                module("atmlut", include_str!("shaders/atmlut.wgsl")),
                module("terrain", include_str!("shaders/terrain.wgsl")),
                module("photo", include_str!("shaders/photo.wgsl")),
                module("buildings", &format!("{facade}\n{}", include_str!("shaders/buildings.wgsl"))),
            ));
            let t2 = s.spawn(|| (
                module("composite", &crate::single_sample::for_samples(&format!("{sky}\n{}", include_str!("shaders/composite.wgsl")), msaa())),
                module("clouds", &format!("{sky}\n{}", include_str!("shaders/clouds.wgsl"))),
                module("post", include_str!("shaders/post.wgsl")),
                module("psim", include_str!("shaders/particles_sim.wgsl")),
                module("pdraw", include_str!("shaders/particles_draw.wgsl")),
            ));
            let t3 = s.spawn(|| (
                module("roads", include_str!("shaders/roads.wgsl")),
                module("airfield", include_str!("shaders/airfield.wgsl")),
                module("lights", include_str!("shaders/lights.wgsl")),
                module("trees", include_str!("shaders/trees.wgsl")),
                module("inst", &format!("{facade}\n{}", include_str!("shaders/inst.wgsl"))),
            ));
            let m4 = (
                module("vol", include_str!("shaders/vol.wgsl")),
                module("ao", include_str!("shaders/ao.wgsl")),
                module("taa", include_str!("shaders/taa.wgsl")),
                module("precip", include_str!("shaders/precip.wgsl")),
                module("contrails", include_str!("shaders/contrails.wgsl")),
                d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("resolve"), source: wgpu::ShaderSource::Wgsl(crate::single_sample::for_samples(include_str!("shaders/resolve.wgsl"), msaa()).into()) }),
            );
            (t_obj.join().unwrap(), t1.join().unwrap(), t2.join().unwrap(), t3.join().unwrap(), m4)
        });
        let obj_src = std::sync::Arc::new(format!("{common}\n{obj_src}"));

        // ---- shared resources
        let sampler = |addr, aniso: u16, cmp| {
            d.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: addr,
                address_mode_v: addr,
                address_mode_w: addr,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: if cmp { wgpu::MipmapFilterMode::Nearest } else { wgpu::MipmapFilterMode::Linear },
                anisotropy_clamp: aniso,
                compare: if cmp { Some(wgpu::CompareFunction::LessEqual) } else { None },
                ..Default::default()
            })
        };
        let s_clamp = sampler(wgpu::AddressMode::ClampToEdge, 1, false);
        let s_repeat = sampler(wgpu::AddressMode::Repeat, 1, false);
        let s_aniso = sampler(wgpu::AddressMode::ClampToEdge, 16, false);
        let s_shadow = sampler(wgpu::AddressMode::ClampToEdge, 1, true);
        let rt = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        // Apple's GPUs keep render targets in a compressed form, and (macOS 27.2,
        // M4) the transmittance table came back from it with blocks missing:
        // every light in the scene lost its red and blue. Marking the tables as
        // storage textures too keeps them uncompressed; they are tiny.
        let lut = rt | wgpu::TextureUsages::STORAGE_BINDING;
        let lut_trans = view(&make_tex(d, "trans", 256, 64, 1, 1, HDR, lut));
        let lut_ms = view(&make_tex(d, "ms", 32, 32, 1, 1, HDR, lut));
        let lut_sky = view(&make_tex(d, "sky", 256, 128, 1, 1, HDR, lut));
        let black = view(&make_tex(d, "black", 1, 1, 1, 1, HDR, rt));
        let cs_tex = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("cloud shadow"),
            size: wgpu::Extent3d { width: CS_RES, height: CS_RES, depth_or_array_layers: 6 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CS_FMT,
            usage: rt,
            view_formats: &[],
        });
        let cs_all = cs_tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let cs_layers: Vec<_> = (0..6)
            .map(|i| cs_tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2), base_array_layer: i, array_layer_count: Some(1), ..Default::default() }))
            .collect();
        // What lookups read: the two cascades cross-faded between the last two maps.
        let cs_mix_tex = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("cloud shadow mix"),
            size: wgpu::Extent3d { width: CS_RES, height: CS_RES, depth_or_array_layers: 2 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: CS_FMT,
            usage: rt,
            view_formats: &[],
        });
        let cs_mix = cs_mix_tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let cs_mix_layers: Vec<_> = (0..2)
            .map(|i| cs_mix_tex.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2), base_array_layer: i, array_layer_count: Some(1), ..Default::default() }))
            .collect();
        // No cloud shadow (all zeros) for the passes that draw it or run before it.
        let no_cs = make_tex(d, "no cloud shadow", 1, 1, 1, 1, CS_FMT, rt).create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let arr = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let (shadow_all, shadow) = make_shadow(d, SHADOW_RES);
        let dummy_depth = arr(&make_tex(d, "dummy depth", 1, 1, 1, 1, DEPTH, rt));
        let vol3 = |label: &str, n: [u32; 3]| {
            d.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: n[0], height: n[1], depth_or_array_layers: n[2] },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D3,
                format: HDR,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default())
        };
        let vol_dens = vol3("smoke density", VOL_N);
        let vol_light = vol3("smoke light", VOL_N);
        let dummy_vol = vol3("dummy volume", [1, 1, 1]);
        let ap_l = vol3("ap scatter", [96, 54, 32]);
        let ap_t = vol3("ap trans", [96, 54, 32]);
        let sky_frustum = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky frustum"),
            size: wgpu::Extent3d { width: 256, height: 144, depth_or_array_layers: 2 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HDR,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }).create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let cloud_refl = view(&make_tex(d, "cloud refl", 384, 384, 1, 1, HDR, wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING));
        let noise_tex = make_tex(d, "noise3d", 128, 128, 128, 1, wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING);
        let noise = view(&noise_tex);
        {
            let m = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("noise3d"), source: wgpu::ShaderSource::Wgsl(include_str!("shaders/noise3d.wgsl").into()) });
            let p = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("noise3d"), layout: None, module: &m, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
            let bg = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &p.get_bind_group_layout(0), entries: &[tex_res(0, &noise)] });
            let mut enc = d.create_command_encoder(&Default::default());
            {
                let mut cp = enc.begin_compute_pass(&Default::default());
                cp.set_pipeline(&p);
                cp.set_bind_group(0, &bg, &[]);
                cp.dispatch_workgroups(32, 32, 32);
            }
            queue.submit([enc.finish()]);
        }
        let ocean = crate::ocean::OceanGpu::new(d, &queue);
        let shore = crate::shore::ShoreGpu::new(d, &queue, PHOTO_MASK_FMT);
        let wave = upload_mips(d, &queue, "wave", 256, wgpu::TextureFormat::Rgba8Unorm, 4, &crate::sources::mip_chain(wave_texture(), 256, 4));
        let ground_mats = GroundMats::new(d, &queue);
        let (det_alb, det_nrm) = ground_mats.views();
        let s_detail = sampler(wgpu::AddressMode::Repeat, 8, false);

        let globals_ub = d.create_buffer(&wgpu::BufferDescriptor { label: Some("globals"), size: std::mem::size_of::<Globals>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let d2 = wgpu::TextureViewDimension::D2;
        let l0 = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                ub_entry(0, false, std::mem::size_of::<Globals>() as u64),
                tex_entry(1, d2, false),
                tex_entry(2, d2, false),
                tex_entry(3, d2, false),
                tex_entry(4, wgpu::TextureViewDimension::D3, false),
                tex_entry(5, d2, false),
                samp_entry(6, wgpu::SamplerBindingType::Filtering),
                samp_entry(7, wgpu::SamplerBindingType::Filtering),
                tex_entry(8, wgpu::TextureViewDimension::D2Array, true),
                samp_entry(9, wgpu::SamplerBindingType::Comparison),
                samp_entry(10, wgpu::SamplerBindingType::Filtering),
                tex_entry(11, wgpu::TextureViewDimension::D3, false),
                tex_entry(12, wgpu::TextureViewDimension::D2Array, false),
                tex_entry(13, wgpu::TextureViewDimension::D2Array, false),
                samp_entry(14, wgpu::SamplerBindingType::Filtering),
                tex_entry(15, d2, false),
                tex_entry(16, wgpu::TextureViewDimension::D2Array, false),
                tex_entry(17, wgpu::TextureViewDimension::D2Array, false),
                tex_entry(18, d2, false),
                tex_entry(19, d2, false),
                tex_entry(20, d2, false),
                tex_entry(21, d2, false),
                tex_entry(22, d2, false),
            ],
        });
        // Coverage of the photorealistic 3D tiles, seen from above (tiles3d.rs).
        let photo_mask = view(&make_tex(d, "photo mask", PHOTO_MASK, PHOTO_MASK, 1, 1, PHOTO_MASK_FMT, rt));
        let light_cover = make_tex(d, "light cover", COVER_N as u32, COVER_N as u32, 1, 1, wgpu::TextureFormat::R8Unorm, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let light_cover_view = view(&light_cover);
        let glow_tex = make_tex(d, "city glow", crate::skyglow::N as u32, crate::skyglow::N as u32, 1, 1, wgpu::TextureFormat::Rgba16Float, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let glow_view = view(&glow_tex);
        // (cleared to zero: no clouds until a map comes in)
        let gclouds_tex = make_tex(d, "globe clouds", crate::globeclouds::W, crate::globeclouds::H, 1, 1, wgpu::TextureFormat::R8Unorm, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let gclouds_view = view(&gclouds_tex);
        let snow_tex = make_tex(d, "snow cover", crate::snowmap::N as u32, crate::snowmap::N as u32, 1, 1, wgpu::TextureFormat::Rg8Unorm, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let snow_view = view(&snow_tex);
        let snow_track_tex = make_tex(d, "snow tracks", crate::snowtrack::N as u32, crate::snowtrack::N as u32, 1, 1, wgpu::TextureFormat::Rg8Unorm, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let snow_track_view = view(&snow_track_tex);
        let no_mask = view(&make_tex(d, "no photo mask", 1, 1, 1, 1, PHOTO_MASK_FMT, rt));
        let make_bg0 = |sky: &wgpu::TextureView, trans: &wgpu::TextureView, ms: &wgpu::TextureView, sh: &wgpu::TextureView, vl: &wgpu::TextureView, pm: &wgpu::TextureView, cs: &wgpu::TextureView| {
            d.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("globals"),
                layout: &l0,
                entries: &[
                    buf_res(0, &globals_ub, 0),
                    tex_res(1, trans),
                    tex_res(2, ms),
                    tex_res(3, sky),
                    tex_res(4, &noise),
                    tex_res(5, &wave),
                    wgpu::BindGroupEntry { binding: 6, resource: wgpu::BindingResource::Sampler(&s_clamp) },
                    wgpu::BindGroupEntry { binding: 7, resource: wgpu::BindingResource::Sampler(&s_repeat) },
                    tex_res(8, sh),
                    wgpu::BindGroupEntry { binding: 9, resource: wgpu::BindingResource::Sampler(&s_shadow) },
                    wgpu::BindGroupEntry { binding: 10, resource: wgpu::BindingResource::Sampler(&s_aniso) },
                    tex_res(11, vl),
                    tex_res(12, &det_alb),
                    tex_res(13, &det_nrm),
                    wgpu::BindGroupEntry { binding: 14, resource: wgpu::BindingResource::Sampler(&s_detail) },
                    tex_res(15, pm),
                    tex_res(16, cs),
                    tex_res(17, &ocean.view),
                    tex_res(18, &shore.view),
                    tex_res(19, &glow_view),
                    tex_res(20, &gclouds_view),
                    tex_res(21, &snow_view),
                    tex_res(22, &snow_track_view),
                ],
            })
        };
        let bg0 = make_bg0(&lut_sky, &lut_trans, &lut_ms, &shadow_all, &vol_light, &photo_mask, &cs_mix);
        // What `bg0` is made of, to make it again for a new shadow map.
        let bg0_src = Bg0Src {
            layout: l0.clone(), ub: globals_ub.clone(), trans: lut_trans.clone(), ms: lut_ms.clone(), sky: lut_sky.clone(), noise: noise.clone(), wave: wave.clone(),
            s_clamp: s_clamp.clone(), s_repeat: s_repeat.clone(), s_shadow: s_shadow.clone(), s_aniso: s_aniso.clone(), vol: vol_light.clone(), det_alb: det_alb.clone(), det_nrm: det_nrm.clone(),
            s_detail: s_detail.clone(), mask: photo_mask.clone(), cs: cs_mix.clone(), ocean: ocean.view.clone(), shore: shore.view.clone(), glow: glow_view.clone(), gclouds: gclouds_view.clone(), snow: snow_view.clone(), snow_track: snow_track_view.clone(),
        };
        // Variant for passes that render into the LUTs / shadow map / smoke volume / photo mask / cloud shadow themselves.
        let bg0_pre = make_bg0(&black, &lut_trans, &lut_ms, &dummy_depth, &dummy_vol, &no_mask, &no_cs);
        let bg0_shadow = make_bg0(&black, &lut_trans, &lut_ms, &dummy_depth, &dummy_vol, &photo_mask, &no_cs);
        let bg0_boot = make_bg0(&black, &black, &black, &dummy_depth, &dummy_vol, &no_mask, &no_cs);
        let bg0_ms = make_bg0(&black, &lut_trans, &black, &dummy_depth, &dummy_vol, &no_mask, &no_cs);

        // ---- layouts
        let layout = |label: &str, entries: &[wgpu::BindGroupLayoutEntry]| d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some(label), entries });
        let l_tile = layout(
            "tile",
            &[
                ub_entry(0, true, 48),
                tex_entry(1, d2, false),
                tex_entry(2, d2, false),
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
                tex_entry(4, d2, false),
                tex_entry(5, d2, false),
            ],
        );
        let l_build = layout("build", &[ub_entry(0, true, 64), tex_entry(1, d2, false)]);
        let l_photo = layout("photo", &[ub_entry(0, true, 32), tex_entry(1, d2, false)]);
        let l_skin = layout(
            "skin",
            &[
                tex_entry(1, d2, false),
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(64) }, count: None },
            ],
        );
        let l_contrails = layout(
            "contrails",
            &[
                wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(64) }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: wgpu::BufferSize::new(8) }, count: None },
            ],
        );
        let skin_slots = d.create_buffer(&wgpu::BufferDescriptor { label: Some("skin slots"), size: ((SKIN_SLOT_BUF + SKIN_SLOTS) * 64) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let l_tileu = layout("tile offset", &[ub_entry(0, true, 48)]);
        let l_casc = layout("cascade", &[ub_entry(0, true, 64)]);
        let l_dres = layout(
            "depth resolve",
            &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: d2, multisampled: msaa() > 1 }, count: None }],
        );
        use crate::aircraft::b747::deck::{STRINGS, packed_string};
        use crate::stroke_font::{SEGMENTS, STARTS};
        let mut font_data = Vec::from(bytemuck::cast_slice::<_, u8>(&SEGMENTS));
        font_data.extend_from_slice(bytemuck::cast_slice(&STARTS));
        for s in STRINGS {
            font_data.extend_from_slice(bytemuck::bytes_of(&packed_string(s)));
        }
        let font_ub = d.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("aircraft stroke font"), contents: &font_data, usage: wgpu::BufferUsages::UNIFORM,
        });
        let l_obj = layout("obj", &[ub_entry(0, true, std::mem::size_of::<ObjU>() as u64), tex_entry(1, wgpu::TextureViewDimension::D2Array, false), ub_entry(2, false, PAINT_UB), ub_entry(3, false, font_ub.size())]);
        let l_glass = layout("glass", &[tex_entry(1, d2, false), tex_entry(2, d2, true)]);
        let l_scenery = layout("scenery", &[ub_entry(0, false, SC_MATS as u64 * 64), tex_entry(1, wgpu::TextureViewDimension::D2Array, false), tex_entry(2, wgpu::TextureViewDimension::D2Array, false)]);
        let ms_entry = |binding, depth| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: if depth { wgpu::TextureSampleType::Depth } else { wgpu::TextureSampleType::Float { filterable: false } },
                view_dimension: d2,
                multisampled: msaa() > 1,
            },
            count: None,
        };
        let l_comp_cls = layout("comp classify", &[
            tex_entry(0, d2, false), tex_entry(1, d2, false), tex_entry(2, d2, true), ms_entry(6, false),
        ]);
        let l_comp_sky = layout("comp sky", &[
            tex_entry(9, wgpu::TextureViewDimension::D3, false),
            tex_entry(10, wgpu::TextureViewDimension::D3, false),
            tex_entry(11, wgpu::TextureViewDimension::D2Array, false),
        ]);
        let hiz_ro = wgpu::BindGroupLayoutEntry {
            binding: 13,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: d2, multisampled: false },
            count: None,
        };
        let l_comp_main = layout("comp main", &[
            tex_entry(0, d2, false), tex_entry(1, d2, false), tex_entry(2, d2, true), tex_entry(3, d2, false), tex_entry(4, d2, false),
            tex_entry(9, wgpu::TextureViewDimension::D3, false), tex_entry(10, wgpu::TextureViewDimension::D3, false), tex_entry(12, d2, false),
            hiz_ro,
        ]);
        let l_hiz = layout("hiz", &[
            tex_entry(0, d2, true),
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::R32Float, view_dimension: d2 },
                count: None,
            },
        ]);
        let l_comp_edge = layout("comp edge", &[
            tex_entry(0, d2, false), tex_entry(1, d2, false), tex_entry(2, d2, true), tex_entry(3, d2, false),
            ms_entry(5, false), ms_entry(6, false), ms_entry(7, true), ms_entry(8, false), tex_entry(12, d2, false),
        ]);
        let l_comp_globe = layout("comp globe", &[
            tex_entry(0, d2, false), tex_entry(1, d2, false), tex_entry(2, d2, true), tex_entry(3, d2, false), tex_entry(4, d2, false),
            tex_entry(12, d2, false),
        ]);
        let stex_dim = |binding: u32, dim: wgpu::TextureViewDimension| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: HDR, view_dimension: dim },
            count: None,
        };
        let l_atmlut = layout("atmlut", &[
            stex_dim(0, wgpu::TextureViewDimension::D3),
            stex_dim(1, wgpu::TextureViewDimension::D3),
            stex_dim(2, wgpu::TextureViewDimension::D2Array),
            stex_dim(3, d2),
        ]);
        let bg_atmlut = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atmlut"),
            layout: &l_atmlut,
            entries: &[tex_res(0, &ap_l), tex_res(1, &ap_t), tex_res(2, &sky_frustum), tex_res(3, &cloud_refl)],
        });
        let bg_comp_sky = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("comp sky"),
            layout: &l_comp_sky,
            entries: &[tex_res(9, &ap_l), tex_res(10, &ap_t), tex_res(11, &sky_frustum)],
        });
        let m_hiz = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("hiz"), source: wgpu::ShaderSource::Wgsl(include_str!("shaders/hiz.wgsl").into()) });
        let pl_hiz = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l_hiz)], immediate_size: 0 });
        let p_hiz_build = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("hiz build"), layout: Some(&pl_hiz), module: &m_hiz, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
        let l_ao = layout("ao", &[tex_entry(0, d2, true), tex_entry(1, d2, false), tex_entry(2, d2, false)]);
        let utex_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: ALL_STAGES,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint, view_dimension: d2, multisampled: false },
            count: None,
        };
        let l_cloud = layout("cloud", &[tex_entry(0, d2, true)]);
        let l_cloud_storm = layout("cloud storm", &[tex_entry(0, d2, true), utex_entry(6)]);
        let l_cloud_rain = layout("cloud rain", &[tex_entry(0, d2, true), tex_entry(2, d2, false), tex_entry(3, d2, false), utex_entry(6)]);
        let l_cloud_apply = layout("cloud apply", &[tex_entry(0, d2, true), tex_entry(1, d2, false), tex_entry(3, d2, false), tex_entry(5, d2, false)]);
        let l_cloud_temporal = layout("cloud temporal", &[tex_entry(2, d2, false), tex_entry(3, d2, false), tex_entry(4, d2, false)]);
        let l_post = layout("post", &[tex_entry(0, d2, false), tex_entry(1, d2, false), tex_entry(2, d2, true)]);
        let storage = |ro: bool| wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: if ro { wgpu::ShaderStages::VERTEX } else { wgpu::ShaderStages::COMPUTE },
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: ro }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let psim_size = std::mem::size_of::<PSim>() as u64;
        let l_psim = layout("psim", &[ub_entry(0, false, psim_size), storage(false)]);
        let l_pdraw = layout("pdraw", &[ub_entry(0, false, psim_size), storage(true), tex_entry(2, d2, true)]);
        let cbuf = |binding: u32, ro: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: ro }, has_dynamic_offset: false, min_binding_size: None },
            count: None,
        };
        let stex = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: HDR, view_dimension: wgpu::TextureViewDimension::D3 },
            count: None,
        };
        let l_vol_a = layout("smoke splat", &[ub_entry(0, false, psim_size), cbuf(1, true), cbuf(2, false), cbuf(3, false), cbuf(4, false), stex(5)]);
        let l_vol_b = layout("smoke light", &[ub_entry(0, false, psim_size), tex_entry(6, wgpu::TextureViewDimension::D3, false), stex(7)]);

        let pl = |l1: &wgpu::BindGroupLayout| d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l0), Some(l1)], immediate_size: 0 });
        let pl0 = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l0)], immediate_size: 0 });

        // ---- pipelines (built in parallel across threads to cut startup wait)
        let full = |label: &str, layout: &wgpu::PipelineLayout, m: &wgpu::ShaderModule, fs: &str, format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState { module: m, entry_point: Some("vs_full"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: m, entry_point: Some(fs), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })] }),
                multiview_mask: None,
                cache: None,
            })
        };
        let l_csmix = layout("cloud shadow mix", &[tex_entry(0, wgpu::TextureViewDimension::D2Array, false), ub_entry(1, false, 48)]);
        let mix_ub = d.create_buffer(&wgpu::BufferDescriptor { label: Some("cloud shadow mix"), size: 48, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let bg_mix = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cloud shadow mix"),
            layout: &l_csmix,
            entries: &[tex_res(0, &cs_all), wgpu::BindGroupEntry { binding: 1, resource: mix_ub.as_entire_binding() }],
        });
        let m_csmix = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("csmix"), source: wgpu::ShaderSource::Wgsl(include_str!("shaders/csmix.wgsl").into()) });
        let pl_csmix = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l_csmix)], immediate_size: 0 });
        let (pl_va, pl_vb) = (pl(&l_vol_a), pl(&l_vol_b));
        let pl_obj = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l0), Some(&l_obj)], immediate_size: 0 });

        // Geometry pipelines. Mode 0: lit colour + aux into the multisampled G-buffer.
        // Mode 1: depth only into a shadow cascade. Mode 2: blended decal (roads).
        // Mode 3: as 0, with the leaf card entry points. Mode 4: as 1, with a
        // fragment stage that can cut holes (the aircraft's windows).
        // Mode 7: as 0, with the entry points for trees near the camera.
        // Mode 10: as 1, drawing each tree with the model it is given.
        // Modes 12 and 13: as 1 and 4, only the faces turned away from the sun
        // (the 3D tiles: their ground then casts nothing, sparing the fill of
        // a whole layer of ground per cascade, and walls cast from the back).
        let geom = |label: &str, l1: Option<&wgpu::BindGroupLayout>, m: &wgpu::ShaderModule, buffers: &[Option<wgpu::VertexBufferLayout>], mode: u8| geom_pipeline(d, &l0, &l_casc, label, l1, m, buffers, mode);
        use wgpu::VertexFormat as VF;
        let vb = |stride: u64, instance: bool, attributes: &'static [wgpu::VertexAttribute]| {
            Some(wgpu::VertexBufferLayout { array_stride: stride, step_mode: if instance { wgpu::VertexStepMode::Instance } else { wgpu::VertexStepMode::Vertex }, attributes })
        };
        const A_TERRAIN: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x3, 3 => Float32, 4 => Float32x2];
        const A_BUILD: [wgpu::VertexAttribute; 5] = [
            wgpu::VertexAttribute { format: VF::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: VF::Snorm8x4, offset: 12, shader_location: 1 },
            wgpu::VertexAttribute { format: VF::Float32x2, offset: 16, shader_location: 2 },
            wgpu::VertexAttribute { format: VF::Unorm8x4, offset: 24, shader_location: 3 },
            wgpu::VertexAttribute { format: VF::Uint8x4, offset: 28, shader_location: 4 },
        ];
        const A_MESH: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];
        const A_ROAD: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32];
        const A_TREEV: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 6 => Float32x4, 7 => Float32x4];
        const A_TREE: [wgpu::VertexAttribute; 3] = [
            wgpu::VertexAttribute { format: VF::Float32x4, offset: 0, shader_location: 3 },
            wgpu::VertexAttribute { format: VF::Float32x4, offset: 16, shader_location: 4 },
            wgpu::VertexAttribute { format: VF::Unorm8x4, offset: 32, shader_location: 5 },
        ];
        const A_INST: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4];
        const A_PHOTO: [wgpu::VertexAttribute; 3] = [
            wgpu::VertexAttribute { format: VF::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: VF::Float32x2, offset: 12, shader_location: 1 },
            wgpu::VertexAttribute { format: VF::Snorm8x4, offset: 20, shader_location: 2 },
        ];
        const A_SKIN: [wgpu::VertexAttribute; 4] = [
            wgpu::VertexAttribute { format: VF::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: VF::Float32x2, offset: 12, shader_location: 1 },
            wgpu::VertexAttribute { format: VF::Snorm8x4, offset: 20, shader_location: 2 },
            wgpu::VertexAttribute { format: VF::Uint32, offset: 24, shader_location: 3 },
        ];
        const A_AIR: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x4, 3 => Float32, 4 => Float32x2];
        const A_LIGHT: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x3, 3 => Float32, 4 => Unorm8x4, 5 => Uint8x4];
        const A_BOLT: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4];

        crate::progress::step("shaders");
        let (
            (p_cshadow_in, p_cshadow_out, p_cshadow_in_plain, p_cshadow_out_plain, p_cshadow_mix, (p_ap_build, p_sky_build, p_cloud_refl, p_cloud_refl_plain), (p_comp_cls, p_comp_sky, p_comp_opaque, p_comp_glass, p_comp_water, p_comp_edge, p_composite, p_composite_edge), p_cloud, p_cloud_plain, p_cloud_classify, p_cloud_plain_cls, p_cloud_storm_cls, p_cloud_rain, p_cloud_temporal, p_cloud_apply),
            (p_terrain, p_terrain_far, p_terrain_water, p_terrain_tess, p_terrain_cells, p_terrain_photo, p_terrain_tess_photo, p_terrain_cells_photo, p_build, p_build_whole, p_build_shadow, p_photo, p_skin, p_photo_shadow, p_photo_shadow_fast, p_photo_mask, p_water_top, p_shore_cover),
            (p_road, p_field, p_lights, p_lamp_top, p_rain, p_bolt, p_veil, p_contrails, p_tree, imp, p_tree_near, p_tree_shadow, p_tree_shadow_fixed, p_card, p_inst, p_inst_shadow, p_scenery, p_scenery_shadow),
            (p_trans, p_ms, p_sky, p_gtao, p_ao_blur, p_taa, p_down, p_up, p_final, p_final_taa, p_blit, p_particles, p_psim, p_vol, p_dres),
        ) = std::thread::scope(|s| {

            let t_cloud = s.spawn(|| {
                let p_cshadow_in = full("cloud shadow", &pl0, &m_cloud, "fs_cshadow_in", CS_FMT, None);
                let p_cshadow_out = full("cloud shadow", &pl0, &m_cloud, "fs_cshadow_out", CS_FMT, None);
                let p_cshadow_in_plain = full("cloud shadow plain", &pl0, &m_cloud, "fs_cshadow_in_plain", CS_FMT, None);
                let p_cshadow_out_plain = full("cloud shadow plain", &pl0, &m_cloud, "fs_cshadow_out_plain", CS_FMT, None);
                let p_cshadow_mix = [full("cloud shadow mix", &pl_csmix, &m_csmix, "fs_in", CS_FMT, None), full("cloud shadow mix", &pl_csmix, &m_csmix, "fs_out", CS_FMT, None)];
                let pl_atmlut = pl(&l_atmlut);
                let mk_atmlut = |label: &str, entry: &str| {
                    d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(label), layout: Some(&pl_atmlut), module: &m_atmlut, entry_point: Some(entry), compilation_options: Default::default(), cache: None })
                };
                let p_ap_build = mk_atmlut("ap build", "ap_build");
                let p_sky_build = mk_atmlut("sky build", "sky_build");
                let p_cloud_refl = mk_atmlut("cloud refl", "cloud_refl_build");
                let p_cloud_refl_plain = mk_atmlut("cloud refl plain", "cloud_refl_plain_build");
                let p_comp_cls = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("composite classify"),
                    layout: Some(&pl(&l_comp_cls)),
                    vertex: wgpu::VertexState { module: &m_comp, entry_point: Some("vs_full"), compilation_options: Default::default(), buffers: &[] },
                    primitive: Default::default(),
                    depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth16Unorm, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Always), stencil: Default::default(), bias: Default::default() }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState { module: &m_comp, entry_point: Some("fs_classify"), compilation_options: Default::default(), targets: &[] }),
                    multiview_mask: None,
                    cache: None,
                });
                let comp_blend = wgpu::BlendState {
                    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
                    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
                };
                let mk_comp = |label: &str, l1: &wgpu::BindGroupLayout, fs: &str, cmp: wgpu::CompareFunction, blend: Option<wgpu::BlendState>| {
                    d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some(label),
                        layout: Some(&pl(l1)),
                        vertex: wgpu::VertexState { module: &m_comp, entry_point: Some("vs_cls"), compilation_options: Default::default(), buffers: &[] },
                        primitive: Default::default(),
                        depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth16Unorm, depth_write_enabled: Some(false), depth_compare: Some(cmp), stencil: Default::default(), bias: Default::default() }),
                        multisample: Default::default(),
                        fragment: Some(wgpu::FragmentState { module: &m_comp, entry_point: Some(fs), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: HDR, blend, write_mask: wgpu::ColorWrites::ALL })] }),
                        multiview_mask: None,
                        cache: None,
                    })
                };
                let p_comp_sky = mk_comp("comp sky", &l_comp_sky, "fs_sky", wgpu::CompareFunction::Less, None);
                let p_comp_opaque = mk_comp("comp opaque", &l_comp_main, "fs_opaque", wgpu::CompareFunction::Equal, Some(comp_blend));
                let p_comp_glass = mk_comp("comp glass", &l_comp_main, "fs_glass", wgpu::CompareFunction::Equal, Some(comp_blend));
                let p_comp_water = mk_comp("comp water", &l_comp_main, "fs_water", wgpu::CompareFunction::Equal, None);
                let p_comp_edge = (msaa() > 1).then(|| mk_comp("comp edge", &l_comp_edge, "fs_water_edge", wgpu::CompareFunction::Equal, None));
                let p_composite = full("composite", &pl(&l_comp_globe), &m_comp, "fs", HDR, None);
                // (not built at one sample: unused there, and Intel's compiler took minutes over it)
                let p_composite_edge = (msaa() > 1).then(|| full("composite edge", &pl(&l_comp_edge), &m_comp, "fs_edge", HDR, None));
                let mk_cloud = |label: &str, l1: &wgpu::BindGroupLayout, vs: &str, entry: &str, depth: Option<wgpu::CompareFunction>| {
                    d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some(label),
                        layout: Some(&pl(l1)),
                        vertex: wgpu::VertexState { module: &m_cloud, entry_point: Some(vs), compilation_options: Default::default(), buffers: &[] },
                        primitive: Default::default(),
                        depth_stencil: depth.map(|cmp| wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth16Unorm, depth_write_enabled: Some(false), depth_compare: Some(cmp), stencil: Default::default(), bias: Default::default() }),
                        multisample: Default::default(),
                        fragment: Some(wgpu::FragmentState {
                            module: &m_cloud,
                            entry_point: Some(entry),
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState { format: HDR, blend: None, write_mask: wgpu::ColorWrites::ALL }), Some(wgpu::ColorTargetState { format: CLOUD_DIST, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                        }),
                        multiview_mask: None,
                        cache: None,
                    })
                };
                let p_cloud = mk_cloud("cloud", &l_cloud, "vs_full", "fs", None);
                let p_cloud_plain = mk_cloud("cloud plain", &l_cloud, "vs_full", "fs_plain", None);
                let p_cloud_classify = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("cloud classify"),
                    layout: Some(&pl(&l_cloud)),
                    vertex: wgpu::VertexState { module: &m_cloud, entry_point: Some("vs_full"), compilation_options: Default::default(), buffers: &[] },
                    primitive: Default::default(),
                    depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth16Unorm, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Always), stencil: Default::default(), bias: Default::default() }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &m_cloud,
                        entry_point: Some("fs_classify"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba32Uint, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                    }),
                    multiview_mask: None,
                    cache: None,
                });
                let p_cloud_plain_cls = mk_cloud("cloud plain comp", &l_cloud_rain, "vs_full", "fs_plain_comp", None);
                let p_cloud_storm_cls = mk_cloud("cloud storm cls", &l_cloud_storm, "vs_full_z05", "fs_storm", Some(wgpu::CompareFunction::Less));
                let p_cloud_rain = mk_cloud("cloud rain", &l_cloud_rain, "vs_full", "fs_storm_rain", None);
                let p_cloud_temporal = full("cloud temporal", &pl(&l_cloud_temporal), &m_cloud, "fs_temporal", HDR, None);
                let over = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::SrcAlpha, operation: wgpu::BlendOperation::Add };
                let p_cloud_apply = full("cloud apply", &pl(&l_cloud_apply), &m_cloud, "fs_apply", HDR, Some(wgpu::BlendState { color: over, alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add } }));
                crate::progress::step("shaders-part");
                (p_cshadow_in, p_cshadow_out, p_cshadow_in_plain, p_cshadow_out_plain, p_cshadow_mix, (p_ap_build, p_sky_build, p_cloud_refl, p_cloud_refl_plain), (p_comp_cls, p_comp_sky, p_comp_opaque, p_comp_glass, p_comp_water, p_comp_edge, p_composite, p_composite_edge), p_cloud, p_cloud_plain, p_cloud_classify, p_cloud_plain_cls, p_cloud_storm_cls, p_cloud_rain, p_cloud_temporal, p_cloud_apply)
            });

            let t_world = s.spawn(|| {
                let p_terrain = geom("terrain", Some(&l_tile), &m_terrain, &[vb(40, false, &A_TERRAIN)], 15);
                let p_terrain_far = geom("terrain far", Some(&l_tile), &m_terrain, &[vb(40, false, &A_TERRAIN)], 19);
                let p_terrain_water = geom("terrain water", Some(&l_tile), &m_terrain, &[vb(40, false, &A_TERRAIN)], 20);
                let tess_layout = wgpu::VertexBufferLayout { array_stride: 4, step_mode: wgpu::VertexStepMode::Instance, attributes: &wgpu::vertex_attr_array![0 => Uint32] };
                let p_terrain_tess = geom("terrain tess", Some(&l_tile), &m_terrain, &[Some(tess_layout.clone())], 8);
                let p_terrain_cells = geom("terrain cells", Some(&l_tile), &m_terrain, &[], 9);
                let p_terrain_photo = geom("terrain photo", Some(&l_tile), &m_terrain, &[vb(40, false, &A_TERRAIN)], 16);
                let p_terrain_tess_photo = geom("terrain tess photo", Some(&l_tile), &m_terrain, &[Some(tess_layout)], 17);
                let p_terrain_cells_photo = geom("terrain cells photo", Some(&l_tile), &m_terrain, &[], 18);
                let p_build = geom("build", Some(&l_build), &m_build, &[vb(std::mem::size_of::<crate::buildings::BVertex>() as u64, false, &A_BUILD)], 0);
                let p_build_whole = geom("build whole", Some(&l_build), &m_build, &[vb(std::mem::size_of::<crate::buildings::BVertex>() as u64, false, &A_BUILD)], 14);
                let p_build_shadow = geom("build shadow", Some(&l_build), &m_build, &[vb(std::mem::size_of::<crate::buildings::BVertex>() as u64, false, &A_BUILD)], 1);
                let p_photo = geom("photo", Some(&l_photo), &m_photo, &[vb(24, false, &A_PHOTO)], 0);
                let p_skin = geom("skin", Some(&l_skin), &m_photo, &[vb(28, false, &A_SKIN)], 11);
                let p_photo_shadow = geom("photo shadow", Some(&l_photo), &m_photo, &[vb(24, false, &A_PHOTO)], 13);
                let p_photo_shadow_fast = geom("photo shadow fast", Some(&l_photo), &m_photo, &[vb(24, false, &A_PHOTO)], 12);
                let p_photo_mask = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("photo mask"),
                    layout: Some(&d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l0), Some(&l_photo), Some(&l_casc)], immediate_size: 0 })),
                    vertex: wgpu::VertexState { module: &m_photo, entry_point: Some("vs_mask"), compilation_options: Default::default(), buffers: &[vb(24, false, &A_PHOTO)] },
                    primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState { module: &m_photo, entry_point: Some("fs_mask"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: PHOTO_MASK_FMT, blend: None, write_mask: wgpu::ColorWrites::RED })] }),
                    multiview_mask: None,
                    cache: None,
                });
                let p_water_top = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("water top"),
                    layout: Some(&d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l0), Some(&l_tile), Some(&l_casc)], immediate_size: 0 })),
                    vertex: wgpu::VertexState { module: &m_terrain, entry_point: Some("vs_top"), compilation_options: Default::default(), buffers: &[vb(40, false, &A_TERRAIN)] },
                    primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState { module: &m_terrain, entry_point: Some("fs_top"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: PHOTO_MASK_FMT, blend: None, write_mask: wgpu::ColorWrites::GREEN })] }),
                    multiview_mask: None,
                    cache: None,
                });
                let p_shore_cover = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("shore cover"),
                    layout: Some(&d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l0), Some(&l_tile), Some(&l_casc)], immediate_size: 0 })),
                    vertex: wgpu::VertexState { module: &m_terrain, entry_point: Some("vs_top"), compilation_options: Default::default(), buffers: &[vb(40, false, &A_TERRAIN)] },
                    primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState { module: &m_terrain, entry_point: Some("fs_cover"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: PHOTO_MASK_FMT, blend: None, write_mask: wgpu::ColorWrites::RED })] }),
                    multiview_mask: None,
                    cache: None,
                });
                crate::progress::step("shaders-part");
                (p_terrain, p_terrain_far, p_terrain_water, p_terrain_tess, p_terrain_cells, p_terrain_photo, p_terrain_tess_photo, p_terrain_cells_photo, p_build, p_build_whole, p_build_shadow, p_photo, p_skin, p_photo_shadow, p_photo_shadow_fast, p_photo_mask, p_water_top, p_shore_cover)
            });

            let t_misc = s.spawn(|| {
                let p_road = geom("road", Some(&l_tileu), &m_road, &[vb(32, false, &A_ROAD)], 2);
                let p_field = geom("airfield", Some(&l_tileu), &m_field, &[vb(56, false, &A_AIR)], 2);
                let p_lights = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("lights"),
                    layout: Some(&d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l0), Some(&l_tileu)], immediate_size: 0 })),
                    vertex: wgpu::VertexState { module: &m_lights, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[vb(40, true, &A_LIGHT)] },
                    primitive: Default::default(),
                    depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::Greater), stencil: Default::default(), bias: Default::default() }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &m_lights,
                        entry_point: Some("fs"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: HDR,
                            blend: Some(wgpu::BlendState {
                                color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
                                alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                });
                let max = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Max };
                let p_lamp_top = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("lamp top"),
                    layout: Some(&d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l0), Some(&l_tileu), Some(&l_casc)], immediate_size: 0 })),
                    vertex: wgpu::VertexState { module: &m_road, entry_point: Some("vs_top"), compilation_options: Default::default(), buffers: &[vb(32, false, &A_ROAD)] },
                    primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState { module: &m_road, entry_point: Some("fs_top"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: PHOTO_MASK_FMT, blend: Some(wgpu::BlendState { color: max, alpha: max }), write_mask: wgpu::ColorWrites::BLUE | wgpu::ColorWrites::ALPHA })] }),
                    multiview_mask: None,
                    cache: None,
                });
                let precip = |label: &str, vs: &str, fs: &str, color: wgpu::BlendComponent, alpha: wgpu::BlendComponent, buffers: &[Option<wgpu::VertexBufferLayout>]| {
                    d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some(label),
                        layout: Some(&pl(&l_cloud_apply)),
                        vertex: wgpu::VertexState { module: &m_precip, entry_point: Some(vs), compilation_options: Default::default(), buffers },
                        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                        depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::Greater), stencil: Default::default(), bias: Default::default() }),
                        multisample: Default::default(),
                        fragment: Some(wgpu::FragmentState {
                            module: &m_precip,
                            entry_point: Some(fs),
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: HDR,
                                blend: Some(wgpu::BlendState { color, alpha }),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                        }),
                        multiview_mask: None,
                        cache: None,
                    })
                };
                let cover = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
                let over = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
                let p_rain = precip("rain", "vs_rain", "fs_rain", over, cover, &[]);
                let p_bolt = precip("lightning", "vs_bolt", "fs_bolt", wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add }, cover, &[vb(32, true, &A_BOLT)]);
                let p_veil = precip("rain veil", "vs_veil", "fs_veil", over, wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add }, &[]);
                let p_contrails = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("contrails"),
                    layout: Some(&d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: None,
                        bind_group_layouts: &[Some(&l0), Some(&l_cloud_apply), Some(&l_contrails)],
                        immediate_size: 0,
                    })),
                    vertex: wgpu::VertexState { module: &m_contrails, entry_point: Some("vs_contrail"), compilation_options: Default::default(), buffers: &[] },
                    primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                    depth_stencil: None,
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &m_contrails,
                        entry_point: Some("fs_contrail"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: HDR,
                            blend: Some(wgpu::BlendState {
                                color: over,
                                alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                });
                let p_tree = geom("tree", Some(&l_tileu), &m_tree, &[vb(72, false, &A_TREEV), vb(36, true, &A_TREE)], 0);
                let imp = crate::impostor::Impostors::new(d, &l0, &l_tileu, &m_tree, if integrated { crate::impostor::CELL_INTEGRATED } else { crate::impostor::CELL });
                let p_tree_near = geom("trees near", Some(&l_tileu), &m_tree, &[vb(72, false, &A_TREEV), vb(36, true, &A_TREE)], 7);
                let p_tree_shadow = geom("tree shadow", Some(&l_tileu), &m_tree, &[vb(72, false, &A_TREEV), vb(36, true, &A_TREE)], 21);
                let p_tree_shadow_fixed = geom("tree shadow fixed", Some(&l_tileu), &m_tree, &[vb(72, false, &A_TREEV), vb(36, true, &A_TREE)], 22);
                let p_card = geom("leaf cards", Some(&l_tileu), &m_tree, &[vb(72, false, &A_TREEV), vb(36, true, &A_TREE)], 3);
                let p_inst = geom("inst", None, &m_inst, &[vb(40, false, &A_MESH), vb(80, true, &A_INST)], 0);
                let p_inst_shadow = geom("inst shadow", None, &m_inst, &[vb(40, false, &A_MESH), vb(80, true, &A_INST)], 1);
                // Scenery vertex: position, normal and tangent (signed bytes), uv (m), material.
                const A_SCENERY: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Snorm8x4, 2 => Snorm8x4, 8 => Float32x2, 9 => Uint32];
                let m_scenery = module("scenery", include_str!("shaders/scenery.wgsl"));
                let p_scenery = geom("scenery", Some(&l_scenery), &m_scenery, &[vb(32, false, &A_SCENERY), vb(80, true, &A_INST)], 0);
                // (mode 4: the shadow with its fragment stage, for the fence's cut-out wires)
                let p_scenery_shadow = geom("scenery shadow", Some(&l_scenery), &m_scenery, &[vb(32, false, &A_SCENERY), vb(80, true, &A_INST)], 4);
                crate::progress::step("shaders-part");
                (p_road, p_field, p_lights, p_lamp_top, p_rain, p_bolt, p_veil, p_contrails, p_tree, imp, p_tree_near, p_tree_shadow, p_tree_shadow_fixed, p_card, p_inst, p_inst_shadow, p_scenery, p_scenery_shadow)
            });

            let p_trans = full("trans", &pl0, &m_lut, "fs_trans", HDR, None);
            let p_ms = full("ms", &pl0, &m_lut, "fs_ms", HDR, None);
            let p_sky = full("sky", &pl0, &m_lut, "fs_sky", HDR, None);
            let p_gtao = full("gtao", &pl(&l_ao), &m_ao, "fs_gtao", AMB, None);
            let p_ao_blur = full("ao blur", &pl(&l_ao), &m_ao, "fs_blur", AMB, None);
            let p_taa = full("taa", &pl(&l_post), &m_taa, "fs_taa", HDR, None);
            let p_down = full("down", &pl(&l_post), &m_post, "fs_down", HDR, None);
            let add = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
            let p_up = full("up", &pl(&l_post), &m_post, "fs_up", HDR, Some(wgpu::BlendState { color: add, alpha: add }));
            let p_final = full("final", &pl(&l_post), &m_post, "fs_final", out_format(&config), None);
            let p_final_taa = full("final taa", &pl(&l_post), &m_post, "fs_final_taa", out_format(&config), None);
            let p_blit = full("blit", &pl(&l_post), &m_post, if config.format.is_srgb() { "fs_blit_srgb" } else { "fs_blit" }, config.format, None);
            let pm = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add };
            let p_particles = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("particles"),
                layout: Some(&pl(&l_pdraw)),
                vertex: wgpu::VertexState { module: &m_pdraw, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: &m_pdraw, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: HDR, blend: Some(wgpu::BlendState { color: pm, alpha: pm }), write_mask: wgpu::ColorWrites::ALL })] }),
                multiview_mask: None,
                cache: None,
            });
            let p_psim = d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("psim"), layout: Some(&pl(&l_psim)), module: &m_psim, entry_point: Some("main"), compilation_options: Default::default(), cache: None });
            let p_vol = [("splat", &pl_va), ("blur_x", &pl_va), ("blur_y", &pl_va), ("blur_z", &pl_va), ("light", &pl_vb)]
                .map(|(e, l)| d.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some(e), layout: Some(l), module: &m_vol, entry_point: Some(e), compilation_options: Default::default(), cache: None }));
            let p_dres = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("depth resolve"),
                layout: Some(&d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&l_dres)], immediate_size: 0 })),
                vertex: wgpu::VertexState { module: &m_dres, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::Always), stencil: Default::default(), bias: Default::default() }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: &m_dres, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[] }),
                multiview_mask: None,
                cache: None,
            });
            crate::progress::step("shaders-part");
            (
                t_cloud.join().unwrap(),
                t_world.join().unwrap(),
                t_misc.join().unwrap(),
                (p_trans, p_ms, p_sky, p_gtao, p_ao_blur, p_taa, p_down, p_up, p_final, p_final_taa, p_blit, p_particles, p_psim, p_vol, p_dres),
            )
        });
        crate::progress::step("shaders-done");
        let obj_gen_rx = {
            let (tx, rx) = crossbeam_channel::bounded(2);
            let (d, l0, l_obj, l_casc, l_glass, m) = (d.clone(), l0.clone(), l_obj.clone(), l_casc.clone(), l_glass.clone(), m_obj.clone());
            // (a roomy stack, as for `want_obj_kind`)
            let spawned = std::thread::Builder::new().name("obj pipelines".into()).stack_size(16 << 20).spawn(move || {
                // After the kind flown has its own set (it draws the aircraft),
                // or once nothing has asked for one in 15 s (the start screen).
                let t = std::time::Instant::now();
                use std::sync::atomic::Ordering::Relaxed;
                while FAST_STATE.load(Relaxed) == 1 || (FAST_STATE.load(Relaxed) == 0 && t.elapsed().as_secs() < 15) {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                let t = std::time::Instant::now();
                let draw = {
                    let _big = BIG_COMPILE.lock().unwrap_or_else(|e| e.into_inner());
                    obj_generic_draw(&d, &l0, &l_obj, &l_casc, &m)
                };
                log::info!("aircraft draw pipelines ready in {:.1} s", t.elapsed().as_secs_f64());
                if tx.send(ObjGenMsg::Draw(draw.0, draw.1)).is_err() {
                    return;
                }
                // (the lock let go between the two, so a specialised set asked
                // for meanwhile is not kept waiting behind the shadows)
                while FAST_STATE.load(Relaxed) == 1 {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                let rest = {
                    let _big = BIG_COMPILE.lock().unwrap_or_else(|e| e.into_inner());
                    obj_generic_rest(&d, &l0, &l_obj, &l_casc, &l_glass, &m)
                };
                log::info!("aircraft pipelines ready in {:.1} s", t.elapsed().as_secs_f64());
                let _ = tx.send(ObjGenMsg::Rest(rest));
            });
            if let Err(e) = spawned {
                log::error!("aircraft pipeline thread: {e}");
            }
            rx
        };

        let cshadow = CloudShadow {
            p_in: p_cshadow_in,
            p_out: p_cshadow_out,
            p_in_plain: p_cshadow_in_plain,
            p_out_plain: p_cshadow_out_plain,
            layers: cs_layers,
            p_mix: p_cshadow_mix,
            bg_mix,
            mix_ub,
            mix_layers: cs_mix_layers,
            prev: [[0.0; 4]; CS_MAP],
            front: [[0.0; 4]; CS_MAP],
            back: [[0.0; 4]; CS_MAP],
            slot: [0, 0, 1],
            slice: 0,
            slices: if integrated { CS_SLICES_INTEGRATED } else { CS_SLICES },
            ok: false,
            anchor: [glam::DVec3::Z, glam::DVec3::X, glam::DVec3::Y],
            sun: glam::DVec3::Z,
            sun_v: glam::DVec3::ZERO,
            mix: ([0; 8], 1.0),
        };

        // ---- per-draw uniform pools
        let pool = |label: &str, n: u64| d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size: n * UB_ALIGN, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let (tile_ub, build_ub, obj_ub, casc_ub) = (pool("tiles", MAX_TILES), pool("builds", MAX_BUILDS + 1 + MAX_FIELDS as u64), pool("objs", MAX_OBJS * OBJ_STRIDE / UB_ALIGN), pool("cascades", CASCADES as u64 + 4));
        let photo_ub = pool("photo tiles", MAX_PHOTO);
        let bg_tileu = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &l_tileu, entries: &[buf_res(0, &build_ub, 48)] });
        let bg_casc = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &l_casc, entries: &[buf_res(0, &casc_ub, 64)] });
        let dynbuf = |label: &str, size: u64| d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let (inst_buf, fall_buf) = (dynbuf("instances", MAX_INST as u64 * 80), dynbuf("loose trees", MAX_FALL as u64 * 36));
        let near_buf = dynbuf("near trees", MAX_NEAR as u64 * 36);
        let scenery_buf = dynbuf("scenery instances", MAX_SCENERY as u64 * 80);
        let dyn_lights = dynbuf("aircraft lights", MAX_DYN_LIGHTS as u64 * 40);
        let bolt_buf = dynbuf("lightning", crate::storm::MAX_SEGS as u64 * 32);
        let contrail_nodes = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("contrail nodes"),
            size: (crate::contrails::MAX_NODES * std::mem::size_of::<crate::contrails::GpuNode>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let contrail_segs = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("contrail segs"),
            size: (crate::contrails::MAX_SEGS * std::mem::size_of::<crate::contrails::GpuSeg>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg_contrails = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("contrails"),
            layout: &l_contrails,
            entries: &[buf_res(0, &contrail_nodes, 0), buf_res(1, &contrail_segs, 0)],
        });
        // (No paint until the aircraft is known: `set_paint`.)
        let paint = paint_texture(d, &queue, None);
        let paint_ub = d.create_buffer(&wgpu::BufferDescriptor { label: Some("paint"), size: PAINT_UB, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let bg_obj = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &l_obj, entries: &[buf_res(0, &obj_ub, std::mem::size_of::<ObjU>() as u64), tex_res(1, &paint), buf_res(2, &paint_ub, PAINT_UB), buf_res(3, &font_ub, font_ub.size())] });
        let ix = crate::sources::tile_indices();
        let tile_ibuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("tile idx"), contents: bytemuck::cast_slice(&ix), usage: wgpu::BufferUsages::INDEX });
        let patch = |n: u16| {
            let mut v = Vec::new();
            for b in 0..n {
                for a in 0..n {
                    let (p00, p10, p01, p11) = (b * (n + 1) + a, b * (n + 1) + a + 1, (b + 1) * (n + 1) + a, (b + 1) * (n + 1) + a + 1);
                    v.extend([p00, p01, p10, p10, p01, p11]);
                }
            }
            v
        };
        let (mut tix, mut tess_idx) = (Vec::new(), std::array::from_fn(|_| 0..0));
        for (k, r) in tess_idx.iter_mut().enumerate() {
            let p = patch(1 << k);
            *r = tix.len() as u32..(tix.len() + p.len()) as u32;
            tix.extend(p);
        }
        debug_assert_eq!(1 << (tess_idx.len() - 1), TESS_N);
        let cix = patch(1);
        let (tess_cap, tess_buf) = (4096, dynbuf("tess cells", 4096 * 4));
        let tess_ibuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("tess idx"), contents: bytemuck::cast_slice(&tix), usage: wgpu::BufferUsages::INDEX });
        let cell_ibuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("cell idx"), contents: bytemuck::cast_slice(&cix), usage: wgpu::BufferUsages::INDEX });

        // ---- particles
        let psim_ub = d.create_buffer(&wgpu::BufferDescriptor { label: Some("psim"), size: psim_size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let pbuf = d.create_buffer(&wgpu::BufferDescriptor { label: Some("particles"), size: MAX_PARTICLES as u64 * 64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let bg_psim = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &l_psim, entries: &[buf_res(0, &psim_ub, 0), buf_res(1, &pbuf, 0)] });
        let sbuf = |label: &str, size: u64| d.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let (vol_acc, vol_a, vol_b) = (sbuf("smoke splat", VOL_CELLS * 8), sbuf("smoke blur a", VOL_CELLS * 8), sbuf("smoke blur b", VOL_CELLS * 8));
        let bg_vol_a = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &l_vol_a,
            entries: &[buf_res(0, &psim_ub, 0), buf_res(1, &pbuf, 0), buf_res(2, &vol_acc, 0), buf_res(3, &vol_a, 0), buf_res(4, &vol_b, 0), tex_res(5, &vol_dens)],
        });
        let bg_vol_b = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &l_vol_b, entries: &[buf_res(0, &psim_ub, 0), tex_res(6, &vol_dens), tex_res(7, &vol_light)] });
        let fluid = crate::fluid::FluidGpu::new(d, &l0, common, &vol_acc);

        let egui = egui_wgpu::Renderer::new(d, out_format(&config), egui_wgpu::RendererOptions::default());
        let mut up = crate::upscale::Upscale::new(d, &pl(&l_post), &m_post, out_format(&config));
        up.fsr4_device = fsr4_device;
        let targets = Self::make_targets(d, &config, (config.width, config.height), &l_dres, &l_ao, &l_hiz, &l_comp_cls, &l_comp_main, &l_comp_edge, &l_comp_globe, &ap_l, &ap_t, &cloud_refl, &l_cloud, &l_cloud_storm, &l_cloud_rain, &l_cloud_apply, &l_cloud_temporal, &l_post, &l_pdraw, &l_glass, &psim_ub, &pbuf, &black);
        let fluid_t = fluid.draw_targets(d, &targets.depth, config.width, config.height);
        // The transmittance and multi-scatter tables depend only on the planet
        // and the haze: bake them now, and again whenever the haze changes.
        let lut_bake = LutBake { p_trans, p_ms, bg_trans: bg0_boot, bg_ms: bg0_ms, trans: lut_trans, ms: lut_ms };
        {
            let mut g = Globals::zeroed();
            g.planet = [0.0, 0.0, 0.0, crate::geo::R as f32];
            g.weather[3] = 1.0;
            queue.write_buffer(&globals_ub, 0, bytemuck::bytes_of(&g));
            let mut enc = d.create_command_encoder(&Default::default());
            lut_bake.draw(&mut enc);
            queue.submit([enc.finish()]);
        }
        if let Some(err) = gpu_error.lock().clone() {
            anyhow::bail!("GPU initialisation error: {err}");
        }
        Ok(Self {
            ground_mats,
            casc_ub, bg_casc, bg_tileu, l_dres, l_ao, p_gtao, p_ao_blur, p_dres, p_road, p_field, p_lights, dyn_lights, p_rain, p_veil, p_bolt, bolt_buf, p_contrails, bg_contrails, contrail_nodes, contrail_segs, p_tree, p_tree_near, imp, p_tree_shadow, p_tree_shadow_fixed, p_card, near_buf, p_inst, p_inst_shadow, inst_buf, fall_buf, tree_lib: None, inst_lib: None,
            p_scenery, p_scenery_shadow, l_scenery, scenery_buf, scenery_lib: None, bg_scenery: None,
            gpu_timer: crate::gputime::GpuTimer::new(&device, &queue),
            frame_clock: crate::gputime::FrameClock::new(&device, &queue),
            globals_ub, bg0, bg0_pre, bg0_shadow, lut_sky, shadow, shadow_res: SHADOW_RES, bg0_src, scale: 1.0, ao: true, cloud_step: 1.0, l_tile, l_build, l_comp_cls, l_comp_main, l_comp_edge, l_comp_globe, l_hiz, ap_l, ap_t, cloud_refl, bg_atmlut, bg_comp_sky, p_hiz_build, p_ap_build, p_sky_build, p_cloud_refl, p_cloud_refl_plain, l_cloud, l_cloud_storm, l_cloud_rain, l_cloud_apply, l_cloud_temporal, l_post, l_pdraw, l_glass,
            tile_ub, l_photo, photo_ub, p_photo, p_photo_mask, p_photo_shadow, p_photo_shadow_fast, l_skin, p_skin, skin_slots, p_water_top, p_lamp_top, photo_mask, photo_mask_dirty: true, light_cover, light_cover_view, glow_tex, gclouds_tex, snow_tex, snow_track_tex, build_ub, obj_ub, bg_obj, l_obj, paint_ub, font_ub, tile_ibuf, tile_nidx: ix.len() as u32,
            p_sky, p_terrain, p_terrain_far, p_terrain_water, p_terrain_tess, p_terrain_cells, p_terrain_photo, p_terrain_tess_photo, p_terrain_cells_photo, tess_ibuf, tess_idx, tess_buf, tess_cap, cell_ibuf, p_build, p_build_whole, p_build_shadow, obj_gen: None, obj_gen_rx: Some(obj_gen_rx), wait_objs: false, obj_wait: None, obj_src, pl_obj, obj_fast: None, obj_fast_rx: None, p_comp_cls, p_comp_sky, p_comp_opaque, p_comp_glass, p_comp_water, p_comp_edge, p_composite, p_composite_edge, p_cloud, p_cloud_plain, p_cloud_classify, p_cloud_plain_cls, p_cloud_storm_cls, p_cloud_rain, p_cloud_apply, p_cloud_temporal, p_particles, p_taa, p_psim, p_down, p_up, p_final, p_final_taa, p_blit,
            psim_ub, pbuf, bg_psim, phead: 0, p_vol, bg_vol_a, bg_vol_b, vol_acc, fluid, fluid_t, ocean, shore, p_shore_cover, smoke_until: 0.0, parts_until: 0.0, now: 0.0, black, targets, cloud_flip: 0, cloud_hist_ok: false, clouds: true, cloud_shadows: true, vendor: info.vendor, cshadow, lut_bake, lut_haze: 1.0, taa_flip: 0, taa_hist_ok: false, up, gpu: info.clone(), meshes: Vec::new(), egui, instance, window, gpu_error, device_lost, surface, config, queue, device, stats: [0.0; 4], headless: false, integrated,
        })
    }

    /// Takes the latest uncaptured GPU or device-lost error message, if any.
    pub fn take_gpu_error(&self) -> Option<String> {
        self.gpu_error.lock().take()
    }

    /// True if the GPU device was lost.
    pub fn is_device_lost(&self) -> bool {
        self.device_lost.load(std::sync::atomic::Ordering::Relaxed)
    }

    #[allow(clippy::too_many_arguments)]
    fn make_targets(
        d: &wgpu::Device, config: &wgpu::SurfaceConfiguration, (w, h): (u32, u32), l_dres: &wgpu::BindGroupLayout, l_ao: &wgpu::BindGroupLayout, l_hiz: &wgpu::BindGroupLayout,
        l_comp_cls: &wgpu::BindGroupLayout, l_comp_main: &wgpu::BindGroupLayout, l_comp_edge: &wgpu::BindGroupLayout, l_comp_globe: &wgpu::BindGroupLayout,
        ap_l: &wgpu::TextureView, ap_t: &wgpu::TextureView, cloud_refl: &wgpu::TextureView,
        l_cloud: &wgpu::BindGroupLayout, l_cloud_storm: &wgpu::BindGroupLayout, l_cloud_rain: &wgpu::BindGroupLayout, l_cloud_apply: &wgpu::BindGroupLayout, l_cloud_temporal: &wgpu::BindGroupLayout,
        l_post: &wgpu::BindGroupLayout, l_pdraw: &wgpu::BindGroupLayout, l_glass: &wgpu::BindGroupLayout, psim_ub: &wgpu::Buffer, pbuf: &wgpu::Buffer, black: &wgpu::TextureView,
    ) -> Targets {
        // Everything is drawn at the render resolution (w, h); the tonemap
        // scales it to the window's, where the UI goes on top.
        let (ow, oh) = (config.width, config.height);
        let rt = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let ms = |label: &str, f| view(&make_tex_ms(d, label, w, h, 1, 1, f, rt, msaa()));
        let amb = view(&make_tex(d, "amb", w, h, 1, 1, AMB, rt));
        let ao_raw = view(&make_tex(d, "ao raw", w, h, 1, 1, AMB, rt));
        let ao = view(&make_tex(d, "ao", w, h, 1, 1, AMB, rt));
        let color = view(&make_tex(d, "color", w, h, 1, 1, HDR, rt));
        let aux = view(&make_tex(d, "aux", w, h, 1, 1, HDR, rt));
        // Without MSAA the g-buffer is drawn straight into what the composite
        // reads (there is nothing to resolve); depth keeps its own copy.
        let (color_ms, aux_ms, amb_ms) = if msaa() > 1 { (ms("color ms", HDR), ms("aux ms", HDR), ms("amb ms", AMB)) } else { (color.clone(), aux.clone(), amb.clone()) };
        let depth_ms = ms("depth ms", DEPTH);
        let depth_tex = make_tex(d, "depth", w, h, 1, 1, DEPTH, rt);
        let depth = view(&depth_tex);
        let hiz = view(&make_tex(d, "ssr hiz", w.div_ceil(8).max(1), h.div_ceil(8).max(1), 1, 1, wgpu::TextureFormat::R32Float, wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING));
        let comp_cls_depth = view(&make_tex(d, "comp cls depth", w, h, 1, 1, wgpu::TextureFormat::Depth16Unorm, wgpu::TextureUsages::RENDER_ATTACHMENT));
        let hdr_tex = make_tex(d, "hdr", w, h, 1, 1, HDR, rt | wgpu::TextureUsages::COPY_SRC);
        let hdr = view(&hdr_tex);
        let scene_tex = make_tex(d, "scene behind glass", w, h, 1, 1, HDR, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let (cw, ch) = ((w / 2).max(1), (h / 2).max(1));
        let cloud = view(&make_tex(d, "cloud", cw, ch, 1, 1, HDR, rt));
        let cloud_dist = view(&make_tex(d, "cloud dist", cw, ch, 1, 1, CLOUD_DIST, rt));
        let cloud_raw = view(&make_tex(d, "cloud raw", cw, ch, 1, 1, HDR, rt));
        let cloud_dist_raw = view(&make_tex(d, "cloud dist raw", cw, ch, 1, 1, CLOUD_DIST, rt));
        let cloud_cls_data = view(&make_tex(d, "cloud cls data", cw, ch, 1, 1, wgpu::TextureFormat::Rgba32Uint, rt));
        let cloud_cls_depth = view(&make_tex(d, "cloud cls depth", cw, ch, 1, 1, wgpu::TextureFormat::Depth16Unorm, wgpu::TextureUsages::RENDER_ATTACHMENT));
        let cloud_hist = [view(&make_tex(d, "cloud hist 0", cw, ch, 1, 1, HDR, rt)), view(&make_tex(d, "cloud hist 1", cw, ch, 1, 1, HDR, rt))];
        let bloom_tex = make_tex(d, "bloom", (w / 2).max(64), (h / 2).max(64), 1, BLOOM_MIPS, HDR, rt);
        let bloom: Vec<_> = (0..BLOOM_MIPS).map(|i| bloom_tex.create_view(&wgpu::TextureViewDescriptor { base_mip_level: i, mip_level_count: Some(1), ..Default::default() })).collect();
        let ldr_tex = make_tex(d, "ldr", ow, oh, 1, 1, out_format(config), rt | wgpu::TextureUsages::COPY_SRC);
        let ldr = view(&ldr_tex);
        let taa = [view(&make_tex(d, "taa 0", w, h, 1, 1, HDR, rt)), view(&make_tex(d, "taa 1", w, h, 1, 1, HDR, rt))];
        let bg = |l: &wgpu::BindGroupLayout, e: &[wgpu::BindGroupEntry]| d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: l, entries: e });
        let post = |src: &wgpu::TextureView, bl: &wgpu::TextureView| bg(l_post, &[tex_res(0, src), tex_res(1, bl), tex_res(2, &depth)]);
        Targets {
            bg_dres: bg(l_dres, &[tex_res(0, &depth_ms)]),
            comp_cls_depth,
            bg_hiz: bg(l_hiz, &[tex_res(0, &depth), tex_res(1, &hiz)]),
            bg_comp_cls: bg(l_comp_cls, &[tex_res(0, &color), tex_res(1, &aux), tex_res(2, &depth), tex_res(6, &aux_ms)]),
            bg_comp_main: bg(l_comp_main, &[
                tex_res(0, &color), tex_res(1, &aux), tex_res(2, &depth), tex_res(3, &ao), tex_res(4, &amb),
                tex_res(9, ap_l), tex_res(10, ap_t), tex_res(12, cloud_refl), tex_res(13, &hiz),
            ]),
            bg_comp_edge: bg(l_comp_edge, &[
                tex_res(0, &color), tex_res(1, &aux), tex_res(2, &depth), tex_res(3, &ao),
                tex_res(5, &color_ms), tex_res(6, &aux_ms), tex_res(7, &depth_ms), tex_res(8, &amb_ms), tex_res(12, cloud_refl),
            ]),
            bg_comp_globe: bg(l_comp_globe, &[
                tex_res(0, &color), tex_res(1, &aux), tex_res(2, &depth), tex_res(3, &ao), tex_res(4, &amb), tex_res(12, cloud_refl),
            ]),
            color_ms, aux_ms, amb_ms, depth_ms,
            // Each pass reads the other AO texture from the one it writes.
            bg_gtao: bg(l_ao, &[tex_res(0, &depth), tex_res(1, &aux), tex_res(2, &ao)]),
            bg_ao_blur: bg(l_ao, &[tex_res(0, &depth), tex_res(1, &aux), tex_res(2, &ao_raw)]),
            ao_raw, ao,
            bg_cloud: bg(l_cloud, &[tex_res(0, &depth)]),
            bg_cloud_storm: bg(l_cloud_storm, &[tex_res(0, &depth), tex_res(6, &cloud_cls_data)]),
            bg_cloud_plain_comp: bg(l_cloud_rain, &[tex_res(0, &depth), tex_res(2, &cloud), tex_res(3, &cloud_dist), tex_res(6, &cloud_cls_data)]),
            bg_cloud_rain: bg(l_cloud_rain, &[tex_res(0, &depth), tex_res(2, &cloud_raw), tex_res(3, &cloud_dist_raw), tex_res(6, &cloud_cls_data)]),
            bg_cloud_temporal: [0, 1].map(|i| bg(l_cloud_temporal, &[tex_res(2, &cloud), tex_res(3, &cloud_dist), tex_res(4, &cloud_hist[1 - i])])),
            bg_cloud_apply: [0, 1].map(|i| bg(l_cloud_apply, &[tex_res(0, &depth), tex_res(1, &cloud_hist[i]), tex_res(3, &cloud_dist), tex_res(5, &color)])),
            bg_glass: bg(l_glass, &[tex_res(1, &view(&scene_tex)), tex_res(2, &depth)]),
            hdr_tex, scene_tex,
            bg_particles: bg(l_pdraw, &[buf_res(0, psim_ub, 0), buf_res(1, pbuf, 0), tex_res(2, &depth)]),
            bg_taa: [0, 1].map(|i| post(&hdr, &taa[1 - i])),
            bg_down0: [&hdr, &taa[0], &taa[1]].map(|src| post(src, black)),
            bg_down: (1..BLOOM_MIPS as usize).map(|i| post(&bloom[i - 1], black)).collect(),
            bg_up: (0..BLOOM_MIPS as usize - 1).map(|i| post(&bloom[i + 1], black)).collect(),
            bg_final: [&hdr, &taa[0], &taa[1]].map(|src| post(src, &bloom[0])),
            taa,
            bg_blit: post(&ldr, black),
            color, aux, amb, depth, depth_tex, hdr, cloud, cloud_dist, cloud_raw, cloud_dist_raw, cloud_cls_data, cloud_cls_depth, cloud_hist, bloom, ldr_tex, ldr,
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 || (w == self.config.width && h == self.config.height) {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.cshadow.ok = false;
        self.remake_targets();
    }

    /// The size everything is drawn at before the tonemap: the window's
    /// scaled by the render resolution setting.
    pub fn render_size(&self) -> (u32, u32) {
        let max = self.device.limits().max_texture_dimension_2d;
        let s = |v: u32| ((v as f32 * self.scale).round() as u32).clamp(1, max);
        (s(self.config.width), s(self.config.height))
    }

    /// Side of the part of its shadow map layer the aircraft's shadow uses.
    pub fn plane_shadow_res(&self) -> u32 {
        if self.integrated { self.shadow_res / PLANE_SHADOW_DIV_INTEGRATED } else { self.shadow_res }
    }

    fn remake_targets(&mut self) {
        self.cloud_hist_ok = false;
        self.taa_hist_ok = false;
        let (w, h) = self.render_size();
        self.targets = Self::make_targets(&self.device, &self.config, (w, h), &self.l_dres, &self.l_ao, &self.l_hiz, &self.l_comp_cls, &self.l_comp_main, &self.l_comp_edge, &self.l_comp_globe, &self.ap_l, &self.ap_t, &self.cloud_refl, &self.l_cloud, &self.l_cloud_storm, &self.l_cloud_rain, &self.l_cloud_apply, &self.l_cloud_temporal, &self.l_post, &self.l_pdraw, &self.l_glass, &self.psim_ub, &self.pbuf, &self.black);
        self.fluid_t = self.fluid.draw_targets(&self.device, &self.targets.depth, w, h);
        self.rebuild_upscale();
    }

    /// Remakes what the upscaler in use needs for the current targets.
    fn rebuild_upscale(&mut self) {
        let t = &self.targets;
        let i = crate::upscale::Inputs {
            render: (t.hdr_tex.width(), t.hdr_tex.height()),
            display: (self.config.width, self.config.height),
            hdr: &t.hdr,
            taa: [&t.taa[0], &t.taa[1]],
            depth: &t.depth,
            bloom: &t.bloom[0],
            black: &self.black,
            l_post: &self.l_post,
        };
        self.up.rebuild(&self.device, &i);
    }

    /// The upscaler asked for (graphics settings); `upscale::active` decides
    /// which runs at the render resolution.
    pub fn set_upscaler(&mut self, u: crate::graphics::Upscaler) {
        if self.up.set(u, self.scale) {
            self.rebuild_upscale();
        }
    }

    /// The GPU time of a recent frame (ms), where the adapter has timestamps.
    pub fn frame_gpu_ms(&self) -> Option<f32> {
        self.frame_clock.as_ref().and_then(|c| c.last)
    }

    /// Render resolution as a share of the window's (graphics settings).
    pub fn set_scale(&mut self, s: f32) {
        let s = s.clamp(crate::graphics::SCALE_MIN, crate::graphics::SCALE_MAX);
        if s != self.scale {
            let old = self.render_size();
            self.scale = s;
            let up_changed = self.up.rescale(s);
            if self.render_size() != old {
                self.remake_targets();
            } else if up_changed {
                self.rebuild_upscale();
            }
        }
    }

    pub fn shadow_res(&self) -> u32 {
        self.shadow_res
    }

    /// Frames a redraw of the cloud shadow map is spread over (graphics
    /// settings); never fewer than an integrated GPU's.
    pub fn set_cloud_shadow_slices(&mut self, n: u32) {
        let n = n.max(if self.integrated { CS_SLICES_INTEGRATED } else { CS_SLICES });
        if n != self.cshadow.slices {
            self.cshadow.slices = n;
            // (a redraw begun at the old pace starts over, whole)
            self.cshadow.ok = false;
        }
    }

    /// A new sun shadow map of this side (texels).
    pub fn set_shadow_res(&mut self, res: u32) {
        let res = res.clamp(256, self.device.limits().max_texture_dimension_2d);
        if res != self.shadow_res {
            let (all, layers) = make_shadow(&self.device, res);
            self.bg0 = self.bg0_src.make(&self.device, &all);
            self.shadow = layers;
            self.shadow_res = res;
        }
    }

    pub fn set_vsync(&mut self, on: bool) {
        let mode = if on { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::AutoNoVsync };
        if self.config.present_mode != mode {
            self.config.present_mode = mode;
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// A new cloud map for the start screen's globe (globeclouds.rs: W x H bytes).
    pub fn upload_globe_clouds(&self, px: &[u8]) {
        let (w, h) = (crate::globeclouds::W, crate::globeclouds::H);
        if px.len() != (w * h) as usize {
            return;
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &self.gclouds_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            px,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w), rows_per_image: Some(h) },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
    }

    pub fn upload_tile(&self, t: &crate::sources::TileData) -> TileGpu {
        let d = &self.device;
        let vbuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&t.verts), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE });
        let img = upload_mips(d, &self.queue, "imagery", t.img_size, wgpu::TextureFormat::Rgba8UnormSrgb, 4, &t.imagery);
        let mask = upload_mips(d, &self.queue, "mask", crate::sources::MASK as u32, wgpu::TextureFormat::Rgba8Unorm, 4, &t.mask);
        let lights = upload_mips(d, &self.queue, "lights", crate::sources::MASK as u32, wgpu::TextureFormat::R8Unorm, 1, &t.lights);
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.l_tile, entries: &[buf_res(0, &self.tile_ub, 48), tex_res(1, &img), tex_res(2, &mask), buf_res(3, &vbuf, 0), tex_res(4, &lights), tex_res(5, &self.light_cover_view)] });
        let mut wet = false;
        let mut all_water = true;
        for p in t.mask[0].chunks_exact(4) {
            if p[0] > 0 { wet = true; }
            if p[0] <= 127 { all_water = false; }
        }
        TileGpu { vbuf, bind, radius: t.radius as f32, wet, all_water }
    }

    /// Create a building tile's GPU objects. The geometry buffers start out
    /// empty: a dense downtown tile is ~100 MB, and every byte written goes
    /// through wgpu staging memory, which (without resizable BAR) sits in a
    /// 246 MiB heap. Filling them all at once overflows that heap and makes
    /// every later upload crawl, so `World::update` fills them a slice per
    /// frame with `fill_buffer`, matching `BuildGpu::targets` to
    /// `building_payload`.
    /// A photorealistic 3D tile: its mesh and its texture (with its mips).
    pub fn upload_photo(&self, m: &crate::tiles3d::MeshData) -> PhotoGpu {
        let d = &self.device;
        let vbuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("photo"), contents: bytemuck::cast_slice(&m.verts), usage: wgpu::BufferUsages::VERTEX });
        let ibuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("photo idx"), contents: bytemuck::cast_slice(&m.idx), usage: wgpu::BufferUsages::INDEX });
        let (w, h) = (m.mips[0].0, m.mips[0].1);
        let t = make_tex(d, "photo", w, h, 1, m.mips.len() as u32, wgpu::TextureFormat::Rgba8UnormSrgb, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        for (i, (mw, mh, px)) in m.mips.iter().enumerate() {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &t, mip_level: i as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                px,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(mw * 4), rows_per_image: Some(*mh) },
                wgpu::Extent3d { width: *mw, height: *mh, depth_or_array_layers: 1 },
            );
        }
        let tv = view(&t);
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.l_photo, entries: &[buf_res(0, &self.photo_ub, 32), tex_res(1, &tv)] });
        let rad = m.verts.iter().map(|v| glam::Vec3::from(v.pos).length()).fold(0.0f32, f32::max);
        PhotoGpu { vbuf, ibuf, n_idx: m.idx.len() as u32, bind, view: tv, rad }
    }

    /// A destroyed building's mesh from one tile (skin.rs), with that tile's texture.
    pub fn make_skin(&self, verts: &[SkinVertex], tex: &wgpu::TextureView) -> SkinGpu {
        let d = &self.device;
        let vbuf = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("skin"), contents: bytemuck::cast_slice(verts), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST });
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.l_skin, entries: &[tex_res(1, tex), buf_res(2, &self.skin_slots, (SKIN_SLOTS * 64) as u64)] });
        SkinGpu { vbuf, bind, n: verts.len() as u32 }
    }

    /// Rewrites some of a skin's vertices (pieces that broke in two).
    pub fn write_skin(&self, s: &SkinGpu, first: usize, verts: &[SkinVertex]) {
        self.queue.write_buffer(&s.vbuf, (first * std::mem::size_of::<SkinVertex>()) as u64, bytemuck::cast_slice(verts));
    }

    pub fn upload_buildings(&self, b: &crate::buildings::BuildingData) -> BuildGpu {
        let d = &self.device;
        let buf = |data: &[u8], usage| d.create_buffer(&wgpu::BufferDescriptor { label: None, size: (data.len().max(64) as u64).next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT), usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let vbuf = buf(bytemuck::cast_slice(&b.verts), wgpu::BufferUsages::VERTEX);
        let ibuf = buf(bytemuck::cast_slice(&b.indices), wgpu::BufferUsages::INDEX);
        let atlas = upload_mips(d, &self.queue, "atlas", b.atlas_size, wgpu::TextureFormat::Rgba8UnormSrgb, 4, &b.atlas);
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.l_build, entries: &[buf_res(0, &self.build_ub, 64), tex_res(1, &atlas)] });
        let road = (!b.road_idx.is_empty()).then(|| (buf(bytemuck::cast_slice(&b.road_verts), wgpu::BufferUsages::VERTEX), buf(bytemuck::cast_slice(&b.road_idx), wgpu::BufferUsages::INDEX), b.road_idx.len() as u32));
        let trees = (!b.trees.is_empty()).then(|| buf(bytemuck::cast_slice(&b.trees), wgpu::BufferUsages::VERTEX));
        BuildGpu { vbuf, ibuf, n_main: b.n_main, n_det: b.n_det, n_far: b.n_far, cell_end: b.cell_end.clone(), cell_bound: b.cell_bound.clone(), bind, road, trees }
    }

    pub fn upload_field(&self, m: &crate::airfield::FieldMesh) -> FieldGpu {
        let d = &self.device;
        let init = |data: &[u8], usage| d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("airfield"), contents: if data.is_empty() { &[0u8; 64] } else { data }, usage });
        FieldGpu {
            vbuf: init(bytemuck::cast_slice(&m.verts), wgpu::BufferUsages::VERTEX),
            ibuf: init(bytemuck::cast_slice(&m.idx), wgpu::BufferUsages::INDEX),
            n_idx: m.idx.len() as u32,
            lights: (!m.lights.is_empty()).then(|| (init(bytemuck::cast_slice(&m.lights), wgpu::BufferUsages::VERTEX), m.lights.len() as u32)),
        }
    }
    /// Bytes an airfield's upload takes, for the per-frame budget.
    pub fn field_bytes(m: &crate::airfield::FieldMesh) -> usize {
        std::mem::size_of_val(&m.verts[..]) + m.idx.len() * 4 + std::mem::size_of_val(&m.lights[..])
    }

    /// Write `data` into `buf` at byte offset `off` (both multiples of 4).
    pub fn fill_buffer(&self, buf: &wgpu::Buffer, off: u64, data: &[u8]) {
        self.queue.write_buffer(buf, off, data);
    }

    fn mesh_lib<V: Pod>(&self, meshes: &[(Vec<V>, Vec<u32>)]) -> MeshLib {
        let (mut v, mut i, mut ranges) = (Vec::new(), Vec::new(), Vec::new());
        for (mv, mi) in meshes {
            ranges.push((i.len() as u32, mi.len() as u32, v.len() as i32));
            v.extend_from_slice(mv);
            i.extend_from_slice(mi);
        }
        let d = &self.device;
        MeshLib {
            vbuf: d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&v), usage: wgpu::BufferUsages::VERTEX }),
            ibuf: d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&i), usage: wgpu::BufferUsages::INDEX }),
            ranges,
        }
    }
    /// Tree and undergrowth models, indexed by `variant * LODS + level of detail`
    /// (the last "level" being the model's leaf cards).
    /// Also takes the pictures for the impostors.
    pub fn set_tree_meshes(&mut self, meshes: &[(Vec<TreeVertex>, Vec<u32>)]) {
        let lib = self.mesh_lib(meshes);
        let bounds = crate::impostor::Impostors::bounds(meshes);
        self.imp.bake(&self.device, &self.queue, &self.bg0, &self.bg_tileu, &lib.vbuf, &lib.ibuf, &lib.ranges, &bounds);
        self.tree_lib = Some(lib);
    }
    /// Models for instanced solids (cars, rubble, rocks).
    pub fn set_inst_meshes(&mut self, meshes: &[(Vec<MeshVertex>, Vec<u32>)]) {
        self.inst_lib = Some(self.mesh_lib(meshes));
    }

    /// The hand-made airport scenery (scenery.rs): every pack's meshes in one
    /// library (in pack order, as `Pack::mesh_base` counts them), their
    /// materials and texture layers.
    pub fn set_scenery(&mut self, packs: &[crate::scenery::Pack]) {
        let mut meshes = Vec::new();
        let mut mats: Vec<crate::scenery::SceneryMat> = Vec::new();
        let size = packs.iter().map(|p| p.tex_size).max().unwrap_or(1).max(1);
        let (mut base, mut nrm): (Vec<Vec<u8>>, Vec<Vec<u8>>) = (Vec::new(), Vec::new());
        for p in packs {
            let (mat0, layer0) = (mats.len() as u32, base.len() as f32);
            for (b, n) in &p.layers {
                let fit = |px: &Vec<u8>| if p.tex_size == size { px.clone() } else { image::imageops::resize(&image::RgbaImage::from_raw(p.tex_size, p.tex_size, px.clone()).unwrap(), size, size, image::imageops::FilterType::Triangle).into_raw() };
                base.push(fit(b));
                nrm.push(fit(n));
            }
            mats.extend(p.mats.iter().map(|m| {
                let mut m = *m;
                if m.p[3] >= 0.0 {
                    m.p[3] += layer0;
                }
                m
            }));
            for m in &p.meshes {
                let verts = m.verts.iter().map(|v| crate::scenery::SceneryVertex { mat: v.mat + mat0, ..*v }).collect::<Vec<_>>();
                meshes.push((verts, m.idx.clone()));
            }
        }
        if meshes.is_empty() {
            return;
        }
        assert!(mats.len() <= SC_MATS, "scenery: {} materials, the shader holds {SC_MATS}", mats.len());
        if base.is_empty() {
            base.push(vec![255; (size * size * 4) as usize]);
            nrm.push([128u8, 128, 200, 0].repeat((size * size) as usize));
        }
        mats.resize(SC_MATS, crate::scenery::SceneryMat::zeroed());
        let d = &self.device;
        let ub = d.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("scenery materials"), contents: bytemuck::cast_slice(&mats), usage: wgpu::BufferUsages::UNIFORM });
        let tb = layer_texture(d, &self.queue, "scenery base", size, &base, wgpu::TextureFormat::Rgba8UnormSrgb);
        let tn = layer_texture(d, &self.queue, "scenery normal", size, &nrm, wgpu::TextureFormat::Rgba8Unorm);
        self.bg_scenery = Some(d.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("scenery"), layout: &self.l_scenery, entries: &[buf_res(0, &ub, SC_MATS as u64 * 64), tex_res(1, &tb), tex_res(2, &tn)] }));
        self.scenery_lib = Some(self.mesh_lib(&meshes));
    }

    /// The paint of the aircraft being flown (none: `None`): only its pictures are on the GPU.
    pub fn set_paint(&mut self, paint: Option<&crate::aircraft::Paint>) {
        let view = paint_texture(&self.device, &self.queue, paint);
        self.queue.write_buffer(&self.paint_ub, 0, bytemuck::cast_slice(&paint_box(paint)));
        self.bg_obj = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.l_obj, entries: &[buf_res(0, &self.obj_ub, std::mem::size_of::<ObjU>() as u64), tex_res(1, &view), buf_res(2, &self.paint_ub, PAINT_UB), buf_res(3, &self.font_ub, self.font_ub.size())] });
    }

    /// The paint scheme of a type that takes one (`aircraft::livery_uniform`:
    /// four colours, the style in the first's w, the title's glyphs).
    pub fn set_livery(&mut self, scheme: [u32; 20]) {
        self.queue.write_buffer(&self.paint_ub, 32, bytemuck::cast_slice(&scheme));
    }

    pub fn add_mesh(&mut self, verts: &[PartVertex], idx: &[u32]) -> usize {
        let m = self.mesh_gpu(verts, idx, None);
        self.meshes.push(m);
        self.meshes.len() - 1
    }
    /// A downloaded aircraft's textures, as the object bind group its meshes draw with.
    pub fn texture_set(&self, t: &crate::fgmodel::TexSet) -> std::sync::Arc<wgpu::BindGroup> {
        let white = vec![255u8; (t.size * t.size * 4) as usize];
        let layers: Vec<&[u8]> = if t.layers.is_empty() { vec![&white[..]] } else { t.layers.iter().map(|l| &l[..]).collect() };
        let view = array_texture(&self.device, &self.queue, "aircraft textures", t.size, t.size, &layers);
        std::sync::Arc::new(self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.l_obj, entries: &[buf_res(0, &self.obj_ub, std::mem::size_of::<ObjU>() as u64), tex_res(1, &view), buf_res(2, &self.paint_ub, PAINT_UB), buf_res(3, &self.font_ub, self.font_ub.size())] }))
    }
    /// Draw a mesh with its own textures.
    pub fn set_mesh_bind(&mut self, i: usize, bind: std::sync::Arc<wgpu::BindGroup>) {
        self.meshes[i].bind = Some(bind);
    }
    /// Swap the geometry behind an existing mesh id.
    pub fn replace_mesh(&mut self, i: usize, verts: &[PartVertex], idx: &[u32]) {
        let bind = self.meshes[i].bind.take();
        self.meshes[i] = self.mesh_gpu(verts, idx, bind);
    }
    /// ask for the aircraft shader specialised to `kind`, built on a worker thread.
    /// until it is ready the generic pipelines draw everything.
    fn want_obj_kind(&mut self, kind: u32) {
        if let Some((k, rx)) = &self.obj_fast_rx {
            if let Ok(f) = rx.try_recv() {
                self.obj_fast = Some(f);
                self.obj_fast_rx = None;
            } else if *k == kind {
                return;
            }
        }
        if self.obj_fast.as_ref().is_some_and(|f| f.kind == kind) || kind >= 99 {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let (d, src, pl) = (self.device.clone(), self.obj_src.clone(), self.pl_obj.clone());
        // A roomy stack: the aircraft shader is the biggest, and on DirectX 12
        // naga's HLSL writer and DXC compile it on this thread (2 MB, Rust's
        // default for a spawned thread, isn't enough there).
        use std::sync::atomic::Ordering::Relaxed;
        let _ = FAST_STATE.compare_exchange(0, 1, Relaxed, Relaxed);
        FAST_STEPS.store(0, Relaxed);
        let spawned = std::thread::Builder::new().name("obj shader".into()).stack_size(16 << 20).spawn(move || {
            let t = std::time::Instant::now();
            let f = {
                let _big = BIG_COMPILE.lock().unwrap_or_else(|e| e.into_inner());
                let m = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("plane fast"), source: wgpu::ShaderSource::Wgsl(specialise(&src, kind).into()) });
                let cut = obj_pipeline(&d, &pl, &m, true);
                FAST_STEPS.store(1, Relaxed);
                let solid = obj_pipeline(&d, &pl, &m, false);
                FAST_STEPS.store(2, Relaxed);
                ObjFast { kind, cut, solid }
            };
            FAST_STATE.store(2, Relaxed);
            log::info!("aircraft shader for kind {kind} ready in {:.1} s", t.elapsed().as_secs_f64());
            let _ = tx.send(f);
        });
        if spawned.is_ok() {
            self.obj_fast_rx = Some((kind, rx));
        } else {
            FAST_STATE.store(2, Relaxed);
        }
    }
    /// Whether the aircraft `kind` (of the first object drawn, if any) can be
    /// drawn yet, and if not how far its pipelines are (`aircraft_prep`).
    fn update_obj_wait(&mut self, kind: Option<u32>) {
        use std::sync::atomic::Ordering::Relaxed;
        let drawn = kind.is_none_or(|k| self.obj_gen.is_some() || self.obj_fast.as_ref().is_some_and(|f| f.kind == k));
        if drawn {
            self.obj_wait = None;
            return;
        }
        // whichever of the two sets is nearer: the first done draws it
        let fast = if self.obj_fast_rx.is_some() { FAST_STEPS.load(Relaxed) } else { 0 };
        let done = fast.max(GEN_STEPS.load(Relaxed)).min(1) as u32;
        let t0 = self.obj_wait.map_or_else(std::time::Instant::now, |w| w.0);
        self.obj_wait = Some((t0, AircraftPrep { done, total: 2, secs: t0.elapsed().as_secs_f64() }));
    }
    /// The aircraft is not drawn yet: its shaders are being compiled (minutes
    /// on some drivers with a cold cache). None once it is drawn.
    pub fn aircraft_prep(&self) -> Option<AircraftPrep> {
        self.obj_wait.map(|w| w.1)
    }
    fn mesh_gpu(&self, verts: &[PartVertex], idx: &[u32], bind: Option<std::sync::Arc<wgpu::BindGroup>>) -> MeshGpu {
        // an empty piece, a hinge others hang from, still gets a buffer.
        let pad = [PartVertex::zeroed()];
        let verts = if verts.is_empty() { &pad[..] } else { verts };
        let (idx, n_main) = split_instruments(verts, idx);
        let vbuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(verts), usage: wgpu::BufferUsages::VERTEX });
        let ibuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(if idx.is_empty() { &[0u32][..] } else { &idx }), usage: wgpu::BufferUsages::INDEX });
        MeshGpu { vbuf, ibuf, n_idx: idx.len() as u32, n_main, bind }
    }

    /// Append particles to the ring buffer (oldest are overwritten).
    pub fn emit(&mut self, ps: &[Particle]) {
        let mut ps = ps;
        let smoky = ps.iter().filter(|p| matches!(p.kind as u32, 0 | 2 | 5 | 7 | 8)).map(|p| p.life).fold(0.0f32, f32::max);
        if smoky > 0.0 {
            self.smoke_until = self.smoke_until.max(self.now + smoky + 0.5);
        }
        if let Some(life) = ps.iter().map(|p| p.life).reduce(f32::max) {
            self.parts_until = self.parts_until.max(self.now + life + 1.0);
        }
        if ps.len() > MAX_PARTICLES as usize {
            ps = &ps[..MAX_PARTICLES as usize];
        }
        let first = (MAX_PARTICLES - self.phead).min(ps.len() as u32) as usize;
        self.queue.write_buffer(&self.pbuf, self.phead as u64 * 64, bytemuck::cast_slice(&ps[..first]));
        if first < ps.len() {
            self.queue.write_buffer(&self.pbuf, 0, bytemuck::cast_slice(&ps[first..]));
        }
        self.phead = (self.phead + ps.len() as u32) % MAX_PARTICLES;
    }
    pub fn clear_particles(&mut self) {
        self.queue.write_buffer(&self.pbuf, 0, &vec![0u8; MAX_PARTICLES as usize * 64]);
        self.phead = 0;
        self.smoke_until = 0.0;
        self.parts_until = 0.0;
    }

    /// Start loading the photo-scanned ground materials (see materials.rs).
    pub fn load_ground_photos(&mut self, fetch: crate::fetch::Fetcher) {
        self.ground_mats.photos = Some(crate::materials::spawn(fetch));
    }

    pub fn render(&mut self, mut f: Frame) {
        if self.up.stale {
            // (AMD's runtime failed last frame: FSR 3.1 takes over)
            self.rebuild_upscale();
        }
        if self.is_device_lost() {
            std::thread::sleep(std::time::Duration::from_millis(30));
            return;
        }
        // The UI's texture changes are taken in whether or not a frame can be
        // drawn: each builds on the one before, so one dropped with a skipped
        // frame (a hidden window on macOS) would leave the next nothing to patch.
        if let Some((_, delta, _)) = &mut f.egui {
            for (id, ds) in delta.set.drain() {
                for d in &ds {
                    self.egui.update_texture(&self.device, &self.queue, id, d);
                }
            }
        }
        let t_acq = std::time::Instant::now();
        // Headless: everything is drawn but nothing is shown, so no frame of the
        // window's is needed (and a hidden or locked screen does not stop it).
        let frame = if self.headless {
            None
        } else {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
                wgpu::CurrentSurfaceTexture::Outdated => {
                    self.surface.configure(&self.device, &self.config);
                    None
                }
                wgpu::CurrentSurfaceTexture::Lost => {
                    match self.instance.create_surface(self.window.clone()) {
                        Ok(s) => self.surface = s,
                        Err(e) => log::error!("recreating lost GPU surface failed: {e}"),
                    }
                    self.surface.configure(&self.device, &self.config);
                    None
                }
                _ => {
                    // Hidden or minimised (macOS hands out no frames then): idle
                    // rather than spin the loop at thousands of empty frames a second.
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    None
                }
            }
        };
        if frame.is_none() && !self.headless {
            if let Some((_, delta, _)) = &mut f.egui {
                for id in delta.free.drain() {
                    self.egui.free_texture(&id);
                }
            }
            return;
        }
        let t_enc = std::time::Instant::now();
        self.stats[1] += (t_enc - t_acq).as_secs_f64();
        crate::gputime::cpu("render.acquire_frame");
        // (asked for first: a screenshot waiting below for the generic set
        // would otherwise hold it back until the 15 s are up)
        let kind = f.objs.iter().find(|o| !o.glass).map(|o| o.glow[1].round().max(0.0) as u32);
        if let Some(k) = kind {
            self.want_obj_kind(k);
        }
        while let Some(rx) = &self.obj_gen_rx {
            let got = if self.wait_objs { rx.recv().map_err(|_| crossbeam_channel::TryRecvError::Disconnected) } else { rx.try_recv() };
            match got {
                Ok(ObjGenMsg::Draw(obj, solid)) => self.obj_gen = Some(ObjGeneric { obj, solid, rest: None }),
                Ok(ObjGenMsg::Rest(r)) => {
                    if let Some(g) = &mut self.obj_gen {
                        g.rest = Some(r);
                    }
                    self.obj_gen_rx = None;
                }
                Err(crossbeam_channel::TryRecvError::Empty) => break,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    log::error!("the aircraft pipelines could not be built");
                    self.obj_gen_rx = None;
                }
            }
        }
        self.update_obj_wait(kind);
        let sview = frame.as_ref().map(|f| f.texture.create_view(&Default::default()));
        let q = &self.queue;
        self.cloud_flip ^= 1;
        if !self.cloud_hist_ok {
            f.g.cam_delta[3] = 0.0;
        }
        self.cloud_hist_ok = true;
        // Anti-aliasing blends into last frame's result, if there is one and
        // the camera has not jumped since.
        let taa = f.g.taa[2] > 0.5;
        if !self.taa_hist_ok || f.g.cam_delta[3] < 0.5 {
            f.g.taa[2] = 0.0;
        }
        self.taa_hist_ok = taa;
        self.taa_flip ^= 1;
        // A clock jump (new flight) makes the bookkeeping meaningless.
        if f.g.time[0] < self.now - 1.0 {
            self.smoke_until = 0.0;
            self.parts_until += f.g.time[0] - self.now;
        } else if !f.sim_steps {
            // (paused: the particles do not age, so neither does their deadline)
            self.parts_until += (f.g.time[0] - self.now).max(0.0);
        }
        self.now = f.g.time[0];
        let parts = self.now < self.parts_until;
        let smoke = self.now < self.smoke_until || f.fluid.is_some();
        f.g.vol_u[3] = if smoke { 1.0 } else { 0.0 };
        self.ground_mats.poll(&self.device, q);
        f.g.det = self.ground_mats.params;
        f.g.det_mean = self.ground_mats.means;
        f.g.det_hmean = self.ground_mats.hmeans;
        f.g.quality = [self.cloud_step, self.scale, 1.0 / self.config.width.max(1) as f32, 1.0 / self.config.height.max(1) as f32];
        // Cloud shadows off (or no clouds): lookups find none (common.wgsl
        // `cloud_sun`), and the map is redrawn whole once they are back on.
        let cs_on = self.clouds && self.cloud_shadows;
        let cs_full = if cs_on {
            self.cshadow.prepare(&mut f.g)
        } else {
            self.cshadow.ok = false;
            f.g.cs[4][3] = 0.0;
            false
        };
        q.write_buffer(&self.cshadow.mix_ub, 0, &self.cshadow.mix_bytes());
        let horizon = (2.0 * crate::geo::R * (f.g.up[3].max(500.0) as f64)).sqrt() as f32;
        f.g.lite[2] = (horizon * 1.4).clamp(100_000.0, 500_000.0);
        let n_tiles = f.tiles.len().min(MAX_TILES as usize);
        let n_shore_tiles = f.shore_tiles.len().min(MAX_TILES as usize - n_tiles);
        let n_builds = f.details.len().min(MAX_BUILDS as usize);
        let n_objs = f.objs.len().min(MAX_OBJS as usize);
        // Active shadow cascade bitmask (bits 0..3): lets shadow_world skip empty cascades.
        let mut casc_mask = 0u32;
        if f.g.csm[3] > 0.0 {
            for dd in f.details.iter().take(n_builds) {
                casc_mask |= dd.casc as u32 & 7;
            }
            for o in f.objs.iter().take(n_objs) {
                if !o.glass { casc_mask |= o.cast as u32 & 7; }
            }
            if !f.insts.is_empty() || !f.fall.is_empty() {
                casc_mask |= 3;
            }
            if !f.photo.is_empty() && f.g.photo[2] > 0.0 && f.g.photo_sun[3] >= 0.0 {
                casc_mask |= 7;
            }
        }
        f.g.lite[3] = casc_mask as f32;
        q.write_buffer(&self.globals_ub, 0, bytemuck::bytes_of(&f.g));
        q.write_buffer(&self.psim_ub, 0, bytemuck::bytes_of(&f.psim));
        // Outer shadow cascade and aircraft shadow matrices for per-tile shadow skipping in fs_far.
        let sm2 = glam::Mat4::from_cols_array_2d(&f.g.shadow_vp[2]);
        let (s2x, s2y) = (sm2.row(0).truncate().length(), sm2.row(1).truncate().length());
        let psm = glam::Mat4::from_cols_array_2d(&f.g.pshadow_vp);
        let (psx, psy) = (psm.row(0).truncate().length(), psm.row(1).truncate().length());
        let psh_on = f.g.pshadow[3] > 0.0;
        let mut blob = vec![0u8; (n_tiles + n_shore_tiles) * UB_ALIGN as usize];
        for (i, (tile, u)) in f.tiles.iter().take(n_tiles).chain(f.shore_tiles.iter().take(n_shore_tiles)).enumerate() {
            let mut u2 = *u;
            if !tile.wet {
                u2[4] = -u2[4];
            }
            if u2[10] < 1.0 || u2[11] < 1.0 {
                let o = glam::Vec3::new(u2[0], u2[1], u2[2]);
                let q2 = sm2 * o.extend(1.0);
                let in_csm = casc_mask != 0 && q2.x.abs() <= 1.0 + tile.radius * s2x && q2.y.abs() <= 1.0 + tile.radius * s2y;
                let qp = psm * o.extend(1.0);
                let in_psh = psh_on && qp.x.abs() <= 1.0 + tile.radius * psx && qp.y.abs() <= 1.0 + tile.radius * psy;
                // Bitmask for fs_far: bit 0 = shadow_world, bit 1 = plane_shadow.
                u2[8] = ((in_csm as u32) | ((in_psh as u32) << 1)) as f32;
            }
            blob[i * 256..i * 256 + 48].copy_from_slice(bytemuck::bytes_of(&u2));
        }
        q.write_buffer(&self.tile_ub, 0, &blob);
        if f.tess.cells.len() > self.tess_cap {
            self.tess_cap = f.tess.cells.len().next_power_of_two();
            self.tess_buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("tess cells"), size: self.tess_cap as u64 * 4, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        }
        if !f.tess.cells.is_empty() {
            q.write_buffer(&self.tess_buf, 0, bytemuck::cast_slice(&f.tess.cells));
        }
        if let Some(px) = &f.glow {
            let n = crate::skyglow::N as u32;
            q.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.glow_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                bytemuck::cast_slice(px),
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(n * 8), rows_per_image: Some(n) },
                wgpu::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
            );
        }
        if let Some(px) = &f.snow {
            let n = crate::snowmap::N as u32;
            q.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.snow_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                px,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(n * 2), rows_per_image: Some(n) },
                wgpu::Extent3d { width: n, height: n, depth_or_array_layers: 1 },
            );
        }
        for u in &f.snow_tracks {
            q.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.snow_track_tex, mip_level: 0, origin: wgpu::Origin3d { x: u.x, y: u.y, z: 0 }, aspect: wgpu::TextureAspect::All },
                &u.data,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(u.w * 2), rows_per_image: Some(u.h) },
                wgpu::Extent3d { width: u.w, height: u.h, depth_or_array_layers: 1 },
            );
        }
        if f.cover.len() == COVER_N * COVER_N {
            q.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.light_cover, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &f.cover,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(COVER_N as u32), rows_per_image: Some(COVER_N as u32) },
                wgpu::Extent3d { width: COVER_N as u32, height: COVER_N as u32, depth_or_array_layers: 1 },
            );
        }
        // One extra slot after the tiles for loose trees: they are already
        // camera-relative, and offset.w = 1 keeps them to the model they were given.
        let n_fields = f.fields.len().min(MAX_FIELDS);
        let mut blob = vec![0u8; (n_builds + 1 + n_fields) * UB_ALIGN as usize];
        for (i, dd) in f.details.iter().take(n_builds).enumerate() {
            blob[i * 256..i * 256 + 64].copy_from_slice(bytemuck::bytes_of(&dd.u));
        }
        for (i, (_, o)) in f.fields.iter().take(n_fields).enumerate() {
            let k = (n_builds + 1 + i) * 256;
            blob[k..k + 12].copy_from_slice(bytemuck::cast_slice(o));
        }
        let field_slot = |i: usize| ((n_builds + 1 + i) * 256) as u32;
        let n_bolt = f.bolts.len().min(crate::storm::MAX_SEGS);
        if n_bolt > 0 {
            q.write_buffer(&self.bolt_buf, 0, bytemuck::cast_slice(&f.bolts[..n_bolt]));
        }
        let n_cnode = f.contrails.nodes.len().min(crate::contrails::MAX_NODES);
        let n_cseg = f.contrails.segs.len().min(crate::contrails::MAX_SEGS);
        if n_cnode > 0 && n_cseg > 0 {
            q.write_buffer(&self.contrail_nodes, 0, bytemuck::cast_slice(&f.contrails.nodes[..n_cnode]));
            q.write_buffer(&self.contrail_segs, 0, bytemuck::cast_slice(&f.contrails.segs[..n_cseg]));
        }
        let n_dyn = f.lights.len().min(MAX_DYN_LIGHTS);
        if n_dyn > 0 {
            q.write_buffer(&self.dyn_lights, 0, bytemuck::cast_slice(&f.lights[..n_dyn]));
        }
        blob[n_builds * 256 + 12..n_builds * 256 + 16].copy_from_slice(&1f32.to_ne_bytes());
        q.write_buffer(&self.build_ub, 0, &blob);
        let n_slots = f.skin_slots.len().min(SKIN_SLOT_BUF);
        if n_slots > 0 {
            q.write_buffer(&self.skin_slots, 0, bytemuck::cast_slice(&f.skin_slots[..n_slots]));
        }
        let n_photo = f.photo.len().min(MAX_PHOTO as usize);
        if n_photo > 0 {
            let mut blob = vec![0u8; n_photo * UB_ALIGN as usize];
            for (i, (_, u)) in f.photo.iter().take(n_photo).enumerate() {
                blob[i * 256..i * 256 + 32].copy_from_slice(bytemuck::cast_slice(u));
            }
            q.write_buffer(&self.photo_ub, 0, &blob);
        }
        let mut blob = vec![0u8; (CASCADES + 4) * UB_ALIGN as usize];
        for (i, m) in f.casc_vp.iter().enumerate() {
            blob[i * 256..i * 256 + 64].copy_from_slice(bytemuck::bytes_of(m));
        }
        q.write_buffer(&self.casc_ub, 0, &blob);
        // Instances grouped by model so each model is one draw.
        let group = |n_meshes: usize, items: &mut dyn Iterator<Item = u32>| -> (Vec<u32>, Vec<(u32, u32)>) {
            let mut counts = vec![0u32; n_meshes + 1];
            let ids: Vec<u32> = items.collect();
            for m in &ids {
                counts[(*m as usize).min(n_meshes.saturating_sub(1)) + 1] += 1;
            }
            for k in 0..n_meshes {
                counts[k + 1] += counts[k];
            }
            let ranges = (0..n_meshes).map(|k| (counts[k], counts[k + 1])).collect();
            let mut next = counts;
            let order = ids.iter().map(|m| { let k = (*m as usize).min(n_meshes.saturating_sub(1)); next[k] += 1; next[k] - 1 }).collect();
            (order, ranges)
        };
        let mut inst_ranges = Vec::new();
        if let Some(lib) = &self.inst_lib {
            // near to far within each model, and the nearest kept if over the cap.
            let d2 = |i: &Inst| i.m[0][3] * i.m[0][3] + i.m[1][3] * i.m[1][3] + i.m[2][3] * i.m[2][3];
            f.insts.sort_by(|a, b| d2(&a.1).total_cmp(&d2(&b.1)));
            let n = f.insts.len().min(MAX_INST);
            let (order, ranges) = group(lib.ranges.len(), &mut f.insts.iter().take(n).map(|i| i.0));
            let mut data = vec![Inst::zeroed(); n];
            for (k, (_, inst)) in f.insts.iter().take(n).enumerate() {
                data[order[k] as usize] = *inst;
            }
            q.write_buffer(&self.inst_buf, 0, bytemuck::cast_slice(&data));
            inst_ranges = ranges;
        }
        let mut scenery_ranges = Vec::new();
        if let Some(lib) = &self.scenery_lib {
            let d2 = |i: &Inst| i.m[0][3] * i.m[0][3] + i.m[1][3] * i.m[1][3] + i.m[2][3] * i.m[2][3];
            f.scenery.sort_by(|a, b| d2(&a.1).total_cmp(&d2(&b.1)));
            let n = f.scenery.len().min(MAX_SCENERY);
            let (order, ranges) = group(lib.ranges.len(), &mut f.scenery.iter().take(n).map(|i| i.0));
            let mut data = vec![Inst::zeroed(); n];
            for (k, (_, inst)) in f.scenery.iter().take(n).enumerate() {
                data[order[k] as usize] = *inst;
            }
            q.write_buffer(&self.scenery_buf, 0, bytemuck::cast_slice(&data));
            scenery_ranges = ranges;
        }
        let mut fall_ranges = Vec::new();
        if let Some(lib) = &self.tree_lib {
            let n = f.fall.len().min(MAX_FALL);
            let (order, ranges) = group(lib.ranges.len(), &mut f.fall.iter().take(n).map(|i| i.0));
            let mut data = vec![TreeInst::zeroed(); n];
            for (k, (_, inst)) in f.fall.iter().take(n).enumerate() {
                data[order[k] as usize] = *inst;
            }
            q.write_buffer(&self.fall_buf, 0, bytemuck::cast_slice(&data));
            fall_ranges = ranges;
        }
        let n_near = f.near.len().min(MAX_NEAR);
        q.write_buffer(&self.near_buf, 0, bytemuck::cast_slice(&f.near[..n_near]));
        let loose_slot = n_builds as u32 * 256;
        let stride = OBJ_STRIDE as usize;
        let mut blob = vec![0u8; n_objs * stride];
        for (i, o) in f.objs.iter().take(n_objs).enumerate() {
            let u = ObjU { model: o.model.to_cols_array_2d(), tint: o.tint, glow: o.glow, params: o.params, tear: o.tear, dmg: o.dmg, dmg2: o.dmg2, crumple: o.crumple, crumple_dir: o.crumple_dir, pose: o.pose, flex: [o.flex, 0.0, 0.0, 0.0] };
            blob[i * stride..i * stride + std::mem::size_of::<ObjU>()].copy_from_slice(bytemuck::bytes_of(&u));
        }
        q.write_buffer(&self.obj_ub, 0, &blob);

        crate::gputime::cpu("render.uploads+sort");
        let _ = self.device.poll(wgpu::PollType::Poll);
        self.fluid.poll();
        let t = &self.targets;
        let mut enc = self.device.create_command_encoder(&Default::default());
        if let Some(c) = self.frame_clock.as_mut() {
            c.begin(&mut enc);
        }
        let mut gt = self.gpu_timer.take();
        let gtr = gt.as_ref();
        let mut mark = |enc: &mut wgpu::CommandEncoder, l: &'static str| {
            if let Some(g) = gtr {
                g.mark(enc, l);
            }
        };
        // (a stamp inside a render pass: the time since the last stamp)
        let rmark = |rp: &mut wgpu::RenderPass<'_>, l: &'static str| {
            if let Some(g) = gtr {
                g.mark_rp(rp, l);
            }
        };
        mark(&mut enc, "start");
        match &f.fluid {
            Some(ff) => self.fluid.compute(q, &mut enc, ff),
            None => self.fluid.live = false,
        }
        mark(&mut enc, "fire.sim");
        // the wave fft only runs while some water is drawn.
        if let Some(o) = &f.ocean {
            self.ocean.compute(q, &mut enc, o);
        }
        mark(&mut enc, "ocean.fft");
        if f.g.weather[3] != self.lut_haze {
            self.lut_haze = f.g.weather[3];
            self.lut_bake.draw(&mut enc);
        }
        mark(&mut enc, "sky.lut_bake");
        if f.sim_steps && parts {
            let mut cp = enc.begin_compute_pass(&Default::default());
            cp.set_pipeline(&self.p_psim);
            cp.set_bind_group(0, &self.bg0, &[]);
            cp.set_bind_group(1, &self.bg_psim, &[]);
            cp.dispatch_workgroups(MAX_PARTICLES / 64, 1, 1);
        }
        mark(&mut enc, "particles.sim");
        if smoke {
            // Smoke volume: splat, smooth, then light it (see vol.wgsl).
            enc.clear_buffer(&self.vol_acc, 0, None);
            if self.fluid.live {
                self.fluid.splat_coarse(&mut enc);
            }
            let mut cp = enc.begin_compute_pass(&Default::default());
            cp.set_bind_group(0, &self.bg0_pre, &[]);
            let g = [VOL_N[0] / 4, VOL_N[1] / 4, VOL_N[2] / 4];
            for (k, p) in self.p_vol.iter().enumerate() {
                cp.set_pipeline(p);
                cp.set_bind_group(1, if k == 4 { &self.bg_vol_b } else { &self.bg_vol_a }, &[]);
                if k == 0 {
                    cp.dispatch_workgroups(MAX_PARTICLES / 64, 1, 1);
                } else {
                    cp.dispatch_workgroups(g[0], g[1], g[2]);
                }
            }
        }
        mark(&mut enc, "smoke.volume");
        let plain_wx = f.g.storm[3] <= 0.0 && f.g.nimbo[0][3] <= 0.0;
        if cs_on {
            self.cshadow.draw(&mut enc, &self.bg0_pre, cs_full, plain_wx);
        }
        mark(&mut enc, "clouds.shadow_map");
        {
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&self.lut_sky, true))], None));
            rp.set_pipeline(&self.p_sky);
            rp.set_bind_group(0, &self.bg0_pre, &[]);
            rp.draw(0..3, 0..1);
        }
        mark(&mut enc, "sky.lut");
        // Stage names for the draws `solids` makes: the main view, then each cascade.
        const SOLID_LABELS: [[&str; 5]; CASCADES + 1] = [
            ["view.buildings", "view.trees", "view.tree_cards+impostors", "view.cars+props", "view.aircraft"],
            ["shadow0.buildings", "shadow0.trees", "shadow0.-", "shadow0.cars+props", "shadow0.aircraft"],
            ["shadow1.buildings", "shadow1.trees", "shadow1.-", "shadow1.cars+props", "shadow1.aircraft"],
            ["shadow2.buildings", "shadow2.trees", "shadow2.-", "shadow2.cars+props", "shadow2.aircraft"],
        ];
        // Draws shared by the shadow cascades (c = Some) and the main view (c = None).
        // part: 1 the aircraft and objects, 2 the rest, 3 both.
        let casc_m: [glam::Mat4; CASCADES] = std::array::from_fn(|c| glam::Mat4::from_cols_array_2d(&f.casc_vp[c]));
        // The view frustum's side planes (camera-relative), and the graphics
        // settings' city detail (common.wgsl `city_k`).
        let frustum = {
            let m = glam::Mat4::from_cols_array_2d(&f.g.view_proj);
            let (r0, r1, r3) = (m.row(0), m.row(1), m.row(3));
            [r3 + r0, r3 - r0, r3 + r1, r3 - r1]
        };
        let city_k = if f.g.detail[0] <= 0.0 { 1.0 } else { f.g.detail[0] };
        use crate::buildings::NCELL;
        let solids = |rp: &mut wgpu::RenderPass, c: Option<usize>, part: u8| {
            let lab = &SOLID_LABELS[c.map_or(0, |c| c + 1)];
            let shadow = c.is_some();
            let bit = c.map(|c| 1u8 << c).unwrap_or(0);
            // The 3D tiles zone (its radius on the ground, centre and up), where
            // the mesh may stand in for buildings and the shader cuts them away.
            let zone = (f.g.photo[0] > 0.0 && f.g.photo[2] > 0.0).then(|| {
                let c = glam::Vec3::from_slice(&f.g.photo_c[..3]);
                (f.g.photo[0], c, (c - glam::Vec3::from_slice(&f.g.planet[..3])).normalize_or_zero())
            });
            // In view, the runs nothing can cut away or dissolve go first with
            // the pipeline that has no discard (early depth testing hides the
            // facades behind them before they are shaded); the rest follow.
            for pass in if part & 2 == 0 { 0..0 } else if shadow { 1..2 } else { 0..2 } {
                rp.set_pipeline(match (shadow, pass) {
                    (true, _) => &self.p_build_shadow,
                    (false, 0) => &self.p_build_whole,
                    _ => &self.p_build,
                });
                let mut runs: Vec<(u32, u32, u32, bool)> = Vec::with_capacity(32);
                for (i, dd) in f.details.iter().take(n_builds).enumerate() {
                    let g = dd.gpu;
                    let (mid, fine) = (dd.u[12] > 0.0, dd.u[13] > 0.0);
                    // Index runs (start, end, instance, whole); the instance index tells
                    // the shader the level of detail in its low 3 bits: 0 always drawn,
                    // 1 middle, 2 finest, 3 full geometry dissolving out with distance,
                    // 4 far stand-ins dissolving in; above them, 4x the distance scale
                    // of the run's height class (buildings::HCLASS). Whole runs have
                    // nothing cut away this frame (DetailDraw::settled).
                    runs.clear();
                    let mut push = |r: (u32, u32, u32, bool)| {
                        if r.1 > r.0 {
                            runs.push(r);
                        }
                    };
                    use crate::buildings::{class_groups, sort_groups_to, GROUPS, HCLASS};
                    let o = glam::Vec3::new(dd.u[0], dd.u[1], dd.u[2]);
                    let in_zone = zone.is_some_and(|(r, zc, up)| {
                        let q = o - zc;
                        (q - up * q.dot(up)).length() < r + dd.radius + 50.0
                    });
                    // Nothing of the tile is fading in, nor inside the 3D tiles zone.
                    let base = dd.u[14] >= 1.0 && !in_zone;
                    let (k_full, k_mid, k_fine) = (dd.reach[0] as usize, if mid { dd.reach[1] as usize } else { 0 }, if fine { dd.reach[2] as usize } else { 0 });
                    let [s_full, s_mid, s_fine] = dd.settled;
                    // Only the cells (buildings::CELLS) a draw can show: in a shadow
                    // cascade those inside its box; in view those in the frustum and,
                    // for the levels that end at a distance, short of it (the vertex
                    // shader would cut every triangle of the rest away).
                    let (cells, near, far) = match c {
                        Some(ci) if dd.casc & bit != 0 => (g.cells_in(&casc_m[ci], o), [0.0; NCELL], [0.0; NCELL]),
                        Some(_) => (0, [0.0; NCELL], [0.0; NCELL]),
                        None if dd.view => g.cells_seen(&frustum, o),
                        None => (0, [0.0; NCELL], [0.0; NCELL]),
                    };
                    if cells == 0 {
                        continue;
                    }
                    // Cells in view with some of them nearer than `d` metres, or
                    // some of them further, for height class `c` (buildings.wgsl `lod_dist`).
                    let short = |c: usize, d: f32| (0..NCELL).filter(|&k| near[k] <= d * HCLASS[c].1 * city_k).fold(0u64, |m, k| m | 1 << k) & cells;
                    let past = |c: usize, d: f32| (0..NCELL).filter(|&k| far[k] >= d * HCLASS[c].1 * city_k).fold(0u64, |m, k| m | 1 << k) & cells;
                    let draw = |push: &mut dyn FnMut((u32, u32, u32, bool)), r: usize, groups: std::ops::Range<usize>, lvl: u32, whole: bool, mask: u64| {
                        g.cell_ranges(r, groups, mask, |s, e| push((s, e, lvl, whole)));
                    };
                    // Height classes `from..to` of level `r`, each with the
                    // level of detail `lvl` in the instance index; the first
                    // `whole` of them are whole.
                    let classes = |push: &mut dyn FnMut((u32, u32, u32, bool)), r: usize, from: usize, to: usize, lvl: u32, whole: u8| {
                        for c in from..to {
                            let mask = match lvl {
                                1 | 3 => short(c, crate::buildings::BFAR[1] + 250.0),
                                2 => short(c, crate::buildings::LOD2_FAR),
                                _ => past(c, crate::buildings::BFAR[0] - 250.0),
                            };
                            draw(push, r, class_groups(r, c), lvl | ((HCLASS[c].1 * 4.0) as u32) << 3, base && c < whole as usize, mask);
                        }
                    };
                    match c {
                        // The far cascade: the stand-ins of all but the lowest buildings.
                        Some(2) => draw(&mut push, 3, 0..4, 0, false, cells),
                        // The second cascade swaps to the stand-ins at their usual distance.
                        Some(1) if k_full < HCLASS.len() => {
                            draw(&mut push, 0, GROUPS - 1..GROUPS, 0, false, cells);
                            draw(&mut push, 3, 0..GROUPS, 0, false, cells);
                        }
                        Some(1) => draw(&mut push, 0, 0..GROUPS, 0, false, cells),
                        Some(_) => {
                            draw(&mut push, 0, 0..GROUPS, 0, false, cells);
                            draw(&mut push, 1, 0..sort_groups_to(k_mid), 1, false, cells);
                            if dd.u[13] > 0.5 {
                                draw(&mut push, 2, 0..sort_groups_to(k_fine), 2, false, cells);
                            }
                        }
                        None => match dd.far {
                            0 => {
                                draw(&mut push, 0, 0..GROUPS, 0, base, cells);
                                classes(&mut push, 1, 0, k_mid, 1, s_mid);
                                classes(&mut push, 2, 0, k_fine, 2, s_fine);
                            }
                            1 => {
                                classes(&mut push, 0, 0, k_full, 3, s_full);
                                draw(&mut push, 0, GROUPS - 1..GROUPS, 0, base, cells);
                                classes(&mut push, 1, 0, k_mid, 1, s_mid);
                                classes(&mut push, 2, 0, k_fine, 2, s_fine);
                                // Stand-ins of the classes wholly short of their dissolve
                                // would only be discarded pixel by pixel: left out.
                                classes(&mut push, 3, (s_full as usize).min(k_full), k_full, 4, 0);
                                // Classes past their full geometry's reach: stand-ins only.
                                draw(&mut push, 3, sort_groups_to(k_full)..GROUPS, 0, base, cells);
                            }
                            _ => {
                                draw(&mut push, 0, GROUPS - 1..GROUPS, 0, base, cells);
                                draw(&mut push, 3, 0..GROUPS, 0, base, cells);
                            }
                        },
                    }
                    let mut runs = runs.iter().filter(|r| shadow || r.3 == (pass == 0)).peekable();
                    if runs.peek().is_none() {
                        continue;
                    }
                    rp.set_bind_group(1, &g.bind, &[i as u32 * 256]);
                    rp.set_vertex_buffer(0, g.vbuf.slice(..));
                    rp.set_index_buffer(g.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    for r in runs {
                        rp.draw_indexed(r.0..r.1, 0, r.2..r.2 + 1);
                    }
                }
            }
            if part & 2 != 0 {
                rmark(rp, lab[0]);
            }
            if let Some(lib) = self.tree_lib.as_ref().filter(|_| part & 2 != 0) {
                rp.set_vertex_buffer(0, lib.vbuf.slice(..));
                rp.set_index_buffer(lib.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                // Shadow cascades draw standing trees from the per-tile buffers (both
                // cascade 0 and cascade 1 pick one LOD per chunk, drawn with fixed mode).
                if shadow {
                    rp.set_pipeline(&self.p_tree_shadow_fixed);
                    for (i, dd) in f.details.iter().take(n_builds).enumerate() {
                        let Some(tb) = &dd.gpu.trees else { continue };
                        let mut bound = false;
                        for t in &dd.trees {
                            if t.imp || t.casc & bit == 0 {
                                continue;
                            }
                            if !bound {
                                rp.set_bind_group(1, &self.bg_tileu, &[i as u32 * 256]);
                                rp.set_vertex_buffer(1, tb.slice(..));
                                bound = true;
                            }
                            let (first, count, base) = lib.ranges[t.mesh as usize];
                            rp.draw_indexed(first..first + count, base, t.first..t.first + t.count);
                        }
                    }
                    rp.set_pipeline(&self.p_tree_shadow);
                } else {
                    rp.set_pipeline(&self.p_tree);
                }
                let loose = c != Some(2) && fall_ranges.iter().any(|r| r.1 > r.0);
                if loose {
                    rp.set_bind_group(1, &self.bg_tileu, &[loose_slot]);
                    rp.set_vertex_buffer(1, self.fall_buf.slice(..));
                    for (k, r) in fall_ranges.iter().enumerate() {
                        if r.1 > r.0 {
                            let (first, count, base) = lib.ranges[k];
                            rp.draw_indexed(first..first + count, base, r.0..r.1);
                        }
                    }
                }
                // What stands close to the camera: in view, both standing trees short of
                // IMP_DIST and undergrowth (ring-sorted front-to-back); in cascade 0,
                // only undergrowth (standing trees already cast via `dd.trees` above).
                let near = |rp: &mut wgpu::RenderPass, cards: bool, only_under: bool| {
                    for (i, dd) in f.details.iter().take(n_builds).enumerate() {
                        let mut bound = false;
                        for nd in dd.close.iter().filter(|nd| nd.cards == cards && (!only_under || !nd.tree)) {
                            let (first, count, base) = lib.ranges[nd.mesh as usize];
                            if count == 0 || nd.first + nd.count > n_near as u32 {
                                continue;
                            }
                            if !bound {
                                rp.set_bind_group(1, &self.bg_tileu, &[i as u32 * 256]);
                                rp.set_vertex_buffer(1, self.near_buf.slice(..));
                                bound = true;
                            }
                            rp.draw_indexed(first..first + count, base, nd.first..nd.first + nd.count);
                        }
                    }
                };
                if c.is_none() || c == Some(0) {
                    if !shadow {
                        rp.set_pipeline(&self.p_tree_near);
                    }
                    near(rp, false, shadow);
                }
                rmark(rp, lab[1]);
                if c.is_none() {
                    rp.set_pipeline(&self.p_card);
                    near(rp, true, false);
                    if loose {
                        rp.set_bind_group(1, &self.bg_tileu, &[loose_slot]);
                        rp.set_vertex_buffer(1, self.fall_buf.slice(..));
                        for (k, r) in fall_ranges.iter().enumerate() {
                            if r.1 > r.0 && k % crate::trees::LODS == 0 {
                                let (first, count, base) = lib.ranges[k + crate::trees::CARDS as usize];
                                rp.draw_indexed(first..first + count, base, r.0..r.1);
                            }
                        }
                    }
                    rmark(rp, "view.tree_cards");
                    // Everything further off: one picture each, drawn ring by ring from
                    // nearest to farthest across tiles so Early-Z / Hi-Z kills occluded crowns.
                    // Specialized pipelines drop photo3d mask/cut code outside the 3D tiles zone
                    // and use a loop-free single-view shader for distant rings (>= 600 m).
                    if self.imp.ready {
                        rp.set_bind_group(2, &self.imp.bg, &[]);
                        rp.set_index_buffer(self.imp.quad.slice(..), wgpu::IndexFormat::Uint32);
                        let has_photo = zone.is_some();
                        if has_photo {
                            rp.set_pipeline(&self.imp.p_draw_photo);
                        }
                        for ring in 0..IMP_RINGS {
                            if !has_photo {
                                if ring == 0 {
                                    rp.set_pipeline(&self.imp.p_draw);
                                } else if ring == 2 {
                                    rp.set_pipeline(&self.imp.p_draw_far);
                                }
                            }
                            for (i, dd) in f.details.iter().take(n_builds).enumerate() {
                                let Some(tb) = &dd.gpu.trees else { continue };
                                let mut bound = false;
                                for t in dd.trees.iter().filter(|t| t.imp && t.ring == ring) {
                                    if !bound {
                                        rp.set_bind_group(1, &self.bg_tileu, &[i as u32 * 256]);
                                        rp.set_vertex_buffer(0, tb.slice(..));
                                        bound = true;
                                    }
                                    rp.draw_indexed(0..6, t.mesh as i32 * 4, t.first..t.first + t.count);
                                }
                            }
                        }
                    }
                    rmark(rp, "view.impostors");
                }
            }
            if c == Some(2) {
                return;
            }
            if let Some(lib) = self.inst_lib.as_ref().filter(|_| part & 2 != 0) {
                rp.set_pipeline(if shadow { &self.p_inst_shadow } else { &self.p_inst });
                rp.set_vertex_buffer(0, lib.vbuf.slice(..));
                rp.set_vertex_buffer(1, self.inst_buf.slice(..));
                rp.set_index_buffer(lib.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                for (k, r) in inst_ranges.iter().enumerate() {
                    // (rebar is too thin to cast a shadow worth its cost)
                    let thin = (crate::cars::M_REBAR..crate::cars::M_REBAR + crate::cars::N_REBAR).contains(&(k as u32));
                    // (what stands on the street is drawn in view after the roads)
                    if r.1 > r.0 && !(shadow && thin) && (shadow || !on_street(k as u32)) {
                        let (first, count, base) = lib.ranges[k];
                        rp.draw_indexed(first..first + count, base, r.0..r.1);
                    }
                }
                rmark(rp, lab[3]);
            }
            if let (Some(lib), Some(bg)) = (self.scenery_lib.as_ref().filter(|_| part & 2 != 0), self.bg_scenery.as_ref()) {
                rp.set_pipeline(if shadow { &self.p_scenery_shadow } else { &self.p_scenery });
                rp.set_bind_group(1, bg, &[]);
                rp.set_vertex_buffer(0, lib.vbuf.slice(..));
                rp.set_vertex_buffer(1, self.scenery_buf.slice(..));
                rp.set_index_buffer(lib.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                for (k, r) in scenery_ranges.iter().enumerate() {
                    if r.1 > r.0 {
                        let (first, count, base) = lib.ranges[k];
                        rp.draw_indexed(first..first + count, base, r.0..r.1);
                    }
                }
            }
            // pipeline set last: 0 generic solid, 1 generic cut, 2 fast solid, 3 fast cut.
            let mut cur: Option<u8> = None;
            let fast = self.obj_fast.as_ref().filter(|_| !shadow);
            // (until the generic pipelines are built, only what the fast ones draw)
            let generic = self.obj_gen.as_ref();
            let rest = generic.and_then(|g| g.rest.as_ref());
            for (i, o) in f.objs.iter().take(if part & 1 == 0 { 0 } else { n_objs }).enumerate() {
                if o.glass || if shadow { o.cast & bit == 0 } else { false } {
                    continue;
                }
                let is_fast = fast.is_some_and(|fp| fp.kind == o.glow[1].round() as u32);
                let ready = if shadow { rest.is_some() } else { generic.is_some() || is_fast };
                if !ready {
                    continue;
                }
                let m = &self.meshes[o.mesh];
                let mut set = |rp: &mut wgpu::RenderPass, want: u8| {
                    if cur != Some(want) {
                        rp.set_pipeline(match (shadow, want) {
                            (true, 1) => &rest.expect("generic").shadow,
                            (true, _) => &rest.expect("generic").shadow_solid,
                            (false, 1) => &generic.expect("generic").obj,
                            (false, 0) => &generic.expect("generic").solid,
                            (false, 3) => &fast.expect("fast").cut,
                            (false, _) => &fast.expect("fast").solid,
                        });
                        cur = Some(want);
                    }
                };
                if m.n_idx == 0 {
                    continue;
                }
                rp.set_bind_group(1, m.bind.as_deref().unwrap_or(&self.bg_obj), &[(i as u64 * OBJ_STRIDE) as u32]);
                rp.set_vertex_buffer(0, m.vbuf.slice(..));
                rp.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                // the lean shader for the kind flown, the full one for the instruments.
                if is_fast {
                    if m.n_main > 0 {
                        set(rp, 2 + o.cut as u8);
                        rp.draw_indexed(0..m.n_main, 0, 0..1);
                    }
                    if m.n_main < m.n_idx && generic.is_some() {
                        set(rp, o.cut as u8);
                        rp.draw_indexed(m.n_main..m.n_idx, 0, 0..1);
                    }
                } else {
                    set(rp, o.cut as u8);
                    rp.draw_indexed(0..m.n_idx, 0, 0..1);
                }
            }
            if part & 1 != 0 {
                rmark(rp, lab[4]);
            }
        };
        // (first: the cascades leave out our scenery the mesh stands over)
        // Where the 3D tiles are, seen from above: our own scenery gives way there.
        if n_photo > 0 || self.photo_mask_dirty {
            let clear = wgpu::RenderPassColorAttachment { view: &self.photo_mask, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: -1e4, b: 0.0, a: -1e4 }), store: wgpu::StoreOp::Store } };
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(clear)], None));
            if n_photo > 0 && f.g.photo[2] > 0.0 {
                rp.set_pipeline(&self.p_photo_mask);
                rp.set_bind_group(0, &self.bg0_pre, &[]);
                rp.set_bind_group(2, &self.bg_casc, &[MASK_SLOT as u32 * 256]);
                for (i, (pg, _)) in f.photo.iter().take(n_photo).enumerate() {
                    rp.set_bind_group(1, &pg.bind, &[i as u32 * 256]);
                    rp.set_vertex_buffer(0, pg.vbuf.slice(..));
                    rp.set_index_buffer(pg.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    rp.draw_indexed(0..pg.n_idx, 0, 0..1);
                }
                rp.set_pipeline(&self.p_water_top);
                rp.set_index_buffer(self.tile_ibuf.slice(..), wgpu::IndexFormat::Uint16);
                for (i, (tile, _)) in f.tiles.iter().take(n_tiles).enumerate() {
                    if !tile.wet { continue; }
                    rp.set_bind_group(1, &tile.bind, &[i as u32 * 256]);
                    rp.set_vertex_buffer(0, tile.vbuf.slice(..));
                    rp.draw_indexed(0..self.tile_nidx, 0, 0..1);
                }
                // Street lamps, once the sun is low enough for them to matter.
                let sun_up = f.g.up[0] * f.g.sun_dir[0] + f.g.up[1] * f.g.sun_dir[1] + f.g.up[2] * f.g.sun_dir[2];
                if sun_up < 0.12 {
                    rp.set_pipeline(&self.p_lamp_top);
                    for (i, dd) in f.details.iter().take(n_builds).enumerate() {
                        if let (true, Some((vb, ib, n))) = (dd.view && dd.near, &dd.gpu.road) {
                            rp.set_bind_group(1, &self.bg_tileu, &[i as u32 * 256]);
                            rp.set_vertex_buffer(0, vb.slice(..));
                            rp.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                            rp.draw_indexed(0..*n, 0, 0..1);
                        }
                    }
                }
            }
            self.photo_mask_dirty = n_photo > 0;
        }
        mark(&mut enc, "photo3d.mask");
        // The shore distance field (shore.rs), a step a frame: our water from
        // above, and where there is ground at all, then the flood.
        if let Some(step) = f.shore {
            if let crate::shore::Step::Raster = step {
                // the square itself, and the wide one around it (for the exposure)
                for (view, slot, half) in [(&self.shore.mask, SHORE_SLOT, crate::shore::HALF), (&self.shore.wide, SHORE_WIDE_SLOT, crate::shore::WIDE_HALF)] {
                    let clear = wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: -1e4, b: 0.0, a: 0.0 }), store: wgpu::StoreOp::Store } };
                    let mut rp = enc.begin_render_pass(&pass_desc(&[Some(clear)], None));
                    rp.set_bind_group(0, &self.bg0_pre, &[]);
                    rp.set_bind_group(2, &self.bg_casc, &[slot as u32 * 256]);
                    rp.set_index_buffer(self.tile_ibuf.slice(..), wgpu::IndexFormat::Uint16);
                    let c = glam::Vec3::from(f.shore_c);
                    let reach = half as f32 * 1.45;
                    let near = |u: &[f32; 12]| (glam::Vec3::new(u[0], u[1], u[2]) - c).length() - u[3] * crate::sources::GRID as f32 * 0.75 < reach;
                    let all = f.tiles.iter().take(n_tiles).chain(f.shore_tiles.iter().take(n_shore_tiles)).enumerate();
                    for (is_water, p) in [(false, &self.p_shore_cover), (true, &self.p_water_top)] {
                        rp.set_pipeline(p);
                        for (i, (tile, u)) in all.clone() {
                            if near(u) && (!is_water || tile.wet) {
                                rp.set_bind_group(1, &tile.bind, &[i as u32 * 256]);
                                rp.set_vertex_buffer(0, tile.vbuf.slice(..));
                                rp.draw_indexed(0..self.tile_nidx, 0, 0..1);
                            }
                        }
                    }
                }
            }
            self.shore.compute(q, &mut enc, step, [f.g.ocean[0], f.g.ocean[1]]);
        }
        mark(&mut enc, "shore.field");
        for c in 0..CASCADES {
            let mut rp = enc.begin_render_pass(&pass_desc(&[], Some(depth_att(&self.shadow[c], 1.0))));
            if f.g.csm[3] > 0.0 {
                rp.set_bind_group(0, &self.bg0_shadow, &[]);
                rp.set_bind_group(2, &self.bg_casc, &[c as u32 * 256]);
                solids(&mut rp, Some(c), 3);
                // The 3D tiles cast their own shadows: those this cascade takes
                // in, and that the next finer one does not wholly cover (the
                // shaders read the finest there is, shadow_world).
                if n_photo > 0 && f.g.photo[2] > 0.0 && f.g.photo_sun[3] >= 0.0 {
                    let mat = |c: usize| {
                        let m = glam::Mat4::from_cols_array_2d(&f.casc_vp[c]);
                        (m, m.row(0).truncate().length(), m.row(1).truncate().length())
                    };
                    let (m, sx, sy) = mat(c);
                    let finer = (c > 0).then(|| mat(c - 1));
                    let zc = glam::Vec3::new(f.g.photo_c[0], f.g.photo_c[1], f.g.photo_c[2]);
                    // (fs_shadow stops casting at the middle of the zone's band)
                    let r_cast = f.g.photo[0] - 0.5 * f.g.photo[1];
                    let n_cut = (f.g.photo[3] as usize).min(crate::tiles3d::MAX_CUTS);
                    let mut fast: Option<bool> = None;
                    for (i, (pg, u)) in f.photo.iter().take(n_photo).enumerate() {
                        // Of a tile and the one it is dissolving into, the
                        // one more than half there casts (no dither here).
                        if u[1][0] < 0.5 {
                            continue;
                        }
                        let o = &u[0];
                        let p = glam::Vec3::new(o[0], o[1], o[2]);
                        let q = m * p.extend(1.0);
                        if q.x.abs() > 1.0 + pg.rad * sx || q.y.abs() > 1.0 + pg.rad * sy {
                            continue;
                        }
                        // (past 7% in from its edge the finer cascade is read alone)
                        if let Some((mf, fx, fy)) = finer {
                            let qf = mf * p.extend(1.0);
                            if qf.x.abs() + pg.rad * fx < 0.86 && qf.y.abs() + pg.rad * fy < 0.86 && qf.z > 0.0 && qf.z < 1.0 {
                                continue;
                            }
                        }
                        let dz = p.distance(zc);
                        if dz - pg.rad > r_cast {
                            continue;
                        }
                        // Depth only, unless something in the tile must not cast:
                        // the outer half of the band, a cut-out building. (Its
                        // still water casts: our water looks above it, composite.wgsl.)
                        let plain = dz + pg.rad < r_cast && (0..n_cut).all(|k| p.distance(glam::Vec3::from_slice(&f.g.cut[k * 3][..3])) > pg.rad + f.g.cut[k * 3][3]);
                        if fast != Some(plain) {
                            rp.set_pipeline(if plain { &self.p_photo_shadow_fast } else { &self.p_photo_shadow });
                            fast = Some(plain);
                        }
                        rp.set_bind_group(1, &pg.bind, &[i as u32 * 256]);
                        rp.set_vertex_buffer(0, pg.vbuf.slice(..));
                        rp.set_index_buffer(pg.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                        rp.draw_indexed(0..pg.n_idx, 0, 0..1);
                    }
                    rmark(&mut rp, ["shadow0.photo3d", "shadow1.photo3d", "shadow2.photo3d"][c.min(2)]);
                }
            }
            drop(rp);
            mark(&mut enc, ["shadow0.pass_end", "shadow1.pass_end", "shadow2.pass_end"][c.min(2)]);
        }

        {
            // The aircraft's own fine shadow map.
            let mut rp = enc.begin_render_pass(&pass_desc(&[], Some(depth_att(&self.shadow[PLANE_LAYER], 1.0))));
            if let (true, Some(generic)) = (f.g.pshadow[3] > 0.0, self.obj_gen.as_ref().and_then(|g| g.rest.as_ref())) {
                let r = self.plane_shadow_res() as f32;
                rp.set_viewport(0.0, 0.0, r, r, 0.0, 1.0);
                rp.set_bind_group(0, &self.bg0_pre, &[]);
                rp.set_bind_group(2, &self.bg_casc, &[PLANE_LAYER as u32 * 256]);
                for (i, o) in f.objs.iter().take(n_objs).enumerate() {
                    if o.glass || o.cast & (1 << PLANE_LAYER) == 0 {
                        continue;
                    }
                    rp.set_pipeline(if o.cut { &generic.shadow } else { &generic.shadow_solid });
                    let m = &self.meshes[o.mesh];
                    if m.n_idx == 0 {
                    continue;
                }
                rp.set_bind_group(1, m.bind.as_deref().unwrap_or(&self.bg_obj), &[(i as u64 * OBJ_STRIDE) as u32]);
                    rp.set_vertex_buffer(0, m.vbuf.slice(..));
                    rp.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    rp.draw_indexed(0..m.n_idx, 0, 0..1);
                }
            }
        }
        mark(&mut enc, "shadow.aircraft_fine");
        {
            // The composite reads original samples at water/foreground boundaries.
            let att = |v, r| wgpu::RenderPassColorAttachment { view: v, depth_slice: None, resolve_target: (msaa() > 1).then_some(r), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } };
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(att(&t.color_ms, &t.color)), Some(att(&t.aux_ms, &t.aux)), Some(att(&t.amb_ms, &t.amb))], Some(depth_att(&t.depth_ms, 0.0))));
            rp.set_bind_group(0, &self.bg0, &[]);
            // the aircraft first, so early depth testing skips what the cockpit hides.
            solids(&mut rp, None, 1);
            // The 3D tiles first: up close they hide most of what follows.
            if n_photo > 0 {
                rp.set_pipeline(&self.p_photo);
                for (i, (pg, _)) in f.photo.iter().take(n_photo).enumerate() {
                    rp.set_bind_group(1, &pg.bind, &[i as u32 * 256]);
                    rp.set_vertex_buffer(0, pg.vbuf.slice(..));
                    rp.set_index_buffer(pg.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    rp.draw_indexed(0..pg.n_idx, 0, 0..1);
                }
                rmark(&mut rp, "view.photo3d");
            }
            // Destroyed buildings: their own mesh, on the pieces they came down in.
            if !f.skins.is_empty() {
                rp.set_pipeline(&self.p_skin);
                for (sk, first) in &f.skins {
                    if (*first as usize) + SKIN_SLOTS > SKIN_SLOT_BUF + SKIN_SLOTS || (*first as usize) >= n_slots.max(1) {
                        continue;
                    }
                    rp.set_bind_group(1, &sk.bind, &[first * 64]);
                    rp.set_vertex_buffer(0, sk.vbuf.slice(..));
                    rp.draw(0..sk.n, 0..1);
                }
                rmark(&mut rp, "view.destroyed_buildings");
            }
            // scenery before the ground, so terrain under buildings and trees is never shaded.
            solids(&mut rp, None, 2);
            rp.set_bind_group(0, &self.bg0, &[]);
            let photo_zone = (f.g.photo[0] > 0.0 && f.g.photo[2] > 0.0).then(|| {
                let c = glam::Vec3::from_slice(&f.g.photo_c[..3]);
                (f.g.photo[0], c, (c - glam::Vec3::from_slice(&f.g.planet[..3])).normalize_or_zero())
            });
            let in_photo = |u: &[f32; 12]| {
                photo_zone.is_some_and(|(r, zc, up)| {
                    let q = glam::Vec3::new(u[0], u[1], u[2]) - zc;
                    let tile_r = u[3] * crate::sources::GRID as f32 * 0.8 + 50.0;
                    (q - up * q.dot(up)).length() < r + tile_r
                })
            };
            let tess = |u: &[f32; 12]| u[10] >= 1.0 && u[11] >= 1.0;
            let grid = crate::sources::GRID as u32;
            // Draw nearest (tessellated) terrain first so it populates depth/Hi-Z before farther tiles.
            let mut groups = f.tess.groups.iter().peekable();
            for (i, (tile, u)) in f.tiles.iter().take(n_tiles).enumerate() {
                if !tess(u) {
                    continue;
                }
                let ph = in_photo(u);
                rp.set_bind_group(1, &tile.bind, &[i as u32 * 256]);
                // Each cell of the block as the smallest patch that makes the same triangles (nearest first).
                rp.set_pipeline(if ph { &self.p_terrain_tess_photo } else { &self.p_terrain_tess });
                rp.set_index_buffer(self.tess_ibuf.slice(..), wgpu::IndexFormat::Uint16);
                while groups.next_if(|g| (g.0 as usize) < i).is_some() {}
                while let Some(&(_, k, first, n)) = groups.next_if(|g| g.0 as usize == i) {
                    rp.set_vertex_buffer(0, self.tess_buf.slice(first as u64 * 4..(first + n) as u64 * 4));
                    rp.draw_indexed(self.tess_idx[k as usize].clone(), 0, 0..n);
                }
                // Middle rows spanning the tessellated block use vs_cells; rows above/below + skirts share vertices via tile_ibuf.
                let y0 = (u[9] as u32).min(grid);
                let y1 = ((u[9] + u[11]) as u32).min(grid);
                if y1 > y0 {
                    rp.set_pipeline(if ph { &self.p_terrain_cells_photo } else { &self.p_terrain_cells });
                    rp.set_index_buffer(self.cell_ibuf.slice(..), wgpu::IndexFormat::Uint16);
                    rp.draw_indexed(0..6, 0, (y0 * grid)..(y1 * grid));
                }
                rp.set_pipeline(if tile.all_water { &self.p_terrain_water } else if ph { &self.p_terrain_photo } else { &self.p_terrain });
                rp.set_vertex_buffer(0, tile.vbuf.slice(..));
                rp.set_index_buffer(self.tile_ibuf.slice(..), wgpu::IndexFormat::Uint16);
                if y0 > 0 {
                    rp.draw_indexed(0..(y0 * grid * 6), 0, 0..1);
                }
                rp.draw_indexed((y1 * grid * 6)..self.tile_nidx, 0, 0..1);
            }
            rmark(&mut rp, "view.terrain_tess");
            let det_far = if f.g.quality[0] > 1.5 { 500.0f32 } else { 1000.0f32 };
            rp.set_index_buffer(self.tile_ibuf.slice(..), wgpu::IndexFormat::Uint16);
            let mut cur_pipe: u8 = 255;
            for (i, (tile, u)) in f.tiles.iter().take(n_tiles).enumerate() {
                if tess(u) {
                    continue;
                }
                let kind = if tile.all_water {
                    0u8
                } else if in_photo(u) {
                    1
                } else if glam::Vec3::new(u[0], u[1], u[2]).length() - tile.radius > det_far + 10.0 {
                    2
                } else {
                    3
                };
                if cur_pipe != kind {
                    rp.set_pipeline(match kind {
                        0 => &self.p_terrain_water,
                        1 => &self.p_terrain_photo,
                        2 => &self.p_terrain_far,
                        _ => &self.p_terrain,
                    });
                    cur_pipe = kind;
                }
                rp.set_bind_group(1, &tile.bind, &[i as u32 * 256]);
                rp.set_vertex_buffer(0, tile.vbuf.slice(..));
                rp.draw_indexed(0..self.tile_nidx, 0, 0..1);
            }
            rmark(&mut rp, "view.terrain");
            // Airfield pavement, then roads, straight onto the terrain.
            rp.set_pipeline(&self.p_field);
            for (i, (fg, _)) in f.fields.iter().take(n_fields).enumerate() {
                if fg.n_idx > 0 {
                    rp.set_bind_group(1, &self.bg_tileu, &[field_slot(i)]);
                    rp.set_vertex_buffer(0, fg.vbuf.slice(..));
                    rp.set_index_buffer(fg.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    rp.draw_indexed(0..fg.n_idx, 0, 0..1);
                }
            }
            rmark(&mut rp, "view.airfields");
            rp.set_pipeline(&self.p_road);
            for (i, dd) in f.details.iter().take(n_builds).enumerate() {
                if let (true, Some((vb, ib, n))) = (dd.view && dd.near, &dd.gpu.road) {
                    rp.set_bind_group(1, &self.bg_tileu, &[i as u32 * 256]);
                    rp.set_vertex_buffer(0, vb.slice(..));
                    rp.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    rp.draw_indexed(0..*n, 0, 0..1);
                }
            }
            rmark(&mut rp, "view.roads");
            // Cars and the player stand on the road ribbons, which are decals
            // pulled toward the eye and write no depth: drawn after them, a
            // ribbon a few centimetres proud of the physics surface (a footway
            // crossing, a junction's apron) cannot bury feet or tyres. They
            // take the ribbons' own pull (inst.wgsl), so terrain bumps under
            // the road still lose to them as they lose to the road.
            if let Some(lib) = self.inst_lib.as_ref() {
                rp.set_pipeline(&self.p_inst);
                rp.set_bind_group(0, &self.bg0, &[]);
                rp.set_vertex_buffer(0, lib.vbuf.slice(..));
                rp.set_vertex_buffer(1, self.inst_buf.slice(..));
                rp.set_index_buffer(lib.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                for (k, r) in inst_ranges.iter().enumerate() {
                    if r.1 > r.0 && on_street(k as u32) {
                        let (first, count, base) = lib.ranges[k];
                        rp.draw_indexed(first..first + count, base, r.0..r.1);
                    }
                }
                rmark(&mut rp, "view.street_objects");
            }
        }
        mark(&mut enc, "view.pass_end(msaa_resolve)");
        {
            let mut rp = enc.begin_render_pass(&pass_desc(&[], Some(depth_att(&t.depth, 0.0))));
            rp.set_pipeline(&self.p_dres);
            rp.set_bind_group(0, &t.bg_dres, &[]);
            rp.draw(0..3, 0..1);
        }
        mark(&mut enc, "depth_resolve");
        let full = |enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, clear: bool, p: &wgpu::RenderPipeline, bg1: &wgpu::BindGroup| {
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(target, clear))], None));
            rp.set_pipeline(p);
            rp.set_bind_group(0, &self.bg0, &[]);
            rp.set_bind_group(1, bg1, &[]);
            rp.draw(0..3, 0..1);
        };
        if self.ao {
            full(&mut enc, &t.ao_raw, true, &self.p_gtao, &t.bg_gtao);
            full(&mut enc, &t.ao, true, &self.p_ao_blur, &t.bg_ao_blur);
        } else {
            // No occlusion anywhere.
            let white = wgpu::RenderPassColorAttachment { view: &t.ao, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::WHITE), store: wgpu::StoreOp::Store } };
            drop(enc.begin_render_pass(&pass_desc(&[Some(white)], None)));
        }
        mark(&mut enc, "ao");
        let fast_comp = f.g.up[3] < 90_000.0;
        let has_water = f.ocean.is_some();
        let has_ssr = f.g.misc[2] > 0.5;
        if self.clouds && (has_water || has_ssr) {
            let mut cp = enc.begin_compute_pass(&Default::default());
            cp.set_bind_group(0, &self.bg0, &[]);
            cp.set_bind_group(1, &self.bg_atmlut, &[]);
            cp.set_pipeline(if plain_wx { &self.p_cloud_refl_plain } else { &self.p_cloud_refl });
            cp.dispatch_workgroups(384 / 8, 384 / 8, 1);
        }
        mark(&mut enc, "composite.cloud_refl");
        if fast_comp {
            {
                let mut cp = enc.begin_compute_pass(&Default::default());
                cp.set_bind_group(0, &self.bg0, &[]);
                cp.set_bind_group(1, &self.bg_atmlut, &[]);
                cp.set_pipeline(&self.p_ap_build);
                cp.dispatch_workgroups(96 / 8, 54u32.div_ceil(8), 1);
                cp.set_pipeline(&self.p_sky_build);
                cp.dispatch_workgroups(256 / 8, 144 / 8, 1);
                if has_ssr {
                    let (rw, rh) = (t.depth_tex.width(), t.depth_tex.height());
                    cp.set_pipeline(&self.p_hiz_build);
                    cp.set_bind_group(0, &t.bg_hiz, &[]);
                    cp.dispatch_workgroups(rw.div_ceil(8), rh.div_ceil(8), 1);
                }
            }
            mark(&mut enc, "composite.atmlut");
            {
                let mut rp = enc.begin_render_pass(&pass_desc(&[], Some(depth_att(&t.comp_cls_depth, 1.0))));
                rp.set_bind_group(0, &self.bg0, &[]);
                rp.set_bind_group(1, &t.bg_comp_cls, &[]);
                rp.set_pipeline(&self.p_comp_cls);
                rp.draw(0..3, 0..1);
            }
            mark(&mut enc, "composite.classify");
            {
                let ds = wgpu::RenderPassDepthStencilAttachment {
                    view: &t.comp_cls_depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                };
                let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.hdr, true))], Some(ds)));
                rp.set_bind_group(0, &self.bg0, &[]);
                rp.set_pipeline(&self.p_comp_sky);
                rp.set_bind_group(1, &self.bg_comp_sky, &[]);
                rp.draw(0..3, 11..12);
                rmark(&mut rp, "composite.sky");
                rp.set_bind_group(1, &t.bg_comp_main, &[]);
                rp.set_pipeline(&self.p_comp_opaque);
                rp.draw(0..3, 4..5);
                rp.draw(0..3, 12..13);
                rmark(&mut rp, "composite.opaque");
                if has_ssr {
                    rp.set_pipeline(&self.p_comp_glass);
                    rp.draw(0..3, 5..6);
                    rp.draw(0..3, 13..14);
                    rmark(&mut rp, "composite.glass");
                }
                if has_water {
                    rp.set_pipeline(&self.p_comp_water);
                    rp.draw(0..3, 6..7);
                    rmark(&mut rp, "composite.water");
                    if let Some(p) = &self.p_comp_edge {
                        rp.set_bind_group(1, &t.bg_comp_edge, &[]);
                        rp.set_pipeline(p);
                        rp.draw(0..3, 7..8);
                        rmark(&mut rp, "composite.edge");
                    }
                }
            }
            mark(&mut enc, "composite.pass_end");
        } else {
            full(&mut enc, &t.hdr, true, &self.p_composite, &t.bg_comp_globe);
            // (one sample a pixel: no water edges to shade sample by sample)
            if let Some(p) = &self.p_composite_edge {
                full(&mut enc, &t.hdr, false, p, &t.bg_comp_edge);
            }
            mark(&mut enc, "composite(lighting+water+sky)");
        }
        // Lights over the lit scene, under the clouds.
        if n_fields > 0 || n_dyn > 0 {
            let ds = wgpu::RenderPassDepthStencilAttachment { view: &t.depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }), stencil_ops: None };
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.hdr, false))], Some(ds)));
            rp.set_bind_group(0, &self.bg0, &[]);
            rp.set_pipeline(&self.p_lights);
            for (i, (fg, _)) in f.fields.iter().take(n_fields).enumerate() {
                if let Some((buf, n)) = &fg.lights {
                    rp.set_bind_group(1, &self.bg_tileu, &[field_slot(i)]);
                    rp.set_vertex_buffer(0, buf.slice(..));
                    rp.draw(0..6, 0..*n);
                }
            }
            if n_dyn > 0 {
                rp.set_bind_group(1, &self.bg_tileu, &[loose_slot]);
                rp.set_vertex_buffer(0, self.dyn_lights.slice(..));
                rp.draw(0..6, 0..n_dyn as u32);
            }
        }
        mark(&mut enc, "lights");
        if !self.clouds {
            // No clouds drawn: their textures go unused, and the history
            // starts afresh once they are back.
            self.cloud_hist_ok = false;
        } else if !plain_wx && f.g.nimbo[0][3] <= 0.0 {
            // 1. Classify half-res cloud pixels and cache storm_mask bounds in
            //    cloud_cls_data; depth 1.0 marks storm-intersecting pixels.
            {
                let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.cloud_cls_data, true))], Some(depth_att(&t.cloud_cls_depth, 1.0))));
                rp.set_pipeline(&self.p_cloud_classify);
                rp.set_bind_group(0, &self.bg0, &[]);
                rp.set_bind_group(1, &t.bg_cloud, &[]);
                rp.draw(0..3, 0..1);
            }
            mark(&mut enc, "clouds.storm_classify");
            // 2. Pure-storm march on storm-intersecting pixels via Early-Z into
            //    (cloud, cloud_dist); untouched pixels clear to (0, 0, 0, T=1).
            {
                let ds = wgpu::RenderPassDepthStencilAttachment {
                    view: &t.cloud_cls_depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                };
                let storm_clear = wgpu::RenderPassColorAttachment {
                    view: &t.cloud,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }), store: wgpu::StoreOp::Store },
                };
                let mut rp = enc.begin_render_pass(&pass_desc(&[Some(storm_clear), Some(color_att(&t.cloud_dist, true))], Some(ds)));
                rp.set_bind_group(0, &self.bg0, &[]);
                rp.set_bind_group(1, &t.bg_cloud_storm, &[]);
                rp.set_pipeline(&self.p_cloud_storm_cls);
                rp.draw(0..3, 0..1);
            }
            mark(&mut enc, "clouds.storm_cell_march");
            // 3. Lean ordinary-cloud march + depth-ordered storm/alto composite
            //    + aerial haze + cirrus into (cloud_raw, cloud_dist_raw).
            {
                let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.cloud_raw, true)), Some(color_att(&t.cloud_dist_raw, true))], None));
                rp.set_bind_group(0, &self.bg0, &[]);
                rp.set_bind_group(1, &t.bg_cloud_plain_comp, &[]);
                rp.set_pipeline(&self.p_cloud_plain_cls);
                rp.draw(0..3, 0..1);
            }
            mark(&mut enc, "clouds.cumulus+alto+cirrus_march(storm_wx)");
            // 4. Sub-cloud storm rain shafts composited into (cloud, cloud_dist).
            {
                let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.cloud, true)), Some(color_att(&t.cloud_dist, true))], None));
                rp.set_bind_group(0, &self.bg0, &[]);
                rp.set_bind_group(1, &t.bg_cloud_rain, &[]);
                rp.set_pipeline(&self.p_cloud_rain);
                rp.draw(0..3, 0..1);
            }
            mark(&mut enc, "clouds.storm_rain_shafts");
        } else {
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.cloud, true)), Some(color_att(&t.cloud_dist, true))], None));
            rp.set_pipeline(if plain_wx { &self.p_cloud_plain } else { &self.p_cloud });
            rp.set_bind_group(0, &self.bg0, &[]);
            rp.set_bind_group(1, &t.bg_cloud, &[]);
            rp.draw(0..3, 0..1);
            drop(rp);
            mark(&mut enc, if plain_wx { "clouds.cumulus+alto+cirrus_march" } else { "clouds.nimbostratus_march" });
        }
        let cf = self.cloud_flip;
        if self.clouds {
            full(&mut enc, &t.cloud_hist[cf], true, &self.p_cloud_temporal, &t.bg_cloud_temporal[cf]);
            mark(&mut enc, "clouds.temporal");
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.hdr, false))], None));
            rp.set_bind_group(0, &self.bg0, &[]);
            rp.set_pipeline(&self.p_cloud_apply);
            rp.set_bind_group(1, &t.bg_cloud_apply[cf], &[]);
            rp.draw(0..3, 0..1);
        }
        mark(&mut enc, "clouds.apply");
        if n_cnode > 0 && n_cseg > 0 {
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.hdr, false))], None));
            rp.set_bind_group(0, &self.bg0, &[]);
            rp.set_bind_group(1, &t.bg_cloud_apply[cf], &[]);
            rp.set_bind_group(2, &self.bg_contrails, &[]);
            rp.set_pipeline(&self.p_contrails);
            rp.draw(0..6, 0..n_cseg as u32);
        }
        mark(&mut enc, "contrails");
        if parts {
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.hdr, false))], None));
            rp.set_bind_group(0, &self.bg0, &[]);
            rp.set_pipeline(&self.p_particles);
            rp.set_bind_group(1, &t.bg_particles, &[]);
            // Ring order = age order, so blending stays stable without sorting.
            rp.draw(0..6, self.phead..MAX_PARTICLES);
            if self.phead > 0 {
                rp.draw(0..6, 0..self.phead);
            }
        }
        mark(&mut enc, "particles.draw");
        // The fire is drawn over the particles: sparks and leaves behind or
        // inside it are hidden by it (those in front are dimmed with them).
        if self.fluid.live {
            let ft = &self.fluid_t;
            {
                let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&ft.half, true)), Some(color_att(&ft.half_z, true))], None));
                rp.set_bind_group(0, &self.bg0, &[]);
                rp.set_pipeline(&self.fluid.p_draw);
                rp.set_bind_group(1, &ft.bg_march, &[]);
                rp.draw(0..3, 0..1);
            }
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.hdr, false))], None));
            rp.set_bind_group(0, &self.bg0, &[]);
            rp.set_pipeline(&self.fluid.p_up);
            rp.set_bind_group(1, &ft.bg_up, &[]);
            rp.draw(0..3, 0..1);
        }
        mark(&mut enc, "fire.draw");
        if f.rain_drops > 0 || n_bolt > 0 {
            let ds = wgpu::RenderPassDepthStencilAttachment { view: &t.depth, depth_ops: None, stencil_ops: None };
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.hdr, false))], Some(ds)));
            rp.set_bind_group(0, &self.bg0, &[]);
            rp.set_bind_group(1, &t.bg_cloud_apply[cf], &[]);
            if f.rain_drops > 0 {
                rp.set_pipeline(&self.p_veil);
                rp.draw(0..3, 0..1);
            }
            if n_bolt > 0 {
                rp.set_pipeline(&self.p_bolt);
                rp.set_vertex_buffer(0, self.bolt_buf.slice(..));
                rp.draw(0..6, 0..n_bolt as u32);
            }
            if f.rain_drops > 0 {
                rp.set_pipeline(&self.p_rain);
                rp.draw(0..6, 0..f.rain_drops);
            }
        }
        mark(&mut enc, "rain+lightning");
        if let (true, Some(generic)) = (f.objs.iter().take(n_objs).any(|o| o.glass), self.obj_gen.as_ref().and_then(|g| g.rest.as_ref())) {
            // The windows bend what is seen through them, so they read the
            // scene from a copy. The depth stays read-only: it is sampled too.
            let (w, h) = (t.hdr_tex.width(), t.hdr_tex.height());
            enc.copy_texture_to_texture(t.hdr_tex.as_image_copy(), t.scene_tex.as_image_copy(), wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
            let ds = wgpu::RenderPassDepthStencilAttachment { view: &t.depth, depth_ops: None, stencil_ops: None };
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(&t.hdr, false))], Some(ds)));
            rp.set_bind_group(0, &self.bg0, &[]);
            rp.set_bind_group(2, &t.bg_glass, &[]);
            rp.set_pipeline(&generic.glass);
            for (i, o) in f.objs.iter().take(n_objs).enumerate() {
                if !o.glass {
                    continue;
                }
                let m = &self.meshes[o.mesh];
                if m.n_idx == 0 {
                    continue;
                }
                rp.set_bind_group(1, m.bind.as_deref().unwrap_or(&self.bg_obj), &[(i as u64 * OBJ_STRIDE) as u32]);
                rp.set_vertex_buffer(0, m.vbuf.slice(..));
                rp.set_index_buffer(m.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..m.n_idx, 0, 0..1);
            }
        }
        mark(&mut enc, "glass");
        // The finished frame: `hdr` itself, or blended into the frames before
        // it (temporal AA), or upscaled from it by FSR 3.1 (upscale.rs).
        let mode = self.up.active();
        let mut fsr_parity = None;
        let src = if mode.temporal() && self.up.t.is_some() {
            // (the temporal upscaler takes the temporal AA's place)
            self.taa_hist_ok = false;
            if let Some(ut) = &self.up.t {
                full(&mut enc, &ut.motion, true, &self.up.p_motion, &ut.bg_motion);
            }
            let reset = !self.up.hist_ok || f.g.cam_delta[3] < 0.5;
            self.up.hist_ok = true;
            let (rw, rh) = (t.hdr_tex.width() as f32, t.hdr_tex.height() as f32);
            let jitter = [f.g.taa[0] * rw, f.g.taa[1] * rh];
            let exposure = f.g.time[2] * AGX_EXPOSURE;
            if mode == crate::graphics::Upscaler::Fsr4 {
                // AMD's runtime records into a command buffer of its own, so
                // what is recorded so far goes first (fsr4.rs).
                let ut = self.up.t.as_ref().expect("upscale targets");
                if let Some(f4) = self.up.fsr4.as_mut() {
                    f4.prepare(q, &mut enc, &self.up.p_touch, exposure);
                    let done = std::mem::replace(&mut enc, self.device.create_command_encoder(&Default::default()));
                    q.submit([done.finish()]);
                    let images = crate::fsr4::rt::Images { color: &t.hdr_tex, depth: &t.depth_tex, motion: &ut.motion_tex, exposure: &f4.exposure, output: &f4.output };
                    // (G.misc.w is 2 tan(fov / 2) / height)
                    let fov_y = 2.0 * (f.g.misc[3] * rh * 0.5).atan();
                    let fr = crate::fsr4::rt::Frame { jitter, near: f.g.misc[0], fov_y, dt: f.g.time[1], reset };
                    if let Err(e) = f4.rt.run(&self.device, q, &images, &fr) {
                        self.up.fsr4_failed(e);
                    }
                }
            } else if let Some(fsr) = self.up.fsr3.as_mut() {
                let frame = crate::fsr3::Frame { jitter, near: f.g.misc[0], exposure, dt: f.g.time[1], reset };
                fsr_parity = fsr.dispatch(q, &mut enc, &frame, &mut mark);
            }
            0
        } else if taa {
            self.up.hist_ok = false;
            full(&mut enc, &t.taa[self.taa_flip], true, &self.p_taa, &t.bg_taa[self.taa_flip]);
            1 + self.taa_flip
        } else {
            self.up.hist_ok = false;
            0
        };
        mark(&mut enc, "aa+upscale");
        for i in 0..BLOOM_MIPS as usize {
            full(&mut enc, &t.bloom[i], true, &self.p_down, if i == 0 { &t.bg_down0[src] } else { &t.bg_down[i - 1] });
        }
        for i in (0..BLOOM_MIPS as usize - 1).rev() {
            full(&mut enc, &t.bloom[i], false, &self.p_up, &t.bg_up[i]);
        }
        mark(&mut enc, "bloom");
        // The tonemap, UI and title go straight into the window's frame when
        // it takes them as they are; `ldr` and a copy to the window otherwise,
        // and for screenshots, which read `ldr` back.
        let direct = sview.as_ref().filter(|_| f.shot.is_none() && direct_out(&self.config));
        let out = direct.unwrap_or(&t.ldr);
        let fsr3_final = if mode == crate::graphics::Upscaler::Fsr4 { self.up.fsr4.as_ref().and_then(|f| f.bg_final.as_ref()) } else { fsr_parity.and_then(|p| self.up.fsr3.as_ref()?.bg_final(p)) };
        match (&self.up.t, fsr3_final) {
            (Some(_), Some(bg)) => full(&mut enc, out, true, &self.up.p_final_fsr3, bg),
            (Some(ut), None) if mode == crate::graphics::Upscaler::Fsr1 => {
                full(&mut enc, &ut.up, true, &self.up.p_easu, &ut.bg_easu[src]);
                full(&mut enc, out, true, &self.up.p_final_fsr1, &ut.bg_final_fsr1);
            }
            _ => full(&mut enc, out, true, if taa { &self.p_final_taa } else { &self.p_final }, &t.bg_final[src]),
        }

        mark(&mut enc, "tonemap");
        let egui_jobs = f.egui.map(|(jobs, delta, ppp)| {
            let sd = egui_wgpu::ScreenDescriptor { size_in_pixels: [self.config.width, self.config.height], pixels_per_point: ppp };
            self.egui.update_buffers(&self.device, &self.queue, &mut enc, &jobs, &sd);
            (jobs, delta, sd)
        });
        if let Some((jobs, _, sd)) = &egui_jobs {
            let mut rp = enc.begin_render_pass(&pass_desc(&[Some(color_att(out, false))], None)).forget_lifetime();
            self.egui.render(&mut rp, jobs, sd);
        }
        mark(&mut enc, "ui");
        if let Some(sview) = sview.as_ref().filter(|_| direct.is_none()) {
            full(&mut enc, sview, true, &self.p_blit, &t.bg_blit);
            mark(&mut enc, "blit");
        }
        let bgra = out_format(&self.config) == wgpu::TextureFormat::Bgra8Unorm;
        let still = f.g.detail[3] < -0.5;
        let shot = f.shot.map(|path| {
            let (w, h) = (self.config.width, self.config.height);
            let row = (w * 4).div_ceil(256) * 256;
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            enc.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo { texture: &t.ldr_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            (path, buf, w, h, row, bgra)
        });
        let t_sub = std::time::Instant::now();
        self.stats[2] += (t_sub - t_enc).as_secs_f64();
        crate::gputime::cpu("render.encode");
        if let Some(g) = gt.as_mut() {
            g.resolve(&mut enc);
        }
        if let Some(c) = self.frame_clock.as_mut() {
            c.end(&mut enc);
        }
        self.queue.submit([enc.finish()]);
        if let Some(c) = self.frame_clock.as_mut() {
            c.after_submit();
        }
        crate::gputime::cpu("render.submit");
        if let Some(g) = gt.as_mut() {
            g.collect(&self.device);
        }
        crate::gputime::cpu("render.profiler_gpu_wait");
        self.gpu_timer = gt;
        self.fluid.after_submit();
        match frame {
            Some(frame) => self.queue.present(frame),
            // Nothing paces a frame that is not shown: wait for the GPU to finish it.
            None => {
                let _ = self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
            }
        }
        self.stats[3] += t_sub.elapsed().as_secs_f64();
        if let Some((_, mut delta, _)) = egui_jobs {
            for id in delta.free.drain() {
                self.egui.free_texture(&id);
            }
        }
        if let Some((path, buf, w, h, row, bgra)) = shot {
            buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = self.device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
            let data = buf.slice(..).get_mapped_range().expect("mapped");
            let mut img = image::RgbaImage::new(w, h);
            for y in 0..h as usize {
                img.as_flat_samples_mut().samples[y * w as usize * 4..(y + 1) * w as usize * 4].copy_from_slice(&data[y * row as usize..y * row as usize + w as usize * 4]);
            }
            if bgra {
                for p in img.pixels_mut() {
                    p.0.swap(0, 2);
                }
            }
            // (Aircraft-picker stills keep the alpha post.wgsl gives them, in a PNG.)
            let img = image::DynamicImage::ImageRgba8(img);
            let png = path.extension().is_some_and(|x| x.eq_ignore_ascii_case("png"));
            let saved = if still && png { img.save(&path) } else { img.to_rgb8().save(&path) };
            match saved {
                Ok(_) => log::info!("saved screenshot {}", path.display()),
                Err(e) => log::error!("screenshot failed: {e}"),
            }
        }
    }
}

/// The format the tonemap, UI and title are drawn in: the window's own when it
/// is plain 8-bit unorm (the same bytes either way, so they can be drawn
/// straight into it), RGBA8 otherwise, which `fs_blit`/`fs_blit_srgb` convert.
fn out_format(config: &wgpu::SurfaceConfiguration) -> wgpu::TextureFormat {
    match config.format {
        f @ (wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm) => f,
        _ => wgpu::TextureFormat::Rgba8Unorm,
    }
}
/// Whether the final passes can draw into the window's frame without the blit.
fn direct_out(config: &wgpu::SurfaceConfiguration) -> bool {
    out_format(config) == config.format && config.usage.contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
}

fn color_att(view: &wgpu::TextureView, clear: bool) -> wgpu::RenderPassColorAttachment<'_> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations { load: if clear { wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT) } else { wgpu::LoadOp::Load }, store: wgpu::StoreOp::Store },
    }
}
fn depth_att(view: &wgpu::TextureView, clear: f32) -> wgpu::RenderPassDepthStencilAttachment<'_> {
    wgpu::RenderPassDepthStencilAttachment { view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store }), stencil_ops: None }
}
fn pass_desc<'a>(colors: &'a [Option<wgpu::RenderPassColorAttachment<'a>>], depth: Option<wgpu::RenderPassDepthStencilAttachment<'a>>) -> wgpu::RenderPassDescriptor<'a> {
    wgpu::RenderPassDescriptor { label: None, color_attachments: colors, depth_stencil_attachment: depth, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.fluid.shutdown(&self.device);
    }
}
