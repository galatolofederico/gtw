//! Snow lying on the ground: where the terrain, trees and roofs turn white.
//!
//! Level 1 is worked out, not measured. A seasonal snow line (the lowest
//! height snow lies at) is read from a small climatology by latitude and day
//! of the year. When the real weather gives a temperature, the snow line is
//! kept within reach of the freezing level. The shaders then lay the snow on
//! each pixel from its height above that line, its slope (none on cliffs) and
//! the way it faces (lower on the shady side, the pole-facing one).
//! common.wgsl `snow_cover` is this file's [`cover`]: keep the two the same.
//!
//! The climatology is a rough fit, not a dataset. Its summer heights follow
//! the climatic snow line (about 5 km in the tropics, 3 km in the Alps, sea
//! level in the high Arctic; e.g. Ohmura et al. 1992, "Climate at the
//! equilibrium line of glaciers"); its winter heights the usual lower edge of
//! the seasonal snow cover (Alps about 700 m in February, the Belgian
//! Ardennes about 400 m, Scandinavia down to the sea). It knows latitude only:
//! a maritime coast and a continental interior at the same latitude get the
//! same snow line. The southern hemisphere uses the same table half a year
//! later.

/// Where snow lies, a choice in the settings.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    /// from the date, place and weather
    #[default]
    Auto,
    /// never
    Off,
    /// always, as in the depth of winter, wherever the slope holds it
    Always,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Auto, Mode::Off, Mode::Always];

    /// The name in settings.json.
    pub fn code(self) -> &'static str {
        match self {
            Mode::Auto => "auto",
            Mode::Off => "off",
            Mode::Always => "always",
        }
    }

    pub fn parse(s: &str) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| m.code().eq_ignore_ascii_case(s.trim()))
    }
}

// The per-pixel part below runs in the shaders; here it is the reference the
// tests check (and that they hold common.wgsl to). The game calls it for the
// ground under the camera (is there snow to blow about?).
/// Height (m) of the transition from bare ground to full snow cover: the
/// snow thins out over this band around the snow line.
pub const BAND: f64 = 400.0;
/// How far (m) the snow line drops on a steep slope facing the pole, and
/// rises on one facing the equator, per unit of the normal's poleward part.
pub const ASPECT: f64 = 500.0;
/// Cosines of the slopes between which the snow slides off: it holds fully
/// up to 30°, none is left on cliffs from 60°.
pub const HOLD_COS: f64 = 0.866;
pub const SLIDE_COS: f64 = 0.5;
/// The snow line in Always mode: below any ground.
const ALWAYS_LINE: f64 = -1000.0;
/// Standard lapse rate (K/m), for the freezing level from a surface temperature.
const LAPSE: f64 = 0.0065;
/// How far below the freezing level snow can still lie (it fell during a
/// colder spell and has not melted yet), and how far above it the snow line
/// may stay (fresh snow falls a few hundred metres under the freezing level).
const MELT_REACH: f64 = 1200.0;
const FALL_REACH: f64 = 500.0;

/// Latitude (degrees from the equator), snow line in the depth of winter and
/// at the end of summer (m).
const CLIMATE: [(f64, f64, f64); 12] = [
    (0.0, 4700.0, 4900.0),
    (15.0, 4700.0, 5000.0),
    (25.0, 3800.0, 5100.0),
    (32.0, 2200.0, 4800.0),
    (40.0, 1200.0, 3900.0),
    (46.0, 700.0, 3100.0),
    (50.0, 400.0, 2700.0),
    (55.0, 150.0, 2000.0),
    (62.0, -200.0, 1400.0),
    (70.0, -400.0, 800.0),
    (80.0, -500.0, 300.0),
    (90.0, -500.0, 0.0),
];
/// Days of the year (northern hemisphere) with the most snow (the snow line
/// lowest, late February) and the least (highest, early September).
const DEEPEST: f64 = 50.0;
const LEAST: f64 = 245.0;

