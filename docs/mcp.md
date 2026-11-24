# AI copilot over MCP

GTW can expose the running aircraft and its world to an AI assistant
through the [Model Context Protocol](https://modelcontextprotocol.io).  The
assistant works as a copilot: it reads the instruments and the environment,
tunes radios, handles the throttle, flaps, gear and engine state, changes weather
or time, repositions the aircraft when asked, and captures screenshots of the
view.  It does not move the stick.

The setup has two processes:

```
AI client (OpenCode, Claude Code, Claude Desktop, ...)
        │ MCP over stdio
        ▼
gtw-mcp                    the stdio bridge
        │ JSON-RPC on 127.0.0.1:42761
        ▼
gtw (the running simulator)
```

The bridge can be started before the game: `tools/list` works, and tool calls
answer with a clear "start the simulator" error until the game is up.  The
game's control service binds only to localhost.

## Build and run

```sh
cargo build --release
./target/release/gtw
```

The game listens on `127.0.0.1:42761` by default and writes a per-session secret token to `<config dir>/gtw/mcp-<port>.token` (mode `0600` on Unix).  `--mcp-port PORT` changes the port; `--no-mcp` disables the service for one run.  The bridge uses the same default port, reads the token automatically (or from `GTW_MCP_TOKEN`), and reads `GTW_MCP_PORT` if the port was changed, or accepts `--port PORT`.

## Client configuration

### OpenCode

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "servers": {
      "gtw": {
        "type": "local",
        "command": ["/absolute/path/to/gtw-mcp"]
      }
    }
  }
}
```

or:

```sh
opencode mcp add gtw -- /absolute/path/to/gtw-mcp
```

### Claude Code

```sh
claude mcp add gtw /absolute/path/to/gtw-mcp
```

### Claude Desktop

```json
{
  "mcpServers": {
    "gtw": {
      "command": "/absolute/path/to/gtw-mcp",
      "args": []
    }
  }
}
```

Put `GTW_MCP_PORT` in the server's environment if you changed the
game's port.

## A copilot prompt

The bridge's `initialize` instructions already tell the assistant what it can
do.  A useful first request is:

> You are my copilot. Call `get_aircraft_state`, then tell me what phase of
> flight we are in and what needs attention. Use `get_airport_info` and
> `get_navigation` for the approach; do not change weather, time or position
> unless I ask.

## Tools

Units are feet, knots, ft/min, degrees true, hPa and kg.

### Read-only

| Tool | What it returns |
|---|---|
| `get_aircraft_state` | Position, altitude and AGL, heading/pitch/roll, IAS/TAS/GS, vertical speed, Mach, angle of attack, load factor, stall/overspeed, throttle, flaps, gear, speedbrake, trim, mixture, engine, fuel and endurance, electrics, failures, damage, warnings, radios and time. |
| `get_navigation` | NAV1 and ADF stations and readings, CDI, TO/FROM, glide slope, DME, OBS, altimeter setting, nearest airports, VORs, NDBs and ILS. Optional `radius_nm` and `limit`. |
| `get_environment` | Cloud cover/base/depth/ceiling, storms, haze, visibility, wind and gusts, QNH, turbulence and updraughts, the live METAR source, UTC/local time, day and time rate. |
| `get_traffic` | Live ADS-B contacts around the aircraft: callsign, type, altitude, speed, vertical speed, distance and bearing. Optional `radius_nm` and `limit`. |
| `get_airport_info` | The loaded airport or one named by ICAO/IATA code: runways, active runway, ILS, wind, and structured approach guidance (distance, centreline offset, height above threshold, glide-path deviation, on centreline/glide path). |

### Changing the aircraft

| Tool | What it changes |
|---|---|
| `set_aircraft_config` | Throttle/power lever, flap notch or one-notch move, gear, speedbrake/braking parachute, parking brake, mixture (pistons) and pitch trim. |
| `set_engine` | Normal start/stop, ignition or fuel control, and the jet starters. |
| `set_radios` | NAV1 by ident or VOR frequency, ADF by ident or kHz, OBS, and the altimeter setting (a value, or `"qnh"`). |
| `set_autopilot` | Engage or disconnect the autopilot; lateral mode (`hdg`, `nav`, `apr`, `roll`), vertical mode (`alt`, `vs`, `pitch`), heading bug, preselected altitude, vertical speed, autothrottle and its speed. `get_aircraft_state` reports it under `autopilot`. |
| `set_environment` | Clouds, cloud base/depth, storms, mid-level cloud, visibility, wind and gusts, time of day, day of year, clock rate, live weather and real time. Setting weather turns live weather off; setting the clock turns real time off unless asked otherwise. |
| `place_aircraft` | An airport code with an optional runway or final approach, or an airborne latitude/longitude/altitude/heading/speed. Airport placement loads the layout in the background. |

### Seeing the aircraft

`take_screenshot` captures the next rendered frame.  Optional `view` is
`current` (default), `cockpit` or `chase`; optional `hud` defaults to `true`.
The camera and HUD are restored afterwards.  The result is an MCP image (JPEG,
downscaled to at most 1600 px) plus the path of the saved PNG under the
GTW config folder's `screenshots` directory.

## Safety and behaviour

- The MCP service is on by default and binds only to `127.0.0.1`.  Use
  `--no-mcp` when you do not want it.
- The stick is deliberately not writable: the human pilot keeps positive
  control of attitude.  The copilot can set the throttle; the pilot's own
  throttle input from the keys, gamepad or a bound lever takes it back.
- The copilot can engage the autopilot (`set_autopilot`), which then flies
  the attitude; a firm input on the controls disconnects it at once, as in
  the aircraft, and the pilot's small inputs still add to it.
- A tool call is processed on the game thread at the start of the next frame,
  so reads are live rather than cached.  When the game is paused or in the
  menu, read tools still work; `place_aircraft` can start or move a flight.
- Screenshots wait for the next rendered frame and are encoded on a background
  thread, so the sim keeps running.
- The bridge speaks the tools-only legacy MCP handshake (`2025-11-25`), which
  is what OpenCode uses by default and is broadly compatible.  Protocol
  `2026-07-28` clients should be configured for the legacy handshake.
