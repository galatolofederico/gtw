//! The G650's flight deck: Honeywell's PlaneView II (Primus Epic) for two
//! pilots, laid out from photographs of real ones and the documents below.
//!
//! - The four 14.1-inch landscape display units side by side across the panel
//!   (pilot's PFD, two MFDs, copilot's PFD; DU-1310 cases 312 x 254 mm), the
//!   knee panels under the outboard ones with the chronometers, and between
//!   them the centre panel: the L and R fuel control switches, the brake
//!   pressure indicator, IRS mode select and the landing gear handle.
//! - The glareshield: the Guidance Panel in the middle, a Standby
//!   Multifunction Controller each side of it, display keypads and the master
//!   warning and caution switchlights at the ends.
//! - The pedestal: two MCDUs and between them the two thrust levers with
//!   their piggy-back reverse levers; behind them the speed brake handle
//!   (left), the flap handle (right, 0/10/20/39), the trim panel, the third
//!   MCDU, the park/emergency brake lever and the RAT handle.
//! - The overhead, forward to aft: the lights strip (exterior lights, pass
//!   warn, cockpit lights, probe and windshield heat); engine start, main
//!   batteries and the electrical power panels on the left; anti-ice, fuel,
//!   APU and fire extinguishing in the middle; cabin pressure and
//!   temperature, bleed air on the right; the test, display switching and
//!   fire detection panels; the circuit breakers behind the pilots' heads.
//! - The Gulfstream yokes on their columns, rudder pedals, the side
//!   consoles with the cursor control devices (and the tiller on the
//!   captain's), the crew seats, the door to the cabin.
//!
//! # Sources
//!
//! - Photographs (layout, proportions, panel titles): Wikimedia Commons
//!   "Cockpit G650 N520GA.jpg" (G650), "Gulfstream G650ER, EBACE 2018, Le
//!   Grand-Saconnex (BL7C0735).jpg" and "(BL7C0736).jpg" (a G650ER: the same
//!   flight deck; the second is the whole overhead).
//! - FAA MMEL GVI (G650/G650ER), rev. 6a draft,
//!   https://www.faa.gov/aircraft/draft_docs/mmel/MMEL_GVI_Rev_6a_Draft.pdf :
//!   4 DUs, 2 SMCs, 2 CCDs, 3 MCDUs, the Guidance Panel; TOGA and autothrottle
//!   disconnect buttons on the power levers; FLAP, SPEED BRAKE and PARK/EMERG
//!   BRAKE handles; continuous ignition, engine selector and start switches.
//! - J. Albright, "G450 – G550 to G650 Differences" (2015),
//!   https://static1.squarespace.com/static/61099203fca27a27990251fa/t/6292c60efb47591c59f3e96e/1653786127622/g450_g550_g650_differences_ni_v_4-4.pdf :
//!   the SMC replacing the display controller and standby instruments, baro
//!   and altitude preselect on the Guidance Panel, the lever-type park brake,
//!   the FCS trim panel, the alternate gear handle under DU2, flaps 0/10/20/39,
//!   Collins HUD II (left out here: its combiner in front of the captain's
//!   eye read as out of place, so the deck is built as if without the option).
//! - Aviation Week, "Honeywell Primus Epic PlaneView II Avionics" (2013): four
//!   14.1-in landscape LCDs, PFDs outboard, MFDs inboard. Honeywell's DU-1310
//!   page: 12.3 x 10 x 8 in case.
//!
//! Estimated, not measured: every position is scaled from the photographs by
//! the DU width and fitted to the nose (`super::section`, the glazing). Not
//! seen in any photograph and placed by Gulfstream practice: the aft
//! pedestal (flap handle right, park brake and RAT handle, trim panel, third
//! MCDU) and the tiller on the left console. The keypad labels and the GP's
//! legends are approximations. The SMC's screen is about 5 in (FlightGlobal).
//!
//! # What works
//!
//! Every control below is a mouse (and gamepad aim) target, `aircraft::Hotspot`,
//! doing what its action's key does, and moves with the simulated state:
//! thrust levers (wheel), reverse levers (hold), speed brake (click), flaps
//! (wheel, a detent a notch), gear (click), the L and R fuel control switches
//! and the L and R main battery switches (click; the sim has one fuel control
//! for both engines and one battery master, which stay on while either
//! switch of the pair is on: `Plane::pair_off`), the L and R ENG start
//! buttons on the ENGINE START panel (click: the sim's one starter for both
//! engines, latched until they idle), park brake (click), pitch trim on the
//! pedestal and the captain's yoke (wheel), the captain's toe brakes (hold),
//! the L and R LDG landing light switches (click), and on the Guidance Panel
//! AP, HDG, NAV, APPR, ALT, VS and A/T (click), the heading, altitude, speed
//! and course knobs and the pitch wheel (wheel), baro to the QNH (click). A
//! cold and dark start: BATT, FUEL CONTROL to RUN, START, thrust levers.
//!
//! Lit from the simulated state: the master WARNING (red) and CAUTION
//! (amber) while the CAS list holds a warning or a caution, the zone
//! temperature and bus tie AUTO caps while the bus is powered, the battery
//! and landing light switches' ON legends.
//!
//! Decorative: the APU, fire handles and extinguishers, the electrical panels
//! other than the batteries, fuel pumps, hydraulics, bleeds, pressurisation,
//! anti-ice and heat, the other lights, the engine start master (CRANK,
//! START), continuous ignition, circuit breakers and tests, IRS, the
//! alternate gear handle, the MCDU, SMC and keypad keys, the CCDs, audio
//! panels, the FD, YD, BC, FLCH, VNAV and the second course and baro knobs,
//! TOGA and the autothrottle and autopilot disconnects, the first officer's
//! trim switch and toe brakes, the aileron and rudder trim switches, the RAT
//! handle, the tiller (it turns with the nose wheel but cannot be clicked).
//! The status legends the real caps light (amber OFF, FAULT and the like) are
//! not simulated: those caps stay dark.
//!
//! The overhead's panel titles, switch names and circuit breaker labels are
//! read from the photographs (BL7C0735, BL7C0736). Where a name could not be
//! read the cap or breaker is left unlabelled; the audio and lighting panels
//! on the side consoles, which no photograph shows, carry blank caps and
//! unnamed knobs.
//!
//! # The displays (for `deck650.wgsl`)
//!
//! Each screen is a `mat::DISPLAY` quad (`airframe::Mesh` via
//! `b747::deck::screen_quad`) whose `extra` is one of `screen::*` (60 and up;
//! plane.wgsl sends those to `display650`). Its uv runs -1..1 across and up the
//! screen as the pilot sees it, whatever its shape: `screen::HALF` gives each
//! one's half size in metres, so a shader wanting square units scales u by
//! the aspect (4/3 on the DUs).
//!
//! deck650.wgsl draws Honeywell PlaneView II formats on them from the
//! simulated state (PFDs with synthetic vision, map and engine strip, CAS and
//! the Flight Controls synoptic, standby, MCDU pages, the Guidance Panel's
//! windows and mode lights); its
//! header lists what each shows, the sources and what is simplified. The
//! autopilot and systems state reaches it through `Globals::cockpit` rows
//! 8-11 (main.rs `cockpit_globals`).
//!
//! Body frame (+X right, +Y up, -Z forward); positions are given as drawing
//! stations (`p(x, d, h)`: m right, m behind the radome's tip, m above the
//! ground at rest).
use super::*;
use crate::aircraft::b747::deck::{legend_mat, rect, screen_quad, sid, style, Face};
use crate::aircraft::fa18::pit as hp;
use crate::aircraft::Hotspot;
use glam::{Quat, Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI};
use std::sync::OnceLock;

// ---------------------------------------------------------------- the displays

/// What the screens show (`mat::DISPLAY` extra); keep in step with deck650.wgsl.
pub mod screen {
    /// The display units, left to right: the pilot's PFD, the left and right MFDs, the copilot's PFD.
    pub const PFD_L: f32 = 60.0;
    pub const MFD_L: f32 = 61.0;
    pub const MFD_R: f32 = 62.0;
    pub const PFD_R: f32 = 63.0;
    /// The standby multifunction controllers on the glareshield (the left
    /// shows the standby attitude, the right a menu page).
    pub const SMC_L: f32 = 64.0;
    pub const SMC_R: f32 = 65.0;
    /// The MCDUs: left and right on the forward pedestal, the third behind them.
    pub const MCDU_L: f32 = 66.0;
    pub const MCDU_R: f32 = 67.0;
    pub const MCDU_C: f32 = 68.0;
    /// The Guidance Panel's windows: speed, heading, vertical speed, altitude.
    pub const GP_SPD: f32 = 69.0;
    pub const GP_HDG: f32 = 70.0;
    pub const GP_VS: f32 = 71.0;
    pub const GP_ALT: f32 = 72.0;
    /// The overhead's cabin altitude readout, the main batteries' volt and amp readouts.
    pub const CABIN_ALT: f32 = 73.0;
    pub const BATT_L: f32 = 74.0;
    pub const BATT_R: f32 = 75.0;
    /// The chronometers on the knee panels.
    pub const CLOCK_L: f32 = 76.0;
    pub const CLOCK_R: f32 = 77.0;
    /// The brake pressure indicator: two vertical bars, inboard and outboard.
    pub const BAPI: f32 = 78.0;
    /// The bars under the Guidance Panel's mode buttons that light when the
    /// mode is engaged: 80 + `GP_MODES` index (AP, YD, HDG, NAV, APPR, BC, ALT,
    /// VS, FLCH, VNAV, A/T, PFD CMD).
    pub const GP_LIGHT: f32 = 80.0;
    pub const GP_MODES: [&str; 12] = ["AP", "YD", "HDG", "NAV", "APPR", "BC", "ALT", "VS", "FLCH", "VNAV", "A/T", "CMD"];
    /// Half width and half height (m) of each screen's quad, by id - 60.
    pub const HALF: [[f32; 2]; 19] = [
        [0.1433, 0.1075], [0.1433, 0.1075], [0.1433, 0.1075], [0.1433, 0.1075],
        [0.050, 0.0375], [0.050, 0.0375],
        [0.044, 0.038], [0.044, 0.038], [0.044, 0.038],
        [0.019, 0.0068], [0.019, 0.0068], [0.019, 0.0068], [0.024, 0.0068],
        [0.020, 0.0085], [0.015, 0.0055], [0.015, 0.0055],
        [0.013, 0.0055], [0.013, 0.0055],
        [0.011, 0.028],
    ];
    /// A mode light's half size.
    pub const GP_LIGHT_HALF: [f32; 2] = [0.0075, 0.0012];
}

// ---------------------------------------------------------------- materials

/// Gulfstream's blue-grey panels (linear), and darker for boxes and bezels.
const PANEL_COL: Vec3 = Vec3::new(0.115, 0.125, 0.150);
const PANEL: Mat = mt(mat::PLASTIC, 0.115, 0.125, 0.150).pat(pat::PANEL);
const PANEL_DK: Mat = mt(mat::PLASTIC, 0.060, 0.064, 0.074).pat(pat::PANEL);
const DU_BEZEL: Mat = mt(mat::PLASTIC, 0.042, 0.044, 0.050).pat(pat::PANEL);
const MASK: Mat = mt(mat::PLASTIC, 0.010, 0.010, 0.011);
const GLARE: Mat = mt(mat::FABRIC, 0.024, 0.024, 0.026);
const GLARE_FACE: Mat = mt(mat::PLASTIC, 0.040, 0.042, 0.048).pat(pat::PANEL);
const LINER: Mat = mt(mat::SATIN, 0.36, 0.355, 0.335).pat(pat::PIT);
const LINER_DK: Mat = mt(mat::SATIN, 0.12, 0.122, 0.125).pat(pat::PIT);
const TRIM: Mat = mt(mat::PLASTIC, 0.30, 0.298, 0.285);
const SEAL: Mat = mt(mat::RUBBER, 0.02, 0.02, 0.022);
const CARPET: Mat = mt(mat::FABRIC, 0.045, 0.046, 0.052).pat(pat::CARPET);
/// The crew seats' leather (a light grey: a fictional but common choice).
const LEATHER: Mat = mt(mat::FABRIC, 0.42, 0.40, 0.36).pat(pat::SEAT);
const LEATHER_DK: Mat = mt(mat::FABRIC, 0.10, 0.098, 0.095).pat(pat::SEAT);
const SEAT_SHELL: Mat = mt(mat::PLASTIC, 0.07, 0.072, 0.078);
const YOKE: Mat = mt(mat::PLASTIC, 0.022, 0.022, 0.024);
const KEY: Mat = mt(mat::PLASTIC, 0.030, 0.032, 0.036);
const POLISH: Mat = mt(mat::ALU, 0.78, 0.79, 0.80);
const RED_HANDLE: Mat = mt(mat::PAINT, 0.42, 0.03, 0.02);

// ---------------------------------------------------------------- where things are

/// Body point at `x` right, drawing station `d` behind the radome's tip, `h` above the ground.
fn p(x: f32, d: f32, h: f32) -> Vec3 {
    Vec3::new(x, by(h), bz(d))
}
/// How far the lining stands inside the skin.
const INSET: f32 = 0.045;
/// The wall ahead of the pedals and the bulkhead with the door (drawing stations).
const D_FWD: f32 = 1.98;
const D_AFT: f32 = 4.40;
/// The pilots sit either side of the centreline.
const SEAT_X: f32 = 0.52;
/// The main panel: its face through the middle of the display units, leaning back 12 degrees.
fn panel_face() -> Face {
    let t = 12f32.to_radians();
    Face::new(p(0.0, 2.330, 2.860), Vec3::X, Vec3::new(0.0, t.cos(), -t.sin()))
}
/// The display units' middles along the panel (u) and their cases' half size.
const DU_U: [f32; 4] = [-0.483, -0.161, 0.161, 0.483];
const DU_HALF: [f32; 2] = [0.156, 0.127];
/// The glareshield's face: from the lip (top) down to its lower edge, leaning back 20 degrees.
const LIP: [f32; 2] = [2.335, 3.125];
fn glare_face() -> Face {
    let t = 20f32.to_radians();
    Face::new(p(0.0, LIP[0] + GLARE_HH * t.sin(), LIP[1] - GLARE_HH * t.cos()), Vec3::X, Vec3::new(0.0, t.cos(), -t.sin()))
}
const GLARE_HH: f32 = 0.045;
/// The pedestal: the forward part's half width, and its stations and
/// heights: its top at the centre panel's foot, the MCDUs' lower edge, the
/// aft part's top. (No drawing gives the real heights. They are set against
/// the seats: the floor is at 2.20, the cushion's top 0.38 m over it and the
/// armrests 0.58 m; the aft pedestal's top is a little over the cushion and
/// the thrust levers' grips stand about 0.50 m over the floor, a hand's
/// width under the armrests, about level with the yokes' hubs, as the
/// photographs show them. The MCDUs hang from the centre panel's foot, which
/// is as high as they can be.)
const PED_W: f32 = 0.30;
const PED_W_AFT: f32 = 0.24;
const PED_D: [f32; 4] = [2.385, 2.56, 2.70, 3.42];
const PED_H: [f32; 3] = [2.555, 2.42, 2.60];
/// The thrust levers (x), their pivot inside the pedestal and the grips' distance from it.
const THR_X: [f32; 2] = [-0.050, 0.050];
const THR_PIVOT: [f32; 2] = [2.565, 2.33];
const THR_ARM: f32 = 0.37;
/// The park/emergency brake lever (x).
const PB_X: f32 = -0.145;
/// The quadrant's cover over the levers' pivot: its radius, half width and the
/// centre block's top under it. (Its top stays under the MCDUs' top edge, so
/// that from the left seat it hides little of the right MCDU; the levers
/// stand about 0.15 m out of it.)
const QUAD_R: f32 = 0.215;
const QUAD_W: f32 = 0.128;
const QUAD_BASE: f32 = 2.515;
/// The side consoles' inner edge and top, front and back.
const CON_X: f32 = 0.86;
const CON_H: f32 = 2.78;
const CON_D: [f32; 2] = [2.62, 3.55];
/// The overhead's underside at x = 0: drawing stations and heights of its three faces' ends,
/// and each face's half width.
const OH_D: [f32; 4] = [2.68, 2.80, 3.35, 3.92];
const OH_W: [f32; 3] = [0.335, 0.400, 0.420];
fn oh_h(d: f32) -> f32 {
    skin_height(bz(d), 0.42, true) + CG_H - INSET - 0.055
}
fn oh_face(k: usize) -> (Face, f32) {
    let (a, b) = (p(0.0, OH_D[k], oh_h(OH_D[k])), p(0.0, OH_D[k + 1], oh_h(OH_D[k + 1])));
    (Face::new((a + b) * 0.5, Vec3::X, b - a), (b - a).length() * 0.5)
}

// ---------------------------------------------------------------- the skin, the lining, the glazing

fn radial(q: Vec3) -> Vec3 {
    Vec3::new(q.x, q.y - section(q.z).mid, 0.0).try_normalize().unwrap_or(Vec3::Y)
}
/// The point `inset` inside the skin, in from its point nearest q.
fn lin_pt(q: Vec3, inset: f32) -> Vec3 {
    let s = to_skin(q);
    s - radial(s) * inset
}
/// The lining's half width at station z, height y; its height x out.
fn lin_x(z: f32, y: f32) -> f32 {
    (skin_half_width(z, y) - INSET).max(0.0)
}
fn lin_top(z: f32, x: f32) -> f32 {
    skin_height(z, x, true) - INSET
}

