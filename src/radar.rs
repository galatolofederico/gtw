//! Weather radar: where it is really raining and storming now, for live
//! weather (livewx.rs). RainViewer's free composite of the world's radar
//! networks (https://www.rainviewer.com/api.html, no key) comes as Web
//! Mercator tiles: a 3x3 block of zoom-6 tiles round the aircraft (some
//! 2 km a pixel, 600-1700 km across) and the matching coverage tiles (where
//! there is no radar at all). `analyse` turns the echoes into storm cells at
//! the intense convective cores (bigger and more mature for bigger, stronger
//! echoes) and a place for the rain deck over the widespread lighter rain.
//!
//! The free tiles only come in one palette (Universal Blue, a colour per dBZ),
//! so each pixel is read back to its reflectivity through RainViewer's table.

use crate::geo;
use std::collections::HashMap;
use std::f64::consts::PI;

/// Zoom of the radar tiles (the free API serves up to 7).
pub const Z: u8 = 6;
/// Storm cells are made out to this far from the aircraft, km.
const RANGE_KM: f64 = 330.0;
/// The rain deck's centre is looked for out to this far, km.
const DECK_RANGE_KM: f64 = 200.0;
/// Widespread rain: at least this much reflectivity (dBZ; light rain is ~20).
const RAIN_DBZ: f32 = 18.0;
/// The weakest echo a shower cell grows from, dBZ.
const SHOWER_DBZ: f32 = 30.0;
/// Most cells made (nearest kept).
const MAX_CELLS: usize = 64;

/// RainViewer's Universal Blue colours (RGBA) for each reflectivity (dBZ),
/// from https://www.rainviewer.com/files/rainviewer_api_colors_table.csv.
const PALETTE: [(i8, u32); 77] = [
    (-10, 0x63615914), (-9, 0x66635a19), (-8, 0x69665c1e), (-7, 0x6c685d24), (-6, 0x6f6b5f29), (-5, 0x726e612e), (-4, 0x75706234), (-3, 0x78736439),
    (-2, 0x7c75653e), (-1, 0x7f786744), (0, 0x827b6949), (1, 0x857d6a4e), (2, 0x88806c54), (3, 0x8b826d59), (4, 0x8e856f5e), (5, 0x92887164),
    (6, 0x9e93756e), (7, 0xaa9e7978), (8, 0xb6a97e82), (9, 0xc2b4828c), (10, 0xcec08796), (11, 0xd2c48ba0), (12, 0xd6c88faa), (13, 0xdacc93b4),
    (14, 0xded097be), (15, 0x88ddeeff), (16, 0x6cd1ebff), (17, 0x51c5e8ff), (18, 0x36bae5ff), (19, 0x1baee2ff), (20, 0x00a3e0ff), (21, 0x009ad5ff),
    (22, 0x0091caff), (23, 0x0088bfff), (24, 0x007fb4ff), (25, 0x0077aaff), (26, 0x0070a3ff), (27, 0x00699cff), (28, 0x006295ff), (29, 0x005b8eff),
    (30, 0x005588ff), (31, 0x005180ff), (32, 0x004e78ff), (33, 0x004a70ff), (34, 0x004768ff), (35, 0xffee00ff), (36, 0xffe000ff), (37, 0xffd200ff),
    (38, 0xffc500ff), (39, 0xffb700ff), (40, 0xffaa00ff), (41, 0xff9f00ff), (42, 0xff9500ff), (43, 0xff8b00ff), (44, 0xff8100ff), (45, 0xff4400ff),
    (46, 0xf23600ff), (47, 0xe62800ff), (48, 0xd91b00ff), (49, 0xcd0d00ff), (50, 0xc10000ff), (51, 0xa80000ff), (52, 0x8f0000ff), (53, 0x760000ff),
    (54, 0x5d0000ff), (55, 0xffaaffff), (56, 0xff9fffff), (57, 0xff95ffff), (58, 0xff8bffff), (59, 0xff81ffff), (60, 0xff77ffff), (61, 0xff6cffff),
    (62, 0xff62ffff), (63, 0xff58ffff), (64, 0xff4effff), (65, 0xffffffff), (75, 0x00ff00ff),
];

fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A radar tile's pixels as reflectivity (dBZ; NaN = no echo), row by row.
pub fn decode_tile(png: &[u8]) -> Result<Vec<f32>, String> {
    let img = image::load_from_memory(png).map_err(|e| e.to_string())?.to_rgba8();
    if img.dimensions() != (256, 256) {
        return Err(format!("radar tile is {:?}", img.dimensions()));
    }
    let exact: HashMap<u32, f32> = PALETTE.iter().map(|&(d, c)| (c, d as f32)).collect();
    let mut seen: HashMap<u32, f32> = HashMap::new();
    Ok(img
        .pixels()
        .map(|p| {
            if p[3] == 0 {
                return f32::NAN;
            }
            let c = u32::from_be_bytes(p.0);
            *seen.entry(c).or_insert_with(|| exact.get(&c).copied().unwrap_or_else(|| nearest(p.0)))
        })
        .collect())
}

