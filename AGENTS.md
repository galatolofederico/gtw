# Agent instructions for GTW

These instructions apply throughout this repository. GTW (Grand Theft World) is a fork of FableFlight: an open-world sandbox on the real Earth where the player walks, drives, and flies. Keep the upstream aircraft rules below for every flight change.

## Project scope

- GTW v1 is single-player on the ground. Do not add pedestrians, crowds, police or wanted levels, building interiors, missions, an economy, swimming, motorcycles, bicycles, boats, trains, ground weapons, or multiplayer ground play.
- Flight-only arcade weapons stay behind the off-by-default arcade toggle. Do not make them ground weapons or turn them on by default.
- Keep the existing aircraft and flight systems. Do not regress the aircraft flight model, `Plane::step`, or `Mode::Fly` while working on ground play.
- Tunnels remain at grade and are not enterable; bridges are decks, not separate worlds. Do not add interiors for either.
- The player-facing product, binaries (`gtw`, `gtw-mcp`, `gtw-launcher`), window titles, config/cache paths (`<config>/gtw/`, `<cache>/gtw/`), README, and credits are GTW. The upstream FableFlight credit, the MIT license, and the ESA WorldCover / Planetary Computer attribution must remain.
- Get persistent paths from `src/paths.rs`; do not join `gtw` or the legacy `fablegame` folder by hand. The legacy `fablegame` config/cache names are read only by the one-time migration, and `<data>/fablegame/` keeps its historical name for downloaded aircraft and AMD's FSR runtime.
- Launcher updates must never install an upstream FableFlight release over GTW. Keep automatic update checks off by default and point the repository at `galatolofederico/gtw`.

## Ground-play conventions

- `Mode` selects `Walk`, `Drive`, or `Fly`. `Game::focus_pos()` drives streaming, weather, the minimap, and the save; branch on the mode at the known call sites rather than making `self.plane` optional.
- Character and vehicle physics use `World::surface_at`, `WorldQuery`, and `SurfaceWeather`. Missing terrain holds the last valid sample; never let the player fall through a loading world.
- Bindings are context-aware (`bind::Context::{Foot, Car, Flight}`). Add new foot/car actions to the matching table and their `action.<id>` label to `eng.lng` and `fr.lng`; global actions stay available in every context.
- A clicked or keyed control must do the same thing: hotspots, key bindings, and gamepad layouts drive existing actions, not separate code paths.
- Ground HUD shows speed, gear, health, and interactions only; hide flight-only instruments outside `Mode::Fly`.
- Save data is a versioned resume snapshot of the player, current vehicle, and aircraft, not the whole traffic world. Corrupt or future saves are ignored with a message, and writes are atomic.
- Run `cargo check --all-targets`, `cargo test`, `cargo test --test shaders`, and (from the rebrand on) `cargo test --test mcp` and `cargo test --test gtw_smoke` for ground changes.

## Learn from the existing aircraft

Read the closest existing aircraft before adding or substantially changing one. Reuse the project's integration patterns while verifying real-world accuracy independently; existing values are not authoritative aircraft specifications.

- `src/aircraft/mod.rs`: aircraft registry and shared `Spec`, including geometry, mass, engines, gear, handling, cameras, and sound.
- `src/aircraft/c172/` and `src/aircraft/dr400/`: aircraft and cabins built in Rust with the toolkit in `src/airframe.rs`.
- `src/aircraft/fa18/` and `assets/blender/fa18.py`: Blender exterior geometry, exported moving pieces, and a Rust cockpit. Other Blender aircraft have their own scripts under `assets/blender/`.
- `src/aircraft/b747/deck.rs` and `src/aircraft/fa18/pit.rs`: detailed cockpit layouts, labels, and live instruments.
- `src/shaders/plane.wgsl` and the relevant cockpit shaders: materials, liveries, surface detail, and instrument rendering.
- `src/plane.rs` and `src/systems.rs`: shared flight physics and aircraft systems.

## Research and dimensions

- Identify the exact aircraft variant and configuration being reproduced. Keep its geometry, cockpit, engines, weights, and performance consistent with that variant.
- Look up real references before modeling or tuning. Prefer manufacturer specifications, pilot operating handbooks, flight manuals, technical drawings, and clear photographs of the correct variant. Cross-check conflicting figures.
- Record source URLs and the relevant figures in aircraft comments or accompanying documentation. State units, configuration, and any assumptions. Mark estimates and unavailable data honestly; do not invent precision or claim unverified accuracy.
- Build at real scale. Match length, wingspan, height, wing area, sweep, dihedral, fuselage cross-sections, tail geometry, engine locations, landing gear, and wheel sizes to documented measurements.
- Use metres for geometry and the existing units documented by `Spec`. The game body frame is +X right, +Y up, -Z forward, with the origin at the reference centre of gravity. Check coordinate conversions when exporting from Blender.
- Keep rendered geometry, collision/contact points, gear locations, hinges, pilot eye position, and aerodynamic dimensions aligned.

## Cockpit realism

