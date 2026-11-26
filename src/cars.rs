//! Traffic: procedurally modelled vehicles and a light simulation that drives
//! them along the road graph of each detail tile.
use crate::buildings::hash;
use crate::render::MeshVertex;
use crate::roads::{car_model, paint, near_keys, Link, RoadNet};
use glam::Vec3;

pub const N_MODELS: usize = 8;
pub const CAR_LEN: [f32; N_MODELS] = [4.7, 4.0, 4.7, 5.4, 5.2, 4.4, 11.5, 7.6];

use crate::carmesh::{self, MB};

/// A player car's meshes and the local positions of its four wheels.
pub struct PlayerModel {
    pub body: (Vec<MeshVertex>, Vec<u32>),
    pub wheel: (Vec<MeshVertex>, Vec<u32>),
    /// Front-left, front-right, rear-left, rear-right, in the body frame
    /// (x right, y up, z back, the origin at the ground under the car).
    pub wheels: [[f32; 3]; 4],
    #[allow(dead_code)]
    pub wheel_r: f32,
    pub len: f32,
    pub width: f32,
}

/// What the physics and the renderer need from a player car's model.
#[derive(Copy, Clone, Debug)]
pub struct CarGeom {
    pub wheels: [[f32; 3]; 4],
    /// Half length and half width for the collision box.
    pub half: [f64; 2],
}

impl PlayerModel {
    pub fn geom(&self) -> CarGeom {
        CarGeom { wheels: self.wheels, half: [self.width as f64 * 0.5, self.len as f64 * 0.5] }
    }
}

fn wheel_spots(s: &carmesh::Spec) -> [[f32; 3]; 4] {
    let (front, rear) = (s.axles[0], *s.axles.last().unwrap());
    let (f, r) = (s.wheel_x(front), s.wheel_x(rear));
    let wz = |af: f32| (af - 0.5) * s.len;
    let (zf, zr) = (wz(front), wz(rear));
    [[-f, s.wheel_r, zf], [f, s.wheel_r, zf], [-r, s.wheel_r, zr], [r, s.wheel_r, zr]]
}

/// The eight player cars: a body mesh without wheels, one wheel mesh (a rim
/// on both faces, as it is mounted on either side), and the four wheel
/// positions. The traffic meshes (`build_models`) are the same bodies with
/// their wheels built in.
pub fn player_models() -> Vec<PlayerModel> {
    carmesh::specs()
        .iter()
        .map(|s| {
            let b = carmesh::body(s, false);
            let w = carmesh::wheel(s, [true, true], false);
            PlayerModel { body: (b.v, b.i), wheel: (w.v, w.i), wheels: wheel_spots(s), wheel_r: s.wheel_r, len: s.len, width: 2.0 * (s.w + 0.05) }
        })
        .collect()
}

/// The geometry of one player car model, for the physics and the wheels.
pub fn player_geom(model: u8) -> CarGeom {
    let s = &carmesh::specs()[model.min(N_MODELS as u8 - 1) as usize];
    CarGeom { wheels: wheel_spots(s), half: [(s.w + 0.05) as f64, s.len as f64 * 0.5] }
}

pub fn build_models() -> Vec<(Vec<MeshVertex>, Vec<u32>)> {
    carmesh::specs().iter().map(|s| carmesh::traffic(s, false)).map(|m| (m.v, m.i)).collect()
}

/// The traffic models with fewer stations, plain wheels and no small parts,
/// for cars beyond `FAR_LOD` metres.
pub fn build_far_models() -> Vec<(Vec<MeshVertex>, Vec<u32>)> {
    carmesh::specs().iter().map(|s| carmesh::traffic(s, true)).map(|m| (m.v, m.i)).collect()
}

