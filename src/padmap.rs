//! The gamepad's flying layout, which can be moved about in Settings ›
//! Controls › Gamepad. It starts as Microsoft Flight Simulator (2020) lays
//! out an Xbox pad, with Y held as a modifier for what that leaves out
//! (see `Game::gamepad` for the whole table). Kept in bindings.json with
//! the rest of the bindings (bind.rs), as what differs from that layout.
//!
//! - A pad action stands for a key (`bind::ACTIONS`): pressed, it does what
//!   its key does, and its key is held while it is. A few are the pad's own:
//!   the quick views and recentring the view.
//! - A binding is a button, or Y + a button. While Y is held, a button with
//!   a Y binding does that and not its own; Y on its own, let go in flight
//!   without having been used with another button, gets out of the aircraft.
//! - The menus' buttons are not part of it: the d-pad and left stick move,
//!   A presses, B goes back, LB / RB turn the page, the right stick scrolls,
//!   so the pad can always find its way back here. In flight only Menu is
//!   held back (it pauses); the others are free to fly with.
//! - The sticks and triggers drive the flight axes and the look-around.

use std::collections::HashMap;
use winit::keyboard::KeyCode;

/// A button of an Xbox-style pad, by the name on it.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum PadBtn {
    A,
    B,
    X,
    Y,
    LB,
    RB,
    LT,
    RT,
    View,
    Menu,
    L3,
    R3,
    Up,
    Down,
    Left,
    Right,
}

impl PadBtn {
    pub const ALL: [PadBtn; 16] = {
        use PadBtn::*;
        [A, B, X, Y, LB, RB, LT, RT, View, Menu, L3, R3, Up, Down, Left, Right]
    };

    /// Its name, as `padui` draws it and the file keeps it.
    pub fn name(self) -> &'static str {
        use PadBtn::*;
        match self {
            A => "A",
            B => "B",
            X => "X",
            Y => "Y",
            LB => "LB",
            RB => "RB",
            LT => "LT",
            RT => "RT",
            View => "View",
            Menu => "Menu",
            L3 => "L3",
            R3 => "R3",
            Up => "Up",
            Down => "Down",
            Left => "Left",
            Right => "Right",
        }
    }

    fn parse(s: &str) -> Option<PadBtn> {
        PadBtn::ALL.into_iter().find(|b| b.name() == s)
    }

    pub fn from_gilrs(b: gilrs::Button) -> Option<PadBtn> {
        PadBtn::ALL.into_iter().find(|p| p.gilrs() == b)
    }

    pub fn gilrs(self) -> gilrs::Button {
        use gilrs::Button as G;
        use PadBtn::*;
        match self {
            A => G::South,
            B => G::East,
            X => G::West,
            Y => G::North,
            LB => G::LeftTrigger,
            RB => G::RightTrigger,
            LT => G::LeftTrigger2,
            RT => G::RightTrigger2,
            View => G::Select,
            Menu => G::Start,
            L3 => G::LeftThumb,
            R3 => G::RightThumb,
            Up => G::DPadUp,
            Down => G::DPadDown,
            Left => G::DPadLeft,
            Right => G::DPadRight,
        }
    }
}

/// A button, or Y + a button. (Y itself is never `y`: Y alone is `Y_ALONE`.)
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub struct Chord {
    pub btn: PadBtn,
    pub y: bool,
}

const fn b(btn: PadBtn) -> Chord {
    Chord { btn, y: false }
}

const fn y(btn: PadBtn) -> Chord {
    Chord { btn, y: true }
}

/// Y pressed and let go on its own.
pub const Y_ALONE: Chord = b(PadBtn::Y);

/// Menu pauses, always: the one flying button that is not to be moved.
pub const PAUSE: Chord = b(PadBtn::Menu);

impl Chord {
    /// As `padui` draws it: "Y + LB", "A".
    pub fn spec(self) -> String {
        if self.y { format!("Y + {}", self.btn.name()) } else { self.btn.name().to_string() }
    }

    fn text(self) -> String {
        if self.y { format!("Y+{}", self.btn.name()) } else { self.btn.name().to_string() }
    }

    fn parse(s: &str) -> Option<Chord> {
        match s.strip_prefix("Y+") {
            Some(r) => PadBtn::parse(r).filter(|b| *b != PadBtn::Y).map(y),
            None => PadBtn::parse(s).map(b),
        }
    }
}

/// The pad's own actions, which no key does.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum View {
    Ahead,
    Down,
    Left,
    Right,
    Reset,
}

const VIEWS: [(&str, View); 5] = [("view_ahead", View::Ahead), ("view_down", View::Down), ("view_left", View::Left), ("view_right", View::Right), ("view_reset", View::Reset)];

/// The layout it starts with: Microsoft Flight Simulator's, then Y + a
/// button for the rest. (The order of the bindings page.)
const LAYOUT: &[(&str, Chord)] = {
    use PadBtn::*;
    &[
        ("throttle_up", b(A)),
        ("throttle_down", b(B)),
        ("brakes", b(X)),
        ("parking_brake", y(B)),
        ("trim_down", y(Up)),
        ("trim_up", y(Down)),
        ("flaps_up", b(LB)),
        ("flaps_down", b(RB)),
        ("gear", b(L3)),
        ("view_reset", b(R3)),
        ("view_ahead", b(Up)),
        ("view_down", b(Down)),
        ("view_left", b(Left)),
        ("view_right", b(Right)),
        ("camera", b(View)),
        ("flight_panel", y(View)),
        ("boost", y(A)),
        ("reversers", y(X)),
        ("speedbrake", y(LB)),
        ("ap", y(RB)),
        ("engine_auto", y(L3)),
        ("hud", y(R3)),
        ("canopy", y(Menu)),
        ("master_arm", y(Left)),
        ("target_lock", y(Right)),
        ("fire_missile", y(RT)),
    ]
};

