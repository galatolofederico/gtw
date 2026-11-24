//! The resume snapshot: a small, versioned JSON file recording where the
//! player was and what they were doing (on foot, in a car, or in an aircraft),
//! so a relaunch can pick up in the same mode and place.
//!
//! It is a resume snapshot, not a simulation save: traffic positions, debris
//! and weather particles are not stored. The clock, the day and the snow
//! setting are, and live weather is fetched afresh.
//!
//! The file lives in `<config dir>/gtw/save.json` and is written through a
//! temporary file and a rename, so a crash mid-write cannot leave a half
//! written save. A missing, malformed, or future-version file is ignored and
//! the game starts afresh.
use crate::character;
use crate::snow;
use crate::world::WorldQuery;
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The only schema version this build understands.
pub const VERSION: u32 = 1;

/// What the player was doing when the snapshot was taken.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SaveMode {
    Foot,
    Drive,
    Fly,
}

/// The whole snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SaveState {
    pub version: u32,
    pub mode: SaveMode,
    #[serde(default)]
    pub foot: Option<SavedFoot>,
    #[serde(default)]
    pub car: Option<SavedCar>,
    #[serde(default)]
    pub plane: Option<SavedPlane>,
    #[serde(default)]
    pub last_car: Option<SavedCar>,
    #[serde(default)]
    pub clock: SavedClock,
}

/// The body on foot: where it stands, which way it faces, its health and look.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedFoot {
    pub pos: [f64; 3],
    pub yaw: f64,
    pub health: f32,
    pub style: u8,
}

/// The car being driven (or the last one left behind): model, paint, pose,
/// speed, health and how far the body is crumpled.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedCar {
    pub model: u8,
    pub colour: [f32; 3],
    pub pos: [f64; 3],
    pub yaw: f64,
    pub speed: f64,
    pub health: f32,
    pub damage: f32,
}

/// The aircraft: its type, pose, velocity, configuration and fuel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedPlane {
    pub spec: String,
    pub pos: [f64; 3],
    pub rot: [f64; 4],
    pub vel: [f64; 3],
    pub gear: f32,
    pub flaps: f32,
    pub throttle: f32,
    pub fuel: f32,
}

/// The clock and the snow setting.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedClock {
    /// UTC hours, 0..24.
    pub utc: f64,
    /// Day of the year, 0..365.
    pub doy: f64,
    /// "auto", "off" or "always".
    #[serde(default = "default_snow")]
    pub snow: String,
}

fn default_snow() -> String {
    "auto".into()
}

impl Default for SavedClock {
    fn default() -> Self {
        Self { utc: 12.0, doy: 0.0, snow: default_snow() }
    }
}

/// Why a save could not be loaded.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// No save file at the expected path.
    Missing,
    /// Unreadable, empty, or not the JSON this build expects.
    Corrupt,
    /// A valid file from a newer build.
    Unsupported(u32),
}

impl LoadError {
    pub fn message(self) -> &'static str {
        match self {
            LoadError::Missing => "no save",
            LoadError::Corrupt => "save is corrupt",
            LoadError::Unsupported(_) => "save is from a newer version",
        }
    }
}

/// The snapshot's path: `<config dir>/gtw/save.json`.
pub fn path() -> Option<PathBuf> {
    crate::paths::config().map(|d| d.join("save.json"))
}

impl SaveState {
    /// A snapshot of a player on foot.
    #[cfg(test)]
    pub fn foot(pos: DVec3, yaw: f64, health: f32, style: u8, clock: SavedClock) -> Self {
        Self {
            version: VERSION,
            mode: SaveMode::Foot,
            foot: Some(SavedFoot { pos: pos.to_array(), yaw, health, style }),
            car: None,
            plane: None,
            last_car: None,
            clock,
        }
    }

    /// Whether the mode's required record is present and every number sane.
    pub fn sane(&self) -> bool {
        if self.version != VERSION {
            return false;
        }
        let has = match self.mode {
            SaveMode::Foot => self.foot.is_some(),
            SaveMode::Drive => self.car.is_some(),
            SaveMode::Fly => self.plane.is_some(),
        };
        has && self.foot.as_ref().is_none_or(valid_foot)
            && self.car.as_ref().is_none_or(valid_car)
            && self.plane.as_ref().is_none_or(valid_plane)
            && self.last_car.as_ref().is_none_or(valid_car)
            && self.clock.utc.is_finite()
            && self.clock.doy.is_finite()
    }

