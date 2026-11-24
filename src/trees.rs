//! Trees and undergrowth: procedurally grown models for ten kinds of tree and
//! five kinds of forest-floor plant (several distinct individuals of each),
//! and their placement from the satellite image, land cover, climate and
//! terrain. Every model is a solid part at five levels of detail plus a set
//! of leaf sprays ("cards") that are only drawn close to the camera. Beyond
//! `IMP_DIST` a tree is drawn as an impostor, a picture of its full model
//! (impostor.rs); the simpler solid models are then only for the shadows.
use crate::buildings::{greenness, hash, Ctx, Hoods, TreeChunk, ATLAS, RASTER};
use crate::render::{TreeInst, TreeVertex};
use crate::sources::*;
use glam::{Vec2, Vec3};
use std::f32::consts::{PI, TAU};

/// Kinds of tree, then kinds of undergrowth.
pub const SPECIES: usize = 15;
pub const TREE_SPECIES: usize = 10;
pub const VARIANTS: usize = 4;
/// Meshes per individual: five levels of detail of the solid model, then its leaf cards.
pub const LODS: usize = 6;
pub const CARDS: u32 = LODS as u32 - 1;
/// Each tree moves its LOD/impostor hand-over distances by up to `LOD_JITTER`
/// of its own, so a forest transitions a tree at a time rather than along a line.
/// Must match `LOD_JITTER` in trees.wgsl.
pub const LOD_JITTER: f32 = 0.12;
/// How far away leaf cards are drawn. Must match `CARD_RANGE` in trees.wgsl.
pub const CARD_RANGE: f32 = 150.0;
/// Beyond this (times the tree's own `tree_jit`) a standing tree is drawn as
/// an impostor, a picture of its full model (impostor.rs). Must match
/// `IMP_DIST` in trees.wgsl, and stay below the range of the leaf cards.
pub const IMP_DIST: f32 = 120.0;
/// A tree's own factor on its change-over distances, from its position.
/// Must match `tree_jit` in trees.wgsl.
pub fn tree_jit(a: [f32; 4]) -> f32 {
    let h = a[0].to_bits().wrapping_mul(0x9E3779B9) ^ a[2].to_bits().wrapping_mul(0x85EBCA6B);
    let h2 = (h ^ (h >> 15)).wrapping_mul(0x2C1B3C6D);
    1.0 + LOD_JITTER * (((h2 ^ (h2 >> 12)) & 0xFFFF) as f32 / 32767.5 - 1.0)
}
pub const OAK: usize = 0;
pub const MAPLE: usize = 1;
pub const SPRUCE: usize = 2;
pub const PINE: usize = 3;
pub const PALM: usize = 4;
pub const CYPRESS: usize = 5;
pub const BIRCH: usize = 6;
pub const TROPICAL: usize = 7;
pub const BUSH: usize = 8;
pub const GUM: usize = 9;
pub const SHRUB: usize = 10;
pub const FERN: usize = 11;
pub const SAPLING: usize = 12;
pub const GRASS: usize = 13;
pub const DEADWOOD: usize = 14;

/// Natural size of one tree model, for collisions.
#[derive(Copy, Clone, Default)]
pub struct TreeDim {
    pub height: f32,
    pub crown_r: f32,
    pub crown_base: f32,
    pub trunk_r: f32,
}

pub struct TreeMeshes {
    /// indexed by (species * VARIANTS + variant) * LODS + lod
    pub meshes: Vec<(Vec<TreeVertex>, Vec<u32>)>,
    pub dims: Vec<TreeDim>,
}

/// A forest-floor plant. Kept compact (12 bytes): there are far more of these
/// than trees and they are only ever drawn within a stone's throw of the camera.
#[derive(Copy, Clone)]
pub struct Under {
    /// centimetres from the centre of its cell
    pub off: [i16; 3],
    /// colour multiplier, 127 = neutral
    pub tint: [u8; 3],
    pub var: u8,
    pub yaw: u8,
    /// height scale * 100
    pub scale: u8,
}
impl Under {
    /// How far away (metres) this kind of plant is drawn.
    pub fn range(&self) -> f32 {
        match self.var as usize / VARIANTS {
            GRASS => 75.0,
            FERN => 95.0,
            _ => 125.0,
        }
    }
    pub fn pos(&self, cell: Vec3) -> Vec3 {
        cell + Vec3::new(self.off[0] as f32, self.off[1] as f32, self.off[2] as f32) * 0.01
    }
    pub fn inst(&self, cell: Vec3) -> TreeInst {
        let p = self.pos(cell);
        let width = 0.85 + 0.35 * (self.yaw.wrapping_mul(37) as f32 / 255.0);
        TreeInst { a: [p.x, p.y, p.z, self.scale as f32 / 100.0], b: [0.0, 0.0, self.yaw as f32 / 255.0 * TAU, width], tint: [self.tint[0], self.tint[1], self.tint[2], self.range() as u8] }
    }
}

/// A patch of a tile, for finding what stands near the camera.
pub struct NearCell {
    pub center: Vec3,
    pub radius: f32,
    /// (first, count) into `Placed::near_idx`
    pub trees: (u32, u32),
    /// (first, count) into `Placed::under`
    pub under: (u32, u32),
}

/// Everything that grows in one tile.
pub struct Placed {
    pub trees: Vec<TreeInst>,
    pub var: Vec<u8>,
    pub chunks: Vec<TreeChunk>,
    pub under: Vec<Under>,
    pub cells: Vec<NearCell>,
    /// tree indices sorted by cell
    pub near_idx: Vec<u32>,
}

/// Mean foliage colour (linear) per species; instances are tinted relative to it.
const LEAF: [[f32; 3]; SPECIES] = [
    [0.075, 0.135, 0.040],
    [0.090, 0.165, 0.045],
    [0.032, 0.080, 0.048],
    [0.050, 0.100, 0.042],
    [0.085, 0.150, 0.040],
    [0.036, 0.078, 0.040],
    [0.130, 0.200, 0.050],
    [0.060, 0.140, 0.035],
    [0.090, 0.120, 0.050],
    [0.085, 0.120, 0.075],
    [0.070, 0.125, 0.040],
    [0.060, 0.150, 0.035],
    [0.100, 0.185, 0.050],
    [0.150, 0.185, 0.070],
    [0.110, 0.095, 0.075],
];

const ST_SPRIG: f32 = 0.0;
const ST_PINNATE: f32 = 1.0;
const ST_GRASS: f32 = 2.0;

/// How a species' leaf sprays look up close. `n` is leaves (or needles) along
/// the spray; for sprigs p1/p2 are leaf width and lobing, for pinnate sprays
/// (needles, fronds) the needle fill and forward slant. `area` is the crown
/// surface each spray has to cover.
#[derive(Copy, Clone)]
struct Spray {
    style: f32,
    n: f32,
    len: f32,
    aspect: f32,
    p1: f32,
    p2: f32,
    droop: f32,
    area: f32,
}
fn spray(species: usize) -> Spray {
    let s = |style, n, len, aspect, p1, p2, droop, area| Spray { style, n, len, aspect, p1, p2, droop, area };
    match species {
        OAK => s(ST_SPRIG, 7.0, 1.25, 0.42, 0.46, 0.30, 0.15, 0.55),
        MAPLE => s(ST_SPRIG, 6.0, 1.35, 0.44, 0.62, 0.45, 0.20, 0.60),
        SPRUCE => s(ST_PINNATE, 46.0, 1.4, 0.30, 0.50, 0.7, 0.30, 0.42),
        PINE => s(ST_PINNATE, 30.0, 1.0, 0.42, 0.42, 1.1, 0.05, 0.45),
        PALM => s(ST_PINNATE, 30.0, 4.0, 0.22, 0.60, 0.9, 0.0, 1.0),
        CYPRESS => s(ST_PINNATE, 40.0, 0.8, 0.34, 0.62, 0.5, 0.0, 0.40),
        BIRCH => s(ST_SPRIG, 10.0, 1.1, 0.34, 0.50, 0.0, 0.50, 0.45),
        TROPICAL => s(ST_SPRIG, 5.0, 1.6, 0.42, 0.50, 0.0, 0.25, 0.70),
        BUSH => s(ST_SPRIG, 7.0, 0.7, 0.42, 0.45, 0.0, 0.10, 0.22),
        GUM => s(ST_SPRIG, 7.0, 1.2, 0.36, 0.22, 0.0, 0.60, 0.50),
        SHRUB => s(ST_SPRIG, 6.0, 0.55, 0.44, 0.50, 0.0, 0.10, 0.16),
        FERN => s(ST_PINNATE, 16.0, 1.0, 0.22, 0.62, 0.45, 0.0, 1.0),
        SAPLING => s(ST_SPRIG, 5.0, 0.95, 0.44, 0.52, 0.2, 0.12, 1.0),
        _ => s(ST_GRASS, 5.0, 0.7, 0.5, 0.0, 0.0, 0.0, 1.0),
    }
}

