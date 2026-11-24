//! FableFlight Launcher: a small window that opens at once and gets the game
//! going. It shows how far the game's (slow, shader-compiling) start has got,
//! keeps the game up to date from the GitHub releases with their changelog,
//! sets the graphics before the game loads (with a suggestion for the GPU),
//! and runs a short graphics test to show whether a preset flies smoothly.
//!
//! It lives beside `fablegame` (the release archives hold both) and works on
//! the files in that folder; the graphics settings are the game's own
//! `graphics.json` (src/graphics_core.rs, shared with the game).
//!
//! The pieces: run.rs starts the game and reads its progress lines,
//! update.rs talks to GitHub, gpu.rs suggests a preset, config.rs remembers
//! the launcher's own things, style.rs draws it all in the game's style.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[allow(dead_code)]
#[path = "../../graphics_core.rs"]
mod graphics_core;

mod config;
mod gpu;
mod run;
mod shot;
mod style;
mod update;

use config::{BenchResult, Config, Verdict};
use crossbeam_channel::Receiver;
use egui::{Color32, Rect, RichText, pos2, vec2};
use graphics_core::{Graphics, Level, Preset, Upscaler};
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use style::{ACCENT, BAD, DIM, FAINT, Kind, OK, TEXT, WARN};

const GLOBE: &[u8] = include_bytes!("../../../assets/ui/globe.jpg");
const GITHUB_ICON: &[u8] = include_bytes!("../../../assets/ui/github.png");
const DISCORD_ICON: &[u8] = include_bytes!("../../../assets/ui/discord.png");
const APP_ICON: &[u8] = include_bytes!("../../../assets/ui/launcher_icon.png");

/// How long the graphics test measures for, after the game's settling time.
const BENCH_SECS: u32 = 15;
/// The game's settling time before it measures (main.rs `Bench::SETTLE`).
const BENCH_SETTLE: f64 = 16.0;
/// The graphics test's flight: a Cessna over San Francisco's south side
/// heading north-north-east, so the city, the bay, trees and clouds are all in view.
const BENCH_FLIGHT: [&str; 21] = ["--no-menu", "--start", "plane", "--no-save", "--mute", "--no-mcp", "--aircraft", "c172", "--lat", "37.735", "--lon", "-122.43", "--alt", "650", "--heading", "20", "--time", "15", "--clouds", "0.35", "--no-vsync"];

fn main() -> eframe::Result {
    // FABLE_LAUNCHER_SHOT=out.png: a picture of a page drawn off screen (shot.rs)
    if let Some(path) = std::env::var_os("FABLE_LAUNCHER_SHOT") {
        let page = match std::env::var("FABLE_LAUNCHER_PAGE").as_deref() {
            Ok("graphics") => Page::Graphics,
            Ok("test") => Page::Test,
            Ok("updates") => Page::Updates,
            _ => Page::Home,
        };
        if let Err(e) = shot::render(std::path::Path::new(&path), page) {
            eprintln!("launcher picture: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let icon = image::load_from_memory(APP_ICON).ok().map(|i| {
        let i = i.to_rgba8();
        egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
    });
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("FableFlight Launcher")
        .with_app_id("fableflight-launcher")
        .with_inner_size([1140.0, 720.0])
        .with_min_inner_size([940.0, 620.0]);
    if let Some(i) = icon {
        viewport = viewport.with_icon(std::sync::Arc::new(i));
    }
    // no vsync: on Wayland a vsynced window the game covers waits on the
    // compositor before each frame and so stops running altogether (it
    // repaints only while something is under way, so this costs nothing idle)
    let mut options = eframe::NativeOptions { viewport, centered: true, ..Default::default() };
    options.wgpu_options.surface.present_mode = egui_wgpu::wgpu::PresentMode::AutoNoVsync;
    eframe::run_native("FableFlight Launcher", options, Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx, cc.wgpu_render_state.as_ref().map(|rs| rs.adapter.get_info()))))))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Graphics,
    Test,
    Updates,
}

enum Releases {
    Unknown,
    Checking(Receiver<Result<Vec<update::Release>, String>>),
    Ready(Vec<update::Release>),
    Failed(String),
}

enum UpdMsg {
    Progress(update::Phase, u64, u64),
    Done(Result<update::Installed, String>),
}

struct UpdateJob {
    tag: String,
    rx: Receiver<UpdMsg>,
    phase: update::Phase,
    done: u64,
    total: u64,
}

/// What the game is doing for us.
enum Task {
    Idle,
    /// flying: the start's stages are in `run.tl`; `ready` once its end has
    /// been seen to here (the timings saved, the window minimised)
    Playing { run: run::Running, ready: bool },
    /// the graphics test: the preset under test (None: the saved settings)
    Testing { run: run::Running, preset: Option<Preset>, result: Option<BenchResult> },
}

/// The graphics test's plan.
struct TestPlan {
    /// find the best preset (else test the one setting)
    auto: bool,
    /// the runs done this time
    done: Vec<(Option<Preset>, Option<BenchResult>)>,
    /// once it is over: the preset it found
    best: Option<Preset>,
    cancelled: bool,
}

struct Textures {
    globe: egui::TextureHandle,
    globe_size: egui::Vec2,
    github: egui::TextureHandle,
    discord: egui::TextureHandle,
}

struct App {
    cfg: Config,
    page: Page,
    /// the folder the game is in (the launcher's own)
    dir: PathBuf,
    game: PathBuf,
    /// the launcher as it was started (to restart it after an update)
    me: PathBuf,
    /// run from a cargo `target` folder: updates come from git, not here
    dev_build: bool,
    writable: bool,
    installed: Option<String>,
    version_rx: Option<Receiver<Option<String>>>,
    gpu: Option<gpu::Gpu>,
    screen: Option<(u32, u32)>,
    gfx: Graphics,
    /// graphics.json exists (else the suggestion is applied at Play)
    gfx_on_disk: bool,
    gfx_note: Option<(String, f64)>,
    tex: Textures,
    releases: Releases,
    job: Option<UpdateJob>,
    upd_error: Option<String>,
    restart_needed: bool,
    task: Task,
    plan: Option<TestPlan>,
    log: VecDeque<String>,
    /// how the last run ended, if worth saying
    ended: Option<(String, bool)>,
    started: Instant,
    /// the window, for the game's reading threads to wake
    ctx: Option<egui::Context>,
}

fn texture(ctx: &egui::Context, name: &str, data: &[u8]) -> (egui::TextureHandle, egui::Vec2) {
    let img = image::load_from_memory(data).map(|i| i.to_rgba8()).unwrap_or_else(|_| image::RgbaImage::new(1, 1));
    let size = [img.width() as usize, img.height() as usize];
    let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    (ctx.load_texture(name, ci, egui::TextureOptions::LINEAR), vec2(size[0] as f32, size[1] as f32))
}

/// Run from inside a cargo `target` folder.
fn is_dev_build(dir: &std::path::Path) -> bool {
    dir.ancestors().any(|a| a.file_name().is_some_and(|n| n == "target") && a.parent().is_some_and(|p| p.join("Cargo.toml").exists()))
}

