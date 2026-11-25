//! The ramp: the game's own flyable types parked at the airfields near the
//! player, drawn with their real models, to walk up to and take (the
//! interact button, Y on the pad, as for a car). They fill stands the airport
//! traffic leaves free (kept from it while they stand there), and a field
//! with too few free stands gets overflow aircraft on the grass beside a
//! runway. Every field gets at least [`MIN_TYPES`] different types, so a
//! player who walks to any airport has a choice to steal. They are scenery
//! until boarded: not simulated, not saved (a field gets a fresh random set
//! each time the player comes back to it), and, like the traffic's parked
//! aircraft, not solid to the character or the car.
use crate::plane::Plane;
use crate::{aircraft, airframe, airports, geo, render, t, Game, Mode, AIRCRAFT_REACH};
use glam::{DVec3, Mat4, Vec2};

/// Fields this close (m, from their edge) get their aircraft.
const FILL_DIST: f64 = 6_000.0;
/// Fields this far away give theirs up.
const DROP_DIST: f64 = 14_000.0;
/// Drawn with their full model this close (m).
const DRAW_DIST: f64 = 3_000.0;
/// At most this many to a field.
const PER_FIELD: usize = 6;
/// At least this many different flyable types at every field, even one with
/// no free stands at all: the rest go on the grass beside a runway.
const MIN_TYPES: usize = 5;
/// Metres between overflow spots along a runway.
const OVERFLOW_STEP: f32 = 30.0;

/// One parked aircraft of a type the player can fly.
pub(crate) struct RampPlane {
    pub apt: u32,
    pub spec: &'static aircraft::Spec,
    /// a stand-in, posed on its gear (never stepped)
    pub plane: Plane,
    /// where it stands: latitude, longitude (radians), ground height (m), bearing and slope (radians)
    pub at: (f64, f64, f64, f64, f64),
}

/// A place to put a ramp aircraft: an airport stand, or an overflow spot on
/// the grass. `size` is the largest stand class it can take.
#[derive(Clone, Copy)]
struct Spot {
    size: u8,
    /// nose point and facing
    pos: Vec2,
    dir: Vec2,
}

/// The size of stand a type needs, as `airports::Stand::size` counts them
/// (0 light single ... 4 wide-body), from its full wingspan and the same
/// span classes [`airports::SIZE_SPAN`] uses for the stands.
fn size_class(spec: &aircraft::Spec) -> u8 {
    match spec.span * 2.0 {
        s if s <= 11.0 => 0,
        s if s <= 16.0 => 1,
        s if s <= 27.0 => 2,
        s if s <= 36.0 => 3,
        _ => 4,
    }
}

/// How far the nose is ahead of the reference centre (m): the forward-most contact point.
fn nose_ahead(spec: &aircraft::Spec) -> f32 {
    spec.contacts.iter().map(|(p, _)| -p[2]).fold(0.0f64, f64::max) as f32
}

/// The types that can be parked: everything built in that takes off from a runway.
fn parkable() -> Vec<&'static aircraft::Spec> {
    aircraft::BUILTIN.iter().copied().filter(|s| s.rocket().is_none()).collect()
}

/// A small xorshift for the picks.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let d = b - a;
    let t = ((p - a).dot(d) / d.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (a + d * t - p).length()
}

/// Runway centre lines to park overflow aircraft beside, longest first. A
/// field whose map has no runway lines falls back to the airport database's
/// runways, and finally to a short line at its reference point, so no field
/// is left without aircraft.
fn parking_lines(g: &airports::Geo) -> Vec<(Vec2, Vec2, f32, f32)> {
    let mut lines: Vec<_> = g.runways.iter().map(|r| (r.a, r.dir, r.len, r.width)).collect();
    if lines.is_empty() {
        for r in &g.apt().rwys {
            let (a, b) = match (r.pos[0], r.pos[1]) {
                (Some(p0), Some(p1)) => (
                    g.local(geo::lla_to_pos(p0.0.to_radians(), p0.1.to_radians(), 0.0)),
                    g.local(geo::lla_to_pos(p1.0.to_radians(), p1.1.to_radians(), 0.0)),
                ),
                _ => {
                    let hdg = airports::ident_heading(&r.ident[0]).unwrap_or(90.0).to_radians();
                    let dir = Vec2::new(hdg.sin(), hdg.cos());
                    let len = r.len.max(80.0).min(1_500.0);
                    (Vec2::ZERO - dir * (len * 0.5), Vec2::ZERO + dir * (len * 0.5))
                }
            };
            let dir = (b - a).normalize_or_zero();
            if dir != Vec2::ZERO {
                lines.push((a, dir, (b - a).length().max(60.0), r.width.max(15.0)));
            }
        }
    }
    if lines.is_empty() {
        lines.push((Vec2::new(-60.0, 0.0), Vec2::X, 120.0, 20.0));
    }
    lines.sort_by(|a, b| b.2.total_cmp(&a.2));
    lines
}

