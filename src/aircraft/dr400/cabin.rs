//! The cockpit under the bubble, laid out as in a Régent: the footwell
//! running forward under the panel to the rudder pedals at the firewall;
//! the anthracite panel under a padded black glareshield (flight
//! instruments on the left, the radio stack in the middle, engine gauges,
//! clock and breakers to the right); the centre console under it with the
//! throttle and mixture levers, the fuel selector and the parking brake;
//! the trim wheel and the handbrake-style flap lever on the tunnel between
//! the front seats; a stick each; contoured seats in velour and leatherette
//! with their belts, the rear bench and the baggage space behind it; vinyl
//! side panels with armrests, map pockets and air vents; and the canopy, a
//! fixed windscreen and the bubble that slides forward over it on its frame.
use crate::aircraft::Hotspot;
use super::*;
use crate::aircraft::c172::cabin::{gauge, rect, strap};
use glam::{Mat3, Quat, Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

pub const PANEL_Z: f32 = -1.08;
pub const FLOOR_Y: f32 = -0.52;
/// The canopy's apex, and where the windscreen meets the sliding bubble.
pub const CANOPY_TOP: f32 = 0.60;
pub const SCREEN_Z: f32 = -0.80;
/// The footwell's front wall, just aft of the firewall.
const TOE_Z: f32 = FIREWALL_Z + 0.05;

const PANEL_COL: Mat = mt(mat::PLASTIC, 0.030, 0.031, 0.034);
const TRIM: Mat = mt(mat::PLASTIC, 0.30, 0.295, 0.28);
/// Dark leatherette: seat bolsters, armrests, the sill pads.
const VINYL: Mat = mt(mat::PLASTIC, 0.038, 0.039, 0.043);
/// Blue-grey velour seat inserts.
const CLOTH: Mat = mt(mat::FABRIC, 0.060, 0.075, 0.110);
const CARPET: Mat = mt(mat::FABRIC, 0.048, 0.050, 0.056);
const STRAP: Mat = mt(mat::FABRIC, 0.040, 0.045, 0.055);
const KNOB: Mat = mt(mat::PLASTIC, 0.02, 0.02, 0.022);
const RED: Mat = mt(mat::PLASTIC, 0.38, 0.025, 0.02);
const FRAME: Mat = mt(mat::PAINT, 0.80, 0.80, 0.78);
const FACE: Mat = mt(mat::GAUGE, 0.02, 0.02, 0.02);

/// Instruments: id, x, y, radius, on the panel.
const GAUGES: [(f32, f32, f32, f32); 16] = [
    (gauge::ASI, -0.44, 0.035, 0.040),
    (gauge::AI, -0.33, 0.035, 0.040),
    (gauge::ALT, -0.22, 0.035, 0.040),
    (gauge::TC, -0.44, -0.075, 0.040),
    (gauge::HI, -0.33, -0.075, 0.040),
    (gauge::VSI, -0.22, -0.075, 0.040),
    (gauge::TACH, -0.10, 0.035, 0.040),
    (gauge::CDI, -0.10, -0.075, 0.040),
    (gauge::FUEL_L, 0.10, 0.045, 0.024),
    (gauge::FUEL_R, 0.16, 0.045, 0.024),
    (gauge::OIL_T, 0.10, -0.015, 0.024),
    (gauge::OIL_P, 0.16, -0.015, 0.024),
    (gauge::AMPS, 0.22, -0.015, 0.024),
    (gauge::SUCTION, 0.22, 0.045, 0.024),
    (gauge::CLOCK, 0.455, 0.045, 0.028),
    (gauge::EGT, 0.455, -0.035, 0.024),
];
/// The radio stack's opening: x0, x1, y0, y1.
const STACK: [f32; 4] = [0.27, 0.40, -0.17, 0.08];

pub fn build(b: &mut Builder) {
    let mut m = Mesh::default();
    lining(&mut m);
    panel(&mut m, b);
    console(&mut m, b);
    seats(&mut m);
    extras(&mut m);
    b.fixed("cabin", G_FUSE, m);
    controls(b);
    canopy(b);
}

// ---------------------------------------------------------------- helpers

/// A bar whose section `prof` (a closed outline, counter-clockwise in
/// (a, b)) is swept along `path`, with a along `ax[k]` and b along `bx[k]`
/// at each point; flat-shaded faces.
fn sweep(m: &mut Mesh, mat: &Mat, path: &[Vec3], ax: &[Vec3], bx: &[Vec3], prof: &[Vec2]) {
    let np = prof.len();
    for e in 0..np {
        let (p0, p1) = (prof[e], prof[(e + 1) % np]);
        let d = p1 - p0;
        let n2 = Vec2::new(d.y, -d.x).normalize();
        let base = m.v.len() as u32;
        let mut s = 0.0;
        for k in 0..path.len() {
            if k > 0 {
                s += (path[k] - path[k - 1]).length();
            }
            let n = ax[k] * n2.x + bx[k] * n2.y;
            for (j, p) in [p0, p1].iter().enumerate() {
                m.push(path[k] + ax[k] * p.x + bx[k] * p.y, n, [j as f32 * d.length(), s], mat, 0.0);
            }
        }
        for k in 0..path.len() as u32 - 1 {
            let a = base + k * 2;
            m.i.extend([a, a + 2, a + 1, a + 1, a + 2, a + 3]);
        }
    }
}

/// A rectangle (a from a0 to a1, b from 0 to depth), counter-clockwise.
fn rect_prof(a0: f32, a1: f32, depth: f32) -> [Vec2; 4] {
    [Vec2::new(a0, 0.0), Vec2::new(a1, 0.0), Vec2::new(a1, depth), Vec2::new(a0, depth)]
}

/// A small printed label.
fn label(m: &mut Mesh, c: Vec3, hx: f32, hy: f32, rot: Quat) {
    m.rbox(&mt(mat::PLASTIC, 0.58, 0.58, 0.56).pat(pat::PLACARD), c, Vec3::new(hx, hy, 0.0004), 0.0002, rot);
}

// ---------------------------------------------------------------- shell

/// The sill's point (right side) at station z: where the skin stops under the canopy.
pub fn sill(z: f32) -> Vec2 {
    sec_pt(&sec(z), 0.62)
}
/// The canopy's height along the centre line at station z.
pub fn canopy_top(z: f32) -> f32 {
    let (z0, z1) = (CANOPY[0] - 0.02, CANOPY[1] + 0.02);
    let base = sec(z).top;
    let up = smoothstep(z0, -0.40, z).powf(0.75) * (1.0 - smoothstep(0.25, z1, z).powf(1.4));
    base + (CANOPY_TOP - base) * up
}
/// A point on the canopy at station z, u from the right sill (0) over the top (1) to the left sill (2).
pub fn canopy_pt(z: f32, u: f32) -> Vec3 {
    let s = sill(z);
    let top = canopy_top(z);
    let th = (u - 1.0) * FRAC_PI_2;
    let (sn, cs) = th.sin_cos();
    let e = |v: f32, n: f32| v.signum() * v.abs().powf(2.0 / n);
    // A bubble: a little fuller than the sill line at shoulder height.
    let bulge = 1.0 + 0.04 * (u * PI).sin();
    Vec3::new(-s.x * bulge * e(sn, 2.4), s.y + (top - s.y) * e(cs, 2.4), z)
}

/// Half width inside the walls at station z, height y (below the sill).
fn inner_x(z: f32, y: f32) -> f32 {
    let s = sec(z);
    let (mut a, mut c) = (0.62f32, PI);
    for _ in 0..30 {
        let m = (a + c) * 0.5;
        if sec_pt(&s, m).y > y { a = m } else { c = m }
    }
    sec_pt(&s, (a + c) * 0.5).x - 0.02
}

/// The floor: flat under the seats, rising into a heel board toward the firewall.
fn floor_y(z: f32) -> f32 {
    FLOOR_Y + 0.12 * smoothstep(-1.20, TOE_Z, z)
}

fn lining(m: &mut Mesh) {
    // The side walls, from the sill down to the floor.
    let (z0, z1) = (TOE_Z, CANOPY[1] + 0.05);
    let nz = 70;
    let mut side = Mesh::default();
    let rows: Vec<Vec<SP>> = (0..=nz)
        .map(|i| {
            let z = z0 + (z1 - z0) * i as f32 / nz as f32;
            let top = sill(z).y - 0.01;
            let bot = floor_y(z) - 0.01;
            (0..=14)
                .map(|k| {
                    let y = top + (bot - top) * k as f32 / 14.0;
                    (Vec3::new(inner_x(z, y), y, z), [z, y], 0.0)
                })
                .collect()
        })
        .collect();
    side.grid(&TRIM.pat(pat::LINING), &rows, false, true);
    // The sill's top: a padded rail from the wall out to the skin's edge.
    let rail: Vec<Vec<SP>> = (0..=nz)
        .map(|i| {
            let z = z0 + (z1 - z0) * i as f32 / nz as f32;
            let s = sill(z);
            let (a, c) = (inner_x(z, s.y - 0.01), s.x);
            (0..=4)
                .map(|k| {
                    let t = k as f32 / 4.0;
                    let y = s.y - 0.01 + 0.012 * (t * PI).sin();
                    (Vec3::new(a + (c - a) * t, y, z), [z, t], 0.0)
                })
                .collect()
        })
        .collect();
    side.grid(&VINYL, &rail, false, true);
    // A beading strip along the wall under the sill.
    let bead: Vec<Vec3> = (0..=40).map(|k| { let z = PANEL_Z + 0.02 + (z1 - 0.1 - PANEL_Z) * k as f32 / 40.0; Vec3::new(inner_x(z, -0.02) - 0.002, -0.02, z) }).collect();
    side.tube(&mt(mat::PLASTIC, 0.12, 0.12, 0.12), &bead, &|_| 0.0045, 8, true);
    // Armrests for the front and rear seats, a map pocket under the front one.
    for (za, zb) in [(-0.80f32, -0.14f32), (0.22, 0.86)] {
        let (zc, ix) = ((za + zb) * 0.5, inner_x((za + zb) * 0.5, -0.12));
        side.rbox(&VINYL, Vec3::new(ix - 0.026, -0.115, zc), Vec3::new(0.034, 0.024, (zb - za) * 0.5), 0.018, Quat::IDENTITY);
        for z in [za + 0.04, zb - 0.04] {
            side.screw(&SCREW_BLACK, Vec3::new(ix - 0.026, -0.115 - 0.024, z), Vec3::NEG_Y, 0.003, z);
        }
    }
    let ix = inner_x(-0.60, -0.30);
    side.rbox(&VINYL, Vec3::new(ix - 0.012, -0.32, -0.60), Vec3::new(0.012, 0.075, 0.17), 0.008, Quat::from_rotation_z(-0.05));
    side.tube(&mt(mat::FABRIC, 0.03, 0.03, 0.033), &[Vec3::new(ix - 0.02, -0.245, -0.77), Vec3::new(ix - 0.026, -0.243, -0.60), Vec3::new(ix - 0.02, -0.245, -0.43)], &|_| 0.006, 8, true);
    // Fresh-air vents just aft of the panel: a chrome ring and a black eyeball.
    let (vz, vy) = (PANEL_Z + 0.07, -0.02);
    let vx = inner_x(vz, vy);
    side.lathe(&CHROME, Vec3::new(vx, vy, vz), Vec3::NEG_X, &[[0.030, 0.0], [0.030, 0.004], [0.024, 0.009], [0.021, 0.009]], 24, false);
    side.lathe(&BLACK, Vec3::new(vx - 0.004, vy, vz), Vec3::new(-1.0, 0.1, 0.15).normalize(), &[[0.021, 0.0], [0.020, 0.008], [0.015, 0.014], [0.009, 0.016], [0.009, 0.03], [0.0, 0.03]], 18, true);
    let l = side.mirror_x();
    m.append(&side);
    m.append(&l);
    // The floor on the centre section, carpeted, with rubber heel mats.
    m.patch(&CARPET.pat(pat::CARPET), 40, 6, false, |a, c| {
        let z = z0 + (z1 - z0) * a;
        let y = floor_y(z);
        let w = inner_x(z, y);
        (Vec3::new((c * 2.0 - 1.0) * w, y, z), [z, c], 0.0)
    });
    for xs in [-0.27f32, 0.27] {
        m.rbox(&RUBBER, Vec3::new(xs, FLOOR_Y + 0.004, -1.03), Vec3::new(0.14, 0.004, 0.15), 0.003, Quat::IDENTITY);
    }
    // The bulkhead behind the baggage space, and the firewall's lining at the front.
    for (z, n, mt_) in [(z1, Vec3::NEG_Z, TRIM.pat(pat::LINING)), (z0, Vec3::Z, mt(mat::SATIN, 0.08, 0.08, 0.085))] {
        let s = sec(z);
        let ring: Vec<Vec3> = (0..=32).map(|k| { let t = 0.62 + (PI - 0.62) * k as f32 / 32.0; let p = sec_pt(&s, t); Vec3::new(p.x - 0.02, p.y, z) }).collect();
        let mut ring2 = ring.clone();
        ring2.extend(ring.iter().rev().map(|p| Vec3::new(-p.x, p.y, p.z)));
        m.fan(&mt_, Vec3::new(0.0, (s.mid + FLOOR_Y) * 0.5, z), &ring2, n);
    }
    // The baggage floor behind the rear seat, with a net on the bulkhead.
    let (bz0, by) = (0.80f32, -0.26f32);
    m.patch(&CARPET.pat(pat::CARPET), 6, 6, false, |a, c| {
        let z = bz0 + (z1 - 0.005 - bz0) * a;
        let w = inner_x(z, by);
        (Vec3::new((c * 2.0 - 1.0) * w, by, z), [z, c], 0.0)
    });
    let nz_ = z1 - 0.012;
    for k in 0..4 {
        let y = by + 0.06 + k as f32 * 0.07;
        let w = inner_x(z1, y) - 0.04;
        strap(m, &STRAP.pat(pat::WEBBING), Vec3::new(-w, y, nz_), Vec3::new(w, y, nz_), Vec3::NEG_Z, 0.02, 0.003);
    }
    for k in 0..5 {
        let x = -0.32 + k as f32 * 0.16;
        strap(m, &STRAP.pat(pat::WEBBING), Vec3::new(x, by + 0.02, nz_ - 0.002), Vec3::new(x, by + 0.28, nz_ - 0.002), Vec3::NEG_Z, 0.02, 0.003);
    }
}

// ---------------------------------------------------------------- panel

/// The panel's top edge at x.
fn panel_top(x: f32) -> f32 {
    let x = x.abs();
    if x < 0.25 { 0.13 - 0.005 * (x / 0.25).powi(2) } else { 0.125 - 0.025 * ((x - 0.25) / 0.25) }
}

fn panel(m: &mut Mesh, b: &mut Builder) {
    let z = PANEL_Z;
    let w = 0.50;
    let outline: Vec<Vec2> = [(-w, -0.25), (w, -0.25), (w, 0.10), (0.25, 0.125), (0.0, 0.13), (-0.25, 0.125), (-w, 0.10)].iter().map(|&(x, y)| Vec2::new(x, y)).collect();
    let mut holes: Vec<Vec<Vec2>> = GAUGES.iter().map(|g| (0..32).map(|k| { let a = -(k as f32) / 32.0 * TAU; Vec2::new(g.1, g.2) + Vec2::new(a.cos(), a.sin()) * (g.3 + 0.001) }).collect()).collect();
    holes.push(vec![Vec2::new(STACK[0], STACK[2]), Vec2::new(STACK[0], STACK[3]), Vec2::new(STACK[1], STACK[3]), Vec2::new(STACK[1], STACK[2])]);
    m.slab(&PANEL_COL.pat(pat::PANEL), &outline, &holes, Vec3::new(0.0, 0.0, z), Vec3::X, Vec3::Y, 0.012);
    let f = z + 0.0005;
    // The seams round the removable flight and engine sub-panels, and their screws.
    let seam = mt(mat::BLACK, 0.008, 0.008, 0.009);
    for &(x0, x1, y0, y1) in &[(-0.495f32, -0.045f32, -0.128f32, 0.088f32), (0.065, 0.255, -0.05, 0.08)] {
        for (a, c) in [(Vec3::new(x0, y0, f), Vec3::new(x1, y0, f)), (Vec3::new(x0, y1, f), Vec3::new(x1, y1, f)), (Vec3::new(x0, y0, f), Vec3::new(x0, y1, f)), (Vec3::new(x1, y0, f), Vec3::new(x1, y1, f))] {
            strap(m, &seam, a, c, Vec3::Z, 0.0016, 0.0004);
        }
        for (x, y) in [(x0 + 0.008, y0 + 0.008), (x1 - 0.008, y0 + 0.008), (x0 + 0.008, y1 - 0.008), (x1 - 0.008, y1 - 0.008)] {
            m.screw(&SCREW_BLACK, Vec3::new(x, y, f), Vec3::Z, 0.0026, x * 13.0 + y);
        }
    }
    for (x, y) in [(-0.49f32, -0.24f32), (0.49, -0.24), (-0.49, 0.092), (0.49, 0.092), (0.0, -0.24)] {
        m.screw(&SCREW_BLACK, Vec3::new(x, y, f), Vec3::Z, 0.0028, x * 7.0 + y);
    }

    // The glareshield: a padded brow over the panel's top edge, then the
    // hood running forward to the windscreen's foot.
    let gs = mt(mat::PLASTIC, 0.022, 0.022, 0.024);
    let brow: Vec<Vec3> = (0..=24).map(|k| { let x = -w + 2.0 * w * k as f32 / 24.0; Vec3::new(x, panel_top(x) + 0.006, z + 0.016) }).collect();
    m.tube(&gs, &brow, &|_| 0.017, 12, true);
    m.patch(&mt(mat::FABRIC, 0.026, 0.026, 0.028), 24, 10, false, |a, c| {
        let x = (a - 0.5) * 2.0 * w;
        let zz = z + 0.016 - c * (z + 0.016 - CANOPY[0] + 0.05);
        let top = sill(zz).y.max(sec(zz).top - 0.05 * (1.0 - smoothstep(CANOPY[0] - 0.02, CANOPY[0] + 0.1, zz)));
        let y0 = panel_top(x) + 0.022;
        let y = y0 + (top - y0) * (1.0 - (1.0 - c) * (1.0 - c));
        let xc = x.signum() * x.abs().min(sill(zz).x - 0.02);
        (Vec3::new(xc, y, zz), [x, c], 0.0)
    });

    // Instruments: the recessed face, its bezel and four screws.
    for (k, &(id, x, y, r)) in GAUGES.iter().enumerate() {
        let c = Vec3::new(x, y, z - 0.010);
        m.disc(&FACE.x(id), c, Vec3::Z, Vec3::Y, r, 40);
        m.lathe(&BLACK, c, Vec3::Z, &[[r, 0.0], [r, 0.0095], [r + 0.0035, 0.0125], [r + 0.0065, 0.0112], [r + 0.0068, 0.0098]], 40, false);
        let q = r * 1.06;
        for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            m.screw(&SCREW_BLACK, Vec3::new(x + dx * q, y + dy * q, z + 0.0005), Vec3::Z, 0.0025, k as f32 * 4.0 + dx + dy * 2.0);
        }
    }
    // The radio stack: audio panel, nav-com, transponder, GPS.
    let units: [(f32, f32, f32); 4] = [(0.078, 0.050, -1.0), (0.047, -0.010, 0.0), (-0.013, -0.065, 2.0), (-0.068, -0.168, 3.0)];
    for (k, &(y1, y0, disp)) in units.iter().enumerate() {
        let (cx, cy) = ((STACK[0] + STACK[1]) * 0.5, (y0 + y1) * 0.5);
        let (hx, hy) = ((STACK[1] - STACK[0]) * 0.5 - 0.001, (y1 - y0) * 0.5 - 0.0012);
        m.rbox(&mt(mat::PLASTIC, 0.045, 0.046, 0.05), Vec3::new(cx, cy, z - 0.003), Vec3::new(hx, hy, 0.008), 0.0025, Quat::IDENTITY);
        let fz = z + 0.0051;
        if disp >= 0.0 {
            let (dw, dh) = if disp == 3.0 { (0.044, 0.030) } else { (0.040, 0.0085) };
            let dx = cx - 0.012;
            m.rbox(&BLACK, Vec3::new(dx, cy + if disp == 3.0 { 0.006 } else { 0.0 }, fz - 0.0005), Vec3::new(dw + 0.003, dh + 0.003, 0.001), 0.001, Quat::IDENTITY);
            rect(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(disp), Vec3::new(dx, cy + if disp == 3.0 { 0.006 } else { 0.0 }, fz + 0.0007), Vec3::X, Vec3::Y, dw, dh);
            // Concentric knobs on the right, a volume knob on the left.
            let kn = Vec3::new(STACK[1] - 0.012, cy - hy * 0.25, fz);
            m.lathe(&KNOB.pat(pat::KNURL), kn, Vec3::Z, &[[0.0095, 0.0], [0.0095, 0.008], [0.0085, 0.009], [0.0, 0.009]], 20, false);
            m.lathe(&KNOB.pat(pat::KNURL), kn + Vec3::Z * 0.009, Vec3::Z, &[[0.0058, 0.0], [0.0058, 0.008], [0.005, 0.009], [0.0, 0.009]], 16, false);
            m.lathe(&KNOB, Vec3::new(STACK[0] + 0.009, cy + hy * 0.4, fz), Vec3::Z, &[[0.005, 0.0], [0.005, 0.006], [0.0, 0.007]], 14, false);
        }
        // Push buttons along the bottom (the audio panel: one row of them, with their lamps).
        let nb = if disp == -1.0 { 7 } else if disp == 3.0 { 5 } else { 3 };
        for i in 0..nb {
            let (bx, by) = if disp == -1.0 { (STACK[0] + 0.012 + i as f32 * 0.0165, cy - 0.002) } else { (cx - 0.045 + i as f32 * 0.018, y0 + 0.0075) };
            m.rbox(&mt(mat::PLASTIC, 0.09, 0.09, 0.095).pat(pat::PLACARD), Vec3::new(bx, by, fz + 0.002), Vec3::new(0.0055, 0.0035, 0.0025), 0.0012, Quat::IDENTITY);
            if disp == -1.0 && i % 3 == 0 {
                m.disc(&mt(mat::LAMP, 0.2, 0.9, 0.3).x(lamp::ANNUNC), Vec3::new(bx, by + 0.0068, fz + 0.0005), Vec3::Z, Vec3::Y, 0.0012, 8);
            }
        }
        for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, 1.0)] {
            m.screw(&SCREW_BLACK, Vec3::new(cx + dx * (hx - 0.004), cy + dy * (hy - 0.003), fz), Vec3::Z, 0.0016, k as f32 * 3.0 + dx);
        }
    }
    // The magneto key switch, its key in at BOTH.
    m.lathe(&CHROME, Vec3::new(-0.44, -0.17, f), Vec3::Z, &[[0.016, 0.0], [0.016, 0.004], [0.012, 0.007], [0.0, 0.007]], 24, false);
    let mut key = Mesh::default();
    key.rbox(&mt(mat::STEEL, 0.6, 0.55, 0.4), Vec3::new(-0.44, -0.17, f + 0.014), Vec3::new(0.003, 0.009, 0.008), 0.0015, Quat::from_rotation_z(-0.5));
    key.rbox(&BLACK, Vec3::new(-0.44, -0.17, f + 0.027), Vec3::new(0.004, 0.012, 0.009), 0.003, Quat::from_rotation_z(-0.5));
    let center = Vec3::new(-0.44, -0.17, f);
    let piece = b.add("magneto key", G_FUSE, key, Anim::hinge(Drive::new(Input::Ignition, -0.8)), center, Vec3::Z, None);
    b.pieces[piece].hotspot = Some(Hotspot::click(Vec3::Z * 0.027, 0.016, "ignition"));
    m.disc(&mt(mat::PLASTIC, 0.32, 0.32, 0.31).pat(pat::PLACARD), Vec3::new(-0.44, -0.17, f - 0.0002), Vec3::Z, Vec3::Y, 0.026, 32);
    // The warning lamps: alternator, fuel pressure, starter engaged.
    for (i, col) in [mt(mat::PLASTIC, 0.25, 0.015, 0.01), mt(mat::PLASTIC, 0.30, 0.13, 0.01), mt(mat::PLASTIC, 0.25, 0.015, 0.01)].iter().enumerate() {
        let x = -0.37 + i as f32 * 0.03;
        m.lathe(&CHROME, Vec3::new(x, -0.142, f), Vec3::Z, &[[0.0075, 0.0], [0.0075, 0.003], [0.006, 0.005]], 16, false);
        m.dome(col, Vec3::new(x, -0.142, f + 0.004), Vec3::Z, 0.0058, 0.004, 14, 0.0);
        label(m, Vec3::new(x, -0.157, f), 0.011, 0.003, Quat::IDENTITY);
    }
    // The rocker switches: master (battery and alternator, red), fuel pump,
    // taxi and landing lights, navigation lights and beacon.
    for i in 0..7 {
        let x = -0.37 + i as f32 * 0.03;
        let col = if i < 2 { RED } else { mt(mat::PLASTIC, 0.03, 0.03, 0.03) };
        m.rbox(&BLACK, Vec3::new(x, -0.19, f + 0.002), Vec3::new(0.008, 0.013, 0.002), 0.0015, Quat::IDENTITY);
        let center = Vec3::new(x, -0.19, f + 0.006);
        let mut rocker = Mesh::default();
        rocker.rbox(&col, center, Vec3::new(0.0062, 0.011, 0.004), 0.002, Quat::IDENTITY);
        match i {
            0 | 4 => {
                let (input, action) = if i == 0 { (Input::Master, "master") } else { (Input::LandingLights, "landing_lights") };
                let piece = b.add("rocker", G_FUSE, rocker, Anim::hinge(Drive::new(input, -0.36).offset(0.18)), center, Vec3::X, None);
                b.pieces[piece].hotspot = Some(Hotspot::click(Vec3::ZERO, 0.011, action));
            }
            _ => m.append(&rocker),
        }
        label(m, Vec3::new(x, -0.214, f), 0.011, 0.0035, Quat::IDENTITY);
    }
    label(m, Vec3::new(-0.295, -0.232, f), 0.08, 0.004, Quat::IDENTITY);
    // Pulls under the engine gauges: carburettor heat, cabin heat, and the primer.
    for (x, col, l) in [(0.0f32, KNOB, true), (0.05, KNOB, true)] {
        m.tube(&CHROME, &[Vec3::new(x, -0.17, z - 0.02), Vec3::new(x, -0.17, z + 0.012)], &|_| 0.0042, 10, false);
        m.lathe(&col.pat(pat::KNURL), Vec3::new(x, -0.17, z + 0.012), Vec3::Z, &[[0.0042, 0.0], [0.013, 0.004], [0.014, 0.016], [0.012, 0.019], [0.0, 0.02]], 18, true);
        if l {
            label(m, Vec3::new(x, -0.198, f), 0.014, 0.004, Quat::IDENTITY);
        }
    }
    m.lathe(&CHROME, Vec3::new(0.10, -0.17, f), Vec3::Z, &[[0.010, 0.0], [0.010, 0.004], [0.006, 0.006], [0.0025, 0.006], [0.0025, 0.016], [0.0, 0.016]], 16, false);
    m.lathe(&CHROME.pat(pat::KNURL), Vec3::new(0.10, -0.17, f + 0.016), Vec3::Z, &[[0.0025, 0.0], [0.007, 0.002], [0.007, 0.012], [0.0, 0.014]], 14, true);
    label(m, Vec3::new(0.10, -0.198, f), 0.014, 0.004, Quat::IDENTITY);
    // The circuit breakers along the bottom right, labelled.
    for i in 0..10 {
        let p = Vec3::new(0.20 + i as f32 * 0.027, -0.218, f);
        m.lathe(&BLACK, p, Vec3::Z, &[[0.0065, 0.0], [0.0065, 0.002], [0.0048, 0.003], [0.0048, 0.009], [0.004, 0.0105], [0.0, 0.0105]], 12, false);
        label(m, p + Vec3::Y * 0.0125, 0.0105, 0.0028, Quat::IDENTITY);
    }
    // The hour meter, and the ELT's remote switch under its red guard.
    m.rbox(&BLACK, Vec3::new(0.455, -0.10, f + 0.002), Vec3::new(0.026, 0.01, 0.003), 0.002, Quat::IDENTITY);
    rect(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(5.0), Vec3::new(0.455, -0.10, f + 0.0055), Vec3::X, Vec3::Y, 0.021, 0.0065);
    let e = Vec3::new(0.455, -0.155, f);
    m.lathe(&CHROME, e, Vec3::Z, &[[0.006, 0.0], [0.006, 0.004], [0.003, 0.006]], 12, false);
    m.tube(&CHROME, &[e + Vec3::Z * 0.004, e + Vec3::new(0.0, 0.004, 0.018)], &|t| 0.0022 - 0.0008 * t, 8, true);
    m.rbox(&RED, e + Vec3::new(0.0, 0.0, 0.010), Vec3::new(0.011, 0.013, 0.010), 0.004, Quat::IDENTITY);
    label(m, e - Vec3::Y * 0.022, 0.016, 0.004, Quat::IDENTITY);
    // Placards: the limitations, and the type's plate.
    let plate = mt(mat::PLASTIC, 0.34, 0.34, 0.33).pat(pat::PLACARD);
    m.rbox(&plate, Vec3::new(-0.10, -0.142, f), Vec3::new(0.028, 0.008, 0.0005), 0.0003, Quat::IDENTITY);
    m.rbox(&plate, Vec3::new(0.16, -0.092, f), Vec3::new(0.05, 0.011, 0.0005), 0.0003, Quat::IDENTITY);

    // The compass on top of the glareshield, on its little pedestal.
    let c = Vec3::new(0.0, 0.178, z - 0.11);
    m.rbox(&BLACK, c - Vec3::Y * 0.035, Vec3::new(0.022, 0.012, 0.03), 0.006, Quat::IDENTITY);
    m.rbox(&BLACK, c, Vec3::new(0.036, 0.03, 0.032), 0.012, Quat::IDENTITY);
    rect(m, &FACE.x(gauge::COMPASS), c + Vec3::Z * 0.0322, Vec3::X, Vec3::Y, 0.026, 0.017);
    m.screw(&SCREW_BLACK, c + Vec3::new(0.026, -0.022, 0.0322), Vec3::Z, 0.0022, 1.0);
    m.screw(&SCREW_BLACK, c + Vec3::new(-0.026, -0.022, 0.0322), Vec3::Z, 0.0022, 2.0);
    label(m, c + Vec3::new(0.0, -0.034, 0.033), 0.02, 0.006, Quat::IDENTITY);
}

