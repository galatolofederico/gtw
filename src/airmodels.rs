//! Models for what stands and moves about an airport: five sizes of
//! aircraft (a light single up to a wide-body twin), ground equipment, jet
//! bridges, windsocks and PAPI units. All are instanced solids drawn by
//! inst.wgsl: forward is -Z, up +Y, the origin on the ground. The vertex
//! alpha picks the material: 0 paint (takes the instance tint), 0.5 glass,
//! 1 as coloured.
use crate::render::MeshVertex;
use glam::{Vec2, Vec3};
use std::f32::consts::{PI, TAU};

/// Index of the first airport model among the instanced solids.
pub const BASE: u32 = crate::cars::M_REBAR + crate::cars::N_REBAR;
pub const N_TYPES: usize = 5;
/// Per aircraft type: body, livery (fin and winglets, tinted separately), gear.
pub const M_BODY: u32 = BASE;
// livery and gear follow each body at these offsets.
#[allow(dead_code)]
pub const M_LIVERY: u32 = BASE + 1;
#[allow(dead_code)]
pub const M_GEAR: u32 = BASE + 2;
pub const M_PAPI: u32 = BASE + 3 * N_TYPES as u32;
pub const M_SOCK_POLE: u32 = M_PAPI + 1;
pub const M_SOCK: u32 = M_PAPI + 2;
pub const M_BRIDGE: u32 = M_PAPI + 3;
pub const M_BRIDGE_HEAD: u32 = M_PAPI + 4;
pub const M_MAST: u32 = M_PAPI + 5;
pub const M_TUG: u32 = M_PAPI + 6;
pub const M_CART: u32 = M_PAPI + 7;
pub const M_FUEL: u32 = M_PAPI + 8;
pub const M_LOADER: u32 = M_PAPI + 9;
pub const M_PUSHBACK: u32 = M_PAPI + 10;
pub const M_STAIRS: u32 = M_PAPI + 11;
pub const M_CONE: u32 = M_PAPI + 12;
#[allow(dead_code)] // the table's last slot.
pub const M_SIGN: u32 = M_PAPI + 13;
pub const COUNT: usize = 3 * N_TYPES + 14;

/// What the traffic needs to know about each type of aircraft.
#[derive(Copy, Clone)]
pub struct Type {
    #[allow(dead_code)]
    pub name: &'static str,
    pub len: f32,
    pub span: f32,
    /// fuselage radius
    pub rf: f32,
    /// nose tip and front door ahead of the origin (which sits between the main wheels)
    pub nose: f32,
    pub door: f32,
    /// height of the fuselage centre and of the door sill
    pub cy: f32,
    pub sill: f32,
    /// approach, rotation and taxi speeds (m/s), ground acceleration and braking (m/s²)
    pub v_app: f32,
    pub v_rot: f32,
    pub accel: f32,
    /// wing tip (x, y, z), tail top, for the lights
    pub tip: Vec3,
    pub tail: Vec3,
    /// eye-height of the landing lights, and how far apart
    pub lamp: Vec3,
}

pub const TYPES: [Type; N_TYPES] = [
    Type { name: "light single", len: 8.3, span: 11.0, rf: 0.62, nose: 2.3, door: 0.4, cy: 1.45, sill: 0.9, v_app: 33.0, v_rot: 29.0, accel: 1.9, tip: Vec3::new(5.5, 2.25, 0.3), tail: Vec3::new(0.0, 2.7, 5.7), lamp: Vec3::new(2.6, 2.2, -0.2) },
    Type { name: "business jet", len: 15.2, span: 15.9, rf: 0.86, nose: 6.6, door: 4.2, cy: 1.75, sill: 1.1, v_app: 58.0, v_rot: 56.0, accel: 2.6, tip: Vec3::new(7.95, 1.9, 1.9), tail: Vec3::new(0.0, 4.5, 8.0), lamp: Vec3::new(1.2, 1.2, -1.0) },
    Type { name: "regional jet", len: 32.5, span: 23.2, rf: 1.35, nose: 15.0, door: 12.2, cy: 2.5, sill: 1.6, v_app: 68.0, v_rot: 68.0, accel: 2.3, tip: Vec3::new(11.6, 2.6, 4.2), tail: Vec3::new(0.0, 7.5, 16.6), lamp: Vec3::new(1.9, 1.8, -2.0) },
    Type { name: "narrow-body", len: 37.6, span: 35.8, rf: 1.98, nose: 16.4, door: 12.6, cy: 3.5, sill: 3.4, v_app: 72.0, v_rot: 75.0, accel: 2.2, tip: Vec3::new(17.9, 4.6, 5.6), tail: Vec3::new(0.0, 11.8, 20.4), lamp: Vec3::new(3.2, 2.9, -2.5) },
    Type { name: "wide-body", len: 63.7, span: 60.9, rf: 3.1, nose: 27.5, door: 21.5, cy: 5.3, sill: 4.9, v_app: 76.0, v_rot: 82.0, accel: 1.9, tip: Vec3::new(30.4, 7.4, 11.0), tail: Vec3::new(0.0, 18.5, 35.0), lamp: Vec3::new(5.0, 4.4, -4.0) },
];