/// The palette entry closest to a colour (should the tiles ever be smoothed).
fn nearest(p: [u8; 4]) -> f32 {
    let d = |c: u32| c.to_be_bytes().iter().zip(p).map(|(&a, b)| (a as i32 - b as i32).pow(2)).sum::<i32>();
    PALETTE.iter().min_by_key(|&&(_, c)| d(c)).map(|&(z, _)| z as f32).unwrap_or(f32::NAN)
}

/// A coverage tile: true where some radar sees (RainViewer paints the places
/// no radar reaches opaque black).
pub fn decode_cover(png: &[u8]) -> Result<Vec<bool>, String> {
    let img = image::load_from_memory(png).map_err(|e| e.to_string())?.to_rgba8();
    if img.dimensions() != (256, 256) {
        return Err(format!("coverage tile is {:?}", img.dimensions()));
    }
    Ok(img.pixels().map(|p| p[3] < 128).collect())
}

/// Reflectivity over a block of tiles: pixel (i, j) is global Web Mercator
/// pixel (gx0 + i, gy0 + j) at zoom `z` (256 pixels a tile).
pub struct Echo {
    pub z: u8,
    pub gx0: i64,
    pub gy0: i64,
    pub w: usize,
    pub h: usize,
    /// dBZ, NaN = no echo
    pub dbz: Vec<f32>,
    /// some radar sees there
    pub cover: Vec<bool>,
}

fn world(z: u8) -> f64 {
    256.0 * (1u64 << z) as f64
}

/// Degrees to global pixel coordinates at zoom z.
pub fn to_px(z: u8, lat: f64, lon: f64) -> (f64, f64) {
    let w = world(z);
    (geo::lon_to_merc_x(lon.to_radians()) * w, geo::lat_to_merc_y(lat.to_radians()) * w)
}

fn to_deg(z: u8, gx: f64, gy: f64) -> (f64, f64) {
    let w = world(z);
    (geo::merc_y_to_lat(gy / w).to_degrees(), gx / w * 360.0 - 180.0)
}

/// Great-circle distance, km.
pub fn dist_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, la2) = (a.0.to_radians(), b.0.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * ((b.1 - a.1).to_radians() / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.sqrt().min(1.0).asin()
}

impl Echo {
    /// The tiles round (lat, lon): (zoom, first tile x, first tile y, tiles
    /// across). x wraps round the date line (take it modulo 2^z).
    pub fn block(lat: f64, lon: f64) -> (u8, i64, i64, i64) {
        let (gx, gy) = to_px(Z, lat, lon);
        let n = 1i64 << Z;
        let (tx, ty) = ((gx / 256.0).floor() as i64, (gy / 256.0).floor() as i64);
        (Z, tx - 1, (ty - 1).clamp(0, n - 3), 3)
    }

    /// Centre of pixel (i, j), degrees.
    fn deg(&self, i: f64, j: f64) -> (f64, f64) {
        to_deg(self.z, self.gx0 as f64 + i + 0.5, self.gy0 as f64 + j + 0.5)
    }

    /// km a pixel spans on row j (Mercator is conformal: the same both ways).
    fn km_px(&self, j: f64) -> f64 {
        let (lat, _) = self.deg(0.0, j);
        40075.017 * lat.to_radians().cos() / world(self.z)
    }

    /// (lat, lon) as pixel coordinates of this block (may fall outside it).
    fn local(&self, lat: f64, lon: f64) -> (f64, f64) {
        let (gx, gy) = to_px(self.z, lat, lon);
        let w = world(self.z);
        let mut x = gx - self.gx0 as f64;
        // the block may run past the date line
        x -= (x / w).floor() * w;
        (x - 0.5, gy - self.gy0 as f64 - 0.5)
    }
}

/// A storm cell where the radar sees a convective core.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub lat: f64,
    pub lon: f64,
    /// tower radius, km
    pub rad: f64,
    /// maturity, 0..1 (mature cells spread an anvil)
    pub age: f64,
    /// strongest echo in it, dBZ
    pub peak: f32,
    /// a squall line: its length (km; 0 = a lone cell), the compass bearing
    /// its axis runs along (degrees, 0..180), and the cells it was made of
    pub len: f64,
    pub axis: f64,
    pub parts: Vec<Cell>,
}