// ---------------------------------------------------------------- console

/// The centre console's sloping face: from the panel's foot down to the floor.
const CON_TOP: Vec2 = Vec2::new(PANEL_Z + 0.005, -0.25);
const CON_FOOT: Vec2 = Vec2::new(-0.86, FLOOR_Y);
const CON_HW: f32 = 0.075;
/// A point on the console's face, x across, s from its top (0) to the floor (1); the face's frame.
fn con_face(x: f32, s: f32) -> (Vec3, Quat) {
    let p = CON_TOP + (CON_FOOT - CON_TOP) * s;
    let d = (CON_FOOT - CON_TOP).normalize();
    let up = Vec3::new(0.0, -d.y, -d.x);
    let n = Vec3::X.cross(up);
    (Vec3::new(x, p.y, p.x) + n * 0.0005, Quat::from_mat3(&Mat3::from_cols(Vec3::X, up, n)))
}

fn console(m: &mut Mesh, b: &mut Builder) {
    let col = PANEL_COL.pat(pat::PANEL);
    // The console: a wedge from behind the panel down to the floor.
    let outline: Vec<Vec2> = [(PANEL_Z - 0.25, -0.25), (CON_TOP.x, CON_TOP.y), (CON_FOOT.x, CON_FOOT.y), (PANEL_Z - 0.25, FLOOR_Y)].iter().map(|&(z, y)| Vec2::new(z, y)).collect();
    m.slab(&col, &outline, &[], Vec3::new(-CON_HW, 0.0, 0.0), Vec3::Z, Vec3::Y, CON_HW * 2.0);
    // The tunnel running aft between the seats, carpeted.
    m.rbox(&CARPET.pat(pat::CARPET), Vec3::new(0.0, FLOOR_Y + 0.06, -0.52), Vec3::new(0.07, 0.06, 0.34), 0.02, Quat::IDENTITY);

    // The throttle quadrant: two slots, the mixture lever (red) on the right.
    let (q, rot) = con_face(0.0, 0.20);
    m.rbox(&BLACK, q, Vec3::new(0.058, 0.055, 0.002), 0.004, rot);
    for x in [-0.025f32, 0.025] {
        m.rbox(&mt(mat::BLACK, 0.004, 0.004, 0.004), q + rot * Vec3::new(x, 0.0, 0.0012), Vec3::new(0.0045, 0.042, 0.001), 0.001, rot);
    }
    label(m, q + rot * Vec3::new(-0.048, 0.03, 0.0022), 0.006, 0.012, rot);
    label(m, q + rot * Vec3::new(-0.048, -0.03, 0.0022), 0.006, 0.012, rot);
    let n = rot * Vec3::Z;
    let mx = q + rot * Vec3::new(0.025, 0.02, 0.0);
    let mut mixture = Mesh::default();
    mixture.tube(&CHROME, &[mx - n * 0.02, mx + n * 0.075], &|_| 0.0042, 10, false);
    mixture.lathe(&RED.pat(pat::KNURL), mx + n * 0.075, n, &[[0.0042, 0.0], [0.012, 0.004], [0.013, 0.016], [0.011, 0.02], [0.0, 0.021]], 18, true);
    let piece = b.add("mixture lever", G_FUSE, mixture, Anim::hinge(Drive::new(Input::Mixture, -0.50).offset(0.25)), mx, Vec3::X, None);
    b.pieces[piece].hotspot = Some(Hotspot::wheel(n * 0.086, 0.014, "mixture_rich", "mixture_lean"));
    // The fuel selector (main tank, wings, off) and the parking brake handle.
    let (fs, rot) = con_face(0.0, 0.55);
    let n = rot * Vec3::Z;
    m.lathe(&CHROME, fs, n, &[[0.05, 0.0], [0.05, 0.003], [0.048, 0.004]], 32, false);
    m.disc(&FACE.x(gauge::FUEL_SEL_MAIN), fs + n * 0.0045, n, rot * Vec3::Y, 0.046, 32);
    let mut selector = Mesh::default();
    selector.rbox(&RED, fs + n * 0.012 + rot * Vec3::Y * 0.014, Vec3::new(0.008, 0.022, 0.007), 0.004, rot);
    let piece = b.add("tank selector", G_FUSE, selector, Anim::hinge(Drive::new(Input::FuelValve, -FRAC_PI_2)), fs, n, None);
    let mut target = Hotspot::wheel(n * 0.012 + rot * Vec3::Y * 0.020, 0.021, "fuel_next", "fuel_prev");
    target.click = Some("fuel_valve");
    b.pieces[piece].hotspot = Some(target);
    m.lathe(&CHROME, fs + n * 0.004, n, &[[0.008, 0.0], [0.008, 0.008], [0.0, 0.009]], 12, false);
    let (pb, rot) = con_face(0.042, 0.70);
    let n = rot * Vec3::Z;
    let mut brake = Mesh::default();
    brake.tube(&CHROME, &[pb - n * 0.01, pb + n * 0.03], &|_| 0.0035, 8, false);
    brake.rbox(&RED, pb + n * 0.034, Vec3::new(0.026, 0.0065, 0.0065), 0.005, rot);
    let piece = b.add("parking brake", G_FUSE, brake, Anim::slide(Drive::new(Input::ParkingBrake, 0.025), n), pb, n, None);
    b.pieces[piece].hotspot = Some(Hotspot::click(n * 0.034, 0.020, "parking_brake"));
    // DR400/180 handbook items 44 and 45: separate starter pushbutton and trim wheel.
    let (start, rot) = con_face(-0.045, 0.67);
    let normal = rot * Vec3::Z;
    let mut button = Mesh::default();
    button.lathe(&BLACK, start, normal, &[[0.010, 0.0], [0.010, 0.006], [0.008, 0.008], [0.0, 0.008]], 16, true);
    let piece = b.fixed("starter button", G_FUSE, button);
    b.pieces[piece].hotspot = Some(Hotspot::hold(start + normal * 0.008, 0.012, "starter"));
    label(m, pb + rot * Vec3::new(0.0, 0.018, 0.0), 0.02, 0.0045, rot);
    for (x, y) in [(-0.06f32, 0.06f32), (0.06, 0.06), (-0.06, -0.06), (0.06, -0.06)] {
        let (p, r) = con_face(x, 0.45 + y * 1.2);
        m.screw(&SCREW_BLACK, p, r * Vec3::Z, 0.0025, x + y * 3.0);
    }

    // The trim wheel's slot on the tunnel, with its nose-up / nose-down scale.
    let top = FLOOR_Y + 0.12;
    m.rbox(&BLACK, Vec3::new(0.0, top + 0.001, -0.66), Vec3::new(0.016, 0.002, 0.08), 0.0015, Quat::IDENTITY);
    m.rbox(&mt(mat::PLASTIC, 0.45, 0.45, 0.43).pat(pat::PLACARD), Vec3::new(0.034, top + 0.001, -0.66), Vec3::new(0.012, 0.0004, 0.06), 0.0002, Quat::IDENTITY);
    m.rbox(&mt(mat::PAINT, 0.6, 0.05, 0.03), Vec3::new(0.034, top + 0.0013, -0.66), Vec3::new(0.012, 0.0004, 0.004), 0.0002, Quat::IDENTITY);
    // The flap lever's quadrant: a slotted plate with three notches.
    for dx in [-0.016f32, 0.016] {
        m.rbox(&ALU, Vec3::new(dx, top + 0.035, -0.30), Vec3::new(0.0025, 0.035, 0.06), 0.002, Quat::IDENTITY);
        for k in 0..3 {
            let a = 0.05 + k as f32 * 0.3;
            let p = Vec3::new(dx * 1.18, top + 0.012 + 0.05 * a.sin(), -0.27 - 0.05 * a.cos());
            m.disc(&BLACK, p, Vec3::X * dx.signum(), Vec3::Y, 0.004, 10);
        }
    }
    label(m, Vec3::new(0.0, top + 0.001, -0.39), 0.014, 0.0004, Quat::IDENTITY);
}

