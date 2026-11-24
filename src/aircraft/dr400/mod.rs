//! A Robin DR 400/180 Régent: the French four-seat wooden tourer and glider
//! tug (180 hp Lycoming O-360, 1100 kg at most, 8.72 m span, 6.96 m long,
//! 2.23 m tall). Built here in Rust like the Cessna (see `c172`): a lofted
//! superellipse fuselage open under a forward-sliding bubble canopy, the
//! Jodel "cranked" wing (a flat centre section, outer panels at 14 degrees
//! of dihedral), an all-moving tailplane, fixed tricycle gear in wheel
//! fairings, a two-blade fixed-pitch propeller. The cockpit lives in `cabin.rs`.
//!
//! The figures: 13.6 m² of wing, flaps at 15° (take-off) and 60° (landing),
//! VFE 170 km/h, VNE 308 km/h, 190 l of fuel (110 l in the main tank behind
//! the engine, 2 × 40 l in the wings).
//!
//! The paint (white, the blue and red cheat lines of the classic Robin
//! scheme, made up, no registration; black walkways on the wing roots) is
//! drawn in plane.wgsl (`livery_dr400`), and so is the skin's relief
//! (`relief_dr400`): wood and fabric, no rivets; rib tapes and the fabric's
//! sag between the ribs behind the spars, the glass-fibre cowling's seams
//! and oil door. The two round cooling inlets in the nose bowl are cut out
//! of the skin there too (`inlet_dr400`), onto ducts built here.
//!
//! Body frame as everywhere: +X right, +Y up, -Z forward; the geometry is
//! laid out in a design frame `DZ` ahead of it (see `DZ`).
pub mod cabin;

use crate::aircraft::{Engine, Handling, Hole, Hull, Leg, Livery, Part, Piston, Sound, Spec, Tail};
use crate::airframe::*;
use glam::{DVec3, Mat4, Quat, Vec2, Vec3};
use std::f32::consts::{PI, TAU};

pub fn build() -> Model {
    let mut b = Builder::default();
    fuselage(&mut b);
    wings(&mut b);
    tail(&mut b);
    engine(&mut b);
    gear(&mut b);
    cabin::build(&mut b);
    // Built about the wing's leading edge; the body frame's origin is the CG.
    for p in &mut b.pieces {
        p.mesh.transform(0, Mat4::from_translation(Vec3::Z * DZ));
        p.pivot.z += DZ;
    }
    bake_ao(&mut b.pieces, &AoBake { vox: 0.025, reach: 2.4, rays: 28, open: &|_, _| false });
    Model::finish(b.pieces)
}

/// Paint schemes for the aircraft picker (`aircraft::liveries`), linear RGB:
/// the cheat line and fin top (`colours[0]`, w 1), the pinstripe under it,
/// the wing tips and spinner, and the airframe. plane.wgsl `livery_dr400`
/// paints them. All are made up: "Classic" is a typical Robin white with blue
/// cheat line and red tips; the others are fictional club and private schemes.
pub const LIVERIES: &[Livery] = &[
    scheme("Classic", [0.012, 0.045, 0.20], [0.42, 0.02, 0.015], [0.42, 0.02, 0.015], [0.80, 0.80, 0.78], include_bytes!("../../../assets/aircraft/liveries/dr400_classic.png")),
    scheme("Sunburst", [0.70, 0.13, 0.01], [0.85, 0.55, 0.03], [0.70, 0.13, 0.01], [0.80, 0.75, 0.60], include_bytes!("../../../assets/aircraft/liveries/dr400_sunburst.png")),
    scheme("Aéroclub", [0.018, 0.018, 0.022], [0.55, 0.025, 0.02], [0.55, 0.025, 0.02], [0.80, 0.80, 0.78], include_bytes!("../../../assets/aircraft/liveries/dr400_aeroclub.png")),
    scheme("Racing green", [0.008, 0.09, 0.035], [0.62, 0.44, 0.10], [0.008, 0.09, 0.035], [0.78, 0.76, 0.68], include_bytes!("../../../assets/aircraft/liveries/dr400_green.png")),
];
const fn scheme(name: &'static str, line: [f32; 3], pin: [f32; 3], tips: [f32; 3], body: [f32; 3], still: &'static [u8]) -> Livery {
    Livery { name, colours: [[line[0], line[1], line[2], 1.0], [pin[0], pin[1], pin[2], 0.0], [tips[0], tips[1], tips[2], 0.0], [body[0], body[1], body[2], 0.0]], title: "", paint: None, swatch: [line, pin], still }
}

/// The model is laid out in a design frame with the wing's root leading edge
/// at z = -1.00; the CG (the body frame's origin) is at about 26% of the
/// root chord, 0.55 m behind that frame's origin. Body z = design z + DZ.
pub const DZ: f32 = 0.55;
const DZ64: f64 = DZ as f64;

const KT: f64 = 1.0 / 1.94384;
const V_NONE: f64 = 1e9;

/// The tyres touch the ground here (body y), the gear at rest.
pub const GROUND_Y: f32 = -1.20;

const fn leg(at: DVec3, r: f32, travel: f64, k: f64, steer: f64, name: &'static str) -> Leg {
    // (Fixed gear: it never folds, so no fold speed or kick.)
    Leg { at, wheel_r: r as f64, travel, k, c: k * 0.1, nf_max: 110_000.0, steer, brakes: true, side_k: 5000.0, side_mu: 0.9, roll_k: 80.0, roll_mu: 0.025, brake_k: 2400.0, brake_mu: 0.55, fold_vn: 1e9, fold_kick: DVec3::ZERO, name }
}

