//! The real clouds on the start screen's globe: a global cloud map, draped
//! over the planet as a thin white layer (composite.wgsl `globe_sample`).
//!
//! Source: Matt Eason's Live Cloud Maps, <https://github.com/matteason/live-cloud-maps>,
//! `https://clouds.matteason.co.uk/images/2048x1024/clouds.jpg`: a greyscale,
//! equirectangular (plate carrée, 180°W at the left edge, 90°N at the top)
//! infrared cloud composite of the whole Earth from the geostationary weather
//! satellites, with no swath gaps (the poles, which the satellites do not
//! see, are mirrored in from lower latitudes). Remade every 3 hours. The
//! images are CC0; the satellite data is EUMETSAT's, whose licence asks for
//! the credit "Contains modified EUMETSAT data" (shown on the start screen).
//!
//! (NASA GIBS was looked at first: its gap-free geostationary layers,
//! GOES-East/West and Himawari ABI/AHI band 13, cover the Americas and the
//! Pacific but leave Europe, Africa and the Indian Ocean out, and its global
//! MODIS/VIIRS products have swath gaps or are 1° daily grids.)
//!
//! A background thread reads it once the menu is up, keeps it in the disk
//! cache (`<cache>/globe/clouds.jpg`), and looks again every half hour for a
//! copy more than 3 hours old. Offline or on any failure the globe simply
//! has no clouds (or the last copy read, up to two days old).

use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Size of the map on the GPU (render.rs `upload_globe_clouds`, R8).
pub const W: u32 = 2048;
pub const H: u32 = 1024;
const URL: &str = "https://clouds.matteason.co.uk/images/2048x1024/clouds.jpg";
/// The map is remade every 3 hours.
const FRESH: Duration = Duration::from_secs(3 * 3600);
/// An older copy is still better than no clouds, up to this age.
const STALE: Duration = Duration::from_secs(48 * 3600);

#[derive(Default)]
struct Shared {
    /// a new map to upload (W x H, one byte of cloud cover per pixel, row 0 at 90°N)
    new: Option<Vec<u8>>,
    /// whether the start screen is up (the thread only fetches then)
    menu: bool,
}

#[derive(Default)]
pub struct GlobeClouds {
    shared: Arc<Mutex<Shared>>,
    started: bool,
    /// a map has been handed to the GPU
    pub shown: bool,
}

impl GlobeClouds {
    /// Once a frame: starts the thread the first time the menu shows, and
    /// returns a newly read map to upload.
    pub fn step(&mut self, fetch: &Arc<crate::sources::Sources>, menu: bool) -> Option<Vec<u8>> {
        let new = {
            let mut s = self.shared.lock();
            s.menu = menu;
            s.new.take()
        };
        if menu && !self.started {
            self.started = true;
            let (src, shared) = (fetch.clone(), self.shared.clone());
            let _ = std::thread::Builder::new().name("globe clouds".into()).spawn(move || run(&src, &shared));
        }
        if new.is_some() {
            self.shown = true;
        }
        new
    }
}

fn run(src: &crate::sources::Sources, shared: &Mutex<Shared>) {
    let path = src.fetch.root.join("globe").join("clouds.jpg");
    let age = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
    let mut loaded: Option<SystemTime> = None;
    loop {
        if shared.lock().menu {
            let a = age(&path);
            if a.is_none_or(|a| a > FRESH) && !src.fetch.offline {
                match src.fetch.get(URL) {
                    Ok(data) if decode(&data).is_some() => {
                        let _ = std::fs::create_dir_all(path.parent().unwrap());
                        let tmp = path.with_extension("tmp");
                        if std::fs::write(&tmp, &data).is_ok() {
                            let _ = std::fs::rename(&tmp, &path);
                        }
                    }
                    Ok(_) => log::info!("globe clouds: could not decode {URL}"),
                    Err(e) => log::info!("globe clouds: {e}"),
                }
            }
            let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            if mtime.is_some() && mtime != loaded && age(&path).is_some_and(|a| a < STALE) {
                if let Some(px) = std::fs::read(&path).ok().and_then(|d| decode(&d)) {
                    log::info!("globe clouds: map from {:.1} h ago", age(&path).map(|a| a.as_secs_f64() / 3600.0).unwrap_or(0.0));
                    shared.lock().new = Some(px);
                }
                loaded = mtime;
            }
        }
        std::thread::sleep(Duration::from_secs(if loaded.is_some() { 1800 } else { 60 }));
    }
}

/// The JPEG as W x H bytes of cloud cover (resized if it comes another size).
fn decode(data: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(data).ok()?.to_luma8();
    let img = if img.dimensions() == (W, H) { img } else { image::imageops::resize(&img, W, H, image::imageops::FilterType::Triangle) };
    Some(cover(img.as_raw()))
}