/// Where the rain deck goes: the middle of an area of widespread rain.
#[derive(Clone, Debug, PartialEq)]
pub struct Deck {
    pub lat: f64,
    pub lon: f64,
    /// the Rain slider (deck amount) for it, 0..1
    pub amount: f32,
    /// how much of the deck's area it rains on, 0..1
    pub frac: f32,
}

/// How much of the ground some radar sees, on a coarse grid (to fade the
/// made-up storm cells out where the real ones take over).
#[derive(Clone, Debug, Default)]
pub struct Cover {
    z: u8,
    gx0: i64,
    gy0: i64,
    /// pixels per block
    block: usize,
    w: usize,
    h: usize,
    frac: Vec<f32>,
}

impl Cover {
    /// 0..1: none to full radar coverage at (lat, lon) degrees (0 outside
    /// the block of tiles fetched).
    pub fn at(&self, lat: f64, lon: f64) -> f64 {
        if self.frac.is_empty() {
            return 0.0;
        }
        let (gx, gy) = to_px(self.z, lat, lon);
        let wp = world(self.z);
        let mut x = gx - self.gx0 as f64;
        x -= (x / wp).floor() * wp;
        let b = self.block as f64;
        let (x, y) = (x / b - 0.5, (gy - self.gy0 as f64) / b - 0.5);
        if x < -0.5 || y < -0.5 || x > self.w as f64 - 0.5 || y > self.h as f64 - 0.5 {
            return 0.0;
        }
        let (x, y) = (x.clamp(0.0, self.w as f64 - 1.0), y.clamp(0.0, self.h as f64 - 1.0));
        let (i, j) = ((x as usize).min(self.w.saturating_sub(2)), (y as usize).min(self.h.saturating_sub(2)));
        let (fx, fy) = ((x - i as f64).clamp(0.0, 1.0), (y - j as f64).clamp(0.0, 1.0));
        let g = |a: usize, b: usize| self.frac[(b.min(self.h - 1)) * self.w + a.min(self.w - 1)] as f64;
        let top = g(i, j) * (1.0 - fx) + g(i + 1, j) * fx;
        let bot = g(i, j + 1) * (1.0 - fx) + g(i + 1, j + 1) * fx;
        top * (1.0 - fy) + bot * fy
    }
}

/// What the radar says round the aircraft.
#[derive(Clone, Debug, Default)]
pub struct Radar {
    /// nearest first
    pub cells: Vec<Cell>,
    pub deck: Option<Deck>,
    pub cover: Cover,
    /// some radar sees where the aircraft is
    pub here: bool,
    /// when the radar frame was taken (Unix seconds)
    pub time: i64,
}

/// Summed-area table of a w x h grid.
fn sat(w: usize, h: usize, v: impl Fn(usize) -> f64) -> Vec<f64> {
    let mut s = vec![0.0; (w + 1) * (h + 1)];
    for j in 0..h {
        let mut row = 0.0;
        for i in 0..w {
            row += v(j * w + i);
            s[(j + 1) * (w + 1) + i + 1] = s[j * (w + 1) + i + 1] + row;
        }
    }
    s
}
/// Sum over the box [i0, i1) x [j0, j1).
fn box_sum(s: &[f64], w: usize, i0: usize, j0: usize, i1: usize, j1: usize) -> f64 {
    let w1 = w + 1;
    s[j1 * w1 + i1] - s[j0 * w1 + i1] - s[j1 * w1 + i0] + s[j0 * w1 + i0]
}

fn lin(dbz: f32) -> f64 {
    10f64.powf(dbz as f64 / 10.0)
}
fn dbz(z: f64) -> f64 {
    10.0 * z.max(1e-3).log10()
}

/// The rain deck amount for a mean reflectivity: light rain (~20 dBZ, under a
/// millimetre an hour) a thin deck, heavy (40+ dBZ) the darkest.
pub fn deck_amount(dbz: f64) -> f32 {
    (0.2 + (dbz - 18.0) / 22.0 * 0.65).clamp(0.15, 0.85) as f32
}

