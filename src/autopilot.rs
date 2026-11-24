//! The autopilot: one for every aircraft, as the simpler general-aviation
//! and airliner ones work.
//!
//! Lateral modes hold the wings level (ROL), fly a heading (HDG), track the
//! NAV 1 course, a VOR radial or a localizer (NAV), or the localizer and
//! glide slope of an ILS (APR). Vertical modes hold the pitch (PIT), an
//! altitude (ALT), a vertical speed (VS, capturing the preselected altitude:
//! ALT*), or the glide slope (GS). An autothrottle (AT) holds an airspeed.
//!
//! Outer loops give a bank and a vertical speed; inner loops give the stick,
//! with gains divided by the dynamic pressure relative to the aircraft's
//! cruise (the surfaces bite harder fast than slow), so one set of gains
//! flies a Cessna and a 747. The pilot's own small inputs add to it; a firm
//! one, the ground, a stall or an unusual attitude disconnects it.
use crate::geo;
use crate::nav::{NavReading, Station};
use crate::plane::Plane;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lat {
    Roll,
    Hdg,
    Nav,
    Apr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vert {
    Pitch,
    Alt,
    Vs,
    Gs,
}

/// Why the autopilot let go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Off {
    Pilot,
    Ground,
    Stall,
    Attitude,
}

#[derive(Clone, Debug)]
pub struct Autopilot {
    pub on: bool,
    pub lat: Lat,
    pub vert: Vert,
    /// heading bug (degrees true)
    pub hdg: f64,
    /// preselected altitude (m)
    pub alt: f64,
    /// vertical speed wanted in VS (m/s)
    pub vs: f64,
    /// autothrottle on, and the airspeed it holds (IAS, m/s)
    pub at: bool,
    pub spd: f64,
    /// the pitch held in PIT (rad)
    pitch_hold: f64,
    /// NAV/APR: the course is captured (else HDG flies until it is)
    pub nav_cap: bool,
    /// APR: the glide slope is captured
    pub gs_cap: bool,
    /// VS: the preselected altitude is being captured (ALT*)
    pub alt_cap: bool,
    /// the altitude ALT holds once captured (m)
    alt_hold: f64,
    /// low-speed protection is lowering the nose, high-speed raising it
    pub slow: bool,
    pub fast: bool,
    /// integrators: angle of attack the pitch carries, crosswind correction, pitch trim, power
    i_aoa: f64,
    i_xw: f64,
    i_pitch: f64,
    i_thr: f64,
}

impl Default for Autopilot {
    fn default() -> Self {
        Autopilot { on: false, lat: Lat::Roll, vert: Vert::Pitch, hdg: 0.0, alt: 0.0, vs: 0.0, at: false, spd: 0.0, pitch_hold: 0.0, nav_cap: false, gs_cap: false, alt_cap: false, alt_hold: 0.0, slow: false, fast: false, i_aoa: 0.0, i_xw: 0.0, i_pitch: 0.0, i_thr: 0.0 }
    }
}

/// What the aircraft is doing, in the terms the autopilot works in.
#[derive(Clone, Copy, Debug)]
pub struct State {
    /// heading and track over the ground (degrees true), bank (rad, + right), pitch (rad, + up)
    pub hdg: f64,
    pub trk: f64,
    pub bank: f64,
    pub pitch: f64,
    /// roll rate (+ right) and pitch rate (+ up), rad/s
    pub roll_rate: f64,
    pub pitch_rate: f64,
    /// altitude (m), vertical speed (m/s), ground speed and true airspeed (m/s), IAS (m/s)
    pub alt: f64,
    pub vs: f64,
    pub gs: f64,
    pub tas: f64,
    pub ias: f64,
    pub throttle: f64,
}

