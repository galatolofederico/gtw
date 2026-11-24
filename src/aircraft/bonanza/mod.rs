//! A Beechcraft A36 Bonanza: the straight-tailed, six-seat Bonanza with the
//! big double cargo doors on the right side, as built from 1984 (serial
//! E-2111 on) with the 300 hp Continental IO-550-B, a three-blade McCauley
//! constant-speed propeller and electric retractable tricycle gear. Built in
//! Rust like the Cessna and the Robin (see `c172`, `dr400`): a lofted
//! superellipse fuselage, NACA 230-series wings and a straight tail, gear
//! that folds on plain hinges (no linkage kinematics of its own: every piece
//! moves with `Anim::Move`). The cockpit and cabin are in `cabin.rs`.
//!
//! # References (read these before changing the shape or the numbers)
//!
//! - [S] Beech Bonanza 36/A36/A36TC Shop Manual (36-590001-9, 1998 printing),
//!   Section 1 "General Information": three-view Figure 1-1A (E-185 and
//!   after), dimensions, control surface travel, tyres, strut extension.
//!   <http://www.aeroelectric.com/Reference_Docs/Beech/beech_36_a36_a36tc_shop_manual_v1998.pdf>
//! - [T] EASA TCDS EASA.IM.A.279 (Beechcraft 33/35/36, model G36, which
//!   keeps the A36 airframe, IO-550-B and propellers; certification basis
//!   FAA TCDS 3A15). <https://www.easa.europa.eu/en/downloads/7204/en>
//! - [A] D. Lednicer, The Incomplete Guide to Airfoil Usage.
//!   <https://m-selig.ae.illinois.edu/ads/aircraft.html>
//! - [R] D. F. Rogers, "Flight Tests: Atlantic Aero A-36" (2004), quoting the
//!   A36 POH (rev. 9/1996, IO-550-B, McCauley 80 in three-blade).
//!   <https://nar-associates.com/technical-flying/AtlanticAero/AtlanticA-36_wide_screen.pdf>
//! - [W] Wikipedia, Beechcraft Bonanza (A36 history: 1970, stretched 10 in
//!   over the 33, four windows a side, right rear double doors; IO-550 from 1984).
//!   <https://en.wikipedia.org/wiki/Beechcraft_Bonanza>
//!
//! # The figures used (units as published; metres in the code)
//!
//! | item | value | source |
//! |---|---|---|
//! | length | 27 ft 5.6 in (8.37 m) [S]; 27 ft 6 in [T] | |
//! | span | 33 ft 5.5 in (10.20 m) [S]; 33 ft 6 in [T] | |
//! | height | 8 ft 5 in (2.57 m) static [S]; 8 ft 7 in [T] | |
//! | wing area, aspect ratio | 181 sq ft (16.82 m²), 6.2 | [T], [W] |
//! | airfoil | NACA 23016.5 root, NACA 23012 tip | [A] |
//! | horizontal tail span | 12 ft 2.1 in (3.71 m) | [S] |
//! | wheel track | 9 ft 6.7 in (2.91 m) | [S] |
//! | tyres | main 7.00-6 6-ply (33-40 psi), nose 5.00-5 4-ply (40 psi) | [S], [T] |
//! | strut extension | main 3 in, nose 3.5 in | [S] |
//! | propeller | McCauley D3A32C409/82NDB-2, 3 blades, 78.5-80 in (we use 80 in, 2.03 m) | [T], [R] |
//! | prop ground clearance | 9.5 in min, 80 in prop, tyre and strut flat | [S] |
//! | cabin top above ground | 6 ft 6.4 in | [S] three-view |
//! | static ground line | 3.56° to the waterline (sits nose-up) | [S] three-view |
//! | engine | Continental IO-550-B, 300 hp at 2700 rpm | [T] |
//! | max weight | 3650 lb (1656 kg) take-off and landing, 3663 lb ramp | [T] |
//! | CG limits (E-2111 on) | +74.0 to +87.7 in at 3100 lb, +81.0 to +87.7 at 3650 lb | A36 POH envelope as quoted by secondary sources; check against a POH |
//! | datum | 83.1 in ahead of the jack pads on the front spars | [T] |
//! | fuel | 80 US gal, 74 usable, arm +75 in | [S] (optional system, standard from 1984), [T] |
//! | speeds (KCAS) | VNE 203, VNO 165, VA 139, VFE 152 (12°) / 122 (30°), VLE 152 | [T] (G36; the A36 POH quotes the same airframe limits in KIAS) |
//! | flaps | 0, 12° approach, 30° full (older A36: 15°/30°) | [T], [S] |
//! | control travel | aileron 20° up/down, elevator 23° up / 20° down, rudder 25° each way, elevator tab 10° up / 27° down | [S] |
//! | stalls | 59 kt gear and flaps down at 3650 lb; about 70 clean | [P] (59); clean from the older A36's ratio [O]: estimate |
//! | cruise | 157 KTAS at 195 BHP (65%), 169 at 225 BHP (75%), 6000 ft, 3400 lb | [R]'s fit to the POH curve |
//! | take-off, 3650 lb | 971 ft ground roll, 1913 ft to 50 ft (G36, 3805 lb: 1300 ft roll) | [P], [G] |
//! | climb, sea level | 1210 fpm at 3650 lb (G36, 3805 lb: 1120 fpm); Vy about 96 KIAS | [P], [G]; Vy secondary |
//! | top speed | G36: 174 KTAS "max cruise" | [G]; no A36 sea-level figure to hand |
//! | fuel flow | about 16 gal/h at 75 % (older 285 hp A36: 15.2 at 75 %, 13.3 at 65 %) | [P], [O] |
//! | parasite drag | f ≈ 3.5 sq ft, CD0 ≈ 0.019 | [R] (an estimate in the article) |
//!
//! [P] Plane & Pilot, "Beechcraft A36 Bonanza N715LT" (2004), its table of
//! the standard A36 from Jane's / the Aircraft Bluebook
//! <https://www.planeandpilotmag.com/article/beechcraft-a36-bonanza-n715lt/>.
//! [G] Textron Aviation, Bonanza G36 product card (2023)
//! <https://beechcraft.txtav.com/-/media/beechcraft/files/product-card/bonanza_product_card.pdf>.
//! [O] AOPA fact sheet (1977 A36, IO-520) and Plane & Pilot's 1979 A36
//! <https://www.aopa.org/go-fly/aircraft-and-ownership/aircraft-fact-sheets/beechcraft-bonanza>.
//! The POH's own tables were not to hand: these are secondary figures, the
//! cruise the best of them (read off the POH by [R]).
//!
//! # Flight model: the book against the simulator
//!
//! Measured by `performance_is_a_bonanza`, `a_bonanza_takes_off_and_climbs`
//! and `a_bonanza_gear_and_flaps_cycle_in_flight` in plane.rs (standard
//! day, no wind, mixture full rich unless said):
//!
//! | | book | simulator |
//! |---|---|---|
//! | stall, 3650 lb, clean / 12° / 30° + gear | ~70 / - / 59 KCAS | 69 / 64 / 59 KCAS |
//! | take-off, 3650 lb, flaps 12°, rotate 70 KIAS | 971 ft (1300 at 3805 lb) roll, 1913 ft to 50 ft | 1190 ft roll (lift-off 75 KIAS), 2165 ft to 50 ft at 85 KIAS |
//! | climb, sea level, 3650 lb, full power | 1210 fpm (1120 at 3805 lb) | 1140 fpm at 96 KIAS, 1180 at 105, 1070 at 85 |
//! | 65 %, 2400 rpm, 6000 ft, 3400 lb | 157 KTAS | 157 KTAS (144 KCAS), 14.2 gal/h leaned, 16.9 rich |
//! | 75 %, 2500 rpm, 6000 ft, 3400 lb | 169 KTAS | 167 KTAS (153 KCAS), 16.0 gal/h leaned, 19.2 rich |
//! | 75 %, 2700 rpm, 8000 ft (full throttle) | - | 169 KTAS (150 KCAS) |
//! | full throttle, 1000 ft, 3400 lb | (G36: 174 KTAS max cruise) | 176 KTAS / 174 KCAS, 96 % power, 24 gal/h |
//! | the same, gear down | - | 154 KTAS |
//! | idle glide, 110 KIAS, 3650 lb | about 10:1 | 9.3:1 |
//! | static, full throttle | about 2700 rpm | 2654 rpm, 4490 N; idle 790 rpm |
//! | gear / flaps travel | about 6 s / 10 s | 6.0 s / 10.0 s, both stop without bus power |
//!
//! Not modelled or not matched: no gear speed limit (VLE 152 KCAS); no cowl
//! flaps; the windmilling glide drags more than the book's with the lever
//! back (see the engine below); the take-off runs about 20 % long against
//! Jane's (but inside the G36's figure) and lifts off a few knots fast; the
//! POH's tables (take-off, climb, stall, fuel) were not to hand, so those
//! targets are secondary. Inertias are estimates.
//!
//! # How the drawing became the model
//!
//! The shape comes from tracing the shop manual's three-view at 300 dpi
//! (`s` below: inches aft of the spinner tip, the drawing's waterline 100 is
//! the thrust line). Lengths were scaled to the published length and span;
//! the drawing's side view is about 11 % taller than the published heights
//! (8 ft 5 in to the fin, 6 ft 6.4 in to the cabin top), so its heights are
//! scaled by 0.89. Measured from it: root chord 87.4 in and tip chord 46.7 in
//! at the outer rib (190.5 in out; the rounded tip reaches 201 in), the
//! leading edge swept back 10.5 in over the panel and the trailing edge
//! swept forward (that gives 185 sq ft against the published 181: within
//! the tracing's error), flaps out to 112 in, ailerons from there to the
//! outer rib (both about 20 % chord), wheelbase 96.6 in, main axles 120 in
//! aft of the spinner tip, fuselage 47.4 in wide outside, stabiliser root
//! chord 46 in and tip chord 33 in, rudder hinge swept 20°, fin leading
//! edge 36°. Four windows a side (the right front one in the cabin door,
//! the right third and fourth in the cargo doors), double cargo doors
//! 48 in wide from s 146 to 194.5 split at s 170.
//!
//! Estimates (no source to hand): the datum sits 22 in aft of the spinner
//! tip (it puts the front spar at 23 % of the root chord, the fuel arm in
//! the leading-edge cells and the CG envelope at 20-30 % MAC, all as they
//! should be); dihedral 6° (the three-view's front view reads 5 ± 1°);
//! incidence 4° at the root washing out to 1° at the tip (the side view
//! reads 3.4° at the root); spinner 13 in across; the gear's hinge points,
//! door shapes and the nose doors standing open with the gear down; panel
//! seam and rivet positions (typical of the type, not from drawings); the
//! landing light in the nose bowl over the induction air inlet ([R]
//! describes that layout) and a taxi light on the nose leg; inertias.
//!
//! # Frames
//!
//! Body frame as everywhere: +X right, +Y up, -Z forward, metres, the origin
//! at the reference CG: datum +82 in (21 % MAC), on station s 104, 0.05 m
//! below the thrust line. `st(s)` turns a station into body z and `wl(h)` a
//! height above the thrust line (inches, true scale) into body y.
//!
//! # For the agents that come next
//!
//! - Cockpit: `cabin.rs` (`cabin::build`, `PANEL_Z`, `FLOOR_Y`, `EYE`,
//!   `sees_inside`) is the 1990s steam-gauge A36 cockpit, every primary
//!   control clickable (its notes list what is decorative).
//!   The windows are cut by `window()` here (and `window_bonanza` in
//!   plane.wgsl); the glass is `glass()` here. The wing's carry-through sits
//!   under the front seats (its skin starts 0.25 m out, inside the cabin).
//! - Liveries: `LIVERIES` (four fictional striped schemes and plain white),
//!   drawn by `livery_bonanza` in plane.wgsl, gated with `livery_style()`;
//!   pictures from assets/aircraft/liveries/stills.sh.
//! - Flight: tuned against the figures above (see "the book against the
//!   simulator"). The constant-speed propeller is `Piston::governor`, the
//!   electric gear and flaps `Spec::electric_actuators` (shared with the
//!   Cessna). The propeller lever's hover label in main.rs (1800 + 900 x
//!   lever rpm) matches the governor's range.
pub mod cabin;

use crate::aircraft::{Engine, Governor, Handling, Hole, Hull, Leg, Livery, Part, Piston, Sound, Spec, Tail};
use crate::airframe::*;
use glam::{DVec3, Mat4, Quat, Vec2, Vec3};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

/// Paint schemes for the aircraft picker, drawn by `livery_bonanza` in
/// plane.wgsl: a broad stripe under the window belt from the nose bowl,
/// sweeping up the tail cone and across the fin and rudder, a pinstripe over
/// it, coloured wing tips, stabiliser tips and spinner, over white.
///
/// All of them are fictional, period-inspired schemes: the layout follows
/// the white-with-sweeping-stripe look of Beech's factory paint on 1970s-80s
/// Bonanzas as seen in photographs, but is not traced from any factory
/// drawing, and the colours are not matched to Beech paint codes. The
/// registration (N741TZ) is made up. No operator's branding.
/// References (photographs and descriptions of factory-era paint):
/// <https://en.wikipedia.org/wiki/Beechcraft_Bonanza>,
/// <https://flightaware.com/photos/view/171364-d1461a5d4f79c9f18aaca082839840cc96465770>
/// (a Bonanza 36 in factory paint),
/// <https://www.controller.com/listing/for-sale/257859817/1984-beechcraft-a36-bonanza-piston-single-aircraft>
/// (1984 A36, white with accent striping).
///
/// colours: [0] main stripe (w: style 1), [1] pinstripe, [2] tips, linear RGB.
pub const LIVERIES: &[Livery] = &[
    striped("Classic tan", [0.13, 0.050, 0.018], [0.52, 0.33, 0.12], [0.13, 0.050, 0.018], include_bytes!("../../../assets/aircraft/liveries/bonanza_tan.png")),
    striped("Blue", [0.010, 0.030, 0.13], [0.10, 0.28, 0.62], [0.010, 0.030, 0.13], include_bytes!("../../../assets/aircraft/liveries/bonanza_blue.png")),
    striped("Maroon & gold", [0.19, 0.012, 0.022], [0.60, 0.40, 0.09], [0.19, 0.012, 0.022], include_bytes!("../../../assets/aircraft/liveries/bonanza_maroon.png")),
    striped("Green", [0.018, 0.11, 0.045], [0.62, 0.48, 0.12], [0.018, 0.11, 0.045], include_bytes!("../../../assets/aircraft/liveries/bonanza_green.png")),
    Livery { name: "None", colours: [[0.0; 4]; 4], title: "", paint: None, swatch: [[0.80, 0.80, 0.78], [0.60, 0.60, 0.60]], still: include_bytes!("../../../assets/aircraft/liveries/bonanza_white.png") },
];
const fn striped(name: &'static str, m: [f32; 3], p: [f32; 3], t: [f32; 3], still: &'static [u8]) -> Livery {
    Livery { name, colours: [[m[0], m[1], m[2], 1.0], [p[0], p[1], p[2], 0.0], [t[0], t[1], t[2], 0.0], [0.0; 4]], title: "", paint: None, swatch: [m, p], still }
}