- Reproduce the actual cockpit's proportions and layout using photographs and manuals: panel shapes, instrument positions, consoles, seats, controls, pedals, canopy or windows, and pilot sightlines.
- Use realistic materials, labels, fasteners, bezels, switches, lighting, and display symbology. Instruments must be readable from the normal pilot view.
- Drive implemented gauges, displays, annunciators, and animated controls from the appropriate simulated state. Document any controls or systems that remain decorative or simplified.
- Check the cockpit from the seated pilot's view and exterior views for scale, visibility, clipping, and fit inside the airframe.

## Clickable cockpit

Every crewed aircraft's cockpit can be worked with the mouse, and with the gamepad's aim dot, from the cockpit view. The pilot looks at a control, such as the throttle, and clicks it, holds it, or turns the wheel on it. This only works if each clickable target sits exactly on the control the pilot sees. New aircraft must include it, and substantial cockpit changes must keep it working. See PRs #33 (C172), #57 (DR 400) and #58 (747), and use `src/aircraft/dr400/cabin.rs`, `src/aircraft/b747/deck.rs` and `src/aircraft/fa18/pit.rs` as the examples.

- **How it works:** `aircraft::Hotspot` is a sphere in the body frame. Its `click`, `hold` and `wheel` fields name action ids from `bind::ACTIONS`, or from `c172::cabin::MOUSE_ACTIONS` for actions that only the mouse can do. A hotspot does exactly what that action's key does, so do not add separate game logic for the mouse. `aircraft::pick` finds the nearest sphere along the pointer ray. `cockpit::occluded` then rejects any hit that lies behind solid cockpit geometry.
- **Attach hotspots to their pieces:** build each control as its own named `Builder` piece and set `b.pieces[piece].hotspot = Some(Hotspot::click/hold/wheel(...))`. The position is an offset from the piece's pivot. This way the target moves with the lever, knob or switch as it animates. Only use `Spec::hotspots` (body-frame, static) for targets that never move, and give those a real moving piece where possible.
- **Accurate placement:** put the sphere's centre on the part the pilot would grab, such as the throttle knob, the key head, the switch toggle or the lever's grip. Do not put it on the pivot or the base. Make the radius roughly the size of that grip, about 1–4 cm. Avoid spheres that overlap a neighbouring control, so that the nearest pick never chooses the wrong one. Keep the visible geometry, pivot, axis, travel and hotspot consistent with one another and with the real cockpit layout.
- **Match the real control's interaction:**
  - `click` is for toggles and detented switches: master, gear, parking brake, canopy, speedbrake, fuel control.
  - `hold` is for spring-loaded or momentary controls: the starter, the key to START, toe brakes, reverse levers.
  - `wheel` is for continuous or notched controls: throttle, mixture, propeller, trim, flaps (one detent per notch), OBS and radio tuning.
  - Several identical controls, such as four thrust levers or two fuel switches, each get their own hotspot.
- **Moving controls show their state:** a clickable control should be an animated piece driven by the simulated state, using `Anim`/`Drive` with an `Input` such as `Throttle`, `GearHandle`, `ParkingBrake`, `Master`, `Ignition` or `Mixture`. Its position should always match the setting. Add an `Input` or a `Plane` field if you need one, rather than leaving the control static.
- **Reachable and not clicked through:** the pilot must be able to aim at every hotspot from `Spec::eye`, using the normal head movement and lean, without a panel, glareshield, yoke, stick or seat in the way. If a cockpit's solid parts are not covered by the name list in `cockpit::blockers`, add them there, so that the mouse cannot click a control through a panel. Looking out ahead at the horizon must not hit any hotspot.
- **Labels and state:** the hover label is the action's `action.<id>` key in `assets/locales/*.lng`, for `bind::ACTIONS` and for the mouse-only actions (`c172::cabin::action_label`) alike. The setting shown under the pointer comes from `hot_state` in `src/main.rs`. For any new action, add both: the `action.<id>` key in `eng.lng` (and `fr.lng`), and its `hot_state`. See `docs/translating.md`.
- **Coverage:** at minimum, provide throttle(s), gear (on retractable-gear types), flaps, parking brake, the engine start controls (master/battery, ignition or fuel control, starter) and the aircraft's other primary controls. It should be possible to do a cold-and-dark start with the mouse alone wherever the simulated systems support one. Document the controls that remain unclickable or decorative.
- **Verification:** `cargo test` runs `hotspot_tests` in `src/aircraft/mod.rs`. These tests check that every action exists, that throttle and gear targets are present, and that every control can be reached and seen from the pilot's eye with gear up and down. In the game, hover over every control from the seat and check the ring, the label and the state. Then try them with `--cursor`, `--click SEC[,HELD[,X,Y]]` and `--wheel SEC,N[,X,Y]`, and confirm that each one moves its piece and changes the simulated state.

## Exterior realism

