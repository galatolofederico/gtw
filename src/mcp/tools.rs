//! The copilot's tool catalogue.
//!
//! The same definitions are used by the game (to answer calls) and by the
//! `gtw-mcp` bridge (to answer `tools/list` even when the game is not
//! running).  Units are the ones pilots use: feet, knots, ft/min, degrees
//! true, hPa and kg.

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// Every tool the copilot exposes.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Tool {
    GetAircraftState,
    GetNavigation,
    GetEnvironment,
    GetTraffic,
    GetAirportInfo,
    SetAircraftConfig,
    SetEngine,
    SetRadios,
    SetAutopilot,
    SetEnvironment,
    PlaceAircraft,
    TakeScreenshot,
}

impl Tool {
    pub const ALL: [Tool; 12] = [
        Tool::GetAircraftState,
        Tool::GetNavigation,
        Tool::GetEnvironment,
        Tool::GetTraffic,
        Tool::GetAirportInfo,
        Tool::SetAircraftConfig,
        Tool::SetEngine,
        Tool::SetRadios,
        Tool::SetAutopilot,
        Tool::SetEnvironment,
        Tool::PlaceAircraft,
        Tool::TakeScreenshot,
    ];

    pub fn from_name(name: &str) -> Option<Tool> {
        Some(match name {
            "get_aircraft_state" => Tool::GetAircraftState,
            "get_navigation" => Tool::GetNavigation,
            "get_environment" => Tool::GetEnvironment,
            "get_traffic" => Tool::GetTraffic,
            "get_airport_info" => Tool::GetAirportInfo,
            "set_aircraft_config" => Tool::SetAircraftConfig,
            "set_engine" => Tool::SetEngine,
            "set_radios" => Tool::SetRadios,
            "set_autopilot" => Tool::SetAutopilot,
            "set_environment" => Tool::SetEnvironment,
            "place_aircraft" => Tool::PlaceAircraft,
            "take_screenshot" => Tool::TakeScreenshot,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::GetAircraftState => "get_aircraft_state",
            Tool::GetNavigation => "get_navigation",
            Tool::GetEnvironment => "get_environment",
            Tool::GetTraffic => "get_traffic",
            Tool::GetAirportInfo => "get_airport_info",
            Tool::SetAircraftConfig => "set_aircraft_config",
            Tool::SetEngine => "set_engine",
            Tool::SetRadios => "set_radios",
            Tool::SetAutopilot => "set_autopilot",
            Tool::SetEnvironment => "set_environment",
            Tool::PlaceAircraft => "place_aircraft",
            Tool::TakeScreenshot => "take_screenshot",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Tool::GetAircraftState => "Read the complete live state of the aircraft and the simulation: position, attitude, speeds, configuration, engine, fuel, electrics, failures, damage, warnings, radios and time. Read-only. Units are feet, knots, ft/min, degrees true, hPa and kg.",
            Tool::GetNavigation => "Read the radios and navigation situation: NAV1 and ADF, CDI, TO/FROM, glide slope and DME, plus the nearest VORs, NDBs, ILS and airports. Read-only.",
            Tool::GetEnvironment => "Read the weather and clock: wind, gusts, clouds, visibility, QNH, turbulence, the live-weather report and the time of day. Read-only.",
            Tool::GetTraffic => "Read the live ADS-B traffic around the aircraft: callsign, type, altitude, speed, distance and bearing. Read-only.",
            Tool::GetAirportInfo => "Read the current or a named airport: runways, active runway, ILS, and on an approach the distance, centreline error and glide-path deviation. Read-only.",
            Tool::SetAircraftConfig => "Change the aircraft's configuration: throttle/power lever, flaps notch or delta, gear, speedbrake, parking brake, mixture and trim. It cannot move the stick. Returns the resulting configuration.",
            Tool::SetEngine => "Start or stop the engine, or set the magnetos/fuel control and the starters. Returns the resulting engine state.",
            Tool::SetRadios => "Tune NAV1 or the ADF by ident or frequency, set the OBS, or set the altimeter setting. Returns the resulting radio state.",
            Tool::SetAutopilot => "Set the autopilot: engage or disconnect it, pick its lateral mode (hdg, nav: follow NAV1, apr: the ILS localizer and glide slope, roll: wings level) and vertical mode (alt, vs, pitch), and set the heading bug, preselected altitude, vertical speed and the autothrottle's speed. NAV and APR need NAV1 tuned (set_radios). A firm input from the pilot disconnects it. Returns the resulting autopilot state.",
            Tool::SetEnvironment => "Set the weather and/or the clock. Any weather change turns live weather off; any manual time change turns real time off. Returns the new environment.",
            Tool::PlaceAircraft => "Move the aircraft: an airport code with an optional runway or final approach, or an airborne latitude/longitude/altitude/heading/speed. Airport placement loads in the background, so poll get_aircraft_state until it is flying.",
            Tool::TakeScreenshot => "Capture the next rendered frame as a JPEG and return it as an MCP image, plus the path of the saved PNG. Defaults to the current view with the HUD visible. Use after changing the view or when instruments or the outside world matter.",
        }
    }

    pub fn annotations(self) -> Value {
        match self {
            Tool::GetAircraftState | Tool::GetNavigation | Tool::GetEnvironment | Tool::GetTraffic | Tool::GetAirportInfo => {
                json!({ "readOnlyHint": true, "openWorldHint": false })
            }
            _ => json!({ "readOnlyHint": false, "destructiveHint": true, "openWorldHint": false }),
        }
    }

    pub fn schema(self) -> Value {
        match self {
            Tool::GetAircraftState => json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            Tool::GetNavigation => json!({
                "type": "object",
                "properties": {
                    "radius_nm": { "type": "number", "minimum": 1.0, "maximum": 500.0, "default": 50.0, "description": "How far to look for airports and navaids, nautical miles." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 8, "description": "Most entries per list." }
                },
                "additionalProperties": false
            }),
            Tool::GetEnvironment => json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
            Tool::GetTraffic => json!({
                "type": "object",
                "properties": {
                    "radius_nm": { "type": "number", "minimum": 1.0, "maximum": 200.0, "default": 40.0, "description": "How far around the aircraft to look, nautical miles." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 15, "description": "Most contacts to return, nearest first." }
                },
                "additionalProperties": false
            }),
            Tool::GetAirportInfo => json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "ICAO or IATA code (KSFO, SFO, LHR). Omit for the nearest loaded airport and the current approach guidance." }
                },
                "additionalProperties": false
            }),
            Tool::SetAircraftConfig => json!({
                "type": "object",
                "properties": {
                    "throttle": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Throttle/power lever, 0 idle to 1 full power (dry thrust on a jet)." },
                    "flaps_notch": { "type": "integer", "minimum": 0, "maximum": 10, "description": "Absolute flap lever notch (0 = up, higher = more). Clamped to the aircraft's notches." },
                    "flaps_delta": { "type": "integer", "minimum": -1, "maximum": 1, "description": "Move the flap lever one notch: -1 up, +1 down. Ignored when flaps_notch is given." },
                    "gear_down": { "type": "boolean", "description": "Landing gear handle: true down, false up. The squat switch blocks retraction on the ground." },
                    "speedbrake": { "type": "boolean", "description": "Speedbrake or braking parachute deployed." },
                    "parking_brake": { "type": "boolean", "description": "Parking brake set or released." },
                    "mixture": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Mixture lever, 0 idle cut-off to 1 full rich. Piston aircraft only." },
                    "trim_deg": { "type": "number", "minimum": -3.0, "maximum": 14.0, "description": "Pitch trim setting in degrees. The simulated auto-trim normally manages this." }
                },
                "additionalProperties": false
            }),
            Tool::SetEngine => json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["start", "stop", "ignition_on", "ignition_off", "starter_on", "starter_off"],
                        "description": "start/stop run the normal start or shutdown sequence; ignition controls the magnetos on a piston or the fuel control on a jet; starters are for jets."
                    }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
            Tool::SetRadios => json!({
                "type": "object",
                "properties": {
                    "nav1": { "type": "string", "description": "NAV1 station: an ident (BUB, EBBR 25L) or a VOR frequency (114.60)." },
                    "adf": { "type": "string", "description": "ADF station: an NDB ident or its frequency in kHz (377)." },
                    "obs_deg": { "type": "number", "minimum": 0.0, "maximum": 360.0, "description": "OBS course, degrees true." },
                    "altimeter_hpa": { "description": "Altimeter setting in hPa, or the string \"qnh\" to follow the QNH again.", "oneOf": [{ "type": "number", "minimum": 850.0, "maximum": 1100.0 }, { "type": "string", "enum": ["qnh"] }] }
                },
                "additionalProperties": false
            }),
            Tool::SetAutopilot => json!({
                "type": "object",
                "properties": {
                    "engaged": { "type": "boolean", "description": "Autopilot on (true) or off (false)." },
                    "lateral": { "type": "string", "enum": ["hdg", "nav", "apr", "roll"], "description": "Lateral mode." },
                    "vertical": { "type": "string", "enum": ["alt", "vs", "pitch"], "description": "Vertical mode (the glide slope comes with apr)." },
                    "heading_deg": { "type": "number", "minimum": 0.0, "maximum": 360.0, "description": "Heading bug, degrees true." },
                    "altitude_ft": { "type": "number", "minimum": 0.0, "maximum": 80000.0, "description": "Preselected altitude, feet: ALT holds it when reached, VS levels off there." },
                    "vs_fpm": { "type": "number", "minimum": -6000.0, "maximum": 6000.0, "description": "Vertical speed for VS, ft/min." },
                    "autothrottle": { "type": "boolean", "description": "Autothrottle holding speed_kt." },
                    "speed_kt": { "type": "number", "minimum": 40.0, "maximum": 2000.0, "description": "Indicated airspeed the autothrottle holds, knots." }
                },
                "additionalProperties": false
            }),
            Tool::SetEnvironment => json!({
                "type": "object",
                "properties": {
                    "weather_live": { "type": "boolean", "description": "Let live weather set the weather (true), or keep hand-set weather (false)." },
                    "clouds": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Cloud cover." },
                    "cloud_base_ft": { "type": "number", "minimum": 0.0, "description": "Cloud base, feet above the ground." },
                    "cloud_depth_ft": { "type": "number", "minimum": 0.0, "description": "Cloud layer thickness, feet." },
                    "storms": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Cumulonimbus storm-cell amount." },
                    "alto": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Mid-level cloud amount." },
                    "visibility_km": { "type": "number", "minimum": 10.0, "maximum": 100.0, "description": "Visibility through haze, km. The model's achievable range is about 11-100 km." },
                    "wind_speed_kt": { "type": "number", "minimum": 0.0, "maximum": 150.0, "description": "Mean wind at 10 m above the ground, knots." },
                    "wind_from_deg": { "type": "number", "minimum": 0.0, "maximum": 360.0, "description": "Direction the wind blows from, degrees true." },
                    "gusts": { "type": "number", "minimum": 0.0, "maximum": 1.0, "description": "Gustiness, 0 calm to 1 rough." },
                    "time_utc_hours": { "type": "number", "minimum": 0.0, "maximum": 24.0, "description": "UTC hour to set, 0..24." },
                    "day_of_year": { "type": "number", "minimum": 0.0, "maximum": 365.0, "description": "Day of year, 0..365." },
                    "time_rate": { "type": "number", "minimum": 0.0, "maximum": 3600.0, "description": "Game seconds per real second." },
                    "real_time": { "type": "boolean", "description": "Follow the real time and date." }
                },
                "additionalProperties": false
            }),
            Tool::PlaceAircraft => json!({
                "type": "object",
                "properties": {
                    "airport": { "type": "string", "description": "ICAO or IATA code. Takes priority over latitude/longitude." },
                    "runway": { "type": "string", "description": "Runway ident to use at the airport (28L, 09R)." },
                    "approach": { "type": "boolean", "default": false, "description": "Place on final approach rather than on the runway threshold." },
                    "latitude_deg": { "type": "number", "minimum": -90.0, "maximum": 90.0, "description": "Airborne placement latitude, degrees." },
                    "longitude_deg": { "type": "number", "minimum": -180.0, "maximum": 180.0, "description": "Airborne placement longitude, degrees." },
                    "altitude_msl_ft": { "type": "number", "description": "Airborne placement altitude, feet MSL." },
                    "height_above_ground_ft": { "type": "number", "description": "Airborne placement height above the ground, feet. Used when altitude_msl_ft is absent." },
                    "heading_deg": { "type": "number", "minimum": 0.0, "maximum": 360.0, "description": "Heading, degrees true." },
                    "speed_kt": { "type": "number", "minimum": 30.0, "maximum": 1200.0, "description": "Initial true airspeed, knots. Defaults to the type's cruise speed." },
                    "on_ground": { "type": "boolean", "default": false, "description": "For a latitude/longitude placement, stand on the ground there. The terrain must already be loaded; airport placement is preferable." }
                },
                "additionalProperties": false
            }),
            Tool::TakeScreenshot => json!({
                "type": "object",
                "properties": {
                    "view": { "type": "string", "enum": ["current", "cockpit", "chase"], "default": "current", "description": "Camera view to capture. The previous view is restored afterwards." },
                    "hud": { "type": "boolean", "default": true, "description": "Show the HUD in the capture." }
                },
                "additionalProperties": false
            }),
        }
    }
}