pub static SPEC: Spec = Spec {
    id: 8,
    name: "Robin DR 400/180",
    keys: &["dr400", "robin", "regent"],
    blurb: "French four-seat wooden tourer and glider tug, 180 hp, fixed gear. Light on the controls; 60° flaps for steep, short landings.",
    origin: None,
    ext: None,
    still: include_bytes!("../../../assets/aircraft/dr400.png"),
    paint: None,
    build,
    groups: [("fuselage", 230.0), ("tail", 45.0), ("left wing", 105.0), ("right wing", 105.0), ("engine", 160.0), ("propeller", 22.0), ("gear", 45.0)],
    span: SPAN,
    wing_sect: |s| {
        let mut k = wing_sect(s);
        k.le.z += DZ;
        k
    },
    fuse: |z| {
        let s = sec(z - DZ);
        (s.top, s.mid, s.bot)
    },
    aileron: AILERON,
    tank: TANK,
    tail: Tail {
        fin_y0: 0.06,
        fin_h: 0.97,
        rudder_y0: 0.0,
        rudder_h: 1.0,
        stab_root: 0.80,
        elev_root: 0.80,
        break_z: [1.3 + DZ, 2.5 + DZ],
        fin_base: DVec3::new(0.0, 0.06, 3.85 + DZ64),
        fin_tip: DVec3::new(0.0, 1.03, 4.05 + DZ64),
        stab_tip: DVec3::new(1.60, 0.06, 3.95 + DZ64),
    },
    eye: Vec3::new(EYE.x, EYE.y, EYE.z + DZ),
    hotspots: &[super::Hotspot { at: [0.0, cabin::FLOOR_Y + 0.15, -0.66 + DZ], r: 0.018, click: None, hold: None, wheel: Some(("trim_down", "trim_up")) }],
    eye2: None,
    sees_inside: |eye, d| cabin::sees_inside(eye - Vec3::Z * DZ, d),
    // (Body frame: the design frame's z + 0.55.)
    contacts: &[
        ([0.0, -0.05, -2.11], Part::Nose),
        ([0.0, -0.55, -1.55], Part::Nose),
        ([0.0, 0.28, -1.05], Part::Nose),
        ([-0.48, -0.15, -1.25], Part::Nose),
        ([0.48, -0.15, -1.25], Part::Nose),
        ([0.0, -0.74, -0.45], Part::Belly),
        ([0.0, -0.74, 0.55], Part::Belly),
        ([0.0, -0.68, 1.55], Part::Belly),
        ([-0.55, -0.20, -0.35], Part::Side),
        ([0.55, -0.20, -0.35], Part::Side),
        ([-0.55, -0.20, 1.05], Part::Side),
        ([0.55, -0.20, 1.05], Part::Side),
        ([0.0, 0.58, 0.35], Part::Roof),
        ([0.0, 0.30, 1.60], Part::Roof),
    ],
    tail_pts: &[[0.0, -0.50, 2.35], [0.0, -0.20, 3.55], [0.0, 0.04, 4.90], [0.0, 0.17, 2.95]],
    wing_st: [0.45 / SPAN, 1.3 / SPAN, 2.2 / SPAN, 3.2 / SPAN, 4.1 / SPAN],
    hard: [
        DVec3::new(0.0, -0.05, -2.11),
        DVec3::new(-4.36, -0.06, 0.0),
        DVec3::new(4.36, -0.06, 0.0),
        DVec3::new(0.0, 0.04, 4.90),
        DVec3::new(0.0, 1.03, 4.60),
        DVec3::new(0.0, -0.74, 0.55),
        DVec3::new(0.0, -0.965, NOSE_AXLE.z as f64 + DZ64),
    ],
    gear: &[
        leg(DVec3::new(0.0, -1.20, NOSE_AXLE.z as f64 + DZ64), NOSE_R, 0.10, 45_000.0, 1.0, "NOSE GEAR"),
        leg(DVec3::new(-1.30, -1.20, MAIN_AXLE.z as f64 + DZ64), MAIN_R, 0.08, 120_000.0, 0.0, "LEFT MAIN GEAR"),
        leg(DVec3::new(1.30, -1.20, MAIN_AXLE.z as f64 + DZ64), MAIN_R, 0.08, 120_000.0, 0.0, "RIGHT MAIN GEAR"),
    ],
    hull: Hull { k: 50_000.0, c: 7_500.0, nf_max: 100_000.0, drag: 2600.0 },
    hole: Hole { fus: [0.56, 0.62, -0.30, 0.45], wing: [-0.66, 0.14, 0.13, 0.08], stab: [0.06, 0.05, 1.60], fin: [0.06, 0.97, 0.12, 0.06], pods: [[0.0; 3]; 2] },
    // Two aboard and half the fuel: about 600 kg empty, 160 kg of people, 68 kg of fuel.
    mass: 900.0,
    // 190 l of avgas usable: 110 l in the main tank behind the firewall, 40 l in each wing.
    fuel_kg: 137.0,
    // The same O-360 as the Cessna: about 2 US gal/h idling, 14 at full power.
    fuel_flow: [0.0015, 0.0105, 0.0135],
    // Two up front included; the back seats and the baggage behind them are aft.
    payload: [160.0, 400.0, 0.75],
    // The centre section's chord; CG 14-33% of it. The main tank is well
    // ahead of the CG, so the fuel burnt moves it aft.
    mac: 1.71,
    cg: [0.25, 0.14, 0.33],
    fuel_arm: -0.45,
    // Smaller and lighter than the Cessna's (pitch, yaw, roll).
    inertia: DVec3::new(1150.0, 1750.0, 950.0),
    wing_area: 13.6,
    b: 8.72,
    // Aspect ratio 5.6: less lift slope and more induced drag than the Cessna;
    // a clean wooden airframe with the wheels in fairings.
    cl0: 0.30,
    cl_a: 5.0,
    cd0: 0.0225,
    k_ind: 0.073,
    // (Fixed gear: always out, its drag always on.)
    cd_gear: 0.006,
    // Plain flaps: 60° is mostly drag, the steep approach the type is known for.
    cd_flap: [0.03, 0.07],
    flap_cl: 0.55,
    stall_alpha: 0.27,
    stall_flap: 0.03,
    stall_slat: 0.0,
    alpha_trim: 0.24,
    mcrit: 0.0,
    cd_wave: 0.0,
    spoiler_cd: 0.0,
    spoiler_cl: 0.0,
    chute: None,
    flaps: &[0.0, 15.0, 60.0],
    // VFE 170 km/h for both notches.
    vfe: &[V_NONE, 92.0 * KT, 92.0 * KT],
    flap_rate: 1.0 / 3.0,
    slats_at: 0,
    lever: None,
    // Fixed gear: it never moves (see `Spec::fixed_gear`).
    gear_rate: 0.0,
    electric_actuators: false,
    // VNE 308 km/h; flutter past about 185 kt.
    vne: 85.6,
    mmo: 0.0,
    flutter_v: 95.0,
    inlet_mach: 0.0,
    dry_mach: 0.0,
    // Normal category: +3.8 g limit; the spar lets go a good way past it.
    g_limit: 3.9,
    g_break: 5.8,
    fbw_g: 0.0,
    steer_max: 0.45,
    steer_fade: 0.0,
    // 180 hp at 2700 rpm turning a 72 in two-blade fixed-pitch propeller.
    engine: Engine::Piston(Piston {
        power: 134_000.0,
        w_rated: 2700.0 * std::f64::consts::TAU / 60.0,
        prop_d: 1.83,
        // Pitched for the cruise (the 76EM8 of the /180): about 2350 rpm
        // static, 2700 flat out and level.
        ct0: 0.109,
        cp0: 0.072,
        j_t: 1.25,
        j_p: 1.97,
        prop_i: 2.6,
        static_thrust: 2400.0,
        fric0: 15.0,
        fric1: 35.0,
        blade_break_y: PROP_HUB.y + 0.40,
        governor: None,
    }),
    // The Cessna's, quicker in roll (shorter span, a stick, light ailerons).
    handling: Handling {
        q_ref: 2200.0,
        a_max: 2.6,
        damp_v: 68.0,
        ad_min: 0.3,
        ad_max: 2.6,
        pitch_k: 7.5,
        pitch_k_lost: 2.0,
        pitch_ctl: 0.65,
        alpha_ctl: 0.27,
        pitch_d: 3.0,
        roll_ctl: 5.2,
        roll_d: 4.0,
        roll_beta: 0.9,
        roll_clean: 1.0,
        yaw_ctl: 1.4,
        yaw_beta: 4.2,
        yaw_d: 2.3,
        dihedral: 0.45,
        adverse: 0.3,
        stall_pitch: 0.9,
        stall_drop: 1.5,
        stall_yaw: 2.4,
    },
    // About 120 kt in the cruise; a 65 kt final with full flaps.
    cruise: 62.0,
    approach: (34.0, 2),
    cam_dist: 13.0,
    shadow_r: 5.2,
    sound: Sound::Piston,
};

// ---------------------------------------------------------------- fuselage

/// Fuselage cross-section keys: station z, half width, top, waist height,
/// bottom, squareness of the upper and the lower half. Under the canopy
/// (`CANOPY`) the top is the sill the glass sits on; the skin there is open.
const FUSE: [[f32; 7]; 17] = [
    [-2.40, 0.20, 0.08, -0.05, -0.20, 2.0, 2.0],
    [-2.36, 0.30, 0.13, -0.06, -0.31, 2.3, 2.3],
    [-2.28, 0.38, 0.18, -0.08, -0.42, 2.6, 2.6],
    [-2.10, 0.45, 0.22, -0.10, -0.52, 2.9, 3.0],
    [-1.80, 0.50, 0.26, -0.12, -0.58, 3.2, 3.4],
    [-1.50, 0.53, 0.28, -0.14, -0.63, 3.6, 3.8],
    [-1.25, 0.55, 0.20, -0.18, -0.70, 5.0, 4.0],
    [-0.80, 0.56, 0.10, -0.20, -0.74, 7.0, 4.0],
    [0.60, 0.56, 0.10, -0.20, -0.74, 7.0, 4.0],
    [1.00, 0.54, 0.24, -0.19, -0.71, 4.0, 3.8],
    [1.50, 0.45, 0.24, -0.15, -0.58, 3.0, 3.3],
    [2.20, 0.33, 0.18, -0.09, -0.42, 2.6, 2.8],
    [2.90, 0.22, 0.12, -0.04, -0.27, 2.4, 2.5],
    [3.60, 0.13, 0.08, -0.01, -0.14, 2.2, 2.2],
    [4.10, 0.06, 0.055, 0.01, -0.06, 2.0, 2.0],
    [4.30, 0.03, 0.045, 0.01, -0.03, 2.0, 2.0],
    [4.35, 0.015, 0.04, 0.01, -0.01, 2.0, 2.0],
];
pub const NOSE_Z: f32 = -2.40;
pub const TAIL_Z: f32 = 4.35;
pub const FIREWALL_Z: f32 = -1.50;
/// The opening under the canopy: windscreen base, rear of the canopy.
pub const CANOPY: [f32; 2] = [-1.30, 1.05];

#[derive(Clone, Copy, Debug)]
pub struct Sec {
    pub w: f32,
    pub top: f32,
    pub mid: f32,
    pub bot: f32,
    pub nt: f32,
    pub nb: f32,
}

