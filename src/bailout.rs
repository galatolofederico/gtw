//! Bailing out of an aircraft: the jump or ejection, freefall, the parachute
//! opening, steering under canopy, and touchdown.
//!
//! The body is the same capsule the on-foot controller uses (0.30 m radius,
//! 1.80 m tall). While it is out of an aircraft this module does the flying:
//! gravity, air drag, tracking, the canopy's lift and drag, the opening
//! shock, wind drift and the ground or water below. [`crate::character`]
//! hands the airborne part of its step to [`Fall::step`], then takes over
//! again on touchdown.
//!
//! Figures are game figures grounded in the usual sport-parachuting numbers:
//! a belly-to-earth skydiver settles at about 55 m/s at sea level (120 mph),
//! a ram-air canopy descends at about 5 m/s and glides at about 9 m/s, and a
//! canopy takes about three seconds to inflate. The opening is rate limited
//! rather than instantaneous: a 55 m/s jump to a 5 m/s canopy would otherwise
//! be a 50 g shock.
//!
//! The parachute is always packed when the body leaves the aircraft: the
//! freefall lasts until the player pulls the ripcord (the jump action, Space
//! by default, or the button on the fall HUD). There is no automatic
//! deployment.
//!
//! Not simulated: the seat itself after ejection, canopy malfunctions, reserve
//! parachutes, braked/toggle flight beyond the simple flare input, and swimming
//! (a water landing is defined in `Game::step_fall`: the player is washed
//! ashore, or rescued to an airfield in open water).

use crate::aircraft::Spec;
use crate::geo;
use crate::render::{Inst, MeshVertex};
use crate::surface::Surface;
use crate::world::WorldQuery;
use glam::{DVec3, Mat4, Vec3};
use std::sync::OnceLock;

/// Downward acceleration (m/s²).
pub const G: f64 = 9.81;
/// Sea-level freefall terminal velocity, belly to earth (m/s).
pub const TERMINAL: f64 = 55.0;
/// The fastest freefall is allowed to get, thin air included (m/s).
pub const TERMINAL_MAX: f64 = 100.0;
/// The canopy's steady descent, fully open and hands off (m/s).
pub const CANOPY_DESCENT: f64 = 5.2;
/// The canopy's steady forward speed through the air (m/s).
pub const CANOPY_GLIDE: f64 = 9.5;
/// Time for the canopy to inflate (s).
pub const OPEN_TIME: f64 = 2.8;
/// The body's capsule, as `character` uses it.
const RADIUS: f64 = 0.30;
const BODY_MID: f64 = 0.90;
/// Hip and head above the feet at rest (m): the freefall eye's offset comes
/// from rotating the body about the hips.
const HIP: f64 = 0.95;
const HEAD: f64 = 0.70;
/// How far the body pitches forward in freefall (rad, 77°).
pub const FREEFALL_TILT: f64 = 1.35;
/// ... and hanging under a developed canopy (rad).
pub const CANOPY_TILT: f64 = 0.16;

/// How the player left: the canopy or door of a light aircraft, an airliner's
/// door, or an ejection seat.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    Jump,
    Door,
    Eject,
}

/// The aircraft's exit: fighters eject, airliners open a door, everything else
/// is a jump from an open canopy or cabin.
pub fn exit_for(spec: &Spec) -> Exit {
    if spec.keys.iter().any(|k| matches!(*k, "f22" | "f-22" | "f4" | "f-4" | "fa18" | "f18" | "f/a-18" | "sr71" | "sr-71")) {
        Exit::Eject
    } else if spec.keys.iter().any(|k| matches!(*k, "747" | "b747" | "g650" | "concorde")) {
        Exit::Door
    } else {
        Exit::Jump
    }
}

/// The velocity the body leaves with, before the air gets hold of it.
pub fn launch_velocity(exit: Exit, plane_vel: DVec3, right: DVec3, up: DVec3, forward: DVec3) -> DVec3 {
    match exit {
        // The seat throws the pilot clear of the tail, then falls away.
        Exit::Eject => plane_vel * 0.90 + up * 15.0 + forward * 1.2,
        // Out of a door into the slipstream.
        Exit::Door => plane_vel + right * 0.8,
        // A step over the side of a light aircraft.
        Exit::Jump => plane_vel + right * 1.3 - up * 0.2,
    }
}

/// What the body is doing.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Freefall,
    Canopy,
    /// It has come down in water; the game gets it out.
    Water,
}

