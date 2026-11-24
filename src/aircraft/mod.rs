//! The aircraft you can fly. Each type is one module with its model (built
//! with the toolkit in `airframe.rs`) and a `Spec`: everything the flight
//! model, the damage model, the landing gear, the engines, the camera and the
//! sound need to know about it. Adding a type is a new module and an entry in
//! `BUILTIN`; nothing else in the game is specific to one aircraft except its
//! paint and surface detail, which `plane.wgsl` picks by `Spec::id`.
//!
//! Body frame (as in airframe.rs): +X right, +Y up, -Z forward, the origin at
//! the centre of gravity.
// Aircraft geometry tables contain physical dimensions in metres (e.g. 1.57 m,
// 3.14 m, -6.28 m) that trip clippy::approx_constant.
#![allow(clippy::approx_constant)]
pub mod b747;
pub mod bonanza;
pub mod c172;
pub mod concorde;
pub mod dr400;
pub mod f22;
pub mod f4;
pub mod fa18;
pub mod g650;
pub mod sr71;
pub mod starship;
#[cfg(test)]
mod standards;

use crate::airframe::{Model, Sect};
use glam::{DVec3, Vec3};

/// A control in the cockpit that the mouse can work: a sphere in the body
/// frame, and the actions (ids from `bind::ACTIONS`) a click and the wheel
/// stand for. Clicking does what the action's key does; with a `hold`
/// action, keeping the button down past a moment holds that key instead
/// (the ignition key turned on to START); each notch of the wheel holds the "up" or
/// "down" action for a moment (mixture, throttle, trim, OBS).
#[derive(Clone, Copy, Debug)]
pub struct Hotspot {
    pub at: [f32; 3],
    pub r: f32,
    pub click: Option<&'static str>,
    /// held past a moment instead of clicked (the key turned on to START)
    pub hold: Option<&'static str>,
    pub wheel: Option<(&'static str, &'static str)>,
}

/// The hotspot a ray from `o` along `d` (body frame, `d` unit length) meets
/// first, if any.
pub fn pick(hs: &[Hotspot], o: Vec3, d: Vec3) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, h) in hs.iter().enumerate() {
        let c = Vec3::from(h.at) - o;
        let t = c.dot(d);
        if t <= 0.0 || c.length_squared() - t * t > h.r * h.r {
            continue;
        }
        if best.is_none_or(|(_, bt)| t < bt) {
            best = Some((i, t));
        }
    }
    best.map(|(i, _)| i)
}

impl Hotspot {
    pub fn click(at: Vec3, r: f32, id: &'static str) -> Self {
        Self { at: at.to_array(), r, click: Some(id), hold: None, wheel: None }
    }

    pub fn hold(at: Vec3, r: f32, id: &'static str) -> Self {
        Self { at: at.to_array(), r, click: None, hold: Some(id), wheel: None }
    }

    pub fn wheel(at: Vec3, r: f32, up: &'static str, down: &'static str) -> Self {
        Self { at: at.to_array(), r, click: None, hold: None, wheel: Some((up, down)) }
    }
}

/// Resolve the mouse targets once when loading the aircraft. Keep the exact
/// piece index so identical levers and inherited cockpit meshes animate correctly.
pub fn cockpit_controls(spec: &Spec, pieces: &[crate::airframe::Piece]) -> Vec<(Hotspot, Option<usize>)> {
    let mut controls: Vec<_> = spec.hotspots.iter().map(|h| {
        let index = c172::cabin::moving_piece(h).and_then(|name| pieces.iter().position(|p|
            p.name == name && match h.hold {
                Some("brake_left") => p.pivot.x < -0.27,
                Some("brake_right") => p.pivot.x > -0.27 && p.pivot.x < 0.0,
                _ => true,
            }));
        (*h, index)
    }).collect();
    if BUILTIN.iter().any(|s| std::ptr::eq(*s, spec)) {
        controls.extend(pieces.iter().enumerate().filter_map(|(i, p)| {
            let mut h = p.hotspot?;
            h.at = (p.pivot + Vec3::from(h.at)).to_array();
            Some((h, Some(i)))
        }));
    }
    controls
}

pub fn animated_hotspots(controls: &[(Hotspot, Option<usize>)], pose: &[glam::Mat4]) -> Vec<Hotspot> {
    controls.iter().map(|&(mut h, index)| {
        if let Some(i) = index {
            h.at = pose[i].transform_point3(Vec3::from(h.at)).to_array();
        }
        h
    }).collect()
}

/// The types built into the game, in the order the menu lists them.
pub static BUILTIN: [&Spec; 11] = [&c172::SPEC, &dr400::SPEC, &bonanza::SPEC, &b747::SPEC, &fa18::SPEC, &f4::SPEC, &f22::SPEC, &starship::SPEC, &concorde::SPEC, &sr71::SPEC, &g650::SPEC];

/// Types added while the game runs: JSBSim aircraft the player downloaded
/// (see hangar.rs), each flying with a stand-in from `BUILTIN` as its body.
static EXTRA: parking_lot::RwLock<Vec<&'static Spec>> = parking_lot::RwLock::new(Vec::new());