/// Monotone cubic through the keys (no overshoot between stations).
fn pchip(col: usize, z: f32) -> f32 {
    let k = &FUSE;
    let n = k.len();
    if z <= k[0][0] {
        return k[0][col];
    }
    if z >= k[n - 1][0] {
        return k[n - 1][col];
    }
    let i = (0..n - 1).find(|&i| z < k[i + 1][0]).unwrap_or(n - 2);
    let h = |i: usize| k[i + 1][0] - k[i][0];
    let d = |i: usize| (k[i + 1][col] - k[i][col]) / h(i);
    let slope = |i: usize| -> f32 {
        if i == 0 || i == n - 1 {
            return if i == 0 { d(0) } else { d(n - 2) };
        }
        let (a, b) = (d(i - 1), d(i));
        if a * b <= 0.0 {
            0.0
        } else {
            let (w1, w2) = (2.0 * h(i) + h(i - 1), h(i) + 2.0 * h(i - 1));
            (w1 + w2) / (w1 / a + w2 / b)
        }
    };
    let t = (z - k[i][0]) / h(i);
    let (y0, y1, m0, m1) = (k[i][col], k[i + 1][col], slope(i) * h(i), slope(i + 1) * h(i));
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * y0 + (t3 - 2.0 * t2 + t) * m0 + (-2.0 * t3 + 3.0 * t2) * y1 + (t3 - t2) * m1
}
pub fn sec(z: f32) -> Sec {
    Sec { w: pchip(1, z), top: pchip(2, z), mid: pchip(3, z), bot: pchip(4, z), nt: pchip(5, z), nb: pchip(6, z) }
}
/// Point on a section at angle th (0 = top, pi/2 = right side, pi = bottom).
pub fn sec_pt(s: &Sec, th: f32) -> Vec2 {
    let (sn, cs) = th.sin_cos();
    let n = if cs > 0.0 { s.nt } else { s.nb };
    let e = |v: f32| v.signum() * v.abs().powf(2.0 / n);
    let h = if cs > 0.0 { s.top - s.mid } else { s.mid - s.bot };
    Vec2::new(s.w * e(sn), s.mid + h * e(cs))
}
pub fn fuse_pt(z: f32, th: f32) -> Vec3 {
    let p = sec_pt(&sec(z), th);
    Vec3::new(p.x, p.y, z)
}
/// Outward normal of the fuselage skin at (z, th).
pub fn fuse_n(z: f32, th: f32) -> Vec3 {
    let e = 1e-3;
    let du = fuse_pt(z + e, th) - fuse_pt(z - e, th);
    let dv = fuse_pt(z, th + e) - fuse_pt(z, th - e);
    du.cross(dv).normalize()
}
/// Angle on the section at z whose point is at height y on the right side.
pub fn fuse_th(z: f32, y: f32) -> f32 {
    let s = sec(z);
    let (mut a, mut b) = (0.0f32, PI);
    for _ in 0..30 {
        let m = (a + b) * 0.5;
        if sec_pt(&s, m).y > y { a = m } else { b = m }
    }
    (a + b) * 0.5
}
/// Point on the skin at station z, height y, on side sx (+1 right), lifted by `off`.
pub fn on_side(z: f32, y: f32, sx: f32, off: f32) -> (Vec3, Vec3) {
    let th = fuse_th(z, y);
    let (mut p, mut n) = (fuse_pt(z, th), fuse_n(z, th));
    p.x *= sx;
    n.x *= sx;
    (p + n * off, n)
}
/// How far round from the top the skin starts at station z: under the
/// canopy it stops at the sill (the corner of the section), elsewhere it
/// closes over the top.
fn skin_th0(z: f32) -> f32 {
    let open = smoothstep(CANOPY[0] - 0.02, CANOPY[0] + 0.03, z) * (1.0 - smoothstep(CANOPY[1] - 0.03, CANOPY[1] + 0.02, z));
    0.62 * open
}

// Landing gear geometry (body frame, right side for the mains).
pub const NOSE_AXLE: Vec3 = Vec3::new(0.0, GROUND_Y + NOSE_R, -1.98);
pub const NOSE_R: f32 = 0.165;
pub const MAIN_AXLE: Vec3 = Vec3::new(1.30, GROUND_Y + MAIN_R, -0.40);
pub const MAIN_R: f32 = 0.19;
/// where the main legs come out of the wing's lower skin
pub const MAIN_ROOT: Vec3 = Vec3::new(1.10, -0.76, -0.55);
pub const PROP_HUB: Vec3 = Vec3::new(0.0, -0.05, -2.46);
/// the pilot's eye (left front seat)
pub const EYE: Vec3 = Vec3::new(-0.27, 0.30, -0.36);

/// How far outside the fuselage skin (x, y) is at station z: < 0 inside.
fn sec_f(z: f32, x: f32, y: f32) -> f32 {
    let s = sec(z);
    let (h, n) = if y > s.mid { (s.top - s.mid, s.nt) } else { (s.mid - s.bot, s.nb) };
    (x.abs() / s.w).powf(n) + ((y - s.mid).abs() / h).powf(n) - 1.0
}
/// Where a line along z through (x, y) first meets the skin, coming from the nose.
fn skin_z(x: f32, y: f32, z0: f32, z1: f32) -> f32 {
    let (mut a, mut b) = (z0, z1);
    if sec_f(a, x, y) < 0.0 {
        return a;
    }
    for _ in 0..32 {
        let m = (a + b) * 0.5;
        if sec_f(m, x, y) > 0.0 { a = m } else { b = m }
    }
    (a + b) * 0.5
}

/// The two round cooling-air inlets in the nose bowl, either side of the
/// spinner (the right one: x, y, radius). The skin is cut away inside them
/// (`inlet_dr400` in plane.wgsl: keep in step).
pub const INLET: [f32; 3] = [0.225, -0.19, 0.062];

fn intakes(m: &mut Mesh) {
    let duct = mt(mat::SATIN, 0.035, 0.034, 0.033);
    let back_z = -2.17;
    for sx in [-1.0f32, 1.0] {
        let c = Vec2::new(sx * INLET[0], INLET[1]);
        let r = INLET[2];
        let ring = |a: f32, rr: f32| c + Vec2::new(a.cos(), a.sin()) * rr;
        // The duct: from the lip back to the cylinder baffles.
        m.patch(&duct, 32, 6, sx < 0.0, |u, v| {
            let a = u * TAU;
            let q = ring(a, r + 0.002);
            let z0 = skin_z(q.x, q.y, NOSE_Z, -2.0) - 0.004;
            let z = z0 + (back_z - z0) * v;
            // (Narrowing a little into the plenum.)
            let q = ring(a, r + 0.002 - 0.008 * v);
            (Vec3::new(q.x, q.y, z), [u * TAU * r, z], 0.0)
        });
        // The lip rolled round the opening, following the curve of the nose bowl.
        let lip: Vec<Vec3> = (0..=48).map(|k| {
            let a = k as f32 / 48.0 * TAU;
            let q = ring(a, r + 0.004);
            Vec3::new(q.x, q.y, skin_z(q.x, q.y, NOSE_Z, -2.0) - 0.002)
        }).collect();
        m.tube(&WHITE, &lip, &|_| 0.0055, 8, false);
        // The baffle at the back and the finned cylinder head seen through it.
        let bc = Vec3::new(c.x, c.y, back_z);
        m.disc(&mt(mat::BLACK, 0.02, 0.02, 0.02), bc + Vec3::Z * 0.004, Vec3::NEG_Z, Vec3::Y, r, 24);
        let fins = mt(mat::SATIN, 0.10, 0.10, 0.10);
        for k in -4..=4 {
            let y = k as f32 * 0.012;
            let hw = ((r - 0.008).powi(2) - y * y).max(0.0).sqrt();
            if hw < 0.01 {
                continue;
            }
            m.rbox(&fins, bc + Vec3::new(0.0, y, -0.012), Vec3::new(hw, 0.0016, 0.012), 0.001, Quat::IDENTITY);
        }
    }
}

/// The cooling air leaves under the cowl just ahead of the firewall, below a
/// flared lip.
fn cooling_exit(m: &mut Mesh) {
    let (z0, z1) = (-1.70, -1.535);
    let th0 = PI - 0.62;
    let lip = |z: f32, th: f32| -> Vec3 {
        let k = ((z - z0) / (z1 - z0)).clamp(0.0, 1.0);
        let fade = smoothstep(th0, th0 + 0.15, th);
        fuse_pt(z, th) + fuse_n(z, th) * (0.003 + 0.032 * k.powf(1.6) * fade)
    };
    let mut half = Mesh::default();
    half.patch(&WHITE.pat(pat::COWL), 10, 12, false, |u, v| {
        let z = z0 + (z1 - z0) * u;
        let th = th0 + (PI - th0) * v;
        (lip(z, th), [z, th], 0.0)
    });
    // The dark gap under its trailing edge.
    half.patch(&mt(mat::BLACK, 0.012, 0.012, 0.012), 1, 12, true, |u, v| {
        let th = th0 + (PI - th0) * v;
        let p = fuse_pt(z1, th).lerp(lip(z1, th), u);
        (p, [u, v], 0.0)
    });
    m.append(&half.mirror_x());
    m.append(&half);
}

