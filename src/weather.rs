//! Weather and time of day: the live state behind the F3 panel, the wind the
//! aircraft flies through (gusts, turbulence, thermals and storm drafts), and
//! how far that wind has carried the clouds.
use crate::geo;
use glam::{DVec2, DVec3};

pub struct Weather {
    /// cloud cover, 0..1
    pub clouds: f32,
    /// cloud layer base and thickness, metres
    pub cloud_base: f32,
    pub cloud_depth: f32,
    /// how many cumulonimbus storm cells tower out of the layer, 0..1
    pub storms: f32,
    /// how much mid-level cloud (altocumulus, thickening into altostratus), 0..1
    pub alto: f32,
    /// aerosol haze, as a multiple of a clear day's
    pub haze: f32,
    /// rain falling where the aircraft is, 0..1 (wets the windows, rain.rs;
    /// falling streaks, precip.wgsl): from the rain deck (`deck_rain`) and the
    /// storm cells (storm.rs `local`), only ever under or in their cloud
    pub rain: f32,
    /// rain set by hand (`--rain`, the slider): it brings a rain deck
    /// (nimbostratus) that heavy over the area around the aircraft
    pub rain_set: Option<f32>,
    /// the rain deck now, 0..1 (eases towards `rain_set`; 0 = none)
    pub deck: f32,
    /// centre of the rain deck's area, cloud space (km on the sphere, drifting
    /// with the wind like the storm cells), and the ground height it sits over (m)
    pub deck_at: Option<DVec3>,
    pub deck_ground: Option<f64>,
    /// where to put the rain area's centre next time (`--rain-at KM,BEARING`)
    pub deck_place: Option<(f64, f64)>,
    deck_prev: Option<f32>,
    /// how far the rain deck's area has grown in, 0..1: it shrinks away (the
    /// sky clearing in from the horizon) when the rain is turned off, rather
    /// than lingering as a full overcast while its rain dwindles
    pub deck_fade: f32,
    /// wind 10 m above the ground, m/s, and the compass direction it blows from
    pub wind: f64,
    pub wind_from: f64,
    /// gustiness, 0..1
    pub gusts: f64,
    /// game seconds per real second for the clock (0 stops it)
    pub time_rate: f64,
    /// days since new moon
    pub moon_age: f64,
    /// how far the wind has carried the clouds, planet frame
    pub cloud_off: DVec3,
    /// sine of the sun's elevation where the aircraft is (drives the thermals)
    pub sun: f64,
    /// how far the boundary-layer wind has carried the thermals, planet frame
    pub thermal_off: DVec3,
    /// the storm cells handed to the shaders last frame (see `storm_cells`)
    pub cells: Vec<[[f32; 4]; 2]>,
    /// storm cells placed by hand (`--storm-at`), in cloud space; always handed on first
    pub placed: Vec<[[f32; 4]; 2]>,
    /// storm cells where the weather radar sees them (live weather, see
    /// livewx.rs and radar.rs), fading in and out as the radar updates
    pub live_cells: Vec<LiveCell>,
    /// where some radar sees (made-up storm cells give way to the real ones there)
    pub radar_cover: Option<crate::radar::Cover>,
    /// live weather: where the rain deck should stand (cloud space, km), over
    /// the real rain; None = round the aircraft
    pub deck_goal: Option<DVec3>,
    /// how quickly the rain deck follows the Rain slider (time constant, s)
    pub deck_tau: f64,
    /// real weather (see livewx.rs) sets the sliders: where its report came
    /// from ("EBBR", "Open-Meteo") and the report itself
    pub live: bool,
    pub live_station: String,
    pub live_raw: String,
    /// the clock follows the real time and date (see `real_clock`)
    pub real_time: bool,
    /// live weather reports showers but no thunder (SH without TS, no CB):
    /// its convective cells are showers (with `live`)
    pub showers_only: bool,
    /// sea-level pressure (hPa): the QNH an altimeter is set to (standard
    /// unless the real weather says otherwise)
    pub qnh: f64,
    /// height of the freezing level (m above sea level) from the real
    /// weather's surface temperature; None with hand-set weather (snow.rs)
    pub freezing: Option<f64>,
    /// snow blown up off the ground the real weather reports (METAR BLSN /
    /// DRSN), 0..1; the wind lifts it too without a report (snow.rs `blowing`)
    pub blowing: f32,
    /// surface relative humidity (0..1) from live weather's temperature and
    /// dew point when available; None with hand-set weather
    pub humidity: Option<f32>,
}

/// What kind of convective cell: a shower (towering cumulus or a small
/// cumulonimbus, no anvil, tops 4-7 km, a narrow light-to-moderate rain
/// shaft, no lightning), an ordinary thunderstorm (anvil, occasional
/// lightning) or a severe one (big, heavy rain, frequent lightning), or a
/// supercell (an isolated giant with a rotating updraft, a 100 km anvil, hail
/// and near-continuous lightning). The shaders know only a cell's radius,
/// maturity and top height; a shower is a small young cell whose tower stops
/// below the height where lightning starts (`LIGHTNING_TOP`, storm.rs); a
/// supercell's maturity runs on past 1 (2 = a full one, `supercell`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CellKind {
    Shower,
    Thunder,
    Severe,
    Supercell,
}

impl CellKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "shower" | "showers" | "sh" | "cu" | "tcu" => Some(Self::Shower),
            "thunder" | "ts" | "ordinary" | "storm" => Some(Self::Thunder),
            "severe" | "cb" => Some(Self::Severe),
            "supercell" | "super" | "sc" => Some(Self::Supercell),
            _ => None,
        }
    }
}

/// How much of a supercell a cell is, 0..1 (its maturity past 1; common.wgsl
/// `storm_super`).
pub fn supercell(c: &[[f32; 4]; 2]) -> f64 {
    (c[1][0] as f64 - 1.0).clamp(0.0, 1.0)
}
/// A supercell: its anvil height (m above sea level) and how far its
/// overshooting top climbs above that (m). (The storm ceiling takes it in.)
pub const SUPER_TOP: f32 = STORM_TOP + 400.0;
pub const SUPER_OVERSHOOT: f32 = 2400.0;
const _: () = assert!(SUPER_TOP + SUPER_OVERSHOOT <= STORM_TOP + 700.0 + STORM_OVERSHOOT + 300.0);
/// A supercell's tower radius, km.
pub const SUPER_R: f64 = 14.0;
/// The cell for the shaders, centre `cen` (cloud space km), radius `rad` km,
/// `on` 0..1 how far it has grown in (to 0.5 an ordinary storm growing, then a
/// supercell).
fn super_cell(cen: DVec3, rad: f64, on: f64, seed: f32) -> [[f32; 4]; 2] {
    let s = smoothstep(0.5, 1.0, on) as f32;
    let age = (2.0 * on).min(1.0) + s as f64;
    [[cen.x as f32, cen.y as f32, cen.z as f32, rad as f32], [age as f32, STORM_TOP + (SUPER_TOP - STORM_TOP) * s, STORM_OVERSHOOT * 0.6 + (SUPER_OVERSHOOT - STORM_OVERSHOOT * 0.6) * s, seed]]
}

/// How high a cell grows, as a share of base..anvil (common.wgsl `storm_top`).
pub fn storm_top_share(age: f64) -> f64 {
    0.3 + 0.7 * smoothstep(0.0, 0.55, age)
}

/// A shower cell's "anvil" height (the shaders' sp.y) for its tower to top out
/// at `top` m (above sea level) at maturity `age`, over a cloud base `base`.
fn shower_anvil(base: f64, top: f64, age: f64) -> f32 {
    let top = top.max(base + 2200.0);
    (base + (top - base) / storm_top_share(age)) as f32
}

/// What the air around the aircraft is doing beyond the mean wind.
#[derive(Clone, Copy, Debug)]
pub struct Air {
    /// RMS turbulence, m/s: vertical, and horizontal (larger near the ground)
    pub sigma_w: f64,
    pub sigma_h: f64,
    /// turbulence length scales, m: vertical, horizontal
    pub len_w: f64,
    pub len_h: f64,
    /// steady vertical motion of the air (thermals, storm drafts), m/s, + up
    pub draft: f64,
    /// a storm's outflow along the ground (the gust front), planet frame m/s
    pub gust: DVec3,
}

/// A storm cell from the weather radar, easing towards what the latest
/// radar picture says (so nothing pops as it updates).
#[derive(Clone, Debug)]
pub struct LiveCell {
    /// centre now and where the radar puts it (cloud space, km on the sphere)
    pub at: DVec3,
    pub goal: DVec3,
    /// tower radius (km) and maturity 0..1, now and wanted
    pub rad: f64,
    pub rad_goal: f64,
    pub age: f64,
    pub age_goal: f64,
    /// how far it has grown in, 0..1, and whether it is growing (or dying away)
    pub on: f64,
    pub alive: bool,
    /// turret layout seed and anvil height
    pub seed: f32,
    pub top: f32,
    /// a squall line: its length (km, across the wind), now and wanted; 0 = a lone cell
    pub len: f64,
    pub len_goal: f64,
}

/// Live storm cells grow in or die away over about this long (time constant, s).
const LIVE_FADE: f64 = 35.0;
/// ...and glide to their new place and size over this (s).
const LIVE_GLIDE: f64 = 50.0;

impl Default for Air {
    fn default() -> Self {
        Self { sigma_w: 0.0, sigma_h: 0.0, len_w: 300.0, len_h: 300.0, draft: 0.0, gust: DVec3::ZERO }
    }
}

