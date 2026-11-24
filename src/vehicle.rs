//! The player's car: single-body arcade physics.
//!
//! GTW v1 drives one rigid body with four visual wheels. There is no per-wheel
//! suspension or tyre model: the body samples the ground at four points for its
//! height, slope and surface class, then longitudinal forces (engine, brakes,
//! rolling resistance, drag), a lateral grip circle and a bicycle-model steering
//! angle move it. The handbrake drops the lateral grip so the tail steps out.
//!
//! All ground information comes from [`WorldQuery`], so the controller runs
//! without a GPU, a streamed world or a network: the tests drive it over
//! synthetic flat, slope, water and wall worlds. The tuning table covers the
//! eight existing traffic models (`cars::N_MODELS`), and damage is persistent
//! for the life of the car.

use crate::cars::{self, CAR_LEN};
use crate::character;
use crate::geo;
use crate::surface::{Surface, SurfaceWeather};
use crate::world::{TrafficBody, WorldQuery};
use glam::DVec3;

/// Downward acceleration (m/s²).
pub const GRAVITY: f64 = 9.81;
/// Air density at sea level (kg/m³).
const RHO: f64 = 1.225;
/// Physics substep (s): 120 Hz, as the plan requires.
pub const SUBSTEP: f64 = 1.0 / 120.0;
/// The most time one call integrates (s).
const MAX_DT: f64 = 0.25;
/// The farthest the body may be moved in one substep (m), against tunnelling.
const MAX_MOVE: f64 = 4.0;
/// Full vehicle health.
pub const MAX_HEALTH: f32 = 100.0;
/// Below this health the engine smokes. Kept for when car health returns.
#[allow(dead_code)]
pub const SMOKE_HEALTH: f32 = 40.0;
/// An impact slower than this does no damage (m/s).
pub const SAFE_IMPACT: f64 = 2.0;
/// Health lost per m/s of impact past [`SAFE_IMPACT`].
pub const IMPACT_DAMAGE: f32 = 2.2;
/// Whether the player's car can be hurt, smoke or be wrecked. Off for now:
/// collisions still shove it and the traffic, but never damage it.
pub const CAR_HEALTH_ENABLED: bool = false;
/// How much of the closing speed two cars give back when they part (0..1).
const RESTITUTION: f64 = 0.15;
/// The steepest slope the tyres hold (degrees); beyond it the body slides.
pub const SLOPE_LIMIT: f64 = 55.0;
/// Top gear.
const TOP_GEAR: i8 = 5;
/// Engine speed to change up (rpm).
const UPSHIFT: f64 = 4400.0;
/// … and down.
const DOWNSHIFT: f64 = 1200.0;

/// Which wheels the engine drives.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Drive {
    Fwd,
    Rwd,
    Awd,
}

impl Drive {
    /// Share of the weight the driven wheels carry.
    fn share(self) -> f64 {
        match self {
            Drive::Awd => 0.95,
            Drive::Rwd => 0.55,
            Drive::Fwd => 0.48,
        }
    }
}

/// One model's arcade tuning.
#[derive(Copy, Clone, Debug)]
pub struct Tuning {
    /// Kerb mass (kg).
    pub mass: f64,
    /// Peak engine power (W).
    pub power: f64,
    pub drive: Drive,
    /// Level-road top speed (m/s).
    pub top: f64,
    /// Tyre grip factor, on top of the surface's own.
    pub grip: f64,
    /// Forward gear ratios, first to fifth.
    pub ratios: [f64; 5],
    /// Reverse gear ratio.
    pub reverse: f64,
    /// Final drive ratio.
    pub final_drive: f64,
    /// Tyre radius (m).
    pub wheel_r: f64,
    /// Wheelbase (m).
    pub wheelbase: f64,
}

/// The tuning of each of the eight models, in `cars::CAR_LEN` order: sedan,
/// compact, SUV, pickup, van, coupé, bus and lorry. The figures are plausible
/// arcade values for those classes, not measurements of particular vehicles.
pub fn tuning(model: u8) -> Tuning {
    let base = Tuning {
        mass: 1500.0,
        power: 110_000.0,
        drive: Drive::Fwd,
        top: 52.0,
        grip: 0.90,
        ratios: [3.50, 2.10, 1.40, 1.00, 0.80],
        reverse: 3.30,
        final_drive: 3.90,
        wheel_r: 0.33,
        wheelbase: 2.9,
    };
    match model.min(cars::N_MODELS as u8 - 1) {
        1 => Tuning { mass: 1050.0, power: 70_000.0, top: 44.0, grip: 0.88, wheelbase: 2.5, ..base },
        2 => Tuning { mass: 2000.0, power: 140_000.0, drive: Drive::Awd, top: 47.0, grip: 0.92, wheel_r: 0.38, wheelbase: 3.0, ..base },
        3 => Tuning { mass: 2200.0, power: 150_000.0, drive: Drive::Rwd, top: 46.0, grip: 0.86, wheel_r: 0.40, wheelbase: 3.4, ..base },
        4 => Tuning { mass: 2100.0, power: 120_000.0, top: 42.0, grip: 0.85, wheel_r: 0.36, wheelbase: 3.2, ..base },
        5 => Tuning { mass: 1200.0, power: 180_000.0, drive: Drive::Rwd, top: 62.0, grip: 0.98, wheel_r: 0.34, wheelbase: 2.6, ratios: [3.30, 2.00, 1.35, 0.98, 0.76], ..base },
        6 => Tuning { mass: 12_000.0, power: 220_000.0, drive: Drive::Rwd, top: 28.0, grip: 0.80, ratios: [4.50, 2.60, 1.60, 1.00, 0.72], reverse: 4.2, wheel_r: 0.50, wheelbase: 6.0, ..base },
        7 => Tuning { mass: 7500.0, power: 200_000.0, drive: Drive::Rwd, top: 33.0, grip: 0.82, ratios: [4.80, 2.80, 1.70, 1.00, 0.75], reverse: 4.5, wheel_r: 0.46, wheelbase: 4.4, ..base },
        _ => base,
    }
}

/// What the driver is asking for this step. `throttle` and `brake` are 0..1,
/// `steer` −1 left to +1 right.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct VehicleControls {
    pub throttle: f64,
    pub brake: f64,
    pub steer: f64,
    pub handbrake: bool,
}

/// Persistent damage. Health only falls, so a car wears its damage for life.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct VehicleDamage {
    /// Hit points, 0..[`MAX_HEALTH`].
    pub health: f32,
    /// Share of engine power still available, 0..1.
    pub engine: f32,
    /// Steering bias, −1 left to +1 right.
    pub steer_pull: f32,
    /// Extra rolling resistance from a dragging wheel.
    pub wheel_drag: f32,
    /// How far the body has crumpled, 0..1 (the shader's deformation).
    pub crumple: f32,
}

impl Default for VehicleDamage {
    fn default() -> Self {
        Self::intact()
    }
}

impl VehicleDamage {
    pub fn intact() -> Self {
        Self { health: MAX_HEALTH, engine: 1.0, steer_pull: 0.0, wheel_drag: 0.0, crumple: 0.0 }
    }