// ---------------------------------------------------------------- seats

/// A seat's cushion and back, in velour panels between leatherette bolsters.
/// `c` is the middle of the cushion, `half` its half width; the back leans
/// back by `lean` from a hinge `back_z` behind the cushion's middle.
fn seat(m: &mut Mesh, c: Vec3, half: f32, back_z: f32, lean: f32, headrest: bool) {
    let cloth = CLOTH.pat(pat::SEAT);
    let tilt = Quat::from_rotation_x(0.10);
    let at = |p: Vec3| c + tilt * p;
    m.rbox(&cloth, at(Vec3::new(0.0, 0.004, 0.01)), Vec3::new(half - 0.035, 0.045, 0.20), 0.03, tilt);
    for s in [-1.0f32, 1.0] {
        m.rbox(&VINYL, at(Vec3::new(s * (half - 0.022), 0.012, 0.0)), Vec3::new(0.03, 0.052, 0.215), 0.026, tilt);
    }
    m.rbox(&VINYL, at(Vec3::new(0.0, -0.004, -0.198)), Vec3::new(half, 0.05, 0.032), 0.03, tilt);
    m.rbox(&VINYL, at(Vec3::new(0.0, -0.05, 0.0)), Vec3::new(half - 0.005, 0.02, 0.2), 0.01, tilt);
    // The back.
    let rot = Quat::from_rotation_x(lean);
    let hinge = c + Vec3::new(0.0, 0.03, back_z);
    let bk = |p: Vec3| hinge + rot * p;
    let h = 0.29;
    m.rbox(&cloth, bk(Vec3::new(0.0, h, 0.0)), Vec3::new(half - 0.035, h - 0.02, 0.045), 0.035, rot);
    for s in [-1.0f32, 1.0] {
        m.rbox(&VINYL, bk(Vec3::new(s * (half - 0.022), h, -0.012)), Vec3::new(0.03, h, 0.058), 0.028, rot);
    }
    m.rbox(&VINYL, bk(Vec3::new(0.0, 2.0 * h - 0.01, -0.004)), Vec3::new(half, 0.03, 0.055), 0.028, rot);
    m.rbox(&VINYL, bk(Vec3::new(0.0, h, 0.052)), Vec3::new(half - 0.004, h + 0.01, 0.012), 0.01, rot);
    if headrest {
        m.rbox(&VINYL, bk(Vec3::new(0.0, 2.0 * h + 0.11, -0.005)), Vec3::new(0.115, 0.065, 0.045), 0.035, rot);
        m.rbox(&cloth, bk(Vec3::new(0.0, 2.0 * h + 0.11, -0.048)), Vec3::new(0.085, 0.042, 0.004), 0.003, rot);
        for dx in [-0.07f32, 0.07] {
            m.tube(&CHROME, &[bk(Vec3::new(dx, 2.0 * h + 0.005, 0.0)), bk(Vec3::new(dx, 2.0 * h + 0.05, 0.0))], &|_| 0.0045, 8, false);
        }
    }
}

