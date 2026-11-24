//! The copilot's game-side half: read the live aircraft, weather, radios and
//! traffic, and carry out the tool calls the MCP bridge sends us.
//!
//! Everything here runs on the game's main thread, from `Game::pump_mcp`, so
//! the handlers can use the aircraft, the world and the weather directly.  A
//! tool call takes at most a frame (except a screenshot, which waits for the
//! next rendered frame and is answered from `finish_mcp_shots`).

use crate::airports;
use crate::geo;
use crate::nav;
use crate::traffic;
use crate::{Game, Mode};
use fablegame::mcp::{rpc, tools};
use glam::{DVec3, Vec2};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A screenshot waiting for the next rendered frame.
pub(crate) struct PendingShot {
    path: PathBuf,
    reply: crossbeam_channel::Sender<Value>,
    restore_cam: Option<u8>,
    restore_hud: Option<bool>,
    view: String,
    hud: bool,
    deadline: Instant,
}

fn num(x: f64) -> Value {
    if x.is_finite() { json!(x) } else { Value::Null }
}

fn num_opt(x: Option<f64>) -> Value {
    x.map(num).unwrap_or(Value::Null)
}

/// Idents and frequencies are reused worldwide; tune the closest match.
fn nearest_station(pos: DVec3, matches: impl Fn(&nav::Station) -> bool) -> Option<nav::Station> {
    nav::db()
        .iter()
        .filter(|st| matches(st))
        .min_by(|a, b| (a.pos() - pos).length_squared().total_cmp(&(b.pos() - pos).length_squared()))
        .cloned()
}

fn kind_name(k: nav::Kind) -> &'static str {
    match k {
        nav::Kind::Vor => "VOR",
        nav::Kind::Ndb => "NDB",
        nav::Kind::Ils => "ILS",
    }
}

fn gear_text(gear: f64) -> &'static str {
    if gear >= 0.999 {
        "down"
    } else if gear <= 0.001 {
        "up"
    } else {
        "transit"
    }
}

fn wind_from_deg(w: DVec3, pos: DVec3) -> f64 {
    let (e, n, _) = geo::enu(pos);
    let to = w.dot(e).atan2(w.dot(n)).to_degrees();
    (to + 180.0).rem_euclid(360.0)
}

fn station_json(st: &nav::Station) -> Value {
    json!({
        "kind": kind_name(st.kind),
        "ident": st.ident,
        "name": st.name,
        "frequency": st.freq_text(),
        "frequency_khz": st.freq_khz,
        "latitude_deg": st.lat,
        "longitude_deg": st.lon,
        "elevation_ft": st.elev * 3.28084,
        "dme": st.dme,
        "course_deg": st.course,
    })
}

fn nav_reading_json(st: &nav::Station, obs: f64, pos: DVec3) -> Value {
    let r = nav::nav_reading(st, obs, pos);
    json!({
        "live": r.live,
        "cdi": num(r.cdi),
        "to_from": match r.to { Some(true) => "TO", Some(false) => "FROM", None => "ILS" },
        "glide_slope": num_opt(r.gs),
        "dme_nm": num_opt(r.dme),
        "radial_deg": num(r.radial),
    })
}

fn airport_summary(a: &airports::Apt) -> Value {
    json!({
        "icao": a.ident,
        "iata": a.iata,
        "name": a.name,
        "city": a.city,
        "country": a.country,
        "kind": match a.kind { 2 => "large", 1 => "medium", _ => "small" },
        "elevation_ft": a.elev * 3.28084,
        "longest_runway_m": a.longest(),
        "any_paved_runway": a.any_paved(),
    })
}

fn runway_db_json(r: &airports::DbRunway) -> Value {
    json!({
        "ident": [r.ident[0], r.ident[1]],
        "length_m": r.len,
        "width_m": r.width,
        "paved": r.paved,
        "concrete": r.concrete,
        "lighted": r.lighted,
        "displaced_threshold_m": r.disp,
    })
}

fn runway_geo_json(r: &airports::Runway) -> Value {
    json!({
        "ident": [r.ident[0], r.ident[1]],
        "length_m": r.len,
        "width_m": r.width,
        "paved": r.paved,
        "concrete": r.concrete,
        "lighted": r.lighted,
        "class": r.class,
    })
}

impl Game {
    /// Drain the tool calls the bridge has sent.  Called from the frame and,
    /// when the window is not asking for redraws, from the event loop.
    pub(crate) fn pump_mcp(&mut self) {
        let calls: Vec<rpc::Call> = match &self.mcp_calls {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };
        for call in calls {
            self.dispatch_mcp(call);
        }
    }

    fn dispatch_mcp(&mut self, call: rpc::Call) {
        let Some(tool) = tools::Tool::from_name(&call.name) else {
            let _ = call.reply.send(tools::error_result(format!("unknown tool: {}", call.name)));
            return;
        };
        if tool == tools::Tool::TakeScreenshot {
            self.start_screenshot(&call.args, call.reply);
            return;
        }
        let result = match self.run_tool(tool, &call.args) {
            Ok(value) => tools::json_result(value),
            Err(e) => tools::error_result(e),
        };
        let _ = call.reply.send(result);
    }

    fn run_tool(&mut self, tool: tools::Tool, args: &Value) -> Result<Value, String> {
        match tool {
            tools::Tool::GetAircraftState => Ok(self.mcp_state()),
            tools::Tool::GetNavigation => self.mcp_navigation(args),
            tools::Tool::GetEnvironment => Ok(self.mcp_environment()),
            tools::Tool::GetTraffic => self.mcp_traffic(args),
            tools::Tool::GetAirportInfo => self.mcp_airport(args),
            tools::Tool::SetAircraftConfig => self.mcp_set_config(args),
            tools::Tool::SetEngine => self.mcp_set_engine(args),
            tools::Tool::SetRadios => self.mcp_set_radios(args),
            tools::Tool::SetAutopilot => self.mcp_set_autopilot(args),
            tools::Tool::SetEnvironment => self.mcp_set_environment(args),
            tools::Tool::PlaceAircraft => self.mcp_place(args),
            tools::Tool::TakeScreenshot => unreachable!("screenshots are deferred"),
        }
    }

    fn require_flying(&self) -> Result<(), String> {
        if self.mode != Mode::Fly {
            return Err("No flight is running. Start a flight, or use place_aircraft with an airport code.".into());
        }
        if self.crashed {
            return Err("The aircraft is wrecked. Respawn it or use place_aircraft before making changes.".into());
        }
        Ok(())
    }

    // ------------------------------------------------------------ state

    /// Ground height and water at a point, preferring a loaded airport's own
    /// surface over a coarse terrain tile.
    fn ground_under(&self, lat: f64, lon: f64) -> (Option<f64>, Option<bool>) {
        let p = geo::lla_to_pos(lat, lon, 0.0);
        if let Some(field) = self.world.airfield_near(p, 6_000.0) {
            if (field.geo.center - p).length() <= field.geo.radius as f64 + 500.0 {
                return (Some(field.geo.h(field.geo.local(p)) as f64), Some(false));
            }
        }
        let ground = self.world.ground(lat, lon);
        (ground.as_ref().map(|g| g.h), ground.as_ref().map(|g| g.water))
    }

