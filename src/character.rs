//! The player on foot: a capsule body that walks, runs, sprints, jumps, steps
//! up kerbs, keeps to slopes it can climb, takes fall damage, regenerates and
//! respawns nearby after dying. It also carries the body out of a moving car:
//! the hop, the ground roll and getting back on the feet.
//!
//! The body is a single capsule (radius 0.30 m, height 1.80 m, eye 1.65 m) in
//! the planet frame. All ground information comes from [`WorldQuery`], so the
//! controller runs without a GPU, a streamed world, or a network: the tests
//! drive it over synthetic flat, step and slope worlds.

use crate::bailout;
use crate::geo;
use crate::surface::{Surface, SurfaceHit, SurfaceWeather};
use crate::world::{TrafficBody, WorldQuery};
use glam::DVec3;

/// Capsule radius (m).
pub const RADIUS: f64 = 0.30;
/// Standing height (m), feet to crown.
pub const HEIGHT: f64 = 1.80;
/// Eye height above the feet (m).
pub const EYE: f64 = 1.65;
/// Strolling pace (m/s).
pub const WALK: f64 = 1.6;
/// Jogging pace (m/s).
pub const RUN: f64 = 4.5;
/// Sprinting pace (m/s).
pub const SPRINT: f64 = 7.0;
/// Jump apex above the take-off point (m).
pub const JUMP_HEIGHT: f64 = 1.1;
/// The highest kerb that can be stepped over (m).
pub const STEP_UP: f64 = 0.35;
/// The steepest slope that can be stood or walked on (degrees).
pub const SLOPE_LIMIT: f64 = 40.0;
/// Downward acceleration (m/s²).
pub const GRAVITY: f64 = 9.81;
/// An impact faster than this hurts (m/s).
pub const SAFE_IMPACT: f64 = 4.0;
/// Hit points lost per m/s of impact past [`SAFE_IMPACT`].
pub const IMPACT_DAMAGE: f32 = 8.0;
/// Full health.
pub const MAX_HEALTH: f32 = 100.0;
/// Quiet time before health starts coming back (s).
pub const REGEN_DELAY: f64 = 5.0;
/// Health regained per second once it does.
pub const REGEN_RATE: f32 = 2.0;
/// Distance between footsteps (m) at `speed` (m/s): short steps at a stroll,
/// long ones at a sprint, so the cadence stays human (about two steps a
/// second walking, four sprinting).
pub fn stride_len(speed: f64) -> f64 {
    (0.55 + 0.16 * speed.max(0.0)).clamp(0.7, 1.7)
}

/// Fraction of a gait cycle spent on one foot (or both during walking's
/// double support). Running's duty factor below 0.5 creates two flight arcs.
/// A sprint has shorter, quicker contacts rather than simply stretching a
/// jog's strides until the legs cannot reach the ground.
pub(crate) fn gait_duty(speed: f64) -> f64 {
    let k = ((speed.max(0.0) - 1.9) / (4.5 - 1.9)).clamp(0.0, 1.0);
    let run = k * k * (3.0 - 2.0 * k);
    let sprint = ((speed.max(0.0) - 4.8) / (7.0 - 4.8)).clamp(0.0, 1.0);
    let sprint = sprint * sprint * (3.0 - 2.0 * sprint);
    (0.62 + (0.34 - 0.62) * run) * (1.0 - sprint) + 0.24 * sprint
}

/// Is a foot down at this point in the cycle? The left foot lands at zero
/// and the right half a cycle later.
pub(crate) fn gait_has_stance(phase: f64, speed: f64) -> bool {
    if speed < 0.05 {
        return true;
    }
    let duty = gait_duty(speed);
    let q = phase.rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU;
    let a = q < duty;
    let b = (q + 0.5).fract() < duty;
    a || b
}

/// How fast keyboard steering turns the body (rad/s).
pub const TURN_RATE: f64 = 2.9;

/// How a footstep on a surface sounds: relative level, how bright its scuff
/// is, and how much of it is a low body thump.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootstepTimbre {
    pub gain: f32,
    pub bright: f32,
    pub thump: f32,
}

/// The timbre of a footstep on `surface`: asphalt clicks, grass and sand are
/// soft and dull, rock rattles, water splashes.
pub fn footstep_timbre(surface: Surface) -> FootstepTimbre {
    match surface {
        Surface::Asphalt => FootstepTimbre { gain: 1.0, bright: 1.0, thump: 0.65 },
        Surface::Concrete => FootstepTimbre { gain: 0.95, bright: 0.9, thump: 0.7 },
        Surface::Dirt => FootstepTimbre { gain: 0.8, bright: 0.45, thump: 0.85 },
        Surface::Grass => FootstepTimbre { gain: 0.55, bright: 0.28, thump: 0.45 },
        Surface::Sand => FootstepTimbre { gain: 0.45, bright: 0.18, thump: 0.3 },
        Surface::Rock => FootstepTimbre { gain: 1.05, bright: 1.15, thump: 0.9 },
        Surface::Water => FootstepTimbre { gain: 0.5, bright: 0.7, thump: 0.2 },
    }
}
/// How fast the body can change its horizontal velocity on the ground (m/s²).
const GROUND_ACCEL: f64 = 22.0;
/// …and in the air.
const AIR_ACCEL: f64 = 3.5;
/// Ground drag while rolling out of a moving car (m/s²): enough to slow the
/// body over a few metres without stopping it dead.
const ROLL_DRAG: f64 = 4.2;

/// Below this speed the driver simply gets out; above it the door is a bailout
/// and the car carries on without them (m/s).
pub const CAR_BAILOUT_SPEED: f64 = 4.0;
/// The sideways speed the driver pushes off with when bailing out (m/s).
pub const CAR_EXIT_SIDE: f64 = 2.8;
/// The upward speed of that step out (m/s): enough air to land and roll.
pub const CAR_EXIT_UP: f64 = 2.6;
/// How long the body somersaults along the ground (s).
pub const CAR_ROLL_TIME: f64 = 1.15;
/// How long it takes to plant a hand, get the feet underneath and stand (s).
pub const CAR_STAND_TIME: f64 = 0.85;

/// How hard a car knocks the body back (m/s per m/s of closing speed).
pub const TRAFFIC_KNOCKBACK: f64 = 1.4;
/// Hit points lost per m/s of closing speed past [`SAFE_IMPACT`].
pub const TRAFFIC_DAMAGE: f32 = 6.0;

