# GTW v1 — Tasks

Source plan: [PLAN.md](PLAN.md)

**Verification commands**

```sh
cargo check --all-targets
cargo test
cargo test --test shaders
cargo build --release --locked
```

Additional phase checks:

```sh
cargo test --test gtw_smoke
cargo run --release -- --place "Buenos Aires" --start foot
cargo run --release -- --place "Buenos Aires" --start car
cargo run --release -- --airport EZE --start plane
cargo run --release -- --headless --flat --start foot --hold 5
cargo run --release -- --headless --flat --start car --hold 5
cargo run --release -- --help
```

## How to use this file

- Work through phases in order unless a phase explicitly allows parallel work.
- Read the linked section of `PLAN.md` before starting a phase.
- Write the tests in the same phase as the code they cover; tests are deliverables.
- A phase is complete only when every checkbox is checked and its verification commands pass.
- Keep the project runnable at every phase boundary.
- Update `PLAN.md` when implementation forces a design change.
- Do not implement ground weapons, pedestrians, police, interiors, swimming, motorcycles, boats, multiplayer ground play, or full-world saves in any phase.

## Phase 1 — Surface identification and world query

**Goal:** Every gameplay position can answer “what surface is here, how high is it, and is it a road, sidewalk, bridge, or water?”
**Depends on:** none
**Plan:** `PLAN.md` § Surface classes, retained tile surface mask, road index, file changes
**Files:** `src/surface.rs` (new), `src/landcover.rs` (new), `src/sources.rs`, `src/world.rs`, `src/roads.rs`, `src/buildings.rs`, `src/main.rs`, `README.md`, `tests/fixtures/landcover_z14.png` (new)

### Implementation

- [x] 1.1 Create `src/surface.rs` with `Surface` (Asphalt, Concrete, Dirt, Grass, Sand, Rock, Water), `SurfaceHit`, `SurfaceWeather`, OSM mapping, WorldCover mapping, and grip/rolling-resistance/foot-speed tables.
- [x] 1.2 Add `surface::merge(osm_class, water, worldcover_class) -> Surface` implementing the precedence water > OSM > WorldCover > Grass.
- [x] 1.3 Create `src/landcover.rs` with the 3° item-id calculation, the Planetary Computer z14 URL, 256×256 `LA` PNG decode, boundary composition for up to four items, and a `Sources`-independent decode entry point.
- [x] 1.4 Add `Sources::landcover(id)` with the cache key `lc/{z}/{x}/{y}.png`, z14 ancestry lookup, and OSM fallback when the fetch fails.
- [x] 1.5 Add `TileData.surface: Vec<u8>` (256×256) and derive it in `sources::build_tile` from the existing OSM mask plus the landcover tile; keep `TileData.mask` GPU-only and drop it after upload.
- [x] 1.6 Add `Tile.surface` and retain it when `World::update` consumes `JobResult::Terrain`; do not retain the RGBA mask.
- [x] 1.7 Add `Path.drive_half`, `Path.walk`, `Path.bridge`, `Path.local`, and `Path.alt` in `src/roads.rs`; populate them in `roads::build`.
- [x] 1.8 Add `RoadIndex` and `RoadHit` in `src/roads.rs`; build a 64×64 tile-unit candidate grid, implement closest-point interpolation, and classify carriageway versus sidewalk.
- [x] 1.9 Add `BuildingData.road_index` and build it after `roads::build` in `src/buildings.rs`.
- [x] 1.10 Add `World::surface_at(p)`, diagnostic `World::surface(lat, lon)`, `World::road_hit(lat, lon)`, and the `WorldQuery` trait methods `surface_at`, `building_push_sphere`, `building_push_box`, `tree_push_sphere`, `tree_push_box` in `src/world.rs`; share the tile-selection loop with `terrain_ground` through a private `terrain_sample` returning both `Ground` and the surface class.
- [x] 1.11 Implement the 2.5 m bridge-height filter and the Grass fallback when no landcover exists.
- [x] 1.12 Add `--surface-probe LAT,LON` to `src/main.rs`; it prints elevation, OSM class, WorldCover class, merged class, and exits.
- [x] 1.13 Add the ESA WorldCover and Planetary Computer attribution to `README.md`.
- [x] 1.14 Add `tests/fixtures/landcover_z14.png` containing known WorldCover codes for decode tests.
- [x] 1.15 Add a debug `RoadIndex` dump helper behind `#[cfg(test)]` so tests can inspect candidate cells without a GPU.

### Tests