impl App {
    fn new(ctx: &egui::Context, adapter: Option<wgpu::AdapterInfo>) -> Self {
        style::fonts(ctx);
        style::visuals(ctx);
        let (globe, globe_size) = texture(ctx, "globe", GLOBE);
        let tex = Textures { globe, globe_size, github: texture(ctx, "github", GITHUB_ICON).0, discord: texture(ctx, "discord", DISCORD_ICON).0 };
        let me = std::env::current_exe().unwrap_or_else(|_| PathBuf::from(update::launcher_name()));
        let dir = me.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        update::clean_up(&dir);
        let game = dir.join(update::game_name());
        let writable = {
            let probe = dir.join(".fableflight-write-test");
            let ok = std::fs::write(&probe, b"").is_ok();
            let _ = std::fs::remove_file(&probe);
            ok
        };
        let cfg = Config::load();
        let gpu = adapter.as_ref().map(gpu::Gpu::from_info);
        let version_rx = game.exists().then(|| {
            let (tx, rx) = crossbeam_channel::bounded(1);
            let (exe, ctx) = (game.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(update::game_version(&exe));
                ctx.request_repaint();
            });
            rx
        });
        let mut app = App {
            page: Page::Home,
            dev_build: is_dev_build(&dir),
            installed: None,
            version_rx,
            gpu,
            screen: None,
            gfx: Graphics::load(),
            gfx_on_disk: Graphics::saved(),
            gfx_note: None,
            tex,
            releases: Releases::Unknown,
            job: None,
            upd_error: None,
            restart_needed: false,
            task: Task::Idle,
            plan: None,
            log: VecDeque::new(),
            ended: None,
            started: Instant::now(),
            ctx: Some(ctx.clone()),
            cfg,
            dir,
            game,
            me,
            writable,
        };
        if app.cfg.check_updates || !app.game.exists() {
            app.check_updates(ctx);
        }
        app
    }

    // ---- state ----

    fn latest(&self) -> Option<&update::Release> {
        match &self.releases {
            Releases::Ready(r) => r.first(),
            _ => None,
        }
    }

    /// A release newer than the one installed (with a download for this machine).
    fn update_available(&self) -> Option<&update::Release> {
        let latest = self.latest()?;
        latest.archive()?;
        match self.installed.as_deref().and_then(update::version) {
            Some(v) => (update::version(&latest.tag)? > v).then_some(latest),
            None => (!self.game.exists()).then_some(latest),
        }
    }

    fn can_update(&self) -> bool {
        !self.dev_build && self.writable && update::platform().is_some()
    }

    fn busy(&self) -> bool {
        !matches!(self.task, Task::Idle) || self.job.is_some()
    }

    fn recommended(&self) -> Option<(Preset, String)> {
        Some(gpu::recommend(self.gpu.as_ref()?, self.screen.unwrap_or((1920, 1080))))
    }

    fn check_updates(&mut self, ctx: &egui::Context) {
        if matches!(self.releases, Releases::Checking(_)) {
            return;
        }
        let (tx, rx) = crossbeam_channel::bounded(1);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(update::releases());
            ctx.request_repaint();
        });
        self.releases = Releases::Checking(rx);
    }

    fn start_update(&mut self, ctx: &egui::Context) {
        let Some(rel) = self.latest().cloned() else { return };
        if self.busy() || !self.can_update() {
            return;
        }
        let (tx, rx) = crossbeam_channel::unbounded();
        let (dir, ctx) = (self.dir.clone(), ctx.clone());
        self.upd_error = None;
        self.job = Some(UpdateJob { tag: rel.tag.clone(), rx, phase: update::Phase::Downloading, done: 0, total: rel.archive().map_or(1, |a| a.size) });
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(Instant::now());
            let r = update::install(&rel, &dir, &|ph, done, total| {
                if last.get().elapsed() > Duration::from_millis(50) || ph != update::Phase::Downloading {
                    last.set(Instant::now());
                    let _ = tx.send(UpdMsg::Progress(ph, done, total));
                    ctx.request_repaint();
                }
            });
            let _ = tx.send(UpdMsg::Done(r));
            ctx.request_repaint();
        });
    }

    fn save_gfx(&mut self, now: f64, note: &str) {
        self.gfx.save();
        self.gfx_on_disk = true;
        self.gfx_note = Some((note.to_string(), now));
    }

    fn play(&mut self, ctx: &egui::Context) {
        if self.busy() || !self.game.exists() {
            return;
        }
        // a first start: the suggested preset, as the game itself would pick on integrated graphics
        if !self.gfx_on_disk {
            if let Some((p, _)) = self.recommended() {
                self.gfx.apply(p);
            }
            self.gfx.save();
            self.gfx_on_disk = true;
        }
        self.ended = None;
        self.log.clear();
        match run::spawn(&self.game, &[], &self.cfg.stage_ms, Some(ctx.clone())) {
            Ok(r) => self.task = Task::Playing { run: r, ready: false },
            Err(e) => self.ended = Some((format!("Couldn't start {}: {e}", self.game.display()), true)),
        }
        ctx.request_repaint();
    }

    fn start_test(&mut self, auto: bool) {
        if self.busy() || !self.game.exists() {
            return;
        }
        self.ended = None;
        let first = if auto { Some(self.recommended().map_or(Preset::Medium, |r| r.0)) } else { None };
        if !auto {
            // the test reads the saved settings
            self.gfx.save();
            self.gfx_on_disk = true;
        }
        self.plan = Some(TestPlan { auto, done: Vec::new(), best: None, cancelled: false });
        self.spawn_test(first);
    }

    fn spawn_test(&mut self, preset: Option<Preset>) {
        let (w, h) = self.screen.unwrap_or((1920, 1080));
        let mut args: Vec<String> = vec!["--bench".into(), BENCH_SECS.to_string()];
        args.extend(BENCH_FLIGHT.iter().map(|s| s.to_string()));
        args.extend(["--width".into(), w.to_string(), "--height".into(), h.to_string()]);
        if let Some(p) = preset {
            args.extend(["--graphics".into(), p.name().to_ascii_lowercase().replace(' ', "-")]);
        }
        self.log.clear();
        match run::spawn(&self.game, &args, &self.cfg.stage_ms, self.ctx.clone()) {
            Ok(r) => self.task = Task::Testing { run: r, preset, result: None },
            Err(e) => {
                self.ended = Some((format!("Couldn't start the graphics test: {e}"), true));
                self.plan = None;
            }
        }
    }

    /// After a test run: the next one, or the verdict.
    fn next_test(&mut self) {
        let Some(plan) = &mut self.plan else { return };
        if plan.cancelled || !plan.auto {
            plan.best = None;
            return;
        }
        let tested: Vec<(Preset, Verdict)> = plan.done.iter().filter_map(|(p, r)| Some(((*p)?, Verdict::of(r.as_ref()?)))).collect();
        let idx = |p: Preset| Preset::ALL.iter().position(|q| *q == p).unwrap_or(0);
        let best_smooth = tested.iter().filter(|(_, v)| *v == Verdict::Smooth).map(|(p, _)| *p).max_by_key(|p| idx(*p));
        let best_ok = tested.iter().filter(|(_, v)| *v != Verdict::Struggles).map(|(p, _)| *p).max_by_key(|p| idx(*p));
        let untested = |p: Preset| !tested.iter().any(|(q, _)| *q == p);
        let failed_run = plan.done.last().is_some_and(|(_, r)| r.is_none());
        let mut next = None;
        if !failed_run && plan.done.len() < 4 {
            if let Some((p, v)) = tested.last().copied() {
                let i = idx(p);
                if v == Verdict::Smooth && i + 1 < Preset::ALL.len() && untested(Preset::ALL[i + 1]) {
                    next = Some(Preset::ALL[i + 1]);
                } else if v != Verdict::Smooth && best_smooth.is_none() && i > 0 && untested(Preset::ALL[i - 1]) {
                    next = Some(Preset::ALL[i - 1]);
                }
            }
        }
        match next {
            Some(p) => self.spawn_test(Some(p)),
            None => {
                plan.best = best_smooth.or(best_ok).or_else(|| (!tested.is_empty()).then_some(Preset::VeryLow));
            }
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if let Some(rx) = &self.version_rx {
            if let Ok(v) = rx.try_recv() {
                self.installed = v.or_else(|| self.cfg.installed_tag.clone());
                self.version_rx = None;
            }
        }
        if let Releases::Checking(rx) = &self.releases {
            if let Ok(r) = rx.try_recv() {
                self.releases = match r {
                    Ok(list) => Releases::Ready(list),
                    Err(e) => Releases::Failed(e),
                };
            }
        }
        if let Some(job) = &mut self.job {
            let mut finished = None;
            while let Ok(m) = job.rx.try_recv() {
                match m {
                    UpdMsg::Progress(ph, d, t) => (job.phase, job.done, job.total) = (ph, d, t),
                    UpdMsg::Done(r) => finished = Some(r),
                }
            }
            if let Some(r) = finished {
                let tag = job.tag.clone();
                self.job = None;
                match r {
                    Ok(done) => {
                        self.installed = Some(done.tag.clone());
                        self.cfg.installed_tag = Some(done.tag);
                        self.cfg.save();
                        self.restart_needed |= done.launcher_replaced;
                        self.ended = Some((format!("Updated to {tag}."), false));
                    }
                    Err(e) => self.upd_error = Some(e),
                }
            }
        }
        // the game
        let mut exit: Option<Option<i32>> = None;
        let mut minimize = false;
        match &mut self.task {
            Task::Idle => {}
            Task::Playing { run, ready } => {
                while let Ok(ev) = run.rx.try_recv() {
                    match ev {
                        run::Event::Stage => {
                            let tl = run.tl.lock();
                            if tl.ready() && !*ready {
                                *ready = true;
                                self.cfg.stage_ms = tl.record();
                                self.cfg.save();
                                minimize = self.cfg.minimize_on_ready;
                            }
                        }
                        run::Event::Log(l) => push_log(&mut self.log, l),
                        run::Event::Bench(_) => {}
                        run::Event::Exit(c) => exit = Some(c),
                    }
                }
            }
            Task::Testing { run, result, .. } => {
                while let Ok(ev) = run.rx.try_recv() {
                    match ev {
                        run::Event::Stage => {}
                        run::Event::Bench(j) => *result = Some(BenchResult::from_json(&j)),
                        run::Event::Log(l) => push_log(&mut self.log, l),
                        run::Event::Exit(c) => exit = Some(c),
                    }
                }
            }
        }
        if minimize {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        if let Some(code) = exit {
            match std::mem::replace(&mut self.task, Task::Idle) {
                Task::Playing { run, .. } => {
                    let flown = run.started.elapsed().as_secs_f64();
                    self.ended = match (run.tl.lock().ready_at(), code) {
                        (_, Some(0)) => Some((format!("Flight over after {}.", style::secs(flown)), false)),
                        (None, c) => Some((format!("The game stopped while starting{}.", code_text(c)), true)),
                        (Some(_), c) => Some((format!("The game closed with an error{}.", code_text(c)), true)),
                    };
                    if self.cfg.minimize_on_ready {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                }
                Task::Testing { preset, result, .. } => {
                    let cancelled = self.plan.as_ref().is_some_and(|p| p.cancelled);
                    if let Some(r) = &result {
                        self.cfg.bench.push(r.clone());
                        let n = self.cfg.bench.len();
                        if n > 20 {
                            self.cfg.bench.drain(..n - 20);
                        }
                        self.cfg.save();
                    } else if !cancelled {
                        let old = code == Some(2) && self.log.iter().any(|l| l.contains("--bench"));
                        self.ended = Some((if old { "This game version has no graphics test yet: update it first.".into() } else { format!("The graphics test stopped early{}.", code_text(code)) }, true));
                    }
                    if let Some(plan) = &mut self.plan {
                        plan.done.push((preset, result));
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    self.next_test();
                }
                Task::Idle => {}
            }
        }
        if self.gfx_note.as_ref().is_some_and(|(_, t)| now - t > 3.0) {
            self.gfx_note = None;
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
    }
}

fn code_text(c: Option<i32>) -> String {
    match c {
        Some(c) => format!(" (exit code {c})"),
        None => String::new(),
    }
}

fn push_log(log: &mut VecDeque<String>, l: String) {
    if log.len() >= 300 {
        log.pop_front();
    }
    log.push_back(l);
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl App {
    /// The whole window.
    fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.screen.is_none() {
            self.screen = ctx.input(|i| {
                let ppp = i.viewport().native_pixels_per_point.unwrap_or(1.0);
                i.viewport().monitor_size.filter(|s| s.x > 0.0).map(|s| ((s.x * ppp).round() as u32, (s.y * ppp).round() as u32))
            });
        }
        self.poll(&ctx);
        self.background(ui);
        egui::Panel::top("header").frame(egui::Frame::new().inner_margin(egui::Margin { left: 28, right: 28, top: 18, bottom: 10 })).show(ui, |ui| self.header(ui));
        egui::Panel::bottom("footer")
            .frame(egui::Frame::new().fill(Color32::from_rgba_premultiplied(5, 7, 10, 236)).inner_margin(egui::Margin { left: 28, right: 28, top: 14, bottom: 16 }))
            .show(ui, |ui| self.footer(ui));
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin { left: 28, right: 28, top: 8, bottom: 8 })).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.set_max_width(980.0_f32.min(ui.available_width()));
                match self.page {
                    Page::Home => self.home(ui),
                    Page::Graphics => self.graphics_page(ui),
                    Page::Test => self.test_page(ui),
                    Page::Updates => self.updates_page(ui),
                }
                ui.add_space(12.0);
            });
        });
    }
}