/// The storm cells and the rain deck's place for the echoes round `at`
/// (degrees).
pub fn analyse(e: &Echo, at: (f64, f64)) -> Radar {
    let (w, h) = (e.w, e.h);
    let echo = |k: usize| !e.dbz[k].is_nan();
    let (ax, ay) = e.local(at.0, at.1);
    let cover = coarse_cover(e, 8);
    let here = cover.at(at.0, at.1) > 0.5;

    // Convective or stratiform (after Steiner, Houze & Yuter 1995): a core is
    // strong (40 dBZ) or stands well clear of the mean echo within 11 km.
    let zsum = sat(w, h, |k| if echo(k) { lin(e.dbz[k]) } else { 0.0 });
    let nsum = sat(w, h, |k| echo(k) as u8 as f64);
    let mut conv = vec![false; w * h];
    for j in 0..h {
        let r = (11.0 / e.km_px(j as f64)).round().max(1.0) as usize;
        let (j0, j1) = (j.saturating_sub(r), (j + r + 1).min(h));
        for i in 0..w {
            let k = j * w + i;
            let z = e.dbz[k];
            if !(z >= SHOWER_DBZ) {
                continue;
            }
            let (i0, i1) = (i.saturating_sub(r), (i + r + 1).min(w));
            let n = box_sum(&nsum, w, i0, j0, i1, j1);
            let bg = dbz(box_sum(&zsum, w, i0, j0, i1, j1) / n.max(1.0)).max(0.0);
            let dz = if bg < 42.43 { 10.0 - bg * bg / 180.0 } else { 0.0 };
            conv[k] = z >= 40.0 || z as f64 - bg >= dz;
        }
    }

    // Cells: the strongest cores first (3x3 mean, so lone noisy pixels don't
    // lead); each takes its neighbourhood, so a big complex becomes a cluster
    // of giant cells and a lone shower one small one.
    let csum = sat(w, h, |k| if conv[k] { lin(e.dbz[k]) } else { 0.0 });
    let mut cand: Vec<(f32, usize)> = (0..w * h)
        .filter(|&k| conv[k])
        .map(|k| {
            let (i, j) = (k % w, k / w);
            let s = box_sum(&csum, w, i.saturating_sub(1), j.saturating_sub(1), (i + 2).min(w), (j + 2).min(h));
            let n = box_sum(&nsum, w, i.saturating_sub(1), j.saturating_sub(1), (i + 2).min(w), (j + 2).min(h));
            (dbz(s / n.max(1.0)) as f32, k)
        })
        .collect();
    cand.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut taken = vec![false; w * h];
    let mut cells: Vec<(f64, Cell)> = Vec::new();
    for &(peak, k) in &cand {
        if taken[k] || peak < SHOWER_DBZ {
            continue;
        }
        let (i, j) = ((k % w) as f64, (k / w) as f64);
        let kp = e.km_px(j);
        let range_px = RANGE_KM / kp;
        if (i - ax).hypot(j - ay) > range_px {
            continue;
        }
        // How much convective echo there is round it (within 20 km), and its
        // centre of weight close in.
        let rm = (20.0 / kp).ceil() as i64;
        let rc = (6.0 / kp).max(1.5);
        let (mut n, mut sw, mut sx, mut sy) = (0usize, 0.0, 0.0, 0.0);
        for dj in -rm..=rm {
            for di in -rm..=rm {
                let d = (di as f64).hypot(dj as f64);
                let (x, y) = (i as i64 + di, j as i64 + dj);
                if d > rm as f64 || x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                    continue;
                }
                let q = y as usize * w + x as usize;
                if !conv[q] {
                    continue;
                }
                n += 1;
                if d <= rc {
                    let wt = lin(e.dbz[q]);
                    (sw, sx, sy) = (sw + wt, sx + wt * x as f64, sy + wt * y as f64);
                }
            }
        }
        let area = n as f64 * kp * kp;
        // A pixel or two: radar noise or a cumulus too small to bother with.
        if area < 10.0 {
            taken[k] = true;
            continue;
        }
        let r_eq = (area / PI).sqrt();
        let p = peak as f64;
        let rad = (1.7 + 0.38 * r_eq + 3.5 * smoothstep(5.0, 16.0, r_eq) + 1.8 * smoothstep(45.0, 58.0, p)).clamp(2.2, 15.0);
        let age = (0.1 + 0.9 * smoothstep(32.0, 52.0, p)).max(0.9 * smoothstep(8.0, 13.0, rad)).min(1.0);
        let (cx, cy) = if sw > 0.0 { (sx / sw, sy / sw) } else { (i, j) };
        // Its neighbourhood is its own: the next cell starts beyond it.
        let rs = (1.7 * rad + 4.0).max(10.0) / kp;
        let r = rs.ceil() as i64;
        for dj in -r..=r {
            for di in -r..=r {
                let (x, y) = (cx.round() as i64 + di, cy.round() as i64 + dj);
                if (di as f64).hypot(dj as f64) <= rs && x >= 0 && y >= 0 && x < w as i64 && y < h as i64 {
                    taken[y as usize * w + x as usize] = true;
                }
            }
        }
        taken[k] = true;
        let (lat, lon) = e.deg(cx, cy);
        cells.push(((cx - ax).hypot(cy - ay) * kp, Cell { lat, lon, rad, age, peak, len: 0.0, axis: 0.0, parts: Vec::new() }));
    }
    cells.sort_by(|a, b| a.0.total_cmp(&b.0));
    cells.truncate(MAX_CELLS);
    let cells = find_supercells(find_lines(cells, at), at);

    Radar { cells: cells.into_iter().map(|c| c.1).collect(), deck: find_deck(e, (ax, ay)), cover, here, time: 0 }
}