- [x] 1.16 Unit-test every `Surface` mapping from OSM `GC_PAVED`, `GC_PITCH`, and `LC_*` values in `src/surface.rs`.
- [x] 1.17 Unit-test every WorldCover code (10, 20, 30, 40, 50, 60, 70, 80, 90, 95, 100) and an unknown code in `src/surface.rs`.
- [x] 1.18 Unit-test the water precedence and all weather grip/foot multipliers in `src/surface.rs`.
- [x] 1.19 Unit-test item ids for north/south/east/west, 3° boundaries, the antimeridian, and a tile crossing two WorldCover items in `src/landcover.rs`.
- [x] 1.20 Unit-test PNG decode against `tests/fixtures/landcover_z14.png`, including empty, truncated, and all-zero inputs.
- [x] 1.21 Unit-test `RoadIndex` on a straight carriageway, a sidewalk band, an off-ribbon point, a bridge deck, a hidden tunnel flag, and a path crossing a tile border in `src/roads.rs`.
- [x] 1.22 Unit-test closest-point distance and altitude interpolation on a synthetic two-segment path in `src/roads.rs`.
- [x] 1.23 Unit-test `surface::merge` for road-over-landcover, water-over-road, and missing-landcover fallback in `src/surface.rs`.
- [x] 1.24 Unit-test `World::surface_at` on `World { flat: Some(0.0) }` returning Grass at 0 m and the diagnostic `World::surface` wrapper, plus `--surface-probe` argument parsing in `src/main.rs`.

### Verification

- [x] Run `cargo check --all-targets`; it exits 0.
- [x] Run `cargo test`; it exits 0 and all new surface, landcover, road, and world tests pass.
- [x] Run `cargo test --test shaders`; it exits 0 (no shader changes expected, but the suite must stay green).
- [x] Run `cargo run --release -- --surface-probe -34.6037,-58.3816`; it prints plausible elevation and classes and exits 0.
- [x] Run `cargo run --release -- --surface-probe 48.8566,2.3522`; it prints a different landcover mixture and exits 0.
- [x] Run `cargo build --release --locked`; it exits 0.
- [x] Confirm no new skipped, quarantined, or failing tests.

**Done when:** A point query returns height, one of six land classes or water, and road/sidewalk/bridge flags with no GPU or streamed world required beyond the loaded tile, and the full suite is green.

## Phase 2 — Player modes, on-foot character, avatar, and input contexts

**Goal:** The player spawns on foot anywhere, walks/runs/sprints/jumps with correct ground behaviour, switches first/third person, takes fall damage, and respawns nearby.
**Depends on:** Phase 1
**Plan:** `PLAN.md` § Architecture and data flow, Interfaces, configuration, and CLI, File changes
**Files:** `src/character.rs` (new), `src/avatar.rs` (new), `src/main.rs`, `src/bind.rs`, `src/padmap.rs`, `src/settings.rs`, `src/settings_ui.rs`, `src/shaders/inst.wgsl`, `src/world.rs`, `src/render.rs`, `src/menu.rs`, `src/mainmenu.rs`, `src/i18n.rs`, `assets/locales/eng.lng`, `assets/locales/fr.lng`, `assets/locales/ko.lng`, `tests/gtw_smoke.rs` (new)

### Implementation

- [x] 2.1 Add `Mode::Walk` beside `Menu` and `Fly`; update every exhaustive `Mode` match in `src/main.rs`.
- [x] 2.2 Add `Game.character: Character`, `Game.plane_visible: bool`, `Game::focus_pos()`, `Game::focus_yaw()`, and `Game::active_context()`.
- [x] 2.3 Add `--start foot|car|plane` and `--flat`; default normal starts to `foot`, `--no-menu` with an aircraft, airport, or altitude flag stays a plane start for existing flight tests and the launcher bench, and `--flat` skips the network start-elevation lookup.
- [x] 2.4 Update `Menu::begin` and `Game::goto` so the normal start and “go to place” place the character on the ground, with a safe-point search if the chosen point is water, a building, or missing a tile.
- [x] 2.5 Create `src/character.rs`: `Character`, `CharacterControls`, capsule radius 0.30 m, height 1.80 m, eye 1.65 m, walk 1.6 m/s, run 4.5 m/s, sprint 7.0 m/s, jump 1.1 m, step-up 0.35 m, slope limit 40°, and the `step` function over `WorldQuery`.
- [x] 2.6 Implement surface speed multipliers, fall damage above 4 m/s impact, health regeneration after 5 s at 2 HP/s, last-safe-point tracking, and nearby respawn on death.
- [x] 2.7 Create `src/avatar.rs`: rigid pelvis, torso, head, upper/lower arms, hands, thighs, shins, and feet; material codes for skin, cloth, hair, and shoes; four clothing palettes; procedural idle/walk/run/jump poses.
- [x] 2.8 Add the avatar mesh base after `birds::BASE` and append the avatar meshes to the instanced mesh library in `Game::new`.
- [x] 2.9 Extend `src/shaders/inst.wgsl` with a character material branch selected by `Inst.ext.w = 2`; keep the existing vehicle and rubble branches unchanged.
- [x] 2.10 Add walking third-person over-the-shoulder and first-person cameras, camera collision against ground and buildings, and a C-key toggle; hide the avatar in first person.
- [x] 2.11 Replace the global flight binding model with `bind::Context { Foot, Car, Flight }`; add foot actions for move, look, sprint, jump, interact, camera, and first-person toggle.
- [x] 2.12 Add context-aware `Bindings::key_of`, `map_key`, and `set_key`; keep global actions available in every context.
- [x] 2.13 Add the foot gamepad layout in `src/padmap.rs` and the controls context tabs in `src/settings_ui.rs`.
- [x] 2.14 Add foot action labels and HUD strings to the three locale files.
- [x] 2.15 Add the health bar to the ground HUD and hide flight instruments when `Mode::Walk`.
- [x] 2.16 Branch `Game::frame`, `controls`, camera, weather, world streaming, live traffic, minimap, birds, and multiplayer sending on `focus_pos()`; do not advance or draw the aircraft when `plane_visible` is false.
- [x] 2.17 Add `Settings::ground_first_person`, `avatar_style`, and `autosave` with defaults; change `Settings::default().multiplayer` to `false` while leaving the multiplayer code and settings reachable.
- [x] 2.18 Add `tests/gtw_smoke.rs` with headless `--flat --start foot --hold` runs; use `CARGO_BIN_EXE_fablegame` until Phase 7 renames the binary.

