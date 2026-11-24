//! The player's avatar: a rigid, multi-part figure posed procedurally.
//!
//! There is no skeletal skinning in GTW v1. Fifteen rigid pieces (pelvis,
//! torso, head, upper and lower arms, hands, thighs, shins and feet) are drawn
//! through the instanced solid path (`inst.wgsl`, selected by `Inst.ext.w = 2`).
//! The `RigPose` is a plain array of part transforms, so a skinned model can
//! replace this later without changing the character controller or the camera.

use crate::geo;
use crate::render::{Inst, MeshVertex};
use glam::{DVec3, Mat4, Vec3};
use std::sync::OnceLock;

/// The rigid pieces, in mesh-id order from [`BASE`].
pub const PARTS: usize = 16;
/// The first mesh id of the avatar in the instanced-solid library.
pub const BASE: u32 = crate::birds::BASE + 30;
/// How many clothing palettes [`palette`] offers.
pub const STYLES: usize = 4;

/// Material codes written into the mesh's vertex colour alpha, as the shader
/// reads them (`inst.wgsl`, the character branch).
pub const MAT_SKIN: f32 = 0.0;
pub const MAT_CLOTH: f32 = 0.25;
pub const MAT_HAIR: f32 = 0.5;
pub const MAT_SHOE: f32 = 0.75;

/// The part names, in the order the meshes and poses use.
#[allow(dead_code)]
pub const NAMES: [&str; PARTS] = [
    "pelvis", "torso", "head", "upper arm left", "lower arm left", "hand left", "upper arm right", "lower arm right", "hand right", "thigh left", "shin left", "foot left", "thigh right", "shin right", "foot right", "hair",
];

/// A clothing palette: linear RGB, as the shader's tints.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Palette {
    pub skin: [f32; 3],
    pub cloth: [f32; 3],
    pub hair: [f32; 3],
    pub shoe: [f32; 3],
}

/// One of four palettes; an out-of-range style falls back to the first.
pub fn palette(style: u8) -> Palette {
    let p = |skin: [f32; 3], cloth: [f32; 3], hair: [f32; 3], shoe: [f32; 3]| Palette { skin, cloth, hair, shoe };
    match style as usize {
        1 => p([0.42, 0.27, 0.18], [0.20, 0.26, 0.10], [0.03, 0.025, 0.02], [0.04, 0.035, 0.03]),
        2 => p([0.62, 0.45, 0.34], [0.16, 0.18, 0.24], [0.55, 0.42, 0.20], [0.08, 0.07, 0.06]),
        3 => p([0.30, 0.19, 0.12], [0.30, 0.12, 0.08], [0.02, 0.02, 0.02], [0.03, 0.03, 0.03]),
        _ => p([0.58, 0.40, 0.30], [0.13, 0.20, 0.26], [0.06, 0.045, 0.03], [0.05, 0.045, 0.04]),
    }
}

/// The colour of a part's material.
fn tint(p: &Palette, part: usize) -> [f32; 3] {
    match part {
        2 => p.skin,
        15 => p.hair,
        11 | 14 => p.shoe,
        _ => p.cloth,
    }
}

/// The gait the avatar is in.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Gait {
    Idle,
    Walk,
    Run,
    Jump,
}

/// A rigid pose: one local transform per part, relative to the body's feet
/// (x right, y up, z forward).
#[derive(Copy, Clone, Debug)]
pub struct RigPose {
    pub parts: [Mat4; PARTS],
}

impl RigPose {
    pub fn is_finite(&self) -> bool {
        self.parts.iter().all(|m| m.to_cols_array().iter().all(|v| v.is_finite()))
    }
}

fn hinge(x: f32, y: f32, z: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, y, z))
}

fn rx(a: f32) -> Mat4 {
    Mat4::from_rotation_x(a)
}

fn rz(a: f32) -> Mat4 {
    Mat4::from_rotation_z(a)
}