/// Infrared brightness to cloud cover. The map is brightest where it is
/// coldest, and in the winter hemisphere the clear ground and sea are cold
/// too: north of 50° nearly all of it is pale grey. So each pixel is measured
/// against the clear background round it: the 8th percentile of its block
/// (22.5° of longitude by 5.6° of latitude), eased over the neighbouring
/// blocks. What stands out above that is cloud, the coldest (highest, thickest)
/// tops whitest; thin or low cloud comes out partly see-through.
fn cover(px: &[u8]) -> Vec<u8> {
    let (w, h) = (W as usize, H as usize);
    // Rows that are one flat grey (the polar caps the source fades out)
    // take the nearest row with weather in it.
    let mut src = px.to_vec();
    let flat = |r: &[u8]| {
        let (lo, hi) = r.iter().fold((255u8, 0u8), |(a, b), &v| (a.min(v), b.max(v)));
        hi - lo < 8
    };
    let first = (0..h).find(|&y| !flat(&px[y * w..(y + 1) * w])).unwrap_or(0);
    let last = (0..h).rev().find(|&y| !flat(&px[y * w..(y + 1) * w])).unwrap_or(h - 1);
    let (lo, hi) = (first + 4, last.saturating_sub(4).max(first + 4));
    for y in 0..h {
        let from = y.clamp(lo, hi);
        if from != y {
            src.copy_within(from * w..(from + 1) * w, y * w);
        }
    }
    const BW: usize = 128;
    const BH: usize = 32;
    let (nx, ny) = (w / BW, h / BH);
    let mut blk = vec![0f32; nx * ny];
    for by in 0..ny {
        for bx in 0..nx {
            let mut hist = [0u32; 256];
            for y in by * BH..(by + 1) * BH {
                for &v in &src[y * w + bx * BW..y * w + (bx + 1) * BW] {
                    hist[v as usize] += 1;
                }
            }
            let want = (BW * BH) as u32 * 8 / 100;
            let (mut acc, mut v) = (0, 0);
            while v < 255 && acc + hist[v] < want {
                acc += hist[v];
                v += 1;
            }
            blk[by * nx + bx] = v as f32 / 255.0;
        }
    }
    // Eased over the four neighbours (longitude wraps), never above the block's own.
    let at = |x: isize, y: isize| blk[y.clamp(0, ny as isize - 1) as usize * nx + x.rem_euclid(nx as isize) as usize];
    let mut bg = vec![0f32; nx * ny];
    for y in 0..ny as isize {
        for x in 0..nx as isize {
            let m = (at(x, y) + at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1)) / 5.0;
            bg[y as usize * nx + x as usize] = m.min(at(x, y));
        }
    }
    let bgat = |x: isize, y: isize| bg[y.clamp(0, ny as isize - 1) as usize * nx + x.rem_euclid(nx as isize) as usize];
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        let fy = (y as f32 + 0.5) / BH as f32 - 0.5;
        let (y0, ty) = (fy.floor() as isize, fy - fy.floor());
        for x in 0..w {
            let fx = (x as f32 + 0.5) / BW as f32 - 0.5;
            let (x0, tx) = (fx.floor() as isize, fx - fx.floor());
            let b = (bgat(x0, y0) * (1.0 - tx) + bgat(x0 + 1, y0) * tx) * (1.0 - ty) + (bgat(x0, y0 + 1) * (1.0 - tx) + bgat(x0 + 1, y0 + 1) * tx) * ty;
            let b = b.min(0.8);
            let v = src[y * w + x] as f32 / 255.0;
            let n = ((v - b) / (1.0 - b).max(0.12)).clamp(0.0, 1.0);
            let a = ((n - 0.15) / 0.8).clamp(0.0, 1.0).powf(1.3);
            out[y * w + x] = (a * 255.0 + 0.5) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn cover_from_brightness() {
        let (w, h) = (super::W as usize, super::H as usize);
        // Clear warm sea (dark) with one cold cloud (white); a pale cold band
        // in the north with a brighter cloud in it.
        let mut px = vec![20u8; w * h];
        for y in 0..h / 4 {
            for x in 0..w {
                px[y * w + x] = 215 + ((x / 7 + y / 5) % 3) as u8;
            }
        }
        for y in 500..540 {
            for x in 1000..1100 {
                px[y * w + x] = 240;
            }
        }
        for y in 100..140 {
            for x in 300..400 {
                px[y * w + x] = 250;
            }
        }
        let c = super::cover(&px);
        assert_eq!(c[700 * w + 200], 0, "clear sea");
        assert!(c[520 * w + 1050] > 200, "cloud over the sea");
        assert!(c[60 * w + 1500] < 40, "cold clear ground in the north");
        assert!(c[120 * w + 350] > 120, "cloud over the cold north");
    }
}