- Match the real aircraft's silhouette and construction, including intakes, exhausts, doors, access panels, control-surface gaps, gear wells, lights, and antennas.
- Include realistic panel seams, rivet rows, bolts, screws, and other fasteners where the actual aircraft has them. Match their placement and scale; respect differences between metal, composite, wood, and fabric construction. The DR400's wood-and-fabric surfaces, for example, should not receive generic metal rivet patterns.
- Use appropriate paint, glass, rubber, fabric, and metal finishes. Base wear, markings, and liveries on references; identify fictional liveries as fictional.
- Give moving parts correct pivots, axes, travel, and timing. Check the full movement of gear, doors, flaps, control surfaces, propellers, and canopies for interference and clipping.
- Preserve exporter piece names, material identifiers, and animation/damage mappings. Read each Blender script's rebuild instructions before using it; a procedural rebuild can overwrite manual `.blend` edits. Keep source assets and exported assets consistent.

## Liveries

Paint schemes are offered in the hangar's aircraft picker (`src/hangar_ui.rs`). Use `c172::LIVERIES`, `dr400::LIVERIES`, `b747::LIVERIES` and `g650::LIVERIES` as the examples.

- **How it works:** each type lists `aircraft::Livery` entries and is registered in `aircraft::liveries` (and `default_livery` if the default is not index 0). `livery_uniform` packs the four `colours` and the `title` into the shader's `PT` uniform. The type's livery function in `src/shaders/plane.wgsl` (`livery`, `livery_dr400`, `livery747`, ...) paints the design from the body-frame rest position, so the paint stays put on moving parts.
- **Fields:** `colours` are linear RGB, not sRGB. `colours[0][3]` picks the design (the style the shader function draws); `colours[1][3]` is overwritten by `livery_uniform`; the other w's are free. `title` is an airline name in capitals, digits and spaces, at most 15 characters (the stroke-font glyphs). `swatch` is the picker's main colour over its trim. `paint` gives the scheme its own baked `Paint` instead of `Spec::paint`, like the G650's plain white skin.
- **Shader schemes versus baked paint:** Rust-built aircraft draw their schemes procedurally in the shader. Blender aircraft use a baked `Paint` JPEG (top, bottom and side views, laid out from the GUIDE_* constants in the type's Blender script). A scheme on a baked type usually needs a plain bake without markings, with the shader adding the scheme over it. Gate new shader code with `livery_ours()` and `livery_style()` so that other aircraft drawn at the same time (multiplayer, traffic, wreckage) keep their own look.
- **Placement:** follow the real aircraft's paint breaks, which line up with its panel lines, cheatlines, window belt, fin and engine cowls. Keep registrations and titles readable, the right way round on both sides, and clear of doors, windows and control-surface gaps. Respect the material: no metal sheen on the DR400's fabric, and keep paint off glass, rubber, and bare metal parts such as leading edges or exhausts where the real aircraft leaves them bare.
- **Real versus fictional:** do not reproduce real airline or operator branding, logos or trademarks. Airline schemes use fictional carriers (such as Orivante and Kestralis, which can be shared across types). A historical or generic scheme based on references should be described as such in comments, and the comment should say which liveries are fictional.
- **Picker picture:** every scheme needs a `still` (680 x 392 PNG in `assets/aircraft/liveries/`). Add the type's key and the picture names, in the same order as `LIVERIES`, to `assets/aircraft/liveries/stills.sh`, then run `cargo build --release && sh assets/aircraft/liveries/stills.sh <key>`.
- **Verification:** run `cargo check` and `cargo test --test shaders`. Then select each scheme in the picker, or launch with `--aircraft <key> --livery <n>`, and look at it from several exterior angles and in the cockpit view. Check colours in daylight and at dusk, that markings line up across moving surfaces, and that another type flying nearby is unchanged.

## Flight behavior

- Tune each aircraft to its real behavior using documented performance, not just another aircraft's settings. A copied `Spec` is only a starting point.
- Match mass, payload, fuel capacity and consumption, centre-of-gravity range, inertia, power or thrust, drag, lift, and configuration effects to the selected variant.
- Validate stall speeds, takeoff and landing performance, climb, cruise, maximum speed, approach behavior, and handling against references at comparable weight, altitude, atmosphere, power, and flap/gear configuration. Distinguish indicated airspeed, true airspeed, groundspeed, and Mach.
- Reproduce the aircraft's characteristic stability, control response, trim, stall behavior, and engine response within the simulator's capabilities. Keep gear, flap, structural, and engine limits grounded in references.
- Do not change shared physics merely to make one aircraft hit a target if it breaks the others. Check affected existing aircraft whenever shared flight code changes.
- Document simulator limitations and remaining discrepancies. Do not describe an aircraft as fully realistic based only on its appearance or a single speed check.

## Verification

- For aircraft code changes, run `cargo check` and the relevant tests; for shared physics changes, include the flight-model tests. Run `cargo test --test shaders` when changing shaders, plus relevant cockpit shader tests.
- For flight changes, use `cargo run --release -- --fly-check <key>` as an initial performance check. Where suitable reference files are available, use `--jsbsim-compare <file> --aircraft <key>` to compare flight-model data. These checks supplement flight testing and real-world references.
- For visual changes, inspect the aircraft in the simulator or rendered previews from several exterior angles and from the cockpit. Check dimensions, surface detail, materials, instrument behavior, and moving parts.
- Report what was checked, the reference targets and measured results where applicable, and any checks that could not be completed. Documentation-only changes do not require a build or flight test.
