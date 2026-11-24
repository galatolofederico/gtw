//! Real weather: the current conditions where the player is (or where they
//! are about to start), turned into the weather sliders. The nearest METAR
//! (aviationweather.gov) within 60 km gives the wind, gusts, visibility, the
//! cloud layers, thunderstorms and rain; away from any airport, Open-Meteo's model
//! stands in. A background thread looks again every ten minutes or after a
//! move of 30 km, and the sky eases into each new report.
//!
//! Where a weather radar sees (RainViewer's composite, radar.rs), the storm
//! cells stand where it shows the convective cores and the rain deck over its
//! widespread rain, refreshed every five minutes or after 40 km, growing in
//! and dying away as the picture changes. Beyond radar coverage (or with no
//! radar picture) the report's storms and rain are spread round as before.

use crate::geo;
use crate::radar::{self, Radar};
use crate::sources::Sources;
use crate::weather::Weather;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often to look again (s), and how far the player may move first (km).
const REFRESH: f64 = 600.0;
const MOVE_KM: f64 = 30.0;
/// The nearest METAR station is used up to this far away (km).
const STATION_KM: f64 = 60.0;
/// How often to look for a new radar picture (s; RainViewer makes one every
/// ten minutes), and how far the player may move before it is read again round
/// them (km).
const RADAR_REFRESH: f64 = 300.0;
const RADAR_MOVE_KM: f64 = 40.0;
/// A radar picture older than this is no longer the weather now (s).
const RADAR_STALE: i64 = 2700;

/// The weather sliders a report comes to.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub clouds: f32,
    /// None: the report says nothing about it (keep what there is)
    pub base_m: Option<f32>,
    pub depth_m: f32,
    pub alto: f32,
    pub storms: f32,
    /// showers but no thunder reported: the convective cells are showers
    pub showers_only: bool,
    /// rain falling at the station, 0..1 (the Rain slider: it brings the
    /// nimbostratus rain deck); 0 = none
    pub rain: f32,
    pub haze: f32,
    /// m/s, and where from (None: variable)
    pub wind: f64,
    pub wind_from: Option<f64>,
    pub gusts: f64,
    /// sea-level pressure, hPa (None: not reported)
    pub qnh: Option<f64>,
    /// freezing level (m above sea level) from the surface temperature
    /// (None: not reported)
    pub freezing: Option<f64>,
    /// snow blown up off the ground (METAR BLSN 1, DRSN 0.5), 0..1
    pub blowing: f32,
    /// surface relative humidity (0..1) from temperature and dew point when reported
    pub humidity: Option<f32>,
    /// what it came from, for the weather panel: "EBBR" or "Open-Meteo",
    /// and the report itself (the raw METAR)
    pub station: String,
    pub raw: String,
}

#[derive(Default)]
struct Shared {
    want: Option<(f64, f64)>,
    got: Option<(Instant, Target)>,
    error: String,
    /// the latest radar picture read round the player (None: none to be had)
    radar: Option<(Instant, Arc<Radar>)>,
}

pub struct LiveWx {
    shared: Arc<Mutex<Shared>>,
    thread: bool,
    last_got: Option<Instant>,
    target: Option<Target>,
    /// ease in (false right after a new place is picked: jump straight there)
    settled: bool,
    /// the Rain slider (`rain_set`) is ours: handed back when live weather goes off
    owns_rain: bool,
    radar_thread: bool,
    /// the radar picture in use (when it came; where the player was)
    radar_got: Option<Instant>,
    radar: Option<Arc<Radar>>,
    radar_at: Option<(f64, f64)>,
}

impl LiveWx {
    pub fn new() -> Self {
        Self { shared: Arc::default(), thread: false, last_got: None, target: None, settled: false, owns_rain: false, radar_thread: false, radar_got: None, radar: None, radar_at: None }
    }

    pub fn error(&self) -> String {
        self.shared.lock().error.clone()
    }