// ---- drawing ----

/// A rectangle shaded from one colour on the left (or top) to another.
fn gradient(p: &egui::Painter, r: Rect, a: Color32, b: Color32, horizontal: bool) {
    let mut m = egui::Mesh::default();
    let (c0, c1, c2, c3) = if horizontal { (a, b, b, a) } else { (a, a, b, b) };
    m.colored_vertex(r.left_top(), c0);
    m.colored_vertex(r.right_top(), c1);
    m.colored_vertex(r.right_bottom(), c2);
    m.colored_vertex(r.left_bottom(), c3);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(m);
}

impl App {
    fn background(&self, ui: &egui::Ui) {
        let screen = ui.max_rect();
        let p = ui.painter();
        p.rect_filled(screen, 0.0, Color32::from_rgb(4, 6, 9));
        // the globe picture cropped to cover the window, the planet to the right
        let img = self.tex.globe_size;
        // the planet's height to the window's, kept to the right as on the
        // game's main menu (the picture's left is dark space to write over)
        let k = (screen.height() / img.y * 1.12).max(screen.width() * 0.8 / img.x);
        let size = img * k;
        let min = pos2(screen.right() - size.x, screen.center().y - size.y * 0.5);
        let fade = (self.started.elapsed().as_secs_f32() / 0.6).min(1.0);
        if fade < 1.0 {
            ui.ctx().request_repaint();
        }
        p.image(self.tex.globe.id(), Rect::from_min_size(min, size), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::from_white_alpha((255.0 * fade) as u8));
        // darkened to the left, where the text is, and towards the foot
        let mid = screen.left() + screen.width() * 0.62;
        p.rect_filled(Rect::from_min_max(screen.min, pos2(screen.left() + screen.width() * 0.25, screen.bottom())), 0.0, Color32::from_rgba_premultiplied(3, 5, 8, 215));
        gradient(p, Rect::from_min_max(pos2(screen.left() + screen.width() * 0.25, screen.top()), pos2(mid, screen.bottom())), Color32::from_rgba_premultiplied(3, 5, 8, 215), Color32::from_rgba_premultiplied(3, 5, 8, 120), true);
        gradient(p, Rect::from_min_max(pos2(mid, screen.top()), screen.max), Color32::from_rgba_premultiplied(3, 5, 8, 120), Color32::from_rgba_premultiplied(3, 5, 8, 60), true);
        gradient(p, Rect::from_min_max(pos2(screen.left(), screen.bottom() - 260.0), screen.max), Color32::TRANSPARENT, Color32::from_rgba_premultiplied(3, 5, 8, 200), false);
        gradient(p, Rect::from_min_max(screen.min, pos2(screen.right(), screen.top() + 120.0)), Color32::from_rgba_premultiplied(3, 5, 8, 170), Color32::TRANSPARENT, false);
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            // the wordmark
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("FABLE FLIGHT").font(style::bold(30.0)).color(Color32::WHITE).extra_letter_spacing(2.0));
                    ui.add_space(4.0);
                    ui.label(RichText::new("LAUNCHER").font(style::bold(14.0)).color(ACCENT).extra_letter_spacing(3.0));
                });
            });
            ui.add_space(28.0);
            // the pages
            for (page, name) in [(Page::Home, "Home"), (Page::Graphics, "Graphics"), (Page::Test, "Graphics test"), (Page::Updates, "Updates")] {
                let on = self.page == page;
                let g = ui.painter().layout_no_wrap(name.to_string(), style::medium(16.0), TEXT);
                let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 20.0, 36.0), egui::Sense::click());
                let lit = style::ease(ui.ctx(), resp.id, resp.hovered());
                let col = if on { Color32::WHITE } else { style::mix(DIM, TEXT, lit) };
                ui.painter().galley(rect.center() - g.size() * 0.5, g, col);
                let u = ui.ctx().animate_bool_with_time(resp.id.with("u"), on, 0.15);
                if u > 0.0 {
                    let w = (rect.width() - 20.0) * u;
                    ui.painter().rect_filled(Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 2.0), vec2(w, 3.0)), 1.5, ACCENT);
                }
                if page == Page::Updates && self.update_available().is_some() {
                    ui.painter().circle_filled(pos2(rect.right() - 6.0, rect.top() + 9.0), 4.0, ACCENT);
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.page = page;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if style::button(ui, "Discord", Some(&self.tex.discord), Kind::Discord, vec2(0.0, 36.0)).on_hover_text(update::DISCORD_URL).clicked() {
                    ctx.open_url(egui::OpenUrl::new_tab(update::DISCORD_URL));
                }
                if style::button(ui, "GitHub", Some(&self.tex.github), Kind::Plain, vec2(0.0, 36.0)).on_hover_text(update::REPO_URL).clicked() {
                    ctx.open_url(egui::OpenUrl::new_tab(update::REPO_URL));
                }
            });
        });
    }

    /// The bar along the foot: what is going on, the progress bar, and Play.
    fn footer(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (title, detail, frac, busy, col) = self.status();
        ui.horizontal(|ui| {
            let play_w = 220.0;
            let left_w = ui.available_width() - play_w - 24.0;
            ui.allocate_ui_with_layout(vec2(left_w, 64.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&title).font(style::semibold(18.0)).color(Color32::WHITE));
                    if let Some(f) = frac.filter(|_| busy) {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("{:.0}%", f * 100.0)).font(style::semibold(16.0)).color(col));
                        });
                    }
                });
                let (rect, _) = ui.allocate_exact_size(vec2(left_w, 8.0), egui::Sense::hover());
                style::progress_bar(ui, rect, frac, busy, col);
                ui.label(RichText::new(&detail).size(13.5).color(DIM));
            });
            ui.add_space(24.0);
            // the big button
            let size = vec2(play_w, 58.0);
            match &self.task {
                Task::Testing { .. } => {
                    if style::button(ui, "Stop test", None, Kind::Danger, size).clicked() {
                        if let Task::Testing { run, .. } = &self.task {
                            run.kill();
                        }
                        if let Some(p) = &mut self.plan {
                            p.cancelled = true;
                        }
                    }
                }
                Task::Playing { run, .. } => {
                    // (shown, not pressed: the game is on its way or flying)
                    let label = if run.tl.lock().ready() { "In flight" } else { "Starting…" };
                    style::button(ui, label, None, Kind::Plain, size).on_hover_cursor(egui::CursorIcon::Default);
                }
                Task::Idle => {
                    if self.restart_needed {
                        if style::button(ui, "Restart launcher", None, Kind::Primary, size).clicked() {
                            let _ = std::process::Command::new(&self.me).spawn();
                            std::process::exit(0);
                        }
                    } else if !self.game.exists() {
                        let can = self.can_update() && self.latest().is_some_and(|r| r.archive().is_some()) && self.job.is_none();
                        if ui.add_enabled_ui(can, |ui| style::button(ui, "Install", None, Kind::Primary, size)).inner.clicked() {
                            self.start_update(&ctx);
                        }
                    } else {
                        let resp = ui.add_enabled_ui(self.job.is_none(), |ui| style::button(ui, "PLAY", None, Kind::Primary, size)).inner;
                        if resp.clicked() {
                            self.play(&ctx);
                        }
                    }
                }
            }
        });
    }

    /// The footer's words and bar: title, detail, fraction (None: unknown), busy, colour.
    fn status(&self) -> (String, String, Option<f32>, bool, Color32) {
        if let Some(job) = &self.job {
            let f = job.done as f32 / job.total.max(1) as f32;
            return match job.phase {
                update::Phase::Downloading => (format!("Downloading {}", job.tag), format!("{} of {}", style::bytes(job.done), style::bytes(job.total)), Some(f), true, ACCENT),
                update::Phase::Checking => (format!("Checking {}", job.tag), "Comparing the download against the release's SHA-256 checksums".into(), Some(1.0), true, ACCENT),
                update::Phase::Installing => (format!("Installing {}", job.tag), format!("Putting the new files in {}", self.dir.display()), Some(1.0), true, ACCENT),
            };
        }
        match &self.task {
            Task::Playing { run, .. } => {
                let el = run.started.elapsed();
                let tl = run.tl.lock();
                if let Some(s) = tl.ready_at() {
                    return ("In flight".into(), format!("The game started in {}. Have a good flight!", style::secs(s)), Some(1.0), false, OK);
                }
                let stage = run::stage_label(tl.current());
                let batch = tl.shader_batch().map(|b| format!(" ({b} of 4)")).unwrap_or_default();
                let why = if tl.shader_batch().is_some() { " · the first start after an install or a driver update takes longest" } else { "" };
                (format!("{stage}{batch}…"), format!("{} so far{why}", style::secs(el.as_secs_f64())), Some(tl.progress(el)), true, ACCENT)
            }
            Task::Testing { run, preset, result } => {
                let el = run.started.elapsed();
                let tl = run.tl.lock();
                let which = preset.map_or("your settings".to_string(), |p| format!("{} preset", p.name()));
                let n = self.plan.as_ref().map_or(0, |p| p.done.len()) + 1;
                let run_no = if self.plan.as_ref().is_some_and(|p| p.auto) { format!("Run {n} · ") } else { String::new() };
                if result.is_some() {
                    return (format!("Testing the {which}"), "Done; closing the test".into(), Some(1.0), true, ACCENT);
                }
                if !tl.ready() {
                    let f = tl.progress(el);
                    return (format!("Graphics test: loading the {which}"), format!("{run_no}{}… {}", run::stage_label(tl.current()), style::secs(el.as_secs_f64())), Some(f * 0.4), true, ACCENT);
                }
                // after the start: the settling time, then the measured flight
                let ready_at = tl.record().last().map_or(0.0, |(_, ms)| *ms as f64 / 1000.0);
                let t = el.as_secs_f64() - ready_at;
                let settle = BENCH_SETTLE;
                let total = settle + BENCH_SECS as f64;
                let f = 0.4 + 0.6 * (t / total).clamp(0.0, 1.0);
                let what = if t < settle { format!("letting the scenery load ({:.0} s)", (settle - t).max(0.0)) } else { format!("measuring frame times ({:.0} s left)", (total - t).max(0.0)) };
                (format!("Graphics test: the {which}"), format!("{run_no}Flying over San Francisco, {what}"), Some(f as f32), true, ACCENT)
            }
            Task::Idle => {
                if let Some(e) = &self.upd_error {
                    return ("Update failed".into(), e.clone(), Some(0.0), false, BAD);
                }
                if let Some((msg, bad)) = &self.ended {
                    return (if *bad { "Something went wrong".into() } else { "Ready".into() }, msg.clone(), Some(if *bad { 0.0 } else { 1.0 }), false, if *bad { BAD } else { OK });
                }
                if self.restart_needed {
                    return ("Update installed".into(), "The launcher was updated too: restart it to use the new one.".into(), Some(1.0), false, OK);
                }
                if !self.game.exists() {
                    let why = if self.dev_build {
                        "Build the game first: cargo build --release".to_string()
                    } else if !self.writable {
                        format!("{} isn't writable, so the game can't be installed here", self.dir.display())
                    } else {
                        match &self.releases {
                            Releases::Ready(r) if r.first().and_then(|r| r.archive()).is_some() => format!("Install {} ({})", r[0].tag, style::bytes(r[0].archive().map_or(0, |a| a.size))),
                            Releases::Failed(e) => format!("Can't reach GitHub: {e}"),
                            _ => "Looking for the latest release…".into(),
                        }
                    };
                    return (format!("{} not found", update::game_name()), why, None, matches!(self.releases, Releases::Checking(_)), ACCENT);
                }
                let preset = self.gfx.matches().map_or("Custom", Preset::name);
                let ver = self.installed.clone().unwrap_or_else(|| "version unknown".into());
                let last = self.cfg.stage_ms.iter().find(|(s, _)| s == "ready").map(|(_, ms)| format!(" · last start took {}", style::secs(*ms as f64 / 1000.0))).unwrap_or_default();
                ("Ready for takeoff".into(), format!("{ver} · {preset} graphics{last}"), None, false, ACCENT)
            }
        }
    }

    // ---- pages ----

    fn home(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(18.0);
        ui.label(RichText::new("An open-world flight simulator with real weather, live traffic and the whole planet to fly over.").size(17.0).color(DIM));
        ui.add_space(14.0);
        if let Some(rel) = self.update_available().cloned() {
            if self.game.exists() {
                egui::Frame::new().fill(ACCENT.gamma_multiply(0.14)).stroke(egui::Stroke::new(1.0, ACCENT.gamma_multiply(0.5))).corner_radius(6).inner_margin(14).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(format!("{} is out", rel.name)).font(style::semibold(18.0)).color(Color32::WHITE));
                            ui.label(RichText::new(format!("You have {}. See what changed on the Updates page.", self.installed.as_deref().unwrap_or("an older version"))).color(DIM));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.can_update() {
                                if ui.add_enabled_ui(!self.busy(), |ui| style::button(ui, "Update now", None, Kind::Primary, vec2(130.0, 36.0))).inner.clicked() {
                                    self.start_update(&ctx);
                                }
                            } else if self.dev_build {
                                style::chip(ui, "Source build: git pull", TEXT);
                            }
                            if style::button(ui, "What's new", None, Kind::Plain, vec2(0.0, 36.0)).clicked() {
                                self.page = Page::Updates;
                            }
                        });
                    });
                });
                ui.add_space(10.0);
            }
        }
        // three cards side by side
        ui.columns(3, |cols| {
            style::card(&mut cols[0], |ui| {
                ui.set_min_height(150.0);
                style::heading(ui, "Your system");
                ui.add_space(4.0);
                match &self.gpu {
                    Some(g) => {
                        ui.label(RichText::new(gpu::short(&g.name)).font(style::semibold(17.0)).color(Color32::WHITE));
                        style::note(ui, format!("{} · {}", g.kind_label(), g.backend));
                    }
                    None => style::note(ui, "The graphics card couldn't be read."),
                }
                if let Some((w, h)) = self.screen {
                    style::note(ui, format!("Screen {w} × {h}"));
                }
                ui.add_space(6.0);
                if let Some((p, _)) = self.recommended() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Suggested").color(DIM));
                        style::chip(ui, p.name(), ACCENT);
                    });
                }
            });
            let cur = self.gfx.matches();
            style::card(&mut cols[1], |ui| {
                ui.set_min_height(150.0);
                style::heading(ui, "Graphics");
                ui.add_space(4.0);
                ui.label(RichText::new(cur.map_or("Custom", Preset::name)).font(style::semibold(17.0)).color(Color32::WHITE));
                style::note(ui, format!("{:.0}% render resolution · {}", self.gfx.scale * 100.0, self.gfx.upscaler.name()));
                style::note(ui, if self.gfx_on_disk { "Saved for the next start" } else { "Not set yet: the suggestion is used" });
                ui.add_space(6.0);
                if style::button(ui, "Change", None, Kind::Plain, vec2(0.0, 30.0)).clicked() {
                    self.page = Page::Graphics;
                }
            });
            let last = self.cfg.bench.last().cloned();
            style::card(&mut cols[2], |ui| {
                ui.set_min_height(150.0);
                style::heading(ui, "Graphics test");
                ui.add_space(4.0);
                match &last {
                    Some(r) => {
                        let v = Verdict::of(r);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{:.0} fps", r.fps)).font(style::semibold(17.0)).color(Color32::WHITE));
                            style::chip(ui, v.label(), verdict_color(v));
                        });
                        style::note(ui, format!("{} · 1% low {:.0} fps", r.preset, r.low1));
                    }
                    None => {
                        ui.label(RichText::new("Not run yet").font(style::semibold(17.0)).color(Color32::WHITE));
                        style::note(ui, "A 40-second flight that measures your frame rate.");
                    }
                }
                ui.add_space(6.0);
                if style::button(ui, if last.is_some() { "Test again" } else { "Run test" }, None, Kind::Plain, vec2(0.0, 30.0)).clicked() {
                    self.page = Page::Test;
                }
            });
        });
        ui.add_space(10.0);
        // the latest release's notes
        let latest = self.latest().cloned();
        style::card(ui, |ui| {
            ui.horizontal(|ui| {
                style::heading(ui, "What's new");
                if let Some(r) = &latest {
                    style::chip(ui, &r.tag, ACCENT);
                    ui.label(RichText::new(&r.date).size(13.0).color(FAINT));
                }
            });
            match (&latest, &self.releases) {
                (Some(r), _) => {
                    ui.label(RichText::new(&r.name).font(style::semibold(18.0)).color(Color32::WHITE));
                    notes(ui, &r.body, Some(6));
                    if ui.link(RichText::new("All release notes →").color(style::LINK)).clicked() {
                        self.page = Page::Updates;
                    }
                }
                (None, Releases::Checking(_)) => style::note(ui, "Fetching the release notes from GitHub…"),
                (None, Releases::Failed(e)) => style::note(ui, format!("Couldn't reach GitHub: {e}")),
                _ => {
                    if style::button(ui, "Check for updates", None, Kind::Plain, vec2(0.0, 30.0)).clicked() {
                        self.check_updates(&ctx);
                    }
                }
            }
        });
        ui.add_space(10.0);
        style::card(ui, |ui| {
            style::heading(ui, "Launcher");
            ui.add_space(2.0);
            style::row(ui, "Minimise when the game is ready", "Out of the way once the game's window is up; back when you quit", |ui| {
                if style::switch(ui, &mut self.cfg.minimize_on_ready) {
                    self.cfg.save();
                }
            });
            style::row(ui, "Check for updates on start", "Asks GitHub for the latest release when the launcher opens", |ui| {
                if style::switch(ui, &mut self.cfg.check_updates) {
                    self.cfg.save();
                }
            });
            ui.horizontal(|ui| {
                style::note(ui, format!("Game folder: {}", self.dir.display()));
                if run::log_path().exists() && ui.link(RichText::new("Open the last game log").size(13.0).color(style::LINK)).clicked() {
                    open_path(&run::log_path());
                }
            });
        });
        if let Some((_, true)) = &self.ended {
            if !self.log.is_empty() {
                ui.add_space(10.0);
                self.log_card(ui);
            }
        }
    }

    fn log_card(&self, ui: &mut egui::Ui) {
        style::card(ui, |ui| {
            style::heading(ui, "The game's last words");
            let lines: Vec<&String> = self.log.iter().rev().take(14).collect();
            for l in lines.into_iter().rev() {
                ui.label(RichText::new(l).monospace().size(12.0).color(if l.contains("ERROR") || l.contains("panicked") { BAD } else { DIM }));
            }
        });
    }

    fn graphics_page(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        ui.add_space(14.0);
        style::page_title(ui, "Graphics", "Set how hard the game works your graphics card before it loads. The game reads these when it starts.");
        let before = self.gfx;
        let rec = self.recommended();
        style::card(ui, |ui| {
            ui.horizontal(|ui| {
                style::heading(ui, "Preset");
                if let Some((note, _)) = &self.gfx_note {
                    style::chip(ui, note, OK);
                }
            });
            ui.add_space(4.0);
            let mut pick = self.gfx.matches();
            let mut opts: Vec<(Option<Preset>, &str)> = Preset::ALL.iter().map(|p| (Some(*p), p.name())).collect();
            if pick.is_none() {
                opts.push((None, "Custom"));
            }
            if style::segments(ui, &mut pick, &opts) {
                if let Some(p) = pick {
                    self.gfx.apply(p);
                }
            }
            ui.add_space(2.0);
            style::note(ui, preset_about(self.gfx.matches()));
            if let Some((p, why)) = &rec {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Suggested for this PC:").color(DIM));
                    style::chip(ui, p.name(), ACCENT);
                    if self.gfx.matches() != Some(*p) && style::button(ui, "Use it", None, Kind::Plain, vec2(0.0, 26.0)).clicked() {
                        self.gfx.apply(*p);
                    }
                });
                style::note(ui, format!("From {why}."));
            }
            if let Some(r) = self.cfg.bench.iter().rev().find(|r| Some(r.preset.as_str()) == self.gfx.matches().map(Preset::name)) {
                let v = Verdict::of(r);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Tested: {:.0} fps average, 1% low {:.0} fps", r.fps, r.low1)).size(13.0).color(DIM));
                    style::chip(ui, v.label(), verdict_color(v));
                });
            }
        });
        ui.add_space(10.0);
        style::card(ui, |ui| {
            style::heading(ui, "Display");
            ui.add_space(4.0);
            let g = &mut self.gfx;
            style::row(ui, "Render resolution", "Below 100% is drawn smaller and upscaled; above, supersampled", |ui| {
                let mut pct = g.scale * 100.0;
                if ui.add(egui::Slider::new(&mut pct, graphics_core::SCALE_MIN * 100.0..=graphics_core::SCALE_MAX * 100.0).step_by(5.0).fixed_decimals(0).suffix("%")).changed() {
                    g.scale = pct / 100.0;
                }
            });
            style::row(ui, "Upscaler", "How a smaller frame is brought up to the window", |ui| {
                let opts: Vec<(Upscaler, &str)> = Upscaler::ALL.iter().map(|u| (*u, u.name())).collect();
                style::segments(ui, &mut g.upscaler, &opts);
            });
            style::row(ui, "Anti-aliasing (TAA)", "Smooths edges and shimmer using past frames", |ui| {
                style::switch(ui, &mut g.taa);
            });
            style::row(ui, "MSAA", "Multisampled edges in the scene (applied at start)", |ui| {
                style::segments(ui, &mut g.msaa, &[(1, "Off"), (2, "2×"), (4, "4×")]);
            });
            style::row(ui, "VSync", "Waits for the screen: no tearing, a little more latency", |ui| {
                style::switch(ui, &mut g.vsync);
            });
            style::row(ui, "Frame limit", "Caps the frame rate: cooler and quieter, longer on battery", |ui| {
                egui::ComboBox::from_id_salt("fps_cap").selected_text(if g.fps_cap == 0 { "None".to_string() } else { format!("{} fps", g.fps_cap) }).show_ui(ui, |ui| {
                    for v in [0u16, 30, 60, 90, 120, 144, 165, 240] {
                        ui.selectable_value(&mut g.fps_cap, v, if v == 0 { "None".to_string() } else { format!("{v} fps") });
                    }
                });
            });
        });
        ui.add_space(10.0);
        style::card(ui, |ui| {
            style::heading(ui, "Quality");
            ui.add_space(4.0);
            let g = &mut self.gfx;
            let all = [(Level::Off, "Off"), (Level::Low, "Low"), (Level::Medium, "Medium"), (Level::High, "High"), (Level::Ultra, "Ultra")];
            let no_off = &all[1..];
            let to_high = &all[..4];
            style::row(ui, "Shadows", "The sun's shadows and how sharp they are", |ui| style::segments(ui, &mut g.shadows, &all));
            style::row(ui, "Clouds", "Volumetric clouds (Off still flies the weather)", |ui| style::segments(ui, &mut g.clouds, &all));
            style::row(ui, "Cloud shadows", "Cloud shadows on the ground: a fixed cost at any resolution", |ui| {
                style::switch(ui, &mut g.cloud_shadows);
            });
            style::row(ui, "Storms", "Detail of thunderstorm and squall cells", |ui| style::segments(ui, &mut g.storms, no_off));
            style::row(ui, "Contrails", "Condensation trails behind aircraft at altitude", |ui| {
                style::switch(ui, &mut g.contrails);
            });
            style::row(ui, "Terrain", "How far out the ground keeps its detail", |ui| style::segments(ui, &mut g.terrain, no_off));
            style::row(ui, "Scenery distance", "How far out buildings and trees are loaded", |ui| style::segments(ui, &mut g.scenery, no_off));
            style::row(ui, "City detail", "How far buildings keep their full detail", |ui| style::segments(ui, &mut g.city, no_off));
            style::row(ui, "Trees", "How far trees are drawn", |ui| style::segments(ui, &mut g.trees, no_off));
            style::row(ui, "Water", "Low plain, Medium full, High with reflections", |ui| style::segments(ui, &mut g.water, &no_off[..3]));
            style::row(ui, "Cars", "Traffic on the roads and parked cars", |ui| style::segments(ui, &mut g.cars, to_high));
            style::row(ui, "Birds", "Flocks in the air", |ui| style::segments(ui, &mut g.birds, to_high));
            style::row(ui, "Ambient occlusion", "Soft contact shadows in corners and under things", |ui| {
                style::switch(ui, &mut g.ao);
            });
        });
        if self.gfx != before {
            self.save_gfx(now, "Saved");
        }
        ui.add_space(10.0);
        style::note(ui, "These are the same settings as the game's Settings → Graphics, saved in graphics.json. Google's photorealistic 3D tiles have their own detail setting in the game.");
    }

    fn test_page(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        ui.add_space(14.0);
        style::page_title(ui, "Graphics test", "A short scripted flight over San Francisco's city and bay, at your screen's resolution, that measures how smoothly the game runs.");
        let testing = matches!(self.task, Task::Testing { .. });
        let idle = !self.busy() && self.game.exists();
        style::card(ui, |ui| {
            style::heading(ui, "Run a test");
            ui.add_space(4.0);
            style::note(ui, format!("Each run opens the game in a window, waits for the scenery to stream in, then measures {BENCH_SECS} seconds of flight. Your settings aren't changed until you choose to use a result."));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(idle, |ui| {
                    if style::button(ui, "Find the best preset", None, Kind::Primary, vec2(200.0, 40.0)).on_hover_text("Starts at the suggested preset and steps up or down: up to four runs of about 45 s each").clicked() {
                        self.start_test(true);
                    }
                    let cur = self.gfx.matches().map_or("Custom".to_string(), |p| p.name().to_string());
                    if style::button(ui, &format!("Test my settings ({cur})"), None, Kind::Plain, vec2(0.0, 40.0)).clicked() {
                        self.start_test(false);
                    }
                });
                if testing && style::button(ui, "Stop", None, Kind::Danger, vec2(90.0, 40.0)).clicked() {
                    if let Task::Testing { run, .. } = &self.task {
                        run.kill();
                    }
                    if let Some(p) = &mut self.plan {
                        p.cancelled = true;
                    }
                }
            });
            if !self.game.exists() {
                style::note(ui, "The game isn't installed yet.");
            }
        });
        // this session's runs and the verdict
        if let Some(plan) = &self.plan {
            ui.add_space(10.0);
            let best = plan.best;
            let auto = plan.auto;
            let runs: Vec<(Option<Preset>, Option<BenchResult>)> = plan.done.clone();
            let mut use_preset = None;
            style::card(ui, |ui| {
                style::heading(ui, "Results");
                ui.add_space(4.0);
                for (p, r) in &runs {
                    result_row(ui, p.map_or("Your settings", Preset::name), r.as_ref());
                }
                if testing {
                    if let Task::Testing { preset, .. } = &self.task {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(14.0).color(ACCENT));
                            ui.label(RichText::new(format!("{}: running…", preset.map_or("Your settings", Preset::name))).color(DIM));
                        });
                    }
                }
                if auto && !testing {
                    ui.add_space(6.0);
                    match best {
                        Some(p) => {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Best for this PC:").font(style::semibold(17.0)).color(Color32::WHITE));
                                style::chip(ui, p.name(), ACCENT);
                                if self.gfx.matches() == Some(p) {
                                    style::chip(ui, "In use", OK);
                                } else if style::button(ui, &format!("Use {}", p.name()), None, Kind::Primary, vec2(0.0, 30.0)).clicked() {
                                    use_preset = Some(p);
                                }
                            });
                            // a higher preset that flew, if not quite smoothly
                            let rank = |q: Preset| Preset::ALL.iter().position(|x| *x == q).unwrap_or(0);
                            let also = runs.iter().filter_map(|(q, r)| Some(((*q)?, r.as_ref()?))).filter(|(q, r)| rank(*q) > rank(p) && Verdict::of(r) == Verdict::Playable).max_by_key(|(q, _)| rank(*q));
                            if let Some((q, r)) = also {
                                ui.horizontal(|ui| {
                                    style::note(ui, format!("{} also flies ({:.0} fps, dipping to {:.0}): the sharper picture if you don't mind the odd dip.", q.name(), r.fps, r.low1));
                                    if self.gfx.matches() != Some(q) && ui.link(RichText::new(format!("Use {}", q.name())).size(13.0).color(style::LINK)).clicked() {
                                        use_preset = Some(q);
                                    }
                                });
                            }
                        }
                        None if runs.iter().all(|(_, r)| r.is_none()) => style::note(ui, "No result: the test didn't finish."),
                        None => {}
                    }
                }
            });
            if let Some(p) = use_preset {
                self.gfx.apply(p);
                self.save_gfx(now, "Saved");
            }
        }
        ui.add_space(10.0);
        let history = self.cfg.bench.clone();
        style::card(ui, |ui| {
            style::heading(ui, "Earlier results");
            ui.add_space(4.0);
            if history.is_empty() {
                style::note(ui, "None yet.");
            }
            for r in history.iter().rev().take(8) {
                result_row(ui, &r.preset, Some(r));
            }
            ui.add_space(6.0);
            style::note(ui, "Stable: the slowest 1% of frames stay above 50 fps. Playable: above 30 fps. Results depend on the weather, the scenery loaded and what else the PC is doing.");
        });
        if let Some((_, true)) = &self.ended {
            if !self.log.is_empty() {
                ui.add_space(10.0);
                self.log_card(ui);
            }
        }
    }

    fn updates_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.add_space(14.0);
        style::page_title(ui, "Updates", "New versions come straight from the FableFlight releases on GitHub: no need to download anything by hand.");
        style::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    style::fact(ui, "Installed", RichText::new(self.installed.clone().unwrap_or_else(|| if self.game.exists() { "unknown".into() } else { "not installed".into() })).color(Color32::WHITE));
                    let latest = match &self.releases {
                        Releases::Ready(r) => r.first().map_or("none published".into(), |r| format!("{} ({})", r.tag, r.date)),
                        Releases::Checking(_) => "checking…".into(),
                        Releases::Failed(_) => "couldn't check".into(),
                        Releases::Unknown => "not checked".into(),
                    };
                    style::fact(ui, "Latest", RichText::new(latest).color(Color32::WHITE));
                    style::fact(ui, "Platform", RichText::new(update::platform().unwrap_or("not supported")).color(Color32::WHITE));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let avail = self.update_available().map(|r| r.tag.clone());
                    if let Some(tag) = avail.as_ref().filter(|_| self.can_update()) {
                        let label = if self.game.exists() { format!("Update to {tag}") } else { format!("Install {tag}") };
                        if ui.add_enabled_ui(self.can_update() && !self.busy(), |ui| style::button(ui, &label, None, Kind::Primary, vec2(170.0, 38.0))).inner.clicked() {
                            self.start_update(&ctx);
                        }
                    }
                    let checking = matches!(self.releases, Releases::Checking(_));
                    if ui.add_enabled_ui(!checking, |ui| style::button(ui, if checking { "Checking…" } else { "Check now" }, None, Kind::Plain, vec2(0.0, 38.0))).inner.clicked() {
                        self.check_updates(&ctx);
                    }
                    if avail.is_none() && matches!(self.releases, Releases::Ready(_)) && self.game.exists() {
                        style::chip(ui, "Up to date", OK);
                    }
                });
            });
            if self.dev_build {
                ui.add_space(4.0);
                style::note(ui, "This launcher runs from a source build (a cargo target folder): update with git pull and cargo build --release instead.");
            } else if !self.writable {
                style::note(ui, format!("{} isn't writable, so updates can't be installed here.", self.dir.display()));
            }
            if let Some(e) = &self.upd_error {
                ui.label(RichText::new(format!("The update failed: {e}")).color(BAD));
            }
            if let Releases::Failed(e) = &self.releases {
                ui.label(RichText::new(format!("Couldn't reach GitHub: {e}")).color(WARN));
            }
            if self.restart_needed {
                ui.label(RichText::new("The launcher itself was updated: restart it to run the new version.").color(OK));
            }
        });
        if let Releases::Ready(list) = &self.releases {
            let installed = self.installed.as_deref().and_then(update::version);
            let list = list.clone();
            for r in &list {
                ui.add_space(10.0);
                let newer = installed.is_some_and(|v| update::version(&r.tag).is_some_and(|t| t > v));
                let current = installed.is_some() && update::version(&r.tag) == installed;
                style::card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&r.name).font(style::semibold(20.0)).color(Color32::WHITE));
                        if newer {
                            style::chip(ui, "New", ACCENT);
                        }
                        if current {
                            style::chip(ui, "Installed", OK);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.link(RichText::new("On GitHub").size(13.0).color(style::LINK)).clicked() {
                                ctx.open_url(egui::OpenUrl::new_tab(&r.url));
                            }
                            ui.label(RichText::new(&r.date).size(13.0).color(FAINT));
                        });
                    });
                    notes(ui, &r.body, None);
                });
            }
        }
    }
}