/// The top of the seat cushion at (x, z) from its middle `c`.
fn on_cushion(c: Vec3, dx: f32, dz: f32) -> Vec3 {
    c + Quat::from_rotation_x(0.10) * Vec3::new(dx, 0.05, dz)
}

fn seats(m: &mut Mesh) {
    let belt = STRAP.pat(pat::WEBBING);
    for xs in [-0.27f32, 0.27] {
        let c = Vec3::new(xs, FLOOR_Y + 0.11, -0.50);
        seat(m, c, 0.20, 0.23, 0.28, true);
        // The seat's frame on its two rails, and the adjuster bar under the front.
        for dx in [-0.13f32, 0.13] {
            m.rbox(&ALU, Vec3::new(xs + dx, FLOOR_Y + 0.006, -0.50), Vec3::new(0.012, 0.006, 0.33), 0.002, Quat::IDENTITY);
            for k in 0..22 {
                m.disc(&BLACK, Vec3::new(xs + dx, FLOOR_Y + 0.0125, -0.80 + k as f32 * 0.028), Vec3::Y, Vec3::Z, 0.004, 8);
            }
            for dz in [-0.68f32, -0.32] {
                m.tube(&BLACK, &[Vec3::new(xs + dx, FLOOR_Y + 0.012, dz), Vec3::new(xs + dx, c.y - 0.06, dz + 0.02)], &|_| 0.011, 8, false);
            }
        }
        m.tube(&CHROME, &[Vec3::new(xs - 0.06, c.y - 0.06, -0.70), Vec3::new(xs - 0.06, c.y - 0.075, -0.745), Vec3::new(xs + 0.06, c.y - 0.075, -0.745), Vec3::new(xs + 0.06, c.y - 0.06, -0.70)], &|_| 0.0045, 6, false);
        // A three-point belt: the lap belt's halves lying on the cushion, the
        // shoulder strap coming over the outboard shoulder of the back.
        let so = xs.signum();
        let buckle = on_cushion(c, -so * 0.10, -0.02) + Vec3::Y * 0.004;
        strap(m, &belt, on_cushion(c, so * 0.20, 0.18) + Vec3::Y * 0.006, buckle - Vec3::X * so * 0.03, Vec3::Y, 0.045, 0.003);
        strap(m, &belt, on_cushion(c, -so * 0.20, 0.17) + Vec3::Y * 0.006, buckle + Vec3::X * so * 0.02, Vec3::Y, 0.045, 0.003);
        m.rbox(&CHROME, buckle, Vec3::new(0.03, 0.005, 0.022), 0.003, Quat::from_rotation_y(0.3 * so));
        m.rbox(&BLACK, buckle + Vec3::new(-so * 0.03, 0.002, 0.0), Vec3::new(0.012, 0.006, 0.018), 0.004, Quat::from_rotation_y(0.3 * so));
        let rot = Quat::from_rotation_x(0.28);
        let hinge = c + Vec3::new(0.0, 0.03, 0.23);
        let top = hinge + rot * Vec3::new(so * 0.13, 0.58, -0.06);
        let low = hinge + rot * Vec3::new(-so * 0.10, 0.09, -0.052);
        strap(m, &belt, top, low, rot * Vec3::NEG_Z, 0.045, 0.003);
        strap(m, &belt, top + rot * Vec3::new(0.0, 0.0, 0.0), hinge + rot * Vec3::new(so * 0.14, 0.62, 0.07), rot * Vec3::Y, 0.045, 0.003);
    }
    // The rear bench: two places, a fold-down armrest between them.
    for xs in [-0.235f32, 0.235] {
        let c = Vec3::new(xs, FLOOR_Y + 0.12, 0.42);
        seat(m, c, 0.205, 0.24, 0.22, false);
        let so = xs.signum();
        let buckle = on_cushion(c, -so * 0.09, 0.02) + Vec3::Y * 0.004;
        strap(m, &belt, on_cushion(c, so * 0.20, 0.17) + Vec3::Y * 0.006, buckle, Vec3::Y, 0.045, 0.003);
        m.rbox(&CHROME, buckle, Vec3::new(0.03, 0.005, 0.022), 0.003, Quat::IDENTITY);
    }
    m.rbox(&VINYL, Vec3::new(0.0, FLOOR_Y + 0.33, 0.70), Vec3::new(0.03, 0.12, 0.05), 0.025, Quat::from_rotation_x(0.22));
    m.rbox(&TRIM, Vec3::new(0.0, FLOOR_Y + 0.035, 0.42), Vec3::new(0.46, 0.035, 0.21), 0.015, Quat::IDENTITY);
}

