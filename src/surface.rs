//! Ground surface classes for on-foot and driving gameplay.
//!
//! The renderer's terrain mask already carries the OpenStreetMap ground class
//! (see `sources::ground_mask`); `sources::build_tile` merges it with ESA
//! WorldCover into one byte per tile cell. This module gives those bytes names,
//! maps both sources onto the six gameplay classes, and holds the grip,
//! rolling-resistance and foot-speed tables plus the weather multipliers.

use crate::sources::{GC_PAVED, GC_PITCH, LC_BARE, LC_FARM, LC_GRASS, LC_ROCK, LC_SCRUB, LC_WET, LC_WOOD};

/// A walkable or drivable surface. `Water` is an obstacle, not a class a body
/// can stand on.
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Surface {
    Asphalt = 0,
    Concrete = 1,
    Dirt = 2,
    Grass = 3,
    Sand = 4,
    Rock = 5,
    Water = 6,
}

#[allow(dead_code)]
impl Surface {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Surface::Asphalt,
            1 => Surface::Concrete,
            2 => Surface::Dirt,
            3 => Surface::Grass,
            4 => Surface::Sand,
            5 => Surface::Rock,
            6 => Surface::Water,
            _ => Surface::Grass,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Surface::Asphalt => "asphalt",
            Surface::Concrete => "concrete",
            Surface::Dirt => "dirt",
            Surface::Grass => "grass",
            Surface::Sand => "sand",
            Surface::Rock => "rock",
            Surface::Water => "water",
        }
    }

    /// Tyre grip factor (1.0 = dry asphalt).
    pub fn grip(self) -> f32 {
        match self {
            Surface::Asphalt => 1.00,
            Surface::Concrete => 0.95,
            Surface::Dirt => 0.62,
            Surface::Grass => 0.55,
            Surface::Sand => 0.42,
            Surface::Rock => 0.78,
            Surface::Water => 0.0,
        }
    }

    /// Rolling-resistance coefficient.
    pub fn rolling(self) -> f32 {
        match self {
            Surface::Asphalt => 0.015,
            Surface::Concrete => 0.018,
            Surface::Dirt => 0.045,
            Surface::Grass => 0.060,
            Surface::Sand => 0.110,
            Surface::Rock => 0.030,
            Surface::Water => 0.0,
        }
    }

    /// Multiplier on the character's walk/run/sprint speed.
    pub fn foot_speed(self) -> f32 {
        match self {
            Surface::Asphalt | Surface::Concrete => 1.00,
            Surface::Dirt => 0.82,
            Surface::Grass => 0.78,
            Surface::Sand => 0.58,
            Surface::Rock => 0.72,
            Surface::Water => 0.0,
        }
    }

    pub fn blocked(self) -> bool {
        self == Surface::Water
    }
}

/// What a ground query found at a point.
#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
pub struct SurfaceHit {
    /// metres above the mean sphere (the height the body stands at)
    pub h: f64,
    pub kind: Surface,
    /// water blocks walking and driving
    pub water: bool,
    /// the point is on a road index carriageway
    pub road: bool,
    /// ... and on its sidewalk band rather than the carriageway itself
    pub sidewalk: bool,
    /// the road index hit is a bridge deck
    pub bridge: bool,
}

#[allow(dead_code)]
impl SurfaceHit {
    pub fn blocked(&self) -> bool {
        self.water || self.kind.blocked()
    }
}

/// Weather's effect on surfaces, 0..1 each.
#[allow(dead_code)]
#[derive(Copy, Clone, Debug, Default)]
pub struct SurfaceWeather {
    pub wet: f32,
    pub snow: f32,
    pub ice: f32,
}

#[allow(dead_code)]
impl SurfaceWeather {
    /// Tyre grip multiplier. Wet asphalt loses a quarter of its grip; snow and
    /// ice take theirs from every class.
    pub fn grip_factor(&self, surface: Surface) -> f32 {
        if surface == Surface::Water {
            return 1.0;
        }
        let wet = if surface == Surface::Asphalt { 0.25 * self.wet.clamp(0.0, 1.0) } else { 0.0 };
        (1.0 - wet) * (1.0 - 0.55 * self.snow.clamp(0.0, 1.0)) * (1.0 - 0.80 * self.ice.clamp(0.0, 1.0))
    }

