//! Control bindings: joysticks, rudder pedals, throttle quadrants and wheels
//! (any device gilrs can open, laid out or not), and the keyboard's keys
//! moved to suit. Kept in `<config dir>/gtw/bindings.json`.
//!
//! - An axis is bound by moving it the way that means "more": nose up,
//!   right roll, right rudder, full throttle, brake pressed. Its sign is
//!   learnt from that, so there is nothing to invert by hand.
//! - A joystick button does what a key does (`Action::key`): the game's own
//!   key handling then takes it, held or pressed.
//! - A key moved elsewhere is translated back to the key it stands for as
//!   it comes in (`map_key`), so the rest of the game is none the wiser.

use std::collections::HashMap;
use winit::keyboard::KeyCode;

/// The flight axes a device axis can drive.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum AxisAct {
    Pitch,
    Roll,
    Yaw,
    Throttle,
    BrakeLeft,
    BrakeRight,
}

impl AxisAct {
    pub const ALL: [AxisAct; 6] = [AxisAct::Pitch, AxisAct::Roll, AxisAct::Yaw, AxisAct::Throttle, AxisAct::BrakeLeft, AxisAct::BrakeRight];

    pub fn id(self) -> &'static str {
        match self {
            AxisAct::Pitch => "pitch",
            AxisAct::Roll => "roll",
            AxisAct::Yaw => "rudder",
            AxisAct::Throttle => "throttle",
            AxisAct::BrakeLeft => "brake_left",
            AxisAct::BrakeRight => "brake_right",
        }
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::t;
        match self {
            AxisAct::Pitch => t!("bind.pitch"),
            AxisAct::Roll => t!("bind.roll"),
            AxisAct::Yaw => t!("bind.rudder"),
            AxisAct::Throttle => t!("bind.throttle"),
            AxisAct::BrakeLeft => t!("bind.left_toe_brake"),
            AxisAct::BrakeRight => t!("bind.right_toe_brake"),
        }
    }

    /// What to do to bind it.
    pub fn how(self) -> &'static str {
        use crate::i18n::t;
        match self {
            AxisAct::Pitch => t!("bind.pull_the_stick_back_nose"),
            AxisAct::Roll => t!("bind.push_the_stick_all_the"),
            AxisAct::Yaw => t!("bind.push_the_right_rudder_pedal"),
            AxisAct::Throttle => t!("bind.push_the_throttle_all_the"),
            AxisAct::BrakeLeft => t!("bind.press_the_left_toe_brake"),
            AxisAct::BrakeRight => t!("bind.press_the_right_toe_brake"),
        }
    }

    /// Centred axes (stick, pedals) as against one-ended ones (throttle, brakes).
    pub fn centred(self) -> bool {
        matches!(self, AxisAct::Pitch | AxisAct::Roll | AxisAct::Yaw)
    }

    /// The dead zone a new binding starts with.
    pub fn default_dead(self) -> f32 {
        if self.centred() { 0.05 } else { 0.02 }
    }

    fn parse(s: &str) -> Option<AxisAct> {
        AxisAct::ALL.into_iter().find(|a| a.id() == s)
    }
}

/// Something a key or a joystick button can do. Each stands for the key
/// that does it by default (`key`); `keyboard` says whether the keyboard
/// may move it (Esc and Tab stay put: the menus need them).
#[derive(Copy, Clone, Debug)]
pub struct Action {
    pub id: &'static str,
    pub key: KeyCode,
    pub keyboard: bool,
}

impl Action {
    /// What it does, from the `action.<id>` key of the language files.
    pub fn label(&self) -> &'static str {
        crate::i18n::tr(&format!("action.{}", self.id))
    }
}

/// Which controller the player is using. A key can mean different things in
/// each: W pitches down in flight, walks forward on foot and opens the
/// throttle in a car.
#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash)]
pub enum Context {
    Foot,
    Car,
    Flight,
}

impl Context {
    pub const ALL: [Context; 3] = [Context::Foot, Context::Car, Context::Flight];

    pub fn name(self) -> &'static str {
        match self {
            Context::Foot => "foot",
            Context::Car => "car",
            Context::Flight => "flight",
        }
    }

    pub fn label(self) -> &'static str {
        use crate::i18n::t;
        match self {
            Context::Foot => t!("settings.controls_foot"),
            Context::Car => t!("settings.controls_car"),
            Context::Flight => t!("settings.controls_flight"),
        }
    }

    #[allow(dead_code)]
    pub fn parse(s: &str) -> Option<Context> {
        Context::ALL.into_iter().find(|c| c.name() == s.trim().to_lowercase())
    }
}

const fn act(id: &'static str, key: KeyCode) -> Action {
    Action { id, key, keyboard: true }
}