// ---------------------------------------------------------------- odds and ends

/// What makes it somebody's aeroplane: a headset and a checklist left on the right seat.
fn extras(m: &mut Mesh) {
    let c = Vec3::new(0.27, FLOOR_Y + 0.11, -0.50);
    let cup = mt(mat::PLASTIC, 0.06, 0.062, 0.068);
    let seal = mt(mat::FABRIC, 0.02, 0.02, 0.022);
    let (ca, cb) = (on_cushion(c, -0.08, -0.08) + Vec3::Y * 0.004, on_cushion(c, 0.08, -0.02) + Vec3::Y * 0.004);
    for p in [ca, cb] {
        m.lathe(&seal, p, Vec3::Y, &[[0.0, 0.0], [0.042, 0.0], [0.047, 0.008], [0.044, 0.016]], 22, false);
        m.lathe(&cup, p + Vec3::Y * 0.014, Vec3::Y, &[[0.043, 0.0], [0.045, 0.012], [0.04, 0.03], [0.026, 0.04], [0.0, 0.042]], 22, false);
        m.disc(&mt(mat::PAINT, 0.5, 0.5, 0.5).pat(pat::PLACARD), p + Vec3::Y * 0.0565, Vec3::Y, Vec3::Z, 0.012, 14);
    }
    let mid = (ca + cb) * 0.5;
    let side = (cb - ca).normalize();
    let bow = -Vec3::Y.cross(side).normalize();
    let band: Vec<Vec3> = (0..=16)
        .map(|k| {
            let a = (k as f32 / 16.0 - 0.5) * PI;
            mid + side * (a.sin() * 0.085) + bow * (a.cos() * 0.10) + Vec3::Y * 0.03
        })
        .collect();
    m.tube(&cup, &band, &|_| 0.0055, 8, true);
    m.tube(&seal, &band[4..13], &|_| 0.011, 10, true);
    let boom: Vec<Vec3> = [(0.0, 0.03, 0.0), (-0.03, 0.028, -0.04), (-0.04, 0.02, -0.10), (-0.025, 0.012, -0.14)].iter().map(|&(x, y, z)| ca + Vec3::new(x, y, z)).collect();
    m.tube(&BLACK, &boom, &|_| 0.0022, 6, true);
    m.dome(&seal, *boom.last().unwrap(), Vec3::new(0.3, 0.2, -1.0).normalize(), 0.011, 0.016, 12, 0.0);
    let cord: Vec<Vec3> = [(0.0, 0.01, 0.03), (0.03, -0.003, 0.09), (-0.05, -0.004, 0.13), (-0.10, -0.004, 0.06), (-0.14, -0.01, 0.0), (-0.15, -0.07, -0.03), (-0.15, -0.16, -0.06)]
        .iter()
        .map(|&(x, y, z)| ca + Vec3::new(x, y, z))
        .collect();
    m.tube(&BLACK, &cord, &|_| 0.0028, 6, true);
    // A laminated checklist beside it.
    let card = Quat::from_rotation_x(0.10) * Quat::from_rotation_y(0.35);
    let cc = on_cushion(c, 0.08, 0.12) + Vec3::Y * 0.0015;
    m.rbox(&mt(mat::PLASTIC, 0.75, 0.74, 0.70).pat(pat::PLACARD), cc, Vec3::new(0.055, 0.0012, 0.08), 0.001, card);
    m.rbox(&mt(mat::PLASTIC, 0.04, 0.12, 0.35), cc + card * Vec3::new(0.0, 0.0006, -0.068), Vec3::new(0.0545, 0.0008, 0.011), 0.0006, card);
}

