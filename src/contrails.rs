//! Intelligent, physically grounded contrail simulation and adaptive-LOD ribbon
//! builder for the player's aircraft, other players,
//! and multiplayer aircraft.
//!
//! Formation follows the Schmidt–Appleman criterion (Schumann 1996) using the
//! USSA 1976 atmosphere adjusted for latitude, season, and live freezing level.
//! Persistence and spreading follow upper-tropospheric relative humidity with
//! respect to ice (RHi), driven by the live/hand-set weather state (mid-level
//! cloud `alto`, cloud cover, storms, rain deck, haze, live surface humidity)
//! combined with a planet-wide synoptic ice-supersaturated region (ISSR) field
//! drifting with `Weather::cloud_off`:
//!
//! - Dry upper air (`RHi < 1.0`): short-lived trails that sublimate cleanly
//!   over 15–40 s.
//! - Lightly ice-supersaturated (`RHi ~ 1.0–1.18`): persistent trails that
//!   streak across the sky for several minutes while drifting with wind aloft.
//! - Strongly ice-supersaturated (`RHi > 1.18`): contrail cirrus that grows,
//!   diffuses wide, and wisps away asymmetrically along the wind-shear vector.
//!
//! Resolution adapts automatically with distance: high vertex density right
//! behind the player's engines (capturing tight fighter maneuvers), progressive
//! chord-error decimation as trails age and recede, and merged single-ribbon
//! trails for other players. All visible segments draw in one instanced pass
//! (`src/shaders/contrails.wgsl`).

use crate::aircraft::{Engine, Spec};
use crate::airframe::{G_ENGINE, G_FUSE, G_LWING, G_RWING, G_TAIL};
use crate::geo;
use crate::mp::Remote;
use crate::plane::{self, Plane};
use crate::snow;
use crate::weather::Weather;
use glam::{DVec3, Mat4, Vec3, Vec4};
use std::collections::HashMap;

/// Hard GPU caps so contrails never blow past the ~1 ms frame-time budget.
pub const MAX_NODES: usize = 8192;
pub const MAX_SEGS: usize = 6144;

/// Maximum distance from camera (m) to track/draw other players' contrails.
const MAX_TRAFFIC_DIST: f64 = 145_000.0;
/// Distance (m) beyond which multi-engine aircraft merge into one thicker trail.
const TRAFFIC_MERGE_DIST: f64 = 5_500.0;
/// Maximum distance (m) from camera to draw any contrail segment.
const MAX_DRAW_DIST: f64 = 200_000.0;

/// GPU node uploaded to `contrails.wgsl` (64 bytes, 16-byte aligned).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuNode {
    /// xyz camera-relative position (m), w physical half-width radius (m)
    pub pos_r: [f32; 4],
    /// xyz unit tangent along trail, w monotonic arc-length s (m)
    pub tan_s: [f32; 4],
    /// xyz horizontal wind-shear unit vector (planet frame), w downwind stretch (0..3.5)
    pub shear: [f32; 4],
    /// x optical density, y age fraction (0..1), z supersat/spread (0..1), w trail seed
    pub props: [f32; 4],
}

/// GPU segment pair indexing two adjacent `GpuNode`s (8 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuSeg {
    pub a: u32,
    pub b: u32,
}

/// Frame upload handed to `render::Frame`.
#[derive(Default)]
pub struct DrawData {
    pub nodes: Vec<GpuNode>,
    pub segs: Vec<GpuSeg>,
}

/// Atmospheric conditions governing contrail formation and evolution at a point.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct ContrailEnv {
    /// Ambient temperature (K) and pressure (Pa).
    pub temp_k: f64,
    pub pressure_pa: f64,
    /// Relative humidity with respect to ice (0..1.5) and liquid water (0..1).
    pub rhi: f64,
    pub rhw: f64,
    /// Schmidt–Appleman critical temperature (K) for this propulsion type.
    pub t_sac: f64,
    /// Smooth 0..1 formation strength (> 0 means contrails form).
    pub strength: f32,
    /// 0..1 ice-supersaturation regime: 0 = short-lived, ~0.5 = persistent streak, 1 = spreading cirrus.
    pub supersat: f32,
    /// Expected contrail lifetime before sublimation/dispersion (s).
    pub life: f32,
    /// Lateral diffusion speed (m/s) during dispersion.
    pub spread_rate: f32,
    /// Upper-level wind vector (planet frame, m/s) carrying the contrail.
    pub wind: DVec3,
    /// Horizontal wind-shear unit direction (planet frame) and shear strength (0..1).
    pub shear_dir: Vec3,
    pub shear_strength: f32,
}

/// Saturation vapor pressure over liquid water (Pa), Magnus–Tetens fit for T in Kelvin.
#[inline]
pub fn es_water(t_k: f64) -> f64 {
    let tc = (t_k - 273.15).clamp(-95.0, 60.0);
    611.21 * (17.502 * tc / (tc + 240.97)).exp()
}

/// Derivative d(es_water)/dT (Pa/K) for T in Kelvin.
#[inline]
fn des_water_dt(t_k: f64) -> f64 {
    let tc = (t_k - 273.15).clamp(-95.0, 60.0);
    let denom = tc + 240.97;
    es_water(t_k) * (17.502 * 240.97) / (denom * denom)
}

/// Saturation vapor pressure over ice (Pa), Magnus–Tetens fit for T in Kelvin.
#[inline]
pub fn es_ice(t_k: f64) -> f64 {
    let tc = (t_k - 273.15).clamp(-105.0, 5.0);
    611.15 * (22.452 * tc / (tc + 272.55)).exp()
}

/// Schmidt–Appleman mixing-line slope G (Pa/K):
/// `G = EI_H2O * c_p * p / (eps * Q * (1 - eta))`.
#[inline]
pub fn sac_slope(pressure_pa: f64, eta: f64, ei_h2o: f64) -> f64 {
    const CP: f64 = 1004.5;
    const EPS: f64 = 0.622;
    const Q_JET: f64 = 43.2e6;
    ei_h2o * CP * pressure_pa / (EPS * Q_JET * (1.0 - eta).max(0.15))
}

/// Schmidt–Appleman threshold ambient temperature `T_sac` (K) given mixing slope `g` (Pa/K)
/// and ambient relative humidity over liquid water `rhw` (0..1).
pub fn sac_threshold_k(g: f64, rhw: f64) -> f64 {
    if g <= 0.055 {
        return 160.0;
    }
    let rhw = rhw.clamp(0.0, 1.0);
    // Schumann (1996) initial guess for T_m where de_w/dT = G.
    let lg = (g - 0.053).ln();
    let mut tm = 273.15 + (-46.46 + 9.43 * lg + 0.720 * lg * lg).clamp(-85.0, -5.0);
    // Refine T_m by 3 Newton steps on ln(de_w/dT) = ln(G).
    let target_ln = g.ln();
    for _ in 0..3 {
        let tc = tm - 273.15;
        let denom = tc + 240.97;
        let f = des_water_dt(tm).ln() - target_ln;
        let df = (17.502 * 240.97) / (denom * denom) - 2.0 / denom;
        tm = (tm - f / df.max(1e-4)).clamp(185.0, 270.0);
    }
    if rhw >= 0.999 {
        return tm;
    }
    let em = es_water(tm);
    // Solve e_w(tm) - rhw * e_w(t) - g * (tm - t) = 0 for t <= tm.
    let mut t = (tm - (1.0 - rhw) * em / g).clamp(170.0, tm);
    for _ in 0..4 {
        let f = em - rhw * es_water(t) - g * (tm - t);
        let df = g - rhw * des_water_dt(t);
        if df.abs() < 1e-6 {
            break;
        }
        let step = (f / df).clamp(-15.0, 15.0);
        t = (t - step).clamp(165.0, tm);
        if step.abs() < 1e-3 {
            break;
        }
    }
    t
}