/// On a rocket these do something else (and are not listed on their own).
const ROCKET: [(&str, &str); 3] = [("ap", "autoland"), ("master_arm", "engines"), ("target_lock", "stage")];

/// The pad on foot: both sticks, and a button for each foot action. Shown in
/// Settings › Controls › On foot; the pad's flying layout is unchanged.
pub const FOOT_LAYOUT: &[(&str, Chord)] = {
    use PadBtn::*;
    &[("sprint", b(RB)), ("jump", b(A)), ("interact", b(Y)), ("first_person", b(R3))]
};

/// What the pad does on foot, as the controls list shows it: (input, does).
pub fn foot_rows() -> Vec<(String, &'static str)> {
    let mut rows = vec![("LS".to_string(), "move"), ("RS".to_string(), "look around")];
    for &(id, c) in FOOT_LAYOUT {
        rows.push((c.spec(), label(id)));
    }
    rows
}

/// The pad in a car: the left stick steers, A accelerates, X brakes and
/// reverses, Y gets out (as it gets in on foot), and a button each for the
/// handbrake, the camera and the horn. Shown in Settings › Controls › In a car; the flying and on-foot
/// layouts are unchanged.
pub const CAR_LAYOUT: &[(&str, Chord)] = {
    use PadBtn::*;
    &[("car_throttle", b(A)), ("car_brake", b(X)), ("car_handbrake", b(RB)), ("car_enter_exit", b(Y)), ("camera", b(R3)), ("car_horn", b(L3))]
};

/// What the pad does in a car, as the controls list shows it: (input, does).
pub fn car_rows() -> Vec<(String, &'static str)> {
    use crate::i18n::t;
    let mut rows = vec![("LS".to_string(), t!("gamepad.steer"))];
    for &(id, c) in CAR_LAYOUT {
        rows.push((c.spec(), label(id)));
    }
    rows
}

/// Keys left off the pad: the stick and pedals are axes on it, and Menu pauses.
const NOT_ON_PAD: [&str; 7] = ["pitch_down", "pitch_up", "roll_left", "roll_right", "rudder_left", "rudder_right", "pause_menu"];

/// Every action the pad can be given, in the bindings page's order, with
/// its default: the layout's, then everything else the keys do (unbound).
pub fn actions() -> &'static [(&'static str, Option<Chord>)] {
    static ALL: std::sync::OnceLock<Vec<(&'static str, Option<Chord>)>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut v: Vec<_> = LAYOUT.iter().map(|&(id, c)| (id, Some(c))).collect();
        for a in crate::bind::ACTIONS {
            let skip = NOT_ON_PAD.contains(&a.id) || ROCKET.iter().any(|r| r.1 == a.id) || v.iter().any(|(id, _)| *id == a.id);
            if !skip {
                v.push((a.id, None));
            }
        }
        v
    })
}

fn known(id: &str) -> Option<&'static str> {
    actions().iter().find(|(a, _)| *a == id).map(|(a, _)| *a)
}

fn default_of(id: &str) -> Option<Chord> {
    actions().iter().find(|(a, _)| *a == id).and_then(|(_, c)| *c)
}

/// What an action does on the pad: a key (the rocket's own on a rocket),
/// or one of the pad's views.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Does {
    Key(KeyCode),
    View(View),
}

pub fn does(id: &str, rocket: bool) -> Option<Does> {
    if let Some((_, v)) = VIEWS.iter().find(|(v, _)| *v == id) {
        return Some(Does::View(*v));
    }
    let id = if rocket { ROCKET.iter().find(|r| r.0 == id).map(|r| r.1).unwrap_or(id) } else { id };
    crate::bind::ACTIONS.iter().find(|a| a.id == id).map(|a| Does::Key(a.key))
}

/// An action's name on the bindings page and in the controls list.
pub fn label(id: &str) -> &'static str {
    use crate::i18n::t;
    match id {
        "view_ahead" => t!("gamepad.look_ahead"),
        "view_down" => t!("gamepad.look_down_behind"),
        "view_left" => t!("gamepad.look_left"),
        "view_right" => t!("gamepad.look_right"),
        "view_reset" => t!("gamepad.reset_the_view"),
        "ap" => t!("gamepad.autopilot_rocket_autoland"),
        "master_arm" => t!("gamepad.master_arm_rocket_engines"),
        "target_lock" => t!("gamepad.lock_target_rocket_staging"),
        "fire_missile" => t!("gamepad.fire_missile"),
        _ => crate::bind::every().find(|a| a.id == id).map(|a| a.label()).unwrap_or("?"),
    }
}

// ------------------------------------------------------------ the sticks

/// What a stick or the triggers can drive.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum PadAxis {
    Pitch,
    Roll,
    Rudder,
    Throttle,
    LookX,
    LookY,
}