/// Every type: the built-in ones, then the downloaded ones.
pub fn list() -> Vec<&'static Spec> {
    BUILTIN.iter().copied().chain(EXTRA.read().iter().copied()).collect()
}

/// Adds a type (leaked: types live for the whole run), or replaces the one
/// answering to the same first key.
pub fn register(spec: Spec) -> &'static Spec {
    let s: &'static Spec = Box::leak(Box::new(spec));
    let mut x = EXTRA.write();
    match x.iter().position(|o| o.keys.first() == s.keys.first()) {
        Some(i) => x[i] = s,
        None => x.push(s),
    }
    s
}

/// Takes a downloaded type off the list (its memory stays: a flight may still use it).
pub fn unregister(key: &str) {
    EXTRA.write().retain(|o| o.keys.first() != Some(&key));
}

/// Where a type sits in `list()` (0 when it is gone).
pub fn index_of(spec: &Spec) -> usize {
    list().iter().position(|s| std::ptr::eq(*s, spec)).unwrap_or(0)
}

/// The type at an index of `list()`, the first when out of range.
pub fn nth(i: usize) -> &'static Spec {
    list().get(i).copied().unwrap_or(BUILTIN[0])
}

/// A type by its short name (`c172`, `747`, `jsbsim:p51d`...), as typed on the command line.
pub fn by_key(key: &str) -> Option<&'static Spec> {
    let k = key.to_ascii_lowercase();
    list().into_iter().find(|s| s.keys.iter().any(|x| x.to_ascii_lowercase() == k))
}

/// The most landing gear legs any type has.
pub const MAX_GEAR: usize = 6;

/// What a contact point on the airframe belongs to (see `Plane::contacts`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Part {
    /// spinner and cowling, or a jet's nose
    Nose,
    /// the propeller's tips
    Prop,
    /// the bottom of an engine pod hanging under that wing (0 left)
    Pod(usize),
    Belly,
    /// the cabin's sides
    Side,
    /// over the cabin and behind the wing
    Roof,
    /// the tail cone
    Tail,
    Fin,
    Stab(usize),
    /// a wing (side) at a span station
    Wing(usize, f32),
    /// a wing that is hanging from its stump
    Fold(usize),
}

/// One landing gear leg: where its tyres meet the ground fully extended
/// (body frame), and how it carries, steers and brakes the aircraft.
#[derive(Clone)]
pub struct Leg {
    pub at: DVec3,
    pub wheel_r: f64,
    /// oleo travel (m), and the spring and damper (N/m, N s/m) with the most force it takes
    pub travel: f64,
    pub k: f64,
    pub c: f64,
    pub nf_max: f64,
    /// how much of the steering angle it follows (1 a nose wheel)
    pub steer: f64,
    pub brakes: bool,
    /// tyre side force per m/s of sideways slip and its friction limit; rolling
    /// resistance likewise; braking force per m/s and its friction limit
    pub side_k: f64,
    pub side_mu: f64,
    pub roll_k: f64,
    pub roll_mu: f64,
    pub brake_k: f64,
    pub brake_mu: f64,
    /// sink rate (m/s) that folds it, and how it throws the aircraft about when it does
    pub fold_vn: f64,
    pub fold_kick: DVec3,
    /// what the HUD calls it
    pub name: &'static str,
}

/// How the airframe itself slides along the ground once it is down on it.
#[derive(Clone)]
pub struct Hull {
    pub k: f64,
    pub c: f64,
    pub nf_max: f64,
    /// friction force per m/s of sliding (before the friction limit)
    pub drag: f64,
}

/// Where the tail surfaces are, for breaking them and for what still touches the ground.
#[derive(Clone)]
pub struct Tail {
    /// height of the fin's root and how tall it is above that; the same for the rudder
    pub fin_y0: f32,
    pub fin_h: f32,
    pub rudder_y0: f32,
    pub rudder_h: f32,
    /// |x| beyond which a stabiliser half breaks off, and an elevator half
    pub stab_root: f32,
    pub elev_root: f32,
    /// where along the tail cone it snaps (z range)
    pub break_z: [f32; 2],
    /// the fin's root and tip, and a stabiliser tip (right side), as contact points
    pub fin_base: DVec3,
    pub fin_tip: DVec3,
    pub stab_tip: DVec3,
}

/// A propeller governor: past the revs the propeller lever selects it turns
/// the blades coarser, which loads the engine more, until the revs come back
/// down; below them it runs the blades back to the fine stop. The blade
/// setting `coarse` (0 at the fine stop, up to `coarse_max`) scales the
/// propeller's thrust and power coefficients and its advance ratios by
/// 1 + gain × coarse (`ct`, `cp`, `j`): a coarser blade bites at a higher
/// advance ratio, its thrust and power running out further along.
#[derive(Clone)]
pub struct Governor {
    /// the revs (rpm) it holds with the lever right back and fully forward
    pub rpm: [f64; 2],
    /// true: a constant-speed propeller, governing with the lever fully
    /// forward too; false: fully forward is the fine stop and it turns as a
    /// fixed-pitch propeller there, governing only once the lever comes back
    pub at_full: bool,
    pub coarse_max: f64,
    pub ct: f64,
    pub cp: f64,
    pub j: f64,
    /// how fast the blades move: `coarse` per second per rad/s off the revs selected
    pub rate: f64,
}