/// The four openings: the left and right windshield panes and side windows,
/// and each one's outline unrolled on the skin: (station, angle round the section).
struct Glaze {
    rings: Vec<Vec<Vec3>>,
    unrolled: Vec<Vec<Vec2>>,
}
/// A point's station and its angle round the section (from the right, about its widest point's height).
fn unroll(q: Vec3) -> Vec2 {
    Vec2::new(q.z + CG_D, (q.y - section(q.z).mid).atan2(q.x))
}
fn glaze() -> &'static Glaze {
    static G: OnceLock<Glaze> = OnceLock::new();
    G.get_or_init(|| {
        let rings = vec![windshield_outline(-1.0), windshield_outline(1.0), side_window_outline(-1.0), side_window_outline(1.0)];
        let unrolled = rings.iter().map(|r| r.iter().map(|&q| unroll(q)).collect()).collect();
        Glaze { rings, unrolled }
    })
}
fn ring_dist(r: &[Vec3], q: Vec3) -> f32 {
    let n = r.len();
    (0..n).map(|i| {
        let (a, b) = (r[i], r[(i + 1) % n]);
        let e = b - a;
        let t = ((q - a).dot(e) / e.length_squared().max(1e-12)).clamp(0.0, 1.0);
        (a + e * t - q).length()
    }).fold(f32::MAX, f32::min)
}
/// Signed distance (m, about: negative in an opening) from a point of the
/// skin to the glazing.
fn glaze_sd(q: Vec3) -> f32 {
    let g = glaze();
    let u = unroll(q);
    let inside = g.unrolled.iter().any(|r| sd_poly(u, r) < 0.0);
    let dist = g.rings.iter().map(|r| ring_dist(r, q)).fold(f32::MAX, f32::min);
    if inside { -dist } else { dist }
}
/// How wide the trim round the opening nearest q is there (m): half way to
/// the next opening at the posts, 5 cm elsewhere.
fn trim_w(q: Vec3) -> f32 {
    let g = glaze();
    let k = (0..g.rings.len()).min_by(|&x, &y| ring_dist(&g.rings[x], q).total_cmp(&ring_dist(&g.rings[y], q))).unwrap();
    (other_dist(k, q) * 0.5 - 0.002).clamp(0.012, 0.05)
}
/// How far from the openings the lining is cut away: its edge under the trim.
fn cut_at(q: Vec3) -> f32 {
    (trim_w(q) - 0.008).max(0.004)
}
/// How far a point is from the openings other than `k`.
fn other_dist(k: usize, q: Vec3) -> f32 {
    glaze().rings.iter().enumerate().filter(|(j, _)| *j != k).map(|(_, r)| ring_dist(r, q)).fold(f32::MAX, f32::min)
}
/// Turn the normals of the vertices from `from` on to face the point `toward(p)` gives.
fn face_to(m: &mut Mesh, from: usize, toward: impl Fn(Vec3) -> Vec3) {
    for v in &mut m.v[from..] {
        let q = Vec3::from(v.pos);
        if Vec3::from(v.nrm).dot(toward(q) - q) < 0.0 {
            v.nrm = (-Vec3::from(v.nrm)).into();
        }
    }
}
/// A point on the fuselage's axis level with q: what the lining faces.
fn axis_of(q: Vec3) -> Vec3 {
    Vec3::new(0.0, section(q.z).mid, q.z)
}
/// The windshield's sill seen from above: the station it runs along at |x|.
fn sill_d(x: f32) -> f32 {
    static S: OnceLock<Vec<Vec2>> = OnceLock::new();
    let s = S.get_or_init(|| {
        let mut v: Vec<Vec2> = windshield_outline(1.0).iter().filter(|q| q.y + CG_H < WS_SILL + 0.012 && q.z + CG_D < 2.66).map(|q| Vec2::new(q.x, q.z + CG_D)).collect();
        v.sort_by(|a, b| a.x.total_cmp(&b.x));
        v
    });
    let x = x.abs();
    match s.iter().position(|q| q.x >= x) {
        Some(0) => s[0].y,
        Some(i) => { let (a, b) = (s[i - 1], s[i]); a.y + (b.y - a.y) * (x - a.x) / (b.x - a.x).max(1e-6) }
        None => s.last().unwrap().y,
    }
}

pub fn build(b: &mut Builder) {
    let mut l = Mesh::default();
    lining(&mut l);
    floor_and_bulkheads(&mut l);
    b.fixed("flight deck lining", G_ENGINE, l);
    // (The window trims cast no occlusion: they would darken the lining
    // under them, and that shows as a stepped shadow past their edges.)
    let mut t = Mesh::default();
    trims(&mut t);
    let k = b.fixed("window trims", G_ENGINE, t);
    b.pieces[k].occluder = false;

    let mut m = Mesh::default();
    let f = panel_face();
    main_panel(&mut m, &f);
    glareshield(&mut m, b);
    b.fixed("instrument panel", G_ENGINE, m);

    let mut c = Mesh::default();
    pedestal(&mut c);
    b.fixed("pedestal", G_ENGINE, c);

    let mut o = Mesh::default();
    overhead(&mut o, b);
    b.fixed("overhead panel", G_ENGINE, o);

    let mut s = Mesh::default();
    side_consoles(&mut s);
    b.fixed("consoles", G_ENGINE, s);

    let mut st = Mesh::default();
    for sx in [-1.0f32, 1.0] {
        seat(&mut st, sx);
    }
    b.fixed("crew seats", G_ENGINE, st);

    controls(b, &f);
}

// ---------------------------------------------------------------- lining, window trims, floor

fn lining(m: &mut Mesh) {
    let (nz, na) = (140usize, 240usize);
    let mut rows: Vec<Vec<SP>> = Vec::new();
    // Each vertex: 0 kept, 1 moved onto the cut's edge, 2 deep in an opening (dropped).
    let mut state: Vec<u8> = Vec::new();
    for i in 0..=nz {
        let z = bz(D_FWD + (D_AFT - D_FWD) * i as f32 / nz as f32);
        // Round from the floor on the right over the crown to the floor on the left.
        let (mut lo, mut hi) = (-FRAC_PI_2, FRAC_PI_2);
        for _ in 0..30 {
            let a = (lo + hi) * 0.5;
            if skin_point(z, a, 0.0).y < FLOOR_Y - 0.02 { lo = a } else { hi = a }
        }
        let a0 = (lo + hi) * 0.5;
        let mut row = Vec::new();
        let mut arc = 0.0;
        let mut prev = Vec3::ZERO;
        for j in 0..=na {
            let a = a0 + (PI - 2.0 * a0) * j as f32 / na as f32;
            let mut q = skin_point(z, a, 0.0);
            // Near an opening, the vertex slides along the skin onto the
            // cut's edge, so the edge is smooth (a ragged one would show past the trim).
            let (sd, cut) = (glaze_sd(q), cut_at(q));
            let st = if sd >= cut { 0 } else if sd > cut - 0.045 { 1 } else { 2 };
            if st == 1 {
                for _ in 0..4 {
                    let (sd, cut) = (glaze_sd(q), cut_at(q));
                    if (sd - cut).abs() < 0.0005 {
                        break;
                    }
                    let e = 0.003;
                    let f = |p: Vec3| glaze_sd(to_skin(p));
                    let g = Vec3::new(f(q + Vec3::X * e) - f(q - Vec3::X * e), f(q + Vec3::Y * e) - f(q - Vec3::Y * e), f(q + Vec3::Z * e) - f(q - Vec3::Z * e)) / (2.0 * e);
                    let nr = radial(q);
                    let gt = g - nr * g.dot(nr);
                    if gt.length_squared() < 1e-6 {
                        break;
                    }
                    q = to_skin(q + gt * ((cut - sd) / gt.length_squared()));
                }
            }
            let l = q - radial(q) * INSET;
            if j > 0 {
                arc += (l - prev).length();
            }
            prev = l;
            row.push((l, [z, arc], 0.0));
            state.push(st);
        }
        rows.push(row);
    }
    let mut g = Mesh::default();
    g.grid(&LINER, &rows, false, false);
    face_to(&mut g, 0, axis_of);
    // Light grey above, a darker band low on the walls.
    for v in &mut g.v {
        let h = v.pos[1] + CG_H;
        let c = LINER.col.lerp(LINER_DK.col, smoothstep(2.78, 2.70, h));
        v.col = [c.x, c.y, c.z, 1.0];
    }
    // Leave out the triangles with a corner deep in an opening, or all three on the cut's edge.
    g.i = g.i.chunks(3).filter(|t| t.iter().all(|&k| state[k as usize] < 2) && t.iter().any(|&k| state[k as usize] == 0)).flatten().copied().collect();
    m.append(&g);
}

/// A closed ring resampled every `step` metres, back on the skin.
fn resample(r: &[Vec3], step: f32) -> Vec<Vec3> {
    let n = r.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (r[i], r[(i + 1) % n]);
        let k = ((b - a).length() / step).ceil().max(1.0) as usize;
        for j in 0..k {
            out.push(to_skin(a.lerp(b, j as f32 / k as f32)));
        }
    }
    out
}

/// The moulded trim round each opening: a rubber seal against the glass, the
/// reveal in from the skin to the lining, a rounded surround over the
/// lining's cut edge; where two openings are close (the posts) the surrounds
/// meet half way.
fn trims(m: &mut Mesh) {
    let g = glaze();
    for k in 0..4 {
        let ring = resample(&g.rings[k], 0.012);
        let n = ring.len();
        let sd = |q: Vec3| glaze_sd(to_skin(q));
        let mut seal: Vec<Vec<SP>> = Vec::new();
        let mut trim: Vec<Vec<SP>> = Vec::new();
        let mut arc = 0.0;
        for i in 0..=n {
            let q = ring[i % n];
            if i > 0 {
                arc += (q - ring[i - 1]).length();
            }
            let nr = radial(q);
            let e = 0.004;
            let gv = Vec3::new(sd(q + Vec3::X * e) - sd(q - Vec3::X * e), sd(q + Vec3::Y * e) - sd(q - Vec3::Y * e), sd(q + Vec3::Z * e) - sd(q - Vec3::Z * e));
            let gt = (gv - nr * gv.dot(nr)).normalize_or_zero();
            let w = (other_dist(k, q) * 0.5 - 0.002).clamp(0.012, 0.05);
            let at = |a: f32, inset: f32| lin_pt(q + gt * a, inset);
            seal.push(vec![(at(-0.006, 0.0075), [arc, 0.0], 0.0), (at(-0.001, 0.010), [arc, 0.004], 0.0), (at(0.002, 0.014), [arc, 0.008], 0.0)]);
            trim.push(vec![
                (at(0.002, 0.014), [arc, 0.0], 0.0),
                (at(0.006, INSET - 0.005), [arc, 0.03], 0.0),
                (at(0.013, INSET - 0.009), [arc, 0.04], 0.0),
                (at(w * 0.6, INSET - 0.010), [arc, 0.04 + w * 0.6], 0.0),
                (at(w - 0.003, INSET - 0.009), [arc, 0.04 + w], 0.0),
                (at(w, INSET - 0.003), [arc, 0.05 + w], 0.0),
                (at(w + 0.002, INSET + 0.001), [arc, 0.06 + w], 0.0),
            ]);
        }
        for (rows, mm) in [(seal, SEAL), (trim, TRIM)] {
            let start = m.v.len();
            m.grid(&mm, &rows, false, false);
            face_to(m, start, axis_of);
        }
    }
}

fn floor_and_bulkheads(m: &mut Mesh) {
    let s0 = m.v.len();
    m.patch(&CARPET, 40, 12, false, |u, v| {
        let z = bz(D_FWD + 0.005 + (D_AFT - D_FWD - 0.01) * u);
        let x = (v * 2.0 - 1.0) * (lin_x(z, FLOOR_Y) + 0.01);
        (Vec3::new(x, FLOOR_Y, z), [x, z], 0.0)
    });
    face_to(m, s0, |q| q + Vec3::Y);
    // Seat tracks either side of each seat.
    for sx in [-1.0f32, 1.0] {
        for dx in [-0.16f32, 0.16] {
            m.rbox(&ALU.pat(pat::KNURL), p(sx * SEAT_X + dx, 3.10, FLOOR_Y + CG_H + 0.006), Vec3::new(0.014, 0.006, 0.36), 0.002, Quat::IDENTITY);
        }
    }
    // The wall ahead of the pedals, up under the glareshield.
    let zf = bz(D_FWD);
    let s0 = m.v.len();
    m.patch(&LINER_DK, 12, 8, false, |u, v| {
        let y = FLOOR_Y - 0.02 + (by(3.03) - FLOOR_Y + 0.02) * v;
        (Vec3::new((u * 2.0 - 1.0) * (lin_x(zf, y) + 0.01), y, zf), [u, y], 0.0)
    });
    face_to(m, s0, |q| q + Vec3::Z);
    // The bulkhead behind, with the door to the cabin.
    let za = bz(D_AFT) - 0.002;
    let s0 = m.v.len();
    m.patch(&LINER, 16, 12, false, |u, v| {
        let y = FLOOR_Y - 0.02 + (lin_top(za, 0.0) + 0.025 - FLOOR_Y + 0.02) * v;
        (Vec3::new((u * 2.0 - 1.0) * (lin_x(za, y) + 0.02), y, za), [u, y], 0.0)
    });
    face_to(m, s0, |q| q - Vec3::Z);
    let d = Face::new(p(0.0, D_AFT - 0.006, FLOOR_Y + CG_H + 0.90), -Vec3::X, Vec3::Y);
    m.rbox(&mt(mat::PLASTIC, 0.32, 0.315, 0.30), d.at_h(0.0, 0.0, 0.006), Vec3::new(0.30, 0.88, 0.006), 0.004, d.rot());
    let frame = [d.at_h(-0.32, -0.90, 0.004), d.at_h(-0.32, 0.90, 0.004), d.at_h(0.32, 0.90, 0.004), d.at_h(0.32, -0.90, 0.004)];
    m.tube(&TRIM, &frame, &|_| 0.014, 8, false);
    m.rbox(&POLISH, d.at_h(-0.24, 0.02, 0.022), Vec3::new(0.012, 0.055, 0.012), 0.006, d.rot());
    m.disc(&mt(mat::GLASS, 0.0, 0.0, 0.0), d.at_h(0.0, 0.52, 0.0135), d.n(), Vec3::Y, 0.035, 18);
    rect(m, &legend_mat(style::PLACARD, 0, Vec3::new(0.80, 0.79, 0.75)), d.at_h(0.0, 0.25, 0.0125), d.ax, d.ay, 0.07, 0.04);
    // A folding jump seat stowed against the bulkhead, right of the door.
    let j = Face::new(p(0.60, D_AFT - 0.006, FLOOR_Y + CG_H + 0.75), -Vec3::X, Vec3::Y);
    m.rbox(&LEATHER_DK, j.at_h(0.0, 0.10, 0.04), Vec3::new(0.18, 0.28, 0.04), 0.03, j.rot());
    m.rbox(&SEAT_SHELL, j.at_h(0.0, -0.25, 0.05), Vec3::new(0.19, 0.06, 0.05), 0.02, j.rot());
}

// ---------------------------------------------------------------- what sits on a panel