impl State {
    pub fn of(p: &Plane) -> State {
        let (e, n, u) = geo::enu(p.pos);
        let f = p.forward();
        let hdg = f.dot(e).atan2(f.dot(n)).to_degrees().rem_euclid(360.0);
        let bank = (-p.right().dot(u)).clamp(-1.0, 1.0).asin();
        let pitch = f.dot(u).clamp(-1.0, 1.0).asin();
        let vs = p.vel.dot(u);
        let flat = p.vel - u * vs;
        let gs = flat.length();
        let trk = if gs > 5.0 { flat.dot(e).atan2(flat.dot(n)).to_degrees().rem_euclid(360.0) } else { hdg };
        State { hdg, trk, bank, pitch, roll_rate: -p.omega.z, pitch_rate: p.omega.x, alt: geo::pos_to_lla(p.pos).2, vs, gs, tas: p.airspeed().length(), ias: p.ias(), throttle: p.throttle }
    }
}

/// What the autopilot asks for.
#[derive(Clone, Copy, Debug, Default)]
pub struct Out {
    pub pitch: f64,
    pub roll: f64,
    pub throttle_set: Option<f64>,
}

/// The radio NAV 1 gives: the station, what it reads, and the OBS.
pub struct Nav<'a> {
    pub st: &'a Station,
    pub reading: NavReading,
    pub obs: f64,
}

fn wrap(a: f64) -> f64 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// Most bank in HDG, and when tracking a course (degrees).
const BANK_MAX: f64 = 25.0;
const BANK_NAV: f64 = 20.0;
/// Most vertical speed when holding or capturing an altitude (m/s).
const VS_ALT: f64 = 7.5;
/// Glide slope angle (degrees).
const GS_DEG: f64 = 3.0;

impl Autopilot {
    /// Engages it in ROL and PIT (or keeps the modes chosen beforehand),
    /// taking up the heading, altitude and speed of the moment where unset.
    pub fn engage(&mut self, s: &State) {
        self.on = true;
        self.pitch_hold = s.pitch;
        self.i_aoa = s.pitch - (s.vs / s.gs.max(1.0)).atan();
        self.i_pitch = 0.0;
        self.i_xw = 0.0;
        self.i_thr = s.throttle;
        self.alt_hold = s.alt;
        if self.vert == Vert::Alt {
            self.alt_hold = s.alt;
        }
        if self.spd <= 0.0 {
            self.spd = s.ias;
        }
    }

    pub fn disengage(&mut self) {
        self.on = false;
        self.at = false;
        self.nav_cap = false;
        self.gs_cap = false;
        self.alt_cap = false;
    }

    /// The heading bug and altitude to the aircraft's own.
    pub fn sync(&mut self, s: &State) {
        self.hdg = s.hdg.round();
        self.alt = (s.alt * 3.28084 / 100.0).round() * 100.0 / 3.28084;
    }

    pub fn set_lat(&mut self, m: Lat, s: &State) {
        self.lat = m;
        self.nav_cap = false;
        self.i_xw = 0.0;
        if m == Lat::Roll {
            self.hdg = s.hdg.round();
        }
        if m != Lat::Apr {
            self.gs_cap = false;
            if self.vert == Vert::Gs {
                self.set_vert(Vert::Alt, s);
            }
        }
    }

    pub fn set_vert(&mut self, m: Vert, s: &State) {
        self.vert = m;
        self.alt_cap = false;
        match m {
            Vert::Pitch => self.pitch_hold = s.pitch,
            Vert::Alt => self.alt_hold = s.alt,
            Vert::Vs => {
                if self.vs == 0.0 {
                    self.vs = (s.vs * 196.85 / 100.0).round() * 100.0 / 196.85;
                }
            }
            Vert::Gs => {}
        }
    }

