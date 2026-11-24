//! The graphics settings themselves: the quality levels, the presets, the
//! upscaler choice and `graphics.json` (`<config>/gtw/graphics.json`).
//! Kept free of the game's other modules so the launcher
//! (src/bin/gtw-launcher/main.rs) reads and writes the very same file with
//! the very same presets; graphics.rs adds the renderer's figures, the labels
//! and the settings screen on top.
use std::path::PathBuf;

/// A quality step. `Off` only applies to the shadows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Off,
    Low,
    Medium,
    High,
    Ultra,
}

impl Level {
    pub const PRESETS: [Level; 4] = [Level::Low, Level::Medium, Level::High, Level::Ultra];

    pub fn name(self) -> &'static str {
        match self {
            Level::Off => "Off",
            Level::Low => "Low",
            Level::Medium => "Medium",
            Level::High => "High",
            Level::Ultra => "Ultra",
        }
    }
    pub fn parse(s: &str) -> Option<Level> {
        [Level::Off, Level::Low, Level::Medium, Level::High, Level::Ultra].into_iter().find(|l| l.name().eq_ignore_ascii_case(s.trim()))
    }

    /// Ultra as High, for the settings that stop at High.
    pub fn min_high(self) -> Level {
        if self == Level::Ultra { Level::High } else { self }
    }

    pub fn pick(self, low: f64, medium: f64, high: f64, ultra: f64) -> f64 {
        match self {
            Level::Off | Level::Low => low,
            Level::Medium => medium,
            Level::High => high,
            Level::Ultra => ultra,
        }
    }
}

/// A preset: every quality setting at once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Preset {
    /// for integrated graphics: Low, plus off whatever costs the same at any
    /// resolution
    VeryLow,
    Low,
    Medium,
    High,
    Ultra,
}

impl Preset {
    pub const ALL: [Preset; 5] = [Preset::VeryLow, Preset::Low, Preset::Medium, Preset::High, Preset::Ultra];

    pub fn name(self) -> &'static str {
        match self {
            Preset::VeryLow => "Very Low",
            Preset::Low => "Low",
            Preset::Medium => "Medium",
            Preset::High => "High",
            Preset::Ultra => "Ultra",
        }
    }

    /// "very low", "very-low", "verylow" and "very_low" all name Very Low.
    pub fn parse(s: &str) -> Option<Preset> {
        let k: String = s.trim().chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        Preset::ALL.into_iter().find(|p| p.name().replace(' ', "").eq_ignore_ascii_case(&k))
    }

    /// The level each quality setting gets.
    pub fn level(self) -> Level {
        match self {
            Preset::VeryLow | Preset::Low => Level::Low,
            Preset::Medium => Level::Medium,
            Preset::High => Level::High,
            Preset::Ultra => Level::Ultra,
        }
    }
}

/// How a frame drawn below the window's resolution is brought up to it
/// (upscale.rs has the renderer's side, docs/upscaling.md the background).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Upscaler {
    /// a bilinear stretch after the game's temporal AA: what the game always did
    #[default]
    Bilinear,
    /// AMD FSR 1: edge-aware spatial upscaling and sharpening (built in, MIT)
    Fsr1,
    /// AMD FSR 3.1: temporal upscaling in place of the temporal AA (built in, MIT)
    Fsr3,
    /// AMD FSR 4.1: machine-learning upscaling from AMD's runtime, downloaded
    /// on request (fsr4.rs); FSR 3.1 stands in where it cannot run
    Fsr4,
}

impl Upscaler {
    pub const ALL: [Upscaler; 4] = [Upscaler::Bilinear, Upscaler::Fsr1, Upscaler::Fsr3, Upscaler::Fsr4];

    pub fn name(self) -> &'static str {
        match self {
            Upscaler::Bilinear => "Bilinear",
            Upscaler::Fsr1 => "FSR 1",
            Upscaler::Fsr3 => "FSR 3.1",
            Upscaler::Fsr4 => "FSR 4.1",
        }
    }

    /// The name saved in graphics.json and taken by `--upscaler`.
    pub fn key(self) -> &'static str {
        match self {
            Upscaler::Bilinear => "bilinear",
            Upscaler::Fsr1 => "fsr1",
            Upscaler::Fsr3 => "fsr3",
            Upscaler::Fsr4 => "fsr4",
        }
    }

    pub fn parse(s: &str) -> Option<Upscaler> {
        let s = s.trim().to_ascii_lowercase().replace([' ', '.', '-', '_'], "");
        match s.as_str() {
            "bilinear" | "off" | "none" => Some(Upscaler::Bilinear),
            "fsr1" | "fsr10" => Some(Upscaler::Fsr1),
            "fsr3" | "fsr31" => Some(Upscaler::Fsr3),
            "fsr4" | "fsr41" => Some(Upscaler::Fsr4),
            _ => None,
        }
    }

    /// Builds its image from past frames as well (and so replaces the temporal AA).
    pub fn temporal(self) -> bool {
        matches!(self, Upscaler::Fsr3 | Upscaler::Fsr4)
    }
}