    fn heading_pitch_roll(&self) -> (f64, f64, f64) {
        let p = &self.plane;
        let (e, n, u) = geo::enu(p.pos);
        let f = p.forward();
        let up = p.up();
        let right = p.right();
        let heading = f.dot(e).atan2(f.dot(n)).to_degrees().rem_euclid(360.0);
        let pitch = f.dot(u).clamp(-1.0, 1.0).asin().to_degrees();
        let roll = (-right.dot(u)).atan2(up.dot(u)).to_degrees();
        (heading, pitch, roll)
    }

    fn config_state(&self) -> Value {
        let p = &self.plane;
        json!({
            "throttle": num(p.throttle),
            "flaps_notch": p.flap_set,
            "flaps_fraction": num(p.flaps),
            "flaps_deg": num(p.flap_deg()),
            "slats": num(p.slats),
            "gear": gear_text(p.gear),
            "gear_handle_down": p.gear_down,
            "speedbrake": num(p.speedbrake),
            "spoilers": num(p.spoilers),
            "parking_brake": p.parking_brake,
            "mixture": num(p.mixture),
            "trim_deg": num(p.trim.to_degrees()),
            "reverser": num(p.reverser),
            "chute_spent": p.chute_spent,
        })
    }

    fn engine_state(&self) -> Value {
        let p = &self.plane;
        let mut engine = json!({
            "running": p.running,
            "ignition": p.ignition,
            "mixture": num(p.mixture),
            "starter_on": p.starter_on,
            "power_pct": num(p.power * 100.0),
            "rpm": num(p.prop_rate * 60.0 / std::f64::consts::TAU),
            "n1_pct": num(p.n1 * 100.0),
            "thrust_n": num(p.thrust_now()),
            "burner": num(p.burner),
            "fuel_kg": num(p.fuel),
            "fuel_capacity_kg": num(p.spec.fuel_kg),
            "fuel_fraction": num(p.fuel_frac()),
            "fuel_flow_kg_per_h": num(p.fuel_flow * 3600.0),
            "endurance_min": num_opt(p.endurance().map(|s| s / 60.0)),
            "payload_kg": num(p.payload),
            "mass_kg": num(p.mass()),
        });
        if let Some((name, lit, total)) = p.engines() {
            engine["rocket"] = json!({
                "stage": p.stage,
                "engine_group": name,
                "group_engines_lit": lit,
                "stage_engines_total": total,
                "autoland": p.auto,
            });
        }
        engine
    }

    fn radio_state(&self) -> Value {
        let p = &self.plane;
        let (heading, _, _) = self.heading_pitch_roll();
        let nav1 = self.radios.nav.as_ref().map(|st| {
            let mut j = station_json(st);
            j["distance_nm"] = num((p.pos - st.pos()).length() / 1852.0);
            j["bearing_deg"] = num(nav::bearing(p.pos, st.pos()));
            j["reading"] = nav_reading_json(st, self.radios.obs, p.pos);
            j
        });
        let adf = self.radios.adf.as_ref().map(|st| {
            let mut j = station_json(st);
            j["distance_nm"] = num((p.pos - st.pos()).length() / 1852.0);
            j["bearing_deg"] = num(nav::bearing(p.pos, st.pos()));
            j["bearing_relative_to_nose_deg"] = num_opt(nav::adf_bearing(st, p.pos, heading));
            j
        });
        json!({
            "nav1": nav1,
            "adf": adf,
            "obs_deg": num(self.radios.obs),
            "altimeter_setting_hpa": num(self.radios.setting(self.wx.qnh)),
            "qnh_hpa": num(self.wx.qnh),
            "altimeter_follows_qnh": self.radios.baro.is_none(),
        })
    }

    fn warning_list(&self, agl_ft: Option<f64>) -> Vec<String> {
        let p = &self.plane;
        let mut w = Vec::new();
        if !self.crashed {
            if p.stalled {
                w.push("STALL".into());
            }
            if self.warn.pull_up {
                w.push("PULL UP".into());
            } else if self.warn.dive > 0.0 {
                w.push("NOSE DIVE".into());
            } else if self.warn.too_low {
                w.push("TOO LOW TERRAIN".into());
            } else if self.warn.sink {
                w.push("SINK RATE".into());
            }
            let over = p.overspeed();
            if over > 13.0 {
                w.push("OVERSPEED AIRFRAME BREAKING UP".into());
            } else if over > 0.0 {
                w.push("OVERSPEED".into());
            }
            if p.flaps > 0.25 && p.ias() > p.vfe() {
                w.push(if p.spec.lever.is_some() { "NOSE OVERSPEED" } else { "FLAPS OVERSPEED" }.into());
            }
            if p.g_load.abs() > 5.0 {
                w.push("OVER G".into());
            }
            if p.fuel <= 0.0 {
                w.push("NO FUEL".into());
            } else if p.fuel_frac() < 0.1 {
                w.push("LOW FUEL".into());
            }
            if !p.running && !p.on_ground {
                w.push("ENGINE OFF".into());
            }
            let el = &p.sys.elec;
            if el.master && (el.volts < 24.0 || (p.sys.fail.alternator && p.running)) {
                w.push("LOW VOLTS".into());
            }
            if p.sys.fail.vacuum && !p.spec.is_jet() {
                w.push("VACUUM".into());
            }
            if p.cg_excess() != 0.0 {
                w.push("CG LIMIT".into());
            }
            if p.gear < 1.0 && agl_ft.is_some_and(|h| h < 150.0) && p.throttle < 0.35 {
                w.push("GEAR UP".into());
            }
        }
        w
    }

