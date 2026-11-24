//! Airports. Two sources make each one what it really is:
//! the OurAirports database (public domain; `assets/airports.tsv.gz`, rebuilt
//! with `assets/mkdb.py`) says where every airfield in the world is, what it
//! is called, and how long, wide, lit and paved its runways are; the
//! OpenStreetMap vector tiles give the layout as it lies on the imagery:
//! runway and taxiway centrelines, aprons, gates. `Geo` joins the two into
//! one airfield in a local east/north frame, with the taxi graph and parking
//! stands the traffic uses; `mesh` turns it into pavement, paint, lights and
//! parked aircraft.
use crate::geo::{self, TileId};
use crate::sources::{Sources, DATA_ZOOM};
use anyhow::{anyhow, Result};
use glam::{DVec3, Vec2, Vec3};
use std::collections::HashMap;
use std::io::Read;
use std::sync::OnceLock;

pub const FT: f32 = 0.3048;

// ---------------------------------------------------------------- database

pub struct DbRunway {
    pub len: f32,
    pub width: f32,
    pub paved: bool,
    pub concrete: bool,
    pub lighted: bool,
    /// low-numbered end, high-numbered end
    pub ident: [String; 2],
    /// threshold positions (degrees), where the database has them
    pub pos: [Option<(f64, f64)>; 2],
    pub disp: [f32; 2],
}

pub struct Apt {
    pub ident: String,
    pub iata: String,
    /// 0 small, 1 medium, 2 large
    pub kind: u8,
    pub lat: f64,
    pub lon: f64,
    pub elev: f32,
    pub name: String,
    pub city: String,
    pub country: String,
    pub sched: bool,
    pub rwys: Vec<DbRunway>,
}

impl Apt {
    /// "KSFO / SFO" or just the identifier.
    pub fn code(&self) -> String {
        if self.iata.is_empty() || self.iata == self.ident { self.ident.clone() } else { format!("{} / {}", self.ident, self.iata) }
    }
    pub fn longest(&self) -> f32 {
        self.rwys.iter().map(|r| r.len).fold(0.0, f32::max)
    }
    pub fn any_paved(&self) -> bool {
        self.rwys.iter().any(|r| r.paved)
    }
    /// How far its pavement may reach from the reference point (m).
    pub fn reach(&self) -> f64 {
        let p0 = geo::lla_to_pos(self.lat.to_radians(), self.lon.to_radians(), 0.0);
        let far = self.rwys.iter().flat_map(|r| r.pos.iter().flatten()).map(|(la, lo)| (geo::lla_to_pos(la.to_radians(), lo.to_radians(), 0.0) - p0).length()).fold(0.0, f64::max);
        let base = [1500.0f64, 2600.0, 4200.0][self.kind as usize].max(self.longest() as f64 * 0.75);
        (far + 900.0).max(base).min(9000.0)
    }
}

pub struct Db {
    pub apts: Vec<Apt>,
    grid: HashMap<(i16, i16), Vec<u32>>,
}

/// "09L" and "9L" are the same runway.
pub fn norm_ident(s: &str) -> String {
    let s = s.trim().to_ascii_uppercase();
    let b = s.as_bytes();
    if b.len() >= 2 && b[0] == b'0' && b[1].is_ascii_digit() { s[1..].to_string() } else { s }
}

/// The compass heading (degrees) a runway identifier stands for, if it is a number.
pub fn ident_heading(s: &str) -> Option<f32> {
    let d: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: f32 = d.parse().ok()?;
    (n >= 1.0 && n <= 36.0).then_some(n * 10.0)
}

fn surface(s: &str, kind: u8, len: f32, lighted: bool) -> (bool, bool) {
    let u = s.to_ascii_uppercase();
    let has = |k: &[&str]| k.iter().any(|k| u.starts_with(k));
    if has(&["CON", "PEM", "PCC"]) {
        (true, true)
    } else if has(&["ASP", "BIT", "TAR", "PAV", "MAC", "ASF", "SEAL", "HARD", "COM", "COP", "B"]) || u == "C" || u == "A" {
        (true, false)
    } else if has(&["TURF", "GR", "GVL", "EARTH", "DIRT", "SAND", "SOIL", "CLAY", "SNOW", "ICE", "WATER", "CORAL", "LAT", "G", "S", "D", "T"]) {
        (false, false)
    } else {
        // Unknown: big fields are paved, and so are lit or long strips.
        (kind >= 1 || lighted || len > 1100.0, false)
    }
}