fn climate(abs_lat: f64) -> (f64, f64) {
    let a = abs_lat.clamp(0.0, 90.0);
    for w in CLIMATE.windows(2) {
        let ((l0, w0, s0), (l1, w1, s1)) = (w[0], w[1]);
        if a <= l1 {
            let t = (a - l0) / (l1 - l0);
            return (w0 + (w1 - w0) * t, s0 + (s1 - s0) * t);
        }
    }
    (CLIMATE[11].1, CLIMATE[11].2)
}

/// How far into the warm season (0 = deepest winter, 1 = least snow) the
/// northern hemisphere is on day `doy` (0 = January 1st). Snow builds up for
/// longer than it melts, so the two halves are not the same length.
pub fn season(doy: f64) -> f64 {
    let d = doy.rem_euclid(365.0);
    let melt = LEAST - DEEPEST;
    if (DEEPEST..=LEAST).contains(&d) {
        0.5 - 0.5 * (std::f64::consts::PI * (d - DEEPEST) / melt).cos()
    } else {
        let since = (d - LEAST).rem_euclid(365.0);
        0.5 + 0.5 * (std::f64::consts::PI * since / (365.0 - melt)).cos()
    }
}

/// The climatological snow line (m above sea level) at latitude `lat`
/// (degrees, + north) on day `doy`.
pub fn climate_line(lat: f64, doy: f64) -> f64 {
    let (winter, summer) = climate(lat.abs());
    // The southern hemisphere's seasons are half a year later.
    let s = season(if lat < 0.0 { doy + 182.5 } else { doy });
    winter + (summer - winter) * s
}

/// The freezing level (m) from a temperature (°C) measured at a height (m).
pub fn freezing_level(temp_c: f64, elev_m: f64) -> f64 {
    elev_m + temp_c / LAPSE
}

/// The snow line (m) for the setting: the climatology, kept within reach of
/// the freezing level when the weather gives one. None: no snow at all.
pub fn snow_line(mode: Mode, lat: f64, doy: f64, freezing: Option<f64>) -> Option<f64> {
    match mode {
        Mode::Off => None,
        Mode::Always => Some(ALWAYS_LINE),
        Mode::Auto => {
            let c = climate_line(lat, doy);
            Some(match freezing {
                Some(f) => c.clamp(f - MELT_REACH, f + FALL_REACH),
                None => c,
            })
        }
    }
}

/// How much snow covers the ground, 0..1, at height `alt` (m) for a snow
/// line `line` (m), on a slope whose normal makes `cos_slope` with the
/// vertical and has `poleward` (-1..1) as its part towards the nearer pole.
/// Same as common.wgsl `snow_cover`.
pub fn cover(alt: f64, cos_slope: f64, poleward: f64, line: f64) -> f64 {
    let z = alt - line + ASPECT * poleward;
    smoothstep(-0.5 * BAND, 0.5 * BAND, z) * smoothstep(SLIDE_COS, HOLD_COS, cos_slope)
}

pub fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What the shaders get (render.rs `Globals::snow`): the snow line under the
/// camera (m), how fast it changes northwards (m per m), which way the pole
/// is (+1 north, -1 south) and whether there is any snow (1) or none (0).
pub fn globals(mode: Mode, lat: f64, doy: f64, freezing: Option<f64>) -> [f32; 4] {
    let Some(line) = snow_line(mode, lat, doy, freezing) else { return [0.0; 4] };
    // The climatology's change over a tenth of a degree either way (the
    // weather's freezing level is one value for the whole region).
    let d = 0.1;
    let grad = if mode == Mode::Auto {
        let n = snow_line(mode, (lat + d).min(90.0), doy, freezing).unwrap_or(line);
        let s = snow_line(mode, (lat - d).max(-90.0), doy, freezing).unwrap_or(line);
        (n - s) / (2.0 * d * 111_195.0)
    } else {
        0.0
    };
    [line as f32, grad as f32, if lat < 0.0 { -1.0 } else { 1.0 }, 1.0]
}

// ---------------------------------------------------------------- falling snow

/// Depth (m) of the melting layer: snow falling through the freezing level
/// has turned to rain this far below it.
const MELT: f64 = 450.0;

