//! What an airfield is drawn with: pavement and paint as decals draped over
//! the (levelled) terrain, the lights, and the fixed objects around it.
//! See airfield.wgsl and lights.wgsl for how they are shaded.
use crate::airmodels as am;
use crate::airports::{hold_dist, right_of, Geo, Runway};
use bytemuck::{Pod, Zeroable};
use glam::{Vec2, Vec3};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct AirVertex {
    pub pos: [f32; 3],
    /// runway / taxiway: across, along, half width, length. Apron: slab
    /// coordinates. Paint: u, v, and the size of the patch.
    pub q: [f32; 4],
    pub r: [f32; 4],
    pub kind: f32,
    /// local position (east, north metres from the reference point)
    pub loc: [f32; 2],
}

pub const K_RUNWAY: f32 = 0.0;
pub const K_TAXI: f32 = 1.0;
pub const K_APRON: f32 = 2.0;
pub const K_PAINT: f32 = 3.0;

/// One light. `dir` zero: seen from all round; otherwise it shines along
/// `dir`, and `spread` is the cosine of the angle at which it has faded out.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Light {
    pub pos: [f32; 3],
    pub inten: f32,
    pub dir: [f32; 3],
    pub spread: f32,
    /// rgb, a = kind: 0 plain, 1 PAPI (`spread` is then its transition angle, rad), 2 floodlight
    pub col: [u8; 4],
    /// flash period (1/10 s, 0 = steady), phase and duty (/255), 1 = shows by day
    pub fx: [u8; 4],
}

pub const L_WHITE: [u8; 3] = [255, 232, 190];
pub const L_STROBE: [u8; 3] = [230, 238, 255];
pub const L_GREEN: [u8; 3] = [30, 255, 110];
pub const L_RED: [u8; 3] = [255, 28, 20];
pub const L_BLUE: [u8; 3] = [40, 80, 255];
pub const L_YELLOW: [u8; 3] = [255, 180, 40];
pub const L_FLOOD: [u8; 3] = [255, 226, 180];

/// A fixed object: an instanced model with its axes (scaled) in the planet frame.
pub struct Prop {
    pub mesh: u32,
    pub pos: Vec3,
    pub x: Vec3,
    pub y: Vec3,
    pub z: Vec3,
    pub tint: [f32; 3],
}

#[derive(Default)]
pub struct FieldMesh {
    pub verts: Vec<AirVertex>,
    pub idx: Vec<u32>,
    pub lights: Vec<Light>,
    pub props: Vec<Prop>,
    /// windsocks: where the sock hangs (relative to the airfield's centre)
    pub socks: Vec<Vec3>,
    /// jet bridge head per stand: where the aircraft's door goes (local)
    pub doors: Vec<Option<Vec2>>,
    /// a hand-made scenery pack's objects, placed (scenery.rs)
    pub scenery: Vec<crate::scenery::Placed>,
    /// floodlights that light the ground (common.wgsl `flood_light`): lamp
    /// (relative to the airfield's centre), beam direction
    pub floods: Vec<(Vec3, Vec3)>,
}

struct B<'a> {
    g: &'a Geo,
    m: FieldMesh,
    /// taxiway segments by grid cell, for "is this spot pavement"
    cells: std::collections::HashMap<(i32, i32), Vec<(Vec2, Vec2, f32)>>,
    boxes: Vec<(Vec2, Vec2)>,
}
const LIFT: f32 = 0.06;
const CELL: f32 = 120.0;