/// (name, cover, base, thickness, storms, mid-level cloud, haze, wind, gusts)
const PRESETS: &[(&str, f32, f32, f32, f32, f32, f32, f64, f64)] = &[
    ("Clear", 0.05, 1500.0, 4500.0, 0.0, 0.0, 0.6, 2.0, 0.0),
    ("Fair", 0.45, 1500.0, 4500.0, 0.2, 0.25, 1.0, 4.0, 0.15),
    ("Hazy", 0.2, 1800.0, 3000.0, 0.0, 0.2, 8.0, 2.0, 0.05),
    ("Overcast", 0.95, 800.0, 2500.0, 0.0, 0.7, 3.0, 7.0, 0.3),
    ("Windy", 0.55, 1200.0, 4000.0, 0.1, 0.35, 1.5, 14.0, 0.7),
    ("Storms", 0.6, 1300.0, 5000.0, 0.85, 0.5, 1.4, 9.0, 0.5),
];
/// Where storm towers flatten into anvils (tropopause), metres; each cell's
/// anvil sits a little above or below it.
pub const STORM_TOP: f32 = 11500.0;
/// Most a mature storm's core overshoots its anvil, metres (clouds.wgsl keeps the
/// ceiling above anvil + overshoot, so nothing is sliced off flat).
pub const STORM_OVERSHOOT: f32 = 1800.0;
/// Most storm cells handed to the shaders at once (nearest first).
pub const MAX_STORMS: usize = 16;
/// The rain deck's area: radius (km, its lobed edge wanders by ~15 km either
/// way) and how soft its edge is (km). Must match common.wgsl `nimbo_area`.
pub const DECK_R: f64 = 45.0;
const DECK_EDGE: f64 = 10.0;
/// How far inside its centre the rain deck's edge goes when it has shrunk
/// away (km): past the wobble and the soft edge, so nothing is left.
const DECK_GONE: f64 = 26.0;
/// How far below its mean base the rain deck's ragged scud hangs, m.
pub const DECK_RAG: f32 = 320.0;
/// Spacing of candidate storm sites, km.
const STORM_GRID: f64 = 52.0;
/// Spacing of candidate squall-line sites, km (on stormy days one now and then
/// stands in for the scattered cells around it).
const SQUALL_GRID: f64 = 400.0;
/// A squall line's tower radius (km): its depth across is about 3x this.
pub const SQUALL_R: f64 = 9.5;
/// Share of candidate storm sites that hold a supercell on the stormiest days,
/// and how far round one no other cell stands, km.
const SUPER_P: f64 = 0.010;
const SUPER_CLEAR: f64 = 68.0;
/// Storm cells are found out to this distance from the camera, km.
const STORM_RANGE: f64 = 300.0;

fn shash(x: u32) -> u32 {
    let h = x.wrapping_mul(747796405).wrapping_add(2891336453);
    let h = ((h >> ((h >> 28) + 4)) ^ h).wrapping_mul(277803737);
    (h >> 22) ^ h
}
fn unit(h: u32) -> f64 {
    (h & 0xffff) as f64 / 65535.0
}
/// Smooth 0..1 value noise over a 3D lattice, for where storms cluster.
fn value_noise(p: DVec3) -> f64 {
    let c = p.floor();
    let f = p - c;
    let f = f * f * (DVec3::splat(3.0) - 2.0 * f);
    let at = |i: i32, j: i32, k: i32| {
        let x = c.x as i32 + i;
        let y = c.y as i32 + j;
        let z = c.z as i32 + k;
        unit(shash((x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663) ^ (z as u32).wrapping_mul(83492791)))
    };
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(at(0, 0, 0), at(1, 0, 0), f.x);
    let x10 = lerp(at(0, 1, 0), at(1, 1, 0), f.x);
    let x01 = lerp(at(0, 0, 1), at(1, 0, 1), f.x);
    let x11 = lerp(at(0, 1, 1), at(1, 1, 1), f.x);
    lerp(lerp(x00, x10, f.y), lerp(x01, x11, f.y), f.z)
}
fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
/// A squall line for the shaders: like a storm cell (centre in cloud space km,
/// tower radius km; maturity, anvil height m, overshoot m), with its length in
/// km as the whole part of the 8th value (the seed its fraction).
fn squall_cell(cen: DVec3, rad: f64, len_km: f64, age: f64, top: f32, seed: f64) -> [[f32; 4]; 2] {
    let over = STORM_OVERSHOOT * 0.8 * smoothstep(0.6, 0.95, age) as f32;
    let len = len_km.round().max(1.0);
    [[cen.x as f32, cen.y as f32, cen.z as f32, rad as f32], [age as f32, top, over, (len + seed.clamp(0.0, 0.999)) as f32]]
}
/// The cube face (0..6) whose direction dominates `q`, with its normal and in-plane axes.
fn cube_face(q: DVec3) -> (u32, DVec3, DVec3, DVec3) {
    let a = q.abs();
    if a.x >= a.y && a.x >= a.z {
        ((q.x < 0.0) as u32, DVec3::X * q.x.signum(), DVec3::Y, DVec3::Z)
    } else if a.y >= a.z {
        (2 + (q.y < 0.0) as u32, DVec3::Y * q.y.signum(), DVec3::Z, DVec3::X)
    } else {
        (4 + (q.z < 0.0) as u32, DVec3::Z * q.z.signum(), DVec3::X, DVec3::Y)
    }
}

/// Candidate sites on a jittered `grid` (km) on each face of a cube projected
/// onto the planet, within `range` km of `q` (cloud space): f(centre,
/// distance, hash). They stay put, and each belongs to exactly one face.
fn sites(q: DVec3, grid: f64, range: f64, mut f: impl FnMut(DVec3, f64, u32)) {
    let reach = (range / grid).ceil() as i32 + 1;
    for face in 0..6u32 {
        let n = [DVec3::X, -DVec3::X, DVec3::Y, -DVec3::Y, DVec3::Z, -DVec3::Z][face as usize];
        if q.normalize().dot(n) < 0.45 {
            continue;
        }
        let (_, _, u, v) = cube_face(n);
        let g = DVec2::new(q.dot(u), q.dot(v)) * (6371.0 / q.dot(n)) / grid;
        // (A coarser grid gets its own hashes.)
        let salt = if grid == STORM_GRID { 0 } else { (grid as u32).wrapping_mul(2654435761) };
        for j in -reach..=reach {
            for i in -reach..=reach {
                let (cx, cy) = (g.x.floor() as i32 + i, g.y.floor() as i32 + j);
                let h0 = shash((cx as u32).wrapping_mul(73856093) ^ (cy as u32).wrapping_mul(19349663) ^ face.wrapping_mul(83492791) ^ salt);
                let h1 = shash(h0);
                let jit = DVec2::new(unit(h1), unit(h1 >> 16));
                let cg = (DVec2::new(cx as f64, cy as f64) + 0.2 + 0.6 * jit) * grid;
                let cen = (n * 6371.0 + u * cg.x + v * cg.y).normalize() * 6371.0;
                if cube_face(cen).0 != face {
                    continue;
                }
                let dist = cen.distance(q);
                if dist <= range {
                    f(cen, dist, h0);
                }
            }
        }
    }
}

impl Weather {
    pub fn new(clouds: f32) -> Self {
        // A light west-south-westerly, as the smoke always drifted before there was a slider for it.
        Self { clouds, cloud_base: 1500.0, cloud_depth: 4500.0, storms: 0.2, alto: 0.25, haze: 1.0, rain: 0.0, rain_set: None, deck: 0.0, deck_at: None, deck_ground: None, deck_place: None, deck_prev: None, deck_fade: 0.0, wind: 4.0, wind_from: 248.0, gusts: 0.15, time_rate: 1.0, moon_age: 11.5, cloud_off: DVec3::ZERO, sun: 0.5, thermal_off: DVec3::ZERO, cells: Vec::new(), placed: Vec::new(), live_cells: Vec::new(), radar_cover: None, deck_goal: None, deck_tau: 6.0, live: false, live_station: String::new(), live_raw: String::new(), real_time: false, showers_only: false, qnh: 1013.25, freezing: None, blowing: 0.0, humidity: None }
    }

    /// The rain deck's mean base and top, metres above sea level: the heavier
    /// the rain, the lower and thicker (darker) the deck.
    pub fn deck_layer(&self) -> Option<(f32, f32)> {
        if self.deck <= 0.0 || self.deck_at.is_none() {
            return None;
        }
        let r = self.deck;
        let base = self.deck_ground.unwrap_or(0.0) as f32 + 1100.0 + (450.0 - 1100.0) * r;
        Some((base, base + 1800.0 + 1600.0 * r))
    }

    /// The cloud layer the shaders draw, metres: base (the lowest any cloud
    /// reaches) and thickness, taking in the rain deck.
    pub fn base(&self) -> f32 {
        match self.deck_layer() {
            Some((b, _)) => self.cloud_base.min(b - DECK_RAG),
            None => self.cloud_base,
        }
    }
    pub fn depth(&self) -> f32 {
        let top = self.cloud_base + self.cloud_depth;
        match self.deck_layer() {
            Some((_, t)) => top.max(t + 100.0) - self.base(),
            None => top - self.base(),
        }
    }

    /// The rain deck's tangent axes at its centre (Globals.nimbo).
    fn deck_axes(c: DVec3) -> (DVec3, DVec3) {
        let cn = c.normalize();
        let a = if cn.z.abs() > 0.9 { DVec3::X } else { DVec3::Z };
        let ax = cn.cross(a).normalize();
        (ax, cn.cross(ax))
    }

    /// How far into the rain deck's area a point (planet frame) is, 0..1: a
    /// lobed patch some 90 km across, soft at the edge. Must match common.wgsl
    /// `nimbo_area`.
    pub fn deck_area(&self, pos: DVec3) -> f64 {
        let Some(c) = self.deck_at else { return 0.0 };
        if self.deck <= 0.0 {
            return 0.0;
        }
        let d = pos.normalize() * 6371.0 + self.cloud_off / 1000.0 - c;
        let (ax, ay) = Self::deck_axes(c);
        let (x, y) = (d.dot(ax), d.dot(ay));
        let wob = 9.0 * (0.083 * x + 1.7 * (0.051 * y).sin()).sin() + 6.0 * (0.13 * y + 0.9 + 0.8 * (0.07 * x).sin()).sin();
        let r = self.deck_radius();
        smoothstep(r, r - DECK_EDGE, x.hypot(y) + wob)
    }

    /// The rain deck's area radius now (km): full size once grown in,
    /// shrinking to nothing as it fades out (`deck_fade`).
    pub fn deck_radius(&self) -> f64 {
        -DECK_GONE + (DECK_R + DECK_GONE) * self.deck_fade as f64
    }

    /// Rain from the deck at a point, 0..1: under it and in it (thinning out
    /// higher up, where it is snow and ice), none above its top or outside it.
    pub fn deck_rain(&self, pos: DVec3) -> f64 {
        let Some((base, top)) = self.deck_layer() else { return 0.0 };
        let a = self.deck_area(pos);
        if a <= 0.0 {
            return 0.0;
        }
        let (base, top) = (base as f64, top as f64);
        let d = top - base;
        let alt = pos.length() - geo::R;
        let v = (1.0 - 0.5 * smoothstep(base, top, alt)) * (1.0 - smoothstep(top - 0.25 * d, top - 0.12 * d, alt));
        self.deck as f64 * smoothstep(0.3, 0.8, a) * v
    }