type Mesh = (Vec<MeshVertex>, Vec<u32>);

#[derive(Default)]
struct MB {
    v: Vec<MeshVertex>,
    i: Vec<u32>,
}
const PAINT: f32 = 0.0;
const GLASS: f32 = 0.5;
const TRIM: f32 = 1.0;
const WHITE: [f32; 3] = [0.82, 0.82, 0.82];
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
    /// A box between two points with a given cross-section.
    fn beam(&mut self, a: Vec3, b: Vec3, w: f32, h: f32, c: [f32; 3], m: f32) {
        let d = (b - a).normalize_or_zero();
        let side = if d.y.abs() > 0.95 { Vec3::X } else { d.cross(Vec3::Y).normalize() };
        let up = side.cross(d);
        let q = |p: Vec3, s: f32, u: f32| p + side * s * w * 0.5 + up * u * h * 0.5;
        let r = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
        for k in 0..4 {
            let (r0, r1) = (r[k], r[(k + 1) % 4]);
            self.poly(&[q(a, r0.0, r0.1), q(a, r1.0, r1.1), q(b, r1.0, r1.1), q(b, r0.0, r0.1)], c, m);
        }
        self.poly(&[q(a, -1.0, 1.0), q(a, 1.0, 1.0), q(a, 1.0, -1.0), q(a, -1.0, -1.0)], c, m);
        self.poly(&[q(b, -1.0, -1.0), q(b, 1.0, -1.0), q(b, 1.0, 1.0), q(b, -1.0, 1.0)], c, m);
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
    /// A wing-like panel from a root section to a tip section: each is
    /// (leading edge position, chord along +Z, thickness). `up` is the
    /// direction the thickness is measured in.
    fn panel(&mut self, root: (Vec3, f32, f32), tip: (Vec3, f32, f32), up: Vec3, c: [f32; 3], m: f32) {
        // Lens section: leading edge, crest at 30% chord, trailing edge.
        let sect = |s: (Vec3, f32, f32)| -> [Vec3; 6] {
            let (le, ch, t) = s;
            let z = |f: f32| le + Vec3::Z * ch * f;
            [z(0.0), z(0.12) + up * t * 0.38, z(0.38) + up * t * 0.5, z(1.0), z(0.38) - up * t * 0.5, z(0.12) - up * t * 0.38]
        };
        let (a, b) = (sect(root), sect(tip));
        let span = (tip.0 - root.0).normalize_or_zero();
        let flip = span.cross(Vec3::Z).dot(up) < 0.0;
        let base = self.v.len() as u32;
        for (k, ring) in [a, b].iter().enumerate() {
            let _ = k;
            for j in 0..6 {
                let (p, q) = (ring[(j + 5) % 6], ring[(j + 1) % 6]);
                let along = (q - p).normalize_or_zero();
                let mut n = along.cross(span);
                if n.dot(ring[j] - (ring[0] + ring[3]) * 0.5) < 0.0 {
                    n = -n;
                }
                self.vert(ring[j], n, c, m);
            }
        }
        for j in 0..6u32 {
            let (p00, p01, p10, p11) = (base + j, base + (j + 1) % 6, base + 6 + j, base + 6 + (j + 1) % 6);
            if flip { self.i.extend([p00, p01, p10, p01, p11, p10]) } else { self.i.extend([p00, p10, p01, p01, p10, p11]) }
        }
        let mut cap: Vec<Vec3> = b.to_vec();
        if flip {
            cap.reverse();
        }
        self.poly(&cap, c, m);
    }
    fn wheel(&mut self, c: Vec3, r: f32, w: f32) {
        self.tube(c - Vec3::X * w * 0.5, c + Vec3::X * w * 0.5, r, 12, DARK, TRIM);
        self.tube(c - Vec3::X * (w * 0.5 + 0.01), c + Vec3::X * (w * 0.5 + 0.01), r * 0.45, 8, METAL, TRIM);
    }
    fn done(self) -> Mesh {
        (self.v, self.i)
    }
}

