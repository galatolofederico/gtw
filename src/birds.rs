//! Birds: flocks of five species around the aircraft, by place, season and
//! hour, flying as each kind does, diving out of the aircraft's way, and now
//! and then striking it (a thud and a burst of feathers; no damage).
//!
//! The models come from Blender (assets/blender/birds.py, assets/birds.bin.gz):
//! per species a body, two wing segments a side hinged at the shoulder and the
//! wrist, and a one-piece model for far away. They are drawn as instanced
//! solids (inst.wgsl, plain diffuse feathers): each wing segment is its own
//! instance, turned about its hinge here on the CPU, so the shader needs
//! nothing new.
//!
//! The species (sizes as in birds.py; flight figures rounded from radar and
//! film studies: Bruderer & Boldt 2001, "Flight characteristics of birds: I.
//! radar measurements of speeds", Ibis 143:178-204; Bruderer et al. 2010,
//! "Wing-beat characteristics of birds recorded with tracking radar and cine
//! camera", Ibis 152:272-291; Pennycuick 1990, J. Exp. Biol. 150:171-185.
//! Approximate values, not re-checked against the tables):
//! - herring gull: coasts, lakes and the land near them; slow flaps (about 3
//!   per second) and long glides, low over the water;
//! - carrion crow: anywhere on land, low, in ones and small groups;
//! - common starling: fast (about 17 m/s), whirring wings (about 11 Hz) in
//!   bursts; in autumn and winter, murmurations over the roost at dusk;
//! - common buzzard: soaring in thermals on sunny days, wings held up in a
//!   shallow V, circling up and gliding off to the next;
//! - greylag goose: V formations at a few hundred metres when migrating
//!   (south-west in autumn, north-east in spring in the north); small groups
//!   near water the rest of the year.
//!
//! Real birds see an aircraft coming and dive out of its way (most strikes are
//! birds that did not), so these do too, if they are alert.

use crate::geo;
use crate::graphics::Level;
use crate::render::{Inst, MeshVertex};
use glam::{DQuat, DVec3, Mat4, Vec3};
use std::sync::OnceLock;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Gull,
    Crow,
    Starling,
    Buzzard,
    Goose,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Gull, Kind::Crow, Kind::Starling, Kind::Buzzard, Kind::Goose];
    pub fn spec(self) -> &'static Species {
        &SPECIES[self as usize]
    }
    pub fn parse(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.spec().name.eq_ignore_ascii_case(s.trim()))
    }
}

pub struct Species {
    /// the pieces' prefix in birds.bin.gz
    pub name: &'static str,
    /// wingspan (m), mass (kg)
    pub span: f64,
    pub mass: f64,
    /// cruising airspeed (m/s)
    pub speed: f64,
    /// wingbeats per second, and their amplitude at the shoulder (rad)
    pub flap_hz: f64,
    pub amp: f64,
    /// share of the time spent gliding between bouts of flapping
    pub glide: f64,
    /// how far up the wings are held when gliding (rad)
    pub dihedral: f64,
    /// how many in a flock
    pub flock: (usize, usize),
    /// the height band above the ground it usually keeps (m)
    pub agl: (f64, f64),
    /// how far off it is drawn (m)
    pub far: f64,
    /// how hard it can turn and speed up (m/s²)
    pub accel: f64,
}

pub const SPECIES: [Species; 5] = [
    Species { name: "gull", span: 1.44, mass: 1.0, speed: 11.5, flap_hz: 2.9, amp: 0.55, glide: 0.45, dihedral: 0.06, flock: (2, 10), agl: (15.0, 120.0), far: 1800.0, accel: 5.0 },
    Species { name: "crow", span: 0.98, mass: 0.5, speed: 12.5, flap_hz: 3.6, amp: 0.65, glide: 0.12, dihedral: 0.04, flock: (1, 8), agl: (10.0, 90.0), far: 1400.0, accel: 6.0 },
    Species { name: "starling", span: 0.40, mass: 0.08, speed: 17.0, flap_hz: 11.0, amp: 0.70, glide: 0.25, dihedral: 0.0, flock: (15, 60), agl: (10.0, 80.0), far: 900.0, accel: 12.0 },
    Species { name: "buzzard", span: 1.20, mass: 0.8, speed: 10.0, flap_hz: 3.0, amp: 0.50, glide: 0.92, dihedral: 0.14, flock: (1, 3), agl: (80.0, 900.0), far: 1800.0, accel: 4.0 },
    Species { name: "goose", span: 1.64, mass: 3.3, speed: 17.0, flap_hz: 3.0, amp: 0.45, glide: 0.03, dihedral: 0.03, flock: (9, 35), agl: (300.0, 1200.0), far: 2600.0, accel: 4.0 },
];

/// The pieces of each species, in the order of their mesh ids.
pub const PIECES: [&str; 6] = ["body", "left inner", "left outer", "right inner", "right outer", "far"];
/// First mesh id of the birds in the instanced-solid library (main.rs `solids`).
pub const BASE: u32 = crate::airmodels::BASE + crate::airmodels::COUNT as u32;

fn mesh_id(k: Kind, piece: usize) -> u32 {
    BASE + (k as usize * PIECES.len() + piece) as u32
}

/// The models, read once.
pub struct Models {
    /// in mesh-id order from BASE
    pub meshes: Vec<(Vec<MeshVertex>, Vec<u32>)>,
    /// per species, the hinges of left inner, left outer, right inner, right outer
    pivots: [[Vec3; 4]; 5],
    /// per species, how far across the far model reaches (m)
    reach: [f32; 5],
}

pub fn models() -> &'static Models {
    static M: OnceLock<Models> = OnceLock::new();
    M.get_or_init(|| {
        let objs = crate::aircraft::fa18::load(include_bytes!("../assets/birds.bin.gz"), "birds");
        let mut meshes = Vec::new();
        let mut pivots = [[Vec3::ZERO; 4]; 5];
        let mut reach = [0.0; 5];
        for k in Kind::ALL {
            for (pi, piece) in PIECES.iter().enumerate() {
                let name = format!("{} {}", k.spec().name, piece);
                let o = objs.get(&name).unwrap_or_else(|| panic!("birds model: no piece \"{name}\""));
                if (1..5).contains(&pi) {
                    pivots[k as usize][pi - 1] = o.pivot;
                }
                if *piece == "far" {
                    reach[k as usize] = o.mesh.v.iter().map(|v| v.pos[0].abs()).fold(0.0, f32::max);
                }
                // (Colour alpha 1: plain diffuse, inst.wgsl's trim material.)
                let v = o.mesh.v.iter().map(|v| MeshVertex { pos: v.pos, nrm: v.nrm, col: [v.col[0], v.col[1], v.col[2], 1.0] }).collect();
                meshes.push((v, o.mesh.i.clone()));
            }
        }
        Models { meshes, pivots, reach }
    })
}

/// How many flocks and birds at most, by setting.
pub fn caps(level: Level) -> (usize, usize) {
    match level {
        Level::Off => (0, 0),
        Level::Low => (3, 60),
        Level::Medium => (6, 250),
        Level::High | Level::Ultra => (10, 800),
    }
}

// ---------------------------------------------------------------- where and when

/// What a place is like when a flock is wanted there.
#[derive(Clone, Copy, Debug)]
pub struct Place {
    /// latitude (degrees)
    pub lat: f64,
    /// day of the year (0 = January 1st) and local solar hour
    pub doy: f64,
    pub hour: f64,
    /// sine of the sun's elevation
    pub sun: f64,
    /// share of the ground around that is water, 0..1
    pub water: f64,
    /// how strongly the ground heats the air (weather.rs `convection`), 0..1
    pub convection: f64,
    /// rain, 0..1
    pub rain: f64,
}