    /// How deep in the rain deck's gloom a point is, 0..1 (the ground and
    /// air under it get only the dull light that diffuses through). Must match
    /// common.wgsl `nimbo_shade`.
    pub fn deck_shade(&self, pos: DVec3) -> f64 {
        let Some((base, top)) = self.deck_layer() else { return 0.0 };
        let alt = pos.length() - geo::R;
        let (base, top) = (base as f64, top as f64);
        (0.6 + 0.4 * self.deck as f64) * self.deck_area(pos) * (1.0 - smoothstep(base, top - 0.15 * (top - base), alt))
    }

    /// Rain at a point from any cloud above it: the deck or a storm cell.
    pub fn rain_at(&self, pos: DVec3, agl: f64, t: f64) -> f64 {
        self.deck_rain(pos).max(crate::storm::local(self, pos, agl, t).rain).min(1.0)
    }

    /// The rain deck follows the slider (over a few seconds); its area is
    /// centred on `pos` (planet frame) when it first comes, or when the
    /// slider is moved outside it. `ground` = the terrain height under the
    /// aircraft, when known (the deck's base is set above it).
    pub fn step_deck(&mut self, dt: f64, pos: DVec3, ground: Option<f64>) {
        let mut target = self.rain_set.unwrap_or(0.0);
        // The deck grows in and shrinks away over this long (s), linearly so
        // it is surely gone in the end.
        let fade_time = 2.5 * self.deck_tau;
        // Live weather puts the deck over the real rain: a small shift glides
        // it there, a big one fades it out and back in at the new place.
        if let (Some(g), Some(at)) = (self.deck_goal, self.deck_at) {
            if g.distance(at) > 35.0 {
                target = 0.0;
            } else {
                self.deck_at = Some(at + (g - at) * (dt / LIVE_GLIDE).min(1.0));
            }
        }
        if let Some(g) = ground {
            self.deck_ground = Some(match self.deck_ground {
                Some(o) => o + (g - o) * (dt / 90.0).min(1.0),
                None => g,
            });
        }
        let moved = self.deck_prev != self.rain_set;
        self.deck_prev = self.rain_set;
        if target > 0.0 && (self.deck_at.is_none() || (moved && self.deck_goal.is_none() && self.deck_area(pos) < 0.5)) {
            if let Some(g) = self.deck_goal {
                self.deck_at = Some(g);
                // (at once only on a fresh start; otherwise it closes in)
                if self.deck <= 0.0 && dt <= 0.0 {
                    (self.deck, self.deck_fade) = (target, 1.0);
                }
                self.deck += (target - self.deck) * (dt / self.deck_tau).min(1.0) as f32;
                self.deck_fade = (self.deck_fade + (dt / fade_time) as f32).min(1.0);
                return;
            }
            let p = match self.deck_place.take() {
                Some((km, bearing)) => {
                    let (e, n, up) = geo::enu(pos);
                    let b = bearing.to_radians();
                    let a = km / 6371.0;
                    up * a.cos() + (e * b.sin() + n * b.cos()) * a.sin()
                }
                None => pos.normalize(),
            };
            self.deck_at = Some(p * 6371.0 + self.cloud_off / 1000.0);
            if self.deck <= 0.0 {
                self.deck = target;
            }
            // (all there at once on a fresh start; otherwise it closes in)
            if dt <= 0.0 {
                self.deck_fade = 1.0;
            }
        }
        if self.deck_at.is_none() {
            return;
        }
        if target > 0.0 {
            // A heavier or lighter rain: the deck eases to it, growing in if new.
            self.deck += (target - self.deck) * (dt / self.deck_tau).min(1.0) as f32;
            self.deck_fade = (self.deck_fade + (dt / fade_time) as f32).min(1.0);
        } else {
            // Turned off: the deck keeps its look and shrinks away (the rain
            // stops as its edge goes by), then it is gone.
            self.deck_fade -= (dt / fade_time) as f32;
            if self.deck_fade <= 0.0 {
                (self.deck, self.deck_at, self.deck_fade) = (0.0, None, 0.0);
            }
        }
    }

    /// The aircraft was put down somewhere new: a rain deck set by hand
    /// moves there if it was left behind (and learns the new ground height).
    pub fn deck_moved(&mut self, pos: DVec3) {
        if self.deck_at.is_some() && self.deck_area(pos) < 0.5 {
            (self.deck_at, self.deck_ground) = (None, None);
        }
    }

    /// The rain deck for the shaders (Globals.nimbo): centre (cloud space, km)
    /// + amount (0 = none); east-ish axis + mean base (m); north-ish axis +
    /// top (m); how dark it is under it, how far below the ordinary cloud's base
    /// the layer now starts (m), how far its scud hangs (m), area radius (km).
    pub fn deck_globals(&self) -> [[f32; 4]; 4] {
        let (Some(c), Some((base, top))) = (self.deck_at, self.deck_layer()) else { return [[0.0; 4]; 4] };
        let (ax, ay) = Self::deck_axes(c);
        let v = |v: DVec3, w: f32| [v.x as f32, v.y as f32, v.z as f32, w];
        [v(c, self.deck), v(ax, base), v(ay, top), [0.6 + 0.4 * self.deck, (self.cloud_base - self.base()).max(0.0), DECK_RAG, self.deck_radius() as f32]]
    }

    /// Top of the cloud layer including any storm towers, metres.
    pub fn cloud_ceiling(&self) -> f32 {
        let top = self.base() + self.depth();
        if self.storms > 0.0 || !self.placed.is_empty() || !self.live_cells.is_empty() { top.max(STORM_TOP + 700.0 + STORM_OVERSHOOT + 300.0).max(top + 1500.0) } else { top }
    }

    /// The weather radar's latest storm cells: (centre in cloud space km,
    /// tower radius km, maturity 0..1). Each is matched to a cell already
    /// standing near there, which glides to it; cells no longer seen die away
    /// and new ones grow in. `jump`: a fresh start, show them as they are.
    /// A squall line comes as one cell with its length (km, the 4th value; 0 =
    /// a lone cell); lines only match lines.
    pub fn set_live_cells(&mut self, cells: &[(DVec3, f64, f64, f64)], jump: bool) {
        if jump {
            self.live_cells.clear();
        }
        let mut used = vec![false; self.live_cells.len()];
        // Biggest first, so a big cell keeps its old self rather than a small neighbour taking it.
        let mut order: Vec<usize> = (0..cells.len()).collect();
        order.sort_by(|&a, &b| cells[b].1.total_cmp(&cells[a].1));
        let mut fresh = Vec::new();
        for &k in &order {
            let (p, rad, age, len) = cells[k];
            let near = self
                .live_cells
                .iter()
                .enumerate()
                .filter(|(i, c)| !used[*i] && c.alive && (c.len_goal > 0.0) == (len > 0.0))
                .map(|(i, c)| (i, c.goal.distance(p)))
                .filter(|&(i, d)| d < (1.3 * (rad + self.live_cells[i].rad_goal) + 0.3 * len).max(10.0))
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match near {
                Some((i, _)) => {
                    used[i] = true;
                    let c = &mut self.live_cells[i];
                    (c.goal, c.rad_goal, c.age_goal, c.len_goal) = (p, rad, age, len);
                }
                None => {
                    let h = shash((p.x * 37.0) as i32 as u32 ^ ((p.y * 41.0) as i32 as u32).wrapping_mul(19349663) ^ ((p.z * 43.0) as i32 as u32).wrapping_mul(83492791));
                    let on = if jump { 1.0 } else { 0.0 };
                    fresh.push(LiveCell { at: p, goal: p, rad, rad_goal: rad, age, age_goal: age, on, alive: true, seed: unit(h) as f32 * 0.999, top: STORM_TOP + 700.0 * (unit(h >> 16) as f32 * 2.0 - 1.0), len, len_goal: len });
                }
            }
        }
        for (c, u) in self.live_cells.iter_mut().zip(used) {
            c.alive &= u;
        }
        self.live_cells.extend(fresh);
    }

    /// Let the radar's cells grow, die away and glide (dt s; 0 = at once).
    pub fn step_live(&mut self, dt: f64) {
        let (kf, kg) = if dt <= 0.0 { (1.0, 1.0) } else { (1.0 - (-dt / LIVE_FADE).exp(), 1.0 - (-dt / LIVE_GLIDE).exp()) };
        for c in &mut self.live_cells {
            let to = if c.alive { 1.0 } else { 0.0 };
            // (a smooth start and a clean end: never quite reaching 0 otherwise)
            c.on = (c.on + (to - c.on) * kf + (to * 2.0 - 1.0) * 0.002 * dt.max(0.0)).clamp(0.0, 1.0);
            c.at += (c.goal - c.at) * kg;
            c.rad += (c.rad_goal - c.rad) * kg;
            c.age += (c.age_goal - c.age) * kg;
            c.len += (c.len_goal - c.len) * kg;
        }
        self.live_cells.retain(|c| c.alive || c.on > 0.0);
    }

    /// Put a convective cell `dist_km` from `pos` (planet frame) towards
    /// compass `bearing` (degrees), its tower `rad_km` in radius (8 = a giant
    /// supercell). None = a mature cumulonimbus as big as asked; a shower is a
    /// young tower topping out around 6 km. It drifts with the cloud field from
    /// there on. For testing: `--storm-at KM,BEARING[,R[,KIND]]`.
    pub fn place_storm(&mut self, pos: DVec3, dist_km: f64, bearing: f64, rad_km: f64, kind: Option<CellKind>) {
        let (e, n, up) = geo::enu(pos);
        let b = bearing.to_radians();
        let dir = e * b.sin() + n * b.cos();
        let a = dist_km / 6371.0;
        let p = up * a.cos() + dir * a.sin();
        let cen = (p * 6371.0 + self.cloud_off / 1000.0).normalize() * 6371.0;
        let size = smoothstep(4.0, 12.0, rad_km);
        // Each placed storm gets its own seed (turret layout), so a line of them is not
        // one cloud repeated.
        let seed = (0.37 + dist_km * 0.1373 + bearing * 0.0619 + rad_km * 0.271).fract() as f32;
        if kind == Some(CellKind::Supercell) {
            self.placed.push(super_cell(cen, rad_km, 1.0, seed));
            return;
        }
        let (age, top, over) = match kind {
            Some(CellKind::Shower) => (0.3, shower_anvil(self.base() as f64, 6000.0, 0.3), 0.0),
            Some(CellKind::Thunder) => (0.75, STORM_TOP, 0.4 * STORM_OVERSHOOT * size as f32),
            _ => (1.0, STORM_TOP + 500.0, 600.0 + (STORM_OVERSHOOT - 600.0) * size as f32),
        };
        self.placed.push([[cen.x as f32, cen.y as f32, cen.z as f32, rad_km as f32], [age, top, over, seed]]);
    }