#[inline]
fn shash(x: u32) -> u32 {
    let h = x.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let h = ((h >> ((h >> 28) + 4)) ^ h).wrapping_mul(277_803_737);
    (h >> 22) ^ h
}

#[inline]
fn unit(h: u32) -> f64 {
    (h & 0xffff) as f64 / 65535.0
}

#[inline]
fn smoothstep(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smooth 0..1 3D lattice value noise (same family as `weather.rs`).
fn value_noise(p: DVec3) -> f64 {
    let c = p.floor();
    let f = p - c;
    let f = f * f * (DVec3::splat(3.0) - 2.0 * f);
    let at = |i: i32, j: i32, k: i32| {
        let x = c.x as i32 + i;
        let y = c.y as i32 + j;
        let z = c.z as i32 + k;
        unit(shash(
            (x as u32).wrapping_mul(73_856_093)
                ^ (y as u32).wrapping_mul(19_349_663)
                ^ (z as u32).wrapping_mul(83_492_791),
        ))
    };
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(at(0, 0, 0), at(1, 0, 0), f.x);
    let x10 = lerp(at(0, 1, 0), at(1, 1, 0), f.x);
    let x01 = lerp(at(0, 0, 1), at(1, 0, 1), f.x);
    let x11 = lerp(at(0, 1, 1), at(1, 1, 1), f.x);
    lerp(lerp(x00, x10, f.y), lerp(x01, x11, f.y), f.z)
}

/// Upper-air wind vector (planet frame, m/s) at `pos` and geometric altitude `alt_m`.
/// Scales smoothly from `Weather::steady` near the ground up to stronger jet-stream
/// winds at cruise altitude (~9–12 km), with gentle Coriolis veering aloft.
pub fn wind_aloft(wx: &Weather, pos: DVec3, alt_m: f64) -> DVec3 {
    if wx.wind <= 0.01 {
        return DVec3::ZERO;
    }
    let (e, n, _) = geo::enu(pos);
    let (lat, _, _) = geo::pos_to_lla(pos);
    // Subtle realistic thermal-wind veering aloft (clockwise in NH, CCW in SH).
    let veer_deg = 12.0 * smoothstep(2000.0, 10000.0, alt_m) * lat.signum();
    let to = (wx.wind_from + 180.0 + veer_deg).to_radians();
    let dir = e * to.sin() + n * to.cos();
    // Power-law profile up to ~1 km, then jet-stream shear up to tropopause (~11 km),
    // easing slightly in the lower stratosphere above 13 km.
    let low_mult = (alt_m.max(2.0) / 10.0).powf(1.0 / 7.0).min(2.0);
    let jet = 1.0 + 0.75 * smoothstep(3500.0, 10500.0, alt_m) * (1.0 - 0.35 * smoothstep(12500.0, 19000.0, alt_m));
    dir * (wx.wind * low_mult * jet)
}

/// Evaluate local atmospheric temperature, upper-tropospheric ice humidity (RHi),
/// Schmidt–Appleman formation strength, persistence regime, and wind/shear at `pos`.
pub fn atmosphere_at(pos: DVec3, wx: &Weather, doy: f64, eta: f64, ei_h2o: f64) -> ContrailEnv {
    let (lat_rad, _, alt_m) = geo::pos_to_lla(pos);
    let lat_deg = lat_rad.to_degrees();
    let abs_lat = lat_deg.abs();

    // 1. Ambient temperature & pressure: USSA 1976 adjusted for latitude, season,
    //    and live weather freezing level.
    let (t_isa, pressure_pa) = plane::atmosphere(alt_m);
    let s_doy = if lat_deg < 0.0 { (doy + 182.5).rem_euclid(365.0) } else { doy };
    let warm_season = snow::season(s_doy); // 0 winter .. 1 summer
    // Tropopause sits higher in the tropics (~15.5 km) and lower near the poles (~8.5 km).
    let tropopause = 15_500.0 - 7_000.0 * smoothstep(12.0, 72.0, abs_lat) + 1_100.0 * (warm_season - 0.5);
    // Climatological surface temperature anomaly relative to ISA (+15 °C).
    let clim_sfc_c = 28.0 - 44.0 * smoothstep(5.0, 78.0, abs_lat) + (12.0 * smoothstep(15.0, 65.0, abs_lat)) * (warm_season * 2.0 - 1.0);
    let mut dt_sfc = (clim_sfc_c - 15.0).clamp(-32.0, 16.0);
    if let Some(frz) = wx.freezing {
        // Live METAR / Open-Meteo freezing level (ISA freezing level is ~2308 m).
        let live_dt = ((frz - 2307.7) * 0.0065).clamp(-48.0, 18.0);
        dt_sfc = live_dt;
    }
    // Surface anomaly attenuates smoothly toward the tropopause; in winter polar air
    // the whole troposphere is noticeably colder, allowing lower contrails.
    let anom_fade = 1.0 - 0.62 * smoothstep(2000.0, tropopause, alt_m);
    // Tropical upper troposphere continues cooling above 11 km up to its higher tropopause.
    let trop_extra = if alt_m > 11_000.0 && tropopause > 11_000.0 {
        -0.0042 * (alt_m.min(tropopause) - 11_000.0)
    } else {
        0.0
    };
    let temp_k = (t_isa + dt_sfc * anom_fade + trop_extra).clamp(180.0, 330.0);

    // 2. Upper-tropospheric relative humidity with respect to ice (RHi).
    //    Driven by game weather + large-scale synoptic ISSR patches drifting with cloud_off + day-of-year.
    let cloud_p = pos.normalize() * 6371.0 + wx.cloud_off / 1000.0;
    let day_shift = DVec3::new((doy * 0.73).sin() * 11.3, (doy * 0.41).cos() * 7.9, doy * 0.19);
    // Coarse synoptic (~380 km) + mesoscale (~140 km) ice-supersaturated regions.
    let issr_big = value_noise(cloud_p / 380.0 + day_shift);
    let issr_mid = value_noise(cloud_p / 140.0 + day_shift * 1.7 + DVec3::new(3.1, 7.4, 1.9));
    let issr = issr_big * 0.68 + issr_mid * 0.32; // ~0.15..0.85 centered ~0.5

    // Weather moisture signal:
    // - `alto` (mid/upper cloudiness) is the clearest indicator of moist upper air.
    // - `storms` pump moisture right into the upper troposphere / anvil layer.
    // - `clouds`, `deck` (rain deck), `haze`, and live `humidity` all add realistic moisture.
    let wx_moist = (wx.alto as f64) * 0.55
        + (wx.storms as f64) * 0.35
        + (wx.clouds as f64) * 0.26
        + (wx.deck as f64) * 0.18
        + ((wx.haze as f64 - 1.0).clamp(0.0, 8.0) / 8.0) * 0.08
        + wx.humidity.map_or(0.0, |rh| (rh as f64 - 0.5) * 0.20);

    // Altitude moisture envelope: peaks in the upper troposphere (~6.5 km to tropopause + 1 km),
    // falls off sharply in the dry stratosphere above the tropopause.
    let strat_dry = 1.0 - smoothstep(tropopause + 800.0, tropopause + 4200.0, alt_m);
    let upper_band = 0.72 + 0.28 * smoothstep(4500.0, 8200.0, alt_m);

    // Base RHi combines weather moisture and synoptic ISSR patches so:
    // - Clear dry days (alto=0, clouds=0.05) -> ~0.37..0.85 (short-lived trails)
    // - Fair days (alto=0.25, clouds=0.45) -> ~0.68..1.17 (short-lived to long sky-streaks)
    // - Overcast/Stormy/Moist days (alto=0.5..0.85) -> ~1.05..1.48 (wide diffusing wispy cirrus)
    let raw_rhi = 0.60 + wx_moist + (issr - 0.5) * 0.58;
    let rhi = (raw_rhi * upper_band * strat_dry + 0.08 * (1.0 - strat_dry)).clamp(0.05, 1.48);

    // Convert RHi to liquid-water RHw for the Schmidt–Appleman liquid saturation check.
    let ew = es_water(temp_k);
    let ei = es_ice(temp_k);
    let rhw = (rhi * ei / ew.max(1e-6)).clamp(0.01, 0.99);

    // 3. Schmidt–Appleman criterion.
    let g = sac_slope(pressure_pa, eta, ei_h2o);
    let t_sac = sac_threshold_k(g, rhw);
    let margin = t_sac - temp_k;
    // Smooth onset over ~4 K margin; also fade out in extreme upper stratosphere (p < 45 hPa).
    let press_ok = smoothstep(3_000.0, 7_500.0, pressure_pa);
    let strength = (smoothstep(-0.6, 3.8, margin) * press_ok) as f32;

    // 4. Wind aloft & vertical wind shear.
    let wind = wind_aloft(wx, pos, alt_m);
    let wind_spd = wind.length();
    let (e, n, _) = geo::enu(pos);
    // Horizontal shear direction: mostly downwind with a slight cross-wind offset from
    // thermal-wind turning & synoptic waves, so spreading cirrus wisps downwind.
    let shear_vec = if wind_spd > 0.25 {
        let w_unit = wind / wind_spd;
        let perp = pos.normalize().cross(w_unit);
        let twist = ((issr_mid - 0.5) * 0.45 + 0.14 * lat_deg.signum()).clamp(-0.5, 0.5);
        (w_unit + perp * twist).normalize()
    } else {
        let a = (wx.wind_from + 180.0).to_radians();
        (e * a.sin() + n * a.cos()).normalize()
    };
    let shear_strength = ((wind_spd / 28.0) * 0.72 + wx.gusts * 0.38).clamp(0.0, 1.0) as f32;

    // 5. Three physical regimes from RHi:
    //    - rhi < 0.96: short-lived, 16..42 s, low spread
    //    - 0.96..1.16: persistent coherent sky-streak, 90..310 s, modest spread
    //    - rhi > 1.16: spreading contrail cirrus, 260..540 s, high lateral diffusion + wind shear wispiness
    let supersat = smoothstep(0.92, 1.36, rhi) as f32;
    let short_life = 14.0 + 28.0 * smoothstep(0.35, 0.96, rhi);
    let persist_life = 85.0 + 230.0 * smoothstep(0.95, 1.18, rhi);
    let cirrus_life = 280.0 + 260.0 * smoothstep(1.15, 1.44, rhi);
    let life = if rhi < 0.96 {
        short_life
    } else if rhi < 1.16 {
        short_life.max(persist_life)
    } else {
        persist_life.max(cirrus_life)
    } as f32;

    // Lateral spread rate (m/s): gentle for dry/tight streaks, strong in supersaturated sheared air.
    let spread_base = 0.25 + 0.45 * smoothstep(0.85, 1.12, rhi);
    let spread_cirrus = 2.6 * smoothstep(1.10, 1.40, rhi) * (0.55 + 0.95 * shear_strength as f64);
    let spread_rate = (spread_base + spread_cirrus) as f32;

    ContrailEnv {
        temp_k,
        pressure_pa,
        rhi,
        rhw,
        t_sac,
        strength,
        supersat,
        life,
        spread_rate,
        wind,
        shear_dir: shear_vec.as_vec3(),
        shear_strength,
    }
}

/// One engine exhaust mount in the aircraft body frame (+X right, +Y up, -Z forward).
#[derive(Clone, Copy, Debug)]
pub struct EngineMount {
    pub pos: Vec3,
    /// Relative initial plume scale (~0.45 piston .. 1.0 fighter .. 2.6 B747).
    pub scale: f32,
    /// Overall propulsion efficiency (higher = higher SAC slope = forms slightly warmer).
    pub eta: f64,
    /// Water-vapor emission index (kg H2O / kg fuel; Jet-A ~ 1.25, avgas ~ 1.4, methalox ~ 2.25).
    pub ei_h2o: f64,
    /// Airframe damage group (`G_LWING`, `G_RWING`, `G_FUSE`, `G_TAIL`, `G_ENGINE`).
    pub group: usize,
}

/// Exact engine exhaust locations and physical scaling for any aircraft `Spec`.
pub fn spec_engines(spec: &Spec, stage: usize) -> Vec<EngineMount> {
    let key = spec.keys.first().copied().unwrap_or("");
    match &spec.engine {
        Engine::Rocket(rk) => {
            let st = &rk.stages[stage.min(1)];
            vec![EngineMount {
                pos: Vec3::new(0.0, 0.0, st.z[1] as f32),
                scale: 5.0,
                eta: 0.35,
                ei_h2o: 2.25,
                group: G_FUSE,
            }]
        }
        Engine::Piston(pe) => {
            let scale = ((pe.power / 135_000.0).powf(0.35) * 0.48).clamp(0.32, 0.75) as f32;
            vec![EngineMount {
                pos: Vec3::new(0.0, -0.55, -1.2),
                scale,
                eta: 0.22,
                ei_h2o: 1.40,
                group: G_ENGINE,
            }]
        }
        Engine::Fan(fe) => {
            // Physical scale: thrust per engine + wingspan (wake vortex circulation).
            // B747 (252 kN, 64 m) -> ~2.55, Concorde (140 kN, 25.6 m) -> ~1.70,
            // G650 (75 kN, 30.4 m) -> ~1.40, F-22 (116 kN, 13.6 m) -> ~1.30,
            // F/A-18 (44 kN, 11.8 m) -> ~0.92.
            let t_scale = (fe.thrust / 80_000.0).powf(0.36);
            let b_scale = (spec.b / 26.0).powf(0.28);
            let scale = (0.78 * t_scale * b_scale).clamp(0.42, 1.95) as f32;
            // High-bypass turbofans have higher overall efficiency (~0.36) than low-bypass fighters (~0.28).
            let eta = if fe.boost >= 1.2 { 0.28 } else { 0.36 };
            let ei_h2o = 1.25;

            if key == "747" || key == "b747" {
                // Four PW4056/CF6 wing pods at exact nacelle nozzle exits (b747/mod.rs).
                let zn = crate::aircraft::b747::ZN;
                let yg = crate::aircraft::b747::YG;
                let mut out = Vec::with_capacity(4);
                for e in crate::aircraft::b747::ENGINES {
                    let (x, y, z) = (e[0], e[1] - yg, e[2] + 6.0 - zn);
                    out.push(EngineMount { pos: Vec3::new(-x, y, z), scale, eta, ei_h2o, group: G_LWING });
                    out.push(EngineMount { pos: Vec3::new(x, y, z), scale, eta, ei_h2o, group: G_RWING });
                }
                return out;
            }
            if key == "concorde" {
                let z = fe.nozzle_z as f32;
                return vec![
                    EngineMount { pos: Vec3::new(-4.83, -1.92, z), scale: scale * 0.92, eta: 0.32, ei_h2o, group: G_LWING },
                    EngineMount { pos: Vec3::new(-6.03, -1.92, z), scale: scale * 0.92, eta: 0.32, ei_h2o, group: G_LWING },
                    EngineMount { pos: Vec3::new(4.83, -1.92, z), scale: scale * 0.92, eta: 0.32, ei_h2o, group: G_RWING },
                    EngineMount { pos: Vec3::new(6.03, -1.92, z), scale: scale * 0.92, eta: 0.32, ei_h2o, group: G_RWING },
                ];
            }
            if key == "g650" {
                let z = fe.nozzle_z as f32;
                let y = crate::aircraft::g650::by(3.75);
                return vec![
                    EngineMount { pos: Vec3::new(-2.20, y, z), scale, eta, ei_h2o, group: G_FUSE },
                    EngineMount { pos: Vec3::new(2.20, y, z), scale, eta, ei_h2o, group: G_FUSE },
                ];
            }
            if key == "sr71" {
                let z = fe.nozzle_z as f32;
                return vec![
                    EngineMount { pos: Vec3::new(-4.12, -0.10, z), scale, eta: 0.30, ei_h2o, group: G_LWING },
                    EngineMount { pos: Vec3::new(4.12, -0.10, z), scale, eta: 0.30, ei_h2o, group: G_RWING },
                ];
            }
            if key == "f22" {
                let z = fe.nozzle_z as f32;
                return vec![
                    EngineMount { pos: Vec3::new(-0.78, -0.10, z), scale, eta, ei_h2o, group: G_TAIL },
                    EngineMount { pos: Vec3::new(0.78, -0.10, z), scale, eta, ei_h2o, group: G_TAIL },
                ];
            }
            if key == "f4" {
                let z = fe.nozzle_z as f32;
                return vec![
                    EngineMount { pos: Vec3::new(-0.50, -0.35, z), scale, eta, ei_h2o, group: G_FUSE },
                    EngineMount { pos: Vec3::new(0.50, -0.35, z), scale, eta, ei_h2o, group: G_FUSE },
                ];
            }
            if key == "fa18" {
                let z = fe.nozzle_z as f32;
                return vec![
                    EngineMount { pos: Vec3::new(-0.52, -0.15, z), scale, eta, ei_h2o, group: G_TAIL },
                    EngineMount { pos: Vec3::new(0.52, -0.15, z), scale, eta, ei_h2o, group: G_TAIL },
                ];
            }
            // Generic / downloaded JSBSim jets: infer from hole.pods or engine count.
            let z = if fe.nozzle_z.abs() > 0.1 { fe.nozzle_z as f32 } else { (spec.tail.break_z[0] * 0.7).max(2.0) };
            let mut out = Vec::new();
            for pod in spec.hole.pods {
                if pod[2] > 0.05 {
                    out.push(EngineMount { pos: Vec3::new(-pod[0], pod[1], z), scale, eta, ei_h2o, group: G_LWING });
                    out.push(EngineMount { pos: Vec3::new(pod[0], pod[1], z), scale, eta, ei_h2o, group: G_RWING });
                }
            }
            if out.is_empty() {
                if fe.n <= 1 {
                    out.push(EngineMount { pos: Vec3::new(0.0, 0.0, z), scale, eta, ei_h2o, group: G_TAIL });
                } else {
                    let x = (spec.b as f32 * 0.14).clamp(0.6, 8.0);
                    out.push(EngineMount { pos: Vec3::new(-x, -0.3, z), scale, eta, ei_h2o, group: G_FUSE });
                    out.push(EngineMount { pos: Vec3::new(x, -0.3, z), scale, eta, ei_h2o, group: G_FUSE });
                }
            }
            out
        }
    }
}

/// Identifies a single contrail ribbon across frames.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum TrailKey {
    Player(u8),
    Mp(u32, u8),
}