    /// Foot-speed multiplier. Snow and ice slow a walker on every class.
    pub fn foot_factor(&self, surface: Surface) -> f32 {
        if surface == Surface::Water {
            return 1.0;
        }
        (1.0 - 0.30 * self.snow.clamp(0.0, 1.0)) * (1.0 - 0.45 * self.ice.clamp(0.0, 1.0))
    }
}

/// The gameplay class of an OSM ground-mask byte, or None where the map says
/// nothing (`0`).
pub fn from_osm(class: u8) -> Option<Surface> {
    match class {
        GC_PAVED => Some(Surface::Asphalt),
        GC_PITCH => Some(Surface::Grass),
        LC_WOOD | LC_SCRUB | LC_GRASS => Some(Surface::Grass),
        LC_FARM | LC_WET => Some(Surface::Dirt),
        LC_BARE => Some(Surface::Sand),
        LC_ROCK => Some(Surface::Rock),
        _ => None,
    }
}

/// The gameplay class of an ESA WorldCover code, or None for a code this
/// version does not know (the caller falls back to grass).
pub fn from_worldcover(code: u8) -> Option<Surface> {
    match code {
        10 | 20 | 30 | 100 => Some(Surface::Grass),
        40 | 90 | 95 => Some(Surface::Dirt),
        50 => Some(Surface::Concrete),
        60 => Some(Surface::Sand),
        70 => Some(Surface::Rock),
        80 => Some(Surface::Water),
        _ => None,
    }
}

/// Merge the two sources. Precedence, highest first: water, OSM, WorldCover,
/// grass.
pub fn merge(osm_class: u8, water: bool, worldcover: Option<u8>) -> Surface {
    if water {
        return Surface::Water;
    }
    if let Some(s) = from_osm(osm_class) {
        return s;
    }
    worldcover.and_then(from_worldcover).unwrap_or(Surface::Grass)
}

/// Human-readable OSM ground class, for `--surface-probe`.
pub fn osm_name(class: u8) -> &'static str {
    match class {
        0 => "none",
        GC_PAVED => "paved",
        GC_PITCH => "pitch",
        LC_WOOD => "wood",
        LC_SCRUB => "scrub",
        LC_GRASS => "grass",
        LC_FARM => "farmland",
        LC_WET => "wetland",
        LC_BARE => "bare",
        LC_ROCK => "rock",
        _ => "unknown",
    }
}