struct Spec {
    t: usize,
    /// wing: root leading edge z (behind the nose), root chord, tip chord, sweep of the leading edge (m back at the tip), dihedral (m up at the tip), y at the root
    wing: (f32, f32, f32, f32, f32, f32),
    /// tail plane: half span, root chord, tip chord, sweep back; fin: height, root chord, tip chord, sweep back
    stab: (f32, f32, f32, f32),
    fin: (f32, f32, f32, f32),
    t_tail: bool,
    /// engines under the wing (x, radius, length) or on the rear fuselage
    under: Option<(f32, f32, f32)>,
    rear: Option<(f32, f32)>,
    winglet: f32,
    /// nose wheel ahead of the origin; main gear half track; wheel radius
    gear: (f32, f32, f32),
    high_wing: bool,
}

fn aircraft(s: &Spec) -> [Mesh; 3] {
    let ty = TYPES[s.t];
    let (r, cy, len) = (ty.rf, ty.cy, ty.len);
    let (z0, z1) = (-ty.nose, len - ty.nose);
    let mut body = MB::default();
    let mut liv = MB::default();
    let mut gear = MB::default();
    // Fuselage: an ogive nose (drooped a little), a constant barrel, and a
    // tail cone whose underside sweeps up.
    let nose_l = r * 3.4;
    let tail_l = if s.t == 0 { len * 0.55 } else { len * 0.3 };
    let mut rings = Vec::new();
    for k in 0..=7 {
        let f = k as f32 / 7.0;
        let rr = r * (1.0 - (1.0 - f).powf(2.1)).max(0.0).powf(0.62);
        rings.push((z0 + nose_l * f, 0.0, cy - r * 0.22 * (1.0 - f).powi(2), rr.max(0.02), rr.max(0.02)));
    }
    let zt = z1 - tail_l;
    rings.push(((z0 + nose_l + zt) * 0.5, 0.0, cy, r, r));
    rings.push((zt, 0.0, cy, r, r));
    for k in 1..=6 {
        let f = k as f32 / 6.0;
        let rr = r * (1.0 - f * f * 0.86);
        rings.push((zt + tail_l * f, 0.0, cy + (r - rr) * 0.75, rr * (1.0 - 0.25 * f), rr));
    }
    body.loft(&rings, 20, WHITE, PAINT, true);
    // Flight deck glass, and the cabin windows down each side.
    if s.t > 0 {
        let zc = z0 + nose_l * 0.42;
        let rr = r * 0.80;
        for sx in [-1.0f32, 1.0] {
            for (a0, a1, dz) in [(0.05f32, 0.42f32, 0.0f32), (0.46, 0.80, r * 0.25)] {
                let p = |a: f32, z: f32, up: f32| Vec3::new(sx * (a * 1.25).sin() * rr * 1.04, cy + r * 0.02 + up + (a * 1.25).cos() * rr * 0.36, z);
                let h = r * 0.34;
                let mut q = vec![p(a0, zc + dz, 0.0), p(a1, zc + dz + r * 0.2, 0.0), p(a1, zc + dz + r * 0.42, h), p(a0, zc + dz * 0.6 + r * 0.26, h)];
                if sx < 0.0 {
                    q.reverse();
                }
                body.poly(&q, DARK, GLASS);
            }
        }
        let (w0, w1) = (z0 + nose_l * 1.25, zt + tail_l * 0.1);
        let step = if s.t >= 3 { 0.9 } else { 1.1 };
        let (wh, ww) = (r * 0.17, 0.26f32.min(r * 0.2));
        let n = ((w1 - w0) / step) as usize;
        for k in 0..n {
            let z = w0 + k as f32 * step;
            // Doors break the row.
            if (z - (z0 + ty.nose - ty.door)).abs() < 1.2 {
                continue;
            }
            for sx in [-1.0f32, 1.0] {
                let a = 0.22f32;
                let x = sx * r * a.cos() * 1.012;
                let y = cy + r * a.sin();
                let mut q = vec![Vec3::new(x, y - wh, z - ww), Vec3::new(x, y - wh, z + ww), Vec3::new(x * 0.992, y + wh, z + ww), Vec3::new(x * 0.992, y + wh, z - ww)];
                if sx > 0.0 {
                    q.reverse();
                }
                body.poly(&q, DARK, GLASS);
            }
        }
    } else {
        // A light single's cabin: windscreen and side windows.
        let (zc, top) = (z0 + 1.55, cy + r * 0.95);
        body.poly(&[Vec3::new(-r * 0.78, cy + r * 0.25, zc), Vec3::new(r * 0.78, cy + r * 0.25, zc), Vec3::new(r * 0.7, top, zc + 0.75), Vec3::new(-r * 0.7, top, zc + 0.75)], DARK, GLASS);
        for sx in [-1.0f32, 1.0] {
            let mut q = vec![Vec3::new(sx * r * 0.99, cy + r * 0.2, zc + 0.3), Vec3::new(sx * r * 0.99, cy + r * 0.2, zc + 2.4), Vec3::new(sx * r * 0.86, top - 0.05, zc + 2.2), Vec3::new(sx * r * 0.86, top - 0.05, zc + 0.85)];
            if sx < 0.0 {
                q.reverse();
            }
            body.poly(&q, DARK, GLASS);
        }
        // Propeller and spinner.
        body.loft(&[(z0 - 0.32, 0.0, cy - 0.12, 0.02, 0.02), (z0 - 0.1, 0.0, cy - 0.12, 0.16, 0.16), (z0 + 0.06, 0.0, cy - 0.12, 0.2, 0.2)], 10, METAL, TRIM, false);
        body.beam(Vec3::new(-0.95, cy - 0.12, z0 - 0.08), Vec3::new(0.95, cy - 0.12, z0 - 0.08), 0.03, 0.14, DARK, TRIM);
    }
    // Wings, tail plane, fin.
    let (wz, cr, ct, sweep, dihedral, wy) = s.wing;
    let half = ty.span * 0.5;
    let tk = if s.t == 0 { 0.16 } else { 0.125 };
    for sx in [-1.0f32, 1.0] {
        let root = (Vec3::new(sx * r * 0.55, cy + wy, z0 + wz), cr, cr * tk);
        let tip = (Vec3::new(sx * half, cy + wy + dihedral, z0 + wz + sweep), ct, ct * tk * 0.8);
        body.panel(root, tip, Vec3::Y, [0.7, 0.71, 0.72], PAINT);
        if s.winglet > 0.0 {
            let w0 = (tip.0, ct, ct * tk * 0.7);
            let w1 = (tip.0 + Vec3::new(sx * s.winglet * 0.25, s.winglet, ct * 0.55), ct * 0.35, ct * 0.04);
            liv.panel(w0, w1, Vec3::X * sx, WHITE, PAINT);
        }
        if s.high_wing {
            body.beam(Vec3::new(sx * r * 0.9, cy - r * 0.7, z0 + wz + cr * 0.45), Vec3::new(sx * half * 0.5, cy + wy - 0.05, z0 + wz + cr * 0.4), 0.05, 0.12, [0.7, 0.71, 0.72], PAINT);
        }
        let (sh, scr, sct, ssw) = s.stab;
        let sy = if s.t_tail { cy + r * 0.3 + s.fin.0 } else { cy + r * 0.35 };
        let sz = if s.t_tail { z1 - s.fin.1 * 0.2 + s.fin.3 * 0.9 - scr * 0.4 } else { z1 - scr * 1.25 };
        body.panel((Vec3::new(sx * if s.t_tail { 0.05 } else { r * 0.3 }, sy, sz), scr, scr * 0.1), (Vec3::new(sx * sh, sy + if s.t_tail { 0.0 } else { sh * 0.1 }, sz + ssw), sct, sct * 0.08), Vec3::Y, [0.7, 0.71, 0.72], PAINT);
        if let Some((ex, er, el)) = s.under {
            // Nacelle slung ahead of and below the wing on its pylon.
            let f = (ex - r * 0.55) / (half - r * 0.55);
            let le = Vec3::new(sx * ex, cy + wy + dihedral * f, z0 + wz + sweep * f);
            let c = Vec3::new(sx * ex, le.y - er * 1.15, le.z - el * 0.45);
            nacelle(&mut body, c, er, el);
            body.beam(le + Vec3::new(0.0, -0.05, cr * 0.1), c + Vec3::new(0.0, er * 0.8, el * 0.5), er * 0.18, er * 0.7, WHITE, PAINT);
        }
        if let Some((er, el)) = s.rear {
            let c = Vec3::new(sx * (r + er * 1.25), cy + r * 0.45, z1 - tail_l * 0.95);
            nacelle(&mut body, c, er, el);
            body.beam(Vec3::new(sx * r * 0.8, c.y, c.z + el * 0.5), c + Vec3::Z * el * 0.5, er * 0.5, er * 0.3, WHITE, PAINT);
        }
    }
    let (fh, fcr, fct, fsw) = s.fin;
    let fz = z1 - fcr * 1.12;
    liv.panel((Vec3::new(0.0, cy + r * 0.55, fz), fcr, fcr * 0.11), (Vec3::new(0.0, cy + r * 0.55 + fh, fz + fsw), fct, fct * 0.09), Vec3::X, WHITE, PAINT);
    // Undercarriage.
    let (gn, gt, gr) = s.gear;
    let belly = cy - r;
    let twin = s.t >= 2;
    let leg = |g: &mut MB, x: f32, z: f32, top: Vec3, bogie: bool| {
        g.tube(Vec3::new(x, gr, z), top, gr * 0.22, 8, METAL, TRIM);
        let w = gr * 0.55;
        let zs: &[f32] = if bogie { &[-gr * 1.15, gr * 1.15] } else { &[0.0] };
        for dz in zs {
            if twin {
                g.wheel(Vec3::new(x - w * 0.75, gr, z + dz), gr, w);
                g.wheel(Vec3::new(x + w * 0.75, gr, z + dz), gr, w);
            } else {
                g.wheel(Vec3::new(x, gr, z + dz), gr, w);
            }
        }
    };
    leg(&mut gear, 0.0, -gn, Vec3::new(0.0, belly + 0.1, -gn), false);
    for sx in [-1.0f32, 1.0] {
        // A high-winged single's legs splay out from the cabin floor.
        let top = if s.high_wing { Vec3::new(sx * r * 0.7, belly + 0.2, 0.0) } else { Vec3::new(sx * gt, cy + wy, 0.0) };
        leg(&mut gear, sx * gt, 0.0, top, s.t == 4);
    }
    [body.done(), liv.done(), gear.done()]
}