### Tests

- [x] 2.19 Unit-test character walk, run, and sprint speed on flat ground in `src/character.rs`.
- [x] 2.20 Unit-test surface speed multipliers for all six classes in `src/character.rs`.
- [x] 2.21 Unit-test step-up at 0.34 m and refusal at 0.36 m in `src/character.rs`.
- [x] 2.22 Unit-test slope walking at 39° and sliding at 41° in `src/character.rs`.
- [x] 2.23 Unit-test jump apex, landing, fall damage thresholds, health regeneration, death, and nearby respawn in `src/character.rs`.
- [x] 2.24 Unit-test that `Character::step` produces finite state under large `dt` and at world bounds in `src/character.rs`.
- [x] 2.25 Unit-test avatar part count, palette bounds, and finite matrices for idle/walk/run/jump poses in `src/avatar.rs`.
- [x] 2.26 Unit-test context isolation: the same physical key maps to different actions in Foot, Car, and Flight; global actions remain available in all three in `src/bind.rs`.
- [x] 2.27 Unit-test `focus_pos()` and `plane_visible` selection for Walk, Drive, and Fly in `src/main.rs`.
- [x] 2.28 Unit-test Settings defaults (`multiplayer == false`, ground first-person off, avatar style 0, autosave on), the first-person avatar-hidden state, and health HUD formatting in `src/settings.rs` and `src/main.rs`.
- [x] 2.29 Run the headless smoke test for `--start foot` from `tests/gtw_smoke.rs`.

### Verification

- [x] Run `cargo check --all-targets`; it exits 0.
- [x] Run `cargo test`; it exits 0 and all character, avatar, bind, and focus tests pass.
- [x] Run `cargo test --test gtw_smoke`; it exits 0.
- [x] Run `cargo test --test shaders`; it exits 0 with the modified `inst.wgsl`.
- [ ] Run `cargo run --release -- --place "Buenos Aires" --start foot`; spawn on foot, walk, run, sprint, jump, toggle first person, climb a kerb, and verify deep water blocks.
- [x] Run `cargo run --release -- --headless --flat --start foot --hold 5`; it exits 0.
- [x] Run `cargo build --release --locked`; it exits 0.
- [x] Confirm no new skipped, quarantined, or failing tests.

**Done when:** The player can spawn on foot anywhere, traverse terrain with the specified moveset, take fall damage, die, respawn nearby, and never sees or depends on an aircraft at spawn.

## Phase 3 — Drivable cars, arcade physics, damage, and HUD

**Goal:** The player can take a parked car, drive it with single-body arcade physics, drift, crash, damage it, exit, and re-enter it.
**Depends on:** Phase 2
**Plan:** `PLAN.md` § Approach, Interfaces, configuration, and CLI, File changes
**Files:** `src/vehicle.rs` (new), `src/cars.rs`, `src/main.rs`, `src/world.rs`, `src/roads.rs`, `src/bind.rs`, `src/padmap.rs`, `src/settings_ui.rs`, `src/shaders/inst.wgsl`, `assets/locales/eng.lng`, `assets/locales/fr.lng`, `assets/locales/ko.lng`, `tests/gtw_smoke.rs`

### Implementation