/// How much of the precipitation at height `alt` (m) falls as snow, 0..1,
/// for a freezing level `freezing` (m): all of it above the freezing level,
/// none once the flakes have fallen through the melting layer under it, wet
/// snow and sleet in between.
pub fn snow_share(alt: f64, freezing: f64) -> f64 {
    smoothstep(freezing - MELT, freezing - 50.0, alt)
}

/// The freezing level (m) when the weather gives no temperature: a little
/// above the climatological snow line; below the ground in Always mode (the
/// depth of winter).
pub fn freezing_guess(mode: Mode, lat: f64, doy: f64) -> f64 {
    match mode {
        Mode::Always => ALWAYS_LINE,
        _ => climate_line(lat, doy) + 150.0,
    }
}

/// Blowing snow, 0..1: lying snow (`white`, 0..1) lifted by the wind
/// (`wind`, m/s at the surface) from about 9 m/s, a blizzard by 16 m/s
/// (with snow falling, `snowing`, it starts a little sooner); or what the
/// report says (`reported`, METAR BLSN / DRSN) where there is snow to lift.
pub fn blowing(white: f64, snowing: f64, wind: f64, reported: f64) -> f64 {
    let lift = smoothstep(9.0 - 2.0 * snowing, 16.0, wind);
    (white * lift).max(reported * white).clamp(0.0, 1.0)
}

/// Fresh snow settling on the ground while it snows there: how much of the
/// ground around has been whitened, 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fresh {
    pub k: f64,
}

/// Moderate snow (intensity 0.5) whitens the ground in about this long (s);
/// fresh snow fades back to the season's over this long once it stops.
const SETTLE: f64 = 900.0;
const FADE: f64 = 6.0 * 3600.0;

impl Fresh {
    /// Advance `dt` seconds of game time with snow falling on the ground at
    /// `rate` (precipitation intensity 0..1 times its snow share there).
    pub fn step(&mut self, dt: f64, rate: f64) {
        if dt <= 0.0 {
            return;
        }
        if rate > 0.01 {
            self.k = (self.k + dt * rate / (0.5 * SETTLE)).min(1.0);
        } else {
            self.k *= (-dt / FADE).exp();
        }
    }

