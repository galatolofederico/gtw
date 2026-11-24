//! Smooth pavement in the photorealistic 3D tiles (tiles3d.rs).
//!
//! Photogrammetry streets and runways are lumpy: cars and buses melted into
//! the road, waves of half a metre along a runway, noise everywhere. As each
//! tile is decoded, the vertices on a road's carriageway or on airfield
//! pavement (from the same OpenStreetMap vector tiles as our own roads) are
//! moved onto a smooth surface fitted through the mesh's own pavement. The
//! fit is robust (cars and the like are left out of it) and follows the mesh's
//! level, so the street still meets its kerbs and buildings; the collision
//! mesh is built afterwards, so the aircraft rolls on what is drawn.
//!
//! Left alone: bridges and tunnels (a deck over a street must not be pulled
//! down to it, nor the street under it up), and anything well above the
//! fitted surface (tree crowns, walls the map's outline strays onto, parked
//! aircraft on aprons).
use crate::geo::{self, TileId};
use crate::sources::{Sources, AP_APRON, AP_HELIPAD, AP_RUNWAY, AP_TAXIWAY, RC_RAIL, RC_SERVICE};
use glam::{DVec3, Vec2, Vec3};
use std::sync::atomic::{AtomicBool, Ordering};

/// Off with `--no-pave` (to compare).
pub static ON: AtomicBool = AtomicBool::new(true);

/// Zoom of the vector tiles the pavement comes from.
const Z: u8 = 14;
/// Tiles wider than this (m) are left as they are: drawn far off, never
/// stood on, and they would take many vector tiles.
const MAX_EXTENT: f32 = 3000.0;

// Raster kinds, in rising priority.
const ROAD: u8 = 1;
const APRON: u8 = 2;
const RUNWAY: u8 = 3;
const BRIDGE: u8 = 4;

/// A fitted surface: cell size (m), the blur of each refining pass (m), and
/// the band about the previous pass's surface that a vertex must lie in to
/// count in the next (m below, m above).
struct Fit {
    cell: f32,
    sigma: [f32; 3],
    band: [(f32, f32); 2],
}
/// Streets: short smoothing (they climb and turn), cars left out.
const FIT_ROAD: Fit = Fit { cell: 2.0, sigma: [6.0, 4.5, 3.5], band: [(1.0, 0.8), (0.4, 0.4)] };
/// Runways, taxiways and aprons: long, gentle smoothing.
const FIT_AERO: Fit = Fit { cell: 4.0, sigma: [20.0, 24.0, 28.0], band: [(1.0, 0.8), (0.4, 0.4)] };

/// How far above (and below) the fitted surface a vertex may be and still be
/// put on it, per raster kind (m).
fn reach(kind: u8) -> (f32, f32) {
    match kind {
        ROAD => (4.5, 3.0),
        APRON => (3.0, 3.0),
        RUNWAY => (5.0, 3.0),
        _ => (0.0, 0.0),
    }
}

/// Over this distance (m) in from the edge of a tile, the move onto the
/// fitted surface fades out to nothing (roads, airfield). Each tile fits its
/// own surface, so two tiles would put the vertices along their common edge
/// at different heights and open a crack between them (sky seen through the
/// street); left where Google put them, the two edges still meet.
const EDGE_FADE: [f32; 2] = [4.0, 16.0];

/// Convex hull of points (counter-clockwise, Andrew's monotone chain).
fn hull(pts: &[Vec2]) -> Vec<Vec2> {
    let mut p: Vec<Vec2> = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a - o).perp_dot(b - o);
    let mut h: Vec<Vec2> = Vec::with_capacity(2 * p.len());
    for pass in 0..2 {
        let start = h.len();
        let it: Box<dyn Iterator<Item = &Vec2>> = if pass == 0 { Box::new(p.iter()) } else { Box::new(p.iter().rev()) };
        for &q in it {
            while h.len() >= start + 2 && cross(h[h.len() - 2], h[h.len() - 1], q) <= 0.0 {
                h.pop();
            }
            h.push(q);
        }
        h.pop();
    }
    h
}