/// Day of the year as the northern hemisphere sees its seasons.
fn north_doy(p: &Place) -> f64 {
    if p.lat < 0.0 { (p.doy + 182.5).rem_euclid(365.0) } else { p.doy }
}

/// Whether geese are on the move: autumn (20 Sep - 30 Nov) or spring
/// (15 Feb - 15 Apr), as the northern hemisphere has them. Some(true) in
/// autumn.
pub fn migrating(p: &Place) -> Option<bool> {
    let d = north_doy(p);
    if (262.0..=333.0).contains(&d) {
        Some(true)
    } else if (45.0..=104.0).contains(&d) {
        Some(false)
    } else {
        None
    }
}

/// Starlings gather over their roosts at dusk from October to mid-March.
pub fn murmuration_time(p: &Place) -> bool {
    let d = north_doy(p);
    (d >= 273.0 || d <= 74.0) && p.hour > 13.0 && (-0.05..0.14).contains(&p.sun)
}

/// How likely each species is to be the next flock here (unnormalised), and
/// whether starlings would be a murmuration. All zero at night.
pub fn weights(p: &Place) -> ([f64; 5], bool) {
    // Nothing flying in the dark (geese do migrate at night, but unseen).
    if p.sun < -0.06 {
        return ([0.0; 5], false);
    }
    let land = 1.0 - p.water;
    let tropics = p.lat.abs() < 23.0;
    let day = (10.0..17.0).contains(&p.hour);
    let murm = murmuration_time(p) && !tropics;
    let mut w = [
        0.15 * land + 2.0 * p.water.min(0.6) / 0.6,
        if p.water > 0.85 { 0.0 } else { land },
        (if murm { 2.5 } else { 0.45 }) * land.max(0.3) * if tropics { 0.1 } else { 1.0 },
        if day { 1.4 * p.convection * land } else { 0.0 },
        if migrating(p).is_some() && !tropics { 1.2 } else { 0.12 * p.water.min(0.5) / 0.5 },
    ];
    let wet = 1.0 - 0.85 * p.rain.clamp(0.0, 1.0);
    for x in w.iter_mut() {
        *x *= wet;
    }
    (w, murm)
}

/// The heading (degrees from north) migrating geese keep: towards the
/// equator and west in autumn, back in spring.
pub fn migration_heading(p: &Place, autumn: bool) -> f64 {
    match (p.lat >= 0.0, autumn) {
        (true, true) => 215.0,
        (true, false) => 35.0,
        (false, true) => 20.0,
        (false, false) => 200.0,
    }
}

// ---------------------------------------------------------------- the flight

/// A small random number generator (xorshift).
#[derive(Clone, Copy, Debug)]
struct Rng(u64);
impl Rng {
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.f()
    }
    fn int(&mut self, a: usize, b: usize) -> usize {
        a + ((b - a + 1) as f64 * self.f()) as usize % (b - a + 1)
    }
}

#[derive(Clone, Debug)]
pub struct Bird {
    /// planet frame (m), and velocity over the ground (m/s)
    pub p: DVec3,
    pub v: DVec3,
    /// its place in the flock: east, north, up of the flock's centre (m)
    slot: DVec3,
    /// wingbeat phase (rad), and whether it is flapping or gliding, and for how much longer
    phase: f64,
    flapping: bool,
    bout: f64,
    /// bank (rad, + right wing down)
    bank: f64,
    /// diving away from the aircraft for this much longer (s), and which way
    flee: f64,
    flee_v: DVec3,
    /// how likely it is to see the aircraft coming, 0..1, and whether it has
    /// had its chance this time (it reacts, or never does, once per approach)
    alert: f64,
    noticed: bool,
    /// where it was in the aircraft's frame last time (for the strike test)
    prev_rel: Option<DVec3>,
    pub alive: bool,
}

#[derive(Clone, Debug)]
pub struct Flock {
    pub kind: Kind,
    pub murmuration: bool,
    /// migrating geese
    pub migrating: bool,
    /// keeping its heading (migrating, or placed on the command line)
    steady: bool,
    /// centre (planet frame) and its velocity
    pub c: DVec3,
    pub v: DVec3,
    /// heading (rad from north) and turn rate (rad/s)
    heading: f64,
    turn: f64,
    /// height above the ground it wants, and the ground under it (m)
    agl_goal: f64,
    ground: f64,
    /// buzzards: circling up a thermal (true) or gliding to the next
    climbing: bool,
    /// murmuration: the point over the roost it wheels about
    anchor: DVec3,
    pub birds: Vec<Bird>,
    age: f64,
    seed: f64,
}

/// What a strike was: how fast the bird hit (m/s), what it was, and where
/// on the aircraft (body frame: where it entered the aircraft's box).
#[derive(Clone, Copy, Debug)]
pub struct Strike {
    pub speed: f64,
    pub kind: Kind,
    pub at: Vec3,
}

/// The aircraft, for the birds: where, how fast, which way, and its box (body
/// frame centre and half size, main.rs `plane_box`).
#[derive(Clone, Copy, Debug)]
pub struct Aircraft<'a> {
    pub pos: DVec3,
    pub vel: DVec3,
    pub rot: DQuat,
    /// its shape, for the strike test (`Shape::of`)
    pub shape: &'a Shape,
    /// false once crashed: no strikes, no fleeing from it
    pub flying: bool,
}

/// An aircraft's shape for the strike test, in its body frame: the fuselage
/// as balls along its length, the wings, fin and stabiliser as thin panels
/// (corners in order, half thickness), the engines as balls.
#[derive(Clone, Debug, Default)]
pub struct Shape {
    pub balls: Vec<(Vec3, f32)>,
    pub panels: Vec<([Vec3; 4], f32)>,
}

impl Shape {
    /// From the aircraft's geometry (aircraft/mod.rs `Spec`): the fuselage
    /// sections from nose to tail, the wing sections from root to tip (their
    /// leading and trailing edges, damage.rs `wing_pt`), the fin and the
    /// stabiliser from their contact points (chords estimated: a tenth and
    /// a fourteenth of the length), and the engine pods (or propeller) as
    /// balls about 3% of the span across.
    pub fn of(spec: &crate::aircraft::Spec) -> Shape {
        let mut sh = Shape::default();
        let (nose, tail) = (spec.hard[0].z as f32, spec.hard[3].z as f32);
        let mut z = nose;
        while z < tail {
            let (top, _, bottom) = (spec.fuse)(z);
            let r = ((top - bottom) * 0.5).max(0.15);
            sh.balls.push((Vec3::new(0.0, (top + bottom) * 0.5, z), r));
            z += (r * 0.7).max(0.1);
        }
        const N: usize = 8;
        for s in 0..2 {
            for i in 0..N {
                let (a, b) = (spec.span * i as f32 / N as f32, spec.span * (i + 1) as f32 / N as f32);
                let k = (spec.wing_sect)(a);
                let half = (0.06 * k.c * k.tf).max(0.03);
                let p = |st: f32, xc: f32| crate::damage::wing_pt(spec, s, st, xc, 0.0);
                sh.panels.push(([p(a, 0.0), p(b, 0.0), p(b, 1.0), p(a, 1.0)], half));
            }
        }
        let len = tail - nose;
        let t = &spec.tail;
        let (fb, ft) = (t.fin_base.as_vec3(), t.fin_tip.as_vec3());
        let fc = 0.1 * len;
        let back = Vec3::Z * (fc * 0.5);
        sh.panels.push(([fb - back, ft - back * 0.6, ft + back * 0.6, fb + back], 0.04 * fc));
        let st = t.stab_tip.as_vec3();
        let sc = 0.07 * len;
        let back = Vec3::Z * (sc * 0.5);
        for side in [-1.0f32, 1.0] {
            let tip = Vec3::new(st.x * side, st.y, st.z);
            let root = Vec3::new(0.0, st.y, st.z);
            sh.panels.push(([root - back, tip - back * 0.6, tip + back * 0.6, root + back], 0.04 * sc));
        }
        let pod = spec.hard[6].as_vec3();
        let r = (0.03 * spec.span * 2.0).max(0.3);
        sh.balls.push((pod + Vec3::Y * r * 0.8, r));
        if pod.x.abs() > 0.5 {
            sh.balls.push((Vec3::new(-pod.x, pod.y + r * 0.8, pod.z), r));
        }
        sh
    }