impl Db {
    pub fn get() -> &'static Db {
        static DB: OnceLock<Db> = OnceLock::new();
        DB.get_or_init(|| {
            let mut text = String::new();
            let _ = flate2::read::GzDecoder::new(&include_bytes!("../assets/airports.tsv.gz")[..]).read_to_string(&mut text);
            let mut apts: Vec<Apt> = Vec::with_capacity(30_000);
            for line in text.lines() {
                let f: Vec<&str> = line.split('\t').collect();
                let num = |i: usize| f.get(i).and_then(|s| s.parse::<f64>().ok());
                match f.first() {
                    Some(&"A") if f.len() >= 11 => {
                        let (Some(lat), Some(lon)) = (num(4), num(5)) else { continue };
                        apts.push(Apt {
                            ident: f[1].into(),
                            iata: f[2].into(),
                            kind: match f[3] { "L" => 2, "M" => 1, _ => 0 },
                            lat,
                            lon,
                            elev: num(6).unwrap_or(0.0) as f32 * FT,
                            name: f[7].into(),
                            city: f[8].into(),
                            country: f[9].into(),
                            sched: f[10] == "1",
                            rwys: Vec::new(),
                        });
                    }
                    Some(&"R") if f.len() >= 13 => {
                        let Some(a) = apts.last_mut() else { continue };
                        let lighted = f[4] == "1";
                        let mut len = num(1).unwrap_or(0.0) as f32 * FT;
                        let pos = [num(6).zip(num(7)), num(10).zip(num(11))];
                        if let [Some(p), Some(q)] = pos {
                            let d = (geo::lla_to_pos(p.0.to_radians(), p.1.to_radians(), 0.0) - geo::lla_to_pos(q.0.to_radians(), q.1.to_radians(), 0.0)).length() as f32;
                            if len < 100.0 || (d > 200.0 && (d - len).abs() > 0.3 * len) {
                                len = d;
                            }
                        }
                        let (paved, concrete) = surface(f[3], a.kind, len, lighted);
                        let w = num(2).unwrap_or(0.0) as f32 * FT;
                        let width = if w >= 6.0 && w <= 100.0 {
                            w
                        } else if !paved {
                            25.0
                        } else if len >= 2400.0 {
                            45.0
                        } else if len >= 1500.0 {
                            30.0
                        } else if len >= 900.0 {
                            23.0
                        } else {
                            18.0
                        };
                        a.rwys.push(DbRunway { len, width, paved, concrete, lighted, ident: [f[5].to_ascii_uppercase(), f[9].to_ascii_uppercase()], pos, disp: [num(8).unwrap_or(0.0) as f32 * FT, num(12).unwrap_or(0.0) as f32 * FT] });
                    }
                    _ => {}
                }
            }
            let mut grid: HashMap<(i16, i16), Vec<u32>> = HashMap::new();
            for (i, a) in apts.iter().enumerate() {
                grid.entry((a.lat.floor() as i16, a.lon.floor() as i16)).or_default().push(i as u32);
            }
            Db { apts, grid }
        })
    }

    /// Airports within `radius` metres of a spot (degrees), nearest first.
    pub fn near(&self, lat: f64, lon: f64, radius: f64) -> Vec<(u32, f64)> {
        let p = geo::lla_to_pos(lat.to_radians(), lon.to_radians(), 0.0);
        let dlat = (radius / 111_000.0).ceil() as i32 + 1;
        let dlon = (radius / (111_000.0 * lat.to_radians().cos().max(0.05))).ceil().min(180.0) as i32 + 1;
        let mut out = Vec::new();
        for y in -dlat..=dlat {
            for x in -dlon..=dlon {
                let key = ((lat.floor() as i32 + y) as i16, (lon.floor() as i32 + x + 180).rem_euclid(360) as i16 - 180);
                for &i in self.grid.get(&key).map(|v| &v[..]).unwrap_or(&[]) {
                    let a = &self.apts[i as usize];
                    let d = (geo::lla_to_pos(a.lat.to_radians(), a.lon.to_radians(), 0.0) - p).length();
                    if d <= radius {
                        out.push((i, d));
                    }
                }
            }
        }
        out.sort_by(|a, b| a.1.total_cmp(&b.1));
        out.dedup_by_key(|a| a.0);
        out
    }

    /// The airports of at least `min_kind` between latitudes `lat0..lat1` and
    /// within `half` degrees of longitude `lon` (degrees; any `half` of 180 or more is the whole world).
    pub fn in_box(&self, lat0: f64, lat1: f64, lon: f64, half: f64, min_kind: u8) -> Vec<u32> {
        let mut out = Vec::new();
        let cols: Vec<i32> = if half >= 180.0 {
            (-180..180).collect()
        } else {
            ((lon - half).floor() as i32..=(lon + half).floor() as i32).map(|c| (c + 180).rem_euclid(360) - 180).collect()
        };
        for row in lat0.floor().max(-90.0) as i32..=lat1.floor().min(89.0) as i32 {
            for &col in &cols {
                for &i in self.grid.get(&(row as i16, col as i16)).map(|v| &v[..]).unwrap_or(&[]) {
                    let a = &self.apts[i as usize];
                    let dlon = (a.lon - lon + 540.0).rem_euclid(360.0) - 180.0;
                    if a.kind >= min_kind && a.lat >= lat0 && a.lat <= lat1 && (half >= 180.0 || dlon.abs() <= half) {
                        out.push(i);
                    }
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    pub fn find(&self, code: &str) -> Option<u32> {
        let c = code.trim().to_ascii_uppercase();
        if c.is_empty() {
            return None;
        }
        self.apts.iter().position(|a| a.ident == c).or_else(|| self.apts.iter().position(|a| a.iata == c)).map(|i| i as u32)
    }

    /// Airports matching what was typed: code first, then name and city; big ones first.
    pub fn search(&self, text: &str, max: usize) -> Vec<u32> {
        let q = text.trim().to_lowercase();
        if q.len() < 2 {
            return Vec::new();
        }
        let up = q.to_ascii_uppercase();
        let mut hits: Vec<(i32, u32)> = Vec::new();
        for (i, a) in self.apts.iter().enumerate() {
            let score = if a.ident == up || a.iata == up {
                0
            } else if q.len() >= 3 && (a.ident.starts_with(&up) || a.iata.starts_with(&up)) {
                10
            } else if q.len() >= 3 && a.city.to_lowercase().starts_with(&q) {
                20
            } else if q.len() >= 3 && (a.name.to_lowercase().contains(&q) || a.city.to_lowercase().contains(&q)) {
                30
            } else {
                continue;
            };
            hits.push((score + (2 - a.kind as i32) * 3 - a.sched as i32, i as u32));
        }
        hits.sort();
        hits.truncate(max);
        hits.into_iter().map(|h| h.1).collect()
    }

    /// Whose pavement is at this spot: the nearest airfield, bigger ones
    /// reaching further.
    pub fn owner(&self, lat: f64, lon: f64) -> Option<u32> {
        self.near(lat, lon, 9000.0).into_iter().filter(|(i, d)| *d < self.apts[*i as usize].reach() + 400.0).min_by(|a, b| {
            let k = |x: &(u32, f64)| x.1 / [1.0, 1.6, 2.6][self.apts[x.0 as usize].kind as usize];
            k(a).total_cmp(&k(b))
        }).map(|x| x.0)
    }

    /// What the database knows about the runway drawn on the map at a spot
    /// (degrees) along a bearing (degrees), named `refs` there ("10L/28R").
    pub fn runway_meta(&self, lat: f64, lon: f64, bearing: f32, refs: &str) -> RwyMeta {
        let p = geo::lla_to_pos(lat.to_radians(), lon.to_radians(), 0.0);
        let names: Vec<String> = refs.split(['/', '-', ' ']).map(norm_ident).filter(|s| !s.is_empty()).collect();
        let fold = |a: f32| { let d = a.rem_euclid(180.0); d.min(180.0 - d) };
        let mut best: Option<(f64, u32, usize)> = None;
        for (i, d) in self.near(lat, lon, 9000.0) {
            let a = &self.apts[i as usize];
            for (k, r) in a.rwys.iter().enumerate() {
                let named = !names.is_empty() && r.ident.iter().all(|id| names.contains(&norm_ident(id)));
                let mut score = d / [1.0, 1.6, 2.6][a.kind as usize] + 4000.0;
                if let [Some(q0), Some(q1)] = r.pos {
                    let (a0, a1) = (geo::lla_to_pos(q0.0.to_radians(), q0.1.to_radians(), 0.0), geo::lla_to_pos(q1.0.to_radians(), q1.1.to_radians(), 0.0));
                    let ab = a1 - a0;
                    let t = ((p - a0).dot(ab) / ab.length_squared().max(1.0)).clamp(0.0, 1.0);
                    let off = (a0 + ab * t - p).length();
                    let (e, n, _) = geo::enu(a0);
                    let brg = (ab.dot(e).atan2(ab.dot(n)).to_degrees()) as f32;
                    if off < 160.0 && fold(brg - bearing) < 9.0 {
                        score = off;
                    }
                } else if let Some(h) = ident_heading(&r.ident[0]) {
                    if fold(h - bearing) < 28.0 {
                        score -= 1500.0;
                    }
                }
                if named {
                    score -= 6000.0;
                }
                if best.map(|b| score < b.0).unwrap_or(true) && (named || score < 4000.0) {
                    best = Some((score, i, k));
                }
            }
        }
        match best {
            Some((_, i, k)) => {
                let r = &self.apts[i as usize].rwys[k];
                RwyMeta { width: r.width, paved: r.paved, concrete: r.concrete, lighted: r.lighted, ident: r.ident.clone(), disp: r.disp, found: true }
            }
            None => {
                let kind = self.owner(lat, lon).map(|i| self.apts[i as usize].kind).unwrap_or(0);
                RwyMeta { width: [18.0, 30.0, 45.0][kind as usize], paved: true, concrete: false, lighted: kind > 0, ident: [String::new(), String::new()], disp: [0.0; 2], found: false }
            }
        }
    }
}

pub struct RwyMeta {
    pub width: f32,
    pub paved: bool,
    pub concrete: bool,
    pub lighted: bool,
    pub ident: [String; 2],
    pub disp: [f32; 2],
    pub found: bool,
}

/// Width a taxiway is drawn and levelled with (the map does not say).
pub fn taxi_width(kind: u8) -> f32 {
    [10.0, 18.0, 23.0][kind as usize]
}

// ---------------------------------------------------------------- one airfield

pub struct Runway {
    /// pavement ends, local metres (east, north)
    pub a: Vec2,
    pub b: Vec2,
    pub dir: Vec2,
    pub len: f32,
    pub width: f32,
    /// `ident[0]` is the runway you are on landing over `a` toward `b`
    pub ident: [String; 2],
    /// displaced threshold, metres in from each end
    pub thr: [f32; 2],
    pub paved: bool,
    pub concrete: bool,
    pub lighted: bool,
    /// markings and lighting: 0 basic, 1 non-precision, 2 precision
    pub class: u8,
}

/// Right of a direction (east, north).
pub fn right_of(d: Vec2) -> Vec2 {
    Vec2::new(d.y, -d.x)
}
/// Compass bearing (degrees) of a local direction.
pub fn bearing(d: Vec2) -> f32 {
    d.x.atan2(d.y).to_degrees().rem_euclid(360.0)
}

impl Runway {
    /// (distance along from `a`, distance right of the centreline)
    pub fn local(&self, p: Vec2) -> (f32, f32) {
        let v = p - self.a;
        (v.dot(self.dir), v.dot(right_of(self.dir)))
    }
    /// Threshold and landing direction of one end.
    pub fn end(&self, e: usize) -> (Vec2, Vec2) {
        if e == 0 { (self.a + self.dir * self.thr[0], self.dir) } else { (self.b - self.dir * self.thr[1], -self.dir) }
    }
    pub fn name(&self) -> String {
        format!("{}/{}", self.ident[0], self.ident[1])
    }
    /// Scale of the painted numbers (1 = the full 18 m).
    pub fn mark_scale(&self) -> f32 {
        (self.width * 0.42).clamp(6.0, 18.0) / 18.0
    }
    pub fn has_letter(&self) -> bool {
        self.ident.iter().any(|s| s.ends_with(['L', 'R', 'C']) && s.len() > 1)
    }
}

pub struct Node {
    pub p: Vec2,
    pub edges: Vec<u32>,
}
pub struct Edge {
    pub a: u32,
    pub b: u32,
    pub len: f32,
    /// the runway this is the centreline of
    pub rwy: Option<u16>,
}
#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}
impl Graph {
    pub fn other(&self, e: u32, n: u32) -> u32 {
        let ed = &self.edges[e as usize];
        if ed.a == n { ed.b } else { ed.a }
    }
    pub fn nearest(&self, p: Vec2, taxi_only: bool) -> Option<u32> {
        (0..self.nodes.len() as u32)
            .filter(|n| !taxi_only || self.nodes[*n as usize].edges.iter().any(|e| self.edges[*e as usize].rwy.is_none()))
            .min_by(|a, b| (self.nodes[*a as usize].p - p).length_squared().total_cmp(&(self.nodes[*b as usize].p - p).length_squared()))
    }
    /// Shortest way from one node to another, as the nodes passed. Rolling
    /// down a runway costs more than a taxiway, so traffic uses the taxiways
    /// where there are any.
    pub fn path(&self, from: u32, to: u32) -> Option<Vec<u32>> {
        let n = self.nodes.len();
        let mut dist = vec![f32::INFINITY; n];
        let mut prev = vec![u32::MAX; n];
        let mut heap = std::collections::BinaryHeap::new();
        dist[from as usize] = 0.0;
        heap.push((std::cmp::Reverse(0u32), from));
        while let Some((std::cmp::Reverse(d), u)) = heap.pop() {
            if u == to {
                break;
            }
            if d as f32 > dist[u as usize] * 10.0 + 1.0 {
                continue;
            }
            for &e in &self.nodes[u as usize].edges {
                let ed = &self.edges[e as usize];
                let v = self.other(e, u);
                let nd = dist[u as usize] + ed.len * if ed.rwy.is_some() { 3.0 } else { 1.0 };
                if nd < dist[v as usize] {
                    dist[v as usize] = nd;
                    prev[v as usize] = u;
                    heap.push((std::cmp::Reverse((nd * 10.0) as u32), v));
                }
            }
        }
        if !dist[to as usize].is_finite() {
            return None;
        }
        let mut out = vec![to];
        let mut cur = to;
        while cur != from {
            cur = prev[cur as usize];
            out.push(cur);
        }
        out.reverse();
        Some(out)
    }
}

/// Where an aircraft parks.
pub struct Stand {
    /// where the nose stops, and the way the aircraft faces
    pub pos: Vec2,
    pub dir: Vec2,
    /// largest aircraft that fits: 0 light single, 1 business jet, 2 regional, 3 narrow-body, 4 wide-body
    pub size: u8,
    /// read by the stand layout probe in the tests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub gate: String,
    /// taxi graph node it is reached from
    pub node: u32,
    /// at a terminal (a gate is near): airliners, a jet bridge
    pub terminal: bool,
}

/// A holding position: where a taxiway meets a runway's protected strip.
pub struct Hold {
    pub p: Vec2,
    /// along the taxiway, toward the runway
    pub dir: Vec2,
    pub rwy: u16,
    pub half: f32,
}

pub struct Geo {
    pub idx: u32,
    /// local frame: origin on the sphere at the reference point, and its axes
    pub origin: DVec3,
    pub e: DVec3,
    pub n: DVec3,
    pub u: DVec3,
    /// what the meshes are relative to
    pub center: DVec3,
    pub elev: f32,
    pub radius: f32,
    pub runways: Vec<Runway>,
    /// (centreline, width)
    pub taxiways: Vec<(Vec<Vec2>, f32)>,
    /// polygons: outer ring, then holes
    pub aprons: Vec<Vec<Vec<Vec2>>>,
    pub gates: Vec<(Vec2, String)>,
    pub helipads: Vec<(Vec2, f32)>,
    pub graph: Graph,
    pub stands: Vec<Stand>,
    pub holds: Vec<Hold>,
    hx: Vec2,
    hstep: f32,
    hn: (usize, usize),
    hgrid: Vec<f32>,
}

impl Geo {
    pub fn apt(&self) -> &'static Apt {
        &Db::get().apts[self.idx as usize]
    }
    /// Ground height (m above the sphere) at a local point.
    pub fn h(&self, p: Vec2) -> f32 {
        let q = ((p - self.hx) / self.hstep).clamp(Vec2::ZERO, Vec2::new(self.hn.0 as f32 - 1.001, self.hn.1 as f32 - 1.001));
        let (x0, y0) = (q.x as usize, q.y as usize);
        let (fx, fy) = (q.x - x0 as f32, q.y - y0 as f32);
        let g = |x: usize, y: usize| self.hgrid[y * self.hn.0 + x];
        (g(x0, y0) * (1.0 - fx) + g(x0 + 1, y0) * fx) * (1.0 - fy) + (g(x0, y0 + 1) * (1.0 - fx) + g(x0 + 1, y0 + 1) * fx) * fy
    }
    /// Planet-frame position of a local point at a height above the sphere.
    pub fn world(&self, p: Vec2, h: f64) -> DVec3 {
        (self.origin + self.e * p.x as f64 + self.n * p.y as f64).normalize() * (geo::R + h)
    }
    /// Local point under a planet-frame position.
    pub fn local(&self, w: DVec3) -> Vec2 {
        // Where the ray from the planet's centre meets the tangent plane.
        let s = geo::R / w.dot(self.u).max(1.0);
        let v = w * s - self.origin;
        Vec2::new(v.dot(self.e) as f32, v.dot(self.n) as f32)
    }
    /// Latitude and longitude (radians) of a local point.
    pub fn lla(&self, p: Vec2) -> (f64, f64) {
        let (lat, lon, _) = geo::pos_to_lla(self.world(p, 0.0));
        (lat, lon)
    }
    /// Position relative to `center` of a local point `lift` metres above the ground.
    pub fn rel(&self, p: Vec2, lift: f32) -> Vec3 {
        (self.world(p, (self.h(p) + lift) as f64) - self.center).as_vec3()
    }
    /// A local direction (east, north, up) in the planet frame.
    pub fn dir3(&self, d: Vec3) -> Vec3 {
        (self.e * d.x as f64 + self.n * d.y as f64 + self.u * d.z as f64).as_vec3()
    }
    /// Which end of which paved runway to use with the wind blowing from
    /// `from` degrees: the longest runways first, into the wind.
    pub fn active(&self, wind_from: f32, wind: f32) -> Option<(usize, usize)> {
        let score = |ri: usize, e: usize| {
            let r = &self.runways[ri];
            let hd = bearing(r.end(e).1);
            let head = ((hd - wind_from).to_radians()).cos() * wind.max(0.5);
            r.len * if r.paved { 1.0 } else { 0.5 } + head * 600.0 + if e == 0 { 1.0 } else { 0.0 }
        };
        (0..self.runways.len()).flat_map(|ri| [(ri, 0), (ri, 1)]).max_by(|a, b| score(a.0, a.1).total_cmp(&score(b.0, b.1)))
    }
    /// Is a local point over a runway's pavement? Which one.
    pub fn on_runway(&self, p: Vec2, margin: f32) -> Option<usize> {
        self.runways.iter().position(|r| {
            let (s, x) = r.local(p);
            s > -margin && s < r.len + margin && x.abs() < r.width * 0.5 + margin
        })
    }
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (a + d * t - p).length()
}

fn in_poly(p: Vec2, rings: &[Vec<Vec2>]) -> bool {
    let mut c = false;
    for r in rings {
        let mut j = r.len() - 1;
        for i in 0..r.len() {
            let (a, b) = (r[i], r[j]);
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                c = !c;
            }
            j = i;
        }
    }
    c
}

/// Clip a ring to the square [0, ext]^2 (Sutherland-Hodgman).
fn clip_ring(r: &[[f32; 2]], ext: f32) -> Vec<[f32; 2]> {
    let mut cur: Vec<[f32; 2]> = r.to_vec();
    for (axis, lim, keep_less) in [(0, 0.0, false), (0, ext, true), (1, 0.0, false), (1, ext, true)] {
        let inside = |p: &[f32; 2]| if keep_less { p[axis] <= lim } else { p[axis] >= lim };
        let mut out = Vec::with_capacity(cur.len() + 4);
        for i in 0..cur.len() {
            let (a, b) = (cur[i], cur[(i + 1) % cur.len()]);
            let (ia, ib) = (inside(&a), inside(&b));
            if ia {
                out.push(a);
            }
            if ia != ib {
                let t = (lim - a[axis]) / (b[axis] - a[axis]);
                out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            }
        }
        cur = out;
        if cur.len() < 3 {
            return Vec::new();
        }
    }
    cur
}

/// Put an airfield together from the map tiles around its reference point.
pub fn build_geo(src: &Sources, idx: u32) -> Result<Geo> {
    let db = Db::get();
    let apt = &db.apts[idx as usize];
    let (lat0, lon0) = (apt.lat.to_radians(), apt.lon.to_radians());
    let origin = geo::lla_to_pos(lat0, lon0, 0.0);
    let (e, n, u) = geo::enu(origin);
    let reach = apt.reach();
    let to_local = |lat: f64, lon: f64| {
        let w = geo::lla_to_pos(lat, lon, 0.0);
        let v = w * (geo::R / w.dot(u).max(1.0)) - origin;
        Vec2::new(v.dot(e) as f32, v.dot(n) as f32)
    };
    // Every tile the airfield may reach into.
    let dlat = reach / geo::R;
    let dlon = dlat / lat0.cos().max(0.05);
    let (t0, t1) = (TileId::at(lat0 + dlat, lon0 - dlon, DATA_ZOOM), TileId::at(lat0 - dlat, lon0 + dlon, DATA_ZOOM));
    let size = 1u32 << DATA_ZOOM;
    let mut tiles = Vec::new();
    let mut x = t0.x;
    loop {
        for y in t0.y..=t1.y {
            tiles.push(TileId::new(DATA_ZOOM, x, y));
        }
        if x == t1.x || tiles.len() > 400 {
            break;
        }
        x = (x + 1) % size;
    }
    struct Piece {
        pts: Vec<Vec2>,
        name: String,
    }
    let mut rw_pieces: Vec<Piece> = Vec::new();
    let mut taxiways: Vec<(Vec<Vec2>, f32)> = Vec::new();
    let mut aprons: Vec<Vec<Vec<Vec2>>> = Vec::new();
    let mut gates: Vec<(Vec2, String)> = Vec::new();
    let mut helipads: Vec<(Vec2, f32)> = Vec::new();
    // Building outlines (bounding box, ring), to keep parked aircraft out of them.
    let mut blds: Vec<(Vec2, Vec2, Vec<Vec2>)> = Vec::new();
    let mut got = 0;
    let tw = taxi_width(apt.kind);
    for id in &tiles {
        let Ok(vt) = src.vector(*id) else { continue };
        got += 1;
        let ext = vt.extent;
        let loc = |p: [f32; 2]| {
            let (la, lo) = id.lat_lon((p[0] / ext) as f64, (p[1] / ext) as f64);
            to_local(la, lo)
        };
        let mine = |p: Vec2| {
            if p.length() > reach as f32 + 600.0 {
                return false;
            }
            let (la, lo, _) = geo::pos_to_lla(origin + e * p.x as f64 + n * p.y as f64);
            db.owner(la.to_degrees(), lo.to_degrees()) == Some(idx)
        };
        for l in &vt.aero_lines {
            for piece in crate::roads::clip(&l.pts, ext) {
                let pts: Vec<Vec2> = piece.iter().map(|p| loc(*p)).collect();
                let len: f32 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
                if len < 2.0 || !mine((pts[0] + pts[pts.len() - 1]) * 0.5) {
                    continue;
                }
                if l.runway {
                    rw_pieces.push(Piece { pts, name: l.name.clone() });
                } else {
                    taxiways.push((pts, tw));
                }
            }
        }
        for (kind, rings) in &vt.aero_polys {
            let rings: Vec<Vec<Vec2>> = rings.iter().map(|r| clip_ring(r, ext).into_iter().map(loc).collect::<Vec<Vec2>>()).filter(|r| r.len() >= 3).collect();
            if rings.is_empty() || !mine(rings[0].iter().copied().sum::<Vec2>() / rings[0].len() as f32) {
                continue;
            }
            match *kind {
                crate::sources::AP_HELIPAD => {
                    let c = rings[0].iter().copied().sum::<Vec2>() / rings[0].len() as f32;
                    let r = rings[0].iter().map(|p| (*p - c).length()).fold(0.0, f32::max);
                    if r > 4.0 && r < 40.0 && !helipads.iter().any(|h: &(Vec2, f32)| (h.0 - c).length() < 10.0) {
                        helipads.push((c, r));
                    }
                }
                crate::sources::AP_RUNWAY | crate::sources::AP_GROUNDS => {}
                _ => aprons.push(rings),
            }
        }
        for b in &vt.buildings {
            if b.min_height > 2.0 {
                continue;
            }
            let ring: Vec<Vec2> = b.rings[0].iter().filter(|p| p[0] > -64.0 && p[1] > -64.0 && p[0] < ext + 64.0 && p[1] < ext + 64.0).map(|p| loc(*p)).collect();
            if ring.len() < 3 || ring[0].length() > reach as f32 + 800.0 {
                continue;
            }
            let (lo, hi) = ring.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
            blds.push((lo, hi, ring));
        }
        for (p, name) in &vt.gates {
            if p[0] >= 0.0 && p[1] >= 0.0 && p[0] < ext && p[1] < ext {
                let q = loc(*p);
                if mine(q) && !gates.iter().any(|g: &(Vec2, String)| (g.0 - q).length() < 6.0) {
                    gates.push((q, name.clone()));
                }
            }
        }
    }
    if got == 0 {
        return Err(anyhow!("no map tiles for {}", apt.ident));
    }

    // ---- runways: the pieces of each one, cut up by the tiles, joined again
    let mut runways: Vec<Runway> = Vec::new();
    let mut used = vec![false; rw_pieces.len()];
    let mut order: Vec<usize> = (0..rw_pieces.len()).collect();
    let plen = |p: &Piece| (p.pts[p.pts.len() - 1] - p.pts[0]).length();
    order.sort_by(|a, b| plen(&rw_pieces[*b]).total_cmp(&plen(&rw_pieces[*a])));
    for &i in &order {
        if used[i] {
            continue;
        }
        used[i] = true;
        let p0 = rw_pieces[i].pts[0];
        let dir = (rw_pieces[i].pts[rw_pieces[i].pts.len() - 1] - p0).normalize_or_zero();
        if dir == Vec2::ZERO {
            continue;
        }
        let (mut lo, mut hi) = (0f32, plen(&rw_pieces[i]));
        let (mut off, mut wsum) = (0f32, 0f32);
        let name = rw_pieces[i].name.clone();
        // Keep taking pieces that continue the line.
        loop {
            let mut grew = false;
            for &j in &order {
                if used[j] {
                    continue;
                }
                let q = &rw_pieces[j];
                if !name.is_empty() && !q.name.is_empty() && q.name != name {
                    continue;
                }
                let dj = (q.pts[q.pts.len() - 1] - q.pts[0]).normalize_or_zero();
                if dj.dot(dir).abs() < 0.997 {
                    continue;
                }
                let along: Vec<f32> = q.pts.iter().map(|p| (*p - p0).dot(dir)).collect();
                let across = q.pts.iter().map(|p| (*p - p0).dot(right_of(dir)).abs()).fold(0.0, f32::max);
                let (a0, a1) = (along.iter().copied().fold(f32::MAX, f32::min), along.iter().copied().fold(f32::MIN, f32::max));
                if across > 14.0 || a0 > hi + 60.0 || a1 < lo - 60.0 {
                    continue;
                }
                used[j] = true;
                grew = true;
                lo = lo.min(a0);
                hi = hi.max(a1);
                for p in &q.pts {
                    off += (*p - p0).dot(right_of(dir));
                    wsum += 1.0;
                }
            }
            if !grew {
                break;
            }
        }
        let shift = right_of(dir) * if wsum > 0.0 { off / (wsum + 2.0) } else { 0.0 };
        let (mut a, mut b) = (p0 + dir * lo + shift, p0 + dir * hi + shift);
        let len = (b - a).length();
        if len < 150.0 {
            continue;
        }
        let mid = (a + b) * 0.5;
        let (la, lo_) = {
            let (la, lo, _) = geo::pos_to_lla(origin + e * mid.x as f64 + n * mid.y as f64);
            (la.to_degrees(), lo.to_degrees())
        };
        let meta = db.runway_meta(la, lo_, bearing(dir), &name);
        // Which end is which: a runway is named for the heading flown along it.
        let mut ids: Vec<String> = name.split('/').map(|s| s.trim().to_ascii_uppercase()).filter(|s| !s.is_empty()).collect();
        if ids.len() != 2 {
            ids = meta.ident.to_vec();
        }
        let mut disp = meta.disp;
        let brg = bearing(dir);
        let turn = |h: f32| { let d = (h - brg).rem_euclid(360.0); d.min(360.0 - d) };
        let (h0, h1) = (ident_heading(&ids[0]), ident_heading(&ids[1]));
        match (h0, h1) {
            (Some(h0), Some(h1)) => {
                // Along a -> b the first name must be the nearer heading.
                if turn(h0) > turn(h1) {
                    ids.swap(0, 1);
                }
            }
            _ => {
                if ids.iter().any(|s| s.is_empty()) {
                    let num = |b: f32| { let k = ((b / 10.0).round() as i32).rem_euclid(36); format!("{:02}", if k == 0 { 36 } else { k }) };
                    ids = vec![num(brg), num(brg + 180.0)];
                } else if dir.y < 0.0 {
                    // "N/S"-style names: keep the map's order along a line drawn west to east or south to north.
                    std::mem::swap(&mut a, &mut b);
                }
            }
        }
        let dir = (b - a).normalize();
        if meta.found && norm_ident(&meta.ident[0]) != norm_ident(&ids[0]) {
            disp.swap(0, 1);
        }
        let lighted = meta.lighted || apt.kind == 2;
        let class = if !meta.paved {
            0
        } else if len >= 1500.0 && (apt.kind == 2 || (apt.kind == 1 && len >= 1800.0)) {
            2
        } else if len >= 1000.0 || apt.kind >= 1 {
            1
        } else {
            0
        };
        let thr = [disp[0].clamp(0.0, len * 0.3), disp[1].clamp(0.0, len * 0.3)];
        runways.push(Runway { a, b, dir, len, width: meta.width, ident: [ids[0].clone(), ids[1].clone()], thr, paved: meta.paved, concrete: meta.concrete, lighted, class });
    }
    // The longest first: it is "the" runway where one is asked for.
    runways.sort_by(|a, b| (b.paved, b.len).partial_cmp(&(a.paved, a.len)).unwrap());
    runways.truncate(12);

    // ---- the ground the airfield lies on
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    let mut grow = |p: Vec2, m: f32| {
        lo = lo.min(p - m);
        hi = hi.max(p + m);
    };
    for r in &runways {
        grow(r.a, 1100.0);
        grow(r.b, 1100.0);
    }
    for p in taxiways.iter().flat_map(|t| t.0.iter()).chain(aprons.iter().flat_map(|a| a[0].iter())) {
        grow(*p, 80.0);
    }
    if lo.x > hi.x {
        (lo, hi) = (Vec2::splat(-300.0), Vec2::splat(300.0));
    }
    let hstep = if (hi - lo).max_element() > 6000.0 { 12.0 } else { 8.0 };
    let hn = (((hi.x - lo.x) / hstep).ceil() as usize + 2, ((hi.y - lo.y) / hstep).ceil() as usize + 2);
    let mut hgrid = vec![apt.elev; hn.0 * hn.1];
    let mut etiles: HashMap<TileId, Option<std::sync::Arc<crate::sources::ElevTile>>> = HashMap::new();
    for j in 0..hn.1 {
        for i in 0..hn.0 {
            let p = lo + Vec2::new(i as f32, j as f32) * hstep;
            let (la, lo_, _) = geo::pos_to_lla(origin + e * p.x as f64 + n * p.y as f64);
            let id = TileId::at(la, lo_, DATA_ZOOM);
            if let Some(t) = etiles.entry(id).or_insert_with(|| src.elev(id).ok()) {
                let (fu, fv) = id.frac(la, lo_);
                hgrid[j * hn.0 + i] = t.sample(fu, fv);
            }
        }
    }
    let mut g = Geo {
        idx,
        origin,
        e,
        n,
        u,
        center: origin,
        elev: apt.elev,
        radius: lo.length().max(hi.length()),
        runways,
        taxiways,
        aprons,
        gates,
        helipads,
        graph: Graph::default(),
        stands: Vec::new(),
        holds: Vec::new(),
        hx: lo,
        hstep,
        hn,
        hgrid,
    };
    g.elev = if g.runways.is_empty() { g.h(Vec2::ZERO) } else { g.runways.iter().map(|r| g.h((r.a + r.b) * 0.5)).sum::<f32>() / g.runways.len() as f32 };
    g.center = g.world(Vec2::ZERO, g.elev as f64);
    build_graph(&mut g);
    find_holds(&mut g);
    find_stands(&mut g, &blds);
    Ok(g)
}

/// The taxi graph: taxiway and runway centrelines, joined wherever they
/// meet. The tiles drop the shared vertex where one straight line crosses
/// another, so crossings are found from the geometry.
fn build_graph(g: &mut Geo) {
    struct Line {
        pts: Vec<Vec2>,
        rwy: Option<u16>,
        /// extra vertices: (segment, t)
        cuts: Vec<(usize, f32)>,
    }
    let mut lines: Vec<Line> = g.taxiways.iter().map(|t| Line { pts: t.0.clone(), rwy: None, cuts: Vec::new() }).collect();
    for (i, r) in g.runways.iter().enumerate() {
        lines.push(Line { pts: vec![r.a, r.b], rwy: Some(i as u16), cuts: Vec::new() });
    }
    // Segments by grid cell.
    const CELL: f32 = 80.0;
    let mut cells: HashMap<(i32, i32), Vec<(u32, u32)>> = HashMap::new();
    for (li, l) in lines.iter().enumerate() {
        for (si, w) in l.pts.windows(2).enumerate() {
            let (lo, hi) = (w[0].min(w[1]) - 3.0, w[0].max(w[1]) + 3.0);
            // A runway is one long segment: walk the cells along it.
            let steps = ((w[1] - w[0]).length() / (CELL * 0.5)).ceil().max(1.0) as i32;
            if steps > 6 {
                let mut seen = std::collections::HashSet::new();
                for k in 0..=steps {
                    let p = w[0] + (w[1] - w[0]) * (k as f32 / steps as f32);
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            let c = ((p.x / CELL).floor() as i32 + dx, (p.y / CELL).floor() as i32 + dy);
                            if seen.insert(c) {
                                cells.entry(c).or_default().push((li as u32, si as u32));
                            }
                        }
                    }
                }
            } else {
                for cy in (lo.y / CELL).floor() as i32..=(hi.y / CELL).floor() as i32 {
                    for cx in (lo.x / CELL).floor() as i32..=(hi.x / CELL).floor() as i32 {
                        cells.entry((cx, cy)).or_default().push((li as u32, si as u32));
                    }
                }
            }
        }
    }
    let mut cuts: Vec<(u32, u32, f32)> = Vec::new();
    for list in cells.values() {
        for x in 0..list.len() {
            for y in x + 1..list.len() {
                let ((la, sa), (lb, sb)) = (list[x], list[y]);
                if la == lb {
                    continue;
                }
                let (a, r) = (lines[la as usize].pts[sa as usize], lines[la as usize].pts[sa as usize + 1] - lines[la as usize].pts[sa as usize]);
                let (c, q) = (lines[lb as usize].pts[sb as usize], lines[lb as usize].pts[sb as usize + 1] - lines[lb as usize].pts[sb as usize]);
                let (lr, lq) = (r.length(), q.length());
                let den = r.perp_dot(q);
                if lr < 1e-3 || lq < 1e-3 || den.abs() < 0.03 * lr * lq {
                    continue;
                }
                let (t, s) = ((c - a).perp_dot(q) / den, (c - a).perp_dot(r) / den);
                const TOL: f32 = 2.5;
                if t < -TOL / lr || t > 1.0 + TOL / lr || s < -TOL / lq || s > 1.0 + TOL / lq {
                    continue;
                }
                cuts.push((la, sa, t.clamp(0.0, 1.0)));
                cuts.push((lb, sb, s.clamp(0.0, 1.0)));
            }
        }
    }
    for (l, s, t) in cuts {
        lines[l as usize].cuts.push((s as usize, t));
    }
    let mut graph = Graph::default();
    let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    const SNAP: f32 = 2.0;
    let mut node = |graph: &mut Graph, p: Vec2| -> u32 {
        let c = ((p.x / SNAP).floor() as i32, (p.y / SNAP).floor() as i32);
        for dy in -1..=1 {
            for dx in -1..=1 {
                for &k in grid.get(&(c.0 + dx, c.1 + dy)).map(|v| &v[..]).unwrap_or(&[]) {
                    if (graph.nodes[k as usize].p - p).length() < SNAP {
                        return k;
                    }
                }
            }
        }
        graph.nodes.push(Node { p, edges: Vec::new() });
        grid.entry(c).or_default().push(graph.nodes.len() as u32 - 1);
        graph.nodes.len() as u32 - 1
    };
    for l in &mut lines {
        l.cuts.sort_by(|a, b| (a.0, a.1).partial_cmp(&(b.0, b.1)).unwrap());
        let mut seq: Vec<Vec2> = Vec::new();
        let mut ci = 0;
        for si in 0..l.pts.len() - 1 {
            seq.push(l.pts[si]);
            while ci < l.cuts.len() && l.cuts[ci].0 == si {
                seq.push(l.pts[si] + (l.pts[si + 1] - l.pts[si]) * l.cuts[ci].1);
                ci += 1;
            }
        }
        seq.push(l.pts[l.pts.len() - 1]);
        let mut prev = node(&mut graph, seq[0]);
        for p in &seq[1..] {
            let cur = node(&mut graph, *p);
            if cur != prev {
                let len = (graph.nodes[cur as usize].p - graph.nodes[prev as usize].p).length();
                let dup = graph.nodes[prev as usize].edges.iter().any(|e| graph.other(*e, prev) == cur);
                if !dup {
                    graph.edges.push(Edge { a: prev, b: cur, len, rwy: l.rwy });
                    let ei = graph.edges.len() as u32 - 1;
                    graph.nodes[prev as usize].edges.push(ei);
                    graph.nodes[cur as usize].edges.push(ei);
                }
                prev = cur;
            }
        }
    }
    g.graph = graph;
}

