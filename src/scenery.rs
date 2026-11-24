//! Hand-made airport scenery: packs modelled in Blender (assets/blender/ebaw/
//! for Antwerp, EBAW) and exported by `07_game_pack.py` as
//! `assets/scenery/<icao>/pack.bin.gz`. A pack holds textured meshes (the
//! terminal and tower, hangars, ground equipment, fences, signs, light masts),
//! where each stands in the pack's own east/north/up frame, the floodlights of
//! its masts, and the footprints where OpenStreetMap's generic buildings must
//! give way to it. The airfield keeps its own pavement, paint and lights; the
//! pack replaces the procedural windsocks and floodlight masts.
//!
//! Drawn by `scenery.wgsl` (render.rs `set_scenery`): base colour and a
//! normal / roughness / metallic picture per material from two texture arrays.
use crate::airports::Geo;
use bytemuck::{Pod, Zeroable};
use glam::{DVec3, Vec2, Vec3};
use std::io::Read;
use std::sync::OnceLock;

/// The packs built into the game: (ICAO code, gzip'd pack).
const PACKS: &[(&str, &[u8])] = &[("EBAW", include_bytes!("../assets/scenery/ebaw/pack.bin.gz"))];

/// The pack replaces the airfield's procedural windsocks / floodlight masts.
pub const F_SOCKS: u32 = 1;
pub const F_MASTS: u32 = 2;

/// Instance classes: how far away each is still drawn (m).
#[cfg_attr(not(test), allow(dead_code))] // (the default: what `reach` gives the rest)
pub const C_BUILDING: u32 = 0;
pub const C_PROP: u32 = 1;
pub const C_SOCK: u32 = 2;
pub const C_SMALL: u32 = 3;
pub fn reach(class: u32) -> f32 {
    match class {
        C_PROP | C_SOCK => 2_500.0,
        C_SMALL => 900.0,
        _ => 12_000.0,
    }
}

/// One vertex of a scenery mesh (scenery.wgsl): normal and tangent as signed
/// bytes (normal w: ambient occlusion 0..127, baked in Blender; tangent w: the
/// bitangent's sign), texture coordinates in metres.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct SceneryVertex {
    pub pos: [f32; 3],
    pub nrm: [i8; 4],
    pub tan: [i8; 4],
    pub uv: [f32; 2],
    pub mat: u32,
}

/// A material as the shader reads it (storage buffer, 4 x vec4).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable, Debug)]
pub struct SceneryMat {
    /// base colour (used when there is no texture layer) and alpha
    pub base: [f32; 4],
    /// roughness, metallic, metres per texture repeat, layer (-1: none)
    pub p: [f32; 4],
    /// emission colour and strength (lit at night)
    pub emit: [f32; 4],
    /// transmission (glass), flags (1 glass, 2 alpha cut-out, 4 lit, 8 takes the instance tint), unused
    pub q: [f32; 4],
}

pub struct PackMesh {
    #[allow(dead_code)]
    pub name: String,
    pub radius: f32,
    pub verts: Vec<SceneryVertex>,
    pub idx: Vec<u32>,
}

pub struct PackInst {
    pub mesh: u32,
    pub class: u32,
    /// where it stands on the ground (pack metres east, north)
    pub anchor: Vec2,
    /// 3 rows of a 3x4 matrix, pack frame (x east, y north, z up)
    pub m: [[f32; 4]; 3],
    /// colour multiplier for the materials that take it (a roof as the orthophoto shows it)
    pub tint: [f32; 3],
}