/// AMD's quality modes: render resolution as a share of the window's
/// (1 / 1.5, 1 / 1.7, 1 / 2); Native AA only for the temporal upscalers.
pub const QUALITY_MODES: [(&str, f32); 4] = [("Native AA", 1.0), ("Quality", 1.0 / 1.5), ("Balanced", 1.0 / 1.7), ("Performance", 0.5)];

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Graphics {
    /// render resolution as a share of the window's (each side): below 1 is
    /// upscaled, above 1 supersampled
    pub scale: f32,
    pub shadows: Level,
    /// the volumetric clouds (Off: none drawn, though the weather still flies)
    pub clouds: Level,
    /// the clouds' shadows on the ground: a map redrawn a slice a frame, the
    /// same cost at any resolution
    pub cloud_shadows: bool,
    /// storm cell rendering detail (separate from ordinary clouds so heavy
    /// thunderstorm, supercell and squall-line skies can be tuned on their own)
    pub storms: Level,
    /// condensation trails behind the player's aircraft and AI/live traffic
    pub contrails: bool,
    pub terrain: Level,
    /// how far out buildings and trees are loaded
    pub scenery: Level,
    /// how far out buildings keep their full geometry, dressing and window depth
    pub city: Level,
    /// how far out trees are drawn, and how near they turn into impostors
    pub trees: Level,
    /// moving and parked cars (Off, or Low to High; Ultra is High)
    pub cars: Level,
    /// birds in the air (birds.rs; Off, or Low to High; Ultra is High)
    pub birds: Level,
    /// ambient occlusion (GTAO)
    pub ao: bool,
    /// water: Low plain (a flat sea bed, simple foam, one reflected cloud
    /// sample), Medium all of it, High also the scenery reflected on screen
    /// (and in glass)
    pub water: Level,
    /// g-buffer MSAA samples (1, 2 or 4), applied on the next start
    pub msaa: u8,
    // Not part of the presets: preferences more than costs.
    /// temporal anti-aliasing (F2)
    pub taa: bool,
    /// how frames below the window's resolution are upscaled
    pub upscaler: Upscaler,
    pub vsync: bool,
    /// Optional frame limit (0 means no software limit).
    pub fps_cap: u16,
    /// dynamic quality (dynamic.rs): the settings above become a ceiling, and
    /// each is lowered and raised again while flying to hold `dyn_fps`
    pub dynamic: bool,
    /// the frame rate dynamic quality aims for (0: the display's refresh rate)
    pub dyn_fps: u16,
}

/// The frame rates dynamic quality can aim for (0: the display's).
pub const DYN_FPS: [u16; 7] = [0, 30, 45, 60, 90, 120, 144];

impl Default for Graphics {
    fn default() -> Self {
        Self::preset(Preset::High)
    }
}

/// (35% is for the weakest integrated graphics: blurry, but it flies)
pub const SCALE_MIN: f32 = 0.35;
pub const SCALE_MAX: f32 = 2.0;

impl Graphics {
    /// One of the presets (taa and vsync on).
    pub fn preset(p: Preset) -> Self {
        let mut g = Graphics { scale: 1.0, shadows: Level::High, clouds: Level::High, cloud_shadows: true, storms: Level::High, contrails: true, terrain: Level::High, scenery: Level::High, city: Level::High, trees: Level::High, cars: Level::High, birds: Level::High, ao: true, water: Level::High, msaa: 4, taa: true, upscaler: Upscaler::Bilinear, vsync: true, fps_cap: 0, dynamic: false, dyn_fps: 0 };
        g.apply(p);
        g
    }

    /// Sets everything a preset covers (not taa, the upscaler, vsync or the frame limit).
    pub fn apply(&mut self, p: Preset) {
        let l = p.level();
        let very_low = p == Preset::VeryLow;
        // each preset down roughly halves gpu work, resolution first, then shadows.
        self.scale = if very_low { 0.5 } else { l.pick(0.6, 0.75, 1.0, 1.5) as f32 };
        self.shadows = match l {
            Level::Off | Level::Low => Level::Off,
            Level::Medium => Level::Low,
            h => h,
        };
        self.clouds = l;
        self.cloud_shadows = !very_low;
        self.storms = l;
        self.contrails = !very_low;
        self.terrain = l;
        self.scenery = l;
        self.city = l;
        self.trees = l;
        let capped = if l == Level::Ultra { Level::High } else { l };
        self.cars = if very_low { Level::Off } else { capped };
        self.birds = if very_low { Level::Off } else { capped };
        self.ao = matches!(l, Level::High | Level::Ultra);
        // (Low has always gone without the reflections on screen)
        self.water = match p {
            Preset::VeryLow => Level::Low,
            Preset::Low => Level::Medium,
            _ => Level::High,
        };
        self.msaa = match p {
            Preset::VeryLow => 1,
            Preset::Low => 2,
            _ => 4,
        };
    }