/// How far from the centreline traffic holds short of a runway.
pub fn hold_dist(r: &Runway) -> f32 {
    r.width * 0.5 + [22.0, 38.0, 55.0][r.class as usize]
}

fn find_holds(g: &mut Geo) {
    let gr = &g.graph;
    let mut holds: Vec<Hold> = Vec::new();
    for (ri, r) in g.runways.iter().enumerate() {
        if !r.paved {
            continue;
        }
        let dh = hold_dist(r);
        let across = |p: Vec2| r.local(p).1.abs();
        for (ni, nd) in gr.nodes.iter().enumerate() {
            if !nd.edges.iter().any(|e| gr.edges[*e as usize].rwy == Some(ri as u16)) {
                continue;
            }
            for &e0 in nd.edges.iter().filter(|e| gr.edges[**e as usize].rwy.is_none()) {
                let (mut cur, mut edge) = (ni as u32, e0);
                for _ in 0..14 {
                    let other = gr.other(edge, cur);
                    let (p0, p1) = (gr.nodes[cur as usize].p, gr.nodes[other as usize].p);
                    let (d0, d1) = (across(p0), across(p1));
                    let seg = (p1 - p0).length().max(1e-3);
                    let toward = (p0 - p1) / seg;
                    let mut put = |p: Vec2| {
                        if !holds.iter().any(|h| (h.p - p).length() < 14.0) {
                            holds.push(Hold { p, dir: toward, rwy: ri as u16, half: taxi_width(g.apt().kind) * 0.5 });
                        }
                    };
                    if d1 >= dh && d1 > d0 {
                        put(p0 + (p1 - p0) * ((dh - d0) / (d1 - d0)).clamp(0.0, 1.0));
                        break;
                    }
                    let deg = gr.nodes[other as usize].edges.len();
                    if deg != 2 {
                        // A junction (the parallel taxiway) inside the hold distance: stop short of it.
                        if d1 >= r.width * 0.5 + 16.0 && deg >= 3 {
                            put(p1 + toward * 12f32.min(seg * 0.5));
                        }
                        break;
                    }
                    let next = gr.nodes[other as usize].edges.iter().copied().find(|e| *e != edge && gr.edges[*e as usize].rwy.is_none());
                    let Some(next) = next else { break };
                    cur = other;
                    edge = next;
                }
            }
        }
    }
    g.holds = holds;
}

