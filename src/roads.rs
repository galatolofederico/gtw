//! Roads from OpenStreetMap: ribbons laid over the terrain, bridge decks with
//! real height, and the lane graph the traffic drives on.
use crate::buildings::{hash, Ctx, Footprint, Hoods, Mesh, Parked, K_DECK, K_LAMP, K_PLAIN, RASTER};
use crate::geo::TileId;
use crate::render::RoadVertex;
use crate::sources::*;
use glam::{Vec2, Vec3};
use std::collections::HashMap;

pub struct Path {
    /// centreline, relative to the tile centre, on the road surface
    pub pts: Vec<Vec3>,
    pub cum: Vec<f32>,
    pub len: f32,
    pub class: u8,
    pub oneway: bool,
    /// lanes per direction (all of them if one-way)
    pub lanes: u8,
    pub lane_w: f32,
    /// distance from the centreline to the middle of the parking lane, 0 if none
    pub park_off: f32,
    pub speed: f32,
    /// in a tunnel: traffic passes through unseen
    pub hidden: bool,
    /// carriageway half-width, kerb to kerb (metres)
    pub drive_half: f32,
    /// the road has a sidewalk band
    pub walk: bool,
    /// the path is a bridge deck
    #[allow(dead_code)]
    pub bridge: bool,
    /// centreline in tile units (y down), for the road index
    pub local: Vec<[f32; 2]>,
    /// absolute surface altitude at each centreline point
    #[allow(dead_code)]
    pub alt: Vec<f32>,
    pub nodes: [u64; 2],
    /// which tile edge each end sits on (0 none, 1 west, 2 east, 3 north, 4 south)
    pub edge: [u8; 2],
    /// side roads joining along the way: (distance along, node)
    pub junctions: Vec<(f32, u64)>,
    /// crossings and stop lines nobody parks on: (distance along, radius)
    pub keep_clear: Vec<(f32, f32)>,
}
impl Path {
    /// Position and direction of travel at a distance along the path.
    pub fn sample(&self, s: f32) -> (Vec3, Vec3) {
        let s = s.clamp(0.0, self.len);
        let i = match self.cum.binary_search_by(|c| c.total_cmp(&s)) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
        .min(self.pts.len() - 2);
        let (a, b) = (self.pts[i], self.pts[i + 1]);
        let l = (self.cum[i + 1] - self.cum[i]).max(1e-4);
        (a + (b - a) * ((s - self.cum[i]) / l), (b - a) / l)
    }
}

/// Sidewalk band width outside the carriageway (metres), as in `cfg`.
pub const SIDEWALK_W: f32 = 1.9;

/// What the road index found under a point.
#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
pub struct RoadHit {
    #[allow(dead_code)]
    pub path: u32,
    /// distance along the path (metres)
    pub s: f32,
    /// distance from the centreline (metres)
    pub dist: f32,
    /// absolute surface altitude
    pub h: f64,
    pub kind: crate::surface::Surface,
    pub sidewalk: bool,
    pub bridge: bool,
    /// a tunnel, which v1 keeps at grade and does not make enterable
    pub hidden: bool,
}

/// A 64 x 64 tile-unit grid of candidate path ids, built from a `RoadNet` so a
/// point query only has to test the roads near it. Distances are measured from
/// the centreline; a point inside `drive_half` is carriageway asphalt, one in
/// the walk band beyond it is concrete sidewalk.
#[allow(dead_code)]
pub struct RoadIndex {
    ext: f32,
    unit_m: f32,
    cell: f32,
    cells: Vec<Vec<u32>>,
}

#[allow(dead_code)]
impl RoadIndex {
    const N: usize = 64;

    pub fn build(net: &RoadNet, ext: f32, unit_m: f32) -> Self {
        let unit_m = unit_m.max(1e-6);
        let cell = (ext / Self::N as f32).max(1e-6);
        let mut cells = vec![Vec::new(); Self::N * Self::N];
        for (i, p) in net.paths.iter().enumerate() {
            if p.local.len() < 2 {
                continue;
            }
            let hw = (p.drive_half + if p.walk { SIDEWALK_W } else { 0.0 }) / unit_m;
            // A cell is a candidate when its centre is within reach of the
            // ribbon: any point in a cell is at most half a diagonal from it.
            let reach = hw + cell * 0.75;
            for w in p.local.windows(2) {
                let (a, b) = (Vec2::from(w[0]), Vec2::from(w[1]));
                let x0 = (((a.x.min(b.x) - reach) / cell).floor() as i64).clamp(0, Self::N as i64 - 1) as usize;
                let x1 = (((a.x.max(b.x) + reach) / cell).ceil() as i64).clamp(0, Self::N as i64) as usize;
                let y0 = (((a.y.min(b.y) - reach) / cell).floor() as i64).clamp(0, Self::N as i64 - 1) as usize;
                let y1 = (((a.y.max(b.y) + reach) / cell).ceil() as i64).clamp(0, Self::N as i64) as usize;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let c = Vec2::new((x as f32 + 0.5) * cell, (y as f32 + 0.5) * cell);
                        if seg_dist(c, a, b) <= reach {
                            cells[y * Self::N + x].push(i as u32);
                        }
                    }
                }
            }
        }
        for c in &mut cells {
            c.sort_unstable();
            c.dedup();
        }
        Self { ext, unit_m, cell, cells }
    }

    pub fn ext(&self) -> f32 {
        self.ext
    }

    pub fn query(&self, net: &RoadNet, p: [f32; 2]) -> Option<RoadHit> {
        let q = Vec2::from(p);
        let cx = ((p[0] / self.cell).floor().max(0.0) as usize).min(Self::N - 1);
        let cy = ((p[1] / self.cell).floor().max(0.0) as usize).min(Self::N - 1);
        let mut best: Option<RoadHit> = None;
        for &pi in &self.cells[cy * Self::N + cx] {
            let path = &net.paths[pi as usize];
            let n = path.local.len().min(path.alt.len());
            if n < 2 {
                continue;
            }
            let (mut d2, mut t, mut seg) = (f32::INFINITY, 0.0f32, 0usize);
            for (k, w) in path.local[..n].windows(2).enumerate() {
                let (a, b) = (Vec2::from(w[0]), Vec2::from(w[1]));
                let d = b - a;
                let dd = d.length_squared().max(1e-9);
                let u = ((q - a).dot(d) / dd).clamp(0.0, 1.0);
                let dist = (a + d * u - q).length_squared();
                if dist < d2 {
                    (d2, t, seg) = (dist, u, k);
                }
            }
            let dist = d2.sqrt() * self.unit_m;
            let max = path.drive_half + if path.walk { SIDEWALK_W } else { 0.0 };
            if dist > max {
                continue;
            }
            if best.is_some_and(|b| dist >= b.dist) {
                continue;
            }
            let (s0, s1) = (path.cum.get(seg).copied().unwrap_or(0.0), path.cum.get(seg + 1).copied().unwrap_or(path.len));
            let h = path.alt[seg] as f64 * (1.0 - t as f64) + path.alt[seg + 1] as f64 * t as f64;
            let sidewalk = dist > path.drive_half;
            best = Some(RoadHit {
                path: pi,
                s: s0 + (s1 - s0) * t,
                dist,
                h,
                kind: if sidewalk { crate::surface::Surface::Concrete } else { crate::surface::Surface::Asphalt },
                sidewalk,
                bridge: path.bridge,
                hidden: path.hidden,
            });
        }
        best
    }

    /// Candidate paths in a grid cell (test aid: the grid is not drawn).
    #[cfg(test)]
    pub fn cell_ids(&self, x: usize, y: usize) -> &[u32] {
        &self.cells[y.min(Self::N - 1) * Self::N + x.min(Self::N - 1)]
    }
}

/// Where a path can be entered: at an end, or part-way along (a side street meeting it).
#[derive(Copy, Clone)]
pub struct Link {
    pub path: u32,
    pub at: f32,
    /// 0 start, 1 end, 2 somewhere along
    pub end: u8,
}

#[derive(Default)]
pub struct RoadNet {
    pub paths: Vec<Path>,
    pub nodes: HashMap<u64, Vec<Link>>,
    /// traffic keeps left here
    pub left: bool,
    pub total_len: f32,
}