/// A printed legend in white on the panel, glyph height `h`. (One longer
/// than the legend table's strings, 15 glyphs, is printed in pieces split at
/// its spaces.)
fn word(m: &mut Mesh, f: &Face, u: f32, v: f32, h: f32, s: &str) {
    let len = s.chars().count();
    if len > 15 {
        let left = u - (len as f32 * 1.45 - 0.45) * 0.25 * h;
        let mut start = 0;
        for piece in split15(s) {
            let n = piece.chars().count() as f32;
            word(m, f, left + (start as f32 * 1.45 + (n * 1.45 - 0.45) * 0.5) * 0.5 * h, v, h, piece);
            start += piece.chars().count() + 1;
        }
        return;
    }
    let n = len.max(1) as f32;
    let hw = (n * 1.45 - 0.45) * h * 0.25 + h * 0.3;
    rect(m, &legend_mat(style::TEXT, sid(s), PANEL_COL), f.at_h(u, v, 0.0004), f.ax, f.ay, hw, h * 0.72);
}
/// A panel plate standing proud with a screw in each corner; its face is 4.3 mm up.
fn plate(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, seed: f32) {
    let (cu, cv) = ((u0 + u1) * 0.5, (v0 + v1) * 0.5);
    let (hu, hv) = ((u1 - u0) * 0.5 - 0.0007, (v1 - v0) * 0.5 - 0.0007);
    m.rbox(&PANEL, f.at_h(cu, cv, 0.0015), Vec3::new(hu, hv, 0.0028), 0.0012, f.rot());
    for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
        m.screw(&mt(mat::SCREW, 0.20, 0.21, 0.23).x(1.0), f.at_h(cu + du * (hu - 0.005), cv + dv * (hv - 0.005), 0.0044), f.n(), 0.0022, seed + k as f32);
    }
}
/// The plate's face, for what sits on it.
fn on(f: &Face) -> Face {
    f.sub(0.0, 0.0, 0.0043)
}
/// A dimmed rule round a group of controls, as Gulfstream's panels are printed.
fn frame_lines(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32) {
    let ink = mt(mat::PLASTIC, 0.40, 0.41, 0.43);
    for (a, b) in [((u0, v0), (u1, v0)), ((u0, v1), (u1, v1))] {
        rect(m, &ink, f.at_h((a.0 + b.0) * 0.5, a.1, 0.0003), f.ax, f.ay, (b.0 - a.0) * 0.5, 0.0005);
    }
    for u in [u0, u1] {
        rect(m, &ink, f.at_h(u, (v0 + v1) * 0.5, 0.0003), f.ax, f.ay, 0.0005, (v1 - v0) * 0.5);
    }
}
/// The G650's switchlight caps (deck747.wgsl `legend`, styles 11-15): one
/// legend and never a made-up second line; string 0 is a blank, dead-front
/// cap (Gulfstream prints most switches' names on the panel above them).
mod cap {
    /// Unlit, the legend centred.
    pub const PLAIN: u32 = 11;
    /// The master WARNING (red) and CAUTION (amber): lit while the CAS list
    /// holds a warning or a caution (deck650.wgsl `master650`).
    pub const WARNING: u32 = 12;
    pub const CAUTION: u32 = 13;
    /// Lit blue while the bus is powered (the AUTO caps).
    pub const BLUE: u32 = 14;
    /// Unlit, the legend in the upper half; the lower half is a lit legend piece of its own.
    pub const TOP: u32 = 15;
}
/// A square switchlight: a dark cap with its legend (a `cap` style).
fn sw(m: &mut Mesh, f: &Face, u: f32, v: f32, hw: f32, hh: f32, st: u32, s: u32) {
    m.rbox(&KEY, f.at_h(u, v, 0.0040), Vec3::new(hw + 0.0016, hh + 0.0016, 0.0040), 0.0012, f.rot());
    rect(m, &legend_mat(st, s, Vec3::new(0.018, 0.019, 0.021)), f.at_h(u, v, 0.0082), f.ax, f.ay, hw, hh);
}
/// A legend split at its spaces into pieces of at most 15 glyphs.
fn split15(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut a = 0;
    let mut last = 0;
    for (i, c) in s.char_indices().chain([(s.len(), ' ')]) {
        if c != ' ' {
            continue;
        }
        if s[a..i].chars().count() > 15 && last > a {
            out.push(&s[a..last]);
            a = last + 1;
        }
        last = i;
    }
    out.push(&s[a..]);
    out
}
/// A knob (black, white pointer).
fn knob(m: &mut Mesh, f: &Face, u: f32, v: f32, r: f32, seed: f32) {
    hp::knob(m, f, u, v, r, seed, false);
}
/// A switch's or a knob's name printed on the panel, as small as it must be
/// to fit `room` (m) across.
fn label(m: &mut Mesh, f: &Face, u: f32, v: f32, room: f32, s: &str) {
    if s.is_empty() {
        return;
    }
    let n = s.chars().count() as f32;
    word(m, f, u, v, (room * 0.47 / ((n * 1.45 - 0.45) * 0.25 + 0.3)).min(0.0022), s);
}
/// A warning light: a lens with its legend, dark until lit.
fn light(m: &mut Mesh, f: &Face, u: f32, v: f32, hw: f32, hh: f32, s: u32, lens: &Mat) {
    m.rbox(&hp::BEZEL, f.at_h(u, v, 0.003), Vec3::new(hw + 0.002, hh + 0.002, 0.003), 0.0012, f.rot());
    m.rbox(lens, f.at_h(u, v, 0.0062), Vec3::new(hw, hh, 0.0006), 0.0004, f.rot());
    rect(m, &legend_mat(cap::PLAIN, s, Vec3::new(0.03, 0.02, 0.02)), f.at_h(u, v, 0.0070), f.ax, f.ay, hw, hh);
}
/// A panel plate under its title with rows of switchlights and knobs, laid
/// out as the photographs show the real one. Rows run from the title down
/// (aft to forward on the overhead), each with a caption ("" for none) and
/// its controls left to right: a dead-front switchlight with its name printed
/// above ("" where the photographs don't show the name), "NAME|LEGEND" for a
/// cap whose legend is lit blue while the bus is powered (AUTO), "*NAME" for
/// a knob, "-" for a gap.
fn panel(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, seed: f32, title: &str, rows: &[(&str, &[&str])]) {
    plate(m, f, u0, v0, u1, v1, seed);
    let g = on(f);
    let mut top = v1;
    if !title.is_empty() {
        word(m, &g, (u0 + u1) * 0.5, v1 - 0.0065, 0.0034, title);
        top -= 0.011;
    }
    let rh = (top - v0) / rows.len().max(1) as f32;
    for (r, (caption, items)) in rows.iter().enumerate() {
        let cv = top - (r + 1) as f32 * rh + (rh * 0.40).max(0.0105);
        if !caption.is_empty() {
            word(m, &g, (u0 + u1) * 0.5, cv + 0.0195, 0.0024, caption);
        }
        let n = items.len().max(1);
        let pitch = ((u1 - u0 - 0.012) / n as f32).min(0.042);
        let start = (u0 + u1) * 0.5 - (n as f32 - 1.0) * pitch * 0.5;
        for (i, it) in items.iter().enumerate() {
            let x = start + i as f32 * pitch;
            if *it == "-" {
                continue;
            }
            if let Some(name) = it.strip_prefix('*') {
                knob(m, &g, x, cv, 0.0055, seed + (r * 13 + i) as f32 * 1.37);
                label(m, &g, x, cv + 0.013, pitch, name);
            } else if let Some((name, lit)) = it.split_once('|') {
                sw(m, &g, x, cv, 0.0085, 0.0075, cap::BLUE, sid(lit));
                label(m, &g, x, cv + 0.013, pitch, name);
            } else {
                sw(m, &g, x, cv, 0.0085, 0.0075, cap::PLAIN, 0);
                label(m, &g, x, cv + 0.013, pitch, it);
            }
        }
    }
}
/// A small readout window (a `screen` id).
fn readout(m: &mut Mesh, f: &Face, u: f32, v: f32, id: f32) {
    let [hw, hh] = screen::HALF[(id - 60.0) as usize];
    m.rbox(&MASK, f.at_h(u, v, 0.0008), Vec3::new(hw + 0.0025, hh + 0.0025, 0.0010), 0.0008, f.rot());
    screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), f.at_h(u, v, 0.0019), f.ax, f.ay, hw, hh);
}

// ---------------------------------------------------------------- the main instrument panel

fn main_panel(m: &mut Mesh, f: &Face) {
    // The panel's outline: as wide as the nose allows across the displays,
    // down to the knee panels outboard and to the pedestal in the middle.
    let lim = |v: f32| {
        let q = f.at(0.0, v);
        lin_x(q.z, q.y) - 0.012
    };
    let (top, knee, bot) = (0.137, -0.245, -0.300);
    let outline = vec![
        Vec2::new(lim(top), top), Vec2::new(-lim(top), top), Vec2::new(-lim(0.0).min(0.74), 0.0), Vec2::new(-lim(knee).min(0.76), knee),
        Vec2::new(-PED_W, knee), Vec2::new(-PED_W, bot), Vec2::new(PED_W, bot), Vec2::new(PED_W, knee), Vec2::new(lim(knee).min(0.76), knee), Vec2::new(lim(0.0).min(0.74), 0.0),
    ];
    m.slab(&PANEL, &hp::wind(outline, false), &[], f.o, f.ax, f.ay, 0.03);
    // The lip under the knee panels.
    for sx in [-1.0f32, 1.0] {
        m.tube(&PANEL_DK, &[f.at_h(sx * (PED_W + 0.01), knee, -0.012), f.at_h(sx * lim(knee).min(0.76), knee, -0.012)], &|_| 0.011, 8, true);
    }

    // The four display units.
    for (k, (u, id)) in DU_U.iter().zip([screen::PFD_L, screen::MFD_L, screen::MFD_R, screen::PFD_R]).enumerate() {
        du(m, f, *u, 0.0, id, k as f32 * 7.0);
    }

    // Knee panels: the chronometers outboard, the alternate gear extension
    // handle (under DU2's left corner) and the CHECK LDG GEAR light.
    for sx in [-1.0f32, 1.0] {
        let (u0, u1) = if sx < 0.0 { (-0.76f32.min(lim(knee)), -PED_W - 0.004) } else { (PED_W + 0.004, 0.76f32.min(lim(knee))) };
        plate(m, f, u0, knee + 0.004, u1, -0.134, 10.0 + sx);
        let g = on(f);
        let cu = sx * 0.655;
        readout(m, &g, cu, -0.176, if sx < 0.0 { screen::CLOCK_L } else { screen::CLOCK_R });
        for (i, s) in ["CHR", "ET", "RESET"].iter().enumerate() {
            hp::rbutton(m, &g, cu + (i as f32 - 1.0) * 0.016, -0.205, 0.0042);
            word(m, &g, cu + (i as f32 - 1.0) * 0.016, -0.218, 0.0022, s);
        }
        // The yoke's column passes in front of the panel here: a dark recess for it.
        rect(m, &mt(mat::PLASTIC, 0.02, 0.02, 0.022), g.at_h(sx * SEAT_X, -0.20, 0.0002), f.ax, f.ay, 0.05, 0.04);
    }
    let g = on(f);
    // Alternate gear extension: a red T-handle in its well.
    m.rbox(&mt(mat::PLASTIC, 0.012, 0.012, 0.013), g.at_h(-0.40, -0.19, 0.0), Vec3::new(0.035, 0.022, 0.002), 0.002, f.rot());
    m.rbox(&RED_HANDLE, g.at_h(-0.40, -0.19, 0.010), Vec3::new(0.028, 0.008, 0.007), 0.004, f.rot());
    word(m, &g, -0.40, -0.161, 0.0026, "ALTN GEAR");
    // (dark: it lights only when the gear is up with the flaps or power set for landing)
    light(m, &g, 0.40, -0.19, 0.030, 0.008, sid("CHECK LDG GEAR"), &mt(mat::PLASTIC, 0.10, 0.02, 0.02));

    // The centre panel between DU2 and DU3, down to the pedestal.
    plate(m, f, -PED_W + 0.004, bot + 0.004, PED_W - 0.004, -0.134, 20.0);
    // The SELCAL and registration placards (a fictional registration).
    rect(m, &legend_mat(style::TEXT, sid("SELCAL ABCD"), Vec3::new(0.70, 0.76, 0.82)), g.at_h(-0.215, -0.150, 0.0006), f.ax, f.ay, 0.034, 0.0075);
    rect(m, &legend_mat(style::TEXT, sid("QX-GLF"), Vec3::new(0.70, 0.76, 0.82)), g.at_h(0.215, -0.150, 0.0006), f.ax, f.ay, 0.030, 0.0075);
    // The fuel control switches' red housings (the levers are pieces of their own).
    for sx in [-1.0f32, 1.0] {
        let u = sx * 0.262;
        m.rbox(&RED_HANDLE, g.at_h(u, -0.215, 0.006), Vec3::new(0.013, 0.030, 0.006), 0.003, f.rot());
        m.rbox(&MASK, g.at_h(u, -0.215, 0.0122), Vec3::new(0.004, 0.022, 0.0005), 0.0003, f.rot());
        word(m, &g, u, -0.258, 0.012, if sx < 0.0 { "L" } else { "R" });
        word(m, &g, u, -0.176, 0.0025, "RUN");
        word(m, &g, u + sx * 0.022, -0.235, 0.0022, "OFF");
    }
    // The brake pressure indicator.
    m.rbox(&MASK, g.at_h(-0.175, -0.21, 0.0012), Vec3::new(0.022, 0.040, 0.0015), 0.001, f.rot());
    readout(m, &g.sub(0.0, 0.0, 0.0028), -0.175, -0.21, screen::BAPI);
    word(m, &g, -0.175, -0.162, 0.0022, "BRAKE PRESS");
    // IRS mode select: its knob and the three IRS buttons.
    frame_lines(m, &g, -0.115, -0.285, 0.055, -0.160);
    word(m, &g, -0.030, -0.170, 0.0026, "IRS MODE SELECT");
    knob(m, &g, -0.030, -0.205, 0.0085, 21.0);
    for k in 0..3 {
        let u = -0.080 + k as f32 * 0.050;
        sw(m, &g, u, -0.255, 0.011, 0.009, cap::PLAIN, sid(["IRS 1", "IRS 2", "IRS 3"][k]));
    }
    // The landing gear: three green lights over the handle's slot, DN LOCK
    // REL and horn silence beside it.
    frame_lines(m, &g, 0.080, -0.285, 0.205, -0.160);
    for (k, du) in [-0.030f32, 0.0, 0.030].iter().enumerate() {
        let lv = if k == 1 { -0.170 } else { -0.178 };
        m.rbox(&MASK, g.at_h(0.14 + du, lv, 0.002), Vec3::new(0.010, 0.006, 0.002), 0.001, f.rot());
        m.rbox(&hp::GREEN_LENS, g.at_h(0.14 + du, lv, 0.0042), Vec3::new(0.008, 0.0045, 0.0005), 0.0004, f.rot());
    }
    m.rbox(&mt(mat::BLACK, 0.004, 0.004, 0.004), g.at_h(0.14, -0.222, 0.0), Vec3::new(0.0065, 0.044, 0.002), 0.002, f.rot());
    word(m, &g, 0.117, -0.192, 0.0024, "UP");
    word(m, &g, 0.117, -0.250, 0.0024, "DN");
    sw(m, &g, 0.183, -0.205, 0.0085, 0.0075, cap::PLAIN, sid("DN LOCK REL"));
    sw(m, &g, 0.183, -0.245, 0.0085, 0.0075, cap::PLAIN, sid("HORN SILENCE"));
    word(m, &g, 0.14, -0.276, 0.0025, "LDG GEAR");
}

/// A display unit: the case's bezel, the black mask round the screen, the
/// screen under its glass (a `screen` id) and the case's screws.
fn du(m: &mut Mesh, f: &Face, u: f32, v: f32, id: f32, seed: f32) {
    let [hx, hy] = DU_HALF;
    m.rbox(&DU_BEZEL, f.at_h(u, v, 0.011), Vec3::new(hx, hy, 0.011), 0.006, f.rot());
    m.rbox(&MASK, f.at_h(u, v + 0.004, 0.0222), Vec3::new(0.1475, 0.1115, 0.0006), 0.002, f.rot());
    let [sx, sy] = screen::HALF[(id - 60.0) as usize];
    screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), f.at_h(u, v + 0.004, 0.0230), f.ax, f.ay, sx, sy);
    for (k, (du, dv)) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].iter().enumerate() {
        m.screw(&SCREW_BLACK, f.at_h(u + du * (hx - 0.0045), v + dv * (hy - 0.0045), 0.0222), f.n(), 0.0018, seed + k as f32);
    }
    // The ambient light sensor and a maker's mark on the lower bezel.
    m.disc(&mt(mat::GLASS, 0.0, 0.0, 0.0), f.at_h(u + hx - 0.012, v - hy + 0.0075, 0.0223), f.n(), f.ay, 0.0022, 10);
    rect(m, &legend_mat(style::TEXT, sid("HONEYWELL"), DU_BEZEL.col), f.at_h(u, v - hy + 0.0075, 0.0223), f.ax, f.ay, 0.014, 0.0022);
}

// ---------------------------------------------------------------- the glareshield

fn glareshield(m: &mut Mesh, b: &mut Builder) {
    let f = glare_face();
    let hh = GLARE_HH;
    let half = |q: Vec3| lin_x(q.z, q.y) - 0.012;
    let w_lip = half(f.at(0.0, hh)).min(0.70);
    // The padded top: from the lip forward to the windshield's sill, just under the glass.
    let s0 = m.v.len();
    m.patch(&GLARE, 40, 10, false, |u, v| {
        let x = (u * 2.0 - 1.0) * w_lip;
        let a = f.at(x, hh) + Vec3::Y * 0.004;
        let ds = sill_d(x).max(1.88) + 0.012;
        let front = p(x, ds, WS_SILL - 0.010);
        let q = a.lerp(front, v);
        let lim = half(q);
        (Vec3::new(q.x.clamp(-lim, lim), q.y, q.z), [x, v], 0.0)
    });
    face_to(m, s0, |q| q + Vec3::Y);
    // Its rolled edge along the lip.
    let edge: Vec<Vec3> = (0..=28).map(|k| f.at_h((k as f32 / 14.0 - 1.0) * w_lip, hh - 0.004, -0.004)).collect();
    m.tube(&GLARE, &edge, &|_| 0.0065, 10, true);
    // The face the units sit in, and the underside back to the panel's top.
    m.rbox(&GLARE_FACE, f.at_h(0.0, 0.0, -0.012), Vec3::new(w_lip, hh, 0.012), 0.003, f.rot());
    let pf = panel_face();
    let s0 = m.v.len();
    m.patch(&GLARE_FACE, 20, 2, false, |u, v| {
        let x = (u * 2.0 - 1.0) * w_lip;
        let a = f.at_h(x, -hh, -0.002);
        let c = pf.at_h(x, 0.137, -0.002);
        let q = a.lerp(c, v);
        (Vec3::new(q.x.clamp(-half(q), half(q)), q.y, q.z), [x, v], 0.0)
    });
    face_to(m, s0, |q| q - Vec3::Y);
    // The ends: closed down to the panel.
    for sx in [-1.0f32, 1.0] {
        let x = sx * w_lip;
        let ring = [f.at_h(x, hh, 0.0), f.at_h(x, -hh, 0.0), pf.at_h(x, 0.137, 0.0), p(x, sill_d(x) + 0.012, WS_SILL - 0.010)];
        hp::quad_to(m, &GLARE_FACE, ring, Vec3::new(0.0, by(3.0), bz(2.6)));
    }

    // Left to right: master warning and caution, a display keypad, SMC 1, the
    // Guidance Panel, SMC 2, a keypad, master warning and caution.
    for sx in [-1.0f32, 1.0] {
        // The master warning (red) and caution (amber) switchlights, lit
        // from the CAS state (deck650.wgsl `master650`), and the aural
        // warning's SOUND OFF.
        let c = sx * 0.620;
        plate(m, &f, c - 0.040, -hh + 0.004, c + 0.040, hh - 0.004, 30.0 + sx);
        let g = on(&f);
        sw(m, &g, c - 0.017, 0.010, 0.012, 0.012, cap::WARNING, sid("WARNING"));
        sw(m, &g, c + 0.017, 0.010, 0.012, 0.012, cap::CAUTION, sid("CAUTION"));
        sw(m, &g, c, -0.024, 0.012, 0.009, cap::PLAIN, sid("SOUND OFF"));
        // The display keypad: three rows of five, and STANDBY.
        let c = sx * 0.500;
        plate(m, &f, c - 0.076, -hh + 0.004, c + 0.076, hh - 0.004, 32.0 + sx);
        let keys = [["PFD", "MAP", "PLAN", "TRAF", "TERR"], ["CHART", "SYN", "WX", "VSD", "VIDEO"], ["CAS", "ENG", "FMS", "CKLST", "INFO"]];
        for (r, row) in keys.iter().enumerate() {
            for (i, k) in row.iter().enumerate() {
                let u = c - 0.056 + i as f32 * 0.028;
                let v = 0.026 - r as f32 * 0.020;
                m.rbox(&KEY, g.at_h(u, v, 0.0025), Vec3::new(0.0115, 0.0072, 0.0025), 0.0012, g.rot());
                rect(m, &legend_mat(style::KEY, sid(k), KEY.col), g.at_h(u, v, 0.0052), g.ax, g.ay, 0.010, 0.0062);
            }
        }
        m.rbox(&KEY, g.at_h(c + 0.042, -0.031, 0.0025), Vec3::new(0.022, 0.0055, 0.0025), 0.0012, g.rot());
        rect(m, &legend_mat(style::KEY, sid("STANDBY"), KEY.col), g.at_h(c + 0.042, -0.031, 0.0052), g.ax, g.ay, 0.020, 0.0048);
        m.rbox(&hp::GREEN_LENS, g.at_h(c - 0.050, -0.031, 0.002), Vec3::new(0.006, 0.0022, 0.0008), 0.0005, g.rot());
        // The SMC: its screen with four line keys each side, a knob in each lower corner.
        let c = sx * 0.322;
        plate(m, &f, c - 0.098, -hh + 0.004, c + 0.098, hh - 0.004, 34.0 + sx);
        m.rbox(&MASK, g.at_h(c, 0.004, 0.0008), Vec3::new(0.054, 0.0405, 0.0012), 0.001, g.rot());
        screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(if sx < 0.0 { screen::SMC_L } else { screen::SMC_R }), g.at_h(c, 0.004, 0.0021), g.ax, g.ay, 0.050, 0.0375);
        for side in [-1.0f32, 1.0] {
            for k in 0..4 {
                let v = 0.030 - k as f32 * 0.017;
                m.rbox(&KEY, g.at_h(c + side * 0.066, v, 0.002), Vec3::new(0.006, 0.0042, 0.002), 0.001, g.rot());
                rect(m, &mt(mat::PLASTIC, 0.6, 0.6, 0.6), g.at_h(c + side * 0.066, v, 0.0042), g.ax, g.ay, 0.0035, 0.0006);
            }
            knob(m, &g, c + side * 0.084, -0.030, 0.0060, 35.0 + side + sx);
        }
        word(m, &g, c, -0.040, 0.0026, if sx < 0.0 { "SMC 1" } else { "SMC 2" });
    }
    guidance_panel(m, b, &f);
}