- [x] 3.1 Create `src/vehicle.rs` with `Vehicle`, `VehicleControls`, `VehicleDamage`, `VehicleStep`, and the single-body state.
- [x] 3.2 Implement longitudinal forces: engine torque curve, automatic gearbox, reverse, drag, rolling resistance, brakes, handbrake, and surface grip.
- [x] 3.3 Implement lateral arcade behaviour: steering yaw target, grip circle, slide at the limit, handbrake rear-grip drop, and cosmetic body roll/pitch.
- [x] 3.4 Implement four-point ground sampling, slope gravity, airborne ballistic motion, water stall, and substeps at 1/120 s with a displacement cap.
- [x] 3.5 Add the per-model tuning table for mass, power, drivetrain (FWD/RWD/AWD), gear ratios, top speed, and grip for all eight existing models.
- [x] 3.6 Implement OBB collisions against buildings, trees, and later traffic bodies; convert penetration into impulses and damage.
- [x] 3.7 Implement persistent damage: engine power loss, steering pull, wheel drag, visual crumple amount, smoke at low health, and wreck state at zero.
- [x] 3.8 Refactor `src/cars.rs` into `vehicle_body` and `wheel_mesh`; keep `build_models()` returning the existing traffic meshes unchanged.
- [x] 3.9 Add `player_models()` returning body mesh, wheel mesh, four wheel local positions, wheel radius, length, and width for each of the eight models.
- [x] 3.10 Add wheel spin and steering instances, body roll/pitch, and the player-body damage instance in `Game`.
- [x] 3.11 Extend `inst.wgsl` with player-car deformation selected by a negative `Inst.ext.w` amount; keep traffic and photo-hidden behaviour unchanged.
- [x] 3.12 Add `Mode::Drive`; implement enter-parked-car via a nearby parked car, exit to the safe side of the car, and `Game.last_car` persistence for re-entry.
- [x] 3.13 Add chase and hood/bumper car cameras with mouse look and camera collision.
- [x] 3.14 Add the speed/gear/minimap HUD and hide flight instruments in `Mode::Drive`.
- [x] 3.15 Add car actions for throttle, brake, steer, handbrake, enter/exit, camera, and horn; add the car gamepad layout and controls tab.
- [x] 3.16 Add `World::nearest_parked_car` and `World::take_parked_car` in `src/world.rs`; remove the taken car from its `BTile.parked` list.
- [x] 3.17 Add locale strings for car actions, HUD, damage, and enter/exit prompts.
- [x] 3.18 Extend `tests/gtw_smoke.rs` with `--flat --start car --hold` using a default sedan spawned at the start point.

### Tests

- [x] 3.19 Unit-test acceleration, braking, top speed, reverse, and auto upshift/downshift for the sedan and bus in `src/vehicle.rs`.
- [x] 3.20 Unit-test handbrake yaw and slide on asphalt versus grass in `src/vehicle.rs`.
- [x] 3.21 Unit-test rolling resistance and grip on all six surfaces in `src/vehicle.rs`.
- [x] 3.22 Unit-test slope gravity, airborne state, landing, and water stall in `src/vehicle.rs`.
- [x] 3.23 Unit-test OBB-versus-building separation and impulse damage in `src/vehicle.rs`.
- [x] 3.24 Unit-test damage persistence, power loss, steering pull, and wreck state in `src/vehicle.rs`.
- [x] 3.25 Unit-test finite state under large `dt`, zero speed, reverse, and full-lock steering in `src/vehicle.rs`.
- [x] 3.26 Unit-test `player_models()` returns eight bodies, eight wheel meshes, and four finite wheel positions per model in `src/cars.rs`.
- [x] 3.27 Unit-test nearest parked-car selection and removal in `src/world.rs`.
- [x] 3.28 Unit-test exit placement on both sides of the car and the safe fallback against a building in `src/main.rs` or `src/vehicle.rs`.
- [x] 3.29 Unit-test the driving HUD formatting for speed, gear, and minimap visibility, and chase/hood camera selection in `src/main.rs`.
- [x] 3.30 Run the headless smoke test for `--start car` from `tests/gtw_smoke.rs`.

### Verification

- [x] Run `cargo check --all-targets`; it exits 0.
- [x] Run `cargo test`; it exits 0 and all vehicle, model, and parked-car tests pass.
- [x] Run `cargo test --test gtw_smoke`; it exits 0.
- [x] Run `cargo test --test shaders`; it exits 0 with the modified `inst.wgsl`.
- [ ] Run `cargo run --release -- --place "Buenos Aires" --start foot`; walk to a parked car, enter it, drive, drift with the handbrake, hit a building, exit, and re-enter.
- [ ] Run `cargo run --release -- --place "Buenos Aires" --start car`; confirm the default sedan spawns and the HUD shows speed, gear, and minimap.
- [x] Run `cargo run --release -- --headless --flat --start car --hold 5`; it exits 0.
- [x] Run `cargo build --release --locked`; it exits 0.
- [x] Confirm no new skipped, quarantined, or failing tests.

**Done when:** A player can take any parked car, drive it with believable single-body arcade handling, see speed and gear, damage it persistently, exit safely, and re-enter it.

## Phase 4 — Solid traffic, carjacking, and ground collisions

**Goal:** Traffic brakes for the player, collides, yields when taken, and the player can steal a moving car without the simulation losing it.
**Depends on:** Phase 3
**Plan:** `PLAN.md` § Architecture and data flow, Failure modes and edge cases, File changes
**Files:** `src/world.rs`, `src/cars.rs`, `src/vehicle.rs`, `src/character.rs`, `src/main.rs`, `src/roads.rs`, `src/bind.rs`, `assets/locales/eng.lng`, `assets/locales/fr.lng`, `assets/locales/ko.lng`, `tests/gtw_smoke.rs`

### Implementation