struct Rng(u32);
impl Rng {
    fn f(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(747796405).wrapping_add(2891336453);
        let w = ((self.0 >> ((self.0 >> 28) + 4)) ^ self.0).wrapping_mul(277803737);
        ((w >> 22) ^ w) as f32 / u32::MAX as f32
    }
    fn r(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    fn disc(&mut self) -> Vec2 {
        let (a, r) = (self.f() * TAU, self.f().sqrt());
        Vec2::new(a.cos(), a.sin()) * r
    }
    fn dir(&mut self) -> Vec3 {
        let (y, a) = (self.r(-1.0, 1.0), self.f() * TAU);
        let s = (1.0 - y * y).max(0.0).sqrt();
        Vec3::new(s * a.cos(), y, s * a.sin())
    }
}

fn vnoise(p: Vec3) -> f32 {
    let i = p.floor();
    let f = p - i;
    let f = f * f * (Vec3::splat(3.0) - 2.0 * f);
    let h = |x: f32, y: f32, z: f32| hash(((i.x + x) as i32 as u32).wrapping_mul(73856093) ^ ((i.y + y) as i32 as u32).wrapping_mul(19349663), ((i.z + z) as i32 as u32).wrapping_mul(83492791));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    lerp(lerp(lerp(h(0., 0., 0.), h(1., 0., 0.), f.x), lerp(h(0., 1., 0.), h(1., 1., 0.), f.x), f.y), lerp(lerp(h(0., 0., 1.), h(1., 0., 1.), f.x), lerp(h(0., 1., 1.), h(1., 1., 1.), f.x), f.y), f.z)
}

/// Octahedron (8 faces): the crown of a tree a few pixels across.
fn octa() -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let v = vec![Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z];
    let f = vec![[2, 4, 0], [2, 0, 5], [2, 5, 1], [2, 1, 4], [3, 0, 4], [3, 5, 0], [3, 1, 5], [3, 4, 1]];
    (v, f)
}

/// Icosphere: level 0 = 20 faces, level 1 = 80.
pub fn ico(level: u32) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let t = (1.0 + 5f32.sqrt()) / 2.0;
    let mut v: Vec<Vec3> = [[-1., t, 0.], [1., t, 0.], [-1., -t, 0.], [1., -t, 0.], [0., -1., t], [0., 1., t], [0., -1., -t], [0., 1., -t], [t, 0., -1.], [t, 0., 1.], [-t, 0., -1.], [-t, 0., 1.]].iter().map(|p| Vec3::from(*p).normalize()).collect();
    let mut f: Vec<[u32; 3]> = vec![[0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8], [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1]];
    for _ in 0..level {
        let mut cache = std::collections::HashMap::new();
        let mut mid = |a: u32, b: u32, v: &mut Vec<Vec3>| {
            *cache.entry((a.min(b), a.max(b))).or_insert_with(|| {
                v.push(((v[a as usize] + v[b as usize]) * 0.5).normalize());
                v.len() as u32 - 1
            })
        };
        let mut nf = Vec::with_capacity(f.len() * 4);
        for t in &f {
            let (a, b, c) = (mid(t[0], t[1], &mut v), mid(t[1], t[2], &mut v), mid(t[2], t[0], &mut v));
            nf.extend([[t[0], a, c], [t[1], b, a], [t[2], c, b], [a, b, c]]);
        }
        f = nf;
    }
    (v, f)
}

/// A point on the lumpy surface of a foliage mass, in the mass's unit space.
fn lumpy(d: Vec3, seed: f32, lump: f32) -> (Vec3, f32) {
    let n = vnoise(d * 1.9 + Vec3::splat(seed * 17.0));
    // Smaller knobs of foliage on the big lumps.
    let k = vnoise(d * 5.3 + Vec3::splat(seed * 5.0 + 3.7));
    let mut p = d * (1.0 + (n - 0.5) * 2.0 * lump + (k - 0.5) * 0.9 * lump);
    if p.y < 0.0 {
        p.y *= 0.6;
    }
    (p, n)
}

/// How deep a point sits inside the other foliage masses: 0 at or outside
/// their surfaces, 1 at a centre.
fn buried(p: Vec3, blobs: &[(Vec3, Vec3)], skip: usize) -> f32 {
    blobs.iter().enumerate().filter(|(k, _)| *k != skip).map(|(_, (c, r))| 1.0 - ((p - *c) / *r).length()).fold(0.0, f32::max)
}

/// Solid vertex. `shrink` is how far it pulls in once leaf cards take over
/// close up, `ao` how open to the sky it is, `style` the kind of bark.
fn sv(pos: Vec3, nrm: Vec3, col: Vec3, leaf: f32, shrink: Vec3, ao: f32, style: f32) -> TreeVertex {
    TreeVertex { pos: pos.into(), nrm: nrm.into(), col: [col.x, col.y, col.z, leaf], uv: [0.0, 0.0, 0.0, style], aux: [shrink.x, shrink.y, shrink.z, ao] }
}

