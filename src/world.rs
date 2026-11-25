//! Streaming world: a quadtree of terrain tiles plus OSM building tiles,
//! loaded around the camera, with ground/obstacle queries for the physics.
use crate::buildings::{self, Footprint, Parked, TreeChunk};
use crate::cars::{self, Car};
use crate::geo::{self, TileId};
use crate::render::{BuildGpu, DetailDraw, Inst, NearDraw, Renderer, TessCells, TileGpu, TreeDraw, TreeInst, MAX_NEAR};
use crate::roads::{RoadHit, RoadIndex, RoadNet};
use crate::trees::{tree_jit, NearCell, TreeDim, Under, CARDS, CARD_RANGE, IMP_DIST, LODS, LOD_JITTER, TREE_SPECIES, VARIANTS};
use crate::weapons::{Target, TargetKey};
use crate::sources::{Job, JobResult, Loader, DATA_ZOOM, GRID, GV, MASK};
use crate::surface::{Surface, SurfaceHit};
use glam::{DMat4, DVec3, DVec4, Vec3};
// Fx hashing: these maps are hit many times a frame with small, locally made keys (no untrusted input).
use rustc_hash::FxHashMap as HashMap;
use std::collections::VecDeque;

struct Tile {
    center: DVec3,
    radius: f64,
    gpu: TileGpu,
    /// the grid vertices' positions as uploaded, to pick the patch each
    /// tessellated cell is drawn with (`tess_cells`)
    grid: Vec<[f32; 3]>,
    heights: Vec<f32>,
    water: Vec<bool>,
    /// one `surface::Surface` byte per MASK cell (sources.rs TileData::surface)
    surface: Vec<u8>,
    /// how much of the ocean's long waves the water shows (sources.rs TileData::sea)
    sea: Vec<f32>,
    /// any water on the tile, so the ocean fft can idle without it.
    wet: bool,
    last_used: u64,
    /// imagery source it was made with; a different one gets it made again
    generation: u32,
    refreshing: bool,
}
enum Slot {
    Loading,
    Ready(Box<Tile>),
    Failed,
}
struct BTile {
    center: DVec3,
    radius: f64,
    gpu: Option<BuildGpu>,
    footprints: Vec<Footprint>,
    last_used: u64,
    net: RoadNet,
    #[allow(dead_code)]
    road_index: RoadIndex,
    cars: Vec<Car>,
    parked: Vec<Parked>,
    trees: Vec<TreeInst>,
    tree_var: Vec<u8>,
    chunks: Vec<TreeChunk>,
    under: Vec<Under>,
    cells: Vec<NearCell>,
    near_idx: Vec<u32>,
    ax: [f32; 4],
    ay: [f32; 4],
    /// seconds the tile has been shown, and has been close enough for the
    /// middle and the finest level of detail (for fading them in)
    shown: f32,
    mid_t: f32,
    fine_t: f32,
    /// distance scale of its tallest height class (buildings::HCLASS)
    tall: f32,
}
/// How far from the camera a floodlight still lights the ground (m).
const FLOOD_REACH: f64 = 3000.0;
/// Every moving car this close to the camera is drawn, whatever the time of
/// day. All traffic is solid to the player, so a car it can hit must never be
/// invisible: the activity share thins out only the far ones.
const CAR_NEAR_DRAW: f64 = 300.0;
/// A mast's floodlights: about 1e5 cd on the beam's axis (Globals units, as
/// the landing lights: cd / 1e4 / pi, times 60), and the beam's half-angle.
const FLOOD_I: f32 = 1.0e5 / 1.0e4 / std::f32::consts::PI * 60.0;
const FLOOD_COS: f32 = 0.42;
/// An airfield near enough to be drawn and to be busy.
pub struct Field {
    pub geo: std::sync::Arc<crate::airports::Geo>,
    gpu: crate::render::FieldGpu,
    props: Vec<crate::airfield::Prop>,
    socks: Vec<Vec3>,
    /// a hand-made scenery pack's objects (scenery.rs)
    scenery: Vec<crate::scenery::Placed>,
    /// floodlights that light the ground: lamp (relative to `geo.center`), beam direction
    floods: Vec<(Vec3, Vec3)>,
    pub traffic: crate::traffic::Traffic,
    last_used: u64,
}
enum FSlot {
    Loading,
    Ready(Box<Field>),
    Failed,
}
enum BSlot {
    Loading,
    Ready(Box<BTile>),
}
/// A building tile whose geometry is still being copied to the GPU.
struct Upload {
    id: TileId,
    data: Box<buildings::BuildingData>,
    gpu: BuildGpu,
    /// next buffer of `BuildGpu::targets` to fill, and how far into it
    part: usize,
    off: usize,
}
/// Bytes copied to the GPU per frame at most (plus one terrain tile or
/// atlas). Everything written sits in wgpu staging memory until the GPU has
/// consumed it, a few frames later; on cards without resizable BAR that is a
/// 246 MiB heap, and overflowing it slows every upload to a crawl.
const UPLOAD_BUDGET: usize = 24 << 20;
/// Largest single write into a building buffer.
const UPLOAD_CHUNK: usize = 8 << 20;

pub struct Ground {
    pub h: f64,
    pub water: bool,
}

/// A parked car the player can take: its pose on the planet (not tile-relative).
#[derive(Copy, Clone, Debug)]
#[allow(dead_code)]
pub struct ParkedCar {
    pub model: u8,
    pub colour: [f32; 3],
    pub pos: DVec3,
    /// Bearing: radians from north, clockwise.
    pub yaw: f64,
}

/// A car that has been taken: the pose (and the speed it was doing) the player's
/// own car starts from.
#[derive(Copy, Clone, Debug)]
pub struct TakenCar {
    pub model: u8,
    pub colour: [f32; 3],
    pub pos: DVec3,
    /// Bearing: radians from north, clockwise.
    pub yaw: f64,
    /// Velocity in the planet frame (m/s); zero for a parked car.
    pub velocity: DVec3,
}

/// A parked airport aircraft the player can take: where it stands on the
/// planet, and which stand and traffic type it is.
#[derive(Copy, Clone, Debug)]
pub struct ParkedAircraft {
    /// Airfield index, as `Traffic`'s field is keyed.
    pub apt: u32,
    pub stand: usize,
    /// Traffic type (0 light single, 1 business jet, larger scenery).
    pub ty: u8,
    /// Ground-contact origin in the planet frame (between the main wheels).
    pub pos: DVec3,
    /// Bearing: radians from north, clockwise.
    pub yaw: f64,
    /// Ground slope along the nose, radians.
    pub pitch: f64,
}

/// Whether a [`TrafficBody`] is moving in its lane or parked at the kerb.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TrafficKind {
    Moving,
    Parked,
}

/// One car as the player's physics sees it: an oriented box in the planet frame.
/// The `tile` and `index` identify the source car, so a hit can be applied back
/// to its lane state (`World::hit_traffic`).
#[derive(Copy, Clone, Debug)]
#[allow(dead_code)]
pub struct TrafficBody {
    pub kind: TrafficKind,
    pub tile: TileId,
    pub index: usize,
    pub model: u8,
    pub colour: [f32; 3],
    /// Ground-contact centre in the planet frame.
    pub pos: DVec3,
    /// Bearing: radians from north, clockwise.
    pub yaw: f64,
    /// Half width and half length in the car's own frame (m).
    pub half: [f64; 2],
    /// Velocity in the planet frame (m/s).
    pub velocity: DVec3,
}

/// Where the player is, so that traffic can brake for them.
#[derive(Copy, Clone, Debug)]
pub struct PlayerPresence {
    pub pos: DVec3,
    /// Walking (traffic brakes inside 12 m) or driving (18 m).
    pub driving: bool,
}

impl PlayerPresence {
    /// Traffic brakes smoothly inside this distance of someone on foot (m).
    pub const WALK_RADIUS: f64 = 12.0;
    /// …and of a car being driven (m).
    pub const DRIVE_RADIUS: f64 = 18.0;

    pub fn radius(&self) -> f64 {
        if self.driving {
            Self::DRIVE_RADIUS
        } else {
            Self::WALK_RADIUS
        }
    }
}

/// Index of the nearest parked car in one tile's list to `p`, within `radius`.
fn nearest_parked(center: DVec3, parked: &[Parked], p: DVec3, radius: f64) -> Option<usize> {
    let mut best: Option<(f64, usize)> = None;
    for (i, pk) in parked.iter().enumerate() {
        let d = (center + (pk.pos + pk.shift).as_dvec3() - p).length();
        if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, i));
        }
    }
    best.map(|(_, i)| i)
}

/// A direction in the planet frame, as a bearing: radians from north, clockwise.
fn bearing_of(pos: DVec3, fwd: Vec3) -> f64 {
    let (e, n, _) = geo::enu(pos);
    let f = fwd.as_dvec3();
    f.dot(e).atan2(f.dot(n))
}

/// A building or bridge deck over a point.
pub struct BHit {
    pub base: f64,
    pub top: f64,
    pub bridge: bool,
    pub tile: TileId,
    pub idx: usize,
    /// footprint area (m²)
    pub area: f32,
}

/// A standing tree, as the physics sees it.
#[derive(Clone)]
pub struct TreeRef {
    pub tile: TileId,
    pub idx: usize,
    pub pos: DVec3,
    pub height: f64,
    pub crown_r: f64,
    pub crown_base: f64,
    pub trunk_r: f64,
    pub variant: u8,
    pub inst: TreeInst,
}

/// What is left to animate when a building is knocked down.
pub struct Wreck {
    pub ring: Vec<DVec3>,
    pub base: f64,
    pub top: f64,
    pub col: [u8; 3],
    /// how its walls were drawn (see `Footprint`)
    pub sty: [u8; 4],
    pub gref: f64,
    pub roof_col: [u8; 3],
    pub tiers: Vec<(f32, f32)>,
}

/// Shape of a crater as a function of distance / radius: height change in
/// units of the crater depth. Must match `crater_field` in the shaders.
/// The block of grid cells (x0, y0, w, h) of tile `id` that is drawn
/// tessellated: everything close enough to the camera for an edge to be split
/// at least once (terrain.wgsl: closer than cell / (2 TESS_K)), plus a margin.
/// Empty when the camera is too high or too far away.
fn tess_block(id: TileId, lat: f64, lon: f64, agl: f64, cell: f64) -> [f32; 4] {
    let r = cell / (2.0 * TESS_K) + 1.5 * cell;
    if agl >= r || agl.is_nan() {
        return [0.0; 4];
    }
    let rh = (r * r - agl.max(0.0).powi(2)).sqrt() / cell;
    let (u, v) = id.frac(lat, lon);
    let (cx, cy) = (u * GRID as f64, v * GRID as f64);
    let span = |c: f64| ((c - rh).floor().clamp(0.0, GRID as f64), (c + rh).ceil().clamp(0.0, GRID as f64));
    let ((x0, x1), (y0, y1)) = (span(cx), span(cy));
    if x1 <= x0 || y1 <= y0 {
        return [0.0; 4];
    }
    [x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32]
}

/// terrain.wgsl TESS_K and TN: triangles are kept to about TESS_K of their
/// distance across; a tessellated cell splits into at most TN x TN.
const TESS_K: f64 = 0.03;
const TESS_N: u32 = 16;

/// Sorts the cells of a tile's tessellated block (`b`, from `tess_block`) by
/// the patch each is drawn with: `out[k]` gets `cell | k << 16` for an
/// n = 2^k patch (terrain.wgsl `vs_tess`). The shader picks a level for each
/// edge and for the inside of a cell from their midpoints' distance. An
/// n x n patch whose vertices stand for every (TN / n)-th of the full
/// TN x TN patch's makes exactly the same triangles, stitching included, when
/// all five levels are n, or when n is at least twice the highest of them
/// (fewer, and the stitching or the corners come out differently). The levels
/// are worked out here as the shader does, with a margin for rounding: where
/// any is in doubt the larger patch is taken, which costs time but changes
/// nothing on screen. `p`: the grid vertices, `o`: the tile's offset from the
/// camera and `cell` its grid spacing, as in the tile's uniform.
fn tess_cells(p: &[[f32; 3]], o: Vec3, cell: f32, b: [f32; 4], fr: Option<&Frustum>, out: &mut [Vec<u32>; 5]) {
    let w = cell as f64;
    // terrain.wgsl `tess_level` at distance d.
    let level = |d: f64| {
        let sp = (d * TESS_K).max(w / TESS_N as f64);
        ((w / sp).max(1.0).log2().floor().exp2() as u32).clamp(1, TESS_N)
    };
    let at = |i: usize| Vec3::from(p[i]) + o;
    let (x0, y0) = (b[0] as usize, b[1] as usize);
    for cj in y0..(y0 + b[3] as usize).min(GRID) {
        for ci in x0..(x0 + b[2] as usize).min(GRID) {
            let i00 = cj * GV + ci;
            let (c00, c10, c01, c11) = (at(i00), at(i00 + 1), at(i00 + GV), at(i00 + GV + 1));
            // Skip cells completely outside the view frustum (pad radius for relief, waves and craters).
            if let Some(fr) = fr {
                let mid = (c00 + c10 + c01 + c11) * 0.25;
                let r2 = (c00 - mid).length_squared().max((c10 - mid).length_squared()).max((c01 - mid).length_squared()).max((c11 - mid).length_squared());
                if !fr.sphere(mid.as_dvec3(), r2.sqrt() as f64 + 20.0) {
                    continue;
                }
            }
            // The edges' midpoints, then the inside's (the shader's order of operations).
            let mids = [(c00 + c10) * 0.5, (c01 + c11) * 0.5, (c00 + c01) * 0.5, (c10 + c11) * 0.5, (c00 + c11) * 0.5];
            let (mut lo, mut hi) = (TESS_N, 1);
            for m in mids {
                let d = m.as_dvec3().length();
                lo = lo.min(level(d * 1.001));
                hi = hi.max(level(d * 0.999));
            }
            let n = if lo == hi { hi } else { (2 * hi).min(TESS_N) };
            let k = n.trailing_zeros();
            out[k as usize].push((cj * GRID + ci) as u32 | k << 16);
        }
    }
}

pub fn crater_profile(d: f64) -> f64 {
    let k = (d - 1.1) / 0.24;
    let rim = 0.22 * (-k * k).exp();
    if d < 1.0 { rim - (1.0 - d * d).powi(2) } else { rim }
}

/// Distance (beyond a tile's bounding sphere) within which its middle and
/// finest levels of detail are drawn. The middle level goes with the full
/// geometry it dresses, which gives way to the far stand-ins by BFAR[1].
/// Intensities of a car's head and tail lights where the 3D tiles hide our
/// cars (lights.wgsl units; an airliner's nav light is 40).
const CAR_HEAD: f32 = 60.0;
const CAR_TAIL: f32 = 12.0;

const LOD_MID: f32 = buildings::BFAR[1];
const LOD_FINE: f32 = 700.0;

pub struct World {
    /// flat ground everywhere tiles have not loaded, at this height (for tests)
    pub flat: Option<f64>,
    loader: Loader,
    tiles: HashMap<TileId, Slot>,
    builds: HashMap<TileId, BSlot>,
    frame: u64,
    in_flight: usize,
    pub max_zoom: u8,
    pub lod_k: f64,
    /// multiplies how far out building (and tree) tiles are loaded (graphics settings)
    pub reach_k: f64,
    /// graphics settings: multiplies the distances buildings change level of
    /// detail at (buildings.wgsl `city_k`)
    pub city_k: f32,
    /// terrain tiles kept loaded before the ones unused for a while go, and
    /// building tiles kept past the reach (the graphics settings: in a city
    /// each building tile is 50-170 MB of geometry, which an integrated GPU
    /// takes out of the system's memory)
    pub tile_cap: usize,
    pub spare_builds: usize,
    /// building tiles made (or waiting to go to the GPU) at once
    pub max_build_jobs: usize,
    /// graphics settings: how far out trees are drawn (m), and the factor on
    /// the distance they turn into impostors at (trees.wgsl `tree_far`, `tree_imp_k`)
    pub tree_far: f32,
    pub tree_imp_k: f32,
    /// graphics settings: share of the cars on the roads (0..1), how far off
    /// moving and parked cars are drawn (m)
    pub cars: (f32, f64, f64),
    pub buildings_on: bool,
    draw: Vec<TileId>,
    /// selected tiles out of view but near the camera: for the shore masks (shore.rs)
    draw_shore: Vec<TileId>,
    draw_b: Vec<TileId>,
    wanted: Vec<(f32, TileId)>,
    in_view: Vec<bool>,
    frustum: Option<Frustum>,
    /// (centre, radius) of the most recent craters
    pub craters: Vec<(DVec3, f32)>,
    pub tree_dims: Vec<TreeDim>,
    tick: u32,
    pub stats: (usize, usize, usize), // drawn tiles, loading, building tiles
    last_update: Option<std::time::Instant>,
    uploads: VecDeque<Upload>,
    fields: HashMap<u32, FSlot>,
    /// photorealistic 3D tiles near the aircraft (taken out while it updates)
    photo: Option<Box<crate::tiles3d::Photo>>,
    /// the waves (their CPU copy, so what floats rides what is drawn)
    pub ocean: crate::ocean::Ocean,
    /// the `--carjack` smoke run's synthetic traffic tile, kept from eviction
    test_traffic: Option<TileId>,
}

#[derive(Clone)]
pub struct Frustum([DVec4; 4]);
impl Frustum {
    /// From a camera-relative view-projection matrix.
    pub fn new(vp: DMat4) -> Self {
        let r = |i: usize| DVec4::new(vp.x_axis[i], vp.y_axis[i], vp.z_axis[i], vp.w_axis[i]);
        let norm = |p: DVec4| p / p.truncate().length();
        Self([norm(r(3) + r(0)), norm(r(3) - r(0)), norm(r(3) + r(1)), norm(r(3) - r(1))])
    }
    pub fn sphere(&self, c: DVec3, r: f64) -> bool {
        self.0.iter().all(|p| p.truncate().dot(c) + p.w > -r)
    }
}

impl World {
    pub fn new(loader: Loader, max_zoom: u8) -> Self {
        let photo = Some(Box::new(crate::tiles3d::Photo::new(loader.src.google.clone(), Some(loader.src.clone()))));
        Self { photo, flat: None, loader, tiles: HashMap::default(), builds: HashMap::default(), frame: 0, in_flight: 0, max_zoom, lod_k: 0.62, reach_k: 1.0, city_k: 1.0, tile_cap: 700, spare_builds: 12, max_build_jobs: 12, tree_far: 3200.0, tree_imp_k: 1.0, cars: (1.0, 2200.0, 1100.0), buildings_on: true, draw: Vec::new(), draw_shore: Vec::new(), draw_b: Vec::new(), wanted: Vec::new(), in_view: Vec::new(), frustum: None, craters: Vec::new(), tree_dims: Vec::new(), tick: 0, stats: (0, 0, 0), last_update: None, uploads: VecDeque::new(), fields: HashMap::default(), ocean: crate::ocean::Ocean::new(), test_traffic: None }
    }

    /// Tiles wanted but not yet asked for (at most 28 are on their way at once).
    pub fn queued(&self) -> usize {
        self.wanted.len()
    }