// ---------------------------------------------------------------- controls

fn controls(b: &mut Builder) {
    for xs in [-0.27f32, 0.27] {
        // A stick each, on a gimbal at the floor: the ball turns fore and aft
        // with the elevator, and carries the stick, which leans with the ailerons.
        let pv = Vec3::new(xs, FLOOR_Y + 0.02, -0.80);
        let mut g = Mesh::default();
        g.lathe(&BLACK, pv, Vec3::Y, &[[0.0, -0.02], [0.02, -0.015], [0.026, 0.0], [0.02, 0.015], [0.0, 0.02]], 14, true);
        let gim = b.add("stick gimbal", G_FUSE, g, Anim::hinge(Drive::new(Input::Pitch, -0.22)), pv, Vec3::X, None);
        let mut s = Mesh::default();
        let bend = pv + Vec3::new(0.0, 0.34, 0.0);
        let top = pv + Vec3::new(0.0, 0.43, 0.035);
        s.tube(&BLACK, &[pv, pv + Vec3::Y * 0.30, bend, pv + Vec3::new(0.0, 0.38, 0.012), top], &|_| 0.0105, 12, false);
        // The grip: shaped for the fingers, a push-to-talk switch on its front.
        let ax = Vec3::new(0.0, 1.0, 0.3).normalize();
        s.lathe(&KNOB.pat(pat::KNURL), top - ax * 0.01, ax, &[[0.012, 0.0], [0.0165, 0.012], [0.0175, 0.03], [0.0155, 0.045], [0.0175, 0.06], [0.0185, 0.08], [0.0175, 0.10], [0.013, 0.114], [0.0, 0.12]], 18, true);
        s.lathe(&mt(mat::PLASTIC, 0.015, 0.015, 0.016), top - ax * 0.01, ax, &[[0.012, -0.006], [0.0125, 0.0], [0.0, 0.0]], 18, false);
        let ptt = top + ax * 0.09 + Vec3::new(0.0, 0.0, -0.016);
        s.rbox(&RED, ptt, Vec3::new(0.006, 0.006, 0.004), 0.003, Quat::from_rotation_x(-0.3));
        s.dome(&BLACK, top + ax * 0.11, ax, 0.006, 0.004, 10, 0.0);
        b.add("stick", G_FUSE, s, Anim::hinge(Drive::new(Input::Roll, -0.30)), pv, Vec3::Z, Some(gim));
        // A leather boot over the gimbal, and its trim ring on the floor.
        let mut s2 = Mesh::default();
        s2.lathe(&mt(mat::PLASTIC, 0.03, 0.028, 0.026), pv, Vec3::Y, &[[0.07, -0.015], [0.065, 0.0], [0.05, 0.022], [0.032, 0.04], [0.022, 0.055], [0.013, 0.075], [0.0115, 0.08]], 18, true);
        s2.lathe(&ALU, pv - Vec3::Y * 0.014, Vec3::Y, &[[0.082, 0.0], [0.082, 0.004], [0.07, 0.006]], 24, false);
        b.fixed("stick boot", G_FUSE, s2);

        // The rudder pedals, hinged on a torque tube low on the heel board,
        // toe brakes on top; the tube's bearings.
        let pz = -1.33;
        let py = floor_y(pz) + 0.035;
        let mut tt = Mesh::default();
        tt.tube(&GREY, &[Vec3::new(xs - 0.14, py, pz), Vec3::new(xs + 0.14, py, pz)], &|_| 0.011, 10, true);
        for dx in [-0.135f32, 0.0, 0.135] {
            tt.rbox(&GREY, Vec3::new(xs + dx, py - 0.016, pz), Vec3::new(0.012, 0.02, 0.018), 0.004, Quat::IDENTITY);
        }
        b.fixed("rudder bar", G_FUSE, tt);
        for dx in [-0.085f32, 0.085] {
            let mut p = Mesh::default();
            let pv = Vec3::new(xs + dx, py, pz);
            let pc = pv + Vec3::new(0.0, 0.13, 0.045);
            p.tube(&GREY, &[pv, pc - Vec3::Y * 0.03], &|_| 0.008, 8, true);
            let tilt = Quat::from_rotation_x(-0.5);
            p.rbox(&ALU, pc, Vec3::new(0.036, 0.062, 0.005), 0.004, tilt);
            for k in 0..4 {
                p.rbox(&RUBBER, pc + tilt * Vec3::new(0.0, -0.045 + k as f32 * 0.028, 0.0055), Vec3::new(0.03, 0.0045, 0.0015), 0.001, tilt);
            }
            p.tube(&BLACK, &[pc + tilt * Vec3::new(0.0, 0.02, -0.006), pc + tilt * Vec3::new(0.0, 0.03, -0.06)], &|_| 0.008, 8, true);
            let piece = b.add("pedal", G_FUSE, p, Anim::hinge2(Drive::new(Input::Yaw, -dx.signum() * 0.18), Drive::new(Input::Brake(dx.signum()), -0.14)), pv, Vec3::X, None);
            b.pieces[piece].hotspot = Some(Hotspot::hold(pc - pv, 0.038, if dx < 0.0 { "brake_left" } else { "brake_right" }));
        }
    }
    // The throttle, in the quadrant's left slot (forward for power).
    let mut t = Mesh::default();
    let (q, rot) = con_face(-0.025, 0.20);
    let n = rot * Vec3::Z;
    let tp = q - n * 0.05;
    t.tube(&CHROME, &[tp, tp + n * 0.13], &|_| 0.0045, 10, false);
    t.lathe(&KNOB, tp + n * 0.13, n, &[[0.0045, 0.0], [0.014, 0.004], [0.019, 0.016], [0.018, 0.028], [0.010, 0.036], [0.0, 0.038]], 20, true);
    let piece = b.add("throttle", G_FUSE, t, Anim::hinge(Drive::new(Input::Throttle, -0.5).offset(0.25)), tp, Vec3::X, None);
    b.pieces[piece].hotspot = Some(Hotspot::wheel(n * 0.15, 0.021, "throttle_up", "throttle_down"));
    // The pitch trim wheel, half sunk in the tunnel.
    let mut w = Mesh::default();
    let wc = Vec3::new(0.0, FLOOR_Y + 0.085, -0.66);
    w.lathe(&mt(mat::PLASTIC, 0.06, 0.06, 0.062).pat(pat::KNURL), wc - Vec3::X * 0.009, Vec3::X, &[[0.0, 0.0], [0.062, 0.0], [0.067, 0.004], [0.067, 0.014], [0.062, 0.018], [0.0, 0.018]], 40, false);
    w.rbox(&PAINT_WHITE, wc + Vec3::new(0.0, 0.0, -0.066), Vec3::new(0.0092, 0.003, 0.0012), 0.0008, Quat::IDENTITY);
    b.add("trim wheel", G_FUSE, w, Anim::hinge(Drive::new(Input::Trim, 18.0)), wc, Vec3::X, None);
    // The flap lever between the seats, pulled up a notch at a time.
    let mut fl = Mesh::default();
    let fp = Vec3::new(0.0, FLOOR_Y + 0.135, -0.27);
    let tip = fp + Vec3::new(0.0, 0.035, -0.24);
    let d = (tip - fp).normalize();
    fl.rbox(&CHROME, (fp + tip) * 0.5, Vec3::new(0.006, 0.012, 0.13), 0.004, Quat::from_rotation_x(d.y.atan2(-d.z)));
    fl.tube(&BLACK, &[fp + d * 0.15, tip + d * 0.02], &|t| 0.013 + 0.002 * (t * PI).sin(), 14, true);
    fl.lathe(&RED, tip + d * 0.02, d, &[[0.007, 0.0], [0.007, 0.006], [0.0, 0.008]], 12, true);
    let piece = b.add("flap lever", G_FUSE, fl, Anim::hinge(Drive::new(Input::FlapLever, 0.6)), fp, Vec3::X, None);
    b.pieces[piece].hotspot = Some(Hotspot::wheel(tip - fp, 0.022, "flaps_up", "flaps_down"));
}