pub const ACTIONS: &[Action] = &[
    act("pitch_down", KeyCode::KeyW),
    act("pitch_up", KeyCode::KeyS),
    act("roll_left", KeyCode::KeyA),
    act("roll_right", KeyCode::KeyD),
    act("rudder_left", KeyCode::KeyQ),
    act("rudder_right", KeyCode::KeyE),
    act("throttle_up", KeyCode::ShiftLeft),
    act("throttle_down", KeyCode::ControlLeft),
    act("trim_down", KeyCode::Numpad7),
    act("trim_up", KeyCode::Numpad1),
    act("brakes", KeyCode::KeyB),
    act("parking_brake", KeyCode::Period),
    act("flaps_down", KeyCode::KeyF),
    act("flaps_up", KeyCode::KeyV),
    act("gear", KeyCode::KeyL),
    act("speedbrake", KeyCode::Slash),
    act("canopy", KeyCode::Digit6),
    act("bailout", KeyCode::F7),
    act("reversers", KeyCode::KeyZ),
    act("boost", KeyCode::Space),
    act("engines", KeyCode::Digit1),
    act("stage", KeyCode::Backspace),
    act("autoland", KeyCode::Digit2),
    act("master_arm", KeyCode::Digit3),
    act("target_lock", KeyCode::Digit4),
    act("fire_missile", KeyCode::Digit5),
    act("engine_auto", KeyCode::F4),
    act("master", KeyCode::Insert),
    act("ignition", KeyCode::KeyI),
    act("starter", KeyCode::KeyO),
    act("mixture_rich", KeyCode::PageUp),
    act("mixture_lean", KeyCode::PageDown),
    act("obs_left", KeyCode::BracketLeft),
    act("obs_right", KeyCode::BracketRight),
    act("baro_qnh", KeyCode::KeyX),
    act("ap", KeyCode::F5),
    act("ap_hdg", KeyCode::Digit7),
    act("ap_nav", KeyCode::Digit8),
    act("ap_apr", KeyCode::Digit9),
    act("ap_alt", KeyCode::Digit0),
    act("ap_hdg_left", KeyCode::Minus),
    act("ap_hdg_right", KeyCode::Equal),
    act("ap_alt_down", KeyCode::Semicolon),
    act("ap_alt_up", KeyCode::Quote),
    act("heading_sync", KeyCode::KeyK),
    act("camera", KeyCode::KeyC),
    act("tail_view", KeyCode::F6),
    act("hud", KeyCode::KeyH),
    act("minimap", KeyCode::Backquote),
    act("full_map", KeyCode::KeyJ),
    act("goto", KeyCode::KeyG),
    act("respawn", KeyCode::KeyR),
    act("pause", KeyCode::KeyP),
    act("sound", KeyCode::KeyU),
    act("time_forward", KeyCode::KeyT),
    act("time_back", KeyCode::KeyY),
    act("clouds_more", KeyCode::KeyN),
    act("clouds_less", KeyCode::KeyM),
    act("taa", KeyCode::F2),
    act("screenshot", KeyCode::F12),
    Action { id: "pause_menu", key: KeyCode::Escape, keyboard: false },
    Action { id: "flight_panel", key: KeyCode::Tab, keyboard: false },
];

/// On foot: move, look, sprint, jump, interact and the first-person toggle.
/// Camera, HUD, minimap, pause and the rest are global and work here too.
pub const FOOT_ACTIONS: &[Action] = &[
    act("move_forward", KeyCode::KeyW),
    act("move_back", KeyCode::KeyS),
    act("move_left", KeyCode::KeyA),
    act("move_right", KeyCode::KeyD),
    act("look_up", KeyCode::ArrowUp),
    act("look_down", KeyCode::ArrowDown),
    act("look_left", KeyCode::ArrowLeft),
    act("look_right", KeyCode::ArrowRight),
    act("sprint", KeyCode::ShiftLeft),
    act("jump", KeyCode::Space),
    act("interact", KeyCode::KeyE),
    act("first_person", KeyCode::KeyV),
];

/// In a car: throttle, brake, steering, the handbrake and getting out. Camera,
/// HUD, minimap, pause and the rest are global and work here too.
pub const CAR_ACTIONS: &[Action] = &[
    act("car_throttle", KeyCode::KeyW),
    act("car_brake", KeyCode::KeyS),
    act("car_steer_left", KeyCode::KeyA),
    act("car_steer_right", KeyCode::KeyD),
    act("car_handbrake", KeyCode::ShiftLeft),
    act("car_enter_exit", KeyCode::KeyE),
    act("car_horn", KeyCode::KeyB),
    act("car_lights", KeyCode::KeyL),
];

/// Actions available in every context (camera, HUD, pause, screenshots…).
pub const GLOBAL: &[&str] = &[
    "camera", "hud", "minimap", "full_map", "pause", "pause_menu", "screenshot", "sound", "respawn", "goto", "time_forward", "time_back", "clouds_more", "clouds_less", "taa",
];

/// The actions of one context (before the global ones).
pub fn context_actions(ctx: Context) -> &'static [Action] {
    match ctx {
        Context::Foot => FOOT_ACTIONS,
        Context::Car => CAR_ACTIONS,
        Context::Flight => ACTIONS,
    }
}

/// Every flight, foot and car action, once each.
pub fn every() -> impl Iterator<Item = &'static Action> {
    ACTIONS.iter().chain(FOOT_ACTIONS).chain(CAR_ACTIONS)
}

pub fn is_global(id: &str) -> bool {
    GLOBAL.contains(&id)
}

/// Every action that can be used in `ctx`: the context's own, then the global
/// ones from the flight table.
pub fn actions_in(ctx: Context) -> impl Iterator<Item = &'static Action> {
    let own = context_actions(ctx).iter().filter(move |a| ctx == Context::Flight || !is_global(a.id));
    let globals = ACTIONS.iter().filter(move |a| ctx != Context::Flight && is_global(a.id));
    own.chain(globals)
}

/// The action with this id in `ctx`, if it is available there.
pub fn action_in(ctx: Context, id: &str) -> Option<&'static Action> {
    actions_in(ctx).find(|a| a.id == id)
}

/// The flight action with this id (the cockpit code's table).
pub fn action(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// The flight action with this id, or any table's, for loading saved keys.
fn any_action(id: &str) -> Option<&'static Action> {
    every().find(|a| a.id == id)
}

/// A device: gilrs's UUID (stable across runs and ports) and its name.
pub type Uuid = [u8; 16];