/// The pose for a gait at `phase` radians. Idle breathes, walk and run swing
/// the limbs in opposition, jump tucks the legs and lifts the arms.
pub fn pose(gait: Gait, phase: f64) -> RigPose {
    let (swing, arm, knee, lean, bounce) = match gait {
        Gait::Idle => {
            let b = (phase * 0.35).sin() as f32;
            (0.0, 0.0, 0.0, 0.015 * b, 0.006 * b)
        }
        Gait::Walk => (0.50, 0.42, 0.55, 0.03, 0.012),
        Gait::Run => (0.85, 0.75, 1.05, 0.16, 0.03),
        Gait::Jump => (0.0, 0.0, 0.0, 0.0, 0.0),
    };
    let s = phase.sin() as f32;
    let mut parts = [Mat4::IDENTITY; PARTS];
    // Pelvis, torso and head.
    parts[0] = hinge(0.0, 0.94 + bounce, 0.0);
    parts[1] = parts[0] * hinge(0.0, 0.08, 0.0) * rx(lean);
    parts[2] = parts[1] * hinge(0.0, 0.52, 0.0) * rx(-lean * 0.5);
    // Arms: shoulders on the torso, elbows and wrists.
    let (l_arm, r_arm) = (-s * arm, s * arm);
    let (l_knee, r_knee) = (knee * (-s).max(0.0), knee * s.max(0.0));
    if gait == Gait::Jump {
        parts[3] = parts[1] * hinge(-0.22, 0.43, 0.0) * rx(-2.1);
        parts[6] = parts[1] * hinge(0.22, 0.43, 0.0) * rx(-2.1);
        parts[4] = parts[3] * hinge(0.0, -0.28, 0.0) * rx(0.6);
        parts[7] = parts[6] * hinge(0.0, -0.28, 0.0) * rx(0.6);
        parts[9] = parts[0] * hinge(-0.10, -0.02, 0.0) * rx(-0.55);
        parts[12] = parts[0] * hinge(0.10, -0.02, 0.0) * rx(0.35);
        parts[10] = parts[9] * hinge(0.0, -0.44, 0.0) * rx(1.05);
        parts[13] = parts[12] * hinge(0.0, -0.44, 0.0) * rx(0.8);
    } else {
        parts[3] = parts[1] * hinge(-0.22, 0.43, 0.0) * rz(-0.10) * rx(l_arm);
        parts[6] = parts[1] * hinge(0.22, 0.43, 0.0) * rz(0.10) * rx(r_arm);
        parts[4] = parts[3] * hinge(0.0, -0.28, 0.0) * rx(0.10 + 0.35 * s.max(0.0) * arm);
        parts[7] = parts[6] * hinge(0.0, -0.28, 0.0) * rx(0.10 + 0.35 * (-s).max(0.0) * arm);
        parts[9] = parts[0] * hinge(-0.10, -0.02, 0.0) * rx(s * swing);
        parts[12] = parts[0] * hinge(0.10, -0.02, 0.0) * rx(-s * swing);
        parts[10] = parts[9] * hinge(0.0, -0.44, 0.0) * rx(l_knee);
        parts[13] = parts[12] * hinge(0.0, -0.44, 0.0) * rx(r_knee);
    }
    // Hands and feet follow their limbs.
    parts[5] = parts[4] * hinge(0.0, -0.27, 0.0);
    parts[8] = parts[7] * hinge(0.0, -0.27, 0.0);
    parts[11] = parts[10] * hinge(0.0, -0.45, 0.0) * rx(-0.15);
    parts[14] = parts[13] * hinge(0.0, -0.45, 0.0) * rx(-0.15);
    // The hair cap follows the head.
    parts[15] = parts[2];
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
        out.push((BASE + i as u32, Inst { m: rows, tint: [t[0], t[1], t[2], 0.0], ext: [0.0, 0.0, 0.0, 2.0] }));
    }
}

// ---------------------------------------------------------------- the meshes

#[derive(Default)]
struct MB {
    v: Vec<MeshVertex>,
    i: Vec<u32>,
}