fn fuselage(b: &mut Builder) {
    let mut zs = Vec::new();
    let mut z = NOSE_Z;
    while z < TAIL_Z - 0.005 {
        zs.push(z);
        z += if z < -2.25 { 0.012 } else if z > 4.1 { 0.02 } else { 0.03 };
    }
    zs.push(TAIL_Z);
    for split in [FIREWALL_Z, CANOPY[1] + 0.1] {
        let k = zs.iter().position(|&z| z > split).unwrap();
        zs[k - 1] = split;
    }
    const NT: usize = 64; // round one side, from the top (or the sill) to the keel
    let rows: Vec<Vec<SP>> = zs
        .iter()
        .map(|&z| {
            let s = sec(z);
            let th0 = skin_th0(z);
            let mut arc = 0.0;
            let mut prev = sec_pt(&s, th0);
            (0..=NT)
                .map(|k| {
                    let th = th0 + (PI - th0) * k as f32 / NT as f32;
                    let p = sec_pt(&s, th);
                    arc += (p - prev).length();
                    prev = p;
                    (Vec3::new(p.x, p.y, z), [z, arc], 0.0)
                })
                .collect()
        })
        .collect();
    let mut full = Mesh::default();
    full.grid(&WHITE, &rows, false, false);
    let nc = NT + 1;
    let row_of = |z: f32| zs.iter().position(|&q| (q - z).abs() < 1e-6).unwrap();
    let sub = |r0: usize, r1: usize, pattern: f32| -> Mesh {
        let mut m = Mesh::default();
        m.v = full.v[r0 * nc..(r1 + 1) * nc].to_vec();
        for v in &mut m.v {
            v.mat[1] = pattern;
        }
        for i in r0..r1 {
            for j in 0..nc - 1 {
                let a = ((i - r0) * nc + j) as u32;
                let c = a + nc as u32;
                m.i.extend([a, c, a + 1, a + 1, c, c + 1]);
            }
        }
        let mut l = m.mirror_x();
        for v in &mut l.v {
            v.uv[1] = -v.uv[1];
        }
        m.append(&l);
        m
    };
    let (rf, rt) = (row_of(FIREWALL_Z), row_of(CANOPY[1] + 0.1));
    let mut cowl = sub(0, rf, pat::COWL);
    // The engine face behind the spinner, and the two intakes either side of it.
    // (Filling the open front of the cowl, just inside it.)
    let s0 = sec(NOSE_Z + 0.02);
    let face: Vec<Vec3> = (0..48).map(|k| { let p = sec_pt(&s0, -(k as f32) / 48.0 * TAU); Vec3::new(p.x * 0.97, s0.mid + (p.y - s0.mid) * 0.97, NOSE_Z + 0.02) }).collect();
    cowl.fan(&mt(mat::BLACK, 0.03, 0.03, 0.03), Vec3::new(0.0, s0.mid, NOSE_Z + 0.02), &face, Vec3::NEG_Z);
    intakes(&mut cowl);
    cooling_exit(&mut cowl);
    // The rubber boot where the nose leg leaves the bottom of the cowl.
    let dir = (Vec3::new(0.0, -0.80, NOSE_AXLE.z) - Vec3::new(0.0, -0.52, -1.80)).normalize();
    let s = sec(-1.82);
    let at = Vec3::new(0.0, -0.52, -1.80) + dir * ((-0.52 - s.bot) / -dir.y - 0.025);
    cowl.lathe(&mt(mat::RUBBER, 0.02, 0.02, 0.022), at, dir, &[[0.075, 0.0], [0.07, 0.012], [0.058, 0.024], [0.052, 0.036], [0.045, 0.05], [0.042, 0.056], [0.0, 0.056]], 20, true);
    let mut cabin = sub(rf, rt, pat::FUSE);
    let mut tailcone = sub(rt, zs.len() - 1, pat::FUSE);
    let s = sec(TAIL_Z);
    let ring: Vec<Vec3> = (0..48).map(|k| { let p = sec_pt(&s, k as f32 / 48.0 * TAU); Vec3::new(p.x, p.y, TAIL_Z) }).collect();
    tailcone.fan(&WHITE.pat(pat::FUSE), Vec3::new(0.0, s.mid, TAIL_Z + 0.004), &ring, Vec3::Z);

    // Cowling fasteners along the split line.
    for sx in [-1.0f32, 1.0] {
        let mut z = -2.25;
        while z < FIREWALL_Z - 0.05 {
            let (p, n) = on_side(z, -0.12, sx, 0.0005);
            cowl.screw(&SCREW_STEEL, p, n, 0.006, z * 13.0 + sx);
            z += 0.11;
        }
    }
    // The wing-root fairings, where the centre section meets the fuselage sides.
    for sx in [-1.0f32, 1.0] {
        let mut f = Mesh::default();
        f.patch(&WHITE.pat(pat::FUSE), 18, 4, sx > 0.0, |a, c| {
            let z = -1.02 + 1.78 * a;
            let (p, n) = on_side(z, -0.62 + 0.0 * c, 1.0, 0.0);
            let bulge = 0.05 * (a * PI).sin() * (1.0 - c);
            let q = p + n * bulge + Vec3::new(0.0, -0.08 * c, 0.0);
            (Vec3::new(q.x * sx, q.y, q.z), [z, c], 0.0)
        });
        cabin.append(&f);
    }
    // The boarding step behind each wing root's trailing edge.
    for sx in [-1.0f32, 1.0] {
        let (p, n) = on_side(0.86, -0.52, sx, 0.0);
        let foot = Vec3::new(sx * 0.66, -0.78, 0.90);
        cabin.lathe(&PAINT_WHITE, p, n, &[[0.032, -0.004], [0.032, 0.004], [0.024, 0.008], [0.0, 0.008]], 14, false);
        cabin.tube(&PAINT_WHITE, &[p, p + n * 0.05 + Vec3::new(0.0, -0.04, 0.0), foot], &|_| 0.011, 10, true);
        cabin.rbox(&mt(mat::BLACK, 0.03, 0.03, 0.03).pat(pat::NONSLIP), foot + Vec3::new(sx * 0.02, 0.008, 0.0), Vec3::new(0.055, 0.007, 0.045), 0.004, Quat::IDENTITY);
    }
    // The tow release under the tail cone (it tows gliders).
    let s = sec(4.0);
    let p = Vec3::new(0.0, s.bot - 0.004, 4.0);
    tailcone.rbox(&mt(mat::SATIN, 0.05, 0.05, 0.055), p + Vec3::new(0.0, -0.016, 0.0), Vec3::new(0.018, 0.016, 0.045), 0.005, Quat::IDENTITY);
    let ring: Vec<Vec3> = (0..=16).map(|k| { let a = k as f32 / 16.0 * TAU; p + Vec3::new(0.0, -0.045 - 0.016 * a.cos(), 0.05 + 0.016 * a.sin()) }).collect();
    tailcone.tube(&STEEL, &ring, &|_| 0.004, 6, false);
    // Tie-down ring under the tail, the beacon on the spine behind the canopy.
    let s = sec(3.9);
    let p = Vec3::new(0.0, s.bot - 0.005, 3.9);
    let ring: Vec<Vec3> = (0..=16).map(|k| { let a = k as f32 / 16.0 * TAU; p + Vec3::new(0.0, -0.03 - 0.03 * a.cos(), 0.03 * a.sin()) }).collect();
    tailcone.tube(&STEEL, &ring, &|_| 0.005, 6, false);
    let s = sec(1.6);
    let top = Vec3::new(0.0, s.top, 1.6);
    tailcone.lathe(&BLACK, top, Vec3::Y, &[[0.022, 0.0], [0.022, 0.01], [0.0, 0.01]], 16, false);
    tailcone.lathe(&mt(mat::LAMP, 0.9, 0.06, 0.03).x(lamp::BEACON), top + Vec3::Y * 0.01, Vec3::Y, &[[0.018, 0.0], [0.018, 0.012], [0.013, 0.026], [0.0, 0.03]], 16, true);
    // A com whip on the spine and the transponder blade under the belly.
    let s = sec(2.3);
    let p = Vec3::new(0.0, s.top, 2.3);
    tailcone.tube(&BLACK, &[p, p + Vec3::new(0.0, 0.45, 0.25)], &|t| 0.005 - t * 0.003, 6, true);
    let s = sec(0.3);
    let p = Vec3::new(0.0, s.bot, 0.3);
    let blade = [Vec2::new(-0.03, 0.0), Vec2::new(0.035, 0.0), Vec2::new(0.02, -0.085), Vec2::new(0.0, -0.09)];
    cabin.slab(&BLACK, &blade, &[], p + Vec3::X * 0.003, Vec3::Z, Vec3::Y, 0.006);

    b.fixed("cowling", G_ENGINE, cowl);
    b.fixed("cabin skin", G_FUSE, cabin);
    let t = b.fixed("tail cone", G_TAIL, tailcone);
    b.role(t, Role::TailCone);
}

