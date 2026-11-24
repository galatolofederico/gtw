//! Night on the photorealistic 3D tiles (tiles3d.rs).
//!
//! The mesh is a daylight photograph: after dusk its walls go dark and
//! nothing in it lights up. Its windows come from our own map instead. As
//! each tile is decoded, the vertices inside an OpenStreetMap building
//! footprint (grown a little, as the mesh's walls stray past the outline)
//! are tagged with which way that building faces and whether it is tall;
//! photo.wgsl lays a grid of lit and dark windows along such walls, square
//! to the building, a storey high. Street light comes from our roads at
//! draw time (the coverage mask, render.rs).
use crate::geo;
use crate::pave::{Raster, vector_around};
use crate::sources::Sources;
use glam::{DVec3, Vec2, Vec3};

/// Footprints are grown by this much (m): photogrammetry walls are soft and
/// stand a little outside the mapped outline.
const GROW: f32 = 1.5;
/// Taller than this (m), a building gets office windows (ribbons, taller
/// storeys, whole floors lit), lower ones houses and flats.
const TALL: f32 = 20.0;
/// Building orientations told apart (over 90 degrees).
pub const ORIENT: u32 = 63;
/// Tiles wider than this (m) are left untagged: far off, their windows are
/// below a pixel anyway.
const MAX_EXTENT: f32 = 3000.0;
/// From this height (m, OpenStreetMap's) a building carries red aviation
/// obstruction lights on its roof (ICAO: 45 m; FAA asks from about 61 m).
pub const BEACON_H: f32 = 45.0;
/// Wider than this (m), a roof gets a light near each corner as well.
const BEACON_WIDE: f32 = 45.0;

/// An obstruction light on the mesh's roof: where (relative to the tile's
/// origin, our frame) and which building it is (the same in every tile that
/// holds part of it, so neighbouring tiles' copies can be told apart).
#[derive(Clone, Copy, Debug)]
pub struct Beacon {
    pub pos: Vec3,
    pub key: u64,
}