/// Distance from a point inside a convex polygon to its boundary.
fn edge_dist(h: &[Vec2], p: Vec2) -> f32 {
    let mut d = f32::MAX;
    for k in 0..h.len() {
        let (a, b) = (h[k], h[(k + 1) % h.len()]);
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
        d = d.min((a + ab * t - p).length());
    }
    d
}

/// Which fit a raster kind belongs to (0 road, 1 airfield).
fn group(kind: u8) -> Option<usize> {
    match kind {
        ROAD => Some(0),
        APRON | RUNWAY => Some(1),
        _ => None,
    }
}

/// Raster of the pavement around a tile, on its local east / north axes.
pub(crate) struct Raster {
    pub min: Vec2,
    pub cell: f32,
    pub w: usize,
    pub h: usize,
    pub k: Vec<u8>,
}
impl Raster {
    /// Empty raster over lo..hi (local metres), `margin` beyond on each side.
    pub fn new(lo: Vec2, hi: Vec2, cell: f32, margin: f32) -> Self {
        let w = ((hi.x - lo.x + 2.0 * margin) / cell).ceil() as usize + 1;
        let h = ((hi.y - lo.y + 2.0 * margin) / cell).ceil() as usize + 1;
        Raster { min: lo - Vec2::splat(margin), cell, w, h, k: vec![0; w * h] }
    }
    fn put(&mut self, i: usize, kind: u8) {
        if self.k[i] < kind {
            self.k[i] = kind;
        }
    }
    fn line(&mut self, pts: &[Vec2], half: f32, kind: u8) {
        let hw = half / self.cell;
        for s in pts.windows(2) {
            let (a, b) = ((s[0] - self.min) / self.cell, (s[1] - self.min) / self.cell);
            let x0 = (a.x.min(b.x) - hw).floor().max(0.0) as usize;
            let x1 = ((a.x.max(b.x) + hw).ceil().max(0.0) as usize).min(self.w);
            let y0 = (a.y.min(b.y) - hw).floor().max(0.0) as usize;
            let y1 = ((a.y.max(b.y) + hw).ceil().max(0.0) as usize).min(self.h);
            let d = b - a;
            let dd = d.length_squared().max(1e-6);
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - a;
                    let t = (p.dot(d) / dd).clamp(0.0, 1.0);
                    if (p - d * t).length_squared() < hw * hw {
                        self.put(y * self.w + x, kind);
                    }
                }
            }
        }
    }
    /// Fills a polygon (rings: outline and holes, even-odd).
    pub fn poly(&mut self, rings: &[Vec<Vec2>], kind: u8) {
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for p in rings.iter().flatten() {
            let q = (*p - self.min) / self.cell;
            (lo, hi) = (lo.min(q), hi.max(q));
        }
        let y0 = lo.y.floor().max(0.0) as usize;
        let y1 = (hi.y.ceil().max(0.0) as usize).min(self.h);
        let mut xs: Vec<f32> = Vec::new();
        for y in y0..y1 {
            let yc = y as f32 + 0.5;
            xs.clear();
            for r in rings {
                for i in 0..r.len() {
                    let (a, b) = ((r[i] - self.min) / self.cell, (r[(i + 1) % r.len()] - self.min) / self.cell);
                    if (a.y > yc) != (b.y > yc) {
                        xs.push(a.x + (yc - a.y) / (b.y - a.y) * (b.x - a.x));
                    }
                }
            }
            xs.sort_by(|a, b| a.total_cmp(b));
            for pair in xs.chunks_exact(2) {
                let x0 = (pair[0] - 0.5).ceil().max(0.0) as usize;
                let x1 = ((pair[1] - 0.5).floor() + 1.0).max(0.0) as usize;
                for x in x0..x1.min(self.w) {
                    self.put(y * self.w + x, kind);
                }
            }
        }
    }
    pub fn at(&self, x: i64, y: i64) -> u8 {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return 0;
        }
        self.k[y as usize * self.w + x as usize]
    }
    /// The kind under a point and how far inside it the point is (0 at its
    /// edge to 1 a couple of metres in), so the flattening fades out at the kerb.
    fn sample(&self, p: Vec2) -> (u8, f32) {
        let q = (p - self.min) / self.cell;
        let (cx, cy) = (q.x.floor() as i64, q.y.floor() as i64);
        let kind = self.at(cx, cy);
        if group(kind).is_none() {
            return (kind, 0.0);
        }
        let g = group(kind);
        let r = (2.0 / self.cell).ceil().max(1.0) as i64;
        let (mut same, mut all) = (0, 0);
        for dy in -r..=r {
            for dx in -r..=r {
                all += 1;
                same += (group(self.at(cx + dx, cy + dy)) == g) as i32;
            }
        }
        let f = same as f32 / all as f32;
        (kind, ((f - 0.3) / 0.5).clamp(0.0, 1.0))
    }
}