// ---------------------------------------------------------------- wings

/// Half span, and where the flat centre section ends and the outer panels'
/// dihedral begins.
/// (Measured along the panels: the tip is 4.36 m out, the span 8.72 m.)
pub const SPAN: f32 = 4.44;
pub const KINK: f32 = 1.95;
/// 14 degrees of dihedral on the outer panels.
const DIH: f32 = 0.2493;
pub const FLAP: [f32; 2] = [0.60, 1.90];
pub const AILERON: [f32; 2] = [2.10, 4.05];
/// The span the wing tank in each wing fills (the main tank is in the fuselage).
pub const TANK: [f32; 2] = [0.60, 1.60];
/// Leading edge and chord at the root, height of the chord plane.
const ROOT_LE: f32 = -1.00;
const ROOT_C: f32 = 1.71;
const ROOT_Y: f32 = -0.66;

/// Right wing section at span station xs (the outer panel's stations run
/// along its own dihedral, as the planform is laid out).
pub fn wing_sect(xs: f32) -> Sect {
    let out = (xs - KINK).max(0.0);
    let t = out / (SPAN - KINK);
    // Straight leading edge, the trailing edge tapering in on the outer panels.
    let mut le = ROOT_LE + 0.06 * t;
    let mut c = ROOT_C - 0.56 * t;
    let mut tf = 1.0 - 0.15 * t;
    if xs > SPAN - 0.18 {
        let u = ((xs - (SPAN - 0.18)) / 0.18).min(1.0);
        let k = 1.0 - (1.0 - u * u).max(0.0).sqrt();
        le += c * 0.35 * k;
        c -= c * 0.45 * k;
        tf *= (1.0 - u * u).max(0.0).sqrt();
    }
    let y = ROOT_Y + out * DIH.sin();
    let x = KINK.min(xs) + out * DIH.cos();
    let inc = (3.0 - 2.0 * t).to_radians();
    let d = if xs > KINK { DIH } else { 0.0 };
    let up = Vec3::new(-d.sin(), d.cos() * inc.cos(), inc.sin()).normalize();
    Sect { le: Vec3::new(x, y, le), c, cdir: Vec3::new(0.0, -inc.sin(), inc.cos()), up, tf }
}
pub fn wing_foil() -> Foil {
    Foil { at: Box::new(wing_sect), t: 0.15, camber: 0.02 }
}
/// Point and outward normal on a foil.
pub fn foil_frame(f: &Foil, s: f32, xc: f32, side: f32) -> (Vec3, Vec3) {
    let p = f.pt(s, xc, side, -1.0);
    let ds = f.pt(s + 0.002, xc, side, -1.0) - f.pt(s - 0.002, xc, side, -1.0);
    let dx = f.pt(s, xc + 0.002, side, -1.0) - f.pt(s, (xc - 0.002).max(0.0), side, -1.0);
    let n = (ds.cross(dx) * side).normalize();
    (p, n)
}

fn wings(b: &mut Builder) {
    let f = wing_foil();
    let skin = WHITE.pat(pat::WING);
    let ctrl = WHITE.pat(pat::CTRL);
    let mut w = Mesh::default();
    let one = |_: f32| 1.0;
    let zero = |_: f32| 0.0;
    // Hinge lines: flaps at 75% of the chord, ailerons at 76%.
    let fh = |_: f32| 0.75;
    let ah = |_: f32| 0.76;
    f.skin(&mut w, &skin, 0.0, FLAP[0], 6, &zero, &one, 28, false);
    f.skin(&mut w, &skin, FLAP[0], FLAP[1], 16, &zero, &fh, 28, false);
    f.skin(&mut w, &skin, FLAP[1], AILERON[0], 8, &zero, &one, 28, false);
    f.skin(&mut w, &skin, AILERON[0], AILERON[1], 22, &zero, &ah, 28, false);
    f.skin(&mut w, &skin, AILERON[1], SPAN, 14, &zero, &one, 28, false);
    f.web(&mut w, &PRIMER, FLAP[0], FLAP[1], 10, &fh);
    f.web(&mut w, &PRIMER, AILERON[0], AILERON[1], 12, &ah);
    f.cap(&mut w, &PRIMER, FLAP[0], 0.75, 1.0, false);
    f.cap(&mut w, &PRIMER, FLAP[1], 0.75, 1.0, false);
    f.cap(&mut w, &PRIMER, AILERON[0], 0.76, 1.0, false);
    f.cap(&mut w, &PRIMER, AILERON[1], 0.76, 1.0, false);
    // Fuel filler on the centre section, inspection plates under the wing.
    let (p, n) = foil_frame(&f, 1.2, 0.30, 1.0);
    w.lathe(&ALU, p, n, &[[0.045, -0.002], [0.045, 0.002], [0.038, 0.004], [0.0, 0.004]], 24, false);
    for &(s, xc) in &[(0.9f32, 0.40f32), (2.6, 0.40), (3.5, 0.38)] {
        let (p, n) = foil_frame(&f, s, xc, -1.0);
        inspection_plate(&mut w, p + n * 0.0012, n, Vec3::Z, 0.05, 6);
    }
    // Hinge brackets under the flaps and ailerons.
    let bracket = |w: &mut Mesh, s: f32, xc: f32| {
        let (p, n) = foil_frame(&f, s, xc, -1.0);
        let rot = Quat::from_rotation_arc(Vec3::Y, n);
        w.rbox(&PAINT_WHITE, p + n * 0.012, Vec3::new(0.0035, 0.016, 0.03), 0.002, rot);
        w.bolt(&STEEL, p + n * 0.02 + rot * Vec3::X * 0.0035, rot * Vec3::X, 0.003);
    };
    for s in [0.80f32, 1.25, 1.75] {
        bracket(&mut w, s, 0.75);
    }
    for s in [2.25f32, 3.05, 3.90] {
        bracket(&mut w, s, 0.76);
    }
    // The fairing strip over the joint where the outer panel meets the centre section.
    {
        let strip = WHITE.pat(pat::WING);
        let mut ring = Vec::new();
        for k in 0..=40 {
            let t = k as f32 / 40.0;
            let (side, xc) = if t < 0.5 { (1.0, 0.97 * (1.0 - 2.0 * t).powi(2)) } else { (-1.0, 0.97 * (2.0 * t - 1.0).powi(2)) };
            let (p, n) = foil_frame(&f, KINK, xc.max(0.002), side);
            ring.push(p + n * 0.0015);
        }
        w.tube(&strip, &ring, &|_| 0.004, 6, true);
    }
    // Fuel drain under the wing tank.
    let (p, n) = foil_frame(&f, 1.1, 0.45, -1.0);
    w.lathe(&ALU, p, n, &[[0.008, -0.002], [0.008, 0.006], [0.004, 0.009], [0.004, 0.016], [0.0, 0.016]], 10, false);
    // Nav light and strobe on the tip.
    let (p, _) = foil_frame(&f, SPAN - 0.03, 0.06, 0.0);
    w.lathe(&mt(mat::LAMP, 0.9, 0.9, 0.9).x(lamp::STROBE), p + Vec3::new(0.0, 0.0, 0.05), Vec3::X, &[[0.015, 0.0], [0.015, 0.014], [0.011, 0.028], [0.0, 0.032]], 14, true);
    w.lathe(&mt(mat::LAMP, 0.1, 0.9, 0.2).x(lamp::NAV_GREEN), p + Vec3::new(0.0, 0.0, -0.01), Vec3::new(0.6, 0.0, -0.8), &[[0.019, 0.0], [0.019, 0.011], [0.012, 0.026], [0.0, 0.03]], 16, true);
    // The landing light and the pitot, under the left wing (built on the right, moved).
    let mut left_extra = Mesh::default();
    // (on the camber line at the nose, facing ahead: side 0 has no surface
    // normal - foil_frame's would be NaN, which hid the lens and its light)
    let p = f.pt(1.5, 0.004, 0.0, -1.0);
    left_extra.disc(&mt(mat::LAMP, 0.95, 0.92, 0.85).x(lamp::LANDING), p - Vec3::Z * 0.006, Vec3::NEG_Z, Vec3::Y, 0.05, 20);
    // The stall warning vane in the leading edge.
    let (p, n) = foil_frame(&f, 0.95, 0.012, -1.0);
    let fwd = (Vec3::NEG_Z - n * Vec3::NEG_Z.dot(n)).normalize();
    let rot = Quat::from_mat3(&glam::Mat3::from_cols(Vec3::X, n, -fwd));
    left_extra.rbox(&ALU, p + n * 0.0008, Vec3::new(0.022, 0.0008, 0.016), 0.0006, rot);
    left_extra.rbox(&ALU, p + n * 0.003 + fwd * 0.012, Vec3::new(0.008, 0.0006, 0.009), 0.0004, rot);
    let (p, n) = foil_frame(&f, 3.2, 0.2, -1.0);
    let bend = p + n * 0.09;
    left_extra.tube(&STEEL, &[p, bend, bend - Vec3::Z * 0.17], &|t| if t > 0.9 { 0.006 } else { 0.0075 }, 10, true);
    let mut lw = w.mirror_x();
    lw.append(&left_extra.mirror_x());
    for v in &mut lw.v {
        if v.mat[0] == mat::LAMP && v.mat[3] == lamp::NAV_GREEN {
            v.mat[3] = lamp::NAV_RED;
            v.col = [0.95, 0.1, 0.05, v.col[3]];
        }
    }
    b.fixed("left wing", G_LWING, lw);
    b.fixed("right wing", G_RWING, w);
    let mut ws = Mesh::default();
    let mut ribs = Vec::new();
    let mut s = 0.45;
    while s < SPAN - 0.15 {
        let to = if (FLAP[0]..FLAP[1]).contains(&s) { 0.73 } else if (AILERON[0]..AILERON[1]).contains(&s) { 0.74 } else { 0.97 };
        ribs.push((s, 0.03, to));
        s += 0.29;
    }
    structure(&f, &mut ws, 0.4, SPAN - 0.1, &[0.25, 0.62], &ribs);
    let l = b.fixed("left wing structure", G_LWING, ws.mirror_x());
    let r = b.fixed("right wing structure", G_RWING, ws);
    b.role(l, Role::WingStructure(0)).role(r, Role::WingStructure(1));

    // Flaps: plain flaps on the centre section, hinged at their nose, down to 60°.
    let mut fl = Mesh::default();
    f.skin(&mut fl, &ctrl, FLAP[0] + 0.01, FLAP[1] - 0.01, 16, &|_| 0.755, &one, 14, true);
    f.cap(&mut fl, &ctrl, FLAP[0] + 0.01, 0.755, 1.0, true);
    f.cap(&mut fl, &ctrl, FLAP[1] - 0.01, 0.755, 1.0, true);
    let axis = (f.pt(FLAP[1], 0.76, 0.0, -1.0) - f.pt(FLAP[0], 0.76, 0.0, -1.0)).normalize();
    let pivot = f.pt((FLAP[0] + FLAP[1]) * 0.5, 0.76, 0.0, -1.0);
    let (l, r) = b.pair(["left flap", "right flap"], [G_LWING, G_RWING], fl, |_| Anim::hinge(Drive::new(Input::Flaps, 60f32.to_radians())), pivot, axis, [None, None]);
    b.role(l, Role::Flap(0)).role(r, Role::Flap(1));

    // Ailerons, on the outer panels (their hinge follows the dihedral).
    let mut al = Mesh::default();
    f.skin(&mut al, &ctrl, AILERON[0] + 0.01, AILERON[1] - 0.01, 20, &|_| 0.765, &one, 14, true);
    f.cap(&mut al, &ctrl, AILERON[0] + 0.01, 0.765, 1.0, true);
    f.cap(&mut al, &ctrl, AILERON[1] - 0.01, 0.765, 1.0, true);
    let axis = (f.pt(AILERON[1], 0.77, 0.0, -1.0) - f.pt(AILERON[0], 0.77, 0.0, -1.0)).normalize();
    let pivot = f.pt((AILERON[0] + AILERON[1]) * 0.5, 0.77, 0.0, -1.0);
    let (l, r) = b.pair(["left aileron", "right aileron"], [G_LWING, G_RWING], al, |s| Anim::hinge(Drive::new(Input::Aileron(s), 0.35)), pivot, axis, [None, None]);
    b.role(l, Role::Aileron(0)).role(r, Role::Aileron(1));
}

