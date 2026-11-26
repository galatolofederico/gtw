//! The player's avatar: a rigid, multi-part figure posed procedurally.
//!
//! The avatar is sixteen rigid pieces (pelvis,
//! torso, head, upper and lower arms, hands, thighs, shins, feet and hair)
//! drawn through the instanced solid path (`inst.wgsl`, selected by
//! `Inst.ext.w = 2`). Each piece is a smooth swept surface built around its
//! joint, with rounded joint caps that overlap the neighbouring piece, so
//! the figure stays closed as it bends.
//!
//! The figure is about 1.80 m tall (7.5 heads): hip joints at 0.97 m, knees
//! at 0.51 m, shoulders at 1.44 m. It wears a hoodie over a tee, jeans with
//! a belt, and sneakers.
//!
//! The pose is a distance-integrated gait cycle (`Character::gait_phase`),
//! signed body-local velocity and explicit aircraft/car bailout states.
//! Walking and running use two-bone leg IK with a planted stance path, heel
//! roll, toe-off and knee-led swing; running has two real flight phases.
//! Directional input gives backwards and sideways steps their own motion.
//! Aircraft exits develop into a slightly buffeted stable arch, and a car
//! exit moves through a braced hop, shoulder-led tucked roll and hand/foot-
//! supported recovery. `RigPose` remains a plain array of
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
const HIP_Y: f32 = 1.0;
/// Hip joints from the hip centre.
const HIP_X: f32 = 0.095;
const HIP_DY: f32 = -0.03;
/// Waist (the torso pivot) above the hip centre.
const WAIST_DY: f32 = 0.09;
/// Shoulder joints and the neck base from the waist.
const SHOULDER_X: f32 = 0.185;
const SHOULDER_DY: f32 = 0.38;
const NECK_DY: f32 = 0.44;
/// Bone lengths. The pelvis-to-ankle measurement is about half the figure's
/// height; the old 0.84 m legs had to lock their knees on every walking step.
const UPPER_ARM: f32 = 0.28;
const FOREARM: f32 = 0.25;
const THIGH: f32 = 0.46;
const SHIN: f32 = 0.44;
/// Keep a little knee flexion even at maximum leg extension.
const LEG_REACH: f32 = (THIGH + SHIN) * 0.99;
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

/// What the body is doing, for the pose. Distances and speeds are in metres
/// and seconds; `forward`/`side` are velocity in the figure's own frame, so a
/// backwards step does not accidentally play the forward gait.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Motion {
    /// Gait cycle (radians): a half turn per footstep, continuous between
    /// them (`Character::gait_phase`). The left foot strikes at 0.
    pub phase: f64,
    /// Horizontal speed (m/s).
    pub speed: f64,
    /// Signed velocity along the body's forward axis (m/s).
    pub forward: f64,
    /// Signed velocity along the body's +X (left) axis (m/s).
    pub side: f64,
    /// Vertical velocity (m/s), used to distinguish take-off from descent.
    pub vertical: f64,
    /// 0 on the ground .. 1 in the air (`Character::air`).
    pub air: f64,
    /// 0 not falling .. 1 belly to earth in freefall (`bailout::Fall::pose`).
    pub fall: f64,
    /// 0 no canopy .. 1 hanging under a fully open one.
    pub chute: f64,
    /// How the pilot left, for the first moment of the jump/ejection. `None`
    /// denotes an already established fall (also convenient in pose tests).
    pub plane_exit: Option<crate::bailout::Exit>,
    /// Seconds since leaving the aircraft.
    pub fall_age: f64,
    /// Canopy steering input, -1 left .. 1 right.
    pub steer: f64,
    /// The complete moving-car exit: push-off, ground roll, or recovery.
    pub car: Option<crate::character::CarBailout>,
    /// Seconds, for breathing, wind buffeting and idle sway.
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

    /// The actual lowest mesh vertex, not merely a joint origin. During a
    /// roll this may be a shoulder, a knee or a hand, so contact needs the
    /// complete figure rather than the old feet-only correction.
    pub fn mesh_low(&self) -> f32 {
        meshes()
            .iter()
            .zip(self.parts.iter())
            .flat_map(|((vertices, _), matrix)| vertices.iter().map(move |v| matrix.transform_point3(Vec3::from(v.pos)).y))
            .fold(f32::MAX, f32::min)
    }

    fn translated(&mut self, x: f32, y: f32, z: f32) {
        let shift = at(x, y, z);
        for part in self.parts.iter_mut() {
            *part = shift * *part;
        }
    }
}

