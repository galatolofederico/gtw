//! Dynamic quality: the graphics settings the player chose become a ceiling,
//! and while flying each is lowered, then raised again, to hold a frame rate.
//!
//! The settings are lowered in a fixed order, one step at a time: the order is
//! the ladder below, cheapest-looking losses first, built from the profile of
//! where a frame's GPU time goes (RTX 3060, 1080p, after the a6e5a42 passes):
//!
//! - storm cells: the storm march alone is 2.8-3.5 ms, and clouds total about
//!   6 ms in storm weather against 1.9 ms for a cumulus field. With storms on
//!   screen the storm detail goes first, down to Low, before anything else.
//! - the cloud shadow map (0.35-0.9 ms at any resolution) and the cloud steps.
//! - trees: leaf cards, impostors and their shadows are 1.5-2 ms in forests.
//! - city detail: buildings are 1.3-2.7 ms over a city.
//! - shadows, terrain, water reflections, AO.
//! - last the render resolution, a tenth at a time (it remakes the render
//!   targets, so it changes least often), down to two thirds of the ceiling.
//!
//! Scenery distance, traffic and birds are left alone: they cost mostly CPU
//! and streaming, and changing them makes the world load and unload. MSAA,
//! the upscaler, TAA and vsync are never touched either.
//!
//! What is measured is the frame's GPU time (gputime.rs `FrameClock`, read a
//! frame or two late without waiting), since that is what the settings
//! change; a frame held back by the CPU or by vsync does not lower them.
//! Where the adapter has no timestamps the frame time stands in, and with
//! vsync (which hides any headroom) the settings are raised by trying them:
//! a step that brings back slow frames soon after is waited longer for the
//! next time.
use crate::graphics::{Graphics, Level, SCALE_MIN};

/// One step down of one setting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Knob {
    Storms,
    CloudShadows,
    Clouds,
    Trees,
    City,
    Shadows,
    Terrain,
    Water,
    Ao,
    Scale,
}

/// The order the settings are lowered in (each entry one step).
const LADDER: [Knob; 23] = [
    Knob::Clouds,
    Knob::Trees,
    Knob::City,
    Knob::CloudShadows,
    Knob::Shadows,
    Knob::Trees,
    Knob::Clouds,
    Knob::Terrain,
    Knob::Water,
    Knob::City,
    Knob::Scale,
    Knob::Ao,
    Knob::Trees,
    Knob::Clouds,
    Knob::Shadows,
    Knob::Scale,
    Knob::City,
    Knob::Terrain,
    Knob::Scale,
    Knob::Scale,
    Knob::Terrain,
    Knob::Water,
    Knob::Shadows,
];

/// One level lower, never below `floor`, and never up: a setting already at
/// or under its floor (Off included) stays where it is.
fn down(l: Level, floor: Level) -> Level {
    if rank(l) <= rank(floor) {
        return l;
    }
    match l {
        Level::Ultra => Level::High,
        Level::High => Level::Medium,
        _ => Level::Low,
    }
}

fn rank(l: Level) -> u8 {
    match l {
        Level::Off => 0,
        Level::Low => 1,
        Level::Medium => 2,
        Level::High => 3,
        Level::Ultra => 4,
    }
}

/// `g` one step lower on `k`, or None where it is already at its floor.
fn step(g: &Graphics, ceiling: &Graphics, k: Knob) -> Option<Graphics> {
    let mut n = *g;
    match k {
        Knob::Storms => n.storms = down(g.storms, Level::Low),
        Knob::CloudShadows => n.cloud_shadows = false,
        // (never Off: that would take the clouds away, not make them cheaper)
        Knob::Clouds => n.clouds = down(g.clouds, Level::Low),
        Knob::Trees => n.trees = down(g.trees, Level::Low),
        Knob::City => n.city = down(g.city, Level::Low),
        // (never Off: the scene goes flat without them)
        Knob::Shadows => n.shadows = down(g.shadows, Level::Low),
        Knob::Terrain => n.terrain = down(g.terrain, Level::Low),
        Knob::Water => n.water = down(g.water, Level::Medium),
        Knob::Ao => n.ao = false,
        Knob::Scale => {
            let top = ceiling.render_scale();
            let floor = (top * 0.67).max(SCALE_MIN);
            let s = ((g.render_scale() - 0.1) * 20.0).round() / 20.0;
            n.scale = s.max(floor);
        }
    }
    (n != *g).then_some(n)
}