#[derive(Default)]
struct TB {
    v: Vec<TreeVertex>,
    i: Vec<u32>,
    /// leaf cards
    cv: Vec<TreeVertex>,
    ci: Vec<u32>,
}
impl TB {
    /// A lumpy mass of foliage. Normals lean away from the crown centre so the
    /// whole canopy shades as one soft volume rather than a pile of balls.
    fn blob(&mut self, c: Vec3, r: Vec3, crown: Vec3, col: Vec3, level: i32, seed: f32, lump: f32, others: &[(Vec3, Vec3)], me: usize) {
        let (vs, fs) = if level < 0 { octa() } else { ico(level as u32) };
        let b = self.v.len() as u32;
        for d in &vs {
            let (p, n) = lumpy(*d, seed, lump);
            let pos = c + p * r;
            let nrm = (*d * 0.45 + (pos - crown).normalize_or_zero() * 0.55).normalize_or_zero();
            // Lighter on top and at the tips, darker underneath.
            let shade = (0.62 + 0.42 * (d.y * 0.5 + 0.5)) * (0.86 + 0.28 * n);
            // Dark where the masses run into each other.
            let ao = 1.0 - 0.65 * (buried(pos, others, me) / 0.4).clamp(0.0, 1.0);
            self.v.push(sv(pos, nrm, col * shade, 1.0, (c - pos) * 0.14, ao, 0.0));
        }
        for f in &fs {
            self.i.extend([b + f[0], b + f[1], b + f[2]]);
        }
    }
    /// Tapering limb through a list of (centre, radius).
    fn tube(&mut self, pts: &[(Vec3, f32)], sides: u32, col: Vec3, style: f32) {
        let b = self.v.len() as u32;
        for (k, (p, r)) in pts.iter().enumerate() {
            let dir = (pts[(k + 1).min(pts.len() - 1)].0 - pts[k.saturating_sub(1)].0).normalize_or_zero();
            let x = dir.cross(Vec3::new(0.3, 0.1, 0.9)).normalize_or_zero();
            let y = dir.cross(x);
            for s in 0..sides {
                let a = s as f32 / sides as f32 * TAU;
                let n = x * a.cos() + y * a.sin();
                let sh = 0.8 + 0.4 * hash(k as u32 * 7 + s, 3);
                self.v.push(sv(*p + n * *r, n, col * sh, 0.0, Vec3::ZERO, 1.0, style));
            }
        }
        for k in 0..pts.len() as u32 - 1 {
            for s in 0..sides {
                let (a, c) = (b + k * sides + s, b + k * sides + (s + 1) % sides);
                self.i.extend([a, c, a + sides, c, c + sides, a + sides]);
            }
        }
    }
    /// One skirt of a conifer: a jagged, drooping cone.
    fn skirt(&mut self, y0: f32, y1: f32, r: f32, sides: u32, col: Vec3, seed: f32) {
        let b = self.v.len() as u32;
        self.v.push(sv(Vec3::Y * y1, Vec3::Y, col * 1.15, 1.0, Vec3::ZERO, 0.8, 0.0));
        for s in 0..sides {
            let a = (s as f32 + seed) / sides as f32 * TAU;
            let jag = if s % 2 == 0 { 1.0 } else { 0.72 } * (0.85 + 0.3 * hash(s, (seed * 977.0) as u32));
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            let sh = 0.6 + 0.3 * hash(s + 3, (seed * 577.0) as u32);
            self.v.push(sv(d * r * jag + Vec3::Y * (y0 - (jag - 0.7) * (y1 - y0) * 0.25), (d + Vec3::Y * 0.55).normalize(), col * sh, 1.0, d * (-0.38 * r * jag), 1.0, 0.0));
        }
        for s in 0..sides {
            self.i.extend([b, b + 1 + (s + 1) % sides, b + 1 + s]);
        }
    }
    /// A palm frond: an arching, folded strip. From afar it is a solid blade;
    /// close up that folds away and a card with cut-out leaflets takes over.
    fn frond(&mut self, base: Vec3, ang: f32, len: f32, lift: f32, segs: u32, col: Vec3, fold: bool, seed: f32) {
        let d = Vec3::new(ang.cos(), 0.0, ang.sin());
        let side = Vec3::new(-ang.sin(), 0.0, ang.cos());
        let b = self.v.len() as u32;
        let per = if fold { 3 } else { 2 };
        let sp = spray(PALM);
        let cb = self.cv.len() as u32;
        for k in 0..=segs {
            let t = k as f32 / segs as f32;
            let spine = base + d * (len * t) + Vec3::Y * (len * (lift * t - 0.95 * t * t));
            let w = len * 0.17 * (PI * (t * 0.86 + 0.1)).sin();
            let c = col * (0.75 + 0.4 * t);
            let n = (Vec3::Y + d * (1.9 * t - lift)).normalize();
            let (l, r) = (spine + side * w - Vec3::Y * w * 0.45, spine - side * w - Vec3::Y * w * 0.45);
            self.v.push(sv(l, (n + side * 0.4).normalize(), c, 1.0, spine - l, 1.0, 0.0));
            if fold {
                self.v.push(sv(spine, n, c, 1.0, Vec3::ZERO, 1.0, 0.0));
                let wc = len * sp.aspect;
                for (s, nn) in [(1.0f32, n + side * 0.4), (0.0, n), (-1.0, n - side * 0.4)] {
                    self.cv.push(TreeVertex { pos: (spine + side * (wc * s) - Vec3::Y * (wc * 0.45 * s.abs())).into(), nrm: nn.normalize().into(), col: [c.x, c.y, c.z, sp.n], uv: [t, s, seed, sp.style], aux: [sp.aspect, sp.p1, sp.p2, 1.0] });
                }
            }
            self.v.push(sv(r, (n - side * 0.4).normalize(), c, 1.0, spine - r, 1.0, 0.0));
        }
        for k in 0..segs {
            for j in 0..per - 1 {
                let a = b + k * per + j;
                // Open strip: emit both windings so shadow backface culling keeps fronds solid.
                self.i.extend([a, a + 1, a + per, a + 1, a + per + 1, a + per, a, a + per, a + 1, a + 1, a + per, a + per + 1]);
                if fold {
                    let a = cb + k * 3 + j;
                    self.ci.extend([a, a + 1, a + 3, a + 1, a + 4, a + 3]);
                }
            }
        }
    }
    /// A spray of leaves: a strip from `base` along `stem`, `len` long, curling
    /// away from its upper face toward the tip.
    fn card(&mut self, base: Vec3, stem: Vec3, side: Vec3, len: f32, sp: &Spray, segs: u32, curl: f32, nrm: Vec3, col: Vec3, seed: f32, ao: f32) {
        let b = self.cv.len() as u32;
        let face = side.cross(stem);
        let w = len * sp.aspect;
        for k in 0..=segs {
            let t = k as f32 / segs as f32;
            let c = base + stem * (len * t) - face * (curl * len * t * t);
            // New growth at the tip is lighter.
            let sh = 0.78 + 0.34 * t;
            for s in [-1.0f32, 1.0] {
                self.cv.push(TreeVertex { pos: (c + side * (w * s)).into(), nrm: nrm.into(), col: [col.x * sh, col.y * sh, col.z * sh, sp.n], uv: [t, s, seed, sp.style], aux: [sp.aspect, sp.p1, sp.p2, ao] });
            }
        }
        for k in 0..segs {
            let a = b + k * 2;
            self.ci.extend([a, a + 1, a + 2, a + 1, a + 3, a + 2]);
        }
    }
    /// A spray lying on a surface whose outward normal is `out`, growing along
    /// `stem`, shingled over the foliage behind it.
    fn spray_on(&mut self, pos: Vec3, out: Vec3, stem: Vec3, sp: &Spray, nrm: Vec3, col: Vec3, ao: f32, rng: &mut Rng) {
        let mut side = stem.cross(out).normalize_or_zero();
        if side == Vec3::ZERO {
            return;
        }
        let tw = rng.r(-0.7, 0.7);
        side = (side * tw.cos() + stem.cross(side) * tw.sin()).normalize();
        let len = sp.len * rng.r(0.75, 1.25);
        self.card(pos - stem * (len * 0.4) + out * 0.05, stem, side, len, sp, 2, 0.12 + sp.droop * 0.4, nrm, col, rng.f(), ao);
    }
    /// Leaf sprays all over the outside of a crown made of foliage masses.
    fn crown_cards(&mut self, blobs: &[(Vec3, Vec3)], crown: Vec3, col: Vec3, sp: &Spray, lump: f32, variant: usize, max: f32, rng: &mut Rng) {
        let areas: Vec<f32> = blobs.iter().map(|(_, r)| 4.0 * PI * (((r.x * r.y).powf(1.6) + (r.x * r.z).powf(1.6) + (r.y * r.z).powf(1.6)) / 3.0).powf(0.625)).collect();
        let thin = (max * sp.area / areas.iter().sum::<f32>()).min(1.0);
        for (k, (c, r)) in blobs.iter().enumerate() {
            for _ in 0..(areas[k] / sp.area * thin) as usize {
                let d = rng.dir();
                let rnd = rng.dir();
                // Fewer underneath, where it is bare branches.
                if d.y < -0.35 && rng.f() < 0.5 {
                    continue;
                }
                let (p, nz) = lumpy(d, k as f32 + variant as f32 * 3.1, lump);
                let pos = *c + p * *r;
                let bur = buried(pos, blobs, k);
                if bur > 0.12 {
                    continue;
                }
                let out = (d / *r).normalize_or_zero();
                let tan = out.cross(rnd).normalize_or_zero();
                let stem = (tan + out * rng.r(0.35, 1.1) - Vec3::Y * (sp.droop * rng.r(0.5, 1.5))).normalize_or_zero();
                let nrm = (d * 0.45 + (pos - crown).normalize_or_zero() * 0.55 + rnd * 0.15).normalize_or_zero();
                let shade = (0.62 + 0.42 * (d.y * 0.5 + 0.5)) * (0.86 + 0.28 * nz) * rng.r(0.85, 1.15) * (0.9 + 0.2 * hash(k as u32, variant as u32));
                self.spray_on(pos, out, stem, sp, nrm, col * shade, 1.0 - 2.5 * bur, rng);
            }
        }
    }
}