    /// Put a squall line's leading edge `dist_km` from `pos` towards compass `bearing`: a
    /// row of merged mature cumulonimbus `len_km` long lying across the wind,
    /// with a shared anvil streaming ahead, a shelf cloud along its leading
    /// edge, heavy rain right behind that and lighter rain trailing behind
    /// (common.wgsl storm_sd, storm.rs). It drifts with the cloud field. For
    /// testing: `--squall-at KM,BEARING[,LENGTH_KM]`.
    pub fn place_squall(&mut self, pos: DVec3, dist_km: f64, bearing: f64, len_km: f64) {
        let (e, n, up) = geo::enu(pos);
        let b = bearing.to_radians();
        let dir = e * b.sin() + n * b.cos();
        // (`dist_km` is to its leading edge, the gust front under the shelf cloud.)
        let a = (dist_km + crate::storm::SHELF_F * SQUALL_R) / 6371.0;
        let p = up * a.cos() + dir * a.sin();
        let cen = (p * 6371.0 + self.cloud_off / 1000.0).normalize() * 6371.0;
        let seed = (0.37 + dist_km * 0.1373 + bearing * 0.0619 + len_km * 0.271).fract() * 0.999;
        self.placed.push(squall_cell(cen, SQUALL_R, len_km, 1.0, STORM_TOP + 300.0, seed));
    }

    /// Set a named preset (case-insensitive, e.g. "storms"); false if there is none.
    pub fn preset(&mut self, name: &str) -> bool {
        let Some(&(_, c, b, d, s, a, h, w, g)) = PRESETS.iter().find(|p| p.0.eq_ignore_ascii_case(name) || p.0.to_lowercase().trim_end_matches('s') == name.to_lowercase().trim_end_matches('s')) else { return false };
        (self.clouds, self.cloud_base, self.cloud_depth, self.storms, self.alto, self.haze, self.wind, self.gusts) = (c, b, d, s, a, h, w, g);
        true
    }

    /// Thunderstorms all round `pos` (`--weather storm`): the Storms preset, a
    /// big mature cell a few km ahead along `heading` with its rain shaft across
    /// the way, and more around it.
    pub fn storm_field(&mut self, pos: DVec3, heading: f64) {
        self.preset("Storms");
        self.place_storm(pos, 26.0, heading, 10.5, None);
        self.place_storm(pos, 46.0, heading + 48.0, 9.0, None);
        self.place_storm(pos, 58.0, heading - 40.0, 12.5, None);
        self.place_storm(pos, 72.0, heading + 140.0, 8.5, None);
        self.place_storm(pos, 90.0, heading - 115.0, 10.5, None);
    }

    /// Storm cells (cumulonimbus) near the camera, nearest first, two vec4s each for
    /// the shaders: centre in cloud space (km on the sphere, planet frame, moving with
    /// the wind like the rest of the cloud field) and tower radius in km; maturity 0..1.
    /// Candidate sites sit on a jittered grid on each face of a cube projected onto
    /// the planet, so they stay put; a hash decides which are active, their size and
    /// age, and a coarse noise makes them cluster.
    pub fn storm_cells(&self, cam: DVec3) -> Vec<[[f32; 4]; 2]> {
        let mut out: Vec<(f64, [[f32; 4]; 2])> = Vec::new();
        let q = cam.normalize() * 6371.0 + self.cloud_off / 1000.0;
        // The weather radar's cells, where it sees them.
        for c in &self.live_cells {
            let dist = c.at.distance(q);
            if c.on <= 0.0 || dist > STORM_RANGE {
                continue;
            }
            let rad = c.rad * (0.6 + 0.4 * c.on);
            let age = c.age * c.on;
            if c.len > 0.0 {
                out.push(((dist - 0.5 * c.len).max(0.0), squall_cell(c.at, rad, c.len * (0.6 + 0.4 * c.on), age.max(0.9 * c.on), c.top, c.seed as f64)));
                continue;
            }
            // An isolated, very intense, large echo: a supercell (radar.rs).
            if c.age > 1.0 {
                out.push((dist, super_cell(c.at, rad, 0.5 * age, c.seed)));
                continue;
            }
            // Weak echoes are showers; where the report has showers but no
            // thunder, all but the strongest (~47 dBZ and up) are.
            let shower = c.age < 0.4 || (self.live && self.showers_only && c.age < 0.8);
            let (age, top) = if shower { let a = age.min(0.35); (a, shower_anvil(self.base() as f64, 4500.0 + 2500.0 * c.seed as f64, a)) } else { (age, c.top) };
            let over = STORM_OVERSHOOT * (0.25 + 0.75 * smoothstep(4.5, 13.0, rad) as f32) * smoothstep(0.6, 0.95, age) as f32;
            out.push((dist, [[c.at.x as f32, c.at.y as f32, c.at.z as f32, rad as f32], [age as f32, top, over, c.seed]]));
        }
        // (Showers are many and small: a low setting still has a fair few.)
        let st = self.storms as f64;
        let prob_amount = (0.19 + 0.39 * st) * (st * 8.0).min(1.0) * smoothstep(0.12, 0.45, self.clouds as f64);
        // Share of cells that are showers: nearly all on a showery day, a few
        // on a stormy one (and all where live weather says no thunder).
        let p_shower = if self.live && self.showers_only { 1.0 } else { (0.95 - 0.85 * st * st.sqrt()).clamp(0.1, 0.95) };
        if prob_amount <= 0.0 {
            out.sort_by(|a, b| a.0.total_cmp(&b.0));
            return self.placed.iter().copied().chain(out.into_iter().map(|c| c.1)).take(MAX_STORMS).collect();
        }
        // Where a weather radar sees, its cells stand instead of made-up ones
        // (fading across the edge of its coverage as the cloud drifts).
        let seen_at = |cen: DVec3| {
            self.radar_cover.as_ref().map_or(0.0, |cv| {
                let (la, lo, _) = geo::pos_to_lla(cen - self.cloud_off / 1000.0);
                cv.at(la.to_degrees(), lo.to_degrees())
            })
        };
        // Squall lines: on stormy days, now and then a line of storms 60-150 km
        // long instead of the scattered cells round it (centre, half-length km).
        let mut lines: Vec<(DVec3, f64)> = Vec::new();
        let p_line = 0.4 * smoothstep(0.55, 0.95, st) * smoothstep(0.3, 0.5, self.clouds as f64) * if self.live && self.showers_only { 0.0 } else { 1.0 };
        if p_line > 0.0 {
            sites(q, SQUALL_GRID, STORM_RANGE + 75.0, |cen, dist, h0| {
                let on = smoothstep(unit(h0) - 0.04, unit(h0) + 0.04, p_line) * (1.0 - seen_at(cen));
                if on <= 0.0 {
                    return;
                }
                let h1 = shash(h0 ^ 0x27d4eb2d);
                let len = (70.0 + 80.0 * unit(h1)) * (0.6 + 0.4 * on);
                let rad = (SQUALL_R - 1.2 + 2.4 * unit(h1 >> 16)) * (0.6 + 0.4 * on);
                let h2 = shash(h1);
                let top = STORM_TOP + 700.0 * (unit(h2) as f32 * 2.0 - 1.0);
                lines.push((cen, 0.5 * len));
                out.push(((dist - 0.5 * len).max(0.0), squall_cell(cen, rad, len, on, top, unit(h2 >> 16))));
            });
        }
        // Supercells: on the stormiest days, now and then one isolated giant
        // (the cells around it keep their distance).
        let mut supers: Vec<DVec3> = Vec::new();
        let p_super = SUPER_P * smoothstep(0.7, 1.0, st) * smoothstep(0.3, 0.5, self.clouds as f64) * if self.live && self.showers_only { 0.0 } else { 1.0 };
        if p_super > 0.0 {
            sites(q, STORM_GRID, STORM_RANGE + SUPER_CLEAR, |cen, dist, h0| {
                let hs = shash(h0 ^ 0x68e3_1da4);
                let clear = lines.iter().map(|&(c, l2)| smoothstep(l2 + 30.0, l2 + 65.0, c.distance(cen))).fold(1.0, f64::min);
                let on = smoothstep(unit(hs) - 0.25 * SUPER_P, unit(hs) + 0.25 * SUPER_P, p_super) * (1.0 - seen_at(cen)) * clear;
                if on <= 0.0 {
                    return;
                }
                supers.push(cen);
                if dist <= STORM_RANGE {
                    let rad = (SUPER_R - 1.5 + 3.0 * unit(hs >> 16)) * (0.6 + 0.4 * on);
                    out.push((dist, super_cell(cen, rad, on, unit(shash(hs)) as f32 * 0.999)));
                }
            });
        }
        sites(q, STORM_GRID, STORM_RANGE, |cen, dist, h0| {
            // (A supercell stands here, or near enough to keep this one off.)
            let lone = supers.iter().map(|s| smoothstep(0.6 * SUPER_CLEAR, SUPER_CLEAR, s.distance(cen))).fold(1.0, f64::min);
            if lone <= 0.0 {
                return;
            }
            let h2 = shash(shash(h0));
            let region = value_noise(cen / 320.0 + DVec3::new(0.41, 0.07, 0.66));
            let prob = prob_amount * smoothstep(0.2, 0.6, region + self.storms as f64 * 0.25);
            // Fades in rather than pops as the slider or the region changes.
            let r0 = unit(h0);
            // (A squall line takes the place of the cells round it.)
            let clear = lines.iter().map(|&(c, l2)| smoothstep(l2 + 30.0, l2 + 65.0, c.distance(cen))).fold(1.0, f64::min);
            let on = smoothstep(r0 - 0.04, r0 + 0.04, prob) * (1.0 - seen_at(cen)) * clear * lone;
            if on <= 0.0 {
                return;
            }
            // A cell fading out shrinks into a smaller, younger (lower) tower.
            let us = unit(h2);
            let u = unit(h2 >> 16);
            let h3 = shash(h2 ^ 0x9e3779b9);
            let seed = unit(h3 >> 16) as f32;
            if unit(shash(h3 ^ 0x5bd1e995)) < p_shower {
                // A shower: a towering cumulus 5-10 km across, no anvil, topping
                // out at 4.2-6.6 km (higher on stormier days).
                let rad = (2.4 + 2.4 * us) * (0.6 + 0.4 * on);
                let age = (0.14 + 0.24 * u) * on;
                let top = 4200.0 + 2400.0 * (unit(h3) * (0.6 + 0.4 * st));
                out.push((dist, [[cen.x as f32, cen.y as f32, cen.z as f32, rad as f32], [age as f32, shower_anvil(self.base() as f64, top, age), 0.0, seed]]));
                return;
            }
            // Thunderstorms. Broad convective complexes 10-20 km across, with a
            // tail of giant multicell storms 20-30 km wide.
            let giant = smoothstep(0.76, 1.0, us) * (0.5 + 0.5 * st);
            let rad = (5.2 + 4.6 * us + 5.5 * giant * giant) * (0.6 + 0.4 * on);
            // Mature (with anvils); stormier days have more of the older,
            // bigger ones; giants always are.
            let age = (0.5 + 0.5 * (u + (1.0 - (1.0 - u) * (1.0 - u) - u) * st)).max(giant * 1.6).min(1.0) * on;
            // Anvil height (the tropopause varies a little) and how far the
            // strongest updraft overshoots it: big, mature cells the most.
            let top = STORM_TOP + 700.0 * (unit(h3) as f32 * 2.0 - 1.0);
            let over = STORM_OVERSHOOT * (0.25 + 0.75 * smoothstep(4.5, 13.0, rad) as f32) * smoothstep(0.6, 0.95, age) as f32;
            out.push((dist, [[cen.x as f32, cen.y as f32, cen.z as f32, rad as f32], [age as f32, top, over, seed]]));
        });
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.placed.iter().copied().chain(out.into_iter().map(|c| c.1)).take(MAX_STORMS).collect()
    }