/// A lumpy boulder for terrain debris.
pub fn rock(seed: u32) -> (Vec<MeshVertex>, Vec<u32>) {
    let (vs, fs) = crate::trees::ico(1);
    let pts: Vec<Vec3> = vs
        .iter()
        .enumerate()
        .map(|(k, d)| {
            let q = (*d * 2.2).round() / 2.2;
            (*d * 0.55 + q * 0.45) * (0.75 + 0.5 * hash(k as u32, seed)) * Vec3::new(1.0, 0.7 + 0.2 * hash(seed, 9), 0.85)
        })
        .collect();
    let mut m = MB::default();
    for f in &fs {
        let sh = 0.7 + 0.5 * hash(f[0] + f[1] * 31, seed);
        m.poly(&[pts[f[0] as usize], pts[f[1] as usize], pts[f[2] as usize]], [sh, sh, sh, 0.0]);
    }
    m.smooth(0.75);
    (m.v, m.i)
}

pub fn cube() -> (Vec<MeshVertex>, Vec<u32>) {
    let mut m = MB::default();
    m.cuboid(Vec3::splat(-1.0), Vec3::splat(1.0), [1.0, 1.0, 1.0, 0.0]);
    (m.v, m.i)
}

pub const M_CUBE: u32 = N_MODELS as u32;
pub const M_ROCK: u32 = N_MODELS as u32 + 1;
/// broken concrete (destruct::slab_meshes), after the cube and three rocks
pub const M_SLAB: u32 = N_MODELS as u32 + 4;
/// reinforcing bar sticking out of broken concrete (destruct::rebar_meshes)
pub const M_REBAR: u32 = M_SLAB + crate::destruct::N_SLAB;
pub const N_REBAR: u32 = 2;

/// The player car meshes follow the avatar in the instanced-solid library:
/// each model's body then its wheel, in model order.
pub const PLAYER_BASE: u32 = crate::avatar::BASE + crate::avatar::PARTS as u32;
/// The far traffic models follow the player cars.
pub const FAR_BASE: u32 = PLAYER_BASE + 2 * N_MODELS as u32;
/// Beyond this a traffic or parked car is drawn with its far model.
pub const FAR_LOD: f64 = 90.0;
/// The mesh id of a traffic model, near or far.
pub fn traffic_mesh(model: u8, d2: f64) -> u32 {
    let m = model.min(N_MODELS as u8 - 1) as u32;
    if d2 > FAR_LOD * FAR_LOD { FAR_BASE + m } else { m }
}
/// The mesh id of a player car body.
pub fn player_body_mesh(model: u8) -> u32 {
    PLAYER_BASE + model.min(N_MODELS as u8 - 1) as u32 * 2
}
/// The mesh id of a player car wheel.
pub fn player_wheel_mesh(model: u8) -> u32 {
    player_body_mesh(model) + 1
}

// ------------------------------------------------------------------ traffic

#[derive(Clone)]
pub struct Car {
    pub path: u32,
    pub s: f32,
    pub fwd: bool,
    pub lane: u8,
    pub v: f32,
    pub vmul: f32,
    pub model: u8,
    pub col: [f32; 3],
    /// 0..1; only cars ranked under the current traffic level are out
    pub rank: f32,
    /// smoothed heading and the fading offset left over from the last turn
    pub dir: Vec3,
    /// Offset from its lane (tile frame, m): the leftovers of a turn and of a
    /// collision shove.
    pub shift: Vec3,
    /// Velocity given to the car by a collision (tile frame, m/s); it moves
    /// `shift` and is scrubbed off by the tyres.
    pub bump: Vec3,
    gap: f32,
    /// Seconds left braking to a stop for a player nearby (see `step`).
    pub yield_t: f32,
    /// Damage taken from the player's car, 0..1; it slows the car for good.
    pub hurt: f32,
    /// Under the 3D tiles: how far the mesh's street is above our road here,
    /// and the frame it was found on (World::photo_car_lights).
    pub street: std::cell::Cell<(f32, u32)>,
}

/// How long a car holds still after the player comes right up to it (s).
const YIELD_HOLD: f32 = 2.0;
/// Within this distance of the player a yielding car stops outright (m): close
/// enough to be walked up to, but clear of the body.
const YIELD_STOP: f32 = 2.5;
/// Within this distance it holds at a standstill, rather than creeping up on
/// the player a little at a time (m).
const YIELD_HOLD_D: f32 = 4.5;
/// How hard a yielding car brakes (m/s²).
const YIELD_BRAKE: f32 = 12.0;