pub struct Pack {
    pub icao: &'static str,
    pub lat: f64,
    pub lon: f64,
    #[allow(dead_code)]
    pub elev: f32,
    pub flags: u32,
    pub tex_size: u32,
    /// per layer: base colour + alpha, normal xy + roughness + metallic (RGBA8)
    pub layers: Vec<(Vec<u8>, Vec<u8>)>,
    pub mats: Vec<SceneryMat>,
    pub meshes: Vec<PackMesh>,
    pub insts: Vec<PackInst>,
    /// floodlights: position (pack frame), direction
    pub floods: Vec<(Vec3, Vec3)>,
    /// footprints where OSM buildings give way (pack frame)
    pub masks: Vec<Vec<Vec2>>,
    /// ground patches laid over the airfield's own pavement (pack frame)
    pub ground: Vec<GroundPatch>,
    /// index of this pack's first mesh in the scenery mesh library
    pub mesh_base: u32,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> &'a [u8] {
        let s = &self.b[self.at..self.at + n];
        self.at += n;
        s
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn i32(&mut self) -> i32 {
        i32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn f32(&mut self) -> f32 {
        f32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn f64(&mut self) -> f64 {
        f64::from_le_bytes(self.take(8).try_into().unwrap())
    }
    fn f32s<const N: usize>(&mut self) -> [f32; N] {
        std::array::from_fn(|_| self.f32())
    }
}

fn decode(bytes: &[u8], size: u32) -> Vec<u8> {
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let img = img.to_rgba8();
            if img.width() == size && img.height() == size {
                img.into_raw()
            } else {
                image::imageops::resize(&img, size, size, image::imageops::FilterType::Triangle).into_raw()
            }
        }
        Err(_) => vec![128; (size * size * 4) as usize],
    }
}

impl Pack {
    pub fn parse(icao: &'static str, gz: &[u8], mesh_base: u32) -> Pack {
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(gz).read_to_end(&mut raw).unwrap_or_else(|_| panic!("{icao} scenery pack"));
        let mut r = Reader { b: &raw, at: 0 };
        assert_eq!(r.take(4), b"FFSC", "{icao} scenery: not a pack");
        assert_eq!(r.u32(), 3, "{icao} scenery: unknown version (rebuild it with 07_game_pack.py)");
        let (lat, lon, elev) = (r.f64(), r.f64(), r.f32());
        let flags = r.u32();
        let (tex_size, nl) = (r.u32(), r.u32());
        let mut layers = Vec::with_capacity(nl as usize);
        for _ in 0..nl {
            let n = r.u32() as usize;
            let base = decode(r.take(n), tex_size);
            let n = r.u32() as usize;
            let nrm = decode(r.take(n), tex_size);
            layers.push((base, nrm));
        }
        let nm = r.u32();
        let mats = (0..nm)
            .map(|_| {
                let base = r.f32s::<4>();
                let (rough, metal) = (r.f32(), r.f32());
                let em = r.f32s::<3>();
                let strength = r.f32();
                let (trans, tile) = (r.f32(), r.f32());
                let layer = r.i32();
                let fl = r.u32();
                let _pad = r.f32();
                SceneryMat { base, p: [rough, metal, tile.max(0.01), layer as f32], emit: [em[0], em[1], em[2], strength], q: [trans, fl as f32, 0.0, 0.0] }
            })
            .collect();
        let nmesh = r.u32();
        let meshes = (0..nmesh)
            .map(|_| {
                let ln = r.u32() as usize;
                let name = String::from_utf8_lossy(r.take(ln)).into_owned();
                let radius = r.f32();
                let nv = r.u32() as usize;
                let verts: Vec<SceneryVertex> = bytemuck::pod_collect_to_vec(r.take(nv * std::mem::size_of::<SceneryVertex>()));
                let ni = r.u32() as usize;
                let idx: Vec<u32> = bytemuck::pod_collect_to_vec(r.take(ni * 4));
                PackMesh { name, radius, verts, idx }
            })
            .collect();
        let ni = r.u32();
        let insts = (0..ni)
            .map(|_| {
                let (mesh, class) = (r.u32(), r.u32());
                let a = r.f32s::<2>();
                let m = r.f32s::<12>();
                let tint = r.f32s::<3>();
                PackInst { mesh, class, anchor: Vec2::from(a), m: [[m[0], m[1], m[2], m[3]], [m[4], m[5], m[6], m[7]], [m[8], m[9], m[10], m[11]]], tint }
            })
            .collect();
        let nf = r.u32();
        let floods = (0..nf)
            .map(|_| {
                let v = r.f32s::<6>();
                (Vec3::new(v[0], v[1], v[2]), Vec3::new(v[3], v[4], v[5]))
            })
            .collect();
        let nk = r.u32();
        let masks = (0..nk)
            .map(|_| {
                let n = r.u32() as usize;
                (0..n).map(|_| Vec2::from(r.f32s::<2>())).collect()
            })
            .collect();
        let ng = r.u32();
        let ground = (0..ng)
            .map(|_| {
                let (kind, axis, nr) = (r.u32(), r.f32(), r.u32());
                let rings = (0..nr)
                    .map(|_| {
                        let n = r.u32() as usize;
                        (0..n).map(|_| Vec2::from(r.f32s::<2>())).collect()
                    })
                    .collect();
                GroundPatch { kind, axis, rings }
            })
            .collect();
        assert_eq!(r.at, raw.len(), "{icao} scenery: trailing bytes");
        Pack { icao, lat, lon, elev, flags, tex_size, layers, mats, meshes, insts, floods, masks, ground, mesh_base }
    }

