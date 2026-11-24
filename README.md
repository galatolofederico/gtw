<div align="center">

# Fable Flight

### An Open-World Flight Simulator Built in Pure Rust

Explore the skies with real-world weather, live air traffic, and an immersive open-world flying experience.

*Created with Claude Fable 5.2, Opus 5.5 and other AI's*

<br>

[![Discord](https://img.shields.io/badge/Discord-5865F2?style=for-the-badge&logo=discord&logoColor=white)](https://discord.gg/9akYzMRBc6)
[![Rust](https://img.shields.io/badge/Rust-CE422B?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
<br>
</div>
<div align="center">
  <video
    src="https://github.com/user-attachments/assets/4f8833da-13c0-4d20-a50b-c0acf2d88df1"
    width="800"
    controls>
  </video>
</div>

## Download
Download the [latest release](https://github.com/Rolandjg/fableflight/releases/latest), unpack it, and run **`fableflight-launcher`** (`fableflight-launcher.exe` on Windows).

## Compiling

Requires **Rust 1.85+** (edition 2024). Note that the Cargo crate and main simulator binary are named `fablegame` (the optional AI copilot bridge binary is `fableflight-mcp`, and the launcher is `fableflight-launcher`), while the application is Fable Flight.

### Platform prerequisites

- **Linux (Debian / Ubuntu):**
  ```sh
  sudo apt-get update && sudo apt-get install -y libasound2-dev libudev-dev pkg-config
  ```
- **Linux (Fedora):**
  ```sh
  sudo dnf install alsa-lib-devel systemd-devel pkgconf-pkg-config
  ```
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Windows:** [Rust](https://www.rust-lang.org/tools/install) with the MSVC C++ Build Tools. On Windows x64, use **Visual Studio Installer** to modify your Build Tools installation and add **C++ ATL for x64/x86** under **Individual components**. ATL provides `atls.lib`, which the bundled DirectX shader compiler needs at link time.

### Build and run

```sh
cargo run --release
```

Or build the release binaries (`target/release/fablegame`, `target/release/fableflight-mcp` and `target/release/fableflight-launcher`) with:

```sh
cargo build --release --locked
```

## Features

- **Aircraft:** Cessna 172RG, Robin DR400, Beechcraft A36 Bonanza, Boeing 747, Gulfstream G650, F/A-18, F-4, F-22, Concorde, SR-71, Starship, and downloadable [FlightGear aircraft](https://wiki.flightgear.org/FGAddon).
- **Flight systems:** fuel, mass and balance, electrical failures, clickable cockpits, radio navigation, and autopilot.
- **World:** real METAR weather, seasonal & live snow cover, satellite/road minimap, live ADS-B traffic, and multiplayer.
- **Controls:** remappable keyboard, joystick, throttle, pedals, and gamepad. Configure them in **Settings → Controls**.
- **Arcade mode:** optional boosts, missiles, and in-flight respawns. Enable in **Settings → General** or pass `--arcade`.
- **AI copilot:** optional MCP integration; see [docs/mcp.md](docs/mcp.md).
- **Upscaling:** bilinear, AMD FSR 1 and FSR 3.1 built in, and AMD FSR 4.1 downloaded from AMD on request (Windows, Radeon RX 6000 or newer), with a suggestion for your GPU. In **Settings → Graphics → Upscaling**; see [docs/upscaling.md](docs/upscaling.md).

The UI supports English (`en`), French (`fr`), and Korean (`ko`). Settings and key bindings are saved automatically.

## Default Controls

All bindings can be customized in **Settings → Controls**. In cockpit view, hover over switches, levers, and knobs to click, hold, or scroll them directly with the mouse (or the gamepad's aim dot).

| Action | Default Key |
| --- | --- |
| Pitch down / up | `W` / `S` |
| Roll left / right | `A` / `D` |
| Rudder left / right | `Q` / `E` |
| Throttle up / down | `Left Shift` / `Left Ctrl` |
| Pitch trim down / up | `Numpad 7` / `Numpad 1` |
| Wheel brakes / Parking brake | `B` / `.` |
| Flaps down / up a notch | `F` / `V` |
| Landing gear | `L` |
| Speedbrake / Braking chute | `/` |
| Thrust reversers (hold) | `Z` |
| Canopy open / close | `6` |
| Boost / Afterburner | `Space` |
| Auto-start / stop engine | `F4` |
| Battery master / Magnetos / Starter | `Insert` / `I` / `O` |
| Mixture richer / leaner | `Page Up` / `Page Down` |
| Autopilot toggle / HDG / NAV / APR / ALT | `F5` / `7` / `8` / `9` / `0` |
| Heading bug / Altitude preselect | `-` `=` / `;` `'` |
| Camera (chase / cockpit) | `C` |
| HUD / Minimap toggle | `H` / `` ` `` |
| Go to place / Respawn / Pause | `G` / `R` / `P` |
| Flight panel / Pause menu | `Tab` / `Esc` |
| Sound toggle / Screenshot | `U` / `F12` |

## Configuration & Data Paths

| Platform | Config (settings, bindings, Google key, MCP token) | Cache (tiles, downloaded hangar cache) |
| --- | --- | --- |
| **Linux** | `~/.config/fablegame/` | `~/.cache/fablegame/` |
| **macOS** | `~/Library/Application Support/fablegame/` | `~/Library/Caches/fablegame/` |
| **Windows** | `%APPDATA%\fablegame\` | `%LOCALAPPDATA%\fablegame\` |

When using optional Google Maps imagery/3D tiles, prefer setting the `FABLE_GOOGLE_KEY` environment variable (or entering the key in **Settings → Map**) rather than passing `--google-key` on the command line.

## Troubleshooting

- **GPU startup or driver issues:** Run from a terminal to see GPU error output. Make sure Vulkan (Linux/Windows), DX12 (Windows), or Metal (macOS) drivers are up to date. On integrated GPUs, try `--graphics very-low` or set `FABLE_MSAA=1`.
- **Low frame rate / laptops without a dedicated GPU:** In **Settings → Graphics**, pick the **Very Low** preset (the first start on integrated graphics picks Very Low on Intel, Low elsewhere). For more speed, set **Clouds** to Off and lower **Render resolution** (down to 35%). A 30 fps **Frame limit** keeps the laptop cooler and saves battery. To see where the GPU time goes, run with `FABLE_GPU_TIMES=1`: it logs the time spent in each render stage.
- **No sound or switched headphones:** Audio automatically attempts to reopen the default output device every few seconds after a stream error. Use `U` to toggle mute or `--mute` to launch silently.
- **Offline / slow network:** Launch with `--offline` to fly using only cached tiles and built-in data.
- **Linux gamepads not detected:** Ensure your user has read access to `/dev/input/event*` (usually the `input` group) and `libudev` is installed.
- **Invalid command-line flags:** Run `fablegame --help` to see all supported options.
- **The game stops while starting from the launcher:** the launcher shows the game's last log lines, and the full log of the last run is in the cache folder as `last-run.log` (Home → Open the last game log).

## Multiplayer

Multiplayer is on by default, under a random pilot name (such as `SwiftKestrel27`). Change the name, pick a server, or turn multiplayer off in the **Multiplayer** settings. Offline and scripted runs (`--offline`, `--shot`, `--record`, `--bench`, `--headless`) stay off the network unless `--mp` is given. To host a relay:

```sh
cd relay
docker compose up -d
```
Allow UDP port `7480` through your firewall.

Read [docs/relay.md](docs/relay.md) for more info.

## Assets & Attributions

The source code is licensed under the MIT License (see [LICENSE](LICENSE)). Bundled data and media assets include:

- **Airports & navigation aids (`assets/airports.tsv.gz`, `assets/navaids.tsv.gz`):** Derived from [OurAirports](https://ourairports.com/data/) (Public Domain).
- **Night lights (`assets/night_lights.bin`):** Block-wise zlib-compressed global radiance tiles packed from NASA Earth Observatory's Black Marble 2016 dataset (Public Domain).
- **Fonts (`assets/fonts/`):** Barlow and IBM Plex Mono, licensed under the SIL Open Font License 1.1 (see `assets/fonts/OFL-Barlow.txt` and `assets/fonts/OFL-IBMPlexMono.txt`).
- **AMD FidelityFX Super Resolution:** `src/shaders/fsr3.wgsl` and the FSR 1/RCAS functions in `src/shaders/post.wgsl` are ported from AMD's FSR SDK v2.4.0 (MIT; notice in [docs/upscaling.md](docs/upscaling.md)). AMD's FSR 4 runtime is not bundled: it is downloaded from AMD at the player's request, under AMD's licence.
- **Google Maps badge (`assets/google_maps_logo.png`):** Displayed when optional Google Maps Platform imagery or Photorealistic 3D Tiles are enabled, per Google Maps attribution requirements.
- **Aircraft models, textures, and audio (`assets/*.bin.gz`, `assets/*_paint.jpg`, `assets/audio/`, `assets/ui/`):** Project-authored or procedural assets bundled with Fable Flight. Optional downloadable aircraft from [FGAddon](https://wiki.flightgear.org/FGAddon) carry their own per-aircraft licenses recorded in each installed aircraft's `source.json`.

## Contributing

Contributions are welcome, see [AGENTS.md](AGENTS.md) for guidance on coding agent standards.