/// A piston engine turning a fixed-pitch propeller, or one with a governor.
#[derive(Clone)]
pub struct Piston {
    /// rated power (W) at rated revs (rad/s)
    pub power: f64,
    pub w_rated: f64,
    /// propeller diameter (m); thrust and power coefficients at rest; the
    /// advance ratios where thrust and then the power it takes run out
    pub prop_d: f64,
    pub ct0: f64,
    pub cp0: f64,
    pub j_t: f64,
    pub j_p: f64,
    /// propeller, crankshaft and flywheel (kg m²)
    pub prop_i: f64,
    /// about the static thrust at full power (scale for torque and slipstream effects)
    pub static_thrust: f64,
    /// friction and pumping losses: constant and at rated revs (N m)
    pub fric0: f64,
    pub fric1: f64,
    /// height of the propeller hub and where a broken blade snaps (body y)
    pub blade_break_y: f32,
    /// the propeller lever's governor (None: a fixed-pitch propeller, the
    /// lever does nothing); `ct0` to `j_p` above are then the fine stop's
    pub governor: Option<Governor>,
}

/// High-bypass turbofans.
#[derive(Clone)]
pub struct Fan {
    pub n: usize,
    /// sea-level static thrust of each (N)
    pub thrust: f64,
    /// fan speed at idle (fraction of N1) and the thrust it gives (fraction)
    pub idle: f64,
    pub idle_thrust: f64,
    /// fan revs at 100% N1
    pub n1_rpm: f64,
    /// how quickly the fan spools up and down (1/s near the top)
    pub spool_up: f64,
    pub spool_down: f64,
    /// reverse thrust as a fraction of forward, and how long the reversers take to deploy (s)
    pub reverse: f64,
    pub reverser_time: f64,
    /// thrust with the boost key held, as a multiple of full dry thrust (an afterburner)
    pub boost: f64,
    /// thrust lapse with Mach number m: 1 - a m + b m² (a high-bypass fan loses thrust
    /// as it speeds up; a low-bypass afterburning engine gains it from the ram air)
    pub mach_lapse: [f64; 2],
    /// station of the nozzle exits (body z), where the afterburners' light comes from
    pub nozzle_z: f64,
}

/// One stage of a rocket. Stations are body z (the nose is at -z).
#[derive(Clone)]
pub struct Stage {
    pub name: &'static str,
    /// empty and propellant masses (kg)
    pub dry: f64,
    pub prop: f64,
    /// thrust of all its engines together at sea level (N), and how many there are
    pub thrust: f64,
    pub n: u32,
    /// specific impulse at sea level and in a vacuum (s): thrust grows as the air thins
    pub isp: [f64; 2],
    /// the sets of engines that can be lit: name, how many burn, how many of those gimbal
    pub groups: &'static [(&'static str, u32, u32)],
    /// its forward end and its engines' exit plane
    pub z: [f64; 2],
    /// where the air's side force acts on it when it flies alone, as a fraction of its length from the front
    pub cp: f64,
    /// side force its control surfaces (grid fins, flaps) give per Pa of dynamic pressure at full deflection (N/Pa)
    pub fins: f64,
    /// dynamic pressure x the sine of the angle of attack (Pa) that breaks it up
    pub q_alpha: f64,
}

/// A liquid-fuelled rocket of two stages: the first (a booster) carries the
/// second until it is dropped (`Plane::stage`), see plane/rocket.rs.
#[derive(Clone)]
pub struct Rocket {
    pub stages: [Stage; 2],
    pub radius: f64,
    /// the lowest the engines throttle to (fraction of full thrust), and how far they gimbal (rad)
    pub throttle_min: f64,
    pub gimbal: f64,
    /// the second stage's flaps, the forward pair and the aft: the area of
    /// each (m²), how far out it is and its station (body x, z)
    pub flaps: [[f64; 3]; 2],
}

#[derive(Clone)]
pub enum Engine {
    Piston(Piston),
    Fan(Fan),
    Rocket(Rocket),
}

