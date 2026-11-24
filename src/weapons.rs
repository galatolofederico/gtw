//! Heat-seeking air-to-air missiles.
//!
//! The player's missile system lives here: master arm, the seeker and its
//! lock, the two wingtip Sidewinders, and the missiles themselves once they
//! have left the rails. The flight model and the rest of the aircraft know
//! nothing about them; `main.rs` feeds in the target list each frame and turns
//! the events into sound, particles and damage.
//!
//! A missile is a simple boost-and-coast body: it leaves the wingtip with the
//! aircraft's velocity, burns for a couple of seconds, then drags down while
//! its seeker follows the target's heat. Guidance is lead pursuit with a
//! finite turn rate, so a hard break can defeat it; a proximity fuse ends the
//! flight when it gets close enough.

use crate::aircraft::{MissileStation, Spec};
use crate::airframe::Piece;
use crate::geo;
use crate::world::World;
use glam::{DQuat, DVec3};
use std::f64::consts::FRAC_PI_4;

/// The seeker's cone, off the aircraft's nose (rad).
const LOCK_CONE: f64 = FRAC_PI_4;
/// How long a lock takes to build (s), and how quickly it falls away when the
/// target leaves the cone or range.
const LOCK_TIME: f64 = 1.1;
const LOCK_DECAY: f64 = 2.5;
/// Lock range: a base distance plus a little for each size class of target.
const LOCK_BASE: f64 = 10_000.0;
const LOCK_SIZE: f64 = 1_500.0;
/// The closest a target can be and still be locked (m).
const MIN_RANGE: f64 = 150.0;
/// How long a missile flies before it gives up (s).
const MISSILE_LIFE: f64 = 32.0;
/// The motor (s), its acceleration (m/s²), and the drag coefficient at speed
/// (a = k v²).  The latter is tuned so a missile burns out around Mach 1.5 and
/// then coasts down towards the target.
const BOOST_TIME: f64 = 2.8;
const BOOST_ACC: f64 = 170.0;
const DRAG_K: f64 = 0.00006;
/// The most lateral acceleration the fins can make (g), and the seeker's
/// gimbal limit (rad).  Past the limit the lock is lost.
const MAX_G: f64 = 32.0;
const SEEKER_GIMBAL: f64 = 65.0f64.to_radians();
/// How close a missile must pass to the target's centre to detonate (m).
const FUSE_MARGIN: f64 = 12.0;
const G0: f64 = 9.80665;

/// What an aircraft is, as far as a missile is concerned.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TargetKey {
    /// A live ADS-B aircraft, by its ICAO address.
    Live(String),
    /// A moving aircraft at an airport: which airfield, and the mover's id.
    Airport { apt: u32, mover: u32 },
}

/// One target a seeker may lock onto.
#[derive(Clone, Debug)]
pub struct Target {
    pub key: TargetKey,
    pub pos: DVec3,
    pub vel: DVec3,
    /// Its size class (0 light single .. 4 wide-body), which sets how hot it
    /// looks to the seeker and how big a fuse circle it needs.
    pub size: u8,
    pub label: String,
}

/// The locked target as the HUD wants it.
#[derive(Clone, Debug)]
pub struct TargetView {
    pub pos: DVec3,
    pub label: String,
    pub range: f64,
    pub closure: f64,
}

/// Why a weapon request was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NotArmed,
    NoPower,
    NoLock,
    NoMissiles,
    NoWeapons,
    NoTarget,
}

/// Something the weapon system did, for `main.rs` to turn into a flash, a
/// sound or particles.
#[derive(Clone, Debug)]
pub enum Event {
    /// The seeker has finished building a lock.
    Locked,
    /// The target went out of the seeker's cone/range, or disappeared.
    Lost,
    /// A missile left a wingtip.
    Fired { station: usize },
    /// A request could not be carried out.
    Refused(Refusal),
    /// A missile detonated.  `target` is set when it was the target that
    /// ended the flight (a hit), not the ground or a timeout.
    Blast {
        pos: DVec3,
        power: f32,
        size: f32,
        target: Option<TargetKey>,
    },
}