    fn mcp_state(&self) -> Value {
        let p = &self.plane;
        let (lat, lon, alt) = geo::pos_to_lla(p.pos);
        let (e, n, u) = geo::enu(p.pos);
        let (heading, pitch, roll) = self.heading_pitch_roll();
        let vs = p.vel.dot(u);
        let (ground_h, water) = self.ground_under(lat, lon);
        let agl = ground_h.map(|h| alt - h);
        let agl_ft = agl.map(|h| h * 3.28084);
        let gs = (p.vel - u * p.vel.dot(u)).length();
        let track = p.vel.dot(e).atan2(p.vel.dot(n)).to_degrees().rem_euclid(360.0);
        let mode = match self.mode {
            Mode::Menu => "menu",
            Mode::Fly => "fly",
        };
        let local = (self.utc + lon.to_degrees() / 15.0).rem_euclid(24.0);
        let el = &p.sys.elec;
        let fail = &p.sys.fail;
        let powered = p.sys.powered();
        let wind_kt = self.wx.wind * 1.94384;
        let wind_at_aircraft = p.wind.length() * 1.94384;
        json!({
            "sim": {
                "mode": mode,
                "paused": self.paused || self.pause_open,
                "paused_by_key": self.paused,
                "menu_open": self.pause_open,
                "in_menu": self.mode == Mode::Menu,
                "crashed": self.crashed,
                "holding_for_terrain": p.hold || self.hold_spot.is_some(),
                "airport_loading": self.airport_rx.is_some() || self.menu.starting,
                "status": self.status,
                "view": match self.cam_mode { 1 => "cockpit", crate::settings::TAIL_VIEW => "tail", _ => "chase" },
                "hud": self.show_hud,
                "view_panel_open": self.info_open,
                "fps": num(self.fps),
                "frame": self.frame,
                "sim_seconds": num(self.time),
                "wall_seconds": num(self.real),
            },
            "aircraft": {
                "name": p.spec.name,
                "keys": p.spec.keys.to_vec(),
                "jet": p.spec.is_jet(),
                "rocket": p.spec.rocket().is_some(),
                "max_flap_notch": p.spec.max_notch(),
                "mass_kg": num(p.mass()),
                "cg_percent_mac": num(p.cg() * 100.0),
                "cg_forward_limit_percent_mac": num(p.spec.cg[1] * 100.0),
                "cg_aft_limit_percent_mac": num(p.spec.cg[2] * 100.0),
                "cg_limit_exceeded": p.cg_excess() != 0.0,
                "seconds_flown": num(p.t),
            },
            "position": {
                "latitude_deg": num(lat.to_degrees()),
                "longitude_deg": num(lon.to_degrees()),
                "altitude_msl_m": num(alt),
                "altitude_msl_ft": num(alt * 3.28084),
                "ground_elevation_ft": num_opt(ground_h.map(|h| h * 3.28084)),
                "height_above_ground_ft": num_opt(agl_ft),
                "terrain_water": water,
                "on_ground": p.on_ground,
                "ground_speed_kt": num(gs * 1.94384),
                "ground_track_deg": num(track),
            },
            "attitude": {
                "heading_deg_true": num(heading),
                "pitch_deg": num(pitch),
                "roll_deg": num(roll),
                "pitch_rate_deg_s": num(p.omega.x.to_degrees()),
                "yaw_rate_deg_s": num(p.omega.y.to_degrees()),
                "roll_rate_deg_s": num(p.omega.z.to_degrees()),
            },
            "motion": {
                "indicated_airspeed_kt": num(p.ias() * 1.94384),
                "true_airspeed_kt": num(p.airspeed().length() * 1.94384),
                "ground_speed_kt": num(gs * 1.94384),
                "vertical_speed_fpm": num(vs * 196.85),
                "mach": num(p.mach()),
                "angle_of_attack_deg": num(p.alpha.to_degrees()),
                "load_factor_g": num(p.g_load),
                "stalled": p.stalled,
                "overspeed_kt": num(p.overspeed() * 1.94384),
                "vfe_kt": num(p.vfe() * 1.94384),
                "on_ground": p.on_ground,
            },
            "configuration": self.config_state(),
            "engine": self.engine_state(),
            "systems": {
                "battery_master": el.master,
                "powered": powered,
                "volts": num(el.volts),
                "amps": num(el.amps),
                "battery_charge": num(el.charge),
                "failures": {
                    "engine": fail.engine,
                    "alternator": fail.alternator,
                    "battery": fail.battery,
                    "vacuum_pump": fail.vacuum,
                    "pitot": fail.pitot,
                    "static_port": fail.static_port,
                    "fuel_leak": fail.fuel_leak,
                },
            },
            "radios": self.radio_state(),
            "autopilot": self.autopilot_state(),
            "environment": {
                "wind_from_deg": num(self.wx.wind_from),
                "wind_speed_kt": num(wind_kt),
                "gust_factor": num(self.wx.gusts),
                "wind_at_aircraft_from_deg": num(wind_from_deg(p.wind, p.pos)),
                "wind_at_aircraft_kt": num(wind_at_aircraft),
                "qnh_hpa": num(self.wx.qnh),
                "visibility_km": num(self.wx.visibility_km()),
                "cloud_cover": num(self.wx.clouds as f64),
                "cloud_base_ft": num(self.wx.cloud_base as f64 * 3.28084),
                "cloud_ceiling_ft": num(self.wx.cloud_ceiling() as f64 * 3.28084),
                "live_weather": self.wx.live,
                "turbulence_sigma_w_ms": num(p.air.sigma_w),
                "turbulence_sigma_h_ms": num(p.air.sigma_h),
                "updraft_ms": num(p.air.draft),
            },
            "time": {
                "utc_hours": num(self.utc),
                "local_hours": num(local),
                "day_of_year": num(self.doy),
                "time_rate": num(self.wx.time_rate),
                "real_time": self.wx.real_time,
                "sun_elevation_deg": num(self.wx.sun.clamp(-1.0, 1.0).asin().to_degrees()),
            },
            "warnings": self.warning_list(agl_ft),
            "damage": {
                "wrecked": p.wrecked(),
                "cause": p.cause,
                "report": p.dmg.report(),
            },
        })
    }

    // ------------------------------------------------------------ reads

    fn mcp_navigation(&self, args: &Value) -> Result<Value, String> {
        let args: tools::NavigationArgs = tools::parse(args)?;
        let radius_nm = args.radius_nm.unwrap_or(50.0).clamp(1.0, 500.0);
        let limit = args.limit.unwrap_or(8).clamp(1, 50);
        let radius = radius_nm * 1852.0;
        let (lat, lon, _) = geo::pos_to_lla(self.plane.pos);
        let station_list = |kind: nav::Kind| -> Vec<Value> {
            nav::near(lat.to_degrees(), lon.to_degrees(), radius, kind)
                .into_iter()
                .take(limit)
                .map(|(st, d)| {
                    let mut j = station_json(&st);
                    j["distance_nm"] = num(d / 1852.0);
                    j["bearing_deg"] = num(nav::bearing(self.plane.pos, st.pos()));
                    j
                })
                .collect()
        };
        let db = airports::Db::get();
        let airports: Vec<Value> = db
            .near(lat.to_degrees(), lon.to_degrees(), radius)
            .into_iter()
            .take(limit)
            .map(|(i, d)| {
                let a = &db.apts[i as usize];
                let mut j = airport_summary(a);
                j["distance_nm"] = num(d / 1852.0);
                j["bearing_deg"] = num(nav::bearing(self.plane.pos, geo::lla_to_pos(a.lat.to_radians(), a.lon.to_radians(), 0.0)));
                j
            })
            .collect();
        Ok(json!({
            "position": {
                "latitude_deg": num(lat.to_degrees()),
                "longitude_deg": num(lon.to_degrees()),
                "altitude_msl_ft": num(geo::pos_to_lla(self.plane.pos).2 * 3.28084),
            },
            "radios": self.radio_state(),
            "nearest_airports": airports,
            "nearest_vors": station_list(nav::Kind::Vor),
            "nearest_ndbs": station_list(nav::Kind::Ndb),
            "nearest_ils": station_list(nav::Kind::Ils),
        }))
    }