impl PadAxis {
    pub const ALL: [PadAxis; 6] = [PadAxis::Pitch, PadAxis::Roll, PadAxis::Rudder, PadAxis::Throttle, PadAxis::LookX, PadAxis::LookY];

    fn id(self) -> &'static str {
        match self {
            PadAxis::Pitch => "pitch",
            PadAxis::Roll => "roll",
            PadAxis::Rudder => "rudder",
            PadAxis::Throttle => "throttle",
            PadAxis::LookX => "look_x",
            PadAxis::LookY => "look_y",
        }
    }

    fn parse(s: &str) -> Option<PadAxis> {
        PadAxis::ALL.into_iter().find(|a| a.id() == s)
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::t;
        match self {
            PadAxis::Pitch => t!("gamepad.pitch"),
            PadAxis::Roll => t!("gamepad.roll"),
            PadAxis::Rudder => t!("gamepad.rudder"),
            PadAxis::Throttle => t!("gamepad.throttle_up_down"),
            PadAxis::LookX => t!("gamepad.look_left_right"),
            PadAxis::LookY => t!("gamepad.look_up_down"),
        }
    }

    /// What to do to bind it.
    pub fn how(self) -> &'static str {
        use crate::i18n::t;
        match self {
            PadAxis::Pitch => t!("gamepad.pull_the_stick_back_nose"),
            PadAxis::Roll => t!("gamepad.push_the_stick_right_then"),
            PadAxis::Rudder => t!("gamepad.push_the_stick_right_or"),
            PadAxis::Throttle => t!("gamepad.push_the_stick_up_more"),
            PadAxis::LookX => t!("gamepad.push_the_stick_right_then"),
            PadAxis::LookY => t!("gamepad.push_the_stick_up_then"),
        }
    }

    /// The sign "the way that means more" has, held the usual way round:
    /// a stick pulled back raises the nose.
    fn natural(self) -> f32 {
        if self == PadAxis::Pitch { -1.0 } else { 1.0 }
    }
}

/// An analogue input: one way of a stick, or the triggers together (RT − LT).
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum Stick {
    LX,
    LY,
    RX,
    RY,
    Trig,
}

impl Stick {
    pub const ALL: [Stick; 5] = [Stick::LX, Stick::LY, Stick::RX, Stick::RY, Stick::Trig];

    fn id(self) -> &'static str {
        match self {
            Stick::LX => "LX",
            Stick::LY => "LY",
            Stick::RX => "RX",
            Stick::RY => "RY",
            Stick::Trig => "LT/RT",
        }
    }

    fn parse(s: &str) -> Option<Stick> {
        Stick::ALL.into_iter().find(|a| a.id() == s)
    }

    /// As `padui` draws it.
    pub fn spec(self) -> String {
        use crate::i18n::t;
        match self {
            Stick::LX => format!("LS {}", t!("gamepad.left_right")),
            Stick::LY => format!("LS {}", t!("gamepad.up_down")),
            Stick::RX => format!("RS {}", t!("gamepad.left_right")),
            Stick::RY => format!("RS {}", t!("gamepad.up_down")),
            Stick::Trig => "LT / RT".to_string(),
        }
    }

    /// The thumbstick (or the triggers) it is on, whose dead zone it has.
    pub fn thumb(self) -> Thumb {
        match self {
            Stick::LX | Stick::LY => Thumb::Left,
            Stick::RX | Stick::RY => Thumb::Right,
            Stick::Trig => Thumb::Triggers,
        }
    }
}

/// A thumbstick, or the two triggers: each has its own dead zone, as
/// pads wear (and drift) unevenly.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum Thumb {
    Left,
    Right,
    Triggers,
}

/// The most a dead zone can take, of the travel.
pub const DEAD_MAX: f32 = 0.5;

impl Thumb {
    pub const ALL: [Thumb; 3] = [Thumb::Left, Thumb::Right, Thumb::Triggers];

    fn id(self) -> &'static str {
        match self {
            Thumb::Left => "LS",
            Thumb::Right => "RS",
            Thumb::Triggers => "LT/RT",
        }
    }

    fn parse(s: &str) -> Option<Thumb> {
        Thumb::ALL.into_iter().find(|t| t.id() == s)
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::t;
        match self {
            Thumb::Left => t!("gamepad.dead_zone_left_stick"),
            Thumb::Right => t!("gamepad.dead_zone_right_stick"),
            Thumb::Triggers => t!("gamepad.dead_zone_triggers"),
        }
    }

    /// What it starts with: the sticks the small dead zone they always had,
    /// the triggers none.
    pub fn default_dead(self) -> f32 {
        if self == Thumb::Triggers { 0.0 } else { 0.12 }
    }
}

/// A stick's -1..1 with the dead zone `d` round its centre taken off, the
/// rest of the travel stretched so there is no step out of it.
pub fn centred(v: f64, d: f32) -> f64 {
    let d = (d as f64).clamp(0.0, DEAD_MAX as f64);
    if v.abs() <= d { 0.0 } else { v.signum() * ((v.abs() - d) / (1.0 - d)).min(1.0) }
}