fn nacelle(m: &mut MB, c: Vec3, r: f32, l: f32) {
    let z = |f: f32| c.z + l * f;
    m.loft(&[(z(0.0), c.x, c.y, r * 0.9, r * 0.9), (z(0.12), c.x, c.y, r, r), (z(0.55), c.x, c.y, r, r), (z(0.86), c.x, c.y, r * 0.72, r * 0.72), (z(1.0), c.x, c.y, r * 0.5, r * 0.5)], 14, WHITE, PAINT, false);
    // The dark of the intake with the fan's hub, and the tail pipe.
    let disc = |m: &mut MB, zz: f32, rr: f32, col: [f32; 3], back: bool| {
        let mut ring: Vec<Vec3> = (0..14).map(|k| { let a = k as f32 / 14.0 * TAU; Vec3::new(c.x + a.cos() * rr, c.y + a.sin() * rr, zz) }).collect();
        if back {
            ring.reverse();
        }
        m.poly(&ring, col, TRIM);
    };
    disc(m, z(0.04), r * 0.86, DARK, true);
    disc(m, z(0.03), r * 0.3, METAL, true);
    disc(m, z(0.995), r * 0.48, DARK, false);
    m.loft(&[(z(0.0), c.x, c.y, r * 0.9, r * 0.9), (z(0.02), c.x, c.y, r * 0.93, r * 0.93)], 14, METAL, TRIM, false);
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

fn vehicles() -> Vec<Mesh> {
    let tyre = |m: &mut MB, x: f32, z: f32, r: f32| m.wheel(Vec3::new(x, r, z), r, 0.22);
    let yellow = [0.85, 0.62, 0.04];
    // Baggage tug: low, open cab, takes the tint.
    let mut tug = MB::default();
    tug.cuboid(Vec3::new(-0.65, 0.25, -1.3), Vec3::new(0.65, 0.85, 1.2), WHITE, PAINT);
    tug.cuboid(Vec3::new(-0.6, 0.85, 0.1), Vec3::new(0.6, 1.0, 1.1), DARK, TRIM);
    for x in [-0.6f32, 0.55] {
        tug.cuboid(Vec3::new(x, 0.85, -0.5), Vec3::new(x + 0.05, 1.75, -0.45), DARK, TRIM);
    }
    tug.cuboid(Vec3::new(-0.65, 1.75, -0.55), Vec3::new(0.65, 1.8, 0.9), WHITE, PAINT);
    for (x, z) in [(-0.62, -0.85), (0.62, -0.85), (-0.62, 0.8), (0.62, 0.8)] {
        tyre(&mut tug, x, z, 0.28);
    }
    // Baggage cart with its load under a cover.
    let mut cart = MB::default();
    cart.cuboid(Vec3::new(-0.75, 0.35, -1.4), Vec3::new(0.75, 0.45, 1.4), METAL, TRIM);
    cart.cuboid(Vec3::new(-0.7, 0.45, -1.3), Vec3::new(0.7, 1.35, 1.3), [0.12, 0.2, 0.42], TRIM);
    cart.cuboid(Vec3::new(-0.72, 1.35, -1.32), Vec3::new(0.72, 1.42, 1.32), [0.6, 0.6, 0.6], TRIM);
    cart.beam(Vec3::new(0.0, 0.4, -1.4), Vec3::new(0.0, 0.4, -2.0), 0.06, 0.06, METAL, TRIM);
    for (x, z) in [(-0.7, -1.0), (0.7, -1.0), (-0.7, 1.0), (0.7, 1.0)] {
        tyre(&mut cart, x, z, 0.2);
    }
    // Fuel bowser: cab and a tank.
    let mut fuel = MB::default();
    fuel.cuboid(Vec3::new(-1.2, 0.5, -4.6), Vec3::new(1.2, 2.6, -2.9), WHITE, PAINT);
    fuel.cuboid(Vec3::new(-1.1, 1.6, -4.63), Vec3::new(1.1, 2.4, -4.6), DARK, GLASS);
    fuel.cuboid(Vec3::new(-1.15, 0.5, -2.9), Vec3::new(1.15, 0.9, 4.4), [0.2, 0.2, 0.22], TRIM);
    fuel.loft(&[(-2.7, 0.0, 2.0, 0.9, 0.9), (-2.5, 0.0, 2.0, 1.2, 1.1), (4.1, 0.0, 2.0, 1.2, 1.1), (4.3, 0.0, 2.0, 0.9, 0.9)], 14, [0.82, 0.82, 0.8], TRIM, true);
    for (x, z) in [(-1.1, -3.7), (1.1, -3.7), (-1.1, 2.4), (1.1, 2.4), (-1.1, 3.6), (1.1, 3.6)] {
        tyre(&mut fuel, x, z, 0.5);
    }
    // Belt loader: chassis and a sloping belt.
    let mut loader = MB::default();
    loader.cuboid(Vec3::new(-0.9, 0.3, -1.6), Vec3::new(0.9, 0.9, 2.4), WHITE, PAINT);
    loader.beam(Vec3::new(0.25, 0.95, 3.0), Vec3::new(0.25, 3.0, -4.2), 0.9, 0.22, [0.1, 0.1, 0.1], TRIM);
    loader.cuboid(Vec3::new(-0.85, 0.9, 1.0), Vec3::new(-0.2, 1.7, 1.9), DARK, TRIM);
    for (x, z) in [(-0.85, -1.0), (0.85, -1.0), (-0.85, 1.8), (0.85, 1.8)] {
        tyre(&mut loader, x, z, 0.32);
    }
    // Pushback tractor: a heavy slab with a small cab.
    let mut push = MB::default();
    push.cuboid(Vec3::new(-1.5, 0.35, -3.4), Vec3::new(1.5, 1.2, 3.4), yellow, TRIM);
    push.cuboid(Vec3::new(-1.2, 1.2, 1.2), Vec3::new(1.2, 2.1, 3.0), yellow, TRIM);
    push.cuboid(Vec3::new(-1.22, 1.45, 1.3), Vec3::new(1.22, 2.0, 2.9), DARK, GLASS);
    push.beam(Vec3::new(0.0, 0.45, -3.4), Vec3::new(0.0, 0.45, -6.4), 0.1, 0.1, METAL, TRIM);
    for (x, z) in [(-1.45, -2.2), (1.45, -2.2), (-1.45, 2.2), (1.45, 2.2)] {
        tyre(&mut push, x, z, 0.6);
    }
    // Passenger stairs on a truck.
    let mut stairs = MB::default();
    stairs.cuboid(Vec3::new(-1.0, 0.4, -1.0), Vec3::new(1.0, 1.1, 3.2), WHITE, PAINT);
    stairs.beam(Vec3::new(0.0, 1.1, 3.0), Vec3::new(0.0, 3.5, -2.6), 1.5, 0.35, [0.75, 0.75, 0.75], TRIM);
    stairs.cuboid(Vec3::new(-0.8, 3.4, -3.6), Vec3::new(0.8, 3.55, -2.4), [0.75, 0.75, 0.75], TRIM);
    for (x, z) in [(-0.95, -0.4), (0.95, -0.4), (-0.95, 2.6), (0.95, 2.6)] {
        tyre(&mut stairs, x, z, 0.38);
    }
    // Traffic cone.
    let mut cone = MB::default();
    cone.loft(&[(0.0, 0.0, 0.0, 0.16, 0.16), (0.5, 0.0, 0.0, 0.03, 0.03)], 8, [0.95, 0.3, 0.03], TRIM, true);
    for v in cone.v.iter_mut() {
        // (lofted along z: stand it up)
        v.pos = [v.pos[0], v.pos[2], v.pos[1]];
        v.nrm = [v.nrm[0], v.nrm[2], v.nrm[1]];
    }
    for t in cone.i.chunks_mut(3) {
        t.swap(1, 2);
    }
    // Taxiway sign: a low lit box (the instance tint colours its face).
    let mut sign = MB::default();
    sign.cuboid(Vec3::new(-0.9, 0.25, -0.12), Vec3::new(0.9, 0.95, 0.12), WHITE, PAINT);
    for x in [-0.7f32, 0.7] {
        sign.cuboid(Vec3::new(x - 0.04, 0.0, -0.04), Vec3::new(x + 0.04, 0.25, 0.04), METAL, TRIM);
    }
    vec![tug.done(), cart.done(), fuel.done(), loader.done(), push.done(), stairs.done(), cone.done(), sign.done()]
}

/// Every airport model, in the order of the `M_*` constants.
pub fn build() -> Vec<Mesh> {
    let specs = [
        Spec { t: 0, wing: (1.75, 1.6, 1.15, 0.0, 0.12, 0.72), stab: (1.7, 1.1, 0.7, 0.25), fin: (1.25, 1.3, 0.6, 0.8), t_tail: false, under: None, rear: None, winglet: 0.0, gear: (1.45, 1.25, 0.2), high_wing: true },
        Spec { t: 1, wing: (5.9, 2.7, 0.9, 2.4, 0.5, -0.55), stab: (3.0, 1.6, 0.7, 1.3), fin: (2.4, 2.6, 1.3, 2.2), t_tail: true, under: None, rear: Some((0.48, 2.4)), winglet: 0.9, gear: (5.4, 1.4, 0.26), high_wing: false },
        Spec { t: 2, wing: (12.6, 4.6, 1.3, 6.2, 0.9, -0.95), stab: (4.3, 2.6, 1.0, 2.4), fin: (4.0, 4.6, 2.3, 4.2), t_tail: true, under: None, rear: Some((0.7, 3.6)), winglet: 1.3, gear: (12.6, 2.1, 0.38), high_wing: false },
        Spec { t: 3, wing: (12.6, 6.1, 1.5, 8.6, 1.9, -1.35), stab: (6.2, 3.6, 1.3, 3.6), fin: (6.3, 5.6, 1.9, 6.0), t_tail: false, under: Some((5.75, 1.05, 4.2)), rear: None, winglet: 2.2, gear: (12.6, 3.8, 0.58), high_wing: false },
        Spec { t: 4, wing: (21.0, 11.5, 2.4, 17.0, 3.6, -2.1), stab: (10.8, 6.2, 2.0, 6.5), fin: (9.6, 9.2, 3.0, 9.6), t_tail: false, under: Some((9.6, 1.75, 7.2)), rear: None, winglet: 0.0, gear: (25.0, 5.5, 0.7), high_wing: false },
    ];
    let mut out = Vec::with_capacity(COUNT);
    for s in &specs {
        out.extend(aircraft(s));
    }
    out.push(papi());
    out.extend(sock());
    out.extend(bridge());
    out.push(mast());
    out.extend(vehicles());
    debug_assert_eq!(out.len(), COUNT);
    let _ = (PI, Vec2::ZERO);
    out
}