    /// Take `amount` hit points, the blow coming from the side `side`    /// (+1 from the right, −1 from the left).
    pub fn hurt(&mut self, amount: f32, side: f32) {
        if amount <= 0.0 || self.health <= 0.0 {
            return;
        }
        self.health = (self.health - amount).max(0.0);
        let frac = self.health / MAX_HEALTH;
        self.crumple = (1.0 - frac).clamp(0.0, 1.0);
        // Engine damage only ever gets worse: a drowned engine stays drowned.
        let derived = 0.35 + 0.65 * frac;
        self.engine = if self.health <= 0.0 { 0.0 } else { self.engine.min(derived) };
        self.wheel_drag = (1.0 - frac) * 0.06;
        if amount > 3.0 {
            self.steer_pull = (self.steer_pull + side.signum() * 0.06).clamp(-0.22, 0.22);
        }
    }

    pub fn wrecked(&self) -> bool {
        self.health <= 0.0
    }

    #[allow(dead_code)]
    pub fn smoking(&self) -> bool {
        self.health > 0.0 && self.health < SMOKE_HEALTH
    }
}

/// One collision, for the HUD and the sound queue.
#[derive(Copy, Clone, Debug)]
#[allow(dead_code)]
pub struct Impact {
    pub point: DVec3,
    /// Outward normal of what was hit.
    pub normal: DVec3,
    /// Closing speed (m/s).
    pub speed: f64,
    pub damage: f32,
}

/// A collision with a traffic car: the source body, so the traffic state can be
/// slowed and damaged too, and what the HUD and (in a later phase) the sound
/// queue need. The player's step owns the queue; `Game` drains it.
#[derive(Copy, Clone, Debug)]
#[allow(dead_code)]
pub struct TrafficHit {
    pub body: TrafficBody,
    pub point: DVec3,
    /// Closing speed along the contact normal (m/s).
    pub speed: f64,
    pub damage: f32,
    /// Velocity change given to the traffic car, in the planet frame (m/s).
    pub velocity: DVec3,
    /// How far the traffic car is shoved clear of the overlap (m).
    pub shove: DVec3,
    /// Whether this hit should trigger HUD flash and crash sound.
    pub notify: bool,
}

/// What one step did.
#[derive(Clone, Debug, Default)]
pub struct VehicleStep {
    pub pos: DVec3,
    pub yaw: f64,
    /// Signed forward speed (m/s).
    pub speed: f64,
    /// Sideways speed: positive slides right (m/s).
    pub lateral: f64,
    pub gear: i8,
    pub rpm: f64,
    pub steer: f64,
    pub grounded: bool,
    pub water: bool,
    /// Wheel rotation (radians) for the visual wheels.
    pub wheel_spin: f64,
    /// Cosmetic body roll and pitch (radians).
    pub roll: f32,
    pub pitch: f32,
    /// Damage taken this step.
    pub damage: f32,
    pub wrecked: bool,
    pub impacts: Vec<Impact>,
    /// Traffic hit this step, for the HUD, the screen flash and the audio queue.
    pub traffic_hits: Vec<TrafficHit>,
}

/// The car: one body in the planet frame.
#[derive(Clone, Debug)]
pub struct Vehicle {
    pub model: u8,
    pub colour: [f32; 3],
    /// Ground-contact position in the planet frame.
    pub pos: DVec3,
    pub vel: DVec3,
    /// Facing as a bearing: radians from north, clockwise.
    pub yaw: f64,
    /// Forward gear 1..5, or −1 in reverse.
    pub gear: i8,
    pub damage: VehicleDamage,
    /// The driver's steering, −1 left to +1 right, as last applied.
    pub steer: f64,
    pub grounded: bool,
    pub water: bool,
    /// Wheel rotation (radians).
    pub wheel_spin: f64,
    pub roll: f32,
    pub pitch: f32,
    /// Engine speed (rpm) and the driver's inputs, kept for the sound mix.
    pub rpm: f64,
    pub load: f32,
    pub brake: f32,
    pub handbrake: bool,
    /// Distance driven this life (m).
    pub distance: f64,
    /// Collision box (half width, half length) and drawn size (m).
    extent: [f64; 2],
    length: f64,
    width_m: f64,
    tuning: Tuning,
    /// Aerodynamic drag area (m²) derived from the tuning's top speed.
    cda: f64,
    /// Landing damage is forgiven while the car has just appeared.
    spawn_grace: f64,
    /// A short pause after a traffic impact, so a car shunting the player does
    /// not deal damage every substep.
    traffic_cool: f64,
}

impl Vehicle {
    /// A car of `model` (`0..8`) in `colour`, sitting at `pos` (ground
    /// contact) facing `yaw` (bearing radians).
    pub fn spawn(model: u8, colour: [f32; 3], pos: DVec3, yaw: f64) -> Self {
        let model = model.min(cars::N_MODELS as u8 - 1);
        let tuning = tuning(model);
        let pos = if pos.is_finite() { pos } else { geo::lla_to_pos(0.0, 0.0, 0.0) };
        let yaw = if yaw.is_finite() { yaw } else { 0.0 };
        let cda = drag_area(&tuning);
        let g = cars::player_geom(model);
        Self { model, colour, pos, vel: DVec3::ZERO, yaw, gear: 1, damage: VehicleDamage::intact(), steer: 0.0, grounded: false, water: false, wheel_spin: 0.0, roll: 0.0, pitch: 0.0, rpm: 0.0, load: 0.0, brake: 0.0, handbrake: false, distance: 0.0, extent: g.half, length: CAR_LEN[model as usize] as f64, width_m: g.half[0] * 2.0, tuning, cda, spawn_grace: 1.0, traffic_cool: 0.0 }
    }

    /// Length and width for the collision box and the drawn meshes (m).
    pub fn len(&self) -> f64 {
        self.length
    }

    pub fn width(&self) -> f64 {
        self.width_m
    }

    pub fn half(&self) -> [f64; 2] {
        self.extent
    }

    pub fn speed(&self) -> f64 {
        self.longitudinal()
    }

    /// The forward direction as a unit vector in the planet frame.
    pub fn forward(&self) -> DVec3 {
        if !self.pos.is_finite() || self.pos.length_squared() < 1.0 {
            return DVec3::Y;
        }
        let (e, n, _) = geo::enu(self.pos);
        e * self.yaw.sin() + n * self.yaw.cos()
    }

    pub fn right(&self) -> DVec3 {
        self.forward().cross(self.pos.normalize()).normalize_or_zero()
    }

    pub fn up(&self) -> DVec3 {
        self.pos.normalize()
    }

    /// Signed longitudinal speed (m/s).
    pub fn longitudinal(&self) -> f64 {
        self.vel.dot(self.forward())
    }

    /// Sideways speed (m/s).
    pub fn lateral(&self) -> f64 {
        self.vel.dot(self.right())
    }

    /// Stop where it is (getting out).
    pub fn stop(&mut self) {
        self.vel = DVec3::ZERO;
        self.gear = 1;
        self.roll = 0.0;
        self.pitch = 0.0;
        self.rpm = 0.0;
        self.load = 0.0;
        self.brake = 0.0;
        self.handbrake = false;
    }

    /// How fast the engine should be turning in `gear` at `v` m/s.
    fn rpm_for(&self, gear: i8, v: f64) -> f64 {
        let ratio = if gear < 0 {
            self.tuning.reverse
        } else {
            self.tuning.ratios[(gear.clamp(1, TOP_GEAR) - 1) as usize]
        };
        (v.abs() / self.tuning.wheel_r.max(0.1) * self.tuning.final_drive * ratio * 60.0 / std::f64::consts::TAU).clamp(0.0, 7200.0)
    }