pub struct Roads {
    pub verts: Vec<RoadVertex>,
    pub idx: Vec<u32>,
    pub net: RoadNet,
    /// RASTER x RASTER, non-zero where road surface is
    pub mask: Vec<u8>,
    pub radius: f32,
}

struct Cfg {
    /// kerb to kerb, metres
    drive: f32,
    lanes: u8,
    walk: bool,
    parking: bool,
    speed: f32,
    cars: bool,
}

fn cfg(l: &RoadLine, urban: bool) -> Cfg {
    let c = |drive: f32, lanes: u8, speed: f32| Cfg { drive, lanes, walk: false, parking: false, speed, cars: true };
    let plain = l.brunnel == 0;
    match l.class {
        RC_MOTORWAY | RC_TRUNK if l.ramp => c(5.5, 1, 16.0),
        RC_MOTORWAY => if l.oneway { c(13.0, 3, 29.0) } else { c(24.0, 6, 29.0) },
        RC_TRUNK => if l.oneway { c(8.6, 2, 22.0) } else { c(15.0, 4, 20.0) },
        RC_PRIMARY => Cfg { walk: urban && plain, ..if l.oneway { c(10.2, 3, 15.0) } else { c(13.6, 4, 15.0) } },
        RC_SECONDARY => Cfg { walk: urban && plain, parking: urban && plain, ..c(7.0 + if urban { 4.4 } else { 0.0 }, 2, 13.0) },
        RC_TERTIARY => Cfg { walk: urban && plain, parking: urban && plain, ..if l.oneway { c(4.0 + if urban { 4.4 } else { 0.0 }, 1, 11.0) } else { c(6.6 + if urban { 4.4 } else { 0.0 }, 2, 11.0) } },
        RC_MINOR => Cfg { walk: urban && plain, parking: urban && plain, ..if l.oneway { c(3.6 + if urban { 4.4 } else { 0.0 }, 1, 8.0) } else { c(5.6 + if urban { 4.4 } else { 0.0 }, 2, 8.5) } },
        RC_SERVICE => Cfg { cars: l.sub == 0, ..c(if l.sub == 1 { 2.8 } else { 3.6 }, 1, 4.0) },
        RC_TRACK => Cfg { cars: false, ..c(2.8, 0, 0.0) },
        RC_PATH => Cfg { cars: false, ..c(if l.sub == 1 { 4.0 } else { 1.7 }, 0, 0.0) },
        _ => Cfg { cars: false, ..c(3.2, 0, 0.0) },
    }
}

/// Width (m) of a road's carriageway, kerb to kerb (parking lanes included).
/// A tile with more than 150 buildings counts as town, as in `build`.
pub fn carriageway(l: &RoadLine, urban: bool) -> f32 {
    cfg(l, urban).drive
}

/// Countries that drive on the left, as rough boxes.
fn drives_left(lat: f64, lon: f64) -> bool {
    let b = |la0: f64, la1: f64, lo0: f64, lo1: f64| lat >= la0 && lat <= la1 && lon >= lo0 && lon <= lo1;
    b(49.8, 61.0, -11.0, 2.0) || b(30.0, 46.0, 129.0, 146.0) || b(-48.0, -9.0, 110.0, 180.0) || b(6.0, 36.0, 68.0, 92.0) && lat < 30.0 || b(-35.0, -8.0, 11.0, 41.0) || b(-11.0, 20.0, 95.0, 120.0) && !(lat > 8.0 && lon > 102.0 && lon < 110.0) || b(-5.0, 5.0, 33.0, 42.0)
}

/// Clip a polyline to the tile square; returns the pieces that lie inside.
pub fn clip(pts: &[[f32; 2]], ext: f32) -> Vec<Vec<[f32; 2]>> {
    let mut out: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut cur: Vec<[f32; 2]> = Vec::new();
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = [b[0] - a[0], b[1] - a[1]];
        let (mut t0, mut t1) = (0f32, 1f32);
        let mut ok = true;
        for (p, q) in [(-d[0], a[0]), (d[0], ext - a[0]), (-d[1], a[1]), (d[1], ext - a[1])] {
            if p == 0.0 {
                if q < 0.0 {
                    ok = false;
                }
            } else {
                let r = q / p;
                if p < 0.0 { t0 = t0.max(r) } else { t1 = t1.min(r) }
            }
        }
        if !ok || t0 >= t1 {
            if cur.len() > 1 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
            continue;
        }
        let (p0, p1) = ([a[0] + d[0] * t0, a[1] + d[1] * t0], [a[0] + d[0] * t1, a[1] + d[1] * t1]);
        if cur.is_empty() {
            cur.push(p0);
        }
        cur.push(p1);
        if t1 < 1.0 {
            out.push(std::mem::take(&mut cur));
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

fn edge_of(p: [f32; 2], ext: f32) -> u8 {
    if p[0] <= 0.01 {
        1
    } else if p[0] >= ext - 0.01 {
        2
    } else if p[1] <= 0.01 {
        3
    } else if p[1] >= ext - 0.01 {
        4
    } else {
        0
    }
}

/// Identity of a road node that is the same from both sides of a tile border.
pub fn node_key(id: TileId, p: [f32; 2], ext: f32) -> u64 {
    let g = |t: u32, v: f32| (t as f64 * 4096.0 + (v / ext) as f64 * 4096.0).round() as u64;
    (g(id.x, p[0]) << 32) | g(id.y, p[1])
}
/// The same node, searched with a little slack (clipping rounds differently per tile).
pub fn near_keys(k: u64) -> [u64; 9] {
    let (x, y) = ((k >> 32) as i64, (k & 0xffff_ffff) as i64);
    let mut out = [0u64; 9];
    let mut n = 0;
    for dy in -1..=1 {
        for dx in -1..=1 {
            out[n] = (((x + dx) as u64) << 32) | ((y + dy) as u64 & 0xffff_ffff);
            n += 1;
        }
    }
    out
}

fn bridge_pieces(vt: &VecTile) -> Vec<Vec<[f32; 2]>> {
    vt.roads.iter().filter(|r| r.brunnel == 1 && r.class <= RC_RAIL && r.class != RC_PATH).flat_map(|r| clip(&r.pts, vt.extent)).collect()
}

fn point_in_ring(p: Vec2, ring: &[[f32; 2]]) -> bool {
    let mut inside = false;
    for w in ring.iter().zip(ring.iter().cycle().skip(1)).take(ring.len()) {
        let (a, b) = (Vec2::from(*w.0), Vec2::from(*w.1));
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

/// Water and a short margin on either bank. Extending the bridge to dry ground
/// prevents a visible road-to-deck gap at the waterline.
fn over_water(rings: &[(&Vec<[f32; 2]>, [f32; 4])], p: Vec2, bank: f32) -> bool {
    rings.iter().any(|(ring, bb)| {
        if ring.len() < 3 || p.x < bb[0] - bank || p.y < bb[1] - bank || p.x > bb[2] + bank || p.y > bb[3] + bank { return false; }
        point_in_ring(p, ring) || ring.iter().zip(ring.iter().cycle().skip(1)).take(ring.len()).any(|(a, b)| seg_dist(p, Vec2::from(*a), Vec2::from(*b)) <= bank)
    })
}

/// Split a clipped road into water-crossing spans, sampled finely enough that
/// shore transitions remain short and the deck can meet the approach cleanly.
fn water_spans(piece: Vec<[f32; 2]>, vt: &VecTile, um: f32) -> Vec<(Vec<[f32; 2]>, bool)> {
    if piece.len() < 2 || vt.water.is_empty() { return vec![(piece, false)]; }
    let bank = 8.0 / um;
    let mut waters = Vec::new();
    for (class, rings) in &vt.water {
        if *class == 1 { continue; } // coastline roads stay on shore
        for ring in rings {
            if ring.len() < 3 { continue; }
            let bb = ring.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]);
            waters.push((ring, bb));
        }
    }
    let road_bb = piece.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])]);
    if !waters.iter().any(|(_, b)| b[0] <= road_bb[2] + bank && b[2] >= road_bb[0] - bank && b[1] <= road_bb[3] + bank && b[3] >= road_bb[1] - bank) {
        return vec![(piece, false)];
    }
    let mut out = Vec::new();
    let mut run = vec![piece[0]];
    let mut run_bridge = None;
    for w in piece.windows(2) {
        let a = Vec2::from(w[0]);
        let b = Vec2::from(w[1]);
        let length = (b - a).length();
        let steps = ((length * um / 7.0).ceil() as usize).max(1);
        for j in 0..steps {
            let t0 = j as f32 / steps as f32;
            let t1 = (j + 1) as f32 / steps as f32;
            let p0 = a.lerp(b, t0);
            let p1 = a.lerp(b, t1);
            let bridge = over_water(&waters, a.lerp(b, (t0 + t1) * 0.5), bank);
            if let Some(old) = run_bridge {
                if old != bridge {
                    if run.len() > 1 { out.push((std::mem::replace(&mut run, vec![p0.into()]), old)); }
                    run_bridge = Some(bridge);
                }
            } else {
                run_bridge = Some(bridge);
            }
            if run.last().is_none_or(|p| (Vec2::from(*p) - p1).length_squared() > 1e-8) { run.push(p1.into()); }
        }
    }
    if run.len() > 1 { out.push((run, run_bridge.unwrap_or(false))); }
    if out.is_empty() { vec![(piece, false)] } else { out }
}