    /// Visibility at sea level through the haze (Koschmieder, green light), km.
    pub fn visibility_km(&self) -> f64 {
        3.912 / (13.558e-6 + (3.996e-6 + 4.4e-6) * self.haze as f64) / 1000.0
    }

    /// The steady wind (where the air moves to) at a point, in the planet frame,
    /// stronger with height above the ground (a 1/7 power law from 10 m).
    pub fn steady(&self, pos: DVec3, agl: f64) -> DVec3 {
        let (e, n, _) = geo::enu(pos);
        let to = (self.wind_from + 180.0).to_radians();
        (e * to.sin() + n * to.cos()) * self.wind * (agl.max(2.0) / 10.0).powf(1.0 / 7.0).min(2.0)
    }

    /// Wind with gusts at a point and time: a few incommensurate swells per
    /// axis, so it never quite repeats; the vertical part is weaker.
    pub fn at(&self, pos: DVec3, agl: f64, t: f64) -> DVec3 {
        let base = self.steady(pos, agl);
        if self.gusts <= 0.0 {
            return base;
        }
        let swell = |k: f64| ((0.53 * t + k).sin() * 0.6 + (1.37 * t + 2.1 * k).sin() * 0.45 + (3.1 * t + 3.7 * k).sin() * 0.3 + (7.9 * t + 5.3 * k).sin() * 0.15) / 0.85;
        let sigma = self.gusts * (1.0 + 0.35 * self.wind);
        let (e, n, u) = geo::enu(pos);
        // Gusts die out in the still air close to the ground.
        let near = (agl / 30.0).clamp(0.2, 1.0);
        base + (e * swell(0.0) + n * swell(1.9) + u * swell(4.3) * 0.5) * sigma * near
    }

    /// Carry the clouds along with the wind aloft (about twice the surface wind),
    /// and the thermals with the wind in the mixed layer below them.
    pub fn advance(&mut self, dt: f64, cam: DVec3) {
        // The cloud field is sampled at (position + offset), so it moves against the offset.
        self.cloud_off -= self.steady(cam, 2000.0) * dt * 1.6;
        self.thermal_off -= self.steady(cam, 300.0) * dt;
        // Keep the offset small so it never costs precision (the thermal lattice repeats nowhere, so a jump is harmless).
        if self.thermal_off.length() > 2.0e5 {
            self.thermal_off = DVec3::ZERO;
        }
    }

    /// How strongly the ground is heating the air: high sun, not overcast. 0..1
    pub fn convection(&self) -> f64 {
        smoothstep(0.1, 0.65, self.sun) * (1.0 - 0.85 * smoothstep(0.6, 0.97, self.clouds as f64))
    }

    /// Top of the convective mixed layer, metres above the ground: cumulus sit on
    /// it; on a clear day it still reaches up a kilometre or two.
    fn mixed_top(&self) -> f64 {
        let clear = 700.0 + 1400.0 * self.convection();
        if self.clouds > 0.1 { (self.cloud_base as f64).min(clear.max(600.0) + 800.0) } else { clear }
    }

    /// Thermals: columns of rising air a few hundred metres across, a kilometre
    /// or two apart, each ringed by sinking air, coming and going over a quarter
    /// of an hour. They sit on a jittered lattice laid over the planet's surface
    /// (altitude dropped, so the columns are vertical) and drift with the wind.
    pub fn thermal(&self, pos: DVec3, agl: f64, t: f64) -> f64 {
        let conv = self.convection();
        let top = self.mixed_top();
        if conv <= 0.01 || agl > top * 1.1 {
            return 0.0;
        }
        const CELL: f64 = 1400.0;
        let p = (pos + self.thermal_off).normalize() * 6.371e6;
        let c = (p / CELL).floor();
        let mut w = 0.0;
        for k in -1..=1 {
            for j in -1..=1 {
                for i in -1..=1 {
                    let (x, y, z) = (c.x as i64 + i, c.y as i64 + j, c.z as i64 + k);
                    let h0 = shash((x as u32).wrapping_mul(73856093) ^ (y as u32).wrapping_mul(19349663) ^ (z as u32).wrapping_mul(83492791) ^ 0x5bd1e995);
                    let (h1, h2) = (shash(h0), shash(h0 ^ 0x68e31da4));
                    // About half the sites are working at any one time, each on its own cycle.
                    let life = (t / (600.0 + 900.0 * unit(h2)) + unit(h1 >> 16)) * std::f64::consts::TAU;
                    let on = life.sin() * 1.4 - 0.35;
                    if on <= 0.0 {
                        continue;
                    }
                    let jit = DVec3::new(unit(h0), unit(h0 >> 16), unit(h1));
                    let cen = ((DVec3::new(x as f64, y as f64, z as f64) + jit) * CELL).normalize() * 6.371e6;
                    let r2 = cen.distance_squared(p);
                    let rad = 110.0 + 200.0 * unit(h2 >> 16);
                    if r2 > (rad * 3.0).powi(2) {
                        continue;
                    }
                    let s = r2 / (rad * rad);
                    // Lift in the core, a ring of sink around it (Allen's profile, simplified).
                    w += (1.6 + 2.6 * unit(h1)) * on.min(1.0) * (-s).exp() * (1.0 - 0.75 * s);
                }
            }
        }
        // Weak to start with near the ground, strongest through the middle of the
        // layer, petering out at its top; the air between the thermals sinks a little.
        let z = (agl / top).max(0.0);
        let shape = (z * 8.0).min(1.0).powf(0.5) * (1.0 - smoothstep(0.75, 1.1, z));
        (w * shape - 0.35 * (1.0 - smoothstep(0.9, 1.1, z))) * conv
    }

    /// The turbulence and vertical air motion at a point: wind tumbling over the
    /// ground, convection on a sunny day, the inside of the cloud layer, and the
    /// up- and downdrafts of storm cells. `water` damps the thermals.
    pub fn air(&self, pos: DVec3, agl: f64, t: f64, water: bool) -> Air {
        let agl = agl.max(1.0);
        let ft = agl * 3.28084;
        // Mechanical: about a tenth of the wind near the ground (MIL-F-8785C),
        // more on a gusty day, dying out above the surface layer.
        let w20 = self.wind * 0.93;
        let mech = (0.1 + 0.2 * self.gusts) * w20 * (1.0 - 0.65 * smoothstep(250.0, 900.0, agl));
        // Convective eddies through the mixed layer.
        // (Under the rain deck the sun is gone and the thermals with it.)
        let sunlit = 1.0 - 0.9 * self.deck_shade(pos);
        let conv = self.convection() * sunlit * if water { 0.25 } else { 1.0 };
        let top = self.mixed_top();
        let convt = 1.1 * conv * (1.0 - smoothstep(0.85 * top, 1.15 * top, agl)) * smoothstep(0.0, 60.0, agl).max(0.3);
        // In the cloud layer itself (strongest where it is cumulus-like rather than a flat deck).
        let (_, _, alt) = geo::pos_to_lla(pos);
        let (base, ctop) = (self.base() as f64, (self.base() + self.depth()) as f64);
        let inside = smoothstep(base - 150.0, base + 100.0, alt) * (1.0 - smoothstep(ctop - 100.0, ctop + 200.0, alt));
        let cloud = inside * self.clouds as f64 * (0.7 + 0.8 * self.convection() + 0.6 * self.storms as f64);
        // In and under the rain deck: smooth, a light chop in the heavier rain
        // (the falling rain drags the air down unevenly).
        let deck = self.deck_area(pos).min(1.0) * self.deck as f64 * 0.25 + 0.6 * self.deck_rain(pos);
        // A light background chop everywhere, as there always is a little.
        let background = 0.12 + 0.03 * self.wind;

        // Storm cells (storm.rs): severe, gusty turbulence in and around a tower,
        // the updraft feeding it, the downdraft in its rain and the gust front
        // that spreads out from under it.
        let st = crate::storm::local(self, pos, agl, t);
        let storm = st.sigma;
        let draft = st.draft;
        let rest = convt * convt + cloud * cloud + deck * deck + storm * storm + background * background;
        let sigma_w = (mech * mech + rest).sqrt().min(13.0);
        // Near the ground the eddies are as small as the height and the horizontal
        // gusts larger than the vertical; aloft they are a few hundred metres across.
        let k = 0.177 + 0.000823 * ft.min(1000.0);
        let len_w = agl.clamp(12.0, 530.0);
        let len_h = (agl / k.powf(1.2)).clamp(40.0, 530.0);
        // (Only the wind's own eddies, tumbling over the ground, are stretched
        // out sideways like that; a storm's are as strong across as up.)
        let sigma_h = (mech * mech / k.powf(0.8).clamp(0.25, 1.0) + rest).sqrt().min(13.0);
        Air { sigma_w, sigma_h, len_w, len_h, draft: self.thermal(pos, agl, t) * sunlit * if water { 0.2 } else { 1.0 } + draft, gust: st.gust }
    }

