//! Models for the airfield's fixtures: PAPI units, windsocks, jet bridges and
//! floodlight masts. All are instanced solids drawn by inst.wgsl: forward is
//! -Z, up +Y, the origin on the ground. The vertex alpha picks the material:
//! 0 paint (takes the instance tint), 0.5 glass, 1 as coloured.
use crate::render::MeshVertex;
use glam::Vec3;
use std::f32::consts::TAU;

/// Index of the first airport fixture among the instanced solids.
pub const BASE: u32 = crate::cars::M_REBAR + crate::cars::N_REBAR;
pub const M_PAPI: u32 = BASE;
pub const M_SOCK_POLE: u32 = M_PAPI + 1;
pub const M_SOCK: u32 = M_PAPI + 2;
pub const M_BRIDGE: u32 = M_PAPI + 3;
pub const M_BRIDGE_HEAD: u32 = M_PAPI + 4;
pub const M_MAST: u32 = M_PAPI + 5;
pub const COUNT: usize = 6;

/// The size of the aircraft a stand of each class is laid out for: fuselage
/// length and wingspan, radius, and the nose tip, front door and sill ahead
/// of the origin (which sits between the main wheels).
#[derive(Copy, Clone)]
pub struct StandDims {
    pub len: f32,
    pub span: f32,
    /// fuselage radius
    pub rf: f32,
    pub nose: f32,
    pub door: f32,
    pub sill: f32,
}

pub const STAND_DIMS: [StandDims; 5] = [
    StandDims { len: 8.3, span: 11.0, rf: 0.62, nose: 2.3, door: 0.4, sill: 0.9 },
    StandDims { len: 15.2, span: 15.9, rf: 0.86, nose: 6.6, door: 4.2, sill: 1.1 },
    StandDims { len: 32.5, span: 23.2, rf: 1.35, nose: 15.0, door: 12.2, sill: 1.6 },
    StandDims { len: 37.6, span: 35.8, rf: 1.98, nose: 16.4, door: 12.6, sill: 3.4 },
    StandDims { len: 63.7, span: 60.9, rf: 3.1, nose: 27.5, door: 21.5, sill: 4.9 },
];

type Mesh = (Vec<MeshVertex>, Vec<u32>);

#[derive(Default)]
struct MB {
    v: Vec<MeshVertex>,
    i: Vec<u32>,
}
const GLASS: f32 = 0.5;
const TRIM: f32 = 1.0;
const DARK: [f32; 3] = [0.03, 0.03, 0.035];
const METAL: [f32; 3] = [0.42, 0.44, 0.46];