// ---------------------------------------------------------------- arguments

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NavigationArgs {
    pub radius_nm: Option<f64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrafficArgs {
    pub radius_nm: Option<f64>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirportArgs {
    pub code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AircraftConfigArgs {
    pub throttle: Option<f64>,
    pub flaps_notch: Option<i32>,
    pub flaps_delta: Option<i32>,
    pub gear_down: Option<bool>,
    pub speedbrake: Option<bool>,
    pub parking_brake: Option<bool>,
    pub mixture: Option<f64>,
    pub trim_deg: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineArgs {
    pub action: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RadiosArgs {
    pub nav1: Option<String>,
    pub adf: Option<String>,
    pub obs_deg: Option<f64>,
    pub altimeter_hpa: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutopilotArgs {
    pub engaged: Option<bool>,
    pub lateral: Option<String>,
    pub vertical: Option<String>,
    pub heading_deg: Option<f64>,
    pub altitude_ft: Option<f64>,
    pub vs_fpm: Option<f64>,
    pub autothrottle: Option<bool>,
    pub speed_kt: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentArgs {
    pub weather_live: Option<bool>,
    pub clouds: Option<f64>,
    pub cloud_base_ft: Option<f64>,
    pub cloud_depth_ft: Option<f64>,
    pub storms: Option<f64>,
    pub alto: Option<f64>,
    pub visibility_km: Option<f64>,
    pub wind_speed_kt: Option<f64>,
    pub wind_from_deg: Option<f64>,
    pub gusts: Option<f64>,
    pub time_utc_hours: Option<f64>,
    pub day_of_year: Option<f64>,
    pub time_rate: Option<f64>,
    pub real_time: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceArgs {
    pub airport: Option<String>,
    pub runway: Option<String>,
    pub approach: Option<bool>,
    pub latitude_deg: Option<f64>,
    pub longitude_deg: Option<f64>,
    pub altitude_msl_ft: Option<f64>,
    pub height_above_ground_ft: Option<f64>,
    pub heading_deg: Option<f64>,
    pub speed_kt: Option<f64>,
    pub on_ground: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotArgs {
    pub view: Option<String>,
    pub hud: Option<bool>,
}

// ---------------------------------------------------------------- results

pub fn parse<T: DeserializeOwned>(args: &Value) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|e| format!("invalid arguments: {e}"))
}

/// A JSON value as the text content of a tool result.
pub fn json_result(value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
}

pub fn error_result(message: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": message.into() }], "isError": true })
}

pub fn image_result(data: String, mime: &str, text: String) -> Value {
    json!({
        "content": [
            { "type": "image", "data": data, "mimeType": mime },
            { "type": "text", "text": text }
        ],
        "isError": false
    })
}

/// `tools/list`'s result.
pub fn catalog_json() -> Value {
    let tools: Vec<Value> = Tool::ALL
        .iter()
        .map(|t| {
            json!({
                "name": t.name(),
                "description": t.description(),
                "inputSchema": t.schema(),
                "annotations": t.annotations()
            })
        })
        .collect();
    json!({ "tools": tools })
}

/// Standard base64 with padding.
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((n >> 18) & 63) as usize] as char);
        out.push(TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_well_formed() {
        let mut names = Vec::new();
        for t in Tool::ALL {
            assert!(Tool::from_name(t.name()) == Some(t));
            assert!(!names.contains(&t.name()));
            names.push(t.name());
            let schema = t.schema();
            assert_eq!(schema["type"], "object", "{}", t.name());
            assert!(schema["properties"].is_object(), "{}", t.name());
            assert!(t.description().contains("Read") || t.description().contains("Change") || t.description().contains("Start") || t.description().contains("Tune") || t.description().contains("Set") || t.description().contains("Move") || t.description().contains("Capture"), "{}", t.name());
        }
        assert_eq!(names.len(), Tool::ALL.len());
        assert_eq!(catalog_json()["tools"].as_array().unwrap().len(), Tool::ALL.len());
    }

    #[test]
    fn base64_matches_rfc_examples() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode(b"Man"), "TWFu");
    }

    #[test]
    fn arguments_parse() {
        let a: NavigationArgs = parse(&json!({ "radius_nm": 25.0, "limit": 3 })).unwrap();
        assert_eq!((a.radius_nm, a.limit), (Some(25.0), Some(3)));
        let a: AircraftConfigArgs = parse(&json!({ "throttle": 0.7, "flaps_delta": 1 })).unwrap();
        assert_eq!((a.throttle, a.flaps_delta), (Some(0.7), Some(1)));
        let a: PlaceArgs = parse(&json!({ "airport": "ksfo", "approach": true })).unwrap();
        assert_eq!((a.airport.as_deref(), a.approach), (Some("ksfo"), Some(true)));
        assert!(parse::<EngineArgs>(&json!({ "action": "start" })).is_ok());
        assert!(parse::<EngineArgs>(&json!({ "action": "fly" })).is_ok()); // enum is checked by the handler
        assert!(parse::<EngineArgs>(&json!({ "nope": 1 })).is_err());
    }
}