/// One hardpoint: what it is, the model piece that is the loaded missile, and
/// where that piece's middle is in the body frame.
pub struct Station {
    pub spec: MissileStation,
    pub piece: Option<usize>,
    pub point: DVec3,
    pub loaded: bool,
}

/// A missile in flight.  Body frame is the aircraft's: the nose is -Z.
pub struct Missile {
    pub station: usize,
    pub pos: DVec3,
    pub vel: DVec3,
    pub rot: DQuat,
    pub age: f64,
    /// How hard the motor is burning, 0 after burnout (for the smoke trail).
    pub burn: f32,
    target: Option<TargetKey>,
    alive: bool,
}

pub struct Weapons {
    pub stations: Vec<Station>,
    pub armed: bool,
    /// How complete the lock is, 0..1.
    pub lock: f64,
    pub target: Option<TargetKey>,
    pub target_view: Option<TargetView>,
    pub missiles: Vec<Missile>,
    /// The seeker's tone: 0 silent, 0.1 searching, up to 1 locked.
    pub seeker_tone: f32,
    cycle: bool,
    fire: bool,
    /// A lock event has been sent for the current target.
    locked: bool,
}

impl Weapons {
    pub fn new(spec: &'static Spec, pieces: &[Piece]) -> Self {
        let mut w = Self {
            stations: Vec::new(),
            armed: false,
            lock: 0.0,
            target: None,
            target_view: None,
            missiles: Vec::new(),
            seeker_tone: 0.0,
            cycle: false,
            fire: false,
            locked: false,
        };
        w.reset(spec, pieces);
        w
    }

    /// Back to a fresh aircraft: two missiles, master arm off, no lock.
    pub fn reset(&mut self, spec: &'static Spec, pieces: &[Piece]) {
        self.stations.clear();
        for st in spec.missiles() {
            let piece = pieces.iter().position(|p| p.name == st.piece);
            let point = piece
                .map(|i| pieces[i].pivot.as_dvec3())
                .unwrap_or(DVec3::ZERO);
            self.stations.push(Station {
                spec: *st,
                piece,
                point,
                loaded: true,
            });
        }
        self.armed = false;
        self.lock = 0.0;
        self.target = None;
        self.target_view = None;
        self.missiles.clear();
        self.seeker_tone = 0.0;
        self.cycle = false;
        self.fire = false;
        self.locked = false;
    }

    pub fn has_weapons(&self) -> bool {
        !self.stations.is_empty()
    }

    pub fn station(&self, i: usize) -> Option<&Station> {
        self.stations.get(i)
    }

    pub fn loaded(&self, i: usize) -> bool {
        self.stations.get(i).is_some_and(|s| s.loaded)
    }

    pub fn toggle_arm(&mut self) -> bool {
        self.armed = !self.armed;
        if !self.armed {
            self.clear_lock();
        }
        self.armed
    }

    pub fn request_cycle(&mut self) {
        self.cycle = true;
    }

    pub fn request_fire(&mut self) {
        self.fire = true;
    }

    /// Is the loaded missile of this model piece gone (and so hidden)?
    pub fn hides_piece(&self, name: &str) -> bool {
        self.stations
            .iter()
            .any(|s| s.piece.is_some() && s.spec.piece == name && !s.loaded)
    }