    /// Advance the car by `dt` seconds. `traffic` are the cars within reach of
    /// it, as solid oriented boxes.
    pub fn step<W: WorldQuery>(&mut self, dt: f64, c: &VehicleControls, world: &W, wx: &SurfaceWeather, traffic: &[TrafficBody]) -> VehicleStep {
        let mut out = VehicleStep { gear: self.gear, ..Default::default() };
        let total = if dt.is_finite() { dt.clamp(0.0, MAX_DT) } else { 0.0 };
        if total > 0.0 {
            let n = ((total / SUBSTEP).ceil() as usize).clamp(1, 32);
            let h = total / n as f64;
            for _ in 0..n {
                self.substep(h, c, world, wx, traffic, &mut out);
            }
        } else {
            self.settle(world);
        }
        out.pos = self.pos;
        out.yaw = self.yaw;
        out.speed = self.longitudinal();
        out.lateral = self.lateral();
        out.gear = self.gear;
        out.rpm = self.rpm_for(self.gear, out.speed);
        self.rpm = out.rpm;
        self.load = (if self.gear < 0 { c.brake } else { c.throttle }).clamp(0.0, 1.0) as f32;
        self.brake = (if self.gear < 0 { c.throttle } else { c.brake }).clamp(0.0, 1.0) as f32;
        self.handbrake = c.handbrake;
        out.steer = self.steer;
        out.grounded = self.grounded;
        out.water = self.water;
        out.wheel_spin = self.wheel_spin;
        out.roll = self.roll;
        out.pitch = self.pitch;
        out.wrecked = self.damage.wrecked();
        out
    }

    /// No time passed: keep the wheels on the ground under them.
    fn settle<W: WorldQuery>(&mut self, world: &W) {
        if let Some(s) = self.sample(world) {
            let up = self.up();
            self.pos = up * (geo::R + s.h);
            self.vel = DVec3::ZERO;
            self.grounded = true;
            self.water = s.water;
        }
    }

