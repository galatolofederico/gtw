//! Gulfstream G650 (GVI, the 2012 baseline; not the G650ER): the
//! ultra-long-range business jet, two Rolls-Royce BR725 A1-12 turbofans on the
//! rear fuselage, a T-tail, a 36-degree swept wing with blended winglets,
//! fly-by-wire controls flown with conventional yokes, Honeywell PlaneView II.
//!
//! Like Concorde, the airframe is modelled in Blender (`assets/blender/g650.py`,
//! which also saves `g650.blend`) and exported to `assets/g650.bin.gz`: one
//! mesh per piece, with the hinge each one turns about. How the pieces move,
//! what they belong to and what they are to the damage model is set here, by
//! name. Rebuild the asset with
//!
//!   blender -b --factory-startup --python assets/blender/g650.py -- assets
//!
//! and its paint (a fictional white scheme, see `assets/blender/g650_paint.py`) with
//!
//!   blender -b --factory-startup --python assets/blender/g650.py -- <scratch> --guides assets/blender/g650_paint
//!   python assets/blender/g650_paint.py
//!
//! and its start-screen picture (`assets/aircraft/g650.png`, painted) from the
//! saved model with
//!
//!   blender -b assets/blender/g650.blend --python assets/blender/g650_still.py -- assets/aircraft/g650.png
//!
//! The flight deck inside is built here in Rust (`deck.rs`).
//!
//! # Sources and figures
//!
//! - EASA type-certificate data sheet EASA.IM.A.169 (Gulfstream GVI), issue 17,
//!   https://www.easa.europa.eu/en/downloads/7278/en : span 30.36 m (99.62 ft),
//!   length 30.41 m (99.78 ft), fuselage 2.74 m (9.00 ft) wide at its constant
//!   section; BR700-725A1-12, 75.2 kN (16,900 lbf); MTOW 45,177 kg, MLW
//!   37,874 kg, MZFW 27,442 kg; fuel 20,048 kg (44,200 lb); VMO/MMO 340 KCAS /
//!   0.925; 51,000 ft; MAC 4.756 m (187.24 in); weight-and-balance datum 100 in
//!   ahead of the radome; tyres (its section III.21): nose twin 21x7.25-10
//!   (bias ply) and main twin H37.5x12.0R19, both at a nominal 216 psi (14.9 bar).
//! - Height 7.82 m (25 ft 8 in), cabin 2.59 m wide x 1.91-1.96 m high, 14.27 m
//!   long, 16 elliptical windows 28 in (0.71 m) wide, 36 deg wing sweep, 119.2 m²
//!   (1,283 ft²) wing: Gulfstream figures as quoted at
//!   https://en.wikipedia.org/wiki/Gulfstream_G650 (the area is quoted for the
//!   G700's "same wing").
//! - Measured (estimates): profiles, glazing, windows, door, fin and nacelles
//!   from a left-side photograph of N47TR (Wikimedia Commons, "AvWest (N47TR)
//!   Gulfstream Aerospace G-VI (G650) taxiing at Wagga Wagga Airport.jpg") scaled by
//!   its 28 in windows and 21 in nose tyres (55-57 px/m, agreeing to 4%), and
//!   the public preview of the-blueprints.com's G650 three-view
//!   (https://the-blueprints.com/vectordrawings/show/1510/gulfstream_g650).
//!   No airport-planning document is public: the gear positions (wheelbase
//!   13.15 m, main track 3.80 m between the twin pairs' middles) are estimates
//!   from that drawing and the photograph, kept ahead of the inboard flaps.
//!
//! The overall 30.41 m runs from the radome to the tips of the swept T-tail's
//! stabiliser, which trail 3.4 m behind the tail cone (27.0 m). The wing is a
//! reference trapezoid fitted to the 119.2 m², the 28.55 m span at the tips
//! (the winglets add the rest) and the 4.756 m MAC: 7.0 m chord on the
//! centreline, 1.35 m at the tip, the leading edge swept 39.6 degrees (36 at the
//! quarter chord); its MAC (4.81 m) lies 5.53 m out, its leading edge 12.90 m
//! behind the nose. The reference centre of gravity, the origin, is put at 30%
//! of it: 14.35 m behind the radome's tip, 2.55 m above the ground at rest.
//!
//! # The flight model
//!
//! Sources, besides the TCDS above:
//! - EASA TCDS E.018 (BR700-710/-725 engines), issue 16,
//!   https://www.easa.europa.eu/en/downloads/7767/en : 100% N1 = 7,000 rpm,
//!   reverse thrust at no more than 78.1% N1.
//! - ICAO aircraft engine emissions databank v32B,
//!   https://www.easa.europa.eu/en/downloads/131424/en , UID 01P11BR016: bypass
//!   ratio 4.35; fuel flow per engine 0.789 kg/s at take-off, 0.650 climbing
//!   out, 0.221 on approach, 0.085 idling.
//! - NTSB AAR-12/02 (the 2011 Roswell accident),
//!   https://www.ntsb.gov/investigations/AccidentReports/Reports/AAR1202.pdf :
//!   flaps 20 the usual take-off setting, 10 for hot and high; free-air stall
//!   at 14.7 deg angle of attack with flaps 10 and 14.2 with flaps 20, some 3
//!   degrees less in ground effect; V2 = 1.13 VSR; the outboard wing stalls first.
//! - EASA operational suitability data (flight crew) GVI, 15 Dec 2014,
//!   https://www.easa.europa.eu/sites/default/files/dfu/20141215%20OSD%20FC%20GVI.pdf ,
//!   and the FAA FSB report (flaps 39 its landing flap),
//!   https://www.faa.gov/aircraft/draft_docs/fsb/FSBR_GVI_Rev_11_Draft.pdf :
//!   angle-of-attack and VMO protection; high induced drag below about
//!   Mach 0.83 near its ceiling; engines slow to spool up at height.
//! - Aviation Week's overview,
//!   https://aviationweek.com/business-aviation/aircraft-overview/aircraft-overview-gulfstream-g650-g650er :
//!   basic operating weight 54,000 lb (24,494 kg), payload 6,500 lb (1,800 lb
//!   with full tanks), 5,858 ft (1,786 m) balanced field at MTOW, sea level,
//!   ISA; 7,000 nmi at Mach 0.85, 6,000 at Mach 0.90; initial cruise FL410.
//! - VFE 250/220/190 KCAS at flaps 10/20/39, VLE 250 KCAS, VMO 300 KCAS below
//!   8,000 ft, from an AFM-based model guide,
//!   https://sprinkle.com/aircraft/manuals/gulfstream-10/gulfstream-g650-940/model-guide/gulfstream-g650-model-guide-102475 .
//! - Landing distance 2,680 ft (817 m) from 50 ft at MLW, sea level, ISA:
//!   https://www.jetcraft.com/jetstream/2016/06/gulfstream-g650-overview-2012-present/ ;
//!   480 US gal/h (1,460 kg/h) average cruise burn, cruise altitude in as
//!   little as 22 min: https://www.bjtonline.com/business-jet-news/gulfstream-g650-a-ramp-king ;
//!   Vref 120 KIAS at 65,500 lb (a flight evaluation quoted second-hand: unverified).
//!
//! Estimates (no public figures; the AFM has them): the CG range, inertia,
//! the drag polar (zero-lift drag from a wetted area of about 560 m², the
//! lift-dependent drag from its aspect ratio of 7.7 with winglets), the lift
//! curve (fitted to the NTSB's stall angles and to Vref), the flaps' lift and
//! drag, idle N1 and thrust, spool rates, reverser effect, thrust lapse (fitted
//! to the climb and ceiling), oleo springs, dampers, brakes, and handling.
//!
//! How it flies (`cargo test g650` in plane.rs, ISA, still air):
//! - FL450, Mach 0.85, 35.3 t: N1 89%, alpha 2.9 deg, 1,294 kg/h (2,850
//!   lb/h); all its fuel at that rate would take it 7,560 nmi. Mach 0.90:
//!   1,388 kg/h. Flat out at FL410 it settles at Mach 0.926.
//! - Climb left at Mach 0.85: 650 fpm at FL410 at 44 t, 450 fpm at FL510 at
//!   33 t, none at FL510 at MTOW. From brake release at MTOW to FL410 (250 kt,
//!   300 kt, Mach 0.80) in 22 min, burning 955 kg.
//! - Stall at MLW 109 kt at flaps 39 (Vref 134 kt), 123 kt clean; Vref at
//!   65,500 lb 119 kt; V2 at MTOW, flaps 20 about 142 kt.
//! - Take-off at MTOW, flaps 20, rotating at 138 kt: off after about 1,170 m
//!   at 151 kt. Approach at MLW, flaps 39, Vref + 5: alpha 5.8 deg, 3.1 deg
//!   nose up, N1 57%. Landing from 50 ft at MLW: 270 m to the runway, 480 m to
//!   a stop with brakes, reversers and ground spoilers (750 m against the
//!   book's 817 m).
//!
//! Not modelled (the shared flight model's limits): the angle-of-attack
//! limiter and VMO overspeed protection (the yoke only asks for no more than
//! 2.5 g), the stall coming earlier in ground effect, the lift slope and stall
//! angle changing with Mach, wave drag growing with lift, a separate climb or
//! cruise thrust rating, engines spooling slower at height, the 300 kt VMO
//! below 8,000 ft, reverse thrust pitching the nose up before the nose wheel
//! is down, and one engine failing alone. Fuel flow follows thrust linearly
//! from idle to take-off: right in the cruise, but the climb burns about a
//! quarter too little and Mach 0.90 costs only 7% more an hour against the
//! real one's 15-20%.
//!
//! Body frame as everywhere: +X right, +Y up, -Z forward, origin at the
//! centre of gravity. Drawing positions are given as `d` (m behind the radome's
//! tip) and `h` (m above the ground, at rest): `bz(d)`, `by(h)`.
use super::*;
use crate::airframe::*;
use glam::Vec2;
use std::collections::HashMap;