    /// Each frame: where the weather is wanted (degrees), and the time since
    /// the last frame (s; 0 jumps straight to the report, as on the globe).
    pub fn step(&mut self, src: &Arc<Sources>, wx: &mut Weather, at: Option<(f64, f64)>, dt: f64) {
        if !wx.live {
            self.settled = false;
            if self.owns_rain {
                self.owns_rain = false;
                wx.rain_set = None;
                wx.live_cells.clear();
                (wx.radar_cover, wx.deck_goal, wx.deck_tau) = (None, None, 6.0);
                (self.radar, self.radar_got, self.radar_at) = (None, None, None);
            }
            // Hand-set weather: the standard atmosphere.
            wx.qnh = 1013.25;
            wx.freezing = None;
            wx.blowing = 0.0;
            wx.humidity = None;
            return;
        }
        let (got, radar) = {
            let mut s = self.shared.lock();
            s.want = at;
            let r = match &s.radar {
                Some((t, r)) if Some(*t) != self.radar_got => Some((Some(*t), Some(r.clone()))),
                None if self.radar_got.is_some() => Some((None, None)),
                _ => None,
            };
            (s.got.as_ref().filter(|(t, _)| Some(*t) != self.last_got).cloned(), r)
        };
        if !self.thread && at.is_some() {
            self.thread = true;
            spawn(src.clone(), self.shared.clone());
        }
        if !self.radar_thread && at.is_some() {
            self.radar_thread = true;
            spawn_radar(src.clone(), self.shared.clone());
        }
        if let Some((t, r)) = radar {
            self.radar_got = t;
            self.apply_radar(wx, r, at, dt);
        }
        wx.step_live(dt);
        // Where there is radar, the rain deck stands over its rain (weather.rs
        // `step_deck` eases it in and out).
        let radar_here = self.radar.as_ref().is_some_and(|r| r.here);
        if let Some(r) = self.radar.as_ref().filter(|r| r.here) {
            wx.rain_set = r.deck.as_ref().map(|d| d.amount);
            wx.deck_tau = 25.0;
        }
        if let Some((t, target)) = got {
            self.last_got = Some(t);
            wx.live_station = target.station.clone();
            wx.live_raw = target.raw.clone();
            self.target = Some(target);
        }
        let Some(t) = &self.target else {
            let e = self.error();
            if !e.is_empty() {
                wx.live_raw = format!("({e})");
            }
            return;
        };
        // About half way in 10 s; at once on the globe or the first time.
        let k = if dt <= 0.0 || !self.settled { 1.0 } else { 1.0 - (-dt / 14.0).exp() };
        self.settled = true;
        let mix = |a: f32, b: f32| a + (b - a) * k as f32;
        wx.clouds = mix(wx.clouds, t.clouds);
        if let Some(b) = t.base_m {
            wx.cloud_base = mix(wx.cloud_base, b);
        }
        wx.cloud_depth = mix(wx.cloud_depth, t.depth_m);
        wx.alto = mix(wx.alto, t.alto);
        wx.storms = mix(wx.storms, t.storms);
        wx.showers_only = t.showers_only;
        wx.haze = mix(wx.haze, t.haze);
        // Beyond the radar, a rain deck round the aircraft as heavy as the
        // report says.
        if !radar_here {
            wx.rain_set = (t.rain > 0.0).then_some(t.rain);
            (wx.deck_goal, wx.deck_tau) = (None, 6.0);
        }
        self.owns_rain = true;
        wx.wind += (t.wind - wx.wind) * k;
        wx.gusts += (t.gusts - wx.gusts) * k;
        if let Some(q) = t.qnh {
            wx.qnh += (q - wx.qnh) * k;
        }
        wx.blowing = t.blowing;
        wx.humidity = match (wx.humidity, t.humidity) {
            (Some(a), Some(b)) => Some(a + (b - a) * k as f32),
            (_, b) => b,
        };
        wx.freezing = match (wx.freezing, t.freezing) {
            (Some(a), Some(b)) => Some(a + (b - a) * k),
            (_, b) => b,
        };
        if let Some(from) = t.wind_from {
            let d = (from - wx.wind_from + 540.0).rem_euclid(360.0) - 180.0;
            wx.wind_from = (wx.wind_from + d * k).rem_euclid(360.0);
        }
    }
}

impl LiveWx {
    /// A new radar picture (None: there is none now): its cells into cloud
    /// space where they stand now, and the rain deck's place.
    fn apply_radar(&mut self, wx: &mut Weather, r: Option<Arc<Radar>>, at: Option<(f64, f64)>, dt: f64) {
        self.owns_rain = true;
        let Some(r) = r else {
            // Lost it: the radar's cells die away, the report takes over.
            wx.set_live_cells(&[], false);
            (wx.radar_cover, wx.deck_goal) = (None, None);
            (self.radar, self.radar_at) = (None, None);
            return;
        };
        // A fresh start (the first picture, or the player put down far away):
        // show it as it is rather than growing it all in.
        let jump = dt <= 0.0 || self.radar_at.zip(at).is_none_or(|(a, b)| dist_km(a, b) > 150.0);
        self.radar_at = at;
        // The cloud field drifts with the wind (Weather::advance): where the
        // radar saw a cell is where it stood when the picture was taken.
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(r.time);
        let age = (now - r.time).clamp(0, RADAR_STALE) as f64;
        let cloud = |lat: f64, lon: f64| {
            let p = geo::lla_to_pos(lat.to_radians(), lon.to_radians(), 0.0);
            let drift = wx.steady(p, 2000.0) * 1.6 * age;
            (p.normalize() * 6371.0 + (wx.cloud_off + drift) / 1000.0).normalize() * 6371.0
        };
        // A squall line stands as one (the shaders draw it across the wind
        // aloft) where it lies within 40 degrees of that; otherwise as its cells.
        let across = (wx.wind_from + 90.0).rem_euclid(180.0);
        let mut cells = Vec::new();
        for c in &r.cells {
            let off = ((c.axis - across).rem_euclid(180.0) + 90.0).rem_euclid(180.0) - 90.0;
            if c.len > 0.0 && off.abs() < 40.0 {
                cells.push((cloud(c.lat, c.lon), c.rad, c.age, c.len * off.to_radians().cos()));
            } else if c.len > 0.0 {
                cells.extend(c.parts.iter().map(|p| (cloud(p.lat, p.lon), p.rad, p.age, 0.0)));
            } else {
                cells.push((cloud(c.lat, c.lon), c.rad, c.age, 0.0));
            }
        }
        let deck = r.deck.as_ref().filter(|_| r.here).map(|d| cloud(d.lat, d.lon));
        wx.set_live_cells(&cells, jump);
        wx.deck_goal = deck;
        wx.radar_cover = Some(r.cover.clone());
        if let Some(a) = at {
            let near: Vec<String> = r
                .cells
                .iter()
                .take(6)
                .map(|c| format!("{:.0} km {:03.0}° r{:.1} age{:.2} {:.0}dBZ{}", radar::dist_km(a, (c.lat, c.lon)), bearing(a, (c.lat, c.lon)), c.rad, c.age, c.peak, if c.len > 0.0 { format!(" squall line {:.0} km along {:03.0}°", c.len, c.axis) } else { String::new() }))
                .collect();
            let deck = r.deck.as_ref().map(|d| format!("{:.0} km {:03.0}° rain {:.2} ({:.0}% wet)", radar::dist_km(a, (d.lat, d.lon)), bearing(a, (d.lat, d.lon)), d.amount, d.frac * 100.0));
            log::info!("radar ({} min old): {} cells{}, deck {:?}; nearest: {}", age as i64 / 60, r.cells.len(), if r.here { "" } else { " (no radar here)" }, deck, near.join(", "));
        }
        self.radar = Some(r);
    }
}