- [x] 4.1 Add `World::traffic_bodies_near(pos, radius, out)` returning oriented boxes for moving and parked cars.
- [x] 4.2 Add `World::take_traffic_car(pos, radius)` and `World::take_parked_car(pos, radius)` returning a `TakenCar { model, colour, pos, yaw, velocity }` and removing the source car.
- [x] 4.3 Add a `player_presence` parameter to `World::update_traffic` carrying mode and position; traffic within 12 m of a walking player or 18 m of a driving player must brake smoothly.
- [x] 4.4 Add a per-car yield state so a traffic car that is about to be taken stops and holds for a short time before removal.
- [x] 4.5 Add OBB-versus-OBB collision between the player vehicle and traffic bodies; apply impulse and damage to both the player car and the traffic car state.
- [x] 4.6 Add circle-versus-OBB collision between the character and traffic; apply knockback, health damage, and a traffic brake event.
- [x] 4.7 Add the interaction prompt and E-key behaviour for parked and moving cars; refuse the interaction if the source tile is not ready.
- [x] 4.8 Add a “traffic hit” event for audio, damage HUD, and screen feedback; wire the event queue without adding audio synthesis yet.
- [x] 4.9 Keep traffic path-following and lane logic unchanged outside the new yield/collision hooks.
- [x] 4.10 Extend `tests/gtw_smoke.rs` with a headless car run that takes a traffic car and exits.

### Tests

- [x] 4.11 Unit-test traffic braking distance and slowdown near a walking and a driving player in `src/world.rs`.
- [x] 4.12 Unit-test `take_traffic_car` removes exactly one car from the correct lane and returns its model, colour, pose, and velocity.
- [x] 4.13 Unit-test `take_parked_car` removes exactly one parked prop and leaves neighbouring props intact.
- [x] 4.14 Unit-test refusal when the target tile is not ready and when no car is in range.
- [x] 4.15 Unit-test OBB-versus-OBB collision damage and separation for two vehicles.
- [x] 4.16 Unit-test character-versus-traffic knockback and health damage.
- [x] 4.17 Unit-test that traffic resumes normal speed after the player leaves the yield radius.
- [x] 4.18 Run the headless traffic-carpack smoke test from `tests/gtw_smoke.rs`.

### Verification

- [x] Run `cargo check --all-targets`; it exits 0.
- [x] Run `cargo test`; it exits 0 and all traffic, carjacking, and collision tests pass.
- [x] Run `cargo test --test gtw_smoke`; it exits 0.
- [x] Run `cargo test --test shaders`; it exits 0.
- [ ] Run `cargo run --release -- --place "Buenos Aires" --start foot`; walk into traffic, verify it brakes, then take a moving car.
- [ ] Run `cargo run --release -- --place "Buenos Aires" --start car`; collide with traffic and a building, verify damage and health feedback.
- [x] Run `cargo build --release --locked`; it exits 0.
- [x] Confirm no new skipped, quarantined, or failing tests.

**Done when:** Traffic is solid and yielding, a moving car can be taken without duplication or loss, and collisions damage the player and vehicles consistently.

## Phase 5 — Aircraft access, save, and resume

**Goal:** The player can enter a parked airport aircraft or use the airport menu fallback, fly, land, exit, quit, and resume in the same mode and place.
**Depends on:** Phases 2, 3, 4
**Plan:** `PLAN.md` § Save schema, Failure modes and edge cases, File changes
**Files:** `src/traffic.rs`, `src/main.rs`, `src/menu.rs`, `src/save.rs` (new), `src/settings.rs`, `src/world.rs`, `tests/gtw_smoke.rs`

### Implementation

- [x] 5.1 Add `Traffic::take_parked(stand)` and expose the stand pose needed to spawn a player aircraft.
- [x] 5.2 Map traffic type 0 (light single) to `c172` and type 1 (business jet) to `g650`; larger types remain scenery.
- [x] 5.3 Add airport detection within 5 km and a parked-aircraft interaction on foot; enter the aircraft with `Plane::spawn_ground`, set `plane_visible = true`, and switch to `Mode::Fly`.
- [x] 5.4 Add the airport menu fallback: choose a built-in aircraft and start on the active runway using the existing `start_airport` path.
- [x] 5.5 Add exit-aircraft interaction on the ground at low speed; place the character beside the plane and keep the plane visible for re-entry.
- [x] 5.6 Refuse exit while airborne or above 2 m/s ground speed with a clear status message.
- [x] 5.7 Create `src/save.rs` with `SaveState`, `SaveMode`, `SavedFoot`, `SavedCar`, `SavedPlane`, `SavedClock` (UTC, day, and snow/weather settings), version 1, serde JSON, atomic write, corrupt/unsupported fallback, and the `<config>/gtw/save.json` path.
- [x] 5.8 Add `Game::update_autosave` every 5 s and on quit; save the active mode plus `last_car` and a visible parked plane.
- [x] 5.9 Add load-on-start: if a valid save exists, show Continue on the main menu and resume the saved mode; New Game starts at the chosen place on foot.
- [x] 5.10 Add `--no-save` to disable load and autosave for automated runs.
- [x] 5.11 Add safe-position validation for saved foot, car, and plane positions before applying them.
- [x] 5.12 Add `--start plane` with `--airport` for scripted flight starts and update the launcher benchmark arguments to use it.
- [x] 5.13 Extend `tests/gtw_smoke.rs` with `--start plane --airport EZE` and `--no-save` smoke coverage that does not require a full flight.