/// A smooth surface over part of the tile, from scattered heights.
struct Surface {
    min: Vec2,
    cell: f32,
    w: usize,
    h: usize,
    z: Vec<f32>,
    wt: Vec<f32>,
}
impl Surface {
    fn new(lo: Vec2, hi: Vec2, cell: f32) -> Self {
        let min = lo - Vec2::splat(cell);
        let w = (((hi.x - min.x) / cell) as usize + 2).max(1);
        let h = (((hi.y - min.y) / cell) as usize + 2).max(1);
        Self { min, cell, w, h, z: vec![0.0; w * h], wt: vec![0.0; w * h] }
    }
    fn index(&self, p: Vec2) -> usize {
        let q = (p - self.min) / self.cell;
        let x = (q.x.max(0.0) as usize).min(self.w - 1);
        let y = (q.y.max(0.0) as usize).min(self.h - 1);
        y * self.w + x
    }
    /// Bilinear between cell centres; None where nothing was fitted.
    fn get(&self, p: Vec2) -> Option<f32> {
        let q = (p - self.min) / self.cell - 0.5;
        let (x0, y0) = (q.x.floor(), q.y.floor());
        let (fx, fy) = (q.x - x0, q.y - y0);
        let (mut s, mut ws) = (0.0, 0.0);
        for (dx, dy, k) in [(0, 0, (1.0 - fx) * (1.0 - fy)), (1, 0, fx * (1.0 - fy)), (0, 1, (1.0 - fx) * fy), (1, 1, fx * fy)] {
            let (x, y) = (x0 as i64 + dx, y0 as i64 + dy);
            if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
                continue;
            }
            let i = y as usize * self.w + x as usize;
            if self.wt[i] > 1e-4 {
                s += self.z[i] * k;
                ws += k;
            }
        }
        (ws > 0.05).then(|| s / ws)
    }
    /// Gaussian blur of the weighted heights (normalised convolution: empty
    /// cells take their neighbours' height, and weigh in nothing).
    fn blur(&mut self, sigma: f32) {
        let s = sigma / self.cell;
        let r = (s * 2.5).ceil() as i64;
        let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * s * s)).exp()).collect();
        let (w, h) = (self.w as i64, self.h as i64);
        let mut zw: Vec<f32> = self.z.iter().zip(&self.wt).map(|(z, w)| z * w).collect();
        let mut wt = self.wt.clone();
        for pass in 0..2 {
            let (n, m) = if pass == 0 { (w, h) } else { (h, w) };
            let at = |a: i64, b: i64| if pass == 0 { (b * w + a) as usize } else { (a * w + b) as usize };
            let mut line_z = vec![0.0f32; n as usize];
            let mut line_w = vec![0.0f32; n as usize];
            for b in 0..m {
                for a in 0..n {
                    let (mut sz, mut sw) = (0.0, 0.0);
                    for (j, kk) in k.iter().enumerate() {
                        let t = a + j as i64 - r;
                        if t >= 0 && t < n {
                            let i = at(t, b);
                            sz += zw[i] * kk;
                            sw += wt[i] * kk;
                        }
                    }
                    line_z[a as usize] = sz;
                    line_w[a as usize] = sw;
                }
                for a in 0..n {
                    let i = at(a, b);
                    zw[i] = line_z[a as usize];
                    wt[i] = line_w[a as usize];
                }
            }
        }
        for i in 0..self.z.len() {
            self.wt[i] = wt[i];
            self.z[i] = if wt[i] > 1e-6 { zw[i] / wt[i] } else { 0.0 };
        }
    }
}

