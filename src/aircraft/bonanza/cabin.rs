//! The Bonanza's cabin: a 1990s A36 (E-2111 and after) as most were
//! equipped before glass panels, with the "steam gauge" instrument panel,
//! the Beech power quadrant and the single throw-over control wheel.
//!
//! # What is here, and where it comes from
//!
//! - [S] Shop manual (see `mod.rs`): the parking brake is a pull control
//!   that only holds a valve (set it, then press the pedals: Section 2,
//!   "Parking Brake"); the elevator trim has a manual wheel and an electric
//!   trim switch on the pilot's control wheel with an enable bar and a trim
//!   master switch (Section 4, "Electric Elevator Trim Ground Checks"); the
//!   single (throw-over) control column (Section 4, Figure 4-1); separate
//!   battery and alternator switches (Section 14); the fuel selector valve
//!   on the floor at the inboard left wing root, under a moulded cover
//!   (Section 2 servicing table, Section 9 "Fuel Selector Valve Removal");
//!   cowl flaps worked from a knob in the panel (Section 6); the alternate
//!   air T-handle on the lower centre panel (Section 6); the landing gear
//!   motor breaker in the left subpanel and the flap position indicator in
//!   the panel (Sections 4 and 5).
//! - [W] Wikipedia and Aviation Consumer ("Beech 36, A36 Bonanza",
//!   <https://aviationconsumer.com/?p=1984>): from 1984 an all-new panel,
//!   the conventional throttle-propeller-mixture power quadrant and the
//!   relocated gear switch (left of the quadrant, the flap switch on its
//!   right).
//! - [B] Black Square / Just Flight, "Bonanza Professional User Guide"
//!   (2025), a study simulation of a mid-2000s A36 that keeps the 1984
//!   panel's arrangement
//!   <https://downloads.justflight.com/products/JFF004080/BKSQ_BonanzaProfessionalManual.pdf>:
//!   the annunciator lamps on the pilot's side under the glareshield; the
//!   engine gauges as a column of round dials in the main panel; the fuel
//!   quantity gauges, then the volt and ammeters, on the centre subpanel
//!   behind (over) the power quadrant; the main circuit breaker panel on the
//!   cockpit sidewall left of the pilot's seat and the avionics breakers
//!   under the copilot's subpanel; the cabin climate controls on the pilot's
//!   subpanel; the panel light rheostats on the copilot's.
//! - Positions of the instruments, switch rows, radio stack, breakers and
//!   the seats: estimates from photographs of 1984-2004 A36 panels in sale
//!   listings and owner forums against [B] (no surveyed drawing to hand), to
//!   within a few centimetres; the instrument sizes are the standard 3 1/8
//!   in (79 mm) and 2 1/4 in (57 mm) faces, the radio stack 6.25 in wide.
//!   The avionics are a generic period fit (audio panel, GPS-com, nav-com,
//!   transponder, autopilot), not a particular aircraft's; the copilot's
//!   side is blanked off, as on many A36s without dual instruments.
//! - Cabin trim: moulded surrounds round the windows and over the slim
//!   windshield posts and header, vinyl side panels with cloth inserts,
//!   piping and cup-washer screws, padded visors on a rod under the
//!   headliner: a period Beech interior as photographed, colours estimated.
//!
//! The airframe's wing skin runs in through the cabin sides from 0.25 m
//! out (see `mod.rs`), so the floor rises over it from the front spar to
//! the rear one (`floor_y`) and the seats sit on that.
//!
//! # What the mouse works (each a moving piece, driven by the simulation)
//!
//! Throttle, propeller and mixture levers; the gear switch; the flap switch
//! (UP / APPR / DN); the parking brake; the magneto and start key (click
//! for the magnetos, hold for the starter); the battery master; the landing
//! light switch; the fuel selector (LEFT / RIGHT / OFF); the trim wheel and
//! the electric trim switch on the control wheel; the cowl flap knob; the
//! pilot's toe brakes; the OBS, ADF, heading indicator and altimeter knobs
//! and the nav receiver's tuning knob (fixed knobs).
//!
//! # Decorative or simplified
//!
//! The alternator half of the split master moves with the battery half
//! (the simulator has one master switch). The auxiliary fuel pump, taxi,
//! navigation, beacon, strobe, pitot heat, avionics and trim master
//! switches, the annunciator test button, all the breakers (the left
//! sidewall panel, the avionics row and the gear motor's), the panel light
//! rheostats, the cabin air, heat and defrost push-pulls, the radios' other
//! knobs and buttons, the alternate air handle, the friction lock, the
//! copilot's pedals' brakes, the door handle and latches, the visors and the
//! seats do not work. The magnetos are OFF / BOTH / START only. The fuel
//! flow shows on its own gauge (the factory panel pairs it with manifold
//! pressure on one dial), and the volts share the ammeter's dial.
use super::*;
use crate::aircraft::Hotspot;
use glam::{Mat3, Quat, Vec2, Vec3};

/// The instrument panel's face (s 76.5) and the cabin floor (body frame).
pub const PANEL_Z: f32 = -0.70;
pub const FLOOR_Y: f32 = -0.46;
/// The glareshield's top over the panel.
pub const GLARESHIELD_Y: f32 = 0.375;
/// The pilot's eye, left front seat: about s 103, 0.66 m over the cushion
/// (which sits on the floor raised over the wing; see the module notes).
pub const EYE: Vec3 = Vec3::new(-0.27, 0.53, -0.02);
/// The throttle lever's pivot in the quadrant, and the gear switch's.
pub const THROTTLE_PIVOT: Vec3 = Vec3::new(0.0, -0.20, PANEL_Z + 0.02);
pub const GEAR_PIVOT: Vec3 = Vec3::new(-0.135, -0.075, PANEL_Z);
/// The panel's lower edge outboard of the centre section: about knee
/// height, so the shins pass under it to the pedals and the pedals show
/// under it from the seat.
const PANEL_BOT: f32 = -0.10;
/// The centre section under the radio stack, dropping to the power
/// quadrant with the gear switch on its left and the flap switch on its
/// right: x0, x1, its bottom.
const CENTRE: [f32; 3] = [-0.175, 0.23, -0.225];
/// Where the control column comes out of the panel, and the wheel's hub.
const COLUMN: Vec3 = Vec3::new(-0.05, -0.04, PANEL_Z);
const HUB: Vec3 = Vec3::new(-0.27, 0.06, -0.47);
/// The pedestal under the quadrant: centre x, half width, face z.
const PED: [f32; 3] = [0.045, 0.06, PANEL_Z + 0.055];
/// The fuel selector on the floor by the left wall, ahead of the seat.
const SELECTOR: Vec3 = Vec3::new(-0.43, FLOOR_Y + 0.075, -0.62);

const PANEL_COL: Mat = mt(mat::PLASTIC, 0.045, 0.046, 0.05);
const LOWER_COL: Mat = mt(mat::PLASTIC, 0.10, 0.10, 0.105);
const KNOB: Mat = mt(mat::PLASTIC, 0.02, 0.02, 0.022);
const RED: Mat = mt(mat::PLASTIC, 0.38, 0.025, 0.02);
const BLUE: Mat = mt(mat::PLASTIC, 0.02, 0.05, 0.25);
const WHITE: Mat = mt(mat::PLASTIC, 0.70, 0.70, 0.68);
/// Tan leather bolsters and cloth inserts, a period Beech interior.
const LEATHER: Mat = mt(mat::PLASTIC, 0.26, 0.19, 0.12);
const CLOTH: Mat = mt(mat::FABRIC, 0.22, 0.17, 0.115);
const CARPET: Mat = mt(mat::FABRIC, 0.075, 0.065, 0.055);
const LINING: Mat = mt(mat::PLASTIC, 0.55, 0.52, 0.47);
const STRAP: Mat = mt(mat::FABRIC, 0.05, 0.05, 0.055);
const FACE: Mat = mt(mat::GAUGE, 0.02, 0.02, 0.02);

/// Instrument ids (`c172::cabin::gauge`, plane.wgsl `gauge`); 34 is the
/// Bonanza's fuel flow.
mod g {
    pub use crate::aircraft::c172::cabin::gauge::*;
    pub const FUEL_FLOW: f32 = 34.0;
}

/// (instrument, x, y, face radius) on the panel. The Beech arrangement
/// (see the module notes): the pilot's flight group with the clock and
/// suction outboard and the CDI and ADF inboard; the radio stack in the
/// middle; the engine gauges in columns right of the stack (manifold
/// pressure, tachometer and fuel flow, then the oil, cylinder head and
/// exhaust temperatures); the fuel quantity gauges and the ammeter on the
/// centre subpanel over the power quadrant; the outside air temperature on
/// the right panel, which is otherwise blanked off.
const GAUGES: [(f32, f32, f32, f32); 21] = [
    (g::ASI, -0.365, 0.250, 0.040),
    (g::AI, -0.270, 0.250, 0.040),
    (g::ALT, -0.175, 0.250, 0.040),
    (g::TC, -0.365, 0.155, 0.040),
    (g::HI, -0.270, 0.155, 0.040),
    (g::VSI, -0.175, 0.155, 0.040),
    (g::CLOCK, -0.465, 0.255, 0.026),
    (g::SUCTION, -0.465, 0.175, 0.022),
    (g::CDI, -0.075, 0.250, 0.040),
    (g::ADF, -0.075, 0.155, 0.040),
    (g::MP, 0.215, 0.295, 0.030),
    (g::TACH, 0.215, 0.222, 0.030),
    (g::FUEL_FLOW, 0.215, 0.152, 0.026),
    (g::OIL_T, 0.287, 0.300, 0.022),
    (g::OIL_P, 0.287, 0.243, 0.022),
    (g::CHT, 0.287, 0.186, 0.022),
    (g::EGT, 0.287, 0.129, 0.022),
    (g::OAT, 0.480, 0.195, 0.020),
    (g::FUEL_L, 0.035, 0.035, 0.022),
    (g::FUEL_R, 0.105, 0.035, 0.022),
    (g::AMPS, 0.070, -0.030, 0.018),
];
/// The radio stack's opening: x0, x1, y0, y1.
const STACK: [f32; 4] = [0.0, 0.16, 0.085, 0.335];

pub fn build(b: &mut Builder) {
    let mut m = Mesh::default();
    lining(&mut m);
    floor(&mut m);
    panel(&mut m, b);
    glareshield(&mut m);
    quadrant(&mut m);
    pedestal(&mut m);
    sides(&mut m);
    b.fixed("cabin", G_FUSE, m);
    let mut s = Mesh::default();
    seats(&mut s);
    b.fixed("seats", G_FUSE, s);
    controls(b);
    switches(b);
    knobs(b);
}

// ---------------------------------------------------------------- helpers

/// Half width of the lining at station z, height y.
pub fn wall_x(z: f32, y: f32) -> f32 {
    let s = sec(z);
    0.955 * fuse_half(z, s.mid + (y - s.mid) / 0.96)
}

/// A small printed legend (white on the panel's grey); `text` must be in
/// `b747::deck::STRINGS`. (Do not add strings to that table for this
/// panel: it is a const array in the shader, and growing it took the
/// NVIDIA driver's compile from under a minute to over six.)
fn label(m: &mut Mesh, c: Vec3, ax: Vec3, ay: Vec3, hx: f32, hy: f32, text: &str) {
    use crate::aircraft::b747::deck;
    let mm = PANEL_COL.pat(pat::LEGEND).x((16 * deck::sid(text)) as f32);
    deck::rect(m, &mm, c, ax, ay, hx, hy);
}
/// A legend on the panel face.
fn plabel(m: &mut Mesh, x: f32, y: f32, hx: f32, hy: f32, text: &str) {
    label(m, Vec3::new(x, y, PANEL_Z + 0.0012), Vec3::X, Vec3::Y, hx, hy, text);
}
fn circle(c: Vec2, r: f32, n: usize) -> Vec<Vec2> {
    (0..n).map(|k| { let a = -(k as f32) / n as f32 * TAU; c + Vec2::new(a.cos(), a.sin()) * r }).collect()
}
/// Strap or bar between two points, `w` wide, lying flat against `n`.
fn strap(m: &mut Mesh, mm: &Mat, a: Vec3, c: Vec3, n: Vec3, w: f32, t: f32) {
    let d = (c - a).normalize();
    let x = n.cross(d).normalize();
    let rot = Quat::from_mat3(&Mat3::from_cols(x, d, x.cross(d)));
    m.rbox(mm, (a + c) * 0.5, Vec3::new(w * 0.5, (c - a).length() * 0.5, t * 0.5), t * 0.45, rot);
}

/// The cabin floor's height at station z: flat in the footwells, then over
/// the wing's skin (outboard is highest, with the dihedral) from just
/// ahead of the front spar to the rear spar, and never under the belly skin.
pub fn floor_y(z: f32) -> f32 {
    static T: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    const Z0: f32 = FIREWALL_Z;
    const STEP: f32 = 0.01;
    let t = T.get_or_init(|| {
        let f = wing_foil();
        let k = wing_sect(0.58);
        let le = k.le.z;
        let over = |z: f32| if z > le + 0.002 && z < le + k.c - 0.002 { wing_top(&f, 0.58, z).y + 0.025 } else { FLOOR_Y };
        let n = ((TAILCONE_Z - Z0) / STEP) as usize + 2;
        (0..n)
            .map(|i| {
                let z = Z0 + i as f32 * STEP;
                // A short ramp up to the front spar's top.
                let ramp = if z > le - 0.07 && z <= le + 0.002 { FLOOR_Y + (over(le + 0.004) - FLOOR_Y) * smoothstep(le - 0.07, le + 0.002, z) } else { FLOOR_Y };
                over(z).max(ramp).max(FLOOR_Y).max(sec(z).bot + 0.05)
            })
            .collect()
    });
    let u = ((z - Z0) / STEP).clamp(0.0, (t.len() - 2) as f32);
    let i = u.floor() as usize;
    t[i] + (t[i + 1] - t[i]) * (u - i as f32)
}