/// Shoulder and lower chest attachments for the parachute, taken from the
/// actual posed rig rather than an independently estimated body tilt. This
/// includes the aircraft-exit transition as well as full canopy flight.
pub fn parachute_harness(rig: &RigPose, left: bool) -> (Vec3, Vec3) {
    let (shoulder, side) = if left { (UARM_L, 1.0f32) } else { (UARM_R, -1.0f32) };
    let high = rig.parts[shoulder].transform_point3(Vec3::ZERO);
    let low = rig.parts[TORSO].transform_point3(Vec3::new(side * 0.16, -0.10, 0.02));
    (high, low)
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

/// Joint angles for one leg. Rotations are about +X: positive swings a
/// hanging limb back, so a thigh forward is negative and a bent knee
/// positive. `yaw` points the stepping plane into the actual movement
/// direction, while `out` spreads the leg sideways for balance and freefall.
#[derive(Copy, Clone)]
struct Leg {
    hip: f32,
    knee: f32,
    ankle: f32,
    yaw: f32,
    out: f32,
}

/// Joint angles for one arm.
#[derive(Copy, Clone)]
struct Arm {
    swing: f32,
    out: f32,
    elbow: f32,
}

/// Every angle of a pose, including the pelvis translation. Keeping root
/// motion in the pose is important: a run's bounce must not be cancelled by
/// indiscriminately moving the lowest foot back to the ground.
#[derive(Copy, Clone)]
struct Angles {
    legs: [Leg; 2],
    arms: [Arm; 2],
    lean: f32,
    twist: f32,
    tilt: f32,
    sway: f32,
    head: f32,
    rise: f32,
    shift: f32,
}

fn direct_leg(hip: f32, knee: f32, ankle: f32) -> Leg {
    Leg { hip, knee, ankle, yaw: 0.0, out: 0.0 }
}

/// An ankle trajectory in the stepping plane. `z` is positive towards the
/// direction of travel, not necessarily towards the figure's facing.
#[derive(Copy, Clone)]
struct Step {
    z: f32,
    y: f32,
    pitch: f32,
    stance: bool,
    progress: f32,
}

/// Ankle height that puts the lower of the heel and toe exactly at y=0 for
/// a requested foot pitch. This gives a real heel roll and toe push-off.
fn contact_ankle_y(pitch: f32) -> f32 {
    let point_y = |z: f32| SOLE_DY * pitch.cos() - z * pitch.sin();
    -point_y(HEEL_Z).min(point_y(TOE_Z))
}

fn gait_step(q: f32, duty: f32, travel: f32, lift: f32, go: f32) -> Step {
    if q < duty {
        // A planted sole travels backwards relative to the body at a steady
        // rate. Heel roll and toe push-off change its height without sliding
        // the contact point through the floor.
        let u = q / duty.max(0.01);
        let pitch = (-0.13 * (1.0 - smooth(0.0, 0.18, u)) + 0.50 * smooth(0.66, 1.0, u)) * go;
        Step { z: travel * (0.5 - u), y: contact_ankle_y(pitch), pitch, stance: true, progress: u }
    } else {
        let u = ((q - duty) / (1.0 - duty).max(0.01)).clamp(0.0, 1.0);
        // A cubic recovery with the same endpoint velocity as the planted
        // sweep. The foot continues briefly backwards after toe-off, reaches
        // ahead of its landing point, then settles onto the next heel strike.
        // A plain smoothstep stopped the foot dead at both contacts and made
        // its velocity snap, which was a major source of the marionette look.
        let swing = 1.0 - duty;
        let endpoint = -swing / duty.max(0.01);
        let u2 = u * u;
        let u3 = u2 * u;
        let ease = -2.0 * u3 + 3.0 * u2 + endpoint * u * (1.0 - u) * (1.0 - 2.0 * u);
        let z = mix(-travel * 0.5, travel * 0.5, ease);
        let pitch = mix(0.50, -0.13, smooth(0.04, 0.82, u)) * go;
        // An exponent above one leaves zero vertical velocity at both ends.
        let clearance = lift * (PI * u).sin().max(0.0).powf(1.35);
        Step { z, y: contact_ankle_y(pitch) + clearance, pitch, stance: false, progress: u }
    }
}

/// Analytic two-bone IK. Given an ankle relative to the hip and a desired
/// world-space foot pitch, return joint rotations without hyperextending the
/// knee. The knee bends forwards, as a human knee does.
fn leg_to(dy: f32, dz: f32, pitch: f32, yaw: f32) -> Leg {
    let raw = dy.hypot(dz).max(0.001);
    let d = raw.clamp(0.16, LEG_REACH);
    let dy = dy * d / raw;
    let dz = dz * d / raw;
    let knee = ((d * d - THIGH * THIGH - SHIN * SHIN) / (2.0 * THIGH * SHIN)).clamp(-1.0, 1.0).acos();
    let target = -dz.atan2(-dy);
    let knee_offset = (SHIN * knee.sin()).atan2(THIGH + SHIN * knee.cos());
    let hip = target - knee_offset;
    Leg { hip, knee, ankle: pitch - hip - knee, yaw, out: 0.0 }
}

/// Three-dimensional foot placement. `dx` is across the stepping plane and
/// `dz` along it; `side` is +1 for the left leg and -1 for the right. Spreading
/// the hip before flexing it lets the pelvis transfer weight while the shoes
/// stay planted, including during sideways and backwards steps.
fn leg_to_offset(dx: f32, dy: f32, dz: f32, pitch: f32, yaw: f32, side: f32) -> Leg {
    let spread = dx.atan2(-dy);
    let planar_dy = -dx.hypot(dy);
    let mut leg = leg_to(planar_dy, dz, pitch, yaw);
    leg.out = side * spread;
    leg
}

fn locomotion(m: &Motion) -> Angles {
    let v = if m.speed.is_finite() { m.speed.max(0.0) as f32 } else { 0.0 };
    let q = (m.phase as f32).rem_euclid(TAU) / TAU;
    let t = if m.time.is_finite() { m.time as f32 } else { 0.0 };
    let go = smooth(0.05, 0.35, v);
    let run = smooth(1.9, 4.5, v);
    let sprint = smooth(4.8, 7.0, v);

    // Old callers that only supply scalar speed mean forwards. New callers
    // pass signed local velocity, enabling backwards and sideways gaits.
    let mut f = if m.forward.is_finite() { m.forward as f32 } else { 0.0 };
    let x = if m.side.is_finite() { m.side as f32 } else { 0.0 };
    if f == 0.0 && x == 0.0 {
        f = v;
    }
    let move_len = f.hypot(x);
    let (move_x, move_z, yaw) = if move_len > 0.001 && go > 0.001 {
        (x / move_len, f / move_len, x.atan2(f))
    } else {
        (0.0, 1.0, 0.0)
    };

    let duty = crate::character::gait_duty(v as f64) as f32;
    let stride = crate::character::stride_len(v as f64) as f32;
    // One full cycle covers two strides, and this foot is planted for
    // `duty` of it. Sweeping by exactly that distance cancels the body's
    // world-space travel instead of merely looking plausible in isolation.
    let travel = 2.0 * duty * stride * go;
    let lift = (mix(0.075, 0.17, run) + 0.02 * sprint) * go;
    let steps = [gait_step(q, duty, travel, lift, go), gait_step((q + 0.5).fract(), duty, travel, lift, go)];

    let cycle = TAU * q;
    let breathe = (t * 1.7).sin() * (1.0 - go);
    let idle_shift = (t * 0.45).sin() * (1.0 - go);
    // The pelvis travels over the support area instead of staying midway
    // between two unreachable heels. Its lateral shift peaks over the stance
    // foot. Both shifts are subtracted from the hip-to-ankle IK vectors, so
    // this moves the body over the feet rather than dragging the feet along.
    let fore_aft = mix(0.08, 0.04, run) * (2.0 * cycle).cos() * go;
    let lateral = 0.026 * cycle.sin() * go * (1.0 - 0.55 * run) + 0.012 * idle_shift;
    let sway = fore_aft * move_x + lateral;
    let shift = fore_aft * move_z;
    let leg_plane = |s: Step| {
        let dx = -lateral * yaw.cos();
        let dz = s.z - fore_aft - lateral * yaw.sin();
        (dx, dz)
    };

    // Weight acceptance dips the pelvis gently; the run's two flight arcs
    // rise above it. Crucially, include the approaching swing foot as well as
    // the stance foot in the reach constraint. Considering a foot only after
    // touchdown caused an instantaneous 10-30 cm drop at every contact.
    let mut rise = -mix(0.006, 0.040, run) * go - 0.020 * sprint * go + 0.003 * breathe;
    if run > 0.001 {
        if let Some(s) = steps.iter().find(|s| s.stance) {
            rise -= 0.025 * run * (PI * s.progress).sin() * go;
        } else {
            let start = if q < 0.5 { duty } else { duty + 0.5 };
            let u = ((q - start) / (0.5 - duty).max(0.01)).clamp(0.0, 1.0);
            rise += 0.065 * run * (PI * u).sin() * go;
        }
    }
    rise -= 0.010 * (2.0 * cycle).cos() * (1.0 - run) * go;
    for &s in &steps {
        let (dx, dz) = leg_plane(s);
        let horizontal = dx * dx + dz * dz;
        let needed = s.y + (LEG_REACH * LEG_REACH - horizontal).max(0.01).sqrt() - (HIP_Y + HIP_DY);
        rise = rise.min(needed);
    }
    rise = rise.max(-0.20);
    let hip_y = HIP_Y + HIP_DY + rise;
    let leg = |s: Step, side: f32| {
        let (dx, dz) = leg_plane(s);
        leg_to_offset(dx, s.y - hip_y, dz, s.pitch, yaw, side)
    };
    let legs = [leg(steps[0], 1.0), leg(steps[1], -1.0)];

    // Arms counter-swing, with relaxed elbows for a walk and compact, powerful
    // arm drive for a sprint. The elbow closes as the hand comes forwards
    // instead of bending most while it is behind the hip.
    let amp = (mix(0.30, 0.80, run) + 0.055 * sprint) * go;
    let arm = |opposite: f32| {
        let swing = amp * cycle.cos() * opposite * move_z;
        let forward = if amp > 0.001 { (-swing).max(0.0) / amp } else { 0.0 };
        let elbow = -(0.14 + (mix(0.20, 1.25, run) + 0.10 * sprint + mix(0.06, 0.20, run) * forward) * go);
        Arm { swing, out: 0.11 + 0.015 * run * go + 0.15 * move_x.abs() * go, elbow }
    };
    let arms = [arm(1.0), arm(-1.0)];
    Angles {
        legs,
        arms,
        lean: (mix(0.035, 0.14, run) + 0.075 * sprint) * go * move_z + 0.012 * breathe,
        twist: -(0.11 + 0.03 * sprint) * cycle.cos() * go * move_z,
        tilt: 0.030 * cycle.sin() * go * (1.0 - 0.65 * run) + 0.016 * idle_shift,
        sway,
        head: -0.018 * (t * 0.8).sin() * (1.0 - go),
        rise,
        shift,
    }
}

fn jump(m: &Motion) -> Angles {
    let vertical = if m.vertical.is_finite() { m.vertical as f32 } else { 0.0 };
    let falling = 1.0 - smooth(-5.0, -0.5, vertical);
    let tuck = |hip: f32, knee: f32, out: f32| Leg { hip: mix(hip, -0.28, falling), knee: mix(knee, 0.72, falling), ankle: 0.18 - hip - knee, yaw: 0.0, out };
    Angles {
        legs: [tuck(-0.85, 1.45, 0.10), tuck(-0.50, 1.10, 0.10)],
        arms: [
            Arm { swing: mix(-0.95, -0.25, falling), out: mix(0.35, 0.82, falling), elbow: mix(-0.90, -0.35, falling) },
            Arm { swing: mix(-0.65, -0.38, falling), out: mix(0.35, 0.76, falling), elbow: mix(-0.78, -0.42, falling) },
        ],
        lean: mix(0.12, 0.025, falling),
        twist: 0.0,
        tilt: 0.0,
        sway: 0.0,
        head: mix(-0.10, 0.08, falling),
        rise: 0.0,
        shift: 0.0,
    }
}

fn freefall_settle(m: &Motion) -> f32 {
    if m.plane_exit.is_none() { 1.0 } else { smooth(0.10, 1.25, m.fall_age.max(0.0) as f32) }
}

/// The aircraft exit and the stable arch are deliberately separate. A pilot
/// first steps clear / takes the ejection posture, then spreads into a
/// belly-to-earth box with small asynchronous wind corrections.
fn freefall(m: &Motion) -> Angles {
    let settle = freefall_settle(m);
    let t = if m.time.is_finite() { m.time as f32 } else { 0.0 };
    let gust = 0.025 + 0.035 * settle;
    let left_leg = Leg {
        hip: -0.24 + gust * (t * 5.1).sin(),
        knee: 0.86 + 0.06 * (t * 3.7).sin(),
        ankle: -0.20,
        yaw: 0.08,
        out: 0.27 + 0.025 * (t * 4.3).cos(),
    };
    let right_leg = Leg {
        hip: -0.18 + gust * (t * 4.6 + 1.2).sin(),
        knee: 0.78 + 0.055 * (t * 4.1 + 0.8).sin(),
        ankle: -0.18,
        yaw: -0.08,
        out: 0.25 + 0.025 * (t * 3.9 + 0.5).cos(),
    };
    let stable = Angles {
        legs: [left_leg, right_leg],
        arms: [
            Arm { swing: -0.25 + gust * (t * 5.7).sin(), out: 1.16 + 0.035 * (t * 4.8).sin(), elbow: -1.20 },
            Arm { swing: -0.20 + gust * (t * 5.3 + 0.9).sin(), out: 1.20 + 0.035 * (t * 4.5 + 0.7).sin(), elbow: -1.16 },
        ],
        lean: -0.12,
        twist: 0.025 * (t * 2.2).sin(),
        tilt: 0.022 * (t * 2.8 + 0.4).sin(),
        sway: 0.0,
        head: -0.24,
        rise: 0.0,
        shift: 0.0,
    };
    let exit = match m.plane_exit.unwrap_or(crate::bailout::Exit::Jump) {
        crate::bailout::Exit::Jump => Angles {
            legs: [direct_leg(-0.72, 0.72, 0.10), direct_leg(0.18, 0.82, -0.05)],
            arms: [Arm { swing: -1.30, out: 0.65, elbow: -0.48 }, Arm { swing: 0.38, out: 0.92, elbow: -0.40 }],
            lean: 0.20,
            twist: -0.16,
            tilt: 0.10,
            sway: 0.06,
            head: -0.12,
            rise: -0.02,
            shift: 0.08,
        },
        crate::bailout::Exit::Door => Angles {
            legs: [direct_leg(-0.48, 1.14, 0.08), direct_leg(-0.20, 0.92, 0.02)],
            arms: [Arm { swing: -0.95, out: 0.52, elbow: -1.42 }, Arm { swing: -0.68, out: 0.60, elbow: -1.35 }],
            lean: 0.28,
            twist: 0.12,
            tilt: -0.08,
            sway: -0.04,
            head: -0.18,
            rise: -0.06,
            shift: 0.03,
        },
        crate::bailout::Exit::Eject => Angles {
            legs: [direct_leg(-1.18, 1.92, 0.08), direct_leg(-1.10, 1.86, 0.06)],
            arms: [Arm { swing: -0.86, out: 0.25, elbow: -1.24 }, Arm { swing: -0.82, out: 0.25, elbow: -1.20 }],
            lean: 0.24,
            twist: 0.0,
            tilt: 0.0,
            sway: 0.0,
            head: 0.12,
            rise: -0.05,
            shift: -0.04,
        },
    };
    blend(exit, stable, settle)
}

fn freefall_tilt(m: &Motion) -> f32 {
    let initial = match m.plane_exit {
        Some(crate::bailout::Exit::Jump) => 0.45,
        Some(crate::bailout::Exit::Door) => 0.15,
        Some(crate::bailout::Exit::Eject) => -0.35,
        None => crate::bailout::FREEFALL_TILT as f32,
    };
    mix(initial, crate::bailout::FREEFALL_TILT as f32, freefall_settle(m))
}

/// Hanging under a canopy, hands on the risers. The knees and torso react
/// gently to steering instead of becoming a frozen T-shaped parachute icon.
fn under_canopy(m: &Motion) -> Angles {
    let t = if m.time.is_finite() { m.time as f32 } else { 0.0 };
    let steer = (m.steer as f32).clamp(-1.0, 1.0);
    Angles {
        legs: [
            Leg { hip: -0.10 + 0.05 * steer, knee: 0.30, ankle: -0.12, yaw: 0.0, out: 0.08 },
            Leg { hip: 0.02 + 0.05 * steer, knee: 0.23, ankle: -0.10, yaw: 0.0, out: 0.08 },
        ],
        arms: [
            Arm { swing: -2.42, out: 0.22, elbow: -0.24 + 0.12 * steer },
            Arm { swing: -2.42, out: 0.22, elbow: -0.24 - 0.12 * steer },
        ],
        lean: 0.025 + 0.018 * (t * 1.5).sin(),
        twist: 0.08 * steer,
        tilt: 0.06 * steer + 0.015 * (t * 1.2).sin(),
        sway: 0.0,
        head: 0.04,
        rise: 0.0,
        shift: 0.0,
    }
}

fn car_exit() -> Angles {
    Angles {
        legs: [direct_leg(-0.50, 0.82, 0.05), direct_leg(0.12, 1.02, -0.02)],
        arms: [Arm { swing: -1.12, out: 0.70, elbow: -0.76 }, Arm { swing: -0.42, out: 0.95, elbow: -0.92 }],
        lean: 0.36,
        twist: -0.18,
        tilt: 0.12,
        sway: 0.04,
        head: -0.24,
        rise: -0.04,
        shift: 0.06,
    }
}

/// Protect the head, pull the knees in, then let the tuck open slightly as
/// the body comes back to its feet. Asymmetry makes this a shoulder roll,
/// rather than a rigid mannequin rotating around one perfect axis.
fn car_roll_angles(progress: f32) -> Angles {
    let p = progress.clamp(0.0, 1.0);
    let open = smooth(0.68, 1.0, p);
    let tuck = 1.0 - 0.30 * open;
    Angles {
        legs: [
            Leg { hip: mix(-1.18, -0.76, open), knee: mix(2.02, 1.46, open), ankle: -0.12, yaw: 0.0, out: 0.14 },
            Leg { hip: mix(-0.98, -0.58, open), knee: mix(1.88, 1.24, open), ankle: -0.10, yaw: 0.0, out: 0.18 },
        ],
        arms: [
            Arm { swing: mix(-1.18, -0.76, open), out: 0.34, elbow: -1.58 * tuck },
            Arm { swing: mix(-0.88, -0.48, open), out: 0.48, elbow: -1.46 * tuck },
        ],
        lean: mix(0.60, 0.42, open),
        twist: -0.16 * (PI * p).sin(),
        tilt: 0.10 * (PI * p).sin(),
        sway: 0.04 * (PI * p).sin(),
        head: mix(-0.55, -0.32, open),
        rise: -0.12,
        shift: 0.0,
    }
}

/// Plant a hand and one foot, bring the hips over the support foot, then
/// uncurl. The first frame exactly matches the end of the tucked roll.
fn car_recovery(m: &Motion, progress: f32) -> Angles {
    let p = progress.clamp(0.0, 1.0);
    let rise = mix(-0.38, -0.14, smooth(0.12, 0.72, p));
    let hip_y = HIP_Y + HIP_DY + rise;
    let planted = Angles {
        legs: [leg_to(contact_ankle_y(0.0) - hip_y, 0.22, 0.0, 0.0), leg_to(contact_ankle_y(0.0) - hip_y, -0.16, 0.0, 0.0)],
        arms: [
            Arm { swing: mix(-1.02, -0.38, smooth(0.18, 0.78, p)), out: 0.22, elbow: -0.30 },
            Arm { swing: mix(-0.70, -0.20, smooth(0.18, 0.78, p)), out: 0.28, elbow: -0.42 },
        ],
        lean: mix(0.70, 0.28, smooth(0.12, 0.74, p)),
        twist: -0.10 * (1.0 - p),
        tilt: 0.08 * (1.0 - p),
        sway: 0.035 * (1.0 - p),
        head: mix(-0.36, -0.10, p),
        rise,
        shift: 0.0,
    };
    let down = blend(car_roll_angles(1.0), planted, smooth(0.0, 0.28, p));
    blend(down, locomotion(m), smooth(0.48, 1.0, p))
}

fn blend_angle(a: f32, b: f32, k: f32) -> f32 {
    let d = (b - a + PI).rem_euclid(TAU) - PI;
    a + d * k
}

fn blend(a: Angles, b: Angles, k: f32) -> Angles {
    let k = k.clamp(0.0, 1.0);
    let l = |x: &Leg, y: &Leg| Leg {
        hip: mix(x.hip, y.hip, k),
        knee: mix(x.knee, y.knee, k),
        ankle: mix(x.ankle, y.ankle, k),
        yaw: blend_angle(x.yaw, y.yaw, k),
        out: mix(x.out, y.out, k),
    };
    let r = |x: &Arm, y: &Arm| Arm { swing: mix(x.swing, y.swing, k), out: mix(x.out, y.out, k), elbow: mix(x.elbow, y.elbow, k) };
    Angles {
        legs: [l(&a.legs[0], &b.legs[0]), l(&a.legs[1], &b.legs[1])],
        arms: [r(&a.arms[0], &b.arms[0]), r(&a.arms[1], &b.arms[1])],
        lean: mix(a.lean, b.lean, k),
        twist: mix(a.twist, b.twist, k),
        tilt: mix(a.tilt, b.tilt, k),
        sway: mix(a.sway, b.sway, k),
        head: mix(a.head, b.head, k),
        rise: mix(a.rise, b.rise, k),
        shift: mix(a.shift, b.shift, k),
    }
}

fn ground_contact(rig: &mut RigPose, margin: f32, share: f32) {
    let lift = (margin - rig.mesh_low()) * share.clamp(0.0, 1.0);
    if lift.is_finite() {
        rig.translated(0.0, lift, 0.0);
    }
}

/// Build the current motion. Walking/running use planted IK foot paths, an
/// aircraft exit resolves into a wind-driven arch, and the car rollout has
/// actual full-mesh ground contact followed by a distinct recovery.
pub fn pose(m: &Motion) -> RigPose {
    let air = (m.air as f32).clamp(0.0, 1.0);
    let fall = (m.fall as f32).clamp(0.0, 1.0);
    let chute = (m.chute as f32).clamp(0.0, 1.0);
    let locomotion_pose = locomotion(m);
    let mut a = blend(locomotion_pose, jump(m), air);
    let mut body_tilt = 0.0;
    let mut ground_roll = None;
    let mut ground_recovery = false;

    if let Some(car) = m.car {
        match car.phase {
            crate::character::CarBailoutPhase::Air => a = car_exit(),
            crate::character::CarBailoutPhase::Roll => {
                let p = (car.roll / std::f64::consts::TAU).clamp(0.0, 1.0) as f32;
                // The previous airborne frame and the first roll frame are
                // exactly the same pose. Impact contraction, bank and ground
                // resolution arrive over 0.18 s instead of snapping a knee
                // through a radian and dropping the entire mesh in one frame.
                let impact = smooth(0.0, 0.18, car.t.max(0.0) as f32);
                a = blend(car_exit(), car_roll_angles(p), impact);
                ground_roll = Some((car.roll as f32, 0.17 * (PI * p).sin() * impact, impact));
            }
            crate::character::CarBailoutPhase::Stand => {
                let p = (car.t / crate::character::CAR_STAND_TIME).clamp(0.0, 1.0) as f32;
                a = car_recovery(m, p);
                ground_recovery = true;
            }
        }
    } else if fall + chute > 0.0001 {
        // Blend the two aerial destinations with each other first. Sequentially
        // blending jump -> freefall -> canopy used to leave 25% of the jump
        // pose in a half-open parachute and made the opening look broken.
        let total = fall + chute;
        let canopy_share = chute / total;
        let aerial = blend(freefall(m), under_canopy(m), canopy_share);
        a = blend(a, aerial, total.min(1.0));
        body_tilt = (freefall_tilt(m) * fall + crate::bailout::CANOPY_TILT as f32 * chute) / total * total.min(1.0);
    }

    let mut rig = build(&a);
    if let Some((roll, bank, impact)) = ground_roll.filter(|(roll, _, _)| roll.is_finite()) {
        let pivot = HIP_Y + a.rise;
        let turn = at(0.0, pivot, a.shift) * rz(bank) * rx(roll) * at(0.0, -pivot, -a.shift);
        for part in rig.parts.iter_mut() {
            *part = turn * *part;
        }
        ground_contact(&mut rig, 0.006, impact);
        return rig;
    }
    if ground_recovery {
        ground_contact(&mut rig, 0.003, 1.0);
        return rig;
    }

    if body_tilt.abs() > 0.0001 {
        let pivot = HIP_Y + a.rise;
        let turn = at(0.0, pivot, a.shift) * rx(body_tilt.clamp(-0.4, 1.45)) * at(0.0, -pivot, -a.shift);
        for part in rig.parts.iter_mut() {
            *part = turn * *part;
        }
    }

    // Only a genuine stance phase gets a sole correction. In particular a
    // running flight phase is allowed to have both feet above the ground.
    if air < 1.0 && crate::character::gait_has_stance(m.phase, m.speed) {
        let lift = -rig.sole_low() * (1.0 - air);
        if lift.is_finite() && lift.abs() > 0.000001 {
            rig.translated(0.0, lift, 0.0);
        }
    }
    rig
}

fn build(a: &Angles) -> RigPose {
    let mut parts = [Mat4::IDENTITY; PARTS];
    parts[PELVIS] = at(a.sway, HIP_Y + a.rise, a.shift) * ry(a.twist) * rz(a.tilt);
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
        // The pelvis rolls and twists over this root-aligned lower-body mount.
        // Starting the IK chain from the already rotated pelvis made a stance
        // foot orbit with it. The overlapping hip meshes still deform visually
        // with the pelvis, while the joint itself can counter-rotate exactly.
        let hips = at(a.sway, HIP_Y + a.rise, a.shift);
        parts[th] = hips * at(side * HIP_X, HIP_DY, 0.0) * ry(leg.yaw) * rz(side * leg.out) * rx(leg.hip);
        parts[sh] = parts[th] * at(0.0, -THIGH, 0.0) * rx(leg.knee);
        // Cancel the complete leg orientation before applying the desired
        // body-facing foot pitch. This includes both out-spread and stepping
        // yaw, in their actual (non-commuting) order.
        let bend = leg.hip + leg.knee;
        let foot_rotation = rx(-bend) * rz(-side * leg.out) * ry(-leg.yaw) * rx(bend + leg.ankle);
        parts[ft] = parts[sh] * at(0.0, -SHIN, 0.0) * foot_rotation;
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
        Motion { phase: 0.0, speed: 0.0, time: 0.0, ..Motion::default() }
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
            let mut highest_flight: f32 = f32::MIN;
            let mut ahead = (f32::MAX, f32::MIN);
            for k in 0..64 {
                let m = Motion { phase: k as f64 * std::f64::consts::TAU / 64.0, speed, ..standing() };
                let rig = pose(&m);
                assert!(rig.is_finite());
                if crate::character::gait_has_stance(m.phase, m.speed) {
                    // A supporting sole is planted through heel roll and
                    // toe-off, without forcing the swing foot down.
                    assert!(rig.sole_low().abs() < 1e-3, "{speed} m/s, phase {k}: soles at {}", rig.sole_low());
                } else {
                    // Toe-off and touchdown are continuous with the ground.
                    // Between them both soles must clear it by a useful amount.
                    assert!(rig.sole_low() > -1e-3, "{speed} m/s, phase {k}: a flight foot penetrates the ground: {}", rig.sole_low());
                    highest_flight = highest_flight.max(rig.sole_low());
                }
                let ankle = point(&rig, FOOT_L, Vec3::ZERO);
                lowest = lowest.min(ankle.y);
                highest = highest.max(ankle.y);
                ahead = (ahead.0.min(ankle.z), ahead.1.max(ankle.z));
            }
            // The foot lifts through the swing and travels fore and aft.
            if speed > 2.0 {
                assert!(highest_flight > 0.03, "{speed} m/s: run has no genuine flight arc ({highest_flight:.3})");
            }
            assert!(highest - lowest > 0.06, "{speed} m/s: foot lift {:.3}", highest - lowest);
            assert!(ahead.1 - ahead.0 > 0.4, "{speed} m/s: foot travel {:.3}", ahead.1 - ahead.0);
        }
        // The arm swings against the leg: left foot forward, left hand back.
        let rig = pose(&Motion { phase: 0.0, speed: 1.6, ..standing() });
        assert!(point(&rig, FOOT_L, Vec3::ZERO).z > 0.1, "left foot ahead at its strike");
        assert!(point(&rig, HAND_L, Vec3::ZERO).z < point(&rig, HAND_R, Vec3::ZERO).z, "left hand behind the right");
    }

    #[test]
    fn stance_sweep_cancels_exact_world_travel() {
        for speed in [1.6, 4.5, 7.0] {
            let duty = crate::character::gait_duty(speed) as f32;
            let stride = crate::character::stride_len(speed) as f32;
            let travel = 2.0 * duty * stride;
            let q0 = 0.08;
            let q1 = 0.12;
            let seconds = (q1 - q0) * 2.0 * stride / speed as f32;
            let a = gait_step(q0, duty, travel, 0.2, 1.0);
            let b = gait_step(q1, duty, travel, 0.2, 1.0);
            let swept_in_body_frame = a.z - b.z;
            let moved_over_ground = speed as f32 * seconds;
            assert!((swept_in_body_frame - moved_over_ground).abs() < 1e-5, "{speed} m/s: stance sweep {swept_in_body_frame} vs body travel {moved_over_ground}");
        }
    }

    #[test]
    fn swing_joins_stance_without_a_velocity_snap() {
        for duty in [0.62, 0.34, 0.24] {
            let travel = 2.0 * duty * 1.2;
            let e = 0.0001;
            // Both ends of the swing have the planted foot's backwards
            // velocity, rather than stopping dead and starting again.
            for touchdown in [false, true] {
                let (a, b, c) = if touchdown {
                    (gait_step(1.0 - e, duty, travel, 0.16, 1.0), gait_step(1.0, duty, travel, 0.16, 1.0), gait_step(e, duty, travel, 0.16, 1.0))
                } else {
                    (gait_step(duty - e, duty, travel, 0.16, 1.0), gait_step(duty, duty, travel, 0.16, 1.0), gait_step(duty + e, duty, travel, 0.16, 1.0))
                };
                let before = (b.z - a.z) / e;
                let after = (c.z - b.z) / e;
                let planted = -travel / duty;
                assert!((before - planted).abs() < 0.03 && (after - planted).abs() < 0.03, "duty {duty}, touchdown {touchdown}: {before} / {after}, expected {planted}");
            }
        }
    }

    #[test]
    fn weight_transfer_does_not_drag_planted_feet() {
        for speed in [1.6, 4.5, 7.0] {
            let duty = crate::character::gait_duty(speed) as f32;
            let stride = crate::character::stride_len(speed) as f32;
            let travel = 2.0 * duty * stride;
            let (q0, q1) = (duty * 0.35, duty * 0.65);
            for (forward, side) in [(speed, 0.0), (-speed, 0.0), (0.0, speed), (0.0, -speed)] {
                let direction = Vec3::new(side as f32 / speed as f32, 0.0, forward as f32 / speed as f32);
                let across = Vec3::new(direction.z, 0.0, -direction.x);
                let frame = |q| {
                    pose(&Motion { phase: q as f64 * std::f64::consts::TAU, speed, forward, side, ..standing() })
                };
                let (a, b) = (frame(q0), frame(q1));
                let a0 = point(&a, FOOT_L, Vec3::ZERO);
                let a1 = point(&b, FOOT_L, Vec3::ZERO);
                let swept = (a0 - a1).dot(direction);
                let wanted = travel * (q1 - q0) / duty;
                assert!((swept - wanted).abs() < 2e-4, "{forward}/{side} m/s: planted sweep {swept}, wanted {wanted}");
                assert!(((a0 - a1).dot(across)).abs() < 2e-4, "{forward}/{side} m/s: the stance foot was dragged sideways");
            }
        }
    }

    #[test]
    fn pelvis_motion_is_compact_and_continuous() {
        for speed in [1.6, 4.5, 7.0] {
            let mut low = f32::MAX;
            let mut high = f32::MIN;
            let mut greatest_step: f32 = 0.0;
            let mut smallest_knee = f32::MAX;
            let mut previous: Option<f32> = None;
            for k in 0..=256 {
                let phase = k as f64 * std::f64::consts::TAU / 256.0;
                let m = Motion { phase, speed, ..standing() };
                let y = point(&pose(&m), PELVIS, Vec3::ZERO).y;
                low = low.min(y);
                high = high.max(y);
                if let Some(old) = previous {
                    greatest_step = greatest_step.max((y - old).abs());
                }
                previous = Some(y);
                let a = locomotion(&m);
                smallest_knee = smallest_knee.min(a.legs[0].knee).min(a.legs[1].knee);
            }
            assert!(high - low < 0.14, "{speed} m/s: pelvis bounces {:.3} m", high - low);
            assert!(greatest_step < 0.02, "{speed} m/s: pelvis jumps {greatest_step:.3} m between frames");
            assert!(smallest_knee > 0.08, "{speed} m/s: knee locks (minimum {smallest_knee:.3} rad)");
        }
    }

    #[test]
    fn directional_ik_keeps_shoes_body_facing() {
        for yaw in [0.0, PI, std::f32::consts::FRAC_PI_2, -std::f32::consts::FRAC_PI_2] {
            let leg = leg_to(-0.58, 0.12, 0.09, yaw);
            let angles = Angles {
                legs: [leg, leg],
                arms: [Arm { swing: 0.0, out: 0.12, elbow: -0.14 }; 2],
                lean: 0.0,
                twist: 0.0,
                tilt: 0.0,
                sway: 0.0,
                head: 0.0,
                rise: 0.0,
                shift: 0.0,
            };
            let rig = build(&angles);
            let shoe_forward = rig.parts[FOOT_L].transform_vector3(Vec3::Z).normalize();
            let expected = Vec3::new(0.0, -0.09f32.sin(), 0.09f32.cos());
            assert!(shoe_forward.distance(expected) < 1e-4, "yaw {yaw}: shoe axis {shoe_forward:?}, expected {expected:?}");
        }
    }

    #[test]
    fn backwards_and_sideways_steps_follow_the_actual_velocity() {
        let backwards = pose(&Motion { phase: 0.0, speed: 1.6, forward: -1.6, ..standing() });
        assert!(point(&backwards, FOOT_L, Vec3::ZERO).z < -0.1, "backwards foot strike stays behind the body");
        assert!(point(&backwards, HAND_L, Vec3::ZERO).z > point(&backwards, HAND_R, Vec3::ZERO).z, "arms counter-swing backwards too");
        for k in 0..32 {
            let rig = pose(&Motion { phase: k as f64 * std::f64::consts::TAU / 32.0, speed: 1.6, side: 1.6, ..standing() });
            assert!(rig.is_finite());
            assert!(point(&rig, FOOT_L, Vec3::ZERO).z.abs() < 0.35, "a sidestep is not drawn as a forward walk");
        }
    }

    #[test]
    fn the_jump_tucks_the_legs() {
        let rig = pose(&Motion { air: 1.0, ..standing() });
        assert!(rig.sole_low() > 0.05, "feet lifted: {}", rig.sole_low());
        assert!(pose(&Motion { air: 0.5, ..standing() }).is_finite());
    }

    #[test]
    fn the_car_bailout_rolls_on_its_body_and_gets_back_up() {
        let frame = |phase, t, roll| Motion { air: 1.0, car: Some(crate::character::CarBailout { phase, t, roll }), ..standing() };
        let axis = |motion: Motion| {
            let rig = pose(&motion);
            point(&rig, HAIR, Vec3::new(0.0, 0.301, -0.01)) - point(&rig, PELVIS, Vec3::ZERO)
        };
        let upright = axis(frame(crate::character::CarBailoutPhase::Roll, 0.0, 0.0));
        let quarter = axis(frame(crate::character::CarBailoutPhase::Roll, 0.2, std::f64::consts::FRAC_PI_2));
        let upside_down = axis(frame(crate::character::CarBailoutPhase::Roll, 0.4, std::f64::consts::PI));
        assert!(upright.y > 0.4 && upside_down.y < -0.2, "head goes over: {upright:?} -> {upside_down:?}");
        assert!(quarter.z > upright.z + 0.4, "and forward with the roll: {quarter:?}");
        let airborne = pose(&frame(crate::character::CarBailoutPhase::Air, 0.5, 0.0));
        let touchdown = pose(&frame(crate::character::CarBailoutPhase::Roll, 0.0, 0.0));
        for (a, b) in airborne.parts.iter().zip(touchdown.parts.iter()) {
            assert!(a.to_cols_array().iter().zip(b.to_cols_array().iter()).all(|(a, b)| (a - b).abs() < 1e-5), "touchdown must continue the airborne pose");
        }
        for k in 0..48 {
            let p = k as f64 / 47.0;
            let rig = pose(&frame(crate::character::CarBailoutPhase::Roll, p * crate::character::CAR_ROLL_TIME, p * std::f64::consts::TAU));
            assert!(rig.is_finite(), "roll frame {k}");
            if p * crate::character::CAR_ROLL_TIME >= 0.18 {
                assert!((rig.mesh_low() - 0.006).abs() < 1e-4, "roll frame {k}: mesh contact at {}", rig.mesh_low());
            }
        }
        for k in 0..24 {
            let rig = pose(&frame(crate::character::CarBailoutPhase::Stand, k as f64 * crate::character::CAR_STAND_TIME / 23.0, 0.0));
            assert!(rig.is_finite(), "recovery frame {k}");
            assert!((rig.mesh_low() - 0.003).abs() < 1e-4, "recovery frame {k}: mesh contact at {}", rig.mesh_low());
        }
        let standing = pose(&frame(crate::character::CarBailoutPhase::Stand, crate::character::CAR_STAND_TIME, 0.0));
        assert!(point(&standing, HAIR, Vec3::new(0.0, 0.301, -0.01)).y > 1.72, "fully upright by the end");
    }

    #[test]
    fn aircraft_exit_develops_into_a_stable_arch() {
        let at_age = |age| {
            pose(&Motion {
                air: 1.0,
                fall: 1.0,
                plane_exit: Some(crate::bailout::Exit::Jump),
                fall_age: age,
                time: 2.0,
                ..standing()
            })
        };
        let early = at_age(0.0);
        let late = at_age(2.0);
        let early_head = point(&early, HAIR, Vec3::new(0.0, 0.301, -0.01));
        let late_head = point(&late, HAIR, Vec3::new(0.0, 0.301, -0.01));
        assert!(late_head.z > early_head.z + 0.25, "the step-out develops belly-to-earth: {early_head:?} -> {late_head:?}");
        assert!(point(&late, HAND_L, Vec3::ZERO).x > point(&early, HAND_L, Vec3::ZERO).x + 0.08, "hands spread for stability");
        // Buffeting is deterministic and moves the limbs, not a static statue.
        let other = pose(&Motion { air: 1.0, fall: 1.0, time: 5.0, ..standing() });
        assert!(point(&late, HAND_L, Vec3::ZERO).distance(point(&other, HAND_L, Vec3::ZERO)) > 0.005);
    }

    #[test]
    fn freefall_lies_forward_and_the_canopy_hangs_upright() {
        let crown = |m: &Motion| point(&pose(m), HAIR, Vec3::new(0.0, 0.301, -0.01));
        let falling = crown(&Motion { air: 1.0, fall: 1.0, ..standing() });
        assert!(falling.z > 0.5, "head out in front: {falling:?}");
        assert!(falling.y < crown(&standing()).y - 0.45, "body down from standing height: {falling:?}");
        let hanging = crown(&Motion { air: 1.0, chute: 1.0, ..standing() });
        assert!(hanging.y > 1.5, "upright under canopy: {hanging:?}");
        assert!(hanging.z.abs() < 0.35, "and not tipped far: {hanging:?}");
        // The half-open blend is finite and between the two.
        let opening = crown(&Motion { air: 1.0, fall: 0.5, chute: 0.5, ..standing() });
        assert!(opening.is_finite());
    }

    #[test]
    fn poses_are_finite_and_instanced() {
        let root = geo::lla_to_pos(0.5, 0.1, 0.0);
        let cam = root + DVec3::new(0.0, 0.0, 8.0);
        for k in 0..24 {
            let m = Motion { phase: k as f64 * 0.31, speed: k as f64 * 0.35, air: (k % 5) as f64 * 0.25, time: k as f64, ..standing() };
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
