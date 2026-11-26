//! Procedurally modelled low-poly vehicles.
//!
//! Each body is a loft of half cross-sections (`Loft::ring`) along the car,
//! nose (-z) to tail (+z), mirrored across x. The ring runs from the underbody
//! up the side (rocker, body line, shoulder), then over the greenhouse
//! (window sill, glass, header, roof rail) to the roof centre. Stations are
//! placed where something changes: the wheel arches, which are real cut-outs
//! with dark wells, the window and pillar edges, the door seams, and the
//! breaks of the side profile. Materials follow `inst.wgsl`: the vertex
//! colour's alpha picks paint, headlight, glass, tail light, chrome or trim.
use crate::render::MeshVertex;
use glam::Vec3;
use std::f32::consts::{PI, TAU};

#[derive(Default)]
pub(crate) struct MB {
    pub v: Vec<MeshVertex>,
    pub i: Vec<u32>,
}

impl MB {
    pub fn poly(&mut self, p: &[Vec3], col: [f32; 4]) {
        let mut n = Vec3::ZERO;
        for k in 1..p.len() - 1 {
            n += (p[k] - p[0]).cross(p[k + 1] - p[0]);
        }
        if n.length_squared() < 1e-14 {
            return;
        }
        let n = n.normalize();
        let b = self.v.len() as u32;
        for q in p {
            self.v.push(MeshVertex {
                pos: (*q).into(),
                nrm: n.into(),
                col,
            });
        }
        for k in 1..p.len() as u32 - 1 {
            self.i.extend([b, b + k, b + k + 1]);
        }
    }

    pub fn cuboid(&mut self, lo: Vec3, hi: Vec3, col: [f32; 4]) {
        let p = |x: bool, y: bool, z: bool| {
            Vec3::new(
                if x { hi.x } else { lo.x },
                if y { hi.y } else { lo.y },
                if z { hi.z } else { lo.z },
            )
        };
        self.poly(
            &[
                p(false, false, true),
                p(true, false, true),
                p(true, true, true),
                p(false, true, true),
            ],
            col,
        );
        self.poly(
            &[
                p(true, false, false),
                p(false, false, false),
                p(false, true, false),
                p(true, true, false),
            ],
            col,
        );
        self.poly(
            &[
                p(true, false, true),
                p(true, false, false),
                p(true, true, false),
                p(true, true, true),
            ],
            col,
        );
        self.poly(
            &[
                p(false, false, false),
                p(false, false, true),
                p(false, true, true),
                p(false, true, false),
            ],
            col,
        );
        self.poly(
            &[
                p(false, true, true),
                p(true, true, true),
                p(true, true, false),
                p(false, true, false),
            ],
            col,
        );
        self.poly(
            &[
                p(false, false, false),
                p(true, false, false),
                p(true, false, true),
                p(false, false, true),
            ],
            col,
        );
    }

    /// The box and its mirror image across x.
    fn cuboid2(&mut self, lo: Vec3, hi: Vec3, col: [f32; 4]) {
        self.cuboid(lo, hi, col);
        self.cuboid(
            Vec3::new(-hi.x, lo.y, lo.z),
            Vec3::new(-lo.x, hi.y, hi.z),
            col,
        );
    }

    /// Average normals across faces that meet at a shallow angle, so panels
    /// read as curved sheet metal while creases stay crisp.
    pub fn smooth(&mut self, cos_limit: f32) {
        let mut map: std::collections::HashMap<[i32; 3], Vec<usize>> =
            std::collections::HashMap::new();
        for (k, v) in self.v.iter().enumerate() {
            map.entry([
                (v.pos[0] * 500.0).round() as i32,
                (v.pos[1] * 500.0).round() as i32,
                (v.pos[2] * 500.0).round() as i32,
            ])
            .or_default()
            .push(k);
        }
        let orig: Vec<Vec3> = self.v.iter().map(|v| Vec3::from(v.nrm)).collect();
        for group in map.values() {
            for &a in group {
                let mut n = Vec3::ZERO;
                for &b in group {
                    if orig[a].dot(orig[b]) > cos_limit
                        && (self.v[a].col[3] - self.v[b].col[3]).abs() < 0.05
                    {
                        n += orig[b];
                    }
                }
                self.v[a].nrm = n.normalize_or_zero().into();
            }
        }
    }

    /// Append another mesh moved to `at`, mirrored across x if asked.
    fn append(&mut self, o: &MB, at: Vec3, mirror: bool) {
        let b = self.v.len() as u32;
        let sx = if mirror { -1.0 } else { 1.0 };
        for v in &o.v {
            let mut v = *v;
            v.pos = [v.pos[0] * sx + at.x, v.pos[1] + at.y, v.pos[2] + at.z];
            v.nrm[0] *= sx;
            self.v.push(v);
        }
        for t in o.i.chunks(3) {
            if mirror {
                self.i.extend([b + t[0], b + t[2], b + t[1]]);
            } else {
                self.i.extend([b + t[0], b + t[1], b + t[2]]);
            }
        }
    }
}

// Materials (alpha: see inst.wgsl). Colours are linear.
pub(crate) const PAINT: [f32; 4] = [1.0, 1.0, 1.0, 0.0];
const HEAD: [f32; 4] = [0.92, 0.92, 0.86, 0.25];
const GLASS: [f32; 4] = [0.02, 0.03, 0.04, 0.5];
const TAIL: [f32; 4] = [0.5, 0.02, 0.015, 0.75];
const CHROME: [f32; 4] = [0.6, 0.61, 0.63, 0.88];
const TRIM: [f32; 4] = [0.03, 0.03, 0.033, 1.0];
const RUBBER: [f32; 4] = [0.012, 0.012, 0.014, 1.0];
const UNDER: [f32; 4] = [0.008, 0.008, 0.008, 1.0];
const SEAM: [f32; 4] = [0.02, 0.02, 0.02, 1.0];
const LINER: [f32; 4] = [0.035, 0.035, 0.037, 1.0];
const AMBER: [f32; 4] = [0.85, 0.38, 0.03, 1.0];
const REVERSE: [f32; 4] = [0.75, 0.75, 0.75, 1.0];
const PLATE: [f32; 4] = [0.8, 0.8, 0.76, 1.0];
const TYRE: [f32; 4] = [0.018, 0.018, 0.02, 1.0];
const ALLOY: [f32; 4] = [0.52, 0.53, 0.55, 0.88];
const STEEL: [f32; 4] = [0.16, 0.16, 0.17, 1.0];
const BOX: [f32; 4] = [0.68, 0.68, 0.66, 1.0];
const BOX_FRAME: [f32; 4] = [0.34, 0.34, 0.35, 1.0];

/// Points on the half cross-section, bottom to roof centre.
const NR: usize = 11;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Wheel {
    /// Five-spoke alloy.
    Alloy,
    /// Multi-spoke sports alloy.
    Sport,
    /// Plain steel with a chrome hub.
    Steel,
}