impl MB {
    fn vert(&mut self, p: Vec3, n: Vec3, c: [f32; 3], m: f32) -> u32 {
        self.v.push(MeshVertex { pos: p.into(), nrm: n.normalize_or_zero().into(), col: [c[0], c[1], c[2], m] });
        self.v.len() as u32 - 1
    }
    /// Flat polygon (fan), normal from its winding.
    fn poly(&mut self, p: &[Vec3], c: [f32; 3], m: f32) {
        let mut n = Vec3::ZERO;
        for k in 1..p.len() - 1 {
            n += (p[k] - p[0]).cross(p[k + 1] - p[0]);
        }
        let b = self.v.len() as u32;
        for q in p {
            self.vert(*q, n, c, m);
        }
        for k in 1..p.len() as u32 - 1 {
            self.i.extend([b, b + k, b + k + 1]);
        }
    }
    fn cuboid(&mut self, lo: Vec3, hi: Vec3, c: [f32; 3], m: f32) {
        let p = |x: bool, y: bool, z: bool| Vec3::new(if x { hi.x } else { lo.x }, if y { hi.y } else { lo.y }, if z { hi.z } else { lo.z });
        self.poly(&[p(false, false, true), p(true, false, true), p(true, true, true), p(false, true, true)], c, m);
        self.poly(&[p(true, false, false), p(false, false, false), p(false, true, false), p(true, true, false)], c, m);
        self.poly(&[p(true, false, true), p(true, false, false), p(true, true, false), p(true, true, true)], c, m);
        self.poly(&[p(false, false, false), p(false, false, true), p(false, true, true), p(false, true, false)], c, m);
        self.poly(&[p(false, true, true), p(true, true, true), p(true, true, false), p(false, true, false)], c, m);
        self.poly(&[p(false, false, false), p(true, false, false), p(true, false, true), p(false, false, true)], c, m);
    }
    /// Rings of a body of revolution about an axis parallel to Z: each ring is
    /// (z, centre x, centre y, radius x, radius y). Smooth-shaded.
    fn loft(&mut self, rings: &[(f32, f32, f32, f32, f32)], sides: usize, c: [f32; 3], m: f32, caps: bool) {
        let b = self.v.len() as u32;
        for (k, r) in rings.iter().enumerate() {
            let (p, q) = (rings[k.saturating_sub(1)], rings[(k + 1).min(rings.len() - 1)]);
            // Slope of the profile, for the normals.
            let dz = (q.0 - p.0).max(1e-4);
            let (sx, sy) = ((q.3 - p.3) / dz, (q.4 - p.4) / dz);
            for s in 0..=sides {
                let a = s as f32 / sides as f32 * TAU;
                let (sn, cs) = a.sin_cos();
                let n = Vec3::new(cs * r.4.max(1e-3), sn * r.3.max(1e-3), -(sx * cs * cs + sy * sn * sn) * r.3.max(r.4).max(1e-3));
                self.vert(Vec3::new(r.1 + cs * r.3, r.2 + sn * r.4, r.0), n, c, m);
            }
        }
        let w = sides as u32 + 1;
        for k in 0..rings.len() as u32 - 1 {
            for s in 0..sides as u32 {
                let (p00, p01, p10, p11) = (b + k * w + s, b + k * w + s + 1, b + (k + 1) * w + s, b + (k + 1) * w + s + 1);
                self.i.extend([p00, p10, p01, p01, p10, p11]);
            }
        }
        if caps {
            for (r, flip) in [(rings[0], true), (rings[rings.len() - 1], false)] {
                let mut ring: Vec<Vec3> = (0..sides).map(|s| { let a = s as f32 / sides as f32 * TAU; Vec3::new(r.1 + a.cos() * r.3, r.2 + a.sin() * r.4, r.0) }).collect();
                if !flip {
                    ring.reverse();
                }
                self.poly(&ring, c, m);
            }
        }
    }
    /// A cylinder along any axis.
    fn tube(&mut self, a: Vec3, b: Vec3, r: f32, sides: usize, c: [f32; 3], m: f32) {
        let d = (b - a).normalize_or_zero();
        let s = if d.y.abs() > 0.95 { Vec3::X } else { d.cross(Vec3::Y).normalize() };
        let u = s.cross(d);
        let base = self.v.len() as u32;
        for (k, p) in [a, b].into_iter().enumerate() {
            for j in 0..=sides {
                let ang = j as f32 / sides as f32 * TAU;
                let n = s * ang.cos() + u * ang.sin();
                self.vert(p + n * r, n, c, m);
            }
            let ring: Vec<Vec3> = (0..sides).map(|j| { let ang = j as f32 / sides as f32 * TAU * if k == 0 { 1.0 } else { -1.0 }; p + (s * ang.cos() + u * ang.sin()) * r }).collect();
            self.poly(&ring, c, m);
        }
        // (the cap polygons sit between the two rings in the vertex list)
        let w = sides as u32 + 1;
        let second = base + w + sides as u32;
        for j in 0..sides as u32 {
            self.i.extend([base + j, second + j, base + j + 1, base + j + 1, second + j, second + j + 1]);
        }
    }
    fn done(self) -> Mesh {
        (self.v, self.i)
    }
}