#[derive(Clone, Copy, Debug)]
struct Node {
    pos: DVec3,
    age: f32,
    life: f32,
    /// Initial plume radius at emission (m).
    radius0: f32,
    /// Lateral diffusion speed (m/s).
    spread_rate: f32,
    /// Downwind wind-shear unit direction (planet frame).
    shear_dir: Vec3,
    /// Shear strength 0..1.
    shear_strength: f32,
    /// Initial optical density (scales with aircraft size, power, and SAC margin).
    density0: f32,
    /// Ice-supersaturation factor 0..1.
    supersat: f32,
    /// Monotonic arc-length along the trail (m), locking 3D noise to air parcels.
    s_coord: f32,
    /// Soft fade-in factor near the formation gap behind the nozzle (0..1).
    head_fade: f32,
}

impl Node {
    /// Current physical half-width radius (m) after jet, wake-vortex, and dispersion growth.
    #[inline]
    fn current_radius(&self) -> f32 {
        // Phase 1 (jet + wake vortex rollup): starts tight and distinct right behind each engine
        // (so a 747 shows 4 crisp separate plumes), then billows out into vortex puffs over 3..25 s.
        let jet = self.radius0 * (0.90 + 0.75 * (self.age / 1.2).min(1.0));
        let vortex = self.radius0 * 4.8 * (self.age / (14.0 + self.age * 0.22)).min(2.8) * smoothstep_f32(0.3, 5.0, self.age);
        // Phase 2 (turbulent + wind-shear diffusion): widens dramatically in supersaturated air.
        let diff_t = (self.age - 3.0).max(0.0);
        let diff = self.spread_rate * diff_t.powf(0.95) * (0.55 + 0.45 * (self.radius0 / 1.1).clamp(0.45, 2.0));
        (jet + vortex + diff).min(600.0)
    }