    /// Fly the seeker and every missile on.  `dt` is the fixed physics step;
    /// `powered` is the aircraft's electrical bus.
    pub fn step(
        &mut self,
        dt: f64,
        plane: &crate::plane::Plane,
        targets: &[Target],
        world: &World,
        powered: bool,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let can_seek = self.armed && powered && !self.stations.is_empty();
        if !can_seek {
            self.clear_lock();
            self.seeker_tone = 0.0;
        }

        if std::mem::take(&mut self.cycle) {
            if !self.armed {
                events.push(Event::Refused(Refusal::NotArmed));
            } else if !powered {
                events.push(Event::Refused(Refusal::NoPower));
            } else if self.stations.is_empty() {
                events.push(Event::Refused(Refusal::NoWeapons));
            } else if !self.cycle_target(plane, targets) {
                events.push(Event::Refused(Refusal::NoTarget));
            }
        }

        if can_seek {
            if let Some(key) = self.target.clone() {
                if let Some(t) = targets.iter().find(|t| t.key == key) {
                    let (range, angle) = Self::aim(plane, t);
                    if range >= MIN_RANGE && range <= Self::lock_range(t.size) && angle <= LOCK_CONE
                    {
                        self.lock = (self.lock + dt / LOCK_TIME).min(1.0);
                        self.set_view(t, plane);
                        if self.lock >= 1.0 && !self.locked {
                            self.locked = true;
                            events.push(Event::Locked);
                        }
                    } else {
                        self.lock = (self.lock - dt * LOCK_DECAY).max(0.0);
                        self.set_view(t, plane);
                        if self.lock <= 0.0 {
                            self.target = None;
                            self.target_view = None;
                            self.locked = false;
                            events.push(Event::Lost);
                        }
                    }
                } else {
                    self.target = None;
                    self.lock = 0.0;
                    self.target_view = None;
                    self.locked = false;
                    events.push(Event::Lost);
                }
            } else {
                self.lock = 0.0;
                self.target_view = None;
                self.locked = false;
            }
        }

        if std::mem::take(&mut self.fire) {
            events.extend(self.try_fire(plane, powered));
        }

        for m in &mut self.missiles {
            m.step(dt, targets, world, &mut events);
        }
        self.missiles.retain(|m| m.alive);

        self.seeker_tone = if can_seek {
            if self.target_view.is_some() {
                (0.25 + 0.75 * self.lock) as f32
            } else {
                0.1
            }
        } else {
            0.0
        };
        events
    }

    /// Lock range for a target's size class (m).
    fn lock_range(size: u8) -> f64 {
        LOCK_BASE + LOCK_SIZE * size.min(4) as f64
    }

    /// Range and angle off the nose to a target.
    fn aim(plane: &crate::plane::Plane, t: &Target) -> (f64, f64) {
        let d = t.pos - plane.pos;
        (
            d.length(),
            plane.forward().angle_between(d.normalize_or_zero()),
        )
    }

    fn set_view(&mut self, t: &Target, plane: &crate::plane::Plane) {
        let d = t.pos - plane.pos;
        let range = d.length();
        let los = d.normalize_or_zero();
        self.target_view = Some(TargetView {
            pos: t.pos,
            label: t.label.clone(),
            range,
            closure: (t.vel - plane.vel).dot(los),
        });
    }

    fn clear_lock(&mut self) {
        self.target = None;
        self.target_view = None;
        self.lock = 0.0;
        self.locked = false;
    }

    fn cycle_target(&mut self, plane: &crate::plane::Plane, targets: &[Target]) -> bool {
        let mut cand: Vec<(f64, f64, usize)> = Vec::new();
        for (i, t) in targets.iter().enumerate() {
            let (range, angle) = Self::aim(plane, t);
            if range >= MIN_RANGE && range <= Self::lock_range(t.size) && angle <= LOCK_CONE {
                cand.push((angle, range, i));
            }
        }
        if cand.is_empty() {
            self.clear_lock();
            return false;
        }
        cand.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        let current = self
            .target
            .as_ref()
            .and_then(|k| cand.iter().position(|(_, _, i)| &targets[*i].key == k));
        let pick = current.map_or(0, |i| if cand.len() > 1 { (i + 1) % cand.len() } else { i });
        let t = &targets[cand[pick].2];
        let key = t.key.clone();
        if self.target.as_ref() != Some(&key) {
            self.lock = 0.0;
            self.locked = false;
        }
        self.target = Some(key);
        self.set_view(t, plane);
        true
    }