fn verdict_color(v: Verdict) -> Color32 {
    match v {
        Verdict::Smooth => OK,
        Verdict::Playable => WARN,
        Verdict::Struggles => BAD,
    }
}

fn result_row(ui: &mut egui::Ui, name: &str, r: Option<&BenchResult>) {
    ui.horizontal(|ui| {
        ui.add_sized([110.0, 22.0], egui::Label::new(RichText::new(name).font(style::medium(15.0)).color(Color32::WHITE)));
        match r {
            Some(r) => {
                let v = Verdict::of(r);
                // a bar of the average frame rate, to 120 fps
                let (rect, _) = ui.allocate_exact_size(vec2(160.0, 10.0), egui::Sense::hover());
                style::progress_bar(ui, rect, Some((r.fps / 120.0) as f32), false, verdict_color(v));
                ui.label(RichText::new(format!("{:>5.0} fps", r.fps)).monospace().color(Color32::WHITE));
                let hitches = match r.hitches {
                    0 => "no hitches".to_string(),
                    1 => "1 hitch".to_string(),
                    n => format!("{n} hitches"),
                };
                ui.label(RichText::new(format!("1% low {:.0} · {hitches}", r.low1)).size(13.0).color(DIM)).on_hover_text(format!("Slowest frame {:.0} ms; a hitch is a frame over 100 ms (usually scenery loading)", r.worst_ms));
                style::chip(ui, v.label(), verdict_color(v)).on_hover_text(v.about());
                ui.label(RichText::new(format!("{}×{} at {:.0}%", r.width, r.height, r.scale * 100.0)).size(12.0).color(FAINT));
            }
            None => {
                ui.label(RichText::new("no result").color(FAINT));
            }
        }
    });
}