/// A car at a known distance along a path, for a synthetic tile (the
/// `--carjack` smoke run): a real car in every other respect.
pub fn placed(path: u32, s: f32, v: f32, model: u8, col: [f32; 3]) -> Car {
    Car { path, s, fwd: true, lane: 0, v, vmul: 1.0, model, col, rank: 0.0, dir: Vec3::ZERO, shift: Vec3::ZERO, bump: Vec3::ZERO, gap: 1e9, yield_t: 0.0, hurt: 0.0, street: std::cell::Cell::new((0.0, u32::MAX)) }
}

/// Carry a collision shove: the offset moves at the bump velocity, and tyre
/// scrub slows it down and keeps it from sliding off into the next street.
pub fn shove_step(shift: &mut Vec3, bump: &mut Vec3, dt: f32) {
    if !bump.is_finite() || !shift.is_finite() || !(dt > 0.0) {
        *bump = Vec3::ZERO;
        return;
    }
    *shift += *bump * dt;
    *bump *= (-dt * 5.0).exp();
    if bump.length_squared() < 1e-4 {
        *bump = Vec3::ZERO;
    }
    if shift.length() > 10.0 {
        *shift = shift.normalize_or_zero() * 10.0;
        *bump *= 0.5;
    }
}

fn density(class: u8) -> f32 {
    match class {
        0 => 1.0 / 42.0,
        1 => 1.0 / 48.0,
        2 => 1.0 / 55.0,
        3 => 1.0 / 75.0,
        4 => 1.0 / 110.0,
        5 => 1.0 / 210.0,
        _ => 1.0 / 800.0,
    }
}

fn livery(model: u8, h: f32) -> [f32; 3] {
    match model {
        6 => [[0.75, 0.75, 0.72], [0.5, 0.03, 0.02], [0.62, 0.42, 0.02], [0.03, 0.12, 0.35], [0.6, 0.22, 0.02]][(h * 5.0) as usize % 5],
        7 => [[0.75, 0.75, 0.72], [0.4, 0.4, 0.4], [0.05, 0.1, 0.3], [0.4, 0.04, 0.03]][(h * 4.0) as usize % 4],
        _ => paint(h),
    }
}

fn place(net: &RoadNet, c: &mut Car, seed: u32) -> bool {
    // Pick a road with probability proportional to how much traffic it carries.
    let total: f32 = net.paths.iter().map(|p| p.len * density(p.class) * p.lanes as f32 * if p.oneway { 1.0 } else { 2.0 }).sum();
    if total <= 0.0 {
        return false;
    }
    let mut pick = hash(seed, 1) * total;
    for (i, p) in net.paths.iter().enumerate() {
        let w = p.len * density(p.class) * p.lanes as f32 * if p.oneway { 1.0 } else { 2.0 };
        if pick < w {
            c.path = i as u32;
            c.s = hash(seed, 2) * p.len;
            c.fwd = p.oneway || hash(seed, 3) < 0.5;
            c.lane = ((hash(seed, 4) * p.lanes as f32) as u8).min(p.lanes - 1);
            c.v = p.speed * c.vmul;
            c.shift = Vec3::ZERO;
            c.bump = Vec3::ZERO;
            c.dir = Vec3::ZERO;
            c.yield_t = 0.0;
            c.hurt = 0.0;
            return true;
        }
        pick -= w;
    }
    false
}

pub fn spawn(net: &RoadNet, tile_seed: u32) -> Vec<Car> {
    let want: f32 = net.paths.iter().map(|p| p.len * density(p.class) * p.lanes as f32 * if p.oneway { 1.0 } else { 2.0 }).sum();
    let n = (want as usize).min(520);
    let mut cars = Vec::with_capacity(n);
    for k in 0..n as u32 {
        let seed = tile_seed ^ k.wrapping_mul(2654435761);
        let mut c = Car { path: 0, s: 0.0, fwd: true, lane: 0, v: 0.0, vmul: 0.82 + 0.36 * hash(seed, 5), model: 0, col: [0.5; 3], rank: hash(seed, 6), dir: Vec3::ZERO, shift: Vec3::ZERO, bump: Vec3::ZERO, gap: 1e9, yield_t: 0.0, hurt: 0.0, street: std::cell::Cell::new((0.0, u32::MAX)) };
        if place(net, &mut c, seed) {
            c.model = car_model(hash(seed, 7), net.paths[c.path as usize].class <= 2);
            c.col = livery(c.model, hash(seed, 8));
            cars.push(c);
        }
    }
    cars
}