    /// The preset these settings are, if any.
    pub fn matches(&self) -> Option<Preset> {
        Preset::ALL.into_iter().find(|l| {
            let mut p = *self;
            p.apply(*l);
            p == *self
        })
    }

    /// Whether the volumetric clouds are drawn at all.
    pub fn clouds_on(&self) -> bool {
        self.clouds != Level::Off
    }

}

impl Graphics {
    /// Whether the settings have ever been saved (else the defaults may
    /// adapt to the machine, e.g. a lower resolution on an integrated GPU).
    pub fn saved() -> bool {
        Self::path().is_some_and(|p| p.exists())
    }

    pub fn path() -> Option<PathBuf> {
        crate::paths::config().map(|d| d.join("graphics.json"))
    }

    /// The saved settings, or High.
    pub fn load() -> Self {
        let j: serde_json::Value = Self::path().and_then(|p| std::fs::read(p).ok()).and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default();
        Self::from_json(&j)
    }

    pub fn from_json(j: &serde_json::Value) -> Self {
        let mut g = Self::default();
        g.fps_cap = j["fps_cap"].as_u64().map(|v| v as u16).filter(|v| [0, 30, 60, 90, 120, 144, 165, 240].contains(v)).unwrap_or(0);
        g.dynamic = j["dynamic"].as_bool().unwrap_or(false);
        g.dyn_fps = j["dynamic_fps"].as_u64().map(|v| v as u16).filter(|v| DYN_FPS.contains(v)).unwrap_or(0);
        // a saved preset follows the code, so retuned presets reach old saves.
        if let Some(l) = j["preset"].as_str().and_then(Preset::parse) {
            g.apply(l);
            g.taa = j["taa"].as_bool().unwrap_or(g.taa);
            g.upscaler = j["upscaler"].as_str().and_then(Upscaler::parse).unwrap_or(g.upscaler);
            g.vsync = j["vsync"].as_bool().unwrap_or(g.vsync);
            return g;
        }
        let lvl = |k: &str, d: Level| j[k].as_str().and_then(Level::parse).filter(|l| *l != Level::Off).unwrap_or(d);
        if let Some(s) = j["scale"].as_f64() {
            g.scale = (s as f32).clamp(SCALE_MIN, SCALE_MAX);
        }
        g.shadows = j["shadows"].as_str().and_then(Level::parse).unwrap_or(g.shadows);
        g.clouds = j["clouds"].as_str().and_then(Level::parse).unwrap_or(g.clouds);
        g.cloud_shadows = j["cloud_shadows"].as_bool().unwrap_or(g.cloud_shadows);
        g.storms = lvl("storms", if g.clouds == Level::Off { Level::Low } else { g.clouds });
        g.contrails = j["contrails"].as_bool().unwrap_or(g.contrails);
        g.terrain = lvl("terrain", g.terrain);
        g.scenery = lvl("scenery", g.scenery);
        g.city = lvl("city", g.city);
        g.trees = lvl("trees", g.trees);
        g.cars = j["cars"].as_str().and_then(Level::parse).map(|l| if l == Level::Ultra { Level::High } else { l }).unwrap_or(g.cars);
        g.birds = j["birds"].as_str().and_then(Level::parse).map(|l| if l == Level::Ultra { Level::High } else { l }).unwrap_or(g.birds);
        g.ao = j["ao"].as_bool().unwrap_or(g.ao);
        g.water = lvl("water", g.water).min_high();
        g.msaa = j["msaa"].as_u64().filter(|n| [1, 2, 4].contains(n)).map(|n| n as u8).unwrap_or(g.msaa);
        g.taa = j["taa"].as_bool().unwrap_or(g.taa);
        g.upscaler = j["upscaler"].as_str().and_then(Upscaler::parse).unwrap_or(g.upscaler);
        g.vsync = j["vsync"].as_bool().unwrap_or(g.vsync);
        g
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "preset": self.matches().map(Preset::name).unwrap_or("custom"),
            "scale": self.scale,
            "shadows": self.shadows.name(),
            "clouds": self.clouds.name(),
            "cloud_shadows": self.cloud_shadows,
            "storms": self.storms.name(),
            "contrails": self.contrails,
            "terrain": self.terrain.name(),
            "scenery": self.scenery.name(),
            "city": self.city.name(),
            "trees": self.trees.name(),
            "cars": self.cars.name(),
            "birds": self.birds.name(),
            "ao": self.ao,
            "water": self.water.name(),
            "msaa": self.msaa,
            "taa": self.taa,
            "upscaler": self.upscaler.key(),
            "vsync": self.vsync,
            "fps_cap": self.fps_cap,
            "dynamic": self.dynamic,
            "dynamic_fps": self.dyn_fps,
        })
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        let j = self.to_json();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(&j).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

}