/// Fits a smooth surface through pavement points (x, y, height).
fn fit(pts: &[(Vec2, f32)], f: &Fit) -> Option<Surface> {
    if pts.len() < 8 {
        return None;
    }
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for (p, _) in pts {
        (lo, hi) = (lo.min(*p), hi.max(*p));
    }
    let mut s = Surface::new(lo, hi, f.cell);
    // First guess: the median of each cell (the mesh is sparse on flat
    // pavement, a vertex or two per cell, so a lower quantile would only
    // pick out the noise below the road).
    let mut cells: Vec<Vec<f32>> = vec![Vec::new(); s.w * s.h];
    for (p, z) in pts {
        cells[s.index(*p)].push(*z);
    }
    for (i, c) in cells.iter_mut().enumerate() {
        if !c.is_empty() {
            c.sort_by(|a, b| a.total_cmp(b));
            s.z[i] = c[c.len() / 2];
            s.wt[i] = 1.0;
        }
    }
    s.blur(f.sigma[0]);
    // Then the mean of what lies close to it, twice, in a narrowing band.
    for (pass, &(below, above)) in f.band.iter().enumerate() {
        let mut z = vec![0.0f32; s.w * s.h];
        let mut n = vec![0.0f32; s.w * s.h];
        for (p, h) in pts {
            let Some(g) = s.get(*p) else { continue };
            let d = h - g;
            if d > -below && d < above {
                let i = s.index(*p);
                z[i] += h;
                n[i] += 1.0;
            }
        }
        if n.iter().sum::<f32>() < 8.0 {
            break;
        }
        for i in 0..z.len() {
            s.wt[i] = n[i].min(4.0);
            s.z[i] = if n[i] > 0.0 { z[i] / n[i] } else { 0.0 };
        }
        s.blur(f.sigma[pass + 1]);
    }
    Some(s)
}

/// Calls `f` with each OpenStreetMap vector tile (zoom 14) under a 3D tile's
/// vertices (latitude, longitude in `lla`), and a function taking a point of
/// that vector tile to metres east and north of `origin` (on the ground).
/// False, having called nothing, if that would take more than nine of them.
pub(crate) fn vector_around(src: &Sources, origin: DVec3, lla: &[[f64; 3]], mut f: impl FnMut(&crate::sources::VecTile, &dyn Fn(&[f32; 2]) -> Vec2)) -> bool {
    let (e, n, u) = geo::enu(origin);
    let nt = (1u32 << Z) as f64;
    let (mut tx0, mut tx1, mut ty0, mut ty1) = (u32::MAX, 0, u32::MAX, 0);
    for l in lla.iter() {
        let tx = (geo::lon_to_merc_x(l[1]) * nt).floor().clamp(0.0, nt - 1.0) as u32;
        let ty = (geo::lat_to_merc_y(l[0]) * nt).floor().clamp(0.0, nt - 1.0) as u32;
        (tx0, tx1, ty0, ty1) = (tx0.min(tx), tx1.max(tx), ty0.min(ty), ty1.max(ty));
    }
    if tx0 > tx1 || (tx1 - tx0 + 1) * (ty1 - ty0 + 1) > 9 {
        return false;
    }
    let ground = u * geo::R;
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let Ok(vt) = src.vector(TileId::new(Z, tx, ty)) else { continue };
            let k = 1.0 / vt.extent as f64;
            let local = |p: &[f32; 2]| -> Vec2 {
                let lon = (tx as f64 + p[0] as f64 * k) / nt * std::f64::consts::TAU - std::f64::consts::PI;
                let lat = geo::merc_y_to_lat((ty as f64 + p[1] as f64 * k) / nt);
                let d = geo::lla_to_pos(lat, lon, 0.0) - ground;
                Vec2::new(d.dot(e) as f32, d.dot(n) as f32)
            };
            f(&vt, &local);
        }
    }
    true
}