    /// Load the default save, if there is one.
    pub fn load() -> Result<SaveState, LoadError> {
        let p = path().ok_or(LoadError::Missing)?;
        Self::load_from(&p)
    }

    /// Load a save from a path: missing, corrupt and future-version files are
    /// reported rather than trusted.
    pub fn load_from(p: &Path) -> Result<SaveState, LoadError> {
        let data = match std::fs::read(p) {
            Ok(d) => d,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(LoadError::Missing),
            Err(_) => return Err(LoadError::Corrupt),
        };
        if data.is_empty() {
            return Err(LoadError::Corrupt);
        }
        let state: SaveState = serde_json::from_slice(&data).map_err(|_| LoadError::Corrupt)?;
        if state.version != VERSION {
            return Err(LoadError::Unsupported(state.version));
        }
        if !state.sane() {
            return Err(LoadError::Corrupt);
        }
        Ok(state)
    }

    /// Write the default save atomically.
    pub fn save(&self) -> std::io::Result<()> {
        let p = path().ok_or_else(|| std::io::Error::other("no config directory"))?;
        self.save_to(&p)
    }

    /// Write to a path atomically: through a temporary file, then a rename.
    pub fn save_to(&self, p: &Path) -> std::io::Result<()> {
        let data = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        write_atomic(p, &data)
    }
}

fn valid_pos(p: &[f64; 3]) -> bool {
    p.iter().all(|c| c.is_finite()) && DVec3::from_array(*p).length() > 1.0
}

fn valid_foot(f: &SavedFoot) -> bool {
    valid_pos(&f.pos) && f.yaw.is_finite() && f.health.is_finite() && (0.0..=character::MAX_HEALTH).contains(&f.health)
}

fn valid_car(c: &SavedCar) -> bool {
    valid_pos(&c.pos)
        && c.yaw.is_finite()
        && c.speed.is_finite()
        && c.health.is_finite()
        && (0.0..=crate::vehicle::MAX_HEALTH).contains(&c.health)
        && (0.0..=1.0).contains(&c.damage)
        && c.colour.iter().all(|k| k.is_finite())
}

fn valid_plane(p: &SavedPlane) -> bool {
    let q = glam::DQuat::from_array(p.rot);
    valid_pos(&p.pos)
        && p.rot.iter().all(|c| c.is_finite())
        && q.length() > 0.5
        && p.vel.iter().all(|c| c.is_finite())
        && !p.spec.is_empty()
        && (0.0..=1.0).contains(&p.gear)
        && (0.0..=1.0).contains(&p.flaps)
        && (0.0..=1.0).contains(&p.throttle)
        && (0.0..=1.0).contains(&p.fuel)
}

/// Write `data` at `p` through `p.tmp`, then rename it into place. A failure
/// (a full disk, a write error) leaves the old file untouched.
pub fn write_atomic(p: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, p)
}

/// The snow mode named in a save.
pub fn snow_mode(code: &str) -> Option<snow::Mode> {
    snow::Mode::parse(code)
}

/// A position on the ground that a body can safely occupy: the nearest dry,
/// building-free, loaded point to `p` (see [`character::safe_point`]).
pub fn safe_position<W: WorldQuery>(world: &W, p: DVec3) -> Option<DVec3> {
    character::safe_point(world, p)
}

/// Where a snapshot put the player, for a menu label.
pub fn position(s: &SaveState) -> Option<DVec3> {
    let p = match s.mode {
        SaveMode::Foot => s.foot.as_ref().map(|f| f.pos),
        SaveMode::Drive => s.car.as_ref().map(|c| c.pos),
        SaveMode::Fly => s.plane.as_ref().map(|p| p.pos),
    }?;
    Some(DVec3::from_array(p))
}