### Tests

- [x] 5.14 Unit-test `Traffic::take_parked` removes the selected stand and returns the expected type/livery, and that a second take fails.
- [x] 5.15 Unit-test the light-single/business-jet mapping and the menu fallback for larger types.
- [x] 5.16 Unit-test save round-trip for Foot, Drive, and Fly with position, yaw, health, damage, fuel, and clock.
- [x] 5.17 Unit-test corrupt JSON, future version, missing file, and empty file fallbacks.
- [x] 5.18 Unit-test atomic write leaves either the old valid save or the new valid save, never a partial file.
- [x] 5.19 Unit-test safe-position search rejects water, building interiors, and missing tiles and finds a nearby valid point.
- [x] 5.20 Unit-test exit-aircraft refusal while airborne and acceptance on the ground at low speed.
- [x] 5.21 Unit-test `--start plane` argument parsing and `--no-save` behaviour.
- [x] 5.22 Run the headless plane-start smoke test from `tests/gtw_smoke.rs`.

### Verification

- [x] Run `cargo check --all-targets`; it exits 0.
- [x] Run `cargo test`; it exits 0 and all aircraft-access and save tests pass.
- [x] Run `cargo test --test gtw_smoke`; it exits 0.
- [x] Run `cargo test --test shaders`; it exits 0.
- [ ] Run `cargo run --release -- --place "Buenos Aires" --start foot`; drive to an airport, enter a parked aircraft, fly, land, exit, and re-enter.
- [ ] Run `cargo run --release -- --airport EZE --start plane`; fly the airport-start fallback.
- [ ] Quit and relaunch in Foot, Drive, and Fly modes; confirm Continue resumes the same mode and place.
- [x] Run `cargo build --release --locked`; it exits 0.
- [x] Confirm no new skipped, quarantined, or failing tests.

**Done when:** The full spawn → walk → car → airport → plane → land → exit → quit → resume loop works, with a valid save for each mode.

## Phase 6 — Positional audio and ground polish

**Goal:** Driving and walking sound positional and surface-aware, and the ground HUD communicates health, speed, gear, damage, and interactions cleanly.
**Depends on:** Phases 2, 3, 4
**Plan:** `PLAN.md` § Approach, Failure modes and edge cases, File changes
**Files:** `src/audio.rs`, `src/main.rs`, `src/vehicle.rs`, `src/character.rs`, `src/shaders/inst.wgsl`, `assets/locales/eng.lng`, `assets/locales/fr.lng`, `assets/locales/ko.lng`

### Implementation

- [x] 6.1 Add `audio::Emitter`, `EmitterKind`, `WorldAudio`, and `Audio::set_world` with camera-relative gain and pan.
- [x] 6.2 Add a player-car voice with engine RPM/load, tyre rolling, braking, handbrake, and crash layers.
- [x] 6.3 Add nearby-traffic voices using the nearest six cars, with distance gain, pan, and speed-based pitch.
- [x] 6.4 Add footstep events from `Character` stride phase, with surface-dependent timbre and volume.
- [x] 6.5 Add crash, scrape, splash, and horn events to the existing event queue.
- [x] 6.6 Mix the aircraft, player car, traffic, and footsteps in `Audio` without disturbing the existing aircraft synthesis when flying.
- [x] 6.7 Add damage smoke, low-health engine sound, headlights at night, and brake lights for the player car.
- [x] 6.8 Add interaction prompts for cars, aircraft, and the airport menu; style them consistently with the existing HUD.
- [x] 6.9 Add the health bar and damage feedback to the ground HUD; keep the minimap visible in Walk and Drive.
- [x] 6.10 Add locale strings for all new prompts and HUD labels.

### Tests

- [x] 6.11 Unit-test emitter gain and pan for left, right, ahead, behind, and zero-distance cases in `src/audio.rs`.
- [x] 6.12 Unit-test nearest-emitter selection and the six-voice cap.
- [x] 6.13 Unit-test footstep event timing and surface-class timbre selection in `src/character.rs`.
- [x] 6.14 Unit-test that flight audio parameters are unchanged when `Mode::Fly` and muted when `Mode::Menu`.
- [x] 6.15 Unit-test HUD state formatting for speed, gear, health, and damage in `src/main.rs` or a pure helper.
- [x] 6.16 Run the headless audio-off smoke run with `--mute` to confirm no device dependency.

### Verification