/// The handling: angular accelerations straight from dynamic pressure (see
/// `Plane::step`). `q_ref` is the dynamic pressure (Pa) at which control
/// authority is "1"; `damp_v` turns rotation rates into damping.
#[derive(Clone)]
pub struct Handling {
    pub q_ref: f64,
    pub a_max: f64,
    pub damp_v: f64,
    pub ad_min: f64,
    pub ad_max: f64,
    /// pitch: stiffness about the trimmed angle of attack (and without a
    /// tailplane), stick authority, how far the stick moves the angle of attack, damping
    pub pitch_k: f64,
    pub pitch_k_lost: f64,
    pub pitch_ctl: f64,
    pub alpha_ctl: f64,
    pub pitch_d: f64,
    /// roll: authority, damping, roll from sideslip
    pub roll_ctl: f64,
    pub roll_d: f64,
    pub roll_beta: f64,
    /// how much roll authority is left at high speed with the flaps up (outboard ailerons locked out)
    pub roll_clean: f64,
    /// yaw: rudder authority, weathercock stiffness, damping
    pub yaw_ctl: f64,
    pub yaw_beta: f64,
    pub yaw_d: f64,
    /// a banked aircraft rolling back level; adverse yaw from the ailerons
    pub dihedral: f64,
    pub adverse: f64,
    /// a stall: nose drop, wing drop, and how much a yaw rate feeds the wing drop
    pub stall_pitch: f64,
    pub stall_drop: f64,
    pub stall_yaw: f64,
}

/// The head-on outline of the hole it punches through a building (m, body
/// frame seen from ahead), see `destruct::Hitter`.
#[derive(Clone)]
pub struct Hole {
    /// fuselage half width and half height, its centre's height, corner radius
    pub fus: [f32; 4],
    /// wing: root height, dihedral (rise per metre out), half thickness at root and tip
    pub wing: [f32; 4],
    /// stabiliser height, half thickness, half span
    pub stab: [f32; 3],
    /// fin root height, height, half width at root and tip
    pub fin: [f32; 4],
    /// engine pods: (|x|, y, radius); radius 0 for none
    pub pods: [[f32; 3]; 2],
}

/// What a flap lever works on a type with no flaps: Concorde's nose and visor.
/// (To the flight and damage models they are still its flaps: the notches, the
/// speeds they are good for, the two flap roles.)
#[derive(Clone)]
pub struct Lever {
    /// the keys (in the language files) of what it is called and of each notch
    pub name: &'static str,
    pub notches: &'static [&'static str],
    /// the pieces with the left and the right flap role, as the damage messages name them
    pub parts: [&'static str; 2],
}

impl Lever {
    pub fn notch(&self, k: u8) -> &'static str {
        self.notches.get(k as usize).map_or("", |n| crate::i18n::tr(n))
    }

    pub fn label(&self) -> &'static str {
        crate::i18n::tr(self.name)
    }
}

/// A braking parachute, streamed on landing with the speedbrake key and let
/// go with it again: it is used once.
#[derive(Clone)]
pub struct Chute {
    /// its drag area (m²: drag coefficient times canopy area)
    pub cda: f64,
    /// the fastest it may be streamed at (m/s indicated; faster tears it away),
    /// and the speed it is let go at by itself, before it drags on the runway
    pub v_max: f64,
    pub v_min: f64,
}

#[derive(Clone)]
pub enum Sound {
    Piston,
    Jet,
    /// a pair of afterburning engines, from recordings (audio.rs `Fighter`)
    Fighter,
    /// a pair of afterburning turbojets: the fighter's recordings played lower and rougher, with the J79's howl
    Turbojet,
    /// a cluster of rocket engines (audio.rs `Rocket`)
    Rocket,
}

/// A heat-seeking missile on a hardpoint: the model piece that is the loaded
/// missile (the game hides it when fired and draws it as the projectile), the
/// wing it hangs from (0 left, 1 right), and what the HUD calls it.
#[derive(Clone, Copy, Debug)]
pub struct MissileStation {
    pub piece: &'static str,
    pub side: usize,
    pub name: &'static str,
}

/// A paint scheme the aircraft picker offers for a type (`liveries`). The
/// shader takes it as `Paint::a` .. `Paint::d` and `Paint::title` (plane.wgsl);
/// what the colours and the style mean is up to each type's livery function.
pub struct Livery {
    pub name: &'static str,
    /// four colours, linear RGB; `colours[0][3]` is the style (which design
    /// the type's livery function draws); `colours[1][3]` is overwritten
    /// (`livery_uniform`); the other w's are free
    pub colours: [[f32; 4]; 4],
    /// an airline's name painted on, if the design has one: capitals, digits
    /// and spaces, at most 15 (glyphs of plane.wgsl's stroke font)
    pub title: &'static str,
    /// a paint of its own instead of the type's `Spec::paint`
    pub paint: Option<&'static Paint>,
    /// the picker's swatch: its main colour over its trim, linear RGB
    pub swatch: [[f32; 3]; 2],
    /// its picture in the aircraft picker (680 x 392), rendered with `--still`
    /// (assets/aircraft/liveries/stills.sh)
    pub still: &'static [u8],
}

/// The paint schemes the picker offers for a type: those painted by the
/// shader, or baked by script; none for a type that has only its own.
pub fn liveries(spec: &Spec) -> &'static [Livery] {
    if std::ptr::eq(spec, &c172::SPEC) { c172::LIVERIES }
    else if std::ptr::eq(spec, &dr400::SPEC) { dr400::LIVERIES }
    else if std::ptr::eq(spec, &bonanza::SPEC) { bonanza::LIVERIES }
    else if std::ptr::eq(spec, &b747::SPEC) { b747::LIVERIES }
    else if std::ptr::eq(spec, &g650::SPEC) { g650::LIVERIES }
    else { &[] }
}