/// Grow one individual of a species. The same seed is used for every level of
/// detail so the simplified models keep the silhouette of the full one.
/// Returns the solid mesh, its leaf cards (level 0 only) and its size.
fn grow(species: usize, variant: usize, lod: usize) -> (TB, TreeDim) {
    let mut rng = Rng((species * 131 + variant * 17 + 7) as u32);
    let mut t = TB::default();
    if species >= TREE_SPECIES && lod > 0 {
        // Undergrowth is never seen from far enough away to need a simpler model.
        return (t, TreeDim::default());
    }
    let mut leaf = Vec3::from(LEAF[species]);
    // Each individual has its own cast of green.
    leaf *= Vec3::new(rng.r(0.85, 1.15), rng.r(0.9, 1.1), rng.r(0.8, 1.2));
    let bark = match species {
        BIRCH => Vec3::new(0.48, 0.47, 0.43),
        GUM => Vec3::new(0.40, 0.36, 0.30),
        PINE => Vec3::new(0.13, 0.085, 0.06),
        PALM => Vec3::new(0.20, 0.17, 0.13),
        DEADWOOD => Vec3::new(0.15, 0.125, 0.10),
        _ => Vec3::new(0.085, 0.065, 0.045),
    } * rng.r(0.8, 1.2);
    let bark_style = match species {
        BIRCH => 1.0,
        GUM => 3.0,
        _ => 0.0,
    };
    // Sides of the trunk, and how finely the masses of foliage are made.
    let (sides, level) = match lod {
        0 => (7, 1),
        1 => (5, 0),
        2 => (4, 0),
        3 => (3, 0),
        _ => (3, -1),
    };
    // A trunk under half a metre across is sub-texel in cascade 1 (0.73 m/texel)
    // and hidden under the crown; LOD 3 and 4 are shadow-only beyond 260 m.
    let has_trunk = lod < 3;
    let sp = spray(species);
    let lean = Vec3::new(rng.r(-1.0, 1.0), 0.0, rng.r(-1.0, 1.0));
    let mut dim = TreeDim::default();
    // (centre, radii) of each foliage mass, from which every LOD is built.
    let mut blobs: Vec<(Vec3, Vec3)> = Vec::new();

    match species {
        SPRUCE => {
            let h = rng.r(15.0, 20.0);
            let r = rng.r(2.4, 3.3);
            let n = [5, 4, 3, 1, 1][lod];
            let base = rng.r(1.2, 2.6);
            if has_trunk {
                t.tube(&[(Vec3::Y * -0.3, 0.42), (Vec3::Y * 0.5, 0.28), (Vec3::Y * (h * 0.8), 0.06)], sides, bark, 0.0);
            }
            for k in 0..n {
                let f = k as f32 / n as f32;
                let (y0, y1) = (base + (h - base) * f, base + (h - base) * (f + 1.6 / n as f32).min(1.0));
                let (rk, col) = (r * (1.0 - f * 0.82), leaf * (1.0 - 0.12 * f));
                t.skirt(y0, y1 + 0.3, rk, [9, 8, 7, 5, 4][lod], col, variant as f32 * 0.37 + k as f32 * 0.21);
                if lod == 0 {
                    // Boughs of needles hanging down each skirt.
                    let apex = Vec3::Y * (y1 + 0.3);
                    let slant = (rk * rk + (y1 + 0.3 - y0).powi(2)).sqrt();
                    for _ in 0..((PI * rk * slant / sp.area) as usize).min(140) {
                        let a = rng.f() * TAU;
                        let d = Vec3::new(a.cos(), 0.0, a.sin());
                        let (u, rnd) = (rng.r(0.3, 1.0), rng.dir());
                        let rim = d * (rk * 0.9) + Vec3::Y * y0;
                        let out = (d + Vec3::Y * 0.55).normalize();
                        let stem = ((rim - apex).normalize() * 0.8 + d * 0.5 + d.cross(Vec3::Y) * rng.r(-0.4, 0.4)).normalize();
                        t.spray_on(apex.lerp(rim, u), out, stem, &sp, (out + rnd * 0.15).normalize(), col * ((0.6 + 0.4 * u) * rng.r(0.85, 1.15)), 0.55 + 0.45 * u, &mut rng);
                    }
                }
            }
            dim = TreeDim { height: h, crown_r: r, crown_base: base, trunk_r: 0.28 };
        }
        PALM => {
            let h = rng.r(8.0, 13.0);
            let bend = lean * rng.r(0.3, 1.4);
            let top = Vec3::Y * h + bend;
            let tr: Vec<(Vec3, f32)> = (0..=4).map(|k| {
                let f = k as f32 / 4.0;
                (Vec3::Y * (h * f) + bend * f * f, 0.24 - 0.09 * f + if k == 0 { 0.1 } else { 0.0 })
            }).collect();
            if lod == 0 { t.tube(&tr, sides, bark, 2.0) } else { t.tube(&[tr[0], tr[2], tr[4]], sides, bark, 2.0) }
            let n = [11, 9, 7, 5, 4][lod];
            for k in 0..n {
                let a = k as f32 / n as f32 * TAU + rng.r(-0.25, 0.25);
                let lift = if k % 3 == 0 { rng.r(0.75, 1.0) } else { rng.r(0.3, 0.7) };
                let len = rng.r(3.2, 4.4);
                t.frond(top, a, len, lift, [5, 3, 2, 1, 1][lod], leaf, lod == 0, rng.f());
            }
            dim = TreeDim { height: h + 1.5, crown_r: 3.6, crown_base: h - 1.5, trunk_r: 0.25 };
        }
        CYPRESS => {
            let h = rng.r(10.0, 16.0);
            let r = rng.r(1.0, 1.6);
            let rings = [6, 5, 4, 3, 2][lod];
            let sd = [8u32, 6, 5, 4, 3][lod];
            let b = t.v.len() as u32;
            let seed = rng.f();
            // Flame shape: widest a third of the way up.
            let prof = |f: f32| (f * 3.0).min(1.0).powf(0.6) * (1.0 - f).powf(0.75) * 1.75;
            let radius = |d: Vec3, f: f32| r * prof(f) * (0.85 + 0.3 * vnoise(d * 2.0 + Vec3::Y * f * 5.0 + seed * 9.0));
            for k in 0..=rings {
                let f = k as f32 / rings as f32;
                for s in 0..sd {
                    let a = s as f32 / sd as f32 * TAU + f * 1.3;
                    let d = Vec3::new(a.cos(), 0.0, a.sin());
                    let rr = radius(d, f);
                    t.v.push(sv(d * rr + Vec3::Y * (0.5 + (h - 0.5) * f), (d + Vec3::Y * 0.3).normalize(), leaf * (0.6 + 0.5 * f), 1.0, d * (-0.15 * rr), 1.0, 0.0));
                }
            }
            for k in 0..rings {
                for s in 0..sd {
                    let (a, c) = (b + k * sd + s, b + k * sd + (s + 1) % sd);
                    t.i.extend([a, a + sd, c, c, a + sd, c + sd]);
                }
            }
            if has_trunk {
                t.tube(&[(Vec3::Y * -0.3, 0.26), (Vec3::Y * 1.0, 0.16)], sides, bark, 0.0);
            }
            if lod == 0 {
                // Upswept sprays of scale leaves all the way up.
                for _ in 0..((TAU * r * h / sp.area) as usize).min(300) {
                    let (f, a) = (rng.r(0.03, 0.97), rng.f() * TAU);
                    let d = Vec3::new(a.cos(), 0.0, a.sin());
                    let rnd = rng.dir();
                    let out = (d + Vec3::Y * 0.3).normalize();
                    let stem = (Vec3::Y * 0.85 + d * 0.4 + d.cross(Vec3::Y) * rng.r(-0.3, 0.3)).normalize();
                    t.spray_on(d * radius(d, f) + Vec3::Y * (0.5 + (h - 0.5) * f), out, stem, &sp, (out + rnd * 0.15).normalize(), leaf * ((0.6 + 0.5 * f) * rng.r(0.85, 1.15)), 1.0, &mut rng);
                }
            }
            dim = TreeDim { height: h, crown_r: r, crown_base: 0.6, trunk_r: 0.3 };
        }
        FERN => {
            // A rosette of arching fronds.
            let n = 7 + variant * 2;
            for k in 0..n {
                let a = k as f32 / n as f32 * TAU + rng.r(-0.3, 0.3);
                let el = rng.r(0.55, 1.1);
                let d = Vec3::new(a.cos(), 0.0, a.sin());
                let stem = d * el.cos() + Vec3::Y * el.sin();
                // side x stem points up, so the frond arches over and down.
                let side = stem.cross(Vec3::Y).normalize();
                let nrm = (Vec3::Y + d * 0.5).normalize();
                t.card(Vec3::Y * 0.03, stem, side, rng.r(0.75, 1.25), &sp, 4, rng.r(0.45, 0.8), nrm, leaf * rng.r(0.85, 1.15), rng.f(), 0.75 + 0.25 * el);
            }
        }
        GRASS => {
            // A tussock: a few crossed fans of blades.
            let n = 3 + variant % 2;
            for k in 0..n {
                let a = k as f32 / n as f32 * PI + rng.r(-0.3, 0.3);
                let side = Vec3::new(a.cos(), 0.0, a.sin());
                let off = Vec3::new(rng.r(-0.12, 0.12), -0.03, rng.r(-0.12, 0.12));
                let stem = (Vec3::Y + Vec3::new(rng.r(-0.15, 0.15), 0.0, rng.r(-0.15, 0.15))).normalize();
                let face = side.cross(stem);
                t.card(off, stem, side, rng.r(0.5, 0.95), &sp, 2, rng.r(-0.1, 0.1), (Vec3::Y + face * 0.35).normalize(), leaf * rng.r(0.8, 1.2), rng.f(), 1.0);
            }
        }
        SAPLING => {
            // A whip of a young tree with a few leafy twigs.
            let h = rng.r(1.8, 3.6);
            let top = Vec3::Y * h + lean * rng.r(0.1, 0.5);
            let at = |f: f32| top * f + lean * (0.15 * (f * PI).sin());
            t.tube(&[(at(0.0) - Vec3::Y * 0.2, 0.04), (at(0.5), 0.028), (at(1.0), 0.008)], 4, bark * 1.2, 0.0);
            for k in 0..16 + variant * 4 {
                let f = rng.r(0.3, 1.0);
                let a = k as f32 * 2.4 + rng.r(-0.4, 0.4);
                let d = Vec3::new(a.cos(), 0.0, a.sin());
                let stem = (d * 0.8 + Vec3::Y * rng.r(0.25, 0.8)).normalize();
                let side = stem.cross(Vec3::Y).normalize();
                t.card(at(f), stem, side, sp.len * rng.r(0.7, 1.2) * (1.25 - 0.5 * f), &sp, 2, sp.droop, (d * 0.6 + Vec3::Y).normalize(), leaf * rng.r(0.85, 1.15), rng.f(), 0.8 + 0.2 * f);
            }
        }
        DEADWOOD => {
            let a = rng.f() * TAU;
            let d = Vec3::new(a.cos(), 0.0, a.sin());
            match variant {
                0 | 1 => {
                    // A fallen trunk with the stubs of its limbs.
                    let (len, r) = (rng.r(3.0, 6.5), rng.r(0.16, 0.3));
                    let (p0, p1) = (d * (-len * 0.5) + Vec3::Y * (r * 0.75), d * (len * 0.5) + Vec3::Y * (r * 0.55));
                    t.tube(&[(p0 - d * 0.02, 0.0), (p0, r), ((p0 + p1) * 0.5, r * 0.9), (p1, r * 0.7), (p1 + d * 0.02, 0.0)], 7, bark, 0.0);
                    for _ in 0..2 + variant {
                        let from = p0.lerp(p1, rng.r(0.25, 0.9));
                        let dir = (d.cross(Vec3::Y) * rng.r(-1.0, 1.0) + Vec3::Y * rng.r(0.3, 1.0) + d * 0.4).normalize();
                        t.tube(&[(from, r * 0.4), (from + dir * rng.r(0.4, 1.1), r * 0.18)], 4, bark, 0.0);
                    }
                }
                2 => {
                    // A stump.
                    let (r, h) = (rng.r(0.25, 0.45), rng.r(0.35, 0.8));
                    t.tube(&[(Vec3::Y * -0.3, r * 1.5), (Vec3::Y * 0.12, r * 1.1), (Vec3::Y * h, r), (Vec3::Y * (h + 0.02), 0.0)], 8, bark, 0.0);
                }
                _ => {
                    // A standing dead snag, snapped off.
                    let (r, h) = (rng.r(0.16, 0.26), rng.r(3.5, 7.0));
                    let top = Vec3::Y * h + lean * 0.5;
                    t.tube(&[(Vec3::Y * -0.3, r * 1.5), (Vec3::Y * 0.4, r), (top * 0.6, r * 0.75), (top, r * 0.5), (top + Vec3::Y * 0.3, 0.0)], 6, bark, 0.0);
                    for _ in 0..3 {
                        let from = top * rng.r(0.45, 0.95);
                        let dir = (Vec3::new(rng.r(-1.0, 1.0), rng.r(0.2, 0.9), rng.r(-1.0, 1.0))).normalize();
                        t.tube(&[(from, r * 0.35), (from + dir * rng.r(0.5, 1.4), r * 0.1)], 4, bark, 0.0);
                    }
                }
            }
        }
        _ => {
            // Broadleaf-style trees: a trunk, a few limbs, and masses of foliage.
            // (height, bare trunk, trunk radius, crown spread xz, crown spread y, blob radius xz, blob radius y, blobs, lumpiness)
            let (h, th, tr, sx, sy, bx, by, n, lump) = match species {
                OAK => (rng.r(10.0, 14.0), rng.r(2.8, 4.2), 0.40, 3.0, 1.6, 2.9, 2.2, 6, 0.24),
                MAPLE => (rng.r(13.0, 18.0), rng.r(3.5, 5.5), 0.34, 2.0, 3.0, 2.7, 2.6, 5, 0.22),
                PINE => (rng.r(14.0, 19.0), rng.r(8.0, 11.0), 0.30, 2.4, 1.1, 2.5, 1.3, 4, 0.3),
                BIRCH => (rng.r(11.0, 16.0), rng.r(3.5, 5.0), 0.16, 1.0, 3.2, 1.7, 2.0, 4, 0.3),
                TROPICAL => (rng.r(9.0, 13.0), rng.r(4.0, 6.0), 0.36, 4.2, 0.6, 3.6, 1.25, 5, 0.22),
                BUSH => (rng.r(1.6, 2.8), 0.0, 0.0, 0.9, 0.2, 1.2, 1.0, 3, 0.3),
                SHRUB => (rng.r(1.0, 1.9), 0.0, 0.0, 0.55, 0.12, 0.75, 0.62, 3, 0.32),
                _ => (rng.r(17.0, 24.0), rng.r(7.0, 10.0), 0.36, 2.8, 4.0, 2.1, 1.7, 6, 0.34),
            };
            let low = matches!(species, BUSH | SHRUB);
            let cy = match species {
                BUSH => 0.8,
                SHRUB => 0.5,
                _ => th + (h - th) * 0.5,
            };
            let crown = Vec3::new(lean.x * 0.6, cy, lean.z * 0.6) * if species == SHRUB { Vec3::new(0.3, 1.0, 0.3) } else { Vec3::ONE };
            for k in 0..n {
                let d = rng.disc();
                let fy = if n > 1 { k as f32 / (n - 1) as f32 * 2.0 - 1.0 } else { 0.0 };
                let c = crown + Vec3::new(d.x * sx, fy * sy * rng.r(0.7, 1.0), d.y * sx);
                let s = rng.r(0.8, 1.15);
                blobs.push((c, Vec3::new(bx * s, by * s * rng.r(0.85, 1.15), bx * s)));
            }
            if !low {
                let top = Vec3::new(crown.x * 0.7, th + (cy - th) * 0.6, crown.z * 0.7);
                // Flared at the roots.
                let trunk = [(Vec3::Y * -0.3, tr * 1.7), (Vec3::Y * 0.25, tr * 1.25), (Vec3::Y * 0.9, tr), (Vec3::new(top.x * 0.4, th * 0.75, top.z * 0.4), tr * 0.8), (top, tr * 0.45)];
                if lod == 0 {
                    t.tube(&trunk, sides, bark, bark_style);
                    // Limbs reaching into the foliage.
                    for (c, _) in blobs.iter() {
                        let from = trunk[3].0 + (trunk[4].0 - trunk[3].0) * rng.r(0.0, 0.8);
                        t.tube(&[(from, tr * 0.42), ((from + *c) * 0.5 + Vec3::Y * 0.4, tr * 0.3), (*c, tr * 0.12)], 5, bark, bark_style);
                    }
                } else if lod == 1 {
                    t.tube(&[trunk[0], trunk[2], trunk[3], trunk[4]], sides, bark, bark_style);
                } else if lod < 3 {
                    t.tube(&[trunk[1], trunk[4]], sides, bark, bark_style);
                }
            }
            let (lo, hi) = blobs.iter().fold((Vec3::splat(1e9), Vec3::splat(-1e9)), |(lo, hi), (c, r)| (lo.min(*c - *r), hi.max(*c + *r)));
            match lod {
                0 => {
                    for (k, (c, r)) in blobs.iter().enumerate() {
                        t.blob(*c, *r, crown, leaf * (0.9 + 0.2 * hash(k as u32, variant as u32)), level, k as f32 + variant as f32 * 3.1, lump, &blobs, k);
                    }
                    t.crown_cards(&blobs, crown, leaf, &sp, lump, variant, if low { 110.0 } else { 700.0 }, &mut rng);
                }
                1 => {
                    // Every mass, more coarsely made.
                    for (k, (c, r)) in blobs.iter().enumerate() {
                        t.blob(*c, *r, crown, leaf * (0.9 + 0.2 * hash(k as u32, variant as u32)), 0, k as f32 + variant as f32 * 3.1, lump, &blobs, k);
                    }
                }
                2 => {
                    // Three fatter masses standing in for the lot.
                    let m = blobs.len().min(3);
                    for k in 0..m {
                        let grp: Vec<_> = blobs.iter().skip(k).step_by(m).collect();
                        let c = grp.iter().fold(Vec3::ZERO, |a, b| a + b.0) / grp.len() as f32;
                        let r = grp.iter().fold(Vec3::ZERO, |a, b| a.max((b.0 - c).abs() * 0.6 + b.1));
                        t.blob(c, r * 1.05, crown, leaf, 0, k as f32 + variant as f32 * 3.1, lump, &[], 0);
                    }
                }
                // LOD 4 uses an 8-triangle octahedron whose inscribed radius is smaller than
                // LOD 3's icosphere; inflate slightly so projected shadow area matches.
                _ => t.blob((lo + hi) * 0.5, (hi - lo) * if level < 0 { 0.60 } else { 0.52 }, crown, leaf * 0.95, level, variant as f32, lump * 0.6, &[], 0),
            }
            dim = TreeDim { height: hi.y, crown_r: ((hi.x - lo.x).max(hi.z - lo.z)) * 0.5, crown_base: lo.y.max(0.0), trunk_r: tr.max(0.15) };
        }
    }
    (t, dim)
}

