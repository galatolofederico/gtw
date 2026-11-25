//! The ramp: the game's own flyable types parked at the airfields near the
//! player, drawn with their real models, to walk up to and take (the
//! interact button, Y on the pad, as for a car). They fill stands the airport
//! traffic leaves free (kept from it while they stand there); a field with
//! no stands gets a few light aircraft on the grass beside its longest
//! runway. They are scenery until boarded: not simulated, not saved (a field
//! gets a fresh random set each time the player comes back to it), and,
//! like the traffic's parked aircraft, not solid to the character or the car.
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

/// One parked aircraft of a type the player can fly.
pub(crate) struct RampPlane {
    pub apt: u32,
    pub spec: &'static aircraft::Spec,
    /// a stand-in, posed on its gear (never stepped)
    pub plane: Plane,
    /// where it stands: latitude, longitude (radians), ground height (m), bearing and slope (radians)
    pub at: (f64, f64, f64, f64, f64),
}

/// The size of stand a type needs, as `airports::Stand::size` counts them
/// (0 light single ... 4 wide-body), from its wingspan.
fn size_class(spec: &aircraft::Spec) -> u8 {
    match spec.span * 2.0 {
        s if s <= 15.0 => 0,
        s if s <= 32.0 => 1,
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

/// Spots for light aircraft on a field with no stands: on the grass to the
/// right of its longest runway, noses towards it. (nose point, facing)
fn grass_spots(g: &airports::Geo) -> Vec<(Vec2, Vec2)> {
    let Some(r) = g.runways.iter().max_by(|a, b| a.len.total_cmp(&b.len)) else { return Vec::new() };
    if r.len < 300.0 {
        return Vec::new();
    }
    let side = airports::right_of(r.dir);
    [0.22f32, 0.3, 0.38].iter().map(|f| (r.a + r.dir * (r.len * f) + side * (r.width * 0.5 + 32.0), -side)).collect()
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

    /// A random set of the player's types at one field's free stands.
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
        let mut spots: Vec<(Option<usize>, u8, Vec2, Vec2)> = free.iter().take(PER_FIELD).filter_map(|&si| g.stands.get(si).map(|s| (Some(si), s.size, s.pos, s.dir))).collect();
        if g.stands.is_empty() {
            spots.extend(grass_spots(&g).into_iter().map(|(p, d)| (None, 0, p, d)));
        }
        let before = self.ramp.len();
        for (stand, size, nose_at, dir) in spots {
            let fits: Vec<_> = types.iter().copied().filter(|s| size_class(s) <= size).collect();
            if fits.is_empty() {
                continue;
            }
            let spec = fits[rng.pick(fits.len())];
            let lp = nose_at - dir * nose_ahead(spec);
            let h = g.h(lp) as f64;
            let slope = (g.h(lp + dir * 3.0) - g.h(lp - dir * 3.0)) as f64 / 6.0;
            let (lat, lon) = g.lla(lp);
            let yaw = (dir.x as f64).atan2(dir.y as f64);
            let mut plane = Plane::spawn_ground(spec, lat, lon, h, yaw, slope.atan());
            plane.cold_and_dark();
            if let Some(si) = stand {
                self.world.reserve_stand(apt, si);
            }
            self.ramp.push(RampPlane { apt, spec, plane, at: (lat, lon, h, yaw, slope.atan()) });
        }
        let names: Vec<_> = self.ramp[before..].iter().map(|r| r.spec.keys[0]).collect();
        log::info!("ramp: {} parked at {} ({} stands free): {}", names.len(), g.apt().code(), free.len(), names.join(", "));
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
            assert!(size_class(s) <= 4, "{} fits a wide-body stand", s.name);
            assert!(nose_ahead(s) > 0.5 && nose_ahead(s) < 80.0, "{} has its nose ahead", s.name);
        }
        // the light singles go on the smallest stands
        assert_eq!(size_class(aircraft::by_key("c172").unwrap()), 0);
        assert_eq!(size_class(aircraft::by_key("b747").unwrap()), 4);
    }
}