/// Compass bearing from a to b, degrees.
fn bearing(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, la2, dl) = (a.0.to_radians(), b.0.to_radians(), (b.1 - a.1).to_radians());
    let y = dl.sin() * la2.cos();
    let x = la1.cos() * la2.sin() - la1.sin() * la2.cos() * dl.cos();
    y.atan2(x).to_degrees().rem_euclid(360.0)
}

/// The radar thread: a new picture every five minutes (if RainViewer has
/// one), read round the player again after a move of 40 km. The tiles of the
/// picture in use stay in memory; the coverage tiles (which hardly change) on
/// disk.
fn spawn_radar(src: Arc<Sources>, shared: Arc<Mutex<Shared>>) {
    std::thread::spawn(move || {
        let mut frame: Option<(Instant, Frame)> = None;
        let mut done: Option<(String, (f64, f64))> = None;
        let mut tiles: std::collections::HashMap<(i64, i64), Arc<Vec<f32>>> = Default::default();
        let mut retry: Option<Instant> = None;
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let Some(p) = shared.lock().want else { continue };
            if retry.is_some_and(|t| t.elapsed().as_secs() < 120) {
                continue;
            }
            retry = None;
            if frame.as_ref().is_none_or(|(t, _)| t.elapsed().as_secs_f64() > RADAR_REFRESH) {
                match radar_frame(&src) {
                    Ok(f) => {
                        if frame.as_ref().is_none_or(|(_, o)| o.path != f.path) {
                            tiles.clear();
                        }
                        frame = Some((Instant::now(), f));
                    }
                    Err(e) => {
                        radar_failed(&shared, &e, &mut retry);
                        continue;
                    }
                }
            }
            let Some((_, f)) = &frame else { continue };
            if done.as_ref().is_some_and(|(path, q)| *path == f.path && dist_km(p, *q) < RADAR_MOVE_KM) {
                continue;
            }
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            if now - f.time > RADAR_STALE {
                radar_failed(&shared, "the radar picture is old", &mut retry);
                continue;
            }
            match radar_echo(&src, f, p, &mut tiles) {
                Ok(e) => {
                    let mut r = radar::analyse(&e, p);
                    r.time = f.time;
                    let mut s = shared.lock();
                    s.radar = Some((Instant::now(), Arc::new(r)));
                    done = Some((f.path.clone(), p));
                }
                Err(e) => radar_failed(&shared, &e, &mut retry),
            }
        }
    });
}

fn radar_failed(shared: &Mutex<Shared>, e: &str, retry: &mut Option<Instant>) {
    if e != "offline" {
        log::warn!("weather radar: {e}");
    }
    let mut s = shared.lock();
    // An older picture will still do for a while.
    if s.radar.as_ref().is_some_and(|(t, _)| t.elapsed().as_secs() > 1800) {
        s.radar = None;
    }
    *retry = Some(Instant::now());
}

/// RainViewer's newest radar picture.
struct Frame {
    host: String,
    path: String,
    /// Unix seconds
    time: i64,
}

fn radar_frame(src: &Sources) -> Result<Frame, String> {
    let j = get_json(src, "https://api.rainviewer.com/public/weather-maps.json")?;
    let last = j["radar"]["past"].as_array().and_then(|a| a.last()).ok_or("no radar pictures")?;
    Ok(Frame {
        host: j["host"].as_str().unwrap_or("https://tilecache.rainviewer.com").to_string(),
        path: last["path"].as_str().ok_or("no radar path")?.to_string(),
        time: last["time"].as_i64().ok_or("no radar time")?,
    })
}

