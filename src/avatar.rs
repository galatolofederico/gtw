//! The player's avatar: a rigid, multi-part figure posed procedurally.
//!
//! The avatar is sixteen rigid pieces (pelvis,
//! torso, head, upper and lower arms, hands, thighs, shins, feet and hair)
//! drawn through the instanced solid path (`inst.wgsl`, selected by
//! `Inst.ext.w = 2`). Each piece is a smooth swept surface built around its
//! joint, with rounded joint caps that overlap the neighbouring piece, so
//! the figure stays closed as it bends.
//!
//! The figure is about 1.80 m tall (7.5 heads): hip joints at 0.92 m, knees
//! at 0.48 m, shoulders at 1.45 m. It wears a hoodie over a tee, jeans with
//! a belt, and sneakers.
//!
//! The pose is a gait cycle driven by distance (`Character::gait_phase`) and
//! speed, blending smoothly from standing to walking to running, and into a
//! tuck in the air. The legs are posed first and the figure is then lowered
//! or raised until the lower sole rests on the ground, so the feet neither
//! float nor sink as the stride changes. The `RigPose` is a plain array of
//! part transforms, so a skinned model can replace this later without
//! changing the character controller or the camera.

use crate::geo;
use crate::render::{Inst, MeshVertex};
use glam::{DVec3, Mat4, Vec3};
use std::f32::consts::{PI, TAU};
use std::sync::OnceLock;

/// The rigid pieces, in mesh-id order from [`BASE`].
pub const PARTS: usize = 16;
/// The first mesh id of the avatar in the instanced-solid library.
pub const BASE: u32 = crate::birds::BASE + 30;
/// How many clothing palettes [`palette`] offers.
pub const STYLES: usize = 4;

/// Material codes written into the mesh's vertex colour alpha, as the shader
/// reads them (`inst.wgsl`, the character branch). The tinted materials take
/// the part's palette colour times the vertex colour; the fixed one is drawn
/// in the vertex colour alone.
pub const MAT_SKIN: f32 = 0.0;
pub const MAT_CLOTH: f32 = 0.25;
pub const MAT_HAIR: f32 = 0.5;
pub const MAT_SHOE: f32 = 0.75;
pub const MAT_FIXED: f32 = 0.95;

/// The part names, in the order the meshes and poses use. "Left" is the
/// figure's own left (+X in the body frame).
#[allow(dead_code)]
pub const NAMES: [&str; PARTS] = [
    "pelvis", "torso", "head", "upper arm left", "lower arm left", "hand left", "upper arm right", "lower arm right", "hand right", "thigh left", "shin left", "foot left", "thigh right", "shin right", "foot right", "hair",
];

// Part indices.
const PELVIS: usize = 0;
const TORSO: usize = 1;
const HEAD: usize = 2;
const UARM_L: usize = 3;
const FARM_L: usize = 4;
const HAND_L: usize = 5;
const UARM_R: usize = 6;
const FARM_R: usize = 7;
const HAND_R: usize = 8;
const THIGH_L: usize = 9;
const SHIN_L: usize = 10;
const FOOT_L: usize = 11;
const THIGH_R: usize = 12;
const SHIN_R: usize = 13;
const FOOT_R: usize = 14;
const HAIR: usize = 15;

// The skeleton at rest (m), body frame: +X left, +Y up, +Z forward, feet at
// the origin.
/// Hip centre (the pelvis pivot) above the ground, standing.
const HIP_Y: f32 = 0.95;
/// Hip joints from the hip centre.
const HIP_X: f32 = 0.095;
const HIP_DY: f32 = -0.03;
/// Waist (the torso pivot) above the hip centre.
const WAIST_DY: f32 = 0.10;
/// Shoulder joints and the neck base from the waist.
const SHOULDER_X: f32 = 0.185;
const SHOULDER_DY: f32 = 0.38;
const NECK_DY: f32 = 0.46;
/// Bone lengths.
const UPPER_ARM: f32 = 0.28;
const FOREARM: f32 = 0.25;
const THIGH: f32 = 0.44;
const SHIN: f32 = 0.40;
/// The sole's underside below the ankle, and its heel and toe (from the
/// ankle, along +Z).
const SOLE_DY: f32 = -0.08;
const HEEL_Z: f32 = -0.075;
const TOE_Z: f32 = 0.215;

/// A clothing palette: linear RGB, as the shader's tints.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Palette {
    pub skin: [f32; 3],
    pub cloth: [f32; 3],
    pub pants: [f32; 3],
    pub hair: [f32; 3],
    pub shoe: [f32; 3],
}

/// One of four palettes; an out-of-range style falls back to the first.
pub fn palette(style: u8) -> Palette {
    let p = |skin: [f32; 3], cloth: [f32; 3], pants: [f32; 3], hair: [f32; 3], shoe: [f32; 3]| Palette { skin, cloth, pants, hair, shoe };
    match style as usize {
        // Olive hoodie + dark indigo jeans + dark hair + grey sneakers
        1 => p([0.42, 0.27, 0.18], [0.14, 0.18, 0.08], [0.035, 0.05, 0.10], [0.03, 0.025, 0.02], [0.30, 0.30, 0.30]),
        // Slate hoodie + charcoal jeans + blonde hair + red sneakers
        2 => p([0.62, 0.45, 0.34], [0.16, 0.19, 0.25], [0.06, 0.06, 0.07], [0.50, 0.36, 0.16], [0.55, 0.12, 0.08]),
        // Crimson hoodie + black jeans + black hair + white sneakers
        3 => p([0.30, 0.19, 0.12], [0.34, 0.05, 0.05], [0.03, 0.03, 0.035], [0.02, 0.02, 0.02], [0.60, 0.60, 0.58]),
        // Charcoal hoodie + indigo jeans + brown hair + white sneakers
        _ => p([0.58, 0.40, 0.30], [0.045, 0.045, 0.05], [0.04, 0.07, 0.16], [0.07, 0.045, 0.025], [0.62, 0.62, 0.60]),
    }
}