    /// Where a bird (radius `r`) going from a to b (body frame) first meets
    /// the aircraft, if it does.
    pub fn hit(&self, a: DVec3, b: DVec3, r: f64) -> Option<DVec3> {
        let mut best: Option<(f64, DVec3)> = None;
        let mut keep = |t: f64, p: DVec3| {
            if best.is_none_or(|(bt, _)| t < bt) {
                best = Some((t, p));
            }
        };
        let d = b - a;
        for &(c, rad) in &self.balls {
            let c = c.as_dvec3();
            let t = ((c - a).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
            let q = a + d * t;
            if q.distance(c) < rad as f64 + r {
                keep(t, q);
            }
        }
        for (pts, half) in &self.panels {
            let p = pts.map(|v| v.as_dvec3());
            for tri in [[p[0], p[1], p[2]], [p[0], p[2], p[3]]] {
                if let Some((t, q)) = segment_slab(a, b, tri, *half as f64 + r) {
                    keep(t, q);
                }
            }
        }
        best.map(|(_, p)| p)
    }
}

/// Where along a to b (0..1) the segment first comes within `h` of a
/// triangle (as a thin slab), and the point.
fn segment_slab(a: DVec3, b: DVec3, tri: [DVec3; 3], h: f64) -> Option<(f64, DVec3)> {
    let n = (tri[1] - tri[0]).cross(tri[2] - tri[0]);
    if n.length_squared() < 1e-12 {
        return None;
    }
    let n = n.normalize();
    let (da, db) = ((a - tri[0]).dot(n), (b - tri[0]).dot(n));
    if (da > h && db > h) || (da < -h && db < -h) {
        return None;
    }
    // The first point inside the slab (or where it crosses the mid plane).
    let t = if da.abs() <= h || (da - db).abs() < 1e-12 { 0.0 } else { ((da - h * da.signum()) / (da - db)).clamp(0.0, 1.0) };
    let q = a + (b - a) * t;
    let qp = q - n * (q - tri[0]).dot(n);
    // Inside the triangle, with the bird's size as a margin along the edges.
    let inside = (0..3).all(|k| {
        let (e0, e1) = (tri[k], tri[(k + 1) % 3]);
        let edge = e1 - e0;
        let out = edge.cross(n).normalize_or_zero();
        (qp - e0).dot(out) <= h
    });
    inside.then_some((t, q))
}

/// The world, for the birds.
pub struct Env<'a> {
    pub place: &'a dyn Fn(DVec3) -> Option<Place>,
    /// ground height (m) under a point, if known
    pub ground: &'a dyn Fn(DVec3) -> Option<f64>,
    /// rising air (m/s) at a point and height above the ground
    pub thermal: &'a dyn Fn(DVec3, f64) -> f64,
    /// wind (planet frame, m/s) at a point and height above the ground
    pub wind: &'a dyn Fn(DVec3, f64) -> DVec3,
}

/// Flocks are kept between these distances from the camera (m).
const SPAWN_NEAR: f64 = 1300.0;
const SPAWN_FAR: f64 = 3500.0;
const GONE: f64 = 5500.0;
/// How often a new flock is looked for (s).
const SPAWN_EVERY: f64 = 1.5;
/// How far ahead (s) a bird sees the aircraft coming, and how close it has to
/// come before the bird dives (m).
const SEE_AHEAD: f64 = 4.0;
const SEE_NEAR: f64 = 40.0;
const G: f64 = 9.81;

pub struct Birds {
    pub flocks: Vec<Flock>,
    rng: Rng,
    next_spawn: f64,
    time: f64,
    /// flocks asked for on the command line: kind, km, bearing (deg), height
    /// above the ground (m), placed once the ground there is known
    pub forced: Vec<(Kind, f64, f64, Option<f64>)>,
    /// where the camera was last time, and how long it has been since it
    /// jumped (a respawn, a new start): flocks left behind are dropped
    last_cam: Option<DVec3>,
    settled: f64,
}

impl Default for Birds {
    fn default() -> Self {
        Self { flocks: Vec::new(), rng: Rng(0x9E37_79B9_7F4A_7C15), next_spawn: 0.5, time: 0.0, forced: Vec::new(), last_cam: None, settled: 0.0 }
    }
}

fn up_at(p: DVec3) -> DVec3 {
    p.normalize()
}

fn alt(p: DVec3) -> f64 {
    p.length() - geo::R
}

/// Unit vector `deg` degrees from north at p, on the tangent plane.
fn bearing(p: DVec3, deg: f64) -> DVec3 {
    let (e, n, _) = geo::enu(p);
    let a = deg.to_radians();
    e * a.sin() + n * a.cos()
}

impl Birds {
    /// With these flocks asked for (kind, km, bearing in degrees, height above the ground).
    pub fn with_flocks(forced: Vec<(Kind, f64, f64, Option<f64>)>) -> Self {
        Self { forced, ..Default::default() }
    }

    /// The nearest bird to `p` (m), for the log.
    pub fn nearest(&self, p: DVec3) -> f64 {
        self.flocks.iter().flat_map(|f| f.birds.iter()).map(|b| b.p.distance(p)).fold(f64::INFINITY, f64::min)
    }

    pub fn count(&self) -> usize {
        self.flocks.iter().map(|f| f.birds.len()).sum()
    }

    /// Starts a flock of `kind` at `c` (planet frame, at the height wanted).
    fn spawn(&mut self, kind: Kind, c: DVec3, place: &Place, murm: bool, ground: f64, birds_left: usize) -> Option<Flock> {
        let sp = kind.spec();
        let r = &mut self.rng;
        let (lo, hi) = if murm { (60.0, 180.0) } else { sp.agl };
        let season = migrating(place);
        let on_the_move = kind == Kind::Goose && season.is_some() && r.f() < 0.8;
        let (lo, hi) = if kind == Kind::Goose && !on_the_move { (40.0, 250.0) } else { (lo, hi) };
        let agl_goal = if kind == Kind::Buzzard { r.range(120.0, 300.0) } else { r.range(lo, (lo + hi) * 0.5) };
        let n = if murm { r.int(200, 600) } else { r.int(sp.flock.0, sp.flock.1) }.min(birds_left);
        if n == 0 {
            return None;
        }
        let heading = if on_the_move { migration_heading(place, season == Some(true)).to_radians() + r.range(-0.3, 0.3) } else { r.range(0.0, std::f64::consts::TAU) };
        let c = c.normalize() * (geo::R + ground + agl_goal);
        let (e, nn, u) = geo::enu(c);
        let fwd = e * heading.sin() + nn * heading.cos();
        let v = if kind == Kind::Buzzard || murm { DVec3::ZERO } else { fwd * sp.speed };
        let mut birds = Vec::with_capacity(n);
        // (Geese: one arm of the V is usually longer than the other.)
        let long_left = r.f() < 0.5;
        for i in 0..n {
            let slot = match kind {
                Kind::Goose if on_the_move => v_slot(i, sp.span, long_left),
                Kind::Buzzard => DVec3::new(r.range(-30.0, 30.0), r.range(-30.0, 30.0), r.range(-15.0, 15.0)),
                _ => {
                    let rad = if murm { 90.0 } else { 4.0 + 3.0 * (n as f64).sqrt() };
                    DVec3::new(r.range(-rad, rad), r.range(-rad, rad), r.range(-0.35, 0.35) * rad)
                }
            };
            let p = c + e * slot.x + nn * slot.y + u * slot.z;
            birds.push(Bird {
                p,
                v: if v.length() > 0.0 { v } else { fwd * sp.speed },
                slot,
                phase: r.range(0.0, std::f64::consts::TAU),
                flapping: r.f() > sp.glide,
                bout: r.range(0.0, 2.0),
                bank: 0.0,
                flee: 0.0,
                flee_v: DVec3::ZERO,
                alert: r.range(0.55, 1.0),
                noticed: false,
                prev_rel: None,
                alive: true,
            });
        }
        Some(Flock { kind, murmuration: murm, migrating: on_the_move, steady: on_the_move, c, v, heading, turn: 0.0, agl_goal, ground, climbing: true, anchor: c, birds, age: 0.0, seed: r.f() })
    }