    /// The pack's frame: its origin on the sphere and east, north, up there.
    fn frame(&self) -> (DVec3, DVec3, DVec3, DVec3) {
        let o = crate::geo::lla_to_pos(self.lat.to_radians(), self.lon.to_radians(), 0.0);
        let (e, n, u) = crate::geo::enu(o);
        (o, e, n, u)
    }

    /// Pack metres (east, north) of a latitude and longitude (degrees).
    pub fn local_of(&self, lat: f64, lon: f64) -> Vec2 {
        let (o, e, n, u) = self.frame();
        let p = crate::geo::lla_to_pos(lat.to_radians(), lon.to_radians(), 0.0);
        // onto the tangent plane, along the ray from the planet's centre
        let s = crate::geo::R / p.dot(u).max(1.0);
        let v = p * s - o;
        Vec2::new(v.dot(e) as f32, v.dot(n) as f32)
    }

    /// Whether OSM's own building at (lat, lon) must give way to the pack's.
    pub fn masks_point(&self, lat: f64, lon: f64) -> bool {
        let p = self.local_of(lat, lon);
        if p.length() > 5_000.0 {
            return false;
        }
        self.masks.iter().any(|ring| in_ring(p, ring))
    }

    /// Where everything stands at this airfield: each instance's matrix
    /// relative to `g.center` (planet frame), its mesh in the library, its
    /// class and bounding radius; each one stands on the terrain under its anchor.
    pub fn place(&self, g: &Geo) -> Vec<Placed> {
        let (o, e, n, _) = self.frame();
        let mut out = Vec::with_capacity(self.insts.len());
        for it in &self.insts {
            let ground = self.ground(g, o, e, n, it.anchor);
            let up = ground.normalize();
            let t = Vec3::new(it.m[0][3] - it.anchor.x, it.m[1][3] - it.anchor.y, it.m[2][3]);
            let pos = ground + e * t.x as f64 + n * t.y as f64 + up * t.z as f64 - g.center;
            // columns of the rotation (pack axes) mapped to the planet frame
            let col = |c: usize| (e * it.m[0][c] as f64 + n * it.m[1][c] as f64 + up * it.m[2][c] as f64).as_vec3();
            let (cx, cy, cz) = (col(0), col(1), col(2));
            let p = pos.as_vec3();
            let m = [[cx.x, cy.x, cz.x, p.x], [cx.y, cy.y, cz.y, p.y], [cx.z, cy.z, cz.z, p.z]];
            out.push(Placed { mesh: self.mesh_base + it.mesh, class: it.class, m, radius: self.meshes[it.mesh as usize].radius, up: up.as_vec3(), tint: it.tint });
        }
        out
    }