- [x] Run `cargo check --all-targets`; it exits 0.
- [x] Run `cargo test`; it exits 0 and all audio and HUD tests pass.
- [x] Run `cargo test --test shaders`; it exits 0.
- [ ] Run `cargo run --release -- --place "Buenos Aires" --start car`; listen with headphones: engine, tyres by surface, nearby traffic, crashes, and horn.
- [ ] Run `cargo run --release -- --place "Buenos Aires" --start foot`; listen to footsteps on asphalt, grass, gravel, sand, and snow.
- [x] Run `cargo run --release -- --mute --headless --flat --start car --hold 5`; it exits 0.
- [x] Run `cargo build --release --locked`; it exits 0.
- [x] Confirm no new skipped, quarantined, or failing tests.

**Done when:** The ground experience has positional engine, traffic, tyre, crash, and footstep audio, and the HUD shows health, speed, gear, damage, and interactions without leaking flight-only UI.

## Phase 7 — GTW rebrand, config migration, and documentation

**Goal:** The player-facing product, binaries, config paths, docs, and credits are GTW, with safe migration from FableFlight and no upstream overwrite.
**Depends on:** Phase 6
**Plan:** `PLAN.md` § Interfaces, configuration, and CLI, Failure modes and edge cases, File changes
**Files:** `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/paths.rs` (new), `src/settings.rs`, `src/bind.rs`, `src/graphics_core.rs`, `src/gmaps.rs`, `src/i18n.rs`, `src/mcp/rpc.rs`, `src/copilot.rs`, `src/fetch.rs`, `src/bin/fableflight-mcp.rs` (rename), `src/bin/fableflight-launcher/` (rename), `tests/mcp.rs`, `tests/gtw_smoke.rs`, `README.md`, `AGENTS.md`, `docs/`, `assets/locales/eng.lng`, `assets/locales/fr.lng`, `assets/locales/ko.lng`, `assets/aircraft/liveries/stills.sh`

### Implementation

- [x] 7.1 Create `src/paths.rs` with `<config>/gtw/`, `<cache>/gtw/`, and a one-time migration from `<config>/fablegame/` for `settings.json`, `bindings.json`, `graphics.json`, `google_maps.json`, and `launcher.json`; switch `settings.rs`, `bind.rs`, `graphics_core.rs`, `gmaps.rs`, `i18n.rs`, `mcp/rpc.rs`, `copilot.rs`, `autopilot.rs`, and the launcher config/run/update paths to the GTW roots.
- [x] 7.2 Add cache migration: rename `<cache>/fablegame` to `<cache>/gtw` when possible; otherwise read the old cache for the session and log a warning.
- [x] 7.3 Change `Cargo.toml` to package `gtw`, default-run `gtw`; keep edition, rust-version, license, and all dependencies.
- [x] 7.4 Rename `src/bin/fableflight-mcp.rs` to `src/bin/gtw-mcp.rs` and update `use`, help text, and MCP server name.
- [x] 7.5 Rename `src/bin/fableflight-launcher/` to `src/bin/gtw-launcher/` and update window titles, app id, binary names, paths, and update source.
- [x] 7.6 Update `src/lib.rs`, `src/bin/gtw-mcp.rs`, `tests/mcp.rs`, and `tests/gtw_smoke.rs` to the `gtw` crate and `CARGO_BIN_EXE_*` names.
- [x] 7.7 Update `src/main.rs` window title, CLI usage, and all user-facing “Fable Flight” strings to GTW.
- [x] 7.8 Update `src/fetch.rs` user agent to `gtw` and the GTW repository URL.
- [x] 7.9 Update `src/i18n.rs` locale search paths and locale strings, including the hangar “in Fable Flight” label, without renaming translation keys.
- [x] 7.10 Disable automatic upstream updates in the launcher; point the repository constant at `galatolofederico/gtw` and make the updater fail gracefully or hide the install action when no GTW release exists.
- [x] 7.11 Update `README.md` for GTW: identity, controls, build/run names, config/cache paths, features, non-goals, and the ESA WorldCover/Planetary Computer attribution.
- [x] 7.12 Update `AGENTS.md` for GTW scope and ground-play conventions, keeping the existing aircraft rules for flight work.
- [x] 7.13 Add a startup migration notice when legacy settings are copied, update the GTW references in `docs/`, and point `assets/aircraft/liveries/stills.sh` at the renamed `gtw` binary.

### Tests

- [x] 7.14 Unit-test GTW path resolution and the one-time config migration for each file, including the “both exist” and “neither exists” cases in `src/paths.rs`.
- [x] 7.15 Unit-test cache migration fallback when the rename fails.
- [x] 7.16 Unit-test binding migration keeps old flight customizations and adds the new contexts without duplicate keys.
- [x] 7.17 Update `tests/mcp.rs` for the renamed binary and server name; it passes.
- [x] 7.18 Update `tests/gtw_smoke.rs` for `CARGO_BIN_EXE_gtw`; it passes.
- [x] 7.19 Add a test that CLI usage and `--version` mention GTW, not FableFlight.
- [x] 7.20 Run a text audit for remaining user-facing `Fable Flight`, `FableFlight`, `fableflight-launcher`, and `fablegame` strings and either update them or document intentional internal references.
- [x] 7.21 Unit-test the launcher default (automatic updates disabled, GTW repository constant, GTW binary names) and the graceful behaviour when no GTW release exists.