/// A trigger's 0..1 with the dead zone `d` at its rest end taken off, likewise.
pub fn one_ended(v: f64, d: f32) -> f64 {
    let d = (d as f64).clamp(0.0, DEAD_MAX as f64);
    if v <= d { 0.0 } else { ((v - d) / (1.0 - d)).min(1.0) }
}

/// The pad's sticks and triggers as read now, before any dead zone: for
/// Settings › Controls › Gamepad to show where a drifting pad rests.
#[derive(Copy, Clone, Default, PartialEq, Debug)]
pub struct Live {
    /// left stick x, y (-1..1, up and right +)
    pub ls: [f32; 2],
    pub rs: [f32; 2],
    /// the triggers, 0..1
    pub lt: f32,
    pub rt: f32,
}

impl Live {
    /// A thumbstick as x, y; the triggers as LT, RT.
    pub fn at(&self, t: Thumb) -> [f32; 2] {
        match t {
            Thumb::Left => self.ls,
            Thumb::Right => self.rs,
            Thumb::Triggers => [self.lt, self.rt],
        }
    }

    /// Whether each of `at`'s two gets through the dead zone `d`.
    pub fn through(&self, t: Thumb, d: f32) -> [bool; 2] {
        let f = |v: f32| if t == Thumb::Triggers { one_ended(v as f64, d) } else { centred(v as f64, d) } != 0.0;
        self.at(t).map(f)
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct AxisSrc {
    pub stick: Stick,
    /// +1 or -1: the value read is the stick's times this
    pub sign: f32,
}

impl AxisSrc {
    /// The other way round from usual.
    pub fn inverted(self, a: PadAxis) -> bool {
        self.sign != a.natural()
    }
}

fn axis_default(a: PadAxis) -> Option<AxisSrc> {
    let s = |stick, sign| Some(AxisSrc { stick, sign });
    match a {
        PadAxis::Pitch => s(Stick::LY, -1.0),
        PadAxis::Roll => s(Stick::LX, 1.0),
        PadAxis::Rudder => s(Stick::Trig, 1.0),
        PadAxis::Throttle => None,
        PadAxis::LookX => s(Stick::RX, 1.0),
        PadAxis::LookY => s(Stick::RY, 1.0),
    }
}

// ------------------------------------------------------------ the map

/// The layout as it is now: what has moved from the default.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct PadMap {
    /// action id -> its binding now (None: none)
    moved: HashMap<&'static str, Option<Chord>>,
    axes: HashMap<PadAxis, Option<AxisSrc>>,
    /// dead zones tuned away from `Thumb::default_dead`
    dead: HashMap<Thumb, f32>,
}

impl PadMap {
    pub fn chord(&self, id: &str) -> Option<Chord> {
        match self.moved.get(id) {
            Some(c) => *c,
            None => default_of(id),
        }
    }

    pub fn is_moved(&self, id: &str) -> bool {
        self.moved.contains_key(id)
    }

    /// The action on a binding.
    pub fn action_at(&self, c: Chord) -> Option<&'static str> {
        actions().iter().map(|(id, _)| *id).find(|id| self.chord(id) == Some(c))
    }

    /// Every action with a binding, in order.
    pub fn bound(&self) -> impl Iterator<Item = (&'static str, Chord)> + '_ {
        actions().iter().filter_map(|(id, _)| self.chord(id).map(|c| (*id, c)))
    }

    /// Whether Y + this button does something (so the button alone stands
    /// down while Y is held).
    pub fn has_y(&self, btn: PadBtn) -> bool {
        self.action_at(y(btn)).is_some()
    }

    /// Puts an action on a binding. Whatever was on it loses it, and so
    /// does an axis on the triggers when a trigger alone is taken.
    /// Returns the names of what lost out.
    pub fn set(&mut self, id: &str, c: Chord) -> Vec<&'static str> {
        let Some(id) = known(id) else { return Vec::new() };
        let mut lost = Vec::new();
        if c == PAUSE {
            return lost;
        }
        if let Some(other) = self.action_at(c).filter(|o| *o != id) {
            self.moved.insert(other, None);
            lost.push(label(other));
        }
        if !c.y && matches!(c.btn, PadBtn::LT | PadBtn::RT) {
            for a in PadAxis::ALL {
                if self.axis(a).is_some_and(|s| s.stick == Stick::Trig) {
                    self.axes.insert(a, None);
                    lost.push(a.label());
                }
            }
        }
        self.moved.insert(id, Some(c));
        self.tidy();
        lost
    }

    pub fn clear(&mut self, id: &str) {
        if let Some(id) = known(id) {
            self.moved.insert(id, None);
            self.tidy();
        }
    }