/// The Guidance Panel: speed, heading, vertical speed and altitude windows
/// over their knobs, the mode buttons in rows, the course and baro knobs at the ends.
fn guidance_panel(m: &mut Mesh, b: &mut Builder, f: &Face) {
    let hw = 0.218;
    plate(m, f, -hw, -GLARE_HH + 0.004, hw, GLARE_HH - 0.004, 40.0);
    let g = on(f);
    let (vw, vk, vb) = (0.022, -0.004, -0.029);
    // A button: its face, legend and the light under it (a `screen::GP_LIGHT` quad).
    // (The clickable ones' targets, r 12 mm, take in the 21 x 14 mm cap and stay
    // clear of the knobs' 25 mm above.)
    let button = |m: &mut Mesh, u: f32, v: f32, mode: usize| {
        m.rbox(&KEY, g.at_h(u, v, 0.0030), Vec3::new(0.0105, 0.0070, 0.0030), 0.0014, g.rot());
        rect(m, &legend_mat(style::KEY, sid(screen::GP_MODES[mode]), KEY.col), g.at_h(u, v + 0.0015, 0.0062), g.ax, g.ay, 0.0090, 0.0040);
        let [lx, ly] = screen::GP_LIGHT_HALF;
        screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(screen::GP_LIGHT + mode as f32), g.at_h(u, v - 0.0042, 0.0062), g.ax, g.ay, lx, ly);
    };
    // A knob that is a mouse target of its own.
    let gp_knob = |b: &mut Builder, name: &'static str, u: f32, v: f32, r: f32, h: Hotspot| {
        let mut k = Mesh::default();
        let c = g.at(u, v);
        k.lathe(&hp::KNOB.pat(pat::KNURL), c, g.n(), &[[r * 1.25, 0.0], [r * 1.25, r * 0.3], [r, r * 0.4], [r, r * 1.35], [r * 0.85, r * 1.5], [0.0, r * 1.52]], 18, false);
        k.rbox(&hp::INK, c + g.n() * (r * 1.52) + g.ay * (r * 0.45), Vec3::new(r * 0.10, r * 0.40, 0.0003), 0.0001, g.rot());
        let i = b.fixed(name, G_ENGINE, k);
        b.pieces[i].hotspot = Some(h);
    };
    let at = |u: f32, v: f32, h: f32| g.at_h(u, v, h);
    // Column by column.
    // Baro (left): push for standard; the click sets the QNH.
    gp_knob(b, "gp baro knob", -0.196, vk, 0.0085, Hotspot::click(at(-0.196, vk, 0.013), 0.011, "baro_qnh"));
    word(m, &g, -0.196, vw, 0.0026, "BARO");
    word(m, &g, -0.196, vb, 0.0022, "PUSH STD");
    // Speed: its window, the knob, A/T.
    readout(m, &g, -0.150, vw, screen::GP_SPD);
    word(m, &g, -0.150, vw + 0.0125, 0.0022, "SPEED");
    gp_knob(b, "gp spd knob", -0.150, vk, 0.0085, Hotspot::wheel(at(-0.150, vk, 0.013), 0.011, "ap_spd_up", "ap_spd_down"));
    let mut a = Mesh::default();
    button(&mut a, -0.150, vb, 10);
    let i = b.fixed("gp at button", G_ENGINE, a);
    b.pieces[i].hotspot = Some(Hotspot::click(at(-0.150, vb, 0.0085), 0.012, "ap_at"));
    // FLCH, course 1, NAV.
    button(m, -0.106, vw, 8);
    gp_knob(b, "gp crs knob", -0.106, vk, 0.0080, Hotspot::wheel(at(-0.106, vk, 0.012), 0.010, "obs_right", "obs_left"));
    word(m, &g, -0.122, vk + 0.010, 0.0022, "CRS 1");
    let mut a = Mesh::default();
    button(&mut a, -0.106, vb, 3);
    let i = b.fixed("gp nav button", G_ENGINE, a);
    b.pieces[i].hotspot = Some(Hotspot::click(at(-0.106, vb, 0.0085), 0.012, "ap_nav"));
    // Heading: its window, the knob, HDG.
    readout(m, &g, -0.060, vw, screen::GP_HDG);
    word(m, &g, -0.060, vw + 0.0125, 0.0022, "HEADING");
    gp_knob(b, "gp hdg knob", -0.060, vk, 0.0090, Hotspot::wheel(at(-0.060, vk, 0.013), 0.011, "ap_hdg_right", "ap_hdg_left"));
    let mut a = Mesh::default();
    button(&mut a, -0.060, vb, 2);
    let i = b.fixed("gp hdg button", G_ENGINE, a);
    b.pieces[i].hotspot = Some(Hotspot::click(at(-0.060, vb, 0.0085), 0.012, "ap_hdg"));
    // APPR over BC.
    let mut a = Mesh::default();
    button(&mut a, -0.020, vk, 4);
    let i = b.fixed("gp appr button", G_ENGINE, a);
    b.pieces[i].hotspot = Some(Hotspot::click(at(-0.020, vk, 0.0085), 0.012, "ap_apr"));
    button(m, -0.020, vb, 5);
    word(m, &g, -0.020, vw, 0.0024, "LOW BANK");
    // AP over YD, with the PFD command arrows below.
    let mut a = Mesh::default();
    button(&mut a, 0.020, vw - 0.003, 0);
    let i = b.fixed("gp ap button", G_ENGINE, a);
    b.pieces[i].hotspot = Some(Hotspot::click(at(0.020, vw - 0.003, 0.0085), 0.012, "ap"));
    button(m, 0.020, vk - 0.002, 1);
    button(m, 0.020, vb, 11);
    // Vertical speed: its window, the pitch wheel, VS.
    readout(m, &g, 0.062, vw, screen::GP_VS);
    word(m, &g, 0.062, vw + 0.0125, 0.0022, "VS/FPA");
    let mut wh = Mesh::default();
    let wc = at(0.062, vk, 0.002);
    wh.rbox(&MASK, wc, Vec3::new(0.012, 0.010, 0.002), 0.001, g.rot());
    wh.lathe(&hp::KNOB.pat(pat::KNURL), wc - g.ax * 0.007, g.ax, &[[0.0, 0.0], [0.010, 0.0], [0.010, 0.014], [0.0, 0.014]], 18, false);
    let i = b.fixed("gp pitch wheel", G_ENGINE, wh);
    b.pieces[i].hotspot = Some(Hotspot::wheel(at(0.062, vk, 0.010), 0.010, "ap_vs_up", "ap_vs_down"));
    let mut a = Mesh::default();
    button(&mut a, 0.062, vb, 7);
    let i = b.fixed("gp vs button", G_ENGINE, a);
    b.pieces[i].hotspot = Some(Hotspot::click(at(0.062, vb, 0.0085), 0.012, "ap_vs"));
    // Altitude: its window, the knob, ALT.
    readout(m, &g, 0.110, vw, screen::GP_ALT);
    word(m, &g, 0.110, vw + 0.0125, 0.0022, "ALTITUDE");
    gp_knob(b, "gp alt knob", 0.110, vk, 0.0090, Hotspot::wheel(at(0.110, vk, 0.013), 0.011, "ap_alt_up", "ap_alt_down"));
    let mut a = Mesh::default();
    button(&mut a, 0.110, vb, 6);
    let i = b.fixed("gp alt button", G_ENGINE, a);
    b.pieces[i].hotspot = Some(Hotspot::click(at(0.110, vb, 0.0085), 0.012, "ap_alt"));
    // VNAV, course 2 (decorative), baro (right, decorative).
    button(m, 0.152, vw, 9);
    knob(m, &g, 0.152, vk, 0.0080, 41.0);
    word(m, &g, 0.152 + 0.016, vk + 0.010, 0.0022, "CRS 2");
    word(m, &g, 0.152, vb, 0.0022, "SYNC");
    knob(m, &g, 0.196, vk, 0.0085, 42.0);
    word(m, &g, 0.196, vw, 0.0026, "BARO");
    word(m, &g, 0.196, vb, 0.0022, "PUSH STD");
}

// ---------------------------------------------------------------- the pedestal

/// The forward MCDUs' faces (left, right): sloping down and back between the
/// centre panel and the pedestal's top.
fn mcdu_face(sx: f32) -> Face {
    let (a, c) = (Vec2::new(PED_D[0], PED_H[0]), Vec2::new(PED_D[1], PED_H[1]));
    let m = (a + c) * 0.5;
    Face::new(p(sx * 0.205, m.x, m.y + 0.004), Vec3::X, Vec3::new(0.0, a.y - c.y, -(a.x - c.x).abs()))
}
fn mcdu(m: &mut Mesh, f: &Face, id: f32, seed: f32) {
    let (hw, hh) = (0.073, 0.112);
    m.rbox(&PANEL_DK, f.at_h(0.0, 0.0, 0.003), Vec3::new(hw, hh, 0.003), 0.003, f.rot());
    let g = f.sub(0.0, 0.0, 0.006);
    m.rbox(&MASK, g.at_h(0.0, 0.050, 0.0006), Vec3::new(0.048, 0.042, 0.0008), 0.001, g.rot());
    screen_quad(m, &mt(mat::DISPLAY, 0.0, 0.0, 0.0).x(id), g.at_h(0.0, 0.050, 0.0016), g.ax, g.ay, 0.044, 0.038);
    for k in 0..6 {
        let v = 0.080 - k as f32 * 0.012;
        for sx in [-1.0f32, 1.0] {
            m.rbox(&KEY, g.at_h(sx * 0.0605, v, 0.002), Vec3::new(0.0055, 0.0038, 0.002), 0.001, g.rot());
        }
    }
    let fkeys = ["PERF", "NAV", "FPLN", "PROG", "DIR", "MENU", "RADIO", "PREV", "NEXT", "EXEC"];
    for (k, s) in fkeys.iter().enumerate() {
        let (c, r) = (k % 5, k / 5);
        let (u, v) = (-0.052 + c as f32 * 0.026, -0.006 - r as f32 * 0.016);
        m.rbox(&KEY, g.at_h(u, v, 0.0025), Vec3::new(0.0115, 0.0062, 0.0025), 0.0014, g.rot());
        rect(m, &legend_mat(style::KEY, sid(s), KEY.col), g.at_h(u, v, 0.0052), g.ax, g.ay, 0.0105, 0.0056);
    }
    // The alphabet keys (A to Z, space, delete, slash, clear) and the number keys.
    const ALPHA: [&str; 30] = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z", "SP", "DEL", "/", "CLR"];
    for (k, s) in ALPHA.iter().enumerate() {
        let (u, v) = (-0.030 + (k % 5) as f32 * 0.017, -0.044 - (k / 5) as f32 * 0.0105);
        m.rbox(&KEY, g.at_h(u, v, 0.0022), Vec3::new(0.0068, 0.0042, 0.0022), 0.0013, g.rot());
        rect(m, &legend_mat(style::KEY, sid(s), KEY.col), g.at_h(u, v, 0.0046), g.ax, g.ay, 0.006, 0.0038);
    }
    const NUM: [&str; 12] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", ".", "0", "+/-"];
    for (k, s) in NUM.iter().enumerate() {
        let (u, v) = (-0.062 + (k % 3) as f32 * 0.011, -0.044 - (k / 3) as f32 * 0.013);
        m.rbox(&KEY, g.at_h(u, v, 0.0022), Vec3::new(0.0045, 0.0052, 0.0022), 0.0015, g.rot());
        rect(m, &legend_mat(style::KEY, sid(s), KEY.col), g.at_h(u, v, 0.0046), g.ax, g.ay, 0.0038, 0.0044);
    }
    hp::knob(m, &g, 0.058, -0.095, 0.0055, seed, false);
}

/// One block of the pedestal's body between x0 and x1: its sides and its top
/// along the profile `prof` (drawing d, h from the floor forward round to the floor aft).
fn prism(m: &mut Mesh, mm: &Mat, x0: f32, x1: f32, prof: &[(f32, f32)]) {
    let c = prof.iter().fold(Vec2::ZERO, |a, &(d, h)| a + Vec2::new(d, h)) / prof.len() as f32;
    for (x, s) in [(x0, -1.0f32), (x1, 1.0)] {
        let ring: Vec<Vec3> = prof.iter().map(|&(d, h)| p(x, d, h)).collect();
        m.fan(mm, p(x, c.x, c.y), &ring, Vec3::X * s);
    }
    let xm = (x0 + x1) * 0.5;
    for k in 0..prof.len() - 1 {
        let ((d0, h0), (d1, h1)) = (prof[k], prof[k + 1]);
        let mid = p(xm, (d0 + d1) * 0.5, (h0 + h1) * 0.5);
        hp::quad_to(m, mm, [p(x0, d0, h0), p(x1, d0, h0), p(x1, d1, h1), p(x0, d1, h1)], mid + (mid - p(xm, c.x, c.y)) * 4.0);
    }
}