    /// Advances the birds by `dt` seconds around the camera `cam`; returns
    /// the strikes on the aircraft.
    pub fn step(&mut self, dt: f64, cam: DVec3, level: Level, plane: &Aircraft, env: &Env) -> Vec<Strike> {
        let mut strikes = Vec::new();
        if dt <= 0.0 {
            return strikes;
        }
        self.time += dt;
        // The camera jumped (a respawn, a new place): start afresh there.
        if self.last_cam.is_some_and(|p| p.distance(cam) > 1000.0) {
            self.flocks.clear();
            self.settled = 0.0;
        }
        self.last_cam = Some(cam);
        self.settled += dt;
        let (max_flocks, max_birds) = caps(level);
        // Flocks too far away go; so does everything when the birds are off.
        self.flocks.retain(|f| max_flocks > 0 && f.c.distance(cam) < GONE && f.birds.iter().any(|b| b.alive));
        self.next_spawn -= dt;
        if self.next_spawn <= 0.0 {
            self.next_spawn = SPAWN_EVERY;
            self.try_spawn(cam, max_flocks, max_birds, plane, env);
        }
        let t = self.time;
        for f in &mut self.flocks {
            step_flock(f, dt, t, plane, env, &mut self.rng, &mut strikes);
        }
        strikes
    }

    fn try_spawn(&mut self, cam: DVec3, max_flocks: usize, max_birds: usize, plane: &Aircraft, env: &Env) {
        let used = self.count();
        // Flocks asked for: placed as soon as the ground under them is known.
        if let Some(&(kind, km, brg, agl)) = self.forced.first().filter(|_| self.settled > 2.0) {
            // (No bearing: on the aircraft's path, climbing or descending with it.)
            let on_path = brg.is_nan() && plane.vel.length() > 1.0;
            let c = if on_path { plane.pos + plane.vel.normalize() * km * 1000.0 } else { cam.normalize() * geo::R + bearing(cam, brg) * km * 1000.0 };
            if let (Some(g), Some(place)) = ((env.ground)(c), (env.place)(c)) {
                let murm = kind == Kind::Starling && murmuration_time(&place);
                if let Some(mut f) = self.spawn(kind, c, &place, murm, g, 800) {
                    // At the height asked for, else at the aircraft's (or on its path).
                    lift_flock(&mut f, agl.unwrap_or(if on_path { alt(c) } else { alt(plane.pos) } - g));
                    // (Flying the aircraft's way, so that it catches them up
                    // slowly: these are for screenshots.)
                    let (e, n, _) = geo::enu(f.c);
                    if plane.vel.length() > 1.0 && f.kind != Kind::Buzzard && !f.murmuration {
                        f.heading = plane.vel.dot(e).atan2(plane.vel.dot(n));
                        (f.turn, f.steady) = (0.0, true);
                        let v = (e * f.heading.sin() + n * f.heading.cos()) * f.kind.spec().speed;
                        for b in &mut f.birds {
                            b.v = v;
                        }
                    }
                    log::info!("birds: {} {} placed {km} km {}, {:.0} m above the ground", f.birds.len(), kind.spec().name, if on_path { "ahead on the aircraft's path".to_string() } else { format!("away at {brg:.0}°") }, alt(f.c) - g);
                    self.flocks.push(f);
                }
                self.forced.remove(0);
            }
            return;
        }
        if self.flocks.len() >= max_flocks || used >= max_birds {
            return;
        }
        let r = &mut self.rng;
        let dist = r.range(SPAWN_NEAR, SPAWN_FAR);
        let c = cam.normalize() * geo::R + bearing(cam, r.range(0.0, 360.0)) * dist;
        let (Some(g), Some(place)) = ((env.ground)(c), (env.place)(c)) else { return };
        let (w, murm) = weights(&place);
        let total: f64 = w.iter().sum();
        // A quiet spot now and then: the busier the place, the more flocks.
        if total < 0.05 || self.rng.f() > (total / 2.0).min(1.0) {
            return;
        }
        let mut pick = self.rng.f() * total;
        let mut kind = Kind::Crow;
        for k in Kind::ALL {
            if pick < w[k as usize] {
                kind = k;
                break;
            }
            pick -= w[k as usize];
        }
        // One murmuration at a time is plenty.
        let murm = murm && kind == Kind::Starling && !self.flocks.iter().any(|f| f.murmuration);
        // Buzzards start where a thermal is.
        let c = if kind == Kind::Buzzard { find_thermal(c, 150.0, env).unwrap_or(c) } else { c };
        if let Some(f) = self.spawn(kind, c, &place, murm, g, max_birds - used) {
            self.flocks.push(f);
        }
    }

    /// The instanced solids for the birds in view of the camera at `cam`:
    /// `px` is the angle a pixel covers (rad), `visible` a frustum test
    /// (camera-relative centre, radius).
    pub fn instances(&self, cam: DVec3, px: f64, visible: &dyn Fn(DVec3, f64) -> bool, out: &mut Vec<(u32, Inst)>) {
        let m = models();
        for f in &self.flocks {
            let sp = f.kind.spec();
            let far = if f.murmuration { 3000.0 } else { sp.far };
            if f.c.distance(cam) > far + 400.0 {
                continue;
            }
            for b in f.birds.iter().filter(|b| b.alive) {
                let rel = b.p - cam;
                let d = rel.length();
                if d > far || !visible(rel, sp.span) {
                    continue;
                }
                let (base, near) = frame(b, rel, d, sp, px, m.reach[f.kind as usize]);
                if !near {
                    out.push((mesh_id(f.kind, 5), inst(base)));
                    continue;
                }
                out.push((mesh_id(f.kind, 0), inst(base)));
                let (a_in, a_out) = wing_angles(b, sp, f.kind == Kind::Buzzard && f.climbing);
                let piv = &m.pivots[f.kind as usize];
                // Left wing up is a turn about the backward axis by minus the angle.
                for (side, sign) in [(0usize, -1.0f32), (2, 1.0)] {
                    let inner = base * hinge(piv[side], sign * a_in as f32);
                    let outer = inner * hinge(piv[side + 1], sign * a_out as f32);
                    out.push((mesh_id(f.kind, 1 + side), inst(inner)));
                    out.push((mesh_id(f.kind, 2 + side), inst(outer)));
                }
            }
        }
    }
}