/// A one-line description for the main menu's Continue bar.
pub fn summary(s: &SaveState) -> String {
    let mode = match s.mode {
        SaveMode::Foot => "on foot",
        SaveMode::Drive => "driving",
        SaveMode::Fly => "flying",
    };
    match position(s) {
        Some(p) => {
            let (lat, lon, _) = crate::geo::pos_to_lla(p);
            format!("{mode} · {:.3}, {:.3}", lat.to_degrees(), lon.to_degrees())
        }
        None => mode.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo;
    use crate::surface::{Surface, SurfaceHit};
    use std::sync::atomic::{AtomicU32, Ordering};

    fn origin() -> DVec3 {
        geo::lla_to_pos(0.5, 0.1, 0.0)
    }

    fn clock() -> SavedClock {
        SavedClock { utc: 21.25, doy: 123.0, snow: "always".into() }
    }

    fn foot_state() -> SaveState {
        SaveState::foot(origin() + DVec3::new(3.0, 4.0, 5.0), 1.25, 73.0, 2, clock())
    }

    fn car_state() -> SaveState {
        SaveState {
            version: VERSION,
            mode: SaveMode::Drive,
            foot: Some(SavedFoot { pos: origin().to_array(), yaw: 0.2, health: 90.0, style: 1 }),
            car: Some(SavedCar { model: 3, colour: [0.2, 0.4, 0.6], pos: (origin() + DVec3::new(11.0, 12.0, 13.0)).to_array(), yaw: 2.0, speed: 14.5, health: 41.0, damage: 0.59 }),
            plane: None,
            last_car: Some(SavedCar { model: 7, colour: [0.7, 0.1, 0.2], pos: (origin() + DVec3::new(1.0, 2.0, 3.0)).to_array(), yaw: -0.3, speed: 0.0, health: 100.0, damage: 0.0 }),
            clock: clock(),
        }
    }

    fn plane_state() -> SaveState {
        SaveState {
            version: VERSION,
            mode: SaveMode::Fly,
            foot: None,
            car: None,
            plane: Some(SavedPlane { spec: "c172".into(), pos: (origin() + DVec3::new(20.0, 30.0, 40.0)).to_array(), rot: [0.0, 0.0, 0.0, 1.0], vel: [1.0, 2.0, 3.0], gear: 1.0, flaps: 0.25, throttle: 0.6, fuel: 0.75 }),
            last_car: None,
            clock: clock(),
        }
    }

    /// A world with dry ground by default, plus patches of water, buildings and
    /// unloaded tiles for the safe-point test.
    struct TestWorld {
        water: Option<DVec3>,
        building: Option<DVec3>,
        missing: bool,
    }

    impl WorldQuery for TestWorld {
        fn surface_at(&self, p: DVec3) -> Option<SurfaceHit> {
            if self.missing {
                return None;
            }
            let water = self.water.is_some_and(|w| (p - w).length() < 4.0);
            Some(SurfaceHit { h: 0.0, kind: if water { Surface::Water } else { Surface::Asphalt }, water, road: false, sidewalk: false, bridge: false })
        }
        fn building_push_sphere(&self, p: DVec3, r: f64) -> Option<DVec3> {
            self.building.filter(|b| (p - b).length() < r + 3.0).map(|_| DVec3::Z)
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

    fn temp_path(tag: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("gtw-save-{}-{}-{n}", std::process::id(), tag))
    }

    #[test]
    fn round_trips_each_mode() {
        for state in [foot_state(), car_state(), plane_state()] {
            let data = serde_json::to_vec(&state).unwrap();
            let back: SaveState = serde_json::from_slice(&data).unwrap();
            assert_eq!(back, state);
            assert!(back.sane(), "{:?}", back.mode);
            assert_eq!(back.clock, clock());
        }
        // The positions, yaw, health, damage and fuel survive exactly.
        let car = car_state();
        let c = car.car.as_ref().unwrap();
        assert_eq!(c.model, 3);
        assert_eq!(c.colour, [0.2, 0.4, 0.6]);
        assert_eq!((c.yaw, c.speed, c.health, c.damage), (2.0, 14.5, 41.0, 0.59));
        let plane = plane_state();
        let p = plane.plane.as_ref().unwrap();
        assert_eq!((p.spec.as_str(), p.gear, p.flaps, p.throttle, p.fuel), ("c172", 1.0, 0.25, 0.6, 0.75));
        let f = foot_state();
        assert_eq!(f.foot.as_ref().unwrap().style, 2);
    }

    #[test]
    fn bad_files_fall_back() {
        let base = temp_path("bad");
        std::fs::create_dir_all(&base).unwrap();
        // Missing.
        assert_eq!(SaveState::load_from(&base.join("missing.json")), Err(LoadError::Missing));
        // Empty.
        std::fs::write(base.join("empty.json"), b"").unwrap();
        assert_eq!(SaveState::load_from(&base.join("empty.json")), Err(LoadError::Corrupt));
        // Malformed JSON.
        std::fs::write(base.join("broken.json"), b"{\"version\": 1,").unwrap();
        assert_eq!(SaveState::load_from(&base.join("broken.json")), Err(LoadError::Corrupt));
        // Truncated/partial object with missing required fields.
        std::fs::write(base.join("partial.json"), b"{\"version\": 1}").unwrap();
        assert_eq!(SaveState::load_from(&base.join("partial.json")), Err(LoadError::Corrupt));
        // A future schema.
        let mut future = foot_state();
        future.version = VERSION + 1;
        future.save_to(&base.join("future.json")).unwrap();
        assert_eq!(SaveState::load_from(&base.join("future.json")), Err(LoadError::Unsupported(VERSION + 1)));
        // Non-finite numbers are rejected rather than trusted.
        let mut nan = foot_state();
        nan.foot.as_mut().unwrap().pos[1] = f64::NAN;
        nan.save_to(&base.join("nan.json")).unwrap();
        assert_eq!(SaveState::load_from(&base.join("nan.json")), Err(LoadError::Corrupt));
        // The mode's record must be there.
        let mut empty = foot_state();
        empty.foot = None;
        empty.save_to(&base.join("norecord.json")).unwrap();
        assert_eq!(SaveState::load_from(&base.join("norecord.json")), Err(LoadError::Corrupt));
        // A valid file still loads.
        let good = base.join("good.json");
        foot_state().save_to(&good).unwrap();
        assert_eq!(SaveState::load_from(&good).unwrap(), foot_state());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn atomic_write_keeps_a_valid_file() {
        let base = temp_path("atomic");
        std::fs::create_dir_all(&base).unwrap();
        let p = base.join("save.json");
        foot_state().save_to(&p).unwrap();
        assert_eq!(SaveState::load_from(&p).unwrap(), foot_state());
        // A newer save replaces it whole.
        plane_state().save_to(&p).unwrap();
        assert_eq!(SaveState::load_from(&p).unwrap(), plane_state());
        // A write that fails partway leaves the old valid file in place.
        let tmp = p.with_extension("json.tmp");
        std::fs::create_dir_all(&tmp).unwrap();
        assert!(car_state().save_to(&p).is_err());
        assert_eq!(SaveState::load_from(&p).unwrap(), plane_state());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn safe_position_rejects_water_buildings_and_missing_tiles() {
        let mut world = TestWorld { water: Some(origin()), building: None, missing: false };
        let safe = safe_position(&world, origin()).expect("dry land nearby");
        assert!((safe - origin()).length() >= 5.0, "it moves off the water: {safe:?}");
        assert!(character::standable(&world, safe).is_some());
        // Inside a building.
        world.water = None;
        world.building = Some(origin() + DVec3::new(0.0, 1.0, 0.0));
        let safe = safe_position(&world, origin() + DVec3::new(0.0, 1.0, 0.0)).expect("clear of the building");
        assert!(character::standable(&world, safe).is_some());
        assert!((safe - (origin() + DVec3::new(0.0, 1.0, 0.0))).length() >= 3.0);
        // A missing tile cannot be stood on at all.
        world.building = None;
        world.missing = true;
        assert_eq!(safe_position(&world, origin()), None);
    }

    #[test]
    fn summary_and_snow_codes() {
        let s = foot_state();
        let text = summary(&s);
        assert!(text.starts_with("on foot"), "{text}");
        assert!(text.contains(','), "{text}");
        assert_eq!(snow_mode("always"), Some(snow::Mode::Always));
        assert_eq!(snow_mode("off"), Some(snow::Mode::Off));
        assert_eq!(snow_mode("nonsense"), None);
    }
}