/// The scheme a type starts in (an index of `liveries`).
pub fn default_livery(spec: &Spec) -> usize {
    if std::ptr::eq(spec, &c172::SPEC) { c172::DEFAULT_LIVERY } else { 0 }
}

/// A scheme as the shader's uniform takes it (`Paint::a` .. `Paint::title`):
/// the four colours, then the title, 8 bits a glyph with its length in the
/// top byte. `b.w` is the flown type's kind (`Spec::id`) + 1: the uniform is
/// shared by every aircraft drawn, so other types (multiplayer, wreckage)
/// keep their own default look (plane.wgsl `livery_ours`). No scheme: all
/// zero but that (each livery function's style 0).
pub fn livery_uniform(spec: &Spec, l: Option<&Livery>) -> [u32; 20] {
    let mut u = [0u32; 20];
    if let Some(l) = l {
        for (k, c) in l.colours.iter().flatten().enumerate() {
            u[k] = c.to_bits();
        }
    }
    u[7] = (spec.id as f32 + 1.0).to_bits();
    let Some(l) = l else { return u };
    let n = l.title.chars().count().min(15);
    for (k, c) in l.title.chars().take(15).enumerate() {
        u[16 + k / 4] |= b747::deck::glyph(c) << (8 * (k % 4));
    }
    for k in n..15 {
        u[16 + k / 4] |= 15 << (8 * (k % 4));
    }
    u[19] |= (n as u32) << 24;
    u
}

/// A painted skin: pictures of the aircraft seen from straight above, from
/// below and from its left side (and from its right, if `views` is 4;
/// otherwise the left one serves both sides, mirrored), side by side in one
/// JPEG, each nose up. plane.wgsl projects them back onto the skin
/// (`hornet_paint`). `span` x `length` (m) is the rectangle each picture
/// covers, GUIDE_SPAN and GUIDE_LEN in the type's Blender script; `centre`
/// the station ahead of the datum and the height above it at the middle of
/// the pictures (GUIDE_Y, GUIDE_Z).
#[derive(Clone)]
pub struct Paint {
    pub jpg: &'static [u8],
    pub views: u32,
    pub span: f64,
    pub length: f64,
    pub centre: [f64; 2],
    /// how much relief the picture's lines stand for (1: a line as dark as it
    /// looks is that deep; more for a dark paint, whose seams hardly differ
    /// from it in brightness), and how much its sheen varies with its tone
    /// (0: not at all)
    pub relief: f32,
    pub sheen: f32,
}