// ---------------------------------------------------------------- shell

/// The lining: the skin's section a little inside it, from the firewall to
/// the baggage bulkhead; leather-grain vinyl low on the walls, cloth
/// headliner above. The windows are cut out of it in the shader.
fn lining(m: &mut Mesh) {
    let (z0, z1) = (FIREWALL_Z + 0.01, TAILCONE_Z - 0.10);
    let nz = 90;
    let nt = 40;
    let rows: Vec<Vec<SP>> = (0..=nz)
        .map(|i| {
            let z = z0 + (z1 - z0) * i as f32 / nz as f32;
            let s = sec(z);
            (0..=nt)
                .map(|k| {
                    let th = k as f32 / nt as f32 * PI;
                    let p = sec_pt(&s, th);
                    let q = Vec2::new(p.x * 0.955, s.mid + (p.y - s.mid) * 0.96);
                    (Vec3::new(q.x, q.y, z), [z, th], 0.0)
                })
                .collect()
        })
        .collect();
    let mut half = Mesh::default();
    half.grid(&LINING.pat(pat::LINING), &rows, false, true);
    for v in &mut half.v {
        let y = v.pos[1];
        let c = if y > 0.62 {
            v.mat[0] = mat::FABRIC;
            Vec3::new(0.58, 0.56, 0.52)
        } else if y < 0.0 {
            Vec3::new(0.20, 0.15, 0.10)
        } else {
            Vec3::new(0.42, 0.37, 0.30)
        };
        v.col = [c.x, c.y, c.z, 1.0];
    }
    m.append(&half.mirror_x());
    m.append(&half);
    // The baggage bulkhead at the back.
    let s = sec(z1);
    let ring: Vec<Vec3> = (0..48).map(|k| { let p = sec_pt(&s, k as f32 / 48.0 * TAU); Vec3::new(p.x * 0.95, s.mid + (p.y - s.mid) * 0.95, z1) }).collect();
    m.fan(&LINING, Vec3::new(0.0, s.mid, z1), &ring, Vec3::NEG_Z);
    // The firewall's cabin side under the glareshield: an insulating blanket.
    let fz = FIREWALL_Z + 0.015;
    m.patch(&mt(mat::SATIN, 0.10, 0.10, 0.10), 8, 8, false, |a, c| {
        let y = FLOOR_Y - 0.01 + (0.33 - FLOOR_Y) * c;
        let x = (2.0 * a - 1.0) * (wall_x(fz, y) - 0.003);
        (Vec3::new(x, y, fz), [x, y], 0.0)
    });
}

/// The carpeted floor, wall to wall, with heel pads in the front footwells.
fn floor(m: &mut Mesh) {
    let (z0, z1) = (FIREWALL_Z + 0.02, TAILCONE_Z - 0.11);
    let carpet = CARPET.pat(pat::CARPET);
    m.patch(&carpet, 200, 10, false, |a, c| {
        let z = z0 + (z1 - z0) * a;
        let y = floor_y(z);
        let x = (2.0 * c - 1.0) * (wall_x(z, y) + 0.004);
        (Vec3::new(x, y, z), [z, x], 0.0)
    });
    for xs in [-0.27f32, 0.27] {
        m.rbox(&RUBBER, Vec3::new(xs, FLOOR_Y + 0.004, -0.78), Vec3::new(0.12, 0.004, 0.12), 0.003, Quat::IDENTITY);
    }
    // The seat rails on the floor over the spars.
    for xs in [-0.27f32, 0.27] {
        for dx in [-0.16f32, 0.16] {
            let rail: Vec<Vec3> = (0..=12).map(|k| { let z = -0.36 + 0.5 * k as f32 / 12.0; Vec3::new(xs + dx, floor_y(z) + 0.006, z) }).collect();
            m.tube(&ALU.pat(pat::KNURL), &rail, &|_| 0.008, 6, true);
        }
    }
}

// ---------------------------------------------------------------- panel

/// The panel's outline: the side edges follow the lining, the centre
/// section drops lower for the quadrant, its corners chamfered.
fn panel_outline() -> Vec<Vec2> {
    let side = |y: f32| wall_x(PANEL_Z, y) - 0.006;
    let [x0, x1, yb] = CENTRE;
    let mut o = vec![
        Vec2::new(-side(PANEL_BOT), PANEL_BOT),
        Vec2::new(x0 - 0.018, PANEL_BOT),
        Vec2::new(x0, PANEL_BOT - 0.018),
        Vec2::new(x0, yb + 0.014),
        Vec2::new(x0 + 0.014, yb),
        Vec2::new(x1 - 0.014, yb),
        Vec2::new(x1, yb + 0.014),
        Vec2::new(x1, PANEL_BOT - 0.018),
        Vec2::new(x1 + 0.018, PANEL_BOT),
    ];
    let top = GLARESHIELD_Y - 0.03;
    for k in 0..=12 {
        let y = PANEL_BOT + (top - PANEL_BOT) * k as f32 / 12.0;
        o.push(Vec2::new(side(y), y));
    }
    for k in (1..=12).rev() {
        let y = PANEL_BOT + (top - PANEL_BOT) * k as f32 / 12.0;
        o.push(Vec2::new(-side(y), y));
    }
    o
}

/// A toggle switch in its bezel on the panel (fixed: these do not work).
fn toggle(m: &mut Mesh, x: f32, y: f32, up: bool) {
    let f = PANEL_Z + 0.0005;
    m.lathe(&CHROME, Vec3::new(x, y, f), Vec3::Z, &[[0.0058, 0.0], [0.0058, 0.0025], [0.0042, 0.0045], [0.0032, 0.0045]], 14, false);
    let a = Vec3::new(x, y, f + 0.004);
    let tip = a + Vec3::new(0.0, if up { 0.0085 } else { -0.0085 }, 0.011);
    m.tube(&CHROME, &[a, tip], &|t| 0.0021 - 0.0006 * t, 8, false);
    m.dome(&CHROME, tip, (tip - a).normalize(), 0.0026, 0.002, 8, 0.0);
}
/// A push-pull control: chrome shaft, a round knob with a flat face.
fn push_pull(m: &mut Mesh, c: Vec3, knob: &Mat) {
    m.lathe(&CHROME, c, Vec3::Z, &[[0.0075, 0.0], [0.0075, 0.002], [0.0035, 0.003], [0.0035, 0.012]], 14, false);
    m.lathe(knob, c + Vec3::Z * 0.012, Vec3::Z, &[[0.0035, 0.0], [0.010, 0.002], [0.0105, 0.010], [0.0085, 0.013], [0.0, 0.013]], 18, true);
}