/// Remove terrain sampling noise and cap grades on ordinary roads. Both passes
/// are constrained by distance so sparse map vertices cannot create ramps.
fn grade_profile(ground: &[f32], cum: &[f32], max_grade: f32) -> Vec<f32> {
    let mut z = ground.to_vec();
    if z.len() < 3 { return z; }
    let mut smooth = z.clone();
    for i in 1..z.len() - 1 {
        smooth[i] = (z[i - 1] + 2.0 * z[i] + z[i + 1]) * 0.25;
    }
    z = smooth;
    for i in 1..z.len() {
        let rise = max_grade * (cum[i] - cum[i - 1]);
        z[i] = z[i].clamp(z[i - 1] - rise, z[i - 1] + rise);
    }
    for i in (0..z.len() - 1).rev() {
        let rise = max_grade * (cum[i + 1] - cum[i]);
        z[i] = z[i].clamp(z[i + 1] - rise, z[i + 1] + rise);
    }
    z
}

/// Follow a bridge onward from one of its ends - through other bridge pieces
/// and across tile borders - to where it lands. Returns the extra length
/// walked and the ground height at the landing.
fn bridge_end(src: &crate::sources::Sources, cx: &Ctx, own: &[Vec<[f32; 2]>], start: [f32; 2], skip: usize) -> (f32, f32) {
    let ext = cx.ext;
    let mut tile = cx.id;
    let mut vt: Option<std::sync::Arc<VecTile>> = None;
    let mut pieces: Vec<Vec<[f32; 2]>> = own.to_vec();
    let (mut p, mut skip, mut extra, mut hopped) = (start, Some(skip), 0.0, false);
    for _ in 0..24 {
        let e = edge_of(p, ext);
        if e != 0 && !hopped {
            let n = 1i64 << tile.z;
            let (dx, dy, q) = match e {
                1 => (-1, 0, [p[0] + ext, p[1]]),
                2 => (1, 0, [p[0] - ext, p[1]]),
                3 => (0, -1, [p[0], p[1] + ext]),
                _ => (0, 1, [p[0], p[1] - ext]),
            };
            let (x, y) = ((tile.x as i64 + dx).rem_euclid(n), tile.y as i64 + dy);
            if y < 0 || y >= n {
                break;
            }
            tile = TileId::new(tile.z, x as u32, y as u32);
            let Ok(t) = src.vector(tile) else { break };
            pieces = bridge_pieces(&t);
            vt = Some(t);
            p = q;
            skip = None;
            hopped = true;
            continue;
        }
        let near = |a: [f32; 2]| (a[0] - p[0]).abs() < 3.0 && (a[1] - p[1]).abs() < 3.0;
        let next = pieces.iter().enumerate().find_map(|(k, pc)| {
            if Some(k) == skip {
                None
            } else if near(pc[0]) {
                Some((k, false))
            } else if near(pc[pc.len() - 1]) {
                Some((k, true))
            } else {
                None
            }
        });
        let Some((k, rev)) = next else { break };
        extra += pieces[k].windows(2).map(|w| ((w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])) * cx.unit_m).sum::<f32>();
        p = if rev { pieces[k][0] } else { pieces[k][pieces[k].len() - 1] };
        skip = Some(k);
        hopped = false;
    }
    let _ = vt;
    let g = if tile == cx.id { cx.ground(p) } else { src.elev(tile).map(|e| e.sample((p[0] / ext) as f64, (p[1] / ext) as f64)).unwrap_or(0.0) };
    (extra, g.max(0.0))
}

/// What meets at a road node: legs, the biggest road there, the widest half width.
#[derive(Copy, Clone)]
struct NodeInfo {
    legs: u8,
    best: u8,
    worst: u8,
    half: f32,
}
impl NodeInfo {
    /// Busy enough to have traffic lights.
    fn signalled(&self, urban: bool) -> bool {
        urban && (self.best <= RC_SECONDARY && self.worst <= RC_TERTIARY || self.best <= RC_PRIMARY && self.worst <= RC_MINOR)
    }
    /// Does a road of this class give way here? The bigger road runs straight through otherwise.
    fn stops(&self, class: u8, urban: bool) -> bool {
        self.legs >= 3 && (class >= self.worst || self.signalled(urban))
    }
}

/// Where roads really meet, found from their geometry: the map tiles drop the
/// shared vertex where one straight street crosses another. For each road, the
/// junctions along it (tile units) and what meets there.
fn junctions(vt: &VecTile, urban: bool, um: f32) -> Vec<Vec<([f32; 2], NodeInfo)>> {
    let ext = vt.extent;
    let tol = 0.7 / um;
    let half = |l: &RoadLine| {
        let cf = cfg(l, urban);
        cf.drive * 0.5 + if cf.walk { 1.9 } else { 0.0 }
    };
    let idx: Vec<usize> = (0..vt.roads.len()).filter(|i| {
        let l = &vt.roads[*i];
        l.class <= RC_MINOR && !l.ramp && l.brunnel == 0 && l.pts.len() >= 2
    }).collect();
    let bb: Vec<[f32; 4]> = idx.iter().map(|i| vt.roads[*i].pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])).collect();
    let at_end = |l: &RoadLine, p: Vec2| (Vec2::from(l.pts[0]) - p).length() < tol * 3.0 || (Vec2::from(l.pts[l.pts.len() - 1]) - p).length() < tol * 3.0;
    // (point, other road, legs the other road brings, its class, its half width)
    let mut hits: Vec<Vec<(Vec2, usize, u8, u8, f32)>> = vec![Vec::new(); vt.roads.len()];
    for x in 0..idx.len() {
        for y in x + 1..idx.len() {
            let (ba, bq) = (bb[x], bb[y]);
            if ba[0] > bq[2] + tol || bq[0] > ba[2] + tol || ba[1] > bq[3] + tol || bq[1] > ba[3] + tol {
                continue;
            }
            let (la, lb) = (&vt.roads[idx[x]], &vt.roads[idx[y]]);
            // Roads on different map layers cross in projection but do not
            // form an at-grade junction.
            if la.layer != lb.layer { continue; }
            for wa in la.pts.windows(2) {
                let (a, r) = (Vec2::from(wa[0]), Vec2::from(wa[1]) - Vec2::from(wa[0]));
                if a.x.min(a.x + r.x) > bq[2] + tol || a.x.max(a.x + r.x) < bq[0] - tol || a.y.min(a.y + r.y) > bq[3] + tol || a.y.max(a.y + r.y) < bq[1] - tol {
                    continue;
                }
                for wb in lb.pts.windows(2) {
                    let (c, q) = (Vec2::from(wb[0]), Vec2::from(wb[1]) - Vec2::from(wb[0]));
                    let (lr, lq) = (r.length(), q.length());
                    let den = r.perp_dot(q);
                    if lr < 1e-4 || lq < 1e-4 || den.abs() < 0.05 * lr * lq {
                        continue;
                    }
                    let (t, u) = ((c - a).perp_dot(q) / den, (c - a).perp_dot(r) / den);
                    if t < -tol / lr || t > 1.0 + tol / lr || u < -tol / lq || u > 1.0 + tol / lq {
                        continue;
                    }
                    let p = a + r * t.clamp(0.0, 1.0);
                    if p.x < 0.0 || p.y < 0.0 || p.x > ext || p.y > ext {
                        continue;
                    }
                    hits[idx[x]].push((p, idx[y], if at_end(lb, p) { 1 } else { 2 }, lb.class, half(lb)));
                    hits[idx[y]].push((p, idx[x], if at_end(la, p) { 1 } else { 2 }, la.class, half(la)));
                }
            }
        }
    }
    hits.iter().enumerate().map(|(li, h)| {
        let l = &vt.roads[li];
        let mut out: Vec<([f32; 2], NodeInfo)> = Vec::new();
        let mut used = vec![false; h.len()];
        for i in 0..h.len() {
            if used[i] {
                continue;
            }
            // Everything that meets this road within a few metres of here is one junction.
            let p = h[i].0;
            let mut j = NodeInfo { legs: if at_end(l, p) { 1 } else { 2 }, best: l.class, worst: l.class, half: half(l) };
            let mut seen: Vec<usize> = Vec::new();
            for k in i..h.len() {
                if !used[k] && (h[k].0 - p).length() < 4.0 / um {
                    used[k] = true;
                    if !seen.contains(&h[k].1) {
                        seen.push(h[k].1);
                        j.legs = j.legs.saturating_add(h[k].2);
                        j.best = j.best.min(h[k].3);
                        j.worst = j.worst.max(h[k].3);
                        j.half = j.half.max(h[k].4);
                    }
                }
            }
            out.push((p.into(), j));
        }
        out
    }).collect()
}