    /// Back to its default, which whatever has taken that meanwhile gives up.
    pub fn reset(&mut self, id: &str) -> Vec<&'static str> {
        match default_of(id) {
            Some(c) => self.set(id, c),
            None => {
                self.moved.remove(id);
                Vec::new()
            }
        }
    }

    pub fn axis(&self, a: PadAxis) -> Option<AxisSrc> {
        match self.axes.get(&a) {
            Some(s) => *s,
            None => axis_default(a),
        }
    }

    pub fn axis_moved(&self, a: PadAxis) -> bool {
        self.axes.contains_key(&a)
    }

    /// Puts an axis on a stick (or the triggers, which LT and RT alone then
    /// give up). Returns the names of what lost out.
    pub fn set_axis(&mut self, a: PadAxis, s: Option<AxisSrc>) -> Vec<&'static str> {
        let mut lost = Vec::new();
        if let Some(s) = s {
            for o in PadAxis::ALL.into_iter().filter(|o| *o != a) {
                if self.axis(o).is_some_and(|x| x.stick == s.stick) {
                    self.axes.insert(o, None);
                    lost.push(o.label());
                }
            }
            if s.stick == Stick::Trig {
                for t in [b(PadBtn::LT), b(PadBtn::RT)] {
                    if let Some(id) = self.action_at(t) {
                        self.moved.insert(id, None);
                        lost.push(label(id));
                    }
                }
            }
        }
        self.axes.insert(a, s);
        self.tidy();
        lost
    }

    pub fn reset_axis(&mut self, a: PadAxis) -> Vec<&'static str> {
        let lost = self.set_axis(a, axis_default(a));
        self.axes.remove(&a);
        lost
    }

    /// A thumbstick's (or the triggers') dead zone, as a fraction of the travel.
    pub fn dead(&self, t: Thumb) -> f32 {
        self.dead.get(&t).copied().unwrap_or(t.default_dead())
    }

    pub fn set_dead(&mut self, t: Thumb, d: f32) {
        self.dead.insert(t, d.clamp(0.0, DEAD_MAX));
        self.tidy();
    }

    pub fn is_default(&self) -> bool {
        self.moved.is_empty() && self.axes.is_empty() && self.dead.is_empty()
    }

    /// Drops what is back to its default.
    fn tidy(&mut self) {
        self.moved.retain(|id, c| *c != default_of(id));
        self.axes.retain(|a, s| *s != axis_default(*a));
        self.dead.retain(|t, d| *d != t.default_dead());
    }

    // ------------------------------------------------------------ the file

    pub fn json(&self) -> serde_json::Value {
        let mut buttons = serde_json::Map::new();
        for (id, c) in &self.moved {
            buttons.insert(id.to_string(), c.map(|c| c.text().into()).unwrap_or(serde_json::Value::Null));
        }
        let mut axes = serde_json::Map::new();
        for (a, s) in &self.axes {
            axes.insert(a.id().to_string(), s.map(|s| serde_json::json!({ "stick": s.stick.id(), "sign": s.sign })).unwrap_or(serde_json::Value::Null));
        }
        let dead: serde_json::Map<_, _> = self.dead.iter().map(|(t, d)| (t.id().to_string(), serde_json::json!(d))).collect();
        serde_json::json!({ "buttons": buttons, "axes": axes, "dead": dead })
    }

    pub fn read(&mut self, j: &serde_json::Value) {
        for (id, c) in j["buttons"].as_object().into_iter().flatten() {
            if let Some(id) = known(id) {
                let c = c.as_str().and_then(Chord::parse).filter(|c| *c != PAUSE);
                self.moved.insert(id, c);
            }
        }
        for (a, s) in j["axes"].as_object().into_iter().flatten() {
            if let Some(a) = PadAxis::parse(a) {
                let s = s["stick"].as_str().and_then(Stick::parse).map(|stick| AxisSrc { stick, sign: if s["sign"].as_f64().unwrap_or(1.0) < 0.0 { -1.0 } else { 1.0 } });
                self.axes.insert(a, s);
            }
        }
        for (t, d) in j["dead"].as_object().into_iter().flatten() {
            if let (Some(t), Some(d)) = (Thumb::parse(t), d.as_f64()) {
                self.dead.insert(t, (d as f32).clamp(0.0, DEAD_MAX));
            }
        }
        // (two actions on one binding, edited by hand: the first keeps it)
        let mut seen = Vec::new();
        for (id, _) in actions() {
            if let Some(c) = self.chord(id) {
                if seen.contains(&c) {
                    self.moved.insert(id, None);
                } else {
                    seen.push(c);
                }
            }
        }
        self.tidy();
    }
}

// ------------------------------------------------------------ binding from the pad

/// How long B is held to call off a capture.
pub const HOLD_CANCEL: f64 = 0.8;

/// A capture's progress, fed the pad's buttons and sticks.
#[derive(Default, Debug)]
pub struct Catch {
    /// Y pressed since the capture began
    y: bool,
    /// the first other button pressed: (it, with Y held, when)
    down: Option<(PadBtn, bool, f64)>,
    /// the stick pushed far enough, and which way
    stick: Option<(Stick, f32)>,
}

#[derive(Debug, PartialEq)]
pub enum Caught {
    Wait,
    Cancel,
    Button(Chord),
    Axis(AxisSrc),
    /// Menu alone: it pauses
    Refused(Chord),
}

impl Catch {
    /// A button pressed or let go. A binding is taken as its button is let
    /// go, so that Y can be held first and B held to cancel.
    pub fn button(&mut self, btn: PadBtn, down: bool, y_held: bool, now: f64) -> Caught {
        if down {
            if btn == PadBtn::Y {
                self.y = true;
            } else if self.down.is_none() {
                self.down = Some((btn, y_held, now));
            }
            return Caught::Wait;
        }
        match self.down {
            Some((d, with_y, _)) if d == btn => {
                self.down = None;
                let c = Chord { btn, y: with_y };
                if c == PAUSE { Caught::Refused(c) } else { Caught::Button(c) }
            }
            None if btn == PadBtn::Y && self.y => Caught::Button(Y_ALONE),
            _ => Caught::Wait,
        }
    }