fn pedestal(m: &mut Mesh) {
    let fl = FLOOR_Y + CG_H;
    // The forward part: its top sloping down from the centre panel under each
    // MCDU, then level to the aft part; between the MCDUs the thrust lever
    // quadrant, standing above them.
    let slope = [(PED_D[0], fl), (PED_D[0], PED_H[0]), (PED_D[1], PED_H[1]), (PED_D[2], PED_H[1]), (PED_D[2], fl)];
    prism(m, &PANEL_DK, -PED_W, -0.132, &slope);
    prism(m, &PANEL_DK, 0.132, PED_W, &slope);
    prism(m, &PANEL_DK, -0.132, 0.132, &[(PED_D[0], fl), (PED_D[0], PED_H[0]), (2.47, QUAD_BASE), (PED_D[2], QUAD_BASE), (PED_D[2], fl)]);
    // Behind, narrower, between the seats.
    prism(m, &PANEL_DK, -PED_W_AFT, PED_W_AFT, &[(PED_D[2] - 0.01, fl), (PED_D[2] - 0.01, PED_H[2]), (PED_D[3], PED_H[2] + 0.02), (PED_D[3], fl)]);
    for sx in [-1.0f32, 1.0] {
        let f = mcdu_face(sx);
        mcdu(m, &f, if sx < 0.0 { screen::MCDU_L } else { screen::MCDU_R }, 50.0 + sx);
    }
    // The thrust lever quadrant: a cover between the MCDUs, curved about the
    // levers' pivot, with a slot for each and the display brightness strip ahead.
    let piv = p(0.0, THR_PIVOT[0], THR_PIVOT[1]);
    let (qr, qh) = (QUAD_R, QUAD_W);
    let s0 = m.v.len();
    m.patch(&PANEL_DK, 16, 6, false, |u, v| {
        let a = (u - 0.5) * 0.95;
        let x = (v - 0.5) * 2.0 * qh;
        (piv + Vec3::new(x, a.cos() * qr, a.sin() * qr), [x, a], 0.0)
    });
    face_to(m, s0, |q| q + (q - piv));
    // Its sides and ends, down to the block it stands on.
    let base = by(QUAD_BASE) - 0.004;
    for sx in [-1.0f32, 1.0] {
        let ring: Vec<Vec3> = (0..=16).map(|k| { let a = (k as f32 / 16.0 - 0.5) * 0.95; piv + Vec3::new(sx * qh, a.cos() * qr, a.sin() * qr) }).collect();
        for w in ring.windows(2) {
            hp::quad_to(m, &PANEL_DK, [w[0], w[1], Vec3::new(w[1].x, base, w[1].z), Vec3::new(w[0].x, base, w[0].z)], w[0] + Vec3::X * sx);
        }
    }
    for a in [-0.475f32, 0.475] {
        let (y, z) = (piv.y + a.cos() * qr, piv.z + a.sin() * qr);
        hp::quad_to(m, &PANEL_DK, [Vec3::new(-qh, y, z), Vec3::new(qh, y, z), Vec3::new(qh, base, z), Vec3::new(-qh, base, z)], Vec3::new(0.0, y, z + a.signum()));
    }
    for x in THR_X {
        m.patch(&mt(mat::BLACK, 0.004, 0.004, 0.004), 12, 1, true, |u, v| {
            let a = (u - 0.5) * 0.80;
            (piv + Vec3::new(x + (v - 0.5) * 0.010, a.cos() * (qr + 0.0007), a.sin() * (qr + 0.0007)), [u, v], 0.0)
        });
    }
    // (on the block's sloping top between the MCDUs, ahead of the quadrant)
    let up = Vec3::new(0.0, PED_H[0] - QUAD_BASE, PED_D[0] - 2.47).normalize();
    let strip = Face::new(p(0.0, 2.4275, (PED_H[0] + QUAD_BASE) * 0.5) + Vec3::X.cross(up) * 0.0015, Vec3::X, up);
    for (k, u) in [-0.08f32, 0.08].iter().enumerate() {
        hp::knob(m, &strip, *u, 0.0, 0.0055, 55.0 + k as f32, false);
    }
    rect(m, &legend_mat(style::TEXT, sid("DSPL BRT"), PANEL_DK.col), strip.at_h(0.0, 0.0, 0.0004), strip.ax, strip.ay, 0.024, 0.0035);
    for (s, a) in [("IDLE", 0.30f32), ("TO", -0.30)] {
        let q = piv + Vec3::new(-0.105, a.cos() * (qr + 0.0006), a.sin() * (qr + 0.0006));
        let t = Vec3::new(0.0, -a.sin(), a.cos());
        rect(m, &legend_mat(style::TEXT, sid(s), PANEL_DK.col), q, Vec3::X, -t, 0.010, 0.0035);
    }

    // The aft top: speed brake (left) and flap (right) slots with their
    // scales, the trim panel, the third MCDU, park brake and RAT handles.
    let top = |d: f32| PED_H[2] + 0.02 * (d - PED_D[2]) / (PED_D[3] - PED_D[2]) + 0.0005;
    let tf = Face::new(p(0.0, PED_D[2], top(PED_D[2])), Vec3::X, Vec3::new(0.0, 0.02 / (PED_D[3] - PED_D[2]), -1.0));
    let tv = |d: f32| -(d - PED_D[2]);
    for (sx, name, notches) in [(-1.0f32, "SPEED BRAKE", vec!["RET", "", "EXT"]), (1.0, "FLAPS", vec!["0", "10", "20", "39"])] {
        let u = sx * 0.165;
        plate(m, &tf, u - 0.060, tv(2.96), u + 0.060, tv(2.71), 60.0 + sx);
        let g = on(&tf);
        m.rbox(&mt(mat::BLACK, 0.004, 0.004, 0.004), g.at_h(u, tv(2.835), 0.0), Vec3::new(0.006, 0.090, 0.0015), 0.0015, g.rot());
        word(m, &g, u, tv(2.73), 0.0030, name);
        let n = notches.len();
        for (k, s) in notches.iter().enumerate() {
            let v = tv(2.76 + 0.15 * k as f32 / (n - 1) as f32);
            rect(m, &hp::INK, g.at_h(u - sx * 0.013, v, 0.0003), g.ax, g.ay, 0.005, 0.0006);
            if !s.is_empty() {
                word(m, &g, u - sx * 0.030, v, 0.0030, s);
            }
        }
    }
    // The trim panel (the pitch switch is a piece of its own) and the RAAS,
    // FLT CTRL RESET and GND SPLR switches on the left.
    let g = on(&tf);
    plate(m, &tf, -0.095, tv(2.95), 0.095, tv(2.71), 62.0);
    word(m, &g, 0.0, tv(2.725), 0.0030, "TRIM");
    word(m, &g, -0.050, tv(2.755), 0.0024, "PITCH");
    for (u, s) in [(0.0f32, "AIL"), (0.050, "RUD")] {
        hp::toggle(m, &g, u - 0.006, tv(2.80), false);
        hp::toggle(m, &g, u + 0.006, tv(2.80), false);
        word(m, &g, u, tv(2.755), 0.0024, s);
    }
    sw(m, &g, 0.050, tv(2.86), 0.011, 0.0085, cap::PLAIN, sid("ACRT"));
    sw(m, &g, -0.050, tv(2.90), 0.011, 0.0085, cap::PLAIN, sid("FLT CTRL RESET"));
    sw(m, &g, 0.0, tv(2.90), 0.011, 0.0085, cap::PLAIN, sid("GND SPLR ORIDE"));
    sw(m, &g, 0.050, tv(2.925), 0.011, 0.0085, cap::PLAIN, sid("RAAS INHB"));
    // The third MCDU, flat on the top between the seats.
    let cf = Face::new(tf.at_h(0.0, tv(3.10), 0.0), tf.ax, tf.ay);
    mcdu(m, &cf, screen::MCDU_C, 64.0);
    // The park brake's quadrant (its lever moves) and the RAT handle.
    // (inboard, as close to the third MCDU as it goes: the captain's inboard
    // armrest would hide it further out)
    plate(m, &tf, PB_X - 0.065, tv(3.20), PB_X + 0.065, tv(2.97), 65.0);
    word(m, &g, PB_X, tv(3.185), 0.0030, "PARK EMER BRAKE");
    m.rbox(&mt(mat::BLACK, 0.004, 0.004, 0.004), g.at_h(PB_X, tv(3.06), 0.0), Vec3::new(0.009, 0.075, 0.0015), 0.0015, g.rot());
    plate(m, &tf, 0.100, tv(3.20), 0.235, tv(2.97), 66.0);
    let gr = on(&tf);
    m.rbox(&mt(mat::PLASTIC, 0.012, 0.012, 0.013), gr.at_h(0.168, tv(3.08), 0.0), Vec3::new(0.040, 0.020, 0.002), 0.002, gr.rot());
    m.rbox(&hp::stripes(0.006, false), gr.at_h(0.168, tv(3.08), 0.010), Vec3::new(0.034, 0.008, 0.008), 0.004, gr.rot());
    word(m, &gr, 0.168, tv(3.04), 0.0032, "RAT");
    word(m, &gr, 0.168, tv(3.13), 0.0024, "PULL TO DEPLOY");
}

// ---------------------------------------------------------------- the overhead