    fn mcp_environment(&self) -> Value {
        let (_, lon, _) = geo::pos_to_lla(self.plane.pos);
        let local = (self.utc + lon.to_degrees() / 15.0).rem_euclid(24.0);
        json!({
            "position": {
                "latitude_deg": num(geo::pos_to_lla(self.plane.pos).0.to_degrees()),
                "longitude_deg": num(lon.to_degrees()),
                "altitude_msl_ft": num(geo::pos_to_lla(self.plane.pos).2 * 3.28084),
            },
            "weather": {
                "live": self.wx.live,
                "live_station": self.wx.live_station,
                "live_report": self.wx.live_raw,
                "cloud_cover": num(self.wx.clouds as f64),
                "cloud_base_ft": num(self.wx.cloud_base as f64 * 3.28084),
                "cloud_depth_ft": num(self.wx.cloud_depth as f64 * 3.28084),
                "cloud_ceiling_ft": num(self.wx.cloud_ceiling() as f64 * 3.28084),
                "storms": num(self.wx.storms as f64),
                "alto": num(self.wx.alto as f64),
                "haze": num(self.wx.haze as f64),
                "visibility_km": num(self.wx.visibility_km()),
                "wind_from_deg": num(self.wx.wind_from),
                "wind_speed_kt": num(self.wx.wind * 1.94384),
                "gusts": num(self.wx.gusts),
                "qnh_hpa": num(self.wx.qnh),
            },
            "air": {
                "turbulence_sigma_w_ms": num(self.plane.air.sigma_w),
                "turbulence_sigma_h_ms": num(self.plane.air.sigma_h),
                "length_scale_w_m": num(self.plane.air.len_w),
                "length_scale_h_m": num(self.plane.air.len_h),
                "updraft_ms": num(self.plane.air.draft),
            },
            "clock": {
                "utc_hours": num(self.utc),
                "local_hours": num(local),
                "day_of_year": num(self.doy),
                "time_rate": num(self.wx.time_rate),
                "real_time": self.wx.real_time,
                "sun_elevation_deg": num(self.wx.sun.clamp(-1.0, 1.0).asin().to_degrees()),
            },
        })
    }

    fn mcp_traffic(&self, args: &Value) -> Result<Value, String> {
        let args: tools::TrafficArgs = tools::parse(args)?;
        let radius_nm = args.radius_nm.unwrap_or(40.0).clamp(1.0, 200.0);
        let limit = args.limit.unwrap_or(15).clamp(1, 50);
        let radius = radius_nm * 1852.0;
        let (_, _, alt) = geo::pos_to_lla(self.plane.pos);
        let mut contacts: Vec<(f64, Value)> = self
            .live
            .planes
            .iter()
            .filter_map(|p| {
                let d = (p.pos - self.plane.pos).length();
                if d > radius {
                    return None;
                }
                Some((
                    d,
                    json!({
                        "callsign": p.call,
                        "type": p.ty,
                        "hex": p.hex,
                        "altitude_ft": num(p.alt_ft),
                        "relative_altitude_ft": num(p.alt_ft - alt * 3.28084),
                        "ground_speed_kt": num(p.gs_kt),
                        "vertical_speed_fpm": num(p.vs_fpm),
                        "distance_nm": num(d / 1852.0),
                        "bearing_deg": num(nav::bearing(self.plane.pos, p.pos)),
                    }),
                ))
            })
            .collect();
        contacts.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(json!({
            "enabled": self.live.enabled(),
            "error": self.live.error(),
            "count_total": self.live.planes.len(),
            "radius_nm": radius_nm,
            "contacts": contacts.into_iter().take(limit).map(|(_, v)| v).collect::<Vec<_>>(),
        }))
    }

    fn mcp_airport(&self, args: &Value) -> Result<Value, String> {
        let args: tools::AirportArgs = tools::parse(args)?;
        if let Some(code) = args.code.as_deref() {
            let db = airports::Db::get();
            let Some(idx) = db.find(code) else {
                let suggestions: Vec<String> = db.search(code, 5).into_iter().map(|i| db.apts[i as usize].code()).collect();
                return Err(if suggestions.is_empty() {
                    format!("no airport matches {code:?}")
                } else {
                    format!("no airport matches {code:?}; try one of: {}", suggestions.join(", "))
                });
            };
            let a = &db.apts[idx as usize];
            let mut out = airport_summary(a);
            out["source"] = json!("database");
            out["runways"] = json!(a.rwys.iter().map(runway_db_json).collect::<Vec<_>>());
            let ils: Vec<Value> = nav::near(a.lat, a.lon, 25_000.0, nav::Kind::Ils)
                .into_iter()
                .filter(|(st, _)| st.name.eq_ignore_ascii_case(&a.ident))
                .map(|(st, d)| {
                    let mut j = station_json(&st);
                    j["distance_to_airport_nm"] = num(d / 1852.0);
                    j
                })
                .collect();
            out["ils"] = json!(ils);
            return Ok(out);
        }
        let (lat, lon, _) = geo::pos_to_lla(self.plane.pos);
        if let Some(field) = self.world.airfield_near(self.plane.pos, 18_000.0) {
            let g = &field.geo;
            let a = g.apt();
            let mut out = airport_summary(a);
            out["source"] = json!("loaded");
            out["runways"] = json!(g.runways.iter().map(runway_geo_json).collect::<Vec<_>>());
            let active = g.active(self.wx.wind_from as f32, (self.wx.wind * 1.94384) as f32).map(|(ri, e)| {
                let r = &g.runways[ri];
                let (thr, dir) = r.end(e);
                json!({
                    "ident": r.ident[e],
                    "heading_deg": num((dir.x as f64).atan2(dir.y as f64).to_degrees().rem_euclid(360.0)),
                    "length_m": r.len,
                    "threshold": [num(geo::pos_to_lla(g.world(thr, g.h(thr) as f64)).0.to_degrees()), num(geo::pos_to_lla(g.world(thr, g.h(thr) as f64)).1.to_degrees())],
                })
            });
            out["active_runway"] = active.unwrap_or(Value::Null);
            let ils: Vec<Value> = nav::near(lat.to_degrees(), lon.to_degrees(), 25_000.0, nav::Kind::Ils)
                .into_iter()
                .filter(|(st, _)| st.name.eq_ignore_ascii_case(&a.ident))
                .map(|(st, d)| {
                    let mut j = station_json(&st);
                    j["distance_nm"] = num(d / 1852.0);
                    j
                })
                .collect();
            out["ils"] = json!(ils);
            out["wind"] = json!({
                "from_deg": num(self.wx.wind_from),
                "speed_kt": num(self.wx.wind * 1.94384),
            });
            out["approach"] = self.approach_guidance(g);
            return Ok(out);
        }
        let db = airports::Db::get();
        let nearest = db.near(lat.to_degrees(), lon.to_degrees(), 50_000.0).into_iter().next();
        let Some((idx, d)) = nearest else {
            return Err("no airport within 50 km, and no airfield is loaded here".into());
        };
        let a = &db.apts[idx as usize];
        let mut out = airport_summary(a);
        out["source"] = json!("database");
        out["distance_nm"] = num(d / 1852.0);
        out["bearing_deg"] = num(nav::bearing(self.plane.pos, geo::lla_to_pos(a.lat.to_radians(), a.lon.to_radians(), 0.0)));
        out["runways"] = json!(a.rwys.iter().map(runway_db_json).collect::<Vec<_>>());
        let ils: Vec<Value> = nav::near(a.lat, a.lon, 25_000.0, nav::Kind::Ils)
            .into_iter()
            .filter(|(st, _)| st.name.eq_ignore_ascii_case(&a.ident))
            .map(|(st, dd)| {
                let mut j = station_json(&st);
                j["distance_to_airport_nm"] = num(dd / 1852.0);
                j
            })
            .collect();
        out["ils"] = json!(ils);
        out["note"] = json!("the airfield layout is not loaded; start at the airport or fly closer for approach guidance");
        Ok(out)
    }