/// Candidate overflow positions along both sides of a line, noses towards it
/// (nose point, facing). Includes positions beyond each end, so even a very
/// short strip has room for several aircraft.
fn line_spots(a: Vec2, dir: Vec2, len: f32, width: f32) -> Vec<(Vec2, Vec2)> {
    let side = airports::right_of(dir);
    let mut along = Vec::new();
    let mut s = 15.0f32.min(len * 0.5);
    let end = (len - 12.0).max(15.0);
    while s <= end {
        along.push(s);
        s += OVERFLOW_STEP;
    }
    for k in 1..=4 {
        along.push(-(k as f32) * OVERFLOW_STEP);
        along.push(len + (k as f32) * OVERFLOW_STEP);
    }
    let mut out = Vec::with_capacity(along.len() * 4);
    for row in [width * 0.5 + 32.0, width * 0.5 + 64.0] {
        for &s in &along {
            for side_dir in [side, -side] {
                out.push((a + dir * s + side_dir * row, -side_dir));
            }
        }
    }
    out
}

/// Whether an overflow aircraft can stand at `p` facing `dir`: off the
/// pavement, taxiways, stands and helipads, and not on a steep slope.
fn spot_clear(g: &airports::Geo, p: Vec2, dir: Vec2) -> bool {
    if g.runways.iter().any(|r| {
        let (s, x) = r.local(p);
        s > -30.0 && s < r.len + 30.0 && x.abs() < r.width * 0.5 + 20.0
    }) {
        return false;
    }
    if g.taxiways.iter().any(|(pts, w)| pts.windows(2).any(|seg| seg_dist(p, seg[0], seg[1]) < w * 0.5 + 10.0)) {
        return false;
    }
    if g.stands.iter().any(|s| (s.pos - p).length() < 24.0) {
        return false;
    }
    if g.gates.iter().any(|(q, _)| (p - *q).length() < 24.0) {
        return false;
    }
    if g.helipads.iter().any(|(c, r)| (p - *c).length() < *r + 24.0) {
        return false;
    }
    let dh = (g.h(p + dir * 4.0) - g.h(p - dir * 4.0)).abs();
    dh.is_finite() && dh < 2.0
}

fn push_spot(out: &mut Vec<(Vec2, Vec2)>, p: Vec2, dir: Vec2, need: usize, g: &airports::Geo, clear: bool) {
    if out.len() >= need || out.iter().any(|q| (q.0 - p).length() < OVERFLOW_STEP) {
        return;
    }
    if clear && !spot_clear(g, p, dir) {
        return;
    }
    out.push((p, dir));
}

/// Extra grass spots beside the runways, up to `need`, for a field with too
/// few free stands. The second pass drops the pavement checks so a crowded
/// field still gets its aircraft rather than none.
fn overflow_spots(g: &airports::Geo, need: usize) -> Vec<(Vec2, Vec2)> {
    let mut out = Vec::new();
    for (a, dir, len, width) in parking_lines(g) {
        for (p, d) in line_spots(a, dir, len, width) {
            push_spot(&mut out, p, d, need, g, true);
            if out.len() >= need {
                return out;
            }
        }
    }
    for (a, dir, len, width) in parking_lines(g) {
        for (p, d) in line_spots(a, dir, len, width) {
            push_spot(&mut out, p, d, need, g, false);
            if out.len() >= need {
                return out;
            }
        }
    }
    out
}

/// How many overflow spots a field's free stands still need. Size-0 stands
/// take only the three light singles, so two wider spots are needed for five
/// different types; fewer than five free stands always need more.
fn overflow_needed(spots: &[Spot]) -> usize {
    let need = MIN_TYPES.saturating_sub(spots.len());
    let wide = spots.iter().filter(|s| s.size >= 1).count();
    need.max(2usize.saturating_sub(wide))
}