fn overhead(m: &mut Mesh, b: &mut Builder) {
    // The box: its three faces, the sides up to the lining, the front end.
    for k in 0..3 {
        let (f, l) = oh_face(k);
        let w = OH_W[k];
        m.rbox(&PANEL_DK, f.at_h(0.0, 0.0, -0.010), Vec3::new(w, l, 0.010), 0.003, f.rot());
        for sx in [-1.0f32, 1.0] {
            let (a, c) = (f.at(sx * w, -l), f.at(sx * w, l));
            let (ta, tc) = (lin_top(a.z, a.x) + 0.02, lin_top(c.z, c.x) + 0.02);
            hp::quad_to(m, &PANEL_DK, [a, c, Vec3::new(a.x, tc, c.z), Vec3::new(a.x, ta, a.z)], Vec3::new(0.0, a.y, a.z));
        }
    }
    // Where the faces narrow, steps up to the lining; the box's front.
    {
        let (f, l) = oh_face(0);
        let (a, c) = (f.at(-OH_W[0], -l), f.at(OH_W[0], -l));
        hp::quad_to(m, &PANEL_DK, [a, c, Vec3::new(c.x, lin_top(c.z, c.x) + 0.02, c.z), Vec3::new(a.x, lin_top(a.z, a.x) + 0.02, a.z)], EYE);
        for k in 0..2 {
            let (f0, l0) = oh_face(k);
            for sx in [-1.0f32, 1.0] {
                let a = f0.at(sx * OH_W[k], l0);
                let c = Vec3::new(sx * OH_W[k + 1], a.y, a.z);
                hp::quad_to(m, &PANEL_DK, [a, c, Vec3::new(c.x, lin_top(c.z, c.x) + 0.02, c.z), Vec3::new(a.x, lin_top(a.z, a.x) + 0.02, a.z)], EYE);
            }
        }
    }

    // The lights strip along the front, as the photographs show it: dead-
    // front caps under their printed names (the L and R LDG caps are pieces
    // of their own, below).
    let (f, l) = oh_face(0);
    let g = on(&f);
    let (v0, v1) = (-l + 0.004, l - 0.004);
    plate(m, &f, -0.330, v0, -0.088, v1, 70.0);
    word(m, &g, -0.209, v1 - 0.007, 0.0030, "EXTERIOR LIGHTS");
    for (r, row) in EXT_LIGHTS.iter().enumerate() {
        for (i, s) in row.iter().enumerate() {
            let (u, v) = (EXT_U0 + i as f32 * EXT_DU, 0.010 - r as f32 * 0.040);
            label(m, &g, u, v + 0.016, EXT_DU, s);
            if r == 1 && i < 2 {
                continue;
            }
            sw(m, &g, u, v, 0.0105, 0.0095, cap::PLAIN, 0);
        }
    }
    plate(m, &f, -0.084, v0, -0.020, v1, 71.0);
    word(m, &g, -0.052, v1 - 0.007, 0.0028, "PASS WARN");
    for (v, s) in [(0.010f32, "SEAT BELT"), (-0.030, "NO SMOKE")] {
        sw(m, &g, -0.052, v, 0.0105, 0.0095, cap::PLAIN, 0);
        label(m, &g, -0.052, v + 0.016, 0.050, s);
    }
    plate(m, &f, -0.016, v0, 0.170, v1, 72.0);
    word(m, &g, 0.077, v1 - 0.007, 0.0030, "COCKPIT LIGHTS");
    for (k, (u, v, s)) in [(0.010f32, 0.012f32, "OVHD PNL"), (0.010, -0.032, "PED PNL"), (0.077, -0.010, "MASTER CONTROL"), (0.144, 0.012, "CB PANELS"), (0.144, -0.032, "OVHD FLOOD")].into_iter().enumerate() {
        knob(m, &g, u, v, 0.0070, 73.0 + k as f32);
        label(m, &g, u, v + 0.0135, 0.050, s);
    }
    plate(m, &f, 0.174, v0, 0.330, v1, 74.0);
    word(m, &g, 0.244, v1 - 0.007, 0.0024, "ANTI ICE HTR");
    word(m, &g, 0.305, v1 - 0.007, 0.0024, "WINDSHIELD HEAT");
    for (u, v, s) in [(0.192f32, 0.010f32, "EVS WSHLD HT"), (0.192, -0.030, "CABIN WDO HT"), (0.228, 0.010, "PROBE 1"), (0.260, 0.010, "PROBE 2"),
        (0.228, -0.030, "PROBE 3"), (0.260, -0.030, "PROBE 4"), (0.292, 0.010, "LF/RS"), (0.318, 0.010, "RF/LS")] {
        sw(m, &g, u, v, 0.0100, 0.0090, cap::PLAIN, 0);
        label(m, &g, u, v + 0.0155, if u < 0.2 { 0.034 } else { 0.028 }, s);
    }

    // The systems: three columns (seen from the seats u is to the right, v
    // aft), each panel as the photographs show it.
    let (f, l) = oh_face(1);
    let g = on(&f);
    let (va, vb) = (-l + 0.004, l - 0.004);
    let vr = |a: f32, c: f32| (va + (vb - va) * a, va + (vb - va) * c);
    // Left column, forward to aft: engine start and hydraulics, the main
    // batteries, electrical power control, the flight control batteries and
    // the RAT generator, emergency power.
    let (e0, e1) = vr(0.0, 0.20);
    plate(m, &f, -0.396, e0, -0.236, e1, 80.0);
    word(m, &g, -0.316, e1 - 0.007, 0.0030, "ENGINE START");
    // MASTER (CRANK, START) and the L and R ENG start buttons (pieces of
    // their own, below), continuous ignition under its striped guard.
    word(m, &g, -0.360, e0 + 0.077, 0.0024, "MASTER");
    for (k, s) in ["CRANK", "START"].iter().enumerate() {
        sw(m, &g, -0.376 + k as f32 * 0.032, e0 + 0.052, 0.0110, 0.0095, cap::PLAIN, 0);
        label(m, &g, -0.376 + k as f32 * 0.032, e0 + 0.067, 0.032, s);
    }
    word(m, &g, -0.280, e0 + 0.077, 0.0024, "START");
    for (k, s) in ["L ENG", "R ENG"].iter().enumerate() {
        label(m, &g, START_U[k], e0 + 0.067, 0.032, s);
    }
    let start_at = START_U.map(|u| g.at_h(u, e0 + 0.052, 0.0));
    m.rbox(&hp::stripes(0.005, false), g.at_h(-0.280, e0 + 0.020, 0.0012), Vec3::new(0.0185, 0.0165, 0.0012), 0.001, g.rot());
    for sx in [-1.0f32, 1.0] {
        m.rbox(&hp::stripes(0.005, false), g.at_h(-0.280 + sx * 0.0165, e0 + 0.020, 0.0095), Vec3::new(0.0018, 0.0150, 0.0085), 0.0008, g.rot());
    }
    sw(m, &g, -0.280, e0 + 0.020, 0.0110, 0.0095, cap::PLAIN, 0);
    word(m, &g, -0.330, e0 + 0.020, 0.0024, "CONT IGN");
    panel(m, &f, -0.232, e0, -0.136, e1, 81.0, "HYDRAULIC CONTROL", &[("PTU", &["OFF/ARM", "ON"]), ("AUX PUMP", &["OFF/ARM", "ON"])]);
    // The main batteries: their volt and amp readouts either side of the L
    // and R battery switches (pieces of their own, below).
    let (b0, b1) = vr(0.20, 0.34);
    plate(m, &f, -0.396, b0, -0.136, b1, 82.0);
    word(m, &g, -0.266, b1 - 0.007, 0.0030, "MAIN BATTERIES");
    readout(m, &g, -0.350, b0 + 0.028, screen::BATT_L);
    readout(m, &g, -0.182, b0 + 0.028, screen::BATT_R);
    for u in [-0.350f32, -0.182] {
        word(m, &g, u, b0 + 0.012, 0.0022, "VOLTS/AMPS");
    }
    for (k, s) in ["LEFT", "RIGHT"].iter().enumerate() {
        label(m, &g, BATT_U[k], b0 + 0.0435, 0.040, s);
    }
    let batt_at = BATT_U.map(|u| g.at_h(u, b0 + 0.026, 0.0));
    let (c0, c1) = vr(0.34, 0.64);
    panel(m, &f, -0.396, c0, -0.136, c1, 83.0, "ELECTRICAL POWER CONTROL", &[
        ("", &["L BUS TIE|AUTO", "-", "-", "R BUS TIE|AUTO"]),
        ("", &["RESET", "APU GEN", "EXT PWR", "-"]),
        ("", &["L GEN", "GND SVC BUS", "R GEN"]),
    ]);
    let (d0, d1) = vr(0.64, 0.82);
    panel(m, &f, -0.396, d0, -0.236, d1, 84.0, "FLT CTRL BATTERIES", &[("", &["EBHA", "UPS"])]);
    panel(m, &f, -0.232, d0, -0.136, d1, 85.0, "RAM AIR TURBINE", &[("", &["GEN"])]);
    let (x0, x1) = vr(0.82, 1.0);
    panel(m, &f, -0.396, x0, -0.136, x1, 86.0, "EMERGENCY POWER", &[("", &["", "", ""])]);
    // The middle column, forward to aft: anti-ice, the fuel system, fuel
    // return and remote fueling, fire extinguishing and the APU, the masters.
    let (a0, a1) = vr(0.0, 0.15);
    plate(m, &f, -0.132, a0, 0.132, a1, 87.0);
    word(m, &g, 0.0, a1 - 0.007, 0.0030, "ANTI ICE");
    for (k, s) in ["L WING", "L COWL", "R COWL", "R WING"].iter().enumerate() {
        let u = -0.090 + k as f32 * 0.060;
        knob(m, &g, u, a0 + 0.030, 0.0085, 88.0 + k as f32);
        word(m, &g, u, a0 + 0.055, 0.0022, s);
    }
    // The fuel system: the tanks' pumps, the intertank and crossflow valves,
    // the lines between them printed on the panel.
    let (f0, f1) = vr(0.15, 0.48);
    plate(m, &f, -0.132, f0, 0.132, f1, 89.0);
    word(m, &g, 0.0, f1 - 0.007, 0.0030, "FUEL SYSTEM");
    let line = |m: &mut Mesh, a: (f32, f32), c: (f32, f32)| {
        let (cu, cv) = ((a.0 + c.0) * 0.5, (a.1 + c.1) * 0.5);
        rect(m, &hp::INK, g.at_h(cu, cv, 0.0003), g.ax, g.ay, ((c.0 - a.0) * 0.5).abs() + 0.0008, ((c.1 - a.1) * 0.5).abs() + 0.0008);
    };
    let (vt, vp, vx) = (f0 + 0.105, f0 + 0.060, f0 + 0.030);
    line(m, (-0.086, vt), (0.086, vt));
    line(m, (-0.086, vx), (0.086, vx));
    for u in [-0.075f32, 0.075] {
        line(m, (u, vt), (u, vp + 0.017));
        line(m, (u, vp - 0.011), (u, vx));
    }
    word(m, &g, -0.108, vt, 0.0024, "L TANK");
    word(m, &g, 0.108, vt, 0.0024, "R TANK");
    for (k, u) in [-0.096f32, -0.054, 0.054, 0.096].iter().enumerate() {
        sw(m, &g, *u, vp, 0.0105, 0.009, cap::PLAIN, 0);
        word(m, &g, *u, vp + 0.0145, 0.0022, ["ALT", "MAIN", "MAIN", "ALT"][k]);
    }
    sw(m, &g, 0.0, vt, 0.0130, 0.009, cap::PLAIN, 0);
    word(m, &g, 0.0, vt + 0.0145, 0.0022, "INTER TANK");
    sw(m, &g, 0.0, vx, 0.0130, 0.009, cap::PLAIN, 0);
    word(m, &g, 0.0, vx - 0.0145, 0.0022, "X FLOW");
    for (u, s) in [(-0.108f32, "APU"), (-0.075, "L ENG"), (0.075, "R ENG")] {
        word(m, &g, u, vx - if s == "APU" { 0.0 } else { 0.0145 }, 0.0022, s);
    }
    let (r0, r1) = vr(0.48, 0.60);
    panel(m, &f, -0.132, r0, -0.002, r1, 90.0, "FUEL RETURN", &[("", &["OFF/AUTO"])]);
    panel(m, &f, 0.002, r0, 0.132, r1, 91.0, "REMOTE FUELING", &[("", &["SHUTOFF"])]);
    let (p0, p1) = vr(0.60, 0.82);
    plate(m, &f, -0.132, p0, 0.132, p1, 92.0);
    word(m, &g, 0.030, p1 - 0.007, 0.0030, "APU CONTROL");
    word(m, &g, -0.090, p1 - 0.007, 0.0030, "FIRE EXT");
    m.rbox(&hp::stripes(0.006, false), g.at_h(-0.090, (p0 + p1) * 0.5 - 0.004, 0.0008), Vec3::new(0.024, 0.030, 0.0008), 0.0008, g.rot());
    m.rbox(&RED_HANDLE, g.at_h(-0.090, (p0 + p1) * 0.5 - 0.004, 0.006), Vec3::new(0.016, 0.022, 0.005), 0.003, g.rot());
    for (k, s) in ["MASTER", "START", "STOP"].iter().enumerate() {
        let u = -0.010 + k as f32 * 0.040;
        sw(m, &g, u, (p0 + p1) * 0.5 - 0.004, 0.0120, 0.0100, cap::PLAIN, 0);
        label(m, &g, u, (p0 + p1) * 0.5 + 0.0135, 0.040, s);
    }
    let (q0, q1) = vr(0.82, 1.0);
    panel(m, &f, -0.132, q0, 0.132, q1, 93.0, "MASTERS", &[("", &["CABIN/GALLEY", "60 HZ"])]);
    // The right column, forward to aft: cabin pressure control, the cabin
    // altitude, the temperature knobs, bleed air and temperature control.
    let (s0, s1) = vr(0.0, 0.22);
    panel(m, &f, 0.136, s0, 0.396, s1, 94.0, "CABIN PRESSURE CONTROL", &[("", &["*MAN", "-"]), ("", &["FLIGHT", "FAULT", "AUTO"])]);
    let (t0, t1) = vr(0.22, 0.40);
    plate(m, &f, 0.136, t0, 0.396, t1, 95.0);
    word(m, &g, 0.330, t1 - 0.007, 0.0028, "CABIN ALT FT");
    word(m, &g, 0.330, t0 + 0.010, 0.0024, "DFRN PRESS");
    // (The cabin rate of climb dial, thousands of feet a minute: printed, its
    // needle fixed at zero.)
    let dc = (0.200, (t0 + t1) * 0.5 - 0.004);
    m.lathe(&MASK, g.at(dc.0, dc.1), g.n(), &[[0.034, 0.0], [0.034, 0.004], [0.030, 0.005], [0.0, 0.005]], 28, false);
    let dial = g.sub(0.0, 0.0, 0.0052);
    for k in 0..=8 {
        let a = PI - (k as f32 - 4.0) * 0.5 * 75f32.to_radians();
        let (sa, ca) = a.sin_cos();
        let (r0, r1) = (if k % 2 == 0 { 0.021 } else { 0.024 }, 0.0275);
        let mid = (r0 + r1) * 0.5;
        let rot = dial.rot() * Quat::from_rotation_z(a);
        m.rbox(&hp::INK, dial.at_h(dc.0 + ca * mid, dc.1 + sa * mid, 0.0002), Vec3::new((r1 - r0) * 0.5, 0.0004, 0.0002), 0.0001, rot);
        if k % 2 == 0 {
            word(m, &dial, dc.0 + ca * 0.015, dc.1 + sa * 0.015, 0.0030, ["2", "1", "0", "1", "2"][k / 2]);
        }
    }
    word(m, &dial, dc.0 + 0.004, dc.1 + 0.008, 0.0026, "UP");
    word(m, &dial, dc.0 + 0.004, dc.1 - 0.008, 0.0026, "DN");
    m.rbox(&hp::INK, g.at_h(dc.0 - 0.010, dc.1, 0.0056), Vec3::new(0.012, 0.0008, 0.0003), 0.0001, g.rot());
    label(m, &g, dc.0 - 0.049, dc.1 + 0.006, 0.026, "RC");
    label(m, &g, dc.0 - 0.049, dc.1 - 0.006, 0.026, "FPM X 1000");
    readout(m, &g, 0.330, (t0 + t1) * 0.5 - 0.004, screen::CABIN_ALT);
    // The zone temperatures: a knob (cold to hot) and its AUTO cap, lit
    // blue while the bus is powered, for each zone.
    let (k0, k1) = vr(0.40, 0.62);
    plate(m, &f, 0.136, k0, 0.396, k1, 97.0);
    for (k, s) in ["COCKPIT", "FWD CABIN", "AFT CABIN"].iter().enumerate() {
        let u = 0.180 + k as f32 * 0.086;
        knob(m, &g, u, k0 + 0.040, 0.0110, 98.0 + k as f32);
        word(m, &g, u - 0.020, k0 + 0.026, 0.0020, "COLD");
        word(m, &g, u + 0.020, k0 + 0.026, 0.0020, "HOT");
        word(m, &g, u, k0 + 0.068, 0.0022, s);
        sw(m, &g, u, k0 + 0.088, 0.0105, 0.0085, cap::BLUE, sid("AUTO"));
    }
    let (n0, n1) = vr(0.62, 1.0);
    panel(m, &f, 0.136, n0, 0.396, n1, 99.0, "BLEED AIR", &[
        ("", &["L ENG", "APU", "R ENG"]),
        ("", &["ISOLATION"]),
        ("TEMP CONTROL", &["L PACK", "RAM AIR", "R PACK"]),
        ("", &["AUTO TEMP SELECT|ON", "TEMP DISPLAY"]),
    ]);

    // Aft: the test panels, the doors, gear dump valves and the recorders'
    // event switch, display switching, fire test and detection; the circuit
    // breakers over the pilots' heads.
    let (f, l) = oh_face(2);
    let g = on(&f);
    let (va, vb) = (-l + 0.004, l - 0.004);
    let vr = |a: f32, c: f32| (va + (vb - va) * a, va + (vb - va) * c);
    let (h0, h1) = vr(0.0, 0.18);
    panel(m, &f, -0.412, h0, -0.150, h1, 100.0, "SYSTEM TEST", &[
        ("", &["RAT", "FUEL", "ICE DET", "SMOKE DET", "ANN LIGHTS"]),
        ("", &["", "PASS OXYGEN", "", "", ""]),
    ]);
    panel(m, &f, -0.146, h0, 0.146, h1, 101.0, "", &[("", &["DOOR OPEN", "DOOR SAFETY", "NOSE", "MAIN", "FDR/CMC EVENT"])]);
    // (the dump valves' title over NOSE and MAIN, the third and fourth of five)
    word(m, &g, 0.021, h0 + (0.4 * (h1 - h0)).max(0.0105) + 0.0225, 0.0024, "LDG GEAR DUMP VALVES");
    panel(m, &f, 0.150, h0, 0.412, h1, 102.0, "", &[("FIRE TEST", &["L ENG", "APU", "R ENG"]), ("FIRE DETECTION", &["LOOP A", "LOOP B", "FAULT TEST", "LOOP A", "LOOP B"])]);
    let (h0, h1) = vr(0.18, 0.34);
    plate(m, &f, -0.412, h0, 0.146, h1, 103.0);
    word(m, &g, -0.133, h1 - 0.007, 0.0030, "MFD DISPLAY SWITCHING");
    for k in 0..6 {
        let u = -0.37 + k as f32 * 0.094;
        knob(m, &g, u, (h0 + h1) * 0.5 - 0.006, 0.0090, 104.0 + k as f32);
        if k == 0 || k == 5 {
            word(m, &g, u, (h0 + h1) * 0.5 + 0.014, 0.0024, if k == 0 { "PILOT" } else { "COPILOT" });
        }
    }
    word(m, &g, -0.37 + 2.5 * 0.094, (h0 + h1) * 0.5 + 0.014, 0.0024, "DISPLAY SYSTEM CONTROL");
    rect(m, &legend_mat(style::PLACARD, 0, Vec3::new(0.82, 0.81, 0.78)), g.at_h(0.27, (h0 + h1) * 0.5, 0.0006), g.ax, g.ay, 0.10, (h1 - h0) * 0.42);
    for side in [-1.0f32, 1.0] {
        let (u0, u1) = if side < 0.0 { (-0.412, -0.008) } else { (0.008, 0.412) };
        let (c0, c1) = vr(0.36, 1.0);
        plate(m, &f, u0, c0, u1, c1, 110.0 + side);
        breakers(m, &g, u0 + 0.014, c0 + 0.012, u1 - 0.014, c1 - 0.014, side);
    }

    // The clickable ones: the L and R main battery switches, each with the
    // blue ON legend that shows when it is set.
    let g1 = on(&oh_face(1).0);
    for (n, at) in batt_at.into_iter().enumerate() {
        let gf = Face { o: at, ..g1 };
        let mut bm = Mesh::default();
        let face = gf.at_h(0.0, 0.0, 0.0080);
        bm.rbox(&KEY, face - gf.n() * 0.004, Vec3::new(0.0115, 0.0100, 0.004), 0.0012, gf.rot());
        rect(&mut bm, &legend_mat(cap::TOP, sid("BATT"), Vec3::new(0.018, 0.019, 0.021)), face + gf.n() * 0.0002, gf.ax, gf.ay, 0.0100, 0.0088);
        let piece = b.add("battery switch", G_ENGINE, bm, Anim::slide(Drive::new(Input::Battery(n as u8), -0.0018), gf.n()), face, gf.n(), None);
        b.pieces[piece].hotspot = Some(Hotspot::click(gf.n() * 0.004, 0.0125, ["battery_l", "battery_r"][n]));
        let mut lit = Mesh::default();
        rect(&mut lit, &legend_mat(style::TEXT, sid("ON"), Vec3::new(0.06, 0.20, 0.55)), face - gf.n() * 0.0012 + gf.ay * -0.004, gf.ax, gf.ay, 0.006, 0.003);
        b.add("battery ON legend", G_ENGINE, lit, Anim::slide(Drive::new(Input::Battery(n as u8), 0.0016), gf.n()), face, gf.n(), Some(piece));
    }
    // The L and R ENG start buttons (the sim's one starter for both engines).
    for at in start_at {
        let gf = Face { o: at, ..g1 };
        let mut sm = Mesh::default();
        let face = gf.at_h(0.0, 0.0, 0.0080);
        sm.rbox(&KEY, face - gf.n() * 0.004, Vec3::new(0.0120, 0.0105, 0.004), 0.0012, gf.rot());
        rect(&mut sm, &legend_mat(cap::PLAIN, 0, Vec3::new(0.018, 0.019, 0.021)), face + gf.n() * 0.0002, gf.ax, gf.ay, 0.0105, 0.0092);
        let piece = b.add("engine start button", G_ENGINE, sm, Anim::slide(Drive::new(Input::Ignition, -0.002).smooth(1.1, 1.4), gf.n()), face, gf.n(), None);
        b.pieces[piece].hotspot = Some(Hotspot::click(gf.n() * 0.004, 0.0125, "starter"));
    }
    // The landing lights: L and R LDG on the exterior lights panel. (Their
    // targets sit just inside the caps' faces: the panel's forward row is
    // close to the overhead's lip, and a sphere poking below it would be
    // picked looking out through the windshield.)
    let g = on(&oh_face(0).0);
    for k in 0..2 {
        let gf = Face { o: g.at(EXT_U0 + k as f32 * EXT_DU, -0.030), ..g };
        let mut lm = Mesh::default();
        let face = gf.at_h(0.0, 0.0, 0.0080);
        lm.rbox(&KEY, face - gf.n() * 0.004, Vec3::new(0.0120, 0.0110, 0.004), 0.0012, gf.rot());
        rect(&mut lm, &legend_mat(cap::PLAIN, 0, Vec3::new(0.018, 0.019, 0.021)), face + gf.n() * 0.0002, gf.ax, gf.ay, 0.0105, 0.0095);
        let piece = b.add("landing light switch", G_ENGINE, lm, Anim::slide(Drive::new(Input::LandingLights, -0.0018), gf.n()), face, gf.n(), None);
        b.pieces[piece].hotspot = Some(Hotspot::click(gf.n() * -0.004, 0.0105, "landing_lights"));
        let mut lit = Mesh::default();
        rect(&mut lit, &legend_mat(style::TEXT, sid("ON"), Vec3::new(0.06, 0.20, 0.55)), face - gf.n() * 0.0012 + gf.ay * -0.004, gf.ax, gf.ay, 0.006, 0.003);
        b.add("landing light ON legend", G_ENGINE, lit, Anim::slide(Drive::new(Input::LandingLights, 0.0016), gf.n()), face, gf.n(), Some(piece));
    }
}

/// The exterior lights panel: its switches' names, the aft row first, and
/// where the columns are.
const EXT_LIGHTS: [&[&str]; 2] = [&["BCN", "STROBE", "NAV", "ICE", "LOGO"], &["L LDG", "R LDG", "PULSE", "TAXI", "WING TIP", "WHL WELL"]];
const EXT_U0: f32 = -0.310;
const EXT_DU: f32 = 0.0405;
/// The L and R ENG start buttons and the L and R main battery switches (u on the overhead).
const START_U: [f32; 2] = [-0.296, -0.264];
const BATT_U: [f32; 2] = [-0.290, -0.242];

/// The circuit breakers, as the photograph of the overhead shows them
/// (Wikimedia Commons BL7C0736): rows A (aft) to G (forward), columns 1 to
/// 9 from the outboard edge. "-" is no breaker, "" a breaker whose label
/// could not be read, a leading '!' a red collar.
const CB_LEFT: [[&str; 9]; 7] = [
    ["!CLOCK 1", "!MAGNETOMETER 1", "!AHRS 1", "!MAU 3B PRI", "MAU 3A PRI", "MAU 2B SEC", "MAU 2A SEC", "MAU 1B SEC", "!MAU 1A PRI"],
    ["HUD FAN/VIDEO AMP", "HUD PWR 1", "!DU 1", "DU 2", "!DU 4 SEC", "!ADS STBY CH 3B", "!ADS STBY CH 4B", "ADS 3 CH 3A", "ADS 3 CH 4A"],
    ["!MCDU 1", "!CMC SHUTDOWN PWR", "!SMC 1", "!FLT GUID PNL 1", "!MAU 1 TONE GEN", "-", "!L MAIN PUMP CONT", "!L FADEC CH A", "!R FADEC CH A"],
    ["!DOOR CTRL/WARN", "!AP PITCH SERVO 1", "!AP ROLL SERVO 1", "-", "!L/R IGN 1", "!L/R START A", "!L HPSOV", "-", "!FIRE EXT SHOT 1"],
    ["-", "!LDG GEAR CTRL 1", "!INBD BRAKE CTRL", "-", "!APU DOOR", "!APU CTRL 1", "!APU PWR 1", "!APU FIRE EXT", "!L GCU PWR"],
    ["", "!ATC 1", "!NAV RCVR 1", "!VHF COMM 1", "!L SSPC CH 1", "!R SSPC CH 1", "!", "!TRU CTRL 1", "!"],
    ["!", "-", "!", "!", "!", "!", "!", "!", "!"],
];
const CB_RIGHT: [[&str; 9]; 7] = [
    ["!CLOCK 2", "!MAGNETOMETER 2", "!AHRS 2", "MAU 3B SEC", "MAU 3A SEC", "!MAU 2B PRI", "MAU 2A PRI", "!MAU 1B PRI", "MAU 1A SEC"],
    ["-", "HUD PWR 2", "!DU 4 PRI", "DU 3", "-", "ADS 2 CH 2B", "ADS 2 CH 1B", "!ADS 1 CH 2A", "!ADS 1 CH 1A"],
    ["-", "!MCDU 3", "!SMC 2", "!FLT GUID PNL 2", "!MAU 2 TONE GEN", "-", "!R MAIN PUMP CONT", "!L FADEC CH B", "!R FADEC CH B"],
    ["!LDG GEAR DUMP", "!AP PITCH SERVO 2", "!AP ROLL SERVO 2", "-", "!L/R IGN 2", "!L/R START B", "!R HPSOV", "THROT QUAD", "!FIRE EXT SHOT 2"],
    ["!LDG GEAR IND", "LDG GEAR CTRL 2", "!OUTBD BRAKE CTRL", "-", "-", "!APU CTRL 2", "!APU PWR 2", "-", "!R GCU PWR"],
    ["", "ATC 2", "NAV RCVR 2", "VHF COMM 2", "!L SSPC CH 2", "!R SSPC CH 2", "!BATT BUS CONT", "!TRU CTRL 2", "!"],
    ["!COPILOT ACP", "!OBSERVER ACP", "!", "", "!", "!", "!", "!", "!"],
];
/// The breaker groups' titles: (row, column, title) printed under that row,
/// centred on that column (1 to 9 from the outboard edge).
const CB_GROUPS: [(usize, f32, &str); 5] = [(2, 3.5, "FLT INSTRUMENTS"), (4, 2.5, "FLT CTRL/LDG GEAR"), (4, 7.0, "ENG SYS/FUEL"), (6, 2.5, "COM/NAV"), (6, 7.0, "ELECTRICAL")];