/// A bailout in progress.
#[derive(Clone, Debug)]
pub struct Fall {
    pub phase: Phase,
    pub exit: Exit,
    /// Canopy inflation, 0 packed .. 1 fully open.
    pub open: f64,
    /// Seconds since the body left the aircraft.
    pub t: f64,
    /// The way the body or canopy is flying, as a bearing (radians from north,
    /// clockwise).
    pub heading: f64,
    /// `jump` was down on the last step, so a held key opens once.
    jump_latch: bool,
    /// The g of the hardest deceleration in the last step, for the camera and
    /// the opening-shock sound.
    pub shake: f64,
    /// The last steering input (-1 left .. 1 right), for the canopy's lean.
    pub steer: f64,
}

impl Fall {
    pub fn new(exit: Exit, heading: f64) -> Self {
        Self { phase: Phase::Freefall, exit, open: 0.0, t: 0.0, heading: heading.rem_euclid(std::f64::consts::TAU), jump_latch: false, shake: 0.0, steer: 0.0 }
    }

    /// Pull the ripcord: no effect once the canopy is out.
    pub fn deploy(&mut self) -> bool {
        if self.phase != Phase::Freefall {
            return false;
        }
        self.phase = Phase::Canopy;
        self.open = 0.0;
        true
    }

    /// The body's pitch forward about its hips: belly to earth in freefall,
    /// upright under the canopy.
    pub fn tilt(&self) -> f64 {
        match self.phase {
            Phase::Freefall => FREEFALL_TILT,
            Phase::Canopy => FREEFALL_TILT + (CANOPY_TILT - FREEFALL_TILT) * smooth(self.open),
            Phase::Water => 0.0,
        }
    }

    /// The eye above the feet, and forward of them, in the body frame (m).
    /// Freefall rotates the head out in front of the hips.
    pub fn eye_offset(&self) -> (f64, f64) {
        let tilt = self.tilt();
        (HIP + HEAD * tilt.cos(), HEAD * tilt.sin())
    }

    /// The avatar's freefall share and canopy share, for `avatar::Motion`.
    pub fn pose(&self) -> (f64, f64) {
        match self.phase {
            Phase::Freefall => (1.0, 0.0),
            Phase::Canopy => {
                let k = smooth(self.open);
                (1.0 - k, k)
            }
            Phase::Water => (0.0, 0.0),
        }
    }