#[derive(Clone)]
pub struct Spec {
    /// what plane.wgsl knows it by (its paint scheme, windows, panel lines)
    pub id: u32,
    pub name: &'static str,
    /// names it answers to on the command line
    pub keys: &'static [&'static str],
    pub blurb: &'static str,
    /// where it comes from when it is not one of ours: a JSBSim aircraft
    /// flown with our flight model, drawn as one of ours (see hangar.rs)
    pub origin: Option<&'static Origin>,
    /// its own 3D model (a FlightGear aircraft's, see fgmodel.rs), built instead of `build`
    pub ext: Option<&'static crate::fgmodel::Ext>,
    /// its picture in the start screen's aircraft window: a PNG with alpha,
    /// 680 x 392 (the window's 340 x 196 at twice the size, for hi-dpi)
    pub still: &'static [u8],
    /// its painted skin (airframe `pat` 17), if it has one
    pub paint: Option<Paint>,
    pub build: fn() -> Model,
    /// the breakable groups (see airframe::G_*): name, mass (kg)
    pub groups: [(&'static str, f32); 7],

    // ---- geometry the flight and damage models need
    /// span station (m out from the centreline) of the wing tip
    pub span: f32,
    /// the right wing's section at a span station
    pub wing_sect: fn(f32) -> Sect,
    /// fuselage section at station z: top, the height of its widest point, bottom (body y)
    pub fuse: fn(f32) -> (f32, f32, f32),
    /// span stations the ailerons run between, and the fuel tanks
    pub aileron: [f32; 2],
    pub tank: [f32; 2],
    pub tail: Tail,
    /// the pilot's eye
    pub eye: Vec3,
    /// the eye in its other cockpit, if it has two (the view key goes round: outside, front seat, back seat)
    pub eye2: Option<Vec3>,
    /// does a view from the eye along this direction see the cabin (rather than out a window)?
    pub sees_inside: fn(Vec3, Vec3) -> bool,
    /// the cockpit's controls that can be clicked or turned with the mouse wheel
    pub hotspots: &'static [Hotspot],
    /// points of the fuselage that can touch the ground, and of the tail cone
    pub contacts: &'static [([f64; 3], Part)],
    pub tail_pts: &'static [[f64; 3]],
    /// wing contact stations as fractions of the span
    pub wing_st: [f32; 5],
    /// points that must not touch anything: nose, left tip, right tip, tail,
    /// fin top, belly, lowest point of the propeller or engine pods
    pub hard: [DVec3; 7],
    pub gear: &'static [Leg],
    pub hull: Hull,
    pub hole: Hole,

    // ---- mass and aerodynamics
    /// mass with the tanks half full (kg); the fuel on board adds or takes away from it
    pub mass: f64,
    /// usable fuel with the tanks full (kg), and what the engines burn
    /// together (kg/s): idling, at full dry power, and with the boost (afterburners)
    pub fuel_kg: f64,
    pub fuel_flow: [f64; 3],
    /// payload (crew, passengers, baggage, kg): what `mass` includes, the most
    /// it may carry, and how far aft of the reference CG more of it sits (m)
    pub payload: [f64; 3],
    /// balance: the mean aerodynamic chord (m); the centre of gravity at
    /// `mass` and its forward and aft limits (fractions of it); how far aft of
    /// the reference CG the fuel sits (m)
    pub mac: f64,
    pub cg: [f64; 3],
    pub fuel_arm: f64,
    /// pitch, yaw, roll (kg m²)
    pub inertia: DVec3,
    pub wing_area: f64,
    /// wing span (m)
    pub b: f64,
    /// lift at zero angle of attack and its slope per radian
    pub cl0: f64,
    pub cl_a: f64,
    /// drag: clean, the induced-drag factor, the lowered gear, flaps (linear and squared in the extension)
    pub cd0: f64,
    pub k_ind: f64,
    pub cd_gear: f64,
    pub cd_flap: [f64; 2],
    /// lift the flaps add at full travel
    pub flap_cl: f64,
    /// stall angle of attack clean, how much full flaps take off it and leading-edge devices add
    pub stall_alpha: f64,
    pub stall_flap: f64,
    pub stall_slat: f64,
    /// the highest angle of attack the trim will hold
    pub alpha_trim: f64,
    /// Mach number where wave drag starts (0: never flies fast enough to care)
    pub mcrit: f64,
    /// supersonic wave drag it levels off at by `mcrit` + 0.25 (0: Lock's fourth-power rise, for subsonic types)
    pub cd_wave: f64,
    /// spoilers fully up: the drag they add and the lift they dump
    pub spoiler_cd: f64,
    pub spoiler_cl: f64,
    /// a braking parachute on the speedbrake key, for a type with no speedbrake
    pub chute: Option<Chute>,
    /// flap lever notches (degrees) and the fastest each may be out at (m/s indicated)
    pub flaps: &'static [f64],
    pub vfe: &'static [f64],
    /// how quickly the flaps run (fraction of full travel per second)
    pub flap_rate: f64,
    /// from which notch the leading-edge devices are out (0: it has none)
    pub slats_at: u8,
    /// what the flap lever works, when that is not flaps (None: flaps)
    pub lever: Option<Lever>,
    /// how fast the gear goes up or down (fraction per second; 0: fixed gear)
    pub gear_rate: f64,
    /// the flaps and gear run on electric motors: without power on the bus
    /// (master off, battery flat and no alternator) they stay where they are
    pub electric_actuators: bool,
    /// never-exceed speed (m/s indicated) and Mach limit (0: none); flutter sets in at `flutter_v`
    pub vne: f64,
    pub mmo: f64,
    pub flutter_v: f64,
    /// a turbo-ramjet's design Mach number (0: an ordinary engine): past it the
    /// inlets choke and the thrust falls away, and its afterburners' fuel is
    /// metered to the air they get, lapsing with height as the thrust does
    /// (other afterburners burn at their sea-level rate wherever they are)
    pub inlet_mach: f64,
    /// the Mach number by which its thrust without the afterburners has all
    /// but gone (0: dry and afterburning thrust lapse alike): a turbojet built
    /// to cruise in afterburner, whose core does less and less past Mach 2
    pub dry_mach: f64,
    /// load where the spar starts to fatigue, and where it snaps
    pub g_limit: f32,
    pub g_break: f32,
    /// fly-by-wire: the most load (g) the stick can ask for (0: none, the stick moves the elevator)
    pub fbw_g: f64,
    /// nose wheel steering: largest angle, and the speed it fades down to a fifth by (0: it does not)
    pub steer_max: f64,
    pub steer_fade: f64,
    pub engine: Engine,
    pub handling: Handling,

    // ---- the game around it
    /// starting speed in the air, and on a final approach (m/s) with that flap notch
    pub cruise: f64,
    pub approach: (f64, u8),
    /// chase camera distance and the size of its own fine shadow map (m)
    pub cam_dist: f64,
    pub shadow_r: f32,
    pub sound: Sound,
}

/// A downloaded JSBSim aircraft behind a `Spec`.
pub struct Origin {
    /// the license its files state (or the collection's, when they state none), and who made it
    pub license: String,
    pub author: String,
    /// the built-in type whose body, cockpit and paint it borrows
    pub body: &'static str,
}

impl std::fmt::Debug for Spec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

impl Spec {
    /// The heat-seeking missiles it carries, if any. (Only the Hornet has
    /// them modelled; other types get an empty list.)
    pub fn missiles(&self) -> &'static [MissileStation] {
        if self.keys.contains(&"fa18") { &fa18::MISSILES } else { &[] }
    }

    /// Gear that never retracts (no fold rate): always down, the gear key does nothing.
    pub fn fixed_gear(&self) -> bool {
        self.gear_rate == 0.0 && self.rocket().is_none()
    }
    pub fn is_jet(&self) -> bool {
        matches!(self.engine, Engine::Fan(_))
    }
    /// It has a real afterburner or reheat (the boost key works in every
    /// mode); otherwise boost is the game's own, for arcade mode only.
    pub fn real_boost(&self) -> bool {
        matches!(&self.engine, Engine::Fan(f) if f.boost >= 1.2)
    }
    pub fn rocket(&self) -> Option<&Rocket> {
        match &self.engine {
            Engine::Rocket(r) => Some(r),
            _ => None,
        }
    }
    /// The flap notch fraction (of full travel) for a lever notch.
    pub fn flap_frac(&self, notch: u8) -> f64 {
        let full = *self.flaps.last().unwrap_or(&1.0);
        self.flaps.get(notch as usize).map(|d| d / full.max(1e-6)).unwrap_or(1.0)
    }
    pub fn max_notch(&self) -> u8 {
        (self.flaps.len().max(1) - 1) as u8
    }
}