impl<'a> B<'a> {
    fn v(&mut self, p: Vec2, lift: f32, q: [f32; 4], r: [f32; 4], kind: f32) -> u32 {
        self.m.verts.push(AirVertex { pos: self.g.rel(p, lift).into(), q, r, kind, loc: p.into() });
        self.m.verts.len() as u32 - 1
    }
    fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.m.idx.extend([a, b, c, a, c, d]);
    }
    /// A ribbon along a polyline: `strips` quads across, cut every `step` metres.
    fn ribbon(&mut self, pts: &[Vec2], half: f32, strips: usize, step: f32, lift: f32, kind: f32, r: [f32; 4], qw: f32) {
        // Resample so no stretch is longer than `step`.
        let mut line: Vec<Vec2> = vec![pts[0]];
        for w in pts.windows(2) {
            let n = ((w[1] - w[0]).length() / step).ceil().max(1.0) as usize;
            for k in 1..=n {
                line.push(w[0] + (w[1] - w[0]) * (k as f32 / n as f32));
            }
        }
        line.dedup_by(|a, b| (*a - *b).length() < 0.05);
        if line.len() < 2 {
            return;
        }
        let mut along = 0.0;
        let mut prev: Option<u32> = None;
        for k in 0..line.len() {
            let d0 = if k > 0 { (line[k] - line[k - 1]).normalize_or_zero() } else { Vec2::ZERO };
            let d1 = if k + 1 < line.len() { (line[k + 1] - line[k]).normalize_or_zero() } else { Vec2::ZERO };
            let d = (d0 + d1).normalize_or_zero();
            // Mitre: wider where the line turns, so the edges stay parallel.
            let cosh = d.dot(if d1 == Vec2::ZERO { d0 } else { d1 }).max(0.5);
            let side = right_of(d) / cosh;
            if k > 0 {
                along += (line[k] - line[k - 1]).length();
            }
            let first = self.m.verts.len() as u32;
            for s in 0..=strips {
                let x = (s as f32 / strips as f32 * 2.0 - 1.0) * half;
                self.v(line[k] + side * x, lift, [x, along, half, qw], r, kind);
            }
            if let Some(p) = prev {
                for s in 0..strips as u32 {
                    self.quad(p + s, p + s + 1, first + s + 1, first + s);
                }
            }
            prev = Some(first);
        }
    }
    /// A painted bar from `a` to `b`, `w` wide: colour 0 white, 1 yellow, 2 red, 3 black.
    fn bar(&mut self, a: Vec2, b: Vec2, w: f32, col: f32, pattern: f32, caps: bool) {
        let len = (b - a).length();
        if len < 1e-3 {
            return;
        }
        let d = (b - a) / len;
        let side = right_of(d) * w * 0.5;
        let ext = if caps { w * 0.5 } else { 0.0 };
        let total = len + 2.0 * ext;
        let n = (total / 14.0).ceil().max(1.0) as usize;
        let mut prev: Option<u32> = None;
        for k in 0..=n {
            let u = total * k as f32 / n as f32;
            let p = a + d * (u - ext);
            let i0 = self.v(p - side, LIFT + 0.02, [u, 0.0, total, w], [col, pattern, 0.0, 0.0], K_PAINT);
            self.v(p + side, LIFT + 0.02, [u, w, total, w], [col, pattern, 0.0, 0.0], K_PAINT);
            if let Some(p0) = prev {
                self.quad(p0, p0 + 1, i0 + 1, i0);
            }
            prev = Some(i0);
        }
    }
    /// Is a spot on pavement (taxiway, runway or apron), with a margin?
    fn paved(&self, p: Vec2, margin: f32, skip_taxi: Option<(Vec2, Vec2)>) -> bool {
        if self.g.on_runway(p, margin).is_some() {
            return true;
        }
        let c = ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32);
        for dy in -1..=1 {
            for dx in -1..=1 {
                for (a, b, half) in self.cells.get(&(c.0 + dx, c.1 + dy)).map(|v| &v[..]).unwrap_or(&[]) {
                    if skip_taxi.is_some_and(|s| (s.0 - *a).length() < 0.01 && (s.1 - *b).length() < 0.01) {
                        continue;
                    }
                    let d = *b - *a;
                    let t = ((p - *a).dot(d) / d.length_squared().max(1e-6)).clamp(0.0, 1.0);
                    if (*a + d * t - p).length() < half + margin {
                        return true;
                    }
                }
            }
        }
        self.g.aprons.iter().zip(&self.boxes).any(|(a, bb)| p.cmpge(bb.0).all() && p.cmple(bb.1).all() && in_poly(p, a))
    }
    fn light(&mut self, p: Vec2, lift: f32, inten: f32, col: [u8; 3], kind: u8, dir: Vec3, spread: f32, fx: [u8; 4]) {
        let d = if dir == Vec3::ZERO { Vec3::ZERO } else { self.g.dir3(dir).normalize() };
        self.m.lights.push(Light { pos: self.g.rel(p, lift).into(), inten, dir: d.into(), spread, col: [col[0], col[1], col[2], kind], fx });
    }
    fn prop(&mut self, mesh: u32, p: Vec2, lift: f32, fwd: Vec2, scale: Vec3, tint: [f32; 3]) {
        let g = self.g;
        let f = g.dir3(Vec3::new(fwd.x, fwd.y, 0.0));
        let up = g.dir3(Vec3::Z);
        let right = f.cross(up);
        self.m.props.push(Prop { mesh, pos: g.rel(p, lift), x: right * scale.x, y: up * scale.y, z: -f * scale.z, tint });
    }
}

fn in_poly(p: Vec2, rings: &[Vec<Vec2>]) -> bool {
    let mut c = false;
    for r in rings {
        let mut j = r.len() - 1;
        for i in 0..r.len() {
            let (a, b) = (r[i], r[j]);
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                c = !c;
            }
            j = i;
        }
    }
    c
}