    /// The snow line it brings: under the ground at `ground` (m), so the
    /// fresh snow lies on everything around.
    pub fn line(ground: f64) -> f64 {
        ground - 0.75 * BAND
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JAN_15: f64 = 14.0;
    const JUL_15: f64 = 195.0;
    const CHAMONIX: f64 = 45.92;
    const FAGNES: f64 = 50.50;

    #[test]
    fn season_runs_from_winter_to_summer() {
        assert!(season(DEEPEST) < 1e-9);
        assert!((season(LEAST) - 1.0).abs() < 1e-9);
        assert!(season(JAN_15) < 0.15);
        assert!(season(JUL_15) > 0.8);
        // Continuous across the new year.
        assert!((season(364.999) - season(0.0)).abs() < 1e-3);
        for d in 0..365 {
            let s = season(d as f64);
            assert!((0.0..=1.0).contains(&s), "day {d}: {s}");
        }
    }

    #[test]
    fn january_has_more_snow_than_july() {
        for lat in [CHAMONIX, FAGNES, 60.0, 35.0] {
            assert!(climate_line(lat, JAN_15) + 1000.0 < climate_line(lat, JUL_15), "lat {lat}");
        }
        // Chamonix (1035 m) is near the snow line in January; Mont Blanc (4808 m) is white all summer.
        let jan = climate_line(CHAMONIX, JAN_15);
        assert!((700.0..1200.0).contains(&jan), "{jan}");
        assert!(cover(4808.0, 1.0, 0.0, climate_line(CHAMONIX, JUL_15)) > 0.99);
        assert!(cover(1035.0, 1.0, 0.0, climate_line(CHAMONIX, JUL_15)) == 0.0);
        // Hautes Fagnes (694 m): partly white in January, bare in July.
        let fag = cover(694.0, 1.0, 0.0, climate_line(FAGNES, JAN_15));
        assert!((0.3..0.95).contains(&fag), "{fag}");
        assert_eq!(cover(694.0, 1.0, 0.0, climate_line(FAGNES, JUL_15)), 0.0);
    }

    #[test]
    fn southern_hemisphere_is_half_a_year_out() {
        assert!((climate_line(-45.0, JUL_15) - climate_line(45.0, JUL_15 + 182.5)).abs() < 1e-6);
        assert!(climate_line(-45.0, JUL_15) < climate_line(-45.0, JAN_15));
    }

    #[test]
    fn snow_line_falls_towards_the_poles() {
        for doy in [JAN_15, JUL_15] {
            let mut prev = f64::INFINITY;
            for lat in (20..=90).step_by(5) {
                let l = climate_line(lat as f64, doy);
                assert!(l <= prev + 1e-9, "lat {lat} day {doy}");
                prev = l;
            }
        }
    }

    #[test]
    fn cover_grows_with_height() {
        let line = 1500.0;
        let mut prev = -1.0;
        for alt in (0..4000).step_by(50) {
            let c = cover(alt as f64, 1.0, 0.0, line);
            assert!(c >= prev && (0.0..=1.0).contains(&c));
            prev = c;
        }
        assert_eq!(cover(line - BAND, 1.0, 0.0, line), 0.0);
        assert!((cover(line, 1.0, 0.0, line) - 0.5).abs() < 1e-9);
        assert_eq!(cover(line + BAND, 1.0, 0.0, line), 1.0);
    }

    #[test]
    fn cliffs_shed_their_snow() {
        let cos = |deg: f64| deg.to_radians().cos();
        assert_eq!(cover(3000.0, cos(0.0), 0.0, 0.0), 1.0);
        assert!(cover(3000.0, cos(30.0), 0.0, 0.0) > 0.999);
        let mid = cover(3000.0, cos(45.0), 0.0, 0.0);
        assert!(mid > 0.1 && mid < 0.9, "{mid}");
        assert!(cover(3000.0, cos(60.0), 0.0, 0.0) < 1e-3);
        assert_eq!(cover(3000.0, cos(70.0), 0.0, 0.0), 0.0);
        assert_eq!(cover(3000.0, cos(90.0), 0.0, 0.0), 0.0);
    }

    #[test]
    fn shady_side_keeps_more_snow() {
        let line = 1500.0;
        let (c, s) = (30f64.to_radians().cos(), 30f64.to_radians().sin());
        let shady = cover(line, c, s, line);
        let flat = cover(line, c, 0.0, line);
        let sunny = cover(line, c, -s, line);
        assert!(shady > flat && flat > sunny, "{shady} {flat} {sunny}");
        // The pole is south in the southern hemisphere.
        assert_eq!(globals(Mode::Auto, -45.0, 0.0, None)[2], -1.0);
        assert_eq!(globals(Mode::Auto, 45.0, 0.0, None)[2], 1.0);
    }

    #[test]
    fn weather_pulls_the_line_to_the_freezing_level() {
        // A mild January day at Geneva (420 m, +12 °C): freezing level near 2300 m.
        let f = freezing_level(12.0, 420.0);
        assert!((f - 2266.0).abs() < 5.0, "{f}");
        let l = snow_line(Mode::Auto, CHAMONIX, JAN_15, Some(f)).unwrap();
        assert!((l - (f - MELT_REACH)).abs() < 1e-9 && l > climate_line(CHAMONIX, JAN_15));
        // A cold snap (-5 °C at 100 m): snow down to the valleys.
        let cold = snow_line(Mode::Auto, FAGNES, JAN_15, Some(freezing_level(-5.0, 100.0))).unwrap();
        assert!(cold < 0.0, "{cold}");
        // Ordinary weather leaves the climatology alone.
        let jul = climate_line(CHAMONIX, JUL_15);
        assert_eq!(snow_line(Mode::Auto, CHAMONIX, JUL_15, Some(jul)).unwrap(), jul);
    }

    #[test]
    fn rain_turns_to_snow_above_the_melting_layer() {
        let f = 1200.0;
        assert_eq!(snow_share(2000.0, f), 1.0);
        assert_eq!(snow_share(f - MELT - 1.0, f), 0.0);
        let mid = snow_share(f - 250.0, f);
        assert!(mid > 0.2 && mid < 0.8, "{mid}");
        let mut prev = 0.0;
        for a in (0..2000).step_by(25) {
            let s = snow_share(a as f64, f);
            assert!(s >= prev);
            prev = s;
        }
        // Hand-set weather: January in the Alps snows at 1500 m, July rains there.
        assert_eq!(snow_share(1500.0, freezing_guess(Mode::Auto, 45.9, 14.0)), 1.0);
        assert_eq!(snow_share(1500.0, freezing_guess(Mode::Auto, 45.9, 195.0)), 0.0);
        assert_eq!(snow_share(0.0, freezing_guess(Mode::Always, 10.0, 195.0)), 1.0);
    }

    #[test]
    fn blizzards_need_snow_and_wind() {
        assert_eq!(blowing(1.0, 0.0, 5.0, 0.0), 0.0);
        assert!(blowing(1.0, 1.0, 12.0, 0.0) > blowing(1.0, 0.0, 12.0, 0.0));
        assert_eq!(blowing(1.0, 1.0, 20.0, 0.0), 1.0);
        // Nothing to lift on bare ground, whatever the wind or the report.
        assert_eq!(blowing(0.0, 1.0, 25.0, 1.0), 0.0);
        assert_eq!(blowing(1.0, 0.0, 3.0, 0.6), 0.6);
    }

    #[test]
    fn fresh_snow_settles_and_fades() {
        let mut f = Fresh::default();
        for _ in 0..60 {
            f.step(10.0, 0.5);
        }
        assert!((f.k - 600.0 / SETTLE).abs() < 1e-9, "{}", f.k);
        for _ in 0..60 {
            f.step(10.0, 0.5);
        }
        assert_eq!(f.k, 1.0);
        f.step(FADE, 0.0);
        assert!((f.k - (-1f64).exp()).abs() < 1e-9);
        f.step(-1.0, 1.0);
        assert!(f.k < 0.4);
        assert!(cover(500.0, 1.0, 0.0, Fresh::line(500.0)) > 0.95);
    }

    #[test]
    fn modes() {
        assert_eq!(snow_line(Mode::Off, CHAMONIX, JAN_15, None), None);
        assert_eq!(globals(Mode::Off, CHAMONIX, JAN_15, None), [0.0; 4]);
        let always = snow_line(Mode::Always, 10.0, JUL_15, Some(5000.0)).unwrap();
        assert_eq!(cover(0.0, 1.0, 0.0, always), 1.0);
        for m in Mode::ALL {
            assert_eq!(Mode::parse(m.code()), Some(m));
        }
        assert_eq!(Mode::parse("nope"), None);
    }

    #[test]
    fn shaders_use_the_same_constants() {
        let wgsl = include_str!("shaders/common.wgsl");
        let get = |name: &str| -> f64 {
            let line = wgsl.lines().find(|l| l.starts_with(&format!("const {name} ="))).unwrap_or_else(|| panic!("{name} missing"));
            line.split('=').nth(1).unwrap().split(';').next().unwrap().trim().parse().unwrap()
        };
        assert_eq!(get("SNOW_BAND"), BAND);
        assert_eq!(get("SNOW_ASPECT"), ASPECT);
        assert_eq!(get("SNOW_HOLD_COS"), HOLD_COS);
        assert_eq!(get("SNOW_SLIDE_COS"), SLIDE_COS);
    }

    #[test]
    fn gradient_points_the_right_way() {
        // In the northern mid-latitudes the line falls northwards.
        let g = globals(Mode::Auto, CHAMONIX, JAN_15, None);
        assert!(g[1] < 0.0);
        // Tens of metres per degree of latitude (111 km) here in January.
        let per_deg = g[1] as f64 * 111_195.0;
        let direct = (climate_line(CHAMONIX + 0.1, JAN_15) - climate_line(CHAMONIX - 0.1, JAN_15)) / 0.2;
        assert!((per_deg - direct).abs() < 0.5, "{per_deg} {direct}");
        assert!((-150.0..-20.0).contains(&per_deg), "{per_deg}");
    }
}