/// Spacing, mounting height, pole inset from the ribbon edge and arm length of
/// the street lights on a road. Must match `lamp_pool` in the shaders.
fn lamp_spec(class: u8) -> (f32, f32, f32, f32) {
    if class <= RC_TRUNK { (48.0, 11.0, 0.4, 2.5) } else { (36.0, 8.0, 1.5, 1.8) }
}

const POLE: [u8; 4] = [96, 100, 102, K_PLAIN];
const POLE_STY: [u8; 4] = [70, 0, 150, 0];

/// Street light: pole, arm reaching `toward` the carriageway, lamp head.
fn lamp_post(cx: &Ctx, mesh: &mut Mesh, m: Vec2, toward: Vec2, tan: Vec2, z: f32, h: f32, arm: f32) {
    mesh.boxm(cx, true, m, toward * 0.08, tan * 0.08, z - 0.4, z + h, POLE, POLE_STY, None);
    mesh.boxm(cx, true, m + toward * (arm * 0.5), toward * (arm * 0.5), tan * 0.05, z + h - 0.1, z + h, POLE, POLE_STY, None);
    mesh.boxm(cx, true, m + toward * arm, toward * 0.36, tan * 0.15, z + h - 0.2, z + h + 0.02, [255, 196, 120, K_LAMP], [0, 0, 0, 0], Some([70, 72, 74, K_PLAIN]));
}

/// Traffic signal on the kerb, with a mast arm over the wider roads.
fn signal(cx: &Ctx, mesh: &mut Mesh, m: Vec2, toward: Vec2, tan: Vec2, z: f32, mast: f32, seed: u8) {
    let dark = [28, 30, 30, K_PLAIN];
    let head = |mesh: &mut Mesh, c: Vec2, z0: f32| mesh.boxm(cx, true, c, toward * 0.18, tan * 0.18, z0, z0 + 1.05, [20, 20, 20, K_LAMP], [seed, 1, 0, 0], Some(dark));
    if mast > 0.0 {
        mesh.boxm(cx, true, m, toward * 0.1, tan * 0.1, z - 0.4, z + 6.3, POLE, POLE_STY, None);
        mesh.boxm(cx, true, m + toward * (mast * 0.5), toward * (mast * 0.5), tan * 0.07, z + 6.1, z + 6.3, POLE, POLE_STY, None);
        head(mesh, m + toward * mast, z + 5.05);
        head(mesh, m + toward * 0.3, z + 2.6);
    } else {
        mesh.boxm(cx, true, m, toward * 0.07, tan * 0.07, z - 0.4, z + 3.2, POLE, POLE_STY, None);
        head(mesh, m, z + 3.2);
    }
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared().max(1e-9)).clamp(0.0, 1.0);
    (a + d * t - p).length()
}

/// Is `p` within `r` of the polyline (all in tile units)?
fn near_line(pts: &[[f32; 2]], p: Vec2, r: f32) -> bool {
    pts.windows(2).any(|w| seg_dist(p, Vec2::from(w[0]), Vec2::from(w[1])) < r)
}

/// Roads that only repeat another road: the map often carries the same street
/// twice (two overlapping ways, a service road traced along a main road, both
/// halves of a road drawn on one line). Each would be laid as its own ribbon on
/// top of the other, doubling markings and fighting over the surface. A ground
/// road whose centreline lies inside a more important (or earlier, if equal)
/// road's carriageway for most of its length is dropped. Crossings and side
/// streets are not affected: only a few of their samples fall inside the other.
fn redundant(vt: &VecTile, urban: bool, um: f32) -> Vec<bool> {
    let n = vt.roads.len();
    let mut skip = vec![false; n];
    let ground = |l: &RoadLine| l.brunnel == 0 && l.layer == 0 && l.class <= RC_SERVICE && l.pts.len() >= 2;
    // half width of each carriageway, in tile units
    let hw: Vec<f32> = vt.roads.iter().map(|l| cfg(l, urban).drive * 0.5 / um).collect();
    let bb: Vec<[f32; 4]> = vt.roads.iter().map(|l| l.pts.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])])).collect();
    let step = 3.0 / um;
    for i in 0..n {
        let li = &vt.roads[i];
        if !ground(li) {
            continue;
        }
        let rivals: Vec<usize> = (0..n)
            .filter(|&j| {
                let lj = &vt.roads[j];
                j != i
                    && ground(lj)
                    && lj.layer == li.layer
                    && (lj.class < li.class || (lj.class == li.class && j < i))
                    && !(bb[i][0] > bb[j][2] + hw[j] || bb[j][0] - hw[j] > bb[i][2] || bb[i][1] > bb[j][3] + hw[j] || bb[j][1] - hw[j] > bb[i][3])
            })
            .collect();
        if rivals.is_empty() {
            continue;
        }
        let (mut total, mut covered) = (0u32, 0u32);
        for w in li.pts.windows(2) {
            let (a, b) = (Vec2::from(w[0]), Vec2::from(w[1]));
            let m = (((b - a).length() / step).ceil().max(1.0)) as u32;
            for s in 0..m {
                let p = a + (b - a) * ((s as f32 + 0.5) / m as f32);
                total += 1;
                if rivals.iter().any(|&j| near_line(&vt.roads[j].pts, p, hw[j] * 0.6)) {
                    covered += 1;
                }
            }
        }
        if total >= 3 && covered as f32 >= 0.8 * total as f32 {
            skip[i] = true;
        }
    }
    skip
}

/// Stamp the pixels within `hw` pixels of the segment a-b into a RASTER x RASTER mask.
fn stamp_segment(mask: &mut [u8], a: Vec2, b: Vec2, hw: f32) {
    let r = RASTER as i64;
    let (x0, x1) = ((a.x.min(b.x) - hw).floor().max(0.0) as i64, ((a.x.max(b.x) + hw).ceil() as i64).min(r));
    let (y0, y1) = ((a.y.min(b.y) - hw).floor().max(0.0) as i64, ((a.y.max(b.y) + hw).ceil() as i64).min(r));
    let d = b - a;
    let dd = d.length_squared().max(1e-6);
    for y in y0..y1 {
        for x in x0..x1 {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - a;
            let t = (p.dot(d) / dd).clamp(0.0, 1.0);
            if (p - d * t).length_squared() < hw * hw {
                mask[y as usize * RASTER + x as usize] = 1;
            }
        }
    }
}