#[derive(Clone, Debug, PartialEq)]
pub struct AxisBind {
    pub dev: Uuid,
    pub name: String,
    /// the device's own axis code (evdev's on Linux)
    pub code: u32,
    /// +1 or -1: the way that means "more"
    pub sign: f32,
    /// dead zone: round the centre of a stick or pedals, at the idle end
    /// of a throttle or brake (fraction of the travel)
    pub dead: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ButtonBind {
    pub dev: Uuid,
    pub name: String,
    pub code: u32,
}

/// What is waiting for an input to be bound to it.
#[derive(Clone, Debug, PartialEq)]
pub enum Capture {
    Axis(AxisAct),
    Button(&'static str),
    Key(&'static str),
    /// the gamepad's flying layout (padmap.rs): a button or Y + a button
    Pad(&'static str),
    PadAxis(crate::padmap::PadAxis),
}

impl Capture {
    /// Taken from the gamepad (as against a joystick or the keyboard).
    pub fn from_pad(&self) -> bool {
        matches!(self, Capture::Pad(_) | Capture::PadAxis(_))
    }
}

#[derive(Default)]
pub struct Bindings {
    pub axes: HashMap<AxisAct, AxisBind>,
    /// action id -> button
    pub buttons: HashMap<&'static str, ButtonBind>,
    /// action id -> the key it has moved to (None: no key)
    pub keys: HashMap<&'static str, Option<KeyCode>>,
    /// incoming key -> the key it stands for (None: does nothing now), per context
    remap: [HashMap<KeyCode, Option<KeyCode>>; 3],
    /// the devices' axes and buttons as they are now
    axis_now: HashMap<(Uuid, u32), f32>,
    button_now: HashMap<(Uuid, u32), bool>,
    /// joystick buttons pressed and let go since last asked: (action id, down)
    edges: Vec<(&'static str, bool)>,
    pub capture: Option<Capture>,
    /// the axes when the capture began, to see which one moves
    baseline: HashMap<(Uuid, u32), f32>,
    /// devices a preset has been offered to on its own (once each)
    auto_done: std::collections::HashSet<Uuid>,
    /// the gamepad's flying layout
    pub pad: crate::padmap::PadMap,
    /// a capture from the gamepad, under way
    pub pad_catch: crate::padmap::Catch,
}

impl Bindings {
    // ------------------------------------------------------------ the keyboard

    /// The key an action is on now, in `ctx`.
    pub fn key_of(&self, ctx: Context, a: &Action) -> Option<KeyCode> {
        if !is_global(a.id) && !context_actions(ctx).iter().any(|x| x.id == a.id) {
            return None;
        }
        self.keys.get(a.id).copied().unwrap_or(Some(a.key))
    }

    /// The action a physical key does in `ctx`, if any.
    #[allow(dead_code)]
    pub fn action_at(&self, ctx: Context, k: KeyCode) -> Option<&'static Action> {
        actions_in(ctx).find(|a| self.key_of(ctx, a) == Some(k))
    }

    /// An incoming key as the game should see it in `ctx`: the key it stands
    /// for, itself when it has not been touched, None when it does nothing.
    pub fn map_key(&self, ctx: Context, k: KeyCode) -> Option<KeyCode> {
        if let Some(mapped) = self.remap[ctx as usize].get(&k) {
            return *mapped;
        }
        // The extra key beside left Shift on ISO keyboards is within reach of ZQSD.
        if ctx == Context::Flight && k == KeyCode::IntlBackslash && !self.keys.contains_key("tail_view") {
            return Some(KeyCode::F6);
        }
        Some(k)
    }

    /// Moves an action to a key in `ctx`; any other action on that key loses it.
    pub fn set_key(&mut self, ctx: Context, id: &'static str, k: KeyCode) {
        for a in actions_in(ctx).filter(|a| a.keyboard && a.id != id) {
            if self.keys.get(a.id).copied().flatten().unwrap_or(a.key) == k {
                self.keys.insert(a.id, None);
            }
        }
        self.keys.insert(id, Some(k));
        for c in Context::ALL {
            self.tidy(c);
        }
    }

    pub fn reset_key(&mut self, _ctx: Context, id: &str) {
        self.keys.remove(id);
        for c in Context::ALL {
            self.tidy(c);
        }
    }

    /// Drops what is back to its default, and works the translation out again.
    fn tidy(&mut self, ctx: Context) {
        self.keys.retain(|id, k| any_action(id).is_some_and(|a| *k != Some(a.key)));
        let map = &mut self.remap[ctx as usize];
        map.clear();
        let own: Vec<&'static Action> = actions_in(ctx).filter(|a| a.keyboard).collect();
        for a in &own {
            // Its own key no longer does it…
            if self.keys.contains_key(a.id) {
                map.entry(a.key).or_insert(None);
            }
        }
        for a in &own {
            if let Some(Some(k)) = self.keys.get(a.id) {
                // …the new one does.
                map.insert(*k, Some(a.key));
            }
        }
    }

    // ------------------------------------------------------------ the devices

    pub fn uses(&self, dev: &Uuid) -> bool {
        self.axes.values().any(|b| b.dev == *dev) || self.buttons.values().any(|b| b.dev == *dev)
    }

    /// A device axis moved (-1..1).
    pub fn axis_event(&mut self, dev: Uuid, name: &str, code: u32, v: f32) {
        self.axis_now.insert((dev, code), v);
        if let Some(Capture::Axis(a)) = self.capture.clone() {
            let base = *self.baseline.entry((dev, code)).or_insert(v);
            let d = v - base;
            if d.abs() > 0.6 {
                self.axes.retain(|_, b| !(b.dev == dev && b.code == code));
                self.axes.insert(a, AxisBind { dev, name: name.to_string(), code, sign: d.signum(), dead: a.default_dead() });
                self.capture = None;
            }
        }
    }

    /// A device button pressed or let go.
    pub fn button_event(&mut self, dev: Uuid, name: &str, code: u32, down: bool) {
        self.button_now.insert((dev, code), down);
        if down {
            if let Some(Capture::Button(id)) = self.capture.clone() {
                self.buttons.retain(|_, b| !(b.dev == dev && b.code == code));
                self.buttons.insert(id, ButtonBind { dev, name: name.to_string(), code });
                self.capture = None;
                return;
            }
        }
        for (id, b) in &self.buttons {
            if b.dev == dev && b.code == code {
                self.edges.push((id, down));
            }
        }
    }

    pub fn start_capture(&mut self, c: Capture) {
        self.baseline = self.axis_now.clone();
        self.pad_catch = Default::default();
        self.capture = Some(c);
    }

    pub fn take_edges(&mut self) -> Vec<(&'static str, bool)> {
        std::mem::take(&mut self.edges)
    }

    /// An axis's value now, as the action reads it: -1..1 for the stick and
    /// pedals (dead zone taken off), 0..1 for the throttle and brakes.
    pub fn axis(&self, a: AxisAct) -> Option<f64> {
        let b = self.axes.get(&a)?;
        let v = (*self.axis_now.get(&(b.dev, b.code))? * b.sign) as f64;
        // The dead zone, the rest of the travel stretched so there is no step out of it.
        let d = (b.dead as f64).clamp(0.0, 0.5);
        Some(if a.centred() {
            if v.abs() < d { 0.0 } else { v.signum() * (v.abs() - d) / (1.0 - d) }
        } else {
            let f = ((v + 1.0) * 0.5).clamp(0.0, 1.0);
            if f < d { 0.0 } else { (f - d) / (1.0 - d) }
        })
    }

    /// What a binding reads as, for the bindings page.
    pub fn describe_axis(&self, a: AxisAct) -> Option<String> {
        self.axes.get(&a).map(|b| crate::i18n::t!("bind.axis", short(&b.name), axis_name(b.code)))
    }

    pub fn describe_button(&self, id: &str) -> Option<String> {
        self.buttons.get(id).map(|b| crate::i18n::t!("bind.button", short(&b.name), button_name(b.code)))
    }

    // ------------------------------------------------------------ the file

    fn path() -> Option<std::path::PathBuf> {
        crate::paths::config().map(|d| d.join("bindings.json"))
    }

    pub fn load() -> Self {
        let mut b = Self::default();
        if let Some(j) = Self::path().and_then(|p| std::fs::read(p).ok()).and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok()) {
            b.read(&j);
        }
        b
    }

    fn read(&mut self, j: &serde_json::Value) {
        if let Some(m) = j["keys"].as_object() {
            for (id, k) in m {
                if let Some(a) = any_action(id).filter(|a| a.keyboard) {
                    self.keys.insert(a.id, k.as_str().and_then(key_from_name));
                }
            }
        }
        let dev = |v: &serde_json::Value| -> Option<(Uuid, String, u32)> {
            Some((uuid_from_hex(v["device"].as_str()?)?, v["name"].as_str().unwrap_or("").to_string(), v["code"].as_u64()? as u32))
        };
        for v in j["axes"].as_array().into_iter().flatten() {
            if let (Some(a), Some((dev, name, code))) = (v["action"].as_str().and_then(AxisAct::parse), dev(v)) {
                let sign = if v["sign"].as_f64().unwrap_or(1.0) < 0.0 { -1.0 } else { 1.0 };
                let dead = v["dead"].as_f64().map(|d| d.clamp(0.0, 0.5) as f32).unwrap_or(a.default_dead());
                self.axes.insert(a, AxisBind { dev, name, code, sign, dead });
            }
        }
        for v in j["buttons"].as_array().into_iter().flatten() {
            if let (Some(a), Some((dev, name, code))) = (v["action"].as_str().and_then(any_action), dev(v)) {
                self.buttons.insert(a.id, ButtonBind { dev, name, code });
            }
        }
        self.pad.read(&j["gamepad"]);
        for v in j["presets_applied"].as_array().into_iter().flatten() {
            if let Some(u) = v.as_str().and_then(uuid_from_hex) {
                self.auto_done.insert(u);
            }
        }
        // Old files were all flight actions; the new foot and car tables add
        // defaults on the same keys, which is fine across contexts. Within a
        // context one key must mean one action: an explicit choice wins over
        // a default, and the later of two explicit ones is left unbound.
        for ctx in Context::ALL {
            self.resolve_duplicates(ctx);
            self.tidy(ctx);
        }
    }

    /// Leaves one action on each key of `ctx`. An explicit key wins over a
    /// default, and of two on the same key the earlier in the table keeps it.
    fn resolve_duplicates(&mut self, ctx: Context) {
        let acts: Vec<&'static Action> = actions_in(ctx).filter(|a| a.keyboard).collect();
        let mut taken = std::collections::HashSet::new();
        for a in &acts {
            if let Some(Some(k)) = self.keys.get(a.id).copied() {
                if !taken.insert(k) {
                    self.keys.insert(a.id, None);
                }
            }
        }
        for a in &acts {
            if self.keys.contains_key(a.id) {
                continue;
            }
            if !taken.insert(a.key) {
                self.keys.insert(a.id, None);
            }
        }
    }

    fn json(&self) -> serde_json::Value {
        let hex = |u: &Uuid| u.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let mut keys = serde_json::Map::new();
        for (id, k) in &self.keys {
            keys.insert(id.to_string(), k.map(|k| serde_json::Value::String(format!("{k:?}"))).unwrap_or(serde_json::Value::Null));
        }
        let mut axes: Vec<_> = self.axes.iter().map(|(a, b)| serde_json::json!({ "action": a.id(), "device": hex(&b.dev), "name": b.name, "code": b.code, "sign": b.sign, "dead": b.dead })).collect();
        let mut buttons: Vec<_> = self.buttons.iter().map(|(id, b)| serde_json::json!({ "action": id, "device": hex(&b.dev), "name": b.name, "code": b.code })).collect();
        axes.sort_by_key(|v| v["action"].to_string());
        buttons.sort_by_key(|v| v["action"].to_string());
        let mut done: Vec<String> = self.auto_done.iter().map(hex).collect();
        done.sort();
        serde_json::json!({ "keys": keys, "axes": axes, "buttons": buttons, "presets_applied": done, "gamepad": self.pad.json() })
    }

    pub fn save(&self) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        let data = serde_json::to_vec_pretty(&self.json()).unwrap_or_default();
        if let Err(e) = std::fs::write(&tmp, data).and_then(|_| std::fs::rename(&tmp, &p)) {
            log::warn!("saving bindings to {}: {e}", p.display());
        }
    }

    /// A fingerprint of what is bound, to see when it needs saving.
    pub fn stamp(&self) -> String {
        self.json().to_string()
    }
}

// ------------------------------------------------------------ presets

/// The device codes as gilrs gives them: the evdev event type in the high
/// half (3 for an axis, 1 for a key), its code in the low half.
const ABS: u32 = 3 << 16;
const BTN: u32 = 1 << 16;
const ABS_X: u32 = ABS;
const ABS_Y: u32 = ABS + 1;
const ABS_Z: u32 = ABS + 2;
const ABS_RZ: u32 = ABS + 5;
const ABS_THROTTLE: u32 = ABS + 6;
const BTN_TRIGGER: u32 = BTN + 0x120;
const BTN_THUMB: u32 = BTN + 0x121;
const BTN_THUMB2: u32 = BTN + 0x122;
const BTN_TOP: u32 = BTN + 0x123;
const BTN_TOP2: u32 = BTN + 0x124;

/// A ready-made set of bindings for a kind of device.
pub struct Preset {
    /// its name's key in the language files
    key: &'static str,
    /// lower-case pieces of a device name it is put on by itself (none: only by hand)
    names: &'static [&'static str],
    /// (axis, code, sign, dead zone)
    axes: &'static [(AxisAct, u32, f32, f32)],
    /// (action id, button code)
    buttons: &'static [(&'static str, u32)],
}

