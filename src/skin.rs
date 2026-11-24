//! A destroyed building's own look, in the zone of photorealistic 3D tiles:
//! the building is cut out of the mesh (tiles3d.rs, `Photo::take_building`)
//! and the triangles that were inside the cut ride on the pieces the wreck
//! breaks into (destruct.rs). So it is the real building, the photograph of
//! its walls and roof, that leans, breaks up and comes down.
//!
//! Every triangle is split small enough to follow the pieces' edges (finer
//! round the aircraft's hole) and given to the nearest piece of outer wall or
//! roof; triangles of pieces that were blown out go with them, which leaves
//! the hole. Each piece has a slot: where it is now, and how much soot and
//! dust it has taken on (photo.wgsl, `vs_skin`). A piece that breaks in two
//! hands the triangles past the break to a new slot.
use crate::render::{Renderer, SkinGpu, SkinVertex, SKIN_SLOTS};
use crate::tiles3d::SkinSrc;
use glam::{DQuat, DVec3, Mat3, Vec3};
use std::collections::HashMap;

/// A piece without any of the mesh on it.
pub const NO_SKIN: u32 = u32::MAX;
/// The slot of triangles that have gone (never drawn).
const DEAD: u32 = 0;
/// Triangles after splitting, at most per building.
const MAX_TRIS: usize = 260_000;
/// Edge length (m) the triangles are split to round the aircraft's hole.
const FINE_EDGE: f32 = 0.3;
/// Farthest a triangle may lie from a piece and still go with it (m).
const REACH: f32 = 3.0;

/// A piece of the wreck as it stands, before it is built.
pub struct PieceRef {
    pub pos: DVec3,
    pub rot: DQuat,
    pub half: Vec3,
    /// a piece of outer wall or roof (the mesh only covers those)
    pub outer: bool,
}

struct SubTri {
    p: [DVec3; 3],
    uv: [[f32; 2]; 3],
    tex: u32,
    piece: u32,
}

/// The mesh split up and handed out over the pieces.
pub struct Assign {
    tris: Vec<SubTri>,
    /// how far to move each piece out along its thin axis (local y) so its
    /// face lies on the mesh it carries (the mesh and the map rarely agree)
    pub shift: Vec<f32>,
    /// mean colour of the mesh on each piece (linear), and of all of it
    pub col: Vec<Option<[f32; 3]>>,
    pub mean: [f32; 3],
    /// how much of the mesh each piece carries (m²)
    pub area: Vec<f32>,
}

impl Assign {
    pub fn n_tris(&self) -> usize {
        self.tris.len()
    }
}