/// The circuit breakers of one side (`side` -1 the left panel, 1 the right),
/// with their labels, the row letters and column numbers and the groups' titles.
fn breakers(m: &mut Mesh, f: &Face, u0: f32, v0: f32, u1: f32, v1: f32, side: f32) {
    let (nu, nv) = (9usize, 7usize);
    let (pu, pv) = ((u1 - u0) / nu as f32, (v1 - v0) / nv as f32);
    let grid = if side < 0.0 { &CB_LEFT } else { &CB_RIGHT };
    // Column c (1 from the outboard edge) and row r (0 = A, aft) to u, v.
    let at = |r: usize, c: f32| {
        let i = if side < 0.0 { c - 1.0 } else { 9.0 - c };
        (u0 + (i + 0.5) * pu, v0 + (nv - 1 - r) as f32 * pv + 0.62 * pv)
    };
    let black = mt(mat::PLASTIC, 0.010, 0.010, 0.011);
    let red = mt(mat::PLASTIC, 0.30, 0.025, 0.02);
    for (r, row) in grid.iter().enumerate() {
        for (c, s) in row.iter().enumerate() {
            if *s == "-" {
                continue;
            }
            let (u, v) = at(r, c as f32 + 1.0);
            let cc = f.at(u, v);
            m.lathe(&black, cc, f.n(), &[[0.0050, 0.0], [0.0050, 0.0015], [0.0036, 0.0021], [0.0036, 0.0080], [0.0030, 0.0090], [0.0, 0.0090]], 8, false);
            let (red_top, name) = match s.strip_prefix('!') { Some(n) => (true, n), None => (false, *s) };
            if red_top {
                m.disc(&red, cc + f.n() * 0.0091, f.n(), f.ay, 0.0024, 10);
            }
            label(m, f, u, v - pv * 0.42, pu * 0.95, name);
        }
    }
    for (r, c, s) in CB_GROUPS {
        let (u, v) = at(r, c);
        word(m, f, u, v - pv * 0.72, 0.0026, s);
    }
    for r in 0..nv {
        let (_, v) = at(r, 1.0);
        let u = if side < 0.0 { u0 - 0.007 } else { u1 + 0.007 };
        word(m, f, u, v, 0.0032, ["A", "B", "C", "D", "E", "F", "G"][r]);
    }
    for c in 1..=9 {
        let (u, _) = at(0, c as f32);
        word(m, f, u, v1 + 0.004, 0.0032, ["1", "2", "3", "4", "5", "6", "7", "8", "9"][c - 1]);
    }
}

// ---------------------------------------------------------------- side consoles

fn side_consoles(m: &mut Mesh) {
    for sx in [-1.0f32, 1.0] {
        let (z0, z1) = (bz(CON_D[0]), bz(CON_D[1]));
        let y = by(CON_H);
        m.patch(&PANEL_DK, 2, 16, sx > 0.0, |u, v| {
            let z = z0 + (z1 - z0) * v;
            let x = CON_X + (lin_x(z, y) + 0.01 - CON_X) * u;
            (Vec3::new(sx * x, y, z), [x, z], 0.0)
        });
        let x = sx * CON_X;
        hp::quad_to(m, &PANEL_DK, [Vec3::new(x, FLOOR_Y, z0), Vec3::new(x, FLOOR_Y, z1), Vec3::new(x, y, z1), Vec3::new(x, y, z0)], Vec3::new(0.0, y, (z0 + z1) * 0.5));
        for z in [z0, z1] {
            let (xb, xt) = (sx * (lin_x(z, FLOOR_Y) + 0.02), sx * (lin_x(z, y) + 0.01));
            hp::quad_to(m, &PANEL_DK, [Vec3::new(x, FLOOR_Y, z), Vec3::new(xb, FLOOR_Y, z), Vec3::new(xt, y, z), Vec3::new(x, y, z)], Vec3::new(0.0, y, z + (z - EYE.z) * 4.0));
        }
        m.tube(&PANEL, &[Vec3::new(x, y - 0.006, z0), Vec3::new(x, y - 0.006, z1)], &|_| 0.008, 8, true);
        // Looking down on it: u across (out positive on the right), v forward.
        let f = Face::new(Vec3::new(0.0, y, 0.0), Vec3::X, -Vec3::Z);
        let out = |a: f32| sx * a;
        let (ua, ub) = { let a = out(CON_X + 0.012); let c = out(lin_x(bz(3.2), y) - 0.02); (a.min(c), a.max(c)) };
        // The audio control panel, the lighting panel, the oxygen mask's box.
        // (No photograph shows these panels' legends: blank caps, unnamed knobs.)
        panel(m, &f, ua, -bz(3.18), ub, -bz(3.00), 120.0 + sx, "AUDIO", &[("", &["", "", "", ""]), ("", &["*", "*", "*", "*"])]);
        panel(m, &f, ua, -bz(3.32), ub, -bz(3.20), 121.0 + sx, "LIGHTING", &[("", &["*", "*", "*"])]);
        m.rbox(&mt(mat::PLASTIC, 0.11, 0.115, 0.125), f.at_h((ua + ub) * 0.5, -bz(3.44), 0.03), Vec3::new((ub - ua) * 0.5, 0.08, 0.03), 0.008, f.rot());
        rect(m, &legend_mat(style::RED_PLACARD, sid("OXYGEN"), Vec3::new(0.11, 0.115, 0.125)), f.at_h((ua + ub) * 0.5, -bz(3.44), 0.0605), f.ax, f.ay, 0.03, 0.010);
        // The cursor control device at the forward end, on its angled pod:
        // a palm rest, a thumb wheel, three keys.
        let c = p(sx * 0.925, 2.71, CON_H + 0.035);
        let tilt = Quat::from_rotation_z(-sx * 0.35) * Quat::from_rotation_x(-0.25);
        m.rbox(&PANEL_DK, c - Vec3::Y * 0.02, Vec3::new(0.045, 0.030, 0.065), 0.015, tilt);
        m.rbox(&YOKE, c + tilt * Vec3::new(0.0, 0.016, 0.018), Vec3::new(0.035, 0.012, 0.040), 0.012, tilt);
        m.lathe(&hp::KNOB.pat(pat::KNURL), c + tilt * Vec3::new(sx * 0.024, 0.016, -0.030), tilt * Vec3::X, &[[0.0, -0.006], [0.012, -0.006], [0.012, 0.006], [0.0, 0.006]], 16, false);
        for k in 0..3 {
            m.rbox(&KEY, c + tilt * Vec3::new((k as f32 - 1.0) * 0.016, 0.022, -0.035), Vec3::new(0.006, 0.004, 0.010), 0.003, tilt);
        }
        // The pedal steering switchlight (and on the left, the tiller ahead of it: a piece of its own).
        let g = on(&f);
        sw(m, &g, out(0.90), -bz(2.95), 0.011, 0.0085, cap::PLAIN, sid("PEDAL STEERING"));
    }
}

// ---------------------------------------------------------------- seats

fn seat(m: &mut Mesh, sx: f32) {
    let x = sx * SEAT_X;
    let fl = FLOOR_Y + CG_H;
    let at = |dx: f32, d: f32, h: f32| p(x + dx, d, fl + h);
    // The pedestal on the tracks, the pan and cushion.
    m.rbox(&SEAT_SHELL, at(0.0, 3.14, 0.12), Vec3::new(0.16, 0.11, 0.20), 0.03, Quat::IDENTITY);
    m.rbox(&SEAT_SHELL, at(0.0, 3.10, 0.27), Vec3::new(0.24, 0.04, 0.23), 0.03, Quat::IDENTITY);
    m.rbox(&LEATHER, at(0.0, 3.07, 0.33), Vec3::new(0.23, 0.05, 0.23), 0.045, Quat::from_rotation_x(-0.06));
    // The back, leaning back, and the headrest behind the pilot's head.
    let lean = Quat::from_rotation_x(0.22);
    let bc = at(0.0, 3.37, 0.72);
    m.rbox(&SEAT_SHELL, bc + Vec3::new(0.0, 0.0, 0.05), Vec3::new(0.24, 0.36, 0.035), 0.03, lean);
    m.rbox(&LEATHER, bc, Vec3::new(0.225, 0.34, 0.05), 0.045, lean);
    m.rbox(&LEATHER, at(0.0, 3.30, 1.27), Vec3::new(0.13, 0.08, 0.05), 0.04, lean);
    // Leather pleats down the back.
    for k in 0..3 {
        m.rbox(&LEATHER_DK, bc + lean * Vec3::new((k as f32 - 1.0) * 0.07, 0.0, -0.051), Vec3::new(0.003, 0.30, 0.002), 0.001, lean);
    }
    // Armrests both sides, folded down.
    for ax in [-1.0f32, 1.0] {
        let q = at(ax * 0.265, 3.14, 0.58);
        m.rbox(&LEATHER, q, Vec3::new(0.032, 0.032, 0.15), 0.028, Quat::IDENTITY);
        m.rbox(&SEAT_SHELL, q + Vec3::new(0.0, -0.06, 0.12), Vec3::new(0.022, 0.07, 0.025), 0.012, Quat::IDENTITY);
    }
    // The harness: shoulder straps over the back to the buckle on the cushion.
    for dx in [-0.08f32, 0.08] {
        let a = bc + lean * Vec3::new(dx, 0.34, -0.02);
        m.tube(&mt(mat::FABRIC, 0.06, 0.06, 0.065), &[a, a + Vec3::new(0.0, -0.02, -0.06), at(dx * 1.4, 3.08, 0.40)], &|_| 0.010, 6, true);
    }
    m.rbox(&CHROME, at(0.0, 2.98, 0.39), Vec3::new(0.045, 0.007, 0.035), 0.007, Quat::IDENTITY);
}

// ---------------------------------------------------------------- what moves

fn controls(b: &mut Builder, f: &Face) {
    let piv = |x: f32| p(x, THR_PIVOT[0], THR_PIVOT[1]);
    // The two thrust levers: a flat stem up out of its slot to a polished
    // cylindrical grip (TOGA button on its outboard end), the reverse lever
    // piggy-backed on the stem's front.
    for x in THR_X {
        let pv = piv(x);
        let s = x.signum();
        let mut m = Mesh::default();
        m.rbox(&mt(mat::PLASTIC, 0.03, 0.03, 0.032), pv + Vec3::new(0.0, (0.19 + THR_ARM) * 0.5, 0.0), Vec3::new(0.006, (THR_ARM - 0.19) * 0.5, 0.010), 0.003, Quat::IDENTITY);
        let g = pv + Vec3::new(0.0, THR_ARM, 0.0);
        m.lathe(&POLISH.pat(pat::KNURL), g - Vec3::X * 0.0375, Vec3::X, &[[0.0, 0.0], [0.014, 0.0], [0.0165, 0.003], [0.0165, 0.072], [0.014, 0.075], [0.0, 0.075]], 20, false);
        m.lathe(&KEY, g + Vec3::X * (s * 0.0375), Vec3::X * s, &[[0.0, 0.0], [0.009, 0.0], [0.009, 0.004], [0.0, 0.005]], 12, false);
        m.rbox(&mt(mat::PAINT, 0.35, 0.03, 0.02), g + Vec3::new(-s * 0.026, 0.0, -0.016), Vec3::new(0.006, 0.004, 0.002), 0.001, Quat::IDENTITY);
        let k = b.add("thrust lever", G_ENGINE, m, Anim::hinge(Drive::new(Input::Throttle, -0.60).offset(0.30)), pv, Vec3::X, None);
        // (The target covers most of the 75 mm grip; it sits aft and a little
        // low of the grip's axis so that aiming at the reverse lever's grip,
        // just ahead of it, does not pick the thrust lever.)
        b.pieces[k].hotspot = Some(Hotspot::wheel(Vec3::new(0.0, THR_ARM - 0.002, 0.009), 0.028, "throttle_up", "throttle_down"));
        // (The reverse lever rides on the stem's front, its grip just ahead of and above the thrust lever's.)
        let mut r = Mesh::default();
        let rk = pv + Vec3::new(0.0, THR_ARM + 0.020, -0.031);
        r.tube(&mt(mat::PLASTIC, 0.02, 0.02, 0.022), &[pv + Vec3::new(0.0, THR_ARM - 0.030, -0.016), pv + Vec3::new(0.0, THR_ARM - 0.015, -0.024), rk], &|_| 0.0035, 8, false);
        r.rbox(&POLISH, rk, Vec3::new(0.011, 0.006, 0.007), 0.004, Quat::IDENTITY);
        let k = b.add("reverse lever", G_ENGINE, r, Anim::hinge2(Drive::new(Input::Throttle, -0.60).offset(0.30), Drive::new(Input::Reverser, 0.35)), pv, Vec3::X, None);
        b.pieces[k].hotspot = Some(Hotspot::hold(rk - pv + Vec3::new(0.0, 0.003, 0.0), 0.012, "reversers"));
    }

    // Speed brake and flap handles, hinged inside the pedestal's top.
    let hp_top = PED_H[2] + 0.02 * (2.835 - PED_D[2]) / (PED_D[3] - PED_D[2]);
    for (x, name) in [(-0.165f32, "speedbrake handle"), (0.165, "flap lever")] {
        let pv = p(x, 2.835, hp_top - 0.06);
        let mut m = Mesh::default();
        m.tube(&mt(mat::PLASTIC, 0.03, 0.03, 0.032), &[pv + Vec3::Y * 0.05, pv + Vec3::Y * 0.135], &|_| 0.0045, 8, false);
        let kc = pv + Vec3::Y * 0.145;
        let k = if x < 0.0 {
            // A T-handle, its grip across.
            m.lathe(&mt(mat::PLASTIC, 0.12, 0.12, 0.13), kc - Vec3::X * 0.028, Vec3::X, &[[0.0, 0.0], [0.010, 0.0], [0.012, 0.004], [0.012, 0.052], [0.010, 0.056], [0.0, 0.056]], 14, false);
            b.add(name, G_ENGINE, m, Anim::hinge(Drive::new(Input::Speedbrake, 1.2).offset(-0.25)), pv, Vec3::X, None)
        } else {
            // The flap handle's knob, shaped like a wing section.
            m.rbox(&mt(mat::PLASTIC, 0.70, 0.70, 0.68), kc, Vec3::new(0.022, 0.010, 0.016), 0.008, Quat::IDENTITY);
            b.add(name, G_ENGINE, m, Anim::hinge(Drive::new(Input::FlapLever, 0.70).offset(-0.30)), pv, Vec3::X, None)
        };
        b.pieces[k].hotspot = Some(if x < 0.0 { Hotspot::click(Vec3::Y * 0.145, 0.026, "speedbrake") } else { Hotspot::wheel(Vec3::Y * 0.145, 0.018, "flaps_up", "flaps_down") });
    }

    // The pitch trim switch on the trim panel: a split rocker.
    let tp = p(-0.050, 2.80, PED_H[2] + 0.02 * (2.80 - PED_D[2]) / (PED_D[3] - PED_D[2]) + 0.0048);
    let mut t = Mesh::default();
    for dx in [-0.006f32, 0.006] {
        t.rbox(&KEY, tp + Vec3::new(dx, 0.008, 0.0), Vec3::new(0.0045, 0.008, 0.010), 0.002, Quat::IDENTITY);
    }
    let k = b.fixed("pitch trim switch", G_ENGINE, t);
    b.pieces[k].hotspot = Some(Hotspot::wheel(tp + Vec3::Y * 0.018, 0.012, "trim_down", "trim_up"));

    // The park/emergency brake: a big polished lever lying forward along its
    // quadrant, its front end raised to set the brake.
    let top = |d: f32| PED_H[2] + 0.02 * (d - PED_D[2]) / (PED_D[3] - PED_D[2]);
    let pv = p(PB_X, 3.15, top(3.15) + 0.008);
    let mut pb = Mesh::default();
    pb.lathe(&POLISH, pv - Vec3::X * 0.012, Vec3::X, &[[0.0, 0.0], [0.010, 0.0], [0.010, 0.024], [0.0, 0.024]], 12, false);
    pb.rbox(&POLISH, pv + Vec3::new(0.0, 0.006, -0.075), Vec3::new(0.008, 0.006, 0.075), 0.004, Quat::IDENTITY);
    pb.rbox(&mt(mat::PLASTIC, 0.05, 0.05, 0.055), pv + Vec3::new(0.0, 0.012, -0.150), Vec3::new(0.022, 0.012, 0.016), 0.008, Quat::IDENTITY);
    let k = b.add("parking brake lever", G_ENGINE, pb, Anim::hinge(Drive::new(Input::ParkingBrake, 0.45)), pv, Vec3::X, None);
    // (The target takes in the handle and the end of the bar it is on.)
    b.pieces[k].hotspot = Some(Hotspot::click(Vec3::new(0.0, 0.014, -0.140), 0.026, "parking_brake"));

    // The fuel control switches, L and R, lever-locked in their red housings: up is RUN.
    let g = on(f);
    for (n, sx) in [-1.0f32, 1.0].into_iter().enumerate() {
        let c = g.at_h(sx * 0.262, -0.215, 0.012);
        let mut s = Mesh::default();
        s.lathe(&CHROME, c, g.n(), &[[0.0040, 0.0], [0.0040, 0.0030], [0.0028, 0.0040], [0.0028, 0.022], [0.0, 0.023]], 10, false);
        s.dome(&mt(mat::PLASTIC, 0.05, 0.05, 0.05), c + g.n() * 0.022, g.n(), 0.0055, 0.008, 10, 0.0);
        let k = b.add("fuel control switch", G_ENGINE, s, Anim::hinge(Drive::new(Input::FuelControl(n as u8), -0.80).smooth(0.2, 0.8).offset(0.40)), c, g.ax, None);
        b.pieces[k].hotspot = Some(Hotspot::click(g.n() * 0.026, 0.013, ["fuel_control_l", "fuel_control_r"][n]));
    }

    // The landing gear handle: a wheel-shaped knob on its arm, up and down the slot.
    let w = g.sub(0.14, -0.222, 0.0);
    let mut gh = Mesh::default();
    gh.tube(&STEEL, &[w.at_h(0.0, 0.0, -0.01), w.at_h(0.0, 0.0, 0.048)], &|_| 0.0042, 10, false);
    let c = w.at_h(0.0, 0.0, 0.052);
    gh.lathe(&mt(mat::PLASTIC, 0.78, 0.78, 0.76), c - w.ax * 0.012, w.ax, &[[0.0, 0.0], [0.016, 0.0], [0.019, 0.003], [0.019, 0.021], [0.016, 0.024], [0.0, 0.024]], 20, false);
    // (its red disagree light, dark: the gear is not simulated in transit against the handle)
    gh.lathe(&mt(mat::PLASTIC, 0.16, 0.02, 0.02), c - w.ax * 0.0125, w.ax, &[[0.0, 0.0], [0.008, 0.0], [0.008, 0.025], [0.0, 0.025]], 12, false);
    let k = b.add("gear lever", G_ENGINE, gh, Anim::slide(Drive::new(Input::GearHandle, -0.026), w.ay), w.o, Vec3::X, None);
    b.pieces[k].hotspot = Some(Hotspot::click(c - w.o, 0.020, "gear"));

    // The control columns and the yokes.
    for sx in [-1.0f32, 1.0] {
        let x = sx * SEAT_X;
        let base = p(x, 2.42, FLOOR_Y + CG_H);
        let top = p(x, 2.53, 2.62);
        let mut c = Mesh::default();
        c.lathe(&mt(mat::RUBBER, 0.03, 0.03, 0.032).pat(pat::KNURL), base, Vec3::Y, &[[0.055, 0.0], [0.055, 0.02], [0.045, 0.06], [0.050, 0.10], [0.040, 0.13]], 16, false);
        c.tube(&YOKE, &[base + Vec3::Y * 0.10, top], &|t| 0.033 - 0.006 * t, 14, false);
        let ci = b.add("control column", G_ENGINE, c, Anim::hinge(Drive::new(Input::Pitch, 0.09)), base, Vec3::X, None);
        // The yoke: a tall hub with Gulfstream's plate, the horns sweeping down
        // and out and up into the grips.
        let hub = p(x, 2.565, 2.69);
        let n = Vec3::new(0.0, 0.30, 1.0).normalize();
        let rot = Quat::from_rotation_arc(Vec3::Z, n);
        let mut y = Mesh::default();
        y.rbox(&YOKE, hub, Vec3::new(0.055, 0.045, 0.035), 0.016, rot);
        y.rbox(&mt(mat::PLASTIC, 0.035, 0.035, 0.038), hub + rot * Vec3::new(0.0, 0.004, 0.034), Vec3::new(0.045, 0.030, 0.003), 0.004, rot);
        rect(&mut y, &legend_mat(style::TEXT, sid("GULFSTREAM"), Vec3::new(0.035, 0.035, 0.038)), hub + rot * Vec3::new(0.0, -0.006, 0.0375), rot * Vec3::X, rot * Vec3::Y, 0.030, 0.006);
        for s in [-1.0f32, 1.0] {
            let horn = hp::catmull(&[(0.040, -0.010), (0.085, -0.040), (0.135, -0.040), (0.160, -0.005), (0.165, 0.060), (0.160, 0.115)].map(|(a, c)| hub + rot * Vec3::new(s * a, c, -0.005)), 4);
            y.tube(&YOKE, &horn, &|t| 0.015 + 0.007 * smoothstep(0.5, 0.75, t), 12, true);
            // Push-to-talk and the autopilot disconnect (red) on the top of each grip.
            y.dome(&mt(mat::PAINT, 0.40, 0.03, 0.02), hub + rot * Vec3::new(s * 0.157, 0.122, 0.006), rot * Vec3::new(0.0, 1.0, 0.3), 0.0055, 0.004, 10, 0.0);
            y.rbox(&KEY, hub + rot * Vec3::new(s * 0.168, 0.090, 0.016), Vec3::new(0.004, 0.008, 0.004), 0.002, rot);
        }
        let yi = b.add("yoke", G_ENGINE, y, Anim::hinge(Drive::new(Input::Roll, -0.8)), hub, n, Some(ci));
        // The pitch trim rocker under the left grip's thumb (the captain's works).
        if sx < 0.0 {
            let tc = hub + rot * Vec3::new(-0.150, 0.100, 0.020);
            let mut t = Mesh::default();
            t.rbox(&KEY, tc, Vec3::new(0.006, 0.009, 0.004), 0.002, rot);
            let ti = b.add("yoke trim switch", G_ENGINE, t, Anim::Fixed, Vec3::ZERO, Vec3::X, Some(yi));
            b.pieces[ti].hotspot = Some(Hotspot::wheel(tc + rot * Vec3::new(0.0, 0.0, 0.006), 0.010, "trim_down", "trim_up"));
        }
        // Rudder pedals with their toe brakes.
        for dx in [-0.10f32, 0.10] {
            let mut pm = Mesh::default();
            let pv = p(x + dx, 2.08, 2.58);
            let pc = p(x + dx, 2.15, 2.34);
            pm.tube(&STEEL, &[pv, pc], &|_| 0.010, 8, true);
            let tilt = Quat::from_rotation_x(-0.30);
            pm.rbox(&mt(mat::ALU, 0.32, 0.33, 0.34), pc + Vec3::new(0.0, 0.0, 0.010), Vec3::new(0.045, 0.080, 0.008), 0.006, tilt);
            pm.rbox(&RUBBER.pat(pat::KNURL), pc + Vec3::new(0.0, 0.0, 0.019), Vec3::new(0.040, 0.070, 0.0025), 0.002, tilt);
            let k = b.add("rudder pedal", G_ENGINE, pm, Anim::hinge2(Drive::new(Input::Yaw, dx.signum() * 0.16), Drive::new(Input::Brake(dx.signum()), 0.12)), pv, Vec3::X, None);
            // (Only the captain's toe brakes are targets: from the left seat
            // the first officer's are behind the pedestal, and their rays
            // pass the gear handle and the thrust levers.)
            if sx < 0.0 {
                b.pieces[k].hotspot = Some(Hotspot::hold(pc - pv + Vec3::new(0.0, 0.0, 0.03), 0.036, if dx < 0.0 { "brake_left" } else { "brake_right" }));
            }
        }
    }

    // The nose wheel tiller on the captain's console.
    let tc = p(-0.972, 2.86, CON_H);
    let mut t = Mesh::default();
    t.lathe(&YOKE, tc, Vec3::Y, &[[0.012, 0.0], [0.012, 0.022], [0.050, 0.028], [0.055, 0.036], [0.050, 0.042], [0.0, 0.044]], 20, false);
    t.tube(&YOKE, &[tc + Vec3::new(0.036, 0.042, 0.0), tc + Vec3::new(0.036, 0.075, 0.0)], &|_| 0.0065, 8, true);
    b.add("tiller", G_ENGINE, t, Anim::hinge(Drive::new(Input::Steer, -1.8)), tc, Vec3::Y, None);
}