/// Strokes of the runway numerals and letters on a grid 2 wide and 6 tall.
fn glyph(c: char) -> &'static [&'static [[f32; 2]]] {
    match c {
        '0' => &[&[[0.5, 0.0], [1.5, 0.0], [2.0, 0.6], [2.0, 5.4], [1.5, 6.0], [0.5, 6.0], [0.0, 5.4], [0.0, 0.6], [0.5, 0.0]]],
        '1' => &[&[[1.0, 0.0], [1.0, 6.0], [0.35, 5.1]]],
        '2' => &[&[[0.0, 5.3], [0.5, 6.0], [1.5, 6.0], [2.0, 5.3], [2.0, 3.9], [0.0, 1.1], [0.0, 0.0], [2.0, 0.0]]],
        '3' => &[&[[0.0, 6.0], [2.0, 6.0], [0.8, 3.6], [1.5, 3.6], [2.0, 3.0], [2.0, 0.6], [1.5, 0.0], [0.5, 0.0], [0.0, 0.6]]],
        '4' => &[&[[1.55, 0.0], [1.55, 6.0], [0.0, 1.9], [2.0, 1.9]]],
        '5' => &[&[[2.0, 6.0], [0.0, 6.0], [0.0, 3.5], [1.5, 3.5], [2.0, 2.9], [2.0, 0.6], [1.5, 0.0], [0.5, 0.0], [0.0, 0.6]]],
        '6' => &[&[[2.0, 5.4], [1.5, 6.0], [0.5, 6.0], [0.0, 5.4], [0.0, 0.6], [0.5, 0.0], [1.5, 0.0], [2.0, 0.6], [2.0, 2.8], [1.5, 3.4], [0.0, 3.4]]],
        '7' => &[&[[0.0, 6.0], [2.0, 6.0], [0.6, 0.0]]],
        '8' => &[&[[0.5, 3.2], [0.0, 2.7], [0.0, 0.6], [0.5, 0.0], [1.5, 0.0], [2.0, 0.6], [2.0, 2.7], [1.5, 3.2], [0.5, 3.2], [0.0, 3.7], [0.0, 5.4], [0.5, 6.0], [1.5, 6.0], [2.0, 5.4], [2.0, 3.7], [1.5, 3.2]]],
        '9' => &[&[[0.0, 0.6], [0.5, 0.0], [1.5, 0.0], [2.0, 0.6], [2.0, 5.4], [1.5, 6.0], [0.5, 6.0], [0.0, 5.4], [0.0, 3.2], [0.5, 2.6], [2.0, 2.6]]],
        'L' => &[&[[0.0, 6.0], [0.0, 0.0], [2.0, 0.0]]],
        'C' => &[&[[2.0, 5.4], [1.5, 6.0], [0.5, 6.0], [0.0, 5.4], [0.0, 0.6], [0.5, 0.0], [1.5, 0.0], [2.0, 0.6]]],
        'R' => &[&[[0.0, 0.0], [0.0, 6.0], [1.5, 6.0], [2.0, 5.4], [2.0, 3.7], [1.5, 3.1], [0.0, 3.1]], &[[1.0, 3.1], [2.0, 0.0]]],
        'H' => &[&[[0.0, 0.0], [0.0, 6.0]], &[[2.0, 0.0], [2.0, 6.0]], &[[0.0, 3.0], [2.0, 3.0]]],
        _ => &[],
    }
}

/// Where the paint starts and stops near a threshold, in metres from it,
/// for a runway of this class and width: (piano keys from, to, letter base,
/// number base, number top, centreline start). Must match airfield.wgsl.
pub fn mark_layout(r: &Runway) -> [f32; 6] {
    let ks = r.mark_scale();
    let (k0, k1) = if r.class >= 1 { (6.0 * ks, 51.0 * ks) } else { (0.0, 0.0) };
    let yl = if r.class >= 1 { k1 + 12.0 * ks } else { 6.0 * ks };
    let yn = if r.has_letter() { yl + 24.0 * ks } else { yl };
    [k0, k1, yl, yn, yn + 18.0 * ks, yn + 30.0 * ks]
}