// ---------------------------------------------------------------- tail

/// The all-moving tailplane: half span, root and tip chords, its pivot.
const STAB_HALF: f32 = 1.60;
const STAB_Y: f32 = 0.06;
const STAB_PIVOT_Z: f32 = 3.78;
/// The anti-servo tab's hinge line, and where it stops either side of the fuselage.
const TAB_Z: f32 = 4.20;
const TAB_IN: f32 = 0.12;

fn tail(b: &mut Builder) {
    let stab = Foil {
        at: Box::new(|s: f32| {
            let a = s.abs();
            let mut le = 3.58 + a / STAB_HALF * 0.10;
            let te = 4.38 - a / STAB_HALF * 0.10;
            let mut c = te - le;
            let mut tf = 1.0;
            if a > STAB_HALF - 0.1 {
                let u = ((a - (STAB_HALF - 0.1)) / 0.1).min(1.0);
                let k = 1.0 - (1.0 - u * u).max(0.0).sqrt();
                le += c * 0.25 * k;
                c -= c * 0.25 * k;
                tf = (1.0 - u * u).max(0.0).sqrt();
            }
            Sect { le: Vec3::new(s, STAB_Y, le), c, cdir: Vec3::Z, up: Vec3::Y, tf }
        }),
        t: 0.10,
        camber: 0.0,
    };
    let tailc = WHITE.pat(pat::TAIL);
    let ctrl = WHITE.pat(pat::CTRL);
    // The stabilator: the whole surface turns about its pivot, with the
    // anti-servo tab across its trailing edge.
    let mut st = Mesh::default();
    // The tab's hinge runs straight across (the trailing edge is swept forward).
    let tab_x = |s: f32| {
        let k = (stab.at)(s);
        ((TAB_Z - k.le.z) / k.c).clamp(0.0, 0.95)
    };
    let one = |_: f32| 1.0;
    stab.skin(&mut st, &ctrl, -STAB_HALF, -TAB_IN, 26, &|_| 0.0, &tab_x, 22, false);
    stab.skin(&mut st, &ctrl, -TAB_IN, TAB_IN, 6, &|_| 0.0, &one, 22, false);
    stab.skin(&mut st, &ctrl, TAB_IN, STAB_HALF, 26, &|_| 0.0, &tab_x, 22, false);
    for (a, b) in [(-STAB_HALF + 0.12, -TAB_IN), (TAB_IN, STAB_HALF - 0.12)] {
        stab.web(&mut st, &PRIMER, a, b, 10, &tab_x);
    }
    for s in [-STAB_HALF, STAB_HALF] {
        stab.cap(&mut st, &ctrl, s, 0.0, 1.0, false);
    }
    // (Outboard of the tab the stabilator runs on to its trailing edge.)
    stab.skin(&mut st, &ctrl, -STAB_HALF, -STAB_HALF + 0.12, 4, &tab_x, &one, 8, true);
    stab.skin(&mut st, &ctrl, STAB_HALF - 0.12, STAB_HALF, 4, &tab_x, &one, 8, true);
    for s in [-TAB_IN, TAB_IN] {
        stab.cap(&mut st, &ctrl, s, tab_x(s), 1.0, false);
    }
    // The anti-servo tab: it moves the same way as the trailing edge, and the
    // trim sets it.
    let mut tab = Mesh::default();
    let tx = |s: f32| tab_x(s) + 0.006;
    for (a, b) in [(-STAB_HALF + 0.125, -TAB_IN - 0.005), (TAB_IN + 0.005, STAB_HALF - 0.125)] {
        stab.skin(&mut tab, &ctrl, a, b, 18, &tx, &one, 10, true);
        stab.cap(&mut tab, &ctrl, a, tx(a), 1.0, true);
        stab.cap(&mut tab, &ctrl, b, tx(b), 1.0, true);
        // Its control horn and push rod fitting, under the inboard end.
        let s = if a < 0.0 { b - 0.06 } else { a + 0.06 };
        let p = stab.pt(s, tx(s) + 0.03, -1.0, -1.0);
        tab.rbox(&PAINT_WHITE, p + Vec3::new(0.0, -0.018, -0.005), Vec3::new(0.003, 0.02, 0.018), 0.002, Quat::IDENTITY);
    }
    // Its torque tube through the fuselage, and the hinge fittings.
    st.tube(&STEEL, &[Vec3::new(-0.2, STAB_Y, STAB_PIVOT_Z), Vec3::new(0.2, STAB_Y, STAB_PIVOT_Z)], &|_| 0.02, 12, true);
    let k = b.add("stabilator", G_TAIL, st, Anim::hinge2(Drive::new(Input::Elevator, -0.30), Drive::new(Input::Trim, -0.4).offset(0.02)), Vec3::new(0.0, STAB_Y, STAB_PIVOT_Z), Vec3::X, None);
    b.role(k, Role::Elevator);
    b.add("anti-servo tab", G_TAIL, tab, Anim::hinge2(Drive::new(Input::Elevator, -0.25), Drive::new(Input::Trim, 0.35)), Vec3::new(0.0, STAB_Y, TAB_Z), Vec3::X, Some(k));

    // Fin, swept back, with its rudder.
    let fin = Foil {
        at: Box::new(|s: f32| {
            let u = s / 0.97;
            let mut le = 3.15 + 0.62 * u;
            let te = 4.34 - 0.04 * u;
            let mut c = te - le;
            let mut tf = 1.0;
            if s > 0.90 {
                let v = ((s - 0.90) / 0.07).min(1.0);
                let k = 1.0 - (1.0 - v * v).max(0.0).sqrt();
                le += c * 0.15 * k;
                c -= c * 0.15 * k;
                tf = (1.0 - v * v).max(0.05).sqrt();
            }
            Sect { le: Vec3::new(0.0, 0.06 + s, le), c, cdir: Vec3::Z, up: Vec3::X, tf }
        }),
        t: 0.10,
        camber: 0.0,
    };
    let rh = |s: f32| {
        let k = (fin.at)(s);
        (4.00 + 0.02 * s - k.le.z) / k.c
    };
    let mut m = Mesh::default();
    fin.skin(&mut m, &tailc, 0.0, 0.97, 30, &|_| 0.0, &|s| rh(s) - 0.004, 22, false);
    fin.web(&mut m, &PRIMER, 0.0, 0.97, 16, &|s| rh(s) - 0.004);
    // The fin's root fairing into the spine, the white tail light.
    let base = fin.pt(0.0, 0.0, 0.0, -1.0);
    m.tube(&tailc, &[base + Vec3::new(0.0, -0.01, -0.35), base + Vec3::new(0.0, 0.0, 0.02)], &|t| 0.03 * (t * PI).sin() + 0.002, 10, true);
    m.lathe(&mt(mat::LAMP, 0.95, 0.95, 0.95).x(lamp::TAIL), Vec3::new(0.0, 0.04, TAIL_Z + 0.004), Vec3::Z, &[[0.012, 0.0], [0.012, 0.01], [0.0, 0.016]], 12, true);
    let mut ru = Mesh::default();
    fin.skin(&mut ru, &ctrl, -0.05, 0.96, 30, &|s| rh(s) + 0.004, &|_| 1.0, 12, true);
    fin.cap(&mut ru, &ctrl, -0.05, rh(-0.05) + 0.004, 1.0, true);
    let rax = Vec3::new(0.0, 0.97, 0.02).normalize();
    let mut ts = Mesh::default();
    structure(&stab, &mut ts, -1.5, 1.5, &[0.22, 0.6], &[(-1.1, 0.03, 0.9), (-0.6, 0.03, 0.9), (0.6, 0.03, 0.9), (1.1, 0.03, 0.9)]);
    structure(&fin, &mut ts, 0.05, 0.9, &[0.3, 0.6], &[(0.3, 0.03, 0.72), (0.65, 0.03, 0.72)]);
    let k = b.fixed("tail structure", G_TAIL, ts);
    b.role(k, Role::TailStructure);
    let k = b.fixed("fin", G_TAIL, m);
    b.role(k, Role::Stab);
    let k = b.add("rudder", G_TAIL, ru, Anim::hinge(Drive::new(Input::Rudder, 0.45)), Vec3::new(0.0, 0.06, 4.00), rax, None);
    b.role(k, Role::Rudder);
}