/// A squall line is at least this long, km.
const LINE_KM: f64 = 60.0;

/// A supercell on the radar: a large lone cell with a very intense core (hail,
/// 57 dBZ and up) and no other strong echo (40 dBZ) within `SUPER_KM` km. Its
/// maturity becomes 2 (weather.rs CellKind::Supercell). (distance km, cell)
/// in and out.
const SUPER_DBZ: f32 = 57.0;
const SUPER_KM: f64 = 50.0;
fn find_supercells(mut cells: Vec<(f64, Cell)>, at: (f64, f64)) -> Vec<(f64, Cell)> {
    let km = |c: &Cell| ((c.lon - at.1).to_radians() * 6371.0 * at.0.to_radians().cos(), (c.lat - at.0).to_radians() * 6371.0);
    let pos: Vec<(f64, f64)> = cells.iter().map(|c| km(&c.1)).collect();
    let big: Vec<bool> = cells
        .iter()
        .enumerate()
        .map(|(i, (_, c))| {
            c.len == 0.0
                && c.peak >= SUPER_DBZ
                && c.rad >= 8.5
                && cells.iter().enumerate().all(|(j, (_, o))| j == i || o.peak < 40.0 || (pos[i].0 - pos[j].0).hypot(pos[i].1 - pos[j].1) - o.len * 0.5 > SUPER_KM)
        })
        .collect();
    for (c, b) in cells.iter_mut().zip(big) {
        if b {
            c.1.age = 2.0;
        }
    }
    cells
}