/// One vehicle's shape. Lengthwise positions are fractions of the length from
/// the nose; heights are metres above the ground; `w` is the half width.
#[derive(Clone)]
pub(crate) struct Spec {
    pub len: f32,
    pub w: f32,
    pub h: f32,
    /// Underbody height between the wheels.
    pub clear: f32,
    /// Beltline (window sill) height at mid length; it rises `rise` over the car.
    pub belt: f32,
    pub rise: f32,
    /// Top of the nose, the trunk deck (or pickup bed rail), and the tail.
    pub nose: f32,
    pub deck: f32,
    pub tail: f32,
    /// Windscreen base and top, rear window top and base (1.0: the rear
    /// window runs down to the tail, as on a hatch, van or bus).
    pub hood_end: f32,
    pub roof0: f32,
    pub roof1: f32,
    pub deck0: f32,
    /// How far the roof edge sits in from the shoulder.
    pub tumble: f32,
    /// The step in from the shoulder to the glass.
    pub ledge: f32,
    /// Painted frame below and above the side glass, metres.
    pub sill: f32,
    pub header: f32,
    pub wheel_r: f32,
    pub tyre_w: f32,
    /// Rim radius as a fraction of the wheel radius.
    pub rim: f32,
    pub wheel: Wheel,
    pub axles: &'static [f32],
    /// Gap between tyre and arch.
    pub arch_gap: f32,
    pub windows: Vec<(f32, f32)>,
    pub seams: Vec<f32>,
    pub handles: Vec<f32>,
    /// Centre heights of the head and tail lamps.
    pub lamp_y: f32,
    pub tail_y: f32,
    /// Square plan and vertical sides (vans, trucks, buses).
    pub boxy: bool,
    /// Black plastic bumpers and lower cladding.
    pub clad: bool,
    pub chrome: bool,
    /// Flat front: the windscreen is drawn on the nose (bus, cab-over truck).
    pub flat_front: bool,
    /// Pickup bed floor height.
    pub bed: Option<f32>,
    /// Box body height above the deck (box truck).
    pub cargo: Option<f32>,
    pub rear_glass: bool,
    pub rails: bool,
    pub spoiler: bool,
    pub bus: bool,
    pub exhausts: u8,
}

fn lerp_knots(k: &[(f32, f32)], f: f32) -> f32 {
    for w in k.windows(2) {
        if f <= w[1].0 {
            let t = ((f - w[0].0) / (w[1].0 - w[0].0).max(1e-5)).clamp(0.0, 1.0);
            return w[0].1 + (w[1].1 - w[0].1) * t;
        }
    }
    k[k.len() - 1].1
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Region {
    Hood,
    Cabin,
    Bed,
    Chassis,
}

impl Spec {
    /// Half width of the body at `f`.
    pub fn half_width(&self, f: f32) -> f32 {
        let k: &[(f32, f32)] = if self.boxy {
            &[
                (0.0, 0.95),
                (0.006, 0.985),
                (0.02, 1.0),
                (0.98, 1.0),
                (0.994, 0.985),
                (1.0, 0.95),
            ]
        } else {
            &[
                (0.0, 0.84),
                (0.012, 0.915),
                (0.04, 0.965),
                (0.13, 1.0),
                (0.87, 1.0),
                (0.955, 0.975),
                (0.988, 0.93),
                (1.0, 0.87),
            ]
        };
        self.w * lerp_knots(k, f)
    }

    fn belt_at(&self, f: f32) -> f32 {
        self.belt + self.rise * (f - 0.5)
    }

    fn cabin_end(&self) -> f32 {
        if self.deck0 >= 0.999 { 1.0 } else { self.deck0 }
    }

    fn region(&self, f: f32) -> Region {
        if f < self.hood_end - 1e-4 {
            Region::Hood
        } else if f <= self.cabin_end() + 1e-4 {
            Region::Cabin
        } else if self.bed.is_some() {
            Region::Bed
        } else if self.cargo.is_some() {
            Region::Chassis
        } else {
            Region::Hood
        }
    }

    fn arch_r(&self) -> f32 {
        self.wheel_r + self.arch_gap
    }

    /// Depth of the wheel wells in from the body side.
    fn well(&self) -> f32 {
        self.tyre_w + 0.07
    }

    /// The arch over a wheel at `f`: the height of its edge there.
    fn arch(&self, f: f32) -> Option<f32> {
        let z = (f - 0.5) * self.len;
        let ra = self.arch_r();
        self.axles.iter().find_map(|&af| {
            let dz = z - (af - 0.5) * self.len;
            (dz.abs() <= ra + 1e-4).then(|| self.wheel_r + (ra * ra - dz * dz).max(0.0).sqrt())
        })
    }

    /// The bottom edge of the side: the sill, lifted at the bumpers, or the arch.
    fn bottom(&self, f: f32) -> (f32, bool) {
        let lift = if self.boxy { 0.06 } else { 0.11 };
        let end = (1.0 - f.min(1.0 - f) / 0.04).clamp(0.0, 1.0);
        let base = self.clear + lift * end;
        match self.arch(f) {
            Some(y) => (y.max(base), true),
            None => (base, false),
        }
    }

    /// The outward x of a wheel's centre: its tyre flush with the body side.
    pub fn wheel_x(&self, af: f32) -> f32 {
        self.half_width(af) - self.tyre_w * 0.5 - 0.012
    }

    fn top_knots(&self) -> Vec<(f32, f32)> {
        let mut k = vec![(0.0, self.nose)];
        if self.flat_front {
            k.push((self.roof0, self.h));
        } else {
            k.push((0.012, self.nose + 0.03));
            k.push((self.hood_end, self.belt_at(self.hood_end)));
            k.push((self.roof0, self.h));
        }
        k.push((0.5 * (self.roof0 + self.roof1), self.h + 0.005));
        k.push((self.roof1, self.h - 0.015));
        if self.deck0 >= 0.999 {
            k.push((1.0, self.tail));
        } else {
            k.push((self.deck0, self.belt_at(self.deck0)));
            let d1 = self.deck0 + 0.06;
            if d1 < 0.95 {
                k.push((d1, self.deck));
                k.push((0.95, self.deck));
            }
            if 0.988 > self.deck0 + 0.005 {
                k.push((0.988, self.tail + 0.025));
            }
            k.push((1.0, self.tail));
        }
        k.dedup_by(|b, a| b.0 <= a.0 + 1e-4);
        k
    }
}

struct Loft<'a> {
    s: &'a Spec,
    top: Vec<(f32, f32)>,
    /// The far model: no stations at the pillars, so no blacked-out pillars.
    lod: bool,
}