// ---------------------------------------------------------------- canopy

/// How far the bubble slides open (m), and which way: forward, rising a
/// little on its rails so it rides over the windscreen.
const SLIDE: f32 = 0.58;
fn slide_dir() -> Vec3 {
    Vec3::new(0.0, 0.17, -1.0).normalize()
}

/// The bubble's lines, a shade bigger than the windscreen's.
const GROW: f32 = 1.01;
fn bubble_pt(z: f32, u: f32) -> Vec3 {
    let p = canopy_pt(z, u);
    Vec3::new(p.x * GROW, p.y + (GROW - 1.0) * 0.5, p.z)
}

/// A bow across the canopy at station z, inside the glass: `w` wide, `d` deep.
fn bow(m: &mut Mesh, mat: &Mat, z: f32, w: f32, d: f32, pt: &dyn Fn(f32, f32) -> Vec3) {
    let n = 48;
    let path: Vec<Vec3> = (0..=n).map(|k| pt(z, k as f32 / n as f32 * 2.0)).collect();
    let inward: Vec<Vec3> = (0..=n).map(|k| { let t = path[(k + 1).min(n)] - path[k.saturating_sub(1)]; Vec3::Z.cross(t).normalize() }).collect();
    sweep(m, mat, &path, &vec![Vec3::Z; n + 1], &inward, &rect_prof(-w * 0.5, w * 0.5, d));
    // The rubber seal where the glass meets it, either side.
    for a in [-w * 0.5 - 0.003, w * 0.5 + 0.003] {
        let p2: Vec<Vec3> = path.iter().zip(&inward).map(|(p, i)| *p + Vec3::Z * a + *i * 0.002).collect();
        let i2 = inward.clone();
        sweep(m, &BLACK, &p2, &vec![Vec3::Z; n + 1], &i2, &rect_prof(-0.003, 0.003, 0.002));
    }
}