/// RASTER x RASTER mask, non-zero on the carriageway of every road that lies on
/// the ground (no bridges, tunnels, paths, tracks, rail or alleys), a little
/// narrower than the drawn ribbon. Used to keep buildings off the street: it
/// has to exist before the buildings are placed, unlike `Roads::mask`.
pub fn surface_mask(vt: &VecTile, ext: f32, um: f32) -> Vec<u8> {
    let urban = vt.buildings.len() > 150;
    let k = RASTER as f32 / ext;
    let skip = redundant(vt, urban, um);
    let mut mask = vec![0u8; RASTER * RASTER];
    for (li, line) in vt.roads.iter().enumerate() {
        if skip[li] || line.brunnel != 0 || line.class > RC_SERVICE || (line.class == RC_SERVICE && line.sub == 1) || line.pts.len() < 2 {
            continue;
        }
        let cf = cfg(line, urban);
        // (metres -> tile units -> raster pixels)
        let hw = ((cf.drive * 0.5 - 0.5).max(1.0) / um) * k;
        for piece in clip(&line.pts, ext) {
            for w in piece.windows(2) {
                stamp_segment(&mut mask, Vec2::new(w[0][0] * k, w[0][1] * k), Vec2::new(w[1][0] * k, w[1][1] * k), hw);
            }
        }
    }
    mask
}