pub mod deck;

const KT: f64 = 1.0 / 1.94384;

// ---------------------------------------------------------------- the frame (in step with g650.py)

/// The reference centre of gravity behind the radome's tip, and its height
/// above the ground with the aircraft at rest (m).
pub const CG_D: f32 = 14.35;
pub const CG_H: f32 = 2.55;
/// Body z and y of a drawing station `d` (m behind the radome's tip) and height `h` (m above the ground).
pub const fn bz(d: f32) -> f32 {
    d - CG_D
}
pub const fn by(h: f32) -> f32 {
    h - CG_H
}

// ---------------------------------------------------------------- the nose and the flight deck (for deck.rs)

/// The radome's tip (body z).
#[allow(dead_code)] // (for the flight deck, deck.rs)
pub const NOSE_Z: f32 = bz(0.0);
/// The flight deck's aft bulkhead, where the nose section ends and the cabin
/// (with the main door behind it) begins (body z).
pub const DECK_Z: f32 = bz(4.40);
/// The flight deck's floor (body y): just above the nose wheel well's roof (h 2.16).
pub const FLOOR_Y: f32 = by(2.20);
/// The captain's design eye: the left seat, 0.52 m left of the centreline,
/// 3.00 m behind the radome's tip, 3.33 m above the ground (estimates from
/// the photograph: the pilot's head sits a third of the way along the side
/// window; 13 degrees of view down over the nose past the windshield's sill).
pub const EYE: Vec3 = Vec3::new(-0.52, by(3.33), bz(3.00));

/// The fuselage's sections (g650.py `FUSE_DH`): station d, half width, top,
/// height of the widest point, bottom (h), superellipse exponents of the
/// upper and lower half. A smooth curve (`hermite`) runs through them.
const FUSE: [[f32; 7]; 32] = [
    [0.00, 0.000, 2.080, 2.080, 2.080, 2.0, 2.0],
    [0.06, 0.120, 2.170, 2.070, 1.980, 2.0, 2.0],
    [0.20, 0.240, 2.270, 2.060, 1.860, 2.0, 2.0],
    [0.40, 0.360, 2.390, 2.050, 1.750, 2.0, 2.0],
    [0.70, 0.500, 2.540, 2.060, 1.660, 2.0, 2.0],
    [1.00, 0.620, 2.680, 2.100, 1.570, 2.0, 2.0],
    [1.40, 0.770, 2.870, 2.170, 1.525, 2.0, 2.0],
    [1.85, 0.900, 3.080, 2.260, 1.505, 2.0, 2.0],
    [2.30, 1.020, 3.420, 2.360, 1.490, 2.0, 2.0],
    [2.80, 1.130, 3.700, 2.460, 1.460, 2.0, 2.0],
    [3.30, 1.220, 3.870, 2.550, 1.440, 2.0, 2.0],
    [3.80, 1.290, 3.990, 2.620, 1.420, 2.0, 2.0],
    [4.40, 1.340, 4.070, 2.680, 1.400, 2.0, 2.0],
    [5.20, 1.365, 4.120, 2.720, 1.385, 2.0, 2.0],
    [6.00, 1.370, 4.140, 2.750, 1.380, 2.0, 2.0],
    [8.40, 1.370, 4.140, 2.760, 1.380, 2.0, 2.0],
    [9.40, 1.370, 4.140, 2.760, 1.300, 2.0, 2.6],
    [10.4, 1.370, 4.140, 2.760, 1.200, 2.0, 3.2],
    [11.5, 1.370, 4.140, 2.760, 1.150, 2.0, 3.6],
    [16.4, 1.370, 4.140, 2.760, 1.150, 2.0, 3.6],
    [17.4, 1.370, 4.140, 2.770, 1.200, 2.0, 3.0],
    [18.4, 1.360, 4.140, 2.790, 1.330, 2.0, 2.4],
    [19.4, 1.335, 4.130, 2.830, 1.500, 2.0, 2.0],
    [20.4, 1.290, 4.110, 2.890, 1.660, 2.0, 2.0],
    [21.4, 1.230, 4.080, 2.970, 1.840, 2.0, 2.0],
    [22.4, 1.140, 4.040, 3.070, 2.050, 2.0, 2.0],
    [23.4, 1.020, 3.990, 3.170, 2.280, 2.0, 2.0],
    [24.4, 0.870, 3.930, 3.250, 2.540, 2.0, 2.0],
    [25.4, 0.680, 3.850, 3.320, 2.820, 2.0, 2.0],
    [26.3, 0.450, 3.750, 3.380, 3.080, 2.0, 2.0],
    [26.8, 0.260, 3.660, 3.420, 3.220, 2.0, 2.0],
    [27.0, 0.150, 3.600, 3.430, 3.280, 2.0, 2.0],
];