impl Preset {
    pub fn label(&self) -> &'static str {
        crate::i18n::tr(self.key)
    }
}

pub const PRESETS: &[Preset] = &[
    // Measured on the stick itself: Y pitch, X roll (reversed), the twist (Z)
    // rudder, the throttle wheel on the base on Rz.
    Preset {
        key: "preset.velocityone",
        names: &["velocityone flightstick"],
        axes: &[(AxisAct::Pitch, ABS_Y, 1.0, 0.05), (AxisAct::Roll, ABS_X, -1.0, 0.05), (AxisAct::Yaw, ABS_Z, 1.0, 0.08), (AxisAct::Throttle, ABS_RZ, 1.0, 0.02)],
        buttons: &[("brakes", BTN_TRIGGER), ("camera", BTN_THUMB), ("flaps_down", BTN_THUMB2), ("flaps_up", BTN_TOP), ("gear", BTN_TOP2)],
    },
    // The usual HID layout (Logitech Extreme 3D, Thrustmaster T.16000M and
    // the like): X roll, Y pitch, the twist on Rz, a throttle that reads
    // lowest pushed forward.
    Preset {
        key: "preset.joystick",
        names: &[],
        axes: &[(AxisAct::Pitch, ABS_Y, 1.0, 0.05), (AxisAct::Roll, ABS_X, 1.0, 0.05), (AxisAct::Yaw, ABS_RZ, 1.0, 0.08), (AxisAct::Throttle, ABS_THROTTLE, -1.0, 0.02)],
        buttons: &[("brakes", BTN_TRIGGER), ("camera", BTN_THUMB), ("flaps_down", BTN_THUMB2), ("flaps_up", BTN_TOP), ("gear", BTN_TOP2)],
    },
    // Rudder pedals (Logitech/Saitek, Thrustmaster TFRP…): rudder on Rz,
    // the toe brakes on X and Y.
    Preset {
        key: "preset.pedals",
        names: &["rudder", "pedal"],
        axes: &[(AxisAct::Yaw, ABS_RZ, 1.0, 0.03), (AxisAct::BrakeLeft, ABS_X, 1.0, 0.05), (AxisAct::BrakeRight, ABS_Y, 1.0, 0.05)],
        buttons: &[],
    },
];