/// Where a car sits and which way it points, relative to the tile centre.
pub fn pose(net: &RoadNet, c: &Car, up: Vec3) -> (Vec3, Vec3) {
    let p = &net.paths[c.path as usize];
    let (pos, _) = p.sample(c.s);
    let (a, _) = p.sample(c.s - 2.4);
    let (b, _) = p.sample(c.s + 2.4);
    let mut d = (b - a).normalize_or_zero();
    if !c.fwd {
        d = -d;
    }
    let right = d.cross(up).normalize_or_zero();
    let off = if p.oneway { (c.lane as f32 + 0.5 - p.lanes as f32 * 0.5) * p.lane_w } else { (c.lane as f32 + 0.5) * p.lane_w * if net.left { -1.0 } else { 1.0 } };
    (pos + right * off, d)
}

/// Pick the way onward from a node. Returns (path, distance along, forward).
fn onward(net: &RoadNet, links: &[Link], from: u32, seed: u32) -> Option<(u32, f32, bool)> {
    let mut opts: Vec<(u32, f32, bool, f32)> = Vec::new();
    for l in links {
        if l.path == from {
            continue;
        }
        let p = &net.paths[l.path as usize];
        let w = if p.class >= 6 { 0.2 } else { 1.0 };
        match l.end {
            0 => opts.push((l.path, 0.0, true, w)),
            1 => {
                if !p.oneway {
                    opts.push((l.path, p.len, false, w))
                }
            }
            _ => {
                opts.push((l.path, l.at, true, w));
                if !p.oneway {
                    opts.push((l.path, l.at, false, w));
                }
            }
        }
    }
    let total: f32 = opts.iter().map(|o| o.3).sum();
    let mut pick = hash(seed, 11) * total;
    for o in &opts {
        if pick < o.3 {
            return Some((o.0, o.1, o.2));
        }
        pick -= o.3;
    }
    None
}

/// A car that drove off the edge of its tile: which edge, at which node.
pub struct Exit {
    pub car: Car,
    pub edge: u8,
    pub key: u64,
}

/// Put a car arriving from a neighbouring tile onto this tile's roads.
pub fn enter(net: &RoadNet, mut car: Car, key: u64, up: Vec3, was: Vec3) -> Option<Car> {
    for k in near_keys(key) {
        if let Some(links) = net.nodes.get(&k) {
            if let Some((p, s, fwd)) = onward(net, links, u32::MAX, (car.rank * 1e6) as u32 ^ k as u32) {
                car.path = p;
                car.s = s;
                car.fwd = fwd;
                car.lane = car.lane.min(net.paths[p as usize].lanes - 1);
                car.shift = was - pose(net, &car, up).0;
                if car.shift.length() > 12.0 {
                    car.shift = Vec3::ZERO;
                }
                return Some(car);
            }
        }
    }
    None
}

pub fn respawn(net: &RoadNet, car: &mut Car, seed: u32) {
    place(net, car, seed);
}

/// Indices of `cars` by lane (path, direction, lane), then by distance along
/// it, ties in index order: what a stable sort by those gives, but sorted as
/// packed keys (the distance in `f32::total_cmp` order) instead of chasing
/// every comparison through `cars`.
fn lane_order(cars: &[Car]) -> Vec<usize> {
    let mut keys: Vec<u128> = cars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let lane = (c.path as u128) << 9 | (c.fwd as u128) << 8 | c.lane as u128;
            let b = c.s.to_bits();
            let s = b ^ if b >> 31 == 1 { u32::MAX } else { 1 << 31 };
            lane << 64 | (s as u128) << 32 | i as u128
        })
        .collect();
    keys.sort_unstable();
    keys.iter().map(|k| *k as u32 as usize).collect()
}