/// Strong cells (45 dBZ and up) close together in a long, narrow row become
/// one squall line (`Cell::len` > 0, its cells kept in `parts`). Chains of
/// neighbours (gaps no wider than the cells) of four or more, at least
/// `LINE_KM` long end to end and no more than a fifth as wide as long (a
/// cluster or a round complex stays cells). (distance km, cell) in and out,
/// nearest first.
fn find_lines(cells: Vec<(f64, Cell)>, at: (f64, f64)) -> Vec<(f64, Cell)> {
    // km east and north of `at` (flat: a few hundred km at most)
    let km = |c: &Cell| ((c.lon - at.1).to_radians() * 6371.0 * at.0.to_radians().cos(), (c.lat - at.0).to_radians() * 6371.0);
    let strong: Vec<usize> = (0..cells.len()).filter(|&i| cells[i].1.peak >= 45.0 && cells[i].1.len == 0.0).collect();
    let n = cells.len();
    let mut group: Vec<usize> = (0..n).collect();
    fn root(g: &mut [usize], mut i: usize) -> usize {
        while g[i] != i {
            g[i] = g[g[i]];
            i = g[i];
        }
        i
    }
    for (k, &i) in strong.iter().enumerate() {
        for &j in &strong[k + 1..] {
            let (a, b) = (km(&cells[i].1), km(&cells[j].1));
            if (a.0 - b.0).hypot(a.1 - b.1) < 1.6 * (cells[i].1.rad + cells[j].1.rad) + 8.0 {
                let (ri, rj) = (root(&mut group, i), root(&mut group, j));
                group[ri] = rj;
            }
        }
    }
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
    for &i in &strong {
        let r = root(&mut group, i);
        members.entry(r).or_default().push(i);
    }
    let mut used = vec![false; n];
    let mut lines = Vec::new();
    for m in members.values().filter(|m| m.len() >= 4) {
        // The row's axis: the main axis of its cells' spread.
        let p: Vec<(f64, f64)> = m.iter().map(|&i| km(&cells[i].1)).collect();
        let c = (p.iter().map(|q| q.0).sum::<f64>() / p.len() as f64, p.iter().map(|q| q.1).sum::<f64>() / p.len() as f64);
        let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
        for q in &p {
            let (dx, dy) = (q.0 - c.0, q.1 - c.1);
            (sxx, syy, sxy) = (sxx + dx * dx, syy + dy * dy, sxy + dx * dy);
        }
        let th = 0.5 * (2.0 * sxy).atan2(sxx - syy);
        let (ex, ey) = (th.cos(), th.sin());
        let along: Vec<f64> = p.iter().map(|q| (q.0 - c.0) * ex + (q.1 - c.1) * ey).collect();
        let across = (p.iter().map(|q| ((q.0 - c.0) * -ey + (q.1 - c.1) * ex).powi(2)).sum::<f64>() / p.len() as f64).sqrt();
        let (lo, hi) = along.iter().fold((f64::MAX, f64::MIN), |(a, b), &v| (a.min(v), b.max(v)));
        let rad = m.iter().map(|&i| cells[i].1.rad).sum::<f64>() / m.len() as f64;
        let len = hi - lo;
        if len + 2.0 * rad < LINE_KM || across > 0.2 * (len + 2.0 * rad) {
            continue;
        }
        let mid = 0.5 * (lo + hi);
        let (x, y) = (c.0 + ex * mid, c.1 + ey * mid);
        let lat = at.0 + (y / 6371.0).to_degrees();
        let lon = at.1 + (x / (6371.0 * at.0.to_radians().cos())).to_degrees();
        let parts: Vec<Cell> = m.iter().map(|&i| cells[i].1.clone()).collect();
        let age = parts.iter().map(|c| c.age).fold(0.0, f64::max).max(0.9);
        let peak = parts.iter().map(|c| c.peak).fold(0.0, f32::max);
        m.iter().for_each(|&i| used[i] = true);
        lines.push((x.hypot(y).max(0.0), Cell { lat, lon, rad: rad.clamp(7.0, 11.5), age, peak, len, axis: ex.atan2(ey).to_degrees().rem_euclid(180.0), parts }));
    }
    let mut out: Vec<(f64, Cell)> = cells.into_iter().enumerate().filter(|(i, _)| !used[*i]).map(|(_, c)| c).chain(lines).collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Coverage on blocks of `b` pixels.
fn coarse_cover(e: &Echo, b: usize) -> Cover {
    let (bw, bh) = (e.w.div_ceil(b), e.h.div_ceil(b));
    let mut frac = vec![0.0f32; bw * bh];
    let mut cnt = vec![0u32; bw * bh];
    for j in 0..e.h {
        for i in 0..e.w {
            let q = (j / b) * bw + i / b;
            cnt[q] += 1;
            frac[q] += e.cover[j * e.w + i] as u8 as f32;
        }
    }
    for (f, n) in frac.iter_mut().zip(cnt) {
        *f /= n.max(1) as f32;
    }
    Cover { z: e.z, gx0: e.gx0, gy0: e.gy0, block: b, w: bw, h: bh, frac }
}

/// The best place for the rain deck (an area some 90 km across): where most
/// of it is raining, the nearer the aircraft the better; None if it rains
/// widely nowhere near.
fn find_deck(e: &Echo, (ax, ay): (f64, f64)) -> Option<Deck> {
    const B: usize = 4;
    let (bw, bh) = (e.w.div_ceil(B), e.h.div_ceil(B));
    // per block: covered pixels, raining pixels, their summed dBZ (a log
    // mean, capped at 45, so storm cores don't make a downpour of the whole area)
    let mut blk = vec![(0u32, 0u32, 0.0f64); bw * bh];
    for j in 0..e.h {
        for i in 0..e.w {
            let k = j * e.w + i;
            let b = &mut blk[(j / B) * bw + i / B];
            if e.cover[k] {
                b.0 += 1;
            }
            let z = e.dbz[k];
            if z >= RAIN_DBZ {
                b.1 += 1;
                b.2 += z.min(45.0) as f64;
            }
        }
    }
    let mut best: Option<(f64, Deck)> = None;
    let r_area = 40.0;
    for bj in 0..bh {
        for bi in 0..bw {
            let (x, y) = ((bi * B) as f64 + B as f64 / 2.0 - 0.5, (bj * B) as f64 + B as f64 / 2.0 - 0.5);
            let kp = e.km_px(y);
            let d = (x - ax).hypot(y - ay) * kp;
            if d > DECK_RANGE_KM {
                continue;
            }
            let rb = r_area / kp / B as f64;
            let ri = rb.ceil() as i64;
            let (mut cov, mut rain, mut zs, mut all) = (0u32, 0u32, 0.0, 0u32);
            for dj in -ri..=ri {
                for di in -ri..=ri {
                    let (u, v) = (bi as i64 + di, bj as i64 + dj);
                    if (di as f64).hypot(dj as f64) > rb || u < 0 || v < 0 || u >= bw as i64 || v >= bh as i64 {
                        continue;
                    }
                    let b = blk[v as usize * bw + u as usize];
                    all += (B * B) as u32;
                    cov += b.0;
                    rain += b.1;
                    zs += b.2;
                }
            }
            // Half of it out of sight of any radar (or off the block): no judging it.
            if cov * 2 < all || rain == 0 {
                continue;
            }
            let frac = rain as f64 / cov as f64;
            if frac < 0.45 {
                continue;
            }
            let score = frac * 80.0 / (80.0 + d);
            if best.as_ref().is_none_or(|b| score > b.0) {
                let (lat, lon) = e.deg(x, y);
                best = Some((score, Deck { lat, lon, amount: deck_amount(zs / rain as f64), frac: frac as f32 }));
            }
        }
    }
    best.map(|b| b.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3x3 tile block round (lat, lon), all seen by radar, no echoes.
    fn empty(lat: f64, lon: f64) -> Echo {
        let (z, tx, ty, n) = Echo::block(lat, lon);
        let s = 256 * n as usize;
        Echo { z, gx0: tx * 256, gy0: ty * 256, w: s, h: s, dbz: vec![f32::NAN; s * s], cover: vec![true; s * s] }
    }

    /// Paint echoes: `f(km east, km north)` of `c` gives the dBZ (None = leave).
    fn paint(e: &mut Echo, c: (f64, f64), f: impl Fn(f64, f64) -> Option<f32>) {
        for j in 0..e.h {
            for i in 0..e.w {
                let (lat, lon) = e.deg(i as f64, j as f64);
                let x = (lon - c.1).to_radians() * 6371.0 * c.0.to_radians().cos();
                let y = (lat - c.0).to_radians() * 6371.0;
                if let Some(z) = f(x, y) {
                    e.dbz[j * e.w + i] = z;
                }
            }
        }
    }

    /// (lat, lon) `km` away towards `bearing` degrees (flat-earth, short range).
    fn offset(c: (f64, f64), km: f64, bearing: f64) -> (f64, f64) {
        let b = bearing.to_radians();
        (c.0 + (km * b.cos() / 6371.0).to_degrees(), c.1 + (km * b.sin() / (6371.0 * c.0.to_radians().cos())).to_degrees())
    }

    #[test]
    fn palette_reads_back() {
        let mut img = image::RgbaImage::new(256, 256);
        img.put_pixel(0, 0, image::Rgba(0x00a3e0ffu32.to_be_bytes()));
        img.put_pixel(1, 0, image::Rgba(0xcd0d00ffu32.to_be_bytes()));
        img.put_pixel(2, 0, image::Rgba([0xcc, 0x0e, 0x01, 0xff]));
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let d = decode_tile(&png).unwrap();
        assert_eq!((d[0], d[1], d[2]), (20.0, 49.0, 49.0));
        assert!(d[3].is_nan());
    }

    #[test]
    fn no_echo_no_weather() {
        let at = (-28.0, 152.5);
        let r = analyse(&empty(at.0, at.1), at);
        assert!(r.cells.is_empty() && r.deck.is_none() && r.here);
        assert!((r.cover.at(at.0, at.1) - 1.0).abs() < 1e-6);
        // far outside the tiles: no coverage known
        assert_eq!(r.cover.at(10.0, 20.0), 0.0);
    }

    #[test]
    fn small_shower_small_young_cell() {
        let at = (-28.0, 152.5);
        let mut e = empty(at.0, at.1);
        let c = offset(at, 60.0, 90.0);
        paint(&mut e, c, |x, y| {
            let d = x.hypot(y);
            (d < 4.0).then(|| 37.0 - d as f32).or((d < 8.0).then_some(20.0))
        });
        let r = analyse(&e, at);
        assert_eq!(r.cells.len(), 1, "{:?}", r.cells);
        let cell = &r.cells[0];
        assert!(dist_km((cell.lat, cell.lon), c) < 3.0, "{cell:?}");
        assert!(cell.rad < 3.5 && cell.age < 0.5, "{cell:?}");
        // a shower is no rain deck
        assert!(r.deck.is_none(), "{:?}", r.deck);
    }

    #[test]
    fn big_complex_giant_mature_cells() {
        let at = (35.0, -97.0);
        let mut e = empty(at.0, at.1);
        // a squall line 120 km long, 30 km deep, 50 dBZ with 58 dBZ cores,
        // trailing stratiform rain behind it
        let c = offset(at, 90.0, 0.0);
        paint(&mut e, c, |x, y| {
            if x.abs() < 60.0 && y.abs() < 15.0 {
                Some(if (x - 20.0).hypot(y) < 6.0 || (x + 30.0).hypot(y) < 6.0 { 58.0 } else { 50.0 })
            } else if x.abs() < 70.0 && y > 15.0 && y < 60.0 {
                Some(28.0)
            } else {
                None
            }
        });
        let r = analyse(&e, at);
        // ...is one squall line along it, made of big mature cells
        let lines: Vec<&Cell> = r.cells.iter().filter(|c| c.len > 0.0).collect();
        assert_eq!(lines.len(), 1, "{:?}", r.cells);
        let line = lines[0];
        assert!(dist_km((line.lat, line.lon), c) < 15.0 && (60.0..130.0).contains(&line.len), "{line:?}");
        assert!((line.axis - 90.0).abs() < 15.0, "an east-west line: {line:?}");
        let r = Radar { cells: line.parts.clone(), ..r };
        assert!(r.cells.len() >= 3, "{:?}", r.cells);
        for cell in &r.cells {
            let (x, y) = ((cell.lon - c.1).to_radians() * 6371.0 * c.0.to_radians().cos(), (cell.lat - c.0).to_radians() * 6371.0);
            assert!(x.abs() < 62.0 && y.abs() < 17.0, "cell off the line: {cell:?} at {x:.0},{y:.0}");
            assert!(cell.rad > 6.0 && cell.age > 0.85, "{cell:?}");
        }
        // the strongest cores became the biggest cells
        let big = r.cells.iter().map(|c| c.rad).fold(0.0, f64::max);
        assert!(big > 8.5, "{:?}", r.cells);
        // the stratiform rain is not made into storm cells
        assert!(r.cells.iter().all(|c| c.peak >= 45.0));
    }

    #[test]
    fn widespread_rain_brings_the_deck() {
        let at = (51.0, 5.0);
        let mut e = empty(at.0, at.1);
        // 150 km of light-to-moderate rain to the south-west, the aircraft
        // inside it near its north-east corner
        let c = offset(at, 80.0, 225.0);
        paint(&mut e, c, |x, y| (x.abs() < 75.0 && y.abs() < 75.0).then_some(26.0 + ((x * 0.3).sin() * 2.0) as f32));
        let r = analyse(&e, at);
        assert!(r.cells.is_empty(), "uniform rain is not convection: {:?}", r.cells);
        let d = r.deck.expect("a deck");
        // inside the rain, as close to the aircraft as it is still all rain
        let (x, y) = ((d.lon - c.1).to_radians() * 6371.0 * c.0.to_radians().cos(), (d.lat - c.0).to_radians() * 6371.0);
        assert!(x.abs() < 60.0 && y.abs() < 60.0, "{d:?} at {x:.0},{y:.0}");
        assert!(dist_km((d.lat, d.lon), at) < 40.0, "{d:?}");
        // rain far off to the west only: the deck goes there
        let mut e = empty(at.0, at.1);
        let c = offset(at, 150.0, 270.0);
        paint(&mut e, c, |x, y| (x.hypot(y) < 60.0).then_some(22.0));
        let d = analyse(&e, at).deck.expect("a deck");
        assert!(dist_km((d.lat, d.lon), c) < 25.0, "{d:?}");
        assert!(d.frac > 0.9 && (0.3..0.6).contains(&d.amount), "{d:?}");
    }

    #[test]
    fn no_radar_no_judgement() {
        let at = (20.0, -40.0);
        let mut e = empty(at.0, at.1);
        e.cover.iter_mut().for_each(|c| *c = false);
        let r = analyse(&e, at);
        assert!(!r.here && r.deck.is_none() && r.cells.is_empty());
        assert_eq!(r.cover.at(at.0, at.1), 0.0);
        // half seen: coverage fades across the edge
        let mut e = empty(at.0, at.1);
        let half = e.w / 2;
        for j in 0..e.h {
            for i in 0..half {
                e.cover[j * e.w + i] = false;
            }
        }
        let r = analyse(&e, at);
        let (lat, lon) = e.deg(half as f64, 300.0);
        let (la2, lo2) = e.deg(half as f64 - 40.0, 300.0);
        let (la3, lo3) = e.deg(half as f64 + 40.0, 300.0);
        assert!(r.cover.at(la2, lo2) < 0.01 && r.cover.at(la3, lo3) > 0.99, "{} {}", r.cover.at(la2, lo2), r.cover.at(la3, lo3));
        assert!((0.2..0.8).contains(&r.cover.at(lat, lon)));
    }

    #[test]
    fn block_wraps_the_date_line() {
        let at = (-17.0, 179.9);
        let e = empty(at.0, at.1);
        let (x, y) = e.local(at.0, at.1);
        assert!(x > 256.0 && x < 512.0 && y > 256.0 && y < 512.0, "{x} {y}");
        let (x, _) = e.local(at.0, -179.9);
        assert!(x > 256.0 && x < 768.0, "{x}");
    }
}