### Verification

- [x] Run `cargo check --all-targets`; it exits 0 with the renamed package and binaries.
- [x] Run `cargo test`; it exits 0 including migration and renamed-binary tests.
- [x] Run `cargo test --test mcp`; it exits 0.
- [x] Run `cargo test --test gtw_smoke`; it exits 0.
- [x] Run `cargo test --test shaders`; it exits 0.
- [x] Run `cargo build --release --locked`; it produces `gtw`, `gtw-mcp`, and `gtw-launcher`.
- [x] Run `cargo run --release -- --help`; it prints GTW usage.
- [ ] Launch `gtw-launcher`; confirm the title, pages, and disabled/GTW update behaviour.
- [x] Start GTW with an existing FableFlight config; confirm settings and bindings migrate and the game runs.
- [x] Confirm `README.md` and in-game credits preserve MIT, upstream FableFlight credit, and ESA WorldCover attribution.
- [x] Confirm no new skipped, quarantined, or failing tests.

**Done when:** GTW is the visible product and binary identity, old settings migrate once, the launcher cannot overwrite the fork with upstream FableFlight, and all targets build and test.

## Phase 8 — Full verification and acceptance

**Goal:** Prove the v1 acceptance criteria on real data, including the full gameplay loop, surfaces, save/resume, visuals, and 60 fps target.
**Depends on:** Phase 7
**Plan:** `PLAN.md` § Testing strategy, acceptance criteria
**Files:** `PLAN.md`, `TASKS.md`, no application changes unless a defect is found

### Implementation

- [ ] 8.1 Run the complete automated suite from a clean target directory.
- [ ] 8.2 Play the full loop in a dense real city and in a rural area.
- [ ] 8.3 Verify all six surface classes and weather modifiers on foot and in a car.
- [ ] 8.4 Verify water blocking, bridge decks, tunnels-at-grade limitation, building collisions, and tree collisions.
- [ ] 8.5 Verify parked and moving car theft, traffic yielding, collisions, damage, health, death, and nearby respawn.
- [ ] 8.6 Verify parked aircraft entry, airport menu fallback, flight, landing, exit, and re-entry.
- [ ] 8.7 Verify Continue/resume in Foot, Drive, and Fly after a full quit.
- [ ] 8.8 Verify positional audio and the ground HUD with screenshots and headphones.
- [ ] 8.9 Verify the non-goals: no pedestrians, police, interiors, swimming, motorcycles, boats, ground weapons, or multiplayer ground play; arcade flight weapons remain off by default.
- [ ] 8.10 Verify the rebrand, MIT/upstream credit, ESA WorldCover attribution, and disabled upstream updater.
- [ ] 8.11 Measure 60 fps at 1080p on the mid-range target with `--bench 30`, `--fps-log`, and `FABLE_GPU_TIMES=1`; record the measured numbers.
- [ ] 8.12 Confirm the flight benchmark and a short flight do not regress.
- [ ] 8.13 Record any limitation found during acceptance and either fix it or update `PLAN.md` § Assumptions and open questions.

### Tests

- [ ] 8.14 Run `cargo check --all-targets`; it exits 0.
- [ ] 8.15 Run `cargo test`; it exits 0 with zero failures and zero new ignored tests.
- [ ] 8.16 Run `cargo test --test shaders`; it exits 0.
- [ ] 8.17 Run `cargo test --test gtw_smoke`; it exits 0.
- [ ] 8.18 Run `cargo test --test mcp`; it exits 0.
- [ ] 8.19 Run `cargo build --release --locked`; it exits 0.
- [ ] 8.20 Run the acceptance commands for Foot, Car, and Plane; each exits 0 or reaches a playable state.
- [ ] 8.21 Run `cargo run --release -- --headless --flat --start foot --hold 5`; it exits 0.
- [ ] 8.22 Run `cargo run --release -- --headless --flat --start car --hold 5`; it exits 0.
- [ ] 8.23 Capture the required screenshots: foot third person, foot first person, car chase, car hood, damage, airport aircraft entry, HUD, and night driving.
- [ ] 8.24 Capture a short recording of the full loop with `--record`.

### Verification

- [ ] Run `cargo check --all-targets`; it exits 0.
- [ ] Run `cargo test`; it exits 0.
- [ ] Run `cargo test --test shaders`; it exits 0.
- [ ] Run `cargo test --test gtw_smoke`; it exits 0.
- [ ] Run `cargo test --test mcp`; it exits 0.
- [ ] Run `cargo build --release --locked`; it exits 0.
- [ ] Walk the acceptance checklist in `PLAN.md` § Acceptance criteria item by item and record the result.
- [ ] Confirm the 60 fps measurement and the flight regression result in the final report.
- [ ] Confirm every task in every phase is checked.

**Done when:** Every acceptance criterion in `PLAN.md` is demonstrated, all verification commands are green, the full loop is playable, and the measured performance meets the target or a documented exception is approved.