/// Advance every car of one tile. Cars that leave the tile are returned.
/// `player` is the player's position relative to the tile centre and the
/// distance traffic brakes for them within: a car inside it eases off, and one
/// the player has come right up to stops and holds for a moment, so it can be
/// taken at a standstill.
pub fn step(net: &RoadNet, cars: &mut Vec<Car>, dt: f32, up: Vec3, tick: u32, player: Option<(Vec3, f32)>) -> Vec<Exit> {
    // Following distance: sort the cars of each lane and look at the one in front.
    let order = lane_order(cars);
    for c in cars.iter_mut() {
        c.gap = 1e9;
    }
    for w in order.windows(2) {
        let (a, b) = (w[0], w[1]);
        if (cars[a].path, cars[a].fwd, cars[a].lane) == (cars[b].path, cars[b].fwd, cars[b].lane) {
            let g = cars[b].s - cars[a].s - (CAR_LEN[cars[a].model as usize] + CAR_LEN[cars[b].model as usize]) * 0.5;
            if cars[a].fwd { cars[a].gap = g } else { cars[b].gap = g }
        }
    }
    let mut exits = Vec::new();
    let mut i = 0;
    while i < cars.len() {
        let seed = tick.wrapping_mul(31).wrapping_add(i as u32 * 7919);
        let c = &mut cars[i];
        let p = &net.paths[c.path as usize];
        let ahead = if c.fwd { p.len - c.s } else { c.s };
        let mut target = p.speed * c.vmul;
        // Damaged cars go slower for good.
        target *= 1.0 - 0.6 * c.hurt.clamp(0.0, 1.0);
        // Ease off for the junction at the end of the road, and for the car in front.
        if ahead < 22.0 && p.class >= 2 {
            target = target.min(4.5 + ahead * 0.45);
        }
        target = target.min((c.gap - 2.5).max(0.0) * 0.9);
        // Yield to the player: brake smoothly inside the radius, stop outright
        // when they are right next to the car, and hold a moment after they go.
        c.yield_t = (c.yield_t - dt).max(0.0);
        if let Some((pp, radius)) = player {
            let d = (pose(net, c, up).0 + c.shift - pp).length();
            if d < radius {
                let f = ((d - YIELD_STOP) / (radius - YIELD_STOP).max(1.0)).clamp(0.0, 1.0);
                target *= f;
                if d < YIELD_HOLD_D {
                    c.yield_t = c.yield_t.max(YIELD_HOLD);
                }
            }
        }
        let held = c.yield_t > 0.0;
        if held {
            target = 0.0;
        }
        let decel = if held { YIELD_BRAKE } else { 7.0 };
        c.v += (target - c.v).clamp(-decel * dt, 2.6 * dt);
        c.v = c.v.max(0.0);
        let before = c.s;
        c.s += c.v * dt * if c.fwd { 1.0 } else { -1.0 };
        c.shift *= (-dt * 2.2).exp();
        shove_step(&mut c.shift, &mut c.bump, dt);
        let mut next: Option<(u32, f32, bool)> = None;
        let mut gone = false;
        if c.s >= p.len || c.s <= 0.0 {
            let end = (c.s >= p.len) as usize;
            let key = p.nodes[end];
            next = net.nodes.get(&key).and_then(|l| onward(net, l, c.path, seed));
            if next.is_none() {
                if p.edge[end] != 0 {
                    gone = true;
                    exits.push(Exit { car: c.clone(), edge: p.edge[end], key });
                } else if !p.oneway {
                    // Dead end: turn round.
                    c.fwd = !c.fwd;
                    c.s = c.s.clamp(0.0, p.len);
                    c.v = 0.0;
                } else {
                    place(net, c, seed);
                }
            }
        } else if !p.junctions.is_empty() && hash(seed, 12) < 0.3 {
            // Passing a side street: sometimes take it.
            let (lo, hi) = (before.min(c.s), before.max(c.s));
            if let Some((_, key)) = p.junctions.iter().find(|j| j.0 > lo && j.0 <= hi) {
                next = net.nodes.get(key).and_then(|l| {
                    let ends: Vec<Link> = l.iter().filter(|x| x.end != 2).copied().collect();
                    onward(net, &ends, c.path, seed)
                });
            }
        }
        if let Some((np, ns, nf)) = next {
            // Where it was drawn before the turn (only needed for a turn: the
            // car has not been moved off its path or lane yet).
            let was = pose(net, c, up).0 + c.shift;
            c.path = np;
            c.s = ns;
            c.fwd = nf;
            c.lane = c.lane.min(net.paths[np as usize].lanes - 1);
            c.v = c.v.min(7.0);
            c.shift = was - pose(net, c, up).0;
        }
        if gone {
            cars.swap_remove(i);
        } else {
            i += 1;
        }
    }
    exits
}