impl Loft<'_> {
    /// The hood or trunk surface across the car at height `t`, r5..r10.
    fn hood_upper(&self, wb: f32, t: f32) -> [Vec3; 6] {
        let crown = if self.s.boxy { 0.01 } else { 0.028 };
        let xs = [0.95, 0.93, 0.91, 0.89, 0.8, 0.4];
        let ys = [0.0, 0.15, 0.3, 0.4, 0.7, 1.0];
        std::array::from_fn(|k| Vec3::new(wb * xs[k], t - 0.012 + crown * ys[k], 0.0))
    }

    fn ring(&self, f: f32) -> [Vec3; NR] {
        let s = self.s;
        let z = (f - 0.5) * s.len;
        let wb = s.half_width(f);
        let t = lerp_knots(&self.top, f);
        let (ybot, in_arch) = s.bottom(f);
        let belt = s.belt_at(f);
        let region = s.region(f);
        let shoulder = if s.boxy { 1.0 } else { 0.985 };
        let y4 = match region {
            Region::Cabin => belt,
            Region::Hood => belt.min(t - 0.02),
            Region::Bed => s.deck - 0.03,
            Region::Chassis => s.deck - 0.02,
        };
        let y3 = (belt - 0.2).min(y4 - 0.05);
        let band = if s.clad && !in_arch { 0.11 } else { 0.06 };
        let y2 = (ybot + band).min(y3 - 0.02).max(ybot + 0.005);
        let y3 = y3.max(y2 + 0.005);
        let y4 = y4.max(y3 + 0.005);
        let xin = if in_arch { wb - s.well() } else { wb * 0.88 };
        let lower = [
            Vec3::new(xin, ybot, 0.0),
            Vec3::new(wb * 0.985, ybot, 0.0),
            Vec3::new(wb, y2, 0.0),
            Vec3::new(wb, y3, 0.0),
            Vec3::new(wb * shoulder, y4, 0.0),
        ];
        let upper: [Vec3; 6] = match region {
            Region::Hood => self.hood_upper(wb, t),
            Region::Chassis => self.hood_upper(wb, s.deck),
            Region::Bed => {
                let (rail, floor) = (s.deck, s.bed.unwrap_or(s.deck - 0.3));
                [
                    Vec3::new(wb * 0.975, rail, 0.0),
                    Vec3::new(wb - 0.075, rail, 0.0),
                    Vec3::new(wb - 0.075, floor + 0.05, 0.0),
                    Vec3::new(wb - 0.1, floor, 0.0),
                    Vec3::new(wb * 0.5, floor, 0.0),
                    Vec3::new(wb * 0.2, floor, 0.0),
                ]
            }
            Region::Cabin => {
                let gb = y4;
                let gt = t.max(gb);
                let hgt = gt - gb;
                let k = (hgt / (s.h - s.belt).max(0.1)).clamp(0.0, 1.0);
                let sm = (hgt / 0.15).clamp(0.0, 1.0);
                let x5 = wb * shoulder - s.ledge;
                let rx = x5 - s.tumble * k;
                let r5 = Vec3::new(x5, gb + 0.01, 0.0);
                let r8 = Vec3::new(rx, gt - 0.05 * sm, 0.0);
                let h58 = (r8.y - r5.y).max(1e-3);
                let r6 = r5.lerp(r8, (s.sill / h58).min(0.4));
                let r7 = r5.lerp(r8, 1.0 - (s.header / h58).min(0.45));
                let cab = [
                    r5,
                    r6,
                    r7,
                    r8,
                    Vec3::new(rx - 0.07, gt - 0.012 * sm, 0.0),
                    Vec3::new(rx * 0.45, gt, 0.0),
                ];
                let hood = self.hood_upper(wb, gt);
                std::array::from_fn(|i| hood[i].lerp(cab[i], sm))
            }
        };
        let mut r = [Vec3::ZERO; NR];
        let mut lower = lower;
        // A low hood over a tall wheel: keep the well under the skin.
        let skin = upper.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        lower[0].y = lower[0].y.min(skin - 0.03);
        r[..5].copy_from_slice(&lower);
        r[5..].copy_from_slice(&upper);
        for p in &mut r {
            p.z = z;
        }
        r
    }

    /// The side's x at height `y`, on the part of the ring below the shoulder.
    fn side_x(r: &[Vec3; NR], y: f32) -> f32 {
        if y <= r[1].y {
            return r[1].x;
        }
        for j in 1..4 {
            if y <= r[j + 1].y {
                let t = (y - r[j].y) / (r[j + 1].y - r[j].y).max(1e-5);
                return r[j].x + (r[j + 1].x - r[j].x) * t;
            }
        }
        r[4].x
    }

    fn window(&self, f: f32) -> bool {
        self.s.windows.iter().any(|&(a, b)| f > a && f < b)
    }

    /// Between two side windows close together: a blacked-out pillar.
    fn b_pillar(&self, f: f32) -> bool {
        !self.lod
            && self
                .s
                .windows
                .windows(2)
                .any(|w| f >= w[0].1 && f <= w[1].0 && (w[1].0 - w[0].1) * self.s.len < 0.25)
    }

    fn seam(&self, f: f32) -> bool {
        let half = 0.0045 / self.s.len;
        self.s.seams.iter().any(|&q| (f - q).abs() < half)
    }

    /// The colour of loft band `j` (ring point j to j+1; 10 is the roof
    /// centre strip) in the segment centred at `f`.
    fn colour(&self, j: usize, f: f32) -> [f32; 4] {
        let s = self.s;
        let region = s.region(f);
        let ends = f < 0.035 || f > 0.965;
        let screen =
            (f > s.hood_end && f < s.roof0) || (s.rear_glass && f > s.roof1 && f < s.cabin_end());
        if region == Region::Chassis {
            return if j == 0 { UNDER } else { TRIM };
        }
        match j {
            0 => UNDER,
            1 if s.clad => TRIM,
            1 if ends && s.chrome => CHROME,
            1..=4 if self.seam(f) && region == Region::Cabin => SEAM,
            1..=4 => PAINT,
            5..=7 => match region {
                Region::Cabin if self.window(f) => {
                    [RUBBER, GLASS, if s.header > 0.1 { PAINT } else { RUBBER }][j - 5]
                }
                Region::Cabin if self.b_pillar(f) && (j < 7 || s.header <= 0.1) => RUBBER,
                Region::Bed if j > 5 => LINER,
                _ => PAINT,
            },
            8 if region == Region::Bed => LINER,
            8 => PAINT,
            _ => match region {
                Region::Bed => LINER,
                Region::Cabin if screen => GLASS,
                _ => PAINT,
            },
        }
    }

    fn stations(&self, lod: bool) -> Vec<f32> {
        let s = self.s;
        let mut fs: Vec<f32> = if lod {
            vec![0.0, 0.04, 0.955, 1.0]
        } else {
            vec![0.0, 0.012, 0.04, 0.09, 0.13, 0.87, 0.92, 0.955, 0.988, 1.0]
        };
        fs.extend(self.top.iter().map(|k| k.0));
        if !lod {
            fs.extend(s.windows.iter().flat_map(|w| [w.0, w.1]));
            let half = 0.0045 / s.len;
            fs.extend(s.seams.iter().flat_map(|&q| [q - half, q + half]));
            fs.extend([
                0.5 * (s.hood_end + s.roof0),
                0.5 * (s.roof1 + s.cabin_end()),
            ]);
        }
        if s.bed.is_some() || s.cargo.is_some() {
            fs.push(s.deck0 + 0.002);
        }
        let ra = s.arch_r();
        let n = if lod { 2 } else { 10 };
        for &af in s.axles {
            let zc = (af - 0.5) * s.len;
            for k in 0..=n {
                let dz = -ra * (PI * k as f32 / n as f32).cos();
                fs.push((zc + dz) / s.len + 0.5);
            }
            fs.push((zc - ra - 0.004) / s.len + 0.5);
            fs.push((zc + ra + 0.004) / s.len + 0.5);
        }
        let mut fs: Vec<f32> = fs.into_iter().map(|f| f.clamp(0.0, 1.0)).collect();
        fs.sort_by(|a, b| a.total_cmp(b));
        fs.dedup_by(|b, a| (*b - *a).abs() < 3e-4);
        fs
    }
}