/// Every setting from the ceiling down, one step apart: [0] is the ceiling.
fn ladder(ceiling: &Graphics, storms: bool) -> Vec<Graphics> {
    let mut out = vec![*ceiling];
    let mut g = *ceiling;
    let first: &[Knob] = if storms { &[Knob::Storms, Knob::Storms, Knob::Storms] } else { &[] };
    for &k in first.iter().chain(LADDER.iter()) {
        if let Some(n) = step(&g, ceiling, k) {
            g = n;
            out.push(g);
        }
    }
    out
}

/// The controller's state.
#[derive(Debug)]
pub struct Dynamic {
    /// steps down from the ceiling
    pub down: usize,
    /// smoothed frame cost (ms)
    pub ema: f32,
    over: f32,
    under: f32,
    cool: f32,
    /// how long the frames must have room before a step back up (s)
    up_wait: f32,
    /// time since the last step up (s)
    since_up: f32,
    /// time without a failed step up (s)
    calm: f32,
    storms: bool,
    /// seconds the storms count as on screen after they were last seen, so
    /// one at the edge of range does not swap the ladder every frame
    storm_hold: f32,
    steps: Vec<Graphics>,
    ceiling: Option<Graphics>,
}

impl Default for Dynamic {
    fn default() -> Self {
        Self { down: 0, ema: 0.0, over: 0.0, under: 0.0, cool: 0.0, up_wait: UP_WAIT, since_up: f32::INFINITY, calm: 0.0, storms: false, storm_hold: 0.0, steps: Vec::new(), ceiling: None }
    }
}

/// Seconds of frames with room before stepping back up, at first.
const UP_WAIT: f32 = 3.0;
/// Seconds of slow frames before stepping down.
const DOWN_AFTER: f32 = 0.35;
/// Seconds storms keep their place at the top of the ladder once out of sight.
const STORM_HOLD: f32 = 5.0;

impl Dynamic {
    /// The settings to draw with: `ceiling` (the player's) lowered as far as
    /// the controller has gone. `storms`: storm cells are on screen.
    pub fn settings(&mut self, ceiling: &Graphics, storms: bool) -> Graphics {
        if !ceiling.dynamic {
            *self = Self::default();
            return *ceiling;
        }
        if storms {
            self.storm_hold = STORM_HOLD;
        }
        let storms = self.storm_hold > 0.0;
        if self.ceiling != Some(*ceiling) || self.storms != storms || self.steps.is_empty() {
            let steps = ladder(ceiling, storms);
            // (storms coming or going add or remove the storm steps at the
            // top: keep the rest of the ladder where it was. At the ceiling it
            // stays at the ceiling: with room to spare a storm keeps its detail.)
            if self.storms != storms && !self.steps.is_empty() && self.down > 0 {
                let extra = steps.len().abs_diff(ladder(ceiling, !storms).len());
                self.down = if storms { self.down + extra } else { self.down.saturating_sub(extra) };
            }
            self.steps = steps;
            self.ceiling = Some(*ceiling);
            self.storms = storms;
        }
        self.down = self.down.min(self.steps.len() - 1);
        self.steps[self.down]
    }