pub fn build(g: &Geo) -> FieldMesh {
    let apt = g.apt();
    let mut b = B { g, m: FieldMesh::default(), cells: Default::default(), boxes: Vec::new() };
    for (pts, w) in &g.taxiways {
        for s in pts.windows(2) {
            let (lo, hi) = (s[0].min(s[1]) - *w, s[0].max(s[1]) + *w);
            for cy in (lo.y / CELL).floor() as i32..=(hi.y / CELL).floor() as i32 {
                for cx in (lo.x / CELL).floor() as i32..=(hi.x / CELL).floor() as i32 {
                    b.cells.entry((cx, cy)).or_default().push((s[0], s[1], *w * 0.5));
                }
            }
        }
    }
    b.boxes = g.aprons.iter().map(|a| a[0].iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)))).collect();
    let concrete_field = g.runways.iter().filter(|r| r.paved).any(|r| r.concrete);
    let concrete_apron = concrete_field || apt.kind == 2;
    let any_paved = g.runways.iter().any(|r| r.paved);
    let lit_field = apt.kind >= 1 || g.runways.iter().any(|r| r.lighted);
    // An even glow over the aprons at night, unless a scenery pack's masts light them.
    let flood = if apt.kind >= 1 && crate::scenery::for_airport(&apt.ident).is_none() { 1.0 } else { 0.0 };

    // ---- aprons: slabs laid along each apron's longest side
    for (ai, a) in g.aprons.iter().enumerate() {
        if !any_paved {
            break;
        }
        let r0 = &a[0];
        let (mut best, mut axis) = (0.0, Vec2::X);
        for i in 0..r0.len() {
            let d = r0[(i + 1) % r0.len()] - r0[i];
            if d.length() > best {
                best = d.length();
                axis = d / best;
            }
        }
        let side = right_of(axis);
        let mut flat: Vec<f64> = Vec::new();
        let mut holes = Vec::new();
        let mut pts: Vec<Vec2> = Vec::new();
        for (k, ring) in a.iter().enumerate() {
            if k > 0 {
                holes.push(pts.len());
            }
            for p in ring {
                flat.extend([p.x as f64, p.y as f64]);
                pts.push(*p);
            }
        }
        let Ok(tris) = earcutr::earcut(&flat, &holes, 2) else { continue };
        let seed = (ai as f32 * 0.377).fract();
        // Split until no edge is long enough to cut through the ground it lies on.
        let mut stack: Vec<[Vec2; 3]> = tris.chunks(3).map(|t| [pts[t[0]], pts[t[1]], pts[t[2]]]).collect();
        let mut guard = 0;
        while let Some(t) = stack.pop() {
            guard += 1;
            let l = [(t[1] - t[0]).length(), (t[2] - t[1]).length(), (t[0] - t[2]).length()];
            let k = (0..3).max_by(|x, y| l[*x].total_cmp(&l[*y])).unwrap();
            if l[k] > 36.0 && guard < 60_000 {
                let (p, q, o) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
                let mid = (p + q) * 0.5;
                stack.push([p, mid, o]);
                stack.push([mid, q, o]);
                continue;
            }
            let ids = t.map(|p| b.v(p, LIFT - 0.03, [p.dot(axis), p.dot(side), flood, seed], [if concrete_apron { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0], K_APRON));
            b.m.idx.extend(ids);
        }
    }
    // ---- taxiways, then runways over them, then the paint over both
    if any_paved {
        for (ti, (pts, w)) in g.taxiways.iter().enumerate() {
            b.ribbon(pts, w * 0.5, 2, 12.0, LIFT - 0.02, K_TAXI, [if concrete_field { 1.0 } else { 0.0 }, 0.0, 0.0, (ti as f32 * 0.137).fract()], 0.0);
        }
    }
    if any_paved {
            // Rounded joins where taxiways meet or end, so corners are not cut.
        let tw = crate::airports::taxi_width(apt.kind);
        for nd in &g.graph.nodes {
            let taxi = nd.edges.iter().filter(|e| g.graph.edges[**e as usize].rwy.is_none()).count();
            if taxi == 0 || (taxi == 2 && nd.edges.len() == 2) || g.on_runway(nd.p, tw * 0.3).is_some() {
                continue;
            }
            let rad = tw * if nd.edges.len() >= 3 { 0.62 } else { 0.5 };
            let c = b.v(nd.p, LIFT - 0.025, [0.0, 0.0, rad, 0.0], [if concrete_field { 1.0 } else { 0.0 }, 1.0, 0.0, 0.0], K_TAXI);
            let n = 16;
            for k in 0..=n {
                let a = k as f32 / n as f32 * std::f32::consts::TAU;
                let i = b.v(nd.p + Vec2::new(a.cos(), a.sin()) * rad, LIFT - 0.025, [rad, 0.0, rad, 0.0], [if concrete_field { 1.0 } else { 0.0 }, 1.0, 0.0, 0.0], K_TAXI);
                if k > 0 {
                    b.m.idx.extend([c, i - 1, i]);
                }
            }
        }
    }
    // ---- concrete pads a scenery pack has seen on the orthophoto, over the
    // aprons and the taxilanes across them (drawn after both, and above:
    // on concrete a taxilane is only its paint), slabs along their own direction
    if any_paved && let Some(pack) = crate::scenery::for_airport(&apt.ident) {
        for (pi, gp) in pack.ground.iter().enumerate().filter(|(_, gp)| gp.kind == crate::scenery::G_CONCRETE) {
            let rings: Vec<Vec<Vec2>> = gp.rings.iter().map(|r| r.iter().map(|p| pack.to_field(g, *p)).collect()).collect();
            let p0 = rings[0][0];
            let axis = (pack.to_field(g, gp.rings[0][0] + Vec2::new(gp.axis.cos(), gp.axis.sin()) * 10.0) - p0).normalize_or(Vec2::X);
            let side = right_of(axis);
            let seed = (pi as f32 * 0.613).fract();
            let mut flat: Vec<f64> = Vec::new();
            let mut holes = Vec::new();
            let mut pts: Vec<Vec2> = Vec::new();
            for (k, ring) in rings.iter().enumerate() {
                if k > 0 {
                    holes.push(pts.len());
                }
                for p in ring {
                    flat.extend([p.x as f64, p.y as f64]);
                    pts.push(*p);
                }
            }
            let Ok(tris) = earcutr::earcut(&flat, &holes, 2) else { continue };
            let mut stack: Vec<[Vec2; 3]> = tris.chunks(3).map(|t| [pts[t[0]], pts[t[1]], pts[t[2]]]).collect();
            let mut guard = 0;
            while let Some(t) = stack.pop() {
                guard += 1;
                let l = [(t[1] - t[0]).length(), (t[2] - t[1]).length(), (t[0] - t[2]).length()];
                let k = (0..3).max_by(|x, y| l[*x].total_cmp(&l[*y])).unwrap();
                if l[k] > 36.0 && guard < 60_000 {
                    let (p, q, o) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
                    let mid = (p + q) * 0.5;
                    stack.push([p, mid, o]);
                    stack.push([mid, q, o]);
                    continue;
                }
                let ids = t.map(|p| b.v(p, LIFT - 0.015, [p.dot(axis), p.dot(side), flood, seed], [1.0, 0.0, 0.0, 0.0], K_APRON));
                b.m.idx.extend(ids);
            }
        }
    }

    for (ri, r) in g.runways.iter().enumerate() {
        if !r.paved {
            continue;
        }
        let flags = r.class as f32 + if r.concrete { 4.0 } else { 0.0 } + if r.has_letter() { 8.0 } else { 0.0 };
        b.ribbon(&[r.a, r.b], r.width * 0.5, 4, 10.0, LIFT, K_RUNWAY, [r.thr[0], r.thr[1], flags, (ri as f32 * 0.31 + 0.1).fract()], r.len);
    }
    if any_paved {
        // Yellow centrelines, stopping at the runway edge.
        for (pts, _) in &g.taxiways {
            let mut run: Option<Vec2> = None;
            let mut last = pts[0];
            for w in pts.windows(2) {
                let n = ((w[1] - w[0]).length() / 4.0).ceil().max(1.0) as usize;
                for k in 0..=n {
                    let p = w[0] + (w[1] - w[0]) * (k as f32 / n as f32);
                    let on = g.on_runway(p, 1.0).is_none();
                    match (run, on) {
                        (None, true) => run = Some(p),
                        (Some(a), false) => {
                            b.bar(a, last, 0.16, 1.0, 0.0, false);
                            run = None;
                        }
                        (Some(a), true) if (p - a).length() > 24.0 => {
                            b.bar(a, p, 0.16, 1.0, 0.0, true);
                            run = Some(p);
                        }
                        _ => {}
                    }
                    last = p;
                }
            }
            if let Some(a) = run {
                b.bar(a, last, 0.16, 1.0, 0.0, true);
            }
        }
    }
    for r in g.runways.iter().filter(|r| r.paved) {
        let ks = r.mark_scale();
        let lay = mark_layout(r);
        for e in 0..2 {
            let (thr, dir) = r.end(e);
            let right = right_of(dir);
            let id = &r.ident[e];
            let digits: Vec<char> = id.chars().filter(|c| c.is_ascii_digit()).collect();
            let letter = id.chars().last().filter(|c| matches!(c, 'L' | 'R' | 'C') && id.len() > 1);
            let u = 3.0 * ks;
            let draw = |b: &mut B, chars: &[char], base: f32| {
                let total = chars.len() as f32 * 2.0 * u + (chars.len() as f32 - 1.0) * 1.5 * u;
                for (k, c) in chars.iter().enumerate() {
                    let x0 = -total * 0.5 + k as f32 * 3.5 * u;
                    for stroke in glyph(*c) {
                        for s in stroke.windows(2) {
                            let p = |g: [f32; 2]| thr + dir * (base + g[1] * u) + right * (x0 + g[0] * u);
                            b.bar(p(s[0]), p(s[1]), 0.5 * u, 0.0, 0.0, true);
                        }
                    }
                }
            };
            if let Some(l) = letter {
                draw(&mut b, &[l], lay[2]);
            }
            if !digits.is_empty() {
                draw(&mut b, &digits, lay[3]);
            }
        }
    }
    // Holding positions: two solid and two broken yellow lines on black.
    for h in &g.holds {
        let side = right_of(h.dir) * h.half;
        b.bar(h.p - side, h.p + side, 3.3, 3.0, 0.0, false);
        b.bar(h.p - side, h.p + side, 2.1, 1.0, 1.0, false);
    }
    // Stand stop bars.
    if any_paved {
        for s in g.stands.iter().filter(|s| s.terminal) {
            let side = right_of(s.dir) * 3.0;
            b.bar(s.pos - side, s.pos + side, 0.3, 1.0, 0.0, false);
        }
    }
    // Helipads: a circle and an H.
    for (c, rad) in &g.helipads {
        let r = rad.min(12.0) * 0.8;
        for k in 0..24 {
            let (a0, a1) = (k as f32 / 24.0 * std::f32::consts::TAU, (k + 1) as f32 / 24.0 * std::f32::consts::TAU);
            b.bar(*c + Vec2::new(a0.cos(), a0.sin()) * r, *c + Vec2::new(a1.cos(), a1.sin()) * r, 0.4, 0.0, 0.0, true);
        }
        let u = r * 0.16;
        for stroke in glyph('H') {
            let p = |g: [f32; 2]| *c + Vec2::new((g[0] - 1.0) * u * 1.6, (g[1] - 3.0) * u);
            b.bar(p(stroke[0]), p(stroke[1]), 0.5 * u, 0.0, 0.0, true);
        }
    }

    lights(&mut b, lit_field);
    furniture(&mut b);
    // A hand-made scenery pack (scenery.rs): its objects, and its masts' floodlights.
    if let Some(pack) = crate::scenery::for_airport(&g.apt().ident) {
        b.m.scenery = pack.place(g);
        for (p, lift, d) in pack.flood_lights(g) {
            b.light(p, lift, 420.0, L_FLOOD, 2, d, -0.3, STEADY);
            b.m.floods.push((g.rel(p, lift), g.dir3(d).normalize()));
        }
    }
    b.m
}