    /// the flight card's weather: real weather and time, the presets, then the day, wind and sky in tiles.
    pub fn card(&mut self, ui: &mut egui::Ui, utc: &mut f64, doy: &mut f64, lon: f64) {
        use crate::flightui::{cells, key, note, tag, GAP};
        let local = (*utc + lon / 15.0).rem_euclid(24.0);
        cells(ui, 2, 64.0, |ui, i| {
            let (name, on) = if i == 0 { (t!("weather.real_weather"), &mut self.live) } else { (t!("weather.real_time"), &mut self.real_time) };
            ui.horizontal(|ui| {
                tag(ui, name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| crate::menu::switch(ui, on));
            });
            let line = if i == 1 {
                if self.real_time { t!("weather.the_sun_where_you_are", crate::menu::hhmm(local)) } else { t!("weather.set_by_hand_below").into() }
            } else if !self.live {
                t!("weather.set_by_hand_below").into()
            } else if !self.live_station.is_empty() {
                t!("weather.from_2", self.live_station)
            } else if self.live_raw.is_empty() {
                t!("weather.waiting_for_a_report_2").into()
            } else {
                self.live_raw.clone()
            };
            note(ui, &line);
        });
        // the presets as keys across the card.
        let free = !self.live;
        ui.add_enabled_ui(free, |ui| {
            let now = self.summary();
            let n = PRESETS.len() as f32;
            let w = (ui.available_width() - GAP * (n - 1.0)) / n;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for &(name, c, b, d, s, a, h, w2, g) in PRESETS {
                    if key(ui, tr_en(name), free && now == tr_en(name), w, 32.0).clicked() {
                        (self.clouds, self.cloud_base, self.cloud_depth, self.storms, self.alto, self.haze, self.wind, self.gusts) = (c, b, d, s, a, h, w2, g);
                    }
                }
            });
        });
        ui.add_space(GAP - ui.spacing().item_spacing.y);
        let clock_free = !self.real_time;
        cells(ui, 2, 268.0, |ui, i| {
            ui.spacing_mut().item_spacing.y = 2.0;
            if i == 0 {
                tag(ui, t!("weather.day"));
                ui.add_enabled_ui(clock_free, |ui| {
                    let mut l = local;
                    if row(ui, t!("weather.local_time"), crate::menu::hhmm(l), Slider::new(&mut l, 0.0..=23.99)).changed() {
                        *utc = (l - lon / 15.0).rem_euclid(24.0);
                    }
                    let d = date_label(*doy);
                    row(ui, t!("weather.date"), d, Slider::new(doy, 0.0..=364.0).step_by(1.0));
                    let rate = self.time_rate;
                    row(ui, t!("weather.clock"), if rate == 0.0 { t!("weather.stopped").into() } else { format!("{rate:.0}×") }, Slider::new(&mut self.time_rate, 0.0..=3600.0).logarithmic(true).smallest_positive(1.0));
                });
                let moon = tr_en(moon_phase(self.moon_age)).to_string();
                row(ui, t!("weather.moon"), moon, Slider::new(&mut self.moon_age, 0.0..=29.5));
                ui.add_space(10.0);
                tag(ui, t!("weather.wind"));
                ui.add_enabled_ui(free, |ui| {
                    let mut kt = self.wind * 1.94384;
                    if row(ui, t!("weather.speed"), format!("{kt:.0} kt"), Slider::new(&mut kt, 0.0..=50.0)).changed() {
                        self.wind = kt.round() / 1.94384;
                    }
                    let from = self.wind_from;
                    row(ui, t!("weather.from"), format!("{from:03.0}° {}", tr_en(compass(from))), Slider::new(&mut self.wind_from, 0.0..=359.0).step_by(1.0));
                    let g = self.gusts;
                    row(ui, t!("weather.gusts"), format!("{:.0}%", g * 100.0), Slider::new(&mut self.gusts, 0.0..=1.0));
                });
            } else {
                tag(ui, t!("weather.sky"));
                ui.add_enabled_ui(free, |ui| {
                    let pc = |v: f32| format!("{:.0}%", v * 100.0);
                    let v = self.clouds;
                    row(ui, t!("weather.cloud_cover_2"), pc(v), Slider::new(&mut self.clouds, 0.0..=1.0));
                    let v = self.cloud_base;
                    row(ui, t!("weather.cloud_base_2"), format!("{v:.0} m"), Slider::new(&mut self.cloud_base, 300.0..=4000.0).step_by(50.0));
                    let v = self.cloud_depth;
                    row(ui, t!("weather.cloud_depth_2"), format!("{v:.0} m"), Slider::new(&mut self.cloud_depth, 500.0..=8000.0).step_by(50.0));
                    let v = self.storms;
                    row(ui, t!("weather.storm_cells_2"), pc(v), Slider::new(&mut self.storms, 0.0..=1.0));
                    let v = self.alto;
                    row(ui, t!("weather.mid_cloud"), pc(v), Slider::new(&mut self.alto, 0.0..=1.0));
                    // rain left to the storm cells unless it is set here.
                    let mut r = self.rain_set.unwrap_or(self.rain);
                    let shown = if self.rain_set.is_none() { t!("weather.auto").to_string() } else { pc(r) };
                    if row(ui, t!("weather.rain"), shown, Slider::new(&mut r, 0.0..=1.0)).changed() {
                        self.rain_set = Some(r);
                    }
                    let v = self.haze;
                    row(ui, t!("weather.haze"), format!("{v:.1}×"), Slider::new(&mut self.haze, 0.2..=40.0).logarithmic(true));
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    note(ui, &t!("weather.visibility_about_0_km", self.visibility_km()));
                    if self.rain_set.is_some() && free {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if key(ui, t!("weather.rain_auto"), false, 84.0, 22.0).clicked() {
                                self.rain_set = None;
                            }
                        });
                    }
                });
            }
        });
    }

    /// The real-weather switch, and where the report comes from.
    fn live_switch(&mut self, ui: &mut egui::Ui) {
        switch_row(ui, t!("weather.real_weather"), t!("weather.metar_else_open_meteo_radar"), &mut self.live);
        if self.live {
            let text = if self.live_station.is_empty() {
                if self.live_raw.is_empty() { t!("weather.waiting_for_a_report").to_string() } else { self.live_raw.clone() }
            } else if self.live_raw.is_empty() {
                t!("weather.from_weather_model", self.live_station)
            } else {
                self.live_raw.clone()
            };
            ui.add(egui::Label::new(RichText::new(text).font(egui::FontId::monospace(12.5)).color(crate::menu::DIM)).wrap());
        }
    }

    /// The start screen's version: local time and date at the chosen spot
    /// (the clock is set from them on take-off), then the sky and the wind.
    /// The start screen's weather: presets, sky and wind.
    pub fn start_weather(&mut self, ui: &mut egui::Ui) {
        self.live_switch(ui);
        ui.add_enabled_ui(!self.live, |ui| {
            self.presets(ui);
            ui.add_space(4.0);
            self.sky_controls(ui);
            self.wind_controls(ui);
        });
    }

    /// The start screen's time: local time of day and the date.
    pub fn start_time(ui: &mut egui::Ui, hour: &mut f64, doy: &mut f64, real_time: &mut bool) {
        real_time_switch(ui, real_time);
        let free = !*real_time;
        egui::Grid::new("start_time").num_columns(2).min_col_width(92.0).spacing([10.0, 5.0]).show(ui, |ui| {
            row_label(ui, t!("weather.local_time"));
            ui.add_enabled_ui(free, |ui| add(ui, Slider::new(hour, 0.0..=23.99).custom_formatter(|v, _| crate::menu::hhmm(v))));
            ui.end_row();
            row_label(ui, t!("weather.date"));
            ui.add_enabled_ui(free, |ui| add(ui, Slider::new(doy, 0.0..=364.0).step_by(1.0).custom_formatter(|v, _| date_label(v))));
            ui.end_row();
        });
    }

    /// A few words for the weather: the preset it matches, else the cloud
    /// cover and wind.
    pub fn summary(&self) -> String {
        if self.live {
            return if self.live_station.is_empty() { t!("weather.real").to_string() } else { t!("weather.real_2", self.live_station) };
        }
        let near = |a: f64, b: f64| (a - b).abs() < 1e-3 * (1.0 + b.abs());
        for &(name, c, b, d, s, a, h, w, g) in PRESETS {
            if near(self.clouds as f64, c as f64) && near(self.cloud_base as f64, b as f64) && near(self.cloud_depth as f64, d as f64) && near(self.storms as f64, s as f64) && near(self.alto as f64, a as f64) && near(self.haze as f64, h as f64) && near(self.wind as f64, w as f64) && near(self.gusts as f64, g as f64) {
                return tr_en(name).to_string();
            }
        }
        t!("weather.cloud_kt", self.clouds * 100.0, self.wind * 1.94384)
    }

    fn sky_controls(&mut self, ui: &mut egui::Ui) {
        section(ui, t!("weather.sky"), |ui| {
            row_label(ui, t!("weather.cloud_cover"));
            add(ui, Slider::new(&mut self.clouds, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            ui.end_row();
            row_label(ui, t!("weather.cloud_base"));
            add(ui, Slider::new(&mut self.cloud_base, 300.0..=4000.0).step_by(50.0).suffix(" m"));
            ui.end_row();
            row_label(ui, t!("weather.cloud_depth"));
            add(ui, Slider::new(&mut self.cloud_depth, 500.0..=8000.0).step_by(50.0).suffix(" m"));
            ui.end_row();
            row_label(ui, t!("weather.storm_cells"));
            add(ui, Slider::new(&mut self.storms, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            ui.end_row();
            row_label(ui, t!("weather.mid_level_cloud"));
            add(ui, Slider::new(&mut self.alto, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            ui.end_row();
            row_label(ui, t!("weather.rain"));
            ui.horizontal(|ui| {
                let mut r = self.rain_set.unwrap_or(self.rain);
                if add(ui, Slider::new(&mut r, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed() {
                    self.rain_set = Some(r);
                }
                // Set, a rain deck (nimbostratus) that heavy closes in overhead;
                // unset, the rain is wherever the storm cells drop it.
                let mut auto = self.rain_set.is_none();
                if crate::menu::toggle(ui, &mut auto, t!("weather.auto")).changed() {
                    self.rain_set = if auto { None } else { Some(self.rain) };
                }
            });
            ui.end_row();
            row_label(ui, t!("weather.haze"));
            add(ui, Slider::new(&mut self.haze, 0.2..=40.0).logarithmic(true).custom_formatter(|v, _| format!("{v:.1}×")));
            ui.end_row();
            ui.label(RichText::new(t!("weather.visibility")).font(crate::menu::caption()).color(crate::menu::DIM));
            ui.label(RichText::new(format!("~{:.0} km", self.visibility_km())).font(egui::FontId::monospace(12.5)).color(crate::menu::TEXT));
            ui.end_row();
        });
    }

    fn wind_controls(&mut self, ui: &mut egui::Ui) {
        section(ui, t!("weather.wind"), |ui| {
            row_label(ui, t!("weather.speed"));
            let mut kt = self.wind * 1.94384;
            if add(ui, Slider::new(&mut kt, 0.0..=50.0).fixed_decimals(0).suffix(" kt")).changed() {
                self.wind = kt / 1.94384;
            }
            ui.end_row();
            row_label(ui, t!("weather.from"));
            add(ui, Slider::new(&mut self.wind_from, 0.0..=359.0).step_by(1.0).custom_formatter(|v, _| format!("{:03.0}° {}", v, tr_en(compass(v)))));
            ui.end_row();
            row_label(ui, t!("weather.gusts"));
            add(ui, Slider::new(&mut self.gusts, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
            ui.end_row();
        });
    }

    fn presets(&mut self, ui: &mut egui::Ui) {
        crate::flightui::pane_section(ui, t!("weather.presets"));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            let now = self.summary();
            for &(name, c, b, d, s, a, h, w, g) in PRESETS {
                if crate::flightui::chip(ui, tr_en(name), false, !self.live && now == tr_en(name)).clicked() {
                    (self.clouds, self.cloud_base, self.cloud_depth, self.storms, self.alto, self.haze, self.wind, self.gusts) = (c, b, d, s, a, h, w, g);
                }
            }
        });
    }
}

use crate::i18n::{t, tr_en};
use egui::{RichText, Slider};

/// The "real time" switch above the clock's sliders.
fn real_time_switch(ui: &mut egui::Ui, on: &mut bool) {
    switch_row(ui, t!("weather.real_time"), t!("weather.the_sun_where_you_are_2"), on);
}

/// a switch on its own row with its name at the left.
fn switch_row(ui: &mut egui::Ui, name: &str, hint: &str, on: &mut bool) {
    let w = ui.available_width();
    ui.allocate_ui_with_layout(egui::vec2(w, 36.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(w);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(RichText::new(name).font(crate::menu::label()).color(crate::menu::TEXT));
            ui.label(RichText::new(hint).font(crate::menu::caption()).color(crate::flightui::SOFT));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            crate::menu::switch(ui, on);
        });
    });
}

/// The real time now: hours UTC and the day of the year (0 = 1 January,
/// the 29th of February counted as the 28th), from the system clock.
pub fn real_clock() -> (f64, f64) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    clock_at(secs)
}

fn clock_at(secs: f64) -> (f64, f64) {
    let days = (secs / 86_400.0).floor();
    let hours = (secs - days * 86_400.0) / 3600.0;
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let dy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * dy + 2) / 153;
    let d = dy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    const BEFORE: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let doy = (BEFORE[(m - 1) as usize] + d.min(if m == 2 { 28 } else { 31 }) - 1).min(364);
    (hours, doy as f64)
}

/// "Jun 21", or "21 juin" in French.
pub fn date_label(doy: f64) -> String {
    const LEN: [u32; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut d = doy.max(0.0) as u32 % 365;
    let mut m = 0;
    while m < 11 && d >= LEN[m] {
        d -= LEN[m];
        m += 1;
    }
    let month = crate::i18n::tr(&format!("date.month_{}", m + 1));
    t!("date.day_month", day = d.min(LEN[m] - 1) + 1, month = month)
}

/// A headed two-column grid of labelled sliders.
fn section(ui: &mut egui::Ui, title: &str, rows: impl FnOnce(&mut egui::Ui)) {
    crate::flightui::pane_section(ui, title);
    egui::Grid::new(("wx", title)).num_columns(2).min_col_width(104.0).spacing([10.0, 6.0]).show(ui, rows);
}

/// a slider row in a tile: the name, the track and the value, each in its own column.
fn row(ui: &mut egui::Ui, name: &str, value: String, slider: Slider) -> egui::Response {
    let w = ui.available_width();
    ui.allocate_ui_with_layout(egui::vec2(w, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.allocate_ui_with_layout(egui::vec2(86.0, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.set_min_width(86.0);
            row_label(ui, name);
        });
        let vw = 92.0;
        ui.spacing_mut().slider_width = (ui.available_width() - vw - 8.0).max(40.0);
        let r = add(ui, slider.show_value(false));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(value).font(crate::menu::medium(13.5)).color(egui::Color32::WHITE)).truncate());
        });
        r
    })
    .inner
}