fn mir(p: Vec3) -> Vec3 {
    Vec3::new(-p.x, p.y, p.z)
}

/// A polygon and its mirror image across x.
fn poly2(m: &mut MB, p: &[Vec3], col: [f32; 4]) {
    m.poly(p, col);
    let q: Vec<Vec3> = p.iter().rev().map(|v| mir(*v)).collect();
    m.poly(&q, col);
}

/// The body without its wheels.
pub(crate) fn body(s: &Spec, lod: bool) -> MB {
    let loft = Loft {
        s,
        top: s.top_knots(),
        lod,
    };
    let fs = loft.stations(lod);
    let rings: Vec<[Vec3; NR]> = fs.iter().map(|&f| loft.ring(f)).collect();
    // The far model skips the rocker, window frames and roof rail points;
    // each of its bands takes the colour of the main band it spans.
    let pick: &[usize] = if lod {
        &[0, 1, 4, 5, 8, 10]
    } else {
        &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
    };
    let mut m = MB::default();
    for (w, r) in fs.windows(2).zip(rings.windows(2)) {
        let mid = 0.5 * (w[0] + w[1]);
        let (a, b) = (&r[0], &r[1]);
        for p in pick.windows(2) {
            let (i, j) = (p[0], p[1]);
            let band = if j - i > 1 { i + 1 } else { i };
            poly2(&mut m, &[a[i], b[i], b[j], a[j]], loft.colour(band, mid));
        }
        m.poly(
            &[a[NR - 1], b[NR - 1], mir(b[NR - 1]), mir(a[NR - 1])],
            loft.colour(10, mid),
        );
        m.poly(&[mir(a[0]), mir(b[0]), b[0], a[0]], UNDER);
    }
    // Nose and tail: strips between each ring point and its mirror image.
    for (r, f) in [(&rings[0], 0.0f32), (&rings[rings.len() - 1], 1.0)] {
        let chassis = s.region(f) == Region::Chassis;
        for p in pick.windows(2) {
            let (i, j) = (p[0], p[1]);
            let col = if i == 0 || chassis { UNDER } else { PAINT };
            m.poly(&[r[i], r[j], mir(r[j]), mir(r[i])], col);
        }
    }
    m.smooth(0.86);
    // Wheel wells: the inner wall of each arch.
    let ra = s.arch_r();
    for &af in s.axles {
        let zc = (af - 0.5) * s.len;
        let x = s.half_width(af) - s.well();
        let mut p = vec![Vec3::new(x, s.clear, zc + ra)];
        for k in 0..=8 {
            let a = PI * k as f32 / 8.0;
            let z = zc + a.cos() * ra;
            let cap = loft.ring(z / s.len + 0.5)[0].y;
            p.push(Vec3::new(
                x,
                (s.wheel_r + a.sin() * ra).min(cap.max(s.wheel_r)),
                z,
            ));
        }
        p.push(Vec3::new(x, s.clear, zc - ra));
        poly2(&mut m, &p, UNDER);
    }
    ends(&mut m, &loft, &rings[0], &rings[rings.len() - 1], lod);
    sides(&mut m, &loft, lod);
    m
}

/// A flat shape on the nose or tail, `out` metres proud of it.
fn cap_quad(m: &mut MB, z: f32, pts: &[(f32, f32)], col: [f32; 4]) {
    let p: Vec<Vec3> = pts.iter().map(|&(x, y)| Vec3::new(x, y, z)).collect();
    m.poly(&p, col);
}

fn cap_rect(m: &mut MB, z: f32, x0: f32, x1: f32, y0: f32, y1: f32, col: [f32; 4]) {
    cap_quad(m, z, &[(x0, y0), (x1, y0), (x1, y1), (x0, y1)], col);
}

/// A rectangle on both halves of the nose or tail.
fn cap_rect2(m: &mut MB, z: f32, x0: f32, x1: f32, y0: f32, y1: f32, col: [f32; 4]) {
    cap_rect(m, z, x0, x1, y0, y1, col);
    cap_rect(m, z, -x1, -x0, y0, y1, col);
}

/// Something stuck flat onto the side of the body between `f0` and `f1`,
/// from `y0` up to `y1`, on the right (+x), left or both sides.
fn side_decal(
    m: &mut MB,
    loft: &Loft,
    f0: f32,
    f1: f32,
    y0: f32,
    y1: f32,
    col: [f32; 4],
    sides: [bool; 2],
) {
    const N: usize = 3;
    let off = 0.007;
    let pts: Vec<(Vec3, Vec3)> = (0..=N)
        .map(|k| {
            let f = f0 + (f1 - f0) * k as f32 / N as f32;
            let r = loft.ring(f);
            let z = r[0].z;
            (
                Vec3::new(Loft::side_x(&r, y0) + off, y0, z),
                Vec3::new(Loft::side_x(&r, y1) + off, y1, z),
            )
        })
        .collect();
    for k in 0..N {
        let q = [pts[k].0, pts[k + 1].0, pts[k + 1].1, pts[k].1];
        if sides[0] {
            m.poly(&q, col);
        }
        if sides[1] {
            m.poly(&[mir(q[3]), mir(q[2]), mir(q[1]), mir(q[0])], col);
        }
    }
}