const STEADY: [u8; 4] = [0, 0, 255, 0];
const DAY: [u8; 4] = [0, 0, 255, 1];

fn lights(b: &mut B, lit_field: bool) {
    let g = b.g;
    let apt = g.apt();
    for r in g.runways.iter().filter(|r| r.lighted) {
        let right = right_of(r.dir);
        let half = r.width * 0.5;
        // Edge lights every 60 m; over the last 600 m they show yellow to an
        // aircraft rolling toward that end.
        let n = (r.len / 60.0).round().max(2.0) as usize;
        let caution = if r.class >= 1 { 600f32.min(r.len * 0.33) } else { 0.0 };
        for k in 0..=n {
            let s = r.len * k as f32 / n as f32;
            for sx in [-1.0f32, 1.0] {
                let p = r.a + r.dir * s + right * sx * (half + 2.0);
                if s < caution || r.len - s < caution {
                    let toward_b = r.len - s < caution;
                    // Seen looking toward the near end: yellow. From the other side: white.
                    let d = Vec3::new(r.dir.x, r.dir.y, 0.0) * if toward_b { 1.0 } else { -1.0 };
                    b.light(p, 0.45, 26.0, L_YELLOW, 0, -d, -0.2, DAY);
                    b.light(p, 0.45, 26.0, L_WHITE, 0, d, -0.2, DAY);
                } else {
                    b.light(p, 0.45, 26.0, L_WHITE, 0, Vec3::ZERO, 0.0, DAY);
                }
            }
        }
        for e in 0..2 {
            let (thr, dir) = r.end(e);
            let rt = right_of(dir);
            let d3 = Vec3::new(dir.x, dir.y, 0.0);
            let end = if e == 0 { r.a } else { r.b };
            // Green across the threshold for those arriving, red across the
            // end for those running out of runway.
            let nt = ((r.width / 3.0) as usize).clamp(6, 18);
            for k in 0..nt {
                let x = (k as f32 / (nt - 1) as f32 * 2.0 - 1.0) * half;
                b.light(thr + rt * x - dir * 1.0, 0.25, 36.0, L_GREEN, 0, -d3 + Vec3::Z * 0.06, 0.1, DAY);
                b.light(end + rt * x - dir * 2.0, 0.25, 30.0, L_RED, 0, d3 + Vec3::Z * 0.06, 0.1, DAY);
            }
            if !r.paved {
                continue;
            }
            // PAPI: four units left of the runway beside the aiming point.
            let ap = if r.len >= 2400.0 { 400.0 } else if r.len >= 1200.0 { 300.0 } else { (r.len * 0.22).max(120.0) };
            for k in 0..4 {
                let p = thr + dir * ap - rt * (half + 15.0 + 9.0 * k as f32);
                let angle = (3.5f32 - 0.333 * k as f32).to_radians();
                b.light(p, 0.65, 150.0, L_WHITE, 1, -d3, angle, DAY);
                b.prop(am::M_PAPI, p, 0.0, -dir, Vec3::ONE, [1.0; 3]);
            }
            let thr_h = g.h(thr);
            // Out over the approach the lights stand on masts (or piers): level with the runway.
            let lift = |g: &Geo, p: Vec2, d: f32| (thr_h + 0.4 - 0.004 * d - g.h(p)).max(0.5);
            if r.class >= 2 {
                // Approach lights: bars on the centreline out to 420 m, a wide
                // bar at 300 m, and the "rabbit" running in from 900 m.
                for k in 1..=14 {
                    let d = 30.0 * k as f32;
                    let c = thr - dir * d;
                    for x in [-1.5f32, 0.0, 1.5] {
                        b.light(c + rt * x, lift(g, c, d), 70.0, L_WHITE, 0, -d3 + Vec3::Z * 0.1, 0.2, DAY);
                    }
                    if k == 10 {
                        for x in [-13.5f32, -10.5, -7.5, -4.5, 4.5, 7.5, 10.5, 13.5] {
                            b.light(c + rt * x, lift(g, c, d), 70.0, L_WHITE, 0, -d3 + Vec3::Z * 0.1, 0.2, DAY);
                        }
                    }
                }
                for k in 0..8 {
                    let d = 480.0 + 60.0 * k as f32;
                    let c = thr - dir * d;
                    b.light(c, lift(g, c, d), 900.0, L_STROBE, 0, -d3 + Vec3::Z * 0.1, 0.3, [5, (255 - k * 28) as u8, 12, 1]);
                }
                // Touchdown zone: bars of three either side of the centreline.
                if apt.kind == 2 {
                    for k in 1..=(900f32.min(r.len * 0.4) / 60.0) as usize {
                        for sx in [-1.0f32, 1.0] {
                            for j in 0..3 {
                                b.light(thr + dir * (60.0 * k as f32) + rt * sx * (9.5 + 1.5 * j as f32), 0.08, 14.0, L_WHITE, 0, -d3 + Vec3::Z * 0.12, 0.15, STEADY);
                            }
                        }
                    }
                }
            } else {
                // Runway end identifier lights: a pair of strobes at the threshold.
                for sx in [-1.0f32, 1.0] {
                    b.light(thr + rt * sx * (half + 12.0), 0.8, 500.0, L_STROBE, 0, -d3 + Vec3::Z * 0.1, 0.0, [8, 0, 10, 1]);
                }
            }
        }
        // Centreline lights on the big runways: white, then red and white
        // alternating from 900 m to go, red for the last 300.
        if r.class >= 2 && r.len >= 2300.0 {
            let n = (r.len / 30.0) as usize;
            for k in 1..n {
                let s = k as f32 * 30.0;
                let p = r.a + r.dir * s;
                for (to_go, sgn) in [(r.len - s, 1.0f32), (s, -1.0)] {
                    let col = if to_go < 300.0 || (to_go < 900.0 && k % 2 == 0) { L_RED } else { L_WHITE };
                    // Shines back toward whoever is rolling this way.
                    b.light(p, 0.07, 12.0, col, 0, Vec3::new(r.dir.x, r.dir.y, 0.0) * -sgn + Vec3::Z * 0.1, 0.0, STEADY);
                }
            }
        }
    }
    if lit_field {
        // Taxiways: blue along the edges, and at the big airports green down the middle.
        let tw: Vec<(Vec<Vec2>, f32)> = g.taxiways.clone();
        for (pts, w) in &tw {
            let mut carry = 20.0;
            for s in pts.windows(2) {
                let len = (s[1] - s[0]).length();
                if len < 0.1 {
                    continue;
                }
                let d = (s[1] - s[0]) / len;
                let mut t = carry;
                while t < len {
                    let c = s[0] + d * t;
                    for sx in [-1.0f32, 1.0] {
                        let p = c + right_of(d) * sx * (w * 0.5 + 1.2);
                        if !b.paved(p, -0.8, Some((s[0], s[1]))) {
                            b.light(p, 0.35, 5.0, L_BLUE, 0, Vec3::ZERO, 0.0, STEADY);
                        }
                    }
                    if apt.kind == 2 && g.on_runway(c, 2.0).is_none() {
                        b.light(c, 0.06, 2.2, L_GREEN, 0, Vec3::ZERO, 0.0, STEADY);
                    }
                    t += 45.0;
                }
                carry = t - len;
            }
        }
    }
    if apt.kind >= 1 {
        // Runway guard lights flashing either side of each holding position.
        for h in &g.holds {
            for (sx, phase) in [(-1.0f32, 0u8), (1.0, 128)] {
                let p = h.p + right_of(h.dir) * sx * (h.half + 2.5);
                b.light(p, 0.6, 30.0, L_YELLOW, 0, Vec3::new(-h.dir.x, -h.dir.y, 0.15), 0.2, [10, phase, 115, 1]);
            }
        }
    }
    // The beacon: green and white, turn about.
    if g.runways.iter().any(|r| r.lighted) {
        let at = g.aprons.iter().max_by(|a, b| a[0].len().cmp(&b[0].len())).map(|a| a[0].iter().copied().sum::<Vec2>() / a[0].len() as f32).unwrap_or(Vec2::ZERO);
        b.light(at, 26.0, 2500.0, L_STROBE, 0, Vec3::ZERO, 0.0, [50, 0, 10, 0]);
        b.light(at, 26.0, 2500.0, L_GREEN, 0, Vec3::ZERO, 0.0, [50, 128, 10, 0]);
    }
}