    /// The floodlights, as local airfield points: (where, lift above the ground, direction east/north/up).
    pub fn flood_lights(&self, g: &Geo) -> Vec<(Vec2, f32, Vec3)> {
        let (o, e, n, _) = self.frame();
        self.floods
            .iter()
            .map(|(p, d)| {
                let w = self.ground(g, o, e, n, Vec2::new(p.x, p.y));
                (g.local(w), p.z, *d)
            })
            .collect()
    }

    /// A pack point (metres east, north) as a local point of the airfield.
    pub fn to_field(&self, g: &Geo, p: Vec2) -> Vec2 {
        let (o, e, n, _) = self.frame();
        g.local(o + e * p.x as f64 + n * p.y as f64)
    }

    /// The point on the terrain under pack metres `a`.
    fn ground(&self, g: &Geo, o: DVec3, e: DVec3, n: DVec3, a: Vec2) -> DVec3 {
        let flat = o + e * a.x as f64 + n * a.y as f64;
        let h = g.h(g.local(flat));
        flat.normalize() * (crate::geo::R + h as f64)
    }
}

/// A patch of pavement over the aprons: concrete pads on asphalt, as the
/// orthophoto shows them.
pub struct GroundPatch {
    /// 1: concrete
    pub kind: u32,
    /// the slabs' direction, radians from east (pack frame)
    pub axis: f32,
    /// outer ring, then holes
    pub rings: Vec<Vec<Vec2>>,
}
pub const G_CONCRETE: u32 = 1;

/// An instance placed at an airfield (relative to its centre).
#[derive(Clone, Copy)]
pub struct Placed {
    pub mesh: u32,
    pub class: u32,
    pub m: [[f32; 4]; 3],
    pub radius: f32,
    /// local up where it stands (the windsocks turn about it)
    pub up: Vec3,
    pub tint: [f32; 3],
}