    /// Structured version of the HUD's approach guidance.
    fn approach_guidance(&self, g: &airports::Geo) -> Value {
        let (_, _, alt) = geo::pos_to_lla(self.plane.pos);
        let lp = g.local(self.plane.pos);
        let fw = self.plane.forward();
        let mut hd = Vec2::new(fw.dot(g.e) as f32, fw.dot(g.n) as f32);
        if hd.length_squared() > 1e-6 {
            hd = hd.normalize();
        } else {
            hd = Vec2::ZERO;
        }
        if self.plane.on_ground || alt - (g.h(lp) as f64) < 3.0 {
            if let Some(ri) = g.on_runway(lp, 0.0) {
                let r = &g.runways[ri];
                let e = if hd.dot(r.dir) >= 0.0 { 0 } else { 1 };
                let (s, _) = r.local(lp);
                let remaining = if e == 0 { r.len - s } else { s };
                return json!({
                    "on_ground": true,
                    "runway": r.ident[e],
                    "runway_remaining_m": num(remaining.max(0.0) as f64),
                });
            }
            if let Some(h) = g.holds.iter().find(|h| (h.p - lp).length() < 25.0) {
                return json!({
                    "on_ground": true,
                    "holding_point": true,
                    "runway": g.runways[h.rwy as usize].name(),
                });
            }
            return json!({ "on_ground": true });
        }
        let mut best: Option<(f32, f32, usize, usize)> = None;
        for (ri, r) in g.runways.iter().enumerate() {
            for e in 0..2 {
                let (thr, dir) = r.end(e);
                let along = (thr - lp).dot(dir);
                let x = (lp - thr).dot(airports::right_of(dir));
                // Prefer the centreline we are actually on (parallel runways
                // are otherwise easy to confuse); then the nearer threshold.
                let better = best
                    .map(|(bx, ba, _, _)| x.abs() < bx - 0.01 || ((x.abs() - bx).abs() <= 0.01 && along < ba))
                    .unwrap_or(true);
                if along > -200.0 && along < 20_000.0 && hd.dot(dir) > 0.75 && x.abs() < along.max(0.0) * 0.25 + 300.0 && better {
                    best = Some((x.abs(), along, ri, e));
                }
            }
        }
        let Some((_, along, ri, e)) = best else {
            return Value::Null;
        };
        let r = &g.runways[ri];
        let (thr, dir) = r.end(e);
        let x = (lp - thr).dot(airports::right_of(dir));
        let h = alt - g.h(thr) as f64;
        let aim = (along + traffic::aim_dist(r.len)).max(1.0);
        let gs = h.atan2(aim as f64).to_degrees();
        let dev = gs - 3.0;
        json!({
            "on_ground": false,
            "runway": r.ident[e],
            "distance_nm": num(along.max(0.0) as f64 / 1852.0),
            "centerline_offset_m": num(x as f64),
            "centerline_offset_side": if x.abs() < 0.01 * along.max(300.0) + 5.0 { "on_centerline" } else if x > 0.0 { "right" } else { "left" },
            "height_above_threshold_ft": num(h * 3.28084),
            "glide_path_deviation_deg": num(dev),
            "on_centerline": x.abs() < 0.01 * along.max(300.0) + 5.0,
            "on_glidepath": dev.abs() < 0.3,
        })
    }

    // ------------------------------------------------------------ writes

    fn mcp_set_config(&mut self, args: &Value) -> Result<Value, String> {
        let args: tools::AircraftConfigArgs = tools::parse(args)?;
        self.require_flying()?;
        if args.mixture.is_some() && (self.spec.is_jet() || self.spec.rocket().is_some()) {
            return Err("this aircraft has no mixture control".into());
        }
        let mut notes: Vec<String> = Vec::new();
        let p = &mut self.plane;
        let mut any = false;
        if let Some(throttle) = args.throttle {
            any = true;
            let t = throttle.clamp(0.0, 1.0);
            if (t - throttle).abs() > 1e-9 {
                notes.push(format!("throttle clamped to {:.0}%", t * 100.0));
            }
            p.throttle = t;
            self.mcp_throttle = Some(t);
        }
        if let Some(notch) = args.flaps_notch {
            any = true;
            let max = p.spec.max_notch() as i32;
            let v = notch.clamp(0, max);
            if v != notch {
                notes.push(format!("flaps notch clamped to {v} (0..{max})"));
            }
            p.flap_set = v as u8;
            if args.flaps_delta.is_some() {
                notes.push("flaps_delta ignored because flaps_notch was given".into());
            }
        } else if let Some(delta) = args.flaps_delta {
            any = true;
            let d = delta.clamp(-1, 1);
            if d != 0 {
                p.set_flaps(d);
            }
        }
        if let Some(down) = args.gear_down {
            any = true;
            if p.spec.rocket().is_some() {
                notes.push("this aircraft has no landing gear".into());
            } else if !down && p.on_ground {
                notes.push("the squat switch keeps the gear down on the ground".into());
            } else {
                p.gear_down = down;
            }
        }
        if let Some(on) = args.speedbrake {
            any = true;
            let deployed = p.speedbrake > 0.0;
            if on != deployed {
                p.toggle_speedbrake();
            }
            if on && p.speedbrake <= 0.0 {
                notes.push("the speedbrake/parachute is unavailable (already spent or not fitted)".into());
            }
        }
        if let Some(on) = args.parking_brake {
            any = true;
            p.parking_brake = on;
        }
        if let Some(mixture) = args.mixture {
            any = true;
            p.mixture = mixture.clamp(0.0, 1.0);
        }
        if let Some(deg) = args.trim_deg {
            any = true;
            let rad = deg.to_radians();
            let clamped = rad.clamp(-0.05, 0.24);
            if (clamped - rad).abs() > 1e-9 {
                notes.push(format!("trim clamped to {:.1} deg", clamped.to_degrees()));
            }
            p.trim = clamped;
        }
        if !any {
            return Err("nothing to set: give at least one of throttle, flaps_notch, flaps_delta, gear_down, speedbrake, parking_brake, mixture, trim_deg".into());
        }
        let config = self.config_state();
        Ok(json!({ "ok": true, "notes": notes, "configuration": config }))
    }