// ---------------------------------------------------------------- engine, propeller

fn engine(b: &mut Builder) {
    let mut m = Mesh::default();
    // Two exhaust stubs out of the bottom of the cowl.
    for sx in [-1.0f32, 1.0] {
        let pts = [Vec3::new(0.12 * sx, -0.45, -1.80), Vec3::new(0.14 * sx, -0.60, -1.68), Vec3::new(0.15 * sx, -0.66, -1.58)];
        m.tube(&mt(mat::EXHAUST, 0.3, 0.28, 0.26), &pts, &|_| 0.026, 14, false);
        m.tube(&mt(mat::BLACK, 0.01, 0.01, 0.01), &[pts[2] - (pts[2] - pts[1]).normalize() * 0.04, pts[2] + (pts[2] - pts[1]).normalize() * 0.001], &|_| 0.025, 14, true);
    }
    b.fixed("exhaust", G_ENGINE, m);

    // Spinner and two blades.
    let mut sp = Mesh::default();
    // A long pointed spinner (a tangent ogive, the point just rounded off).
    let (sr, sl) = (0.165f32, 0.275f32);
    let rho = (sr * sr + sl * sl) / (2.0 * sr);
    let mut prof: Vec<[f32; 2]> = (0..=22).map(|k| { let h = sl * (1.0 - (1.0 - k as f32 / 22.0).powf(1.4)); [((rho * rho - h * h).max(0.0).sqrt() + sr - rho).max(0.0), h] }).collect();
    let n = prof.len();
    prof[n - 2][0] = prof[n - 2][0].max(0.006);
    prof[n - 1] = [0.0, sl + 0.002];
    let base = Vec3::new(0.0, PROP_HUB.y, NOSE_Z - 0.01);
    sp.lathe(&WHITE, base, Vec3::NEG_Z, &prof, 40, true);
    sp.lathe(&GREY, base, Vec3::NEG_Z, &[[0.172, -0.014], [0.172, 0.0], [0.0, 0.0]], 40, false);
    // The screws holding the dome to its back plate.
    for k in 0..6 {
        let a = (k as f32 + 0.5) / 6.0 * TAU;
        let r = 0.157;
        let h = 0.035;
        let p = base + Vec3::new(a.cos() * r, a.sin() * r, -h);
        let nrm = Vec3::new(a.cos(), a.sin(), -0.45).normalize();
        sp.screw(&SCREW_PAINT, p, nrm, 0.0045, k as f32);
    }
    let blade_m = mt(mat::PROP, 0.5, 0.5, 0.5);
    let r_tip = 0.915;
    let mut blades = Mesh::default();
    for k in 0..2 {
        let mut bl = Mesh::default();
        let blade = Foil {
            at: Box::new(move |r: f32| {
                let beta = (1.50 / (TAU * r.max(0.1))).atan() + 0.035;
                let shank = smoothstep(0.22, 0.11, r);
                let mut c = 0.15 - 0.10 * ((r - 0.52) / 0.40).powi(2);
                c = c * (1.0 - shank) + 0.05 * shank;
                let mut tf = (0.18 - 0.11 * (r - 0.20) / 0.72) / 0.10;
                tf = tf * (1.0 - shank) + 9.0 * shank;
                if r > r_tip - 0.035 {
                    let u = ((r - (r_tip - 0.035)) / 0.035).min(1.0);
                    let q = (1.0 - u * u).max(0.0).sqrt();
                    c *= 0.4 + 0.6 * q;
                    tf *= q.max(0.05);
                }
                let t = Vec3::X;
                let cdir = -t * beta.cos() + Vec3::Z * beta.sin();
                let up = Vec3::NEG_Z * beta.cos() - t * beta.sin();
                Sect { le: PROP_HUB + Vec3::Y * r - cdir * (0.35 * c) - up * (0.03 * c), c, cdir, up, tf }
            }),
            t: 0.10,
            camber: 0.035,
        };
        blade.skin(&mut bl, &blade_m, 0.10, r_tip, 34, &|_| 0.0, &|_| 1.0, 14, false);
        bl.transform(0, Mat4::from_translation(PROP_HUB) * Mat4::from_rotation_z(k as f32 * PI) * Mat4::from_translation(-PROP_HUB));
        blades.append(&bl);
    }
    let spin = Anim::hinge(Drive::new(Input::Prop, -1.0));
    let k = b.add("spinner", G_PROP, sp, spin, PROP_HUB, Vec3::Z, None);
    b.role(k, Role::Prop);
    let prop = b.add("blades", G_PROP, blades, spin, PROP_HUB, Vec3::Z, None);
    b.role(prop, Role::Prop);
    b.pieces[prop].vis = Vis::SlowProp;
    b.pieces[prop].occluder = false;
    let mut disc = Mesh::default();
    disc.disc(&mt(mat::PROPDISC, 0.1, 0.1, 0.1), PROP_HUB + Vec3::Z * 0.005, Vec3::Z, Vec3::Y, r_tip, 64);
    let d = b.add("prop disc", G_PROP, disc, spin, PROP_HUB, Vec3::Z, None);
    b.role(d, Role::Prop);
    b.pieces[d].vis = Vis::FastProp;
    b.pieces[d].glass = true;
    b.pieces[d].occluder = false;
}