    /// Height of the ground (or water surface) at a lat/lon: the top of the
    /// photorealistic 3D mesh inside its zone (buildings included), our own
    /// terrain everywhere else.
    /// Whether a sphere (camera-relative centre, radius) is in this frame's view.
    pub fn in_view(&self, rel: DVec3, r: f64) -> bool {
        self.frustum.as_ref().map(|f| f.sphere(rel, r)).unwrap_or(true)
    }

    pub fn ground(&self, lat: f64, lon: f64) -> Option<Ground> {
        let g = self.terrain_ground(lat, lon);
        if let Some(mut h) = self.photo.as_deref().and_then(|p| p.ground(lat, lon, g.as_ref().map(|g| g.h))) {
            // Our water (the mesh's is a still photograph): ditching floats on it.
            let water = g.as_ref().is_some_and(|g| g.water && h < g.h + 1.5);
            if water {
                h = h.max(g.as_ref().map_or(h, |g| g.h));
            }
            return Some(Ground { h, water });
        }
        g
    }

    /// Outward unit surface normal of the ground at `(lat, lon)` (radians),
    /// estimated from nearby height samples; falls back to local up on flat
    /// ground, water, or sudden vertical cliffs.
    pub fn ground_normal(&self, lat: f64, lon: f64) -> DVec3 {
        let p0 = geo::lla_to_pos(lat, lon, 0.0);
        let (east, north, up) = geo::enu(p0);
        let Some(g0) = self.ground(lat, lon) else { return up };
        if g0.water {
            return up;
        }
        let eps = 1.0;
        let dlat = eps / geo::R;
        let dlon = eps / (geo::R * lat.cos().max(0.05));
        let slope = |a: Option<Ground>, b: Option<Ground>| -> f64 {
            let fwd = a.map(|g| g.h - g0.h).filter(|d| d.abs() <= 3.0);
            let bwd = b.map(|g| g0.h - g.h).filter(|d| d.abs() <= 3.0);
            match (fwd, bwd) {
                (Some(f), Some(b)) => 0.5 * (f + b) / eps,
                (Some(f), None) => f / eps,
                (None, Some(b)) => b / eps,
                (None, None) => 0.0,
            }
            .clamp(-2.0, 2.0)
        };
        let dh_de = slope(self.ground(lat, lon + dlon), self.ground(lat, lon - dlon));
        let dh_dn = slope(self.ground(lat + dlat, lon), self.ground(lat - dlat, lon));
        if dh_de == 0.0 && dh_dn == 0.0 {
            return up;
        }
        (up - east * dh_de - north * dh_dn).normalize()
    }

    /// Obstruction lights on the 3D tiles' tall buildings, if they are on.
    pub fn photo_beacons(&self, cam: DVec3, out: &mut Vec<crate::airfield::Light>) {
        if let Some(p) = self.photo.as_deref() {
            p.beacons(cam, out);
        }
    }