/// The preset that goes on a device by its name, if any.
pub fn preset_for(name: &str) -> Option<&'static Preset> {
    let n = name.to_lowercase();
    PRESETS.iter().find(|p| p.names.iter().any(|m| n.contains(m)))
}

impl Bindings {
    /// Puts a preset on a device. `over`: replace what is bound already
    /// (by hand); else only fill in what is free (on its own).
    pub fn apply_preset(&mut self, p: &Preset, dev: Uuid, name: &str, over: bool) {
        for &(a, code, sign, dead) in p.axes {
            if over || !self.axes.contains_key(&a) {
                self.axes.insert(a, AxisBind { dev, name: name.to_string(), code, sign, dead });
            }
        }
        for &(id, code) in p.buttons {
            if let Some(a) = action(id) {
                if over || !self.buttons.contains_key(a.id) {
                    self.buttons.retain(|_, b| !(b.dev == dev && b.code == code));
                    self.buttons.insert(a.id, ButtonBind { dev, name: name.to_string(), code });
                }
            }
        }
        self.auto_done.insert(dev);
    }

    /// A device seen: the first time, if it has nothing bound and a preset
    /// knows it by name, that preset goes on. Returns the preset's name.
    pub fn auto_preset(&mut self, dev: Uuid, name: &str) -> Option<&'static str> {
        if !self.auto_done.insert(dev) || self.uses(&dev) {
            return None;
        }
        let p = preset_for(name)?;
        self.apply_preset(p, dev, name, false);
        Some(p.label())
    }

    /// Takes everything off a device.
    pub fn clear_device(&mut self, dev: &Uuid) {
        self.axes.retain(|_, b| b.dev != *dev);
        self.buttons.retain(|_, b| b.dev != *dev);
    }
}