    /// One step: what to do with the stick and throttle, or why it let go.
    /// `pilot` is the pilot's own pitch and roll input (-1..1).
    pub fn step(&mut self, s: &State, p: &Plane, nav: Option<&Nav>, pilot: (f64, f64), dt: f64) -> Result<Out, Off> {
        if pilot.0.abs() > 0.5 || pilot.1.abs() > 0.5 {
            return Err(Off::Pilot);
        }
        if p.on_ground {
            return Err(Off::Ground);
        }
        if p.stalled {
            return Err(Off::Stall);
        }
        if s.bank.abs() > 50f64.to_radians() || s.pitch.abs() > 30f64.to_radians() {
            return Err(Off::Attitude);
        }
        // The surfaces bite as the dynamic pressure, so as the indicated
        // airspeed squared: gains go as its inverse, against a typical
        // indicated speed of the type's (its cruise is a true airspeed up high).
        let q = (s.ias / (p.spec.approach.0 * 1.8).max(20.0)).powi(2).clamp(0.25, 8.0);

        // ---- lateral: a bank wanted
        let mut want_hdg = None;
        let mut bank_max = BANK_MAX;
        let course_nav = nav.filter(|n| n.reading.live).map(|n| (if n.st.kind == crate::nav::Kind::Ils { n.st.course } else { n.obs }, n.reading.cdi, n.st.kind == crate::nav::Kind::Ils));
        match self.lat {
            Lat::Roll => {}
            Lat::Hdg => want_hdg = Some(self.hdg),
            Lat::Nav | Lat::Apr => {
                match course_nav {
                    Some((course, cdi, ils)) if self.lat == Lat::Nav || ils => {
                        if !self.nav_cap && cdi.abs() < 0.8 {
                            self.nav_cap = true;
                            self.i_xw = 0.0;
                        }
                        if self.nav_cap {
                            // A track over the ground: the course, turned towards it by the
                            // needle. Flying the track (not the heading) takes the wind's
                            // drift at once; the integral only mops up what is left, slowly
                            // (a fast one sets a slow-rolling airliner swinging across the course).
                            let k = if ils { 30.0 } else { 40.0 };
                            self.i_xw = (self.i_xw + cdi * if ils { 0.4 } else { 0.25 } * dt).clamp(-10.0, 10.0);
                            let want_trk = course + (cdi * k).clamp(-30.0, 30.0) + self.i_xw;
                            want_hdg = Some(s.hdg + wrap(want_trk - s.trk));
                            bank_max = BANK_NAV;
                        } else {
                            want_hdg = Some(self.hdg);
                        }
                    }
                    _ => {
                        self.nav_cap = false;
                        want_hdg = Some(self.hdg);
                    }
                }
            }
        }
        let bank_want = match want_hdg {
            Some(h) => (wrap(h - s.hdg) * 1.2).clamp(-bank_max, bank_max).to_radians(),
            None => 0.0,
        };
        let roll = ((bank_want - s.bank) * 2.5 - s.roll_rate * 1.2) / q;

        // ---- vertical: a vertical speed wanted
        if self.lat == Lat::Apr && !self.gs_cap {
            if let Some(Nav { reading: NavReading { gs: Some(g), .. }, .. }) = nav {
                // From below as the needle centres, or from above (the slope is below: fly down to it).
                if self.nav_cap && *g < 0.3 {
                    self.gs_cap = true;
                    self.vert = Vert::Gs;
                }
            }
        }
        let vs_want = match self.vert {
            Vert::Pitch => None,
            Vert::Alt => Some(((self.alt_hold - s.alt) * 0.12).clamp(-VS_ALT, VS_ALT)),
            Vert::Vs => {
                // Capture the preselected altitude once near enough for a smooth level-off.
                let to_go = self.alt - s.alt;
                if self.alt_cap || (self.vs != 0.0 && to_go * self.vs.signum() < (self.vs.abs() * 8.0).max(30.0)) {
                    self.alt_cap = true;
                    Some((to_go * 0.12).clamp(-self.vs.abs().max(1.0), self.vs.abs().max(1.0)))
                } else {
                    Some(self.vs)
                }
            }
            Vert::Gs => match nav.and_then(|n| n.reading.gs) {
                Some(g) => {
                    let nominal = s.gs * GS_DEG.to_radians().tan();
                    Some((-nominal + g * 2.5).clamp(-2.0 * nominal - 1.0, 0.5))
                }
                None => Some(0.0),
            },
        };
        if self.vert == Vert::Vs && self.alt_cap && (self.alt - s.alt).abs() < 15.0 {
            self.vert = Vert::Alt;
            self.alt_hold = self.alt;
            self.alt_cap = false;
        }
        // High-speed protection: past 95 % of the never-exceed speed it
        // climbs, as fast as it is over, to trade the speed for height.
        let v_max = p.spec.vne * 0.95;
        self.fast = s.ias > v_max;
        let vs_want = match vs_want {
            Some(v) if self.fast => Some(v.max(((s.ias - v_max) * 0.4).min(15.0))),
            v => v,
        };
        let mut pitch_want = match vs_want {
            None => self.pitch_hold,
            Some(v) => {
                let v_air = s.tas.max(15.0);
                // The angle of attack that this needs is learned slowly.
                self.i_aoa = (self.i_aoa + (v - s.vs) / v_air * 0.3 * dt).clamp(-0.25, 0.3);
                ((v / v_air).atan() + self.i_aoa + (v - s.vs) / v_air * 0.5).clamp(-15f64.to_radians(), 18f64.to_radians())
            }
        };
        // Low-speed protection: near the stall it gives up the vertical speed
        // (the aircraft lacks the power for it) and lowers the nose instead.
        let a_max = p.stall_alpha() - 4f64.to_radians();
        self.slow = p.alpha > a_max;
        if self.slow {
            pitch_want = pitch_want.min(s.pitch - (p.alpha - a_max) * 2.0);
            // (and keeps its learned angle of attack from winding up)
            self.i_aoa = self.i_aoa.min(s.pitch - (s.vs / s.tas.max(15.0)).atan());
        }
        let e = pitch_want - s.pitch;
        self.i_pitch = (self.i_pitch + e * 1.5 / q * dt).clamp(-0.6, 0.6);
        // (A banked aircraft needs more pull for the same pitch.)
        let turn = (1.0 / s.bank.cos().max(0.5) - 1.0) * 0.6;
        let pitch = ((e * 4.0 - s.pitch_rate * 1.5) / q + self.i_pitch + turn).clamp(-1.0, 1.0);

        // ---- autothrottle
        let throttle_set = self.at.then(|| {
            let err = self.spd - s.ias;
            self.i_thr = (self.i_thr + err * 0.02 * dt).clamp(0.0, 1.0);
            (self.i_thr + err * 0.06).clamp(0.0, 1.0)
        });

        Ok(Out { pitch: (pitch + pilot.0).clamp(-1.0, 1.0), roll: (roll + pilot.1).clamp(-1.0, 1.0), throttle_set })
    }