/// How busy the roads are through the day (0..1) by local hour.
pub fn activity(hour: f32) -> f32 {
    let bump = |c: f32, w: f32| (-((hour - c) / w).powi(2)).exp();
    (0.1 + 0.9 * (bump(8.3, 2.2) + bump(17.5, 2.8) + 0.7 * bump(13.0, 3.0))).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_models_have_bodies_wheels_and_four_wheels_each() {
        let pm = player_models();
        assert_eq!(pm.len(), N_MODELS);
        for (i, m) in pm.iter().enumerate() {
            assert!(!m.body.0.is_empty() && !m.body.1.is_empty(), "model {i} body");
            assert!(!m.wheel.0.is_empty() && !m.wheel.1.is_empty(), "model {i} wheel");
            assert!(m.body.1.iter().all(|k| (*k as usize) < m.body.0.len()));
            assert!(m.wheel.1.iter().all(|k| (*k as usize) < m.wheel.0.len()));
            assert!(m.body.0.iter().chain(m.wheel.0.iter()).all(|v| v.pos.iter().all(|f| f.is_finite()) && v.nrm.iter().all(|f| f.is_finite())));
            assert!((m.len - CAR_LEN[i]).abs() < 1e-5, "model {i} length");
            assert!(m.width > 0.0 && m.wheel_r > 0.0);
            for (k, w) in m.wheels.iter().enumerate() {
                assert!(w.iter().all(|f| f.is_finite()), "model {i} wheel {k}");
                assert!(w[1] > 0.0, "model {i} wheel {k} sits on the ground");
            }
            // The two sides are mirrored; front and rear differ.
            assert!((m.wheels[0][0] + m.wheels[1][0]).abs() < 1e-5);
            assert!((m.wheels[2][0] + m.wheels[3][0]).abs() < 1e-5);
            assert!(m.wheels[0][2] != m.wheels[2][2], "model {i} axles apart");
        }
        assert_eq!(player_body_mesh(0), PLAYER_BASE);
        assert_eq!(player_wheel_mesh(0), PLAYER_BASE + 1);
        assert_eq!(player_body_mesh(7), PLAYER_BASE + 14);
        assert_eq!(player_wheel_mesh(7), PLAYER_BASE + 15);
    }

    #[test]
    fn player_wheel_meshes_are_centred_on_the_axle() {
        // A wheel is spun about its instance origin. Its mesh must therefore be
        // centred there, not a wheel radius above it, or it orbits its axle.
        for (i, m) in player_models().iter().enumerate() {
            let n = m.wheel.0.len() as f32;
            let cy = m.wheel.0.iter().map(|v| v.pos[1]).sum::<f32>() / n;
            let cz = m.wheel.0.iter().map(|v| v.pos[2]).sum::<f32>() / n;
            assert!(cy.abs() < 0.01, "model {i} wheel is {cy:.3} m above its axle");
            assert!(cz.abs() < 0.01, "model {i} wheel is {cz:.3} m along its axle");
            let ground = m.wheels.iter().map(|w| w[1]).fold(f32::INFINITY, f32::min);
            let low = m.wheel.0.iter().map(|v| v.pos[1]).fold(f32::INFINITY, f32::min);
            assert!((ground + low).abs() < 0.05, "model {i} wheel sits on the ground");
        }
    }

    #[test]
    fn the_traffic_models_are_still_the_eight_single_meshes() {
        let models = build_models();
        assert_eq!(models.len(), N_MODELS);
        assert!(models.iter().all(|(v, i)| !v.is_empty() && !i.is_empty()));
        for (i, (v, _)) in models.iter().enumerate() {
            let hi = v.iter().map(|p| p.pos[2]).fold(f32::MIN, f32::max);
            let lo = v.iter().map(|p| p.pos[2]).fold(f32::MAX, f32::min);
            assert!((hi - lo - CAR_LEN[i]).abs() < 0.3, "model {i} is {:.1} m long", hi - lo);
        }
    }

    #[test]
    fn far_models_are_lighter_and_the_same_size() {
        let (near, far) = (build_models(), build_far_models());
        assert_eq!(far.len(), N_MODELS);
        for (i, ((nv, ni), (fv, fi))) in near.iter().zip(far.iter()).enumerate() {
            assert!(fi.iter().all(|k| (*k as usize) < fv.len()), "model {i}");
            assert!(fv.iter().all(|v| v.pos.iter().chain(v.nrm.iter()).all(|f| f.is_finite())), "model {i}");
            assert!(fi.len() * 3 < ni.len(), "model {i}: far {} vs near {} triangles", fi.len() / 3, ni.len() / 3);
            let span = |v: &[MeshVertex], a: usize| v.iter().map(|p| p.pos[a]).fold(f32::MIN, f32::max) - v.iter().map(|p| p.pos[a]).fold(f32::MAX, f32::min);
            for a in 0..3 {
                assert!((span(nv, a) - span(fv, a)).abs() < 0.12, "model {i} axis {a}");
            }
        }
        assert_eq!(traffic_mesh(3, 10.0), 3);
        assert_eq!(traffic_mesh(3, (FAR_LOD * 2.0).powi(2)), FAR_BASE + 3);
    }

    #[test]
    fn wheels_sit_inside_their_arches() {
        for (i, s) in crate::carmesh::specs().iter().enumerate() {
            for &af in s.axles {
                // The tyre's outer face stays inside the body side, and the
                // top of the tyre below the top of the arch.
                let outer = s.wheel_x(af) + s.tyre_w * 0.5;
                assert!(outer < s.half_width(af), "model {i}: tyre sticks out");
                assert!(outer > s.half_width(af) - 0.05, "model {i}: tyre tucked too deep");
            }
            assert!(s.windows.windows(2).all(|w| w[0].1 < w[1].0), "model {i}: windows overlap");
        }
    }

    /// The packed-key order is the stable sort by lane and distance it replaced,
    /// ties, negative zero and all.
    #[test]
    fn lane_order_matches_stable_sort() {
        let mut seed = 7u32;
        let mut rnd = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            seed >> 8
        };
        let cars: Vec<Car> = (0..600)
            .map(|_| Car {
                path: rnd() % 5,
                s: [0.0, -0.0, 12.5, (rnd() % 4000) as f32 * 0.25 - 50.0][(rnd() % 4) as usize],
                fwd: rnd() % 2 == 0,
                lane: (rnd() % 3) as u8,
                v: 0.0,
                vmul: 1.0,
                model: 0,
                col: [0.0; 3],
                rank: 0.0,
                dir: Vec3::ZERO,
                shift: Vec3::ZERO,
                bump: Vec3::ZERO,
                gap: 0.0,
                yield_t: 0.0,
                hurt: 0.0,
                street: Default::default(),
            })
            .collect();
        let mut want: Vec<usize> = (0..cars.len()).collect();
        want.sort_by(|&a, &b| {
            let (x, y) = (&cars[a], &cars[b]);
            (x.path, x.fwd, x.lane).cmp(&(y.path, y.fwd, y.lane)).then(x.s.total_cmp(&y.s))
        });
        assert_eq!(lane_order(&cars), want);
    }
}