/// Point in polygon (even-odd).
fn inside(r: &[Vec2], p: Vec2) -> bool {
    let mut c = false;
    let mut j = r.len() - 1;
    for i in 0..r.len() {
        let (a, b) = (r[i], r[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            c = !c;
        }
        j = i;
    }
    c
}

/// Lights on the roofs of the tall buildings among `tall` (outlines in the
/// tile's east/north metres, with their keys): at the mesh's highest point
/// inside each, and on wide ones also at the highest point near each corner
/// that stands within 15% of the top.
fn beacons(tall: &[(Vec<Vec2>, u64)], pos: &[Vec3], xy: &[Vec2], up: Vec3) -> Vec<Beacon> {
    let mut out = Vec::new();
    for (ring, key) in tall {
        let (lo, hi) = ring.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
        let ins: Vec<usize> = (0..xy.len()).filter(|&i| xy[i].cmpge(lo).all() && xy[i].cmple(hi).all() && inside(ring, xy[i])).collect();
        let Some(&top) = ins.iter().max_by(|&&a, &&b| pos[a].dot(up).total_cmp(&pos[b].dot(up))) else { continue };
        let base = ins.iter().map(|&i| pos[i].dot(up)).fold(f32::MAX, f32::min);
        let h = pos[top].dot(up);
        // Not a tower here (the mesh's part of it is only its foot).
        if h - base < BEACON_H * 0.6 {
            continue;
        }
        out.push(Beacon { pos: pos[top] + up * 0.6, key: *key });
        if (hi - lo).max_element() > BEACON_WIDE {
            let floor = base + (h - base) * 0.85;
            for c in ring {
                let near = ins.iter().filter(|&&i| xy[i].distance(*c) < 10.0 && pos[i].dot(up) > floor).max_by(|&&a, &&b| pos[a].dot(up).total_cmp(&pos[b].dot(up)));
                if let Some(&i) = near {
                    if out.iter().all(|b: &Beacon| b.key != *key || b.pos.distance(pos[i]) > 15.0) {
                        out.push(Beacon { pos: pos[i] + up * 0.6, key: *key });
                    }
                }
            }
        }
    }
    out
}

/// The tag of a building: 0 none, else 1 + orientation step (0..ORIENT, the
/// angle of its walls from east, 90 / ORIENT degrees each), plus 64 if tall.
pub fn code(angle: f32, tall: bool) -> u8 {
    let q = (angle.rem_euclid(std::f32::consts::FRAC_PI_2) / std::f32::consts::FRAC_PI_2 * ORIENT as f32).round() as u32 % ORIENT;
    1 + q as u8 + if tall { 64 } else { 0 }
}

/// Which way an outline's walls run, folded into 0..90 degrees from east:
/// the length-weighted mean of its edge directions, taken four times over so
/// that perpendicular walls agree.
pub fn orientation(ring: &[Vec2]) -> f32 {
    let mut s = Vec2::ZERO;
    for i in 0..ring.len() {
        let d = ring[(i + 1) % ring.len()] - ring[i];
        let l = d.length();
        if l > 1e-3 {
            let a = d.y.atan2(d.x) * 4.0;
            s += Vec2::new(a.cos(), a.sin()) * l;
        }
    }
    (s.y.atan2(s.x) / 4.0).rem_euclid(std::f32::consts::FRAC_PI_2)
}

/// Tags for the vertices of a tile (`pos` relative to `origin`, in our
/// frame; `lla` their latitude and longitude in radians), one per vertex, or
/// nothing if the tile has no buildings under it; and the obstruction
/// lights on its tall buildings' roofs.
pub fn tag(src: &Sources, origin: DVec3, pos: &[Vec3], lla: &[[f64; 3]]) -> (Vec<u8>, Vec<Beacon>) {
    if pos.is_empty() {
        return Default::default();
    }
    let (e, n, _) = geo::enu(origin);
    let (e3, n3) = (e.as_vec3(), n.as_vec3());
    let xy: Vec<Vec2> = pos.iter().map(|p| Vec2::new(p.dot(e3), p.dot(n3))).collect();
    let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
    for p in &xy {
        (lo, hi) = (lo.min(*p), hi.max(*p));
    }
    let ext = (hi - lo).max_element();
    if ext > MAX_EXTENT {
        return Default::default();
    }
    let mut tall: Vec<(Vec<Vec2>, u64)> = Vec::new();
    let cell = (ext / 1500.0).max(1.0);
    let mut r = Raster::new(lo, hi, cell, 10.0);
    let mut any = false;
    let ok = vector_around(src, origin, lla, |vt, local| {
        for b in &vt.buildings {
            let Some(outer) = b.rings.first() else { continue };
            let ring: Vec<Vec2> = outer.iter().map(local).collect();
            if ring.len() < 3 {
                continue;
            }
            // Only those that reach this tile.
            let (blo, bhi) = ring.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
            if bhi.x < r.min.x || bhi.y < r.min.y || blo.x > r.min.x + r.w as f32 * cell || blo.y > r.min.y + r.h as f32 * cell {
                continue;
            }
            let k = code(orientation(&ring), b.height >= TALL);
            if b.height >= BEACON_H {
                // Keyed by where its middle is on the planet, to 4 m.
                let m = ring.iter().fold(Vec2::ZERO, |a, p| a + *p) / ring.len() as f32;
                let w = origin + e * m.x as f64 + n * m.y as f64;
                let q = (w / 4.0).round();
                let key = (q.x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (q.y as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ (q.z as i64 as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
                tall.push((ring.clone(), key));
            }
            let rings: Vec<Vec<Vec2>> = b.rings.iter().map(|r| r.iter().map(local).collect()).collect();
            r.poly(&rings, k);
            any = true;
        }
    });
    if !ok || !any {
        return Default::default();
    }
    let lights = beacons(&tall, pos, &xy, origin.normalize().as_vec3());
    // Grown: each cell takes the largest tag within GROW of it.
    let g = (GROW / cell).ceil() as i64;
    let src_k = r.k.clone();
    let at = |x: i64, y: i64| if x < 0 || y < 0 || x >= r.w as i64 || y >= r.h as i64 { 0 } else { src_k[y as usize * r.w + x as usize] };
    let mut grown = vec![0u8; r.w * r.h];
    for y in 0..r.h as i64 {
        for x in 0..r.w as i64 {
            let mut m = 0;
            for dy in -g..=g {
                for dx in -g..=g {
                    if dx * dx + dy * dy <= g * g {
                        m = m.max(at(x + dx, y + dy));
                    }
                }
            }
            grown[y as usize * r.w + x as usize] = m;
        }
    }
    r.k = grown;
    let tags = xy
        .iter()
        .map(|p| {
            let q = (*p - r.min) / cell;
            r.at(q.x.floor() as i64, q.y.floor() as i64)
        })
        .collect();
    (tags, lights)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_folds_walls() {
        let sq = |a: f32| -> Vec<Vec2> {
            let (c, s) = (a.cos(), a.sin());
            [(0.0, 0.0), (30.0, 0.0), (30.0, 12.0), (0.0, 12.0)].iter().map(|&(x, y)| Vec2::new(x * c - y * s, x * s + y * c)).collect()
        };
        for deg in [0.0f32, 10.0, 37.0, 80.0, 100.0, -20.0] {
            let got = orientation(&sq(deg.to_radians())).to_degrees();
            let want = deg.rem_euclid(90.0);
            let d = (got - want).abs();
            assert!(d < 0.01 || (d - 90.0).abs() < 0.01, "{deg}: {got}");
        }
    }

    #[test]
    fn beacon_on_the_top() {
        // A 20 m square tower 80 m tall: a ground ring and a roof ring.
        let up = Vec3::Z;
        let mut pos = Vec::new();
        for h in [0.0f32, 80.0] {
            for (x, y) in [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0), (10.0, 10.0)] {
                pos.push(Vec3::new(x * 0.98 + 0.2, y * 0.98 + 0.2, if (x, y) == (10.0, 10.0) { h + 3.0 * (h > 0.0) as u8 as f32 } else { h }));
            }
        }
        let xy: Vec<Vec2> = pos.iter().map(|p| p.truncate()).collect();
        let ring = vec![Vec2::ZERO, Vec2::new(20.0, 0.0), Vec2::new(20.0, 20.0), Vec2::new(0.0, 20.0)];
        let b = beacons(&[(ring.clone(), 7)], &pos, &xy, up);
        assert_eq!(b.len(), 1);
        assert!((b[0].pos.z - 83.6).abs() < 1e-3 && b[0].key == 7);
        // Only its foot in this tile: none.
        let low: Vec<Vec3> = pos.iter().map(|p| Vec3::new(p.x, p.y, p.z.min(10.0))).collect();
        assert!(beacons(&[(ring, 7)], &low, &xy, up).is_empty());
    }

    #[test]
    fn codes_fit_a_byte() {
        assert_eq!(code(0.0, false), 1);
        assert_eq!(code(std::f32::consts::FRAC_PI_2 - 1e-4, false), 1);
        assert!(code(1.5, true) <= 127 && code(1.5, true) > 64);
        assert_eq!(code(std::f32::consts::FRAC_PI_4, false), 1 + (ORIENT as f32 / 2.0).round() as u8);
    }
}