/// A rail along the bubble's lower edge on side sx, from za to zb.
fn edge(m: &mut Mesh, mat: &Mat, sx: f32, za: f32, zb: f32, h: f32, d: f32) {
    let n = 40;
    let u = if sx > 0.0 { 0.0 } else { 2.0 };
    let path: Vec<Vec3> = (0..=n).map(|k| bubble_pt(za + (zb - za) * k as f32 / n as f32, u)).collect();
    let inward: Vec<Vec3> = path.iter().map(|_| Vec3::X * -sx).collect();
    let up = vec![Vec3::Y; n + 1];
    sweep(m, mat, &path, &up, &inward, &rect_prof(0.0, h, d));
}

fn canopy(b: &mut Builder) {
    let (z0, z1) = (CANOPY[0] - 0.02, CANOPY[1] + 0.02);
    let glass = |za: f32, zb: f32, nz: usize, grow: f32| -> Mesh {
        let nu = 48;
        let rows: Vec<Vec<SP>> = (0..=nz)
            .map(|i| {
                let z = za + (zb - za) * i as f32 / nz as f32;
                (0..=nu).map(|k| { let p = canopy_pt(z, k as f32 / nu as f32 * 2.0); let p = Vec3::new(p.x * grow, p.y + (grow - 1.0) * 0.5, p.z); (p, [z, p.x], 0.0) }).collect()
            })
            .collect();
        let mut g = Mesh::default();
        g.grid(&mt(mat::GLASS, 0.0, 0.0, 0.0), &rows, false, false);
        g
    };
    // The windscreen, fixed to the cowl deck, and the bubble behind it (a
    // shade bigger, so it slides over the windscreen without touching it).
    let k = b.fixed("windscreen", G_FUSE, glass(z0, SCREEN_Z, 30, 1.0));
    b.pieces[k].glass = true;
    b.pieces[k].occluder = false;
    let open = || Anim::slide(Drive::new(Input::Canopy, SLIDE), slide_dir());
    let k = b.add("canopy", G_FUSE, glass(SCREEN_Z, z1, 64, GROW), open(), Vec3::ZERO, Vec3::Z, None);
    b.pieces[k].glass = true;
    b.pieces[k].occluder = false;
    // The bubble's frame: a broad front bow with the handle, the rear bow,
    // the rails along its lower edges and a latch either side.
    let mut f = Mesh::default();
    bow(&mut f, &FRAME, SCREEN_Z + 0.028, 0.05, 0.022, &bubble_pt);
    bow(&mut f, &FRAME, CANOPY[1] - 0.06, 0.04, 0.02, &bubble_pt);
    for sx in [-1.0f32, 1.0] {
        edge(&mut f, &FRAME, sx, SCREEN_Z + 0.004, CANOPY[1] - 0.04, 0.03, 0.012);
        let base = bubble_pt(SCREEN_Z + 0.12, if sx > 0.0 { 0.0 } else { 2.0 }) - Vec3::X * sx * 0.012 + Vec3::Y * 0.015;
        f.rbox(&BLACK, base - Vec3::X * sx * 0.006, Vec3::new(0.006, 0.012, 0.018), 0.003, Quat::IDENTITY);
        f.tube(&BLACK, &[base - Vec3::X * sx * 0.01, base + Vec3::new(-sx * 0.02, 0.005, 0.06)], &|t| 0.006 - 0.001 * t, 8, true);
        f.dome(&BLACK, base + Vec3::new(-sx * 0.02, 0.005, 0.06), Vec3::Z, 0.0065, 0.005, 10, 0.0);
    }
    let bow_in = |u: f32| -> Vec3 {
        let z = SCREEN_Z + 0.028;
        let p = bubble_pt(z, u);
        let t = bubble_pt(z, u + 0.01) - bubble_pt(z, u - 0.01);
        p + Vec3::Z.cross(t).normalize() * 0.022
    };
    let h = bow_in(1.0);
    let hl = [h - Vec3::X * 0.065, h - Vec3::X * 0.065 - Vec3::Y * 0.03 + Vec3::Z * 0.008, h + Vec3::X * 0.065 - Vec3::Y * 0.03 + Vec3::Z * 0.008, h + Vec3::X * 0.065];
    f.tube(&BLACK, &hl, &|_| 0.0075, 10, true);
    for p in [hl[0], hl[3]] {
        f.rbox(&BLACK, p, Vec3::new(0.012, 0.004, 0.016), 0.003, Quat::IDENTITY);
        f.screw(&SCREW_BLACK, p - Vec3::Y * 0.004, Vec3::NEG_Y, 0.0022, p.x);
    }
    let piece = b.add("canopy frame", G_FUSE, f, open(), Vec3::ZERO, Vec3::Z, None);
    b.pieces[piece].hotspot = Some(Hotspot::click((hl[1] + hl[2]) * 0.5, 0.034, "canopy"));
    // Fixed: the windscreen's top edge and the rails on the sills.
    let mut r = Mesh::default();
    let arc: Vec<Vec3> = (0..=40).map(|k| canopy_pt(SCREEN_Z - 0.004, k as f32 / 20.0) + Vec3::Y * 0.002).collect();
    r.tube(&FRAME, &arc, &|_| 0.006, 8, true);
    for sx in [-1.0f32, 1.0] {
        let rail: Vec<Vec3> = (0..=30).map(|k| { let z = z0 + (z1 - z0) * k as f32 / 30.0; let s = sill(z); Vec3::new(sx * (s.x + 0.004), s.y + 0.006, z) }).collect();
        r.tube(&CHROME, &rail, &|_| 0.007, 8, true);
    }
    b.fixed("canopy rails", G_FUSE, r);
}

// ---------------------------------------------------------------- what the eye sees

/// Whether a look from `eye` along `d` (body frame) ends on something in the
/// cockpit (true) or goes out through the canopy (false).
pub fn sees_inside(eye: Vec3, d: Vec3) -> bool {
    let mut t = 0.02;
    while t < 3.0 {
        let p = eye + d * t;
        if p.y < FLOOR_Y + 0.01 {
            return true;
        }
        if p.z < PANEL_Z + 0.01 && p.y < 0.13 + 0.6 * (PANEL_Z - p.z).max(0.0) {
            return true;
        }
        if p.z < CANOPY[0] || p.z > CANOPY[1] {
            return p.y < sec(p.z).top;
        }
        let s = sill(p.z);
        if p.y < s.y {
            if p.x.abs() > inner_x(p.z, p.y) {
                return true;
            }
        } else {
            // Out through the glass.
            let top = canopy_top(p.z);
            let u = ((p.y - s.y) / (top - s.y).max(1e-3)).clamp(0.0, 1.0);
            if p.y > top || p.x.abs() > s.x * (1.0 - u * u).max(0.0).sqrt() {
                return false;
            }
        }
        t += 0.015 + t * 0.01;
    }
    true
}