/// Column `col` of a station table at x: a cubic through the stations with
/// finite-difference slopes (g650.py `hermite`, which shapes the skin).
fn hermite<const N: usize, const C: usize>(t: &[[f32; C]; N], x: f32, col: usize) -> f32 {
    let x = x.clamp(t[0][0], t[N - 1][0]);
    for i in 0..N - 1 {
        let (x0, x1) = (t[i][0], t[i + 1][0]);
        if x0 <= x && x <= x1 {
            let (v0, v1) = (t[i][col], t[i + 1][col]);
            let (a, d) = (t[i.saturating_sub(1)], t[(i + 2).min(N - 1)]);
            let m0 = (v1 - a[col]) / (x1 - a[0]);
            let m1 = (d[col] - v0) / (d[0] - x0);
            let h = x1 - x0;
            let u = (x - x0) / h;
            let (u2, u3) = (u * u, u * u * u);
            return (2.0 * u3 - 3.0 * u2 + 1.0) * v0 + (u3 - 2.0 * u2 + u) * h * m0 + (-2.0 * u3 + 3.0 * u2) * v1 + (u3 - u2) * h * m1;
        }
    }
    t[N - 1][col]
}

/// The outer skin's section at body station z: half width, top, the height
/// of its widest point and bottom (body y), and the superellipse exponents of
/// its upper and lower halves (2: an ellipse). Outside the fuselage (ahead of
/// the radome, behind the tail cone) it is the end section.
#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub hw: f32,
    pub top: f32,
    pub mid: f32,
    pub bot: f32,
    pub p_up: f32,
    pub p_lo: f32,
}

pub fn section(z: f32) -> Section {
    let d = z + CG_D;
    let c = |k| hermite(&FUSE, d, k);
    Section { hw: c(1).max(0.0), top: by(c(2)), mid: by(c(3)), bot: by(c(4)), p_up: c(5), p_lo: c(6) }
}

/// A point of the skin at station z, at angle `a` round the section (0 on the
/// right at its widest, a quarter turn at the crown, as g650.py's rings),
/// moved `inset` metres in from it (roughly along its normal): the inner
/// face of the skin for a lining is `skin_point(z, a, 0.05)` or so.
#[allow(dead_code)] // (for the flight deck, deck.rs)
pub fn skin_point(z: f32, a: f32, inset: f32) -> Vec3 {
    let s = section(z);
    let (c, sn) = (a.cos(), a.sin());
    let p = if sn >= 0.0 { s.p_up } else { s.p_lo };
    let h = if sn >= 0.0 { s.top - s.mid } else { s.mid - s.bot };
    let e = 2.0 / p;
    Vec3::new((s.hw - inset).max(0.0) * c.signum() * c.abs().powf(e), s.mid + (h - inset).max(0.0) * sn.signum() * sn.abs().powf(e), z)
}

/// Half width of the skin at station z, height y (0 above or below it).
pub fn skin_half_width(z: f32, y: f32) -> f32 {
    let s = section(z);
    let (h, p) = if y >= s.mid { (s.top - s.mid, s.p_up) } else { (s.mid - s.bot, s.p_lo) };
    let f = ((y - s.mid).abs() / h.max(1e-6)).min(1.0);
    s.hw * (1.0 - f.powf(p)).max(0.0).powf(1.0 / p)
}

/// Height of the skin's upper (or lower) surface at station z, x out.
pub fn skin_height(z: f32, x: f32, upper: bool) -> f32 {
    let s = section(z);
    let p = if upper { s.p_up } else { s.p_lo };
    let f = (1.0 - (x.abs().min(s.hw) / s.hw.max(1e-6)).powf(p)).max(0.0).powf(1.0 / p);
    if upper { s.mid + (s.top - s.mid) * f } else { s.mid - (s.mid - s.bot) * f }
}

/// How far inside the skin p is (m, about; negative outside, and anywhere ahead of or behind the fuselage).
pub fn skin_depth(p: Vec3) -> f32 {
    let d = p.z + CG_D;
    if !(0.0..27.0).contains(&d) {
        return -1.0;
    }
    (p.y - skin_height(p.z, p.x, false)).min(skin_height(p.z, p.x, true) - p.y).min(skin_half_width(p.z, p.y) - p.x.abs())
}

/// The point of the skin straight out from the fuselage's axis through p (g650.py `to_skin`).
pub fn to_skin(p: Vec3) -> Vec3 {
    let c = Vec3::new(0.0, section(p.z).mid, p.z);
    let r = Vec3::new(p.x, p.y - c.y, 0.0).try_normalize().unwrap_or(Vec3::Y);
    let (mut lo, mut hi) = (0.0f32, 3.0f32);
    for _ in 0..40 {
        let m = 0.5 * (lo + hi);
        if skin_depth(c + r * m) > 0.0 { lo = m } else { hi = m }
    }
    c + r * lo
}

/// The flight deck's glazing (g650.py, the same numbers), in drawing terms on
/// the right side; the left is its mirror image. Each windshield pane runs
/// from the centre post (POST either side of the centreline; its sill meets
/// it at WS_SILL_CENTRE) round to the A-post along h = WS_SILL, up the A-post
/// and back across under the crown to the post at WS_TOP_CENTRE. The side
/// window behind it: its corners (d, h).
pub const WS_SILL: f32 = 3.08;
pub const GLAZE_TOP: f32 = 3.54;
pub const POST: f32 = 0.035;
pub const A_POST: [[f32; 2]; 2] = [[2.52, 3.50], [2.62, WS_SILL]];
pub const WS_TOP_CENTRE: f32 = 2.60;
pub const WS_SILL_CENTRE: f32 = 1.88;
pub const SIDE_WINDOW: [[f32; 2]; 4] = [[2.70, WS_SILL], [3.30, WS_SILL], [3.74, GLAZE_TOP], [2.59, GLAZE_TOP]];