fn preset_about(p: Option<Preset>) -> &'static str {
    match p {
        Some(Preset::VeryLow) => "For integrated graphics: half resolution, no shadows, cloud shadows, MSAA, traffic or birds.",
        Some(Preset::Low) => "For older or entry-level cards: 60% resolution, no sun shadows, short draw distances.",
        Some(Preset::Medium) => "For mid-range cards: 75% resolution, soft shadows, everything else on at medium distance.",
        Some(Preset::High) => "For strong cards: full resolution and the full scenery, as the game was designed to look.",
        Some(Preset::Ultra) => "For the fastest cards: supersampled at 150%, sharper shadows, finer clouds, longer views.",
        None => "Your own mix of settings.",
    }
}

/// The release notes, GitHub markdown drawn simply: headings, bullets, links
/// shortened to their pull request numbers.
fn notes(ui: &mut egui::Ui, body: &str, max_lines: Option<usize>) {
    let pr = format!("https://github.com/{}/pull/", update::REPO);
    let mut shown = 0;
    for raw in body.lines() {
        let line = raw.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if max_lines.is_some_and(|m| shown >= m) {
            style::note(ui, "…");
            break;
        }
        shown += 1;
        let mut text = line.replace("**", "").replace('`', "");
        while let Some(i) = text.find(&pr) {
            let rest = &text[i + pr.len()..];
            let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            text = format!("{}#{}{}", &text[..i], n, &rest[n.len()..]);
        }
        let t = text.trim_start();
        if let Some(h) = t.strip_prefix("### ").or_else(|| t.strip_prefix("## ")).or_else(|| t.strip_prefix("# ")) {
            ui.add_space(2.0);
            ui.label(RichText::new(h).font(style::semibold(16.0)).color(Color32::WHITE));
        } else if let Some(b) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new("•").color(ACCENT));
                link_text(ui, b);
            });
        } else {
            ui.horizontal_wrapped(|ui| link_text(ui, t));
        }
    }
}

/// Text with any bare URL in it made a link.
fn link_text(ui: &mut egui::Ui, s: &str) {
    match s.find("https://") {
        Some(i) => {
            let end = s[i..].find(char::is_whitespace).map_or(s.len(), |e| i + e);
            if i > 0 {
                ui.label(RichText::new(&s[..i]).color(TEXT));
            }
            ui.hyperlink_to(RichText::new(s[i..end].trim_start_matches("https://github.com/")).color(style::LINK), &s[i..end]);
            if end < s.len() {
                ui.label(RichText::new(&s[end..]).color(TEXT));
            }
        }
        None => {
            ui.label(RichText::new(s).color(TEXT));
        }
    }
}

/// Opens a file in the system's viewer.
fn open_path(p: &std::path::Path) {
    let p = p.as_os_str();
    #[cfg(target_os = "windows")]
    let r = std::process::Command::new("explorer").arg(p).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(p).spawn();
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(p).spawn();
    let _ = r;
}