    fn station_ok(&self, plane: &crate::plane::Plane, i: usize) -> bool {
        let Some(s) = self.stations.get(i) else {
            return false;
        };
        // A wingtip that has been torn away takes its missile with it.
        s.loaded && s.piece.is_some() && plane.dmg.wing[s.spec.side] as f64 + 0.5 >= s.point.x.abs()
    }

    fn try_fire(&mut self, plane: &crate::plane::Plane, powered: bool) -> Vec<Event> {
        if !self.armed {
            return vec![Event::Refused(Refusal::NotArmed)];
        }
        if !powered {
            return vec![Event::Refused(Refusal::NoPower)];
        }
        if self.stations.is_empty() {
            return vec![Event::Refused(Refusal::NoWeapons)];
        }
        if self.lock < 1.0 || self.target.is_none() {
            return vec![Event::Refused(Refusal::NoLock)];
        }
        let Some(i) = (0..self.stations.len()).find(|i| self.station_ok(plane, *i)) else {
            return vec![Event::Refused(Refusal::NoMissiles)];
        };
        let st = &self.stations[i];
        let pos = plane.model_pos() + plane.rot * st.point;
        let vel = plane.vel + plane.forward() * 12.0;
        let rot = plane.rot;
        let target = self.target.clone();
        self.missiles.push(Missile {
            station: i,
            pos,
            vel,
            rot,
            age: 0.0,
            burn: 0.0,
            target,
            alive: true,
        });
        self.stations[i].loaded = false;
        vec![Event::Fired { station: i }]
    }
}

impl Missile {
    fn step(&mut self, dt: f64, targets: &[Target], world: &World, events: &mut Vec<Event>) {
        self.age += dt;
        self.burn = if self.age < BOOST_TIME {
            (1.0 - self.age / BOOST_TIME) as f32
        } else {
            0.0
        };
        if self.age >= MISSILE_LIFE {
            events.push(Event::Blast {
                pos: self.pos,
                power: 0.4,
                size: 0.5,
                target: None,
            });
            self.alive = false;
            return;
        }

        let tgt = self
            .target
            .as_ref()
            .and_then(|k| targets.iter().find(|t| &t.key == k));
        self.fly(dt, tgt);

        if let Some(t) = tgt {
            let d = (t.pos - self.pos).length();
            let radius = target_radius(t.size);
            if d <= radius + FUSE_MARGIN {
                events.push(Event::Blast {
                    pos: self.pos,
                    power: blast_power(t.size),
                    size: t.size as f32,
                    target: Some(t.key.clone()),
                });
                self.alive = false;
                return;
            }
        }

        // The ground (or the sea) ends it, too.
        let (lat, lon, alt) = geo::pos_to_lla(self.pos);
        if let Some(g) = world.ground(lat, lon) {
            if alt <= g.h + 0.6 {
                events.push(Event::Blast {
                    pos: geo::lla_to_pos(lat, lon, g.h),
                    power: 0.8,
                    size: 0.6,
                    target: None,
                });
                self.alive = false;
            }
        } else if alt < 0.0 {
            events.push(Event::Blast {
                pos: self.pos,
                power: 0.6,
                size: 0.5,
                target: None,
            });
            self.alive = false;
        }
    }

    /// One step of guidance and motion, without the hit and ground checks.
    fn fly(&mut self, dt: f64, tgt: Option<&Target>) {
        if let Some(t) = tgt {
            if !self.steer(dt, t) {
                // The target has slid out of the seeker's gimbal: the missile
                // flies on, but it is no longer guiding. (The fuse still gets
                // its chance this step.)
                self.target = None;
            }
        }

        let speed = self.vel.length();
        let mut acc = self.vel.normalize_or_zero()
            * if self.age < BOOST_TIME {
                BOOST_ACC
            } else {
                0.0
            };
        if speed > 1.0 {
            acc -= self.vel / speed * (DRAG_K * speed * speed);
        }
        acc -= self.pos.normalize() * G0;
        self.vel += acc * dt;
        let s = self.vel.length();
        if s > 1_000.0 {
            self.vel *= 1_000.0 / s;
        }
        self.pos += self.vel * dt;
        // The model follows the flight path (the missile is symmetric, so its
        // roll does not matter).
        if self.vel.length_squared() > 1.0 {
            self.rot = DQuat::from_rotation_arc(DVec3::NEG_Z, self.vel.normalize());
        }
    }