/// Raises a placed flock to `agl` metres above its ground.
fn lift_flock(f: &mut Flock, agl: f64) {
    let dz = agl - f.agl_goal;
    f.agl_goal = agl;
    let u = up_at(f.c);
    f.c += u * dz;
    for b in &mut f.birds {
        b.p += u * dz;
    }
}

/// A goose's place in the V: behind the leader along one arm or the other,
/// each about 1.4 spans further back and out (half-angle about 30°).
pub fn v_slot(i: usize, span: f64, long_left: bool) -> DVec3 {
    if i == 0 {
        return DVec3::ZERO;
    }
    // Every third bird joins the short arm, the others the long one; each
    // arm's rank counts only the birds on it.
    let long = !i.is_multiple_of(3);
    let rank = if long { (i - i / 3) as f64 } else { (i / 3) as f64 };
    let left = long == long_left;
    let step = 1.4 * span;
    let half = 30f64.to_radians();
    let side = if left { -1.0 } else { 1.0 };
    // (East = right, north = forward in the flock's own frame; turned to its heading later.)
    DVec3::new(side * rank * step * half.sin(), -rank * step * half.cos(), 0.15 * rank)
}

/// Looks for a working thermal within about a kilometre of `c` at `agl`.
fn find_thermal(c: DVec3, agl: f64, env: &Env) -> Option<DVec3> {
    let mut best = (0.4, None);
    for k in 0..12 {
        let p = c + bearing(c, k as f64 * 30.0) * (if k % 2 == 0 { 600.0 } else { 1100.0 });
        let w = (env.thermal)(p, agl);
        if w > best.0 {
            best = (w, Some(p));
        }
    }
    best.1
}

fn step_flock(f: &mut Flock, dt: f64, t: f64, plane: &Aircraft, env: &Env, rng: &mut Rng, strikes: &mut Vec<Strike>) {
    let sp = f.kind.spec();
    f.age += dt;
    let (e, n, u) = geo::enu(f.c);
    if let Some(g) = (env.ground)(f.c) {
        f.ground = g;
    }
    let agl = alt(f.c) - f.ground;
    let wind = (env.wind)(f.c, agl.max(5.0));
    // The flock's centre.
    let climb;
    if f.kind == Kind::Buzzard {
        let lift = (env.thermal)(f.c, agl);
        if f.climbing && (lift < 0.3 || agl > 900.0) {
            // Topped out, or the thermal died: glide off to the next one.
            f.climbing = false;
            f.heading = rng.range(0.0, std::f64::consts::TAU);
        } else if !f.climbing && lift > 0.8 && agl < 800.0 {
            f.climbing = true;
        }
        if f.climbing {
            // Circling up: the centre drifts with the wind; climb = lift - sink.
            f.v = wind;
            climb = (lift - 0.9).max(-0.5);
        } else {
            f.v = (e * f.heading.sin() + n * f.heading.cos()) * 12.0 + wind;
            climb = if agl < 120.0 { 0.5 } else { -1.0 };
            if let Some(p) = find_thermal(f.c, agl, env).filter(|_| agl < 120.0) {
                let d = p - f.c;
                f.heading = d.dot(e).atan2(d.dot(n));
            }
        }
    } else if f.murmuration {
        // Wheeling about the roost, the whole flock swinging back and forth.
        let a = t * 0.11 + f.seed * 10.0;
        let goal = f.anchor + e * (180.0 * a.sin()) + n * (120.0 * (a * 1.7).sin());
        let to = goal - f.c;
        let flat = to - u * to.dot(u);
        f.v = flat.clamp_length_max(14.0) + wind * 0.3;
        climb = ((f.agl_goal + 40.0 * (t * 0.23 + f.seed).sin()) - agl).clamp(-4.0, 4.0) * 0.4;
    } else {
        // A slow random walk of the heading; migrating geese keep theirs.
        let wander = if f.steady { 0.02 } else { 0.25 };
        f.turn = (f.turn + rng.range(-1.0, 1.0) * wander * dt).clamp(-wander, wander) * (1.0 - 0.2 * dt);
        f.heading += f.turn * dt;
        if !f.steady && rng.f() < dt / 25.0 {
            f.agl_goal = rng.range(sp.agl.0, (sp.agl.0 + sp.agl.1) * 0.5);
        }
        f.v = (e * f.heading.sin() + n * f.heading.cos()) * sp.speed + wind;
        climb = ((f.agl_goal - agl) * 0.25).clamp(-3.0, 3.0);
    }
    // Never into the ground.
    let climb = if agl < 8.0 { climb.max(3.0) } else { climb };
    f.c += (f.v + u * climb) * dt;
    // The members.
    let fwd_h = e * f.heading.sin() + n * f.heading.cos();
    let right_h = fwd_h.cross(u);
    let formation = f.kind == Kind::Goose && f.migrating;
    let speed = sp.speed;
    let to_plane = plane.rot.inverse();
    let n_birds = f.birds.len();
    for i in 0..n_birds {
        let target = {
            let b = &f.birds[i];
            if !b.alive {
                continue;
            }
            match f.kind {
                Kind::Buzzard if f.climbing => {
                    // Each its own circle, 25-55 m across, over the thermal.
                    let r = 25.0 + 30.0 * ((b.slot.x + 30.0) / 60.0);
                    let w = speed / r * if b.slot.y > 0.0 { 1.0 } else { -1.0 };
                    let a = t * w + b.slot.y;
                    f.c + (e * a.cos() + n * a.sin()) * r + u * b.slot.z
                }
                _ => {
                    let s = if formation {
                        right_h * b.slot.x + fwd_h * b.slot.y + u * b.slot.z
                    } else if f.murmuration {
                        murmur_slot(b.slot, t, f.seed, e, n, u)
                    } else {
                        // A loose flock: each keeps near its place, which wanders a little.
                        let w = (t * 0.3 + b.slot.x * 0.1).sin();
                        e * (b.slot.x + 3.0 * w) + n * (b.slot.y + 3.0 * (t * 0.27 + b.slot.y).cos()) + u * b.slot.z
                    };
                    f.c + s
                }
            }
        };
        // Never below a few metres off the ground.
        let tg = alt(target) - f.ground;
        let target = if tg < 4.0 { target + u * (4.0 - tg) } else { target };
        let b = &mut f.birds[i];
        let mut want = (target - b.p) * 0.6 + f.v + u * climb;
        if alt(b.p) - f.ground < 3.0 {
            want += u * 4.0;
        }
        // Flocking birds keep a little apart from one neighbour.
        let j = (i + 1 + (t * 3.0) as usize) % n_birds;
        if j != i && f.kind != Kind::Buzzard {
            let other = f.birds[j].p;
            let b = &mut f.birds[i];
            let d = b.p - other;
            let gap = sp.span * if f.murmuration { 1.5 } else { 2.5 };
            if d.length() < gap && d.length() > 1e-3 {
                want += d.normalize() * (gap - d.length()) * 3.0;
            }
        }
        let b = &mut f.birds[i];
        // The aircraft: dive away if it is coming close.
        if plane.flying {
            let r = b.p - plane.pos;
            let rv = plane.vel - b.v;
            let closing = r.dot(rv) / r.length().max(1e-3);
            let tca = (r.dot(rv) / rv.length_squared().max(1e-6)).max(0.0);
            let miss = (r - rv * tca).length();
            let threat = r.length() < SEE_NEAR || (closing > 0.0 && tca < SEE_AHEAD && miss < 25.0);
            if threat && !b.noticed {
                // One chance per approach: it dives, or it never sees it coming.
                b.noticed = true;
                if rng.f() < b.alert {
                    // Away from the aircraft's path: diving if it is at or
                    // below it, mostly sideways if it is above (a dive would
                    // take it across the path).
                    let away = (r - rv.normalize_or_zero() * r.dot(rv.normalize_or_zero())).normalize_or_zero();
                    let above = away.dot(u) > 0.3;
                    b.flee_v = (away * 0.8 + u * if above { 0.2 } else { -1.0 }).normalize_or_zero() * speed * 1.3;
                    b.flee = 1.8;
                }
            } else if !threat && r.length() > 300.0 {
                b.noticed = false;
            }
        }
        if b.flee > 0.0 {
            b.flee -= dt;
            want = b.flee_v + wind;
        }
        // Steer: limited acceleration, about the cruising airspeed.
        let air = want - wind;
        let air = air.clamp_length(speed * 0.6, speed * 1.5);
        let goal = air + wind;
        let dv = (goal - b.v).clamp_length_max(sp.accel * dt * if b.flee > 0.0 { 2.0 } else { 1.0 });
        let v0 = b.v;
        b.v += dv;
        let p0 = b.p;
        b.p += b.v * dt;
        // Bank into the turn: a = v x dv / |v|, tan(bank) = a / g.
        let fwd = (b.v - wind).normalize_or_zero();
        let lat = (b.v - v0).dot(fwd.cross(up_at(b.p))) / dt;
        let goal_bank = (lat / G).atan().clamp(-1.0, 1.0);
        b.bank += (goal_bank - b.bank) * (dt * 4.0).min(1.0);
        // Wingbeats: bouts of flapping and gliding, more flapping to climb.
        b.bout -= dt;
        if b.bout <= 0.0 {
            let climbing = (b.v - v0).dot(up_at(b.p)) > 0.0 || climb > 1.0;
            b.flapping = if f.kind == Kind::Buzzard { !f.climbing && rng.f() < 0.15 } else { climbing || rng.f() > sp.glide };
            b.bout = if b.flapping { rng.range(1.0, 4.0) } else { rng.range(1.0, 4.0) * sp.glide / (1.0 - sp.glide).max(0.1) };
        }
        if b.flapping || b.flee > 0.0 {
            b.phase = (b.phase + std::f64::consts::TAU * sp.flap_hz * dt) % std::f64::consts::TAU;
        }
        // Struck by the aircraft?
        if plane.flying && b.p.distance(plane.pos) < 200.0 {
            let rel = to_plane * (b.p - plane.pos);
            let prev = b.prev_rel.unwrap_or(to_plane * (p0 - plane.pos));
            if let Some(at) = plane.shape.hit(prev, rel, sp.span * 0.2) {
                b.alive = false;
                strikes.push(Strike { speed: (plane.vel - b.v).length(), kind: f.kind, at: at.as_vec3() });
            }
            b.prev_rel = Some(rel);
        } else {
            b.prev_rel = None;
        }
    }
    f.birds.retain(|b| b.alive);
}