fn panel(m: &mut Mesh, b: &mut Builder) {
    let z = PANEL_Z;
    let f = z + 0.0005;
    let [x0, x1, yb] = CENTRE;
    let mut holes: Vec<Vec<Vec2>> = GAUGES.iter().filter(|g| g.3 > 0.0).map(|g| circle(Vec2::new(g.1, g.2), g.3 + 0.001, 32)).collect();
    holes.push(vec![Vec2::new(STACK[0], STACK[2]), Vec2::new(STACK[0], STACK[3]), Vec2::new(STACK[1], STACK[3]), Vec2::new(STACK[1], STACK[2])]);
    m.slab(&PANEL_COL.pat(pat::PANEL), &panel_outline(), &holes, Vec3::new(0.0, 0.0, z), Vec3::X, Vec3::Y, 0.012);
    // The subpanels, a lighter grey under the seam: left and right at the
    // switch and breaker rows, the centre one round the quadrant up to the
    // radio stack.
    let side = |y: f32| wall_x(PANEL_Z, y) - 0.008;
    let sub = LOWER_COL.pat(pat::PANEL);
    let seam = |m: &mut Mesh, a: f32, c: f32, y: f32| strap(m, &BLACK, Vec3::new(a, y, f + 0.0008), Vec3::new(c, y, f + 0.0008), Vec3::Z, 0.004, 0.0008);
    for (a, c) in [(-side(0.03), x0 - 0.004), (x1 + 0.004, side(0.03))] {
        let (a0, c0) = (a.max(-side(PANEL_BOT)), c.min(side(PANEL_BOT)));
        let lo = if a < 0.0 { [Vec2::new(a0, PANEL_BOT + 0.003), Vec2::new(c - 0.018, PANEL_BOT + 0.003), Vec2::new(c, PANEL_BOT - 0.012)] } else { [Vec2::new(a, PANEL_BOT - 0.012), Vec2::new(a + 0.018, PANEL_BOT + 0.003), Vec2::new(c0, PANEL_BOT + 0.003)] };
        let o = [lo[0], lo[1], lo[2], Vec2::new(c, 0.028), Vec2::new(a, 0.028)];
        m.slab(&sub, &o, &[], Vec3::new(0.0, 0.0, z + 0.0006), Vec3::X, Vec3::Y, 0.0);
        seam(m, a, c, 0.031);
    }
    let o = [Vec2::new(x0 + 0.004, yb + 0.016), Vec2::new(x0 + 0.016, yb + 0.004), Vec2::new(x1 - 0.016, yb + 0.004), Vec2::new(x1 - 0.004, yb + 0.016), Vec2::new(x1 - 0.004, 0.072), Vec2::new(x0 + 0.004, 0.072)];
    let sub_holes: Vec<Vec<Vec2>> = GAUGES.iter().filter(|g| g.2 < 0.07 && g.1 > x0 && g.1 < x1).map(|g| circle(Vec2::new(g.1, g.2), g.3 + 0.001, 32)).collect();
    m.slab(&sub, &o, &sub_holes, Vec3::new(0.0, 0.0, z + 0.0006), Vec3::X, Vec3::Y, 0.0);
    seam(m, x0, x1, 0.075);
    for x in [x0 - 0.002, x1 + 0.002] {
        strap(m, &BLACK, Vec3::new(x, PANEL_BOT - 0.012, f + 0.0008), Vec3::new(x, 0.075, f + 0.0008), Vec3::Z, 0.004, 0.0008);
    }

    // Instruments: the recessed face, its bezel and four screws.
    for (k, &(id, x, y, r)) in GAUGES.iter().enumerate() {
        if r == 0.0 {
            continue;
        }
        let c = Vec3::new(x, y, z - 0.010);
        m.disc(&FACE.x(id), c, Vec3::Z, Vec3::Y, r, 40);
        m.lathe(&BLACK, c, Vec3::Z, &[[r, 0.0], [r, 0.0095], [r + 0.0035, 0.0125], [r + 0.0065, 0.0112], [r + 0.0068, 0.0098]], 40, false);
        let q = r * 1.06;
        for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            m.screw(&SCREW_BLACK, Vec3::new(x + dx * q, y + dy * q, f), Vec3::Z, if r > 0.03 { 0.0028 } else { 0.0022 }, k as f32 * 4.0 + dx + dy * 2.0);
        }
    }
    // Legends: the temperature column's to its right, the fuel gauges' under
    // them (the big dials carry their own).
    for &(x, y, w, text) in &[(0.329f32, 0.300f32, 0.011f32, "OIL T"), (0.329, 0.243, 0.011, "OIL P"), (0.329, 0.186, 0.011, "TEMP"), (0.329, 0.129, 0.009, "EGT"), (0.035, 0.004, 0.010, "LEFT"), (0.105, 0.004, 0.011, "RIGHT"), (0.07, 0.066, 0.010, "FUEL"), (0.07, -0.0555, 0.007, "AMP")] {
        plabel(m, x, y, w, 0.0035, text);
    }
    // The radio stack: audio panel, GPS-com, nav-com, transponder, autopilot.
    let units: [(f32, f32, f32); 5] = [(0.335, 0.305, -1.0), (0.304, 0.236, 3.0), (0.235, 0.190, 0.0), (0.189, 0.142, 2.0), (0.141, 0.087, 1.0)];
    for (k, &(y1, y0, disp)) in units.iter().enumerate() {
        let (cx, cy) = ((STACK[0] + STACK[1]) * 0.5, (y0 + y1) * 0.5);
        let (hx, hy) = ((STACK[1] - STACK[0]) * 0.5 - 0.001, (y1 - y0) * 0.5 - 0.0012);
        m.rbox(&mt(mat::PLASTIC, 0.04, 0.041, 0.045), Vec3::new(cx, cy, z - 0.003), Vec3::new(hx, hy, 0.008), 0.0025, Quat::IDENTITY);
        let fz = z + 0.0051;
        if disp >= 0.0 {
            let (dw, dh) = if disp == 3.0 { (0.042, 0.026) } else { (0.046, 0.0085) };
            let dx = cx - 0.012;
            m.rbox(&BLACK, Vec3::new(dx, cy + 0.003, fz - 0.0005), Vec3::new(dw + 0.003, dh + 0.003, 0.001), 0.001, Quat::IDENTITY);
            crate::aircraft::c172::cabin::rect(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(disp), Vec3::new(dx, cy + 0.003, fz + 0.0007), Vec3::X, Vec3::Y, dw, dh);
            // Concentric knobs on the right (the nav-com's is a moving
            // target of its own: see `knobs`), a volume knob on the left.
            if disp != 0.0 {
                let kn = Vec3::new(STACK[1] - 0.014, cy - hy * 0.25, fz);
                m.lathe(&KNOB.pat(pat::KNURL), kn, Vec3::Z, &[[0.010, 0.0], [0.010, 0.008], [0.0088, 0.009], [0.0, 0.009]], 20, false);
                m.lathe(&KNOB.pat(pat::KNURL), kn + Vec3::Z * 0.009, Vec3::Z, &[[0.006, 0.0], [0.006, 0.008], [0.005, 0.009], [0.0, 0.009]], 16, false);
            }
            m.lathe(&KNOB, Vec3::new(STACK[0] + 0.010, cy + hy * 0.35, fz), Vec3::Z, &[[0.0055, 0.0], [0.0055, 0.006], [0.0, 0.007]], 14, false);
        }
        // Push buttons along the bottom (the audio panel: one row, with lamps).
        let nb = if disp == -1.0 { 9 } else if disp == 3.0 { 6 } else { 4 };
        for i in 0..nb {
            let (bx, by) = if disp == -1.0 { (STACK[0] + 0.012 + i as f32 * 0.0155, cy - 0.002) } else { (cx - 0.05 + i as f32 * 0.018, y0 + 0.0072) };
            m.rbox(&mt(mat::PLASTIC, 0.09, 0.09, 0.095).pat(pat::PLACARD), Vec3::new(bx, by, fz + 0.002), Vec3::new(0.0055, 0.0035, 0.0025), 0.0012, Quat::IDENTITY);
            if disp == -1.0 && i % 2 == 0 {
                m.disc(&mt(mat::LAMP, 0.2, 0.9, 0.3).x(lamp::ANNUNC), Vec3::new(bx, by + 0.0068, fz + 0.0005), Vec3::Z, Vec3::Y, 0.0012, 8);
            }
        }
        for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, 1.0)] {
            m.screw(&SCREW_BLACK, Vec3::new(cx + dx * (hx - 0.004), cy + dy * (hy - 0.003), fz), Vec3::Z, 0.0016, k as f32 * 3.0 + dx);
        }
    }
    // The annunciator panel on the pilot's side, just under the glareshield:
    // gear warning, low volts, starter engaged, door, suction, alternator
    // out, trim, and the test button at its left end.
    let ay = 0.322;
    m.rbox(&BLACK, Vec3::new(-0.28, ay, f + 0.003), Vec3::new(0.158, 0.0105, 0.003), 0.002, Quat::IDENTITY);
    for i in 0..8 {
        let x = -0.41 + i as f32 * 0.0375;
        // (unlit lenses: dark red, amber and green)
        let col = if i < 2 { mt(mat::PLASTIC, 0.13, 0.015, 0.01) } else if i == 5 { mt(mat::PLASTIC, 0.015, 0.075, 0.025) } else { mt(mat::PLASTIC, 0.14, 0.085, 0.01) };
        m.rbox(&col, Vec3::new(x, ay, f + 0.0062), Vec3::new(0.0155, 0.0068, 0.0012), 0.001, Quat::IDENTITY);
    }
    m.lathe(&mt(mat::PLASTIC, 0.08, 0.08, 0.085), Vec3::new(-0.465, 0.105, f), Vec3::Z, &[[0.0065, 0.0], [0.0065, 0.004], [0.0045, 0.006], [0.0, 0.006]], 14, false);
    plabel(m, -0.465, 0.091, 0.007, 0.003, "TEST");
    plabel(m, -0.465, 0.119, 0.016, 0.003, "ANN LIGHTS");

    // The left subpanel: the magneto and start switch at its outboard end,
    // the red split master (battery and alternator), the circuit-breaker
    // toggle switches for the fuel pump, the lights and pitot heat on a
    // black strip, and under them the parking brake and the cabin air, heat
    // and defroster push-pulls. (The battery master and the landing lights
    // move: see `switches`.)
    m.rbox(&BLACK, Vec3::new((switch_x(0) + switch_x(SWITCHES.len() - 1)) * 0.5, SWITCH_Y + 0.001, f + 0.0012), Vec3::new((switch_x(SWITCHES.len() - 1) - switch_x(0)) * 0.5 + 0.016, 0.0155, 0.0012), 0.003, Quat::IDENTITY);
    for (i, text) in SWITCHES.iter().enumerate() {
        let x = switch_x(i);
        m.rbox(&mt(mat::PLASTIC, 0.06, 0.06, 0.065), Vec3::new(x, SWITCH_Y, f + 0.003), Vec3::new(0.0085, 0.013, 0.0015), 0.0015, Quat::IDENTITY);
        if i != 0 && i != 1 && i != 3 {
            toggle(m, x, SWITCH_Y, matches!(i, 2 | 5));
        }
        label(m, Vec3::new(x, SWITCH_Y - 0.0195, f + 0.0025), Vec3::X, Vec3::Y, 0.0118, 0.0032, text);
    }
    plabel(m, (switch_x(0) + switch_x(1)) * 0.5, SWITCH_Y + 0.021, 0.016, 0.0034, "MASTER");
    // The magneto and start switch: the plate round the key.
    let kc = KEY;
    m.lathe(&CHROME, kc, Vec3::Z, &[[0.016, 0.0], [0.016, 0.004], [0.012, 0.007], [0.0, 0.007]], 24, false);
    m.disc(&mt(mat::PLASTIC, 0.06, 0.06, 0.065), kc - Vec3::Z * 0.0002 + Vec3::Z * 0.0008, Vec3::Z, Vec3::Y, 0.03, 32);
    for (a, text, w) in [(0.0f32, "OFF", 0.007f32), (-0.27, "R", 0.0025), (-0.53, "L", 0.0025), (-0.80, "BOTH", 0.008), (-1.20, "START", 0.009)] {
        let d = Vec2::new(-a.sin(), a.cos()) * 0.036;
        plabel(m, kc.x + d.x, kc.y + d.y, w, 0.003, text);
    }
    plabel(m, kc.x, kc.y - 0.040, 0.016, 0.0035, "IGNITION");
    // The parking brake's legend; the cabin air, heat and defrost knobs.
    plabel(m, PARK_BRAKE.x, PARK_BRAKE.y + 0.019, 0.02, 0.0036, "PARK BRAKE");
    for (x, text, w) in [(-0.335f32, "CABIN", 0.011f32), (-0.285, "TEMP", 0.009), (-0.235, "DEFOG", 0.011)] {
        push_pull(m, Vec3::new(x, PARK_BRAKE.y, f), &KNOB);
        plabel(m, x, PARK_BRAKE.y + 0.019, w, 0.0034, text);
    }

    // The centre section: gear position lights over the gear switch, and its legends.
    for (k, (x, l)) in [(GEAR_PIVOT.x - 0.022, lamp::GEAR_DOWN), (GEAR_PIVOT.x + 0.022, lamp::GEAR_UP)].iter().enumerate() {
        let col = if k == 0 { mt(mat::LAMP, 0.1, 0.9, 0.15) } else { mt(mat::LAMP, 0.95, 0.5, 0.05) };
        m.lathe(&CHROME, Vec3::new(*x, GEAR_PIVOT.y + 0.045, f), Vec3::Z, &[[0.008, 0.0], [0.008, 0.004], [0.0065, 0.006]], 16, false);
        m.dome(&col.x(*l), Vec3::new(*x, GEAR_PIVOT.y + 0.045, f + 0.005), Vec3::Z, 0.006, 0.0045, 14, 0.0);
    }
    m.rbox(&BLACK, GEAR_PIVOT + Vec3::Z * 0.001, Vec3::new(0.012, 0.03, 0.002), 0.002, Quat::IDENTITY);
    plabel(m, GEAR_PIVOT.x, GEAR_PIVOT.y + 0.026, 0.006, 0.0035, "UP");
    plabel(m, GEAR_PIVOT.x, GEAR_PIVOT.y - 0.038, 0.006, 0.0035, "DN");
    plabel(m, GEAR_PIVOT.x - 0.024, GEAR_PIVOT.y, 0.008, 0.0035, "GEAR");
    // The landing gear motor's pull breaker beside it (shop manual, Section 4).
    let gb = Vec3::new(GEAR_PIVOT.x - 0.022, GEAR_PIVOT.y - 0.066, f);
    m.lathe(&BLACK, gb, Vec3::Z, &[[0.0075, 0.0], [0.0075, 0.002], [0.0055, 0.003], [0.0055, 0.010], [0.0045, 0.012], [0.0, 0.012]], 14, false);
    plabel(m, gb.x + 0.024, gb.y, 0.013, 0.0032, "LDG GEAR");
    // The flap switch's slot and the flap position indicator beside it.
    m.rbox(&BLACK, FLAP_PIVOT + Vec3::Z * 0.001, Vec3::new(0.010, 0.03, 0.002), 0.002, Quat::IDENTITY);
    plabel(m, FLAP_PIVOT.x, FLAP_PIVOT.y + 0.042, 0.011, 0.0035, "FLAPS");
    let ind = Vec3::new(FLAP_PIVOT.x + 0.040, FLAP_PIVOT.y, f);
    m.rbox(&BLACK, ind, Vec3::new(0.012, 0.034, 0.0015), 0.002, Quat::IDENTITY);
    for (dy, text, w) in [(0.026f32, "UP", 0.004f32), (0.0, "APPR", 0.006), (-0.026, "DN", 0.004)] {
        label(m, ind + Vec3::new(0.004, dy, 0.0017), Vec3::X, Vec3::Y, w, 0.003, text);
    }
    // The cowl flaps' legend and the alternate air T-handle (lower centre).
    plabel(m, COWL.x, COWL.y + 0.021, 0.02, 0.0036, "COWL FLAPS");
    plabel(m, COWL.x, COWL.y - 0.019, 0.007, 0.003, "PULL");
    let aa = Vec3::new(-0.115, -0.172, z);
    m.tube(&CHROME, &[aa - Vec3::Z * 0.01, aa + Vec3::Z * 0.016], &|_| 0.003, 8, false);
    m.rbox(&mt(mat::PLASTIC, 0.03, 0.03, 0.032), aa + Vec3::Z * 0.019, Vec3::new(0.018, 0.0055, 0.0055), 0.004, Quat::IDENTITY);
    plabel(m, aa.x, aa.y + 0.017, 0.009, 0.0036, "ALTN");

    // The right subpanel: the avionics bus breakers, the avionics master and
    // the trim master, and the panel light rheostats under them.
    const CB: [&str; 8] = ["COMM 1", "NAV 1", "COMM 2", "ADF", "XPDR", "AUDIO", "AP", "RADIO"];
    for (i, text) in CB.iter().enumerate() {
        let p = Vec3::new(0.262 + i as f32 * 0.027, 0.008, f);
        m.lathe(&BLACK, p, Vec3::Z, &[[0.0065, 0.0], [0.0065, 0.002], [0.0048, 0.003], [0.0048, 0.009], [0.004, 0.0105], [0.0, 0.0105]], 12, false);
        m.disc(&PAINT_WHITE, p + Vec3::Z * 0.0106, Vec3::Z, Vec3::Y, 0.0022, 8);
        plabel(m, p.x, p.y - 0.0145, 0.0112, 0.0031, text);
    }
    for (x, text, up) in [(0.482f32, "RADIO", true), (0.512, "PITCH TRIM", true)] {
        m.rbox(&mt(mat::PLASTIC, 0.06, 0.06, 0.065), Vec3::new(x, 0.008, f + 0.003), Vec3::new(0.0085, 0.013, 0.0015), 0.0015, Quat::IDENTITY);
        toggle(m, x, 0.008, up);
        plabel(m, x, -0.0115, 0.0118, 0.0031, text);
    }
    for (i, text) in ["PANEL", "FLOOD", "DOME", "LIGHTS"].iter().enumerate() {
        let p = Vec3::new(0.29 + i as f32 * 0.05, -0.068, f);
        m.lathe(&CHROME, p, Vec3::Z, &[[0.009, 0.0], [0.009, 0.0015], [0.004, 0.0025]], 14, false);
        m.lathe(&KNOB.pat(pat::KNURL), p + Vec3::Z * 0.002, Vec3::Z, &[[0.0085, 0.0], [0.0085, 0.009], [0.0072, 0.011], [0.0, 0.011]], 16, false);
        m.rbox(&PAINT_WHITE, p + Vec3::new(0.0, 0.0055, 0.0132), Vec3::new(0.0008, 0.0025, 0.0003), 0.0002, Quat::IDENTITY);
        plabel(m, p.x, p.y + 0.0185, 0.011, 0.0032, text);
    }

    // The right panel, blanked off where a copilot's instruments would go:
    // two blanking plates, the hour meter, the operating limitations placard.
    for y in [0.250f32, 0.155] {
        let c = Vec3::new(0.400, y, f);
        m.lathe(&mt(mat::PLASTIC, 0.05, 0.051, 0.055), c, Vec3::Z, &[[0.046, 0.0], [0.046, 0.0012], [0.044, 0.002], [0.0, 0.002]], 32, false);
        for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            m.screw(&SCREW_BLACK, c + Vec3::new(dx * 0.0425, dy * 0.0425, 0.002), Vec3::Z, 0.0026, y * 40.0 + dx + dy * 2.0);
        }
    }
    m.rbox(&BLACK, Vec3::new(0.48, 0.255, f + 0.002), Vec3::new(0.022, 0.009, 0.003), 0.002, Quat::IDENTITY);
    crate::aircraft::c172::cabin::rect(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(5.0), Vec3::new(0.48, 0.255, f + 0.0055), Vec3::X, Vec3::Y, 0.018, 0.006);
    let plate = mt(mat::PLASTIC, 0.16, 0.16, 0.15).pat(pat::PLACARD);
    m.rbox(&plate, Vec3::new(0.43, 0.085, f), Vec3::new(0.06, 0.014, 0.0005), 0.0003, Quat::IDENTITY);
    m.rbox(&plate, Vec3::new(-0.27, 0.085, f), Vec3::new(0.05, 0.007, 0.0005), 0.0003, Quat::IDENTITY);
    m.rbox(&plate, Vec3::new(-0.075, 0.085, f), Vec3::new(0.025, 0.005, 0.0005), 0.0003, Quat::IDENTITY);
    // The screws round the panel and its removable subpanels.
    for (x, y) in [(-0.50f32, 0.33f32), (0.50, 0.33), (-0.50, PANEL_BOT + 0.012), (0.50, PANEL_BOT + 0.012), (-0.03, 0.33), (0.19, 0.33), (x0 + 0.014, yb + 0.022), (x1 - 0.014, yb + 0.022), (x0 + 0.012, 0.062), (x1 - 0.012, 0.062), (-0.30, 0.020), (0.38, 0.020), (-0.20, PANEL_BOT + 0.010), (0.30, PANEL_BOT + 0.010)] {
        m.screw(&SCREW_BLACK, Vec3::new(x, y, f), Vec3::Z, 0.0026, x * 11.0 + y);
    }
    // The compass on the glareshield's centre, under the windshield.
    let c = Vec3::new(0.0, GLARESHIELD_Y + 0.04, PANEL_Z - 0.22);
    m.rbox(&BLACK, c - Vec3::Y * 0.03, Vec3::new(0.02, 0.012, 0.025), 0.006, Quat::IDENTITY);
    m.rbox(&BLACK, c, Vec3::new(0.034, 0.028, 0.03), 0.011, Quat::IDENTITY);
    crate::aircraft::c172::cabin::rect(m, &FACE.x(g::COMPASS), c + Vec3::Z * 0.0302, Vec3::X, Vec3::Y, 0.025, 0.016);
    let _ = b;
}