    /// Turn the flight path towards the target's lead point. The fins bend
    /// the velocity itself, not just the model, so this works after burnout
    /// too. Returns false when the target is outside the seeker's gimbal.
    fn steer(&mut self, dt: f64, t: &Target) -> bool {
        let los = t.pos - self.pos;
        let dist = los.length();
        let los_hat = los.normalize_or_zero();
        let speed = self.vel.length();
        let angle = self.vel.normalize_or_zero().angle_between(los_hat);
        if angle > SEEKER_GIMBAL || dist <= 1.0 || speed <= 1.0 {
            return false;
        }
        // Lead the target: where it will be when the missile can get there,
        // solved a few times because the range changes too.
        let closing = (speed + t.vel.dot(los_hat)).max(150.0);
        let mut t_go = dist / closing;
        for _ in 0..3 {
            t_go = (t.pos + t.vel * t_go - self.pos).length() / closing;
        }
        let aim = t.pos + t.vel * t_go.clamp(0.0, 15.0);
        let desired = (aim - self.pos).normalize_or_zero();
        let cur = self.vel / speed;
        let max_rate = MAX_G * G0 / speed.max(100.0);
        let turn = (max_rate * dt).min(cur.angle_between(desired));
        let axis = cur.cross(desired).normalize_or_zero();
        if axis != DVec3::ZERO && turn > 0.0 {
            self.vel = DQuat::from_axis_angle(axis, turn) * self.vel;
        }
        true
    }
}

/// How far from a target's centre its fuse should fire (m): its half span,
/// with a little room to spare.
fn target_radius(size: u8) -> f64 {
    let ty = &crate::airmodels::TYPES[size.min(4) as usize];
    (ty.span as f64 * 0.5).max(5.0)
}

