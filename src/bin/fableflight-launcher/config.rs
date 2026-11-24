//! What the launcher keeps between runs, in `<config>/fablegame/launcher.json`
//! beside the game's own settings: how long each stage of the last start
//! took (for the progress bar), the graphics test results and a few options.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// the last finished start: each stage and the milliseconds it came at
    pub stage_ms: Vec<(String, u64)>,
    /// minimise the launcher once the game's window is drawing
    pub minimize_on_ready: bool,
    /// look for a new release when the launcher opens
    pub check_updates: bool,
    /// the release this launcher last installed into its folder
    pub installed_tag: Option<String>,
    /// the graphics test's results, newest last
    pub bench: Vec<BenchResult>,
}

impl Default for Config {
    fn default() -> Self {
        Config { stage_ms: Vec::new(), minimize_on_ready: true, check_updates: true, installed_tag: None, bench: Vec::new() }
    }
}

/// One run of the graphics test (the game's `--bench` line).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BenchResult {
    /// the preset tested ("Custom" for hand-tuned settings)
    pub preset: String,
    /// average frames per second
    pub fps: f64,
    /// the frame rates the slowest 1% and 0.1% of frames stay above
    pub low1: f64,
    pub low01: f64,
    pub worst_ms: f64,
    /// frames over 100 ms
    pub hitches: u32,
    pub gpu: String,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    /// seconds since 1970
    pub when: u64,
}

impl BenchResult {
    pub fn from_json(j: &serde_json::Value) -> Self {
        let mut r: BenchResult = serde_json::from_value(j.clone()).unwrap_or_default();
        r.when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        r
    }
}

/// How smooth a test run was.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// the slowest frames still above 50 fps
    Smooth,
    /// the slowest above 30
    Playable,
    /// stutters below 30
    Struggles,
}

impl Verdict {
    pub fn of(r: &BenchResult) -> Verdict {
        if r.low1 >= 50.0 && r.fps >= 60.0 {
            Verdict::Smooth
        } else if r.low1 >= 28.0 && r.fps >= 35.0 {
            Verdict::Playable
        } else {
            Verdict::Struggles
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Verdict::Smooth => "Stable",
            Verdict::Playable => "Playable",
            Verdict::Struggles => "Unstable",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Verdict::Smooth => "Smooth flying: even the slowest frames stay above 50 fps.",
            Verdict::Playable => "Flyable, with dips under 50 fps in busy scenery. Fine for most flights.",
            Verdict::Struggles => "Frame rate drops below 30 fps. Pick a lower preset or a lower render resolution.",
        }
    }
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("fablegame").join("launcher.json"))
}

impl Config {
    pub fn load() -> Self {
        path().and_then(|p| std::fs::read(p).ok()).and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
    }

    /// Writes it through a temporary file, as the game does its settings.
    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, serde_json::to_vec_pretty(self).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}