    /// One frame: `dt` its length (s), `gpu_ms` its GPU time where known,
    /// `target_ms` the frame time to hold, `paced` whether vsync or a frame limit paces
    /// frames (so the frame time shows no headroom).
    pub fn update(&mut self, dt: f32, gpu_ms: Option<f32>, target_ms: f32, paced: bool) {
        // (a hitch: loading, a menu, a window drag)
        if !(0.0..0.25).contains(&dt) || self.steps.is_empty() {
            return;
        }
        let (cost, measured) = match gpu_ms {
            Some(ms) => (ms, true),
            None => (dt * 1000.0, false),
        };
        self.ema = if self.ema <= 0.0 { cost } else { self.ema + (cost - self.ema) * (1.0 - (-dt / 0.15).exp()) };
        self.storm_hold -= dt;
        self.since_up += dt;
        self.calm += dt;
        if self.calm > 45.0 {
            // a long while without a failed step up: try again sooner
            self.up_wait = (self.up_wait * 0.5).max(UP_WAIT);
            self.calm = 0.0;
        }
        self.cool -= dt;
        if self.cool > 0.0 {
            return;
        }
        // the GPU's share of the frame, leaving room for the CPU and the present
        let budget = if measured { target_ms * 0.9 } else { target_ms * 1.04 };
        if self.ema > budget {
            self.over += dt;
            self.under = 0.0;
        } else {
            self.over = 0.0;
            // room to step up: clear headroom where it is measured; with only the
            // frame time and frames paced, frames on time are as good as it shows
            let room = if measured { self.ema < target_ms * 0.72 } else { paced || self.ema < target_ms * 0.8 };
            self.under = if room { self.under + dt } else { 0.0 };
        }
        if self.over > DOWN_AFTER && self.down + 1 < self.steps.len() {
            self.down += 1;
            if self.since_up < 4.0 {
                // the last step up was too far: wait longer before the next
                self.up_wait = (self.up_wait * 2.0).min(60.0);
                self.calm = 0.0;
            }
            self.over = 0.0;
            self.cool = 0.8;
        } else if self.under > self.up_wait && self.down > 0 {
            self.down -= 1;
            self.under = 0.0;
            self.since_up = 0.0;
            self.cool = 1.2;
        }
    }

    /// How many steps there are below the ceiling.
    pub fn depth(&self) -> usize {
        self.steps.len().saturating_sub(1)
    }
}