// ---------------------------------------------------------------- what the eye sees

fn in_overhead(q: Vec3) -> bool {
    let d = q.z + CG_D;
    if !(OH_D[0]..OH_D[3]).contains(&d) {
        return false;
    }
    let k = if d < OH_D[1] { 0 } else if d < OH_D[2] { 1 } else { 2 };
    let t = (d - OH_D[k]) / (OH_D[k + 1] - OH_D[k]);
    let h = oh_h(OH_D[k]) + (oh_h(OH_D[k + 1]) - oh_h(OH_D[k])) * t;
    q.x.abs() < OH_W[k] && q.y + CG_H > h
}

/// Does a look from the seat end on the flight deck, rather than out through
/// a window? Followed out to the lining: it is a window only where the skin
/// beyond has an opening.
pub fn sees_inside(eye: Vec3, d: Vec3) -> bool {
    let d = d.normalize_or_zero();
    let mut t = 0.04;
    while t < 6.0 {
        let q = eye + d * t;
        let (dd, h) = (q.z + CG_D, q.y + CG_H);
        if dd > D_AFT - 0.01 || dd < D_FWD || q.y < FLOOR_Y + 0.005 {
            return true;
        }
        // The panel and the glareshield, the overhead.
        if dd < LIP[0] + 0.03 && h < LIP[1] {
            return true;
        }
        if in_overhead(q) {
            return true;
        }
        if skin_depth(q) < INSET {
            return glaze_sd(to_skin(q)) >= 0.0;
        }
        t += 0.015;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing on the flight deck pokes out through the skin.
    #[test]
    fn deck_fits_inside_the_skin() {
        let mut b = Builder::default();
        build(&mut b);
        let mut worst = ("", Vec3::ZERO, 1.0f32);
        let mut n = 0;
        for pc in &b.pieces {
            for v in &pc.mesh.v {
                let q = Vec3::from(v.pos);
                if skin_depth(q) < worst.2 {
                    worst = (pc.name, q, skin_depth(q));
                }
                n += 1;
            }
        }
        for pc in &b.pieces {
            if pc.mesh.v.len() > 5000 {
                println!("  {}: {} vertices", pc.name, pc.mesh.v.len());
            }
        }
        println!("{} flight deck pieces, {n} vertices; the closest to the skin: {} at {:?} ({:.4} m in)", b.pieces.len(), worst.0, worst.1, worst.2);
        assert!(worst.2 > 0.0, "{} sticks out of the skin at {:?} ({:.4})", worst.0, worst.1, worst.2);
    }

    #[test]
    fn the_captain_sees_out_ahead_and_to_his_left() {
        let dir = |yaw: f32, pitch: f32| Vec3::new(yaw.sin() * pitch.cos(), -pitch.sin(), -yaw.cos() * pitch.cos());
        assert!(!sees_inside(EYE, dir(0.0, 0.0)), "ahead: the windshield");
        assert!(!sees_inside(EYE, dir(0.0, -0.3)), "ahead and up: the windshield");
        assert!(sees_inside(EYE, dir(0.0, 0.6)), "down ahead: the panel");
        assert!(sees_inside(EYE, dir(0.0, -1.4)), "up: the overhead");
        assert!(!sees_inside(EYE, dir(-1.4, 0.15)), "to the left, a little down: the side window");
        assert!(sees_inside(EYE, dir(PI, 0.0)), "behind: the bulkhead");
        // The view down over the nose, past the glareshield.
        let cut = (0..60).map(|k| k as f32 * 0.5).find(|a| sees_inside(EYE, dir(0.0, a.to_radians()))).unwrap();
        println!("the glareshield cuts the view ahead at {cut:.1} degrees down");
        assert!(cut > 12.0 && cut < 25.0, "{cut}");
    }

    /// The lining's openings are the skin's (`in_glazing`, from the exterior's outlines).
    #[test]
    fn openings_agree_with_the_glazing() {
        let mut bad = 0;
        let mut n = 0;
        for i in 0..60 {
            for j in 0..60 {
                let z = bz(D_FWD + (D_AFT - D_FWD) * i as f32 / 59.0);
                let q = skin_point(z, 0.15 + (PI - 0.3) * j as f32 / 59.0, 0.0);
                let sd = glaze_sd(q);
                // (in_glazing tells the windshield from above at any height: only
                // above the sill is it the glass; and it takes the side window's
                // top as the straight line between its corners, where the
                // outline, and the cut in the skin, bow up a little over the crown)
                let h = q.y + CG_H;
                if sd.abs() > 0.02 && h > WS_SILL + 0.02 && !(h > GLAZE_TOP - 0.01 && q.x.abs() > 0.30) {
                    n += 1;
                    if (sd < 0.0) != in_glazing(q) {
                        if bad < 12 {
                            println!("disagree at d {:.2} h {h:.2} x {:.2}: sd {sd:.3}, in_glazing {}", q.z + CG_D, q.x, in_glazing(q));
                        }
                        bad += 1;
                    }
                }
            }
        }
        assert!(bad * 100 < n, "{bad} of {n} disagree");
    }

    /// Where each mouse target is on a 1600 x 900 screen (58 degrees of view)
    /// for a few looks from the seat: for scripting `--cursor` / `--click` /
    /// `--wheel` checks. (`G650_VIEWS="yaw_deg,pitch_rad;..."` picks the looks.)
    #[test]
    #[ignore]
    fn where_the_targets_are_on_screen() {
        let model = (SPEC.build)();
        let controls = crate::aircraft::cockpit_controls(&SPEC, &model.pieces);
        let mut plane = crate::plane::Plane::spawn(&SPEC, 0.6, -2.1, 1000.0, 0.0, 120.0);
        if std::env::var("G650_COLD").is_ok() {
            plane.cold_and_dark();
            plane.gear_down = true;
            plane.gear = 1.0;
        }
        let pose = plane.pose(&model.pieces);
        let hs = crate::aircraft::animated_hotspots(&controls, &pose);
        let views: Vec<(f64, f64)> = std::env::var("G650_VIEWS").ok().map(|v| v.split(';').filter_map(|w| { let a: Vec<f64> = w.split(',').filter_map(|x| x.parse().ok()).collect(); (a.len() == 2).then(|| (a[0], a[1])) }).collect()).unwrap_or(vec![(0.0, 0.45), (15.0, 0.75), (0.0, 0.2), (40.0, -0.9), (25.0, 1.0)]);
        let (w, h) = (1600.0f32, 900.0f32);
        for (yd, pt) in views {
            let mut head = crate::cockpit::Head::new(yd.to_radians(), pt);
            let mut view = None;
            for _ in 0..40 {
                view = Some(head.update(&SPEC, 0.1, (yd.to_radians(), pt), 58.0, glam::DVec3::ZERO, glam::DQuat::IDENTITY, glam::DVec3::Y, w / h, 1.0));
            }
            let view = view.unwrap();
            let inv = view.rot.inverse().as_quat();
            let t = (view.fov.to_radians() * 0.5).tan();
            println!("view yaw {yd} deg, pitch {pt} rad:");
            let blockers = crate::cockpit::blockers(&model.pieces);
            for hh in &hs {
                let eye = view.eye.as_vec3();
                let dir = (Vec3::from(hh.at) - eye).normalize();
                let pk = crate::aircraft::pick(&hs, eye, dir);
                let what = match pk {
                    Some(i) => {
                        let depth = (Vec3::from(hs[i].at) - eye).dot(dir) - hs[i].r - 0.005;
                        let occ = crate::cockpit::occluded(&model.pieces, &pose, &blockers, eye, dir, depth);
                        format!("picks {}{}", hs[i].click.or(hs[i].wheel.map(|w| w.0)).or(hs[i].hold).unwrap_or(""), if occ { " OCCLUDED" } else { "" })
                    }
                    None => "picks nothing".into(),
                };
                let v = inv * (Vec3::from(hh.at) - view.eye.as_vec3());
                if v.z >= -0.01 {
                    continue;
                }
                let (x, y) = (v.x / -v.z / (t * w / h), v.y / -v.z / t);
                if x.abs() < 0.98 && y.abs() < 0.98 {
                    let id = hh.click.or(hh.wheel.map(|w| w.0)).or(hh.hold).unwrap_or("");
                    println!("  {:>16} at {:.0},{:.0}  {what}", id, (x + 1.0) * 0.5 * w, (1.0 - y) * 0.5 * h);
                }
            }
        }
    }

    /// The trims cover the lining's cut edges: no hole in the lining reaches
    /// past the trim round the opening it is cut for.
    #[test]
    fn trims_cover_the_lining_cuts() {
        let g = glaze();
        let mut worst = (0.0f32, Vec3::ZERO);
        let (nz, na) = (140usize, 240usize);
        let mut bad = 0;
        for i in 0..nz {
            let z = bz(D_FWD + (D_AFT - D_FWD) * (i as f32 + 0.5) / nz as f32);
            for j in 0..na {
                let a = 0.2 + (PI - 0.4) * (j as f32 + 0.5) / na as f32;
                let q = skin_point(z, a, 0.0);
                let sd = glaze_sd(q);
                if sd <= 0.0 || sd > 0.08 {
                    continue;
                }
                let k = (0..4).min_by(|&x, &y| ring_dist(&g.rings[x], q).total_cmp(&ring_dist(&g.rings[y], q))).unwrap();
                let w = trim_w(q);
                // (ragged by a cell past the cut, unless the next opening's trim is over it)
                if sd < cut_at(q) + 0.002 && sd > w + 0.002 && other_dist(k, q) > trim_w(q) + 0.002 {
                    bad += 1;
                    if sd - w > worst.0 {
                        worst = (sd - w, q);
                    }
                }
            }
        }
        println!("{bad} points of the lining in a cut past the trim; worst {:.3} m at d {:.2} h {:.2} x {:.2}", worst.0, worst.1.z + CG_D, worst.1.y + CG_H, worst.1.x);
        assert_eq!(bad, 0);
    }

    /// The displays are where the screen ids say, and the controls the mouse
    /// works sit on the pieces that show them.
    #[test]
    fn screens_and_targets() {
        let mut b = Builder::default();
        build(&mut b);
        let mut ids = std::collections::BTreeSet::new();
        for pc in &b.pieces {
            for v in &pc.mesh.v {
                if v.mat[0] == mat::DISPLAY && v.mat[3] >= 60.0 {
                    ids.insert(v.mat[3] as u32);
                }
            }
        }
        for id in 60..=78 {
            assert!(ids.contains(&id), "no screen {id}");
        }
        let n = b.pieces.iter().filter(|p| p.hotspot.is_some()).count();
        println!("{n} mouse targets; screens {ids:?}");
        assert!(n >= 30);
    }
}