#[cfg(test)]
mod hotspot_tests {
    use super::*;

    #[test]
    fn crewed_aircraft_have_bound_and_reachable_flight_controls() {
        let known = |id: &str| crate::bind::ACTIONS.iter().any(|a| a.id == id) || c172::cabin::MOUSE_ACTIONS.contains(&id);
        let mut hidden = Vec::new();
        for spec in BUILTIN {
            let model = (spec.build)();
            let controls = cockpit_controls(spec, &model.pieces);
            if std::ptr::eq(spec, &starship::SPEC) {
                assert!(controls.is_empty());
                continue;
            }
            for (h, _) in &controls {
                for id in h.click.iter().chain(h.hold.iter()).chain(h.wheel.iter().flat_map(|w| [&w.0, &w.1])) {
                    assert!(known(id), "{}: no action {id}", spec.name);
                }
            }
            let mut plane = crate::plane::Plane::spawn(spec, 0.6, -2.1, 1000.0, 0.0, 55.0);
            let blockers = crate::cockpit::blockers(&model.pieces);
            for extended in [false, true] {
                plane.gear_down = extended;
                plane.throttle = if extended { 0.0 } else { 1.0 };
                let pose = plane.pose(&model.pieces);
                let targets = animated_hotspots(&controls, &pose);
                let target_action = |h: &Hotspot| h.click.or(h.wheel.map(|w| w.0)).or(h.hold);
                let actions: std::collections::BTreeSet<_> = targets.iter().filter_map(target_action).collect();
                assert!(actions.contains("throttle_up"));
                assert_eq!(actions.contains("gear"), spec.gear_rate > 0.0);
                for action in actions {
                    let reachable = targets.iter().enumerate().any(|(i, h)| {
                        if target_action(h) != Some(action) { return false; }
                        let delta = Vec3::from(h.at) - spec.eye;
                        let yaw = delta.x.atan2(-delta.z) as f64;
                        let pitch = (-delta.y / delta.length()).asin() as f64;
                        // The head moves forward and down when looking at a console.
                        let mut head = crate::cockpit::Head::new(yaw, pitch);
                        [(0.0, 0.0), (-0.14, -0.05), (0.14, -0.05), (0.0, -0.22),
                         (-0.15, -0.35), (0.15, -0.35), (0.0, -0.40)].into_iter().any(|(x, y)| {
                            head.center_seat();
                            head.lean_by(x, y);
                            let view = head.update(spec, 1.0, (yaw, pitch), 58.0, DVec3::ZERO,
                                glam::DQuat::IDENTITY, DVec3::Y, 1.5, 1.0);
                            let eye = view.eye.as_vec3();
                            let ray = Vec3::from(h.at) - eye;
                            let dir = ray.normalize();
                            let picked = pick(&targets, eye, dir);
                            let limit = ray.length() - h.r - 0.005;
                            let clear = !crate::cockpit::occluded(&model.pieces, &pose, &blockers, eye, dir, limit);
                            picked == Some(i) && clear
                        })
                    });
                    if !reachable { hidden.push((spec.name, action, extended)); }
                }
            }
        }
        assert!(hidden.is_empty(), "Unreachable controls {hidden:?}");
    }

