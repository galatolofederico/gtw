//! Graphics settings: how hard the renderer works, as presets (Very Low, Low,
//! Medium, High, Ultra) or tuned one by one. High is what the game always drew
//! before these settings existed; Ultra goes past it (supersampling, a finer
//! shadow map, finer cloud steps, terrain and scenery further out). Very Low is
//! for integrated graphics (a laptop without a GPU of its own): it also turns
//! off the costs that do not shrink with the resolution (the cloud shadow map,
//! MSAA, the water's reflections and sea bed, traffic and birds). Saved in
//! `<config>/fablegame/graphics.json`.
//!
//! Where the time goes (RTX 3060, 1080p, `FABLE_GPU_TIMES=1`, over San
//! Francisco with scattered cloud): High is about 23 ms of GPU work a frame,
//! most of it the deferred composite (water: its reflections and sea bed,
//! 9 ms) and the g-buffer (7 ms), then the clouds (2 ms), the sun's shadow
//! cascades (1.5 ms) and the cloud shadow map (1.3 ms, whatever the
//! resolution). Low is about 6 ms, Very Low about 3.3 ms (2.5 ms with the
//! clouds off). In the cockpit the panel's own shading is most of what is
//! left on Very Low (about 1 ms of its 4.3).
//!
//! Google's photorealistic 3D tiles are not part of this: they are switched on
//! and given their own detail level in the map imagery settings (gmaps.rs).
use crate::i18n::t;
use crate::menu::{self, DIM};
use crate::settings_ui::{note, row};
use egui::RichText;

pub use crate::graphics_core::*;

impl Level {
    /// Its name on the settings screen, in the current language.
    pub fn label(self) -> &'static str {
        crate::i18n::tr(&format!("graphics.level_{}", self.name().to_ascii_lowercase()))
    }
}

impl Preset {
    pub fn label(self) -> &'static str {
        crate::i18n::tr(&format!("graphics.level_{}", self.name().replace(' ', "_").to_ascii_lowercase()))
    }
}