/// The left subpanel's switches, left to right, the x of each and their height.
const SWITCHES: [&str; 9] = ["BAT", "ALT", "FUEL PUMP", "LANDING", "TAXI", "NAV", "BEACON", "STROBE", "PITOT HEAT"];
const SWITCH_Y: f32 = 0.004;
fn switch_x(i: usize) -> f32 {
    // (the split master's two halves side by side)
    if i < 2 { -0.425 + i as f32 * 0.018 } else { -0.425 + 0.018 + (i - 1) as f32 * 0.027 }
}
const KEY: Vec3 = Vec3::new(-0.488, -0.034, PANEL_Z + 0.0005);
const PARK_BRAKE: Vec3 = Vec3::new(-0.395, -0.071, PANEL_Z);
const FLAP_PIVOT: Vec3 = Vec3::new(0.165, -0.075, PANEL_Z);
const COWL: Vec3 = Vec3::new(0.200, -0.165, PANEL_Z);

/// The glareshield: a padded brow along the panel's top edge, then a shelf
/// forward to the windshield's foot, kept inside the skin.
fn glareshield(m: &mut Mesh) {
    let z = PANEL_Z;
    let gs = mt(mat::FABRIC, 0.022, 0.022, 0.024);
    let top = GLARESHIELD_Y - 0.03;
    let hw = |y: f32| wall_x(z, y) - 0.01;
    let brow: Vec<Vec3> = (0..=24).map(|k| { let x = (k as f32 / 12.0 - 1.0) * hw(top); Vec3::new(x, top + 0.012, z + 0.012) }).collect();
    m.tube(&mt(mat::PLASTIC, 0.03, 0.03, 0.032), &brow, &|_| 0.016, 12, true);
    m.patch(&gs, 30, 12, false, |a, c| {
        let zz = z + 0.02 - c * (z + 0.02 - (FIREWALL_Z + 0.06));
        let x = (2.0 * a - 1.0) * hw(top);
        let y = GLARESHIELD_Y - 0.002 - 0.012 * c;
        // Under the cowl deck and the windshield, inside the walls.
        let y = y.min(fuse_top(zz, x) - 0.02);
        let xc = x.signum() * x.abs().min(wall_x(zz, y) - 0.012);
        let y = y.min(fuse_top(zz, xc) - 0.02);
        (Vec3::new(xc, y, zz), [x, c], 0.0)
    });
    // The face under the brow, down to the panel (hides the panel's back).
    m.patch(&gs, 24, 2, false, |a, c| {
        let x = (2.0 * a - 1.0) * hw(top);
        (Vec3::new(x, top + c * (GLARESHIELD_Y - top), z - 0.002), [x, c], 0.0)
    });
}

/// The power quadrant's housing under the stack, its slots and legends.
fn quadrant(m: &mut Mesh) {
    let q = THROTTLE_PIVOT;
    let (x0, x1) = (q.x - 0.025, q.x + 0.115);
    let c = Vec3::new((x0 + x1) * 0.5, -0.19, PANEL_Z + 0.02);
    m.rbox(&PANEL_COL.pat(pat::PANEL), c, Vec3::new((x1 - x0) * 0.5, 0.032, 0.022), 0.006, Quat::IDENTITY);
    for k in 0..3 {
        let x = q.x + k as f32 * 0.045;
        m.rbox(&mt(mat::BLACK, 0.004, 0.004, 0.004), Vec3::new(x, -0.1575, PANEL_Z + 0.02), Vec3::new(0.004, 0.0012, 0.019), 0.001, Quat::IDENTITY);
    }
    let fz = PANEL_Z + 0.0425;
    label(m, Vec3::new(q.x + 0.045, -0.175, fz), Vec3::X, Vec3::Y, 0.05, 0.0045, "THR PROP MIX");
    // The friction lock knob on the housing's face.
    m.lathe(&KNOB.pat(pat::KNURL), Vec3::new(c.x, -0.205, fz - 0.001), Vec3::Z, &[[0.008, 0.0], [0.008, 0.008], [0.006, 0.010], [0.0, 0.010]], 16, false);
}

/// The pedestal under the quadrant: the trim wheel's slot in its left side
/// with the trim indicator on its face.
fn pedestal(m: &mut Mesh) {
    let [cx, hw, fz] = PED;
    let top = -0.21;
    let o = [Vec2::new(PANEL_Z - 0.01, top), Vec2::new(fz, top), Vec2::new(fz + 0.03, FLOOR_Y - 0.01), Vec2::new(PANEL_Z - 0.01, FLOOR_Y - 0.01)];
    m.slab(&LOWER_COL.pat(pat::PANEL), &o.map(|p| Vec2::new(p.x, p.y)), &[], Vec3::new(cx - hw, 0.0, 0.0), Vec3::Z, Vec3::Y, 2.0 * hw);
    // The trim wheel's slot and the indicator scale (nose down at the top).
    let wc = TRIM_WHEEL;
    m.rbox(&BLACK, Vec3::new(cx - hw - 0.0005, wc.y, wc.z), Vec3::new(0.001, 0.066, 0.012), 0.001, Quat::IDENTITY);
    let face = |y: f32| fz + (y - top) / (FLOOR_Y - 0.01 - top) * 0.03;
    let up = Vec3::new(0.0, top - FLOOR_Y + 0.01, -0.03).normalize();
    let n = Vec3::X.cross(up);
    let sc = Vec3::new(cx - 0.02, wc.y, face(wc.y) + 0.0008);
    crate::aircraft::c172::cabin::rect(m, &FACE.x(g::TRIM_SCALE), sc, Vec3::X, up, 0.010, 0.05);
    label(m, sc + up * 0.06, Vec3::X, up, 0.008, 0.0035, "TRIM");
    label(m, sc + up * 0.042 + Vec3::X * 0.022, Vec3::X, up, 0.005, 0.003, "ND");
    label(m, sc - up * 0.042 + Vec3::X * 0.022, Vec3::X, up, 0.005, 0.003, "NU");
    for y in [top - 0.015, FLOOR_Y + 0.03] {
        m.screw(&SCREW_BLACK, Vec3::new(cx + 0.04, y, face(y) + 0.0005), n, 0.0024, y);
    }
}

// ---------------------------------------------------------------- walls and trim

/// Moulded window surrounds, post and header trim, a shade lighter than the
/// walls; vinyl side panels with cloth inserts; the darker kick panels.
const MOULDING: Mat = mt(mat::PLASTIC, 0.50, 0.47, 0.42);
const VINYL: Mat = mt(mat::PLASTIC, 0.24, 0.175, 0.11);
const KICK: Mat = mt(mat::PLASTIC, 0.12, 0.095, 0.07);
const PIPING: Mat = mt(mat::PLASTIC, 0.15, 0.11, 0.07);

/// A point on the lining at station z and angle th round the section (as
/// `lining` builds it: 0 on the roof's centre line, PI/2 on the right side).
fn lin_pt(z: f32, th: f32) -> Vec3 {
    let s = sec(z);
    let p = sec_pt(&s, th);
    Vec3::new(p.x * 0.955, s.mid + (p.y - s.mid) * 0.96, z)
}
/// The lining's normal there, facing into the cabin.
fn lin_n(z: f32, th: f32) -> Vec3 {
    let e = 0.003;
    let n = (lin_pt(z, th + e) - lin_pt(z, th - e)).cross(lin_pt(z + e, th) - lin_pt(z - e, th)).normalize();
    let p = lin_pt(z, th);
    if n.dot(Vec3::new(0.0, sec(z).mid, z) - p) < 0.0 { -n } else { n }
}
/// The angle round the lining's section at z whose point is `x` out on the right.
fn lin_th(z: f32, x: f32) -> f32 {
    let (mut a, mut b) = (0.0f32, FRAC_PI_2);
    for _ in 0..30 {
        let m = (a + b) * 0.5;
        if lin_pt(z, m).x < x { a = m } else { b = m }
    }
    (a + b) * 0.5
}
/// A point `off` inboard of the right side's lining at station z, height y
/// (below the roof's curve), and the inboard normal there.
fn wall(z: f32, y: f32, off: f32) -> (Vec3, Vec3) {
    let e = 0.004;
    let wy = (wall_x(z, y + e) - wall_x(z, y - e)) / (2.0 * e);
    let wz = (wall_x(z + e, y) - wall_x(z - e, y)) / (2.0 * e);
    let n = Vec3::new(-1.0, wy, wz).normalize();
    (Vec3::new(wall_x(z, y), y, z) + n * off, n)
}
/// Turn the normals of the vertices from `from` on to face into the cabin.
fn face_in(m: &mut Mesh, from: usize) {
    let k = from + (m.v.len() - from) / 2;
    let (p, n) = (Vec3::from(m.v[k].pos), Vec3::from(m.v[k].nrm));
    if n.dot(Vec3::new(0.0, sec(p.z).mid, p.z) - p) < 0.0 {
        for v in &mut m.v[from..] {
            v.nrm = (-Vec3::from(v.nrm)).into();
        }
    }
}
/// a..b in steps of about `step`, with extra rows within `r` of each end.
fn steps(a: f32, b: f32, r: f32, step: f32) -> Vec<f32> {
    let mut t = vec![a, a + r * 0.3, a + r * 0.7, a + r];
    let n = (((b - a - 2.0 * r) / step).ceil() as usize).max(1);
    t.extend((1..n).map(|k| a + r + (b - a - 2.0 * r) * k as f32 / n as f32));
    t.extend([b - r, b - r * 0.7, b - r * 0.3, b]);
    t
}
/// A padded trim panel on the right wall: z0..z1 by y0..y1, `off` proud of
/// the lining, its edges rolled back to it over `r`.
fn wall_panel(m: &mut Mesh, mm: &Mat, z: [f32; 2], y: [f32; 2], off: f32, r: f32) {
    let ys = steps(y[0], y[1], r, 0.03);
    let rows: Vec<Vec<SP>> = steps(z[0], z[1], r, 0.03)
        .iter()
        .map(|&zz| {
            ys.iter()
                .map(|&yy| {
                    let e = ((zz - z[0]).min(z[1] - zz).min(yy - y[0]).min(y[1] - yy) / r).clamp(0.0, 1.0);
                    let o = 0.001 + (off - 0.001) * (1.0 - (1.0 - e) * (1.0 - e)).sqrt();
                    (wall(zz, yy, o).0, [zz, yy], 0.0)
                })
                .collect()
        })
        .collect();
    let from = m.v.len();
    m.grid(mm, &rows, false, false);
    face_in(m, from);
}
/// Piping or a stitched seam on the right wall from (z, y) to (z, y).
fn wall_line(m: &mut Mesh, mm: &Mat, a: (f32, f32), c: (f32, f32), off: f32, r: f32) {
    let n = ((Vec2::new(c.0 - a.0, c.1 - a.1).length() / 0.02).ceil() as usize).max(1);
    let pts: Vec<Vec3> = (0..=n).map(|k| { let t = k as f32 / n as f32; wall(a.0 + (c.0 - a.0) * t, a.1 + (c.1 - a.1) * t, off).0 }).collect();
    m.tube(mm, &pts, &|_| r, 6, true);
}
/// A screw in a cup washer on the right wall, `off` proud of the lining.
fn wall_screw(m: &mut Mesh, washer: &Mat, z: f32, y: f32, off: f32) {
    let (p, n) = wall(z, y, off);
    m.lathe(washer, p, n, &[[0.0045, 0.0], [0.0045, 0.0005], [0.0033, 0.0013], [0.0, 0.0013]], 14, false);
    m.screw(&mt(mat::SCREW, 0.30, 0.28, 0.25).x(1.0), p + n * 0.001, n, 0.0025, z * 31.0 + y * 7.0);
}
/// Screws round a panel's edge, `inset` in from it, about `pitch` apart.
fn panel_screws(m: &mut Mesh, washer: &Mat, z: [f32; 2], y: [f32; 2], inset: f32, off: f32, pitch: f32) {
    let (z0, z1, y0, y1) = (z[0] + inset, z[1] - inset, y[0] + inset, y[1] - inset);
    let nz = ((z1 - z0) / pitch).round().max(1.0) as usize;
    let ny = ((y1 - y0) / pitch).round().max(1.0) as usize;
    for k in 0..=nz {
        let zz = z0 + (z1 - z0) * k as f32 / nz as f32;
        wall_screw(m, washer, zz, y0, off);
        wall_screw(m, washer, zz, y1, off);
    }
    for k in 1..ny {
        let yy = y0 + (y1 - y0) * k as f32 / ny as f32;
        wall_screw(m, washer, z0, yy, off);
        wall_screw(m, washer, z1, yy, off);
    }
}