    /// One step of the fall. The body's position and velocity are advanced;
    /// the ground, buildings and tree trunks are resolved here. `wind` is the
    /// air's own motion in the planet frame.
    pub fn step<W: WorldQuery>(&mut self, h: f64, pos: &mut DVec3, vel: &mut DVec3, c: &crate::character::CharacterControls, wind: DVec3, world: &W) -> Event {
        let mut out = Event::default();
        if h <= 0.0 || !h.is_finite() || !pos.is_finite() || !vel.is_finite() {
            return out;
        }
        self.t += h;
        self.shake = 0.0;
        if self.phase == Phase::Water {
            *vel = DVec3::ZERO;
            return out;
        }
        let up = pos.normalize();
        // Pulling the ripcord is a press, not a hold: the player must open
        // the parachute themselves after the freefall.
        let pull = c.jump && !self.jump_latch;
        self.jump_latch = c.jump;
        if pull && self.deploy() {
            out.deployed = true;
        }

        // Steering: A/D (or the pad's right stick) turns; W/S track in
        // freefall and speed up or flare under canopy.
        let turn = (c.turn + c.right).clamp(-1.0, 1.0);
        self.steer = turn;
        let rate = match self.phase {
            Phase::Freefall => 1.1,
            Phase::Canopy => 0.95,
            Phase::Water => 0.0,
        };
        if turn != 0.0 {
            self.heading = (self.heading + turn * rate * h).rem_euclid(std::f64::consts::TAU);
        }
        let (east, north, _) = geo::enu(*pos);
        let dir = east * self.heading.sin() + north * self.heading.cos();
        let forward = c.forward.clamp(-1.0, 1.0);

        let alt = pos.length() - geo::R;
        let rho = air_density(alt);
        let mut accel = DVec3::ZERO;
        match self.phase {
            Phase::Freefall => {
                let air = *vel - wind;
                let speed = air.length();
                // Belly-to-earth drag: the sea-level terminal is 55 m/s, and
                // thinner air lets the body fall faster.
                let terminal = (TERMINAL * (1.225 / rho).sqrt()).clamp(35.0, TERMINAL_MAX);
                let k = G / (terminal * terminal);
                let mut drag = -air * (k * speed);
                // Tracking: a body can move a little towards where it looks.
                drag += dir * (3.0 * forward.max(0.0)) - dir * (1.2 * (-forward).max(0.0));
                let max = 8.0 * G;
                if drag.length() > max {
                    drag = drag.normalize() * max;
                }
                accel = drag - up * G;
            }
            Phase::Canopy => {
                self.open = (self.open + h / OPEN_TIME).clamp(0.0, 1.0);
                let k = smooth(self.open);
                // The canopy is a wing: forward through the air, coming down
                // slowly. A flare trades speed for a moment of lift.
                let flare = (-forward).max(0.0);
                let speed_up = forward.max(0.0);
                let target_glide = CANOPY_GLIDE + 2.0 * speed_up - 5.0 * flare;
                let target_descent = CANOPY_DESCENT + 0.7 * speed_up - 1.8 * flare;
                let air = *vel - wind;
                let v_up = air.dot(up);
                let v_h = air - up * v_up;
                // From the entry speed to the canopy's: the target is the
                // developed canopy from the moment of opening, and the rate it
                // is followed limits the opening shock.
                let tau = 3.0 + (0.9 - 3.0) * k;
                let a_up = (((-target_descent) - v_up) / tau).clamp(-3.0 * G, 6.0 * G);
                let target_h = dir * target_glide;
                let a_h = (target_h - v_h).clamp_length_max(3.0 + 6.0 * k);
                accel = up * a_up + a_h;
                self.shake = a_up.abs().max(a_h.length()) / G;
            }
            Phase::Water => {}
        }
        *vel += accel * h;
        *pos += *vel * h;

        // Solid things on the way down: a building or tree pushes the body
        // clear (it slides down a wall rather than through it).
        let up = pos.normalize();
        let centre = *pos + up * BODY_MID;
        if let Some(push) = world.building_push_sphere(centre, RADIUS) {
            *pos += push;
        }
        if let Some(push) = world.tree_push_sphere(centre, RADIUS) {
            *pos += push;
        }

        // Touchdown: the feet at the surface, still coming down. Water first:
        // there is no ground to stand on, but the impact is the same.
        let up = pos.normalize();
        let feet = pos.length() - geo::R;
        if let Some(s) = world.surface_at(*pos) {
            if feet <= s.h + 0.06 && vel.dot(up) <= 0.0 {
                let impact = -vel.dot(up);
                *pos = up * (geo::R + s.h);
                if s.water || s.kind == Surface::Water {
                    let vh = *vel - up * vel.dot(up);
                    *vel = vh * 0.15;
                    self.phase = Phase::Water;
                    out.water = Some(impact);
                } else {
                    *vel -= up * vel.dot(up);
                    out.landed = Some(impact);
                }
            }
        }
        if !pos.is_finite() || !vel.is_finite() {
            *pos = geo::lla_to_pos(0.0, 0.0, 0.0);
            *vel = DVec3::ZERO;
            self.phase = Phase::Water;
            out.water = Some(0.0);
        }
        out
    }
}

/// What one airborne step did.
#[derive(Clone, Debug, Default)]
pub struct Event {
    pub deployed: bool,
    /// Vertical speed at touchdown on land (m/s).
    pub landed: Option<f64>,
    /// Vertical speed at splashdown (m/s).
    pub water: Option<f64>,
}

/// The damage a touchdown does. A parachute landing fall is safe to about
/// 6.5 m/s; with the canopy still opening it is a hard one, and a freefall
/// is fatal well before the airframe's own safe impact speed.
pub fn landing_damage(impact: f64, open: f64) -> f32 {
    let safe = 4.0 + 2.5 * open.clamp(0.0, 1.0);
    if !impact.is_finite() || impact <= safe {
        0.0
    } else {
        (((impact - safe) * 10.0).min(100.0)) as f32
    }
}

/// Damage from hitting water: softer than ground, but still fatal from
/// freefall.
pub fn water_damage(impact: f64) -> f32 {
    if !impact.is_finite() || impact <= 8.0 {
        0.0
    } else {
        (((impact - 8.0) * 6.0).min(100.0)) as f32
    }
}

/// Air density at altitude `h` above the mean sphere (kg/m³), exponential
/// atmosphere.
pub fn air_density(h: f64) -> f64 {
    (1.225 * (-h / 8500.0).exp()).clamp(0.000_01, 1.5)
}

