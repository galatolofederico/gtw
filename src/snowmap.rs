//! Today's real snow cover (level 2 of snow.rs): where the satellites saw
//! snow, the snow line comes down to the ground; where they saw none, it
//! stays up on the peaks. The worked-out snow line (snow.rs) still shapes it
//! inside each cell: the slope, the shady side, the patches.
//!
//! Source: NOAA/NESDIS and U.S. National Ice Center, "IMS Daily Northern
//! Hemisphere Snow and Ice Analysis at 1 km, 4 km, and 24 km Resolutions,
//! Version 1", distributed by NSIDC (dataset G02156, doi:10.7265/N52R3PMC),
//! <https://nsidc.org/data/g02156>. A U.S. Government work, free to use; the
//! credit is shown on the start screen. The 4 km analysis is used:
//! `https://noaadata.apps.nsidc.org/NOAA/G02156/4km/YYYY/imsYYYYDDD_00UTC_4km_v1.3.asc.gz`,
//! about 450 KB, one a day (the analysis for 00 UTC of day DDD comes out
//! around 14 UTC the day before). It is gap-free (analysts merge every
//! satellite, so no holes under the clouds, unlike MODIS or VIIRS) but covers
//! the northern hemisphere only: south of the equator, offline, or when the
//! game's date is not the real one, the worked-out snow line stands alone.
//!
//! The file is ASCII: a header, then 6144 rows of 6144 digits, the bottom
//! row first: 0 outside the hemisphere, 1 open water, 2 land without snow,
//! 3 sea or lake ice, 4 snow-covered land. The grid is polar stereographic on
//! the WGS 84 ellipsoid, true scale at 60°N, 80°W pointing down from the
//! pole, 4 km cells, the pole in the middle. (Checked against the file of
//! 18 November 2026: snow on Mont Blanc, none down in Chamonix, Greenland and
//! Siberia white, the Atlantic and Lake Superior open water.)
//!
//! A background thread looks for the newest file every half hour once the
//! game wants one, keeps it in the disk cache (`<cache>/snow/`), and drops
//! older ones. Around the aircraft, a square of N x N cells (about 1000 km)
//! is cut out and resampled onto the local tangent plane for the shaders
//! (common.wgsl `snow_line_at`), like the city-light map (skyglow.rs).

use crate::geo;
use glam::DVec3;
use parking_lot::Mutex;
use std::io::{BufRead, Read};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Cells across the IMS 4 km grid.
const GRID: usize = 6144;
/// Side of an IMS cell (m).
const GRID_CELL: f64 = 4000.0;
/// Texels across the map around the aircraft (render.rs, Rg8Unorm).
pub const N: usize = 256;
/// Side of a texel (m): one IMS cell, so the map covers about 1000 km.
const CELL: f64 = 4000.0;
/// How far the camera may move from the map's centre before it is redrawn (m).
const MOVE: f64 = 100_000.0;
/// The game's day may be this far (days) from the analysis' and still use it
/// fully; from twice as far, not at all.
const NEAR_DAYS: f64 = 3.0;
/// The newest file is looked for this many days back.
const DAYS_BACK: i64 = 6;

fn url(year: i32, ddd: u32) -> String {
    format!("https://noaadata.apps.nsidc.org/NOAA/G02156/4km/{year}/ims{year}{ddd:03}_00UTC_4km_v1.3.asc.gz")
}

/// One day's analysis, two bits a cell.
pub struct Grid {
    /// the day it is for: year, and day of the year (1 = January 1st)
    pub year: i32,
    pub ddd: u32,
    bits: Vec<u8>,
}

impl Grid {
    /// The class (0..4, see the module doc; ice and snow are both 3 here:
    /// 0 outside, 1 water, 2 bare land, 3 snow or ice) at a cell, row 0 the
    /// bottom one.
    fn class(&self, col: usize, row: usize) -> u8 {
        let i = row * GRID + col;
        (self.bits[i / 4] >> ((i % 4) * 2)) & 3
    }
}