pub fn build() -> Model {
    let mut b = Builder::default();
    fuselage(&mut b);
    wings(&mut b);
    tail(&mut b);
    engine(&mut b);
    gear(&mut b);
    glass(&mut b);
    cabin::build(&mut b);
    bake_ao(&mut b.pieces, &AoBake { vox: 0.025, reach: 2.6, rays: 28, open: &|q, pat| (cut_by(pat) && window(q) < 0.0) || (well_cut_by(pat) && well(q) < 0.0) });
    // The strut doors are baked where they hang with the gear down, half
    // inside the wing: give them the open skin's light instead, so they don't
    // show a dark band when shut flush with the gear up.
    for pc in b.pieces.iter_mut().filter(|pc| pc.name.ends_with("main leg")) {
        for v in &mut pc.mesh.v {
            if v.mat[0] == mat::LIVERY {
                v.col[3] = 0.92;
            } else if v.mat[0] == PRIMER.id && Vec3::new(v.col[0], v.col[1], v.col[2]).distance(PRIMER.col) < 1e-3 {
                v.col[3] = 0.5;
            }
        }
    }
    Model::finish(b.pieces)
}

const IN: f32 = 0.0254;
/// Body z of station `s` (inches aft of the spinner tip).
pub const fn st(s: f32) -> f32 {
    (s - 104.0) * IN
}
/// The thrust line's height (body y).
pub const THRUST_Y: f32 = 0.05;
/// Body y of a height `h` inches above the thrust line.
pub const fn wl(h: f32) -> f32 {
    THRUST_Y + h * IN
}

const KT: f64 = 1.0 / 1.94384;
const V_NONE: f64 = 1e9;
const LB: f64 = 0.453_592;

const fn leg(at: DVec3, r: f32, travel: f64, k: f64, steer: f64, fold_vn: f64, fold_kick: DVec3, name: &'static str) -> Leg {
    Leg { at, wheel_r: r as f64, travel, k, c: k * 0.1, nf_max: 150_000.0, steer, brakes: at.x != 0.0, side_k: 6000.0, side_mu: 0.9, roll_k: 90.0, roll_mu: 0.025, brake_k: 3000.0, brake_mu: 0.55, fold_vn, fold_kick, name }
}

pub static SPEC: Spec = Spec {
    id: 10,
    name: "Beechcraft A36 Bonanza",
    keys: &["bonanza", "a36", "be36"],
    blurb: "Six-seat single, 300 hp, retractable gear, double cargo doors. Solid, quick and well harmonised: 170 kt cruise.",
    origin: None,
    ext: None,
    still: include_bytes!("../../../assets/aircraft/bonanza.png"),
    paint: None,
    build,
    // About 2500 lb empty (1134 kg).
    groups: [("fuselage", 330.0), ("tail", 60.0), ("left wing", 150.0), ("right wing", 150.0), ("engine", 245.0), ("propeller", 34.0), ("gear", 75.0)],
    span: SPAN,
    wing_sect,
    fuse: |z| {
        let s = sec(z);
        (s.top, s.mid, s.bot)
    },
    aileron: AILERON,
    tank: TANK,
    tail: Tail {
        fin_y0: FIN_ROOT_Y,
        fin_h: FIN_TIP_Y - FIN_ROOT_Y,
        rudder_y0: 0.43,
        rudder_h: 1.27,
        stab_root: 0.12,
        elev_root: 0.09,
        break_z: [2.8, 4.2],
        fin_base: DVec3::new(0.0, FIN_ROOT_Y as f64, 4.6),
        fin_tip: DVec3::new(0.0, FIN_TIP_Y as f64, 5.2),
        stab_tip: DVec3::new(STAB_HALF as f64, STAB_Y as f64, 4.9),
    },
    eye: cabin::EYE,
    eye2: None,
    sees_inside: cabin::sees_inside,
    hotspots: &[],
    contacts: &[
        ([0.0, 0.05, -2.64], Part::Nose),
        ([0.0, -0.55, -2.0], Part::Nose),
        ([0.0, 0.27, -1.9], Part::Nose),
        ([-0.45, -0.15, -1.9], Part::Nose),
        ([0.45, -0.15, -1.9], Part::Nose),
        ([0.0, -0.61, -0.8], Part::Belly),
        ([0.0, -0.61, 0.3], Part::Belly),
        ([0.0, -0.50, 1.3], Part::Belly),
        ([-0.60, 0.05, -0.6], Part::Side),
        ([0.60, 0.05, -0.6], Part::Side),
        ([-0.60, 0.05, 0.8], Part::Side),
        ([0.60, 0.05, 0.8], Part::Side),
        ([0.0, 0.79, 0.2], Part::Roof),
        ([0.0, 0.62, 2.4], Part::Roof),
    ],
    tail_pts: &[[0.0, -0.32, 2.8], [0.0, -0.13, 3.9], [0.0, 0.24, 5.5], [0.0, 0.47, 4.2]],
    wing_st: [0.7 / SPAN, 1.6 / SPAN, 2.6 / SPAN, 3.6 / SPAN, 4.6 / SPAN],
    hard: [
        DVec3::new(0.0, 0.05, -2.642),
        DVec3::new(-5.10, 0.12, 0.25),
        DVec3::new(5.10, 0.12, 0.25),
        DVec3::new(0.0, 0.265, 5.50),
        DVec3::new(0.0, FIN_TIP_Y as f64, 5.30),
        DVec3::new(0.0, -0.61, 0.0),
        DVec3::new(0.0, (THRUST_Y - PROP_R) as f64, PROP_HUB.z as f64),
    ],
    gear: &[
        leg(DVec3::new(0.0, NOSE_GROUND as f64, NOSE_AXLE.z as f64), NOSE_R, 0.089, 60_000.0, 1.0, 4.5, DVec3::new(-0.6, 0.0, 0.0), "NOSE GEAR"),
        leg(DVec3::new(-MAIN_AXLE.x as f64, MAIN_GROUND as f64, MAIN_AXLE.z as f64), MAIN_R, 0.076, 95_000.0, 0.0, 6.5, DVec3::new(0.4, 0.0, 0.5), "LEFT MAIN GEAR"),
        leg(DVec3::new(MAIN_AXLE.x as f64, MAIN_GROUND as f64, MAIN_AXLE.z as f64), MAIN_R, 0.076, 95_000.0, 0.0, 6.5, DVec3::new(0.4, 0.0, -0.5), "RIGHT MAIN GEAR"),
    ],
    hull: Hull { k: 70_000.0, c: 10_000.0, nf_max: 140_000.0, drag: 3600.0 },
    hole: Hole { fus: [0.60, 0.70, 0.09, 0.45], wing: [-0.37, 0.105, 0.18, 0.07], stab: [STAB_Y, 0.05, STAB_HALF], fin: [FIN_ROOT_Y, FIN_TIP_Y - FIN_ROOT_Y, 0.07, 0.04], pods: [[0.0; 3]; 2] },
    // About 2500 lb empty (Jane's standard empty is 2247 lb, the G36's card
    // 2605 lb; a typically equipped A36 lies between), two aboard (170 kg)
    // and half the fuel (100 kg): 3095 lb. The book's figures below are at
    // 3400 or 3650 lb, so the tests in plane.rs load it to those.
    mass: 2500.0 * LB + 170.0 + 100.0,
    // 74 US gal usable of avgas (2.72 kg/gal). The flow grows with the power
    // and with the mixture (plane.rs: x0.7 at the lean end, x1 full rich):
    // 25 gal/h at full rich take-off power, 2 at idle. Leaned (mixture 0.45)
    // that gives 16 gal/h at 75 % and 14 at 65 %, against about 16 at 75 %
    // in Plane & Pilot's A36 table and 15.2 / 13.3 for the older 285 hp A36
    // (AOPA). Estimates: the POH's power tables were not to hand.
    fuel_kg: 74.0 * 2.72,
    fuel_flow: [2.0 * 2.72 / 3600.0, 25.0 * 2.72 / 3600.0, 30.0 * 2.72 / 3600.0],
    // Two up front included; the club seats face aft at about +118 in and the
    // back pair sit at about +157 in, the baggage behind them at +180: a
    // metre or so aft of the reference CG (+82).
    payload: [170.0, 520.0, 1.0],
    // MAC 69.1 in (from the planform above), its leading edge at +67.2 in;
    // the reference CG +82 in is 21 % of it, the limits +74.0 (10 %, at
    // 3100 lb) and +87.7 in (30 %). The fuel at +75 in sits 7 in ahead.
    mac: 1.755,
    cg: [0.214, 0.10, 0.297],
    fuel_arm: -0.18,
    // (pitch, yaw, roll) kg m²: estimates, the Cessna's (from its JSBSim
    // model) scaled by mass and length or span squared: x1.5 for the mass,
    // about the same length, a little less span, fuel in the wing roots.
    inertia: DVec3::new(2700.0, 3600.0, 1900.0),
    wing_area: 16.82,
    b: 10.20,
    // Aspect ratio 6.2, Oswald about 0.75 (k = 1/(pi e A)); lift slope by
    // Helmbold for A 6.2; the wing at 2.5° mean incidence on a 230-series
    // section (zero lift at about -1.2°). CD0 0.022 comes out of [R]'s
    // power-required curve from the POH (6000 ft, 3400 lb, standard day:
    // BHP = 0.0103 V² - 0.8804 V + 78.679, V in KTAS: 157 kt at 65 %, 169
    // at 75 %) with the propeller below working at 84-85 % in the cruise.
    // [R]'s own guess is 0.019 (f = 3.5 sq ft); the simulator lumps cooling,
    // trim and interference drag into CD0 and has no drag rising with the
    // lift other than k, so its figure sits a little higher.
    // Stalls: about 59 KCAS with the gear and full flaps down at 3650 lb
    // (Plane & Pilot's A36 table, from Jane's; the FAR 23 limit is 61) and
    // about 70 clean (the older A36's 62-64 against 52-53 dirty, AOPA, keeps
    // the same ratio): CLmax 1.25 clean (12.1°), 1.71 with 30° of flap.
    // Gear down adds 0.012 (full power falls from about 176 to 154 KTAS).
    cl0: 0.28,
    cl_a: 4.6,
    cd0: 0.022,
    k_ind: 0.0685,
    cd_gear: 0.012,
    cd_flap: [0.018, 0.03],
    flap_cl: 0.55,
    stall_alpha: 0.211,
    stall_flap: 0.02,
    stall_slat: 0.0,
    alpha_trim: 0.19,
    mcrit: 0.0,
    cd_wave: 0.0,
    spoiler_cd: 0.0,
    spoiler_cl: 0.0,
    chute: None,
    flaps: &[0.0, 12.0, 30.0],
    vfe: &[V_NONE, 152.0 * KT, 122.0 * KT],
    // The electric flaps take about ten seconds from up to full.
    flap_rate: 1.0 / 10.0,
    slats_at: 0,
    lever: None,
    // The electric gear: about six seconds.
    gear_rate: 1.0 / 6.0,
    electric_actuators: true,
    // 203 KCAS; flutter is left a margin past it. The simulator has no gear
    // speed limit: VLE/VLO 152 KCAS ([T]) is the pilot's to keep. VFE as
    // above, 152 KCAS approach (12°), 122 full (30°), is enforced.
    vne: 203.0 * KT,
    mmo: 0.0,
    flutter_v: 225.0 * KT,
    inlet_mach: 0.0,
    dry_mach: 0.0,
    // Utility category at 3650 lb: +4.4 g.
    g_limit: 4.6,
    g_break: 6.6,
    fbw_g: 0.0,
    steer_max: 0.30,
    steer_fade: 0.0,
    // 300 hp at 2700 rpm on the 80 in three-blade McCauley, constant speed
    // (`Governor`): the lever selects 1800 to 2700 rpm (the low end an
    // estimate; the POH's power tables run 2100-2700). The fine stop is set
    // for about 2650 rpm and 4500 N (1010 lbf) static at full throttle, so
    // it governs at 2700 from the first few knots of the take-off run. The
    // blade curves are fitted, not from McCauley data: efficiency about 0.78
    // at Vy, 0.84-0.85 in the cruise and at full speed; idling at 110 KIAS
    // the windmilling blades drag about 390 N, a 9:1 glide against the
    // POH's 10:1 or so (the windmill stays under the governor's lowest revs,
    // so pulling the lever back does not help here as it does in the
    // aeroplane). Throttle slammed open in the cruise: under 80 rpm over.
    engine: Engine::Piston(Piston {
        power: 223_700.0,
        w_rated: 2700.0 * std::f64::consts::TAU / 60.0,
        prop_d: 2.032,
        ct0: 0.110,
        cp0: 0.060,
        j_t: 1.0,
        j_p: 1.3,
        prop_i: 4.2,
        static_thrust: 4500.0,
        fric0: 20.0,
        fric1: 48.0,
        blade_break_y: PROP_HUB.y + 0.45,
        governor: Some(Governor { rpm: [1800.0, 2700.0], at_full: true, coarse_max: 1.5, ct: 0.0, cp: 1.3, j: 1.0, rate: 0.1 }),
    }),
    // The Cessna's as a start: firmer in pitch (a heavier airframe further
    // aft), crisp ailerons, good directional stability with the big fin.
    handling: Handling {
        q_ref: 2600.0,
        a_max: 2.6,
        damp_v: 75.0,
        ad_min: 0.3,
        ad_max: 2.6,
        pitch_k: 7.5,
        pitch_k_lost: 2.0,
        pitch_ctl: 0.58,
        alpha_ctl: 0.25,
        pitch_d: 3.4,
        roll_ctl: 4.6,
        roll_d: 4.0,
        roll_beta: 0.8,
        roll_clean: 1.0,
        yaw_ctl: 1.3,
        yaw_beta: 4.8,
        yaw_d: 2.5,
        dihedral: 0.35,
        adverse: 0.3,
        stall_pitch: 0.9,
        stall_drop: 1.6,
        stall_yaw: 2.4,
    },
    // About 165 kt in the cruise; an 80 kt final with full flaps.
    cruise: 85.0,
    approach: (41.0, 2),
    cam_dist: 17.0,
    shadow_r: 6.6,
    sound: Sound::Piston,
};

// ---------------------------------------------------------------- fuselage