    /// The annunciator line: active modes, then armed ones (in brackets).
    pub fn annunciation(&self) -> (Vec<String>, Vec<String>) {
        let ft = |m: f64| format!("{:.0}", m * 3.28084);
        let mut on = Vec::new();
        let mut armed = Vec::new();
        on.push(match self.lat {
            Lat::Roll => "ROL".into(),
            Lat::Hdg => format!("HDG {:03.0}", self.hdg.rem_euclid(360.0)),
            Lat::Nav if self.nav_cap => "NAV".into(),
            Lat::Apr if self.nav_cap => "LOC".into(),
            Lat::Nav | Lat::Apr => format!("HDG {:03.0}", self.hdg.rem_euclid(360.0)),
        });
        if matches!(self.lat, Lat::Nav | Lat::Apr) && !self.nav_cap {
            armed.push(if self.lat == Lat::Nav { "NAV" } else { "LOC" }.into());
        }
        on.push(match self.vert {
            Vert::Pitch => "PIT".into(),
            Vert::Alt => format!("ALT {}", ft(self.alt_hold)),
            Vert::Vs if self.alt_cap => format!("ALT* {}", ft(self.alt)),
            Vert::Vs => format!("VS {:+.0}", self.vs * 196.85),
            Vert::Gs => "GS".into(),
        });
        if self.lat == Lat::Apr && !self.gs_cap {
            armed.push("GS".into());
        }
        if self.vert == Vert::Vs && !self.alt_cap {
            armed.push(format!("ALT {}", ft(self.alt)));
        }
        if self.at {
            on.push(format!("SPD {:.0}", self.spd * 1.943_84));
        }
        if self.slow {
            on.push("MIN SPD".into());
        }
        if self.fast {
            on.push("MAX SPD".into());
        }
        (on, armed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aircraft;
    use crate::nav::{nav_reading, Kind};
    use crate::plane::Controls;
    use crate::world::World;
    use std::sync::Arc;

    fn flat() -> World {
        let dir = std::env::temp_dir().join("fablegame-test-cache");
        let src = Arc::new(crate::sources::Sources::new(crate::fetch::Fetcher::new(dir, true), String::new()));
        let mut w = World::new(crate::sources::Loader::new(src, 0), 14);
        w.flat = Some(0.0);
        w
    }

    const LAT: f64 = 50.9;
    const LON: f64 = 4.5;

    /// Flies `secs` with the autopilot; calls `each` every second.
    fn fly(p: &mut Plane, ap: &mut Autopilot, w: &World, secs: f64, st: Option<&Station>, obs: f64, mut each: impl FnMut(&Plane, &Autopilot, &State) -> bool) -> Result<(), Off> {
        let dt = 1.0 / 120.0;
        for k in 0..(secs / dt) as usize {
            let s = State::of(p);
            let nav = st.map(|st| Nav { st, reading: nav_reading(st, obs, p.pos), obs });
            let o = ap.step(&s, p, nav.as_ref(), (0.0, 0.0), dt)?;
            let c = Controls { pitch: o.pitch, roll: o.roll, throttle_set: o.throttle_set, throttle: 0.0, ..Default::default() };
            if p.step(dt, &c, w).is_some() {
                panic!("{} crashed", p.spec.name);
            }
            p.notes.clear();
            p.shed.clear();
            if k % 120 == 0 && each(p, ap, &State::of(p)) {
                return Ok(());
            }
        }
        Ok(())
    }

    fn planes() -> Vec<&'static crate::aircraft::Spec> {
        aircraft::BUILTIN.iter().copied().filter(|s| s.rocket().is_none()).collect()
    }

    /// In the air, level, `alt` metres up, heading north, at a speed it
    /// would fly down low (not its cruise up high: Mach 3 at 5000 ft is no
    /// flight anyone makes), engaged with the autothrottle holding it.
    fn cruising(spec: &'static crate::aircraft::Spec, alt: f64) -> (Plane, Autopilot) {
        let v = (spec.cruise * 0.85).min(spec.approach.0 * 2.0);
        let mut p = Plane::spawn(spec, LAT.to_radians(), LON.to_radians(), alt, 0.0, v);
        p.throttle = 0.7;
        let mut ap = Autopilot::default();
        ap.spd = v;
        ap.at = true;
        let s = State::of(&p);
        ap.sync(&s);
        ap.engage(&s);
        ap.set_lat(Lat::Hdg, &s);
        ap.set_vert(Vert::Alt, &s);
        (p, ap)
    }

    #[test]
    fn holds_altitude_heading_and_speed() {
        let w = flat();
        for spec in planes() {
            let (mut p, mut ap) = cruising(spec, 1500.0);
            fly(&mut p, &mut ap, &w, 30.0, None, 0.0, |_, _, _| false).unwrap();
            let mut worst = (0.0f64, 0.0f64, 0.0f64);
            fly(&mut p, &mut ap, &w, 120.0, None, 0.0, |_, ap, s| {
                worst.0 = worst.0.max((s.alt - 1500.0).abs());
                worst.1 = worst.1.max(wrap(s.hdg - ap.hdg).abs());
                worst.2 = worst.2.max((s.ias - ap.spd).abs());
                false
            })
            .unwrap_or_else(|e| panic!("{}: let go ({e:?})", spec.name));
            println!("{:16} alt ±{:.0} ft  hdg ±{:.1}°  ias ±{:.1} kt", spec.name, worst.0 * 3.28, worst.1, worst.2 * 1.94);
            assert!(worst.0 * 3.28 < 50.0, "{}: altitude off by {:.0} ft", spec.name, worst.0 * 3.28);
            assert!(worst.1 < 3.0, "{}: heading off by {:.1}°", spec.name, worst.1);
            assert!(worst.2 * 1.94 < 8.0, "{}: speed off by {:.1} kt", spec.name, worst.2 * 1.94);
        }
    }

    #[test]
    fn turns_to_a_heading() {
        let w = flat();
        for spec in planes() {
            let (mut p, mut ap) = cruising(spec, 1500.0);
            fly(&mut p, &mut ap, &w, 20.0, None, 0.0, |_, _, _| false).unwrap();
            ap.hdg = 90.0;
            let (mut bank, mut past, mut alt) = (0.0f64, 0.0f64, 0.0f64);
            // (Long enough for the fast ones: a 25° bank turns them slowly.)
            fly(&mut p, &mut ap, &w, 150.0, None, 0.0, |_, _, s| {
                bank = bank.max(s.bank.to_degrees().abs());
                past = past.max(wrap(s.hdg - 90.0));
                alt = alt.max((s.alt - 1500.0).abs());
                false
            })
            .unwrap_or_else(|e| panic!("{}: let go ({e:?})", spec.name));
            let s = State::of(&p);
            println!("{:16} hdg {:.1}  bank max {bank:.0}°  overshoot {past:.1}°  alt ±{:.0} ft", spec.name, s.hdg, alt * 3.28);
            assert!(wrap(s.hdg - 90.0).abs() < 2.0, "{}: heading {:.1}", spec.name, s.hdg);
            assert!(bank < BANK_MAX + 4.0, "{}: banked {bank:.0}°", spec.name);
            assert!(past < 5.0, "{}: overshot by {past:.1}°", spec.name);
            assert!(alt * 3.28 < 120.0, "{}: altitude off by {:.0} ft in the turn", spec.name, alt * 3.28);
        }
    }

    #[test]
    fn climbs_and_captures_the_altitude() {
        let w = flat();
        for spec in planes() {
            let (mut p, mut ap) = cruising(spec, 1000.0);
            let s = State::of(&p);
            ap.alt = 1000.0 + 600.0;
            ap.vs = 3.5;
            ap.set_vert(Vert::Vs, &s);
            let mut over = 0.0f64;
            fly(&mut p, &mut ap, &w, 400.0, None, 0.0, |_, _, s| {
                over = over.max(s.alt - 1600.0);
                false
            }).unwrap_or_else(|e| panic!("{}: let go ({e:?})", spec.name));
            let s = State::of(&p);
            println!("{:16} alt {:.0} ft  {:?}  overshoot {:.0} ft", spec.name, s.alt * 3.28, ap.vert, over * 3.28);
            assert_eq!(ap.vert, Vert::Alt, "{}: never captured ({:.0} m)", spec.name, s.alt);
            assert!((s.alt - 1600.0).abs() * 3.28 < 50.0, "{}: {:.0} ft", spec.name, s.alt * 3.28);
            assert!(over * 3.28 < 120.0, "{}: overshot by {:.0} ft", spec.name, over * 3.28);
        }
    }

    /// An ILS for a runway 10 km south of the start, landing north; the
    /// aircraft starts off to the side, heading across the localizer.
    fn ils() -> Station {
        let thr_lat = LAT + 0.15;
        let far = geo::pos_to_lla(geo::lla_to_pos(thr_lat.to_radians(), LON.to_radians(), 0.0) + geo::enu(geo::lla_to_pos(thr_lat.to_radians(), LON.to_radians(), 0.0)).1 * 3000.0);
        Station { kind: Kind::Ils, ident: "36".into(), name: "TEST".into(), freq_khz: 0, lat: far.0.to_degrees(), lon: far.1.to_degrees(), elev: 0.0, dme: true, course: 0.0, thr: (thr_lat, LON), aim: 300.0 }
    }

    #[test]
    fn flies_an_ils_approach() {
        ils_approach(0.0);
    }

    #[test]
    fn flies_an_ils_approach_in_a_crosswind() {
        // 15 kt from the left of the approach.
        ils_approach(15.0 / 1.943_84);
    }

    fn ils_approach(crosswind: f64) {
        let w = flat();
        let st = ils();
        for spec in planes() {
            // 25 km out (≈ 13.5 NM), 2.5 km to the east of the centreline, 2500 ft, heading 330: an intercept.
            let start = geo::lla_to_pos((LAT + 0.15).to_radians(), LON.to_radians(), 0.0);
            let (e, n, _) = geo::enu(start);
            let at = geo::pos_to_lla(start - n * 22_000.0 + e * 2500.0);
            let v = spec.approach.0;
            let mut p = Plane::spawn(spec, at.0, at.1, 760.0, 330f64.to_radians(), v);
            // In the approach configuration: its flaps, the gear down.
            p.configure(spec.approach.1, true, 0.0);
            // (The wind blows from the west, across the northbound approach.)
            p.wind = geo::enu(p.pos).0 * crosswind;
            p.vel += p.wind;
            let mut ap = Autopilot { spd: v, at: true, hdg: 330.0, ..Default::default() };
            let s = State::of(&p);
            ap.engage(&s);
            ap.set_vert(Vert::Alt, &s);
            ap.set_lat(Lat::Apr, &s);
            let mut worst = (0.0f64, 0.0f64);
            fly(&mut p, &mut ap, &w, 900.0, Some(&st), 0.0, |p, ap, s| {
                let r = nav_reading(&st, 0.0, p.pos);
                let agl = s.alt;
                if ap.gs_cap && agl > 60.0 && agl < 450.0 {
                    worst.0 = worst.0.max(r.cdi.abs());
                    worst.1 = worst.1.max(r.gs.unwrap_or(1.0).abs());
                }
                // Down to 200 ft: the pilot lands it.
                agl < 60.0
            })
            .unwrap_or_else(|e| panic!("{}: let go ({e:?})", spec.name));
            println!("{:16} wind {crosswind:.0} m/s {:?}/{:?} loc ±{:.2}  gs ±{:.2}  alt {:.0} ft", spec.name, ap.lat, ap.vert, worst.0, worst.1, State::of(&p).alt * 3.28);
            assert!(ap.gs_cap, "{}: never captured the glide slope", spec.name);
            assert!(worst.0 < 0.25, "{}: localizer off by {:.2} of full scale", spec.name, worst.0);
            assert!(worst.1 < 0.5, "{}: glide slope off by {:.2} of full scale", spec.name, worst.1);
        }
    }

    /// A slow single climbing into a strong headwind with the power it had
    /// (no autothrottle): it must climb, not pull itself into a stall.
    #[test]
    fn climbs_into_a_headwind_without_stalling() {
        let w = flat();
        for spec in planes() {
            let (mut p, mut ap) = cruising(spec, 900.0);
            ap.at = false;
            p.wind = geo::enu(p.pos).1 * -15.0;
            p.vel += p.wind;
            let s = State::of(&p);
            ap.alt = 900.0 + 300.0;
            ap.vs = 2.5;
            ap.set_vert(Vert::Vs, &s);
            let mut lo = f64::MAX;
            fly(&mut p, &mut ap, &w, 240.0, None, 0.0, |_, _, s| {
                lo = lo.min(s.ias);
                false
            })
            .unwrap_or_else(|e| panic!("{}: let go ({e:?}), slowest {:.0} kt", spec.name, lo * 1.94));
            println!("{:16} climbed to {:.0} ft, slowest {:.0} kt", spec.name, State::of(&p).alt * 3.28, lo * 1.94);
        }
    }

    #[test]
    fn a_firm_input_lets_go() {
        let w = flat();
        let (p, mut ap) = cruising(&crate::aircraft::c172::SPEC, 1500.0);
        let _ = &w;
        let s = State::of(&p);
        assert_eq!(ap.step(&s, &p, None, (0.0, 0.8), 0.01).unwrap_err(), Off::Pilot);
        assert!(ap.step(&s, &p, None, (0.1, 0.0), 0.01).is_ok());
    }
}