    /// One substep of at most 1/120 s.
    fn substep<W: WorldQuery>(&mut self, h: f64, c: &VehicleControls, world: &W, wx: &SurfaceWeather, traffic: &[TrafficBody], out: &mut VehicleStep) {
        self.spawn_grace = (self.spawn_grace - h).max(0.0);
        self.traffic_cool = (self.traffic_cool - h).max(0.0);
        if !self.pos.is_finite() {
            self.pos = geo::lla_to_pos(0.0, 0.0, 0.0);
            self.vel = DVec3::ZERO;
        }
        let up = self.up();
        let sample = self.sample(world);
        let (ground_h, normal, water, surface) = match sample {
            Some(s) => (s.h, s.normal, s.water, s.kind),
            // A missing tile: hold what we have, do not fall through the world.
            None => (self.pos.length() - geo::R, up, self.water, Surface::Grass),
        };
        self.water = water;
        if water {
            // A car in water is not buoyant: the engine drowns and it drags to a stop.
            self.damage.engine = 0.0;
        } else if !CAR_HEALTH_ENABLED && !self.damage.wrecked() {
            self.damage.engine = 1.0;
        }
        let mu = (surface.grip().max(0.05) as f64) * wx.grip_factor(surface).max(0.05) as f64 * self.tuning.grip;
        let slope = normal.dot(up).clamp(-1.0, 1.0).acos().to_degrees();
        let steep = slope > SLOPE_LIMIT;

        let mut fwd = self.forward();
        let mut right = fwd.cross(up).normalize_or_zero();
        let mut v_long = self.vel.dot(fwd);
        let mut v_lat;
        let wrecked = self.damage.wrecked();
        let mut a_long = 0.0;

        if self.grounded && !steep {
            // Steering: a bicycle model, its angle reduced with speed.
            let steer = (c.steer.clamp(-1.0, 1.0) + self.damage.steer_pull as f64).clamp(-1.0, 1.0);
            self.steer = steer;
            let max_angle = 0.55 / (1.0 + v_long.abs() * 0.055);
            let angle = steer * max_angle;
            let mut yaw_rate = v_long * angle.tan() / self.tuning.wheelbase.max(0.5);
            // The tyres can only turn the car as fast as they can hold it;
            // with the rear locked the car rotates past that a little.
            let cap = mu * GRAVITY / v_long.abs().max(2.0);
            let cap = if c.handbrake { (cap * 1.6).max(0.9) } else { cap };
            yaw_rate = yaw_rate.clamp(-cap, cap);
            self.yaw = (self.yaw + yaw_rate * h).rem_euclid(std::f64::consts::TAU);
            fwd = self.forward();
            right = fwd.cross(up).normalize_or_zero();
            v_long = self.vel.dot(fwd);
            v_lat = self.vel.dot(right);

            // Gravity along the surface.
            let g = -up * GRAVITY;
            let g_slope = g - normal * g.dot(normal);
            let slope_long = g_slope.dot(fwd);
            let slope_lat = g_slope.dot(right);

            // Engine, brakes and resistance.
            let share = self.tuning.drive.share();
            let traction = mu * self.tuning.mass * GRAVITY * share;
            let drive_gear = if self.gear < 0 { c.brake } else { c.throttle };
            let engine = if wrecked || water || self.damage.engine <= 0.0 { 0.0 } else { self.tuning.power * self.damage.engine as f64 };
            let mut force = 0.0;
            if drive_gear > 0.01 {
                // A broad torque curve, normalised so the tuning's top speed
                // still comes out right at the top gear.
                let rpm = self.rpm_for(self.gear.max(1), v_long);
                let peak = torque_curve(self.rpm_for(TOP_GEAR, self.tuning.top)).max(0.1);
                let curve = (torque_curve(rpm) / peak).clamp(0.45, 1.15);
                let f = (engine * curve / v_long.abs().max(2.0)).min(traction);
                force = if self.gear < 0 { -f * drive_gear } else { f * drive_gear };
            }
            let brake_in = if self.gear < 0 { c.throttle } else { c.brake };
            let brake_force = brake_in * mu * self.tuning.mass * GRAVITY * 0.85;
            let hand_force = if c.handbrake { mu * self.tuning.mass * GRAVITY * 0.18 } else { 0.0 };
            let rr = (surface.rolling() as f64 + self.damage.wheel_drag as f64) * self.tuning.mass * GRAVITY;
            let drag = 0.5 * RHO * self.cda * v_long * v_long;
            let water_drag = if water { 5.0 * self.tuning.mass } else { 0.0 };
            let resist = (rr + drag + brake_force + hand_force + water_drag) / self.tuning.mass;
            // Drive, gravity and resistance: resistance never drives the car
            // backwards, and the brakes cannot engage reverse on their own.
            let v_drive = v_long + (force / self.tuning.mass + slope_long) * h;
            a_long = (v_drive - v_long) / h;
            let dir = v_drive.signum();
            v_long = (v_drive.abs() - resist * h).max(0.0) * dir;

            // Lateral grip: cancel sideways motion up to the grip circle,
            // then let the rest slide.
            v_lat += slope_lat * h;
            let grip = mu * GRAVITY * if c.handbrake { 0.45 } else { 1.0 };
            let max_change = grip * h;
            let slid = (v_lat - v_lat.clamp(-max_change, max_change)).abs();
            v_lat -= v_lat.clamp(-max_change, max_change);
            if c.handbrake && slid > 0.01 {
                // Scrubbing sideways slows the car: a little, per second.
                v_long -= v_long.signum() * slid * 1.5 * h;
            }
            // Keep the slide bounded, so a long drift does not turn into a
            // spin: past about a right angle the tyres bite again.
            let slip_cap = v_long.abs() * 0.9 + 2.0;
            if v_lat.abs() > slip_cap {
                v_lat = v_lat.signum() * slip_cap;
            }
            // Gearbox: change up and down on engine speed, and reach reverse
            // by holding the brake at a standstill.
            if self.gear >= 1 {
                if v_long < 0.35 && c.brake > 0.25 && c.throttle < 0.1 && !wrecked {
                    self.gear = -1;
                } else {
                    let rpm = self.rpm_for(self.gear, v_long);
                    if rpm > UPSHIFT && self.gear < TOP_GEAR {
                        self.gear += 1;
                    } else if rpm < DOWNSHIFT && self.gear > 1 {
                        self.gear -= 1;
                    }
                }
            } else if v_long > -0.35 && c.throttle > 0.25 && c.brake < 0.1 {
                self.gear = 1;
            }
            self.vel = fwd * v_long + right * v_lat;

            // Move along the ground, capped against tunnelling.
            let step = self.vel * h;
            let step = step.clamp_length_max(MAX_MOVE);
            self.pos += step;
            let up2 = self.up();
            let new_h = self.sample(world).map_or(ground_h, |s| s.h);
            let feet = self.pos.length() - geo::R;
            if feet > new_h + 0.35 {
                // The ground fell away: carry on through the air.
                self.grounded = false;
            } else {
                self.pos = up2 * (geo::R + new_h);
                self.vel = fwd * v_long + right * v_lat;
                self.grounded = true;
            }
            self.distance += step.length();
        } else {
            // In the air, or sliding down a slope too steep to hold.
            self.grounded = false;
            self.vel += -up * GRAVITY * h;
            if steep {
                let g = -up * GRAVITY;
                let g_slope = g - normal * g.dot(normal);
                self.vel += g_slope * h;
                let lat = self.vel.dot(right);
                self.vel -= right * lat * (0.5 * h).min(1.0);
            }
            let step = (self.vel * h).clamp_length_max(MAX_MOVE);
            self.pos += step;
            // Touchdown.
            if let Some(s) = self.sample(world) {
                let feet = self.pos.length() - geo::R;
                let impact = -self.vel.dot(self.up());
                if feet <= s.h && impact >= 0.0 {
                    self.pos = self.up() * (geo::R + s.h);
                    self.vel -= self.up() * self.vel.dot(self.up());
                    self.grounded = true;
                    if impact > SAFE_IMPACT && self.spawn_grace <= 0.0 {
                        let dmg = if CAR_HEALTH_ENABLED { ((impact - SAFE_IMPACT) * IMPACT_DAMAGE as f64).min(60.0) as f32 } else { 0.0 };
                        if CAR_HEALTH_ENABLED {
                            self.damage.hurt(dmg, 0.0);
                            out.damage += dmg;
                        }
                        out.impacts.push(Impact { point: self.pos, normal: self.up(), speed: impact, damage: dmg });
                    }
                }
            }
        }

        // Solid things: buildings and tree trunks push the body out. Resolve
        // both separately (two passes) so a corner or tree beside a wall
        // never gets skipped, and let grazing contacts slide instead of sticking.
        let up = self.up();
        let half = self.half();
        for _ in 0..2 {
            let mut moved = false;
            for pass in 0..2 {
                let centre = self.pos + up * 0.7;
                let push = if pass == 0 {
                    world.building_push_box(centre, half, self.yaw)
                } else {
                    world.tree_push_box(centre, half, self.yaw)
                };
                if let Some(push) = push {
                    let n = push.normalize_or_zero();
                    if n.length_squared() > 0.5 {
                        self.pos += push;
                        moved = true;
                        let into = -self.vel.dot(n);
                        if into > 0.0 {
                            // Cancel inward velocity and slide smoothly along the surface.
                            let bounce = if into > SAFE_IMPACT { RESTITUTION } else { 0.0 };
                            self.vel += n * (into * (1.0 + bounce));
                            let tangent = self.vel - n * self.vel.dot(n);
                            self.vel -= tangent * (1.8 * h).min(0.25);
                            if into > SAFE_IMPACT {
                                let dmg = if CAR_HEALTH_ENABLED { ((into - SAFE_IMPACT) * IMPACT_DAMAGE as f64).min(40.0) as f32 } else { 0.0 };
                                let side = if n.dot(self.right()) >= 0.0 { 1.0 } else { -1.0 };
                                if CAR_HEALTH_ENABLED {
                                    self.damage.hurt(dmg, side);
                                    out.damage += dmg;
                                }
                                out.impacts.push(Impact { point: centre + push, normal: n, speed: into, damage: dmg });
                            }
                        }
                    }
                }
            }
            if !moved {
                break;
            }
        }

        // Traffic: solid boxes too. The player is pushed out, the closing speed
        // goes into both cars, and a real impact damages both.
        self.collide_traffic(traffic, out);

        // Wheels turn with the road speed; the body leans and squats.
        let v_long = self.longitudinal();
        self.wheel_spin = (self.wheel_spin + v_long / self.tuning.wheel_r.max(0.1) * h).rem_euclid(std::f64::consts::TAU);
        let roll_target = (-self.lateral() * 0.035).clamp(-0.12, 0.12) as f32;
        let pitch_target = (-a_long * 0.006).clamp(-0.06, 0.06) as f32;
        let k = (h * 10.0).min(1.0) as f32;
        self.roll += (roll_target - self.roll) * k;
        self.pitch += (pitch_target - self.pitch) * k;
        if !self.pos.is_finite() || !self.vel.is_finite() || !self.yaw.is_finite() {
            self.pos = geo::lla_to_pos(0.0, 0.0, 0.0);
            self.vel = DVec3::ZERO;
            self.yaw = 0.0;
            self.roll = 0.0;
            self.pitch = 0.0;
        }
    }