pub fn build(cx: &Ctx, vt: &VecTile, src: &crate::sources::Sources, mesh: &mut Mesh, footprints: &mut Vec<Footprint>, built: &[u8]) -> Roads {
    let ext = cx.ext;
    let um = cx.unit_m;
    let urban = vt.buildings.len() > 150;
    let yellow = cx.lon < -30.0;
    let mut out = Roads { verts: Vec::new(), idx: Vec::new(), net: RoadNet { left: drives_left(cx.lat, cx.lon), ..Default::default() }, mask: vec![0u8; RASTER * RASTER], radius: 0.0 };
    let own_bridges = bridge_pieces(vt);
    // Roads that only repeat another one are not laid a second time.
    let dup = redundant(vt, urban, um);
    let mut mids: Vec<(u64, u32, f32)> = Vec::new();
    let mut order: Vec<usize> = (0..vt.roads.len()).collect();
    let prio = |c: u8| match c {
        RC_PATH => 0,
        RC_TRACK => 1,
        RC_RAIL => 2,
        c => 12 - c as i32,
    };
    order.sort_by_key(|i| prio(vt.roads[*i].class));
    let upv = cx.center.as_vec3().normalize();
    let left = out.net.left;

    // Junctions, so markings, crossings and signals go where streets really meet
    // rather than wherever the map happens to split a road in two.
    let junc = junctions(vt, urban, um);
    let mut n_lamps = 0usize;

    // Fill the little wedges left where independently tessellated road ribbons
    // meet. The through roads are drawn afterward so their lane paint continues
    // across the apron without exposing terrain between the arms.
    let mut pads: Vec<(Vec2, f32, u8)> = Vec::new();
    for entries in &junc {
        for (p, node) in entries.iter().filter(|(_, j)| j.legs >= 3) {
            let p = Vec2::from(*p);
            if let Some(existing) = pads.iter_mut().find(|(q, _, _)| (*q - p).length() < 3.0 / um) {
                existing.1 = existing.1.max(node.half + 2.0);
                existing.2 = existing.2.min(node.best);
            } else {
                pads.push((p, node.half + 2.0, node.best));
            }
        }
    }
    for (p, radius, class) in pads.into_iter().take(2048) {
        let center = cx.pos(p.into(), cx.ground(p.into()) + 0.15);
        let start = out.verts.len() as u32;
        let kind = class as u32 | (yellow as u32) << 5 | (left as u32) << 13;
        let vertex = |position: Vec3, across: f32| RoadVertex { pos: position.into(), q: [across, 0.0, radius, 1e4], kind: kind as f32 };
        out.verts.push(vertex(center, 0.0));
        let count = 16u32;
        for i in 0..=count {
            let a = i as f32 / count as f32 * std::f32::consts::TAU;
            let offset = Vec2::new(a.cos(), a.sin()) * radius;
            let position = cx.pos((p + offset / um).into(), cx.ground((p + offset / um).into()) + 0.15);
            out.radius = out.radius.max(position.length());
            out.verts.push(vertex(position, offset.x));
        }
        for i in 0..count {
            out.idx.extend([start, start + i + 1, start + i + 2]);
        }
    }

    for li in order {
        if dup[li] {
            continue;
        }
        let line = &vt.roads[li];
        let cf = cfg(line, urban);
        let half = cf.drive * 0.5 + if cf.walk { 1.9 } else { 0.0 };
        for piece in clip(&line.pts, ext) {
          let spans = if line.brunnel == 0 { water_spans(piece, vt, um) } else { vec![(piece, false)] };
          for (piece, auto_bridge) in spans {
        let is_bridge = (line.brunnel == 1 || auto_bridge) && line.class != RC_PATH && line.class != RC_TRACK;
        let lit = urban && line.brunnel != 2 && line.class <= RC_MINOR && (cf.walk || (line.class <= RC_TRUNK && !line.ramp) || is_bridge);
        let kind = line.class as u32 | (line.oneway as u32) << 4 | (yellow as u32) << 5 | (cf.walk as u32) << 6 | (cf.lanes as u32) << 8 | (cf.parking as u32) << 12 | (left as u32) << 13 | (lit as u32) << 14;
            // Resample to at most ~9 m between points so the ribbon follows the ground.
            let mut pts: Vec<[f32; 2]> = Vec::new();
            let mut orig: Vec<usize> = Vec::new();
            for (k, w) in piece.windows(2).enumerate() {
                let l = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]) * um;
                if l < 0.05 && k + 2 < piece.len() {
                    continue;
                }
                let n = (l / 9.0).ceil().max(1.0) as usize;
                orig.push(pts.len());
                for j in 0..n {
                    let t = j as f32 / n as f32;
                    pts.push([w[0][0] + (w[1][0] - w[0][0]) * t, w[0][1] + (w[1][1] - w[0][1]) * t]);
                }
            }
            pts.push(piece[piece.len() - 1]);
            if pts.len() < 2 {
                continue;
            }
            let n = pts.len();
            let mut cum = vec![0f32; n];
            for i in 1..n {
                cum[i] = cum[i - 1] + (pts[i][0] - pts[i - 1][0]).hypot(pts[i][1] - pts[i - 1][1]) * um;
            }
            let len = cum[n - 1];
            if len < 0.5 {
                continue;
            }
            let edges = [edge_of(pts[0], ext), edge_of(pts[n - 1], ext)];
            // Junctions along the way where this road gives way: (distance along, node).
            let mut stops: Vec<(f32, NodeInfo)> = Vec::new();
            for (p, j) in junc[li].iter().filter(|(_, j)| j.stops(line.class, urban)) {
                // Project the junction onto this piece.
                let q = Vec2::from(*p);
                let mut best = (2.0 / um, 0.0);
                for i in 0..n - 1 {
                    let (a, b) = (Vec2::from(pts[i]), Vec2::from(pts[i + 1]));
                    let t = ((q - a).dot(b - a) / (b - a).length_squared().max(1e-9)).clamp(0.0, 1.0);
                    let d = (a + (b - a) * t - q).length();
                    if d < best.0 {
                        best = (d, cum[i] + (cum[i + 1] - cum[i]) * t);
                    }
                }
                if best.0 < 2.0 / um {
                    stops.push((best.1, *j));
                }
            }
            // Left-pointing unit normals (tile units are x east, y south) with mitred corners.
            let nrm: Vec<Vec2> = (0..n)
                .map(|i| {
                    let d = |a: usize, b: usize| Vec2::new(pts[b][0] - pts[a][0], pts[b][1] - pts[a][1]).normalize_or_zero();
                    let (d0, d1) = (if i > 0 { d(i - 1, i) } else { d(0, 1) }, if i + 1 < n { d(i, i + 1) } else { d(n - 2, n - 1) });
                    let m = (d0 + d1).normalize_or_zero();
                    let m = if m == Vec2::ZERO { d0 } else { m };
                    Vec2::new(m.y, -m.x) / m.dot(d0).max(0.55)
                })
                .collect();
            let side = |i: usize, s: f32, w: f32| [pts[i][0] + nrm[i].x * s * w / um, pts[i][1] + nrm[i].y * s * w / um];

            // Surface height along the centreline.
            let bridge = (line.brunnel == 1 || auto_bridge) && line.class != RC_PATH && line.class != RC_TRACK;
            let mut zs: Vec<f32> = pts.iter().map(|p| cx.ground(*p)).collect();
            let mut pre = 0.0;
            if bridge {
                let me = own_bridges.iter().position(|b| b[0] == piece[0] && b.len() == piece.len()).unwrap_or(usize::MAX);
                let (ea, za) = bridge_end(src, cx, &own_bridges, pts[0], me);
                let (eb, zb) = bridge_end(src, cx, &own_bridges, pts[n - 1], me);
                let total = ea + len + eb;
                let camber = (total * 0.012).min(5.0);
                pre = ea;
                for i in 0..n {
                    let t = (ea + cum[i]) / total;
                    let clear = (std::f32::consts::PI * t).sin().max(0.0) * 6.0;
                    zs[i] = (za + (zb - za) * t + camber * 4.0 * t * (1.0 - t)).max(zs[i] + clear);
                }
            } else {
                let grade = match line.class { RC_MOTORWAY => 0.06, RC_TRUNK => 0.08, RC_PRIMARY | RC_SECONDARY => 0.09, _ => 0.12 };
                zs = grade_profile(&zs, &cum, grade);
            }

            if line.brunnel != 2 {
                if bridge {
                    // Solid deck with parapets, soffit and piers, in the building mesh.
                    let thick = if len + pre > 300.0 { 1.8 } else { 1.0 };
                    let sty = [(half * 4.0).min(255.0) as u8, (kind & 255) as u8, (kind >> 8) as u8, 0];
                    let grey = [150, 148, 142, K_PLAIN];
                    for i in 0..n - 1 {
                        let c = |k: usize, s: f32, dz: f32| cx.pos(side(k, s, half), zs[k] + dz);
                        let (al0, al1) = (pre + cum[i], pre + cum[i + 1]);
                        mesh.poly(false, &[(c(i, 1.0, 0.0), [-half, al0]), (c(i, -1.0, 0.0), [half, al0]), (c(i + 1, -1.0, 0.0), [half, al1]), (c(i + 1, 1.0, 0.0), [-half, al1])], [0, 0, 0, K_DECK], sty, Some(upv));
                        for s in [1.0f32, -1.0] {
                            mesh.poly(false, &[(c(i, s, -thick), [al0, 0.0]), (c(i + 1, s, -thick), [al1, 0.0]), (c(i + 1, s, 0.9), [al1, 1.9]), (c(i, s, 0.9), [al0, 1.9])], grey, [90, 0, 120, 0], None);
                            let cap = |k: usize, dz: f32| cx.pos(side(k, s, half), zs[k] + dz);
                            mesh.poly(false, &[(cap(i, 0.92), [al0, 0.0]), (cap(i + 1, 0.92), [al1, 0.0]), (cap(i + 1, 1.08), [al1, 0.16]), (cap(i, 1.08), [al0, 0.16])], [172, 174, 173, K_PLAIN], [90, 0, 120, 0], None);
                        }
                        mesh.poly(false, &[(c(i, 1.0, -thick), [0.0, al0]), (c(i, -1.0, -thick), [1.0, al0]), (c(i + 1, -1.0, -thick), [1.0, al1]), (c(i + 1, 1.0, -thick), [0.0, al1])], grey, [90, 0, 120, 0], None);
                        let ring: Vec<[f32; 2]> = [side(i, 1.0, half), side(i, -1.0, half), side(i + 1, -1.0, half), side(i + 1, 1.0, half)].iter().map(|p| [p[0] / ext, p[1] / ext]).collect();
                        let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
                        for p in &ring {
                            lo = [lo[0].min(p[0]), lo[1].min(p[1])];
                            hi = [hi[0].max(p[0]), hi[1].max(p[1])];
                        }
                        let z = zs[i].min(zs[i + 1]);
                        footprints.push(Footprint { min: lo, max: hi, ring, base: z - thick - 0.4, top: zs[i].max(zs[i + 1]) + 0.2, kind: 1, col: [150, 148, 142], main: (0, 0), det: (0, 0), fine: (0, 0), far: (0, 0), alive: true, center: cx.pos(pts[i], z), area: 0.0, sty: [0; 4], gref: 0.0, roof_col: [0; 3], tiers: Vec::new() });
                    }
                    let total = pre + len;
                    let sp = if total > 400.0 { 130.0 } else { 34.0 };
                    let mut next = (pre / sp).ceil() * sp;
                    for i in 0..n - 1 {
                        while next >= pre + cum[i] && next < pre + cum[i + 1] {
                            let g = cx.ground(pts[i]);
                            if zs[i] - g > 3.5 {
                                let m = cx.metres(pts[i]);
                                let t = Vec2::new(nrm[i].y, -nrm[i].x).normalize_or_zero();
                                let (a, b) = (nrm[i].normalize_or_zero() * (half * if sp > 100.0 { 0.9 } else { 0.55 }), t * if sp > 100.0 { 2.6 } else { 0.7 });
                                mesh.boxm(cx, false, m, nrm[i].normalize_or_zero() * (half + 0.7), t * 2.1, zs[i] - thick - 0.18, zs[i] - thick + 0.22, [142, 141, 136, K_PLAIN], [40, 0, 110, 0], None);
                                mesh.boxm(cx, false, m, a, b, g - 3.0, zs[i] - thick - 0.18, grey, [40, 0, 110, 0], None);
                            }
                            next += sp;
                        }
                    }
                    // Keep the deck shader's road markings at distance; a
                    // slightly raised ribbon gives the close road pass a stable
                    // surface through LOD changes without depth fighting.
                    let v0 = out.verts.len() as u32;
                    for i in 0..n {
                        let endd = stops.iter().fold(1e4f32, |m, (s, j)| m.min((cum[i] - s).abs() - (j.half - half)));
                        for side_sign in [1.0f32, -1.0] {
                            let p = side(i, side_sign, half);
                            let pos = cx.pos(p, zs[i] + 0.08);
                            out.radius = out.radius.max(pos.length());
                            out.verts.push(RoadVertex { pos: pos.into(), q: [-side_sign * half, pre + cum[i], half, endd], kind: kind as f32 });
                        }
                    }
                    for i in 0..n as u32 - 1 {
                        let a = v0 + i * 2;
                        out.idx.extend([a, a + 1, a + 2, a + 1, a + 3, a + 2]);
                    }
                } else {
                    // Ribbon on the ground: each edge takes its own height so it lies on slopes.
                    let v0 = out.verts.len() as u32;
                    for i in 0..n {
                        // Measured so that the junction's edge is always at `half`.
                        let endd = stops.iter().fold(1e4f32, |m, (s, j)| m.min((cum[i] - s).abs() - (j.half - half)));
                        for s in [1.0f32, -1.0] {
                            let p = side(i, s, half);
                            let pos = cx.pos(p, zs[i] + 0.12);
                            out.radius = out.radius.max(pos.length());
                            out.verts.push(RoadVertex { pos: pos.into(), q: [-s * half, cum[i], half, endd], kind: kind as f32 });
                        }
                    }
                    for i in 0..n as u32 - 1 {
                        let a = v0 + i * 2;
                        out.idx.extend([a, a + 1, a + 2, a + 1, a + 3, a + 2]);
                    }
                }
                // Street furniture: lights along the road, signals where it meets a big one.
                {
                    let k = RASTER as f32 / ext;
                    let free = |p: [f32; 2]| {
                        let (bx, by) = ((p[0] * k) as i64, (p[1] * k) as i64);
                        p[0] >= 0.0 && p[1] >= 0.0 && bx < RASTER as i64 && by < RASTER as i64 && built[by as usize * RASTER + bx as usize] == 0
                    };
                    // Point at a distance along: tile position, left normal, tangent, deck height.
                    let at = |s: f32| {
                        let i = cum.partition_point(|c| *c <= s).clamp(1, n - 1) - 1;
                        let t = ((s - cum[i]) / (cum[i + 1] - cum[i]).max(1e-3)).clamp(0.0, 1.0);
                        let p = [pts[i][0] + (pts[i + 1][0] - pts[i][0]) * t, pts[i][1] + (pts[i + 1][1] - pts[i][1]) * t];
                        let d = Vec2::new(pts[i + 1][0] - pts[i][0], pts[i + 1][1] - pts[i][1]).normalize_or_zero();
                        (p, Vec2::new(d.y, -d.x), d, zs[i] + (zs[i + 1] - zs[i]) * t)
                    };
                    let stand = |p: [f32; 2], nl: Vec2, off: f32, deck: f32| {
                        let q = [p[0] + nl.x * off / um, p[1] + nl.y * off / um];
                        (free(q)).then(|| (cx.metres(q), if is_bridge { deck } else { cx.ground(q) }))
                    };
                    if lit {
                        let (sp, h, inset, arm) = lamp_spec(line.class);
                        let mut j = ((pre / sp) - 0.5).ceil().max(0.0);
                        while (j + 0.5) * sp - pre < len && n_lamps < 3000 {
                            let (p, nl, d, deck) = at((j + 0.5) * sp - pre);
                            let sgn = if j as i64 % 2 == 0 { 1.0 } else { -1.0 };
                            if let Some((m, z)) = stand(p, nl, sgn * (half - inset), deck) {
                                lamp_post(cx, mesh, m, -nl * sgn, d, z, h, arm);
                                n_lamps += 1;
                            }
                            j += 1.0;
                        }
                    }
                    // Signals face the traffic arriving at each junction end.
                    let kerb = half - if cf.walk { 1.9 } else { 0.0 };
                    for (sj, j) in stops.iter().filter(|(_, j)| j.signalled(urban) && cf.lanes > 0) {
                        for dir in [1.0f32, -1.0] {
                            let s = sj - dir * (j.half + 4.9);
                            if s < 2.0 || s > len - 2.0 || (line.oneway && dir < 0.0) {
                                continue;
                            }
                            let (p, nl, d, deck) = at(s);
                            // Arriving traffic keeps to its own side of the road.
                            let sgn = if (dir < 0.0) != left { 1.0 } else { -1.0 };
                            if let Some((m, z)) = stand(p, nl, sgn * (kerb + 0.5), deck) {
                                let (jp, ..) = at(*sj);
                                let key = node_key(cx.id, jp, ext);
                                let axis = ((d.y.atan2(d.x) / std::f32::consts::FRAC_PI_2 + 0.5).floor() as i32 & 1) as u8;
                                let seed = (hash(key as u32 ^ (key >> 32) as u32, 7) * 127.0) as u8 | axis << 7;
                                signal(cx, mesh, m, -nl * sgn, d, z, if kerb > 5.0 { kerb * 0.6 } else { 0.0 }, seed);
                            }
                        }
                    }
                }
                // Keep trees off the carriageway.
                let k = RASTER as f32 / ext;
                for i in 0..n - 1 {
                    let w = half + 1.0;
                    let q: Vec<[f32; 2]> = [side(i, 1.0, w), side(i, -1.0, w), side(i + 1, -1.0, w), side(i + 1, 1.0, w)].iter().map(|p| [p[0] * k, p[1] * k]).collect();
                    crate::mvt::fill_rings(&mut out.mask, RASTER, RASTER, &[q], 1);
                }
            }

            if cf.cars {
                let lanes_all = cf.lanes.max(1);
                let drive_half = (cf.drive - if cf.parking { 4.4 } else { 0.0 }) * 0.5;
                let lane_w = (2.0 * drive_half / lanes_all as f32).min(3.7);
                let pi = out.net.paths.len() as u32;
                let nodes = [node_key(cx.id, pts[0], ext), node_key(cx.id, pts[n - 1], ext)];
                for (k, oi) in orig.iter().enumerate().skip(1) {
                    let _ = k;
                    mids.push((node_key(cx.id, pts[*oi], ext), pi, cum[*oi]));
                }
                out.net.total_len += len;
                out.net.paths.push(Path {
                    pts: (0..n).map(|i| cx.pos(pts[i], zs[i] + if bridge { 0.02 } else { 0.14 })).collect(),
                    cum,
                    len,
                    class: line.class,
                    oneway: line.oneway,
                    lanes: if line.oneway { lanes_all } else { (lanes_all / 2).max(1) },
                    lane_w,
                    park_off: if cf.parking { drive_half + 1.1 } else { 0.0 },
                    speed: cf.speed,
                    hidden: line.brunnel == 2,
                    drive_half: cf.drive * 0.5,
                    walk: cf.walk,
                    bridge,
                    local: pts.clone(),
                    alt: (0..n).map(|i| zs[i] + if bridge { 0.02 } else { 0.14 }).collect(),
                    nodes,
                    edge: edges,
                    junctions: Vec::new(),
                    keep_clear: stops.iter().map(|(s, j)| (*s, j.half + 6.5)).collect(),
                });
            }
          }
        }
    }

    // Wire up the graph: path ends meet other ends, or the middle of a through road.
    for (i, p) in out.net.paths.iter().enumerate() {
        out.net.nodes.entry(p.nodes[0]).or_default().push(Link { path: i as u32, at: 0.0, end: 0 });
        out.net.nodes.entry(p.nodes[1]).or_default().push(Link { path: i as u32, at: p.len, end: 1 });
    }
    for (key, pi, s) in mids {
        if let Some(links) = out.net.nodes.get_mut(&key) {
            if links.iter().any(|l| l.path != pi) {
                links.push(Link { path: pi, at: s, end: 2 });
                out.net.paths[pi as usize].junctions.push((s, key));
            }
        }
    }
    out
}