/// a row's name.
fn row_label(ui: &mut egui::Ui, s: &str) {
    ui.label(RichText::new(s).font(crate::menu::caption()).color(crate::flightui::SOFT));
}

/// A focused slider would also take the arrow keys that fly the aircraft, so it lets go.
fn add(ui: &mut egui::Ui, s: Slider) -> egui::Response {
    let r = ui.add(s);
    if r.drag_stopped() || r.clicked() || r.lost_focus() {
        r.surrender_focus();
    }
    r
}

#[cfg(test)]
pub fn month_day(doy: f64) -> String {
    const M: [(&str, u32); 12] = [("Jan", 31), ("Feb", 28), ("Mar", 31), ("Apr", 30), ("May", 31), ("Jun", 30), ("Jul", 31), ("Aug", 31), ("Sep", 30), ("Oct", 31), ("Nov", 30), ("Dec", 31)];
    let mut d = doy.max(0.0) as u32 % 365;
    for (name, len) in M {
        if d < len {
            return format!("{} {}", name, d + 1);
        }
        d -= len;
    }
    "Dec 31".into()
}

fn moon_phase(age: f64) -> &'static str {
    match (age / 29.53 * 8.0 + 0.5) as u32 % 8 {
        0 => "new",
        1 => "waxing crescent",
        2 => "first quarter",
        3 => "waxing gibbous",
        4 => "full",
        5 => "waning gibbous",
        6 => "last quarter",
        _ => "waning crescent",
    }
}