    fn mcp_set_engine(&mut self, args: &Value) -> Result<Value, String> {
        let args: tools::EngineArgs = tools::parse(args)?;
        self.require_flying()?;
        let action = args.action.trim().to_ascii_lowercase();
        let p = &mut self.plane;
        let note: String;
        match action.as_str() {
            "start" => {
                if p.running {
                    note = "the engine is already running".into();
                } else {
                    p.start_engine();
                    note = if p.fuel <= 0.0 { "start sequence begun, but there is no fuel".into() } else { "start sequence begun".into() };
                }
            }
            "stop" => {
                p.stop_engine();
                note = "engine shutdown begun".into();
            }
            "ignition_on" => {
                p.ignition = true;
                note = "ignition/fuel control on".into();
            }
            "ignition_off" => {
                p.ignition = false;
                note = "ignition/fuel control off".into();
            }
            "starter_on" => {
                if !p.spec.is_jet() {
                    return Err("only a jet has a separate starter control; use action=start for a piston engine".into());
                }
                p.starter_on = true;
                note = "starter engaged".into();
            }
            "starter_off" => {
                if !p.spec.is_jet() {
                    return Err("only a jet has a separate starter control".into());
                }
                p.starter_on = false;
                note = "starter released".into();
            }
            other => return Err(format!("unknown action {other:?}; use start, stop, ignition_on, ignition_off, starter_on or starter_off")),
        }
        let engine = self.engine_state();
        Ok(json!({ "ok": true, "action": action, "note": note, "engine": engine }))
    }

    /// The autopilot as the copilot reads it.
    fn autopilot_state(&self) -> Value {
        let ap = &self.ap;
        let (active, armed) = ap.annunciation();
        json!({
            "engaged": ap.on,
            "active_modes": active,
            "armed_modes": armed,
            "heading_bug_deg": num(ap.hdg),
            "altitude_ft": num(ap.alt * 3.28084),
            "vs_fpm": num(ap.vs * 196.85),
            "autothrottle": ap.at,
            "speed_kt": num(ap.spd * 1.943_84),
        })
    }

    fn mcp_set_autopilot(&mut self, args: &Value) -> Result<Value, String> {
        use crate::autopilot::{Lat, Vert};
        let args: tools::AutopilotArgs = tools::parse(args)?;
        self.require_flying()?;
        if let Some(h) = args.heading_deg {
            self.ap.hdg = h.rem_euclid(360.0);
        }
        if let Some(a) = args.altitude_ft {
            self.ap.alt = a / 3.28084;
        }
        if let Some(v) = args.vs_fpm {
            self.ap.vs = v / 196.85;
        }
        if let Some(k) = args.speed_kt {
            self.ap.spd = k / 1.943_84;
        }
        if let Some(on) = args.autothrottle {
            self.ap.at = on;
        }
        if args.engaged == Some(false) {
            if self.ap.on {
                self.ap_disconnect(None);
            }
            return Ok(json!({ "ok": true, "autopilot": self.autopilot_state() }));
        }
        let s = crate::autopilot::State::of(&self.plane);
        if let Some(m) = args.lateral.as_deref() {
            let m = match m {
                "hdg" => Lat::Hdg,
                "nav" => Lat::Nav,
                "apr" => Lat::Apr,
                "roll" => Lat::Roll,
                _ => return Err(format!("unknown lateral mode {m:?}")),
            };
            self.ap.set_lat(m, &s);
        }
        if let Some(m) = args.vertical.as_deref() {
            let m = match m {
                "alt" => Vert::Alt,
                "vs" => Vert::Vs,
                "pitch" => Vert::Pitch,
                _ => return Err(format!("unknown vertical mode {m:?}")),
            };
            self.ap.set_vert(m, &s);
        }
        let mut notes = Vec::new();
        if (args.engaged == Some(true) || args.lateral.is_some() || args.vertical.is_some()) && !self.ap.on {
            self.ap_toggle();
            if !self.ap.on {
                return Err("the autopilot works in flight only".into());
            }
        }
        if matches!(self.ap.lat, Lat::Nav | Lat::Apr) && self.radios.nav.is_none() {
            notes.push("NAV1 is not tuned: the mode stays armed, flying the heading bug (set_radios)");
        }
        Ok(json!({ "ok": true, "notes": notes, "autopilot": self.autopilot_state() }))
    }

    fn mcp_set_radios(&mut self, args: &Value) -> Result<Value, String> {
        let args: tools::RadiosArgs = tools::parse(args)?;
        self.require_flying()?;
        let (lat, lon, _) = geo::pos_to_lla(self.plane.pos);
        let mut notes: Vec<String> = Vec::new();
        let mut any = false;
        if let Some(query) = args.nav1.as_deref() {
            any = true;
            let Some(st) = Self::find_nav_station(query, self.plane.pos) else {
                let nearby: Vec<String> = nav::near(lat.to_degrees(), lon.to_degrees(), 100_000.0, nav::Kind::Vor).iter().take(6).map(|(s, _)| s.label()).collect();
                return Err(format!("no NAV1 station matches {query:?}{}", if nearby.is_empty() { String::new() } else { format!("; nearby VORs: {}", nearby.join(", ")) }));
            };
            let label = st.label();
            self.radios.tune_nav(st, self.plane.pos);
            notes.push(format!("NAV1 tuned to {label}; OBS set to the station"));
        }
        if let Some(query) = args.adf.as_deref() {
            any = true;
            let Some(st) = Self::find_adf_station(query, self.plane.pos) else {
                let nearby: Vec<String> = nav::near(lat.to_degrees(), lon.to_degrees(), 150_000.0, nav::Kind::Ndb).iter().take(6).map(|(s, _)| s.label()).collect();
                return Err(format!("no ADF station matches {query:?}{}", if nearby.is_empty() { String::new() } else { format!("; nearby NDBs: {}", nearby.join(", ")) }));
            };
            let label = st.label();
            self.radios.tune_adf(st);
            notes.push(format!("ADF tuned to {label}"));
        }
        if let Some(obs) = args.obs_deg {
            any = true;
            self.radios.obs = obs.rem_euclid(360.0);
        }
        if let Some(value) = args.altimeter_hpa {
            any = true;
            match value {
                Value::Null => {
                    self.radios.baro = None;
                    notes.push("altimeter set to the QNH".into());
                }
                Value::String(s) if s.eq_ignore_ascii_case("qnh") => {
                    self.radios.baro = None;
                    notes.push("altimeter set to the QNH".into());
                }
                Value::Number(n) => {
                    let Some(hpa) = n.as_f64() else {
                        return Err("altimeter_hpa must be a number or \"qnh\"".into());
                    };
                    let hpa = hpa.clamp(850.0, 1100.0);
                    self.radios.baro = Some(hpa);
                    notes.push(format!("altimeter set to {hpa:.0} hPa"));
                }
                _ => return Err("altimeter_hpa must be a number or the string \"qnh\"".into()),
            }
        }
        if !any {
            return Err("nothing to set: give nav1, adf, obs_deg or altimeter_hpa".into());
        }
        let radios = self.radio_state();
        Ok(json!({ "ok": true, "notes": notes, "radios": radios }))
    }