impl MB {
    fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, col: [f32; 4]) {
        let n = (b - a).cross(c - a).normalize_or_zero();
        let base = self.v.len() as u32;
        for p in [a, b, c] {
            self.v.push(MeshVertex { pos: p.into(), nrm: n.into(), col });
        }
        self.i.extend([base, base + 1, base + 2]);
    }

    fn quad(&mut self, p: [Vec3; 4], col: [f32; 4]) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        let base = self.v.len() as u32;
        for q in p {
            self.v.push(MeshVertex { pos: q.into(), nrm: n.into(), col });
        }
        self.i.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn cuboid(&mut self, lo: Vec3, hi: Vec3, col: [f32; 4]) {
        let p = |x: bool, y: bool, z: bool| Vec3::new(if x { hi.x } else { lo.x }, if y { hi.y } else { lo.y }, if z { hi.z } else { lo.z });
        self.quad([p(false, false, true), p(true, false, true), p(true, true, true), p(false, true, true)], col);
        self.quad([p(true, false, false), p(false, false, false), p(false, true, false), p(true, true, false)], col);
        self.quad([p(true, false, true), p(true, false, false), p(true, true, false), p(true, true, true)], col);
        self.quad([p(false, false, false), p(false, false, true), p(false, true, true), p(false, true, false)], col);
        self.quad([p(false, true, true), p(true, true, true), p(true, true, false), p(false, true, false)], col);
        self.quad([p(false, false, false), p(true, false, false), p(true, false, true), p(false, false, true)], col);
    }

    /// A tapered tube hanging from its pivot at the origin down -Y.
    fn limb(&mut self, len: f32, r0: f32, r1: f32, segs: usize, col: [f32; 4]) {
        let segs = segs.max(4);
        for k in 0..segs {
            let a0 = k as f32 * std::f32::consts::TAU / segs as f32;
            let a1 = (k + 1) as f32 * std::f32::consts::TAU / segs as f32;
            let ring = |a: f32, y: f32, r: f32| Vec3::new(a.cos() * r, y, a.sin() * r);
            let (p0, p1) = (ring(a0, 0.0, r0), ring(a1, 0.0, r0));
            let (q0, q1) = (ring(a0, -len, r1), ring(a1, -len, r1));
            self.quad([p0, p1, q1, q0], col);
            // Caps, so a limb seen end-on is solid.
            self.tri(Vec3::ZERO, p1, p0, col);
            self.tri(Vec3::new(0.0, -len, 0.0), q0, q1, col);
        }
    }

    /// An ellipsoid: the head.
    fn ball(&mut self, centre: Vec3, radii: Vec3, rings: usize, segs: usize, col: [f32; 4]) {
        let rings = rings.max(3);
        let segs = segs.max(6);
        let pt = |i: usize, j: usize| {
            let phi = i as f32 * std::f32::consts::PI / rings as f32;
            let theta = j as f32 * std::f32::consts::TAU / segs as f32;
            centre + Vec3::new(phi.sin() * theta.cos() * radii.x, phi.cos() * radii.y, phi.sin() * theta.sin() * radii.z)
        };
        for i in 0..rings {
            for j in 0..segs {
                let (a, b, c, d) = (pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1));
                if i == 0 {
                    self.tri(a, b, d, col);
                } else if i == rings - 1 {
                    self.tri(a, b, d, col);
                } else {
                    self.quad([a, b, c, d], col);
                }
            }
        }
    }
}