fn papi() -> Mesh {
    let mut m = MB::default();
    for x in [-0.25f32, 0.25] {
        m.tube(Vec3::new(x, 0.0, 0.0), Vec3::new(x, 0.45, 0.0), 0.03, 6, METAL, TRIM);
    }
    m.cuboid(Vec3::new(-0.4, 0.45, -0.45), Vec3::new(0.4, 0.75, 0.45), [0.75, 0.3, 0.05], TRIM);
    m.cuboid(Vec3::new(-0.34, 0.5, -0.47), Vec3::new(0.34, 0.7, -0.45), DARK, GLASS);
    m.done()
}

/// The pole of a windsock (6 m), and the sock itself: a cone along -Z from
/// the origin, striped orange and white, turned and drooped by the wind.
fn sock() -> [Mesh; 2] {
    let mut p = MB::default();
    p.tube(Vec3::ZERO, Vec3::new(0.0, 6.0, 0.0), 0.06, 8, [0.8, 0.8, 0.8], TRIM);
    p.tube(Vec3::new(0.0, 5.55, 0.0), Vec3::new(0.0, 6.45, 0.0), 0.02, 6, METAL, TRIM);
    let mut s = MB::default();
    let n = 6;
    for k in 0..n {
        let (f0, f1) = (k as f32 / n as f32, (k + 1) as f32 / n as f32);
        let col = if k % 2 == 0 { [0.95, 0.32, 0.04] } else { [0.9, 0.9, 0.88] };
        s.loft(&[(-3.6 * f0, 0.0, 0.0, 0.45 - 0.22 * f0, 0.45 - 0.22 * f0), (-3.6 * f1, 0.0, 0.0, 0.45 - 0.22 * f1, 0.45 - 0.22 * f1)], 10, col, TRIM, false);
    }
    [p.done(), s.done()]
}

/// A jet bridge: the tunnel (one metre long along -Z, to be stretched to the
/// length needed) and the head that meets the aircraft door, both with the
/// floor at y = 0 (raised to the door sill when placed).
fn bridge() -> [Mesh; 2] {
    let wall = [0.72, 0.74, 0.76];
    let mut t = MB::default();
    t.cuboid(Vec3::new(-1.5, -0.25, -1.0), Vec3::new(1.5, 2.55, 0.0), wall, TRIM);
    for x in [-1.52f32, 1.5] {
        t.cuboid(Vec3::new(x, 0.9, -0.96), Vec3::new(x + 0.02, 1.9, -0.04), DARK, GLASS);
    }
    let mut h = MB::default();
    // Rotunda and cab, the wheeled drive column under it.
    h.tube(Vec3::new(0.0, -0.3, 0.0), Vec3::new(0.0, 2.7, 0.0), 2.2, 12, wall, TRIM);
    h.cuboid(Vec3::new(-2.3, -0.25, -2.6), Vec3::new(1.6, 2.5, -1.4), wall, TRIM);
    h.cuboid(Vec3::new(-2.32, 0.9, -2.5), Vec3::new(-2.3, 1.9, -1.5), DARK, GLASS);
    h.cuboid(Vec3::new(-0.35, -6.0, 0.6), Vec3::new(0.35, -0.3, 1.3), [0.5, 0.52, 0.55], TRIM);
    h.cuboid(Vec3::new(-1.6, -6.0, 0.55), Vec3::new(1.6, -5.2, 1.35), [0.85, 0.65, 0.05], TRIM);
    [t.done(), h.done()]
}

/// A floodlight mast, 22 m.
fn mast() -> Mesh {
    let mut m = MB::default();
    m.tube(Vec3::ZERO, Vec3::new(0.0, 22.0, 0.0), 0.22, 8, [0.6, 0.62, 0.64], TRIM);
    m.cuboid(Vec3::new(-1.6, 21.6, -0.25), Vec3::new(1.6, 22.5, 0.25), [0.3, 0.3, 0.32], TRIM);
    m.done()
}

/// Every airport fixture, in the order of the `M_*` constants.
pub fn build() -> Vec<Mesh> {
    let mut out = Vec::with_capacity(COUNT);
    out.push(papi());
    out.extend(sock());
    out.extend(bridge());
    out.push(mast());
    debug_assert_eq!(out.len(), COUNT);
    out
}