    fn find_nav_station(query: &str, pos: DVec3) -> Option<nav::Station> {
        let q = query.trim();
        if q.is_empty() {
            return None;
        }
        if let Ok(f) = q.parse::<f64>() {
            if (108.0..=118.0).contains(&f) {
                let khz = (f * 1000.0).round() as u32;
                if let Some(st) = nearest_station(pos, |s| s.kind == nav::Kind::Vor && s.freq_khz == khz) {
                    return Some(st);
                }
            }
        }
        let up = q.to_ascii_uppercase();
        let (apt, rwy) = match up.split_once(' ') {
            Some((a, b)) => (Some(a.trim()), b.trim()),
            None => (None, up.as_str()),
        };
        let (lat, lon, _) = geo::pos_to_lla(pos);
        let ils = nav::near(lat.to_degrees(), lon.to_degrees(), 80_000.0, nav::Kind::Ils);
        if let Some((st, _)) = ils.iter().find(|(st, _)| {
            let name_ok = apt.map(|a| st.name.eq_ignore_ascii_case(a)).unwrap_or(true);
            name_ok && st.ident.eq_ignore_ascii_case(rwy)
        }) {
            return Some(st.clone());
        }
        if apt.is_none() {
            if let Some((st, _)) = ils.iter().find(|(st, _)| st.name.eq_ignore_ascii_case(&up)) {
                return Some(st.clone());
            }
        }
        nearest_station(pos, |s| s.kind == nav::Kind::Vor && s.ident.eq_ignore_ascii_case(&up))
    }

    fn find_adf_station(query: &str, pos: DVec3) -> Option<nav::Station> {
        let q = query.trim();
        if q.is_empty() {
            return None;
        }
        if let Ok(f) = q.parse::<f64>() {
            let khz = f.round() as u32;
            if let Some(st) = nearest_station(pos, |s| s.kind == nav::Kind::Ndb && s.freq_khz == khz) {
                return Some(st);
            }
        }
        let up = q.to_ascii_uppercase();
        nearest_station(pos, |s| s.kind == nav::Kind::Ndb && s.ident.eq_ignore_ascii_case(&up))
    }

    fn mcp_set_environment(&mut self, args: &Value) -> Result<Value, String> {
        let args: tools::EnvironmentArgs = tools::parse(args)?;
        if args.visibility_km.is_some_and(|v| v <= 0.0) {
            return Err("visibility_km must be positive".into());
        }
        let mut any = false;
        let mut notes: Vec<String> = Vec::new();
        let mut weather = false;
        if let Some(live) = args.weather_live {
            any = true;
            self.wx.live = live;
            if live {
                notes.push("live weather on".into());
            }
        }
        if let Some(v) = args.clouds {
            weather = true;
            self.wx.clouds = v.clamp(0.0, 1.0) as f32;
        }
        if let Some(v) = args.cloud_base_ft {
            weather = true;
            self.wx.cloud_base = (v.max(0.0) / 3.28084) as f32;
        }
        if let Some(v) = args.cloud_depth_ft {
            weather = true;
            self.wx.cloud_depth = (v.max(0.0) / 3.28084) as f32;
        }
        if let Some(v) = args.storms {
            weather = true;
            self.wx.storms = v.clamp(0.0, 1.0) as f32;
        }
        if let Some(v) = args.alto {
            weather = true;
            self.wx.alto = v.clamp(0.0, 1.0) as f32;
        }
        if let Some(v) = args.visibility_km {
            weather = true;
            // visibility = 3.912 / (13.558e-6 + 8.396e-6 * haze) / 1000
            let haze = ((3.912 / (v * 1000.0) - 13.558e-6) / 8.396e-6).clamp(0.2, 40.0);
            self.wx.haze = haze as f32;
            notes.push(format!("haze set for {:.0} km visibility", self.wx.visibility_km()));
        }
        if let Some(v) = args.wind_speed_kt {
            weather = true;
            self.wx.wind = (v.max(0.0) / 1.94384) as f64;
        }
        if let Some(v) = args.wind_from_deg {
            weather = true;
            self.wx.wind_from = v.rem_euclid(360.0);
        }
        if let Some(v) = args.gusts {
            weather = true;
            self.wx.gusts = v.clamp(0.0, 1.0);
        }
        if weather {
            any = true;
            if args.weather_live != Some(true) {
                self.wx.live = false;
                notes.push("live weather turned off (hand-set weather)".into());
            }
        }
        let mut time = false;
        if let Some(v) = args.time_utc_hours {
            any = true;
            time = true;
            self.utc = v.rem_euclid(24.0);
        }
        if let Some(v) = args.day_of_year {
            any = true;
            time = true;
            self.doy = v.rem_euclid(365.0);
        }
        if let Some(v) = args.time_rate {
            any = true;
            time = true;
            self.wx.time_rate = v.clamp(0.0, 3600.0);
        }
        if time && args.real_time != Some(true) {
            self.wx.real_time = false;
            notes.push("real time turned off".into());
        }
        if let Some(real) = args.real_time {
            any = true;
            self.wx.real_time = real;
            if real {
                notes.push("clock follows real time".into());
            }
        }
        if !any {
            return Err("nothing to set".into());
        }
        Ok(self.mcp_environment())
    }

