//! A picture of the launcher drawn off screen (`FABLE_LAUNCHER_SHOT=out.png`,
//! `FABLE_LAUNCHER_PAGE=home|graphics|test|updates`): the same UI code run
//! for a few frames without a window and rendered by egui-wgpu to a texture,
//! for checking the look where the window can't be seen.
use crate::{App, Page};
use std::path::Path;
use std::time::{Duration, Instant};

const W: u32 = 1140;
const H: u32 = 720;

pub fn render(path: &Path, page: Page) -> Result<(), String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })).map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| e.to_string())?;
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, Some(adapter.get_info()));
    app.page = page;
    app.screen = Some((1920, 1080));
    app.started = Instant::now() - Duration::from_secs(5);
    // FABLE_LAUNCHER_STATE=starting: the footer as a start is under way
    // (a stand-in process, the stages of a start fed in by hand)
    if std::env::var("FABLE_LAUNCHER_STATE").as_deref() == Ok("starting") {
        let sleeper = if cfg!(windows) { "timeout" } else { "sleep" };
        if let Ok(run) = crate::run::spawn(Path::new(sleeper), &["20".to_string()], &[], None) {
            for (s, ms) in [("boot", 0), ("window", 300), ("gpu", 450), ("shaders", 700), ("shaders-part", 1500)] {
                run.tl.lock().stage(s, Duration::from_millis(ms));
            }
            app.task = crate::Task::Playing { run, ready: false };
        }
    }
    // FABLE_LAUNCHER_STATE=bench (or auto): a real graphics test of the
    // saved settings (or the search for the best preset) first, the game run
    // as the launcher runs it, then the picture
    let state = std::env::var("FABLE_LAUNCHER_STATE").unwrap_or_default();
    let bench = state == "bench" || state == "auto";
    if bench {
        app.start_test(state == "auto");
    }
    let input = |t: f64| egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(W as f32, H as f32))), time: Some(t), ..Default::default() };
    let mut deltas = Vec::new();
    // frames until the release notes are in (or 6 s), then two more to settle the layout
    let t0 = Instant::now();
    let mut out;
    let mut extra = 0;
    loop {
        out = ctx.run_ui(input(t0.elapsed().as_secs_f64()), |ui| app.draw(ui));
        deltas.push(std::mem::take(&mut out.textures_delta));
        let waiting = matches!(app.releases, crate::Releases::Checking(_)) || app.version_rx.is_some() || (bench && !matches!(app.task, crate::Task::Idle) && t0.elapsed() < Duration::from_secs(400));
        if !waiting || (!bench && t0.elapsed() > Duration::from_secs(6)) {
            extra += 1;
            if extra > 3 {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let ppp = out.pixels_per_point;
    let jobs = ctx.tessellate(out.shapes, ppp);
    // (egui blends in gamma space: a plain, not sRGB, target as eframe picks)
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let mut r = egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
    for d in &deltas {
        for (id, imgs) in &d.set {
            for img in imgs {
                r.update_texture(&device, &queue, *id, img);
            }
        }
    }
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("launcher picture"),
        size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    let sd = egui_wgpu::ScreenDescriptor { size_in_pixels: [W, H], pixels_per_point: ppp };
    let mut enc = device.create_command_encoder(&Default::default());
    let cmds = r.update_buffers(&device, &queue, &mut enc, &jobs, &sd);
    {
        let mut rp = enc
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                ..Default::default()
            })
            .forget_lifetime();
        r.render(&mut rp, &jobs, &sd);
    }
    let row = (W * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * H) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(H) } },
        wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
    );
    queue.submit(cmds.into_iter().chain([enc.finish()]));
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None });
    let data = buf.slice(..).get_mapped_range().map_err(|e| e.to_string())?;
    let mut img = image::RgbImage::new(W, H);
    for y in 0..H as usize {
        for x in 0..W as usize {
            let i = y * row as usize + x * 4;
            img.put_pixel(x as u32, y as u32, image::Rgb([data[i], data[i + 1], data[i + 2]]));
        }
    }
    if let crate::Task::Playing { run, .. } = &app.task {
        run.kill();
    }
    img.save(path).map_err(|e| e.to_string())
}