/// Set once at startup when the GPU shares the CPU's memory (graphics.rs
/// only uses it to suggest a preset).
static INTEGRATED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_integrated(on: bool) {
    INTEGRATED.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// The preset a first start picks on an integrated GPU (`vendor`: PCI id):
/// Very Low on Intel's, Low on the others (AMD's APUs, Apple's chips).
pub fn integrated_preset(vendor: u32) -> Preset {
    if vendor == 0x8086 { Preset::VeryLow } else { Preset::Low }
}

impl Graphics {
    /// The water's detail for the composite (Globals.lite.x, composite.wgsl
    /// `water`): 0 plain, 1 full, 2 full with screen-space reflections.
    pub fn water_detail(&self) -> f32 {
        match self.water {
            Level::Off | Level::Low => 0.0,
            Level::Medium => 1.0,
            Level::High | Level::Ultra => 2.0,
        }
    }

    /// Screen-space reflections (of the scenery in water and glass).
    pub fn ssr(&self) -> bool {
        self.water_detail() >= 2.0
    }

    /// Frames a redraw of the cloud shadow map is spread over (render.rs
    /// `CloudShadow`): the slow refresh integrated GPUs get, for Low clouds too.
    pub fn cloud_shadow_slices(&self) -> u32 {
        if matches!(self.clouds, Level::Off | Level::Low) { 64 } else { 16 }
    }

    /// The render resolution the renderer is given: the setting, held to 100%
    /// for the temporal upscalers (they upscale or anti-alias, never supersample).
    pub fn render_scale(&self) -> f32 {
        if self.upscaler.temporal() { self.scale.min(1.0) } else { self.scale }
    }

    /// Side of the sun's shadow map (texels; Off keeps a small unused one).
    pub fn shadow_res(&self) -> u32 {
        match self.shadows {
            Level::Off => 256,
            Level::Low => 1024,
            Level::Medium => 2048,
            Level::High => 4096,
            Level::Ultra => 6144,
        }
    }

    /// Length of the cloud raymarch's steps relative to High (clouds.wgsl).
    pub fn cloud_step(&self) -> f32 {
        self.clouds.pick(1.8, 1.35, 1.0, 0.7) as f32
    }

    /// Storm cell quality tier passed in Globals.detail.w (1 Low .. 4 Ultra;
    /// 0 in WGSL falls back to High).
    pub fn storm_tier(&self) -> f32 {
        match self.storms {
            Level::Off | Level::Low => 1.0,
            Level::Medium => 2.0,
            Level::High => 3.0,
            Level::Ultra => 4.0,
        }
    }

    /// Maximum nearby storm cells uploaded for rendering (nearest first; flight
    /// physics and weather radar still use all cells in `Weather::cells`).
    pub fn storm_max_cells(&self) -> usize {
        match self.storms {
            Level::Off | Level::Low => 8,
            Level::Medium => 12,
            Level::High | Level::Ultra => crate::weather::MAX_STORMS,
        }
    }

    /// Terrain tile split threshold (tile width / distance, world.rs `lod_k`):
    /// lower splits sooner, so finer tiles reach further.
    pub fn lod_k(&self) -> f64 {
        self.terrain.pick(0.95, 0.78, 0.62, 0.5)
    }

    /// Multiplies how far out building (and tree) tiles are loaded.
    pub fn reach(&self) -> f64 {
        self.scenery.pick(0.6, 0.8, 1.0, 1.3)
    }

    /// Terrain tiles kept loaded at most, before the least recently drawn go
    /// (High and Ultra as the game always kept). Each is about 1.6 MB of GPU
    /// memory (imagery, mask, lights, mesh), which an integrated GPU takes
    /// from the system's.
    pub fn tile_cap(&self) -> usize {
        self.terrain.pick(300.0, 450.0, 700.0, 700.0) as usize
    }

    /// Building tiles made at once (each holds 150-250 MB in a city until it
    /// is on the GPU: twelve at once took a 16 GB laptop out of memory).
    pub fn max_build_jobs(&self) -> usize {
        self.scenery.pick(3.0, 5.0, 10.0, 12.0) as usize
    }

    /// Building tiles kept loaded once out of reach, for turning back.
    pub fn spare_builds(&self) -> usize {
        self.scenery.pick(3.0, 6.0, 12.0, 16.0) as usize
    }

    /// Multiplies the distances buildings change level of detail at (their
    /// dissolve into far stand-ins, the dressing, the window depth close up).
    pub fn city_k(&self) -> f32 {
        self.city.pick(0.45, 0.7, 1.0, 1.4) as f32
    }

    /// How far out trees are drawn (m); the last fifth dissolves away.
    pub fn tree_far(&self) -> f32 {
        self.trees.pick(1400.0, 2200.0, 3200.0, 4500.0) as f32
    }

    /// Multiplies the distance trees turn from models into impostors at.
    /// Never above 1: the leaf cards and the impostor bake are made for it.
    pub fn tree_imp_k(&self) -> f32 {
        self.trees.pick(0.5, 0.75, 1.0, 1.0) as f32
    }

    /// Share of the cars on the roads (0..1, by each car's own rank) and how
    /// far off moving and parked cars are drawn (m).
    pub fn car_density(&self) -> (f32, f64, f64) {
        match self.cars {
            Level::Off => (0.0, 0.0, 0.0),
            Level::Low => (0.35, 900.0, 450.0),
            Level::Medium => (0.65, 1500.0, 750.0),
            Level::High | Level::Ultra => (1.0, 2200.0, 1100.0),
        }
    }

    /// The preset row and what the chosen preset does (the settings
    /// screen's Graphics section, settings_ui.rs).
    pub fn preset_ui(&mut self, ui: &mut egui::Ui) {
        row(ui, t!("graphics.preset"), t!("graphics.preset_hint"), |ui| {
            ui.horizontal(|ui| {
                let cur = self.matches();
                let mut pick = cur;
                menu::segments(ui, |ui| {
                    for p in Preset::ALL {
                        menu::seg(ui, &mut pick, Some(p), p.label());
                    }
                });
                if let (Some(p), true) = (pick, pick != cur) {
                    self.apply(p);
                }
                if cur.is_none() {
                    ui.label(RichText::new(t!("graphics.custom")).font(menu::label()).color(DIM));
                }
            });
        });
        note(ui, match self.matches() {
            Some(Preset::VeryLow) => t!("graphics.very_low_note"),
            Some(Preset::Low) => t!("graphics.low_note"),
            Some(Preset::Medium) => t!("graphics.medium_note"),
            Some(Preset::High) => t!("graphics.high_note"),
            Some(Preset::Ultra) => t!("graphics.ultra_note"),
            _ => t!("graphics.custom_note"),
        });
        if INTEGRATED.load(std::sync::atomic::Ordering::Relaxed) && matches!(self.matches(), Some(Preset::Medium | Preset::High | Preset::Ultra) | None) {
            note(ui, t!("graphics.integrated_note"));
        }
        row(ui, t!("graphics.dynamic"), t!("graphics.dynamic_hint"), |ui| {
            menu::switch(ui, &mut self.dynamic);
        });
        if self.dynamic {
            row(ui, t!("graphics.dynamic_target"), t!("graphics.dynamic_target_hint"), |ui| {
                menu::segments(ui, |ui| {
                    for f in DYN_FPS {
                        menu::seg(ui, &mut self.dyn_fps, f, &if f == 0 { t!("graphics.dynamic_display").to_owned() } else { f.to_string() });
                    }
                });
            });
            note(ui, t!("graphics.dynamic_note"));
        }
    }

    /// Shadows, clouds, terrain, scenery, cities, trees, traffic, AO, water,
    /// MSAA: what the presets set.
    pub fn quality_ui(&mut self, ui: &mut egui::Ui) {
        let levels = |ui: &mut egui::Ui, v: &mut Level, off: bool| {
            menu::segments(ui, |ui| {
                if off {
                    menu::seg(ui, v, Level::Off, Level::Off.label());
                }
                for l in Level::PRESETS {
                    menu::seg(ui, v, l, l.label());
                }
            });
        };
        let dim = |ui: &mut egui::Ui, s: String| {
            ui.label(RichText::new(s).font(menu::caption()).color(DIM));
        };
        row(ui, t!("graphics.shadows"), "", |ui| {
            levels(ui, &mut self.shadows, true);
            if self.shadows != Level::Off {
                let r = self.shadow_res();
                dim(ui, t!("graphics.shadow_map", r = r));
            }
        });
        row(ui, t!("graphics.clouds"), "", |ui| {
            levels(ui, &mut self.clouds, true);
            if self.clouds == Level::Off {
                dim(ui, t!("graphics.clouds_off").into());
            }
        });
        row(ui, t!("graphics.cloud_shadows"), t!("graphics.cloud_shadows_hint"), |ui| {
            ui.add_enabled_ui(self.clouds_on(), |ui| menu::switch(ui, &mut self.cloud_shadows));
            if self.clouds_on() && self.cloud_shadows {
                dim(ui, if self.cloud_shadow_slices() > 16 { t!("graphics.cloud_shadows_slow").into() } else { t!("graphics.cloud_shadows_16").into() });
            }
        });
        row(ui, t!("graphics.storm_cells"), t!("graphics.storm_cells_hint"), |ui| {
            levels(ui, &mut self.storms, false);
            dim(ui, match self.storms {
                Level::Off | Level::Low => t!("graphics.storms_low").into(),
                Level::Medium => t!("graphics.storms_medium").into(),
                Level::High => t!("graphics.storms_high").into(),
                Level::Ultra => t!("graphics.storms_ultra").into(),
            });
        });
        row(ui, t!("graphics.contrails"), t!("graphics.contrails_hint"), |ui| {
            menu::switch(ui, &mut self.contrails);
        });
        row(ui, t!("graphics.terrain_detail"), "", |ui| levels(ui, &mut self.terrain, false));
        row(ui, t!("graphics.scenery_distance"), "", |ui| {
            levels(ui, &mut self.scenery, false);
            dim(ui, t!("graphics.scenery_reach", 7.5 * self.reach()));
        });
        row(ui, t!("graphics.city_detail"), "", |ui| {
            levels(ui, &mut self.city, false);
            let k = self.city_k();
            dim(ui, t!("graphics.city_reach", 1.3 * k, 550.0 * k));
        });
        row(ui, t!("graphics.tree_detail"), "", |ui| {
            levels(ui, &mut self.trees, false);
            dim(ui, t!("graphics.tree_reach", self.tree_far() / 1000.0, 120.0 * self.tree_imp_k()));
        });
        row(ui, t!("graphics.traffic"), t!("graphics.traffic_hint"), |ui| {
            menu::segments(ui, |ui| {
                for l in [Level::Off, Level::Low, Level::Medium, Level::High] {
                    menu::seg(ui, &mut self.cars, l, l.label());
                }
            });
            let (share, far, _) = self.car_density();
            dim(ui, if share > 0.0 { t!("graphics.traffic_share", share * 100.0, far / 1000.0) } else { t!("graphics.no_cars").into() });
        });
        row(ui, t!("graphics.birds"), t!("graphics.birds_hint"), |ui| {
            menu::segments(ui, |ui| {
                for l in [Level::Off, Level::Low, Level::Medium, Level::High] {
                    menu::seg(ui, &mut self.birds, l, l.label());
                }
            });
            let (flocks, birds) = crate::birds::caps(self.birds);
            dim(ui, if flocks > 0 { t!("graphics.birds_caps", flocks, birds) } else { t!("graphics.no_birds").to_string() });
        });
        row(ui, t!("graphics.ambient_occlusion"), t!("graphics.ambient_occlusion_hint"), |ui| {
            menu::switch(ui, &mut self.ao);
        });
        row(ui, t!("graphics.water"), t!("graphics.water_hint"), |ui| {
            menu::segments(ui, |ui| {
                for l in [Level::Low, Level::Medium, Level::High] {
                    menu::seg(ui, &mut self.water, l, l.label());
                }
            });
            dim(ui, match self.water {
                Level::Off | Level::Low => t!("graphics.water_low").into(),
                Level::Medium => t!("graphics.water_medium").into(),
                Level::High | Level::Ultra => t!("graphics.water_high").into(),
            });
        });
        row(ui, t!("graphics.msaa"), t!("graphics.msaa_hint"), |ui| {
            menu::segments(ui, |ui| {
                for (n, name) in [(1u8, t!("graphics.level_off")), (2, "2×"), (4, "4×")] {
                    menu::seg(ui, &mut self.msaa, n, name);
                }
            });
            let now = crate::render::msaa();
            if self.msaa as u32 != now {
                dim(ui, t!("graphics.msaa_restart", if now == 1 { t!("graphics.level_off").to_string() } else { format!("{now}×") }));
            }
        });
    }

    /// Render resolution, anti-aliasing, vsync and the frame limit. `window`
    /// is the window's size in pixels (for the resolution readout), `fps`
    /// the frame rate now.
    pub fn display_ui(&mut self, ui: &mut egui::Ui, window: (u32, u32), fps: f64) {
        row(ui, t!("graphics.render_resolution"), t!("graphics.render_resolution_hint"), |ui| {
            let (w, h) = window;
            let (rw, rh) = ((w as f32 * self.scale).round() as u32, (h as f32 * self.scale).round() as u32);
            // (the temporal upscalers never supersample)
            let top = if self.upscaler.temporal() { 1.0 } else { SCALE_MAX };
            self.scale = self.scale.min(top);
            // (clamped and stepped only when dragged, so AMD's 59% and 67% stay exact)
            ui.add(egui::Slider::new(&mut self.scale, SCALE_MIN..=top).step_by(0.05).clamping(egui::SliderClamping::Edits).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            let what = if self.scale > 1.001 { t!("graphics.supersampled").to_owned() } else if self.scale < 0.999 { t!("graphics.upscaled_with", crate::upscale::active(self.upscaler, self.scale).name()) } else { t!("graphics.native").to_owned() };
            ui.label(RichText::new(format!("{rw}×{rh}, {what}")).font(menu::caption()).color(DIM));
        });
        let aa = if crate::upscale::active(self.upscaler, self.render_scale()).temporal() { t!("graphics.anti_aliasing_hint_replaced") } else { t!("graphics.anti_aliasing_hint") };
        row(ui, t!("graphics.anti_aliasing"), aa, |ui| {
            menu::switch(ui, &mut self.taa);
        });
        row(ui, "VSync", if self.vsync { t!("graphics.vsync_on") } else { t!("graphics.vsync_off") }, |ui| {
            menu::switch(ui, &mut self.vsync);
        });
        row(ui, t!("graphics.frame_limit"), t!("graphics.frame_limit_hint"), |ui| {
            menu::segments(ui, |ui| {
                for cap in [0u16, 30, 60, 90, 120, 144, 165, 240] {
                    menu::seg(ui, &mut self.fps_cap, cap, &if cap == 0 { t!("graphics.unlimited").to_owned() } else { cap.to_string() });
                }
            });
        });
        note(ui, &t!("graphics.fps_now", fps = fps));
    }
}

/// What the upscaling card needs to know about the machine.
pub struct UpscaleCx<'a> {
    pub gpu: &'a wgpu::AdapterInfo,
    /// the window's size in pixels
    pub window: (u32, u32),
    /// the upscaler running now (renderer)
    pub active: Upscaler,
    /// why FSR 4.1 cannot run here, if it cannot
    pub fsr4_blocked: Option<String>,
    /// the upscaler AMD's runtime picked, while it runs
    pub fsr4_provider: Option<String>,
}

/// A suggested upscaler and render resolution for a GPU and window.
#[derive(Clone, Debug, PartialEq)]
pub struct Advice {
    pub upscaler: Upscaler,
    pub scale: f32,
    pub why: String,
}

/// The upscaler suited to a GPU, and the quality mode suited to the window
/// (AMD's guidance: Performance at 4K, Balanced at 1440p, Quality at 1080p).
pub fn recommend(vendor: u32, gpu_name: &str, kind: wgpu::DeviceType, window: (u32, u32), fsr4_here: bool) -> Advice {
    let h = window.1.min(window.0);
    let mode = if h >= 2000 { 0.5 } else if h >= 1400 { 1.0 / 1.7 } else { 1.0 / 1.5 };
    let name = gpu_name.to_ascii_lowercase();
    if kind == wgpu::DeviceType::Cpu || ["llvmpipe", "swiftshader", "lavapipe", "basic render", "warp"].iter().any(|s| name.contains(s)) {
        return Advice { upscaler: Upscaler::Bilinear, scale: 0.5, why: "a software renderer: the plain stretch costs least".into() };
    }
    if let Some(model) = crate::fsr4::gpu_support(vendor, gpu_name) {
        return if fsr4_here {
            Advice { upscaler: Upscaler::Fsr4, scale: mode, why: format!("AMD lists this GPU for FSR 4.1 ({model}): AMD's best image") }
        } else {
            Advice { upscaler: Upscaler::Fsr3, scale: mode, why: format!("AMD lists this GPU for FSR 4.1 ({model}), but FSR 4.1 only runs on Windows with the Vulkan renderer; FSR 3.1 is the best that runs here") }
        };
    }
    if kind == wgpu::DeviceType::IntegratedGpu {
        return Advice { upscaler: Upscaler::Fsr1, scale: if h >= 1400 { 1.0 / 1.7 } else { 1.0 / 1.5 }, why: "an integrated GPU: FSR 1 sharpens the upscale for very little, where FSR 3.1's extra passes cost more of a shared memory bus".into() };
    }
    let who = match vendor {
        0x10DE => "an NVIDIA GPU (the game has no DLSS)",
        0x8086 => "an Intel GPU (the game has no XeSS)",
        0x1002 => "an AMD GPU older than FSR 4.1 supports",
        _ => "this GPU",
    };
    Advice { upscaler: Upscaler::Fsr3, scale: mode, why: format!("{who}: FSR 3.1 gives the best image that runs on it") }
}

impl Upscaler {
    /// What it is and when it is the right choice (settings card).
    pub fn about(self) -> &'static str {
        match self {
            Upscaler::Bilinear => "The cheapest: the frame is stretched after the game's own temporal anti-aliasing. Fine near 100%; soft below about 80%.",
            Upscaler::Fsr1 => "AMD FSR 1, built in (MIT): finds edges and upsamples along them, then sharpens. Costs very little; best for integrated GPUs and older cards. It upscales the anti-aliased frame, so keep anti-aliasing on.",
            Upscaler::Fsr3 => "AMD FSR 3.1, built in (a port of AMD's MIT shaders): rebuilds detail from past frames with motion vectors and takes the place of the temporal anti-aliasing. The best image on GPUs without FSR 4.1; at 100% it is AMD's Native AA. Costs more than FSR 1 (time it in the frame-time readout).",
            Upscaler::Fsr4 => "AMD FSR 4.1: a neural network trained by AMD, the sharpest and steadiest in motion. Runs on Windows with the Vulkan renderer on a Radeon RX 6000 or newer, a Ryzen AI 300/400 or Z2 (AMD's list); AMD's figures: 1.3 ms at 4K Performance on an RX 9070 XT, 2.2 ms at 1080p on an RX 6600. Not shipped with the game: downloaded from AMD under AMD's licence. Where it cannot run, FSR 3.1 runs instead.",
        }
    }
}