fn furniture(b: &mut B) {
    let g = b.g;
    let apt = g.apt();
    // What a hand-made scenery pack brings of its own instead (scenery.rs).
    let pack_flags = crate::scenery::for_airport(&apt.ident).map_or(0, |p| p.flags);
    // Windsocks beside the touchdown end of the main runways.
    for r in g.runways.iter().take(if pack_flags & crate::scenery::F_SOCKS != 0 { 0 } else { 2 }) {
        for e in 0..2 {
            let (thr, dir) = r.end(e);
            let spot = [1.0f32, -1.0].iter().map(|sx| thr + dir * (r.len * 0.12).min(260.0) + right_of(dir) * *sx * (r.width * 0.5 + 45.0)).find(|p| !b.paved(*p, 12.0, None));
            if let Some(p) = spot {
                if b.m.socks.len() < 4 {
                    b.prop(am::M_SOCK_POLE, p, 0.0, Vec2::Y, Vec3::ONE, [1.0; 3]);
                    b.m.socks.push(g.rel(p, 6.0));
                    if r.lighted {
                        b.light(p, 6.8, 8.0, L_FLOOD, 0, Vec3::ZERO, 0.0, STEADY);
                    }
                }
            }
        }
    }
    // Jet bridges: from the gate on the terminal wall to where the door of
    // an aircraft on the stand will be.
    b.m.doors = vec![None; g.stands.len()];
    for (si, s) in g.stands.iter().enumerate() {
        if !s.terminal || s.size < 3 {
            continue;
        }
        let Some((gp, _)) = g.gates.iter().filter(|gt| (gt.0 - s.pos).length() < 95.0).min_by(|a, b| (a.0 - s.pos).length().total_cmp(&(b.0 - s.pos).length())) else { continue };
        let ty = am::TYPES[s.size as usize];
        // Passengers board on the left.
        let left = -right_of(s.dir);
        let door = s.pos - s.dir * (ty.nose - ty.door) + left * (ty.rf + 2.6);
        let run = door - *gp;
        let len = run.length();
        // The gate has to be on the boarding side, ahead of the wing.
        if !(9.0..75.0).contains(&len) || (*gp - s.pos).dot(left) < -4.0 {
            continue;
        }
        let d = run / len;
        let sill = ty.sill - 0.2;
        b.prop(am::M_BRIDGE, *gp, sill, d, Vec3::new(1.0, 1.0, len), [1.0; 3]);
        b.prop(am::M_BRIDGE_HEAD, door, sill, -left, Vec3::ONE, [1.0; 3]);
        b.m.doors[si] = Some(door - left * 2.6);
    }
    // Floodlight masts round the larger aprons.
    if apt.kind >= 1 && pack_flags & crate::scenery::F_MASTS == 0 {
        let mut n = 0;
        for a in &g.aprons {
            let r0 = &a[0];
            let area = (0..r0.len()).map(|i| r0[i].perp_dot(r0[(i + 1) % r0.len()])).sum::<f32>().abs() * 0.5;
            if area < 12_000.0 {
                continue;
            }
            let c = r0.iter().copied().sum::<Vec2>() / r0.len() as f32;
            let mut since = 60.0f32;
            for i in 0..r0.len() {
                let (p, q) = (r0[i], r0[(i + 1) % r0.len()]);
                let len = (q - p).length();
                let mut t = (150.0 - since).max(0.0);
                while t < len && n < 70 {
                    let at = p + (q - p) * (t / len);
                    let inward = (c - at).normalize_or_zero();
                    let spot = at - inward * 3.0;
                    if !b.paved(spot, 6.0, None) && !b.m.props.iter().any(|pr| pr.mesh == am::M_MAST && (pr.pos - g.rel(spot, 0.0)).length() < 100.0) {
                        b.prop(am::M_MAST, spot, 0.0, inward, Vec3::ONE, [1.0; 3]);
                        b.light(spot + inward * 0.5, 22.0, 420.0, L_FLOOD, 2, Vec3::new(inward.x, inward.y, -0.9), -0.3, STEADY);
                        n += 1;
                    }
                    t += 150.0;
                }
                since = (since + len) % 150.0;
            }
        }
    }
    let _ = hold_dist;
}