/// The avatar's meshes, built once.
pub fn meshes() -> &'static Vec<(Vec<MeshVertex>, Vec<u32>)> {
    static M: OnceLock<Vec<(Vec<MeshVertex>, Vec<u32>)>> = OnceLock::new();
    M.get_or_init(|| {
        let skin = [0.95, 0.90, 0.86, MAT_SKIN];
        let cloth = [0.95, 0.95, 0.95, MAT_CLOTH];
        let hair = [0.95, 0.95, 0.95, MAT_HAIR];
        let shoe = [0.95, 0.95, 0.95, MAT_SHOE];
        let mut all = Vec::with_capacity(PARTS);
        // 0 pelvis
        let mut m = MB::default();
        m.cuboid(Vec3::new(-0.17, -0.11, -0.11), Vec3::new(0.17, 0.11, 0.11), cloth);
        all.push((m.v, m.i));
        // 1 torso
        let mut m = MB::default();
        m.cuboid(Vec3::new(-0.16, 0.0, -0.09), Vec3::new(0.16, 0.30, 0.09), cloth);
        m.cuboid(Vec3::new(-0.20, 0.30, -0.10), Vec3::new(0.20, 0.54, 0.10), cloth);
        all.push((m.v, m.i));
        // 2 head
        let mut m = MB::default();
        m.ball(Vec3::new(0.0, 0.13, 0.0), Vec3::new(0.105, 0.13, 0.11), 8, 12, skin);
        all.push((m.v, m.i));
        // 3, 6 upper arms
        for _ in 0..2 {
            let mut m = MB::default();
            m.limb(0.28, 0.055, 0.045, 6, cloth);
            m.ball(Vec3::ZERO, Vec3::splat(0.055), 4, 8, cloth);
            all.push((m.v, m.i));
        }
        // 4, 7 lower arms
        for _ in 0..2 {
            let mut m = MB::default();
            m.limb(0.26, 0.045, 0.035, 6, cloth);
            all.push((m.v, m.i));
        }
        // 5, 8 hands
        for _ in 0..2 {
            let mut m = MB::default();
            m.cuboid(Vec3::new(-0.04, -0.09, -0.03), Vec3::new(0.04, 0.0, 0.03), skin);
            all.push((m.v, m.i));
        }
        // 9, 12 thighs
        for _ in 0..2 {
            let mut m = MB::default();
            m.limb(0.44, 0.085, 0.065, 7, cloth);
            all.push((m.v, m.i));
        }
        // 10, 13 shins
        for _ in 0..2 {
            let mut m = MB::default();
            m.limb(0.45, 0.06, 0.045, 6, cloth);
            all.push((m.v, m.i));
        }
        // 11, 14 feet
        for _ in 0..2 {
            let mut m = MB::default();
            m.cuboid(Vec3::new(-0.055, -0.07, -0.06), Vec3::new(0.055, 0.0, 0.18), shoe);
            all.push((m.v, m.i));
        }
        // 15 hair: a cap over the back and top of the head, a little larger
        // than the skull so it always covers it.
        let mut m = MB::default();
        m.ball(Vec3::new(0.0, 0.155, -0.012), Vec3::new(0.112, 0.115, 0.118), 7, 12, hair);
        all.push((m.v, m.i));
        debug_assert_eq!(all.len(), PARTS);
        all
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_and_meshes_line_up() {
        let meshes = meshes();
        assert_eq!(meshes.len(), PARTS);
        assert_eq!(NAMES.len(), PARTS);
        assert_eq!(BASE, crate::birds::BASE + 30);
        for (i, (v, idx)) in meshes.iter().enumerate() {
            assert!(!v.is_empty(), "{} has vertices", NAMES[i]);
            assert!(!idx.is_empty(), "{} has triangles", NAMES[i]);
            assert!(idx.iter().all(|k| (*k as usize) < v.len()), "{} indices in range", NAMES[i]);
            assert!(v.iter().all(|x| x.pos.iter().all(|f| f.is_finite()) && x.nrm.iter().all(|f| f.is_finite())), "{} finite", NAMES[i]);
        }
    }

    #[test]
    fn palettes_are_in_range() {
        for style in 0..STYLES as u8 {
            let p = palette(style);
            for c in [p.skin, p.cloth, p.hair, p.shoe] {
                assert!(c.iter().all(|v| (0.0..=1.0).contains(v)), "style {style}: {c:?}");
            }
        }
        assert_eq!(palette(0), palette(255));
        assert_ne!(palette(0), palette(1));
    }

    #[test]
    fn poses_are_finite_for_every_gait() {
        let root = geo::lla_to_pos(0.5, 0.1, 0.0);
        let cam = root + DVec3::new(0.0, 0.0, 8.0);
        for gait in [Gait::Idle, Gait::Walk, Gait::Run, Gait::Jump] {
            for k in 0..24 {
                let rig = pose(gait, k as f64 * 0.31);
                assert!(rig.is_finite(), "{gait:?} at {k}");
                let mut out = Vec::new();
                instances(cam, root, 0.7, &rig, 2, &mut out);
                assert_eq!(out.len(), PARTS);
                assert!(out.iter().all(|(id, inst)| (BASE..BASE + PARTS as u32).contains(id) && inst.m.iter().flatten().all(|v| v.is_finite())));
                assert!(out.iter().all(|(_, inst)| inst.ext[3] == 2.0));
            }
        }
    }

    #[test]
    fn a_broken_position_draws_nothing() {
        let mut out = Vec::new();
        instances(DVec3::NAN, DVec3::ZERO, 0.0, &pose(Gait::Idle, 0.0), 0, &mut out);
        assert!(out.is_empty());
    }
}