/// Lamps, grille, bumpers and plates on the nose and tail.
fn ends(m: &mut MB, loft: &Loft, front: &[Vec3; NR], back: &[Vec3; NR], lod: bool) {
    let s = loft.s;
    let cw = |r: &[Vec3; NR], y: f32| Loft::side_x(r, y);
    // Front.
    {
        let r = front;
        let (z1, z2) = (r[0].z - 0.008, r[0].z - 0.015);
        let ybot = r[1].y;
        let ly = s.lamp_y;
        let w = cw(r, ly);
        if s.clad {
            let wb = cw(r, ybot + 0.15);
            cap_rect(m, z1, -wb * 0.99, wb * 0.99, ybot, ybot + 0.3, TRIM);
        } else if s.chrome {
            let wb = cw(r, ybot + 0.15);
            cap_rect(m, z1, -wb * 1.0, wb * 1.0, ybot + 0.04, ybot + 0.28, CHROME);
        }
        if s.flat_front {
            let wg = cw(r, s.belt) * 0.93;
            let top = if s.bus { s.h - 0.42 } else { s.h - 0.2 };
            let bottom = if s.bus { s.belt - 0.25 } else { s.belt + 0.02 };
            cap_rect(
                m,
                z1,
                -wg - 0.03,
                wg + 0.03,
                bottom - 0.04,
                top + 0.04,
                RUBBER,
            );
            cap_rect(m, z2, -wg, wg, bottom, top, GLASS);
            if s.bus {
                cap_rect(m, z1, -wg, wg, s.h - 0.36, s.h - 0.1, RUBBER);
                cap_rect(
                    m,
                    z2,
                    -wg * 0.7,
                    wg * 0.7,
                    s.h - 0.3,
                    s.h - 0.16,
                    [1.0, 0.62, 0.15, 0.25],
                );
            }
            // Lamps low on the corners, a grille between.
            cap_rect2(m, z2, w * 0.62, w * 0.9, ly - 0.07, ly + 0.07, HEAD);
            cap_rect2(m, z2, w * 0.62, w * 0.9, ly - 0.14, ly - 0.09, AMBER);
            cap_rect(m, z1, -w * 0.5, w * 0.5, ly - 0.1, ly + 0.12, TRIM);
            if !lod {
                for k in 0..3 {
                    let y = ly - 0.06 + k as f32 * 0.07;
                    cap_rect(
                        m,
                        z2,
                        -w * 0.47,
                        w * 0.47,
                        y,
                        y + 0.025,
                        if s.chrome { CHROME } else { RUBBER },
                    );
                }
            }
        } else {
            // Swept headlamps, the grille between them, an intake below.
            for sx in [1.0f32, -1.0] {
                let p = [
                    (w * 0.5, ly - 0.035),
                    (w * 0.9, ly - 0.055),
                    (w * 0.93, ly + 0.05),
                    (w * 0.52, ly + 0.05),
                ];
                let p: Vec<(f32, f32)> = p.iter().map(|&(x, y)| (x * sx, y)).collect();
                cap_quad(m, z2, &p, HEAD);
            }
            let gw = w * 0.44;
            if s.chrome {
                cap_rect(m, z1, -gw - 0.04, gw + 0.04, ly - 0.1, ly + 0.07, CHROME);
            }
            cap_rect(m, z2, -gw, gw, ly - 0.07, ly + 0.045, RUBBER);
            if s.chrome && !lod {
                for k in 0..2 {
                    let y = ly - 0.04 + k as f32 * 0.045;
                    cap_rect(m, z2 - 0.004, -gw, gw, y, y + 0.015, CHROME);
                }
            }
            let iw = cw(r, ybot + 0.12);
            cap_rect(
                m,
                z2,
                -iw * 0.58,
                iw * 0.58,
                ybot + 0.06,
                ybot + 0.17,
                RUBBER,
            );
            cap_rect2(m, z2, iw * 0.66, iw * 0.82, ybot + 0.09, ybot + 0.14, AMBER);
            // Lamps wrap round the corners.
            side_decal(
                m,
                loft,
                0.002,
                0.03,
                ly - 0.035,
                ly + 0.05,
                HEAD,
                [true, true],
            );
        }
        if !s.clad && !s.chrome && !s.flat_front {
            let wb = cw(r, ybot + 0.27);
            cap_rect(
                m,
                z1,
                -wb * 0.98,
                wb * 0.98,
                ybot + 0.26,
                ybot + 0.272,
                SEAM,
            );
        }
        let py = if s.flat_front {
            ybot + 0.1
        } else {
            ybot + 0.065
        };
        cap_rect(m, z2 - 0.005, -0.26, 0.26, py, py + 0.11, PLATE);
    }
    // Tail.
    {
        let r = back;
        let (z1, z2) = (r[0].z + 0.008, r[0].z + 0.015);
        let ybot = r[1].y;
        let ty = s.tail_y;
        let w = cw(r, ty);
        if s.clad {
            let wb = cw(r, ybot + 0.15);
            cap_rect(m, z1, -wb * 0.99, wb * 0.99, ybot, ybot + 0.28, TRIM);
        } else if s.chrome {
            let wb = cw(r, ybot + 0.15);
            cap_rect(m, z1, -wb, wb, ybot + 0.03, ybot + 0.25, CHROME);
        }
        if s.boxy {
            // Tall lamps up the corners.
            let wt = cw(r, ty).min(cw(r, ty + 0.2));
            cap_rect2(m, z2, wt * 0.8, wt * 0.96, ty - 0.18, ty + 0.18, TAIL);
            cap_rect2(m, z2, wt * 0.8, wt * 0.96, ty - 0.26, ty - 0.19, AMBER);
            cap_rect2(m, z2, wt * 0.8, wt * 0.96, ty + 0.19, ty + 0.26, REVERSE);
        } else {
            cap_rect2(m, z2, w * 0.56, w * 0.94, ty - 0.06, ty + 0.06, TAIL);
            cap_rect2(m, z2, w * 0.56, w * 0.66, ty - 0.06, ty - 0.01, REVERSE);
            side_decal(
                m,
                loft,
                0.972,
                0.998,
                ty - 0.06,
                ty + 0.06,
                TAIL,
                [true, true],
            );
        }
        if s.cargo.is_none() && s.bed.is_none() && s.boxy {
            // Rear windows in the doors (van) or the back of the bus.
            let (y0, y1) = if s.bus {
                (s.h - 1.15, s.h - 0.4)
            } else {
                (s.belt + 0.08, s.h - 0.22)
            };
            let wg = r[7].x.min(r[6].x) * 0.9;
            if s.bus {
                cap_rect(m, z2, -wg, wg, y0, y1, GLASS);
                cap_rect(m, z1, -wg * 0.9, wg * 0.9, ybot + 0.35, ybot + 0.75, RUBBER);
            } else {
                cap_rect2(m, z2, 0.03, wg, y0, y1, GLASS);
                cap_rect(m, z1, -0.006, 0.006, ybot + 0.05, s.h - 0.06, SEAM);
            }
        }
        if s.bed.is_some() && !lod {
            // Tailgate handle.
            cap_rect(
                m,
                z2,
                -0.12,
                0.12,
                s.deck - 0.12,
                s.deck - 0.07,
                if s.chrome { CHROME } else { TRIM },
            );
        }
        if !s.clad && !s.chrome && !s.boxy {
            let wb = cw(r, ybot + 0.25);
            cap_rect(
                m,
                z1,
                -wb * 0.98,
                wb * 0.98,
                ybot + 0.24,
                ybot + 0.252,
                SEAM,
            );
        }
        let py = if s.boxy || s.bed.is_some() {
            ybot + 0.08
        } else {
            ty - 0.22
        };
        cap_rect(m, z2 + 0.005, -0.26, 0.26, py, py + 0.11, PLATE);
        if !lod {
            for k in 0..s.exhausts {
                let x = if k == 0 { -w * 0.6 } else { w * 0.6 };
                let y = r[1].y + 0.02;
                m.cuboid(
                    Vec3::new(x - 0.04, y - 0.035, r[0].z - 0.2),
                    Vec3::new(x + 0.04, y + 0.035, r[0].z + 0.035),
                    CHROME,
                );
            }
        }
    }
}