/// What the player is asking the body to do this step. `forward` and `right`
/// are -1..1; `sprint` is held; `jump` is the press edge.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct CharacterControls {
    pub forward: f64,
    pub right: f64,
    pub sprint: bool,
    pub jump: bool,
    /// The direction "forward" means, as a bearing; none uses the body's own
    /// facing (useful for tests).
    pub heading: Option<f64>,
    /// Keyboard steering, -1..1: turns the body itself at [`TURN_RATE`]
    /// (right is positive), so the camera can stay behind it.
    pub turn: f64,
    /// Keep facing the way the body faces instead of turning to the way it
    /// moves (backing up with the keyboard).
    pub hold_facing: bool,
}

impl CharacterControls {
    /// The intended pace for the input: a nudge walks, holding it runs, and
    /// sprint adds the rest. Zero when there is no input.
    pub fn pace(&self) -> f64 {
        let m = self.forward.hypot(self.right);
        if m < 0.01 {
            0.0
        } else if self.sprint {
            SPRINT
        } else if m > 0.5 {
            RUN
        } else {
            WALK
        }
    }
}

/// The stages of leaving a moving car: the hop out, the roll along the
/// ground, and getting back on the feet.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CarBailoutPhase {
    Air,
    Roll,
    Stand,
}

/// A bailout from a moving car.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct CarBailout {
    pub phase: CarBailoutPhase,
    /// Seconds spent in this phase.
    pub t: f64,
    /// Whole-body somersault angle (radians): one turn over the roll.
    pub roll: f64,
}

impl CarBailout {
    pub fn new() -> Self {
        Self { phase: CarBailoutPhase::Air, t: 0.0, roll: 0.0 }
    }
}

impl Default for CarBailout {
    fn default() -> Self {
        Self::new()
    }
}

/// The body's state.
#[derive(Clone, Debug)]
pub struct Character {
    /// Feet position in the planet frame.
    pub pos: DVec3,
    /// Velocity in the planet frame (the vertical part included).
    pub vel: DVec3,
    /// Facing as a bearing: radians from north, clockwise.
    pub yaw: f64,
    pub grounded: bool,
    pub health: f32,
    /// The last place the body stood safely (respawn point).
    pub last_safe: DVec3,
    /// Where the body first stood this run: the last resort when a water
    /// landing has no dry land within reach.
    pub home: DVec3,
    /// The air's own motion where the body is (planet frame). It matters
    /// while freefalling or under canopy, which drift downwind; on the
    /// ground the body's own step ignores it.
    pub wind: DVec3,
    /// A bailout in progress: freefall or a canopy, until touchdown.
    pub fall: Option<bailout::Fall>,
    /// A bailout from a moving car: the hop, the ground roll and standing up.
    pub car_bailout: Option<CarBailout>,
    /// Seconds since the last damage.
    pub since_hurt: f64,
    /// Phase at the most recent completed footstep (radians), for audio tests.
    pub stride_phase: f64,
    /// Continuous visual gait phase. Unlike distance divided by the *current*
    /// stride length, this cannot jump when walking speeds change.
    gait: f64,
    /// 0 on the ground, easing to 1 in the air: the jump pose's share.
    pub air: f64,
    /// Landing damage is forgiven while the body has just appeared (streaming
    /// may put the ground a little below the spawn estimate).
    pub spawn_grace: f64,
    /// Deaths this session.
    pub respawns: u32,
}

/// What one step did, for the camera, the HUD, audio and the pose.
#[derive(Clone, Debug, Default)]
pub struct CharacterStep {
    pub pos: DVec3,
    pub vel: DVec3,
    pub grounded: bool,
    /// Impact speed, on the step that landed (m/s).
    pub landed: Option<f64>,
    /// Hit points lost this step.
    pub damage: f32,
    /// The body died and was moved to a nearby safe point.
    pub died: bool,
    /// One surface class per footstep this step.
    pub footsteps: Vec<Surface>,
    /// The player opened the parachute on this step.
    pub chute_opened: bool,
    /// A car bailout touched down and began its roll.
    pub car_roll_started: bool,
    /// Vertical speed at a splashdown (m/s). `Game` gets the body out of the
    /// water: swimming is a separate feature.
    pub water: Option<f64>,
}

/// A bump from traffic, for the HUD and (in a later phase) the sound queue.
#[derive(Copy, Clone, Debug)]
pub struct TrafficContact {
    pub body: TrafficBody,
    /// Closing speed along the contact normal (m/s).
    pub speed: f64,
    pub damage: f32,
}

impl Character {
    /// Standing at `pos` (feet), facing `yaw` (bearing radians).
    pub fn spawn(pos: DVec3, yaw: f64) -> Self {
        let pos = if pos.is_finite() { pos } else { geo::lla_to_pos(0.0, 0.0, 0.0) };
        Self {
            pos,
            vel: DVec3::ZERO,
            yaw,
            grounded: false,
            health: MAX_HEALTH,
            last_safe: pos,
            home: pos,
            wind: DVec3::ZERO,
            fall: None,
            car_bailout: None,
            since_hurt: REGEN_DELAY,
            stride_phase: 0.0,
            gait: 0.0,
            air: 0.0,
            spawn_grace: 2.0,
            respawns: 0,
        }
    }

    /// The eye in the planet frame. Freefalling, the body is belly to earth and
    /// the head is out in front of the hips; under canopy it is upright again.
    pub fn eye(&self) -> DVec3 {
        let (up, forward) = self.fall.as_ref().map_or((EYE, 0.0), |f| f.eye_offset());
        self.pos + self.pos.normalize() * up + self.forward() * forward
    }

    /// The gait cycle as a continuous angle: a half turn per footstep. The
    /// phase is integrated at the speed of each substep, so accelerating from
    /// a walk to a sprint never scrubs the feet forwards or backwards.
    pub fn gait_phase(&self) -> f64 {
        self.gait
    }

    /// Horizontal speed (m/s).
    pub fn speed(&self) -> f64 {
        let up = self.pos.normalize();
        (self.vel - up * self.vel.dot(up)).length()
    }

    /// The forward direction as a unit vector in the planet frame.
    pub fn forward(&self) -> DVec3 {
        let (e, n, _) = geo::enu(self.pos);
        e * self.yaw.sin() + n * self.yaw.cos()
    }

    /// Take `amount` hit points; returns whether this killed the body.
    pub fn hurt(&mut self, amount: f32) -> bool {
        if amount <= 0.0 || self.health <= 0.0 {
            return false;
        }
        self.health = (self.health - amount).max(0.0);
        self.since_hurt = 0.0;
        self.health <= 0.0
    }