fn in_ring(p: Vec2, r: &[Vec2]) -> bool {
    let mut c = false;
    let mut j = r.len().wrapping_sub(1);
    for i in 0..r.len() {
        let (a, b) = (r[i], r[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
        j = i;
    }
    c
}

/// All the packs (parsed once).
pub fn packs() -> &'static [Pack] {
    static P: OnceLock<Vec<Pack>> = OnceLock::new();
    P.get_or_init(|| {
        let mut base = 0u32;
        PACKS
            .iter()
            .map(|(icao, gz)| {
                let p = Pack::parse(icao, gz, base);
                base += p.meshes.len() as u32;
                p
            })
            .collect()
    })
}

/// `--no-scenery`: the airfields as the game builds them, no packs.
pub static DISABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
fn on() -> bool {
    !DISABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// The pack for an airport, by its identifier.
pub fn for_airport(ident: &str) -> Option<&'static Pack> {
    packs().iter().filter(|_| on()).find(|p| p.icao.eq_ignore_ascii_case(ident))
}

/// Whether an OSM building at (lat, lon) degrees stands where a pack has its own.
pub fn masked(lat: f64, lon: f64) -> bool {
    on() && packs().iter().any(|p| p.masks_point(lat, lon))
}

/// The windsock's axes for this frame: it hangs from its pivot along +Y,
/// streams downwind and droops in a light breeze (as the procedural socks,
/// `world::sock_dir`). Returns (x, y, z) columns in the planet frame.
pub fn sock_axes(fwd: Vec3, up: Vec3) -> (Vec3, Vec3, Vec3) {
    let upv = (up - fwd * fwd.dot(up)).normalize();
    (fwd.cross(upv), fwd, upv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ebaw_pack_reads() {
        let p = for_airport("ebaw").expect("EBAW pack");
        assert!((p.lat - 51.19).abs() < 0.01 && (p.lon - 4.463).abs() < 0.01);
        assert!(p.flags & F_SOCKS != 0 && p.flags & F_MASTS != 0);
        assert!(!p.meshes.is_empty() && p.insts.len() > 1000);
        for m in &p.meshes {
            assert!(m.idx.len() % 3 == 0);
            assert!(m.idx.iter().all(|&i| (i as usize) < m.verts.len()));
            assert!(m.verts.iter().all(|v| (v.mat as usize) < p.mats.len()));
        }
        for mat in &p.mats {
            assert!(mat.p[3] < p.layers.len() as f32);
        }
        assert!(p.insts.iter().all(|i| (i.mesh as usize) < p.meshes.len()));
        assert_eq!(p.insts.iter().filter(|i| i.class == C_SOCK).count(), 3, "three windsocks");
        assert!(p.insts.iter().any(|i| i.class == C_BUILDING) && p.insts.iter().any(|i| i.class == C_SMALL));
        assert!(reach(C_BUILDING) > reach(C_PROP) && reach(C_PROP) > reach(C_SMALL));
        assert_eq!(p.floods.len(), 4);
        // the aprons' concrete pads: closed rings, tens of thousands of m2
        assert!(!p.ground.is_empty() && p.ground.iter().all(|gp| gp.kind == G_CONCRETE && gp.rings[0].len() >= 3));
        let area: f32 = p.ground.iter().map(|gp| {
            let r = &gp.rings[0];
            (0..r.len()).map(|i| r[i].perp_dot(r[(i + 1) % r.len()])).sum::<f32>().abs() * 0.5
        }).sum();
        assert!((20_000.0..120_000.0).contains(&area), "{area} m2 of concrete");
        // occlusion baked into the buildings' own meshes, and some roofs tinted
        let ao: Vec<i8> = p.meshes.iter().flat_map(|m| m.verts.iter().map(|v| v.nrm[3])).collect();
        assert!(ao.iter().all(|&a| a >= 0), "occlusion is 0..127");
        assert!(ao.iter().filter(|&&a| a < 100).count() * 10 > ao.len(), "some occlusion");
        assert!(p.insts.iter().any(|i| i.tint != [1.0; 3]) && p.insts.iter().all(|i| i.tint.iter().all(|c| (0.2..=3.5).contains(c))));
        let sz = (p.tex_size * p.tex_size * 4) as usize;
        assert!(p.layers.iter().all(|(b, n)| b.len() == sz && n.len() == sz));
    }

    #[test]
    fn masks_cover_the_modelled_buildings_only() {
        // OSM way 4414121 (the terminal) and 445909590 (the big south hangar), by a node of each.
        assert!(masked(51.18998612 + (-111.0 / 111_257.0), 4.46305325 + (-820.0 / 69_860.0)), "terminal");
        assert!(masked(51.18998612 + (-392.0 / 111_257.0), 4.46305325 + (-366.0 / 69_860.0)), "south hangar");
        // the middle of the runway, and the city of Antwerp
        assert!(!masked(51.18998612, 4.46305325));
        assert!(!masked(51.2194, 4.4025));
    }

    #[test]
    fn local_frame_round_trip() {
        let p = &packs()[0];
        assert!(p.local_of(p.lat, p.lon).length() < 0.01);
        // 0.001 deg of latitude is about 111 m north
        let q = p.local_of(p.lat + 0.001, p.lon);
        assert!(q.x.abs() < 0.05 && (q.y - 111.2).abs() < 0.5, "{q:?}");
    }

    #[test]
    fn sock_axes_are_a_rotation() {
        let fwd = Vec3::new(0.6, 0.0, -0.8).normalize();
        let (x, y, z) = sock_axes(fwd, Vec3::Z);
        assert!((x.cross(y) - z).length() < 1e-5, "right-handed");
        assert!(x.dot(y).abs() < 1e-5 && y.dot(z).abs() < 1e-5);
    }
}