const PAINT: &[([f32; 3], f32)] = &[
    ([0.80, 0.80, 0.78], 0.24), // white
    ([0.015, 0.015, 0.018], 0.19), // black
    ([0.16, 0.17, 0.18], 0.17), // grey
    ([0.45, 0.46, 0.47], 0.13), // silver
    ([0.02, 0.06, 0.22], 0.09), // blue
    ([0.35, 0.02, 0.02], 0.08), // red
    ([0.03, 0.10, 0.05], 0.03), // green
    ([0.30, 0.22, 0.12], 0.03), // beige / gold
    ([0.55, 0.30, 0.02], 0.02), // orange
    ([0.60, 0.48, 0.03], 0.02), // yellow
];
/// A car colour in roughly the proportions seen on real roads.
pub fn paint(h: f32) -> [f32; 3] {
    let mut acc = 0.0;
    for (c, w) in PAINT {
        acc += w;
        if h < acc {
            return *c;
        }
    }
    PAINT[0].0
}
/// A car body type: mostly ordinary cars, the odd van, pickup or sports car.
pub fn car_model(h: f32, big_road: bool) -> u8 {
    let table: &[(u8, f32)] = if big_road { &[(0, 0.26), (1, 0.14), (2, 0.24), (3, 0.1), (4, 0.1), (5, 0.03), (6, 0.03), (7, 0.1)] } else { &[(0, 0.3), (1, 0.2), (2, 0.26), (3, 0.08), (4, 0.07), (5, 0.04), (6, 0.02), (7, 0.03)] };
    let mut acc = 0.0;
    for (m, w) in table {
        acc += w;
        if h < acc {
            return *m;
        }
    }
    0
}