/// The frame time to hold (ms): the setting, or the display's rate (60 Hz
/// where it is unknown), never past the frame limit. `refresh_hz` is given
/// where the display's rate applies: with vsync (nothing faster is shown, so
/// a higher setting is held to it), or with the setting at "display".
pub fn target_ms(g: &Graphics, refresh_hz: Option<f32>) -> f32 {
    let hz = refresh_hz.filter(|h| *h >= 30.0);
    let mut fps = match (g.dyn_fps, hz) {
        (0, h) => h.unwrap_or(60.0),
        (f, Some(h)) => (f as f32).min(h),
        (f, None) => f as f32,
    };
    if g.fps_cap > 0 {
        fps = fps.min(g.fps_cap as f32);
    }
    1000.0 / fps.max(20.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::Preset;

    fn dyn_ultra() -> Graphics {
        let mut g = Graphics::preset(Preset::Ultra);
        g.dynamic = true;
        g
    }

    #[test]
    fn off_is_the_ceiling() {
        let g = Graphics::preset(Preset::High);
        let mut d = Dynamic::default();
        for _ in 0..200 {
            d.update(0.1, Some(90.0), 16.6, true);
        }
        assert_eq!(d.settings(&g, true), g);
    }

    #[test]
    fn storms_go_first() {
        let l = ladder(&dyn_ultra(), true);
        assert_eq!(l[1].storms, Level::High);
        assert_eq!(l[3].storms, Level::Low);
        assert_eq!(l[3].clouds, Level::Ultra);
        assert_eq!(ladder(&dyn_ultra(), false)[1].storms, Level::Ultra);
    }

    #[test]
    fn storms_appearing_at_the_ceiling_keep_their_detail() {
        let c = dyn_ultra();
        let mut d = Dynamic::default();
        assert_eq!(d.settings(&c, false), c);
        assert_eq!(d.settings(&c, true), c, "room to spare: storms stay at the ceiling");
        // already lowered: storms go first, the rest stays where it was
        let mut d = Dynamic::default();
        d.settings(&c, false);
        d.down = 2;
        let before = d.settings(&c, false);
        let now = d.settings(&c, true);
        assert_eq!(now.storms, Level::Low);
        assert_eq!(Graphics { storms: before.storms, ..now }, before);
        // and they keep that place a while after going out of sight
        assert_eq!(d.settings(&c, false), now);
        for _ in 0..60 * 6 {
            d.update(1.0 / 60.0, Some(12.0), 16.6, true);
        }
        assert_eq!(d.settings(&c, false).storms, Level::Ultra);
    }

    #[test]
    fn never_raises_what_is_off() {
        let mut c = dyn_ultra();
        c.shadows = Level::Off;
        c.storms = Level::Off;
        assert!(ladder(&c, true).iter().all(|g| g.shadows == Level::Off && g.storms == Level::Off));
    }

    #[test]
    fn ladder_stays_above_floors() {
        let c = dyn_ultra();
        let l = ladder(&c, true);
        let last = l.last().unwrap();
        assert!(l.windows(2).all(|w| w[0] != w[1]));
        assert_eq!((last.trees, last.clouds, last.shadows, last.water, last.ao), (Level::Low, Level::Low, Level::Low, Level::Medium, false));
        assert!(last.scale >= c.render_scale() * 0.67 - 1e-4);
        // never touched
        assert_eq!((last.scenery, last.cars, last.birds, last.msaa, last.upscaler, last.vsync), (c.scenery, c.cars, c.birds, c.msaa, c.upscaler, c.vsync));
    }

    #[test]
    fn slow_frames_step_down_and_room_steps_up() {
        let c = dyn_ultra();
        let mut d = Dynamic::default();
        d.settings(&c, false);
        for _ in 0..60 {
            d.update(1.0 / 60.0, Some(25.0), 16.6, true);
        }
        assert!(d.down >= 1, "stepped down");
        let low = d.down;
        // the same cost stays put once the budget is met
        for _ in 0..600 {
            d.update(1.0 / 60.0, Some(13.0), 16.6, true);
        }
        assert_eq!(d.down, low);
        // plenty of room: back up
        for _ in 0..60 * 20 {
            d.update(1.0 / 60.0, Some(6.0), 16.6, true);
        }
        assert!(d.down < low);
    }

    #[test]
    fn failed_step_up_waits_longer() {
        let c = dyn_ultra();
        let mut d = Dynamic::default();
        d.settings(&c, false);
        d.down = 3;
        for _ in 0..60 * 4 {
            d.update(1.0 / 60.0, None, 16.6, true);
        }
        assert_eq!(d.down, 2);
        for _ in 0..120 {
            d.update(1.3 / 60.0, None, 16.6, true);
        }
        assert!(d.down >= 3);
        assert!(d.up_wait > UP_WAIT);
    }

    #[test]
    fn targets() {
        let mut g = dyn_ultra();
        assert!((target_ms(&g, Some(144.0)) - 1000.0 / 144.0).abs() < 1e-3);
        assert!((target_ms(&g, None) - 1000.0 / 60.0).abs() < 1e-3);
        g.dyn_fps = 45;
        assert!((target_ms(&g, Some(144.0)) - 1000.0 / 45.0).abs() < 1e-3);
        // vsync on a 60 Hz display: 144 cannot be shown, so 60 is held
        g.dyn_fps = 144;
        assert!((target_ms(&g, Some(60.0)) - 1000.0 / 60.0).abs() < 1e-3);
        assert!((target_ms(&g, None) - 1000.0 / 144.0).abs() < 1e-3);
        g.dyn_fps = 45;
        g.fps_cap = 30;
        assert!((target_ms(&g, Some(144.0)) - 1000.0 / 30.0).abs() < 1e-3);
    }
}