/// Fuselage cross-section keys: station z, half width, top, waist height,
/// bottom, squareness of the upper and the lower half (traced from [S],
/// station in inches aft of the spinner tip on each row).
const FUSE: [[f32; 7]; 38] = [
    [-2.311, 0.270, 0.215, -0.060, -0.310, 2.2, 2.2], // s 13: nose bowl lip
    [-2.286, 0.315, 0.222, -0.086, -0.346, 2.2, 2.2], // s 14
    [-2.261, 0.338, 0.225, -0.097, -0.413, 2.3, 2.3], // s 15
    [-2.210, 0.368, 0.229, -0.108, -0.470, 2.4, 2.4], // s 17
    [-2.134, 0.399, 0.236, -0.120, -0.522, 2.5, 2.5], // s 20
    [-2.032, 0.427, 0.247, -0.131, -0.554, 2.6, 2.6], // s 24
    [-1.880, 0.455, 0.270, -0.131, -0.578, 2.7, 2.7], // s 30
    [-1.702, 0.490, 0.295, -0.131, -0.594, 2.8, 2.8], // s 37
    [-1.499, 0.523, 0.319, -0.120, -0.606, 2.9, 2.9], // s 45
    [-1.295, 0.549, 0.339, -0.108, -0.615, 3.0, 3.0], // s 53
    [-1.118, 0.564, 0.353, -0.086, -0.619, 3.1, 3.0], // s 60: firewall, windshield base
    [-0.965, 0.574, 0.425, -0.063, -0.619, 3.2, 3.0], // s 66
    [-0.813, 0.582, 0.509, -0.040, -0.619, 3.3, 3.0], // s 72
    [-0.660, 0.589, 0.595, -0.018, -0.619, 3.4, 3.0], // s 78
    [-0.508, 0.593, 0.674, 0.005, -0.619, 3.5, 3.0], // s 84
    [-0.356, 0.596, 0.735, 0.027, -0.617, 3.6, 3.0], // s 90
    [-0.178, 0.599, 0.773, 0.050, -0.615, 3.6, 3.0], // s 97
    [0.051, 0.602, 0.789, 0.050, -0.610, 3.6, 3.0], // s 106
    [0.305, 0.602, 0.794, 0.050, -0.603, 3.6, 3.0], // s 116
    [0.660, 0.599, 0.778, 0.050, -0.581, 3.6, 3.0], // s 130
    [0.965, 0.597, 0.755, 0.050, -0.545, 3.6, 3.0], // s 142
    [1.219, 0.592, 0.737, 0.061, -0.506, 3.5, 2.9], // s 152
    [1.524, 0.564, 0.710, 0.073, -0.477, 3.3, 2.8], // s 164
    [1.829, 0.530, 0.678, 0.084, -0.445, 3.1, 2.7], // s 176
    [2.134, 0.493, 0.649, 0.095, -0.407, 2.9, 2.6], // s 188
    [2.438, 0.455, 0.620, 0.107, -0.366, 2.7, 2.5], // s 200
    [2.743, 0.417, 0.593, 0.118, -0.321, 2.6, 2.5], // s 212
    [3.048, 0.382, 0.565, 0.129, -0.271, 2.5, 2.4], // s 224
    [3.353, 0.338, 0.538, 0.145, -0.224, 2.4, 2.4], // s 236
    [3.658, 0.288, 0.516, 0.163, -0.176, 2.4, 2.3], // s 248
    [3.962, 0.244, 0.489, 0.181, -0.129, 2.3, 2.3], // s 260
    [4.267, 0.196, 0.461, 0.199, -0.081, 2.3, 2.3], // s 272
    [4.572, 0.150, 0.434, 0.217, -0.031, 2.2, 2.2], // s 284
    [4.877, 0.114, 0.412, 0.235, 0.014, 2.2, 2.2], // s 296
    [5.131, 0.086, 0.394, 0.249, 0.064, 2.1, 2.1], // s 306
    [5.309, 0.066, 0.376, 0.258, 0.118, 2.0, 2.0], // s 313
    [5.436, 0.043, 0.348, 0.262, 0.172, 2.0, 2.0], // s 318
    [5.499, 0.015, 0.312, 0.265, 0.217, 2.0, 2.0], // s 320.5: tail cone end
];
/// The nose bowl's flat face, where the cowling's skin starts (s 14): the
/// rounded lip ahead of it (the first key, s 13) is left off so the face is wide enough for
/// the cooling inlets either side of the spinner, whose back sits flush with it.
pub const NOSE_FACE_Z: f32 = st(14.0);
pub const TAIL_Z: f32 = 5.499;
/// The firewall and the cowling's aft edge (s 60).
pub const FIREWALL_Z: f32 = -1.118;
/// Where the tail cone joins the cabin section, behind the baggage bulkhead (s 214).
pub const TAILCONE_Z: f32 = 2.794;
/// The spinner's tip.
pub const SPINNER_TIP_Z: f32 = st(0.0);

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
/// Height of the lower skin at station z, lateral position x.
pub fn fuse_bottom_y(z: f32, x: f32) -> f32 {
    let s = sec(z);
    let x = x.abs().min(s.w * 0.999);
    let (mut a, mut b) = (FRAC_PI_2, PI);
    for _ in 0..30 {
        let m = (a + b) * 0.5;
        if sec_pt(&s, m).x > x { a = m } else { b = m }
    }
    sec_pt(&s, (a + b) * 0.5).y
}
/// Half width of the fuselage at station z, height y (0 above or below it).
pub fn fuse_half(z: f32, y: f32) -> f32 {
    let s = sec(z);
    if y > s.top || y < s.bot {
        return 0.0;
    }
    let (h, n) = if y > s.mid { (s.top - s.mid, s.nt) } else { (s.mid - s.bot, s.nb) };
    s.w * (1.0 - ((y - s.mid).abs() / h).powf(n)).max(0.0).powf(1.0 / n)
}

/// Signed distance to the nearest window opening (negative inside), body
/// frame: the one-piece windshield and four windows a side (the same on both
/// sides; on the right the first is in the cabin door, the third and fourth
/// in the cargo doors). Keep in step with `window_bonanza` in plane.wgsl.
pub fn window(p: Vec3) -> f32 {
    let zy = Vec2::new(p.z, p.y);
    let ws = [Vec2::new(-1.054, 0.36), Vec2::new(-0.762, 0.375), Vec2::new(-0.292, 0.731), Vec2::new(-0.241, 0.82), Vec2::new(-1.092, 0.82)];
    let mut d = sd_poly(zy, &ws) + 0.01;
    if p.x.abs() > 0.33 && p.y < 0.75 {
        // The first window's forward edge runs parallel to the windshield's
        // aft edge, 10 cm behind it (glass 7.5 cm apart: a slim windshield
        // post, as on the A36), its lower corner cut back to stay aft of the
        // instrument panel (cabin.rs).
        let w1 = [Vec2::new(-0.56, 0.265), Vec2::new(0.31, 0.265), Vec2::new(0.31, 0.665), Vec2::new(-0.214, 0.665), Vec2::new(-0.616, 0.36)];
        d = d.min(sd_poly(zy, &w1) - WIN_R);
        for w in WINDOWS {
            d = d.min(sd_rect(zy, Vec2::new(w[0], w[1]), Vec2::new(w[2], w[3])) - WIN_R);
        }
    }
    d
}
/// The second to fourth windows, each side: z0, y0, z1, y1 of the rounded
/// rectangle's straight edges (the corners round off by `WIN_R` beyond them).
pub const WINDOWS: [[f32; 4]; 3] = [[0.446, 0.280, 0.956, 0.650], [1.196, 0.280, 1.565, 0.574], [1.790, 0.296, 2.162, 0.513]];
pub const WIN_R: f32 = 0.035;
/// Signed distance to a rectangle from `lo` to `hi`.
pub fn sd_rect(p: Vec2, lo: Vec2, hi: Vec2) -> f32 {
    let c = (lo + hi) * 0.5;
    let h = (hi - lo) * 0.5;
    let q = (p - c).abs() - h;
    q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
}

/// The cabin door on the right (z, y outline), hinged on its forward edge.
pub const DOOR: [[f32; 2]; 5] = [[-0.69, -0.153], [0.368, -0.153], [0.368, 0.77], [-0.28, 0.77], [-0.76, 0.39]];
/// The cargo doors on the right: z from, split, z to; y from, y to.
pub const CARGO: [f32; 5] = [1.074, 1.687, 2.299, -0.313, 0.654];

// Landing gear geometry (body frame; right side for the mains).
pub const NOSE_R: f32 = 0.180;
pub const MAIN_R: f32 = 0.222;
/// Where the tyres touch the ground, gear down and the struts fully out: the
/// nose wheel lower than the mains, so it sits nose-up (2.8°; [S] draws the
/// ground line at 3.56° with the struts loaded).
pub const NOSE_GROUND: f32 = -1.25;
pub const MAIN_GROUND: f32 = -1.13;
pub const NOSE_AXLE: Vec3 = Vec3::new(0.0, NOSE_GROUND + NOSE_R, -2.05);
pub const MAIN_AXLE: Vec3 = Vec3::new(1.457, MAIN_GROUND + MAIN_R, 0.404);
/// The nose leg's hinge, up in the well: it folds back through 90°.
pub const NOSE_PIVOT: Vec3 = Vec3::new(0.0, -0.42, -2.12);
/// The main leg's hinge inside the wing, above the lower skin: it folds
/// inboard through `MAIN_FOLD`, the leg lying along the lower skin and the
/// wheel flat in the wing root (`MAIN_WELL`), clear of the upper skin.
pub const MAIN_PIVOT: Vec3 = Vec3::new(1.59, -0.355, 0.404);
/// The main leg's fold: 83.5°, about 90° less the wing's 6° dihedral, so the
/// retracted leg lies along the lower skin, which rises outboard, and the
/// wheel sits low enough in the root to clear the upper skin.
pub const MAIN_FOLD: f32 = 1.4574;
/// The main strut's line, outboard of the wheel (a little inboard of the
/// hinge, so the retracted strut lies inside the lower skin).
pub const MAIN_STRUT_X: f32 = 1.578;
/// The main wheel well in the wing's lower skin, right side: a circle (x, z,
/// radius) for the wheel and the slot (x0, x1, z0, z1) the leg and its door
/// swing through. Keep in step with `well_bonanza` in plane.wgsl.
pub const MAIN_WELL: [f32; 3] = [1.037, 0.404, 0.25];
pub const MAIN_SLOT: [f32; 4] = [1.06, 1.67, 0.33, 0.48];
/// The nose well in the cowling's bottom: z0, z1, half width.
pub const NOSE_WELL: [f32; 3] = [-2.20, -1.25, 0.115];
pub const PROP_HUB: Vec3 = Vec3::new(0.0, THRUST_Y, -2.44);
pub const PROP_R: f32 = 1.016;
pub const SPINNER_R: f32 = 0.165;

/// Signed distance to the landing gear wells (5 mm inside their walls).
/// Keep in step with `well_bonanza` in plane.wgsl.
pub fn well(p: Vec3) -> f32 {
    let mut d = 1.0f32;
    if p.y < -0.40 && p.z < -1.0 {
        let [z0, z1, hw] = NOSE_WELL;
        d = d.min(sd_rect(Vec2::new(p.z, p.x), Vec2::new(z0, -hw), Vec2::new(z1, hw)) + 0.005);
    }
    let ax = p.x.abs();
    if p.y < -0.2 && p.y > -0.7 && ax > 0.6 && ax < 1.8 {
        let q = Vec2::new(ax, p.z);
        let c = (q - Vec2::new(MAIN_WELL[0], MAIN_WELL[1])).length() - MAIN_WELL[2];
        let s = sd_rect(q, Vec2::new(MAIN_SLOT[0], MAIN_SLOT[2]), Vec2::new(MAIN_SLOT[1], MAIN_SLOT[3]));
        d = d.min(c.min(s) + 0.005);
    }
    d
}