    /// Separate the body from every traffic box it overlaps, and turn the
    /// closing speed into a momentum exchange, damage and a [`TrafficHit`].
    /// Light cars are shoved aside; a bus or lorry mostly stops the player.
    fn collide_traffic(&mut self, traffic: &[TrafficBody], out: &mut VehicleStep) {
        if traffic.is_empty() {
            return;
        }
        let up = self.up();
        let centre = self.pos + up * 0.7;
        let half = self.half();
        let reach = half[0].hypot(half[1]);
        let inv_self = 1.0 / self.tuning.mass.max(1.0);
        for body in traffic {
            if !body.pos.is_finite() || !body.yaw.is_finite() {
                continue;
            }
            let t_up = body.pos.normalize();
            let t_centre = body.pos + t_up * 0.7;
            let t_reach = body.half[0].hypot(body.half[1]);
            if (t_centre - centre).length() > reach + t_reach {
                continue;
            }
            let Some((n, overlap)) = box_push(centre, self.yaw, half, t_centre, body.yaw, body.half) else { continue };
            let inv_other = 1.0 / tuning(body.model).mass.max(1.0);
            let inv_total = inv_self + inv_other;
            let sep = overlap.min(1.5) + 0.01;
            // Share the position separation by inverse mass, so a heavy truck
            // moves the player while a light car gives way.
            self.pos += n * (sep * inv_self / inv_total + 0.005);
            let shove = -n * (sep * inv_other / inv_total + 0.005);
            let rel = self.vel - body.velocity;
            let closing = -rel.dot(n);
            if closing <= 0.0 {
                continue;
            }
            let impulse = (1.0 + RESTITUTION) * closing / inv_total;
            self.vel += n * (impulse * inv_self);
            let other_velocity = -n * (impulse * inv_other);
            if closing < 0.05 {
                continue;
            }
            let notify = self.traffic_cool <= 0.0 && self.spawn_grace <= 0.0;
            let dmg = if notify && CAR_HEALTH_ENABLED && closing > SAFE_IMPACT {
                ((closing - SAFE_IMPACT) * IMPACT_DAMAGE as f64).min(40.0) as f32
            } else {
                0.0
            };
            if notify {
                self.traffic_cool = 0.35;
                if CAR_HEALTH_ENABLED && dmg > 0.0 {
                    let side = if n.dot(self.right()) >= 0.0 { 1.0 } else { -1.0 };
                    self.damage.hurt(dmg, side);
                    out.damage += dmg;
                }
            }
            let point = t_centre - n * overlap * 0.5;
            if notify {
                out.impacts.push(Impact { point, normal: n, speed: closing, damage: dmg });
            }
            out.traffic_hits.push(TrafficHit { body: *body, point, speed: closing, damage: dmg, velocity: other_velocity, shove, notify });
        }
    }

    /// Ground at the four wheel contact points: average height, normal, water.
    fn sample<W: WorldQuery>(&self, world: &W) -> Option<GroundSample> {
        let up = self.up();
        let fwd = self.forward();
        let right = fwd.cross(up).normalize_or_zero();
        let hl = self.len() * 0.36;
        let hw = self.width() * 0.42;
        let pts = [self.pos + fwd * hl + right * hw, self.pos + fwd * hl - right * hw, self.pos - fwd * hl + right * hw, self.pos - fwd * hl - right * hw];
        let mut hs = [0.0f64; 4];
        let mut water_wheels = 0u8;
        let mut kind = Surface::Grass;
        for (i, p) in pts.iter().enumerate() {
            let s = world.surface_at(*p)?;
            hs[i] = s.h;
            if s.blocked() {
                water_wheels += 1;
            }
            if i == 0 || s.kind != Surface::Grass {
                kind = s.kind;
            }
        }
        let center_water = world.surface_at(self.pos).is_some_and(|s| s.blocked());
        let water = water_wheels >= 2 || center_water;
        let h = hs.iter().sum::<f64>() * 0.25;
        let dh_f = (hs[0] + hs[1] - hs[2] - hs[3]) * 0.5;
        let dh_r = (hs[0] + hs[2] - hs[1] - hs[3]) * 0.5;
        let normal = (up - fwd * (dh_f / (2.0 * hl).max(0.1)) - right * (dh_r / (2.0 * hw).max(0.1))).normalize_or(up);
        Some(GroundSample { h, normal, water, kind })
    }
}

struct GroundSample {
    h: f64,
    normal: DVec3,
    water: bool,
    kind: Surface,
}

/// The push that takes ground-plane box A out of box B, with the contact
/// normal pointing from B to A, and how far they overlap. `ya`/`yb` are
/// bearings and `ha`/`hb` half width and half length. Separating-axis test over
/// the four box axes; the least overlap is the push.
fn box_push(a: DVec3, ya: f64, ha: [f64; 2], b: DVec3, yb: f64, hb: [f64; 2]) -> Option<(DVec3, f64)> {
    if !a.is_finite() || !b.is_finite() || !ya.is_finite() || !yb.is_finite() {
        return None;
    }
    let up = (a + b).normalize_or_zero();
    if up.length_squared() < 0.5 {
        return None;
    }
    let dir = |yaw: f64| {
        let (e, n, _) = geo::enu(a);
        e * yaw.sin() + n * yaw.cos()
    };
    let (fa, fb) = (dir(ya), dir(yb));
    let ra = fa.cross(up).normalize_or_zero();
    let rb = fb.cross(up).normalize_or_zero();
    if ra.length_squared() < 0.5 || rb.length_squared() < 0.5 {
        return None;
    }
    let d = a - b;
    let mut best = f64::MAX;
    let mut best_n = DVec3::ZERO;
    for n in [ra, fa, rb, fb] {
        let pa = ha[0] * ra.dot(n).abs() + ha[1] * fa.dot(n).abs();
        let pb = hb[0] * rb.dot(n).abs() + hb[1] * fb.dot(n).abs();
        let along = d.dot(n);
        let overlap = pa + pb - along.abs();
        if overlap <= 0.0 {
            return None;
        }
        if overlap < best {
            best = overlap;
            best_n = if along >= 0.0 { n } else { -n };
        }
    }
    (best.is_finite() && best_n.length_squared() > 0.5).then_some((best_n.normalize_or_zero(), best))
}

/// Fraction of peak torque at `rpm`: a broad arcade curve, strongest around
/// 3000 rpm and tapering towards the redline.
fn torque_curve(rpm: f64) -> f64 {
    let x = (rpm / 6000.0).clamp(0.0, 1.2);
    0.55 + 0.45 * (std::f64::consts::PI * x).sin().max(0.0)
}

/// The drag area that makes the tuning's top speed come out right, with the
/// body's power and the asphalt's rolling resistance.
fn drag_area(t: &Tuning) -> f64 {
    let rr = Surface::Asphalt.rolling() as f64 * t.mass * GRAVITY;
    let at_top = t.power / t.top.max(1.0);
    (2.0 * (at_top - rr) / (RHO * t.top.max(1.0) * t.top.max(1.0))).max(0.15)
}