/// The strongest storms anywhere a radar sees right now (lat, lon, dBZ), a
/// few hundred km apart, strongest first: places to fly to see live storms
/// (`--find-storms`). Reads the whole world at zoom 2 (16 tiles).
pub fn find_storms(fetch: &crate::fetch::Fetcher) -> Result<Vec<(f64, f64, f32)>, String> {
    let j: serde_json::Value = serde_json::from_slice(&fetch.get("https://api.rainviewer.com/public/weather-maps.json").map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let host = j["host"].as_str().unwrap_or("https://tilecache.rainviewer.com");
    let path = j["radar"]["past"].as_array().and_then(|a| a.last()).and_then(|f| f["path"].as_str()).ok_or("no radar pictures")?;
    let mut hits: Vec<(f32, f64, f64)> = Vec::new();
    for y in 0..4i64 {
        for x in 0..4i64 {
            let d = radar::decode_tile(&fetch.get(&format!("{host}{path}/256/2/{x}/{y}/2/0_0.png")).map_err(|e| e.to_string())?)?;
            for (k, &z) in d.iter().enumerate() {
                if z >= 45.0 {
                    let (gx, gy) = ((x * 256 + (k % 256) as i64) as f64 + 0.5, (y * 256 + (k / 256) as i64) as f64 + 0.5);
                    hits.push((z, geo::merc_y_to_lat(gy / 1024.0).to_degrees(), gx / 1024.0 * 360.0 - 180.0));
                }
            }
        }
    }
    hits.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out: Vec<(f64, f64, f32)> = Vec::new();
    for (z, lat, lon) in hits {
        if out.len() < 12 && out.iter().all(|o| radar::dist_km((o.0, o.1), (lat, lon)) > 300.0) {
            out.push((lat, lon, z));
        }
    }
    Ok(out)
}

/// The block of radar and coverage tiles round p, as one grid.
fn radar_echo(src: &Sources, f: &Frame, (lat, lon): (f64, f64), tiles: &mut std::collections::HashMap<(i64, i64), Arc<Vec<f32>>>) -> Result<radar::Echo, String> {
    let (z, tx0, ty0, n) = radar::Echo::block(lat, lon);
    let s = 256 * n as usize;
    let mut e = radar::Echo { z, gx0: tx0 * 256, gy0: ty0 * 256, w: s, h: s, dbz: vec![f32::NAN; s * s], cover: vec![false; s * s] };
    let wrap = 1i64 << z;
    for ty in 0..n {
        for tx in 0..n {
            let (x, y) = ((tx0 + tx).rem_euclid(wrap), ty0 + ty);
            let d = match tiles.get(&(x, y)) {
                Some(d) => d.clone(),
                None => {
                    // (smooth 0: the pixels as measured; snow 0: one palette)
                    let url = format!("{}{}/256/{z}/{x}/{y}/2/0_0.png", f.host, f.path);
                    let d = Arc::new(radar::decode_tile(&src.fetch.get(&url).map_err(|e| e.to_string())?)?);
                    tiles.insert((x, y), d.clone());
                    d
                }
            };
            let url = format!("{}/v2/coverage/0/256/{z}/{x}/{y}/0/0_0.png", f.host);
            let c = radar::decode_cover(&src.fetch.get_cached(&format!("radar/coverage/{z}/{x}/{y}.png"), &url).map_err(|e| e.to_string())?)?;
            for j in 0..256 {
                let row = (ty as usize * 256 + j) * s + tx as usize * 256;
                e.dbz[row..row + 256].copy_from_slice(&d[j * 256..j * 256 + 256]);
                e.cover[row..row + 256].copy_from_slice(&c[j * 256..j * 256 + 256]);
            }
        }
    }
    Ok(e)
}

fn spawn(src: Arc<Sources>, shared: Arc<Mutex<Shared>>) {
    std::thread::spawn(move || {
        let mut last: Option<(Instant, (f64, f64))> = None;
        loop {
            let want = shared.lock().want;
            if let Some(p) = want {
                let due = match last {
                    None => true,
                    Some((t, q)) => t.elapsed().as_secs_f64() > REFRESH || dist_km(p, q) > MOVE_KM,
                };
                if due {
                    let res = metar(&src, p).or_else(|e| {
                        log::info!("live weather: {e}; using Open-Meteo");
                        open_meteo(&src, p)
                    });
                    let mut s = shared.lock();
                    match res {
                        Ok(t) => {
                            s.got = Some((Instant::now(), t));
                            s.error.clear();
                            last = Some((Instant::now(), p));
                        }
                        Err(e) => {
                            if e != "offline" {
                                log::warn!("live weather: {e}");
                            }
                            s.error = e;
                            // try again in a minute, not every second
                            last = Some((Instant::now() - Duration::from_secs_f64(REFRESH - 60.0), p));
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

fn dist_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let p = |q: (f64, f64)| geo::lla_to_pos(q.0.to_radians(), q.1.to_radians(), 0.0);
    (p(a) - p(b)).length() / 1000.0
}

fn get_json(src: &Sources, url: &str) -> Result<serde_json::Value, String> {
    let d = src.fetch.get(url).map_err(|e| e.to_string())?;
    serde_json::from_slice(&d).map_err(|e| e.to_string())
}

/// The nearest METAR station's latest report.
fn metar(src: &Sources, (lat, lon): (f64, f64)) -> Result<Target, String> {
    let dlat = STATION_KM / 111.0;
    let dlon = dlat / lat.to_radians().cos().max(0.2);
    let url = format!("https://aviationweather.gov/api/data/metar?bbox={:.3},{:.3},{:.3},{:.3}&format=json", lat - dlat, lon - dlon, lat + dlat, lon + dlon);
    let j = get_json(src, &url)?;
    let near = j
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| Some((dist_km((lat, lon), (m["lat"].as_f64()?, m["lon"].as_f64()?)), m)))
        .filter(|(d, _)| *d < STATION_KM)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, m)| m)
        .ok_or_else(|| "no METAR station near".to_string())?;
    from_metar(near).ok_or_else(|| "unreadable METAR".to_string())
}

/// A METAR (aviationweather.gov's JSON) as the weather sliders.
fn from_metar(m: &serde_json::Value) -> Option<Target> {
    let raw = m["rawOb"].as_str().unwrap_or("").to_string();
    let station = m["icaoId"].as_str()?.to_string();
    let elev = m["elev"].as_f64().unwrap_or(0.0);
    let kt = |v: &serde_json::Value| v.as_f64();
    let wind = kt(&m["wspd"]).unwrap_or(0.0) / 1.94384;
    let gust = kt(&m["wgst"]).map(|g| g / 1.94384);
    let wind_from = m["wdir"].as_f64();
    // Visibility in statute miles: 6, "6+", "1/2", "1 1/2".
    let vis_km = match &m["visib"] {
        serde_json::Value::Number(n) => n.as_f64(),
        // ("6+": at least six miles, as clear as the sky model goes)
        serde_json::Value::String(s) if s.trim().ends_with('+') => None,
        serde_json::Value::String(s) => sm(s),
        _ => None,
    }
    .map(|v| v * 1.609);
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    let wx = m["wxString"].as_str().unwrap_or("");
    let cavok = tokens.contains(&"CAVOK");
    // The cloud layers: (cover 0..1, base in m above sea level).
    let mut layers: Vec<(f32, f32)> = m["clouds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            let cover = match c["cover"].as_str()? {
                "FEW" => 0.2,
                "SCT" => 0.45,
                "BKN" => 0.75,
                "OVC" | "OVX" | "VV" => 1.0,
                _ => return None,
            };
            Some((cover, (c["base"].as_f64()? / 3.28084 + elev) as f32))
        })
        .collect();
    layers.sort_by(|a, b| a.1.total_cmp(&b.1));
    let cb = tokens.iter().any(|t| t.ends_with("CB"));
    let tcu = tokens.iter().any(|t| t.ends_with("TCU"));
    let p = precip(wx);
    let storms: f32 = if p.thunder { 0.75 } else if cb { 0.45 } else if p.showers > 0.0 || tcu { 0.2 } else { 0.0 };
    let storms = storms.max(p.showers);
    let (clouds, base_m, alto) = match layers.first() {
        None => (0.03, None, 0.0),
        Some(&(_, base)) => {
            let low = layers.iter().filter(|l| l.1 < base + 1000.0).map(|l| l.0).fold(0.0f32, f32::max);
            let high = layers.iter().filter(|l| l.1 >= base + 1000.0).map(|l| l.0).fold(0.0f32, f32::max);
            (low, Some(base.clamp(300.0, 4000.0)), high)
        }
    };
    // Showers fall from cumulus: enough of it for the storm cells to grow.
    let clouds = if p.showers > 0.0 { clouds.max(0.45) } else { clouds };
    let foggy = ["FG", "BR", "HZ", "FU", "DU", "SA"].iter().any(|w| wx.contains(w));
    Some(Target {
        clouds,
        base_m,
        depth_m: depth(clouds, storms),
        alto,
        storms,
        showers_only: storms > 0.0 && !p.thunder && !cb,
        rain: p.rain,
        haze: haze(if cavok { None } else { vis_km }, foggy),
        wind,
        wind_from,
        gusts: gust.map(|g| ((g - wind) / 8.0).clamp(0.0, 1.0)).unwrap_or(0.1),
        qnh: m["altim"].as_f64().filter(|q| (850.0..1100.0).contains(q)),
        freezing: freezing_for(m["temp"].as_f64().filter(|t| (-80.0..60.0).contains(t)).map(|t| crate::snow::freezing_level(t, elev)), elev, p.snow),
        blowing: p.blowing,
        humidity: rh_from_td(m["temp"].as_f64(), m["dewp"].as_f64()),
        station,
        raw,
    })
}

/// What falls, from a METAR's present weather ("-SHRA BR", "+TSRA", "DZ").
#[derive(Debug, PartialEq)]
struct Precip {
    /// the rain deck (the Rain slider), 0..1: rain, or snow as heavy
    rain: f32,
    /// showers: the storm-cell slider they want (convective cloud), 0 = none
    showers: f32,
    thunder: bool,
    /// how much of what falls is snow, when the report says (None: nothing
    /// falls); 1 snow, snow grains or ice pellets, 0 rain or freezing rain
    snow: Option<f32>,
    /// snow blown up off the ground: BLSN 1, DRSN (drifting, low) 0.5
    blowing: f32,
}

/// Surface relative humidity (0..1) from temperature and dew point (°C) via Magnus-Tetens.
fn rh_from_td(temp_c: Option<f64>, dew_c: Option<f64>) -> Option<f32> {
    let (t, d) = (temp_c?, dew_c?);
    if !((-80.0..60.0).contains(&t) && (-90.0..60.0).contains(&d)) {
        return None;
    }
    let es = |c: f64| (17.625 * c / (243.04 + c)).exp();
    Some((es(d) / es(t)).clamp(0.05, 1.0) as f32)
}

/// The freezing level (m) moved, if need be, so that what the report says is
/// falling at the station (height `elev`, m) falls as that: snow, rain or a mix.
fn freezing_for(freezing: Option<f64>, elev: f64, snow: Option<f32>) -> Option<f64> {
    let Some(s) = snow else { return freezing };
    // (snow.rs `snow_share`: all snow 50 m under the freezing level, all rain 450 m under it.)
    let at = elev + 50.0 + (1.0 - s as f64) * 400.0;
    Some(match freezing {
        Some(f) if s >= 0.99 => f.min(at),
        Some(f) if s <= 0.01 => f.max(at),
        _ => at,
    })
}

/// Rain deck amount for each intensity (light, moderate, heavy): drizzle
/// lighter still. Showers and thunderstorms rain mostly from their cells, so
/// they bring a thinner deck.
const RAIN: [f32; 3] = [0.3, 0.55, 0.85];
const DRIZZLE: [f32; 3] = [0.12, 0.18, 0.25];
const CONVECTIVE_DECK: f32 = 0.5;

fn precip(wx: &str) -> Precip {
    let mut p = Precip { rain: 0.0, showers: 0.0, thunder: false, snow: None, blowing: 0.0 };
    // (rain amount, snow amount) falling here, to tell the mix.
    let (mut wet, mut white) = (0.0f32, 0.0f32);
    for tok in wx.split_whitespace() {
        let (level, rest) = match tok.as_bytes().first() {
            Some(b'-') => (0, &tok[1..]),
            Some(b'+') => (2, &tok[1..]),
            _ => (1, tok),
        };
        // In the vicinity (VC) or recent (RE): not falling here.
        let near = !(rest.starts_with("VC") || rest.starts_with("RE"));
        let codes: Vec<&str> = (0..rest.len() / 2).map(|i| &rest[2 * i..2 * i + 2]).collect();
        let has = |c: &str| codes.contains(&c);
        let ts = has("TS");
        let sh = has("SH");
        p.thunder |= ts;
        // Blowing (BL) or drifting (DR) snow is lifted off the ground, not falling.
        if has("BL") || has("DR") {
            if has("SN") && near {
                p.blowing = p.blowing.max(if has("BL") { 1.0 } else { 0.5 });
            }
            continue;
        }
        let mut r = 0.0f32;
        if has("RA") {
            r = RAIN[level];
        } else if has("DZ") {
            r = DRIZZLE[level];
        }
        // Snow (SN), snow grains (SG), ice pellets (PL), snow pellets (GS):
        // as thick as rain of the same strength to the eye.
        let w = if has("SN") || has("PL") { RAIN[level] } else if has("SG") || has("GS") { DRIZZLE[level] } else { 0.0 };
        if near {
            wet = wet.max(if has("FZ") { r.max(w) } else { r });
            if !has("FZ") {
                white = white.max(w);
            }
        }
        r = r.max(w);
        if sh && !ts {
            // stronger showers, taller and more cells
            p.showers = p.showers.max(if near { [0.25, 0.35, 0.5][level] } else { 0.2 });
        }
        if near && r > 0.0 {
            p.rain = p.rain.max(if ts || sh { r * CONVECTIVE_DECK } else { r });
        }
    }
    if wet + white > 0.0 {
        p.snow = Some(white / (wet + white));
    }
    p
}

/// "6+", "1/2", "1 1/2" statute miles.
fn sm(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches('+');
    s.split_whitespace().map(|p| match p.split_once('/') {
        Some((a, b)) => Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok()?),
        None => p.parse().ok(),
    }).sum()
}

/// Cloud layer thickness: towering with storms, a deck when overcast.
fn depth(clouds: f32, storms: f32) -> f32 {
    if storms > 0.3 { 6000.0 } else if clouds >= 0.75 { 2500.0 } else { 1800.0 }
}

/// The haze slider for a visibility (km). The sky model's clear air already
/// sees ~180 km; it cannot show less than ~11 km, so anything under ten
/// (mist, fog) takes the thickest haze.
fn haze(vis_km: Option<f64>, foggy: bool) -> f32 {
    match vis_km {
        None => 1.0,
        Some(v) if v >= 9.5 && !foggy => 1.0 + (2.5 * (1.0 - (v - 9.5) / 15.0)).clamp(0.0, 2.5) as f32,
        Some(v) => {
            let h = (3.912 / (v.max(0.1) * 1000.0) - 13.558e-6) / 8.396e-6;
            h.clamp(6.0, 40.0) as f32
        }
    }
}

/// Open-Meteo's model where there is no METAR near.
fn open_meteo(src: &Sources, (lat, lon): (f64, f64)) -> Result<Target, String> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat:.3}&longitude={lon:.3}&current=temperature_2m,dew_point_2m,cloud_cover_low,cloud_cover_mid,wind_speed_10m,wind_direction_10m,wind_gusts_10m,visibility,weather_code,rain,showers,snowfall,pressure_msl&wind_speed_unit=ms"
    );
    let j = get_json(src, &url)?;
    from_open_meteo(&j).ok_or_else(|| "unreadable Open-Meteo answer".to_string())
}