    #[test]
    fn animated_targets_follow_actual_throttle_and_gear_geometry() {
        let mut builder = crate::airframe::Builder::default();
        c172::cabin::build(&mut builder);
        let pieces = builder.pieces;
        let mut plane = crate::plane::Plane::spawn(&c172::SPEC,0.6,-2.1,1000.0,0.0,55.0);
        for state in [0.0,0.5,1.0] {
            plane.throttle = state;
            plane.gear_down = state > 0.0;
            let pose = plane.pose(&pieces);
            let hs = animated_hotspots(&cockpit_controls(&c172::SPEC, &pieces), &pose);
            for (name, point, action) in [
                ("throttle",Vec3::new(0.0,-0.195,c172::cabin::PANEL_Z+0.043),"throttle_up"),
                ("gear handle",c172::cabin::GEAR_PIVOT + Vec3::Z * 0.05,"gear")
            ] {
                let i = pieces.iter().position(|p| p.name == name).unwrap();
                let visible = pose[i].transform_point3(point);
                let hit = pick(&hs,c172::SPEC.eye,(visible-c172::SPEC.eye).normalize()).expect(name);
                assert_eq!(hs[hit].click.or(hs[hit].wheel.map(|w| w.0)),Some(action),"{name} at {state}");
            }
        }
    }

    /// From the pilot's eye, every target is the first one its own ray
    /// meets, and looking out ahead at the horizon meets none: the static
    /// `Spec::hotspots` and the ones on moving pieces as they are posed (gear
    /// down at idle, gear up at full power), as `hotspot_tests` resolves them.
    #[test]
    fn the_pilot_can_point_at_every_hotspot() {
        let mut bad = Vec::new();
        for s in BUILTIN.iter() {
            let model = (s.build)();
            let controls = cockpit_controls(s, &model.pieces);
            let mut plane = crate::plane::Plane::spawn(s, 0.6, -2.1, 1000.0, 0.0, 55.0);
            for extended in [true, false] {
                plane.gear_down = extended;
                plane.throttle = if extended { 0.0 } else { 1.0 };
                let targets = animated_hotspots(&controls, &plane.pose(&model.pieces));
                for (i, h) in targets.iter().enumerate() {
                    let d = (Vec3::from(h.at) - s.eye).normalize();
                    let hit = pick(&targets, s.eye, d);
                    if hit != Some(i) {
                        let id = |h: &Hotspot| h.click.or(h.wheel.map(|w| w.0)).or(h.hold).unwrap_or("?");
                        bad.push(format!("{}: {} (gear {}) hidden behind {:?}", s.name, id(h), if extended { "down" } else { "up" }, hit.map(|k| id(&targets[k]))));
                    }
                }
                // Looking out ahead, at the horizon: nothing.
                if let Some(k) = pick(&targets, s.eye, Vec3::new(0.0, 0.05, -1.0).normalize()) {
                    bad.push(format!("{}: looking out ahead picks {:?}", s.name, targets[k]));
                }
            }
        }
        assert!(bad.is_empty(), "\n{}", bad.join("\n"));
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;

    /// No vertex of any aircraft may be NaN or infinite, and the bounds (the
    /// box temporal anti-aliasing tells the aircraft's pixels by) hold every
    /// solid vertex. A NaN in the C172's door pocket once shrank that box to
    /// the fuselage, and the wings smeared and stair-stepped in flight.
    #[test]
    fn every_vertex_is_finite_and_inside_the_bounds() {
        let mut bad = Vec::new();
        for spec in BUILTIN {
            let model = (spec.build)();
            let (c, h) = model.bounds();
            for (i, p) in model.pieces.iter().enumerate() {
                let nan: Vec<usize> = (0..p.mesh.v.len()).filter(|&k| !Vec3::from(p.mesh.v[k].pos).is_finite()).collect();
                if let Some(&k) = nan.first() {
                    let v = &p.mesh.v[k];
                    bad.push(format!("{}: piece {i} ({}) has {} non-finite vertices (first #{k} of {}, pos {:?}, mat {:?})", spec.name, p.name, nan.len(), p.mesh.v.len(), v.pos, v.mat));
                }
                if !p.glass && p.vis != crate::airframe::Vis::Chute {
                    if let Some(pos) = p.mesh.v.iter().map(|v| Vec3::from(v.pos)).find(|&q| q.is_finite() && ((q - c).abs() - h).max_element() > 1e-3) {
                        bad.push(format!("{}: piece {i} vertex {pos:?} outside bounds {c:?} ± {h:?}", spec.name));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "\n{}", bad.join("\n"));
    }
}

#[cfg(test)]
mod boost_tests {
    use super::*;

    /// Real afterburners and reheat work in every mode; the game's own boost
    /// (pistons, the 747) is arcade mode's.
    #[test]
    fn real_afterburners() {
        let real: Vec<&str> = BUILTIN.iter().filter(|s| s.real_boost()).map(|s| s.keys[0]).collect();
        let want: Vec<&str> = BUILTIN.iter().map(|s| s.keys[0]).filter(|k| ["fa18", "f4", "f22", "sr71", "concorde"].iter().any(|w| k.contains(w))).collect();
        assert_eq!(real, want, "keys: {:?}", BUILTIN.iter().map(|s| s.keys[0]).collect::<Vec<_>>());
        assert!(!c172::SPEC.real_boost() && !b747::SPEC.real_boost());
    }
}