fn fuselage(b: &mut Builder) {
    // Stations: finer round the nose bowl, the windshield and the tail cap.
    let mut zs = Vec::new();
    let mut z = NOSE_FACE_Z;
    while z < TAIL_Z - 0.005 {
        zs.push(z);
        z += if z < -2.1 { 0.010 } else if z < -0.2 && z > -1.2 { 0.025 } else if z > 5.2 { 0.015 } else { 0.035 };
    }
    zs.push(TAIL_Z);
    for split in [FIREWALL_Z, TAILCONE_Z] {
        let k = zs.iter().position(|&z| z > split).unwrap();
        zs[k - 1] = split;
    }
    const NT: usize = 80; // round one side, top to keel
    let rows: Vec<Vec<SP>> = zs
        .iter()
        .map(|&z| {
            let s = sec(z);
            let mut arc = 0.0;
            let mut prev = sec_pt(&s, 0.0);
            (0..=NT)
                .map(|k| {
                    let th = k as f32 / NT as f32 * PI;
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
        // The left half is the mirror image, its surface coordinate negative.
        let mut l = m.mirror_x();
        for v in &mut l.v {
            v.uv[1] = -v.uv[1];
        }
        m.append(&l);
        m
    };
    let (rf, rt) = (row_of(FIREWALL_Z), row_of(TAILCONE_Z));
    let mut cowl = sub(0, rf, pat::COWL);
    nose_face(&mut cowl);
    louvers(&mut cowl);
    let mut cabin = sub(rf, rt, pat::FUSE);
    let mut tailcone = sub(rt, zs.len() - 1, pat::FUSE);
    let s = sec(TAIL_Z);
    let ring: Vec<Vec3> = (0..48).map(|k| { let p = sec_pt(&s, k as f32 / 48.0 * TAU); Vec3::new(p.x, p.y, TAIL_Z) }).collect();
    tailcone.fan(&WHITE.pat(pat::FUSE), Vec3::new(0.0, s.mid, TAIL_Z + 0.004), &ring, Vec3::Z);

    // The nose well: walls and a roof behind the opening in the cowl's bottom.
    let [z0, z1, hw] = NOSE_WELL;
    let roof = -0.24;
    let mut nw = Mesh::default();
    for (a, c) in [((z0, -hw), (z1, -hw)), ((z1, -hw), (z1, hw)), ((z1, hw), (z0, hw)), ((z0, hw), (z0, -hw))] {
        let rows: Vec<Vec<SP>> = (0..=16).map(|i| {
            let t = i as f32 / 16.0;
            let (z, x) = (a.0 + (c.0 - a.0) * t, a.1 + (c.1 - a.1) * t);
            let ys = fuse_bottom_y(z, x) - 0.004;
            vec![(Vec3::new(x, ys, z), [z, ys], 0.0), (Vec3::new(x, roof, z), [z, roof], 0.0)]
        }).collect();
        nw.grid(&PRIMER, &rows, false, false);
    }
    nw.quad(&PRIMER, [Vec3::new(-hw, roof, z0), Vec3::new(hw, roof, z0), Vec3::new(hw, roof, z1), Vec3::new(-hw, roof, z1)]);
    cowl.append(&nw);

    // Cowling: camloc fasteners along the bottom edge of each engine
    // compartment door, round the nose bowl joint and along the firewall;
    // the doors hinge on the top centre line (a piano hinge, drawn in the shader).
    for sx in [-1.0f32, 1.0] {
        let mut z = -1.99;
        while z < FIREWALL_Z - 0.04 {
            let (p, n) = on_side(z, COWL_SPLIT_Y - 0.015, sx, 0.0005);
            cowl.screw(&SCREW_STEEL, p, n, 0.0065, z * 13.0 + sx);
            z += 0.12;
        }
        for k in 0..8 {
            let y = 0.21 - k as f32 * 0.085;
            for zz in [NOSE_BOWL_Z + 0.018, FIREWALL_Z - 0.02] {
                let (p, n) = on_side(zz, y, sx, 0.0005);
                cowl.screw(&SCREW_PAINT, p, n, 0.0055, y * 7.0 + zz + sx);
            }
        }
    }
    // The oil filler door on top of the cowl, left of the hinge: two quarter-turn latches.
    for z in [-1.66f32, -1.44] {
        let x = -0.17;
        let p = Vec3::new(x, fuse_top(z, x) + 0.0005, z);
        cowl.screw(&SCREW_STEEL, p, Vec3::Y, 0.008, z * 3.0);
    }

    // The wing-root fillets: a concave fairing between the fuselage side and
    // the wing's upper skin from the leading edge to the trailing edge, and
    // the fillet's nose running forward of the leading edge.
    let f = wing_foil();
    for sx in [-1.0f32, 1.0] {
        let mut fl = Mesh::default();
        let (za, zb) = (wing_sect(0.6).le.z + 0.01, wing_sect(0.6).le.z + wing_sect(0.6).c - 0.02);
        let at = |a: f32, t: f32| -> SP {
            let z = za + (zb - za) * a;
            let r = 0.035 + 0.075 * (a * PI).sin().max(0.0).powf(0.6);
            let x0 = 0.56;
            let up = wing_top(&f, x0, z);
            let hw = fuse_half(z, up.y + r).max(0.2);
            let pa = Vec3::new(hw, up.y + r, z);
            let pb = wing_top(&f, hw + r, z);
            let c = Vec3::new(hw, pb.y, z);
            let p = pa * (1.0 - t) * (1.0 - t) + c * 2.0 * t * (1.0 - t) + pb * t * t;
            (Vec3::new(p.x * sx, p.y + 0.002, p.z), [z, t], 0.0)
        };
        fl.patch(&WHITE.pat(pat::FUSE), 40, 6, sx < 0.0, at);
        // Screws along its outer edge on the wing.
        for k in 1..18 {
            let (p, _, _) = at(k as f32 / 18.0, 0.97);
            fl.screw(&SCREW_PAINT, p + Vec3::Y * 0.001, Vec3::Y, 0.0035, p.z * 9.0 + sx);
        }
        // The fillet's nose: a half cone along the fuselage side, ahead of the leading edge.
        let le = wing_sect(0.6);
        let zn = le.le.z - 0.24;
        let yn = f.pt(0.6, 0.0, 0.0, -1.0).y;
        fl.patch(&WHITE.pat(pat::FUSE), 16, 10, sx > 0.0, |a, t| {
            let z = zn + (le.le.z + 0.03 - zn) * a;
            let r = 0.11 * (a.powf(0.7));
            let th = -FRAC_PI_2 + PI * t;
            let hw = fuse_half(z, yn).max(0.3);
            let p = Vec3::new(hw + r * 0.7 * th.cos(), yn + r * th.sin() * 0.95, z);
            (Vec3::new(p.x * sx, p.y, p.z), [z, t], 0.0)
        });
        cabin.append(&fl);
    }

    // Belly and tail cone inspection plates, the static ports.
    for &(z, r) in &[(-0.25f32, 0.07f32), (0.95, 0.06), (2.05, 0.06), (3.25, 0.055), (4.05, 0.05)] {
        let p = Vec3::new(0.0, sec(z).bot - 0.0012, z);
        inspection_plate(if z > TAILCONE_Z { &mut tailcone } else { &mut cabin }, p, Vec3::NEG_Y, Vec3::Z, r, 6);
    }
    for sx in [-1.0f32, 1.0] {
        let (p, n) = on_side(3.9, 0.20, sx, 0.0012);
        inspection_plate(&mut tailcone, p, n, Vec3::Y, 0.06, 6);
        let (p, n) = on_side(3.30, 0.13, sx, 0.0);
        tailcone.lathe(&ALU, p, n, &[[0.015, 0.0], [0.015, 0.0015], [0.004, 0.0015], [0.004, -0.001], [0.0, -0.001]], 16, false);
    }

    // The cabin door (right): its handle by the aft edge, two hinges on the
    // forward edge. The cargo doors: a flush handle at the split, hinges on
    // the forward door's front edge and the aft door's rear edge.
    let (p, n) = on_side(0.30, 0.18, 1.0, 0.004);
    let rot = Quat::from_rotation_arc(Vec3::X, n);
    cabin.rbox(&CHROME, p, Vec3::new(0.004, 0.012, 0.06), 0.004, rot);
    cabin.rbox(&BLACK, p - n * 0.003, Vec3::new(0.002, 0.022, 0.075), 0.002, rot);
    let (p, n) = on_side(0.24, 0.70, 1.0, 0.003);
    cabin.rbox(&CHROME, p, Vec3::new(0.003, 0.010, 0.03), 0.003, Quat::from_rotation_arc(Vec3::X, n));
    // (both on the forward edge's lower, straight part, below 0.39: a hinge
    // higher up stood out in the windshield's glass, off the door)
    for y in [0.0f32, 0.30] {
        let z = -0.69 - 0.129 * (y + 0.153) - 0.012;
        let (p, n) = on_side(z, y, 1.0, 0.005);
        cabin.rbox(&STEEL, p, Vec3::new(0.005, 0.03, 0.012), 0.004, Quat::from_rotation_arc(Vec3::X, n));
        cabin.bolt(&STEEL, p + n * 0.005, n, 0.003);
    }
    let (p, n) = on_side(CARGO[1] + 0.05, 0.16, 1.0, 0.003);
    cabin.rbox(&CHROME, p, Vec3::new(0.003, 0.014, 0.05), 0.003, Quat::from_rotation_arc(Vec3::X, n));
    for y in [-0.20f32, 0.15, 0.50] {
        for z in [CARGO[0] - 0.012, CARGO[2] + 0.012] {
            let (p, n) = on_side(z, y, 1.0, 0.004);
            cabin.rbox(&STEEL, p, Vec3::new(0.004, 0.028, 0.01), 0.003, Quat::from_rotation_arc(Vec3::X, n));
        }
    }
    // Screws round the door frames.
    for k in 0..DOOR.len() {
        let (a, c) = (DOOR[k], DOOR[(k + 1) % DOOR.len()]);
        let len = Vec2::new(c[0] - a[0], c[1] - a[1]).length();
        let n = (len / 0.09).round().max(1.0) as usize;
        for i in 0..n {
            let t = (i as f32 + 0.5) / n as f32;
            let (z, y) = (a[0] + (c[0] - a[0]) * t, a[1] + (c[1] - a[1]) * t);
            let mid = Vec2::new(-0.16, 0.31);
            let q = Vec2::new(z, y) + (Vec2::new(z, y) - mid).normalize() * 0.022;
            let (p, nn) = on_side(q.x, q.y, 1.0, 0.0004);
            if window(p) > 0.02 && fuse_half(q.x, q.y) > 0.3 {
                cabin.screw(&SCREW_PAINT, p, nn, 0.004, q.y * 31.0 + q.x);
            }
        }
    }

    // The boarding step behind the right wing's trailing edge, under the cargo doors.
    let (p, n) = on_side(1.80, -0.36, 1.0, 0.0);
    let foot = Vec3::new(0.66, -0.66, 1.82);
    cabin.lathe(&PAINT_WHITE, p, n, &[[0.03, -0.004], [0.03, 0.004], [0.022, 0.008], [0.0, 0.008]], 14, false);
    cabin.tube(&CHROME, &[p, p + n * 0.04 + Vec3::new(0.0, -0.05, 0.0), foot], &|_| 0.012, 10, true);
    cabin.rbox(&mt(mat::BLACK, 0.03, 0.03, 0.03).pat(pat::NONSLIP), foot + Vec3::new(0.0, -0.004, 0.0), Vec3::new(0.065, 0.008, 0.05), 0.004, Quat::IDENTITY);

    // Antennas: a com whip on the cabin roof, the GPS puck ahead of it, a com
    // blade on the tail cone, the ELT whip, the transponder blade and the
    // marker beacon's boat under the belly.
    let s = sec(1.05);
    let p = Vec3::new(0.0, s.top, 1.05);
    cabin.rbox(&BLACK, p + Vec3::Y * 0.006, Vec3::new(0.02, 0.006, 0.035), 0.004, Quat::IDENTITY);
    let tip = p + Vec3::new(0.0, 0.62, 0.36);
    cabin.tube(&BLACK, &[p + Vec3::Y * 0.01, tip], &|t| 0.006 - t * 0.0035, 6, true);
    cabin.dome(&BLACK, tip, tip - p, 0.004, 0.004, 6, 0.0);
    let s = sec(0.45);
    cabin.lathe(&mt(mat::PLASTIC, 0.62, 0.62, 0.60), Vec3::new(0.0, s.top, 0.45), Vec3::Y, &[[0.045, 0.0], [0.045, 0.006], [0.038, 0.014], [0.02, 0.019], [0.0, 0.02]], 20, true);
    let s = sec(3.0);
    let p = Vec3::new(0.0, s.top, 3.0);
    let blade = [Vec2::new(-0.06, 0.0), Vec2::new(0.07, 0.0), Vec2::new(0.06, 0.20), Vec2::new(0.02, 0.21)];
    tailcone.slab(&BLACK, &blade, &[], p - Vec3::X * 0.004 - Vec3::Y * 0.005, Vec3::Z, Vec3::Y, 0.008);
    let s = sec(3.6);
    let p = Vec3::new(0.0, s.top, 3.6);
    tailcone.tube(&BLACK, &[p, p + Vec3::new(0.0, 0.36, 0.14)], &|t| 0.004 - t * 0.0025, 6, true);
    let s = sec(1.25);
    let p = Vec3::new(0.0, s.bot, 1.25);
    let blade = [Vec2::new(-0.03, 0.0), Vec2::new(0.035, 0.0), Vec2::new(0.02, -0.085), Vec2::new(0.0, -0.09)];
    cabin.slab(&BLACK, &blade, &[], p + Vec3::X * 0.003, Vec3::Z, Vec3::Y, 0.006);
    let s = sec(2.35);
    cabin.lathe(&BLACK, Vec3::new(0.0, s.bot - 0.012, 2.35), Vec3::Z, &[[0.0, -0.11], [0.012, -0.09], [0.016, -0.04], [0.016, 0.04], [0.012, 0.09], [0.0, 0.11]], 12, true);
    // The tail tie-down ring.
    let s = sec(4.45);
    let p = Vec3::new(0.0, s.bot - 0.005, 4.45);
    let ring: Vec<Vec3> = (0..=16).map(|k| { let a = k as f32 / 16.0 * TAU; p + Vec3::new(0.0, -0.03 - 0.03 * a.cos(), 0.03 * a.sin()) }).collect();
    tailcone.tube(&STEEL, &ring, &|_| 0.005, 6, false);
    // The white tail light on the tail cone's end.
    tailcone.lathe(&mt(mat::LAMP, 0.95, 0.95, 0.95).x(lamp::TAIL), Vec3::new(0.0, 0.262, TAIL_Z + 0.003), Vec3::Z, &[[0.014, 0.0], [0.014, 0.012], [0.0, 0.02]], 12, true);

    b.fixed("cowling", G_ENGINE, cowl);
    b.fixed("cabin skin", G_FUSE, cabin);
    let t = b.fixed("tail cone", G_TAIL, tailcone);
    b.role(t, Role::TailCone);
}

/// The nose bowl's joint with the engine compartment doors, and the height of
/// the doors' bottom edges (both drawn in the shader; fasteners here).
pub const NOSE_BOWL_Z: f32 = -2.032;
pub const COWL_SPLIT_Y: f32 = -0.15;

/// Height of the upper skin at station z, lateral position x.
pub fn fuse_top(z: f32, x: f32) -> f32 {
    let s = sec(z);
    let x = x.abs().min(s.w * 0.999);
    let (mut a, mut b) = (0.0, FRAC_PI_2);
    for _ in 0..30 {
        let m = (a + b) * 0.5;
        if sec_pt(&s, m).x < x { a = m } else { b = m }
    }
    sec_pt(&s, (a + b) * 0.5).y
}

/// The cooling-air inlets either side of the spinner, as on photographs of
/// IO-550 A36s: big round-cornered openings filling the nose bowl's face
/// from beside the spinner out to a narrow lip (sizes estimated from
/// photographs against the 13 in spinner, no drawing to hand). Their inner
/// edge (its least radius about the spinner axis, and least x), top and
/// bottom (y), the lip left at the bowl's rim, and the corners' rounding.
const INLET: [f32; 6] = [0.172, 0.17, 0.135, -0.085, 0.010, 0.03];
const LANDING: [f32; 2] = [-0.16, 0.05];
const INDUCTION: [f32; 3] = [-0.25, 0.06, 0.028];

/// A rounded rectangle's outline (counter-clockwise), centre c, half size h, corner r.
fn rrect(c: Vec2, h: Vec2, r: f32, per: usize) -> Vec<Vec2> {
    let mut v = Vec::new();
    for (k, corner) in [Vec2::new(1.0, 1.0), Vec2::new(-1.0, 1.0), Vec2::new(-1.0, -1.0), Vec2::new(1.0, -1.0)].iter().enumerate() {
        let cc = c + (h - Vec2::splat(r)) * *corner;
        for i in 0..=per {
            let a = (k as f32 + i as f32 / per as f32) * FRAC_PI_2;
            v.push(cc + Vec2::new(a.cos(), a.sin()) * r);
        }
    }
    v
}

/// The right cooling inlet's outline on the nose face (x, y; counter-clockwise),
/// given the face's outline: inside the rim by the lip, outside a circle round
/// the spinner axis, between a top and a bottom, the corners rounded.
fn inlet_outline(rim: &[Vec2]) -> Vec<Vec2> {
    let [r_in, x_in, top, bot, lip, k] = INLET;
    let axis = Vec2::new(0.0, THRUST_Y);
    let smax = |a: f32, b: f32| { let h = (k - (a - b).abs()).max(0.0) / k; a.max(b) + h * h * k * 0.25 };
    // Signed distance to the opening (negative inside).
    let d = |p: Vec2| -> f32 {
        let to_rim = rim.iter().map(|q| q.distance(p)).fold(f32::MAX, f32::min);
        let inside = span_at(rim, p.y).is_some_and(|(a, b)| p.x > a && p.x < b);
        let mut d = if inside { lip - to_rim } else { lip + to_rim };
        d = smax(d, (r_in - p.distance(axis)).max(x_in - p.x));
        d = smax(d, p.y - top);
        smax(d, bot - p.y)
    };
    let c = Vec2::new(0.225, 0.02);
    (0..56)
        .map(|i| {
            let a = i as f32 / 56.0 * TAU;
            let dir = Vec2::new(a.cos(), a.sin());
            let (mut lo, mut hi) = (0.0f32, 0.3f32);
            for _ in 0..30 {
                let m = (lo + hi) * 0.5;
                if d(c + dir * m) < 0.0 { lo = m } else { hi = m }
            }
            c + dir * lo
        })
        .collect()
}

/// Where a closed outline crosses the line at height y: its leftmost and rightmost x.
fn span_at(ring: &[Vec2], y: f32) -> Option<(f32, f32)> {
    let mut xs = Vec::new();
    for (k, a) in ring.iter().enumerate() {
        let b = ring[(k + 1) % ring.len()];
        if (a.y - y) * (b.y - y) < 0.0 {
            xs.push(a.x + (b.x - a.x) * (y - a.y) / (b.y - a.y));
        }
    }
    let lo = xs.iter().copied().fold(f32::MAX, f32::min);
    let hi = xs.iter().copied().fold(f32::MIN, f32::max);
    (xs.len() >= 2).then_some((lo, hi))
}

/// The flat face of the nose bowl round the spinner, with the cooling
/// inlets, the landing light and the induction air inlet in it, and the
/// ducts and baffles behind them.
fn nose_face(m: &mut Mesh) {
    let s = sec(NOSE_FACE_Z);
    // (u, v) on the face: x = -u (the slab's first axis points left so it faces ahead).
    let outline: Vec<Vec2> = (0..64).map(|k| { let p = sec_pt(&s, -(k as f32) / 64.0 * TAU); Vec2::new(-p.x, p.y) }).collect();
    let rim: Vec<Vec2> = (0..512).map(|k| sec_pt(&s, k as f32 / 512.0 * TAU)).collect();
    let inlet = inlet_outline(&rim);
    let mut holes = Vec::new();
    for sx in [-1.0f32, 1.0] {
        // (u = -x: the right inlet lies at negative u, turned back counter-clockwise.)
        let mut h: Vec<Vec2> = inlet.iter().map(|q| Vec2::new(-sx * q.x, q.y)).collect();
        if sx > 0.0 {
            h.reverse();
        }
        holes.push(h);
    }
    holes.push((0..24).map(|k| { let a = -(k as f32) / 24.0 * TAU; Vec2::new(LANDING[1] * a.cos(), LANDING[0] + LANDING[1] * a.sin()) }).collect());
    holes.push(rrect(Vec2::new(0.0, INDUCTION[0]), Vec2::new(INDUCTION[1], INDUCTION[2]), 0.012, 3));
    let o = Vec3::new(0.0, 0.0, NOSE_FACE_Z + 0.002);
    m.slab(&WHITE.pat(pat::COWL), &outline, &holes, o, Vec3::NEG_X, Vec3::Y, 0.0);

    // Inside: a near-black duct and baffles, the finned cylinder heads behind.
    let duct = mt(mat::SATIN, 0.018, 0.018, 0.017);
    let fins = mt(mat::SATIN, 0.045, 0.045, 0.047);
    // Each opening's duct: its outline swept back, narrowing.
    let sweep = |m: &mut Mesh, ring: &[Vec2], depth: f32, shrink: f32| {
        let c = ring.iter().copied().sum::<Vec2>() / ring.len() as f32;
        let n = ring.len();
        m.patch(&duct, n, 4, false, |u, v| {
            let k = ((u * n as f32) as usize).min(n - 1);
            let k2 = (k + 1) % n;
            let f = u * n as f32 - k as f32;
            let q = ring[k].lerp(ring[k2], f);
            let q = c + (q - c) * (1.0 - shrink * v);
            (Vec3::new(-q.x, q.y, o.z + depth * v), [u, v], 0.0)
        });
        let back: Vec<Vec3> = ring.iter().map(|q| { let q = c + (*q - c) * (1.0 - shrink); Vec3::new(-q.x, q.y, o.z + depth) }).collect();
        m.fan(&mt(mat::BLACK, 0.02, 0.02, 0.02), Vec3::new(-c.x, c.y, o.z + depth), &back, Vec3::NEG_Z);
    };
    for k in 0..2 {
        sweep(m, &holes[k], 0.24, 0.22);
        // The finned cylinder heads seen through the inlet, across its width.
        let c = holes[k].iter().copied().sum::<Vec2>() / holes[k].len() as f32;
        for j in -7..=7 {
            let y = c.y + j as f32 * 0.0135;
            if let Some((a, b)) = span_at(&holes[k], y) {
                let (mid, half) = ((a + b) * 0.5, (b - a) * 0.5 * 0.72);
                let mid = c.x + (mid - c.x) * 0.8;
                m.rbox(&fins, Vec3::new(-mid, c.y + (y - c.y) * 0.8, o.z + 0.20), Vec3::new(half, 0.0018, 0.014), 0.001, Quat::IDENTITY);
            }
        }
    }
    sweep(m, &holes[3], 0.05, 0.1);
    // The induction air filter's grille.
    for j in -4..=4 {
        let x = j as f32 * 0.013;
        m.rbox(&mt(mat::BLACK, 0.03, 0.03, 0.03), Vec3::new(x, INDUCTION[0], o.z + 0.02), Vec3::new(0.0015, INDUCTION[2], 0.01), 0.001, Quat::IDENTITY);
    }
    // The landing light: a reflector cup and its lens.
    let lc = Vec3::new(0.0, LANDING[0], o.z);
    m.lathe(&CHROME, lc + Vec3::Z * 0.06, Vec3::NEG_Z, &[[0.0, 0.0], [0.02, 0.005], [0.04, 0.025], [LANDING[1], 0.06]], 24, true);
    m.disc(&mt(mat::LAMP, 0.95, 0.92, 0.85).x(lamp::LANDING), lc + Vec3::Z * 0.008, Vec3::NEG_Z, Vec3::Y, LANDING[1] - 0.004, 24);
}

/// The louvred cooling-air exits on each side of the lower cowl: slats
/// standing proud of the skin over a dark opening.
fn louvers(m: &mut Mesh) {
    let (z0, z1, y0, y1) = (-1.96f32, -1.55f32, -0.31f32, -0.11f32);
    let zc = (z0 + z1) * 0.5;
    for sx in [-1.0f32, 1.0] {
        for k in 0..7 {
            let y = y0 + 0.02 + k as f32 * (y1 - y0 - 0.04) / 6.0;
            let (p, n) = on_side(zc, y, sx, 0.0);
            let t = (Vec3::Z - n * n.z).normalize();
            let rot = Quat::from_mat3(&glam::Mat3::from_cols(t, n, t.cross(n)));
            m.rbox(&mt(mat::BLACK, 0.015, 0.015, 0.015), p - n * 0.001, Vec3::new((z1 - z0) * 0.46, 0.0015, 0.011), 0.001, rot);
            m.rbox(&WHITE.pat(pat::COWL), p + n * 0.005 + rot * Vec3::Z * 0.004, Vec3::new((z1 - z0) * 0.45, 0.0035, 0.009), 0.003, rot);
        }
    }
}

// ---------------------------------------------------------------- wings

/// The tip station (half the span), the outer rib where the rounded tip
/// begins, and the wing skin's root (inside the cabin: the carry-through).
pub const SPAN: f32 = 5.10;
pub const TIP_RIB: f32 = 4.84;
const ROOT: f32 = 0.25;
pub const FLAP: [f32; 2] = [0.66, 2.845];
pub const AILERON: [f32; 2] = [2.875, 4.81];
/// The span the leading-edge fuel cells fill.
pub const TANK: [f32; 2] = [0.70, 2.30];
/// Hinge lines (chord fractions) of the flaps and ailerons.
const FLAP_H: f32 = 0.80;
const AIL_H: f32 = 0.79;
/// How far the upper skin overhangs the hinge lines (chord fraction).
const SHROUD: f32 = 0.045;
/// Out to here the lower skin carries the fuselage pattern, so the wheel
/// well can be cut out of it (`airframe::cut_by`, `well`).
const LOWER_CUT: f32 = 1.75;
/// 6° of dihedral (estimate, see the module notes).
const DIH: f32 = 0.10472;
/// Root chord (at the centreline) and its taper per metre out; the leading
/// edge's sweep per metre out; the chord plane's height at the centreline.
const ROOT_C: f32 = 2.220;
const TAPER: f32 = 0.2135;
const LE_SWEEP: f32 = 0.05512;
const ROOT_LE_Z: f32 = st(84.5);
const WING_Y0: f32 = -0.407;

/// Right wing section at span station xs (metres out, horizontal).
pub fn wing_sect(xs: f32) -> Sect {
    let mut le = ROOT_LE_Z + LE_SWEEP * xs;
    let mut c = ROOT_C - TAPER * xs;
    // NACA 23016.5 at the root thinning to 23012 at the tip.
    let mut tf = (0.165 - 0.045 * (xs / TIP_RIB).clamp(0.0, 1.0)) / 0.165;
    if xs > TIP_RIB {
        let u = ((xs - TIP_RIB) / (SPAN - TIP_RIB)).min(1.0);
        let k = 1.0 - (1.0 - u * u).max(0.0).sqrt();
        le += c * 0.32 * k;
        c -= c * 0.55 * k;
        tf *= (1.0 - u * u).max(0.0).sqrt();
    }
    let y = WING_Y0 + xs * DIH.tan();
    let inc = (4.0 - 3.0 * (xs / SPAN).min(1.0)).to_radians();
    let up = Vec3::new(-DIH.sin(), DIH.cos() * inc.cos(), inc.sin()).normalize();
    Sect { le: Vec3::new(xs, y, le), c, cdir: Vec3::new(0.0, -inc.sin(), inc.cos()), up, tf }
}
pub fn wing_foil() -> Foil {
    Foil { at: Box::new(wing_sect), t: 0.165, camber: 0.02 }
}
/// Point and outward normal on a foil.
pub fn foil_frame(f: &Foil, s: f32, xc: f32, side: f32) -> (Vec3, Vec3) {
    let p = f.pt(s, xc, side, -1.0);
    let ds = f.pt(s + 0.002, xc, side, -1.0) - f.pt(s - 0.002, xc, side, -1.0);
    let dx = f.pt(s, xc + 0.002, side, -1.0) - f.pt(s, (xc - 0.002).max(0.0), side, -1.0);
    let n = (ds.cross(dx) * side).normalize();
    (p, n)
}
/// The wing's upper (side 1) or lower (-1) skin over (x, z).
fn wing_at(f: &Foil, x: f32, z: f32, side: f32) -> Vec3 {
    let k = wing_sect(x);
    let xc = ((z - k.le.z) / k.c).clamp(0.0, 1.0);
    f.pt(x, xc, side, -1.0)
}
fn wing_top(f: &Foil, x: f32, z: f32) -> Vec3 {
    wing_at(f, x, z, 1.0)
}

/// One side of a foil's skin (1 upper, -1 lower) between chord fractions x0 and x1.
fn half_skin(f: &Foil, mesh: &mut Mesh, m: &Mat, s0: f32, s1: f32, ns: usize, x0: f32, x1: f32, nx: usize, side: f32) {
    let k = (f.at)((s0 + s1) * 0.5);
    let ds = (f.at)(s1).le - (f.at)(s0).le;
    let flip = ds.cross(-k.cdir).dot(k.up) < 0.0;
    let rows: Vec<Vec<SP>> = (0..=ns)
        .map(|i| {
            let s = s0 + (s1 - s0) * i as f32 / ns as f32;
            let c = (f.at)(s).c;
            (0..=nx)
                .map(|j| {
                    let w = if side > 0.0 { 1.0 - j as f32 / nx as f32 } else { j as f32 / nx as f32 };
                    let xc = x0 + (x1 - x0) * (1.0 - (w * FRAC_PI_2).cos());
                    let wear = 1.0 - smoothstep(0.0, 0.1, xc);
                    (f.pt(s, xc, side, -1.0), [s, side * xc * c], wear)
                })
                .collect()
        })
        .collect();
    mesh.grid(m, &rows, false, flip);
}

fn wings(b: &mut Builder) {
    let f = wing_foil();
    let skin = WHITE.pat(pat::WING);
    let lower_cut = WHITE.pat(pat::FUSE);
    let ctrl = WHITE.pat(pat::CTRL);
    let mut w = Mesh::default();
    // Upper skin, then lower: full chord where nothing hinges, to the hinge
    // lines along the flaps and ailerons.
    for (side, root_m) in [(1.0f32, &skin), (-1.0, &lower_cut)] {
        half_skin(&f, &mut w, root_m, ROOT, FLAP[0], 6, 0.0, 1.0, 28, side);
        // (The upper skin runs on past the hinge as a shroud over the
        // flaps' and ailerons' noses, as on the real wing: only a narrow
        // gap shows from above.)
        let shroud = if side > 0.0 { SHROUD } else { 0.0 };
        if side > 0.0 {
            half_skin(&f, &mut w, &skin, FLAP[0], FLAP[1], 22, 0.0, FLAP_H + shroud, 28, side);
        } else {
            half_skin(&f, &mut w, &lower_cut, FLAP[0], LOWER_CUT, 14, 0.0, FLAP_H, 28, side);
            half_skin(&f, &mut w, &skin, LOWER_CUT, FLAP[1], 10, 0.0, FLAP_H, 28, side);
        }
        half_skin(&f, &mut w, &skin, FLAP[1], AILERON[0], 2, 0.0, 1.0, 28, side);
        half_skin(&f, &mut w, &skin, AILERON[0], AILERON[1], 20, 0.0, AIL_H + shroud, 28, side);
        half_skin(&f, &mut w, &skin, AILERON[1], SPAN, 20, 0.0, 1.0, 28, side);
    }
    f.web(&mut w, &PRIMER, FLAP[0], FLAP[1], 12, &|_| FLAP_H);
    f.web(&mut w, &PRIMER, AILERON[0], AILERON[1], 12, &|_| AIL_H);
    for (s, a) in [(FLAP[0], FLAP_H), (FLAP[1], FLAP_H), (AILERON[0], AIL_H), (AILERON[1], AIL_H)] {
        f.cap(&mut w, &PRIMER, s, a, 1.0, false);
    }
    f.cap(&mut w, &PRIMER, ROOT, 0.0, 1.0, false);

    // The main wheel well: a drum round the wheel's opening and the slot for
    // the leg, up to just under the upper skin.
    well_liner(&f, &mut w);

    // Fuel filler on the leading-edge cell, its sump drain, fuel cell access
    // plates and inspection plates under the wing, the tie-down ring.
    let (p, n) = foil_frame(&f, 1.95, 0.14, 1.0);
    w.lathe(&ALU, p, n, &[[0.045, -0.002], [0.045, 0.002], [0.038, 0.004], [0.0, 0.004]], 28, false);
    w.lathe(&mt(mat::ALU, 0.5, 0.5, 0.52), p + n * 0.004, n, &[[0.032, 0.0], [0.032, 0.006], [0.026, 0.009], [0.0, 0.009]], 24, false);
    let tab = n.any_orthonormal_vector();
    w.rbox(&mt(mat::PAINT, 0.5, 0.04, 0.03), p + n * 0.015, Vec3::new(0.022, 0.006, 0.003), 0.0025, Quat::from_mat3(&glam::Mat3::from_cols(tab, n, tab.cross(n))));
    let (p, n) = foil_frame(&f, 1.30, 0.12, -1.0);
    w.lathe(&ALU, p, n, &[[0.008, -0.002], [0.008, 0.006], [0.004, 0.009], [0.004, 0.016], [0.0, 0.016]], 10, false);
    for &(s, xc) in &[(2.10f32, 0.42f32), (3.30, 0.40), (4.30, 0.38)] {
        let (p, n) = foil_frame(&f, s, xc, -1.0);
        inspection_plate(&mut w, p + n * 0.0012, n, Vec3::Z, 0.055, 6);
    }
    let (p, _) = foil_frame(&f, 3.0, 0.30, -1.0);
    let ring: Vec<Vec3> = (0..=16).map(|k| { let a = k as f32 / 16.0 * TAU; p + Vec3::new(0.0, -0.03 + 0.03 * a.cos(), 0.03 * a.sin()) }).collect();
    w.tube(&STEEL, &ring, &|_| 0.005, 6, false);
    // The fuel vent under the wing.
    let (p, n) = foil_frame(&f, 2.6, 0.30, -1.0);
    w.tube(&ALU, &[p, p + n * 0.04, p + n * 0.05 - Vec3::Z * 0.05], &|_| 0.005, 8, true);
    // Hinge brackets under the flaps and ailerons.
    let bracket = |w: &mut Mesh, s: f32, xc: f32| {
        let (p, n) = foil_frame(&f, s, xc, -1.0);
        let rot = Quat::from_rotation_arc(Vec3::Y, n);
        w.rbox(&PAINT_WHITE, p + n * 0.012, Vec3::new(0.0035, 0.016, 0.034), 0.002, rot);
        w.bolt(&STEEL, p + n * 0.02 + rot * Vec3::X * 0.0035, rot * Vec3::X, 0.003);
    };
    for s in [0.95f32, 1.60, 2.25] {
        bracket(&mut w, s, FLAP_H - 0.02);
    }
    for s in [3.15f32, 3.85, 4.55] {
        bracket(&mut w, s, AIL_H - 0.02);
    }
    // Nav light (green on this side) and strobe on the tip, in a clear fairing.
    let (p, _) = foil_frame(&f, SPAN - 0.02, 0.08, 0.0);
    w.lathe(&mt(mat::LAMP, 0.9, 0.9, 0.9).x(lamp::STROBE), p + Vec3::new(0.0, 0.0, 0.07), Vec3::X, &[[0.015, 0.0], [0.015, 0.014], [0.011, 0.028], [0.0, 0.032]], 14, true);
    w.lathe(&mt(mat::LAMP, 0.1, 0.9, 0.2).x(lamp::NAV_GREEN), p + Vec3::new(0.0, 0.0, -0.02), Vec3::new(0.6, 0.0, -0.8), &[[0.02, 0.0], [0.02, 0.012], [0.013, 0.027], [0.0, 0.031]], 16, true);
    // Screws round the tip fairing.
    for k in 0..14 {
        let xc = 0.03 + k as f32 * 0.068;
        for side in [-1.0f32, 1.0] {
            let (p, n) = foil_frame(&f, TIP_RIB - 0.012, xc, side);
            w.screw(&SCREW_PAINT, p + n * 0.0003, n, 0.0035, xc * 17.0 + side);
        }
    }

    // The left wing gets the pitot tube (under it, mid-span), the stall
    // warning vane in the leading edge and the red light (built on the right, mirrored).
    let mut left_extra = Mesh::default();
    let (p, n) = foil_frame(&f, 2.55, 0.22, -1.0);
    let bend = p + n * 0.11;
    left_extra.slab(&WHITE, &[Vec2::new(-0.035, 0.0), Vec2::new(0.035, 0.0), Vec2::new(0.012, -0.10), Vec2::new(-0.012, -0.10)], &[], p + Vec3::X * 0.003, Vec3::Z, Vec3::Y, 0.006);
    left_extra.tube(&STEEL, &[bend + Vec3::Z * 0.04, bend - Vec3::Z * 0.19], &|t| if t > 0.9 { 0.006 } else { 0.0075 }, 10, true);
    let (p, n) = foil_frame(&f, 2.05, 0.012, -1.0);
    let fwd = (Vec3::NEG_Z - n * Vec3::NEG_Z.dot(n)).normalize();
    let rot = Quat::from_mat3(&glam::Mat3::from_cols(Vec3::X, n, -fwd));
    left_extra.rbox(&ALU, p + n * 0.0008, Vec3::new(0.024, 0.0008, 0.017), 0.0006, rot);
    left_extra.rbox(&ALU, p + n * 0.003 + fwd * 0.012, Vec3::new(0.008, 0.0006, 0.009), 0.0004, rot);
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
    let mut s = 0.70;
    while s < TIP_RIB {
        let to = if (FLAP[0]..FLAP[1]).contains(&s) { FLAP_H - 0.03 } else if (AILERON[0]..AILERON[1]).contains(&s) { AIL_H - 0.03 } else { 0.97 };
        ribs.push((s, 0.03, to));
        s += 0.28;
    }
    structure(&f, &mut ws, 0.6, TIP_RIB, &[0.23, 0.62], &ribs);
    let l = b.fixed("left wing structure", G_LWING, ws.mirror_x());
    let r = b.fixed("right wing structure", G_RWING, ws);
    b.role(l, Role::WingStructure(0)).role(r, Role::WingStructure(1));

    // Flaps: hinged at their nose under the wing, down to 30°.
    let mut fl = Mesh::default();
    f.skin(&mut fl, &ctrl, FLAP[0] + 0.01, FLAP[1] - 0.01, 22, &|_| FLAP_H + 0.005, &|_| 1.0, 14, true);
    f.cap(&mut fl, &ctrl, FLAP[0] + 0.01, FLAP_H + 0.005, 1.0, true);
    f.cap(&mut fl, &ctrl, FLAP[1] - 0.01, FLAP_H + 0.005, 1.0, true);
    let axis = (f.pt(FLAP[1], FLAP_H + 0.01, -0.6, -1.0) - f.pt(FLAP[0], FLAP_H + 0.01, -0.6, -1.0)).normalize();
    let pivot = f.pt((FLAP[0] + FLAP[1]) * 0.5, FLAP_H + 0.01, -0.6, -1.0);
    let (l, r) = b.pair(["left flap", "right flap"], [G_LWING, G_RWING], fl, |_| Anim::hinge(Drive::new(Input::Flaps, 30f32.to_radians())), pivot, axis, [None, None]);
    b.role(l, Role::Flap(0)).role(r, Role::Flap(1));

    // Ailerons, 20° each way, with the fixed sheet-metal tab on the trailing edge.
    let mut al = Mesh::default();
    f.skin(&mut al, &ctrl, AILERON[0] + 0.01, AILERON[1] - 0.01, 20, &|_| AIL_H + 0.005, &|_| 1.0, 14, true);
    f.cap(&mut al, &ctrl, AILERON[0] + 0.01, AIL_H + 0.005, 1.0, true);
    f.cap(&mut al, &ctrl, AILERON[1] - 0.01, AIL_H + 0.005, 1.0, true);
    let (a, c) = (f.pt(3.55, 0.995, 0.0, -1.0), f.pt(3.85, 0.995, 0.0, -1.0));
    let k = wing_sect(3.7);
    al.slab(&WHITE.pat(pat::CTRL), &[Vec2::new(0.0, 0.0), Vec2::new((c - a).length(), 0.0), Vec2::new((c - a).length() - 0.01, 0.05), Vec2::new(0.01, 0.05)], &[], a + k.up * 0.0012, (c - a).normalize(), k.cdir, 0.0024);
    let axis = (f.pt(AILERON[1], AIL_H + 0.01, 0.0, -1.0) - f.pt(AILERON[0], AIL_H + 0.01, 0.0, -1.0)).normalize();
    let pivot = f.pt((AILERON[0] + AILERON[1]) * 0.5, AIL_H + 0.01, 0.0, -1.0);
    let (l, r) = b.pair(["left aileron", "right aileron"], [G_LWING, G_RWING], al, |s| Anim::hinge(Drive::new(Input::Aileron(s), 0.35)), pivot, axis, [None, None]);
    b.role(l, Role::Aileron(0)).role(r, Role::Aileron(1));
}

/// The main wheel well behind its opening in the lower skin (right wing).
fn well_liner(f: &Foil, m: &mut Mesh) {
    let roof = |x: f32, z: f32| wing_at(f, x, z, 1.0).y - 0.025;
    let low = |x: f32, z: f32| wing_at(f, x, z, -1.0).y + 0.003;
    let [cx, cz, r] = MAIN_WELL;
    let n = 40;
    let ring: Vec<(f32, f32)> = (0..=n).map(|k| { let a = k as f32 / n as f32 * TAU; (cx + r * a.cos(), cz + r * a.sin()) }).collect();
    let rows: Vec<Vec<SP>> = ring.iter().map(|&(x, z)| vec![(Vec3::new(x, low(x, z), z), [x, z], 0.0), (Vec3::new(x, roof(x, z), z), [x, z], 0.0)]).collect();
    m.grid(&PRIMER, &rows, false, false);
    let top: Vec<Vec3> = ring[..n].iter().map(|&(x, z)| Vec3::new(x, roof(x, z), z)).collect();
    m.fan(&PRIMER, Vec3::new(cx, roof(cx, cz), cz), &top, Vec3::NEG_Y);
    let [x0, x1, z0, z1] = MAIN_SLOT;
    let slot_roof: f32 = -0.30;
    for (a, c) in [((x0, z0), (x1, z0)), ((x1, z0), (x1, z1)), ((x1, z1), (x0, z1))] {
        let rows: Vec<Vec<SP>> = (0..=8).map(|i| {
            let t = i as f32 / 8.0;
            let (x, z) = (a.0 + (c.0 - a.0) * t, a.1 + (c.1 - a.1) * t);
            vec![(Vec3::new(x, low(x, z), z), [x, z], 0.0), (Vec3::new(x, slot_roof.min(roof(x, z)), z), [x, z], 0.0)]
        }).collect();
        m.grid(&PRIMER, &rows, false, false);
    }
    m.quad(&PRIMER, [Vec3::new(x0, slot_roof, z0), Vec3::new(x1, slot_roof, z0), Vec3::new(x1, slot_roof, z1), Vec3::new(x0, slot_roof, z1)]);
}

// ---------------------------------------------------------------- tail

/// The stabiliser: half span (12 ft 2.1 in overall, [S]), the chord plane's
/// height, root and tip leading and trailing edges, the elevator hinge line.
pub const STAB_HALF: f32 = 1.855;
pub const STAB_Y: f32 = 0.31;
const STAB_LE: [f32; 2] = [st(273.0), st(280.0)];
const STAB_TE: [f32; 2] = [st(319.0), st(313.5)];
const ELEV_Z: f32 = st(298.5);
/// The fin: its root (on the tail cone) and tip heights.
pub const FIN_ROOT_Y: f32 = 0.42;
pub const FIN_TIP_Y: f32 = 1.68;

fn tail(b: &mut Builder) {
    let stab = Foil {
        at: Box::new(|s: f32| {
            let a = s.abs() / (STAB_HALF - 0.17);
            let mut le = STAB_LE[0] + (STAB_LE[1] - STAB_LE[0]) * a.min(1.0);
            let te = STAB_TE[0] + (STAB_TE[1] - STAB_TE[0]) * a.min(1.0);
            let mut c = te - le;
            let mut tf = 1.0;
            if s.abs() > STAB_HALF - 0.17 {
                let u = ((s.abs() - (STAB_HALF - 0.17)) / 0.17).min(1.0);
                let k = 1.0 - (1.0 - u * u).max(0.0).sqrt();
                le += c * 0.25 * k;
                c -= c * 0.45 * k;
                tf = (1.0 - u * u).max(0.0).sqrt();
            }
            Sect { le: Vec3::new(s, STAB_Y, le), c, cdir: Vec3::Z, up: Vec3::Y, tf }
        }),
        t: 0.10,
        camber: 0.0,
    };
    let hinge = |s: f32| {
        let k = (stab.at)(s);
        ((ELEV_Z - k.le.z) / k.c).min(0.99)
    };
    let tailc = WHITE.pat(pat::TAIL);
    let ctrl = WHITE.pat(pat::CTRL);
    let mut m = Mesh::default();
    stab.skin(&mut m, &tailc, -STAB_HALF, STAB_HALF, 64, &|_| 0.0, &|s| hinge(s) - 0.004, 24, false);
    stab.web(&mut m, &PRIMER, -STAB_HALF, STAB_HALF, 30, &|s| hinge(s) - 0.004);
    for s in [-1.0f32, 1.0] {
        let p = stab.pt(s * 0.95, 0.3, -1.0, -1.0);
        inspection_plate(&mut m, p - Vec3::Y * 0.0012, Vec3::NEG_Y, Vec3::Z, 0.04, 4);
    }
    // The elevators, left and right on one torque tube, the rudder passing
    // down between their inboard ends; a trim tab inboard on each.
    let mut el = Mesh::default();
    for sx in [-1.0f32, 1.0] {
        let tip = STAB_HALF - 0.015;
        for (a, c, x1) in [(ELEV_ROOT, TAB_IN, 1.0f32), (TAB_IN, TAB_OUT, TAB_H - 0.003), (TAB_OUT, tip, 1.0)] {
            let (s0, s1) = if sx > 0.0 { (a, c) } else { (-c, -a) };
            stab.skin(&mut el, &ctrl, s0, s1, ((c - a) / 0.04).ceil() as usize, &|s| hinge(s) + 0.004, &|_| x1, 12, true);
            if x1 < 1.0 {
                stab.web(&mut el, &PRIMER, s0, s1, 12, &|_| x1);
            }
        }
        for s in [sx * ELEV_ROOT, sx * tip] {
            stab.cap(&mut el, &ctrl, s, hinge(s) + 0.004, 1.0, true);
        }
    }
    // Elevator horn and its cable bolts in the tail cone.
    let hp = Vec3::new(0.0, STAB_Y, ELEV_Z + 0.02);
    el.rbox(&GREY, hp - Vec3::Y * 0.04, Vec3::new(0.012, 0.05, 0.02), 0.004, Quat::IDENTITY);
    el.tube(&STEEL, &[Vec3::new(-ELEV_ROOT, STAB_Y, ELEV_Z + 0.01), Vec3::new(ELEV_ROOT, STAB_Y, ELEV_Z + 0.01)], &|_| 0.012, 10, true);

    let mut tabs = Mesh::default();
    for sx in [-1.0f32, 1.0] {
        let (a, c) = (sx * (TAB_IN + 0.006), sx * (TAB_OUT - 0.006));
        stab.skin(&mut tabs, &ctrl, a.min(c), a.max(c), 10, &|_| TAB_H + 0.003, &|_| 1.0, 6, true);
        for s in [a, c] {
            stab.cap(&mut tabs, &ctrl, s, TAB_H + 0.003, 1.0, true);
        }
        // The tab's push rod and horn under the elevator.
        let h = stab.pt(sx * 0.5, TAB_H + 0.02, -1.0, -1.0);
        tabs.rbox(&GREY, h - Vec3::Y * 0.012, Vec3::new(0.003, 0.012, 0.012), 0.002, Quat::IDENTITY);
        let h0 = stab.pt(sx * 0.5, hinge(0.5) + 0.05, -1.0, -1.0) - Vec3::Y * 0.01;
        tabs.tube(&STEEL, &[h0, h - Vec3::Y * 0.02], &|_| 0.0025, 6, true);
    }

    let fin_le = |y: f32| st(273.0) + 0.7225 * (y - wl(29.7));
    let fin_hinge = |y: f32| st(300.0) + 0.36 * (y - 0.41);
    let rudder_te = |y: f32| st(319.0) + 0.18 * (y - 0.29);
    let fin = Foil {
        at: Box::new(move |s: f32| {
            let y = FIN_ROOT_Y + s;
            let mut le = fin_le(y);
            let te = rudder_te(y);
            let mut c = te - le;
            let mut tf = 1.0 - 0.25 * s / (FIN_TIP_Y - FIN_ROOT_Y);
            let top = FIN_TIP_Y - FIN_ROOT_Y;
            if s > top - 0.10 {
                let v = ((s - (top - 0.10)) / 0.10).min(1.0);
                let k = 1.0 - (1.0 - v * v).max(0.0).sqrt();
                le += c * 0.10 * k;
                c -= c * 0.12 * k;
                tf *= (1.0 - v * v).max(0.05).sqrt();
            }
            Sect { le: Vec3::new(0.0, y, le), c, cdir: Vec3::Z, up: Vec3::X, tf }
        }),
        t: 0.10,
        camber: 0.0,
    };
    let fh = |s: f32| {
        let k = (fin.at)(s);
        (fin_hinge(FIN_ROOT_Y + s) - k.le.z) / k.c
    };
    // The fin proper ends under the rudder's horn balance.
    let horn = HORN_Y - FIN_ROOT_Y;
    fin.skin(&mut m, &tailc, -0.06, horn, 40, &|_| 0.0, &|s| fh(s) - 0.004, 24, false);
    fin.web(&mut m, &PRIMER, -0.06, horn, 20, &|s| fh(s) - 0.004);
    fin.cap(&mut m, &tailc, horn, 0.0, fh(horn) - 0.004, false);
    // The dorsal fin along the tail cone's top, blending into the fin's leading edge.
    dorsal(&mut m, fin_le);
    // VOR antenna whiskers on the fin, the beacon on the rudder's cap.
    for sx in [-1.0f32, 1.0] {
        let p = fin.pt(0.85, 0.10, 0.0, -1.0);
        m.tube(&BLACK, &[p, p + Vec3::new(sx * 0.38, 0.06, -0.18)], &|t| 0.005 - 0.002 * t, 6, true);
    }

    let mut ru = Mesh::default();
    let rtop = FIN_TIP_Y - FIN_ROOT_Y;
    fin.skin(&mut ru, &ctrl, RUDDER_Y0 - FIN_ROOT_Y, horn, 40, &|s| fh(s) + 0.004, &|_| 1.0, 12, true);
    fin.cap(&mut ru, &ctrl, RUDDER_Y0 - FIN_ROOT_Y, fh(RUDDER_Y0 - FIN_ROOT_Y) + 0.004, 1.0, true);
    // The horn balance over the fin's tip: the full section from the fin's
    // leading edge to the trailing edge, above the hinge break.
    fin.skin(&mut ru, &ctrl, horn + 0.006, rtop, 10, &|_| 0.04, &|_| 1.0, 16, true);
    fin.cap(&mut ru, &ctrl, horn + 0.006, 0.04, 1.0, true);
    let top = fin.pt(rtop - 0.02, 0.45, 0.0, -1.0);
    ru.lathe(&BLACK, top + Vec3::Y * 0.008, Vec3::Y, &[[0.02, 0.0], [0.02, 0.012], [0.0, 0.012]], 16, false);
    ru.lathe(&mt(mat::LAMP, 0.9, 0.06, 0.03).x(lamp::BEACON), top + Vec3::Y * 0.02, Vec3::Y, &[[0.017, 0.0], [0.017, 0.012], [0.012, 0.027], [0.0, 0.031]], 16, true);
    // The ground-adjustable tab low on the rudder's trailing edge.
    let (a, c) = (fin.pt(0.05, 0.995, 0.0, -1.0), fin.pt(0.30, 0.995, 0.0, -1.0));
    ru.slab(&WHITE.pat(pat::CTRL), &[Vec2::new(0.0, 0.0), Vec2::new((c - a).length(), 0.0), Vec2::new((c - a).length() - 0.01, 0.05), Vec2::new(0.01, 0.05)], &[], a - Vec3::X * 0.0012, (c - a).normalize(), Vec3::Z, 0.0024);
    // The rudder's horn and its cable bolts.
    let hp = Vec3::new(0.0, RUDDER_Y0 + 0.03, fin_hinge(RUDDER_Y0 + 0.03) + 0.02);
    ru.rbox(&GREY, hp, Vec3::new(0.08, 0.012, 0.022), 0.004, Quat::IDENTITY);
    for sx in [-1.0f32, 1.0] {
        ru.bolt(&STEEL, hp + Vec3::new(sx * 0.072, 0.012, 0.0), Vec3::Y, 0.004);
    }
    let rax = Vec3::new(0.0, 1.0, 0.36).normalize();

    let mut ts = Mesh::default();
    structure(&stab, &mut ts, -1.75, 1.75, &[0.20, 0.52], &[(-1.43, 0.03, 0.55), (-0.91, 0.03, 0.55), (-0.39, 0.03, 0.55), (0.39, 0.03, 0.55), (0.91, 0.03, 0.55), (1.43, 0.03, 0.55)]);
    structure(&fin, &mut ts, 0.05, horn - 0.05, &[0.25, 0.55], &[(0.35, 0.03, 0.62), (0.75, 0.03, 0.62)]);
    let k = b.fixed("tail structure", G_TAIL, ts);
    b.role(k, Role::TailStructure);
    let k = b.fixed("stabiliser", G_TAIL, m);
    b.role(k, Role::Stab);
    // 23° up, 20° down: the Cessna's sign, the mean travel; the trim holds it
    // a little up or down from the stick, and the tabs move against it.
    let e = b.add("elevator", G_TAIL, el, Anim::hinge2(Drive::new(Input::Elevator, -0.375), Drive::new(Input::Trim, -0.5).offset(0.02)), Vec3::new(0.0, STAB_Y, ELEV_Z), Vec3::X, None);
    b.role(e, Role::Elevator);
    let tz = (stab.at)(0.5).le.z + (stab.at)(0.5).c * TAB_H;
    let t = b.add("elevator trim tabs", G_TAIL, tabs, Anim::hinge(Drive::new(Input::Trim, 1.4).offset(-0.03)), Vec3::new(0.0, STAB_Y, tz), Vec3::X, Some(e));
    b.role(t, Role::Elevator);
    let k = b.add("rudder", G_TAIL, ru, Anim::hinge(Drive::new(Input::Rudder, 0.436)), Vec3::new(0.0, 0.41, fin_hinge(0.41)), rax, None);
    b.role(k, Role::Rudder);
}
/// The elevators' inboard ends (clear of the rudder) and the trim tabs' hinge.
const ELEV_ROOT: f32 = 0.09;
const TAB_H: f32 = 0.90;
const TAB_IN: f32 = 0.20;
const TAB_OUT: f32 = 0.82;
/// The rudder's foot, and where its horn balance begins over the fin.
const RUDDER_Y0: f32 = 0.43;
const HORN_Y: f32 = 1.56;

/// The dorsal fin: a thin fairing on the tail cone's top from the cargo
/// door's height of the spine back to the fin's leading edge.
fn dorsal(m: &mut Mesh, fin_le: impl Fn(f32) -> f32) {
    let y_end = FIN_ROOT_Y + 0.44;
    let (z0, z1) = (st(214.0), fin_le(y_end));
    let base1 = sec(z1).top - 0.01;
    m.patch(&WHITE.pat(pat::TAIL), 30, 16, true, |a, t| {
        let z = z0 + (z1 - z0) * a;
        let base = sec(z).top - 0.01;
        // Its top: rising slowly, then curving up into the fin's leading edge.
        let h = 0.012 + 0.04 * a + (y_end - base1 - 0.052) * a.powf(4.0);
        let w = 0.035 * (1.0 - 0.4 * a) * (a * 12.0).min(1.0);
        let th = -FRAC_PI_2 + TAU * t;
        let p = Vec3::new(w * th.cos(), base + h * (th.sin() * 0.5 + 0.5), z);
        (p, [z, t], 0.0)
    });
}

// ---------------------------------------------------------------- engine, propeller

fn engine(b: &mut Builder) {
    let mut m = Mesh::default();
    // The two exhaust stacks out of the bottom of the cowl, ahead of the firewall.
    for sx in [-1.0f32, 1.0] {
        let x = sx * 0.21;
        let y0 = fuse_bottom_y(-1.26, x);
        let pts = [Vec3::new(x, y0 + 0.06, -1.32), Vec3::new(x, y0 - 0.02, -1.25), Vec3::new(x * 1.05, y0 - 0.08, -1.17), Vec3::new(x * 1.07, y0 - 0.11, -1.11)];
        m.tube(&mt(mat::EXHAUST, 0.3, 0.28, 0.26), &pts, &|t| 0.031 + 0.004 * smoothstep(0.85, 1.0, t), 16, false);
        let d = (pts[3] - pts[2]).normalize();
        m.tube(&mt(mat::BLACK, 0.01, 0.01, 0.01), &[pts[3] - d * 0.05, pts[3] + d * 0.001], &|_| 0.03, 16, true);
    }
    b.fixed("exhaust", G_ENGINE, m);
    // The cowl flaps on the bottom of the cowl at the firewall, hinged on
    // their forward edges, opened by the cowl flap control.
    for sx in [-1.0f32, 1.0] {
        let mut cf = Mesh::default();
        let (z0, z1) = (FIREWALL_Z - 0.16, FIREWALL_Z - 0.006);
        cf.patch(&WHITE.pat(pat::COWL), 8, 6, sx > 0.0, |a, c| {
            let z = z0 + (z1 - z0) * a;
            let x = sx * (0.26 + 0.20 * c);
            (Vec3::new(x, fuse_bottom_y(z, x) - 0.002, z), [z, x], 0.0)
        });
        cf.patch(&PRIMER, 8, 6, sx < 0.0, |a, c| {
            let z = z0 + (z1 - z0) * a;
            let x = sx * (0.26 + 0.20 * c);
            (Vec3::new(x, fuse_bottom_y(z, x) + 0.003, z), [z, x], 0.0)
        });
        let x = sx * 0.36;
        let pivot = Vec3::new(x, fuse_bottom_y(z0, x), z0);
        b.add(if sx < 0.0 { "left cowl flap" } else { "right cowl flap" }, G_ENGINE, cf, Anim::hinge(Drive::new(Input::CowlFlaps, 0.30)), pivot, Vec3::NEG_X, None);
    }

    // Spinner and three blades (McCauley 82NDB, 80 in).
    let mut sp = Mesh::default();
    let len = PROP_HUB.z - SPINNER_TIP_Z + 0.15;
    let base = Vec3::new(0.0, THRUST_Y, SPINNER_TIP_Z + len);
    let prof: Vec<[f32; 2]> = (0..=18).map(|k| { let h = k as f32 / 18.0 * len; [SPINNER_R * (1.0 - (h / len).powf(2.1)).max(0.0).sqrt(), h] }).collect();
    sp.lathe(&WHITE, base, Vec3::NEG_Z, &prof, 40, true);
    sp.lathe(&GREY, base, Vec3::NEG_Z, &[[SPINNER_R + 0.004, -0.012], [SPINNER_R + 0.004, 0.0], [0.0, 0.0]], 40, false);
    for k in 0..8 {
        let a = k as f32 / 8.0 * TAU + 0.2;
        let p = base + Vec3::new(a.cos(), a.sin(), 0.0) * (SPINNER_R - 0.006) - Vec3::Z * 0.02;
        sp.screw(&SCREW_STEEL, p, Vec3::new(a.cos(), a.sin(), -0.15), 0.0045, k as f32);
    }
    let blade_m = mt(mat::PROP, 0.5, 0.5, 0.5);
    let mut blades = Mesh::default();
    let kr = PROP_R / 0.975;
    for k in 0..3 {
        let mut bl = Mesh::default();
        let blade = Foil {
            at: Box::new(move |r: f32| {
                let rr = r / kr;
                let beta = (1.95 / (TAU * r.max(0.1))).atan() + 0.03;
                let shank = smoothstep(0.24, 0.12, rr);
                let mut c = (0.15 - 0.09 * ((rr - 0.6) / 0.42).powi(2)) * kr;
                c = c * (1.0 - shank) + 0.06 * shank;
                let mut tf = (0.18 - 0.11 * (rr - 0.22) / 0.75) / 0.10;
                tf = tf * (1.0 - shank) + 8.0 * shank;
                if rr > 0.93 {
                    let u = ((rr - 0.93) / 0.035).min(1.0);
                    let q = (1.0 - u * u).max(0.0).sqrt();
                    c *= 0.55 + 0.45 * q;
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
        blade.skin(&mut bl, &blade_m, 0.10 * kr, 0.965 * kr, 36, &|_| 0.0, &|_| 1.0, 14, false);
        bl.transform(0, Mat4::from_translation(PROP_HUB) * Mat4::from_rotation_z(k as f32 * TAU / 3.0) * Mat4::from_translation(-PROP_HUB));
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
    disc.disc(&mt(mat::PROPDISC, 0.1, 0.1, 0.1), PROP_HUB + Vec3::Z * 0.005, Vec3::Z, Vec3::Y, PROP_R, 64);
    let d = b.add("prop disc", G_PROP, disc, spin, PROP_HUB, Vec3::Z, None);
    b.role(d, Role::Prop);
    b.pieces[d].vis = Vis::FastProp;
    b.pieces[d].glass = true;
    b.pieces[d].occluder = false;
}

// ---------------------------------------------------------------- landing gear

/// How the gear folds: legs over the middle of the cycle, the inner main
/// doors opening before and shutting after, the nose doors open whenever the leg is out.
const LEGS: (f32, f32) = (0.15, 0.85);

fn gear(b: &mut Builder) {
    let f = wing_foil();
    // ---- main legs (right, mirrored): the oleo cylinder hangs from the hinge
    // inside the wing, the piston slides in it carrying the axle stub, which
    // reaches inboard to the wheel.
    let fold = |_: f32| Anim::hinge(Drive::new(Input::Gear, MAIN_FOLD).offset(-MAIN_FOLD).smooth(LEGS.0, LEGS.1));
    let x = MAIN_STRUT_X;
    let p = MAIN_PIVOT;
    let mut lg = Mesh::default();
    lg.tube(&PAINT_WHITE, &[Vec3::new(x, p.y + 0.03, p.z), Vec3::new(x, -0.76, p.z)], &|t| 0.034 - 0.005 * t, 16, true);
    // The trunnion across the hinge, and the drag brace forward to the spar.
    lg.tube(&STEEL, &[p - Vec3::Z * 0.09, p + Vec3::Z * 0.09], &|_| 0.022, 12, true);
    lg.tube(&PAINT_WHITE, &[Vec3::new(x, -0.62, p.z), Vec3::new(x - 0.02, -0.40, p.z - 0.25)], &|_| 0.014, 8, true);
    lg.lathe(&GREY, Vec3::new(x, -0.76, p.z), Vec3::Y, &[[0.042, -0.02], [0.042, 0.02], [0.034, 0.03]], 16, false);
    // The strut door: a fairing on the leg's outboard side that closes the
    // slot in the lower skin when the leg is up. It is laid out on the skin
    // as it lies retracted (from the wheel door's edge out past the slot's
    // end, the slot's width plus a lap) and turned back down with the leg.
    let down = Quat::from_rotation_z(MAIN_FOLD);
    let [cx, cz, rr] = MAIN_WELL;
    let [_, sx1, sz0, sz1] = MAIN_SLOT;
    let (dz0, dz1) = (sz0 - 0.006, sz1 + 0.006);
    let door = |a: f32, t: f32, d: f32| -> Vec3 {
        let z = dz0 + (dz1 - dz0) * t;
        let xa = cx + (rr * rr - (z - cz) * (z - cz)).max(0.0).sqrt() + 0.004;
        let xf = xa + (sx1 + 0.006 - xa) * a;
        p + down * (wing_at(&f, xf, z, -1.0) - Vec3::Y * d - p)
    };
    lg.patch(&WHITE, 14, 8, false, |a, t| (door(a, t, -0.001), [a, t], 0.0));
    lg.patch(&PRIMER, 14, 8, true, |a, t| (door(a, t, -0.005), [a, t], 0.0));
    // Two arms from the strut out to the door.
    for a in [0.35f32, 0.85] {
        let q = door(a, 0.5, -0.006);
        lg.tube(&GREY, &[Vec3::new(x + 0.025, q.y, p.z), q], &|_| 0.008, 8, true);
    }
    // The brake line down the leg.
    let line: Vec<Vec3> = (0..=8).map(|k| Vec3::new(x - 0.03, p.y - 0.05 - 0.5 * k as f32 / 8.0, p.z + 0.04)).collect();
    lg.tube(&RUBBER, &line, &|_| 0.0045, 6, false);
    let (ll, lr) = b.pair(["left main leg", "right main leg"], [G_GEAR, G_GEAR], lg, fold, p, Vec3::Z, [None, None]);
    b.role(ll, Role::Gear(1)).role(lr, Role::Gear(2));

    let mut pi = Mesh::default();
    let axle = MAIN_AXLE;
    pi.tube(&CHROME, &[Vec3::new(x, -0.70, p.z), Vec3::new(x, axle.y, p.z)], &|_| 0.026, 14, true);
    pi.rbox(&PAINT_WHITE, Vec3::new(x - 0.02, axle.y, p.z), Vec3::new(0.035, 0.04, 0.045), 0.012, Quat::IDENTITY);
    pi.tube(&STEEL, &[Vec3::new(x - 0.02, axle.y, p.z), Vec3::new(axle.x + 0.05, axle.y, p.z)], &|_| 0.018, 12, true);
    // Torque links behind the strut.
    let (u, l) = (Vec3::new(x, -0.74, p.z + 0.04), Vec3::new(x, axle.y + 0.04, p.z + 0.04));
    let apex = (u + l) * 0.5 + Vec3::Z * 0.07;
    for (a, c) in [(u, apex), (apex, l)] {
        pi.rbox(&GREY, (a + c) * 0.5, Vec3::new(0.012, 0.004, (c - a).length() * 0.5 + 0.008), 0.003, Quat::from_rotation_arc(Vec3::Z, (c - a).normalize()));
        pi.bolt(&STEEL, c + Vec3::X * 0.012, Vec3::X, 0.004);
    }
    // The brake caliper inboard of the leg.
    let cal = axle + Vec3::new(0.10, 0.08, 0.05);
    pi.rbox(&mt(mat::SATIN, 0.08, 0.08, 0.09), cal, Vec3::new(0.016, 0.03, 0.022), 0.006, Quat::from_rotation_x(-0.5));
    pi.bolt(&STEEL, cal + Vec3::new(0.016, 0.012, 0.0), Vec3::X, 0.004);
    let (pl, pr) = b.pair(["left main piston", "right main piston"], [G_GEAR, G_GEAR], pi, |s| Anim::slide(Drive::new(Input::Compress(if s < 0.0 { 1 } else { 2 }), 1.0), Vec3::Y), Vec3::new(x, axle.y, p.z), Vec3::Y, [Some(ll), Some(lr)]);
    b.role(pl, Role::Gear(1)).role(pr, Role::Gear(2));
    let mut wh = Mesh::default();
    wheel(&mut wh, axle, Vec3::X, MAIN_R, 0.085, 0.076, true);
    let (l, r) = b.pair(["left main wheel", "right main wheel"], [G_GEAR, G_GEAR], wh, |s| Anim::hinge(Drive::new(Input::Wheel(if s < 0.0 { 1 } else { 2 }), -1.0)), axle, Vec3::X, [Some(pl), Some(pr)]);
    b.role(l, Role::Gear(1)).role(r, Role::Gear(2));

    // The inner wheel door over the wheel's opening, hinged on its inboard
    // edge: shut with the gear up and down, open while it moves.
    // (Plain paint, no surface pattern: a cut pattern would have the shader
    // cut the door away with the well it covers.)
    let [cx, cz, rr] = MAIN_WELL;
    let lift = |x: f32, z: f32, d: f32| wing_at(&f, x, z, -1.0) - Vec3::Y * d;
    let mut wd = Mesh::default();
    let disc = |a: f32, t: f32| -> (f32, f32) {
        let ang = a * TAU;
        let r = (rr - 0.004) * t;
        (cx + r * ang.cos(), cz + r * ang.sin())
    };
    wd.patch(&WHITE, 40, 6, true, |a, t| { let (x, z) = disc(a, t); (lift(x, z, -0.002), [x, z], 0.0) });
    wd.patch(&PRIMER, 40, 6, false, |a, t| { let (x, z) = disc(a, t); (lift(x, z, -0.008), [x, z], 0.0) });
    let h0 = lift(cx - rr, cz - 0.15, -0.002);
    let h1 = lift(cx - rr, cz + 0.15, -0.002);
    b.pair(["left wheel door", "right wheel door"], [G_GEAR, G_GEAR], wd, |_| Anim::hinge(Drive::new(Input::Gear, -1.45).pulse(0.0, 0.12, 0.88, 1.0)), h0, h1 - h0, [None, None]);

    // ---- nose leg: it folds back into the well under the engine; the fork
    // steers on the strut and slides with the oleo.
    let np = NOSE_PIVOT;
    let ax = NOSE_AXLE;
    let dir = (ax - np).normalize();
    let mut nl = Mesh::default();
    nl.tube(&PAINT_WHITE, &[np + dir * -0.02, np + dir * 0.40], &|t| 0.040 - 0.004 * t, 18, true);
    nl.tube(&STEEL, &[np - Vec3::X * 0.10, np + Vec3::X * 0.10], &|_| 0.02, 12, true);
    nl.rbox(&PAINT_WHITE, np + Vec3::new(0.0, -0.02, 0.0), Vec3::new(0.06, 0.035, 0.032), 0.012, Quat::IDENTITY);
    // The retract brace back to the firewall structure, and the shimmy damper.
    nl.tube(&PAINT_WHITE, &[np + dir * 0.25, np + dir * 0.10 + Vec3::new(0.0, 0.06, 0.30)], &|_| 0.013, 8, true);
    let d0 = np + dir * 0.16 + Vec3::X * 0.06;
    nl.tube(&mt(mat::SATIN, 0.1, 0.1, 0.11), &[d0, d0 + dir * 0.17], &|_| 0.015, 10, true);
    // The taxi light on the strut, facing ahead.
    let tl = np + dir * 0.34 + Vec3::new(0.0, 0.0, -0.06);
    nl.lathe(&GREY, tl + Vec3::Z * 0.035, Vec3::NEG_Z, &[[0.035, 0.0], [0.04, 0.03], [0.0, 0.03]], 18, false);
    nl.disc(&mt(mat::LAMP, 0.95, 0.92, 0.85).x(lamp::LANDING), tl + Vec3::Z * 0.004, Vec3::NEG_Z, Vec3::Y, 0.034, 18);
    let fold_n = Anim::hinge(Drive::new(Input::Gear, FRAC_PI_2).offset(-FRAC_PI_2).smooth(LEGS.0, LEGS.1));
    let leg = b.add("nose leg", G_GEAR, nl, fold_n, np, Vec3::X, None);
    b.role(leg, Role::Gear(0));

    let mut fk = Mesh::default();
    fk.tube(&CHROME, &[np + dir * 0.36, ax - dir * 0.12], &|_| 0.028, 16, true);
    let crown = ax - dir * 0.13;
    fk.rbox(&PAINT_WHITE, crown, Vec3::new(0.09, 0.022, 0.05), 0.012, Quat::IDENTITY);
    for sx in [-1.0f32, 1.0] {
        let a = crown + Vec3::new(sx * 0.078, 0.0, 0.0);
        let c = ax + Vec3::new(sx * 0.078, 0.0, 0.0);
        fk.rbox(&PAINT_WHITE, (a + c) * 0.5, Vec3::new(0.008, (c - a).length() * 0.5 + 0.02, 0.025), 0.006, Quat::from_rotation_arc(Vec3::Y, -(c - a).normalize()));
        fk.bolt(&STEEL, c + Vec3::X * sx * 0.008, Vec3::X * sx, 0.007);
    }
    // Torque links ahead of the strut.
    let (u, l) = (np + dir * 0.33 - Vec3::Z * 0.045, crown - Vec3::Z * 0.045 + Vec3::Y * 0.02);
    let apex = (u + l) * 0.5 - Vec3::Z * 0.06;
    for (a, c) in [(u, apex), (apex, l)] {
        fk.rbox(&GREY, (a + c) * 0.5, Vec3::new(0.012, 0.004, (c - a).length() * 0.5 + 0.008), 0.003, Quat::from_rotation_arc(Vec3::Z, (c - a).normalize()));
        fk.bolt(&STEEL, c + Vec3::X * 0.012, Vec3::X, 0.004);
    }
    let fork = b.add("nose fork", G_GEAR, fk, Anim::turn_slide(Drive::new(Input::Steer, 1.0), Drive::new(Input::Compress(0), 1.0), -dir), ax, dir, Some(leg));
    b.role(fork, Role::Gear(0));
    let mut nw = Mesh::default();
    wheel(&mut nw, ax, Vec3::X, NOSE_R, 0.064, 0.064, false);
    let k = b.add("nose wheel", G_GEAR, nw, Anim::hinge(Drive::new(Input::Wheel(0), -1.0)), ax, Vec3::X, Some(fork));
    b.role(k, Role::Gear(0));

    // The nose doors along the well, hinged on its outer edges: hanging open
    // with the gear down, shut over the wheel when it is up.
    let [z0, z1, hw] = NOSE_WELL;
    let mut dr = Mesh::default();
    let at = |a: f32, c: f32, lift: f32| -> SP {
        let z = z0 + 0.008 + (z1 - z0 - 0.016) * a;
        let x = 0.003 + (hw - 0.006) * c;
        (Vec3::new(x, fuse_bottom_y(z, x) + lift, z), [z, x], 0.0)
    };
    dr.patch(&WHITE, 16, 6, true, |a, c| at(a, c, -0.002));
    dr.patch(&PRIMER, 16, 6, false, |a, c| at(a, c, 0.004));
    let h0 = Vec3::new(hw, fuse_bottom_y(z0, hw), z0);
    let h1 = Vec3::new(hw, fuse_bottom_y(z1, hw), z1);
    b.pair(["left nose door", "right nose door"], [G_ENGINE, G_ENGINE], dr, |_| Anim::hinge(Drive::new(Input::Gear, 1.45).smooth(0.0, LEGS.0 + 0.05)), h0, h1 - h0, [None, None]);
}

// ---------------------------------------------------------------- glass

/// The windshield and the side windows: a sheet over the skin wherever
/// `window` opens it.
fn glass(b: &mut Builder) {
    let (z0, z1) = (-1.12f32, 2.25f32);
    let nz = 170;
    let nt = 70;
    let rows: Vec<Vec<_>> = (0..=nz)
        .map(|i| {
            let z = z0 + (z1 - z0) * i as f32 / nz as f32;
            (0..=nt).map(|k| { let p = fuse_pt(z, k as f32 / nt as f32 * (FRAC_PI_2 + 0.35)); (p, [z, p.y], 0.0) }).collect()
        })
        .collect();
    let mut g = Mesh::default();
    g.grid(&mt(mat::GLASS, 0.0, 0.0, 0.0), &rows, false, false);
    let keep: Vec<bool> = g.v.iter().map(|v| window(Vec3::from(v.pos)) < 0.02).collect();
    let idx: Vec<u32> = g.i.chunks(3).filter(|t| t.iter().any(|&i| keep[i as usize])).flatten().copied().collect();
    g.i = idx;
    let l = g.mirror_x();
    g.append(&l);
    let k = b.fixed("windows", G_FUSE, g);
    b.pieces[k].glass = true;
    b.pieces[k].occluder = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The model's size against the published figures ([S], [T]).
    #[test]
    fn dimensions_are_the_a36s() {
        let model = build();
        let (c, h) = model.bounds();
        let (lo, hi) = (c - h, c + h);
        // Span 33 ft 5.5 in.
        assert!((hi.x - lo.x - 10.20).abs() < 0.08, "span {}", hi.x - lo.x);
        // Length 27 ft 5.6 in, spinner tip to the rudder's trailing edge.
        assert!((hi.z - lo.z - 8.37).abs() < 0.12, "length {}", hi.z - lo.z);
        // Track 9 ft 6.7 in.
        assert!((2.0 * MAIN_AXLE.x - 2.913).abs() < 0.01);
        // Height to the fin (sat on its wheels, nose up): 8 ft 5 in.
        let slope = (NOSE_GROUND - MAIN_GROUND) / (NOSE_AXLE.z - MAIN_AXLE.z);
        let ground = |z: f32| MAIN_GROUND + slope * (z - MAIN_AXLE.z);
        let top = model.pieces.iter().filter(|p| !p.glass).flat_map(|p| p.mesh.v.iter()).map(|v| Vec3::from(v.pos)).map(|p| p.y - ground(p.z)).fold(f32::MIN, f32::max);
        assert!((top * slope.atan().cos() - 2.59).abs() < 0.06, "height {top}");
        // Propeller clearance over the ground: 9.5 in with the tyre and strut flat, more at rest.
        let clear = (PROP_HUB.y - PROP_R) - ground(PROP_HUB.z);
        assert!(clear > 0.24 && clear < 0.36, "prop clearance {clear}");
    }

    #[test]
    fn windows_and_wells_do_not_meet() {
        // The wells are in the bottom; the windows high on the sides.
        for z in [-2.0f32, -1.5, 0.4] {
            assert!(window(Vec3::new(0.1, -0.5, z)) > 0.0);
        }
        assert!(well(Vec3::new(MAIN_WELL[0], -0.45, MAIN_WELL[1])) < 0.0);
        assert!(well(Vec3::new(0.0, -0.6, -1.8)) < 0.0);
        assert!(window(Vec3::new(0.0, 0.6, -0.7)) < 0.0, "windshield");
        assert!(window(Vec3::new(0.55, 0.45, 0.7)) < 0.0, "second window");
    }
}