/// Mirrors, handles, roof rails, spoiler, the box of a truck, bus doors.
fn sides(m: &mut MB, loft: &Loft, lod: bool) {
    let s = loft.s;
    // Door mirrors, just behind the windscreen pillar.
    if let Some(&(wf, _)) = s.windows.first() {
        if s.bus {
            let z = -s.len * 0.5 - 0.22;
            let x = s.w + 0.12;
            m.cuboid2(
                Vec3::new(x - 0.07, s.h - 1.05, z - 0.04),
                Vec3::new(x + 0.07, s.h - 0.6, z + 0.04),
                TRIM,
            );
            m.cuboid2(
                Vec3::new(s.w - 0.02, s.h - 0.55, z - 0.02),
                Vec3::new(x, s.h - 0.52, z + 0.25),
                TRIM,
            );
        } else {
            let f = wf + 0.04 / s.len;
            let r = loft.ring(f);
            let (bx, by, z) = (r[5].x, r[5].y, r[0].z);
            let big = s.boxy;
            let (reach, y0, y1, th) = if big {
                (0.24, by + 0.02, by + 0.34, 0.05)
            } else {
                (0.2, by + 0.03, by + 0.14, 0.05)
            };
            m.cuboid2(
                Vec3::new(bx - 0.02, y0 + 0.02, z - 0.025),
                Vec3::new(bx + 0.06, y0 + 0.06, z + 0.025),
                TRIM,
            );
            m.cuboid2(
                Vec3::new(bx + 0.05, y0, z - th),
                Vec3::new(bx + reach, y1, z + th),
                if big { TRIM } else { PAINT },
            );
            // The glass faces back.
            let g = [
                Vec3::new(bx + 0.065, y0 + 0.012, z + th + 0.003),
                Vec3::new(bx + reach - 0.012, y0 + 0.012, z + th + 0.003),
                Vec3::new(bx + reach - 0.012, y1 - 0.012, z + th + 0.003),
                Vec3::new(bx + 0.065, y1 - 0.012, z + th + 0.003),
            ];
            poly2(m, &g, CHROME);
        }
    }
    if !lod {
        for &f in &s.handles {
            let b = s.belt_at(f);
            let df = 0.09 / s.len;
            side_decal(
                m,
                loft,
                f - df,
                f + df,
                b - 0.115,
                b - 0.08,
                if s.chrome { CHROME } else { TRIM },
                [true, true],
            );
        }
        if !s.boxy {
            // A side repeater on each front wing.
            let f = s.axles[0] + (s.arch_r() + 0.18) / s.len;
            let b = s.belt_at(f);
            side_decal(
                m,
                loft,
                f - 0.03 / s.len,
                f + 0.03 / s.len,
                b - 0.13,
                b - 0.105,
                AMBER,
                [true, true],
            );
        }
    }
    let top = s.top_knots();
    let rail_x = s.w * if s.boxy { 1.0 } else { 0.985 } - s.ledge - s.tumble;
    if s.rails {
        let (z0, z1) = (
            (s.roof0 + 0.05 - 0.5) * s.len,
            (s.roof1 - 0.02 - 0.5) * s.len,
        );
        m.cuboid2(
            Vec3::new(rail_x - 0.13, s.h - 0.01, z0),
            Vec3::new(rail_x - 0.09, s.h + 0.05, z1),
            TRIM,
        );
        for zz in [z0, z1 - 0.05] {
            m.cuboid2(
                Vec3::new(rail_x - 0.135, s.h - 0.02, zz),
                Vec3::new(rail_x - 0.085, s.h + 0.06, zz + 0.05),
                TRIM,
            );
        }
    }
    if s.spoiler {
        let y = lerp_knots(&top, 0.97);
        let (z0, z1) = ((0.955 - 0.5) * s.len, (0.995 - 0.5) * s.len);
        m.cuboid(
            Vec3::new(-s.w * 0.86, y + 0.075, z0),
            Vec3::new(s.w * 0.86, y + 0.1, z1),
            TRIM,
        );
        m.cuboid2(
            Vec3::new(s.w * 0.5, y - 0.02, z0 + 0.05),
            Vec3::new(s.w * 0.5 + 0.03, y + 0.08, z0 + 0.12),
            TRIM,
        );
    }
    if s.bus && !lod {
        // Doors on the kerb side: the front one ahead of the wheel, one mid-way.
        for (f0, f1) in [(0.022, 0.108), (0.47, 0.555)] {
            side_decal(
                m,
                loft,
                f0,
                f1,
                s.clear + 0.18,
                s.h - 0.42,
                RUBBER,
                [true, false],
            );
            let pad = 0.03 / s.len;
            let r = loft.ring(f0);
            let x = Loft::side_x(&r, s.belt) + 0.012;
            let mut glass = |a: f32, b: f32| {
                let (za, zb) = ((a - 0.5) * s.len, (b - 0.5) * s.len);
                m.poly(
                    &[
                        Vec3::new(x, s.clear + 0.3, za),
                        Vec3::new(x, s.clear + 0.3, zb),
                        Vec3::new(x, s.h - 0.5, zb),
                        Vec3::new(x, s.h - 0.5, za),
                    ],
                    GLASS,
                );
            };
            let mid = 0.5 * (f0 + f1);
            glass(f0 + pad, mid - pad * 0.3);
            glass(mid + pad * 0.3, f1 - pad);
        }
    }
    if s.bus {
        // Air conditioning on the roof.
        m.cuboid(
            Vec3::new(-0.7, s.h - 0.02, -0.15 * s.len),
            Vec3::new(0.7, s.h + 0.22, 0.05 * s.len),
            BOX,
        );
    }
    if let Some(bh) = s.cargo {
        let z0 = (s.deck0 + 0.016 - 0.5) * s.len;
        let z1 = 0.5 * s.len - 0.01;
        let bx = s.w + 0.06;
        let (y0, y1) = (s.deck - 0.04, s.deck + bh);
        m.cuboid(Vec3::new(-bx, y0, z0), Vec3::new(bx, y1, z1), BOX);
        let e = 0.035;
        // Corner posts, roof and floor rails.
        for zz in [z0 - e * 0.3, z1 - 0.06] {
            m.cuboid2(
                Vec3::new(bx - 0.05, y0, zz),
                Vec3::new(bx + e * 0.4, y1 + e * 0.4, zz + 0.06 + e * 0.3),
                BOX_FRAME,
            );
        }
        m.cuboid2(
            Vec3::new(bx - 0.04, y1 - 0.06, z0),
            Vec3::new(bx + e * 0.3, y1 + e * 0.3, z1),
            BOX_FRAME,
        );
        m.cuboid2(
            Vec3::new(bx - 0.04, y0 - 0.02, z0),
            Vec3::new(bx + e * 0.3, y0 + 0.13, z1),
            BOX_FRAME,
        );
        if !lod {
            // Rear doors: the seam, lock bars and hinges.
            let zb = z1 + 0.004;
            cap_rect(m, zb, -0.008, 0.008, y0 + 0.13, y1 - 0.06, SEAM);
            for x in [-0.32, -0.12, 0.12, 0.32] {
                cap_rect(
                    m,
                    zb + 0.004,
                    x - 0.012,
                    x + 0.012,
                    y0 + 0.2,
                    y1 - 0.12,
                    CHROME,
                );
            }
            for y in [y0 + 0.4, 0.5 * (y0 + y1), y1 - 0.4] {
                cap_rect2(
                    m,
                    zb + 0.004,
                    bx - 0.14,
                    bx - 0.05,
                    y - 0.05,
                    y + 0.05,
                    BOX_FRAME,
                );
            }
            // Side guards between the wheels.
            let ra = s.arch_r();
            let (za, zb2) = (
                (s.axles[0] - 0.5) * s.len + ra + 0.25,
                (s.axles[1] - 0.5) * s.len - ra - 0.1,
            );
            for y in [0.42f32, 0.62] {
                m.cuboid2(
                    Vec3::new(s.w - 0.06, y, za),
                    Vec3::new(s.w + 0.02, y + 0.06, zb2),
                    BOX_FRAME,
                );
            }
        }
        // Rear underrun bar.
        m.cuboid(
            Vec3::new(-s.w * 0.95, s.clear + 0.05, z1 - 0.06),
            Vec3::new(s.w * 0.95, s.clear + 0.2, z1 + 0.04),
            TRIM,
        );
    }
}