/// Wingspan and length of each size of aircraft a stand may take.
pub const SIZE_SPAN: [f32; 5] = [11.0, 16.0, 27.0, 36.0, 62.0];

fn find_stands(g: &mut Geo, blds: &[(Vec2, Vec2, Vec<Vec2>)]) {
    let apt = g.apt();
    let gr = &g.graph;
    let near_apron = |p: Vec2, m: f32| g.aprons.iter().any(|a| in_poly(p, a) || (0..a[0].len()).any(|i| seg_dist(p, a[0][i], a[0][(i + 1) % a[0].len()]) < m));
    let near_rwy = |p: Vec2, m: f32| g.runways.iter().any(|r| { let (s, x) = r.local(p); s > -m && s < r.len + m && x.abs() < r.width * 0.5 + m });
    let top = if apt.kind == 2 { 4 } else if apt.kind == 1 { if apt.longest() >= 2400.0 { 4 } else { 3 } } else if apt.any_paved() && apt.longest() >= 1300.0 { 1 } else { 0 };
    let mut stands: Vec<Stand> = Vec::new();
    // Taxi lanes that end on an apron or at a gate end at a stand.
    for (ni, nd) in gr.nodes.iter().enumerate() {
        if nd.edges.len() != 1 || gr.edges[nd.edges[0] as usize].rwy.is_some() || near_rwy(nd.p, 45.0) {
            continue;
        }
        let gate = g.gates.iter().filter(|gt| (gt.0 - nd.p).length() < 95.0).min_by(|a, b| (a.0 - nd.p).length().total_cmp(&(b.0 - nd.p).length()));
        if gate.is_none() && !near_apron(nd.p, 20.0) && apt.kind == 2 {
            continue;
        }
        // Facing the way the lane arrives, measured over its last stretch.
        let (mut cur, mut edge, mut back) = (ni as u32, nd.edges[0], nd.p);
        for _ in 0..6 {
            let o = gr.other(edge, cur);
            back = gr.nodes[o as usize].p;
            if (back - nd.p).length() > 14.0 || gr.nodes[o as usize].edges.len() != 2 {
                break;
            }
            edge = *gr.nodes[o as usize].edges.iter().find(|e| **e != edge).unwrap();
            cur = o;
        }
        let dir = (nd.p - back).normalize_or_zero();
        if dir == Vec2::ZERO {
            continue;
        }
        stands.push(Stand { pos: nd.p, dir, size: top, gate: gate.map(|g| g.1.clone()).unwrap_or_default(), node: ni as u32, terminal: gate.is_some() });
    }
    // Gates no taxi lane was drawn to: the aircraft stands nose-in before them.
    if apt.kind >= 1 {
        for (gp, name) in &g.gates {
            if stands.iter().any(|s| (s.pos - *gp).length() < 70.0) {
                continue;
            }
            let Some(node) = gr.nearest(*gp, true) else { continue };
            let q = gr.nodes[node as usize].p;
            let d = (q - *gp).length();
            if !(30.0..260.0).contains(&d) {
                continue;
            }
            let out = (q - *gp) / d;
            stands.push(Stand { pos: *gp + out * 14.0, dir: -out, size: top, gate: name.clone(), node, terminal: true });
        }
    }
    // Light aircraft tie-downs in rows on the aprons, where nothing better is known.
    let want = if apt.kind == 2 { 0 } else if apt.kind == 1 { 14 } else { 22 };
    let mut ties = 0;
    let mut aprons: Vec<&Vec<Vec<Vec2>>> = g.aprons.iter().collect();
    let area = |a: &Vec<Vec<Vec2>>| (0..a[0].len()).map(|i| a[0][i].perp_dot(a[0][(i + 1) % a[0].len()])).sum::<f32>().abs() * 0.5;
    aprons.sort_by(|a, b| area(b).total_cmp(&area(a)));
    for a in aprons {
        if ties >= want || area(a) < 900.0 {
            continue;
        }
        let r0 = &a[0];
        let (mut best, mut axis) = (0.0, Vec2::X);
        for i in 0..r0.len() {
            let d = r0[(i + 1) % r0.len()] - r0[i];
            if d.length() > best {
                best = d.length();
                axis = d / best;
            }
        }
        let side = right_of(axis);
        let c = r0.iter().copied().sum::<Vec2>() / r0.len() as f32;
        let ext = r0.iter().map(|p| (*p - c).length()).fold(0.0, f32::max);
        let nrow = (ext / 26.0) as i32;
        'rows: for row in -nrow..=nrow {
            for col in -(ext / 13.5) as i32..=(ext / 13.5) as i32 {
                let p = c + side * (row as f32 * 26.0) + axis * (col as f32 * 13.5);
                let facing = if row.rem_euclid(2) == 0 { side } else { -side };
                let fits = [Vec2::ZERO, axis * 6.0, axis * -6.0, facing * 3.0, facing * -8.0].iter().all(|o| in_poly(p + *o, a));
                if !fits || g.taxiways.iter().any(|t| t.0.windows(2).any(|w| seg_dist(p, w[0], w[1]) < t.1 * 0.5 + 9.0)) || near_rwy(p, 40.0) || stands.iter().any(|s| (s.pos - p).length() < 13.0) {
                    continue;
                }
                let Some(node) = gr.nearest(p, true).or_else(|| gr.nearest(p, false)) else { break 'rows };
                stands.push(Stand { pos: p, dir: facing, size: 0, gate: String::new(), node, terminal: false });
                ties += 1;
                if ties >= want {
                    break 'rows;
                }
            }
        }
    }
    // Nothing parks inside a building or off the airside pavement.
    let in_bld = |p: Vec2| blds.iter().any(|(lo, hi, r)| p.cmpge(*lo).all() && p.cmple(*hi).all() && in_poly(p, std::slice::from_ref(r)));
    let on_pavement = |p: Vec2| near_apron(p, 25.0) || g.taxiways.iter().any(|t| t.0.windows(2).any(|w| seg_dist(p, w[0], w[1]) < t.1 * 0.5 + 25.0));
    stands.retain(|s| {
        let ty = &crate::airmodels::TYPES[s.size as usize];
        let o = s.pos - s.dir * ty.nose;
        let side = right_of(s.dir) * ty.span * 0.45;
        let body = [s.pos - s.dir * 6.0, o, o - s.dir * (ty.len - ty.nose) * 0.9, o + side, o - side];
        on_pavement(o) && !body.iter().any(|p| in_bld(*p))
    });
    // What fits where: by the room to the next stand, and by what the airport takes.
    let pts: Vec<Vec2> = stands.iter().map(|s| s.pos).collect();
    for (i, s) in stands.iter_mut().enumerate() {
        let room = pts.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| (*p - s.pos).length()).fold(120.0, f32::min);
        let fit = (0..5).rev().find(|k| SIZE_SPAN[*k] + 5.0 <= room).unwrap_or(0) as u8;
        s.size = s.size.min(fit);
        if !s.terminal && apt.kind < 2 {
            s.size = s.size.min(2);
        }
        // Most gates take narrow-bodies; a few the wide-bodies; the
        // smaller airports' see regional jets too.
        let h = ((i as u32).wrapping_mul(2654435761) >> 8) as f32 / 16_777_216.0;
        if s.size == 4 && h > if apt.kind == 2 { 0.3 } else { 0.08 } {
            s.size = 3;
        }
        if s.size == 3 && apt.kind < 2 && h < 0.35 {
            s.size = 2;
        }
    }
    stands.truncate(220);
    g.stands = stands;
}