fn lerpf(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
/// The skin's point at drawing station d, height h, on side s (+1 right).
fn side_pt(d: f32, h: f32, s: f32) -> Vec3 {
    let (z, y) = (bz(d), by(h));
    Vec3::new(s * skin_half_width(z, y), y, z)
}
/// The skin's upper surface at station d, x out.
fn top_pt(d: f32, x: f32) -> Vec3 {
    let z = bz(d);
    Vec3::new(x, skin_height(z, x, true), z)
}
/// A closed outline through these points of the skin, every `step` metres, on it.
fn outline(pts: &[Vec3], step: f32) -> Vec<Vec3> {
    let mut out = Vec::new();
    for (i, &a) in pts.iter().enumerate() {
        let b = pts[(i + 1) % pts.len()];
        let n = ((b - a).length() / step).round().max(1.0) as usize;
        for k in 0..n {
            out.push(to_skin(a.lerp(b, k as f32 / n as f32)));
        }
    }
    out
}

/// The opening of the windshield pane on side `s` (+1 right, -1 left): a
/// closed outline on the skin, body frame, a point every 3 cm. Its glass (the
/// "windshield" piece) lies 6 mm inside it.
pub fn windshield_outline(s: f32) -> Vec<Vec3> {
    let mut p = vec![top_pt(WS_SILL_CENTRE, s * POST)];
    p.extend((0..7).map(|k| side_pt(lerpf(WS_SILL_CENTRE + 0.06, A_POST[1][0], k as f32 / 6.0), WS_SILL, s)));
    p.push(side_pt(A_POST[0][0], A_POST[0][1], s));
    let xa = p.last().unwrap().x.abs();
    p.extend((1..5).map(|k| top_pt(lerpf(A_POST[0][0], WS_TOP_CENTRE, k as f32 / 4.0), s * lerpf(xa, POST, k as f32 / 4.0))));
    outline(&p, 0.03)
}

/// The opening of the side window on side `s`, as `windshield_outline`.
#[allow(dead_code)] // (for the flight deck, deck.rs)
pub fn side_window_outline(s: f32) -> Vec<Vec3> {
    let p: Vec<Vec3> = SIDE_WINDOW.iter().map(|&[d, h]| side_pt(d, h, s)).collect();
    outline(&p, 0.03)
}

/// Is this point of the skin (or near it) in one of the flight deck's window
/// openings? The side windows are told in side view (d, h), the windshield
/// panes in plan view (d, |x|).
#[allow(dead_code)] // (deck.rs's tests check the flight deck's openings against it)
pub fn in_glazing(p: Vec3) -> bool {
    let (d, h) = (p.z + CG_D, p.y + CG_H);
    let side: Vec<Vec2> = SIDE_WINDOW.iter().map(|&[a, b]| Vec2::new(a, b)).collect();
    if sd_poly(Vec2::new(d, h), &side) < 0.0 {
        return true;
    }
    let ws: Vec<Vec2> = windshield_outline(1.0).iter().map(|q| Vec2::new(q.z + CG_D, q.x)).collect();
    sd_poly(Vec2::new(d, p.x.abs()), &ws) < 0.0
}

// ---------------------------------------------------------------- the wing (in step with g650.py)

/// The wing's tip, where the winglet takes over (span station, m); the
/// winglet's tip is 15.18 m out.
pub const W_X1: f32 = 14.27;
const W_LE0: f32 = 8.33;
const W_TAN: f32 = 0.826;
const W_TE0: f32 = 15.33;
const W_TE_TAN: f32 = 0.430;
const W_KINK: f32 = 4.6;
const W_TE_IN: f32 = 0.14;
const W_H0: f32 = 1.66;
/// tan 4.5 degrees of dihedral
const W_DIH: f32 = 0.0787;

fn wing_le(x: f32) -> f32 {
    W_LE0 + W_TAN * x
}
fn wing_te(x: f32) -> f32 {
    if x >= W_KINK { W_TE0 + W_TE_TAN * x } else { W_TE0 + W_TE_TAN * W_KINK - W_TE_IN * (W_KINK - x) }
}

pub fn wing_sect(x: f32) -> Sect {
    let x = x.clamp(0.0, W_X1);
    Sect { le: Vec3::new(x, by(W_H0 + W_DIH * x), bz(wing_le(x))), c: wing_te(x) - wing_le(x), cdir: Vec3::Z, up: Vec3::new(-W_DIH, 1.0, 0.0).normalize(), tf: 1.0 }
}

fn fuse(z: f32) -> (f32, f32, f32) {
    let s = section(z);
    (s.top, s.mid, s.bot)
}

// ---------------------------------------------------------------- the model

/// Every piece: name (as in the .blend), breakable group, what it is to the
/// damage model, and the piece it hangs from (listed before it). The engines
/// hang from the rear fuselage: they go with it.
const PIECES: &[(&str, usize, Role, Option<&str>)] = &[
    ("radome", G_PROP, Role::Other, None),
    ("nose section", G_ENGINE, Role::Other, None),
    ("windshield", G_ENGINE, Role::Other, None),
    ("windshield frame", G_ENGINE, Role::Other, None),
    ("fuselage", G_FUSE, Role::Other, None),
    ("entry door", G_FUSE, Role::Other, None),
    ("cabin windows", G_FUSE, Role::Other, None),
    ("tail cone", G_TAIL, Role::TailCone, None),
    ("tail light", G_TAIL, Role::Other, None),
    ("fin", G_TAIL, Role::Stab, None),
    ("rudder", G_TAIL, Role::Rudder, None),
    ("stabiliser", G_TAIL, Role::Stab, None),
    ("left elevator", G_TAIL, Role::Elevator, None),
    ("right elevator", G_TAIL, Role::Elevator, None),
    ("left wing", G_LWING, Role::Other, None),
    ("right wing", G_RWING, Role::Other, None),
    ("left inboard flap", G_LWING, Role::Flap(0), None),
    ("right inboard flap", G_RWING, Role::Flap(1), None),
    ("left outboard flap", G_LWING, Role::Flap(0), None),
    ("right outboard flap", G_RWING, Role::Flap(1), None),
    ("left aileron", G_LWING, Role::Aileron(0), None),
    ("right aileron", G_RWING, Role::Aileron(1), None),
    ("left ground spoiler", G_LWING, Role::Other, None),
    ("right ground spoiler", G_RWING, Role::Other, None),
    ("left spoiler 1", G_LWING, Role::Other, None),
    ("right spoiler 1", G_RWING, Role::Other, None),
    ("left spoiler 2", G_LWING, Role::Other, None),
    ("right spoiler 2", G_RWING, Role::Other, None),
    ("left spoiler 3", G_LWING, Role::Other, None),
    ("right spoiler 3", G_RWING, Role::Other, None),
    ("left nav light", G_LWING, Role::Other, None),
    ("right nav light", G_RWING, Role::Other, None),
    ("left landing light", G_LWING, Role::Other, None),
    ("right landing light", G_RWING, Role::Other, None),
    ("left nacelle", G_FUSE, Role::Other, None),
    ("right nacelle", G_FUSE, Role::Other, None),
    ("left reverser", G_FUSE, Role::Other, Some("left nacelle")),
    ("right reverser", G_FUSE, Role::Other, Some("right nacelle")),
    ("nose leg", G_GEAR, Role::Gear(0), None),
    ("nose fork", G_GEAR, Role::Gear(0), Some("nose leg")),
    ("nose wheel", G_GEAR, Role::Gear(0), Some("nose fork")),
    ("left main leg", G_GEAR, Role::Gear(1), None),
    ("left main oleo", G_GEAR, Role::Gear(1), Some("left main leg")),
    ("left main wheel", G_GEAR, Role::Gear(1), Some("left main oleo")),
    ("right main leg", G_GEAR, Role::Gear(2), None),
    ("right main oleo", G_GEAR, Role::Gear(2), Some("right main leg")),
    ("right main wheel", G_GEAR, Role::Gear(2), Some("right main oleo")),
    ("left nose gear door", G_ENGINE, Role::Other, None),
    ("right nose gear door", G_ENGINE, Role::Other, None),
    ("left aft nose gear door", G_ENGINE, Role::Other, None),
    ("right aft nose gear door", G_ENGINE, Role::Other, None),
    ("left main gear door", G_FUSE, Role::Other, None),
    ("right main gear door", G_FUSE, Role::Other, None),
];

/// How a piece moves. Hinges point so that a positive angle puts a trailing
/// edge down (the rudder: to the right; the spoilers: up); the gear is
/// modelled down and locked with its oleos extended and its doors open, the
/// flaps, spoilers and reversers stowed, the entry door shut. A piece's
/// `angle` (from the .blend) is how far it turns to stow (the legs, the
/// doors), to full flap (39 degrees: the lever's notches 0/10/20/39 are
/// fractions of it), to full spoiler, to open the entry door (128 degrees,
/// onto the ground), or how far it slides (the reversers' sleeves, 0.45 m aft).
fn anim(name: &str, stow: f32, axis: Vec3) -> Anim {
    let left = name.starts_with("left");
    let s = if left { -1.0 } else { 1.0 };
    let leg = if name.starts_with("nose") { 0 } else if left { 1 } else { 2 };
    if name == "entry door" {
        // the airstair: worked by the canopy key
        Anim::hinge(Drive::new(Input::Canopy, stow))
    } else if name.ends_with(" flap") {
        Anim::hinge(Drive::new(Input::Flaps, stow))
    } else if name.ends_with("aileron") {
        // about 20 degrees either way (estimate)
        Anim::hinge(Drive::new(Input::Aileron(s), 0.35))
    } else if name.contains("spoiler") {
        Anim::hinge(Drive::new(Input::Spoiler(s), stow))
    } else if name.ends_with("elevator") {
        Anim::hinge(Drive::new(Input::Elevator, 0.40))
    } else if name == "rudder" {
        Anim::hinge(Drive::new(Input::Rudder, 0.45))
    } else if name.ends_with("reverser") {
        Anim::slide(Drive::new(Input::Reverser, stow), axis)
    } else if name.ends_with(" leg") {
        Anim::hinge(Drive::new(Input::Gear, -stow).smooth(0.15, 0.85).offset(stow))
    } else if name.ends_with("main gear door") {
        // open for the wheels to pass, shut again behind them either way
        Anim::hinge(Drive::new(Input::Gear, -stow).pulse(0.0, 0.18, 0.88, 1.0).offset(stow))
    } else if name.ends_with("aft nose gear door") {
        // either side of the strut where it leaves the well: open before the
        // leg comes down, and open as long as it is down; shut once it is up
        Anim::hinge(Drive::new(Input::Gear, -stow).smooth(0.0, 0.15).offset(stow))
    } else if name.ends_with("nose gear door") {
        // the well's forward doors: open for the wheels to pass, shut again
        // behind them either way, as Gulfstreams' are ("doors open, uplocks
        // release, gear extends, doors close")
        Anim::hinge(Drive::new(Input::Gear, -stow).pulse(0.0, 0.15, 0.88, 1.0).offset(stow))
    } else if name == "nose fork" {
        Anim::turn_slide(Drive::new(Input::Steer, 1.0), Drive::new(Input::Compress(0), 1.0), Vec3::Y)
    } else if name == "nose wheel" {
        Anim::hinge(Drive::new(Input::Wheel(0), -1.0))
    } else if name.ends_with("main oleo") {
        Anim::slide(Drive::new(Input::Compress(leg), 1.0), Vec3::Y)
    } else if name.ends_with("main wheel") {
        Anim::hinge(Drive::new(Input::Wheel(leg), -1.0))
    } else {
        Anim::Fixed
    }
}

pub fn build() -> Model {
    let objs = fa18::load(include_bytes!("../../../assets/g650.bin.gz"), "g650");
    let mut b = Builder::default();
    let mut at: HashMap<&str, usize> = HashMap::new();
    for &(name, group, role, parent) in PIECES {
        let o = objs.get(name).unwrap_or_else(|| panic!("g650 model has no piece called {name:?}"));
        let k = b.add(name, group, o.mesh.clone(), anim(name, o.angle, o.axis), o.pivot, o.axis, parent.map(|p| at[p]));
        b.role(k, role);
        if name == "windshield" || name == "cabin windows" {
            b.pieces[k].glass = true;
        }
        at.insert(name, k);
    }
    bake_ao(&mut b.pieces, &AoBake { vox: 0.08, reach: 3.0, rays: 24, open: &|_, _| false });
    // The flight deck gets its own, finer bake, shut in by the nose section's
    // skin; the light comes in through the openings.
    let mut c = Builder::default();
    deck::build(&mut c);
    let n_deck = c.pieces.len();
    if n_deck > 0 {
        for name in ["nose section", "windshield frame"] {
            if let Some(k) = b.pieces.iter().position(|p| p.name == name) {
                let mut skin = b.pieces[k].mesh.clone();
                skin.i = skin.i.chunks(3).filter(|t| t.iter().all(|&i| { let z = skin.v[i as usize].pos[2]; z > bz(0.85) && z < DECK_Z + 0.05 })).flatten().copied().collect();
                c.fixed("skin (occluder)", G_ENGINE, skin);
            }
        }
        bake_ao(&mut c.pieces, &AoBake { vox: 0.015, reach: 1.4, rays: 32, open: &|_, _| false });
    }
    let off = b.pieces.len();
    for mut p in c.pieces.into_iter().take(n_deck) {
        p.parent = p.parent.map(|i| i + off);
        b.pieces.push(p);
    }
    Model::finish(b.pieces)
}

// ---------------------------------------------------------------- the gear (in step with g650.py)

/// The tyres meet the ground with the oleos fully extended 10 cm below it:
/// they settle that far under the aircraft's weight at rest.
const GROUND_Y: f64 = (-0.10 - CG_H) as f64;
/// Nominal radii of the 21x7.25-10 and H37.5x12.0R19 tyres (m).
const NOSE_R: f64 = 0.267;
const MAIN_R: f64 = 0.476;
/// The nose axle 2.40 m behind the radome's tip; the main axles 15.55 m, their
/// twin wheels' middles 1.90 m either side (estimates, see above).
const NOSE_AXLE_Z: f64 = (2.40 - CG_D) as f64;
const MAIN_AXLE_Z: f64 = (15.55 - CG_D) as f64;
const MAIN_X: f64 = 1.90;

// The legs' springs, dampers, tyres and brakes (estimates: no oleo data is
// public). The mains sit 1.20 m behind the centre of gravity and the nose
// wheels 11.95 m ahead of it, so at the 35.3 t of `mass` the nose leg carries
// 9.1% of the weight (31.5 kN) and each main leg 157.5 kN: the springs give
// them the 10 cm of static compression the model is drawn with (12.7 cm at
// MTOW), damped at about 0.55 of critical as the 747's are. The design sink
// rate is 10 ft/s (3.05 m/s) at the landing weight (CS 25.473); a leg folds
// at about one and a half times that. Carbon brakes with anti-skid, braking
// at a friction of 0.5 on a dry runway; radial main tyres.
const fn main_leg(x: f64, name: &'static str, kick: f64) -> Leg {
    Leg {
        at: DVec3::new(x, GROUND_Y, MAIN_AXLE_Z),
        wheel_r: MAIN_R,
        travel: 0.35,
        k: 1.58e6,
        c: 1.75e5,
        nf_max: 2.5e6,
        steer: 0.0,
        brakes: true,
        side_k: 2.0e5,
        side_mu: 0.8,
        roll_k: 3.5e3,
        roll_mu: 0.013,
        // carbon brakes
        brake_k: 1.2e5,
        brake_mu: 0.5,
        fold_vn: 4.5,
        fold_kick: DVec3::new(0.3, 0.0, kick),
        name,
    }
}

/// Paint schemes for the aircraft picker (`aircraft::liveries`). "Private"
/// is the baked white jet (assets/g650_paint.jpg). The others are the 747's
/// two FICTIONAL airlines (`b747::ORIVANTE`, `b747::KESTRALIS`), as a
/// corporate shuttle in their colours: painted by plane.wgsl `livery_g650`
/// over the same skin baked without its scheme (assets/g650_plain.jpg,
/// `python assets/blender/g650_paint.py --plain`), whose panel lines,
/// windows and doors show through. Colours: as the 747's, the fourth unused.
pub const LIVERIES: &[Livery] = &[
    Livery { name: "Private", colours: [[0.0; 4]; 4], title: "", paint: None, swatch: [[0.009, 0.018, 0.055], [0.47, 0.30, 0.085]], still: include_bytes!("../../../assets/aircraft/liveries/g650_private.png") },
    Livery { name: "Orivante", colours: [b747::ORIVANTE[0], b747::ORIVANTE[1], b747::ORIVANTE[2], [0.0; 4]], title: "ORIVANTE", paint: Some(&PLAIN), swatch: [[0.004, 0.10, 0.11], [0.80, 0.24, 0.10]], still: include_bytes!("../../../assets/aircraft/liveries/g650_orivante.png") },
    Livery { name: "Kestralis", colours: [b747::KESTRALIS[0], b747::KESTRALIS[1], b747::KESTRALIS[2], [0.0; 4]], title: "KESTRALIS", paint: Some(&PLAIN), swatch: [[0.006, 0.02, 0.10], [0.85, 0.36, 0.02]], still: include_bytes!("../../../assets/aircraft/liveries/g650_kestralis.png") },
];
/// The skin baked without its scheme: white all over, no registration.
static PLAIN: Paint = Paint { jpg: include_bytes!("../../../assets/g650_plain.jpg"), ..PAINT };
const PAINT: Paint = Paint { jpg: include_bytes!("../../../assets/g650_paint.jpg"), views: 4, span: 31.4, length: 31.8, centre: [(CG_D - 15.2) as f64, (3.9 - CG_H) as f64], relief: 1.0, sheen: 0.0 };

pub static SPEC: Spec = Spec {
    id: 9,
    name: "Gulfstream G650",
    keys: &["g650", "gulfstream", "glf6"],
    blurb: "Twin-engine long-range business jet, 36 t. Mach 0.85-0.90 cruise at 41-51,000 ft, MMO 0.925; flaps 10/20/39, about 130 kt approach. / speedbrake, Z reversers, canopy key: airstair door.",
    origin: None,
    ext: None,
    still: include_bytes!("../../../assets/aircraft/g650.png"),
    // (GUIDE_SPAN, GUIDE_LEN, GUIDE_Y, GUIDE_Z in g650.py)
    paint: Some(PAINT),
    build,
    // Estimates of how its empty mass (about 24 t) divides.
    groups: [("fuselage", 11_500.0), ("tail", 1_300.0), ("left wing", 3_900.0), ("right wing", 3_900.0), ("flight deck", 1_500.0), ("radome", 60.0), ("gear", 1_300.0)],
    span: W_X1,
    wing_sect,
    fuse,
    aileron: [10.66, 13.72],
    tank: [1.2, 12.8],
    tail: Tail {
        fin_y0: by(4.05),
        fin_h: 7.82 - 4.05,
        rudder_y0: by(3.78),
        rudder_h: 7.15 - 3.78,
        stab_root: 0.30,
        elev_root: 0.32,
        break_z: [bz(21.4), bz(24.0)],
        fin_base: DVec3::new(0.0, by(4.05) as f64, bz(24.0) as f64),
        fin_tip: DVec3::new(0.0, by(7.82) as f64, bz(27.0) as f64),
        stab_tip: DVec3::new(5.45, by(7.45) as f64, bz(30.0) as f64),
    },
    eye: EYE,
    eye2: None,
    sees_inside: deck::sees_inside,
    hotspots: &[],
    contacts: &[
        ([0.0, by(2.08) as f64, bz(0.0) as f64], Part::Nose),
        ([0.0, by(1.57) as f64, bz(1.0) as f64], Part::Nose),
        ([0.0, by(3.08) as f64, bz(1.85) as f64], Part::Nose),
        ([-0.90, by(2.26) as f64, bz(1.85) as f64], Part::Nose),
        ([0.90, by(2.26) as f64, bz(1.85) as f64], Part::Nose),
        ([0.0, by(1.41) as f64, bz(4.0) as f64], Part::Belly),
        ([0.0, by(1.15) as f64, bz(12.0) as f64], Part::Belly),
        ([0.0, by(1.15) as f64, bz(16.0) as f64], Part::Belly),
        ([0.0, by(1.50) as f64, bz(19.4) as f64], Part::Belly),
        ([-1.37, by(2.75) as f64, bz(6.0) as f64], Part::Side),
        ([1.37, by(2.75) as f64, bz(6.0) as f64], Part::Side),
        ([-1.37, by(2.76) as f64, bz(14.0) as f64], Part::Side),
        ([1.37, by(2.76) as f64, bz(14.0) as f64], Part::Side),
        ([-1.29, by(2.89) as f64, bz(20.4) as f64], Part::Side),
        ([1.29, by(2.89) as f64, bz(20.4) as f64], Part::Side),
        ([0.0, by(4.14) as f64, bz(6.0) as f64], Part::Roof),
        ([0.0, by(4.14) as f64, bz(14.0) as f64], Part::Roof),
        ([0.0, by(4.08) as f64, bz(21.4) as f64], Part::Roof),
        // the nacelles: under their intakes, and under their nozzles
        ([-2.20, by(2.96) as f64, bz(19.8) as f64], Part::Pod(0)),
        ([2.20, by(2.96) as f64, bz(19.8) as f64], Part::Pod(1)),
        ([-2.20, by(3.20) as f64, bz(23.9) as f64], Part::Pod(0)),
        ([2.20, by(3.20) as f64, bz(23.9) as f64], Part::Pod(1)),
    ],
    // under the tail cone, back to the APU's exhaust: with the oleos
    // extended it meets the runway at about 16 degrees nose up
    tail_pts: &[[0.0, by(1.84) as f64, bz(21.4) as f64], [0.0, by(2.28) as f64, bz(23.4) as f64], [0.0, by(2.82) as f64, bz(25.4) as f64], [0.0, by(3.28) as f64, bz(27.0) as f64]],
    wing_st: [3.0 / W_X1, 6.0 / W_X1, 9.0 / W_X1, 12.0 / W_X1, 14.0 / W_X1],
    hard: [
        DVec3::new(0.0, by(2.08) as f64, bz(0.0) as f64),
        DVec3::new(-14.27, by(2.70) as f64, bz(20.9) as f64),
        DVec3::new(14.27, by(2.70) as f64, bz(20.9) as f64),
        DVec3::new(0.0, by(3.43) as f64, bz(27.0) as f64),
        DVec3::new(0.0, by(7.82) as f64, bz(26.5) as f64),
        DVec3::new(0.0, by(1.15) as f64, bz(14.0) as f64),
        DVec3::new(2.20, by(2.96) as f64, bz(20.4) as f64),
    ],
    gear: &[
        // (estimates, as the main legs': see `main_leg`)
        Leg {
            at: DVec3::new(0.0, GROUND_Y, NOSE_AXLE_Z),
            wheel_r: NOSE_R,
            travel: 0.30,
            k: 3.15e5,
            c: 3.5e4,
            nf_max: 6.0e5,
            steer: 1.0,
            brakes: false,
            side_k: 4.5e4,
            side_mu: 0.8,
            roll_k: 1.2e3,
            roll_mu: 0.015,
            brake_k: 0.0,
            brake_mu: 0.0,
            fold_vn: 3.8,
            fold_kick: DVec3::new(-0.4, 0.0, 0.0),
            name: "NOSE GEAR",
        },
        main_leg(-MAIN_X, "LEFT MAIN GEAR", 0.3),
        main_leg(MAIN_X, "RIGHT MAIN GEAR", -0.3),
    ],
    // The airframe sliding on the ground: Concorde's scaled by mass (an estimate).
    hull: Hull { k: 2.2e6, c: 3.2e5, nf_max: 4.3e6, drag: 1.1e5 },
    hole: Hole { fus: [1.37, 1.38, by(2.76), 1.3], wing: [by(W_H0), W_DIH, 0.45, 0.06], stab: [by(7.45), 0.12, 5.45], fin: [by(4.05), 3.77, 0.25, 0.15], pods: [[2.20, by(3.75), 0.79], [0.0; 3]] },
    // ---- mass, balance and aerodynamics (see "The flight model" in the module docs)
    // The 54,000 lb (24,494 kg) basic operating weight with four crew (Aviation
    // Week), 816 kg (1,800 lb: what it may carry with full tanks) of
    // passengers and bags, and half its fuel; full, it is at its 45,359 kg
    // maximum taxi weight.
    mass: 35_334.0,
    // 20,048 kg (TCDS). The two BR725s burn 0.085 kg/s each idling and 0.789 kg/s
    // each at take-off thrust (ICAO emissions databank, UID 01P11BR016); the
    // third figure is the game's arcade boost (8% more thrust), not a real rating.
    fuel_kg: 20_048.0,
    fuel_flow: [0.17, 1.578, 1.72],
    // 6,500 lb (2,948 kg) at most (Aviation Week). Its centroid (estimate):
    // the seats in the forward and middle cabin, the bags behind it, about a
    // metre ahead of the reference CG on the whole.
    payload: [816.0, 2_948.0, -1.0],
    // MAC 4.756 m (TCDS). The CG range is not public (the AFM has it): the
    // reference 30% and limits of 15-45% are estimates (a G450's run 36-43% at
    // its zero-fuel weight). The fuel in the swept wing sits a little aft.
    mac: 4.756,
    cg: [0.30, 0.15, 0.45],
    fuel_arm: 0.3,
    // Pitch, yaw, roll: estimates at `mass` from business-jet radii of gyration
    // (0.33 of the length, 0.45 of the mean of span and length, 0.29 of the
    // span); the 747's scaled by mass and size gives 1.1e6, 1.8e6, 0.73e6.
    inertia: DVec3::new(0.89e6, 1.65e6, 0.68e6),
    wing_area: 119.2,
    b: 30.36,
    cl0: 0.17,
    cl_a: 4.3,
    cd0: 0.0150,
    k_ind: 0.050,
    cd_gear: 0.018,
    cd_flap: [0.009, 0.051],
    flap_cl: 0.452,
    // Stalls (free air, take-off Mach) at 14.7 deg with flaps 10 and 14.2 with
    // flaps 20 (NTSB AAR-12/02): 14.6 clean, 13.3 at flaps 39. No slats.
    stall_alpha: 0.255,
    stall_flap: 0.023,
    stall_slat: 0.0,
    alpha_trim: 0.20,
    mcrit: 0.80,
    cd_wave: 0.0,
    spoiler_cd: 0.05,
    spoiler_cl: 0.40,
    chute: None,
    // The flap lever's notches: 0, 10, 20, 39 degrees; VFE 250, 220 and 190 KCAS.
    flaps: &[0.0, 10.0, 20.0, 39.0],
    vfe: &[1e9, 250.0 * KT, 220.0 * KT, 190.0 * KT],
    flap_rate: 1.0 / 20.0,
    slats_at: 0,
    lever: None,
    gear_rate: 1.0 / 8.0,
    electric_actuators: false,
    // VMO 340 KCAS (300 below 8,000 ft, which cannot be set here), MMO 0.925
    // (TCDS). Flutter sets in 15% past the dive speed, the margin CS 25.629
    // demands (the dive speed an estimate, VMO + 50 kt: 390 kt; it was dived
    // to Mach 0.995 in its flight tests). Between VMO and that it shakes. (The
    // fly-by-wire's protection that keeps it from passing VMO is not modelled:
    // held level at full thrust low down it would go on to some 530 kt.)
    vne: 340.0 * KT,
    mmo: 0.925,
    flutter_v: 450.0 * KT,
    inlet_mach: 0.0,
    dry_mach: 0.0,
    // Limit load 2.5 g, ultimate 3.75 g (CS 25.337).
    g_limit: 2.6,
    g_break: 3.8,
    // Fly-by-wire: the yoke asks for no more than the limit load (an
    // assumption; its angle-of-attack limiter and VMO protection are not
    // modelled).
    fbw_g: 2.5,
    steer_max: 1.0,
    steer_fade: 12.0,
    // Two BR725 A1-12s, 75.2 kN (16,900 lbf) each (TCDS); 100% N1 is 7,000 rpm
    // (EASA TCDS E.018). Idle N1, idle thrust, spool rates (idle to 95% thrust in
    // about 6 s), reverser effect and the thrust lapse are estimates, the lapse
    // fitted to the climb and ceiling (see the module docs).
    // Ground idle: 3.5% of the rating, 2.6 kN an engine (no BR725 figure is
    // public; the ICAO databank's 7% "idle" is its emissions-test setting,
    // above a real ground idle). Against the tyres' rolling resistance (about
    // 4.6 kN at `mass`) it creeps off on a level runway with the brakes
    // released: at `mass` 3 kt after a minute, 5 after two, 19 after ten,
    // levelling off near 30; light (15% fuel) 13 kt after two minutes and
    // 29 kt after five; near MTOW it hardly moves. So holding a 20 kt taxi
    // takes the brakes now and then, as Gulfstream's
    // taxi brake-energy planning assumes (and idle reverse, which it allows
    // without limit for taxiing: G450 AFM excerpt at
    // https://code450.squarespace.com/brake-energy).
    // Their nozzles' exits are 24.0 m behind the radome's tip.
    engine: Engine::Fan(Fan { n: 2, thrust: 75_200.0, idle: 0.24, idle_thrust: 0.035, n1_rpm: 7_000.0, spool_up: 0.7, spool_down: 0.6, reverse: 0.30, reverser_time: 2.0, boost: 1.08, mach_lapse: [0.72, 0.36], nozzle_z: bz(24.0) as f64 }),
    // Estimates: the 747's, quicker (about 25 deg/s of roll at 250 kt; no
    // outboard-aileron lockout), little adverse yaw (the fly-by-wire
    // coordinates the turn), and a sharper wing drop in a stall (its
    // outboard wing stalls first: NTSB AAR-12/02).
    handling: Handling {
        q_ref: 11_000.0,
        a_max: 1.8,
        damp_v: 68.0,
        ad_min: 0.3,
        ad_max: 1.8,
        pitch_k: 3.2,
        pitch_k_lost: 1.0,
        pitch_ctl: 0.35,
        alpha_ctl: 0.2,
        pitch_d: 5.0,
        roll_ctl: 0.8,
        roll_d: 2.75,
        roll_beta: 0.25,
        roll_clean: 0.8,
        yaw_ctl: 0.20,
        yaw_beta: 1.3,
        yaw_d: 2.5,
        dihedral: 0.07,
        adverse: 0.05,
        stall_pitch: 0.3,
        stall_drop: 0.6,
        stall_yaw: 0.8,
    },
    cruise: 128.6,
    // Vref + 5 kt at `mass` with flaps 39.
    approach: (69.0, 3),
    cam_dist: 38.0,
    shadow_r: 18.0,
    // (The 747's turbofans' voice for now.)
    sound: Sound::Jet,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_builds_whole() {
        let m = build();
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in &m.pieces {
            assert!(p.mesh.v.iter().all(|v| v.pos.iter().chain(v.nrm.iter()).all(|x| x.is_finite())), "{} has NaNs", p.name);
            assert!(!p.mesh.i.is_empty(), "{} is empty", p.name);
            for v in &p.mesh.v {
                lo = lo.min(Vec3::from(v.pos));
                hi = hi.max(Vec3::from(v.pos));
            }
        }
        // 30.41 m long (to the stabiliser's tips), 30.36 m across the winglets, 7.82 m tall at rest.
        let (len, span, top) = (hi.z - lo.z, hi.x - lo.x, hi.y + CG_H);
        println!("{} pieces, length {len:.2} m, span {span:.2} m, height {top:.2} m", m.pieces.len());
        assert!((len - 30.41).abs() < 0.1, "{len}");
        assert!((span - 30.36).abs() < 0.1, "{span}");
        assert!((top - 7.82).abs() < 0.05, "{top}");
        // The wing in the flight and damage models is the one drawn: its area
        // to the tips, with the root's extension, a little over the 119.2 m²
        // reference; its MAC near the TCDS's 4.756 m.
        let n = 2000;
        let dx = W_X1 / n as f32;
        let (mut area, mut c2) = (0.0, 0.0);
        for i in 0..n {
            let c = wing_sect((i as f32 + 0.5) * dx).c;
            area += 2.0 * c * dx;
            c2 += 2.0 * c * c * dx;
        }
        // (The root's extension, mostly inside the fuselage, adds 6 m² and
        // lengthens the drawn planform's mean chord to about 5.25 m; the
        // reference trapezoid's is 4.81 m, the TCDS's 4.756 m.)
        println!("wing area {area:.1} m², mean chord {:.2} m", c2 / area);
        assert!(area > 119.0 && area < 130.0, "{area}");
        assert!((c2 / area - 5.2).abs() < 0.2, "{}", c2 / area);
        let (cr, ct, s2) = (W_TE0 - W_LE0, wing_sect(W_X1).c, W_X1);
        let lam = ct / cr;
        let mac = 2.0 / 3.0 * cr * (1.0 + lam + lam * lam) / (1.0 + lam);
        println!("reference trapezoid: {cr:.2} m root, {ct:.2} m tip, {:.1} m², MAC {mac:.2} m", (cr + ct) * s2);
        assert!(((cr + ct) * s2 - 119.2).abs() < 1.0 && (mac - 4.756).abs() < 0.1, "{mac}");
    }

    #[test]
    fn gear_stows_inside_the_skin() {
        let m = build();
        let mut p = crate::plane::Plane::spawn(&SPEC, 0.6, -2.1, 1000.0, 0.0, 120.0);
        p.gear = 0.0;
        let pose = p.pose(&m.pieces);
        // (the landing gear's wheels only: not the flight deck's pitch wheel)
        let wheels: Vec<usize> = m.pieces.iter().enumerate().filter(|(_, pc)| matches!(pc.role, Role::Gear(_)) && pc.name.ends_with(" wheel")).map(|(i, _)| i).collect();
        assert_eq!(wheels.len(), 3);
        for (i, pc) in wheels.into_iter().map(|i| (i, &m.pieces[i])) {
            let c = pose[i].transform_point3(pc.pivot);
            println!("{} stowed at {c:?}, {:.2} m inside the skin", pc.name, skin_depth(c));
            assert!(skin_depth(c) > 0.15, "{} sticks out: {c:?}", pc.name);
        }
    }

    /// The glazing outlines lie on the skin and are where the pilot looks out.
    #[test]
    fn glazing_is_on_the_skin_and_in_view() {
        for s in [-1.0, 1.0] {
            for q in windshield_outline(s).iter().chain(side_window_outline(s).iter()) {
                assert!(skin_depth(*q).abs() < 0.01, "{q:?}");
            }
        }
        // Where a look from the eye meets the skin.
        let hit = |d: Vec3| {
            let d = d.normalize();
            let mut t = 0.0;
            while skin_depth(EYE + d * t) > 0.0 {
                t += 0.005;
            }
            EYE + d * t
        };
        for (what, d, open) in [("ahead, level", Vec3::new(0.0, 0.0, -1.0), true), ("ahead, 10 degrees down", Vec3::new(0.0, -0.176, -1.0), true),
                                ("left", Vec3::new(-1.0, -0.1, 0.0), true), ("right, past the other seat", Vec3::new(1.0, -0.05, -0.1), true), ("up", Vec3::new(0.0, 1.0, 0.0), false)] {
            let q = hit(d);
            println!("{what}: the skin at d {:.2} h {:.2} x {:.2}", q.z + CG_D, q.y + CG_H, q.x);
            assert_eq!(in_glazing(q), open, "{what}: {q:?}");
        }
        assert!(skin_depth(EYE) > 0.3);
    }
}