pub fn build_meshes() -> TreeMeshes {
    let mut meshes = Vec::new();
    let mut dims = Vec::new();
    for s in 0..SPECIES {
        for v in 0..VARIANTS {
            let mut cards = (Vec::new(), Vec::new());
            for l in 0..LODS - 1 {
                let (m, d) = grow(s, v, l);
                if l == 0 {
                    dims.push(d);
                    cards = (m.cv, m.ci);
                }
                // The shader draws each tree with only the model for its distance.
                let mut v = m.v;
                for x in &mut v {
                    x.uv[0] = l as f32;
                }
                meshes.push((v, m.i));
            }
            meshes.push(cards);
        }
    }
    TreeMeshes { meshes, dims }
}

/// How likely each species is at a place: climate from latitude and altitude,
/// a few regional specialities, and whether the spot is city, scrub or wild.
fn species_weights(lat: f64, lon: f64, elev: f32, urban: bool, arid: bool, scrub: bool) -> [f32; TREE_SPECIES] {
    let alat = lat.abs() as f32;
    // Climbing ~140 m is like moving a degree toward the pole.
    let eff = alat + elev.max(0.0) / 140.0;
    let mut w = [0f32; TREE_SPECIES];
    let california = lon > -125.0 && lon < -114.0 && lat > 32.0 && lat < 42.0;
    let med = lon > -10.0 && lon < 45.0 && lat > 30.0 && lat < 46.0;
    let australia = lon > 110.0 && lon < 180.0 && lat < -9.0;
    if eff > 68.0 {
        return w;
    } else if eff > 62.0 {
        w[SPRUCE] = 0.4;
        w[BIRCH] = 0.4;
        w[BUSH] = 0.3;
    } else if eff > 50.0 {
        w[SPRUCE] = 0.45;
        w[PINE] = 0.2;
        w[BIRCH] = 0.25;
        w[MAPLE] = 0.1;
        if urban {
            w[MAPLE] += 0.4;
            w[OAK] += 0.25;
        }
    } else if eff > 35.0 || alat > 35.0 {
        let cold = ((eff - 40.0) / 10.0).clamp(0.0, 1.0);
        w[OAK] = 0.32;
        w[MAPLE] = 0.3;
        w[PINE] = 0.1 + 0.1 * cold;
        w[SPRUCE] = 0.04 + 0.3 * cold;
        w[BIRCH] = 0.08;
        w[CYPRESS] = 0.03;
        w[BUSH] = 0.05;
        if urban {
            w[MAPLE] += 0.3;
            w[SPRUCE] *= 0.4;
        }
    } else if alat > 23.5 {
        w[OAK] = 0.22;
        w[PINE] = 0.2;
        w[TROPICAL] = 0.1;
        w[CYPRESS] = 0.06;
        w[BUSH] = 0.14;
        w[MAPLE] = 0.12;
        w[GUM] = 0.05;
        if elev < 500.0 {
            w[PALM] = if urban { 0.3 } else { 0.1 };
        }
    } else if elev < 1000.0 {
        w[TROPICAL] = 0.45;
        w[PALM] = if urban { 0.4 } else { 0.22 };
        w[MAPLE] = 0.2;
        w[BUSH] = 0.08;
    } else {
        w[OAK] = 0.3;
        w[PINE] = 0.25;
        w[GUM] = 0.2;
        w[TROPICAL] = 0.15;
    }
    if california {
        w[GUM] += 0.22;
        w[CYPRESS] += 0.18;
        w[PINE] += 0.15;
        w[SPRUCE] *= 0.3;
        w[BIRCH] = 0.0;
        w[MAPLE] *= 0.6;
        if urban && elev < 300.0 {
            w[PALM] += 0.07;
        }
    }
    if med {
        w[CYPRESS] += 0.22;
        w[PINE] += 0.2;
        w[BUSH] += 0.1;
        if elev < 300.0 && alat < 42.0 {
            w[PALM] += if urban { 0.08 } else { 0.02 };
        }
    }
    if australia {
        w[GUM] += 0.9;
        w[SPRUCE] = 0.0;
        w[BIRCH] = 0.0;
    }
    if arid {
        w[BUSH] = w[BUSH] * 3.0 + 0.3;
        for s in [OAK, MAPLE, SPRUCE, BIRCH] {
            w[s] *= 0.4;
        }
        if alat < 36.0 && elev < 800.0 {
            w[PALM] += 0.15;
        }
    }
    if scrub {
        w[BUSH] = w[BUSH] * 2.0 + 1.2;
    }
    w
}