    /// Head and tail lights of one of our moving cars (number `i` of tile
    /// `b`, at `pos`, `dir` its heading) where the 3D tiles hide the car
    /// itself, its own lamps with it: sprites on the mesh's street, so the
    /// photographed city has traffic after dark. `streets`: the mesh's tiles
    /// round `b`, gathered when a car first needs them.
    #[allow(clippy::too_many_arguments)]
    fn photo_car_light<'a>(&self, photo: &'a crate::tiles3d::Photo, streets: &mut Option<Option<crate::tiles3d::Streets<'a>>>, b: &BTile, i: usize, c: &Car, pos: DVec3, dir: Vec3, cam: DVec3, out: &mut Vec<crate::airfield::Light>) {
        use crate::airfield::Light;
        // Cars are drawn out to `cars.1` (2.2 km at High); their lights fade
        // out before that, so the traffic has no edge.
        let car_far = self.cars.1;
        let fade = (car_far * 0.27).max(1.0);
        let d = (pos - cam).length();
        // Room left for the aircraft and the obstruction lights.
        if out.len() + 4 > crate::render::MAX_DYN_LIGHTS - 1024 || !self.frustum.as_ref().map(|f| f.sphere(pos - cam, 8.0)).unwrap_or(true) {
            return;
        }
        let road = pos.length() - geo::R;
        // The street under a car changes slowly: it is looked up every 8th
        // frame (the cars in turn), or when the car comes into view.
        let frame = self.frame as u32;
        let (dh, at) = c.street.get();
        let street = if at != u32::MAX && frame.wrapping_sub(at) < 8 && (frame as usize + i) % 8 != 0 {
            road + dh as f64
        } else {
            let Some(st) = streets.get_or_insert_with(|| photo.streets(b.center, b.radius + 20.0)).as_ref().and_then(|s| s.at(pos, road)) else {
                c.street.set((0.0, u32::MAX));
                return;
            };
            c.street.set(((st - road) as f32, frame));
            st
        };
        let k = ((car_far - d) / fade).clamp(0.0, 1.0) as f32;
        let n = pos.normalize();
        let base = (n * (geo::R + street) - cam).as_vec3();
        let u = n.as_vec3();
        let f = (dir - u * dir.dot(u)).normalize_or_zero();
        let r = f.cross(u);
        let len = cars::CAR_LEN[c.model as usize];
        let half_w = if len > 7.0 { 1.0 } else { 0.62 };
        // Seen from above or ahead (headlights) or behind (tail lights), not
        // from the other end. Far off the pair is one light: one sprite each
        // end, twice as bright.
        let mut lamp = |along: f32, lift: f32, inten: f32, col: [u8; 3], aim: Vec3| {
            let l = |side: f32, i: f32| Light {
                pos: (base + f * along + r * side + u * lift).into(),
                inten: i * k,
                dir: (aim - u * 0.15).normalize().into(),
                spread: -0.4,
                col: [col[0], col[1], col[2], 0],
                fx: [0, 0, 255, 0],
            };
            if d > 500.0 {
                out.push(l(0.0, inten * 2.0));
            } else {
                out.push(l(-half_w, inten));
                out.push(l(half_w, inten));
            }
        };
        lamp(len * 0.5 - 0.1, 0.7, CAR_HEAD, crate::airfield::L_WHITE, f);
        lamp(-len * 0.5 + 0.1, 0.85, CAR_TAIL, [255, 20, 12], -f);
    }

    /// The photorealistic 3D tiles.
    pub fn photo(&self) -> &crate::tiles3d::Photo {
        self.photo.as_deref().expect("photo tiles")
    }
    pub fn photo_mut(&mut self) -> &mut crate::tiles3d::Photo {
        self.photo.as_deref_mut().expect("photo tiles")
    }

    /// Under the 3D tiles, the building whose mesh is at a lat/lon (`top`: the
    /// mesh's height there): the one of our own footprints it stands on, or
    /// the nearest within a few metres (the mesh and the map rarely agree to
    /// the metre). None where the mesh is only ground.
    pub fn photo_building(&self, lat: f64, lon: f64, top: f64) -> Option<BHit> {
        if !self.hidden(lat, lon) {
            return None;
        }
        let g = self.terrain_ground(lat, lon)?;
        if top - g.h < 2.5 {
            return None;
        }
        let p = geo::lla_to_pos(lat, lon, g.h);
        let (e, n, _) = geo::enu(p);
        let b = std::iter::once((0.0, 0.0))
            .chain([1.5f64, 3.0, 5.0].iter().flat_map(|&r| (0..8).map(move |k| {
                let a = k as f64 * std::f64::consts::FRAC_PI_4;
                (r * a.cos(), r * a.sin())
            })))
            .find_map(|(x, y)| {
                let (la, lo, _) = geo::pos_to_lla(p + e * x + n * y);
                self.footprint_at(la, lo).filter(|b| !b.bridge)
            })?;
        Some(BHit { top, ..b })
    }

    /// The outlines of a destroyed building's parts (our frame, at the base),
    /// grown by up to `grow` metres, less where another building stands close
    /// by: what is cut out of the 3D tiles' mesh.
    pub fn cut_rings(&self, parts: &[Wreck], grow: f64) -> Vec<Vec<DVec3>> {
        let free = |p: DVec3| {
            let (la, lo, _) = geo::pos_to_lla(p);
            self.footprint_at(la, lo).is_none()
        };
        parts
            .iter()
            .map(|w| {
                let n = w.ring.len();
                if n < 3 {
                    return w.ring.clone();
                }
                let c = w.ring.iter().fold(DVec3::ZERO, |a, p| a + *p) / n as f64;
                let (e, nn, _) = geo::enu(c);
                let r2: Vec<glam::DVec2> = w.ring.iter().map(|p| glam::DVec2::new((*p - c).dot(e), (*p - c).dot(nn))).collect();
                let area: f64 = (0..n).map(|i| r2[i].perp_dot(r2[(i + 1) % n])).sum::<f64>();
                let sign = area.signum();
                (0..n)
                    .map(|i| {
                        let (p, q, v) = (r2[(i + n - 1) % n], r2[(i + 1) % n], r2[i]);
                        let (e0, e1) = ((v - p).normalize_or_zero(), (q - v).normalize_or_zero());
                        // Outward, mitred (but not into long spikes at sharp corners).
                        let (n0, n1) = (-e0.perp() * sign, -e1.perp() * sign);
                        let m = n0 + n1;
                        let k = m.length_squared();
                        let off = if k > 0.15 { m * (2.0 / k) } else { (n0 + n1).normalize_or_zero() };
                        let off = if off.length() > 2.5 { off.normalize() * 2.5 } else { off };
                        let at = |d: f64| c + e * (v.x + off.x * d) + nn * (v.y + off.y * d);
                        let d = [grow, grow * 0.6, grow * 0.3].into_iter().find(|&d| free(at(d)) && free(at(d * 0.5))).unwrap_or(0.15);
                        at(d)
                    })
                    .collect()
            })
            .collect()
    }

    /// Whether our own scenery at lat/lon is hidden under the 3D mesh (and so
    /// is not there for the physics either).
    fn hidden(&self, lat: f64, lon: f64) -> bool {
        self.photo.as_deref().is_some_and(|p| p.hides(lat, lon))
    }

    /// Streams the 3D tiles for this frame (see tiles3d.rs for the arguments).
    #[allow(clippy::too_many_arguments)]
    pub fn update_photo(&mut self, r: &Renderer, cam: DVec3, focus: DVec3, fr: &Frustum, px: f64, day: f64, dt: f64) {
        let mut p = self.photo.take().expect("photo tiles");
        let fine = 13.min(self.max_zoom);
        // (a sample is only good for the datum on fine terrain, and not on a
        // building's roof: downtown most of the mesh is roofs and walls)
        p.update(r, cam, focus, fr, px, day, dt, &|la, lo| self.terrain_ground(la, lo).map(|g| (g.h, self.ground_zoom(la, lo).is_some_and(|z| z >= fine) && self.footprint_at(la, lo).is_none())));
        self.photo = Some(p);
    }

    /// How the 3D tiles blend in this frame (see tiles3d::PhotoFrame).
    pub fn photo_frame(&mut self, cam: DVec3) -> crate::tiles3d::PhotoFrame {
        self.photo.as_deref_mut().expect("photo tiles").frame(cam)
    }

    /// The 3D tiles to draw, with their offsets from the camera.
    pub fn photo_draws(&self, cam: DVec3) -> Vec<(&crate::render::PhotoGpu, [[f32; 4]; 2])> {
        self.photo().draws(cam)
    }

    /// Height of our own terrain surface (or water surface) and its gameplay
    /// class at a lat/lon, from the most detailed tile currently loaded there.
    /// Shared by `terrain_ground` and `surface_at`, so height and class always
    /// come from the same tile.
    fn terrain_sample(&self, lat: f64, lon: f64) -> Option<(Ground, u8)> {
        let (mx, my) = (geo::lon_to_merc_x(lon).rem_euclid(1.0), geo::lat_to_merc_y(lat));
        for z in (2..=self.max_zoom).rev() {
            let id = TileId::at_merc(mx, my, z);
            if let Some(Slot::Ready(t)) = self.tiles.get(&id) {
                let n = (1u64 << z) as f64;
                let (u, v) = (mx * n - id.x as f64, my * n - id.y as f64);
                let (x, y) = ((u * GRID as f64).clamp(0.0, GRID as f64 - 1e-6), (v * GRID as f64).clamp(0.0, GRID as f64 - 1e-6));
                let (x0, y0) = (x as usize, y as usize);
                let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32);
                let g = |i: usize, j: usize| t.heights[j * GV + i];
                let h = (g(x0, y0) * (1.0 - fx) + g(x0 + 1, y0) * fx) * (1.0 - fy) + (g(x0, y0 + 1) * (1.0 - fx) + g(x0 + 1, y0 + 1) * fx) * fy;
                let w = t.water[(y + 0.5) as usize * GV + (x + 0.5) as usize];
                let mut h = h as f64;
                // Water rides the ocean's long waves, as much as it shows them.
                let s = |i: usize, j: usize| t.sea[j * GV + i];
                let sea = (s(x0, y0) * (1.0 - fx) + s(x0 + 1, y0) * fx) * (1.0 - fy) + (s(x0, y0 + 1) * (1.0 - fx) + s(x0 + 1, y0 + 1) * fx) * fy;
                if w && sea > 0.0 {
                    h += sea as f64 * self.ocean.height(geo::lla_to_pos(lat, lon, h));
                }
                if !self.craters.is_empty() {
                    let p = geo::lla_to_pos(lat, lon, h);
                    for (c, r) in &self.craters {
                        let v = p - *c;
                        if v.length_squared() < (*r as f64 * 2.0).powi(2) {
                            let up = c.normalize();
                            let d = (v - up * v.dot(up)).length() / *r as f64;
                            if d < 1.8 {
                                h += crater_profile(d) * *r as f64 * 0.34;
                            }
                        }
                    }
                }
                // Nearest mask cell, as the plan specifies: the class is a label,
                // not a field to interpolate.
                let (sx, sy) = (((x / GRID as f64) * MASK as f64) as usize, ((y / GRID as f64) * MASK as f64) as usize);
                let surface = t.surface.get(sy.min(MASK - 1) * MASK + sx.min(MASK - 1)).copied().unwrap_or(Surface::Grass as u8);
                return Some((Ground { h, water: w }, surface));
            }
        }
        let mut h = self.flat?;
        if !self.craters.is_empty() {
            let p = geo::lla_to_pos(lat, lon, h);
            for (c, r) in &self.craters {
                let v = p - *c;
                if v.length_squared() < (*r as f64 * 2.0).powi(2) {
                    let up = c.normalize();
                    let d = (v - up * v.dot(up)).length() / *r as f64;
                    if d < 1.8 {
                        h += crater_profile(d) * *r as f64 * 0.34;
                    }
                }
            }
        }
        Some((Ground { h, water: false }, Surface::Grass as u8))
    }

    /// Height of our own terrain surface (or water surface) at a lat/lon.
    pub fn terrain_ground(&self, lat: f64, lon: f64) -> Option<Ground> {
        self.terrain_sample(lat, lon).map(|(g, _)| g)
    }

    /// The road index hit under a lat/lon, from the loaded detail tile.
    #[allow(dead_code)]
    pub fn road_hit(&self, lat: f64, lon: f64) -> Option<RoadHit> {
        let id = TileId::at(lat, lon, DATA_ZOOM);
        let Some(BSlot::Ready(b)) = self.builds.get(&id) else { return None };
        let (u, v) = id.frac(lat, lon);
        let ext = b.road_index.ext();
        b.road_index.query(&b.net, [(u * ext as f64) as f32, (v * ext as f64) as f32])
    }

    /// The road index hit under a lat/lon for a body at `alt`: the ribbon at
    /// the body's own level where several roads overlap in plan (a deck over a
    /// street, a switchback), so a car under a bridge finds the street below
    /// it and a car on the deck stays on it.
    fn road_hit_at(&self, lat: f64, lon: f64, alt: f64) -> Option<RoadHit> {
        let id = TileId::at(lat, lon, DATA_ZOOM);
        let Some(BSlot::Ready(b)) = self.builds.get(&id) else { return None };
        let (u, v) = id.frac(lat, lon);
        let ext = b.road_index.ext();
        b.road_index.query_for_body(&b.net, [(u * ext as f64) as f32, (v * ext as f64) as f32], alt)
    }

    /// The shared surface query: terrain height and class, refined by the road
    /// index. `body` is the querying body's altitude: a bridge deck more than
    /// 2.5 m above it is ignored, so a body under the bridge stays on the
    /// terrain, and an at-grade street a little above it still counts, so a
    /// body that has sunk under a ribbon climbs back out. `None` means the
    /// tile selection had no ground.
    #[allow(dead_code)]
    fn surface_hit(&self, lat: f64, lon: f64, body: Option<f64>) -> Option<SurfaceHit> {
        let (g, class) = self.terrain_sample(lat, lon)?;
        let kind = Surface::from_u8(class);
        let mut hit = SurfaceHit { h: g.h, kind, water: g.water || kind == Surface::Water, road: false, sidewalk: false, bridge: false };
        let road = match body {
            Some(alt) => self.road_hit_at(lat, lon, alt),
            None => self.road_hit(lat, lon),
        };
        if let Some(r) = road {
            if !r.hidden {
                hit.kind = r.kind;
                hit.h = r.h;
                hit.road = true;
                hit.sidewalk = r.sidewalk;
                hit.bridge = r.bridge;
                hit.water = r.kind == Surface::Water;
                // Where the photorealistic mesh is drawn over our road, its
                // street is what the player sees: stand on that rather than on
                // the ribbon hidden under it. The mesh surface is looked up
                // near the road's own height, so roofs, trees and decks over
                // the street are passed by; no mesh street near that level
                // leaves the ribbon in charge.
                if let Some(h) = self.photo.as_deref().and_then(|p| p.street(lat, lon, r.h, r.bridge)) {
                    hit.h = h;
                }
            }
        }
        Some(hit)
    }

    /// The surface under a world position: the gameplay query. It applies the
    /// bridge-height filter against the body's own altitude.
    #[allow(dead_code)]
    pub fn surface_at(&self, p: DVec3) -> Option<SurfaceHit> {
        let (lat, lon, alt) = geo::pos_to_lla(p);
        self.surface_hit(lat, lon, Some(alt))
    }

    /// Diagnostic surface query at a lat/lon, without the bridge-height filter.
    #[allow(dead_code)]
    pub fn surface(&self, lat: f64, lon: f64) -> Option<SurfaceHit> {
        self.surface_hit(lat, lon, None)
    }

    /// The nearest parked car within `radius` of `p`, if any.
    pub fn nearest_parked_car(&self, p: DVec3, radius: f64) -> Option<ParkedCar> {
        let (lat, lon, _) = geo::pos_to_lla(p);
        let c = TileId::at(lat, lon, DATA_ZOOM);
        let size = 1i64 << DATA_ZOOM;
        let mut best: Option<(f64, ParkedCar)> = None;
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                let (x, y) = ((c.x as i64 + dx).rem_euclid(size), c.y as i64 + dy);
                if y < 0 || y >= size {
                    continue;
                }
                let id = TileId::new(DATA_ZOOM, x as u32, y as u32);
                let Some(BSlot::Ready(b)) = self.builds.get(&id) else { continue };
                let Some(i) = nearest_parked(b.center, &b.parked, p, radius) else { continue };
                let pk = &b.parked[i];
                let pos = b.center + (pk.pos + pk.shift).as_dvec3();
                let d = (pos - p).length();
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, ParkedCar { model: pk.model, colour: pk.col, pos, yaw: bearing_of(pos, pk.fwd) }));
                }
            }
        }
        best.map(|(_, car)| car)
    }

    /// Take the nearest parked car within `radius` of `p`, removing it from its
    /// tile so it cannot be taken twice.
    pub fn take_parked_car(&mut self, p: DVec3, radius: f64) -> Option<TakenCar> {
        let (lat, lon, _) = geo::pos_to_lla(p);
        let c = TileId::at(lat, lon, DATA_ZOOM);
        let size = 1i64 << DATA_ZOOM;
        let mut best: Option<(f64, TileId, usize, TakenCar)> = None;
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                let (x, y) = ((c.x as i64 + dx).rem_euclid(size), c.y as i64 + dy);
                if y < 0 || y >= size {
                    continue;
                }
                let id = TileId::new(DATA_ZOOM, x as u32, y as u32);
                let Some(BSlot::Ready(b)) = self.builds.get(&id) else { continue };
                let Some(i) = nearest_parked(b.center, &b.parked, p, radius) else { continue };
                let pk = &b.parked[i];
                let pos = b.center + (pk.pos + pk.shift).as_dvec3();
                let d = (pos - p).length();
                if best.as_ref().is_none_or(|(bd, _, _, _)| d < *bd) {
                    best = Some((d, id, i, TakenCar { model: pk.model, colour: pk.col, pos, yaw: bearing_of(pos, pk.fwd), velocity: pk.bump.as_dvec3() }));
                }
            }
        }
        let (_, id, i, car) = best?;
        if let Some(BSlot::Ready(b)) = self.builds.get_mut(&id) {
            if i < b.parked.len() {
                b.parked.swap_remove(i);
            }
        }
        Some(car)
    }

    /// The nearest moving traffic car within `radius` of `p`, without taking it:
    /// for the interaction prompt.
    pub fn nearest_traffic_car(&self, p: DVec3, radius: f64) -> Option<TakenCar> {
        self.pick_traffic_car(p, radius).map(|(_, _, _, car)| car)
    }

    /// Take the nearest moving traffic car within `radius` of `p`, removing it
    /// from its lane so it can neither be taken twice nor drawn again. Cars in
    /// tiles that are not ready are refused: the interaction is offered only for
    /// a car the simulation actually has.
    pub fn take_traffic_car(&mut self, p: DVec3, radius: f64) -> Option<TakenCar> {
        let (_, id, i, car) = self.pick_traffic_car(p, radius)?;
        let removed = match self.builds.get_mut(&id) {
            Some(BSlot::Ready(b)) if i < b.cars.len() && b.cars[i].model == car.model => {
                b.cars.swap_remove(i);
                true
            }
            _ => false,
        };
        removed.then_some(car)
    }

    /// Find the nearest moving car in the ready tiles around `p`.
    fn pick_traffic_car(&self, p: DVec3, radius: f64) -> Option<(f64, TileId, usize, TakenCar)> {
        if !p.is_finite() || !(radius > 0.0) {
            return None;
        }
        let (lat, lon, _) = geo::pos_to_lla(p);
        let c = TileId::at(lat, lon, DATA_ZOOM);
        let size = 1i64 << DATA_ZOOM;
        let mut best: Option<(f64, TileId, usize, TakenCar)> = None;
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                let (x, y) = ((c.x as i64 + dx).rem_euclid(size), c.y as i64 + dy);
                if y < 0 || y >= size {
                    continue;
                }
                let id = TileId::new(DATA_ZOOM, x as u32, y as u32);
                let Some(BSlot::Ready(b)) = self.builds.get(&id) else { continue };
                let up = b.center.normalize().as_vec3();
                for (i, car) in b.cars.iter().enumerate() {
                    if b.net.paths.get(car.path as usize).is_none_or(|path| path.hidden) {
                        continue;
                    }
                    let (rel, dir) = cars::pose(&b.net, car, up);
                    let pos = b.center + (rel + car.shift).as_dvec3();
                    let d = (pos - p).length();
                    if d > radius || best.as_ref().is_some_and(|(bd, _, _, _)| *bd <= d) {
                        continue;
                    }
                    let velocity = dir.as_dvec3() * car.v as f64 + car.bump.as_dvec3();
                    best = Some((d, id, i, TakenCar { model: car.model, colour: car.col, pos, yaw: bearing_of(pos, dir), velocity }));
                }
            }
        }
        best
    }

    /// Oriented boxes for the moving and parked cars within `radius` of `p`.
    /// Only cars in ready tiles are returned; tunnels stay out of sight.
    pub fn traffic_bodies_near(&self, p: DVec3, radius: f64, out: &mut Vec<TrafficBody>) {
        out.clear();
        if !p.is_finite() || !(radius > 0.0) {
            return;
        }
        // With the cars setting off none are drawn: none may be solid either,
        // or the player would be stopped by traffic it cannot see.
        if self.cars.0 <= 0.0 {
            return;
        }
        let (lat, lon, _) = geo::pos_to_lla(p);
        let c = TileId::at(lat, lon, DATA_ZOOM);
        let size = 1i64 << DATA_ZOOM;
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                let (x, y) = ((c.x as i64 + dx).rem_euclid(size), c.y as i64 + dy);
                if y < 0 || y >= size {
                    continue;
                }
                let id = TileId::new(DATA_ZOOM, x as u32, y as u32);
                let Some(BSlot::Ready(b)) = self.builds.get(&id) else { continue };
                if (b.center - p).length() > radius + b.radius {
                    continue;
                }
                let up = b.center.normalize().as_vec3();
                for (i, car) in b.cars.iter().enumerate() {
                    if b.net.paths.get(car.path as usize).is_none_or(|path| path.hidden) {
                        continue;
                    }
                    let (rel, dir) = cars::pose(&b.net, car, up);
                    let pos = b.center + (rel + car.shift).as_dvec3();
                    let half = cars::player_geom(car.model).half;
                    if (pos - p).length() > radius + half[1] {
                        continue;
                    }
                    let velocity = dir.as_dvec3() * car.v as f64 + car.bump.as_dvec3();
                    out.push(TrafficBody { kind: TrafficKind::Moving, tile: id, index: i, model: car.model, colour: car.col, pos, yaw: bearing_of(pos, dir), half, velocity });
                }
                for (i, pk) in b.parked.iter().enumerate() {
                    let pos = b.center + (pk.pos + pk.shift).as_dvec3();
                    let half = cars::player_geom(pk.model).half;
                    if (pos - p).length() > radius + half[1] {
                        continue;
                    }
                    out.push(TrafficBody { kind: TrafficKind::Parked, tile: id, index: i, model: pk.model, colour: pk.col, pos, yaw: bearing_of(pos, pk.fwd), half, velocity: pk.bump.as_dvec3() });
                }
            }
        }
    }

    /// Release the `--carjack` tile, so the world streams over it as usual.
    pub fn clear_test_traffic(&mut self) {
        self.test_traffic = None;
    }

    /// A player car hit another car: give it the impulse the player's physics
    /// worked out, so the other car is shoved away too. Moving cars settle back
    /// onto their lane; a parked car stays where the hit left it.
    pub fn hit_traffic(&mut self, body: &TrafficBody, impact: f64, shove: DVec3, velocity: DVec3) {
        if !(impact > 0.0) || !impact.is_finite() || !shove.is_finite() || !velocity.is_finite() {
            return;
        }
        let Some(BSlot::Ready(b)) = self.builds.get_mut(&body.tile) else { return };
        match body.kind {
            TrafficKind::Moving => {
                let Some(car) = b.cars.get_mut(body.index) else { return };
                if car.model != body.model {
                    return;
                }
                car.shift += shove.as_vec3();
                car.bump += velocity.as_vec3();
                // A hit from behind speeds it up; a head-on hit slows it. The
                // sideways part of the impulse lives in `shift`/`bump`.
                let (e, n, _) = geo::enu(body.pos);
                let fwd = e * body.yaw.sin() + n * body.yaw.cos();
                let along = velocity.dot(fwd) as f32;
                if along.is_finite() {
                    let path_top = b.net.paths.get(car.path as usize).map_or(0.0, |p| p.speed);
                    car.v = (car.v + along).clamp(0.0, (path_top.max(car.v) * 1.6).max(1.0));
                }
                car.hurt = (car.hurt + (impact * 0.04) as f32).clamp(0.0, 1.0);
            }
            TrafficKind::Parked => {
                let Some(pk) = b.parked.get_mut(body.index) else { return };
                if pk.model != body.model {
                    return;
                }
                pk.shift += shove.as_vec3();
                pk.bump += velocity.as_vec3();
            }
        }
    }

    /// Traffic braked because it hit the player on foot: hold it stopped for a
    /// few seconds.
    pub fn brake_traffic(&mut self, body: &TrafficBody) {
        let Some(car) = self.traffic_car_mut(body) else { return };
        car.v *= 0.3;
        car.yield_t = car.yield_t.max(3.0);
    }

    fn traffic_car_mut(&mut self, body: &TrafficBody) -> Option<&mut Car> {
        if body.kind != TrafficKind::Moving {
            return None;
        }
        let Some(BSlot::Ready(b)) = self.builds.get_mut(&body.tile) else { return None };
        let car = b.cars.get_mut(body.index)?;
        (car.model == body.model).then_some(car)
    }

    /// One moving car on a straight, empty road through `pos`, facing `yaw`, at
    /// `speed` m/s: the deterministic traffic the `--carjack` smoke run takes.
    /// It sits in an ordinary building tile, so it is stepped, drawn and taken
    /// exactly as streamed traffic is.
    pub fn spawn_test_traffic(&mut self, pos: DVec3, yaw: f64, speed: f32) -> bool {
        if !pos.is_finite() || !yaw.is_finite() {
            return false;
        }
        let (lat, lon, alt) = geo::pos_to_lla(pos);
        let id = TileId::at(lat, lon, DATA_ZOOM);
        let (clat, clon) = id.lat_lon(0.5, 0.5);
        let center = geo::lla_to_pos(clat, clon, alt);
        let (east, north, _) = geo::enu(center);
        let dir = east * yaw.sin() + north * yaw.cos();
        let right = dir.cross(center.normalize()).normalize_or_zero();
        // Offset the road so the car's lane runs through `pos`.
        let base = pos - right * 1.5;
        let length = 120.0f32;
        let half = length * 0.5;
        let a = base - dir * half as f64;
        let b = base + dir * half as f64;
        let pts = vec![(a - center).as_vec3(), (b - center).as_vec3()];
        let path = crate::roads::Path {
            pts: pts.clone(),
            cum: vec![0.0, length],
            len: length,
            class: 1,
            oneway: false,
            lanes: 1,
            lane_w: 3.0,
            park_off: 0.0,
            speed: speed.max(1.0),
            hidden: false,
            drive_half: 5.0,
            walk: false,
            bridge: false,
            local: Vec::new(),
            alt: vec![alt as f32, alt as f32],
            nodes: [0, 0],
            edge: [0, 0],
            junctions: Vec::new(),
            keep_clear: Vec::new(),
        };
        let net = RoadNet { paths: vec![path], ..Default::default() };
        let car = cars::placed(0, half + 2.8, speed.max(0.0), 0, [0.62, 0.08, 0.05]);
        let road_index = RoadIndex::build(&net, 1024.0, 1.0);
        let radius = (pos - center).length() + 400.0;
        let tile = BTile { center, radius, gpu: None, footprints: Vec::new(), last_used: self.frame, net, road_index, cars: vec![car], parked: Vec::new(), trees: Vec::new(), tree_var: Vec::new(), chunks: Vec::new(), under: Vec::new(), cells: Vec::new(), near_idx: Vec::new(), ax: [0.0; 4], ay: [0.0; 4], shown: 1.0, mid_t: 1.0, fine_t: 1.0, tall: 0.0 };
        self.builds.insert(id, BSlot::Ready(Box::new(tile)));
        self.test_traffic = Some(id);
        true
    }

    /// Building or bridge deck over a lat/lon.
    pub fn building(&self, lat: f64, lon: f64) -> Option<BHit> {
        if self.hidden(lat, lon) {
            return None;
        }
        self.footprint_at(lat, lon)
    }

    /// Our own building or bridge deck over a lat/lon, hidden under the 3D tiles or not.
    fn footprint_at(&self, lat: f64, lon: f64) -> Option<BHit> {
        let id = TileId::at(lat, lon, DATA_ZOOM);
        if let Some(BSlot::Ready(b)) = self.builds.get(&id) {
            let (u, v) = id.frac(lat, lon);
            let p = [u as f32, v as f32];
            for (i, f) in b.footprints.iter().enumerate() {
                if f.alive && p[0] >= f.min[0] && p[0] <= f.max[0] && p[1] >= f.min[1] && p[1] <= f.max[1] && buildings::point_in_ring(p, &f.ring) {
                    return Some(BHit { base: f.base as f64, top: f.top as f64, bridge: f.kind == 1, tile: id, idx: i, area: f.area });
                }
            }
        }
        None
    }

    /// Standing trees whose base is within `r` metres (horizontally) of a point.
    pub fn trees_near(&self, p: DVec3, r: f64, out: &mut Vec<TreeRef>) {
        let (lat, lon, _) = geo::pos_to_lla(p);
        if self.hidden(lat, lon) {
            return;
        }
        let c = TileId::at(lat, lon, DATA_ZOOM);
        let size = 1i64 << DATA_ZOOM;
        let up = p.normalize();
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                let (x, y) = ((c.x as i64 + dx).rem_euclid(size), c.y as i64 + dy);
                if y < 0 || y >= size {
                    continue;
                }
                let id = TileId::new(DATA_ZOOM, x as u32, y as u32);
                let Some(BSlot::Ready(b)) = self.builds.get(&id) else { continue };
                let lp = (p - b.center).as_vec3();
                for ch in &b.chunks {
                    if (ch.center - lp).length() > ch.radius + r as f32 + 40.0 {
                        continue;
                    }
                    for (_, first, count) in &ch.runs {
                        for i in *first as usize..(*first + *count) as usize {
                            let t = &b.trees[i];
                            if t.a[3] <= 0.0 {
                                continue;
                            }
                            let d = glam::Vec3::new(t.a[0], t.a[1], t.a[2]) - lp;
                            let dv = d.as_dvec3();
                            let horiz = dv - up * dv.dot(up);
                            if horiz.length_squared() > r * r {
                                continue;
                            }
                            let dim = self.tree_dims.get(b.tree_var[i] as usize).copied().unwrap_or_default();
                            let (hs, ws) = (t.a[3] as f64, (t.a[3] * t.b[3]) as f64);
                            out.push(TreeRef { tile: id, idx: i, pos: b.center + glam::Vec3::new(t.a[0], t.a[1], t.a[2]).as_dvec3(), height: dim.height as f64 * hs, crown_r: dim.crown_r as f64 * ws, crown_base: dim.crown_base as f64 * hs, trunk_r: dim.trunk_r as f64 * ws, variant: b.tree_var[i], inst: *t });
                        }
                    }
                }
            }
        }
    }

    /// Take a tree out of the standing forest (it has been knocked loose).
    pub fn remove_tree(&mut self, queue: &wgpu::Queue, t: &TreeRef) {
        if let Some(BSlot::Ready(b)) = self.builds.get_mut(&t.tile) {
            if let Some(inst) = b.trees.get_mut(t.idx) {
                inst.a[3] = 0.0;
                if let Some(buf) = b.gpu.as_ref().and_then(|g| g.trees.as_ref()) {
                    queue.write_buffer(buf, t.idx as u64 * 36 + 12, &[0u8; 4]);
                }
            }
        }
    }

    /// Remove a building from the world and hand back what is needed to wreck it.
    pub fn destroy_building(&mut self, queue: &wgpu::Queue, tile: TileId, idx: usize) -> Option<Wreck> {
        let Some(BSlot::Ready(b)) = self.builds.get_mut(&tile) else { return None };
        let f = b.footprints.get_mut(idx)?;
        if !f.alive || f.kind != 0 {
            return None;
        }
        f.alive = false;
        if let Some(g) = &b.gpu {
            for (start, n) in [f.main, f.det, f.fine, f.far] {
                if n > 0 {
                    queue.write_buffer(&g.ibuf, start as u64 * 4, &vec![0u8; n as usize * 4]);
                }
            }
        }
        let ring = f.ring.iter().map(|p| {
            let (lat, lon) = tile.lat_lon(p[0] as f64, p[1] as f64);
            geo::lla_to_pos(lat, lon, f.base as f64)
        }).collect();
        Some(Wreck { ring, base: f.base as f64, top: f.top as f64, col: f.col, sty: f.sty, gref: f.gref as f64, roof_col: f.roof_col, tiers: f.tiers.clone() })
    }

    /// The rest of the building a footprint belongs to, which comes down with
    /// it: parts standing on it (a tower on its podium, a spire on its tower)
    /// and, for a tall building, the parts beside it that rise about as high
    /// (towers are often mapped as several full-height pieces side by side).
    pub fn structure_of(&self, tile: TileId, idx: usize) -> Vec<(TileId, usize)> {
        let Some(BSlot::Ready(b0)) = self.builds.get(&tile) else { return Vec::new() };
        let Some(f0) = b0.footprints.get(idx) else { return Vec::new() };
        let org = b0.center + f0.center.as_dvec3();
        let (e, n, _) = geo::enu(org);
        let hit_h = f0.top - f0.base;
        let tall = hit_h >= 28.0;
        let size = 1i64 << DATA_ZOOM;
        // Every candidate nearby, with its outline in metres on a local grid.
        struct Cand {
            id: (TileId, usize),
            ring: Vec<glam::Vec2>,
            base: f32,
            top: f32,
        }
        let o0 = {
            let (la, lo, _) = geo::pos_to_lla(org);
            geo::lla_to_pos(la, lo, 0.0)
        };
        let to_local = |t: TileId, p: [f32; 2]| {
            let (la, lo) = t.lat_lon(p[0] as f64, p[1] as f64);
            let q = geo::lla_to_pos(la, lo, 0.0) - o0;
            glam::Vec2::new(q.dot(e) as f32, q.dot(n) as f32)
        };
        let mut cands: Vec<Cand> = Vec::new();
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                let (x, y) = ((tile.x as i64 + dx).rem_euclid(size), tile.y as i64 + dy);
                if y < 0 || y >= size {
                    continue;
                }
                let id = TileId::new(DATA_ZOOM, x as u32, y as u32);
                let Some(BSlot::Ready(b)) = self.builds.get(&id) else { continue };
                for (i, f) in b.footprints.iter().enumerate() {
                    if !f.alive || f.kind != 0 || f.ring.len() < 3 {
                        continue;
                    }
                    if (b.center + f.center.as_dvec3() - org).length() > 250.0 + f.area.sqrt() as f64 {
                        continue;
                    }
                    cands.push(Cand { id: (id, i), ring: f.ring.iter().map(|p| to_local(id, *p)).collect(), base: f.base, top: f.top });
                }
            }
        }
        let inside = |r: &[glam::Vec2], p: glam::Vec2| {
            let mut c = false;
            let mut j = r.len() - 1;
            for i in 0..r.len() {
                let (p0, p1) = (r[i], r[j]);
                if (p0.y > p.y) != (p1.y > p.y) && p.x < (p1.x - p0.x) * (p.y - p0.y) / (p1.y - p0.y) + p0.x {
                    c = !c;
                }
                j = i;
            }
            c
        };
        // Outlines that overlap or share a wall (within a metre and a half).
        let touch = |a: &[glam::Vec2], b: &[glam::Vec2]| {
            if a.iter().any(|p| inside(b, *p)) || b.iter().any(|p| inside(a, *p)) {
                return true;
            }
            let near = |p: glam::Vec2, r: &[glam::Vec2]| {
                (0..r.len()).any(|i| {
                    let (s0, s1) = (r[i], r[(i + 1) % r.len()]);
                    let d = s1 - s0;
                    let t = ((p - s0).dot(d) / d.length_squared().max(1e-6)).clamp(0.0, 1.0);
                    (s0 + d * t - p).length() < 1.5
                })
            };
            a.iter().any(|p| near(*p, b)) || b.iter().any(|p| near(*p, a))
        };
        let Some(start) = cands.iter().position(|c| c.id == (tile, idx)) else { return Vec::new() };
        let mut taken = vec![false; cands.len()];
        taken[start] = true;
        let mut todo = vec![start];
        let mut out = Vec::new();
        while let Some(k) = todo.pop() {
            if out.len() >= 40 {
                break;
            }
            for j in 0..cands.len() {
                if taken[j] {
                    continue;
                }
                let (cur, c) = (&cands[k], &cands[j]);
                let stacked = c.base >= cur.base + 0.5 && c.base <= cur.top + 3.0;
                let beside = tall && c.top - c.base >= 20.0 && c.top >= f0.top - hit_h * 0.5;
                if !(stacked || beside) || !touch(&cur.ring, &c.ring) {
                    continue;
                }
                taken[j] = true;
                out.push(c.id);
                todo.push(j);
            }
        }
        out
    }

    /// Footprints of small buildings whose centre is within `r` of a point.
    /// (Under the 3D tiles too: they are cut out of the mesh when they come down.)
    /// A standing building's base, top and footprint area.
    pub fn footprint(&self, tile: TileId, idx: usize) -> Option<(f64, f64, f32)> {
        let Some(BSlot::Ready(b)) = self.builds.get(&tile) else { return None };
        b.footprints.get(idx).filter(|f| f.alive).map(|f| (f.base as f64, f.top as f64, f.area))
    }

    pub fn buildings_near(&self, p: DVec3, r: f64, max_area: f32) -> Vec<(TileId, usize)> {
        let (lat, lon, _) = geo::pos_to_lla(p);
        let id = TileId::at(lat, lon, DATA_ZOOM);
        let mut out = Vec::new();
        if let Some(BSlot::Ready(b)) = self.builds.get(&id) {
            let lp = (p - b.center).as_vec3();
            for (i, f) in b.footprints.iter().enumerate() {
                if f.alive && f.kind == 0 && f.area < max_area && (f.center - lp).length() < r as f32 {
                    out.push((id, i));
                }
            }
        }
        out
    }

    /// Standing buildings and bridge decks near a point: outline (planet
    /// frame, at the base), base and top altitude, whether it is a bridge.
    pub fn footprints_near(&self, p: DVec3, r: f64) -> Vec<(Vec<DVec3>, f64, f64, bool)> {
        let (lat, lon, _) = geo::pos_to_lla(p);
        if self.hidden(lat, lon) {
            return Vec::new();
        }
        let c = TileId::at(lat, lon, DATA_ZOOM);
        let size = 1i64 << DATA_ZOOM;
        let mut out = Vec::new();
        for dy in -1..=1i64 {
            for dx in -1..=1i64 {
                let (x, y) = ((c.x as i64 + dx).rem_euclid(size), c.y as i64 + dy);
                if y < 0 || y >= size {
                    continue;
                }
                let id = TileId::new(DATA_ZOOM, x as u32, y as u32);
                let Some(BSlot::Ready(b)) = self.builds.get(&id) else { continue };
                let lp = (p - b.center).as_vec3();
                for f in &b.footprints {
                    if !f.alive || (f.center - lp).length() > r as f32 + f.area.sqrt() {
                        continue;
                    }
                    let ring = f.ring.iter().map(|q| {
                        let (la, lo) = id.lat_lon(q[0] as f64, q[1] as f64);
                        geo::lla_to_pos(la, lo, f.base as f64)
                    }).collect();
                    out.push((ring, f.base as f64, f.top as f64, f.kind == 1));
                }
            }
        }
        out
    }

    pub fn add_crater(&mut self, pos: DVec3, radius: f32) {
        if self.craters.len() >= 8 {
            self.craters.remove(0);
        }
        self.craters.push((pos, radius));
    }

    /// Drive the traffic on the tiles around a point. `player` is where the
    /// person on foot or the car being driven is, so traffic can brake for them.
    pub fn update_traffic(&mut self, dt: f32, focus: DVec3, player: Option<PlayerPresence>) {
        self.tick = self.tick.wrapping_add(1);
        let tick = self.tick;
        let mut exits: Vec<(TileId, cars::Exit, glam::Vec3)> = Vec::new();
        for (id, slot) in self.builds.iter_mut() {
            let BSlot::Ready(b) = slot else { continue };
            if (b.center - focus).length() > 2600.0 + b.radius {
                continue;
            }
            let up = b.center.normalize().as_vec3();
            if !b.cars.is_empty() {
                let near = player.filter(|p| (p.pos - b.center).length() <= p.radius() + b.radius + 30.0).map(|p| ((p.pos - b.center).as_vec3(), p.radius() as f32));
                let mut cs = std::mem::take(&mut b.cars);
                for e in cars::step(&b.net, &mut cs, dt, up, tick, near) {
                    let was = cars::pose(&b.net, &e.car, up).0;
                    exits.push((*id, e, was));
                }
                b.cars = cs;
            }
            // Parked cars that have been hit roll clear.
            if b.parked.iter().any(|p| p.bump != Vec3::ZERO) {
                for pk in &mut b.parked {
                    cars::shove_step(&mut pk.shift, &mut pk.bump, dt);
                }
            }
        }
        let size = 1i64 << DATA_ZOOM;
        for (from, e, was) in exits {
            let (dx, dy) = match e.edge {
                1 => (-1, 0),
                2 => (1, 0),
                3 => (0, -1),
                _ => (0, 1),
            };
            let nid = TileId::new(DATA_ZOOM, (from.x as i64 + dx).rem_euclid(size) as u32, (from.y as i64 + dy).clamp(0, size - 1) as u32);
            let from_center = match self.builds.get(&from) {
                Some(BSlot::Ready(b)) => b.center,
                _ => continue,
            };
            let mut car = Some(e.car);
            if let Some(BSlot::Ready(n)) = self.builds.get_mut(&nid) {
                let up = n.center.normalize().as_vec3();
                let was_n = ((from_center - n.center).as_vec3()) + was;
                if let Some(c) = cars::enter(&n.net, car.clone().unwrap(), e.key, up, was_n) {
                    n.cars.push(c);
                    car = None;
                }
            }
            // Nowhere to go (next tile not loaded, or the road ends at the border): start afresh.
            if let (Some(mut c), Some(BSlot::Ready(b))) = (car, self.builds.get_mut(&from)) {
                cars::respawn(&b.net, &mut c, tick ^ (e.key as u32));
                b.cars.push(c);
            }
        }
    }

    /// Instances for every visible moving and parked car; and, at night
    /// (`sun`: sine of the sun's elevation at the camera), the lights of the
    /// moving ones that the 3D tiles hide (`photo_car_light`).
    ///
    /// This is also the traffic the physics treats as solid (see
    /// [`World::traffic_bodies_near`]): the time-of-day share may thin out
    /// only cars far from the camera, never one close enough to reach the
    /// player, and Cars: Off draws and collides with none.
    pub fn car_instances(&self, cam: DVec3, activity: f32, sun: f64, out: &mut Vec<(u32, Inst)>, lights: &mut Vec<crate::airfield::Light>) {
        let fr = self.frustum.as_ref();
        let push = |pos: DVec3, fwd: glam::Vec3, up: glam::Vec3, model: u8, col: [f32; 3], out: &mut Vec<(u32, Inst)>| {
            let o = (pos - cam).as_vec3();
            if fr.map(|f| f.sphere(pos - cam, 8.0)).unwrap_or(true) {
                let right = fwd.cross(up).normalize_or_zero();
                let upv = right.cross(fwd);
                out.push((model as u32, Inst { m: [[right.x, upv.x, -fwd.x, o.x], [right.y, upv.y, -fwd.y, o.y], [right.z, upv.z, -fwd.z, o.z]], tint: [col[0], col[1], col[2], 0.0], ext: [0.0, 0.0, 0.0, 1.0] }));
            }
        };
        let photo = self.photo.as_deref().filter(|_| sun < 0.15);
        let (share, car_far, parked_far) = self.cars;
        if share <= 0.0 {
            return;
        }
        // Each car has its own rank: the settings keep the lower share of them.
        let activity = activity * share;
        for id in &self.draw_b {
            let Some(BSlot::Ready(b)) = self.builds.get(id) else { continue };
            let d = (b.center - cam).length() - b.radius;
            if d > car_far {
                continue;
            }
            let lit = photo.filter(|p| p.covers(b.center));
            let mut streets = None;
            let up = b.center.normalize().as_vec3();
            for (i, c) in b.cars.iter().enumerate() {
                if b.net.paths[c.path as usize].hidden {
                    continue;
                }
                let (p, dir) = cars::pose(&b.net, c, up);
                let pos = b.center + (p + c.shift).as_dvec3();
                let d2 = (pos - cam).length_squared();
                // A car close enough to reach the player is always drawn: the
                // physics treats every car as solid, and a car that yields or
                // blocks must be visible. The time-of-day share only thins the
                // far traffic out of the view.
                if c.rank > activity && d2 > CAR_NEAR_DRAW * CAR_NEAR_DRAW {
                    continue;
                }
                if d2 < car_far * car_far {
                    let fwd = if c.dir == glam::Vec3::ZERO { dir } else { c.dir };
                    push(pos, fwd, up, c.model, c.col, out);
                    if let Some(photo) = lit {
                        self.photo_car_light(photo, &mut streets, b, i, c, pos, fwd, cam, lights);
                    }
                }
            }
            if d < parked_far {
                for p in &b.parked {
                    let pos = b.center + (p.pos + p.shift).as_dvec3();
                    if (pos - cam).length_squared() < parked_far * parked_far {
                        push(pos, p.fwd, p.up, p.model, p.col, out);
                    }
                }
            }
        }
    }

    pub fn update(&mut self, r: &Renderer, cam: DVec3, fr: &Frustum) {
        self.frame += 1;
        let now = std::time::Instant::now();
        let real_dt = self.last_update.map(|t| (now - t).as_secs_f32().min(0.25)).unwrap_or(0.0);
        self.last_update = Some(now);
        // Take finished tiles off the worker pool, capping the GPU uploads per
        // frame both in count and in bytes (see UPLOAD_BUDGET). Terrain goes
        // first; queued building geometry always gets at least half the bytes.
        let mut bytes = 0;
        let mut budget = 10;
        while budget > 0 && bytes < UPLOAD_BUDGET {
            match self.loader.rx.try_recv() {
                Ok(JobResult::Terrain(id, res)) => {
                    self.in_flight -= 1;
                    budget -= 1;
                    let slot = match res {
                        Ok(td) => {
                            bytes += crate::render::tile_bytes(&td);
                            Slot::Ready(Box::new(Tile { center: td.center, radius: td.radius, gpu: r.upload_tile(&td), grid: td.verts[..GV * GV].iter().map(|v| v.pos).collect(), heights: td.heights, wet: td.water.iter().any(|w| *w), water: td.water, surface: td.surface, sea: td.sea, last_used: self.frame, generation: td.imagery_gen, refreshing: false }))
                        }
                        Err(e) => {
                            log::debug!("tile {id:?} failed: {e}");
                            // A refresh that failed keeps the tile it was replacing.
                            if let Some(Slot::Ready(t)) = self.tiles.get_mut(&id) {
                                (t.generation, t.refreshing) = (self.loader.src.google.generation(), false);
                                continue;
                            }
                            Slot::Failed
                        }
                    };
                    self.tiles.insert(id, slot);
                }
                Ok(JobResult::Buildings(id, res)) => {
                    self.in_flight -= 1;
                    budget -= 3;
                    match res {
                        Ok(bd) => {
                            // The slot stays Loading until the geometry is all on the GPU.
                            bytes += bd.atlas.iter().map(|m| m.len()).sum::<usize>();
                            let gpu = r.upload_buildings(&bd);
                            self.uploads.push_back(Upload { id, data: Box::new(bd), gpu, part: 0, off: 0 });
                        }
                        Err(_) => {
                            self.builds.remove(&id);
                        }
                    }
                }
                Ok(JobResult::Airfield(i, res)) => {
                    self.in_flight -= 1;
                    budget -= 3;
                    match res {
                        Ok((geo, mesh)) => {
                            bytes += Renderer::field_bytes(&mesh);
                            let gpu = r.upload_field(&mesh);
                            let traffic = crate::traffic::Traffic::new(&geo, i.wrapping_mul(2654435761) ^ 0x5bd1e995);
                            log::info!("airfield {} ready: {} runways, {} stands, {} lights", geo.apt().ident, geo.runways.len(), geo.stands.len(), mesh.lights.len());
                            self.fields.insert(i, FSlot::Ready(Box::new(Field { geo, gpu, props: mesh.props, socks: mesh.socks, scenery: mesh.scenery, floods: mesh.floods, traffic, last_used: self.frame })));
                        }
                        Err(e) => {
                            log::debug!("airfield {i} failed: {e}");
                            self.fields.insert(i, FSlot::Failed);
                        }
                    }
                }
                Err(_) => break,
            }
        }
        self.pump_uploads(r, bytes.min(UPLOAD_BUDGET / 2));

        let (clat, clon, calt) = geo::pos_to_lla(cam);
        let gh = self.ground(clat, clon).map(|g| g.h).unwrap_or(0.0);
        self.draw.clear();
        self.draw_shore.clear();
        self.wanted.clear();
        for y in 0..4 {
            for x in 0..4 {
                self.select(TileId::new(2, x, y), cam, (clat, clon, calt.max(1.0), gh), fr);
            }
        }

        // Buildings near the ground.
        self.draw_b.clear();
        self.in_view.clear();
        self.frustum = Some(fr.clone());
        let agl = calt - gh;
        if self.buildings_on && agl < 7000.0 && clat.abs() < 1.45 {
            let c = TileId::at(clat, clon, DATA_ZOOM);
            let w = c.width_m();
            let reach = (4500.0 + agl * 0.6).min(7500.0) * self.reach_k;
            let n = (reach / w).ceil() as i64;
            let size = 1i64 << DATA_ZOOM;
            for dy in -n..=n {
                for dx in -n..=n {
                    let (x, y) = ((c.x as i64 + dx).rem_euclid(size), c.y as i64 + dy);
                    if y < 0 || y >= size {
                        continue;
                    }
                    let id = TileId::new(DATA_ZOOM, x as u32, y as u32);
                    let (lat, lon) = id.lat_lon(0.5, 0.5);
                    let d = (geo::lla_to_pos(lat, lon, gh) - cam).length();
                    if d > reach + w {
                        continue;
                    }
                    match self.builds.get_mut(&id) {
                        Some(BSlot::Ready(b)) => {
                            b.last_used = self.frame;
                            // Detail arrives gradually: the tile dissolves in, then its middle and
                            // finest levels as the camera comes close enough for each.
                            let dist = ((b.center - cam).length() - b.radius) as f32;
                            b.shown += real_dt;
                            // Towers carry their detail farther (buildings::HCLASS).
                            let (mid, fine) = (LOD_MID * b.tall * self.city_k, LOD_FINE * b.tall * self.city_k);
                            b.mid_t = if dist < mid { b.mid_t + real_dt } else if dist > mid * 1.1 { 0.0 } else { b.mid_t };
                            b.fine_t = if dist < fine { b.fine_t + real_dt } else if dist > fine * 1.2 { 0.0 } else { b.fine_t };
                            if b.gpu.is_some() {
                                self.draw_b.push(id);
                                self.in_view.push(fr.sphere(b.center - cam, b.radius));
                            }
                        }
                        Some(BSlot::Loading) => {}
                        None => self.wanted.push((0.8 + (d / w) as f32 * 0.6, id.with_tag())),
                    }
                }
            }
        }

        // Airfields within reach: the bigger ones from further off.
        if self.frame % 15 == 1 && clat.abs() < 1.45 {
            let db = crate::airports::Db::get();
            let mut n = 0;
            for (i, d) in db.near(clat.to_degrees(), clon.to_degrees(), 35_000.0) {
                let a = &db.apts[i as usize];
                let reach = [9000.0, 22_000.0, 35_000.0][a.kind as usize] * if a.any_paved() { 1.0 } else { 0.5 };
                if d > reach || n >= 6 {
                    continue;
                }
                n += 1;
                match self.fields.get_mut(&i) {
                    Some(FSlot::Ready(f)) => f.last_used = self.frame,
                    Some(_) => {}
                    None if self.in_flight < 28 => {
                        self.fields.insert(i, FSlot::Loading);
                        self.in_flight += 1;
                        if let Some(evicted) = self.loader.submit(1.0 + (d / 9000.0) as f32, Job::Airfield(i)) {
                            self.drop_loading(evicted);
                        }
                    }
                    None => {}
                }
            }
        }

        // Cancel still-queued jobs that are now far behind the camera (fast flight / teleport).
        if self.in_flight > 0 && self.frame % 15 == 0 {
            let tile_pos = |id: TileId| {
                let (la, lo) = id.lat_lon(0.5, 0.5);
                geo::lla_to_pos(la, lo, 0.0)
            };
            let cancelled = self.loader.cancel_where(|job| match *job {
                Job::Terrain(id) => id.z >= 10 && (tile_pos(id) - cam).length() > id.width_m() * 6.0 + 15_000.0,
                Job::Buildings(id) => (tile_pos(id) - cam).length() > id.width_m() * 6.0 + 15_000.0,
                Job::Airfield(_) => false,
            });
            for job in cancelled {
                self.drop_loading(job);
            }
        }

        // Dispatch the most urgent requests.
        self.wanted.sort_by(|a, b| a.0.total_cmp(&b.0));
        // (What is not asked for this time stays in `wanted` until the next
        // update clears it: `queued` counts it.)
        // Building tiles being made or waiting for the GPU: each is some
        // 150-250 MB of memory in a city until uploaded, so only a few at once
        // (`max_build_jobs`); the rest wait their turn in `wanted`.
        let mut building = self.builds.values().filter(|s| matches!(s, BSlot::Loading)).count();
        let mut sent = vec![false; self.wanted.len()];
        for k in 0..self.wanted.len() {
            if self.in_flight >= 28 {
                break;
            }
            let (prio, id) = self.wanted[k];
            if id.z & 0x80 != 0 {
                // The `--carjack` test tile is made and owned by the smoke run.
                if self.test_traffic.is_some_and(|t| t.z == id.z & 0x7f && t.x == id.x && t.y == id.y) {
                    sent[k] = true;
                    continue;
                }
                if building >= self.max_build_jobs {
                    continue;
                }
                building += 1;
            }
            sent[k] = true;
            self.in_flight += 1;
            let evicted = if id.z & 0x80 != 0 {
                let id = TileId::new(id.z & 0x7f, id.x, id.y);
                self.builds.insert(id, BSlot::Loading);
                self.loader.submit(prio, Job::Buildings(id))
            } else if id.z & 0x40 != 0 {
                // keeps drawing the old tile until the new one is in
                let id = TileId::new(id.z & 0x3f, id.x, id.y);
                match self.tiles.get_mut(&id) {
                    Some(Slot::Ready(t)) if !t.refreshing => {
                        t.refreshing = true;
                        self.loader.submit(prio, Job::Terrain(id))
                    }
                    _ => {
                        self.in_flight -= 1;
                        None
                    }
                }
            } else {
                self.tiles.insert(id, Slot::Loading);
                self.loader.submit(prio, Job::Terrain(id))
            };
            if let Some(ev) = evicted {
                self.drop_loading(ev);
            }
        }

        let mut k = 0;
        self.wanted.retain(|_| {
            k += 1;
            !sent[k - 1]
        });

        // Evict what has not been needed for a while.
        if self.frame % 120 == 0 {
            let f = self.frame;
            if self.tiles.len() > self.tile_cap {
                self.tiles.retain(|id, s| id.z <= 3 || !matches!(s, Slot::Ready(t) if f - t.last_used > 600));
            }
            // Still over: the least recently used of those not drawn in the
            // last couple of seconds go too.
            if self.tiles.len() > self.tile_cap {
                let mut old: Vec<(u64, TileId)> = self.tiles.iter().filter_map(|(id, s)| match s { Slot::Ready(t) if id.z > 3 && f - t.last_used > 120 => Some((t.last_used, *id)), _ => None }).collect();
                old.sort_unstable_by_key(|o| o.0);
                for (_, id) in old.iter().take(self.tiles.len() - self.tile_cap) {
                    self.tiles.remove(id);
                }
            }
            self.tiles.retain(|_, s| !matches!(s, Slot::Failed) || f % 3600 != 0);
            if let Some(id) = self.test_traffic {
                if let Some(BSlot::Ready(b)) = self.builds.get_mut(&id) {
                    b.last_used = f;
                }
            }
            self.builds.retain(|_, s| !matches!(s, BSlot::Ready(b) if f - b.last_used > 900));
            // Of the building tiles left behind (out of reach this frame),
            // only the most recent few stay, for turning back.
            let mut behind: Vec<(u64, TileId)> = self.builds.iter().filter_map(|(id, s)| match s { BSlot::Ready(b) if b.last_used != f => Some((b.last_used, *id)), _ => None }).collect();
            if behind.len() > self.spare_builds {
                behind.sort_unstable_by_key(|b| b.0);
                for (_, id) in &behind[..behind.len() - self.spare_builds] {
                    self.builds.remove(id);
                }
            }
            self.fields.retain(|_, s| match s {
                FSlot::Ready(fl) => f - fl.last_used < 1200,
                FSlot::Failed => f % 3600 != 0,
                FSlot::Loading => true,
            });
        }
        self.stats = (self.draw.len(), self.in_flight, self.draw_b.len());
    }

    fn drop_loading(&mut self, job: Job) {
        self.in_flight = self.in_flight.saturating_sub(1);
        match job {
            Job::Terrain(id) => match self.tiles.get_mut(&id) {
                Some(Slot::Loading) => { self.tiles.remove(&id); }
                Some(Slot::Ready(t)) => t.refreshing = false,
                _ => {}
            },
            Job::Buildings(id) => {
                if matches!(self.builds.get(&id), Some(BSlot::Loading)) {
                    self.builds.remove(&id);
                }
            }
            Job::Airfield(i) => {
                if matches!(self.fields.get(&i), Some(FSlot::Loading)) {
                    self.fields.remove(&i);
                }
            }
        }
    }

    /// Copy queued building geometry to the GPU until `bytes` (already used
    /// this frame) reaches the budget; finished tiles become drawable.
    fn pump_uploads(&mut self, r: &Renderer, mut bytes: usize) -> usize {
        while bytes < UPLOAD_BUDGET {
            let Some(u) = self.uploads.front_mut() else { break };
            let payload = crate::render::building_payload(&u.data);
            let targets = u.gpu.targets();
            while u.part < payload.len() && bytes < UPLOAD_BUDGET {
                let (Some(buf), data) = (targets[u.part], payload[u.part]) else {
                    (u.part, u.off) = (u.part + 1, 0);
                    continue;
                };
                if u.off >= data.len() {
                    (u.part, u.off) = (u.part + 1, 0);
                    continue;
                }
                // Chunks stay multiples of 4 (all the vertex types are).
                let n = (data.len() - u.off).min(UPLOAD_CHUNK).min((UPLOAD_BUDGET - bytes).max(1 << 20)) & !3;
                let n = if n == 0 { data.len() - u.off } else { n };
                r.fill_buffer(buf, u.off as u64, &data[u.off..u.off + n]);
                u.off += n;
                bytes += n;
            }
            if u.part < payload.len() {
                break;
            }
            let Upload { id, data, gpu, .. } = self.uploads.pop_front().unwrap();
            let bd = *data;
            let main = bd.cls[0];
            let tall = (0..buildings::HCLASS.len()).find(|&c| main[c] > if c == 0 { 0 } else { main[c - 1] }).map_or(1.0, |c| buildings::HCLASS[c].1);
            let cars = cars::spawn(&bd.net, id.x.wrapping_mul(2246822519) ^ id.y.wrapping_mul(3266489917));
            self.builds.insert(id, BSlot::Ready(Box::new(BTile { center: bd.center, radius: bd.radius, gpu: Some(gpu), footprints: bd.footprints, last_used: self.frame, net: bd.net, road_index: bd.road_index, cars, parked: bd.parked, trees: bd.trees, tree_var: bd.tree_var, chunks: bd.chunks, under: bd.under, cells: bd.cells, near_idx: bd.near_idx, ax: bd.ax, ay: bd.ay, shown: 0.0, mid_t: 0.0, fine_t: 0.0, tall })));
        }
        bytes
    }

    /// whether any drawn terrain tile has water on it.
    pub fn water_in_view(&self) -> bool {
        self.draw.iter().any(|id| matches!(self.tiles.get(id), Some(Slot::Ready(t)) if t.wet))
    }

    /// Zoom of the most detailed terrain tile loaded at a spot.
    pub fn ground_zoom(&self, lat: f64, lon: f64) -> Option<u8> {
        let (mx, my) = (geo::lon_to_merc_x(lon).rem_euclid(1.0), geo::lat_to_merc_y(lat));
        (2..=self.max_zoom).rev().find(|z| matches!(self.tiles.get(&TileId::at_merc(mx, my, *z)), Some(Slot::Ready(_))))
    }

    /// The nearest airfield that is loaded, within `r` metres.
    pub fn airfield_near(&self, p: DVec3, r: f64) -> Option<&Field> {
        self.fields.values().filter_map(|s| if let FSlot::Ready(f) = s { Some(f.as_ref()) } else { None }).filter(|f| (f.geo.center - p).length() < r + f.geo.radius as f64).min_by(|a, b| (a.geo.center - p).length().total_cmp(&(b.geo.center - p).length()))
    }
    #[allow(dead_code)]
    pub fn airfield(&self, idx: u32) -> Option<&Field> {
        match self.fields.get(&idx) {
            Some(FSlot::Ready(f)) => Some(f),
            _ => None,
        }
    }

    /// The nearest parked airport aircraft within `radius` of `p`, whatever its
    /// type. `take_parked_aircraft` removes it once the player gets in.
    pub fn parked_aircraft_near(&self, p: DVec3, radius: f64) -> Option<ParkedAircraft> {
        let mut best: Option<(f64, ParkedAircraft)> = None;
        for s in self.fields.values() {
            let FSlot::Ready(f) = s else { continue };
            for parked in &f.traffic.parked {
                let Some(st) = f.geo.stands.get(parked.stand) else { continue };
                let (lp, dir) = crate::traffic::stand_pose(st, parked.ty);
                let pos = f.geo.world(lp, f.geo.h(lp) as f64);
                let d = (pos - p).length();
                if d > radius || best.as_ref().is_some_and(|(bd, _)| d >= *bd) {
                    continue;
                }
                let slope = (f.geo.h(lp + dir * 3.0) - f.geo.h(lp - dir * 3.0)) as f64 / 6.0;
                let a = ParkedAircraft {
                    apt: f.geo.idx,
                    stand: parked.stand,
                    ty: parked.ty,
                    pos,
                    yaw: (dir.x as f64).atan2(dir.y as f64),
                    pitch: slope.atan(),
                };
                best = Some((d, a));
            }
        }
        best.map(|(_, a)| a)
    }

    /// Take the aircraft parked at `stand` of airfield `apt` out of the traffic.
    pub fn take_parked_aircraft(&mut self, apt: u32, stand: usize) -> Option<crate::traffic::Parked> {
        let Some(FSlot::Ready(f)) = self.fields.get_mut(&apt) else { return None };
        f.traffic.take_parked(stand)
    }

    /// Move the airport traffic on. `player`: the player's aircraft position.
    pub fn update_airports(&mut self, dt: f32, wind_from: f32, wind_kt: f32, activity: f32, player: Option<DVec3>, focus: DVec3) {
        for s in self.fields.values_mut() {
            let FSlot::Ready(f) = s else { continue };
            if (f.geo.center - focus).length() > 40_000.0 {
                continue;
            }
            let pl = player.map(|p| {
                let lp = f.geo.local(p);
                (lp, geo::pos_to_lla(p).2 as f32 - f.geo.h(lp))
            });
            f.traffic.step(&f.geo, dt, wind_from, wind_kt, activity, pl);
        }
    }

    /// The airborne aircraft at nearby airports, as missile targets.
    pub fn air_targets(&self, near: DVec3) -> Vec<Target> {
        let mut out = Vec::new();
        for s in self.fields.values() {
            let FSlot::Ready(f) = s else { continue };
            if (f.geo.center - near).length() > 40_000.0 {
                continue;
            }
            f.traffic.targets(&f.geo, &mut out);
        }
        out
    }

    /// A missile hit an airport mover: take it out of the traffic.
    pub fn down_air_target(&mut self, key: &TargetKey) -> bool {
        let TargetKey::Airport { apt, mover } = key else { return false };
        let Some(FSlot::Ready(f)) = self.fields.get_mut(apt) else { return false };
        f.traffic.down(*mover)
    }

    /// Pavement to draw: each airfield with its centre relative to the camera.
    pub fn field_draws(&self, cam: DVec3) -> Vec<(&crate::render::FieldGpu, [f32; 3])> {
        let mut out: Vec<(f64, &crate::render::FieldGpu, [f32; 3])> = Vec::new();
        for s in self.fields.values() {
            let FSlot::Ready(f) = s else { continue };
            let o = f.geo.center - cam;
            let d = o.length();
            if d > 60_000.0 || !self.frustum.as_ref().map(|fr| fr.sphere(o, f.geo.radius as f64 + 600.0)).unwrap_or(true) {
                continue;
            }
            out.push((d, &f.gpu, o.as_vec3().into()));
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0));
        out.into_iter().take(crate::render::MAX_FIELDS).map(|x| (x.1, x.2)).collect()
    }

    /// Fixed airport objects, parked and moving aircraft and their lights.
    /// `scenery` takes the hand-made scenery packs' objects (scenery.rs).
    /// `wind`: where it blows from (degrees) and how hard (knots).
    /// The eight floodlights nearest the camera within 3 km, for Globals
    /// `flood` (common.wgsl `flood_light`), `on` 0..1 as they come on at dusk.
    pub fn flood_globals(&self, cam: DVec3, on: f32, out: &mut [[f32; 4]; 16]) {
        let mut near: Vec<(f64, DVec3, Vec3)> = Vec::new();
        for s in self.fields.values() {
            let FSlot::Ready(f) = s else { continue };
            for (p, d) in &f.floods {
                let o = f.geo.center - cam + p.as_dvec3();
                let r = o.length();
                if r < FLOOD_REACH {
                    near.push((r, o, *d));
                }
            }
        }
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (k, (r, o, d)) in near.iter().take(out.len() / 2).enumerate() {
            // fading out toward the edge of the reach, so none pops in or out
            let fade = (((FLOOD_REACH - r) / 500.0) as f32).clamp(0.0, 1.0);
            out[2 * k] = [o.x as f32, o.y as f32, o.z as f32, FLOOD_I * on * fade];
            out[2 * k + 1] = [d.x, d.y, d.z, FLOOD_COS];
        }
    }

    pub fn airport_instances(&self, cam: DVec3, wind: (f32, f32), t: f64, out: &mut Vec<(u32, Inst)>, scenery: &mut Vec<(u32, Inst)>, lights: &mut Vec<crate::airfield::Light>) {
        let (wind_from, wind_kt) = wind;
        let fr = self.frustum.as_ref();
        for s in self.fields.values() {
            let FSlot::Ready(f) = s else { continue };
            let g = &f.geo;
            let dc = (g.center - cam).length();
            if dc > 40_000.0 {
                continue;
            }
            let base = g.center - cam;
            for p in &f.props {
                let o = base + p.pos.as_dvec3();
                if o.length() > 6000.0 || !fr.map(|f| f.sphere(o, 40.0)).unwrap_or(true) {
                    continue;
                }
                let o = o.as_vec3();
                out.push((p.mesh, Inst { m: [[p.x.x, p.y.x, p.z.x, o.x], [p.x.y, p.y.y, p.z.y, o.y], [p.x.z, p.y.z, p.z.z, o.z]], tint: [p.tint[0], p.tint[1], p.tint[2], 0.0], ext: [0.0; 4] }));
            }
            // Windsocks stream downwind, and droop in a light breeze.
            let b = (wind_from + 180.0).to_radians();
            let hd = g.dir3(Vec3::new(b.sin(), b.cos(), 0.0));
            let up = g.dir3(Vec3::Z);
            for (k, sp) in f.socks.iter().enumerate() {
                let o = base + sp.as_dvec3();
                if o.length() > 5000.0 {
                    continue;
                }
                let fwd = sock_dir(hd, up, wind_kt, t, k);
                let upv = (up - fwd * fwd.dot(up)).normalize();
                let x = fwd.cross(upv);
                let z = -fwd;
                let o = o.as_vec3();
                out.push((crate::airmodels::M_SOCK, Inst { m: [[x.x, upv.x, z.x, o.x], [x.y, upv.y, z.y, o.y], [x.z, upv.z, z.z, o.z]], tint: [1.0, 1.0, 1.0, 0.0], ext: [0.0; 4] }));
            }
            // The scenery pack's objects, each as far as its class reaches; its
            // windsocks turn with the wind like the procedural ones. They give
            // way to the 3D tiles (ext.w).
            let mut ks = 0;
            for (pi, p) in f.scenery.iter().enumerate() {
                let o = base + DVec3::new(p.m[0][3] as f64, p.m[1][3] as f64, p.m[2][3] as f64);
                let r = p.radius as f64;
                if o.length() > crate::scenery::reach(p.class) as f64 + r || !fr.map(|f| f.sphere(o, r)).unwrap_or(true) {
                    if p.class == crate::scenery::C_SOCK {
                        ks += 1;
                    }
                    continue;
                }
                let o = o.as_vec3();
                let mut m = p.m;
                if p.class == crate::scenery::C_SOCK {
                    let (x, y, z) = crate::scenery::sock_axes(sock_dir(hd, p.up, wind_kt, t, ks), p.up);
                    ks += 1;
                    m = [[x.x, y.x, z.x, 0.0], [x.y, y.y, z.y, 0.0], [x.z, y.z, z.z, 0.0]];
                }
                (m[0][3], m[1][3], m[2][3]) = (o.x, o.y, o.z);
                // ext.y: a steady random number per object (scenery.wgsl varies its wear by it)
                let rnd = ((pi as u32).wrapping_mul(2_654_435_761) >> 8) as f32 / 16_777_216.0;
                scenery.push((p.mesh, Inst { m, tint: [p.tint[0], p.tint[1], p.tint[2], 0.0], ext: [0.0, rnd, 0.0, 1.0] }));
            }
            f.traffic.draw(g, cam, t, out, lights);
        }
    }

    /// Nearest point of a tile to the camera, on the ground, and whether the
    /// whole tile lies below the horizon (never drawn, so never needed).
    fn tile_near(id: TileId, cam: DVec3, c: (f64, f64, f64, f64)) -> (DVec3, bool) {
        let (clat, clon, calt, gh) = c;
        let (lat_n, lon_w) = id.lat_lon(0.0, 0.0);
        let (lat_s, lon_e) = id.lat_lon(1.0, 1.0);
        let mut dl = clon;
        if dl < lon_w - std::f64::consts::PI {
            dl += std::f64::consts::TAU;
        } else if dl > lon_e + std::f64::consts::PI {
            dl -= std::f64::consts::TAU;
        }
        let near = geo::lla_to_pos(clat.clamp(lat_s, lat_n), dl.clamp(lon_w, lon_e), gh);
        let hidden = id.z > 2 && {
            let ang = (near.normalize().dot(cam.normalize())).clamp(-1.0, 1.0).acos();
            let horizon = (geo::R / (geo::R + calt)).acos() + (geo::R / (geo::R + 9000.0)).acos();
            ang > horizon
        };
        (near, hidden)
    }

    /// Returns true if this tile (or its descendants) covers its area.
    fn select(&mut self, id: TileId, cam: DVec3, c: (f64, f64, f64, f64), fr: &Frustum) -> bool {
        let (near, hidden) = Self::tile_near(id, cam, c);
        let dist = (near - cam).length().max(1.0);
        if hidden {
            return true;
        }
        let (center, radius) = match self.tiles.get_mut(&id) {
            Some(Slot::Ready(t)) => {
                t.last_used = self.frame;
                // made with the imagery source switched away from: again, after what is missing
                if !t.refreshing && t.generation != self.loader.src.google.generation() {
                    self.wanted.push((2.0 + (dist / id.width_m()) as f32, id.with_refresh()));
                }
                (t.center, t.radius)
            }
            Some(_) => return false,
            None => {
                self.wanted.push(((dist / id.width_m()) as f32, id));
                return false;
            }
        };
        let width = id.width_m();
        if id.z < self.max_zoom && width / dist > self.lod_k {
            let kids = id.children();
            let mut all = true;
            for k in kids {
                // A child below the horizon is never drawn: it need not be in
                // memory (and is evicted, not being used), so it must not
                // hold its siblings back and leave this coarser tile showing.
                if Self::tile_near(k, cam, c).1 {
                    continue;
                }
                match self.tiles.get(&k) {
                    Some(Slot::Ready(_)) => {}
                    Some(Slot::Loading) => all = false,
                    Some(Slot::Failed) => all = false,
                    None => {
                        all = false;
                        self.wanted.push(((dist / width) as f32 + 0.01, k));
                    }
                }
            }
            if all {
                for k in kids {
                    self.select(k, cam, c, fr);
                }
                return true;
            }
        }
        if fr.sphere(center - cam, radius) {
            self.draw.push(id);
        } else if (center - cam).length() - radius < crate::shore::WIDE_HALF * 1.6 {
            self.draw_shore.push(id);
        }
        true
    }

    /// Terrain tiles out of view that reach into the shore mask's square
    /// (centre `c`, both camera-relative), with their uniforms as `draw_lists`.
    pub fn shore_tiles(&self, cam: DVec3, c: DVec3) -> Vec<(&TileGpu, [f32; 12])> {
        let mut out = Vec::new();
        for id in &self.draw_shore {
            if let Some(Slot::Ready(t)) = self.tiles.get(id) {
                let o = t.center - cam;
                if (o - c).length() - t.radius > crate::shore::WIDE_HALF * 1.45 {
                    continue;
                }
                let o = o.as_vec3();
                out.push((&t.gpu, [o.x, o.y, o.z, (id.width_m() / GRID as f64) as f32, id.z as f32, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
            }
        }
        out
    }

    /// The corner (x, y at sources::DATA_ZOOM) of the square of building tiles
    /// around the camera that `cover` describes.
    pub fn cover_base(cam: DVec3) -> (i64, i64) {
        let (lat, lon, _) = geo::pos_to_lla(cam);
        let t = TileId::at(lat, lon, DATA_ZOOM);
        let h = (crate::render::COVER_N / 2) as i64;
        (t.x as i64 - h, t.y as i64 - h)
    }

    /// A terrain tile's place in the cover square (terrain.wgsl `city_glow`):
    /// its corner in building tiles from the square's corner, and the square's
    /// corner itself (x | y << 14, as the bits of a float) so the shader can
    /// tell where on Earth it is exactly.
    fn cover_info(id: TileId, base: (i64, i64)) -> [f32; 3] {
        let s = 2f64.powi(DATA_ZOOM as i32 - id.z as i32);
        let packed = (base.0.clamp(0, 16383) as u32) | ((base.1.clamp(0, 16383) as u32) << 14);
        [(id.x as f64 * s - base.0 as f64) as f32, (id.y as f64 * s - base.1 as f64) as f32, f32::from_bits(packed)]
    }

    /// Building tiles over which the night glow on the terrain hands over to
    /// the lit windows, counted in from the edge of the loaded ones.
    const COVER_RAMP: f32 = 3.0;

    /// How far each building tile in the square around the camera has faded
    /// in (render::Frame::cover): the terrain leaves its city lights to them.
    pub fn light_cover(&self, cam: DVec3) -> Vec<u8> {
        let n = crate::render::COVER_N;
        let mut out = vec![0u8; n * n];
        if !self.buildings_on {
            return out;
        }
        let mut shown = vec![0f32; n * n];
        let base = Self::cover_base(cam);
        for id in &self.draw_b {
            if let Some(BSlot::Ready(b)) = self.builds.get(id) {
                let (x, y) = (id.x as i64 - base.0, id.y as i64 - base.1);
                if (0..n as i64).contains(&x) && (0..n as i64).contains(&y) {
                    shown[y as usize * n + x as usize] = b.shown.clamp(0.0, 1.0);
                }
            }
        }
        // Soft erosion: the glow only gives way some tiles inside the edge of
        // what is loaded, ramping over COVER_RAMP tiles, so there is no line
        // where it stops (the terrain samples this bilinearly as well). Outside
        // the square counts as not covered.
        let r = Self::COVER_RAMP.ceil() as i64 + 1;
        let ramp = |d2: i64| {
            let t = (((d2 as f32).sqrt() - 1.0) / (Self::COVER_RAMP - 1.0)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        for y in 0..n as i64 {
            for x in 0..n as i64 {
                let mut v = shown[y as usize * n + x as usize];
                if v <= 0.0 {
                    continue;
                }
                for dy in -r..=r {
                    for dx in -r..=r {
                        let (u, w) = (x + dx, y + dy);
                        let c = if (0..n as i64).contains(&u) && (0..n as i64).contains(&w) { shown[w as usize * n + u as usize] } else { 0.0 };
                        if c < v {
                            v = v.min(c + ramp(dx * dx + dy * dy));
                        }
                    }
                }
                out[y as usize * n + x as usize] = (v * 255.0) as u8;
            }
        }
        out
    }

    /// Terrain tiles and detail tiles to draw. `focus` and `casc` (radius of
    /// each shadow cascade) decide what has to be rendered into the shadow maps,
    /// with `casc_vp` tightening tree shadow culling to each cascade's ortho box.
    /// Also the tessellated cells of the terrain tiles, by the patch they are
    /// drawn with (`tess_cells`).
    pub fn draw_lists(&self, cam: DVec3, focus: DVec3, casc: [f32; 3], casc_vp: &[[[f32; 4]; 4]; 3]) -> (Vec<(&TileGpu, [f32; 12])>, TessCells, Vec<DetailDraw<'_>>, Vec<TreeInst>) {
        let mut drawn = Vec::with_capacity(self.draw.len());
        let (clat, clon, calt) = geo::pos_to_lla(cam);
        let agl = calt - self.ground(clat, clon).map(|g| g.h).unwrap_or(0.0);
        let base = Self::cover_base(cam);
        for id in &self.draw {
            if let Some(Slot::Ready(t)) = self.tiles.get(id) {
                let o = (t.center - cam).as_vec3();
                let cell = id.width_m() / GRID as f64;
                let tess = tess_block(*id, clat, clon, agl, cell);
                let cv = Self::cover_info(*id, base);
                drawn.push((&**t, [o.x, o.y, o.z, cell as f32, id.z as f32, cv[0], cv[1], cv[2], tess[0], tess[1], tess[2], tess[3]]));
            }
        }
        // near to far, so nearer geometry hides what is behind it before it is shaded.
        drawn.sort_by(|a, b| Vec3::new(a.1[0], a.1[1], a.1[2]).length_squared().total_cmp(&Vec3::new(b.1[0], b.1[1], b.1[2]).length_squared()));
        let mut tess = TessCells::default();
        let mut by_n: [Vec<u32>; 5] = Default::default();
        for (i, (t, u)) in drawn.iter().enumerate() {
            // (render.rs draws a block tessellated only if it has cells both ways)
            if u[10] >= 1.0 && u[11] >= 1.0 {
                tess_cells(&t.grid, Vec3::new(u[0], u[1], u[2]), u[3], [u[8], u[9], u[10], u[11]], self.frustum.as_ref(), &mut by_n);
                // Highest subdivision (nearest cells) first so early-Z hides farther cells behind ridges.
                for (k, cells) in by_n.iter_mut().enumerate().rev() {
                    if !cells.is_empty() {
                        tess.groups.push((i as u32, k as u32, tess.cells.len() as u32, cells.len() as u32));
                        tess.cells.append(cells);
                    }
                }
            }
        }
        let tiles: Vec<_> = drawn.into_iter().map(|(t, u)| (&t.gpu, u)).collect();
        let mut details = Vec::with_capacity(self.draw_b.len());
        let mut near_inst: Vec<TreeInst> = Vec::new();
        let mut tb = TreeBatcher::new();
        let casc_box: [(glam::Mat4, f32, f32); 2] = std::array::from_fn(|c| {
            let m = glam::Mat4::from_cols_array_2d(&casc_vp[c]);
            (m, m.row(0).truncate().length(), m.row(1).truncate().length())
        });
        let in_casc = |c: usize, p: Vec3, r: f32| {
            let (m, sx, sy) = casc_box[c];
            let q = m * p.extend(1.0);
            q.x.abs() <= 1.0 + r * sx && q.y.abs() <= 1.0 + r * sy
        };
        for (id, view) in self.draw_b.iter().zip(&self.in_view) {
            let Some(BSlot::Ready(b)) = self.builds.get(id) else { continue };
            let Some(g) = &b.gpu else { continue };
            let o = (b.center - cam).as_vec3();
            let dist = (o.length() as f64 - b.radius) as f32;
            let fdist = ((b.center - focus).length() - b.radius) as f32;
            let mut mask = 0u8;
            for (c, r) in casc.iter().enumerate() {
                if fdist < r * 1.45 {
                    mask |= 1 << c;
                }
            }
            // (how far out trees are drawn and where they become impostors: the graphics settings)
            let imp_dist = IMP_DIST * self.tree_imp_k;
            if dist < self.tree_far + 100.0 && g.trees.is_some() {
                for ch in &b.chunks {
                    let cp = b.center + ch.center.as_dvec3();
                    let rel = (cp - cam).as_vec3();
                    let dc = rel.length();
                    if dc - ch.radius > self.tree_far {
                        continue;
                    }
                    let seen = self.frustum.as_ref().map(|f| f.sphere(cp - cam, ch.radius as f64 + 25.0)).unwrap_or(true);
                    let fd = (cp - focus).length() as f32 - ch.radius;
                    // Exact orthographic XY box test per shadow cascade cuts ~39% of out-of-box chunks.
                    let sr = ch.radius + 10.0;
                    let cm = (fd < casc[0] * 1.5 && in_casc(0, rel, sr)) as u8;
                    let in_c1 = fd < casc[1] * 1.5 && in_casc(1, rel, sr);
                    if !seen && cm == 0 && !in_c1 {
                        continue;
                    }
                    if in_c1 {
                        // LOD 3 (20-tri icosphere) up to 850 m, area-matched LOD 4 (8-tri
                        // octahedron) beyond 850 m where cascade 1 texels are 0.73 m wide.
                        let lod = if dc - ch.radius >= 850.0 { LODS - 2 } else { C1_LOD };
                        for (variant, first, count) in &ch.runs {
                            tb.push(TreeDraw { mesh: *variant as u32 * LODS as u32 + lod as u32, first: *first, count: *count, view: false, casc: 2, close: false, imp: false, fixed: true, ring: 0 });
                        }
                    }
                    // Cascade 0 shadow models: keep full LOD 0 close to the camera (< 140 m)
                    // so nearby ground shadows preserve exact branch/skirt silhouettes; use
                    // LOD 1 (same 6 blobs, 4.5x fewer tris) from 140..260 m, LOD 2 beyond.
                    if cm != 0 {
                        let dmin = dc - ch.radius;
                        let lod = if dmin < 140.0 { 0 } else if dmin < 260.0 { 1 } else { 2 };
                        for (variant, first, count) in &ch.runs {
                            tb.push(TreeDraw { mesh: *variant as u32 * LODS as u32 + lod as u32, first: *first, count: *count, view: false, casc: cm, close: false, imp: false, fixed: true, ring: 0 });
                        }
                    }
                    // Further off, the trees are impostors, bucketed into distance rings so
                    // foreground crowns draw first and populate Hi-Z before distant ones.
                    if seen && dc + ch.radius >= imp_dist * (1.0 - LOD_JITTER) {
                        let ring = if dc < 260.0 { 0 } else if dc < 600.0 { 1 } else if dc < 950.0 { 2 } else if dc < 1450.0 { 3 } else if dc < 2150.0 { 4 } else { 5 };
                        for (variant, first, count) in &ch.runs {
                            tb.push(TreeDraw { mesh: *variant as u32, first: *first, count: *count, view: true, casc: 0, close: false, imp: true, fixed: false, ring });
                        }
                    }
                }
            }
            // Close to the camera: near trees (solid + leaf cards) and undergrowth.
            let mut near = Vec::new();
            if dist < CARD_RANGE && near_inst.len() < MAX_NEAR {
                let lp = -o;
                let seen = |c: Vec3, r: f32| self.frustum.as_ref().map(|f| f.sphere(c.as_dvec3(), r as f64)).unwrap_or(true);
                // Sorted into rings by distance so the nearest are drawn first and hide the rest.
                let ring = |d2: f32| if d2 < 20.0 * 20.0 { 0u16 } else if d2 < 45.0 * 45.0 { 256 } else if d2 < 80.0 * 80.0 { 512 } else { 768 };
                let mut found: Vec<(u16, TreeInst)> = Vec::new();
                for c in &b.cells {
                    let co = c.center - lp;
                    if co.length() - c.radius > CARD_RANGE || !seen(co, c.radius + 20.0) {
                        continue;
                    }
                    for i in &b.near_idx[c.trees.0 as usize..(c.trees.0 + c.trees.1) as usize] {
                        let t = &b.trees[*i as usize];
                        let to = Vec3::new(t.a[0], t.a[1], t.a[2]) - lp;
                        // Past IMP_DIST the tree is an impostor and has no cards.
                        if t.a[3] > 0.0 && to.length() < imp_dist * 1.01 * tree_jit(t.a) && seen(to, 26.0 * t.a[3]) {
                            found.push((ring(to.length_squared()) | b.tree_var[*i as usize] as u16, *t));
                        }
                    }
                    for u in &b.under[c.under.0 as usize..(c.under.0 + c.under.1) as usize] {
                        let uo = u.pos(c.center) - lp;
                        if uo.length_squared() > u.range() * u.range() || !seen(uo, 8.0) {
                            continue;
                        }
                        // Nothing grows in a fresh crater.
                        if self.craters.iter().any(|(cp, r)| ((*cp - cam).as_vec3() - uo).length_squared() < r * r * 1.5) {
                            continue;
                        }
                        found.push((ring(uo.length_squared()) | u.var as u16, u.inst(c.center)));
                    }
                }
                let cap = MAX_NEAR - near_inst.len();
                if found.len() > cap {
                    // Keep standing trees ahead of undergrowth if the near buffer fills up.
                    found.sort_unstable_by_key(|f| ((f.0 & 255) as usize / VARIANTS >= TREE_SPECIES, f.0));
                    found.truncate(cap);
                }
                found.sort_unstable_by_key(|f| f.0);
                let mut i = 0;
                while i < found.len() {
                    let (key, first) = (found[i].0, near_inst.len() as u32);
                    let var = key & 255;
                    while i < found.len() && found[i].0 == key {
                        near_inst.push(found[i].1);
                        i += 1;
                    }
                    let (mesh, count) = (var as u32 * LODS as u32, near_inst.len() as u32 - first);
                    let is_tree = (var as usize / VARIANTS) < TREE_SPECIES;
                    near.push(NearDraw { mesh, first, count, cards: false, tree: is_tree });
                    near.push(NearDraw { mesh: mesh + CARDS, first, count, cards: true, tree: is_tree });
                }
            }
            let fade = |t: f32, secs: f32| (t / secs).clamp(0.0, 1.0);
            let lod = [fade(b.mid_t - 0.3, 1.2), fade(b.fine_t - 0.6, 1.6), fade(b.shown, 1.0)];
            // How many height classes, tallest first, are near enough for their
            // full geometry, their middle level and their finest level.
            let ck = self.city_k;
            // How many height classes have every building inside the distance
            // where their full geometry, middle and finest levels start to
            // dissolve, with those levels done fading in: nothing of them is
            // cut away this frame, so they can be drawn without a discard.
            let maxd = o.length() + b.radius as f32;
            let inside = |r: f32| buildings::HCLASS.iter().take_while(|c| maxd < r * c.1 * ck).count() as u8;
            let settled = [inside(buildings::BFAR[0]), if lod[0] >= 1.0 { inside(buildings::BFAR[0]) } else { 0 }, if lod[1] >= 1.0 { inside(buildings::FINE_FADE) } else { 0 }];
            let reach = [buildings::BFAR[1], LOD_MID, LOD_FINE].map(|r| buildings::HCLASS.iter().take_while(|c| dist < r * c.1 * ck).count() as u8);
            details.push(DetailDraw {
                gpu: g,
                u: [o.x, o.y, o.z, 0.0, b.ax[0], b.ax[1], b.ax[2], b.ax[3], b.ay[0], b.ay[1], b.ay[2], b.ay[3], lod[0], lod[1], lod[2], 0.0],
                view: *view,
                near: dist < 3000.0,
                far: if o.length() + (b.radius as f32) < buildings::BFAR[0] * ck { 0 } else if reach[0] == 0 { 2 } else { 1 },
                reach,
                settled,
                radius: b.radius as f32,
                casc: mask,
                trees: tb.take(),
                close: near,
            });
        }
        let key = |d: &DetailDraw| Vec3::new(d.u[0], d.u[1], d.u[2]).length() - d.radius;
        details.sort_by(|a, b| key(a).total_cmp(&key(b)));
        (tiles, tess, details, near_inst)
    }
}

/// What character and vehicle physics need from the world. Test worlds can
/// implement it without a GPU or streamed tiles.
#[allow(dead_code)]
pub trait WorldQuery {
    fn surface_at(&self, p: DVec3) -> Option<SurfaceHit>;
    /// Correction that pushes a sphere out of a building (planet frame).
    fn building_push_sphere(&self, p: DVec3, radius: f64) -> Option<DVec3>;
    /// Correction that pushes an oriented box out of a building.
    fn building_push_box(&self, p: DVec3, half: [f64; 2], yaw: f64) -> Option<DVec3>;
    /// Correction that pushes a sphere out of a tree trunk.
    fn tree_push_sphere(&self, p: DVec3, radius: f64) -> Option<DVec3>;
    /// Correction that pushes an oriented box out of a tree trunk.
    fn tree_push_box(&self, p: DVec3, half: [f64; 2], yaw: f64) -> Option<DVec3>;
}

impl WorldQuery for World {
    fn surface_at(&self, p: DVec3) -> Option<SurfaceHit> {
        World::surface_at(self, p)
    }

    fn building_push_sphere(&self, p: DVec3, radius: f64) -> Option<DVec3> {
        let (_, _, alt) = geo::pos_to_lla(p);
        let (e, n) = enu_of(p);
        let mut best: Option<DVec3> = None;
        for (ring, base, top, bridge) in self.footprints_near(p, radius + 40.0) {
            if bridge || alt > top || alt + radius < base {
                continue;
            }
            if let Some(v) = push_circle(&ring, p, radius, e, n) {
                if best.is_none_or(|b| v.length() > b.length()) {
                    best = Some(v);
                }
            }
        }
        best
    }

    fn building_push_box(&self, p: DVec3, half: [f64; 2], yaw: f64) -> Option<DVec3> {
        let (_, _, alt) = geo::pos_to_lla(p);
        let (e, n) = enu_of(p);
        let radius = half[0].hypot(half[1]);
        let mut best: Option<DVec3> = None;
        for (ring, base, top, bridge) in self.footprints_near(p, radius + 40.0) {
            if bridge || alt > top || alt + 1.0 < base {
                continue;
            }
            if let Some(v) = obb_ring_push(&ring, p, half, yaw, e, n) {
                if best.is_none_or(|b| v.length() > b.length()) {
                    best = Some(v);
                }
            }
        }
        best
    }

    fn tree_push_sphere(&self, p: DVec3, radius: f64) -> Option<DVec3> {
        let (_, _, alt) = geo::pos_to_lla(p);
        let up = p.normalize();
        let mut trees = Vec::new();
        self.trees_near(p, radius + 40.0, &mut trees);
        let mut best: Option<DVec3> = None;
        for t in trees {
            let base = t.pos.length() - geo::R;
            if alt > base + t.height || alt + radius < base {
                continue;
            }
            let d = p - t.pos;
            let horiz = d - up * d.dot(up);
            let r = t.trunk_r + radius;
            let l = horiz.length();
            if l < r {
                let dir = if l > 1e-9 {
                    horiz / l
                } else {
                    let some = if up.cross(DVec3::X).length_squared() > 1e-6 { up.cross(DVec3::X) } else { up.cross(DVec3::Z) };
                    some.normalize()
                };
                let push = dir * (r - l);
                if best.is_none_or(|b| push.length() > b.length()) {
                    best = Some(push);
                }
            }
        }
        best
    }

    fn tree_push_box(&self, p: DVec3, half: [f64; 2], yaw: f64) -> Option<DVec3> {
        let (_, _, alt) = geo::pos_to_lla(p);
        let (e, n) = enu_of(p);
        let (hx, hz) = (half[0].max(0.0), half[1].max(0.0));
        let mut trees = Vec::new();
        self.trees_near(p, hx.hypot(hz) + 40.0, &mut trees);
        let mut best: Option<DVec3> = None;
        for t in trees {
            let base = t.pos.length() - geo::R;
            if alt > base + t.height || alt + 1.0 < base {
                continue;
            }
            if let Some(push) = tree_box_push(t.pos, t.trunk_r, p, half, yaw, e, n) {
                if best.is_none_or(|b| push.length() > b.length()) {
                    best = Some(push);
                }
            }
        }
        best
    }
}

/// Push an oriented box at `p` (half extents `half`, bearing `yaw`) out of the
/// vertical trunk at `trunk` with radius `trunk_r`. The push moves the box away
/// from the trunk; `e` and `n` are the east and north axes at `p`.
pub(crate) fn tree_box_push(trunk: DVec3, trunk_r: f64, p: DVec3, half: [f64; 2], yaw: f64, e: DVec3, n: DVec3) -> Option<DVec3> {
    let (c, s) = (yaw.cos(), yaw.sin());
    let (hx, hz) = (half[0].max(0.0), half[1].max(0.0));
    let d = trunk - p;
    let (x, z) = (d.dot(e), d.dot(n));
    // The trunk in the box's own frame: +x right, +z forward.
    let (bx, bz) = (c * x - s * z, s * x + c * z);
    let (qx, qz) = (bx.clamp(-hx, hx), bz.clamp(-hz, hz));
    // From the trunk towards its nearest point on the box: the way the box
    // must move to part from it.
    let (dx, dz) = (qx - bx, qz - bz);
    let dist = dx.hypot(dz);
    let (ux, uz, amount) = if dist > 1e-9 {
        if dist >= trunk_r {
            return None;
        }
        (dx / dist, dz / dist, trunk_r - dist)
    } else {
        // The trunk centre is inside the box: leave by the nearest face,
        // far enough for the radius to clear the box altogether.
        let (px, pz) = (hx - bx.abs(), hz - bz.abs());
        if px < pz {
            (-bx.signum(), 0.0, px + trunk_r)
        } else {
            (0.0, -bz.signum(), pz + trunk_r)
        }
    };
    let wx = c * ux + s * uz;
    let wz = -s * ux + c * uz;
    Some((e * wx + n * wz) * amount)
}

#[allow(dead_code)]
fn enu_of(p: DVec3) -> (DVec3, DVec3) {
    let (e, n, _) = geo::enu(p);
    (e, n)
}

/// Push a circle at `p` (radius `radius`) out of the horizontal polygon `ring`.
#[allow(dead_code)]
fn push_circle(ring: &[DVec3], p: DVec3, radius: f64, e: DVec3, n: DVec3) -> Option<DVec3> {
    if ring.len() < 3 {
        return None;
    }
    let local: Vec<glam::DVec2> = ring.iter().map(|q| { let d = *q - p; glam::DVec2::new(d.dot(e), d.dot(n)) }).collect();
    let mut inside = false;
    let (mut best_d, mut best_c) = (f64::INFINITY, glam::DVec2::ZERO);
    let mut j = local.len() - 1;
    for i in 0..local.len() {
        let (a, b) = (local[i], local[j]);
        if (a.y > 0.0) != (b.y > 0.0) && 0.0 < (b.x - a.x) * (0.0 - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        let d = b - a;
        let t = ((-a).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
        let c = a + d * t;
        let dist = c.length();
        if dist < best_d {
            (best_d, best_c) = (dist, c);
        }
        j = i;
    }
    if !inside && best_d >= radius {
        return None;
    }
    let (dir, amount) = if inside {
        if best_d < 1e-9 {
            return None;
        }
        (best_c / best_d, best_d + radius)
    } else {
        let dir = -best_c;
        let l = dir.length();
        if l < 1e-9 {
            return None;
        }
        (dir / l, radius - best_d)
    };
    Some((e * dir.x + n * dir.y) * amount)
}

/// Whether a simple 2D polygon is convex.
#[allow(dead_code)]
fn ring_is_convex(r: &[glam::DVec2]) -> bool {
    let n = r.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0f64;
    for i in 0..n {
        let a = r[i];
        let b = r[(i + 1) % n];
        let c = r[(i + 2) % n];
        let cr = (b - a).perp_dot(c - b);
        if cr.abs() > 1e-9 {
            if sign == 0.0 {
                sign = cr.signum();
            } else if cr * sign < 0.0 {
                return false;
            }
        }
    }
    sign != 0.0
}

#[allow(dead_code)]
fn point_in_poly_2d(r: &[glam::DVec2], p: glam::DVec2) -> bool {
    let mut inside = false;
    let mut j = r.len() - 1;
    for i in 0..r.len() {
        let (a, b) = (r[i], r[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[allow(dead_code)]
fn seg_intersects_aabb(a: glam::DVec2, b: glam::DVec2, hx: f64, hz: f64) -> bool {
    let d = b - a;
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-d.x, a.x + hx), (d.x, hx - a.x), (-d.y, a.y + hz), (d.y, hz - a.y)] {
        if p.abs() < 1e-12 {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 { return false; }
                t0 = t0.max(r);
            } else {
                if r < t0 { return false; }
                t1 = t1.min(r);
            }
        }
    }
    t0 <= t1
}

#[allow(dead_code)]
fn nearest_on_ring(r: &[glam::DVec2], p: glam::DVec2) -> glam::DVec2 {
    let mut best_d2 = f64::INFINITY;
    let mut best_q = p;
    for i in 0..r.len() {
        let (a, b) = (r[i], r[(i + 1) % r.len()]);
        let d = b - a;
        let t = ((p - a).dot(d) / d.length_squared().max(1e-12)).clamp(0.0, 1.0);
        let q = a + d * t;
        let d2 = (q - p).length_squared();
        if d2 < best_d2 {
            best_d2 = d2;
            best_q = q;
        }
    }
    best_q
}

/// Push an axis-aligned box `[-hx, hx] x [-hz, hz]` out of a concave 2D polygon `r`.
#[allow(dead_code)]
fn concave_aabb_push(r: &[glam::DVec2], hx: f64, hz: f64) -> Option<glam::DVec2> {
    let edge_hit = (0..r.len()).any(|i| seg_intersects_aabb(r[i], r[(i + 1) % r.len()], hx, hz));
    if !edge_hit && !point_in_poly_2d(r, glam::DVec2::ZERO) {
        return None;
    }
    let samples = [
        glam::DVec2::ZERO,
        glam::DVec2::new(-hx, -hz),
        glam::DVec2::new(hx, -hz),
        glam::DVec2::new(hx, hz),
        glam::DVec2::new(-hx, hz),
        glam::DVec2::new(-hx, 0.0),
        glam::DVec2::new(hx, 0.0),
        glam::DVec2::new(0.0, -hz),
        glam::DVec2::new(0.0, hz),
        glam::DVec2::new(-hx, -hz * 0.5),
        glam::DVec2::new(hx, -hz * 0.5),
        glam::DVec2::new(-hx, hz * 0.5),
        glam::DVec2::new(hx, hz * 0.5),
    ];
    let mut best = glam::DVec2::ZERO;
    let mut best_len = 0.0f64;
    for s in samples {
        if point_in_poly_2d(r, s) {
            let q = nearest_on_ring(r, s);
            let v = q - s;
            let l = v.length();
            if l > best_len {
                best_len = l;
                best = v;
            }
        }
    }
    if best_len > 1e-6 {
        return Some(best + best / best_len * 0.01);
    }
    // A sharp building corner poked into the side of the car between samples.
    let mut corner_push = None;
    let mut corner_d = f64::INFINITY;
    for &v in r {
        if v.x.abs() < hx && v.y.abs() < hz {
            let cands = [
                glam::DVec2::new(v.x - hx - 0.01, 0.0),
                glam::DVec2::new(v.x + hx + 0.01, 0.0),
                glam::DVec2::new(0.0, v.y - hz - 0.01),
                glam::DVec2::new(0.0, v.y + hz + 0.01),
            ];
            for cand in cands {
                let l = cand.length();
                if l < corner_d {
                    corner_d = l;
                    corner_push = Some(cand);
                }
            }
        }
    }
    corner_push
}

/// Push an oriented box at `p` out of the horizontal polygon `ring`.
/// Convex polygons use exact SAT; concave polygons (courtyards, L-shapes)
/// test against the real polygon boundary so empty notches stay driveable.
#[allow(dead_code)]
fn obb_ring_push(ring: &[DVec3], p: DVec3, half: [f64; 2], yaw: f64, e: DVec3, n: DVec3) -> Option<DVec3> {
    if ring.len() < 3 {
        return None;
    }
    let (c, s) = (yaw.cos(), yaw.sin());
    let (hx, hz) = (half[0].max(0.0), half[1].max(0.0));
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    let local: Vec<glam::DVec2> = ring.iter().map(|q| {
        let d = *q - p;
        let (x, z) = (d.dot(e), d.dot(n));
        let lp = glam::DVec2::new(c * x - s * z, s * x + c * z);
        min_x = min_x.min(lp.x);
        max_x = max_x.max(lp.x);
        min_y = min_y.min(lp.y);
        max_y = max_y.max(lp.y);
        lp
    }).collect();
    if max_x <= -hx || min_x >= hx || max_y <= -hz || min_y >= hz {
        return None;
    }
    let out_local = if ring_is_convex(&local) {
        let mut axes: Vec<glam::DVec2> = vec![glam::DVec2::X, glam::DVec2::Y];
        for i in 0..local.len() {
            let (a, b) = (local[i], local[(i + 1) % local.len()]);
            let d = b - a;
            if d.length_squared() > 1e-12 {
                axes.push(glam::DVec2::new(-d.y, d.x).normalize());
            }
        }
        let centre = local.iter().fold(glam::DVec2::ZERO, |a, q| a + *q) / local.len() as f64;
        let (mut overlap, mut axis_out) = (f64::INFINITY, glam::DVec2::X);
        for axis in axes {
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for q in &local {
                let d = q.dot(axis);
                lo = lo.min(d);
                hi = hi.max(d);
            }
            let be = hx * axis.x.abs() + hz * axis.y.abs();
            let o = hi.min(be) - lo.max(-be);
            if o <= 0.0 {
                return None;
            }
            if o < overlap {
                overlap = o;
                let sign = if (-centre).dot(axis) >= 0.0 { 1.0 } else { -1.0 };
                axis_out = axis * sign;
            }
        }
        axis_out * overlap
    } else {
        concave_aabb_push(&local, hx, hz)?
    };
    let wx = c * out_local.x + s * out_local.y;
    let wz = -s * out_local.x + c * out_local.y;
    Some(e * wx + n * wz)
}

/// Model the second shadow cascade draws trees with, at the least: 20-triangle
/// icosphere crown (no sub-texel trunk), matching full crown shadow area at 0.73 m/texel.
const C1_LOD: usize = 3;

/// Collects a tile's tree batches, joining each to the last one drawn the same
/// way when its instances follow straight on (a tile's trees are sorted by
/// variant, then chunk, and chunks are visited in order), so a stretch of
/// neighbouring chunks costs one draw instead of one each.
struct TreeBatcher {
    /// index + 1 into `out` of the open batch for each way of drawing (0: none)
    open: Vec<u32>,
    touched: Vec<usize>,
    out: Vec<TreeDraw>,
}
impl TreeBatcher {
    fn new() -> Self {
        Self { open: vec![0; 64 * 512], touched: Vec::new(), out: Vec::new() }
    }
    fn push(&mut self, t: TreeDraw) {
        let flags = if t.imp {
            1 | (((t.ring as usize) & 7) << 1)
        } else {
            ((t.fixed as usize) << 1) | ((t.view as usize) << 2) | ((t.close as usize) << 3) | (((t.casc as usize) & 3) << 4)
        };
        let k = flags * 512 + (t.mesh as usize).min(511);
        let o = self.open[k] as usize;
        if o > 0 {
            let p = &mut self.out[o - 1];
            if p.mesh == t.mesh && p.ring == t.ring && p.first + p.count == t.first {
                p.count += t.count;
                return;
            }
        } else {
            self.touched.push(k);
        }
        self.out.push(t);
        self.open[k] = self.out.len() as u32;
    }
    /// The batches so far, ready for the next tile.
    fn take(&mut self) -> Vec<TreeDraw> {
        for k in self.touched.drain(..) {
            self.open[k] = 0;
        }
        std::mem::take(&mut self.out)
    }
}

trait Tag {
    fn with_tag(self) -> Self;
    fn with_refresh(self) -> Self;
}
impl Tag for TileId {
    /// Marks a request as a building tile inside the shared request list.
    fn with_tag(self) -> Self {
        TileId::new(self.z | 0x80, self.x, self.y)
    }
    /// Marks a request to remake a loaded terrain tile (new imagery source).
    fn with_refresh(self) -> Self {
        TileId::new(self.z | 0x40, self.x, self.y)
    }
}

/// Which way a windsock hangs: downwind (`hd`, horizontal), drooping in a
/// light breeze and stirring a little; `k` staggers neighbouring socks.
pub fn sock_dir(hd: Vec3, up: Vec3, wind_kt: f32, t: f64, k: usize) -> Vec3 {
    let wob = (t as f32 * (2.3 + k as f32 * 0.4)).sin() * 0.05 * (wind_kt / 10.0).min(1.5);
    let droop = (1.0 - (wind_kt / 15.0).clamp(0.0, 1.0)) * 1.25 + wob;
    let fwd = hd * droop.cos() - up * droop.sin();
    (fwd + hd.cross(up) * wob).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn flat_world() -> World {
        let src = Arc::new(crate::sources::Sources::new(crate::fetch::Fetcher::new(std::env::temp_dir().join("gtw-world-surface-test"), true), String::new()));
        let loader = crate::sources::Loader::new(src, 0);
        let mut w = World::new(loader, 14);
        w.flat = Some(0.0);
        w
    }

    #[test]
    fn flat_world_is_grass_at_zero() {
        let w = flat_world();
        let p = geo::lla_to_pos(0.0, 0.0, 0.0);
        let hit = w.surface_at(p).unwrap();
        assert_eq!(hit.kind, Surface::Grass);
        assert!(hit.h.abs() < 1e-9);
        assert!(!hit.water && !hit.road && !hit.sidewalk && !hit.bridge);
        assert!(!hit.blocked());
        assert_eq!(w.surface(0.0, 0.0).unwrap().kind, Surface::Grass);
        assert_eq!(w.terrain_ground(0.0, 0.0).unwrap().h, 0.0);
    }

    #[test]
    fn world_query_trait_delegates_to_the_world() {
        let w = flat_world();
        let p = geo::lla_to_pos(0.0, 0.0, 0.0);
        assert_eq!(WorldQuery::surface_at(&w, p).unwrap().kind, Surface::Grass);
        assert!(WorldQuery::building_push_sphere(&w, p, 0.3).is_none());
        assert!(WorldQuery::building_push_box(&w, p, [1.0, 0.5], 0.0).is_none());
        assert!(WorldQuery::tree_push_sphere(&w, p, 0.3).is_none());
        assert!(WorldQuery::tree_push_box(&w, p, [1.0, 0.5], 0.0).is_none());
    }

    #[test]
    fn tree_box_push_moves_the_box_away_from_the_trunk() {
        let p = origin();
        let (e, n, _) = geo::enu(p);
        let half = [0.9, 2.2];
        let push = |at: DVec3, r: f64| tree_box_push(p + at, r, p, half, 0.0, e, n);
        // A trunk just ahead of the front bumper pushes the box back.
        let v = push(n * 2.4, 0.25).expect("the trunk touches the front");
        assert!(v.dot(n) < -0.04 && v.dot(e).abs() < 1e-9, "backwards: {v:?}");
        // A trunk beside the right-hand side pushes the box left.
        let v = push(e * 1.0, 0.25).expect("the trunk touches the side");
        assert!(v.dot(e) < -0.14 && v.dot(n).abs() < 1e-9, "to the left: {v:?}");
        // A trunk behind the rear bumper pushes the box forwards, not into it:
        // a car with a tree at its tail must still be able to drive away.
        let v = push(e * -0.5 + n * -2.35, 0.25).expect("the trunk touches the tail");
        assert!(v.dot(n) > 0.09 && v.dot(e).abs() < 0.01, "forwards: {v:?}");
        // The trunk centre inside the box leaves by the nearest face, clear of
        // its radius.
        let v = push(e * 0.5, 0.25).expect("the trunk is inside");
        assert!((v.dot(e) + 0.65).abs() < 1e-9 && v.dot(n).abs() < 1e-9, "out the right face: {v:?}");
        let v = push(e * -0.5 + n * -2.0, 0.25).expect("the trunk is inside");
        assert!((v.dot(n) - 0.45).abs() < 1e-9 && v.dot(e).abs() < 1e-9, "out the back: {v:?}");
        // The same holds with the box at a bearing.
        let yaw = std::f64::consts::FRAC_PI_4;
        let fwd = e * yaw.sin() + n * yaw.cos();
        let right = e * yaw.cos() - n * yaw.sin();
        let v = tree_box_push(p + fwd * 2.4, 0.25, p, half, yaw, e, n).expect("contact at 45 degrees");
        assert!(v.dot(fwd) < -0.04 && v.dot(right).abs() < 1e-9, "back along the box's axis: {v:?}");
        // Clear of the trunk: no correction.
        assert!(push(n * 2.6, 0.25).is_none());
        assert!(push(e * 1.2, 0.25).is_none());
    }

    #[test]
    fn a_body_on_a_bridge_takes_the_deck_and_one_under_it_the_street() {
        // A ground street with a bridge deck directly over it, as at an
        // overpass. The surface query must use the body's altitude: on the
        // deck it takes the deck, underneath it the street below.
        let mut w = flat_world();
        let id = TileId::at(0.0, 0.0, DATA_ZOOM);
        let (clat, clon) = id.lat_lon(0.5, 0.5);
        let center = geo::lla_to_pos(clat, clon, 0.0);
        let ext = 1024.0f32;
        let unit_m = (id.width_m() / ext as f64) as f32;
        let path = |alt: f32, bridge: bool| crate::roads::Path {
            pts: Vec::new(),
            cum: vec![0.0, 120.0],
            len: 120.0,
            class: crate::sources::RC_MINOR,
            oneway: false,
            lanes: 1,
            lane_w: 3.0,
            park_off: 0.0,
            speed: 8.0,
            hidden: false,
            drive_half: 5.0,
            walk: false,
            bridge,
            local: vec![[ext * 0.5 - 60.0 / unit_m, ext * 0.5], [ext * 0.5 + 60.0 / unit_m, ext * 0.5]],
            alt: vec![alt, alt],
            nodes: [0, 0],
            edge: [0, 0],
            junctions: Vec::new(),
            keep_clear: Vec::new(),
        };
        let net = RoadNet { paths: vec![path(0.0, false), path(8.0, true)], ..Default::default() };
        let road_index = RoadIndex::build(&net, ext, unit_m);
        let tile = BTile { center, radius: 500.0, gpu: None, footprints: Vec::new(), last_used: 0, net, road_index, cars: Vec::new(), parked: Vec::new(), trees: Vec::new(), tree_var: Vec::new(), chunks: Vec::new(), under: Vec::new(), cells: Vec::new(), near_idx: Vec::new(), ax: [0.0; 4], ay: [0.0; 4], shown: 1.0, mid_t: 1.0, fine_t: 1.0, tall: 0.0 };
        w.builds.insert(id, BSlot::Ready(Box::new(tile)));
        let up = center.normalize();
        let street = w.surface_at(center).unwrap();
        assert!(street.road && !street.bridge, "the street: {street:?}");
        assert!(street.h.abs() < 1e-6, "at the street's height: {:.2}", street.h);
        let deck = w.surface_at(center + up * 8.0).unwrap();
        assert!(deck.road && deck.bridge, "the deck: {deck:?}");
        assert!((deck.h - 8.0).abs() < 1e-6, "at the deck's height: {:.2}", deck.h);
    }

    #[test]
    fn parked_cars_are_selected_removed_and_faced() {
        let center = geo::lla_to_pos(0.0, 0.0, 0.0);
        let (e, n, up) = geo::enu(center);
        let mk = |at: DVec3, model| Parked { pos: (at - center).as_vec3(), fwd: n.as_vec3(), up: up.as_vec3(), model, col: [0.5; 3], shift: Vec3::ZERO, bump: Vec3::ZERO };
        let mut list = vec![mk(center + e * 2.0, 0), mk(center + e * 5.0, 3), mk(center + n * 1.0, 5)];
        let p = center + e * 1.8;
        let i = nearest_parked(center, &list, p, 3.0).expect("one is in reach");
        assert_eq!(list[i].model, 0, "the nearest, not the first in reach");
        let taken = list.swap_remove(i);
        assert_eq!(taken.model, 0);
        assert_eq!(list.len(), 2, "only the taken one goes");
        assert!(nearest_parked(center, &list, p, 3.0).is_some(), "neighbours stay");
        assert!(nearest_parked(center, &list, center + n * 20.0, 1.0).is_none());
        // The prop's facing becomes a bearing: north 0, east a quarter turn.
        assert!(bearing_of(center, n.as_vec3()).abs() < 1e-9);
        assert!((bearing_of(center, e.as_vec3()) - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        // A world with no detail tile loaded has nothing to take.
        let mut w = flat_world();
        assert!(w.nearest_parked_car(p, 10.0).is_none());
        assert!(w.take_parked_car(p, 10.0).is_none());
    }

    // ------------------------------------------------------------- traffic

    fn origin() -> DVec3 {
        geo::lla_to_pos(0.0, 0.0, 0.0)
    }

    /// An eastbound road through `origin`, facing east.
    fn east() -> DVec3 {
        geo::enu(origin()).0
    }

    const EAST: f64 = std::f64::consts::FRAC_PI_2;

    /// A flat world with one test car on a straight road through the origin,
    /// facing east at `speed`.
    fn traffic_world(speed: f32) -> World {
        let mut w = flat_world();
        assert!(w.spawn_test_traffic(origin(), EAST, speed));
        w
    }

    /// Put the test car `ahead` metres east of `origin` along its road.
    fn place_car(w: &mut World, ahead: f64) {
        let id = w.test_traffic.expect("test tile");
        let Some(BSlot::Ready(b)) = w.builds.get_mut(&id) else { panic!("test tile ready") };
        b.cars[0].s = 60.0 + ahead as f32;
    }

    fn test_car(w: &World) -> &Car {
        let id = w.test_traffic.expect("test tile");
        let Some(BSlot::Ready(b)) = w.builds.get(&id) else { panic!("test tile ready") };
        &b.cars[0]
    }

    fn walk_presence() -> PlayerPresence {
        PlayerPresence { pos: origin(), driving: false }
    }

    fn drive_presence() -> PlayerPresence {
        PlayerPresence { pos: origin(), driving: true }
    }

    fn run_traffic(w: &mut World, secs: f64, player: Option<PlayerPresence>) {
        let n = (secs * 60.0).round() as usize;
        for _ in 0..n {
            w.update_traffic(1.0 / 60.0, origin(), player);
        }
    }

    #[test]
    fn traffic_brakes_for_a_walking_and_for_a_driving_player() {
        // A car 27 m away, driving at the player: with someone on foot in the
        // road it brakes inside 12 m and stops short of them.
        let mut w = traffic_world(8.0);
        place_car(&mut w, -27.0);
        run_traffic(&mut w, 4.0, Some(walk_presence()));
        let car = test_car(&w);
        assert!(car.v < 0.2, "the car stopped: {:.2} m/s", car.v);
        let id = w.test_traffic.unwrap();
        let Some(BSlot::Ready(b)) = w.builds.get(&id) else { panic!() };
        let up = b.center.normalize().as_vec3();
        let (rel, _) = cars::pose(&b.net, car, up);
        let d = (b.center + rel.as_dvec3() - origin()).length();
        assert!(d < PlayerPresence::WALK_RADIUS, "stopped within the yield radius: {d:.1} m");
        assert!(d > 1.0, "did not drive over the player: {d:.1} m");

        // The same car with nobody about keeps its speed.
        let mut w = traffic_world(8.0);
        place_car(&mut w, -27.0);
        run_traffic(&mut w, 1.0, None);
        assert!(test_car(&w).v > 6.0, "unhindered traffic keeps moving: {:.2}", test_car(&w).v);

        // Inside the driving radius but outside the walking one, a driver makes
        // the car slow; a walker at the same distance does not.
        let mut walk = traffic_world(8.0);
        place_car(&mut walk, -15.0);
        run_traffic(&mut walk, 0.25, Some(walk_presence()));
        let mut drive = traffic_world(8.0);
        place_car(&mut drive, -15.0);
        run_traffic(&mut drive, 0.25, Some(drive_presence()));
        assert!(test_car(&drive).v < test_car(&walk).v - 0.5, "traffic slows for a car 15 m off ({:.2}) but not for a walker ({:.2})", test_car(&drive).v, test_car(&walk).v);
    }

    #[test]
    fn traffic_holds_for_a_carjack_and_resumes_when_the_player_leaves() {
        let mut w = traffic_world(6.0);
        place_car(&mut w, -5.0);
        run_traffic(&mut w, 2.0, Some(walk_presence()));
        assert!(test_car(&w).v < 0.2, "held at a standstill: {:.2}", test_car(&w).v);
        assert!(test_car(&w).yield_t > 0.0, "in its yield state");
        // The player walks off: after the hold expires the car pulls away again.
        run_traffic(&mut w, 1.0, None);
        assert!(test_car(&w).v < 0.2, "still holding through the short hold");
        run_traffic(&mut w, 5.0, None);
        assert!(test_car(&w).v > 5.5, "back to normal speed: {:.2}", test_car(&w).v);
    }

    #[test]
    fn taking_a_traffic_car_removes_exactly_one_and_keeps_its_pose() {
        let mut w = traffic_world(9.0);
        place_car(&mut w, -17.0);
        let id = w.test_traffic.unwrap();
        if let Some(BSlot::Ready(b)) = w.builds.get_mut(&id) {
            // A second car, well out of reach, proves only one is taken.
            b.cars.push(cars::placed(0, 85.0, 5.0, 3, [0.2, 0.3, 0.4]));
            b.cars[0].col = [0.1, 0.5, 0.2];
        }
        let taken = w.take_traffic_car(origin(), 20.0).expect("one in reach");
        assert_eq!(taken.model, 0);
        assert_eq!(taken.colour, [0.1, 0.5, 0.2]);
        assert!((taken.pos - (origin() - east() * 17.0)).length() < 0.5, "its pose: {:.2} m off", (taken.pos - (origin() - east() * 17.0)).length());
        assert!((taken.yaw - EAST).abs() < 0.15, "facing east: {:.2}", taken.yaw);
        assert!((taken.velocity.length() - 9.0).abs() < 0.3, "its speed comes with it: {:.2}", taken.velocity.length());
        assert!((taken.velocity.dot(east()) - 9.0).abs() < 0.3, "and its direction");
        let Some(BSlot::Ready(b)) = w.builds.get(&id) else { panic!() };
        assert_eq!(b.cars.len(), 1, "exactly one left the lane");
        assert_eq!(b.cars[0].model, 3, "the neighbour stays");
        // It cannot be taken twice, and nothing else is in reach.
        assert!(w.take_traffic_car(origin(), 20.0).is_none());
        assert!(w.nearest_traffic_car(origin(), 20.0).is_none());
    }

    #[test]
    fn taking_a_parked_car_removes_one_and_leaves_the_neighbours() {
        let mut w = flat_world();
        let p = origin();
        assert!(w.spawn_test_traffic(origin(), EAST, 0.0));
        let (e, n, up) = geo::enu(p);
        let id = w.test_traffic.unwrap();
        let center = match w.builds.get(&id) {
            Some(BSlot::Ready(b)) => b.center,
            _ => panic!("test tile ready"),
        };
        let mk = |at: DVec3, model| Parked { pos: (at - center).as_vec3(), fwd: n.as_vec3(), up: up.as_vec3(), model, col: [0.5; 3], shift: Vec3::ZERO, bump: Vec3::ZERO };
        if let Some(BSlot::Ready(b)) = w.builds.get_mut(&id) {
            b.parked = vec![mk(p + e * 1.0, 0), mk(p + e * 3.0, 3), mk(p - e * 2.0, 5)];
        }
        let taken = w.take_parked_car(p + e * 0.9, 2.0).expect("one in reach");
        assert_eq!(taken.model, 0);
        assert_eq!(taken.velocity, DVec3::ZERO, "a parked car is at rest");
        let Some(BSlot::Ready(b)) = w.builds.get(&id) else { panic!() };
        assert_eq!(b.parked.len(), 2, "only the taken prop goes");
        let mut left: Vec<u8> = b.parked.iter().map(|pk| pk.model).collect();
        left.sort_unstable();
        assert_eq!(left, vec![3, 5], "the neighbours stay");
        // Out of range finds nothing.
        assert!(w.take_parked_car(p + e * 40.0, 2.0).is_none());
    }

    #[test]
    fn carjacking_is_refused_when_the_tile_is_not_ready_or_nothing_is_in_reach() {
        let p = origin();
        // No detail tile at all: nothing to take, however close it feels.
        let mut w = flat_world();
        assert!(w.nearest_traffic_car(p, 50.0).is_none());
        assert!(w.take_traffic_car(p, 50.0).is_none());
        assert!(w.take_parked_car(p, 50.0).is_none());
        // A tile still loading owns no cars the simulation can see.
        let mut w = flat_world();
        let id = TileId::at(geo::pos_to_lla(p).0, geo::pos_to_lla(p).1, DATA_ZOOM);
        w.builds.insert(id, BSlot::Loading);
        assert!(w.nearest_traffic_car(p, 50.0).is_none());
        assert!(w.take_traffic_car(p, 50.0).is_none());
        // A ready tile, but the only car is out of reach.
        let mut w = traffic_world(4.0);
        assert!(w.nearest_traffic_car(p, 2.0).is_none());
        assert!(w.take_traffic_car(p, 2.0).is_none());
        let near = w.nearest_traffic_car(p, 3.2).expect("in talk reach");
        assert!((near.pos - p).length() < 3.2, "distance {:.2}", (near.pos - p).length());
        assert!(w.nearest_traffic_car(p, 5.0).is_some());
    }

    #[test]
    fn a_hit_shoves_the_other_car_and_its_parked_neighbour() {
        let mut w = traffic_world(6.0);
        place_car(&mut w, 0.0);
        let mut out = Vec::new();
        w.traffic_bodies_near(origin(), 20.0, &mut out);
        let moving = *out.iter().find(|b| b.kind == TrafficKind::Moving).expect("one moving car");
        let before = test_car(&w).shift;
        w.hit_traffic(&moving, 8.0, east() * 0.4, east() * 3.0);
        let car = test_car(&w);
        assert!((car.shift - before).dot(east().as_vec3()) > 0.3, "the moving car is shoved: {:?}", car.shift);
        assert!(car.bump.dot(east().as_vec3()) > 2.0, "and carries the impulse: {:?}", car.bump);
        // The shove is integrated by the traffic step, not just drawn once.
        let shoved = car.shift;
        run_traffic(&mut w, 0.2, None);
        assert!(test_car(&w).shift.dot(east().as_vec3()) > shoved.dot(east().as_vec3()) + 0.05, "it moves on");

        // A parked car rolls with the hit too.
        let id = w.test_traffic.unwrap();
        let center = match w.builds.get(&id) {
            Some(BSlot::Ready(b)) => b.center,
            _ => panic!("test tile ready"),
        };
        let (e, n, up) = geo::enu(origin());
        if let Some(BSlot::Ready(b)) = w.builds.get_mut(&id) {
            b.parked.push(Parked { pos: (origin() + e * 2.0 - center).as_vec3(), fwd: n.as_vec3(), up: up.as_vec3(), model: 0, col: [0.5; 3], shift: Vec3::ZERO, bump: Vec3::ZERO });
        }
        out.clear();
        w.traffic_bodies_near(origin(), 20.0, &mut out);
        let parked = *out.iter().find(|b| b.kind == TrafficKind::Parked).expect("one parked car");
        w.hit_traffic(&parked, 5.0, east() * 0.25, east() * 2.0);
        let Some(BSlot::Ready(b)) = w.builds.get(&id) else { panic!("test tile ready") };
        assert!(b.parked[0].shift.dot(east().as_vec3()) > 0.2, "a parked car is shoved: {:?}", b.parked[0].shift);
        assert!(b.parked[0].bump.dot(east().as_vec3()) > 1.0, "and carries the impulse: {:?}", b.parked[0].bump);
    }

    #[test]
    fn traffic_bodies_are_oriented_boxes_with_the_cars_velocity() {
        let p = origin();
        let mut w = traffic_world(7.0);
        place_car(&mut w, -12.0);
        let mut out = Vec::new();
        w.traffic_bodies_near(p, 20.0, &mut out);
        assert_eq!(out.len(), 1, "one moving car");
        let b = out[0];
        assert_eq!(b.kind, TrafficKind::Moving);
        assert_eq!(b.model, 0);
        assert!((b.pos - (p - east() * 12.0)).length() < 0.5);
        assert!((b.yaw - EAST).abs() < 0.15, "facing east: {:.2}", b.yaw);
        assert!((b.velocity.dot(east()) - 7.0).abs() < 0.3);
        assert!((b.half[0] - cars::player_geom(0).half[0]).abs() < 1e-9 && (b.half[1] - cars::player_geom(0).half[1]).abs() < 1e-9);
        // Far away it is not returned.
        out.clear();
        w.traffic_bodies_near(p + east() * 400.0, 10.0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn a_close_moving_car_is_drawn_even_when_the_traffic_share_is_low() {
        // Late at night only a share of the moving cars is drawn by rank, but
        // every one of them is solid and yields to the player. A car close
        // enough to reach the player must therefore stay visible, or it is an
        // invisible obstacle: the user drives up to a car that is not drawn
        // and is stopped by it.
        let mut w = traffic_world(5.0);
        let id = w.test_traffic.expect("test tile");
        let Some(BSlot::Ready(b)) = w.builds.get_mut(&id) else { panic!("test tile ready") };
        b.cars[0].rank = 0.99;
        w.draw_b.push(id);
        // Keep the far-draw limit out of the test's way.
        w.cars = (1.0, 10_000.0, 10_000.0);
        let mut out = Vec::new();
        let mut lights = Vec::new();
        w.car_instances(origin(), 0.1, 1.0, &mut out, &mut lights);
        assert_eq!(out.len(), 1, "the car next to the player is drawn");
        out.clear();
        w.car_instances(origin() + east() * 800.0, 0.1, 1.0, &mut out, &mut lights);
        assert!(out.is_empty(), "an 800 m car is still thinned out by rank");
    }

    #[test]
    fn cars_that_are_not_drawn_are_not_solid() {
        // The cars setting can be Off: car_instances draws nothing at all.
        // traffic_bodies_near must agree, or an invisible traffic wall stops
        // the player.
        let mut w = traffic_world(5.0);
        let mut out = Vec::new();
        w.traffic_bodies_near(origin(), 20.0, &mut out);
        assert_eq!(out.len(), 1, "traffic is solid with the cars setting on");
        w.cars.0 = 0.0;
        w.traffic_bodies_near(origin(), 20.0, &mut out);
        assert!(out.is_empty(), "with the cars setting off, traffic is not solid");
    }

    #[test]
    fn obb_and_tree_collisions_respect_45_deg_yaw_and_concave_notches() {
        let p = origin();
        let (e, n, _) = geo::enu(p);
        let yaw = std::f64::consts::FRAC_PI_4; // facing NE
        let fwd = e * yaw.sin() + n * yaw.cos();
        let right = e * yaw.cos() - n * yaw.sin();
        let half = [1.0, 2.5]; // 2m wide, 5m long

        // A small box directly ahead at +2.0m along `fwd` must collide;
        // a box at +2.0m along `right` (beyond half-width 1.0m) must NOT collide.
        let mk_box = |c: DVec3, r: f64| vec![
            c - e * r - n * r,
            c + e * r - n * r,
            c + e * r + n * r,
            c - e * r + n * r,
        ];
        assert!(obb_ring_push(&mk_box(p + fwd * 2.0, 0.2), p, half, yaw, e, n).is_some(), "nose hits at 45 deg");
        assert!(obb_ring_push(&mk_box(p + right * 2.0, 0.2), p, half, yaw, e, n).is_none(), "side is clear at 45 deg");

        // Concave L-shaped building: notch in (+e, +n) quadrant [2..12, 2..12].
        let l_ring = vec![
            p + e * 0.0 + n * 0.0,
            p + e * 12.0 + n * 0.0,
            p + e * 12.0 + n * 2.0,
            p + e * 2.0 + n * 2.0,
            p + e * 2.0 + n * 12.0,
            p + e * 0.0 + n * 12.0,
        ];
        // Parked in the open courtyard notch at (6, 6): must NOT collide!
        assert!(obb_ring_push(&l_ring, p + e * 6.0 + n * 6.0, half, 0.0, e, n).is_none(), "courtyard notch is open");
        // Touching the actual wing wall at (1.5, 6): MUST push out east!
        let push = obb_ring_push(&l_ring, p + e * 1.5 + n * 6.0, half, 0.0, e, n).expect("wing wall pushes");
        assert!(push.dot(e) > 0.4, "pushed out of wing toward courtyard: {:?}", push);
    }
}