/// One wheel centred on its axle, its axis along x; `detail` puts the rim on
/// the -x and +x faces, the other face showing a dark disc.
pub(crate) fn wheel(s: &Spec, detail: [bool; 2], lod: bool) -> MB {
    let n: usize = match (lod, s.wheel) {
        (true, _) => 8,
        (_, Wheel::Alloy) => 15,
        (_, Wheel::Sport) => 20,
        (_, Wheel::Steel) => 16,
    };
    let r = s.wheel_r;
    let hw = s.tyre_w * 0.5;
    let rim = r * s.rim;
    let hub = rim * 0.3;
    let rim_col = if s.wheel == Wheel::Steel {
        STEEL
    } else {
        ALLOY
    };
    let spoke = |k: usize| match s.wheel {
        Wheel::Alloy => k % 3 == 0,
        Wheel::Sport => k % 2 == 0,
        Wheel::Steel => true,
    };
    let pt = |k: usize, x: f32, rad: f32| {
        let a = (k % n) as f32 / n as f32 * TAU;
        Vec3::new(x, a.sin() * rad, a.cos() * rad)
    };
    let mut m = MB::default();
    let band = |m: &mut MB, k: usize, a: (f32, f32), b: (f32, f32), col: [f32; 4]| {
        m.poly(
            &[
                pt(k, a.0, a.1),
                pt(k + 1, a.0, a.1),
                pt(k + 1, b.0, b.1),
                pt(k, b.0, b.1),
            ],
            col,
        );
    };
    let fan = |m: &mut MB, k: usize, a: (f32, f32), c: f32, col: [f32; 4]| {
        m.poly(
            &[pt(k, a.0, a.1), pt(k + 1, a.0, a.1), Vec3::new(c, 0.0, 0.0)],
            col,
        );
    };
    for k in 0..n {
        if lod {
            // Far off: the tread and the outer face only.
            band(&mut m, k, (-hw, r), (hw, r), TYRE);
            let side = if detail[1] { 1.0 } else { -1.0 };
            band(&mut m, k, (side * hw, r), (side * hw, rim), TYRE);
            fan(&mut m, k, (side * hw, rim), side * (hw - 0.03), rim_col);
            continue;
        }
        let tread = [
            (-hw, r * 0.9),
            (-hw * 0.72, r),
            (hw * 0.72, r),
            (hw, r * 0.9),
        ];
        for p in tread.windows(2) {
            band(&mut m, k, p[0], p[1], TYRE);
        }
        for (side, det) in [(-1.0f32, detail[0]), (1.0, detail[1])] {
            let x = |d: f32| side * (hw - d);
            band(&mut m, k, (x(0.0), r * 0.9), (x(0.0), rim), TYRE);
            if !det {
                fan(&mut m, k, (x(0.0), rim), x(0.03), UNDER);
                continue;
            }
            band(&mut m, k, (x(0.0), rim), (x(0.008), rim * 0.93), rim_col);
            band(
                &mut m,
                k,
                (x(0.008), rim * 0.93),
                (x(0.03), rim * 0.86),
                rim_col,
            );
            if spoke(k) {
                band(&mut m, k, (x(0.03), rim * 0.86), (x(0.03), hub), rim_col);
            } else {
                band(
                    &mut m,
                    k,
                    (x(0.03), rim * 0.86),
                    (x(0.075), rim * 0.86),
                    UNDER,
                );
                band(&mut m, k, (x(0.075), rim * 0.86), (x(0.075), hub), UNDER);
                band(&mut m, k, (x(0.075), hub), (x(0.03), hub), UNDER);
            }
            // The sides of each spoke.
            for e in [k, k + 1] {
                let other = if e == k { (k + n - 1) % n } else { (k + 1) % n };
                if spoke(k) && !spoke(other) {
                    m.poly(
                        &[
                            pt(e, x(0.03), rim * 0.86),
                            pt(e, x(0.03), hub),
                            pt(e, x(0.075), hub),
                            pt(e, x(0.075), rim * 0.86),
                        ],
                        rim_col,
                    );
                }
            }
            band(
                &mut m,
                k,
                (x(0.03), hub),
                (x(0.022), hub * 0.6),
                if s.wheel == Wheel::Steel {
                    CHROME
                } else {
                    rim_col
                },
            );
            fan(
                &mut m,
                k,
                (x(0.022), hub * 0.6),
                x(0.018),
                if s.wheel == Wheel::Steel {
                    CHROME
                } else {
                    rim_col
                },
            );
        }
    }
    m.smooth(0.8);
    m
}

/// A traffic model: body and wheels in one mesh.
pub(crate) fn traffic(s: &Spec, lod: bool) -> MB {
    let mut m = body(s, lod);
    let w = wheel(s, [false, true], lod);
    for &af in s.axles {
        let z = (af - 0.5) * s.len;
        let x = s.wheel_x(af);
        m.append(&w, Vec3::new(x, s.wheel_r, z), false);
        m.append(&w, Vec3::new(-x, s.wheel_r, z), true);
    }
    m
}