    /// Move the body to the nearest safe point around `center`, or to `center`
    /// when the world cannot be queried (tiles not in yet).
    pub fn respawn_near<W: WorldQuery>(&mut self, world: &W, center: DVec3) {
        let p = safe_point(world, center).or_else(|| safe_point(world, self.home)).unwrap_or(center);
        self.pos = p;
        self.vel = DVec3::ZERO;
        self.wind = DVec3::ZERO;
        self.fall = None;
        self.car_bailout = None;
        self.last_safe = p;
        self.grounded = false;
        self.health = MAX_HEALTH;
        self.since_hurt = REGEN_DELAY;
        self.spawn_grace = 2.0;
        self.respawns += 1;
    }

    /// Resolve the capsule against the traffic boxes: push the body out of any
    /// overlap, and let a moving car knock it away and hurt it. Returns the
    /// worst contact, which `Game` uses for the status line, the screen flash
    /// and the traffic brake event.
    pub fn hit_traffic(&mut self, traffic: &[TrafficBody]) -> Option<TrafficContact> {
        let mut worst: Option<TrafficContact> = None;
        let up = self.pos.normalize();
        let centre = self.pos + up * (HEIGHT * 0.5);
        for body in traffic {
            if !body.pos.is_finite() || !body.yaw.is_finite() {
                continue;
            }
            let b_up = body.pos.normalize();
            let b_centre = body.pos + b_up * (HEIGHT * 0.5);
            if (b_centre - centre).length() > RADIUS + body.half[0].hypot(body.half[1]) + HEIGHT {
                continue;
            }
            let Some((n, overlap)) = circle_box_push(centre, RADIUS, b_centre, body.yaw, body.half) else { continue };
            self.pos += n * (overlap + 0.005);
            let closing = -(self.vel - body.velocity).dot(n);
            if closing <= 0.0 {
                continue;
            }
            // Knocked away, losing the closing speed and a little more.
            self.vel += n * (closing * TRAFFIC_KNOCKBACK);
            if closing <= SAFE_IMPACT || self.spawn_grace > 0.0 {
                continue;
            }
            let dmg = ((closing - SAFE_IMPACT) * TRAFFIC_DAMAGE as f64).min(70.0) as f32;
            self.hurt(dmg);
            self.since_hurt = 0.0;
            if worst.is_none_or(|w| closing > w.speed) {
                worst = Some(TrafficContact { body: *body, speed: closing, damage: dmg });
            }
        }
        worst
    }

    /// Advance the body by `dt` (s).
    pub fn step<W: WorldQuery>(&mut self, dt: f64, c: &CharacterControls, world: &W, wx: &SurfaceWeather) -> CharacterStep {
        let mut out = CharacterStep::default();
        // Never integrate an absurd or broken frame; substep for stability.
        let total = if dt.is_finite() { dt.clamp(0.0, 0.5) } else { 0.0 };
        if total > 0.0 {
            let n = ((total / (1.0 / 60.0)).ceil() as usize).clamp(1, 32);
            let h = total / n as f64;
            for _ in 0..n {
                self.substep(h, c, world, wx, &mut out);
            }
        } else {
            self.settle(world, &mut out);
        }
        out.pos = self.pos;
        out.vel = self.vel;
        out.grounded = self.grounded;
        out
    }