/// Splits the cut-out triangles and gives each to the nearest piece of outer
/// wall or roof. `fine`: round this point (and this far from it) they are
/// split finer, to follow the aircraft's hole; `edge`: their longest edge elsewhere.
pub fn assign(src: &SkinSrc, pieces: &[PieceRef], fine: Option<(DVec3, f64)>, edge: f32) -> Assign {
    let area = |p: &[DVec3; 3]| ((p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5) as f32;
    let total: f32 = src.tiles.iter().flat_map(|t| t.tris.iter()).map(|t| area(&t.0)).sum();
    // Coarser everywhere if the whole would be too many.
    let edge = edge.max((total / (0.43 * MAX_TRIS as f32 * 0.7)).sqrt());
    let mut tris: Vec<SubTri> = Vec::new();
    for (k, tile) in src.tiles.iter().enumerate() {
        for (p, uv) in &tile.tris {
            let mut stack = vec![(*p, *uv)];
            while let Some((p, uv)) = stack.pop() {
                let c = (p[0] + p[1] + p[2]) / 3.0;
                let l = match fine {
                    Some((f, r)) if (c - f).length_squared() < r * r => FINE_EDGE,
                    _ => edge,
                };
                let e2 = [(p[1] - p[0]).length_squared(), (p[2] - p[1]).length_squared(), (p[0] - p[2]).length_squared()];
                let i = if e2[0] >= e2[1] && e2[0] >= e2[2] { 0 } else if e2[1] >= e2[2] { 1 } else { 2 };
                if e2[i] > (l * l) as f64 && tris.len() + stack.len() < MAX_TRIS {
                    // In two across the longest edge.
                    let (a, b, o) = (i, (i + 1) % 3, (i + 2) % 3);
                    let m = (p[a] + p[b]) * 0.5;
                    let mu = [(uv[a][0] + uv[b][0]) * 0.5, (uv[a][1] + uv[b][1]) * 0.5];
                    let _ = o;
                    let mut t1 = (p, uv);
                    t1.0[b] = m;
                    t1.1[b] = mu;
                    let mut t2 = (p, uv);
                    t2.0[a] = m;
                    t2.1[a] = mu;
                    stack.push(t1);
                    stack.push(t2);
                    continue;
                }
                if src.inside(c) {
                    tris.push(SubTri { p, uv, tex: k as u32, piece: u32::MAX });
                }
            }
        }
    }
    // The outer pieces in a grid on the cut's axes, each in every cell it
    // comes within reach of.
    const G: f64 = 2.5;
    let key = |p: DVec3| {
        let d = p - src.o;
        ((d.dot(src.e) / G).floor() as i32, (d.dot(src.n) / G).floor() as i32, (d.dot(src.u) / G).floor() as i32)
    };
    let mut grid: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();
    let inv: Vec<Mat3> = pieces.iter().map(|p| Mat3::from_quat(p.rot.as_quat()).transpose()).collect();
    for (i, pc) in pieces.iter().enumerate() {
        if !pc.outer {
            continue;
        }
        let r = pc.half.length() as f64 + REACH as f64;
        let (a, b) = (key(pc.pos - (src.e + src.n + src.u) * r), key(pc.pos + (src.e + src.n + src.u) * r));
        let (a, b) = ((a.0.min(b.0), a.1.min(b.1), a.2.min(b.2)), (a.0.max(b.0), a.1.max(b.1), a.2.max(b.2)));
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                for z in a.2..=b.2 {
                    grid.entry((x, y, z)).or_default().push(i as u32);
                }
            }
        }
    }
    let dist = |i: u32, p: DVec3| -> (f32, Vec3) {
        let pc = &pieces[i as usize];
        let l = inv[i as usize] * (p - pc.pos).as_vec3();
        ((l.abs() - pc.half).max(Vec3::ZERO).length(), l)
    };
    let n = pieces.len();
    let (mut ys, mut ws, mut cols) = (vec![0.0f32; n], vec![0.0f32; n], vec![[0.0f32; 3]; n]);
    let mut all = [0.0f32; 3];
    let mut wall = 0.0f32;
    tris.retain_mut(|t| {
        let c = (t.p[0] + t.p[1] + t.p[2]) / 3.0;
        let Some(cell) = grid.get(&key(c)) else { return false };
        let Some((best, d, l)) = cell.iter().map(|&i| (i, dist(i, c))).map(|(i, (d, l))| (i, d, l)).min_by(|a, b| a.1.total_cmp(&b.1)) else { return false };
        if d > REACH {
            return false;
        }
        t.piece = best;
        let a = area(&t.p);
        let b = best as usize;
        ys[b] += l.y * a;
        ws[b] += a;
        let uv = [(t.uv[0][0] + t.uv[1][0] + t.uv[2][0]) / 3.0, (t.uv[0][1] + t.uv[1][1] + t.uv[2][1]) / 3.0];
        let col = src.tiles[t.tex as usize].tiny.at(uv);
        for j in 0..3 {
            cols[b][j] += col[j] * a;
            all[j] += col[j] * a;
        }
        wall += a;
        true
    });
    let mean = if wall > 0.0 { all.map(|v| v / wall) } else { [0.3; 3] };
    let shift = (0..n).map(|i| if ws[i] > 0.5 { (ys[i] / ws[i] - pieces[i].half.y).clamp(-3.0, 3.0) } else { 0.0 }).collect();
    let col = (0..n).map(|i| (ws[i] > 0.0).then(|| cols[i].map(|v| v / ws[i]))).collect();
    Assign { tris, shift, col, mean, area: ws }
}

/// A piece's slot: where its mesh's own origin lies on the piece's axes (a
/// piece that broke in two is no longer centred where it was), the colour
/// it had when it was cut out, and the runs of triangles it may have.
struct Slot {
    off: Vec3,
    col0: [f32; 3],
    /// (texture, first triangle, end)
    runs: Vec<(u32, u32, u32)>,
}

struct Tex {
    view: wgpu::TextureView,
    verts: Vec<SkinVertex>,
    gpu: Option<SkinGpu>,
    /// triangles rewritten since the last upload
    dirty: Option<(u32, u32)>,
}

/// The mesh of one destroyed building, on its pieces.
pub struct Group {
    slots: Vec<Slot>,
    texs: Vec<Tex>,
    /// this frame's slots (photo.wgsl `Slot`), filled by destruct.rs
    pub xf: Vec<[[f32; 4]; 4]>,
}

impl Group {
    pub fn new(src: &SkinSrc) -> Self {
        let texs = src.tiles.iter().map(|t| Tex { view: t.view.clone(), verts: Vec::new(), gpu: None, dirty: None }).collect();
        Self { slots: vec![Slot { off: Vec3::ZERO, col0: [1.0; 3], runs: Vec::new() }], texs, xf: Vec::new() }
    }