/// Where the windshield's opening ends on the lining at angle th, going aft.
fn windshield_edge(th: f32) -> Option<f32> {
    let w = |z: f32| window(lin_pt(z, th));
    let (mut z, mut inside) = (FIREWALL_Z + 0.02, false);
    while z < 0.0 {
        if w(z) < 0.0 {
            inside = true;
        } else if inside {
            let (mut a, mut b) = (z - 0.005, z);
            for _ in 0..24 {
                let c = (a + b) * 0.5;
                if w(c) < 0.0 { a = c } else { b = c }
            }
            return Some((a + b) * 0.5);
        }
        z += 0.005;
    }
    None
}

/// The windshield posts' trim, up each side from the glareshield and across
/// the header over the windshield: a slim moulding over the post, from the
/// windshield's edge back to the first side window's surround (on the roof,
/// a strip 7 cm wide). Right half; mirrored.
fn post_trim(m: &mut Mesh) {
    let mut rows: Vec<(f32, f32, f32)> = Vec::new();
    for k in 0..=80 {
        let th = k as f32 / 80.0 * 1.25;
        let Some(ze) = windshield_edge(th) else { continue };
        if lin_pt(ze, th).y < GLARESHIELD_Y + 0.004 {
            break;
        }
        // Back to where the side window's surround begins, if there is one.
        let (mut zf, mut prev, mut z) = (ze + 0.07, 1.0f32, ze + 0.012);
        while z < ze + 0.17 {
            let v = window(lin_pt(z, th));
            // (stopping short of the surround's flange: no two faces on top of each other)
            if z > ze + 0.03 && v < prev && v < 0.028 {
                zf = z;
                break;
            }
            prev = v;
            z += 0.002;
        }
        rows.push((th, ze, zf - ze));
    }
    if rows.len() < 2 {
        return;
    }
    // Smooth the width where the side window's top edge starts.
    let w: Vec<f32> = (0..rows.len()).map(|i| { let (a, b) = (i.saturating_sub(2), (i + 2).min(rows.len() - 1)); rows[a..=b].iter().map(|r| r.2).sum::<f32>() / (b - a + 1) as f32 }).collect();
    let grid: Vec<Vec<SP>> = rows
        .iter()
        .zip(&w)
        .map(|(&(th, ze, _), &wd)| {
            (0..=8)
                .map(|j| {
                    let t = j as f32 / 8.0;
                    let z = ze - 0.004 + (wd + 0.004) * t;
                    let off = 0.001 + 0.009 * (t * PI).sin().max(0.0).powf(0.5);
                    (lin_pt(z, th) + lin_n(z, th) * off, [z, th], 0.0)
                })
                .collect()
        })
        .collect();
    let from = m.v.len();
    m.grid(&MOULDING, &grid, false, false);
    face_in(m, from);
    // Two screws up each post, and the header's at its ends.
    for (i, &(th, ze, _)) in rows.iter().enumerate() {
        if i % 12 == 6 {
            let z = ze + w[i] * 0.5;
            let (p, n) = (lin_pt(z, th), lin_n(z, th));
            m.screw(&mt(mat::SCREW, 0.40, 0.38, 0.34).x(1.0), p + n * 0.0098, n, 0.0024, th * 13.0);
        }
    }
}

/// A moulded surround round a side window's opening (right side; mirrored):
/// a lip over the glass's edge, a rounded face, a flange onto the wall.
fn window_surround(m: &mut Mesh, c: Vec2) {
    let sd = |q: Vec2| window(Vec3::new(0.5, q.y, q.x));
    let n = 96;
    let flange = 0.024;
    let prof = [(-0.003f32, 0.004f32), (0.0015, 0.0095), (0.011, 0.0085), (flange, 0.0015)];
    let mut rows: Vec<Vec<SP>> = Vec::new();
    let mut screws = Vec::new();
    for k in 0..=n {
        let a = k as f32 / n as f32 * TAU;
        let d = Vec2::new(a.cos(), a.sin());
        let mut r = 0.0;
        while sd(c + d * r) < 0.0 && r < 0.8 {
            r += 0.01;
        }
        let (mut lo, mut hi) = (r - 0.01, r);
        for _ in 0..24 {
            let mid = (lo + hi) * 0.5;
            if sd(c + d * mid) < 0.0 { lo = mid } else { hi = mid }
        }
        let e = c + d * (lo + hi) * 0.5;
        let h = 0.002;
        let g = Vec2::new(sd(e + Vec2::X * h) - sd(e - Vec2::X * h), sd(e + Vec2::Y * h) - sd(e - Vec2::Y * h)).normalize();
        rows.push(prof.iter().map(|&(s, off)| { let q = e + g * s; (wall(q.x, q.y, off).0, [q.x, s], 0.0) }).collect());
        if k % 12 == 6 {
            screws.push(e + g * 0.0125);
        }
    }
    let from = m.v.len();
    m.grid(&MOULDING, &rows, false, false);
    face_in(m, from);
    for q in screws {
        let (p, nn) = wall(q.x, q.y, 0.0088);
        m.screw(&mt(mat::SCREW, 0.40, 0.38, 0.34).x(1.0), p, nn, 0.0024, q.x * 17.0);
    }
}

/// The sun visors, folded up under the headliner just aft of the header
/// trim over the windshield: padded vinyl boards on a chrome rod, a bracket
/// at the inboard end and a clip at the outboard one. Right; mirrored.
fn visor(m: &mut Mesh) {
    // Aft of the header strip (7 cm wide), over the pilot's and the
    // passenger's heads, inboard of the side windows' surrounds.
    let (xa, xb) = (0.075f32, 0.335f32);
    let ze = [xa, (xa + xb) * 0.5, xb].iter().filter_map(|&x| windshield_edge(lin_th(-0.25, x))).fold(-1.0f32, f32::max);
    let (z0, z1) = (ze + 0.085, ze + 0.235);
    let ths: Vec<f32> = (0..=10).map(|k| { let x = xa + (xb - xa) * k as f32 / 10.0; lin_th(z0, x) }).collect();
    let face = |z: f32, th: f32, off: f32| lin_pt(z, th) + lin_n(z, th) * off;
    for (off, mm) in [(0.024f32, mt(mat::PLASTIC, 0.46, 0.42, 0.36)), (0.012, mt(mat::PLASTIC, 0.40, 0.37, 0.32))] {
        let rows: Vec<Vec<SP>> = ths.iter().map(|&th| (0..=6).map(|j| { let z = z0 + (z1 - z0) * j as f32 / 6.0; (face(z, th, off), [z, th], 0.0) }).collect()).collect();
        let from = m.v.len();
        m.grid(&mm, &rows, false, false);
        face_in(m, from);
        if off < 0.02 {
            // (the top face looks up at the headliner)
            for v in &mut m.v[from..] {
                v.nrm = (-Vec3::from(v.nrm)).into();
            }
        }
    }
    // The padded rim round the board's edge.
    let (ta, tb) = (ths[0], ths[ths.len() - 1]);
    let mut rim = Vec::new();
    for k in 0..=10 { rim.push(face(z0, ta + (tb - ta) * k as f32 / 10.0, 0.018)); }
    for k in 1..=6 { rim.push(face(z0 + (z1 - z0) * k as f32 / 6.0, tb, 0.018)); }
    for k in (0..10).rev() { rim.push(face(z1, ta + (tb - ta) * k as f32 / 10.0, 0.018)); }
    for k in (0..6).rev() { rim.push(face(z0 + (z1 - z0) * k as f32 / 6.0, ta, 0.018)); }
    m.tube(&PIPING, &rim, &|_| 0.0062, 8, false);
    // The rod along its forward edge, the bracket and the clip.
    let t0 = lin_th(z0, 0.045);
    let rod: Vec<Vec3> = (0..=12).map(|k| face(z0 - 0.003, t0 + (tb - t0) * k as f32 / 12.0, 0.018)).collect();
    m.tube(&CHROME, &rod, &|_| 0.0038, 8, true);
    let (bp, bn) = (lin_pt(z0 - 0.003, t0), lin_n(z0 - 0.003, t0));
    m.rbox(&mt(mat::PLASTIC, 0.40, 0.37, 0.32), bp + bn * 0.011, Vec3::new(0.008, 0.012, 0.012), 0.004, Quat::from_rotation_arc(Vec3::Y, bn));
    let tc = lin_th(z1 - 0.03, xb + 0.012);
    let (cp, cn) = (lin_pt(z1 - 0.03, tc), lin_n(z1 - 0.03, tc));
    m.rbox(&mt(mat::PLASTIC, 0.40, 0.37, 0.32), cp + cn * 0.009, Vec3::new(0.006, 0.010, 0.014), 0.003, Quat::from_rotation_arc(Vec3::Y, cn));
}

/// The headliner's seams across the roof between the windows' tops, over
/// the bows it is stretched on. Right half; mirrored.
fn headliner(m: &mut Mesh) {
    for z in [0.16f32, 0.50, 0.84, 1.18, 1.52, 1.86, 2.20, 2.54] {
        let pts: Vec<Vec3> = (0..=24)
            .map(|k| k as f32 / 24.0 * 1.2)
            .take_while(|&th| lin_pt(z, th).y > 0.705)
            .map(|th| lin_pt(z, th) + lin_n(z, th) * 0.0015)
            .collect();
        if pts.len() > 1 {
            m.tube(&mt(mat::FABRIC, 0.44, 0.42, 0.38), &pts, &|_| 0.003, 6, true);
        }
    }
}