fn compass(deg: f64) -> &'static str {
    const P: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    P[((deg / 45.0 + 0.5) as usize) % 8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(month_day(0.0), "Jan 1");
        assert_eq!(month_day(171.0), "Jun 21");
        assert_eq!(month_day(364.0), "Dec 31");
        assert_eq!(moon_phase(0.0), "new");
        assert_eq!(moon_phase(14.8), "full");
        assert_eq!(compass(248.0), "W");
        assert_eq!(compass(350.0), "N");
        // 2026-11-15 17:20 UTC, 2024-02-29 (leap day) and 2024-12-31 noon
        let (h, d) = clock_at(1_794_763_200.0);
        assert_eq!((month_day(d).as_str(), (h * 60.0).round()), ("Nov 15", 17.0 * 60.0 + 20.0));
        assert_eq!(month_day(clock_at(1_709_208_000.0).1), "Feb 28");
        assert_eq!(month_day(clock_at(1_735_646_400.0).1), "Dec 31");
        assert_eq!(clock_at(0.0), (0.0, 0.0));
    }

    #[test]
    fn rain_deck_clears_when_turned_off() {
        let mut w = Weather::new(0.3);
        let p = geo::lla_to_pos(0.72, -1.68, 800.0);
        let step = |w: &mut Weather, secs: f64| {
            for _ in 0..(secs * 60.0) as usize {
                w.step_deck(1.0 / 60.0, p, Some(300.0));
            }
        };
        // Turned on in flight, it closes in over a few seconds, then rains.
        w.rain_set = Some(0.7);
        step(&mut w, 1.0);
        assert!(w.deck_at.is_some() && w.deck_rain(p) == 0.0);
        step(&mut w, 20.0);
        assert!((w.deck_rain(p) - 0.7).abs() < 0.05, "{}", w.deck_rain(p));
        // Turned off, the rain stops and the deck is gone within the fade
        // (it used to linger as a full overcast for half a minute and more).
        w.rain_set = Some(0.0);
        step(&mut w, 10.0);
        assert_eq!(w.deck_rain(p), 0.0);
        step(&mut w, 6.0);
        assert!(w.deck_at.is_none() && w.deck_layer().is_none());
        assert_eq!(w.deck_globals(), [[0.0; 4]; 4]);
        assert_eq!(w.base(), w.cloud_base);
        // Back on half way through a fade: it grows back where it was.
        w.rain_set = Some(0.5);
        step(&mut w, 20.0);
        w.rain_set = Some(0.0);
        step(&mut w, 5.0);
        let at = w.deck_at;
        w.rain_set = Some(0.5);
        step(&mut w, 20.0);
        assert!(w.deck_at.is_some() && w.deck_fade == 1.0 && w.deck_rain(p) > 0.4);
        assert!(w.deck_at.unwrap().distance(at.unwrap()) < 1.0);
    }

    #[test]
    fn wind_blows_from_its_direction() {
        let mut w = Weather::new(0.5);
        w.gusts = 0.0;
        (w.wind, w.wind_from) = (10.0, 270.0);
        let p = geo::lla_to_pos(0.6, -2.1, 100.0);
        let (e, _, _) = geo::enu(p);
        let v = w.at(p, 10.0, 3.0);
        // A westerly blows toward the east, at the stated speed 10 m up.
        assert!((v.dot(e) - 10.0).abs() < 1e-6, "{v:?}");
        assert!(w.steady(p, 300.0).length() > 10.0);
        assert!((w.visibility_km() - 3.912 / (13.558e-6 + 8.396e-6) / 1000.0).abs() < 1e-9);
    }

    #[test]
    fn radar_cells_grow_in_and_die_away() {
        let mut w = Weather::new(0.5);
        w.storms = 0.0;
        let cam = geo::lla_to_pos(0.6, -2.1, 1000.0);
        let (e, n, _) = geo::enu(cam);
        let q = cam.normalize() * 6371.0;
        let a = (q + e * 40.0).normalize() * 6371.0;
        let b = (q + n * 80.0).normalize() * 6371.0;
        // A fresh start shows the radar's cells as they are.
        w.set_live_cells(&[(a, 7.0, 0.95, 0.0), (b, 2.5, 0.3, 0.0)], true);
        assert_eq!(w.storm_cells(cam).len(), 2);
        assert!(w.live_cells.iter().all(|c| c.on == 1.0));
        let seed = w.live_cells[0].seed;
        // The next picture: the big cell has moved 5 km and grown, the shower is
        // gone and a new one has popped up.
        let a2 = (a + e * 5.0).normalize() * 6371.0;
        let c = (q - n * 60.0).normalize() * 6371.0;
        w.set_live_cells(&[(a2, 8.0, 1.0, 0.0), (c, 3.0, 0.4, 0.0)], false);
        assert_eq!(w.live_cells.len(), 3);
        assert_eq!(w.live_cells[0].seed, seed, "the big cell is the same cell");
        w.step_live(1.0);
        let big = &w.live_cells[0];
        assert!(big.at.distance(a) < 1.0 && big.rad < 7.3, "it glides, not jumps: {big:?}");
        let fresh = w.live_cells.iter().find(|x| x.goal == c).unwrap();
        assert!(fresh.on > 0.0 && fresh.on < 0.1, "it grows in: {fresh:?}");
        for _ in 0..600 {
            w.step_live(1.0);
        }
        assert_eq!(w.live_cells.len(), 2, "the shower has died away");
        assert!(w.live_cells.iter().all(|c| c.on == 1.0));
        assert!(w.live_cells[0].at.distance(a2) < 0.01 && (w.live_cells[0].rad - 8.0).abs() < 0.01);
        assert!(w.cloud_ceiling() > STORM_TOP);
        // Where radar sees, the made-up cells give way to the real ones.
        w.storms = 1.0;
        w.live_cells.clear();
        let made_up = w.storm_cells(cam).len();
        assert!(made_up > 0);
        let (la, lo, _) = geo::pos_to_lla(cam);
        let mut e = crate::radar::Echo { z: 0, gx0: 0, gy0: 0, w: 0, h: 0, dbz: vec![], cover: vec![] };
        let (z, tx, ty, nt) = crate::radar::Echo::block(la.to_degrees(), lo.to_degrees());
        let sz = 256 * nt as usize;
        (e.z, e.gx0, e.gy0, e.w, e.h) = (z, tx * 256, ty * 256, sz, sz);
        (e.dbz, e.cover) = (vec![f32::NAN; sz * sz], vec![true; sz * sz]);
        w.radar_cover = Some(crate::radar::analyse(&e, (la.to_degrees(), lo.to_degrees())).cover);
        let near = w.storm_cells(cam).iter().filter(|s| DVec3::new(s[0][0] as f64, s[0][1] as f64, s[0][2] as f64).distance(q) < 150.0).count();
        assert_eq!(near, 0);
    }

    #[test]
    fn showery_days_mostly_showers_stormy_days_thunderstorms() {
        let cam = geo::lla_to_pos(0.71, -1.68, 1000.0);
        // A shower tops out below the lightning height, a thunderstorm above it.
        let tops = |storms: f32, live_showers: bool| {
            let mut w = Weather::new(0.5);
            w.storms = storms;
            (w.live, w.showers_only) = (live_showers, live_showers);
            let base = w.base() as f64;
            let cells = w.storm_cells(cam);
            let top = |c: &[[f32; 4]; 2]| base + (c[1][1] as f64 - base) * storm_top_share(c[1][0] as f64);
            let showers = cells.iter().filter(|c| c[1][0] > 0.0 && top(c) < crate::storm::LIGHTNING_TOP - 800.0).count();
            let storms = cells.iter().filter(|c| top(c) > crate::storm::LIGHTNING_TOP).count();
            (showers, storms)
        };
        let (sh, ts) = tops(0.3, false);
        assert!(sh > 2 * ts && sh >= 4, "showery: {sh} showers, {ts} thunderstorms");
        let (sh, ts) = tops(1.0, false);
        assert!(ts > sh && ts >= 4, "stormy: {sh} showers, {ts} thunderstorms");
        let (sh, ts) = tops(0.5, true);
        assert!(ts == 0 && sh > 0, "live, SH without TS: {sh} showers, {ts} thunderstorms");
        // A shower placed by hand: young, no anvil, topping out around 6 km.
        let mut w = Weather::new(0.5);
        w.storms = 0.0;
        w.place_storm(cam, 10.0, 90.0, 1.8, CellKind::parse("shower"));
        let c = w.storm_cells(cam)[0];
        let top = w.base() as f64 + (c[1][1] as f64 - w.base() as f64) * storm_top_share(c[1][0] as f64);
        assert!(c[1][0] < 0.45 && (top - 6000.0).abs() < 1.0 && c[1][2] == 0.0, "{c:?} top {top}");
    }

    #[test]
    fn squall_lines_on_stormy_days_and_by_hand() {
        // Placed by hand: one cell carrying its length, its leading edge where asked.
        let cam = geo::lla_to_pos(0.72, -1.68, 500.0);
        let mut w = Weather::new(0.3);
        w.storms = 0.0;
        w.place_squall(cam, 20.0, 270.0, 100.0);
        let c = w.storm_cells(cam);
        assert_eq!(c.len(), 1);
        let (len2, seed) = crate::storm::line_of(&c[0]);
        assert!(len2 == 50_000.0 && (0.0..1.0).contains(&seed), "{:?}", c[0]);
        let cen = DVec3::new(c[0][0][0] as f64, c[0][0][1] as f64, c[0][0][2] as f64);
        let d = cen.distance(cam.normalize() * 6371.0);
        assert!((d - 20.0 - crate::storm::SHELF_F * SQUALL_R).abs() < 0.5, "{d}");
        // On a stormy day somewhere a line stands in for the cells round it;
        // on an ordinary one never.
        let lines = |st: f32| {
            let mut w = Weather::new(0.55);
            w.storms = st;
            let mut n = 0;
            for k in 0..40 {
                let p = geo::lla_to_pos(0.3 + 0.05 * k as f64, -1.5 - 0.07 * k as f64, 500.0);
                let cells = w.storm_cells(p);
                let near: Vec<_> = cells.iter().filter(|c| c[1][3] >= 1.0).collect();
                if let Some(l) = near.first() {
                    let len = l[1][3].floor() as f64;
                    assert!((36.0..=150.0).contains(&len) && l[0][3] > 2.5, "{l:?}");
                    let lc = DVec3::new(l[0][0] as f64, l[0][1] as f64, l[0][2] as f64);
                    // (no thunderstorm right beside it)
                    assert!(cells.iter().filter(|c| c[1][3] < 1.0 && c[0][3] > 2.6).all(|c| DVec3::new(c[0][0] as f64, c[0][1] as f64, c[0][2] as f64).distance(lc) > 0.5 * len + 25.0));
                    n += 1;
                }
            }
            n
        };
        let stormy = lines(1.0);
        assert!((4..40).contains(&stormy), "{stormy} of 40 places see a squall line on a stormy day");
        assert_eq!(lines(0.4), 0);
        // Live: the radar's line comes in as one, and only matches a line.
        let mut w = Weather::new(0.5);
        let a = (cam.normalize() * 6371.0 + DVec3::X * 30.0).normalize() * 6371.0;
        w.set_live_cells(&[(a, 6.0, 1.0, 110.0)], true);
        let c = w.storm_cells(cam);
        assert!(c.iter().any(|c| c[1][3].floor() == 110.0), "{c:?}");
        w.set_live_cells(&[(a, 6.0, 1.0, 0.0)], false);
        assert_eq!(w.live_cells.len(), 2, "a lone cell does not take over a line");
    }

    #[test]
    fn supercells_rare_isolated_and_by_hand() {
        // Placed by hand: maturity 2, the higher anvil and the biggest overshoot.
        let cam = geo::lla_to_pos(0.72, -1.68, 500.0);
        let mut w = Weather::new(0.3);
        w.storms = 0.0;
        w.place_storm(cam, 60.0, 270.0, SUPER_R, CellKind::parse("supercell"));
        let c = w.storm_cells(cam);
        assert!(c.len() == 1 && supercell(&c[0]) == 1.0 && c[0][1][1] == SUPER_TOP && c[0][1][2] == SUPER_OVERSHOOT, "{c:?}");
        assert!(w.cloud_ceiling() >= SUPER_TOP + SUPER_OVERSHOOT);
        // Only on the stormiest days, now and then, and with no other cell near it.
        let supers = |st: f32| {
            let mut w = Weather::new(0.55);
            w.storms = st;
            let mut n = 0;
            for k in 0..40 {
                let p = geo::lla_to_pos(0.3 + 0.05 * k as f64, -1.5 - 0.07 * k as f64, 500.0);
                let cells = w.storm_cells(p);
                for s in cells.iter().filter(|c| supercell(c) > 0.5) {
                    let sc = DVec3::new(s[0][0] as f64, s[0][1] as f64, s[0][2] as f64);
                    assert!(s[0][3] > 6.0, "{s:?}");
                    assert!(cells.iter().filter(|c| supercell(c) == 0.0 && c[1][3] < 1.0).all(|c| DVec3::new(c[0][0] as f64, c[0][1] as f64, c[0][2] as f64).distance(sc) > 0.5 * SUPER_CLEAR), "a cell beside a supercell");
                    n += 1;
                }
            }
            n
        };
        let stormy = supers(1.0);
        assert!((1..40).contains(&stormy), "{stormy} supercells seen from 40 places on the stormiest day");
        assert_eq!(supers(0.6), 0);
        // Live: an isolated, intense, large echo comes in as one.
        let mut w = Weather::new(0.5);
        let a = (cam.normalize() * 6371.0 + DVec3::X * 30.0).normalize() * 6371.0;
        w.set_live_cells(&[(a, 8.0, 2.0, 0.0)], true);
        let c = w.storm_cells(cam);
        assert!(c.iter().any(|c| supercell(c) == 1.0), "{c:?}");
    }

    #[test]
    fn turbulence_follows_the_weather() {
        let p = geo::lla_to_pos(0.6, -2.1, 300.0);
        let mut w = Weather::new(0.3);
        (w.wind, w.gusts, w.sun) = (2.0, 0.0, -0.2);
        let calm = w.air(p, 300.0, 0.0, false);
        assert!(calm.sigma_w < 0.6, "{calm:?}");
        assert_eq!(w.thermal(p, 300.0, 0.0), 0.0);
        (w.wind, w.gusts) = (14.0, 0.7);
        let windy = w.air(geo::lla_to_pos(0.6, -2.1, 80.0), 80.0, 0.0, false);
        assert!(windy.sigma_w > 1.5 && windy.sigma_h > windy.sigma_w, "{windy:?}");
        // A sunny day: somewhere along a line there is lift and somewhere sink.
        w.sun = 0.8;
        let (e, _, _) = geo::enu(p);
        let ws: Vec<f64> = (0..400).map(|i| w.thermal(p + e * (i as f64 * 25.0), 500.0, 0.0)).collect();
        let (lo, hi) = ws.iter().fold((0.0f64, 0.0f64), |(a, b), &x| (a.min(x), b.max(x)));
        assert!(hi > 1.0 && lo < -0.2, "{lo} {hi}");
    }
}