    /// A slot for a piece whose mesh has colour `col0` (DEAD when full).
    pub fn alloc(&mut self, col0: [f32; 3]) -> u32 {
        if self.slots.len() >= SKIN_SLOTS {
            return DEAD;
        }
        self.slots.push(Slot { off: Vec3::ZERO, col0, runs: Vec::new() });
        self.slots.len() as u32 - 1
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// The slot as the shader takes it, for a piece at `pos` (camera-relative)
    /// turned by `rot`, of colour `col` now.
    pub fn slot_xf(&self, s: u32, pos: Vec3, rot: DQuat, col: [f32; 3]) -> [[f32; 4]; 4] {
        let sl = &self.slots[s as usize];
        let m = Mat3::from_quat(rot.as_quat());
        let t = pos + m * sl.off;
        let k = |j: usize| (col[j] / sl.col0[j].max(1e-3)).clamp(0.15, 1.6);
        [[m.x_axis.x, m.y_axis.x, m.z_axis.x, t.x], [m.x_axis.y, m.y_axis.y, m.z_axis.y, t.y], [m.x_axis.z, m.y_axis.z, m.z_axis.z, t.z], [k(0), k(1), k(2), 1.0]]
    }

    /// Puts the triangles on their pieces: `slot_of` each piece's slot
    /// (NO_SKIN: blown out, its triangles gone with it), `frames` where each
    /// piece stands and how it is turned.
    pub fn build(&mut self, a: &Assign, slot_of: &[u32], frames: &[(DVec3, DQuat)]) {
        let mut per: Vec<Vec<(u32, [SkinVertex; 3])>> = vec![Vec::new(); self.texs.len()];
        for t in &a.tris {
            let s = slot_of[t.piece as usize];
            if s == NO_SKIN || s == DEAD {
                continue;
            }
            let (pos, rot) = frames[t.piece as usize];
            let inv = Mat3::from_quat(rot.as_quat()).transpose();
            let l = t.p.map(|p| inv * (p - pos).as_vec3());
            let nrm = (l[1] - l[0]).cross(l[2] - l[0]).normalize_or_zero();
            let q = |v: f32| (v * 127.0).round().clamp(-127.0, 127.0) as i8;
            let nr = [q(nrm.x), q(nrm.y), q(nrm.z), 0];
            per[t.tex as usize].push((s, [0, 1, 2].map(|k| SkinVertex { pos: l[k].to_array(), uv: t.uv[k], nrm: nr, slot: s })));
        }
        for (k, mut v) in per.into_iter().enumerate() {
            v.sort_by_key(|x| x.0);
            let mut i = 0;
            while i < v.len() {
                let s = v[i].0;
                let j = i + v[i..].iter().take_while(|x| x.0 == s).count();
                self.slots[s as usize].runs.push((k as u32, i as u32, j as u32));
                i = j;
            }
            self.texs[k].verts = v.into_iter().flat_map(|x| x.1).collect();
        }
    }

    /// A piece broke in two across its local `axis`: the part past `cut`
    /// (on its axes before the break) is a new piece, moved `sh_o` back along
    /// the axis, while the rest moved `sh_c` forward. Returns the new piece's slot.
    pub fn split(&mut self, parent: u32, axis: usize, cut: f32, sh_c: f32, sh_o: f32) -> u32 {
        if parent == NO_SKIN || parent == DEAD || parent as usize >= self.slots.len() {
            return parent;
        }
        let col0 = self.slots[parent as usize].col0;
        let new = self.alloc(col0);
        let off = self.slots[parent as usize].off;
        let runs = self.slots[parent as usize].runs.clone();
        for &(k, s, e) in &runs {
            let Some(t) = self.texs.get_mut(k as usize) else { continue };
            let mut any = false;
            for i in s as usize..e as usize {
                let v = &mut t.verts[i * 3..i * 3 + 3];
                if v[0].slot != parent {
                    continue;
                }
                let c = (Vec3::from(v[0].pos) + Vec3::from(v[1].pos) + Vec3::from(v[2].pos)) / 3.0 + off;
                if c[axis] > cut {
                    for x in v.iter_mut() {
                        x.slot = new;
                    }
                    any = true;
                }
            }
            if any {
                t.dirty = Some(t.dirty.map_or((s, e), |(a, b)| (a.min(s), b.max(e))));
            }
        }
        let ax = Vec3::AXES[axis];
        self.slots[parent as usize].off = off + ax * sh_c;
        if new != DEAD {
            let sl = &mut self.slots[new as usize];
            sl.off = off - ax * sh_o;
            sl.runs = runs;
        }
        new
    }

    /// Copies new and rewritten triangles to the GPU.
    pub fn upload(&mut self, r: &Renderer) {
        for t in &mut self.texs {
            match (&t.gpu, t.dirty.take()) {
                (None, _) if !t.verts.is_empty() => t.gpu = Some(r.make_skin(&t.verts, &t.view)),
                (Some(g), Some((s, e))) => r.write_skin(g, s as usize * 3, &t.verts[s as usize * 3..e as usize * 3]),
                _ => {}
            }
        }
    }

    pub fn gpus(&self) -> impl Iterator<Item = &SkinGpu> {
        self.texs.iter().filter_map(|t| t.gpu.as_ref())
    }
}