/// The colour of a part's material.
fn tint(p: &Palette, part: usize) -> [f32; 3] {
    match part {
        HEAD | HAND_L | HAND_R => p.skin,
        PELVIS | THIGH_L | SHIN_L | THIGH_R | SHIN_R => p.pants,
        HAIR => p.hair,
        FOOT_L | FOOT_R => p.shoe,
        _ => p.cloth,
    }
}

/// What the body is doing, for the pose.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Motion {
    /// Gait cycle (radians): a half turn per footstep, continuous between
    /// them (`Character::gait_phase`). The left foot strikes at 0.
    pub phase: f64,
    /// Horizontal speed (m/s).
    pub speed: f64,
    /// 0 on the ground .. 1 in the air (`Character::air`).
    pub air: f64,
    /// Seconds, for breathing and idle sway.
    pub time: f64,
}

/// A rigid pose: one local transform per part, relative to the body's feet
/// (x left, y up, z forward).
#[derive(Copy, Clone, Debug)]
pub struct RigPose {
    pub parts: [Mat4; PARTS],
}

impl RigPose {
    pub fn is_finite(&self) -> bool {
        self.parts.iter().all(|m| m.to_cols_array().iter().all(|v| v.is_finite()))
    }

    /// The lowest point of either sole (m above the feet origin).
    pub fn sole_low(&self) -> f32 {
        [FOOT_L, FOOT_R]
            .iter()
            .flat_map(|&f| [HEEL_Z, TOE_Z].map(|z| self.parts[f].transform_point3(Vec3::new(0.0, SOLE_DY, z)).y))
            .fold(f32::MAX, f32::min)
    }
}

fn at(x: f32, y: f32, z: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, y, z))
}

fn rx(a: f32) -> Mat4 {
    Mat4::from_rotation_x(a)
}

fn ry(a: f32) -> Mat4 {
    Mat4::from_rotation_y(a)
}