/// Moves the vertices of a tile that lie on pavement onto a smooth surface.
/// `pos`: relative to `origin` (our frame); `lla`: their latitude, longitude
/// (radians) and height, kept in step. Returns how many were moved.
pub fn flatten(src: &Sources, origin: DVec3, pos: &mut [Vec3], lla: &mut [[f64; 3]]) -> usize {
    if !ON.load(Ordering::Relaxed) || pos.is_empty() {
        return 0;
    }
    let (e, n, u) = geo::enu(origin);
    let (e3, n3, u3) = (e.as_vec3(), n.as_vec3(), u.as_vec3());
    let xy: Vec<Vec2> = pos.iter().map(|p| Vec2::new(p.dot(e3), p.dot(n3))).collect();
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for p in &xy {
        (lo, hi) = (lo.min(*p), hi.max(*p));
    }
    let ext = (hi - lo).max_element();
    if ext > MAX_EXTENT {
        return 0;
    }

    let cell = (ext / 1500.0).max(1.0);
    let mut r = Raster::new(lo, hi, cell, 30.0);
    let mut any = false;
    let tiles = vector_around(src, origin, lla, |vt, local| {
        let urban = vt.buildings.len() > 150;
        for l in &vt.roads {
            let kind = match l.brunnel {
                2 => continue,
                1 => BRIDGE,
                _ if l.class <= RC_SERVICE => ROAD,
                _ => continue,
            };
            // A bridge's whole deck stays (rails included); a road a
            // little short of its kerbs.
            let half = if kind == BRIDGE { if l.class == RC_RAIL { 4.0 } else { crate::roads::carriageway(l, urban) * 0.5 + 3.0 } } else { crate::roads::carriageway(l, urban) * 0.5 - 0.3 };
            let pts: Vec<Vec2> = l.pts.iter().map(local).collect();
            r.line(&pts, half, kind);
            any = true;
        }
        for l in vt.aero_lines.iter().filter(|l| l.paved) {
            let pts: Vec<Vec2> = l.pts.iter().map(local).collect();
            r.line(&pts, l.width * 0.5, RUNWAY);
            any = true;
        }
        for (kind, rings) in &vt.aero_polys {
            let kind = match *kind {
                AP_RUNWAY | AP_TAXIWAY | AP_HELIPAD => RUNWAY,
                AP_APRON => APRON,
                _ => continue,
            };
            let rings: Vec<Vec<Vec2>> = rings.iter().map(|r| r.iter().map(local).collect()).collect();
            r.poly(&rings, kind);
            any = true;
        }
    });
    if !tiles || !any {
        return 0;
    }

    // Which vertices lie on pavement, and how far in.
    let mut on: Vec<(u32, u8, f32)> = Vec::new();
    let mut pts: [Vec<(Vec2, f32)>; 2] = [Vec::new(), Vec::new()];
    for (i, p) in xy.iter().enumerate() {
        let (kind, w) = r.sample(*p);
        let Some(g) = group(kind) else { continue };
        if w <= 0.0 {
            continue;
        }
        let z = pos[i].dot(u3);
        on.push((i as u32, kind, w));
        if w > 0.5 {
            pts[g].push((*p, z));
        }
    }
    let surf = [fit(&pts[0], &FIT_ROAD), fit(&pts[1], &FIT_AERO)];
    // The tile's edge: it is cut out of a convex cell, so seen from above
    // the vertices along the cut lie on its convex hull.
    let edge = hull(&xy);

    let mut moved = 0;
    for (i, kind, w) in on {
        let i = i as usize;
        let Some(s) = group(kind).and_then(|g| surf[g].as_ref()).and_then(|s| s.get(xy[i])) else { continue };
        let z = pos[i].dot(u3);
        let d = z - s;
        let (up, down) = reach(kind);
        if d > up || d < -down {
            continue;
        }
        // (fading out towards the ends of its reach, so what stands on the
        // pavement is not cut off with a step)
        let fade = EDGE_FADE[group(kind).unwrap()];
        let e = (edge_dist(&edge, xy[i]) / fade).clamp(0.0, 1.0);
        let k = w * ((up - d) / (up * 0.25)).clamp(0.0, 1.0) * ((d + down) / (down * 0.25)).clamp(0.0, 1.0) * e * e * (3.0 - 2.0 * e);
        let dz = (s - z) * k;
        pos[i] += u3 * dz;
        lla[i][2] += dz as f64;
        moved += 1;
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_ignores_cars() {
        // A 2 m-wide street, 120 m long, rising 1 in 20, with a "car" on it and noise.
        let mut pts = Vec::new();
        let mut seed = 1u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
        };
        for i in 0..1200 {
            let x = i as f32 * 0.1;
            for y in [-4.0, -2.0, 0.0, 2.0, 4.0] {
                let car = (40.0..45.0).contains(&x) && y > -1.0 && y < 3.0;
                let z = x * 0.05 + rnd() * 0.3 + if car { 1.5 } else { 0.0 };
                pts.push((Vec2::new(x, y), z));
            }
        }
        let s = fit(&pts, &FIT_ROAD).unwrap();
        for x in [10.0f32, 42.0, 60.0, 100.0] {
            let z = s.get(Vec2::new(x, 1.0)).unwrap();
            assert!((z - x * 0.05).abs() < 0.12, "at {x}: {z} vs {}", x * 0.05);
        }
    }

    #[test]
    fn tile_edge_distance() {
        let mut pts = vec![Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), Vec2::new(100.0, 50.0), Vec2::new(0.0, 50.0)];
        pts.extend((0..200).map(|i| Vec2::new((i * 37 % 100) as f32, (i * 13 % 50) as f32)));
        let h = hull(&pts);
        assert_eq!(h.len(), 4);
        assert!((edge_dist(&h, Vec2::new(50.0, 25.0)) - 25.0).abs() < 1e-4);
        assert!((edge_dist(&h, Vec2::new(3.0, 20.0)) - 3.0).abs() < 1e-4);
        assert!(edge_dist(&h, Vec2::new(100.0, 10.0)) < 1e-4);
    }

    #[test]
    fn raster_fills_polygons_and_lines() {
        let mut r = Raster { min: Vec2::ZERO, cell: 1.0, w: 50, h: 50, k: vec![0; 2500] };
        let sq = vec![Vec2::new(10.0, 10.0), Vec2::new(30.0, 10.0), Vec2::new(30.0, 30.0), Vec2::new(10.0, 30.0)];
        r.poly(&[sq], APRON);
        r.line(&[Vec2::new(0.0, 40.0), Vec2::new(50.0, 40.0)], 3.0, ROAD);
        assert_eq!(r.at(20, 20), APRON);
        assert_eq!(r.at(5, 20), 0);
        assert_eq!(r.at(25, 40), ROAD);
        assert_eq!(r.at(25, 44), 0);
        assert_eq!(r.sample(Vec2::new(20.0, 20.0)), (APRON, 1.0));
    }
}