fn blast_power(size: u8) -> f32 {
    (1.0 + 0.25 * size.min(4) as f32).min(2.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airframe::{Anim, Mesh, Piece, Role, Vis};
    use glam::Vec3;

    fn sidewinder(name: &'static str, x: f32) -> Piece {
        Piece {
            hotspot: None,
            name,
            group: crate::airframe::G_LWING,
            mesh: Mesh::default(),
            parent: None,
            pivot: Vec3::new(x, 0.0, 1.4),
            axis: Vec3::X,
            anim: Anim::Fixed,
            role: Role::Other,
            vis: Vis::Always,
            glass: false,
            occluder: false,
        }
    }

    fn hornet_with_missiles() -> (crate::plane::Plane, Vec<Piece>) {
        let pieces = vec![
            sidewinder("left sidewinder", -5.9),
            sidewinder("right sidewinder", 5.9),
        ];
        let plane =
            crate::plane::Plane::spawn(&crate::aircraft::fa18::SPEC, 0.0, 0.0, 1_000.0, 0.0, 200.0);
        (plane, pieces)
    }

    #[test]
    fn a_target_in_front_locks_and_fires() {
        let (plane, pieces) = hornet_with_missiles();
        let mut w = Weapons::new(&crate::aircraft::fa18::SPEC, &pieces);
        w.armed = true;
        let target = Target {
            key: TargetKey::Live("TEST123".into()),
            pos: plane.pos + plane.forward() * 5_000.0,
            vel: plane.vel,
            size: 2,
            label: "TEST123".into(),
        };
        assert!(w.cycle_target(&plane, std::slice::from_ref(&target)));
        w.lock = 1.0;
        let events = w.try_fire(&plane, true);
        assert!(matches!(events.as_slice(), [Event::Fired { station: 0 }]));
        assert!(!w.loaded(0));
        assert_eq!(w.missiles.len(), 1);
    }

    #[test]
    fn safe_aircraft_refuses_to_fire() {
        let (plane, pieces) = hornet_with_missiles();
        let mut w = Weapons::new(&crate::aircraft::fa18::SPEC, &pieces);
        w.armed = false;
        w.lock = 1.0;
        let events = w.try_fire(&plane, true);
        assert!(matches!(
            events.as_slice(),
            [Event::Refused(Refusal::NotArmed)]
        ));
    }

    #[test]
    fn a_second_shot_uses_the_other_wingtip() {
        let (plane, pieces) = hornet_with_missiles();
        let mut w = Weapons::new(&crate::aircraft::fa18::SPEC, &pieces);
        w.armed = true;
        let target = Target {
            key: TargetKey::Live("TEST123".into()),
            pos: plane.pos + plane.forward() * 5_000.0,
            vel: plane.vel,
            size: 2,
            label: "TEST123".into(),
        };
        w.target = Some(target.key.clone());
        w.lock = 1.0;
        assert!(matches!(
            w.try_fire(&plane, true).as_slice(),
            [Event::Fired { station: 0 }]
        ));
        assert!(matches!(
            w.try_fire(&plane, true).as_slice(),
            [Event::Fired { station: 1 }]
        ));
        assert!(matches!(
            w.try_fire(&plane, true).as_slice(),
            [Event::Refused(Refusal::NoMissiles)]
        ));
    }

    #[test]
    fn guided_missile_closes_on_a_target() {
        let start = geo::lla_to_pos(0.0, 0.0, 1_000.0);
        let mut m = Missile {
            station: 0,
            pos: start,
            vel: DVec3::X * 200.0,
            rot: DQuat::from_rotation_arc(DVec3::NEG_Z, DVec3::X),
            age: 0.0,
            burn: 0.0,
            target: None,
            alive: true,
        };
        let mut t = Target {
            key: TargetKey::Live("LEAD".into()),
            pos: start + DVec3::new(2_000.0, 0.0, -1_000.0),
            vel: DVec3::NEG_Z * 200.0,
            size: 2,
            label: "LEAD".into(),
        };
        let dt = 1.0 / 180.0;
        let mut closest = (t.pos - m.pos).length();
        for _ in 0..(20.0 / dt) as usize {
            m.age += dt;
            m.fly(dt, Some(&t));
            t.pos += t.vel * dt;
            closest = closest.min((t.pos - m.pos).length());
        }
        assert!(closest < target_radius(2) + FUSE_MARGIN, "missed by {closest:.0} m");
    }

    #[test]
    fn guidance_turns_the_flight_path_after_burnout() {
        let mut m = Missile {
            station: 0,
            pos: DVec3::ZERO,
            vel: DVec3::X * 300.0,
            rot: DQuat::IDENTITY,
            age: 10.0,
            burn: 0.0,
            target: None,
            alive: true,
        };
        let target = Target {
            key: TargetKey::Live("TURN".into()),
            pos: DVec3::new(1_000.0, 0.0, -1_000.0),
            vel: DVec3::ZERO,
            size: 2,
            label: "TURN".into(),
        };
        for _ in 0..180 {
            assert!(m.steer(1.0 / 180.0, &target));
        }
        // It started flying +X; a second of 32 g at 300 m/s should have bent
        // the flight path well towards -Z.
        assert!(m.vel.z < -100.0, "{:?}", m.vel);
    }

    #[test]
    fn lock_range_grows_with_the_target() {
        assert!(Weapons::lock_range(4) > Weapons::lock_range(0));
        assert!((Weapons::lock_range(0) - LOCK_BASE).abs() < 1e-9);
    }

    #[test]
    fn a_big_aircraft_needs_a_bigger_fuse() {
        assert!(target_radius(4) > target_radius(0));
        assert!(blast_power(4) > blast_power(0));
    }

    #[test]
    fn the_hornet_has_two_stations() {
        let w = Weapons::new(&crate::aircraft::fa18::SPEC, &[]);
        assert_eq!(w.stations.len(), 2);
        assert!(w.has_weapons());
        assert!(w.stations.iter().all(|s| s.loaded));
    }
}