/// The walls: the moulded trim round the windshield posts and the windows,
/// vinyl side panels with cloth inserts, armrests and seams, the kick
/// panels, the door's seal and handle, air vents, the left side's circuit
/// breaker panel, the sun visors, the headliner's seams, an overhead console.
fn sides(m: &mut Mesh) {
    // Built on the right and mirrored where both sides are alike.
    let mut both = Mesh::default();
    post_trim(&mut both);
    window_surround(&mut both, Vec2::new(0.0, 0.465));
    for w in WINDOWS {
        window_surround(&mut both, Vec2::new((w[0] + w[2]) * 0.5, (w[1] + w[3]) * 0.5));
    }
    visor(&mut both);
    headliner(&mut both);
    // The kick panels in the footwells, ahead of the door and the breakers.
    wall_panel(&mut both, &KICK, [FIREWALL_Z + 0.06, -0.70], [FLOOR_Y + 0.025, -0.13], 0.007, 0.012);
    panel_screws(&mut both, &KICK, [FIREWALL_Z + 0.06, -0.70], [FLOOR_Y + 0.025, -0.13], 0.014, 0.0072, 0.14);
    // The side panels' upper edge (under the window), their armrests.
    let top = 0.200;
    let insert = CLOTH.pat(pat::SEAT);
    // Panels under the rear windows, both sides.
    for (z0, z1) in [(0.40f32, 1.06f32), (1.10, 2.32)] {
        wall_panel(&mut both, &VINYL, [z0, z1], [-0.06, top], 0.010, 0.012);
        wall_panel(&mut both, &insert, [z0 + 0.06, z1 - 0.06], [0.05, top - 0.04], 0.0135, 0.006);
        wall_line(&mut both, &PIPING, (z0 + 0.012, top - 0.004), (z1 - 0.012, top - 0.004), 0.0095, 0.0028);
        panel_screws(&mut both, &PIPING, [z0, z1], [-0.06, top], 0.012, 0.0095, 0.16);
    }
    // The air vents in the walls by the panel.
    let (vz, vy) = (PANEL_Z + 0.08, 0.20);
    let vx = wall_x(vz, vy);
    let n = Vec3::new(-1.0, 0.0, 0.2).normalize();
    both.lathe(&CHROME, Vec3::new(vx, vy, vz), n, &[[0.026, 0.0], [0.026, 0.004], [0.021, 0.008], [0.019, 0.008]], 20, false);
    both.lathe(&BLACK, Vec3::new(vx - 0.004, vy, vz), n, &[[0.019, 0.0], [0.018, 0.007], [0.013, 0.012], [0.007, 0.014], [0.0, 0.014]], 16, true);
    m.append(&both.mirror_x());
    m.append(&both);

    // The front side panels: the door's on the right (from its forward edge
    // to its handle), on the left from the breaker panel back. Built on the
    // right; the left one is mirrored from a mesh of its own.
    for (side, z0) in [(1.0f32, -0.64f32), (-1.0, -0.29)] {
        let z1 = 0.33;
        let mut s = Mesh::default();
        wall_panel(&mut s, &VINYL, [z0, z1], [-0.13, top], 0.011, 0.012);
        // The cloth insert above the armrest, piped round its edge, and the
        // stitched seam under the armrest.
        let (iz0, iz1) = (z0 + 0.05, z1 - 0.07);
        wall_panel(&mut s, &insert, [iz0, iz1], [0.085, top - 0.03], 0.0145, 0.006);
        for (a, c) in [((iz0, 0.085), (iz1, 0.085)), ((iz0, top - 0.03), (iz1, top - 0.03)), ((iz0, 0.085), (iz0, top - 0.03)), ((iz1, 0.085), (iz1, top - 0.03))] {
            wall_line(&mut s, &PIPING, a, c, 0.0128, 0.0022);
        }
        wall_line(&mut s, &PIPING, (z0 + 0.02, -0.02), (z1 - 0.02, -0.02), 0.0108, 0.0016);
        panel_screws(&mut s, &PIPING, [z0, z1], [-0.13, top], 0.012, 0.0105, 0.16);
        // The armrest: a padded bar along the panel, its front end rounded,
        // a pull cup moulded into its top at the front.
        let (az0, az1) = (z0.max(-0.36) + 0.06, 0.16);
        let ay = 0.05;
        let (p, nn) = wall(az0 * 0.5 + az1 * 0.5, ay, 0.036);
        s.rbox(&VINYL, p, Vec3::new(0.026, 0.024, (az1 - az0) * 0.5), 0.016, Quat::IDENTITY);
        s.rbox(&PIPING, p + Vec3::new(0.0, -0.022, 0.0) - nn * 0.012, Vec3::new(0.020, 0.006, (az1 - az0) * 0.5 - 0.01), 0.004, Quat::IDENTITY);
        s.rbox(&BLACK, p + Vec3::new(-0.002, 0.0225, -(az1 - az0) * 0.5 + 0.06), Vec3::new(0.012, 0.003, 0.03), 0.003, Quat::IDENTITY);
        if side > 0.0 {
            // The door's map pocket low at the front, its stiff lip.
            wall_panel(&mut s, &KICK, [-0.58, -0.22], [-0.115, -0.04], 0.026, 0.012);
            wall_line(&mut s, &PIPING, (-0.57, -0.042), (-0.23, -0.042), 0.026, 0.0035);
            // The inside handle at the door's aft edge, in its escutcheon.
            let hz = 0.30;
            let hx = wall_x(hz, 0.10);
            s.rbox(&BLACK, Vec3::new(hx - 0.013, 0.10, hz), Vec3::new(0.004, 0.03, 0.05), 0.004, Quat::IDENTITY);
            s.rbox(&CHROME, Vec3::new(hx - 0.027, 0.10, hz), Vec3::new(0.008, 0.009, 0.045), 0.006, Quat::IDENTITY);
            m.append(&s);
        } else {
            m.append(&s.mirror_x());
        }
    }
    // The door's seal round its opening on the right (see `DOOR`), and its
    // upper latch at the top aft corner.
    let mut pts = Vec::new();
    for k in 0..DOOR.len() {
        let (a, c) = (Vec2::from(DOOR[k]), Vec2::from(DOOR[(k + 1) % DOOR.len()]));
        let n = ((c - a).length() / 0.03).ceil() as usize;
        for i in 0..n {
            let q = a.lerp(c, i as f32 / n as f32);
            let y = q.y.min(sec(q.x).top * 0.96 + sec(q.x).mid * 0.04 - 0.03);
            pts.push(Vec3::new(wall_x(q.x, y) - 0.004, y, q.x));
        }
    }
    pts.push(pts[0]);
    m.tube(&mt(mat::RUBBER, 0.03, 0.03, 0.03), &pts, &|_| 0.006, 6, false);
    let (lp, ln) = wall(0.335, 0.69, 0.008);
    m.rbox(&CHROME, lp, Vec3::new(0.008, 0.01, 0.03), 0.004, Quat::from_rotation_arc(Vec3::X, -ln));
    // The left (storm) window's latch on its lower frame.
    let (sp, sn) = wall(-0.08, 0.215, 0.012);
    m.rbox(&BLACK, Vec3::new(-sp.x, sp.y, sp.z), Vec3::new(0.008, 0.01, 0.025), 0.004, Quat::from_rotation_arc(Vec3::NEG_X, Vec3::new(-sn.x, sn.y, sn.z)));

    // The circuit breaker panel on the left wall by the pilot's knee: three
    // rows of push-to-reset breakers on a black plate, each labelled.
    let (bz, by) = ([-0.62f32, -0.33f32], [-0.13f32, -0.005f32]);
    let mut cb = Mesh::default();
    wall_panel(&mut cb, &PANEL_COL.pat(pat::PANEL), bz, by, 0.010, 0.005);
    panel_screws(&mut cb, &PANEL_COL, bz, by, 0.007, 0.0105, 0.10);
    const BREAKERS: [[&str; 7]; 3] = [
        ["GEAR", "FLAPS", "TRIM", "ANN LIGHTS", "FUEL PUMP", "PITOT HEAT", "ALT"],
        ["LANDING", "TAXI", "NAV", "BEACON", "STROBE", "DOME", "PANEL"],
        ["BAT", "LIGHTS", "CABIN", "SUCTION", "SPARE", "SPARE", "SPARE"],
    ];
    let mut labels = Vec::new();
    for (r, row) in BREAKERS.iter().enumerate() {
        for (k, text) in row.iter().enumerate() {
            // (back to front reads left to right, facing the wall)
            let z = bz[1] - 0.024 - k as f32 * 0.039;
            let y = by[1] - 0.026 - r as f32 * 0.04;
            let (p, n) = wall(z, y, 0.010);
            cb.lathe(&BLACK, p, n, &[[0.0065, 0.0], [0.0065, 0.002], [0.0048, 0.003], [0.0048, 0.009], [0.004, 0.0105], [0.0, 0.0105]], 12, false);
            cb.disc(&PAINT_WHITE, p + n * 0.0106, n, Vec3::Y, 0.0022, 8);
            labels.push((wall(z, y - 0.0135, 0.0106), *text));
        }
    }
    let mut cb = cb.mirror_x();
    for ((p, n), text) in labels {
        let (p, n) = (Vec3::new(-p.x, p.y, p.z), Vec3::new(-n.x, n.y, n.z));
        let ax = (-n).cross(Vec3::Y).normalize();
        label(&mut cb, p, ax, n.cross(ax), 0.0165, 0.0034, text);
    }
    m.append(&cb);

    // The overhead console: two air outlets and the dome light.
    let (z0, z1) = (-0.12f32, 0.18f32);
    let top = sec(0.03).top * 0.96 + sec(0.03).mid * 0.04;
    m.rbox(&mt(mat::PLASTIC, 0.50, 0.48, 0.44).pat(pat::PANEL), Vec3::new(0.0, top - 0.012, (z0 + z1) * 0.5), Vec3::new(0.06, 0.016, (z1 - z0) * 0.5), 0.008, Quat::IDENTITY);
    for x in [-0.03f32, 0.03] {
        m.lathe(&CHROME, Vec3::new(x, top - 0.028, -0.05), Vec3::NEG_Y, &[[0.016, 0.0], [0.016, 0.003], [0.012, 0.006]], 16, false);
        m.dome(&BLACK, Vec3::new(x, top - 0.031, -0.05), Vec3::new(0.0, -1.0, 0.3), 0.012, 0.008, 12, 0.0);
    }
    m.lathe(&mt(mat::PLASTIC, 0.75, 0.74, 0.70), Vec3::new(0.0, top - 0.028, 0.10), Vec3::NEG_Y, &[[0.03, 0.0], [0.03, 0.004], [0.022, 0.009], [0.0, 0.01]], 18, true);
}

// ---------------------------------------------------------------- seats

/// A seat, built facing forward with the cushion's top middle at the
/// origin, then turned (`aft`: the club seats face back) and moved to `at`.
/// `half` is the seat's half width. The frame's legs reach down to the
/// floor under each, or to the curved lining where the aft cabin narrows.
fn seat(m: &mut Mesh, at: Vec3, aft: bool, headrest: bool, half: f32) {
    let turn = if aft { Quat::from_rotation_y(PI) } else { Quat::IDENTITY };
    let mut s = Mesh::default();
    let cloth = CLOTH.pat(pat::SEAT);
    let tilt = Quat::from_rotation_x(0.08);
    s.rbox(&cloth, tilt * Vec3::new(0.0, -0.035, 0.0), Vec3::new(half - 0.04, 0.035, 0.21), 0.03, tilt);
    for sx in [-1.0f32, 1.0] {
        s.rbox(&LEATHER, tilt * Vec3::new(sx * (half - 0.024), -0.03, 0.0), Vec3::new(0.03, 0.045, 0.22), 0.026, tilt);
    }
    s.rbox(&LEATHER, tilt * Vec3::new(0.0, -0.035, -0.205), Vec3::new(half, 0.042, 0.03), 0.028, tilt);
    s.rbox(&LEATHER, tilt * Vec3::new(0.0, -0.075, 0.0), Vec3::new(half - 0.005, 0.012, 0.21), 0.008, tilt);
    // The back, leaning from its hinge behind the cushion.
    let rot = Quat::from_rotation_x(0.22);
    let hinge = Vec3::new(0.0, -0.02, 0.22);
    let bk = |p: Vec3| hinge + rot * p;
    let h = 0.30;
    s.rbox(&cloth, bk(Vec3::new(0.0, h, 0.0)), Vec3::new(half - 0.04, h - 0.03, 0.04), 0.03, rot);
    for sx in [-1.0f32, 1.0] {
        s.rbox(&LEATHER, bk(Vec3::new(sx * (half - 0.024), h, 0.012)), Vec3::new(0.03, h, 0.055), 0.026, rot);
    }
    s.rbox(&LEATHER, bk(Vec3::new(0.0, 2.0 * h - 0.012, 0.012)), Vec3::new(half, 0.028, 0.055), 0.026, rot);
    s.rbox(&LEATHER, bk(Vec3::new(0.0, h, 0.058)), Vec3::new(half - 0.004, h, 0.012), 0.01, rot);
    if headrest {
        s.rbox(&LEATHER, bk(Vec3::new(0.0, 2.0 * h + 0.10, 0.015)), Vec3::new(0.11, 0.06, 0.04), 0.03, rot);
        for dx in [-0.07f32, 0.07] {
            s.tube(&CHROME, &[bk(Vec3::new(dx, 2.0 * h, 0.012)), bk(Vec3::new(dx, 2.0 * h + 0.05, 0.012))], &|_| 0.0045, 8, false);
        }
    }
    // The frame: four legs down to the rails, the adjuster bar under the front.
    let lx = half - 0.055;
    for (dx, dz) in [(-lx, -0.17f32), (lx, -0.17), (-lx, 0.17), (lx, 0.17)] {
        let w = turn * Vec3::new(dx, 0.0, dz) + at;
        let mut y = floor_y(w.z);
        while wall_x(w.z, y) < w.x.abs() + 0.02 && y < at.y - 0.1 {
            y += 0.005;
        }
        let foot = y - at.y + 0.004;
        s.tube(&GREY, &[Vec3::new(dx, -0.08, dz), Vec3::new(dx, foot, dz)], &|_| 0.009, 8, true);
    }
    s.tube(&CHROME, &[Vec3::new(-0.06, -0.085, -0.20), Vec3::new(-0.06, -0.095, -0.235), Vec3::new(0.06, -0.095, -0.235), Vec3::new(0.06, -0.085, -0.20)], &|_| 0.004, 6, false);
    for v in &mut s.v {
        v.pos = (turn * Vec3::from(v.pos) + at).into();
        v.nrm = (turn * Vec3::from(v.nrm)).into();
    }
    m.append(&s);
}

/// The back pair's centre line off the middle, and their half width.
const REAR_SEAT: [f32; 2] = [0.195, 0.18];