    /// Current optical density (accounting for ice-deposition thickening in supersaturated air
    /// and smooth sublimation/dispersion fade at both ends).
    #[inline]
    fn current_density(&self) -> f32 {
        let k = (self.age / self.life.max(1.0)).clamp(0.0, 1.0);
        // Smooth fade-out at end of life (head fade-in is handled spatially in `build_draw`).
        let fade_out = 1.0 - smoothstep_f32(0.50 - 0.14 * self.supersat, 1.0, k);
        // In supersaturated air, ice crystals grow by vapor deposition before gradually thinning out
        // as the ribbon spreads into a wide cirrus veil.
        let growth = 1.0 + 0.42 * self.supersat * smoothstep_f32(0.04, 0.24, k) * (1.0 - 0.48 * smoothstep_f32(0.35, 0.95, k));
        let dilution = 1.0 / (1.0 + 0.0022 * self.spread_rate * self.age * (1.0 - 0.55 * self.supersat));
        (self.density0 * self.head_fade * fade_out * growth * dilution).clamp(0.0, 1.5)
    }

    /// Downwind shear stretch multiplier (0 = symmetric, up to ~3.4 = wide wind-sheared cirrus wisp).
    #[inline]
    fn shear_stretch(&self) -> f32 {
        let k = (self.age / self.life.max(1.0)).clamp(0.0, 1.0);
        let mature = smoothstep_f32(0.02, 0.32, k) * smoothstep_f32(2.5, 14.0, self.age);
        (0.45 + 2.85 * self.supersat) * self.shear_strength * mature
    }
}