/// Reads the gzipped ASCII file. `n` is the grid's side (6144; smaller in tests).
fn decode(gz: &[u8], n: usize) -> Option<Vec<u8>> {
    let mut text = Vec::new();
    flate2::read::GzDecoder::new(gz).read_to_end(&mut text).ok()?;
    let mut bits = vec![0u8; (n * n).div_ceil(4)];
    let mut row = 0;
    for line in text.lines() {
        let line = line.ok()?;
        let line = line.trim_end();
        if line.len() != n || !line.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if row == n {
            return None;
        }
        for (col, b) in line.bytes().enumerate() {
            let c = match b - b'0' {
                0 => 0,
                1 => 1,
                2 => 2,
                3 | 4 => 3,
                _ => return None,
            };
            let i = row * n + col;
            bits[i / 4] |= c << ((i % 4) * 2);
        }
        row += 1;
    }
    (row == n).then_some(bits)
}

/// Where a place (degrees) falls on the IMS grid: column, and row counted
/// from the bottom. None south of the equator or off the grid.
pub fn cell_of(lat: f64, lon: f64) -> Option<(usize, usize)> {
    if lat < 0.0 {
        return None;
    }
    // Polar stereographic, WGS 84, standard parallel 60°N, central meridian 80°W
    // (Snyder, "Map Projections: A Working Manual", 1987, eq. 15-9, 21-33, 21-34).
    const A: f64 = 6_378_137.0;
    const E: f64 = 0.081_819_190_842_6;
    let t = |phi: f64| {
        let es = E * phi.sin();
        (std::f64::consts::FRAC_PI_4 - phi / 2.0).tan() / ((1.0 - es) / (1.0 + es)).powf(E / 2.0)
    };
    let ts = 60f64.to_radians();
    let mc = ts.cos() / (1.0 - (E * ts.sin()).powi(2)).sqrt();
    let rho = A * mc * t(lat.to_radians()) / t(ts);
    let dl = (lon + 80.0).to_radians();
    let (x, y) = (rho * dl.sin(), -rho * dl.cos());
    let half = GRID as f64 * GRID_CELL / 2.0;
    let (c, r) = ((x + half) / GRID_CELL, (y + half) / GRID_CELL);
    let inside = |v: f64| (0.0..GRID as f64).contains(&v);
    (inside(c) && inside(r)).then_some((c as usize, r as usize))
}

/// The UTC date `days_ago` days before now: year and day of the year (1-based).
fn utc_day(now: SystemTime, days_ago: i64) -> (i32, u32) {
    let secs = now.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let mut days = secs.div_euclid(86_400) - days_ago;
    let mut year = 1970;
    loop {
        let len = if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 { 366 } else { 365 };
        if days < len {
            return (year, days as u32 + 1);
        }
        days -= len;
        year += 1;
    }
}

#[derive(Default)]
struct Shared {
    /// a newly read analysis
    new: Option<Arc<Grid>>,
    /// whether the game wants the snow cover now (the thread only fetches then)
    want: bool,
}

struct Map {
    /// centre on the sea-level sphere
    c: DVec3,
    grid: Arc<Grid>,
}

#[derive(Default)]
pub struct SnowMap {
    shared: Arc<Mutex<Shared>>,
    started: bool,
    grid: Option<Arc<Grid>>,
    map: Option<Map>,
    job: Option<(DVec3, Arc<Grid>, Receiver<Vec<u8>>)>,
}