    /// B held long enough alone: cancel.
    pub fn tick(&self, now: f64) -> Caught {
        match self.down {
            Some((PadBtn::B, false, t)) if now - t >= HOLD_CANCEL => Caught::Cancel,
            _ => Caught::Wait,
        }
    }

    /// How far through holding B to cancel (0..1), for the banner.
    pub fn cancelling(&self, now: f64) -> f32 {
        match self.down {
            Some((PadBtn::B, false, t)) => ((now - t) / HOLD_CANCEL).clamp(0.0, 1.0) as f32,
            _ => 0.0,
        }
    }

    /// The sticks and triggers now (`Stick::ALL`'s order): one pushed well
    /// over is taken once it is let back.
    pub fn sticks(&mut self, v: [f32; 5]) -> Caught {
        match self.stick {
            None => {
                let (i, m) = v.iter().enumerate().max_by(|a, b| a.1.abs().total_cmp(&b.1.abs())).map(|(i, m)| (i, *m)).unwrap_or((0, 0.0));
                if m.abs() > 0.6 {
                    self.stick = Some((Stick::ALL[i], m.signum()));
                }
                Caught::Wait
            }
            Some((s, sign)) => {
                let i = Stick::ALL.iter().position(|x| *x == s).unwrap_or(0);
                if v[i].abs() < 0.25 { Caught::Axis(AxisSrc { stick: s, sign }) } else { Caught::Wait }
            }
        }
    }
}

// ------------------------------------------------------------ for the menus

thread_local! {
    static NOW: std::cell::RefCell<PadMap> = std::cell::RefCell::new(PadMap::default());
}

/// The layout as the menus and hints should show it (set whenever it changes).
pub fn publish(m: &PadMap) {
    NOW.with(|n| {
        if *n.borrow() != *m {
            *n.borrow_mut() = m.clone();
        }
    });
}

/// What an action is on now, as `padui` draws it ("" for nothing).
pub fn spec_of(id: &str) -> String {
    NOW.with(|n| n.borrow().chord(id).map(Chord::spec).unwrap_or_default())
}

/// Actions shown on one row of the controls list when they sit together
/// (all with Y, or all without): their buttons joined, and what they do.
const GROUPS: &[(&[&str], &str)] = &[
    (&["throttle_up", "throttle_down"], "throttle up / down (hold)"),
    (&["trim_down", "trim_up"], "trim nose down / up"),
    (&["flaps_up", "flaps_down"], "flaps up / down a notch"),
    (&["view_ahead", "view_down", "view_left", "view_right"], "quick views"),
    (&["master_arm", "target_lock"], "master arm, lock a target"),
];

/// The flying rows of the gamepad controls list, as the layout is now:
/// (buttons, what they do), translated, in lower case like the rest of the list.
pub fn flying_rows() -> Vec<(String, String)> {
    let low = |s: &str| {
        let mut c = s.chars();
        c.next().map(|f| f.to_lowercase().chain(c).collect()).unwrap_or_default()
    };
    NOW.with(|n| rows(&n.borrow())).into_iter().map(|(k, w)| (k, low(crate::i18n::tr_en(w)))).collect()
}