fn smooth(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

// ------------------------------------------------------------------ the canopy
//
// A ram-air canopy and its lines, drawn with the instanced-solid path (the
// avatar's `ext.w = 2` character branch). They follow the player in the
// instanced-solid library, after the traffic's far models, so the existing
// mesh ids are untouched.

/// Meshes: the canopy, then its lines.
pub const MESHES: usize = 2;
/// The first mesh id of the parachute.
pub const MESH_BASE: u32 = crate::cars::FAR_BASE + crate::cars::N_MODELS as u32;
/// Where the canopy sits above the feet (m), and the harness point the lines
/// run down to.
const CANOPY_Y: f32 = 5.6;
const HARNESS_Y: f32 = 1.45;

#[derive(Default)]
struct MB {
    v: Vec<MeshVertex>,
    i: Vec<u32>,
}

impl MB {
    fn quad(&mut self, q: [Vec3; 4], n: Vec3, col: [f32; 4]) {
        let base = self.v.len() as u32;
        for p in q {
            self.v.push(MeshVertex { pos: p.into(), nrm: n.into(), col });
        }
        self.i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A thin square section from `a` to `b`.
    fn line(&mut self, a: Vec3, b: Vec3, width: f32, col: [f32; 4]) {
        let axis = (b - a).normalize_or(Vec3::Y);
        let side = axis.cross(Vec3::Z).normalize_or(Vec3::X) * width;
        let side2 = axis.cross(side).normalize_or(Vec3::Y) * width;
        for k in 0..4 {
            let (u0, u1) = match k {
                0 => (side, side2),
                1 => (side2, -side),
                2 => (-side, -side2),
                _ => (-side2, side),
            };
            let n = (u0 - u1).normalize_or(Vec3::Y);
            self.quad([a + u0, b + u0, b + u1, a + u1], n, col);
        }
    }

    fn done(self) -> (Vec<MeshVertex>, Vec<u32>) {
        (self.v, self.i)
    }
}

/// The canopy: a swept, slightly cambered wing with closed tips.
fn canopy_mesh() -> (Vec<MeshVertex>, Vec<u32>) {
    const SPAN: usize = 11;
    const RING: usize = 18;
    let cloth = [0.93, 0.36, 0.12, crate::avatar::MAT_FIXED];
    let mut m = MB::default();
    let mut rings: Vec<Vec<Vec3>> = Vec::with_capacity(SPAN);
    for i in 0..SPAN {
        let s = -1.0 + 2.0 * i as f32 / (SPAN - 1) as f32;
        let x = s * 3.4;
        let y0 = CANOPY_Y + 0.18 * (1.0 - s * s);
        let chord = 2.1 + 0.2 * (1.0 - s * s);
        let thick = 0.28 * (1.0 - 0.25 * s * s);
        let mut ring = Vec::with_capacity(RING);
        for k in 0..RING {
            let a = k as f32 * std::f32::consts::TAU / RING as f32;
            let (sn, cs) = a.sin_cos();
            // Nose towards +z. The upper surface is fuller than the lower.
            let z = 0.5 * chord * cs;
            let yy = if sn >= 0.0 { thick * sn } else { thick * 0.45 * sn };
            ring.push(Vec3::new(x, y0 + yy + 0.05 * (1.0 - cs), z));
        }
        rings.push(ring);
    }
    let base = m.v.len() as u32;
    for ring in &rings {
        for p in ring {
            m.v.push(MeshVertex { pos: (*p).into(), nrm: Vec3::ZERO.into(), col: cloth });
        }
    }
    for i in 0..SPAN - 1 {
        for k in 0..RING {
            let k1 = (k + 1) % RING;
            let (a, b, c, d) = ((i * RING + k) as u32, (i * RING + k1) as u32, ((i + 1) * RING + k1) as u32, ((i + 1) * RING + k) as u32);
            let pa = Vec3::from(m.v[(base + a) as usize].pos);
            let pb = Vec3::from(m.v[(base + b) as usize].pos);
            let pc = Vec3::from(m.v[(base + c) as usize].pos);
            let n = (pb - pa).cross(pc - pa).normalize_or(Vec3::Y);
            m.i.extend([base + a, base + b, base + c, base + a, base + c, base + d]);
            for &v in &[base + a, base + b, base + c, base + d] {
                let nn = Vec3::from(m.v[v as usize].nrm) + n;
                m.v[v as usize].nrm = nn.into();
            }
        }
    }
    // Tip caps: a fan to the middle of each end ring.
    for (i, flip) in [(0usize, true), (SPAN - 1, false)] {
        let ring = &rings[i];
        let centre = ring.iter().fold(Vec3::ZERO, |a, p| a + *p) / ring.len() as f32;
        let tip = m.v.len() as u32;
        m.v.push(MeshVertex { pos: centre.into(), nrm: Vec3::X.into(), col: cloth });
        for k in 0..RING as u32 {
            let k1 = (k + 1) % RING as u32;
            let a = base + i as u32 * RING as u32 + k;
            let b = base + i as u32 * RING as u32 + k1;
            if flip {
                m.i.extend([tip, b, a]);
            } else {
                m.i.extend([tip, a, b]);
            }
        }
    }
    // Smooth the cap normals too (the ring's own normals are already there).
    for v in m.v.iter_mut() {
        if Vec3::from(v.nrm).length_squared() < 1e-6 {
            v.nrm = Vec3::Y.into();
        } else {
            v.nrm = Vec3::from(v.nrm).normalize().into();
        }
    }
    m.done()
}

/// Six suspension lines and the risers at the harness.
fn line_mesh() -> (Vec<MeshVertex>, Vec<u32>) {
    let dark = [0.10, 0.10, 0.11, crate::avatar::MAT_FIXED];
    let mut m = MB::default();
    for (x, z) in [(-2.6, 0.55), (-2.2, -0.55), (2.6, 0.55), (2.2, -0.55), (-0.9, 0.95), (0.9, 0.95)] {
        let top = Vec3::new(x, CANOPY_Y - 0.08, z);
        let low = Vec3::new(x.signum() * 0.20, HARNESS_Y, 0.0);
        m.line(top, low, 0.018, dark);
    }
    // A short riser on each shoulder.
    for s in [-1.0f32, 1.0] {
        m.line(Vec3::new(s * 0.20, HARNESS_Y, 0.0), Vec3::new(s * 0.16, HARNESS_Y - 0.55, 0.02), 0.025, dark);
    }
    m.done()
}

/// The parachute's meshes, built once.
pub fn meshes() -> &'static [(Vec<MeshVertex>, Vec<u32>)] {
    static M: OnceLock<Vec<(Vec<MeshVertex>, Vec<u32>)>> = OnceLock::new();
    M.get_or_init(|| vec![canopy_mesh(), line_mesh()])
}

/// Append the canopy and its lines for a body at `root` (feet), facing `yaw`.
/// `open` is 0 packed .. 1 fully open; `steer` leans the canopy into a turn.
pub fn instances(cam: DVec3, root: DVec3, yaw: f64, open: f64, steer: f64, out: &mut Vec<(u32, Inst)>) {
    if !root.is_finite() || !cam.is_finite() || !open.is_finite() || open <= 0.01 {
        return;
    }
    let up = root.normalize();
    let (east, north, _) = geo::enu(root);
    let fwd = east * yaw.sin() + north * yaw.cos();
    let right = up.cross(fwd).normalize_or_zero();
    if right.length_squared() < 0.5 {
        return;
    }
    let forward = right.cross(up);
    let basis = Mat4::from_cols(right.as_vec3().extend(0.0), up.as_vec3().extend(0.0), forward.as_vec3().extend(0.0), (root - cam).as_vec3().extend(1.0));
    // The canopy hangs above the pilot and leans into a turn; it grows out
    // of the pack over the first seconds.
    let lean = (steer.clamp(-1.0, 1.0) as f32) * 0.22;
    let sway = at(0.0, HARNESS_Y, 0.0) * Mat4::from_rotation_z(lean) * Mat4::from_rotation_x(-0.05) * at(0.0, -HARNESS_Y, 0.0);
    let s = (0.18 + 0.82 * open.clamp(0.0, 1.0)) as f32;
    let grow = at(0.0, CANOPY_Y, 0.0) * Mat4::from_scale(Vec3::splat(s)) * at(0.0, -CANOPY_Y, 0.0);
    for (k, local) in [sway * grow, sway].into_iter().enumerate() {
        let m = basis * local;
        let c = m.to_cols_array_2d();
        let rows = [[c[0][0], c[1][0], c[2][0], c[3][0]], [c[0][1], c[1][1], c[2][1], c[3][1]], [c[0][2], c[1][2], c[2][2], c[3][2]]];
        out.push((MESH_BASE + k as u32, Inst { m: rows, tint: [0.0; 4], ext: [0.0, 0.0, 0.0, 2.0] }));
    }
}

fn at(x: f32, y: f32, z: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, y, z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::CharacterControls;
    use crate::surface::SurfaceHit;

    struct Flat {
        water: bool,
    }
    impl WorldQuery for Flat {
        fn surface_at(&self, _p: DVec3) -> Option<SurfaceHit> {
            Some(SurfaceHit { h: 0.0, kind: if self.water { Surface::Water } else { Surface::Grass }, water: self.water, road: false, sidewalk: false, bridge: false })
        }
        fn building_push_sphere(&self, _p: DVec3, _r: f64) -> Option<DVec3> {
            None
        }
        fn building_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
            None
        }
        fn tree_push_sphere(&self, _p: DVec3, _r: f64) -> Option<DVec3> {
            None
        }
        fn tree_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
            None
        }
    }

    fn origin() -> DVec3 {
        geo::lla_to_pos(48.0f64.to_radians(), 2.0f64.to_radians(), 0.0)
    }

    fn above(metres: f64) -> DVec3 {
        origin().normalize() * (geo::R + metres)
    }

    fn run<W: WorldQuery>(fall: &mut Fall, pos: &mut DVec3, vel: &mut DVec3, controls: CharacterControls, world: &W, secs: f64) -> Event {
        let mut last = Event::default();
        let n = (secs * 60.0).round() as usize;
        for _ in 0..n {
            last = fall.step(1.0 / 60.0, pos, vel, &controls, DVec3::ZERO, world);
            if last.landed.is_some() || last.water.is_some() {
                break;
            }
        }
        last
    }

    #[test]
    fn freefall_settles_at_about_the_terminal_speed() {
        let w = Flat { water: false };
        let mut fall = Fall::new(Exit::Jump, 0.0);
        let mut pos = above(4_000.0);
        let mut vel = DVec3::ZERO;
        run(&mut fall, &mut pos, &mut vel, CharacterControls::default(), &w, 20.0);
        let up = pos.normalize();
        let down = -vel.dot(up);
        let expect = (TERMINAL * (1.225 / air_density(pos.length() - geo::R)).sqrt()).clamp(35.0, TERMINAL_MAX);
        assert!((down - expect).abs() < 2.5, "freefall descent {down:.1} m/s, want about {expect:.1} at {:.0} m", pos.length() - geo::R);
        assert!(fall.phase == Phase::Freefall, "the parachute is still packed");
    }

    #[test]
    fn thin_air_falls_faster() {
        let w = Flat { water: false };
        let mut a = Fall::new(Exit::Jump, 0.0);
        let mut b = Fall::new(Exit::Jump, 0.0);
        let (mut pa, mut va) = (above(500.0), DVec3::ZERO);
        let (mut pb, mut vb) = (above(10_000.0), DVec3::ZERO);
        run(&mut a, &mut pa, &mut va, CharacterControls::default(), &w, 18.0);
        run(&mut b, &mut pb, &mut vb, CharacterControls::default(), &w, 18.0);
        assert!(-vb.dot(pb.normalize()) > -va.dot(pa.normalize()) + 10.0, "10 km: {:.1} m/s, sea level: {:.1}", -vb.dot(pb.normalize()), -va.dot(pa.normalize()));
    }

    #[test]
    fn a_fall_from_airliner_altitude_stays_freefall_until_pulled() {
        let w = Flat { water: false };
        let mut fall = Fall::new(Exit::Door, 0.0);
        let mut pos = above(10_000.0);
        let mut vel = DVec3::ZERO;
        for _ in 0..60 * 12 {
            let ev = fall.step(1.0 / 60.0, &mut pos, &mut vel, &CharacterControls::default(), DVec3::ZERO, &w);
            assert!(!ev.deployed, "nothing opens the canopy by itself");
        }
        assert_eq!(fall.phase, Phase::Freefall);
        assert!(-vel.dot(pos.normalize()) > 45.0, "the thin air has been had");
        assert!(-vel.dot(pos.normalize()) < TERMINAL_MAX + 5.0, "the fall is finite and below the cap");
        assert!(fall.deploy());
        assert_eq!(fall.phase, Phase::Canopy);
    }

    #[test]
    fn a_low_jump_stays_in_freefall_until_the_button_is_pressed() {
        // Leaving at 180 m used to fire the reserve at once. Now the body
        // freefalls; the player has to pull the ripcord.
        let w = Flat { water: false };
        let mut fall = Fall::new(Exit::Jump, 0.0);
        let mut pos = above(180.0);
        let mut vel = DVec3::ZERO;
        for _ in 0..60 {
            let ev = fall.step(1.0 / 60.0, &mut pos, &mut vel, &CharacterControls::default(), DVec3::ZERO, &w);
            assert!(!ev.deployed);
        }
        assert_eq!(fall.phase, Phase::Freefall);
        assert!(fall.t > 0.9);
        assert!(fall.deploy(), "the jump control opens the canopy");
        assert_eq!(fall.phase, Phase::Canopy);
    }

    #[test]
    fn the_canopy_opens_without_a_killing_shock_and_settles() {
        let w = Flat { water: false };
        let mut fall = Fall::new(Exit::Jump, 0.0);
        let mut pos = above(3_000.0);
        let mut vel = DVec3::ZERO;
        run(&mut fall, &mut pos, &mut vel, CharacterControls::default(), &w, 12.0);
        let before = -vel.dot(pos.normalize());
        assert!(before > 45.0, "still in freefall: {before:.1} m/s");
        assert!(fall.deploy());
        let mut peak = 0.0f64;
        for _ in 0..600 {
            let ev = fall.step(1.0 / 60.0, &mut pos, &mut vel, &CharacterControls::default(), DVec3::ZERO, &w);
            peak = peak.max(fall.shake);
            if ev.landed.is_some() || ev.water.is_some() {
                break;
            }
        }
        assert!(fall.open > 0.95, "canopy open {:.0}%", fall.open * 100.0);
        let down = -vel.dot(pos.normalize());
        assert!((down - CANOPY_DESCENT).abs() < 1.0, "canopy descent {down:.1} m/s");
        assert!(peak < 7.0, "opening shock {peak:.1} g");
        let glide = (vel - pos.normalize() * vel.dot(pos.normalize())).length();
        assert!((glide - CANOPY_GLIDE).abs() < 1.5, "glide {glide:.1} m/s");
    }

    #[test]
    fn a_low_pull_is_hard() {
        // Pulled at the last moment: the canopy is still opening at touchdown.
        let w = Flat { water: false };
        let mut fall = Fall::new(Exit::Jump, 0.0);
        let mut pos = above(45.0);
        let mut vel = -pos.normalize() * 55.0;
        assert!(fall.deploy());
        let mut landed = None;
        for _ in 0..300 {
            let ev = fall.step(1.0 / 60.0, &mut pos, &mut vel, &CharacterControls::default(), DVec3::ZERO, &w);
            if let Some(i) = ev.landed {
                landed = Some(i);
                break;
            }
        }
        let impact = landed.expect("landed");
        assert!(impact > 6.5, "a low pull lands hard: {impact:.1} m/s");
        assert!(landing_damage(impact, fall.open) > 0.0);
    }

    #[test]
    fn steering_turns_the_canopy_and_wind_carries_it() {
        let w = Flat { water: false };
        let mut fall = Fall::new(Exit::Door, 0.0);
        fall.deploy();
        fall.open = 1.0;
        let mut pos = above(1_000.0);
        let mut vel = -pos.normalize() * CANOPY_DESCENT;
        let controls = CharacterControls { turn: 1.0, ..Default::default() };
        let start = fall.heading;
        // Two seconds of a full right turn.
        for _ in 0..120 {
            fall.step(1.0 / 60.0, &mut pos, &mut vel, &controls, DVec3::ZERO, &w);
        }
        assert!((fall.heading - start).rem_euclid(std::f64::consts::TAU) > 1.0, "turned {:.2} rad", (fall.heading - start).rem_euclid(std::f64::consts::TAU));

        // With no input the canopy flies its heading, and a steady wind
        // carries the ground track downwind.
        let mut fall = Fall::new(Exit::Door, 0.0);
        fall.deploy();
        fall.open = 1.0;
        let mut pos = above(1_000.0);
        let mut vel = -pos.normalize() * CANOPY_DESCENT;
        let (east, _, _) = geo::enu(pos);
        let wind = east * 10.0;
        let before = pos;
        for _ in 0..600 {
            fall.step(1.0 / 60.0, &mut pos, &mut vel, &CharacterControls::default(), wind, &w);
        }
        let moved = pos - before;
        assert!(moved.dot(east) > 50.0, "drifted {:.0} m downwind", moved.dot(east));
    }

    #[test]
    fn water_and_land_touchdowns_are_told_apart() {
        // Ten metres up and coming down at 5 m/s: land.
        let w = Flat { water: false };
        let mut fall = Fall::new(Exit::Jump, 0.0);
        let mut pos = above(10.0);
        let mut vel = -pos.normalize() * 5.0;
        let ev = run(&mut fall, &mut pos, &mut vel, CharacterControls::default(), &w, 3.0);
        assert!(ev.landed.is_some() && ev.water.is_none());

        let w = Flat { water: true };
        let mut fall = Fall::new(Exit::Jump, 0.0);
        let mut pos = above(10.0);
        let mut vel = -pos.normalize() * 5.0;
        let ev = run(&mut fall, &mut pos, &mut vel, CharacterControls::default(), &w, 3.0);
        assert!(ev.water.is_some() && ev.landed.is_none());
        assert_eq!(fall.phase, Phase::Water);
    }

    #[test]
    fn damage_thresholds_sit_where_the_landing_falls_do() {
        assert_eq!(landing_damage(6.4, 1.0), 0.0);
        assert!(landing_damage(8.0, 1.0) > 0.0);
        assert!(landing_damage(55.0, 0.0) >= 100.0);
        assert_eq!(water_damage(8.0), 0.0);
        assert!(water_damage(55.0) >= 100.0);
    }

    #[test]
    fn every_flyable_aircraft_has_an_exit() {
        for spec in crate::aircraft::list() {
            let exit = exit_for(spec);
            assert!(matches!(exit, Exit::Jump | Exit::Door | Exit::Eject), "{}: {exit:?}", spec.name);
            assert!(spec.eye.is_finite(), "{} has a pilot's eye", spec.name);
        }
    }

    #[test]
    fn fighters_eject_and_airliners_use_a_door() {
        // The registry's own types, looked up the way the game does.
        let fa18 = crate::aircraft::by_key("fa18").expect("the Hornet");
        assert_eq!(exit_for(fa18), Exit::Eject);
        let b747 = crate::aircraft::by_key("b747").expect("the 747");
        assert_eq!(exit_for(b747), Exit::Door);
        let c172 = crate::aircraft::by_key("c172").expect("the Skyhawk");
        assert_eq!(exit_for(c172), Exit::Jump);
        let up = DVec3::Y;
        let right = DVec3::X;
        let fwd = -DVec3::Z;
        assert!(launch_velocity(Exit::Eject, DVec3::ZERO, right, up, fwd).dot(up) > 10.0, "the seat throws the pilot up");
    }

    #[test]
    fn a_city_roof_pushes_the_body_clear_and_it_lands_in_the_street() {
        struct City;
        impl WorldQuery for City {
            fn surface_at(&self, _p: DVec3) -> Option<SurfaceHit> {
                Some(SurfaceHit { h: 0.0, kind: Surface::Asphalt, water: false, road: true, sidewalk: false, bridge: false })
            }
            fn building_push_sphere(&self, p: DVec3, r: f64) -> Option<DVec3> {
                // A roof from 8 m down, a little wider than the body.
                let alt = p.length() - geo::R;
                if !(3.0..8.0).contains(&alt) {
                    return None;
                }
                // A block of roofs: the body is slid out towards the street.
                let (east, _, _) = geo::enu(origin());
                Some(east * (0.04 + r * 0.1))
            }
            fn building_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
                None
            }
            fn tree_push_sphere(&self, _p: DVec3, _r: f64) -> Option<DVec3> {
                None
            }
            fn tree_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
                None
            }
        }
        let w = City;
        let mut fall = Fall::new(Exit::Jump, 0.0);
        fall.deploy();
        fall.open = 1.0;
        let mut pos = above(30.0);
        let mut vel = -pos.normalize() * CANOPY_DESCENT;
        let ev = run(&mut fall, &mut pos, &mut vel, CharacterControls::default(), &w, 30.0);
        assert!(ev.landed.is_some(), "it comes down");
        let (east, _, _) = geo::enu(origin());
        assert!((pos - origin()).dot(east) > 1.5, "pushed clear of the roof: {:.2} m", (pos - origin()).dot(east));
    }

    #[test]
    fn the_canopy_meshes_are_finite_and_carried_by_the_player() {
        let meshes = meshes();
        assert_eq!(meshes.len(), MESHES);
        for (v, i) in meshes {
            assert!(!v.is_empty() && !i.is_empty(), "a mesh with geometry");
            assert!(i.iter().all(|k| (*k as usize) < v.len()), "indices in range");
            assert!(v.iter().all(|x| x.pos.iter().all(|f| f.is_finite()) && x.nrm.iter().all(|f| f.is_finite())), "finite vertices");
        }
        let root = geo::lla_to_pos(0.5, 0.1, 500.0);
        let cam = root + DVec3::new(0.0, 0.0, 20.0);
        let mut out = Vec::new();
        instances(cam, root, 0.7, 1.0, 0.5, &mut out);
        assert_eq!(out.len(), MESHES);
        assert!(out.iter().all(|(id, inst)| (MESH_BASE..MESH_BASE + MESHES as u32).contains(id) && inst.m.iter().flatten().all(|v| v.is_finite()) && inst.ext[3] == 2.0));
        // A packed canopy draws nothing.
        out.clear();
        instances(cam, root, 0.7, 0.0, 0.0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn eye_and_pose_follow_the_phase() {
        let mut f = Fall::new(Exit::Jump, 0.0);
        let (up, fwd) = f.eye_offset();
        assert!(up < 1.2 && fwd > 0.6, "freefall eye {up:.2} up, {fwd:.2} forward");
        assert_eq!(f.pose(), (1.0, 0.0));
        f.deploy();
        f.open = 1.0;
        let (up, fwd) = f.eye_offset();
        assert!((up - 1.65).abs() < 0.05 && fwd.abs() < 0.2, "canopy eye {up:.2} up, {fwd:.2} forward");
        assert_eq!(f.pose(), (0.0, 1.0));
    }
}