    /// A single integration step of at most a sixtieth of a second.
    fn substep<W: WorldQuery>(&mut self, h: f64, c: &CharacterControls, world: &W, wx: &SurfaceWeather, out: &mut CharacterStep) {
        if self.fall.is_some() && !self.grounded {
            self.fall_substep(h, c, world, out);
            return;
        }
        self.since_hurt += h;
        self.spawn_grace = (self.spawn_grace - h).max(0.0);
        let car_bail = self.car_bailout;
        let car_rolling = car_bail.is_some_and(|b| b.phase == CarBailoutPhase::Roll);
        let car_airborne = car_bail.is_some_and(|b| b.phase == CarBailoutPhase::Air) && !self.grounded;
        let up = self.pos.normalize();
        let (east, north, _) = geo::enu(self.pos);
        let hit = world.surface_at(self.pos);
        let normal = ground_normal(world, self.pos, up);
        let slope = normal.dot(up).clamp(-1.0, 1.0).acos().to_degrees();
        let steep = slope > SLOPE_LIMIT;
        let surface = hit.map(|h| h.kind).filter(|k| *k != Surface::Water).unwrap_or(Surface::Grass);
        let blocked = hit.is_none_or(|h| h.blocked());

        // The intended horizontal velocity: the facing is the camera's, so
        // forward means "away from the camera". A bailout out of a moving car
        // has the body out of control until it has rolled and stood up.
        let wish_len = if car_bail.is_some() { 0.0 } else { c.forward.hypot(c.right).clamp(0.0, 1.0) };
        if car_bail.is_none() && c.turn.is_finite() && c.turn != 0.0 {
            self.yaw = (self.yaw + c.turn.clamp(-1.0, 1.0) * TURN_RATE * h).rem_euclid(std::f64::consts::TAU);
        }
        let heading = c.heading.unwrap_or(self.yaw);
        let (sin_h, cos_h) = heading.sin_cos();
        let wish_dir = if wish_len > 1e-6 { (east * (sin_h * c.forward + cos_h * c.right) + north * (cos_h * c.forward - sin_h * c.right)).normalize() } else { DVec3::ZERO };
        let pace = c.pace() * surface.foot_speed() as f64 * wx.foot_factor(surface) as f64;
        // Too steep to walk on: the input cannot push the body uphill.
        let wish = if steep { DVec3::ZERO } else { wish_dir * pace };

        let vh = self.vel - up * self.vel.dot(up);
        let accel = if car_rolling {
            ROLL_DRAG
        } else if car_airborne {
            // The push-off is a ballistic hop, not a giant invisible air
            // brake. Keeping its momentum also makes the landing and roll
            // read as one continuous action.
            0.15
        } else if self.grounded {
            GROUND_ACCEL
        } else {
            AIR_ACCEL
        };
        let mut new_vh = vh + (wish - vh).clamp_length_max(accel * h);
        if steep {
            // Gravity takes the body down the slope.
            let downhill = (up - normal * up.dot(normal)).normalize_or_zero();
            let downhill = if downhill.length_squared() > 1e-9 { -downhill } else { DVec3::ZERO };
            new_vh += downhill * GRAVITY * 0.5 * h;
            self.grounded = false;
        }
        if blocked {
            new_vh = DVec3::ZERO;
        }
        self.vel = new_vh + up * self.vel.dot(up);

        // Jump from the ground.
        if self.grounded && c.jump && !blocked && !steep && car_bail.is_none() {
            self.vel += up * (2.0 * GRAVITY * JUMP_HEIGHT).sqrt();
            self.grounded = false;
        }

        // Horizontal move, with the kerb test.
        let old_h = hit.map(|h| h.h);
        let mut p = self.pos + new_vh * h;
        if new_vh.length_squared() > 1e-12 {
            // (keep the current altitude: the vertical pass puts the feet on the ground)
            p = p.normalize() * self.pos.length();
            let step_hit = world.surface_at(p);
            let rise = match (old_h, step_hit) {
                (Some(a), Some(b)) if !b.blocked() => b.h - a,
                _ => 0.0,
            };
            if step_hit.is_none_or(|b| b.blocked()) || (self.grounded && rise > STEP_UP) {
                // A wall, deep water, or a ledge too high to step over.
                p = self.pos;
                self.vel -= up * self.vel.dot(up);
            }
        }
        self.pos = p;

        // Vertical: stick to the ground, or fall.
        let up = self.pos.normalize();
        let hit = world.surface_at(self.pos);
        match hit {
            Some(s) if !s.blocked() => {
                let ground = up * (geo::R + s.h);
                let feet = self.pos.length() - geo::R;
                let vertical = self.vel.dot(up);
                if self.grounded {
                    self.pos = ground;
                    self.vel -= up * vertical;
                } else if feet <= s.h + 0.02 && vertical <= 0.0 {
                    // Touchdown.
                    let impact = -vertical;
                    self.pos = ground;
                    self.vel -= up * vertical;
                    self.grounded = true;
                    out.landed = Some(impact);
                    if impact > SAFE_IMPACT && self.spawn_grace <= 0.0 {
                        let dmg = ((impact - SAFE_IMPACT) * IMPACT_DAMAGE as f64) as f32;
                        if self.hurt(dmg) {
                            out.died = true;
                        }
                        out.damage += dmg;
                    }
                } else {
                    self.vel += up * (-GRAVITY * h);
                    self.pos += up * (self.vel.dot(up) * h);
                }
            }
            _ => {
                // No ground here (a missing tile): hold the last sample.
                self.vel -= up * self.vel.dot(up);
            }
        }

        // Solid things: buildings and tree trunks push the capsule out.
        let up = self.pos.normalize();
        let centre = self.pos + up * (HEIGHT * 0.5);
        if let Some(push) = world.building_push_sphere(centre, RADIUS) {
            self.pos += push;
        }
        if let Some(push) = world.tree_push_sphere(centre, RADIUS) {
            self.pos += push;
        }
        if !self.pos.is_finite() {
            self.pos = geo::lla_to_pos(0.0, 0.0, 0.0);
            self.vel = DVec3::ZERO;
        }

        // Facing follows the way the body is going, turning smoothly.
        if new_vh.length() > 0.25 && !c.hold_facing && car_bail.is_none() {
            let target = new_vh.dot(east).atan2(new_vh.dot(north));
            let diff = (target - self.yaw + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
            self.yaw = (self.yaw + diff * (12.0 * h).min(1.0)).rem_euclid(std::f64::consts::TAU);
        }

        // Footsteps, and the last safe place. Advance the visual phase by
        // the actual substep distance at the *current* stride length rather
        // than recomputing an accumulated fraction with a new denominator.
        let speed = self.speed();
        let travelled = speed * h;
        if self.grounded && self.car_bailout.is_none() {
            if speed > 0.01 {
                let before = (self.gait / std::f64::consts::PI).floor() as i64;
                self.gait += std::f64::consts::PI * travelled / stride_len(speed);
                let after = (self.gait / std::f64::consts::PI).floor() as i64;
                for _ in before..after {
                    out.footsteps.push(surface);
                }
                if after > before {
                    self.stride_phase = after as f64 * std::f64::consts::PI;
                }
            }
            if !steep && hit.is_some_and(|s| !s.blocked()) {
                self.last_safe = self.pos;
            }
        }

        // The car bailout keeps the body tucked in the air and through the
        // roll; once it is standing, `air` eases back to a normal stance.
        let air = if self.car_bailout.is_some_and(|b| b.phase != CarBailoutPhase::Stand) {
            1.0
        } else if self.grounded {
            0.0
        } else {
            1.0
        };
        self.air += (air - self.air) * (h * 9.0).min(1.0);
        if self.step_car_bailout(h) {
            out.car_roll_started = true;
        }

        // Health: a slow mend after a quiet spell.
        if self.health > 0.0 && self.health < MAX_HEALTH && self.since_hurt > REGEN_DELAY {
            self.health = (self.health + REGEN_RATE * h as f32).min(MAX_HEALTH);
        }
        if self.health <= 0.0 {
            let center = self.last_safe;
            self.respawn_near(world, center);
            out.died = true;
            out.damage = 0.0;
        }
    }

    /// Advance the car bailout: the hop, the ground roll and standing up.
    /// Returns true on the step the body first touches down and rolls.
    fn step_car_bailout(&mut self, h: f64) -> bool {
        let Some(mut b) = self.car_bailout else { return false };
        b.t += h;
        let mut landed = false;
        match b.phase {
            CarBailoutPhase::Air => {
                if self.grounded {
                    b.phase = CarBailoutPhase::Roll;
                    b.t = 0.0;
                    b.roll = 0.0;
                    landed = true;
                } else if b.t > 4.0 {
                    // A hole or water: the normal controller takes over rather
                    // than leaving the body frozen in a bailout state.
                    self.car_bailout = None;
                    return false;
                }
            }
            CarBailoutPhase::Roll => {
                let p = (b.t / CAR_ROLL_TIME).clamp(0.0, 1.0);
                let ease = p * p * (3.0 - 2.0 * p);
                b.roll = ease * std::f64::consts::TAU;
                if p >= 1.0 {
                    b.phase = CarBailoutPhase::Stand;
                    b.t = 0.0;
                    b.roll = 0.0;
                }
            }
            CarBailoutPhase::Stand => {
                if b.t >= CAR_STAND_TIME {
                    self.car_bailout = None;
                    return false;
                }
            }
        }
        self.car_bailout = Some(b);
        landed
    }

    /// A step while out of an aircraft: freefall, canopy, or the last few
    /// metres to the ground. `bailout::Fall` does the air and the touchdown;
    /// the body's health, respawn and pose stay here.
    fn fall_substep<W: WorldQuery>(&mut self, h: f64, c: &CharacterControls, world: &W, out: &mut CharacterStep) {
        self.since_hurt += h;
        self.spawn_grace = (self.spawn_grace - h).max(0.0);
        let Some(fall) = self.fall.as_mut() else { return };
        let event = fall.step(h, &mut self.pos, &mut self.vel, c, self.wind, world);
        let open = fall.open;
        self.yaw = fall.heading;
        if event.deployed {
            out.chute_opened = true;
        }

        if let Some(impact) = event.water {
            // There is no swimming yet: the game gets the body out of the
            // water. The impact itself can still hurt.
            self.fall = None;
            self.grounded = false;
            self.wind = DVec3::ZERO;
            out.water = Some(impact);
            let dmg = bailout::water_damage(impact);
            if dmg > 0.0 {
                if self.hurt(dmg) {
                    out.died = true;
                }
                out.damage += dmg;
            }
            if self.health <= 0.0 {
                let center = self.last_safe;
                self.respawn_near(world, center);
                out.died = true;
                out.damage = 0.0;
            }
            return;
        }

        if let Some(impact) = event.landed {
            let damage = bailout::landing_damage(impact, open);
            self.fall = None;
            self.grounded = true;
            self.wind = DVec3::ZERO;
            self.vel -= self.pos.normalize() * self.vel.dot(self.pos.normalize());
            self.last_safe = self.pos;
            out.landed = Some(impact);
            if damage > 0.0 && self.spawn_grace <= 0.0 {
                if self.hurt(damage) {
                    out.died = true;
                }
                out.damage += damage;
            }
        }

        let air = if self.grounded { 0.0 } else { 1.0 };
        self.air += (air - self.air) * (h * 9.0).min(1.0);

        if self.health <= 0.0 {
            let center = self.last_safe;
            self.respawn_near(world, center);
            out.died = true;
            out.damage = 0.0;
        } else if self.health > 0.0 && self.health < MAX_HEALTH && self.since_hurt > REGEN_DELAY {
            self.health = (self.health + REGEN_RATE * h as f32).min(MAX_HEALTH);
        }
    }

    /// No time passed: keep the feet on the ground under them.
    fn settle<W: WorldQuery>(&mut self, world: &W, _out: &mut CharacterStep) {
        if self.fall.is_some() {
            return;
        }
        let up = self.pos.normalize();
        if let Some(hit) = world.surface_at(self.pos).filter(|h| !h.blocked()) {
            self.pos = up * (geo::R + hit.h);
            self.grounded = true;
            self.vel = DVec3::ZERO;
        }
    }
}

/// The push that takes a circle of `radius` at `c` out of a ground-plane box at
/// `b` with bearing `yaw`, and the push normal (from the box to the circle).
fn circle_box_push(c: DVec3, radius: f64, b: DVec3, yaw: f64, half: [f64; 2]) -> Option<(DVec3, f64)> {
    if !c.is_finite() || !b.is_finite() || !yaw.is_finite() {
        return None;
    }
    let up = (c + b).normalize_or_zero();
    if up.length_squared() < 0.5 {
        return None;
    }
    let (east, north, _) = geo::enu(b);
    let fwd = east * yaw.sin() + north * yaw.cos();
    let right = fwd.cross(up).normalize_or_zero();
    if right.length_squared() < 0.5 {
        return None;
    }
    let d = c - b;
    let (lx, lz) = (d.dot(right), d.dot(fwd));
    let (cx, cz) = (lx.clamp(-half[0], half[0]), lz.clamp(-half[1], half[1]));
    let delta = d - (right * cx + fwd * cz);
    let dist = delta.length();
    if dist >= radius {
        return None;
    }
    if dist > 1e-6 {
        return Some((delta / dist, radius - dist));
    }
    // The centre is inside the box: leave by the nearest face.
    let px = half[0] - lx.abs();
    let pz = half[1] - lz.abs();
    let n = if px < pz { right * if lx >= 0.0 { 1.0 } else { -1.0 } } else { fwd * if lz >= 0.0 { 1.0 } else { -1.0 } };
    Some((n, px.min(pz) + radius))
}

/// Outward unit normal of the surface under `p`, from four one-metre samples.
fn ground_normal<W: WorldQuery>(world: &W, p: DVec3, up: DVec3) -> DVec3 {
    let (east, north, _) = geo::enu(p);
    let Some(h0) = world.surface_at(p).map(|h| h.h) else { return up };
    let d = 1.0;
    let hx = world.surface_at(p + east * d).map(|h| h.h).unwrap_or(h0);
    let hz = world.surface_at(p + north * d).map(|h| h.h).unwrap_or(h0);
    (up - east * ((hx - h0) / d) - north * ((hz - h0) / d)).normalize_or(up)
}

/// Whether a point can be stood on: there is ground, it is not water, and no
/// building or tree trunk occupies the body's space.
pub fn standable<W: WorldQuery>(world: &W, p: DVec3) -> Option<DVec3> {
    let hit: SurfaceHit = world.surface_at(p)?;
    if hit.blocked() {
        return None;
    }
    let up = p.normalize();
    let ground = up * (geo::R + hit.h);
    let centre = ground + up * (HEIGHT * 0.5);
    if world.building_push_sphere(centre, RADIUS).is_some() || world.tree_push_sphere(centre, RADIUS).is_some() {
        return None;
    }
    Some(ground)
}

/// The nearest standable point to `center`: there first, then rings out to
/// 72 m. `None` only where nothing can be queried (a missing tile).
pub fn safe_point<W: WorldQuery>(world: &W, center: DVec3) -> Option<DVec3> {
    if let Some(p) = standable(world, center) {
        return Some(p);
    }
    let (east, north, _) = geo::enu(center);
    for ring in 1..=12 {
        let r = ring as f64 * 6.0;
        for k in 0..12 {
            let a = k as f64 * std::f64::consts::TAU / 12.0;
            let q = center + (east * a.cos() + north * a.sin()) * r;
            if let Some(p) = standable(world, q) {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::SurfaceWeather;

    /// Flat asphalt at height 0.
    struct FlatWorld;
    impl WorldQuery for FlatWorld {
        fn surface_at(&self, p: DVec3) -> Option<SurfaceHit> {
            let _ = p;
            Some(SurfaceHit { h: 0.0, kind: Surface::Asphalt, water: false, road: false, sidewalk: false, bridge: false })
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

    /// Grass everywhere, of one class.
    struct SurfaceWorld(Surface);
    impl WorldQuery for SurfaceWorld {
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

    /// Flat, with a raised platform from half a metre east of the origin.
    struct StepWorld(f64);
    impl WorldQuery for StepWorld {
        fn surface_at(&self, p: DVec3) -> Option<SurfaceHit> {
            let (east, _, _) = geo::enu(origin());
            let h = if (p - origin()).dot(east) >= 0.5 { self.0 } else { 0.0 };
            Some(SurfaceHit { h, kind: Surface::Grass, water: false, road: false, sidewalk: false, bridge: false })
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

    /// A plane rising to the east at `angle` degrees.
    struct SlopeWorld(f64);
    impl SlopeWorld {
        fn height(&self, p: DVec3) -> f64 {
            let (east, _, _) = geo::enu(origin());
            (p - origin()).dot(east) * self.0.to_radians().tan()
        }
    }
    impl WorldQuery for SlopeWorld {
        fn surface_at(&self, p: DVec3) -> Option<SurfaceHit> {
            Some(SurfaceHit { h: self.height(p), kind: Surface::Rock, water: false, road: false, sidewalk: false, bridge: false })
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

    fn origin() -> DVec3 {
        geo::lla_to_pos(48.0f64.to_radians(), 2.0f64.to_radians(), 0.0)
    }

    fn at_origin() -> Character {
        // Facing east.
        Character::spawn(origin(), std::f64::consts::FRAC_PI_2)
    }

    fn run<W: WorldQuery>(c: &mut Character, world: &W, controls: CharacterControls, secs: f64) -> CharacterStep {
        let mut last = CharacterStep::default();
        let n = (secs * 60.0).round() as usize;
        for _ in 0..n {
            last = c.step(1.0 / 60.0, &controls, world, &SurfaceWeather::default());
        }
        last
    }

    fn height(c: &Character) -> f64 {
        c.pos.length() - geo::R
    }

    #[test]
    fn walk_run_and_sprint_on_flat_ground() {
        let w = FlatWorld;
        for (input, want) in [(CharacterControls { forward: 0.3, ..Default::default() }, WALK), (CharacterControls { forward: 1.0, ..Default::default() }, RUN), (CharacterControls { forward: 1.0, sprint: true, ..Default::default() }, SPRINT)] {
            let mut c = at_origin();
            run(&mut c, &w, input, 2.0);
            assert!((c.speed() - want).abs() < 0.05, "speed {} want {want}", c.speed());
            assert!(c.grounded);
        }
    }

    #[test]
    fn keyboard_steering_turns_the_body_and_backing_up_keeps_the_facing() {
        let w = FlatWorld;
        // A quarter turn to the right takes a little over half a second.
        let mut c = at_origin();
        run(&mut c, &w, CharacterControls { turn: 1.0, ..Default::default() }, std::f64::consts::FRAC_PI_2 / TURN_RATE);
        assert!((c.yaw - std::f64::consts::PI).abs() < 0.03, "facing south after a right turn from east: {}", c.yaw);
        // Walking forward while turning curves the path, facing along it.
        let mut c = at_origin();
        run(&mut c, &w, CharacterControls { forward: 1.0, turn: -0.5, ..Default::default() }, 1.0);
        assert!(c.yaw < std::f64::consts::FRAC_PI_2 - 0.5, "turned left while walking: {}", c.yaw);
        // Backing up does not spin the body round to face the camera.
        let mut c = at_origin();
        let start = c.pos;
        run(&mut c, &w, CharacterControls { forward: -0.4, hold_facing: true, ..Default::default() }, 1.5);
        assert!((c.yaw - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "still facing east: {}", c.yaw);
        let (east, _, _) = geo::enu(start);
        assert!((c.pos - start).dot(east) < -1.0, "moved west, backwards");
    }

    #[test]
    fn surface_multipliers_apply_to_on_foot_speed() {
        let classes = [(Surface::Asphalt, 1.00), (Surface::Concrete, 1.00), (Surface::Dirt, 0.82), (Surface::Grass, 0.78), (Surface::Sand, 0.58), (Surface::Rock, 0.72)];
        for (kind, factor) in classes {
            let w = SurfaceWorld(kind);
            let mut c = at_origin();
            run(&mut c, &w, CharacterControls { forward: 1.0, ..Default::default() }, 2.0);
            assert!((c.speed() - RUN * factor).abs() < 0.08, "{kind:?}: speed {} want {}", c.speed(), RUN * factor);
        }
        let w = SurfaceWorld(Surface::Water);
        let mut c = at_origin();
        run(&mut c, &w, CharacterControls { forward: 1.0, ..Default::default() }, 1.0);
        assert!(c.speed() < 0.01, "deep water blocks walking");
    }

    #[test]
    fn kerbs_up_to_the_limit_are_stepped_over() {
        for (rise, should_climb) in [(0.34, true), (0.36, false)] {
            let w = StepWorld(rise);
            let mut c = at_origin();
            run(&mut c, &w, CharacterControls { forward: 1.0, ..Default::default() }, 2.0);
            let (east, _, _) = geo::enu(origin());
            let travelled = (c.pos - origin()).dot(east);
            if should_climb {
                assert!(travelled > 2.0, "climbed {travelled:.2} m");
                assert!((height(&c) - rise).abs() < 0.03, "height {:.3}", height(&c));
            } else {
                assert!(travelled < 0.6, "refused, stayed at {travelled:.2} m");
                assert!(height(&c) < 0.03);
            }
        }
    }

    #[test]
    fn slopes_are_walked_up_to_the_limit_and_slid_beyond_it() {
        for (deg, climbs) in [(39.0, true), (41.0, false)] {
            let w = SlopeWorld(deg);
            let mut c = at_origin();
            // Put the feet on the slope at the origin.
            c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default());
            let start = height(&c);
            run(&mut c, &w, CharacterControls { forward: 1.0, ..Default::default() }, 1.0);
            let end = height(&c);
            if climbs {
                assert!(end > start + 0.2, "{deg}° should be walkable: {start:.2} -> {end:.2}");
                assert!(c.grounded);
            } else {
                assert!(!c.grounded || end <= start + 0.05, "{deg}° should not be walkable: {start:.2} -> {end:.2}");
                // With no input it keeps sliding downhill.
                let before = height(&c);
                run(&mut c, &w, CharacterControls::default(), 0.5);
                assert!(height(&c) < before + 0.02, "{deg}° should slide");
            }
        }
    }

    #[test]
    fn jump_reaches_its_apex_and_lands() {
        let w = FlatWorld;
        let mut c = at_origin();
        c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default());
        let mut apex: f64 = 0.0;
        let mut landed = false;
        let ctrl = CharacterControls { jump: true, ..Default::default() };
        c.step(1.0 / 60.0, &ctrl, &w, &SurfaceWeather::default());
        for _ in 0..180 {
            let s = c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default());
            apex = apex.max(height(&c));
            landed |= s.landed.is_some();
        }
        assert!((apex - JUMP_HEIGHT).abs() < 0.16, "apex {apex:.2}");
        assert!(landed && c.grounded);
    }

    #[test]
    fn falls_hurt_and_health_mends() {
        let w = FlatWorld;
        let mut c = at_origin();
        // Ten metres up, falling.
        c.pos = c.pos.normalize() * (geo::R + 10.0);
        c.grounded = false;
        c.spawn_grace = 0.0;
        let mut damage = 0.0;
        for _ in 0..300 {
            let s = c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default());
            damage += s.damage;
            if s.landed.is_some() {
                break;
            }
        }
        assert!(damage > 0.0, "a ten-metre fall hurts");
        assert!(c.health < MAX_HEALTH);
        // A small drop does not.
        let mut c2 = at_origin();
        c2.pos = c2.pos.normalize() * (geo::R + 0.5);
        c2.grounded = false;
        let mut d2 = 0.0;
        for _ in 0..120 {
            d2 += c2.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default()).damage;
        }
        assert_eq!(d2, 0.0);
        // Five quiet seconds, then two points a second.
        c.since_hurt = REGEN_DELAY + 1.0;
        let before = c.health;
        c.step(0.5, &CharacterControls::default(), &w, &SurfaceWeather::default());
        c.step(0.5, &CharacterControls::default(), &w, &SurfaceWeather::default());
        assert!((c.health - (before + REGEN_RATE)).abs() < 0.2, "regenerated {:.2}", c.health - before);
    }

    #[test]
    fn death_respawns_close_by() {
        let w = FlatWorld;
        let mut c = at_origin();
        c.last_safe = origin();
        c.health = 1.0;
        c.pos = c.pos.normalize() * (geo::R + 30.0);
        c.grounded = false;
        c.spawn_grace = 0.0;
        let mut died = false;
        for _ in 0..600 {
            let s = c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default());
            if s.died || c.respawns > 0 {
                died = true;
                break;
            }
        }
        assert!(died);
        assert_eq!(c.respawns, 1);
        assert_eq!(c.health, MAX_HEALTH);
        assert!((c.pos - origin()).length() < 200.0, "respawned nearby");
        assert!((height(&c)).abs() < 0.5);
    }

    #[test]
    fn footsteps_keep_time_with_the_stride_and_the_surface() {
        let w = SurfaceWorld(Surface::Asphalt);
        let mut c = at_origin();
        c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default());
        // Standing still makes no sound.
        let mut still = 0;
        for _ in 0..120 {
            still += c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default()).footsteps.len();
        }
        assert_eq!(still, 0, "no footsteps without moving");
        // Jogging for three seconds steps about once every stride, and each one
        // carries the ground class under the foot.
        let mut steps = Vec::new();
        let mut phase = c.stride_phase;
        for _ in 0..180 {
            let s = c.step(1.0 / 60.0, &CharacterControls { forward: 1.0, ..Default::default() }, &w, &SurfaceWeather::default());
            for surf in s.footsteps {
                assert_eq!(surf, Surface::Asphalt);
                assert!((c.stride_phase - phase - std::f64::consts::PI).abs() < 1e-9, "a half turn per step");
                phase = c.stride_phase;
                steps.push(surf);
            }
        }
        let travelled = c.pos.distance(origin());
        let want = (travelled / stride_len(RUN)).floor() as usize;
        assert!((steps.len() as i64 - want as i64).abs() <= 1, "{} steps in {travelled:.1} m, want about {want}", steps.len());
        assert!(steps.len() >= 8, "three seconds of jogging: {} steps", steps.len());
    }

    #[test]
    fn gait_phase_does_not_scrub_when_the_speed_changes() {
        let w = FlatWorld;
        let mut c = at_origin();
        c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default());
        let mut previous = c.gait_phase();
        for i in 0..180 {
            let controls = match (i / 30) % 3 {
                0 => CharacterControls { forward: 0.3, ..Default::default() },
                1 => CharacterControls { forward: 1.0, ..Default::default() },
                _ => CharacterControls { forward: 1.0, sprint: true, ..Default::default() },
            };
            c.step(1.0 / 60.0, &controls, &w, &SurfaceWeather::default());
            let phase = c.gait_phase();
            let advance = phase - previous;
            assert!(advance >= 0.0 && advance < 0.25, "phase jumped by {advance} at frame {i}");
            previous = phase;
        }
    }

    #[test]
    fn footstep_timbre_follows_the_surface() {
        let asphalt = footstep_timbre(Surface::Asphalt);
        let grass = footstep_timbre(Surface::Grass);
        let sand = footstep_timbre(Surface::Sand);
        let rock = footstep_timbre(Surface::Rock);
        assert!(asphalt.gain > grass.gain && grass.gain > sand.gain, "hard ground is louder");
        assert!(asphalt.bright > grass.bright && grass.bright > sand.bright, "and brighter");
        assert!(rock.bright > asphalt.bright && rock.gain > asphalt.gain, "rock rattles loudest");
        assert!(footstep_timbre(Surface::Water).thump < asphalt.thump, "water has no hard thump");
    }

    #[test]
    fn state_stays_finite_under_bad_inputs() {
        let w = FlatWorld;
        let mut c = at_origin();
        for dt in [1e9, f64::NAN, f64::INFINITY, -1.0, 0.0] {
            let s = c.step(dt, &CharacterControls { forward: 1.0, right: -1.0, sprint: true, jump: true, heading: None, ..Default::default() }, &w, &SurfaceWeather::default());
            assert!(s.pos.is_finite() && s.vel.is_finite(), "dt {dt}");
            assert!(c.pos.is_finite() && c.vel.is_finite() && c.health.is_finite());
        }
        // At the pole and at the antimeridian the local frame is still finite.
        for (lat, lon) in [(89.999f64, 0.0f64), (0.0, 179.999), (-89.999, -179.999)] {
            let mut c = Character::spawn(geo::lla_to_pos(lat.to_radians(), lon.to_radians(), 0.0), 0.0);
            let s = c.step(1.0 / 60.0, &CharacterControls { forward: 1.0, ..Default::default() }, &w, &SurfaceWeather::default());
            assert!(s.pos.is_finite() && s.vel.is_finite(), "{lat},{lon}");
        }
    }

    #[test]
    fn traffic_knocks_the_body_back_and_hurts() {
        let (east, _, _) = geo::enu(origin());
        let body = |pos: DVec3, velocity: DVec3| TrafficBody { kind: crate::world::TrafficKind::Moving, tile: crate::geo::TileId::new(14, 0, 0), index: 0, model: 0, colour: [0.4; 3], pos, yaw: std::f64::consts::FRAC_PI_2, half: crate::cars::player_geom(0).half, velocity };
        let mut c = at_origin();
        c.spawn_grace = 0.0;
        // A car sliding into the body: pushed clear, knocked back, hurt.
        let hit = c.hit_traffic(&[body(origin() + east * 1.6, -east * 10.0)]).expect("a contact");
        assert!(hit.damage > 0.0 && hit.speed > 4.0, "a real blow: {:.1} m/s, {:.0} damage", hit.speed, hit.damage);
        assert!(c.health < MAX_HEALTH, "health {:.0}", c.health);
        assert!((c.pos - origin()).dot(east) < -0.2, "pushed clear: {:.2} m", (c.pos - origin()).dot(east));
        assert!(c.vel.dot(east) < 0.0, "knocked back: {:.2} m/s", c.vel.dot(east));
        assert_eq!(c.since_hurt, 0.0);

        // A parked car is solid but does not hurt.
        let mut c = at_origin();
        c.spawn_grace = 0.0;
        assert!(c.hit_traffic(&[body(origin() + east * 1.6, DVec3::ZERO)]).is_none());
        assert_eq!(c.health, MAX_HEALTH);
        assert!((c.pos - origin()).dot(east) < -0.2, "still pushed clear");
        // With no traffic there is nothing to resolve.
        assert!(at_origin().hit_traffic(&[]).is_none());
    }

    #[test]
    fn a_bailout_lands_on_its_feet_and_walks_again() {
        let w = FlatWorld;
        let mut c = at_origin();
        c.pos = c.pos.normalize() * (geo::R + 380.0);
        c.grounded = false;
        c.spawn_grace = 0.0;
        c.last_safe = origin();
        c.fall = Some(crate::bailout::Fall::new(crate::bailout::Exit::Jump, 0.0));
        // Freefall for a second, then pull the ripcord as the player would.
        let mut landed = None;
        for i in 0..60 * 150 {
            let pull = i == 60;
            let s = c.step(1.0 / 60.0, &CharacterControls { jump: pull, ..Default::default() }, &w, &SurfaceWeather::default());
            if i == 60 {
                assert!(s.chute_opened, "the ripcord was pulled");
            }
            if s.landed.is_some() {
                landed = s.landed;
                break;
            }
        }
        assert!(landed.is_some(), "it comes down");
        assert!(c.fall.is_none() && c.grounded, "and the fall is over");
        assert_eq!(c.health, MAX_HEALTH, "a canopy landing is soft");
        assert!(height(&c).abs() < 0.05, "feet on the ground: {:.3}", height(&c));
        // Walking works straight away.
        run(&mut c, &w, CharacterControls { forward: 1.0, ..Default::default() }, 1.0);
        assert!(c.speed() > 3.0 && c.grounded, "walking again at {:.1} m/s", c.speed());
    }

    #[test]
    fn a_car_bailout_rolls_along_the_ground_and_stands_up() {
        let w = FlatWorld;
        let mut c = at_origin();
        let (east, _, up) = geo::enu(origin());
        c.pos = c.pos.normalize() * (geo::R + 0.3);
        c.vel = east * 12.0 + up * CAR_EXIT_UP;
        c.grounded = false;
        c.spawn_grace = 0.0;
        c.air = 1.0;
        c.car_bailout = Some(CarBailout::new());
        let start = c.pos;
        let mut rolled = false;
        let mut max_roll = 0.0f64;
        let mut phases = Vec::new();
        for _ in 0..240 {
            let s = c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default());
            rolled |= s.car_roll_started;
            if let Some(b) = c.car_bailout {
                max_roll = max_roll.max(b.roll);
                phases.push(b.phase);
            }
        }
        assert!(rolled, "the car bailout touched down and rolled");
        assert!(max_roll > 5.0, "one somersault: {max_roll:.2} rad");
        assert!(phases.contains(&CarBailoutPhase::Roll) && phases.contains(&CarBailoutPhase::Stand));
        assert!(c.car_bailout.is_none(), "back on the feet");
        assert!(c.grounded);
        assert_eq!(c.health, MAX_HEALTH, "the hop is survivable");
        let travelled = (c.pos - start).dot(east);
        assert!(travelled > 4.0, "momentum carries the body: {travelled:.1} m");
    }

    #[test]
    fn pressing_jump_opens_the_parachute_once() {
        let w = FlatWorld;
        let mut c = at_origin();
        c.pos = c.pos.normalize() * (geo::R + 2_000.0);
        c.grounded = false;
        c.fall = Some(crate::bailout::Fall::new(crate::bailout::Exit::Jump, 0.0));
        let jump = CharacterControls { jump: true, ..Default::default() };
        let s = c.step(1.0 / 60.0, &jump, &w, &SurfaceWeather::default());
        assert!(s.chute_opened, "Space pulled the ripcord");
        assert_eq!(c.fall.as_ref().map(|f| f.phase), Some(crate::bailout::Phase::Canopy));
        // Holding it does not change anything again.
        let s = c.step(1.0 / 60.0, &jump, &w, &SurfaceWeather::default());
        assert!(!s.chute_opened);
    }

    #[test]
    fn a_water_landing_is_reported_for_the_game_to_resolve() {
        let w = SurfaceWorld(Surface::Water);
        let mut c = at_origin();
        c.pos = c.pos.normalize() * (geo::R + 30.0);
        c.grounded = false;
        c.spawn_grace = 0.0;
        let mut fall = crate::bailout::Fall::new(crate::bailout::Exit::Door, 0.0);
        fall.deploy();
        fall.open = 1.0;
        c.fall = Some(fall);
        let mut water = None;
        for _ in 0..120 * 60 {
            let s = c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default());
            if let Some(v) = s.water {
                water = Some(v);
                break;
            }
        }
        assert!(water.is_some(), "splashdown reported");
        assert!(c.fall.is_none(), "the game takes the body out of the water");
        assert!(c.health > 0.0);
    }

    #[test]
    fn a_freefall_into_the_ground_is_fatal_and_respawns() {
        let w = FlatWorld;
        let mut c = at_origin();
        c.pos = c.pos.normalize() * (geo::R + 5.0);
        c.vel = -c.pos.normalize() * 55.0;
        c.grounded = false;
        c.spawn_grace = 0.0;
        c.last_safe = origin();
        c.fall = Some(crate::bailout::Fall::new(crate::bailout::Exit::Jump, 0.0));
        let mut died = false;
        for _ in 0..60 * 5 {
            let s = c.step(1.0 / 60.0, &CharacterControls::default(), &w, &SurfaceWeather::default());
            if s.died {
                died = true;
                break;
            }
        }
        assert!(died, "a 55 m/s arrival is not survivable");
        assert_eq!(c.respawns, 1);
        assert_eq!(c.health, MAX_HEALTH);
    }

    #[test]
    fn safe_point_skips_water_and_finds_dry_land() {
        struct HalfWater;
        impl WorldQuery for HalfWater {
            fn surface_at(&self, p: DVec3) -> Option<SurfaceHit> {
                let (east, _, _) = geo::enu(origin());
                let water = (p - origin()).dot(east) > -20.0;
                Some(SurfaceHit { h: 0.0, kind: if water { Surface::Water } else { Surface::Grass }, water, road: false, sidewalk: false, bridge: false })
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
        let w = HalfWater;
        let p = safe_point(&w, origin()).expect("dry land within reach");
        assert!(!w.surface_at(p).unwrap().blocked());
    }
}