/// Cars parked along the kerbs of built-up streets.
pub fn park_cars(cx: &Ctx, net: &RoadNet, hoods: &Hoods, built: &[u8]) -> Vec<Parked> {
    let mut out = Vec::new();
    let up0 = cx.center.as_vec3().normalize();
    let k = RASTER as f32 / cx.ext;
    for (pi, p) in net.paths.iter().enumerate() {
        if p.park_off == 0.0 || p.hidden || p.len < 30.0 {
            continue;
        }
        for (si, side) in [1.0f32, -1.0].iter().enumerate() {
            let seed = (pi as u32 * 2 + si as u32).wrapping_mul(747796405) ^ cx.id.x.wrapping_mul(31) ^ cx.id.y;
            let mut s = 10.0 + hash(seed, 0) * 6.0;
            let mut j = 0u32;
            while s < p.len - 10.0 {
                j += 1;
                let h = hash(seed, j * 4);
                let (c, t) = p.sample(s);
                let right = t.cross(up0).normalize_or_zero();
                let pos = c + right * (*side * p.park_off);
                // Tile-unit position, to look the spot up in the neighbourhood maps.
                let m = pos.dot(Vec3::Z.cross(up0).normalize());
                let nn = pos.dot(up0.cross(Vec3::Z.cross(up0).normalize()));
                let u = cx.units(Vec2::new(m, -nn));
                let hood = hoods.at(cx, u);
                let fill = ((hood.cover - 0.06) / 0.25).clamp(0.0, 0.85);
                let (bx, by) = ((u[0] * k) as i64, (u[1] * k) as i64);
                let clear = bx >= 1 && by >= 1 && bx < RASTER as i64 - 1 && by < RASTER as i64 - 1 && built[by as usize * RASTER + bx as usize] == 0;
                let crossing = p.keep_clear.iter().any(|(sj, r)| (s - sj).abs() < *r);
                if h < fill && clear && !crossing && out.len() < 2600 {
                    let model = car_model(hash(seed, j * 4 + 1), false).min(5);
                    let facing = if (*side > 0.0) == net.left && !p.oneway { -t } else { t };
                    out.push(Parked { pos: pos - up0 * 0.02, fwd: facing, up: up0, model, col: paint(hash(seed, j * 4 + 2)), shift: Vec3::ZERO, bump: Vec3::ZERO });
                }
                s += 5.6 + hash(seed, j * 4 + 3) * 2.5;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(pts: Vec<[f32; 2]>, class: u8) -> RoadLine {
        RoadLine { pts, class, brunnel: 0, oneway: false, ramp: false, layer: 0, sub: 0 }
    }
    fn tile(roads: Vec<RoadLine>) -> VecTile {
        VecTile { extent: 1024.0, water: vec![], buildings: vec![], roads, landcover: vec![], landuse: vec![], aero_lines: vec![], aero_polys: vec![], gates: vec![] }
    }

    #[test]
    fn a_repeated_road_is_laid_once() {
        let a = vec![[100.0, 500.0], [400.0, 500.0]];
        let t = tile(vec![line(a.clone(), RC_MINOR), line(a, RC_MINOR)]);
        let dup = redundant(&t, false, 1.0);
        assert_eq!(dup.iter().filter(|d| **d).count(), 1);
    }

    #[test]
    fn crossing_roads_both_stay() {
        let t = tile(vec![line(vec![[100.0, 500.0], [400.0, 500.0]], RC_MINOR), line(vec![[250.0, 300.0], [250.0, 700.0]], RC_MINOR)]);
        assert!(redundant(&t, false, 1.0).iter().all(|d| !*d));
    }

    #[test]
    fn a_road_beside_another_stays() {
        let t = tile(vec![line(vec![[100.0, 500.0], [400.0, 500.0]], RC_MINOR), line(vec![[100.0, 520.0], [400.0, 520.0]], RC_MINOR)]);
        assert!(redundant(&t, false, 1.0).iter().all(|d| !*d));
    }

    #[test]
    fn buildings_on_the_street_are_found() {
        let t = tile(vec![line(vec![[100.0, 500.0], [400.0, 500.0]], RC_MINOR)]);
        let mask = surface_mask(&t, 1024.0, 1.0);
        let k = RASTER as f32 / 1024.0;
        assert!(mask[(500.0 * k) as usize * RASTER + (250.0 * k) as usize] != 0);
        assert!(mask[(560.0 * k) as usize * RASTER + (250.0 * k) as usize] == 0);
        let on = [[240.0, 497.0], [260.0, 497.0], [260.0, 503.0], [240.0, 503.0]];
        let off = [[240.0, 600.0], [260.0, 600.0], [260.0, 610.0], [240.0, 610.0]];
        assert!(crate::buildings::on_road(&mask, &on, 1024.0));
        assert!(!crate::buildings::on_road(&mask, &off, 1024.0));
    }

    fn road(pts: &[[f32; 2]], alt: &[f32], drive_half: f32, walk: bool, bridge: bool, hidden: bool) -> Path {
        let mut cum = vec![0f32; pts.len()];
        for i in 1..pts.len() {
            cum[i] = cum[i - 1] + (pts[i][0] - pts[i - 1][0]).hypot(pts[i][1] - pts[i - 1][1]);
        }
        let len = cum.last().copied().unwrap_or(0.0);
        Path {
            pts: pts.iter().map(|p| Vec3::new(p[0], 0.0, p[1])).collect(),
            cum,
            len,
            class: RC_MINOR,
            oneway: false,
            lanes: 1,
            lane_w: 3.0,
            park_off: 0.0,
            speed: 8.0,
            hidden,
            drive_half,
            walk,
            bridge,
            local: pts.to_vec(),
            alt: alt.to_vec(),
            nodes: [0, 0],
            edge: [0, 0],
            junctions: Vec::new(),
            keep_clear: Vec::new(),
        }
    }

    fn net(paths: Vec<Path>) -> RoadNet {
        RoadNet { paths, ..Default::default() }
    }

    #[test]
    fn road_index_finds_carriageway_sidewalk_and_off_ribbon() {
        use crate::surface::Surface;
        let n = net(vec![road(&[[100.0, 500.0], [400.0, 500.0]], &[10.0, 10.0], 4.0, true, false, false)]);
        let idx = RoadIndex::build(&n, 1024.0, 1.0);
        let c = idx.query(&n, [250.0, 501.0]).unwrap();
        assert_eq!(c.kind, Surface::Asphalt);
        assert!(!c.sidewalk);
        assert!((c.dist - 1.0).abs() < 1e-4);
        assert!((c.h - 10.0).abs() < 1e-9);
        assert!((c.s - 150.0).abs() < 1e-4);
        let s = idx.query(&n, [250.0, 505.5]).unwrap();
        assert_eq!(s.kind, Surface::Concrete);
        assert!(s.sidewalk);
        assert!(idx.query(&n, [250.0, 507.0]).is_none());
        assert!(idx.query(&n, [250.0, 480.0]).is_none());
    }

    #[test]
    fn road_index_reports_a_bridge_deck_and_a_hidden_tunnel() {
        let n = net(vec![
            road(&[[100.0, 200.0], [400.0, 200.0]], &[30.0, 30.0], 5.0, false, true, false),
            road(&[[100.0, 800.0], [400.0, 800.0]], &[0.0, 0.0], 5.0, false, false, true),
        ]);
        let idx = RoadIndex::build(&n, 1024.0, 1.0);
        let b = idx.query(&n, [250.0, 201.0]).unwrap();
        assert!(b.bridge);
        assert!(!b.hidden);
        assert!((b.h - 30.0).abs() < 1e-9);
        let t = idx.query(&n, [250.0, 801.0]).unwrap();
        assert!(t.hidden);
        assert!(!t.bridge);
        assert_eq!(t.kind, crate::surface::Surface::Asphalt);
    }

    #[test]
    fn road_index_covers_a_path_crossing_the_tile_border() {
        let n = net(vec![road(&[[-8.0, 500.0], [1032.0, 500.0]], &[4.0, 4.0], 3.0, false, false, false)]);
        let idx = RoadIndex::build(&n, 1024.0, 1.0);
        assert!(idx.query(&n, [2.0, 501.0]).is_some());
        assert!(idx.query(&n, [1022.0, 501.0]).is_some());
        // The candidate grid can be inspected without a GPU.
        assert!(idx.cell_ids(15, 31).contains(&0));
    }

    #[test]
    fn road_index_interpolates_closest_point_and_altitude() {
        let n = net(vec![road(&[[100.0, 500.0], [200.0, 500.0], [200.0, 600.0]], &[0.0, 0.0, 20.0], 5.0, false, false, false)]);
        let idx = RoadIndex::build(&n, 1024.0, 1.0);
        let a = idx.query(&n, [150.0, 503.0]).unwrap();
        assert!((a.dist - 3.0).abs() < 1e-4);
        assert!((a.s - 50.0).abs() < 1e-4);
        assert!(a.h.abs() < 1e-9);
        let b = idx.query(&n, [203.0, 550.0]).unwrap();
        assert!((b.dist - 3.0).abs() < 1e-4);
        assert!((b.s - 150.0).abs() < 1e-4);
        assert!((b.h - 10.0).abs() < 1e-9);
    }
}