fn rz(a: f32) -> Mat4 {
    Mat4::from_rotation_z(a)
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// A bump of width `w` (radians) centred on `c` in the cycle.
fn bump(phase: f32, c: f32, w: f32) -> f32 {
    let d = (phase - c + PI).rem_euclid(TAU) - PI;
    (-(d / w) * (d / w)).exp()
}

/// Joint angles for one leg. Rotations are about +X: positive swings a
/// hanging limb back, so a thigh forward is negative and a bent knee
/// positive.
struct Leg {
    hip: f32,
    knee: f32,
    ankle: f32,
}

/// Joint angles for one arm.
struct Arm {
    swing: f32,
    out: f32,
    elbow: f32,
}

/// Every angle of a pose, so a locomotion and a jump pose can be blended.
struct Angles {
    legs: [Leg; 2],
    arms: [Arm; 2],
    lean: f32,
    twist: f32,
    tilt: f32,
    sway: f32,
    head: f32,
}

/// One leg at its own phase (0 = its heel strike).
fn leg(phase: f32, amp: f32, swing_knee: f32, stance_knee: f32, roll: f32, run: f32) -> Leg {
    let (s, c) = phase.sin_cos();
    // Thigh: forward at the heel strike, back at the toe-off. Running
    // throws the thigh further forward than back.
    let fwd = amp * c + run * amp * 0.25 * (1.0 + c) * 0.5;
    // Knee: a big bend through the swing (sharper running), a small give
    // as the stance leg takes the weight.
    let swing = (-s).max(0.0).powf(mix(1.4, 0.8, run));
    let knee = swing_knee * swing + stance_knee * bump(phase, 0.55, 0.5);
    // The foot: toe up at the strike, level through the stance, a push off
    // the toe at its end, and lifted to clear the ground in the swing.
    let toe_up = roll * (0.30 * bump(phase, 0.0, 0.45) - 0.55 * bump(phase, 2.75, 0.4) + 0.25 * bump(phase, 4.5, 0.6));
    let hip = -fwd;
    // The ankle angle that gives the foot that pitch, whatever the shin does.
    let ankle = -toe_up - hip - knee;
    Leg { hip, knee, ankle }
}

fn locomotion(m: &Motion) -> Angles {
    let v = m.speed.max(0.0) as f32;
    let phase = m.phase.rem_euclid(std::f64::consts::TAU) as f32;
    let t = m.time as f32;
    // How much it is moving at all, and how much of that is a run.
    let go = smooth(0.05, 1.1, v);
    let run = smooth(2.4, 4.4, v);
    let stride = crate::character::stride_len(v as f64) as f32;
    // Through the stance the foot sweeps back by about a stride walking,
    // and less running (it is in the air for part of each step).
    let leg_len = THIGH + SHIN;
    let reach = stride * mix(0.5, 0.36, run) / leg_len;
    let amp = reach.clamp(0.0, 0.85).asin() * go;
    let swing_knee = mix(1.0, 1.85, run) * go;
    let stance_knee = mix(0.14, 0.42, run) * go;
    let roll = mix(1.0, 0.8, run) * go;
    let legs = [leg(phase, amp, swing_knee, stance_knee, roll, run), leg(phase + PI, amp, swing_knee, stance_knee, roll, run)];
    // Arms swing against the leg on their side, bent more for a run.
    let arm_amp = mix(0.32, 0.62, run) * go;
    let arm = |p: f32| {
        let fwd = -arm_amp * p.cos();
        let elbow = -mix(0.22 + 0.25 * fwd.max(0.0), 1.45 + 0.25 * fwd.max(0.0), run) * go - 0.14 * (1.0 - go);
        Arm { swing: -fwd, out: mix(0.12, 0.16, run) * go + 0.12 * (1.0 - go), elbow }
    };
    let arms = [arm(phase), arm(phase + PI)];
    // Breathing and a slow weight shift while standing.
    let breathe = (t * 1.7).sin() * (1.0 - go);
    let shift = (t * 0.45).sin() * (1.0 - go);
    let (s, c) = phase.sin_cos();
    Angles {
        legs,
        arms,
        lean: mix(0.035, 0.16, run) * go + 0.012 * breathe,
        twist: -0.09 * c * go,
        tilt: 0.045 * s * go * (1.0 - run) + 0.02 * shift,
        sway: 0.022 * s * go * (1.0 - run) + 0.012 * shift,
        head: 0.0,
    }
}

fn jump() -> Angles {
    // Knees up, feet nearly level (toes a little down).
    let tuck = |hip: f32, knee: f32| Leg { hip, knee, ankle: 0.3 - hip - knee };
    Angles {
        legs: [tuck(-0.85, 1.45), tuck(-0.5, 1.1)],
        arms: [Arm { swing: -0.9, out: 0.35, elbow: -0.9 }, Arm { swing: -0.6, out: 0.35, elbow: -0.9 }],
        lean: 0.12,
        twist: 0.0,
        tilt: 0.0,
        sway: 0.0,
        head: -0.1,
    }
}

fn blend(a: Angles, b: Angles, k: f32) -> Angles {
    let l = |x: &Leg, y: &Leg| Leg { hip: mix(x.hip, y.hip, k), knee: mix(x.knee, y.knee, k), ankle: mix(x.ankle, y.ankle, k) };
    let r = |x: &Arm, y: &Arm| Arm { swing: mix(x.swing, y.swing, k), out: mix(x.out, y.out, k), elbow: mix(x.elbow, y.elbow, k) };
    Angles {
        legs: [l(&a.legs[0], &b.legs[0]), l(&a.legs[1], &b.legs[1])],
        arms: [r(&a.arms[0], &b.arms[0]), r(&a.arms[1], &b.arms[1])],
        lean: mix(a.lean, b.lean, k),
        twist: mix(a.twist, b.twist, k),
        tilt: mix(a.tilt, b.tilt, k),
        sway: mix(a.sway, b.sway, k),
        head: mix(a.head, b.head, k),
    }
}

/// The pose for a motion. On the ground the lower sole is put on the ground
/// (y = 0); in the air the hips stay at standing height and the tucked legs
/// lift the feet.
pub fn pose(m: &Motion) -> RigPose {
    let air = (m.air as f32).clamp(0.0, 1.0);
    let a = blend(locomotion(m), jump(), air);
    let mut rig = build(&a);
    // Plant the feet: lower or raise the whole figure so the lowest sole
    // point rests on the ground, fading out as the body leaves it.
    let lift = -rig.sole_low() * (1.0 - air);
    if lift.is_finite() && lift != 0.0 {
        let shift = at(0.0, lift, 0.0);
        for p in rig.parts.iter_mut() {
            *p = shift * *p;
        }
    }
    rig
}

fn build(a: &Angles) -> RigPose {
    let mut parts = [Mat4::IDENTITY; PARTS];
    parts[PELVIS] = at(a.sway, HIP_Y, 0.0) * ry(a.twist) * rz(a.tilt);
    // The torso turns against the hips and leans; the head stays level.
    parts[TORSO] = parts[PELVIS] * at(0.0, WAIST_DY, 0.0) * ry(-a.twist * 1.8) * rz(-a.tilt * 1.4) * rx(a.lean);
    parts[HEAD] = parts[TORSO] * at(0.0, NECK_DY, 0.0) * ry(a.twist * 0.6) * rx(-a.lean * 0.7 + a.head);
    parts[HAIR] = parts[HEAD];
    for (k, side, (ua, fa, ha)) in [(0usize, 1.0f32, (UARM_L, FARM_L, HAND_L)), (1, -1.0, (UARM_R, FARM_R, HAND_R))] {
        let arm = &a.arms[k];
        parts[ua] = parts[TORSO] * at(side * SHOULDER_X, SHOULDER_DY, 0.0) * rz(side * arm.out) * rx(arm.swing);
        parts[fa] = parts[ua] * at(0.0, -UPPER_ARM, 0.0) * rx(arm.elbow);
        parts[ha] = parts[fa] * at(0.0, -FOREARM, 0.0) * rx(-0.08);
    }
    for (k, side, (th, sh, ft)) in [(0usize, 1.0f32, (THIGH_L, SHIN_L, FOOT_L)), (1, -1.0, (THIGH_R, SHIN_R, FOOT_R))] {
        let leg = &a.legs[k];
        // (the hips' tilt is undone at the thigh, so the legs hang plumb)
        parts[th] = parts[PELVIS] * at(side * HIP_X, HIP_DY, 0.0) * rz(-a.tilt) * rx(leg.hip);
        parts[sh] = parts[th] * at(0.0, -THIGH, 0.0) * rx(leg.knee);
        parts[ft] = parts[sh] * at(0.0, -SHIN, 0.0) * rx(leg.ankle);
    }
    RigPose { parts }
}

/// Append the avatar's part instances for a body standing at `root` (feet),
/// facing `yaw` (bearing radians), seen from `cam`.
pub fn instances(cam: DVec3, root: DVec3, yaw: f64, rig: &RigPose, style: u8, out: &mut Vec<(u32, Inst)>) {
    if !root.is_finite() || !cam.is_finite() || !rig.is_finite() {
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
    let pal = palette(style);
    for (i, local) in rig.parts.iter().enumerate() {
        let m = basis * *local;
        let t = tint(&pal, i);
        // `Inst.m` holds the matrix's rows (inst.wgsl dots each with (p, 1)).
        let c = m.to_cols_array_2d();
        let rows = [[c[0][0], c[1][0], c[2][0], c[3][0]], [c[0][1], c[1][1], c[2][1], c[3][1]], [c[0][2], c[1][2], c[2][2], c[3][2]]];
        // (ext.y = 1: drawn with the road ribbons' pull toward the eye, so
        // the feet are not hidden in the asphalt)
        out.push((BASE + i as u32, Inst { m: rows, tint: [t[0], t[1], t[2], 0.0], ext: [0.0, 1.0, 0.0, 2.0] }));
    }
}

// ---------------------------------------------------------------- the meshes

/// One ring of a sweep: centre, the two half-axes, and how square it is
/// (2 an ellipse, larger towards a rounded rectangle).
#[derive(Copy, Clone)]
struct Ring {
    c: Vec3,
    u: Vec3,
    w: Vec3,
    sq: f32,
}

/// A horizontal ring (for limbs and the trunk) at height `y`: half-width
/// `rx` across, half-depth `rz` front to back, centred `z` forward.
fn hr(y: f32, rx: f32, rz: f32, z: f32) -> Ring {
    Ring { c: Vec3::new(0.0, y, z), u: Vec3::X * rx, w: Vec3::Z * rz, sq: 2.0 }
}

/// The same, centred `x` to the left.
fn hrx(x: f32, y: f32, rx: f32, rz: f32, z: f32) -> Ring {
    Ring { c: Vec3::new(x, y, z), u: Vec3::X * rx, w: Vec3::Z * rz, sq: 2.0 }
}

/// A vertical ring across the foot at `z`: half-width `hw`, half-height
/// `hh`, centred at height `y`.
fn vr(z: f32, hw: f32, hh: f32, y: f32, sq: f32) -> Ring {
    Ring { c: Vec3::new(0.0, y, z), u: Vec3::X * hw, w: Vec3::Y * hh, sq }
}

fn sq(r: Ring, sq: f32) -> Ring {
    Ring { sq, ..r }
}

#[derive(Default)]
struct MB {
    v: Vec<MeshVertex>,
    i: Vec<u32>,
}

impl MB {
    fn cuboid(&mut self, lo: Vec3, hi: Vec3, col: [f32; 4]) {
        let p = |x: bool, y: bool, z: bool| Vec3::new(if x { hi.x } else { lo.x }, if y { hi.y } else { lo.y }, if z { hi.z } else { lo.z });
        let mut quad = |q: [Vec3; 4]| {
            let n = (q[1] - q[0]).cross(q[2] - q[0]).normalize_or_zero();
            let base = self.v.len() as u32;
            for p in q {
                self.v.push(MeshVertex { pos: p.into(), nrm: n.into(), col });
            }
            self.i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        };
        quad([p(false, false, true), p(true, false, true), p(true, true, true), p(false, true, true)]);
        quad([p(true, false, false), p(false, false, false), p(false, true, false), p(true, true, false)]);
        quad([p(true, false, true), p(true, false, false), p(true, true, false), p(true, true, true)]);
        quad([p(false, false, false), p(false, false, true), p(false, true, true), p(false, true, false)]);
        quad([p(false, true, true), p(true, true, true), p(true, true, false), p(false, true, false)]);
        quad([p(false, false, false), p(true, false, false), p(true, false, true), p(false, false, true)]);
    }

    /// A smooth closed surface through `rings`, capped with a gentle dome at
    /// both ends. Normals are averaged over the faces around each vertex and
    /// point away from the ring's centre.
    fn sweep(&mut self, rings: &[Ring], segs: usize, col: [f32; 4]) {
        if rings.len() < 2 {
            return;
        }
        let segs = segs.max(6);
        let point = |r: &Ring, k: usize| {
            let a = k as f32 * TAU / segs as f32;
            let (s, c) = a.sin_cos();
            let e = 2.0 / r.sq;
            r.c + r.u * c.signum() * c.abs().powf(e) + r.w * s.signum() * s.abs().powf(e)
        };
        let n = rings.len();
        let base = self.v.len() as u32;
        let mut pos = Vec::with_capacity(n * segs);
        for r in rings {
            for k in 0..segs {
                pos.push(point(r, k));
            }
        }
        let mut nrm = vec![Vec3::ZERO; pos.len()];
        let mut idx = Vec::new();
        for r in 0..n - 1 {
            for k in 0..segs {
                let k1 = (k + 1) % segs;
                let (a, b, c, d) = (r * segs + k, r * segs + k1, (r + 1) * segs + k1, (r + 1) * segs + k);
                for t in [[a, b, c], [a, c, d]] {
                    let f = (pos[t[1]] - pos[t[0]]).cross(pos[t[2]] - pos[t[0]]);
                    for &v in &t {
                        nrm[v] += f;
                    }
                    idx.extend(t);
                }
            }
        }
        for (k, p) in pos.iter().enumerate() {
            let centre = rings[k / segs].c;
            let mut nn = nrm[k].normalize_or_zero();
            if nn.dot(*p - centre) < 0.0 {
                nn = -nn;
            }
            if nn == Vec3::ZERO {
                nn = (*p - centre).normalize_or(Vec3::Y);
            }
            self.v.push(MeshVertex { pos: (*p).into(), nrm: nn.into(), col });
        }
        self.i.extend(idx.iter().map(|&k| base + k as u32));
        // Domed caps: a centre pushed out a little along the sweep.
        for (end, inner) in [(0usize, 1usize), (n - 1, n - 2)] {
            let r = rings[end];
            let axis = (r.c - rings[inner].c).normalize_or(Vec3::Y);
            let bulge = r.u.length().min(r.w.length()) * 0.35;
            let tip = r.c + axis * bulge;
            let t = self.v.len() as u32;
            self.v.push(MeshVertex { pos: tip.into(), nrm: axis.into(), col });
            let ring0 = self.v.len() as u32;
            for k in 0..segs {
                let p = point(&r, k);
                let nn = ((p - r.c).normalize_or_zero() * 0.6 + axis).normalize_or(axis);
                self.v.push(MeshVertex { pos: p.into(), nrm: nn.into(), col });
            }
            for k in 0..segs as u32 {
                let k1 = (k + 1) % segs as u32;
                self.i.extend([t, ring0 + k, ring0 + k1]);
            }
        }
    }

    /// An ellipsoid with smooth normals.
    fn ball(&mut self, centre: Vec3, radii: Vec3, rings: usize, segs: usize, col: [f32; 4]) {
        let rings = rings.max(4);
        let segs = segs.max(8);
        let base = self.v.len() as u32;
        for i in 0..=rings {
            let phi = i as f32 * PI / rings as f32;
            for j in 0..segs {
                let theta = j as f32 * TAU / segs as f32;
                let dir = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
                let pos = centre + dir * radii;
                let nrm = (dir / radii.max(Vec3::splat(1e-4))).normalize_or(Vec3::Y);
                self.v.push(MeshVertex { pos: pos.into(), nrm: nrm.into(), col });
            }
        }
        let stride = segs as u32;
        for i in 0..rings as u32 {
            for j in 0..stride {
                let jn = (j + 1) % stride;
                let a = base + i * stride + j;
                let b = base + (i + 1) * stride + j;
                let c = base + (i + 1) * stride + jn;
                let d = base + i * stride + jn;
                self.i.extend([a, b, c, a, c, d]);
            }
        }
    }

    /// A thin strip lying on the front of a sweep (a zip, a seam): through
    /// the front-most point of each ring, `half` wide and standing `lift`
    /// proud of it.
    fn front_strip(&mut self, rings: &[Ring], half: f32, lift: f32, col: [f32; 4]) {
        let front: Vec<Vec3> = rings.iter().map(|r| r.c + r.w + Vec3::Z * lift).collect();
        for w in front.windows(2) {
            let n = Vec3::new(0.0, w[1].z - w[0].z, -(w[1].y - w[0].y)).normalize_or(Vec3::Z);
            let n = if n.z < 0.0 { -n } else { n };
            let base = self.v.len() as u32;
            for p in [w[0] - Vec3::X * half, w[0] + Vec3::X * half, w[1] + Vec3::X * half, w[1] - Vec3::X * half] {
                self.v.push(MeshVertex { pos: p.into(), nrm: n.into(), col });
            }
            self.i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    fn done(self) -> (Vec<MeshVertex>, Vec<u32>) {
        (self.v, self.i)
    }
}

/// A tinted material, shaded by `shade` (1 = the palette colour).
fn tinted(shade: f32, mat: f32) -> [f32; 4] {
    [shade, shade, shade, mat]
}

fn fixed(r: f32, g: f32, b: f32) -> [f32; 4] {
    [r, g, b, MAT_FIXED]
}

// The pieces. Each is built around its own pivot joint at the origin.

fn pelvis() -> (Vec<MeshVertex>, Vec<u32>) {
    let jeans = tinted(1.0, MAT_CLOTH);
    let mut m = MB::default();
    // Jeans: crotch, seat (further back), hips and waistband.
    let rings = [hr(-0.115, 0.13, 0.088, 0.0), hr(-0.07, 0.155, 0.1, -0.01), hr(-0.01, 0.163, 0.105, -0.012), hr(0.05, 0.158, 0.102, -0.004), hr(0.10, 0.152, 0.099, 0.0)];
    m.sweep(&rings.map(|r| sq(r, 2.3)), 22, jeans);
    // Fly seam and back pockets.
    m.front_strip(&[hr(-0.10, 0.0, 0.088, 0.0), hr(-0.04, 0.0, 0.099, 0.0), hr(0.06, 0.0, 0.1, 0.0)], 0.004, 0.002, tinted(0.7, MAT_CLOTH));
    for s in [-1.0f32, 1.0] {
        m.cuboid(Vec3::new(s * 0.078 - 0.035, -0.04, -0.119), Vec3::new(s * 0.078 + 0.035, 0.03, -0.109), tinted(0.82, MAT_CLOTH));
    }
    // Leather belt and buckle.
    m.sweep(&[sq(hr(0.064, 0.161, 0.106, -0.002), 2.3), sq(hr(0.096, 0.157, 0.104, 0.0), 2.3)], 22, fixed(0.055, 0.04, 0.03));
    m.cuboid(Vec3::new(-0.022, 0.066, 0.101), Vec3::new(0.022, 0.094, 0.111), fixed(0.55, 0.56, 0.58));
    m.done()
}

/// The hoodie's body: waist to neck.
fn hoodie_rings() -> [Ring; 10] {
    [
        hr(-0.02, 0.163, 0.108, 0.002),
        hr(0.06, 0.166, 0.110, 0.006),
        hr(0.16, 0.178, 0.117, 0.014),
        hr(0.26, 0.193, 0.124, 0.02),
        hr(0.33, 0.201, 0.121, 0.014),
        hr(0.385, 0.198, 0.111, 0.002),
        hr(0.41, 0.182, 0.103, -0.004),
        hr(0.432, 0.145, 0.092, -0.009),
        hr(0.445, 0.110, 0.082, -0.012),
        hr(0.47, 0.068, 0.064, -0.012),
    ]
}

fn torso() -> (Vec<MeshVertex>, Vec<u32>) {
    let cloth = tinted(1.0, MAT_CLOTH);
    let rib = tinted(0.78, MAT_CLOTH);
    let mut m = MB::default();
    let rings = hoodie_rings();
    m.sweep(&rings.map(|r| sq(r, 2.35)), 24, cloth);
    // Ribbed hem at the waist.
    m.sweep(&[sq(hr(-0.035, 0.166, 0.111, 0.002), 2.35), sq(hr(0.025, 0.169, 0.113, 0.004), 2.35)], 24, rib);
    // Zip from the hem to the collar, and a kangaroo pocket.
    m.front_strip(&rings[..8], 0.0045, 0.003, fixed(0.08, 0.08, 0.085));
    m.sweep(&[sq(hr(0.03, 0.122, 0.104, 0.016), 3.0), sq(hr(0.15, 0.126, 0.107, 0.022), 3.0)], 20, tinted(0.9, MAT_CLOTH));
    // The tee showing at the open collar.
    m.sweep(&[hr(0.40, 0.07, 0.07, 0.012), hr(0.455, 0.062, 0.062, 0.0)], 14, fixed(0.80, 0.80, 0.78));
    // Collar and the hood bunched behind the neck.
    m.sweep(&[hr(0.435, 0.078, 0.075, -0.008), hr(0.49, 0.072, 0.070, -0.012)], 18, rib);
    m.ball(Vec3::new(0.0, 0.45, -0.075), Vec3::new(0.105, 0.065, 0.06), 8, 16, rib);
    // Drawstrings.
    for s in [-1.0f32, 1.0] {
        m.cuboid(Vec3::new(s * 0.03 - 0.003, 0.30, 0.131), Vec3::new(s * 0.03 + 0.003, 0.41, 0.137), fixed(0.75, 0.75, 0.72));
    }
    m.done()
}

fn head() -> (Vec<MeshVertex>, Vec<u32>) {
    let skin = tinted(1.0, MAT_SKIN);
    let mut m = MB::default();
    // Neck, from inside the collar.
    m.sweep(&[hr(-0.04, 0.054, 0.056, -0.008), hr(0.05, 0.050, 0.052, -0.006), hr(0.09, 0.052, 0.056, -0.004)], 14, skin);
    // Skull and face, chin to crown: the face sits forward of the skull.
    let skull = [
        hr(0.055, 0.032, 0.026, 0.062),
        hr(0.072, 0.052, 0.058, 0.04),
        hr(0.10, 0.066, 0.083, 0.022),
        hr(0.13, 0.073, 0.094, 0.012),
        hr(0.165, 0.077, 0.099, 0.005),
        hr(0.20, 0.079, 0.101, 0.0),
        hr(0.235, 0.074, 0.094, -0.005),
        hr(0.262, 0.058, 0.075, -0.008),
        hr(0.282, 0.033, 0.043, -0.01),
        hr(0.29, 0.006, 0.007, -0.01),
    ];
    m.sweep(&skull.map(|r| sq(r, 2.15)), 22, skin);
    // Nose: bridge, tip and wings.
    m.ball(Vec3::new(0.0, 0.15, 0.1), Vec3::new(0.011, 0.026, 0.014), 6, 10, skin);
    m.ball(Vec3::new(0.0, 0.131, 0.108), Vec3::new(0.017, 0.01, 0.012), 5, 10, skin);
    // Ears.
    for s in [-1.0f32, 1.0] {
        m.ball(Vec3::new(s * 0.078, 0.158, -0.006), Vec3::new(0.011, 0.029, 0.019), 6, 10, tinted(0.93, MAT_SKIN));
    }
    // Eyes: whites, irises, and brows above them.
    for s in [-1.0f32, 1.0] {
        let x = s * 0.031;
        m.ball(Vec3::new(x, 0.167, 0.089), Vec3::new(0.0125, 0.0075, 0.0085), 5, 10, fixed(0.80, 0.78, 0.74));
        m.ball(Vec3::new(x, 0.167, 0.0965), Vec3::new(0.0058, 0.0058, 0.0022), 4, 10, fixed(0.07, 0.045, 0.03));
        m.cuboid(Vec3::new(x - 0.016, 0.182, 0.093), Vec3::new(x + 0.016, 0.1875, 0.1), fixed(0.05, 0.035, 0.025));
    }
    // Mouth.
    m.cuboid(Vec3::new(-0.017, 0.099, 0.1), Vec3::new(0.017, 0.106, 0.106), tinted(0.62, MAT_SKIN));
    m.done()
}

fn hair() -> (Vec<MeshVertex>, Vec<u32>) {
    let hair = tinted(1.0, MAT_HAIR);
    let mut m = MB::default();
    // A shell a little outside the skull, set back so it frames the face:
    // the hairline is where it passes through the forehead.
    let shell = [
        hr(0.115, 0.066, 0.07, -0.038),
        hr(0.165, 0.083, 0.092, -0.022),
        hr(0.20, 0.0855, 0.104, -0.01),
        hr(0.235, 0.082, 0.103, -0.004),
        hr(0.265, 0.067, 0.087, -0.006),
        hr(0.288, 0.041, 0.055, -0.008),
        hr(0.301, 0.008, 0.01, -0.01),
    ];
    m.sweep(&shell.map(|r| sq(r, 2.15)), 22, hair);
    // A little volume on top, swept up at the front.
    m.ball(Vec3::new(0.004, 0.268, 0.042), Vec3::new(0.062, 0.03, 0.055), 6, 14, hair);
    m.done()
}

fn upper_arm() -> (Vec<MeshVertex>, Vec<u32>) {
    let cloth = tinted(1.0, MAT_CLOTH);
    let mut m = MB::default();
    m.ball(Vec3::new(0.004, -0.03, 0.0), Vec3::new(0.068, 0.072, 0.07), 8, 14, cloth);
    m.sweep(&[hr(0.02, 0.067, 0.069, 0.0), hr(-0.07, 0.065, 0.068, 0.002), hr(-0.17, 0.059, 0.062, 0.0), hr(-0.26, 0.053, 0.056, -0.002), hr(-0.30, 0.052, 0.054, -0.002)], 16, cloth);
    m.done()
}

fn forearm() -> (Vec<MeshVertex>, Vec<u32>) {
    let cloth = tinted(1.0, MAT_CLOTH);
    let mut m = MB::default();
    m.ball(Vec3::ZERO, Vec3::splat(0.054), 7, 14, cloth);
    m.sweep(&[hr(0.02, 0.052, 0.054, 0.0), hr(-0.06, 0.053, 0.056, 0.004), hr(-0.16, 0.046, 0.048, 0.002), hr(-0.205, 0.043, 0.045, 0.0)], 16, cloth);
    // Ribbed cuff.
    m.sweep(&[hr(-0.20, 0.045, 0.047, 0.0), hr(-0.248, 0.039, 0.041, 0.0)], 16, tinted(0.78, MAT_CLOTH));
    m.done()
}

/// A hand hanging from the wrist, palm towards the body: `side` is +1 for
/// the left hand (palm facing -X), -1 for the right.
fn hand(side: f32) -> (Vec<MeshVertex>, Vec<u32>) {
    let skin = tinted(1.0, MAT_SKIN);
    let mut m = MB::default();
    // The palm faces the thigh: its width runs front to back (z) and its
    // thickness across (x). The fingers curl in towards the palm.
    let ring = |x: f32, y: f32, thick: f32, width: f32, z: f32| Ring { c: Vec3::new(x, y, z), u: Vec3::X * thick, w: Vec3::Z * width, sq: 2.6 };
    let curl = -side;
    m.sweep(
        &[
            ring(0.0, 0.03, 0.022, 0.026, 0.0),
            ring(0.0, -0.005, 0.02, 0.033, 0.0),
            ring(0.0, -0.045, 0.019, 0.04, 0.002),
            ring(curl * 0.002, -0.085, 0.016, 0.041, 0.002),
            ring(curl * 0.008, -0.12, 0.013, 0.038, 0.001),
            ring(curl * 0.017, -0.15, 0.011, 0.034, 0.0),
            ring(curl * 0.024, -0.168, 0.008, 0.026, 0.0),
        ],
        14,
        skin,
    );
    // The thumb, along the front of the palm, turned in.
    m.sweep(&[hrx(curl * 0.006, -0.02, 0.011, 0.012, 0.03), hrx(curl * 0.012, -0.05, 0.010, 0.011, 0.045), hrx(curl * 0.018, -0.075, 0.008, 0.009, 0.048)], 10, skin);
    // Knuckles.
    m.ball(Vec3::new(-curl * 0.004, -0.085, 0.002), Vec3::new(0.014, 0.012, 0.041), 5, 12, tinted(0.96, MAT_SKIN));
    m.done()
}

fn thigh() -> (Vec<MeshVertex>, Vec<u32>) {
    let jeans = tinted(1.0, MAT_CLOTH);
    let mut m = MB::default();
    m.sweep(&[hr(0.07, 0.088, 0.092, -0.006), hr(0.0, 0.093, 0.097, 0.0), hr(-0.10, 0.089, 0.093, 0.008), hr(-0.25, 0.078, 0.082, 0.007), hr(-0.38, 0.065, 0.068, 0.006), hr(-0.44, 0.061, 0.064, 0.008)], 18, jeans);
    m.done()
}

fn shin() -> (Vec<MeshVertex>, Vec<u32>) {
    let jeans = tinted(1.0, MAT_CLOTH);
    let mut m = MB::default();
    // Knee, then the leg down to a hem that sits on the shoe.
    m.ball(Vec3::new(0.0, 0.0, 0.006), Vec3::new(0.063, 0.065, 0.065), 8, 16, jeans);
    m.sweep(&[hr(0.02, 0.06, 0.063, 0.004), hr(-0.08, 0.061, 0.066, -0.005), hr(-0.17, 0.058, 0.063, -0.008), hr(-0.29, 0.053, 0.056, -0.004), hr(-0.345, 0.054, 0.058, 0.0)], 18, jeans);
    m.sweep(&[hr(-0.335, 0.057, 0.061, 0.0), hr(-0.37, 0.057, 0.061, 0.0)], 18, tinted(0.85, MAT_CLOTH));
    m.done()
}

fn foot() -> (Vec<MeshVertex>, Vec<u32>) {
    let upper = tinted(1.0, MAT_SHOE);
    let sole = fixed(0.82, 0.82, 0.80);
    let mut m = MB::default();
    // Ankle in a dark sock, inside the shoe's collar.
    m.sweep(&[hr(0.04, 0.035, 0.038, -0.005), hr(-0.03, 0.036, 0.04, -0.005)], 12, fixed(0.03, 0.03, 0.035));
    // Rubber sole: heel to toe, flat underneath (its bottom is SOLE_DY).
    let hs = 0.0185;
    let y = SOLE_DY + hs;
    m.sweep(&[vr(HEEL_Z, 0.036, hs, y, 4.0), vr(-0.05, 0.046, hs, y, 4.0), vr(0.05, 0.049, hs, y, 4.0), vr(0.15, 0.053, hs, y, 4.0), vr(0.195, 0.046, hs * 0.95, y, 4.0), vr(TOE_Z, 0.022, hs * 0.8, y + 0.001, 4.0)], 18, sole);
    // The upper: heel counter, collar, instep and toe box.
    m.sweep(
        &[
            vr(-0.07, 0.036, 0.045, -0.025, 2.6),
            vr(-0.03, 0.043, 0.058, -0.012, 2.6),
            vr(0.03, 0.046, 0.05, -0.02, 2.6),
            vr(0.10, 0.049, 0.033, -0.036, 2.6),
            vr(0.16, 0.048, 0.025, -0.043, 2.6),
            vr(0.2, 0.032, 0.017, -0.048, 2.6),
        ],
        18,
        upper,
    );
    // Laces across the instep, and a stripe down each side.
    for (k, z) in [0.035f32, 0.06, 0.085].iter().enumerate() {
        let top = 0.03 - 0.33 * (z - 0.03) - 0.002 * k as f32;
        m.cuboid(Vec3::new(-0.022, top - 0.004, z - 0.006), Vec3::new(0.022, top + 0.002, z + 0.006), fixed(0.86, 0.86, 0.84));
    }
    for s in [-1.0f32, 1.0] {
        m.cuboid(Vec3::new(s * 0.0475 - 0.003, -0.05, 0.0), Vec3::new(s * 0.0475 + 0.003, -0.03, 0.09), fixed(0.85, 0.85, 0.83));
    }
    m.done()
}

/// The avatar's meshes, built once.
pub fn meshes() -> &'static Vec<(Vec<MeshVertex>, Vec<u32>)> {
    static M: OnceLock<Vec<(Vec<MeshVertex>, Vec<u32>)>> = OnceLock::new();
    M.get_or_init(|| {
        let all = vec![pelvis(), torso(), head(), upper_arm(), forearm(), hand(1.0), upper_arm(), forearm(), hand(-1.0), thigh(), shin(), foot(), thigh(), shin(), foot(), hair()];
        debug_assert_eq!(all.len(), PARTS);
        all
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standing() -> Motion {
        Motion { phase: 0.0, speed: 0.0, air: 0.0, time: 0.0 }
    }

    /// Where a point of a part is, in the body frame.
    fn point(rig: &RigPose, part: usize, p: Vec3) -> Vec3 {
        rig.parts[part].transform_point3(p)
    }

    #[test]
    fn parts_and_meshes_line_up() {
        let meshes = meshes();
        assert_eq!(meshes.len(), PARTS);
        assert_eq!(NAMES.len(), PARTS);
        assert_eq!(BASE, crate::birds::BASE + 30);
        for (i, (v, idx)) in meshes.iter().enumerate() {
            assert!(!v.is_empty(), "{} has vertices", NAMES[i]);
            assert!(!idx.is_empty() && idx.len() % 3 == 0, "{} has triangles", NAMES[i]);
            assert!(idx.iter().all(|k| (*k as usize) < v.len()), "{} indices in range", NAMES[i]);
            assert!(v.iter().all(|x| x.pos.iter().all(|f| f.is_finite()) && x.nrm.iter().all(|f| f.is_finite())), "{} finite", NAMES[i]);
            assert!(v.iter().all(|x| (Vec3::from(x.nrm).length() - 1.0).abs() < 1e-3), "{} unit normals", NAMES[i]);
        }
    }

    #[test]
    fn palettes_are_in_range() {
        for style in 0..STYLES as u8 {
            let p = palette(style);
            for c in [p.skin, p.cloth, p.pants, p.hair, p.shoe] {
                assert!(c.iter().all(|v| (0.0..=1.0).contains(v)), "style {style}: {c:?}");
            }
        }
        assert_eq!(palette(0), palette(255));
        assert_ne!(palette(0), palette(1));
    }

    #[test]
    fn standing_figure_has_human_proportions() {
        let rig = pose(&standing());
        // Feet on the ground, not in it and not above it.
        assert!(rig.sole_low().abs() < 1e-4, "soles at {}", rig.sole_low());
        // About 1.8 m to the crown of the hair.
        let crown = point(&rig, HAIR, Vec3::new(0.0, 0.301, -0.01)).y;
        assert!((1.76..1.86).contains(&crown), "height {crown:.3}");
        let shoulder = point(&rig, UARM_L, Vec3::ZERO);
        assert!((1.40..1.50).contains(&shoulder.y), "shoulder height {:.3}", shoulder.y);
        let wrist = point(&rig, HAND_L, Vec3::ZERO);
        assert!((0.80..0.95).contains(&wrist.y), "wrist height {:.3}", wrist.y);
        // The left arm is on the figure's left (+X) and hangs clear of the hip.
        assert!(wrist.x > HIP_X + 0.1, "wrist out at {:.3}", wrist.x);
    }

    #[test]
    fn walking_plants_a_foot_and_swings_the_limbs() {
        for speed in [1.6, 4.5, 7.0] {
            let mut lowest = f32::MAX;
            let mut highest = f32::MIN;
            let mut ahead = (f32::MAX, f32::MIN);
            for k in 0..64 {
                let m = Motion { phase: k as f64 * std::f64::consts::TAU / 64.0, speed, air: 0.0, time: 0.0 };
                let rig = pose(&m);
                assert!(rig.is_finite());
                // Grounded, the figure always rests on its lower sole.
                assert!(rig.sole_low().abs() < 1e-4, "{speed} m/s, phase {k}: soles at {}", rig.sole_low());
                let ankle = point(&rig, FOOT_L, Vec3::ZERO);
                lowest = lowest.min(ankle.y);
                highest = highest.max(ankle.y);
                ahead = (ahead.0.min(ankle.z), ahead.1.max(ankle.z));
            }
            // The foot lifts through the swing and travels fore and aft.
            assert!(highest - lowest > 0.06, "{speed} m/s: foot lift {:.3}", highest - lowest);
            assert!(ahead.1 - ahead.0 > 0.4, "{speed} m/s: foot travel {:.3}", ahead.1 - ahead.0);
        }
        // The arm swings against the leg: left foot forward, left hand back.
        let rig = pose(&Motion { phase: 0.0, speed: 1.6, air: 0.0, time: 0.0 });
        assert!(point(&rig, FOOT_L, Vec3::ZERO).z > 0.1, "left foot ahead at its strike");
        assert!(point(&rig, HAND_L, Vec3::ZERO).z < point(&rig, HAND_R, Vec3::ZERO).z, "left hand behind the right");
    }

    #[test]
    fn the_jump_tucks_the_legs() {
        let rig = pose(&Motion { air: 1.0, ..standing() });
        assert!(rig.sole_low() > 0.05, "feet lifted: {}", rig.sole_low());
        assert!(pose(&Motion { air: 0.5, ..standing() }).is_finite());
    }

    #[test]
    fn poses_are_finite_and_instanced() {
        let root = geo::lla_to_pos(0.5, 0.1, 0.0);
        let cam = root + DVec3::new(0.0, 0.0, 8.0);
        for k in 0..24 {
            let m = Motion { phase: k as f64 * 0.31, speed: k as f64 * 0.35, air: (k % 5) as f64 * 0.25, time: k as f64 };
            let rig = pose(&m);
            assert!(rig.is_finite(), "{m:?}");
            let mut out = Vec::new();
            instances(cam, root, 0.7, &rig, 2, &mut out);
            assert_eq!(out.len(), PARTS);
            assert!(out.iter().all(|(id, inst)| (BASE..BASE + PARTS as u32).contains(id) && inst.m.iter().flatten().all(|v| v.is_finite())));
            assert!(out.iter().all(|(_, inst)| inst.ext[3] == 2.0 && inst.ext[1] == 1.0));
        }
    }

    #[test]
    fn a_broken_position_draws_nothing() {
        let mut out = Vec::new();
        instances(DVec3::NAN, DVec3::ZERO, 0.0, &pose(&standing()), 0, &mut out);
        assert!(out.is_empty());
    }
}