fn from_open_meteo(j: &serde_json::Value) -> Option<Target> {
    let c = &j["current"];
    let f = |k: &str| c[k].as_f64();
    let elev = j["elevation"].as_f64().unwrap_or(0.0);
    let code = f("weather_code").unwrap_or(0.0) as u32;
    let low = (f("cloud_cover_low").unwrap_or(0.0) / 100.0) as f32;
    let mid = (f("cloud_cover_mid").unwrap_or(0.0) / 100.0) as f32;
    // Cumulus base from the spread: about 125 m per degree.
    let base = match (f("temperature_2m"), f("dew_point_2m")) {
        (Some(t), Some(d)) => Some(((t - d).max(0.0) * 125.0 + elev).clamp(300.0, 4000.0) as f32),
        _ => None,
    };
    let storms = match code {
        95..=99 => 0.75,
        80 | 85 => 0.25,
        81 => 0.35,
        82 | 86 => 0.5,
        _ => 0.0,
    };
    // WMO weather codes: drizzle 51-57, rain 61-67, snow 71-77, showers 80-82,
    // snow showers 85-86, storms 95-99.
    let by_code = match code {
        71 => RAIN[0],
        73 => RAIN[1],
        75 => RAIN[2],
        77 => DRIZZLE[1],
        85 => RAIN[0] * CONVECTIVE_DECK,
        86 => RAIN[2] * CONVECTIVE_DECK,
        51 | 56 => DRIZZLE[0],
        53 => DRIZZLE[1],
        55 | 57 => DRIZZLE[2],
        61 | 66 => RAIN[0],
        63 => RAIN[1],
        65 | 67 => RAIN[2],
        80 | 95 => RAIN[0] * CONVECTIVE_DECK,
        81 | 96 => RAIN[1] * CONVECTIVE_DECK,
        82 | 99 => RAIN[2] * CONVECTIVE_DECK,
        _ => 0.0,
    };
    // The amount (mm/h) where the code says nothing falls: light under 2.5,
    // heavy from 7.6.
    let mm = f("rain").unwrap_or(0.0) + f("showers").unwrap_or(0.0);
    let by_amount = if mm < 0.1 { 0.0 } else if mm < 2.5 { RAIN[0] } else if mm < 7.6 { RAIN[1] } else { RAIN[2] };
    // Snowfall (cm/h): about a centimetre an hour is moderate snow.
    let cm = f("snowfall").unwrap_or(0.0);
    let by_snow = if cm < 0.05 { 0.0 } else if cm < 0.5 { RAIN[0] } else if cm < 2.0 { RAIN[1] } else { RAIN[2] };
    let rain = if by_code > 0.0 || (50..=99).contains(&code) { by_code } else { by_amount.max(by_snow) };
    let snow = match code {
        71..=77 | 85 | 86 => Some(1.0),
        51..=67 | 80..=82 | 95..=99 => Some(if cm > 0.05 && mm > 0.1 { 0.5 } else if cm > 0.05 { 1.0 } else { 0.0 }),
        _ if rain > 0.0 => Some(if mm < 0.1 { 1.0 } else if by_snow > 0.0 { 0.5 } else { 0.0 }),
        _ => None,
    };
    let clouds = if low > 0.05 { low } else { mid * 0.6 }.max(0.03);
    let clouds = if matches!(code, 80..=82 | 85 | 86) { clouds.max(0.45) } else { clouds };
    let wind = f("wind_speed_10m")?;
    Some(Target {
        clouds,
        base_m: base,
        depth_m: depth(clouds, storms),
        alto: mid,
        storms,
        showers_only: matches!(code, 80..=82 | 85 | 86),
        rain,
        haze: haze(f("visibility").map(|v| v / 1000.0), matches!(code, 45 | 48)),
        wind,
        wind_from: f("wind_direction_10m"),
        gusts: f("wind_gusts_10m").map(|g| ((g - wind) / 8.0).clamp(0.0, 1.0)).unwrap_or(0.1),
        qnh: f("pressure_msl").filter(|q| (850.0..1100.0).contains(q)),
        freezing: freezing_for(f("temperature_2m").filter(|t| (-80.0..60.0).contains(t)).map(|t| crate::snow::freezing_level(t, elev)), elev, snow),
        blowing: 0.0,
        humidity: rh_from_td(f("temperature_2m"), f("dew_point_2m")),
        station: "Open-Meteo".into(),
        raw: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metar() {
        let m = serde_json::json!({"icaoId":"EGLL","rawOb":"METAR EGLL 151720Z 24015G28KT 9999 -TSRA FEW012 BKN025CB OVC080 12/10 Q1002","altim":1002,"elev":25,"temp":12,"wdir":240,"wspd":15,"wgst":28,"visib":"6+","wxString":"-TSRA","clouds":[{"cover":"FEW","base":1200},{"cover":"BKN","base":2500},{"cover":"OVC","base":8000}]});
        let t = from_metar(&m).unwrap();
        assert_eq!((t.station.as_str(), t.qnh), ("EGLL", Some(1002.0)));
        // 12 °C at 25 m: the freezing level about 1850 m up (snow.rs).
        assert!((t.freezing.unwrap() - (25.0 + 12.0 / 0.0065)).abs() < 1e-6);
        assert_eq!(t.wind_from, Some(240.0));
        assert!((t.wind - 15.0 / 1.94384).abs() < 1e-6);
        assert!(t.gusts > 0.8);
        assert_eq!(t.storms, 0.75);
        assert_eq!(t.rain, RAIN[0] * CONVECTIVE_DECK);
        // FEW at 1200 ft and BKN at 2500 ft are one layer; OVC at 8000 ft is mid-level cloud.
        assert_eq!(t.clouds, 0.75);
        assert_eq!(t.alto, 1.0);
        assert!((t.base_m.unwrap() - (1200.0 / 3.28084 + 25.0) as f32).abs() < 0.5);
        assert!(t.haze < 2.0);

        let cavok = serde_json::json!({"icaoId":"EBBR","rawOb":"METAR EBBR 151720Z 19010KT CAVOK 10/09 Q0999","elev":56,"wdir":190,"wspd":10,"visib":"6+","clouds":[]});
        let t = from_metar(&cavok).unwrap();
        assert_eq!((t.clouds, t.base_m, t.storms, t.haze, t.rain), (0.03, None, 0.0, 1.0, 0.0));

        let fog = serde_json::json!({"icaoId":"LFPG","rawOb":"METAR LFPG 150600Z VRB02KT 0300 FG VV001 05/05 Q1020","wdir":"VRB","wspd":2,"visib":0.25,"wxString":"FG","clouds":[{"cover":"OVX","base":100}]});
        let t = from_metar(&fog).unwrap();
        assert_eq!(t.wind_from, None);
        assert_eq!(t.haze, 40.0);
        assert_eq!(t.base_m, Some(300.0));
        assert_eq!(t.rain, 0.0);

        let rain = serde_json::json!({"icaoId":"KOMA","rawOb":"METAR KOMA 151852Z 18012KT 3SM +RA BR OVC008 08/07 A2990","elev":299,"wdir":180,"wspd":12,"visib":3,"wxString":"+RA BR","clouds":[{"cover":"OVC","base":800}]});
        let t = from_metar(&rain).unwrap();
        assert_eq!((t.rain, t.storms), (RAIN[2], 0.0));

        let showers = serde_json::json!({"icaoId":"KLNK","rawOb":"METAR KLNK 151854Z 27010KT 10SM -SHRA SCT040 BKN060 15/09 A2995","elev":362,"wdir":270,"wspd":10,"visib":"10+","wxString":"-SHRA","clouds":[{"cover":"SCT","base":4000},{"cover":"BKN","base":6000}]});
        let t = from_metar(&showers).unwrap();
        assert_eq!((t.rain, t.storms), (RAIN[0] * CONVECTIVE_DECK, 0.25));
        assert!(t.clouds >= 0.45);
    }

    #[test]
    fn reports_set_the_freezing_level() {
        // +2 °C at 500 m would put the freezing level 300 m up, all snow there;
        // the report says rain: it goes up to where rain reaches the ground.
        let warm = freezing_for(Some(800.0), 500.0, Some(0.0)).unwrap();
        assert!((crate::snow::snow_share(500.0, warm)).abs() < 1e-9, "{warm}");
        // Snow reported at +3 °C (wet bulb): brought down to snow at the station.
        let s = freezing_for(Some(960.0), 500.0, Some(1.0)).unwrap();
        assert!((crate::snow::snow_share(500.0, s) - 1.0).abs() < 1e-9, "{s}");
        // A mix: half and half.
        let m = freezing_for(Some(3000.0), 500.0, Some(0.5)).unwrap();
        assert!((crate::snow::snow_share(500.0, m) - 0.5).abs() < 1e-9);
        // Nothing falling: the temperature alone; snow with no temperature: at the station.
        assert_eq!(freezing_for(Some(1234.0), 500.0, None), Some(1234.0));
        assert_eq!(freezing_for(None, 500.0, Some(1.0)), Some(550.0));
        // Open-Meteo: moderate snow at the grid cell.
        let j = serde_json::json!({"elevation":37.0,"current":{"temperature_2m":1.0,"dew_point_2m":0.0,"cloud_cover_low":100,"cloud_cover_mid":100,"wind_speed_10m":17.0,"wind_direction_10m":20,"wind_gusts_10m":25.0,"visibility":400.0,"weather_code":73,"rain":0.0,"showers":0.0,"snowfall":1.2,"pressure_msl":1008.0}});
        let t = from_open_meteo(&j).unwrap();
        assert_eq!(t.rain, RAIN[1]);
        assert!(crate::snow::snow_share(37.0, t.freezing.unwrap()) > 0.99);
    }

    #[test]
    fn present_weather() {
        let p = |s| precip(s);
        assert_eq!(p("").rain, 0.0);
        assert_eq!(p("BR HZ").rain, 0.0);
        assert_eq!(p("-RA").rain, RAIN[0]);
        assert_eq!(p("RA").rain, RAIN[1]);
        assert_eq!(p("+RA").rain, RAIN[2]);
        assert_eq!(p("-DZ").rain, DRIZZLE[0]);
        assert_eq!(p("DZ BR").rain, DRIZZLE[1]);
        assert_eq!(p("-RADZ").rain, RAIN[0]);
        assert_eq!(p("-DZ RA").rain, RAIN[1]);
        assert!(p("+DZ").rain < p("-RA").rain);
        assert_eq!(p("-RASN").rain, RAIN[0]);
        assert_eq!(p("-RASN").snow, Some(0.5));
        assert_eq!((p("SN").rain, p("SN").snow), (RAIN[1], Some(1.0)));
        assert_eq!((p("+SHSN").rain, p("+SHSN").showers, p("+SHSN").snow), (RAIN[2] * CONVECTIVE_DECK, 0.5, Some(1.0)));
        assert_eq!(p("-FZRA").snow, Some(0.0));
        assert_eq!(p("RA").snow, Some(0.0));
        assert_eq!(p("BR").snow, None);
        // Blowing snow alone: nothing falls, but it blows.
        let bl = p("BLSN");
        assert_eq!((bl.rain, bl.snow, bl.blowing), (0.0, None, 1.0));
        let bz = p("+SN BLSN");
        assert_eq!((bz.rain, bz.snow, bz.blowing), (RAIN[2], Some(1.0), 1.0));
        assert_eq!(p("-SN DRSN").blowing, 0.5);
        assert_eq!(p("VCBLSN").blowing, 0.0);
        let sh = p("+SHRA");
        assert_eq!((sh.rain, sh.showers, sh.thunder), (RAIN[2] * CONVECTIVE_DECK, 0.5, false));
        let ts = p("TSRA");
        assert_eq!((ts.rain, ts.showers, ts.thunder), (RAIN[1] * CONVECTIVE_DECK, 0.0, true));
        assert!(p("TS").thunder && p("TS").rain == 0.0);
        let vc = p("VCSH");
        assert_eq!((vc.rain, vc.showers), (0.0, 0.2));
        assert_eq!(p("RERA").rain, 0.0);
        assert_eq!(p("FZRA").rain, RAIN[1]);
    }

    #[test]
    fn visibility_text() {
        assert_eq!(sm("6+"), Some(6.0));
        assert_eq!(sm("1/2"), Some(0.5));
        assert_eq!(sm("1 1/2"), Some(1.5));
    }

    #[test]
    fn open_meteo_model() {
        let j = serde_json::json!({"elevation":37.0,"current":{"temperature_2m":10.0,"dew_point_2m":8.0,"cloud_cover_low":100,"cloud_cover_mid":40,"wind_speed_10m":6.6,"wind_direction_10m":203,"wind_gusts_10m":13.1,"visibility":19420.0,"weather_code":53}});
        let t = from_open_meteo(&j).unwrap();
        assert_eq!((t.clouds, t.alto, t.storms, t.rain), (1.0, 0.4, 0.0, DRIZZLE[1]));
        assert_eq!(t.base_m, Some(300.0));
        assert_eq!(t.wind_from, Some(203.0));
        assert_eq!(t.station, "Open-Meteo");
        // 10 °C at the grid cell's 37 m.
        assert!((t.freezing.unwrap() - (37.0 + 10.0 / 0.0065)).abs() < 1e-6);

        let om = |code: u32, rain: f64, showers: f64| {
            let j = serde_json::json!({"elevation":0.0,"current":{"cloud_cover_low":30,"cloud_cover_mid":0,"wind_speed_10m":3.0,"weather_code":code,"rain":rain,"showers":showers}});
            from_open_meteo(&j).unwrap()
        };
        assert_eq!(om(0, 0.0, 0.0).rain, 0.0);
        assert_eq!(om(3, 0.0, 0.0).rain, 0.0);
        assert_eq!(om(61, 0.5, 0.0).rain, RAIN[0]);
        assert_eq!(om(65, 9.0, 0.0).rain, RAIN[2]);
        // a cloudy code with rain falling anyway: the amount decides
        assert_eq!(om(3, 4.0, 0.0).rain, RAIN[1]);
        let t = om(81, 0.0, 3.0);
        assert_eq!((t.rain, t.storms), (RAIN[1] * CONVECTIVE_DECK, 0.35));
        assert!(t.clouds >= 0.45);
        assert_eq!(om(95, 0.0, 5.0).storms, 0.75);
        // fog (45) with a trace of rain in the model: no deck
        assert_eq!(om(45, 0.05, 0.0).rain, 0.0);
    }
}