impl Graphics {
    /// The upscaling card: the upscaler, AMD's quality modes, what suits this
    /// machine, and FSR 4.1's download (settings_ui.rs).
    pub fn upscaler_ui(&mut self, ui: &mut egui::Ui, cx: &UpscaleCx) {
        let advice = recommend(cx.gpu.vendor, &cx.gpu.name, cx.gpu.device_type, cx.window, cx.fsr4_blocked.is_none());
        row(ui, "Upscaler", "how frames below 100% reach the window's size", |ui| {
            let before = self.upscaler;
            menu::segments(ui, |ui| {
                for u in Upscaler::ALL {
                    let label = if u == advice.upscaler { format!("{} ★", u.name()) } else { u.name().to_owned() };
                    menu::seg(ui, &mut self.upscaler, u, &label);
                }
            });
            if self.upscaler != before && self.upscaler.temporal() {
                self.scale = self.scale.min(1.0);
            }
            let now = if cx.active == self.upscaler || crate::upscale::active(self.upscaler, self.render_scale()) == cx.active {
                format!("in use: {}", cx.active.name())
            } else {
                format!("in use: {} ({} needs the render resolution below 100%)", cx.active.name(), self.upscaler.name())
            };
            ui.label(RichText::new(now).font(menu::caption()).color(DIM));
        });
        row(ui, "Quality mode", "AMD's render resolutions", |ui| {
            let (w, h) = cx.window;
            menu::segments(ui, |ui| {
                for (name, k) in QUALITY_MODES {
                    if k >= 1.0 && !self.upscaler.temporal() {
                        continue;
                    }
                    let mut cur = (self.scale - k).abs() < 0.005;
                    let was = cur;
                    menu::seg(ui, &mut cur, true, name);
                    if cur && !was {
                        self.scale = k;
                    }
                }
            });
            let k = self.render_scale();
            ui.label(RichText::new(format!("{:.0}% · {}×{} from {w}×{h}", k * 100.0, (w as f32 * k).round(), (h as f32 * k).round())).font(menu::caption()).color(DIM));
        });
        note(ui, self.upscaler.about());
        if self.upscaler == Upscaler::Fsr4 {
            self.fsr4_ui(ui, cx);
        }
        ui.add_space(6.0);
        let mode = QUALITY_MODES.iter().find(|(_, k)| (k - advice.scale).abs() < 0.005).map_or("", |(n, _)| n);
        row(ui, "Suggested here", &format!("for {}", cx.gpu.name), |ui| {
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("{} at {mode} ({:.0}%): {}.", advice.upscaler.name(), advice.scale * 100.0, advice.why)).font(menu::caption()).color(DIM));
                let here = self.upscaler == advice.upscaler && (self.scale - advice.scale).abs() < 0.005;
                if !here && ui.button("Use the suggestion").clicked() {
                    self.upscaler = advice.upscaler;
                    self.scale = advice.scale;
                }
            });
        });
    }

    fn fsr4_ui(&mut self, ui: &mut egui::Ui, cx: &UpscaleCx) {
        use crate::fsr4::{self, State};
        row(ui, "FSR 4.1 runtime", &format!("AMD FSR SDK {} · not part of the game", fsr4::SDK), |ui| {
            ui.vertical(|ui| {
                if let Some(p) = &cx.fsr4_provider {
                    ui.label(RichText::new(format!("Running: AMD's runtime picked {p}.")).font(menu::caption()).color(DIM));
                }
                if let Some(why) = &cx.fsr4_blocked {
                    ui.label(RichText::new(format!("Can't run here: {why}. FSR 3.1 runs instead.")).font(menu::caption()).color(DIM));
                }
                match fsr4::state() {
                    State::NotInstalled | State::Failed(_) => {
                        if let State::Failed(e) = fsr4::state() {
                            ui.label(RichText::new(format!("Download failed: {e}")).font(menu::caption()).color(DIM));
                        }
                        let label = format!("Download from AMD ({})", crate::bytes(fsr4::download_size()));
                        if ui.button(label).clicked() {
                            fsr4::install();
                        }
                        ui.label(RichText::new("Fetched from AMD's FSR SDK v2.4.0 release on GitHub, checked against pinned SHA-256 sums. AMD's licence (binary use; no reverse engineering) comes with it and is shown after the download.").font(menu::caption()).color(DIM));
                    }
                    State::Downloading(a, b) => {
                        ui.add(egui::ProgressBar::new(a as f32 / b.max(1) as f32).text(format!("Downloading… {} / {}", crate::bytes(a), crate::bytes(b))));
                    }
                    State::Installed => {
                        let folder = fsr4::dir().map(|d| d.display().to_string()).unwrap_or_default();
                        ui.label(RichText::new(format!("Installed (FSR {}, AMD's licence in {folder}).", fsr4::VERSION)).font(menu::caption()).color(DIM));
                        if ui.button("Remove").clicked() {
                            fsr4::remove();
                        }
                    }
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip() {
        for p in Preset::ALL {
            assert_eq!(Graphics::preset(p).matches(), Some(p));
            assert_eq!(Graphics::from_json(&Graphics::preset(p).to_json()), Graphics::preset(p));
            assert_eq!(Preset::parse(p.name()), Some(p));
        }
        assert_eq!(Preset::parse("very-low"), Some(Preset::VeryLow));
        assert_eq!(Preset::parse("VERY_LOW"), Some(Preset::VeryLow));
        assert_eq!(Preset::parse("off"), None);
        let mut g = Graphics::preset(Preset::High);
        g.taa = false;
        g.vsync = false;
        assert_eq!(g.matches(), Some(Preset::High), "taa and vsync are not part of a preset");
        g.scale = 1.2;
        assert_eq!(g.matches(), None);
    }

    #[test]
    fn custom_round_trip() {
        let mut g = Graphics::preset(Preset::Medium);
        g.clouds = Level::Off;
        g.cloud_shadows = false;
        g.water = Level::Low;
        g.msaa = 1;
        g.fps_cap = 30;
        assert_eq!(g.matches(), None);
        assert_eq!(Graphics::from_json(&g.to_json()), g);
        assert!(!g.clouds_on());
    }

    #[test]
    fn high_is_the_old_look() {
        let g = Graphics::default();
        assert_eq!((g.scale, g.shadow_res(), g.cloud_step(), g.lod_k(), g.reach(), g.ao), (1.0, 4096, 1.0, 0.62, 1.0, true));
        assert_eq!((g.storms, g.storm_tier(), g.storm_max_cells(), g.contrails), (Level::High, 3.0, crate::weather::MAX_STORMS, true));
        assert_eq!((g.city_k(), g.tree_far(), g.tree_imp_k(), g.car_density()), (1.0, 3200.0, 1.0, (1.0, 2200.0, 1100.0)));
        assert_eq!((g.clouds_on(), g.cloud_shadows, g.cloud_shadow_slices(), g.msaa, g.ssr(), g.water_detail()), (true, true, 16, 4, true, 2.0));
    }

    #[test]
    fn low_keeps_its_old_look() {
        // Low always had 2x MSAA and no screen-space reflections.
        let g = Graphics::preset(Preset::Low);
        assert_eq!((g.msaa, g.ssr(), g.water_detail(), g.cloud_shadows), (2, false, 1.0, true));
    }

    #[test]
    fn very_low_drops_the_fixed_costs() {
        let g = Graphics::preset(Preset::VeryLow);
        assert_eq!((g.scale, g.shadows, g.ao, g.msaa, g.cloud_shadows, g.contrails, g.water_detail()), (0.5, Level::Off, false, 1, false, false, 0.0));
        assert_eq!((g.cars, g.birds, g.car_density().0), (Level::Off, Level::Off, 0.0));
        assert!(g.clouds_on(), "Very Low keeps the clouds (Off is a choice of its own)");
        assert_eq!(integrated_preset(0x8086), Preset::VeryLow);
        assert_eq!(integrated_preset(0x1002), Preset::Low);
    }

    #[test]
    fn old_saves_load_as_before() {
        // A custom save from before Very Low, the water and MSAA settings.
        let j = serde_json::json!({"preset": "custom", "scale": 0.9, "shadows": "High", "clouds": "Ultra", "storms": "Medium", "ao": true, "fps_cap": 30});
        let g = Graphics::from_json(&j);
        assert_eq!((g.clouds, g.storms, g.cloud_shadows, g.water, g.msaa, g.fps_cap), (Level::Ultra, Level::Medium, true, Level::High, 4, 30));
        assert_eq!(Graphics::from_json(&serde_json::json!({"preset": "Low"})), Graphics::preset(Preset::Low));
    }

    #[test]
    fn upscaler_is_not_part_of_a_preset() {
        let mut g = Graphics::preset(Preset::Medium);
        g.upscaler = Upscaler::Fsr3;
        assert_eq!(g.matches(), Some(Preset::Medium));
        for u in Upscaler::ALL {
            assert_eq!(Upscaler::parse(u.key()), Some(u));
            assert_eq!(Upscaler::parse(u.name()), Some(u));
        }
        // the temporal upscalers never supersample
        let mut g = Graphics::preset(Preset::Ultra);
        g.upscaler = Upscaler::Fsr3;
        assert_eq!(g.render_scale(), 1.0);
        g.upscaler = Upscaler::Fsr1;
        assert_eq!(g.render_scale(), 1.5);
    }

    #[test]
    fn suggestions_follow_the_hardware() {
        use wgpu::DeviceType::{Cpu, DiscreteGpu, IntegratedGpu};
        let nv = |w| recommend(0x10DE, "NVIDIA GeForce RTX 3060", DiscreteGpu, w, false);
        assert_eq!(nv((2560, 1440)).upscaler, Upscaler::Fsr3);
        assert_eq!(nv((3840, 2160)).scale, 0.5);
        assert_eq!(nv((1920, 1080)).scale, 1.0 / 1.5);
        assert_eq!(recommend(0x1002, "AMD Radeon RX 7800 XT", DiscreteGpu, (2560, 1440), true).upscaler, Upscaler::Fsr4);
        assert_eq!(recommend(0x1002, "AMD Radeon RX 7800 XT", DiscreteGpu, (2560, 1440), false).upscaler, Upscaler::Fsr3, "FSR 4.1 only where it can run");
        assert_eq!(recommend(0x8086, "Intel(R) Iris(R) Xe Graphics", IntegratedGpu, (1920, 1080), false).upscaler, Upscaler::Fsr1);
        assert_eq!(recommend(0x10005, "llvmpipe (LLVM 22.1.8, 256 bits)", Cpu, (1920, 1080), false).upscaler, Upscaler::Bilinear);
    }

    #[test]
    fn storm_quality_independent_of_clouds() {
        let mut g = Graphics::preset(Preset::High);
        g.storms = Level::Medium;
        assert_eq!(g.matches(), None, "changing storm quality independently makes a custom profile");
        assert_eq!((g.clouds, g.storms, g.storm_tier(), g.storm_max_cells()), (Level::High, Level::Medium, 2.0, 12));
        g.storms = Level::Low;
        assert_eq!((g.storm_tier(), g.storm_max_cells()), (1.0, 8));
        g.storms = Level::Ultra;
        assert_eq!((g.storm_tier(), g.storm_max_cells()), (4.0, crate::weather::MAX_STORMS));
    }
}