/// Decide where trees stand in a tile and what they are.
pub fn place(cx: &Ctx, vt: &VecTile, hoods: &Hoods, built: &[u8], roads: &[u8]) -> Placed {
    let ext = cx.ext;
    // Patches of the tile used to find what stands near the camera.
    const NC: usize = 24;
    let cell_of = |p: [f32; 2]| (((p[1] / ext * NC as f32) as usize).min(NC - 1) * NC + ((p[0] / ext * NC as f32) as usize).min(NC - 1)) as u16;
    // Land cover and no-go rasters.
    let mut cover = vec![0u8; RASTER * RASTER];
    for want in [LC_GRASS, LC_FARM, LC_WET, LC_SCRUB, LC_WOOD, LC_BARE, LC_ROCK] {
        for (_, rings) in vt.landcover.iter().filter(|l| l.0 == want) {
            let mut layer = vec![0u8; RASTER * RASTER];
            crate::buildings::raster(&mut layer, RASTER, ext, rings, 1);
            for (d, s) in cover.iter_mut().zip(&layer) {
                if *s != 0 {
                    *d = want;
                }
            }
        }
    }
    let mut block = roads.to_vec();
    for (_, rings) in &vt.water {
        crate::buildings::raster(&mut block, RASTER, ext, rings, 1);
    }
    for (_, rings) in vt.landuse.iter().filter(|l| l.0 == LU_OPEN) {
        crate::buildings::raster(&mut block, RASTER, ext, rings, 1);
    }
    // Nothing grows on an airfield: inside its fence, and well clear of its
    // pavement where the map has no fence drawn.
    for (_, rings) in &vt.aero_polys {
        crate::buildings::raster(&mut block, RASTER, ext, rings, 1);
    }
    let px_m = cx.unit_m * ext / RASTER as f32;
    for l in &vt.aero_lines {
        let hw = (l.width * 0.5 + if l.runway { 75.0 } else { 20.0 }) / px_m;
        let mut pts = l.pts.clone();
        if l.runway && pts.len() >= 2 {
            // And the approach and climb-out beyond each end.
            let (a, b) = (glam::Vec2::from(pts[0]), glam::Vec2::from(pts[pts.len() - 1]));
            let d = (b - a).normalize_or_zero() * (450.0 / cx.unit_m);
            pts[0] = (a - d).into();
            let n = pts.len() - 1;
            pts[n] = (b + d).into();
        }
        crate::sources::thick_line(&pts, RASTER as f32 / ext, hw, RASTER, |i| block[i] = 1);
    }
    // "Treeness" of each satellite pixel: vegetation that is dark and rough
    // (canopy) rather than bright and smooth (lawn, crops).
    let lum = |x: usize, y: usize| {
        let p = &cx.atlas[(y.min(ATLAS - 1) * ATLAS + x.min(ATLAS - 1)) * 4..];
        (p[0] as f32 + p[1] as f32 + p[2] as f32) / 3.0
    };
    let mut treeness = vec![0f32; ATLAS * ATLAS];
    let mut gsum = 0.0;
    for y in 0..ATLAS {
        for x in 0..ATLAS {
            let p = &cx.atlas[(y * ATLAS + x) * 4..];
            let g = greenness([p[0], p[1], p[2]]);
            gsum += g;
            if g <= 0.0 {
                continue;
            }
            let (mut s, mut s2) = (0.0, 0.0);
            for (dx, dy) in [(0i32, 0i32), (1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
                let l = lum((x as i32 + dx).max(0) as usize, (y as i32 + dy).max(0) as usize);
                s += l;
                s2 += l * l;
            }
            let sd = (s2 / 9.0 - (s / 9.0) * (s / 9.0)).max(0.0).sqrt();
            let dark = ((150.0 - s / 9.0) / 55.0).clamp(0.0, 1.0);
            treeness[y * ATLAS + x] = g * (0.2 + 0.8 * ((sd - 3.0) / 8.0).clamp(0.0, 1.0)) * (0.35 + 0.65 * dark);
        }
    }
    let arid = gsum / ((ATLAS * ATLAS) as f32) < 0.035;
    let urban_tile = hoods.mean_cover() > 0.06;

    let width_m = ext * cx.unit_m;
    const CELL: f32 = 6.4;
    let n = (width_m / CELL).ceil() as u32;
    let tseed = cx.id.x.wrapping_mul(7919) ^ cx.id.y.wrapping_mul(104729);
    let mut cand: Vec<(TreeInst, u8, u8, u16)> = Vec::new(); // instance, variant, chunk, near cell
    // Chunks per side of the tile: small enough that few of them straddle a
    // change of model (each draws every model its trees might use), and at
    // most 16 so the chunk index fits a byte.
    const CH: usize = 16;
    for iy in 0..n {
        for ix in 0..n {
            let s = (ix + iy * 8191) ^ tseed;
            let m = Vec2::new((ix as f32 + hash(s, 1)) * CELL - width_m * 0.5, (iy as f32 + hash(s, 2)) * CELL - width_m * 0.5);
            let p = cx.units(m);
            if p[0] < 0.0 || p[1] < 0.0 || p[0] >= ext || p[1] >= ext {
                continue;
            }
            let (rx, ry) = ((p[0] / ext * RASTER as f32) as usize, (p[1] / ext * RASTER as f32) as usize);
            let ri = ry.min(RASTER - 1) * RASTER + rx.min(RASTER - 1);
            if block[ri] != 0 {
                continue;
            }
            // Keep clear of walls.
            let near_wall = (-1i32..=1).any(|dy| (-1i32..=1).any(|dx| built[((ry as i32 + dy).clamp(0, RASTER as i32 - 1) as usize) * RASTER + (rx as i32 + dx).clamp(0, RASTER as i32 - 1) as usize] != 0));
            if near_wall {
                continue;
            }
            let (axp, ayp) = ((p[0] / ext * ATLAS as f32) as usize, (p[1] / ext * ATLAS as f32) as usize);
            let tr = treeness[ayp.min(ATLAS - 1) * ATLAS + axp.min(ATLAS - 1)];
            let img = cx.img(p);
            let g = greenness(img);
            let lc = cover[ri];
            let density = match lc {
                LC_WOOD => if g < 0.02 && lum(axp, ayp) > 150.0 { 0.08 } else { 0.5 + 0.45 * g },
                LC_SCRUB => 0.22 + 0.3 * g,
                LC_GRASS => tr * tr * 1.3,
                LC_FARM => tr * tr * tr * 0.5,
                LC_WET => tr * 0.2,
                LC_BARE | LC_ROCK => 0.0,
                _ => tr * tr * 1.5,
            };
            if hash(s, 3) >= density {
                continue;
            }
            let ground = cx.ground(p);
            let hood = hoods.at(cx, p);
            let urban = urban_tile && lc != LC_WOOD && hood.cover > 0.05;
            let mut w = species_weights(cx.lat, cx.lon, ground, urban, arid, lc == LC_SCRUB);
            // Trees grow in stands: a slow noise field favours different species from place to place.
            let mut total = 0.0;
            for (k, wk) in w.iter_mut().enumerate() {
                let nz = vnoise(Vec3::new(m.x / 140.0 + k as f32 * 5.3, m.y / 140.0, k as f32 * 1.7 + (tseed % 97) as f32));
                *wk *= 0.15 + 2.2 * nz * nz;
                total += *wk;
            }
            if total <= 0.0 {
                continue;
            }
            // Dark, blue-ish canopy in the image is more likely conifer.
            let mut pickv = hash(s, 4) * total;
            let mut species = BUSH;
            for (k, wk) in w.iter().enumerate() {
                if pickv < *wk {
                    species = k;
                    break;
                }
                pickv -= wk;
            }
            let variant = (species * VARIANTS + (hash(s, 5) * VARIANTS as f32) as usize % VARIANTS) as u8;
            let young = hash(s, 6);
            let scale = (0.58 + 0.7 * young.powf(0.8)) * if lc == LC_WOOD { 1.1 } else { 0.92 } * if eff_cold(cx.lat, ground) { 0.7 } else { 1.0 };
            // Tint toward the canopy colour actually seen from above.
            let lin = [0, 1, 2].map(|k| (img[k] as f32 / 255.0).powf(2.2));
            let bright = 0.82 + 0.36 * hash(s, 7);
            let tint = [0, 1, 2].map(|k| {
                let target = LEAF[species][k] * 0.55 + lin[k] * 1.1 * 0.45;
                ((target / LEAF[species][k]).clamp(0.5, 1.8) * bright * 127.0).clamp(0.0, 255.0) as u8
            });
            let pos = cx.pos(p, ground - 0.2);
            let chunk = (((p[1] / ext * CH as f32) as usize).min(CH - 1) * CH + ((p[0] / ext * CH as f32) as usize).min(CH - 1)) as u8;
            cand.push((TreeInst { a: [pos.x, pos.y, pos.z, scale], b: [0.0, 0.0, hash(s, 8) * TAU, 0.85 + 0.35 * hash(s, 9)], tint: [tint[0], tint[1], tint[2], 255] }, variant, chunk, cell_of(p)));
        }
    }
    // Thin out evenly if a tile is wall-to-wall forest.
    const MAX: usize = 46000;
    if cand.len() > MAX {
        let keep = MAX as f32 / cand.len() as f32;
        let mut k = 0u32;
        cand.retain(|_| {
            k += 1;
            hash(k, tseed) < keep
        });
    }
    // Sorted by variant, then chunk: each chunk's trees of one variant are a
    // run, and the runs of neighbouring chunks follow on from each other, so
    // the renderer can draw many chunks of one variant as a single batch.
    cand.sort_by_key(|c| (c.1, c.2));
    let mut bounds = vec![(Vec3::splat(1e9), Vec3::splat(-1e9)); CH * CH];
    let mut runs: Vec<Vec<(u16, u32, u32)>> = vec![Vec::new(); CH * CH];
    let mut i = 0;
    while i < cand.len() {
        let (v, ch) = (cand[i].1, cand[i].2 as usize);
        let rs = i;
        let (lo, hi) = &mut bounds[ch];
        while i < cand.len() && cand[i].1 == v && cand[i].2 as usize == ch {
            let p = Vec3::new(cand[i].0.a[0], cand[i].0.a[1], cand[i].0.a[2]);
            *lo = lo.min(p);
            *hi = hi.max(p);
            i += 1;
        }
        runs[ch].push((v as u16, rs as u32, (i - rs) as u32));
    }
    let chunks: Vec<TreeChunk> = runs
        .into_iter()
        .zip(bounds)
        .filter(|(r, _)| !r.is_empty())
        .map(|(runs, (lo, hi))| TreeChunk { center: (lo + hi) * 0.5, radius: (hi - lo).length() * 0.5 + 14.0, runs })
        .collect();
    // What grows underneath: shrubs, ferns, saplings, tussocks and deadwood,
    // thick in the woods and thinning out into open country.
    const UCELL: f32 = 3.4;
    const MAX_UNDER: usize = 200_000;
    let un = (width_m / UCELL).ceil() as u32;
    let mut under: Vec<(u16, Vec3, Under)> = Vec::new();
    for iy in 0..un {
        for ix in 0..un {
            let s = (ix + iy * 16381) ^ tseed.rotate_left(7) ^ 0x5bd1e995;
            let m = Vec2::new((ix as f32 + hash(s, 1)) * UCELL - width_m * 0.5, (iy as f32 + hash(s, 2)) * UCELL - width_m * 0.5);
            let p = cx.units(m);
            if p[0] < 0.0 || p[1] < 0.0 || p[0] >= ext || p[1] >= ext {
                continue;
            }
            let (rx, ry) = (((p[0] / ext * RASTER as f32) as usize).min(RASTER - 1), ((p[1] / ext * RASTER as f32) as usize).min(RASTER - 1));
            let ri = ry * RASTER + rx;
            if block[ri] != 0 || built[ri] != 0 {
                continue;
            }
            let (axp, ayp) = (((p[0] / ext * ATLAS as f32) as usize).min(ATLAS - 1), ((p[1] / ext * ATLAS as f32) as usize).min(ATLAS - 1));
            let tr = treeness[ayp * ATLAS + axp];
            let lc = cover[ri];
            // Plants come in drifts rather than an even sprinkle.
            let drift = vnoise(Vec3::new(m.x / 23.0, m.y / 23.0, (tseed % 89) as f32));
            let density = match lc {
                LC_WOOD => if tr < 0.01 && lum(axp, ayp) > 150.0 { 0.05 } else { 0.9 },
                LC_SCRUB => 0.6,
                LC_GRASS => 0.03 + tr * 0.7,
                LC_FARM => tr * tr * 0.5,
                LC_WET => 0.25,
                LC_BARE | LC_ROCK => 0.0,
                _ => tr * 0.8,
            } * (0.35 + 1.3 * drift) * if arid { 0.45 } else { 1.0 };
            if hash(s, 3) >= density {
                continue;
            }
            let ground = cx.ground(p);
            let eff = cx.lat.abs() as f32 + ground.max(0.0) / 140.0;
            if eff > 68.0 {
                continue;
            }
            let wood = lc == LC_WOOD || tr > 0.45;
            let cold = eff > 60.0;
            // shrub, fern, sapling, grass, deadwood
            let mut w = match lc {
                LC_WOOD => [0.26, 0.30, 0.14, 0.22, 0.08],
                LC_SCRUB => [0.50, 0.02, 0.03, 0.40, 0.05],
                LC_WET => [0.10, 0.10, 0.0, 0.80, 0.0],
                _ => if wood { [0.32, 0.18, 0.12, 0.33, 0.05] } else { [0.35, 0.0, 0.05, 0.60, 0.0] },
            };
            if arid || cold {
                w[1] = 0.0;
                w[2] *= 0.4;
                w[3] += 0.3;
            }
            // Ferns like the damp, shady hollows of the drift pattern.
            w[1] *= 2.0 * (1.0 - drift);
            let mut pickv = hash(s, 4) * w.iter().sum::<f32>();
            let mut kind = 3;
            for (k, wk) in w.iter().enumerate() {
                if pickv < *wk {
                    kind = k;
                    break;
                }
                pickv -= wk;
            }
            let species = SHRUB + kind;
            let img = cx.img(p);
            let lin = [0, 1, 2].map(|k| (img[k] as f32 / 255.0).powf(2.2));
            let bright = 0.8 + 0.4 * hash(s, 7);
            let tint = [0, 1, 2].map(|k| {
                if species == DEADWOOD {
                    return (bright * 127.0) as u8;
                }
                let target = LEAF[species][k] * 0.6 + lin[k] * 1.1 * 0.4;
                ((target / LEAF[species][k]).clamp(0.55, 1.7) * bright * 127.0).clamp(0.0, 255.0) as u8
            });
            let scale = (0.65 + 0.75 * hash(s, 6)) * if cold { 0.75 } else { 1.0 };
            under.push((cell_of(p), cx.pos(p, ground - 0.04), Under { off: [0; 3], tint, var: (species * VARIANTS + (hash(s, 5) * VARIANTS as f32) as usize % VARIANTS) as u8, yaw: (hash(s, 8) * 255.0) as u8, scale: (scale * 100.0) as u8 }));
        }
    }
    if under.len() > MAX_UNDER {
        let keep = MAX_UNDER as f32 / under.len() as f32;
        let mut k = 0u32;
        under.retain(|_| {
            k += 1;
            hash(k, tseed ^ 77) < keep
        });
    }
    under.sort_by_key(|u| u.0);
    // Cells: which trees and which undergrowth stand in each patch of the tile.
    let mut near_idx: Vec<u32> = (0..cand.len() as u32).collect();
    near_idx.sort_by_key(|i| cand[*i as usize].3);
    let mut cells = Vec::new();
    let (mut ti, mut ui) = (0, 0);
    while ti < near_idx.len() || ui < under.len() {
        let id = near_idx.get(ti).map(|i| cand[*i as usize].3).unwrap_or(u16::MAX).min(under.get(ui).map(|u| u.0).unwrap_or(u16::MAX));
        let (t0, u0) = (ti, ui);
        let (mut lo, mut hi) = (Vec3::splat(1e9), Vec3::splat(-1e9));
        while ti < near_idx.len() && cand[near_idx[ti] as usize].3 == id {
            let a = cand[near_idx[ti] as usize].0.a;
            lo = lo.min(Vec3::new(a[0], a[1], a[2]));
            hi = hi.max(Vec3::new(a[0], a[1], a[2]));
            ti += 1;
        }
        while ui < under.len() && under[ui].0 == id {
            lo = lo.min(under[ui].1);
            hi = hi.max(under[ui].1);
            ui += 1;
        }
        let center = (lo + hi) * 0.5;
        for u in &mut under[u0..ui] {
            u.2.off = ((u.1 - center) * 100.0).clamp(Vec3::splat(-32767.0), Vec3::splat(32767.0)).to_array().map(|v| v as i16);
        }
        cells.push(NearCell { center, radius: (hi - lo).length() * 0.5, trees: (t0 as u32, (ti - t0) as u32), under: (u0 as u32, (ui - u0) as u32) });
    }
    Placed { var: cand.iter().map(|c| c.1).collect(), trees: cand.into_iter().map(|c| c.0).collect(), chunks, under: under.into_iter().map(|u| u.2).collect(), cells, near_idx }
}

fn eff_cold(lat: f64, elev: f32) -> bool {
    lat.abs() as f32 + elev.max(0.0) / 140.0 > 60.0
}