// ---------------------------------------------------------------- landing gear

/// A wheel fairing (spat): a teardrop shell round a wheel, open underneath
/// for the tyre, `l` long, `w` wide, `h` tall.
fn spat(m: &mut Mesh, axle: Vec3, l: f32, w: f32, h: f32) {
    let mat = WHITE.pat(pat::COWL);
    m.patch(&mat, 24, 20, true, |a, c| {
        // a: round the shell from the slot's right edge over the top to its left; c: nose to tail.
        // (Down to below the axle on both sides: only the bottom of the tyre shows.)
        let th = -0.72 * PI + a * 1.44 * PI;
        let zt = -0.42 + c * 1.0;
        // Teardrop: blunt nose, long tail.
        let r = if zt < 0.0 { (1.0 - (zt / 0.42).powi(2)).max(0.0).sqrt() } else { (1.0 - (zt / 0.58).powf(1.6)).max(0.0).powf(0.8) };
        let p = Vec3::new(w * 0.5 * r * th.sin(), h * 0.5 * r * th.cos(), zt * l);
        (axle + p - Vec3::new(0.0, 0.01, 0.0), [a, c], 0.0)
    });
}

/// A streamlined fairing round a gear leg from `top` to `bot`, `c` long and
/// `t` thick, the leg a third of the way back from its nose.
fn leg_fairing(m: &mut Mesh, top: Vec3, bot: Vec3, c: f32, t: f32) {
    let mat = WHITE.pat(pat::COWL);
    m.patch(&mat, 28, 6, false, |u, v| {
        // u round the section from the trailing edge over the outside, back by the inside.
        let side = if u < 0.5 { 1.0 } else { -1.0 };
        let w = if u < 0.5 { 1.0 - 2.0 * u } else { 2.0 * u - 1.0 };
        let x = 1.0 - (w * PI * 0.5).cos();
        let yt = 5.0 * 0.2 * (0.2969 * x.sqrt() - 0.126 * x - 0.3516 * x * x + 0.2843 * x.powi(3) - 0.1036 * x.powi(4));
        let p = top.lerp(bot, v);
        (p + Vec3::new(side * yt * c * (t / (0.2 * c)), 0.0, (x - 0.33) * c), [u, v], 0.0)
    });
}

fn gear(b: &mut Builder) {
    // Main legs: a strut down from the wing to an oleo on the axle, and the fairing.
    let mut leg = Mesh::default();
    let knee = MAIN_AXLE + Vec3::new(-0.08, 0.30, -0.02);
    leg.tube(&PAINT_WHITE, &[MAIN_ROOT, knee], &|t| 0.04 - 0.008 * t, 14, true);
    leg.tube(&CHROME, &[knee, MAIN_AXLE + Vec3::new(-0.08, 0.06, 0.0)], &|_| 0.026, 12, true);
    leg.rbox(&PAINT_WHITE, MAIN_AXLE + Vec3::new(-0.08, 0.03, 0.0), Vec3::new(0.018, 0.04, 0.03), 0.01, Quat::IDENTITY);
    // The leg's streamlined fairing, from the wing down into the wheel fairing.
    leg_fairing(&mut leg, MAIN_AXLE + Vec3::new(-0.075, 0.38, -0.02), MAIN_AXLE + Vec3::new(-0.075, 0.12, -0.02), 0.20, 0.055);
    // The brake line down its back.
    let bl = MAIN_AXLE + Vec3::new(-0.06, 0.0, 0.0);
    leg.tube(&BLACK, &[MAIN_AXLE + Vec3::new(-0.075, 0.36, 0.10), MAIN_AXLE + Vec3::new(-0.075, 0.16, 0.095), bl + Vec3::new(0.0, 0.10, 0.07)], &|_| 0.0045, 6, false);
    let (ll, lr) = b.pair(["left main leg", "right main leg"], [G_GEAR, G_GEAR], leg, |s| Anim::slide(Drive::new(Input::Compress(if s < 0.0 { 1 } else { 2 }), 1.0), Vec3::Y), MAIN_AXLE, Vec3::Y, [None, None]);
    b.role(ll, Role::Gear(1)).role(lr, Role::Gear(2));
    let mut wh = Mesh::default();
    wheel(&mut wh, MAIN_AXLE, Vec3::X, MAIN_R, 0.065, 0.075, true);
    let (l, r) = b.pair(["left main wheel", "right main wheel"], [G_GEAR, G_GEAR], wh, |s| Anim::hinge(Drive::new(Input::Wheel(if s < 0.0 { 1 } else { 2 }), -1.0)), MAIN_AXLE, Vec3::X, [Some(ll), Some(lr)]);
    b.role(l, Role::Gear(1)).role(r, Role::Gear(2));
    let mut sp = Mesh::default();
    spat(&mut sp, MAIN_AXLE, 0.82, 0.24, 0.42);
    let (l, r) = b.pair(["left wheel fairing", "right wheel fairing"], [G_GEAR, G_GEAR], sp, |_| Anim::Fixed, MAIN_AXLE, Vec3::X, [Some(ll), Some(lr)]);
    b.role(l, Role::Gear(1)).role(r, Role::Gear(2));

    // Nose leg: an oleo that steers, from the engine mount under the cowl.
    let top = Vec3::new(0.0, -0.52, -1.80);
    let mut nl = Mesh::default();
    nl.tube(&PAINT_WHITE, &[top, Vec3::new(0.0, -0.80, NOSE_AXLE.z)], &|t| 0.036 - 0.004 * t, 16, true);
    nl.tube(&CHROME, &[Vec3::new(0.0, -0.78, NOSE_AXLE.z), Vec3::new(0.0, NOSE_AXLE.y + 0.06, NOSE_AXLE.z)], &|_| 0.026, 12, true);
    for sx in [-1.0f32, 1.0] {
        let a = Vec3::new(sx * 0.07, NOSE_AXLE.y + 0.10, NOSE_AXLE.z);
        nl.rbox(&PAINT_WHITE, (a + NOSE_AXLE + Vec3::X * sx * 0.07) * 0.5, Vec3::new(0.007, 0.06, 0.022), 0.006, Quat::IDENTITY);
    }
    let fork = b.add("nose leg", G_GEAR, nl, Anim::turn_slide(Drive::new(Input::Steer, 1.0), Drive::new(Input::Compress(0), 1.0), Vec3::Y), Vec3::new(0.0, NOSE_AXLE.y, NOSE_AXLE.z), Vec3::Y, None);
    b.role(fork, Role::Gear(0));
    let mut nw = Mesh::default();
    wheel(&mut nw, NOSE_AXLE, Vec3::X, NOSE_R, 0.055, 0.06, false);
    let k = b.add("nose wheel", G_GEAR, nw, Anim::hinge(Drive::new(Input::Wheel(0), -1.0)), NOSE_AXLE, Vec3::X, Some(fork));
    b.role(k, Role::Gear(0));
    let mut ns = Mesh::default();
    spat(&mut ns, NOSE_AXLE, 0.66, 0.20, 0.40);
    let k = b.add("nose fairing", G_GEAR, ns, Anim::Fixed, NOSE_AXLE, Vec3::X, Some(fork));
    b.role(k, Role::Gear(0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_are_the_dr400s() {
        let m = build();
        let names: Vec<&str> = m.pieces.iter().map(|p| p.name).collect();
        for n in ["cowling", "cabin skin", "tail cone", "left wing", "right wing", "left flap", "right flap", "left aileron", "right aileron", "stabilator", "fin", "rudder", "spinner", "blades", "nose leg", "nose wheel", "left main wheel", "right main wheel", "canopy"] {
            assert!(names.contains(&n), "no {n}");
        }
        let (c, h) = m.bounds();
        let (len, span, tall) = (2.0 * h.z, 2.0 * h.x, c.y + h.y - GROUND_Y);
        println!("DR 400: {len:.2} m long, {span:.2} m across, {tall:.2} m tall");
        assert!((len - 6.96).abs() < 0.12, "length {len}");
        assert!((span - 8.72).abs() < 0.12, "span {span}");
        assert!((tall - 2.23).abs() < 0.12, "height {tall}");
        // The propeller clears the ground, and the tyres are on it.
        assert!(PROP_HUB.y - 0.915 > GROUND_Y + 0.2);
        assert!((MAIN_AXLE.y - MAIN_R - GROUND_Y).abs() < 1e-6);
    }
}