fn rows(m: &PadMap) -> Vec<(String, &'static str)> {
    let mut rows = Vec::new();
    // The sticks: one row for a stick that does both of a pair.
    let mut done = Vec::new();
    for (pair, stick, what) in [([PadAxis::Pitch, PadAxis::Roll], "LS", "pitch and roll"), ([PadAxis::LookX, PadAxis::LookY], "RS", "look around")] {
        let s = pair.map(|a| m.axis(a).map(|s| s.stick));
        let one = if stick == "LS" { [Some(Stick::LY), Some(Stick::LX)] } else { [Some(Stick::RX), Some(Stick::RY)] };
        if s == one {
            rows.push((stick.to_string(), what));
            done.extend(pair);
        }
    }
    for a in PadAxis::ALL.into_iter().filter(|a| !done.contains(a)) {
        if let Some(s) = m.axis(a) {
            rows.push((s.stick.spec(), a.label()));
        }
    }
    let mut done: Vec<&str> = Vec::new();
    for (id, c) in m.bound() {
        if done.contains(&id) {
            continue;
        }
        if let Some((ids, what)) = GROUPS.iter().find(|g| g.0.contains(&id)) {
            let cs: Vec<Option<Chord>> = ids.iter().map(|i| m.chord(i)).collect();
            if cs.iter().all(|x| x.is_some_and(|x| x.y == c.y && x != Y_ALONE)) {
                let names: Vec<&str> = cs.iter().flatten().map(|x| x.btn.name()).collect();
                let joined = if names == ["Up", "Down", "Left", "Right"] { "Dpad".to_string() } else { names.join(" / ") };
                rows.push((if c.y { format!("Y + {joined}") } else { joined }, *what));
                done.extend(ids.iter());
                continue;
            }
        }
        let spec = if c == Y_ALONE { crate::i18n::t!("gamepad.y_let_go").to_string() } else { c.spec() };
        rows.push((spec, label(id)));
    }
    rows.push(("A".into(), "fly again after a crash"));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use PadBtn::*;

    #[test]
    fn defaults_are_the_msfs_layout() {
        let m = PadMap::default();
        assert_eq!(m.chord("throttle_up"), Some(b(A)));
        assert_eq!(m.chord("parking_brake"), Some(y(B)));
        assert_eq!(m.action_at(Y_ALONE), None);
        assert_eq!(m.action_at(y(RT)), Some("fire_missile"));
        assert_eq!(m.action_at(y(RB)), Some("ap"));
        assert_eq!(m.chord("minimap"), None);
        assert!(m.has_y(B) && !m.has_y(LT));
        assert_eq!(m.axis(PadAxis::Pitch), Some(AxisSrc { stick: Stick::LY, sign: -1.0 }));
        assert!(!m.axis(PadAxis::Pitch).unwrap().inverted(PadAxis::Pitch));
        // No two actions start on one binding, and Menu alone is left to pause.
        let mut seen = Vec::new();
        for (id, c) in m.bound() {
            assert!(!seen.contains(&c), "{id} shares {c:?}");
            assert_ne!(c, PAUSE);
            seen.push(c);
        }
        // Every action does something.
        for (id, _) in actions() {
            assert!(does(id, false).is_some() && does(id, true).is_some(), "{id}");
        }
        assert_eq!(does("ap", true), Some(Does::Key(KeyCode::Digit2)));
        assert_eq!(does("ap", false), Some(Does::Key(KeyCode::F5)));
        // The controls list keeps its pairs together.
        let r = rows(&m);
        assert_eq!(r[0], ("LS".to_string(), "pitch and roll"));
        assert!(r.contains(&("A / B".to_string(), "throttle up / down (hold)")));
        assert!(r.contains(&("Y + Up / Down".to_string(), "trim nose down / up")));
        assert!(r.contains(&("Dpad".to_string(), "quick views")));
        assert_eq!(does("tail_view", false), Some(Does::Key(KeyCode::F6)));
    }

    #[test]
    fn the_foot_layout_has_its_own_buttons() {
        // Every foot action is on a different button, and none takes the pause.
        let mut seen = Vec::new();
        for &(id, c) in FOOT_LAYOUT {
            assert!(!seen.contains(&c), "{id} shares {c:?}");
            assert_ne!(c, PAUSE);
            seen.push(c);
            assert!(crate::bind::FOOT_ACTIONS.iter().any(|a| a.id == id), "{id} is a foot action");
            assert_ne!(label(id), "?", "{id} has a label");
        }
        // Flying and walking do not take the same button for their own.
        for &(id, c) in FOOT_LAYOUT {
            let flying = crate::bind::ACTIONS.iter().any(|a| a.id == id);
            assert!(!flying, "the flight layout leaves {id} to the foot layout");
            assert_ne!(PadMap::default().chord(id), Some(c), "{id} differs from the pad's flight layout");
        }
        let rows = foot_rows();
        assert_eq!(rows[0], ("LS".to_string(), "move"));
        assert!(rows.iter().any(|(k, what)| k == "A" && *what == label("jump")));
    }

    #[test]
    fn the_car_layout_has_its_own_buttons() {
        let mut seen = Vec::new();
        for &(id, c) in CAR_LAYOUT {
            assert!(!seen.contains(&c), "{id} shares {c:?}");
            assert_ne!(c, PAUSE);
            seen.push(c);
            assert!(crate::bind::CAR_ACTIONS.iter().any(|a| a.id == id) || crate::bind::is_global(id), "{id} is a car or global action");
            assert_ne!(label(id), "?", "{id} has a label");
        }
        let rows = car_rows();
        assert_eq!(rows[0], ("LS".to_string(), crate::i18n::tr("gamepad.steer")));
        assert!(rows.iter().any(|(k, what)| k == "A" && *what == label("car_throttle")));
        assert!(rows.iter().any(|(k, what)| k == "X" && *what == label("car_brake")));
        assert!(rows.iter().any(|(k, what)| k == "RB" && *what == label("car_handbrake")));
        assert!(rows.iter().any(|(k, what)| k == "Y" && *what == label("car_enter_exit")));
    }

    #[test]
    fn moving_bindings() {        let mut m = PadMap::default();
        // Gear onto Y + LB takes it from the speedbrake.
        let lost = m.set("gear", y(LB));
        assert_eq!(lost.len(), 1);
        assert_eq!((m.chord("gear"), m.chord("speedbrake")), (Some(y(LB)), None));
        assert_eq!(m.action_at(b(L3)), None);
        // Menu alone stays the pause.
        m.set("gear", PAUSE);
        assert_eq!(m.chord("gear"), Some(y(LB)));
        // A trigger alone frees the rudder off the triggers.
        m.set("minimap", b(LT));
        assert_eq!(m.axis(PadAxis::Rudder), None);
        // The rudder onto the right stick sideways takes it from looking.
        m.set_axis(PadAxis::Rudder, Some(AxisSrc { stick: Stick::RX, sign: 1.0 }));
        assert_eq!(m.axis(PadAxis::LookX), None);
        // It all goes in the file and comes back.
        let mut n = PadMap::default();
        n.read(&m.json());
        assert_eq!(n, m);
        // And back to the default, one by one.
        for (id, _) in actions() {
            m.reset(id);
        }
        for a in PadAxis::ALL {
            m.reset_axis(a);
        }
        assert!(m.is_default(), "{m:?}");
    }

    #[test]
    fn dead_zones() {
        let mut m = PadMap::default();
        // The sticks start with the small dead zone they always had, the triggers with none.
        assert_eq!(m.dead(Thumb::Left), 0.12);
        assert_eq!(m.dead(Thumb::Right), 0.12);
        assert_eq!(m.dead(Thumb::Triggers), 0.0);
        assert_eq!((Stick::LX.thumb(), Stick::RY.thumb(), Stick::Trig.thumb()), (Thumb::Left, Thumb::Right, Thumb::Triggers));
        // Inside it nothing; out of it the rest of the travel, with no step.
        assert_eq!(centred(0.1, 0.12), 0.0);
        // (a pad reads in f32, as the dead zone is kept)
        assert_eq!(centred(-0.12f32 as f64, 0.12), 0.0);
        assert!(centred(0.1201, 0.12) < 1e-3);
        assert_eq!(centred(1.0, 0.12), 1.0);
        assert_eq!(centred(-1.0, 0.12), -1.0);
        assert!((centred(0.56, 0.12) - 0.5).abs() < 1e-6);
        assert_eq!(one_ended(0.04, 0.05), 0.0);
        assert_eq!(one_ended(1.0, 0.05), 1.0);
        assert_eq!(one_ended(0.5, 0.0), 0.5);
        // Tuned, it goes in the file and comes back; held to a sane range.
        m.set_dead(Thumb::Left, 0.2);
        m.set_dead(Thumb::Triggers, 0.9);
        assert_eq!(m.dead(Thumb::Triggers), 0.5);
        assert!(!m.is_default());
        let mut n = PadMap::default();
        n.read(&m.json());
        assert_eq!(n, m);
        // Out of range by hand in the file: held in range too.
        let mut n = PadMap::default();
        n.read(&serde_json::json!({ "dead": { "RS": -1.0 } }));
        assert_eq!(n.dead(Thumb::Right), 0.0);
        // Back to its default, it is not kept.
        m.set_dead(Thumb::Left, 0.12);
        m.set_dead(Thumb::Triggers, 0.0);
        assert!(m.is_default(), "{m:?}");
    }

    #[test]
    fn live_readout() {
        let l = Live { ls: [0.03, -0.08], rs: [0.5, 0.0], lt: 0.02, rt: 0.9 };
        // A thumbstick as x, y; the triggers as LT, RT.
        assert_eq!(l.at(Thumb::Left), [0.03, -0.08]);
        assert_eq!(l.at(Thumb::Right), [0.5, 0.0]);
        assert_eq!(l.at(Thumb::Triggers), [0.02, 0.9]);
        // Whether each way gets through a dead zone, as the flying reads it.
        assert_eq!(l.through(Thumb::Left, 0.05), [false, true]);
        assert_eq!(l.through(Thumb::Left, 0.12), [false, false]);
        assert_eq!(l.through(Thumb::Right, 0.12), [true, false]);
        assert_eq!(l.through(Thumb::Triggers, 0.05), [false, true]);
    }

    #[test]
    fn catching_from_the_pad() {
        // A plain button, taken as it is let go.
        let mut c = Catch::default();
        assert_eq!(c.button(LB, true, false, 0.0), Caught::Wait);
        assert_eq!(c.button(LB, false, false, 0.1), Caught::Button(b(LB)));
        // Y held, then a button.
        let mut c = Catch::default();
        c.button(Y, true, true, 0.0);
        c.button(R3, true, true, 0.2);
        assert_eq!(c.button(Y, false, false, 0.3), Caught::Wait);
        assert_eq!(c.button(R3, false, false, 0.35), Caught::Button(y(R3)));
        // Y on its own.
        let mut c = Catch::default();
        c.button(Y, true, true, 0.0);
        assert_eq!(c.button(Y, false, false, 0.2), Caught::Button(Y_ALONE));
        // The A that started the capture, let go, is not taken.
        let mut c = Catch::default();
        assert_eq!(c.button(A, false, false, 0.0), Caught::Wait);
        // B held cancels; B tapped binds.
        let mut c = Catch::default();
        c.button(B, true, false, 1.0);
        assert_eq!(c.tick(1.5), Caught::Wait);
        assert_eq!(c.tick(1.9), Caught::Cancel);
        let mut c = Catch::default();
        c.button(B, true, false, 1.0);
        assert_eq!(c.button(B, false, false, 1.2), Caught::Button(b(B)));
        // Menu alone is refused.
        let mut c = Catch::default();
        c.button(Menu, true, false, 0.0);
        assert_eq!(c.button(Menu, false, false, 0.1), Caught::Refused(PAUSE));
        // A stick: the biggest push, taken once let back.
        let mut c = Catch::default();
        assert_eq!(c.sticks([0.1, 0.0, 0.3, 0.0, 0.0]), Caught::Wait);
        assert_eq!(c.sticks([0.1, -0.9, 0.3, 0.0, 0.0]), Caught::Wait);
        assert_eq!(c.sticks([0.0, -0.5, 0.0, 0.0, 0.0]), Caught::Wait);
        assert_eq!(c.sticks([0.0, -0.1, 0.0, 0.0, 0.0]), Caught::Axis(AxisSrc { stick: Stick::LY, sign: -1.0 }));
    }
}