/// Where to stand after getting out: beside the driver's door, on solid dry
/// ground, clear of buildings and trees. Right side first, then left.
pub fn exit_spot<W: WorldQuery>(v: &Vehicle, world: &W) -> DVec3 {
    let up = v.up();
    let right = v.right();
    let offset = v.width() * 0.5 + 0.85;
    for side in [1.0, -1.0] {
        let p = v.pos + right * (side * offset);
        if let Some(spot) = character::standable(world, p) {
            return spot;
        }
        // Half a metre forward or back may clear a wall or a tree.
        for along in [0.6, -0.6] {
            let q = p + v.forward() * along;
            if let Some(spot) = character::standable(world, q) {
                return spot;
            }
        }
    }
    // Nothing clear nearby: put the driver down beside the car all the same.
    let p = v.pos + right * offset;
    let h = world.surface_at(p).map(|s| s.h).unwrap_or_else(|| v.pos.length() - geo::R);
    up * (geo::R + h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::SurfaceHit;

    /// Flat ground of one class at height 0.
    struct FlatWorld(Surface);

    impl WorldQuery for FlatWorld {
        fn surface_at(&self, _p: DVec3) -> Option<SurfaceHit> {
            Some(SurfaceHit { h: 0.0, kind: self.0, water: self.0 == Surface::Water, road: false, sidewalk: false, bridge: false })
        }
        fn building_push_sphere(&self, _p: DVec3, _r: f64) -> Option<DVec3> {
            None
        }
        fn building_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
            None
        }
        fn tree_push_sphere(&self, _p: DVec3, _r: f64) -> Option<DVec3> {
            None
        }
        fn tree_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
            None
        }
    }

    /// Ground rising to the east at `deg`.
    struct SlopeWorld(f64);

    impl SlopeWorld {
        fn height(&self, p: DVec3) -> f64 {
            let (east, _, _) = geo::enu(origin());
            (p - origin()).dot(east) * self.0.to_radians().tan()
        }
    }

    impl WorldQuery for SlopeWorld {
        fn surface_at(&self, p: DVec3) -> Option<SurfaceHit> {
            Some(SurfaceHit { h: self.height(p), kind: Surface::Asphalt, water: false, road: false, sidewalk: false, bridge: false })
        }
        fn building_push_sphere(&self, _p: DVec3, _r: f64) -> Option<DVec3> {
            None
        }
        fn building_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
            None
        }
        fn tree_push_sphere(&self, _p: DVec3, _r: f64) -> Option<DVec3> {
            None
        }
        fn tree_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
            None
        }
    }

    /// A wall at east = 4 m, and flat asphalt.
    struct WallWorld;

    impl WorldQuery for WallWorld {
        fn surface_at(&self, _p: DVec3) -> Option<SurfaceHit> {
            Some(SurfaceHit { h: 0.0, kind: Surface::Asphalt, water: false, road: false, sidewalk: false, bridge: false })
        }
        fn building_push_sphere(&self, _p: DVec3, _r: f64) -> Option<DVec3> {
            None
        }
        fn building_push_box(&self, p: DVec3, half: [f64; 2], yaw: f64) -> Option<DVec3> {
            let (east, _, _) = geo::enu(origin());
            let (c, s) = (yaw.cos().abs(), yaw.sin().abs());
            let ex = half[0] * c + half[1] * s;
            let x = (p - origin()).dot(east);
            (x + ex > 4.0).then(|| -east * (x + ex - 4.0))
        }
        fn tree_push_sphere(&self, _p: DVec3, _r: f64) -> Option<DVec3> {
            None
        }
        fn tree_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
            None
        }
    }

    /// Flat, but a building fills the space east of the origin.
    struct RightWallWorld;

    impl WorldQuery for RightWallWorld {
        fn surface_at(&self, _p: DVec3) -> Option<SurfaceHit> {
            Some(SurfaceHit { h: 0.0, kind: Surface::Asphalt, water: false, road: false, sidewalk: false, bridge: false })
        }
        fn building_push_sphere(&self, p: DVec3, _r: f64) -> Option<DVec3> {
            let (east, _, _) = geo::enu(origin());
            let x = (p - origin()).dot(east);
            (x > 1.0).then(|| -east * (x - 1.0))
        }
        fn building_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
            None
        }
        fn tree_push_sphere(&self, _p: DVec3, _r: f64) -> Option<DVec3> {
            None
        }
        fn tree_push_box(&self, _p: DVec3, _h: [f64; 2], _y: f64) -> Option<DVec3> {
            None
        }
    }

    fn origin() -> DVec3 {
        geo::lla_to_pos(48.0f64.to_radians(), 2.0f64.to_radians(), 0.0)
    }

    /// A sedan parked at the origin, facing east (along +east).
    fn car(model: u8, surface: Surface) -> (Vehicle, FlatWorld) {
        let mut v = Vehicle::spawn(model, [0.6, 0.05, 0.03], origin(), std::f64::consts::FRAC_PI_2);
        v.grounded = true;
        v.spawn_grace = 0.0;
        let _ = surface;
        (v, FlatWorld(surface))
    }

    fn drive<W: WorldQuery>(v: &mut Vehicle, w: &W, c: VehicleControls, secs: f64) -> VehicleStep {
        let mut last = VehicleStep::default();
        let n = (secs * 120.0).round() as usize;
        for _ in 0..n {
            last = v.step(1.0 / 120.0, &c, w, &SurfaceWeather::default(), &[]);
        }
        last
    }

    /// Bearing difference wrapped to ±π.
    fn wrap(a: f64) -> f64 {
        (a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
    }

    #[test]
    fn acceleration_braking_top_speed_reverse_and_gears() {
        // The sedan pulls away and reaches its top speed.
        let (mut sedan, w) = car(0, Surface::Asphalt);
        let c = VehicleControls { throttle: 1.0, ..Default::default() };
        let s = drive(&mut sedan, &w, c, 6.0);
        assert!(s.speed > 10.0, "sedan after 6 s: {:.1} m/s", s.speed);
        assert!(sedan.gear > 1, "upshifted, gear {}", sedan.gear);
        let sedan_6 = s.speed;
        let s = drive(&mut sedan, &w, c, 60.0);
        let top = sedan.tuning.top;
        assert!((s.speed - top).abs() < top * 0.08, "sedan top {:.1}, want {top:.1}", s.speed);
        assert!(sedan.gear >= 4, "top gear reached: {}", sedan.gear);

        // The bus is much slower off the line and at the top.
        let (mut bus, _) = car(6, Surface::Asphalt);
        let s_bus = drive(&mut bus, &w, c, 6.0);
        assert!(s_bus.speed < sedan_6, "bus {:.1} m/s is slower than the sedan {:.1}", s_bus.speed, sedan_6);
        assert!(bus.gear > 1, "bus upshifted, gear {}", bus.gear);

        // Braking: from speed, the pedal stops it without going into reverse
        // while it is still moving.
        let (mut brake, _) = car(0, Surface::Asphalt);
        brake.vel = brake.forward() * 15.0;
        let c = VehicleControls { brake: 0.8, ..Default::default() };
        let mut n = 0;
        while brake.speed() > 0.6 && n < 600 {
            brake.step(1.0 / 120.0, &c, &w, &SurfaceWeather::default(), &[]);
            n += 1;
        }
        assert!(brake.speed() < 1.0, "brakes stopped it: {:.2}", brake.speed());
        // Held at a standstill, the brake selects reverse and backs away.
        drive(&mut brake, &w, c, 2.0);
        assert!(brake.speed() < -0.5, "reverse: {:.2}", brake.speed());
        assert_eq!(brake.gear, -1);
        // Coasting down shifts back to first.
        brake.vel = DVec3::ZERO;
        brake.gear = 1;
        for _ in 0..1200 {
            brake.step(1.0 / 120.0, &VehicleControls::default(), &w, &SurfaceWeather::default(), &[]);
        }
        assert_eq!(brake.gear, 1);
    }

    #[test]
    fn handbrake_slides_and_grass_slides_more() {
        // At speed, a handbrake turn swings the tail out more than a grip turn.
        let (mut plain, w) = car(0, Surface::Asphalt);
        plain.vel = plain.forward() * 15.0;
        let grip = drive(&mut plain, &w, VehicleControls { steer: 1.0, ..Default::default() }, 0.8);
        let yaw_plain = wrap(plain.yaw - std::f64::consts::FRAC_PI_2);

        let (mut drift, _) = car(0, Surface::Asphalt);
        drift.vel = drift.forward() * 15.0;
        let slide = drive(&mut drift, &w, VehicleControls { steer: 1.0, handbrake: true, ..Default::default() }, 0.8);
        let yaw_drift = wrap(drift.yaw - std::f64::consts::FRAC_PI_2);
        assert!(yaw_plain.abs() > 0.1, "it turns: {yaw_plain:.2}");
        assert!(yaw_drift.abs() > yaw_plain.abs(), "handbrake turns more: {yaw_drift:.2} vs {yaw_plain:.2}");
        assert!(slide.lateral.abs() > grip.lateral.abs(), "handbrake slides: {:.2} vs {:.2}", slide.lateral, grip.lateral);
        assert!(slide.speed < grip.speed, "the slide scrubs speed: {:.1} vs {:.1}", slide.speed, grip.speed);

        // Grass holds less than asphalt: the same handbrake turn slides further.
        let (mut grass, gw) = car(0, Surface::Grass);
        grass.vel = grass.forward() * 15.0;
        let g = drive(&mut grass, &gw, VehicleControls { steer: 1.0, handbrake: true, ..Default::default() }, 0.8);
        assert!(g.lateral.abs() > slide.lateral.abs(), "grass slides {:.2} m/s, asphalt {:.2}", g.lateral.abs(), slide.lateral.abs());
    }

    #[test]
    fn every_surface_has_its_grip_and_rolling_resistance() {
        let classes = [Surface::Asphalt, Surface::Concrete, Surface::Rock, Surface::Dirt, Surface::Grass, Surface::Sand];
        let mut speeds = Vec::new();
        for kind in classes {
            let (mut v, w) = car(0, kind);
            let s = drive(&mut v, &w, VehicleControls { throttle: 1.0, ..Default::default() }, 3.0);
            assert!(s.speed.is_finite() && s.speed > 0.0, "{kind:?}: {:.2}", s.speed);
            speeds.push((kind, s.speed));
        }
        for pair in speeds.windows(2) {
            assert!(pair[0].1 > pair[1].1, "{:?} {:?}: {:.2} > {:.2}", pair[0].0, pair[1].0, pair[0].1, pair[1].1);
        }
        // Rolling resistance: coasting, asphalt keeps more speed than sand.
        let coast = |kind| {
            let (mut v, w) = car(0, kind);
            v.vel = v.forward() * 20.0;
            drive(&mut v, &w, VehicleControls::default(), 6.0).speed
        };
        let (a, s) = (coast(Surface::Asphalt), coast(Surface::Sand));
        assert!(a > s + 2.0, "asphalt {a:.1} > sand {s:.1}");
    }

    #[test]
    fn slopes_gravity_air_and_water() {
        // Facing uphill with no throttle, gravity rolls the car back down.
        let slope = SlopeWorld(10.0);
        let mut v = Vehicle::spawn(0, [0.5; 3], origin(), std::f64::consts::FRAC_PI_2);
        v.grounded = true;
        v.spawn_grace = 0.0;
        let s = drive(&mut v, &slope, VehicleControls::default(), 2.0);
        assert!(s.speed < -0.5, "rolls downhill: {:.2}", s.speed);

        // Dropped from height it falls, then lands.
        let w = FlatWorld(Surface::Asphalt);
        let mut air = Vehicle::spawn(0, [0.5; 3], origin() + origin().normalize() * 6.0, std::f64::consts::FRAC_PI_2);
        air.grounded = false;
        air.spawn_grace = 0.0;
        let first = air.step(1.0 / 120.0, &VehicleControls::default(), &w, &SurfaceWeather::default(), &[]);
        assert!(!first.grounded);
        let mut landed = None;
        for _ in 0..600 {
            let s = air.step(1.0 / 120.0, &VehicleControls::default(), &w, &SurfaceWeather::default(), &[]);
            if s.grounded {
                landed = Some(s);
                break;
            }
        }
        let landed = landed.expect("it lands");
        assert!(!landed.impacts.is_empty(), "the drop is reported");
        // Car health is off for now: the impact does not hurt or wreck the car.
        assert_eq!(landed.damage, 0.0);
        assert_eq!(air.damage.health, MAX_HEALTH);

        // Water: the engine drowns, drag brings it to a stop and throttle
        // cannot pull it out.
        let (mut boat, ww) = car(0, Surface::Water);
        boat.vel = boat.forward() * 10.0;
        let s = drive(&mut boat, &ww, VehicleControls { throttle: 1.0, ..Default::default() }, 2.0);
        assert!(s.water, "the car is in water");
        assert!(s.speed < 2.0, "water drag stops it: {:.2}", s.speed);
        assert_eq!(boat.damage.engine, 0.0);
    }

    #[test]
    fn walls_push_the_car_out() {
        let w = WallWorld;
        let mut v = Vehicle::spawn(0, [0.5; 3], origin(), std::f64::consts::FRAC_PI_2);
        v.grounded = true;
        v.spawn_grace = 0.0;
        let c = VehicleControls { throttle: 1.0, ..Default::default() };
        let mut damage = 0.0;
        let mut impacts = 0;
        for _ in 0..360 {
            let s = v.step(1.0 / 120.0, &c, &w, &SurfaceWeather::default(), &[]);
            damage += s.damage;
            impacts += s.impacts.len();
        }
        let (east, _, _) = geo::enu(origin());
        let x = (v.pos - origin()).dot(east);
        assert!(x < 4.0, "did not go through the wall: {x:.2} m");
        assert!(impacts > 0, "an impact is reported");
        // Car health is off for now: a wall stops the car but does not hurt it.
        assert_eq!(damage, 0.0);
        assert_eq!(v.damage.health, MAX_HEALTH);
    }

    #[test]
    fn damage_is_persistent_and_slows_and_pulls() {
        let mut d = VehicleDamage::intact();
        assert_eq!((d.health, d.engine, d.crumple), (MAX_HEALTH, 1.0, 0.0));
        d.hurt(40.0, 1.0);
        assert!((d.health - 60.0).abs() < 0.01);
        assert!(d.engine < 1.0 && d.crumple > 0.3);
        assert!(d.steer_pull > 0.0, "the blow pulled the steering");
        assert!(!d.smoking(), "60 health is not smoking yet");
        d.hurt(30.0, 1.0);
        assert!((d.health - 30.0).abs() < 0.01);
        assert!(d.smoking());
        d.hurt(30.0, -1.0);
        assert!(d.wrecked() && d.engine == 0.0 && d.crumple == 1.0);

        // A damaged car makes less speed than a fresh one.
        let w = FlatWorld(Surface::Asphalt);
        let (mut fresh, _) = car(0, Surface::Asphalt);
        let fresh_speed = drive(&mut fresh, &w, VehicleControls { throttle: 1.0, ..Default::default() }, 6.0).speed;
        let (mut hurt, _) = car(0, Surface::Asphalt);
        hurt.damage.hurt(60.0, 0.0);
        let hurt_speed = drive(&mut hurt, &w, VehicleControls { throttle: 1.0, ..Default::default() }, 6.0).speed;
        assert!(hurt_speed < fresh_speed, "power loss: {hurt_speed:.1} < {fresh_speed:.1}");

        // Steering pull turns a car pointing straight.
        let (mut pull, _) = car(0, Surface::Asphalt);
        pull.damage.hurt(70.0, 1.0);
        let start = pull.yaw;
        drive(&mut pull, &w, VehicleControls { throttle: 0.6, ..Default::default() }, 6.0);
        assert!(wrap(pull.yaw - start).abs() > 0.05, "pulls to one side: {:.3}", wrap(pull.yaw - start));

        // A wreck does not drive, and its damage does not mend.
        let (mut wreck, _) = car(0, Surface::Asphalt);
        wreck.damage.hurt(MAX_HEALTH, 0.0);
        let s = drive(&mut wreck, &w, VehicleControls { throttle: 1.0, ..Default::default() }, 3.0);
        assert!(s.speed.abs() < 0.2, "a wreck stays put: {:.2}", s.speed);
        assert!(s.wrecked);
        assert_eq!(wreck.damage.health, 0.0);
    }

    #[test]
    fn state_stays_finite_under_bad_inputs() {
        let w = FlatWorld(Surface::Asphalt);
        let (mut v, _) = car(5, Surface::Asphalt);
        for dt in [1e9, f64::NAN, f64::INFINITY, -1.0, 0.0] {
            let s = v.step(dt, &VehicleControls { throttle: 1.0, brake: 1.0, steer: 1.0, handbrake: true }, &w, &SurfaceWeather::default(), &[]);
            assert!(s.pos.is_finite() && s.yaw.is_finite() && s.speed.is_finite(), "dt {dt}");
            assert!(v.pos.is_finite() && v.vel.is_finite() && v.damage.health.is_finite());
        }
        // Zero speed at full lock does not spin the car.
        let (mut still, _) = car(0, Surface::Asphalt);
        let start = still.yaw;
        drive(&mut still, &w, VehicleControls { steer: 1.0, ..Default::default() }, 1.0);
        assert!((still.yaw - start).abs() < 1e-6);
        assert!(still.speed().abs() < 1e-6);
        // Full lock in reverse is finite.
        still.gear = -1;
        still.vel = still.forward() * -6.0;
        let s = drive(&mut still, &w, VehicleControls { brake: 1.0, steer: -1.0, ..Default::default() }, 2.0);
        assert!(s.speed.is_finite() && s.lateral.is_finite() && still.yaw.is_finite());
        // At the pole and the antimeridian the frame is still finite.
        for (lat, lon) in [(89.999f64, 0.0f64), (0.0, 179.999), (-89.999, -179.999)] {
            let mut v = Vehicle::spawn(0, [0.5; 3], geo::lla_to_pos(lat.to_radians(), lon.to_radians(), 0.0), 0.0);
            let s = drive(&mut v, &w, VehicleControls { throttle: 1.0, steer: 1.0, ..Default::default() }, 0.5);
            assert!(s.pos.is_finite() && s.yaw.is_finite(), "{lat},{lon}");
        }
    }

    #[test]
    fn traffic_is_solid_and_both_cars_get_the_impulse() {
        let w = FlatWorld(Surface::Asphalt);
        let wx = SurfaceWeather::default();
        let (east, _, _) = geo::enu(origin());
        let body = |pos: DVec3, velocity: DVec3| crate::world::TrafficBody {
            kind: crate::world::TrafficKind::Moving,
            tile: crate::geo::TileId::new(14, 0, 0),
            index: 0,
            model: 0,
            colour: [0.4, 0.4, 0.4],
            pos,
            yaw: std::f64::consts::FRAC_PI_2,
            half: cars::player_geom(0).half,
            velocity,
        };

        // Driving into a stopped car: the body is kept out of its box, and the
        // hit gives the stopped car a velocity and a shove of its own.
        let (mut v, _) = car(0, Surface::Asphalt);
        let ahead = body(origin() + east * 9.0, DVec3::ZERO);
        let c = VehicleControls { throttle: 1.0, ..Default::default() };
        let mut hits = 0;
        let mut shove = DVec3::ZERO;
        let mut push = DVec3::ZERO;
        for _ in 0..300 {
            let s = v.step(1.0 / 120.0, &c, &w, &wx, &[ahead]);
            hits += s.traffic_hits.len();
            for h in &s.traffic_hits {
                shove += h.velocity;
                push += h.shove;
            }
            assert!(s.traffic_hits.iter().all(|h| h.body.index == 0 && h.speed > 0.0));
        }
        assert!(hits > 0, "the traffic hit is reported");
        assert!(shove.dot(east) > 0.5, "the other car is knocked east: {shove:?}");
        assert!(push.dot(east) > 0.0, "and shoved clear of the contact: {push:?}");
        let along = (v.pos - origin()).dot(east);
        assert!(along < 6.5, "kept out of the traffic box: centre {along:.2} m along");
        // Car health is off for now: the player's car is not hurt.
        assert_eq!(v.damage.health, MAX_HEALTH);

        // A car running into the player's stationary car: the player is pushed
        // clear, and the hit slows the other car rather than passing through.
        let (mut still, _) = car(0, Surface::Asphalt);
        let shunt = body(origin() + east * 4.0, -east * 8.0);
        let s = still.step(1.0 / 120.0, &VehicleControls::default(), &w, &wx, &[shunt]);
        assert!(!s.traffic_hits.is_empty(), "a shunting car is reported");
        assert!(s.traffic_hits.iter().all(|h| h.velocity.dot(east) >= 0.0), "the shunt pushes it back");
        assert_eq!(still.damage.health, MAX_HEALTH);
        assert!((still.pos - origin()).dot(east) < -0.2, "pushed back clear: {:.2}", (still.pos - origin()).dot(east));

        // Nothing within reach: no hits, no damage.
        let (mut clear, _) = car(0, Surface::Asphalt);
        let far = body(origin() + east * 40.0, DVec3::ZERO);
        let s = clear.step(1.0 / 120.0, &VehicleControls::default(), &w, &wx, &[far]);
        assert!(s.traffic_hits.is_empty() && s.damage == 0.0);
        assert_eq!(clear.damage.health, MAX_HEALTH);
    }

    #[test]
    fn collisions_transfer_momentum_by_mass() {
        let w = FlatWorld(Surface::Asphalt);
        let wx = SurfaceWeather::default();
        let (east, _, _) = geo::enu(origin());
        let body = |model: u8, pos: DVec3| crate::world::TrafficBody {
            kind: crate::world::TrafficKind::Moving,
            tile: crate::geo::TileId::new(14, 0, 0),
            index: 0,
            model,
            colour: [0.4, 0.4, 0.4],
            pos,
            yaw: std::f64::consts::FRAC_PI_2,
            half: cars::player_geom(0).half,
            velocity: DVec3::ZERO,
        };
        let hit = |model: u8| {
            let (mut v, _) = car(0, Surface::Asphalt);
            v.vel = v.forward() * 12.0;
            let s = v.step(1.0 / 120.0, &VehicleControls::default(), &w, &wx, &[body(model, origin() + east * 4.0)]);
            s.traffic_hits.first().expect("the cars touch").velocity.length()
        };
        let compact = hit(1);
        let bus = hit(6);
        assert!(compact > 4.0, "the compact is thrown clear: {compact:.1} m/s");
        assert!(bus < compact * 0.35, "the bus barely moves: {bus:.1} vs {compact:.1} m/s");
    }

    #[test]
    fn exit_spot_prefers_the_right_side_and_falls_back_to_the_left() {
        let v = Vehicle::spawn(0, [0.5; 3], origin(), 0.0);
        let flat = FlatWorld(Surface::Asphalt);
        let right = v.right();
        let spot = exit_spot(&v, &flat);
        assert!((spot - v.pos).dot(right) > 0.5, "stands on the right");
        let walled = RightWallWorld;
        let spot = exit_spot(&v, &walled);
        assert!((spot - v.pos).dot(right) < -0.5, "the building side gives way");
        assert!(character::standable(&walled, spot).is_some());
    }
}