/// One different type for each spot, the biggest that fits (so big stands
/// show big aircraft), never the same type twice in one field.
fn choose_types(spots: &[Spot], types: &[&'static aircraft::Spec], rng: &mut Rng) -> Vec<(&'static aircraft::Spec, usize)> {
    let mut used = vec![false; types.len()];
    let mut order: Vec<usize> = (0..spots.len()).collect();
    for i in (1..order.len()).rev() {
        order.swap(i, rng.pick(i + 1));
    }
    order.sort_by_key(|&i| std::cmp::Reverse(spots[i].size));
    let mut out = Vec::with_capacity(spots.len());
    for si in order {
        let mut best: Option<(u8, Vec<usize>)> = None;
        for (ti, spec) in types.iter().enumerate() {
            if used[ti] {
                continue;
            }
            let class = size_class(spec);
            if class > spots[si].size {
                continue;
            }
            match &mut best {
                Some((bc, cands)) if class == *bc => cands.push(ti),
                Some((bc, cands)) if class > *bc => {
                    *bc = class;
                    *cands = vec![ti];
                }
                None => best = Some((class, vec![ti])),
                _ => {}
            }
        }
        let Some((_, cands)) = best else { continue };
        let ti = cands[rng.pick(cands.len())];
        used[ti] = true;
        out.push((types[ti], si));
    }
    out
}

impl Game {
    /// Park aircraft at the fields that came near and clear those left far
    /// behind. Runs while playing (not in the menus).
    pub(crate) fn update_ramp(&mut self) {
        if self.mode == Mode::Menu {
            return;
        }
        let focus = self.focus_pos();
        let near = self.world.fields_near(focus, DROP_DIST);
        self.ramp.retain(|r| near.contains(&r.apt));
        self.ramp_filled.retain(|a| near.contains(a));
        for apt in self.world.fields_near(focus, FILL_DIST) {
            if self.ramp_filled.insert(apt) {
                self.fill_field(apt);
            }
        }
        // Start loading the models of those near enough to draw.
        let wanted: Vec<_> = self.ramp.iter().filter(|r| (r.plane.pos - focus).length() < DRAW_DIST * 1.5).map(|r| r.spec).collect();
        for s in wanted {
            self.fleet_ready(s);
        }
    }

    /// A random set of different types at one field: free stands first, then
    /// overflow grass spots, so every field has at least [`MIN_TYPES`].
    fn fill_field(&mut self, apt: u32) {
        let Some(g) = self.world.field_geo(apt) else { return };
        let types = parkable();
        if types.is_empty() {
            return;
        }
        let seed = (apt as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) ^ self.frame;
        let mut rng = Rng(seed | 1);
        let mut free = self.world.free_stands(apt);
        // shuffled, then the first few
        for i in (1..free.len()).rev() {
            free.swap(i, rng.pick(i + 1));
        }
        let mut spots: Vec<Spot> = free.iter().take(PER_FIELD).filter_map(|&si| g.stands.get(si).map(|s| Spot { size: s.size, pos: s.pos, dir: s.dir })).collect();
        let extra = overflow_needed(&spots);
        if extra > 0 {
            spots.extend(overflow_spots(&g, extra).into_iter().map(|(pos, dir)| Spot { size: 1, pos, dir }));
        }
        // Keep the widest spots when there are more than PER_FIELD: a full
        // field of tie-downs must not crowd out the overflow that gives the
        // field its different types.
        spots.sort_by_key(|s| std::cmp::Reverse(s.size));
        spots.truncate(PER_FIELD);
        let picks = choose_types(&spots, &types, &mut rng);
        let before = self.ramp.len();
        for (spec, si) in picks {
            let spot = spots[si];
            let lp = spot.pos - spot.dir * nose_ahead(spec);
            let h = g.h(lp) as f64;
            let slope = (g.h(lp + spot.dir * 3.0) - g.h(lp - spot.dir * 3.0)) as f64 / 6.0;
            let (lat, lon) = g.lla(lp);
            let yaw = (spot.dir.x as f64).atan2(spot.dir.y as f64);
            let mut plane = Plane::spawn_ground(spec, lat, lon, h, yaw, slope.atan());
            plane.cold_and_dark();
            self.ramp.push(RampPlane { apt, spec, plane, at: (lat, lon, h, yaw, slope.atan()) });
        }
        let names: Vec<_> = self.ramp[before..].iter().map(|r| r.spec.keys[0]).collect();
        if names.len() < MIN_TYPES {
            log::warn!("ramp: only {} parked at {} ({} stands free): {}", names.len(), g.apt().code(), free.len(), names.join(", "));
        } else {
            log::info!("ramp: {} parked at {} ({} stands free): {}", names.len(), g.apt().code(), free.len(), names.join(", "));
        }
    }

    /// How close the player must be to get into a parked aircraft (m): bigger types further from their centre.
    fn ramp_reach(spec: &aircraft::Spec) -> f64 {
        AIRCRAFT_REACH.max(spec.span as f64 * 0.8)
    }

    /// The parked aircraft the player on foot can get into, nearest first.
    pub(crate) fn ramp_in_reach(&self) -> Option<usize> {
        let p = self.character.pos;
        self.ramp
            .iter()
            .enumerate()
            .map(|(i, r)| (i, (r.plane.pos - p).length()))
            .filter(|(i, d)| *d < Self::ramp_reach(self.ramp[*i].spec))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Get into parked aircraft `i` and take the controls. The player's own
    /// aircraft, if it was left on the ground, is parked in its place.
    pub(crate) fn board_ramp(&mut self, i: usize) {
        let r = self.ramp.swap_remove(i);
        if self.plane_visible && self.plane.on_ground && !self.crashed {
            let (lat, lon, h) = geo::pos_to_lla(self.plane.pos);
            let ground = self.world.ground(lat, lon).map(|g| g.h).unwrap_or(h);
            let yaw = self.focus_yaw_of_plane();
            let mut plane = Plane::spawn_ground(self.spec, lat, lon, ground, yaw, 0.0);
            plane.cold_and_dark();
            self.ramp.push(RampPlane { apt: r.apt, spec: self.spec, plane, at: (lat, lon, ground, yaw, 0.0) });
        }
        let (lat, lon, h, yaw, pitch) = r.at;
        self.take_controls(r.spec, lat, lon, h, yaw, pitch);
    }

    /// The player's aircraft's bearing (radians from north, clockwise).
    fn focus_yaw_of_plane(&self) -> f64 {
        let (e, n, _) = geo::enu(self.plane.pos);
        let f = self.plane.forward();
        f.dot(e).atan2(f.dot(n))
    }

    /// The parked aircraft near the camera, drawn with their own models.
    pub(crate) fn draw_ramp(&mut self, cam: DVec3, objs: &mut Vec<render::DrawObj>) {
        let ramp = std::mem::take(&mut self.ramp);
        for r in &ramp {
            if (r.plane.pos - cam).length() > DRAW_DIST || !self.fleet_ready(r.spec) {
                continue;
            }
            let (model, mesh, cut) = if std::ptr::eq(r.spec, self.spec) {
                (&self.model, &self.piece_mesh, &self.piece_cut)
            } else {
                let Some(l) = self.hangar.iter().find(|l| std::ptr::eq(l.spec, r.spec)) else { continue };
                (&l.model, &l.piece_mesh, &l.piece_cut)
            };
            // (room for all of it, or none: never half an aircraft)
            if objs.len() + model.pieces.len() + 48 > render::MAX_OBJS as usize {
                break;
            }
            let pose = r.plane.pose(&model.pieces);
            let base = Mat4::from_rotation_translation(r.plane.rot.as_quat(), (r.plane.pos - cam).as_vec3());
            let params = [[0.0f32; 4]; 6];
            for (i, p) in model.pieces.iter().enumerate() {
                let hide = match p.vis {
                    airframe::Vis::Always | airframe::Vis::SlowProp => false,
                    airframe::Vis::FastProp | airframe::Vis::Chute => true,
                };
                if hide {
                    continue;
                }
                objs.push(render::DrawObj { mesh: mesh[i], model: base * pose[i], tint: [0.0; 4], glow: [0.0, r.spec.id as f32, 0.0, 0.0], params, cast: 0b11, glass: p.glass, cut: cut[i], ..render::DrawObj::DEFAULT });
            }
        }
        self.ramp = ramp;
    }

    /// The hover prompt for the parked aircraft in reach.
    pub(crate) fn ramp_prompt(&self) -> Option<String> {
        self.ramp_in_reach().map(|i| t!("ground.get_in_named_aircraft", self.ramp[i].spec.name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_parkable_type_fits_some_stand() {
        let types = parkable();
        assert!(!types.is_empty());
        for s in &types {
            let class = size_class(s) as usize;
            assert!(size_class(s) <= 4, "{} fits a wide-body stand", s.name);
            assert!(s.span * 2.0 <= airports::SIZE_SPAN[class] + 5.0, "{} is in the right span class", s.name);
            assert!(nose_ahead(s) > 0.5 && nose_ahead(s) < 80.0, "{} has its nose ahead", s.name);
        }
        // the light singles go on the smallest stands
        assert_eq!(size_class(aircraft::by_key("c172").unwrap()), 0);
        assert_eq!(size_class(aircraft::by_key("fa18").unwrap()), 1);
        assert_eq!(size_class(aircraft::by_key("g650").unwrap()), 3);
        assert_eq!(size_class(aircraft::by_key("b747").unwrap()), 4);
        // the light singles and fighters together are enough for a field's set
        assert!(types.iter().filter(|s| size_class(s) <= 1).count() >= MIN_TYPES);
    }

    fn spot(size: u8) -> Spot {
        Spot { size, pos: Vec2::ZERO, dir: Vec2::X }
    }

    #[test]
    fn every_field_gets_five_different_types() {
        let types = parkable();
        // The spots fill_field can produce: size-0 stands plus class-1
        // overflow, mixed with wider stands.
        let cases: [&[u8]; 8] = [&[1, 1, 0, 0, 0], &[1, 1, 1, 1, 1], &[4, 1, 0, 0, 0, 0], &[2, 2, 0, 0, 0], &[3, 3, 0, 0, 0], &[4, 4, 4, 4, 4], &[1, 1, 1, 1, 1, 1], &[1, 1, 0, 0, 0, 0]];
        for sizes in cases {
            let spots: Vec<Spot> = sizes.iter().map(|&size| spot(size)).collect();
            let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
            let picks = choose_types(&spots, &types, &mut rng);
            assert!(picks.len() >= MIN_TYPES, "only {} types for {sizes:?}", picks.len());
            let mut seen = std::collections::HashSet::new();
            for (spec, si) in picks {
                assert!(seen.insert(spec.keys[0]), "duplicate type {} at one field", spec.name);
                assert!(size_class(spec) <= spots[si].size, "{} does not fit spot {}", spec.name, si);
            }
        }
    }

    #[test]
    fn small_stands_get_enough_overflow() {
        assert_eq!(overflow_needed(&[]), MIN_TYPES);
        assert_eq!(overflow_needed(&[spot(0)]), MIN_TYPES - 1);
        assert_eq!(overflow_needed(&[spot(0); 3]), 2);
        assert_eq!(overflow_needed(&[spot(0); 5]), 2);
        assert_eq!(overflow_needed(&[spot(0); 6]), 2);
        assert_eq!(overflow_needed(&[spot(4), spot(0), spot(0), spot(0), spot(0)]), 1);
        assert_eq!(overflow_needed(&[spot(1), spot(0), spot(0), spot(0), spot(0)]), 1);
        assert_eq!(overflow_needed(&[spot(1); 5]), 0);
    }

    #[test]
    fn every_mix_of_free_stands_gets_five_types() {
        // Enumerate every possible set of free stand sizes a field can offer,
        // apply the same overflow/truncate step fill_field uses, and check the
        // field still gets at least five different flyable types.
        let types = parkable();
        fn walk(free: &mut Vec<Spot>, depth: usize, types: &[&'static aircraft::Spec]) {
            let mut spots = free.clone();
            let extra = overflow_needed(&spots);
            spots.extend((0..extra).map(|_| spot(1)));
            spots.sort_by_key(|s| std::cmp::Reverse(s.size));
            spots.truncate(PER_FIELD);
            for seed in [1u64, 0x9e37_79b9_7f4a_7c15, 0xdead_beef_cafe_f00d] {
                let mut rng = Rng(seed);
                let picks = choose_types(&spots, types, &mut rng);
                assert!(picks.len() >= MIN_TYPES, "free sizes {:?}: only {} types", free.iter().map(|s| s.size).collect::<Vec<_>>(), picks.len());
                let mut seen = std::collections::HashSet::new();
                for (spec, _) in picks {
                    assert!(seen.insert(spec.keys[0]), "duplicate type {}", spec.name);
                }
            }
            if depth < PER_FIELD {
                for size in 0..=4 {
                    free.push(spot(size));
                    walk(free, depth + 1, types);
                    free.pop();
                }
            }
        }
        walk(&mut Vec::new(), 0, &types);
    }

    #[test]
    fn every_strip_has_enough_overflow_spots() {
        // A field with no stands still has room for the minimum set, even
        // when its runway is very short: the row continues past both ends.
        for len in [0.0, 20.0, 50.0, 150.0, 300.0, 1_000.0] {
            let spots = line_spots(Vec2::ZERO, Vec2::X, len, 20.0);
            assert!(spots.len() >= MIN_TYPES, "len {len}: only {} spots", spots.len());
            for (p, d) in spots {
                assert!(p.is_finite() && (d.length() - 1.0).abs() < 1e-4, "len {len}: bad spot {p:?} {d:?}");
            }
        }
    }
}