/// A starling's place in a murmuration: its spot in a loose ball, swept by
/// slow folds and turns of the whole shape, so waves run through it.
fn murmur_slot(s: DVec3, t: f64, seed: f64, e: DVec3, n: DVec3, u: DVec3) -> DVec3 {
    let a = t * 0.35 + seed * 7.0;
    // Squashed into a sheet that tilts and turns, swelling and thinning.
    let sx = s.x * (1.0 + 0.6 * (a * 0.8).sin());
    let sy = s.y * (0.5 + 0.4 * (a * 0.6 + 1.0).cos());
    let sz = s.z * 1.6 + 25.0 * ((s.x * 0.03 + a).sin());
    let yaw = a * 0.4;
    let (cy, syw) = (yaw.cos(), yaw.sin());
    let x = sx * cy - sy * syw;
    let y = sx * syw + sy * cy;
    // A wave travelling across it.
    let wave = 12.0 * (x * 0.04 - t * 1.6).sin();
    e * x + n * y + u * (sz + wave)
}

/// The wings' angles (rad, tip up): the inner segment at the shoulder, the
/// outer one at the wrist relative to it.
fn wing_angles(b: &Bird, sp: &Species, soaring: bool) -> (f64, f64) {
    if b.flee > 0.0 {
        // Diving: wings half folded, tips swept down.
        return (0.45, -0.9);
    }
    if !b.flapping || soaring {
        return (sp.dihedral, 0.04);
    }
    flap(b.phase, sp)
}

/// Flapping: the hand follows the arm a little behind, so the wing bends on
/// each stroke.
pub fn flap(phase: f64, sp: &Species) -> (f64, f64) {
    let a = sp.amp * phase.sin() + 0.1;
    let o = 0.55 * sp.amp * (phase - 0.8).sin();
    (a, o)
}

/// The body's model matrix (camera-relative) and whether it is near enough
/// to draw with moving wings. It is drawn no smaller than about three pixels
/// across, so flocks stay visible as specks.
fn frame(b: &Bird, rel: DVec3, d: f64, sp: &Species, px: f64, reach: f32) -> (Mat4, bool) {
    let up0 = up_at(b.p);
    let fwd = if b.v.length_squared() > 1e-6 { b.v.normalize() } else { up0.any_orthonormal_vector() };
    let right = fwd.cross(up0);
    let right = if right.length_squared() > 1e-9 { right.normalize() } else { up0.any_orthonormal_vector() };
    let up = right.cross(fwd);
    let q = DQuat::from_axis_angle(fwd, b.bank);
    let (right, up) = (q * right, q * up);
    let near = d < 320.0 * sp.span;
    // (The eye picks out a bird a screen cannot draw: never under about three
    // pixels across, or the anti-aliasing all but wipes it out.)
    let scale = (3.0 * px * d / (2.0 * reach.max(0.05) as f64)).clamp(1.0, 8.0);
    let m = Mat4::from_cols(
        (right * scale).as_vec3().extend(0.0),
        (up * scale).as_vec3().extend(0.0),
        (-fwd * scale).as_vec3().extend(0.0),
        rel.as_vec3().extend(1.0),
    );
    (m, near)
}

/// A turn of `a` radians about the body's backward axis through `pivot`.
fn hinge(pivot: Vec3, a: f32) -> Mat4 {
    Mat4::from_translation(pivot) * Mat4::from_rotation_z(a) * Mat4::from_translation(-pivot)
}

fn inst(m: Mat4) -> Inst {
    let c = m.to_cols_array_2d();
    Inst { m: [[c[0][0], c[1][0], c[2][0], c[3][0]], [c[0][1], c[1][1], c[2][1], c[3][1]], [c[0][2], c[1][2], c[2][2], c[3][2]]], tint: [1.0, 1.0, 1.0, 0.0], ext: [0.0; 4] }
}