/// Human-readable ESA WorldCover code, for `--surface-probe`.
pub fn worldcover_name(code: u8) -> &'static str {
    match code {
        10 => "tree cover",
        20 => "shrubland",
        30 => "grassland",
        40 => "cropland",
        50 => "built-up",
        60 => "bare / sparse",
        70 => "snow / ice",
        80 => "permanent water",
        90 => "herbaceous wetland",
        95 => "mangroves",
        100 => "moss / lichen",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_osm_class_maps() {
        assert_eq!(from_osm(GC_PAVED), Some(Surface::Asphalt));
        assert_eq!(from_osm(GC_PITCH), Some(Surface::Grass));
        assert_eq!(from_osm(LC_WOOD), Some(Surface::Grass));
        assert_eq!(from_osm(LC_SCRUB), Some(Surface::Grass));
        assert_eq!(from_osm(LC_GRASS), Some(Surface::Grass));
        assert_eq!(from_osm(LC_FARM), Some(Surface::Dirt));
        assert_eq!(from_osm(LC_WET), Some(Surface::Dirt));
        assert_eq!(from_osm(LC_BARE), Some(Surface::Sand));
        assert_eq!(from_osm(LC_ROCK), Some(Surface::Rock));
        assert_eq!(from_osm(0), None);
        assert_eq!(from_osm(200), None);
    }

    #[test]
    fn every_worldcover_code_maps() {
        let want = [
            (10, Surface::Grass),
            (20, Surface::Grass),
            (30, Surface::Grass),
            (40, Surface::Dirt),
            (50, Surface::Concrete),
            (60, Surface::Sand),
            (70, Surface::Rock),
            (80, Surface::Water),
            (90, Surface::Dirt),
            (95, Surface::Dirt),
            (100, Surface::Grass),
        ];
        for (code, surface) in want {
            assert_eq!(from_worldcover(code), Some(surface), "WorldCover {code}");
            assert_eq!(worldcover_name(code) == "unknown", false, "WorldCover {code} name");
        }
        assert_eq!(from_worldcover(0), None);
        assert_eq!(from_worldcover(15), None);
        assert_eq!(worldcover_name(15), "unknown");
    }

    #[test]
    fn water_precedes_both_sources() {
        assert_eq!(merge(GC_PAVED, true, Some(10)), Surface::Water);
        assert_eq!(merge(0, true, Some(10)), Surface::Water);
        assert_eq!(merge(0, false, Some(80)), Surface::Water);
        assert_eq!(merge(GC_PAVED, false, Some(80)), Surface::Asphalt);
        assert_eq!(merge(0, false, Some(50)), Surface::Concrete);
        assert_eq!(merge(LC_FARM, false, Some(50)), Surface::Dirt);
        assert_eq!(merge(0, false, None), Surface::Grass);
        assert_eq!(merge(0, false, Some(250)), Surface::Grass);
    }

    #[test]
    fn grip_rolling_and_foot_tables() {
        let table = [
            (Surface::Asphalt, 1.00, 0.015, 1.00),
            (Surface::Concrete, 0.95, 0.018, 1.00),
            (Surface::Dirt, 0.62, 0.045, 0.82),
            (Surface::Grass, 0.55, 0.060, 0.78),
            (Surface::Sand, 0.42, 0.110, 0.58),
            (Surface::Rock, 0.78, 0.030, 0.72),
        ];
        for (surface, grip, rolling, foot) in table {
            assert!((surface.grip() - grip).abs() < 1e-6, "{surface:?} grip");
            assert!((surface.rolling() - rolling).abs() < 1e-6, "{surface:?} rolling");
            assert!((surface.foot_speed() - foot).abs() < 1e-6, "{surface:?} foot");
            assert!(!surface.blocked());
        }
        assert!(Surface::Water.blocked());
    }

    #[test]
    fn weather_multipliers() {
        let wet = SurfaceWeather { wet: 1.0, snow: 0.0, ice: 0.0 };
        assert!((wet.grip_factor(Surface::Asphalt) - 0.75).abs() < 1e-6);
        assert!((wet.grip_factor(Surface::Grass) - 1.0).abs() < 1e-6);
        assert!((wet.foot_factor(Surface::Asphalt) - 1.0).abs() < 1e-6);

        let snow = SurfaceWeather { wet: 0.0, snow: 1.0, ice: 0.0 };
        assert!((snow.grip_factor(Surface::Asphalt) - 0.45).abs() < 1e-6);
        assert!((snow.foot_factor(Surface::Grass) - 0.70).abs() < 1e-6);

        let ice = SurfaceWeather { wet: 0.0, snow: 0.0, ice: 1.0 };
        assert!((ice.grip_factor(Surface::Concrete) - 0.20).abs() < 1e-6);
        assert!((ice.foot_factor(Surface::Rock) - 0.55).abs() < 1e-6);

        // Weather never makes water walkable.
        let all = SurfaceWeather { wet: 1.0, snow: 1.0, ice: 1.0 };
        assert_eq!(all.grip_factor(Surface::Water), 1.0);
        assert_eq!(all.foot_factor(Surface::Water), 1.0);
    }

    #[test]
    fn hit_reports_blocks_and_names() {
        let hit = SurfaceHit { h: 1.0, kind: Surface::Water, water: true, road: false, sidewalk: false, bridge: false };
        assert!(hit.blocked());
        assert_eq!(Surface::from_u8(2), Surface::Dirt);
        assert_eq!(Surface::from_u8(255), Surface::Grass);
        for s in [Surface::Asphalt, Surface::Concrete, Surface::Dirt, Surface::Grass, Surface::Sand, Surface::Rock, Surface::Water] {
            assert_eq!(Surface::from_u8(s as u8), s);
            assert_ne!(s.name(), "");
        }
    }
}