#[inline]
fn smoothstep_f32(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

struct Trail {
    nodes: Vec<Node>,
    /// True while an aircraft is actively emitting at the head of this trail.
    active: bool,
    /// True for the player's own trails (highest resolution & priority).
    is_player: bool,
    /// Cumulative arc-length emitted so far (m).
    s_total: f32,
    /// Deterministic seed 0..1 for procedural noise variation.
    seed: f32,
    /// Last emitted velocity direction (for curvature-sensitive player sampling).
    last_dir: DVec3,
}

impl Trail {
    fn new(is_player: bool, seed: f32) -> Self {
        Self {
            nodes: Vec::with_capacity(if is_player { 256 } else { 64 }),
            active: true,
            is_player,
            s_total: 0.0,
            seed,
            last_dir: DVec3::ZERO,
        }
    }
}

/// Active contrail emitter for one engine (or a merged distant aircraft).
struct ActiveEmitter {
    key: TrailKey,
    pos: DVec3,
    vel: DVec3,
    env: ContrailEnv,
    radius0: f32,
    density0: f32,
    is_player: bool,
    /// Allow back-projecting an initial trail when a cruising live plane first appears.
    preseed: bool,
}

pub struct Contrails {
    trails: HashMap<TrailKey, Trail>,
    /// Reusable sort scratch buffer.
    order: Vec<(TrailKey, f64)>,
}

impl Default for Contrails {
    fn default() -> Self {
        Self::new()
    }
}

impl Contrails {
    pub fn new() -> Self {
        Self { trails: HashMap::new(), order: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.trails.clear();
    }

    pub fn trail_count(&self) -> usize {
        self.trails.len()
    }

    #[allow(dead_code)]
    pub fn total_nodes(&self) -> usize {
        self.trails.values().map(|t| t.nodes.len()).sum()
    }

    /// Advance all contrails by `dt` seconds: advect existing trails with wind aloft,
    /// emit new high-res nodes for the player and LOD nodes for multiplayer,
    /// and adaptively decimate distant/aged segments.
    #[allow(clippy::too_many_arguments)]
    pub fn step(
        &mut self,
        dt: f64,
        cam: DVec3,
        wx: &Weather,
        doy: f64,
        player: Option<(&Plane, bool)>,
        mp_players: &[Remote],
    ) {
        if dt <= 0.0 {
            return;
        }
        let dt_c = dt.min(0.25);
        let dt_f = dt_c as f32;

        // 1. Advect and age all existing trails, pruning dead tails.
        for trail in self.trails.values_mut() {
            trail.active = false;
            if trail.nodes.is_empty() {
                continue;
            }
            // Compute wind aloft once per trail midpoint (plus gentle fallstreak sedimentation
            // for spreading ice crystals) for minimal CPU cost.
            let mid = trail.nodes[trail.nodes.len() / 2].pos;
            let alt = (mid.length() - geo::R).max(0.0);
            let wind = wind_aloft(wx, mid, alt);
            let up = mid.normalize();
            for n in &mut trail.nodes {
                n.age += dt_f;
                // Ice crystals in spreading contrail cirrus slowly settle (~0.15..0.45 m/s)
                // creating realistic virga/fallstreaks sheared by wind.
                let sink = 0.08 + 0.35 * n.supersat * smoothstep_f32(25.0, 180.0, n.age);
                // Older portions drift slightly faster/differently with altitude shear.
                let shear_drift = n.shear_dir.as_dvec3() * (n.shear_strength as f64 * (n.age as f64 * 0.012).min(4.5));
                n.pos += (wind + shear_drift - up * sink as f64) * dt_c;
            }
            // Drop expired tail nodes (keeping one fading node past boundary so tail tapers cleanly).
            let mut drop = 0usize;
            while drop + 1 < trail.nodes.len() && trail.nodes[drop].age >= trail.nodes[drop].life && trail.nodes[drop + 1].age >= trail.nodes[drop + 1].life {
                drop += 1;
            }
            if drop > 0 {
                trail.nodes.drain(..drop);
            }
            if trail.nodes.len() == 1 && trail.nodes[0].age >= trail.nodes[0].life {
                trail.nodes.clear();
            }
        }

        // 2. Gather active emitters: player and multiplayer.
        let mut emitters: Vec<ActiveEmitter> = Vec::with_capacity(64);

        // 2a. Player aircraft.
        if let Some((plane, alive)) = player {
            if alive && !plane.on_ground && plane.running {
                let spd = plane.airspeed().length();
                if spd > 28.0 {
                    let mounts = spec_engines(plane.spec, plane.stage);
                    let n_eng = mounts.len().max(1) as f64;
                    // Power factor from actual fuel flow & throttle/afterburner.
                    let ff_max = plane.spec.fuel_flow[1].max(0.01);
                    let ff_ratio = (plane.fuel_flow / ff_max).clamp(0.0, 2.5);
                    let pwr = (0.35 + 0.65 * ff_ratio.sqrt()).clamp(0.25, 1.55) as f32;
                    let base_pos = plane.model_pos();
                    let fwd = plane.forward();
                    let base_env = atmosphere_at(base_pos, wx, doy, mounts[0].eta, mounts[0].ei_h2o);
                    for (idx, m) in mounts.iter().enumerate() {
                        // Seized engine or broken-off wing stops emitting cleanly.
                        let wing_gone = (m.group == G_LWING && m.pos.x.abs() > plane.dmg.wing[0])
                            || (m.group == G_RWING && m.pos.x.abs() > plane.dmg.wing[1]);
                        if plane.dmg.engine <= 0.0 || wing_gone {
                            continue;
                        }
                        let env = if (m.eta - mounts[0].eta).abs() > 0.01 || (m.ei_h2o - mounts[0].ei_h2o).abs() > 0.01 {
                            atmosphere_at(base_pos, wx, doy, m.eta, m.ei_h2o)
                        } else {
                            base_env
                        };
                        if env.strength <= 0.01 {
                            continue;
                        }
                        // Formation gap: hot exhaust cools to saturation ~10..24 m aft of nozzle.
                        let gap = (8.0 + spd * 0.042 + (plane.burner * 12.0)).clamp(8.0, 28.0);
                        let nozzle_world = base_pos + plane.rot * m.pos.as_dvec3();
                        let emit_pos = nozzle_world - fwd * gap;
                        // Initial radius & density scale directly with aircraft/engine size and fuel burn.
                        let radius0 = (m.scale * (0.85 + 0.15 * pwr)).clamp(0.35, 6.0);
                        let size_dens = (0.48 + 0.42 * (m.scale / 1.4).sqrt()).clamp(0.35, 1.35);
                        let density0 = (size_dens * pwr * env.strength * (1.0 + 0.08 * (n_eng as f32 - 1.0).min(3.0))).clamp(0.05, 1.6);
                        emitters.push(ActiveEmitter {
                            key: TrailKey::Player(idx as u8),
                            pos: emit_pos,
                            vel: plane.vel,
                            env,
                            radius0,
                            density0,
                            is_player: true,
                            preseed: false,
                        });
                    }
                }
            }
        }

        // 2d. Multiplayer remote aircraft.
        for r in mp_players {
            let dist = (r.pos - cam).length();
            if dist > MAX_TRAFFIC_DIST {
                continue;
            }
            let spd = r.vel.length();
            if spd < 35.0 {
                continue;
            }
            let spec = crate::aircraft::nth(r.aircraft as usize);
            let mounts = spec_engines(spec, 0);
            let env = atmosphere_at(r.pos, wx, doy, mounts[0].eta, mounts[0].ei_h2o);
            if env.strength <= 0.02 {
                continue;
            }
            let fwd = (r.rot * DVec3::NEG_Z).normalize_or_zero();
            let gap = (16.0 + spd * 0.07).clamp(14.0, 42.0);
            if dist >= TRAFFIC_MERGE_DIST {
                let r_avg = mounts[0].scale * (1.0 + 0.25 * (mounts.len().min(4) as f32 - 1.0));
                emitters.push(ActiveEmitter {
                    key: TrailKey::Mp(r.id, 0),
                    pos: r.pos - fwd * gap,
                    vel: r.vel,
                    env,
                    radius0: r_avg,
                    density0: (0.85 * env.strength).clamp(0.05, 1.45),
                    is_player: false,
                    preseed: false,
                });
            } else {
                for (idx, m) in mounts.iter().enumerate() {
                    emitters.push(ActiveEmitter {
                        key: TrailKey::Mp(r.id, idx as u8),
                        pos: r.pos + r.rot * m.pos.as_dvec3() - fwd * gap,
                        vel: r.vel,
                        env,
                        radius0: m.scale,
                        density0: (0.8 * (m.scale / 1.4).sqrt() * env.strength).clamp(0.05, 1.4),
                        is_player: false,
                        preseed: false,
                    });
                }
            }
        }

        // 3. Emit nodes for each active emitter (with adaptive distance-based spacing & live pre-seeding).
        for em in emitters {
            let dist = (em.pos - cam).length();
            let seed = seed_for_key(em.key);
            let trail = self.trails.entry(em.key).or_insert_with(|| Trail::new(em.is_player, seed));
            trail.active = true;

            // Pre-seed a realistic trailing contrail behind newly spotted cruising live planes
            // so the sky immediately shows overhead contrails on startup/arrival.
            if em.preseed && trail.nodes.is_empty() && em.vel.length() > 60.0 {
                preseed_trail(trail, &em);
            }

            let dir = em.vel.normalize_or_zero();
            let min_step = if em.is_player {
                let turn = if trail.last_dir.length_squared() > 0.5 {
                    (1.0 - dir.dot(trail.last_dir)).max(0.0)
                } else {
                    0.0
                };
                if turn > 0.0015 { 8.0 } else { 16.0 }
            } else if dist < 5_000.0 {
                65.0
            } else if dist < 25_000.0 {
                180.0
            } else {
                450.0
            };

            let make_node = |pos: DVec3, s_coord: f32, head_fade: f32| Node {
                pos,
                age: 0.0,
                life: em.env.life,
                radius0: em.radius0,
                spread_rate: em.env.spread_rate,
                shear_dir: em.env.shear_dir,
                shear_strength: em.env.shear_strength,
                density0: em.density0,
                supersat: em.env.supersat,
                s_coord,
                head_fade,
            };

            match trail.nodes.len() {
                0 => {
                    // First node + live head node so a segment forms smoothly on the next step.
                    trail.nodes.push(make_node(em.pos, 0.0, 0.0));
                    trail.last_dir = dir;
                }
                1 => {
                    let seg_len = em.pos.distance(trail.nodes[0].pos);
                    if seg_len >= 1.5 {
                        trail.s_total += seg_len as f32;
                        trail.nodes[0].head_fade = 1.0;
                        trail.nodes.push(make_node(em.pos, trail.s_total, 0.0));
                        trail.last_dir = dir;
                    }
                }
                n => {
                    // Keep the very tip node glued smoothly to the aircraft every frame,
                    // and stamp a permanent waypoint whenever distance from `nodes[n-2]` exceeds `min_step`.
                    let anchor = trail.nodes[n - 2].pos;
                    let seg_len = em.pos.distance(anchor);
                    if seg_len > 2500.0 {
                        // Huge teleport/jump: start fresh.
                        trail.nodes.clear();
                        trail.s_total = 0.0;
                        trail.nodes.push(make_node(em.pos, 0.0, 0.0));
                        trail.last_dir = dir;
                    } else if seg_len >= min_step {
                        trail.s_total += seg_len as f32;
                        let head = &mut trail.nodes[n - 1];
                        head.pos = em.pos;
                        head.age = 0.0;
                        head.s_coord = trail.s_total;
                        head.head_fade = 1.0;
                        head.radius0 = em.radius0;
                        head.density0 = em.density0;
                        head.life = em.env.life;
                        head.spread_rate = em.env.spread_rate;
                        head.shear_dir = em.env.shear_dir;
                        head.shear_strength = em.env.shear_strength;
                        head.supersat = em.env.supersat;
                        trail.nodes.push(make_node(em.pos, trail.s_total, 0.0));
                        trail.last_dir = dir;
                    } else {
                        let prev_s = trail.nodes[n - 2].s_coord;
                        let head = &mut trail.nodes[n - 1];
                        head.pos = em.pos;
                        head.age = 0.0;
                        head.s_coord = prev_s + seg_len as f32;
                        head.head_fade = 0.0;
                        head.radius0 = em.radius0;
                        head.density0 = em.density0;
                    }
                }
            }
        }

        // 4. Adaptive LOD decimation & cleanup: lower resolution as trails recede from camera,
        //    preserving crisp high-resolution geometry right behind the player!
        self.trails.retain(|_, trail| {
            if trail.nodes.is_empty() {
                return false;
            }
            // Drop abandoned trails that are completely out of sight range.
            if !trail.active {
                let nearest = trail.nodes.first().unwrap().pos.distance(cam).min(trail.nodes.last().unwrap().pos.distance(cam));
                if nearest > MAX_DRAW_DIST {
                    return false;
                }
            }
            decimate_trail(trail, cam);
            true
        });
    }

    /// Pack visible contrail nodes & segments for the GPU (frustum-culled, back-to-front
    /// trail ordered, strictly bounded by `MAX_NODES` and `MAX_SEGS`).
    pub fn build_draw(&mut self, cam: DVec3, view_proj: Mat4, out: &mut DrawData) {
        out.nodes.clear();
        out.segs.clear();
        if self.trails.is_empty() {
            return;
        }
        // Extract 4 frustum side planes + near plane in camera-relative space.
        let (r0, r1, r2, r3) = (view_proj.row(0), view_proj.row(1), view_proj.row(2), view_proj.row(3));
        let planes = [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r2];
        let in_frustum = |p: Vec3, rad: f32| -> bool {
            let p4 = Vec4::new(p.x, p.y, p.z, 1.0);
            for pl in planes {
                let len = pl.truncate().length();
                if pl.dot(p4) < -rad * len {
                    return false;
                }
            }
            true
        };

        // Sort trails back-to-front so translucent overlaps composite cleanly;
        // reserve guaranteed budget for player trails first if we ever approach MAX_SEGS.
        self.order.clear();
        for (&k, t) in &self.trails {
            if t.nodes.len() < 2 {
                continue;
            }
            let mid = t.nodes[t.nodes.len() / 2].pos;
            let d = mid.distance(cam);
            // Slight bias so player trails always get priority when budgets are tight,
            // while still sorting roughly back-to-front among peers.
            self.order.push((k, d));
        }
        // Farther trails first (back-to-front).
        self.order.sort_by(|a, b| b.1.total_cmp(&a.1));

        // Reserve room for player trails so multiplayer can never starve the player's own contrails.
        let player_need: usize = self
            .trails
            .values()
            .filter(|t| t.is_player && t.nodes.len() >= 2)
            .map(|t| t.nodes.len())
            .sum();
        let other_node_cap = MAX_NODES.saturating_sub(player_need.min(MAX_NODES / 2));

        for &(key, _) in &self.order {
            let Some(trail) = self.trails.get(&key) else { continue };
            let n = trail.nodes.len();
            if n < 2 {
                continue;
            }
            if !trail.is_player && out.nodes.len() >= other_node_cap {
                continue;
            }
            // Quick trail-level distance cull.
            let p_first = trail.nodes[0].pos - cam;
            let p_last = trail.nodes[n - 1].pos - cam;
            let p_mid = trail.nodes[n / 2].pos - cam;
            let min_d = p_first.length().min(p_last.length()).min(p_mid.length());
            if min_d > MAX_DRAW_DIST {
                continue;
            }

            // Emit visible contiguous runs of segments with shared mitered nodes.
            let mut prev_gpu_idx: Option<u32> = None;
            let mut prev_vis_node: Option<usize> = None;

            for i in 0..n {
                let nd = &trail.nodes[i];
                let rel = (nd.pos - cam).as_vec3();
                let rad = nd.current_radius();
                let stretch = nd.shear_stretch();
                let bound_r = rad * (1.0 + stretch) + 25.0;
                let dist = rel.length();
                // Check whether either this node or the segment to/from it can be visible.
                let next_rel = if i + 1 < n { (trail.nodes[i + 1].pos - cam).as_vec3() } else { rel };
                let seg_mid = 0.5 * (rel + next_rel);
                let seg_half = 0.5 * (next_rel - rel).length() + bound_r;
                let seg_vis = i + 1 < n && dist < MAX_DRAW_DIST as f32 && in_frustum(seg_mid, seg_half);
                let needed = seg_vis || prev_vis_node == Some(i.saturating_sub(1));

                if !needed {
                    prev_gpu_idx = None;
                    prev_vis_node = None;
                    continue;
                }
                if out.nodes.len() >= MAX_NODES || out.segs.len() >= MAX_SEGS {
                    break;
                }

                // Smooth central-difference unit tangent along the trail.
                let tan = if i == 0 {
                    (trail.nodes[1].pos - nd.pos).as_vec3().normalize_or_zero()
                } else if i + 1 == n {
                    (nd.pos - trail.nodes[i - 1].pos).as_vec3().normalize_or_zero()
                } else {
                    (trail.nodes[i + 1].pos - trail.nodes[i - 1].pos).as_vec3().normalize_or_zero()
                };
                let tan = if tan.length_squared() < 1e-6 { Vec3::Y } else { tan };
                let k_age = (nd.age / nd.life.max(1.0)).clamp(0.0, 1.0);
                // Always feather the very oldest node (and first ~250 m of a freshly started trail)
                // smoothly to 0 so there is never a hard square cut-off at the tail end!
                let tail_cap = if i == 0 {
                    0.0
                } else {
                    smoothstep_f32(0.0, 260.0, nd.s_coord - trail.nodes[0].s_coord)
                };
                // Also smoothly taper the active head tip behind the engine over ~18 m.
                let head_cap = if trail.active && i + 1 == n {
                    0.0
                } else if trail.active {
                    smoothstep_f32(0.0, 18.0, trail.nodes[n - 1].s_coord - nd.s_coord)
                } else if i + 1 == n {
                    0.0
                } else {
                    1.0
                };
                let dens = nd.current_density() * tail_cap * head_cap;

                let idx = out.nodes.len() as u32;
                out.nodes.push(GpuNode {
                    pos_r: [rel.x, rel.y, rel.z, rad],
                    tan_s: [tan.x, tan.y, tan.z, nd.s_coord],
                    shear: [nd.shear_dir.x, nd.shear_dir.y, nd.shear_dir.z, stretch],
                    props: [dens, k_age, nd.supersat, trail.seed],
                });

                if let Some(p_idx) = prev_gpu_idx {
                    if out.segs.len() < MAX_SEGS {
                        out.segs.push(GpuSeg { a: p_idx, b: idx });
                    }
                }
                if seg_vis {
                    prev_gpu_idx = Some(idx);
                    prev_vis_node = Some(i);
                } else {
                    prev_gpu_idx = None;
                    prev_vis_node = None;
                }
            }
        }
    }
}

/// Pre-seed a natural trailing history behind a newly observed high-altitude aircraft
/// so overhead cruising aircraft already displays wind-drifted contrails immediately.
fn preseed_trail(trail: &mut Trail, em: &ActiveEmitter) {
    let max_age = (em.env.life * 0.65).min(150.0);
    if max_age < 6.0 {
        return;
    }
    let steps: usize = if max_age > 60.0 { 14 } else { 8 };
    let up = em.pos.normalize();
    let spd = em.vel.length();
    trail.s_total = 0.0;
    for k in (0..=steps).rev() {
        let frac = k as f32 / steps as f32;
        let age = max_age * frac;
        let age64 = age as f64;
        let sink = (0.08 + 0.35 * em.env.supersat) as f64 * age64;
        let shear_drift = em.env.shear_dir.as_dvec3() * (em.env.shear_strength as f64 * (age64 * 0.006).min(2.5) * age64 * 0.5);
        // Where an air parcel emitted `age` seconds ago is now:
        // emitted at `pos - vel * age`, then carried by wind for `age` seconds!
        let pos = em.pos - em.vel * age64 + em.env.wind * age64 + shear_drift - up * sink;
        let s_coord = ((max_age - age) as f64 * spd) as f32;
        trail.s_total = trail.s_total.max(s_coord);
        // Smooth tail fade on the oldest pre-seeded node so there is no hard cut-off.
        let tail_taper = if k == steps { 0.0 } else { 1.0 };
        let head_fade = if k == 0 { 0.0 } else { tail_taper };
        trail.nodes.push(Node {
            pos,
            age,
            life: em.env.life,
            radius0: em.radius0,
            spread_rate: em.env.spread_rate,
            shear_dir: em.env.shear_dir,
            shear_strength: em.env.shear_strength,
            density0: em.density0,
            supersat: em.env.supersat,
            s_coord,
            head_fade,
        });
    }
    trail.last_dir = em.vel.normalize_or_zero();
}

/// Adaptive distance-based LOD decimation: keeps high resolution near the camera
/// (and near the player's fresh wake), progressively collapsing colinear interior
/// nodes as they age and recede into the distance.
fn decimate_trail(trail: &mut Trail, cam: DVec3) {
    // Keep at least 3 nodes intact and never touch the freshest 4 nodes behind an active aircraft.
    let protect_tail = if trail.active { if trail.is_player { 6 } else { 3 } } else { 1 };
    if trail.nodes.len() <= protect_tail + 2 {
        return;
    }
    let last_mergeable = trail.nodes.len() - protect_tail;
    let mut write = 1usize;
    let mut i = 1usize;
    while i < last_mergeable {
        let prev = trail.nodes[write - 1];
        let cur = trail.nodes[i];
        let next = trail.nodes[i + 1];
        let dist = cur.pos.distance(cam);
        // Keep close player wake crisp.
        let near_guard = if trail.is_player { 350.0 } else { 200.0 };
        let mut drop = false;
        if dist > near_guard || (!trail.is_player && cur.age > 8.0) {
            let span = next.pos - prev.pos;
            let span_len = span.length();
            let max_seg = if trail.is_player {
                (dist * 0.032).clamp(30.0, 2200.0)
            } else {
                (dist * 0.048).clamp(80.0, 3200.0)
            };
            if span_len < max_seg && span_len > 1e-3 {
                // Perpendicular chord error of `cur` from segment `prev -> next`.
                let t = ((cur.pos - prev.pos).dot(span) / (span_len * span_len)).clamp(0.0, 1.0);
                let proj = prev.pos + span * t;
                let err = cur.pos.distance(proj);
                // ~1.5–2 pixels angular error tolerance + similar age/density so fade stays smooth.
                let tol = (dist * if trail.is_player { 0.0016 } else { 0.0025 }).max(1.5);
                let d_age = (next.age - prev.age).abs();
                let d_dens = (cur.density0 - 0.5 * (prev.density0 + next.density0)).abs();
                if err < tol && d_age < cur.life * 0.14 && d_dens < 0.18 {
                    drop = true;
                }
            }
        }
        if !drop {
            trail.nodes[write] = cur;
            write += 1;
        }
        i += 1;
    }
    // Copy protected head nodes.
    while i < trail.nodes.len() {
        trail.nodes[write] = trail.nodes[i];
        write += 1;
        i += 1;
    }
    trail.nodes.truncate(write);
}

fn seed_for_key(key: TrailKey) -> f32 {
    let x: u32 = match key {
        TrailKey::Player(e) => 0x1111_0000u32 ^ (e as u32).wrapping_mul(2_654_435_761),
        TrailKey::Mp(id, e) => id.wrapping_mul(2_654_435_761) ^ (e as u32).wrapping_mul(83_492_791),
    };
    (shash(x) & 0xffff) as f32 / 65535.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aircraft;

    #[test]
    fn schmidt_appleman_matches_real_cruise_physics() {
        // At 10.5 km (~FL345, ISA -53 °C, ~245 hPa), modern turbofans strongly form contrails.
        let wx = Weather::new(0.45);
        let p_cruise = geo::lla_to_pos(40f64.to_radians(), -100f64.to_radians(), 10_500.0);
        let env = atmosphere_at(p_cruise, &wx, 180.0, 0.36, 1.25);
        assert!(env.temp_k < env.t_sac, "expected T={:.1} K < T_sac={:.1} K", env.temp_k, env.t_sac);
        assert!(env.strength > 0.8, "cruise contrail strength should be strong, got {}", env.strength);

        // At warm sea level / 2 km, contrails never form.
        let p_low = geo::lla_to_pos(37f64.to_radians(), -122f64.to_radians(), 2_000.0);
        let env_low = atmosphere_at(p_low, &wx, 180.0, 0.36, 1.25);
        assert_eq!(env_low.strength, 0.0, "no contrails at warm 2 km: {env_low:?}");

        // High dry stratosphere (24 km, SR-71 cruise): contrails stop forming.
        let p_strat = geo::lla_to_pos(40f64.to_radians(), -100f64.to_radians(), 24_000.0);
        let env_strat = atmosphere_at(p_strat, &wx, 180.0, 0.30, 1.25);
        assert_eq!(env_strat.strength, 0.0, "no contrails in dry 24 km stratosphere: {env_strat:?}");

        // Deep Arctic winter (-40 °C surface): contrails can form much lower (~4 km)!
        let mut arctic = Weather::new(0.4);
        arctic.freezing = Some(-6000.0); // ~-40 °C surface
        let p_arctic = geo::lla_to_pos(72f64.to_radians(), 25f64.to_radians(), 4_000.0);
        let env_arctic = atmosphere_at(p_arctic, &arctic, 15.0, 0.36, 1.25);
        assert!(env_arctic.strength > 0.5, "extreme Arctic cold should form contrails lower: {env_arctic:?}");
    }

    #[test]
    fn weather_produces_short_persistent_and_spreading_regimes() {
        let p = geo::lla_to_pos(45f64.to_radians(), -95f64.to_radians(), 10_500.0);

        // Clear/dry weather -> short-lived contrails.
        let mut dry = Weather::new(0.05);
        dry.preset("Clear");
        let e_dry = atmosphere_at(p, &dry, 120.0, 0.36, 1.25);

        // Overcast/moist windy weather -> long spreading contrail cirrus.
        let mut moist = Weather::new(0.95);
        moist.preset("Overcast");
        moist.wind = 18.0;
        moist.alto = 0.85;
        let e_moist = atmosphere_at(p, &moist, 120.0, 0.36, 1.25);

        assert!(e_dry.strength > 0.5 && e_moist.strength > 0.8);
        assert!(e_dry.life < 60.0, "dry day should be short-lived, got {} s (rhi={:.2})", e_dry.life, e_dry.rhi);
        assert!(e_moist.life > 240.0, "moist day should persist, got {} s (rhi={:.2})", e_moist.life, e_moist.rhi);
        assert!(e_moist.spread_rate > e_dry.spread_rate * 2.0, "moist windy day should spread much faster: {} vs {}", e_moist.spread_rate, e_dry.spread_rate);
        assert!(e_moist.supersat > 0.6 && e_dry.supersat < 0.2);
    }

    #[test]
    fn bigger_aircraft_produce_bigger_contrails() {
        let e747 = spec_engines(&aircraft::b747::SPEC, 0);
        let e650 = spec_engines(&aircraft::g650::SPEC, 0);
        let ef18 = spec_engines(&aircraft::fa18::SPEC, 0);
        let ec172 = spec_engines(&aircraft::c172::SPEC, 0);
        assert_eq!(e747.len(), 4);
        assert_eq!(e650.len(), 2);
        assert_eq!(ef18.len(), 2);
        assert_eq!(ec172.len(), 1);
        assert!(e747[0].scale > e650[0].scale, "747 {} > G650 {}", e747[0].scale, e650[0].scale);
        assert!(e650[0].scale > ef18[0].scale, "G650 {} > F/A-18 {}", e650[0].scale, ef18[0].scale);
        assert!(ef18[0].scale > ec172[0].scale, "F/A-18 {} > C172 {}", ef18[0].scale, ec172[0].scale);
    }

    #[test]
    fn wind_drifts_trails_and_lod_decimates_distant_nodes() {
        let mut wx = Weather::new(0.6);
        wx.alto = 0.7;
        wx.wind = 16.0;
        wx.wind_from = 270.0; // westerly -> blows east!
        let mut plane = Plane::spawn(&aircraft::b747::SPEC, 40f64.to_radians(), -100f64.to_radians(), 10_500.0, 0.0, 240.0);
        plane.throttle = 0.82;
        plane.fuel_flow = plane.spec.fuel_flow[1] * 0.6;
        let (east, north, _) = geo::enu(plane.pos);
        plane.vel = north * 240.0;

        let mut ct = Contrails::new();
        let mut first_pos = None;
        for step in 0..300 {
            let dt = 0.1;
            plane.pos += plane.vel * dt;
            let cam = plane.pos - north * 120.0;
            ct.step(dt, cam, &wx, 170.0, Some((&plane, true)), &[]);
            if step == 5 {
                first_pos = Some(ct.trails[&TrailKey::Player(0)].nodes[0].pos);
            }
        }
        let t0 = &ct.trails[&TrailKey::Player(0)];
        assert_eq!(ct.trail_count(), 4, "B747 has 4 engines");
        // Verify eastward wind drift of the oldest parcel!
        let drift_east = (t0.nodes[0].pos - first_pos.unwrap()).dot(east);
        assert!(drift_east > 300.0, "expected strong eastward wind drift, got {drift_east:.1} m");
        // Plane flew 7.2 km in 30 s: without decimation that would be ~450 nodes per engine;
        // adaptive LOD should decimate straight distant sections dramatically!
        assert!(t0.nodes.len() < 160, "expected adaptive LOD decimation, still has {} nodes", t0.nodes.len());
        assert!(t0.nodes.len() >= 10, "should keep smooth nodes, got {}", t0.nodes.len());
    }


}