impl SnowMap {
    /// Once a frame: starts the thread the first time the snow cover is
    /// wanted, takes in a newly read analysis, redraws the map around the
    /// camera when it has moved away (or the analysis changed), and hands back
    /// a finished map's texels (N x N, two bytes each) to upload this frame.
    pub fn update(&mut self, src: &Arc<crate::sources::Sources>, cam: DVec3, want: bool) -> Option<Vec<u8>> {
        let new = {
            let mut s = self.shared.lock();
            s.want = want;
            s.new.take()
        };
        if want && !self.started {
            self.started = true;
            let (src, shared) = (src.clone(), self.shared.clone());
            let _ = std::thread::Builder::new().name("snow cover".into()).spawn(move || run(&src, &shared));
        }
        if let Some(g) = new {
            self.grid = Some(g);
        }
        let mut out = None;
        if let Some((c, g, rx)) = &self.job {
            match rx.try_recv() {
                Ok(px) => {
                    self.map = Some(Map { c: *c, grid: g.clone() });
                    self.job = None;
                    out = Some(px);
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.job = None,
                Err(_) => {}
            }
        }
        let Some(grid) = &self.grid else { return out };
        let (lat, lon, _) = geo::pos_to_lla(cam);
        let here = geo::lla_to_pos(lat, lon, 0.0);
        let stale = self.map.as_ref().is_none_or(|m| m.c.distance(here) > MOVE || !Arc::ptr_eq(&m.grid, grid));
        if want && stale && self.job.is_none() && lat > -0.2 {
            let (tx, rx) = channel();
            let g = grid.clone();
            std::thread::spawn(move || {
                let _ = tx.send(build(&g, here));
            });
            self.job = Some((here, grid.clone(), rx));
        }
        out
    }

    /// The day of the analysis in use (year, day of the year), for the credit.
    pub fn shown(&self) -> Option<(i32, u32)> {
        self.map.as_ref().map(|m| (m.grid.year, m.grid.ddd))
    }

    /// The snow line at a point (matching common.wgsl `snow_line_at`), for
    /// CPU-side effects such as snow spray vs dirt dust particles.
    pub fn snow_line_at(&self, p: DVec3, line: f64, doy: f64) -> f64 {
        let Some(grid) = &self.grid else { return line };
        let d = (doy - (grid.ddd as f64 - 1.0)).rem_euclid(365.0);
        let days = d.min(365.0 - d);
        let w = ((2.0 * NEAR_DAYS - days) / NEAR_DAYS).clamp(0.0, 1.0);
        if w <= 0.0 {
            return line;
        }
        let (la, lo, _) = geo::pos_to_lla(p);
        let Some((col, row)) = cell_of(la.to_degrees(), lo.to_degrees()) else { return line };
        let target = match grid.class(col, row) {
            2 => line + 800.0,
            3 => line - 1500.0,
            _ => return line,
        };
        line + (target - line) * w
    }

    /// Globals `snow_c`, `snow_e`, `snow_n`: the map's centre relative to the
    /// camera + half its side; its east axis + how much it counts (0..1, by how
    /// near the game's day `doy` (0 = January 1st) is to the analysis' day);
    /// its north axis. All zero without a map.
    pub fn globals(&self, cam: DVec3, doy: f64) -> [[f32; 4]; 3] {
        let Some(m) = &self.map else { return [[0.0; 4]; 3] };
        let d = (doy - (m.grid.ddd as f64 - 1.0)).rem_euclid(365.0);
        let days = d.min(365.0 - d);
        let w = ((2.0 * NEAR_DAYS - days) / NEAR_DAYS).clamp(0.0, 1.0);
        let (e, n, _) = geo::enu(m.c);
        let c = m.c - cam;
        let v = |a: DVec3, w: f32| [a.x as f32, a.y as f32, a.z as f32, w];
        [v(c, (N as f64 * CELL * 0.5) as f32), v(e, w as f32), v(n, 0.0)]
    }
}

/// The map around `c`: per texel, the share of it the analysis says is land
/// under snow or ice (R), and the share it says anything about the land (G;
/// open water and the other hemisphere say nothing). R / G is the snowy part
/// of what is known, also once the texels are filtered. Row 0 is the north edge.
fn build(g: &Grid, c: DVec3) -> Vec<u8> {
    let (e, n, _) = geo::enu(c);
    let mut px = vec![0u8; N * N * 2];
    const SUB: [f64; 2] = [0.25, 0.75];
    for j in 0..N {
        for i in 0..N {
            let (mut snow, mut known) = (0u32, 0u32);
            for sy in SUB {
                for sx in SUB {
                    let x = (i as f64 + sx - N as f64 / 2.0) * CELL;
                    let y = (N as f64 / 2.0 - j as f64 - sy) * CELL;
                    let (la, lo, _) = geo::pos_to_lla(c + e * x + n * y);
                    if let Some((col, row)) = cell_of(la.to_degrees(), lo.to_degrees()) {
                        match g.class(col, row) {
                            2 => known += 1,
                            3 => {
                                known += 1;
                                snow += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
            let k = (j * N + i) * 2;
            px[k] = (snow * 255 / 4) as u8;
            px[k + 1] = (known * 255 / 4) as u8;
        }
    }
    px
}

/// The thread: finds the newest analysis (on disk or online), reads it, and
/// looks again every half hour.
fn run(src: &crate::sources::Sources, shared: &Mutex<Shared>) {
    let dir = src.fetch.root.join("snow");
    let mut loaded: Option<(i32, u32)> = None;
    loop {
        if shared.lock().want {
            // Tomorrow's 00 UTC analysis is out from about 14 UTC today.
            for ago in -1..=DAYS_BACK {
                let (y, d) = utc_day(SystemTime::now(), ago);
                if loaded == Some((y, d)) {
                    break;
                }
                let path = dir.join(format!("ims{y}{d:03}_4km.asc.gz"));
                let (data, fresh) = match std::fs::read(&path) {
                    Ok(data) => (data, false),
                    Err(_) if !src.fetch.offline => match src.fetch.get(&url(y, d)) {
                        Ok(data) => (data, true),
                        Err(_) => continue,
                    },
                    Err(_) => continue,
                };
                let Some(bits) = decode(&data, GRID) else {
                    log::info!("snow cover: could not read the analysis for {y}-{d:03}");
                    if !fresh {
                        let _ = std::fs::remove_file(&path);
                    }
                    continue;
                };
                if fresh {
                    let _ = std::fs::create_dir_all(&dir);
                    let tmp = path.with_extension("tmp");
                    if std::fs::write(&tmp, &data).is_ok() {
                        let _ = std::fs::rename(&tmp, &path);
                    }
                }
                // Keep only the newest.
                for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
                    if f.path() != path && f.file_name().to_string_lossy().starts_with("ims") {
                        let _ = std::fs::remove_file(f.path());
                    }
                }
                log::info!("snow cover: IMS analysis for {y}-{d:03}{}", if fresh { " (downloaded)" } else { "" });
                loaded = Some((y, d));
                shared.lock().new = Some(Arc::new(Grid { year: y, ddd: d, bits }));
                break;
            }
        }
        std::thread::sleep(Duration::from_secs(if loaded.is_some() { 1800 } else { 60 }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn projection_matches_the_grid() {
        // Reference cells from the PROJ-style formula, checked against the
        // 18 November 2026 analysis (module doc).
        assert_eq!(cell_of(90.0, 0.0), Some((3072, 3072)));
        assert_eq!(cell_of(45.83, 6.86), Some((4279, 3005))); // Mont Blanc
        assert_eq!(cell_of(45.92, 6.87), Some((4276, 3006))); // Chamonix
        assert_eq!(cell_of(47.7, -87.5), Some((2921, 1928))); // Lake Superior
        assert_eq!(cell_of(0.0, 0.0), Some((5992, 2557)));
        assert_eq!(cell_of(-10.0, 0.0), None);
    }

    #[test]
    fn decodes_bottom_row_first() {
        let n = 4;
        let mut text = String::from("Header line\nDimensions: 4 x 4\n------------- Data set starts here: --------\n");
        // Bottom row first: row 0 all snow, the top row (last) water.
        for r in ["4444", "2342", "0000", "1111"] {
            text += r;
            text += "\n";
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(text.as_bytes()).unwrap();
        let bits = decode(&gz.finish().unwrap(), n).unwrap();
        let class = |c: usize, r: usize| {
            let i = r * n + c;
            (bits[i / 4] >> ((i % 4) * 2)) & 3
        };
        assert_eq!((class(0, 0), class(3, 0)), (3, 3));
        assert_eq!((class(0, 1), class(1, 1), class(2, 1)), (2, 3, 3));
        assert_eq!(class(0, 2), 0);
        assert_eq!(class(2, 3), 1);
        // A short file is refused.
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(b"4444\n2222\n").unwrap();
        assert!(decode(&gz.finish().unwrap(), n).is_none());
    }

    #[test]
    fn utc_days() {
        let at = |s: u64| SystemTime::UNIX_EPOCH + Duration::from_secs(s);
        assert_eq!(utc_day(at(0), 0), (1970, 1));
        // 2026-11-18 12:00 UTC is day 322; the day before and after.
        let t = at(1_795_003_200);
        assert_eq!(utc_day(t, 0), (2026, 322));
        assert_eq!(utc_day(t, 1), (2026, 321));
        assert_eq!(utc_day(t, -1), (2026, 323));
        // 2024 is a leap year: 31 December is day 366.
        assert_eq!(utc_day(at(1_735_646_400), 0), (2024, 366));
        assert_eq!(url(2026, 5), "https://noaadata.apps.nsidc.org/NOAA/G02156/4km/2026/ims2026005_00UTC_4km_v1.3.asc.gz");
    }

    #[test]
    fn map_reads_the_grid_around_the_camera() {
        // A grid with snow on everything north of 46°N in Europe (by cell),
        // bare land elsewhere.
        let mut bits = vec![0u8; (GRID * GRID).div_ceil(4)];
        // (Here, 87° east of the grid's down meridian, north is towards
        // smaller columns.)
        let (c46, _) = cell_of(46.0, 6.9).unwrap();
        for row in 0..GRID {
            for col in 0..GRID {
                let c = if col <= c46 { 3u8 } else { 2 };
                let i = row * GRID + col;
                bits[i / 4] |= c << ((i % 4) * 2);
            }
        }
        let g = Grid { year: 2026, ddd: 322, bits };
        let px = build(&g, geo::lla_to_pos(45.9f64.to_radians(), 6.9f64.to_radians(), 0.0));
        let at = |i: usize, j: usize| (px[(j * N + i) * 2], px[(j * N + i) * 2 + 1]);
        // Everything is land: known everywhere.
        assert_eq!(at(N / 2, N / 2).1, 255);
        // 100 km north is snowy, 100 km south bare.
        assert_eq!(at(N / 2, N / 2 - 25).0, 255);
        assert_eq!(at(N / 2, N / 2 + 25).0, 0);
        // The weight falls off with the distance between the days.
        let m = SnowMap { map: Some(Map { c: geo::lla_to_pos(0.8, 0.12, 0.0), grid: Arc::new(g) }), ..Default::default() };
        assert_eq!(m.globals(DVec3::ZERO, 321.0)[1][3], 1.0);
        assert_eq!(m.globals(DVec3::ZERO, 321.0 + NEAR_DAYS)[1][3], 1.0);
        assert_eq!(m.globals(DVec3::ZERO, 321.0 + 2.0 * NEAR_DAYS)[1][3], 0.0);
        assert_eq!(m.globals(DVec3::ZERO, 180.0)[1][3], 0.0);
        // Across the new year.
        let ny = SnowMap { map: Some(Map { c: DVec3::X * geo::R, grid: Arc::new(Grid { year: 2027, ddd: 1, bits: Vec::new() }) }), ..Default::default() };
        assert_eq!(ny.globals(DVec3::ZERO, 364.0)[1][3], 1.0);
    }
}