fn seats(m: &mut Mesh) {
    let belt = STRAP.pat(pat::WEBBING);
    // The front pair, on the floor raised over the spars.
    for xs in [-0.27f32, 0.27] {
        let c = Vec3::new(xs, -0.075, -0.07);
        seat(m, c, false, true, 0.215);
        // The lap belt's halves on the cushion; the shoulder harness from
        // the roof down across the outboard shoulder.
        let so = xs.signum();
        let buckle = c + Vec3::new(-so * 0.06, 0.004, 0.02);
        strap(m, &belt, c + Vec3::new(so * 0.21, 0.004, 0.17), buckle - Vec3::X * so * 0.03, Vec3::Y, 0.045, 0.003);
        strap(m, &belt, c + Vec3::new(-so * 0.21, 0.004, 0.16), buckle + Vec3::X * so * 0.02, Vec3::Y, 0.045, 0.003);
        m.rbox(&CHROME, buckle, Vec3::new(0.03, 0.005, 0.022), 0.003, Quat::from_rotation_y(0.3 * so));
        let roof = Vec3::new(so * 0.40, 0.66, 0.42);
        let shoulder = Vec3::new(xs + so * 0.14, 0.43, 0.215);
        strap(m, &belt, roof, shoulder + Vec3::new(0.0, 0.02, 0.03), Vec3::new(-so * 0.3, 0.3, -1.0).normalize(), 0.045, 0.003);
        strap(m, &belt, shoulder, buckle + Vec3::new(0.0, 0.004, 0.0), Vec3::new(0.0, 0.25, -1.0).normalize(), 0.045, 0.003);
        m.rbox(&CHROME, roof - Vec3::Y * 0.02, Vec3::new(0.012, 0.02, 0.008), 0.003, Quat::IDENTITY);
    }
    // The club seats facing aft, and the back pair facing forward: those
    // narrower and closer together, as the cabin tapers towards the
    // baggage bulkhead (their backs and frames would otherwise go through
    // the skin).
    for so in [-1.0f32, 1.0] {
        let c = Vec3::new(so * 0.25, floor_y(1.0) + 0.20, 1.02);
        seat(m, c, true, true, 0.215);
        let (xs, half) = (so * REAR_SEAT[0], REAR_SEAT[1]);
        let c = Vec3::new(xs, floor_y(1.86) + 0.225, 1.86);
        seat(m, c, false, true, half);
        strap(m, &belt, c + Vec3::new(so * (half - 0.005), 0.004, 0.17), c + Vec3::new(-so * 0.04, 0.004, 0.02), Vec3::Y, 0.045, 0.003);
    }
}

// ---------------------------------------------------------------- controls

/// The trim wheel's centre, on the pedestal's left side.
const TRIM_WHEEL: Vec3 = Vec3::new(PED[0] - PED[1] - 0.009, -0.31, PANEL_Z + 0.035);

fn controls(b: &mut Builder) {
    // The throw-over control column: a chrome tube out of the panel that
    // slides fore and aft with the elevator, the arm swung over to the
    // pilot's side, and the wheel on it that turns with the ailerons.
    let mut col = Mesh::default();
    let end = Vec3::new(COLUMN.x, COLUMN.y, HUB.z);
    col.tube(&CHROME, &[COLUMN - Vec3::Z * 0.10, end], &|_| 0.0125, 16, false);
    col.rbox(&BLACK, end, Vec3::new(0.022, 0.022, 0.024), 0.01, Quat::IDENTITY);
    let mid = (end + HUB) * 0.5 + Vec3::new(0.01, 0.02, 0.0);
    let arm: Vec<Vec3> = (0..=10).map(|k| { let t = k as f32 / 10.0; end.lerp(mid, t).lerp(mid.lerp(HUB, t), t) }).collect();
    col.tube(&mt(mat::PAINT, 0.05, 0.05, 0.055), &arm, &|t| 0.014 - 0.002 * t, 12, true);
    col.lathe(&BLACK, HUB, Vec3::Z, &[[0.0, -0.02], [0.022, -0.02], [0.024, 0.0], [0.0, 0.0]], 16, false);
    let column = b.add("control column", G_FUSE, col, Anim::slide(Drive::new(Input::Pitch, 0.07), Vec3::Z), COLUMN, Vec3::Z, None);
    let mut collar = Mesh::default();
    collar.lathe(&CHROME, COLUMN, Vec3::Z, &[[0.032, 0.0], [0.032, 0.004], [0.018, 0.009], [0.0135, 0.009]], 24, false);
    b.fixed("yoke collar", G_FUSE, collar);
    // The wheel: ram's horns, a hub with the Beech emblem plate, the
    // push-to-talk and the electric trim switch on the left horn.
    let mut y = Mesh::default();
    let c = HUB + Vec3::Z * 0.004;
    let path: Vec<Vec3> = [(-0.128, 0.085), (-0.14, 0.04), (-0.133, 0.0), (-0.10, -0.03), (-0.05, -0.042), (0.05, -0.042), (0.10, -0.03), (0.133, 0.0), (0.14, 0.04), (0.128, 0.085)]
        .iter()
        .map(|&(dx, dy)| c + Vec3::new(dx, dy, 0.012))
        .collect();
    y.tube(&KNOB, &path, &|t| 0.015 + 0.004 * (1.0 - (t * PI).sin()), 14, true);
    y.rbox(&KNOB, c + Vec3::new(0.0, -0.02, 0.008), Vec3::new(0.052, 0.03, 0.02), 0.013, Quat::IDENTITY);
    y.disc(&mt(mat::ALU, 0.6, 0.6, 0.62), c + Vec3::new(0.0, -0.02, 0.0285), Vec3::Z, Vec3::Y, 0.013, 20);
    y.rbox(&PAINT_WHITE.pat(pat::PLACARD), c + Vec3::new(0.0, 0.003, 0.0285), Vec3::new(0.012, 0.003, 0.0004), 0.0002, Quat::IDENTITY);
    y.dome(&RED, c + Vec3::new(-0.13, 0.098, 0.012), Vec3::new(0.0, 1.0, 0.2), 0.006, 0.004, 10, 0.0);
    let trim_at = Vec3::new(-0.112, 0.062, 0.03);
    y.rbox(&mt(mat::PLASTIC, 0.08, 0.08, 0.08), c + trim_at, Vec3::new(0.0045, 0.008, 0.004), 0.002, Quat::IDENTITY);
    y.rbox(&mt(mat::PLASTIC, 0.08, 0.08, 0.08), c + trim_at + Vec3::new(0.012, 0.0, -0.002), Vec3::new(0.003, 0.009, 0.003), 0.0015, Quat::IDENTITY);
    y.screw(&SCREW_BLACK, c + Vec3::new(0.035, -0.04, 0.0285), Vec3::Z, 0.0022, 1.0);
    y.screw(&SCREW_BLACK, c + Vec3::new(-0.035, -0.04, 0.0285), Vec3::Z, 0.0022, 2.0);
    let wheel = b.add("yoke", G_FUSE, y, Anim::hinge(Drive::new(Input::Roll, -0.7)), HUB, Vec3::Z, Some(column));
    b.pieces[wheel].hotspot = Some(Hotspot::wheel(trim_at + Vec3::Z * 0.004, 0.010, "trim_down", "trim_up"));

    // Rudder pedals with toe brakes, hinged low on a bar in each footwell
    // (the pilot's brakes work with the mouse; the copilot's follow).
    let pz = -0.90;
    let py = FLOOR_Y + 0.035;
    for xs in [-0.27f32, 0.27] {
        let mut tt = Mesh::default();
        tt.tube(&GREY, &[Vec3::new(xs - 0.14, py, pz), Vec3::new(xs + 0.14, py, pz)], &|_| 0.011, 10, true);
        for dx in [-0.135f32, 0.0, 0.135] {
            tt.rbox(&GREY, Vec3::new(xs + dx, py - 0.016, pz), Vec3::new(0.012, 0.02, 0.018), 0.004, Quat::IDENTITY);
        }
        b.fixed("rudder bar", G_FUSE, tt);
        for dx in [-0.085f32, 0.085] {
            let mut p = Mesh::default();
            let pv = Vec3::new(xs + dx, py, pz);
            let pc = pv + Vec3::new(0.0, 0.15, 0.06);
            p.tube(&GREY, &[pv, pc - Vec3::Y * 0.03], &|_| 0.008, 8, true);
            let tilt = Quat::from_rotation_x(-0.42);
            p.rbox(&ALU, pc, Vec3::new(0.036, 0.065, 0.005), 0.004, tilt);
            for k in 0..4 {
                p.rbox(&RUBBER, pc + tilt * Vec3::new(0.0, -0.045 + k as f32 * 0.028, 0.0055), Vec3::new(0.03, 0.0045, 0.0015), 0.001, tilt);
            }
            let side = dx.signum();
            let piece = b.add("pedal", G_FUSE, p, Anim::hinge2(Drive::new(Input::Yaw, -side * 0.18), Drive::new(Input::Brake(side), -0.14)), pv, Vec3::X, None);
            if xs < 0.0 {
                b.pieces[piece].hotspot = Some(Hotspot::hold(pc - pv + tilt * Vec3::new(0.0, 0.03, 0.01), 0.036, if dx < 0.0 { "brake_left" } else { "brake_right" }));
            }
        }
    }

    // The power quadrant's levers: pushed forward for more, the knobs
    // shaped and coloured as on the Beech quadrant (a black flat-topped
    // throttle, the blue propeller with its ridges, the red knurled mixture).
    let knob_at = Vec3::new(0.0, 0.10, 0.075);
    for (k, (name, input, up, down)) in [("throttle", Input::Throttle, "throttle_up", "throttle_down"), ("prop control", Input::PropControl, "prop_up", "prop_down"), ("mixture lever", Input::Mixture, "mixture_rich", "mixture_lean")].iter().enumerate() {
        let pv = THROTTLE_PIVOT + Vec3::X * (k as f32 * 0.045);
        let mut lv = Mesh::default();
        lv.tube(&CHROME, &[pv, pv + knob_at * 0.92], &|_| 0.0045, 8, true);
        let kr = Quat::from_rotation_x(-0.6);
        match k {
            0 => {
                lv.rbox(&KNOB, pv + knob_at, Vec3::new(0.016, 0.014, 0.016), 0.006, kr);
                lv.rbox(&mt(mat::PLASTIC, 0.06, 0.06, 0.065), pv + knob_at + kr * Vec3::new(0.0, 0.0145, 0.0), Vec3::new(0.013, 0.001, 0.013), 0.001, kr);
            }
            1 => {
                lv.lathe(&BLUE, pv + knob_at - kr * Vec3::Y * 0.012, kr * Vec3::Y, &[[0.006, 0.0], [0.013, 0.004], [0.013, 0.008], [0.011, 0.010], [0.013, 0.012], [0.013, 0.018], [0.009, 0.024], [0.0, 0.025]], 8, false);
            }
            _ => {
                lv.lathe(&RED.pat(pat::KNURL), pv + knob_at - kr * Vec3::Y * 0.012, kr * Vec3::Y, &[[0.006, 0.0], [0.012, 0.004], [0.013, 0.016], [0.011, 0.022], [0.0, 0.024]], 18, true);
            }
        }
        let piece = b.add(name, G_FUSE, lv, Anim::hinge(Drive::new(*input, -0.7).offset(0.35)), pv, Vec3::X, None);
        b.pieces[piece].hotspot = Some(Hotspot::wheel(knob_at, 0.018, up, down));
    }

    // The gear switch: a lever with a little wheel-shaped knob, down for down.
    let mut gs = Mesh::default();
    let tip = Vec3::new(0.0, 0.0, 0.045);
    gs.rbox(&CHROME, GEAR_PIVOT + Vec3::Z * 0.004, Vec3::new(0.008, 0.012, 0.004), 0.003, Quat::IDENTITY);
    gs.tube(&CHROME, &[GEAR_PIVOT, GEAR_PIVOT + tip], &|_| 0.0035, 8, true);
    gs.lathe(&WHITE, GEAR_PIVOT + tip - Vec3::X * 0.007, Vec3::X, &[[0.0, 0.0], [0.012, 0.0], [0.014, 0.002], [0.014, 0.012], [0.012, 0.014], [0.0, 0.014]], 20, false);
    let k = b.add("gear handle", G_FUSE, gs, Anim::hinge(Drive::new(Input::GearHandle, 0.35)), GEAR_PIVOT, Vec3::X, None);
    b.pieces[k].hotspot = Some(Hotspot::click(tip, 0.017, "gear"));

    // The flap switch: a lever with a flat, airfoil-shaped knob, three
    // positions (UP, APPR, DN) driven by the flap notch selected.
    let mut fl = Mesh::default();
    let tip = Vec3::new(0.0, 0.0, 0.04);
    fl.rbox(&CHROME, FLAP_PIVOT + Vec3::Z * 0.004, Vec3::new(0.007, 0.011, 0.004), 0.003, Quat::IDENTITY);
    fl.tube(&CHROME, &[FLAP_PIVOT, FLAP_PIVOT + tip], &|_| 0.003, 8, true);
    let foil: Vec<Vec2> = (0..20).map(|k| { let a = k as f32 / 20.0 * TAU; Vec2::new(0.013 * a.cos(), 0.005 * a.sin() * (1.0 - 0.5 * a.cos())) }).collect();
    fl.slab(&WHITE, &foil, &[], FLAP_PIVOT + tip + Vec3::Z * 0.008, Vec3::X, Vec3::Y, 0.012);
    let k = b.add("flap switch", G_FUSE, fl, Anim::hinge(Drive::new(Input::FlapLever, 0.7).offset(-0.35)), FLAP_PIVOT, Vec3::X, None);
    b.pieces[k].hotspot = Some(Hotspot::wheel(tip + Vec3::Z * 0.002, 0.016, "flaps_up", "flaps_down"));
    // The flap position indicator's pointer, following the flaps themselves.
    let mut pt = Mesh::default();
    let ind = Vec3::new(FLAP_PIVOT.x + 0.040 - 0.007, FLAP_PIVOT.y + 0.026, PANEL_Z + 0.0025);
    pt.rbox(&mt(mat::PAINT, 0.85, 0.85, 0.82), ind, Vec3::new(0.004, 0.0015, 0.0008), 0.0005, Quat::IDENTITY);
    b.add("flap indicator", G_FUSE, pt, Anim::slide(Drive::new(Input::Flaps, -0.052), Vec3::Y), ind, Vec3::Y, None);

    // The elevator trim wheel: rolled forward, nose down.
    let mut w = Mesh::default();
    let wc = TRIM_WHEEL;
    w.lathe(&mt(mat::PLASTIC, 0.04, 0.04, 0.042).pat(pat::KNURL), wc - Vec3::X * 0.007, Vec3::X, &[[0.0, 0.0], [0.052, 0.0], [0.056, 0.003], [0.056, 0.011], [0.052, 0.014], [0.0, 0.014]], 40, false);
    w.rbox(&PAINT_WHITE, wc + Vec3::new(-0.0075, 0.0, -0.05), Vec3::new(0.0008, 0.004, 0.003), 0.0005, Quat::IDENTITY);
    let k = b.add("trim wheel", G_FUSE, w, Anim::hinge(Drive::new(Input::Trim, 9.0)), wc, Vec3::X, None);
    // (on the rim facing the pilot, up and aft of the hub)
    b.pieces[k].hotspot = Some(Hotspot::wheel(Vec3::new(-0.006, 0.03, 0.035), 0.024, "trim_down", "trim_up"));
}