/// How hard a strike is, for the thud (0.3..2): the bird's kinetic energy
/// relative to the aircraft.
pub fn thud(s: &Strike) -> f32 {
    let e = 0.5 * s.kind.spec().mass * s.speed * s.speed;
    (0.3 + (e / 1500.0).sqrt()).min(2.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place() -> Place {
        Place { lat: 50.0, doy: 160.0, hour: 13.0, sun: 0.8, water: 0.0, convection: 0.8, rain: 0.0 }
    }

    #[test]
    fn models_are_at_real_scale() {
        let m = models();
        assert_eq!(m.meshes.len(), Kind::ALL.len() * PIECES.len());
        for k in Kind::ALL {
            let sp = k.spec();
            let span = 2.0 * m.reach[k as usize] as f64;
            assert!((span / sp.span - 1.0).abs() < 0.05, "{}: {span:.3} m vs {}", sp.name, sp.span);
            // The hinges: left ones on the left, the wrist outboard of the shoulder.
            let p = m.pivots[k as usize];
            assert!(p[0].x < 0.0 && p[1].x < p[0].x && p[2].x > 0.0 && p[3].x > p[2].x, "{}: {p:?}", sp.name);
            assert!((p[0].x + p[2].x).abs() < 1e-4);
            // Plain diffuse (trim) material everywhere.
            assert!(m.meshes[k as usize * PIECES.len()].0.iter().all(|v| v.col[3] == 1.0));
        }
        assert_eq!(mesh_id(Kind::Gull, 0), BASE);
        assert_eq!(mesh_id(Kind::Goose, 5), BASE + 29);
    }

    #[test]
    fn species_by_place_and_time() {
        let p = place();
        let (w, _) = weights(&p);
        // Inland on a sunny summer afternoon: crows and buzzards, few gulls, no migrating geese.
        assert!(w[Kind::Crow as usize] > w[Kind::Gull as usize]);
        assert!(w[Kind::Buzzard as usize] > 0.5);
        assert_eq!(w[Kind::Goose as usize], 0.0);
        // On the coast, gulls lead.
        let (w, _) = weights(&Place { water: 0.5, ..p });
        assert!(w[Kind::Gull as usize] > w[Kind::Crow as usize]);
        // Over open sea: no crows.
        assert_eq!(weights(&Place { water: 0.95, ..p }).0[Kind::Crow as usize], 0.0);
        // At night, nothing.
        assert_eq!(weights(&Place { sun: -0.2, ..p }).0, [0.0; 5]);
        // No thermals in the morning or under overcast: no buzzards.
        assert_eq!(weights(&Place { hour: 8.0, ..p }).0[Kind::Buzzard as usize], 0.0);
        assert_eq!(weights(&Place { convection: 0.0, ..p }).0[Kind::Buzzard as usize], 0.0);
        // Rain keeps most of them down.
        let dry: f64 = weights(&p).0.iter().sum();
        let wet: f64 = weights(&Place { rain: 1.0, ..p }).0.iter().sum();
        assert!(wet < 0.2 * dry);
    }

    #[test]
    fn seasons() {
        let p = place();
        // Geese migrate in October (autumn, south-west) and March (spring, north-east), not in June.
        assert_eq!(migrating(&Place { doy: 290.0, ..p }), Some(true));
        assert_eq!(migrating(&Place { doy: 75.0, ..p }), Some(false));
        assert_eq!(migrating(&p), None);
        assert!(weights(&Place { doy: 290.0, ..p }).0[Kind::Goose as usize] > 1.0);
        assert_eq!(migration_heading(&p, true), 215.0);
        // The southern hemisphere: half a year out, and they fly the other way.
        assert_eq!(migrating(&Place { lat: -40.0, doy: 110.0, ..p }), Some(true));
        assert_eq!(migration_heading(&Place { lat: -40.0, ..p }, true), 20.0);
        // Murmurations: November dusk, not a June dusk nor a November noon.
        let dusk = Place { doy: 320.0, hour: 16.5, sun: 0.05, ..p };
        assert!(murmuration_time(&dusk));
        assert!(weights(&dusk).1);
        assert!(!murmuration_time(&Place { doy: 170.0, ..dusk }));
        assert!(!murmuration_time(&Place { hour: 12.0, sun: 0.4, ..dusk }));
    }

    #[test]
    fn flapping_bends_the_wing() {
        let sp = Kind::Gull.spec();
        let (up, _) = flap(std::f64::consts::FRAC_PI_2, sp);
        let (down, _) = flap(-std::f64::consts::FRAC_PI_2, sp);
        assert!(up > 0.5 && down < -0.4, "{up} {down}");
        // The hand lags the arm: at the top of the stroke it is still coming up.
        let (_, o) = flap(std::f64::consts::FRAC_PI_2, sp);
        assert!(o > 0.0 && o < sp.amp);
        // Gliding and diving poses.
        let mut b = Birds::default();
        let pl = place();
        let c = geo::lla_to_pos(50f64.to_radians(), 5f64.to_radians(), 0.0);
        let f = b.spawn(Kind::Buzzard, c, &pl, false, 0.0, 10).unwrap();
        let mut bird = f.birds[0].clone();
        bird.flapping = false;
        assert_eq!(wing_angles(&bird, Kind::Buzzard.spec(), true), (Kind::Buzzard.spec().dihedral, 0.04));
        bird.flee = 1.0;
        assert!(wing_angles(&bird, Kind::Buzzard.spec(), true).1 < -0.5);
    }

    #[test]
    fn geese_fly_in_a_v() {
        let span = Kind::Goose.spec().span;
        assert_eq!(v_slot(0, span, true), DVec3::ZERO);
        let slots: Vec<_> = (1..12).map(|i| v_slot(i, span, true)).collect();
        // All behind the leader, on both arms, the long arm longer.
        assert!(slots.iter().all(|s| s.y < 0.0));
        let left = slots.iter().filter(|s| s.x < 0.0).count();
        assert!(left > slots.len() - left && slots.len() - left >= 3, "{left}");
        // About 30° off the line of flight, no two on the same spot.
        for s in &slots {
            let ang = (s.x.abs() / -s.y).atan().to_degrees();
            assert!((ang - 30.0).abs() < 1.0, "{ang}");
        }
        for (i, a) in slots.iter().enumerate() {
            for b in &slots[i + 1..] {
                assert!(a.distance(*b) > span, "{a} {b}");
            }
        }
    }

    type FlatEnv = (Box<dyn Fn(DVec3) -> Option<Place>>, Box<dyn Fn(DVec3) -> Option<f64>>, Box<dyn Fn(DVec3, f64) -> f64>, Box<dyn Fn(DVec3, f64) -> DVec3>);
    /// Flat ground `h` metres up, the sunny summer afternoon of `place`, no wind, no thermals.
    fn env_flat(h: f64) -> FlatEnv {
        (Box::new(|_| Some(place())), Box::new(move |_| Some(h)), Box::new(|_, _| 0.0), Box::new(|_, _| DVec3::ZERO))
    }

    #[test]
    fn birds_keep_off_the_ground_and_spawn_in_range() {
        let (pl, gr, th, wi) = env_flat(300.0);
        let env = Env { place: &*pl, ground: &*gr, thermal: &*th, wind: &*wi };
        let cam = geo::lla_to_pos(50f64.to_radians(), 5f64.to_radians(), 800.0);
        let shape = Shape::default();
        let plane = Aircraft { pos: cam, vel: DVec3::ZERO, rot: DQuat::IDENTITY, shape: &shape, flying: false };
        let mut b = Birds::default();
        for _ in 0..(120.0 / 0.05) as usize {
            b.step(0.05, cam, Level::High, &plane, &env);
        }
        assert!(!b.flocks.is_empty() && b.flocks.len() <= caps(Level::High).0);
        assert!(b.count() <= caps(Level::High).1);
        for f in &b.flocks {
            for bird in &f.birds {
                assert!(alt(bird.p) > 300.0 + 1.0, "{} at {:.1} m", f.kind.spec().name, alt(bird.p) - 300.0);
            }
            assert!(f.c.distance(cam) < GONE);
        }
        // A jump of the camera (a respawn) leaves them behind.
        let far = geo::lla_to_pos(51f64.to_radians(), 5f64.to_radians(), 800.0);
        b.step(0.05, far, Level::High, &plane, &env);
        assert!(b.flocks.is_empty());
        for _ in 0..200 {
            b.step(0.05, far, Level::High, &plane, &env);
        }
        assert!(!b.flocks.is_empty());
        // Off: none left.
        b.step(0.05, far, Level::Off, &plane, &env);
        assert!(b.flocks.is_empty());
    }

    #[test]
    fn birds_dive_out_of_the_way_and_strikes_are_found() {
        let (pl, gr, th, wi) = env_flat(0.0);
        let env = Env { place: &*pl, ground: &*gr, thermal: &*th, wind: &*wi };
        let c = geo::lla_to_pos(50f64.to_radians(), 5f64.to_radians(), 0.0);
        let (e, n, u) = geo::enu(c);
        let mut b = Birds::default();
        let mut f = b.spawn(Kind::Gull, c, &place(), false, 0.0, 1).unwrap();
        lift_flock(&mut f, 100.0);
        // Flying north, straight (the aircraft catches it up from behind).
        (f.heading, f.turn, f.steady) = (0.0, 0.0, true);
        f.birds[0].slot = DVec3::ZERO;
        f.birds[0].p = f.c;
        let bird_at = f.birds[0].p;
        // An aircraft 150 m away flying straight at it at 60 m/s, its body frame
        // facing north (-Z forward = north).
        let rot = DQuat::from_mat3(&glam::DMat3::from_cols(e, u, -n));
        let mk = |alert: f64, f: &Flock| {
            let mut f = f.clone();
            f.birds[0].alert = alert;
            f.birds[0].v = n * Kind::Gull.spec().speed;
            f
        };
        // A light aircraft: a fuselage of balls and a straight wing 10 m across.
        let shape = Shape {
            balls: (0..8).map(|k| (Vec3::new(0.0, 0.0, -3.5 + k as f32), 0.6)).collect(),
            // (A low wing, level with the bird, which wanders a few metres about its place.)
            panels: vec![([Vec3::new(-5.0, 0.0, -1.0), Vec3::new(5.0, 0.0, -1.0), Vec3::new(5.0, 0.0, 0.5), Vec3::new(-5.0, 0.0, 0.5)], 0.1)],
        };
        let plane0 = |t: f64| Aircraft { pos: bird_at - n * (150.0 - 60.0 * t), vel: n * 60.0, rot, shape: &shape, flying: true };
        // An alert bird dives clear.
        let mut alert = mk(1.0, &f);
        let mut hits = Vec::new();
        for k in 0..150 {
            step_flock(&mut alert, 0.03, k as f64 * 0.03, &plane0(k as f64 * 0.03), &env, &mut b.rng, &mut hits);
        }
        assert!(hits.is_empty(), "{hits:?}");
        assert!(alt(alert.birds[0].p) < 100.0 - 5.0, "it dived: {:.1} m", alt(alert.birds[0].p));
        // One that never sees it is struck, on the nose or the wing's leading edge.
        let mut blind = mk(0.0, &f);
        for k in 0..150 {
            step_flock(&mut blind, 0.03, k as f64 * 0.03, &plane0(k as f64 * 0.03), &env, &mut b.rng, &mut hits);
        }
        assert_eq!(hits.len(), 1);
        assert!(blind.birds.is_empty());
        let s = hits[0];
        assert!((s.at.z < -3.0 && s.at.x.abs() < 1.0) || (s.at.z < -0.9 && s.at.y.abs() < 0.5), "struck at {:?}", s.at);
        // (Caught up from behind: 60 m/s less the gull's own 11.5.)
        assert!((s.speed - 48.5).abs() < 3.0 && thud(&s) > 0.5, "{}", s.speed);
    }

    #[test]
    fn strikes_meet_the_real_shape() {
        // Every aircraft: a bird dead ahead of the nose is hit; one in the
        // empty air ahead of a swept wing's root, or above the fin, is not.
        // (Not the Starship: no wings, no fin.)
        for spec in crate::aircraft::BUILTIN.iter().filter(|s| s.keys[0] != "starship") {
            let sh = Shape::of(spec);
            assert!(!sh.balls.is_empty() && sh.panels.len() >= 19, "{}", spec.keys[0]);
            let nose = spec.hard[0];
            let ahead = nose + DVec3::new(0.0, 0.0, -3.0);
            assert!(sh.hit(ahead, nose + DVec3::new(0.0, 0.0, 2.0), 0.1).is_some(), "{}: nose", spec.keys[0]);
            // Through the wing at mid span (top to bottom), and well above everything.
            let mid = crate::damage::wing_pt(spec, 1, spec.span * 0.5, 0.4, 0.0).as_dvec3();
            assert!(sh.hit(mid + DVec3::Y * 3.0, mid - DVec3::Y * 3.0, 0.1).is_some(), "{}: wing", spec.keys[0]);
            let top = spec.hard[4].y.max(spec.tail.fin_tip.y) + 5.0;
            assert!(sh.hit(DVec3::new(0.0, top, -50.0), DVec3::new(0.0, top, 50.0), 0.1).is_none(), "{}: above", spec.keys[0]);
            // Off to the side of the wing tip.
            let tip = spec.hard[2];
            assert!(sh.hit(tip + DVec3::new(4.0, 0.0, -30.0), tip + DVec3::new(4.0, 0.0, 30.0), 0.1).is_none(), "{}: past the tip", spec.keys[0]);
        }
        // A swept wing (the 747): nothing ahead of the leading edge near the tip.
        let b747 = crate::aircraft::by_key("747").unwrap();
        let sh = Shape::of(b747);
        let le = crate::damage::wing_pt(b747, 1, b747.span * 0.8, 0.0, 0.0).as_dvec3();
        assert!(sh.hit(le + DVec3::new(0.0, 4.0, -6.0), le + DVec3::new(0.0, -4.0, -6.0), 0.1).is_none());
        assert!(sh.hit(le + DVec3::new(0.0, 4.0, 2.0), le + DVec3::new(0.0, -4.0, 2.0), 0.1).is_some());
    }

    #[test]
    fn near_birds_get_wings_far_ones_one_speck() {
        let mut b = Birds::default();
        let c = geo::lla_to_pos(50f64.to_radians(), 5f64.to_radians(), 0.0);
        let f = b.spawn(Kind::Crow, c, &place(), false, 0.0, 1).unwrap();
        b.flocks.push(f);
        let p = b.flocks[0].birds[0].p;
        let u = up_at(p);
        let all = |_: DVec3, _: f64| true;
        let mut out = Vec::new();
        b.instances(p - u * 50.0, 0.001, &all, &mut out);
        assert_eq!(out.len(), 5);
        assert!(out.iter().all(|(id, _)| (BASE..BASE + 30).contains(id)));
        out.clear();
        b.instances(p - u * 900.0, 0.001, &all, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, mesh_id(Kind::Crow, 5));
        // Scaled up to stay a speck: 3 px at 900 m is 2.7 m, nearly three crows' span.
        let sx = Vec3::new(out[0].1.m[0][0], out[0].1.m[1][0], out[0].1.m[2][0]).length();
        assert!((sx - 2.7 / 0.98).abs() < 0.1, "{sx}");
        out.clear();
        b.instances(p - u * 2000.0, 0.001, &all, &mut out);
        assert!(out.is_empty());
        // The strike's thud grows with the bird and the speed.
        let s = |kind, speed| thud(&Strike { speed, kind, at: Vec3::ZERO });
        assert!(s(Kind::Goose, 70.0) > s(Kind::Starling, 70.0));
        assert!(s(Kind::Gull, 120.0) > s(Kind::Gull, 40.0));
    }
}