/// Get (or put together) an airfield. Called from worker threads.
pub fn geo_for(src: &Sources, idx: u32) -> Result<std::sync::Arc<Geo>> {
    if let Some(g) = src.fields.lock().get(&idx) {
        return Ok(g.clone());
    }
    let g = std::sync::Arc::new(build_geo(src, idx)?);
    let mut f = src.fields.lock();
    if f.len() > 24 {
        f.clear();
    }
    f.insert(idx, g.clone());
    Ok(g)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `AIRPORT=KSFO cargo test --release geo_report -- --ignored --nocapture`
    /// builds an airfield from the (cached or fetched) map tiles and writes a
    /// plan of it to $AIRPORT_SVG.
    #[test]
    #[ignore]
    fn geo_report() {
        let cache = crate::paths::cache().unwrap_or_else(|| std::path::PathBuf::from("."));
        let src = Sources::new(crate::fetch::Fetcher::new(cache, false), String::new());
        for code in std::env::var("AIRPORT").unwrap_or("KSFO".into()).split(',') {
            let idx = Db::get().find(code).expect("unknown airport");
            let t0 = std::time::Instant::now();
            let g = build_geo(&src, idx).unwrap();
            let a = g.apt();
            println!("{} {} kind {} elev {:.0} m: {} runways, {} taxiways, {} aprons, {} gates, {} nodes, {} edges, {} stands, {} holds, {:.0} ms", a.ident, a.name, a.kind, g.elev, g.runways.len(), g.taxiways.len(), g.aprons.len(), g.gates.len(), g.graph.nodes.len(), g.graph.edges.len(), g.stands.len(), g.holds.len(), t0.elapsed().as_secs_f64() * 1000.0);
            for r in &g.runways {
                let hs: Vec<String> = (0..=10).map(|k| format!("{:.1}", g.h(r.a + (r.b - r.a) * (k as f32 / 10.0)))).collect();
                println!("  {} {:.0} x {:.0} m  brg {:.0}  class {} paved {} lit {} thr {:?}  h {}", r.name(), r.len, r.width, bearing(r.dir), r.class, r.paved, r.lighted, r.thr, hs.join(" "));
            }
            if std::env::var("PROFILE").is_ok() {
                for r in &g.runways {
                    let mut line = String::new();
                    let mut k = 0.0;
                    while k < r.len {
                        let p = r.a + r.dir * k;
                        let (la, lo) = g.lla(p);
                        let id = TileId::at(la, lo, DATA_ZOOM);
                        let (fu, fv) = id.frac(la, lo);
                        let raw = src.elev_raw(id).unwrap().sample(fu, fv);
                        let lev = src.elev(id).unwrap().sample(fu, fv);
                        line += &format!("{:.1}/{:.1} ", raw, lev);
                        k += 40.0;
                    }
                    println!("  {} raw/levelled: {}", r.name(), line);
                }
            }
            if let Ok(pr) = std::env::var("PROBE") {
                let v: Vec<f32> = pr.split(',').filter_map(|x| x.parse().ok()).collect();
                let q = Vec2::new(v[0], v[1]);
                let (la, lo) = g.lla(q);
                println!("  probe at {:.6}, {:.6}", la.to_degrees(), lo.to_degrees());
                for (i, st) in g.stands.iter().enumerate().filter(|(_, st)| (st.pos - q).length() < 150.0) {
                    let nd = g.graph.nodes[st.node as usize].p;
                    println!("  stand {i} pos {:.0?} dir {:.2?} size {} gate '{}' terminal {} node {:.0?} deg {}", st.pos, st.dir, st.size, st.gate, st.terminal, nd, g.graph.nodes[st.node as usize].edges.len());
                }
                for gt in g.gates.iter().filter(|gt| (gt.0 - q).length() < 150.0) {
                    println!("  gate {} at {:.0?}", gt.1, gt.0);
                }
            }
            let mut sizes = [0; 5];
            for s in &g.stands {
                sizes[s.size as usize] += 1;
            }
            println!("  stands by size {sizes:?}, terminal {}", g.stands.iter().filter(|s| s.terminal).count());
            if let Ok(path) = std::env::var("AIRPORT_SVG") {
                let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
                for p in g.runways.iter().flat_map(|r| [r.a, r.b]).chain(g.taxiways.iter().flat_map(|t| t.0.iter().copied())) {
                    lo = lo.min(p - 100.0);
                    hi = hi.max(p + 100.0);
                }
                let k = 1600.0 / (hi - lo).max_element();
                let t = |p: Vec2| format!("{:.1},{:.1}", (p.x - lo.x) * k, (hi.y - p.y) * k);
                let mut s = format!("<svg xmlns='http://www.w3.org/2000/svg' width='{:.0}' height='{:.0}' style='background:#263'>", (hi.x - lo.x) * k, (hi.y - lo.y) * k);
                for a in &g.aprons {
                    let d: Vec<String> = a.iter().map(|r| format!("M{}Z", r.iter().map(|p| t(*p)).collect::<Vec<_>>().join(" L"))).collect();
                    s += &format!("<path d='{}' fill='#999' fill-rule='evenodd'/>", d.join(" "));
                }
                for (pts, w) in &g.taxiways {
                    s += &format!("<polyline points='{}' fill='none' stroke='#777' stroke-width='{:.1}'/>", pts.iter().map(|p| t(*p)).collect::<Vec<_>>().join(" "), w * k);
                }
                for r in &g.runways {
                    s += &format!("<line x1='{}' y1='{}' x2='{}' y2='{}' stroke='{}' stroke-width='{:.1}'/>", (r.a.x - lo.x) * k, (hi.y - r.a.y) * k, (r.b.x - lo.x) * k, (hi.y - r.b.y) * k, if r.paved { "#333" } else { "#6a4" }, r.width * k);
                    for e in 0..2 {
                        let (p, d) = r.end(e);
                        s += &format!("<text x='{}' y='{}' fill='white' font-size='14'>{}</text>", (p.x + d.x * 40.0 - lo.x) * k, (hi.y - p.y - d.y * 40.0) * k, r.ident[e]);
                    }
                }
                for e in &g.graph.edges {
                    s += &format!("<polyline points='{} {}' fill='none' stroke='{}' stroke-width='0.7'/>", t(g.graph.nodes[e.a as usize].p), t(g.graph.nodes[e.b as usize].p), if e.rwy.is_some() { "#fff" } else { "#fd0" });
                }
                for nd in g.graph.nodes.iter().filter(|n| n.edges.len() >= 3) {
                    s += &format!("<circle cx='{}' cy='{}' r='1.6' fill='#0ff'/>", (nd.p.x - lo.x) * k, (hi.y - nd.p.y) * k);
                }
                for h in &g.holds {
                    s += &format!("<circle cx='{}' cy='{}' r='3' fill='none' stroke='#f00' stroke-width='1.5'/>", (h.p.x - lo.x) * k, (hi.y - h.p.y) * k);
                }
                for st in &g.stands {
                    let q = st.pos - st.dir * 30.0 * k.recip().min(1.0);
                    s += &format!("<line x1='{}' y1='{}' x2='{}' y2='{}' stroke='{}' stroke-width='2'/>", (st.pos.x - lo.x) * k, (hi.y - st.pos.y) * k, (q.x - lo.x) * k, (hi.y - q.y) * k, ["#0f0", "#8f0", "#ff0", "#f80", "#f0f"][st.size as usize]);
                }
                for gt in &g.gates {
                    s += &format!("<circle cx='{}' cy='{}' r='1.5' fill='#f8f'/>", (gt.0.x - lo.x) * k, (hi.y - gt.0.y) * k);
                }
                s += "</svg>";
                std::fs::write(path.replace("{}", code), s).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod seam_tests {
    use super::*;
    /// `AIRPORT=LFLJ cargo test --release level_seams -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn level_seams() {
        let cache = crate::paths::cache().unwrap_or_else(|| std::path::PathBuf::from("."));
        let src = Sources::new(crate::fetch::Fetcher::new(cache, false), String::new());
        let code = std::env::var("AIRPORT").unwrap_or("LFLJ".into());
        let a = &Db::get().apts[Db::get().find(&code).unwrap() as usize];
        let (lat0, lon0) = (a.lat.to_radians(), a.lon.to_radians());
        let step = 4.0f64;
        let n = 500;
        let h = |i: i32, j: i32| {
            let lat = lat0 + (j as f64 * step) / geo::R;
            let lon = lon0 + (i as f64 * step) / (geo::R * lat0.cos());
            let id = TileId::at(lat, lon, DATA_ZOOM);
            let (u, v) = id.frac(lat, lon);
            (if std::env::var("RAW").is_ok() { src.elev_raw(id).unwrap().sample(u, v) } else { src.elev(id).unwrap().sample(u, v) }, id)
        };
        let mut worst = Vec::new();
        for j in -n / 2..n / 2 {
            for i in -n / 2..n / 2 {
                let (a0, t0) = h(i, j);
                let (a1, t1) = h(i + 1, j);
                let (a2, t2) = h(i + 2, j);
                // A jump: the step from i to i+1 much bigger than its neighbours'.
                let (d01, d12) = ((a1 - a0).abs(), (a2 - a1).abs());
                if d01 > 1.5 && d01 > 3.0 * d12 {
                    worst.push((d01, i, j, t0 != t1, t1 != t2));
                }
            }
        }
        worst.sort_by(|a, b| b.0.total_cmp(&a.0));
        println!("{} jumps", worst.len());
        for w in worst.iter().take(25) {
            println!("  {:.1} m at ({:.0}, {:.0}) m  tile border {}", w.0, w.1 as f64 * step, w.2 as f64 * step, w.3 || w.4);
        }
    }
}