/// The eight models, in `cars::CAR_LEN` order: sedan, hatchback, SUV,
/// pickup, van, sports car, bus, box truck. Dimensions follow typical cars of
/// each class (e.g. a 4.7 m mid-size saloon on a 2.8 m wheelbase); they are
/// generic shapes, not any particular make.
pub(crate) fn specs() -> [Spec; 8] {
    let base = Spec {
        len: 4.7,
        w: 0.91,
        h: 1.44,
        clear: 0.16,
        belt: 0.94,
        rise: 0.06,
        nose: 0.68,
        deck: 0.99,
        tail: 0.86,
        hood_end: 0.31,
        roof0: 0.44,
        roof1: 0.66,
        deck0: 0.8,
        tumble: 0.15,
        ledge: 0.025,
        sill: 0.035,
        header: 0.03,
        wheel_r: 0.32,
        tyre_w: 0.215,
        rim: 0.6,
        wheel: Wheel::Alloy,
        axles: &[0.2, 0.795],
        arch_gap: 0.05,
        windows: vec![(0.335, 0.54), (0.553, 0.715)],
        seams: vec![0.306, 0.547, 0.712],
        handles: vec![0.505, 0.685],
        lamp_y: 0.6,
        tail_y: 0.77,
        boxy: false,
        clad: false,
        chrome: false,
        flat_front: false,
        bed: None,
        cargo: None,
        rear_glass: true,
        rails: false,
        spoiler: false,
        bus: false,
        exhausts: 1,
    };
    let bus_windows = vec![
        (0.012, 0.115),
        (0.13, 0.225),
        (0.237, 0.332),
        (0.344, 0.455),
        (0.467, 0.553),
        (0.565, 0.665),
        (0.677, 0.777),
        (0.789, 0.884),
        (0.896, 0.975),
    ];
    [
        // Sedan.
        base.clone(),
        // Hatchback.
        Spec {
            len: 4.0,
            w: 0.88,
            h: 1.48,
            clear: 0.15,
            belt: 0.95,
            nose: 0.66,
            deck: 1.0,
            tail: 0.95,
            hood_end: 0.28,
            roof0: 0.42,
            roof1: 0.84,
            deck0: 0.975,
            tumble: 0.14,
            wheel_r: 0.31,
            tyre_w: 0.2,
            axles: &[0.185, 0.82],
            windows: vec![(0.31, 0.545), (0.557, 0.76), (0.772, 0.87)],
            seams: vec![0.29, 0.551, 0.766],
            handles: vec![0.51, 0.735],
            lamp_y: 0.58,
            tail_y: 0.86,
            ..base.clone()
        },
        // SUV.
        Spec {
            w: 0.95,
            h: 1.72,
            clear: 0.22,
            belt: 1.1,
            nose: 0.9,
            deck: 1.15,
            tail: 1.08,
            hood_end: 0.28,
            roof0: 0.4,
            roof1: 0.92,
            deck0: 0.98,
            tumble: 0.11,
            wheel_r: 0.37,
            tyre_w: 0.235,
            rim: 0.62,
            axles: &[0.19, 0.79],
            arch_gap: 0.06,
            windows: vec![(0.315, 0.535), (0.548, 0.735), (0.748, 0.935)],
            seams: vec![0.296, 0.541, 0.742],
            handles: vec![0.505, 0.71],
            lamp_y: 0.8,
            tail_y: 1.02,
            clad: true,
            rails: true,
            ..base.clone()
        },
        // Pickup: crew cab and an open bed.
        Spec {
            len: 5.4,
            w: 0.98,
            h: 1.85,
            clear: 0.3,
            belt: 1.17,
            rise: 0.03,
            nose: 1.02,
            deck: 1.19,
            tail: 1.19,
            hood_end: 0.27,
            roof0: 0.345,
            roof1: 0.53,
            deck0: 0.545,
            tumble: 0.1,
            wheel_r: 0.4,
            tyre_w: 0.26,
            rim: 0.55,
            axles: &[0.17, 0.795],
            arch_gap: 0.045,
            windows: vec![(0.29, 0.415), (0.427, 0.535)],
            seams: vec![0.278, 0.421, 0.54],
            handles: vec![0.395, 0.51],
            lamp_y: 0.92,
            tail_y: 1.02,
            chrome: true,
            bed: Some(0.87),
            exhausts: 1,
            ..base.clone()
        },
        // Panel van.
        Spec {
            len: 5.2,
            w: 0.98,
            h: 2.05,
            clear: 0.2,
            belt: 1.08,
            rise: 0.0,
            nose: 0.86,
            tail: 2.0,
            hood_end: 0.1,
            roof0: 0.23,
            roof1: 0.97,
            deck0: 1.0,
            tumble: 0.06,
            ledge: 0.0,
            header: 0.06,
            wheel_r: 0.35,
            tyre_w: 0.22,
            rim: 0.55,
            wheel: Wheel::Steel,
            axles: &[0.18, 0.79],
            windows: vec![(0.13, 0.285)],
            seams: vec![0.118, 0.292, 0.31, 0.525],
            handles: vec![0.27, 0.335],
            lamp_y: 0.76,
            tail_y: 0.9,
            boxy: true,
            clad: true,
            rear_glass: false,
            exhausts: 0,
            ..base.clone()
        },
        // Sports car.
        Spec {
            len: 4.4,
            w: 0.96,
            h: 1.2,
            clear: 0.11,
            belt: 0.8,
            nose: 0.58,
            deck: 0.86,
            tail: 0.82,
            hood_end: 0.37,
            roof0: 0.5,
            roof1: 0.6,
            deck0: 0.86,
            tumble: 0.24,
            wheel_r: 0.34,
            tyre_w: 0.27,
            rim: 0.7,
            arch_gap: 0.035,
            wheel: Wheel::Sport,
            axles: &[0.2, 0.79],
            windows: vec![(0.39, 0.65)],
            seams: vec![0.36, 0.665],
            handles: vec![0.635],
            lamp_y: 0.5,
            tail_y: 0.73,
            spoiler: true,
            exhausts: 2,
            ..base.clone()
        },
        // City bus.
        Spec {
            len: 11.5,
            w: 1.27,
            h: 3.05,
            clear: 0.3,
            belt: 1.15,
            rise: 0.0,
            nose: 2.97,
            tail: 2.97,
            hood_end: 0.0,
            roof0: 0.012,
            roof1: 0.988,
            deck0: 1.0,
            tumble: 0.03,
            ledge: 0.0,
            sill: 0.05,
            header: 0.45,
            wheel_r: 0.5,
            tyre_w: 0.3,
            rim: 0.6,
            wheel: Wheel::Steel,
            axles: &[0.225, 0.745],
            arch_gap: 0.07,
            windows: bus_windows,
            seams: vec![],
            handles: vec![],
            lamp_y: 0.65,
            tail_y: 0.9,
            boxy: true,
            flat_front: true,
            rear_glass: false,
            bus: true,
            exhausts: 0,
            ..base.clone()
        },
        // Box truck: a cab-over with a box body.
        Spec {
            len: 7.6,
            w: 1.1,
            h: 2.6,
            clear: 0.38,
            belt: 1.55,
            rise: 0.0,
            nose: 2.5,
            deck: 1.08,
            tail: 1.08,
            hood_end: 0.0,
            roof0: 0.03,
            roof1: 0.215,
            deck0: 0.225,
            tumble: 0.05,
            ledge: 0.0,
            header: 0.12,
            wheel_r: 0.46,
            tyre_w: 0.25,
            rim: 0.56,
            wheel: Wheel::Steel,
            axles: &[0.14, 0.72],
            arch_gap: 0.07,
            windows: vec![(0.02, 0.2)],
            seams: vec![0.015, 0.205],
            handles: vec![0.185],
            lamp_y: 0.78,
            tail_y: 0.85,
            boxy: true,
            clad: true,
            flat_front: true,
            cargo: Some(2.3),
            rear_glass: false,
            exhausts: 0,
            ..base
        },
    ]
}