fn uuid_from_hex(hex: &str) -> Option<Uuid> {
    let mut u = [0u8; 16];
    for (i, b) in u.iter_mut().enumerate() {
        *b = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(u)
}

/// An axis by its evdev name ("Y", "Rz", "Throttle"), else its number.
fn axis_name(code: u32) -> String {
    const N: [&str; 11] = ["X", "Y", "Z", "Rx", "Ry", "Rz", "Throttle", "Rudder", "Wheel", "Gas", "Brake"];
    match (code >> 16, code & 0xffff) {
        (3, c) if (c as usize) < N.len() => N[c as usize].to_string(),
        (_, c) => c.to_string(),
    }
}

/// A button by its number on a joystick (1 = trigger), or its gamepad name.
fn button_name(code: u32) -> String {
    const PAD: [&str; 15] = ["A", "B", "C", "X", "Y", "Z", "LB", "RB", "LT", "RT", "Select", "Start", "Mode", "L3", "R3"];
    match code & 0xffff {
        c @ 0x120..=0x12f => (c - 0x120 + 1).to_string(),
        c @ 0x130..=0x13e => PAD[(c - 0x130) as usize].to_string(),
        c @ 0x2c0..=0x2ff => (c - 0x2c0 + 17).to_string(),
        c => format!("0x{c:x}"),
    }
}

/// A device's name, cut short for the bindings page.
fn short(name: &str) -> String {
    let n: String = name.chars().take(40).collect();
    if n.is_empty() { "?".into() } else { n }
}

/// The keys a binding can be put on, by the name winit gives them.
const KEYS: &[KeyCode] = {
    use KeyCode::*;
    &[
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide, NumpadDecimal,
        F2, F4, F5, F6, F7, F8, F9, F10, F11, F12,
        ArrowUp, ArrowDown, ArrowLeft, ArrowRight, Home, End, PageUp, PageDown, Insert, Delete,
        ShiftLeft, ShiftRight, ControlLeft, ControlRight, AltLeft, AltRight, Space, Backspace,
        Comma, Period, Slash, Semicolon, Quote, BracketLeft, BracketRight, Backslash, Minus, Equal, Backquote, IntlBackslash,
    ]
};

fn key_from_name(s: &str) -> Option<KeyCode> {
    KEYS.iter().copied().find(|k| format!("{k:?}") == s)
}

/// Whether a key can take a binding (not Esc, Tab, F1, F3, Enter: the menus' own).
pub fn bindable(k: KeyCode) -> bool {
    KEYS.contains(&k)
}

/// A key's name for the bindings page: "W", "Left Shift", "Num 7"…
pub fn key_name(k: KeyCode) -> String {
    let s = format!("{k:?}");
    if let Some(c) = s.strip_prefix("Key").or_else(|| s.strip_prefix("Digit")) {
        return c.to_string();
    }
    if let Some(c) = s.strip_prefix("Numpad") {
        return format!("{} {c}", crate::i18n::tr("key.numpad"));
    }
    let named = match k {
        KeyCode::ShiftLeft => crate::i18n::tr("key.left_shift"),
        KeyCode::ShiftRight => crate::i18n::tr("key.right_shift"),
        KeyCode::ControlLeft => crate::i18n::tr("key.left_ctrl"),
        KeyCode::ControlRight => crate::i18n::tr("key.right_ctrl"),
        KeyCode::AltLeft => "Alt",
        KeyCode::AltRight => "Alt Gr",
        KeyCode::Space => crate::i18n::tr("key.space"),
        KeyCode::Period => ".",
        KeyCode::Comma => ",",
        KeyCode::Slash => "/",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::Backquote => "`",
        KeyCode::IntlBackslash => return crate::i18n::tr("key.iso").to_string(),
        KeyCode::ArrowUp => "↑",
        KeyCode::ArrowDown => "↓",
        KeyCode::ArrowLeft => "←",
        KeyCode::ArrowRight => "→",
        KeyCode::PageUp => crate::i18n::tr("key.page_up"),
        KeyCode::PageDown => crate::i18n::tr("key.page_down"),
        KeyCode::Escape => crate::i18n::tr("key.esc"),
        _ => "",
    };
    if named.is_empty() { s } else { named.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canopy_and_master_arm_rebind_independently() {
        let mut b = Bindings::default();
        let f = Context::Flight;
        let canopy = action("canopy").unwrap();
        let arm = action("master_arm").unwrap();
        assert_ne!(b.key_of(f, canopy), b.key_of(f, arm));
        b.set_key(f, "canopy", KeyCode::Digit7);
        assert_eq!(b.map_key(f, KeyCode::Digit7), Some(canopy.key));
        assert_eq!(b.key_of(f, arm), Some(arm.key));
        assert_eq!(b.map_key(f, arm.key), Some(arm.key));
        b.set_key(f, "master_arm", KeyCode::Digit8);
        assert_eq!(b.map_key(f, KeyCode::Digit8), Some(arm.key));
        assert_eq!(b.map_key(f, KeyCode::Digit7), Some(canopy.key));
    }

    #[test]
    fn rear_camera_iso_shortcut_respects_custom_bindings() {
        use KeyCode::{F6, F9, IntlBackslash, KeyL};
        let f = Context::Flight;
        let mut b = Bindings::default();
        assert_eq!(b.map_key(f, F6), Some(F6));
        assert_eq!(b.map_key(f, IntlBackslash), Some(F6));
        b.set_key(f, "gear", IntlBackslash);
        assert_eq!(b.map_key(f, IntlBackslash), Some(KeyL));
        b.reset_key(f, "gear");
        b.set_key(f, "tail_view", F9);
        assert_eq!(b.map_key(f, IntlBackslash), Some(IntlBackslash));
        assert_eq!(b.map_key(f, F6), None);
        assert_eq!(b.map_key(f, F9), Some(F6));
    }

    #[test]
    fn moving_keys() {
        let f = Context::Flight;
        let mut b = Bindings::default();
        let pitch_down = action("pitch_down").unwrap();
        let pitch_up = action("pitch_up").unwrap();
        assert_eq!(b.map_key(f, KeyCode::KeyW), Some(KeyCode::KeyW));
        // Pitch down onto F9: F9 now pitches down, W does nothing.
        b.set_key(f, "pitch_down", KeyCode::F9);
        assert_eq!(b.map_key(f, KeyCode::F9), Some(KeyCode::KeyW));
        assert_eq!(b.map_key(f, KeyCode::KeyW), None);
        // Pitch down onto S takes it from pitch up, which is left without a key.
        b.set_key(f, "pitch_down", KeyCode::KeyS);
        assert_eq!(b.map_key(f, KeyCode::KeyS), Some(KeyCode::KeyW));
        assert_eq!(b.key_of(f, pitch_up), None);
        assert_eq!(b.map_key(f, KeyCode::F9), Some(KeyCode::F9));
        // Back to the defaults.
        b.reset_key(f, "pitch_down");
        b.reset_key(f, "pitch_up");
        assert_eq!((b.key_of(f, pitch_down), b.key_of(f, pitch_up)), (Some(KeyCode::KeyW), Some(KeyCode::KeyS)));
        assert_eq!(b.map_key(f, KeyCode::KeyS), Some(KeyCode::KeyS));
        assert!(b.keys.is_empty());
    }

    #[test]
    fn axes_learn_their_sign() {
        let mut b = Bindings::default();
        let dev = [7u8; 16];
        b.axis_event(dev, "Stick", 1, 0.02);
        b.start_capture(Capture::Axis(AxisAct::Pitch));
        // Another axis drifting a little is not taken; this one pulled back (to -1) is.
        b.axis_event(dev, "Stick", 5, 0.3);
        b.axis_event(dev, "Stick", 1, -0.9);
        assert_eq!(b.capture, None);
        assert_eq!(b.axes[&AxisAct::Pitch].sign, -1.0);
        assert!((b.axis(AxisAct::Pitch).unwrap() - 0.896).abs() < 0.01);
        // A throttle at idle reads 0, full 1.
        b.start_capture(Capture::Axis(AxisAct::Throttle));
        b.axis_event(dev, "Stick", 2, -1.0);
        b.axis_event(dev, "Stick", 2, 1.0);
        assert_eq!(b.axis(AxisAct::Throttle), Some(1.0));
        b.axis_event(dev, "Stick", 2, -1.0);
        assert_eq!(b.axis(AxisAct::Throttle), Some(0.0));
    }

    #[test]
    fn buttons_and_file() {
        let f = Context::Flight;
        let mut b = Bindings::default();
        let dev = [0xab; 16];
        b.start_capture(Capture::Button("gear"));
        b.button_event(dev, "Throttle quadrant", 290, true);
        b.button_event(dev, "Throttle quadrant", 290, false);
        b.button_event(dev, "Throttle quadrant", 290, true);
        assert_eq!(b.take_edges(), vec![("gear", false), ("gear", true)]);
        b.set_key(f, "gear", KeyCode::KeyJ);
        b.axes.insert(AxisAct::Yaw, AxisBind { dev, name: "Pedals".into(), code: 5, sign: -1.0, dead: 0.1 });
        let mut c = Bindings::default();
        c.read(&b.json());
        assert_eq!(c.buttons, b.buttons);
        assert_eq!(c.axes, b.axes);
        assert_eq!(c.map_key(f, KeyCode::KeyJ), Some(KeyCode::KeyL));
        assert!(c.uses(&dev));
        assert_eq!((axis_name(ABS_Y), axis_name(ABS_RZ), button_name(BTN_TRIGGER), button_name(BTN + 0x2c5)), ("Y".into(), "Rz".into(), "1".into(), "22".into()));
    }

    #[test]
    fn contexts_share_keys_but_not_actions() {
        // W pitches down in flight, walks forward on foot and opens the
        // throttle in a car; the same physical key, three actions.
        let b = Bindings::default();
        assert_eq!(b.action_at(Context::Flight, KeyCode::KeyW).map(|a| a.id), Some("pitch_down"));
        assert_eq!(b.action_at(Context::Foot, KeyCode::KeyW).map(|a| a.id), Some("move_forward"));
        assert_eq!(b.action_at(Context::Car, KeyCode::KeyW).map(|a| a.id), Some("car_throttle"));
        // Global actions are in every context.
        for ctx in Context::ALL {
            assert_eq!(b.action_at(ctx, KeyCode::KeyC).map(|a| a.id), Some("camera"));
            assert_eq!(b.action_at(ctx, KeyCode::KeyH).map(|a| a.id), Some("hud"));
            assert_eq!(b.action_at(ctx, KeyCode::Escape).map(|a| a.id), Some("pause_menu"));
        }
        // A key moved on foot leaves the flight and car contexts alone.
        let mut b = b;
        b.set_key(Context::Foot, "move_forward", KeyCode::F9);
        assert_eq!(b.map_key(Context::Foot, KeyCode::F9), Some(KeyCode::KeyW));
        assert_eq!(b.map_key(Context::Foot, KeyCode::KeyW), None);
        assert_eq!(b.map_key(Context::Flight, KeyCode::F9), Some(KeyCode::F9));
        assert_eq!(b.action_at(Context::Flight, KeyCode::KeyW).map(|a| a.id), Some("pitch_down"));
        assert_eq!(b.action_at(Context::Car, KeyCode::KeyW).map(|a| a.id), Some("car_throttle"));
        // Actions that were there still are: every table's keys load back.
        let mut c = Bindings::default();
        c.read(&b.json());
        assert_eq!(c.map_key(Context::Foot, KeyCode::F9), Some(KeyCode::KeyW));
        // The flight table is unchanged by the new contexts.
        assert!(actions_in(Context::Flight).all(|a| ACTIONS.iter().any(|f| f.id == a.id)));
        assert_eq!(actions_in(Context::Flight).count(), ACTIONS.len());
        assert!(actions_in(Context::Foot).any(|a| a.id == "move_forward") && actions_in(Context::Foot).any(|a| a.id == "camera"));
    }

    #[test]
    fn legacy_bindings_keep_flight_keys_and_gain_contexts() {
        // A FableFlight file, from before foot and car contexts existed: only
        // flight actions, one of them moved. The old custom key stays; the new
        // contexts appear with their own defaults.
        let j = serde_json::json!({
            "keys": { "pitch_down": "F9" },
            "axes": [], "buttons": [], "presets_applied": [], "gamepad": {}
        });
        let mut b = Bindings::default();
        b.read(&j);
        assert_eq!(b.key_of(Context::Flight, action("pitch_down").unwrap()), Some(KeyCode::F9));
        assert_eq!(b.map_key(Context::Flight, KeyCode::F9), Some(KeyCode::KeyW));
        assert_eq!(b.map_key(Context::Foot, KeyCode::F9), Some(KeyCode::F9));
        assert_eq!(b.action_at(Context::Foot, KeyCode::KeyW).map(|a| a.id), Some("move_forward"));
        assert_eq!(b.action_at(Context::Car, KeyCode::KeyW).map(|a| a.id), Some("car_throttle"));
        // A duplicate in an old file does not leave one key on two actions:
        // the explicit action wins, otherwise the earlier one keeps it.
        let mut b = Bindings::default();
        b.read(&serde_json::json!({ "keys": { "gear": "KeyJ" } }));
        assert_eq!(b.key_of(Context::Flight, action("gear").unwrap()), Some(KeyCode::KeyJ));
        assert_eq!(b.key_of(Context::Flight, action("full_map").unwrap()), None);
        // An explicit key wins even over an earlier default.
        let mut b = Bindings::default();
        b.read(&serde_json::json!({ "keys": { "full_map": "KeyS" } }));
        assert_eq!(b.key_of(Context::Flight, action("full_map").unwrap()), Some(KeyCode::KeyS));
        assert_eq!(b.key_of(Context::Flight, action("pitch_up").unwrap()), None);
        for ctx in Context::ALL {
            let mut keys = std::collections::HashSet::new();
            for a in actions_in(ctx).filter(|a| a.keyboard) {
                if let Some(k) = b.key_of(ctx, a) {
                    assert!(keys.insert(k), "{} puts two actions on {k:?}", ctx.name());
                }
            }
        }
    }

    #[test]
    fn dead_zones() {
        let mut b = Bindings::default();
        let dev = [1u8; 16];
        b.axes.insert(AxisAct::Roll, AxisBind { dev, name: "S".into(), code: 0, sign: 1.0, dead: 0.2 });
        b.axes.insert(AxisAct::Throttle, AxisBind { dev, name: "S".into(), code: 6, sign: 1.0, dead: 0.1 });
        b.axis_event(dev, "S", 0, 0.15);
        b.axis_event(dev, "S", 6, -0.85);
        assert_eq!((b.axis(AxisAct::Roll), b.axis(AxisAct::Throttle)), (Some(0.0), Some(0.0)));
        b.axis_event(dev, "S", 0, 0.6);
        b.axis_event(dev, "S", 6, 1.0);
        assert!((b.axis(AxisAct::Roll).unwrap() - 0.5).abs() < 1e-6);
        assert_eq!(b.axis(AxisAct::Throttle), Some(1.0));
    }

    #[test]
    fn presets() {
        let mut b = Bindings::default();
        let stick = [2u8; 16];
        // A VelocityOne seen for the first time gets its preset, once.
        assert!(b.auto_preset(stick, "Turtle Beach VelocityOne Flightstick").is_some());
        assert_eq!(b.axes[&AxisAct::Yaw].code, ABS_Z);
        assert_eq!(b.buttons["gear"].code, BTN_TOP2);
        b.clear_device(&stick);
        assert!(b.auto_preset(stick, "Turtle Beach VelocityOne Flightstick").is_none());
        assert!(b.axes.is_empty());
        // Pedals fill in only what is free; an unknown device is left alone.
        let pedals = [3u8; 16];
        b.apply_preset(&PRESETS[1], stick, "Stick", true);
        assert!(b.auto_preset(pedals, "Thrustmaster T-Rudder").is_some());
        assert_eq!(b.axes[&AxisAct::Yaw].dev, stick);
        assert_eq!(b.axes[&AxisAct::BrakeLeft].dev, pedals);
        assert!(b.auto_preset([4u8; 16], "Some Gamepad").is_none());
        // And it is all remembered.
        let mut c = Bindings::default();
        c.read(&b.json());
        assert!(c.auto_preset(stick, "Turtle Beach VelocityOne Flightstick").is_none());
        assert_eq!(c.axes, b.axes);
    }
}