/// The switches, handles and the fuel selector that show how they are set.
fn switches(b: &mut Builder) {
    let f = PANEL_Z + 0.0065;
    // The red split master: the battery half clicks; the alternator half
    // moves with it (the simulator has one master switch).
    for (i, clickable) in [(0usize, true), (1, false)] {
        let mut r = Mesh::default();
        let c = Vec3::new(switch_x(i), SWITCH_Y, f);
        r.rbox(&RED, c, Vec3::new(0.0072, 0.011, 0.004), 0.002, Quat::IDENTITY);
        let k = b.add(if clickable { "master switch" } else { "alternator switch" }, G_FUSE, r, Anim::hinge(Drive::new(Input::Master, -0.36).offset(0.18)), c - Vec3::Z * 0.004, Vec3::X, None);
        if clickable {
            b.pieces[k].hotspot = Some(Hotspot::click(Vec3::Z * 0.006, 0.010, "master"));
        }
    }
    let mut r = Mesh::default();
    let c = Vec3::new(switch_x(3), SWITCH_Y, f);
    r.rbox(&KNOB, c, Vec3::new(0.0062, 0.011, 0.004), 0.002, Quat::IDENTITY);
    let k = b.add("landing light switch", G_FUSE, r, Anim::hinge(Drive::new(Input::LandingLights, -0.36).offset(0.18)), c - Vec3::Z * 0.004, Vec3::X, None);
    b.pieces[k].hotspot = Some(Hotspot::click(Vec3::Z * 0.006, 0.011, "landing_lights"));

    // The key turns clockwise from OFF to BOTH, on to START while the starter runs.
    let mut kk = Mesh::default();
    kk.rbox(&mt(mat::STEEL, 0.6, 0.55, 0.4), KEY + Vec3::Z * 0.014, Vec3::new(0.003, 0.009, 0.008), 0.0015, Quat::IDENTITY);
    kk.rbox(&BLACK, KEY + Vec3::Z * 0.027, Vec3::new(0.004, 0.012, 0.009), 0.003, Quat::IDENTITY);
    let k = b.add("ignition key", G_FUSE, kk, Anim::hinge(Drive::new(Input::Ignition, -0.8)), KEY, Vec3::Z, None);
    b.pieces[k].hotspot = Some(Hotspot { at: [0.0, 0.0, 0.025], r: 0.016, click: Some("ignition"), hold: Some("starter"), wheel: None });

    // The parking brake: a T handle, pulled out when set.
    let mut p = Mesh::default();
    p.tube(&CHROME, &[PARK_BRAKE - Vec3::Z * 0.03, PARK_BRAKE + Vec3::Z * 0.02], &|_| 0.0035, 8, false);
    p.rbox(&mt(mat::PLASTIC, 0.03, 0.03, 0.032), PARK_BRAKE + Vec3::Z * 0.025, Vec3::new(0.024, 0.0065, 0.0065), 0.005, Quat::IDENTITY);
    let k = b.add("parking brake", G_FUSE, p, Anim::slide(Drive::new(Input::ParkingBrake, 0.035), Vec3::Z), PARK_BRAKE, Vec3::Z, None);
    b.pieces[k].hotspot = Some(Hotspot::click(Vec3::Z * 0.025, 0.02, "parking_brake"));

    // The cowl flaps: a push-pull knob, pulled out to open them.
    let mut cw = Mesh::default();
    cw.tube(&CHROME, &[COWL - Vec3::Z * 0.03, COWL + Vec3::Z * 0.012], &|_| 0.0035, 8, false);
    cw.lathe(&KNOB.pat(pat::KNURL), COWL + Vec3::Z * 0.012, Vec3::Z, &[[0.0035, 0.0], [0.011, 0.003], [0.012, 0.014], [0.010, 0.017], [0.0, 0.018]], 16, true);
    let k = b.add("cowl flap knob", G_FUSE, cw, Anim::slide(Drive::new(Input::CowlFlaps, 0.03), Vec3::Z), COWL, Vec3::Z, None);
    b.pieces[k].hotspot = Some(Hotspot::wheel(Vec3::Z * 0.022, 0.016, "cowl_open", "cowl_close"));

    // The fuel selector on the floor by the left wall: a moulded cover, the
    // placard (LEFT, RIGHT, OFF) tilted toward the pilot, and the handle.
    let n = Vec3::new(0.30, 1.0, 0.45).normalize();
    let fwd = (Vec3::NEG_Z - n * n.dot(Vec3::NEG_Z)).normalize();
    let rgt = fwd.cross(n);
    let rot = Quat::from_mat3(&Mat3::from_cols(rgt, n, -fwd));
    let mut cover = Mesh::default();
    cover.rbox(&mt(mat::PLASTIC, 0.12, 0.10, 0.08), SELECTOR - n * 0.04, Vec3::new(0.065, 0.04, 0.075), 0.02, rot);
    cover.disc(&mt(mat::PLASTIC, 0.45, 0.44, 0.40), SELECTOR + n * 0.0005, n, fwd, 0.052, 32);
    for (d, text, w) in [(-rgt, "LEFT", 0.011f32), (rgt, "RIGHT", 0.013), (-fwd, "OFF", 0.009), (fwd, "FUEL", 0.010)] {
        label(&mut cover, SELECTOR + n * 0.001 + d * 0.036, rgt, fwd, w, 0.0045, text);
    }
    b.fixed("cabin", G_FUSE, cover);
    let mut h = Mesh::default();
    // (a grip across the centre, the long end pointing at the tank selected)
    h.rbox(&RED, SELECTOR + n * 0.013 + fwd * 0.008, Vec3::new(0.009, 0.011, 0.030), 0.005, rot);
    h.rbox(&mt(mat::PAINT, 0.85, 0.85, 0.82), SELECTOR + n * 0.0245 + fwd * 0.028, Vec3::new(0.0015, 0.0006, 0.008), 0.0005, rot);
    let tip: Vec<Vec2> = vec![Vec2::new(-0.006, 0.0), Vec2::new(0.006, 0.0), Vec2::new(0.0, 0.016)];
    h.slab(&RED, &tip, &[], SELECTOR + n * 0.017 + fwd * 0.037, rgt, fwd, 0.008);
    h.lathe(&CHROME, SELECTOR, n, &[[0.012, 0.0], [0.012, 0.004], [0.0, 0.005]], 14, false);
    let k = b.add("fuel selector", G_FUSE, h, Anim::hinge(Drive::new(Input::FuelValve, -FRAC_PI_2)), SELECTOR, n, None);
    let mut t = Hotspot::wheel(n * 0.016 + fwd * 0.02, 0.028, "fuel_next", "fuel_prev");
    t.click = Some("fuel_valve");
    b.pieces[k].hotspot = Some(t);
}

/// Knobs that work but do not move: on the instruments (OBS, ADF card,
/// heading indicator, altimeter setting) and the nav receiver's tuning.
fn knobs(b: &mut Builder) {
    let z = PANEL_Z + 0.004;
    let gauge_knob = |id: f32| {
        let g = GAUGES.iter().find(|g| g.0 == id).unwrap();
        Vec3::new(g.1 - g.3 * 0.82, g.2 - g.3 * 0.82, z)
    };
    let nav = Vec3::new(STACK[1] - 0.014, 0.2125 - 0.0055, PANEL_Z + 0.0051);
    let list: [(Vec3, Hotspot); 5] = [
        (gauge_knob(g::CDI), Hotspot::wheel(Vec3::ZERO, 0.012, "obs_right", "obs_left")),
        (gauge_knob(g::ADF), Hotspot::wheel(Vec3::ZERO, 0.012, "adf_next", "adf_prev")),
        (gauge_knob(g::HI), Hotspot::click(Vec3::ZERO, 0.012, "heading_sync")),
        (gauge_knob(g::ALT), Hotspot::click(Vec3::ZERO, 0.012, "baro_qnh")),
        (nav, Hotspot::wheel(Vec3::ZERO, 0.013, "nav_next", "nav_prev")),
    ];
    for (c, mut h) in list {
        let mut m = Mesh::default();
        m.lathe(&KNOB.pat(pat::KNURL), c, Vec3::Z, &[[0.009, 0.0], [0.009, 0.008], [0.0078, 0.0095], [0.0, 0.0095]], 18, false);
        m.lathe(&KNOB.pat(pat::KNURL), c + Vec3::Z * 0.0095, Vec3::Z, &[[0.0055, 0.0], [0.0055, 0.007], [0.0045, 0.008], [0.0, 0.008]], 14, false);
        let k = b.add("panel knob", G_FUSE, m, Anim::Fixed, c, Vec3::Z, None);
        h.at = (Vec3::from(h.at) + Vec3::Z * 0.009).to_array();
        b.pieces[k].hotspot = Some(h);
    }
}

// ---------------------------------------------------------------- what the eye sees

/// Whether a look from `eye` along `d` (body frame) ends on something in the
/// cabin (true) or goes out through the glass (false).
pub fn sees_inside(eye: Vec3, d: Vec3) -> bool {
    let mut t = 0.02;
    while t < 3.5 {
        let p = eye + d * t;
        if p.z > FIREWALL_Z && p.z < TAILCONE_Z && p.y < floor_y(p.z) + 0.01 {
            return true;
        }
        if p.z < PANEL_Z + 0.01 && p.y < GLARESHIELD_Y {
            return true;
        }
        if p.z < FIREWALL_Z || p.z > TAILCONE_Z - 0.10 || p.x.abs() > wall_x(p.z, p.y) {
            return window(p) >= 0.0;
        }
        t += 0.015 + t * 0.01;
    }
    true
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn instrument_cutouts_do_not_overlap() {
        let g: Vec<_> = GAUGES.iter().filter(|g| g.3 > 0.0).collect();
        for (i, &&(_, x, y, r)) in g.iter().enumerate() {
            for &&(_, xx, yy, rr) in &g[i + 1..] {
                assert!(Vec2::new(x - xx, y - yy).length() > r + rr + 0.012, "overlapping instruments at {x},{y} / {xx},{yy}");
            }
            assert!(x.abs() + r + 0.008 < wall_x(PANEL_Z, y), "instrument at {x},{y} off the panel");
            let s = STACK;
            assert!(x + r < s[0] - 0.005 || x - r > s[1] + 0.005 || y + r < s[2], "instrument at {x},{y} in the radio stack");
        }
    }

    /// The seats and the floor sit over the wing's skin where it runs in
    /// through the cabin sides, and the front cushion fits the pilot.
    #[test]
    fn floor_and_seats_clear_the_wing() {
        let f = wing_foil();
        for i in 0..=60 {
            let z = -0.47 + 2.1 * i as f32 / 60.0;
            for x in [0.26f32, 0.4, 0.55] {
                let k = wing_sect(x);
                if z > k.le.z + 0.01 && z < k.le.z + k.c - 0.01 {
                    assert!(floor_y(z) > wing_top(&f, x, z).y + 0.005, "floor under the wing at z {z}, x {x}");
                }
            }
        }
        let cushion = -0.075 - 0.075;
        assert!(cushion > floor_y(0.10), "front seat cushion bottom {cushion} under the floor {}", floor_y(0.10));
        let sitting = EYE.y - (-0.075);
        assert!(sitting > 0.58 && sitting < 0.80, "eye {sitting} m over the cushion");
    }

    /// No part of any seat goes through the lining (and so the skin) where
    /// the cabin tapers aft: the back pair once showed outside the airframe.
    #[test]
    fn seats_stay_inside_the_cabin() {
        let mut m = Mesh::default();
        seats(&mut m);
        for v in &m.v {
            let p = Vec3::from(v.pos);
            let s = sec(p.z);
            assert!(p.x.abs() < wall_x(p.z, p.y), "seat vertex {p} outside the cabin wall {}", wall_x(p.z, p.y));
            assert!(p.y < s.mid + (s.top - s.mid) * 0.96, "seat vertex {p} through the headliner");
            assert!(p.z < TAILCONE_Z - 0.10, "seat vertex {p} behind the baggage bulkhead");
        }
    }
}