    fn mcp_place(&mut self, args: &Value) -> Result<Value, String> {
        let args: tools::PlaceArgs = tools::parse(args)?;
        if let Some(code) = args.airport.as_deref() {
            let db = airports::Db::get();
            let Some(idx) = db.find(code) else {
                let suggestions: Vec<String> = db.search(code, 5).into_iter().map(|i| db.apts[i as usize].code()).collect();
                return Err(if suggestions.is_empty() {
                    format!("no airport matches {code:?}")
                } else {
                    format!("no airport matches {code:?}; try one of: {}", suggestions.join(", "))
                });
            };
            let a = &db.apts[idx as usize];
            let (code, name) = (a.code(), a.name.clone());
            self.start_airport(idx, args.runway.clone(), args.approach.unwrap_or(false), true);
            self.paused = false;
            self.set_pause_menu(false);
            return Ok(json!({
                "ok": true,
                "status": "loading",
                "airport": code,
                "name": name,
                "note": "the airport layout is loading; poll get_aircraft_state until sim.airport_loading is false and mode is fly"
            }));
        }
        let (Some(lat), Some(lon)) = (args.latitude_deg, args.longitude_deg) else {
            return Err("give either airport, or both latitude_deg and longitude_deg".into());
        };
        let lat = lat.clamp(-90.0, 90.0).to_radians();
        let lon = lon.clamp(-180.0, 180.0).to_radians();
        let ground = self.ground_under(lat, lon).0;
        let alt_m = if let Some(ft) = args.altitude_msl_ft {
            ft / 3.28084
        } else {
            let agl = args.height_above_ground_ft.unwrap_or(2000.0).max(0.0) / 3.28084;
            ground.unwrap_or(0.0) + agl
        };
        let heading = args.heading_deg.map(|d| d.rem_euclid(360.0).to_radians()).unwrap_or_else(|| {
            let (heading, _, _) = self.heading_pitch_roll();
            heading.to_radians()
        });
        let speed = args.speed_kt.unwrap_or(self.spec.cruise * 1.94384).clamp(30.0, 1200.0) / 1.94384;
        let (was_menu, was_paused) = (self.mode == Mode::Menu, self.paused || self.pause_open);
        // An airport that is still loading must not arrive later and take us
        // somewhere else.
        self.airport_rx = None;
        self.pending_airport = None;
        let on_ground = args.on_ground.unwrap_or(false);
        if on_ground {
            let Some(h) = ground else {
                return Err("the ground is not loaded at that spot; place at an airport or in the air instead".into());
            };
            self.respawn_ground(lat, lon, h, heading, 0.0);
        } else {
            self.respawn_at(lat, lon, alt_m, heading, speed);
        }
        self.mode = Mode::Fly;
        self.menu.starting = false;
        self.paused = false;
        self.set_pause_menu(false);
        let (lat_d, lon_d, alt) = geo::pos_to_lla(self.plane.pos);
        Ok(json!({
            "ok": true,
            "status": "placed",
            "was_in_menu": was_menu,
            "was_paused": was_paused,
            "on_ground": self.plane.on_ground,
            "latitude_deg": num(lat_d.to_degrees()),
            "longitude_deg": num(lon_d.to_degrees()),
            "altitude_msl_ft": num(alt * 3.28084),
        }))
    }

    // ------------------------------------------------------------ screenshots

    fn start_screenshot(&mut self, args: &Value, reply: crossbeam_channel::Sender<Value>) {
        if !self.mcp_shots.is_empty() {
            let _ = reply.send(tools::error_result("A screenshot is already being captured; try again in a moment."));
            return;
        }
        let args: tools::ScreenshotArgs = match tools::parse(args) {
            Ok(a) => a,
            Err(e) => {
                let _ = reply.send(tools::error_result(e));
                return;
            }
        };
        let view = args.view.unwrap_or_else(|| "current".into()).to_ascii_lowercase();
        if !matches!(view.as_str(), "current" | "cockpit" | "chase") {
            let _ = reply.send(tools::error_result("view must be current, cockpit or chase"));
            return;
        }
        let hud = args.hud.unwrap_or(true);
        let mut restore_cam = None;
        if view == "cockpit" && self.cam_mode != 1 {
            restore_cam = Some(self.cam_mode);
            self.cam_mode = 1;
        } else if view == "chase" && self.cam_mode != 0 {
            restore_cam = Some(self.cam_mode);
            self.cam_mode = 0;
        }
        let mut restore_hud = None;
        if self.show_hud != hud {
            restore_hud = Some(self.show_hud);
            self.show_hud = hud;
        }
        let path = screenshot_path(self.frame, self.mcp_shot_n);
        self.mcp_shot_n += 1;
        self.shots.insert(0, (0.0, path.clone(), None));
        self.mcp_shots.push(PendingShot {
            path,
            reply,
            restore_cam,
            restore_hud,
            view,
            hud,
            deadline: Instant::now() + Duration::from_secs(8),
        });
    }

    /// After the frame has rendered: hand the written screenshots to background
    /// threads to encode, and put the camera and HUD back.
    pub(crate) fn finish_mcp_shots(&mut self) {
        if self.mcp_shots.is_empty() {
            return;
        }
        let mut keep = Vec::new();
        for shot in std::mem::take(&mut self.mcp_shots) {
            if let Some(cam) = shot.restore_cam {
                self.cam_mode = cam;
            }
            if let Some(hud) = shot.restore_hud {
                self.show_hud = hud;
            }
            if shot.path.exists() {
                std::thread::spawn(move || send_shot(shot));
            } else if Instant::now() >= shot.deadline {
                let _ = shot.reply.send(tools::error_result(format!("the screenshot could not be saved to {}", shot.path.display())));
            } else {
                keep.push(shot);
            }
        }
        self.mcp_shots = keep;
    }
}

fn screenshot_path(frame: u64, n: u64) -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
    let dir = base.join("fablegame").join("screenshots");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("mcp-{frame}-{n}.png"))
}

fn send_shot(shot: PendingShot) {
    let encoded = (|| -> Result<(Vec<u8>, u32, u32), String> {
        let img = image::open(&shot.path).map_err(|e| format!("reading {}: {e}", shot.path.display()))?;
        let img = img.thumbnail(1600, 1600);
        let (w, h) = (img.width(), img.height());
        let rgb = img.to_rgb8();
        let mut jpeg = Vec::new();
        image::DynamicImage::ImageRgb8(rgb)
            .write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .map_err(|e| format!("encoding the screenshot: {e}"))?;
        Ok((jpeg, w, h))
    })();
    let result = match encoded {
        Ok((jpeg, w, h)) => {
            let text = format!(
                "Screenshot ({}, HUD {}) saved to {}; {}x{} JPEG returned.",
                shot.view,
                if shot.hud { "on" } else { "off" },
                shot.path.display(),
                w,
                h
            );
            tools::image_result(tools::base64_encode(&jpeg), "image/jpeg", text)
        }
        Err(e) => tools::error_result(e),
    };
    let _ = shot.reply.send(result);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near_sfo() -> DVec3 {
        geo::lla_to_pos(37.6_f64.to_radians(), -122.4_f64.to_radians(), 3000.0)
    }

    #[test]
    fn adf_selects_nearest_station_with_matching_ident_or_frequency() {
        for query in ["SF", " sf ", "379"] {
            let st = Game::find_adf_station(query, near_sfo()).unwrap();
            assert_eq!(st.name, "Brijj", "query {query:?}");
            assert_eq!(st.freq_khz, 379);
        }
    }

    #[test]
    fn nav_selects_nearest_station_with_matching_ident_or_frequency() {
        for query in ["SAU", " sau ", "116.20"] {
            let st = Game::find_nav_station(query, near_sfo()).unwrap();
            assert_eq!(st.name, "Sausalito", "query {query:?}");
            assert_eq!(st.freq_khz, 116200);
        }
    }

    #[test]
    fn radio_lookup_preserves_ils_and_missing_station_handling() {
        let pos = near_sfo();
        let st = Game::find_nav_station("KSFO 28L", pos).unwrap();
        assert_eq!((st.kind, st.name.as_str(), st.ident.as_str()), (nav::Kind::Ils, "KSFO", "28L"));
        for query in ["", "  ", "no-such-station"] {
            assert!(Game::find_adf_station(query, pos).is_none());
            assert!(Game::find_nav_station(query, pos).is_none());
        }
    }
}
